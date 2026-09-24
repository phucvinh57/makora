use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::core::model::AppInfo;

pub mod categories;
pub mod queries;
pub mod sqlite;

pub use categories::{
    AiReviewPage, CategorizationResetOutcome, Category, CategoryStore, FeedbackStats,
    LabeledActivity, NewCategory, SeedTaxonomy,
};
pub use queries::{
    ActivityStats, CategoryFilter, CategorySourceFilter, HistoryReader, SessionFilter, SessionRow,
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

    fn refine_title(&mut self, _id: i64, _window_title: &str) -> Result<()> {
        Ok(())
    }

    fn close_dangling_sessions(&mut self) -> Result<()>;
}
