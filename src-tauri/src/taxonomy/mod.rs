//! The taxonomy context: the tree of categories.
//!
//! - [`domain`]: the shape of the tree and the questions asked of it (descendants, deletion
//!   order);
//! - [`error`]: why a change to it can be refused;
//! - [`application`] and [`store`]: creating, editing and deleting categories in SQLite.

pub mod application;
pub mod domain;
pub mod error;
pub mod store;

pub use application::{CategorySvc, SqliteCategories};
pub use error::TaxonomyError;
