use anyhow::Result;
use chrono::{Local, TimeDelta, Utc};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use tracing::warn;

use crate::db::Db;
use crate::reporting::{self, CategoryScope, Window};
use crate::shared::time::{self, TimeRange};
use crate::taxonomy::store::load_tree;
use crate::tracking::domain::source_key::{source_url, KeyKind};

use super::habits::{self, Interval};
use super::types::{
    ActivityStats, AppStat, CategoryFilter, CategorySourceFilter, CategoryStat,
    Paging, SessionFilter, SessionRow, SessionsPage,
};

/// Longest range that also gets the per-day timeline; an hour of slack covers a DST week.
const TIMELINE_RANGE: TimeDelta = TimeDelta::hours(7 * 24 + 1);

pub use super::traits::HistorySvc;

pub struct SqliteHistory {
    db: Db,
}

impl SqliteHistory {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl HistorySvc for SqliteHistory {
    fn get_sessions(&self, page: i64, page_size: i64, filter: &SessionFilter) -> Result<SessionsPage> {
        let paging = Paging::new(page, page_size);
        let mut conn = self.db.read()?;
        // One transaction, so the rows and the total come from the same snapshot.
        (*conn).transaction(|conn| {
            let query = to_session_query(conn, filter)?;
            let rows = reporting::history::HistoryRepo::get_sessions(conn, paging.limit, paging.offset, &query)?;
            let total = reporting::history::HistoryRepo::get_session_count(conn, &query)?;
            Ok(SessionsPage {
                rows: rows.into_iter().map(to_session_row).collect(),
                total,
            })
        })
    }

    fn get_activity_stats(&self, range: &TimeRange) -> Result<ActivityStats> {
        let window = Window::new(range, Utc::now());
        let mut conn = self.db.read()?;
        let (totals, categories, apps, rows) = (*conn).transaction(|conn| -> Result<_> {
            Ok((
                reporting::history::HistoryRepo::get_totals(conn, &window)?,
                reporting::history::HistoryRepo::get_category_stats(conn, &window)?,
                reporting::history::HistoryRepo::get_app_stats(conn, &window)?,
                reporting::history::HistoryRepo::get_intervals(conn, &window)?,
            ))
        })?;
        drop(conn);
        let intervals: Vec<Interval> = rows.into_iter().filter_map(to_interval).collect();
        let days = habits::day_profiles(&intervals, &Local);
        let hour_table = habits::hour_table(&intervals, &Local);
        let summary = habits::summarize(&days, &hour_table);
        let timeline = if range.end - range.start <= TIMELINE_RANGE {
            habits::timeline(&intervals, &Local)
        } else {
            Vec::new()
        };
        Ok(ActivityStats {
            total_seconds: totals.total_seconds,
            session_count: totals.session_count,
            categories: categories.into_iter().map(to_category_stat).collect(),
            apps: apps.into_iter().map(to_app_stat).collect(),
            daily: habits::daily_totals(&intervals, &Local),
            hour_table,
            days,
            observations: habits::observations(&summary),
            habits: summary,
            timeline,
        })
    }
}

fn to_session_query(conn: &mut SqliteConnection, filter: &SessionFilter) -> Result<reporting::history::SessionQuery> {
    let search = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let category = match &filter.category {
        CategoryFilter::All => CategoryScope::Any,
        CategoryFilter::Uncategorized => CategoryScope::Uncategorized,
        // Picking a parent means "this category and everything filed under it".
        CategoryFilter::Category(id) => CategoryScope::Ids(load_tree(conn)?.descendants((*id).into())),
    };
    let source = match filter.source {
        CategorySourceFilter::All => reporting::history::SourceScope::Any,
        CategorySourceFilter::Manual => reporting::history::SourceScope::Manual,
        CategorySourceFilter::Rule => reporting::history::SourceScope::Rule,
        CategorySourceFilter::Ai => reporting::history::SourceScope::Ai,
    };
    Ok(reporting::history::SessionQuery {
        search,
        category,
        source,
        start: filter.start.map(time::to_db),
        end: filter.end.map(time::to_db),
    })
}

fn to_session_row(row: reporting::history::SessionListRow) -> SessionRow {
    let source_kind = row.source_kind;
    let url = (source_kind == KeyKind::Domain).then(|| source_url(&row.source_key, &row.path));
    SessionRow {
        id: row.id.into(),
        activity_id: row.activity_id.into(),
        app_name: row.app_name,
        source_kind,
        source_key: row.source_key,
        title: row.title,
        url,
        start_time: row.start_time,
        end_time: row.end_time,
        category_id: row.category_id.map(Into::into),
        category_name: row.category_name,
        category_color: row.category_color,
        category_is_pinned: row.category_is_pinned,
        category_source: row.category_source,
    }
}

fn to_category_stat(row: reporting::history::CategoryStatRow) -> CategoryStat {
    CategoryStat {
        category_id: row.category_id.map(Into::into),
        name: row.name,
        color: row.color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_app_stat(row: reporting::history::AppStatRow) -> AppStat {
    AppStat {
        label: row.label,
        kind: row.kind,
        category_color: row.category_color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_interval(row: reporting::history::IntervalRow) -> Option<Interval> {
    match (time::parse_db(&row.start_time), time::parse_db(&row.end_time)) {
        (Some(start), Some(end)) => Some(Interval {
            start,
            end,
            category_id: row.category_id.map(Into::into),
            category_name: row.category_name,
            category_color: row.category_color,
        }),
        _ => {
            warn!(start = %row.start_time, end = %row.end_time, "skipping a session with an unreadable time");
            None
        }
    }
}

