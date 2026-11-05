//! The tray icon: Show and Quit in its menu, and a left click shows the window.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod native;

use anyhow::{Context, Result};
use gpui_kit::{App, Global};
use tracing::warn;

use crate::runtime::commands::CommandSender;

#[cfg(target_os = "macos")]
const TRAY_PNG: &[u8] = include_bytes!("../../../../../src-tauri/icons/tray.png");
#[cfg(not(target_os = "macos"))]
const TRAY_PNG: &[u8] = include_bytes!("../../../../../src-tauri/icons/32x32.png");

pub const TOOLTIP: &str = "Makora";
pub const SHOW_LABEL: &str = "Show";
pub const QUIT_LABEL: &str = "Quit";

pub trait SystemTray {
    fn remove(&self);
}

struct InstalledTray(Box<dyn SystemTray>);

impl Global for InstalledTray {}

/// Puts the icon in the tray for the life of the app. Call it from the app's launch callback, on
/// the main thread, which macOS and Windows require.
pub fn install(commands: CommandSender, cx: &mut App) {
    let tray = Rgba::decode(TRAY_PNG).and_then(|icon| spawn(icon, commands));
    match tray {
        Ok(tray) => cx.set_global(InstalledTray(tray)),
        Err(err) => warn!(
            error = format!("{err:#}"),
            "no tray icon; closing the window will hide the app"
        ),
    }
}

pub fn remove(cx: &mut App) {
    if let Some(tray) = cx.try_global::<InstalledTray>() {
        tray.0.remove();
    }
}

#[cfg(target_os = "linux")]
fn spawn(icon: Rgba, commands: CommandSender) -> Result<Box<dyn SystemTray>> {
    linux::spawn(icon, commands)
}

#[cfg(not(target_os = "linux"))]
fn spawn(icon: Rgba, commands: CommandSender) -> Result<Box<dyn SystemTray>> {
    native::spawn(icon, commands)
}

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn decode(png: &[u8]) -> Result<Self> {
        let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .context("decoding the tray icon")?
            .into_rgba8();
        Ok(Self {
            width: image.width(),
            height: image.height(),
            pixels: image.into_raw(),
        })
    }

    /// The pixels as ARGB32 in network byte order, the StatusNotifierItem pixmap format.
    pub fn to_argb(&self) -> Vec<u8> {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [a, r, g, b])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_icon_is_a_real_png() {
        let icon = Rgba::decode(TRAY_PNG).expect("run `git lfs pull` if this is an LFS pointer");
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.pixels.len(), 32 * 32 * 4);
    }

    #[test]
    fn argb_moves_alpha_first() {
        let icon = Rgba {
            width: 1,
            height: 1,
            pixels: vec![1, 2, 3, 4],
        };
        assert_eq!(icon.to_argb(), vec![4, 1, 2, 3]);
    }
}
