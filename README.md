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

## Prerequisites

- [Bun](https://bun.sh)
- A [Rust](https://www.rust-lang.org/tools/install) toolchain (stable)
- On Windows: MSVC C++ build tools (Visual Studio Build Tools, "Desktop
  development with C++") — `rusqlite` builds SQLite from source via its
  `bundled` feature
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
(on Windows: `%APPDATA%\phucvinh57\makora\data\data.db`).

## Browser extension

The [extension](extension) is required for web URL tracking and loaded separately as an unpacked
WebExtension — see [extension/README.md](extension/README.md) for what it
does and how it talks to the app.
