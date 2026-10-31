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
use super::domain::exclusion::ExclusionSet;
use super::domain::session_policy::{is_noise, settle_dangling, should_merge, DanglingRow};
use super::domain::source_key::{url_path, SourceKey};
use super::domain::title::canonical_title;
use super::ports::{ExclusionPolicy, SessionRecorder};
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
    fn start(&self, app: &FocusedApp, window_title: &str, url: Option<&str>, at_time: DateTime<Utc>) -> Result<SessionId> {
        let source = SourceKey::of(&app.exe_path, url);
        let path = url.map(url_path).unwrap_or_default();
        let canonical = canonical_title(window_title, &app.app_name, url);
        let at = time::to_db(at_time);
        let mut conn = self.db.write();
        let (session_id, created) = conn.transaction(|conn| -> QueryResult<(i64, bool)> {
            let source_id = ensure_source(conn, &source, &source.display_name(&app.app_name))?;
            let (activity, created) = ensure_activity(conn, source_id, &path, &canonical)?;

            if let Some(last) = db::sessions::SessionsRepo::get_last(conn)? {
                if last.activity_id == activity.id {
                    if let Some(end_time) = &last.end_time {
                        if time::parse_db(end_time).is_some_and(|closed_at| should_merge(closed_at, at_time)) {
                            db::sessions::SessionsRepo::reopen(conn, last.id, &at)?;
                            return Ok((last.id, created));
                        }
                    }
                }
            }

            let session = db::sessions::SessionsRepo::insert(
                conn,
                &NewSessionRow {
                    activity_id: activity.id,
                    start_time: &at,
                    last_seen: &at,
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

    fn touch(&self, id: SessionId, at: DateTime<Utc>) -> Result<()> {
        db::sessions::SessionsRepo::touch(&mut *self.db.write(), id.into(), &time::to_db(at))?;
        Ok(())
    }

    fn close_dangling(&self) -> Result<()> {
        let min_seconds = self.settings.get().tracking.min_session_seconds;
        let mut conn = self.db.write();
        conn.transaction(|conn| -> QueryResult<()> {
            let mut dropped = Vec::new();
            for session in db::sessions::SessionsRepo::get_open(conn)? {
                let last_seen = session.last_seen.as_deref();
                // A start time that is unreadable is a reason to keep the row, not to throw it away.
                let Some(started_at) = time::parse_db(&session.start_time) else {
                    db::sessions::SessionsRepo::set_end(conn, session.id, last_seen.unwrap_or(&session.start_time))?;
                    continue;
                };
                match settle_dangling(started_at, last_seen.and_then(time::parse_db), min_seconds) {
                    DanglingRow::Close { ended_at } => {
                        db::sessions::SessionsRepo::set_end(conn, session.id, &time::to_db(ended_at))?;
                    }
                    DanglingRow::Drop => {
                        db::sessions::SessionsRepo::delete(conn, session.id)?;
                        dropped.push(session.activity_id);
                    }
                }
            }
            prune_orphans(conn, &dropped)
        })
        .map_err(Into::into)
    }
}

pub struct SettingsExclusions {
    settings: Arc<dyn SettingsStore>,
}

impl SettingsExclusions {
    pub fn new(settings: Arc<dyn SettingsStore>) -> Self {
        Self { settings }
    }
}

impl ExclusionPolicy for SettingsExclusions {
    fn is_excluded(&self, app: &FocusedApp, window_title: &str, url: Option<&str>) -> bool {
        ExclusionSet::new(&self.settings.get().tracking.excluded).excludes(app, window_title, url)
    }
}
