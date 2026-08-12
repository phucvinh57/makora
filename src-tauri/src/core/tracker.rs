use std::sync::Arc;

use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::core::category::exe_file_name;
use crate::core::ignore::IgnorePolicy;
#[cfg(test)]
use crate::core::ignore::NoopIgnorePolicy;
use crate::core::model::{AppInfo, BrowserKind, FocusEvent, PresenceEvent, TabEvent};
use crate::core::store::SessionStore;

/// Executables whose focus events are dropped entirely: they neither end the
/// current session nor start a new one, so briefly focusing one (e.g.
/// clicking the desktop) doesn't fragment the session on either side of it.
/// Matched case-insensitively against the exe's file name, not the full path.
///
/// One entry per desktop shell: `explorer.exe` is the Windows desktop and
/// taskbar, `gnome-shell` owns the GNOME panel, notification popups and
/// on-screen keyboard. (GNOME's Activities overview needs no entry — it
/// reports *no* focused window rather than a shell-owned one, which the
/// Linux watcher drops before the tracker ever sees it.)
const IGNORED_APPS: &[&str] = &["explorer.exe", "gnome-shell"];

fn is_ignored(exe_path: &str) -> bool {
    let file_name = exe_file_name(exe_path);
    IGNORED_APPS
        .iter()
        .any(|ignored| ignored.eq_ignore_ascii_case(file_name))
}

/// The currently open activity row — either a plain app focus (`url: None`)
/// or a browser tab (`url: Some`).
struct Current {
    /// `None` when `browser.is_some()` and no tab event has arrived yet: a
    /// browser is focused but no DB row has been opened, since browser rows
    /// are only ever created from tab events, never from the focus hook.
    id: Option<i64>,
    app: AppInfo,
    /// `Some` when this row's app is a known browser — gates whether tab
    /// events are allowed to segment it.
    browser: Option<BrowserKind>,
    url: Option<String>,
    /// Latest window/tab title. This snapshot is retained while away so a
    /// fresh row can begin with current metadata after unlock or wake.
    window_title: String,
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
/// implementation (Windows, macOS, and each of the Linux desktop providers)
/// gets identical behavior for free.
pub struct SessionTracker<S: SessionStore> {
    store: S,
    current: Option<Current>,
    ignore: Arc<dyn IgnorePolicy>,
    /// False while the desktop session is locked or the system is asleep.
    /// Focus and tab observations still refresh `current`, but cannot write
    /// activity rows until a presence event marks the user present again.
    present: bool,
}

impl<S: SessionStore> SessionTracker<S> {
    /// No ignore list configured — nothing beyond the hardcoded
    /// `IGNORED_APPS` is excluded from tracking. See
    /// [`Self::with_ignore_policy`] for the production path.
    #[cfg(test)]
    pub fn new(store: S) -> Result<Self> {
        Self::with_ignore_policy(store, Arc::new(NoopIgnorePolicy))
    }

    pub fn with_ignore_policy(mut store: S, ignore: Arc<dyn IgnorePolicy>) -> Result<Self> {
        store.close_dangling_sessions()?;
        Ok(Self {
            store,
            current: None,
            ignore,
            present: true,
        })
    }

    /// Feed one focus-change observation into the tracker.
    pub fn handle_event(&mut self, event: FocusEvent) -> Result<()> {
        if is_ignored(&event.app.exe_path) {
            return Ok(());
        }
        if let Some(current) = &self.current {
            if current.app.exe_path == event.app.exe_path {
                if !self.present {
                    // Focus providers can re-sync while locked/unlocked.
                    // Keep the newest metadata for the row that will open
                    // on return without changing same-app semantics while
                    // the user is present.
                    let current = self.current.as_mut().expect("checked above");
                    current.app = event.app;
                    current.window_title = event.window_title;
                }
                return Ok(()); // same app still focused (e.g. window title changed)
            }
        }
        self.end_current_row(event.timestamp)?;
        let browser = BrowserKind::from_app_name(&event.app.app_name);
        self.current = Some(Current {
            id: None,
            app: event.app,
            browser,
            url: None,
            window_title: event.window_title,
        });
        self.open_current_row(event.timestamp)
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
        self.end_current_row(event.timestamp)?;
        let current = self.current.as_mut().expect("checked above");
        current.url = Some(event.url);
        current.window_title = event.title;
        self.open_current_row(event.timestamp)
    }

    /// Feed one authoritative user-presence observation into the tracker.
    /// A transition away closes the database row but deliberately retains
    /// the focused app/tab snapshot. Focus and tab events can update that
    /// snapshot while away, and a transition back opens a fresh row from it.
    pub fn handle_presence_event(&mut self, event: PresenceEvent) -> Result<()> {
        if self.present == event.present {
            return Ok(());
        }
        self.present = event.present;
        if event.present {
            self.open_current_row(event.timestamp)
        } else {
            self.end_current_row(event.timestamp)
        }
    }

    /// Close whatever row is open, e.g. on shutdown. No-op if none is open.
    pub fn close_current(&mut self, at: DateTime<Utc>) -> Result<()> {
        self.end_current_row(at)?;
        self.current = None;
        Ok(())
    }

    /// Close only the persisted row, retaining the focused app/tab snapshot.
    fn end_current_row(&mut self, at: DateTime<Utc>) -> Result<()> {
        let id = self.current.as_ref().and_then(|current| current.id);
        if let Some(id) = id {
            self.store.end_session(id, at)?;
            self.current.as_mut().expect("row came from current").id = None;
        }
        Ok(())
    }

    /// Open a row from the retained snapshot when the user is present.
    /// Browser focus without a tab observation, ignored apps, and ignored
    /// sites intentionally retain a snapshot but write no row.
    fn open_current_row(&mut self, at: DateTime<Utc>) -> Result<()> {
        if !self.present
            || self
                .current
                .as_ref()
                .is_none_or(|current| current.id.is_some())
        {
            return Ok(());
        }

        let current = self.current.as_ref().expect("checked above");
        if self.ignore.is_app_ignored(&current.app.exe_path) {
            return Ok(());
        }
        if current.browser.is_some()
            && current
                .url
                .as_deref()
                .is_none_or(|url| self.ignore.is_site_ignored(url))
        {
            // Browsers only produce URL-bearing rows, never a duplicate
            // plain-app row before the extension reports its active tab.
            return Ok(());
        }

        let app = current.app.clone();
        let title = current.window_title.clone();
        let url = current.url.clone();
        let id = self.store.start_session(&app, &title, url.as_deref(), at)?;
        self.current.as_mut().expect("checked above").id = Some(id);
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
        fn start_session(
            &mut self,
            app: &AppInfo,
            _window_title: &str,
            url: Option<&str>,
            _at: DateTime<Utc>,
        ) -> Result<i64> {
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

    /// An [`IgnorePolicy`] that ignores exactly the exe/site substrings it's
    /// constructed with, for testing `SessionTracker::with_ignore_policy`.
    struct FakeIgnorePolicy {
        apps: &'static [&'static str],
        sites: &'static [&'static str],
    }

    impl IgnorePolicy for FakeIgnorePolicy {
        fn is_app_ignored(&self, exe_path: &str) -> bool {
            self.apps.iter().any(|a| exe_path.contains(a))
        }

        fn is_site_ignored(&self, url: &str) -> bool {
            self.sites.iter().any(|s| url.contains(s))
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

    /// A focus event for a recognized browser exe reported under a specific
    /// `app_name` — lets tests exercise `BrowserKind::from_app_name` across
    /// the different exe stems platforms report (e.g. `chrome` on Windows vs.
    /// `Google Chrome` on macOS), unlike `browser_event` which always uses
    /// the Windows stem.
    fn browser_event_named(exe: &str, app_name: &str) -> FocusEvent {
        FocusEvent {
            app: AppInfo {
                process_id: 1,
                exe_path: exe.to_string(),
                app_name: app_name.to_string(),
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

    fn presence_event(present: bool, timestamp: DateTime<Utc>) -> PresenceEvent {
        PresenceEvent { present, timestamp }
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
        tracker
            .handle_event(event(r"C:\Windows\explorer.exe", "File Explorer"))
            .unwrap();
        tracker.handle_event(event("a.exe", "A")).unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:a.exe"]);
    }

    #[test]
    fn ignored_app_is_matched_case_insensitively_by_file_name() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker
            .handle_event(event(r"C:\Windows\Explorer.EXE", "File Explorer"))
            .unwrap();

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
                "start:chrome.exe:https://a.example/",
                "end:1",
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

        assert_eq!(*log.0.borrow(), vec!["start:chrome.exe:https://a.example/"]);
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
            vec![
                "start:chrome.exe:https://a.example/",
                "end:1",
                "start:b.exe"
            ]
        );
    }

    #[test]
    fn tab_event_ignored_when_no_browser_is_focused() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker
            .handle_event(event("notepad.exe", "Untitled"))
            .unwrap();
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

        assert_eq!(*log.0.borrow(), Vec::<String>::new());
    }

    #[test]
    fn browser_focus_without_tab_event_records_nothing() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker.handle_event(event("b.exe", "B")).unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:b.exe"]);
    }

    #[test]
    fn macos_chrome_exe_stem_is_recognized_as_a_browser() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker
            .handle_event(browser_event_named(
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "Google Chrome",
            ))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec!["start:/Applications/Google Chrome.app/Contents/MacOS/Google Chrome:https://a.example/"]
        );
    }

    #[test]
    fn macos_edge_exe_stem_is_recognized_as_a_browser() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker
            .handle_event(browser_event_named("msedge.app", "Microsoft Edge"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Edge, "https://a.example/"))
            .unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:msedge.app:https://a.example/"]);
    }

    #[test]
    fn macos_brave_exe_stem_is_recognized_as_a_browser() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker
            .handle_event(browser_event_named("brave.app", "Brave Browser"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Brave, "https://a.example/"))
            .unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:brave.app:https://a.example/"]);
    }

    #[test]
    fn ignore_policy_closes_previous_session_but_opens_none_for_an_ignored_app() {
        let log = RecordingStore::default();
        let ignore = Arc::new(FakeIgnorePolicy {
            apps: &["steam"],
            sites: &[],
        });
        let mut tracker = SessionTracker::with_ignore_policy(log.clone(), ignore).unwrap();

        tracker.handle_event(event("a.exe", "A")).unwrap();
        tracker.handle_event(event("steam.exe", "Steam")).unwrap();
        tracker.handle_event(event("b.exe", "B")).unwrap();

        // a.exe's session ends when steam.exe gains focus, but steam.exe
        // itself never starts one — and b.exe starts cleanly afterward.
        assert_eq!(*log.0.borrow(), vec!["start:a.exe", "end:1", "start:b.exe"]);
    }

    #[test]
    fn ignore_policy_ignores_a_site_without_leaving_the_previous_tab_open() {
        let log = RecordingStore::default();
        let ignore = Arc::new(FakeIgnorePolicy {
            apps: &[],
            sites: &["youtube.com"],
        });
        let mut tracker = SessionTracker::with_ignore_policy(log.clone(), ignore).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://youtube.com/watch"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://b.example/"))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec![
                "start:chrome.exe:https://a.example/",
                "end:1",
                "start:chrome.exe:https://b.example/",
            ]
        );
    }

    #[test]
    fn revisiting_the_same_ignored_site_is_a_noop() {
        let log = RecordingStore::default();
        let ignore = Arc::new(FakeIgnorePolicy {
            apps: &[],
            sites: &["youtube.com"],
        });
        let mut tracker = SessionTracker::with_ignore_policy(log.clone(), ignore).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://youtube.com/watch"))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://youtube.com/watch"))
            .unwrap();

        assert_eq!(*log.0.borrow(), Vec::<String>::new());
    }

    #[test]
    fn lock_closes_an_app_row_and_unlock_opens_a_fresh_one() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("editor", "Draft")).unwrap();
        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec!["start:editor", "end:1", "start:editor"]
        );
    }

    #[test]
    fn lock_and_unlock_use_the_presence_event_timestamps() {
        type TimedLog = Rc<RefCell<Vec<(String, DateTime<Utc>)>>>;

        #[derive(Default, Clone)]
        struct TimedStore(TimedLog);

        impl SessionStore for TimedStore {
            fn start_session(
                &mut self,
                app: &AppInfo,
                _window_title: &str,
                _url: Option<&str>,
                at: DateTime<Utc>,
            ) -> Result<i64> {
                let mut log = self.0.borrow_mut();
                log.push((format!("start:{}", app.exe_path), at));
                Ok(log.len() as i64)
            }

            fn end_session(&mut self, id: i64, at: DateTime<Utc>) -> Result<()> {
                self.0.borrow_mut().push((format!("end:{id}"), at));
                Ok(())
            }

            fn close_dangling_sessions(&mut self) -> Result<()> {
                Ok(())
            }
        }

        let start = DateTime::from_timestamp(10, 0).unwrap();
        let locked = DateTime::from_timestamp(20, 0).unwrap();
        let unlocked = DateTime::from_timestamp(30, 0).unwrap();
        let log = TimedStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();
        let mut focus = event("editor", "Draft");
        focus.timestamp = start;

        tracker.handle_event(focus).unwrap();
        tracker
            .handle_presence_event(presence_event(false, locked))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(true, unlocked))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec![
                ("start:editor".into(), start),
                ("end:1".into(), locked),
                ("start:editor".into(), unlocked),
            ]
        );
    }

    #[test]
    fn browser_tab_row_is_split_across_lock_and_unlock() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(
                BrowserKind::Chrome,
                "https://youtube.com/watch?v=one",
            ))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec![
                "start:chrome.exe:https://youtube.com/watch?v=one",
                "end:1",
                "start:chrome.exe:https://youtube.com/watch?v=one",
            ]
        );
    }

    #[test]
    fn focus_and_tab_changes_while_away_only_update_the_resume_snapshot() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("editor", "Draft")).unwrap();
        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://b.example/"))
            .unwrap();

        assert_eq!(*log.0.borrow(), vec!["start:editor", "end:1"]);

        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();
        assert_eq!(
            *log.0.borrow(),
            vec![
                "start:editor",
                "end:1",
                "start:chrome.exe:https://b.example/",
            ]
        );
    }

    #[test]
    fn startup_focus_observation_while_locked_writes_nothing_until_unlock() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker.handle_event(event("editor", "Draft")).unwrap();
        assert_eq!(*log.0.borrow(), Vec::<String>::new());

        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();
        assert_eq!(*log.0.borrow(), vec!["start:editor"]);
    }

    #[test]
    fn duplicate_presence_events_do_not_duplicate_row_boundaries() {
        let log = RecordingStore::default();
        let mut tracker = SessionTracker::new(log.clone()).unwrap();

        tracker.handle_event(event("editor", "Draft")).unwrap();
        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec!["start:editor", "end:1", "start:editor"]
        );
    }

    #[test]
    fn ignored_site_selected_while_away_does_not_resume_a_row() {
        let log = RecordingStore::default();
        let ignore = Arc::new(FakeIgnorePolicy {
            apps: &[],
            sites: &["youtube.com"],
        });
        let mut tracker = SessionTracker::with_ignore_policy(log.clone(), ignore).unwrap();

        tracker.handle_event(browser_event("chrome.exe")).unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://a.example/"))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(false, Utc::now()))
            .unwrap();
        tracker
            .handle_tab_event(tab_event(BrowserKind::Chrome, "https://youtube.com/watch"))
            .unwrap();
        tracker
            .handle_presence_event(presence_event(true, Utc::now()))
            .unwrap();

        assert_eq!(
            *log.0.borrow(),
            vec!["start:chrome.exe:https://a.example/", "end:1"]
        );
    }
}
