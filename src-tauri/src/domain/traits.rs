//! The business layer's public interface, one trait per service, gathered here so the whole
//! surface can be read in one place. Each trait is implemented by its `Sqlite`-prefixed struct in
//! the sibling module of the same name (`Categories` by `SqliteCategories` in `categories.rs`,
//! and so on); that module re-exports its trait, so callers keep writing `crate::domain::Categories`.

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::focus::AppInfo;

use super::types::{
    ActivityStats, AiReviewGroupPage, AiReviewPage, Category, FeedbackStats,
    LabeledActivity, NewCategory, ReviewFilter, SessionFilter, SessionsPage, UncategorizedActivity,
};

pub trait CategorySvc: Send + Sync {
    /// Every category, flat — the caller (the UI) assembles the tree from `parent_id`.
    /// Siblings come out in the order they should be shown under their parent.
    fn get_list(&self) -> Result<Vec<Category>>;
    #[allow(dead_code)]
    fn get(&self, id: i64) -> Result<Option<Category>>;
    fn create(&self, new: &NewCategory) -> Result<Category>;
    /// Replaces every editable field.
    fn update(&self, id: i64, new: &NewCategory) -> Result<Category>;
    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its pins with it.
    fn delete(&self, id: i64) -> Result<()>;
    /// Pins the activity behind each session row to `category_id` (or unpins with `None`).
    fn assign_sessions(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()>;
    /// Like [`assign_sessions`](Self::assign_sessions), for activities picked directly.
    fn assign_activities(&self, activity_ids: &[i64], category_id: Option<i64>) -> Result<()>;
    /// Clears every pinned category in one transaction while keeping the taxonomy and tracking
    /// history intact.
    fn reset_categorization(&self) -> Result<()>;
}

pub trait HistorySvc: Send + Sync {
    fn get_sessions(&self, page: i64, page_size: i64, filter: &SessionFilter) -> Result<SessionsPage>;
    fn get_activity_stats(&self, start: &str, end: &str) -> Result<ActivityStats>;
}

pub trait LabelingSvc: Send + Sync {
    /// Activities for the LLM to label, most recently seen first. Normally those with neither a
    /// pin nor an AI label nor a rejected one; with `recheck`, those carrying an AI label the
    /// user has not confirmed.
    fn get_activities_to_label(&self, recheck: bool) -> Result<Vec<UncategorizedActivity>>;
    /// Stores AI labels as `(activity_id, category_id, confidence)`; `None` clears a label the
    /// model no longer stands by. A manual pin made in the meantime is untouched: reads prefer
    /// it over the AI label. Returns how many labels actually changed.
    fn assign_ai(&self, labels: &[(i64, Option<i64>, f64)]) -> Result<usize>;
    /// Turns each activity's AI label into a manual pin.
    fn accept_ai_labels(&self, activity_ids: &[i64]) -> Result<()>;
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

pub trait TrackingSvc: Send + Sync {
    fn start(&self, app: &AppInfo, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<i64>;
    /// Closes the row and derives its category in Rust in one transaction, unless the block
    /// lasted less than `MIN_SESSION_SECONDS`, in which case the row is dropped instead.
    fn end(&self, id: i64, at: DateTime<Utc>) -> Result<()>;
    /// A browser reported the real title for a session that opened with a placeholder one. If
    /// that resolves to a different activity than the one the session opened against, the
    /// session moves onto it, carrying its manual pin along if the target has none yet.
    fn refine_title(&self, id: i64, app: &AppInfo, window_title: &str, url: Option<&str>) -> Result<()>;
    /// Closes every row a crash or a forced quit left open, and prunes the activities that
    /// leaves orphaned.
    fn close_dangling(&self) -> Result<()>;
}
