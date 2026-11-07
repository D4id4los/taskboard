-- F1's author-tagged clocks (boards-over-files chunk 1, D-BF2, ADR 0022):
-- one nullable `au_<field>` TEXT author column per `ck_<field>` clock
-- column on the four roaming entity tables — 23 columns, no data
-- rewrite. NULL = "no author": every pre-F1 row backfills as absent,
-- and the clock columns themselves are untouched (the author lives in
-- its own column, the storage realization of the `rfc3339 [# author]`
-- text-form grammar; NULL ↔ absent suffix are the same state, the
-- `RemoteTaskBaseline` NULL-column precedent). The author value is the
-- bare lowercase-hyphenated UUID (`AuthorId`'s display form).
--
-- Deliberately not migrated: the device-local clock families (journal
-- stamps, habit logs, reflection — ADR 0021) and the sidecar (JSON,
-- serde-default tolerant, no migration).
ALTER TABLE tasks ADD COLUMN au_title TEXT;
ALTER TABLE tasks ADD COLUMN au_description TEXT;
ALTER TABLE tasks ADD COLUMN au_duedate TEXT;
ALTER TABLE tasks ADD COLUMN au_scheduled TEXT;
ALTER TABLE tasks ADD COLUMN au_done TEXT;
ALTER TABLE tasks ADD COLUMN au_position TEXT;
ALTER TABLE tasks ADD COLUMN au_effort TEXT;
ALTER TABLE tasks ADD COLUMN au_importance TEXT;
ALTER TABLE tasks ADD COLUMN au_urgency TEXT;
ALTER TABLE tasks ADD COLUMN au_recur TEXT;
ALTER TABLE tasks ADD COLUMN au_kind TEXT;
ALTER TABLE tasks ADD COLUMN au_archived TEXT;
ALTER TABLE tasks ADD COLUMN au_deleted TEXT;
ALTER TABLE stacks ADD COLUMN au_title TEXT;
ALTER TABLE stacks ADD COLUMN au_sort_order TEXT;
ALTER TABLE stacks ADD COLUMN au_deleted TEXT;
ALTER TABLE labels ADD COLUMN au_title TEXT;
ALTER TABLE labels ADD COLUMN au_color TEXT;
ALTER TABLE labels ADD COLUMN au_deleted TEXT;
ALTER TABLE boards ADD COLUMN au_title TEXT;
ALTER TABLE boards ADD COLUMN au_color TEXT;
ALTER TABLE boards ADD COLUMN au_archived TEXT;
ALTER TABLE boards ADD COLUMN au_deleted TEXT;
