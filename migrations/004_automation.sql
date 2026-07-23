-- Automation rules, folder growth snapshots, and duplicate results.
--
-- Rules are stored disabled; `enabled` only becomes 1 through an explicit user
-- action. `consecutive_errors` backs the stop-after-repeated-failures guard so
-- a rule that keeps failing cannot run forever.

CREATE TABLE automation_rules (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    name                    TEXT NOT NULL,
    enabled                 INTEGER NOT NULL DEFAULT 0,
    trigger_kind            TEXT NOT NULL,
    action_kind             TEXT NOT NULL,
    weekday                 INTEGER NOT NULL DEFAULT 0,
    free_space_threshold    INTEGER NOT NULL DEFAULT 0,
    growth_threshold        INTEGER NOT NULL DEFAULT 0,
    watched_path            TEXT,
    categories              TEXT NOT NULL DEFAULT '[]',
    minimum_age_days        INTEGER NOT NULL DEFAULT 14,
    last_run                INTEGER,
    consecutive_errors      INTEGER NOT NULL DEFAULT 0,
    created_at              INTEGER NOT NULL
);

CREATE TABLE automation_runs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id         INTEGER NOT NULL REFERENCES automation_rules (id) ON DELETE CASCADE,
    ran_at          INTEGER NOT NULL,
    outcome         TEXT NOT NULL,
    detail          TEXT NOT NULL DEFAULT '',
    recovered_bytes INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_automation_runs_rule ON automation_runs (rule_id, ran_at DESC);

-- Periodic folder sizes. Growth is derived by differencing these rather than
-- by watching every file change.
CREATE TABLE folder_snapshots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL,
    taken_at    INTEGER NOT NULL,
    bytes       INTEGER NOT NULL DEFAULT 0,
    UNIQUE (path, taken_at)
);

CREATE INDEX idx_folder_snapshots_path ON folder_snapshots (path, taken_at DESC);
