# [Plan] Undo journal chunk (undo/1) — journal substrate, inverse synthesis, RestoreTask

- **Date**: 2026-10-30
- **Kind**: stage-1 implementation plan for dispatch chunk **undo/1**
  (#509, branch `feat/undo-journal`, wave 9 of the six-rounds dispatch).
  TDD-first; no production code in this commit.
- **Origins** (normative, in precedence order):
  `.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5, accepted 2026-10-19 — binding, not re-opened);
  `.artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md`
  (§5 chunk 1 normative; §4 R-U6…R-U12 accepted 2026-10-26; §3 the
  33-arm seed table; §6 the test seed; §9 the risks);
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§2/§3.6/§12.1.
- **Branch / worktree**: `feat/undo-journal` in
  `../taskboard-WT-feat-undo-journal`, branched off `origin/main` @
  `bb850282` (2026-10-30). The local `main` checkout was 50 commits
  behind `origin/main`, so every seam below was read **at the worktree
  base**, not at the stale checkout — the branch point is the
  dispatch's "then-current `main`".
- **Status**: **READY** — dispatched by the dispatch plan §12.1; all
  three stage-1 preconditions verified (§2).

---

## 1. Mission & binding constraints

Implement undo v1 chunk 1 exactly as accepted: an engine command journal
with plan-time pre-images riding the command's own persistence batch
(D-U1); the pushed-edit hard rule (D-U2); observation machinery
untouched (D-U3); a tree-shaped journal with a linear v1 line and the
`UndoView` projection (D-U4); the v1 scope of D-U5. The chunk delivers
domain + storage + engine + tests and is independently green
end-to-end through `EngineHandle::execute`; surfaces are undo/2's.

Binding, restated and re-verified on this branch (§2):

1. **Single-advance rule**: memory advances only by interpreting the
   persisted batch (`EngineCore::interpret` → domain
   `apply_actions`; `core.rs:63-72`). The journal joins as batch
   actions — no second write stream, no restore path, no rollback.
2. **`plan_command`'s catalogue keeps its semantics** (R-U8): the
   wrapper is the seam; pre-images are captured after planning, from
   the pre-state, never inside the planner. The one sanctioned
   extension is the new `RestoreTask` verb arm plus defensive
   `Undo`/`Redo` arms — new surface, zero edits to existing arms
   (decision D6).
3. **LWW over per-field write clocks; observation stamps monotone**
   (`TaskClocks`, `remote_seen`, `remote_baseline`): undo restores
   pre-image *values* with *fresh* write clocks; never rewinds
   `remote_seen`/`remote_baseline`/validators (D-U3, R-U12).
4. **Push-first sync, no remote take-back** (ADR 0007): post-push-boundary
   undo/redo is a new edit; pre-boundary undo retracts (`FailOp`) —
   the server never knew.
5. **AGENTS §5 discipline**: typed outcomes asserted with `matches!`
   (no text assertions), fixed/stepping injected clocks (no sleeps),
   no quality ceilings, one behavior one test at the lowest layer.

## 2. Verified preconditions, base, and resource ladder

| Check | Result |
|---|---|
| Undo v1 design plan Status | **ACCEPTED** (2026-10-26) — §7 R-U6…R-U12 confirmed as recommended |
| DR-5/1 (#486) merged | PR #519, merged 2026-10-27 |
| DR-6/2 (#499) merged | PR #534, merged 2026-10-28 |
| DR-6/5 (#502) merged | PR #546, merged 2026-10-29 |
| Migration-carrying chunk in flight? | **No** — the only open PRs are three dependabot Cargo bumps (#539–#541); DR-6/6 (#503, mig-carrying, wave 10) is not dispatched yet |
| `worktree-doctor.sh` | exit 0 (244G free, floor 25G); worktree created with `git worktree add -b feat/undo-journal ../taskboard-WT-feat-undo-journal origin/main` |
| Migrations at base | `0001…0015` — **0015 is taken** (`0015_time_tracking.sql`, DR-5/2) |
| ADRs at base | head `0018` → next free **0019** ✓ matches dispatch assignment |

**Resource deviation (recorded, dispatch §2 rule 3)**: the dispatch
assigned migration **0015** to this chunk, but landing order diverged
from the wave table (DR-6/3's recur columns took 0010, shifting every
later claim by one; DR-5/2 landed as 0015). This chunk therefore takes
the **next free number at its base: 0016** (`0016_command_journal.sql`).
Consequence: DR-6/6 (#503), dispatched in wave 10 with "0016" in its
issue body, renumbers to **0017** at its rebase (mechanical, one file
rename + one stale comment in `repo.rs:989-993` that currently says
"chunk DR-6/6's migration (0016)"). **Stage 3 should note the
renumbering in the PR body** so the wave-10 agent finds it; this chunk
does not edit DR-6/6's issue.

ADR **0019** is free and matches the dispatch assignment — no deviation.

**Surface delta since the origin's 33-arm table**: `StateCommand` now
has **44 arms** — the 33 classified in the origin §3 plus the five
DR-5/1 tracking verbs (`StartTracking`, `StopTracking`, `LogTime`,
`AdjustTimeEntry`, `DeleteTimeEntry`) and the six DR-6/5 habit verbs
(`CreateHabit`, `UpdateHabit`, `MoveHabit`, `LogHabit`,
`AdjustHabitLog`, `DeleteHabit`). §3 classifies all 44 plus the three
arms this chunk adds (`Undo`, `Redo`, `RestoreTask`).

**Snapshot corpus delta**: the insta suite is now **33 dump+tail pairs
(66 `.snap` files** — scenarios s1–s10, s30–s38a-c, s41–s45a-b in
`crates/taskboard-state/tests/snapshots/`), not the origin's 19. The
"one deliberate regeneration pass" ruling is unchanged; only its
breadth grew (risk §16.3).

## 3. The classification table — every current `StateCommand` arm

Re-derived from `messages.rs` at the base (44 arms, declaration order).
This table is the seed of the synthesis catalogue; the catalogue's
exhaustive no-wildcard `match` is the lockstep (a future arm fails
compilation until it gets a row here). "Watermark" names the fields the
*inverse* writes back — exactly what the R-U7 check compares (§6,
decision D7).

| # | Arm | Class | Inverse (synthesis) | v1 undo? | Watermark scope | Reason / notes |
|---|---|---|---|---|---|---|
| 1 | `CreateTask` | create-class | `DeleteTask { id }` — planner's own bound/unbound split: unbound/pending ⇒ create-op retraction (`FailOp`); bound ⇒ delete-push (new edit) | **yes** | all content stamps of the created task (title, description, duedate, scheduled, done, position, effort, importance, urgency, recur, archived); any later edit ⇒ refuse (protects the edits the delete would destroy) | D-U1 table row 4. Created-entity stamps never include `remote`/`remote_seen`/`remote_baseline` — the post-push echo adoption must not refuse the delete-push undo |
| 2 | `CaptureTask` | create-class | same as #1 (undo deletes the captured task) | **yes** | same as #1 | D-V5 v2 landed as this verb; undo deletes only the task — a capture-minted Inbox board / Backlog stack stays (an empty landing container is harmless; deleting it would be cascade territory for no value). Recorded honest note |
| 3 | `UpdateTask` | same-verb invertible | `UpdateTask` with the pre-image `TaskChanges` (double-option fields round-trip: cleared-then-undone re-clears) | **yes** | exactly the fields the changeset stamped (each `Some` arm's clock) | Field-family fields (`recur`, effort/importance/urgency, scheduled, archived) ride `TaskChanges` — inherited undoability, zero catalogue work (origin §4 coherence note) |
| 4 | `SetTaskDone` *(enriched by DR-5/DR-6)* | same-verb invertible | `SetTaskDone { id, done: pre-flag }` — done *timestamp* not restored (fresh stamp, honest) | **yes** | the task's `done` clock **only** | The enriched row (dispatch mandate): the completion's side effects are **one-way facts the undo never reverts** — (a) the completion spawn's successor is a real task that stays; (b) closed time entries stay closed (ADR 0017: no payload clears a stamped end); (c) dismissals stay dismissed (permanent by construction); (d) the bound-suppressed `recur → None` clear stays cleared. Hence the watermark covers `done` alone — including the entry closes or recur clear would make undo refuse spuriously after an unrelated entry adjust or rule re-attach. The suppressed-clear case reopens the head with the series still detached (re-attach by hand; v2 candidate). Recorded as the row's accepted semantics (decision D9) |
| 5 | `MoveTask` | same-verb invertible | `MoveTask { stack: pre, order: pre }` | **yes** | the `position` clock (stack + order share one write clock) | unchanged from origin |
| 6 | `DeleteTask` | delete-class | **`RestoreTask { id, resurrect: Box<Task> }`** — the one new verb; three-case planner (§11) | **yes** | the `deleted` clock | Reminder rules cascaded by the delete (D-c) stay deleted — rules are cheap to re-add (the #354 ruling's own rationale); closed entries stay closed. Recorded in the row |
| 7 | `CreateStack` | create-class | `DeleteStack { id }` with **refuse-if-occupied** | **partial** | title + order stamps of the created stack | The occupancy check is *presence*, not clocks: any live task in the stack ⇒ typed refusal `UndoTargetHasDependents { stack }` (tasks created/moved in after the create carry no pre-image stamps — the watermark cannot see them). A renamed/reordered created stack ⇒ ordinary `TargetNoLongerInvertible` via the watermark |
| 8 | `RenameStack` | same-verb invertible | `RenameStack { id, new_title: pre }` | **yes** | the stack `title` clock | unchanged |
| 9 | `DeleteStack` (cascades N tasks) | restore-class | none in v1 — needs `RestoreStack` + N-task restore machinery | no — `NotUndoable(RestoreClass)` | — | **#356** (family: restore-class verbs, stack + label). The entry still journals with its pre-images (the batch tombstoned the whole cascade) — v2 is additive |
| 10 | `MoveStack` | same-verb invertible | `MoveStack { id, order: pre }` | **yes** | the stack `order` clock | unchanged |
| 11 | `CreateLabel` | create-class | `DeleteLabel { id }` — unconditional (per origin) | **yes** | title + color stamps of the created label | Assignments made after the create are not entity destruction (memberships die with the tombstone, re-assignable) — no occupancy check, unlike #7 |
| 12 | `UpdateLabel` | same-verb invertible | `UpdateLabel` with pre-image `LabelChanges` | **yes** | the changed clocks (title / color) | unchanged |
| 13 | `DeleteLabel` | restore-class | none in v1 — the label row plus its force-removed memberships would need a restore verb | no — `NotUndoable(RestoreClass)` | — | **#356**, family widened to labels at the 2026-10-26 reaffirmation |
| 14 | `AssignLabel` | pair-invertible | `UnassignLabel { task, label }` | **yes** | the `(task, label)` membership's `MembershipState::at` stamp | OR-set membership stamp is the field clock here; no `TaskClocks` field moves |
| 15 | `UnassignLabel` | pair-invertible | `AssignLabel { task, label }` | **yes** | the `(task, label)` membership stamp | the `Removed { at }` tombstone's stamp is what the check compares; re-assign by another path ⇒ refuse |
| 16 | `CreateBoard` | ceremony | — | no — `NotUndoable(BoardCeremony)` | — | **#357**; D-U5 ruled the board family out (ceremonies are the defense, DF-5) |
| 17 | `BindBoard` | ceremony | — | no — `NotUndoable(BoardCeremony)` | — | **#357**; the bind's ref-wipe forecast is its ceremony |
| 18 | `RenameBoard` | ceremony *(mechanically invertible)* | — | no — `NotUndoable(BoardCeremony)` | — | **#357** in v1; mechanically the cheapest future widening (origin §3 note) — v2 candidate recorded on #357 |
| 19 | `SetBoardColor` | ceremony *(mechanically invertible)* | — | no — `NotUndoable(BoardCeremony)` | — | same as #18 |
| 20 | `SetBoardArchived` | ceremony *(mechanically invertible)* | — | no — `NotUndoable(BoardCeremony)` | — | same as #18 |
| 21 | `DeleteBoard` | catastrophic | — | no — `NotUndoable(BoardCeremony)` | — | **#357**; whole-board pre-images are snapshot-class |
| 22 | `ResetBoard` | catastrophic | — | no — `NotUndoable(BoardCeremony)` | — | **#357** |
| 23 | `SnoozeReminder` | reminder surface | — | no — `NotUndoable(ReminderSurface)` | — | **#354**; per-occurrence ephemera, cheap to re-snooze |
| 24 | `SetTaskRemindersMuted` | reminder surface | — | no — `NotUndoable(ReminderSurface)` | — | **#354** |
| 25 | `DismissReminder` | reminder surface (settled fact) | — | no — `NotUndoable(ReminderSurface)` | — | **#354**, family widened at the reaffirmation; the dismissal-settlement model (DR-1 R3 / DR-3) strengthens the ruling: a dismissal is a recorded fact, never retracted |
| 26 | `AddReminderRule` | reminder surface | — | no — `NotUndoable(ReminderSurface)` | — | **#354**; rules are cheap to remove by hand (`RemoveReminderRule` exists as its own verb) |
| 27 | `RemoveReminderRule` | reminder surface | — | no — `NotUndoable(ReminderSurface)` | — | **#354** |
| 28 | `ClearReminderRules` | reminder surface | — | no — `NotUndoable(ReminderSurface)` | — | **#354** |
| 29 | `UnbindBoard` | ceremony | — | no — `NotUndoable(BoardCeremony)` | — | **#357**; stranding is honest, re-binding is the undo |
| 30 | `ReconcileByTitle` | ceremony (idempotent merge) | — | no — `NotUndoable(BoardCeremony)` | — | **#357** |
| 31 | `CreateAdhocAlarm` | reminder surface *(mechanically invertible via `CancelAdhocAlarm`)* | — | no — `NotUndoable(ReminderSurface)` | — | **#354**, family widened to the adhoc-alarm pair at the reaffirmation; ruled out with the surface: ephemeral timers are cheap to re-add |
| 32 | `CancelAdhocAlarm` | reminder surface | — | no — `NotUndoable(ReminderSurface)` | — | **#354**; undo-of-cancel would re-create under a fresh id — identity churn for no value |
| 33 | `StartTracking` *(new — DR-5/1)* | tracking surface (**inverse not expressible**) | — | no — `NotUndoable(TrackingSurface)` | — | The start closes every foreign open entry and mints one; its inverse would have to re-open closed entries — **structurally impossible through the command catalogue** (ADR 0017: no payload clears a stamped end; `AdjustTimeEntry` cannot un-close; `DeleteTimeEntry` cannot reopen the foreign entries). A synthesis outside the catalogue is forbidden (it forks the semantics). Recorded reason per the dispatch mandate |
| 34 | `StopTracking` *(new)* | tracking surface (**inverse not expressible**) | — | no — `NotUndoable(TrackingSurface)` | — | same structural reason as #33 |
| 35 | `LogTime` *(new)* | tracking surface *(mechanically invertible: `DeleteTimeEntry` on the minted id)* | — | no — `NotUndoable(TrackingSurface)` | — | v1 scope closed by the 2026-10-26 session ruling 2 ("no v2 item folds in — v1 ships exactly the 2026-10-19 scope"); the tracking verbs are post-design surface, and widening the invertible family is a scope change a chunk cannot make. Mechanical class recorded for the v2 widening (decision D20) |
| 36 | `AdjustTimeEntry` *(new)* | tracking surface *(mechanically invertible: same verb with pre start/end — the change mask expresses both)* | — | no — `NotUndoable(TrackingSurface)` | — | same scope reason as #35 |
| 37 | `DeleteTimeEntry` *(new)* | restore-class | — | no — `NotUndoable(RestoreClass)` | — | No un-delete verb exists (`LogTime` mints fresh ids — resurrect-by-re-log is identity churn); joins the #356 restore-family for v2 |
| 38 | `CreateHabit` *(new — DR-6/5)* | habit surface *(mechanically invertible: `DeleteHabit`)* | — | no — `NotUndoable(HabitSurface)` | — | post-design surface; v1 scope closed (same reason as #35) |
| 39 | `UpdateHabit` *(new)* | habit surface *(mechanically invertible: same verb with pre `HabitChanges`)* | — | no — `NotUndoable(HabitSurface)` | — | same scope reason as #38 |
| 40 | `MoveHabit` *(new)* | habit surface *(mechanically invertible)* | — | no — `NotUndoable(HabitSurface)` | — | same scope reason as #38 |
| 41 | `LogHabit` *(new)* | habit surface (**inverse lossy**) | — | no — `NotUndoable(HabitSurface)` | — | The inverse would decrement the window count via `AdjustHabitLog`, but a pre-log *absent* row would become an explicit count-0 row — a different stored fact (the domain deliberately keeps "emptied window keeps its row" distinct from "no row"), and no log-row delete action exists. Lossy ⇒ refuse-by-classification, recorded |
| 42 | `AdjustHabitLog` *(new)* | habit surface *(mechanically invertible: same verb with pre count)* | — | no — `NotUndoable(HabitSurface)` | — | Expressible but ruled out with the family (#38's scope reason) |
| 43 | `DeleteHabit` *(new)* | restore-class | — | no — `NotUndoable(RestoreClass)` | — | No `RestoreHabit` verb; tombstone-only delete; joins the #356 restore-family shape for v2 |
| 44 | `RequestSync` | not a mutation | — | n/a — **never journaled** | — | D-U1 scope: accepted *mutating* commands; the engine forwards it before the planner |
| + | `Undo` *(added by this chunk)* | engine-routed | — | never journaled | — | Routed to `plan_undo` by the wrapper before `plan_command` sees it; `plan_command` carries a defensive typed-rejection arm |
| + | `Redo` *(added by this chunk)* | engine-routed | — | never journaled | — | symmetric with `Undo` |
| + | `RestoreTask` *(added by this chunk)* | create-class (if ever journaled via direct dispatch — not a v1 user surface) | `DeleteTask { id }` | journaled like any accepted mutation | the resurrected task's stamps | The inverse *instrument* of row 6, not a UI verb in v1 |

**v1 answer summary**: 13 arms undoable (12 fully + `CreateStack`
refuse-if-occupied) — exactly the D-U5 scope; 30 arms typed
`NotUndoable` with the reason families `RestoreClass` (#356),
`BoardCeremony` (#357), `ReminderSurface` (#354), `TrackingSurface` and
`HabitSurface` (post-design surface, scope-closed — the two new reason
variants), 1 never journaled. The v2-widening candidates among the new
arms (rows 18–20, 31, 35, 36, 38–40, 42 — the "mechanically invertible"
notes) are **recorded here, not scheduled**: stage 3 should open one
backlog issue (`backlog: undo v2 — tracking & habit arm classification`,
`architecture`, `p3`, body = permalink to this plan §3) alongside the
PR; this chunk does not open it (decision D20).

## 4. Journal data shapes (domain `journal.rs`, new module)

All pure data + pure functions; serde derives throughout (the shapes
ride `PersistedState`'s serde and the sqlite JSON payload).

```rust
pub struct JournalId(Uuid);            // ids.rs local_id! macro (Ord, serde)
// minted via IdGenerator::new_journal_id() (UUIDv7) — trait widened,
// both implementors updated (UuidV7Generator, test_support::CountingIds)

pub struct JournalEntry {
    pub id: JournalId,
    pub parent: Option<JournalId>,     // the line head when this command planned
    pub command: StateCommand,         // the intent, verbatim (the undo label)
    pub facts: UndoFacts,
    pub ops: Vec<OpId>,                // the batch's enqueued op ids, in batch order
    pub pushed: bool,                  // flipped by MarkJournalPushed, never by planning
    pub board: Option<BoardId>,        // best-effort touched-board tag (D-U4 per-board-later note); None when not board-scoped
    pub created_at: DateTime<Utc>,
}

pub enum UndoFacts {
    PreImages { images: Vec<PreImage>, watermark: UndoWatermark },
    NotUndoable(NotUndoableReason),
}

pub enum NotUndoableReason {          // Copy; the #353–#357 families + the two new ones
    RestoreClass, BoardCeremony, ReminderSurface, TrackingSurface, HabitSurface,
}

pub enum PreImage {                   // before: None = the entity did not exist (created)
    Board { id: BoardId, before: Option<Box<Board>> },
    Stack { id: StackId, before: Option<Box<Stack>> },
    Task { id: TaskId, before: Option<Box<Task>> },
    Label { id: LabelId, before: Option<Box<Label>> },
    Habit { id: HabitId, before: Option<Box<Habit>> },
    TimeEntry { id: TimeEntryId, before: Option<Box<TimeEntry>> },
}
// Box: a Task with its baseline dwarfs the other payloads; the enum's
// other arms stay small.

pub struct UndoWatermark(BTreeMap<JournalEntity, EntityStamps>);  // deterministic order (snapshots)

pub enum JournalEntity {              // Ord via field order; ids are Ord
    Board(BoardId), Stack(StackId), Task(TaskId), Label(LabelId),
    Habit(HabitId), TimeEntry(TimeEntryId),
}

pub enum EntityStamps {               // the entity's full post-write clock set (Copy structs)
    Task { clocks: TaskClocks, memberships: BTreeMap<LabelId, MembershipState> },
    Stack(StackClocks), Board(BoardClocks), Label(LabelClocks),
    Habit(HabitClocks), TimeEntry(DateTime<Utc> /* updated_at */),
}

pub struct JournalState {             // PersistedState's 11th field (R-U10)
    pub entries: Vec<JournalEntry>,   // append order = chronological (storage: ORDER BY rowid)
    pub head: Option<JournalId>,      // explicit; moves on undo/redo without a new node
}
// Default: empty entries, head None.

pub struct UndoView {                 // AppState's derived projection (memory-only, like last_updated)
    pub can_undo: bool,               // head.is_some() — a NotUndoable head still shows true; Undo refuses and names (R-U6)
    pub can_redo: bool,               // head has a child
    pub head_label: Option<String>,   // derived display text (never test-asserted)
    pub recent: Vec<UndoLabel>,       // bounded window, newest first, default 10
}
pub struct UndoLabel { pub id: JournalId, pub label: String }
```

Shape decisions:

- **D8 (watermark carries full clock structs)**: recording each touched
  entity's whole clock set is cheap (Copy), keeps the per-arm "which
  fields does the inverse write" selection at the check site (§6), and
  avoids a ~30-variant field enum in the data — the error payload alone
  carries a small `UndoField` discriminant for the toast.
- **D19 (pre-image coverage)**: `pre_images` walks the plan's
  `Upsert*` actions only (R-U8 verbatim). `Record*` rows (dismissals,
  deliveries, habit-log) and `DeleteTimeEntry` carry no pre-image in
  v1 — their arms are `NotUndoable` anyway; the journal entry's
  `command` field is the debug record of the intent.
- **Snapshot determinism**: `BTreeMap` for the watermark and
  memberships, `Vec` append order for entries and ops — two devices
  with equal inputs serialize equally (the state.rs convergence
  property). The journal rides `persisted_state` snapshots; `UndoView`
  rides `published_tail` snapshots (§14 regeneration).
- `UndoView::derive(&JournalState)` is the single derivation used by
  both `EngineCore::app()` and `From<PersistedState> for AppState`.

## 5. The wrapper contract (`plan_journaled_command`)

```rust
pub fn plan_journaled_command(
    app: &AppState, outbox: &[PendingOp], journal: &JournalState,
    command: &StateCommand, ids: &dyn IdGenerator,
    defaults: &ReminderDefaults, day: DayCalendar, now: DateTime<Utc>,
) -> Result<CommandPlan, CommandError>
```

Binding contract (R-U8 + decisions):

1. **Pure.** Same input set as `plan_command` plus `journal`; no IO, no
   runtime, Miri-clean.
2. **Same batch.** The journal entry rides the command's own batch as
   `PersistenceAction::RecordJournal(entry)`, appended **last**. The
   applied batch and the interpreted batch stay identical — the
   single-advance rule covers the journal by construction.
3. **Pre-images from the pre-state only.** `pre_images(app,
   &plan.actions)` runs *after* `plan_command` returns, diffing the
   plan's upserts against `app`. Nothing about the journal enters
   `plan_command`.
4. **Routing.** `StateCommand::Undo` → `plan_undo`, `Redo` →
   `plan_redo` (§7); everything else → `plan_command`, then:
5. **Empty batch ⇒ no entry.** An accepted no-op (`NoOp` outcomes) and
   `RequestSync` plan zero actions and journal nothing — the
   empty-batch rule makes "accepted *mutating* command" mechanical; no
   per-arm journaling switch exists (D15).
6. **Entry assembly**: `parent = journal.head`; `command` verbatim;
   `facts = PreImages { pre_images, watermark }` for the 13 undoable
   arms, `NotUndoable(reason)` per §3 otherwise; `ops` = the batch's
   `EnqueueOp` ids in order; `pushed = false`; `board` derived from the
   primary touched entity (task → stack → board; stack/label/habit →
   their board; time entry → task → stack → board; `None` when
   ambiguous e.g. `StopTracking`); `created_at = now`. The watermark is
   computed per §6.
7. The engine's one call site (`actor.rs:649`) switches to the wrapper.
   `EngineCore` gains `pub fn journal(&self) -> &JournalState`.

**D6 (what "plan_command's catalogue is NOT edited" means here)**: the
rule (R-U8) forbids weaving journal capture into the planner and
editing existing arms' semantics. `RestoreTask` is a *new verb* whose
three-case planner arm lands in `command.rs` (that file is the
semantics catalogue — §5 chunk 1 lists "RestoreTask: the three-case
planner" as a domain deliverable, and the lockstep requires every
`StateCommand` arm to have a `plan_command` arm). `Undo`/`Redo` get
defensive arms returning a new typed rejection
(`CommandError::EngineRouted`-class variant — unreachable through the
wrapper, total function preserved). Zero existing arms change.

## 6. Watermark mechanics (R-U7 instantiated)

- **Recorded data**: per touched entity (every entity the batch
  upserted), the post-write clock set (§4 `EntityStamps`) — captured by
  the same post-plan pass as the pre-images, from the plan's upsert
  payloads (they carry the clocks inside).
- **Checked fields = the inverse's write set** (decision D7). The
  generic "all stamped fields" reading would make undo of
  `SetTaskDone` refuse after an unrelated entry adjust (the closes rode
  the same batch) — wrong. The check therefore compares exactly the
  fields the chosen inverse restores, per the table's "watermark scope"
  column: creation stamps for create-class, changed-field stamps for
  same-verb class, the membership stamp for the label pair, the
  `deleted` clock for `DeleteTask`.
- **Excluded forever**: `remote`, `remote_seen`, `remote_baseline`,
  validators, outbox state — observation machinery (D-U3/R-U12). The
  post-push echo adoption of a pushed create must not refuse the
  delete-push undo.
- **Refusal**: first divergent (entity, field) ⇒
  `CommandError::TargetNoLongerInvertible { entity: JournalEntity,
  field: UndoField }` — refuse-and-name, never force-apply. `UndoField`
  is a small Copy enum over the clocked fields (error payload only).
- **Check order** in `plan_undo`: `NothingToUndo` → `TargetNotUndoable`
  (R-U6) → watermark → `CreateStack` occupancy. Deterministic, cheapest
  first.

## 7. Line mechanics: undo, redo, head

**D3 (model reconciliation)**: D-U2's sentence "the undo command is
itself a journal node" described the 2026-10-19 sketch; R-U10's
accepted mechanics supersede it: the head "moves on undo without any
new node being written". Adopted model: **entries are the accepted
mutating commands; `Undo`/`Redo` write no nodes — they move the
persisted head and apply their effects in their own batches.** D-U2's
*requirement* — redo is symmetric under the pushed-edit rule — is
satisfied semantically: redo re-plans the recorded intent through
`plan_command` (fresh ops ⇒ new edit, always), and undo's two op
behaviors are per-entry via the `pushed` flag.

**D4 (the third journal action)**: with no undo node, the head move is
itself state change that must survive restarts (R-U10: the head cannot
be derived). A persisted head therefore needs its own action:

```rust
PersistenceAction::RecordJournal(JournalEntry)     // append + head := entry.id
PersistenceAction::MarkJournalPushed(JournalId)    // idempotent flag flip
PersistenceAction::MoveJournalHead(Option<JournalId>) // undo/redo head move
```

`apply_actions` arms: record appends (with eviction, §9) and sets head;
mark sets `pushed = true` unconditionally (idempotent by construction);
move assigns head (None allowed). The undo/redo batch **always**
contains the head-move action, so the engine's empty-batch early return
can never eat a line move.

**`plan_undo(app, outbox, journal, ids, defaults, day, now)`**:

1. `journal.head == None` ⇒ `Err(NothingToUndo)`.
2. Head entry `NotUndoable(reason)` ⇒ `Err(TargetNotUndoable(reason))`
   — refuse and name, never skip (R-U6).
3. Watermark check (§6) ⇒ `Err(TargetNoLongerInvertible { .. })`.
4. Arm guards: `CreateStack` occupancy ⇒
   `Err(UndoTargetHasDependents { stack })`.
5. Synthesize the inverse `StateCommand` per §3 (e.g. the pre-image
   `TaskChanges` derived by diffing the stored pre-image entity against
   the current entity over the recorded fields — sound because the
   watermark just proved them unchanged since the command). Plan it
   through `plan_command` (typed rejections, no-op semantics,
   bound/unbound splits, cascades, op enqueue, nudge: all inherited).
6. **Op handling** (D-U2's one rule, two behaviors):
   - `entry.pushed == false` (retraction): drop the inverse plan's
     `EnqueueOp`s; append `FailOp` for each of `entry.ops` (all still
     pending by the flag's honesty). The restore writes land locally
     with fresh stamps; the server never knew. No nudge (no enqueues).
   - `entry.pushed == true` (new edit): keep the plan verbatim — fresh
     ops, fresh stamps; never a remote take-back.
7. Batch = inverse actions + `MoveJournalHead(entry.parent)`.
8. Outcome: `CommandOutcome::Undone { entry: entry.id }` (the durable
   receipt — F16-13's correlation; the label lives in `UndoView`).

**`plan_redo(...)`**:

1. Head None or no child ⇒ `Err(NothingToRedo)`.
2. **D5 (redo target)**: head's child with the **greatest
   (`created_at`, `id`)** — stateless beyond the head, deterministic,
   and equivalent to editor discipline in every v1-reachable case (a
   new edit after undos always becomes the newest child, so the forked
   branch's redo path is dead exactly as D-U4 describes; the tree data
   stays queryable for the v2 picker, #353). A `NotUndoable` child is
   unreachable as a redo target (undo refuses at it, so the line never
   retreats past it) — defensive typed refusal anyway.
3. No watermark check: redo re-plans the **forward** intent through
   `plan_command` against the restored state — a fresh edit with fresh
   stamps by construction; plan_command's typed rejections still apply
   (e.g. the task was deleted by a sync since ⇒ `TaskDeleted`). A
   replanned no-op (sync already re-applied the effect) yields a
   head-move-only batch — honest, the line advances.
4. Batch = forward plan actions + `MoveJournalHead(target.id)`;
   outcome `Redone { entry: target.id }`.

## 8. The R-U9 post-pass — placement in both ingestion paths

Both `CompleteOp` emission sites are inside the pipeline functions
(`apply_sync_report` ~`pipeline.rs:1354`, `apply_push_report`
~`:1551`), both called from the single `ingest_sync_report` match
(`actor.rs:771` / `:875`). The post-pass therefore sits at **one call
site** serving both arms: immediately after the match, before the
empty-batch check and `repo.apply`:

```rust
let marks = journal_push_marks(core.journal(), &actions);
if !marks.is_empty() { actions.extend(marks); }
```

`journal_push_marks(&journal, &actions) -> Vec<PersistenceAction>`:
for every `CompleteOp(op_id)` in the batch, the entries whose `ops`
contain it and whose `pushed == false` — deduped, one
`MarkJournalPushed` per entry (multi-entry op sets, e.g. `DeleteStack`
cascades completing across entries, flag each owning entry exactly
once; idempotent at the apply arm and at the SQL `UPDATE ... SET
pushed = 1` regardless). Pipeline signatures unchanged; the flag flip
is crash-consistent with the completion it annotates (same sqlite
transaction). `execute_command` never completes ops — no post-pass
there.

## 9. Retention eviction (R-U11) — with the storage mirror

- Pure helper `evict_journal(&[JournalEntry], incoming_created_at) ->
  Vec<JournalId>`: oldest-first **leaf** entries (no descendants via
  `parent`) beyond **cap 1000** and older than the **90-day** floor,
  judged against the incoming entry's stamp — deterministic, no `now`
  injection. Domain consts `JOURNAL_CAP`, `JOURNAL_AGE_FLOOR`.
- `apply_actions`' `RecordJournal` arm inserts the entry, then removes
  the evicted ids; if `head` is somehow in the evicted set (only
  reachable in degenerate shapes), `head = None` — a pruned line
  reports `NothingToUndo`, the ruled graceful degradation.
- **D11 (storage mirror — forced by memory ≡ disk)**: the sqlite repo
  must delete the same rows or the next boot resurrects pruned entries.
  The repo's `RecordJournal` handling (inside the same transaction)
  reads the table's `(id, parent, created_at)` rows after the insert,
  calls the **same domain pure helper**, and `DELETE`s the returned
  ids. One extra SELECT per recorded command (human-rate; fine).
  "No new pruning action" (R-U11) is preserved: no new
  `PersistenceAction` exists — the eviction decision stays a single
  pure domain function shared by both sides.
- The `InMemoryRepository` fake delegates to `apply_actions` and
  inherits eviction for free.

## 10. Storage — migration 0016, head encoding, codec, hydration

**Migration `0016_command_journal.sql`** (additive, single transaction,
no down-migration; chrono TEXT stamps stored but never `ORDER BY`-ed):

```sql
CREATE TABLE command_journal (
    id         TEXT PRIMARY KEY NOT NULL,
    parent     TEXT,
    created_at TEXT NOT NULL,
    pushed     INTEGER NOT NULL DEFAULT 0,
    board      TEXT,
    payload    TEXT NOT NULL          -- serde_json of JournalPayload { command, facts }
);
CREATE TABLE command_journal_head (
    id TEXT                            -- exactly one row; NULL = empty line
);
INSERT INTO command_journal_head (id) VALUES (NULL);
```

**D12 (head encoding — R-U10 freedom)**: a dedicated one-row metadata
table, not a flagged column — a flag would touch two rows per move and
invites double-head corruption; the metadata row is a single `UPDATE`
per move and a single cell at load.

**D13 (payload split)**: scalars as columns (queryable, debuggable),
`command` + `facts` as one JSON payload (serde round-trip pinned by a
test; decode failure ⇒ `RepositoryError::Corrupted`).

- **`execute_batch`** (the no-wildcard partition): three new buckets —
  journal appends, journal head moves, journal flag flips — flushed in
  the fixed order after the existing partitions (the journal references
  op ids by value only; no FK constraints, eviction order owns
  integrity). The E0004 no-wildcard rule makes the storage build fail
  until the three arms persist — the second lockstep.
- **Hydration**: `SELECT ... FROM command_journal ORDER BY rowid`
  (insertion order = chronological; the convention forbids ordering by
  TEXT stamps) + the head row; absent table impossible post-migration,
  empty rows ⇒ `JournalState::default()` (upgrade-pinned test: a DB at
  0015 migrated up hydrates an empty journal; then accepts writes).
- `repo.rs`'s stale "0016 = DR-6/6 habit tables" comment renumbers to
  0017 in the same diff (mechanical, recorded in §2).

## 11. `RestoreTask` — the three-case planner

`StateCommand::RestoreTask { id: TaskId, resurrect: Box<Task> }`
(decision D10: the pre-image entity rides the command — self-contained,
`plan_command` stays journal-free; the synthesis builds it from the
journal entry's pre-images). Planner semantics, mirroring the delete
path's split in reverse:

1. **Pending delete**: a `DeleteTask` op for `id` still sits in the
   outbox (detect: `op_targets_task` + kind, or via the deleting
   entry's `ops` at synthesis time — planner checks the outbox slice)
   ⇒ `FailOp` it, write `resurrect` with `deleted = false` and a fresh
   `clocks.deleted` — **no new op** (the server never saw the delete;
   pure retraction).
2. **Pushed delete** (no pending op, task was bound): resurrect with
   fresh clocks + enqueue a **create-class op** — Deck undelete is
   re-POST; the restored task re-binds to a fresh remote card id
   (identity churn is the honest price, D-U2; the surfaces chunk's copy
   must not promise identity preservation — risk §16.2).
3. **Unbound, never pushed**: resurrect locally, no op.

The resurrect write is `deleted = false` + fresh `clocks.deleted` —
the R5 merge-rule field shape (F16-14). Target-not-tombstoned ⇒
accepted no-op (terminal-repeat discipline); unknown id ⇒
`UnknownTask`. Precondition order: existence → tombstone check →
outbox scan.

## 12. Engine wiring (`taskboard-state`)

- `execute_command`'s planner call switches to `plan_journaled_command`
  (`actor.rs:649`); `RequestSync` keeps its pre-planner early return
  (never journaled). Everything downstream — empty-batch reply,
  `repo.apply`, `interpret`, nudge (`enqueued_ops` retraction batches
  contain none ⇒ no spurious nudge), withdrawals, publish — unchanged.
- `ingest_sync_report`: the §8 post-pass.
- `EngineCore`: `journal()` accessor; `app()` derives
  `undo: UndoView::derive(&self.state.journal)`; `From<PersistedState>
  for AppState` derives the same.
- `AppState` gains `pub undo: UndoView` (Serialize-only derive chain
  preserved); `PersistedState` gains `pub journal: JournalState`.
- `Undo`/`Redo` ride `EngineEnvelope::Execute` — **no new envelope
  variant, no new channel, no new actor**; `EngineSignal` stays
  `Copy`-only, single variant (E-15 stays dodged).
- Structural constructors of `AppState`/`PersistedState` outside these
  two sites (tests, `bench_fixtures.rs`) pick up the new fields —
  `..Default::default()` sites are unaffected.

## 13. ADR 0019 + the dated ADR 0006 amendment

- **ADR 0019** (`docs/adr/0019-undo-command-journal.org`): records
  D-U1…D-U4 + the D-U5 scope as the decision, R-U6…R-U12 as the
  refinement rulings, and this plan's §15 decisions D1–D20 as the
  chunk-level settlements (migration 0016 deviation included).
- **ADR 0006 amendment** (the Consequences bullet "The engine has no
  undo path", `0006-…org:104`): a dated note — as of 0019 the journal
  rides the single-advance rule (`RecordJournal`/`MoveJournalHead`/
  `MarkJournalPushed` are batch actions); ordering, not rollback,
  remains the correctness story. The sentence is amended, never
  rewritten away.
- **CHANGELOG** Unreleased/Added: the engine verbs, the
  restart-surviving undo line, and the storage migration (user-visible
  via chunk 2's CLI later; the entry records the engine contract).

## 14. Test catalogue (TDD-first; AGENTS §5 throughout)

Build order = test-first order. Every commit standalone green (§17).

1. **Domain shapes + apply arms** (`journal.rs` unit tests +
   `persistence.rs` tests):
   - `RecordJournal` appends + moves head; `MoveJournalHead` moves;
     `MarkJournalPushed` flips idempotently (double-apply pinned).
   - Eviction: cap overflow evicts oldest leaves only (a parent with a
     descendant survives); age floor vs the incoming stamp; head-in-
     evicted ⇒ None; a pruned head yields `NothingToUndo` at plan_undo.
   - Serde round-trip of `JournalEntry` (all `UndoFacts` variants,
     `PreImage::before: None` included).
2. **Wrapper + pre-images + watermark** (pure, seeded `AppState`s):
   - An accepted mutating command's batch ends with `RecordJournal`;
     `ops` = batch enqueues in order; `parent` = prior head; empty
     batch ⇒ no entry (`RequestSync`, `NoOp` repeats).
   - `pre_images`: overwrite captures the pre entity; create captures
     `before: None`; `SetTaskDone` with closes captures the closed
     entries; cascade (`DeleteStack` — journaled NotUndoable) still
     captures its task pre-images.
   - Watermark per-arm scope: the §3 column, one test per undoable arm
     class — interpose a later write on a *covered* field ⇒
     `TargetNoLongerInvertible`; interpose on an *uncovered* field
     (entry adjust after `SetTaskDone`; rule re-attach after the
     suppressed clear; echo-shaped rebinding of a pushed create) ⇒ undo
     still proceeds.
   - Observation machinery never rewound: after undo,
     `remote_seen`/`remote_baseline`/validators are byte-identical;
     restored values carry fresh write clocks.
3. **Synthesis catalogue + `plan_undo`/`plan_redo`** (the 13 rows, one
   behavior one test, lowest layer = domain pure):
   - Per undoable arm: plan → undo → pre-image values restored, typed
     outcome, inverse ops per pushed-edit class (retraction:
     `FailOp(entry.ops)` + zero `EnqueueOp`; pushed: inverse ops
     enqueue, no `FailOp`).
   - `SetTaskDone` enriched: undo reopens the flag only — the spawned
     successor stays, closed entries stay closed, dismissals stay
     dismissed, suppressed recur clear stays cleared (D9's row, pinned).
   - `RestoreTask` three cases (§11) — pending-op retraction with no
     new op; pushed resurrect with create op; local resurrect; plus
     no-op/unknown-id repeats.
   - `CreateStack` occupied ⇒ `UndoTargetHasDependents`; renamed-since
     ⇒ `TargetNoLongerInvertible`.
   - R-U6: `NotUndoable` head ⇒ `TargetNotUndoable` (snooze head,
     tracking head, habit head — one representative per reason family).
   - Line mechanics: undo moves head (persisted via the batch's
     `MoveJournalHead`); redo re-applies the forward intent; new edit
     after undo forks (`parent = head`); D5's max-child redo rule
     (fork retires the old branch's redo); `NothingToUndo`/
     `NothingToRedo` on empty line/tip.
   - Redo after a sync-interleaved write replans honestly (fresh edit,
     or head-move-only on an already-true effect).
4. **Lockstep**: the synthesis match has no wildcard — a new
   `StateCommand` arm fails compilation (the test *is* the type
   system). A completeness smoke walks every arm through
   `plan_journaled_command` asserting entry-or-noop only (no
   per-arm duplication of the unit tests).
5. **Property (proptest)**: random invertible-command sequences over a
   generated state; undo-all ⇒ field-level pre-image restoration;
   retraction class empties the entry's ops from the outbox; pushed
   class (flag pre-set) enqueues inverse ops; redo round-trips;
   watermark monotonicity (any interleaved write on a covered field ⇒
   typed refusal, never corruption). Strategies extend
   `test_support` (the habit/recur strategy patterns).
6. **Storage** (`crates/taskboard-storage-sqlite/tests/`):
   - Migration: fresh DB at head ⇒ tables exist, journal empty, head
     NULL; 0015-schema DB migrated up hydrates empty (upgrade pin).
   - Codec: payload JSON round-trip; corrupt payload ⇒ `Corrupted`.
   - `RecordJournal` insert + head write + the eviction mirror (D11:
     evicted rows actually deleted — a pruned journal stays pruned
     across `load()`).
   - `MarkJournalPushed` idempotence on the real repo (double-flip,
     multi-entry op set — D-U5 risk (a)).
   - Head survives restart: apply, drop, `load()` ⇒ same head
     (R-U10's v1 contract, pinned).
7. **Engine** (`crates/taskboard-state/tests/` — new `undo.rs`, plus
   `sqlite_smoke.rs` additions):
   - End-to-end: `EngineHandle::execute` a command, execute `Undo`,
     assert the published state and the fake repo's persisted journal;
     storage-failure arm leaves journal and head untouched.
   - Report ingestion flips `pushed`: a synced create's report lands,
     the batch carries `MarkJournalPushed` (both arms: `Completed` and
     `Failed`-with-pushes).
   - Routing only (per the A-series discipline): semantics stay in the
     domain suite.
8. **Insta**: new undo scenarios join the corpus (e.g. s50 undo
   restores + moves head; s51 redo re-applies; s52 a `NotUndoable`
   head journaling) — **new** `.snap` files via the normal
   bundle-write flow. The **existing 33 pairs** regenerate **once**
   — the diff is exactly `journal:` on persisted_state and `undo:` on
   published_tail, reviewed hunk-by-hunk as "the widening, nothing
   else" (risk §16.3).
9. **Determinism**: fixed/stepping clocks everywhere (the
   `plan_command` pattern); `CountingIds` + `new_journal_id`; no
   sleeps; engine tests poll with the existing deadline helpers.

## 15. Decision record (chunk-level; one per open question)

| # | Question | Decision |
|---|---|---|
| D1 | Migration number | **0016** (`0016_command_journal.sql`) — the dispatch's 0015 is taken by DR-5/2 at this base; deviation recorded per dispatch §2 rule 3; DR-6/6 renumbers to 0017 (stage 3 notes it in the PR body) |
| D2 | ADR number | **0019**, matching the dispatch assignment (head at base is 0018) |
| D3 | D-U2's "undo is a journal node" vs R-U10's "head moves without a new node" | R-U10 supersedes (later, accepted, explicit): entries = accepted mutating commands; `Undo`/`Redo` write no nodes; D-U2's redo symmetry holds semantically (redo replans the forward intent under the identical pushed-edit rule) |
| D4 | How the head move persists | Third journal action `MoveJournalHead(Option<JournalId>)`; undo/redo batches always contain it (the empty-batch early return can never eat a line move) |
| D5 | Redo target at a fork | Head's child with the greatest `('created_at', 'id')` — stateless, deterministic, editor-equivalent in every v1-reachable case; `NotUndoable` children unreachable (defensive typed refusal) |
| D6 | Scope of "plan_command's catalogue is NOT edited" | No existing arm changes and no journal logic enters the planner; the new `RestoreTask` arm + defensive `Undo`/`Redo` arms land in `command.rs` (new surface, lockstep-covered) |
| D7 | Watermark scope | Exactly the inverse's write set (per-arm, §3 column) — not "all stamped fields"; observation/binding fields (`remote`, `remote_seen`, `remote_baseline`) and ops never watermarked |
| D8 | Watermark data shape | Per-entity full clock structs (`EntityStamps`) + the task's membership map in a `BTreeMap`; the refusal payload carries entity + a small `UndoField` discriminant |
| D9 | The enriched `SetTaskDone` row | Undo restores the done flag only; spawn successor / entry closes / dismissals / suppressed recur clear are one-way facts (per-fact reasons in §3 row 4); watermark = `done` clock |
| D10 | `RestoreTask` payload | `RestoreTask { id, resurrect: Box<Task> }` — self-contained, `plan_command` journal-free |
| D11 | Retention vs memory ≡ disk | The sqlite repo mirrors eviction by calling the same domain pure helper inside the record transaction (SELECT + DELETE); no new `PersistenceAction` (R-U11 preserved) |
| D12 | Head encoding | Dedicated one-row `command_journal_head` table (metadata row), not a flagged column |
| D13 | Journal payload split | Scalar columns (id, parent, created_at, pushed, board) + JSON payload (command, facts); hydrate `ORDER BY rowid`, never by TEXT stamps |
| D14 | Receipt/refusal types | `CommandOutcome::Undone`/`Redone { entry: JournalId }`; refusals are `CommandError` variants (`NothingToUndo`, `NothingToRedo`, `TargetNotUndoable(NotUndoableReason)`, `TargetNoLongerInvertible { entity, field }`, `UndoTargetHasDependents { stack }`, engine-routed guard) — `CommandError` stays `Copy` |
| D15 | Which commands journal | Batch non-empty ⇔ journaled (accepted *effective* mutations); `NoOp`/`RequestSync` fall out mechanically |
| D16 | Batch position of `RecordJournal` | Appended last (after all effects; reads naturally, order-free at apply) |
| D17 | New arms' classification | Tracking + habit verbs `NotUndoable` in v1 (scope closed by the 2026-10-26 session ruling 2; per-row mechanical class recorded for v2); `StartTracking`/`StopTracking`/`LogHabit`/`DeleteTimeEntry`/`DeleteHabit` additionally structurally non-invertible (§3 rows 33/34/37/41/43) |
| D18 | `IdGenerator` | Gains `new_journal_id()`; both implementors updated |
| D19 | Pre-image coverage | `Upsert{Board,Stack,Task,Label,Habit,TimeEntry}` with `before: Option<_>` (None = created); `Record*` actions carry no pre-image in v1 |
| D20 | v2-widening bookkeeping | The mechanically-invertible rows (§3: 18–20, 31, 35, 36, 38–40, 42) are recorded here; stage 3 opens one `backlog:` issue alongside the PR — this chunk opens nothing |

## 16. Risks (carried from origin §9, updated)

1. **Multi-entry op sets double-flip** — pinned idempotent at the apply
   arm, the SQL UPDATE, and both `journal_push_marks` dedup paths
   (storage + domain tests).
2. **`RestoreTask` pushed-case identity churn** — engine/domain tests
   pin the create-op re-POST shape; the no-identity-promise copy is
   undo/2's, flagged in the PR body for the surfaces chunk.
3. **Insta regeneration breadth grew** — 33 pairs (was 19); still one
   deliberate pass, diff-audited hunk-by-hunk; new undo scenarios are
   *new* files, not regeneration.
4. **Migration renumber ripple** — DR-6/6's issue body and the stale
   `repo.rs` comment say 0016; this chunk takes 0016 and documents the
   0017 renumber for the wave-10 agent (§2, D1).
5. **Watermark false-refusal surface** — bounded by D7's per-arm scope
   (the uncovered-field tests pin the benign interpositions); the
   synced-board refusal rate remains a real-usage observation for the
   #353–#356 family, unchanged from the origin's ruling.
6. **Journal memory/disk footprint** — whole-entity pre-images × 1000
   cap: a few MB worst case; the cap is the knob (AGENTS §1 kiosk
   budget, origin ruling unchanged).
7. **Redo of a spawned completion double-spawns** (D3/D9 consequence):
   redo re-runs the full `SetTaskDone` arm; the head's still-admitting
   rule spawns a second successor. Accepted v1 semantics (redo is a new
   edit; the successor is a real task); surfaced here and in the PR so
   the surfaces chunk's toast copy and any future refinement start from
   the honest behavior.
8. **Concurrent dependabot PRs** (#539–#541) may land first — the
   stage-3 rebase dance (AGENTS §8) absorbs Cargo.lock conflicts;
   nothing structural.

## 17. Suggested commit spine (stage 2; every commit standalone green)

1. `feat(domain,state)`: journal substrate — shapes, `IdGenerator`
   widening, the three actions + apply arms + eviction, `JournalState`
   + `UndoView` fields, **the one snapshot regeneration pass** (the
   widening, nothing else).
2. `feat(domain)`: the synthesis catalogue, `plan_undo`/`plan_redo`,
   `RestoreTask`, `plan_journaled_command`, typed refusals (domain
   suite green; engine not yet switched).
3. `feat(state,storage)`: migration 0016 + codec/hydration + the
   engine switch + the R-U9 post-pass + engine/sqlite tests.
4. `docs(adr)`: ADR 0019 + the dated ADR 0006 amendment +
   CHANGELOG entry.

Gates: the full AGENTS §10 suite via `scripts/run-quality-gates.sh`
from the worktree. Handoff: stage 3 per the dispatch protocol (rebase,
review, gates, push, PR, `CLEAN`, worktree teardown). No push, no PR,
no issue edits before stage 3.

## 18. References

- `.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5; §3.1 journal shape, §3.2 pushed-edit rule, §6 test seed)
- `.artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md`
  (§5 chunk 1 normative; §4 R-U6…R-U12; §3 the 33-arm seed; §9 risks)
- `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (lanes, protocol), §2 (resource ladder), §3.6 (binding notes),
  §12.1 (this chunk's prompts)
- Code anchors (base `bb850282`): `messages.rs:21-419` (the 44 arms),
  `command.rs:700` (`plan_command`), `:936` (`SetTaskDone` + spawn),
  `:1050` (`DeleteTask`), `:1151` (`DeleteStack`), `:1993-2323`
  (tracking/habit arms), `persistence.rs:82-106` (`PersistedState`),
  `:184-258` (`PersistenceAction`), `:287` (`apply_actions`),
  `state.rs:25-51` (`AppState`), `actor.rs:616-711`
  (`execute_command`), `:743-935` (`ingest_sync_report`), `core.rs:63`
  (`interpret`), `pipeline.rs:1354/1551` (`CompleteOp` sites),
  `repo.rs:958` (`execute_batch` no-wildcard), migrations `0001-0015`,
  snapshots 33 pairs.
- Issues: #509 (this chunk), #510 (undo/2), #176 (umbrella),
  #353–#357 (v2 families), #244 (lockstep pattern), #481 (the
  migration-contention precedent this dispatch resolves by landing
  order).
