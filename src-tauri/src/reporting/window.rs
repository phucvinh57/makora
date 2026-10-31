//! The one rule for how much of a session counts towards a time window: the part that overlaps
//! it, where a still-open session runs until now. Every duration in [`super`] is built from these
//! fragments, so the statistics, the review queue and the rule totals always agree.

use chrono::{DateTime, Utc};

use crate::db::sql::Param;
use crate::shared::time::{self, TimeRange};

pub struct Window {
    from: String,
    to: String,
    now: String,
}

impl Window {
    pub fn new(range: &TimeRange, now: DateTime<Utc>) -> Self {
        Self {
            from: time::to_db(range.start),
            to: time::to_db(range.end),
            now: time::to_db(now),
        }
    }

    /// Everything tracked up to `now`.
    pub fn until(now: DateTime<Utc>) -> Self {
        Self::new(
            &TimeRange {
                start: DateTime::UNIX_EPOCH,
                end: now,
            },
            now,
        )
    }

    pub(super) fn binds(&self) -> Vec<Param> {
        vec![
            Param::Text(self.from.clone()),
            Param::Text(self.to.clone()),
            Param::Text(self.now.clone()),
        ]
    }
}

/// Leads every statement that uses the fragments below, so the window's binds are always the
/// first three; the statement then joins `w`.
pub(super) const WITH_WINDOW: &str = "WITH w(lo, hi, now) AS (SELECT ?, ?, ?)";

pub(super) fn overlaps(s: &str) -> String {
    format!("({s}.start_time < w.hi AND COALESCE({s}.end_time, w.now) > w.lo)")
}

pub(super) fn clipped_start(s: &str) -> String {
    format!("MAX({s}.start_time, w.lo)")
}

pub(super) fn clipped_end(s: &str) -> String {
    format!("MIN(COALESCE({s}.end_time, w.now), w.hi)")
}

pub(super) fn clipped_seconds(s: &str) -> String {
    format!(
        "((julianday({}) - julianday({})) * 86400.0)",
        clipped_end(s),
        clipped_start(s)
    )
}

/// Total of [`clipped_seconds`] over a group; 0 when the group has no session in the window
/// (a `LEFT JOIN` that found none).
pub(super) fn sum_seconds(s: &str) -> String {
    format!("COALESCE(SUM({}), 0.0)", clipped_seconds(s))
}
