# 11-backend-types-cleanup

1. `KeyKind::from_db` (`source_key.rs:20`) → `TryFrom<&str>`, error on unknown; row models' `kind`/`field` parsed to enums in repos.
2. `LlmSpec.provider: Option<String>` → `Option<ProviderId>` (reuse `shared/ids.rs`).
3. Remove `#[allow(dead_code)] CategorySvc::get` (`taxonomy/application.rs:16`) and its impl.
4. Fix default clippy warning (`wlr.rs:205` `chunks_exact` → `as_chunks`); pedantic: fix the lossy casts flagged (`usize→i32/i64`) with `try_from`.
Verify: `cargo clippy --all-targets` clean.
