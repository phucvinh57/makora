use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::shared::ids::SessionId;
use crate::tracking::ports::{ExclusionPolicy, SessionRecorder, SessionTracker};

use super::events::{BrowserKind, FocusChange, FocusedApp, PresenceChange, TabChange};
use super::session_policy::is_ignored_app;

struct Current {
    id: Option<SessionId>,
    app: FocusedApp,
    browser: Option<BrowserKind>,
    url: Option<String>,
    window_title: String,
}

struct LastTab {
    url: String,
    title: String,
}

pub struct FocusStateMachine {
    recorder: Box<dyn SessionRecorder>,
    exclusions: Box<dyn ExclusionPolicy>,
    current: Option<Current>,
    /// Latest tab the extension reported per browser. The extension only reports on
    /// change, so this is what a browser regaining focus is still showing.
    last_tabs: HashMap<BrowserKind, LastTab>,
    present: bool,
}

impl FocusStateMachine {
    pub fn new(recorder: Box<dyn SessionRecorder>, exclusions: Box<dyn ExclusionPolicy>) -> Result<Self> {
        recorder.close_dangling()?;
        Ok(Self {
            recorder,
            exclusions,
            current: None,
            last_tabs: HashMap::new(),
            present: true,
        })
    }

    fn end_current_row(&mut self, at: DateTime<Utc>) -> Result<()> {
        let id = self.current.as_ref().and_then(|current| current.id);
        if let Some(id) = id {
            self.recorder.end(id, at)?;
            self.current.as_mut().expect("row came from current").id = None;
        }
        Ok(())
    }

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
        let app = current.app.clone();
        let title = current.window_title.clone();
        let url = current.url.clone();
        if self.exclusions.is_excluded(&app, &title, url.as_deref()) {
            return Ok(());
        }
        let id = self.recorder.start(&app, &title, url.as_deref(), at)?;
        self.current.as_mut().expect("checked above").id = Some(id);
        Ok(())
    }
}

impl SessionTracker for FocusStateMachine {
    fn handle_focus(&mut self, change: FocusChange) -> Result<()> {
        if is_ignored_app(&change.app.exe_path) {
            return Ok(());
        }
        if let Some(current) = &self.current {
            if current.app.exe_path == change.app.exe_path {
                if !self.present {
                    let current = self.current.as_mut().expect("checked above");
                    current.app = change.app;
                    current.window_title = change.window_title;
                }
                return Ok(());
            }
        }
        self.end_current_row(change.at)?;
        let browser = BrowserKind::from_app_name(&change.app.app_name);
        let tab = browser.and_then(|kind| self.last_tabs.get(&kind));
        let (url, window_title) = match tab {
            Some(tab) => (Some(tab.url.clone()), tab.title.clone()),
            None => (None, change.window_title),
        };
        self.current = Some(Current {
            id: None,
            app: change.app,
            browser,
            url,
            window_title,
        });
        self.open_current_row(change.at)
    }

    fn handle_tab(&mut self, change: TabChange) -> Result<()> {
        self.last_tabs.insert(
            change.browser,
            LastTab {
                url: change.url.clone(),
                title: change.title.clone(),
            },
        );
        let Some(current) = &self.current else {
            return Ok(());
        };
        if current.browser != Some(change.browser) {
            return Ok(());
        }
        if current.url.as_deref() == Some(change.url.as_str()) {
            let id = current.id;
            // The real title can be what an exclusion pattern was waiting for.
            let excluded = self
                .exclusions
                .is_excluded(&current.app, &change.title, current.url.as_deref());
            if let Some(id) = id {
                if excluded {
                    self.end_current_row(change.at)?;
                } else {
                    self.recorder
                        .refine_title(id, &current.app, &change.title, current.url.as_deref())?;
                }
            }
            self.current.as_mut().expect("checked above").window_title = change.title;
            return Ok(());
        }
        self.end_current_row(change.at)?;
        let current = self.current.as_mut().expect("checked above");
        current.url = Some(change.url);
        current.window_title = change.title;
        self.open_current_row(change.at)
    }

    fn handle_presence(&mut self, change: PresenceChange) -> Result<()> {
        if self.present == change.present {
            return Ok(());
        }
        self.present = change.present;
        if change.present {
            self.open_current_row(change.at)
        } else {
            self.end_current_row(change.at)
        }
    }

    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()> {
        self.end_current_row(at)?;
        self.current = None;
        Ok(())
    }
}
