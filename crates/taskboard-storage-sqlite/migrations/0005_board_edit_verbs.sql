-- 0005_board_edit_verbs: the board edit verbs (`boards color`/`archive`,
-- `boards delete`) complete `BoardClocks` (chunk 15 of remediation wave 1,
-- ADR 0004 2026-10-18 amendment), and the board edit op is renamed to tell
-- the full-send truth.
--
-- The epoch defaults mean "never locally written": pre-migration rows keep
-- the remote-wins merge behavior bit-for-bit (0002's mechanism).
ALTER TABLE boards ADD COLUMN ck_color TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE boards ADD COLUMN ck_archived TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE boards ADD COLUMN ck_deleted TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
-- The op's push has full-sent title + color + archived since its
-- introduction; `update_board` is the honest tag (0003's backfill
-- precedent — one representation, no alias drift; decode rejects the
-- legacy tag after this migration).
UPDATE outbox SET op_kind = 'update_board' WHERE op_kind = 'rename_board';
