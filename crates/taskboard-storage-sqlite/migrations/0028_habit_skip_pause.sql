-- The habit skip/pause field family (HF1, round 4 — plan
-- 2026-11-21-habit-skip-pause): the typed planned-skip mark on the log
-- row (0 = unmarked; additive default) and the habit's inclusive
-- pause-through local date (NULL = not paused; the ISO date string)
-- with its per-field LWW clock column (the 0027 columnar precedent,
-- no data rewrite): `paused_until`, `ck_paused_until`, `skip`.
ALTER TABLE habits ADD COLUMN paused_until TEXT;
ALTER TABLE habits ADD COLUMN ck_paused_until TEXT;
ALTER TABLE habit_log ADD COLUMN skip INTEGER NOT NULL DEFAULT 0;
