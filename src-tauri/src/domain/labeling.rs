//! AI category suggestions, read side: which activities still need one, what the user has
//! judged so far, and the review queue they judge through. Storing and changing the labels is
//! [`crate::categorization`]'s job.

use anyhow::Result;
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

use crate::db::Db;
use crate::reporting::{self, CategoryScope};
use crate::shared::confidence::Confidence;
use crate::shared::ids::{raw, ActivityId, CategoryId};
use crate::taxonomy::store::load_tree;
use crate::tracking::domain::source_key::KeyKind;

use super::types::{
    AiReviewGroup, AiReviewGroupPage, AiReviewPage, AiReviewRow, FeedbackStats, ReviewFilter, UncategorizedActivity,
};

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
        let mut conn = self.db.read()?;
        Ok(reporting::review::ReviewRepo::get_activities_to_label(&mut *conn, recheck)?
            .into_iter()
            .map(|row| UncategorizedActivity {
                id: row.id.into(),
                app_name: row.app_name,
                url: row.url,
                title: row.title,
            })
            .collect())
    }

    fn get_review_page(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewPage> {
        let mut conn = self.db.read()?;
        let category = resolve_category_scope(&mut conn, filter.category)?;
        let activities = filter.activities.as_deref().map(raw);
        let (rows, total) = reporting::review::ReviewRepo::get_review_page(
            &mut *conn,
            page,
            page_size,
            &category,
            activities.as_deref(),
            filter.uncertain_only,
            CONFIDENT,
            filter.min_seconds,
        )?;
        Ok(AiReviewPage {
            rows: rows.into_iter().map(to_review_row).collect(),
            total,
        })
    }

    fn get_review_groups(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewGroupPage> {
        let mut conn = self.db.read()?;
        let category = resolve_category_scope(&mut conn, filter.category)?;
        let (groups, total) = reporting::review::ReviewRepo::get_review_groups(
            &mut *conn,
            page,
            page_size,
            &category,
            filter.uncertain_only,
            CONFIDENT,
            filter.min_seconds,
        )?;
        Ok(AiReviewGroupPage {
            groups: groups.into_iter().map(to_review_group).collect(),
            total,
        })
    }

    fn get_feedback_stats(&self) -> Result<FeedbackStats> {
        let mut conn = self.db.read()?;
        (*conn).transaction(|conn| {
            let counts = reporting::review::ReviewRepo::get_feedback_counts(conn)?;
            let seconds = reporting::review::ReviewRepo::get_feedback_seconds(conn)?;
            Ok(FeedbackStats {
                corrected: counts.corrected,
                confirmed: counts.confirmed,
                pinned: counts.pinned,
                rejected: counts.rejected,
                pending_review: counts.pending_review,
                pending_seconds: seconds.pending_seconds,
                reviewed_seconds: seconds.reviewed_seconds,
            })
        })
    }
}

fn resolve_category_scope(conn: &mut SqliteConnection, category: Option<CategoryId>) -> Result<CategoryScope> {
    Ok(match category {
        None => CategoryScope::Any,
        Some(id) => CategoryScope::Ids(load_tree(conn)?.descendants(id.into())),
    })
}

fn to_review_row(row: reporting::review::ReviewRow) -> AiReviewRow {
    AiReviewRow {
        activity_id: row.activity_id.into(),
        app_name: row.app_name,
        url: row.url,
        title: row.title,
        category_id: row.category_id.into(),
        category_name: row.category_name,
        category_color: row.category_color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_review_group(row: reporting::review::ReviewGroupRow) -> AiReviewGroup {
    AiReviewGroup {
        key_kind: KeyKind::from_db(&row.key_kind),
        key: row.key,
        app_name: row.app_name,
        category_id: row.category_id.into(),
        category_name: row.category_name,
        category_color: row.category_color,
        activity_count: row.activity_count,
        total_seconds: row.total_seconds,
        min_confidence: row.min_confidence.map(Confidence::new),
        activity_ids: row
            .activity_ids
            .split(',')
            .filter_map(|id| id.parse::<i64>().ok())
            .map(ActivityId::from)
            .collect(),
    }
}
