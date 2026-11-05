use chrono::{DateTime, Local, NaiveDate};

use super::date_range::{DateBounds, days_up_to_today, parse_instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RangePreset {
    #[default]
    Today,
    SevenDays,
    ThirtyDays,
}

pub const RANGE_PRESETS: [(RangePreset, &str); 3] = [
    (RangePreset::Today, "Today"),
    (RangePreset::SevenDays, "7 days"),
    (RangePreset::ThirtyDays, "30 days"),
];

impl RangePreset {
    fn days_back(self) -> i64 {
        match self {
            Self::Today => 0,
            Self::SevenDays => 6,
            Self::ThirtyDays => 29,
        }
    }
}

pub fn compute_range(preset: RangePreset, now: DateTime<Local>) -> DateBounds {
    days_up_to_today(preset.days_back(), now)
}

/// A `YYYY-MM-DD` day as `Nov 5`; text that is not a date is returned as is.
pub fn format_day(day: &str) -> String {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .or_else(|| parse_instant(day).map(|at| at.with_timezone(&Local).date_naive()))
        .map_or_else(|| day.to_owned(), |date| date.format("%b %-d").to_string())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::super::date_range::to_iso_string;
    use super::*;

    fn local(d: u32, h: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, d, h, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn presets_cover_whole_local_days() {
        let now = local(30, 15);
        let end = to_iso_string(
            Local
                .with_ymd_and_hms(2026, 12, 1, 0, 0, 0)
                .single()
                .unwrap(),
        );
        let today = compute_range(RangePreset::Today, now);
        assert_eq!(today.start, to_iso_string(local(30, 0)));
        assert_eq!(today.end, end);
        assert_eq!(
            compute_range(RangePreset::SevenDays, now).start,
            to_iso_string(local(24, 0))
        );
        assert_eq!(
            compute_range(RangePreset::ThirtyDays, now).start,
            to_iso_string(local(1, 0))
        );
    }

    #[test]
    fn preset_labels() {
        assert_eq!(
            RANGE_PRESETS.map(|(_, label)| label),
            ["Today", "7 days", "30 days"]
        );
        assert_eq!(RangePreset::default(), RangePreset::Today);
    }

    #[test]
    fn days_format_short() {
        assert_eq!(format_day("2026-11-05"), "Nov 5");
        assert_eq!(format_day("2026-01-31"), "Jan 31");
        assert_eq!(format_day("not a day"), "not a day");
    }

    #[test]
    fn day_with_a_time_uses_its_local_date() {
        let at = to_iso_string(local(5, 12));
        assert_eq!(format_day(&at), "Nov 5");
    }
}
