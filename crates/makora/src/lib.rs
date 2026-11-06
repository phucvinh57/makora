//! Makora's native front end: the GPUI window, tray and single-instance handling around the
//! services in [`makora_lib::app_core::AppCore`].
//!
//! The app is a background process ([`runtime::background`]) that starts a window process
//! (`makora --window`) when the window is asked for. Each AI run is a worker process
//! (`makora --ai-worker`).

pub mod runtime;
pub mod ui;

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;

use makora_lib::app_core::AppCore;
use makora_lib::ipc::worker;
use makora_lib::logging;
use makora_lib::paths::AppPaths;
use tracing::{error, warn};

use crate::runtime::commands;
use crate::runtime::events::ChannelEvents;
use crate::runtime::single_instance::{self, Claim};

pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if worker::parse_args(&args) {
        return run_child(worker::serve);
    }
    if let Some(open_at) = makora_lib::ipc::window::parse_args(&args) {
        return run_child(|paths| window_process::run(paths, open_at));
    }

    let (command_tx, command_rx) = commands::channel();
    let instance_issue = match single_instance::claim(command_tx.clone()) {
        Claim::Primary => None,
        Claim::Secondary => return ExitCode::SUCCESS,
        Claim::Unavailable(issue) => Some(issue),
    };

    let Some(paths) = start_logging() else {
        return ExitCode::FAILURE;
    };
    if let Some(issue) = instance_issue {
        warn!(error = %issue, "single-instance check unavailable; a second launch will open another app");
    }

    let (events, event_rx) = ChannelEvents::new();
    let core = match AppCore::open(paths, Arc::new(events)) {
        Ok(core) => core,
        Err(err) => {
            error!(error = format!("{err:#}"), "failed to open Makora");
            logging::flush();
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = core.start_background() {
        error!(
            error = format!("{err:#}"),
            "failed to start background work"
        );
        core.shutdown();
        return ExitCode::FAILURE;
    }

    runtime::background::run(core, command_tx, command_rx, event_rx);
    ExitCode::SUCCESS
}

fn start_logging() -> Option<AppPaths> {
    let paths = match AppPaths::resolve() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("makora: cannot resolve the app directories: {err:#}");
            return None;
        }
    };
    if let Err(err) = logging::init(&paths.data_dir) {
        eprintln!("makora: failed to start logging: {err:#}");
    }
    Some(paths)
}

/// A process the app started, which talks to it over stdin and stdout. It is not an instance of
/// the app, so it skips the single-instance check; started by hand, from a terminal, it refuses.
fn run_child(body: impl FnOnce(AppPaths) -> anyhow::Result<()>) -> ExitCode {
    if std::io::stdin().is_terminal() {
        eprintln!("makora: this mode is started by the app itself");
        return ExitCode::FAILURE;
    }
    let Some(paths) = start_logging() else {
        return ExitCode::FAILURE;
    };
    let result = body(paths);
    if let Err(err) = &result {
        error!(error = format!("{err:#}"), "the child process failed");
    }
    logging::flush();
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

mod window_process {
    use std::io::{self, BufReader, BufWriter};
    use std::sync::Arc;

    use anyhow::{Context, Result};
    use makora_lib::app_core::{ClientCore, Services};
    use makora_lib::events::{EventSink, NavTarget};
    use makora_lib::ipc::window::{RemoteDownloads, RemoteRunner, ToWindow, WindowClient};
    use makora_lib::paths::AppPaths;

    use crate::runtime::app;
    use crate::runtime::commands::{self, UiCommand};
    use crate::runtime::events::ChannelEvents;

    /// The window, in a process of its own: the background process that started it holds the
    /// core, answers its calls on stdin and stdout, and sends it the core's events.
    pub fn run(paths: AppPaths, open_at: Option<NavTarget>) -> Result<()> {
        let (events, event_rx) = ChannelEvents::new();
        let events = Arc::new(events);
        let services = Services::open(paths, events.clone())?;
        let client = WindowClient::new(BufWriter::new(io::stdout()));
        let saved = client.clone();
        services.settings_store.on_save(Box::new(move || saved.settings_saved()));
        let core = ClientCore::new(
            &services,
            Arc::new(RemoteRunner(client.clone())),
            Arc::new(RemoteDownloads(client.clone())),
        );

        let (command_tx, command_rx) = commands::channel();
        let reader = client.clone();
        std::thread::Builder::new()
            .name("makora-app-link".into())
            .spawn(move || {
                reader.serve(BufReader::new(io::stdin()), |message| match message {
                    ToWindow::Show(target) => {
                        let _ = command_tx.try_send(target.map_or(UiCommand::Show, UiCommand::ShowAt));
                    }
                    ToWindow::Quit => {
                        let _ = command_tx.try_send(UiCommand::Quit);
                    }
                    ToWindow::Event(event) => events.emit(event),
                    ToWindow::Reply { .. } => {}
                });
                let _ = command_tx.try_send(UiCommand::Quit);
            })
            .context("spawning the app link thread")?;

        app::launch(Arc::new(core), client, command_rx, event_rx, open_at);
        Ok(())
    }
}
