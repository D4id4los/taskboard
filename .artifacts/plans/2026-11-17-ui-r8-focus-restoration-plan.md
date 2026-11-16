# [Plan] ui-r8 chunk 2 — focus restoration & modal containment

- **Date**: 2026-11-17
- **Kind**: chunk plan (stage-1) — TDD-first implementation spec for
  `fix/ui-r8-focus-restoration`
- **Origin**: the ACCEPTED UX journey plan
  `.artifacts/plans/2026-11-16-focus-restoration-ux-plan.md` (all
  sections normative: §3 US-1..US-6 with ACs, §4 step tables, §6
  W-1..W-6, §7 delta, §8 verification, §9 DoD, rulings R-1..R-3) and
  the dispatch plan
  `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
  §1/§3. This plan resolves the origin's open design questions into
  recorded decisions; it introduces no new scope and loosens nothing.
- **Status**: READY for stage 2 (TDD; every commit standalone; gate =
  pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run
  --workspace`; the coverage trio never runs in the subagent).
- **Worktree**: `../taskboard-WT-fix-ui-r8-focus-restoration` off
  `main` at `dfdf2562` (doctor exit 0, 153G free).

## 1. Ground truth read from the seams (stage-1 findings)

What the code actually does today, and the exact attachment points:

- **One key handler** (`ui/app-window.slint` §5.1): `key-root :=
  FocusScope` is the tree's only key-event consumer; `refocus-view()`
  (line ~1545) re-anchors it. The control walk's focus *movement* is
  not adapter-owned: `NavNextControl`/`NavPrevControl` route to
  `UiEffect::ControlNext|Prev` (router.rs:1022) and
  `dispatch_traversal` (adapter.rs:3674) injects a **synthetic Tab**
  through `slint::platform::WindowEvent`, so the runtime traversal
  decides the next stop — it has **no mask boundary**: inside a
  palette or capture overlay, Tab can walk into the obscured view.
  This is the US-2 gap.
- **The focus-item oracle exists**: `tests/focus_coverage.rs` reads
  `WindowInner::focus_item` directly (the recorded Slint-bump churn
  surface, pin `=1.18.1`). Journey tests can assert *where* slint
  focus rests without any new facts global. The walk half of that
  test pins synthetic-Tab wrapping across whole views — it is the
  regression surface for the clamp (§3 D-W2).
- **The chain-level restore exists** (`src/level_state.rs` D-H7):
  every level carries `FocusAnchor` (`None`/`View`); a level's exit
  restores `invoke_refocus_view()`. What is missing is the
  **element-level** origin: today every overlay exit re-anchors the
  view scope (the window top) — the exact N-3 finding.
- **The US-4 double handoff** (`adapter.rs` interpret tail,
  ~line 4310): the identity-diff tail fires
  `window.invoke_refocus_view()` whenever
  `view_identity()` changed. On Today, `step_focus` →
  `follow_board` (view_state.rs:1570) switches `self.board` when the
  walk crosses boards → the identity diff sees a board-segment change
  → the tail re-anchors the key root over the walked row. One dispatch,
  two handoff consumers; the second parks the key root (#670).
- **Notification surface** (`ui/compositions/notification-surface.slint`):
  six callbacks + `row-open-requested`, each an installed seam
  (adapter.rs:11840 `install_notification_seams`) routing exactly one
  engine command or one routed registry action over a one-shot target
  (`reminder_target` / `timer_target` / `track_target`). The surface
  is **ambient**: mounted whenever `notification-model.length > 0`
  (app-window.slint:704) — rows persist until dismissed. No context,
  no walk. The keyboard half of the row-open is a recorded exemption
  in `focus_coverage.rs` citing #724 (this chunk retires that
  exemption).
- **The r5 template** (`ContextId::AlarmWindow` + presets.rs:366-371
  et al.): `MenuNext`/`MenuPrevious` (Up/Down) walk, `MenuDismiss`
  (Escape) closes, three per-verb keys (`d`/`s`/`c`), rows bound in
  every preset, total mask while the window holds. US-6 clones this
  letter.
- **US-5**: `task-view.slint` already binds
  `border-width: title-stop.has-focus ? Theme.focus-ring-width : 0`
  (~line 490) and the description twin (~1129). The gap: the walk's
  focus is *logical* (view_state `focused_task` + walk facts), not
  slint `has-focus` — an armed title/description field reached by the
  walk or by F2 shows no ring until clicked. The field **models**
  need the focus flag.
- **Menu open** (`Flow::Menu` over the shell): rows are a model the
  adapter pushes; nothing focuses the rows' FocusScope on open — the
  #669 gap. The palette's pattern (its own field grabs focus on
  mount, adapter.rs D20 seam 3) is the house precedent.
- **Census raw material**: `FlowTag` (surfaces.rs:32) enumerates 26
  flow kinds; the overlay hosts and their dismissal backdrops live in
  `ui/app-window.slint` (date-picker, timer-setter, labels, capture,
  palette, focus overlay, track entries, notification surface) and
  `ui/views/review-walk-overlay.slint`; the pickers/menus open via
  `open_task_context_menu` / `open_stack_context` (adapter.rs:8728/
  9020) and the switcher flyout (9151). The census below (§2) is the
  complete enumeration.

## 2. The overlay-site census (US-1/AC-4 deliverable, W-1)

The census table. Every site that opens a picker, editor, menu, or
overlay: its open seam, its exit arms, the origin/anchor it records,
and the journey test that pins its restore. No site is "decided
later"; a site discovered mid-implementation joins this table in the
same commit as its restore (dispatch prompt 2.2 step 3).

| # | Site | Open seam (adapter) | Exit arms | Origin recorded | Restore target / fallback | Test (tests/journeys.rs unless noted) |
|---|---|---|---|---|---|---|
| 1 | Due/start-date editor (`Flow::EditDate`) | `task_due_edit_asked` / `task_scheduled_edit_asked` funnels | commit (Activate/`accepted`), Escape clean close, picker-phase Escape | `(DueEditor, Task(subject))` | the subject's host row; field-phase Escape restores into the editor field (existing `FocusHandover`) | `us1_s1_picker_close_restores_the_invoking_field` |
| 2 | Calendar picker (`Flow::EditDate` + picker phase) | the editor's picker arm (`DatePicker` ctx) | day click commit, outside click, Escape ladder rung | same as 1 | the due field inside the editor (the editor stays; the *picker* exits inward) | covered by test 1's picker leg |
| 3 | Recur editor (`Flow::EditRecur`) | recur editor open funnel | commit, cancel, detach strip, Escape | `(RecurEditor, Task(subject))` | subject row → row fallback | `us1_s1_overlay_escape_restores_the_origin` (recur leg) |
| 4 | Labels overlay (`Flow::Labels`) | `labels_manage_asked` | dismiss (Escape/outside), chip delete, roster commit | `(LabelsOverlay, Task(subject))` | subject row → row fallback | `us1_s3_commit_exit_restores_the_row_anchor` |
| 5 | Priority pickers (`priority_open`) | `task_importance_edit_asked` / `task_urgency_edit_asked` | slot click commit, `priority_dismiss_requested`, Escape (the CancelMove arm) | `(PriorityPicker(field), Task(subject))` | the picker's anchor cell → subject row fallback | `us1_s1_picker_close_restores_the_invoking_field` (priority leg) |
| 6 | Track-entries overlay + form (`Flow::TrackEntries`/`TrackEntryForm`) | `TaskTrackEntries` route | form commit/cancel, overlay dismiss ladder end, entry date-picker exit | `(TrackEntries, Task(subject))` | subject row → row fallback | `us1_s1_overlay_escape_restores_the_origin` (entries leg) |
| 7 | Action menu (`Flow::Menu`) | `ActionId::AppMenu` / context-menu opens (`open_task_context_menu`, `open_stack_context`) | row invoke, Escape, outside click | `(ActionMenu, Task/Stack origin)` | the invoking row/header → view root | `us1_s1_menu_close_restores_the_origin` |
| 8 | Search palette (`Flow::Palette`) | palette toggle route | invoke jump, Escape close | `(Palette, View)` | the pre-palette view scope (existing behavior — *pinned*, first regression net) | `us1_s1_overlay_escape_restores_the_origin` (palette leg) |
| 9 | Capture overlay (`Flow::Capture`) | capture toggle route | commit, cancel, Escape | `(Capture, View)` | view scope; Today's capture-leg restores into the walked row if one was focused | `us1_s1_overlay_escape_restores_the_origin` (capture leg) |
| 10 | Timer setter (timer-setter-open flag) | timer setter ask seam | commit, dismiss request, outside click (backdrop), Escape | `(TimerSetter, Task/anchor)` | invoking anchor cell → view root | `us1_s1_picker_close_restores_the_invoking_field` (setter leg) |
| 11 | Focus overlay (`Flow::FocusMode`) | `FocusModeToggle` route | toggle end, session stop, Escape | `(FocusOverlay, View)` | view scope | `us1_s1_overlay_escape_restores_the_origin` (focus leg) |
| 12 | Notification surface (this chunk's context) | ambient mount + the walk's engagement (D-N1) | per-verb row exit, Escape, rows emptied | `(NotificationSurface, row cursor)` | the pre-engagement focus → view root | `us6_s1`/`us6_s3` (§6) |
| 13 | Review walk overlay (`Flow::ReviewWalk`) + its children (File-to `WalkFilePick`, `WalkDatePick`, `EntryDatePicker`) | `OpenReviewWalk` / planner handoff | row-step commit, rung exits, walk close (`Record`/`Finish`/Escape ladder) | `(ReviewWalk, Task(cursor))` | the walked/next row (the D15 handoff already re-anchors; the *rung* exits restore into the walk's own field — existing `FocusAnchor::View` arms) | `us1_s3_commit_exit_restores_the_row_anchor` (walk leg) |
| 14 | Title editor (`Flow::EditTitle`) | title arm seam (`task_title_edit` family) | commit (focus-loss settle / accepted), Escape discard | `(TitleEditor, Task(subject))` | the title cell (`title-stop`) → subject row | `us1_s1_picker_close_restores_the_invoking_field` (title leg) |
| 15 | Description editor (`Flow::EditDescription`) | description arm seam | commit, Escape discard | `(DescriptionEditor, Task(subject))` | the description cell → subject row | covered by test 14's twin |
| 16 | Create task / create stack / rename flows (`Flow::Create*`, `Rename*`) | `BeginCreate*`/`StackRename`/`BoardRename` routes | commit, SetMode(Normal) discard, Escape | `(CreateFlow, View/Stack/Board)` | view scope / stack header / board bar cell | `us1_s2_vanished_origin_falls_to_the_container_anchor` (rename-of-deleted leg) |
| 17 | Move flows (`Flow::Move`/`MoveStack`/`DragMove`) | `RequestMove` route / drag start | confirm, cancel, Escape | `(MoveFlow, Task/Stack)` | invoking row → row fallback | `us1_s1_overlay_escape_restores_the_origin` (move leg) |
| 18 | Armed-delete confirm strip (`Flow::ConfirmDelete`) | delete verb first stroke | confirm, second stroke, Escape | `(ConfirmDelete, Stack/Board header)` | the header cell | `us1_s1_overlay_escape_restores_the_origin` (delete leg) |
| 19 | Board color picker (`Flow::BoardColorPick`) | `BoardColorEdit` route | pick commit, Escape | `(BoardColorPick, Board header)` | board bar cell → view root | covered by test 18's family run |
| 20 | Plan-next-day placement (`Flow::PlanPlace`) | `PlanBeginPlace` route | commit, cancel, Escape | `(PlanPlace, Task(cursor))` | planner cursor row | `us1_s1_overlay_escape_restores_the_origin` (planner leg) |

Families for the journey tests (the origin §8 names): **picker**
(1, 2, 5, 10, 14), **menu** (7), **overlay-escape** (3, 6, 8, 9, 11,
17, 18, 19, 20), **row-anchor-commit** (4, 13), **container-anchor
fallback** (16 + every leg's vanished branch), **notification** (12).

## 3. Decisions (each settles an origin assumption)

### D-F1 — The focus-origin fact: typed shape, one-shot, last-open-wins

A new `FocusOrigin` in `src/adapter.rs` (module-private, like
`priority_open`), **not** a chain level and **not** a command:

```rust
pub(crate) enum FocusSeam {
    DueEditor, RecurEditor, LabelsOverlay, PriorityDue, PriorityScheduled,
    TrackEntries, ActionMenu, Palette, Capture, TimerSetter, FocusOverlay,
    NotificationSurface, ReviewWalk, TitleEditor, DescriptionEditor,
    CreateFlow, MoveFlow, ConfirmDelete, BoardColorPick, PlanPlace,
}
pub(crate) enum OriginAnchor {
    View,                       // the flat-flow letter (today's D-H7 letter)
    Task(TaskId),               // the row anchor (the invoking card/today row/detail subject)
    Stack(StackId),             // the column header
    Board(taskboard_domain::BoardId), // the board bar cell
}
pub(crate) struct FocusOrigin { seam: FocusSeam, anchor: OriginAnchor }
```

- Held as `Adapter.focus_origin: Option<FocusOrigin>`. **Recording**:
  every census site's open arm sets it (overwrites — last-open-wins;
  one-shot: consumed by the next restore). **Restoration**: the exit
  funnel resolves it once, then clears it — every exit arm (commit,
  cancel, Escape, ladder end, outside click) passes through the same
  funnel points the census names, so the restore is funnel-side, not
  per-arm.
- **Resolution order** (the AC-2 fallback is structural, not a
  special case): `Task(id)` live in `state.tasks` → the targeted row
  focus; dead → the row's container anchor (the section/stack header
  — the adapter resolves the anchor's parent via the row tables) →
  if that is gone too, `View`. `Stack`/`Board` anchors resolve dead →
  `View`. The restore *never* lands on no-control.
- **Why Adapter-side, not the chain's `FocusAnchor`** (recorded
  reason): the chain anchor is per-level parent→child and already
  covers the *view-scope* letter (D-H7); the origin fact is
  cross-cutting (a menu opened from an arbitrary row above any chain
  shape) and one-shot last-open-wins — a level lifetime cannot
  express "the second open overwrote the first". The two mechanisms
  compose: level exits keep their D-H7 letter; the origin fact rides
  on top for the element letter. Where both fire on one exit (the
  common flat-flow case) the origin resolution runs *before*
  `invoke_refocus_view()`, and the view-scope call only fires when
  the origin resolved to `View` — this ordering is the fix that makes
  exits stop landing on the window top.
- **The typed oracle**: a new `FocusFacts` slint global (`in
  property <int> seam-id; in property <int> anchor-kind; in property
  <int> anchor-row; in property <int> nonce`) pushed by the restore —
  journey tests assert the tuple (typed ints, no text), and
  `focus_coverage.rs`'s `WindowInner::focus_item` read asserts where
  slint focus physically rests where a physical assertion is
  meaningful.
- **The targeted restore calls**: one new slint function per anchor
  family on AppWindow (`focus-task-row(int)` — the existing
  FocusScrollHint target pair already names row (stack, position) —
  reuse its target types; `focus-stack-header(int)`;
  `focus-board-cell(int)`), each with its `// slint: <callback>`
  comment (#782 law) and a `seam_inventory.rs` entry. The existing
  per-view walk facts already implement "give row *i* the focused
  look"; the new calls set the *slint* focus sibling (the walk stops)
  beside the logical facts.

### D-W2 — The walk clamp: adapter-owned boundary over pushed mask facts

`NavNextControl`/`NavPrevControl` keep their router letters. The
change is at `dispatch_traversal`'s call site (adapter.rs:3519):

- **In-mask walk**: a new `MaskWalkFacts` global (`in property <bool>
  armed; in property <int> cursor; in property <int> extent; in
  property <int> nonce`) — each masked host (palette, capture,
  timer-setter, focus overlay, track entries, due editor, recur
  editor, labels overlay, notification surface; the hosts that mount
  the contexts `Palette`, `FocusMode`, `TrackEntries`, `RecurEditor`,
  `Labels`, the picker/forms, and this chunk's `NotificationSurface`)
  pushes `armed=true` + its focusable-control extent while mounted.
  When `armed`, `ControlNext/Prev` compute `cursor ± 1` **mod
  extent** (wrap = the clamp; the degenerate extent=1 stays on 0 —
  AC-2 of US-2) and invoke a new `mask-focus(int)` AppWindow function
  that sets focus to the host's control at that index (each host
  exposes its ordered control list to the function; the `// slint:`
  comment law applies). No synthetic Tab is injected while armed.
- **Not armed**: today's synthetic traversal, byte-identical.
- **Insert-mode text grammar untouched** (US-2/AC-3): the clamp
  governs only the `ControlNext/Prev` *walk* effects; text fields
  keep consuming Tab per their editor-command tables ahead of the
  resolver (the existing `intercept` precedence). No keymap row
  changes → the RW-W-N1 command-free Escape preset cannot regress
  (nothing in this decision touches Escape or any preset row).
- The cursor state is the **host's** (pushed through the facts
  global on its own focus changes, nonce-edged like
  `FocusScrollHint`); the adapter owns only the arithmetic — one
  consumer per nonce edge, the house push discipline.
- `tests/focus_coverage.rs`'s whole-view Tab walk is the regression
  net: it runs *unarmed* (no mask mounted) and must stand untouched.

### D-M3 — The menu focused-open seam (US-3/W-3)

The menu open arm (the `AppMenu` route / the context-menu opens) sets
the menu's cursor to the remembered/first row (today's behavior for
the *cursor*) and additionally invokes a new
`window.invoke_menu_rows_focus()` AppWindow function: the menu panel's
row FocusScope grabs slint focus on mount, arrows/Tab walk rows
inside the panel, Escape restores per D-F1 (the census row 7). One
new seam; `// slint: menu-rows-focus-requested`; the palette's
mount-focus (D20 seam 3) is the precedent. The disabled-row notice
posture (r5 D-6) is untouched: an invoked disabled row still notices
and does not close.

### D-H4 — The Today walk handoff: suppress the second refocus (US-4/W-4)

Root cause (§1): `interpret`'s identity-diff tail refocuses on the
board-segment change that `follow_board` itself produced. Fix: the
tail's `window.invoke_refocus_view()` is skipped when the dispatch
was a focus walk that landed on a row — concretely, when `effect` is
`FocusNext|FocusPrev|FocusFirst|FocusLast` and the pre/post
`focused_task` is `Some → Some` (the walk carries its own target; the
walk facts render the focused look). `Some → None` (retreat to the
switcher) and every other identity change keep the tail call. This is
a latch-free three-line guard computed from data `interpret` already
holds (`focus_before` is captured at entry). The same-board walk
regresses nothing (the tail never fired for it — no identity change).

### D-N1 — The notification surface: context, engagement posture, verb ids (US-6/W-6)

- **`ContextId::NotificationSurface`** (`"notification-surface"`),
  declared a total mask in the origin §7 letter. **Recorded posture
  deviation, scoped**: the surface is *ambient* — its rows persist
  until dismissed while the user keeps working — so a mask armed
  whenever rows are visible would paralyze every other keyboard
  journey (the doctrine bar is the regression bar; `ux_planning.org`
  §2.6). The mask is therefore **engagement-armed**: `Shell.
  notification_surface: bool` is true while rows are visible **and**
  the surface is engaged (armed by the surface's first walk stroke —
  ArrowUp/Down/d/s/m/c/t/o while rows are visible; disarmed by
  `MenuDismiss` closing it, the last row's exit, or rows emptying).
  The accepted plan's ACs are met on their own terms: AC-1's
  "surface mounted" journeys drive strokes before asserting; AC-4's
  totality is the keymap contract (ids bound in every preset), which
  is posture-independent. This is the one deliberate deviation from
  the §7 letter — flagged here for the operator to overturn at
  review; the stage-3 walkthrough drives the ambient case (a visible
  row while the user walks the board — keys must still route
  normally).
- **Per-verb ids** (candidates; the contract-suite totality oracle is
  the resolution authority — any preset collision is re-picked from
  the context's free set and recorded, never a dropped row, the C-5
  law): `NotificationDismiss` (`d`), `NotificationDone` (`e`),
  `NotificationSnooze` (`s`), `NotificationMute` (`m`),
  `NotificationCancel` (`c`), `NotificationTrackStart` (`t`),
  `NotificationOpen` (`o`). Walk rows: the r5 grammar verbatim —
  `MenuNext`/`MenuPrevious` (`Down`/`Up`) + `MenuDismiss` (`Escape`).
  Registry rows: `contexts: &[ContextId::NotificationSurface]`,
  `Category::Task`, `menu_group: None` (the AlarmWindow letter,
  actions.rs:1381). Preset rows in all three presets (presets.rs
  alarm blocks at ~366, ~696, ~904).
- **Routing = the seams' bodies, re-addressed by cursor**: each verb
  id's router arm resolves `notification_row(cursor)` and performs
  **exactly the seam's command** (`DismissReminder`;
  `SetTaskDone{done:true}`; `ReminderSnooze` over
  `reminder_target`; `ReminderMute` likewise; `TimerCancel` over
  `timer_target`; `TaskTrackStart` over `track_target`; open = focus
  + `TaskEdit`). `us6_s2` proves click ≡ key structurally: one
  fixture drives both the button seam and the verb id and asserts the
  sent commands equal (typed, no text).
- **Escape** = `MenuDismiss` closes the surface dismissing nothing
  (rows keep their delivery state) and restores per D-F1
  (census row 12). The mute-class `NotUndoable(ReminderSurface)`
  posture stands.
- The `focus_coverage.rs` exemption for
  `TouchArea:NotificationSurface::row-touch` **retires** in the same
  commit as the walk (its citation #724 closes with the PR).

### D-F5 — The title/description focus flags (US-5/W-5)

The task-view field **models** (`TaskViewModel` in `models.rs` /
the view-state derivation) gain `title-armed: bool` /
`description-armed: bool`, true when the walk's logical focus or the
arm path (F2/Return, the `TaskTitleEdit`/`TaskDescriptionEdit`
routes) targets the field on the viewed task. `task-view.slint`
binds the ring as
`title-stop.has-focus || task.title-armed ? Theme.focus-ring-width : 0`
(the description twin likewise). No theme work (R-3): the same
tokens, one flag per field, the census rider fixes any further
armed-field gaps under the same seam. Presentation test
`us5_s1_the_title_and_description_fields_carry_the_focus_flag`
asserts the flags (typed), per field.

## 4. Test catalogue (the origin §8 names; TDD order)

Journey tests in `tests/journeys.rs` (step-ID-named; the census §2
families are their fixtures):

| Test | Pins | Census legs |
|---|---|---|
| `us1_s1_picker_close_restores_the_invoking_field` | picker-family exit → origin restored (FocusFacts tuple + focus_item) | 1, 2, 5, 10, 14 (+15 twin) |
| `us1_s1_menu_close_restores_the_origin` | menu invoke/Escape → the invoking row | 7 |
| `us1_s1_overlay_escape_restores_the_origin` | Escape per overlay family | 3, 6, 8, 9, 11, 17, 18, 19, 20 |
| `us1_s2_vanished_origin_falls_to_the_container_anchor` | tombstoned task → row/stack header anchor; never no-control | all families' fallback branch |
| `us1_s3_commit_exit_restores_the_row_anchor` | commit exits (labels, walk) → the subject row | 4, 13 |
| `us2_s1_tab_wraps_inside_the_masked_context` | clamp wrap; nothing under the mask | all armed hosts |
| `us2_s2_single_control_context_degenerates_to_stay` | extent=1 stays | the degenerate host |
| `us3_s1_menu_open_lands_focus_on_the_menu` | mount focus on the rows; walk + invoke inside | 7 |
| `us4_s1_cross_board_walk_parks_focus_on_the_row` | board-crossing step → the walked row survives the tail | US-4 |
| `us5_s1_the_title_and_description_fields_carry_the_focus_flag` (tests/presentation.rs) | the flags per field | US-5 |
| `us6_s1_notification_walk_selects_and_clamps` | walk + clamp + pre-select | 12 |
| `us6_s2_notification_verb_id_matches_the_button_command` | structural command equality, click ≡ key, per verb | 12 |
| `us6_s3_notification_escape_closes_without_dispatch` | Escape = no command; restore fires | 12 |

Structural/contract tests: the keymap contract suite (the new ids in
all three presets, totality, snapshots regenerated — the per-preset
candidate keys in D-N1 are candidates until these resolve);
`tests/seam_inventory.rs` gains every new install; `us6_s2`'s
equality harness; the FocusFacts push/consume unit tests beside the
adapter. Existing tests that must **stand**: `focus_coverage.rs`
(both halves; minus the retired notification exemption),
`level_state`/`surfaces` chain tests (D-F1 composes, does not
replace), the insert-mode text-motion tests (US-2/AC-3), the whole
walk family (`walk_probe_tests.rs`), and the RW-W-N1 Escape-preset
positives in the keymap snapshots.

Kiosk gates: none new (origin §8). Time: no wall-clock-dependent
assertion here; nothing needs `tokio::time::pause()` — the walks are
stroke-driven (the law recorded, not exercised).

## 5. Commit sequencing (every commit standalone; census cells ride
the code commits citing the plan's US/W ids)

1. `test(ui): the focus-origin contract — census, FocusOrigin, the restore funnels` —
   D-F1 + the §2 census table (in this plan; the plan file is the
   census's home) + restore calls across every site + the us1_s1/s2/s3
   journeys + FocusFacts + seam inventory rows (US-1, W-1).
2. `feat(ui): the control walk clamps at the mask boundary` — D-W2 +
   us2_s1/us2_s2 + MaskWalkFacts + per-host extents (US-2, W-2).
3. `feat(ui): the action menu opens focused on its rows` — D-M3 +
   us3_s1 (US-3, W-3).
4. `fix(ui): the today walk parks focus on the walked row across boards` —
   D-H4 + us4_s1 (US-4, W-4; closes #670's fix half).
5. `feat(ui): the title and description fields carry the focus flag` —
   D-F5 + us5_s1 (US-5, W-5).
6. `feat(ui): the notification surface's keyboard grammar` — D-N1 +
   the context + ids + preset rows + walk + us6_s1..s3 + the keymap
   contract snapshot regeneration + the exemption retirement (US-6,
   W-6).
7. `docs(census): the round-8 focus family evidence` — the §9 census
   cells (the three `app.*` rows minted with R-1's scoping,
   `nav.item_walk` Today evidence, indicator evidence, notification
   keyboard evidence) if any remain after the code commits.

Gate steady state before handoff:
`scripts/ux-coverage.sh .artifacts/plans/2026-11-16-focus-restoration-ux-plan.md
--gate` — every W-1..W-6 row and every US-1..US-6 step token FOUND;
then `scripts/run-quality-gates.sh cargo nextest run --workspace`.

## 6. Risks & watch items for stage 2/3

- **D-N1's posture deviation** is the one place this chunk reads the
  accepted plan narrowly. Stage 3 must surface it in the walkthrough
  report; the operator may order the literal total mask instead.
- **`focus_coverage.rs` churn**: the clamp's armed hosts must not
  shift the unarmed whole-view walk order (the probe's walk half pins
  reading order). If a host's mask-focus changes its internal control
  order, the probe's floor re-baseline is a recorded event, not a
  silent retune.
- **Seam inventory size**: every new install (FocusFacts pushers are
  globals, not installs; `focus-task-row`/`focus-stack-header`/
  `focus-board-cell`/`mask-focus`/`menu-rows-focus-requested` are)
  bumps the counted total — both test sides update in the same
  commits.
- **The restore-before-view-scope ordering** (D-F1) touches the
  shared `apply_outcomes` tail — the highest-blast-radius line in the
  chunk. The us1 family is the net; the D15/D20 hand-off tests must
  stand.
- **Diff estimate**: ~450 lines src + ~150 slint + ~250 preset/registry
  + ~900 tests + census/docs ≈ **1.7–2.0k LOC** across ~7 commits.
