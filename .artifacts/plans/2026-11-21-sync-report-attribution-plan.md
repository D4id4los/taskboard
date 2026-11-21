# Sync Report Attribution — Chunk Plan (Round 4, S1 / #1080)

- **Date**: 2026-11-21
- **Kind**: chunk plan — stage-1 output for chunk S1 of the round-4
  dispatch plan (`.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`,
  Prompt S1.1). Scope report §B-S1 is normative; issue #1080 is the
  divergence analysis; the R3 plan
  (`.artifacts/plans/2026-11-19-push-phase-generator-plan.md`)
  describes the generator that exposed the divergence.
- **Branch**: `fix/sync-report-attribution` (worktree
  `../taskboard-WT-fix-sync-report-attribution`, branched off
  `origin/main` @ `e0458e59`).
- **Stage-2 gate**: pre-commit hooks +
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (flock-safe; background + poll; defer per the #879/#894 rule when
  the lock is held). No coverage trio inside the subagent.

## 1. The divergence, restated precisely

`apply_sync_report` (`crates/taskboard-domain/src/pipeline.rs:446`)
writes per-board sync bookkeeping for **exactly one board**: the
report's board, resolved from the snapshot's remote binding
(`board_id`, pipeline.rs ~530) —
`sync.set_stored(board_id, phase, Some(now))` (~1393) plus the
`features` sidecar stamp (~1398). A cycle is per-board by
construction: the Deck connector plans one cycle per bound board and
the report names that board (`SyncReport::Completed/Failed { board }`).

`apply_push_report` (`pipeline.rs:1998`) has no report board in its
signature. It reconstructs "the boards this cycle wrote to" from the
resolved ops:

```rust
let affected: BTreeSet<BoardId> = outbox.iter()
    .filter(|entry| op_results.contains_key(&entry.op_id))
    .filter_map(|entry| crate::state::attributed_board(&entry.op, &stacks, &tasks, &labels))
    .collect();
```

`attributed_board` (`crates/taskboard-domain/src/state.rs:447`)
returns board ops' **target board** verbatim
(`UpdateBoard(b) | DeleteBoard(b) → Some(b)`). When such an op
targets an unbound or deleted board while a bound live board also
exists, the push path stamps a sync row for the unbound/deleted
target while the full sync path stamps the bound report board. The
equivalence property
`apply_push_report_equals_the_pipeline_push_phase`
(pipeline.rs:5411) fails on exactly that class — reachable only since
R3's generator dropped the five-clause single-board `prop_filter`
(#1080).

## 2. Decision: the attribution rule

**The per-board sync bookkeeping attributes to the cycle's board —
the board the report names — not to the op's target board.**

Semantic argument (the sync bookkeeping's meaning decides):

1. **A sync row is a per-bound-board record of server contact.**
   `sync.last_success` means "this board's last successful contact
   with its server". Only bound boards have a server relationship;
   every producer of rows keys them to bound boards: the full sync
   path (snapshot board), the actor's Failed-without-pushes lane
   (`actor.rs` ~1205: "every board **bound to the reporting
   connector** carries the verdict"; "a board-less failure vanishes
   from the map"), and the strategy factory itself (one stored row
   per generated board). An unbound board has no row and no server;
   advancing its `last_success` fabricates bookkeeping for a
   relationship that does not exist.
2. **The cycle, not the op, is the unit of sync.** A push-only cycle
   is the failed tail of a per-board cycle: `SyncReport::Failed`
   carries `board` — the report names its board — and the counters
   lane already attributes the cycle verdict to that board
   ("Board-scoped verdict: the report names its failing board"). The
   row lane must attribute the same way or the two lanes of one
   report disagree.
3. **The op's target board is a planning fact, not a sync fact.**
   `attributed_board` answers "whose effects does this op belong to"
   — the right question for the dead-board drain (pipeline.rs:2508,
   its other consumer) and for `read_scope`. It is the wrong question
   for "which server relationship did this cycle advance". The
   pipeline's planner only plans ops with
   `owning_board == Some(cycle_board)` (plus owning-entity-less
   orphans; `crates/taskboard-sync-nextcloud/src/actor.rs` ~537/~562),
   so in production the target board and the cycle board coincide for
   every *actually pushed* op; they diverge only for outcomes the
   property synthesizes (and for the latent orphan case below) —
   exactly where the two paths disagreed.

**Consequence accepted (behavior deltas, all unifying):**
- A push-only cycle no longer fabricates/advances a row for an
  unbound or deleted op-target board.
- Resolved ops whose `attributed_board` is `None` (the orphan lane:
  owning entity wiped) now credit the cycle's board — previously they
  silently contributed to `phase` but no row transition, an asymmetry
  against `apply_sync_report`, which always writes the report board's
  row.
- The report board's row transitions **unconditionally** (as in
  `apply_sync_report`), instead of only when the affected set is
  non-empty. A failed cycle that landed pushes is a server contact
  for that board even if every resolved op was entity-less.

**Rejected alternative**: re-pointing unbound-target ops at the bound
board (R3's temporary disposition). It is a generator-side dodge, not
a semantic rule — ambiguous when no bound board matches, and it
narrows the property's domain instead of fixing the pipelines.

## 3. The fix

### 3.1 `apply_push_report` takes the cycle's board (pipeline.rs)

- Signature gains a `board: crate::ids::BoardId` parameter (the
  report's board), keeping the injected-seams constructor shape.
- The `affected` BTreeSet + per-board loop is replaced by a single
  unconditional write for `board`:
  `sync.set_stored(board, phase, if any_rejected { carried } else { Some(now) })`
  (carry rule unchanged: a rejected outcome is not a success) and the
  `features` stamp applies to `board`'s row. `sidecar` stays
  untouched — no app-data evidence exists in a push-only cycle
  (matching today's behavior; the property already agrees on it).
- The doc comment ("board identity comes from the pushed ops'
  attribution") is rewritten to state the cycle-board rule and cite
  this plan §2.
- `attributed_board`'s use *here* disappears; its other consumers
  (dead-board drain, `read_scope`) are untouched.
- Caller `crates/taskboard-state/src/actor.rs` (~1253) passes the
  destructured `board` from `SyncReport::Failed`.

### 3.2 The property's cross-board case additions (TDD: red first)

- **The R3 generator is not modified.** `push_ready_state_strategy`,
  `repair_for_push_phase`, `repair_board_clause`,
  `repair_seen_clauses`, `repair_seen`, and `build_outbox` in
  `crates/taskboard-domain/src/test_support/state.rs` stay
  byte-identical (R3's coverage is the round's asset; stage 3
  verifies no hunk touches them).
- **Addition**: a new `push_ready_cross_board_state_strategy()` in
  the same module, built as
  `persisted_state_strategy().prop_map(cross_board_repair)` where
  `cross_board_repair` = `repair_for_push_phase` (reused verbatim)
  **plus** a total, minimal-repair cross-board step:
  - If the repaired state has no second board, synthesize one: a
    fresh `BoardId`, **unbound** (`remote: None`), live, with drawn
    title/color and epoch clocks — the "op targets an unbound board
    while a bound live board exists" shape. A tombstoned second
    board is out of scope here (same rule governs it; a variant can
    follow if mutants ask).
  - Append one `LocalOp::UpdateBoard(second_board)` op (fresh
    `OpId`, `queued_at = repair_seen()`) when no outbox op targets
    the second board — the minimal divergence trigger.
  - `debug_assert!`s: exactly one bound live board exists; the
    second board is unbound; the outbox names the second board.
- **The existing property consumes the union**:
  `push_ready_state_strategy().prop_union(push_ready_cross_board_state_strategy())`
  — one behavior, one test (AGENTS §5); the single-board domain is
  unchanged, the cross-board class joins it. No narrowing, no second
  property.
- The property body gains the cycle-board argument:
  `let bound = state.boards.values().find(|b| b.remote.is_some())...`
  passed to `apply_push_report` (both call sites in the property use
  the same bound board; `mirror_snapshot` already keys on it).

### 3.3 Unit tests (typed outcomes, lowest layer, pipeline.rs tests)

- `apply_push_report_attributes_the_bookkeeping_to_the_cycles_board`:
  state with a bound live board + an unbound board, outbox with
  `UpdateBoard(unbound)`; a `Failed`-tail push cycle with one
  `Applied` outcome. Assert: the unbound board has **no** sync row
  (`sync.get(unbound).is_none()` — structural fact, no text), the
  bound board's row is `Idle` with `last_success = now`, the op
  completed (typed `PersistenceAction::CompleteOp`).
- `apply_push_report_keeps_last_success_when_an_outcome_was_rejected`
  (existing, line 5163) is ported to the new signature, not weakened.
- Rejection-carries rule exercised on the cross-board shape: rejected
  outcome ⇒ the cycle board's row carries its previous `last_success`
  (`Some(ts(BASE))`), asserted via the row, not a message.
- No assertion on error text/labels anywhere (AGENTS §5).

## 4. Non-goals

- No change to `plan_pushes`, the orphan lane, or the dead-board
  drain's use of `attributed_board`.
- No narrowing of the R3 generator; no re-pointing repair.
- No tombstoned-second-board generator variant (mutants may ask
  later).
- No snapshot/merge semantics changes — only the row-attribution
  seam.

## 5. Risks

- **Signature churn**: `apply_push_report` has few callers (actor +
  property + unit tests); risk is mechanical, contained to the sync
  seams per the dispatch plan's S1.2 diff confinement.
- **Semantics change visible in persistence**: boards that previously
  gained phantom rows stop doing so; a persisted DB carrying such a
  phantom row simply keeps it (rows are per-board, never deleted
  here) — no migration considered; `UpsertSyncStatus` actions shrink
  accordingly. Session-restore/UI reads of the aggregate are
  unaffected (aggregate sums rows; a missing phantom row is the
  correct absence).
- **Property shrink depth**: the union's synthesized second board
  must shrink to the minimal failing pair; synthesized ids via
  `uuid::new_v4` inside `prop_map` follow the R3 precedent
  (constant, not fresh RNG, for timestamps) — shrinking stays
  structural.
- **The attribution rule is a judgment call**: recorded in §2 with
  its argument; if the operator disagrees, the alternative (op-target
  attribution) would instead require `apply_sync_report` to write
  rows for unbound targets — rejected there as fabricated
  bookkeeping. Stage 3 verifies the plan's §2 argument matches the
  implementation's comments.

## 6. Estimated diff size

~150–200 lines: pipeline.rs (~40: signature + write loop + doc),
actor.rs (~2), test_support/state.rs (~80–100: cross-board strategy),
property + unit tests (~50–60). One commit series on
`fix/sync-report-attribution`; no push, no PR from this stage.

## 7. Open questions — all resolved

| Question | Decision |
|---|---|
| Op-target vs bound-board attribution | **Cycle's report board** (§2) |
| Parameter vs derivation of the cycle board | Parameter — the report names it; derivation would re-introduce ambiguity |
| Generator change | Additive only (`prop_union` + new strategy); R3 functions byte-identical |
| Orphan-op row credit | Cycle board (unifying delta, accepted §2) |
| Unconditional row write | Yes — matches `apply_sync_report` (§2) |
| Tombstoned second board variant | Deferred (§4) |
