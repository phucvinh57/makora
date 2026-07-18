use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::sqlite::SharedConnection;

/// A user-created focus block: a time window plus what the user intended to
/// do with it (a name, a planned duration, and which categories count as
/// "on-task"). `end_time` is always set — start + target at creation, so the
/// session "auto-ends" by simply reaching it — and is rewritten by an early
/// stop or a manual edit. `target_seconds` keeps the original plan for
/// comparing planned vs. actual after edits.
#[derive(Debug, Clone, Serialize)]
pub struct WorkSession {
    pub id: i64,
    pub name: String,
    pub note: Option<String>,
    pub target_seconds: i64,
    pub start_time: String,
    pub end_time: String,
    pub created_at: String,
    pub category_ids: Vec<i64>,
}

/// A full rewrite of a work session, as sent by the edit dialog — every
/// field is replaced, including the target-category set.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkSessionUpdate {
    pub id: i64,
    pub name: String,
    pub note: Option<String>,
    pub target_seconds: i64,
    /// RFC 3339 UTC.
    pub start_time: String,
    pub end_time: String,
    pub category_ids: Vec<i64>,
}

/// How the tracked history overlapping a work session's window breaks down
/// against its target categories. Computed at read time from `sessions`
/// rows, so category/rule edits after the fact re-evaluate past work
/// sessions too.
#[derive(Debug, Clone, Serialize)]
pub struct WorkSessionStats {
    pub window_start: String,
    /// The analyzed window's end: `min(end_time, now)`, so an in-progress
    /// session is only judged on the time that has actually elapsed.
    pub window_end: String,
    pub in_progress: bool,
    pub on_task_seconds: f64,
    pub off_task_seconds: f64,
    /// Window time with no tracked session at all (machine idle, tracker
    /// off). Counts toward neither on- nor off-task, but
    /// `on + off + idle` always equals the window length so the timeline
    /// strip can render edge to edge.
    pub idle_seconds: f64,
    /// `on / (on + off)` as 0–100; 0 when nothing was tracked.
    pub focus_percent: f64,
    /// Transitions from an on-task block to an off-task one. Idle gaps are
    /// skipped: stepping away from the machine between two on-task blocks
    /// isn't a context switch, coming back to a distraction is.
    pub switch_count: i64,
    pub apps: Vec<WorkSessionAppStat>,
    pub timeline: Vec<TimelineBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkSessionAppStat {
    /// Domain for browser-tab keys, exe file stem otherwise — same labeling
    /// convention as `HistoryReader::activity_stats`' top-apps breakdown.
    pub label: String,
    /// `"exe"` or `"domain"` — which kind of key `label` came from.
    pub kind: String,
    pub on_task: bool,
    pub category_color: Option<String>,
    pub total_seconds: f64,
}

/// One contiguous stretch of the analyzed window, clipped to it.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineBlock {
    pub start_time: String,
    pub end_time: String,
    /// Empty for `"idle"` blocks.
    pub label: String,
    /// `"on_task"`, `"off_task"`, or `"idle"`.
    pub kind: String,
    pub category_color: Option<String>,
}

/// One tracked `sessions` row overlapping the analyzed window, already
/// clipped to it and tagged with whether its resolved category is one of the
/// work session's targets.
struct ClippedRow {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    label: String,
    key_kind: String,
    on_task: bool,
    color: Option<String>,
}

/// Adjacent tracked rows closer together than this merge into one timeline
/// block (a browser closing one tab row and opening the next produces
/// back-to-back rows, not a real gap), and gaps shorter than this don't
/// become `idle` blocks.
const GAP_TOLERANCE_SECONDS: f64 = 1.0;

/// Storage for user-created work sessions and the read-time evaluation of
/// tracked history against them. Shares the process-wide connection like
/// every other store — SQLite serializes writes at the file level, so a
/// separate connection would add `SQLITE_BUSY` risk, not concurrency.
pub struct WorkSessionStore {
    conn: SharedConnection,
}

impl WorkSessionStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    /// `start_time: None` starts the session now. `end_time` is derived as
    /// start + target — there is no timer anywhere; reaching that instant
    /// *is* the auto-end.
    pub fn create(
        &self,
        name: &str,
        note: Option<&str>,
        target_seconds: i64,
        start_time: Option<DateTime<Utc>>,
        category_ids: &[i64],
    ) -> Result<WorkSession> {
        let now = Utc::now();
        let start = start_time.unwrap_or(now);
        let end = start + Duration::seconds(target_seconds);
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO work_sessions (name, note, target_seconds, start_time, end_time, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                name,
                note,
                target_seconds,
                start.to_rfc3339(),
                end.to_rfc3339(),
                now.to_rfc3339()
            ],
        )?;
        let id = tx.last_insert_rowid();
        for category_id in category_ids {
            tx.execute(
                "INSERT OR IGNORE INTO work_session_categories (work_session_id, category_id) VALUES (?1, ?2)",
                params![id, category_id],
            )?;
        }
        tx.commit()?;
        Ok(WorkSession {
            id,
            name: name.to_string(),
            note: note.map(str::to_string),
            target_seconds,
            start_time: start.to_rfc3339(),
            end_time: end.to_rfc3339(),
            created_at: now.to_rfc3339(),
            category_ids: category_ids.to_vec(),
        })
    }

    /// Newest first. The per-session category-id lookup is a second query
    /// per row, which is fine at "a handful of work sessions a day" scale.
    pub fn list(&self) -> Result<Vec<WorkSession>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, note, target_seconds, start_time, end_time, created_at
             FROM work_sessions ORDER BY start_time DESC, id DESC",
        )?;
        let mut sessions = stmt
            .query_map([], |row| {
                Ok(WorkSession {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    note: row.get(2)?,
                    target_seconds: row.get(3)?,
                    start_time: row.get(4)?,
                    end_time: row.get(5)?,
                    created_at: row.get(6)?,
                    category_ids: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut ids_stmt = conn.prepare(
            "SELECT category_id FROM work_session_categories WHERE work_session_id = ?1 ORDER BY category_id",
        )?;
        for session in &mut sessions {
            session.category_ids = ids_stmt
                .query_map(params![session.id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(sessions)
    }

    /// Rewrites every field, replacing the target-category set wholesale.
    /// Editing is unrestricted by design — the user owns their own plan.
    pub fn update(&self, update: &WorkSessionUpdate) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let updated = tx.execute(
            "UPDATE work_sessions
             SET name = ?1, note = ?2, target_seconds = ?3, start_time = ?4, end_time = ?5
             WHERE id = ?6",
            params![
                update.name,
                update.note,
                update.target_seconds,
                update.start_time,
                update.end_time,
                update.id
            ],
        )?;
        if updated == 0 {
            bail!("work session {} not found", update.id);
        }
        tx.execute(
            "DELETE FROM work_session_categories WHERE work_session_id = ?1",
            params![update.id],
        )?;
        for category_id in &update.category_ids {
            tx.execute(
                "INSERT OR IGNORE INTO work_session_categories (work_session_id, category_id) VALUES (?1, ?2)",
                params![update.id, category_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Ends an in-progress session now. A session whose window has already
    /// passed is left alone — its end already happened.
    pub fn stop(&self, id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.lock().unwrap().execute(
            "UPDATE work_sessions SET end_time = ?1
             WHERE id = ?2 AND julianday(end_time) > julianday(?1)",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM work_session_categories WHERE work_session_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM work_sessions WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn stats(&self, id: i64) -> Result<WorkSessionStats> {
        self.stats_at(id, Utc::now())
    }

    /// `now` is injected so tests can pin an "in-progress" moment; `stats`
    /// is the production entry point.
    fn stats_at(&self, id: i64, now: DateTime<Utc>) -> Result<WorkSessionStats> {
        let conn = self.conn.lock().unwrap();
        let (start_str, end_str): (String, String) = conn.query_row(
            "SELECT start_time, end_time FROM work_sessions WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let start = parse_utc(&start_str)?;
        let end = parse_utc(&end_str)?;
        // Judge an in-progress session only on elapsed time; a session
        // scheduled in the future has an empty window.
        let window_end = end.min(now).max(start);
        let in_progress = now >= start && now < end;

        // The category-resolution join stack is exactly the one
        // `HistoryReader::list_sessions` uses (pin → key-wide override →
        // best-matching rule), so on/off-task here always agrees with the
        // category shown for the same row on the History page. Overlap
        // filtering goes through `julianday` rather than string comparison:
        // stored timestamps mix `+00:00`/`Z` suffixes and fractional-digit
        // counts, which don't order lexicographically. A still-open row
        // participates as if it ended `now`.
        let sql = format!(
            "SELECT s.start_time, s.end_time, s.app_name, s.category_key, s.category_key_kind,
                    cat.color,
                    (cat.id IS NOT NULL AND cat.id IN
                        (SELECT category_id FROM work_session_categories WHERE work_session_id = ?1))
             FROM sessions s
             LEFT JOIN category_overrides ov
                    ON ov.key_kind = s.category_key_kind AND ov.pattern = s.category_key
             {rule_join}
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(s.category_id, ov.category_id, r.category_id)
             WHERE julianday(COALESCE(s.end_time, ?4)) > julianday(?2)
               AND julianday(s.start_time) < julianday(?3)
             ORDER BY s.start_time ASC",
            rule_join = crate::core::category::rule_join_sql("r", "s"),
        );
        let mut stmt = conn.prepare(&sql)?;
        let raw_rows = stmt
            .query_map(
                params![
                    id,
                    start.to_rfc3339(),
                    window_end.to_rfc3339(),
                    now.to_rfc3339()
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, bool>(6)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        drop(conn);

        let mut rows = Vec::with_capacity(raw_rows.len());
        for (row_start, row_end, app_name, key, key_kind, color, on_task) in raw_rows {
            let row_start = parse_utc(&row_start)?;
            let row_end = match row_end {
                Some(t) => parse_utc(&t)?,
                None => now,
            };
            let clip_start = row_start.max(start);
            let clip_end = row_end.min(window_end);
            if clip_end <= clip_start {
                continue;
            }
            // Same labeling as `activity_stats`' top-apps breakdown: the
            // domain for a browser-tab row, the app otherwise.
            let label = if key_kind == "domain" { key } else { app_name };
            rows.push(ClippedRow {
                start: clip_start,
                end: clip_end,
                label,
                key_kind,
                on_task,
                color,
            });
        }

        Ok(compute_stats(&start, &window_end, in_progress, &rows))
    }
}

/// Stored timestamps are chrono `to_rfc3339` (`+00:00`) but frontend-edited
/// ones may carry a `Z` suffix; `parse_from_rfc3339` accepts both.
fn parse_utc(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .with_context(|| format!("invalid timestamp: {value}"))
}

fn seconds_between(start: &DateTime<Utc>, end: &DateTime<Utc>) -> f64 {
    (*end - *start).num_milliseconds() as f64 / 1000.0
}

fn compute_stats(
    window_start: &DateTime<Utc>,
    window_end: &DateTime<Utc>,
    in_progress: bool,
    rows: &[ClippedRow],
) -> WorkSessionStats {
    // Merge back-to-back rows with the same label and verdict (e.g. a run
    // of tab rows on one site) into single timeline blocks.
    struct Block {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        label: String,
        on_task: bool,
        color: Option<String>,
    }
    let mut blocks: Vec<Block> = Vec::new();
    for row in rows {
        if let Some(last) = blocks.last_mut() {
            if last.on_task == row.on_task
                && last.label == row.label
                && seconds_between(&last.end, &row.start) <= GAP_TOLERANCE_SECONDS
            {
                last.end = last.end.max(row.end);
                continue;
            }
        }
        blocks.push(Block {
            start: row.start,
            end: row.end,
            label: row.label.clone(),
            on_task: row.on_task,
            color: row.color.clone(),
        });
    }

    let mut on_task_seconds = 0.0;
    let mut off_task_seconds = 0.0;
    let mut switch_count = 0i64;
    let mut previous_on_task: Option<bool> = None;
    let mut timeline: Vec<TimelineBlock> = Vec::new();
    let mut cursor = *window_start;
    for block in &blocks {
        if seconds_between(&cursor, &block.start) > GAP_TOLERANCE_SECONDS {
            timeline.push(idle_block(&cursor, &block.start));
        }
        let duration = seconds_between(&block.start, &block.end);
        if block.on_task {
            on_task_seconds += duration;
        } else {
            off_task_seconds += duration;
        }
        if previous_on_task == Some(true) && !block.on_task {
            switch_count += 1;
        }
        previous_on_task = Some(block.on_task);
        timeline.push(TimelineBlock {
            start_time: block.start.to_rfc3339(),
            end_time: block.end.to_rfc3339(),
            label: block.label.clone(),
            kind: if block.on_task { "on_task" } else { "off_task" }.to_string(),
            category_color: block.color.clone(),
        });
        cursor = cursor.max(block.end);
    }
    if seconds_between(&cursor, window_end) > GAP_TOLERANCE_SECONDS {
        timeline.push(idle_block(&cursor, window_end));
    }

    // Derive idle from the remainder rather than summing idle blocks, so
    // on + off + idle equals the window length exactly even though
    // sub-tolerance gaps never became blocks.
    let window_seconds = seconds_between(window_start, window_end);
    let idle_seconds = (window_seconds - on_task_seconds - off_task_seconds).max(0.0);
    let tracked = on_task_seconds + off_task_seconds;
    let focus_percent = if tracked > 0.0 {
        on_task_seconds / tracked * 100.0
    } else {
        0.0
    };

    // Per-app rollup. Keyed by (key kind, label, verdict) rather than key
    // alone: a per-session pin can put one row of an app on-task while the
    // rest stay off, and folding those together would hide it.
    let mut app_totals: Vec<WorkSessionAppStat> = Vec::new();
    for row in rows {
        let duration = seconds_between(&row.start, &row.end);
        match app_totals
            .iter_mut()
            .find(|a| a.kind == row.key_kind && a.on_task == row.on_task && a.label == row.label)
        {
            Some(stat) => stat.total_seconds += duration,
            None => app_totals.push(WorkSessionAppStat {
                label: row.label.clone(),
                kind: row.key_kind.clone(),
                on_task: row.on_task,
                category_color: row.color.clone(),
                total_seconds: duration,
            }),
        }
    }
    app_totals.sort_by(|a, b| b.total_seconds.total_cmp(&a.total_seconds));

    WorkSessionStats {
        window_start: window_start.to_rfc3339(),
        window_end: window_end.to_rfc3339(),
        in_progress,
        on_task_seconds,
        off_task_seconds,
        idle_seconds,
        focus_percent,
        switch_count,
        apps: app_totals,
        timeline,
    }
}

fn idle_block(start: &DateTime<Utc>, end: &DateTime<Utc>) -> TimelineBlock {
    TimelineBlock {
        start_time: start.to_rfc3339(),
        end_time: end.to_rfc3339(),
        label: String::new(),
        kind: "idle".to_string(),
        category_color: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::categories::CategoryStore;
    use crate::core::store::sqlite::open_connection;

    fn temp_store() -> (WorkSessionStore, CategoryStore, std::path::PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64;
        let path = std::env::temp_dir().join(format!("makora-work-session-test-{unique}.sqlite"));
        let conn = open_connection(&path).expect("opening test database");
        (
            WorkSessionStore::new(conn.clone()),
            CategoryStore::new(conn),
            path,
        )
    }

    fn insert_tracked(
        store: &WorkSessionStore,
        exe: &str,
        start_time: &str,
        end_time: Option<&str>,
    ) -> i64 {
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time, end_time)
             VALUES (?1, ?1, 'title', ?1, 'exe', ?2, ?3)",
            params![exe, start_time, end_time],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn utc(value: &str) -> DateTime<Utc> {
        parse_utc(value).unwrap()
    }

    /// Creates a work session over a fixed window by creating and then
    /// editing it, since `create` derives its window from "now".
    fn fixed_window_session(
        store: &WorkSessionStore,
        start: &str,
        end: &str,
        category_ids: &[i64],
    ) -> i64 {
        let created = store
            .create("Deep work", None, 3600, None, category_ids)
            .unwrap();
        store
            .update(&WorkSessionUpdate {
                id: created.id,
                name: "Deep work".to_string(),
                note: None,
                target_seconds: 3600,
                start_time: start.to_string(),
                end_time: end.to_string(),
                category_ids: category_ids.to_vec(),
            })
            .unwrap();
        created.id
    }

    #[test]
    fn create_derives_end_time_from_target_and_round_trips_categories() {
        let (store, categories, path) = temp_store();
        let cat = categories.create_category("Focus", "#123456").unwrap();

        let created = store
            .create("Ship feature", Some("the release"), 1800, None, &[cat.id])
            .unwrap();
        assert_eq!(
            utc(&created.end_time) - utc(&created.start_time),
            Duration::seconds(1800)
        );

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Ship feature");
        assert_eq!(listed[0].note.as_deref(), Some("the release"));
        assert_eq!(listed[0].category_ids, vec![cat.id]);

        store.delete(created.id).unwrap();
        assert!(store.list().unwrap().is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn straddling_tracked_row_is_clipped_to_the_window() {
        let (store, categories, path) = temp_store();
        let cat = categories.create_category("Focus", "#123456").unwrap();
        categories
            .create_rule("exe", "editor64.exe", "exact", cat.id)
            .unwrap();
        let id = fixed_window_session(
            &store,
            "2024-06-15T10:00:00+00:00",
            "2024-06-15T10:10:00+00:00",
            &[cat.id],
        );
        // Tracked row runs 09:55–10:20; only 10:00–10:10 lies inside.
        insert_tracked(
            &store,
            "editor64.exe",
            "2024-06-15T09:55:00+00:00",
            Some("2024-06-15T10:20:00+00:00"),
        );

        let stats = store.stats_at(id, utc("2024-06-16T00:00:00Z")).unwrap();
        assert_eq!(stats.on_task_seconds, 600.0);
        assert_eq!(stats.off_task_seconds, 0.0);
        assert_eq!(stats.idle_seconds, 0.0);
        assert!(!stats.in_progress);
        assert_eq!(stats.timeline.len(), 1);
        assert_eq!(stats.timeline[0].kind, "on_task");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn on_task_resolution_uses_rules_and_row_pins_beat_them() {
        let (store, categories, path) = temp_store();
        let focus = categories.create_category("Focus", "#123456").unwrap();
        let other = categories.create_category("Other", "#654321").unwrap();
        categories
            .create_rule("exe", "chat64.exe", "exact", other.id)
            .unwrap();
        let id = fixed_window_session(
            &store,
            "2024-06-15T10:00:00+00:00",
            "2024-06-15T10:10:00+00:00",
            &[focus.id],
        );
        // Rule puts chat64.exe in Other (off-task)…
        let pinned = insert_tracked(
            &store,
            "chat64.exe",
            "2024-06-15T10:00:00+00:00",
            Some("2024-06-15T10:04:00+00:00"),
        );
        insert_tracked(
            &store,
            "chat64.exe",
            "2024-06-15T10:04:00+00:00",
            Some("2024-06-15T10:10:00+00:00"),
        );
        // …but a per-row pin overrides the rule for the first row only.
        categories.set_row_category(pinned, Some(focus.id)).unwrap();

        let stats = store.stats_at(id, utc("2024-06-16T00:00:00Z")).unwrap();
        assert_eq!(stats.on_task_seconds, 240.0);
        assert_eq!(stats.off_task_seconds, 360.0);
        // Same app, split verdicts — the rollup keeps them apart.
        assert_eq!(stats.apps.len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn switch_count_ignores_idle_gaps_between_on_task_blocks() {
        let (store, categories, path) = temp_store();
        let focus = categories.create_category("Focus", "#123456").unwrap();
        categories
            .create_rule("exe", "editor64.exe", "exact", focus.id)
            .unwrap();
        let id = fixed_window_session(
            &store,
            "2024-06-15T10:00:00+00:00",
            "2024-06-15T10:30:00+00:00",
            &[focus.id],
        );
        // on → off → on → off = 2 switches…
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:00:00+00:00", Some("2024-06-15T10:05:00+00:00"));
        insert_tracked(&store, "surf.exe", "2024-06-15T10:05:00+00:00", Some("2024-06-15T10:08:00+00:00"));
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:08:00+00:00", Some("2024-06-15T10:12:00+00:00"));
        insert_tracked(&store, "surf.exe", "2024-06-15T10:12:00+00:00", Some("2024-06-15T10:14:00+00:00"));
        // …then an idle gap between two on-task blocks adds none.
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:14:00+00:00", Some("2024-06-15T10:16:00+00:00"));
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:20:00+00:00", Some("2024-06-15T10:25:00+00:00"));

        let stats = store.stats_at(id, utc("2024-06-16T00:00:00Z")).unwrap();
        assert_eq!(stats.switch_count, 2);
        // 10:16–10:20 idle gap plus the untracked 10:25–10:30 tail.
        assert_eq!(
            stats.timeline.iter().filter(|b| b.kind == "idle").count(),
            2
        );
        // Idle is the window remainder: 30min − 16min on − 5min off = 9min.
        assert_eq!(stats.idle_seconds, 540.0);
        assert_eq!(
            stats.on_task_seconds + stats.off_task_seconds + stats.idle_seconds,
            1800.0
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn in_progress_session_clamps_window_to_now_and_counts_the_open_row() {
        let (store, categories, path) = temp_store();
        let focus = categories.create_category("Focus", "#123456").unwrap();
        categories
            .create_rule("exe", "editor64.exe", "exact", focus.id)
            .unwrap();
        let id = fixed_window_session(
            &store,
            "2024-06-15T10:00:00+00:00",
            "2024-06-15T11:00:00+00:00",
            &[focus.id],
        );
        // Still-open row, focused since 10:00; "now" is 10:15.
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:00:00+00:00", None);

        let stats = store.stats_at(id, utc("2024-06-15T10:15:00Z")).unwrap();
        assert!(stats.in_progress);
        assert_eq!(stats.window_end, utc("2024-06-15T10:15:00Z").to_rfc3339());
        assert_eq!(stats.on_task_seconds, 900.0);
        assert_eq!(stats.idle_seconds, 0.0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn consecutive_rows_of_the_same_app_merge_into_one_timeline_block() {
        let (store, categories, path) = temp_store();
        let focus = categories.create_category("Focus", "#123456").unwrap();
        categories
            .create_rule("exe", "editor64.exe", "exact", focus.id)
            .unwrap();
        let id = fixed_window_session(
            &store,
            "2024-06-15T10:00:00+00:00",
            "2024-06-15T10:10:00+00:00",
            &[focus.id],
        );
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:00:00+00:00", Some("2024-06-15T10:03:00+00:00"));
        insert_tracked(&store, "editor64.exe", "2024-06-15T10:03:00+00:00", Some("2024-06-15T10:10:00+00:00"));

        let stats = store.stats_at(id, utc("2024-06-16T00:00:00Z")).unwrap();
        assert_eq!(stats.timeline.len(), 1);
        assert_eq!(stats.timeline[0].kind, "on_task");
        assert_eq!(stats.on_task_seconds, 600.0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stop_ends_an_in_progress_session_and_leaves_past_ones_alone() {
        let (store, _categories, path) = temp_store();
        // In progress: created now with an hour target.
        let active = store.create("Active", None, 3600, None, &[]).unwrap();
        store.stop(active.id).unwrap();
        let stopped = store
            .list()
            .unwrap()
            .into_iter()
            .find(|s| s.id == active.id)
            .unwrap();
        assert!(utc(&stopped.end_time) < utc(&active.end_time));

        // Already over: end_time stays untouched.
        let past = store.create("Past", None, 3600, None, &[]).unwrap();
        store
            .update(&WorkSessionUpdate {
                id: past.id,
                name: "Past".to_string(),
                note: None,
                target_seconds: 3600,
                start_time: "2024-06-15T10:00:00+00:00".to_string(),
                end_time: "2024-06-15T11:00:00+00:00".to_string(),
                category_ids: Vec::new(),
            })
            .unwrap();
        store.stop(past.id).unwrap();
        let unchanged = store
            .list()
            .unwrap()
            .into_iter()
            .find(|s| s.id == past.id)
            .unwrap();
        assert_eq!(unchanged.end_time, "2024-06-15T11:00:00+00:00");

        let _ = std::fs::remove_file(&path);
    }
}
