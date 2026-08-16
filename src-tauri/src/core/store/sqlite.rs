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
            -- Built-in categorization: matches a session's (key_kind, category_key)
            -- against an exe file name or a URL domain.
            CREATE TABLE IF NOT EXISTS category_rules (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                key_kind    TEXT NOT NULL CHECK (key_kind IN ('exe', 'domain')),
                pattern     TEXT NOT NULL,
                category_id INTEGER NOT NULL REFERENCES categories(id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_rules_key ON category_rules(key_kind, pattern);
            -- User-set categorization, keyed the same way as `category_rules`. Checked
            -- first so a manual choice always wins over the built-in rules.
            CREATE TABLE IF NOT EXISTS category_overrides (
                key_kind    TEXT NOT NULL CHECK (key_kind IN ('exe', 'domain')),
                pattern     TEXT NOT NULL,
                category_id INTEGER NOT NULL REFERENCES categories(id),
                PRIMARY KEY (key_kind, pattern)
            );",
        ),
        // Added after the categories/rules/overrides step above (rather than
        // reordered before it) so that a database which already applied that
        // step picks up this one too instead of being skipped as
        // already-migrated.
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
        // Per-session category pin, checked before the key-wide override and
        // rules — lets a single activity be categorized on its own instead of
        // always affecting every session sharing its app/site key.
        M::up("ALTER TABLE sessions ADD COLUMN category_id INTEGER REFERENCES categories(id);"),
        // User-configured apps/sites to exclude from tracking entirely (see
        // `core::ignore::IgnorePolicy`). Matched as a substring of the exe
        // file name or URL domain, not an exact key like `category_rules`.
        M::up("CREATE TABLE IF NOT EXISTS ignore_rules (
                id      INTEGER PRIMARY KEY AUTOINCREMENT,
                kind    TEXT NOT NULL CHECK (kind IN ('app', 'site')),
                pattern TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ignore_rules_pattern ON ignore_rules(kind, pattern);",
        ),
        // Lets a rule match a substring or a regex instead of only an exact
        // key (see `category::rule_join_sql`), so e.g. one rule can catch
        // every `*.corp.example.com` subdomain instead of one row per host.
        // The old (key_kind, pattern) unique index is replaced with one that
        // also includes match_kind, since the same pattern text can now be
        // meaningful under more than one match kind at once.
        M::up("ALTER TABLE category_rules ADD COLUMN match_kind TEXT NOT NULL DEFAULT 'exact'
                CHECK (match_kind IN ('exact', 'substring', 'regex'));
            DROP INDEX IF EXISTS idx_category_rules_key;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_category_rules_key
                ON category_rules(key_kind, match_kind, pattern);",
        ),
        // Gives `ignore_rules` the same exact/substring/regex matching as
        // `category_rules` (see `category::MatchKind`) instead of only ever
        // matching a substring. Existing rows default to 'substring' so
        // their behavior is unchanged by the migration. The old (kind,
        // pattern) unique index is replaced the same way the category_rules
        // one was above.
        M::up("ALTER TABLE ignore_rules ADD COLUMN match_kind TEXT NOT NULL DEFAULT 'substring'
                CHECK (match_kind IN ('exact', 'substring', 'regex'));
            DROP INDEX IF EXISTS idx_ignore_rules_pattern;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ignore_rules_pattern
                ON ignore_rules(kind, match_kind, pattern);",
        ),
        // Superseded by the next migration: per-rule priority turned out to
        // be finer-grained than needed (see below), but this step may
        // already be applied on some databases, so it's kept rather than
        // edited in place — never rewrite a migration once it may have run.
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
        // Lets the winner between two rules that both match the same
        // session key (e.g. a `Substring` rule in one category and a
        // narrower one in another, both matching an overlapping domain) be
        // manually ordered from the Categories page — by dragging whole
        // categories, not individual rules, since which specific rule wins
        // never matters on its own (only its category does; see
        // `category::rule_join_sql` and `CategoryStore::reorder_categories`).
        // Lower `priority` wins; backfilled here in category creation order
        // as the best available default — unlike the per-rule priority it
        // replaces, this can't exactly reproduce every key's prior
        // resolution (that depended on match-kind specificity per rule, not
        // just category), so some already-ambiguous keys may resolve to a
        // different category until manually reordered.
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
        // Replaces category priority ordering with pure specificity-based
        // rule resolution (see `category::rule_join_sql`): a `url` rule now
        // beats a `domain`/`exe` rule, then match kind ranks `exact` >
        // `prefix` > `substring` > `regex`, then longer pattern wins. This
        // makes per-category ordering unnecessary — a rule pattern already
        // belongs to exactly one category (`idx_category_rules_key`) — so
        // `categories.priority` and the drag-to-reorder UI it backed are
        // removed entirely.
        //
        // `category_rules`' CHECK constraints can't be altered in place, so
        // the table is rebuilt to add the `url` key_kind and `prefix` match
        // kind rather than just adding a column as earlier migrations did.
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
            -- Normalized URL match target for `url` rules (see
            -- `category::url_key`), computed at write time like
            -- `category_key`. NULL for non-browser rows.
            ALTER TABLE sessions ADD COLUMN url_key TEXT;",
        ),
        // Lets query-string params be selectively kept per domain instead of
        // always stripped (see `ingest::normalize_url` and
        // `store::QueryWhitelistStore`) — e.g. keeping YouTube's `v` so
        // different videos become different sessions rather than one
        // continuous `youtube.com` block. Seeded here (rather than in
        // `seed_default_categories`-style startup logic) so a user who
        // deliberately empties the list isn't re-seeded on next launch.
        M::up("CREATE TABLE IF NOT EXISTS url_query_whitelist (
                id     INTEGER PRIMARY KEY AUTOINCREMENT,
                domain TEXT NOT NULL,
                param  TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_url_query_whitelist ON url_query_whitelist(domain, param);
            INSERT INTO url_query_whitelist (domain, param) VALUES ('youtube.com', 'v');",
        ),
        // User-created focus blocks (see `store::WorkSessionStore`), evaluated
        // after the fact against tracked `sessions` rows overlapping
        // [start_time, end_time). `end_time` is always set — computed as
        // start + target at creation (the session "auto-ends" by simply
        // reaching it) and rewritten on an early stop or a manual edit.
        // `target_seconds` stays as the original plan for comparing planned
        // vs. actual. The join table lists which categories count as
        // "on-task"; rows are cascaded manually in `WorkSessionStore` /
        // `CategoryStore::delete_category` since the connection doesn't
        // enable SQLite foreign-key enforcement.
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
        // Retires the key-wide override tier and the per-domain query
        // whitelist, leaving `category_rules` as the only thing that decides
        // a category (plus `sessions.category_id` as a single-activity
        // escape hatch). The override tier was the "apply to this app/site
        // everywhere" hint, and it worked by minting a mirror `category_rules`
        // row alongside each override — a pairing that only existed to make
        // the clicked-on app/site visible on the Categories page, and that
        // `recategorize_all` then had to unpick. With rules as the sole
        // mechanism there is nothing left to mirror.
        //
        // Every hand-curated row goes with it: the existing categories and
        // rules were seeded from a small Windows-centric default list that a
        // generated rule set replaces wholesale, so keeping them would just
        // leave history resolved half by the old taxonomy and half by the new
        // one. Everything referencing `categories` is cleared before the
        // categories themselves — `work_session_categories` rows and the
        // `sessions.category_id` pins alike. Foreign keys are off at runtime
        // but enforced while migrating, so that ordering is load-bearing, not
        // just tidy. Tracked `sessions`, `work_sessions` and `ignore_rules`
        // rows are all left alone. The sequence reset keeps ids starting from
        // 1 for the reseed.
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

/// Registers the `regexp` scalar function SQLite dispatches `X REGEXP Y`
/// to (as `regexp(Y, X)` — pattern first, then the text being tested), so
/// `category::rule_join_sql`'s `Regex` match arm has an operator to use.
/// Matching is case-insensitive, same convention as the `Substring` arm's
/// `lower()`/`instr()`. An invalid pattern (e.g. a rule someone is still
/// mid-edit on) simply never matches rather than erroring the whole query.
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

/// A `SessionStore` backed by a local SQLite file. Timestamps are stored as
/// RFC 3339 strings in UTC.
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
        // A session still open at startup means the previous run never saw
        // its end (crash, kill, power loss). We can't know the real end
        // time, so close it at its own start rather than "now" — that
        // undercounts one session instead of silently inflating it by
        // however long the machine was off.
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

    /// Simulates an install upgrading across the migration that replaced
    /// per-rule `priority` with per-category `priority` (see `migrations`):
    /// stops at the pre-existing schema, inserts categories the way an
    /// older build's `create_category` would have (no `priority` column in
    /// play yet), then migrates through that step and checks the backfill
    /// assigned priority in category creation (id) order — the best
    /// available default described in that migration's comment. `priority`
    /// was itself later dropped entirely (rules now resolve by specificity,
    /// see `category::rule_join_sql`), so this only pins down the
    /// now-historical backfill behavior rather than any current-schema
    /// guarantee.
    #[test]
    fn category_priority_backfill_orders_by_creation_id_ascending() {
        let path = temp_db_path();
        let mut conn = Connection::open(&path).unwrap();
        register_regexp_function(&conn).unwrap();
        let migs = migrations();
        // Version 7: through the superseded per-rule `priority` migration,
        // one short of the per-category `priority` migration being tested.
        migs.to_version(&mut conn, 7).unwrap();
        conn.execute_batch(
            "INSERT INTO categories (name, color) VALUES ('First', '#111111');
             INSERT INTO categories (name, color) VALUES ('Second', '#222222');
             INSERT INTO categories (name, color) VALUES ('Third', '#333333');",
        )
        .unwrap();

        // Version 8: through the per-category `priority` backfill, one short
        // of the migration that drops `priority` again.
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

    /// After migrating to latest, `categories.priority` is gone and
    /// `category_rules` accepts the new `url` key_kind and `prefix`
    /// match_kind (its CHECK constraints were rebuilt, not just extended).
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
