//! JSON over HTTP for the provider clients: one agent with a timeout, retries with backoff on
//! rate limits and server errors, and failures that carry the provider's own message.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tracing::{info, warn};
use ureq::http::Response;
use ureq::{Agent, Body};

use crate::logs::NETWORK_TARGET;
use crate::shared::text::clip;

const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ATTEMPTS: u32 = 4;
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const MAX_ERROR_CHARS: usize = 300;

/// A provider answered with an error status. `message` is what it said, when it said anything.
#[derive(Debug, thiserror::Error)]
#[error("the provider answered {status}: {message}")]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl ApiError {
    /// Whether this is the provider refusing a request field, named by any of `fields`.
    pub fn rejects(&self, fields: &[&str]) -> bool {
        let message = self.message.to_ascii_lowercase();
        matches!(self.status, 400 | 422) && fields.iter().any(|field| message.contains(field))
    }
}

pub struct Http {
    agent: Agent,
}

impl Http {
    pub fn new() -> Self {
        let config = Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: Agent::new_with_config(config),
        }
    }

    pub fn get_json(&self, url: &str, headers: &[(&str, String)]) -> Result<Value> {
        let request = RequestLog::new("GET", url, 0, "list models");
        self.with_retries(url, &request, || {
            let mut request = self.agent.get(url);
            for (name, value) in headers {
                request = request.header(*name, value);
            }
            request.call()
        })
    }

    pub fn post_json(&self, url: &str, headers: &[(&str, String)], body: &Value) -> Result<Value> {
        let body = serde_json::to_string(body).context("serializing request")?;
        let request = RequestLog::new("POST", url, body.len(), "AI categorization");
        self.with_retries(url, &request, || {
            let mut request = self.agent.post(url).content_type("application/json");
            for (name, value) in headers {
                request = request.header(*name, value);
            }
            request.send(body.as_str())
        })
    }

    fn with_retries(
        &self,
        url: &str,
        log: &RequestLog,
        send: impl Fn() -> Result<Response<Body>, ureq::Error>,
    ) -> Result<Value> {
        let mut attempt = 1;
        loop {
            let started = Instant::now();
            let sent = send();
            if let Err(err) = &sent {
                log.failed(attempt, started, err);
            }
            let (error, wait) = match sent {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .map(Duration::from_secs);
                    let text = response.body_mut().read_to_string();
                    log.answered(attempt, started, status, text.as_ref().map_or(0, String::len));
                    let text = text.with_context(|| format!("reading the response from {url}"))?;
                    if (200..300).contains(&status) {
                        return serde_json::from_str(&text)
                            .with_context(|| format!("{url} did not answer with JSON"));
                    }
                    let error = ApiError {
                        status,
                        message: error_message(&text),
                    };
                    if !(status == 408 || status == 429 || status >= 500) {
                        return Err(error.into());
                    }
                    (anyhow::Error::from(error), retry_after)
                }
                Err(err @ (ureq::Error::Timeout(_) | ureq::Error::Io(_) | ureq::Error::ConnectionFailed)) => {
                    (anyhow!("requesting {url}: {err}"), None)
                }
                Err(err) => return Err(anyhow!("requesting {url}: {err}")),
            };
            if attempt >= MAX_ATTEMPTS {
                return Err(error);
            }
            let wait = wait.unwrap_or(Duration::from_secs(1 << (attempt - 1))).min(MAX_BACKOFF);
            warn!(host = log.host.as_str(), path = log.path.as_str(), attempt, wait_s = wait.as_secs(), error = %error, "provider request failed; retrying");
            std::thread::sleep(wait);
            attempt += 1;
        }
    }
}

/// One provider request as the network log shows it: never the query string, headers or body.
struct RequestLog {
    method: &'static str,
    host: String,
    path: String,
    bytes_sent: usize,
    purpose: &'static str,
}

impl RequestLog {
    fn new(method: &'static str, url: &str, bytes_sent: usize, purpose: &'static str) -> Self {
        let parsed = url::Url::parse(url).ok();
        Self {
            method,
            host: parsed
                .as_ref()
                .and_then(|url| url.host_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".into()),
            path: parsed.as_ref().map_or_else(String::new, |url| url.path().to_string()),
            bytes_sent,
            purpose,
        }
    }

    fn answered(&self, attempt: u32, started: Instant, status: u16, bytes_received: usize) {
        info!(
            target: NETWORK_TARGET,
            method = self.method,
            host = self.host.as_str(),
            path = self.path.as_str(),
            status,
            bytes_sent = self.bytes_sent,
            bytes_received,
            duration_ms = elapsed_ms(started),
            attempt,
            purpose = self.purpose,
            "HTTP request"
        );
    }

    /// The body may have gone out before the failure, so it counts as sent.
    fn failed(&self, attempt: u32, started: Instant, error: &ureq::Error) {
        warn!(
            target: NETWORK_TARGET,
            method = self.method,
            host = self.host.as_str(),
            path = self.path.as_str(),
            bytes_sent = self.bytes_sent,
            duration_ms = elapsed_ms(started),
            attempt,
            purpose = self.purpose,
            error = %error,
            "HTTP request failed"
        );
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// `error.message` from an OpenAI- or Anthropic-style error body, or the start of the body.
fn error_message(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed.as_ref().and_then(|value| {
        value
            .pointer("/error/message")
            .or_else(|| value.get("error"))
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
    });
    match message {
        Some(message) => clip(message, MAX_ERROR_CHARS),
        None if body.trim().is_empty() => "no details".to_string(),
        None => clip(body.trim(), MAX_ERROR_CHARS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs::layer::tests::capture;

    #[test]
    fn the_network_log_leaves_out_the_query_string() {
        let request = RequestLog::new("GET", "https://api.anthropic.com/v1/models?limit=1000&key=secret", 0, "list models");
        let text = capture(|| request.answered(1, Instant::now(), 200, 42));

        assert!(!text.contains("secret") && !text.contains("limit"));
        let entry: crate::logs::LogEntry = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(entry.target, crate::logs::NETWORK_TARGET);
        assert_eq!(entry.fields["host"], "api.anthropic.com");
        assert_eq!(entry.fields["path"], "/v1/models");
        assert_eq!(entry.fields["bytes_received"], 42);
    }
}
