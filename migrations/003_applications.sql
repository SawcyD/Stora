-- Application discovery and locally observed activity.
--
-- Activity is recorded only when the user turns tracking on, and never leaves
-- the device. `launch_count` and `last_observed` describe what Stora actually
-- witnessed; an application with no row here is reported as having no reliable
-- activity data rather than being assumed unused.

CREATE TABLE applications (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    publisher           TEXT NOT NULL DEFAULT '',
    version             TEXT NOT NULL DEFAULT '',
    reported_bytes      INTEGER,
    detected_bytes      INTEGER,
    install_location    TEXT,
    install_date        INTEGER,
    app_type            TEXT NOT NULL DEFAULT 'unknown',
    uninstall_command   TEXT,
    source              TEXT NOT NULL DEFAULT '',
    confidence          TEXT NOT NULL DEFAULT 'unknown',
    suggestable         INTEGER NOT NULL DEFAULT 1,
    last_seen           INTEGER NOT NULL
);

CREATE INDEX idx_applications_name ON applications (name);

CREATE TABLE application_paths (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id  TEXT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    relationship    TEXT NOT NULL,
    bytes           INTEGER NOT NULL DEFAULT 0,
    confidence      TEXT NOT NULL DEFAULT 'unknown',
    reason          TEXT NOT NULL DEFAULT '',
    UNIQUE (application_id, path)
);

CREATE INDEX idx_application_paths_app ON application_paths (application_id);

CREATE TABLE application_activity (
    executable_path TEXT PRIMARY KEY,
    executable_name TEXT NOT NULL,
    first_observed  INTEGER NOT NULL,
    last_observed   INTEGER NOT NULL,
    launch_count    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_activity_last_observed ON application_activity (last_observed);
