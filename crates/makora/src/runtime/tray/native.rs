//! The macOS menu-bar extra and the Windows notification-area icon, through `tray-icon`. Both must
//! be created on the main thread, whose event loop GPUI already runs.

use anyhow::{Context, Result};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{QUIT_LABEL, Rgba, SHOW_LABEL, SystemTray, TOOLTIP};
use crate::runtime::commands::{CommandSender, UiCommand};

struct NativeTray(TrayIcon);

impl SystemTray for NativeTray {
    fn remove(&self) {
        let _ = self.0.set_visible(false);
    }
}

pub fn spawn(icon: Rgba, commands: CommandSender) -> Result<Box<dyn SystemTray>> {
    let show = MenuItem::new(SHOW_LABEL, true, None);
    let quit = MenuItem::new(QUIT_LABEL, true, None);
    let menu = Menu::new();
    menu.append_items(&[&show, &quit])
        .context("building the tray menu")?;

    let (show_id, quit_id) = (show.id().clone(), quit.id().clone());
    let menu_commands = commands.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = if event.id == show_id {
            UiCommand::Show
        } else if event.id == quit_id {
            UiCommand::Quit
        } else {
            return;
        };
        let _ = menu_commands.try_send(command);
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = commands.try_send(UiCommand::Show);
        }
    }));

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(TOOLTIP)
        .with_icon(
            Icon::from_rgba(icon.pixels, icon.width, icon.height)
                .context("loading the tray icon")?,
        )
        .with_icon_as_template(cfg!(target_os = "macos"))
        .with_menu_on_left_click(false)
        .build()
        .context("creating the tray icon")?;
    Ok(Box::new(NativeTray(tray)))
}
