use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use super::{LogEntry, EVENTS_FILE_PREFIX, EVENTS_FILE_SUFFIX, NETWORK_TARGET, REMOTE_SEND};

const MAX_DAYS: u32 = 31;

/// Everything sent to or received from one host in a period.
#[derive(Debug, PartialEq, Serialize)]
pub struct HostTraffic {
    pub host: String,
    pub requests: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub activities_sent: u64,
    /// What was taken out of those activities before they were sent.
    pub redactions: u64,
    /// RFC 3339.
    pub last_contact: String,
    pub purposes: Vec<String>,
}

/// One AI run that sent activities to a remote provider.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteSend {
    /// RFC 3339.
    pub at: String,
    pub host: String,
    pub model: String,
    pub activities_sent: u64,
    /// What each activity told the provider.
    pub fields: String,
    pub redactions: u64,
    /// Per kind of redaction, as [`crate::ai::privacy::RedactionReport::summary`] names them.
    pub kinds: Vec<(String, u64)>,
}

/// What was masked before activities left the machine in a period.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct MaskedSummary {
    pub activities_sent: u64,
    pub redactions: u64,
    /// Most redacted first.
    pub kinds: Vec<(String, u64)>,
    /// Newest first.
    pub sends: Vec<RemoteSend>,
}

pub trait NetworkLog: Send + Sync {
    /// Per host, most recently contacted first.
    fn hosts(&self, days: u32, now: DateTime<Utc>) -> Result<Vec<HostTraffic>>;

    fn masked(&self, days: u32, now: DateTime<Utc>) -> Result<MaskedSummary>;
}

/// Reads the network events in the daily `events.<date>.jsonl` files [`super::AppLogLayer`] writes.
pub struct JsonlNetworkLog {
    dir: PathBuf,
}

impl JsonlNetworkLog {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Calls `visit` for each network event newer than `since`, newest first. Lines that do not
    /// parse, such as one cut short by a crash, are skipped.
    fn for_each_entry(&self, since: DateTime<Utc>, mut visit: impl FnMut(LogEntry, DateTime<Utc>)) -> Result<()> {
        for path in self.files_since(since.date_naive())? {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
            };
            for line in String::from_utf8_lossy(&bytes).lines().rev() {
                if !line.contains(NETWORK_TARGET) {
                    continue;
                }
                let Ok(entry) = serde_json::from_str::<LogEntry>(line) else {
                    continue;
                };
                if entry.target != NETWORK_TARGET {
                    continue;
                }
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

impl NetworkLog for JsonlNetworkLog {
    fn hosts(&self, days: u32, now: DateTime<Utc>) -> Result<Vec<HostTraffic>> {
        let mut hosts: BTreeMap<String, (DateTime<Utc>, HostTraffic, BTreeSet<String>)> = BTreeMap::new();
        self.for_each_entry(since(now, days), |entry, at| {
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
                    redactions: 0,
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
            traffic.redactions += number(fields, "redactions");
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

    fn masked(&self, days: u32, now: DateTime<Utc>) -> Result<MaskedSummary> {
        let mut summary = MaskedSummary::default();
        let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
        self.for_each_entry(since(now, days), |entry, _| {
            if entry.message != REMOTE_SEND {
                return;
            }
            let fields = &entry.fields;
            let send = RemoteSend {
                at: entry.ts.clone(),
                host: text(fields, "host"),
                model: text(fields, "model"),
                activities_sent: number(fields, "activities_sent"),
                fields: text(fields, "fields"),
                redactions: number(fields, "redactions"),
                kinds: parse_kinds(&text(fields, "redaction_kinds")),
            };
            summary.activities_sent += send.activities_sent;
            summary.redactions += send.redactions;
            for (kind, count) in &send.kinds {
                *kinds.entry(kind.clone()).or_default() += count;
            }
            summary.sends.push(send);
        })?;
        summary.kinds = kinds.into_iter().collect();
        summary.kinds.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        Ok(summary)
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

fn text(fields: &Map<String, Value>, key: &str) -> String {
    fields.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// `email:3,secret:1` back into its counts; parts that do not parse are skipped.
fn parse_kinds(summary: &str) -> Vec<(String, u64)> {
    summary
        .split(',')
        .filter_map(|part| {
            let (name, count) = part.trim().split_once(':')?;
            Some((name.to_string(), count.parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::logs::layer::tests::capture;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("makora-network-log-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, date: &str, lines: &[String]) {
            let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
            fs::write(self.0.join(format!("events.{date}.jsonl")), text).unwrap();
        }

        fn reader(&self) -> JsonlNetworkLog {
            JsonlNetworkLog::new(self.0.clone())
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

    fn line(ts: &str, target: &str, message: &str, fields: Value) -> String {
        json!({
            "ts": ts,
            "level": "info",
            "target": target,
            "message": message,
            "fields": fields,
        })
        .to_string()
    }

    #[test]
    fn events_written_by_the_layer_read_back() {
        let scratch = Scratch::new("round-trip");
        let text = capture(|| {
            tracing::info!(target: "makora_lib::network", host = "api.openai.com", method = "GET", bytes_received = 10u64, "HTTP request");
        });
        let today = Utc::now();
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        scratch.write(&today.format("%Y-%m-%d").to_string(), &lines);

        let hosts = scratch.reader().hosts(1, today).unwrap();

        assert_eq!(hosts.len(), 1);
        assert_eq!((hosts[0].requests, hosts[0].bytes_received), (1, 10));
    }

    #[test]
    fn a_missing_log_directory_is_empty() {
        let reader = JsonlNetworkLog::new(std::env::temp_dir().join("makora-network-log-does-not-exist"));
        assert!(reader.hosts(7, now()).unwrap().is_empty());
        assert_eq!(reader.masked(7, now()).unwrap(), MaskedSummary::default());
    }

    #[test]
    fn masked_adds_up_redactions_per_kind_newest_send_first() {
        let scratch = Scratch::new("masked");
        let net = NETWORK_TARGET;
        scratch.write(
            "2026-10-29",
            &[line("2026-10-29T10:00:00Z", net, REMOTE_SEND, json!({
                "host": "api.openai.com", "model": "old", "activities_sent": 99, "redactions": 9, "redaction_kinds": "email:9",
            }))],
        );
        scratch.write(
            "2026-10-31",
            &[
                line("2026-10-31T08:00:00Z", net, REMOTE_SEND, json!({
                    "host": "api.openai.com", "model": "gpt-4o-mini", "activities_sent": 10, "fields": "app name",
                    "redactions": 4, "redaction_kinds": "email:3,secret:1",
                })),
                line("2026-10-31T08:00:01Z", net, "HTTP request", json!({
                    "host": "api.openai.com", "method": "POST",
                })),
                line("2026-10-31T09:00:00Z", net, REMOTE_SEND, json!({
                    "host": "api.anthropic.com", "model": "claude", "activities_sent": 5,
                    "redactions": 2, "redaction_kinds": "secret:1,never_send:1,bad",
                })),
                line("2026-10-31T09:30:00Z", "makora_lib::ai", REMOTE_SEND, json!({
                    "activities_sent": 1000,
                })),
            ],
        );

        let masked = scratch.reader().masked(1, now()).unwrap();

        assert_eq!((masked.activities_sent, masked.redactions), (15, 6));
        assert_eq!(
            masked.kinds,
            [("email".to_string(), 3), ("secret".to_string(), 2), ("never_send".to_string(), 1)]
        );
        let models: Vec<&str> = masked.sends.iter().map(|send| send.model.as_str()).collect();
        assert_eq!(models, ["claude", "gpt-4o-mini"]);
        assert_eq!(masked.sends[1].fields, "app name");
        assert_eq!(masked.sends[1].at, "2026-10-31T08:00:00Z");
    }

    #[test]
    fn network_summary_adds_up_traffic_per_host() {
        let scratch = Scratch::new("network");
        let net = NETWORK_TARGET;
        scratch.write(
            "2026-10-30",
            &[
                line("2026-10-30T13:00:00Z", net, "HTTP request", json!({
                    "method": "GET", "host": "api.openai.com", "path": "/v1/models", "status": 200,
                    "bytes_sent": 0, "bytes_received": 2000, "purpose": "list models",
                })),
                line("2026-10-30T14:00:00Z", net, "model download started", json!({
                    "host": "registry.ollama.ai", "total_bytes": 5000, "purpose": "model download",
                })),
                line("2026-10-30T15:00:00Z", net, "model download finished", json!({
                    "host": "registry.ollama.ai", "requests": 3, "bytes_received": 5000, "purpose": "model download",
                })),
            ],
        );
        scratch.write(
            "2026-10-31",
            &[
                line(
                    "2026-10-31T09:00:00Z",
                    net,
                    "HTTP request",
                    json!({
                        "method": "POST", "host": "api.openai.com", "path": "/v1/chat/completions", "status": 200,
                        "bytes_sent": 1500, "bytes_received": 300, "purpose": "AI categorization",
                    }),
                ),
                line(
                    "2026-10-31T09:00:01Z",
                    net,
                    "sent activities to remote provider",
                    json!({
                        "host": "api.openai.com", "model": "gpt-4o-mini", "activities_sent": 12, "input_tokens": 900, "redactions": 4,
                    }),
                ),
                line(
                    "2026-10-31T09:30:00Z",
                    "makora_lib::ai",
                    "not network",
                    json!({
                        "host": "elsewhere.example", "method": "GET",
                    }),
                ),
            ],
        );

        let summary = scratch.reader().hosts(7, now()).unwrap();

        assert_eq!(
            summary,
            [
                HostTraffic {
                    host: "api.openai.com".into(),
                    requests: 2,
                    bytes_sent: 1500,
                    bytes_received: 2300,
                    activities_sent: 12,
                    redactions: 4,
                    last_contact: "2026-10-31T09:00:01Z".into(),
                    purposes: vec!["AI categorization".into(), "list models".into()],
                },
                HostTraffic {
                    host: "registry.ollama.ai".into(),
                    requests: 3,
                    bytes_sent: 0,
                    bytes_received: 5000,
                    activities_sent: 0,
                    redactions: 0,
                    last_contact: "2026-10-30T15:00:00Z".into(),
                    purposes: vec!["model download".into()],
                },
            ]
        );
    }
}
