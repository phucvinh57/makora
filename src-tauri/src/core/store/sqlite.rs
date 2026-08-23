use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};

use crate::core::category::{category_key, url_key, KeyKind};
use crate::core::model::AppInfo;
use crate::core::store::{resolve, SessionStore};
use crate::core::title::{canonical_title, title_quality};

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
        M::up(
            "INSERT OR IGNORE INTO settings (key, value)
                VALUES ('ai_categorization_confidence_threshold', '0.75');",
        ),
        // Sub-second blocks are event noise, never activity. Existing history is purged once
        // here; `end_session` keeps new ones out.
        //
        // `unixepoch(…, 'subsec')` rather than the `julianday` arithmetic the read queries
        // use: a julianday difference is a fraction of a day, so a row of exactly one second
        // comes back as 0.9999999… and would be deleted by a `< 1.0` test.
        M::up(
            "DELETE FROM sessions
             WHERE end_time IS NOT NULL
               AND unixepoch(end_time, 'subsec') - unixepoch(start_time, 'subsec') < 1.0;",
        ),
        M::up(
            "ALTER TABLE categories ADD COLUMN description TEXT;

             CREATE TABLE category_suggestions (
                 id            INTEGER PRIMARY KEY,
                 key_kind      TEXT NOT NULL CHECK(key_kind IN ('exe', 'domain')),
                 key           TEXT NOT NULL,
                 category_id   INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                 confidence    REAL NOT NULL,
                 seconds       REAL NOT NULL,
                 model_version INTEGER NOT NULL,
                 created_at    TEXT NOT NULL,
                 status        TEXT NOT NULL DEFAULT 'pending'
                               CHECK(status IN ('pending', 'dismissed')),
                 UNIQUE(key_kind, key)
             );

             CREATE TABLE key_embeddings (
                 key_kind TEXT NOT NULL,
                 key      TEXT NOT NULL,
                 doc_hash TEXT NOT NULL,
                 model_id TEXT NOT NULL,
                 vector   BLOB NOT NULL,
                 PRIMARY KEY (key_kind, key)
             );

             INSERT OR IGNORE INTO settings (key, value)
                 VALUES ('ai_categorization_model_version', '0');
             INSERT OR IGNORE INTO settings (key, value)
                 VALUES ('ai_categorization_input_revision', '0');

             UPDATE sessions
                SET ml_category_id = NULL,
                    ml_confidence = NULL,
                    ml_model_version = NULL;",
        )
        .foreign_key_check(),
        M::up(
            "CREATE TABLE multi_purpose_keys (
                key_kind    TEXT NOT NULL CHECK(key_kind IN ('exe', 'domain')),
                key         TEXT NOT NULL,
                source      TEXT NOT NULL CHECK(source IN ('seed', 'auto', 'user')),
                enabled     INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
                top_share   REAL CHECK(top_share IS NULL OR (top_share >= 0.0 AND top_share <= 1.0)),
                detected_at TEXT,
                PRIMARY KEY (key_kind, key)
            );

            CREATE TABLE category_suggestions_new (
                id            INTEGER PRIMARY KEY,
                key_kind      TEXT NOT NULL CHECK(key_kind IN ('exe', 'domain', 'url')),
                key           TEXT NOT NULL,
                category_id   INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                confidence    REAL NOT NULL,
                seconds       REAL NOT NULL,
                model_version INTEGER NOT NULL,
                created_at    TEXT NOT NULL,
                status        TEXT NOT NULL DEFAULT 'pending'
                              CHECK(status IN ('pending', 'dismissed')),
                UNIQUE(key_kind, key)
            );
            INSERT INTO category_suggestions_new
                (id, key_kind, key, category_id, confidence, seconds, model_version, created_at, status)
                SELECT id, key_kind, key, category_id, confidence, seconds, model_version, created_at, status
                FROM category_suggestions;
            DROP TABLE category_suggestions;
            ALTER TABLE category_suggestions_new RENAME TO category_suggestions;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_suggestions_key
                ON category_suggestions(key_kind, key);",
        )
        .foreign_key_check(),
        M::up(
            "CREATE TABLE activity_clusters (
                id            INTEGER PRIMARY KEY,
                scope         TEXT NOT NULL CHECK(scope IN ('key', 'title')),
                parent_kind   TEXT CHECK(parent_kind IS NULL OR parent_kind IN ('exe', 'domain')),
                parent_key    TEXT,
                label         TEXT NOT NULL,
                category_id   INTEGER REFERENCES categories(id) ON DELETE CASCADE,
                confidence    REAL CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
                lead          REAL CHECK(lead IS NULL OR (lead >= 0.0 AND lead <= 1.0)),
                cohesion      REAL NOT NULL CHECK(cohesion >= -1.0 AND cohesion <= 1.0),
                member_count  INTEGER NOT NULL CHECK(member_count > 0),
                seconds       REAL NOT NULL CHECK(seconds >= 0.0),
                model_version INTEGER NOT NULL,
                created_at    TEXT NOT NULL,
                CHECK ((scope = 'key'   AND parent_kind IS NULL     AND parent_key IS NULL) OR
                       (scope = 'title' AND parent_kind IS NOT NULL AND parent_key IS NOT NULL))
            );
            CREATE INDEX idx_activity_clusters_scope
                ON activity_clusters(scope, seconds DESC);

            CREATE TABLE activity_cluster_members (
                id            INTEGER PRIMARY KEY,
                cluster_id    INTEGER NOT NULL REFERENCES activity_clusters(id) ON DELETE CASCADE,
                key_kind      TEXT NOT NULL CHECK(key_kind IN ('exe', 'domain')),
                key           TEXT NOT NULL,
                url_key       TEXT,
                window_title  TEXT,
                seconds       REAL NOT NULL CHECK(seconds >= 0.0),
                session_count INTEGER NOT NULL DEFAULT 0 CHECK(session_count >= 0)
            );
            CREATE INDEX idx_activity_cluster_members_cluster
                ON activity_cluster_members(cluster_id);
            -- Nullable identity fields cannot be part of a SQLite PRIMARY KEY expression.
            CREATE UNIQUE INDEX idx_activity_cluster_members_identity
                ON activity_cluster_members(cluster_id, key_kind, key,
                                            COALESCE(url_key, ''), COALESCE(window_title, ''));

            INSERT OR IGNORE INTO settings (key, value)
                VALUES ('ai_categorization_cluster_granularity', 'balanced');",
        )
        .foreign_key_check(),
        M::up(
            "CREATE TABLE ai_verdicts (
                id                      INTEGER PRIMARY KEY,
                recorded_at             TEXT NOT NULL,
                surface                 TEXT NOT NULL CHECK(surface IN ('session','suggestion','cluster')),
                bulk                    INTEGER NOT NULL DEFAULT 0 CHECK(bulk IN (0,1)),
                outcome                 TEXT NOT NULL
                                        CHECK(outcome IN ('agree','corrected','rejected','no_prediction')),
                key_kind                TEXT,
                key                     TEXT,
                url_key                 TEXT,
                window_title            TEXT,
                session_id              INTEGER,
                predicted_category_id   INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                predicted_category_name TEXT,
                chosen_category_id      INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                chosen_category_name    TEXT,
                confidence              REAL,
                model_version           INTEGER,
                seconds                 REAL NOT NULL DEFAULT 0.0
            );
            CREATE INDEX idx_ai_verdicts_recorded ON ai_verdicts(recorded_at);
            CREATE INDEX idx_ai_verdicts_surface ON ai_verdicts(surface, outcome);

            CREATE TABLE ai_run_evaluations (
                id                          INTEGER PRIMARY KEY,
                recorded_at                 TEXT NOT NULL,
                model_version               INTEGER NOT NULL,
                update_kind                 TEXT NOT NULL,
                confidence_threshold        REAL NOT NULL,
                holdout_accuracy            REAL,
                holdout_sample_count        INTEGER NOT NULL,
                manual_holdout_accuracy     REAL,
                manual_holdout_sample_count INTEGER NOT NULL,
                predicted_session_count     INTEGER NOT NULL,
                labeled_key_count           INTEGER NOT NULL,
                category_count              INTEGER NOT NULL
            );
            CREATE INDEX idx_ai_run_evaluations_recorded ON ai_run_evaluations(recorded_at);",
        )
        .foreign_key_check(),
        // Query parameters were removed by migration 12, but URL identity needs a small,
        // explicit allowlist again so sites such as YouTube can distinguish resources.
        M::up(
            "CREATE TABLE IF NOT EXISTS url_query_whitelist (
                id     INTEGER PRIMARY KEY AUTOINCREMENT,
                domain TEXT NOT NULL,
                param  TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_url_query_whitelist
                ON url_query_whitelist(domain, param);
            INSERT OR IGNORE INTO url_query_whitelist (domain, param)
                VALUES ('youtube.com', 'v');",
        ),
        M::up(
            "CREATE TABLE activities (
                id               INTEGER PRIMARY KEY,
                key_kind         TEXT NOT NULL CHECK(key_kind IN ('exe','domain')),
                key              TEXT NOT NULL,
                url_key          TEXT,
                title            TEXT NOT NULL DEFAULT '',
                display_title    TEXT NOT NULL DEFAULT '',
                app_name         TEXT NOT NULL DEFAULT '',
                url              TEXT,
                first_seen       TEXT NOT NULL,
                last_seen        TEXT NOT NULL,
                category_id      INTEGER REFERENCES categories(id),
                rule_category_id INTEGER REFERENCES categories(id),
                ml_category_id   INTEGER REFERENCES categories(id),
                ml_confidence    REAL,
                ml_model_version INTEGER
            );
            CREATE UNIQUE INDEX idx_activities_identity
                ON activities(key_kind, key, COALESCE(url_key, ''), title);
            CREATE INDEX idx_activities_key ON activities(key_kind, key);
            CREATE INDEX idx_activities_category ON activities(category_id);
            CREATE INDEX idx_activities_ml ON activities(ml_category_id);
            ALTER TABLE sessions ADD COLUMN activity_id INTEGER REFERENCES activities(id);
            CREATE INDEX idx_sessions_activity ON sessions(activity_id);
            ALTER TABLE ai_verdicts ADD COLUMN activity_id INTEGER REFERENCES activities(id) ON DELETE SET NULL;
            CREATE INDEX idx_ai_verdicts_activity ON ai_verdicts(activity_id);
            ALTER TABLE activity_cluster_members ADD COLUMN activity_id INTEGER REFERENCES activities(id) ON DELETE CASCADE;
            CREATE INDEX idx_activity_cluster_members_activity_lookup ON activity_cluster_members(activity_id);
            CREATE UNIQUE INDEX idx_activity_cluster_members_activity
                ON activity_cluster_members(cluster_id, activity_id)
                WHERE activity_id IS NOT NULL;
            INSERT OR IGNORE INTO settings (key, value)
                VALUES ('activity_identity_version', '0');",
        )
        .foreign_key_check(),
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
    let activity_version: Option<String> = tx
        .query_row(
            "SELECT value FROM settings WHERE key = 'activity_identity_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if activity_version.as_deref() != Some("1") {
        backfill_activities(&tx)?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ('activity_identity_version', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )?;
    }
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

#[derive(Debug, Clone)]
struct BackfillSession {
    id: i64,
    activity_id: i64,
    app_name: String,
    url: Option<String>,
    window_title: String,
    start_time: String,
    end_time: Option<String>,
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

fn backfill_activities(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    let rows = {
        let mut stmt = tx.prepare(
            "SELECT id, app_name, window_title, url, category_key_kind, category_key,
                    url_key, start_time, end_time
             FROM sessions ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let app_name: String = row.get(1)?;
                let window_title: String = row.get(2)?;
                let url: Option<String> = row.get(3)?;
                let key_kind = KeyKind::parse(&row.get::<_, String>(4)?).unwrap_or(KeyKind::Exe);
                let key: String = row.get(5)?;
                let url_key_value: Option<String> = row.get(6)?;
                let title = canonical_title(&window_title, &app_name, url.as_deref());
                Ok((
                    row.get::<_, i64>(0)?,
                    app_name,
                    window_title,
                    url,
                    key_kind,
                    key,
                    url_key_value,
                    title,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    for (id, app_name, window_title, url, key_kind, key, url_key_value, title, start_time, _) in
        rows
    {
        let activity_id = upsert_activity(
            tx,
            key_kind,
            &key,
            url_key_value.as_deref(),
            &title,
            &window_title,
            &app_name,
            url.as_deref(),
            &start_time,
        )?;
        tx.execute(
            "UPDATE sessions SET activity_id = ?1 WHERE id = ?2",
            params![activity_id, id],
        )?;
    }

    // Promote a placeholder only inside an exact resource partition with one real title.
    let placeholders: Vec<(i64, String, String, Option<String>)> = {
        let mut stmt =
            tx.prepare("SELECT id, key_kind, key, url_key FROM activities WHERE title = ''")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (old_id, key_kind, key, url_key_value) in placeholders {
        let target_id: Option<i64> = tx
            .query_row(
                "SELECT MIN(id) FROM activities
                 WHERE key_kind = ?1 AND key = ?2 AND COALESCE(url_key, '') = COALESCE(?3, '')
                   AND title <> ''
                 HAVING COUNT(DISTINCT title) = 1",
                params![key_kind, key, url_key_value],
                |row| row.get(0),
            )
            .optional()?;
        let Some(target_id) = target_id else {
            continue;
        };
        let display_title: String = tx.query_row(
            "SELECT display_title FROM activities WHERE id = ?1",
            params![target_id],
            |row| row.get(0),
        )?;
        let old_pin: Option<i64> = tx.query_row(
            "SELECT category_id FROM activities WHERE id = ?1",
            params![old_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE sessions SET activity_id = ?1, window_title = ?2 WHERE activity_id = ?3",
            params![target_id, display_title, old_id],
        )?;
        if let Some(old_pin) = old_pin {
            tx.execute(
                "UPDATE activities SET category_id = COALESCE(category_id, ?1) WHERE id = ?2",
                params![old_pin, target_id],
            )?;
        }
        tx.execute("DELETE FROM activities WHERE id = ?1", params![old_id])?;
    }

    let activity_ids: Vec<i64> = tx
        .prepare("SELECT id FROM activities")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for activity_id in activity_ids {
        let manual: Option<i64> = tx
            .query_row(
                "SELECT category_id FROM sessions WHERE activity_id = ?1 AND category_id IS NOT NULL
                 GROUP BY category_id ORDER BY COUNT(*) DESC, category_id LIMIT 1",
                params![activity_id],
                |row| row.get(0),
            )
            .optional()?;
        let ml: Option<(i64, Option<f64>, Option<i64>)> = tx
            .query_row(
                "SELECT ml_category_id, AVG(ml_confidence), MAX(ml_model_version)
                 FROM sessions WHERE activity_id = ?1 AND ml_category_id IS NOT NULL
                 GROUP BY ml_category_id ORDER BY COUNT(*) DESC, ml_category_id LIMIT 1",
                params![activity_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        tx.execute(
            "UPDATE activities SET category_id = ?1, ml_category_id = ?2,
                    ml_confidence = ?3, ml_model_version = ?4 WHERE id = ?5",
            params![
                manual,
                ml.as_ref().map(|v| v.0),
                ml.as_ref().and_then(|v| v.1),
                ml.as_ref().and_then(|v| v.2),
                activity_id
            ],
        )?;
    }

    let sessions = {
        let mut stmt = tx.prepare(
            "SELECT id, activity_id, app_name, url, window_title, start_time, end_time
             FROM sessions WHERE activity_id IS NOT NULL ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BackfillSession {
                    id: row.get(0)?,
                    activity_id: row.get(1)?,
                    app_name: row.get(2)?,
                    url: row.get(3)?,
                    window_title: row.get(4)?,
                    start_time: row.get(5)?,
                    end_time: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let mut previous: Option<BackfillSession> = None;
    for current in sessions {
        let merge = previous.as_ref().is_some_and(|prev| {
            prev.activity_id == current.activity_id
                && prev.end_time.is_some()
                && current.end_time.is_some()
                && parse_time(&current.start_time)
                    .zip(parse_time(prev.end_time.as_deref().unwrap_or_default()))
                    .is_some_and(|(start, end)| {
                        let gap = (start - end).num_seconds();
                        (0..=MERGE_GAP_SECONDS).contains(&gap)
                    })
        });
        if merge {
            let prev = previous.as_mut().unwrap();
            let better = title_quality(
                &current.window_title,
                &current.app_name,
                current.url.as_deref(),
            ) > title_quality(&prev.window_title, &prev.app_name, prev.url.as_deref());
            tx.execute(
                "UPDATE sessions SET end_time = ?1,
                    window_title = CASE WHEN ?2 THEN ?3 ELSE window_title END WHERE id = ?4",
                params![current.end_time, better, current.window_title, prev.id],
            )?;
            tx.execute(
                "UPDATE ai_verdicts SET activity_id = ?1 WHERE session_id = ?2",
                params![prev.activity_id, current.id],
            )?;
            tx.execute("DELETE FROM sessions WHERE id = ?1", params![current.id])?;
            prev.end_time = current.end_time;
            if better {
                prev.window_title = current.window_title;
            }
        } else {
            previous = Some(current);
        }
    }
    tx.execute(
        "UPDATE ai_verdicts SET activity_id = (
            SELECT activity_id FROM sessions WHERE sessions.id = ai_verdicts.session_id
         ) WHERE activity_id IS NULL AND session_id IS NOT NULL",
        [],
    )?;
    Ok(())
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
        resolve::resolve_activity(&tx, activity_id)?;
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
            resolve::resolve_activity(&tx, target_id)?;
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

        let ai_confidence_threshold: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ai_categorization_confidence_threshold'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ai_confidence_threshold, "0.75");

        let resolved_column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'rule_category_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved_column_count, 1);

        let description_column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('categories') WHERE name = 'description'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(description_column_count, 1);
        let model_version: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ai_categorization_model_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(model_version, "0");
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('category_suggestions', 'key_embeddings')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            2
        );

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

    #[test]
    fn migration_22_preserves_suggestions_and_accepts_url_kind() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 21).unwrap();
        conn.execute(
            "INSERT INTO categories (id, name, color) VALUES (1, 'Learning', '#111111')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO category_suggestions
                (id, key_kind, key, category_id, confidence, seconds, model_version, created_at, status)
             VALUES
                (7, 'domain', 'example.com', 1, 0.81, 120.0, 3, '2024-01-01T00:00:00Z', 'pending'),
                (8, 'exe', 'player', 1, 0.61, 80.0, 2, '2024-01-02T00:00:00Z', 'dismissed')",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO category_suggestions
                (id, key_kind, key, category_id, confidence, seconds, model_version, created_at)
             VALUES (9, 'url', 'example.com/learning', 1, 0.9, 50.0, 4, '2024-01-03T00:00:00Z')",
            [],
        )
        .unwrap();
        let rows: Vec<(i64, String, String, String)> = conn
            .prepare("SELECT id, key_kind, key, status FROM category_suggestions ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (7, "domain".into(), "example.com".into(), "pending".into()),
                (8, "exe".into(), "player".into(), "dismissed".into()),
                (
                    9,
                    "url".into(),
                    "example.com/learning".into(),
                    "pending".into()
                ),
            ]
        );
        let foreign_key_violations: Vec<String> = conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(foreign_key_violations.is_empty());
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
                (4, None, None, None),
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

    #[test]
    fn migration_20_drops_sub_second_history_and_keeps_open_rows() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 19).unwrap();
        conn.execute_batch(
            "INSERT INTO sessions (
                 app_name, exe_path, window_title, category_key, category_key_kind,
                 start_time, end_time
             ) VALUES
                ('chrome', 'chrome', 'https://a.example/', 'a.example', 'domain',
                 '2024-06-15T00:00:00+00:00', '2024-06-15T00:00:00.400+00:00'),
                ('chrome', 'chrome', 'Real title', 'a.example', 'domain',
                 '2024-06-15T00:01:00+00:00', '2024-06-15T00:01:01+00:00'),
                ('code', 'code', 'main.rs', 'code', 'exe',
                 '2024-06-15T00:02:00+00:00', '2024-06-15T00:12:00+00:00');
             INSERT INTO sessions (
                 app_name, exe_path, window_title, category_key, category_key_kind, start_time
             ) VALUES ('code', 'code', 'still open', 'code', 'exe', '2024-06-15T00:20:00+00:00');",
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let surviving: Vec<String> = conn
            .prepare("SELECT window_title FROM sessions ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            surviving,
            vec![
                "Real title".to_string(),
                "main.rs".to_string(),
                "still open".to_string(),
            ],
            "only the sub-second block goes; a row still open has no duration to judge yet"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ending_a_sub_second_block_deletes_the_row_instead_of_closing_it() {
        let path = temp_db_path();
        let conn = open_connection(&path).unwrap();
        let mut store = SqliteStore::new(conn.clone());
        let app = AppInfo {
            process_id: 1,
            exe_path: "chrome.exe".to_string(),
            app_name: "chrome".to_string(),
        };
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();

        let blip = store
            .start_session(
                &app,
                "https://a.example/",
                Some("https://a.example/"),
                start,
            )
            .unwrap();
        store
            .end_session(blip, start + chrono::Duration::milliseconds(400))
            .unwrap();
        let kept = store
            .start_session(&app, "Real title", Some("https://b.example/"), start)
            .unwrap();
        store
            .end_session(kept, start + chrono::Duration::seconds(1))
            .unwrap();

        let surviving: Vec<(i64, String)> = conn
            .lock()
            .unwrap()
            .prepare("SELECT id, window_title FROM sessions ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            surviving,
            vec![(kept, "Real title".to_string())],
            "exactly one second is long enough to keep; anything under it is noise"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_19_adds_the_ai_confidence_threshold_without_overwriting_settings() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 18).unwrap();
        conn.execute(
            "UPDATE settings SET value = 'daily' WHERE key = 'ai_categorization_schedule'",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let threshold: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ai_categorization_confidence_threshold'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let schedule: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ai_categorization_schedule'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(threshold, "0.75");
        assert_eq!(schedule, "daily");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_24_creates_evaluation_tables_and_preserves_deleted_names() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 23).unwrap();
        migrations().to_version(&mut conn, 24).unwrap();

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('ai_verdicts', 'ai_run_evaluations')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 2);

        conn.execute(
            "INSERT INTO categories (name, color) VALUES ('Old label', '#123456')",
            [],
        )
        .unwrap();
        let category_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO ai_verdicts
                (recorded_at, surface, outcome, predicted_category_id,
                 predicted_category_name, chosen_category_id, chosen_category_name)
             VALUES ('2025-01-01T00:00:00Z', 'session', 'corrected', ?1, 'Old label', ?1, 'Old label')",
            params![category_id],
        )
        .unwrap();
        conn.execute("DELETE FROM categories WHERE id = ?1", params![category_id])
            .unwrap();
        let snapshot: (Option<i64>, Option<i64>, String, String) = conn
            .query_row(
                "SELECT predicted_category_id, chosen_category_id,
                        predicted_category_name, chosen_category_name
                 FROM ai_verdicts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            snapshot,
            (None, None, "Old label".into(), "Old label".into())
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn identical_activity_resumes_inside_the_merge_gap() {
        let path = temp_db_path();
        let conn = open_connection(&path).unwrap();
        let mut store = SqliteStore::new(conn.clone());
        let app = AppInfo {
            process_id: 1,
            exe_path: "chrome.exe".into(),
            app_name: "Google Chrome".into(),
        };
        let first = store
            .start_session(
                &app,
                "Example",
                Some("https://example.com/page"),
                parse_time("2025-01-01T00:00:00Z").unwrap(),
            )
            .unwrap();
        store
            .end_session(first, parse_time("2025-01-01T00:00:10Z").unwrap())
            .unwrap();
        let resumed = store
            .start_session(
                &app,
                "Example",
                Some("https://example.com/page"),
                parse_time("2025-01-01T00:00:11Z").unwrap(),
            )
            .unwrap();
        assert_eq!(resumed, first);
        let count: i64 = conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        store
            .end_session(resumed, parse_time("2025-01-01T00:00:20Z").unwrap())
            .unwrap();
        let separate = store
            .start_session(
                &app,
                "Example",
                Some("https://example.com/page"),
                parse_time("2025-01-01T00:00:50Z").unwrap(),
            )
            .unwrap();
        assert_ne!(separate, first);
        let count: i64 = conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn refinement_repoints_a_placeholder_and_prunes_the_old_activity() {
        let path = temp_db_path();
        let conn = open_connection(&path).unwrap();
        let mut store = SqliteStore::new(conn.clone());
        let app = AppInfo {
            process_id: 1,
            exe_path: "chrome.exe".into(),
            app_name: "Google Chrome".into(),
        };
        let id = store
            .start_session(
                &app,
                "YouTube",
                Some("https://www.youtube.com/shorts/abc"),
                parse_time("2025-01-01T00:00:00Z").unwrap(),
            )
            .unwrap();
        let old_activity: i64 = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT activity_id FROM sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        store.refine_title(id, "A real short title").unwrap();
        let row: (String, i64) = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT window_title, activity_id FROM sessions WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, "A real short title");
        assert_ne!(row.1, old_activity);
        assert_eq!(
            conn.lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM activities", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_keeps_placeholder_activities_without_a_unique_real_title() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 24).unwrap();
        conn.execute_batch(
            "INSERT INTO sessions
                (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key,
                 start_time, end_time)
             VALUES
                ('Google Chrome', 'chrome.exe', 'YouTube', 'https://www.youtube.com/shorts/only-placeholder', 'youtube.com', 'domain', 'youtube.com/shorts/only-placeholder', '2025-01-01T00:00:00Z', '2025-01-01T00:00:10Z'),
                ('Google Chrome', 'chrome.exe', 'YouTube', 'https://www.youtube.com/shorts/ambiguous', 'youtube.com', 'domain', 'youtube.com/shorts/ambiguous', '2025-01-01T01:00:00Z', '2025-01-01T01:00:10Z'),
                ('Google Chrome', 'chrome.exe', 'First real title', 'https://www.youtube.com/shorts/ambiguous', 'youtube.com', 'domain', 'youtube.com/shorts/ambiguous', '2025-01-01T02:00:00Z', '2025-01-01T02:00:10Z'),
                ('Google Chrome', 'chrome.exe', 'Second real title', 'https://www.youtube.com/shorts/ambiguous', 'youtube.com', 'domain', 'youtube.com/shorts/ambiguous', '2025-01-01T03:00:00Z', '2025-01-01T03:00:10Z');",
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let activity_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM activities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(activity_count, 4);
        let unlinked_sessions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE activity_id IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unlinked_sessions, 0);
        let placeholder_only: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM activities
                 WHERE url_key = 'youtube.com/shorts/only-placeholder' AND title = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(placeholder_only, 1);
        let ambiguous_placeholders: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM activities
                 WHERE url_key = 'youtube.com/shorts/ambiguous' AND title = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ambiguous_placeholders, 1);
        let activity_version: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'activity_identity_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(activity_version, "1");
        let foreign_key_violations: Vec<String> = conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(foreign_key_violations.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_compacts_the_three_title_variants_into_one_pinned_activity() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        migrations().to_version(&mut conn, 24).unwrap();
        conn.execute(
            "INSERT INTO categories (name, color) VALUES ('Pinned', '#111111')",
            [],
        )
        .unwrap();
        let category_id = conn.last_insert_rowid();
        conn.execute_batch(&format!(
            "INSERT INTO sessions
                (app_name, exe_path, window_title, url, category_key, category_key_kind, url_key,
                 start_time, end_time, category_id)
             VALUES
                ('Google Chrome', 'chrome.exe', 'YouTube', 'https://www.youtube.com/shorts/abc', 'youtube.com', 'domain', 'youtube.com/shorts/abc', '2025-01-01T00:00:00Z', '2025-01-01T00:00:10Z', {category_id}),
                ('Google Chrome', 'chrome.exe', 'A real title', 'https://www.youtube.com/shorts/abc', 'youtube.com', 'domain', 'youtube.com/shorts/abc', '2025-01-01T01:00:00Z', '2025-01-01T01:00:10Z', NULL),
                ('Google Chrome', 'chrome.exe', 'youtube.com/shorts/abc', 'https://www.youtube.com/shorts/abc', 'youtube.com', 'domain', 'youtube.com/shorts/abc', '2025-01-01T02:00:00Z', '2025-01-01T02:00:10Z', NULL);"
        )).unwrap();
        migrate(&mut conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM activities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let pinned: Option<i64> = conn
            .query_row("SELECT category_id FROM activities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pinned, Some(category_id));
        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT activity_id) FROM sessions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(linked, 1);
        let _ = std::fs::remove_file(path);
    }
}
