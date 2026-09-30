//! The ways a person or the model can change how activities are categorized. Each use case loads
//! the [`ActivityClassification`]s it concerns, asks them to change, and saves the difference,
//! all in one transaction; the rules themselves are in
//! [`crate::categorization::domain::classification`].

use std::collections::HashSet;

use anyhow::Result;
use diesel::sqlite::SqliteConnection;
use diesel::{Connection, QueryResult};

use crate::categorization::store;
use crate::db::{self, Db};
use crate::domain::types::AiVerdict;
use crate::shared::ids::{raw, ActivityId, CategoryId, SessionId};

pub trait ClassificationSvc: Send + Sync {
    /// Pins the activity behind each session row to `category_id` (or unpins with `None`).
    fn assign_sessions(&self, session_ids: &[SessionId], category_id: Option<CategoryId>) -> Result<()>;
    /// Like [`assign_sessions`](Self::assign_sessions), for activities picked directly.
    fn assign_activities(&self, activity_ids: &[ActivityId], category_id: Option<CategoryId>) -> Result<()>;
    /// Stores the model's verdicts; a verdict without a category clears a label the model no
    /// longer stands by. A manual pin made in the meantime is untouched: reads prefer it over
    /// the AI label. Returns how many labels actually changed.
    fn assign_ai(&self, verdicts: &[AiVerdict]) -> Result<usize>;
    /// Turns each activity's AI label into a manual pin.
    fn accept_ai_labels(&self, activity_ids: &[ActivityId]) -> Result<()>;
    /// Clears every pin and AI label in one transaction while keeping the taxonomy and tracking
    /// history intact. Returns how many were removed.
    fn reset(&self) -> Result<usize>;
}

pub struct SqliteClassification {
    db: Db,
}

impl SqliteClassification {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

}

/// Files each of `activity_ids` under `category_id`, or takes it out of its category with `None`.
fn assign(conn: &mut SqliteConnection, activity_ids: &[ActivityId], category_id: Option<CategoryId>) -> QueryResult<()> {
    store::modify_many(conn, activity_ids, |classifications| {
        for classification in classifications {
            match category_id {
                Some(category) => classification.pin_to(category),
                None => classification.unpin(),
            }
        }
    })
    .map(|_| ())
}

impl ClassificationSvc for SqliteClassification {
    fn assign_sessions(&self, session_ids: &[SessionId], category_id: Option<CategoryId>) -> Result<()> {
        if session_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            let activity_ids: Vec<ActivityId> =
                db::sessions::SessionsRepo::get_activity_ids_for(conn, &raw(session_ids))?
                    .into_iter()
                    .map(ActivityId::from)
                    .collect();
            assign(conn, &activity_ids, category_id)
        })
        .map_err(Into::into)
    }

    fn assign_activities(&self, activity_ids: &[ActivityId], category_id: Option<CategoryId>) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.write();
        conn.transaction(|conn| assign(conn, activity_ids, category_id))
            .map_err(Into::into)
    }

    fn assign_ai(&self, verdicts: &[AiVerdict]) -> Result<usize> {
        if verdicts.is_empty() {
            return Ok(0);
        }
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            // A verdict naming a category that has since been deleted is ignored altogether.
            let wanted: Vec<i64> = verdicts
                .iter()
                .filter_map(|verdict| verdict.category_id.map(i64::from))
                .collect();
            let existing: HashSet<i64> = db::categories::CategoriesRepo::filter_existing(conn, &wanted)?
                .into_iter()
                .collect();
            let verdicts: Vec<&AiVerdict> = verdicts
                .iter()
                .filter(|verdict| verdict.category_id.is_none_or(|category| existing.contains(&category.into())))
                .collect();

            let activity_ids: Vec<ActivityId> = verdicts.iter().map(|verdict| verdict.activity_id).collect();
            let (before, after) = store::modify_many(conn, &activity_ids, |classifications| {
                // Verdicts apply in order, so when one activity appears twice the last one wins.
                for verdict in &verdicts {
                    let Some(classification) = classifications
                        .iter_mut()
                        .find(|classification| classification.activity_id() == verdict.activity_id)
                    else {
                        continue;
                    };
                    match verdict.category_id {
                        Some(category) => classification.suggest(category, verdict.confidence),
                        None => classification.withdraw_suggestion(),
                    }
                }
            })?;

            // A re-check that lands on the same category only refreshes how sure the model was,
            // so it is not counted as a change.
            Ok(before
                .iter()
                .zip(&after)
                .filter(|(old, new)| old.active_suggestion() != new.active_suggestion())
                .count())
        })
        .map_err(|err: diesel::result::Error| err.into())
    }

    fn accept_ai_labels(&self, activity_ids: &[ActivityId]) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            store::modify_many(conn, activity_ids, |classifications| {
                for classification in classifications {
                    classification.accept_suggestion();
                }
            })
            .map(|_| ())
        })
        .map_err(Into::into)
    }

    fn reset(&self) -> Result<usize> {
        let mut conn = self.db.write();
        conn.transaction(store::clear_all).map_err(Into::into)
    }
}
