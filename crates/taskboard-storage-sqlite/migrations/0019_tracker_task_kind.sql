-- 0019_tracker_task_kind: the tracker task kind (backlog round 2,
-- chunk 4.2, issue #492; design D-1). Additive only (ADR 0005), the
-- 0018 recipe verbatim over the 0007 column pair.
--
-- The kind column's DEFAULT 'standard' backfill IS the default-off
-- decode of pre-0019 local data: every existing row reads `Standard`.
-- Stored TEXT is the domain serde unit name; unknown text decodes
-- Corrupted at the codec (local rows are trusted-written — a corrupt
-- enum is a corrupted database, no CHECK re-legislates the enum).
-- The clock column is epoch-defaulted NOT NULL (0007's mechanism) so
-- pre-migration rows read as "never locally written" — every remote
-- stamp / document clock wins bit-for-bit.
ALTER TABLE tasks ADD COLUMN kind    TEXT NOT NULL DEFAULT 'standard';
ALTER TABLE tasks ADD COLUMN ck_kind TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
