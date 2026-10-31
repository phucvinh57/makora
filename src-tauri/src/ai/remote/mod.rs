//! Activity categorization by a remote LLM provider: OpenAI or anything that speaks its
//! chat-completions API (OpenRouter, Groq, a local Ollama or LM Studio server, ...), or Anthropic.
//!
//! Each activity gets the same prompt the local model sees, sent as its own request, a few at a
//! time. The reply is constrained to the category list wherever the provider allows it — a JSON
//! schema or a forced tool call — and matched against the list either way.

mod anthropic;
mod http;
mod openai;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tracing::{info, warn};

use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::{ActivityText, TokenUsage, UsageMeter};
use crate::domain::types::Category;
use crate::settings::{ProviderConfig, ProviderKind};
use crate::shared::ids::CategoryId;

use super::choices::choices;
use super::prompt::{related_line, system_prompt, user_prompt, NONE_ANSWER};

/// Requests in flight at once during a run.
const CONCURRENCY: usize = 4;
/// Failed activities in a row after which the run gives up, e.g. on an exhausted quota.
const MAX_FAILURES_IN_A_ROW: usize = 5;

/// A model's answer and the tokens the request took.
pub struct Pick {
    pub answer: String,
    pub usage: TokenUsage,
}

pub trait ChatApi: Send + Sync {
    /// Asks `model` to pick one of `choices` for the activity in `user`.
    fn pick(&self, model: &str, system: &str, user: &str, choices: &[String]) -> Result<Pick>;
    /// The ids of the models the provider offers.
    fn list_models(&self) -> Result<Vec<String>>;
}

pub fn api_for(provider: &ProviderConfig, key: Option<String>) -> Box<dyn ChatApi> {
    match provider.kind {
        ProviderKind::OpenAi => Box::new(openai::OpenAi::new(&provider.base_url, key)),
        ProviderKind::Anthropic => Box::new(anthropic::Anthropic::new(&provider.base_url, key)),
    }
}

/// Classifies each activity with `model`, reporting answers through `on_answer` as they arrive
/// and every request through `meter`. The first activity is asked alone, so a wrong key or model
/// fails the run at once; after that a failed activity is skipped, unless too many fail in a row.
///
/// Whatever happens, once anything was sent one `makora_lib::network` event records where it
/// went and what it held, never the prompt itself.
#[allow(clippy::too_many_arguments)]
pub fn classify(
    api: &dyn ChatApi,
    base_url: &str,
    model: &str,
    taxonomy: &[Category],
    activities: &[ActivityText],
    related: &dyn Fn(usize) -> Vec<RelatedActivity>,
    meter: &UsageMeter,
    on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
) -> Result<()> {
    let before = meter.snapshot();
    let sent = AtomicUsize::new(0);
    let result = classify_all(api, model, taxonomy, activities, related, meter, &sent, on_answer);
    let activities_sent = sent.into_inner();
    if activities_sent > 0 {
        let after = meter.snapshot();
        info!(
            target: "makora_lib::network",
            host = %host_of(base_url),
            purpose = "AI categorization",
            model,
            activities_sent,
            fields = SENT_FIELDS,
            input_tokens = after.input_tokens - before.input_tokens,
            output_tokens = after.output_tokens - before.output_tokens,
            "sent activities to remote provider"
        );
    }
    result
}

/// What each request tells the provider, as built by [`user_prompt`] and [`system_prompt`].
const SENT_FIELDS: &str = "app name, URL domain and path (no query), window title, \
    category names and descriptions, similar filed activities (app, URL domain and path, title, category)";

/// One request, timed and counted.
struct Asker<'a> {
    api: &'a dyn ChatApi,
    model: &'a str,
    system: &'a str,
    options: &'a [String],
    meter: &'a UsageMeter,
    sent: &'a AtomicUsize,
}

impl Asker<'_> {
    fn ask(&self, user: &str) -> Result<String> {
        self.sent.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        match self.api.pick(self.model, self.system, user, self.options) {
            Ok(pick) => {
                self.meter.record(pick.usage, started.elapsed());
                Ok(pick.answer)
            }
            Err(err) => {
                self.meter.record_failure(started.elapsed());
                Err(err)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn classify_all(
    api: &dyn ChatApi,
    model: &str,
    taxonomy: &[Category],
    activities: &[ActivityText],
    related: &dyn Fn(usize) -> Vec<RelatedActivity>,
    meter: &UsageMeter,
    sent: &AtomicUsize,
    on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
) -> Result<()> {
    let choices = choices(taxonomy);
    if choices.is_empty() {
        bail!("there are no categories to choose from");
    }
    if activities.is_empty() {
        return Ok(());
    }
    let paths: HashMap<CategoryId, &str> = choices.iter().map(|c| (c.id, c.path.as_str())).collect();
    let by_path: HashMap<&str, CategoryId> = choices.iter().map(|c| (c.path.as_str(), c.id)).collect();
    let options: Vec<String> = std::iter::once(NONE_ANSWER.to_string())
        .chain(choices.iter().map(|c| c.path.clone()))
        .collect();
    let system = system_prompt(&choices);
    let prompts: Vec<String> = activities
        .iter()
        .enumerate()
        .map(|(index, activity)| {
            let lines: Vec<String> = related(index)
                .iter()
                .filter_map(|related| related_line(related, &paths))
                .collect();
            user_prompt(activity, &lines)
        })
        .collect();
    let mut report = |index: usize, answer: String| {
        let category = matching_choice(&answer, &options).and_then(|path| by_path.get(path).copied());
        on_answer(index, category)
    };
    let asker = Asker {
        api,
        model,
        system: &system,
        options: &options,
        meter,
        sent,
    };

    let first = asker
        .ask(&prompts[0])
        .with_context(|| format!("asking {model} to categorize"))?;
    if !report(0, first) || prompts.len() == 1 {
        return Ok(());
    }

    let next = AtomicUsize::new(1);
    let stop = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel::<(usize, Result<String>)>();
    std::thread::scope(|scope| {
        for _ in 0..CONCURRENCY.min(prompts.len() - 1) {
            let tx = tx.clone();
            let (next, stop, prompts, asker) = (&next, &stop, &prompts, &asker);
            scope.spawn(move || loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::AcqRel);
                if index >= prompts.len() {
                    break;
                }
                let result = asker.ask(&prompts[index]);
                if tx.send((index, result)).is_err() {
                    break;
                }
            });
        }
        drop(tx);

        let (mut failed, mut in_a_row) = (0, 0);
        let mut outcome = Ok(());
        for (index, result) in rx.iter() {
            match result {
                Ok(answer) => {
                    in_a_row = 0;
                    if !report(index, answer) {
                        break;
                    }
                }
                Err(err) => {
                    failed += 1;
                    in_a_row += 1;
                    warn!(index, error = format!("{err:#}"), "provider could not categorize an activity; skipping it");
                    if in_a_row >= MAX_FAILURES_IN_A_ROW {
                        outcome = Err(err.context(format!("{in_a_row} activities in a row could not be categorized")));
                        break;
                    }
                }
            }
        }
        // Workers finish the request they are on, and take no new one.
        stop.store(true, Ordering::Release);
        drop(rx);
        if failed > 0 {
            info!(failed, "activities skipped after provider errors");
        }
        outcome
    })
}

/// The option the reply names: exactly, or once stripped of quotes and a trailing period, or
/// ignoring case — what a model that was not held to the list tends to add.
fn matching_choice<'a>(answer: &str, options: &'a [String]) -> Option<&'a str> {
    let exact = answer.trim();
    let cleaned = exact
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '*'))
        .trim_end_matches('.')
        .trim();
    options
        .iter()
        .find(|option| option.as_str() == exact || option.as_str() == cleaned)
        .or_else(|| options.iter().find(|option| option.eq_ignore_ascii_case(cleaned)))
        .map(String::as_str)
}

/// A token count at `pointer` in a reply; zero when the provider does not report it.
fn count(reply: &Value, pointer: &str) -> u64 {
    reply.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

/// The host part of `base_url`, all that the network log names of a provider.
fn host_of(base_url: &str) -> String {
    url::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `base_url` without a trailing slash.
fn trimmed(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::*;

    /// Answers "Work" for every activity, except those whose title contains "fail".
    struct FakeApi;

    impl ChatApi for FakeApi {
        fn pick(&self, _model: &str, _system: &str, user: &str, _choices: &[String]) -> Result<Pick> {
            if user.contains("fail") {
                return Err(anyhow!("rate limited"));
            }
            Ok(Pick {
                answer: "Work".to_string(),
                usage: TokenUsage {
                    input: 100,
                    cached_input: 80,
                    output: 2,
                },
            })
        }

        fn list_models(&self) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    fn activity(title: &str) -> ActivityText {
        ActivityText {
            app_name: "Code".to_string(),
            url: None,
            title: title.to_string(),
        }
    }

    #[test]
    fn every_request_is_metered_failures_included() {
        let taxonomy = [Category {
            id: CategoryId(1),
            name: "Work".to_string(),
            color: "#000000".to_string(),
            parent_id: None,
            description: None,
        }];
        let activities = [activity("a"), activity("b"), activity("fail"), activity("c")];
        let meter = UsageMeter::default();
        let mut answered = Vec::new();
        classify(
            &FakeApi,
            "https://api.example.com/v1",
            "model",
            &taxonomy,
            &activities,
            &|_| Vec::new(),
            &meter,
            &mut |index, category| {
                answered.push((index, category));
                true
            },
        )
        .unwrap();

        answered.sort();
        assert_eq!(answered, [(0, Some(CategoryId(1))), (1, Some(CategoryId(1))), (3, Some(CategoryId(1)))]);
        let usage = meter.snapshot();
        assert_eq!((usage.requests, usage.failed_requests), (4, 1));
        assert_eq!((usage.input_tokens, usage.cached_input_tokens, usage.output_tokens), (300, 240, 6));
    }

    #[test]
    fn the_network_log_names_only_the_host() {
        assert_eq!(host_of("https://api.openai.com/v1"), "api.openai.com");
        assert_eq!(host_of("http://user:secret@localhost:11434/v1/"), "localhost");
        assert_eq!(host_of("not a url"), "unknown");
    }
}
