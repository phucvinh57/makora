use chrono::{DateTime, Local};

use super::date_range::parse_instant;

/// The time from `start` to `end`, or to `now` while the block is still open.
pub fn format_duration(start: &str, end: Option<&str>, now: DateTime<Local>) -> String {
    let Some(start) = parse_instant(start) else {
        return format_seconds(0.0);
    };
    let end = match end.filter(|end| !end.is_empty()) {
        Some(end) => parse_instant(end),
        None => Some(now.to_utc()),
    };
    let seconds = end.map_or(0, |end| (end - start).num_seconds());
    format_seconds(seconds.max(0) as f64)
}

/// `2h 5m`, `5m 3s` or `3s`.
pub fn format_seconds(total_seconds: f64) -> String {
    let seconds = total_seconds.floor().max(0.0) as u64;
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// `Date.prototype.toLocaleString()` in `en-US`, in local time: `11/5/2026, 3:04:05 PM`.
pub fn format_timestamp(value: &str) -> String {
    parse_instant(value).map_or_else(
        || "Invalid Date".to_owned(),
        |at| {
            at.with_timezone(&Local)
                .format("%-m/%-d/%Y, %-I:%M:%S %p")
                .to_string()
        },
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::super::date_range::to_iso_string;
    use super::*;

    fn local(h: u32, m: u32, s: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, 5, h, m, s)
            .single()
            .unwrap()
    }

    #[test]
    fn seconds_alone() {
        assert_eq!(format_seconds(0.0), "0s");
        assert_eq!(format_seconds(59.0), "59s");
        assert_eq!(format_seconds(59.9), "59s");
    }

    #[test]
    fn minutes_show_seconds() {
        assert_eq!(format_seconds(60.0), "1m 0s");
        assert_eq!(format_seconds(125.0), "2m 5s");
        assert_eq!(format_seconds(3599.0), "59m 59s");
    }

    #[test]
    fn hours_show_minutes() {
        assert_eq!(format_seconds(3600.0), "1h 0m");
        assert_eq!(format_seconds(7_530.0), "2h 5m");
        assert_eq!(format_seconds(100.0 * 3600.0 + 59.0), "100h 0m");
    }

    #[test]
    fn negative_and_nan_are_zero() {
        assert_eq!(format_seconds(-5.0), "0s");
        assert_eq!(format_seconds(f64::NAN), "0s");
    }

    #[test]
    fn duration_between_bounds() {
        let start = to_iso_string(local(10, 0, 0));
        let end = to_iso_string(local(11, 30, 0));
        assert_eq!(
            format_duration(&start, Some(&end), local(23, 0, 0)),
            "1h 30m"
        );
    }

    #[test]
    fn open_duration_runs_to_now() {
        let start = to_iso_string(local(10, 0, 0));
        assert_eq!(format_duration(&start, None, local(10, 2, 3)), "2m 3s");
        assert_eq!(format_duration(&start, Some(""), local(10, 0, 45)), "45s");
    }

    #[test]
    fn duration_truncates_and_never_goes_negative() {
        let start = "2026-11-05T10:00:00.900Z";
        assert_eq!(
            format_duration(start, Some("2026-11-05T10:00:02.100Z"), local(0, 0, 0)),
            "1s"
        );
        assert_eq!(
            format_duration(start, Some("2026-11-05T09:00:00Z"), local(0, 0, 0)),
            "0s"
        );
        assert_eq!(format_duration("garbage", None, local(0, 0, 0)), "0s");
    }

    #[test]
    fn timestamp_is_en_us_local_time() {
        assert_eq!(
            format_timestamp(&to_iso_string(local(15, 4, 5))),
            "11/5/2026, 3:04:05 PM"
        );
        assert_eq!(
            format_timestamp(&to_iso_string(local(0, 7, 0))),
            "11/5/2026, 12:07:00 AM"
        );
        assert_eq!(format_timestamp("nope"), "Invalid Date");
    }
}
