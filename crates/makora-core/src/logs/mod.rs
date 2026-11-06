//! Makora's own activity log: what it did, kept as JSON lines next to the plain-text log.
//! [`layer`] writes it; [`reader`] reads its network events back for the Monitoring page.

pub mod layer;
pub mod reader;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use layer::AppLogLayer;
pub use reader::{HostTraffic, JsonlNetworkLog, MaskedSummary, NetworkLog, RemoteSend};

/// The target every event about traffic leaving the machine uses, whichever module sends it.
pub const NETWORK_TARGET: &str = "makora_lib::network";

/// The message of the event [`crate::ai::remote::classify`] writes once a run sent activities.
pub const REMOTE_SEND: &str = "sent activities to remote provider";

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// RFC 3339, UTC.
    pub ts: String,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: Map<String, Value>,
}
