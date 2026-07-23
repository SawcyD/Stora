-- Category totals are accumulated during the scan rather than derived by
-- grouping `scan_entries`.
--
-- This is what lets Stora stop storing a row for every small file: the only
-- queries that needed them were the category breakdown (now precomputed here)
-- and the large-file list (which has a size floor). A full drive previously
-- produced hundreds of megabytes of records for data no view ever read.

CREATE TABLE scan_categories (
    scan_id     INTEGER NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    category    TEXT NOT NULL,
    bytes       INTEGER NOT NULL DEFAULT 0,
    file_count  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (scan_id, category)
);
