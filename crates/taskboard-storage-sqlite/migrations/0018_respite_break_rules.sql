-- 0018_respite_break_rules: the respite entry kind and the break-rule
-- table (backlog round 2, chunk 3.5, issues #493 + #495; design
-- D-12/D-13). Additive only (ADR 0005), the 0013 precedent verbatim.
--
-- The kind column's DEFAULT 'work' backfill IS the default-off decode
-- of pre-0018 local data: every existing row reads `Work`. Stored TEXT
-- is the domain serde unit name; unknown text decodes Corrupted at the
-- codec (local rows are trusted-written — a corrupt enum is a
-- corrupted database, no CHECK re-legislates the enum).
--
-- `break_rules` follows the 0008 adhoc-alarms conventions: hyphenated
-- UUID TEXT id, INTEGER seconds (the WorkThreshold newtype guarantees
-- positivity; no CHECK re-legislates it), rfc3339 TEXT stamps, no
-- foreign keys, no extra indexes (the primary key is the lookup path
-- at kiosk scale). Device-local rows by ruling (D-5) — like alarms,
-- they never roam, so no board column and no sync participation.
ALTER TABLE time_entries ADD COLUMN kind TEXT NOT NULL DEFAULT 'work';

CREATE TABLE break_rules (
    rule_id           TEXT PRIMARY KEY NOT NULL,
    label             TEXT NOT NULL,
    threshold_seconds INTEGER NOT NULL,
    insistence        TEXT NOT NULL DEFAULT 'normal',
    created_at        TEXT NOT NULL
);
