-- 0032_habit_events: the per-completion habit event log (#1519
-- chunk 3; design §6.1). Additive only (ADR 0005 — never edit a
-- shipped migration). One row per completion or skip-intent mark;
-- uuid primary key; no LWW columns — the union fold arbitrates
-- nothing and the state engine holds the write side.
--
-- MIGRATION CONSEQUENCE (operator ruling on #1549, option 3,
-- 2026-11-27): the load-time remap synthesizes events from the
-- legacy habit_log rows at each row's clock (Ruling A1). Under the
-- event grammar those synthesized events re-derive at read time, so
-- a legacy row whose audited count exceeds the habit's quota
-- (the over-quota AdjustHabitLog valve, #1549) migrates to a derived
-- count capped at the quota, with the surplus re-attributed into the
-- unmet predecessor window — the limit the event grammar accepted on
-- #1549 now applies to migrated data. Recorded here, at the remap
-- site, in the chunk-3 plan; chunk 5 carries the user-facing note
-- (see .artifacts/plans/2026-11-28-habit-event-migration-plan.md §3).
CREATE TABLE habit_event (
    id         TEXT PRIMARY KEY NOT NULL,  -- event uuid (hyphenated)
    habit_id   TEXT NOT NULL,
    kind       TEXT NOT NULL,              -- 'completion' | 'skip' | 'unskip'
    window     TEXT,                       -- mark provenance date; NULL for completion
    at         TEXT NOT NULL,              -- rfc3339 (UTC, nanos form)
    au_by      TEXT,                       -- author (0023/0026 column grammar)
    deleted_at TEXT,                       -- tombstone (Ruling C2) — rides the row
    deleted_by TEXT
);
CREATE INDEX idx_habit_event_habit_at ON habit_event (habit_id, at);

-- The one-shot remap marker (design §6.2): a row here means "the
-- load-then-remap pass ran with a 0032-aware binary" — the pass
-- never re-runs after the user logs new events mid-upgrade. The
-- migration creates the table empty; the remap inserts the row (data
-- belongs to Rust, not to an immutable SQL file).
CREATE TABLE habit_event_migration (
    done INTEGER NOT NULL CHECK (done = 1)
);
