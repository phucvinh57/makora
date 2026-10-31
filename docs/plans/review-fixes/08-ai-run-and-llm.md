# 08-ai-run-and-llm

1. **Embedding failure doesn't abort runs** (`ai_run.rs:142`): `embed_missing` error → `warn!`, run continues with no related examples. `embed_missing` checks `cancel` between batches. A batch that fails → retry items one by one; persistently failing ids go into an in-memory skip set (indexer + run) so one bad text can't block forever.
2. **Save failure surfaces** (`ai_run.rs:178-183`): capture the `assign_ai` error, stop, return `Err` after `classify`.
3. **Category names validated at the boundary** (`taxonomy/application.rs` create/update): `single_line` + trim, reject empty and names containing `" > "` (path separator) → `invalid` error. `by_path` keyed by `single_line(path)` (`llm.rs:451`) matching the grammar (`llm.rs:369`).
4. `MAX_REPLY_TOKENS`: compute from longest tokenized path + margin instead of fixed 48.
5. **Special-token injection**: embeddings tokenize with `parse_special=false` (`embedding.rs:85`); LLM: strip the model's control-token texts (enumerated once per model from vocab) from untrusted fields (titles/urls/names) before templating.
6. Remote `base_url`: require `https` unless host is loopback/private (validated in provider save).
7. Dedupe thread-count calc (`llm.rs:429`, `embedding.rs:61`) into one fn.
8. `ai/gguf.rs:42-57`: `checked_mul`/`i64::try_from` and a nesting-depth limit → error on corrupt files.
Tests: name normalization, path lookup, sanitize fn, gguf skip on crafted bytes.
