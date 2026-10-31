use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeDelta, TimeZone, Utc};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use tracing::warn;

use crate::db::Db;
use crate::reporting::{self, CategoryScope, Window};
use crate::shared::time::{self, TimeRange};
use crate::taxonomy::store::load_tree;
use crate::tracking::domain::source_key::{source_url, KeyKind};

use super::types::{
    ActivityStats, AppStat, CategoryFilter, CategorySourceFilter, CategoryStat, DailyStat,
    Paging, SessionFilter, SessionRow, SessionsPage,
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
        (*conn).transaction(|conn| {
            let totals = reporting::history::HistoryRepo::get_totals(conn, &window)?;
            let categories = reporting::history::HistoryRepo::get_category_stats(conn, &window)?;
            let apps = reporting::history::HistoryRepo::get_app_stats(conn, &window)?;
            let intervals = reporting::history::HistoryRepo::get_intervals(conn, &window)?;
            Ok(ActivityStats {
                total_seconds: totals.total_seconds,
                session_count: totals.session_count,
                categories: categories.into_iter().map(to_category_stat).collect(),
                apps: apps.into_iter().map(to_app_stat).collect(),
                daily: split_by_local_day(intervals.iter().filter_map(to_interval), &Local),
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

fn to_interval(row: &reporting::history::IntervalRow) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    match (time::parse_db(&row.start_time), time::parse_db(&row.end_time)) {
        (Some(start), Some(end)) => Some((start, end)),
        _ => {
            warn!(start = %row.start_time, end = %row.end_time, "skipping a session with an unreadable time");
            None
        }
    }
}

/// Time per calendar day in `tz`, oldest first; a session that runs past midnight counts towards
/// both days. Days without any time are left out.
fn split_by_local_day<Tz: TimeZone>(
    intervals: impl IntoIterator<Item = (DateTime<Utc>, DateTime<Utc>)>,
    tz: &Tz,
) -> Vec<DailyStat> {
    let mut days: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for (mut start, end) in intervals {
        while start < end {
            let day = start.with_timezone(tz).date_naive();
            let cut = day
                .succ_opt()
                .and_then(|next| start_of_day(next, tz))
                .filter(|&next| next > start)
                .map_or(end, |next| next.min(end));
            *days.entry(day).or_default() += (cut - start).as_seconds_f64();
            start = cut;
        }
    }
    days.into_iter()
        .map(|(day, total_seconds)| DailyStat {
            day: day.format("%Y-%m-%d").to_string(),
            total_seconds,
        })
        .collect()
}

/// When `day` begins in `tz`: its midnight, or the first moment after it when a DST change
/// skips midnight.
fn start_of_day<Tz: TimeZone>(day: NaiveDate, tz: &Tz) -> Option<DateTime<Utc>> {
    let midnight = day.and_time(NaiveTime::MIN);
    (0..48)
        .map(|step| midnight + TimeDelta::minutes(30 * step))
        .find_map(|local| tz.from_local_datetime(&local).earliest())
        .map(|start| start.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, MappedLocalTime, NaiveDateTime};

    use super::*;

    fn utc(at: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(at).unwrap().with_timezone(&Utc)
    }

    fn days(stats: &[DailyStat]) -> Vec<(&str, f64)> {
        stats.iter().map(|d| (d.day.as_str(), d.total_seconds)).collect()
    }

    #[test]
    fn a_session_across_local_midnight_counts_towards_both_days() {
        let tz = FixedOffset::east_opt(7 * 3600).unwrap();
        // 23:00 to 01:30 local time.
        let stats = split_by_local_day([(utc("2026-03-01T16:00:00Z"), utc("2026-03-01T18:30:00Z"))], &tz);
        assert_eq!(days(&stats), [("2026-03-01", 3600.0), ("2026-03-02", 5400.0)]);
    }

    #[test]
    fn sessions_of_one_day_add_up_and_days_come_oldest_first() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let stats = split_by_local_day(
            [
                (utc("2026-03-02T09:00:00Z"), utc("2026-03-02T09:10:00Z")),
                (utc("2026-03-01T09:00:00Z"), utc("2026-03-01T09:01:00Z")),
                (utc("2026-03-02T10:00:00Z"), utc("2026-03-02T10:05:00Z")),
            ],
            &tz,
        );
        assert_eq!(days(&stats), [("2026-03-01", 60.0), ("2026-03-02", 900.0)]);
    }

    #[test]
    fn a_session_spanning_several_days_fills_each_one() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let stats = split_by_local_day([(utc("2026-03-01T12:00:00Z"), utc("2026-03-03T06:00:00Z"))], &tz);
        assert_eq!(
            days(&stats),
            [("2026-03-01", 43200.0), ("2026-03-02", 86400.0), ("2026-03-03", 21600.0)]
        );
    }

    /// UTC+1 that moves to UTC+2 at local midnight going into 2026-03-29, so that day starts at
    /// 01:00 local (2026-03-28T23:00Z) and is 23 hours long.
    #[derive(Clone)]
    struct MidnightDst;

    const SWITCH_UTC: &str = "2026-03-28T23:00:00Z";

    impl MidnightDst {
        fn offset_at(utc_at: DateTime<Utc>) -> FixedOffset {
            let hours = if utc_at < utc(SWITCH_UTC) { 1 } else { 2 };
            FixedOffset::east_opt(hours * 3600).unwrap()
        }
    }

    impl TimeZone for MidnightDst {
        type Offset = FixedOffset;

        fn from_offset(_: &FixedOffset) -> Self {
            MidnightDst
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
            self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN))
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
            let before = FixedOffset::east_opt(3600).unwrap();
            let after = FixedOffset::east_opt(2 * 3600).unwrap();
            let gap_start = utc(SWITCH_UTC).naive_utc() + TimeDelta::hours(1);
            let gap_end = gap_start + TimeDelta::hours(1);
            if *local < gap_start {
                MappedLocalTime::Single(before)
            } else if *local < gap_end {
                MappedLocalTime::None
            } else {
                MappedLocalTime::Single(after)
            }
        }

        fn offset_from_utc_date(&self, utc_date: &NaiveDate) -> FixedOffset {
            self.offset_from_utc_datetime(&utc_date.and_time(NaiveTime::MIN))
        }

        fn offset_from_utc_datetime(&self, utc_at: &NaiveDateTime) -> FixedOffset {
            Self::offset_at(utc_at.and_utc())
        }
    }

    #[test]
    fn a_dst_change_that_skips_midnight_still_splits_at_the_start_of_the_day() {
        // 2026-03-28 22:00 local (+1) to 2026-03-29 03:00 local (+2).
        let stats = split_by_local_day([(utc("2026-03-28T21:00:00Z"), utc("2026-03-29T01:00:00Z"))], &MidnightDst);
        assert_eq!(days(&stats), [("2026-03-28", 7200.0), ("2026-03-29", 7200.0)]);
    }

    #[test]
    fn an_empty_or_backwards_interval_counts_nothing() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let at = utc("2026-03-01T09:00:00Z");
        let stats = split_by_local_day([(at, at), (at, at - TimeDelta::hours(1))], &tz);
        assert!(stats.is_empty());
    }
}
