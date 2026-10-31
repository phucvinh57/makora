# 06-concurrency-and-locks

1. **Poisoned mutexes**: add `shared::sync::lock(&Mutex<T>)` recovering via `into_inner` + `warn!`; use it at `lib.rs:175,178,206`, `db/mod.rs:46`, `macos.rs`; `ModelCache::lock` reuses it.
2. **ModelCache** (`ai/llm.rs:173-210`): load outside the lock. `Slot` gains `loading: HashSet<String>` + `Condvar`; `acquire` marks loading, drops guard, loads, relocks, inserts, notifies; concurrent acquirers of the same name wait.
3. **`backend()` init race** (`llm.rs:116-124`): guard init with a `static INIT: Mutex<()>` around get/init/set.
4. **Main-thread commands**: mark blocking sync commands `#[tauri::command(async)]` (runs on worker): `models.rs` (`ai_model_is_loaded`, `unload`, `is_installed`, `list_installed`), `categorization.rs` rule create/update/delete, `taxonomy.rs` create/update/delete. Keyring calls in `providers.rs` → `tauri::async_runtime::spawn_blocking`.
5. Dedupe `RunGuard` (`ai_run.rs:83`, `model_download.rs:49`) into `shared/`.
Verify: clippy; manual — start AI run with a local 7B model, UI stays responsive while loading.
