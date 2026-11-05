//! Makora's own activity log: what it did, kept as JSON lines next to the plain-text log so the
//! app can show it. [`layer`] writes it, [`reader`] reads it back.

pub mod layer;
pub mod reader;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use layer::AppLogLayer;
pub use reader::{JsonlLogReader, LogReader};

/// The target every event about traffic leaving the machine uses, whichever module sends it.
pub const NETWORK_TARGET: &str = "makora_lib::network";

pub const EVENTS_FILE_PREFIX: &str = "events";
pub const EVENTS_FILE_SUFFIX: &str = "jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<&tracing::Level> for LogLevel {
    fn from(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::ERROR => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogArea {
    App,
    Network,
    Browser,
    Tracking,
    Ai,
    Storage,
    Settings,
}

const AREA_MODULES: &[(&str, LogArea)] = &[
    ("tracking::adapters::ingest", LogArea::Browser),
    ("tracking::adapters::native_host", LogArea::Browser),
    ("tracking", LogArea::Tracking),
    ("ai::providers", LogArea::Settings),
    ("ai", LogArea::Ai),
    ("categorization", LogArea::Ai),
    ("db", LogArea::Storage),
    ("domain", LogArea::Storage),
    ("settings", LogArea::Settings),
];

/// The part of the app an event's `target` comes from. The first matching module wins, so the
/// more specific ones are listed first.
pub fn area_for(target: &str) -> LogArea {
    if target == NETWORK_TARGET {
        return LogArea::Network;
    }
    let Some(module) = target.strip_prefix("makora_lib::") else {
        return LogArea::App;
    };
    AREA_MODULES
        .iter()
        .find(|(prefix, _)| {
            module
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
        })
        .map_or(LogArea::App, |(_, area)| *area)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// RFC 3339, UTC.
    pub ts: String,
    pub level: LogLevel,
    pub target: String,
    pub area: LogArea,
    pub message: String,
    pub fields: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_map_to_their_area() {
        let cases = [
            ("makora_lib::network", LogArea::Network),
            ("makora_lib::tracking::adapters::ingest", LogArea::Browser),
            ("makora_lib::tracking::adapters::native_host", LogArea::Browser),
            ("makora_lib::tracking::adapters::platform::linux", LogArea::Tracking),
            ("makora_lib::tracking", LogArea::Tracking),
            ("makora_lib::ai::remote::http", LogArea::Ai),
            ("makora_lib::categorization::application::ai_run", LogArea::Ai),
            ("makora_lib::categorization::application::schedule", LogArea::Ai),
            ("makora_lib::db::migrate", LogArea::Storage),
            ("makora_lib::domain::history", LogArea::Storage),
            ("makora_lib::settings", LogArea::Settings),
            ("makora_lib::settings::application", LogArea::Settings),
            ("makora_lib::ai::providers", LogArea::Settings),
            ("makora_lib::app_core", LogArea::App),
            ("makora::runtime::tray", LogArea::App),
            ("makora_lib", LogArea::App),
            ("makora_lib::aim", LogArea::App),
            ("makora_lib::network::extra", LogArea::App),
            ("ureq::unversioned", LogArea::App),
        ];
        for (target, area) in cases {
            assert_eq!(area_for(target), area, "{target}");
        }
    }
}
