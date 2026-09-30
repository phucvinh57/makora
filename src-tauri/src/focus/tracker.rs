use std::collections::HashMap;

use chrono::{DateTime, Utc};

use anyhow::Result;

use crate::domain::TrackingSvc;
use crate::shared::ids::SessionId;
use crate::tracking::domain::source_key::exe_file_name;

use super::model::{AppInfo, BrowserKind, FocusEvent, PresenceEvent, TabEvent};

const IGNORED_APPS: &[&str] = &["explorer.exe", "gnome-shell"];

fn is_ignored(exe_path: &str) -> bool {
    let file_name = exe_file_name(exe_path);
    IGNORED_APPS
        .iter()
        .any(|ignored| ignored.eq_ignore_ascii_case(file_name))
}

struct Current {
    id: Option<SessionId>,
    app: AppInfo,
    browser: Option<BrowserKind>,
    url: Option<String>,
    window_title: String,
}

struct LastTab {
    url: String,
    title: String,
}

pub use super::traits::SessionTracker;

pub struct ActivitySessionTracker {
    tracking: Box<dyn TrackingSvc>,
    current: Option<Current>,
    /// Latest tab the extension reported per browser. The extension only reports on
    /// change, so this is what a browser regaining focus is still showing.
    last_tabs: HashMap<BrowserKind, LastTab>,
    present: bool,
}

impl ActivitySessionTracker {
    pub fn new(tracking: Box<dyn TrackingSvc>) -> Result<Self> {
        tracking.close_dangling()?;
        Ok(Self {
            tracking,
            current: None,
            last_tabs: HashMap::new(),
            present: true,
        })
    }

    fn end_current_row(&mut self, at: DateTime<Utc>) -> Result<()> {
        let id = self.current.as_ref().and_then(|current| current.id);
        if let Some(id) = id {
            self.tracking.end(id, at)?;
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
        let id = self.tracking.start(&app, &title, url.as_deref(), at)?;
        self.current.as_mut().expect("checked above").id = Some(id);
        Ok(())
    }
}

impl SessionTracker for ActivitySessionTracker {
    fn handle_event(&mut self, event: FocusEvent) -> Result<()> {
        if is_ignored(&event.app.exe_path) {
            return Ok(());
        }
        if let Some(current) = &self.current {
            if current.app.exe_path == event.app.exe_path {
                if !self.present {
                    let current = self.current.as_mut().expect("checked above");
                    current.app = event.app;
                    current.window_title = event.window_title;
                }
                return Ok(());
            }
        }
        self.end_current_row(event.timestamp)?;
        let browser = BrowserKind::from_app_name(&event.app.app_name);
        let tab = browser.and_then(|kind| self.last_tabs.get(&kind));
        let (url, window_title) = match tab {
            Some(tab) => (Some(tab.url.clone()), tab.title.clone()),
            None => (None, event.window_title),
        };
        self.current = Some(Current {
            id: None,
            app: event.app,
            browser,
            url,
            window_title,
        });
        self.open_current_row(event.timestamp)
    }

    fn handle_tab_event(&mut self, event: TabEvent) -> Result<()> {
        self.last_tabs.insert(
            event.browser,
            LastTab {
                url: event.url.clone(),
                title: event.title.clone(),
            },
        );
        let Some(current) = &self.current else {
            return Ok(());
        };
        if current.browser != Some(event.browser) {
            return Ok(());
        }
        if current.url.as_deref() == Some(event.url.as_str()) {
            let id = current.id;
            if let Some(id) = id {
                self.tracking
                    .refine_title(id, &current.app, &event.title, current.url.as_deref())?;
            }
            self.current.as_mut().expect("checked above").window_title = event.title;
            return Ok(());
        }
        self.end_current_row(event.timestamp)?;
        let current = self.current.as_mut().expect("checked above");
        current.url = Some(event.url);
        current.window_title = event.title;
        self.open_current_row(event.timestamp)
    }

    fn handle_presence_event(&mut self, event: PresenceEvent) -> Result<()> {
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

    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()> {
        self.end_current_row(at)?;
        self.current = None;
        Ok(())
    }
}
