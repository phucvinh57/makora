# CLAUDE.md

## What this is

Makora is a local desktop time tracker: Tauri 2 + Rust backend, React/TypeScript frontend, SQLite storage. It records one row per activity block — an app gains focus, a row opens; a different app takes focus, it closes. Nothing leaves the machine. An optional browser extension (`browser-extension/`) reports the active tab so browser time splits per site instead of one opaque `chrome` block.

## Commands

```
bun install
bun tauri dev      # full app (Vite + Rust) — the normal dev loop
bun dev            # frontend only
bun run build      # tsc typecheck + vite build
```

From `src-tauri/` (single crate, no workspace): `cargo build`, `cargo test`, `cargo clippy`.

No ESLint/Prettier; `tsc` in `bun run build` enforces TypeScript strictness.

Build deps: Rust, Node, a C/C++ toolchain (`rusqlite` builds SQLite from source). Linux also needs Tauri's WebKitGTK stack (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf`).

Database: `makora.sqlite` in Tauri's app-data dir (`~/.local/share/dev.phucvinh57.makora/` on Linux), resolved by `database_path` in `src-tauri/src/lib.rs`.

## Rules

- **Do not write useless tests.** A test must guard real behavior or a past bug. No tests that restate the implementation, assert on mocks, cover trivial getters/wiring, or exist only to raise coverage.
- **No `println!`/`eprintln!`** — use `tracing::{info, warn, error}`. The only exception is the bootstrap fallback in `lib.rs`, for when the subscriber failed to install.
- **Build process-wide state inside `setup`**, never before `tauri::Builder`. The app is single-instance; a second launch exits inside the single-instance plugin initializer, so anything built earlier runs in that doomed process. This includes `logging::init`, which must be the first thing in `setup`.

## Backend

`src-tauri/src/core` is platform- and storage-agnostic, behind two traits:

- **`FocusWatcher`** (`core/watcher.rs`) — a platform reports focus changes on its own thread. Impls in `core/platform/`: windows, macos, linux/.
- **`SessionStore`** (`core/store/mod.rs`) — `SqliteStore` is the only impl. `HistoryReader` (`core/store/queries.rs`) is the read-only API for UI queries, sharing the same `Arc<Mutex<Connection>>` that `lib.rs` hands every store. `resolve_all` holds that mutex; the tracker thread blocks but loses nothing, since focus events carry their own timestamps and queue in the channel.
- **`SessionTracker`** (`core/tracker.rs`) — the only place session semantics live: same exe = same session, different exe = close + open, away = close but retain the last app/tab snapshot, return = open fresh.

`lib.rs` wires everything in the Tauri `setup` hook; `commands.rs` exposes the `invoke` API.

Closing/minimizing hides to the tray and keeps tracking. Only tray "Quit" exits, closing the open session first.

### Logging

Release builds have no terminal (`windows_subsystem = "windows"`), so they log to `<app-data>/logs/makora.<yyyy-mm-dd>.log`, rolled daily and pruned to the newest 30 files. Debug builds log to stderr. Default filters are `makora=debug,warn` (dev) and `makora=info,warn` (release); `RUST_LOG` overrides both. The non-blocking writer's `WorkerGuard` lives in a `static OnceLock` — dropping it kills the writer thread.

### Linux focus

Wayland can't report focus to clients and GNOME blocks the usual workarounds, so there is one provider per desktop stack behind `ToplevelProvider` (`core/platform/linux/provider.rs`), probed in order:

- `gnome.rs` — zbus client for the GNOME Shell extension in `gnome-extension/`. **Bus name, path, interface and `PROTOCOL_VERSION` are duplicated in `gnome-extension/extension.js` — change both.**
- `wlr.rs` — `wlr-foreign-toplevel-management-v1` for sway/Hyprland/river. Untested.

`proc.rs` turns a focused window into `AppInfo`. `exe_path` is the session identity and must stay stable: snap revisions are rewritten to `current`, flatpaks are keyed on app id, pid-less windows fall back to `wayland-app-id://<app_id>`.

`presence.rs` uses systemd-logind (lock hint + sleep state) to close the row when the user leaves and open a fresh one on return. Input inactivity is deliberately *not* a presence signal.

### Per-site browser tracking

The extension POSTs `{ browser, url, title }` to `127.0.0.1:47632` (`src-tauri/src/ingest.rs`) on tab change; `SessionTracker::handle_tab_event` closes and reopens a row for the same app. Tab events only apply while that browser is focused. **Port `47632` is hardcoded in both `browser-extension/src/background.ts` and `ingest.rs` — change both.** The extension is bundled by `browser-extension/build.mjs` (esbuild) into the gitignored `browser-extension/dist/`, which is what browsers load; drive it with `bun run ext:build` / `ext:watch` / `ext:check`. Auth is loopback binding + an `Origin` header check — enough to stop web pages, not a local process.

### Data model

Table `activities` is the identity table: `(key_kind, key, url_key, title)`, where `key_kind`/`key` are the executable or normalized domain, `url_key` is the normalized resource URL, and `title` is empty for browser placeholders. `display_title` keeps the best raw title for history. Rules and manual pins live on the activity; sessions inherit them through `sessions.activity_id`.

Table `sessions` stores occurrences: `id`, `activity_id`, `app_name`, `exe_path`, `window_title`, `url`, `start_time`, `end_time` (`NULL` while open). The tracker resumes the most recent closed occurrence of the same activity when the gap is at most `MERGE_GAP_SECONDS` (2s). `SessionStore::refine_title` upgrades placeholder titles in place and repoints the occurrence when the canonical identity becomes real.

Legacy session category columns remain nullable for old databases; all categorization reads and writes use `activities`.

A row still open at startup means the last run crashed; it's deleted rather than closed at a guessed time (undercount rather than inflate).

**Blocks shorter than one second are deleted, not closed.** Redirects, tabs reported before their title loads, and windows flashing past are not activity. `SqliteStore::end_session` drops such a row instead of setting `end_time`. Since no sub-second row can exist, **no query filters by duration** — don't add `WHERE duration >= …`. If the threshold changes, the migration and `MIN_SESSION_SECONDS` move together.

### Categorization

Code: `core/category.rs`, `core/store/categories.rs`.

- Two lookup keys are written per session: `category_key` (URL host, else exe name) and `url_key` (host + path, `NULL` for non-browser). Computed at write time so rule edits re-categorize history without re-parsing.
- **Materialized, not resolved at read time.** Rust resolves `activities.rule_category_id` when an activity is created or a rule changes; reads join through `activities`.
- **Anything that can change the answer must replay history** via `store::resolve::resolve_all`, inside the caller's own transaction. Today: `create_rule`, `delete_rule`, `delete_category`, `recategorize_all`, seed application. Miss one and history silently disagrees with the rule list.
- `activities.category_id` is a *pin only* (what `category_is_pinned` reports). Rule results never go there.
- Rule conflicts resolve by specificity only, no manual ordering: `url` kind beats `exe`/`domain`, then `exact` > `prefix` > `substring` > `glob`, then longer pattern, then lower rule id.
- Category and ignore rules share the matcher in `core/category.rs`. Matching is ASCII-case-insensitive; globs are anchored and `*` crosses `/`. Malformed new globs are rejected; malformed legacy globs stay visible but inactive. Regex was retired in migration 14, which deleted every `match_kind = 'regex'` row from `category_rules` *and* `ignore_rules`.
- **`MatchKind` lives in four places with no compiler help**: `core/category.rs`, the CHECK constraints in the migrations, `src/types.ts`, and the label maps in `RulesTab.tsx` / `IgnoreListSection.tsx`. Change all four.
- Duration doesn't affect categorization; a rule applies to open or closed rows. An unmatched row stays Uncategorized until a rule or manual pin labels it.
- `recategorize_all` replays the rules and never touches rules or manual pins; pins still take precedence.
- Categories are an arbitrarily deep tree (`parent_id`), names unique among siblings only. Rust expands descendants for history filtering, focus on-task marking, cycle prevention, and branch deletion; statistics rollup happens in the UI.
- A fresh database has no rules and no categories. Every category is user-owned; names like "Distraction" have no reserved behavior.
- `settings` is a key/value table, also holding an internal resolver-version marker used to retry Rust migration backfills safely.
- Foreign keys are on by default (`SQLITE_DEFAULT_FOREIGN_KEYS=1`). Migrations that rebuild a *referenced* table only work because `store::sqlite::migrate` lifts the check around `to_latest` — it can't be toggled inside a migration's transaction.

## Frontend

React + TypeScript, Vite, Tailwind v4, TanStack Router file routes in `src/routes/`, base-ui-derived components in `src/components/ui/`. `src/api.ts` wraps `invoke`; `src/types.ts` mirrors the Rust serialize types.

- No paged category API: pages load the flat list once (`listCategories`, query key `["categories"]`) and build the tree client-side with `src/categories.ts`. `src/components/CategoryOptions.tsx` renders every category dropdown.
- **Any category or rule mutation must invalidate `["sessions"]` and `["activityStats"]`**, not just `["categories"]`/`["categoryRules"]` — these mutations rewrite `sessions` rows on the backend.
- The history time filter lives in the URL (`?kind=…&from=&to=&sessionId=`, see `routes/history/-components/timeRange.ts`) so other pages can link into a scoped view. Relative presets are stored as the preset, so "today" keeps meaning today. Rows *overlapping* the window match — unlike `activity_stats`, which buckets by `start_time`.
- History can filter by category source: Manual means `activities.category_id` is pinned. The history table shows a `×N` badge for the number of occurrences of the activity in the selected range.

## Known limitations

- Session identity is the exe path — switching between two windows of the same app doesn't end the session.
- Lock/sleep only ends the row on Linux; Windows and macOS have no presence watcher yet.
- macOS needs Accessibility permission for window titles; without it every `window_title` is empty. Unsigned development rebuilds may need it granted again.
- Windows UWP/Store apps collapse under `ApplicationFrameHost.exe`; resolving child windows is deferred until it can be tested on Windows hardware.
- Non-`http(s)` tabs are never recorded. Query strings are stripped unless the domain/parameter pair is in `url_query_whitelist` (YouTube's `v` is seeded). Older history may retain collapsed keys.
- macOS's focus watcher and `wlr.rs` are implemented but not exercised end-to-end.
- On GNOME the tray needs a StatusNotifierItem host (Ubuntu has one). Under libappindicator, left-clicking the tray icon does nothing — only menu items work.
