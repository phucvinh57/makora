//! What an AI run is remembered by: what started it, how it ended, and what it cost.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::categorization::domain::rules::UnknownValue;
use crate::categorization::ports::RunUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RunTrigger {
    Manual,
    Scheduled,
}

impl RunTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
        }
    }
}

impl TryFrom<&str> for RunTrigger {
    type Error = UnknownValue;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "manual" => Ok(Self::Manual),
            "scheduled" => Ok(Self::Scheduled),
            _ => Err(UnknownValue::new("run trigger", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Completed,
    Stopped,
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for RunStatus {
    type Error = UnknownValue;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "completed" => Ok(Self::Completed),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            _ => Err(UnknownValue::new("run status", value)),
        }
    }
}

/// One finished run. `provider` is `None` for the local model; `items` counts the activities
/// answered, `labeled` those given a category.
#[derive(Debug, Clone)]
pub struct AiRunRecord {
    pub trigger: RunTrigger,
    pub provider: Option<String>,
    pub model: String,
    pub status: RunStatus,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub items: u64,
    pub labeled: u64,
    pub usage: RunUsage,
}
