pub mod category;
pub mod classifier;
pub mod ignore;
pub mod model;
pub mod platform;
pub mod store;
pub mod tracker;
pub mod watcher;

pub use classifier::{AiCategorizationSchedule, AiCategorizationService, ClassifierStatus};
pub use model::{BrowserKind, TabEvent, WatchEvent};
pub use platform::PlatformFocusWatcher;
pub use store::{
    open_connection, ActivityStats, Category, CategoryFilter, CategoryRule, CategorySourceFilter,
    CategoryStore, HistoryReader, IgnoreRule, IgnoreStore, RecategorizeOutcome, SessionFilter,
    SessionRow, SessionStore, SqliteStore, WorkSession, WorkSessionStats, WorkSessionStore,
    WorkSessionUpdate,
};
pub use tracker::SessionTracker;
pub use watcher::FocusWatcher;
