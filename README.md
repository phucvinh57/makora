# makora

A desktop app that tracks how long you spend in each application, as a
series of focus sessions: a session starts the moment an app gains focus and
ends the moment a different app takes focus. Sessions are stored locally in
SQLite — nothing leaves the machine.

It's a [Tauri](https://tauri.app) app: the focus watcher and the browser
tab-ingest server run in-process alongside the window, and closing or
minimizing the window hides it to the system tray instead of quitting —
tracking keeps running in the background, and the tray icon brings the
window back.

A companion browser [extension](browser-extension) can optionally report the active
tab's URL, so browser time is broken down per site instead of showing up as
one opaque `chrome`/`firefox` block.

Runs on Windows, macOS and Linux. On **GNOME** the app cannot see window
focus by itself — Wayland forbids it — so a small [GNOME Shell
extension](gnome-extension) is required there; see
[gnome-extension/README.md](gnome-extension/README.md).

On Linux, an explicit screen lock or system suspend ends the current
activity row. Unlocking or waking starts a fresh row for the app/site that
is still focused, so time spent locked or asleep is not attributed to it.
Ordinary keyboard/mouse inactivity does not stop tracking: an unlocked
video remains valid activity even when the user is not providing input.
Lock detection uses systemd-logind's `LockedHint`; custom wlroots lock-screen
setups must keep that standard property updated.

On macOS, grant Makora access in **System Settings → Privacy & Security →
Accessibility** so focused window titles can be read. Tracking continues without
the permission, but titles remain empty. Unsigned development rebuilds may need
the permission granted again because the app signature changes.

## Prerequisites

- [Bun](https://bun.sh)
- A [Rust](https://www.rust-lang.org/tools/install) toolchain (stable)
- On Windows: MSVC C++ build tools (Visual Studio Build Tools, "Desktop
  development with C++") — `rusqlite` builds SQLite from source via its
  `bundled` feature
- On Linux: a C compiler (for the same reason) plus Tauri's WebKitGTK stack.
  On Debian/Ubuntu:
  ```sh
  sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev \
      librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf
  ```
- On macOS: Accessibility permission for Makora, as described above
- See the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)
  for any platform-specific system dependencies

## Building and running

```sh
bun install       # install JS dependencies
bun tauri dev     # run the full app (Vite dev server + Rust backend)
```

Other useful commands:

```sh
bun dev           # frontend only (Vite), no Tauri/Rust backend
bun run build     # tsc typecheck + vite build (frontend only)
bun tauri build   # build a release bundle/installer
```

From `src-tauri/`, the usual Cargo commands also work directly:

```sh
cargo build
cargo test        # Rust unit tests
cargo clippy
```

Sessions are stored in a SQLite database named `makora.sqlite`, in the
per-user app-data directory Tauri names after the bundle identifier
(on Windows: `%APPDATA%\dev.phucvinh57.makora\`; on macOS:
`~/Library/Application Support/dev.phucvinh57.makora/`; on Linux:
`~/.local/share/dev.phucvinh57.makora/`).

## Browser extension

The [extension](browser-extension) is required for web URL tracking and loaded separately as an unpacked
WebExtension — see [browser-extension/README.md](browser-extension/README.md) for what it
does and how it talks to the app.

## GNOME Shell extension (Linux)

On Wayland a client is not allowed to ask which window has focus, and GNOME
offers no way around it — `org.gnome.Shell.Introspect` is allowlisted to the
desktop portal, and the `ext-foreign-toplevel-list-v1` protocol Mutter
implements carries no focus state. So on GNOME the
[gnome-extension](gnome-extension) is **required**, not optional: without it
the app runs but records nothing.

```sh
./gnome-extension/install.sh
# log out and back in, then:
gnome-extensions enable makora@phucvinh57.dev
```

The `.deb` package ships the extension already, so only the
`enable` step is needed there. Installing it later is fine — the app notices
and starts tracking without a restart.

On wlroots-based compositors (sway, Hyprland, river) no extension is needed:
those advertise `wlr-foreign-toplevel-management-v1`, which reports focus
directly, and the app uses it automatically.

## Categorization

Categorization is manual. Makora records sessions grouped into activities (the
domain for browser activity, or the executable name for other apps, plus the
title); nothing is categorized until you pin a category to one. Uncategorized
time shows up as **Uncategorized**.

- A pin is stored as `category_id` on the activity, so it applies to every
  occurrence of that activity, past and future.
- Deleting a category deletes its subcategories and clears the pins that pointed
  at them.
- **Reset categorization data** clears every pin but keeps the category taxonomy
  and tracking history.

## Known limitations

- Lock/sleep presence handling is Linux-only; Windows and macOS do not yet have presence watchers.
- Windows UWP/Store apps currently appear as `ApplicationFrameHost.exe` instead of their underlying app identity.
- macOS focus delivery and window-title access still need end-to-end testing on macOS hardware.

## Note
To build llama-cpp-2 with vulkan on Windows, please set `$env:CARGO_TARGET_DIR = "C:\mk"` before every build.