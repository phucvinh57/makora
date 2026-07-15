use std::path::Path;

use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::core::model::{AppInfo, BrowserKind, FocusEvent, TabEvent};
use crate::core::store::SessionStore;

/// Executables whose focus events are dropped entirely: they neither end the
/// current session nor start a new one, so briefly focusing one (e.g.
/// clicking the desktop) doesn't fragment the session on either side of it.
/// Matched case-insensitively against the exe's file name, not the full path.
const IGNORED_APPS: &[&str] = &["explorer.exe"];

fn is_ignored(exe_path: &str) -> bool {
    let file_name = Path::new(exe_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(exe_path);
    IGNORED_APPS.iter().any(|ignored| ignored.eq_ignore_ascii_case(file_name))
}

/// The currently open activity row — either a plain app focus (`url: None`)
/// or a browser tab (`url: Some`).
struct Current {
    id: i64,
    app: AppInfo,
    /// `Some` when this row's app is a known browser — gates whether tab
    /// events are allowed to segment it.
    browser: Option<BrowserKind>,
    url: Option<String>,
}

/// Turns a stream of raw focus events into activity rows: a row opens when
/// an app first gains focus and closes the moment a *different* app gains
/// focus. Two windows of the same executable are the same "app" and do not
/// end the row — only switching to a different executable does.
///
/// When the focused app is a browser, `handle_tab_event` further segments
/// that app's focus into one row per URL, closing the current row and
/// opening a new one for the same app — exactly like switching apps does —
/// so time in a browser can be broken down by site without a separate
/// parent/child table.
///
/// This is the one place session semantics live, so every `FocusWatcher`
/// implementation (Windows today, macOS later) gets identical behavior for
/// free.
pub struct SessionTracker<S: SessionStore> {
    store: S,
    current: Option<Current>,
}

impl<S: SessionStore> SessionTracker<S> {
    pub fn new(mut store: S) -> Result<Self> {
        store.close_dangling_sessions()?;
        Ok(Self { store, current: None })
    }

    /// Feed one focus-change observation into the tracker.
    pub fn handle_event(&mut self, event: FocusEvent) -> Result<()> {
        if is_ignored(&event.app.exe_path) {
            return Ok(());
        }
        if let Some(current) = &self.current {
            if current.app.exe_path == event.app.exe_path {
                return Ok(()); // same app still focused (e.g. window title changed)
            }
        }
        self.close_current(event.timestamp)?;
        let browser = BrowserKind::from_app_name(&event.app.app_name);
        let id = self
            .store
            .start_session(&event.app, &event.window_title, None, event.timestamp)?;
        self.current = Some(Current {
            id,
            app: event.app,
            browser,
            url: None,
        });
        Ok(())
    }

    /// Feed one active-tab observation into the tracker. Ignored unless a
    /// session for a *matching, currently-focused* browser is open — this
    /// stops a backgrounded browser's extension from misattributing its tab
    /// changes to whatever app the OS says is focused.
    pub fn handle_tab_event(&mut self, event: TabEvent) -> Result<()> {
        let Some(current) = &self.current else {
            return Ok(());
        };
        if current.browser != Some(event.browser) {
            return Ok(());
        }
        if current.url.as_deref() == Some(event.url.as_str()) {
            return Ok(()); // same URL, e.g. only the title changed
        }
        let current = self.current.take().expect("checked above");
        self.store.end_session(current.id, event.timestamp)?;
        let id = self
            .store
            .start_session(&current.app, &event.title, Some(&event.url), event.timestamp)?;
        self.current = Some(Current {
            id,
            app: current.app,
            browser: current.browser,
            url: Some(event.url),
        });
        Ok(())
    }

    /// Close whatever row is open, e.g. on shutdown. No-op if none is open.
    pub fn close_current(&mut self, at: DateTime<Utc>) -> Result<()> {
        if let Some(current) = self.current.take() {
            self.store.end_session(current.id, at)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::AppInfo;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default, Clone)]
    struct RecordingStore(Rc<RefCell<Vec<String>>>);

    impl SessionStore for RecordingStore {
        fn start_session(&mut self, app: &AppInfo, _window_title: &str, url: Option<&str>, _at: DateTime<Utc>) -> Result<i64> {
            let mut log = self.0.borrow_mut();
            match url {
                Some(url) => log.push(format!("start:{}:{url}", app.exe_path)),
                None => log.push(format!("start:{}", app.exe_path)),
            }
            Ok(log.len() as i64)
        }

        fn end_session(&mut self, id: i64, _at: DateTime<Utc>) -> Result<()> {
            self.0.borrow_mut().push(format!("end:{id}"));
            Ok(())
        }

        fn close_dangling_sessions(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn event(exe: &str, title: &str) -> FocusEvent {
        FocusEvent {
            app: AppInfo {
                process_id: 1,
                exe_path: exe.to_string(),
                app_name: exe.to_string(),
            },
            window_title: title.to_string(),
            timestamp: Utc::now(),
        }
    }

    /// A focus event for a recognized browser exe (`app_name` must match
    /// `BrowserKind::from_app_name`, unlike the generic `event` helper above).
    fn browser_event(exe: &str) -> FocusEvent {
        FocusEvent {
            app: AppInfo {
                process_id: 1,
                exe_path: exe.to_string(),
                app_name: "chrome".to_string(),
            },
            window_title: "New Tab".to_string(),
            timestamp: Utc::now(),
        }
    }

    fn tab_event(browser: BrowserKind, url: &str) -> TabEvent {
        TabEvent {
            browser,
            url: url.to_string(),
            title: "Title".to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn switching_apps_ends_previous_session_and_starts_new_one() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("a.exe", "A")).unwrap();
        tracker.handle_event(event("b.exe", "B")).unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:a.exe", "end:1", "start:b.exe"]);
    }

    #[test]
    fn ignored_app_does_not_interrupt_current_session() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("a.exe", "A")).unwrap();
        tracker.handle_event(event(r"C:\Windows\explorer.exe", "File Explorer")).unwrap();
        tracker.handle_event(event("a.exe", "A")).unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:a.exe"]);
    }

    #[test]
    fn ignored_app_is_matched_case_insensitively_by_file_name() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event(r"C:\Windows\Explorer.EXE", "File Explorer")).unwrap();

        assert_eq!(*log.0.borrow(), Vec::<String>::new());
    }

    #[test]
    fn refocusing_same_exe_does_not_start_a_new_session() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("a.exe", "Window 1")).unwrap();
        tracker.handle_event(event("a.exe", "Window 2")).unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:a.exe"]);
    }

    #[test]
    fn close_current_ends_open_session_once() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("a.exe", "A")).unwrap();
        tracker.close_current(Utc::now()).unwrap();
        tracker.close_current(Utc::now()).unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:a.exe", "end:1"]);
    }

    #[test]
    fn tab_url_change_segments_into_new_tab_session() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://b.example/"))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec![
                "start:chrome.exe",
                "end:1",
                "start:chrome.exe:https://a.example/",
                "end:3",
                "start:chrome.exe:https://b.example/",
            ]
        );
    }

    #[test]
    fn same_url_tab_event_is_a_noop() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec!["start:chrome.exe", "end:1", "start:chrome.exe:https://a.example/"]
        );
    }

    #[test]
    fn switching_apps_closes_open_tab_session() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();
        tracker.handle_event(event("b.exe", "B")).unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec!["start:chrome.exe", "end:1", "start:chrome.exe:https://a.example/", "end:3", "start:b.exe"]
        );
    }

    #[test]
    fn tab_event_ignored_when_no_browser_is_focused() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("notepad.exe", "Untitled")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:notepad.exe"]);
    }

    #[test]
    fn tab_event_ignored_when_reported_browser_does_not_match_focused_one() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Firefox, "https://a.example/"))
            .unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:chrome.exe"]);
    }
}
