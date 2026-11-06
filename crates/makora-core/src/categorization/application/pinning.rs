//! The ways a person or the model can change how activities are categorized. Each use case loads
//! the [`ActivityClassification`]s it concerns, asks them to change, and saves the difference,
//! all in one transaction; the rules themselves are in
//! [`crate::categorization::domain::classification`].

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::Utc;
use diesel::sqlite::SqliteConnection;
use diesel::{Connection, QueryResult};

use crate::categorization::domain::classification::ActivityClassification;
use crate::categorization::store;
use crate::db::{self, Db};
use crate::domain::types::AiVerdict;
use crate::shared::ids::{raw, ActivityId, CategoryId, SessionId};
use crate::shared::time;

pub trait ClassificationSvc: Send + Sync {
    /// Returns the classifications as they were before, for [`Self::undo_assignment`].
    fn assign_sessions(
        &self,
        session_ids: &[SessionId],
        category_id: Option<CategoryId>,
    ) -> Result<Vec<ActivityClassification>>;
    fn assign_activities(
        &self,
        activity_ids: &[ActivityId],
        category_id: Option<CategoryId>,
    ) -> Result<Vec<ActivityClassification>>;
    fn undo_assignment(&self, prior: &[ActivityClassification]) -> Result<()>;
    /// Stores the verdicts `model` gave; a verdict without a category forgets an answer the model
    /// no longer stands by and records that it found no category. A manual pin made in the
    /// meantime is untouched: reads prefer it over the AI label. Returns how many labels actually
    /// changed.
    fn assign_ai(&self, verdicts: &[AiVerdict], model: &str) -> Result<usize>;
    fn accept_ai_labels(&self, activity_ids: &[ActivityId]) -> Result<Vec<ActivityClassification>>;
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

fn assign(
    conn: &mut SqliteConnection,
    activity_ids: &[ActivityId],
    category_id: Option<CategoryId>,
) -> QueryResult<Vec<ActivityClassification>> {
    store::modify_many(conn, activity_ids, |classifications| {
        for classification in classifications {
            match category_id {
                Some(category) => classification.pin_to(category),
                None => classification.unpin(),
            }
        }
    })
    .map(|(before, _)| before)
}

impl ClassificationSvc for SqliteClassification {
    fn assign_sessions(
        &self,
        session_ids: &[SessionId],
        category_id: Option<CategoryId>,
    ) -> Result<Vec<ActivityClassification>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
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

    fn assign_activities(
        &self,
        activity_ids: &[ActivityId],
        category_id: Option<CategoryId>,
    ) -> Result<Vec<ActivityClassification>> {
        if activity_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.db.write();
        conn.transaction(|conn| assign(conn, activity_ids, category_id))
            .map_err(Into::into)
    }

    fn undo_assignment(&self, prior: &[ActivityClassification]) -> Result<()> {
        if prior.is_empty() {
            return Ok(());
        }
        let prior: HashMap<ActivityId, &ActivityClassification> =
            prior.iter().map(|classification| (classification.activity_id(), classification)).collect();
        let activity_ids: Vec<ActivityId> = prior.keys().copied().collect();
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            store::modify_many(conn, &activity_ids, |classifications| {
                for classification in classifications {
                    if let Some(prior) = prior.get(&classification.activity_id()) {
                        classification.undo_assignment(prior);
                    }
                }
            })
            .map(|_| ())
        })
        .map_err(Into::into)
    }

    fn assign_ai(&self, verdicts: &[AiVerdict], model: &str) -> Result<usize> {
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
                        Some(category) => classification.suggest(category),
                        None => classification.withdraw_suggestion(),
                    }
                }
            })?;

            let unlabeled: Vec<ActivityId> = verdicts
                .iter()
                .filter(|verdict| verdict.category_id.is_none())
                .map(|verdict| verdict.activity_id)
                .collect();
            if !unlabeled.is_empty() {
                store::mark_unlabeled(conn, &unlabeled, model, &time::to_db(Utc::now()))?;
            }

            // A re-check that lands on the same category is not counted as a change.
            Ok(before
                .iter()
                .zip(&after)
                .filter(|(old, new)| old.active_suggestion() != new.active_suggestion())
                .count())
        })
        .map_err(|err: diesel::result::Error| err.into())
    }

    fn accept_ai_labels(&self, activity_ids: &[ActivityId]) -> Result<Vec<ActivityClassification>> {
        if activity_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            store::modify_many(conn, activity_ids, |classifications| {
                for classification in classifications {
                    classification.accept_suggestion();
                }
            })
            .map(|(before, _)| before)
        })
        .map_err(Into::into)
    }

    fn reset(&self) -> Result<usize> {
        let mut conn = self.db.write();
        conn.transaction(store::clear_all).map_err(Into::into)
    }
}
