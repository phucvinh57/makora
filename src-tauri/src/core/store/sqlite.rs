use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};

use crate::core::category::{category_key, url_key, KeyKind};
use crate::core::model::AppInfo;
use crate::core::store::SessionStore;
use crate::core::title::{canonical_title, title_quality};

pub type SharedConnection = Arc<Mutex<Connection>>;

/// The schema as of the last of the 28 migrations this replaced. A database that already ran
/// them is adopted by [`migrate`] rather than rebuilt.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        "CREATE TABLE categories (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            color       TEXT NOT NULL,
            parent_id   INTEGER REFERENCES categories(id),
            description TEXT
        );
        CREATE UNIQUE INDEX idx_categories_name ON categories(COALESCE(parent_id, 0), name);
        CREATE INDEX idx_categories_parent ON categories(parent_id);

        CREATE TABLE activities (
            id            INTEGER PRIMARY KEY,
            key_kind      TEXT NOT NULL CHECK(key_kind IN ('exe','domain')),
            key           TEXT NOT NULL,
            url_key       TEXT,
            title         TEXT NOT NULL DEFAULT '',
            display_title TEXT NOT NULL DEFAULT '',
            app_name      TEXT NOT NULL DEFAULT '',
            url           TEXT,
            first_seen    TEXT NOT NULL,
            last_seen     TEXT NOT NULL,
            category_id   INTEGER REFERENCES categories(id)
        );
        CREATE UNIQUE INDEX idx_activities_identity
            ON activities(key_kind, key, COALESCE(url_key, ''), title);
        CREATE INDEX idx_activities_key ON activities(key_kind, key);
        CREATE INDEX idx_activities_category ON activities(category_id);

        CREATE TABLE sessions (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            app_name          TEXT NOT NULL,
            exe_path          TEXT NOT NULL,
            window_title      TEXT NOT NULL,
            url               TEXT,
            category_key      TEXT NOT NULL DEFAULT '',
            category_key_kind TEXT NOT NULL DEFAULT 'exe',
            start_time        TEXT NOT NULL,
            end_time          TEXT,
            category_id       INTEGER REFERENCES categories(id),
            url_key           TEXT,
            activity_id       INTEGER REFERENCES activities(id)
        );
        CREATE INDEX idx_sessions_exe_path ON sessions(exe_path);
        CREATE INDEX idx_sessions_start_time ON sessions(start_time);
        CREATE INDEX idx_sessions_category_key ON sessions(category_key_kind, category_key);
        CREATE INDEX idx_sessions_category ON sessions(category_id);
        CREATE INDEX idx_sessions_activity ON sessions(activity_id);

        CREATE TABLE work_sessions (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            name           TEXT NOT NULL,
            note           TEXT,
            target_seconds INTEGER NOT NULL,
            start_time     TEXT NOT NULL,
            end_time       TEXT NOT NULL,
            created_at     TEXT NOT NULL
        );
        CREATE INDEX idx_work_sessions_start_time ON work_sessions(start_time);
        CREATE TABLE work_session_categories (
            work_session_id INTEGER NOT NULL REFERENCES work_sessions(id),
            category_id     INTEGER NOT NULL REFERENCES categories(id),
            PRIMARY KEY (work_session_id, category_id)
        );",
    )])
}

/// A focus or tab change that is undone within a second never was activity: a page redirects,
/// a browser reports a tab before its title has loaded and corrects itself, a window flashes
/// past on the way somewhere else. Rows that short are deleted instead of closed, so no query
/// downstream has to filter by duration.
const MIN_SESSION_SECONDS: i64 = 1;
const MERGE_GAP_SECONDS: i64 = 2;

pub fn open_connection(path: &Path) -> Result<SharedConnection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating database directory {}", parent.display()))?;
    }
    let mut conn = Connection::open(path)
        .with_context(|| format!("opening database at {}", path.display()))?;
    migrate(&mut conn).context("migrating schema")?;
    Ok(Arc::new(Mutex::new(conn)))
}

/// `user_version` the 28 migrations this schema replaced ended at. Such a database already
/// has the squashed schema, so it is stamped as migrated instead of being run again.
const LEGACY_SCHEMA_VERSION: i64 = 28;

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == LEGACY_SCHEMA_VERSION {
        conn.execute_batch("DROP TABLE IF EXISTS settings;")?;
        conn.pragma_update(None, "user_version", 1)?;
    } else if version > 1 {
        anyhow::bail!(
            "database schema version {version} is not supported; \
             open it with an older release first, or delete it"
        );
    }
    migrations().to_latest(conn)?;
    Ok(())
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn upsert_activity(
    tx: &rusqlite::Transaction<'_>,
    key_kind: KeyKind,
    key: &str,
    url_key_value: Option<&str>,
    title: &str,
    display_title: &str,
    app_name: &str,
    url: Option<&str>,
    seen: &str,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO activities
            (key_kind, key, url_key, title, display_title, app_name, url, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT DO NOTHING",
        params![
            key_kind.as_str(),
            key,
            url_key_value,
            title,
            display_title,
            app_name,
            url,
            seen
        ],
    )?;
    let id: i64 = tx.query_row(
        "SELECT id FROM activities WHERE key_kind = ?1 AND key = ?2
         AND COALESCE(url_key, '') = COALESCE(?3, '') AND title = ?4",
        params![key_kind.as_str(), key, url_key_value, title],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE activities SET first_seen = MIN(first_seen, ?1), last_seen = MAX(last_seen, ?1),
                display_title = CASE WHEN length(?2) > length(display_title) THEN ?2 ELSE display_title END,
                app_name = CASE WHEN app_name = '' THEN ?3 ELSE app_name END,
                url = COALESCE(url, ?4)
         WHERE id = ?5",
        params![seen, display_title, app_name, url, id],
    )?;
    Ok(id)
}

/// A row whose `start_time` is missing or unparseable is left alone — an unreadable timestamp
/// is a reason to keep the row, not to throw it away.
fn is_too_short(tx: &rusqlite::Transaction<'_>, id: i64, at: DateTime<Utc>) -> Result<bool> {
    let start: Option<String> = tx
        .query_row(
            "SELECT start_time FROM sessions WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(start) = start.as_deref().and_then(|start| {
        DateTime::parse_from_rfc3339(start)
            .ok()
            .map(|start| start.with_timezone(&Utc))
    }) else {
        return Ok(false);
    };
    Ok(at.signed_duration_since(start).num_milliseconds() < MIN_SESSION_SECONDS * 1000)
}

pub struct SqliteStore {
    conn: SharedConnection,
}

impl SqliteStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }
}

impl SessionStore for SqliteStore {
    fn start_session(
        &mut self,
        app: &AppInfo,
        window_title: &str,
        url: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<i64> {
        let (key_kind, key) = category_key(&app.exe_path, url);
        let url_key_value = url.and_then(url_key);
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let canonical = canonical_title(window_title, &app.app_name, url);
        let activity_id = upsert_activity(
            &tx,
            key_kind,
            &key,
            url_key_value.as_deref(),
            &canonical,
            window_title,
            &app.app_name,
            url,
            &at.to_rfc3339(),
        )?;
        let previous: Option<(i64, Option<i64>, Option<String>)> = tx
            .query_row(
                "SELECT id, activity_id, end_time FROM sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((id, Some(previous_activity), Some(end_time))) = previous {
            let within_gap = parse_time(&end_time)
                .map(|end| at.signed_duration_since(end).num_milliseconds())
                .is_some_and(|gap| (0..=MERGE_GAP_SECONDS * 1000).contains(&gap));
            if previous_activity == activity_id && within_gap {
                tx.execute(
                    "UPDATE sessions SET end_time = NULL WHERE id = ?1",
                    params![id],
                )?;
                tx.commit()?;
                return Ok(id);
            }
        }
        tx.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key, start_time, activity_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![app.app_name, app.exe_path, window_title, url, key, key_kind.as_str(), url_key_value, at.to_rfc3339(), activity_id],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    fn refine_title(&mut self, id: i64, window_title: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let current: Option<(String, String, String, Option<String>, i64)> = tx
            .query_row(
                "SELECT s.window_title, s.app_name, s.exe_path, s.url, s.activity_id
                 FROM sessions s WHERE s.id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((current_title, app_name, exe_path, url, old_activity_id)) = current else {
            tx.commit()?;
            return Ok(());
        };
        if title_quality(window_title, &app_name, url.as_deref())
            <= title_quality(&current_title, &app_name, url.as_deref())
        {
            tx.commit()?;
            return Ok(());
        }
        let (key_kind, key) = category_key(&exe_path, url.as_deref());
        let canonical = canonical_title(window_title, &app_name, url.as_deref());
        let target_id = upsert_activity(
            &tx,
            key_kind,
            &key,
            url.as_deref().and_then(url_key).as_deref(),
            &canonical,
            window_title,
            &app_name,
            url.as_deref(),
            &Utc::now().to_rfc3339(),
        )?;
        if target_id != old_activity_id {
            let old_pin: Option<i64> = tx.query_row(
                "SELECT category_id FROM activities WHERE id = ?1",
                params![old_activity_id],
                |row| row.get(0),
            )?;
            let target_pin: Option<i64> = tx.query_row(
                "SELECT category_id FROM activities WHERE id = ?1",
                params![target_id],
                |row| row.get(0),
            )?;
            if old_pin.is_some() && target_pin.is_some() && old_pin != target_pin {
                tx.commit()?;
                return Ok(());
            }
            if let Some(old_pin) = old_pin {
                tx.execute(
                    "UPDATE activities SET category_id = COALESCE(category_id, ?1) WHERE id = ?2",
                    params![old_pin, target_id],
                )?;
            }
            tx.execute(
                "UPDATE sessions SET window_title = ?1, activity_id = ?2 WHERE id = ?3",
                params![window_title, target_id, id],
            )?;
            prune_orphan_activity(&tx, old_activity_id)?;
        } else {
            tx.execute(
                "UPDATE sessions SET window_title = ?1 WHERE id = ?2",
                params![window_title, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Closes the row and derives its category in Rust in one transaction, unless the block
    /// lasted less than [`MIN_SESSION_SECONDS`], in which case the row is dropped instead.
    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()> {
        let end_time = at.to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if is_too_short(&tx, id, at)? {
            let old_activity: Option<i64> = tx
                .query_row(
                    "SELECT activity_id FROM sessions WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;
            tx.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
            if let Some(old_activity) = old_activity {
                prune_orphan_activity(&tx, old_activity)?;
            }
            tx.commit()?;
            return Ok(());
        }
        tx.execute(
            "UPDATE sessions SET end_time = ?1 WHERE id = ?2",
            params![end_time, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn close_dangling_sessions(&mut self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let ids: Vec<i64> = tx
            .prepare("SELECT DISTINCT activity_id FROM sessions WHERE end_time IS NULL AND activity_id IS NOT NULL")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tx.execute("DELETE FROM sessions WHERE end_time IS NULL", [])?;
        for id in ids {
            prune_orphan_activity(&tx, id)?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn prune_orphan_activity(tx: &rusqlite::Transaction<'_>, id: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM activities WHERE id = ?1 AND category_id IS NULL
         AND NOT EXISTS (SELECT 1 FROM sessions WHERE activity_id = ?1)",
        params![id],
    )?;
    Ok(())
}
