//! What the categorization context needs from the outside world: something that turns text into
//! vectors, and something that reads a taxonomy and some activities and says which category each
//! belongs to. [`Embedder`] and [`Classifier`] are implemented in [`crate::ai`]; nothing here
//! knows about them.

use anyhow::Result;

use crate::domain::types::Category;
use crate::shared::ids::{CategoryId, ProviderId};

use super::domain::retrieval::RelatedActivity;

/// A unit-length vector, so that the dot product of two is their cosine similarity.
pub type Embedding = Vec<f32>;

#[derive(Debug, Clone)]
pub struct ActivityText {
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
}

/// The language model a run labels with: `provider` is `None` for the local one, or the id of a
/// remote provider the classifier knows how to reach.
#[derive(Debug, Clone)]
pub struct LlmSpec {
    pub provider: Option<ProviderId>,
    pub model: String,
}

pub trait Embedder: Send + Sync {
    /// The embedding of each of `texts`, in order.
    fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Embedding>>;
}

pub trait Classifier: Send + Sync {
    /// Classifies each activity into one of the categories of `taxonomy`, reporting
    /// `(index, category)` through `on_answer` as each one is answered, not necessarily in order —
    /// the category is `None` when none fits. Returning `false` from `on_answer` stops the run,
    /// keeping what was reported so far.
    fn classify(
        &self,
        llm: &LlmSpec,
        taxonomy: &[Category],
        activities: &[ActivityText],
        related: &dyn Fn(usize) -> Vec<RelatedActivity>,
        on_answer: &mut dyn FnMut(usize, Option<CategoryId>) -> bool,
    ) -> Result<()>;
}
