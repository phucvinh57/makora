//! The window's content: the shell, the screens, and what they share.

pub mod app_state;
pub mod data;
pub mod dialogs;
pub mod error;
pub mod format;
pub mod motion;
pub mod prefs;
pub mod route;
pub mod screens;
pub mod shell;
pub mod theme;
pub mod widgets;

use std::sync::Arc;

use async_channel::Receiver;
use gpui_kit::App;
use makora_lib::app_core::ClientCore;
use makora_lib::events::CoreEvent;

use crate::ui::app_state::Core;

pub fn init(core: Arc<ClientCore>, events: Receiver<CoreEvent>, cx: &mut App) {
    prefs::init(&core.paths.config_dir, cx);
    cx.set_global(Core(core));
    theme::init(cx);
    app_state::init(events, cx);
}
