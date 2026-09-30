use std::sync::mpsc::Sender;

use anyhow::Result;

use super::model::WatchEvent;

/// Watches the operating system for window focus changes and reports them, implemented per OS in
/// [`super::platform`].
pub trait FocusWatcher {
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()>;
}
