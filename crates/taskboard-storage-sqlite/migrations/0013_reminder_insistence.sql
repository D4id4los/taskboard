-- 0013_reminder_insistence: the insistence flavour columns (DR-3/2,
-- #475; design D-I4). Additive only (ADR 0005). Conventions as in
-- 0007/0012: stored TEXT is the domain serde unit name; unknown text
-- decodes Corrupted at the codec (no CHECK re-legislates the enum).
-- Numbering: the dispatch table assigned 0012, superseded by landing
-- order — 0012_task_label_memberships.sql landed first; 0013 is the
-- next free number at this chunk's base (dispatch §1 rule 3).
ALTER TABLE reminder_rules ADD COLUMN insistence TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE adhoc_alarms   ADD COLUMN insistence TEXT NOT NULL DEFAULT 'normal';
