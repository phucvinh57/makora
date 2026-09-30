//! The activity-tracking context: what a focus or tab change means, and the identity an
//! activity is filed under.
//!
//! - [`domain`]: the state machine that turns focus, tab and presence changes into blocks of
//!   activity, the rules for what counts as one, and the identity of a source and an activity;
//! - [`ports`]: how the outside drives it and where it records;
//! - [`application`] and [`store`]: recording blocks in SQLite;
//! - [`adapters`]: the window watchers and the browser-extension endpoint that feed it.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
pub mod store;
