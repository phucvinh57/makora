//! A StatusNotifierItem over D-Bus, so the tray needs no GTK. GNOME shows it through the
//! AppIndicator extension.

use anyhow::{Context, Result};
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, OfflineReason, Tray};
use makora_lib::paths::APP_ID;
use tracing::warn;

use super::{QUIT_LABEL, Rgba, SHOW_LABEL, SystemTray, TOOLTIP};
use crate::runtime::commands::{CommandSender, UiCommand};

struct MakoraTray {
    icon: Icon,
    commands: CommandSender,
}

impl MakoraTray {
    fn send(&self, command: UiCommand) {
        let _ = self.commands.try_send(command);
    }
}

impl Tray for MakoraTray {
    fn id(&self) -> String {
        APP_ID.into()
    }

    fn title(&self) -> String {
        TOOLTIP.into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![self.icon.clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: TOOLTIP.into(),
            ..Default::default()
        }
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        warn!(
            ?reason,
            "no tray host is running; the icon appears once one starts"
        );
        true
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(UiCommand::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: SHOW_LABEL.into(),
                activate: Box::new(|tray: &mut Self| tray.send(UiCommand::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: QUIT_LABEL.into(),
                activate: Box::new(|tray: &mut Self| tray.send(UiCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

struct KsniTray(Handle<MakoraTray>);

impl SystemTray for KsniTray {
    fn remove(&self) {
        self.0.shutdown().wait();
    }
}

pub fn spawn(icon: Rgba, commands: CommandSender) -> Result<Box<dyn SystemTray>> {
    let tray = MakoraTray {
        icon: Icon {
            width: icon.width as i32,
            height: icon.height as i32,
            data: icon.to_argb(),
        },
        commands,
    };
    let handle = tray
        .assume_sni_available(true)
        .spawn()
        .context("registering the tray icon over D-Bus")?;
    Ok(Box::new(KsniTray(handle)))
}
