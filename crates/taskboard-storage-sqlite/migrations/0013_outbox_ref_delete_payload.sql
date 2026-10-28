-- 0013_outbox_ref_delete_payload: the crash-window duplicate collapse
-- (#150, chunk DR-4/R) enqueues delete ops whose target is a remote
-- (board, stack, card) triple — remote u64 ids no local-id column can
-- carry. The ref rides a JSON payload column next to the op tag;
-- board_id keeps the local attribution board. Additive per ADR 0005:
-- NULL for every pre-existing row.
ALTER TABLE outbox ADD COLUMN payload TEXT NULL;
