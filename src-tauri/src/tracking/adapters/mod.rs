//! Where activity is observed: the operating system's window watchers and the local endpoint the
//! browser extension reports tabs to. Both translate what they see into domain events and hand
//! them to a [`SessionTracker`](crate::tracking::ports::SessionTracker).

pub mod ingest;
pub mod model;
pub mod platform;
pub mod watcher;
