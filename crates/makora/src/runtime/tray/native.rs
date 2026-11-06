//! The macOS menu-bar extra and the Windows notification-area icon, through `tray-icon`. On macOS
//! it lives on the main thread, whose AppKit loop the background process runs; on Windows on a
//! thread of its own that pumps its messages.

use anyhow::{Context, Result};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{QUIT_LABEL, Rgba, SHOW_LABEL, SystemTray, TOOLTIP};
use crate::runtime::commands::{CommandSender, UiCommand};

fn build(icon: Rgba, commands: CommandSender) -> Result<TrayIcon> {
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

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(TOOLTIP)
        .with_icon(
            Icon::from_rgba(icon.pixels, icon.width, icon.height)
                .context("loading the tray icon")?,
        )
        .with_icon_as_template(cfg!(target_os = "macos"))
        .with_menu_on_left_click(false)
        .build()
        .context("creating the tray icon")
}

#[cfg(target_os = "macos")]
struct NativeTray(TrayIcon);

#[cfg(target_os = "macos")]
impl SystemTray for NativeTray {
    fn remove(&self) {
        let _ = self.0.set_visible(false);
    }
}

#[cfg(target_os = "macos")]
pub fn spawn(icon: Rgba, commands: CommandSender) -> Result<Box<dyn SystemTray>> {
    Ok(Box::new(NativeTray(build(icon, commands)?)))
}

#[cfg(windows)]
pub use windows::spawn;

#[cfg(windows)]
mod windows {
    use std::sync::{Mutex, mpsc};
    use std::thread::JoinHandle;

    use anyhow::{Context, Result, anyhow};
    use makora_lib::shared::sync::lock;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PostThreadMessageW, TranslateMessage, WM_QUIT,
    };

    use super::{Rgba, SystemTray, build};
    use crate::runtime::commands::CommandSender;

    struct PumpedTray {
        thread_id: u32,
        thread: Mutex<Option<JoinHandle<()>>>,
    }

    impl SystemTray for PumpedTray {
        fn remove(&self) {
            // SAFETY: plain values; the thread has a message queue since it created the tray's window.
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
            if let Some(thread) = lock(&self.thread).take() {
                let _ = thread.join();
            }
        }
    }

    pub fn spawn(icon: Rgba, commands: CommandSender) -> Result<Box<dyn SystemTray>> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("makora-tray".into())
            .spawn(move || {
                let tray = match build(icon, commands) {
                    Ok(tray) => tray,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                // SAFETY: takes nothing.
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
                pump_messages();
                drop(tray);
            })
            .context("spawning the tray thread")?;
        let thread_id = ready_rx
            .recv()
            .map_err(|_| anyhow!("the tray thread stopped before the icon was made"))??;
        Ok(Box::new(PumpedTray {
            thread_id,
            thread: Mutex::new(Some(thread)),
        }))
    }

    /// Dispatches this thread's messages, the tray's clicks and menu among them, until `WM_QUIT`.
    fn pump_messages() {
        // SAFETY: MSG is plain data, and a zeroed one is valid to fill.
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: `message` outlives each call, and a null window takes every message of this thread.
        unsafe {
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}
