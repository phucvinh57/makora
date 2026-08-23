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

No ESLint/Prettier. TypeScript strictness is enforced by `tsc` in `bun run build`.

Build deps: Rust, Node, and a C/C++ toolchain (`rusqlite` builds SQLite from source). Linux also needs Tauri's WebKitGTK stack (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf`). macOS requires Accessibility permission for window titles; unsigned development rebuilds may need to be granted again after the app signature changes.

Database: `makora.sqlite` in Tauri's app-data dir (`~/.local/share/dev.phucvinh57.makora/` on Linux), resolved by `database_path` in `src-tauri/src/lib.rs`.

## Architecture

### Backend

`src-tauri/src/core` is platform- and storage-agnostic, behind two traits:

- **`FocusWatcher`** (`core/watcher.rs`) — a platform reports focus changes on its own thread. Impls in `core/platform/`: windows, macos, linux/.
- **`SessionStore`** (`core/store/mod.rs`) — `SqliteStore` is the only impl. `HistoryReader` (`core/store/queries.rs`) is the read-only *API* for UI queries, but not a separate connection: `lib.rs` hands every store the same `Arc<Mutex<Connection>>`. `resolve_all` holds that mutex; the tracker thread blocks but loses nothing, since focus events carry their own timestamps and queue in the channel.
- **`SessionTracker`** (`core/tracker.rs`) — the only place session semantics live: same exe = same session, different exe = close + open, away = close but retain the last app/tab snapshot, return = open fresh.

`lib.rs` wires everything in the Tauri `setup` hook; `commands.rs` exposes the `invoke` API.

**All process-wide state must be built inside `setup`, not before `tauri::Builder`.** The app is single-instance; a second launch exits from inside the single-instance plugin initializer, and anything constructed before the builder would run in that doomed process.

Closing/minimizing hides to the tray and keeps tracking. Only tray "Quit" exits, closing the open session first.

### Logging

`tracing`, installed by `logging::init` as the first thing in `setup` (it is process-wide state, so the same rule applies). Use `tracing::{info, warn, error}` — **no `println!`/`eprintln!` anywhere except the bootstrap fallback in `lib.rs`**, which fires when the subscriber itself failed to install.

- Debug builds log to stderr with ANSI; `bun tauri dev` shows them in the terminal.
- Release builds have no terminal (`windows_subsystem = "windows"`), so they log to `<app-data>/logs/makora.<yyyy-mm-dd>.log`, rolled daily by `tracing-appender` and pruned to the newest 30 files.
- Default filters are `makora=debug,warn` (dev) and `makora=info,warn` (release) — third-party crates stay at `warn` so tao/wry chatter doesn't drown the app's own events. `RUST_LOG` overrides both.
- The non-blocking writer's `WorkerGuard` is parked in a `static OnceLock`; dropping it kills the writer thread, so it must live as long as the process.

### Linux focus

Wayland gives clients no way to ask what's focused, and GNOME blocks the usual workarounds. So there's one provider per desktop stack behind `ToplevelProvider` (`core/platform/linux/provider.rs`), probed in order:

- `gnome.rs` — zbus client for the GNOME Shell extension in `gnome-extension/`. **Bus name, path, interface and `PROTOCOL_VERSION` are duplicated in `gnome-extension/extension.js` — change one, change the other.**
- `wlr.rs` — `wlr-foreign-toplevel-management-v1` for sway/Hyprland/river. Untested.

`proc.rs` turns a focused window into `AppInfo`. `exe_path` is the session identity and must stay stable, so snap revisions are rewritten to `current`, flatpaks are keyed on app id, and pid-less windows fall back to `wayland-app-id://<app_id>`.

`presence.rs` uses systemd-logind (lock hint + sleep state) to close the row when the user leaves and open a fresh one on return. Input inactivity is deliberately *not* a presence signal.

### Per-site browser tracking

The extension POSTs `{ browser, url, title }` to `127.0.0.1:47632` (`src-tauri/src/ingest.rs`) on tab change; `SessionTracker::handle_tab_event` closes and reopens a row for the same app. Tab events only apply while that browser is focused. **Port `47632` is hardcoded in both `browser-extension/src/background.ts` and `ingest.rs` — change both.** The extension is TypeScript bundled by `browser-extension/build.mjs` (esbuild) into `browser-extension/dist/`, which is the directory browsers load and is gitignored; `bun run ext:build` / `ext:watch` / `ext:check` drive it. Auth is loopback binding + an `Origin` header check; enough to stop web pages, not a local process.

### Data model

Table `sessions`: `id`, `app_name` (exe file stem), `exe_path`, `window_title` (at start, not updated), `url` (origin + path only — fragment and query string are stripped in `ingest::normalize_url`; `NULL` for non-browser rows), `start_time`, `end_time` (`NULL` while open).

A row still open at startup means the last run crashed; it's deleted rather than closed at a guessed time (undercount rather than inflate).

**Blocks shorter than one second are deleted, not closed.** A redirect, a tab reported before its title has loaded, or a window flashing past on the way somewhere else all open and close a row within the same second, and none of them are activity. `SqliteStore::end_session` drops such a row instead of setting `end_time`; migration 20 purged the ones already recorded. Because no sub-second row can exist, **no query filters by duration** — don't add a `WHERE duration >= …` anywhere, and if the threshold ever changes, the migration and `MIN_SESSION_SECONDS` have to move together.

Categorization (`core/category.rs`, `core/store/categories.rs`):

- Two lookup keys are written per session: `category_key` (URL host, else exe name) and `url_key` (host + path, `NULL` for non-browser). Computed at write time so rule edits re-categorize history without re-parsing.
- **Categorization is materialized, not resolved at read time.** Rust resolves `sessions.rule_category_id` when a row opens or closes and persists it in the same transaction; background AI jobs persist `ml_category_id`. Reads are a plain `COALESCE(s.category_id, s.rule_category_id, s.ml_category_id)` — no rule join or classifier call anywhere.
- The classifier has two document levels: one key document per `(key_kind, category_key)` for coherent rule suggestions, and one activity document per `(key_kind, category_key, url_key, window_title)` for multi-purpose title classification. Activity documents include open rows with zero duration, and title embeddings use `key_embeddings.key_kind = 'title'` with stale entries pruned on each run.
- `multi_purpose_keys` identifies domains/apps that need title-aware labels. Enabled exact `exe`/`domain` rules remain stored but are skipped at resolve time; `url` rules still win, so repeated stable paths can graduate from title predictions into reviewable URL rules. User removal is a disabled veto, not deletion.
- Because nothing re-derives on read, **anything that can change the answer must replay history**: `store::resolve::resolve_all` is called by `create_rule`, `delete_rule`, `delete_category`, `recategorize_all`, seed application, and multi-purpose toggles, each inside the caller's own transaction. The classifier also replays inside the same transaction when automatic detection changes the enabled set. Miss one and history silently disagrees with the rule list.
- Rule suggestions must be *clearable*. A key already resolved by a stored rule is never suggested (`rule_already_covers` asks the real resolver, so globs and prefixes count), a key whose pins disagree keeps its dominant pin as its label instead of falling out of `load_labeled_keys` and looking unlabeled, and accepting a suggestion retargets an existing exact rule rather than failing. Break any of the three and the same domains reappear on every run.
- `sessions.category_id` stays a *pin only* — it is what `category_is_pinned` reports. Automatic labels never go there.
- AI categorization is scheduled-only background work (`core/classifier.rs`). Startup, tracking, label/category/rule edits, and recategorization never enqueue it. Only `run_ai_categorization` or the persisted manual/hourly/daily/weekly scheduler may run it; missed intervals are skipped while the app is stopped. A successful title pass atomically replaces qualifying `ml_*` labels while preserving pins and surviving rules.
- AI evaluation is append-only in `ai_verdicts` and `ai_run_evaluations`. Verdict outcomes are computed at the moment a user accepts, rejects, corrects, or pins an AI result and are never recomputed on read. Category names are stored as snapshots and category foreign keys use `ON DELETE SET NULL`, so deleting a category cannot rewrite historical precision. New label-write paths must record a verdict or explicitly document why they are not a user judgment. The Accuracy report also records holdout accuracy per successful classifier run (including cached-embedding runs), with a manual-labels-only variant; neither reviewed precision nor holdout accuracy is presented as the single truth.
- Rule conflicts resolve in Rust by specificity only, with no manual ordering: `url` kind beats `exe`/`domain`, then `exact` > `prefix` > `substring` > `glob`, then longer pattern, then lower rule id.
- Category and ignore rules share the Rust matcher in `core/category.rs`. Matching is ASCII-case-insensitive; globs are anchored, `*` crosses `/`, and malformed new globs are rejected. Malformed legacy globs remain visible but inactive. Regex was retired in migration 14, which deletes every `match_kind = 'regex'` row from `category_rules` *and* `ignore_rules`.
- **`MatchKind` lives in four places with no compiler help**: `core/category.rs`, the CHECK constraints in the migrations, `src/types.ts`, and the label maps in `RulesTab.tsx` / `IgnoreListSection.tsx`. Change one, change all four.
- Activity duration does not affect categorization. A matching rule applies to open or closed rows; an unmatched row remains Uncategorized until it receives a manual pin or AI prediction.
- "Recategorize" (`recategorize_all`) preserves every manual pin, clears AI predictions, and replays the rules. Rules are never touched, and pins continue to take precedence over the refreshed rule labels.
- Categories are an arbitrarily deep tree (`parent_id`). Names are unique among siblings only. Rust expands descendants for history filtering, focus on-task marking, cycle prevention, and branch deletion; statistics rollup still happens in the UI.
- A fresh database has no rules and no categories. Every category is user-owned; names such as "Distraction" have no reserved behavior. Activity stays Uncategorized until a rule, manual pin, or AI prediction labels it.
- `settings` is a key/value table. It stores the AI schedule and last-completion timestamp, plus an internal resolver-version marker used to retry Rust migration backfills safely.
- Foreign keys are on by default (`SQLITE_DEFAULT_FOREIGN_KEYS=1`). Migrations that rebuild a *referenced* table only work because `store::sqlite::migrate` lifts the check around `to_latest` — it can't be toggled inside a migration's transaction.

### Frontend

React + TypeScript, Vite, Tailwind v4, TanStack Router file routes in `src/routes/`, base-ui-derived components in `src/components/ui/`. `src/api.ts` wraps `invoke`; `src/types.ts` mirrors the Rust serialize types.

- No paged category API: pages load the whole flat list once (`listCategories`, query key `["categories"]`) and build the tree client-side with `src/categories.ts`. `src/components/CategoryOptions.tsx` renders every category dropdown, so all four `Select`s look alike.
- **Any category or rule mutation must invalidate `["sessions"]` and `["activityStats"]`, not just `["categories"]`/`["categoryRules"]`.** Those mutations now rewrite `sessions` rows on the backend, so the cached history and statistics are genuinely stale, not merely out of sync.
- AI completion and multi-purpose mutations must also invalidate `["sessions"]`, `["activityStats"]`, `["categoryRules"]`, `["multiPurposeKeys"]`, and `["aiAccuracyReport"]` because they rewrite materialized session labels, rule effectiveness, and evaluation history. History pins must invalidate `["aiAccuracyReport"]` too.
- Accuracy verdict history is forward-only: a new database has no report until a user judgment is recorded. The report’s reviewed precision is a lower bound because users preferentially correct suspicious rows; holdout accuracy is an in-sample leave-one-key-out trend, with a manual-labels-only variant.
- The history time filter lives in the URL (`?kind=…&from=&to=&sessionId=`, see `routes/history/-components/timeRange.ts`) so other pages can link into a scoped view. Relative presets are stored as the preset, so "today" keeps meaning today. Rows *overlapping* the window match — unlike `activity_stats`, which buckets by `start_time`.
- History can also filter by effective category source: Manual means `sessions.category_id` is pinned, while AI means no manual/rule category wins and `ml_category_id` supplies the displayed category.

## Known limitations

- Session identity is the exe path — switching between two windows of the same app doesn't end the session.
- Lock/sleep only ends the row on Linux; Windows and macOS have no presence watcher yet.
- macOS requires Accessibility permission for window titles. Without it, tracking continues but every `window_title` is empty; unsigned development rebuilds may need the permission granted again because the app signature changes.
- Windows UWP/Store apps currently collapse under `ApplicationFrameHost.exe`; resolving their child windows is deferred until it can be tested on Windows hardware.
- Non-`http(s)` tabs are never recorded. Query strings are stripped, so pages distinguished only by a param collapse (every YouTube video is `youtube.com/watch`).
- macOS's focus watcher and the `wlr.rs` provider are implemented but not exercised end-to-end.
- On GNOME the tray needs a StatusNotifierItem host (Ubuntu has one). Under libappindicator, left-clicking the tray icon does nothing — only menu items work.
