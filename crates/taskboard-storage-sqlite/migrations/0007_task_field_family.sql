-- 0007_task_field_family: the Wave-1 domain field family (#208, #209)
-- plus the reminder-rule write clock doc 1 D-S9 reserved (chunk 4 wires
-- the domain field; until then the column sits at its default).
-- Conventions: value columns NULL = unset (0001's duedate precedent);
-- clock columns epoch-defaulted NOT NULL (0005's mechanism) so
-- pre-migration rows read as "never locally written" — every remote
-- stamp / document clock wins bit-for-bit. Additive only (ADR 0005).
ALTER TABLE tasks ADD COLUMN scheduled     TEXT NULL;    -- rfc3339, duedate's twin
ALTER TABLE tasks ADD COLUMN effort        INTEGER NULL; -- Effort seconds (u64, codec-checked ≤ i64::MAX)
ALTER TABLE tasks ADD COLUMN importance    TEXT NULL;    -- serde unit names, five levels
ALTER TABLE tasks ADD COLUMN urgency       TEXT NULL;    -- serde unit names, five levels
ALTER TABLE tasks ADD COLUMN ck_scheduled  TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_effort     TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_importance TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_urgency    TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE reminder_rules ADD COLUMN updated_at TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
