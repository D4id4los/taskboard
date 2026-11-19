# Push-Phase Generator Chunk Plan (R3, round 3) — 2026-11-19

- **Chunk**: R3 of the round-3 dispatch plan
  (`.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md` §4),
  scope report §A row R3 (normative). Issue: **#1072**.
- **Problem**: `taskboard-domain::pipeline::tests::apply_push_report_equals_the_pipeline_push_phase`
  took **75s** — the suite's slowest test (runtime report §B, the
  non-quantized outlier). The property's `state` draw runs
  `persisted_state_strategy()` through a five-clause `prop_filter`
  ("bound board and seen baselines"); the joint acceptance probability
  of the clauses is small, so rejection sampling discards most draws
  before a case runs.
- **Fix shape (per #1072 and the scope report)**: a direct-construction
  strategy that produces the accepted shape; the property's assertions
  stay byte-identical.
- **Branch**: `test/push-phase-generator`. Stage-2 continues in
  `../taskboard-WT-test-push-phase-generator`.

## 1. The seam (as read on `origin/main` @ `6bd0f4dd`)

- Property: `crates/taskboard-domain/src/pipeline.rs` ~5378–5528
  (`proptest!` block, config `test_support::proptest_config(256)`).
- The five state clauses (the `prop_filter` closure, pipeline.rs:5380):
  1. a **bound board exists** (`boards.values().find(|b| b.remote.is_some())`);
  2. that board has **`remote_seen` set**;
  3. that board is **not deleted** (`!board.deleted`);
  4. every **bound** stack, label, and task has **`remote_seen` set**
     (unbound entities are unconstrained);
  5. the **outbox is non-empty**.
- These clauses mirror exactly what the property body's
  `mirror_snapshot` (pipeline.rs:5533) needs to succeed: it takes the
  first bound board and unwraps `board.remote_seen?` / each bound
  entity's `remote_seen?` (`last_modified` per entity). The filter is
  the mirror's precondition, stated as data.
- Second filter: `pushes` vec (`0..6` outcomes) filtered `!is_empty()`
  (pipeline.rs:5416).
- Strategy: `persisted_state_strategy()`
  (crates/taskboard-domain/src/test_support/state.rs:488) —
  `app_state_strategy()` + outbox rows + validators + sync phases,
  composed with `prop_map`. `build_outbox` drops rows whose target
  entity doesn't exist, so the outbox is frequently empty — clause 5
  rejects often. `app_state_strategy` may generate zero boards and
  boards that are all deleted/unbound — clauses 1–3 reject often.
  Bound entities with `remote_seen = None` are common — clause 4
  rejects often. Rejection is multiplicative → the 75s.
- Other `prop_filter` sites: merge.rs:8239 (distinct fact ranks, local
  generator, cheap) — unrelated shape, not touched here.

## 2. Recorded decisions

### D1 — Minimal-repair post-transform, not a from-scratch generator

The strategy is

```rust
pub fn push_ready_state_strategy() -> impl Strategy<Value = PersistedState> {
    persisted_state_strategy().prop_map(repair_for_push_phase)
}
```

with `repair_for_push_phase` a **total, minimal-repair function**: it
changes a draw only where a clause fails, and leaves every
already-conforming state untouched. This is what makes the
coverage-equivalence argument in §3 trivial and airtight — a
rebuilding generator (the issue's alternative sketch) or a
`prop_flat_map` re-derives a state universe and must re-prove
referential guarantees the existing strategies already carry.

### D2 — Clause disposition: all five become construction invariants

Clauses 1–5 are guaranteed by the repair function (construction
invariants). **No post-generation `prop_assume!`/filter remains on the
state side.** The repair function carries `debug_assert!`s for the
five invariants as executable documentation (they compile out in
release; the invariant property test in §5 pins them in test builds).

The **second filter on `pushes` stays byte-identical**: it rejects
only the empty vec (~1/6 of draws, negligible cost) and touching it
would violate the byte-identical law (L-c) for no measurable win.

### D3 — Repair specifics (each clause's minimal repair)

- **Board (clauses 1–3)**: if a bound, seen, live board already
  exists, touch nothing. Otherwise:
  - if a non-deleted board exists, bind the first one:
    `remote = Some(RemoteBoardId::deck(n))` (n = next unused remote
    number — derive from the max deck number already bound, +1, to
    avoid colliding with other boards' bindings), `remote_seen =
    Some(REPAIR_SEEN)`, `deleted` left as-is (false for the
    non-deleted pick);
  - if no boards exist or all are deleted, **insert one synthesized
    live bound board row** (bound, `remote_seen = Some(REPAIR_SEEN)`,
    `deleted = false`). This keeps the repair total — the function
    never fails, so the strategy never falls back to rejection.
- **Stacks/labels/tasks (clause 4)**: for each **bound** entity whose
  `remote_seen` is `None`, set it to `Some(REPAIR_SEEN)`. Existing
  `Some` values are **never overwritten** (drawn diversity — the
  mirror's `last_modified` values stay varied). Unbound entities are
  untouched.
- **`REPAIR_SEEN`**: one fixed `DateTime<Utc>` constant (e.g. the
  existing `ts(...)` helper's convention, documented). Rationale: the
  repair runs inside `prop_map` (no fresh RNG there); a constant is
  deterministic, and diversity is preserved because conforming draws
  (the common case) pass through untouched and repaired `None`s are
  the minority edge. Any `Some` instant satisfies the mirror's
  precondition.
- **Outbox (clause 5)**: if empty, append exactly one `PendingOp`:
  `LocalOp::UpdateBoard(first_board_id)` — after the board repair a
  board always exists, so `build_outbox`-style resolution always
  succeeds; `queued_at` = `REPAIR_SEEN`; `op_id` = a fresh
  `uuid::Uuid::new_v4()` guarded against collision with the existing
  op ids (a tiny retry loop over the existing id set — bounded, never
  expected to iterate).
  A single append is sufficient: the property maps `pushes` onto
  `outbox_ids[i % len]`, which needs `len >= 1`, nothing more.
- The property body's `let Some(...) = mirror_snapshot(&state) else {
  prop_assume!(...) }` fallback **stays byte-identical** (L-c). It
  becomes unreachable by construction; the byte-identical law
  outranks dead-code tidiness, and stage-3's mandate is explicitly
  "only the strategy rewired".

### D4 — Name, location, reusability

- Name: `push_ready_state_strategy()`; repair helper
  `repair_for_push_phase` (private to `test_support::state`, the
  strategy is `pub`). Location: `crates/taskboard-domain/src/
  test_support/state.rs`, directly below `persisted_state_strategy`.
- Doc comment states the produced shape ("a `PersistedState` with a
  bound live seen board, every bound stack/label/task seen, non-empty
  outbox — the mirror-snapshot precondition of the push-phase
  equivalence property") so a future filtered property can adopt it.
- **Reusability verdict**: today there is exactly one consumer (this
  property); merge.rs:8239's filter is a different shape. Per the
  no-speculative-generality line (dispatch §1 code-quality mandate),
  the strategy stays **concrete** — no parameters, no generics. It is
  reusable by *adoption* (call it), not by configuration. If a second
  filtered property needs a variant shape, generalize then, on
  evidence.
- Crate boundary: `test_support` is already the domain crate's test
  module — pure data, no new dependencies. Complies with AGENTS §2.

### D5 — Case count: keep 256, no `PROPTEST_CASES` change

`proptest_config(256)` stays. With rejection gone, all 256 configured
cases now *execute* (previously an unknown fraction was burned on
rejected draws); the accepted-case count can only rise. The
`PROPTEST_CASES` env override (documented in `proptest_config`)
continues to work unchanged. No tuning is warranted — the generator
*is* the fix (#1072's own wording).

### D6 — Verification method (TDD order, timing honesty)

1. **Invariant property test first** (the chunk's TDD anchor), in the
   same `state.rs`/`pipeline.rs` test area as the strategy:

   ```text
   proptest: every draw of push_ready_state_strategy() satisfies all
   five clauses and mirror_snapshot(&state).is_some()
   ```

   Cheap (no filter on it), deterministic, pins the shape contract
   independently of the equivalence property. 256 cases.
2. Rewire the equivalence property: the `state in ...` line swaps
   `persisted_state_strategy().prop_filter(...)` for
   `push_ready_state_strategy()`; **every other byte of the property
   (assertions included) unchanged** — the stage-3 review trap checks
   exactly this hunk.
3. **Timing**: identical command before and after, on the branch:

   ```sh
   scripts/memory-gate.sh cargo nextest run -p taskboard-domain \
     -E 'test(apply_push_report_equals_the_pipeline_push_phase)'
   ```

   Baseline re-measured on the branch pre-change (the 75s figure is
   the runtime report's; the re-measured number is the one recorded in
   the chunk report), then the after number. Both recorded with the
   command + environment (round law L-a analogue: measured claims
   carry the command).
4. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo
   nextest run --workspace` (flock-deferral rule applies).

## 3. Coverage-equivalence argument (the L-c proof)

Let **A** = the filter's admitted set: every `PersistedState` the old
pipeline accepted, i.e. states satisfying clauses 1–5 above. Let **G**
= `repair_for_push_phase ∘ persisted_state_strategy` (the new
generator's value function).

- **G's image ⊇ A**: for any `S ∈ A`, the underlying strategy can draw
  `S` itself (S is in the strategy's output space — the filter only
  *selected* among those draws), and the repair is the **identity on
  every conforming state** (minimal-repair design, D3's "touch
  nothing" branches). Hence `G(S) = S` — every previously admitted
  state remains producible. Nothing is removed.
- **Every G output satisfies the shape**: the repair's branches jointly
  guarantee clauses 1–5 for any draw (the synthesized-board branch
  makes the function total), so no draw reaches the property outside
  the accepted shape — the shape class is *at least* the filter's.
- **Shrinking stays valid**: `prop_map` applies the repair to every
  shrunken draw too, and the repair is total — a shrunken case still
  satisfies the five clauses, so failure reproductions keep the
  accepted shape (stage-3 trap (a)).
- Net: the generator's coverage is a **superset** of the filter's
  admitted set (it may additionally produce rare repaired shapes, e.g.
  a synthesized board — growth, never shrinkage). L-c satisfied.

## 4. Acceptance criteria (from dispatch §4, made checkable)

- [ ] The 75s test drops to a normal profile; before/after recorded
      with the identical command (§2 D6.3).
- [ ] Coverage does not shrink: the §3 argument executed — the
      invariant property test green, the equivalence property green at
      256 cases, the repair's identity-on-conforming behavior visible
      in the diff.
- [ ] The property hunk shows assertions byte-identical; only the
      strategy line rewired.
- [ ] The strategy is reusable where the same shape is needed (D4's
      adoption contract; no speculative generality).
- [ ] Diffs confined to `crates/taskboard-domain/src/test_support/
      state.rs` and `crates/taskboard-domain/src/pipeline.rs` (the
      `state in` line) + this plan/report. No production code, no
      other tests touched.

## 5. Estimated diff size

| File | Change | Size |
|---|---|---|
| `test_support/state.rs` | `push_ready_state_strategy` + `repair_for_push_phase` + `REPAIR_SEEN` + doc comments | ~80–110 lines |
| `pipeline.rs` | strategy rewire in the property (one clause) + the new invariant property test | ~15–25 lines |
| plan/report artefacts | this file + the chunk report | ~150 lines md |

Total: **~100–135 lines of Rust**, zero production-code lines.

## 6. Risks

1. **Repair inserts state the generated universe didn't contain**
   (synthesized board; appended outbox op): could interact with
   referential invariants (`build_outbox` resolution, mirror
   construction). Mitigation: the invariant property test asserts
   `mirror_snapshot(...).is_some()` — the mirror is the strictest
   consumer; the equivalence property itself re-verifies end-to-end at
   256 cases.
2. **`REPAIR_SEEN` constant reduces timestamp diversity on repaired
   draws**: bounded risk — conforming draws pass untouched and are the
   majority; a repaired draw was previously *rejected outright*, so
   any repaired diversity is strictly new coverage.
3. **OpId collision loop**: `new_v4` collision with an existing id is
   astronomically unlikely; the guard loop is bounded and defensive
   only.
4. **Dead `prop_assume!` fallback flagged in review**: recorded
   decision D3 — the byte-identical law (L-c) governs; the fallback
   stays. If stage 3 wants it annotated, a comment (not a code change)
   is the vehicle.

## 7. Out-of-scope findings → backlog issues

None discovered during this stage: the only other `prop_filter` sites
(merge.rs:8239, recur.rs filter_maps) are cheap, local, and unrelated
shapes; the second `pushes` filter is negligible by measurement
reasoning and inside the chunk's byte-law scope. **No backlog issues
filed.**

## 8. Stage-2 execution notes (2026-11-19, deviations from D3)

The synthesized-board branch of D3 and the byte-identity of the
property body were tested against reality at 256 cases; two deviations
were forced by failures and are recorded here for stage 3:

1. **D3's board repair operates in place, not by synthesis.** The first
   failing case showed the filter's clauses 1–3 are about the *first
   bound board* (`values().find(|b| b.remote.is_some())`), not "any
   bound board" — a repaired draw with an earlier bound-but-unseen
   board fails clause 2. The repair now binds/stamps/undeletes the
   draw's single existing board in place (the strategy always generates
   exactly one board); a synthesized row only covers the
   strategy-unreachable no-boards case.
2. **The repair never creates cross-board topology.** A second failing
   case proved that repaired multi-board shapes (synthesized board +
   entities owned by the deleted unbound board, including an
   `UpdateBoard` op targeting it) *fail the equivalence property* — the
   two pipelines attribute board-op bookkeeping (`sync.last_success`)
   to different boards. The five clauses are necessary but not
   sufficient outside the single-board topology the old filter's
   universe contained. The repair therefore stays inside the validated
   single-board domain; the divergence itself is production behavior
   out of scope for this chunk, filed as **#1080**. The §3
   coverage-equivalence argument holds as stated (superset of the
   admitted set), with the operational caveat that new shapes are
   constrained to the single-board topology.

Also: `test_support::mod.rs` gained a one-line re-export of
`push_ready_state_strategy` (necessary; the plan's file list omitted
it), and the auto-appended proptest regression entry from the
intermediate generator was reverted (it encoded a no-longer-reproducible
case). Measured before/after with the identical command
(`scripts/memory-gate.sh cargo nextest run -p taskboard-domain -E
'test(apply_push_report_equals_the_pipeline_push_phase)'`):
**97.406s → 0.802s** (pre-change re-measured on the branch; the 75s
runtime-report figure was under full-suite load).
