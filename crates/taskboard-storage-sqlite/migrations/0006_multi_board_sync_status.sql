-- 0006_multi_board_sync_status: the SyncStatus singleton becomes a
-- per-board row (F16-22 three-layer widening, wave chunk 1); board
-- bindings gain their connector tag (connect-flow §4.5).
-- Convention notes: timestamps stay chrono TEXT (never ORDER BY);
-- connector attribution is NOT stored in sync_status (derived from the
-- board's binding at read — one source of truth); no triggers — the
-- backfill is plain SQL inside the file's transaction.

-- 1. Binding connector tag: NULL = the legacy default connector
--    (resolved at decode time in the repository). Pre-migration
--    rows keep their behavior bit-for-bit: the one binding belonged to
--    the one configured connector, which is the default.
ALTER TABLE boards ADD COLUMN connector TEXT NULL;

-- 2. Status rebuild: one row per bound board. A v0.1.x database has at
--    most one bound live board (the single-board invariant), so the
--    singleton's phase/last_success backfills that board's row; a
--    board-less singleton (standalone installs) is dropped — its
--    content is vacuous (the recorded, priced deviation from 0005's
--    bit-for-bit bar, which governs merge behavior, not status
--    cosmetics).
CREATE TABLE sync_status_new (
    board        TEXT PRIMARY KEY NOT NULL REFERENCES boards (id),
    phase        TEXT NOT NULL,
    last_error   TEXT NULL,
    last_success TEXT NULL
);
INSERT INTO sync_status_new (board, phase, last_error, last_success)
SELECT b.id, s.phase, s.last_error, s.last_success
  FROM boards b
  JOIN sync_status s ON s.id = 1
 WHERE b.remote_board_id IS NOT NULL AND b.deleted = 0;
DROP TABLE sync_status;
ALTER TABLE sync_status_new RENAME TO sync_status;
