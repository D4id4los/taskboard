# Test-Support Residue — Chunk D1, Round 6 — Implementation Plan

Branch: `test/test-support-residue` (off `origin/main` @ 8cc77780, 2026-11-24)
Issues: #1264, #1269, #1267, #1273, #1271, #1293
Scope report: `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md` §D
Dispatch plan: `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md` §12

## 0. Stage-1 survival verdicts (verified against origin/main 2026-11-24)

| Issue | Verdict | Evidence |
|---|---|---|
| #1264 | **ALIVE** | 15 local clock-double definitions across 13 sites (census below, grown from the ~7 the issue named) |
| #1269 | **ALIVE** | `test_support::entities` has zero `mint_*` constructors; `seed_board/stack/task/label` in `crates/taskboard-app/src/test_support.rs:212-293` still hand-roll struct literals |
| #1267 | **ALIVE** | `age_phrase` at `crates/taskboard-app/src/commands/status.rs:535`; only the structural injectivity pin (`status.rs:1457`, #1229) exists — no boundary table |
| #1273 | **ALIVE** | raw mounts at `crates/taskboard-ui-slint/tests/journeys/usability_components.rs:681-691` and `:763-773` (exact `ArcSwap::from_pointee` + `FakeEngine::channel` + `dummy_signals` + `dummy_system` + `detached_health`); `dummy_signals`/`dummy_system` defined in the same file (`:24`, `:29`) |
| #1271 | **ALIVE** | strict serde decode at `crates/taskboard-storage-sqlite/src/repo.rs:906`; mint law in force (`HabitSnooze::hours(0) == None`, `habit.rs:4463`) with the documented decode leniency; no normalization pass exists |
| #1293 | **ALIVE** | pass 2 of `resolve_journal_entry` (`crates/taskboard-app/src/commands/track.rs:584-594`) fills `AmbiguousJournalEntry.candidates` with `TimeEntryId` uuids while pass 1 (`:567`) and the prose use journal ids |

None of the six was closed by an earlier round; no drops.

## 1. #1264 — fold the clock doubles onto one shared domain impl

### Census (before, measured on origin/main 2026-11-24)

| Definition | Site |
|---|---|
| `FakeClock(AtomicI64)` | `crates/taskboard-app/src/escalation.rs:1182` (cfg(test)) |
| `FakeClock(AtomicI64)` | `crates/taskboard-app/src/focus.rs:802` (cfg(test)) |
| `FixedTestClock` | `crates/taskboard-app/src/bootstrap.rs:3118` (cfg(test)) |
| `FrozenClock` | `crates/taskboard-app/src/test_support.rs:99` (pub(crate)) |
| `FrozenClock` | `crates/taskboard-app/tests/core/bootstrap.rs:1242` |
| `FrozenClock(Mutex<i64>)` | `crates/taskboard-app/tests/services/files_sync_convergence.rs:73` |
| `StepClock(AtomicI64)` | `crates/taskboard-app/tests/services/files_sync_convergence.rs:100` |
| `FixedClock(Mutex<i64>)` | `crates/taskboard-state/tests/common/mod.rs:26` |
| `FrozenClock` | `crates/taskboard-state/tests/suite/journal_degraded.rs:78` |
| `FixedTestClock` | `crates/taskboard-state/tests/proptest.rs:793` |
| `FixedClock` | `crates/taskboard-sync-files-nextcloud/src/device.rs:107` (cfg(test)) |
| `FixedClock` | `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs:1734` (cfg(test)) |
| `TestClock` | `crates/taskboard-sync-files-nextcloud/tests/suite/retained_etag_wiremock.rs:336` |
| `FixedClock(DateTime<Utc>)` | `crates/taskboard-sync-nextcloud/tests/common/mod.rs:30` |

**Before: 14 definitions / 13 files.** (The issue's "~7" undercounted; every
one folds.)

### Decision

One shared home: `taskboard-domain::test_support::clock`, behind the
existing `test-support` feature, with two types:

- `FrozenClock(DateTime<Utc>)` — one instant, `Clock` trait impl.
- `ManualClock(Mutex<DateTime<Utc>>)` — `set`/`advance` for step tests
  (subsumes StepClock/Mutex-i64 variants; chrono keeps the unit explicit).

All 14 definitions are deleted and their use sites import the shared
impls. Crates needing it add `taskboard-domain` (or
`taskboard-sync-files`) dev-dependency with `features = ["test-support"]`
— the pattern `taskboard-state`/`storage-sqlite`/`sync-nextcloud`
already use. `taskboard-ui-slint` has no clock double and needs nothing.

The C1-added `crates/taskboard-app/src/test_support.rs` `FrozenClock`
re-export is subsumed (the pub(crate) shim either dies or re-exports the
domain type; the domain import is the only surviving spelling).

**Measured after-target: 1 definition site (the domain module), 0 local
doubles.** The plan records before=14 / after=1 as the #1264 measurement.

Trait seam check happens at implementation time: all doubles implement
the domain `Clock` port (that is what makes the fold mechanical); if any
site implements a *different* local trait, that trait's consumers move
to the port and the local trait dies too — diagnosis-first, nothing
bypasses the port.

Tests: none new for the fold itself (no behavior change); the fold is
proven by the existing suites compiling and passing against the shared
impls. Dedup law: one behavior, one test — no per-crate re-pins of
clock freezing semantics.

## 2. #1269 — deterministic entity mints in `test_support::entities`

Add, next to the strategies (same module, flat re-export from
`test_support`):

- `mint_board(raw: u128, title: &str) -> Board`
- `mint_stack(raw: u128, board: BoardId, title: &str) -> Stack`
- `mint_task(raw: u128, stack: StackId, title: &str) -> Task`
- `mint_label(raw: u128, board: BoardId, title: &str) -> Label`

Deterministic: uuids from `Uuid::from_u128(raw)` (same spelling
`CountingIds` uses), clocks at a fixed epoch constant, defaults for every
other field — the A1 house pattern, hand-rolled, documented leniency
(nowhere: these are test-only, outside the trust boundary, so plain
constructors are fine).

Fold sites:

1. `crates/taskboard-app/src/test_support.rs` `seed_board`/`seed_stack`/
   `seed_task`/`seed_label` become one-line delegations (their raw-id
   call contract is preserved so callers don't churn).
2. The file-local struct-literal helpers in the domain's command-test
   modules (`find.rs`, `state.rs`, `ownership.rs`, `push.rs` — the
   `fn board(raw, title)` family) fold onto the mints. In-crate use is
   free (`test_support` is always compiled inside the domain).

Out of scope: proptest strategies stay as they are; the mints are the
deterministic complement, not a replacement.

Tests: a domain unit test pinning determinism (same raw ⇒ same uuid,
twice) — one behavior, one test, lowest layer.

## 3. #1267 — the `age_phrase` fixed-instant boundary table

`age_phrase(then, now)` (`status.rs:535`) cuts at 60 s / 3 600 s /
86 400 s with `<`. The 9 M5 mutants flip each cut to `==`, `>`, `<=`.

Decision — the kill table is **pairwise boundary distinctness**, not
prose equality (text-assertion law: the function's output is prose, so
the oracle is structural: adjacent instants across each cut must render
differently, and instants within a bucket must not):

| then+secs | expected relationship |
|---|---|
| 58 vs 59 | equal (same bucket interior) |
| 59 vs 60 | **differ** (the `<60` cut) |
| 60 vs 61 | differ (interior, guards `==` flips) |
| 3 599 vs 3 600 | **differ** (the `<3600` cut) |
| 3 600 vs 3 601 | differ |
| 86 399 vs 86 400 | **differ** (the `<86400` cut) |
| 86 400 vs 86 401 | differ |
| 172 800 vs 259 200 | differ (day-bucket interior is not flat) |

Fixed instant via `chrono::TimeZone::with_ymd_and_local`-style anchor
(the fn takes instants, not the clock — clock-law compliant; the anchor
is the fixture, no clock seam needed). `assert_ne!` pairs + one interior
`assert_eq!` kill all nine flips: a `<`→`==` mutation makes (59,60)
equal; `<`→`>`/`<=` collapse (58,59) or (60,61); the day-interior pair
kills flat-line replacements already pinned by #1229's injectivity test
(which stays, untouched).

Test home: the existing `mod tests` in `status.rs`, next to the #1229
pin, named `age_phrase_bucket_cuts_distinguish_adjacent_instants`.

## 4. #1273 — the two raw mounts onto the B4a harness

Both probe tests (`:681-691` pixel/tonal probe, `:763-773` segmented
picker) replace their raw `ArcSwap` + `FakeEngine::channel` + `mount(...)`
block with `Harness::mount_on_driver_no_escape_with_settings`
(`tests/support/harness.rs:107`, `UiSettings::default()` fits both —
neither test touches settings).

Then delete from the member file: `dummy_signals` (`:24`),
`dummy_system` (`:29`), `detached_health` import (`:15`), and any
`arc_swap` import left unused. Verify nothing else in the file references
them before deleting (implementation-time grep; if a third site
surfaces, it migrates too — the class, not the count, is the scope).

Tests: none new; the two probe tests' existing assertions are the proof.

## 5. #1271 — zero-duration `HabitSnooze` rows (migration decision)

**Decision (recorded, per round law [L]c): NORMALIZE ON DECODE — rewrite
`HabitSnooze::Hours(0)`/`Days(0)` to the mint default `Hours(1)` at the
single sqlite decode choke point, `repo.rs:906`.**

Rationale, options considered:

- *Normalize-on-decode* (chosen): one site, no schema change, no codec
  duplication; runs for every reader regardless of when the row was
  written; the stored grammar converges lazily. The rewrite happens
  before the strict-serde `HabitReminderOverride` decode result enters
  the domain (post-serde, pre-use), documented at the `HabitSnooze`
  type next to the existing leniency comment (`habit.rs:267`).
- *SQL migration rewriting the reminder JSON column* — rejected:
  duplicates the serde codec in SQL/string surgery and forks the decode
  law.
- *Drop the whole override row* — rejected: discards co-fields (times,
  sound) the CLI wrote correctly.
- *Do nothing* — rejected by the chunk scope; the degenerate value makes
  the snooze chain walk a no-op, silently differing from what the user
  asked (`snooze=0h` refused today, but an old journal still carries it).

Pin: a `storage-sqlite` roundtrip test writes an override payload with
`Hours(0)`, decodes, and asserts the typed outcome
`matches!(ovr.snooze, HabitSnooze::Hours(1))` — garbage-in floor, typed
assertion, no text. The comment at the decode site names #1271 and the
decision; the issue gets the decision as a comment.

## 6. #1293 — typed separation of the ambiguous-prefix candidates

**Decision: the refusal always lists journal ids** — the engine arm's
address, the identity pass 1 lists, and the identity the `--from`
selector ultimately consumes (pass 2 re-resolves to a journal id anyway,
so no usable information is lost). Pass 2's multi-hit arm maps each
`entry_id` hit through its roster row (`row.journal_id`) before filling
`CommandError::AmbiguousJournalEntry.candidates`. The variant stays
single-identity; its doc comment gains one line: "candidates are always
journal ids, in both passes". Renaming per pass (second option in the
issue) is rejected — two identities in one typed error is the bug; a
per-pass rename would canonize it.

Edge: if two roster rows share an `entry_id` prefix but different
journal ids, mapping preserves exactly the journal-id candidate set pass
1 would have produced for those rows — semantics unchanged for scripts.

Pin: an app-tier unit test seeds two roster rows with entry-id prefixes
colliding, asserts `matches!(err, CommandError::AmbiguousJournalEntry{..})`
and that every candidate matches the journal ids' uuid prefixes — typed
structural facts, no prose.

## 7. Implementation order & gates

1. #1264 fold (largest, mechanical) → 2. #1269 mints + folds →
3. #1267 table → 4. #1293 typing → 5. #1271 normalization →
6. #1273 harness migration (ui crate, isolates ui-only fallout last).

Every cargo invocation through `scripts/memory-gate.sh`; the full-suite
gate runs via `scripts/run-quality-gates.sh`; pre-commit hooks are the
commit gate; commit landed work before any wait; if the gate lock is
contended with no other work remaining, commit, report "gate deferred:
lock held by <holder>", end the stage.

## 8. Risks

- **R1 (#1264)**: some cfg(test) doubles may implement a *local* trait,
  not the domain port — the fold then drags in a trait unification.
  Mitigation: diagnosis-first at implementation; the port wins, local
  traits die.
- **R2 (#1269)**: folding the domain-internal literal helpers may churn
  many test lines. Bounded: mechanical s/literal/mint/; snapshot tests
  (insta) unaffected since the minted values equal the old literals.
- **R3 (#1271)**: normalizing on decode mutates read results; if any
  test asserts a zero-duration decode passes through verbatim (the E2
  leniency pin), it gets updated to the normalized expectation — that
  is the decision landing, not a weakened test (verified: the current
  roundtrip suite does not pin pass-through).
- **R4 (#1273)**: the harness may not expose a `None`-window-extras
  variant; if the signature differs, extend the harness rather than
  keep the raw mount (the harness is the shared seam).

## 9. Estimated diff size

~500–800 lines changed across ~16 files: 13 clock-double files (deletions
dominate), `test_support/{mod,entities}.rs` (+~80), app `test_support.rs`
(~−150 net), `status.rs` (+~40), `track.rs` (+~10), `repo.rs` (+~15),
`usability_components.rs` (~−50), 3–4 new/updated tests.
