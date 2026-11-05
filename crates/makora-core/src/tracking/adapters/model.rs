use chrono::{DateTime, Utc};

use crate::tracking::domain::events::{FocusChange, FocusedApp, PresenceChange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub process_id: u32,
    pub exe_path: String,
    pub app_name: String,
}

#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub app: AppInfo,
    pub window_title: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PresenceEvent {
    pub present: bool,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Focus(FocusEvent),
    /// Focus moved to something that is not recorded: Makora's own window, or no window at all.
    Unattributed { timestamp: DateTime<Utc> },
    Presence(PresenceEvent),
}

impl From<FocusEvent> for WatchEvent {
    fn from(event: FocusEvent) -> Self {
        Self::Focus(event)
    }
}

impl From<PresenceEvent> for WatchEvent {
    fn from(event: PresenceEvent) -> Self {
        Self::Presence(event)
    }
}

impl From<AppInfo> for FocusedApp {
    fn from(app: AppInfo) -> Self {
        Self {
            exe_path: app.exe_path,
            app_name: app.app_name,
        }
    }
}

impl From<FocusEvent> for FocusChange {
    fn from(event: FocusEvent) -> Self {
        Self {
            app: event.app.into(),
            window_title: event.window_title,
            at: event.timestamp,
        }
    }
}

impl From<PresenceEvent> for PresenceChange {
    fn from(event: PresenceEvent) -> Self {
        Self {
            present: event.present,
            at: event.timestamp,
        }
    }
}
