# CLAUDE.md

## What this is

Makora is a local desktop time tracker: Tauri 2 + Rust backend, React/TypeScript frontend, SQLite storage. It records one row per activity block — an app gains focus, a row opens; a different app takes focus, it closes. Nothing leaves the machine. An optional browser extension (`extension/`) reports the active tab so browser time splits per site instead of one opaque `chrome` block.

## Commands

```
bun install
bun tauri dev      # full app (Vite + Rust) — the normal dev loop
bun dev            # frontend only
bun run build      # tsc typecheck + vite build
```

From `src-tauri/` (single crate, no workspace): `cargo build`, `cargo test`, `cargo clippy`.

No ESLint/Prettier. TypeScript strictness is enforced by `tsc` in `bun run build`.

Build deps: Rust, Node, and a C/C++ toolchain (`rusqlite` builds SQLite from source). Linux also needs Tauri's WebKitGTK stack (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf`).

Database: `makora.sqlite` in Tauri's app-data dir (`~/.local/share/dev.phucvinh57.makora/` on Linux), resolved by `database_path` in `src-tauri/src/lib.rs`.

## Architecture

### Backend

`src-tauri/src/core` is platform- and storage-agnostic, behind two traits:

- **`FocusWatcher`** (`core/watcher.rs`) — a platform reports focus changes on its own thread. Impls in `core/platform/`: windows, macos, linux/.
- **`SessionStore`** (`core/store/mod.rs`) — `SqliteStore` is the only impl. `HistoryReader` (`core/store/queries.rs`) is a *separate read-only connection* for UI queries, so reads never contend with the write lock.
- **`SessionTracker`** (`core/tracker.rs`) — the only place session semantics live: same exe = same session, different exe = close + open, away = close but retain the last app/tab snapshot, return = open fresh.

`lib.rs` wires everything in the Tauri `setup` hook; `commands.rs` exposes the `invoke` API.

**All process-wide state must be built inside `setup`, not before `tauri::Builder`.** The app is single-instance; a second launch exits from inside the single-instance plugin initializer, and anything constructed before the builder would run in that doomed process.

Closing/minimizing hides to the tray and keeps tracking. Only tray "Quit" exits, closing the open session first.

### Linux focus

Wayland gives clients no way to ask what's focused, and GNOME blocks the usual workarounds. So there's one provider per desktop stack behind `ToplevelProvider` (`core/platform/linux/provider.rs`), probed in order:

- `gnome.rs` — zbus client for the GNOME Shell extension in `gnome-extension/`. **Bus name, path, interface and `PROTOCOL_VERSION` are duplicated in `gnome-extension/extension.js` — change one, change the other.**
- `wlr.rs` — `wlr-foreign-toplevel-management-v1` for sway/Hyprland/river. Untested.

`proc.rs` turns a focused window into `AppInfo`. `exe_path` is the session identity and must stay stable, so snap revisions are rewritten to `current`, flatpaks are keyed on app id, and pid-less windows fall back to `wayland-app-id://<app_id>`.

`presence.rs` uses systemd-logind (lock hint + sleep state) to close the row when the user leaves and open a fresh one on return. Input inactivity is deliberately *not* a presence signal.

### Per-site browser tracking

The extension POSTs `{ browser, url, title }` to `127.0.0.1:47632` (`src-tauri/src/ingest.rs`) on tab change; `SessionTracker::handle_tab_event` closes and reopens a row for the same app. Tab events only apply while that browser is focused. **Port `47632` is hardcoded in both `extension/background.js` and `ingest.rs` — change both.** Auth is loopback binding + an `Origin` header check; enough to stop web pages, not a local process.

### Data model

Table `sessions`: `id`, `app_name` (exe file stem), `exe_path`, `window_title` (at start, not updated), `url` (origin + path only — fragment and query string are stripped in `ingest::normalize_url`; `NULL` for non-browser rows), `start_time`, `end_time` (`NULL` while open).

A row still open at startup means the last run crashed; it's closed at its own `start_time` (undercount rather than inflate).

Categorization (`core/category.rs`, `core/store/categories.rs`):

- Two lookup keys are written per session: `category_key` (URL host, else exe name) and `url_key` (host + path, `NULL` for non-browser). Computed at write time so rule edits re-categorize history without re-parsing.
- Category resolves at read time via `COALESCE`: per-session pin (`sessions.category_id`) first, then the best-matching rule. Rules are the whole taxonomy — there is no "apply everywhere" tier.
- Rule conflicts resolve by specificity only (`rule_join_sql`), no manual ordering: `url` kind beats `exe`/`domain`, then `exact` > `prefix` > `substring` > `regex`, then longer pattern.
- "Recategorize" (`recategorize_all`) clears *every* pin and leaves history resolved from rules alone. Rules are never touched.
- Categories are an arbitrarily deep tree (`parent_id`). Names are unique among siblings only. Rules and pins name one exact category; only *reads* widen to the subtree (`subtree_ids_sql`) — history filtering, focus on-task marking, statistics rollup (rollup happens in the UI).
- A fresh database has **no categories and no rules**; everything is "Uncategorized" until a rule set is loaded.
- Foreign keys are on by default (`SQLITE_DEFAULT_FOREIGN_KEYS=1`). Migrations that rebuild a *referenced* table only work because `store::sqlite::migrate` lifts the check around `to_latest` — it can't be toggled inside a migration's transaction.

### Frontend

React + TypeScript, Vite, Tailwind v4, TanStack Router file routes in `src/routes/`, base-ui-derived components in `src/components/ui/`. `src/api.ts` wraps `invoke`; `src/types.ts` mirrors the Rust serialize types.

- No paged category API: pages load the whole flat list once (`listCategories`, query key `["categories"]`) and build the tree client-side with `src/categories.ts`. `src/components/CategoryOptions.tsx` renders every category dropdown, so all four `Select`s look alike.
- The history time filter lives in the URL (`?kind=…&from=&to=&sessionId=`, see `routes/history/-components/timeRange.ts`) so other pages can link into a scoped view. Relative presets are stored as the preset, so "today" keeps meaning today. Rows *overlapping* the window match — unlike `activity_stats`, which buckets by `start_time`.

## Known limitations

- Session identity is the exe path — switching between two windows of the same app doesn't end the session.
- Lock/sleep only ends the row on Linux; Windows and macOS have no presence watcher yet.
- Non-`http(s)` tabs are never recorded. Query strings are stripped, so pages distinguished only by a param collapse (every YouTube video is `youtube.com/watch`).
- macOS's focus watcher and the `wlr.rs` provider are implemented but not exercised end-to-end.
- On GNOME the tray needs a StatusNotifierItem host (Ubuntu has one). Under libappindicator, left-clicking the tray icon does nothing — only menu items work.
