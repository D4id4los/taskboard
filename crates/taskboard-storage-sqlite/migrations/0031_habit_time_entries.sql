-- 0031_habit_time_entries: the habit time-entry fact family (#1305,
-- journey plan 2026-11-24 D-1). Additive only (ADR 0005). Conventions
-- as in 0015: hyphenated-UUID TEXT ids, RFC 3339 TEXT stamps, no
-- foreign keys — entries outlive tombstoned habits by design (D-4:
-- history survives the habit; the reference is the entry's data, not a
-- join). The open entry (`end` NULL) IS the running timer, persisted
-- like any other row — a restart rehydrates it and the timer "resumes"
-- because it never stopped. `updated_at` is the row's LWW mutation
-- clock for the sidecar lane. Kind-less this slice (journey plan D-2:
-- no break-reminder consumer exists for habits; a kind column is
-- additive later).
CREATE TABLE habit_time_entries (
    entry_id    TEXT PRIMARY KEY NOT NULL,
    habit_id    TEXT NOT NULL,
    habit_title TEXT NOT NULL,
    start       TEXT NOT NULL,
    end         TEXT,                -- NULL = open (the running timer)
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    au_by       TEXT
);
