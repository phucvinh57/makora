use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use rusqlite_migration::{Migrations, M};

use crate::core::category::category_key;
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
        rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
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
    let mut conn = Connection::open(path).with_context(|| format!("opening database at {}", path.display()))?;
    register_regexp_function(&conn).context("registering regexp SQL function")?;
    migrations().to_latest(&mut conn).context("migrating schema")?;
    seed_default_categories(&conn).context("seeding default categories")?;
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

fn seed_default_categories(conn: &Connection) -> Result<()> {
    let already_seeded: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM categories)", [], |row| row.get(0))?;
    if already_seeded {
        return Ok(());
    }

    // (name, color, exe rules, domain rules)
    const DEFAULTS: &[(&str, &str, &[&str], &[&str])] = &[
        (
            "Development",
            "#6366f1",
            &["code.exe", "devenv.exe", "pycharm64.exe", "windowsterminal.exe", "powershell.exe", "pwsh.exe", "cmd.exe"],
            &["github.com", "gitlab.com", "stackoverflow.com", "developer.mozilla.org"],
        ),
        (
            "Communication",
            "#22c55e",
            &["slack.exe", "discord.exe", "teams.exe", "outlook.exe", "whatsapp.exe", "whatsapp.root.exe", "signal.exe"],
            &["mail.google.com", "outlook.office.com"],
        ),
        (
            "Entertainment",
            "#f97316",
            &["spotify.exe", "steam.exe", "vlc.exe"],
            &["youtube.com", "netflix.com", "twitch.tv", "open.spotify.com"],
        ),
        (
            "Social Media",
            "#ec4899",
            &[],
            &["twitter.com", "x.com", "facebook.com", "instagram.com", "reddit.com"],
        ),
        (
            "Productivity",
            "#0ea5e9",
            &["winword.exe", "excel.exe", "powerpnt.exe", "notion.exe"],
            &["notion.so", "docs.google.com"],
        ),
    ];

    for (name, color, exe_patterns, domain_patterns) in DEFAULTS {
        conn.execute("INSERT INTO categories (name, color) VALUES (?1, ?2)", params![name, color])?;
        let category_id = conn.last_insert_rowid();
        for pattern in *exe_patterns {
            conn.execute(
                "INSERT INTO category_rules (key_kind, pattern, category_id) VALUES ('exe', ?1, ?2)",
                params![pattern, category_id],
            )?;
        }
        for pattern in *domain_patterns {
            conn.execute(
                "INSERT INTO category_rules (key_kind, pattern, category_id) VALUES ('domain', ?1, ?2)",
                params![pattern, category_id],
            )?;
        }
    }
    Ok(())
}

impl SessionStore for SqliteStore {
    fn start_session(&mut self, app: &AppInfo, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<i64> {
        let (key_kind, key) = category_key(&app.exe_path, url);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (app_name, exe_path, window_title, url, category_key, category_key_kind, start_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![app.app_name, app.exe_path, window_title, url, key, key_kind.as_str(), at.to_rfc3339()],
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
        self.conn.lock().unwrap().execute(
            "DELETE FROM sessions WHERE end_time IS NULL",
            [],
        )?;
        Ok(())
    }
}
