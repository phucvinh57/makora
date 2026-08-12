//! Platform-specific `FocusWatcher` implementations.
//!
//! Adding a new OS means writing one file here that implements
//! `FocusWatcher` and re-exporting it as `PlatformFocusWatcher` below —
//! nothing in `tracker.rs` or `store.rs` needs to change.
//!
//! Linux is a directory rather than a file: "Linux" is not one focus API
//! but several, none of them usable from an ordinary Wayland client, so
//! `linux/` picks between per-desktop providers at startup. See its module
//! docs for why.

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsFocusWatcher as PlatformFocusWatcher;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacFocusWatcher as PlatformFocusWatcher;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::start_presence_watcher;
#[cfg(target_os = "linux")]
pub use linux::LinuxFocusWatcher as PlatformFocusWatcher;

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
compile_error!("makora supports Windows, macOS and Linux only");
