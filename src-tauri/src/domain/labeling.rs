//! AI category suggestions: which activities still need one, storing what the model comes back
//! with, and the review queue the user judges them through.

use std::collections::{HashMap, HashSet};

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

pub use super::traits::LabelingSvc;

pub struct SqliteLabeling {
    db: Db,
}

impl SqliteLabeling {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl LabelingSvc for SqliteLabeling {
    fn get_activities_to_label(&self, recheck: bool) -> Result<Vec<UncategorizedActivity>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::review::ReviewRepo::get_activities_to_label(&mut *conn, recheck)?
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

    fn assign_ai(&self, labels: &[(i64, Option<i64>, f64)]) -> Result<usize> {
        if labels.is_empty() {
            return Ok(0);
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<usize> {
            let activity_ids: Vec<i64> = labels.iter().map(|&(activity_id, ..)| activity_id).collect();
            let unpinned: HashSet<i64> = db::activities::ActivitiesRepo::filter_unpinned(conn, &activity_ids)?
                .into_iter()
                .collect();

            let wanted_categories: Vec<i64> = labels.iter().filter_map(|&(_, category_id, _)| category_id).collect();
            let existing_categories: HashSet<i64> = db::categories::CategoriesRepo::filter_existing(conn, &wanted_categories)?
                .into_iter()
                .collect();

            // A repeated activity_id keeps only its last entry — same as the sequential loop
            // this replaced. `upsert_many` runs as one statement, and SQLite rejects an
            // `ON CONFLICT DO UPDATE` that targets the same row twice in one statement.
            let mut to_upsert_by_id: HashMap<i64, (i64, f64)> = HashMap::new();
            let mut to_clear_ids: HashSet<i64> = HashSet::new();
            for &(activity_id, category_id, confidence) in labels {
                if !unpinned.contains(&activity_id) {
                    continue;
                }
                match category_id {
                    Some(category_id) if existing_categories.contains(&category_id) => {
                        to_clear_ids.remove(&activity_id);
                        to_upsert_by_id.insert(activity_id, (category_id, confidence));
                    }
                    Some(_) => {}
                    None => {
                        to_upsert_by_id.remove(&activity_id);
                        to_clear_ids.insert(activity_id);
                    }
                }
            }
            let to_upsert: Vec<(i64, i64, f64)> = to_upsert_by_id
                .into_iter()
                .map(|(activity_id, (category_id, confidence))| (activity_id, category_id, confidence))
                .collect();
            let to_clear: Vec<i64> = to_clear_ids.into_iter().collect();

            // Not counted as a change: a re-check that lands on the same category only
            // refreshes how sure the model was.
            let upsert_ids: Vec<i64> = to_upsert.iter().map(|&(activity_id, ..)| activity_id).collect();
            let before: HashMap<i64, i64> = db::ai_labels::AiLabelsRepo::get_active_many(conn, &upsert_ids)?
                .into_iter()
                .map(|label| (label.activity_id, label.category_id))
                .collect();
            let changed = to_upsert
                .iter()
                .filter(|&&(activity_id, category_id, _)| before.get(&activity_id).copied() != Some(category_id))
                .count();

            db::ai_labels::AiLabelsRepo::upsert_many(conn, &to_upsert)?;
            let cleared = db::ai_labels::AiLabelsRepo::clear_active_many(conn, &to_clear)?;

            Ok(changed + cleared)
        })
        .map_err(Into::into)
    }

    fn accept_ai_labels(&self, activity_ids: &[i64]) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            let labels = db::ai_labels::AiLabelsRepo::get_active_many(conn, activity_ids)?;
            let mut by_category: HashMap<i64, Vec<i64>> = HashMap::new();
            for label in labels {
                by_category.entry(label.category_id).or_default().push(label.activity_id);
            }
            for (category_id, ids) in by_category {
                db::activities::ActivitiesRepo::set_category_many_if_unpinned(conn, &ids, category_id)?;
            }
            Ok(())
        })
        .map_err(Into::into)
    }

    fn get_labeled_activities(&self) -> Result<Vec<LabeledActivity>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::review::ReviewRepo::get_labeled_activities(&mut *conn)?
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

    fn get_review_page(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewPage> {
        let mut conn = self.db.lock().unwrap();
        let category = resolve_category_scope(&mut conn, filter.category)?;
        let page_result = db::review::ReviewRepo::get_review_page(
            &mut *conn,
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

    fn get_review_groups(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewGroupPage> {
        let mut conn = self.db.lock().unwrap();
        let category = resolve_category_scope(&mut conn, filter.category)?;
        let page_result = db::review::ReviewRepo::get_review_groups(
            &mut *conn,
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

    fn get_feedback_stats(&self) -> Result<FeedbackStats> {
        let mut conn = self.db.lock().unwrap();
        let counts = db::review::ReviewRepo::get_feedback_counts(&mut *conn)?;
        let seconds = db::review::ReviewRepo::get_feedback_seconds(&mut *conn)?;
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
