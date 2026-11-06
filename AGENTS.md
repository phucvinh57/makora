## What this is

Makora is a local desktop time tracker: a native Rust app with a GPUI (gpui-component) UI and SQLite storage. It records one row per activity block — an app gains focus, a row opens; a different app takes focus, it closes. Nothing leaves the machine unless the user opts into a remote LLM provider (OpenAI, Anthropic or OpenAI-compatible, configured under Settings) for AI categorization; embeddings always stay local. An optional browser extension (`extensions/browser/`) reports the active tab so browser time splits per site instead of one opaque `chrome` block.

## Commands

```
scripts/dev.sh             # builds makora + makora-bridge, runs the app — the normal dev loop
cargo run -p makora        # the app alone
cargo test --workspace
cargo clippy --workspace --all-targets
bun install                # JS tooling: Biome, lefthook, the browser extension build
node scripts/bundle.mjs    # release installers: deb + rpm (cargo-deb, cargo-generate-rpm) or app/dmg, nsis/msi (cargo-packager)
```

Packaging config lives in `crates/makora/Cargo.toml` (`[package.metadata.{deb,generate-rpm,packager}]`) and `packaging/linux/`. Inspect built packages with `dpkg-deb -c` / `rpm -qlp`; never install them on the dev machine.

Cargo workspace at the repo root (`Cargo.lock` and `target/` live there). Members: `crates/makora-core` (lib `makora_lib`, every backend module, and the `makora-bridge` bin) and `crates/makora` (bin `makora`, the GPUI app: `runtime/` for tray, window, single instance and autostart; `ui/` for screens, dialogs and widgets).

No ESLint/Prettier: Biome lints and formats the extension and scripts (`bun run lint`, `bun run check`); `bun run ext:check` typechecks the extension.

Build deps: Rust, Node/Bun, a C/C++ toolchain and CMake (SQLite and llama.cpp build from source), Vulkan headers and `glslc` on Linux/Windows. Linux also needs GPUI's libraries (`libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libxcb1-dev libx11-xcb-dev libfontconfig-dev libfreetype-dev`).

Database: `makora.sqlite` in the app-data dir (`~/.local/share/dev.phucvinh57.makora/` on Linux), resolved by `AppPaths` in `crates/makora-core/src/paths.rs`. Debug builds take `MAKORA_DATA_DIR` / `MAKORA_CONFIG_DIR` to run on other data.

Backend layout: `Services` (`crates/makora-core/src/app_core.rs`) opens the database and settings with no background work; `AppCore` adds the tracker, AI runner, schedule and downloads and starts the background work; the UI in `crates/makora/src/ui/` calls `ClientCore`'s application-layer traits.

Processes: on Linux, `makora` is a background process (`crates/makora/src/runtime/background.rs`: tracking, tray, notifications, AI schedule, no GPUI) that starts `makora --window` when the window is asked for; closing the window ends that process. The two talk over the child's stdin/stdout (`crates/makora-core/src/ipc/`): the window reads and writes SQLite and `settings.toml` itself, and asks the background process for AI runs and model downloads. Each AI run is a `makora --ai-worker` process, so llama.cpp and the GPU driver leave with it. macOS and Windows still run the window in the background process.

## Workflow

- **One worktree per independent change.** Start with `claude -w <name>`, or ask to work in a worktree. Worktrees live in `.claude/worktrees/`. A hook runs `scripts/worktree-setup.sh`, which installs JS deps and symlinks `target/` to the main checkout's. Run it yourself for a worktree made with plain `git worktree add`.
- **Show the result, don't claim it.** `scripts/check.sh` runs the checks that cover the paths changed since `main` (`--all` runs every check). Report its output. For UI changes, also launch the app (`cargo run -p makora`) in the background and screenshot the affected screen. The pre-push hook runs `scripts/check.sh` too; don't bypass it with `--no-verify`.
- **Finish with `/pre-pr`.** It checks, reviews against `REVIEW.md`, commits, and opens the PR. Related changes go up as a stack with `gh stack`. There is no AI review in CI.
- **Run `/pre-pr` without asking.** When a change is done, run it straight away. If checks pass and review findings are resolved, commit, push and open the PR without waiting for confirmation. Stop and ask only when checks keep failing or findings remain after round 3.

## Rules

- **Do not add unnecessary comments.**
- **No `println!`/`eprintln!`** — use `tracing::{info, warn, error}`. The only exceptions are the bootstrap fallback in `crates/makora/src/lib.rs`, for errors before the subscriber is installed, and `cargo:` directives in build scripts.
- **Build process-wide state in `main` after the single-instance check** (`makora::run`), never before it: a second launch exits there. The `--window` and `--ai-worker` children skip the check; the app starts them.
- **Before applying new migration, make sure to back up the db first.
