# [Plan] Duplicate-Create Crash-Window Reconcile (DR-4/R)

- **Date**: 2026-10-28
- **Kind**: stage-1 implementation plan for chunk DR-4/R of the
  design-rounds batch dispatch — the #150 fix. TDD-first; this file is
  written before any code and is the chunk's contract.
- **Origin** (all normative):
  - `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
    §3 D-5 a–f (the design), §2 facts 9/12–13/16, §4 F-150/F-150b
    (both confirmed as recommended), §5 (the false-positive risk).
  - `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
    §1 (wave 6, merge/pipeline single-flight lane), §9.2 (chunk scope +
    acceptance + the three stage prompts).
  - ADR 0007 (push-first, the crash windows) — gains a dated note;
    ADR 0010 (the connect ceremonies) — W2 runbook sentence amended
    per F-150b. Amendment-not-rewrite, both.
- **Branch / worktree**: `feat/sync-duplicate-reconcile` in
  `../taskboard-WT-feat-sync-duplicate-reconcile`, branched off
  `origin/main` @ `7355fcb`. Preconditions verified: DR-4/D merged —
  PR #535 rebase-merged 2026-10-28T17:19Z; the local
  `feat/sync-resurrect-contention` ref has new SHAs (rebase-merge) and
  all 8 of its commits are patch-equivalent to `origin/main` per
  `git cherry` (`-` on every line). `scripts/worktree-doctor.sh`
  exit 0 (231G free, floor 25G). No migration is assigned to this
  chunk (the dispatch's ladder claims 0012 for wave-6 mate DR-3/2;
  this chunk stores nothing new).
- **Lane check**: wave-mate DR-3/2 (`feat/reminder-insistence`, (w) +
  mig 0012 + CLI lane) and `feat/ui-r3-legacy-config-migration` (ui
  lane) are in flight; neither touches `merge.rs`/`pipeline.rs`/
  `push.rs`/the sync crate, and this chunk touches no `AppState` field
  (the insta shape-widener lane is DR-3/2's alone). The merge/pipeline
  lane is single-flight with this chunk. The outbox **payload** gains
  two `LocalOp` variants — that is local-cache serde, not the
  `AppState` shape; no shared snapshot regenerates.
- **Status**: READY FOR STAGE-2. Every open question is resolved in
  §7 (decision record), including the stack/board DTO verification the
  dispatch prompt asks for (§4/§6, DEC-1). No implementation has
  happened.

---

## 1. Mission & binding constraints

A crash between a create POST completing server-side and the landing
batch committing locally leaves the create op queued; the replay mints
a second card; the pull then R1-adopts the first card as a *new local
task* — a duplicate on both sides. This chunk teaches the pull to
recognize that duplicate as ours and collapse it: one survivor card,
one local task, the rest deleted through the 404-tolerant delete path —
triggered **only** when an observed duplicate exists (structural, not
configured). The same collapse applies to stacks (with a survivorship
inversion, DEC-3) and to the `boards publish` orphan (W2, in the
ceremony, per F-150b).

Non-negotiable (design §1 + dispatch §9.2 + AGENTS):

- **The sync actor stays a translator.** Detection and survivorship
  are pure domain functions the pipeline composes; the actor gains a
  mapping field (`created_at`) and executor arms for the new
  ref-scoped deletes — no policy crosses into `taskboard-sync-nextcloud`.
- **Purity**: the predicate reads only injected data (snapshot cards,
  post-echo tasks, outbox ops, stacks, index, `now`). No clocks, no
  ids, no IO. Fully proptest-able.
- **Trigger-only-on-duplicate**: with no pending/just-pushed create in
  scope, the pass is a no-op *by construction* — pinned as a property,
  not a config flag (D-5c: "No config: a duplicate we created seconds
  apart is never worth keeping").
- **Honesty surface** (D-5f): `tracing` typed facts under a new rule
  tag `R-dup` only. No `SystemEvent`, no UI churn — a test pins the
  empty events vector on the collapse path.
- **Test discipline** (AGENTS §5): typed outcomes, no text
  assertions, each detection clause pinned independently plus its
  near-miss negatives, proptest for the structural no-op, the
  deterministic crash journey via `tokio` paused time / `eventually`
  polling — no sleeps.
- **Existing stock untouched**: the DR-4/G baseline proptests, the
  DR-4/D resurrect snapshots, and the R9 echo tests must pass
  unchanged.

## 2. Verified baseline — numbered facts

All against `origin/main` @ `7355fcb` (worktree HEAD).

1. The Deck card DTO parses `created_at: i64` epoch seconds with
   `#[serde(default)]` (`model.rs:118`); `map_card` drops it
   (`mapping.rs:76-98`). `RemoteTask` has no creation time
   (`remote.rs:74-99`). **The stack DTO has no `created_at`** — its
   wire shape is `id/title/boardId/deletedAt/order/lastModified/
   archived/cards` (`model.rs:80-96`); the 2026-10-22 transport
   characterization records `createdAt` on cards and attachments only.
   **The board DTO likewise** (`model.rs:18-46`). This is DEC-1's
   verification outcome.
2. The epoch sentinel rule exists: `epoch_secs` maps `raw <= 0` (or
   unparseable) to `DateTime::<Utc>::MIN_UTC` (`mapping.rs:31-36`),
   unit-pinned by `epoch_zero_sentinel_maps_to_the_minimum_datetime`.
3. `PendingOp { op_id, op, queued_at }` — the outbox entry carries the
   queue stamp (`outbox.rs:64-71`). The outbox slice the engine
   receives in `apply_sync_report` is the *pre-cycle* persisted
   outbox: a create whose echo lands this cycle is still in it, so
   `queued_at` is available to the pass in **both** clause-2 arms.
4. `apply_sync_report`'s fixed order (`pipeline.rs:418-1041`):
   §1 push outcomes (R9 echoes bind; `task_by_board_card` built
   post-echo) → §2 board → §3 stacks → §4 labels → §5 tasks
   (R1 adoption of unbound cards happens **inside** this loop) → §5b
   sidecar fold → §6 presence reconciliation (R3 + resurrect) → §6b
   stack cascade → §7 persistence batch (`actions_for_changed_entities`
   diffs against `current`; op completions emit `CompleteOp`/`FailOp`).
5. R1 task adoption (`pipeline.rs:675-686`): an unbound snapshot card
   mints `ids.new_task_id()` and adopts when its stack resolves
   (`adopt_remote_task`, `merge.rs:167`). This insertion is the
   duplicate row the pass must prevent.
6. `op_results: HashMap<OpId, bool>` semantics: `true` ⇒ `CompleteOp`,
   `false` ⇒ `FailOp`; **any** `false` stamps the board phase
   `Failed { BadRequest }` (`pipeline.rs:984-992`). The resurrect path
   cancels ops with `false`; a satisfied create must complete with
   `true` (DEC-6).
7. Deletes are task-scoped: `MaterializedOp::DeleteTask { task }`
   resolves the ref through `BindingOverlay::card_of` at push time
   (`push_exec/execute.rs:82-84`); an unbound entity's delete group
   materializes `Noop`. **There is no delete lane for a remote ref
   with no local binding** — the collapse needs one (DEC-7).
8. The client already has every write the collapse needs:
   `delete_card(board, stack, card)` (`client.rs:1148`),
   `delete_stack(board, stack)` (`:1075`), `delete_board(id)`
   (`:533`), `boards()` listing (`:414`). `classify_push` maps a 404
   on a delete-class write to `PushResult::RemoteMissing`
   (`PushCtx { existing: true, delete: true }`); `apply_push_outcomes`
   completes RemoteMissing ops (`pipeline.rs:1426-1489`) — the
   "404-tolerant by construction" the design names.
9. R3 reads presence from `snapshot.tasks` **plus this cycle's task
   echoes** (`pipeline.rs:764-776`), then reconciles bound-but-absent
   refs. A collapsed duplicate never becomes a binding, so R3 is
   unaffected; the pass must run before §6 so R3 sees post-rebind
   refs. §6's resurrect carve-out (DR-4/D) and this pass are disjoint:
   an absent card is never a candidate (candidates come from
   `snapshot.tasks`).
10. `reconcile.rs:37-39` — `normalized_title` is trim + case-fold;
    the bind ceremony's `plan_reconcile` neutralizes a local duplicate
    as a *ref-less tombstone* (LOCAL duplicates only — no remote
    delete exists there). Vocabulary reuse only; no code reuse: the
    crash-window duplicate is remote-live and needs the delete push.
11. The R9 echo adoption (`merge.rs:891`, called at
    `pipeline.rs:1319-1342`) writes the echo's server-normalized
    content **into the local task** — so after §1, the bound task's
    fields ARE the bound card's content. Clause 4 compares candidate
    vs task fields directly; no second snapshot lookup is needed.
12. `RemoteTaskBaseline::of(remote)` exists (`merge.rs:217` usage) —
    the rebind refreshes the baseline with it (D-1c's
    every-adoption-point rule extends to the rebind).
13. The engine+actor journey harness exists:
    `tests/e2e_hermetic.rs` — `DeckSim` (stateful in-wiremock Deck:
    minted card ids, delete route, op log), `boot`/`spawn_actor`/
    `nudge_cycle`/`eventually` (deterministic predicate polling), and
    I1 as the create-round-trip template. The report-level template is
    `a1_nudge_pushes_the_create_and_pulls_it_back`
    (`tests/actor_wiremock.rs:288`).
14. The publish ceremony is `commands/boards.rs::publish_on`
    (`:658-742`): guard → `client.create_board` → engine
    `BindBoard` → `bind_nudge_wait_reconcile` (nudge → wait for
    adoption → automatic `ReconcileByTitle`). Its own doc comment
    names the W2 orphan and defers to "the duplicate-create
    crash-window backlog entry" — this chunk. The sweep slots between
    `BindBoard` and the nudge (§6).
15. `RULE_*` consts are `pub(crate)` in `merge.rs:64-69`; the pass adds
    `RULE_R_DUP: &str = "R-dup"` beside them.
16. Deck full-sends card creates from `NewCardShape`
    (`push.rs:156-174`: title, order, description, duedate,
    scheduled, done, archived, labels). Two POSTs of the same op send
    identical bodies — except `order` (each POST appends at the
    server-chosen next position) and labels *only if* overlay
    resolution deferred on one attempt (post-create sub-endpoint).

## 3. The design, decision by decision

### R-1 — `RemoteTask.created_at` and the mapping (D-5a)

`RemoteTask` grows `created_at: DateTime<Utc>` (serde-transparent;
`remote.rs`'s roundtrip proptest strategies in `test_support` gain the
field). `map_card` sets `created_at: epoch_secs(card.created_at)` —
the existing sentinel rule (fact 2) applies verbatim: an old server's
`0` decodes to `MIN_UTC`, which fails clause 3's window and degrades
to today's behavior (duplicate adopted). The echo path carries it for
free (`map_echo` → `map_card`). The stack and board DTOs do **not**
carry `created_at` (fact 1, DEC-1) — their variants use
`last_modified` as the creation proxy (freshly minted entities carry
`last_modified ≈ creation`; nothing touches a crash-window orphan
between its POST and the collapse).

Unit pins: `map_card` carries the stamp; the sentinel test extends to
`created_at`; the `remote_task_strategy` roundtrip keeps passing.

### R-2 — the detection predicate (D-5b), pure, four clauses

New pure function in `merge.rs` (the algebra's home), composed by the
pipeline per candidate card:

```rust
/// One crash-window duplicate group: the survivor card, the local task
/// it belongs to, every doomed card (each gets a ref-delete push), and
/// the pending create op the collapse satisfies, if any.
pub struct DuplicateCollapse {
    pub task: TaskId,
    pub survivor: RemoteCardRef,
    pub rebind: bool,                 // false ⇒ the survivor is already bound
    pub doomed: Vec<RemoteCardRef>,
    pub satisfied_op: Option<OpId>,   // pending-create arm only
}

/// Group constructor over one unbound snapshot card + the cycle's
/// create signatures. Pure; total; no IO.
pub fn detect_crash_window_duplicates(
    candidate: &RemoteTask,
    tasks: &BTreeMap<TaskId, Task>,
    stacks: &BTreeMap<StackId, Stack>,
    creates: &[CreateSignature],
    ctx: &RemoteIndex,
    now: DateTime<Utc>,
) -> Option<DuplicateCollapse>
```

with

```rust
pub struct CreateSignature {
    pub task: TaskId,
    pub op: OpId,
    pub queued_at: DateTime<Utc>,
    pub echoed_this_cycle: bool,   // outcome Applied{echo: Task} this cycle
}
```

built once per cycle in `apply_sync_report` from `(outbox, pushes,
op_results)`: every `LocalOp::CreateTask(t)` in the outbox slice is a
signature; `echoed_this_cycle` iff §1 recorded an `Applied { echo:
Task }` outcome for it; a signature whose op already has an outcome
that is *not* an applied create (rejected, RemoteMissing) is dropped.
**No signatures ⇒ the pass cannot fire** — the structural no-op is a
type-level fact, not a runtime check.

The clauses, per candidate `U` and its matching local task `T`
(§3a–§3d below), all must hold:

1. **Same normalized title, same resolved stack**: `normalized_title`
   (`reconcile.rs` — the house vocabulary, fact 13) of `U.title`
   equals `T`'s, and `U.stack` equals `stacks[T.stack].remote?.stack`.
   An unmapped local stack fails the clause (a task whose stack never
   bound could not have pushed, so it has no crash window).
2. **The crash signature**: `T` has a `CreateSignature` — either
   `echoed_this_cycle` (the replay's echo just bound `T`; `T.remote`
   is the newer card) or the op is still pending (`!op_results`
   contains it; `T.remote` is `None`). Anything else — no pending
   create, no this-cycle echo — is not our crash and never collapses.
3. **The window**: `U.created_at` (and, when `T` is bound, the bound
   card's stamp) fall in `[op.queued_at − 60s, now + 60s]`. The lower
   bound is the discriminator (a same-titled old card is a near-miss);
   the upper bound carries the same 60s tolerance to absorb
   server-ahead clock skew (DEC-4). The bound card's stamp comes from
   the task's echo-adopted state via the snapshot card when present,
   else from this cycle's echo copy (fact 11); when neither is
   available (stale 304 listing, no echo) the clause fails
   conservatively — the next full pull retries (see §5, residue 2).
4. **Content equality — byte-ours** (candidate vs `T`'s current
   fields, which for the echo arm ARE the bound card's server-normalized
   values, fact 11): `description`, `duedate`, `done`, `archived`
   equal; `scheduled` equal *iff* `features.card_startdate` (when the
   capability is off the create never sent it — comparing would
   false-negative on the local value, DEC-8); `title` equality is
   clause 1's job; `order` and `labels` are **excluded** (order
   differs by server construction between the two POSTs, fact 16;
   labels ride a post-create sub-endpoint whose deferral is
   timing-dependent — DEC-8).

Group formation and survivorship (D-5c) fall out of the constructor:
all candidates matching one `T` form the group together with `T`'s
bound card (when its stamp is in-window). The survivor is the
**oldest** member by `created_at`; on an exact stamp tie (Deck's
seconds resolution makes this real) the **bound card wins** — no
rebind churn for an undecidable order (DEC-5). Every non-survivor
member is doomed. Determinism under candidate arrival order is
pinned by permutation tests (§5, T-9).

### R-3 — survivorship mechanics (D-5c) and R1 suppression

The pipeline pre-computes the create signatures, then runs the §5
task loop with one guard at the top of the adoption else-branch: a
candidate with a `DuplicateCollapse` verdict is **skipped before
`ids.new_task_id()`** — no id burned, no row inserted, no
`ctx`/`newly_adopted` cleanup (this is the R1 suppression; it
deviates from D-5b's literal "after the task-merge loop" position and
is strictly stronger — DEC-2). The verdict's effects apply as:

- **Rebind arm** (survivor is the unbound older card, the canonical
  case): a new pure helper `rebind_task_to_survivor(local: &Task,
  survivor: &RemoteTask) -> Task` sets `remote = Some(survivor.id)`,
  `remote_seen = survivor.last_modified`, `remote_baseline =
  Some(RemoteTaskBaseline::of(survivor))` (fact 12 — the D-1c rule),
  and touches **nothing else** (field values and clocks stay local:
  content equality was clause 4's job; labels/order of the survivor
  land via the next pull's R4 compare — a tie keeps local, which is
  the pushed truth). The rebind happens in the loop, before §5b.
- **Doomed cards**: one `PersistenceAction::EnqueueOp` per doomed ref
  (next section) — collected in a `dup_enqueues` vec that rides §7
  last, beside `resurrect_enqueues`.
- **Pending-create arm** (`satisfied_op`): the op completes with
  `true` (`CompleteOp`) — its effect exists server-side (the survivor
  IS the first POST's card) and a `false` would stamp the board
  `Failed` (fact 6, DEC-6). `T` rebinds to the survivor exactly as
  above; nothing is deleted remotely (the survivor is the only card).
- **Bound-card survivor** (tie, or the bound card is the oldest):
  no rebind, `T` untouched; the doomed candidates get delete pushes
  only.

The delete primitive (fact 7 — no unbound-ref delete lane exists):

```rust
// outbox.rs — LocalOp grows two variants (the chunk's only payload change):
DeleteRemoteCard { board: BoardId, card: RemoteCardRef },
DeleteRemoteStack { board: BoardId, stack: RemoteStackRef },
```

`owning_board` answers `Some(board)` (exact cycle attribution — not
the orphan lane); `op_targets_task` answers `false`. Mechanics per
layer: `push.rs` — `MaterializedOp::DeleteRemoteCard { card }` /
`DeleteRemoteStack { stack }`, new `PushTarget::RemoteCard/
RemoteStack` ranks beside the task/stack groups, planner arms
materialize one single-op group each (no coalescing — the ref carries
no `TaskId` to coalesce by); `push_exec/execute.rs` — the arms call
`client.delete_card/delete_stack` with the ref's components, building
`PushCtx { existing: true, delete: true }` so a 404 classifies
`RemoteMissing` (fact 8); transport errors ride the existing
`GroupOutcome::Aborted` path; `apply_push_outcomes` — the new
variants complete under `Applied { echo: None }` and under
`RemoteMissing` (the card is gone either way — goal achieved), with
**no tombstone side effects** (there is no local row to tombstone);
`Rejected` keeps today's dead-letter rule unchanged. The outbox codec
stores `PendingOp` as JSON (`codec.rs`) — new enum variants need no
migration; a roundtrip test pins them.

Sidecar note: `T`'s document entries stay keyed under the doomed
card's id until the next flush re-keys to the survivor; the doomed
card's remote appdata dies with its card. No action.

### R-4 — the stack variant (D-5d), with the survivorship inversion

DTO verification (DEC-1, the outcome the dispatch asks to be
reported): the stack wire shape carries no `created_at` (fact 1) ⇒
per D-5a's fallback, the stack predicate uses `last_modified` in the
window clause and skips the stamp comparison between the two stacks.

Predicate (same four-clause shape, stack vocabulary): normalized
title equal + same board (clause 1); `CreateStack` signature —
pending op or this cycle's stack echo (clause 2); the unbound
stack's `last_modified` in `[op.queued_at − 60s, now + 60s]`
(clause 3); `order` equality (clause 4 — the create-stack body is
title + order; archived defaults false on both).

**Survivorship inverts (DEC-3).** D-5d says "keep the older" — for
stacks that would delete the *content carrier*: every replayed card
create resolves its stack through the binding overlay, i.e. into the
**bound** (newer) stack, and the unbound orphan is provably empty
(it was never bound, so no card could ever have targeted it). A
naive keep-older collapses the duplicate stack *and every card in
it* (Deck's stack delete cascades). The rule is therefore: **keep
the bound stack; delete-push the unbound orphan; no rebind** — one
mental model with the card rule ("the survivor owns the remote-side
content": for cards that is the older POST, which accrues comments;
for stacks the bound one, which holds the cards). Guard: if the
orphan's nested snapshot cards contain any ref that resolves to a
live bound local task, skip the collapse with an `R-dup` trace
(multi-window mess; leave both, document) — the lossless-by-
construction path never fires it. Mechanics: the §3 stack loop gets
the same skip-before-adopt guard; the doomed orphan gets
`DeleteRemoteStack`.

### R-5 — the boards W2 sweep in the publish ceremony (D-5e/F-150b)

Site: `publish_on`, after `BindBoard` commits and **before**
`bind_nudge_wait_reconcile` (fact 14) — one extra `client.boards()`
listing, one compare, so the nudge/wait runs once against the final
binding. Predicate for an orphan, per listed live board `B ≠ `
just-created: `normalized_title(B.title) == normalized_title(local
title)` **and** `B.color == minted color` **and** `B.last_modified ∈
[just_created.last_modified − 10 min, just_created.last_modified]`
(DEC-11 — boards carry no `created_at` (fact 1/DEC-1), the stamp is
the creation proxy, and the 10-minute window covers the human-scale
crash→rerun gap while a user's coincidental same-title *and*
same-color board inside it is vanishing; color equality is free and
tightens hard).

Survivorship keep-older per D-5e (the just-bound board wins identity
only if it is older): if the just-created board is older, keep it and
delete-push each orphan through `client.delete_board`. Otherwise
**rebind** to the oldest orphan — `UnbindBoard` then `BindBoard`
through the engine (typed refusals are the ceremony's existing error
surface) — and delete-push every other candidate including the
just-created one. The subsequent nudge/wait/reconcile and the receipt
then name the survivor. N candidates collapse to one survivor (the
multi-attempt generalization). Failure inside the sweep degrades to
today's behavior (the orphan stays; ADR 0010's runbook sentence
remains the documented floor) — a sweep error is swallowed into an
`R-dup`-tagged warning, never fails the publish.

Tests ride the app-crate wiremock pattern (`tests/cli_connect.rs`):
mount `POST /boards`, a listing with the orphan injected, the
`DELETE /boards/{id}` expectation; assert the engine binds the older
id and the delete fired; near-miss: orphan outside the window ⇒ both
boards listed, none deleted, binding unchanged.

### R-6 — the crash journey (dispatch acceptance)

`tests/e2e_hermetic.rs`, DeckSim + engine + actor (fact 13),
deterministic, no sleeps:

1. Boot sim + stack; engine-create task `T` (unbound, `CreateTask`
   queued); the sim's stack exists; `created_at` on minted cards is
   sim-assigned epoch seconds (the fixture gains the field — absent
   `createdAt` would decode to `MIN_UTC` and silently defeat clause 3).
2. **The crash**: snapshot the repo state, nudge one cycle (the POST
   lands server-side: sim card A), then rebuild the stack on the
   saved pre-cycle repo state — storage is exactly "POST completed,
   landing batch lost"; the actor is dropped (restart).
3. **The replay**: fresh engine+actor on the restored repo; the sim
   still holds A. Nudge: push-first replays the create → sim mints
   card B (echo binds `T`→B); the pull lists A+B → the collapse fires:
   `T` rebinds to A, the ref-delete op for B is enqueued, B's R1
   adoption is suppressed.
4. Nudge again: the ref-delete drains (sim deletes B). `eventually`:
   exactly one live task, bound to A's ref, `remote_seen ==
   A.last_modified`, baseline set; outbox empty; `sim.cards == [A]`.
5. Typed-fact assertions throughout: no `SystemEvent` on any cycle
   (the honesty rule, pinned structurally).

### R-7 — the honesty surface (D-5f) and ADR notes

`tracing::debug!` with `rule = RULE_R_DUP` and typed fields only
(`task_id`, `survivor`, `doomed.len()`, `satisfied_op`, per-clause
facts where a clause fails silently — no, failures are *not* traced
at debug; only collapses and the stack-guard skip trace). No
`SystemEvent`, no UI change — pinned by the empty events vector in
the pipeline tests and the journey.

- **ADR 0007** (`docs/adr/0007-…`): one dated note — the W1
  create-window is now reconciled at pull time under the four-clause
  predicate; the window itself remains (Deck has no idempotency keys;
  a response-lost replay can still mint an extra card, and a pull
  failure in the echo cycle degrades to today's adoption — residues
  §5). Amendment-not-rewrite.
- **ADR 0010** (`docs/adr/0010-…`): the W2 runbook sentence
  (`:118-120` "delete it in the Deck UI") is amended: the ceremony
  sweeps same-title orphans inside the window; the runbook stays the
  floor for anything the window misses. Per F-150b's confirmation.

## 4. What does NOT change

- `merge_task`, the R1–R9 rules' semantics, the self-clobber guards,
  the R3 resurrect carve-out (DR-4/D), the sidecar fold, the OR-set
  label algebra (DR-4/L), the baseline gating (DR-4/G) — the pass
  composes around them; one tracing tag and one helper join
  `merge.rs`.
- `reconcile.rs` (the bind ceremony) — different duplicate class
  (local-vs-local, no remote delete); vocabulary reuse only.
- The pull cache, validators, conditional reads, push-first ordering.
- No migration, no `AppState` field, no config knob, no IPC surface.
- The UI and CLI — zero diff.

## 5. Test plan (TDD order)

Lowest layer first; one behavior, one test; typed outcomes only.

- **T-1 (mapping.rs)**: `map_card` carries `created_at`; the
  epoch-zero sentinel extends to `created_at` (extends the existing
  sentinel pin). `remote_task_strategy` gains the field; serde
  roundtrips keep passing untouched.
- **T-2 (outbox.rs)**: the two `LocalOp` variants' `owning_board`
  and `op_targets_task` arms; serde roundtrip of a `PendingOp`
  carrying each variant; the storage codec roundtrip
  (`taskboard-storage-sqlite` codec tests) pins the persisted shape.
- **T-3 (push.rs)**: the planner materializes one single-op group per
  ref-delete; it does not coalesce with any task-scoped group; group
  ordering stays stacks→labels→tasks with ref-deletes beside their
  class.
- **T-4 (push_exec tests)**: a ref-delete card group executes
  `delete_card` with the ref's components; a 404 completes the op
  (`RemoteMissing`); a 200 completes with no echo; transport failure
  rides `GroupOutcome::Aborted`. Stack twin.
- **T-5 (pipeline.rs, clause pins — the chunk's core)**: per clause,
  a positive and its near-miss negatives, all through
  `detect_crash_window_duplicates` on hand-built states:
  - T-5a clause 1: same title, **different stack** ⇒ `None`; same
    title, unmapped local stack ⇒ `None`.
  - T-5b clause 2: same title/stack/content/window but **no create
    signature** ⇒ `None` (the stranger's twin card is adopted
    normally — pin `apply_sync_report` R1-adopts it unchanged).
  - T-5c clause 3: created **before `queued_at − 60s`** ⇒ `None`;
    after `now + 60s` (server-ahead skew extreme) ⇒ `None`.
  - T-5d clause 4: **description mismatch** ⇒ `None`; duedate
    mismatch ⇒ `None`; with `card_startdate = false`, a scheduled
    mismatch does **not** reject; with it on, it does.
  - T-5e positive: all four hold ⇒ collapse with the older card as
    survivor, rebind flag set.
- **T-6 (pipeline.rs, survivorship)**: the tie (equal `created_at`)
  keeps the **bound** card (no rebind, candidate deleted); permutation
  property — for both orderings of [A, B] in `snapshot.tasks` the
  survivor/domed split is identical; multi-orphan (A, B unbound, C
  bound) collapses to one survivor and two delete pushes;
  `remote_seen`/`remote_baseline` land on the survivor's stamps; the
  pending arm completes the op with `true` and the board phase stays
  `Idle` (fact 6); the echo arm leaves the op's completion to §1.
- **T-7 (pipeline.rs, integration pins)**: end-to-end
  `apply_sync_report` on the canonical two-card report — zero new
  task rows, rebind applied, `EnqueueOp` actions present, `system_events`
  empty (the honesty pin); the stack variant — orphan suppressed,
  bound stack untouched, `DeleteRemoteStack` enqueued; the stack guard
  (orphan holding a bound card) skips with the collapse refused.
- **T-8 (proptest)**: the structural no-op — for arbitrary generated
  states with an empty `creates` slice, the pass yields no verdicts
  and `apply_sync_report` output equals today's output; plus the
  survivor-selection total-order property (oldest wins; tie ⇒ bound).
- **T-9 (e2e_hermetic.rs)**: the crash journey per §3-R6 — the
  dispatch's acceptance verbatim (create POST → simulated crash →
  restart → replay POST → duplicate injected → pull reconciles to one
  card, zero duplicate local rows).
- **T-10 (actor_wiremock.rs)**: report-level pin — the replay cycle's
  `Completed` report against a server holding both cards still
  produces exactly one create echo and the pass's inputs (guards the
  actor/translator boundary).
- **T-11 (app tests)**: the W2 sweep per §3-R5 — bind-older +
  delete-newer; rebind path (just-created newer ⇒ `UnbindBoard` +
  re-`BindBoard` to the older, the just-created deleted); outside-
  window near-miss ⇒ no deletion, publish unaffected; sweep failure ⇒
  publish still succeeds (typed receipt).

Mutant targets (D-8): each clause's comparison, the window's two
bounds, the tie-break, the rebind's three field writes, the `true`
completion of the pending arm — each with its zero-expected-survivors
set.

## 6. Implementation order (stage-2 guidance)

1. T-1 + `RemoteTask.created_at` + mapping (the translator change).
2. T-2 + the `LocalOp` variants; T-3/T-4 + planner/executor arms
   (the delete primitive is a prerequisite of every collapse test).
3. T-5/T-6/T-8 — the pure predicate + survivorship, the chunk's core.
4. T-7 — the pipeline composition (signatures precompute, loop guard,
   `dup_enqueues` in §7).
5. T-9/T-10 — the journeys.
6. T-11 — the W2 sweep in `publish_on`.
7. ADR 0007/0010 notes; full gate suite per AGENTS §10 through
   `scripts/run-quality-gates.sh`.

## 7. Decision record (every open question, resolved)

- **DEC-1 — stack/board DTO verification (the dispatch's named
  report item)**: neither the stack nor the board wire shape carries
  `created_at` (fact 1: `model.rs:80-96` / `:18-46`; the 2026-10-22
  transport characterization found `createdAt` on cards and
  attachments only). D-5a's fallback applies to both: `last_modified`
  is the creation proxy in the window clause; no inter-card stamp
  comparison for stacks. `RemoteTask` alone gains the field.
- **DEC-2 — suppression position**: D-5b's literal "pass after the
  task-merge loop" would let R1 insert-then-remove the duplicate row.
  Implemented instead as skip-before-adopt inside the loop (verdicts
  precomputed after §1, applied in §5's else-branch) — strictly
  stronger ("no local duplicate row is ever created" holds in memory,
  not just storage; no id burn, no index cleanup) and identical
  relative to R3 (before §6). Recorded as a position refinement, not
  a semantics change.
- **DEC-3 — stack survivorship inverts**: keep the **bound** (newer)
  stack, delete the unbound orphan. Rationale: every replayed card
  resolves into the bound stack (overlay) and the orphan is provably
  empty (never bound ⇒ untargetable), while Deck's stack delete
  cascades its cards — D-5d's keep-older would destroy content. The
  unified principle across cards/stacks/boards: *the survivor owns
  the remote-side content*. The card rule keeps the design's
  keep-older (comments accrue on the first POST). Guard + trace for
  the never-by-construction "orphan holds bound cards" case.
- **DEC-4 — window bounds**: `[op.queued_at − 60s, now + 60s]` — the
  design's lower bound verbatim; the upper bound gains the same 60s
  tolerance because a server-ahead clock would otherwise exclude the
  true duplicate while buying zero false-positive protection
  (nothing is created in the future). False-positive surface
  unchanged: a stranger's twin still needs same title/stack/content
  within a minute of our create.
- **DEC-5 — stamp tie**: equal `created_at` (real at Deck's
  seconds resolution) ⇒ the bound card survives — deterministic, and
  keeping the binding avoids rebind churn the data cannot adjudicate.
- **DEC-6 — op completion polarity**: the pending-create arm
  completes with `true` (`CompleteOp`): the create's effect exists
  server-side (the survivor is the first POST's card); `false` would
  stamp the board `Failed{BadRequest}` (fact 6) — wrong for a silent
  reconcile. Echo-arm ops are already completed by §1.
- **DEC-7 — the delete primitive**: two new `LocalOp` variants
  carrying the local board (attribution) + the remote ref (target).
  D-5c's "the existing delete path" is task-scoped and cannot express
  an unbound ref (fact 7); the variants are additive local-cache
  serde (no migration), the only outbox payload change in the chunk.
  The design's D-6 table undercounted the sync crate's surface
  (executor arms + planner arms); the actor's cycle itself remains
  untouched — translator duty intact.
- **DEC-8 — clause 4's field set**: description, duedate, done,
  archived, always; scheduled iff `card_startdate`; title via clause
  1; **order and labels excluded** (order differs by server
  construction between the two POSTs; labels ride a post-create
  sub-endpoint whose resolution timing varies). D-5b's four-field
  list read as "the pushed shape" — scheduled/archived are create-
  body fields of the same class; excluding labels trades a hair of
  false-positive surface for robustness against resolution deferral
  false-negatives.
- **DEC-9 — clause 2 stays one-cycle** (normative): the signature is
  the pending op or this cycle's echo; nothing else. Residue: a pull
  failure in the echo cycle (the echo still binds via
  `apply_push_report`'s evidence path) leaves the next pull unable to
  fire — the orphan then R1-adopts as today. Widening the signature
  (e.g. "recently bound") was rejected: it breaks §5's
  "the crash signature cannot exist without our own create" guarantee.
  Recorded in the ADR 0007 note.
- **DEC-10 — multi-orphan generalization**: N candidates matching one
  task collapse to one survivor (oldest; tie ⇒ bound) with N−1 (+the
  bound card, when doomed) delete pushes. Falls out of group
  formation; pinned by T-6.
- **DEC-11 — the W2 window**: 10 minutes, anchored on the
  just-created board's `last_modified`, plus color equality. No
  `created_at` exists (DEC-1) and no local state records the crashed
  attempt (the ceremony is stateless), so the window trades the
  human-scale retry gap against deleting a user's coincidental
  same-title+same-color board. Outside-window orphans keep the ADR
  0010 runbook floor.
- **DEC-12 — W2 rebind mechanics**: keep-older per D-5e; the rebind
  is `UnbindBoard` → `BindBoard` through the engine (typed refusals,
  the ceremony's existing error surface), placed before the
  nudge/wait so the ceremony's single wait targets the final binding;
  the receipt names the survivor. A sweep failure never fails the
  publish (warn + continue).

## 8. Risks

- **False positive deletes a stranger's twin** (design §5): the four-
  clause conjunction plus content equality is deliberately strict;
  the crash signature cannot exist without our own create in the
  window. Residual: a genuinely-twin same-minute card misread as
  ours — assessed acceptable by the accepted design; the near-miss
  negatives (T-5) are the regression fence.
- **The stack inversion is a deliberate design deviation** (DEC-3):
  stage-3 review must check the deviation against the *reason* (Deck
  stack delete cascades cards; the orphan is provably empty), not the
  letter of D-5d.
- **Ref-delete ops in the wild**: a collapse followed by a board
  unbind before the delete drains leaves the doomed card on the
  server (the unbound-board wait behavior). Benign pollution,
  documented; the next bind cycle drains it.
- **Journey brittleness**: the two-POST sequence needs ordered wiremock
  mocks (`up_to_n_times(1)`) and `createdAt` on sim-minted cards —
  both called out in §3-R6; a missing `createdAt` fails *safe* (no
  detection), so the journey genuinely pins the mapping.
- **Skew residue**: a server clock ahead by > 60s defeats clause 3
  (degrade-to-today); a local clock ahead by > 60s can exclude a true
  duplicate the same way. Same class as #133's documented skew
  exposure; not widened (the design's tolerance choice, DEC-4).

## 9. Estimated diff size

| Area | Files | Lines (incl. tests) |
|---|---|---|
| `created_at` mapping | `remote.rs`, `mapping.rs`, `test_support` | ~60 |
| outbox variants | `outbox.rs`, storage `codec.rs` tests | ~80 |
| delete primitive | `push.rs`, `push_exec/execute.rs`, `push_exec/tests.rs` | ~220 |
| predicate + survivorship | `merge.rs` (+ `RULE_R_DUP`) | ~260 |
| pipeline composition | `pipeline.rs` | ~300 |
| journeys | `e2e_hermetic.rs`, `actor_wiremock.rs` | ~230 |
| W2 sweep | `commands/boards.rs` + app test | ~200 |
| ADR notes | ADR 0007, ADR 0010 | ~30 |
| **Total** | | **~1300–1500** (production ~500–550) |

Size M, matching the dispatch table. No migration, no snapshot
regeneration, no new crate dependency.

## 10. References

- Design: `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
  §3 D-5, §2 facts 9/12–13/16, §4 F-150/F-150b, §5.
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1, §9.2.
- ADRs 0004 (untouched this chunk), 0007 (dated note), 0010 (W2
  sentence), 0005 (n/a — no migration).
- Audit: W1/W2 window sizing
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-sync-persistence-report.md`);
  transport characterization
  (`.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`).
- Issues: #150 (origin), #484 (implementation) — stage 3 closes both.
