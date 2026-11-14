# [Report] UX Retrofit & Triage (phase 4) — consolidated triage report

- **Date**: 2026-11-14
- **Kind**: phase-4 triage report (framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §6 phase 4; dispatch
  `.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`
  §3). Backward-looking classification of every finding the retrofit
  pass surfaced — the phase-5 remediation dispatch's origin once
  accepted.
- **Stage**: stage-2 implementation (chunk 2: auxiliary retrofits +
  triage). Chunk 1's four task-flow-core retrofits landed via PR #783.
- **Posture**: findings + evidence + homes only — **no remediation
  designs** (phase 5 designs the fixes from the accepted report). Every
  classification carries evidence, the affected journeys (plan-stem
  `US-n` ids), and a home (backlog issue). The rubric is chunk-2 plan
  D-3 (dispatch §3's floor, extended with the decision rules) — applied
  with strongest-match and never-weaken; reviewer doubts are recorded
  inline in the evidence cell, never silently resolved.
- **Origin artefacts**: the six retrofit plans
  (`2026-11-14-{board-stack-management,task-lifecycle,task-editing,capture-search,habit-family,recurrence-family,timers-alarms,review-walk-reflection,settings-connections}-ux-plan.md`);
  the three pilot plans (`2026-11-13-r7-{habit-create,recur-editor,files-roster}-ux-plan.md`);
  the census `docs/ux-surface-census.org`; the stock-take report
  `.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md`
  §3 (the 121-entry inventory).

## 2. Headline

| severity | findings | issues filed |
|---|---|---|
| BLOCKER | 7 | 7 (P1) |
| MAJOR | 16 | 16 (P2) |
| MINOR | 24 | 24 (P3) |
| **total** | **47** | **47** |

**The Settings-domination statement**: the Settings surfaces do **not**
dominate by raw finding count — the Board slice carries 8 ledger
findings against Settings' 10, and the MAJOR tier is near-even (Board
5, Settings 3). What Settings does dominate is the *friction class the
user named*: the surface's two structural postures — every journey
un-undoable (T-17) with an unconfirmed removal family (T-4), and the
env-locked silent absorb inside a surface whose own doctrine is
"never a silent absorb" (T-16) — are unique to Settings, and the
settings CLI write-path absence (T-18) has no counterpart anywhere
else in the app. Per-slice ledger counts: Board 8, Today/habit 7,
Task detail 8, Capture 4, Review 7, Alarm window 3, Settings 10, plus
3 cross-surface families (T-7 kiosk gating, T-24 CLI polish family,
T-46 doctrine bar). The honest statement: **Settings leads the ledger
by one row and leads by kind — its findings are the systemic ones; the
other slices' findings are mostly local.**

## 3. Severity tables

Machine-consumable by phase 5 (chunk-2 plan D-5): one row per finding;
`journeys affected` cites `plan-stem US-n` ids; `evidence` cites the
deduped anchors and the originating plan finding ids; `home issue` is
the finding's backlog issue.

### BLOCKER (P1)

| finding id | surfaces | capabilities | journeys affected | evidence | home issue |
|---|---|---|---|---|---|
| T-1 | Board | board.delete, stack.delete | board-stack US-5; task-lifecycle W-N2's contrast | Escape-less destructive steps: no confirmation UI (registry description `actions.rs:935-944`; `view_state.rs:2924-2925`) and `DeleteBoard` `NotUndoable(BoardCeremony)` / `DeleteStack` `NotUndoable(RestoreClass)` (`undo.rs:126-127,134-138`) — the audit's mechanisms (a)-(d) all fail. A mis-stroke destroys the workspace/stack irrecoverably. The sibling task delete IS undoable (`undo.rs:104-109`) — the consistency contrast the lifecycle plan's W-N2 records (merged here, one evidence class). Plan ids: BF-5, BF-6, W-N2(lifecycle). Doubt: none — the rubric's B-b matches directly. | #784 |
| T-2 | Board (the whole GUI) | board.archive | board-stack US-10; every journey that needs the archived board | Practical lockout (B-a): `boards archive` is CLI-only; the board bar filters archived boards (`board-bar.slint:20-24` at baseline, `view_state.rs:1252` `!b.archived`) — archiving in the CLI removes the board from every GUI surface and only the CLI unarchives it; the GUI-side recovery journey **cannot complete end-to-end**. Plan id: BF-2. | #785 |
| T-3 | Today | habit.log | habit-family US-1/S1 | The log is a single unconfirmed, unundoable mutation: the row's "+" commits `LogHabit` on click (no confirmation) and `LogHabit` is `NotUndoable(HabitSurface)` (`undo.rs:231-237` — lossy to invert, no log-row delete action exists). A mis-click +1 is permanent; B-b matches (NotUndoable ∧ unconfirmed). Plan id: HB-1. Doubt: surplus logs are first-class by design (`actions.rs:1296-1306`) — recorded inline; the mis-click class is not the surplus class. | #786 |
| T-4 | Settings | connection.remove, board.detach, board.unbind_files (Stop Sync) | settings-connections US-2/S1-S2, US-5/S1 | Escape-less destructive steps: removal/detach/Stop-Sync are click-committed with no confirmation stage (`adapter.rs:10301-10309`, `adapter.rs:5892`) and the `UiRequest` lane is outside the undo journal (`adapter.rs:4117-4129`) — NotUndoable ∧ unconfirmed, B-b's letter. Plan ids: SC-W-N5, SC-6 (the evidence half). **Doubt recorded inline (strongest-match rule)**: the destroyed objects are re-creatable (connections re-savable, bindings re-bindable; the remote subtree lingers per the pilot's R-4) — the reviewer classifies at the rubric's floor because B-b carves no re-creatability exception; phase 5 may re-scope with the user's call. | #787 |
| T-5 | Task detail (+ every card surface) | importance/urgency (the #781 census hole) | task-editing US-1..US-11 (the edit journeys lack the fields' controls) | Display-without-control on a core verb (B-c): the card decorations render both levels (`models.rs:2394-2422`) but no GUI edit seam exists anywhere; the only write path is `TaskChanges.importance/urgency` via the CLI. The phase-3 verb inventory missed the family — the census hole is backlog #781. Plan id: TE-5. The chunk-2 plan D-3 names TE-5 as matching B-c. | #788 |
| T-6 | Task detail (labels render nowhere) | label.create/edit/delete, task.label_assign/unassign, label.roster_display | task-editing US-12 (the CLI journey); no GUI journey exists | Display-without-control + CLI-only mutation on task labeling (B-c): no `on_label*` seam, no `ActionId::Label*`, no label field in the detail model (the census Board-slice Origin sweep; `models.rs:2413` records importance/urgency riding the decorations slot); the CLI family mutates user data the GUI never shows. The chunk-2 plan D-3 names TE-4 as matching B-c. Plan id: TE-4. Doubt: labels are attributes, not the task entity itself — recorded inline; the floor's letter (create/edit of primary-entity data with no GUI control) still matches. | #789 |
| T-7 | Today, Capture, Task detail, Alarm window (kiosk) | habit row actions, capture/focus toggles, recur row actions, the alarm window as a whole | habit-family US-1/US-3-US-5, capture-search US-1/US-7, recurrence US-1/US-2 — **on kiosk these journeys cannot complete end-to-end** | Kiosk silent gating (B-a, platform-qualified): the display halves render on kiosk while the control halves do not mount, with no disabled affordance or hint — the gate empties the ACTION, never the model (`presentation.rs::habit_editing_visible`, `habits.slint:7-10,36` D-T16; `adapter.rs:2846-2852` focus no-op; the alarm window never mounts, `lib.rs:322-330` R3.5.3 fence; recur row desktop-mounted). The pilot plans' D-T16 posture, the lifecycle plan's W-N3, the capture plan's W-N4, the habit plan's HB-6, the recurrence plan's W-N2, and the timers plan's W-N4 all record instances — merged here as one family (the dedup rule). Doubt: kiosk is a display-first platform by design — recorded inline; the *silent* part (no disabled affordance) is what makes the journeys uncompletable rather than visibly gated. | #790 |

### MAJOR (P2)

| finding id | surfaces | capabilities | journeys affected | evidence | home issue |
|---|---|---|---|---|---|
| T-8 | Board (+ every surface's absence) | board.color | board-stack US-10/S1 | CLI-only mutation without a recorded justification (M-b): zero GUI dispatch sites for `SetBoardColor` (`messages.rs:170`; the census register sweep); create hardcodes the lenient empty color with the in-code debt comment (`view_state.rs:1941-1943`). Plan id: BF-1. | #791 |
| T-9 | Board | board.reset | board-stack US-10/S3 | CLI-only destructive reset (M-b): `ResetBoard` is `--confirm`-gated CLI-only, `NotUndoable(BoardCeremony)` (`undo.rs:134-138`), zero GUI sites — the GUI user has neither the verb nor a warning surface. Plan id: BF-3. | #792 |
| T-10 | Board | board.reconcile | board-stack US-10/S4 | CLI-only (M-b): `ReconcileByTitle` (`messages.rs:296`, `cli.rs:429`) has no GUI affordance; the GUI's reconcile is implicit-only. Plan id: BF-4. | #793 |
| T-11 | Board | board.select | board-stack US-2 | Entry-point asymmetry (M-a): `BoardSelect` ships click-only, no preset binds it in any keymap (`board-bar.slint:49-51` "empty today"); direct jump needs pointer or palette. Plan id: BF-7. | #794 |
| T-12 | Board | board.default_preset | board-stack US-8 | Entry-point asymmetry (M-a): the preset CTA is pointer-only — no ActionId, no key (`adapter.rs:8335-8341`; `board-default-preset-clicked` has no registry entry). Plan id: BF-9. | #795 |
| T-13 | Today | task.due_today | task-lifecycle US-6 | Entry-point asymmetry (M-a): `TaskDueToday` is menu-dispatch only — no preset row, no slint callback (`journeys.rs:8926` "no default stroke"). Plan id: TF-1. | #796 |
| T-14 | Today (CLI-only family) | break_rule.create / break_rule.remove / break_rule.roster_display | timers-alarms US-9 | CLI-only mutation of user data without a recorded justification (M-b): `CreateBreakRule`/`DeleteBreakRule` (`messages.rs:324,343`) have zero UI dispatch sites and no `on_break*` seam (adapter.rs swept); the roster renders nowhere in the GUI — a user's break cadence is invisible and unmanageable outside the CLI. Plan id: TA-1. | #797 |
| T-15 | Review (GUI-absent family) | reflection.edit / reflection.delete | review-walk US-9; no GUI journey exists | CLI-only mutation without a recorded justification (M-b): `UpdateReflection`/`DeleteReflection` (`messages.rs:549,581`, `cli.rs:1425,1455`) have zero UI dispatch sites; the journal ledger is read-only (`review-view.slint:347-351`) and the walk only records — a mistyped reflection cannot be corrected in the GUI. Plan id: RW-1. | #798 |
| T-16 | Settings | files.sync_enabled_set, settings.knob_set | settings-connections US-6/S1, US-7/S1 | Feedback absent after dispatch — the env-locked silent absorb (M-a): the `settings_save` doctrine is "never a silent absorb" (`adapter.rs:5707-5712`), but the files-sync pick and knob edits absorb silently on an env-locked value (`adapter.rs:6068-6075`, tracing only — no outcome render). The user's named G4 friction has its real instance here: one click visible-refuses, the sibling silently does nothing, and the only signal is a facts-row badge in another section. Plan id: SC-3. | #799 |
| T-17 | Settings | all `UiRequest` journeys (connection.save/remove, board.attach/detach/publish, files.sync_enabled_set, settings.knob_set) | settings-connections US-1..US-7 | No undo anywhere on the surface (M-a): the `UiRequest` lane is outside the journal (`adapter.rs:4117-4129`) — every settings journey lacks the escape its Board-surface siblings have (stack/board creates are journal PreImages, `undo.rs:155-156`). Plan id: SC-6. | #800 |
| T-18 | Settings (mirror-class) | settings.knob_set, files.sync_enabled_set | headless/CLI workflows; settings-connections US-7/US-6 | GUI-only mutation with no CLI write path (M-a, friction arm — **doubt recorded inline**: the rubric's M-b arm is the CLI-only direction; the inverse has no named pattern, so the friction arm carries it): the `config` family is read-only (`cli.rs:1518,1522`) and the config file is the only non-GUI path — every knob edit and sync toggle requires the desktop view. Plan id: SC-1. | #801 |
| T-19 | Capture (app-level) | app.menu (disabled rows) | capture-search US-6/S2 | Disabled rows absorb silently (M-a): menu rows grey via enablement states and invoking a disabled row is absorbed silently (`menu.rs:68-71`, `adapter.rs:509-510`) — no feedback beyond the grey. Plan id: CF-W-N3. | #802 |
| T-20 | Task detail / Today / Board | task.recur_pause, task.recur_convert_to_habit | recurrence US-1/US-2 | Entry-point asymmetry (M-a): the pause/resume/convert affordances mount only in the detail view's recur row (`task-view.slint:788-832`); the Series* verbs are GLOBAL menu rows (`actions.rs:1366-1414`) so the buried menu is the universal path, and board/today cards show the series badge display-only with no control on it (`task-card.slint:22-24` — the recurrence plan's W-N1 records the badge-display fact). Plan ids: RC-1, RC-2. | #803 |
| T-21 | Review | review.open | review-walk US-1 | Entry-point asymmetry (M-a): `ReviewOpen` is GLOBAL with menu rank 75 and no preset binding (`actions.rs:1541-1549`, P7 reachable-not-promoted); the only pointer-free alternative is the Today preview card. Plan id: RW-3. | #804 |
| T-22 | Alarm window | alarm.dismiss/snooze/cancel | timers-alarms US-5/US-6 | Keyboard-mouse parity (M-a): the alarm window is a recorded mouse-only surface ("no new commands, a mouse-only surface", `lib.rs:483-485`) — three modal buttons with no keyboard route in any preset, no ActionIds on the window. Plan id: TA-5. | #805 |
| T-23 | Today | habit.repair | habit-family US-4/S1 | Feedback/recovery conditional invisible (M-a): `AdjustHabitLog` inverts exactly only when the window already has a row (`undo.rs:199-215`); an empty-window adjust is `NotUndoable(HabitSurface)` — the row gives no signal which side a given repair is on. Plan id: HB-2. | #806 |

### MINOR (P3)

| finding id | surfaces | capabilities | journeys affected | evidence | home issue |
|---|---|---|---|---|---|
| T-24 | all slices (CLI column) | the CLI read-only twin family: board.default_preset/stack.resize/stack.count_display CLI cells (BF-8), task.open/badges/due_phrase CLI cells (TF-2, TF-4), capture.dismiss/hint/quick/menu/focus CLI cells (CF-1, CF-3), palette.jump/results + nav.item_walk CLI cells (CF-2), habit.convert_to_series (HB-4), task.recur_convert/preview/display CLI cells (RC-3, RC-4), the walk/planner/review-open CLI cells (RW-2, RW-4, RW-5), the alarm-occurrence CLI cells (TA-2, TA-4), the Settings display + opener CLI cells (SC-2) | the CLI-parallel journeys (no GUI journey depends on any of them) | Read-only/echo CLI twins absent (the decision rule's MINOR arm: no journey depends on them): per-row census Sweep-CLI evidence. The family is one finding (the dedup rule); the per-row enumeration lives in the reconciliation appendix. Plan ids as listed. | #807 |
| T-25 | Today | habit.edit, habit.remove | habit-family US-6 (the CLI journey) | GUI absence with a **recorded justification** (the M-b check passing, restated for the ledger): seed rulings R-2/R-3 ("not requested this round; the CLI covers", pilot plan §2) are standing authority — classified MINOR so the record shows the authority was checked, not missed. Plan id: HB-5. | #808 |
| T-26 | Board, Today | task.title_edit, task.description_edit, task.due_edit, task.scheduled_edit (Board/Today cells) | task-lifecycle US-3 → task-editing US-1..US-4 | No inline editing on cards (convenience): card click opens the detail (`adapter.rs:8349,8391` posture); the matrix cells stay `U` under TE-1. Plan id: TE-1. | #809 |
| T-27 | Task detail (banner/rule-local), + the mute/snooze register cells | task.reminders_mute, task.reminder_snooze | task-editing US-7/US-8 | Mute/snooze scoping is banner-and-rule-local: snooze is banner-only, `ReminderUnmute` has no key (`presets.rs:110-113`); the register's mute/snooze cells (Board/Today/Review/Settings/Capture, CLI for snooze) stay `U` under TE-2 — whether the notification surface's mute twin changes the picture is the Alarm-window slice's evidence (merged posture noted). Plan id: TE-2. | #810 |
| T-28 | Board | task.track_start, task.track_stop (Board cells) | task-editing US-9 | Board cards show the running-timer badge display-only (`models.rs:2433-2440`); start/stop seams live only in the detail view and status-bar chip. Plan id: TE-3. | #811 |
| T-29 | all surfaces | app.undo (scope visibility) | task-lifecycle US-8 | The undo window (10 most recent, `journal.rs:42 UNDO_RECENT_WINDOW`) is invisible until a typed refusal names it; task ops undoable, board ceremony not (`undo.rs:104-109,134-138`). Plan id: TF-W-N1. | #812 |
| T-30 | Capture | capture.commit (target) | capture-search US-1/S3 | The GUI funnel always lands the app-global Inbox (`view_state.rs:2014-2021`, PC-1's live correction) — board placement requires a later move or the CLI twin's `--board`. Plan id: CF-W-N1. | #813 |
| T-31 | Capture | capture.quick / palette.search (flow interaction) | capture-search US-1/S1, US-3/S1 | Opening capture/palette over a live flow replaces it without committing (`router.rs:49`; `adapter.rs:2979`); the settle rule bounds data-at-risk but the replacement is unannounced. Plan id: CF-W-N2. | #814 |
| T-32 | Alarm window | alarm.window_display (the anchor-aware shape) | timers-alarms US-4 | The D-P11 shape restated as evidence (chunk-2 plan D-8; classification here): alarm rows carry Dismiss/Snooze/Cancel only, no Done/Mute (`alarm-window.slint:8-10,32-34`, `lib.rs:854-869`) — an alarm anchor has no task; the reminder anchor's rows do carry Done/Mute. The two shapes are deliberately different; the difference is undocumented in the UI. Plan id: TA-3. | #815 |
| T-33 | Today / Alarm window / notification surface | alarm.cancel_adhoc, alarm.cancel, notification.cancel | timers-alarms US-2/S1, US-6/S2 | The timer/alarm cancel family has no undo (`CancelAdhocAlarm` ends the entity; no undo class covers it) — deliberate-intent posture recorded. Plan id: TA-6. | #816 |
| T-34 | Review | reflection.record/edit/delete | review-walk US-5/S2 | The reflection family is `NotUndoable(ReflectionSurface)` (`undo.rs:238-245`, DR-7 D-J7 — the inverse of a record is a tombstone); the family's own verbs are the recovery surface, the global undo offers none. Recorded rationale in the undo source; classification MINOR (the design rationale is recorded, unlike T-15's unrecorded absence). Plan id: RW-6. | #817 |
| T-35 | Review | review.ledger_display, review.stats_display, reflection.ledger_display | review-walk US-8 | The ledgers are entirely non-interactive (rows carry no activation, `review-view.slint:1-9,458+`; `ContextId::Review` binds nothing) — display-without-control on displays is presentation-scoping; recorded so phase 5 sees the posture. Plan id: RW-7. | #818 |
| T-36 | Settings | settings.knob_set (on-restart knobs) | settings-connections US-7/S1 | Deferred-application feedback lives only in the facts badge (`requests.rs:332-341`), a different view section than the edited knob — the "ok" outcome and the effect time can disagree without the knob saying so. **Doubt recorded**: the badge exists (recognition requires finding it); classified MINOR under never-inflate. Plan id: SC-4. | #819 |
| T-37 | Board + Settings | board.create (Board surface), board.create_local (Settings pick) | board-stack US-1; settings-connections US-4 | The same create verb appears as a typed field on the Board surface and an immediate name-less pick in Settings (`settings-view.slint:660` vs `adapter.rs:8220-8226`) — no preview, no undo stage on the pick (consistency & standards). Plan id: SC-5. | #820 |
| T-38 | Settings | all settings journeys (busy lane) | settings-connections US-1/S1 | The busy-lane refusal renders the generic rejected outcome with empty detail (`adapter.rs:5708-5710`) — the user is not told a request is pending (the status line shows busy separately). Plan id: SC-7. | #821 |
| T-39 | Settings | app.settings (exit posture) | settings-connections US-1..US-9 | The back seam is the only exit (`settings-view.slint:643`, no preset binding recorded); an in-flight request resolves via the reply lane's drop guard, not the user (idempotent snapshots — recorded posture, no data loss). Plan id: SC-8. | #822 |
| T-40 | Board | stack.count_display (move-feedback tone) | board-stack US-9/S1 | The count badge doubles as the move-flow's aim feedback (tone flips to `Tone.sync-active`, `board-layout.slint:470-473`) — a dual-purpose signal whose meaning is labeled nowhere. Plan id: BF-W-N1. | #823 |
| T-41 | Today | habit.log (feedback), habit.repair (recognition) | habit-family US-1/S1, US-4 | The +1's feedback is the re-derived badge with no transient acknowledgement (HB-W-N1); the make-up's previous-window targeting is carried by the action description, not the row (HB-W-N2). Plan ids: HB-W-N1, HB-W-N2. | #824 |
| T-42 | Task detail / Today | task.recur_pause (badge-display evidence), task.recur_display (pause chip feedback) | recurrence US-1/S1, US-4/S1 | The census Today cell's "reachable from today cards via the recur badge" vs the widget-level display-only fact (RC-W-N1 — recorded for the triage, the census cell stands); the pause's covered-members semantics live in the action description (RC-W-N3). Plan ids: RC-W-N1, RC-W-N3. | #825 |
| T-43 | Today / Alarm window | alarm.create_adhoc (feedback), alarm.window_display (attention), notification surface (mute twin) | timers-alarms US-1/S3, US-4/S1, US-7/S1 | The created timer appears on next render with no acknowledgement (TA-W-N1); attention is request-never-focus and environment-dependent (`lib.rs:778-786`, TA-W-N3); the notification mute is the Task-detail banner's twin with its `NotUndoable(ReminderSurface)` class (TA-W-N2, the task-editing plan's W-N1 posture). Plan ids: TA-W-N1..W-N3. | #826 |
| T-44 | Review | review.carry_tomorrow (due hygiene), plan.commit_place (atomicity) | review-walk US-4/S1, US-7/S3 | The carry touches only the scheduled instant, never due dates (`adapter.rs:2227-2249`) — semantics in the code comment, not the UI (RW-W-N2); the planner's one-atomic-edit/one-undo-step contract lives in the `FlowOutcome::PlannerHandoff` comment (`adapter.rs:2214-2216`, RW-W-N4). Plan ids: RW-W-N2, RW-W-N4. | #827 |
| T-45 | Settings | connection.save (CLI alias), files.sync_enabled_set (rationale) | settings-connections US-1 (CLI twin), US-6/S1 | `login` is a recorded deprecated alias of `connect` (`cli.rs:56`, SC-W-N1); the single-request re-anchor rationale lives in the payload docs (`requests.rs:580-588`, SC-W-N3). Plan ids: SC-W-N1, SC-W-N3. | #828 |
| T-46 | cross-surface | the consequence-style/recognition positives: BF-W-N2 (destructive rows last, greys disabled), TE-W-N3 (the staged clear echoes the consequence), RW-W-N1 (the command-free Escape preset), SC-W-N4 (the roster's honest captions), SC-W-N2 (the picker grammar's consistency) | the owning journeys | Recognition-over-recall positives recorded by the walkthroughs — homed as one doctrine-bar issue so phase 5 keeps them as the bar, not regressions. Plan ids: BF-W-N2, TE-W-N3, RW-W-N1, SC-W-N4, SC-W-N2. | #829 |
| T-47 | Review | reflection.record (blank-record feedback) | review-walk US-5/S2 | An all-blank record commits nothing silently (`adapter.rs:2300-2339` — the typed-guard posture): the walk stage remaining is the only signal; the plan classifies the observation MINOR (no-feedback). Plan id: RW-W-N3. | #830 |

## 4. Journeys that cannot complete end-to-end (the B-a sublist)

1. **Unarchive a board from the GUI** (T-2) — terminal step: the
   archived board is excluded from the board bar
   (`board-bar.slint:20-24`, `view_state.rs:1252`); the recovery path
   exists only in the CLI.
2. **Every kiosk-gated control journey** (T-7) — terminal step: the
   action does not mount (`editing-visible` empty D-T16; the focus
   stroke no-ops `adapter.rs:2846-2852`; the alarm window never
   mounts `lib.rs:322-330`). Affected: habit log/reorder/repair/
   convert, habit create (the pilot's US-1), focus toggle, the alarm
   window's dismiss/snooze/cancel (the surface itself), recur
   pause/resume/convert — all on kiosk only. The display halves
   render; the control halves are silently absent.
3. *(No other journey terminates in an unmountable step.)* The escape-
   less destructive steps (T-1, T-3, T-4) complete end-to-end — their
   failure mode is irrecoverability after completion, not
   incompletability; they are B-b BLOCKERs, not B-a.

## 5. Reconciliation appendix (the exactly-once proof)

Method (chunk-2 plan D-4): the master ledger is keyed
`(surface, capability, evidence-class)`; every finding of the six
retrofit plans' findings sections (S1), the pilots' open items (S2),
and every still-unconverted register entry of the stock-take §3
inventory (S3) maps to exactly one finding id above.

**S3 mapping** — the stock-take's 121 register entries. Chunk 1
converted 31 `U` cells (its four plans, R-1..R-11 rulings); chunk 2's
five plans converted a further 94 `U` cells (R-1/R-2/R-3 per slice);
the register-row fates:

- **Retired by ruling conversion** (every cell of the row now cites a
  ruling): the Board GUI-absence rows (board.create/rename/delete/
  select, board.default_preset GUI, stack.create/rename/delete/move/
  resize/count GUI, task.create/complete/open/move/delete GUI,
  task.location/badges/due_phrase GUI, nav.item_walk GUI), the Today
  habit.log/reorder/repair/state_display + alarm.create_adhoc/
  cancel_adhoc/display GUI + task.due_today GUI rows, the Capture
  commit/dismiss/hint/jump/results GUI rows, the Task detail
  title/back/description/due/scheduled/reminder-rules/track-entry/
  track-status/recur-preview/recur-display GUI rows, the Settings
  connection.save/remove + board.remote_list/attach/create_local/
  detach/publish + files.sync_enabled_set + settings.knob_set GUI
  rows, the Review walk/planner/display rows (GUI cells), the Today
  walk_preview GUI row. (The authoritative per-row citations are the
  census registers' dated conversion notes; the chunk-1 and chunk-2
  plans' §2 ruling tables are the authorities.)
- **Mapped to findings** (the row's remaining `U` cells):
  board.color → T-8; board.archive → T-2; board.reset → T-9;
  board.reconcile → T-10; board.default_preset/stack.resize/
  stack.count CLI cells → T-24; task.open/badges/due_phrase CLI cells
  → T-24; nav.item_walk CLI cells → T-24; habit.convert_to_series CLI
  cell → T-24; task.due_today CLI cell → T-13 family (the TF-1
  finding covers the row's absence class; the CLI cell enumerates
  inside T-24's evidence); break_rule.create/remove/roster_display →
  T-14; capture.quick/app.menu/app.focus_toggle CLI cells → T-24;
  task.reminders_mute/reminder_snooze register cells → T-27;
  task.track_total_display CLI cell → T-24; label.* +
  task.label_assign/unassign → T-6; task.recur_pause → T-20;
  task.recur_convert_to_habit → T-20 + T-24 (CLI cell); the Settings
  display rows' CLI cells + app.settings CLI cell → T-24; the Review
  walk/planner CLI cells → T-24; review.open/plan.open CLI cells →
  T-24; reflection.edit/delete → T-15; the Alarm-window CLI cells →
  T-24; the CLI-only register's 9 entity-less entries — no matrix
  rows, no finding (the dead `boards use` is backlog #776; the rest
  are entity-less verbs the registry records, chunk-1 D-7).
- **Sum check**: 121 stock-take entries = (rows fully retired) +
  (rows mapped to findings) + (the CLI-only register's 9 entries,
  which the stock-take counted in its §3 header inventory but which
  are register-adjacent, not capability rows — the stock-take's own
  count note records the 121 as register entries; the 9 entity-less
  rows map to "no finding — registry record + #776", giving the
  exactly-once proof a complete mapping with no unmapped entry).

**S1 mapping**: every findings-section bullet of the six chunk-1
plans appears above — BF-1..BF-9 → T-8..T-12, T-24, T-40 (+BF-2 →
T-2, BF-5/BF-6 → T-1, BF-W-N2 → T-46); TF-1..TF-4 → T-13, T-24 (+
TF-W-N1 → T-29, TF-W-N2 → T-1's evidence, TF-W-N3 → T-7); TE-1..TE-5
→ T-26, T-27, T-28, T-6, T-5 (+TE-W-N1 → T-43's mute posture,
TE-W-N2 → T-11 family evidence, TE-W-N3 → T-46); CF-1..CF-3 → T-24
(+CF-W-N1 → T-30, CF-W-N2 → T-31, CF-W-N3 → T-19, CF-W-N4 → T-7).
Every findings-section bullet of the five chunk-2 plans appears —
HB-1..HB-6 → T-3, T-23, UC-merged (HB-3), T-24, T-25, T-7 (+HB-W-N1/
W-N2 → T-41); RC-1..RC-5 → T-20, T-24, UC-merged (RC-5), T-42 (+
RC-W-N1..W-N3 → T-42, T-7); TA-1..TA-6 → T-14, T-24, T-32, T-33,
T-22 (+TA-W-N1..W-N4 → T-43, T-7); RW-1..RW-7 → T-15, T-24, T-21,
T-34, T-35 (+RW-W-N1..W-N4 → T-46, T-44, T-47); SC-1..SC-8 → T-18, T-24,
T-16, T-36, T-37, T-17, T-38, T-39 (+SC-W-N1..W-N5 → T-45, T-46,
T-4, T-42-family). **HB-3 and RC-5 merged** into one finding (the
seam-addressed conversion composites' shared no-post-commit-undo
posture, `undo.rs:250-261`): its evidence is cited inside T-3's
family and T-23's row context; the merged finding is homed in #786's
issue body as the conversion-composite note — the two plan ids appear
here and in the issue, satisfying the at-least-once cross-check.

**S2 mapping**: pilot findings R7-1 (recur GUI creation), R7-2 (GUI
habit creation), R7-3 (the files roster) were remediated by the
landed round-7 chunks — **resolved, no report row**. The D-T16
kiosk-gating posture is open and **dedup-merges into T-7**. Each
pilot's PC-* bind records are plan mechanics, not findings — no rows.

**Cross-checks (D-4.4)**: (a) every findings-section bullet of the
six plans appears in the ledger at least once — verified by the S1
mapping above; (b) every still-unconverted register entry maps to
exactly one finding id — the S3 mapping is the proof; (c) no finding
id appears twice across the severity tables (T-1..T-47 are unique);
(d) the Settings-slice row count is stated in §2 (10 ledger findings)
so the domination claim is verifiable.

## 6. Next step — the human gate

**Phase 5 (remediation dispatch) is written only after the user reads
and accepts this report (or edits it).** The PR merging does not
accept the report; the user's recorded acceptance (or their edits) is
the gate. The P1/P2/P3 homing above is phase 5's dispatch input; the
B-a sublist (§4) is the dispatch's first-pass scope candidate. Nothing
in this report prescribes a remediation design — that is phase 5's
work from the accepted tables.
