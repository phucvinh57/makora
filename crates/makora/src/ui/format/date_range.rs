use chrono::{
    DateTime, FixedOffset, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    SecondsFormat, TimeDelta, TimeZone, Utc,
};

/// RFC 3339 bounds, `end` exclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateBounds {
    pub start: String,
    pub end: String,
}

/// From local midnight `days_back` days before today up to the coming local midnight.
pub fn days_up_to_today(days_back: i64, now: DateTime<Local>) -> DateBounds {
    let today = now.date_naive();
    DateBounds {
        start: to_iso_string(local_midnight(today - TimeDelta::days(days_back))),
        end: to_iso_string(local_midnight(today + TimeDelta::days(1))),
    }
}

pub fn yesterday(now: DateTime<Local>) -> DateBounds {
    let today = now.date_naive();
    DateBounds {
        start: to_iso_string(local_midnight(today - TimeDelta::days(1))),
        end: to_iso_string(local_midnight(today)),
    }
}

/// `Date.prototype.toISOString`: UTC with milliseconds, e.g. `2026-11-05T17:00:00.000Z`.
pub fn to_iso_string<Tz: TimeZone>(at: DateTime<Tz>) -> String {
    at.with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The start of `day` in local time; where midnight is skipped by a clock change, the first
/// moment after it, as JavaScript's `Date` does.
pub fn local_midnight(day: NaiveDate) -> DateTime<Local> {
    local_instant(day.and_time(NaiveTime::MIN))
}

/// A local wall-clock time as an instant: the earlier one when the clock repeats it, and one
/// read with the offset before the change when the clock skips it.
pub fn local_instant(naive: NaiveDateTime) -> DateTime<Local> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => at,
        LocalResult::None => {
            let before = Local
                .offset_from_local_datetime(&(naive - TimeDelta::days(1)))
                .earliest()
                .map_or_else(|| Utc.fix(), |offset| offset.fix());
            skipped_instant(naive, before).with_timezone(&Local)
        }
    }
}

fn skipped_instant(naive: NaiveDateTime, before: FixedOffset) -> DateTime<Utc> {
    (naive - TimeDelta::seconds(i64::from(before.local_minus_utc()))).and_utc()
}

/// What `new Date(value)` accepts of ISO 8601: RFC 3339, a bare date (UTC midnight), or a
/// date-time with no offset (local time).
pub fn parse_instant(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(at) = DateTime::parse_from_rfc3339(value) {
        return Some(at.with_timezone(&Utc));
    }
    if let Ok(day) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(day.and_time(NaiveTime::MIN).and_utc());
    }
    ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
        .map(|naive| local_instant(naive).with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).single().unwrap()
    }

    fn iso(at: DateTime<Local>) -> String {
        to_iso_string(at)
    }

    #[test]
    fn today_spans_local_midnight_to_midnight() {
        let bounds = days_up_to_today(0, local(2026, 11, 18, 15, 30));
        assert_eq!(bounds.start, iso(local(2026, 11, 18, 0, 0)));
        assert_eq!(bounds.end, iso(local(2026, 11, 19, 0, 0)));
    }

    #[test]
    fn days_back_moves_the_start_only() {
        let bounds = days_up_to_today(6, local(2026, 11, 18, 0, 0));
        assert_eq!(bounds.start, iso(local(2026, 11, 12, 0, 0)));
        assert_eq!(bounds.end, iso(local(2026, 11, 19, 0, 0)));
    }

    #[test]
    fn days_back_crosses_month_and_year() {
        let bounds = days_up_to_today(2, local(2026, 1, 1, 23, 59));
        assert_eq!(bounds.start, iso(local(2025, 12, 30, 0, 0)));
        assert_eq!(bounds.end, iso(local(2026, 1, 2, 0, 0)));
    }

    #[test]
    fn yesterday_ends_at_todays_midnight() {
        let bounds = yesterday(local(2026, 11, 18, 9, 0));
        assert_eq!(bounds.start, iso(local(2026, 11, 17, 0, 0)));
        assert_eq!(bounds.end, iso(local(2026, 11, 18, 0, 0)));
    }

    #[test]
    fn iso_string_has_millis_and_z() {
        let at = Utc.with_ymd_and_hms(2026, 11, 5, 17, 0, 0).unwrap();
        assert_eq!(to_iso_string(at), "2026-11-05T17:00:00.000Z");
    }

    #[test]
    fn local_midnight_is_midnight_on_that_day() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(local_midnight(day), local(2026, 11, 18, 0, 0));
    }

    #[test]
    fn skipped_time_reads_with_the_earlier_offset() {
        let naive = NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        let before = FixedOffset::west_opt(5 * 3600).unwrap();
        assert_eq!(
            skipped_instant(naive, before),
            Utc.with_ymd_and_hms(2026, 3, 8, 7, 30, 0).unwrap()
        );
    }

    #[test]
    fn parse_instant_reads_the_forms_date_accepts() {
        let utc = Utc.with_ymd_and_hms(2026, 11, 5, 10, 0, 0).unwrap();
        assert_eq!(parse_instant("2026-11-05T10:00:00Z"), Some(utc));
        assert_eq!(parse_instant("2026-11-05T10:00:00.000Z"), Some(utc));
        assert_eq!(parse_instant("2026-11-05T12:00:00+02:00"), Some(utc));
        assert_eq!(
            parse_instant("2026-11-05"),
            Some(Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap())
        );
        assert_eq!(
            parse_instant("2026-11-05T14:30"),
            Some(local(2026, 11, 5, 14, 30).with_timezone(&Utc))
        );
        assert_eq!(
            parse_instant("2026-11-05T14:30:15"),
            Some((local(2026, 11, 5, 14, 30) + TimeDelta::seconds(15)).with_timezone(&Utc))
        );
        assert_eq!(parse_instant("not a date"), None);
        assert_eq!(parse_instant(""), None);
    }
}
