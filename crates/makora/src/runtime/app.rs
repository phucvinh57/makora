//! The GPUI application: the window, its menus and keys, and what quitting means for the process
//! it runs in.

use std::sync::Arc;

use async_channel::Receiver;
use gpui_kit::assets::AllAssets;
use gpui_kit::{App, KeyBinding, Menu, MenuItem, QuitMode};
use makora_lib::app_core::{AppCore, ClientCore};
use makora_lib::events::{CoreEvent, NavTarget};
use makora_lib::ipc::window::WindowClient;
use tracing::warn;

use crate::runtime::autostart::{self, AutostartService};
use crate::runtime::commands::{self, CommandReceiver, CommandSender};
use crate::runtime::{tray, window};
use crate::ui;
use crate::ui::prefs::Prefs;

gpui_kit::actions!(makora, [Quit]);

#[cfg(target_os = "macos")]
const QUIT_KEY: &str = "cmd-q";
#[cfg(not(target_os = "macos"))]
const QUIT_KEY: &str = "ctrl-q";

/// Where the core lives, which decides what closing the window and quitting do.
#[derive(Clone)]
pub enum Host {
    /// The core and the tray are in this process, which stays alive with no window open, until
    /// Quit.
    InProcess(Arc<AppCore>, CommandSender),
    /// A window process started by the background process: closing the window ends it, and
    /// Quit asks the background process to quit the app.
    Child(Arc<WindowClient>),
}

/// Quit from the keyboard and, on macOS, the app menu, besides the tray.
fn install_quit(host: &Host, cx: &mut App) {
    let host = host.clone();
    cx.on_action(move |_: &Quit, cx| {
        if let Host::Child(client) = &host {
            client.quit_app();
        }
        cx.quit();
    });
    cx.bind_keys([KeyBinding::new(QUIT_KEY, Quit, None)]);
    cx.set_menus(vec![
        Menu::new("Makora").items([MenuItem::action("Quit Makora", Quit)]),
    ]);
}

pub fn launch(
    core: Arc<ClientCore>,
    host: Host,
    command_rx: CommandReceiver,
    events: Receiver<CoreEvent>,
    open_at: Option<NavTarget>,
) {
    let quit_mode = match host {
        Host::InProcess(..) => QuitMode::Explicit,
        Host::Child(_) => QuitMode::LastWindowClosed,
    };
    let app = gpui_kit::application()
        .with_assets(AllAssets)
        .with_quit_mode(quit_mode);
    app.on_reopen(|cx| window::show_main_window(None, cx));
    app.run(move |cx| {
        ui::theme::register_ui_font(cx);
        gpui_kit::init(cx);
        let autostart = autostart::for_build().unwrap_or_else(|err| {
            warn!(error = format!("{err:#}"), "autostart is unavailable");
            None
        });
        cx.set_global(AutostartService(autostart));
        ui::init(core, events, cx);
        window::init(cx);
        install_quit(&host, cx);
        if let Host::InProcess(_, commands) = &host {
            tray::install(commands.clone(), cx);
        }
        commands::start_pump(command_rx, cx);

        cx.on_app_quit(move |cx| {
            Prefs::flush(cx);
            if let Host::InProcess(core, _) = &host {
                tray::remove(cx);
                core.shutdown();
            }
            async {}
        })
        .detach();

        window::show_main_window(open_at.map(commands::route), cx);
    });
}
