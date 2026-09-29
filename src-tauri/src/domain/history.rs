//! Read-only access to tracked sessions, for the history list and the stats page.

use anyhow::Result;
use diesel::sqlite::SqliteConnection;

use crate::category::source_url;
use crate::db::{self, Db};

use super::categories::load_tree;
use super::types::{
    ActivityStats, AppStat, CategoryFilter, CategorySourceFilter, CategoryStat, DailyStat, SessionFilter,
    SessionRow, SessionsPage,
};

pub struct History {
    db: Db,
}

impl History {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn list_sessions(&self, page: i64, page_size: i64, filter: &SessionFilter) -> Result<SessionsPage> {
        let page_size = page_size.max(1);
        let offset = page.max(0) * page_size;
        let mut conn = self.db.lock().unwrap();
        let query = to_session_query(&mut conn, filter)?;
        let rows = db::history::list_sessions(&mut conn, page_size, offset, &query)?;
        let total = db::history::count_sessions(&mut conn, &query)?;
        Ok(SessionsPage {
            rows: rows.into_iter().map(to_session_row).collect(),
            total,
        })
    }

    pub fn activity_stats(&self, start: &str, end: &str) -> Result<ActivityStats> {
        let mut conn = self.db.lock().unwrap();
        let totals = db::history::totals(&mut conn, start, end)?;
        let categories = db::history::category_stats(&mut conn, start, end)?;
        let apps = db::history::app_stats(&mut conn, start, end)?;
        let daily = db::history::daily_stats(&mut conn, start, end)?;
        Ok(ActivityStats {
            total_seconds: totals.total_seconds,
            session_count: totals.session_count,
            categories: categories.into_iter().map(to_category_stat).collect(),
            apps: apps.into_iter().map(to_app_stat).collect(),
            daily: daily.into_iter().map(to_daily_stat).collect(),
        })
    }
}

fn to_session_query(conn: &mut SqliteConnection, filter: &SessionFilter) -> Result<db::history::SessionQuery> {
    let search = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let category = match &filter.category {
        CategoryFilter::All => db::history::CategoryScope::Any,
        CategoryFilter::Uncategorized => db::history::CategoryScope::Uncategorized,
        // Picking a parent means "this category and everything filed under it".
        CategoryFilter::Category(id) => db::history::CategoryScope::Ids(load_tree(conn)?.descendants(*id)),
    };
    let source = match filter.source {
        CategorySourceFilter::All => db::history::SourceScope::Any,
        CategorySourceFilter::Manual => db::history::SourceScope::Manual,
        CategorySourceFilter::Ai => db::history::SourceScope::Ai,
    };
    Ok(db::history::SessionQuery {
        search,
        category,
        source,
        start: filter.start.clone(),
        end: filter.end.clone(),
    })
}

fn to_session_row(row: db::history::SessionListRow) -> SessionRow {
    let url = (row.source_kind == "domain").then(|| source_url(&row.source_key, &row.path));
    SessionRow {
        id: row.id,
        activity_id: row.activity_id,
        app_name: row.app_name,
        source_kind: row.source_kind,
        source_key: row.source_key,
        title: row.title,
        url,
        start_time: row.start_time,
        end_time: row.end_time,
        category_id: row.category_id,
        category_name: row.category_name,
        category_color: row.category_color,
        category_is_pinned: row.category_is_pinned,
        category_source: row.category_source,
    }
}

fn to_category_stat(row: db::history::CategoryStatRow) -> CategoryStat {
    CategoryStat {
        category_id: row.category_id,
        name: row.name,
        color: row.color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_app_stat(row: db::history::AppStatRow) -> AppStat {
    AppStat {
        label: row.label,
        kind: row.kind,
        category_color: row.category_color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_daily_stat(row: db::history::DailyStatRow) -> DailyStat {
    DailyStat {
        day: row.day,
        total_seconds: row.total_seconds,
    }
}
