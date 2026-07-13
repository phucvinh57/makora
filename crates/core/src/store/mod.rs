use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::model::AppInfo;

pub mod sqlite;

pub use sqlite::SqliteStore;

/// Persistence for app-usage sessions.
///
/// A storage backend only needs to support opening and closing sessions —
/// it has no opinion on *when* a session should end, that decision belongs
/// to `SessionTracker`. This is also the seam for swapping backends later
/// (e.g. an append-only log or a date-rolled LSM tree) without touching
/// `SessionTracker` or anything upstream of it.
pub trait SessionStore {
    /// Insert a new open session (`end_time` is NULL) and return its id.
    fn start_session(&mut self, app: &AppInfo, window_title: &str, at: DateTime<Utc>) -> Result<i64>;

    /// Close a previously opened session.
    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()>;

    /// Close any sessions left open from a previous run (e.g. the process
    /// was killed instead of shut down cleanly). Called once at startup.
    fn close_dangling_sessions(&mut self) -> Result<()>;

    /// Insert a new open tab sub-session (`end_time` is NULL) under a parent
    /// app session and return its id.
    fn start_tab_session(&mut self, session_id: i64, url: &str, title: &str, at: DateTime<Utc>) -> Result<i64>;

    /// Close a previously opened tab sub-session.
    fn end_tab_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()>;
}
