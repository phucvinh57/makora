//! What the categorization context needs from the outside world: something that can read a
//! taxonomy and some activities and say which category each belongs to. The local LLM in
//! [`crate::ai::llm`] is the implementation; nothing here knows about it.

use anyhow::Result;

use crate::domain::types::Category;
use crate::shared::confidence::Confidence;
use crate::shared::ids::CategoryId;

use super::domain::feedback::Example;

/// What the model is shown about one activity.
#[derive(Debug, Clone)]
pub struct ActivityText {
    pub app_name: String,
    /// `domain/path` of the page, when the activity is a browser tab.
    pub url: Option<String>,
    pub title: String,
    /// What the user already decided about similar activities.
    pub examples: Vec<Example>,
}

pub trait Classifier: Send + Sync {
    /// The model used when the caller does not name one.
    fn default_model(&self) -> &str;

    /// Classifies each activity into one of `taxonomy`, reporting `(index, category,
    /// confidence)` through `on_answer` after every one — the category is `None` when none
    /// fits. Returning `false` from `on_answer` stops the run, keeping what was reported so far.
    ///
    /// `global` are the user's corrections shown for every activity; each activity's own
    /// `examples` are added to its prompt.
    fn classify(
        &self,
        model: &str,
        taxonomy: &[Category],
        global: &[Example],
        activities: &[ActivityText],
        on_answer: &mut dyn FnMut(usize, Option<CategoryId>, Confidence) -> bool,
    ) -> Result<()>;
}
