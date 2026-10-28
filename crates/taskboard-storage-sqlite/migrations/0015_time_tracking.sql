-- 0015_time_tracking: settled time entries (DR-5, design D-T9; ADR
-- 0017). Additive only (ADR 0005). Conventions as in 0004/0008:
-- hyphenated-UUID TEXT ids, RFC 3339 TEXT stamps, no foreign keys —
-- entries outlive soft-deleted tasks by design (history survives
-- deletion; the reference is the entry's data, not a join).
--
-- There is no intent table and no running-timer row: the open entry
-- (`end` NULL) IS the running timer, persisted like any other row — a
-- daemon restart rehydrates it and the timer "resumes" because it
-- never stopped. `updated_at` is the row's LWW mutation clock for the
-- sidecar lane.
CREATE TABLE time_entries (
    entry_id   TEXT PRIMARY KEY NOT NULL,
    task_id    TEXT NOT NULL,
    task_title TEXT NOT NULL,
    start      TEXT NOT NULL,
    end        TEXT,                -- NULL = open (the running timer)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
