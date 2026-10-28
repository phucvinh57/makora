//! The OpenAI chat-completions API, which OpenAI and most other providers and local servers speak.
//!
//! The reply is held to the category list by a strict JSON schema, and the log-probabilities of its
//! tokens give a confidence. Not every provider or model takes all of that, so a field a provider
//! refuses is dropped for the rest of the run and the request is sent again without it.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tracing::info;

use super::http::{ApiError, Http};
use super::{trimmed, ChatApi, Pick};

const PLAIN_REPLY: &str = "\n\nReply with only the category, exactly as written in the list, and nothing else.";

pub struct OpenAi {
    http: Http,
    base_url: String,
    key: Option<String>,
    schema: AtomicBool,
    logprobs: AtomicBool,
    temperature: AtomicBool,
}

impl OpenAi {
    pub fn new(base_url: &str, key: Option<String>) -> Self {
        Self {
            http: Http::new(),
            base_url: trimmed(base_url),
            key: key.filter(|key| !key.trim().is_empty()),
            schema: AtomicBool::new(true),
            logprobs: AtomicBool::new(true),
            temperature: AtomicBool::new(true),
        }
    }

    fn headers(&self) -> Vec<(&'static str, String)> {
        self.key
            .iter()
            .map(|key| ("Authorization", format!("Bearer {}", key.trim())))
            .collect()
    }

    fn body(&self, model: &str, system: &str, user: &str, choices: &[String]) -> Value {
        let schema = self.schema.load(Ordering::Acquire);
        let system = if schema {
            system.to_string()
        } else {
            format!("{system}{PLAIN_REPLY}")
        };
        let mut body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        if self.temperature.load(Ordering::Acquire) {
            body["temperature"] = json!(0);
        }
        if self.logprobs.load(Ordering::Acquire) {
            body["logprobs"] = json!(true);
        }
        if schema {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "category",
                    "strict": true,
                    "schema": {
                        "type": "object",
                        "properties": { "category": { "type": "string", "enum": choices } },
                        "required": ["category"],
                        "additionalProperties": false,
                    },
                },
            });
        }
        body
    }

    /// Turns off the field `err` says the provider refuses. `false` when it names none still on.
    fn drop_refused(&self, err: &ApiError) -> bool {
        let features = [
            (&self.schema, &["response_format", "json_schema"][..], "JSON schema"),
            (&self.logprobs, &["logprobs"][..], "log-probabilities"),
            (&self.temperature, &["temperature"][..], "temperature"),
        ];
        for (flag, fields, label) in features {
            if flag.load(Ordering::Acquire) && err.rejects(fields) {
                flag.store(false, Ordering::Release);
                info!(feature = label, "provider does not take this request field; sending without it");
                return true;
            }
        }
        false
    }
}

impl ChatApi for OpenAi {
    fn pick(&self, model: &str, system: &str, user: &str, choices: &[String]) -> Result<Pick> {
        let url = format!("{}/chat/completions", self.base_url);
        loop {
            let schema = self.schema.load(Ordering::Acquire);
            let body = self.body(model, system, user, choices);
            let reply = match self.http.post_json(&url, &self.headers(), &body) {
                Ok(reply) => reply,
                Err(err) => match err.downcast_ref::<ApiError>() {
                    Some(api) if self.drop_refused(api) => continue,
                    _ => return Err(err),
                },
            };
            let choice = reply
                .pointer("/choices/0")
                .ok_or_else(|| anyhow!("the provider's reply has no choices"))?;
            let content = choice
                .pointer("/message/content")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("the provider's reply has no message"))?;
            // A provider may accept the schema and still not follow it.
            let answer = schema
                .then(|| serde_json::from_str::<Value>(content).ok())
                .flatten()
                .and_then(|value| value.get("category").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| content.to_string());
            return Ok(Pick {
                answer,
                confidence: confidence(choice),
            });
        }
    }

    fn list_models(&self) -> Result<Vec<String>> {
        let reply = self.http.get_json(&format!("{}/models", self.base_url), &self.headers())?;
        model_ids(&reply)
    }
}

/// The probability of the whole reply, from its tokens' log-probabilities. Under a strict schema
/// the JSON around the answer is all but certain, so this is the probability of the answer.
fn confidence(choice: &Value) -> Option<f64> {
    let tokens = choice.pointer("/logprobs/content")?.as_array()?;
    if tokens.is_empty() {
        return None;
    }
    let sum: f64 = tokens
        .iter()
        .map(|token| token.get("logprob").and_then(Value::as_f64))
        .sum::<Option<f64>>()?;
    Some(sum.exp())
}

/// The chat models the provider lists; embedding models are left out.
pub(super) fn model_ids(reply: &Value) -> Result<Vec<String>> {
    let models = reply
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("the provider did not list its models"))?;
    let mut ids: Vec<String> = models
        .iter()
        .filter_map(|model| model.get("id").and_then(Value::as_str).map(str::to_string))
        .filter(|id| !id.to_lowercase().contains("embed"))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}
