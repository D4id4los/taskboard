-- 0004_reminders: persisted reminder facts (reminders design round §9,
-- chunk 2 of the implementation dispatch).
--
-- Additive only (ADR 0005). Conventions as in 0001: hyphenated-UUID TEXT
-- ids, rfc3339 TEXT timestamps, no triggers or generated columns. The
-- domain newtype guarantees `offset_seconds <= 0`, so no CHECK
-- constraint re-legislates it here; the primary keys are the lookup
-- paths at kiosk scale, so no extra indexes. Deliveries/snoozes/muted
-- carry no foreign keys: a rule delete prunes its dependent rows first
-- (the D-c cascade in the command planner and the batch mirror).

CREATE TABLE reminder_rules (
    rule_id        TEXT PRIMARY KEY NOT NULL,
    task_id        TEXT NOT NULL REFERENCES tasks (id),
    offset_seconds INTEGER NOT NULL,
    created_at     TEXT NOT NULL
);

CREATE TABLE reminder_deliveries (
    reminder     TEXT NOT NULL,
    occurrence   TEXT NOT NULL,
    delivered_at TEXT NOT NULL,
    PRIMARY KEY (reminder, occurrence)
);

CREATE TABLE reminder_snoozes (
    reminder       TEXT NOT NULL,
    occurrence     TEXT NOT NULL,
    rescheduled_to TEXT NOT NULL,
    PRIMARY KEY (reminder, occurrence)
);

CREATE TABLE reminder_muted (
    task_id TEXT PRIMARY KEY NOT NULL
);
