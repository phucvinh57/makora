//! Platform-agnostic core of the makora app-usage collector.
//!
//! - [`model`] — data types shared by every platform.
//! - [`watcher::FocusWatcher`] — the interface a platform implements to
//!   report focus changes.
//! - [`store::SessionStore`] — the interface a storage backend implements
//!   to persist sessions.
//! - [`tracker::SessionTracker`] — turns a stream of focus events into
//!   sessions, independent of both the platform and the storage backend.
//! - [`platform`] — picks the right `FocusWatcher` for the target OS.

pub mod model;
pub mod platform;
pub mod sqlite_store;
pub mod store;
pub mod tracker;
pub mod watcher;

pub use model::{AppInfo, BrowserKind, FocusEvent, TabEvent};
pub use platform::PlatformFocusWatcher;
pub use sqlite_store::SqliteStore;
pub use store::SessionStore;
pub use tracker::SessionTracker;
pub use watcher::FocusWatcher;
