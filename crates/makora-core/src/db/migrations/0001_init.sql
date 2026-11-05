CREATE TABLE categories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL,
    parent_id   INTEGER REFERENCES categories(id),
    description TEXT
);
CREATE UNIQUE INDEX idx_categories_name ON categories(COALESCE(parent_id, 0), name);
CREATE INDEX idx_categories_parent ON categories(parent_id);

CREATE TABLE activities (
    id            INTEGER PRIMARY KEY,
    key_kind      TEXT NOT NULL CHECK(key_kind IN ('exe','domain')),
    key           TEXT NOT NULL,
    url_key       TEXT,
    title         TEXT NOT NULL DEFAULT '',
    display_title TEXT NOT NULL DEFAULT '',
    app_name      TEXT NOT NULL DEFAULT '',
    url           TEXT,
    first_seen    TEXT NOT NULL,
    last_seen     TEXT NOT NULL,
    category_id   INTEGER REFERENCES categories(id)
);
CREATE UNIQUE INDEX idx_activities_identity
    ON activities(key_kind, key, COALESCE(url_key, ''), title);
CREATE INDEX idx_activities_key ON activities(key_kind, key);
CREATE INDEX idx_activities_category ON activities(category_id);

CREATE TABLE sessions (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    app_name          TEXT NOT NULL,
    exe_path          TEXT NOT NULL,
    window_title      TEXT NOT NULL,
    url               TEXT,
    category_key      TEXT NOT NULL DEFAULT '',
    category_key_kind TEXT NOT NULL DEFAULT 'exe',
    start_time        TEXT NOT NULL,
    end_time          TEXT,
    category_id       INTEGER REFERENCES categories(id),
    url_key           TEXT,
    activity_id       INTEGER REFERENCES activities(id)
);
CREATE INDEX idx_sessions_exe_path ON sessions(exe_path);
CREATE INDEX idx_sessions_start_time ON sessions(start_time);
CREATE INDEX idx_sessions_category_key ON sessions(category_key_kind, category_key);
CREATE INDEX idx_sessions_category ON sessions(category_id);
CREATE INDEX idx_sessions_activity ON sessions(activity_id);

CREATE TABLE work_sessions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    note           TEXT,
    target_seconds INTEGER NOT NULL,
    start_time     TEXT NOT NULL,
    end_time       TEXT NOT NULL,
    created_at     TEXT NOT NULL
);
CREATE INDEX idx_work_sessions_start_time ON work_sessions(start_time);
CREATE TABLE work_session_categories (
    work_session_id INTEGER NOT NULL REFERENCES work_sessions(id),
    category_id     INTEGER NOT NULL REFERENCES categories(id),
    PRIMARY KEY (work_session_id, category_id)
);
