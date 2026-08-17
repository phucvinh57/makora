use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};

use crate::core::category::{category_key, url_key};
use crate::core::model::AppInfo;
use crate::core::store::{resolve, SessionStore};

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
        M::up("CREATE TABLE categories_new (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                name      TEXT NOT NULL,
                color     TEXT NOT NULL,
                parent_id INTEGER REFERENCES categories(id)
            );
            INSERT INTO categories_new (id, name, color) SELECT id, name, color FROM categories;
            DROP TABLE categories;
            ALTER TABLE categories_new RENAME TO categories;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_categories_name
                ON categories(COALESCE(parent_id, 0), name);
            CREATE INDEX IF NOT EXISTS idx_categories_parent ON categories(parent_id);",
        ),
        // Glob replaces regex and categorization gains a materialized automatic result.
        // Runtime matching and the data backfill happen in Rust after the schema commits.
        M::up(
            "DELETE FROM category_rules WHERE match_kind = 'regex';
            CREATE TABLE category_rules_new (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                key_kind    TEXT NOT NULL CHECK (key_kind IN ('exe', 'domain', 'url')),
                pattern     TEXT NOT NULL,
                category_id INTEGER NOT NULL REFERENCES categories(id),
                match_kind  TEXT NOT NULL DEFAULT 'exact'
                    CHECK (match_kind IN ('exact', 'prefix', 'substring', 'glob'))
            );
            INSERT INTO category_rules_new (id, key_kind, pattern, category_id, match_kind)
                SELECT id, key_kind, pattern, category_id, match_kind FROM category_rules;
            DROP TABLE category_rules;
            ALTER TABLE category_rules_new RENAME TO category_rules;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_rules_key
                ON category_rules(key_kind, match_kind, pattern);

            -- `prefix` joins the ignore-list CHECK too: IgnoreStore has accepted
            -- MatchKind::Prefix since it was added, but the constraint never listed it, so
            -- such a rule failed with an opaque constraint error.
            DELETE FROM ignore_rules WHERE match_kind = 'regex';
            CREATE TABLE ignore_rules_new (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                kind       TEXT NOT NULL CHECK (kind IN ('app', 'site')),
                pattern    TEXT NOT NULL,
                match_kind TEXT NOT NULL DEFAULT 'substring'
                    CHECK (match_kind IN ('exact', 'prefix', 'substring', 'glob'))
            );
            INSERT INTO ignore_rules_new (id, kind, pattern, match_kind)
                SELECT id, kind, pattern, match_kind FROM ignore_rules;
            DROP TABLE ignore_rules;
            ALTER TABLE ignore_rules_new RENAME TO ignore_rules;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ignore_rules_pattern
                ON ignore_rules(kind, match_kind, pattern);

            -- ADD COLUMN … UNIQUE is rejected outright, so the uniqueness is a partial
            -- index instead. Rust adopts or creates the built-in after migration.
            ALTER TABLE categories ADD COLUMN system_key TEXT;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_categories_system_key
                ON categories(system_key) WHERE system_key IS NOT NULL;

            ALTER TABLE sessions ADD COLUMN rule_category_id INTEGER REFERENCES categories(id);
            CREATE INDEX IF NOT EXISTS idx_sessions_rule_category ON sessions(rule_category_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_category ON sessions(category_id);

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT OR IGNORE INTO settings (key, value)
                VALUES ('distraction_threshold_seconds', '10');",
        )
        .foreign_key_check(),
        // Compatibility marker: databases that ran the earlier development version of
        // migration 14 need one Rust replay to replace its SQL-derived values.
        M::up(
            "INSERT OR IGNORE INTO settings (key, value)
                VALUES ('category_resolver_version', '0');",
        ),
        M::up(
            "ALTER TABLE sessions ADD COLUMN ml_category_id INTEGER REFERENCES categories(id);
             ALTER TABLE sessions ADD COLUMN ml_confidence REAL;
             ALTER TABLE sessions ADD COLUMN ml_model_version INTEGER;
             CREATE INDEX IF NOT EXISTS idx_sessions_ml_category ON sessions(ml_category_id);",
        ),
        M::up(
            "INSERT OR IGNORE INTO settings (key, value)
                VALUES ('ai_categorization_schedule', 'manual');",
        ),
        M::up(
            "UPDATE sessions
             SET rule_category_id = NULL
             WHERE rule_category_id IN (
                 SELECT id FROM categories WHERE system_key = 'distraction'
             );
             DROP INDEX IF EXISTS idx_categories_system_key;
             DELETE FROM settings WHERE key = 'distraction_threshold_seconds';
             ALTER TABLE categories DROP COLUMN system_key;",
        )
        .foreign_key_check(),
    ])
}

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

/// The bundled SQLite enforces foreign keys by default, which a migration that rebuilds a
/// referenced table (`DROP TABLE categories` while rules and pins point at it) would trip
/// over. SQLite's own recipe is to lift the check for the duration — it cannot be toggled
/// inside a transaction, so it has to happen out here rather than inside a migration.
fn migrate(conn: &mut Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = migrations().to_latest(conn);
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result?;

    let tx = conn.transaction()?;
    let resolver_version: Option<String> = tx
        .query_row(
            "SELECT value FROM settings WHERE key = 'category_resolver_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if resolver_version.as_deref() != Some("2") {
        resolve::resolve_all(&tx)?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ('category_resolver_version', '2')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
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
        tx.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key, start_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![app.app_name, app.exe_path, window_title, url, key, key_kind.as_str(), url_key_value, at.to_rfc3339()],
        )?;
        let id = tx.last_insert_rowid();
        resolve::resolve_session(&tx, id)?;
        tx.commit()?;
        Ok(id)
    }

    /// Closes the row and derives its category in Rust in one transaction.
    fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()> {
        let end_time = at.to_rfc3339();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let category_id = resolve::resolve_session_category(&tx, id)?;
        tx.execute(
            "UPDATE sessions SET end_time = ?1, rule_category_id = ?2 WHERE id = ?3",
            params![end_time, category_id, id],
        )?;
        tx.commit()?;
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

    type MigratedResolution = (i64, Option<i64>, Option<i64>, Option<i64>);

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
    fn subcategory_migration_keeps_existing_categories_and_scopes_names_to_their_parent() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        let migs = migrations();
        migs.to_version(&mut conn, 12).unwrap();
        conn.execute_batch(
            "INSERT INTO categories (name, color) VALUES ('Work', '#111111');
             INSERT INTO categories (name, color) VALUES ('Personal', '#222222');",
        )
        .unwrap();
        let work_id: i64 = conn
            .query_row("SELECT id FROM categories WHERE name = 'Work'", [], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
             VALUES ('exe', 'code.exe', 'exact', ?1)",
            params![work_id],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let surviving: Vec<(i64, String)> = conn
            .prepare("SELECT id, name FROM categories ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            surviving,
            vec![(1, "Work".to_string()), (2, "Personal".to_string())],
            "the rebuild has to keep ids, since rules and pins point at them"
        );
        let rules_still_pointing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM category_rules WHERE category_id = ?1",
                params![work_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rules_still_pointing, 1);

        conn.execute(
            "INSERT INTO categories (name, color, parent_id) VALUES ('Reading', '#333333', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO categories (name, color, parent_id) VALUES ('Reading', '#444444', 2)",
            [],
        )
        .expect("the same subcategory name under a different parent is a different category");
        assert!(
            conn.execute(
                "INSERT INTO categories (name, color, parent_id) VALUES ('Reading', '#555555', 1)",
                [],
            )
            .is_err(),
            "two subcategories of the same parent must not share a name"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn latest_schema_drops_category_priority_and_accepts_url_and_prefix_rules() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn).unwrap();

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
        conn.execute(
            "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
             VALUES ('url', 'github.com/*/pulls', 'glob', ?1)",
            params![category_id],
        )
        .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                 VALUES ('exe', 'anything', 'regex', ?1)",
                params![category_id],
            )
            .is_err(),
            "regex was retired; the CHECK is what keeps a stale client from reintroducing it"
        );

        let threshold_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'distraction_threshold_seconds'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(threshold_count, 0);

        let ai_schedule: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ai_categorization_schedule'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ai_schedule, "manual");

        let resolved_column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'rule_category_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved_column_count, 1);

        let system_key_column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('categories') WHERE name = 'system_key'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(system_key_column_count, 0);
        let category_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            category_count, 1,
            "only the Test category inserted by this test exists"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// Everything the schema migration and Rust bootstrap have to get right on a v13 database.
    fn migrated_from_v13(setup: &str) -> (Connection, std::path::PathBuf) {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 13).unwrap();
        conn.execute_batch(setup).unwrap();
        migrate(&mut conn).unwrap();
        (conn, path)
    }

    #[test]
    fn migration_14_drops_regex_rules_from_both_tables_and_keeps_the_rest() {
        let (conn, path) = migrated_from_v13(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#111111');
             INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                VALUES ('exe', 'code.exe', 'exact', 1), ('exe', '^code', 'regex', 1);
             INSERT INTO ignore_rules (kind, pattern, match_kind)
                VALUES ('app', 'steam', 'substring'), ('app', '^game', 'regex');",
        );

        let surviving_rules: Vec<String> = conn
            .prepare("SELECT pattern FROM category_rules ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(surviving_rules, vec!["code.exe".to_string()]);

        let surviving_ignores: Vec<String> = conn
            .prepare("SELECT pattern FROM ignore_rules ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(surviving_ignores, vec!["steam".to_string()]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rust_bootstrap_backfills_resolved_categories_over_existing_history() {
        let (conn, path) = migrated_from_v13(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Work', '#111111');
             INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                VALUES ('exe', 'code.exe', 'exact', 1);
             INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time, end_time)
                VALUES ('code.exe', 'code.exe', 't', 'code.exe', 'exe', '2024-06-15T00:00:00Z', '2024-06-15T00:10:00Z'),
                       ('code.exe', 'code.exe', 't', 'code.exe', 'exe', '2024-06-15T01:00:00Z', '2024-06-15T01:00:04Z'),
                       ('other.exe', 'other.exe', 't', 'other.exe', 'exe', '2024-06-15T02:00:00Z', '2024-06-15T02:30:00Z'),
                       ('blip.exe', 'blip.exe', 't', 'blip.exe', 'exe', '2024-06-15T03:00:00Z', '2024-06-15T03:00:03Z');
             INSERT INTO sessions (app_name, exe_path, window_title, category_key, category_key_kind, start_time)
                VALUES ('code.exe', 'code.exe', 't', 'code.exe', 'exe', '2024-06-15T04:00:00Z');",
        );

        let resolved: Vec<Option<i64>> = conn
            .prepare("SELECT rule_category_id FROM sessions ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            resolved,
            vec![Some(1), Some(1), None, None, Some(1)],
            "rules apply regardless of duration; unmatched rows remain uncategorized"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_keeps_a_category_the_user_already_named_distraction() {
        let (conn, path) = migrated_from_v13(
            "INSERT INTO categories (id, name, color) VALUES (7, 'Distraction', '#ff0000');
             INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                VALUES ('domain', 'reddit.com', 'exact', 7);",
        );

        let kept: Vec<(i64, String, String)> = conn
            .prepare("SELECT id, name, color FROM categories WHERE name = 'Distraction'")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            kept,
            vec![(7, "Distraction".to_string(), "#ff0000".to_string())],
            "an ordinary category keeps its id, colour and rules"
        );

        let rules_kept: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM category_rules WHERE category_id = 7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rules_kept, 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_15_replays_a_database_that_already_ran_the_old_sql_resolver() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 14).unwrap();
        conn.execute_batch(
            "INSERT INTO categories (id, name, color, system_key)
                VALUES (1, 'Work', '#111111', NULL),
                       (2, 'Distraction', '#a1a1aa', 'distraction');
             INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                VALUES ('exe', 'code.exe', 'exact', 1);
             INSERT INTO sessions (
                 app_name, exe_path, window_title, category_key, category_key_kind,
                 start_time, end_time, rule_category_id
             ) VALUES (
                 'code.exe', 'code.exe', 't', 'code.exe', 'exe',
                 '2024-06-15T00:00:00Z', '2024-06-15T00:10:00Z', 2
             );",
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let resolved: Option<i64> = conn
            .query_row("SELECT rule_category_id FROM sessions LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(resolved, Some(1));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_18_clears_only_automatic_distraction_assignments() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 17).unwrap();
        conn.execute_batch(
            "INSERT INTO categories (id, name, color, system_key)
                VALUES (1, 'Work', '#111111', NULL),
                       (2, 'Distraction', '#a1a1aa', 'distraction');
             INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                VALUES ('exe', 'noise.exe', 'exact', 2);
             INSERT INTO sessions (
                 id, app_name, exe_path, window_title, category_key, category_key_kind,
                 start_time, end_time, category_id, rule_category_id, ml_category_id,
                 ml_confidence, ml_model_version
             ) VALUES
                (1, 'random.exe', 'random.exe', 't', 'random.exe', 'exe',
                 '2024-06-15T00:00:00Z', '2024-06-15T00:00:03Z', NULL, 2, NULL, NULL, NULL),
                (2, 'noise.exe', 'noise.exe', 't', 'noise.exe', 'exe',
                 '2024-06-15T01:00:00Z', '2024-06-15T01:00:03Z', NULL, 2, NULL, NULL, NULL),
                (3, 'manual.exe', 'manual.exe', 't', 'manual.exe', 'exe',
                 '2024-06-15T02:00:00Z', '2024-06-15T02:00:03Z', 2, 2, NULL, NULL, NULL),
                (4, 'predicted.exe', 'predicted.exe', 't', 'predicted.exe', 'exe',
                 '2024-06-15T03:00:00Z', '2024-06-15T03:00:03Z', NULL, 2, 2, 0.9, 7);
             INSERT INTO work_sessions (
                 id, name, target_seconds, start_time, end_time, created_at
             ) VALUES (
                 1, 'Focus', 600, '2024-06-15T00:00:00Z',
                 '2024-06-15T00:10:00Z', '2024-06-15T00:00:00Z'
             );
             INSERT INTO work_session_categories (work_session_id, category_id) VALUES (1, 2);",
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let categories: Vec<(i64, String, String)> = conn
            .prepare("SELECT id, name, color FROM categories ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            categories,
            vec![
                (1, "Work".to_string(), "#111111".to_string()),
                (2, "Distraction".to_string(), "#a1a1aa".to_string()),
            ]
        );

        let resolved: Vec<MigratedResolution> = conn
            .prepare(
                "SELECT id, category_id, rule_category_id, ml_category_id
                 FROM sessions ORDER BY id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            resolved,
            vec![
                (1, None, None, None),
                (2, None, Some(2), None),
                (3, Some(2), None, None),
                (4, None, None, Some(2)),
            ],
            "fallback labels are cleared, while rule, manual and AI assignments remain"
        );

        let work_session_link: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM work_session_categories
                 WHERE work_session_id = 1 AND category_id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(work_session_link, 1);
        let obsolete_schema: i64 = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM pragma_table_info('categories') WHERE name = 'system_key') +
                    (SELECT COUNT(*) FROM settings WHERE key = 'distraction_threshold_seconds')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(obsolete_schema, 0);

        let _ = std::fs::remove_file(&path);
    }
}
