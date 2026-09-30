//! The Tauri shell: the IPC commands the frontend invokes and the system tray. Everything here
//! is glue — parsing arguments, calling a service, turning its result into something
//! serializable — and holds no rules of its own.

pub mod commands;
pub mod error;
pub mod tray;
