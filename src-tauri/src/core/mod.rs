pub mod category;
pub mod llm;
pub mod model;
pub mod platform;
pub mod store;
pub mod title;
pub mod tracker;
pub mod watcher;

pub use model::{BrowserKind, TabEvent, WatchEvent};
pub use platform::PlatformFocusWatcher;
pub use store::{
    open_connection, ActivityStats, CategorizationResetOutcome, Category, CategoryFilter,
    CategorySourceFilter, CategoryStore, HistoryReader, NewCategory, SeedTaxonomy, SessionFilter,
    SessionRow, SessionStore, SqliteStore,
};
pub use tracker::SessionTracker;
pub use watcher::FocusWatcher;
