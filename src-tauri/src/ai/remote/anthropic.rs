//! Anthropic's messages API. The model is made to call a tool whose one argument is limited to the
//! category list, and the taxonomy prompt is marked for caching since every request repeats it.
//! The API gives no token probabilities, so there is no confidence.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tracing::info;

use super::http::{ApiError, Http};
use super::openai::model_ids;
use super::{trimmed, ChatApi, Pick};

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
                confidence: None,
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
