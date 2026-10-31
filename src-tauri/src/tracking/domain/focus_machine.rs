use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::shared::ids::SessionId;
use crate::tracking::ports::{ExclusionPolicy, SessionRecorder, SessionTracker};

use super::events::{BrowserKind, FocusChange, FocusedApp, PresenceChange, TabChange};
use super::session_policy::{is_ignored_app, is_noise};
use super::source_key::{canonical_tab_url, normalized_domain};
use super::title::canonical_title;

/// What has focus.
struct Current {
    app: FocusedApp,
    browser: Option<BrowserKind>,
    url: Option<String>,
    window_title: String,
}

impl Current {
    fn has_tab(&self) -> bool {
        self.browser.is_some() && self.url.is_some()
    }

    /// A tab is recorded by its site, so one without a domain (a new-tab or settings page) is not
    /// recorded at all.
    fn is_recordable(&self) -> bool {
        self.browser.is_none() || self.url.as_deref().is_none_or(|url| normalized_domain(url).is_some())
    }

    fn title_key(&self) -> String {
        canonical_title(&self.window_title, &self.app.app_name, self.url.as_deref())
    }

    fn recorded(&self) -> Recorded {
        Recorded {
            exe_path: self.app.exe_path.clone(),
            url: self.url.clone(),
            title: self.title_key(),
        }
    }
}

/// What the open row was recorded against.
struct Recorded {
    exe_path: String,
    url: Option<String>,
    title: String,
}

enum Row {
    Closed,
    Open {
        id: SessionId,
        started_at: DateTime<Utc>,
        recorded: Recorded,
    },
}

enum Step {
    Keep,
    Open,
    Close,
    Retitle,
    Split,
}

struct LastTab {
    url: String,
    title: String,
}

pub struct FocusStateMachine {
    recorder: Box<dyn SessionRecorder>,
    exclusions: Box<dyn ExclusionPolicy>,
    current: Option<Current>,
    row: Row,
    present: bool,
    /// Latest tab the extension reported per browser. The extension only reports on
    /// change, so this is what a browser regaining focus is still showing.
    last_tabs: HashMap<BrowserKind, LastTab>,
    /// Events reach the machine from several sources, each stamped when it was seen; time never
    /// runs backwards past the latest one.
    last_at: Option<DateTime<Utc>>,
}

impl FocusStateMachine {
    pub fn new(recorder: Box<dyn SessionRecorder>, exclusions: Box<dyn ExclusionPolicy>) -> Result<Self> {
        recorder.close_dangling()?;
        Ok(Self {
            recorder,
            exclusions,
            current: None,
            row: Row::Closed,
            present: true,
            last_tabs: HashMap::new(),
            last_at: None,
        })
    }

    fn clock(&mut self, at: DateTime<Utc>) -> DateTime<Utc> {
        let at = self.last_at.map_or(at, |last| last.max(at));
        self.last_at = Some(at);
        at
    }

    fn wanted(&self) -> Option<&Current> {
        if !self.present {
            return None;
        }
        self.current.as_ref().filter(|current| {
            current.is_recordable()
                && !self
                    .exclusions
                    .is_excluded(&current.app, &current.window_title, current.url.as_deref())
        })
    }

    /// The one place that decides whether the row opens, closes, moves to a new title or splits.
    fn reconcile(&mut self, at: DateTime<Utc>) -> Result<()> {
        let step = match (&self.row, self.wanted()) {
            (Row::Closed, None) => Step::Keep,
            (Row::Closed, Some(_)) => Step::Open,
            (Row::Open { .. }, None) => Step::Close,
            (
                Row::Open {
                    started_at, recorded, ..
                },
                Some(current),
            ) => step_for_change(recorded, current, is_noise(*started_at, at, self.recorder.min_session_seconds())),
        };
        match step {
            Step::Keep => Ok(()),
            Step::Open => self.open_row(at),
            Step::Close => self.close_row(at),
            Step::Retitle => self.retitle_row(),
            Step::Split => {
                self.close_row(at)?;
                self.open_row(at)
            }
        }
    }

    fn open_row(&mut self, at: DateTime<Utc>) -> Result<()> {
        let Some(current) = &self.current else {
            return Ok(());
        };
        let opened = self
            .recorder
            .start(&current.app, &current.window_title, current.url.as_deref(), at)?;
        self.row = Row::Open {
            id: opened.id,
            started_at: opened.started_at,
            recorded: current.recorded(),
        };
        Ok(())
    }

    fn close_row(&mut self, at: DateTime<Utc>) -> Result<()> {
        if let Row::Open { id, .. } = self.row {
            self.recorder.end(id, at)?;
            self.row = Row::Closed;
        }
        Ok(())
    }

    fn retitle_row(&mut self) -> Result<()> {
        let (Row::Open { id, recorded, .. }, Some(current)) = (&mut self.row, &self.current) else {
            return Ok(());
        };
        self.recorder
            .retitle(*id, &current.app, &current.window_title, current.url.as_deref())?;
        recorded.title = current.title_key();
        Ok(())
    }
}

/// How an open row follows what now has focus. A title change on a row too young to keep moves
/// the row to the new title instead of splitting it, so a window whose title settles after a
/// moment loses no time. A tab is one block per page: its title only ever fills in a placeholder.
fn step_for_change(recorded: &Recorded, current: &Current, young: bool) -> Step {
    if recorded.exe_path != current.app.exe_path || recorded.url != current.url {
        return Step::Split;
    }
    let title = current.title_key();
    if title == recorded.title {
        return Step::Keep;
    }
    if current.has_tab() {
        let fills_in = !title.is_empty() && (young || recorded.title.is_empty());
        return if fills_in { Step::Retitle } else { Step::Keep };
    }
    if young {
        Step::Retitle
    } else {
        Step::Split
    }
}

impl SessionTracker for FocusStateMachine {
    fn handle_focus(&mut self, change: FocusChange) -> Result<()> {
        let at = self.clock(change.at);
        if is_ignored_app(&change.app.exe_path) {
            return Ok(());
        }
        match &mut self.current {
            Some(current) if current.app.exe_path == change.app.exe_path => {
                if !current.has_tab() {
                    current.window_title = change.window_title;
                }
                current.app = change.app;
            }
            _ => {
                let browser = BrowserKind::from_app_name(&change.app.app_name);
                let tab = browser.and_then(|kind| self.last_tabs.get(&kind));
                let (url, window_title) = match tab {
                    Some(tab) => (Some(tab.url.clone()), tab.title.clone()),
                    None => (None, change.window_title),
                };
                self.current = Some(Current {
                    app: change.app,
                    browser,
                    url,
                    window_title,
                });
            }
        }
        self.reconcile(at)
    }

    fn handle_tab(&mut self, change: TabChange) -> Result<()> {
        let at = self.clock(change.at);
        let url = canonical_tab_url(&change.url);
        self.last_tabs.insert(
            change.browser,
            LastTab {
                url: url.clone(),
                title: change.title.clone(),
            },
        );
        match &mut self.current {
            Some(current) if current.browser == Some(change.browser) => {
                current.url = Some(url);
                current.window_title = change.title;
            }
            _ => return Ok(()),
        }
        self.reconcile(at)
    }

    fn handle_presence(&mut self, change: PresenceChange) -> Result<()> {
        let at = self.clock(change.at);
        self.present = change.present;
        self.reconcile(at)
    }

    fn heartbeat(&mut self, at: DateTime<Utc>) -> Result<()> {
        let at = self.clock(at);
        match self.row {
            Row::Open { id, .. } => self.recorder.touch(id, at),
            Row::Closed => Ok(()),
        }
    }

    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()> {
        let at = self.clock(at);
        self.current = None;
        self.reconcile(at)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::{Duration, TimeZone};

    use super::*;
    use crate::tracking::ports::OpenedRow;

    const MIN_SECONDS: i64 = 5;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Start { title: String, url: Option<String>, at: i64 },
        End { id: i64, at: i64 },
        Retitle { id: i64, title: String },
        Touch { id: i64, at: i64 },
    }

    #[derive(Clone, Default)]
    struct Log(Arc<Mutex<Vec<Call>>>);

    impl Log {
        fn push(&self, call: Call) {
            self.0.lock().unwrap().push(call);
        }

        fn take(&self) -> Vec<Call> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    struct FakeRecorder {
        log: Log,
        next_id: Mutex<i64>,
    }

    impl SessionRecorder for FakeRecorder {
        fn start(&self, _app: &FocusedApp, title: &str, url: Option<&str>, at: DateTime<Utc>) -> Result<OpenedRow> {
            self.log.push(Call::Start {
                title: title.into(),
                url: url.map(Into::into),
                at: secs(at),
            });
            let mut next = self.next_id.lock().unwrap();
            *next += 1;
            Ok(OpenedRow {
                id: SessionId(*next),
                started_at: at,
            })
        }

        fn end(&self, id: SessionId, at: DateTime<Utc>) -> Result<()> {
            self.log.push(Call::End { id: id.0, at: secs(at) });
            Ok(())
        }

        fn retitle(&self, id: SessionId, _app: &FocusedApp, title: &str, _url: Option<&str>) -> Result<()> {
            self.log.push(Call::Retitle {
                id: id.0,
                title: title.into(),
            });
            Ok(())
        }

        fn touch(&self, id: SessionId, at: DateTime<Utc>) -> Result<()> {
            self.log.push(Call::Touch { id: id.0, at: secs(at) });
            Ok(())
        }

        fn close_dangling(&self) -> Result<()> {
            Ok(())
        }

        fn min_session_seconds(&self) -> i64 {
            MIN_SECONDS
        }
    }

    struct TitleExclusions(Vec<&'static str>);

    impl ExclusionPolicy for TitleExclusions {
        fn is_excluded(&self, _app: &FocusedApp, title: &str, _url: Option<&str>) -> bool {
            self.0.contains(&title)
        }
    }

    fn epoch() -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).unwrap()
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        epoch() + Duration::seconds(seconds)
    }

    fn secs(at: DateTime<Utc>) -> i64 {
        (at - epoch()).num_seconds()
    }

    fn machine(excluded: Vec<&'static str>) -> (FocusStateMachine, Log) {
        let log = Log::default();
        let recorder = FakeRecorder {
            log: log.clone(),
            next_id: Mutex::new(0),
        };
        let machine = FocusStateMachine::new(Box::new(recorder), Box::new(TitleExclusions(excluded))).unwrap();
        (machine, log)
    }

    fn focus(app: &str, title: &str, seconds: i64) -> FocusChange {
        FocusChange {
            app: FocusedApp {
                exe_path: format!("/usr/bin/{app}"),
                app_name: app.into(),
            },
            window_title: title.into(),
            at: at(seconds),
        }
    }

    fn tab(url: &str, title: &str, seconds: i64) -> TabChange {
        TabChange {
            browser: BrowserKind::Chrome,
            url: url.into(),
            title: title.into(),
            at: at(seconds),
        }
    }

    fn presence(present: bool, seconds: i64) -> PresenceChange {
        PresenceChange {
            present,
            at: at(seconds),
        }
    }

    fn start(title: &str, url: Option<&str>, at: i64) -> Call {
        Call::Start {
            title: title.into(),
            url: url.map(Into::into),
            at,
        }
    }

    #[test]
    fn tab_reopens_once_its_title_is_no_longer_excluded() {
        let (mut machine, log) = machine(vec!["Secret"]);
        machine.handle_focus(focus("chrome", "Chrome", 0)).unwrap();
        machine.handle_tab(tab("https://a.com/x", "Public", 0)).unwrap();
        machine.handle_tab(tab("https://a.com/x", "Secret", 60)).unwrap();
        machine.handle_tab(tab("https://a.com/x", "Public", 120)).unwrap();
        assert_eq!(
            log.take(),
            [
                start("Chrome", None, 0),
                Call::End { id: 1, at: 0 },
                start("Public", Some("https://a.com/x"), 0),
                Call::End { id: 2, at: 60 },
                start("Public", Some("https://a.com/x"), 120),
            ]
        );
    }

    #[test]
    fn earlier_timestamps_never_run_time_backwards() {
        let (mut machine, log) = machine(vec![]);
        machine.handle_focus(focus("code", "main.rs", 100)).unwrap();
        machine.handle_focus(focus("slack", "general", 40)).unwrap();
        assert_eq!(
            log.take(),
            [start("main.rs", None, 100), Call::End { id: 1, at: 100 }, start("general", None, 100)]
        );
    }

    #[test]
    fn title_change_on_a_young_row_retitles_it() {
        let (mut machine, log) = machine(vec![]);
        machine.handle_focus(focus("code", "loading", 0)).unwrap();
        machine.handle_focus(focus("code", "main.rs", 2)).unwrap();
        machine.handle_focus(focus("code", "lib.rs", 60)).unwrap();
        assert_eq!(
            log.take(),
            [
                start("loading", None, 0),
                Call::Retitle {
                    id: 1,
                    title: "main.rs".into()
                },
                Call::End { id: 1, at: 60 },
                start("lib.rs", None, 60),
            ]
        );
    }

    #[test]
    fn tab_title_change_on_an_old_row_keeps_it_unless_it_was_a_placeholder() {
        let (mut machine, log) = machine(vec![]);
        machine.handle_focus(focus("chrome", "Chrome", 0)).unwrap();
        machine.handle_tab(tab("https://a.com/x", "a.com", 0)).unwrap();
        machine.handle_tab(tab("https://a.com/x", "Inbox", 60)).unwrap();
        machine.handle_tab(tab("https://a.com/x", "Inbox (1)", 120)).unwrap();
        assert_eq!(
            log.take(),
            [
                start("Chrome", None, 0),
                Call::End { id: 1, at: 0 },
                start("a.com", Some("https://a.com/x"), 0),
                Call::Retitle {
                    id: 2,
                    title: "Inbox".into()
                },
            ]
        );
    }

    #[test]
    fn presence_closes_the_row_and_reopens_what_has_focus_on_return() {
        let (mut machine, log) = machine(vec![]);
        machine.handle_focus(focus("code", "main.rs", 0)).unwrap();
        machine.handle_presence(presence(false, 30)).unwrap();
        machine.handle_focus(focus("slack", "general", 40)).unwrap();
        machine.heartbeat(at(45)).unwrap();
        machine.handle_presence(presence(true, 50)).unwrap();
        machine.heartbeat(at(80)).unwrap();
        assert_eq!(
            log.take(),
            [
                start("main.rs", None, 0),
                Call::End { id: 1, at: 30 },
                start("general", None, 50),
                Call::Touch { id: 2, at: 80 },
            ]
        );
    }

    #[test]
    fn unattributed_focus_closes_the_row_and_the_next_focus_opens_one() {
        let (mut machine, log) = machine(vec![]);
        machine.handle_focus(focus("code", "main.rs", 0)).unwrap();
        machine.close_current(at(30)).unwrap();
        machine.heartbeat(at(40)).unwrap();
        machine.handle_focus(focus("code", "main.rs", 50)).unwrap();
        assert_eq!(
            log.take(),
            [start("main.rs", None, 0), Call::End { id: 1, at: 30 }, start("main.rs", None, 50)]
        );
    }
}
