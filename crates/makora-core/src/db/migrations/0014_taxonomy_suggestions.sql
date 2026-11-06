-- Activities the model answered "None" for: no category fits. Kept apart from `ai_labels`, which
-- only holds categories the model picked, so a "None" answer can be told from a never-asked
-- activity. A row goes when the activity gets a pin or an AI label, or is deleted.
CREATE TABLE ai_unlabeled (
    activity_id INTEGER PRIMARY KEY REFERENCES activities(id) ON DELETE CASCADE,
    model       TEXT NOT NULL,
    answered_at TEXT NOT NULL
);

-- Changes to the taxonomy proposed from the data: a `new` category for a cluster of unlabeled
-- activities, a sharper description for one the model keeps getting wrong (`describe`), or two
-- categories to `merge`. `payload` is JSON whose shape depends on `kind`. `decided_at` is when it
-- was accepted or dismissed.
CREATE TABLE taxonomy_suggestions (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('new', 'describe', 'merge')),
    payload    TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'dismissed')),
    created_at TEXT NOT NULL,
    decided_at TEXT,
    run_id     INTEGER REFERENCES ai_runs(id) ON DELETE SET NULL
);
CREATE INDEX idx_taxonomy_suggestions_status ON taxonomy_suggestions(status, kind);
