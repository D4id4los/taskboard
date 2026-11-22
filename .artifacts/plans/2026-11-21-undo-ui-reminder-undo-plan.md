# UX Journey Plan — undo v2, chunk UF1: reminder-rule verbs become undoable

<!--
Copied from .artifacts/templates/ux-journey-plan.md; guidance comment
retained per the template's own instruction. This is the round-4
undo-v2 UI family's chunk plan for UF1 (`feat/domain-reminder-undo`,
issue #354) — one of the two per-chunk journey plans the J3 agent
lands (the granularity decision is recorded in §1). The sibling plan
is 2026-11-21-undo-ui-branch-picker-plan.md (UF2, #353).
-->

- **Date**: 2026-11-21
- **Kind**: UX journey plan — user stories, journeys, flows, wiring
  contract for making the reminder-rule verbs undoable (UF1). No
  implementation decisions beyond the command contract.
- **Origin**: the undo architecture design plan
  (`.artifacts/plans/2026-10-19-undo-architecture-design-plan.md`)
  §3.5 (out-of-scope v1 note) + §8 candidate 2 → issue #354; the
  landed undo base (the journal tree, `plan_undo`/`plan_redo`, the
  `PreImage::ReminderRule` capture + the #900 rule-resurrect path in
  `synthesize_undo`, `NotUndoableReason::ReminderSurface`); the round-4
  scope report §A3; the round-4 dispatch plan J3/UF1 rows (traps a–c).
  **Authority note (recorded decision)**: the 2026-10-19 decision
  session parked #354 as "needs further consideration"; the operator
  **reaffirmed #353/#354 into round 4 on 2026-11-21** (recorded on the
  round-4 dispatch plan's header) — this plan is written under that
  reaffirmation, which supersedes the parking note.
- **Status**: DRAFT — operator gate pending.
- **Surfaces touched**: no new surface — TaskDetail rule rows
  (`task-view.slint` / `rule-list.slint`, `ReminderRuleAdd` /
  `ReminderRuleRemove` `actions.rs:344-345`) and the global undo line
  (`AppUndo`/`AppRedo`) already exist; the delta is that their
  composition now works. `command_label` gains three arms
  (presentation).
- **Platform scope**: desktop and kiosk alike — the undo line is a
  global reflex on both (census `app.undo` row: "E (global)" on every
  surface); no kiosk gating (the verbs' existing entry points keep
  their platform qualification untouched).
- **O-rulings**: the round's standing operator rulings are authority
  where they bind: O-2 (additive-only domain work — UF1 adds
  classification arms and reuses `PreImage::ReminderRule`; no new
  entity shape, no journal migration) and O-5 (CLI parity — the CLI
  already owns `undo`; no new CLI verb is planned here, ruled in §2
  R-4). The UF1 dispatch traps bind verbatim: (a) delivery proofs and
  snooze links are NEVER retractable — a synthesis entry that would
  retract a delivery is a shipping blocker; (b) the catalogue entries
  follow the N-B precedent — `PreImage::ReminderRule` exists, extend
  it, don't duplicate; (c) the DeleteBoard cascade interplay stays
  #357's vehicle per #980's recorded fact.

## 1. Scope & goals

**What the user can now do that they couldn't**: edit a task's
reminder rules (add / remove) and then press the global undo reflex
(`AppUndo`, Ctrl+Z) to restore the exact pre-edit rule set — same ids,
same `created_at`, same stamps — instead of hand-re-adding rules and
eating fresh identity churn. Removing a rule becomes as recoverable as
deleting a task. Undoing an undo (redo) re-applies the rule edit.

**Granularity decision (recorded, per the framework's call at stage
1)**: the scope report §A3 suggested "one journey plan, #354 first";
the dispatch plan's J3 row says "journeys + 2 chunk plans". Decision:
**two per-chunk journey plans** (this file; UF2's sibling), not one
family plan — UF1 is a domain-side catalogue reclassification with a
near-zero surface delta (no new affordance anywhere), while UF2 is a
full new UI surface; a merged plan would force one gate root to carry
both an engine-catalogue verification set and a UI journey set, and
the chunks are independently dispatchable (UF1 → UF2 order per the
dispatch plan's §1, but neither needs the other's stories).

**Goals**:

- G1: `AddReminderRule`, `RemoveReminderRule`, `ClearReminderRules`
  reclassify from `NotUndoable(ReminderSurface)` to `UndoFacts::PreImages`
  (the arm-agnostic capture already walks `UpsertReminderRule` /
  `DeleteReminderRule` actions — the `PreImage::ReminderRule` row has
  existed since #900).
- G2: `synthesize_undo` gains the three inverse arms: the remove/clear
  inverse rides the existing verbatim rule-resurrect path (no fresh
  id, no re-stamp — the #900 D-3 law); the add inverse is
  `DeleteReminderRule`.
- G3: `command_label` arms so the undo toast / head label names the
  reminder edit ("Add reminder rule" / "Remove reminder rule" /
  "Clear reminder rules" — presentation text, never test-asserted).
- G4: the pushed-edit rule (D-U2) composes unchanged: an unpushed
  rule edit retracts (`FailOp`); a pushed one is answered with the
  inverse enqueue (document-lane flush mirrors the delete's own
  enqueue — already implemented in the resurrect path).

**Non-goals (each with a home)**:

- N1: mute undo (`SetTaskRemindersMuted`) — ruled out, §2 R-1; home:
  backlog issue (filed at plan acceptance; see §9).
- N2: snooze / dismissal / ad-hoc alarms — delivery facts, never
  retractable; **no home needed** (the design plan's hard constraint;
  not debt, a law).
- N3: break rules (`CreateBreakRule`/`DeleteBreakRule`) — out of this
  chunk's mandate (#354 names the D-R1 rule verbs); home: this plan's
  §2 R-2, follow-up only if the operator asks.
- N4: board-cascade restore interplay (#980) — `DeleteBoard`
  restore stays #357's vehicle; the dispatch trap (c). Home: #357.
- N5: a reminder-rules CLI verb for `ClearReminderRules` — the verb
  has no surface today (§2 R-3); giving it one is a separate
  capability, not undo work. Home: §2 R-3.

## 2. Capability × surface matrix

<!-- Fixed header per the template. Cells: D display, E edit, C create,
X remove + story ID; — = deliberately absent. -->

| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |
|---|---|---|---|---|---|---|---|
| reminder_rule.add | — (R-6) | E (pre-existing; census `task.reminder_rules_edit`) + US-1 undo coverage | — (R-6) | — (R-6) | — (R-6) | — (R-6) | E (pre-existing, `tasks remind`) |
| reminder_rule.remove | — (R-6) | E (pre-existing) + US-2 undo coverage | — (R-6) | — (R-6) | — (R-6) | — (R-6) | E (pre-existing, `tasks remind`) |
| reminder_rule.clear | — (R-6) | — (R-3) | — (R-3) | — (R-3) | — (R-3) | — (R-3) | — (R-3) |
| reminder_rule.mute | — (R-6) | E (pre-existing) — stays NotUndoable (R-1) | — (R-6) | — (R-6) | — (R-6) | — (R-6) | E (pre-existing) |
| journal.undo (of rule edits) | E (US-1, US-2 — global `AppUndo`) | E (US-1, US-2) | E (US-1, US-2) | E (US-1, US-2) | E (US-1, US-2) | E (US-1, US-2) | E (pre-existing, `undo` — same coverage) |

**Rulings** (every deliberate absence):

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | `SetTaskRemindersMuted` stays `NotUndoable(ReminderSurface)` | Mute is a one-click toggle at the same seam that set it (self-healing, the SC-W-N4 doctrine); un-muting restores nothing that was a delivery — but its undo value is ~nil while its catalogue row would add a dynamic rule for nothing. The #354 body's "possibly mute" is resolved negatively by this plan | Backlog issue (filed at acceptance) |
| R-2 | Break rules stay `NotUndoable(ReminderSurface)` | Out of #354's mandate (D-R1 task-rule verbs); a break-rule edit is already escaped by the staged-remove arming grammar (census `break_rule.remove` row, T-14). Revisit only with a motivation | This plan §1 N3 |
| R-3 | `ClearReminderRules` has no first-class surface today and this plan adds none | The verb exists in the domain (messages.rs:277) with zero entry points; its catalogue entry lands anyway (the synthesis match is exhaustive — the lockstep compiles it), so a future surface inherits undoability for free. Adding a clear-all affordance is a separate UX capability, not undo work | Backlog issue (filed at acceptance) |
| R-4 | No new CLI verb; the existing `undo` covers rule edits automatically | O-5 parity is satisfied by composition — the CLI's `undo` plans through the same `plan_undo`, so rule-edit inverses ride it with zero CLI work | This plan §1 (G2) |
| R-5 | Snooze/dismissal/ad-hoc-alarm rows stay `NotUndoable` (no matrix rows) | The hard constraint: a delivered notification happened; snooze links are recorded facts (design plan §3.5, dispatch trap a). A synthesis entry that would retract a delivery is a shipping blocker | Not debt — a law (§1 N2) |
| R-6 | Board/Today/Review/Settings/Capture cells stay `—` for rule-editing verbs | The verbs' homes are unchanged by this plan (TaskDetail owns rule editing; the census rows carry the authorities). This plan changes undoability, not placement | Census rows (pre-existing authorities) |

## 3. User stories

### US-1 — Undo a removed reminder rule

- **Story**: As a desktop user I removed a task's reminder rule and
  want Ctrl+Z to restore the exact rule (same id, same schedule) so
  that I don't have to re-add it by hand and eat a fresh identity.
- **Platforms**: both (the undo reflex is global; the rule edit's
  entry point is desktop's TaskDetail — platform-qualified there, not
  here, because undo coverage is what this story adds).
- **Entry points**: the global undo reflex — `ActionId::AppUndo`
  (keymap preset, Ctrl+Z), the action-menu undo row, CLI `undo`.
- **Priority**: P1.
- **Acceptance**:
  - AC-1: Given a task with rule R journaled as removed When
    `StateCommand::Undo` plans Then the plan contains the verbatim
    `UpsertReminderRule(R)` (same id, same `created_at`) — verified by
    domain fold test (`undo/tests.rs::us1_s4_removed_rule_resurrects_verbatim`).
  - AC-2: Given the removal was already pushed When undo plans Then
    the plan enqueues the document-lane flush op (the D-U2 pushed
    class), never a retraction — verified by domain fold test.
  - AC-3: Given the removal is unpushed When undo plans Then the
    entry's own ops are failed and no enqueue remains (retraction
    class) — verified by domain fold test.
  - AC-4: Given the UI after the undo When the projection publishes
    Then `UndoView.head_label` is `Some(_)` naming the undone edit
    (typed `Option` fact, never label text) — verified by journey test.

### US-2 — Undo an added reminder rule

- **Story**: As a desktop user I added a reminder rule to the wrong
  task and want Ctrl+Z to delete it again so that the mistaken rule
  never fires.
- **Platforms**: both (same reflex shape as US-1).
- **Entry points**: `ActionId::AppUndo`; action-menu undo row; CLI
  `undo`.
- **Priority**: P1.
- **Acceptance**:
  - AC-1: Given a task whose journaled head is an `AddReminderRule`
    When `StateCommand::Undo` plans Then the plan contains
    `DeleteReminderRule(rule_id)` for the added rule's id — verified by
    domain fold test (`undo/tests.rs::us2_s4_added_rule_deletes_again`).
  - AC-2: Given the rule fired a notification between add and undo
    When undo plans Then the plan is unchanged (the delivery proof is
    never touched; the rule edit inverts, the fact stands) — verified
    by domain fold test asserting the same `DeleteReminderRule` plan
    with no action addressing the delivery record.
  - AC-3: Given the undo committed When the state publishes Then the
    rule row is absent from `app.reminders.rules` and the journal head
    moved to the entry's parent — verified by state-engine snapshot
    test.

### US-3 — Undo a cleared rule set (catalogue-only, no surface)

- **Story**: When a future surface dispatches `ClearReminderRules` I
  want the whole pre-clear rule set restored verbatim on undo so that
  bulk rule loss is recoverable the day the verb gains an entry point.
- **Platforms**: both (no surface today — R-3; the story pins the
  engine contract only).
- **Entry points**: none today (engine-level coverage; the exhaustive
  synthesis lockstep is the guarantee).
- **Priority**: P2.
- **Acceptance**:
  - AC-1: Given a journaled `ClearReminderRules` over rules R1..Rn
    When `StateCommand::Undo` plans Then the plan contains one
    verbatim `UpsertReminderRule` per rule, id-ascending — verified by
    domain fold test (`undo/tests.rs::us3_s4_cleared_set_resurrects_id_ascending`).

## 4. Journey specs

### US-1 — Undo a removed reminder rule

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-1/S1 | TaskDetail rule list (`rule-list.slint`) | Remove a rule row (existing affordance) | `ActionId::ReminderRuleRemove` → `StateCommand::RemoveReminderRule` | Rule row leaves the list; the edit journals with `PreImage::ReminderRule` pre-image (capture exists since #900) | The staged-remove arming grammar is the pre-commit escape (pre-existing, census `task.reminder_rules_edit` row) |
| US-1/S2 | Global undo reflex (`AppUndo`) | Ctrl+Z (or action-menu undo row) | `StateCommand::Undo` | `plan_undo` synthesizes the verbatim resurrect (`UpsertReminderRule`, same id/stamps); outcome `CommandOutcome::Undone { entry }` | Typed refusal if the head is `NotUndoable`: `TargetNotUndoable(reason)` — the toast renders the typed reason (the R-U6 law); nothing moves |
| US-1/S3 | Undo toast / head label | — (display only) | — | `UndoView.head_label` names the undone edit; the rule row is back in `rule-list.slint` with its original identity | Re-edit at the seam: removing the rule again is the deliberate redo-by-hand path |
| US-1/S4 | Journal (engine-side assertion point) | — (assertion only) | — | Fold tests pin the plan shape (AC-1..AC-3 of US-1) | — (test-only step) |

### US-2 — Undo an added reminder rule

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-2/S1 | TaskDetail rule rows (`task-view.slint:731-735`) | Add a rule (preset/offset affordance) | `ActionId::ReminderRuleAdd` → `StateCommand::AddReminderRule` | New rule row; journals with `PreImages` (the arm-agnostic capture records the `UpsertReminderRule` — before: None) | Editor escape paths pre-existing (the rule editor's own cancel) |
| US-2/S2 | Global undo reflex | Ctrl+Z | `StateCommand::Undo` | `plan_undo` synthesizes `DeleteReminderRule(added_id)`; `CommandOutcome::Undone { entry }` | Typed refusal per US-1/S2 (same refusal lane) |
| US-2/S3 | TaskDetail rule list | — (display only) | — | The mistaken rule row is gone; a delivery that already fired stays recorded (R-5) — the undo adds no delivery-addressing action; the action-layer prune is the standing delete contract *(amended 2026-11-22 by operator ruling, comment 7079171410 on #1215: superseding the earlier end-to-end wording "the alarm history is untouched by construction" — the plan-batch-level statement remains true and stays pinned by `undo/tests.rs::us2_s4b_delivery_proof_untouched`; no storage change: `PersistenceAction::DeleteReminderRule`'s prune contract keeps applying uniformly to every removal path, undo included)* | Re-add at the seam if the undo itself was a mistake |
| US-2/S4 | Journal (engine-side assertion point) | — (assertion only) | — | Fold tests pin the plan shape (US-2 AC-1..AC-3) | — (test-only step) |

### US-3 — Undo a cleared rule set

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-3/S1 | (no surface — engine contract) | Test fixture dispatches | `StateCommand::ClearReminderRules` | Journals one `PreImage::ReminderRule` per cleared rule | NotUndoable refusal if the arm were ever misclassified (the lockstep forbids silent gaps) |
| US-3/S2 | (no surface) | Test fixture dispatches | `StateCommand::Undo` | Verbatim resurrect per rule, id-ascending; `CommandOutcome::Undone` | Same typed-refusal lane |

## 5. Interaction flows

No new multi-surface journey: US-1/US-2 compose two existing flows
(rule edit; global undo reflex). The composition in one diagram:

```mermaid
flowchart TD
    TD[TaskDetail rule rows] -->|ReminderRuleAdd / Remove| CMD[StateCommand::Add/RemoveReminderRule]
    CMD --> J[(Journal: PreImage::ReminderRule)]
    Z[AppUndo reflex / CLI undo] --> UNDO[StateCommand::Undo]
    J --> UNDO
    UNDO -->|"add inverse: DeleteReminderRule"| S1[Rule restored / deleted in state]
    UNDO -->|"remove+clear inverse: verbatim resurrect (#900 path)"| S1
    S1 --> TOAST[UndoView.head_label names the edit]
    UNDO -->|typed refusal| REF[TargetNotUndoable(reason) toast]
```

No overlay/editor/modal is introduced — no `stateDiagram-v2` block is
required (the rule editor's own state machine is the pre-existing
task-editing plan's, unchanged).

## 6. Wiring inventory

<!-- Gate-grammar normalization (2026-11-22, the #1181 classes):
the W-row cells below carry the exact accepted row semantics,
re-flowed into the parser's bare-token grammar (callbacks/commands
cells bare or em-dash; semicolon-separated command tokens; the
unstep-nominated tests mapped through bound-test lines in §8). The
prose the earlier cells carried survives in the paragraph after the
table and in §8; no row's meaning changed. -->

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | (none — domain) | (none) | — | `synthesize_undo` new arms over `PreImage::ReminderRule` | — | StateCommand::RemoveReminderRule | — (no UI row; the add's synthesized inverse — the remove/clear inverse is the #900 action-level verbatim resurrect, no command) | US-1/S2, US-2/S2, US-3/S2 | crates/taskboard-domain/src/undo/tests.rs::us1_s4_removed_rule_resurrects_verbatim crates/taskboard-domain/src/undo/tests.rs::us3_s4_cleared_set_resurrects_id_ascending |
| W-2 | (none — domain) | (none) | — | `record_entry` classification re-arms (ReminderSurface → PreImages for the three verbs) | — | StateCommand::AddReminderRule; StateCommand::RemoveReminderRule; StateCommand::ClearReminderRules | — (journal-side; the three verbs gain undoable facts) | US-1/S1, US-2/S1, US-3/S1 | crates/taskboard-domain/src/undo/tests.rs::us2_s4_added_rule_deletes_again |
| W-3 | (none — presentation) | (none) | — | `command_label` three new arms | — | — | — (no command; pure display derivation over existing commands — the row exists so the label contract is greppable) | US-1/S3, US-2/S3 | crates/taskboard-domain/src/journal/tests.rs::us1_s3_command_label_arms_present |
| W-4 | ui adapter (existing toast lane) | existing undo toast | — | existing `install_seam!` undo path (unchanged) | ActionId::AppUndo | StateCommand::Undo | — (no new affordance; the existing AppUndo handler needs zero change — the row proves the composition end-to-end through the UI driver) | US-1/S2, US-2/S2 | crates/taskboard-ui-slint/tests/journeys/undo_ui_reminder_undo_journeys.rs::us1_s2_undo_restores_removed_rule_row crates/taskboard-ui-slint/tests/journeys/undo_ui_reminder_undo_journeys.rs::us2_s2_undo_deletes_added_rule_row |

Rows W-1–W-3 have no `.slint` callback by design: UF1's delta is
engine-side; the user-visible change is that existing affordances
start composing. W-4 exists to prove the composition end-to-end
through the UI driver.

## 7. Command-contract delta

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|
| StateCommand | (none new) | — | — |
| StateCommand (changed facts) | `AddReminderRule` | classification: `NotUndoable(ReminderSurface)` → `UndoFacts::PreImages` (capture: the `UpsertReminderRule` action, before: None) | US-2 |
| StateCommand (changed facts) | `RemoveReminderRule` | classification → `UndoFacts::PreImages` (capture: the `DeleteReminderRule` action's rule row) | US-1 |
| StateCommand (changed facts) | `ClearReminderRules` | classification → `UndoFacts::PreImages` (capture: one `PreImage::ReminderRule` per `DeleteReminderRule` action, the arm-agnostic walk) | US-3 |
| CommandOutcome | (none new) | `Undone { entry }` unchanged; typed refusals unchanged (`TargetNotUndoable(ReminderSurface)` simply stops occurring for these three verbs) | US-1, US-2, US-3 |
| Presentation | `command_label` + 3 arms | "Add reminder rule" / "Remove reminder rule" / "Clear reminder rules" (labels are presentation; tests assert the typed facts) | US-1/S3, US-2/S3 |
| UiRequest | (none new) | — | — |
| ActionId | (none new) | — | — |

No new `UiRequest` / `ActionId` is implied: the journeys dispatch only
existing verbs. This is the derivation hinge for UF1's engine work
items — every catalogue entry exists because US-1/US-2/US-3's steps
plan against it.

## 8. Verification plan

<!-- Gate-grammar normalization (2026-11-22): the steps whose covering
test does not carry the step's own ID prefix map through bound-test
lines (the #768 grammar) — the covering test names the assertion
point, not a re-run of the step. -->

bound-test: US-1/S1 -> crates/taskboard-ui-slint/tests/journeys/undo_ui_reminder_undo_journeys.rs::us1_s2_undo_restores_removed_rule_row
bound-test: US-2/S1 -> crates/taskboard-ui-slint/tests/journeys/undo_ui_reminder_undo_journeys.rs::us2_s2_undo_deletes_added_rule_row
bound-test: US-2/S3 -> crates/taskboard-ui-slint/tests/journeys/undo_ui_reminder_undo_journeys.rs::us2_s2_undo_deletes_added_rule_row
bound-test: US-3/S1 -> crates/taskboard-domain/src/undo/tests.rs::us3_s4_cleared_set_resurrects_id_ascending
bound-test: US-3/S2 -> crates/taskboard-domain/src/undo/tests.rs::us3_s4_cleared_set_resurrects_id_ascending

- **Journey tests** (step-ID-named,
  `crates/taskboard-ui-slint/tests/journeys/undo_ui_reminder_undo_journeys.rs`):
  `us1_s2_undo_restores_removed_rule_row` (US-1/S2 through the driver:
  dispatch the remove, dispatch undo, assert the rule row's presence
  fact in the presented model — typed, not text); `us2_s2_undo_deletes_added_rule_row`.
- **Seam tests**: existing `AppUndo` dispatch assertions suffice
  (W-4 changes nothing); no new seam to pin.
- **Fold/presentation tests** (domain, exact-name nominated):
  `undo/tests.rs::us1_s4_removed_rule_resurrects_verbatim`,
  `us1_s4b_pushed_removal_enqueues_flush_not_retraction`,
  `us1_s4c_unpushed_removal_retracts`,
  `us2_s4_added_rule_deletes_again`,
  `us2_s4b_delivery_proof_untouched`,
  `us3_s4_cleared_set_resurrects_id_ascending`;
  `journal/tests.rs::us1_s3_command_label_arms_present` (typed arm
  totality, never label text).
- **Snapshot**: the state-engine command corpus gains
  `AddReminderRule`/`RemoveReminderRule` + undo sequences — the
  journal facts widen, so this is the one deliberate insta
  regeneration pass (DF-9 rule: the widening, nothing else).
- **Lockstep**: the exhaustive synthesis match compiles the three
  arms — no test needed beyond compilation (the #244 pattern).
- **Kiosk gates**: none — no affordance is added or gated; the undo
  reflex is already global on kiosk (census `app.undo` row).
- **Manual**: none — no judgment call this plan introduces that a
  test cannot pin (the toast label is judged at the stage-3
  walkthrough like every other label).

## 9. Definition of done

- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0
- [ ] Every P1 story's journey completes end-to-end including escapes
      (US-1, US-2)
- [ ] Walkthrough protocol executed at stage 3; findings resolved or
      homed
- [ ] All matrix absences carry rulings (R-1..R-6); census rows
      updated: `task.reminder_rules_edit` gains the undoability note,
      `app.undo` row gains the three verbs' coverage note
- [ ] Non-goals filed as backlog issues: R-1's mute item, R-3's
      clear-all surface item (numbers recorded in the stage-3 report
      and on issue #354)
