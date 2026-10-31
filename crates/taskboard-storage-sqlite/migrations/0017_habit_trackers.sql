-- 0017_habit_trackers: habit trackers and their count log (DR-6/6;
-- design §4.4/§4.5). Additive only (ADR 0005). Conventions as in 0015:
-- hyphenated-UUID TEXT ids, RFC 3339 TEXT stamps, no foreign keys —
-- the log outlives its tracker by design (DeleteHabit never touches
-- the log), so a habit_log row is data, not a join.
--
-- `target` is one serde_json TEXT column of `HabitTarget` (the stored
-- contract the write path already speaks — the design sketch's
-- `target_kind`/`target_n` pair is superseded); the per-field `ck_*`
-- clocks carry the LWW stamps. `anchor_time` is "HH:MM:SS" TEXT, NULL
-- = no display chip.
CREATE TABLE habits (
    habit_id       TEXT PRIMARY KEY NOT NULL,
    board_id       TEXT NOT NULL,
    title          TEXT NOT NULL,
    target         TEXT NOT NULL,      -- serde_json HabitTarget
    sort_order     INTEGER NOT NULL,
    anchor_time    TEXT,               -- "HH:MM:SS", NULL = no chip
    ck_title       TEXT NOT NULL,
    ck_target      TEXT NOT NULL,
    ck_order       TEXT NOT NULL,
    ck_anchor_time TEXT NOT NULL,
    ck_deleted     TEXT NOT NULL,
    deleted        INTEGER NOT NULL,   -- 0/1 tombstone
    continued_from TEXT                -- UUID, NULL = unlinked
);
CREATE TABLE habit_log (
    habit_id     TEXT NOT NULL,
    window_start TEXT NOT NULL,        -- canonical period-start "YYYY-MM-DD"
    count        INTEGER NOT NULL,
    clock        TEXT NOT NULL,        -- the row's LWW clock
    PRIMARY KEY (habit_id, window_start)
);
