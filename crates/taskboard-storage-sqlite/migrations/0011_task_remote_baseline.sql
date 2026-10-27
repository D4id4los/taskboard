-- 0011_task_remote_baseline: the observed remote card snapshot per
-- task (DR-4/G, #128) — merge_task's changed-field diff reference
-- (ADR 0004's remote baseline).
-- Conventions: NULL = never observed (pre-migration rows and local
-- creates) ⇒ the merge falls back to the blanket per-field LWW for
-- that cycle and establishes the record. The TEXT is the serde JSON
-- of the domain's RemoteTaskBaseline (the recur column's precedent).
-- Additive only (ADR 0005).
ALTER TABLE tasks ADD COLUMN remote_baseline TEXT NULL; -- serde JSON of RemoteTaskBaseline
