use chrono::{DateTime, Local};

use super::date_range::{days_up_to_today, parse_instant, to_iso_string, yesterday};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeRangeKind {
    All,
    Today,
    Yesterday,
    LastThreeDays,
    LastSevenDays,
    Custom,
}

/// The history filter's time range; `from` and `to` are RFC 3339 and only used by `Custom`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRange {
    pub kind: TimeRangeKind,
    pub from: Option<String>,
    pub to: Option<String>,
}

pub const ALL_TIME: TimeRange = TimeRange {
    kind: TimeRangeKind::All,
    from: None,
    to: None,
};

impl TimeRange {
    pub const fn preset(kind: TimeRangeKind) -> Self {
        Self {
            kind,
            from: None,
            to: None,
        }
    }

    /// A custom range, or all time when neither bound is set.
    pub fn custom(from: Option<String>, to: Option<String>) -> Self {
        if from.is_none() && to.is_none() {
            ALL_TIME
        } else {
            Self {
                kind: TimeRangeKind::Custom,
                from,
                to,
            }
        }
    }
}

impl Default for TimeRange {
    fn default() -> Self {
        ALL_TIME
    }
}

pub const RELATIVE_PRESETS: [(TimeRangeKind, &str); 5] = [
    (TimeRangeKind::All, "All time"),
    (TimeRangeKind::Today, "Today"),
    (TimeRangeKind::Yesterday, "Yesterday"),
    (TimeRangeKind::LastThreeDays, "Last 3 days"),
    (TimeRangeKind::LastSevenDays, "Last 7 days"),
];

/// RFC 3339 bounds, `end` exclusive; a missing bound is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedRange {
    pub start: Option<String>,
    pub end: Option<String>,
}

pub fn resolve_range(range: &TimeRange, now: DateTime<Local>) -> ResolvedRange {
    let days_back = match range.kind {
        TimeRangeKind::All => return ResolvedRange::default(),
        TimeRangeKind::Yesterday => {
            let bounds = yesterday(now);
            return ResolvedRange {
                start: Some(bounds.start),
                end: Some(bounds.end),
            };
        }
        TimeRangeKind::Custom => {
            return ResolvedRange {
                start: range.from.clone(),
                end: range.to.clone(),
            };
        }
        TimeRangeKind::Today => 0,
        TimeRangeKind::LastThreeDays => 2,
        TimeRangeKind::LastSevenDays => 6,
    };
    let bounds = days_up_to_today(days_back, now);
    ResolvedRange {
        start: Some(bounds.start),
        end: Some(bounds.end),
    }
}

/// The preset's label, or `Nov 3, 09:00 – Nov 5, 18:00` for a custom range.
pub fn describe_range(range: &TimeRange) -> String {
    if range.kind == TimeRangeKind::Custom {
        let from = range.from.as_deref().map_or("…".to_owned(), format_bound);
        let to = range.to.as_deref().map_or("…".to_owned(), format_bound);
        return format!("{from} – {to}");
    }
    RELATIVE_PRESETS
        .iter()
        .find(|(kind, _)| *kind == range.kind)
        .map_or("All time", |(_, label)| label)
        .to_owned()
}

fn format_bound(value: &str) -> String {
    parse_instant(value).map_or_else(
        || "…".to_owned(),
        |at| at.with_timezone(&Local).format("%b %-d, %H:%M").to_string(),
    )
}

/// An instant as a `yyyy-MM-ddTHH:mm` local wall-clock value for a date-time input.
pub fn to_local_input_value(value: Option<&str>) -> String {
    value
        .and_then(parse_instant)
        .map(|at| {
            at.with_timezone(&Local)
                .format("%Y-%m-%dT%H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// A date-time input's local wall-clock value as an RFC 3339 instant.
pub fn from_local_input_value(value: &str) -> Option<String> {
    parse_instant(value).map(to_iso_string)
}

pub fn is_range_active(range: &TimeRange) -> bool {
    range.kind != TimeRangeKind::All
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn local(d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, d, h, min, 0)
            .single()
            .unwrap()
    }

    fn iso(at: DateTime<Local>) -> String {
        to_iso_string(at)
    }

    #[test]
    fn all_time_is_open() {
        assert_eq!(
            resolve_range(&ALL_TIME, local(18, 12, 0)),
            ResolvedRange::default()
        );
        assert_eq!(TimeRange::default(), ALL_TIME);
    }

    #[test]
    fn presets_resolve_to_local_days() {
        let now = local(18, 12, 0);
        let today = resolve_range(&TimeRange::preset(TimeRangeKind::Today), now);
        assert_eq!(today.start, Some(iso(local(18, 0, 0))));
        assert_eq!(today.end, Some(iso(local(19, 0, 0))));
        let three = resolve_range(&TimeRange::preset(TimeRangeKind::LastThreeDays), now);
        assert_eq!(three.start, Some(iso(local(16, 0, 0))));
        assert_eq!(three.end, Some(iso(local(19, 0, 0))));
        let seven = resolve_range(&TimeRange::preset(TimeRangeKind::LastSevenDays), now);
        assert_eq!(seven.start, Some(iso(local(12, 0, 0))));
        let yesterday = resolve_range(&TimeRange::preset(TimeRangeKind::Yesterday), now);
        assert_eq!(yesterday.start, Some(iso(local(17, 0, 0))));
        assert_eq!(yesterday.end, Some(iso(local(18, 0, 0))));
    }

    #[test]
    fn custom_passes_its_bounds_through() {
        let range = TimeRange::custom(Some("2026-11-01T00:00:00.000Z".into()), None);
        let resolved = resolve_range(&range, local(18, 12, 0));
        assert_eq!(resolved.start.as_deref(), Some("2026-11-01T00:00:00.000Z"));
        assert_eq!(resolved.end, None);
    }

    #[test]
    fn custom_without_bounds_is_all_time() {
        assert_eq!(TimeRange::custom(None, None), ALL_TIME);
        assert_eq!(
            TimeRange::custom(None, Some("x".into())).kind,
            TimeRangeKind::Custom
        );
    }

    #[test]
    fn describes_presets_by_label() {
        assert_eq!(describe_range(&ALL_TIME), "All time");
        assert_eq!(
            describe_range(&TimeRange::preset(TimeRangeKind::Today)),
            "Today"
        );
        assert_eq!(
            describe_range(&TimeRange::preset(TimeRangeKind::Yesterday)),
            "Yesterday"
        );
        assert_eq!(
            describe_range(&TimeRange::preset(TimeRangeKind::LastThreeDays)),
            "Last 3 days"
        );
        assert_eq!(
            describe_range(&TimeRange::preset(TimeRangeKind::LastSevenDays)),
            "Last 7 days"
        );
    }

    #[test]
    fn describes_custom_bounds() {
        let range = TimeRange::custom(Some(iso(local(3, 9, 0))), Some(iso(local(5, 18, 30))));
        assert_eq!(describe_range(&range), "Nov 3, 09:00 – Nov 5, 18:30");
        let open = TimeRange::custom(Some(iso(local(3, 9, 0))), None);
        assert_eq!(describe_range(&open), "Nov 3, 09:00 – …");
        let bad = TimeRange::custom(Some("garbage".into()), None);
        assert_eq!(describe_range(&bad), "… – …");
    }

    #[test]
    fn local_input_values_round_trip() {
        let instant = iso(local(5, 14, 30));
        assert_eq!(to_local_input_value(Some(&instant)), "2026-11-05T14:30");
        assert_eq!(from_local_input_value("2026-11-05T14:30"), Some(instant));
        assert_eq!(to_local_input_value(None), "");
        assert_eq!(to_local_input_value(Some("")), "");
        assert_eq!(to_local_input_value(Some("garbage")), "");
        assert_eq!(from_local_input_value(""), None);
        assert_eq!(from_local_input_value("garbage"), None);
    }

    #[test]
    fn from_input_accepts_seconds_and_offsets() {
        assert_eq!(
            from_local_input_value("2026-11-05T14:30:15"),
            Some(iso(local(5, 14, 30) + chrono::TimeDelta::seconds(15)))
        );
        assert_eq!(
            from_local_input_value("2026-11-05T10:00:00Z").as_deref(),
            Some("2026-11-05T10:00:00.000Z")
        );
    }

    #[test]
    fn only_all_time_is_inactive() {
        assert!(!is_range_active(&ALL_TIME));
        for (kind, _) in RELATIVE_PRESETS.iter().skip(1) {
            assert!(is_range_active(&TimeRange::preset(*kind)));
        }
        assert!(is_range_active(&TimeRange::custom(Some("x".into()), None)));
    }
}
