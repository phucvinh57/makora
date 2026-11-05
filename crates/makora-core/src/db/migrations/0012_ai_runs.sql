-- One row per AI categorization run that had something to label: what started it, the model, how
-- it ended, and what it cost. `provider` is NULL for the local model. Times are in milliseconds;
-- `busy_ms` sums request latencies, so with requests in flight at once it can exceed `llm_ms`.
CREATE TABLE ai_runs (
    id                  INTEGER PRIMARY KEY,
    triggered_by        TEXT NOT NULL CHECK (triggered_by IN ('manual', 'scheduled')),
    provider            TEXT,
    model               TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (status IN ('completed', 'stopped', 'failed')),
    error               TEXT,
    started_at          TEXT NOT NULL,
    finished_at         TEXT NOT NULL,
    items               INTEGER NOT NULL,
    labeled             INTEGER NOT NULL,
    requests            INTEGER NOT NULL,
    failed_requests     INTEGER NOT NULL,
    input_tokens        INTEGER NOT NULL,
    cached_input_tokens INTEGER NOT NULL,
    output_tokens       INTEGER NOT NULL,
    llm_ms              INTEGER NOT NULL,
    busy_ms             INTEGER NOT NULL,
    prefill_ms          INTEGER NOT NULL,
    decode_ms           INTEGER NOT NULL,
    embed_count         INTEGER NOT NULL,
    embed_ms            INTEGER NOT NULL
);

CREATE INDEX ai_runs_started_at ON ai_runs (started_at);
