use chrono::{DateTime, Local, NaiveDate, TimeDelta};
use serde_json::{Map, Value};

use super::date_range::parse_instant;
use super::number::{js_string, to_fixed, to_locale_string};

/// Fields shown on a collapsed row, in this order, before any others.
pub const KEY_FIELDS: [&str; 12] = [
    "host",
    "method",
    "path",
    "status",
    "setting",
    "value",
    "provider",
    "model",
    "browser",
    "activities_sent",
    "redactions",
    "error",
];
pub const INLINE_FIELD_COUNT: usize = 4;

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

/// `Today`, `Yesterday`, or `Monday, Nov 3`.
pub fn format_day_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        "Today".to_owned()
    } else if day == today - TimeDelta::days(1) {
        "Yesterday".to_owned()
    } else {
        day.format("%A, %b %-d").to_string()
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

pub fn format_field_value(key: &str, value: &Value) -> String {
    match value {
        Value::Number(number) => {
            let number = number.as_f64().unwrap_or(f64::NAN);
            if key.starts_with("bytes_") || key.ends_with("_bytes") {
                format_size(number)
            } else if key == "duration_ms" {
                format!("{} ms", js_string(number))
            } else {
                to_locale_string(number)
            }
        }
        Value::String(text) => text.clone(),
        Value::Null => "null".to_owned(),
        other => other.to_string(),
    }
}

/// The fields worth seeing without expanding a row: the well-known ones first.
pub fn key_fields(fields: &Map<String, Value>) -> Vec<(&str, &Value)> {
    let rank = |key: &str| {
        KEY_FIELDS
            .iter()
            .position(|known| *known == key)
            .unwrap_or(KEY_FIELDS.len())
    };
    let mut entries: Vec<(&str, &Value)> = fields
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    entries.sort_by_key(|(key, _)| rank(key));
    entries.truncate(INLINE_FIELD_COUNT);
    entries
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::super::date_range::to_iso_string;
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

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
    fn day_labels() {
        let today = day(2026, 11, 5);
        assert_eq!(format_day_label(today, today), "Today");
        assert_eq!(format_day_label(day(2026, 11, 4), today), "Yesterday");
        assert_eq!(format_day_label(day(2026, 11, 2), today), "Monday, Nov 2");
        assert_eq!(
            format_day_label(day(2026, 10, 31), day(2026, 11, 1)),
            "Yesterday"
        );
        assert_eq!(format_day_label(day(2026, 11, 6), today), "Friday, Nov 6");
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

    #[test]
    fn field_values() {
        assert_eq!(format_field_value("bytes_sent", &json!(2_048)), "2.0 KB");
        assert_eq!(format_field_value("body_bytes", &json!(512)), "512 B");
        assert_eq!(format_field_value("duration_ms", &json!(1_234)), "1234 ms");
        assert_eq!(format_field_value("duration_ms", &json!(12.5)), "12.5 ms");
        assert_eq!(format_field_value("count", &json!(1_234_567)), "1,234,567");
        assert_eq!(format_field_value("ratio", &json!(0.5)), "0.5");
        assert_eq!(
            format_field_value("host", &json!("example.com")),
            "example.com"
        );
        assert_eq!(format_field_value("ok", &json!(true)), "true");
        assert_eq!(format_field_value("error", &Value::Null), "null");
        assert_eq!(format_field_value("bytes_sent", &json!("n/a")), "n/a");
    }

    #[test]
    fn key_fields_put_known_ones_first() {
        let fields = json!({
            "zeta": 1,
            "status": 200,
            "alpha": 2,
            "host": "example.com",
            "method": "GET",
            "error": "boom",
        });
        let fields = fields.as_object().unwrap();
        let keys: Vec<&str> = key_fields(fields).into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, vec!["host", "method", "status", "error"]);
    }

    #[test]
    fn key_fields_fill_with_unknown_ones() {
        let fields = json!({ "model": "gpt", "items": 3 });
        let fields = fields.as_object().unwrap();
        let found = key_fields(fields);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], ("model", &json!("gpt")));
        assert_eq!(found[1], ("items", &json!(3)));
        assert!(key_fields(&Map::new()).is_empty());
    }
}
