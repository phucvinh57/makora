# Log

- 2026-10-31: Review (3 parallel reviewers + Biome/tsc/clippy) → findings.
- 2026-10-31: Plan approved; split into this folder on branch `fix/review-findings`.
- 2026-10-31: Committed the in-progress UI cleanup + AGENTS.md test-rule removal (a085e2d) so streams share one base.
- 2026-10-31: Ran 5 streams in parallel worktrees (`../makora-wt/*`), grouped to minimise file overlap; shared cargo target dir to avoid 5 llama.cpp builds.
  - tracking (01, 02, 03, 05, wlr clippy) → PR #9
  - ai (06, 07, 08, 10, 11.2) → PR #8
  - reporting (09, 11.1, 11.3, 11.4 casts) → PR #7
  - frontend (12, 13; 13.8 already done) → PR #6
  - ingest (04) → PR #10
- 2026-10-31: Rebased into one stack #9 → #8 → #7 → #6 → #10 to resolve cross-stream conflicts once; fix-ups: reporting `open_in_memory` for new `migrate(path)`, ts-rs derive for `SessionListQuery`, Option URLs in the reconciled focus machine, `shared::sync::lock` in the ingest listener.
- 2026-10-31: Skipped `cargo fmt` — the codebase is not rustfmt-formatted; reformatting would touch every file.
- Open: Windows/macOS code only type-checked (C deps need SDKs); manual checklists live in each PR.
