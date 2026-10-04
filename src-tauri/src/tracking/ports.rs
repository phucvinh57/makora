use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::shared::ids::SessionId;

use super::domain::events::{FocusChange, FocusedApp, PresenceChange, TabChange};

pub trait SessionTracker: Send {
    fn handle_focus(&mut self, change: FocusChange) -> Result<()>;
    fn handle_tab(&mut self, change: TabChange) -> Result<()>;
    fn handle_presence(&mut self, change: PresenceChange) -> Result<()>;
    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()>;
}

pub trait SessionRecorder: Send + Sync {
    fn start(&self, app: &FocusedApp, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<SessionId>;
    /// Closes the block in one transaction, unless it lasted less than the configured minimum
    /// (`tracking.min_session_seconds` in [`crate::settings`]), in which case it is dropped and
    /// its orphaned activity pruned.
    fn end(&self, id: SessionId, at: DateTime<Utc>) -> Result<()>;
    /// A browser reported the real title for a session that opened with a placeholder one. If
    /// that resolves to a different activity than the one the session opened against, the
    /// session moves onto it, carrying its manual pin along if the target has none yet.
    fn refine_title(&self, id: SessionId, app: &FocusedApp, window_title: &str, url: Option<&str>) -> Result<()>;
    fn close_dangling(&self) -> Result<()>;
}

/// Whether the person asked for this activity not to be recorded (`tracking.excluded` in
/// [`crate::settings`]). Asked each time a block would open, so a change applies to the next one.
pub trait ExclusionPolicy: Send + Sync {
    fn is_excluded(&self, app: &FocusedApp, window_title: &str, url: Option<&str>) -> bool;
}
