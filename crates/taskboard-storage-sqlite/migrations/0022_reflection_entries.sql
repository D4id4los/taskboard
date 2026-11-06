-- DR-7 D-J13 (#637): the device-local reflection store (ADR 0021).
-- The 0008/0009/0015 recipe: hyphenated-UUID TEXT ids, RFC 3339 TEXT
-- stamps, no foreign keys (nothing references a reflection entry, and
-- entries reference no task). `deleted` is the in-place tombstone —
-- the row *is* the delete; there is no row-absence path. `flags` /
-- `extra_flags` are JSON arrays of kebab names (the parked-unknown
-- decode tolerance lives one layer up, in the load codec).
CREATE TABLE reflection_entries (
    entry_id     TEXT PRIMARY KEY NOT NULL,  -- hyphenated UUID
    day          TEXT NOT NULL,              -- ISO date
    cadence      TEXT NOT NULL,              -- "day" | "week" | "month"
    window_start TEXT NOT NULL,              -- RFC 3339
    window_end   TEXT NOT NULL,              -- RFC 3339
    text         TEXT NOT NULL,
    mood         INTEGER,                    -- NULL = unset
    energy       INTEGER,                    -- NULL = unset
    flags        TEXT NOT NULL,              -- JSON array of known names
    extra_flags  TEXT NOT NULL,              -- JSON array of parked unknown names
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted      INTEGER NOT NULL            -- the in-place tombstone
);
