use chrono::{DateTime, Local, TimeDelta};

use super::date_range::parse_instant;
use super::number::{js_string, to_fixed};

/// `512 B`, `1.5 KB`, `2.3 MB`, `1.25 GB`, in powers of 1000.
pub fn format_size(bytes: f64) -> String {
    if bytes < 1_000.0 {
        format!("{} B", js_string(bytes))
    } else if bytes < 1_000_000.0 {
        format!("{} KB", to_fixed(bytes / 1_000.0, 1))
    } else if bytes < 1_000_000_000.0 {
        format!("{} MB", to_fixed(bytes / 1_000_000.0, 1))
    } else {
        format!("{} GB", to_fixed(bytes / 1_000_000_000.0, 2))
    }
}

/// `14:05`, `Yesterday 14:05`, or `Nov 3, 14:05`, in local time; text that is not a time is
/// returned as is.
pub fn format_relative(ts: &str, now: DateTime<Local>) -> String {
    let Some(at) = parse_instant(ts) else {
        return ts.to_owned();
    };
    let at = at.with_timezone(&Local);
    let today = now.date_naive();
    let day = at.date_naive();
    if day == today {
        at.format("%H:%M").to_string()
    } else if day == today - TimeDelta::days(1) {
        at.format("Yesterday %H:%M").to_string()
    } else {
        at.format("%b %-d, %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::super::date_range::to_iso_string;
    use super::*;

    fn local(d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 11, d, h, min, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn sizes_step_by_thousands() {
        assert_eq!(format_size(0.0), "0 B");
        assert_eq!(format_size(999.0), "999 B");
        assert_eq!(format_size(1_000.0), "1.0 KB");
        assert_eq!(format_size(1_536.0), "1.5 KB");
        assert_eq!(format_size(999_949.0), "999.9 KB");
        assert_eq!(format_size(2_300_000.0), "2.3 MB");
        assert_eq!(format_size(1_250_000_000.0), "1.25 GB");
    }

    #[test]
    fn relative_times() {
        let now = local(5, 18, 0);
        assert_eq!(
            format_relative(&to_iso_string(local(5, 9, 7)), now),
            "09:07"
        );
        assert_eq!(
            format_relative(&to_iso_string(local(4, 23, 59)), now),
            "Yesterday 23:59"
        );
        assert_eq!(
            format_relative(&to_iso_string(local(1, 14, 5)), now),
            "Nov 1, 14:05"
        );
        assert_eq!(format_relative("bad", now), "bad");
    }
}
