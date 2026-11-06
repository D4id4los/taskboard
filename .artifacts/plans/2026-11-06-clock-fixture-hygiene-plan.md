# [Plan] Clock fixture hygiene below the UI (chunk 4 of the clock-seam remediation)

- **Date**: 2026-11-06
- **Kind**: stage-1 implementation plan, chunk 4 ("fixture hygiene
  below the UI") of
  `.artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md`.
- **Normative origin**: `.artifacts/plans/2026-11-06-clock-remediation-plan.md`
  §0 (scope), §1 (decisions D-1…D-6), §5 (this chunk); audit report
  `.artifacts/reports/2026-11-06-clock-seam-audit-report.md` §4.3 row
  C6; ruling **R4** (replace `StepClock`). The rule being enforced:
  `docs/testing_strategy.org` §10 — *"Fixture builders must not read
  the clock: they stamp fixed, distinct, strictly increasing
  timestamps … so ordering semantics stay exercised while the tests
  run interpreted."*
- **Branch / worktree**: `test/clock-fixture-hygiene` at
  `../taskboard-WT-test-clock-fixture-hygiene`, off `main` @ `154167f4`.
- **Nature of the diff**: test-only. No production code changes; no
  snapshot, golden, or fixture-file regeneration; no new dependencies.

## 0. Scope

Every inline-test wall-clock read in the four stamped-actor crates
named by audit row C6, plus the R4 `StepClock` replacement:

| # | Site (`main` @ `154167f4`) | Read | Consumer that would observe it |
|---|---|---|---|
| S1 | `crates/taskboard-state/tests/engine.rs:1502` | `Some(chrono::Utc::now())` as the stale `last_success` of a seeded `SyncPhase::Failed` row | `:1521` asserts `row.last_success == None` after `UnbindBoard` — the value is only ever *dropped*; it is never compared |
| S2 | `crates/taskboard-sync-nextcloud/src/push_exec/tests.rs:46` (`op()` → `queued_at`) | `chrono::Utc::now()` | No test in the module reads `queued_at` back (grep: zero `queued_at` outside the builder); the executor forwards the op id, never the stamp |
| S3 | `push_exec/tests.rs:103` (`stack_row` → `StackClocks`), `:122` (`label_row` → `LabelClocks`), `:156` (`board_row` → `BoardClocks`) | one `let now = Utc::now()` fanned into every clock field | No assertion touches `clocks.*` (grep: zero hits); rows feed `tables_with`/`tables_with_boards` whose consumers read `id`/`remote`/`title`/`order`/`color` only |
| S4 | `crates/taskboard-sync-files/src/identity.rs:96` | `created_at: chrono::Utc::now()` in the serde round-trip test | The assertion is `decode(encode(x)) == x` — any instant round-trips; sibling tests at `:57/:69/:80/:86/:107` already use the RFC-3339 literal `"2026-11-05T09:15:32Z"` |
| S5 | `crates/taskboard-sync-files-nextcloud/src/device.rs:139/:156/:175/:191` | `FixedClock(Utc::now())` | The four tests assert temp-residue absence, the `Unusable` refusal + untouched bytes, the decoded `device_id`, and the `Io` path shape — none reads `created_at`. The asserting sibling `:124` (`create_once_…`) already uses `FixedClock("2026-11-05T09:15:32Z".parse()…)` and asserts `identity.created_at == clock.0` |
| S6 (R4) | `crates/taskboard-sync-files-nextcloud/src/actor.rs:213-222` (`StepClock`) | `Utc::now() + 30 s` **per read** behind a doc comment claiming "an advancing logical clock" | The actor stamps heartbeats with it; the suite asserts request counts, park kinds, and status transitions — no test asserts a heartbeat instant. The stamps must merely be *distinct and increasing* |

Confirmed by reading each consumer: **no assertion observes any of
these values today.** The change is stamp replacement only (origin §5).

**Non-goals** (origin §0): no production diffs; no `cfg_attr(miri,
ignore)` added anywhere — S1 in particular must simply stop reading
the clock, not be gated; no change to `tests/common/mod.rs`
(`SteppingClock` there is the model, not a target); the live/docker
tiers and the `sync_nextcloud` actor suite are untouched; the
`manifest_routing.rs:26` private `ts()` duplicate in the state suite
is out of scope (it is already fixed-stamp).

## 1. Decisions

- **D-C6-1 — one fixed base per module, chosen to match what the
  module already prints.** Every replacement stamp is derived from a
  base that the file's neighbouring fixtures already use, so a reader
  sees one time family per file:
  - state `engine.rs`: the suite's `common::ts(common::T0)` block
    (`T0 = 36_000`, 10:00:00Z of the epoch day) — the plan's named
    helper, already imported via `mod common;`.
  - `push_exec/tests.rs`: base `1_700_000_000` — the literal already
    stamped as `lastModified` in `board_json` (`:92`), so the row clocks
    and the wire fixtures share one epoch.
  - `identity.rs` and `device.rs`: the literal `"2026-11-05T09:15:32Z"`
    their siblings already use.
  - `actor.rs`: base `2026-11-05T09:00:00Z` — one hour after the
    fixture stamp `"2026-11-05T08:00:00Z"` at `:246`, so every minted
    heartbeat compares *newer* than every fixture stamp (the same
    "pinned after the fixtures' base" rationale as
    `sync-nextcloud/tests/common/mod.rs:25-28`).
- **D-C6-2 — distinct where ordering could ever matter (§10's
  "distinct, strictly increasing").** `op()` stamps `queued_at` from
  the op's own `raw` id (`T0 + raw`) so two ops in one test never
  share a stamp and queue order follows id order. The row-clock
  builders give each clock field its own offset (`title: T0`,
  `order: T0 + 1`, `deleted: T0 + 2`, `color`/`archived` likewise) —
  a future LWW assertion gets a well-ordered fixture for free instead
  of a tie. S1, S4, S5 are single-stamp sites with no ordering
  partner; a single fixed literal is correct there.
- **D-C6-3 — the ts-helper question in `push_exec`.** The module has
  no `ts()`; add one, private to the test module, mirroring the state
  suite's shape exactly (`fn ts(secs: i64) -> DateTime<Utc>` over
  `Utc.timestamp_opt(secs, 0).unwrap()`) plus `const T0: i64 =
  1_700_000_000`. Not shared across crates (no new test-support
  surface in `taskboard-domain`; the three existing `ts()` blocks in
  the workspace are each module-local too — this follows the house
  pattern rather than inventing a fourth home).
- **D-C6-4 — the R4 replacement: atomic base + step, renamed
  `SteppingClock`.** Replace the unit struct with
  `struct SteppingClock { next_secs: AtomicI64, step_secs: i64 }` and
  `fn new(start: DateTime<Utc>, step_secs: i64)`, byte-for-byte the
  shape of `sync-nextcloud/tests/common/mod.rs:43-68` (fetch-add,
  `Relaxed`, `timestamp_opt(..).single().expect(..)`). **Rename** to
  `SteppingClock`: the two crates then name the same idea the same
  way, and the old name is a one-site private type (`cfg()` at
  `:229`), so the rename costs one line. Construction becomes
  `Arc::new(SteppingClock::new(HEARTBEAT_BASE, 30))` where
  `HEARTBEAT_BASE` is a module const parsed from the literal in
  D-C6-1. The doc comment is rewritten to tell the truth: *a logical
  stepping clock — 30 s per read from a fixed base; heartbeat stamps
  stay distinct and strictly increasing without any wall read.*
  Step stays 30 s (the existing semantics the suite was written
  against).
- **D-C6-5 — `device.rs` helper.** Four identical constructions
  justify a one-line `fn fixed_clock() -> FixedClock` in the test
  module returning the literal; `:124` keeps its inline construction
  because it *asserts* against `clock.0` and reads better with the
  value visible. (A helper is preferred over four repeated parse
  expressions; it is not production code and tests nothing itself.)
- **D-C6-6 — no new tests, no weakened tests.** Every assertion stays
  verbatim. A fixture hygiene pass does not add behaviour; the TDD
  loop below is "run red-free before and after" plus the §10 grep
  oracle, not a new failing test.

## 2. TDD discipline for a hygiene pass

There is no new behaviour to drive red→green, so the loop is the
*oracle-first* form:

1. **Write the oracle first**: the acceptance grep from origin §5
   (below) must go from 11 hits to 0 — run it before any edit and
   record the count.
2. **Per site**: run the owning test binary (filtered to the touched
   tests) green **before** the edit; apply the stamp replacement; run
   again green. A red after the edit means the stamp *was* observed
   (the audit's claim was wrong for that site) — stop, re-read the
   consumer, and report rather than adjust the assertion (AGENTS §5).
3. **R4 specifically**: the heartbeat-stamp distinctness the old doc
   promised is now actually true; after the swap, run the full
   `actor.rs` test module and confirm the status-transition and
   request-count tests are unchanged.
4. **Miri statement**: `engine.rs` is in the weekly Miri scope
   (`cargo miri test -p taskboard-state`). After S1, the
   `unbind_arm_…` test has no realtime read; **do not** add a gate.
   Verifying under the interpreter locally is optional (slow); the
   grep oracle plus the §10 statement is the acceptance.

## 3. Steps (one commit each, each a correct build)

1. `test(state): stamp the stale sync row from the fixed ts block` —
   `engine.rs:1502` → `Some(common::ts(common::T0))`. Comment stays
   (it explains *why* the row exists, which is unchanged).
2. `test(sync-nextcloud): fixed push_exec fixture stamps` — add
   `const T0` + `fn ts()`; `op()` → `queued_at: ts(T0 + raw as i64)`
   (`raw` is `u128`; the ids in this file are small literals — use
   `i64::try_from(raw).expect("small fixture id")` to keep the cast
   honest); `stack_row`/`label_row`/`board_row` drop `let now` and
   stamp per-field offsets per D-C6-2.
3. `test(sync-files): fixed created_at in the identity round trip` —
   `identity.rs:96` → `"2026-11-05T09:15:32Z".parse().expect("rfc3339")`.
4. `test(sync-files-nextcloud): fixed device clocks and a stepping
   heartbeat clock` — `device.rs` helper + four call sites (D-C6-5);
   `actor.rs` `StepClock` → `SteppingClock` (D-C6-4). One commit:
   same crate, same theme; both files stay independently correct.
5. Run the §10 gates through `scripts/run-quality-gates.sh`; fmt and
   clippy must be clean (`-D warnings` — the `AtomicI64` import and
   the unused-`Utc` import in `actor.rs`/`device.rs` after the swaps
   are the likely nits).

## 4. Acceptance (origin §5, restated as checks)

```sh
grep -rn "Utc::now()" \
  crates/taskboard-state/tests \
  crates/taskboard-sync-nextcloud/src/push_exec/tests.rs \
  crates/taskboard-sync-files/src/identity.rs \
  crates/taskboard-sync-files-nextcloud/src/device.rs \
  crates/taskboard-sync-files-nextcloud/src/actor.rs
```

- returns **zero** hits (baseline on `main`: 11 — 1 + 4 + 1 + 4 + 1);
- `grep -rn "cfg_attr(miri" crates/taskboard-state/tests/engine.rs`
  shows **no new** gate;
- `cargo nextest run -p taskboard-state -p taskboard-sync-nextcloud
  -p taskboard-sync-files -p taskboard-sync-files-nextcloud` green with
  the same test count as before (no test added or removed);
- full `scripts/run-quality-gates.sh` green; no snapshot or golden
  diff (none of these crates owns one that embeds these stamps —
  verified: the 78 insta snapshots carry only T0/DAY0/epoch content,
  audit §2).

## 5. Risks

- **A stamp turns out to be observed** (contradicting the audit): the
  only plausible candidate is `push_exec` row clocks feeding an LWW
  compare inside the executor. Grep says no (`clocks` appears nowhere
  in the module's assertions), and D-C6-2's strictly-increasing
  per-field offsets would satisfy any ordering compare anyway. If a
  test still goes red, that is a finding to report, not a reason to
  touch the assertion.
- **`SteppingClock` base vs fixture stamps**: if any `actor.rs` test
  stamps a fixture *after* 09:00:00Z and relies on the heartbeat being
  newer, the base must move later. Reading the module, the only
  literal is `08:00:00Z` (`:246`); the stage-2 agent re-greps
  `"2026-11-05T` in `actor.rs` before committing the base.
- **Shared-`T0` collision in `push_exec`**: `T0 = 1_700_000_000`
  equals `board_json`'s `lastModified`. That is intentional (one
  epoch per file) and harmless — the wire fixture and the local row
  are never compared against each other in this module. If a future
  test wants "remote newer than local", it offsets explicitly.
- **Clippy on the `u128 → i64`** cast in `op()`: use `try_from` +
  `expect`, never `as` (the house prefers fallible minting; AGENTS §2).

## 6. Estimated diff

Five files, roughly **+45 / −25 lines**, four commits, test-only:
`engine.rs` (±1), `push_exec/tests.rs` (~+15/−6), `identity.rs` (±1),
`device.rs` (~+6/−4), `actor.rs` (~+20/−8). No production file is
touched; no artefact regenerates.
