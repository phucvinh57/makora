# 05-presence-windows-macos

Files: `tracking/adapters/platform/windows.rs`, `macos.rs`, `linux/presence.rs`, `lib.rs:161-163`.
1. Move `PresenceState` (locked/sleeping → present, publish on change) from `linux/presence.rs` to a shared `tracking/adapters/presence.rs`; Linux keeps using it.
2. **Windows**: in the watcher thread create a message-only window (`HWND_MESSAGE`), `WTSRegisterSessionNotification` (`WM_WTSSESSION_CHANGE`: LOCK/UNLOCK) and handle `WM_POWERBROADCAST` (`PBT_APMSUSPEND` / `PBT_APMRESUMEAUTOMATIC`); message loop gains `TranslateMessage/DispatchMessageW`. Features: `Win32_System_RemoteDesktop`, `Win32_System_Power`.
3. **macOS**: NSWorkspace notification center: `WillSleep`/`DidWake`, `ScreensDidSleep`/`ScreensDidWake`; `NSDistributedNotificationCenter`: `com.apple.screenIsLocked`/`Unlocked`. Keep observer tokens + blocks alive for the thread lifetime (current activation observer is dropped at `macos.rs:90` — fix too). Replace `lock().unwrap()` there (see 06).
4. `start_presence_watcher` becomes cross-platform in `lib.rs`.
Verify: `cargo check` for `x86_64-pc-windows-msvc` / `aarch64-apple-darwin` in CI; manual test needs a Windows/macOS machine (call out in PR).
