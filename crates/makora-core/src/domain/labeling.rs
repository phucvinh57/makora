//! AI category suggestions, read side: which activities still need one, what the user has
//! judged so far, and the review queue they judge through. Storing and changing the labels is
//! [`crate::categorization`]'s job.

use anyhow::Result;
use chrono::Utc;
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

use crate::categorization::domain::retrieval::SourceRef;
use crate::db::Db;
use crate::reporting::{self, CategoryScope, Window};
use crate::shared::ids::{raw, ActivityId, CategoryId};
use crate::taxonomy::store::load_tree;

use super::types::{
    ActivitySample, AiReviewGroup, AiReviewGroupPage, AiReviewPage, AiReviewRow, FeedbackStats, Paging,
    ReviewFilter,
};

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
    fn get_activities_to_label(&self, recheck: bool, only: Option<&[ActivityId]>) -> Result<Vec<ActivitySample>> {
        let only = only.map(raw);
        let mut conn = self.db.read()?;
        Ok(reporting::review::ReviewRepo::get_activities_to_label(&mut *conn, recheck, only.as_deref())?
            .into_iter()
            .map(sample_of)
            .collect())
    }

    fn get_ai_filed_ids(&self, category: CategoryId) -> Result<Vec<ActivityId>> {
        let mut conn = self.db.read()?;
        Ok(reporting::review::ReviewRepo::get_ai_filed_ids(&mut *conn, category.into())?
            .into_iter()
            .map(ActivityId::from)
            .collect())
    }

    fn get_recent_activities(&self, limit: i64) -> Result<Vec<ActivitySample>> {
        let mut conn = self.db.read()?;
        Ok(reporting::review::ReviewRepo::get_recent_activities(&mut *conn, limit)?
            .into_iter()
            .map(sample_of)
            .collect())
    }

    fn get_review_page(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewPage> {
        let paging = Paging::new(page, page_size);
        let activities = filter.activities.as_deref().map(raw);
        let window = Window::until(Utc::now());
        let mut conn = self.db.read()?;
        (*conn).transaction(|conn| {
            let category = resolve_category_scope(conn, filter.category)?;
            let rows = reporting::review::ReviewRepo::get_review_page(
                conn,
                paging.limit,
                paging.offset,
                &category,
                activities.as_deref(),
                &window,
            )?;
            let total = reporting::review::ReviewRepo::get_review_count(conn, &category, activities.as_deref())?;
            Ok(AiReviewPage {
                rows: rows.into_iter().map(to_review_row).collect(),
                total,
            })
        })
    }

    fn get_review_groups(&self, page: i64, page_size: i64, filter: &ReviewFilter) -> Result<AiReviewGroupPage> {
        let paging = Paging::new(page, page_size);
        let window = Window::until(Utc::now());
        let mut conn = self.db.read()?;
        (*conn).transaction(|conn| {
            let category = resolve_category_scope(conn, filter.category)?;
            let groups =
                reporting::review::ReviewRepo::get_review_groups(conn, paging.limit, paging.offset, &category, &window)?;
            let total = reporting::review::ReviewRepo::get_review_group_count(conn, &category)?;
            Ok(AiReviewGroupPage {
                groups: groups.into_iter().map(to_review_group).collect(),
                total,
            })
        })
    }

    fn get_feedback_stats(&self) -> Result<FeedbackStats> {
        let mut conn = self.db.read()?;
        (*conn).transaction(|conn| {
            let counts = reporting::review::ReviewRepo::get_feedback_counts(conn)?;
            let seconds = reporting::review::ReviewRepo::get_feedback_seconds(conn, &Window::until(Utc::now()))?;
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
        key_kind: row.key_kind,
        key: row.key,
        app_name: row.app_name,
        category_id: row.category_id.into(),
        category_name: row.category_name,
        category_color: row.category_color,
        activity_count: row.activity_count,
        total_seconds: row.total_seconds,
        activity_ids: row
            .activity_ids
            .split(',')
            .filter_map(|id| id.parse::<i64>().ok())
            .map(ActivityId::from)
            .collect(),
    }
}

fn sample_of(row: reporting::review::ActivityToLabelRow) -> ActivitySample {
    ActivitySample {
        id: row.id.into(),
        app_name: row.app_name,
        url: row.url,
        title: row.title,
        source: SourceRef {
            kind: row.source_kind,
            key: row.source_key,
            path: row.path,
        },
    }
}
