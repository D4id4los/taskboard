# Undo architecture design round (D4) — daily-driver wave, stage-0 question 5

- **Date:** 2026-10-19 (design round; no implementation in this wave)
- **Agent:** E (chunk 5 of 5, sequential round; chunks 1–4 verified on the
  branch at `2b1a657`…`9377ea6` before this doc was written)
- **Issues:** #349 (this round), #176 (the implementation, left
  unscheduled for a later wave)
- **Inputs:** future-directions analysis report §4 D4 + §3.7 (undo row) +
  §7 sync-rule sidebar; daily-driver wave design plan §5 item 5; ADR 0006
  (interpret loop), ADR 0007 (push-first cycle, evidence-then-verdict,
  self-clobber guard); v0.1.0 audit arch-fit report F16-12/F16-13/F16-14
  (+ F7-06, E-02, E-15, E-18 as cited there); the multi-board world's
  DF-1…DF-10; sibling chunks' docs (sidecar, field family, reminders
  surfaces, views & finding).
- **Deliverable shape:** one recommendation per question (§5 is the
  decision record); the pushed-edit rule is stated as a hard rule (§3.2).

## 1. Mission & binding constraints

Decide the undo architecture before any implementation wave: journal +
inverse synthesis vs snapshots (D4 core), the pushed-edit rule as a hard
rule, the sync interplay (validators, `remote_seen`, the self-clobber
guard), and the v1 scope with a cost estimate. The wave plan's framing
(wave plan §5 item 5): "command journal + inverse synthesis vs state
snapshots; interaction with sync (the pushed-edit rule: undo after push
is a **new edit**, never a pretended reversion); the undo-tree scope;
what a v1 implementation would cost. Design only."

Binding constraints, all verified in code this round:

1. **The single-advance rule is the journal.** Memory advances only by
   interpreting exactly the persisted batch (`EngineCore::interpret`,
   `crates/taskboard-state/src/core.rs:37-45`; ADR 0006); the
   persistence action sequence is therefore already a faithful journal —
   audit F16-12 located this ("the action sequence is already a faithful
   journal, but batches are anonymous"). Undo does not need to *add* a
   journal; it needs to give the existing one **identity and
   invertibility**.
2. **Everything mutates through `plan_command`.** The engine's only
   mutating path is `execute_command` → `plan_command` (pure, total,
   clock/id-injected; `crates/taskboard-domain/src/command.rs:240`) →
   `repo.apply` → `interpret` → publish (`crates/taskboard-state/src/actor.rs:513-589`).
   An undo mechanism that bypasses this path would fork the mutation
   semantics catalogue — rejected by construction.
3. **Merge is LWW over per-field write clocks; observation stamps are
   monotone.** Task fields carry per-field clocks
   (`TaskClocks`, `crates/taskboard-domain/src/entities.rs:49-70`);
   `remote_seen` is the remote-observation stamp (`entities.rs:191`);
   the self-clobber guard restores *values and write clocks* while
   observation stamps "still advance" (ADR 0007, self-clobber section;
   `crates/taskboard-domain/src/pipeline.rs:83-110`). Any undo design
   must be clock-honest or the next pull silently overturns it.
4. **Sync is push-first, evidence-then-verdict, no remote idempotency**
   (ADR 0007). Outbox ops complete on push (`PersistenceAction::CompleteOp`),
   cancel/dead-letter on `FailOp`; Deck has no write idempotency keys.
5. **The multi-board world is accepted context** (wave plan §5 preamble):
   nine board-scoped commands gain `board: BoardId` (DF-1), per-board
   `SyncStates` map (DF-9), `ConnectorId` end-to-end (DF-3). The undo
   design below is stated in that vocabulary.
6. **Siblings' decisions hold**: the field family rides
   `Task`/`TaskClocks`/`TaskChanges` under the #244 lockstep manifest
   (field-family doc D-F rulings); the reminder surface retires
   `SetReminderRule` in favour of `AddReminderRule`/`RemoveReminderRule`
   (reminders doc D-R1); quick capture is `CreateTask` into the
   designated global-inbox board (views doc D-V5 v2).
7. **No quality ceilings weakened, no text assertions** (AGENTS §5) —
   the test plan in §6 follows.

## 2. Verified baseline (symbols are the stable keys; lines are this round's)

| Fact | Symbol / file | Plan-day anchor |
|---|---|---|
| Sequential command loop; one message at a time; no report can interleave mid-command | `run_loop` biased `select!` | `taskboard-state/src/actor.rs:359-488` |
| Plan → apply → interpret → publish; storage failure needs no rollback | `execute_command` | `actor.rs:513-589` |
| Memory ≡ disk; the batch is the only advance | `EngineCore::interpret` | `taskboard-state/src/core.rs:37-45` |
| Command surface (26 arms today; multi-board widens nine) | `StateCommand` | `taskboard-domain/src/messages.rs:20-204` |
| Pure planner; outbox feeds only `FailOp` derivation; terminal repeats are accepted no-ops | `plan_command` + module doc | `command.rs:221-260` |
| Plan receipt: batch + typed outcome (fresh ids in outcomes) | `CommandPlan`, `CommandOutcome` | `command.rs:44-50, 96-129` |
| Persistence actions are **result-shaped**: whole-entity upserts (post-images, clocks inside), outbox transitions, validators, status, reminder facts | `PersistenceAction` | `taskboard-domain/src/persistence.rs:184-232` |
| In-memory apply semantics; re-enqueue replaces in place | `apply_actions` | `persistence.rs:242-320` |
| Outbox ops per entity; `queued_at` participates in the self-clobber guard | `LocalOp`, `PendingOp` | `taskboard-domain/src/outbox.rs:22-64` |
| Self-clobber guard: ops queued after `read_at` protect their fields from the pull's LWW | `PendingTaskIntent`, `pending_task_intents` | `pipeline.rs:83-135` |
| Merge entry points a journaled command's push evidence flows through | `apply_sync_report`, `apply_push_report` | `pipeline.rs:376, 819` |
| Resurrect is a merge rule, not a command: bound tombstone resurrects iff remote write strictly newer | R5 in `merge_task` | `taskboard-domain/src/merge.rs:227-234` |
| Push planner is pure, coalesces per entity, deletes subsume; executor resolves bindings via overlay | `domain::push::plan_pushes` | ADR 0007 "Coalescing" section |
| UI undo rows are reserved but disabled — no undo unit exists to route | `ActionId::AppUndo/AppRedo` → `DispatchOutcome::Disabled` | `taskboard-ui-slint/src/actions.rs:221-231`, `router.rs:156-162` |
| Repaint signal is `Copy`-only — a rich undo-changed payload there is its own widening | `EngineSignal` | `messages.rs:263-268` |
| Repository port is `load`/`apply` only — no restore action, no snapshot store | `TaskRepository` | `persistence.rs:36-54` |
| Storage is entity tables + outbox + sync metadata + reminder facts; migrations 0001–0005 on this branch (0006 allocated to the in-flight multi-board wave) | `crates/taskboard-storage-sqlite/migrations/` | `0001_init.sql` … `0005_board_edit_verbs.sql` |
| Receipts die with their oneshots; no end-to-end command correlation | F16-13 (evidence E-02, F7-06) | audit arch-fit report §5.5 |
| Undo-as-new-edit already natural: fresh stamps by construction, resurrect through normal rules, pure total planner | F16-14 | audit arch-fit report §5.5 |

## 3. The design, point by point

### 3.1 D-U1 — Engine command journal + inverse-command synthesis (not state snapshots)

**Recommendation: journal the *intent + pre-images* of every accepted
mutating command, and implement undo as inverse-command synthesis — a
freshly planned `StateCommand` (or a `Restore*` command where no inverse
verb exists) run through `plan_command` like any user edit. State
snapshots are rejected.**

The audit (F16-12) names three branches. Evaluated honestly:

**(a) Batch identity on the action log alone** (a group id or boundary
marker on `PersistenceAction`s). True but insufficient:
`PersistenceAction`s are *results* — `UpsertTask(Task)` carries the
whole post-write entity (`persistence.rs:184-232`), so the pre-image of
an overwrite exists only in the state *at plan time*. Post-hoc inversion
from the log alone can replay forward but cannot invert; the pre-image
must be captured when the command plans, or not at all. A batch id
without pre-images journals *what happened* but not *how to undo it*.

**(b) Snapshots.** The steelman: no synthesis table, restore is
mechanical, and the repository port already batches writes. Rejected on
four grounds, the first decisive:

1. **Snapshots fight the merge architecture.** Restore would write back
   old field values carrying *old write clocks* — and per-field LWW
   (`merge_task`, R4/R5/R7/R8) lets the next pull's remote stamps beat
   them: the undo silently un-happens at the next sync. Making an undo
   stick requires stamping *fresh* write clocks on restored values — but
   that is exactly "undo = a new edit," at which point the snapshot is
   merely a fat pre-image with worse granularity (whole state, no
   command identity, no tree structure) than a journal entry.
2. **The undo tree is a causal structure; snapshots have none.** A tree
   node needs "which command, undone along which line, producing which
   branch" — a snapshot is a point state that discards exactly this
   history. The report's "log the undo tree for debugging" perk
   (§4 D4) falls out of a journal for free and out of snapshots not at
   all.
3. **Storage shape.** One journal row per mutation (bounded, §3.5)
   versus one full-state clone per undo point on the kiosk's modest
   disk (AGENTS §1). The repository port would also need a new restore
   action (E-18 is `load`/`apply` only) — a port widening that exists
   only to serve the rejected branch.
4. **F16-13's correlation problem is unsolved by snapshots.** The UI
   still cannot label the undo unit ("Undo: rename task X") — a
   snapshot names no command. The journal entry is the label.

**(c) Intent journal (the chosen branch, completing (a)):** record, at
plan time, in the *same persistence batch and sqlite transaction* as the
command's effects, one journal entry per accepted mutating command. The
entry is the tree node:

```rust
// domain: journal.rs (new module, pure data)
pub struct JournalEntry {
    pub id: JournalId,              // UUIDv7 — time-ordered, the tree reads chronologically
    pub parent: Option<JournalId>,  // the line head when this command planned (None = root)
    pub command: StateCommand,      // the intent, verbatim (the undo label)
    pub facts: UndoFacts,           // plan-time capture for inversion
    pub ops: Vec<OpId>,             // the batch's enqueued op ids (push-boundary tracking)
    pub pushed: bool,               // flipped by the pipeline when any op completes (below)
}

pub enum UndoFacts {
    /// Whole-entity pre-images of every entity the batch overwrote,
    /// captured before `apply`. Upserts are whole-entity, so whole-entity
    /// pre-images are the matching granularity.
    PreImages(Vec<PreImage>),       // PreImage: enum Board/Stack/Task/Label/… (entity, id, value)
    /// Accepted mutation with no v1 inverse (ceremony verbs, reminder
    /// surface, §3.5): journaled for the tree/debug log, typed as not
    /// undoable.
    NotUndoable(NotUndoableReason),
}
```

Mechanics, each pinned to an existing rule:

- The journal lands **inside the single-advance rule**: a new
  `PersistenceAction::RecordJournal(JournalEntry)` (and
  `MarkJournalPushed(JournalId)` for the flag) rides the command's own
  batch, so journal ≡ state ≡ disk by the same construction that makes
  memory ≡ disk today (`core.rs:37-45`). No second write stream to keep
  consistent — the audit's objection to a separate intent stream
  ("a second stream that must stay consistent with the action stream at
  every apply", F16-12) is answered by making the journal *part of* the
  action stream.
- `PersistedState` gains `journal: Vec<JournalEntry>` (bounded, tail =
  newest; pruning §3.5); `apply_actions` handles the two new arms;
  `EngineCore` needs no new advance path.
- **Push-boundary flag:** the pipeline emits
  `MarkJournalPushed(entry_id)` in the same batch whenever it emits a
  `CompleteOp(op_id)` whose op belongs to a journal entry (the engine
  maps `op_id → entry` through the journal's `ops` — `CompleteOp`
  already addresses op ids, `pipeline.rs` push-outcome phase). One flag
  flip per completed-op-carrying entry, crash-consistent with the
  completion itself. This is what lets undo *know* — not guess —
  whether the pushed-edit rule applies (§3.2).
- **Synthesis level: `StateCommand`, never `PersistenceAction`.** The
  inverse is planned through `plan_command`, inheriting for free: typed
  rejections (undo target edited meanwhile → typed
  not-invertible-anymore outcome, not a corrupting force-apply),
  accepted-no-op semantics, the bound/unbound delete split, cascade
  behaviour, outbox op enqueueing, and the nudge. A synthesis that
  emitted raw actions would fork the catalogue — rejected (constraint 2).

**The inverse table** (the planner's synthesis catalogue; exhaustive
`match` with no wildcard arm, so a future `StateCommand` variant is a
compile error until classified — the #244 lockstep pattern applied to
undo). Against the current + scheduled surface:

| Command (post-multi-board shape per DF-1) | Inverse class | v1 undo? |
|---|---|---|
| `UpdateTask` (incl. field-family fields riding `TaskChanges`, chunk 2) | invertible: `UpdateTask` with pre-image `TaskChanges` (double-option `duedate` round-trips; `messages.rs:208-225`) | yes |
| `SetTaskDone` | invertible: `SetTaskDone` with pre-image flag (the done *timestamp* is not restored — undo-of-done reopens or re-completes at a fresh stamp; honest) | yes |
| `MoveTask` | invertible: `MoveTask` to pre-image `(stack, order)` | yes |
| `CreateTask` (incl. quick capture into the inbox board, D-V5 v2) | inverse: `DeleteTask` — unbound ⇒ cancels its pending create op (true retraction, the outbox is the natural seam, #176's own observation); bound ⇒ queues delete push (new edit) | yes |
| `DeleteTask` | **no inverse verb exists** — `UpdateTask` on a tombstone is `CommandError::TaskDeleted` (`command.rs:60-62`); resurrection today exists only as the remote-merge rule R5 (`merge.rs:227-234`). Needs a new `RestoreTask { id }` command (below) | yes, via `RestoreTask` |
| `AssignLabel` / `UnassignLabel` | invertible: each other | yes |
| `CreateLabel` / `UpdateLabel` | `DeleteLabel` / `UpdateLabel` with pre-image `LabelChanges` | yes |
| `RenameStack` / `MoveStack` | invertible: same verb with pre-image title/order | yes |
| `CreateStack` | inverse: `DeleteStack` — but the cascade tombstones tasks created *after* the journal entry; v1 refuses with a typed `UndoTargetHasDependents`-class outcome rather than silently destroying them (refuse-and-name, §3.5) | partial: refuse-if-occupied |
| `DeleteStack` (cascades N tasks, `command.rs:23-25`) | restore-class: `RestoreStack` + N `RestoreTask`-equivalents from pre-images; one journal entry already holds all cascaded pre-images (the batch tombstoned them all) | deferred past v1 (§3.5) |
| `DeleteTask`-family delete of an *unbound* task | the planner already cancels ops instead of enqueueing deletes (`command.rs:19-22`); undo = `RestoreTask`, no op at all | via `RestoreTask` |
| `CreateBoard`, `BindBoard`/`UnbindBoard`, `RenameBoard`, `SetBoardColor`, `SetBoardArchived`, `DeleteBoard`, `ResetBoard`, `ReconcileByTitle`, `ClearBoardContent`-class | ceremony/catastrophic verbs keep their typed `ConfirmationRequired`/pre-count ceremonies (DF-5) — that is their undo story; board delete/reset pre-images are whole-board and would re-need snapshot-class storage | no — typed `NotUndoable` |
| `AddReminderRule` / `RemoveReminderRule` / `ClearReminderRules` (D-R1), `SnoozeReminder`, `SetTaskRemindersMuted` | reminder surface v1: rules are cheap to re-add by hand; snoozes are per-occurrence ephemera; deliveries are proof-of-work facts that undo must *never* retract (a delivered notification happened) | no — typed `NotUndoable` |
| `RequestSync` | not a mutation; never journaled | n/a |

**`RestoreTask { id }`** (the one new verb v1 needs): planner semantics
mirror the delete path's bound/unbound split in reverse — (i) a pending
`DeleteTask` op for the task still sits in the outbox ⇒ cancel it
(`FailOp`) and write the resurrection locally, **no new op** (the
server never saw the delete; pure retraction); (ii) the delete pushed
(ops gone, entry flagged pushed) ⇒ resurrect locally with fresh clocks
and enqueue a create-class op (the remote card is gone — Deck undelete
is re-POST, so the restored task re-binds to a fresh remote card id;
identity churn is the honest price, §3.2); (iii) unbound task never
pushed ⇒ resurrect locally, no op. The resurrect write itself is
`deleted = false` + fresh `clocks.deleted` — the same field shape R5
merges (`entities.rs` `deleted` is an ordinary clocked field, F16-14).

### 3.2 D-U2 — The pushed-edit rule (hard rule)

> **The pushed-edit rule (hard, not a default):** an undo (or redo)
> whose target command's outbox ops have crossed a push boundary — any
> of the entry's ops completed into a sync cycle — is a **new edit**:
> planned fresh, stamped with the undo-time clock, enqueueing its own
> outbox ops. It is **never** a retraction or amendment of the pushed
> op, **never** a remote take-back, and **never** a rewind of
> `remote_seen`, validators, write clocks, or any other recorded fact
> about what the server has seen. Before the push boundary, undo may
> *retract*: cancel the still-pending op (`FailOp`) and restore the
> pre-image locally — the server never knew.

Why this is the only honest rule, from the verified baseline:

- **Deck has no take-back.** No write idempotency keys, no per-card
  delete timestamps, whole-entity `lastModified` stamps (ADR 0007
  context). A "reversion" would have to be a PUT or POST or DELETE —
  i.e. an edit — anyway; pretending otherwise would only mislabel the
  outbox. The backlog entry (#176) and the report's §7 sidebar say the
  same: "do not let the undo tree imply transactional sync it cannot
  have."
- **Redo of an already-pushed edit is a new edit remote-side** (#176,
  verbatim) — symmetric with undo by the same argument.
- **The clock argument forces the same shape even offline.** An undo
  that restored old write clocks would be overturned by the next pull
  (per-field LWW, §3.1 ground 1); an undo that stamps fresh clocks *is*
  a new edit by the domain's own definition (every mutation is a fresh
  stamp, F16-14). Uniformity wins: **undo always restores pre-image
  *values* with *fresh* write clocks** — pre-push and post-push alike —
  and differs only in op handling (retract vs enqueue). One rule, two
  op behaviours.
- **The retraction half is real and valuable.** Offline (or mid-cycle)
  undo of an unpushed edit cancels an op the server never saw — the
  outbox is the natural seam (#176: "undo before a sync push could
  instead retract the queued op"). The `CreateTask`-unbound and
  `RestoreTask`-pending-delete rows of §3.1's table implement exactly
  this through the planner's existing bound/unbound vocabulary
  (`command.rs:19-22`).
- **Undo of undo (redo) is journaled symmetrically:** the undo command
  is itself a journal node (its `UndoFacts` pre-image the undo's own
  restore writes), so `Redo` plans against a recorded node like any
  other inversion, under the identical pushed-edit rule — a redo after
  its undo pushed is itself a new edit.

### 3.3 D-U3 — Interaction with sync validators, `remote_seen`, and the self-clobber guard

**Recommendation: undo touches *none* of the remote-observation
machinery. Validators and `remote_seen` are monotone observations; undo
is a write. The existing self-clobber guard and the sequential loop
already cover every race an undo can enter — no new sync-side
mechanism is needed.**

Point by point:

- **`remote_seen`: never rewound.** It records "what the remote looked
  like when last observed" (`entities.rs:191`; fast-path guard for pull
  rules). The self-clobber guard already establishes the principle —
  it restores field values and write clocks but lets observation stamps
  advance (ADR 0007). Undo is the mirror image (restore values, fresh
  write clocks) and adopts the same discipline: rewinding `remote_seen`
  would be an un-observation. Mechanically it would also re-open merges
  against unchanged remotes (the fast path misses, `remote_seen` older
  than the listing stamp) — wasted cycles that converge back to the
  same verdicts, since the undo's fresh clocks win the LWW comparisons.
- **Validators: untouched.** They mirror remote endpoint state
  (`ETag`/`Last-Modified`, `persistence.rs:175-181`) and exist so polls
  survive restarts. They say nothing about local edits; an undo op rides
  the outbox and the *next* cycle's pushes/validators handle it like any
  edit. A pushed undo changes the remote, so the post-push snapshot's
  fresh validators land through the normal report path.
- **The self-clobber guard covers undo for free.** An undo planned
  while a cycle is in flight enqueues ops stamped after the cycle's
  `read_at`; `pending_task_intents(outbox, read_at)` therefore protects
  the undone fields from that cycle's pull exactly as it protects any
  post-read edit (`pipeline.rs:83-135`; ADR 0007 self-clobber section).
  No guard widening, no `Undo` special case. (The sidecar chunk's
  D-S6 extends the same guard to document-embedded clocks — same
  principle, disjoint clock domain; undo of sidecar-carried fields
  inherits protection the same way once those fields exist.)
- **The sequential loop closes the retract-vs-push race.** The danger:
  undo plans "retract: cancel op X" while an in-flight cycle is about
  to complete X. Cannot happen: the engine processes one message at a
  time (`run_loop` biased `select!`, `actor.rs:359-488`) — the undo's
  plan+apply+interpret run inside one `Execute` arm, and
  `ingest_sync_report` runs in its own arm, so the outbox the undo
  plans against is always the settled post-report state. The
  `MarkJournalPushed` flag (§3.1) is therefore exact at plan time: ops
  still pending ⇒ retractable; completed ⇒ new-edit class. The
  audit's "receipts die with oneshots" correlation gap (F16-13) is
  closed by the *journal* being the durable correlation — not by
  widening any oneshot.
- **Cascade honesty on the remote side.** The push planner coalesces
  per entity and lets deletes subsume (ADR 0007 "Coalescing"); an undo
  that cancels a pending delete (retraction class) simply removes the
  op — the planner never sees it. An undo that must *push* a restore
  (new-edit class) enqueues create/update ops the planner already
  knows how to coalesce. No planner changes.
- **Crash windows stay benign.** A crash between the undo's batch
  pieces is impossible (one sqlite transaction, ADR 0005 / port
  contract "land together or fail together"); a crash between a push
  and its report lands through the existing evidence-then-verdict
  paths (`apply_push_report`), and the journal flag flips in the same
  batch as the `CompleteOp` it annotates — a reboot can never observe
  the flag without the completion or vice versa.

### 3.4 D-U4 — The undo tree: journal the tree, expose a line (v1)

**Recommendation: the journal *is* the undo tree (parent pointers from
day one, every accepted mutation a node); the v1 command surface is a
linear undo/redo line over it. Branch navigation UI is deferred.**

- #176's open question ("undo stack vs undo tree — decide when planning
  the feature; record in an ADR") is answered here: **the stored
  structure is a tree; the exposed discipline is a line.** The tree
  costs one `parent` field per entry and gives: (a) correct branch
  bookkeeping when a new edit follows undos (the classic branch point —
  `parent` = the line head, the abandoned redo branch stays queryable);
  (b) the debugging log the report wants for free (§4 D4: "which also
  gives the 'log the undo tree for debugging' idea for free"); (c) a
  v2 branch-picker UI that is additive data work, not a migration.
- **v1 commands:** `StateCommand::Undo` and `StateCommand::Redo` —
  engine-level, board-agnostic arms that plan against the journal
  (caller supplies nothing; the engine's current line head is engine
  state — deliberately *not* caller-owned, unlike DF-2 selection,
  because the undo line is a property of the mutation history, not of
  any view). Outcomes: `CommandOutcome::Undone { entry }` /
  `Redone { entry }` (the label the UI toasts), typed refusals
  (`NothingToUndo`, `TargetNotUndoable(reason)`,
  `TargetNoLongerInvertible(..)`).
- **The line rule:** undo walks `parent` once per invocation; a new
  accepted mutation after undos journals with `parent` = current head
  (a fresh branch — the tree grows; the abandoned branch's redo path is
  reached only by undoing back to the fork, the discipline every
  desktop editor trains). `Redo` re-plans the recorded intent through
  `plan_command` / `RestoreTask` (fresh ids where the retraction class
  lost the entity — honest identity churn, §3.2).
- **UI read path (F16-13's seam, resolved):** `AppState` gains a
  bounded projection `undo: UndoView` (can-undo / can-redo, the line
  head's label, a small recent-labels window — the full journal stays
  persisted-only; kiosk memory stays modest). The reserved
  `AppUndo`/`AppRedo` rows (`actions.rs:221-231`, currently
  `DispatchOutcome::Disabled`) flip to `FlowOutcome::Command(Undo/Redo)`
  — the correlation the audit said "nothing provides end-to-end"
  becomes: journal entry (durable id) → `CommandOutcome::Undone` (the
  receipt) → `UndoView` (the projection). `EngineSignal` stays
  `Copy`-only (`StateUpdated` already covers the repaint; no payload
  widening needed — E-15 dodged).
- **Board scoping:** the line is engine-global in v1; entries carry
  their board (derivable from touched entities, stored anyway) so a
  per-board filtered view is additive later. Rationale: undo as a
  global reflex (Ctrl+Z) must work regardless of which board the user
  is viewing; surprising would be an undo that *skips* the visible
  board's edit to reach a same-board one. Recorded as an open point
  with this recommendation; the user may overrule at the decision
  session.

### 3.5 D-U5 — v1 scope and cost (for the later implementation wave, #176)

**Recommendation: the v1 below, in two implementation chunks, house
scale L overall (domain M + storage S + engine S + CLI S + UI M +
tests S). The report §3.7's "L core + M view" splits the same way.**

**In scope (v1):**

1. **Journal substrate:** `JournalEntry`/`UndoFacts` domain types;
   `PersistenceAction::RecordJournal` + `MarkJournalPushed`;
   `PersistedState.journal`; sqlite migration (next free number at
   landing time — 0006 is the multi-board wave's) with a
   `command_journal` table keyed by `JournalId`, parent column, JSON
   payload columns; codec in the storage crate's existing pattern.
2. **Inverse synthesis catalogue** (§3.1's table, exhaustive match, no
   wildcard): the invertible family — `UpdateTask` (field-family fields
   included via `TaskChanges`), `SetTaskDone`, `MoveTask`, `CreateTask`
   (both retraction and delete-push classes), `AssignLabel`/
   `UnassignLabel`, `CreateLabel`/`UpdateLabel`, `RenameStack`/
   `MoveStack`; `CreateStack` with refuse-if-occupied.
3. **`RestoreTask`** with the three-case planner semantics (§3.1).
4. **`StateCommand::Undo`/`Redo`, `CommandOutcome::Undone`/`Redone`,
   typed undo refusals; `plan_undo` pure function in the command
   module's spirit (pure over `AppState` + outbox + journal,
   clock/id-injected).**
5. **`AppState.undo: UndoView`** projection + the one deliberate insta
   regeneration pass it costs (DF-9 snapshot rule: the widening, nothing
   else).
6. **Surfaces:** CLI verbs `tasks undo`/`tasks redo` (REPL inherits);
   UI wiring of the reserved `AppUndo`/`AppRedo` rows + toast labels
   from `UndoView`.
7. **Retention:** bounded journal — oldest-first leaf-pruning at a cap
   (default 1000 entries) and an age floor (default 90 days), defaults
   only in v1 (a `history.*` config key later, if asked). A pruned
   line head simply reports `NothingToUndo` — graceful degradation,
   no correctness cliff.
8. **The ADR** (#176 asks for one): ADR lands with the implementation
   chunk, recording this doc's D-U1…D-U4 as the decision.

**Out of scope (v1), each typed `NotUndoable` with its reason in the
outcome (no silent gaps) — and, per the 2026-10-19 decision session,
each deferral captured as a backlog issue so it is not lost:**
`DeleteStack`/`RestoreStack` (#356 — multi-entity restore machinery;
the journal already holds the pre-images, so v2 is additive),
`DeleteBoard`/`ResetBoard`/`ClearBoardContent`/bind & reconcile
ceremonies (#357 — their `ConfirmationRequired`/pre-count ceremonies
*are* the defense, DF-5), the reminder surface (#354 — D-R1 verbs,
snooze, mute; deliveries are proof facts that must never be retracted),
board-level `Undo`-of-`Undo` special cases beyond the symmetric rule
(**no issue, recorded as a non-item**: the symmetric rule of §3.2 is
the complete answer — there is no known work under this bullet; if a
real special case surfaces it arrives with its own evidence).

**Cost, on the house scale:**

| Slice | Contents | Size |
|---|---|---|
| Domain | journal types, both actions, `apply_actions` arms, synthesis catalogue, `plan_undo`, `RestoreTask`, `Undo`/`Redo` arms, `UndoView` | **M** |
| Storage | migration, `command_journal` table, codec, retention pruning | **S** |
| State engine | journal rides existing envelopes (no new `EngineEnvelope` variant — `Undo` is a `StateCommand`); interpret arms; nudge path unchanged | **S** |
| CLI/REPL | two verbs + receipts | **S** |
| UI | enable reserved rows, toast, `UndoView` binding | **M** |
| Tests | corpus pass (one deliberate insta regeneration), proptest inverse round-trip, lockstep-style exhaustive-arm test | **S** |
| **Total** | two chunks: (1) domain+storage+engine+tests, (2) surfaces | **L** |

**Why not smaller:** the synthesis catalogue and `RestoreTask` are the
irreducible core — without them the journal is a log, not an undo
system. **Why not larger:** snapshots, branch-picker UI, reminder
surface undo, and per-board line scoping are all deliberately cut;
each has a recorded additive path (v2).

## 4. Coherence map

- **ADR 0006 (interpret loop):** the journal extends the single-advance
  rule rather than parallel to it — `RecordJournal` rides the command's
  own batch; no second stream (answers F16-12's stated risk of branch
  (b) intent journals).
- **ADR 0007 (push-first cycle):** the pushed-edit rule is the
  evidence-then-verdict philosophy applied to undo — completed work is
  never un-happened, only answered with new work. `MarkJournalPushed`
  is derived from the report's own `CompleteOp`s. Validators and
  `remote_seen` untouched (monotone observations).
- **ADR 0004 (conflict policy):** no new merge semantics — F16-14's
  finding holds ("the existing machinery already treats undo edits as
  new edits"); undo feeds the same per-field LWW.
- **Multi-board DFs:** DF-1's board-scoped commands invert id-addressed
  or board-keyed exactly as they plan (no new scoping problems); DF-9's
  snapshot rule prices the `UndoView` widening (one deliberate pass);
  DF-2 caller-owned selection is *not* extended to the undo line
  (engine-owned, §3.4, with rationale); DF-5 ceremonies keep their
  confirmations (typed not-undoable here).
- **Chunk 1 (sidecar):** disjoint clock domains stay disjoint — undo
  never touches document-embedded clocks; the D-S6 guard extension
  covers post-read undo writes to sidecar fields when those land.
  The command journal is device-local by design (v1). Note the word
  collision, corrected at the decision session: the report's D5
  "journal/export roaming" line refers to the *user-facing journaling
  feature* of report §3.4 (`JournalEntry`/`ReflectionTemplate`
  entities), not to this command journal — the two share a word and
  nothing else (see §8 item 5's withdrawal).
- **Chunk 2 (field family):** the #244 lockstep manifest gains one more
  consumer — the synthesis catalogue matches `StateCommand` arms
  exhaustively, so a new field riding `TaskChanges` is undoable with
  zero catalogue work (the `UpdateTask` row covers it).
- **Chunk 3 (reminders):** D-R1's new verbs land as typed
  `NotUndoable` in v1 (recorded in the table, not forgotten);
  deliveries/snoozes/mutes are facts, not edits.
- **Chunk 4 (views):** quick capture (D-V5 v2) is `CreateTask` — undo
  of a capture is the unbound-retraction class (the inbox task never
  pushed); the capture overlay's commit hook gains an undo toast
  path via `UndoView` when wired.
- **AGENTS §1 kiosk goals:** one bounded table + one projection window;
  no per-undo state clones; no new actor, channel, or port.

## 5. Decision record — one recommendation each

| # | Question | Recommendation | Status |
|---|---|---|---|
| D-U1 | Journal + inverse synthesis vs snapshots | **Journal intents + plan-time pre-images in the command's own batch; undo = inverse-command synthesis through `plan_command`** (snapshots rejected: they fight LWW clocks, carry no causal structure, cost whole-state clones, and don't name commands — §3.1) | **ACCEPTED (2026-10-19)** |
| D-U2 | The pushed-edit rule | **Hard rule (§3.2): post-push-boundary undo/redo is a new edit — fresh plan, fresh clocks, own outbox ops; never a retraction, never a rewind of `remote_seen`/validators/clocks. Pre-boundary undo retracts (`FailOp`) — the server never knew. Undo always restores pre-image values with fresh write clocks.** | **ACCEPTED (2026-10-19)** (stated as a hard rule per the round's mandate) |
| D-U3 | Sync validators + `remote_seen` interplay | **Touch nothing observational: validators and `remote_seen` monotone; the existing self-clobber guard covers post-`read_at` undo ops for free; the sequential loop makes retract-vs-push race-free; `MarkJournalPushed` (flipped with the `CompleteOp` batch) is the exact boundary signal** (§3.3) | **ACCEPTED (2026-10-19)** |
| D-U4 | Undo-tree scope | **Journal the tree (parent pointers day one); v1 exposes a linear undo/redo line (`StateCommand::Undo`/`Redo` + `AppState.undo: UndoView`); branch-picker UI and per-board line scoping deferred with additive paths recorded** (§3.4) | **ACCEPTED (2026-10-19)** |
| D-U5 | v1 scope + cost | **§3.5's list — journal substrate, invertible family, `RestoreTask`, Undo/Redo + projection, CLI verbs, reserved-row wiring, bounded retention, the ADR — in two chunks; house scale L (domain M, UI M, rest S)**; #176 stays the implementation carrier, unscheduled until the user opens the wave | **ACCEPTED (2026-10-19)** — with the rider that the out-of-scope items are captured as backlog issues (#353–#357) so they are not lost |

Decision session (user review), 2026-10-19: D-U1…D-U4 accepted as
proposed; D-U5 accepted with the backlog-capture rider. §8 records the
per-candidate rulings; candidate 5 was withdrawn after the user's
clarification (the report's D5 "journal" is the §3.4 user journaling
feature, not this design's command journal — see §8).

## 6. Consequences for the later implementation wave (#176)

- **Chunking:** (1) domain + storage + engine + tests; (2) surfaces
  (CLI/REPL/UI). Chunk (1) is independently green (the `Undo` command
  works end-to-end through `execute`); chunk (2) is presentation.
- **Test plan (AGENTS §5 discipline):**
  - *Unit (domain, pure):* synthesis catalogue per arm — plan command
    against seeded state, capture journal entry, plan undo, assert
    pre-image values restored, write clocks fresh, `remote_seen`
    unchanged; typed refusals asserted as variants (`matches!`), never
    text.
  - *Property (proptest):* random invertible-command sequences over a
    generated `AppState`, undo all, assert field-level pre-image
    restoration; the retraction class asserts the outbox empties; the
    pushed class (flag pre-set) asserts inverse ops enqueue.
  - *Engine (snapshot, insta):* `Undo`/`Redo` join the command corpus;
    the `UndoView` widening is the one deliberate regeneration pass
    (DF-9 rule).
  - *Lockstep-style test:* the synthesis catalogue's exhaustive match
    fails compilation on any new `StateCommand` arm (the #244 pattern).
  - *No sleeps:* fixed/stepping injected clocks throughout (the
    `plan_command` pattern).
- **Migration:** one file, single transaction, next free number; no
  down-migration; chrono TEXT stamps (never `ORDER BY` text) — the
  storage conventions (0005/DF-9).
- **Risks to name at dispatch:** (a) the `MarkJournalPushed` op-mapping
  must handle multi-entry op sets (`DeleteStack` cascades) without
  double-flipping (idempotent flag write); (b) `RestoreTask`'s
  bound-and-pushed case re-POSTs (new remote identity) — the toast copy
  must not promise identity preservation; (c) the `UndoView` insta pass
  touches many snapshots — one deliberate diff, reviewed as "the
  widening, nothing else."

## 7. Assumptions borrowed from the round's docs (reconciliation list)

1. From the multi-board wave (DF-1): nine commands gain `board:
   BoardId`; this doc's inverse table is written against that shape —
   the invertible classifications are board-agnostic (id-addressed
   commands) or trivially board-keyed (board-scoped creates refuse via
   the same `live_board_of` the forward command uses).
2. From chunk 2 (D-F rulings): new scheduling fields ride
   `TaskChanges`/`TaskClocks` — inherited undoability via the
   `UpdateTask` row; if the session instead lands a *new command arm*
   per field, the lockstep test forces its classification the moment it
   is written.
3. From chunk 3 (D-R1): `SetReminderRule` retires; the table's reminder
   rows name the D-R1 verbs, not the retired one.
4. From chunk 4 (D-V5 v2): the global inbox is a designated board —
   capture is `CreateTask`, no special undo case.
5. From the sidecar doc (D-S2/D-S6): the document schema's section
   registry and clock-domain separation are untouched by undo; the
   command journal is device-local (v1). The report's D5
   "journal/export roaming" concern belongs to the §3.4 user
   journaling feature, not this command journal (corrected at the
   decision session — §8 item 5).
6. From the report (§3.7/§4 D4/§7): the tree ambition, the
  "journal-for-debugging" perk, and the pushed-edit sidebar are the
   round's mandates — all three carried into D-U1/D-U2/D-U4.

## 8. Backlog candidates (out-of-scope ideas) — rulings recorded 2026-10-19

1. Branch-picker UI over the journaled tree (v2; additive — the data is
   already there). **Accepted → #353** (`ui`, `p3`), with the ruling
   note that it **needs further consideration** before scheduling.
2. Reminder-surface undo (D-R1 verbs; deliveries stay irretractable).
   **Accepted → #354** (`architecture`, `p3`), with the ruling note
   that it **needs further consideration** before scheduling.
3. Per-board undo line scoping (engine option or caller filter;
   §3.4's open point). **Accepted → #355** (`architecture`, `p3`),
   with the ruling note that it **needs further consideration** before
   scheduling.
4. `RestoreStack` + cascaded-task restore (v2; pre-images already
   journaled). **Accepted → #356** (`architecture`, `p3`).
5. **Journal roaming/export on the D5 substrate — WITHDRAWN at the
   2026-10-19 decision session; no issue, nothing to add to the
   backlog.** The candidate rested on a misreading: the report's D5
   "journal/export roaming" line refers to the *user-facing
   journaling feature* of report §3.4 ("Review, journaling, planning,
   decision support" — `JournalEntry`/`ReflectionTemplate` entities,
   end-of-day reviews, wellbeing attributes) and to syncing **that**
   data across devices. When the report was written, no AppState-level
   command journal had been conceived of; this design's command
   journal is a different object that happens to share the word. The
   command journal stays device-local (v1) by this design's own
   choice, and whether *it* should ever roam is not an open question
   this round carries — it would need its own motivation if one ever
   appears.

## 9. References

- Future-directions analysis report:
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  — §3.7 (undo row, "L core + M view"), §4 D4, §7 (pushed-edit
  sidebar), §6 wave placement (design round in Wave 1; implementation
  Wave 3 "if not already").
- Daily-driver wave design plan:
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md` §5
  item 5 (this round's mandate), §4 D-D3 (#176 unscheduled).
- ADR 0006 (`docs/adr/0006-state-engine-interpret-loop.org`) — the
  single-advance rule this design extends.
- ADR 0007 (`docs/adr/0007-sync-actor-push-first-cycle.org`) —
  push-first cycle, evidence-then-verdict, self-clobber guard,
  coalescing planner, no idempotency keys.
- ADR 0004 (`docs/adr/0004-sync-conflict-policy.org`) — the LWW merge
  policy undo composes with (unchanged).
- v0.1.0 audit arch-fit report:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`
  §5.5 — F16-12 (batch identity, the three branches), F16-13
  (correlation seam, E-02/E-15/E-18/F7-06), F16-14 (undo-as-new-edit
  natural today).
- Multi-board wave implementation plan §2 (DF-1…DF-10):
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`.
- Sibling chunk docs: `2026-10-18-appdata-sidecar-design-plan.md`,
  `2026-10-18-field-family-design-plan.md`,
  `2026-10-19-reminders-surfaces-design-plan.md`,
  `2026-10-19-views-finding-design-plan.md`.
- Issues: #349 (this round), #176 (implementation carrier; its audit
  section carries F16-12/13/14), #244 (lockstep manifest pattern), #207
  (sidecar, for the clock-domain separation), #346–#348 (views).
- Code anchors: §2's table (symbols are the stable keys; line numbers
  are this round's, re-located from the worktree branch
  `docs/daily-driver-design-round` @ pre-commit HEAD).
