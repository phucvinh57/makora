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

A companion browser [extension](extension) can optionally report the active
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

Sessions are stored in a SQLite database in the per-user app-data directory
(on Windows: `%APPDATA%\phucvinh57\makora\data\data.db`; on Linux:
`~/.local/share/makora/`).

## Browser extension

The [extension](extension) is required for web URL tracking and loaded separately as an unpacked
WebExtension — see [extension/README.md](extension/README.md) for what it
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

The `.deb` and `.rpm` packages ship the extension already, so only the
`enable` step is needed there. Installing it later is fine — the app notices
and starts tracking without a restart.

On wlroots-based compositors (sway, Hyprland, river) no extension is needed:
those advertise `wlr-foreign-toplevel-management-v1`, which reports focus
directly, and the app uses it automatically.
