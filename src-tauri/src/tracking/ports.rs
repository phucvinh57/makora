//! The two edges of the tracking context. [`SessionTracker`] is how the outside tells it what
//! happened; [`SessionRecorder`] is how it records the blocks of activity that follow.

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::shared::ids::SessionId;

use super::domain::events::{FocusChange, FocusedApp, PresenceChange, TabChange};

/// Driven by the window watchers and the browser extension.
pub trait SessionTracker: Send {
    fn handle_focus(&mut self, change: FocusChange) -> Result<()>;
    fn handle_tab(&mut self, change: TabChange) -> Result<()>;
    fn handle_presence(&mut self, change: PresenceChange) -> Result<()>;
    /// Closes whatever is open, e.g. as the app shuts down.
    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()>;
}

/// Where blocks of activity are kept.
pub trait SessionRecorder: Send + Sync {
    fn start(&self, app: &FocusedApp, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<SessionId>;
    /// Closes the block in one transaction, unless it lasted less than
    /// [`MIN_SESSION_SECONDS`](super::domain::session_policy::MIN_SESSION_SECONDS), in which case
    /// it is dropped and its orphaned activity pruned.
    fn end(&self, id: SessionId, at: DateTime<Utc>) -> Result<()>;
    /// A browser reported the real title for a session that opened with a placeholder one. If
    /// that resolves to a different activity than the one the session opened against, the
    /// session moves onto it, carrying its manual pin along if the target has none yet.
    fn refine_title(&self, id: SessionId, app: &FocusedApp, window_title: &str, url: Option<&str>) -> Result<()>;
    /// Closes every block a crash or a forced quit left open, and prunes the activities that
    /// leaves orphaned.
    fn close_dangling(&self) -> Result<()>;
}
