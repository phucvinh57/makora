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

use anyhow::{bail, Context, Result};
use tracing::{info, warn};

use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::ActivityText;
use crate::domain::types::Category;
use crate::settings::{ProviderConfig, ProviderKind};
use crate::shared::ids::CategoryId;

use super::choices::choices;
use super::prompt::{related_line, system_prompt, user_prompt, NONE_ANSWER};

/// Requests in flight at once during a run.
const CONCURRENCY: usize = 4;
/// Failed activities in a row after which the run gives up, e.g. on an exhausted quota.
const MAX_FAILURES_IN_A_ROW: usize = 5;

pub trait ChatApi: Send + Sync {
    /// Asks `model` to pick one of `choices` for the activity in `user`, returning its answer.
    fn pick(&self, model: &str, system: &str, user: &str, choices: &[String]) -> Result<String>;
    /// The ids of the models the provider offers.
    fn list_models(&self) -> Result<Vec<String>>;
}

pub fn api_for(provider: &ProviderConfig, key: Option<String>) -> Box<dyn ChatApi> {
    match provider.kind {
        ProviderKind::OpenAi => Box::new(openai::OpenAi::new(&provider.base_url, key)),
        ProviderKind::Anthropic => Box::new(anthropic::Anthropic::new(&provider.base_url, key)),
    }
}

/// Classifies each activity with `model`, reporting answers through `on_answer` as they arrive.
/// The first activity is asked alone, so a wrong key or model fails the run at once; after that a
/// failed activity is skipped, unless too many fail in a row.
pub fn classify(
    api: &dyn ChatApi,
    model: &str,
    taxonomy: &[Category],
    activities: &[ActivityText],
    related: &dyn Fn(usize) -> Vec<RelatedActivity>,
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

    let first = api
        .pick(model, &system, &prompts[0], &options)
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
            let (next, stop, system, prompts, options) = (&next, &stop, &system, &prompts, &options);
            scope.spawn(move || loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let index = next.fetch_add(1, Ordering::AcqRel);
                if index >= prompts.len() {
                    break;
                }
                let result = api.pick(model, system, &prompts[index], options);
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

/// `base_url` without a trailing slash.
fn trimmed(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}
