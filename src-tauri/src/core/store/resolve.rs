use std::collections::HashSet;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::core::category::{
    compile_rules, resolve_category, CompiledRule, KeyKind, MatchKind, RuleSpec,
    SessionCategoryInput,
};

#[derive(Debug)]
struct StoredSession {
    id: i64,
    key_kind: KeyKind,
    key: String,
    url_key: Option<String>,
}

struct ResolutionContext {
    rules: Vec<CompiledRule>,
    multi_purpose_keys: HashSet<(KeyKind, String)>,
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
        let resolved = resolve_stored_session(&context, &session);
        update.execute(params![resolved, session.id])?;
    }
    Ok(rows_resolved)
}

/// Re-derives one stored session, used immediately after opening a row and by tests that insert
/// fixtures directly.
pub fn resolve_session(conn: &Connection, id: i64) -> Result<()> {
    let resolved = resolve_session_category(conn, id)?;
    conn.execute(
        "UPDATE sessions SET rule_category_id = ?1 WHERE id = ?2",
        params![resolved, id],
    )?;
    Ok(())
}

/// Computes one session's automatic category without writing it.
pub fn resolve_session_category(conn: &Connection, id: i64) -> Result<Option<i64>> {
    let context = load_context(conn)?;
    let session = load_session(conn, id)?.with_context(|| format!("session {id} not found"))?;
    Ok(resolve_stored_session(&context, &session))
}

/// Loads every stored rule in the same specificity order the resolver applies.
///
/// The classifier reuses this so a suggestion is judged against the rules that will actually
/// label the key, globs included, rather than against an exact-pattern lookup.
pub(crate) fn load_rules(conn: &Connection) -> Result<Vec<CompiledRule>> {
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
    Ok(compile_rules(stored.into_iter().filter_map(
        |(id, key_kind, pattern, match_kind, category_id)| {
            Some(RuleSpec {
                id,
                key_kind: KeyKind::parse(&key_kind)?,
                pattern,
                match_kind: MatchKind::parse(&match_kind)?,
                category_id,
            })
        },
    )))
}

fn load_context(conn: &Connection) -> Result<ResolutionContext> {
    let rules = load_rules(conn)?;

    let mut multi_purpose_keys = HashSet::new();
    let mut stmt =
        conn.prepare("SELECT key_kind, key FROM multi_purpose_keys WHERE enabled = 1")?;
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (key_kind, key) = row?;
        if let Some(key_kind) = KeyKind::parse(&key_kind) {
            multi_purpose_keys.insert((key_kind, key));
        }
    }

    Ok(ResolutionContext {
        rules,
        multi_purpose_keys,
    })
}

fn load_sessions(conn: &Connection) -> Result<Vec<StoredSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, category_key_kind, category_key, url_key
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
            "SELECT id, category_key_kind, category_key, url_key
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
    })
}

fn resolve_stored_session(context: &ResolutionContext, session: &StoredSession) -> Option<i64> {
    let multi_purpose = context
        .multi_purpose_keys
        .contains(&(session.key_kind, session.key.clone()));
    resolve_category(
        &context.rules,
        &SessionCategoryInput {
            key_kind: session.key_kind,
            key: &session.key,
            url_key: session.url_key.as_deref(),
            multi_purpose,
        },
    )
}
