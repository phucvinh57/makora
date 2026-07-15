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

## Layout

```
src/                 React + TypeScript UI (history page)
src-tauri/           Tauri backend (makora-app, a single Rust crate — no workspace)
  src/core/          platform-agnostic session logic: write path
                       (watcher/tracker/store) plus a read-only query API
  src/commands.rs    Tauri commands the frontend calls (list_sessions, ...)
  src/ingest.rs      local HTTP server the browser extension posts tab events to
  src/tray.rs        tray icon (Show/Quit)
  src/lib.rs         setup hook: wires watcher + ingest server + tray together
extension/           cross-browser WebExtension that reports the active tab's URL
```

### Why `core` is its own module

`src-tauri/src/core` defines three things and nothing platform-specific
depends on the others:

- [`FocusWatcher`](src-tauri/src/core/watcher.rs) — trait a platform
  implements to report focus changes. It owns its own event-loop thread and
  streams `FocusEvent`s over a channel; it has no opinion on sessions.
- [`SessionStore`](src-tauri/src/core/store/mod.rs) — trait a storage
  backend implements to open/close sessions.
  [`SqliteStore`](src-tauri/src/core/store/sqlite.rs) is the only
  implementation today. [`HistoryReader`](src-tauri/src/core/store/queries.rs)
  is a separate, read-only connection to the same database that the UI reads
  through — it never contends with the write path's lock.
- [`SessionTracker`](src-tauri/src/core/tracker.rs) — the only place
  session semantics live (same exe = same session; different exe = close the
  old session, open a new one). Every `FocusWatcher` impl and every
  `SessionStore` impl gets identical behavior for free because this piece
  doesn't know or care which platform or backend it's talking to.

`lib.rs` wires those pieces to a window: its `setup` hook starts the focus
watcher, spawns the tab-ingest server, and builds the tray icon, all on their
own threads inside the single Tauri process. It exposes `list_sessions` as a
Tauri command for the frontend to read through.

macOS is implemented via `NSWorkspace`'s activation notifications plus the
Accessibility API for window titles
([`src-tauri/src/core/platform/macos.rs`](src-tauri/src/core/platform/macos.rs)),
but is not yet exercised in the Tauri app end-to-end the way Windows is.

Windows is implemented via `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`
([src-tauri/src/core/platform/windows.rs](src-tauri/src/core/platform/windows.rs)):
event-driven rather than polled, so session boundaries line up with the
exact moment the user switched windows and no CPU is spent while focus is
unchanged.

## Per-site browser tracking

An app session for a browser (Chrome, Edge, Firefox, Brave) is a single opaque
block by default — `SessionTracker` only knows the OS says "chrome.exe is
focused", not which site. The [`extension/`](extension) WebExtension closes
that gap: it reports the active tab's URL to a local HTTP server the app
runs on `127.0.0.1:47632`
([src-tauri/src/ingest.rs](src-tauri/src/ingest.rs)), and
`SessionTracker::handle_tab_event` closes the current row and opens a new one
for the same app whenever the active URL changes — exactly like switching
apps does — so a 3-hour Chrome session becomes a series of per-site rows in
the same `sessions` table instead of one row. See
[extension/README.md](extension/README.md) for how to load it. This is
entirely optional: without the extension loaded, browser sessions behave
exactly like any other app.

A tab event is only applied while a session for the *matching* browser is
currently focused — this stops a backgrounded browser's extension from
misattributing its tab changes to whatever app the OS says is focused.

## Data model

SQLite table `sessions`, one row per activity row — a plain app focus, or a
browser tab if the extension is loaded:

| column         | meaning                                                          |
| -------------- | ------------------------------------------------------------------ |
| `id`           | primary key                                                        |
| `app_name`     | executable file stem, e.g. `chrome`, `Code`                        |
| `exe_path`     | full path to the executable                                        |
| `window_title` | window title (or tab title, for a browser-tab row) when it started |
| `url`          | origin + path only, no query string or fragment; `NULL` unless this row is a browser tab |
| `start_time`   | RFC 3339 UTC timestamp                                              |
| `end_time`     | RFC 3339 UTC timestamp, `NULL` while the row is open                |

A row left with `end_time IS NULL` after a restart means the previous run
was killed rather than shut down cleanly; it's closed at its own
`start_time` on the next startup so a crash undercounts by one row instead of
silently inflating it by however long the machine was off.

## Running

Requires a Rust toolchain ([rustup](https://rustup.rs)), Node.js/npm, and,
since `rusqlite` builds SQLite from source (`bundled` feature), the MSVC C++
build tools (Visual Studio Build Tools, "Desktop development with C++").

```
npm install
npm run tauri dev
```

The database is written to the per-user app-data directory (on Windows,
`%APPDATA%\phucvinh57\makora\data\data.db`). Closing the window hides it to
the system tray instead of quitting — the focus watcher and tab-ingest
server keep running. Use "Quit" from the tray menu to actually exit; that
closes the currently open session with an accurate `end_time` before the
process ends.

## Known limitations (v1, Windows)

- Session identity is the executable path, so switching between two windows
  of the *same* app doesn't end the session — only switching to a different
  app does. `window_title` reflects the title at session start and doesn't
  update if it changes mid-session.
- Locking the screen or letting the machine sleep doesn't end a session by
  itself (Windows doesn't fire `EVENT_SYSTEM_FOREGROUND` for that) — a
  session left focused across a lock/sleep will include that idle time.
  Worth revisiting if it turns out to skew the numbers in practice.
- The tab-ingest server (`127.0.0.1:47632`) has no shared-secret auth — it
  relies on binding to loopback plus checking the request's `Origin` header
  is a `chrome-extension://`/`moz-extension://` origin. That stops ordinary
  web pages and unrelated local processes, but not another extension or a
  process willing to spoof that header.
- Non-`http(s)` tabs (`chrome://`, `about:`, extension pages, ...) and query
  strings/fragments are never recorded, by design — see
  [extension/README.md](extension/README.md).
- The window-hide-to-tray behavior and the packaged app are only exercised
  on Windows so far, even though `core/platform/macos.rs` implements the
  macOS focus watcher.
