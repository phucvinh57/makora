use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::core::model::AppInfo;

pub mod categories;
pub mod clusters;
pub mod ignore;
pub mod queries;
pub mod resolve;
pub mod sqlite;
pub mod work_sessions;

pub use categories::{
    Category, CategoryRule, CategoryStore, CategorySuggestion, MultiPurposeKey,
    RecategorizeOutcome, SeedTaxonomy,
};
pub use clusters::{ActivityCluster, ClusterAssignment};
pub use ignore::{IgnoreRule, IgnoreStore};
pub use queries::{
    ActivityStats, CategoryFilter, CategorySourceFilter, HistoryReader, SessionFilter, SessionRow,
};
pub use sqlite::{open_connection, SqliteStore};
pub use work_sessions::{WorkSession, WorkSessionStats, WorkSessionStore, WorkSessionUpdate};

pub trait SessionStore {
    fn start_session(
        &mut self,
        app: &AppInfo,
        window_title: &str,
        url: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<i64>;

    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()>;

    fn close_dangling_sessions(&mut self) -> Result<()>;
}
