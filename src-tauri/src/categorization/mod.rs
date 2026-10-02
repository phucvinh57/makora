//! The categorization context: how an activity comes to be filed under a category — pinned by
//! the user, matched by one of the user's rules, or suggested by the model and then accepted,
//! changed or rejected.
//!
//! - [`domain`]: the [`ActivityClassification`](domain::classification::ActivityClassification)
//!   aggregate and its rules, and the [`rules`](domain::rules) users write;
//! - [`store`]: loading and saving it (a pin and an AI label, across two tables);
//! - [`rules_store`]: the user's rules and which one wins for each activity;
//! - [`application`]: the use cases built on both — changing pins and labels, and running the
//!   model over uncategorized activities;
//! - [`ports`]: what the context needs from outside (the model that does the classifying).
//!
//! The review queue and the feedback counts are read models and still live in
//! [`crate::domain::labeling`].

pub mod application;
pub mod domain;
pub mod ports;
pub mod rules_store;
pub mod store;

pub use application::{ClassificationSvc, RuleSvc, SqliteClassification, SqliteRules};
