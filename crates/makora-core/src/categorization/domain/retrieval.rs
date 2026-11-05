//! Finding the activities that look most like another. Activities are compared by the
//! embeddings of their text, so "Pull request · acme/api" lands near other code-review pages
//! whatever words they share.

use crate::categorization::ports::Embedding;
use crate::shared::ids::{ActivityId, CategoryId};
use crate::shared::text::{clip, single_line};

/// How many related activities the model is shown with each activity it labels.
pub const RELATED_COUNT: usize = 5;

const MAX_FIELD_CHARS: usize = 200;

/// `app | domain/path | title`, the text that stands for an activity.
pub fn activity_text(app_name: &str, url: Option<&str>, title: &str) -> String {
    let mut parts = vec![single_line(app_name)];
    if let Some(url) = url {
        parts.push(clip(url, MAX_FIELD_CHARS));
    }
    parts.push(clip(title, MAX_FIELD_CHARS));
    parts.retain(|part| !part.is_empty());
    parts.join(" | ")
}

/// An activity already filed under a category, by the user or by one of their rules.
#[derive(Debug, Clone)]
pub struct RelatedActivity {
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
    pub category: CategoryId,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub activity_id: ActivityId,
    pub activity: RelatedActivity,
    pub vector: Embedding,
}

/// Up to `k` of `candidates` most similar to `query`, closest first, leaving out `own_id` — an
/// activity is not related to itself. Ties keep the order the candidates were given in, so a run
/// is repeatable.
pub fn top_related(query: &[f32], candidates: &[Candidate], own_id: ActivityId, k: usize) -> Vec<RelatedActivity> {
    let mut scored: Vec<(f32, usize)> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.activity_id != own_id)
        .map(|(position, candidate)| (similarity(query, &candidate.vector), position))
        .collect();
    let order = |a: &(f32, usize), b: &(f32, usize)| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1));
    if scored.len() > k {
        scored.select_nth_unstable_by(k, order);
        scored.truncate(k);
    }
    scored.sort_by(order);
    scored
        .into_iter()
        .map(|(_, position)| candidates[position].activity.clone())
        .collect()
}

fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
