-- The habit additive field family (round 9 chunk 4, plan
-- 2026-11-18-habit-domain-fields §Phase 3): a free-text description
-- (the `tasks.description` columnar precedent, defaulting empty) and
-- the optional display/nudge anchor day stored in its serde JSON form
-- (NULL = unset; the one canonical string form of `AnchorDay`). Two
-- new per-field LWW clock columns (the 0002/0026 columnar precedent,
-- no data rewrite): `ck_description`, `ck_anchor_day`.
ALTER TABLE habits ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE habits ADD COLUMN anchor_day TEXT;
ALTER TABLE habits ADD COLUMN ck_description TEXT;
ALTER TABLE habits ADD COLUMN ck_anchor_day TEXT;
