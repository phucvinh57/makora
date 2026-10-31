# 07-model-download

File: `ai/model_download.rs` (ureq 3.4.2; reuse agent-config pattern from `ai/remote/http.rs:41-46`).
1. Shared `Agent` with `timeout_resolve/connect/recv_response` (30 s).
2. Stall bound: fetch blob in **ranged chunks** (64 MiB, `timeout_global` 5 min per chunk) → stalls error out, cancel checked per read and per chunk; `running` always cleared (RunGuard).
3. `.partial` already complete (`have == total`): skip request, go straight to hash verify + rename (no 416).
4. Manifest write atomic (tmp + rename).
Verify: pure test for chunk-range planning; manual: download, kill network mid-way → error within timeout, retry resumes.
