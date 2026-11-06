# Contributing to Makora

When reporting a bug, include your OS, desktop environment if relevant, what you expected, and steps to reproduce. Use fictional data in screenshots and activity history.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable), [Bun](https://bun.sh) and [Node.js](https://nodejs.org).
- A C/C++ toolchain and CMake: SQLite and llama.cpp build from source.
- **Linux:** a Vulkan driver, Vulkan headers, a shader compiler (`glslc`), and GPUI's libraries. On Debian/Ubuntu:

  ```sh
  sudo apt install build-essential libxkbcommon-dev libxkbcommon-x11-dev \
      libwayland-dev libxcb1-dev libx11-xcb-dev libfontconfig-dev libfreetype-dev
  ```
- **macOS:** Xcode command line tools. Dev rebuilds change the app signature, so you may need to grant Accessibility again.
- **Windows:** Visual Studio Build Tools with **Desktop development with C++**, and Vulkan. Set a short target path before every build: `$env:CARGO_TARGET_DIR = "C:\mk"`.

## Build and run

```sh
scripts/dev.sh                          # build the app and makora-bridge, then run the app
cargo run -p makora                     # the app alone (does not rebuild the bridge)
cargo test --workspace
cargo clippy --workspace --all-targets
scripts/check.sh                        # the checks for what changed since main

bun install
bun run lint                            # Biome; no ESLint or Prettier
bun run check                           # Biome with fixes
```

## Layout

| Location              | Purpose                                                                        |
| --------------------- | ------------------------------------------------------------------------------ |
| `crates/makora-core/` | Backend: tracking, categorization, storage, AI, and the `makora-bridge` binary |
| `crates/makora/`      | The GPUI app: window, tray, single instance, screens and dialogs               |
| `packaging/`          | Linux desktop entry and package scripts                                        |
| `extensions/browser/` | Optional active-tab reporter ([guide](../extensions/browser/README.md))        |
| `extensions/gnome/`   | GNOME Shell focus reporter ([guide](../extensions/gnome/contributing.md))      |
| `demo/`               | Remotion demo video ([guide](../demo/README.md))                               |

## How it works

- **Tracking.** An app gaining focus opens an activity row; another app taking focus closes it. Browser tab reports split browser rows by site. Locking or suspending closes the row (systemd-logind's `LockedHint` on Linux, workspace sleep and lock notifications on macOS, session and power messages on Windows); idle input alone does not.
- **Processes.** `makora` runs in the background (tracking, tray, notifications, AI schedule) and starts `makora --window` for the window. Each AI run is a `makora --ai-worker` process. On macOS the Dock icon shows only while the window is open.
- **Browser bridge.** The browser starts `makora-bridge`, which relays tab messages to the app over a per-user socket (`$XDG_RUNTIME_DIR/makora/bridge.sock` on Linux, a named pipe on Windows). Each start, the app registers the bridge next to its own executable, so a dev run points browsers at `target/debug/makora-bridge` until the installed app starts again.
- **GNOME.** GNOME Wayland hides window focus from apps, so a [Shell extension](../extensions/gnome/README.md) reports it over D-Bus.

## Categorization

A category comes from a manual pin first, then a matching rule, then an AI suggestion. Pins and rules apply to past and future activity.

Before a remote AI provider sees an activity, the `Redactor` (`crates/makora-core/src/ai/privacy/`) replaces detected values with placeholders, strips ids from URLs, and masks never-send apps. The local model gets the full text. To add a detector:

1. Write a finder in `privacy/detectors.rs`, with positive and near-miss tests.
2. Add its `Kind`, placeholder and priority in `privacy/mod.rs`.
3. Add a label in `crates/makora/src/ui/format/privacy.rs`.

## Local data

The database is `makora.sqlite` in the app-data directory. Debug builds accept `MAKORA_DATA_DIR` and `MAKORA_CONFIG_DIR`.

| Platform | Directory                                              |
| -------- | ------------------------------------------------------ |
| Windows  | `%APPDATA%\dev.phucvinh57.makora\`                     |
| macOS    | `~/Library/Application Support/dev.phucvinh57.makora/` |
| Linux    | `~/.local/share/dev.phucvinh57.makora/`                |

**Back up the database before applying a new migration.** Stop Makora first, or use SQLite's backup API. Never commit real activity data.

## Packaging and releases

`bun run bundle` builds release packages for the host OS (`--target <triple>` to cross-build). Every package puts `makora-bridge` next to the app.

| OS      | Tool                                                                                                               | Formats   | Output                                   |
| ------- | ------------------------------------------------------------------------------------------------------------------ | --------- | ---------------------------------------- |
| Linux   | [cargo-deb](https://crates.io/crates/cargo-deb), [cargo-generate-rpm](https://crates.io/crates/cargo-generate-rpm) | DEB, RPM  | `target/debian/`, `target/generate-rpm/` |
| macOS   | [cargo-packager](https://crates.io/crates/cargo-packager)                                                          | app, DMG  | `target/packager/`                       |
| Windows | cargo-packager                                                                                                     | NSIS, MSI | `target/packager/`                       |

- Config lives in `crates/makora/Cargo.toml` under `[package.metadata.{deb,generate-rpm,packager}]`.
- The Linux desktop file must stay `dev.phucvinh57.makora.desktop` to match the window's app ID.
- `packaging/linux/post-install.sh` installs the GNOME extension on GNOME Wayland and stops a running pre-GPUI release.
- Inspect packages with `dpkg-deb -c` or `rpm -qlp`; don't install them on your dev machine.

Pushing a `v*` tag runs `.github/workflows/release.yaml`: it sets the version in the root `Cargo.toml`, builds Linux packages into a draft release, and publishes the browser extension for stable tags. Run the workflow by hand to build macOS and Windows installers.

## Guidelines

- Keep each change focused. Describe the resulting behavior in the PR.
- Follow the rules in [AGENTS.md](../AGENTS.md). No new tests for now; run the existing checks.
- Keep activity data local.

## Known gaps

- Windows Store apps can show up as `ApplicationFrameHost.exe`.
- macOS focus and title tracking needs more testing on real hardware.
