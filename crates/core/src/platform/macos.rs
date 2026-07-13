use std::sync::mpsc::Sender;

use anyhow::{bail, Result};

use crate::model::FocusEvent;
use crate::watcher::FocusWatcher;

/// Placeholder for the macOS implementation of `FocusWatcher`.
///
/// Expected approach: subscribe to
/// `NSWorkspace.shared.notificationCenter` for
/// `didActivateApplicationNotification` (app-level focus changes) via
/// `objc2`/`objc2-app-kit`, running its own `NSRunLoop` on a dedicated
/// thread — the same "own thread, stream over `tx`" shape as
/// `WindowsFocusWatcher`, so `SessionTracker` and everything above this
/// trait needs no changes to support macOS.
#[derive(Default)]
pub struct MacFocusWatcher;

impl MacFocusWatcher {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FocusWatcher for MacFocusWatcher {
    fn start(&mut self, _tx: Sender<FocusEvent>) -> Result<()> {
        bail!("macOS focus watcher is not implemented yet")
    }
}
