use chrono::{DateTime, Utc};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Chrome,
    Edge,
    Firefox,
    Brave,
    Other,
}

impl BrowserKind {
    pub fn from_app_name(app_name: &str) -> Option<Self> {
        match app_name.to_ascii_lowercase().as_str() {
            "chrome"
            | "google chrome"
            | "google-chrome"
            | "google-chrome-stable"
            | "chromium"
            | "chromium-browser" => Some(Self::Chrome),
            "msedge" | "microsoft edge" | "microsoft-edge" | "microsoft-edge-stable" => {
                Some(Self::Edge)
            }
            "firefox" | "firefox-esr" => Some(Self::Firefox),
            "brave" | "brave browser" | "brave-browser" => Some(Self::Brave),
            _ => None,
        }
    }

    pub fn from_wire(value: &str) -> Self {
        match value {
            "chrome" => Self::Chrome,
            "edge" => Self::Edge,
            "firefox" => Self::Firefox,
            "brave" => Self::Brave,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TabEvent {
    pub browser: BrowserKind,
    pub url: String,
    pub title: String,
    pub timestamp: DateTime<Utc>,
}
