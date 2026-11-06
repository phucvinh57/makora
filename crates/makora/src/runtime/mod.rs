//! Operating-system integration: the GPUI application, the tray, single-instance handling and
//! autostart. Each OS-facing piece sits behind a small trait so the UI never touches the platform.

pub mod app;
pub mod autostart;
pub mod background;
pub mod commands;
pub mod events;
mod main_loop;
pub mod single_instance;
pub mod tray;
pub mod window;
