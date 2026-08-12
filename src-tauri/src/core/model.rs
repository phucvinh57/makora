use chrono::{DateTime, Utc};

/// Identifies the application owning a focused window.
///
/// `exe_path` (not `app_name` or process id) is the identity used to decide
/// whether a focus change starts a new session: two windows belonging to the
/// same executable are treated as the same "app" for session purposes, and
/// two processes of the same executable are treated as the same app too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub process_id: u32,
    pub exe_path: String,
    pub app_name: String,
}

/// A raw "this window became focused" observation from a platform watcher.
#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub app: AppInfo,
    pub window_title: String,
    pub timestamp: DateTime<Utc>,
}

/// A raw observation of whether the user can currently interact with this
/// desktop session. Linux emits `present: false` for either a screen lock or
/// system sleep and does not infer presence from keyboard/mouse activity.
#[derive(Debug, Clone)]
pub struct PresenceEvent {
    pub present: bool,
    pub timestamp: DateTime<Utc>,
}

/// Events produced by platform watchers and serialized through one channel
/// before they reach [`SessionTracker`](crate::core::tracker::SessionTracker).
/// Keeping focus and presence observations in the same stream prevents a
/// lock boundary from racing a focus observation on separate tracker threads.
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

/// Which browser reported a [`TabEvent`].
///
/// Used to confirm a tab event belongs to the browser that's *currently
/// focused* — without this, an extension in a backgrounded browser could
/// misattribute its tab changes to whatever app the OS says is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Chrome,
    Edge,
    Firefox,
    Brave,
    Other,
}

impl BrowserKind {
    /// Maps an executable file stem (as captured by a `FocusWatcher`, e.g.
    /// `chrome`, `msedge` on Windows, or `Google Chrome`, `Microsoft Edge` on
    /// macOS) to the browser it belongs to.
    ///
    /// Linux mostly needs no special cases — the Debian and tarball builds
    /// install to `/opt/google/chrome/chrome`, `/opt/brave.com/brave/brave`
    /// and `/opt/microsoft/msedge/msedge`, whose stems already match. The
    /// aliases below cover the distro-packaged names that don't, plus the
    /// `app_id`-derived names used when a compositor reports no process id.
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

    /// Maps the extension's self-reported browser string (from
    /// `navigator.userAgent` sniffing on the extension side) to a kind.
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

/// A raw "the active tab changed" observation reported by the browser
/// extension over the local HTTP ingest endpoint.
///
/// `url` is already normalized to origin + path (no query string or
/// fragment) by the extension before it's sent.
#[derive(Debug, Clone)]
pub struct TabEvent {
    pub browser: BrowserKind,
    pub url: String,
    pub title: String,
    pub timestamp: DateTime<Utc>,
}
