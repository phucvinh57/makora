## What this is

Makora is a local desktop time tracker: a native Rust app with a GPUI (gpui-component) UI and SQLite storage. It records one row per activity block: an app gains focus, a row opens; another app takes focus, it closes. Nothing leaves the machine unless the user opts into a remote LLM (OpenAI, Anthropic or OpenAI-compatible) for AI categorization; embeddings always stay local. The optional browser extension (`extensions/browser/`) reports the active tab so browser time splits per site.

## Commands

```
scripts/dev.sh             # build makora + makora-bridge, run the app (normal dev loop)
cargo run -p makora        # the app alone
cargo test --workspace
cargo clippy --workspace --all-targets
bun install                # JS tooling: Biome, lefthook, extension build
bun run lint / check       # Biome (no ESLint/Prettier)
bun run ext:check          # typecheck the extension
node scripts/bundle.mjs    # release packages: deb/rpm, app/dmg, nsis/msi
```

Build deps: Rust, Node/Bun, a C/C++ toolchain and CMake (SQLite and llama.cpp build from source), Vulkan headers and `glslc` on Linux/Windows. Linux also needs `libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libxcb1-dev libx11-xcb-dev libfontconfig-dev libfreetype-dev`.

Packaging config: `crates/makora/Cargo.toml` (`[package.metadata.{deb,generate-rpm,packager}]`) and `packaging/linux/`. Inspect packages with `dpkg-deb -c` / `rpm -qlp`; never install them on the dev machine.

## Layout

- Cargo workspace at the repo root (`Cargo.lock`, `target/`).
- `crates/makora-core`: lib `makora_lib` (all backend modules) and the `makora-bridge` bin.
- `crates/makora`: bin `makora`, the GPUI app. `runtime/` holds tray, window, single instance and autostart; `ui/` holds screens, dialogs and widgets.
- `Services` (`crates/makora-core/src/app_core.rs`) opens the database and settings, with no background work. `AppCore` adds the tracker, AI runner, schedule and downloads, and starts them. The UI calls `ClientCore`'s application-layer traits.
- Database: `makora.sqlite` in the app-data dir (`~/.local/share/dev.phucvinh57.makora/` on Linux), resolved by `AppPaths` in `crates/makora-core/src/paths.rs`. Debug builds accept `MAKORA_DATA_DIR` / `MAKORA_CONFIG_DIR`.

## Processes

On Linux, `makora` is a background process (`crates/makora/src/runtime/background.rs`: tracking, tray, notifications, AI schedule, no GPUI). It starts `makora --window` when the window is requested; closing the window ends that process. They talk over the child's stdin/stdout (`crates/makora-core/src/ipc/`): the window reads and writes SQLite and `settings.toml` itself, and asks the background process for AI runs and model downloads. Each AI run is a `makora --ai-worker` process, so llama.cpp and the GPU driver exit with it. macOS and Windows still run the window in the background process.

## Workflow

- **One worktree per independent change.** Start with `claude -w <name>` or ask for a worktree. They live in `.claude/worktrees/`. A hook runs `scripts/worktree-setup.sh` (JS deps, `target/` symlink to the main checkout); run it yourself after a plain `git worktree add`.
- **Show the result, don't claim it.** Run `scripts/check.sh` (checks for paths changed since `main`; `--all` for everything) and report its output. For UI changes, launch the app in the background and screenshot the affected screen. Never bypass the pre-push hook with `--no-verify`.
- **Finish with `/pre-pr`, without asking.** It checks, reviews against `REVIEW.md`, commits, and opens the PR (a `gh stack` for related changes). There is no AI review in CI. Stop and ask only when checks keep failing or findings remain after round 3.

## Rules

- **No unnecessary comments.**
- **No `println!`/`eprintln!`.** Use `tracing::{info, warn, error}`. Only exceptions: the bootstrap fallback in `crates/makora/src/lib.rs` (errors before the subscriber exists) and `cargo:` directives in build scripts.
- **Build process-wide state in `main` after the single-instance check** (`makora::run`), never before: a second launch exits there. The `--window` and `--ai-worker` children skip the check.
- **Back up the db before applying a new migration.**
