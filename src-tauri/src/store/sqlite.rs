use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use rusqlite_migration::{Migrations, M};

use crate::category::{category_key, url_path, KeyKind};
use crate::focus::AppInfo;

use super::title::canonical_title;
use super::SessionStore;

pub type SharedConnection = Arc<Mutex<Connection>>;

/// The schema as of the last of the 28 migrations this replaced. A database that already ran
/// them is adopted by [`migrate`] rather than rebuilt.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
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
        ),
        // Category suggested by the local LLM. Kept apart from the manual pin
        // (`category_id`) so a pin always wins and AI output can be wiped without
        // touching the user's own choices.
        M::up(
            "ALTER TABLE activities ADD COLUMN ai_category_id INTEGER REFERENCES categories(id);
        CREATE INDEX idx_activities_ai_category ON activities(ai_category_id);",
        ),
        // Work sessions (the focus feature) were removed.
        M::up(
            "DROP TABLE work_session_categories;
        DROP TABLE work_sessions;",
        ),
        // The AI label a user rejected. Moved out of `ai_category_id` so reads stop showing it,
        // and kept so the model can be told "not this" next time.
        M::up("ALTER TABLE activities ADD COLUMN ai_rejected_category_id INTEGER REFERENCES categories(id);"),
        // How sure the model was of `ai_category_id`, from 0 to 1. NULL for labels made before
        // it was recorded; the review queue treats those as uncertain.
        M::up("ALTER TABLE activities ADD COLUMN ai_confidence REAL;"),
        // Normalizes to 3NF: `sources` pulls the app/site identity out of `activities` (it
        // depended on key_kind+key, not on the activity); `path` replaces `url`/`url_key` with
        // the one fact that isn't already in the source's key; `display_title` is dropped since
        // it never held more than the source name and URL already give; AI suggestion and
        // rejection move to `ai_labels`, one row per activity, since the two were never set at
        // once; and `sessions` keeps only the timestamps, since every other column it had was
        // copied from its activity.
        M::up(
            "CREATE TABLE sources (
            id   INTEGER PRIMARY KEY,
            kind TEXT NOT NULL CHECK(kind IN ('exe','domain')),
            key  TEXT NOT NULL,
            name TEXT NOT NULL,
            UNIQUE(kind, key)
        );
        INSERT INTO sources (kind, key, name)
        SELECT kind, key, name FROM (
            SELECT key_kind AS kind, key,
                   CASE WHEN key_kind = 'domain' THEN key ELSE app_name END AS name,
                   MAX(last_seen)
            FROM activities
            GROUP BY key_kind, key
        );

        UPDATE activities SET category_id = (
            SELECT category_id FROM sessions
            WHERE sessions.activity_id = activities.id AND category_id IS NOT NULL
            ORDER BY start_time DESC LIMIT 1
        )
        WHERE category_id IS NULL AND EXISTS (
            SELECT 1 FROM sessions
            WHERE sessions.activity_id = activities.id AND category_id IS NOT NULL
        );

        CREATE TABLE activities_new (
            id          INTEGER PRIMARY KEY,
            source_id   INTEGER NOT NULL REFERENCES sources(id),
            path        TEXT NOT NULL DEFAULT '',
            title       TEXT NOT NULL DEFAULT '',
            category_id INTEGER REFERENCES categories(id)
        );
        INSERT INTO activities_new (id, source_id, path, title, category_id)
        SELECT a.id, src.id,
               CASE WHEN a.url_key IS NOT NULL
                         AND substr(a.url_key, 1, length(src.key)) = src.key
                    THEN substr(a.url_key, length(src.key) + 1)
                    ELSE '' END,
               a.title, a.category_id
        FROM activities a
        JOIN sources src ON src.kind = a.key_kind AND src.key = a.key;

        CREATE TABLE ai_labels (
            activity_id INTEGER PRIMARY KEY REFERENCES activities(id),
            category_id INTEGER NOT NULL REFERENCES categories(id),
            confidence  REAL,
            rejected    INTEGER NOT NULL DEFAULT 0 CHECK(rejected IN (0, 1))
        );
        INSERT INTO ai_labels (activity_id, category_id, confidence, rejected)
        SELECT id, COALESCE(ai_category_id, ai_rejected_category_id), ai_confidence,
               CASE WHEN ai_category_id IS NULL THEN 1 ELSE 0 END
        FROM activities
        WHERE ai_category_id IS NOT NULL OR ai_rejected_category_id IS NOT NULL;

        CREATE TABLE sessions_new (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            activity_id INTEGER NOT NULL REFERENCES activities(id),
            start_time  TEXT NOT NULL,
            end_time    TEXT
        );
        INSERT INTO sessions_new (id, activity_id, start_time, end_time)
        SELECT id, activity_id, start_time, end_time FROM sessions
        WHERE activity_id IS NOT NULL;

        DROP TABLE sessions;
        DROP TABLE activities;
        ALTER TABLE sessions_new RENAME TO sessions;
        ALTER TABLE activities_new RENAME TO activities;

        CREATE UNIQUE INDEX idx_activities_identity ON activities(source_id, path, title);
        CREATE INDEX idx_activities_category ON activities(category_id);
        CREATE INDEX idx_ai_labels_category ON ai_labels(category_id);
        CREATE INDEX idx_sessions_start_time ON sessions(start_time);
        CREATE INDEX idx_sessions_activity ON sessions(activity_id);",
        ),
    ])
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
const LATEST_SCHEMA_VERSION: i64 = 6;

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == LEGACY_SCHEMA_VERSION {
        conn.execute_batch("DROP TABLE IF EXISTS settings;")?;
        conn.pragma_update(None, "user_version", 1)?;
    } else if version > LATEST_SCHEMA_VERSION {
        anyhow::bail!(
            "database schema version {version} is not supported; \
             open it with an older release first, or delete it"
        );
    }
    // `PRAGMA foreign_keys` is a no-op inside a transaction, and rusqlite_migration wraps each
    // migration in one, so it has to be toggled out here. Off because migration 6 drops a table
    // still referenced by a table it just created (the reference resolves once the replacement
    // is renamed into its place); on again before the connection does any real work.
    conn.pragma_update(None, "foreign_keys", false)?;
    let result = migrations().to_latest(conn);
    conn.pragma_update(None, "foreign_keys", true)?;
    result?;
    Ok(())
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn upsert_source(tx: &Transaction<'_>, kind: KeyKind, key: &str, name: &str) -> Result<i64> {
    tx.execute(
        "INSERT INTO sources (kind, key, name) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
        params![kind.as_str(), key, name],
    )?;
    let id: i64 = tx.query_row(
        "SELECT id FROM sources WHERE kind = ?1 AND key = ?2",
        params![kind.as_str(), key],
        |row| row.get(0),
    )?;
    if !name.is_empty() {
        tx.execute(
            "UPDATE sources SET name = ?1 WHERE id = ?2 AND name = ''",
            params![name, id],
        )?;
    }
    Ok(id)
}

fn upsert_activity(tx: &Transaction<'_>, source_id: i64, path: &str, title: &str) -> Result<i64> {
    tx.execute(
        "INSERT INTO activities (source_id, path, title) VALUES (?1, ?2, ?3)
         ON CONFLICT DO NOTHING",
        params![source_id, path, title],
    )?;
    let id: i64 = tx.query_row(
        "SELECT id FROM activities WHERE source_id = ?1 AND path = ?2 AND title = ?3",
        params![source_id, path, title],
        |row| row.get(0),
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

/// The source name: the domain itself for a browser tab, or the app's display name.
fn source_name(key_kind: KeyKind, key: &str, app_name: &str) -> String {
    match key_kind {
        KeyKind::Domain => key.to_string(),
        KeyKind::Exe => app_name.to_string(),
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
        let path = url.map(url_path).unwrap_or_default();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let canonical = canonical_title(window_title, &app.app_name, url);
        let source_id = upsert_source(&tx, key_kind, &key, &source_name(key_kind, &key, &app.app_name))?;
        let activity_id = upsert_activity(&tx, source_id, &path, &canonical)?;
        let previous: Option<(i64, i64, Option<String>)> = tx
            .query_row(
                "SELECT id, activity_id, end_time FROM sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((id, previous_activity, Some(end_time))) = previous {
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
            "INSERT INTO sessions (activity_id, start_time) VALUES (?1, ?2)",
            params![activity_id, at.to_rfc3339()],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    fn refine_title(
        &mut self,
        id: i64,
        app: &AppInfo,
        window_title: &str,
        url: Option<&str>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let old_activity_id: Option<i64> = tx
            .query_row(
                "SELECT activity_id FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(old_activity_id) = old_activity_id else {
            tx.commit()?;
            return Ok(());
        };
        let current_title: String = tx.query_row(
            "SELECT title FROM activities WHERE id = ?1",
            params![old_activity_id],
            |row| row.get(0),
        )?;
        let canonical = canonical_title(window_title, &app.app_name, url);
        if canonical.is_empty() || !current_title.is_empty() {
            tx.commit()?;
            return Ok(());
        }
        let (key_kind, key) = category_key(&app.exe_path, url);
        let path = url.map(url_path).unwrap_or_default();
        let source_id = upsert_source(&tx, key_kind, &key, &source_name(key_kind, &key, &app.app_name))?;
        let target_id = upsert_activity(&tx, source_id, &path, &canonical)?;
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
            "UPDATE sessions SET activity_id = ?1 WHERE id = ?2",
            params![target_id, id],
        )?;
        prune_orphan_activity(&tx, old_activity_id)?;
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
            .prepare("SELECT DISTINCT activity_id FROM sessions WHERE end_time IS NULL")?
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
    let source_id: Option<i64> = tx
        .query_row(
            "SELECT source_id FROM activities WHERE id = ?1 AND category_id IS NULL
             AND NOT EXISTS (SELECT 1 FROM sessions WHERE activity_id = ?1)",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(source_id) = source_id else {
        return Ok(());
    };
    tx.execute("DELETE FROM ai_labels WHERE activity_id = ?1", params![id])?;
    tx.execute("DELETE FROM activities WHERE id = ?1", params![id])?;
    tx.execute(
        "DELETE FROM sources WHERE id = ?1
         AND NOT EXISTS (SELECT 1 FROM activities WHERE source_id = ?1)",
        params![source_id],
    )?;
    Ok(())
}
