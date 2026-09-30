//! What remains of the original business layer once tracking, taxonomy and categorization moved
//! into their own contexts: the services behind the history and review pages, and the shapes
//! the frontend sees ([`types`]). Each service holds its own handle to the database and is what
//! the Tauri commands talk to.

mod history;
mod labeling;
mod traits;
pub mod types;

pub use history::{HistorySvc, SqliteHistory};
pub use labeling::{LabelingSvc, SqliteLabeling};
