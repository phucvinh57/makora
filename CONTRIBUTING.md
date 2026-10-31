# Contributing to Makora

For bugs, include your operating system, desktop environment if relevant,
what you expected, and steps to reproduce. Use fictional examples when
sharing screenshots or activity history.

## Development prerequisites

- [Bun](https://bun.sh), [Node.js](https://nodejs.org), and a stable
  [Rust toolchain](https://www.rust-lang.org/tools/install).
- A C/C++ toolchain: SQLite is compiled from source.
- CMake and dependencies for the local AI backend: Vulkan on Windows/Linux,
  Metal on macOS.
- On Windows, install Visual Studio Build Tools with **Desktop development
  with C++**. For the Vulkan AI build, set a short Cargo target path in
  PowerShell before every build:
  ```powershell
  $env:CARGO_TARGET_DIR = "C:\mk"
  ```
- On Linux, install Tauri's WebKitGTK dependencies. On Debian/Ubuntu:
  ```sh
  sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev \
      librsvg2-dev libayatana-appindicator3-dev libssl-dev patchelf
  ```
  The AI backend also needs CMake, Vulkan development headers/libraries,
  and a shader compiler; package names depend on your distribution.
- On macOS, install Xcode command line tools. Grant Accessibility permission
  to Makora to read focused window titles. Unsigned development rebuilds may
  need that permission granted again because the app signature changes.

See the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)
for additional platform setup.

## Building and running

From the repository root:

```sh
bun install
bun tauri dev       # full desktop app: Vite frontend and Rust backend
```

Other commands:

```sh
bun dev             # frontend only; desktop commands need a Tauri host
bun run build       # route generation, TypeScript checks, frontend build
bun tauri build     # release bundle/installer
bun run lint        # Biome lint checks
bun run check       # Biome checks with automatic fixes
bun run format      # Biome formatting checks
```

The Rust backend is a single crate, not a workspace. From `src-tauri/`:

```sh
cargo build
cargo test          # run existing Rust tests
cargo clippy
```

Use Biome for formatting and linting, including React hook rules, rather
than ESLint or Prettier. The frontend build checks strict TypeScript.

## Project layout and architecture

| Location | Purpose |
| --- | --- |
| `src/` | React/TypeScript interface, routes, queries, shared components |
| `src-tauri/` | Tauri 2 desktop host and Rust backend |
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

Sessions live in `makora.sqlite` under Tauri's per-user app-data directory,
resolved in `src-tauri/src/lib.rs`:

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
`makora-bridge` (`src-tauri/src/bin/makora_bridge.rs`), which relays each
`{ browser, url, title }` message to the app over a per-user local socket
(`$XDG_RUNTIME_DIR/makora/bridge.sock` on Linux, a named pipe on Windows) and
drops it while the app is not running. On every start the app writes the host
manifests that point browsers at the `makora-bridge` next to its executable
(`tracking/adapters/native_host.rs`), so a dev run points browsers at
`target/debug/makora-bridge` until the installed app starts again.

`bun tauri dev` needs no extra step: Tauri builds every binary of the crate, so
`target/debug/makora-bridge` sits next to the dev app. Release installers need
the bridge bundled as a sidecar, which one `tauri build` cannot do; build them
with `bun run bundle` (`scripts/bundle.mjs`), as the release workflow does. A
plain `bun tauri build` produces installers without the bridge.

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
  The bootstrap fallback in `lib.rs` is the exception when logging setup fails.
- Build process-wide state inside Tauri's `setup`, never before `tauri::Builder`.
- Preserve local storage and local inference for activity data.

## Platform gaps

- Lock/sleep presence watchers are currently Linux-only.
- Windows UWP/Store apps can appear as `ApplicationFrameHost.exe`.
- macOS focus events and title access need more end-to-end hardware validation.
