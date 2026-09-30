//! The reporting context: read models over what the other contexts recorded — the history list,
//! the statistics, the AI review queue. They are hand-written SQL (aggregates, computed joins)
//! that Diesel's typed builder cannot express cleanly, and they never write.
//!
//! The category of an activity is not re-derived here: every query joins the `activity_category`
//! view (migration 7), the SQL form of the precedence rules in
//! [`crate::categorization::domain::classification`].

pub mod history;
pub mod review;
mod scope;

pub use scope::CategoryScope;
