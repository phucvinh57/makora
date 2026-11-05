use std::collections::HashMap;

use chrono::{Local, NaiveDate};
use makora_lib::logs::{LogArea, LogEntry, LogLevel};

use crate::ui::format::date_range::parse_instant;
use crate::ui::format::logs::format_day_label;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Period {
    Day,
    #[default]
    Week,
    Month,
}

impl Period {
    pub const ALL: [Period; 3] = [Period::Day, Period::Week, Period::Month];

    pub fn label(self) -> &'static str {
        match self {
            Self::Day => "24 hours",
            Self::Week => "7 days",
            Self::Month => "30 days",
        }
    }

    pub fn days(self) -> u32 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::Month => 30,
        }
    }

    pub fn phrase(self) -> &'static str {
        match self {
            Self::Day => "in the last 24 hours",
            Self::Week => "in the last 7 days",
            Self::Month => "in the last 30 days",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MinLevel {
    #[default]
    Info,
    Warn,
    Error,
}

impl MinLevel {
    pub const ALL: [MinLevel; 3] = [MinLevel::Info, MinLevel::Warn, MinLevel::Error];

    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "All levels",
            Self::Warn => "Warnings & errors",
            Self::Error => "Errors only",
        }
    }

    pub fn level(self) -> LogLevel {
        match self {
            Self::Info => LogLevel::Info,
            Self::Warn => LogLevel::Warn,
            Self::Error => LogLevel::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GroupBy {
    #[default]
    Day,
    Area,
}

pub const AREAS: [LogArea; 7] = [
    LogArea::Network,
    LogArea::Ai,
    LogArea::Browser,
    LogArea::Tracking,
    LogArea::Settings,
    LogArea::Storage,
    LogArea::App,
];

pub fn area_label(area: LogArea) -> &'static str {
    match area {
        LogArea::Network => "Network",
        LogArea::Ai => "AI",
        LogArea::Browser => "Browser",
        LogArea::Tracking => "Tracking",
        LogArea::Settings => "Settings",
        LogArea::Storage => "Storage",
        LogArea::App => "App",
    }
}

pub fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "Trace",
        LogLevel::Debug => "Debug",
        LogLevel::Info => "Info",
        LogLevel::Warn => "Warn",
        LogLevel::Error => "Error",
    }
}

/// Adds `area` to the filter, or takes it out when it is in.
pub fn toggle_area(areas: &mut Vec<LogArea>, area: LogArea) {
    if let Some(ix) = areas.iter().position(|other| *other == area) {
        areas.remove(ix);
    } else {
        areas.push(area);
    }
}

pub struct KeyedEntry {
    pub key: String,
    pub entry: LogEntry,
}

/// Keys that stay the same across refetches, so expanded rows stay expanded as new entries arrive
/// on top.
pub fn with_keys(entries: Vec<LogEntry>) -> Vec<KeyedEntry> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    entries
        .into_iter()
        .map(|entry| {
            let fields = serde_json::to_string(&entry.fields).unwrap_or_default();
            let base = format!("{}|{}|{}|{fields}", entry.ts, entry.target, entry.message);
            let n = seen.entry(base.clone()).or_default();
            let key = format!("{base}|{n}");
            *n += 1;
            KeyedEntry { key, entry }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub label: String,
    /// Positions in the keyed entries.
    pub rows: Vec<usize>,
}

pub fn local_day(ts: &str) -> Option<NaiveDate> {
    parse_instant(ts).map(|at| at.with_timezone(&Local).date_naive())
}

/// Runs of entries on the same local day, newest first; or one group per area in [`AREAS`] order.
pub fn group_rows(rows: &[KeyedEntry], group_by: GroupBy, today: NaiveDate) -> Vec<Group> {
    match group_by {
        GroupBy::Area => AREAS
            .iter()
            .filter_map(|area| {
                let rows: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.entry.area == *area)
                    .map(|(ix, _)| ix)
                    .collect();
                (!rows.is_empty()).then(|| Group {
                    label: area_label(*area).to_owned(),
                    rows,
                })
            })
            .collect(),
        GroupBy::Day => {
            let mut groups: Vec<(Option<NaiveDate>, Group)> = Vec::new();
            for (ix, row) in rows.iter().enumerate() {
                let day = local_day(&row.entry.ts);
                match groups.last_mut() {
                    Some((last, group)) if *last == day => group.rows.push(ix),
                    _ => groups.push((
                        day,
                        Group {
                            label: day.map_or_else(
                                || "Invalid Date".to_owned(),
                                |day| format_day_label(day, today),
                            ),
                            rows: vec![ix],
                        },
                    )),
                }
            }
            groups.into_iter().map(|(_, group)| group).collect()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListItem {
    /// A group's header, by its position in the groups.
    Header(usize),
    /// An entry, by its group and its position in the keyed entries.
    Row { group: usize, row: usize },
}

pub fn list_items(groups: &[Group]) -> Vec<ListItem> {
    let mut items = Vec::new();
    for (group_ix, group) in groups.iter().enumerate() {
        items.push(ListItem::Header(group_ix));
        items.extend(group.rows.iter().map(|row| ListItem::Row {
            group: group_ix,
            row: *row,
        }));
    }
    items
}

pub fn empty_message(filtered: bool, period: Period) -> String {
    if filtered {
        format!("No log entries match these filters {}.", period.phrase())
    } else {
        format!("Nothing was logged {}.", period.phrase())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::{Map, json};

    use super::*;
    use crate::ui::format::date_range::to_iso_string;

    fn entry(ts: &str, area: LogArea, message: &str) -> LogEntry {
        LogEntry {
            ts: ts.to_owned(),
            level: LogLevel::Info,
            target: "makora_lib::app".to_owned(),
            area,
            message: message.to_owned(),
            fields: Map::new(),
        }
    }

    fn at(d: u32, h: u32) -> String {
        to_iso_string(
            Local
                .with_ymd_and_hms(2026, 11, d, h, 0, 0)
                .single()
                .unwrap(),
        )
    }

    #[test]
    fn duplicate_entries_get_distinct_stable_keys() {
        let mut with_field = entry("t1", LogArea::App, "a");
        with_field.fields.insert("n".into(), json!(1));
        let keyed = with_keys(vec![
            entry("t1", LogArea::App, "a"),
            entry("t1", LogArea::App, "a"),
            with_field,
        ]);
        assert_ne!(keyed[0].key, keyed[1].key);
        assert_ne!(keyed[0].key, keyed[2].key);
        let again = with_keys(vec![
            entry("t0", LogArea::App, "new"),
            entry("t1", LogArea::App, "a"),
            entry("t1", LogArea::App, "a"),
        ]);
        assert_eq!(again[1].key, keyed[0].key);
        assert_eq!(again[2].key, keyed[1].key);
    }

    #[test]
    fn days_group_consecutive_entries() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let keyed = with_keys(vec![
            entry(&at(5, 10), LogArea::App, "a"),
            entry(&at(5, 9), LogArea::App, "b"),
            entry(&at(4, 9), LogArea::App, "c"),
            entry(&at(2, 9), LogArea::App, "d"),
        ]);
        let groups = group_rows(&keyed, GroupBy::Day, today);
        let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Today", "Yesterday", "Monday, Nov 2"]);
        assert_eq!(groups[0].rows, [0, 1]);
        assert_eq!(groups[2].rows, [3]);
    }

    #[test]
    fn areas_group_in_fixed_order_and_skip_empty_ones() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let keyed = with_keys(vec![
            entry("t3", LogArea::App, "a"),
            entry("t2", LogArea::Network, "b"),
            entry("t1", LogArea::App, "c"),
        ]);
        let groups = group_rows(&keyed, GroupBy::Area, today);
        assert_eq!(
            groups,
            vec![
                Group {
                    label: "Network".into(),
                    rows: vec![1]
                },
                Group {
                    label: "App".into(),
                    rows: vec![0, 2]
                },
            ]
        );
        assert_eq!(
            list_items(&groups),
            vec![
                ListItem::Header(0),
                ListItem::Row { group: 0, row: 1 },
                ListItem::Header(1),
                ListItem::Row { group: 1, row: 0 },
                ListItem::Row { group: 1, row: 2 },
            ]
        );
    }

    #[test]
    fn area_chips_toggle() {
        let mut areas = Vec::new();
        toggle_area(&mut areas, LogArea::Ai);
        toggle_area(&mut areas, LogArea::Network);
        assert_eq!(areas, [LogArea::Ai, LogArea::Network]);
        toggle_area(&mut areas, LogArea::Ai);
        assert_eq!(areas, [LogArea::Network]);
    }

    #[test]
    fn empty_messages_name_the_period() {
        assert_eq!(
            empty_message(false, Period::Day),
            "Nothing was logged in the last 24 hours."
        );
        assert_eq!(
            empty_message(true, Period::Month),
            "No log entries match these filters in the last 30 days."
        );
    }
}
