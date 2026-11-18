# Chunk N-B Plan — The State Restore Family (#913 + #900 + #901)

Round: `2026-11-18-post-round9-parallel-dispatch-plan.md` §3 (normative) ·
Origin: `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
§Tier-1 N-B · Branch: `feat/state-restore-family` · Stage: 2 implements,
3 reviews. Engine-side only (`taskboard-state` + the domain message
types this plan names) — journey-plan exempt; no UI crate change; the
#353/#354 UI legs stay untouched; r9's H1 habit fields are disjoint
files, and stage-1 confirms symbol-level disjointness (r9 H1 touches
`Habit` additive fields in `messages.rs`/`command.rs` habit arms — this
chunk touches the `DeleteHabit` classification row, `RestoreHabit` as a
new appended arm, and `journal.rs`/`undo.rs` rows; the shared-file risk
is the rebase, R-6).

Base: C8 (`feat/state-restore-stack`, PR #916) + C9
(`feat/state-per-board-undo`) landed — the strict frontier rule stands
(scoped undo operates on the global head only), ceremony verbs stay
`NotUndoable(BoardCeremony)`, and the undo/journal snapshot corpus is
**extended, not rewritten** (law [L]c).

## 0. Current state of the world (verified at main `7aaa3452`)

- **#913's exact bug**: `command_class` maps
  `StateCommand::RestoreTask` into the `InverseClass::CreatedTask` row
  (`undo.rs:871`), whose synthesis finds only
  `PreImage::Task { before: None }` images via the `created_task`
  helper (`undo.rs:970-977`). A restore entry's pre-images are
  `before: Some` (the tombstoned task), so undoing a `RestoreTask`
  journal entry refuses `NothingToUndo`. `RestoreStack` already has
  its own `InverseClass::RestoredStack` row (C8 D-2/D-5,
  `undo.rs:1191-1225`) and does **not** exhibit the bug.
- **#900's gap**: `task_rule_deletes` (`command.rs:551`) emits
  `PersistenceAction::DeleteReminderRule(rule.id)` from the
  `DeleteTask` (1293), `DeleteStack` (1417) and `DeleteBoard` (1731)
  cascades and from `ClearReminderRules`/`RemoveReminderRule`. The
  `pre_images` capture (`undo.rs:483-544`) walks `Upsert*` and
  `RecordHabitLog` actions only — `DeleteReminderRule` hits the
  `_ => continue` arm. Deleted rules carry no journal pre-image (D19),
  so every restore-class resurrection brings tasks back **without
  their reminder rules**. C8 pinned this as documented leniency
  (`restore_stack_currently_pins_the_reminder_rule_loss`,
  `undo/tests.rs:3073`); #900 un-pins it.
- **#901's targets**: `DeleteLabel` is journaled
  `NotUndoable(RestoreClass)` (`undo.rs:219-221`);
  `DeleteTimeEntry` is journaled `NotUndoable(TrackingSurface)`
  (`undo.rs:320-324`); `DeleteHabit` is journaled
  `NotUndoable(HabitSurface)` (`undo.rs:340-343`). Note the recorded
  drift: `NotUndoableReason::RestoreClass`'s doc comment
  (`journal.rs:97-101`) names all three verbs, but only `DeleteLabel`
  actually carries the variant — the doc drift is corrected by this
  chunk (D-2), not a separate bug.
- Planners (verified): `DeleteLabel` tombstones the label + stamps
  `Removed` memberships on every live member task (entity-death
  dominance) + bound/unbound op split (`command.rs:1493-1545`);
  `DeleteTimeEntry` is a hard row delete — `PersistenceAction::DeleteTimeEntry`
  removes the row, plus an `UpdateAppData(task)` flush enqueue
  (`command.rs:2624-2639`); `DeleteHabit` is tombstone-only, log rows
  untouched, deletion roams the board flush (D-H9)
  (`command.rs:2804-2826`).
- `RestoreTask { id, resurrect: Box<Task> }` is the payload-carrying
  restore-verb precedent (`command.rs:3179` `plan_task_restore`,
  three-case: pending-op retraction / pushed re-POST as `CreateTask` /
  local resurrect). `AddReminderRule` **mints a fresh rule id** and
  normalizes its payload (`command.rs:2222-2303`) — it is *not* a
  verbatim resurrect path (D-3 rules this out).
- `EntityStamps` (`journal.rs:211-226`): `Task { clocks, memberships }`,
  `Stack`, `Board`, `Label`, `Habit`, `TimeEntry(DateTime<Utc>)`,
  `HabitLog` — every entity kind this chunk needs already has a stamp
  variant. `JournalEntity` (`journal.rs:196-206`) is declaration-ordered;
  new kinds append last.
- The engine (`taskboard-state`) plans through `plan_journaled_command`
  — no engine-side planner change expected (C8/C9 precedent, confirmed
  in `crates/taskboard-state/tests/undo.rs`).

## 1. Recorded decisions

### D-1 — #913 semantics: undo of a restore re-deletes via the recorded inverse; no new refusal

**Decision: re-delete.** Undoing a restore-class journal entry
(`RestoreTask` today; `RestoreLabel`/`RestoreTimeEntry`/`RestoreHabit`
under #901) synthesizes the ordinary delete command
(`DeleteTask { id }`, `DeleteLabel { id }`, `DeleteTimeEntry { id }`,
`DeleteHabit { id }`) — the `RestoredStack` → `DeleteStack` precedent
(C8 D-5), generalized. **Not** a typed refusal that is not
`NothingToUndo`, because:

1. The journal already holds the facts (the restore's own
   `PreImages`); refusing would make a restore a one-way door,
   contradicting the family's own design (C8 D-5 recorded undoing a
   restore as re-delete through the cascade).
2. A refusal variant would be indistinguishable in intent from
   `TargetNotUndoable`-family noise; the honest typed refusals this
   machinery needs already exist (`TargetNoLongerInvertible`,
   `UndoTargetHasDependents`) and are what divergence produces.

Mechanics: a new `InverseClass::RestoredTask` row;
`RestoreTask` **leaves** the `CreatedTask` row (the one-line #913 fix —
the `created_task` helper then matches only true creations again).
The `RestoredTask` synthesis (`synthesize_undo`, mirroring
`RestoredStack` `undo.rs:1191-1225`):

- Find the entry's `PreImage::Task { before: Some(_), .. }` — absent is
  `NothingToUndo` (an empty/incomplete entry, the defensive arm).
- Checked scope: the task row still live (`!current.deleted` — a
  re-tombstone since refuses, field `Deleted`), then the **full
  content-stamp array** against the entry's own watermark (title,
  description, duedate, scheduled, done, position, effort, importance,
  urgency, recur, kind, archived — the `CreatedTask` array verbatim,
  `undo.rs:1019-1036`). Any later edit refuses field-named,
  whole-line, zero actions. The push echo's `remote`/`remote_seen`
  adoption never refuses (never watermarked, D-U3).
- Inverse: `StateCommand::DeleteTask { id }`. No occupancy guard —
  `DeleteTask` cascades only the task's own rules/tracking closes,
  which is the honest content of the undo. (The `DeleteStack`-shaped
  occupancy guard in `plan_undo` keys on `DeleteStack` inverses only;
  `RestoredStack` keeps its guard unchanged.)

The generalization to the three new restore verbs' rows is D-4's table.

### D-2 — #901 classification: all three deletes become `PreImages`; `RestoreClass` retires unassigned

`record_entry` reclassification (three rows leave the `NotUndoable`
families; no other arm migrates — the ceremony fence and the
reminder/reflection/tracking-lossiness fences hold verbatim):

| Command | Before | After |
|---|---|---|
| `DeleteLabel` | `NotUndoable(RestoreClass)` | `PreImages` |
| `DeleteTimeEntry` | `NotUndoable(TrackingSurface)` | `PreImages` |
| `DeleteHabit` | `NotUndoable(HabitSurface)` | `PreImages` |

`NotUndoableReason::RestoreClass` becomes **unassigned**: keep the
variant (serde stability — persisted journals that recorded it decode
unchanged and keep refusing with their recorded reason, the C8 D-8
precedent verbatim), correct its doc comment to say so. The
`TrackingSurface`/`HabitSurface` doc comments drop their
`DeleteTimeEntry`/`DeleteHabit` mentions. `journal.rs`'s stale
"restore-class = all three verbs" doc block is rewritten to the
post-chunk truth (in-scope doc correction — the drift discovered in §0).

The `command_label` table gains "Restore label" / "Restore time entry" /
"Restore habit" rows for the new verbs (D16 lockstep; presentation-only).

### D-3 — #900 pre-image storage shape: `PreImage::ReminderRule`, captured from the same plan, resurrected action-level

- **Capture**: `PreImage::ReminderRule { id: ReminderRuleId,
  before: Option<Box<ReminderRule>> }` — new variant, appended to
  `PreImage` (journal.rs). `pre_images` gains the
  `PersistenceAction::DeleteReminderRule(rule_id)` arm reading
  `before` from `app.reminders.rules` (the pre-state snapshot — the
  same `app` the plan was built from, so the image cannot diverge from
  what the cascade actually deleted: capture and cascade read one
  plan/one snapshot, the stage-3 "same transaction/apply path" trap).
  `before: None` is unreachable for a real cascade (the rule existed)
  but kept for the enum's `before` convention (the `HabitLogRow`
  precedent, `journal.rs:163-166`).
- **Watermark**: none. Rule deletes carry no clock stamps (D19 — the
  row is removed, not tombstoned); `JournalEntity::ReminderRule`
  enters the **`JournalEntity` enum (appended last, declaration-order
  `Ord` never reshuffles)** but no `EntityStamps::ReminderRule`
  variant is added. The resurrect's divergence guard is the owning
  task's own watermark (rules ride the task), not a rule stamp.
- **Resurrect write (decision: action-level append, not a
  command)**: `plan_undo`, after the inverse-batch planning loop, for
  each `PreImage::ReminderRule { before: Some(rule) }` in the entry's
  images: if the rule row is **absent** from `app.reminders.rules`,
  append `PersistenceAction::UpsertReminderRule(rule.clone())` +
  `EnqueueOp(UpdateAppData(rule.task))` (the flush trigger — rules
  ride the document lane; the delete enqueued the same trigger, the
  restore mirrors it); if the row is **present**, skip. Skip-not-refuse
  is deliberate: ids are minted once (`AddReminderRule` never re-mints
  an existing id), so a present same-id row can only exist if a prior
  resurrect placed it — skipping gives the head-move idempotency the
  redo path needs. The payload is **verbatim** (same id, same
  `created_at`, same stamps): `AddReminderRule` is ruled out because
  it mints a fresh id and re-stamps (verified §0), which would forge a
  different stored fact.
  - Placement rationale: rule resurrection is not a user intent (no
    surface verb exists), so it is not a `StateCommand`; it rides the
    one journal entry whose facts recorded the deletes, after the
    inverse commands planned through the catalogue. The appended
    actions sit inside the same `MoveJournalHead`-guarded batch — one
    undo, one head move, all-or-nothing at the apply layer.
  - Scope of effect: this resurrect fires for **every**
    `PreImages`-classified entry that cascaded rule deletes —
    `DeleteTask` and `DeleteStack` entries today, the three #901
    deletes if they ever cascade rules (they do not), and the
    restore-class entries' own undo (which re-deletes, re-captures,
    and re-resurrects on the next undo — the round trip is honest).
    `RemoveReminderRule`/`ClearReminderRules`/`DeleteBoard` cascade
    sites gain the *capture* mechanically but never journal facts
    (ReminderSurface / BoardCeremony), so no behaviour change there.
- **Un-pinning**: `restore_stack_currently_pins_the_reminder_rule_loss`
  (`undo/tests.rs:3073`) is deleted — the debt it pins is paid. Its
  replacement is the positive test in §3 (catalogue item 4). This is
  the one corpus *removal* this chunk costs; it is a
  `..._currently_pins_...` debt pin, not a contract snapshot (AGENTS §5
  no-pins rule), so deleting it is not a corpus rewrite.
- **Deliveries/snooze links stay pruned**: `DeleteReminderRule`'s apply
  prunes deliveries and snooze links (bounded growth, by design); a
  resurrect does not forge delivery proofs back into existence. The
  resurrected rule re-arms future occurrences; the past stays settled.
  Recorded here as correct-by-design, pinned by the catalogue.

### D-4 — #901 verb semantics: three forward verbs, payload-carrying where the row is gone

New `StateCommand` arms, **appended** (serde position discipline,
C8 R-3):

```rust
RestoreLabel { id: LabelId },
RestoreTimeEntry { entry: Box<TimeEntry> },   // payload-carrying — D-4c
RestoreHabit { id: HabitId },
```

**D-4a — `RestoreLabel { id }`** (tombstone survives, id-only verb,
`RestoreStack` shape):

- Forward planner three cases over `plan_task_restore`'s template:
  pending `DeleteLabel` op → `FailOp` + upsert (retraction); pushed/
  bound → upsert + enqueue `LocalOp::CreateLabel(id)` (identity churn
  is the honest price, the D-U2 precedent); otherwise local upsert.
  The resurrected label keeps its pre-tombstone values verbatim; a
  fresh `deleted` clock per the R5 merge-field shape; a live target is
  the accepted `NoOp` (terminal-repeat discipline); `UnknownLabel`
  covers absence.
- **Leniency (recorded)**: the forward verb resurrects the label
  entity only. The entity-death-dominance removals (stamped
  `Removed` memberships) stay removed — each membership's recovery
  surface is the ordinary `AssignLabel` verb (the OR-set re-add). The
  *undo* inverse (below) restores the memberships that the entry's own
  delete removed; the forward verb does not guess.
- Typed outcome: `CommandOutcome::LabelRestored { label: LabelId }`
  (new variant, appended).

**D-4b — `RestoreHabit { id }`** (tombstone-only delete, id-only verb):

- Forward planner: tombstoned → live, pre-tombstone values verbatim,
  fresh `deleted` clock; live target `NoOp`; `UnknownHabit` typed. The
  log is never touched (D26 — history outlives the tracker; the
  resurrection-safe LWW note in the `DeleteHabit` planner carries
  verbatim). No three-case split: `DeleteHabit` roams the board flush
  (D-H9) and enqueues no per-habit op — the restore is
  `UpsertHabit` + `board_flush`.
- Typed outcome: `CommandOutcome::HabitRestored { habit: HabitId }`.
- r9 H4 composition: the machinery assumes no UI arm — the verbs are
  plain `StateCommand`s any future surface dispatches; no surface work
  here (round constraint).

**D-4c — `RestoreTimeEntry { entry: Box<TimeEntry> }`** (the row is
gone — hard delete, so the payload rides the command, the
`RestoreTask { id, resurrect }` precedent):

- Forward planner: absent-or-identical row → upsert the payload
  verbatim + enqueue `LocalOp::UpdateAppData(entry.task)` (mirroring
  the delete's flush shape); a **present different** row with the same
  id is `CommandError::TimeEntryConflict`-shaped refusal — new typed
  variant `TimeEntryAlreadyPresent(TimeEntryId)` (ids are minted once,
  so this is the "already restored / externally forged" divergence;
  refuse, never clobber). `UnknownTask` covers a gone owner task.
  No pushed/pending split: the delete enqueued only an
  `UpdateAppData` flush; the restore enqueues the same trigger —
  document-lane id preservation means no identity churn.
- Typed outcome: `CommandOutcome::EntryRestored { id: TimeEntryId }`.
- Direct dispatch with an arbitrary payload is honest: the planner
  stamps nothing, restores verbatim; the undo path synthesizes it from
  the journal pre-image (D-5), which is the only sanctioned synthesizer.

### D-5 — The #901 inverse-synthesis rows (undo of the three deletes) and the restore-verbs' own rows

New `InverseClass` rows (the synthesis table's other half; one row per
arm, the lockstep):

| Entry (command) | `InverseClass` | Synthesis |
|---|---|---|
| `DeleteLabel` | `DeletedLabel` | watermark: label's `deleted` clock == recorded stamp and row present & tombstoned (else `TargetNoLongerInvertible { Label, Deleted }`); per member task in pre-image order whose pre-image membership was `Added` for this label: the task's watermark **membership entry for that label only** compared against current (covered-field discipline — an unrelated membership edit must not refuse); synthesize `RestoreLabel { id }` + N × `AssignLabel { task, label }` (label first — a membership on a not-yet-live label would plan `UnknownLabel`). |
| `DeleteTimeEntry` | `DeletedTimeEntry` | guard: the row absent from `app.tracking.entries` (present → `TargetNoLongerInvertible { TimeEntry, Deleted }` — a present same-id row is divergence, ids mint once); the pre-image payload is the resurrect source: synthesize `RestoreTimeEntry { entry: pre.clone() }`. |
| `DeleteHabit` | `DeletedHabit` | watermark: habit's `deleted` clock == recorded stamp and still tombstoned; synthesize `RestoreHabit { id }`. |
| `RestoreTask` | `RestoredTask` (D-1) | `DeleteTask { id }` under the full content-stamp scope. |
| `RestoreLabel` | `RestoredLabel` | `DeleteLabel { id }`; checked scope: still live + title/color stamps (the `CreatedLabel` scope shape, `undo.rs:1276-1299`). |
| `RestoreTimeEntry` | `RestoredTimeEntry` | `DeleteTimeEntry { id }`; checked scope: row present and its `updated_at` stamp == the entry's watermark stamp (the `CreatedTimeEntry` scope, `undo.rs:1367+`). |
| `RestoreHabit` | `RestoredHabit` | `DeleteHabit { id }`; checked scope: still live + title/target/order stamps (the `CreatedHabit` scope shape). |

The restore verbs journal `PreImages` of their own writes (the
mechanical capture), so undoing a restore re-deletes (D-1) and redo
re-plans the forward intent (D5 greatest-child rule unchanged). The
D-3 rule-resurrect rides `DeleteTask`/`DeleteStack` entries — including
the one synthesized by undoing a restore, closing the loop.

Pushed-edit rule: no new code (C8 D-3's composition argument verbatim —
the blanket retraction plus the per-verb pending-op checks compose).

`touched_board` gains arms: `RestoreLabel`/`DeleteLabel` via the label
→ board chain; `RestoreHabit`/`DeleteHabit` via habit → board;
`RestoreTimeEntry`/`DeleteTimeEntry` via entry → task → stack → board
(best-effort, `None` when the chain is gone — the D-3 honesty of C9
holds). The board-ceremony verbs are untouched.

### D-6 — Ceremony and surface fences hold; scope of churn

The only `NotUndoableReason` migrations are D-2's three rows.
`StartTracking`/`StopTracking` (structural inexpressibility),
`LogHabit`/absent-window `AdjustHabitLog` (lossiness), the reflection
family (D-J7), the conversion composites (D25), and every
`BoardCeremony` arm stay exactly as landed. `AdjustTimeEntry`'s
closes-open-entry refusal is unchanged — the hard row delete
(`DeleteTimeEntry`) is a different verb and stays restore-class.
Stage-3 trace trap: grep the diff for every `record_entry` /
`command_class` row move and verify it is one of D-2's three plus the
new arms.

### D-7 — Persisted-journal compatibility

No `JournalEntry`/`JournalState` shape change beyond the appended
`PreImage::ReminderRule` variant and the appended
`JournalEntity::ReminderRule` key (both additive to serde enums — old
payloads decode unchanged; new payloads with the variants refuse on
old binaries, the accepted forward-only posture). The three new
`StateCommand` arms and the new `CommandOutcome` variants are appended.
A serde round-trip test pins the new shapes (catalogue item 12).

## 2. Implementation map (stage 2)

| File | Change |
|---|---|
| `crates/taskboard-domain/src/messages.rs` | Three appended `StateCommand` arms (D-4); four appended `CommandOutcome` variants (`LabelRestored`, `HabitRestored`, `EntryRestored` — plus none needed for #913, its inverse reuses `Applied` via `DeleteTask`). |
| `crates/taskboard-domain/src/journal.rs` | `PreImage::ReminderRule` variant; `JournalEntity::ReminderRule` key (appended last); `NotUndoableReason::RestoreClass` doc correction (retired-unassigned) + `TrackingSurface`/`HabitSurface` doc drops; `command_label` rows; `EntityStamps` unchanged. |
| `crates/taskboard-domain/src/command.rs` | `RestoreLabel`/`RestoreTimeEntry`/`RestoreHabit` planner arms + `plan_task_restore`-style helpers where the three-case shape repeats; new typed error variant `TimeEntryAlreadyPresent`; `command_label` outcome wiring. |
| `crates/taskboard-domain/src/undo.rs` | `record_entry` reclassification (D-2); `pre_images` gains the `DeleteReminderRule` capture (D-3); `command_class`: `RestoreTask` → new `RestoredTask` row, `DeleteLabel`/`DeleteTimeEntry`/`DeleteHabit` + three restore verbs → D-5 rows; `synthesize_undo` arms (D-1/D-5); `plan_undo` gains the rule-resurrect append (D-3) and `touched_board` arms (D-5). |
| `crates/taskboard-state/src/*` | No expected change (engine plans through `plan_journaled_command`); confirm no exhaustive match on `PreImage`/`InverseClass` outside the domain needs a new arm. |
| tests | §3 catalogue: `undo/tests.rs` (domain harness), `crates/taskboard-state/tests/undo.rs` (engine mirrors), `tests/proptest.rs` (domain proptest module), serde round-trip tests. |

UI crates, the CLI caller (`taskboard-app/src/commands/*`), storage,
sync: **no change**. The three new verbs are engine machinery; their
dispatch surfaces are filed backlog work (see the issue list in the
final report).

## 3. Test catalogue

Snapshots / domain tests (extend the corpus, never rewrite — law
[L]c; the one deletion is D-3's paid debt pin):

1. `undo_of_restore_task_re_deletes_through_delete_task` — the #913
   scenario red-first: restore a locally deleted task, undo the
   restore → task tombstoned again through the ordinary `DeleteTask`
   cascade, head moved, entry classified `PreImages`.
2. `undo_of_restore_task_refuses_field_named_when_edited_since` — the
   full content-stamp scope: any post-restore content edit refuses
   `TargetNoLongerInvertible` with its field, zero actions.
3. `undo_of_restore_task_refuses_when_re_tombstoned_since` —
   `TargetNoLongerInvertible { field: Deleted }` (and the guard is not
   the occupancy guard — pinned structurally by the absence of
   `UndoTargetHasDependents`).
4. `undo_of_delete_stack_resurrects_the_cascaded_reminder_rules` — the
   #900 un-pin: delete a stack with N rules, undo → every rule row
   back **verbatim** (same id, same `created_at`), the flush trigger
   enqueued per rule's task, deliveries still pruned.
5. `undo_of_delete_task_resurrects_its_rules_and_a_rule_readded_since_is_skipped` —
   the skip-not-refuse guard (D-3): a same-id rule present at undo
   time is not clobbered.
6. `delete_label_journals_pre_images_and_undo_restores_label_and_memberships` —
   D-5 `DeletedLabel`: label live again, the member tasks' memberships
   re-`Added` through `AssignLabel`, head moved.
7. `undo_of_delete_label_refuses_when_a_member_membership_diverged` —
   only the deleted label's own membership stamp is checked; an
   unrelated label's membership edit does not refuse; an edit to the
   checked membership does (covered-field discipline).
8. `restore_label_on_a_live_label_is_the_accepted_noop` +
   `restore_label_of_an_unknown_id_is_typed` — terminal-repeat /
   typed-absence pair (the `RestoreTask` pair's mirror).
9. `restore_label_then_undo_re_deletes_and_redo_re_restores` — the
   composition round trip, both directions.
10. `delete_time_entry_journals_pre_images_and_undo_restores_the_row_verbatim` —
    D-5 `DeletedTimeEntry`: the row returns byte-identical (id,
    task, start/end, description), flush trigger enqueued.
11. `undo_of_delete_time_entry_refuses_when_a_row_reappeared` — the
    present-row guard.
12. `restore_time_entry_round_trips_and_conflicts_typed` — the forward
    verb restores a journaled payload; a direct dispatch onto a
    present different row refuses `TimeEntryAlreadyPresent`; unknown
    owner task is `UnknownTask`.
13. `delete_habit_journals_pre_images_and_undo_restores_the_tombstone_target` —
    D-5 `DeletedHabit`: habit live again, log rows untouched (D26
    pinned inside the test's assertions as structural facts — log map
    equality), fresh deleted clock.
14. `restore_habit_on_a_live_habit_is_the_accepted_noop` +
    `restore_habit_then_undo_re_deletes_through_delete_habit`.
15. `every_arm_journals_entry_or_nothing` — the lockstep table gains:
    `DeleteLabel`/`DeleteTimeEntry`/`DeleteHabit` → PreImages, three
    restore verbs → PreImages (extension, not rewrite; law [L]c).
16. `not_undoable_heads_refuse_and_name_the_reason` — extended for the
    retired `RestoreClass` decode path: a hand-built entry carrying
    `NotUndoable(RestoreClass)` still refuses with that reason
    (persisted-compat D-7).
17. Serde round-trip: journal entries carrying `PreImage::ReminderRule`,
    the three new arms, and the new outcome variants decode
    byte-stably (D-7).
18. `label_habit_and_time_entry_restore_entries_carry_the_board_tag` —
    the D-5 `touched_board` arms (C9 D-9 seam law).
19. Engine mirrors (`crates/taskboard-state/tests/undo.rs`):
    `engine_undo_of_restore_task_flips_the_published_state_and_the_persisted_journal`
    and `engine_undo_of_delete_stack_resurrects_rules_in_the_persisted_state`
    (mirror of the existing engine undo test over the new paths —
    proves the action-level resurrect reaches sqlite).

Proptests (pure domain, generated fixtures):

- P1 `restore_undo_round_trips_never_lose_fields`: for a generated
  task/label/habit/time-entry, delete → undo (restore) → undo (re-delete)
  → redo … leaves every content field at its pre-delete value modulo
  the documented fresh-clock fields (the C8 P2 shape generalized).
- P2 `rule_resurrect_restores_exactly_the_cascaded_set`: over a
  generated stack with N tasks × M rules, undo of the delete leaves
  exactly the rules the cascade removed — no more, no fewer —
  byte-identical.
- P3 `ceremony_and_surface_fences_hold`: generated
  `BoardCeremony`/reminder-surface/reflection verbs never journal
  `PreImages` (the C8 P3 property re-proven over the migrated table).
- No sleeps/pauses: planner-level, injected clock seams per the
  existing harness (law [L]b).

## 4. Acceptance (stage 3 gate)

- Typed outcomes only; no text assertions; no `RwLock`/`Mutex`.
- The #913 scenario is pinned by item 1 and the fix handles the whole
  restore-class outcome set (D-1/D-5's table is the enumeration; stage
  3 traces each row).
- #900's pre-image rides the same plan/snapshot as the cascade (§0
  argument); stage 3 checks the capture site reads the same `app`.
- #901's three verbs each have restore + undo round-trip tests; the
  `RestoreHabit` machinery assumes no UI arm (D-4b).
- Corpus delta: new tests only + the one debt-pin deletion (D-3) +
  the lockstep-table extension; any *changed* existing snapshot is a
  recorded diagnosis, not a refresh.
- Diff confined to `taskboard-domain` (the named files) +
  `taskboard-state` tests; no UI crate import; no storage migration.
- Gates: pre-commit hooks + `scripts/run-quality-gates.sh cargo
  nextest run --workspace` (defer per the #879/#894 flock rule if
  contended; the dispatcher owns the full §10 suite and CI).

## 5. Risks

- **R-1 (action-level resurrect vs catalogue discipline)**: the D-3
  append bypasses `plan_command` for rule rows. Mitigation: the append
  is guarded, verbatim, and inside the head-move batch; the engine
  mirror test (item 19) proves the apply path end-to-end; stage 3
  reviews the append site for ordering (after the inverse commands, so
  a resurrect of a rule whose owning task's restore refused never
  fires — the `?` on planning keeps all-or-nothing).
- **R-2 (membership-scope check drift)**: `DeletedLabel`'s per-label
  membership check must not silently widen to whole-map comparison
  (it would over-refuse). Pinned by item 7's negative case.
- **R-3 (`RestoreTimeEntry` payload trust)**: a payload-carrying verb
  is a wider surface than an id-only one. The planner stamps nothing
  and validates structurally (`UnknownTask`, the conflict guard);
  the journal is the only sanctioned synthesizer. Recorded in D-4c.
- **R-4 (`RestoreClass` retirement visibility)**: an unassigned
  variant can look like dead code. The doc correction + item 16's
  decode test make it contract (serde stability), not leftover.
- **R-5 (rebase onto r9 H1)**: `messages.rs`/`command.rs` habit arms
  are r9-disjoint by symbol but shared by file; stage 2 opens with
  `git fetch --prune; git rebase origin/main` and re-runs the gate
  after the rebase; serde-arm positions are append-only on both
  sides, so the merge risk is textual, not wire-shape.
- **R-6 (time-entry stamp coarseness)**: `EntityStamps::TimeEntry` is
  a single `DateTime<Utc>` — the checked scope cannot name a diverged
  *field*. Accepted: time entries carry one mutation stamp
  (`updated_at`); the refusal names the entity, which is the honest
  granularity the stamp admits.

## 6. Estimated diff

~950–1200 lines: domain ~300 (capture variant, seven synthesis rows,
three planners, reclassification, doc corrections), tests ~600–850
(19 domain/engine items + 3 proptests + serde round-trips),
storage/sync/UI zero.
