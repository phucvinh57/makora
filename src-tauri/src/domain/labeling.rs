//! AI category suggestions: which activities still need one, storing what the model comes back
//! with, and the review queue the user judges them through.

use anyhow::Result;
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

use crate::db::{self, Db};

use super::categories::load_tree;
use super::types::{
    AiReviewGroup, AiReviewGroupPage, AiReviewPage, AiReviewRow, FeedbackStats, LabeledActivity,
    ReviewFilter, UncategorizedActivity,
};

/// Labels the model was at least this sure of are left out of the default review queue.
const CONFIDENT: f64 = 0.85;

pub struct Labeling {
    db: Db,
}

impl Labeling {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Activities for the LLM to label, most recently seen first. Normally those with neither a
    /// pin nor an AI label nor a rejected one; with `recheck`, those carrying an AI label the
    /// user has not confirmed.
    pub fn activities_to_label(&self, recheck: bool) -> Result<Vec<UncategorizedActivity>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::review::activities_to_label(&mut conn, recheck)?
            .into_iter()
            .map(|row| UncategorizedActivity {
                id: row.id,
                key_kind: row.key_kind,
                key: row.key,
                app_name: row.app_name,
                url: row.url,
                title: row.title,
            })
            .collect())
    }

    /// Stores AI labels as `(activity_id, category_id, confidence)`; `None` clears a label the
    /// model no longer stands by. A manual pin made in the meantime is untouched: reads prefer
    /// it over the AI label. Returns how many labels actually changed.
    pub fn assign_ai(&self, labels: &[(i64, Option<i64>, f64)]) -> Result<usize> {
        if labels.is_empty() {
            return Ok(0);
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<usize> {
            let mut changed = 0usize;
            for &(activity_id, category_id, confidence) in labels {
                if !db::activities::is_unpinned(conn, activity_id)? {
                    continue;
                }
                changed += match category_id {
                    Some(category_id) => {
                        if !db::categories::exists(conn, category_id)? {
                            continue;
                        }
                        // Not counted as a change: a re-check that lands on the same category
                        // only refreshes how sure the model was.
                        let before = db::ai_labels::find_active(conn, activity_id)?.map(|label| label.category_id);
                        db::ai_labels::upsert(conn, activity_id, category_id, confidence)?;
                        usize::from(before != Some(category_id))
                    }
                    None => db::ai_labels::clear_active(conn, activity_id)?,
                };
            }
            Ok(changed)
        })
        .map_err(Into::into)
    }

    /// Turns each activity's AI label into a manual pin.
    pub fn accept_ai_labels(&self, activity_ids: &[i64]) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            for &activity_id in activity_ids {
                if let Some(label) = db::ai_labels::find_active(conn, activity_id)? {
                    db::activities::set_category_if_unpinned(conn, activity_id, label.category_id)?;
                }
            }
            Ok(())
        })
        .map_err(Into::into)
    }

    /// Every activity the user has pinned or rejected an AI label for, newest first.
    pub fn labeled_activities(&self) -> Result<Vec<LabeledActivity>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::review::labeled_activities(&mut conn)?
            .into_iter()
            .map(|row| LabeledActivity {
                key_kind: row.key_kind,
                key: row.key,
                app_name: row.app_name,
                url: row.url,
                title: row.title,
                category_id: row.category_id,
                ai_category_id: row.ai_category_id,
                ai_rejected_category_id: row.ai_rejected_category_id,
            })
            .collect())
    }

    /// AI labels awaiting review, longest-tracked first so the ones that matter come up first.
    /// Filtering by a category includes everything under it.
    pub fn review_page(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewPage> {
        let mut conn = self.db.lock().unwrap();
        let category = resolve_category_scope(&mut conn, filter.category)?;
        let page_result = db::review::review_page(
            &mut conn,
            page,
            page_size,
            &category,
            filter.activities.as_deref(),
            filter.uncertain_only,
            CONFIDENT,
            filter.min_seconds,
        )?;
        let Some((rows, total)) = page_result else {
            return Ok(AiReviewPage { rows: Vec::new(), total: 0 });
        };
        Ok(AiReviewPage {
            rows: rows.into_iter().map(to_review_row).collect(),
            total,
        })
    }

    /// The pending AI labels folded into one row per source and suggested category, the
    /// longest-tracked first. `filter.activities` is ignored: a group picks its own members.
    pub fn review_groups(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewGroupPage> {
        let mut conn = self.db.lock().unwrap();
        let category = resolve_category_scope(&mut conn, filter.category)?;
        let page_result = db::review::review_groups(
            &mut conn,
            page,
            page_size,
            &category,
            filter.uncertain_only,
            CONFIDENT,
            filter.min_seconds,
        )?;
        let Some((groups, total)) = page_result else {
            return Ok(AiReviewGroupPage { groups: Vec::new(), total: 0 });
        };
        Ok(AiReviewGroupPage {
            groups: groups.into_iter().map(to_review_group).collect(),
            total,
        })
    }

    pub fn feedback_stats(&self) -> Result<FeedbackStats> {
        let mut conn = self.db.lock().unwrap();
        let counts = db::review::feedback_counts(&mut conn)?;
        let seconds = db::review::feedback_seconds(&mut conn)?;
        Ok(FeedbackStats {
            corrected: counts.corrected,
            confirmed: counts.confirmed,
            pinned: counts.pinned,
            rejected: counts.rejected,
            pending_review: counts.pending_review,
            pending_seconds: seconds.pending_seconds,
            reviewed_seconds: seconds.reviewed_seconds,
        })
    }
}

fn resolve_category_scope(conn: &mut SqliteConnection, category: Option<i64>) -> Result<db::review::CategoryScope> {
    Ok(match category {
        None => db::review::CategoryScope::Any,
        Some(id) => db::review::CategoryScope::Ids(load_tree(conn)?.descendants(id)),
    })
}

fn to_review_row(row: db::review::ReviewRow) -> AiReviewRow {
    AiReviewRow {
        activity_id: row.activity_id,
        app_name: row.app_name,
        url: row.url,
        title: row.title,
        category_id: row.category_id,
        category_name: row.category_name,
        category_color: row.category_color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_review_group(row: db::review::ReviewGroupRow) -> AiReviewGroup {
    AiReviewGroup {
        key_kind: row.key_kind,
        key: row.key,
        app_name: row.app_name,
        category_id: row.category_id,
        category_name: row.category_name,
        category_color: row.category_color,
        activity_count: row.activity_count,
        total_seconds: row.total_seconds,
        min_confidence: row.min_confidence,
        activity_ids: row.activity_ids.split(',').filter_map(|id| id.parse().ok()).collect(),
    }
}
