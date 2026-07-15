/// Checked by [`crate::core::tracker::SessionTracker`] before opening a
/// session for a newly-focused app or a newly-visited site: a match means
/// the time isn't recorded at all (as opposed to the hardcoded
/// `IGNORED_APPS` list in `tracker.rs`, which only skips *interrupting* the
/// current session — this stops the ignored app/site itself from ever
/// accumulating tracked time). Implemented by `IgnoreStore` against the
/// user-editable `ignore_rules` table; storage-agnostic so `SessionTracker`
/// doesn't depend on SQLite directly.
pub trait IgnorePolicy: Send + Sync {
    fn is_app_ignored(&self, exe_path: &str) -> bool;
    fn is_site_ignored(&self, url: &str) -> bool;
}

/// The default policy: nothing is ignored. Used where no ignore list is
/// configured (e.g. `SessionTracker::new` in tests).
#[cfg(test)]
pub struct NoopIgnorePolicy;

#[cfg(test)]
impl IgnorePolicy for NoopIgnorePolicy {
    fn is_app_ignored(&self, _exe_path: &str) -> bool {
        false
    }

    fn is_site_ignored(&self, _url: &str) -> bool {
        false
    }
}
