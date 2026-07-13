# makora

A background collector that tracks how long you spend in each application,
as a series of focus sessions: a session starts the moment an app gains
focus and ends the moment a different app takes focus. Sessions are stored
locally in SQLite — nothing leaves the machine.

## Layout

```
crates/
  core/       platform-agnostic session logic (makora-core)
  collector/  the binary that wires a platform watcher to storage (makora-collector)
extension/    cross-browser WebExtension that reports the active tab's URL
```

### Why it's split this way

`makora-core` defines three things and nothing platform-specific depends on
the others:

- [`FocusWatcher`](crates/core/src/watcher.rs) — trait a platform implements
  to report focus changes. It owns its own event-loop thread and streams
  `FocusEvent`s over a channel; it has no opinion on sessions.
- [`SessionStore`](crates/core/src/store.rs) — trait a storage backend
  implements to open/close sessions. [`SqliteStore`](crates/core/src/sqlite_store.rs)
  is the only implementation today.
- [`SessionTracker`](crates/core/src/tracker.rs) — the only place session
  semantics live (same exe = same session; different exe = close the old
  session, open a new one). Every `FocusWatcher` impl and every
  `SessionStore` impl gets identical behavior for free because this piece
  doesn't know or care which platform or backend it's talking to.

Adding macOS support later means writing
[`crates/core/src/platform/macos.rs`](crates/core/src/platform/macos.rs)
(currently a stub returning an error) — implementing `FocusWatcher` against
`NSWorkspace` notifications — and nothing else changes.

Windows is implemented via `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`
([crates/core/src/platform/windows.rs](crates/core/src/platform/windows.rs)):
event-driven rather than polled, so session boundaries line up with the
exact moment the user switched windows and no CPU is spent while focus is
unchanged.

## Per-site browser tracking

An app session for a browser (Chrome, Edge, Firefox, Brave) is a single opaque
block by default — `SessionTracker` only knows the OS says "chrome.exe is
focused", not which site. The [`extension/`](extension) WebExtension closes
that gap: it reports the active tab's URL to a local HTTP server the
collector runs on `127.0.0.1:47632`
([crates/collector/src/ingest.rs](crates/collector/src/ingest.rs)), and
`SessionTracker::handle_tab_event` segments the open browser session into
per-URL `tab_sessions` rows — so a 3-hour Chrome session becomes a series of
per-site sub-sessions instead of one row. See
[extension/README.md](extension/README.md) for how to load it. This is
entirely optional: without the extension loaded, browser sessions behave
exactly like any other app.

A tab event is only applied while a session for the *matching* browser is
currently focused — this stops a backgrounded browser's extension from
misattributing its tab changes to whatever app the OS says is focused.

## Data model

SQLite table `sessions`, one row per app-focus session:

| column         | meaning                                             |
| -------------- | ---------------------------------------------------- |
| `id`           | primary key                                          |
| `app_name`     | executable file stem, e.g. `chrome`, `Code`           |
| `exe_path`     | full path to the executable                           |
| `window_title` | window title at the moment the session started        |
| `start_time`   | RFC 3339 UTC timestamp                                 |
| `end_time`     | RFC 3339 UTC timestamp, `NULL` while the session is open |

SQLite table `tab_sessions`, one row per URL sub-session within a browser
session (empty unless the extension is loaded):

| column       | meaning                                                    |
| ------------ | ------------------------------------------------------------ |
| `id`         | primary key                                                   |
| `session_id` | the parent `sessions.id` this tab activity happened within     |
| `url`        | origin + path only — no query string or fragment               |
| `title`      | tab title at the moment this URL became active                 |
| `start_time` | RFC 3339 UTC timestamp                                          |
| `end_time`   | RFC 3339 UTC timestamp, `NULL` while the tab sub-session is open |

A session (or tab sub-session) left with `end_time IS NULL` after a restart
means the previous run was killed rather than shut down cleanly; it's closed
at its own `start_time` on the next startup so a crash undercounts by one
session instead of silently inflating it by however long the machine was off.

## Running

Requires a Rust toolchain ([rustup](https://rustup.rs)) and, since
`rusqlite` builds SQLite from source (`bundled` feature), the MSVC C++ build
tools (Visual Studio Build Tools, "Desktop development with C++").

```
cargo run -p makora-collector
```

The database is written to the per-user app-data directory (on Windows,
`%APPDATA%\makora\makora\data\usage.db`). Stop the collector with Ctrl+C —
that closes the currently open session with an accurate `end_time` before
exiting.

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

## Build note

This was written and reviewed without a local Rust toolchain available to
compile it — `cargo build` hasn't actually been run against it. Dependencies
are pinned to their latest versions as of 2026-07-12 (checked against
crates.io/docs.rs, not compiled). Note in particular that `windows-sys`
0.6x represents `HANDLE`/`HWND`/`HMODULE`/`HWINEVENTHOOK` as `*mut c_void`
(they were `isize` in the 0.52 line) — `platform/windows.rs` is written
against the pointer representation. That file is still the highest-risk
spot for any further API drift; if `cargo build -p makora-core` fails,
that's where to look first.
