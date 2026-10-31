use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{LogArea, LogEntry, LogLevel, EVENTS_FILE_PREFIX, EVENTS_FILE_SUFFIX, NETWORK_TARGET};

pub const MAX_ENTRIES: u32 = 1000;
const MAX_DAYS: u32 = 31;

#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    pub days: u32,
    pub min_level: LogLevel,
    /// Every area when left out.
    #[ts(optional)]
    pub areas: Option<Vec<LogArea>>,
    #[ts(optional)]
    pub search: Option<String>,
    /// At most [`MAX_ENTRIES`], which is also the default.
    #[ts(optional)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LogPage {
    /// Newest first.
    pub entries: Vec<LogEntry>,
    /// Matches per area for the period, level and search, whatever the area filter.
    pub area_counts: BTreeMap<LogArea, u32>,
    /// More entries matched than were returned.
    pub truncated: bool,
}

/// Everything sent to or received from one host in a period.
#[derive(Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct HostTraffic {
    pub host: String,
    pub requests: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub activities_sent: u64,
    /// RFC 3339.
    pub last_contact: String,
    pub purposes: Vec<String>,
}

pub trait LogReader: Send + Sync {
    fn dir(&self) -> &Path;

    fn query(&self, query: &LogQuery, now: DateTime<Utc>) -> Result<LogPage>;

    /// Per host, most recently contacted first.
    fn network_summary(&self, days: u32, now: DateTime<Utc>) -> Result<Vec<HostTraffic>>;
}

/// Reads the daily `events.<date>.jsonl` files [`super::AppLogLayer`] writes.
pub struct JsonlLogReader {
    dir: PathBuf,
}

impl JsonlLogReader {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Calls `visit` for each entry newer than `since`, newest first. Lines that do not parse,
    /// such as one cut short by a crash, are skipped.
    fn for_each_entry(&self, since: DateTime<Utc>, mut visit: impl FnMut(LogEntry, DateTime<Utc>)) -> Result<()> {
        for path in self.files_since(since.date_naive())? {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
            };
            for line in String::from_utf8_lossy(&bytes).lines().rev() {
                let Ok(entry) = serde_json::from_str::<LogEntry>(line) else {
                    continue;
                };
                let Ok(at) = DateTime::parse_from_rfc3339(&entry.ts) else {
                    continue;
                };
                let at = at.with_timezone(&Utc);
                if at >= since {
                    visit(entry, at);
                }
            }
        }
        Ok(())
    }

    /// The event files dated `from` or later, newest first.
    fn files_since(&self, from: NaiveDate) -> Result<Vec<PathBuf>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err).with_context(|| format!("listing {}", self.dir.display())),
        };
        let mut files: Vec<(NaiveDate, PathBuf)> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let date = file_date(entry.file_name().to_str()?)?;
                (date >= from).then(|| (date, entry.path()))
            })
            .collect();
        files.sort_by_key(|file| std::cmp::Reverse(file.0));
        Ok(files.into_iter().map(|(_, path)| path).collect())
    }
}

impl LogReader for JsonlLogReader {
    fn dir(&self) -> &Path {
        &self.dir
    }

    fn query(&self, query: &LogQuery, now: DateTime<Utc>) -> Result<LogPage> {
        let limit = query.limit.unwrap_or(MAX_ENTRIES).clamp(1, MAX_ENTRIES) as usize;
        let needle = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|search| !search.is_empty())
            .map(str::to_lowercase);
        let mut page = LogPage {
            entries: Vec::new(),
            area_counts: BTreeMap::new(),
            truncated: false,
        };
        self.for_each_entry(since(now, query.days), |entry, _| {
            if entry.level < query.min_level || needle.as_deref().is_some_and(|needle| !matches(&entry, needle)) {
                return;
            }
            *page.area_counts.entry(entry.area).or_default() += 1;
            if query.areas.as_ref().is_some_and(|areas| !areas.contains(&entry.area)) {
                return;
            }
            if page.entries.len() < limit {
                page.entries.push(entry);
            } else {
                page.truncated = true;
            }
        })?;
        Ok(page)
    }

    fn network_summary(&self, days: u32, now: DateTime<Utc>) -> Result<Vec<HostTraffic>> {
        let mut hosts: BTreeMap<String, (DateTime<Utc>, HostTraffic, BTreeSet<String>)> = BTreeMap::new();
        self.for_each_entry(since(now, days), |entry, at| {
            if entry.target != NETWORK_TARGET {
                return;
            }
            let Some(host) = entry.fields.get("host").and_then(Value::as_str) else {
                return;
            };
            let (last, traffic, purposes) = hosts.entry(host.to_string()).or_insert_with(|| {
                let traffic = HostTraffic {
                    host: host.to_string(),
                    requests: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                    activities_sent: 0,
                    last_contact: entry.ts.clone(),
                    purposes: Vec::new(),
                };
                (at, traffic, BTreeSet::new())
            });
            if at > *last {
                *last = at;
                traffic.last_contact = entry.ts.clone();
            }
            let fields = &entry.fields;
            let default_requests = u64::from(fields.contains_key("method"));
            traffic.requests += fields
                .get("requests")
                .and_then(Value::as_u64)
                .unwrap_or(default_requests);
            traffic.bytes_sent += number(fields, "bytes_sent");
            traffic.bytes_received += number(fields, "bytes_received");
            traffic.activities_sent += number(fields, "activities_sent");
            if let Some(purpose) = fields.get("purpose").and_then(Value::as_str) {
                purposes.insert(purpose.to_string());
            }
        })?;
        let mut summary: Vec<(DateTime<Utc>, HostTraffic)> = hosts
            .into_values()
            .map(|(last, mut traffic, purposes)| {
                traffic.purposes = purposes.into_iter().collect();
                (last, traffic)
            })
            .collect();
        summary.sort_by_key(|host| std::cmp::Reverse(host.0));
        Ok(summary.into_iter().map(|(_, traffic)| traffic).collect())
    }
}

fn since(now: DateTime<Utc>, days: u32) -> DateTime<Utc> {
    now - Duration::days(i64::from(days.clamp(1, MAX_DAYS)))
}

fn file_date(name: &str) -> Option<NaiveDate> {
    let date = name
        .strip_prefix(EVENTS_FILE_PREFIX)?
        .strip_prefix('.')?
        .strip_suffix(EVENTS_FILE_SUFFIX)?
        .strip_suffix('.')?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

fn number(fields: &Map<String, Value>, key: &str) -> u64 {
    fields.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// `needle` is already lowercase.
fn matches(entry: &LogEntry, needle: &str) -> bool {
    entry.message.to_lowercase().contains(needle)
        || entry.fields.iter().any(|(key, value)| {
            key.to_lowercase().contains(needle)
                || match value {
                    Value::String(text) => text.to_lowercase().contains(needle),
                    other => other.to_string().contains(needle),
                }
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::logs::area_for;
    use crate::logs::layer::tests::capture;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("makora-logs-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, date: &str, lines: &[String]) {
            let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
            fs::write(self.0.join(format!("events.{date}.jsonl")), text).unwrap();
        }

        fn reader(&self) -> JsonlLogReader {
            JsonlLogReader::new(self.0.clone())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-10-31T12:00:00Z".parse().unwrap()
    }

    fn line(ts: &str, level: &str, target: &str, message: &str, fields: Value) -> String {
        json!({
            "ts": ts,
            "level": level,
            "target": target,
            "area": area_for(target),
            "message": message,
            "fields": fields,
        })
        .to_string()
    }

    fn query(min_level: LogLevel) -> LogQuery {
        LogQuery {
            days: 1,
            min_level,
            areas: None,
            search: None,
            limit: None,
        }
    }

    #[test]
    fn entries_written_by_the_layer_read_back_newest_first() {
        let scratch = Scratch::new("round-trip");
        let text = capture(|| {
            tracing::info!(target: "makora_lib::settings", setting = "min_session_seconds", value = 30, "setting changed");
            tracing::warn!(target: "makora_lib::tracking", "focus watcher restarted");
        });
        let today = Utc::now();
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        scratch.write(&today.format("%Y-%m-%d").to_string(), &lines);

        let page = scratch.reader().query(&query(LogLevel::Info), today).unwrap();

        let messages: Vec<&str> = page.entries.iter().map(|entry| entry.message.as_str()).collect();
        assert_eq!(messages, ["focus watcher restarted", "setting changed"]);
        assert_eq!(page.entries[1].area, LogArea::Settings);
        assert_eq!(page.entries[1].fields["value"], 30);
        assert!(!page.truncated);
    }

    #[test]
    fn filters_by_level_search_area_and_period() {
        let scratch = Scratch::new("filters");
        scratch.write(
            "2026-10-29",
            &[line(
                "2026-10-29T10:00:00Z",
                "error",
                "makora_lib::ai",
                "too old",
                json!({}),
            )],
        );
        scratch.write(
            "2026-10-30",
            &[
                line(
                    "2026-10-30T11:00:00Z",
                    "info",
                    "makora_lib::ai",
                    "before the period",
                    json!({}),
                ),
                line(
                    "2026-10-30T13:00:00Z",
                    "info",
                    "makora_lib::network",
                    "HTTP request",
                    json!({"host": "API.openai.com"}),
                ),
                line(
                    "2026-10-30T14:00:00Z",
                    "debug",
                    "makora_lib::ai",
                    "noise",
                    json!({"host": "api.openai.com"}),
                ),
            ],
        );
        scratch.write(
            "2026-10-31",
            &[
                line(
                    "2026-10-31T09:00:00Z",
                    "warn",
                    "makora_lib::ai::remote::http",
                    "retrying",
                    json!({"host": "api.openai.com", "attempt": 2}),
                ),
                "not json".to_string(),
                line(
                    "2026-10-31T10:00:00Z",
                    "info",
                    "makora_lib::settings",
                    "setting changed",
                    json!({"value": 30}),
                ),
            ],
        );
        let reader = scratch.reader();

        let all = reader.query(&query(LogLevel::Info), now()).unwrap();
        let messages: Vec<&str> = all.entries.iter().map(|entry| entry.message.as_str()).collect();
        assert_eq!(messages, ["setting changed", "retrying", "HTTP request"]);

        let searched = LogQuery {
            search: Some(" OpenAI ".into()),
            areas: Some(vec![LogArea::Network]),
            ..query(LogLevel::Info)
        };
        let page = reader.query(&searched, now()).unwrap();
        let messages: Vec<&str> = page.entries.iter().map(|entry| entry.message.as_str()).collect();
        assert_eq!(messages, ["HTTP request"]);
        assert_eq!(
            page.area_counts,
            BTreeMap::from([(LogArea::Ai, 1), (LogArea::Network, 1)])
        );

        let warnings = reader.query(&query(LogLevel::Warn), now()).unwrap();
        assert_eq!(warnings.entries.len(), 1);

        let by_number = LogQuery {
            search: Some("30".into()),
            ..query(LogLevel::Info)
        };
        assert_eq!(reader.query(&by_number, now()).unwrap().entries.len(), 1);
    }

    #[test]
    fn caps_the_entries_and_says_so() {
        let scratch = Scratch::new("cap");
        let lines: Vec<String> = (0..1005)
            .map(|i| {
                line(
                    "2026-10-31T10:00:00Z",
                    "info",
                    "makora_lib",
                    &format!("event {i}"),
                    json!({}),
                )
            })
            .collect();
        scratch.write("2026-10-31", &lines);
        let reader = scratch.reader();

        let page = reader.query(&query(LogLevel::Info), now()).unwrap();
        assert_eq!(page.entries.len(), MAX_ENTRIES as usize);
        assert_eq!(page.entries[0].message, "event 1004");
        assert!(page.truncated);
        assert_eq!(page.area_counts[&LogArea::App], 1005);

        let small = LogQuery {
            limit: Some(10),
            ..query(LogLevel::Info)
        };
        assert_eq!(reader.query(&small, now()).unwrap().entries.len(), 10);
    }

    #[test]
    fn a_missing_log_directory_is_empty() {
        let reader = JsonlLogReader::new(std::env::temp_dir().join("makora-logs-does-not-exist"));
        assert!(reader.query(&query(LogLevel::Info), now()).unwrap().entries.is_empty());
        assert!(reader.network_summary(7, now()).unwrap().is_empty());
    }

    #[test]
    fn network_summary_adds_up_traffic_per_host() {
        let scratch = Scratch::new("network");
        let net = NETWORK_TARGET;
        scratch.write(
            "2026-10-30",
            &[
                line("2026-10-30T13:00:00Z", "info", net, "HTTP request", json!({
                    "method": "GET", "host": "api.openai.com", "path": "/v1/models", "status": 200,
                    "bytes_sent": 0, "bytes_received": 2000, "purpose": "list models",
                })),
                line("2026-10-30T14:00:00Z", "info", net, "model download started", json!({
                    "host": "registry.ollama.ai", "total_bytes": 5000, "purpose": "model download",
                })),
                line("2026-10-30T15:00:00Z", "info", net, "model download finished", json!({
                    "host": "registry.ollama.ai", "requests": 3, "bytes_received": 5000, "purpose": "model download",
                })),
            ],
        );
        scratch.write(
            "2026-10-31",
            &[
                line(
                    "2026-10-31T09:00:00Z",
                    "info",
                    net,
                    "HTTP request",
                    json!({
                        "method": "POST", "host": "api.openai.com", "path": "/v1/chat/completions", "status": 200,
                        "bytes_sent": 1500, "bytes_received": 300, "purpose": "AI categorization",
                    }),
                ),
                line(
                    "2026-10-31T09:00:01Z",
                    "info",
                    net,
                    "sent activities to remote provider",
                    json!({
                        "host": "api.openai.com", "model": "gpt-4o-mini", "activities_sent": 12, "input_tokens": 900,
                    }),
                ),
                line(
                    "2026-10-31T09:30:00Z",
                    "info",
                    "makora_lib::ai",
                    "not network",
                    json!({
                        "host": "elsewhere.example", "method": "GET",
                    }),
                ),
            ],
        );

        let summary = scratch.reader().network_summary(7, now()).unwrap();

        assert_eq!(
            summary,
            [
                HostTraffic {
                    host: "api.openai.com".into(),
                    requests: 2,
                    bytes_sent: 1500,
                    bytes_received: 2300,
                    activities_sent: 12,
                    last_contact: "2026-10-31T09:00:01Z".into(),
                    purposes: vec!["AI categorization".into(), "list models".into()],
                },
                HostTraffic {
                    host: "registry.ollama.ai".into(),
                    requests: 3,
                    bytes_sent: 0,
                    bytes_received: 5000,
                    activities_sent: 0,
                    last_contact: "2026-10-30T15:00:00Z".into(),
                    purposes: vec!["model download".into()],
                },
            ]
        );
    }
}
