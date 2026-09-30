//! Vocabulary every context speaks: typed ids, model confidence, and the one place that knows
//! how a point in time is stored. Nothing here knows about Diesel, Tauri or any single context.

pub mod confidence;
pub mod ids;
pub mod time;
