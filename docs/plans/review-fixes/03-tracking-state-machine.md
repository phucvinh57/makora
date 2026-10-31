# 03-tracking-state-machine

Files: `tracking/domain/focus_machine.rs`, `session_policy.rs`, `tracking/application.rs`, `tracking/adapters/model.rs`, platform watchers.
1. **Make illegal states unrepresentable**: replace `Current { id: Option<SessionId>, .. }` + separate `present` with `Current { app, browser, url, window_title, row: Row }`, `enum Row { Open { id, started_at }, Closed }`. Add one `reconcile(at)` that decides open/close/retitle from (present, exclusion, browser-domain rule) — every handler mutates `Current` then calls `reconcile`. This fixes "row closed by title exclusion never reopens on same URL" (`handle_tab` same-URL path).
2. **Monotonic time**: machine keeps `last_at`; every incoming `at = max(at, last_at)`. `is_noise` no longer sees negative durations (ingest uses receive time while focus events are queued earlier).
3. **Fast-changing titles lose time**: if the open row is younger than `min_session_seconds`, a title change on the same app **retitles** the open row (reassign activity, prune old) instead of close+open. Generalize `refine_title` → `retitle(id, app, title, url)`.
4. **PinConflict** (`application.rs:117-122`): still reassign the session to the target activity; target's pin wins, placeholder pruned.
5. **Exclusion set cache**: `is_excluded` (`application.rs:158`) caches `ExclusionSet` with the `Vec<Exclusion>` it was built from; rebuild only when settings differ.
6. **Unattributed focus**: add `FocusEvent` "nothing attributable" (GNOME `present=false` `gnome.rs`, Makora's own window on Windows/macOS/Linux — drop `WINEVENT_SKIPOWNPROCESS`, filter in callback). Machine closes the current row (keeps machine alive).
7. **wlroots titles** (`linux/wlr.rs:182`): emit on `Done` when activated and (`active != id` **or** title changed since last emit).
Tests: focus machine with a fake recorder — reopen after exclusion, out-of-order timestamps, retitle young row, presence off/on, unattributed focus.
