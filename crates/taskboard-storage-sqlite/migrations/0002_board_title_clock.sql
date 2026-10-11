-- 0002_board_title_clock: the `boards rename` command (WI-3) makes the
-- board title locally writable, so the board gains its `ck_title` field
-- clock (per-field LWW, ADR 0004 amendment).
--
-- The epoch default means "never locally written": every pre-migration
-- row keeps today's remote-wins merge behavior bit-for-bit (a remote
-- stamp strictly newer than `remote_seen` adopts, exactly as before).
ALTER TABLE boards ADD COLUMN ck_title TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
