use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedApp {
    pub exe_path: String,
    pub app_name: String,
}

#[derive(Debug, Clone)]
pub struct FocusChange {
    pub app: FocusedApp,
    pub window_title: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TabChange {
    pub browser: BrowserKind,
    pub url: String,
    pub title: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct PresenceChange {
    pub present: bool,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}
