//! The rules for what counts as a block of activity, as pure functions of time.

use chrono::{DateTime, Utc};

use super::source_key::exe_file_name;

/// A focus or tab change that is undone within a second never was activity: a page redirects,
/// a browser reports a tab before its title has loaded and corrects itself, a window flashes
/// past on the way somewhere else. Blocks that short are deleted instead of closed, so no query
/// downstream has to filter by duration.
pub const MIN_SESSION_SECONDS: i64 = 1;

/// A block that reopens within this long of closing is the same activity resuming (alt-tab
/// away and back, a window manager hiccup), not a new one starting.
pub const MERGE_GAP_SECONDS: i64 = 2;

/// Windows that are the shell itself, not something the person is doing.
const IGNORED_APPS: &[&str] = &["explorer.exe", "gnome-shell"];

/// Whether a block that started at `started_at` and ended at `ended_at` was too brief to keep.
pub fn is_noise(started_at: DateTime<Utc>, ended_at: DateTime<Utc>) -> bool {
    ended_at.signed_duration_since(started_at).num_milliseconds() < MIN_SESSION_SECONDS * 1000
}

/// Whether activity resuming at `resumed_at` continues the block that closed at `closed_at`.
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
