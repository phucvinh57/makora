//! The activity-tracking context: what a focus or tab change means, and the identity an
//! activity is filed under. For now only its pure rules live here ([`domain`]); the state machine
//! and the persistence still sit in [`crate::focus`] and [`crate::domain`].

pub mod domain;
