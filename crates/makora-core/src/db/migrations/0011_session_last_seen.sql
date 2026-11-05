-- When an open session was last known to be running, refreshed by the tracker's heartbeat, so a
-- crash closes it there instead of losing it.
ALTER TABLE sessions ADD COLUMN last_seen TEXT;
UPDATE sessions SET last_seen = COALESCE(end_time, start_time);
