//! The focus layer's public interface, gathered here so both traits can be read in one place.
//! `FocusWatcher` is implemented per OS in `platform/`; `SessionTracker` is implemented by
//! `ActivitySessionTracker` in `tracker.rs`. Each implementing module re-exports its trait, so
//! callers keep writing `crate::focus::FocusWatcher` / `crate::focus::SessionTracker`.

use std::sync::mpsc::Sender;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::model::{FocusEvent, PresenceEvent, TabEvent, WatchEvent};

pub trait FocusWatcher {
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()>;
}

pub trait SessionTracker: Send {
    fn handle_event(&mut self, event: FocusEvent) -> Result<()>;
    fn handle_tab_event(&mut self, event: TabEvent) -> Result<()>;
    fn handle_presence_event(&mut self, event: PresenceEvent) -> Result<()>;
    fn close_current(&mut self, at: DateTime<Utc>) -> Result<()>;
}
