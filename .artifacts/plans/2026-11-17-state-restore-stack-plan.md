# Chunk C8 Plan — RestoreStack Foundation (#356)

Round: `2026-11-17-backlog-reduction-round-plan.md` §3-C8 (normative) ·
Dispatch: `2026-11-17-backlog-reduction-dispatch-plan.md` §1/§9 ·
Branch: `feat/state-restore-stack` · Stage: 2 implements, 3 reviews.
Family ordering: #356 (this) → #355 (C9 per-board scoping) → #354 →
#353; #357 exploratory. Engine-side only — journey-plan exempt.

## 0. Naming disambiguation (read first)

"RestoreStack" in #356 means **restore a kanban stack (column) plus the
tasks its delete cascaded** — not a LIFO container of restores. There is
**no new undo/restore data structure** in this chunk: the journal line
(`JournalState`) *is* the restore surface, already bounded by
`JOURNAL_CAP` (1000 leaf entries) and `JOURNAL_AGE_FLOOR` (90 days).
Prompt language about "stack depth/bound" resolves to: **restore
history depth = journal retention; no second container** (D-1).

## 1. Current state of the world (verified at main `2f0510e2`)

- `StateCommand::DeleteStack` cascades: tombstones the stack + every
  live task of the stack, deletes each task's reminder rules
  (`task_rule_deletes`), enqueues `LocalOp::DeleteStack` for the stack,
  and per task: `DeleteTask` op if remote-bound, `FailOp` of pending ops
  if unbound (`command.rs:1354-1395`).
- `DeleteStack` is journaled `NotUndoable(RestoreClass)`
  (`undo.rs:126-128`) — **but the journal already captures the full
  cascade pre-images** for it: `pre_images` is arm-agnostic and walks
  the plan's `Upsert*` actions (stack + every tombstoned task), so the
  classification flip is purely a synthesis-layer change. (The
  classification flip also makes `record_entry` stop taking the
  `NotUndoable` early exit — the facts become `PreImages`; the capture
  code is shared.)
- `RestoreTask` exists with the three-case planner (pending-delete
  retraction / pushed re-POST as `CreateTask` / local resurrect,
  `command.rs:3065-3108`); `undo_of_delete_resurrects_locally` and
  `undo_of_pushed_delete_reposts_as_a_create` pin it.
- `plan_undo` already (a) runs the `UndoTargetHasDependents` occupancy
  guard for the `CreateStack` inverse, (b) applies the pushed-edit rule
  blanket retraction (`retain !EnqueueOp` + fail `entry.ops`) when
  `!entry.pushed`, (c) moves the head unconditionally.
- `UndoView.recent` (`UNDO_RECENT_WINDOW = 10`) is a memory-only display
  projection; entries carry `board: Option<BoardId>` (D-U4's per-board
  tag) — the C9 seam.

## 2. Recorded decisions

### D-1 — What enters the restore surface; depth/bound
Only `StateCommand::DeleteStack` is reclassified from
`NotUndoable(RestoreClass)` to undoable (`PreImages`). `DeleteLabel`,
`DeleteTimeEntry`, `DeleteHabit` stay `NotUndoable(RestoreClass)` —
their restore verbs are separate future work (backlog issue filed).
Bound: no new container; the journal line's existing retention
governs (§0). An evicted stack-delete head degrades to
`NothingToUndo`, the documented graceful path.

### D-2 — The command surface
One new arm, `StateCommand::RestoreStack { id: StackId }` — stack-only,
mirroring `RestoreTask`'s terminal-intent shape:

- Forward verb (recovery surface): restores the **stack entity only**
  (tombstoned → live, pre-tombstone values verbatim, fresh tombstone
  clock per the R5 merge-field shape; a live target is the accepted
  no-op). Tasks tombstoned by the old cascade are *not* resurrected by
  the forward verb — they keep their own `RestoreTask` recovery path
  (each tombstone is individually addressable; the undo inverse, D-3,
  is the combined surface). Deliberate leniency, named here and in the
  doc comment.
- Forward planner three cases (verbatim `RestoreTask` semantics over
  the stack): pending `DeleteStack` op → `FailOp` + upsert (retraction);
  pushed/bound → upsert + enqueue `LocalOp::CreateStack` (identity
  churn is the honest price, D-U2 precedent); otherwise local upsert.
- Typed outcome: `CommandOutcome::StackRestored { stack: StackId }`.
  No new `CommandError` variants required: `UnknownStack` covers
  existence; a live stack is `CommandOutcome::NoOp` (terminal-repeat
  discipline, `RestoreTask` precedent).
- Implementation note: extract the per-task three-case body of the
  `RestoreTask` arm into a shared helper (`plan_task_restore`) used by
  both the `RestoreTask` arm and the undo planner (D-3) — pure
  refactor, the existing `RestoreTask` corpus must stay green.
- The lockstep matches gain arms per the compilation contract:
  `command_label` ("Restore stack"), `touched_board` (via the stack →
  board chain; tombstones stay in the map so the lookup is honest),
  `record_entry` classification (`RestoreStack` → `PreImages`: its own
  inverse is the ordinary `DeleteStack` cascade, D-5),
  `command_class` (`DeletedStack` new row; `RestoreStack` under the
  existing `CreatedTask`-style row — actually its own
  `InverseClass::RestoredStack` row: the inverse is `DeleteStack { id }`
  with the `CreatedStack`-style title+order stamps. The inverse
  **carries the same occupancy guard** as the `CreateStack` inverse
  (`UndoTargetHasDependents`): undo never silently cascades entities
  newer than the entry it inverts. Both directions refuse typed;
  composition stays honest.)

### D-3 — Undo of `DeleteStack`: composition, not a mega-verb
The `DeleteStack` journal entry's `PreImages` hold the stack (before:
`Some`) and every cascaded task (before: `Some`). The new
`InverseClass::DeletedStack` synthesis:

1. Watermark check, all-or-nothing (atomicity): the stack row exists,
   still tombstoned, `clocks.deleted ==` recorded stamp; every cascaded
   task likewise (its `deleted` clock). Any divergence →
   `TargetNoLongerInvertible { entity, field: Deleted }` — field-named,
   whole-line refuse, zero actions (no partial restore).
2. Synthesize **`RestoreStack { stack_id }` + N ×
   `RestoreTask { id, resurrect: pre_image }`** (one per cascaded
   member, pre-image payloads, not current tombstone values). The
   design plan §3.1's "RestoreStack + N RestoreTask-equivalents" shape,
   reusing the existing planners verbatim — this is why the forward
   verb is stack-only (D-2): the per-entity push-boundary decisions
   stay inside the battle-tested `RestoreTask` three-case logic.
3. Plan each through the ordinary catalogue; merge into one batch
   (deterministic order: stack first, then members by the entry's
   pre-image order = `BTreeSet` insertion order); one head move rides
   last per `plan_undo`'s existing tail.

The pushed-edit rule needs **no new code**: `plan_undo`'s blanket
retraction (drop the inverse's fresh enqueues, fail `entry.ops`) plus
the per-entity pending-op checks inside `RestoreTask`/`RestoreStack`
case 1 compose correctly, exactly as they do today for
`undo_of_create_retracts_delete_class_ops`.

### D-4 — Cascade granularity
Cascade membership = **the entry's own pre-image set** (the tasks
tombstoned by *that* delete), never "all currently tombstoned tasks in
the stack". A task individually deleted before/after the stack delete
keeps its independent tombstone and recovery path; the stack restore
does not touch it. This is mechanical: pre-images are captured at
record time from the plan's upserts; the watermark check (D-3.1)
already refuses if any member diverged.

### D-5 — Interaction with the recent-window undo line: composition
`RestoreStack` and the reclassified `DeleteStack` ride the *same*
journal line as everything else — no mutual exclusion, no side stack.
Consequences (all desired, none new):
- A `DeleteStack` head now attempts the combined restore (D-3) instead
  of refusing with `RestoreClass`.
- The restore itself is journaled (`PreImages` over the stack + member
  tasks, pre = tombstoned states); undoing a restore re-deletes the
  stack via the ordinary `DeleteStack` cascade — with the occupancy
  guard honoured as recorded in D-2. Redo re-plans the forward intent
  (`D5` greatest-child rule unchanged).
- `UndoView.recent` (the 10-row window) is display-only: stack-delete
  rows simply appear as ordinary rows. No window/widening change; the
  one deliberate insta regeneration this chunk costs is the new entry
  shapes in the S-series snapshots (DF-9 rule), nothing else.
- A new edit after undos forks the line and kills the old redo path —
  the existing fork law holds verbatim for the new arms.

### D-6 — Ceremony fences hold (the #842 erratum ruling)
The board-ceremony family (`CreateBoard`…`ReconcileByTitle`,
`SetBoardArchived` included) stays `NotUndoable(BoardCeremony)` — undo
v2 does **not** widen the fence; the r1 chunk-plan claims that said
otherwise were corrected under #842 and the correction stands. The
stage-3 trace trap: every new stack-entry site is
`DeleteStack`-class only; no board verb moves classification. Board
level restore remains #357's exploratory surface (not this chunk).
Also unchanged: reminder/tracking/habit/reflection refusals — no arm
migrates between `NotUndoableReason`s except `DeleteStack` leaving
`RestoreClass`.

### D-7 — Documented leniency: reminder rules do not resurrect
`DeleteStack` (like `DeleteTask`) cascades
`DeleteReminderRule` actions, and deletes carry no pre-image (D19), so
the restored tasks come back **without their reminder rules** — the
same leniency `RestoreTask` v1 already carries. Pinned by one
`..._currently_pins_...` test (debt, not contract); widening
pre-image capture to reminder rules is a backlog issue, not this chunk.

### D-8 — Persisted-journal compatibility
Old persisted entries with `DeleteStack` as `NotUndoable(RestoreClass)`
decode unchanged and keep refusing (typed) after upgrade — honest:
their facts were never recorded, so their inverses cannot be
synthesized. Only newly recorded stack deletes are undoable. No sqlite
migration: the journal table stores the entry payload as JSON; the new
`StateCommand` arm serializes into the existing column shape (serde
round-trip test included).

### D-9 — The C9 seam (per-board lines) is designed-in, not retrofitted
No shape change in this chunk. The seam is the existing
`JournalEntry.board: Option<BoardId>` tag:
- `DeleteStack` entries are tagged at record time from the pre-state
  (`touched_board` → stack → board), so every entry of a stack delete +
  its restore carries the same board id.
- C9 (#355) scopes lines by filtering/deriving on the tag (the design
  plan's "additive either way" ruling); C8 only guarantees the tag is
  present and correct on the new arms (`RestoreStack` included) and
  that no C8 code assumes a single global line beyond what exists
  today. Stage-2 acceptance: a test asserting the `board` tag of a
  stack-delete entry and of its restore inverse's entry.

## 3. Implementation map (stage 2)

| File | Change |
|---|---|
| `crates/taskboard-domain/src/messages.rs` | `StateCommand::RestoreStack { id }` arm (serde: appended, no field reshuffle). |
| `crates/taskboard-domain/src/command.rs` | `RestoreStack` planner arm (three cases; shared `plan_task_restore` extraction); typed outcome arm. |
| `crates/taskboard-domain/src/undo.rs` | `record_entry`: `DeleteStack` → `PreImages` arm (capture already mechanical); `RestoreStack` → `PreImages`. `InverseClass::DeletedStack` + `RestoredStack` rows in `command_class`/`synthesize_undo`; `DeleteStack` leaves the `NotUndoable` arms; occupancy guard for the `RestoredStack` inverse alongside the existing `CreatedStack` guard in `plan_undo`. `touched_board` arm. |
| `crates/taskboard-domain/src/journal.rs` | `command_label` arm only. No shape change (D-9). |
| `crates/taskboard-state/src/*` | No expected change (the engine plans through `plan_journaled_command`); confirm the occupancy-guard site compiles in `plan_undo`. |
| tests | §4 catalogue: `undo/tests.rs`, `journal/tests.rs` (lockstep table row), `crates/taskboard-state/tests/undo.rs`, `tests/snapshots.rs` (one S-scenario extension), `tests/proptest.rs` or domain proptest module. |

**C6 coordination (binding):** C8.2 starts only after C6
(`test/app-storage-hygiene`, #876 dead-param removal in
`taskboard-state/src/actor.rs`) lands; stage 2 begins with
`git fetch --prune; git rebase origin/main` on this branch (drop
nothing — this branch has no other commits) and re-runs the gate after
the rebase.

## 4. Test catalogue

Snapshots (insta, engine S-series / domain harness — extend, never
rewrite):

1. `delete_stack_journals_pre_images_and_is_undoable` — classification
   flip: entry facts are `PreImages` with stack + N task images;
   snapshot the entry shape.
2. `undo_of_delete_stack_restores_stack_and_cascade_locally` — unbound:
   stack + N tasks live again, rules still gone (D-7 pins the
   leniency), head moved.
3. `undo_of_pushed_delete_stack_reposts_as_creates` — bound: stack and
   remote members re-POST (`CreateStack`/`CreateTask` enqueues), no
   retraction (the `!entry.pushed` rule).
4. `undo_of_delete_stack_refuses_field_named_when_a_member_diverged` —
   member edited since → `TargetNoLongerInvertible { field: Deleted }`,
   state unchanged (atomicity).
5. `undo_of_delete_stack_refuses_when_a_member_was_individually_restored`
   — member's tombstone clock moved → refuse, whole-line.
6. `restore_stack_on_a_live_stack_is_the_accepted_noop` — terminal
   repeat; empty batch, nothing journaled.
7. `restore_stack_then_undo_re_deletes_through_the_cascade` —
   composition both directions; redo re-restores.
8. `a_new_edit_after_stack_restores_forks_and_the_old_redo_path_dies` —
   fork law on the new arms.
9. `every_arm_journals_entry_or_nothing` — the lockstep table gains its
   `DeleteStack`→PreImages and `RestoreStack`→PreImages rows (the
   existing test's table extension, not a rewrite).
10. `stack_delete_and_restore_entries_carry_the_board_tag` — D-9 seam
    assertion.
11. `undo_of_delete_stack_refuses_when_the_stack_row_gone` — evicted /
    externally removed stack → refuse typed (not panic).
12. Engine-level: `a_delete_stack_undo_flips_the_published_state_and_the_persisted_journal`
    (mirror of the existing engine undo test, over the new arms).
13. `restore_stack_currently_pins_the_reminder_rule_loss` — D-7 debt
    pin, named per the no-pins rule.
14. Serde round-trip: a journal entry carrying `RestoreStack` and a
    reclassified `DeleteStack` decode byte-stably (persisted-compat
    D-8).

Proptest (pure domain, generated stacks/tasks/labels):

- P1 `undo_delete_stack_restores_exactly_the_cascade`: for a generated
  stack with N tasks (mix of bound/unbound), undo either refuses with a
  typed error or leaves exactly the stack + its N members live and
  everything else byte-identical modulo tombstone clocks.
- P2 `delete_undo_round_trip_preserves_member_fields`: delete → undo
  restores every member's content fields to pre-delete values (fresh
  `deleted` clock excluded from the comparison).
- P3 `board_ceremony_commands_never_journal_pre_images`: over generated
  board verbs, facts are always `NotUndoable(BoardCeremony)` (D-6
  fence property).
- No sleep/pause anywhere: all tests are deterministic by construction
  (planner-level, injected clock seams via the existing harness).

## 5. Acceptance (stage 3 gate)

- Typed outcomes only; no text assertions; no `RwLock`/`Mutex`.
- Ceremony fence holds (D-6): diff-trace every classification change.
- C9 seam exists (D-9): board tag test green; no `JournalEntry` shape
  change in the diff.
- Snapshot corpus extended, not rewritten: existing snapshot names
  unchanged except the mechanical DF-9 widening; no `.snap` deletion.
- Existing `RestoreTask` corpus green after the `plan_task_restore`
  extraction (pure refactor).
- Gates: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
  run --workspace` (dispatcher owns full-suite/CI per the dispatch
  discipline; C8.2 defers per the #879 rule if the lock is held).

## 6. Risks

- **R-1 (occupancy-guard duality):** two guard sites now exist in
  `plan_undo` (`CreatedStack` and `RestoredStack` inverses). Drift
  risk is low (same predicate shape) but stage 3 greps both.
- **R-2 (batch merge order):** the combined restore batch must be
  deterministic (stack first, members in pre-image order); a merge
  order bug would flake the sqlite mirror equivalence. Covered by the
  S-series persisted-state snapshots.
- **R-3 (serde arm position):** the new `StateCommand` arm must be
  appended, never inserted, or persisted journals/outbox payloads
  renumber. D-8's round-trip test catches this.
- **R-4 (C6 rebase):** if C6 grows beyond `actor.rs` call sites, the
  rebase in §3 may touch shared test helpers — resolve in favour of
  landed `origin/main`, rerun the gate.
- **R-5 (rule-loss surprise):** D-7's leniency is user-visible once a
  stack delete becomes undoable; the debt pin test + backlog issue
  keep it from becoming an undocumented limitation.

## 7. Estimated diff

~700–900 lines: domain ~250 (messages/command/undo, mostly the
synthesis rows + planner), tests ~450–600 (snapshot + proptest +
engine), zero storage/engine logic expected.
