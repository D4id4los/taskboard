# [Plan] Storage Test Hygiene + Reminder Roundtrips — #313 + #323

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 4 of the remediation-followups dispatch
  (dispatch §1 row 4 + §5, prompt 4.1). Planning only — no implementation
  in this commit.
- **Origin**: issues **#313** (normative: sqlite test pools leak
  `-shm`/`-wal` sidecars into `/tmp`; watchlist report
  `.artifacts/reports/2026-10-17-flaky-test-watchlist-investigation-report.md`
  §5 items 1–2) and **#323** (reminder partitions absent from the
  whole-state roundtrip tier; execute-batch plan
  `.artifacts/plans/2026-10-17-execute-batch-exhaustive-plan.md` §7
  bookkeeping item). Dispatch §5 settles scope: fixture wrapper or
  explicit close at test end, a path-scoped teardown guard, the reminder
  roundtrip fixture shape, and an IPC-tempdir fold-in verdict.
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups`, continuing after
  chunk 3's tip (`c74559e`). Chunks 1–3 confined to
  `taskboard-sync-nextcloud` / `taskboard-state` —
  `crates/taskboard-storage-sqlite/` is byte-identical to `main`, so
  every anchor below is against this tree.

---

## §summary — what this chunk delivers

One additive, behavior-neutral seam on `SqliteTaskRepository` (an awaited
`close()`), one RAII test fixture (`FileDb`) in
`crates/taskboard-storage-sqlite/tests/common/mod.rs` that owns the temp
file and guarantees the pool closes before the file drops, the conversion
of every NamedTempFile-based database site in the crate (4 integration
sites + 6 unit sites + 1 unclosed raw pool), two path-scoped teardown
guard tests, and the #323 extension of `actions_from_state` so the four
snapshot-derivable reminder partitions join the randomized whole-state
roundtrip proptest. No production behavior, schema, or query changes; the
only `src/` diff is the `close()` method and its doc.

Two fact-finding results of this planning round recalibrate the chunk:

1. **Drop order alone cannot fix #313** — verified by a live probe
   (§D2): sqlx 0.9's `Pool` drop only *marks* the pool closed; the
   sqlite sidecar files survive plain drop deterministically (still
   present 500 ms later). An awaited `pool.close()` is what removes
   them. The wrapper must await, not just reorder drops.
2. **#323's premise is partially stale** — the domain contract harness
   has covered all five reminder variants *against the sqlite adapter*
   (via `tests/contract.rs` C1/C2) since `53ac27c` (2026-10-13), four
   days before the issue was filed. The genuine remaining gap is the
   one the issue's root-cause sentence names: `actions_from_state`
   emits only the 7 snapshot-derivable entity/outbox/validator/sync
   variants, so the randomized proptest tier silently discards the
   reminder content that `persisted_state_strategy` already generates.
   This chunk closes exactly that gap and adds no duplicate tests
   (§D4).

## §decisions — every open question settled

### D1 — fixture wrapper shape: `FileDb` RAII guard + an additive `close()` seam on the repository

**The wrapper** lives in `tests/common/mod.rs` (integration tier, where
the file-database tests live):

```rust
/// A file-backed database fixture: owns the temp file and the
/// repository, and closes the pool *before* the temp handle drops.
/// sqlx 0.9's `Pool` drop only marks the pool closed — sqlite's
/// `-wal`/`-shm` siblings survive `NamedTempFile`'s single-file
/// cleanup (issue #313), so disposal must be awaited, never dropped.
pub struct FileDb {
    repo: Option<SqliteTaskRepository>,
    file: tempfile::NamedTempFile, // in a struct: fields drop repo-first
}

impl FileDb {
    pub async fn new() -> Self;                    // tempfile(.db) + open()
    pub fn path(&self) -> &std::path::Path;        // for raw second connections
    pub fn repo(&self) -> &SqliteTaskRepository;   // expect("database still open")
    pub async fn close(&mut self);                 // awaited close; repo = None
    pub async fn reopen(&mut self);                // open(path) again (T8)
}

impl Drop for FileDb {
    fn drop(&mut self) {
        // Forgetful-path backstop: drop the repo, then unlink the
        // sidecar siblings synchronously — NamedTempFile only removes
        // its own named file. Best-effort on every platform.
        drop(self.repo.take());
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(sibling(self.file.path(), suffix));
        }
    }
}
```

**The seam** (the only `src/` diff, `crates/taskboard-storage-sqlite/src/repo.rs`):

```rust
impl SqliteTaskRepository {
    /// Disposes the pool deterministically: awaits the closure of every
    /// connection, letting sqlite checkpoint and remove the `-wal`/`-shm`
    /// siblings. `Drop` alone only marks the pool closed (sqlx 0.9), so a
    /// WAL database whose owner is dropped without `close()` leaves the
    /// sidecar files behind — callers that care about teardown (tests,
    /// embedders) must await this before dropping the database handle.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}
```

**Why a public method is required and in-scope.** Integration tests
cannot reach `repo.pool` (`pub(crate)`), and an awaited close cannot run
inside `Drop`. The dispatch's own fix phrasing — "dispose/close the pool
(and raw connections) before the temp handle drops — an RAII fixture
wrapper **or explicit close at test end**" — presupposes a close surface;
the same section's non-goal ("no storage production changes") is read
here as *no behavior/schema/query changes*, which an additive,
behavior-neutral disposal method satisfies. Rejected alternatives:

- **`pub fn pool(&self)` accessor** — leaks the `sqlx` concrete type into
  the crate's public API (sqlx is otherwise an implementation detail
  behind `OpenError`/`RepositoryError`); strictly worse encapsulation.
- **Making `pool` `pub`** — same objection, worse.
- **Test-side sidecar unlink only** (no real close) — hides the defect
  class instead of fixing teardown discipline; makes the guard test
  vacuous (the wrapper itself would delete the siblings before any
  assertion could distinguish clean from dirty teardown).
- **Converting all sites to `TempDir` + `dir.join("x.db")`** (recursive
  cleanup removes sidecars for free) — same hiding effect; also makes
  the leak impossible to detect rather than fixed; the issue explicitly
  prefers disposing the pool.
- **Moving the fixtures into `src/` unit tests** — wrong tier; the
  roundtrip tests are integration tests by design (docs/testing_strategy
  tiering; they exist as `tests/roundtrip.rs` today).
- **Putting `close()` on the `TaskRepository` port** — drags a
  disposal concern into the runtime-agnostic domain port; fakes/memory
  repositories need no teardown. Concrete inherent method only.

`close(&self)` mirrors `SqlitePool::close(&self)`'s shape (sqlx-core
0.9.0 `pool/mod.rs:441`); it is safe to call on a clone (pool closure is
global, like sqlx's own) and idempotent.

### D2 — awaited close is mandatory: probe evidence (the load-bearing fact)

A throwaway probe (run 2026-10-18 in this worktree, since deleted;
WAL/`synchronous=NORMAL`/FK/busy-timeout pragmas identical to
`connect.rs`, single-connection pool) measured three teardown patterns
on `dir.path().join("x.db")` after one DDL + one DML:

| Pattern | `-wal`/`-shm` while open | right after `drop(pool)` | 500 ms later |
|---|---|---|---|
| A: plain `drop(pool)` | present / present | **present / present** | **present / present** |
| B: `pool.close().await` | present / present | **gone / gone** | gone / gone |
| C: `drop(pool)` then unlink main db (today's effective order) | present / present | — | **present / present** (leak) |

Mechanism (read from the vendored sqlx-core 0.9.0): `PoolInner::drop`
(`pool/inner.rs:441`) only calls `mark_closed()`; idle connections are
dropped raw and the sqlite worker thread's eventual `sqlite3_close`
either never runs in time or never performs the checkpoint+unlink that a
joined close does (`PoolInner::close` awaits every
`raw.close().await`, which shuts the worker down and joins it —
`sqlx-sqlite connection/mod.rs:220-235`). Conclusion: **drop-order
correctness is insufficient; only an awaited close is deterministic**.
This also settles the guard test's determinism: post-`close()` sidecar
absence is synchronous (pattern B), and post-plain-drop sidecar
*presence* is stable on the timescale of a test assertion (pattern A) —
so the stage-2 red state (§test-design) fails for real, not flakily.

### D3 — #323 fixture shape: **extend `actions_from_state`** (+ P1 assertion)

Decision: extend the existing helper; no dedicated fixture.

- `persisted_state_strategy` **already generates full reminder states**
  (`test_support/state.rs:390` → `app_state_strategy` →
  `test_support/reminders.rs::build_reminders` populates rules,
  deliveries, snoozes, muted — referentially consistent,
  invariant-satisfying by its own proptests). The randomized tier is
  ready; only the batch fixture and the assertion are blind.
- The bench support module already set the precedent:
  `benches/support/mod.rs::whole_state_actions` extends the same
  vocabulary with `UpsertReminderRule` (with an explicit comment that it
  mirrors `tests/common/mod.rs`'s vocabulary). A second, parallel
  whole-state fixture would drift from both.
- A dedicated "reminder fixture" would have to duplicate the seven
  entity arms to produce a valid whole state — duplication with no
  customer.

Concretely, after the outbox arm and before the validators arm
(mirroring the bench's order, sync status stays terminal):

```rust
for rule in state.reminders.rules.values() {
    actions.push(PersistenceAction::UpsertReminderRule(rule.clone()));
}
for delivery in &state.reminders.deliveries {
    actions.push(PersistenceAction::RecordReminderDelivery(*delivery));
}
for (key, rescheduled_to) in &state.reminders.snoozes {
    actions.push(PersistenceAction::RecordReminderSnooze(ReminderSnooze {
        reminder: key.reminder,
        occurrence: key.occurrence,
        rescheduled_to: *rescheduled_to,
    }));
}
for task in &state.reminders.muted {
    actions.push(PersistenceAction::SetTaskRemindersMuted(*task, true));
}
```

and in P1 (`persisted_state_roundtrips_through_sqlite`), one new
assertion after `validators`:

```rust
prop_assert_eq!(loaded.reminders, state.reminders);
```

The helper's doc comment updates to name the reminder partitions
("entity upserts, outbox appends, reminder facts, validator upserts,
and the sync status").

### D4 — what #323 does *not* add, and the corrected coverage map

The issue's premise — the reminder partitions are "exercised only
end-to-end by `taskboard-app`'s `tests/cli_reminders.rs`" — is stale:
the domain contract harness gained a reminder section in `53ac27c`
(2026-10-13) covering **all five variants with the sqlite adapter**
(both `:memory:` and file, via `tests/contract.rs` C1/C2):

| Variant | Covered today (crate tier) | After this chunk |
|---|---|---|
| `UpsertReminderRule` | contract harness (fixed scenario) | **+ randomized whole-state tier (P1)** |
| `RecordReminderDelivery` incl. replay set-union | contract harness (:287-309) | **+ P1** |
| `RecordReminderSnooze` incl. same-key replace | contract harness (:315-336) | **+ P1** |
| `SetTaskRemindersMuted` true/false | contract harness (:343-353) | **+ P1 (true arm)** |
| `DeleteReminderRule` + delivery/snooze prune | contract harness (:386) | unchanged (event-shaped, snapshot-underivable) |

So this chunk adds **no targeted reminder micro-tests** — replay-union,
snooze-replace, unmute, and delete-prune are already pinned at this very
tier by C1/C2, and AGENTS §5 (one behavior, one test, lowest layer)
forbids re-pinning them through the proptest file. The genuine gap is
exactly the one the issue's root-cause sentence names, and D3 closes it.

Bookkeeping correction the implementation carries into the helper docs:
`PersistenceAction` now has **15** variants (the issue says 14;
`ClearBoardContent` landed after it, chunk 15 of the board-stack round).
Snapshot-derivable set: 7 current + 4 reminder = 11. Event-shaped four:
`CompleteOp`/`FailOp` (roundtrip.rs P2), `DeleteReminderRule` +
`ClearBoardContent` (contract harness via C1/C2 — ClearBoardContent at
`test_support/contract.rs:396`).

### D5 — the sidecar guard tests: two, path-scoped, in a new `tests/fixture_hygiene.rs`

- `closed_file_databases_leave_no_wal_sidecars`: build a `FileDb`,
  `apply` one write, pre-assert non-vacuity (`wal.exists() ||
  shm.exists()` — a disjunction so the precondition stays tolerant of
  platform differences in sidecar creation timing; the write-while-open
  existence is already unix-pinned by
  `connect.rs::wal_and_shm_siblings_are_owner_only_while_the_database_is_open`),
  then `db.close().await` and assert `!wal.exists() && !shm.exists()`
  (probe pattern B: sqlite removed them on the clean close — this is
  the strong assertion, "closed cleanly", not "cleaned up after").
- `dropping_without_close_still_leaves_no_sidecars`: build, write,
  plain `drop(db)`; assert `!wal.exists() && !shm.exists() &&
  !db_path.exists()` — pins the `Drop` backstop's synchronous unlinks
  (deterministic: they run inside `Drop` before it returns).

Both tests check **only the two exact sibling paths of the fixture's own
file** (`<path>-wal`, `<path>-shm`) plus the fixture's own path — never a
`/tmp` scan, per the dispatch constraint. On the "don't test
test-helpers" rule (AGENTS §5): these guards are the acceptance
instrument the issue and dispatch explicitly prescribe for #313 — the
fixture's teardown discipline *is* the deliverable, and it is exactly
the kind of environmental contract that silently rots (as the 10,923
leaked files prove); the generic rule targets trivial helpers.

**Stage-2 TDD sequencing (red before green, per prompt 4.2):**

1. Write `FileDb` with `close()` implemented as drop-only (no awaited
   pool close) + both guard tests → run
   `cargo nextest run -p taskboard-storage-sqlite` → guard 1 fails
   deterministically (probe pattern A: sidecars present after drop).
   This is the required "fails against the unconverted pattern" state.
2. Add the `close()` seam to `SqliteTaskRepository` and make
   `FileDb::close` await it → guard 1 goes green; guard 2 stays green
   (backstop unlinks are independent).
3. Extend `actions_from_state` and P1 in the same red-first spirit: add
   the `prop_assert_eq!(loaded.reminders, state.reminders)` line first,
   run once to see it fail against the unextended helper (loaded
   reminders are default-shaped), then add the four emission arms.
4. Convert every site (§site-audit), run the full AGENTS §10 gate.

### D6 — IPC-tempdir fold-in verdict: **NO** (evidence recorded)

The three leftover dirs (`/tmp/.tmp{8CCkPu,18scaK,AvkfpP}` with
`zz.db`/`zz.lock`/`zz.sock`, 2026-10-11) are **not reproducible by
current code**:

- No `zz` fixture name exists anywhere in the tree (grep over
  `crates/taskboard-app/{src,tests}`) — the fixtures died with the
  2026-10-11 IPC-test era.
- Current IPC tests are `tempfile::tempdir()`-based
  (`src/ipc/client.rs:321`, `src/ipc/server.rs:578`) — recursive
  cleanup removes any content.
- The kill-based tests that exist today (`cli_tty.rs:760`,
  `cli_ipc.rs`, `cli_lock.rs`, `mcp_smoke.rs`) kill daemons whose
  artifacts live in **test-owned** `TempDir`s (`common::tempdir()`), so
  the test's own recursive cleanup removes them; daemon shutdown
  elsewhere is graceful (`shutdown_timeout_secs` config,
  `common/mod.rs:469`). A SIGKILLed process's own `TempDir` can never
  self-clean — that is inherent to kill-based tests, not a code defect,
  and fixing it would mean prod daemon tmp handling, which the dispatch
  rules out of scope.

The stale dirs are one-time debris; the user may clear them with
`rm -rf /tmp/.tmp{8CCkPu,18scaK,AvkfpP}` (recorded here; agents do not
run it — deleting files outside the repo is the user's call). Same for
the 10,923 sidecar files themselves: this chunk stops the regrowth; it
does not sweep history.

### D7 — unit-tier sites use the same public seam; TempDir-based sites stay as-is

`src/connect.rs`'s in-crate tests convert to `repo.close().await` (the
same public seam — one idiom everywhere) and the single unclosed raw
pool (`connect.rs:170`, `reopening_applies_no_new_migrations`) gets an
explicit `raw.close().await`. The three `tempfile::tempdir()`-based
F12-01 tests (`connect.rs:536/551/572`) and `benches/hot_path.rs` are
**not converted**: a `TempDir`'s recursive cleanup removes sidecar
siblings by construction, so they cannot leak; churning them would be
noise (recorded so stage-3's no-straggler grep doesn't flag them).

## §site-audit — every db-creating test site, complete

Grep basis: `tempfile|tempdir|SqlitePool::connect|open(` over
`crates/taskboard-storage-sqlite/{src,tests,benches}` (2026-10-18,
chunk-3 tip `c74559e`; `open_memory` sites create no files and are
listed for completeness only).

### Integration tests (`tests/`) — convert to `FileDb`

| # | Site | Today | Conversion |
|---|---|---|---|
| 1 | `tests/roundtrip.rs:298` `failed_batch_rolls_back_atomically` (T6) | NamedTempFile + `open`; raw pool closed explicitly (:335); repo never closed | `FileDb`; raw keeps its explicit close (via `db.path()`) |
| 2 | `tests/roundtrip.rs:351` `rename_board_op_lands_in_the_board_column` (T9) | same shape; raw closed (:371); repo never closed | `FileDb` |
| 3 | `tests/roundtrip.rs:386` `corrupt_and_load` helper (T7, 9 call sites) | file+repo+raw per call; raw closed (:395); repo never closed | helper holds `FileDb`; `let outcome = db.repo().load().await; db.close().await; outcome` |
| 4 | `tests/roundtrip.rs:459` `state_survives_reopen` (T8) | NamedTempFile + two sequential `open`s; first closed by plain `drop(repo)` (:474 — not awaited); second never closed | `FileDb` close→reopen flow (D1 API) |
| 5 | `tests/contract.rs:17` `sqlite_file_repository_satisfies_contract` (C2) | NamedTempFile + `open`; never closed | `FileDb` |

No-ops: P1–P4, T5, `per_board_validators_roundtrip` (all
`open_memory`); `tests/actor.rs` (memory); `tests/bench_fixtures.rs`
(memory); C1 (memory).

### Unit tests (`src/connect.rs` `#[cfg(test)]`) — add awaited closes

| # | Site | Today | Conversion |
|---|---|---|---|
| 6 | `:164` `reopening_applies_no_new_migrations` | first repo plain-`drop`ped (:166); second repo never closed; **raw pool (:170) never closed** | `drop(first)` → `first.close().await`; close second repo; `raw.close().await` |
| 7 | `:185` `migration_0002_upgrades_a_pre_clock_database` | raw closed (:210); repo never closed | close repo at end |
| 8 | `:230` `migration_0005_upgrades_a_pre_edit_verb_database` | raw closed (:277); repo never closed | close repo at end |
| 9 | `:321` `migration_0003_backfills_legacy_rename_board_rows` | raws closed (:392, :429); repo plain-`drop`ped (:403) | `drop(repo)` → `repo.close().await` |
| 10 | `:438` `migration_0004_upgrades_a_pre_0004_database` | raw closed (:492); repo never closed | close repo at end |
| 11 | `:507` `journal_mode_is_wal` | repo never closed | close repo at end |
| 12 | `:536/551/572` F12-01 trio | `tempdir()`-based | **no conversion** (D7) |

### Benches / other crates

- `benches/hot_path.rs` — TempDir-based, outside the test gate → no
  conversion (D7).
- `taskboard-app` tests use `common::tempdir()` (dir-based) for their
  db paths → no leak by construction, and outside this chunk's diff
  boundary (dispatch: diff confined to storage tests + domain fixtures
  if needed — none are).

## §test-design — new tests

1. `tests/fixture_hygiene.rs::closed_file_databases_leave_no_wal_sidecars`
   (guard, D5) — red against drop-only `FileDb`, green with awaited
   close.
2. `tests/fixture_hygiene.rs::dropping_without_close_still_leaves_no_sidecars`
   (backstop pin, D5).
3. `tests/roundtrip.rs` P1 extension: `prop_assert_eq!(loaded.reminders,
   state.reminders)` — red until `actions_from_state` emits the four
   reminder arms (D3).

No other new tests: the five reminder variants' targeted semantics are
already pinned at this tier by the contract harness (D4); the converted
sites keep their existing assertions byte-identical (fixture mechanics
only — "the other seven variants' coverage unchanged" acceptance).

Assertion-style notes: guards assert filesystem existence facts
(structural, allowed); P1 extension is a typed equality; no text
assertions anywhere; no sleeps (the guards' post-conditions are
synchronous by construction — probe B / Drop-time unlinks).

## §non-goals

- No production behavior/schema/query changes beyond the additive
  `close()` disposal seam (D1 records the non-goal interpretation).
- No changes to `taskboard-app`'s `tests/cli_reminders.rs`, no new
  app-tier tests.
- No changelog edits (chunk 5 owns `[Unreleased]`).
- No sweep of the 10,923 historical `/tmp` files or the three IPC dirs
  (D6 records the user-side commands; regrowth prevention is the fix).
- No bench changes; no contract-harness changes; no port-trait changes.
- No `#[non_exhaustive]`/enum changes (execute-batch §7 rules stand).

## §risks

| Risk | Mitigation |
|---|---|
| P1's new assertion may expose a real reminder codec roundtrip bug (e.g. `ReminderSnoozeKey` text-form or sub-second precision loss through the TEXT columns) — randomized states hit wider value domains than the contract's fixed scenario | That is the test working. AGENTS §5 protocol: do not weaken; if unfixable in-chunk, leave failing, report, open a follow-up issue with the failing case |
| Stage-3 reads "no storage production changes" absolutely and rejects `close()` | D1 pre-argues the interpretation against the dispatch's own "explicit close at test end" phrasing; if still vetoed, the fallback is the Drop-backstop-only wrapper — strictly weaker (guard 1 then pins nothing) and recorded as a deviation |
| Windows CI: sidecar creation timing / sharing violations | Non-vacuity pre-assert is a disjunction; post-close/post-drop assertions are absence-only; Drop-backstop unlinks are best-effort (`let _ =`) — primary path stays the awaited close |
| Drop-backstop unlinks race the sqlite worker thread | Harmless by construction on POSIX (unlink of an open file); the worker's close only ever unlinks too — it never recreates sidecars |
| sqlx upgrade (0.9 → future) changes Drop/close semantics | Guard test 1 fails loudly on drift — that is its job |
| Reminder-emission order matters somewhere | All four partitions iterate `BTree*` collections — deterministic order; `apply` is order-insensitive for these upserts (contract-pinned replace/union semantics) |

## §stage-2 execution checklist

1. Sequencing exactly as D5: drop-only `FileDb` + guards (red) →
   `close()` seam + awaited `FileDb::close` (green) → P1 assertion (red)
   → `actions_from_state` arms (green) → site-audit conversions.
2. Commit plan (each standalone, rebase-merge hygiene):
   - `feat(storage): add a close() disposal seam for clean WAL teardown (#313)`
   - `test(storage): convert file-database fixtures to close-before-drop (#313)`
   - `test(storage): roundtrip reminder partitions through the whole-state batch (#323)`
   (guards land with the second; if the codec-bug risk materializes, the
   third commit may carry a follow-up issue number instead of a green
   assertion — report it either way.)
3. Full AGENTS §10 gate suite green; delete `target/llvm-cov-target`
   after `lcov.info` (AGENTS §10 trailing step).
4. No push, no PR, no CHANGELOG.md (chunk 5 ships the round).
5. Report: per-gate results, the red→green evidence for both TDD
   threads, any deviation from this plan and why.

**Estimated diff**: ~230–280 lines over 6 files — `src/repo.rs` (+10),
`tests/common/mod.rs` (+~85: `FileDb` + reminder arms + docs),
`tests/roundtrip.rs` (~50 churn + 1 assertion), `tests/contract.rs`
(~5), `tests/fixture_hygiene.rs` (new, ~55), `src/connect.rs` (~15).
Test-only except the seam.
