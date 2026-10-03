use std::ops::RangeInclusive;

use chrono::{DateTime, Utc};

use super::source_key::exe_file_name;

/// Blocks shorter than this are dropped unless the person chose another threshold. Anything
/// that brief is a window flashing past, an alt-tab on the way somewhere else, a tab corrected
/// after its title loaded; it was never something they were doing.
pub const DEFAULT_MIN_SESSION_SECONDS: i64 = 5;

/// What the threshold may be set to. The floor is one second so a redirect or a title that
/// corrects itself is never kept; the ceiling stops a typo from discarding real work.
pub const MIN_SESSION_SECONDS_RANGE: RangeInclusive<i64> = 1..=300;

/// A block that reopens within this long of closing is the same activity resuming (alt-tab
/// away and back, a window manager hiccup), not a new one starting.
pub const MERGE_GAP_SECONDS: i64 = 2;

const IGNORED_APPS: &[&str] = &["explorer.exe", "gnome-shell"];

/// Whether a block that started at `started_at` and ended at `ended_at` lasted less than
/// `min_seconds` and so is too brief to keep. Blocks that short are deleted instead of closed,
/// so no query downstream has to filter by duration.
pub fn is_noise(started_at: DateTime<Utc>, ended_at: DateTime<Utc>, min_seconds: i64) -> bool {
    ended_at.signed_duration_since(started_at).num_milliseconds() < min_seconds * 1000
}

pub fn should_merge(closed_at: DateTime<Utc>, resumed_at: DateTime<Utc>) -> bool {
    let gap = resumed_at.signed_duration_since(closed_at).num_milliseconds();
    (0..=MERGE_GAP_SECONDS * 1000).contains(&gap)
}

pub fn is_ignored_app(exe_path: &str) -> bool {
    let file_name = exe_file_name(exe_path);
    IGNORED_APPS
        .iter()
        .any(|ignored| ignored.eq_ignore_ascii_case(file_name))
}
