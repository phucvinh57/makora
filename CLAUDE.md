# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Makora is a desktop app (Tauri 2 + Rust backend, React/TypeScript frontend) that tracks how long the user spends in each application as a series of focus sessions: a session starts when an app gains focus and ends when a different app takes it. Sessions are stored locally in SQLite — nothing leaves the machine. A companion browser WebExtension (`extension/`) reports the active tab's URL so browser time can be broken down per site instead of one opaque `chrome`/`firefox` block.

## Commands

Requires a Rust toolchain, Node.js, and — because `rusqlite` builds SQLite from source via the `bundled` feature — MSVC C++ build tools on Windows (Visual Studio Build Tools, "Desktop development with C++").

```
bun install          # installs JS deps (bun.lock is present but bun scripts are what's wired up)
bun tauri dev    # runs the full app (Vite dev server + Rust backend) — the normal dev loop
bun dev           # frontend only (Vite), no Tauri/Rust backend
bun run build          # tsc typecheck + vite build (frontend only)
```

Rust backend (`src-tauri/`, a single crate — no workspace):

```
cargo build           # from src-tauri/
cargo test             # run Rust unit tests (see src-tauri/src/core/category.rs and tracker.rs)
cargo clippy
```

There is no ESLint/Prettier config in the repo; TypeScript strictness (`strict`, `noUnusedLocals`, `noUnusedParameters`) is enforced via `tsc` in `bun run build`.

The database is written to the per-user app-data directory (on Windows, `%APPDATA%\phucvinh57\makora\data\data.db`).

## Architecture

### Backend: `src-tauri/src/core` is platform- and backend-agnostic

Everything platform-specific or storage-specific is isolated behind two traits, so business logic (session semantics) doesn't depend on either:

- **[`FocusWatcher`](src-tauri/src/core/watcher.rs)** — trait a platform implements to report focus changes. Owns its own event-loop thread, streams `FocusEvent`s over a channel; has no opinion on sessions.
  - Windows (`src-tauri/src/core/platform/windows.rs`): event-driven via `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` — no polling, session boundaries line up exactly with focus changes.
  - macOS (`src-tauri/src/core/platform/macos.rs`): `NSWorkspace` activation notifications + Accessibility API for window titles. Implemented but not yet exercised end-to-end the way Windows is.
- **[`SessionStore`](src-tauri/src/core/store/mod.rs)** — trait a storage backend implements to open/close sessions. `SqliteStore` (`src-tauri/src/core/store/sqlite.rs`) is the only implementation. `HistoryReader` (`src-tauri/src/core/store/queries.rs`) is a *separate, read-only* connection to the same database used by the UI-facing query API — it never contends with the write path's lock.
- **[`SessionTracker`](src-tauri/src/core/tracker.rs)** — the only place session semantics live (same exe = same session; different exe = close old, open new). Every `FocusWatcher` impl and every `SessionStore` impl gets identical behavior for free.

`src-tauri/src/lib.rs` wires it all together in the Tauri `setup` hook: starts the focus watcher on its own thread, spawns the tab-ingest HTTP server, builds the tray icon — all inside the single Tauri process. `src-tauri/src/commands.rs` exposes Tauri commands (currently `list_sessions`) that the frontend calls via `invoke`.

Closing or minimizing the window hides it to the system tray instead of quitting (`on_window_event` in `lib.rs` calls `api.prevent_close()`); tracking keeps running in the background. Only "Quit" from the tray menu (`src-tauri/src/tray.rs`) actually exits, closing the currently open session with an accurate `end_time` first.

### Per-site browser tracking

`SessionTracker` normally only knows "chrome.exe is focused," not which site. The `extension/` WebExtension POSTs `{ browser, url, title }` to a local HTTP server on `127.0.0.1:47632` (`src-tauri/src/ingest.rs`) whenever the active tab changes; `SessionTracker::handle_tab_event` then closes the current row and opens a new one for the same app — exactly like switching apps does — so a long Chrome session becomes per-site rows instead of one row. A tab event is only applied while a session for the *matching* browser is currently focused, so a backgrounded browser's extension can't misattribute tab changes to whatever the OS says is focused. This is entirely optional — without the extension loaded, browsers behave like any other app.

The ingest server has no shared-secret auth; it relies on binding to loopback plus checking the request's `Origin` header is a `chrome-extension://`/`moz-extension://` origin. Good enough to stop ordinary web pages, not a spoofed header from another local process.

The port `47632` is hardcoded in both `extension/background.js` and `src-tauri/src/ingest.rs` — if you change one, change the other (see `extension/README.md`).

### Data model

SQLite table `sessions`, one row per activity block (plain app focus, or a browser tab if the extension is loaded):

| column         | meaning |
| -------------- | ------- |
| `id`           | primary key |
| `app_name`     | executable file stem, e.g. `chrome`, `Code` |
| `exe_path`     | full path to the executable |
| `window_title` | window/tab title at session start (does not update mid-session) |
| `url`          | origin + path only, no query string/fragment; `NULL` unless a browser-tab row |
| `start_time`   | RFC 3339 UTC timestamp |
| `end_time`     | RFC 3339 UTC timestamp, `NULL` while the row is open |

A row left with `end_time IS NULL` after a restart means the previous run was killed rather than shut down cleanly; it's closed at its own `start_time` on next startup (so a crash undercounts by one row rather than inflating it by however long the machine was off).

`src-tauri/src/core/category.rs` derives a stable `category_key` per session (URL host for a browser-tab row, exe file name otherwise) so sessions can later be grouped/categorized without re-parsing at query time; `src-tauri/src/core/store/categories.rs` is the storage side of that. This is present in the codebase but not yet wired into `commands.rs`/the frontend.

### Frontend

`src/` is a small React + TypeScript app (Vite, Tailwind v4, shadcn-derived components in `src/components/ui/`). `src/api.ts` wraps Tauri `invoke` calls; `src/types.ts` mirrors the Rust `#[derive(Serialize)]` types returned by commands. `HistoryPage.tsx` is the main (currently only) view, paginating through `list_sessions`.

## Known limitations (v1, Windows)

- Session identity is the executable path — switching between two windows of the *same* app doesn't end the session, only switching to a different app does.
- Locking the screen or sleep doesn't end a session by itself (Windows doesn't fire `EVENT_SYSTEM_FOREGROUND` for that) — a session left focused across a lock/sleep includes that idle time.
- Non-`http(s)` tabs (`chrome://`, `about:`, extension pages, ...) and query strings/fragments are never recorded, by design.
- The tray/hide-on-close behavior and the packaged app are only exercised on Windows so far, even though macOS's focus watcher is implemented.
