use std::collections::HashSet;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::core::category::{
    category_key, compile_rules, resolve_category, CompiledRule, KeyKind, MatchKind, RuleSpec,
    SessionCategoryInput,
};
use crate::core::title::canonical_title;

#[derive(Debug)]
struct StoredActivity {
    id: i64,
    key_kind: KeyKind,
    key: String,
    url_key: Option<String>,
}

struct ResolutionContext {
    rules: Vec<CompiledRule>,
    multi_purpose_keys: HashSet<(KeyKind, String)>,
}

/// Re-derives `rule_category_id` for every activity in Rust.
///
/// Callers pass their transaction so changing a rule or setting and replaying history remain
/// one atomic operation. SQLite only supplies stored inputs and persists the computed result.
pub fn resolve_all(conn: &Connection) -> Result<usize> {
    // A few older callers and imported databases can still create a session row directly.
    // Give those rows their activity identity before replaying rules so the activity table
    // remains the single source of derived labels even while the legacy columns are retained.
    let legacy_session_ids = conn
        .prepare("SELECT id FROM sessions WHERE activity_id IS NULL ORDER BY id")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for session_id in legacy_session_ids {
        ensure_activity_for_session(conn, session_id)?;
    }
    let context = load_context(conn)?;
    let activities = load_activities(conn)?;
    let rows_resolved = activities.len();
    let mut update = conn.prepare(
        "UPDATE activities SET rule_category_id = ?1
         WHERE id = ?2 AND rule_category_id IS NOT ?1",
    )?;
    for activity in activities {
        let resolved = resolve_stored_activity(&context, &activity);
        update.execute(params![resolved, activity.id])?;
    }
    conn.execute(
        "UPDATE sessions SET rule_category_id = (
             SELECT rule_category_id FROM activities WHERE activities.id = sessions.activity_id
         ) WHERE activity_id IS NOT NULL",
        [],
    )?;
    Ok(rows_resolved)
}

/// Re-derives one stored session, used immediately after opening a row and by tests that insert
/// fixtures directly.
pub fn resolve_activity(conn: &Connection, id: i64) -> Result<()> {
    let resolved = resolve_activity_category(conn, id)?;
    conn.execute(
        "UPDATE activities SET rule_category_id = ?1 WHERE id = ?2",
        params![resolved, id],
    )?;
    conn.execute(
        "UPDATE sessions SET rule_category_id = ?1 WHERE activity_id = ?2",
        params![resolved, id],
    )?;
    Ok(())
}

/// Computes one session's automatic category without writing it.
pub fn resolve_activity_category(conn: &Connection, id: i64) -> Result<Option<i64>> {
    let context = load_context(conn)?;
    let activity = load_activity(conn, id)?.with_context(|| format!("activity {id} not found"))?;
    Ok(resolve_stored_activity(&context, &activity))
}

// Compatibility aliases for older callers and fixtures. New code resolves activities directly.
#[allow(dead_code)]
pub fn resolve_session(conn: &Connection, id: i64) -> Result<()> {
    let activity_id = conn
        .query_row(
            "SELECT activity_id FROM sessions WHERE id = ?1",
            params![id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    let activity_id = match activity_id {
        Some(activity_id) => activity_id,
        None => ensure_activity_for_session(conn, id)?,
    };
    resolve_activity(conn, activity_id)
}

#[allow(dead_code)]
pub fn resolve_session_category(conn: &Connection, id: i64) -> Result<Option<i64>> {
    let activity_id = conn
        .query_row(
            "SELECT activity_id FROM sessions WHERE id = ?1",
            params![id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    match activity_id {
        Some(id) => resolve_activity_category(conn, id),
        None => Ok(None),
    }
}

fn ensure_activity_for_session(conn: &Connection, id: i64) -> Result<i64> {
    let (app_name, exe_path, window_title, url, key_kind, key, url_key_value, start_time): (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        String,
    ) = conn.query_row(
        "SELECT app_name, exe_path, window_title, url, category_key_kind, category_key,
                url_key, start_time FROM sessions WHERE id = ?1",
        params![id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        },
    )?;
    let (computed_kind, computed_key) = category_key(&exe_path, url.as_deref());
    let kind = KeyKind::parse(&key_kind).unwrap_or(computed_kind);
    let key = if key.is_empty() { computed_key } else { key };
    let canonical = canonical_title(&window_title, &app_name, url.as_deref());
    conn.execute(
        "INSERT INTO activities (key_kind, key, url_key, title, display_title, app_name, url, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT DO NOTHING",
        params![kind.as_str(), key, url_key_value, canonical, window_title, app_name, url, start_time],
    )?;
    let activity_id: i64 = conn.query_row(
        "SELECT id FROM activities WHERE key_kind = ?1 AND key = ?2
         AND COALESCE(url_key, '') = COALESCE(?3, '') AND title = ?4",
        params![kind.as_str(), key, url_key_value, canonical],
        |row| row.get(0),
    )?;
    conn.execute(
        "UPDATE sessions SET activity_id = ?1 WHERE id = ?2",
        params![activity_id, id],
    )?;
    // Direct legacy inserts may still carry their labels on the session row. Lift them when the
    // compatibility path first materializes the activity so classifier inputs do not mistake a
    // manually labeled occurrence for an unlabeled activity.
    conn.execute(
        "UPDATE activities SET
             category_id = COALESCE(category_id, (SELECT category_id FROM sessions WHERE id = ?1)),
             rule_category_id = COALESCE(rule_category_id, (SELECT rule_category_id FROM sessions WHERE id = ?1)),
             ml_category_id = COALESCE(ml_category_id, (SELECT ml_category_id FROM sessions WHERE id = ?1)),
             ml_confidence = COALESCE(ml_confidence, (SELECT ml_confidence FROM sessions WHERE id = ?1)),
             ml_model_version = COALESCE(ml_model_version, (SELECT ml_model_version FROM sessions WHERE id = ?1))
         WHERE id = ?2",
        params![id, activity_id],
    )?;
    Ok(activity_id)
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

fn load_activities(conn: &Connection) -> Result<Vec<StoredActivity>> {
    let mut stmt = conn.prepare(
        "SELECT id, key_kind, key, url_key
         FROM activities",
    )?;
    let rows = stmt
        .query_map([], stored_activity_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn load_activity(conn: &Connection, id: i64) -> Result<Option<StoredActivity>> {
    Ok(conn
        .query_row(
            "SELECT id, key_kind, key, url_key
             FROM activities WHERE id = ?1",
            params![id],
            stored_activity_from_row,
        )
        .optional()?)
}

fn stored_activity_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredActivity> {
    let raw_kind: String = row.get(1)?;
    Ok(StoredActivity {
        id: row.get(0)?,
        // The schema constrains this value. Falling back keeps a manually repaired legacy row
        // resolvable instead of making every future rule edit fail.
        key_kind: KeyKind::parse(&raw_kind).unwrap_or(KeyKind::Exe),
        key: row.get(2)?,
        url_key: row.get(3)?,
    })
}

fn resolve_stored_activity(context: &ResolutionContext, session: &StoredActivity) -> Option<i64> {
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
