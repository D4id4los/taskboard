-- 0012_task_label_memberships: per-membership OR-set state for task
-- labels (DR-4/L, #129) — Task.label_memberships' storage.
-- Conventions: state is 'added' | 'removed' (unknown text decodes to
-- Corrupted); ck_membership is the membership's write clock,
-- epoch-defaulted NOT NULL (0010's mechanism) so pre-migration rows
-- decode as fresh adds — the pre-migration live set bit-for-bit (ADR
-- 0005). Removed rows persist (never garbage-collected while either
-- entity lives); the (task, label) PK and both ON DELETE CASCADEs stay
-- (hard row deletion still sweeps).
ALTER TABLE task_labels ADD COLUMN state TEXT NOT NULL DEFAULT 'added';
ALTER TABLE task_labels
    ADD COLUMN ck_membership TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
