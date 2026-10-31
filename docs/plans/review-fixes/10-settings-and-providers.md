# 10-settings-and-providers

Files: `settings.rs`, `host/commands/settings.rs`, `host/commands/providers.rs`.
1. Malformed settings file (`settings.rs:122-125`): rename it to `settings.toml.corrupt-<ts>`, `error!`, load defaults — user data never silently overwritten. Emit an event the UI shows as a toast.
2. `set_llm` provider-exists check moves inside the `update` write lock.
3. Provider save: set keyring key first, then settings; on settings failure delete the key.
4. `settings_remove_exclusion` parses via `Exclusion::parse` like add; range/model-name validation lives only in `settings.rs` (remove copies in commands).
Tests: load with malformed file → backup created, defaults returned.
