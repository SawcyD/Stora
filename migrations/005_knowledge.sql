-- Curated location knowledge.
--
-- The authoritative copy is the checked-in JSON in `stora-knowledge`, which is
-- embedded in the binary. This table is a seeded mirror so the entries can be
-- queried alongside scan results, and so a user-supplied note can sit beside a
-- curated one later without changing the shipped file.
--
-- `seeded` marks rows that came from the JSON: they are replaced wholesale on
-- every startup, so editing one here has no lasting effect.

CREATE TABLE knowledge_entries (
    id              TEXT PRIMARY KEY,
    pattern         TEXT NOT NULL,
    title           TEXT NOT NULL,
    written_by      TEXT NOT NULL,
    if_removed      TEXT NOT NULL,
    removable       INTEGER NOT NULL DEFAULT 0,
    source_title    TEXT NOT NULL DEFAULT '',
    source_url      TEXT NOT NULL DEFAULT '',
    seeded          INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX idx_knowledge_pattern ON knowledge_entries (pattern);
