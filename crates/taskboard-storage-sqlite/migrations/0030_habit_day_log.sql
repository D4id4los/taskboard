-- The per-day habit activity overlay (US-8, #1144; the 2026-11-22
-- habit-surface journeys plan's recorded D-H12 amendment): one row per
-- `(habit, day)` attribution fact, display data beside the window log
-- (the window log stays the stats oracle, R-2). Same whole-second
-- clock grammar as `habit_log` (#1095); device-local — the overlay
-- never enters a sync lane.
CREATE TABLE habit_day_log (
    habit_id TEXT NOT NULL,
    day TEXT NOT NULL,
    count INTEGER NOT NULL,
    clock TEXT NOT NULL,
    PRIMARY KEY (habit_id, day)
);
