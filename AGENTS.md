## What this is

Makora is a local desktop time tracker: Tauri 2 + Rust backend, React/TypeScript frontend, SQLite storage. It records one row per activity block — an app gains focus, a row opens; a different app takes focus, it closes. Nothing leaves the machine unless the user opts into a remote LLM provider (OpenAI, Anthropic or OpenAI-compatible, configured under Settings) for AI categorization; embeddings always stay local. An optional browser extension (`extensions/browser/`) reports the active tab so browser time splits per site instead of one opaque `chrome` block.

## Commands

```
bun install
bun tauri dev      # full app (Vite + Rust) — the normal dev loop
bun dev            # frontend only
bun run build      # tsc typecheck + vite build
```

From `src-tauri/` (single crate, no workspace): `cargo build`, `cargo test`, `cargo clippy`.

No ESLint/Prettier: Biome lints and formats (`bun run lint`, `bun run check`, with the React hook rules on), and `tsc` in `bun run build` enforces TypeScript strictness.

Build deps: Rust, Node, a C/C++ toolchain (`rusqlite` builds SQLite from source). Linux also needs Tauri's WebKitGTK stack (`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf`).

Database: `makora.sqlite` in the app-data dir (`~/.local/share/dev.phucvinh57.makora/` on Linux), resolved by `AppPaths` in `src-tauri/src/paths.rs`. Debug builds take `MAKORA_DATA_DIR` / `MAKORA_CONFIG_DIR` to run on other data.

Backend layout: `AppCore` (`src-tauri/src/app_core.rs`) builds every service and starts the background work; the Tauri commands in `src-tauri/src/host/` are thin wrappers over it.

## Workflow

- **One worktree per independent change.** Start with `claude -w <name>`, or ask to work in a worktree. Worktrees live in `.claude/worktrees/`. A hook runs `scripts/worktree-setup.sh`, which installs JS deps and points cargo at the main checkout's `target/`. Run it yourself for a worktree made with plain `git worktree add`.
- **Show the result, don't claim it.** `scripts/check.sh` runs the checks that cover the paths changed since `main` (`--all` runs every check). Report its output. For UI changes, also launch `bun tauri dev` in the background and screenshot the affected screen. The pre-push hook runs `scripts/check.sh` too; don't bypass it with `--no-verify`.
- **Finish with `/pre-pr`.** It checks, reviews against `REVIEW.md`, commits, and opens the PR. Related changes go up as a stack with `gh stack`. There is no AI review in CI.
- **Run `/pre-pr` without asking.** When a change is done, run it straight away. If checks pass and review findings are resolved, commit, push and open the PR without waiting for confirmation. Stop and ask only when checks keep failing or findings remain after round 3.

## Rules

- **Do not add unnecessary comments.**
- **No `println!`/`eprintln!`** — use `tracing::{info, warn, error}`. The only exception is the bootstrap fallback in `lib.rs`, for when the subscriber failed to install.
- **Build process-wide state inside `setup`**, never before `tauri::Builder`.
- **Before applying new migration, make sure to back up the db first.
