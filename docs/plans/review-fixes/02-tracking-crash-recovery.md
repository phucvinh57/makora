# 02-tracking-crash-recovery

Problem: `close_dangling` (`tracking/application.rs:135`, `db/sessions.rs:76`) deletes open rows; `start()` reopens merged rows (`end_time = NULL`) so a crash loses the whole merged span.
1. Migration `0011_session_last_seen.sql`: `ALTER TABLE sessions ADD COLUMN last_seen TEXT` (backfill `last_seen = COALESCE(end_time, start_time)`); update `schema.rs`, `models.rs`.
2. `SessionsRepo`: `insert`/`reopen` set `last_seen = at`; new `touch(id, at)`; replace `delete_open` with `close_open()` = `end_time = COALESCE(last_seen, start_time)` for open rows, then delete only rows that are noise by `is_noise` (reuse `session_policy::is_noise`).
3. Heartbeat: tracker loop in `lib.rs:169-186` switches `for event in rx` → `rx.recv_timeout(HEARTBEAT = 30s)`; on timeout call new `SessionTracker::heartbeat(now)` → `FocusStateMachine` → `recorder.touch(open_id, now)`.
4. Reporting treats open rows as running until `now` (see 09).
Verify: unit test close-dangling policy (pure fn deciding end time / noise); manual: start app, focus an app 2 min, `kill -9`, restart → row kept with end ≈ last heartbeat.
