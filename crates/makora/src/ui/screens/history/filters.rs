//! The history list's filters as the core asks for them, and the rules the web UI kept in its
//! URL: an empty custom range is all time, the first page is page 1, a filter change starts over
//! on the first page.

use chrono::{DateTime, Local};
use makora_lib::reporting::query::SessionListQuery;

use crate::ui::format::time_range::{TimeRange, TimeRangeKind, is_range_active, resolve_range};
use crate::ui::route::{HistoryFilters, HistorySource};

use super::rows::parse_category_value;

pub const PAGE_SIZE: i64 = 50;
pub const ALL_CATEGORIES: &str = "all";
pub const ALL_SOURCES: &str = "all";

pub fn time_range(filters: &HistoryFilters) -> TimeRange {
    match filters.kind {
        TimeRangeKind::Custom => TimeRange::custom(filters.from.clone(), filters.to.clone()),
        kind => TimeRange::preset(kind),
    }
}

/// The filters as a link may spell them, settled the way the web UI read its URL.
pub fn normalized(mut filters: HistoryFilters) -> HistoryFilters {
    let range = time_range(&filters);
    filters.kind = range.kind;
    filters.from = range.from;
    filters.to = range.to;
    if filters
        .category
        .as_deref()
        .is_some_and(|value| parse_category_value(value).is_err())
    {
        filters.category = None;
    }
    filters.page = filters.page.max(1);
    filters
}

pub fn with_range(filters: &HistoryFilters, range: TimeRange) -> HistoryFilters {
    HistoryFilters {
        kind: range.kind,
        from: range.from,
        to: range.to,
        page: 1,
        ..filters.clone()
    }
}

pub fn with_category(filters: &HistoryFilters, value: &str) -> HistoryFilters {
    HistoryFilters {
        category: (value != ALL_CATEGORIES).then(|| value.to_owned()),
        page: 1,
        ..filters.clone()
    }
}

pub fn with_source(filters: &HistoryFilters, source: Option<HistorySource>) -> HistoryFilters {
    HistoryFilters {
        source,
        page: 1,
        ..filters.clone()
    }
}

/// A committed search: the trimmed text, back on the first page.
pub fn with_query(filters: &HistoryFilters, text: &str) -> HistoryFilters {
    HistoryFilters {
        q: text.trim().to_owned(),
        page: 1,
        ..filters.clone()
    }
}

pub fn is_filtered(filters: &HistoryFilters) -> bool {
    !filters.q.is_empty()
        || filters.category.is_some()
        || filters.source.is_some()
        || is_range_active(&time_range(filters))
}

pub fn source_value(source: Option<HistorySource>) -> &'static str {
    match source {
        None => ALL_SOURCES,
        Some(HistorySource::Manual) => "manual",
        Some(HistorySource::Rule) => "rule",
    }
}

pub fn parse_source(value: &str) -> Option<HistorySource> {
    match value {
        "manual" => Some(HistorySource::Manual),
        "rule" => Some(HistorySource::Rule),
        _ => None,
    }
}

/// The page to load, with the range worked out from `now` so "Today" stays today after midnight.
pub fn session_query(filters: &HistoryFilters, now: DateTime<Local>) -> SessionListQuery {
    let range = resolve_range(&time_range(filters), now);
    SessionListQuery {
        page: i64::from(filters.page.max(1) - 1),
        page_size: PAGE_SIZE,
        search: (!filters.q.is_empty()).then(|| filters.q.clone()),
        category: filters.category.clone(),
        source: filters
            .source
            .map(|source| source_value(Some(source)).to_owned()),
        start: range.start,
        end: range.end,
    }
}

pub fn page_count(total: i64) -> u32 {
    let pages = (total.max(0) + PAGE_SIZE - 1) / PAGE_SIZE;
    u32::try_from(pages.max(1)).unwrap_or(u32::MAX)
}

/// The one-based page to settle on when `page` is past the last one there is.
pub fn clamped_page(page: u32, total: i64) -> Option<u32> {
    let last = page_count(total);
    (page > last).then_some(last)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 5, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn a_custom_range_without_bounds_is_all_time() {
        let filters = normalized(HistoryFilters {
            kind: TimeRangeKind::Custom,
            page: 0,
            ..HistoryFilters::default()
        });
        assert_eq!(filters, HistoryFilters::default());
    }

    #[test]
    fn a_preset_drops_custom_bounds() {
        let filters = normalized(HistoryFilters {
            kind: TimeRangeKind::Today,
            from: Some("2026-11-01T00:00:00.000Z".into()),
            ..HistoryFilters::default()
        });
        assert_eq!(filters.from, None);
        assert_eq!(filters.kind, TimeRangeKind::Today);
    }

    #[test]
    fn all_categories_is_no_category_filter() {
        let filters = normalized(HistoryFilters {
            category: Some(ALL_CATEGORIES.into()),
            ..HistoryFilters::default()
        });
        assert_eq!(filters.category, None);
        let unknown = normalized(HistoryFilters {
            category: Some("work".into()),
            ..HistoryFilters::default()
        });
        assert_eq!(unknown.category, None);
        let picked = with_category(&HistoryFilters::default(), "uncategorized");
        assert_eq!(picked.category.as_deref(), Some("uncategorized"));
        assert_eq!(with_category(&picked, ALL_CATEGORIES).category, None);
    }

    #[test]
    fn changing_a_filter_goes_back_to_the_first_page() {
        let on_page_3 = HistoryFilters {
            page: 3,
            ..HistoryFilters::default()
        };
        assert_eq!(with_category(&on_page_3, "7").page, 1);
        assert_eq!(with_source(&on_page_3, Some(HistorySource::Rule)).page, 1);
        assert_eq!(with_query(&on_page_3, " zoom ").page, 1);
        assert_eq!(with_query(&on_page_3, " zoom ").q, "zoom");
        let range = with_range(&on_page_3, TimeRange::preset(TimeRangeKind::Yesterday));
        assert_eq!((range.page, range.kind), (1, TimeRangeKind::Yesterday));
    }

    #[test]
    fn any_filter_counts_as_filtered() {
        assert!(!is_filtered(&HistoryFilters::default()));
        assert!(is_filtered(&with_query(&HistoryFilters::default(), "x")));
        assert!(is_filtered(&with_source(
            &HistoryFilters::default(),
            Some(HistorySource::Manual)
        )));
        assert!(is_filtered(&with_range(
            &HistoryFilters::default(),
            TimeRange::preset(TimeRangeKind::LastSevenDays)
        )));
    }

    #[test]
    fn sources_round_trip() {
        for source in [None, Some(HistorySource::Manual), Some(HistorySource::Rule)] {
            assert_eq!(parse_source(source_value(source)), source);
        }
    }

    #[test]
    fn the_query_is_zero_based_and_leaves_out_empty_filters() {
        let query = session_query(&HistoryFilters::default(), now());
        assert_eq!((query.page, query.page_size), (0, PAGE_SIZE));
        assert!(query.search.is_none() && query.category.is_none() && query.source.is_none());
        assert!(query.start.is_none() && query.end.is_none());

        let filters = HistoryFilters {
            kind: TimeRangeKind::Today,
            q: "zoom".into(),
            category: Some("3".into()),
            source: Some(HistorySource::Manual),
            page: 2,
            ..HistoryFilters::default()
        };
        let query = session_query(&filters, now());
        assert_eq!(query.page, 1);
        assert_eq!(query.search.as_deref(), Some("zoom"));
        assert_eq!(query.category.as_deref(), Some("3"));
        assert_eq!(query.source.as_deref(), Some("manual"));
        assert!(query.start.is_some() && query.end.is_some());
    }

    #[test]
    fn pages_round_up_and_clamp_to_the_last() {
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(50), 1);
        assert_eq!(page_count(51), 2);
        assert_eq!(clamped_page(3, 120), None);
        assert_eq!(clamped_page(4, 120), Some(3));
        assert_eq!(clamped_page(2, 0), Some(1));
    }
}
