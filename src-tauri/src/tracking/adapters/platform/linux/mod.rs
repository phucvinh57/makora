mod gnome;
mod presence;
mod proc;
mod provider;
mod wlr;

use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{anyhow, Result};
use chrono::Utc;
use tracing::{error, info};

use crate::tracking::adapters::model::{FocusEvent, WatchEvent};
use crate::tracking::adapters::watcher::FocusWatcher;

pub use presence::start_presence_watcher;
pub use provider::{ToplevelProvider, WindowObservation};

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
    fn start(&mut self, tx: Sender<WatchEvent>) -> Result<()> {
        let handle = thread::Builder::new()
            .name("makora-focus-watcher".into())
            .spawn(move || select_and_run(tx))
            .map_err(|e| anyhow!("spawning focus watcher thread: {e}"))?;
        self.thread = Some(handle);
        Ok(())
    }
}

fn select_and_run(tx: Sender<WatchEvent>) {
    let mut emit = move |observation: WindowObservation| {
        if observation.pid == Some(std::process::id()) {
            return;
        }

        let event = FocusEvent {
            app: proc::resolve_app_info(&observation),
            window_title: observation.title,
            timestamp: Utc::now(),
        };
        let _ = tx.send(event.into());
    };

    if let Some(provider) = gnome::GnomeProvider::probe() {
        info!(
            provider = gnome::GnomeProvider::NAME,
            "focus watcher started"
        );
        provider.run(&mut emit);
        return;
    }

    if let Some(provider) = wlr::WlrProvider::probe() {
        info!(provider = wlr::WlrProvider::NAME, "focus watcher started");
        provider.run(&mut emit);
        return;
    }

    error!(
        desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        "no supported focus source on this desktop — app usage will not be recorded. \
         On GNOME, install the shell extension from gnome-extension/."
    );
}
