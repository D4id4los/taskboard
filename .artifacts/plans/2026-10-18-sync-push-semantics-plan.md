# [Plan] Sync Push Semantics — per-op label-delta outcomes (#314) + delete-path re-resolve (#319)

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 1 of the remediation-followups
  dispatch (dispatch §1/§2, prompt 1.1). Planning only — no
  implementation in this commit.
- **Origin**: issues #314 and #319, both normative. Owning artefacts:
  `.artifacts/reports/2026-10-17-update-label-rejection-arms-analysis-report.md`
  (§Summary, §F1–F5, §Proposed fix shape) for #314;
  `.artifacts/plans/2026-10-17-sync-stale-binding-clock-plan.md`
  (§non-goals, the noticed delete-arm gap; §decision-1 for the #258
  seam shape) for #319. Dispatch decisions **D1** (per-op emission
  classes) and **D2** (bare-404 convergence) are settled; this plan
  implements them and records the sub-decisions they leave open
  (§decisions D1a/D1b/D4–D7).
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups`, cut off `main` @
  `361ab0b` (= `origin/main`). Shared by the whole 5-chunk round;
  single-PR delivery per dispatch §1.
- **Anchor base**: all `file:line` anchors below are against `361ab0b`
  — i.e. **post chunk-10 module split** (`src/push_exec.rs` →
  `src/push_exec/{execute,overlay,outcomes,tests}.rs`). The 2026-10-17
  analysis predates the split; §re-anchor carries the mapping.
- **Size**: M (per the dispatch's grade).

---

## §re-anchor — the analysis's line references, remapped

| Analysis reference (pre-split `push_exec.rs`) | Current home @ `361ab0b` |
|---|---|
| the two `classify_push(..).is_some()` early returns (assignLabel/removeLabel arms) | unified into `label_delta_failure`, `execute.rs:817-841`; called from the assign arm `execute.rs:656` and the remove arm `execute.rs:676` |
| post-#310 success tail (per-op emission) | `execute.rs:681-726` — comment `:681-687`, primary outcome `:688-693`, subsumed loop `:694-725` |
| `create_task`'s per-op classified mapping (F3 precedent) | `execute.rs:374-437` (classified push at `:410-411`, transport abort with landed evidence at `:418`) |
| `primary_outcome` (synthetic Applied for every subsumed op) | `outcomes.rs:50-60` |
| `PushCtx::existing` classification | `mapping.rs:152-182` (NotFound+existing → `RemoteMissing` at `:155-161`; `Forbidden if delete` → `RemoteMissing` at `:164`; transport class → `None` at `:173`) |
| `apply_push_outcomes` first-`Applied` completion | `pipeline.rs:932-1157` (CompleteOp emission at `apply_push_report` `:858-864`) |
| #258 re-resolve seam | `fetch_card_for_write`, `execute.rs:486-549` (listing loop `:510-547`) |
| `delete_card` (stale-stack delete, no re-resolve) | `execute.rs:782-810`; dispatch site `execute.rs:68-71` |

## §bug-314 — what exactly is broken

After the field PUT has landed (its echo held in `updated`), the
label-set delta runs through `assignLabel`/`removeLabel`
(`execute.rs:637-679`). When one of those sub-endpoint calls fails
with a *classified* (non-transport) error, both arms return
`Self::label_delta_failure(..)` (`:656`, `:676`), whose classified
branch returns `GroupOutcome::Outcomes(primary_outcome(group,
Applied { echo: Some(..) }))` (`execute.rs:832-837`) — completing the
primary **and every subsumed op** with the primary's result /
synthetic `Applied { echo: None }`, and discarding the classification
(`is_some()` is all that is checked, `execute.rs:823-831`). Per the
analysis (§F1–F5):

- **F1**: a subsumed label op whose own write was the one that failed
  is completed as `Applied` — the op leaves the outbox
  (`PersistenceAction::CompleteOp`) and never retries. Silent
  local/server label divergence.
- **F2**: subsumed label ops for *unresolvable* labels get the same
  synthetic `Applied`; their `Rejected { LocalData }` — which the
  post-#310 success tail correctly emits (`execute.rs:702-713`) — is
  swallowed. #310's fix is incomplete on exactly this path.
- **F4**: a 404 on a label sub-call classifies `RemoteMissing`; today
  it is discarded entirely. The fix must route it to the label ops,
  **never to the primary's result slot** (a task-level `RemoteMissing`
  would tombstone the task under R9e because of a label-level
  failure — domain `pipeline.rs:1088-1115`).
- **F5**: neither arm has any test (no mock mounts a classified
  failure on `assignLabel`/`removeLabel`; verified by grep — every
  existing mount answers 200).

The in-code comment on `label_delta_failure` ("the label ops retry
next cycle") narrates behaviour the code does not have; the rework
rewrites it to state the real contract (§comments).

## §design-314 — per-op emission in the classified arms (D1 + extensions)

### The outcome table (normative)

On a classified failure of the label write for remote number *N*, the
arm emits **one outcome per op**, exactly like the success tail and
the `create_task` precedent:

| Subsumed op class | Outcome | Source |
|---|---|---|
| primary (`UpdateTask` materialized) | `Applied { echo: Some(fields-PUT echo) }` | D1 — the PUT genuinely landed |
| `UpdateTask(_)` / `MoveTask(_)` | `Applied { echo: None }` | D1 — they rode the PUT's field state (success-tail arm `execute.rs:715-720`) |
| `AssignLabel(_, l)` / `UnassignLabel(_, l)`, `l` unresolvable (`overlay.label_by_local` miss) | `Rejected { LocalData }` | D1 — the #310/F2 semantics verbatim (`execute.rs:705-711`) |
| label op whose remote number **is** *N* (the write that failed) | the classified `PushResult` per-op | D1 — the F3 precedent (`execute.rs:410-411`); for 403 → `Rejected { Forbidden }`, 404 → `RemoteMissing` (routed to the op, never the primary — F4), 400 → `Rejected { BadRequest }`, `Envelope` → `Applied { echo: None }` (a 2xx landed) |
| label op whose label **converged** before the failure (already matching on the fetched card, or its assign/remove returned `Ok` earlier in the delta) | `Applied { echo: None }` | D1b below — the analysis's "label ops whose write succeeded → Applied" |
| label op the delta **never reached** (loop stopped at *N*) | **omitted from the report** — stays queued, retries next cycle | **D1a** below |
| anything `cannot_ride_a_task_group` filters | skipped (stays queued) | unchanged (`outcomes.rs:31-46`) |
| transport-class failure (classification `None`) | `Aborted { landed: Vec::new() }` | D1 — unchanged: the PUT is an idempotent full-send, re-sending the whole group is safe (`execute.rs:839-840`) |

### D1a — the never-attempted class is omitted, not completed

D1 names three classes; the delta's early exit creates a fourth:
label ops whose wire call was never made (a number ordered after the
failing one in the assign loop, or any remove — the remove phase
never starts after an assign failure). **Decision: omit them from the
report.** `apply_push_outcomes` documents "ops absent from the map
stay queued untouched" (`pipeline.rs:909-911`; CompleteOp is emitted
only for ops present in the report, `pipeline.rs:858-864`), so
omission *is* the retry semantics — the analysis's own fallback
option ("omit them from the report so they stay queued"), narrowed to
only the never-attempted ops. Emitting `Applied` would be F1 verbatim;
emitting a `Rejected { Server }` would fabricate a rejection reason
and needlessly hold back `SyncStatus.last_success`
(`pipeline.rs:867-884`: `any_rejected` keeps the last success stamp).

### D1b — tracking convergence through the delta

`update_task`'s delta loops (`execute.rs:637-679`) gain a
`settled: BTreeSet<u64>` of remote label numbers whose server state
now matches `desired`:

- assign loop skip (`current.contains`) → insert (already matching on
  the fetched card);
- assign loop `Ok` → insert;
- remove loop skip (`desired.contains`) → already settled by the
  assign phase (no insert needed — every `desired ∩ current` number
  passed through an assign-loop skip or `Ok`);
- remove loop `Ok` → insert (the label is now absent, as desired).

On `Err(err)` the arm passes `settled`, the failing number *N*, and
the error to the reworked `label_delta_failure`.

### The op → label → remote-number mapping (via `overlay.label_by_local`)

Both emission sites (success tail `execute.rs:701-713`, failure arm)
decide a subsumed label op through **one shared helper**, so the two
tails cannot drift:

```rust
/// One subsumed label op's result after the delta phase. `None` omits
/// the op from the report (stays queued): only the failure arm
/// produces omissions, for labels the delta never reached.
fn subsumed_label_result(
    &self,
    label: taskboard_domain::LabelId,
    failed: Option<(u64, &PushResult)>, // (failing remote number, classified result)
    settled: &std::collections::BTreeSet<u64>,
) -> Option<PushResult> {
    let Some(num) = self.overlay.label_by_local.get(&label).map(|n| n.get()) else {
        return Some(PushResult::Rejected { kind: SyncErrorKind::LocalData });
    };
    if let Some((failed_num, result)) = failed
        && failed_num == num
    {
        return Some(result.clone());
    }
    if failed.is_none() || settled.contains(&num) {
        return Some(PushResult::Applied { echo: None });
    }
    None // never reached before the failure
}
```

(`overlay.label_by_local` is `pub(super)` — `overlay.rs:16` — already
readable from `execute.rs`.) The success tail calls it with
`failed = None` (a completed delta settled everything resolvable),
which reproduces today's `Applied`/`Rejected { LocalData }` split
exactly; the failure arm calls it with `Some((N, &classified))`. The
shared subsumed-loop skeleton (iterate `group.subsumed`, find in
`outbox`, `cannot_ride` filter, `UpdateTask`/`MoveTask` arm) is
factored once — a `subsumed_task_outcomes(&self, group, outbox,
|label| …)` helper or equivalent — so the emission table lives in one
place. Exact factoring shape is stage-2's latitude; the constraint is
**one emission site**, no duplicated iteration.

`label_delta_failure` becomes a `&self`**method** (it now reads the
overlay) with signature `(group, updated, board, err, failed_num,
settled, outbox) -> GroupOutcome`; its classified branch builds the
primary outcome plus the shared per-op loop, its `None` branch keeps
`Aborted { landed: Vec::new() }` verbatim.

### Downstream consequences (expected, correct)

- `SyncStatus.last_success` no longer advances on a cycle whose label
  delta hit a classified rejection (`any_rejected` —
  `pipeline.rs:867-884`): the report now tells the truth instead of
  all-`Applied`. No `apply_push_report` semantics change (non-goal).
- A 404 on a label sub-call now finalizes the failing label's ops
  (R9e's label-op arm, `pipeline.rs:1140-1143` — op completes, no task
  tombstone). If the label was deleted remotely, the pull's label
  listing tombstones the local label row on the same/next cycle.
- `pending_ops` (the outbox depth) now correctly retains the
  never-attempted ops (`pipeline.rs:866`).

## §bug-319 — what exactly is broken

`delete_card` (`execute.rs:782-810`) DELETEs at the overlay's ref —
whose **stack component may be stale** when another client moved the
card (the exact #258/F10-02 shape, delete intent). The 404 classifies
`RemoteMissing` (`mapping.rs:155-161` with `existing: true`) → R9e
tombstones the task and cancels sibling ops (`pipeline.rs:1088-1115`)
→ the same cycle's pull lists the card at its fresh stack and, with
the binding cleared by the tombstone (`merge.rs:268-282`), re-adopts
it as a **twin** — the delete intent is silently lost. The
stale-binding plan's §non-goals flagged this out of #258's scope;
issue #319 is the follow-up.

## §design-319 — delete-path re-resolve (D4/D5/D2/D7)

### D4 — seam choice: lazy re-resolve on the delete-path 404, mirroring `fetch_card_for_write`

The issue offers two shapes; **this chunk implements the lazy
re-resolve**, not an unconditional pre-write fetch:

1. **It is the #258 seam's shape.** `fetch_card_for_write` asks the
   listings only when the primary signal 404s (`execute.rs:506-547`);
   update/move pre-fetch unconditionally only because they *need the
   card body* to build the PUT/reorder. A delete needs no body.
2. **No wasted round trip on the hot path.** Every delete would pay a
   GET under the pre-write-fetch shape; the lazy shape pays the
   listing pair only in the raced window (a kiosk-perf goal, AGENTS §1).
3. **Classification stays under the delete ctx.** A pre-write fetch's
   non-404 failures classify with `PushCtx { delete: false }` — a 403
   on that fetch would become `Rejected { Forbidden }` where the
   delete table says gone (`mapping.rs:164`). The lazy shape keeps
   every delete-path classification under `{ existing: true, delete:
   true }`.

### D5 — one shared `relocate_card` helper

The listing-scan loop of `fetch_card_for_write` (`execute.rs:510-547`)
is extracted into a helper both write paths share:

```rust
/// One re-resolve against the fresh active+archived listing pair
/// (ADR 0007's #258 amendment): the stack the card actually lives
/// under, rebinds the overlay, or `None` when both listings confirm
/// absence. A listing failure classifies through the normative table
/// with `ctx` (the delete path passes the delete ctx).
async fn relocate_card(
    &mut self,
    group: &PushGroup,
    task: taskboard_domain::TaskId,
    card_ref: &RemoteCardRef,
    ctx: PushCtx,
) -> Result<Option<RemoteStackId>, GroupOutcome>
```

`fetch_card_for_write` becomes: fetch → on `NotFound`,
`relocate_card(.., ctx_read)` → `Ok(Some(fresh))` → refetch once at
the fresh location (unchanged detail-outranks-listening rule);
`Ok(None)` → `classify(NotFound)`; `Err(outcome)` → propagate. The
rebind-before-retry ordering is preserved (a subsequent failure still
leaves the overlay refreshed — next cycle addresses the fresh stack).

### The delete flow

`MaterializedOp::DeleteTask` dispatch (`execute.rs:68-71`) passes
`task` into `delete_card` (the rebind needs it); `delete_card` becomes:

```rust
match self.client.delete_card(board, stack, card).await {
    Ok(deleted) => primary_outcome(Applied { echo: Some(Task(map_card(..))) }),  // unchanged
    Err(DeckError::NotFound) => self.delete_after_re_resolve(group, task, card_ref).await,
    Err(err) => classify(group, &err, PushCtx { existing: true, delete: true }), // unchanged
}
```

`delete_after_re_resolve` (ctx `{ existing: true, delete: true }`
throughout): `relocate_card` → `Ok(Some(fresh))` → DELETE once at the
fresh stack (`Ok` → the unchanged `Applied`-with-echo shape; any
`Err` → `classify`, so **D7**: a second 404 after a successful locate
converges `RemoteMissing` — the write path outranks the lagging
listing, the delete-path analogue of the fetch rule); `Ok(None)` →
`classify(NotFound)` — **D2**: the bare 404 stays
`RemoteMissing`-converging, the tombstone is then correct; `Err(o)` →
`o`. At most one re-resolve + one retried delete per group execution —
no loop, no retry storm (pinned by `.expect(1)` mounts, §tests).

Note the ctx difference from the update path is *real and wanted*: a
403 on the listing during a delete re-resolve classifies
`RemoteMissing` (`mapping.rs:164` — "Forbidden on a delete is gone"),
which tombstones-and-converges; the update path's E6 pin
(`Rejected { Forbidden }`) is untouched because it passes the read
ctx. Pinned by test D5t.

### D2 — bare-404 convergence, stated

A delete-path 404 that survives the re-resolve (card absent from both
fresh listings, or gone again on the retried delete) reports
`RemoteMissing`, R9e tombstones the task, the binding clears, and the
pull adopts nothing: the delete's intent is *already satisfied
server-side* — convergence, not loss. The loss case was only ever the
card that still lives at a different stack; that is what the
re-resolve fixes.

## §tests — TDD catalogue

Tier rule (AGENTS §5, one-behavior-one-test-lowest-layer): the #314
per-op semantics and the #319 re-resolve arms are **executor-tier**
(`src/push_exec/tests.rs` — wiremock `MockServer`-mounted, the tier
the issue's "wiremock tier" names; report-travel of rejection
outcomes is already pinned actor-tier by `a5_a6`). The #319
twin-re-adoption manifests only through a full cycle, so it gets
**one actor-tier test**. All assertions are typed outcomes /
request-shape facts — never log or label text; no sleeps (the
executor tier is `await`-direct; the actor tier uses the established
`next_report` polling).

### #314 — `src/push_exec/tests.rs` (new tests T1–T6)

Mounting: card detail GET and field PUT answer 200 as in
`update_completes_subsumed_label_ops_by_resolvability` (`tests.rs:704`);
per-label sub-endpoint responses are mounted with
`body_partial_json(json!({"labelId": N}))` matchers (the client sends
`{"labelId": N}`, `client.rs:1284`) or, where ordering matters, one
closure mock inspecting the body (the DeckSim pattern).

- **T1** `update_assign_rejection_completes_ops_per_class` — overlay
  labels {A→2, B→3}, fetched card unlabelled; `assignLabel(2)` → 200,
  `assignLabel(3)` → 403. Subsumed: Assign(A), Assign(B), UpdateTask,
  MoveTask. Assert: primary `Applied { echo: Some(_) }`; Assign(A) →
  `Applied { echo: None }`; Assign(B) → `Rejected { Forbidden }`;
  Update/Move → `Applied { echo: None }`; exactly 5 outcomes. *Fails
  pre-fix (Assign(B) is `Applied` today — F1).*
- **T2** `update_assign_404_routes_remote_missing_to_the_label_op_not_the_primary`
  — `assignLabel` → 404. Assert: the label op → `RemoteMissing`; the
  primary stays `Applied { echo: Some(_) }` (F4 — never the primary's
  slot, no task-tombstone fuel). *Fails pre-fix (outcome is
  `Applied`).*
- **T3** `update_remove_rejection_completes_ops_per_class` — fetched
  card labelled {3, 4}, desired ∅; `removeLabel(3)` → 200,
  `removeLabel(4)` → 403. Subsumed: Unassign(→3), Unassign(→4),
  UpdateTask. Assert: →3 `Applied`, →4 `Rejected { Forbidden }`,
  Update `Applied`. *Fails pre-fix (the remove arm shares the same
  `label_delta_failure` bug — both arms must be pinned, F5).* 
- **T4** `update_rejection_does_not_shadow_unresolvable_label_ops` —
  `assignLabel(3)` → 403; subsumed Assign(unresolvable label). Assert:
  `Rejected { LocalData }`. *Fails pre-fix (F2 — the #310 shape
  surviving on this path).*
- **T5** `update_rejection_leaves_unreached_label_ops_queued` —
  desired {2, 3, 5} (all bound), fetched unlabelled; assign 2 → 200,
  assign 3 → 403 (5 never attempted); subsumed Assign(→5) and
  Unassign(→4, a label on the fetched card whose remove phase never
  ran). Assert: neither op id appears in the outcomes vec (D1a);
  outcome count exact. *Fails pre-fix (both are `Applied`).*
- **T6** `label_delta_transport_failure_aborts_the_cycle` — raw-TCP
  server serving the GET and the PUT (`Connection: close`), then
  dropping the connection so `assignLabel` fails transport-class (the
  `serve_one_create` pattern, `tests.rs:2431`, extended to two
  responses). Assert: `Aborted { landed: empty }`. *Pins the
  preserved transport arm through the restructure — currently
  unpinned anywhere (F5 adjacent).*

### #319 — `src/push_exec/tests.rs` (new tests D1t–D5t)

- **D1t** `delete_404_re_resolves_and_deletes_at_the_fresh_binding` —
  binding (7, 55); DELETE@7 → 404; active listing carries 55 under
  stack 8; archived `[]`; DELETE@8 → 200 (deleted-card body). Assert:
  primary `Applied { echo: Some(Task) }` with the stack-8 ref;
  `.expect(1)` on each DELETE and on each listing GET (exactly one
  re-resolve, no retry storm).
- **D2t** `delete_404_with_the_card_absent_from_both_listings_stays_remote_missing`
  — DELETE → 404; both listings without the card. Assert: the
  single primary outcome is `RemoteMissing` (D2; subsumed ops, if
  any, stay queued — primary-only, matching `classify`'s failure
  shape).
- **D3t** `delete_404_located_but_gone_on_the_wire_converges_remote_missing`
  — listing carries 55 under 8; DELETE@8 → 404. Assert:
  `RemoteMissing` (D7 — the write path outranks the listing).
- **D4t** `delete_re_resolve_consults_the_archived_listing` — active
  `[]`; archived listing carries 55 under stack 9; DELETE@9 → 200.
  Assert: `Applied` (the listing pair, same presence truth as the
  pull).
- **D5t** `delete_re_resolve_with_a_forbidden_listing_converges_remote_missing`
  — DELETE@7 → 404; stacks listing → 403. Assert: `RemoteMissing`
  (delete ctx: 403-on-delete is gone — the deliberate divergence
  from the update path's E6).

A listing transport failure during the delete re-resolve aborts the
cycle through the same `relocate_card` → `classify(None)` path E5
already pins for the update ctx; transport classification is
ctx-independent (`mapping.rs:173`), so no delete-path duplicate (D6,
recorded).

### #319 — `tests/actor_wiremock.rs` (one new test)

- **A1** `a20_remote_moved_card_delete_lands_without_a_twin` — the
  a18 fixture pattern (`actor_wiremock.rs:1135`): local task bound at
  (BOARD, STACK=7, card 55) with a queued `DeleteTask`; remote truth
  keeps 55 under stack 8. Mounts: DELETE@7 → 404; DELETE@8 → 200 with
  the card body; the `/stacks` listing served by a call-counting
  closure — first serving (the re-resolve) nests 55 under 8, later
  servings (the pull) omit it; `/stacks/archived` → `[]`; the boards/
  board-detail pulls per a18. Assert (typed): no `RemoteMissing` in
  `pushes`; an `Applied { echo: Some(Task) }` whose ref is stack 8 /
  card 55; the post-cycle snapshot carries **no live view for card
  55** (the tombstone finalized, nothing re-adopted — no twin).
  *Fails pre-fix: today's report is `RemoteMissing` and the pull
  re-adopts the card.*

### Floor — existing tests that must pass unmodified

`src/push_exec/tests.rs`: `create_binds_the_echo_and_rides_label_assignments`
(:255), `delete_of_an_unbound_entity_completes_synthetic_without_sending`
(:372), `label_assignment_on_an_update_rides_the_sub_endpoint` (:464),
`label_removal_on_an_update_rides_the_sub_endpoint` (:533),
`preflight_404_on_an_update_maps_to_remote_missing` (:609),
`update_defers_when_only_unresolvable_labels_differ` (:653),
`update_completes_subsumed_label_ops_by_resolvability` (:704),
E1–E6 (:985/:1067/:1148/:1195/:1260/:1306),
`transport_failure_aborts_without_outcomes` (:1352),
`transport_failure_after_a_landed_create_keeps_its_evidence` (:1396),
`a_rejected_write_leaves_the_subsumed_ops_queued` (:1461).
`tests/actor_wiremock.rs`: a4 (:388), a5_a6 (:810), a15–a17
(:978/:1024/:1078), a18 (:1135). `tests/e2e_hermetic.rs`: i1–i4
(DeckSim untouched). `mapping.rs` unit tests: the classification
table is not modified. None of these mounts a classified failure on
a label sub-endpoint or a 404 on a bound-card DELETE (grep-verified),
so no floor expectation shifts.

## §adr — the ADR 0007 amendment note (D2, verbatim text)

Appended after the clock-seam amendment at the tail of
`docs/adr/0007-sync-actor-push-first-cycle.org`, house style (dated
heading, issue cross-ref, decision-scope statement, no rewrite of the
decision above it):

```text
* Amendment (2026-10-18): delete-path 404s re-resolve once before classification (#319)

The stale-binding re-resolve above now covers the delete path. A
local DeleteTask pushes at the overlay's ref, whose stack component
may be stale when another client moved the card: the DELETE 404s at
the stale location. On a delete-path 404 the executor re-resolves the
card once against the fresh active+archived listings (the same
presence truth the pull and the pre-write fetch trust); found → the
overlay binding is refreshed and the delete retried once at the fresh
location; absent from both listings, or a 404 on that retried delete
(the write path outranks a lagging listing), → =RemoteMissing= — for
a delete this converges rather than loses: the card is genuinely gone
from every stack, the tombstone is the correct end state, and the
same-cycle pull adopts no twin. A listing failure classifies through
the normative write table's delete column (a listing 403 is "gone"
there). The residual lagging-listing window rides the #258
amendment's accepted risk unchanged. There is no configuration knob.
```

#314 needs no ADR note: it is outcome-report mechanics inside one
executor arm, not classification policy (the #310 fix set that
precedent). Its documentation is the rewritten `label_delta_failure`
contract comment plus `push_exec/mod.rs`'s module doc sentence gains
"and the label-delta and delete arms emit per-op outcomes /
re-resolve at the fresh binding" — a one-line touch, riding commit 2/3
below.

## §comments — the two production comments that must change

- `label_delta_failure`'s doc (`execute.rs:811-816`) currently claims
  "the label ops retry next cycle" — false today (F1). Reworked to
  state the D1/D1a/D1b table: primary `Applied` (the PUT landed),
  per-op emission for subsumed ops, omissions stay queued, transport
  aborts with no landed evidence (idempotent full-send).
- `push_exec/mod.rs`'s module doc (the fetch-before-write sentence)
  and `execute.rs`'s `delete_card` arm gain the delete-path
  re-resolve mention (one sentence each; constraints, not narration).

## §execution — stage-2 commit series (each standalone, rebase-merge hygiene)

1. `fix(sync): emit per-op outcomes in the label-delta rejection arms`
   — TDD inside the commit: T1–T6 written first and run against
   unmodified production (T1–T5 fail — the recorded bug proof; T6
   passes and pins the transport arm), then the `settled` tracking,
   the shared `subsumed_label_result`/emission helper, the reworked
   `label_delta_failure`, the comment rewrite. A failing test is
   never committed alone.
2. `fix(sync): re-resolve delete-path 404s at the fresh binding` —
   D1t–D5t + A1 first (D1t/A1 fail pre-fix; D2t/D3t pass — they pin
   arms that must not regress), then `relocate_card` extraction,
   `delete_card`/`delete_after_re_resolve`, the `task` parameter,
   the mod.rs doc sentence.
3. `docs(adr): ADR 0007 amendment — delete-path 404 re-resolve (#319)`
   — §adr's note, plus a grep for other docs quoting the delete-arm
   classification (`docs/architecture.org`, crate docs) with a
   pointer-style update where they name it.

Gates: the full AGENTS §10 suite. No CHANGELOG touch (chunk 5 owns
`[Unreleased]`). No push, no PR (chunk 5's stage-3 ships the round).

## §decisions — summary

| Id | Decision | Status |
|---|---|---|
| D1 | Per-op emission classes in the classified arms (primary `Applied{echo:Some}`, update/move synthetic `Applied`, unresolvable `Rejected{LocalData}`, failing label's ops carry the classified result, transport `Aborted`) | settled (dispatch §2) |
| D1a | Never-attempted label ops are **omitted** from the report (stay queued) | recorded here — the fourth class D1 doesn't name; omission is the only non-lying semantics (`pipeline.rs:909-911`) |
| D1b | Convergence tracking via a `settled` set threaded through both delta loops | recorded here — mechanism for the "write succeeded → Applied" class |
| D2 | Bare 404 after the re-resolve stays `RemoteMissing`-converging; ADR 0007 amendment note | settled (dispatch §2) |
| D4 | Lazy re-resolve on the delete-path 404 (mirror `fetch_card_for_write`), not a pre-write fetch | recorded here — matches the #258 seam shape, avoids a per-delete GET, keeps the delete ctx |
| D5 | Shared `relocate_card` helper; delete passes the delete ctx (listing 403 → `RemoteMissing`) | recorded here |
| D6 | Test tiers: executor tier for per-op semantics and re-resolve arms; one actor-tier twin test; no e2e additions; no duplicate of E5's transport-listing pin | recorded here (AGENTS §5) |
| D7 | Second-delete 404 after a successful locate converges `RemoteMissing` (write path outranks the listing) | recorded here — the delete analogue of the detail-outranks-listing rule |

## §non-goals

- **No `apply_push_report` changes** — the completion semantics,
  R9e/R9c arms, and dead-lettering are untouched; the fix only stops
  feeding the report lies.
- **No pull-side, pipeline, or domain production changes** — diff
  confined to `taskboard-sync-nextcloud` (+ the ADR file + the two
  doc sentences in §comments).
- **No classification-table changes** — `mapping.rs` is not modified;
  both fixes route existing classifications to better recipients.
- **No outbox reordering, no planner changes, no `move_task` label
  phase** (moves carry no label delta).
- **No DeckSim changes** — its DELETE handler ignores the stack
  component (a fixture infidelity that let #319 hide from the e2e
  tier); making it stack-faithful is deliberately left to the sync
  test-gap chunk (dispatch §3) rather than risked here.
- **No CHANGELOG entry** — chunk 5 owns `[Unreleased]` for the round.

## §risks

- **`last_success` observable shift** (§design-314 consequences): a
  partially-rejected update group no longer bumps the stamp. Correct
  per domain semantics; chunk 5's changelog entry should mention it.
- **RemoteMissing finalizes label ops on 404** (T2's consequence): a
  remotely-deleted label's pending assignment is dropped, not
  retried; convergence rests on the pull tombstoning the label row.
  Same trust model as R9e's existing label-op arm — but newly
  *reachable* from this arm, so A1-style actor scrutiny does not
  cover it; the executor-tier pin (T2) is the guard.
- **a20 fixture complexity**: the count-based listing closure must
  serve present-then-absent bodies on one URL; if the pull's ETag
  dance (304 paths) interferes, mount distinct ETags per serving so
  the cache misses (the a10/a2 pattern documents the cache rules).
- **Listing-lag residual** (inherited from #258): a card moved so
  recently that both fresh listings omit it still misclassifies
  `RemoteMissing` → tombstone → possible twin if the card is alive.
  Accepted risk, same as the pull's — stated in the ADR note.
- **Same-file chain pressure**: chunks 2–4 follow on this branch; the
  restructure of `update_task`/`delete_card` shifts line anchors the
  chunk-2 plan must re-derive (its prompt already says to).
- **Coverage gate**: all new tests are wiremock/raw-TCP-backed
  instrumented binaries, like the rest of the crate's suites — no
  change to the quarantine dance expected.
