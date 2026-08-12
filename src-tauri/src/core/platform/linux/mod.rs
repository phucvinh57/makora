//! Linux focus watching.
//!
//! Wayland has no equivalent of `GetForegroundWindow`: by design a client
//! cannot ask the compositor which window is focused, and the two portable
//! options both fall short on GNOME — `org.gnome.Shell.Introspect` is
//! allowlisted to xdg-desktop-portal-gnome and returns `Access denied` to
//! everyone else, and `ext-foreign-toplevel-list-v1`, which Mutter does
//! implement, deliberately carries no focus state (only `title`, `app_id`
//! and `identifier`). So focus has to come from something running *inside*
//! the compositor.
//!
//! That means there is no single Linux implementation, only one per desktop
//! stack, behind [`ToplevelProvider`]:
//!
//! - [`gnome`] — talks to the shell extension shipped in `gnome-extension/`
//!   over the session bus. The path exercised end to end.
//! - [`wlr`] — `wlr-foreign-toplevel-management-v1`, whose `state` event
//!   carries the `activated` flag Mutter's protocol lacks. Covers sway,
//!   Hyprland, river and other wlroots-based compositors.
//!
//! Everything downstream of a provider is shared, so which desktop the user
//! runs changes nothing about how sessions are recorded.

mod gnome;
mod proc;
mod provider;
mod wlr;

use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;

use crate::core::model::FocusEvent;
use crate::core::watcher::FocusWatcher;

pub use provider::{ToplevelProvider, WindowObservation};

/// Reports focus changes on Linux by delegating to whichever
/// [`ToplevelProvider`] matches the running desktop.
///
/// Event-driven like the Windows and macOS watchers — no polling. The
/// provider blocks on its own event source (a D-Bus message stream, a
/// Wayland event queue) and only wakes when focus actually moves.
#[derive(Default)]
pub struct LinuxFocusWatcher {
    thread: Option<thread::JoinHandle<()>>,
}

impl LinuxFocusWatcher {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FocusWatcher for LinuxFocusWatcher {
    fn start(&mut self, tx: Sender<FocusEvent>) -> Result<()> {
        let handle = thread::Builder::new()
            .name("makora-focus-watcher".into())
            .spawn(move || select_and_run(tx))
            .map_err(|e| anyhow!("spawning focus watcher thread: {e}"))?;
        self.thread = Some(handle);
        Ok(())
    }
}

/// Picks the first provider that recognizes the running session and hands
/// the thread over to it for the lifetime of the process.
///
/// Returning without finding one is not fatal: the app keeps running (the
/// UI, the browser-tab ingest and the stored history all still work), it
/// just records nothing new. That's the right failure mode for a desktop
/// nobody has written a provider for yet.
fn select_and_run(tx: Sender<FocusEvent>) {
    let mut emit = move |observation: WindowObservation| {
        // Makora's own window is not app usage. Windows gets this from
        // `WINEVENT_SKIPOWNPROCESS` and macOS checks the pid explicitly;
        // here the Tauri window lives in this very process, so comparing
        // pids is the same check. Dropping the event (rather than closing
        // the session) means a detour through Makora leaves the session
        // you came from running, exactly as on the other platforms.
        if observation.pid == Some(std::process::id()) {
            return;
        }

        let event = FocusEvent {
            app: proc::resolve_app_info(&observation),
            window_title: observation.title,
            timestamp: Utc::now(),
        };
        // A send error means the tracker thread is gone, i.e. the process is
        // shutting down — nothing useful to do about it here.
        let _ = tx.send(event);
    };

    if let Some(provider) = gnome::GnomeProvider::probe() {
        eprintln!(
            "makora: focus watcher using the {} provider",
            gnome::GnomeProvider::NAME
        );
        provider.run(&mut emit);
        return;
    }

    if let Some(provider) = wlr::WlrProvider::probe() {
        eprintln!(
            "makora: focus watcher using the {} provider",
            wlr::WlrProvider::NAME
        );
        provider.run(&mut emit);
        return;
    }

    eprintln!(
        "makora: no supported focus source on this desktop \
         (XDG_CURRENT_DESKTOP={:?}) — app usage will not be recorded. \
         On GNOME, install the shell extension from gnome-extension/.",
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
    );
}
