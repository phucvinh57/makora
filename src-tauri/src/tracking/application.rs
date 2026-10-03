use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use diesel::{Connection, QueryResult};

use crate::categorization::store as classifications;
use crate::categorization::ActivityIndexer;
use crate::db::models::NewSessionRow;
use crate::db::{self, Db};
use crate::shared::ids::SessionId;
use crate::settings::SettingsStore;
use crate::shared::time;

use super::domain::events::FocusedApp;
use super::domain::session_policy::{is_noise, should_merge};
use super::domain::source_key::{url_path, SourceKey};
use super::domain::title::canonical_title;
use super::ports::SessionRecorder;
use super::store::{ensure_activity, ensure_source, prune_orphans};

pub struct SqliteSessionRecorder {
    db: Db,
    settings: Arc<dyn SettingsStore>,
    indexer: Arc<dyn ActivityIndexer>,
}

impl SqliteSessionRecorder {
    pub fn new(db: Db, settings: Arc<dyn SettingsStore>, indexer: Arc<dyn ActivityIndexer>) -> Self {
        Self { db, settings, indexer }
    }
}

impl SessionRecorder for SqliteSessionRecorder {
    fn start(&self, app: &FocusedApp, window_title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<SessionId> {
        let source = SourceKey::of(&app.exe_path, url);
        let path = url.map(url_path).unwrap_or_default();
        let canonical = canonical_title(window_title, &app.app_name, url);
        let mut conn = self.db.write();
        let (session_id, created) = conn.transaction(|conn| -> QueryResult<(i64, bool)> {
            let source_id = ensure_source(conn, &source, &source.display_name(&app.app_name))?;
            let (activity, created) = ensure_activity(conn, source_id, &path, &canonical)?;

            if let Some(last) = db::sessions::SessionsRepo::get_last(conn)? {
                if last.activity_id == activity.id {
                    if let Some(end_time) = &last.end_time {
                        if time::parse_db(end_time).is_some_and(|closed_at| should_merge(closed_at, at)) {
                            db::sessions::SessionsRepo::reopen(conn, last.id)?;
                            return Ok((last.id, created));
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
            Ok((session.id, created))
        })?;
        if created {
            self.indexer.refresh();
        }
        Ok(session_id.into())
    }

    fn end(&self, id: SessionId, at: DateTime<Utc>) -> Result<()> {
        let id = i64::from(id);
        let end_time = time::to_db(at);
        // Read per close, so a change on the settings page applies to the very next block.
        let min_seconds = self.settings.get().tracking.min_session_seconds;
        let mut conn = self.db.write();
        conn.transaction(|conn| -> QueryResult<()> {
            let Some(session) = db::sessions::SessionsRepo::get(conn, id)? else {
                return Ok(());
            };
            // A start time that is missing or unreadable is a reason to keep the row, not to
            // throw it away.
            if time::parse_db(&session.start_time).is_some_and(|started_at| is_noise(started_at, at, min_seconds)) {
                db::sessions::SessionsRepo::delete(conn, id)?;
                prune_orphans(conn, &[session.activity_id])?;
                return Ok(());
            }
            db::sessions::SessionsRepo::set_end(conn, id, &end_time)
        })
        .map_err(Into::into)
    }

    fn refine_title(&self, id: SessionId, app: &FocusedApp, window_title: &str, url: Option<&str>) -> Result<()> {
        let id = i64::from(id);
        let mut conn = self.db.write();
        let created = conn.transaction(|conn| -> QueryResult<bool> {
            let Some(session) = db::sessions::SessionsRepo::get(conn, id)? else {
                return Ok(false);
            };
            let Some(old_activity) = db::activities::ActivitiesRepo::get(conn, session.activity_id)? else {
                return Ok(false);
            };
            let canonical = canonical_title(window_title, &app.app_name, url);
            if canonical.is_empty() || !old_activity.title.is_empty() {
                return Ok(false);
            }

            let source = SourceKey::of(&app.exe_path, url);
            let path = url.map(url_path).unwrap_or_default();
            let source_id = ensure_source(conn, &source, &source.display_name(&app.app_name))?;
            let (target, created) = ensure_activity(conn, source_id, &path, &canonical)?;

            // The session moves onto the target activity, so the user's pin moves with it —
            // unless the target already has a different one, in which case it stays put.
            let before = classifications::load_many(conn, &[old_activity.id.into(), target.id.into()])?;
            let mut after = before.clone();
            let Some((old, [target_classification])) = after.split_first_mut() else {
                return Ok(created);
            };
            if old.carry_pin_into(target_classification).is_err() {
                return Ok(created);
            }
            classifications::save_changes(conn, &before, &after)?;

            db::sessions::SessionsRepo::reassign_activity(conn, id, target.id)?;
            prune_orphans(conn, &[old_activity.id])?;
            Ok(created)
        })?;
        if created {
            self.indexer.refresh();
        }
        Ok(())
    }

    fn close_dangling(&self) -> Result<()> {
        let mut conn = self.db.write();
        conn.transaction(|conn| -> QueryResult<()> {
            let activity_ids = db::sessions::SessionsRepo::get_open_activity_ids(conn)?;
            db::sessions::SessionsRepo::delete_open(conn)?;
            prune_orphans(conn, &activity_ids)?;
            Ok(())
        })
        .map_err(Into::into)
    }
}
