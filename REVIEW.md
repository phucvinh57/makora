# Review guide

For human reviewers and the local `/pre-pr` skill. Keep it short. When a bug slips through, add a line under **Misses**.

## What counts as a failure

- Activity data leaves the machine. Titles and URLs may only go to the remote LLM provider the user opted into; embeddings never leave. A new network call, telemetry, or log line carrying titles/URLs is Important.
- Tracked time is lost or double-counted: a session row left open on focus change, lock/suspend or quit; overlapping rows; duration clipping done in more than one place.
- A migration loses or corrupts user data. Shipped migration files are never edited; changes go in a new numbered file.
- API keys stored or logged in plain text.
- Process-wide state built before the single-instance check in `makora::run`; `println!`/`eprintln!` outside the `crates/makora/src/lib.rs` bootstrap fallback and build-script `cargo:` directives.
- The bridge or native-host manifest points browsers at the wrong binary, or the socket/pipe accepts another user's messages.
- Docs that lie: README, CONTRIBUTING or AGENTS.md out of step with the change.

## Risk tiers

Set by the paths a change touches, not by who wrote it.

- **High**, a human reads the diff: `crates/makora-core/src/db/migrations/`, `crates/makora-core/src/db/migrate.rs`, `crates/makora-core/src/ai/remote/`, `crates/makora-core/src/ai/secrets.rs`, `crates/makora-core/src/bin/`, `crates/makora-core/src/tracking/adapters/native_host.rs`, `crates/makora-core/src/tracking/adapters/bridge_wire.rs`, `crates/makora/src/runtime/single_instance.rs`, `packaging/`, `.github/workflows/`, `scripts/bundle.mjs`, `extensions/browser/manifest.json`, `extensions/gnome/`.
- **Medium**: other tracking, categorization, reporting and db code.
- **Low**: UI styling and copy, docs, `demo/`.

## Ignore

- Generated files: lockfiles, `dist/`, `target/`.
- Anything Biome or clippy already enforces, rustfmt formatting, and missing tests (none required for now).

## How to report

- Open with a summary: the number of findings, or "No issues found."
- Cite `path:line` from the diff for every finding. Never guess file names.
- Label each finding Important, Nit, or Pre-existing (not introduced by this diff).
- For a bug fix, name the root cause, not only the symptom.
- Name the class of bug so the author fixes every instance in one pass.

## Converge

- At most 3 review rounds per PR. After round 3, question the approach instead of the lines.
- After round 1, review only the new commits.
- Don't re-report an issue already raised, fixed, or resolved with a reason.

## Misses

One line per bug that shipped (revert or fix-forward): PR, what broke, which check should have caught it.
