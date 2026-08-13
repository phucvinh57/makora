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
pub mod ignore;
pub mod model;
pub mod platform;
pub mod store;
pub mod tracker;
pub mod watcher;

pub use model::{BrowserKind, TabEvent, WatchEvent};
pub use platform::PlatformFocusWatcher;
pub use store::{
    open_connection, ActivityStats, CategorizePreview, Category, CategoryFilter, CategoryRule,
    CategoryStore, HistoryReader, IgnoreRule, IgnoreStore, QueryWhitelistEntry,
    QueryWhitelistStore, RecategorizeOutcome, SessionFilter, SessionRow, SessionStore, SqliteStore,
    WorkSession, WorkSessionStats, WorkSessionStore, WorkSessionUpdate,
};
pub use tracker::SessionTracker;
pub use watcher::FocusWatcher;
