# Contributing to Makora

For bugs, include your operating system, desktop environment if relevant,
what you expected, and steps to reproduce. Use fictional examples when
sharing screenshots or activity history.

## Development prerequisites

- A stable [Rust toolchain](https://www.rust-lang.org/tools/install), plus
  [Bun](https://bun.sh) and [Node.js](https://nodejs.org) for the browser
  extension and the Biome/lefthook tooling.
- A C/C++ toolchain: SQLite is compiled from source.
- CMake and dependencies for the local AI backend: Vulkan on Windows/Linux,
  Metal on macOS.
- On Windows, install Visual Studio Build Tools with **Desktop development
  with C++**. For the Vulkan AI build, set a short Cargo target path in
  PowerShell before every build:
  ```powershell
  $env:CARGO_TARGET_DIR = "C:\mk"
  ```
- On Linux, install GPUI's dependencies. On Debian/Ubuntu:
  ```sh
  sudo apt install build-essential libxkbcommon-dev libxkbcommon-x11-dev \
      libwayland-dev libxcb1-dev libx11-xcb-dev libfontconfig-dev \
      libfreetype-dev
  ```
  GPUI renders through Vulkan, so a Vulkan driver must be installed. GNOME
  needs the AppIndicator extension to show the tray icon.
  The AI backend also needs CMake, Vulkan development headers/libraries,
  and a shader compiler; package names depend on your distribution.
- On macOS, install Xcode command line tools. Grant Accessibility permission
  to Makora to read focused window titles. Unsigned development rebuilds may
  need that permission granted again because the app signature changes.

## Building and running

From the repository root:

```sh
scripts/dev.sh      # build the app and the browser bridge, then run the app
```

The Rust code is a Cargo workspace rooted at the repository root, which holds
`Cargo.lock` and `target/`:

```sh
cargo run -p makora             # the app alone
cargo build --workspace         # the app and makora-bridge
cargo test --workspace          # run existing Rust tests
cargo clippy --workspace --all-targets
```

The browser extension and scripts use Biome rather than ESLint or Prettier:

```sh
bun install
bun run lint        # Biome lint checks
bun run check       # Biome checks with automatic fixes
bun run format      # Biome formatting checks
```

## Packaging and releases

`node scripts/bundle.mjs` (or `bun run bundle`) builds `makora` and
`makora-bridge` in release mode and packages both for the host OS; it accepts
`--target <triple>`. Every package puts the bridge next to the app, because
the app registers the bridge it finds beside its own executable.

| OS | Tool | Formats | Output |
| --- | --- | --- | --- |
| Linux | [cargo-deb](https://crates.io/crates/cargo-deb), [cargo-generate-rpm](https://crates.io/crates/cargo-generate-rpm) | DEB, RPM | `target/debian/`, `target/generate-rpm/` |
| macOS | [cargo-packager](https://crates.io/crates/cargo-packager) | app, DMG (ad hoc signed, macOS 10.15+) | `target/packager/` |
| Windows | cargo-packager | NSIS, MSI | `target/packager/` |

Install the tools with `cargo install --locked cargo-deb cargo-generate-rpm`
or `cargo install --locked cargo-packager`. Their configuration lives in
`crates/makora/Cargo.toml` under `[package.metadata.deb]`,
`[package.metadata.generate-rpm]` and `[package.metadata.packager]`;
`crates/makora/build.rs` embeds the icon and version info into the Windows
executable. The Linux packages add the desktop entry from `packaging/linux/`,
whose name must stay `dev.phucvinh57.makora.desktop` to match the window's
app ID, and the GNOME extension files. `packaging/linux/post-install.sh`
copies the extension into GNOME Shell on GNOME Wayland and stops a running
copy of a pre-GPUI release, which does not share the single-instance lock
(`packaging/linux/deb/` links the Debian script names to these scripts). Check
a package with `dpkg-deb -c` or `rpm -qlp` instead of installing it on your
machine.

The release workflow (`.github/workflows/release.yaml`) runs on a `v*` tag
push and builds the Linux packages into a draft release; run it manually to
build macOS and Windows installers for the latest release. It sets the version
from the tag in the root `Cargo.toml` (`[workspace.package]`), the only place
it lives. The same workflow publishes the browser extension to the Chrome Web
Store for stable tags.

## Project layout and architecture

| Location | Purpose |
| --- | --- |
| `crates/makora-core/` | Rust backend (tracking, categorization, storage, AI) and the `makora-bridge` binary |
| `crates/makora/` | GPUI desktop app: window, tray, single instance, screens and dialogs |
| `packaging/` | Linux desktop entry and package maintainer scripts |
| `extensions/browser/` | Optional active-tab reporter |
| `extensions/gnome/` | GNOME Shell focus reporter |
| `demo/` | Remotion video and fictional screenshot fixtures |
| `docs/` | [Domain design](docs/ddd.md) and [trait guidance](docs/traits.md) |

The focus watcher and browser tab listener run in-process alongside the
desktop window. An app gaining focus opens a session row; switching to another
app closes it. Browser reports add the active URL and title so browser sessions
can be split by site. Closing or minimizing the window hides it to the tray;
tracking continues until the app quits.

Linux presence handling closes the current session on lock or suspend and
starts a fresh one on unlock or resume. It uses systemd-logind's `LockedHint`;
custom wlroots lock screens must update that property. Keyboard/mouse inactivity
alone does not stop tracking.

## Local data and migrations

Sessions live in `makora.sqlite` under the per-user app-data directory,
resolved by `AppPaths` in `crates/makora-core/src/paths.rs` (debug builds take
`MAKORA_DATA_DIR` and `MAKORA_CONFIG_DIR` overrides):

| Platform | Directory |
| --- | --- |
| Windows | `%APPDATA%\dev.phucvinh57.makora\` |
| macOS | `~/Library/Application Support/dev.phucvinh57.makora/` |
| Linux | `~/.local/share/dev.phucvinh57.makora/` |

**Back up the database before applying a new migration.** Stop Makora before
copying the database, or use SQLite's backup API for a consistent live backup.
Do not commit real activity data.

## Categorization

Activities identify an app/site together with its title. A manual pin applies
to every occurrence of that activity, past and future. Category resolution
prefers a manual pin, then a matching rule, then an AI suggestion. Rules are
retroactive and re-evaluated when edited. AI inference is local; model downloads
use the remote model registry.

Deleting a category also deletes its subcategories and clears associated
categorization references. Resetting categorization clears manual pins and
AI labels while keeping categories, rules, and tracking history.

## Browser extension development

From the repository root:

```sh
bun run ext:build    # bundle TypeScript and copy manifest/icons to dist/
bun run ext:watch    # rebuild on changes
bun run ext:check    # extension TypeScript checks
```

Load `extensions/browser/dist/` as an unpacked extension. It is gitignored, so
a fresh checkout needs a build first. Reload the extension after rebuilding.
See the [installation guide](extensions/browser/README.md#installing) for
Chromium browsers and Firefox.

The extension reaches the app through native messaging. The browser starts
`makora-bridge` (`crates/makora-core/src/bin/makora_bridge.rs`), which relays each
`{ browser, url, title }` message to the app over a per-user local socket
(`$XDG_RUNTIME_DIR/makora/bridge.sock` on Linux, a named pipe on Windows) and
drops it while the app is not running. On every start the app writes the host
manifests that point browsers at the `makora-bridge` next to its executable
(`tracking/adapters/native_host.rs`), so a dev run points browsers at
`target/debug/makora-bridge` until the installed app starts again.

`scripts/dev.sh` builds the bridge with the app, so `target/debug/makora-bridge`
sits next to the dev app. `cargo run -p makora` alone does not rebuild the
bridge.

## GNOME and Wayland development

GNOME on Wayland requires the [focus reporter](extensions/gnome/README.md).
Wayland blocks ordinary clients from inspecting focused windows. GNOME's
introspection API is allowlisted to the desktop portal, and Mutter's
`ext-foreign-toplevel-list-v1` protocol does not report focus state.

From a checkout:

```sh
./extensions/gnome/install.sh
# Log out and back in after the first installation, then:
gnome-extensions enable makora@phucvinh57.dev
gnome-extensions info makora@phucvinh57.dev
```

The DEB and RPM packages install the extension only when a GNOME Wayland
session is detected. The install script skips other sessions. Makora notices
an extension installed later without requiring an app restart. On wlroots
compositors that expose `wlr-foreign-toplevel-management-v1`, such as sway,
Hyprland, and river, no GNOME extension is needed.

The [GNOME extension guide](extensions/gnome/README.md) documents its D-Bus
contract, supported Shell versions, and debugging commands. Keep protocol names
and versions in sync with the Rust platform implementation when changing it.

## Demo video

The [Remotion demo guide](demo/README.md) covers screenshot capture, preview,
typechecking, and rendering. Capture injects fictional responses into an
isolated browser context; it never reads or writes the real activity database.

## Change guidelines

- Keep changes focused and describe the resulting behavior in pull requests.
- Follow `AGENTS.md`; do not write new tests for now. Run existing checks
  relevant to your changes.
- Avoid unnecessary comments.
- Use `tracing::{info, warn, error}` instead of `println!` or `eprintln!`.
  The bootstrap fallback in `crates/makora/src/lib.rs`, for errors before
  logging is set up, and `cargo:` directives in build scripts are the
  exceptions.
- Build process-wide state in `main` after the single-instance check, never
  before it.
- Preserve local storage and local inference for activity data.

## Platform gaps

- Lock/sleep presence watchers are currently Linux-only.
- Windows UWP/Store apps can appear as `ApplicationFrameHost.exe`.
- macOS focus events and title access need more end-to-end hardware validation.
