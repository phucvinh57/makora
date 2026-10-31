# 01-db-migration-safety

Files: `src-tauri/src/db/migrate.rs`, `db/mod.rs`.
1. **Backup before pending migrations**: in `migrate()`, if `user_version < LATEST` and DB has tables, `VACUUM INTO '<db>.bak-v{from}-{yyyymmddHHMMSS}'` before `run_pending`; keep newest 3 backups, delete older. Needs db path passed into `migrate` (from `open_sqlite`, `lib.rs:115`).
2. **Legacy-version collision** (`LEGACY_SCHEMA_VERSION = 28`): set `PRAGMA application_id = <MAKORA const>` at the end of every successful migrate; treat as legacy only when `user_version == 28 && application_id != MAKORA`. Pure fn `classify_db(version, app_id) -> DbState` + tests.
3. **FK check**: after `run_pending`, `PRAGMA foreign_key_check`; any row → error (backup exists) instead of silently continuing.
4. Fix stale doc in `db/schema.rs:1` ("after migration 9").
Verify: tests for `classify_db`; run app on a copy of a v10 DB → backup file created, app opens.
