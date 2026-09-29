//! Business layer: the rules for merging sessions, resolving categories, and running the AI
//! review queue, on top of the plain reads and writes [`crate::db`] provides. Each service holds
//! its own handle to the database and is what the rest of the app — Tauri commands, the focus
//! tracker, the AI code — talks to.

mod categories;
mod history;
mod labeling;
pub mod title;
mod tracking;
pub mod types;

pub use categories::Categories;
pub use history::History;
pub use labeling::Labeling;
pub use tracking::Tracking;
