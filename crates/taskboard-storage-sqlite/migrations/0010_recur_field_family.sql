-- 0010_recur_field_family: the recurring-series field family (#498,
-- chunk DR-6/1). The codec arm lands in the same change; until a rule
-- is set the column sits at its default.
-- Conventions: value column NULL = unset (0007's precedent — the recur
-- TEXT is the serde JSON of the Option<RecurRule>); the clock column is
-- epoch-defaulted NOT NULL (0007's mechanism) so pre-migration rows
-- read as "never locally written" — every remote stamp / document
-- clock wins bit-for-bit. Additive only (ADR 0005).
ALTER TABLE tasks ADD COLUMN recur     TEXT NULL; -- serde JSON of Option<RecurRule>
ALTER TABLE tasks ADD COLUMN ck_recur  TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
