-- DR-9 D-BF3 (#685): the roaming settings home (the `settings` wire
-- section's imported rows). The 0022 recipe: a stable TEXT key (the
-- setting's name), the typed value as JSON, an RFC 3339 TEXT clock.
-- No foreign keys (nothing references a setting row). One row per
-- key; the import fold owns the LWW guard, the upsert applies its
-- winner verbatim.
CREATE TABLE roaming_settings (
    name  TEXT PRIMARY KEY NOT NULL,  -- the RoamingSettingValue key name
    value TEXT NOT NULL,              -- JSON: the RoamingSettingValue serde
    clock TEXT NOT NULL               -- RFC 3339 row write clock
);
