# [Plan] Undo v1 — Implementation Design Round (journal substrate, inverse synthesis, surfaces)

- **Date**: 2026-10-26
- **Kind**: implementation-design round — the accepted 2026-10-19 undo
  architecture turned into a dispatchable v1 spec: anchors re-verified
  against current `main`, the seven-day delta absorbed, the mechanical
  seams this round is allowed to settle settled, the two implementation
  chunks specified. Design only — no production code in this commit.
- **Origin**: the undo architecture design round D4
  (`.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`,
  accepted 2026-10-19 — **D-U1…D-U5 are binding for this round and are
  not re-opened**); implementation carrier **#176**; v2 deferrals
  **#353–#357** (rulings reaffirmed in §8); backlog-triage dispatch plan
  §15 ("undo v2 family — needs-consideration rulings", deliberately
  excluded from that dispatch). Audit inputs carried from the
  architecture round: F16-12/F16-13/F16-14
  (`.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`).
- **Session rulings (2026-10-26, user-confirmed at round entry)**:
  (1) this is an **implementation round** — D-U1…D-U5 untouched;
  (2) **no v2 item folds in** — v1 ships exactly the 2026-10-19 scope,
  #353–#357 stay deferred with reaffirmation comments;
  (3) tracking is **two chunk issues** with #176 as umbrella;
  (4) handoff is **push + PR** (this document's PR).
- **Branch / worktree**: `docs/undo-implementation-design` in
  `../taskboard-WT-docs-undo-implementation-design` (off `origin/main`
  @ `b805117`).
- **Status**: **ACCEPTED** — the user accepted the plan as designed
  (2026-10-26): all four session rulings were already user-confirmed at
  round entry, and §7's refinement decisions (R-U6…R-U12) are confirmed
  as recommended. Implementation is the two chunk issues the round
  opened (#509, #510), dispatched by
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §12 (waves 9–11).

---

## 1. Mission & binding constraints

Implement undo v1 exactly as accepted: an engine command journal with
plan-time pre-images riding the command's own persistence batch (D-U1),
the pushed-edit hard rule (D-U2), observation machinery untouched
(D-U3), a tree-shaped journal with a linear v1 line and the `UndoView`
projection (D-U4), in two chunks — (1) domain + storage + engine +
tests, (2) surfaces — at house scale L (D-U5). Everything that landed
since 2026-10-19 (§3) must be absorbed **without** re-opening any D-U
decision; everything the architecture left mechanical (§4) is settled
here with one recommendation each.

Binding constraints restated from the architecture round, re-verified
on this branch (§2): the single-advance rule is the journal substrate
(journal actions ride the command's own batch — no second stream);
everything user-facing mutates through `plan_command`; merge is LWW
over per-field write clocks with monotone observation stamps; sync is
push-first with no remote take-back; AGENTS §5 test discipline
throughout (no text assertions, no sleeps, injected clocks).

## 2. Verified anchors on current main (this branch, 2026-10-26)

Symbols are the stable keys; line numbers are this branch's.

| Fact | Symbol / file | Anchor |
|---|---|---|
| Command surface: **33 `StateCommand` arms** (post-design arrivals: `CaptureTask`, `DismissReminder`, `CreateAdhocAlarm`, `CancelAdhocAlarm`, `RequestSync`, the full board family) | `StateCommand` | `taskboard-domain/src/messages.rs:20-281` |
| Pure planner over `(&AppState, &[PendingOp], &StateCommand, ids, defaults, now)`; receipt = batch + typed outcome | `plan_command`, `CommandPlan`, `CommandOutcome` | `command.rs:322`, `:47`, `:120` |
| Result-shaped actions: whole-entity upserts, outbox transitions, validators, status, reminder facts; **no prior-value capture anywhere** | `PersistenceAction` (20 variants) | `persistence.rs:177-236` |
| Disk contract the journal joins as a ninth field (`boards…reminders` today) | `PersistedState` | `persistence.rs:82-107` |
| Published projection (no outbox/validators; `last_updated` memory-only) — `UndoView` widens this | `AppState` | `state.rs:25-45` |
| Repository port is `load`/`apply` (+`read_state`) **only** — the journal needs **no port widening** (it rides `apply` batches) | `TaskRepository` | `persistence.rs:29-70` |
| Plan → apply → interpret → publish; storage failure needs no rollback (ADR 0006) | `execute_command` | `taskboard-state/src/actor.rs:582-663` |
| Memory ≡ disk; the batch is the only advance | `EngineCore::interpret` | `core.rs:58` |
| Sequential loop; one message at a time (retract-vs-push race-free, D-U3) | `run_loop` biased `select!` | `actor.rs:330-540` |
| **Four mutation sources**: commands, sync reports, system events, reminder ticks — only the first goes through `plan_command`; the other three bypass the command enum (journal covers commands only, by design) | `execute_command` / `ingest_sync_report` / `go_offline` / `reminder_tick` | `actor.rs:582` / `:706` / `:986` / `:1085` |
| The two `CompleteOp` emission sites `MarkJournalPushed` annotates (push-outcome and sync-report phases) | `pipeline.rs` | `:839`, `:1031` |
| Outbox: per-entity ops, `queued_at` feeds the self-clobber guard | `PendingOp`, `pending_task_intents` | `outbox.rs:23-70`, `pipeline.rs:83-135` |
| Tombstones forever; **no un-delete verb exists** (R5 is a merge rule, not a command) — `RestoreTask` is v1's one new verb | `merge.rs` resurrection note | `merge.rs:1023` (rule R5 `:227-234` in the 2026-10-19 numbering) |
| Repaint signal stays `Copy`-only, single variant | `EngineSignal` | `messages.rs:397-402` |
| ADR 0006 still declares "the engine has no undo path" — chunk 1 amends with a dated note, not a rewrite | ADR 0006 | `docs/adr/0006-…org:104` |
| UI reserved rows exist, wired, `Disabled`; keymap already binds Ctrl+Z / Ctrl+Shift+Z (standard), emacs + vi presets too | `AppUndo`/`AppRedo` | `actions.rs:162-163, 367-383, 1023`; `router.rs:317`; `keymap/presets.rs:65-66, 176-177, 341-342` |
| Snapshot suite is in the narrowed dumps+tails shape (19 scenarios × 2) — the `UndoView`/journal widening is one deliberate regeneration pass over exactly these | `tests/snapshots.rs` + 38 `.snap` files | `taskboard-state/tests/snapshots/` |
| Migrations head **0009**; **0010 is claimed by DR-4 chunk G (#481, `remote_baseline`) if it lands first** — the journal migration takes the next free number *at landing time* | `migrations/` | `0001…0009` |
| ADR head **0016** — the undo ADR takes the next free number at landing time (expected 0017) | `docs/adr/` | `…0016-kiosk-appliance-mode.org` |
| CHANGELOG carries an Unreleased section (both chunks add entries) | `CHANGELOG.md` | `:10` |
| CLI has no undo verb yet; `TasksCmd` is the subcommand enum chunk 2 extends | `TasksCmd` | `taskboard-app/src/cli.rs:679` |

## 3. What changed since 2026-10-19 — and how the design absorbs it

The architecture round wrote its inverse table against "the current +
scheduled surface" (26 arms + the multi-board widening + D-R1). All of
that has landed, plus verbs the table never saw. None of it moves a
D-U decision; the table below is the complete 33-arm classification —
the seed of the exhaustive-match synthesis catalogue (the #244
lockstep pattern: a future `StateCommand` variant is a compile error
until classified).

| # | Command arm | Class / inverse | v1 undo? |
|---|---|---|---|
| 1 | `CreateTask` | inverse `DeleteTask`: unbound/pending ⇒ op retraction (`FailOp`); bound ⇒ delete-push (new edit) | **yes** |
| 2 | `CaptureTask` *(new since design)* | `CreateTask` into the inbox board (D-V5 v2, landed as this verb) — same row as #1 | **yes** |
| 3 | `UpdateTask` (field family via `TaskChanges`) | same verb with pre-image `TaskChanges` | **yes** |
| 4 | `SetTaskDone` | same verb with pre-image flag (done *timestamp* not restored — fresh stamp, honest) | **yes** |
| 5 | `MoveTask` | same verb to pre-image `(stack, order)` | **yes** |
| 6 | `DeleteTask` | **`RestoreTask`** — the one new verb (three-case planner, §5 chunk 1) | **yes** |
| 7 | `CreateStack` | inverse `DeleteStack` synthesis, **refuse-if-occupied** (post-create tasks would be cascade-destroyed) — the watermark (R-U7) is the general form of this refusal | partial |
| 8 | `RenameStack` | same verb with pre-image title | **yes** |
| 9 | `DeleteStack` (cascades N tasks) | restore-class — `RestoreStack` deferred (**#356**) | no — typed `NotUndoable` |
| 10 | `MoveStack` | same verb with pre-image order | **yes** |
| 11 | `CreateLabel` | inverse `DeleteLabel` | **yes** |
| 12 | `UpdateLabel` | same verb with pre-image `LabelChanges` | **yes** |
| 13 | `DeleteLabel` *(gap the 2026-10-19 table never rowed)* | restore-class, no restore verb in v1 — joins the **#356** restore-verb family (its reaffirmation comment widens the family to "restore-class verbs: stack, label") | no — typed `NotUndoable` |
| 14/15 | `AssignLabel` / `UnassignLabel` | each other | **yes** |
| 16-22, 29, 30 | `CreateBoard`, `BindBoard`/`UnbindBoard`, `RenameBoard`, `SetBoardColor`, `SetBoardArchived`, `DeleteBoard`, `ResetBoard`, `ReconcileByTitle` | board family stays typed `NotUndoable` **as ruled** (D-U5 table row): ceremonies are the defense (DF-5), delete/reset pre-images are snapshot-class. Note for later: `RenameBoard`/`SetBoardColor`/`SetBoardArchived` are mechanically invertible same-verb rows — the cheapest possible future widening, recorded on **#357** when its time comes, not scheduled now | no — typed `NotUndoable` |
| 23-28 | `SnoozeReminder`, `SetTaskRemindersMuted`, `DismissReminder` *(new)*, `AddReminderRule`, `RemoveReminderRule`, `ClearReminderRules` | reminder surface as ruled (**#354**); since the design, the surface gained `DismissReminder` and the dismissal-settlement model (DR-1 R3 / DR-3) — dismissal is now a *settled fact*, which strengthens the ruling: deliveries and dismissals are recorded facts undo never retracts | no — typed `NotUndoable` |
| 31/32 | `CreateAdhocAlarm` / `CancelAdhocAlarm` *(new since design)* | reminder surface (**#354** family — its reaffirmation comment widens the family to adhoc alarms). `CancelAdhocAlarm` exists as an explicit verb, so create is mechanically invertible — ruled out with the surface: ephemeral timers are cheap to re-add, and undo-of-cancel re-creates under a fresh id (identity churn for no value) | no — typed `NotUndoable` |
| 33 | `RequestSync` | not a mutation; never journaled (D-U1 scope: *accepted mutating commands*) | n/a |

Landed-since deltas beyond the command enum, each absorbed:

- **Multi-board (DF-1/DF-9/DF-3)**: the inverse table's board
  vocabulary is now literal — no adaptation needed. The journal is
  board-tagged via touched entities (stored anyway, per D-U4's
  per-board-later note).
- **Per-field LWW unchanged by DR-4's landed chunks**; the open
  **DR-4 chunk G (#481, `remote_baseline`)** adds a *remote
  observation* to `merge_task`. D-U3's "observation machinery is
  monotone, undo is a write" extends verbatim: `remote_baseline` is
  never rewound by undo; an undo's fresh clocks win LWW and the next
  cycle's echo refreshes the baseline through the normal report path.
  **No sequencing dependency** — undo composes with either merge
  shape — but the two migrations contend for numbers (§2); whichever
  lands second renumbers (mechanical, one file).
- **GUI r2**: the reserved-row/router/keymap infrastructure is mature;
  chunk 2's UI diff is minimal (§5).
- **Snapshot narrowing (backlog-triage chunk 10, landed)**: the
  `UndoView` widening's regeneration cost shrank to the 19 dump+tail
  pairs — same "one deliberate pass" ruling, smaller diff.

## 4. Round refinements — the mechanical seams, one recommendation each

The architecture round fixed the *what*; these six decisions fix the
*how* at the level an implementer must not guess at. Each is a
recommendation travelling with this plan for acceptance; a chunk agent
deviates only with a recorded reason.

- **R-U6 — line behaviour over a `NotUndoable` head: refuse and name,
  never skip.** When the line head's entry is typed `NotUndoable`,
  `Undo` refuses with `TargetNotUndoable(reason)` and the toast names
  the head's label ("Snooze reminder — not undoable"). The
  alternative (walk the parent chain to the nearest undoable entry)
  was rejected: silently reverting an *older* edit when the user
  aimed at the visible one is the worse failure class — a refused
  Ctrl+Z with an honest label is recoverable, a wrong-target undo may
  not be. `TargetNotUndoable` firing only in the refusal case is
  exactly its D-U4 vocabulary role; branch-picking (v2, #353) is
  where explicit-node selection would use it differently.
- **R-U7 — the invertibility watermark.** D-U1 names the
  `TargetNoLongerInvertible` refusal but leaves its mechanism
  unspecified. Mechanism: each journal entry records, per touched
  entity, the **post-write field-clock stamps the command left**
  (derivable from the batch's own upserts — the actions carry the
  entities with their clocks inside). At undo time, for every touched
  field: current stamp ≠ recorded stamp ⇒ a later write (local edit,
  another undo, or a remote merge adoption) touched it ⇒ refuse
  `TargetNoLongerInvertible` naming entity and field — refuse-and-name,
  never force-apply. This one check subsumes the specialized cases:
  it is the general form of `CreateStack`'s refuse-if-occupied
  (liveness of dependents stays its own outcome — an occupied stack
  is *presence*, not clock divergence) and it makes the classic
  hazard — undo clobbering a remote edit that pulled in after the
  target command — a typed refusal instead of a silent lossy win.
- **R-U8 — pre-image capture seam: a pure post-plan pass inside a
  wrapper, `plan_command`'s catalogue untouched.** Pre-images are
  captured *after* planning, *from the pre-state*, by walking the
  plan's `Upsert*` actions against `app` (`pre_images(&app, &actions)
  -> Vec<PreImage>` — arm-agnostic, cascades captured for free). It
  runs inside a new pure wrapper
  `plan_journaled_command(app, outbox, journal, command, ids,
  defaults, now) -> Result<CommandPlan, CommandError>` that calls
  `plan_command`, computes pre-images + watermark, and appends the
  `RecordJournal` action to the returned batch. `plan_command` itself
  is not edited — the catalogue stays single-purpose, the wrapper is
  independently unit-testable, and the engine switches its one call
  site. Constraints (binding): pure, same batch, same transaction,
  pre-images from the pre-state only.
- **R-U9 — `MarkJournalPushed` emission: an engine-side post-pass
  between pipeline and `repo.apply`.** The pipeline functions
  (`apply_sync_report`/`apply_push_report`) own `AppState`, not the
  journal (which lives in `PersistedState`); widening their
  signatures would churn both pure functions for one flag. Instead a
  domain helper `journal_push_marks(&journal, &actions)` maps every
  `CompleteOp(op_id)` in a returned report batch to the owning
  entries (via each entry's `ops`) and the engine appends the
  idempotent `MarkJournalPushed` actions **before** `repo.apply` —
  the applied batch and the interpreted batch stay identical
  (single-advance rule holds), and the flag flip is crash-consistent
  with the completion it annotates (one sqlite transaction).
- **R-U10 — the line head is persisted state; undo survives restarts.**
  `PersistedState` gains `journal: JournalState { entries:
  Vec<JournalEntry>, head: Option<JournalId> }` — `head` is explicit
  (it moves on undo without any new node being written, so it cannot
  be derived) and persisted with the entries, making the undo line a
  durable, restart-surviving property — free from the journal being
  persisted, and worth stating as a v1 contract. Storage encoding of
  `head` (a flagged row vs a metadata row) is chunk-1 freedom.
- **R-U11 — retention is a deterministic apply-side eviction.** No
  new pruning action: when `apply_actions` handles `RecordJournal`,
  it evicts oldest-first **leaf** entries (no descendants via
  `parent`) beyond the cap (default **1000**) and older than the age
  floor (default **90 days**, judged against the incoming entry's
  stamp — deterministic, no `now` injection). A pruned line head
  reports `NothingToUndo` — graceful degradation (D-U5). Defaults
  only in v1; a `history.*` config key only if ever asked.
- **R-U12 — `remote_baseline` (DR-4 G) is observation, same D-U3
  discipline.** Stated in §3; recorded as a ruling so chunk agents
  treat it as covered, not novel: never rewound by undo, refreshed by
  the next cycle's normal report path.

## 5. Chunk specifications (the two implementation issues)

### Chunk 1 — journal substrate + inverse synthesis + `RestoreTask` (domain, storage, engine, tests) — size M+‡

‡ house scale: domain M, storage S, engine S, tests S.

**Domain** (`taskboard-domain`):

- New module `journal.rs` (pure data): `JournalId` (UUIDv7 via
  `IdGenerator`), `JournalEntry` (D-U1's shape: `id`, `parent`,
  `command: StateCommand`, `facts: UndoFacts`, `ops: Vec<OpId>`,
  `pushed: bool`, `board: Option<BoardId>`, `created_at`), `UndoFacts`
  (`PreImages { images: Vec<PreImage>, watermark: UndoWatermark }` |
  `NotUndoable(NotUndoableReason)`), `PreImage` (enum over
  Board/Stack/Task/Label whole entities), `UndoWatermark`
  (per-entity field-stamp maps), `JournalState` (R-U10),
  `UndoView { can_undo, can_redo, head_label, recent: Vec<UndoLabel> }`
  (bounded recent window, default 10).
- `PersistenceAction::RecordJournal(JournalEntry)` +
  `MarkJournalPushed(JournalId)`; `apply_actions` arms (R-U11
  eviction inside the record arm); `pre_images` +
  `journal_push_marks` pure helpers (R-U8/R-U9);
  `plan_journaled_command` wrapper.
- The **synthesis catalogue**: exhaustive `match` over `StateCommand`
  with no wildcard arm (§3's table is the classification; the match
  is the lockstep guarantee), emitting either a synthesized inverse
  `StateCommand` (re-planned through `plan_command` — typed
  rejections, no-op semantics, bound/unbound splits, cascade
  behaviour, op enqueue, nudge: all inherited) or a typed
  `NotUndoable`/refusal outcome.
- **`RestoreTask { id }`**: the three-case planner (pending delete op
  ⇒ `FailOp` retraction, resurrect locally, no new op; pushed ⇒
  resurrect with fresh clocks + create-class op — Deck undelete is
  re-POST, remote identity churnes, honestly; never-pushed unbound ⇒
  resurrect locally, no op). The resurrect write is `deleted = false`
  + fresh `clocks.deleted` — the R5 field shape.
- `StateCommand::Undo`/`Redo` + `CommandOutcome::Undone { entry }` /
  `Redone { entry }` + typed refusals (`NothingToUndo`,
  `TargetNotUndoable(reason)`, `TargetNoLongerInvertible { .. }`);
  `plan_undo` pure over `(state, outbox, journal, clock, ids)` — the
  line rule (walk `parent` once; new accepted mutation journals with
  `parent = head`; R-U6 refusal; R-U7 watermark check; the
  pushed-edit rule's two op behaviours per entry `pushed`).
- `AppState.undo: UndoView` derived from the journal at publish
  (memory-only, like `last_updated`).
- **ADR (next free number, expected 0017)** recording D-U1…D-U4 + the
  D-U5 scope as the decision, with a dated amendment note on ADR
  0006:104 ("no undo path" → the journal rides the single-advance
  rule; ordering, not rollback, remains the correctness story — the
  sentence is amended, never rewritten away).

**Storage** (`taskboard-storage-sqlite`): migration at the next free
number at landing time (contention with #481 — §2); `command_journal`
table (id TEXT PK, parent TEXT NULL, created_at TEXT, pushed INTEGER,
payload TEXT serde-JSON, board TEXT NULL) + the head encoding
(R-U10 freedom); codec in the house pattern; boot hydration: absent
table/empty rows ⇒ empty journal (upgrade-pinned test); retention is
domain-side (R-U11) — no storage pruning logic.

**Engine** (`taskboard-state`): switch `execute_command`'s planner
call to `plan_journaled_command`; the R-U9 post-pass in both report
ingestion paths before `repo.apply`; `Undo`/`Redo` ride the existing
`EngineEnvelope::Execute` — **no new envelope variant, no new channel,
no new actor**; publish path unchanged (`EngineSignal` stays
`Copy`-only — E-15 stays dodged).

**Tests** (§6): the make-or-break gate — chunk 1 is independently
green end-to-end (`Undo` works through `EngineHandle::execute`).

### Chunk 2 — surfaces (CLI/REPL, UI) — size M (UI M, CLI S)

- **CLI**: `tasks undo` / `tasks redo` verbs (D-U5's naming; awaited
  `execute` like every verb, so REPL and the daemon socket inherit
  per ADR 0012); receipts print the `Undone`/`Redone` entry label and
  typed refusals render as their typed outcomes (no text assertions
  in tests — structural outcomes only).
- **UI** (`taskboard-ui-slint`): flip `router.rs:317`'s
  `AppUndo | AppRedo` arm from `Disabled` to command dispatch;
  enable the registry entries (`actions.rs:367-383` + the
  enabled-set exclusions `:870-871, :959-960, :1023` + `menu.rs`);
  bind `UndoView` into the adapter's derived state; the toast labels
  (undo label, R-U6's refusal label, redo identity-churn copy that
  does **not** promise remote identity preservation — D-U5 risk (b)).
  Keymap: nothing to add — all three presets already bind the strokes
  (§2).
- **CHANGELOG** (both chunks): Unreleased entries — chunk 1 the
  engine verbs + restart-surviving undo line (user-observable via
  CLI in the same chunk), chunk 2 the UI affordances.

## 6. Test plan (AGENTS §5 discipline throughout)

- **Unit (domain, pure)**: per catalogue arm — plan against seeded
  state, capture the entry, plan undo, assert pre-image values
  restored / entity retracted, write clocks **fresh**, `remote_seen`
  (and, once landed, `remote_baseline`) unchanged; watermark
  refusal: interpose a later write ⇒ `TargetNoLongerInvertible`;
  `CreateStack`-occupied ⇒ its typed outcome; R-U6 refusal on a
  `NotUndoable` head; `matches!` on variants, never text.
- **Property (proptest)**: random invertible-command sequences over a
  generated `AppState`; undo-all ⇒ field-level pre-image restoration;
  the retraction class empties the outbox; the pushed class (flag
  pre-set) enqueues inverse ops; redo round-trips; watermark
  monotonicity (any interleaved write ⇒ refusal, never corruption).
- **Engine (insta)**: `Undo`/`Redo` join the command corpus; the
  journal + `UndoView` widening is the **one deliberate regeneration
  pass** over the 19 dump+tail pairs, reviewed as "the widening,
  nothing else".
- **Lockstep**: the synthesis catalogue's exhaustive match fails
  compilation on any new `StateCommand` arm (the #244 pattern) — the
  test *is* the type system.
- **Storage**: upgrade path (old DB ⇒ empty journal), round-trip
  codec, `MarkJournalPushed` idempotence (double-flip on multi-entry
  op sets — D-U5 risk (a)).
- **Determinism**: fixed/stepping injected clocks everywhere; no
  sleeps; poll-with-deadline in engine tests (existing `eventually`).
- **UI (chunk 2)**: router dispatch tests for the two enabled
  actions; `UndoView` binding in the adapter's derived-state tests.

## 7. Decision record — one recommendation each (this round)

| # | Question | Recommendation | Status |
|---|---|---|---|
| R-U6 | Line head is `NotUndoable`: refuse or skip? | **Refuse and name** (`TargetNotUndoable` + label toast); wrong-target undo is the worse failure class | proposed (this doc) |
| R-U7 | `TargetNoLongerInvertible` mechanism | **Post-write field-clock watermark** recorded per entry; stamp divergence ⇒ typed refusal; refuse-and-name, never force-apply | proposed (this doc) |
| R-U8 | Pre-image capture seam | **Pure post-plan pass** `pre_images(&app, &actions)` inside `plan_journaled_command`; `plan_command`'s catalogue untouched | proposed (this doc) |
| R-U9 | `MarkJournalPushed` emission | **Engine-side post-pass** `journal_push_marks` before `repo.apply`; pipeline signatures unchanged; same-batch crash consistency | proposed (this doc) |
| R-U10 | Journal head | **Persisted** `JournalState { entries, head }`; undo line survives restarts (stated v1 contract) | proposed (this doc) |
| R-U11 | Retention mechanics | **Deterministic apply-side leaf eviction** (cap 1000, age floor 90 d vs incoming stamp); no pruning action, no config in v1 | proposed (this doc) |
| R-U12 | `remote_baseline` interplay (DR-4 G) | Observation machinery — **never rewound**; D-U3 extends verbatim; no sequencing dependency on #481 | proposed (this doc) |

D-U1…D-U5 remain **accepted (2026-10-19)** and unamended by this
round. Session rulings 1-4 (round shape, v2 deferrals, issue shape,
handoff) user-confirmed 2026-10-26 at round entry.

## 8. Tracker mapping & completion bookkeeping

- **Chunk issues** (opened with this round): *undo v1 chunk 1 —
  journal substrate + inverse synthesis + RestoreTask* (`architecture`,
  `p2`) and *undo v1 chunk 2 — undo/redo surfaces (CLI, REPL, UI)*
  (`ui`, `dx`, `p2`) — each an implementation issue in the DR-chunk
  house shape (origin = this plan + branch), unscheduled (no
  milestone until the user schedules the wave).
- **#176** stays open as the umbrella: body updated to point at this
  plan and both chunks; it closes when both chunk issues land.
- **#353, #354, #355, #356, #357**: reaffirmation comments recording
  the 2026-10-26 ruling (stay deferred; needs-consideration notes
  stand). Two family widenings recorded in the comments: **#356**'s
  family grows `DeleteLabel` (restore-class verbs: stack, label);
  **#354**'s family grows `DismissReminder` + the adhoc-alarm pair.
  All five stay open, `p3`, unscheduled.
- **ADR 0006**: dated amendment lands in chunk 1 (not this doc's PR).
- **Follow-up dispatch**: per the house rule (dispatch plans chunk
  *accepted* work), a dispatch round for the two chunks follows this
  plan's acceptance — not part of this round.

## 9. Risks to name at dispatch

1. **Multi-entry op sets double-flip** (`DeleteStack`-class cascades
   completing across entries): `MarkJournalPushed` must be an
   idempotent flag write (D-U5 risk (a); pinned by a storage test).
2. **`RestoreTask` pushed-case identity churn**: re-POST ⇒ fresh
   remote card id; toast copy must not promise identity preservation
   (D-U5 risk (b)).
3. **Insta regeneration breadth**: 19 dump+tail pairs regenerate —
   one deliberate pass, diff-audited as "the widening, nothing else"
   (D-U5 risk (c), cheaper post-chunk-10).
4. **Migration-number contention with #481** (both claim the next
   free number): mechanical renumber, whichever lands second.
5. **Watermark false-refusal surface**: any later write to a touched
   field refuses the undo — including remote echo adoptions. Honest
   by construction (better refused than silently lossy), but the
   refusal rate on synced boards is a real-usage observation to
   carry into the #353–#356 family's future rulings.
6. **Journal memory/disk footprint**: whole-entity pre-images × 1000
   entries worst case — a few MB; within kiosk budgets (AGENTS §1),
   and the cap is the knob if measured otherwise.
7. **Concurrent ui-slint work** (e.g. the listview-virtualisation
   branch): chunk 2's small UI diff may need the AGENTS §8 rebase
   dance; chunk 1 is untouched by it.

## 10. References

- The binding architecture:
  `.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5, accepted 2026-10-19; §3.1 the journal shape, §3.2 the
  pushed-edit rule, §3.5 the v1 scope this round implements, §6 the
  chunking/test seed).
- Issues: #176 (umbrella), #353–#357 (v2 family, rulings §8), #349
  (closed design-round carrier), #481 (migration contention, R-U12),
  #244 (lockstep pattern), #128/#129/#130/#133 (DR-4 family context).
- ADRs: 0004 (LWW conflict policy — unchanged), 0005 (sqlite
  migrations/tombstones), 0006 (interpret loop — amended by chunk 1),
  0007 (push-first cycle, self-clobber guard), 0012 (REPL/daemon
  shared verb resolution), 0015 (reserved global actions).
- Audit inputs: F16-12/F16-13/F16-14
  (`.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`;
  arch-fit theme report §5.5).
- House context: `docs/testing_strategy.org` (§5 rules this plan's
  §6 follows); backlog-triage dispatch plan §14/§15 (the round's
  queue position); snapshot narrowing (its chunk 10) for the
  regeneration cost.
