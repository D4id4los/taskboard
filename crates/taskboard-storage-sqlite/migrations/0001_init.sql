-- 0001_init: the normalized taskboard schema (phase 2, storage-sqlite plan §4).
--
-- Conventions:
-- * Local ids are hyphenated UUID TEXT primary keys (debuggable in the
--   sqlite3 CLI; converted explicitly in the row codecs).
-- * Timestamps are chrono-encoded TEXT; they must NEVER be used in
--   ORDER BY (fractional-digit lengths vary) — ordering is always by
--   sort_order, op_seq, or id.
-- * Per-field write clocks are `ck_*` TEXT columns; tombstones are the
--   `deleted` flag plus `ck_deleted` and are retained forever (see ADR 0005).
-- * Remote bindings decompose into nullable INTEGER columns; the all-or-none
--   NULL invariant is enforced at the codec layer.
-- * No triggers, views, or generated columns — all logic lives in Rust.

CREATE TABLE boards (
    id              TEXT PRIMARY KEY NOT NULL,
    remote_board_id INTEGER NULL,
    title           TEXT NOT NULL,
    color           TEXT NOT NULL,
    archived        INTEGER NOT NULL,
    deleted         INTEGER NOT NULL,
    remote_seen     TEXT NULL
);

CREATE TABLE stacks (
    id               TEXT PRIMARY KEY NOT NULL,
    board            TEXT NOT NULL REFERENCES boards (id),
    remote_board_id  INTEGER NULL,
    remote_stack_id  INTEGER NULL,
    title            TEXT NOT NULL,
    sort_order       INTEGER NOT NULL,
    archived         INTEGER NOT NULL,
    deleted          INTEGER NOT NULL,
    ck_title         TEXT NOT NULL,
    ck_sort_order    TEXT NOT NULL,
    ck_deleted       TEXT NOT NULL,
    remote_seen      TEXT NULL
);

CREATE TABLE tasks (
    id               TEXT PRIMARY KEY NOT NULL,
    stack            TEXT NOT NULL REFERENCES stacks (id),
    remote_board_id  INTEGER NULL,
    remote_stack_id  INTEGER NULL,
    remote_card_id   INTEGER NULL,
    title            TEXT NOT NULL,
    description      TEXT NOT NULL,
    duedate          TEXT NULL,
    done             TEXT NULL,
    sort_order       INTEGER NOT NULL,
    archived         INTEGER NOT NULL,
    deleted          INTEGER NOT NULL,
    ck_title         TEXT NOT NULL,
    ck_description   TEXT NOT NULL,
    ck_duedate       TEXT NOT NULL,
    ck_done          TEXT NOT NULL,
    ck_position      TEXT NOT NULL,
    ck_labels        TEXT NOT NULL,
    ck_archived      TEXT NOT NULL,
    ck_deleted       TEXT NOT NULL,
    remote_seen      TEXT NULL
);

CREATE INDEX tasks_by_stack ON tasks (stack);

CREATE TABLE labels (
    id               TEXT PRIMARY KEY NOT NULL,
    board            TEXT NOT NULL REFERENCES boards (id),
    remote_board_id  INTEGER NULL,
    remote_label_id  INTEGER NULL,
    title            TEXT NOT NULL,
    color            TEXT NOT NULL,
    deleted          INTEGER NOT NULL,
    ck_title         TEXT NOT NULL,
    ck_color         TEXT NOT NULL,
    ck_deleted       TEXT NOT NULL,
    remote_seen      TEXT NULL
);

CREATE TABLE task_labels (
    task  TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    label TEXT NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (task, label)
);

CREATE TABLE outbox (
    op_seq    INTEGER PRIMARY KEY AUTOINCREMENT,
    op_id     TEXT NOT NULL UNIQUE,
    op_kind   TEXT NOT NULL,
    task_id   TEXT NULL,
    stack_id  TEXT NULL,
    label_id  TEXT NULL,
    queued_at TEXT NOT NULL
);

CREATE TABLE sync_metadata (
    key           TEXT PRIMARY KEY NOT NULL,
    etag          TEXT NULL,
    last_modified TEXT NULL
);

CREATE TABLE sync_status (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    phase        TEXT NOT NULL,
    last_error   TEXT NULL,
    last_success TEXT NULL
);

INSERT INTO sync_status (id, phase) VALUES (1, 'idle');
