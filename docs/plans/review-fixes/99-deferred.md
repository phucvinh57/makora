# 99-deferred

- Idle (no-input) detection on all platforms (new setting).
- App identity: real UWP app behind `ApplicationFrameHost.exe`; desktop app id for interpreted runtimes on Linux (changes source keys).
- Incremental rule rematch (per-rule re-resolve; `rule_matches` keyed by `activity_id`, index on `rule_id`) — 06 already moves it off the main thread.
