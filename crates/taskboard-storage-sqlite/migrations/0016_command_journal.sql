-- 0016 — the undo command journal (ADR 0019, dispatch chunk undo/1).
-- Additive: two new tables, no rewrites, no down-migration. Stamps are
-- RFC-3339 TEXT carried verbatim; the convention forbids ORDER BY on
-- them (hydration orders by rowid = insertion = chronological).
CREATE TABLE command_journal (
    id         TEXT PRIMARY KEY NOT NULL,
    parent     TEXT,
    created_at TEXT NOT NULL,
    pushed     INTEGER NOT NULL DEFAULT 0,
    board      TEXT,
    -- serde_json of { command, facts } — the scalar columns stay
    -- queryable, the payload rides one JSON blob (D13).
    payload    TEXT NOT NULL
);

-- The persisted line head (R-U10: it moves on undo/redo without a new
-- node, so it cannot be derived). Exactly one row; NULL = empty line.
CREATE TABLE command_journal_head (
    id TEXT
);
INSERT INTO command_journal_head (id) VALUES (NULL);
