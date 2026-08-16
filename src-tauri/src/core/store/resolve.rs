use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::core::category::{
    compile_rules, resolve_category, CompiledRule, KeyKind, MatchKind, RuleSpec,
    SessionCategoryInput, DEFAULT_DISTRACTION_THRESHOLD_SECONDS, DISTRACTION_SYSTEM_KEY,
    DISTRACTION_THRESHOLD_KEY,
};

#[derive(Debug)]
struct StoredSession {
    id: i64,
    key_kind: KeyKind,
    key: String,
    url_key: Option<String>,
    start_time: String,
    end_time: Option<String>,
}

struct ResolutionContext {
    rules: Vec<CompiledRule>,
    distraction_threshold_seconds: i64,
    distraction_category_id: i64,
}

/// Re-derives `rule_category_id` for every session in Rust.
///
/// Callers pass their transaction so changing a rule or setting and replaying history remain
/// one atomic operation. SQLite only supplies stored inputs and persists the computed result.
pub fn resolve_all(conn: &Connection) -> Result<usize> {
    let context = load_context(conn)?;
    let sessions = load_sessions(conn)?;
    let rows_resolved = sessions.len();
    let mut update = conn.prepare(
        "UPDATE sessions SET rule_category_id = ?1
         WHERE id = ?2 AND rule_category_id IS NOT ?1",
    )?;
    for session in sessions {
        let resolved = resolve_stored_session(&context, &session, None);
        update.execute(params![resolved, session.id])?;
    }
    Ok(rows_resolved)
}

/// Re-derives one stored session, used immediately after opening a row and by tests that insert
/// fixtures directly.
pub fn resolve_session(conn: &Connection, id: i64) -> Result<()> {
    let resolved = resolve_session_category(conn, id, None)?;
    conn.execute(
        "UPDATE sessions SET rule_category_id = ?1 WHERE id = ?2",
        params![resolved, id],
    )?;
    Ok(())
}

/// Computes one session's automatic category without writing it. `end_time_override` lets
/// `SqliteStore::end_session` decide the closing row before persisting both fields together.
pub fn resolve_session_category(
    conn: &Connection,
    id: i64,
    end_time_override: Option<&str>,
) -> Result<Option<i64>> {
    let context = load_context(conn)?;
    let session = load_session(conn, id)?.with_context(|| format!("session {id} not found"))?;
    Ok(resolve_stored_session(
        &context,
        &session,
        end_time_override,
    ))
}

fn load_context(conn: &Connection) -> Result<ResolutionContext> {
    let mut stmt = conn.prepare(
        "SELECT id, key_kind, pattern, match_kind, category_id
         FROM category_rules",
    )?;
    let stored = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rules = compile_rules(stored.into_iter().filter_map(
        |(id, key_kind, pattern, match_kind, category_id)| {
            Some(RuleSpec {
                id,
                key_kind: KeyKind::parse(&key_kind)?,
                pattern,
                match_kind: MatchKind::parse(&match_kind)?,
                category_id,
            })
        },
    ));

    let raw_threshold: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![DISTRACTION_THRESHOLD_KEY],
            |row| row.get(0),
        )
        .optional()?;
    let distraction_threshold_seconds = raw_threshold
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_DISTRACTION_THRESHOLD_SECONDS);
    let distraction_category_id = conn.query_row(
        "SELECT id FROM categories WHERE system_key = ?1",
        params![DISTRACTION_SYSTEM_KEY],
        |row| row.get(0),
    )?;

    Ok(ResolutionContext {
        rules,
        distraction_threshold_seconds,
        distraction_category_id,
    })
}

fn load_sessions(conn: &Connection) -> Result<Vec<StoredSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, category_key_kind, category_key, url_key, start_time, end_time
         FROM sessions",
    )?;
    let rows = stmt
        .query_map([], stored_session_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn load_session(conn: &Connection, id: i64) -> Result<Option<StoredSession>> {
    Ok(conn
        .query_row(
            "SELECT id, category_key_kind, category_key, url_key, start_time, end_time
             FROM sessions WHERE id = ?1",
            params![id],
            stored_session_from_row,
        )
        .optional()?)
}

fn stored_session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSession> {
    let raw_kind: String = row.get(1)?;
    Ok(StoredSession {
        id: row.get(0)?,
        // The schema constrains this value. Falling back keeps a manually repaired legacy row
        // resolvable instead of making every future rule edit fail.
        key_kind: KeyKind::parse(&raw_kind).unwrap_or(KeyKind::Exe),
        key: row.get(2)?,
        url_key: row.get(3)?,
        start_time: row.get(4)?,
        end_time: row.get(5)?,
    })
}

fn resolve_stored_session(
    context: &ResolutionContext,
    session: &StoredSession,
    end_time_override: Option<&str>,
) -> Option<i64> {
    resolve_category(
        &context.rules,
        &SessionCategoryInput {
            key_kind: session.key_kind,
            key: &session.key,
            url_key: session.url_key.as_deref(),
            start_time: &session.start_time,
            end_time: end_time_override.or(session.end_time.as_deref()),
        },
        context.distraction_threshold_seconds,
        context.distraction_category_id,
    )
}
