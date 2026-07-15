//! Platform-specific `FocusWatcher` implementations.
//!
//! Adding a new OS means writing one file here that implements
//! `FocusWatcher` and re-exporting it as `PlatformFocusWatcher` below —
//! nothing in `tracker.rs` or `store.rs` needs to change.

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsFocusWatcher as PlatformFocusWatcher;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacFocusWatcher as PlatformFocusWatcher;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
compile_error!("makora supports Windows and macOS only");
