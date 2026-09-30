//! The tables behind tracking — `sources`, `activities`, `sessions` — as the recorder needs
//! them: finding or creating the source and activity a block belongs to, and pruning the ones
//! nothing refers to any more.

use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use crate::categorization;
use crate::db::models::{ActivityRow, NewActivityRow, NewSourceRow};
use crate::db::{self};

use super::domain::source_key::SourceKey;

/// Finds the source by its key, or creates it. An existing source keeps its name unless it
/// never had one — a domain's or app's display name can only improve, never be blanked.
pub fn ensure_source(conn: &mut SqliteConnection, source: &SourceKey, name: &str) -> QueryResult<i64> {
    if let Some(existing) = db::sources::SourcesRepo::get_by_key(conn, source.kind.as_str(), &source.key)? {
        if existing.name.is_empty() && !name.is_empty() {
            db::sources::SourcesRepo::set_name(conn, existing.id, name)?;
        }
        return Ok(existing.id);
    }
    let inserted = db::sources::SourcesRepo::insert(
        conn,
        &NewSourceRow {
            kind: source.kind.as_str(),
            key: &source.key,
            name,
        },
    )?;
    Ok(inserted.id)
}

/// Finds the activity by its identity (source, path, title), or creates it.
pub fn ensure_activity(conn: &mut SqliteConnection, source_id: i64, path: &str, title: &str) -> QueryResult<ActivityRow> {
    if let Some(existing) = db::activities::ActivitiesRepo::get_by_identity(conn, source_id, path, title)? {
        return Ok(existing);
    }
    db::activities::ActivitiesRepo::insert(conn, &NewActivityRow { source_id, path, title })
}

/// Deletes every activity among `activity_ids` (and its source, if nothing else uses it) that
/// has no category pin and no session referencing it any more, in a handful of queries
/// regardless of how many ids are given.
pub fn prune_orphans(conn: &mut SqliteConnection, activity_ids: &[i64]) -> QueryResult<()> {
    if activity_ids.is_empty() {
        return Ok(());
    }
    let prunable = db::activities::ActivitiesRepo::get_prunable_many(conn, activity_ids)?;
    if prunable.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = prunable.iter().map(|activity| activity.id).collect();
    let source_ids: Vec<i64> = prunable.iter().map(|activity| activity.source_id).collect();

    categorization::store::discard_labels(conn, &ids)?;
    db::activities::ActivitiesRepo::delete_many(conn, &ids)?;
    db::sources::SourcesRepo::delete_unused_many(conn, &source_ids)?;
    Ok(())
}
