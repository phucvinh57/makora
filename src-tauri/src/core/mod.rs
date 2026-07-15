//! Platform-agnostic core of makora's app-usage tracking.
//!
//! - [`model`] — data types shared by every platform.
//! - [`watcher::FocusWatcher`] — the interface a platform implements to
//!   report focus changes.
//! - [`store::SessionStore`] — the interface a storage backend implements
//!   to persist sessions.
//! - [`tracker::SessionTracker`] — turns a stream of focus events into
//!   sessions, independent of both the platform and the storage backend.
//! - [`platform`] — picks the right `FocusWatcher` for the target OS.

pub mod category;
pub mod model;
pub mod platform;
pub mod store;
pub mod tracker;
pub mod watcher;

pub use model::{BrowserKind, FocusEvent, TabEvent};
pub use platform::PlatformFocusWatcher;
pub use store::{HistoryReader, SessionRow, SessionStore, SqliteStore};
pub use tracker::SessionTracker;
pub use watcher::FocusWatcher;
