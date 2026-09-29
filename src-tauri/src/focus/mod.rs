//! Watches OS focus/presence, tracks it into session rows, and ingests browser tab events.

pub mod ingest;
pub mod model;
pub mod platform;
pub mod tracker;
mod traits;
pub mod watcher;

pub use model::{AppInfo, BrowserKind, TabEvent, WatchEvent};
pub use platform::PlatformFocusWatcher;
pub use tracker::{ActivitySessionTracker, SessionTracker};
pub use watcher::FocusWatcher;
