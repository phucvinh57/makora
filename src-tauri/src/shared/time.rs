//! Times are stored as RFC 3339 text, which SQLite compares as strings. Every write and every
//! bound query parameter goes through [`to_db`] so they all use the same spelling (`+00:00`,
//! never `Z`); a range or timestamp taken from the UI is parsed first and only then formatted.

use chrono::{DateTime, Utc};

pub fn to_db(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

pub fn parse_db(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}
