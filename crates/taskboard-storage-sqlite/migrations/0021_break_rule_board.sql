-- 0021_break_rule_board: break rules become board-scoped roaming rows
-- (#588, round 3 chunk 1.2; design D-2/D-14). Additive only (ADR 0005).
-- The backfill hands pre-0021 device-local rows to the device's sole
-- bound board: the fleet this feature has existed on (it landed
-- 2026-11-02) is single-board by deployment. A zero-board device's
-- rows keep '' and fail the codec's UUID parse (trusted-written
-- local rows: a corrupt id is a corrupted database, the 0008 rule) —
-- recorded leniency, not a silent path: such rows can only exist on
-- a device that never bound a board, which cannot have flushed them.
-- The EXISTS guard keeps those rows at '' instead of writing the
-- subselect's NULL into the NOT NULL column.
ALTER TABLE break_rules ADD COLUMN board_id TEXT NOT NULL DEFAULT '';

UPDATE break_rules
   SET board_id = (SELECT id FROM boards WHERE deleted = 0 ORDER BY id LIMIT 1)
 WHERE board_id = ''
   AND EXISTS (SELECT 1 FROM boards WHERE deleted = 0);
