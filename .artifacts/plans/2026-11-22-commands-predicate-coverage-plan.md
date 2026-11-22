# Plan — Chunk C1: the Commands-Layer Decision Predicates' Fake-Engine Unit Layer (#1231)

- **Date**: 2026-11-22
- **Kind**: chunk plan — test-first coverage spec for chunk C1 of
  backlog-reduction round 5 (branch `test/commands-predicate-coverage`).
- **Origin**: issue #1231 (normative body — its proposal is the
  binding design); the S3 mutants report
  `.artifacts/reports/2026-11-21-cargo-mutants-commands-report.md`
  (the survivor census is the test-target inventory); round-5 scope
  report `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  §C; dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §1 C1 row + §6.
- **Stage**: 1 (planning). Stage 2 implements the tests TDD-first
  (each test lands before — and its mutant kill is evidenced after —
  nothing else changes); stage 3 reviews.
- **Acceptance (from the dispatch plan, binding)**: every decision
  branch in the inventory below has a test whose outcome the surviving
  mutant would flip; no integration-layer duplication
  (one-behavior-one-test — the unit layer is the lowest layer for
  these predicates); kills evidenced by a cargo-mutants re-run over
  the affected files flipping the survivors to caught (or a recorded
  exclusion rationale for the contract-guard class, §4).

## 1. The structural gap (why a new tier)

The commands layer's decision logic — selector ambiguity triage,
outcome mapping, freshness/wait predicates, projection boundaries —
has no unit tier of its own. Every pin that exists today rides the
full `taskboard-app` suite (`standalone_boot_fixture` /
`it_cli.rs`), so cargo-mutants deletes or flips the predicates with
the whole suite green (~30 survivors, #1231's census).

The fix is *not* more integration tests (those duplicate the
routing tier) and *not* mocks (forbidden — fakes over mocks,
AGENTS §5). It is a small fixture seam that makes the decisions
reachable at unit cost, then per-branch flip tests.

## 2. The seam — one shared fake-engine fixture (resolved decision)

**Decision**: the seam lives in the shared
`crates/taskboard-app/src/test_support.rs` (the #981 dedup law —
no per-test-module local doubles). It composes *existing* domain
fakes; the only new code is the wiring:

```rust
// crates/taskboard-app/src/test_support.rs (new, pub(crate))
pub(crate) struct PredicateEngineFixture {
    pub(crate) engine: taskboard_state::EngineHandle,
    pub(crate) join: tokio::task::JoinHandle<()>,
    // deterministic clock + ids the tests may read/advance
}
pub(crate) async fn predicate_engine(state: PersistedState) -> PredicateEngineFixture
```

- Engine actor: the real `taskboard_state::spawn_state_engine`
  (fields of `EngineHandle` are private and there is no fake
  constructor — resolved: we do not fake the *handle*, we fake the
  *world under it*), over
  `taskboard_domain::test_support::InMemoryRepository::with_state(state)`
  (the sanctioned fake repository — fakes-over-mocks).
- Ids: `CountingIds`; clock: a frozen `Arc<dyn Clock>` impl added
  *once* to `test_support.rs` (there are ~7 scattered local
  `FakeClock` impls across the tree — the fold is out-of-scope,
  backlog issue opened; this chunk adds only the one shared impl the
  fixture needs, in the shared file, not per test module).
- Channels: quiet/dropped sync + notifier lanes, `CountingIds`,
  default `SettingsSnapshot`/`DayRollConfig` — the same quiet-lane
  shape `spawn_state_engine` already accepts.
- AppState seeding: `PersistedState` composed from the domain
  `test_support::entities` mints (boards/stacks/tasks/labels/rules/
  alarms/reflection/undo rows as each predicate needs). Tests load a
  snapshot via `engine.shared_state().load_full()` and hand it to
  the `&AppState`-taking predicates directly — so one fixture serves
  both sub-tiers:

  - **Tier A (pure, `&AppState` in)**: `resolve_stack`,
    `resolve_target`, `resolve_task`/`resolve_task_exact_any`,
    `resolve_rule` (reminders + breaks), `timer_rows`,
    `filtered_rows`, `journal_receipt`'s label lookup, `collapse`
    (reminders mode switch — pure over the clap enum, no fixture).
  - **Tier B (async, engine-outcome reachability)**: `add_on`/
    `move_on` (tasks), `add_on`/`update_on`/`rm_on` (habits),
    `read_scope`, `publish_on`'s pre-IO guards,
    `bind_nudge_wait_reconcile` + `wait_for_adoption`,
    `wait_for_sync_result`/`fresh_outcome`. Reachable branches are
    driven by the seeded persisted state (empty board set ⇒ the
    zero-boards arms; duplicate titles ⇒ ambiguity arms; a bound
    board ⇒ the `BoardAlreadyBound` guard; seeded sync rows ⇒ the
    freshness/failure arms). Wait-loop timing uses
    `tokio::time::pause()` + `advance()` (AGENTS §5; never sleep).

**Test placement**: in-file `#[cfg(test)] mod *_tests` beside each
predicate (the `freshness_tests` precedent in `sync.rs`). These are
crate-unit tests — the lowest layer that can see `pub(crate)` — so
one-behavior-one-test is satisfied by construction and the
integration tier is not touched.

## 3. Predicate inventory (the test targets)

Per predicate: the surviving mutant shape (S3 census), the decision
branches, and the flip test.

### boards.rs

| # | Predicate | Survivor shape | Branches → flip test |
|---|---|---|---|
| 1 | `read_scope` (L196) | filter/count flips | (a) unknown-board-mid-race arm → seeded state missing the id ⇒ typed `Engine(Rejected(UnknownBoard))`; (b) per-entity scoping filters (`s.board == board.id`, `t.is_live() && stacks.contains`, `!l.deleted`) ⇒ seed a neighbour board's rows + a deleted label and assert the descendant_refs/sets exclude them; (c) `remote.board.as_deck()` extraction ⇒ bound vs unbound. |
| 2 | `publish_on` (L679) | guard/flip | (a) bound-board guard ⇒ seeded `board.remote = Some(..)` ⇒ `BoardAlreadyBound` (this is the guard whose bypass would publish a duplicate — the mutation is a real-duplicate-publish risk); (b) the mid-race miss arm ⇒ board absent from state. Post-`create_board` mapping is out of unit reach (concrete reqwest `DeckClient`, no transport trait — §4 D3): left to its existing integration pin, not duplicated. |
| 3 | `files_bind_on` (L1006) | flips | same pattern as publish_on's pre-IO guards (resolve + bound-state triage) against the fixture. |
| 4 | `wait_for_adoption` (L1295) | predicate flips (`fresh_settled`, `binding_present`) | `tokio::time::pause()`; seed sync row `Syncing` + board unbound ⇒ advance to deadline ⇒ `TimedOut`-class typed outcome; then seed settled `Idle` + bound ⇒ `SelectReceipt` (flip: with `fresh_settled` broken, success never returns); fresh-`Failed` ⇒ `SyncFailed`; binding-absent-at-success-time ⇒ keeps waiting (the `binding_present` conjunct's own test). |
| 5 | `select_prelude` (L1109) | the listing-find predicate (`b.id == board && !b.archived && b.is_live()`) | resolved via the §4 D2 micro-refactor: extract `validated_listing_entry(listing, board) -> Option<BoardsListEntry>` (pure); flip tests: archived entry refused, tombstoned (`is_live() == false`) refused, wrong id refused, live match returns the entry. |

### tasks.rs

| # | Predicate | Survivor shape | Branches → flip test |
|---|---|---|---|
| 6 | `resolve_stack` (L67) | arm deletions | table: zero live ⇒ `NoStacks`; exactly one ⇒ it; many without selector ⇒ `AmbiguousStack{candidates}`; uuid hit (live) / uuid miss (falls through to title); title match; duplicate titles ⇒ ambiguous; deleted stack invisible. |
| 7 | `resolve_target` (L106) | remap flip | zero live *boards* + `NoStacks` ⇒ remapped `AmbiguousBoard{candidates: vec![]}`; with a live board ⇒ plain `NoStacks` (both directions — the `match (err, no_board)` arms). |
| 8 | `tasks::add_on` (L203) | live-boards triage arm deletions | fixture: zero live boards ⇒ `AmbiguousBoard{[]}`; one ⇒ capture-to-its-backlog path (receipt names the created task); many ⇒ ambiguity naming every candidate uuid. The post-execute `else ⇒ UnknownTask` contract guard: §4 D4 disposition. |
| 9 | `tasks::move_on` (L330) | scoped triage arms | board+stack pair: uuid out-of-scope ⇒ `UnknownStack`; scoped title 0/1/many (`many` stays ambiguous *within* the board — the typed candidates are the colliding titles); `--board` without `--stack` ⇒ `AmbiguousStack{[]}` usage shape; board-less `resolve_target` passthrough. |
| 10 | `resolve_task` (L602) + `resolve_task_exact_any` (L648) | ambiguity boundaries | uuid-exact; ≥8-char id-prefix unique hit / multi-hit ambiguity / short-selector fallthrough to title; title 0/1/many. |
| 11 | `journal_receipt` (L778) | `l.id == entry` flip | §4 D1 micro-refactor (signature `&AppState`); in-window entry ⇒ `Some(label)`, absent entry ⇒ `None` (the flip makes the lookup never match ⇒ label always `None`). |

### habits.rs

| # | Predicate | Survivor shape | Branches → flip test |
|---|---|---|---|
| 12 | `add_on` (L495) | outcome arms + `HabitChanges` field deletions | fixture engine: plain create ⇒ `HabitCreated` receipt; create with `--at`/`--description`/`--day` ⇒ the sparse `UpdateHabit` lands (assert the seeded→published state carries `anchor_time`/`description`/`anchor_day` — kills each field-deletion mutant); `Applied`/`NoOp` both accepted (re-run the same sparse edit ⇒ `NoOp`, receipt still Ok). |
| 13 | `update_on` (L657) | `mask_hit || moved` flip + outcome arms | update that only edits fields ⇒ `changed = true` via `mask_hit`; update that only moves ⇒ `changed = true` via `moved`; a vacuous update ⇒ `changed = false` (kills `||→&&`); the `Applied`/`NoOp` acceptance arms (same value twice ⇒ `NoOp`, Ok). |
| 14 | `rm_on` (L723) | outcome arm | seeded log rows ⇒ receipt's surviving-row count reflects the tombstone (log survives); re-rm ⇒ the non-`Applied` guard path's typed outcome (§4 D4 for the mismatch guard itself). |

### reminders.rs

| # | Predicate | Survivor shape | Branches → flip test |
|---|---|---|---|
| 15 | `resolve_rule` (L211) | `==` flip | table: uuid exact hit; ≥8-char prefix unique / ambiguous (`AmbiguousReminderRule{candidates}`) / short-selector skipped; `ReminderOffset::parse_text_form` match by `DueOffset` kind ⇒ the rule; none ⇒ `UnknownReminderRule`. The `==` flip makes uuid/prefix match wrong rules — each positive case flips. |
| 16 | `collapse` (L497) | `off || on` flip | mode table over `TasksCmd::Remind`: `list` ⇒ List; `clear` ⇒ Clear; `remove` ⇒ Remove; `off` alone ⇒ `Mute{muted: true}` (flips to unreachable under `&&`); `on` alone ⇒ `Mute{muted: false}` (the flip-killer); neither ⇒ Add. Non-Remind ⇒ `None`. |

### sync.rs

| # | Predicate | Survivor shape | Branches → flip test |
|---|---|---|---|
| 17 | `wait_for_sync_result` (L231) | arm + predicate flips | `tokio::time::pause()`; decision table per its own doc: fresh cycle + Idle ⇒ `Succeeded`; fresh + `Failed` ⇒ `Failed(last_error)`; stale persisted `Failed` (no fresh cycle) ⇒ keeps waiting then `TimedOut` at deadline (not `Failed` — the S0-indistinguishability rule); signal-armed `seen_change` + fresh failure on a stale row ⇒ `Failed`; no report ever ⇒ `TimedOut`. `fresh_outcome` already has `freshness_tests` — extend only for arms those tests miss, no duplication. |
| 18 | `report_resurrections` (L168) | body deletion | the pure fold `resurrected_tasks` gets the typed pin: ids in arrival order, duplicates preserved, non-`TaskResurrected` events filtered. The `println!` shell is prose — §4 D5 disposition (joins #1229's golden home). |

### timers.rs / reflection.rs / connect.rs

| # | Predicate | Survivor shape | Branches → flip test |
|---|---|---|---|
| 19 | `timer_rows` (L209) | boundary flip (`delta > 0`, `effective_at <= now`) | injected `now` (no wall clock — the fn takes the instant): future alarm ⇒ `remaining = Some`, `overdue = false`; exact-instant ⇒ `remaining = None`, `overdue = true` (both boundary arms flip); past ⇒ overdue; `alarm_occurrence` `None` (no rule) ⇒ row filtered; sort order `(fire_at, id)`. |
| 20 | `filtered_rows` (L413) | boundary flips | injected filters (dates, not the clock — day_start derives from the filter, clock-law compliant): cadence match/mismatch; `since` inclusive-at-day-start / before; `until` exclusive-at-day-end (the `next`-day boundary the mutant flips); tombstoned entries excluded; sort `(window_start, id)`. |
| 21 | `write_config_overlay` (connect.rs L536) | `is_inline_table()` guard replacement | filesystem unit test (tempfile): (a) scalar-world doc (`[nextcloud]`, no connectors) ⇒ path A writes server/user (+ `sync.backend` only when `switch_backend`); (b) array-of-tables world ⇒ first entry's identity keys rewritten, no `[nextcloud]`/`backend` key added, other entries untouched; (c) **the guard's own case** — plain array of *inline tables* ⇒ the inline entry's keys are inserted (with the guard broken this path bails ⇒ flip); (d) comment/formatting preservation spot-check (structural: the doc still parses with the original comment span — not a text assertion). |

## 4. Open questions — recorded decisions

- **D1 — `journal_receipt` takes `&App`**: resolved, micro-refactor:
  change the signature to `&AppState` (its only use of the app is
  `engine.shared_state().load_full()`); callers pass the snapshot
  they already hold or load once. ~5-line production diff, no
  behavior change; keeps the predicate pure and unit-reachable
  without any App-sized fake.
- **D2 — `select_prelude`'s find predicate is buried behind a
  concrete `DeckClient`**: resolved, micro-refactor: extract the
  pure `validated_listing_entry(&[listing entry], u64)` helper and
  have `select_prelude` call it; the IO shell stays integration-tier
  (already pinned there — not duplicated). Same ~15-line class as D1
  (round law c: the issue's proposal is normative; O-2 agility — API
  churn is not a decision variable while internal).
- **D3 — `publish_on`'s post-`create_board` mapping (the
  `Bound{wiped_refs}` match)**: resolved, *not* unit-reachable:
  `DeckClient` is a concrete reqwest struct with no transport trait,
  and introducing one to kill two mutants is a real architecture
  change, not a micro-refactor. Decision: the pre-IO guards
  (bound-board guard, mid-race miss) get unit kills; the post-IO
  mapping keeps its integration pin; the gap is *recorded* on #1231
  (comment at stage-2 close), not silently dropped. If stage 2 finds
  the integration pin absent, it opens a `backlog:` issue instead of
  building an HTTP fixture here.
- **D4 — the `outcome_mismatch()` contract-guard arms** (`let
  CommandOutcome::X(..) = outcome else …`, `_ => return
  Err(outcome_mismatch())`): these are engine-contract guards only
  reachable when the engine violates its own reply contract. Killing
  them requires an engine that lies — i.e. a mock, which the fakes
  law forbids; the real engine over the in-memory repo cannot
  produce those outcomes. Resolved: excluded from the kill target
  set; stage 2 records them via `mutants` config exclusion (with a
  rationale comment pointing here) or leaves them documented
  survivors on #1231 — either way the disposition is written down,
  satisfying the "recorded, not silently dropped" law. The
  *reachable* outcome arms (Applied/NoOp/HabitCreated success
  mappings) are tested per §3 and remain kill targets.
- **D5 — `report_resurrections`' `println!` shell**: prose output;
  the no-text-assertion law forbids pinning it at unit tier.
  Resolved: pin the pure fold `resurrected_tasks` (typed ids);
  the printing shell's prose surface is #1229's (comment added).
- **D6 — why not a hand-built `AppState` without the engine?** The
  `&AppState` tier could construct `AppState` directly, but
  `AppState`'s canonical construction path is
  `EngineCore::from_persisted(..).app()`; going through the real
  engine over the in-memory repo gives one fixture for both tiers,
  exercises the exact published shape the predicates read, and adds
  ~50 ms per fixture — cheap at unit counts. Resolved: one fixture,
  engine-backed; a direct-`AppState` shortcut is permitted only if a
  predicate needs a shape the quiet-lane engine cannot publish.
- **D7 — existing integration coverage**: the `*_on` fns have
  integration pins riding `standalone_boot_fixture`/`it_cli` for
  *routing*. Those stay untouched; where one happens to pin the same
  *decision* as a new unit test, the unit test wins and the
  integration assertion is flagged to stage 3 for removal
  (one-behavior-one-test, lowest layer) — removal only with the
  stage-3 reviewer's confirm, since the routing tier must keep at
  least its routing assertions.
- **D8 — clock discipline**: the wait-loop tests pause the tokio
  clock; `timer_rows`/`filtered_rows` take instants/dates as
  parameters (no clock seam needed); the fixture's `Arc<dyn Clock>`
  is frozen so engine stamps are deterministic. No wall-clock
  dependence anywhere in the new tests (testing_strategy §6.1–6.2).

## 5. Stage-2 work order

1. `test_support.rs`: the fixture (`predicate_engine`, frozen clock,
   quiet lanes) + the shared `PersistedState` seed helpers.
2. Tier-A tables first (tasks/reminders/timers/reflection collapses
   and resolvers) — pure, fast, ~60% of the mutants.
3. Tier-B engine-fixture tests (habits/tasks `*_on`, boards guards,
   sync wait table, connect overlay) with `tokio::time::pause()`.
4. Micro-refactors D1/D2 (tests written first against the extracted
   helpers).
5. Kill-evidence pass: `cargo mutants -p taskboard-app --in-place
   --file <each touched file>` (the S3 cap template: memory-gate
   preflight, 7200 s windows, disk-backed TMPDIR) — flip the
   targeted survivors to caught; record the D3/D4 dispositions.
6. Update #1231 with the kill map + remaining-disposition table;
   quality gates via `scripts/run-quality-gates.sh`; commit per
   logical group (`test(commands): ...`).

## 6. Risks

- **R1 — mutant-run cost**: the evidence pass is hours of
  cargo-mutants windows (S3 pace ~1.4–6.5 min/mutant). Mitigation:
  `--file` filters per touched file only; cap template; dispatcher
  owns full-suite runs.
- **R2 — `wait_for_*` flake class**: the event-driven loops are the
  flakiest shape in the tree. Mitigation: paused tokio clock +
  explicit signal sends (deterministic by construction; no
  real-time deadlines shorter than the 200 ms poll — all timing is
  virtual).
- **R3 — `AppState` seed helpers duplicating domain
  `test_support::state` strategies**: reuse the entity mints; do
  not re-derive rows (the #981 class again). Stage 3 checks.
- **R4 — `collapse` tables may already exist** for
  track/repeat/habits/reflection (`collapse_yields_every_*`); the
  reminders one is the survivor — extend the *existing* table
  pattern, never a second table for an already-pinned collapse.
- **R5 — the D4 exclusion must not become a quality ceiling**: the
  excluded guards are unreachable-by-a-correct-engine, not
  "hard to test" — the exclusion comment must state the
  unreachability argument, or the mutant stays a documented
  survivor instead.

## 7. Estimated diff

- `test_support.rs`: +120–180 lines (fixture + clock + seed helpers).
- Test modules across 9 files (`boards`, `tasks`, `habits`,
  `reminders`, `sync`, `timers`, `reflection`, `connect`, `breaks`
  resolve_rule twin): ~25–35 test fns, ~800–1100 lines.
- Production micro-refactors (D1, D2): ~25 lines, no behavior change.
- `mutants` exclusion config/comment for D4: ~10 lines.
- Total: ~1000–1300 lines, >95% of it tests.

## 8. Backlog issues opened by stage 1

- **#1264** — fold the ~7 scattered local `FakeClock`/`FrozenClock`
  test doubles onto one shared domain `test_support` clock impl
  (discovered while designing D6/D8; the same class as #981's
  `RefusingStore` dedup).
- Comment (not a new issue) on **#1229**: the `report_resurrections`
  survivor splits — the pure fold is C1's typed kill (§3), the
  `println!` shell joins #1229's prose family (D5).
