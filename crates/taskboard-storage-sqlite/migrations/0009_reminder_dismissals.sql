-- 0009_reminder_dismissals: user-dismissal facts (reminder-rule sync
-- round R3, design D-5/D-11). Additive only (ADR 0005). Conventions as
-- in 0004: hyphenated/`rule:`-prefixed TEXT reminder keys, rfc3339 TEXT
-- timestamps, no triggers, no foreign keys (a rule/alarm delete prunes
-- its dependent rows first — the batch mirror cascades).
--
-- Primary-key parity with reminder_deliveries: `(reminder, occurrence)`
-- is the identity, first write wins (a dismissal is permanent for its
-- occurrence; there is no un-dismiss), `dismissed_at` is the row's LWW
-- write clock for the document lane.
CREATE TABLE reminder_dismissals (
    reminder     TEXT NOT NULL,
    occurrence   TEXT NOT NULL,
    dismissed_at TEXT NOT NULL,
    PRIMARY KEY (reminder, occurrence)
);

-- Upgrade backfill (design D-10): pre-R3 behavior auto-settled every
-- fired occurrence, so without it every already-fired occurrence would
-- resurface as an active in-app notification after upgrade. One
-- dismissal row per existing delivery row, `dismissed_at =
-- delivered_at`: old behavior ≡ everything already dismissed. (The
-- synced `dismissals` document section later emits only the Rule-ref
-- rows — the emission filter, not this table, enforces the split.)
INSERT INTO reminder_dismissals (reminder, occurrence, dismissed_at)
    SELECT reminder, occurrence, delivered_at FROM reminder_deliveries;
