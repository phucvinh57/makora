//! What Makora sent over the network, kept as JSON lines so the Monitoring page can show it.
//! [`layer`] writes it, [`reader`] reads it back.

pub mod layer;
pub mod reader;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use layer::NetworkLogLayer;
pub use reader::{HostTraffic, JsonlNetworkLog, MaskedSummary, NetworkLog, RemoteSend};

/// The target every event about traffic leaving the machine uses, whichever module sends it.
pub const NETWORK_TARGET: &str = "makora_lib::network";

/// The message of the event [`crate::ai::remote::classify`] writes once a run sent activities.
pub const REMOTE_SEND: &str = "sent activities to remote provider";

pub const EVENTS_FILE_PREFIX: &str = "events";
pub const EVENTS_FILE_SUFFIX: &str = "jsonl";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// RFC 3339, UTC.
    pub ts: String,
    pub target: String,
    pub message: String,
    pub fields: Map<String, Value>,
}
