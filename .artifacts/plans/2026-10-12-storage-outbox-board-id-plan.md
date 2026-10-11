# [Plan] Storage — Outbox `board_id` Column (kill the `RenameBoard` StackId cast)

- **Date**: 2026-10-12
- **Target**: `taskboard-storage-sqlite` (`migrations/0003_outbox_board_id.sql`
  new, `src/codec.rs`, `src/repo.rs`, `src/connect.rs` tests,
  `tests/roundtrip.rs`), `taskboard-domain` (`src/test_support/state.rs`
  strategy only), workspace-root `.sqlx/` offline cache,
  `.artifacts/taskboard-potential-improvements-backlog.md` (final-commit
  bookkeeping). **No changes** to `taskboard-state`, `taskboard-sync-nextcloud`,
  `taskboard-app`, `taskboard-ui-slint`, any ADR, or any `docs/*.org` file.
  **No new dependencies.**
- **Goal**: resolve backlog "[2026-10-11] Outbox `board_id` column for
  board-scoped ops" (dispatch plan §4 WI-A). `LocalOp::RenameBoard` today
  persists its `BoardId` by casting through a `StackId` into the outbox
  `stack_id` column (codec.rs:101-108 write, codec.rs:151-153 read). Migration
  `0003` adds a nullable `board_id` column and backfills legacy
  `rename_board` rows into it; the codec then uses the board column in both
  directions and the cast — plus its workaround comment — dies. The open
  board color/archive and unbind backlog entries would each widen the hack;
  this is the cheap window to close it.
- **Status**: PROPOSED — ready for stage 2 (implementation) upon user
  approval per the dispatch plan §2 three-stage process. All four WI-A open
  questions are resolved with recorded decisions in §3 (D1–D4); nothing here
  is user-gated.
- **Parent**: `.artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md`
  §2 (execution model) and §4 WI-A (context, open questions, acceptance).
- **Branch / worktree**: `fix/storage-outbox-board-id`, worktree
  `../taskboard-WT-fix-storage-outbox-board-id` (created from `main` @
  `3ca11ea`). One PR for the plan (WI-3 precedent: plan revisions and
  implementation commits share the branch).

---

## 1. Context & Known Facts (do not re-derive; verified on `main` @ `3ca11ea`)

- **The cast exists at exactly two sites, both in
  `crates/taskboard-storage-sqlite/src/codec.rs`**: the write side
  `op_to_columns` (`:101-108`) stores
  `StackId::from_uuid(board.as_uuid())` into `stack_id` under an explanatory
  comment ("Board-scoped op: the outbox has no board column …"); the read
  side `op_from_columns` (`:151-153`) parses `stack_id` as a `StackId` and
  re-casts to `BoardId`. A workspace-wide grep finds **no other
  cross-type id cast** — every other `BoardId::from_uuid` / `StackId::
  from_uuid` hit is a same-type construction from a raw `Uuid` (test
  fixtures, CLI id parsing, `ids.rs` accessor proptests).
- **Outbox schema** (`migrations/0001_init.sql:86-94`):
  `op_seq INTEGER PRIMARY KEY AUTOINCREMENT, op_id TEXT NOT NULL UNIQUE,
  op_kind TEXT NOT NULL, task_id TEXT NULL, stack_id TEXT NULL, label_id
  TEXT NULL, queued_at TEXT NOT NULL` — no board column. Shipped
  migrations: 0001 (init), 0002 (`boards.ck_title`, epoch default).
- **`OpColumns`** (codec.rs:60-66) is `pub(crate)`, carries
  `task_id/stack_id/label_id: Option<_>`, and wears
  `#[allow(clippy::struct_field_names)]` with the comment "the *_id names
  mirror the SQL columns". `op_from_columns` is `pub` (crate-doc'd codec
  module) with exactly **one** caller outside the codec's own tests:
  `repo.rs:308` (`decode_outbox_row`). `op_to_columns` has one external
  caller: `repo.rs:754` (`EnqueueOp` arm of `execute_batch`).
- **The repo speaks only compile-time-checked macros**: `OutboxRow`
  (repo.rs:102-110) via `query_as!` SELECT (repo.rs:402-410, explicit
  column list ordered `op_id, op_kind, task_id, stack_id, label_id,
  queued_at`), and the outbox INSERT … `ON CONFLICT(op_id) DO UPDATE`
  (repo.rs:761-779). Touching these changes the offline prepare data.
- **Offline workflow** (ADR 0005; `scripts/sqlx-prepare.sh`): the tracked
  cache is `.sqlx/` at the workspace root (18 query JSON files); the
  script migrates a throwaway db and runs `cargo sqlx prepare
  --workspace -- --all-targets` with sqlx-cli pinned to major/minor 0.9
  (format skew across versions). CI's `check` job compiles with
  `SQLX_OFFLINE=true` and runs `scripts/sqlx-prepare.sh --check` as a
  freshness gate — a forgotten refresh fails the PR there. The script
  header and ADR both mandate committing the refreshed `.sqlx/` **with**
  the change, never separately. (The stray
  `crates/taskboard-storage-sqlite/src/.sqlx-prepare.db` is untracked —
  `*.db` is gitignored — and is not this plan's business.)
- **How migration 0002 tested the upgrade-from-0001 path** (the harness
  WI-A OQ1 asks to reuse): in-crate `#[tokio::test]
  migration_0002_upgrades_a_pre_clock_database` (connect.rs:131-169). The
  pattern is *forward-then-strip*: temp file db → raw pool → run the full
  embedded `sqlx::migrate!` → downgrade by `ALTER TABLE boards DROP
  COLUMN ck_title` + `DELETE FROM _sqlx_migrations WHERE version = 2` →
  seed a legacy-shaped row → close → reopen through `open()` (which
  applies pending migrations) → `load()` and assert semantics.
  `reopening_applies_no_new_migrations` (connect.rs:110-126) additionally
  pins the applied-migration count — currently `assert_eq!(applied, 2)`.
- **The storage P1 roundtrip proptest never exercises a `RenameBoard` op
  through SQL today**: `persisted_state_strategy`'s outbox rows draw
  `kind in 0_usize..13` (test_support/state.rs:390) and `build_outbox`
  (:450-478) maps kinds 0–12 onto task/stack/label/assign arms with `_`
  falling back to `AssignLabel` — `RenameBoard` (the 14th `LocalOp`
  variant) is never generated. It *is* covered at the pure column level by
  the codec unit test `op_columns_roundtrip_every_variant` (codec.rs:321),
  and the state crate's outbox-validity predicate already has a
  `RenameBoard` arm (state/tests/proptest.rs:622) — so widening the
  strategy breaks nothing downstream.
- **Multi-statement migration files are proven** — 0001 itself is many
  statements plus a seed INSERT — and SQLite DDL/DML is transactional, so
  each migration file applies atomically. `ALTER TABLE … DROP COLUMN`
  (needed by the forward-then-strip test) is already used by the 0002
  test; the bundled libsqlite3-sys far exceeds the 3.35 floor.
- **No documentation drift**: no `docs/*.org` file or ADR mentions the
  outbox's id columns or `rename_board` storage shape. ADR 0005's
  statement — "`LocalOp` decomposes into an `op_kind` tag column plus the
  id columns it addresses" — remains true after this change (it gains a
  fourth id column); per ADR 0005 the shipped 0001/0002 files are
  immutable and the schema change is a new numbered file. The phase-2
  plan §4.1 sketch is a historical planning artefact and is not edited.
- **Backlog bookkeeping precedent**: the WI-A entry lives at
  `.artifacts/taskboard-potential-improvements-backlog.md:1426` ("[2026-10-11]
  Outbox `board_id` column for board-scoped ops"); DONE marking follows the
  `Status: ✅ DONE (…)` convention (e.g. the release-pipeline entry), edited
  in a final `docs`-type commit (WI-3 precedent `09d03b6`).

## 2. Scope

**In scope** (single deliverable, one PR):

1. Migration `0003_outbox_board_id.sql`: additive `board_id TEXT NULL` +
   op_kind-scoped backfill (D2).
2. Codec: `OpColumns.board_id`, `rename_board` uses the board column in
   both directions, cast + workaround comment removed,
   `op_from_columns` signature gains the `board_id` parameter.
3. Repo: `OutboxRow.board_id`, outbox SELECT and INSERT/ON CONFLICT carry
   the column.
4. `.sqlx/` offline cache refresh (same commit as 3).
5. Tests: codec unit updates, a new SQL-level column-placement test (T9),
   a new migration upgrade/backfill test (T10), a new T7 corruption-class
   entry, the connect.rs migration-count fix, and the domain strategy
   extension so P1 covers the board column (D6).
6. Final in-branch commit: backlog entry ✅ DONE.

**Out of scope** (recorded, not backlogged — these are design alternatives
considered and rejected inside this plan, not deferred improvements):

- *Strict rejection of over-populated op rows* (e.g. a `create_task` row
  with `stack_id` set): today's decode ignores extra columns; tightening
  this is a deliberate behavior change beyond WI-A and stays out. The
  per-kind contract remains "required column NULL ⇒ `Corrupted`".
- Board color/archive verbs and unbind ops (separate backlog entries that
  *benefit from* this column), any UI/state/sync change, ADR or
  `docs/*.org` amendments, schema changes beyond the one column.

## 3. Design Decisions (WI-A open questions resolved)

Each WI-A open question from dispatch §4 maps to a decision below; D5–D8
record the remaining design choices the implementation must follow.

1. **D1 — OQ1 (upgrade-test harness): reuse the 0002 forward-then-strip
   pattern, with a queued `rename_board` fixture row.** New in-crate test
   `migration_0003_backfills_legacy_rename_board_rows` in `connect.rs`:
   migrate fully → strip (`ALTER TABLE outbox DROP COLUMN board_id` +
   `DELETE FROM _sqlx_migrations WHERE version = 3`) → seed one legacy
   `rename_board` row (board uuid riding `stack_id`, the exact 0002-era
   wire shape) plus one `rename_stack` and one `create_task` row as
   controls → reopen via `open()` → assert (a) `load()` decodes the row to
   `LocalOp::RenameBoard(<board>)` — the op *survives* the backfill — and
   (b) via raw SQL on a second pool, the `rename_board` row now has
   `board_id = <board-uuid-text>` and `stack_id IS NULL`, while the
   control rows keep their original columns untouched (`rename_stack`
   keeps `stack_id`, `board_id` stays NULL). Also bump
   `reopening_applies_no_new_migrations` to `assert_eq!(applied, 3)`.
2. **D2 — OQ2 (NULL `stack_id` on backfilled rows): yes — backfill and
   clear in the same migration.** Rationale: post-migration every
   `rename_board` row must have exactly the shape new writes produce
   (`board_id` set, `stack_id` NULL); leaving the old `stack_id` value in
   place would keep a dead, misleading copy of the id in the column the
   cast used to ride — precisely the trip-hazard this item exists to
   remove — and would give one op kind two stored shapes. The UPDATE is
   `op_kind`-scoped, executes exactly once per database (bookkept by
   `_sqlx_migrations`), and applies atomically with the `ADD COLUMN`
   (transactional DDL/DML, §1).
3. **D3 — OQ3 (sqlx offline workflow): compile-time macros are involved —
   refresh `.sqlx/` via `scripts/sqlx-prepare.sh` in the same commit as
   the repo changes.** Exactly two cached queries change shape (the outbox
   `query_as!` SELECT and the outbox `query!` INSERT), so the refresh
   replaces ~2 JSON files with ~2 new-hash ones; commit them with the
   codec/repo commit (commit 3 in §6), never separately. Use sqlx-cli 0.9
   (`cargo install sqlx-cli --version 0.9 --no-default-features
   --features sqlite` if missing). The migration-only commit (§6 commit 2)
   needs **no** refresh — its queries' explicit column lists are unchanged
   and the added nullable column does not alter their describe data; CI's
   `--check` gate verifies this claim on every push. Stage 2 runs
   `scripts/sqlx-prepare.sh --check` locally before handing off.
4. **D4 — OQ4 (`#[allow(clippy::struct_field_names)]`): keep it, comment
   unchanged.** Adding `board_id` gives four fields sharing the `_id`
   suffix, so the lint still fires; the existing justification ("the *_id
   names mirror the SQL columns") remains exactly the reason — renaming
   the fields to dodge the lint would diverge from the SQL columns the
   struct documents itself as mirroring.
5. **D5 — decode semantics: a `rename_board` row with NULL `board_id` is
   `Corrupted`** (the "missing required op column for the op kind" class,
   plan T7 of the storage plan) — regardless of what `stack_id` holds.
   This is a deliberate tightening: the legacy shape (board id in
   `stack_id`) now fails loudly instead of silently casting. It cannot
   occur on migrated databases (the backfill is exhaustive over
   `op_kind = 'rename_board'`); a hand-mangled db surfacing as
   `Corrupted` is the intended, honest outcome. Pinned by the new T7
   entry (§5).
6. **D6 — the domain strategy learns `RenameBoard`** so the storage P1
   proptest covers the board column through real SQL with 256 random
   cases per run: `outbox_rows`'s kind range widens `0_usize..13` →
   `0_usize..14`, `build_outbox` gains `13 =>` mapping the first
   generated board id to `LocalOp::RenameBoard` (rows with no matching
   entity are skipped, same as every other kind), and the now-unreachable
   `_` arm becomes `_ => None` (defensive only). `RenameBoard` is a
   fully-supported `LocalOp` since WI-3, and the state crate's
   outbox-validity predicate already carries its arm — every consumer of
   the strategy stays green.
7. **D7 — the invariant that kills the hack needs its own SQL-level pin.**
   Semantic roundtrips alone cannot distinguish the new column from the
   old cast: a codec that writes the board id into `stack_id` *and reads
   it back from there* passes P1 and the codec roundtrip unit test
   identically. New test **T9** (roundtrip.rs) enqueues a `RenameBoard`
   through `apply` on a file db, then asserts via raw SQL on a second pool
   that the stored row has `board_id = <uuid-text>` **and**
   `stack_id IS NULL` — structural column facts, no message text —
   and that `load()` returns the op unchanged. This is the test a future
   relapse trips over.
8. **D8 — naming and schema mechanics**: the migration is
   `0003_outbox_board_id.sql` (dispatch-mandated name); SQLite appends the
   new column physically after `queued_at`, which is cosmetic — all
   access is by named column lists. No index, constraint, or trigger is
   added (0001's convention: logic lives in Rust); no ADR/docs amendment
   (§1). `op_from_columns`' public signature change is a workspace-internal
   API evolution — no semver ceremony beyond the normal gate suite.

## 4. File-by-File Specification

### 4.1 `migrations/0003_outbox_board_id.sql` (new)

```sql
-- 0003_outbox_board_id: board-scoped outbox ops get their own column.
-- `rename_board` rows previously rode `stack_id` with a BoardId↔StackId
-- cast in the codec (a WI-3 workaround, closed by backlog WI-A); the
-- backfill moves the id into `board_id` and clears `stack_id` so every
-- op kind addresses exactly the columns its tag names.
ALTER TABLE outbox ADD COLUMN board_id TEXT NULL;
UPDATE outbox
   SET board_id = stack_id,
       stack_id = NULL
 WHERE op_kind = 'rename_board';
```

### 4.2 `src/codec.rs`

- Import `BoardId` alongside `LabelId, RemoteBoardId, StackId, TaskId`
  (the two rewritten arms then read like every other arm instead of the
  current fully-qualified paths).
- `OpColumns` gains `board_id: Option<BoardId>` (placed after
  `label_id`); the `#[allow(clippy::struct_field_names)]` and its comment
  stay (D4).
- `op_to_columns`: every existing arm gains `board_id: None`; the
  `RenameBoard` arm becomes
  `LocalOp::RenameBoard(board) => OpColumns { task_id: None, stack_id: None, label_id: None, board_id: Some(board) }`
  and the three-line workaround comment dies with the cast.
- `op_from_columns` gains a `board_id: Option<&str>` parameter (after
  `label_id`, mirroring the column order); the `rename_board` arm becomes
  `"rename_board" => Ok(LocalOp::RenameBoard(parse_id(board_id)?))` — the
  `StackId` parse-and-recast dies. The doc comment's corruption contract
  is unchanged (D5 makes NULL `board_id` the missing-required-column
  class for this kind). Single external caller updated (repo.rs:308,
  passes `row.board_id.as_deref()`).
- Tests: `op_columns_roundtrip_every_variant` threads a fourth
  `board_text` column through the loop; `missing_required_op_column_
  is_rejected` gains `op_from_columns("rename_board", None, None, None,
  None) == Err(CodecError)` (the old cast's shape is now a rejection).

### 4.3 `src/repo.rs`

- `OutboxRow` gains `board_id: Option<String>` (after `label_id`).
- Outbox SELECT (load): `SELECT op_id, op_kind, task_id, stack_id,
  label_id, board_id, queued_at … FROM outbox ORDER BY op_seq`.
- EnqueueOp INSERT: add the `board_id` column to the insert list, bind
  `cols.board_id.map(|b| codec::uuid_to_text(b.as_uuid()))`, and extend
  the `ON CONFLICT(op_id) DO UPDATE` arm with
  `board_id = excluded.board_id` (a re-enqueued id must fully refresh the
  row, exactly like the other columns — plan P4 semantics).

### 4.4 `crates/taskboard-domain/src/test_support/state.rs`

Per D6: kind range `0_usize..13` → `0_usize..14`; `build_outbox` takes
`let board_id = state.boards.keys().next().copied();` alongside the
existing task/stack/label lookups and adds
`13 => b.map(LocalOp::RenameBoard), _ => None,` (the previous `_ =>
AssignLabel` fallback becomes unreachable and defensive). Doc comment on
`OutboxRow.kind` updated to say the index resolves against all 14
`LocalOp` variants.

### 4.5 `.sqlx/` (workspace root)

Refreshed by `scripts/sqlx-prepare.sh` during commit 3 (D3): the two
outbox query hashes rotate; commit the resulting `.sqlx/` churn with that
commit. Nothing manual.

### 4.6 `.artifacts/taskboard-potential-improvements-backlog.md`

Final commit only: the "[2026-10-11] Outbox `board_id` column for
board-scoped ops" entry gains
`- **Status**: ✅ DONE (implemented 2026-10-12 via this plan; migration
0003 + codec, PR <n>)` — one line, house convention.

## 5. Test Plan (TDD order; catalogue extends the phase-2 plan's §10)

Deterministic by construction; typed/structural assertions only — decoded
ops, `matches!(…, Err(RepositoryError::Corrupted))`, raw-SQL column
facts. No text assertions on messages, no sleeps.

| ID | Layer | Test | Red-before state |
|----|-------|------|------------------|
| T9 | roundtrip.rs (file db) | `rename_board_op_lands_in_the_board_column`: `apply([EnqueueOp(RenameBoard)])` → raw SQL on a second pool asserts `board_id = <uuid-text> AND stack_id IS NULL` for the row → `load()` outbox equals the input op | fails on `main`: row lands in `stack_id`, `board_id` column doesn't exist |
| T10 | connect.rs | `migration_0003_backfills_legacy_rename_board_rows` (D1 full scenario) | fails on `main`: migration 0003 doesn't exist; the strip step's `DROP COLUMN board_id` errors |
| T7+ | roundtrip.rs | corruption catalogue entry: raw `INSERT INTO outbox (op_id, op_kind, queued_at) VALUES (…, 'rename_board', …)` (no ids) → `load()` = `Err(Corrupted)` | fails after the codec switch if the arm were left permissive |
| P1 | roundtrip.rs | unchanged test body; now covers `RenameBoard` via the widened strategy (D6), 256 cases | codec unit tests (codec.rs) | roundtrip + rejection arms extended per §4.2 |
| — | connect.rs | `reopening_applies_no_new_migrations`: `applied == 3` | fails until 0003 ships |

Write T9 and T10 **first** and watch them fail (TDD red), then implement
§4.1–4.3 (green), then the codec unit-test extensions and the T7 entry
ride the same working session. The domain strategy change (§4.4) is
verified by P1 picking up board ops in its generated states.

## 6. Implementation Order & Commits (each standalone-green, rebase-merge hygiene)

1. `test(domain): persisted_state_strategy generates RenameBoard outbox ops`
   — §4.4 alone. Green on its own: every strategy consumer already
   handles `RenameBoard` (§1).
2. `fix(storage): migration 0003 — outbox board_id column, rename_board backfill`
   — §4.1 + connect.rs tests (T10, count 3). Green on its own: codec
   untouched, so the backfilled rows still decode through the legacy
   `stack_id` path; T10's decode assertion holds either way. No `.sqlx/`
   churn (D3).
3. `fix(storage): rename_board rides the outbox board_id column`
   — §4.2, §4.3, §4.5 (refresh committed here), codec test extensions,
   T7 entry, T9. This is the commit that deletes the cast.
4. `docs(backlog): WI-A bookkeeping — outbox board_id entry done`
   — §4.6; the stage-2 agent's final commit per dispatch §2.

Then the full gate suite (§8). Stage 3 reviews `git diff main...HEAD`,
re-runs gates, ships the PR.

## 7. Risks & Mitigations

- **Offline cache skew** (wrong sqlx-cli version → silently incompatible
  `.sqlx/` format): pinned 0.9 in the script header and ADR 0005; the CI
  freshness gate and the local `--check` run catch it. Mitigation: run
  `scripts/sqlx-prepare.sh --check` as part of commit 3's gate pass.
- **Forgotten cache refresh** (query changed, `.sqlx/` stale): impossible
  to ship — `SQLX_OFFLINE=true` builds in CI fail to resolve the macros.
- **Backfill misses a shape** (a `rename_board` row with NULL `stack_id`):
  harmless — `UPDATE` copies nothing; the row already decodes `Corrupted`
  today (missing column) and continues to (D5). No pre-migration row shape
  exists that regresses.
- **Downstream proptest surprise from D6**: none expected — `RenameBoard`
  is fully supported domain surface; the state predicate arm exists
  (proptest.rs:622). A failure there would be a genuine find (AGENTS §5:
  evaluate production vs test before changing anything).
- **Miri weekly job** (WI-C's tuning): the strategy widening adds a kind
  to generated cases, a negligible cost change; the storage crate is out
  of Miri scope (testing strategy §10). Not a risk to that work item.
- **Wave-1 backlog-file collision** (every item's final commit edits the
  backlog): anticipated by dispatch §2; trivial rebase if WI-A lands
  after another wave-1 item.
- **DROP COLUMN floor** (sqlite ≥ 3.35 for the strip step): already
  relied on by the 0002 upgrade test via the bundled libsqlite3-sys;
  not a real risk, recorded for completeness.

## 8. Verification & Acceptance (dispatch §4 WI-A criteria mapped)

1. **No `BoardId`↔`StackId` casts remain anywhere in the workspace**:
   `grep -rn "StackId::from_uuid(board\|BoardId::from_uuid(.*StackId"
   crates/` returns nothing; both codec.rs sites (101-108, 151-153) are
   gone with their comment. (Reviewed again by stage 3 across the full
   diff.)
2. **Roundtrip test extended through the board column**: P1 covers
   `RenameBoard` through SQL (D6); T9 pins the column placement; T10 pins
   the backfill; the codec unit test threads all four columns.
3. **Full gates green** (AGENTS §10, run from the worktree root):
   `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
   -- -D warnings`; `cargo nextest run --workspace`; `cargo test --doc
   --workspace`; `cargo deny check`; `cargo llvm-cov --workspace`; plus
   `scripts/sqlx-prepare.sh --check` (WI-A-specific, D3).
4. **Backlog entry ✅ DONE** in the final commit (§4.6).

Estimated diff: ~170 changed lines across 7 files (migration ~10, codec
~45 incl. tests, repo ~12, connect tests ~60, roundtrip tests ~35,
strategy ~5, backlog ~2) plus ~4 `.sqlx/` JSON file rotations — size M as
dispatched.
