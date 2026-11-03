-- 0020_reminder_rule_kind: the reminder rule kind (GUI review round 4,
-- chunk 8; design D29 — at/in reminder kind). Additive only (ADR 0005),
-- the 0019 recipe verbatim.
--
-- The kind column's DEFAULT 'due_offset' backfill IS the legacy decode:
-- every existing row reads `DueOffset` (upgrade pin, no loss). Stored
-- TEXT is the domain serde unit name; unknown text decodes Corrupted at
-- the codec (local rows are trusted-written — a corrupt enum is a
-- corrupted database, no CHECK re-legislates the enum).
-- `at_epoch_seconds` is deliberately nullable: NULL ⇔ a due-offset row;
-- NOT NULL (epoch seconds) iff kind = 'at'. NULL is honest absence, not
-- a lost race — the time_entries `end` precedent, not the epoch-default
-- trick.
ALTER TABLE reminder_rules ADD COLUMN kind TEXT NOT NULL DEFAULT 'due_offset';
ALTER TABLE reminder_rules ADD COLUMN at_epoch_seconds INTEGER;
