use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::core::model::AppInfo;

pub mod categories;
pub mod ignore;
pub mod queries;
pub mod query_whitelist;
pub mod sqlite;
pub mod work_sessions;

pub use categories::{
    CategorizePreview, Category, CategoryKeyRef, CategoryRule, CategoryStore, RecategorizeFilter,
};
pub use ignore::{IgnoreRule, IgnoreStore};
pub use queries::{ActivityStats, CategoryFilter, HistoryReader, SessionFilter, SessionRow};
pub use query_whitelist::{QueryWhitelistEntry, QueryWhitelistStore};
pub use sqlite::{open_connection, SqliteStore};
pub use work_sessions::{WorkSession, WorkSessionStats, WorkSessionStore, WorkSessionUpdate};

/// Persistence for app-usage sessions.
///
/// A storage backend only needs to support opening and closing sessions —
/// it has no opinion on *when* a session should end, that decision belongs
/// to `SessionTracker`. This is also the seam for swapping backends later
/// (e.g. an append-only log or a date-rolled LSM tree) without touching
/// `SessionTracker` or anything upstream of it.
pub trait SessionStore {
    /// Insert a new open session (`end_time` is NULL) and return its id.
    /// `url` is `Some` when this row represents a browser tab rather than a
    /// plain app focus.
    fn start_session(
        &mut self,
        app: &AppInfo,
        window_title: &str,
        url: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<i64>;

    /// Close a previously opened session.
    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()>;

    /// Close any sessions left open from a previous run (e.g. the process
    /// was killed instead of shut down cleanly). Called once at startup.
    fn close_dangling_sessions(&mut self) -> Result<()>;
}
