# Review guide

For human reviewers and the `/pre-pr` skill. When a bug slips through, add a line under **Misses**.

## Failures

- **Data leaves the machine.** Titles and URLs may go only to the remote LLM the user chose; embeddings never leave. A new network call, telemetry, or log line with titles or URLs is Important.
- **Time is lost or double-counted:** a row left open on focus change, lock, suspend or quit; overlapping rows; duration clipping in more than one place.
- **A migration loses data.** Never edit a shipped migration; add a new numbered file.
- **API keys** stored or logged in plain text.
- **AGENTS.md rules broken:** process-wide state before the single-instance check; `println!`/`eprintln!` outside the allowed spots.
- **Bridge:** the native-host manifest points at the wrong binary, or the socket/pipe accepts another user's messages.
- **Docs out of date** with the change (README, CONTRIBUTING, AGENTS.md).

## Risk tiers

Set by the paths touched. The highest wins.

- **High** (a human reads the diff): `crates/makora-core/src/db/migrations/`, `crates/makora-core/src/db/migrate.rs`, `crates/makora-core/src/ai/remote/`, `crates/makora-core/src/ai/secrets.rs`, `crates/makora-core/src/bin/`, `crates/makora-core/src/tracking/adapters/native_host.rs`, `crates/makora-core/src/tracking/adapters/bridge_wire.rs`, `crates/makora/src/runtime/single_instance.rs`, `packaging/`, `.github/workflows/`, `scripts/bundle.mjs`, `extensions/browser/manifest.json`, `extensions/gnome/`.
- **Medium:** other tracking, categorization, reporting and db code.
- **Low:** UI styling and copy, docs, `demo/`.

## Ignore

Generated files (lockfiles, `dist/`, `target/`), anything Biome, clippy or rustfmt enforce, and missing tests.

## Reporting

- Start with the number of findings, or "No issues found."
- Cite `path:line` for each. Never guess file names.
- Label each Important, Nit, or Pre-existing.
- For a bug fix, name the root cause. Name the class of bug so every instance gets fixed.

## Rounds

At most 3 per PR; after that, question the approach. After round 1, review only new commits. Don't re-raise anything already fixed or answered.

## Misses

One line per bug that shipped: PR, what broke, which check should have caught it.
