//! Read-only access to tracked sessions, for the history list and the stats page.

use anyhow::Result;
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

use crate::db::Db;
use crate::reporting::{self, CategoryScope};
use crate::shared::time::{self, TimeRange};
use crate::taxonomy::store::load_tree;
use crate::tracking::domain::source_key::{source_url, KeyKind};

use super::types::{
    ActivityStats, AppStat, CategoryFilter, CategorySource, CategorySourceFilter, CategoryStat, DailyStat,
    SessionFilter, SessionRow, SessionsPage,
};

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
        let page_size = page_size.max(1);
        let offset = page.max(0) * page_size;
        let mut conn = self.db.read()?;
        // One transaction, so the rows and the total come from the same snapshot.
        (*conn).transaction(|conn| {
            let query = to_session_query(conn, filter)?;
            let rows = reporting::history::HistoryRepo::get_sessions(conn, page_size, offset, &query)?;
            let total = reporting::history::HistoryRepo::get_session_count(conn, &query)?;
            Ok(SessionsPage {
                rows: rows.into_iter().map(to_session_row).collect(),
                total,
            })
        })
    }

    fn get_activity_stats(&self, range: &TimeRange) -> Result<ActivityStats> {
        let (start, end) = (time::to_db(range.start), time::to_db(range.end));
        let (start, end) = (start.as_str(), end.as_str());
        let mut conn = self.db.read()?;
        (*conn).transaction(|conn| {
            let totals = reporting::history::HistoryRepo::get_totals(conn, start, end)?;
            let categories = reporting::history::HistoryRepo::get_category_stats(conn, start, end)?;
            let apps = reporting::history::HistoryRepo::get_app_stats(conn, start, end)?;
            let daily = reporting::history::HistoryRepo::get_daily_stats(conn, start, end)?;
            Ok(ActivityStats {
                total_seconds: totals.total_seconds,
                session_count: totals.session_count,
                categories: categories.into_iter().map(to_category_stat).collect(),
                apps: apps.into_iter().map(to_app_stat).collect(),
                daily: daily.into_iter().map(to_daily_stat).collect(),
            })
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
    let source_kind = KeyKind::from_db(&row.source_kind);
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
        category_source: row.category_source.as_deref().and_then(CategorySource::from_db),
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
        kind: KeyKind::from_db(&row.kind),
        category_color: row.category_color,
        total_seconds: row.total_seconds,
        session_count: row.session_count,
    }
}

fn to_daily_stat(row: reporting::history::DailyStatRow) -> DailyStat {
    DailyStat {
        day: row.day,
        total_seconds: row.total_seconds,
    }
}
