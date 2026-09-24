use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::sqlite::SharedConnection;

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

/// Fields for a work session that does not exist yet. `start_time` defaults to now.
#[derive(Debug, Clone)]
pub struct NewWorkSession {
    pub name: String,
    pub note: Option<String>,
    pub target_seconds: i64,
    pub start_time: Option<DateTime<Utc>>,
    pub category_ids: Vec<i64>,
}

/// Every editable field of an existing work session.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkSessionFields {
    pub name: String,
    pub note: Option<String>,
    pub target_seconds: i64,
    pub start_time: String,
    pub end_time: String,
    pub category_ids: Vec<i64>,
}

/// The wire shape of an edit: which session, plus its new fields.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkSessionUpdate {
    pub id: i64,
    #[serde(flatten)]
    pub fields: WorkSessionFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkSessionStats {
    pub window_start: String,
    pub window_end: String,
    pub in_progress: bool,
    pub on_task_seconds: f64,
    pub off_task_seconds: f64,
    pub idle_seconds: f64,
    pub focus_percent: f64,
    pub switch_count: i64,
    pub apps: Vec<WorkSessionAppStat>,
    pub timeline: Vec<TimelineBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkSessionAppStat {
    pub label: String,
    pub kind: String,
    pub on_task: bool,
    pub category_color: Option<String>,
    pub total_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineBlock {
    pub start_time: String,
    pub end_time: String,
    pub label: String,
    pub kind: String,
    pub category_color: Option<String>,
}

struct ClippedRow {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    label: String,
    key_kind: String,
    on_task: bool,
    color: Option<String>,
}

const GAP_TOLERANCE_SECONDS: f64 = 1.0;

pub struct WorkSessionStore {
    conn: SharedConnection,
}

impl WorkSessionStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    pub fn create(&self, new: &NewWorkSession) -> Result<WorkSession> {
        let NewWorkSession {
            name,
            note,
            target_seconds,
            start_time,
            category_ids,
        } = new;
        let target_seconds = *target_seconds;
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
            name: name.clone(),
            note: note.clone(),
            target_seconds,
            start_time: start.to_rfc3339(),
            end_time: end.to_rfc3339(),
            created_at: now.to_rfc3339(),
            category_ids: category_ids.clone(),
        })
    }

    pub fn list(&self) -> Result<Vec<WorkSession>> {
        self.query(None, -1, 0)
    }

    pub fn search(
        &self,
        search: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<WorkSession>> {
        self.query(
            search.map(str::trim).filter(|s| !s.is_empty()),
            limit.max(1),
            offset.max(0),
        )
    }

    pub fn get(&self, id: i64) -> Result<Option<WorkSession>> {
        let conn = self.conn.lock().unwrap();
        let mut sessions = Self::select(&conn, None, Some(id), -1, 0)?;
        if sessions.is_empty() {
            return Ok(None);
        }
        Self::attach_category_ids(&conn, &mut sessions)?;
        Ok(sessions.pop())
    }

    fn query(&self, search: Option<&str>, limit: i64, offset: i64) -> Result<Vec<WorkSession>> {
        let conn = self.conn.lock().unwrap();
        let mut sessions = Self::select(&conn, search, None, limit, offset)?;
        Self::attach_category_ids(&conn, &mut sessions)?;
        Ok(sessions)
    }

    fn select(
        conn: &rusqlite::Connection,
        search: Option<&str>,
        id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<WorkSession>> {
        let mut clauses = vec!["1=1".to_string()];
        let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(term) = search {
            clauses.push("(name LIKE ? ESCAPE '\\' OR note LIKE ? ESCAPE '\\')".to_string());
            let pattern = format!("%{}%", term.replace('%', "\\%").replace('_', "\\_"));
            binds.push(Box::new(pattern.clone()));
            binds.push(Box::new(pattern));
        }
        if let Some(id) = id {
            clauses.push("id = ?".to_string());
            binds.push(Box::new(id));
        }
        binds.push(Box::new(limit));
        binds.push(Box::new(offset));

        let sql = format!(
            "SELECT id, name, note, target_seconds, start_time, end_time, created_at
             FROM work_sessions
             WHERE {}
             ORDER BY start_time DESC, id DESC
             LIMIT ? OFFSET ?",
            clauses.join(" AND "),
        );
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            binds.iter().map(|b| b.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let sessions = stmt
            .query_map(param_refs.as_slice(), |row| {
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
        Ok(sessions)
    }

    fn attach_category_ids(
        conn: &rusqlite::Connection,
        sessions: &mut [WorkSession],
    ) -> Result<()> {
        let mut ids_stmt = conn.prepare(
            "SELECT category_id FROM work_session_categories WHERE work_session_id = ?1 ORDER BY category_id",
        )?;
        for session in sessions {
            session.category_ids = ids_stmt
                .query_map(params![session.id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(())
    }

    pub fn update(&self, id: i64, fields: &WorkSessionFields) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let updated = tx.execute(
            "UPDATE work_sessions
             SET name = ?1, note = ?2, target_seconds = ?3, start_time = ?4, end_time = ?5
             WHERE id = ?6",
            params![
                fields.name,
                fields.note,
                fields.target_seconds,
                fields.start_time,
                fields.end_time,
                id
            ],
        )?;
        if updated == 0 {
            bail!("work session {} not found", id);
        }
        tx.execute(
            "DELETE FROM work_session_categories WHERE work_session_id = ?1",
            params![id],
        )?;
        for category_id in &fields.category_ids {
            tx.execute(
                "INSERT OR IGNORE INTO work_session_categories (work_session_id, category_id) VALUES (?1, ?2)",
                params![id, category_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn stop(&self, id: i64) -> Result<()> {
        let now = Utc::now();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let end_time: Option<String> = tx
            .query_row(
                "SELECT end_time FROM work_sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if end_time
            .as_deref()
            .map(parse_utc)
            .transpose()?
            .is_some_and(|end| end > now)
        {
            tx.execute(
                "UPDATE work_sessions SET end_time = ?1 WHERE id = ?2",
                params![now.to_rfc3339(), id],
            )?;
        }
        tx.commit()?;
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

    fn stats_at(&self, id: i64, now: DateTime<Utc>) -> Result<WorkSessionStats> {
        let conn = self.conn.lock().unwrap();
        let (start_str, end_str): (String, String) = conn.query_row(
            "SELECT start_time, end_time FROM work_sessions WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let start = parse_utc(&start_str)?;
        let end = parse_utc(&end_str)?;
        let window_end = end.min(now).max(start);
        let in_progress = now >= start && now < end;

        let selected_category_ids = {
            let mut stmt = conn.prepare(
                "SELECT category_id FROM work_session_categories WHERE work_session_id = ?1",
            )?;
            let ids = stmt
                .query_map(params![id], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?;
            ids
        };
        let category_tree = super::categories::load_category_tree(&conn)?;
        let on_task_category_ids: std::collections::HashSet<i64> = selected_category_ids
            .into_iter()
            .flat_map(|category_id| category_tree.descendants(category_id))
            .collect();

        let sql =
            "SELECT s.start_time, s.end_time, s.app_name, s.category_key, s.category_key_kind,
                    cat.color, cat.id
             FROM sessions s
             LEFT JOIN activities a ON a.id = s.activity_id
             LEFT JOIN categories cat
                    ON cat.id = COALESCE(a.category_id, s.category_id)
             WHERE julianday(COALESCE(s.end_time, ?3)) > julianday(?1)
               AND julianday(s.start_time) < julianday(?2)
             ORDER BY s.start_time ASC";
        let mut stmt = conn.prepare(sql)?;
        let raw_rows = stmt
            .query_map(
                params![
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
                        row.get::<_, Option<i64>>(6)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        drop(conn);

        let mut rows = Vec::with_capacity(raw_rows.len());
        for (row_start, row_end, app_name, key, key_kind, color, category_id) in raw_rows {
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
            let label = if key_kind == "domain" { key } else { app_name };
            let on_task = category_id.is_some_and(|id| on_task_category_ids.contains(&id));
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

    let window_seconds = seconds_between(window_start, window_end);
    let idle_seconds = (window_seconds - on_task_seconds - off_task_seconds).max(0.0);
    let tracked = on_task_seconds + off_task_seconds;
    let focus_percent = if tracked > 0.0 {
        on_task_seconds / tracked * 100.0
    } else {
        0.0
    };

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
