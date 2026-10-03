-- One embedding per activity, from the model named in `model`: a vector of little-endian f32s.
-- They are what lets the AI be shown the activities most like the one it is labeling. A vector
-- from a model other than the configured one is stale and gets replaced; deleting an activity
-- takes its vector with it.
CREATE TABLE activity_embeddings (
    activity_id INTEGER PRIMARY KEY REFERENCES activities(id) ON DELETE CASCADE,
    model       TEXT NOT NULL,
    vector      BLOB NOT NULL
);
