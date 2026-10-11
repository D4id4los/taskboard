-- 0003_outbox_board_id: board-scoped outbox ops get their own column.
-- `rename_board` rows previously rode `stack_id` with a BoardId↔StackId
-- cast in the codec (a WI-3 workaround, closed by backlog WI-A); the
-- backfill moves the id into `board_id` and clears `stack_id` so every
-- op kind addresses exactly the columns its tag names.
ALTER TABLE outbox ADD COLUMN board_id TEXT NULL;
UPDATE outbox
   SET board_id = stack_id,
       stack_id = NULL
 WHERE op_kind = 'rename_board';
