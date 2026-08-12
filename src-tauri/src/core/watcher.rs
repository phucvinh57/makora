use std::sync::mpsc::Sender;

use anyhow::Result;

use crate::core::model::WatchEvent;

/// Platform-specific source of focus-change events.
///
/// Each OS (Windows, macOS, ...) implements this by listening to the
/// relevant native API and reporting every time a different window becomes
/// foreground. Implementations own their event-loop thread: `start` spawns
/// it and returns immediately, streaming events over `tx` for the lifetime
/// of the process.
///
/// Session semantics (deciding when one focus event ends a session and
/// starts another) are deliberately *not* this trait's job — that lives in
/// `SessionTracker`, so every platform gets identical session behavior for
/// free.
pub trait FocusWatcher {
    /// Start watching for focus changes, sending each one to `tx`.
    ///
    /// Must not block: the platform event loop runs on its own thread.
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()>;
}
