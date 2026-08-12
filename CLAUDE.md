# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Makora is a desktop app (Tauri 2 + Rust backend, React/TypeScript frontend) that tracks how long the user spends in each application as a series of focus sessions: a session starts when an app gains focus and ends when a different app takes it. Sessions are stored locally in SQLite — nothing leaves the machine. A companion browser WebExtension (`extension/`) reports the active tab's URL so browser time can be broken down per site instead of one opaque `chrome`/`firefox` block.

## Commands

Requires a Rust toolchain, Node.js, and — because `rusqlite` builds SQLite from source via the `bundled` feature — a C/C++ toolchain: MSVC build tools on Windows (Visual Studio Build Tools, "Desktop development with C++"), `build-essential` on Linux. Linux additionally needs Tauri's WebKitGTK stack (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf`).

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

The database is written to the per-user app-data directory (on Windows, `%APPDATA%\phucvinh57\makora\data\data.db`; on Linux, `~/.local/share/makora/makora.sqlite`).

## Architecture

### Backend: `src-tauri/src/core` is platform- and backend-agnostic

Everything platform-specific or storage-specific is isolated behind two traits, so business logic (session semantics) doesn't depend on either:

- **[`FocusWatcher`](src-tauri/src/core/watcher.rs)** — trait a platform implements to report focus changes. Owns its own event-loop thread and streams `WatchEvent::Focus` observations over the same channel Linux presence events use; it has no opinion on sessions.
  - Windows (`src-tauri/src/core/platform/windows.rs`): event-driven via `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` — no polling, session boundaries line up exactly with focus changes.
  - macOS (`src-tauri/src/core/platform/macos.rs`): `NSWorkspace` activation notifications + Accessibility API for window titles. Implemented but not yet exercised end-to-end the way Windows is.
  - Linux (`src-tauri/src/core/platform/linux/`): a directory, not a file — see below.
- **[`SessionStore`](src-tauri/src/core/store/mod.rs)** — trait a storage backend implements to open/close sessions. `SqliteStore` (`src-tauri/src/core/store/sqlite.rs`) is the only implementation. `HistoryReader` (`src-tauri/src/core/store/queries.rs`) is a *separate, read-only* connection to the same database used by the UI-facing query API — it never contends with the write path's lock.
- **[`SessionTracker`](src-tauri/src/core/tracker.rs)** — the only place session semantics live (same exe = same session; different exe = close old, open new; away = close the row but retain the latest app/tab snapshot; return = open a fresh row). Every `FocusWatcher` impl and every `SessionStore` impl gets identical behavior for free.

`src-tauri/src/lib.rs` wires it all together in the Tauri `setup` hook: starts the focus watcher on its own thread, spawns the tab-ingest HTTP server, builds the tray icon — all inside the single Tauri process. `src-tauri/src/commands.rs` exposes Tauri commands (currently `list_sessions`) that the frontend calls via `invoke`.

Closing or minimizing the window hides it to the system tray instead of quitting (`on_window_event` in `lib.rs` calls `api.prevent_close()`); tracking keeps running in the background. Only "Quit" from the tray menu (`src-tauri/src/tray.rs`) actually exits, closing the currently open session with an accurate `end_time` first.

### Linux: focus has to come from inside the compositor

Wayland gives a client no way to ask which window is focused, and on GNOME both usual workarounds are closed: `org.gnome.Shell.Introspect` is allowlisted to xdg-desktop-portal-gnome (everyone else gets `Access denied`), and `ext-foreign-toplevel-list-v1` — which Mutter *does* implement — carries only `title`/`app_id`/`identifier` and no focus state at all. Mutter does not implement `wlr-foreign-toplevel-management-v1`.

So there is no single Linux watcher, only one per desktop stack behind `ToplevelProvider` (`src-tauri/src/core/platform/linux/provider.rs`). `LinuxFocusWatcher` probes them in order at startup:

- `gnome.rs` — a zbus client for the GNOME Shell extension in `gnome-extension/`, which owns `dev.phucvinh57.MakoraShell` on the session bus and exports a `FocusChanged` signal plus a `GetFocusedWindow()` method (so the app picks up what's already focused instead of waiting for the next switch). The provider tolerates the extension being absent and re-syncs via `NameOwnerChanged` when it appears, so installing/enabling it needs no app restart. **The bus name, path, interface and `PROTOCOL_VERSION` are duplicated in `gnome-extension/extension.js` — change one, change the other**, same rule as the ingest port.
- `wlr.rs` — `wlr-foreign-toplevel-management-v1` for sway/Hyprland/river. Its `state` event carries the `activated` flag Mutter's protocol lacks. Implemented but not exercised end-to-end.

A provider only reports *what is focused*; `proc.rs` turns that into `AppInfo`. That split matters because `exe_path` is the session identity and has to stay stable: snap revisions (`/snap/firefox/<rev>/…`) are rewritten to `current` so an update doesn't split an app's history in two, flatpaks (whose `/proc/<pid>/exe` is `/app/bin/…` inside every sandbox) are keyed on their application id, and windows with no pid at all — wlr reports none — fall back to `wayland-app-id://<app_id>`. GNOME reports a *null* focus window whenever the Activities overview opens; that's dropped rather than treated as a focus change, so the open session survives it.

`presence.rs` is separate from those focus providers and works across Linux desktops through systemd-logind. It resolves the app's graphical login session (falling back to the user's `Display` session for systemd-user/autostart launches), composes the session's `LockedHint` with the manager's `PrepareForSleep` state, and emits one `WatchEvent::Presence` only when effective presence changes. Lock or suspend closes the activity row; focus/tab observations received while away only update the retained snapshot; unlock/wake opens a fresh row. Input inactivity is deliberately not a presence signal because it cannot distinguish watching a video from leaving the computer. Custom wlroots lockers must maintain logind's `LockedHint` for lock detection to work.

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
| `url`          | origin + path, plus any query params whitelisted for the domain (`url_query_whitelist` table, editable from Settings — e.g. YouTube's `v`); fragment never recorded; `NULL` unless a browser-tab row |
| `start_time`   | RFC 3339 UTC timestamp |
| `end_time`     | RFC 3339 UTC timestamp, `NULL` while the row is open |

A row left with `end_time IS NULL` after a restart means the previous run was killed rather than shut down cleanly; it's closed at its own `start_time` on next startup (so a crash undercounts by one row rather than inflating it by however long the machine was off).

`src-tauri/src/core/category.rs` derives two stable lookup keys per session, both computed once at write time so rule/override edits later re-categorize history without re-parsing: `category_key` (URL host for a browser-tab row, exe file name otherwise — coarse grouping) and `url_key` (host + path + whitelisted query, `NULL` for non-browser rows — lets two pages on the same domain, e.g. two Google Docs documents, resolve to different categories). `src-tauri/src/core/store/categories.rs` is the storage side.

A session's category is resolved at read time via `COALESCE`, most specific tier first: a per-session pin (`sessions.category_id`), then a key-wide manual override (`category_overrides`), then the best-matching rule. Rule conflicts resolve purely by specificity (`category::rule_join_sql`) — categories have no manual ordering: a `url`-kind rule beats an `exe`/`domain`-kind rule, then match kind ranks `exact` > `prefix` > `substring` > `regex`, then the longer pattern wins.

### Frontend

`src/` is a small React + TypeScript app (Vite, Tailwind v4, shadcn-derived components in `src/components/ui/`). `src/api.ts` wraps Tauri `invoke` calls; `src/types.ts` mirrors the Rust `#[derive(Serialize)]` types returned by commands. `HistoryPage.tsx` is the main (currently only) view, paginating through `list_sessions`.

## Known limitations

- Session identity is the executable path — switching between two windows of the *same* app doesn't end the session, only switching to a different app does.
- Locking the screen or sleeping ends the current activity row on Linux through systemd-logind. Windows and macOS still keep the focused row open across lock/sleep until native presence watchers are added there.
- Non-`http(s)` tabs (`chrome://`, `about:`, extension pages, ...) are never recorded, by design. The fragment never is either; the query string is stripped except for params whitelisted per domain in Settings.
- The tray/hide-on-close behavior and the packaged app are exercised on Windows and Linux (GNOME); macOS's focus watcher is implemented but not exercised end-to-end.
- On GNOME the tray depends on a StatusNotifierItem host, which vanilla GNOME does not provide (Ubuntu does, via its bundled AppIndicator extension). Left-clicking the tray icon to show the window does *not* work under libappindicator — only the tray menu's items do.
- The `wlr.rs` provider (sway/Hyprland/river) is implemented but untested; that protocol reports no pid, so sessions there are keyed on `app_id` rather than an executable path.
