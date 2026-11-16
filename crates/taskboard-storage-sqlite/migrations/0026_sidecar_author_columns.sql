-- The stored sidecar author slot (#708 Axis A; ADR 0022's fence
-- amendment, ADR 0021's device-locality unchanged): one nullable
-- `au_by` TEXT author column per roaming sidecar table —
-- `reminder_rules`, `break_rules`, `time_entries`, `habits`,
-- `habit_log` — 0023's columnar precedent, no data rewrite. NULL =
-- "author unknown": every pre-slot row backfills as absent, and the
-- clock columns themselves are untouched (the author lives in its own
-- column; NULL ↔ absent are the same state, so a mid-upgrade reader
-- never changes fold rank). The author value is the bare lowercase-
-- hyphenated UUID (`AuthorId`'s display form).
--
-- Deliberately not migrated: the device-local families (journal
-- stamps, reflection — ADR 0021) and the document lane itself (JSON,
-- serde-default tolerant, no migration).
ALTER TABLE reminder_rules ADD COLUMN au_by TEXT;
ALTER TABLE break_rules ADD COLUMN au_by TEXT;
ALTER TABLE time_entries ADD COLUMN au_by TEXT;
ALTER TABLE habits ADD COLUMN au_by TEXT;
ALTER TABLE habit_log ADD COLUMN au_by TEXT;
