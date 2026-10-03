//! The categorization context: how an activity comes to be filed under a category — pinned by
//! the user, matched by one of the user's rules, or suggested by the model and then
//! accepted, changed or rejected.
//!
//! The review queue and the feedback counts are read models and still live in
//! [`crate::domain::labeling`].

pub mod application;
pub mod domain;
pub mod ports;
pub mod rules_store;
pub mod store;

pub use application::{
    ActivityIndexer, BackgroundIndexer, ClassificationSvc, EmbeddingSvc, RuleSvc, SqliteClassification,
    SqliteEmbeddings, SqliteRules,
};
