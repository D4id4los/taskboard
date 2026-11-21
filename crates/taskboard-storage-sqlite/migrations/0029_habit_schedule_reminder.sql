-- HF4 (#960, #959): the habit scheduled-day set and the reminder
-- override, plus their two LWW clock slots — the 0027/0028 shape (JSON
-- payload TEXT, NULL = unset; clock TEXT nullable, read through the
-- epoch COALESCE like `ck_description`/`ck_anchor_day` so pre-HF4
-- rows lose every clock race exactly alike). Additive, no data
-- rewrite.
ALTER TABLE habits ADD COLUMN scheduled_days TEXT;
ALTER TABLE habits ADD COLUMN reminder TEXT;
ALTER TABLE habits ADD COLUMN ck_scheduled_days TEXT;
ALTER TABLE habits ADD COLUMN ck_reminder TEXT;
