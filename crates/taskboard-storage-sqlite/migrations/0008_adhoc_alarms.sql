-- 0008_adhoc_alarms: standalone task-independent alarms (#211,
-- reminders-surfaces D-R8). Additive only (ADR 0005). Conventions as
-- in 0004: hyphenated-UUID TEXT id, rfc3339 TEXT timestamps, no CHECK
-- re-legislation, no foreign keys (alarms reference no entity;
-- cancel/delete prunes dependents in the batch mirror). Deliveries and
-- snoozes are untouched: an alarm is the text form "alarm:<uuid>"
-- through the existing codec. Device-local: no outbox linkage, no
-- sidecar section.
CREATE TABLE adhoc_alarms (
    alarm_id   TEXT PRIMARY KEY NOT NULL,
    label      TEXT NOT NULL,
    fire_at    TEXT NOT NULL,
    created_at TEXT NOT NULL
);
