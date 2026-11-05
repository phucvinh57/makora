use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::shared::ids::SessionId;

use super::domain::events::{FocusChange, FocusedApp, PresenceChange, TabChange};

pub trait SessionTracker: Send {
    fn handle_focus(&mut self, change: FocusChange) -> Result<()>;
    fn handle_tab(&mut self, change: TabChange) -> Result<()>;
    fn handle_presence(&mut self, change: PresenceChange) -> Result<()>;
    /// The tracker is alive at `at`: the open row, if any, is known to run at least until then.
    fn heartbeat(&mut self, at: DateTime<Utc>) -> Result<()>;
    /// Nothing that can be recorded has focus any more (Makora's own window, the desktop, or the
    /// app shutting down).
    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()>;
}

/// A row the recorder opened, or reopened when the same activity resumed right after it closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenedRow {
    pub id: SessionId,
    pub started_at: DateTime<Utc>,
}

pub trait SessionRecorder: Send + Sync {
    fn start(&self, app: &FocusedApp, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<OpenedRow>;
    /// Closes the block in one transaction, unless it lasted less than the configured minimum
    /// (`tracking.min_session_seconds` in [`crate::settings`]), in which case it is dropped and
    /// its orphaned activity pruned.
    fn end(&self, id: SessionId, at: DateTime<Utc>) -> Result<()>;
    /// Moves the open session onto the activity `window_title` resolves to. The old activity's
    /// manual pin moves along unless the target already has one, which then wins; the old
    /// activity is pruned once nothing references it.
    fn retitle(&self, id: SessionId, app: &FocusedApp, window_title: &str, url: Option<&str>) -> Result<()>;
    /// The open session was still running at `at`; a crash after this closes it there.
    fn touch(&self, id: SessionId, at: DateTime<Utc>) -> Result<()>;
    /// Closes the sessions a crash or a forced quit left open at the moment each was last seen.
    fn close_dangling(&self) -> Result<()>;
    /// Blocks shorter than this are dropped when they end.
    fn min_session_seconds(&self) -> i64;
}

/// Whether the person asked for this activity not to be recorded (`tracking.excluded` in
/// [`crate::settings`]). Asked each time a block would open, so a change applies to the next one.
pub trait ExclusionPolicy: Send + Sync {
    fn is_excluded(&self, app: &FocusedApp, window_title: &str, url: Option<&str>) -> bool;
}
