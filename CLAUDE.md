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
- **Build process-wide state inside `setup`**, never before `tauri::Builder`.
