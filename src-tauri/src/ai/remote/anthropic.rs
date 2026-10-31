//! Anthropic's messages API. The model is made to call a tool whose one argument is limited to the
//! category list, and the taxonomy prompt is marked for caching since every request repeats it.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tracing::info;

use super::http::{ApiError, Http};
use super::openai::model_ids;
use crate::categorization::ports::TokenUsage;

use super::{count, trimmed, ChatApi, Pick};

const VERSION: &str = "2023-06-01";
const TOOL: &str = "pick_category";
const MAX_TOKENS: u32 = 256;

pub struct Anthropic {
    http: Http,
    /// Up to, not including, `/v1`, whether or not the person typed it.
    base_url: String,
    key: Option<String>,
    temperature: AtomicBool,
}

impl Anthropic {
    pub fn new(base_url: &str, key: Option<String>) -> Self {
        let base_url = trimmed(base_url);
        let base_url = base_url.strip_suffix("/v1").map(str::to_string).unwrap_or(base_url);
        Self {
            http: Http::new(),
            base_url,
            key: key.filter(|key| !key.trim().is_empty()),
            temperature: AtomicBool::new(true),
        }
    }

    fn headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![("anthropic-version", VERSION.to_string())];
        if let Some(key) = &self.key {
            headers.push(("x-api-key", key.trim().to_string()));
        }
        headers
    }
}

impl ChatApi for Anthropic {
    fn pick(&self, model: &str, system: &str, user: &str, choices: &[String]) -> Result<Pick> {
        let url = format!("{}/v1/messages", self.base_url);
        loop {
            let mut body = json!({
                "model": model,
                "max_tokens": MAX_TOKENS,
                "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }],
                "messages": [{ "role": "user", "content": user }],
                "tools": [{
                    "name": TOOL,
                    "description": "Record the category of the activity.",
                    "input_schema": {
                        "type": "object",
                        "properties": { "category": { "type": "string", "enum": choices } },
                        "required": ["category"],
                    },
                }],
                "tool_choice": { "type": "tool", "name": TOOL },
            });
            if self.temperature.load(Ordering::Acquire) {
                body["temperature"] = json!(0);
            }
            let reply = match self.http.post_json(&url, &self.headers(), &body) {
                Ok(reply) => reply,
                Err(err) => match err.downcast_ref::<ApiError>() {
                    Some(api) if self.temperature.load(Ordering::Acquire) && api.rejects(&["temperature"]) => {
                        self.temperature.store(false, Ordering::Release);
                        info!("provider does not take a temperature; sending without it");
                        continue;
                    }
                    _ => return Err(err),
                },
            };
            let blocks = reply
                .get("content")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("the provider's reply has no content"))?;
            let from_tool = blocks
                .iter()
                .find(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
                .and_then(|block| block.pointer("/input/category"))
                .and_then(Value::as_str);
            let from_text = || {
                blocks
                    .iter()
                    .find(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                    .and_then(|block| block.get("text"))
                    .and_then(Value::as_str)
            };
            let answer = from_tool
                .or_else(from_text)
                .ok_or_else(|| anyhow!("the provider's reply names no category"))?;
            return Ok(Pick {
                answer: answer.to_string(),
                usage: usage(&reply),
            });
        }
    }

    fn list_models(&self) -> Result<Vec<String>> {
        let reply = self
            .http
            .get_json(&format!("{}/v1/models?limit=1000", self.base_url), &self.headers())?;
        model_ids(&reply)
    }
}

/// The tokens a message reports using. Anthropic counts the cached prompt apart from
/// `input_tokens`, both what was read from the cache and what was written to it, so all three
/// make up the input.
fn usage(reply: &Value) -> TokenUsage {
    let cache_read = count(reply, "/usage/cache_read_input_tokens");
    TokenUsage {
        input: count(reply, "/usage/input_tokens")
            + cache_read
            + count(reply, "/usage/cache_creation_input_tokens"),
        cached_input: cache_read,
        output: count(reply, "/usage/output_tokens"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn usage_counts_cached_prompt_as_input() {
        let reply = json!({
            "content": [{ "type": "tool_use", "name": TOOL, "input": { "category": "Work" } }],
            "usage": {
                "input_tokens": 42,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 1500,
                "output_tokens": 18,
            },
        });
        assert_eq!(
            usage(&reply),
            TokenUsage {
                input: 1542,
                cached_input: 1500,
                output: 18,
            }
        );
    }

    #[test]
    fn a_cache_write_is_input_but_not_cached() {
        let reply = json!({ "usage": { "input_tokens": 42, "cache_creation_input_tokens": 1500, "output_tokens": 18 } });
        assert_eq!(
            usage(&reply),
            TokenUsage {
                input: 1542,
                cached_input: 0,
                output: 18,
            }
        );
    }

    #[test]
    fn missing_usage_counts_as_zero() {
        assert_eq!(usage(&json!({ "content": [] })), TokenUsage::default());
    }
}
