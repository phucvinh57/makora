//! What the tracking context is told about: the app in focus, the tab a browser shows, whether
//! the person is at the machine. The adapters (window watchers, the browser extension) translate
//! whatever they observe into these; nothing here knows how it was observed.

use chrono::{DateTime, Utc};

/// The application whose window has focus.
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

/// A browser reported the tab it is showing.
#[derive(Debug, Clone)]
pub struct TabChange {
    pub browser: BrowserKind,
    pub url: String,
    pub title: String,
    pub at: DateTime<Utc>,
}

/// The person came back to the machine (`present`) or left it (screen locked, asleep).
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
    /// The browser an application's name says it is, if it is one that can report tabs.
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
