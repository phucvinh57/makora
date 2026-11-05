//! Makora's native front end: the GPUI window, tray and single-instance handling around the
//! services in [`makora_lib::app_core::AppCore`].

pub mod runtime;
pub mod ui;

use std::process::ExitCode;
use std::sync::Arc;

use makora_lib::app_core::AppCore;
use makora_lib::logging;
use makora_lib::paths::AppPaths;
use tracing::{error, warn};

use crate::runtime::commands;
use crate::runtime::events::ChannelEvents;
use crate::runtime::single_instance::{self, Claim};

pub fn run() -> ExitCode {
    let (command_tx, command_rx) = commands::channel();
    let instance_issue = match single_instance::claim(command_tx.clone()) {
        Claim::Primary => None,
        Claim::Secondary => return ExitCode::SUCCESS,
        Claim::Unavailable(issue) => Some(issue),
    };

    let paths = match AppPaths::resolve() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("makora: cannot resolve the app directories: {err:#}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = logging::init(&paths.data_dir) {
        eprintln!("makora: failed to start logging: {err:#}");
    }
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

    runtime::app::launch(core, command_tx, command_rx, event_rx);
    ExitCode::SUCCESS
}
