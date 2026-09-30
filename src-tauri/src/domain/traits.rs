//! The read-side services' public interface, one trait per service, gathered here so the whole
//! surface can be read in one place. Each trait is implemented by its `Sqlite`-prefixed struct in
//! the sibling module of the same name (`HistorySvc` by `SqliteHistory` in `history.rs`, and so
//! on); `mod.rs` re-exports both.

use anyhow::Result;

use crate::shared::time::TimeRange;

use super::types::{
    ActivityStats, AiReviewGroupPage, AiReviewPage, FeedbackStats, LabeledActivity, ReviewFilter,
    SessionFilter, SessionsPage, UncategorizedActivity,
};

pub trait HistorySvc: Send + Sync {
    fn get_sessions(&self, page: i64, page_size: i64, filter: &SessionFilter) -> Result<SessionsPage>;
    fn get_activity_stats(&self, range: &TimeRange) -> Result<ActivityStats>;
}

pub trait LabelingSvc: Send + Sync {
    /// Activities for the LLM to label, most recently seen first. Normally those with neither a
    /// pin nor an AI label nor a rejected one; with `recheck`, those carrying an AI label the
    /// user has not confirmed.
    fn get_activities_to_label(&self, recheck: bool) -> Result<Vec<UncategorizedActivity>>;
    /// Every activity the user has pinned or rejected an AI label for, newest first.
    fn get_labeled_activities(&self) -> Result<Vec<LabeledActivity>>;
    /// AI labels awaiting review, longest-tracked first so the ones that matter come up first.
    /// Filtering by a category includes everything under it.
    fn get_review_page(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewPage>;
    /// The pending AI labels folded into one row per source and suggested category, the
    /// longest-tracked first. `filter.activities` is ignored: a group picks its own members.
    fn get_review_groups(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewGroupPage>;
    fn get_feedback_stats(&self) -> Result<FeedbackStats>;
}
