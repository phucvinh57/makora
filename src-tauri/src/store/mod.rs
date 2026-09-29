use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::focus::AppInfo;

pub mod categories;
pub mod queries;
pub mod sqlite;
mod title;

pub use categories::{
    AiReviewGroupPage, AiReviewPage, CategorizationResetOutcome, Category, FeedbackStats,
    LabeledActivity, NewCategory, ReviewFilter, SqliteCategoryStore, UncategorizedActivity,
};
pub use queries::{
    ActivityStats, CategoryFilter, CategorySourceFilter, SessionFilter, SessionRow,
    SqliteHistoryStore,
};
pub use sqlite::{open_connection, SqliteStore};

pub trait SessionStore {
    fn start_session(
        &mut self,
        app: &AppInfo,
        window_title: &str,
        url: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<i64>;

    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()>;

    fn refine_title(
        &mut self,
        _id: i64,
        _app: &AppInfo,
        _window_title: &str,
        _url: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    fn close_dangling_sessions(&mut self) -> Result<()>;
}

/// The category taxonomy and the manual/AI labels pinned to activities.
pub trait CategoryStore: Send + Sync {
    fn list(&self) -> Result<Vec<Category>>;
    #[allow(dead_code)]
    fn get(&self, id: i64) -> Result<Option<Category>>;
    fn create(&self, new: &NewCategory) -> Result<Category>;
    fn update(&self, id: i64, new: &NewCategory) -> Result<Category>;
    fn delete(&self, id: i64) -> Result<()>;

    fn assign_sessions(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()>;
    fn assign_activities(&self, activity_ids: &[i64], category_id: Option<i64>) -> Result<()>;
    fn reset_categorization(&self) -> Result<CategorizationResetOutcome>;

    fn get_activities_to_label(&self, recheck: bool) -> Result<Vec<UncategorizedActivity>>;
    fn assign_ai(&self, labels: &[(i64, Option<i64>, f64)]) -> Result<usize>;
    fn accept_ai_labels(&self, activity_ids: &[i64]) -> Result<()>;
    fn labeled_activities(&self) -> Result<Vec<LabeledActivity>>;
    fn ai_review_page(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewPage>;
    fn ai_review_groups(
        &self,
        page: i64,
        page_size: i64,
        filter: &ReviewFilter,
    ) -> Result<AiReviewGroupPage>;
    fn feedback_stats(&self) -> Result<FeedbackStats>;
}

/// Read-only access to tracked sessions, for the history and stats views.
pub trait HistoryStore: Send + Sync {
    fn count_sessions(&self, filter: &SessionFilter) -> Result<i64>;
    fn list_sessions(&self, limit: i64, offset: i64, filter: &SessionFilter)
        -> Result<Vec<SessionRow>>;
    fn activity_stats(&self, start: &str, end: &str) -> Result<ActivityStats>;
}
