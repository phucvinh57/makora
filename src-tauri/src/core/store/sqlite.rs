use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};

use crate::core::category::{category_key, url_key};
use crate::core::model::AppInfo;
use crate::core::store::SessionStore;

pub type SharedConnection = Arc<Mutex<Connection>>;

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up("CREATE TABLE IF NOT EXISTS categories (
                id    INTEGER PRIMARY KEY AUTOINCREMENT,
                name  TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS category_rules (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                key_kind    TEXT NOT NULL CHECK (key_kind IN ('exe', 'domain')),
                pattern     TEXT NOT NULL,
                category_id INTEGER NOT NULL REFERENCES categories(id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_rules_key ON category_rules(key_kind, pattern);
            CREATE TABLE IF NOT EXISTS category_overrides (
                key_kind    TEXT NOT NULL CHECK (key_kind IN ('exe', 'domain')),
                pattern     TEXT NOT NULL,
                category_id INTEGER NOT NULL REFERENCES categories(id),
                PRIMARY KEY (key_kind, pattern)
            );",
        ),
        M::up("CREATE TABLE IF NOT EXISTS sessions (
                id                 INTEGER PRIMARY KEY AUTOINCREMENT,
                app_name           TEXT NOT NULL,
                exe_path           TEXT NOT NULL,
                window_title       TEXT NOT NULL,
                url                TEXT,
                category_key       TEXT NOT NULL DEFAULT '',
                category_key_kind  TEXT NOT NULL DEFAULT 'exe',
                start_time         TEXT NOT NULL,
                end_time           TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_exe_path ON sessions(exe_path);
            CREATE INDEX IF NOT EXISTS idx_sessions_start_time ON sessions(start_time);
            CREATE INDEX IF NOT EXISTS idx_sessions_category_key ON sessions(category_key_kind, category_key);",
        ),
        M::up("ALTER TABLE sessions ADD COLUMN category_id INTEGER REFERENCES categories(id);"),
        M::up("CREATE TABLE IF NOT EXISTS ignore_rules (
                id      INTEGER PRIMARY KEY AUTOINCREMENT,
                kind    TEXT NOT NULL CHECK (kind IN ('app', 'site')),
                pattern TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ignore_rules_pattern ON ignore_rules(kind, pattern);",
        ),
        M::up("ALTER TABLE category_rules ADD COLUMN match_kind TEXT NOT NULL DEFAULT 'exact'
                CHECK (match_kind IN ('exact', 'substring', 'regex'));
            DROP INDEX IF EXISTS idx_category_rules_key;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_rules_key
                ON category_rules(key_kind, match_kind, pattern);",
        ),
        M::up("ALTER TABLE ignore_rules ADD COLUMN match_kind TEXT NOT NULL DEFAULT 'substring'
                CHECK (match_kind IN ('exact', 'substring', 'regex'));
            DROP INDEX IF EXISTS idx_ignore_rules_pattern;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ignore_rules_pattern
                ON ignore_rules(kind, match_kind, pattern);",
        ),
        M::up("ALTER TABLE category_rules ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
            UPDATE category_rules
            SET priority = ranked.rn
            FROM (
                SELECT id, ROW_NUMBER() OVER (
                    PARTITION BY key_kind
                    ORDER BY CASE match_kind WHEN 'exact' THEN 0 WHEN 'substring' THEN 1 ELSE 2 END, id
                ) - 1 AS rn
                FROM category_rules
            ) AS ranked
            WHERE category_rules.id = ranked.id;",
        ),
        M::up("ALTER TABLE category_rules DROP COLUMN priority;
            ALTER TABLE categories ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
            UPDATE categories
            SET priority = ranked.rn
            FROM (
                SELECT id, ROW_NUMBER() OVER (ORDER BY id) - 1 AS rn
                FROM categories
            ) AS ranked
            WHERE categories.id = ranked.id;",
        ),
        M::up("CREATE TABLE category_rules_new (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                key_kind    TEXT NOT NULL CHECK (key_kind IN ('exe', 'domain', 'url')),
                pattern     TEXT NOT NULL,
                category_id INTEGER NOT NULL REFERENCES categories(id),
                match_kind  TEXT NOT NULL DEFAULT 'exact'
                    CHECK (match_kind IN ('exact', 'prefix', 'substring', 'regex'))
            );
            INSERT INTO category_rules_new (id, key_kind, pattern, category_id, match_kind)
                SELECT id, key_kind, pattern, category_id, match_kind FROM category_rules;
            DROP TABLE category_rules;
            ALTER TABLE category_rules_new RENAME TO category_rules;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_rules_key
                ON category_rules(key_kind, match_kind, pattern);
            ALTER TABLE categories DROP COLUMN priority;
            ALTER TABLE sessions ADD COLUMN url_key TEXT;",
        ),
        M::up("CREATE TABLE IF NOT EXISTS url_query_whitelist (
                id     INTEGER PRIMARY KEY AUTOINCREMENT,
                domain TEXT NOT NULL,
                param  TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_url_query_whitelist ON url_query_whitelist(domain, param);
            INSERT INTO url_query_whitelist (domain, param) VALUES ('youtube.com', 'v');",
        ),
        M::up("CREATE TABLE IF NOT EXISTS work_sessions (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                name           TEXT NOT NULL,
                note           TEXT,
                target_seconds INTEGER NOT NULL,
                start_time     TEXT NOT NULL,
                end_time       TEXT NOT NULL,
                created_at     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_work_sessions_start_time ON work_sessions(start_time);
            CREATE TABLE IF NOT EXISTS work_session_categories (
                work_session_id INTEGER NOT NULL REFERENCES work_sessions(id),
                category_id     INTEGER NOT NULL REFERENCES categories(id),
                PRIMARY KEY (work_session_id, category_id)
            );",
        ),
        M::up("DROP TABLE IF EXISTS category_overrides;
            DROP TABLE IF EXISTS url_query_whitelist;
            DELETE FROM work_session_categories;
            DELETE FROM category_rules;
            UPDATE sessions SET category_id = NULL;
            DELETE FROM categories;
            DELETE FROM sqlite_sequence WHERE name IN ('categories', 'category_rules');",
        ),
    ])
}

fn register_regexp_function(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "regexp",
        2,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern = ctx.get::<String>(0)?;
            let text = ctx.get::<String>(1)?;
            let matched = regex::RegexBuilder::new(&pattern)
                .case_insensitive(true)
                .build()
                .map(|re| re.is_match(&text))
                .unwrap_or(false);
            Ok(matched)
        },
    )?;
    Ok(())
}

pub fn open_connection(path: &Path) -> Result<SharedConnection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating database directory {}", parent.display()))?;
    }
    let mut conn = Connection::open(path)
        .with_context(|| format!("opening database at {}", path.display()))?;
    register_regexp_function(&conn).context("registering regexp SQL function")?;
    migrations()
        .to_latest(&mut conn)
        .context("migrating schema")?;
    Ok(Arc::new(Mutex::new(conn)))
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key, start_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![app.app_name, app.exe_path, window_title, url, key, key_kind.as_str(), url_key_value, at.to_rfc3339()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE sessions SET end_time = ?1 WHERE id = ?2",
            params![at.to_rfc3339(), id],
        )?;
        Ok(())
    }

    fn close_dangling_sessions(&mut self) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM sessions WHERE end_time IS NULL", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path() -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ (std::process::id() as u128) << 64;
        std::env::temp_dir().join(format!("makora-sqlite-migration-test-{unique}.sqlite"))
    }

    #[test]
    fn category_priority_backfill_orders_by_creation_id_ascending() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        register_regexp_function(&conn).unwrap();
        let migs = migrations();
        migs.to_version(&mut conn, 7).unwrap();
        conn.execute_batch(
            "INSERT INTO categories (name, color) VALUES ('First', '#111111');
             INSERT INTO categories (name, color) VALUES ('Second', '#222222');
             INSERT INTO categories (name, color) VALUES ('Third', '#333333');",
        )
        .unwrap();

        migs.to_version(&mut conn, 8).unwrap();

        let mut stmt = conn
            .prepare("SELECT name, priority FROM categories ORDER BY id")
            .unwrap();
        let rows: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("First".to_string(), 0),
                ("Second".to_string(), 1),
                ("Third".to_string(), 2),
            ]
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn latest_schema_drops_category_priority_and_accepts_url_and_prefix_rules() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        register_regexp_function(&conn).unwrap();
        migrations().to_latest(&mut conn).unwrap();

        let priority_column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('categories') WHERE name = 'priority'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(priority_column_count, 0);

        conn.execute(
            "INSERT INTO categories (name, color) VALUES ('Test', '#000000')",
            [],
        )
        .unwrap();
        let category_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
             VALUES ('url', 'example.com/path', 'prefix', ?1)",
            params![category_id],
        )
        .unwrap();

        let _ = std::fs::remove_file(&path);
    }
}
