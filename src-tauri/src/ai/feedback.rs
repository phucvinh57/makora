//! Turns the user's categorization decisions into examples for the LLM prompt.
//!
//! What the user did tells the model different things. Overriding an AI label or rejecting it
//! is the strongest signal — the model was wrong — so those come first; confirming a label and
//! pinning an activity by hand say what is right, and follow.

use std::collections::HashMap;

use crate::category::url_location;
use crate::store::LabeledActivity;

use super::llm::{Example, Verdict};

/// Corrections and rejections shown to the model for every activity.
const GLOBAL_EXAMPLES: usize = 10;
/// Examples shown for one activity: those sharing its domain or executable.
const KEY_EXAMPLES: usize = 5;

/// Ordered by how much the model should learn from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Corrected,
    Rejected,
    Confirmed,
    Pinned,
}

fn classify(activity: &LabeledActivity) -> Option<(Kind, Verdict)> {
    match (
        activity.category_id,
        activity.ai_category_id,
        activity.ai_rejected_category_id,
    ) {
        (Some(pin), Some(ai), _) if ai != pin => Some((Kind::Corrected, Verdict::Is(pin))),
        (Some(pin), Some(_), _) => Some((Kind::Confirmed, Verdict::Is(pin))),
        (Some(pin), None, Some(_)) => Some((Kind::Corrected, Verdict::Is(pin))),
        (Some(pin), None, None) => Some((Kind::Pinned, Verdict::Is(pin))),
        (None, _, Some(rejected)) => Some((Kind::Rejected, Verdict::Not(rejected))),
        (None, _, None) => None,
    }
}

pub struct FeedbackIndex {
    global: Vec<Example>,
    by_key: HashMap<(String, String), Vec<Example>>,
}

impl FeedbackIndex {
    /// `labeled` should be newest first: recency breaks ties within a kind.
    pub fn new(labeled: &[LabeledActivity]) -> Self {
        let mut ranked: Vec<(Kind, &LabeledActivity, Example)> = labeled
            .iter()
            .filter_map(|activity| {
                let (kind, verdict) = classify(activity)?;
                let example = Example {
                    app_name: activity.app_name.clone(),
                    url: activity.url.as_deref().and_then(url_location),
                    title: activity.title.clone(),
                    verdict,
                };
                Some((kind, activity, example))
            })
            .collect();
        ranked.sort_by_key(|(kind, ..)| *kind);

        let mut global = Vec::new();
        let mut by_key: HashMap<(String, String), Vec<Example>> = HashMap::new();
        for (kind, activity, example) in ranked {
            if matches!(kind, Kind::Corrected | Kind::Rejected) && global.len() < GLOBAL_EXAMPLES {
                global.push(example.clone());
            }
            let examples = by_key
                .entry((activity.key_kind.clone(), activity.key.clone()))
                .or_default();
            if examples.len() < KEY_EXAMPLES {
                examples.push(example);
            }
        }
        Self { global, by_key }
    }

    pub fn global(&self) -> &[Example] {
        &self.global
    }

    /// The examples that share `key_kind` and `key` with an activity, best first.
    pub fn for_key(&self, key_kind: &str, key: &str) -> Vec<Example> {
        self.by_key
            .get(&(key_kind.to_string(), key.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}
