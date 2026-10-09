# [Report] taskboard-domain Coverage Rationale Audit

- **Date:** 2026-10-09
- **Branch:** `feat/domain-canonical-model` (commit `b22f98d`)
- **Method:** `cargo llvm-cov -p taskboard-domain --all-features --lcov`,
  per-line mapping of every uncovered region to source; each gap
  classified as *justified*, *test gap*, or *defect*.
- **Headline numbers:** lines 85.96%, functions 89.01%, regions 83.82%
  overall; `merge.rs` carries nearly all of the gap (76.88% lines), which
  is expected — it is the policy surface. The important result of this
  audit is not the percentage but that **two of the uncovered regions are
  defects, not gaps**.

## 1. Defects found via coverage (need code changes, not just tests)

### D1 — R9c is exported but never wired: `finalize_pushed_delete` has zero call sites

`merge.rs:248-254` (the function) plus the missing arm in
`apply_sync_report`. The pipeline's `PushResult::Applied { echo }` match
only dispatches on the *echo* variant (`Task`/`Stack`/`Label`/`None`) and
never on the *op kind*. A successful `DeleteTask` push therefore falls
into the `echo: None` arm, completes the op, and leaves `remote` /
`remote_seen` stale on the tombstoned task until the next pull's R3 pass
re-tombstones. ADR 0004 / plan §7.3 R9c say the tombstone must finalize
immediately (clear binding, `remote_seen = None`) — Deck's DELETE
response is authoritative, and leaving the binding is what makes the
next-pull dependency exist in the first place.

*Proposed fix:* in the `Applied` arm, before the echo match, dispatch on
`op.op`: `LocalOp::DeleteTask(id)` → `tasks.insert(id,
finalize_pushed_delete(&tasks[&id]))` + complete; `DeleteStack`/
`DeleteLabel` → the finalize helpers + complete. Then add the R9c unit
test (tombstone kept, binding cleared) and a pipeline test.

### D2 — `live_stack_refs` is write-only dead code

`merge.rs:643, 660, 667`: the `BTreeSet` is created, inserted into from
two arms, and never read — a leftover from an earlier cascade design
(the 6b cascade matches on the remote ref directly).

*Proposed fix:* delete the set and its inserts. The
`merged_is_deleted`/`stacks[&id].deleted` checks feeding it go with it.

## 2. Test gaps — uncovered policy paths with no rationale (plan rules)

These are plan-normative behaviors (§7.2/§7.3, ADR 0004) that no test
exercises. All are cheap pipeline/unit tests using the existing
`CountingIds` + `base_state` helpers:

| Gap (lines) | Missing behavior | Plan rule |
|---|---|---|
| 343-377, 675-682 | `merge_label` + pipeline label merge — never merged a bound label (title/color LWW, soft-delete defense) | R6, decision 4 |
| 538-546, 322-339 | Stack echo adoption (`Applied { Some(Stack) }` binds remote + adopts) | R9a/b |
| 548-556, 400-416 | Label echo adoption | R9a/b |
| 585-589, 847-854 | `RemoteMissing` for a stack op → finalize tombstone | R9e |
| 591-595, 856-863 | `RemoteMissing` for a label op → finalize tombstone | R9e |
| 746-747 | Stack absent from snapshot → finalize tombstone (presence reconciliation is task-only in tests) | R6/§7.1 contract |
| 754-757 | Label absent from snapshot → finalize tombstone | R6/§7.1 contract |
| 631-639, 865-906 | **Board tombstone cascade** — plan says "a board tombstone cascades to all stacks/tasks"; `cascade_board` fully uncovered | R6 |
| 302-318, 663-668 | New remote stack adoption (R1-stack) | R1 |
| 442-452, 624-627, 428-437, 791 | New remote board adoption + `merge_board`'s adopt branch + the board upsert action | R1 / MVP-single-board caveat below |
| 705-710 | Pipeline resurrect drops the pending `DeleteTask` op (resurrect itself is unit-tested; the op-drop loop is not) | R5 |
| 576-579 | `RemoteMissing` cancels *sibling* ops of the task (only the single-op case is tested) | R9e→R3 |
| 569-570, 597-599 | `RemoteMissing` for `MoveTask`/`DeleteTask`/assign-unassign ops (only `UpdateTask` variant exercised) | R9e |
| 743 | **R2** — local-only never-pushed task is untouched by a sync (no test has an unbound task in state) | R2 |
| 780-782 | Stack cascade cancels the cascaded tasks' pending ops (cascade test had an empty outbox) | R6→R3 |
| 457-468 | `op_targets_task` arms (CreateTask/MoveTask/DeleteTask/Assign/Unassign + the false arm) — trivial, but one direct unit test or an R3 test with diverse ops covers all | helper |

## 3. Gaps with an accepted rationale (no action needed)

- **`contract.rs:124` (`Poll::Pending` → panic):** the violation detector
  itself; unreachable for any conforming repository by construction (the
  harness must not need a reactor). Uncoverable without writing a
  deliberately broken fake.
- **`CountingIds::new_op_id` (merge.rs:943-945):** trait completeness in
  the test fake. Op ids are minted by the *engine* when enqueuing (Phase
  3), never inside a sync report; the production generator's
  `new_op_id` is unit-tested in `clock.rs`. Same class:
  `CountingIds::new_board_id`'s `unreachable!` — but see caveat below.
- **`utc_min` (merge.rs:48-50):** defensive `remote_seen: None` guard.
  The pipeline only invokes merges for bound entities with a change
  detected via `remote_seen`, so `None` should be unreachable; the guard
  keeps the primitives total for direct callers. One-line unit test
  (e.g. `merge_board` with `remote_seen: None`) would cover it if we
  want zero untested private fns.

### Caveat on board adoption

Board adoption (R1-board) is *reachable in production* (a second device
sharing a board whose snapshot arrives before binding, multi-board era)
but unreachable in the MVP's single-board tests — and the test fake
actively panics on `new_board_id`, which would turn a future
referentially-loose proptest into a false failure. Recommended: give
`CountingIds::new_board_id` a real id and add the adoption test now
rather than documenting it as deferred.

## 4. Minor gaps in test-support surfaces

- **`memory.rs:57-62, 66-68`:** the contract harness only upserts
  `Task`s. Boards/stacks/labels arms of the fake are never exercised,
  and Phase 2's sqlite adapter will inherit exactly this breadth when it
  re-runs the harness. Extend `assert_task_repository_contract` to
  upsert one of each entity kind (asserts all four round-trip).
- **`persistence.rs:120` (`ValidatorKey` deserialize error branch):**
  one-line unit test — `serde_json::from_str::<ValidatorKey>("\"bogus\"")`
  must be `Err`.
- **`ids.rs` monomorphizations:** `from_uuid`/`as_uuid`/`get` are
  macro-generated per id type; tests exercise only the `TaskId`/
  `RemoteCardId`/`RemoteBoardId` copies. Extend the existing proptest to
  loop all four local ids and both remaining remote ids (cosmetic — the
  bodies are macro-shared).

## 5. Summary

| Class | Count | Disposition |
|---|---|---|
| Defects (D1 R9c wiring, D2 dead set) | 2 | fix in code, then test |
| Uncovered plan-normative policy paths | 16 clusters | add tests (one PR: `test/domain-coverage-gaps`) |
| Justified-uncovered | 4 | documented here; no action |
| Test-support breadth | 3 | extend harness + 2 one-liners |

The 76.88% line coverage of `merge.rs` is therefore *not* a healthy
number to ship for the crate the weekly `cargo mutants` gate targets:
every cluster in §2 is a behavior the roadmap pins on this exact code.
Recommended order: D1/D2 fixes first (they change behavior), then the §2
tests so the mutant-killing surface matches the plan's catalogue.

---

## 6. Follow-up execution (2026-10-09, same branch)

All §1/§2/§4 items were fixed and re-measured. Results:

### Defects
- **D1 fixed:** the `Applied` arm now dispatches on op kind before the echo
  match: `DeleteTask`/`DeleteStack`/`DeleteLabel` pushes finalize their
  tombstone immediately (R9c) and complete. Pipeline test
  `pushed_delete_finalizes_tombstone_immediately` and
  `pushed_stack_and_label_deletes_finalize_immediately` pin it.
- **D2 fixed:** `live_stack_refs` (write-only) and its `merged_is_deleted`
  helper removed.
- **New defect found by the new tests (fixed):** `cascade_board` only
  finalized stacks that were *already* tombstoned; a live stack under a
  deleted board survived. It now finalizes every stack of the deleted
  board. Pinned by `board_tombstone_cascades_stacks_tasks_and_labels`.

### Coverage
Lines 85.96% → **99.74%** (merge.rs 76.88% → 99.73%, state.rs 100%).
The 8 remaining uncovered lines all have rationales: the contract
harness' `Poll::Pending` violation panic (unreachable by construction)
and the test fake's `new_op_id` (engine-minted ids; production generator
unit-tested in `clock.rs`), plus match-arm attribution on total matches.

### Mutation surface
`cargo mutants -p taskboard-domain --all-features`: **43 → 2 survivors**.
- `test_support/**` excluded via `.cargo/mutants.toml` with rationale
  (mutating generators/harness = testing the testers, AGENTS §5; the
  harness gets real validation from the Phase 2 sqlite adapter).
- Killed by new tests: `resolve_local_stack`→None (position-index
  adoption), the echo-arm and cancel-loop conditionals (wrong-op-kind
  echo, unrelated-op cancellation on R3/R5/R9e/stack-cascade), the R4
  baseline guards (older-remote kept), absence re-finalization guards,
  `cascade_board` filters (present-entity cascade, dead entities
  spared), and the vacuous `state.rs` view-helper properties (now
  exact-set equality).
- **Remaining 2 survivors are equivalent mutants, documented:**
  1. `max_ts: > → >=` — max returns the identical value either way.
  2. `apply_sync_report` resurrect flag `&& → ||` — differs only for a
     *live* task with a pending `DeleteTask` op, a state the engine
     invariant forbids (delete commands tombstone atomically with
     enqueueing; resurrect cancels the op in the same report).

All quality gates green (fmt, clippy `-D warnings`, 161 workspace tests,
doc tests, `cargo deny`).
