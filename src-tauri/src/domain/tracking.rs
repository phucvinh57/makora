//! Turns focus/tab events into `sessions` rows: opens one when an app or tab gains focus,
//! closes it when it loses focus, and folds a browser's tab-title update into the row already
//! open for that tab.

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::sqlite::SqliteConnection;
use diesel::{Connection, QueryResult};

use crate::categorization::store as classifications;
use crate::db::models::{ActivityRow, NewActivityRow, NewSessionRow, NewSourceRow};
use crate::db::{self, Db};
use crate::focus::AppInfo;
use crate::shared::ids::SessionId;
use crate::shared::time;
use crate::tracking::domain::source_key::{url_path, SourceKey};
use crate::tracking::domain::title::canonical_title;

/// A focus or tab change that is undone within a second never was activity: a page redirects,
/// a browser reports a tab before its title has loaded and corrects itself, a window flashes
/// past on the way somewhere else. Rows that short are deleted instead of closed, so no query
/// downstream has to filter by duration.
const MIN_SESSION_SECONDS: i64 = 1;
/// A block that reopens within this long of closing is the same activity resuming (alt-tab
/// away and back, a window manager hiccup), not a new one starting.
const MERGE_GAP_SECONDS: i64 = 2;

pub use super::traits::TrackingSvc;

pub struct SqliteTracking {
    db: Db,
}

impl SqliteTracking {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl TrackingSvc for SqliteTracking {
    fn start(&self, app: &AppInfo, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<SessionId> {
        let source = SourceKey::of(&app.exe_path, url);
        let path = url.map(url_path).unwrap_or_default();
        let canonical = canonical_title(window_title, &app.app_name, url);
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> QueryResult<i64> {
            let source_id = ensure_source(conn, &source, &source.display_name(&app.app_name))?;
            let activity = ensure_activity(conn, source_id, &path, &canonical)?;

            if let Some(last) = db::sessions::SessionsRepo::get_last(conn)? {
                if last.activity_id == activity.id {
                    if let Some(end_time) = &last.end_time {
                        if within_merge_gap(end_time, at) {
                            db::sessions::SessionsRepo::reopen(conn, last.id)?;
                            return Ok(last.id);
                        }
                    }
                }
            }

            let session = db::sessions::SessionsRepo::insert(
                conn,
                &NewSessionRow {
                    activity_id: activity.id,
                    start_time: &time::to_db(at),
                },
            )?;
            Ok(session.id)
        })
        .map(SessionId::from)
        .map_err(Into::into)
    }

    fn end(&self, id: SessionId, at: DateTime<Utc>) -> Result<()> {
        let id = i64::from(id);
        let end_time = time::to_db(at);
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> QueryResult<()> {
            let Some(session) = db::sessions::SessionsRepo::get(conn, id)? else {
                return Ok(());
            };
            if is_too_short(&session.start_time, at) {
                db::sessions::SessionsRepo::delete(conn, id)?;
                prune_orphans(conn, &[session.activity_id])?;
                return Ok(());
            }
            db::sessions::SessionsRepo::set_end(conn, id, &end_time)
        })
        .map_err(Into::into)
    }

    fn refine_title(&self, id: SessionId, app: &AppInfo, window_title: &str, url: Option<&str>) -> Result<()> {
        let id = i64::from(id);
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> QueryResult<()> {
            let Some(session) = db::sessions::SessionsRepo::get(conn, id)? else {
                return Ok(());
            };
            let Some(old_activity) = db::activities::ActivitiesRepo::get(conn, session.activity_id)? else {
                return Ok(());
            };
            let canonical = canonical_title(window_title, &app.app_name, url);
            if canonical.is_empty() || !old_activity.title.is_empty() {
                return Ok(());
            }

            let source = SourceKey::of(&app.exe_path, url);
            let path = url.map(url_path).unwrap_or_default();
            let source_id = ensure_source(conn, &source, &source.display_name(&app.app_name))?;
            let target = ensure_activity(conn, source_id, &path, &canonical)?;

            // The session moves onto the target activity, so the user's pin moves with it —
            // unless the target already has a different one, in which case it stays put.
            let before = classifications::load_many(conn, &[old_activity.id.into(), target.id.into()])?;
            let mut after = before.clone();
            let Some((old, [target_classification])) = after.split_first_mut() else {
                return Ok(());
            };
            if old.carry_pin_into(target_classification).is_err() {
                return Ok(());
            }
            classifications::save_changes(conn, &before, &after)?;

            db::sessions::SessionsRepo::reassign_activity(conn, id, target.id)?;
            prune_orphans(conn, &[old_activity.id])?;
            Ok(())
        })
        .map_err(Into::into)
    }

    fn close_dangling(&self) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> QueryResult<()> {
            let activity_ids = db::sessions::SessionsRepo::get_open_activity_ids(conn)?;
            db::sessions::SessionsRepo::delete_open(conn)?;
            prune_orphans(conn, &activity_ids)?;
            Ok(())
        })
        .map_err(Into::into)
    }
}

/// Finds the source by its key, or creates it. An existing source keeps its name unless it
/// never had one — a domain's or app's display name can only improve, never be blanked.
fn ensure_source(conn: &mut SqliteConnection, source: &SourceKey, name: &str) -> QueryResult<i64> {
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
fn ensure_activity(conn: &mut SqliteConnection, source_id: i64, path: &str, title: &str) -> QueryResult<ActivityRow> {
    if let Some(existing) = db::activities::ActivitiesRepo::get_by_identity(conn, source_id, path, title)? {
        return Ok(existing);
    }
    db::activities::ActivitiesRepo::insert(conn, &NewActivityRow { source_id, path, title })
}

/// Deletes every activity among `activity_ids` (and its source, if nothing else uses it) that
/// has no category pin and no session referencing it any more, in a handful of queries
/// regardless of how many ids are given.
fn prune_orphans(conn: &mut SqliteConnection, activity_ids: &[i64]) -> QueryResult<()> {
    if activity_ids.is_empty() {
        return Ok(());
    }
    let prunable = db::activities::ActivitiesRepo::get_prunable_many(conn, activity_ids)?;
    if prunable.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = prunable.iter().map(|activity| activity.id).collect();
    let source_ids: Vec<i64> = prunable.iter().map(|activity| activity.source_id).collect();

    db::ai_labels::AiLabelsRepo::delete_for_activities(conn, &ids)?;
    db::activities::ActivitiesRepo::delete_many(conn, &ids)?;
    db::sources::SourcesRepo::delete_unused_many(conn, &source_ids)?;
    Ok(())
}

fn within_merge_gap(closed_at: &str, resumed_at: DateTime<Utc>) -> bool {
    time::parse_db(closed_at)
        .map(|end| resumed_at.signed_duration_since(end).num_milliseconds())
        .is_some_and(|gap| (0..=MERGE_GAP_SECONDS * 1000).contains(&gap))
}

/// A row whose `start_time` is missing or unparseable is left alone — an unreadable timestamp
/// is a reason to keep the row, not to throw it away.
fn is_too_short(start_time: &str, at: DateTime<Utc>) -> bool {
    let Some(start) = time::parse_db(start_time) else {
        return false;
    };
    at.signed_duration_since(start).num_milliseconds() < MIN_SESSION_SECONDS * 1000
}
