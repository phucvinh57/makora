# Makora review fixes

## Context
A read-only review (3 parallel reviewers + Biome/tsc/clippy) found correctness, safety and structure issues across the Rust backend, browser ingest and the React frontend. Worst: a crash deletes the open (possibly merged, multi-hour) activity row; model loading freezes the UI; downloads can hang forever; migrations run without the backup AGENTS.md requires; the fixed-port ingest server crashes startup when the port is taken and is shared across OS users.

User decisions (2026-10-31):
- WIP diff: keep the cleanup, restore a **compact** remote-provider disclosure, fix the empty `<p>`.
- Browser ingest: **replace the localhost HTTP server with browser native messaging**.
- Tests: **unit tests for pure logic only** (overrides AGENTS.md "no tests" for this work).
- Scope: include **Windows/macOS sleep+lock**; defer idle detection, app identity (UWP/interpreted), incremental rule rematch.

## Deliverable layout
Work happens on branch `fix/review-findings` (carrying the uncommitted WIP diff, which 12 finishes). Each plan file = one or more small commits (why / what / how verified). Push the branch; merge to `main` only with user OK. Keep [LOG.md](LOG.md) with every action + reason.

Order matters: 01 (backup) must land before any new migration (02).

## Conventions
- Order: 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13; 99 = deferred.
- Per unit: (pure logic) write failing unit test → fix → `cargo test`; then `cargo clippy --all-targets`, `bun run build`, `bun run lint`.
- No `println!`; `tracing` only. No new comments unless non-obvious.
- Before running the app against the real DB after a migration change: copy `~/.local/share/dev.phucvinh57.makora/makora.sqlite` aside. Run app/builds in background.
- Windows/macOS code can only be compile-checked (CI `release.yaml` matrix / `cargo check --target`); flag in PR.

## Files
- [01-db-migration-safety.md](01-db-migration-safety.md)
- [02-tracking-crash-recovery.md](02-tracking-crash-recovery.md)
- [03-tracking-state-machine.md](03-tracking-state-machine.md)
- [04-browser-native-messaging.md](04-browser-native-messaging.md)
- [05-presence-windows-macos.md](05-presence-windows-macos.md)
- [06-concurrency-and-locks.md](06-concurrency-and-locks.md)
- [07-model-download.md](07-model-download.md)
- [08-ai-run-and-llm.md](08-ai-run-and-llm.md)
- [09-reporting.md](09-reporting.md)
- [10-settings-and-providers.md](10-settings-and-providers.md)
- [11-backend-types-cleanup.md](11-backend-types-cleanup.md)
- [12-frontend-correctness.md](12-frontend-correctness.md)
- [13-frontend-structure.md](13-frontend-structure.md)
- [99-deferred.md](99-deferred.md)

## Verification (end-to-end)
- Every unit: `cd src-tauri && cargo test && cargo clippy --all-targets`; `bun run build && bun run lint`.
- After 01–04: run `bun tauri dev` (background) against a **copy** of the real DB: backup file appears, migration 0011 applies, kill -9 keeps the open row, Chrome/Firefox tab tracking via native messaging works.
- Windows/macOS (04, 05): CI build on the release matrix; manual check on those OSes listed as open item in the PR.
