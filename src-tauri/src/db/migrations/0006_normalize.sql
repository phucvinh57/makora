-- Normalizes to 3NF: `sources` pulls the app/site identity out of `activities` (it depended on
-- key_kind+key, not on the activity); `path` replaces `url`/`url_key` with the one fact that
-- isn't already in the source's key; `display_title` is dropped since it never held more than
-- the source name and URL already give; AI suggestion and rejection move to `ai_labels`, one
-- row per activity, since the two were never set at once; and `sessions` keeps only the
-- timestamps, since every other column it had was copied from its activity.
CREATE TABLE sources (
    id   INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('exe','domain')),
    key  TEXT NOT NULL,
    name TEXT NOT NULL,
    UNIQUE(kind, key)
);
INSERT INTO sources (kind, key, name)
SELECT kind, key, name FROM (
    SELECT key_kind AS kind, key,
           CASE WHEN key_kind = 'domain' THEN key ELSE app_name END AS name,
           MAX(last_seen)
    FROM activities
    GROUP BY key_kind, key
);

UPDATE activities SET category_id = (
    SELECT category_id FROM sessions
    WHERE sessions.activity_id = activities.id AND category_id IS NOT NULL
    ORDER BY start_time DESC LIMIT 1
)
WHERE category_id IS NULL AND EXISTS (
    SELECT 1 FROM sessions
    WHERE sessions.activity_id = activities.id AND category_id IS NOT NULL
);

CREATE TABLE activities_new (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES sources(id),
    path        TEXT NOT NULL DEFAULT '',
    title       TEXT NOT NULL DEFAULT '',
    category_id INTEGER REFERENCES categories(id)
);
INSERT INTO activities_new (id, source_id, path, title, category_id)
SELECT a.id, src.id,
       CASE WHEN a.url_key IS NOT NULL
                 AND substr(a.url_key, 1, length(src.key)) = src.key
            THEN substr(a.url_key, length(src.key) + 1)
            ELSE '' END,
       a.title, a.category_id
FROM activities a
JOIN sources src ON src.kind = a.key_kind AND src.key = a.key;

CREATE TABLE ai_labels (
    activity_id INTEGER PRIMARY KEY REFERENCES activities(id),
    category_id INTEGER NOT NULL REFERENCES categories(id),
    confidence  REAL,
    rejected    INTEGER NOT NULL DEFAULT 0 CHECK(rejected IN (0, 1))
);
INSERT INTO ai_labels (activity_id, category_id, confidence, rejected)
SELECT id, COALESCE(ai_category_id, ai_rejected_category_id), ai_confidence,
       CASE WHEN ai_category_id IS NULL THEN 1 ELSE 0 END
FROM activities
WHERE ai_category_id IS NOT NULL OR ai_rejected_category_id IS NOT NULL;

CREATE TABLE sessions_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id INTEGER NOT NULL REFERENCES activities(id),
    start_time  TEXT NOT NULL,
    end_time    TEXT
);
INSERT INTO sessions_new (id, activity_id, start_time, end_time)
SELECT id, activity_id, start_time, end_time FROM sessions
WHERE activity_id IS NOT NULL;

DROP TABLE sessions;
DROP TABLE activities;
ALTER TABLE sessions_new RENAME TO sessions;
ALTER TABLE activities_new RENAME TO activities;

CREATE UNIQUE INDEX idx_activities_identity ON activities(source_id, path, title);
CREATE INDEX idx_activities_category ON activities(category_id);
CREATE INDEX idx_ai_labels_category ON ai_labels(category_id);
CREATE INDEX idx_sessions_start_time ON sessions(start_time);
CREATE INDEX idx_sessions_activity ON sessions(activity_id);
