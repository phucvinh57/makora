//! The GPUI application in the window process: the window, its menus and keys. Closing the
//! window ends the process; Quit asks the background process to quit the app.

use std::sync::Arc;

use async_channel::Receiver;
use gpui_kit::assets::AllAssets;
use gpui_kit::{App, KeyBinding, Menu, MenuItem, QuitMode};
use makora_lib::app_core::ClientCore;
use makora_lib::events::{CoreEvent, NavTarget};
use makora_lib::ipc::window::WindowClient;
use tracing::warn;

use crate::runtime::autostart::{self, AutostartService};
use crate::runtime::commands::{self, CommandReceiver};
use crate::runtime::window;
use crate::ui;
use crate::ui::prefs::Prefs;

gpui_kit::actions!(makora, [Quit]);

#[cfg(target_os = "macos")]
const QUIT_KEY: &str = "cmd-q";
#[cfg(not(target_os = "macos"))]
const QUIT_KEY: &str = "ctrl-q";

/// Quit from the keyboard and, on macOS, the app menu, besides the tray.
fn install_quit(client: Arc<WindowClient>, cx: &mut App) {
    cx.on_action(move |_: &Quit, cx| {
        client.quit_app();
        cx.quit();
    });
    cx.bind_keys([KeyBinding::new(QUIT_KEY, Quit, None)]);
    cx.set_menus(vec![
        Menu::new("Makora").items([MenuItem::action("Quit Makora", Quit)]),
    ]);
}

pub fn launch(
    core: Arc<ClientCore>,
    client: Arc<WindowClient>,
    command_rx: CommandReceiver,
    events: Receiver<CoreEvent>,
    open_at: Option<NavTarget>,
) {
    let app = gpui_kit::application()
        .with_assets(AllAssets)
        .with_quit_mode(QuitMode::LastWindowClosed);
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
        install_quit(client, cx);
        commands::start_pump(command_rx, cx);

        cx.on_app_quit(|cx| {
            Prefs::flush(cx);
            async {}
        })
        .detach();

        window::show_main_window(open_at.map(commands::route), cx);
    });
}
