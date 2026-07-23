-- Stora initial schema.
--
-- Design notes:
--  * `scan_entries` holds individual file records and is the largest table by
--    far; `folder_aggregates` stores pre-rolled folder totals so the tree and
--    breakdown views never scan it.
--  * Paths are stored in their readable form (no `\\?\` prefix), normalized by
--    stora-security before they reach here.
--  * File contents are never stored — only metadata.

CREATE TABLE drives (
    root            TEXT PRIMARY KEY,
    label           TEXT NOT NULL DEFAULT '',
    filesystem      TEXT NOT NULL DEFAULT '',
    total_bytes     INTEGER NOT NULL DEFAULT 0,
    free_bytes      INTEGER NOT NULL DEFAULT 0,
    drive_type      TEXT NOT NULL DEFAULT 'unknown',
    is_removable    INTEGER NOT NULL DEFAULT 0,
    last_seen       INTEGER NOT NULL
);

CREATE TABLE scans (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    root            TEXT NOT NULL,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    duration_ms     INTEGER NOT NULL DEFAULT 0,
    files_scanned   INTEGER NOT NULL DEFAULT 0,
    folders_scanned INTEGER NOT NULL DEFAULT 0,
    bytes_analyzed  INTEGER NOT NULL DEFAULT 0,
    error_count     INTEGER NOT NULL DEFAULT 0,
    state           TEXT NOT NULL DEFAULT 'preparing'
);

CREATE INDEX idx_scans_root_started ON scans (root, started_at DESC);

CREATE TABLE scan_entries (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id         INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    parent_path     TEXT NOT NULL,
    name            TEXT NOT NULL,
    extension       TEXT,
    logical_size    INTEGER NOT NULL DEFAULT 0,
    allocated_size  INTEGER NOT NULL DEFAULT 0,
    created         INTEGER,
    modified        INTEGER,
    accessed        INTEGER,
    attributes      INTEGER NOT NULL DEFAULT 0,
    is_directory    INTEGER NOT NULL DEFAULT 0,
    is_reparse      INTEGER NOT NULL DEFAULT 0,
    category        TEXT NOT NULL DEFAULT 'other'
);

CREATE INDEX idx_entries_scan_parent ON scan_entries (scan_id, parent_path);
CREATE INDEX idx_entries_scan_size ON scan_entries (scan_id, logical_size DESC);
CREATE INDEX idx_entries_scan_ext ON scan_entries (scan_id, extension);
CREATE INDEX idx_entries_scan_category ON scan_entries (scan_id, category);
CREATE INDEX idx_entries_path ON scan_entries (path);

CREATE TABLE folder_aggregates (
    scan_id         INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    parent_path     TEXT,
    name            TEXT NOT NULL,
    logical_size    INTEGER NOT NULL DEFAULT 0,
    allocated_size  INTEGER NOT NULL DEFAULT 0,
    file_count      INTEGER NOT NULL DEFAULT 0,
    folder_count    INTEGER NOT NULL DEFAULT 0,
    modified        INTEGER,
    PRIMARY KEY (scan_id, path)
);

CREATE INDEX idx_aggregates_parent ON folder_aggregates (scan_id, parent_path);
CREATE INDEX idx_aggregates_size ON folder_aggregates (scan_id, allocated_size DESC);

CREATE TABLE scan_errors (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id         INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    code            TEXT NOT NULL,
    message         TEXT NOT NULL
);

CREATE INDEX idx_scan_errors_scan ON scan_errors (scan_id);

CREATE TABLE cleanup_operations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id         TEXT NOT NULL,
    started_at      INTEGER NOT NULL,
    duration_ms     INTEGER NOT NULL DEFAULT 0,
    method          TEXT NOT NULL,
    files_selected  INTEGER NOT NULL DEFAULT 0,
    files_removed   INTEGER NOT NULL DEFAULT 0,
    files_skipped   INTEGER NOT NULL DEFAULT 0,
    recovered_bytes INTEGER NOT NULL DEFAULT 0,
    error_count     INTEGER NOT NULL DEFAULT 0,
    state           TEXT NOT NULL,
    categories      TEXT NOT NULL DEFAULT '[]',
    automation_rule TEXT
);

CREATE INDEX idx_cleanup_started ON cleanup_operations (started_at DESC);

CREATE TABLE cleanup_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id    INTEGER NOT NULL REFERENCES cleanup_operations (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    category_id     TEXT NOT NULL,
    size            INTEGER NOT NULL DEFAULT 0,
    removed         INTEGER NOT NULL DEFAULT 0,
    error_code      TEXT,
    error_message   TEXT
);

CREATE INDEX idx_cleanup_items_operation ON cleanup_items (operation_id);

CREATE TABLE quarantine_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id    INTEGER REFERENCES cleanup_operations (id) ON DELETE SET NULL,
    original_path   TEXT NOT NULL,
    quarantine_path TEXT NOT NULL,
    size            INTEGER NOT NULL DEFAULT 0,
    quarantined_at  INTEGER NOT NULL,
    expires_at      INTEGER,
    restored        INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_quarantine_expires ON quarantine_items (expires_at);

CREATE TABLE exclusions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern         TEXT NOT NULL,
    kind            TEXT NOT NULL,
    reason          TEXT NOT NULL DEFAULT 'userExclusion',
    created_at      INTEGER NOT NULL,
    UNIQUE (pattern, kind)
);

CREATE TABLE settings (
    key             TEXT PRIMARY KEY,
    value           TEXT NOT NULL
);
