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
