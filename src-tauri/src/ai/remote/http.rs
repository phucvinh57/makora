//! JSON over HTTP for the provider clients: one agent with a timeout, retries with backoff on
//! rate limits and server errors, and failures that carry the provider's own message.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tracing::warn;
use ureq::http::Response;
use ureq::{Agent, Body};

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
        self.with_retries(url, || {
            let mut request = self.agent.get(url);
            for (name, value) in headers {
                request = request.header(*name, value);
            }
            request.call()
        })
    }

    pub fn post_json(&self, url: &str, headers: &[(&str, String)], body: &Value) -> Result<Value> {
        let body = serde_json::to_string(body).context("serializing request")?;
        self.with_retries(url, || {
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
        send: impl Fn() -> Result<Response<Body>, ureq::Error>,
    ) -> Result<Value> {
        let mut attempt = 1;
        loop {
            let (error, wait) = match send() {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .map(Duration::from_secs);
                    let text = response
                        .body_mut()
                        .read_to_string()
                        .with_context(|| format!("reading the response from {url}"))?;
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
            warn!(%url, attempt, wait_s = wait.as_secs(), error = %error, "provider request failed; retrying");
            std::thread::sleep(wait);
            attempt += 1;
        }
    }
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
