# 09-reporting

Files: `reporting/history.rs`, `review.rs`, `rules.rs`, `domain/labeling.rs`, `domain/types.rs`, `host/commands/reporting.rs`.
1. **One duration rule**: shared SQL fragment `clipped_seconds(from, to)` = `julianday(MIN(COALESCE(end_time, now), to)) - julianday(MAX(start_time, from))`, filter = overlap (`start < to AND COALESCE(end, now) > from`) — same as history list. Replaces 6 copies (`history.rs:197,209,224,255`, `review.rs:20`, `rules.rs:5`). Open sessions count up to now.
2. **Daily split**: fetch clipped intervals, bucket by local day in Rust — pure fn `split_by_local_day` (tested across midnight/DST).
3. LIKE escape (`history.rs:50`): escape `\` first.
4. Paging (`labeling.rs:45-70`, `review.rs`): clamp like `domain/history.rs:29-43`; count + rows in one read transaction.
5. `CategorySourceFilter` gains `Rule` (backend parse + history source filter); frontend filter (see 12).
6. Precedence (pin > rule > AI) stays decided by the `activity_category` view; feedback `rejected` count excludes rule-matched activities; fix stale comment `history.rs:14-16`.
7. `session_list` `#[allow(clippy::too_many_arguments)]` → `SessionListQuery` DTO.
8. Dedupe `CountRow`.
Verify: tests for `split_by_local_day`, clipping on fixture sessions (in-memory SQLite via `migrate`); Statistics and History totals agree in the app.
