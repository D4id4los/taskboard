// SPDX-License-Identifier: AGPL-3.0-only

//! The action router (chunk decision D16, design §5 pipeline): a
//! pure, exhaustive `ActionId` → outcome mapping. No slint types, no
//! channels — the adapter sends [`DispatchOutcome::Command`] outcomes
//! over the mpsc and applies [`UiEffect`]s to the shell state.

use chrono::TimeDelta;
use taskboard_domain::{AdhocAlarmId, BoardId, ReminderRuleId, StackId, StateCommand, TaskId};

use crate::actions::{ActionId, entry as registry_entry};
use crate::presentation::ReminderTarget;

/// How far a banner snooze pushes an occurrence out (D4): one hour.
pub const SNOOZE_POSTPONE: TimeDelta = TimeDelta::hours(1);

/// The snooze choice flyout's span set (H4 D3, US-6/S2): hours first,
/// days after, ascending — O-1's "snooze in hours AND days". Every
/// reminder family snoozes through the same banner grammar (the ask
/// is tracker-side; per-habit spans stay #959's home). The keyboard
/// verb keeps [`SNOOZE_POSTPONE`]; the flyout adds choice, the key
/// grammar gains none.
pub const SNOOZE_SPANS: [TimeDelta; 4] = [
    TimeDelta::hours(1),
    TimeDelta::hours(4),
    TimeDelta::days(1),
    TimeDelta::days(3),
];

/// The typed span behind the flyout's index (the adapter's trust
/// boundary): an out-of-range index degrades to the keyboard verb's
/// fixed hop (the stale-index law — never a panic, never a negative
/// delta).
#[must_use]
pub fn snooze_span(index: usize) -> TimeDelta {
    SNOOZE_SPANS.get(index).copied().unwrap_or(SNOOZE_POSTPONE)
}

/// The vi mode (D17): two plain shell states — ordinary state, not a
/// subsystem (D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
}

/// Which task-detail priority field a picker intent addresses (UX R-3
/// D-1, #788): the two independent enum fields share one quick-list
/// grammar (distinct types — the domain's own ruling; the field, not
/// the vocabulary, discriminates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityField {
    Importance,
    Urgency,
}

/// The directional-focus family's four directions (J3 US-1, round 9
/// chunk 9). Pure data — the meaning of "left" is decided per view by
/// the adapter's directional table, never here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// UI-internal effects (design §5: "ui-internal actions → focus
/// moves, view switches, mode flips"). The adapter applies them to
/// the shell/window; `Exit` maps to the slint event-loop quit.
/// The notification surface's verbs (US-6/W-6, D-N1): the r5 alarm
/// grammar's per-row twins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationVerb {
    Dismiss,
    Done,
    Snooze,
    Mute,
    Cancel,
    TrackStart,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    Exit,
    OpenMenu,
    /// Open the jump-to search palette (#348, chunk 9). The adapter's
    /// interpretation table opens the flow; any other flow is inert
    /// (the modal discipline).
    OpenPalette,
    CloseMenu,
    /// Open the undo branch picker (UF2, #353 W-1): mount only —
    /// pure navigation, no command dispatch on open. The adapter
    /// refuses on kiosk (the structural exclusion, R-2).
    UndoBranchesOpen,
    /// Level-aware menu back (round-1 D6): a submenu steps back one
    /// level, the root closes.
    MenuBack,
    MenuNext,
    MenuPrevious,
    /// The notification surface's per-verb dispatch (US-6/W-6, D-N1):
    /// routed over the walk's selected row — the adapter applies the
    /// exact seam body the row's button callback runs (the click ≡
    /// key law).
    NotificationVerb(NotificationVerb),
    /// Enter the Today view (chunk 10, D-V2): subjectless, no engine
    /// command.
    FocusToday,
    /// Jump to the global Inbox (chunk 10, D-V7): read-side only — the
    /// interpretation quietly no-ops with no Inbox board.
    OpenInbox,
    /// Open the capture overlay (chunk 10, D-V6): any flow is replaced
    /// without committing (the XOR rule); typing rides `ViInsert`.
    BeginCapture,
    /// Toggle the concentration overlay (#496): open, or close when it
    /// already holds (the `focus.toggle` action). A kiosk dispatch
    /// no-ops in the adapter (the structural exclusion, D-F13).
    ToggleFocusMode,
    /// Open the time-entries overlay (r4 chunk 10, D-TS6): the
    /// adapter opens `Flow::TrackEntries` over the subject task;
    /// refuses on kiosk (the structural exclusion) and without a
    /// viewed task.
    TrackEntriesOpen,
    /// The entries overlay's Escape ladder (D-TS2): the adapter
    /// resolves it against the current flow — a focused field's
    /// Insert disengages first, then the form steps to the list, then
    /// the overlay closes.
    TrackEntryDismiss,
    /// Open the labels overlay (UX R-3 D-2, #789): the adapter opens
    /// `Flow::Labels` over the subject task; refuses on kiosk (the
    /// structural exclusion) and without a viewed task — the
    /// `TrackEntriesOpen` shape.
    LabelsOpen,
    /// The labels overlay's Escape ladder (D-2): the adapter
    /// resolves it against the current flow — a focused field's
    /// Insert disengages first, then the overlay closes.
    LabelsDismiss,
    /// The adapter closes the menu and re-dispatches the selected
    /// entry — bounded (the menu is closed first, no recursion).
    InvokeSelected,
    /// Open the recur row's inline pause editor (issue #506): a view
    /// intent over the subject task — the adapter flips the model's
    /// `recur-pause-open` flag; any other flow/state is inert.
    SeriesPauseOpen,
    /// The focused row's context-menu opener (US-7, W-8, #1032): the
    /// adapter opens the walk subject's context menu through the same
    /// openers the pointer seams use; the subjectless refusal lives in
    /// the arm (the uniform law). Pure presentation — no command.
    RowMenuRequested,
    /// The rule cursor's disarm chord (US-10, W-11, #1091): the
    /// adapter disarms the armed cursor (the latch composes with the
    /// armed-flow grammar) or surfaces the typed inert while
    /// disarmed. Pure view-state — no command.
    RowCursorDisarm,
    /// The break-rules roster's remove arm (US-9, W-10, #1066's
    /// roster half): the adapter toggles the armed subject over the
    /// roster cursor (the click ≡ key re-rooting of the pointer
    /// seam); the dispatch itself waits for Confirm. Nothing
    /// dispatches engine-side.
    BreakRuleRemoveArm,
    /// Arm the habit row's conversion confirm strip (issue #506): the
    /// adapter records the armed row; the dispatch itself waits for
    /// Confirm.
    HabitConvertArm,
    /// The armed conversion's cancel step (H2 D4, O-7 Q3): the
    /// adapter disarms the armed habit subject and re-projects the
    /// open menu (the confirm greys again); nothing dispatched.
    HabitConvertCancel,
    /// The habit editor's opener (H4 D1, US-5): the adapter opens
    /// the shared task-detail surface over the resolved habit
    /// subject — the `Activate` shape with a habit one-shot.
    HabitEdit,
    /// The habit history dialog's opener (HF3 W-8, US-7/S1): the
    /// adapter opens the read-role month-grid projection over the
    /// resolved habit subject; refuses on kiosk (R-3 — the dialog is
    /// unmountable) and without a subject. Zero commands.
    HabitHistoryOpen,
    /// Arm the staged habit remove (H4 D4, #944): the adapter
    /// records the armed subject; the dispatch waits for Confirm.
    HabitRemoveArm,
    /// The armed remove's cancel step: the adapter disarms and
    /// re-projects the open menu; nothing dispatched.
    HabitRemoveCancel,
    /// Arm the undo-family habit restore (W-3): staging touches no
    /// engine state — the roster's restorable habit rides the adapter
    /// flag; the dispatch waits for Confirm.
    HabitRestoreArm,
    /// The armed restore's cancel step: the adapter disarms and
    /// re-projects the open menu; nothing dispatched.
    HabitRestoreCancel,
    /// The create form's open/toggle (round-7 D-7.2): the adapter
    /// flips the armed flag — open when closed, discard-and-close
    /// when armed — only with an active board; kiosk stays inert
    /// (the action gate, D-T16).
    HabitCreateToggle,
    /// The habit pause editor's opener (HF1, W-3): the adapter flips
    /// the habits section's inline pause editor over the resolved
    /// habit subject (the `SeriesPauseOpen` shape).
    HabitPauseOpen,
    /// The quota popover's open intent (W-5, #961): the adapter arms
    /// the draft over the resolved subject (the focused row's fact —
    /// click ≡ key) and flips the window's popover flag; kiosk stays
    /// inert (R-3) and a subjectless dispatch surfaces the typed
    /// refusal (the T-19 twin).
    HabitQuotaEdit,
    /// Arm the recur row's conversion confirm strip (issue #506).
    SeriesConvertArm,
    /// Open the recurrence-rule editor overlay (round-7 D-7.1): the
    /// adapter opens `Flow::EditRecur` over the subject task, arms
    /// the one-shot target, and seeds the level's draft from the
    /// task's rule or the anchor; refuses on kiosk (the
    /// entries-overlay precedent) and without an open subject.
    SeriesEditorOpen,
    /// The stop-repeating strip's arm step (round-7 D-7.1): flips the
    /// editor level's `detach_armed` flag (the conversion-strip
    /// pattern); inert without the flow.
    SeriesDetachArm,
    /// The stop-repeating strip's cancel (round-7 D-7.1).
    SeriesDetachCancel,
    SetMode(Mode),
    // --- chunk-3 view intents (D6/D9): names state the function; the
    // adapter resolves them against the view-state model.
    /// Advance board focus in reading order (also the move-flow's
    /// target-cycle: the adapter's interpretation table decides).
    FocusNext,
    /// Retreat board focus in reading order.
    FocusPrev,
    /// The directional-focus step (J3 US-1, round 9 chunk 9): the
    /// interpretation is per view (the adapter's directional table) —
    /// focus movement, never task selection; during a board move the
    /// same directions are the move's kept arms (US-2).
    FocusDirectional(Direction),
    /// The selection-extend twin of the item walk (#875 RIDER): the
    /// route-level front for the printable-bound Shift rows. The
    /// adapter guards the effect to the Board view (the typed no-op
    /// elsewhere — the intercept's scoping).
    ExtendSelectionNext,
    ExtendSelectionPrev,
    /// Enter the next view in the one view cycle.
    NextView,
    /// Enter the previous view in the one view cycle.
    PrevView,
    /// Return to the board view and its scope.
    FocusBoard,
    /// `nav.show-task` (round-3 D23): act on the search palette's
    /// selected row — the Task retarget mounts the task's home view.
    /// Inert dispatched outside a palette flow (no preset binds it;
    /// the palette's Return is `menu.invoke`).
    ShowTask,
    /// The control-navigation walk (round-3 D25): the adapter
    /// re-injects the runtime's tab traversal (Shift-wrapped for the
    /// previous direction) under the walk's re-entrancy guard.
    ControlNext,
    ControlPrev,
    /// The scroll actions (round-3 D26): the adapter resolves the
    /// focused container per view and pages it (the `ScrollPageHint`
    /// global, or the settings facts two-way for the settings pane).
    /// Inert without a focused container.
    ScrollUp,
    ScrollDown,
    /// The picker keys (round-3 D25): applied before/aside the flow
    /// table (the menu-family pattern) — the picker mutates its own
    /// adapter state and refreshes; the date flow is Insert and the
    /// picker's own seams own the commit funnel.
    PickerKey(crate::due_edit::DatePickerKey),
    /// Open the inline create row. The target is the clicked column's
    /// stack (r4 D4) — `None` falls through to the keyboard's
    /// focused → selected → first resolution, byte-identical. The
    /// payload changes only *where* the flow opens, never the
    /// (flow → flow-after) outcome, so the settle matrix stays
    /// payload-blind (the `BoardSelect(_)` precedent).
    BeginCreate {
        target: Option<StackId>,
    },
    /// Open the inline create row for a new stack on the live board
    /// (works on an empty board — no target stack needed).
    BeginCreateStack,
    /// The primary-activation intent: open/commit whatever the focus
    /// and the active flow mean by "activate".
    Activate,
    /// Ask for move mode (a second request cancels it).
    RequestMove,
    /// Arm the viewed task's title editor (round-3 W-SM-d): the
    /// registry action behind `F2`/`Return`/the closed title's click.
    BeginTitleEdit,
    /// Arm the viewed task's date editor over `field` (keymap-parity
    /// plan US-3, D-4): the registry action behind the detail
    /// shortcuts — the adapter routes the round-3 D11
    /// `begin_date_edit` funnel (the row affordance's exact seam).
    BeginDateEdit {
        field: crate::view_state::DateField,
    },
    /// Arm the viewed task's description editor (keymap-parity plan
    /// US-3, D-4): the adapter routes the round-3 D15 arm exactly as
    /// the field's pointer affordance does.
    BeginDescriptionEdit,
    /// Focus the viewed task's always-on effort field (J3 US-4,
    /// round 9 chunk 11): a *focus* verb — it arms nothing and
    /// commits nothing; the M9 field's own seams keep owning the
    /// commit (commit-on-focus-loss law, the D-5 guard absorbing an
    /// untouched draft's blur).
    BeginEffortEdit,
    // --- chunk-7 board dimension (design §3.8): pure view state; the
    // adapter resolves them against the view-state model. Never a
    // `DispatchOutcome::Command`.
    /// Step to the next live board (canonical order, no wrap).
    BoardNext,
    /// Step to the previous live board.
    BoardPrevious,
    /// Select the parameterized board (`DispatchCtx::board_target`).
    BoardSelect(BoardId),
    /// Open the board bar's switcher flyout (UX R-4 D-4, #794): the
    /// target-free `BoardSelect` flow — the stroke arms the flyout,
    /// the row activation routes the same `BoardSelect` selection.
    /// The adapter owns the trigger (the facts-global nonce).
    OpenBoardBarFlyout,
    // --- round-1 chunk 2 (D18): the keyboard stack-width verbs —
    // pure view intents (the BoardNext precedent); the adapter
    // computes the live ceiling and routes to the view state.
    /// Widen the focused stack's column by one step.
    StackWider,
    /// Narrow the focused stack's column by one step.
    StackNarrower,
    // --- round 2 (D16–D21): the action-registry round-up verbs. All
    // pure view intents — target resolution is the view state's (the
    // `StackWider` precedent); never a `Command` except the reopen
    // gate, which is the `TaskComplete` machinery inverted.
    /// End the active move/stack-move flow without moving anything.
    CancelMove,
    /// Step the stack selection to the next stack (wraps).
    StackNext,
    /// Step the stack selection to the previous stack (wraps).
    StackPrev,
    /// Delete the focused or selected stack.
    StackDelete,
    /// Open the stack title editor.
    StackRename,
    /// Start the stack-move flow (toggle semantics mid-flow).
    StackMove,
    /// Open the new-board field.
    BoardCreate,
    /// Delete the current board (engine owns the refusals).
    BoardDelete,
    /// Open the board title editor.
    BoardRename,
    /// Arm the board color picker over the selected board (UX R-4
    /// D-1, #791) — the strip mounts from the flow.
    BoardColorEdit,
    /// The empty-board preset's dispatch (UX R-4 D-5, #795): the
    /// adapter's arm owns the empty-board gate.
    BoardDefaultPreset,
    /// The board ceremony verbs (O-5 parity chunk OP1, US-1/US-2/US-3):
    /// arm the consent strips over the selected board — the adapter's
    /// interpretation owns the arm/commit split and the reconcile
    /// preview request. The ceremony fences hold engine-side.
    BoardArchive,
    BoardReset,
    BoardReconcile,
    /// Focus the first card in reading order.
    FocusFirst,
    /// Focus the last card in reading order.
    FocusLast,
    // --- round-1 chunk 3 (D28b): the settings surface — a `ViewId`
    // excluded from the view cycle; the adapter toggles it.
    /// Open the settings view (or close it — toggle semantics).
    OpenSettings,
    /// Open the export dialog (the J2 export plan US-1, D-1): a
    /// pure presentation mount — the adapter seeds the dialog VM
    /// from the hand-over defaults; a kiosk dispatch is inert (R-2).
    ExportDialogOpen,
    /// Close the export dialog (W-1's close callback): nothing
    /// dispatches; the pending reply drops (US-2/S2's escape law).
    ExportDialogClose,
    /// The status panel's toggle request (OP3 W-1): the adapter owns
    /// the toggle and the D-3 snapshot-on-open query.
    OpenStatusPanel,
    // --- the plan-next-day surface (#622, D-4/D-3): the routed
    // reachability seam plus the placement interaction. All pure
    // view intents (never a parallel write path — the placement
    // rides `UpdateTask`'s landed masks); the adapter's
    // interpretation table owns them.
    /// Open the plan-next-day surface (or close it — toggle
    /// semantics; the Escape rung reuses this action). A held flow
    /// keeps it inert (the modal discipline: Escape out first).
    OpenPlanNextDay,
    /// Arm the placement form over the addressed task.
    PlanBeginPlace(TaskId),
    /// Open one of the task-detail priority pickers (UX R-3 D-1,
    /// #788) — the flag flips ride the adapter's owned state (the
    /// recur row's intents' shape); the slot commits are the view's
    /// own seam, never routed effects.
    PriorityPickerOpen(PriorityField),
    /// Commit the placement form: the adapter parses its buffers and
    /// routes the one `UpdateTask` (invalid input stays typed-inert).
    PlanCommitPlace,
    /// Swap the addressed placed box with the earlier neighbor.
    PlanBoxEarlier(TaskId),
    /// Swap the addressed placed box with the later neighbor.
    PlanBoxLater(TaskId),
    /// Unplace the addressed box (back to the pool).
    PlanRemoveBox(TaskId),
    // --- the review walk (DR-7 chunk 3, #638): the routed opener plus
    // one carrier enum for the in-walk verbs (keeps the effect enum
    // flat — plan R4). All pure view intents; the adapter's table owns
    // them, the kiosk gate empties them.
    /// Open the review walk (the `review.open` action; the Today
    /// card's activation). The adapter derives the reference date and
    /// refuses on kiosk (the structural exclusion).
    OpenReviewWalk,
    /// An in-walk verb, resolved against `Flow::ReviewWalk`'s stage.
    ReviewWalk(crate::router::WalkIntent),
}

/// The walk's verb vocabulary (DR-7 D-J9; plan D-W6): one carrier
/// enum so the effect enum grows two variants, not fifteen. The row
/// verbs resolve adapter-side against the flow's stage and the pass's
/// rows (typed inert outside their stage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkIntent {
    CursorUp,
    CursorDown,
    /// The Escape ladder's stage rung (J4 D-6): the insert rung and the
    /// child rung first, then the abort-confirm arm (or the Entry
    /// stage's one silent exit).
    Back,
    /// The Prev control (the dialog button row): one visited stage
    /// back.
    Prev,
    Advance,
    /// The plan-next-day choice out of Carry (J4 US-3/S1): mounts the
    /// embedded `ReviewPlanner` child.
    Planner,
    InboxFile,
    InboxKeep,
    FileTargetPrev,
    FileTargetNext,
    FileCommit,
    CarryTomorrow,
    CarryPickDate,
    CarryLeave,
    CarryDateCommit,
    Record,
    Finish,
    /// The completion summary's exit (J4 D-8): closes the walk and
    /// restores focus.
    Complete,
    /// The abort arm (the dialog host's outside click; J4 R-2): the
    /// Entry stage closes silently, any other stage arms the confirm.
    AbortArm,
    /// The abort confirm's accept: close + focus restore. Decisions
    /// already made are applied `StateCommand`s — never undone.
    AbortAccept,
    /// The abort confirm's dismiss ("Keep reviewing"): the confirm
    /// closes, the walk stays exactly as it was.
    AbortDismiss,
}

/// The effect classes the flow-settle matrix ranges over (round-3
/// W-SM-b; the audit report §6 is the normative table). Exhaustive
/// over [`UiEffect`] — adding a variant without a class is a compile
/// error, and `Adapter::resolve` matches every class without a
/// wildcard, so no `(Flow, effect)` pair can be silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectClass {
    /// View/read-side jumps and the focus walk.
    Nav,
    /// The board/stack lifecycle verbs.
    Lifecycle,
    /// The begin intents (`BeginCreate`, `BeginCreateStack`,
    /// `RequestMove`).
    Begin,
    /// The palette overlay request.
    OpenPalette,
    /// The concentration-overlay toggle request (#496): settle-then-run
    /// like the palette request; the adapter owns the toggle.
    FocusOverlay,
    /// The time-entries overlay family (r4 chunk 10): the open
    /// request settle-then-runs (the `FocusOverlay` shape); the
    /// dismissal is the modal's own exit arm.
    TrackOverlay,
    /// The labels overlay's intents (UX R-3 D-2, #789) — the
    /// [`EffectClass::TrackOverlay`] class's twin.
    LabelsOverlay,
    /// The capture overlay request (the one replace-without-commit).
    BeginCapture,
    /// The edit-flow's own commit/discard verbs (`Activate`,
    /// `SetMode`).
    Edit,
    /// The move-flow cancel verb.
    Cancel,
    /// The menu family (applied in `apply_effect`, before the table).
    Menu,
    /// The process-exit verb (applied in `apply_effect`).
    System,
    /// The title editor's arm verb (round-3 W-SM-d).
    Title,
    /// The picker keys: applied in `apply_effect`, before the table
    /// (the menu-family pattern) — unreachable in the resolver.
    Picker,
    /// The plan-next-day family (#622): the surface is a deliberate
    /// workspace — a held flow keeps every planner intent inert (the
    /// modal discipline verbatim; Escape out of the flow first), so
    /// the resolver cell is a named inert, not settle-then-run.
    PlanSurface,
    /// The review walk family (#638): the opener settle-then-runs (the
    /// overlay-family shape — a foreign flow settles, then the walk
    /// opens over it); the walk's own intents are its kept arms.
    ReviewWalk,
}

impl UiEffect {
    /// The matrix class of this effect (audit §6).
    #[must_use]
    pub const fn class(&self) -> EffectClass {
        match self {
            Self::NextView
            | Self::PrevView
            | Self::FocusBoard
            | Self::ShowTask
            | Self::ControlNext
            | Self::ControlPrev
            | Self::ScrollUp
            | Self::ScrollDown
            | Self::BoardNext
            | Self::BoardPrevious
            | Self::BoardSelect(_)
            | Self::FocusToday
            | Self::OpenInbox
            | Self::OpenSettings
            | Self::ExportDialogOpen
            | Self::ExportDialogClose
            | Self::OpenStatusPanel
            | Self::FocusNext
            | Self::FocusPrev
            | Self::FocusDirectional(_)
            | Self::ExtendSelectionNext
            | Self::ExtendSelectionPrev
            | Self::FocusFirst
            | Self::FocusLast
            | Self::StackNext
            | Self::StackPrev
            | Self::StackWider
            | Self::StackNarrower
            // The habit trio's + create form's intents (issue #506;
            // round-7 D-7.2) and the recur editor's intents (round-7
            // D-7.1) are flag flips over the subject: no flow moves,
            // no command routes — the nav bucket's contract (the
            // adapter's interpretation table owns them; no shell
            // state moves).
            | Self::SeriesPauseOpen
            | Self::RowMenuRequested
            | Self::RowCursorDisarm
            | Self::BreakRuleRemoveArm
            | Self::HabitPauseOpen
            | Self::HabitConvertArm
            | Self::HabitConvertCancel
            | Self::HabitRemoveArm
            | Self::HabitRemoveCancel
            | Self::HabitRestoreArm
            | Self::HabitRestoreCancel
            // The H4 editor opener joins the view-intent bucket (the
            // settle-then-open cell: a foreign flow settles first,
            // then the surface opens over the resolved subject).
            | Self::HabitEdit
            // The HF3 history opener rides the same view-intent
            // bucket (the read-role dialog's mount is a flag flip —
            // no flow moves, no command routes).
            | Self::HabitHistoryOpen
            | Self::HabitCreateToggle
            | Self::HabitQuotaEdit
            | Self::SeriesConvertArm
            | Self::SeriesEditorOpen
            | Self::SeriesDetachArm
            | Self::SeriesDetachCancel
            // The priority pickers' open intents (UX R-3 D-1): the
            // same flag-flip class — no flow moves, no command
            // routes.
            | Self::PriorityPickerOpen(_)
            // The board bar flyout's open request (UX R-4 D-4, #794):
            // a pure view intent — the adapter owns the trigger.
            | Self::OpenBoardBarFlyout => EffectClass::Nav,
            Self::StackDelete
            | Self::StackRename
            | Self::StackMove
            | Self::BoardCreate
            | Self::BoardDelete
            | Self::BoardRename
            | Self::BoardColorEdit
            | Self::BoardDefaultPreset
            | Self::BoardArchive
            | Self::BoardReset
            | Self::BoardReconcile => EffectClass::Lifecycle,
            Self::BeginCreate { .. } | Self::BeginCreateStack | Self::RequestMove => {
                EffectClass::Begin
            }
            Self::OpenPalette => EffectClass::OpenPalette,
            Self::ToggleFocusMode => EffectClass::FocusOverlay,
            Self::TrackEntriesOpen | Self::TrackEntryDismiss => EffectClass::TrackOverlay,
            Self::LabelsOpen | Self::LabelsDismiss => EffectClass::LabelsOverlay,
            Self::OpenPlanNextDay
            | Self::PlanBeginPlace(_)
            | Self::PlanCommitPlace
            | Self::PlanBoxEarlier(_)
            | Self::PlanBoxLater(_)
            | Self::PlanRemoveBox(_) => EffectClass::PlanSurface,
            Self::OpenReviewWalk | Self::ReviewWalk(_) => EffectClass::ReviewWalk,
            Self::BeginCapture => EffectClass::BeginCapture,
            Self::Activate | Self::SetMode(_) => EffectClass::Edit,
            // The detail editors' arm verbs (keymap-parity plan D-4)
            // share the Begin cell: settle-then-run — the funnel
            // settles a live flow first, then arms (dead buttons
            // cannot exist).
            Self::BeginDateEdit { .. } | Self::BeginDescriptionEdit
            | Self::BeginTitleEdit
            // The effort field's focus opener (J3 US-4, chunk 11)
            // shares the Begin cell: settle-then-run — the funnel
            // settles a live flow first, then focuses.
            | Self::BeginEffortEdit => EffectClass::Begin,
            Self::PickerKey(_) => EffectClass::Picker,
            Self::CancelMove => EffectClass::Cancel,
            Self::OpenMenu
            | Self::UndoBranchesOpen
            | Self::CloseMenu
            | Self::MenuBack
            | Self::MenuNext
            | Self::MenuPrevious
            | Self::InvokeSelected
            // The notification verbs ride the menu family's letter:
            // applied in `apply_effect` before the table (the
            // engaged context masks the chain; the walk arms live
            // beside it).
            | Self::NotificationVerb(_) => EffectClass::Menu,
            Self::Exit => EffectClass::System,
        }
    }
}

/// What dispatching an action produced (D16).
#[derive(Debug, Clone, PartialEq)]
pub enum DispatchOutcome {
    /// A domain intent for the engine (via the command mpsc).
    Command(StateCommand),
    /// A UI-internal effect for the shell/window.
    UiEffect(UiEffect),
    /// The action is reserved or chunk-3 staged: an absorbed no-op
    /// that reads as disabled in the registry (§5.8).
    Disabled,
    /// The action is enabled but needs a focused entity that is not
    /// there (chunk-2 sessions have no views, D16).
    NoTarget,
}

/// The timers form's parsed draft (DR-3/3, D-P10): the adapter owns
/// it between the seam's parse and the routed dispatch (the
/// `rule_add_offset` one-shot pattern).
#[derive(Debug, Clone, PartialEq)]
pub struct TimerDraft {
    pub label: String,
    pub when: taskboard_domain::AdhocWhen,
    /// The escalation rung (r4 D-T7): the GUI's insistence picker
    /// projects the two-rung enum's alarm bit; the default pick mints
    /// `Normal`.
    pub insistence: taskboard_domain::Insistence,
}

/// The habit create form's commit payload (round-7 D-7.2): the trust
/// boundary's minted triple — the section's board (resolved fresh at
/// commit, never cached at arm), the trimmed title, and the quota
/// target through the `NonZeroCount` mint.
#[derive(Debug, Clone, PartialEq)]
pub struct HabitCreateDraft {
    pub board: taskboard_domain::BoardId,
    pub title: String,
    pub target: taskboard_domain::HabitTarget,
}

/// The quota popover's one-shot draft (W-5, #961): the addressed
/// habit and the commit target — the new quota over the habit's
/// **current** period kind (the inline edit is a one-number edit; the
/// kind is locked, UX plan §2 `habit.quota_edit`). The adapter mints it
/// at the popover's commit seam (the trust boundary); the internal
/// `HabitQuotaCommit` verb consumes it (`NoTarget` without one — the
/// stale/disarmed popover commits nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct HabitQuotaDraft {
    pub habit: taskboard_domain::HabitId,
    pub target: taskboard_domain::HabitTarget,
}

/// The habit editor's staged draft (H4 D5): the field mirrors the
/// editor rows push, and the parses live at the trust boundary — the
/// count mints through `NonZeroCount`, the month day through
/// `MonthDay` (0 and >31 unrepresentable), the time through the due
/// editor's lenient parser. The commit computes the sparse
/// `HabitChanges` against the stored row (None = untouched, clocks
/// preserved).
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // the mirrors' validity/set/edited
// flavours are the draft's whole contract (the create form's shape)
pub struct HabitEditDraft {
    pub habit: taskboard_domain::HabitId,
    pub title: String,
    pub description: String,
    pub target: taskboard_domain::HabitTarget,
    pub quota_count: String,
    pub quota_valid: bool,
    pub time_text: String,
    pub time_set: bool,
    pub time_valid: bool,
    pub time_edited: bool,
    pub month_day: String,
    pub month_valid: bool,
    pub day_edited: bool,
    pub anchor_time: Option<chrono::NaiveTime>,
    pub anchor_day: Option<taskboard_domain::habit::AnchorDay>,
    // ---- HF4 (W-6/W-7): the schedule picker + the reminder override --
    /// The schedule picker's raw selection bits (bit n = n days from
    /// Monday). The draft deliberately holds the *raw* mask — a
    /// mid-edit empty selection is a legal draft state whose commit
    /// the save gate blocks (US-5 AC-3: the empty set is a typed
    /// invalid draft, not a validate-and-forget).
    pub schedule_bits: u8,
    /// The schedule picker's edited bit (the sparse-changeset name).
    pub schedule_edited: bool,
    /// The reminder override's edit grammar: `reminder_edited` names
    /// the section touched; `reminder_clear` is the clear leg (a
    /// `Some(None)` changeset value); the four field mirrors mint the
    /// override at commit.
    pub reminder_edited: bool,
    pub reminder_clear: bool,
    pub reminder_base: usize,
    pub reminder_offset: String,
    pub reminder_time: String,
    pub reminder_snooze: String,
}

impl HabitEditDraft {
    /// The draft seeded from the stored row (the editor opens
    /// showing the habit as it is).
    #[must_use]
    pub fn from_habit(habit: &taskboard_domain::habit::Habit) -> Self {
        Self {
            habit: habit.id,
            title: habit.title.clone(),
            description: habit.description.clone(),
            target: habit.target,
            quota_count: habit.target.quota().get().to_string(),
            quota_valid: true,
            time_text: habit
                .anchor_time
                .map_or(String::new(), crate::due_edit::format_time_hm),
            time_set: habit.anchor_time.is_some(),
            time_valid: true,
            time_edited: false,
            month_day: match habit.anchor_day {
                Some(taskboard_domain::habit::AnchorDay::MonthDay(day)) => day.get().to_string(),
                _ => String::new(),
            },
            month_valid: true,
            day_edited: false,
            anchor_time: habit.anchor_time,
            anchor_day: habit.anchor_day,
            schedule_bits: habit
                .scheduled_days
                .map_or(0, taskboard_domain::habit::WeekdaySet::bits),
            schedule_edited: false,
            reminder_edited: false,
            reminder_clear: false,
            reminder_base: match habit.reminder {
                Some(ovr) if ovr.base == taskboard_domain::habit::NudgeBase::PeriodStart => 1,
                _ => 0,
            },
            reminder_offset: habit
                .reminder
                .map_or(String::new(), |ovr| ovr.offset_days.to_string()),
            reminder_time: habit
                .reminder
                .and_then(|ovr| ovr.time)
                .map_or(String::new(), crate::due_edit::format_time_hm),
            reminder_snooze: habit
                .reminder
                .map_or(String::new(), |ovr| match ovr.snooze {
                    taskboard_domain::habit::HabitSnooze::Hours(h) => format!("{h}h"),
                    taskboard_domain::habit::HabitSnooze::Days(d) => format!("{d}d"),
                }),
        }
    }

    /// The weekday picker's gated visibility (US-5 AC-4, R-6 — the
    /// fold's gate): the picker mounts for PerWeek/PerMonth habits
    /// only. A daily habit happens *every day*; the field is absent
    /// by construction, not hidden-by-convenience.
    #[must_use]
    pub fn schedule_picker_visible(&self) -> bool {
        !matches!(self.target.kind(), taskboard_domain::PeriodKind::Day)
    }

    /// The schedule picker's checked slots (Monday-first; the slint
    /// `[bool]` model's mirror).
    #[must_use]
    pub fn schedule_day_checked(&self, index: usize) -> bool {
        self.schedule_bits & (1_u8 << index) != 0
    }

    /// The adapter seam's toggle body (Monday-first slot index).
    pub fn toggle_schedule_slot(&mut self, index: usize) {
        let bit = 1_u8 << (index % 7);
        self.schedule_bits ^= bit;
        self.schedule_edited = true;
    }

    /// The test twin: this draft's stored-row shape (same id, target)
    /// for the fold tests' `changes`/`can_save` oracles.
    #[cfg(test)]
    pub(crate) fn stored_twin(self) -> taskboard_domain::habit::Habit {
        taskboard_domain::habit::Habit {
            id: self.habit,
            board: taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
            title: self.title,
            description: self.description,
            target: self.target,
            order: 0,
            anchor_time: self.anchor_time,
            anchor_day: self.anchor_day,
            paused_until: None,
            scheduled_days: taskboard_domain::habit::WeekdaySet::from_bits(self.schedule_bits),
            reminder: None,
            clocks: taskboard_domain::habit::HabitClocks::all(chrono::DateTime::UNIX_EPOCH),
            continued_from: None,
            deleted: false,
            by: None,
        }
    }

    /// The override editor's parse at the trust boundary: the minted
    /// `HabitReminderOverride` while every field holds. A blank time
    /// field mints `time: None` — the anchor-time-only posture (#1212,
    /// operator ruling comment 7079171625); the retired 09:00 fallback
    /// is never minted here, and a non-blank unparseable field refuses
    /// the mint (the save gate greys).
    #[must_use]
    fn minted_reminder_override(&self) -> Option<taskboard_domain::habit::HabitReminderOverride> {
        let base = match self.reminder_base {
            0 => taskboard_domain::habit::NudgeBase::NudgeDay,
            _ => taskboard_domain::habit::NudgeBase::PeriodStart,
        };
        let offset_days = self.reminder_offset.trim().parse::<i32>().ok()?;
        let time_text = self.reminder_time.trim();
        let time = if time_text.is_empty() {
            None
        } else {
            Some(crate::due_edit::parse_time_lenient(time_text)?)
        };
        let snooze_text = self.reminder_snooze.trim();
        let digits = snooze_text.trim_end_matches(['h', 'H', 'd', 'D']);
        let n = digits.parse::<u8>().ok()?;
        // The mint is the zero gate (#1211): the save gate greys
        // because the domain mint refused, not because of a locally
        // duplicated predicate. Identical behavior for n >= 1.
        let snooze = match snooze_text.chars().next_back() {
            Some('h' | 'H') => taskboard_domain::habit::HabitSnooze::hours(n),
            Some('d' | 'D') => taskboard_domain::habit::HabitSnooze::days(n),
            _ => return None,
        }?;
        Some(taskboard_domain::habit::HabitReminderOverride {
            base,
            offset_days,
            time,
            snooze,
        })
    }

    /// The override section's validity (the save gate's input): an
    /// untouched section is valid (untouched = untouched); an edited
    /// one must mint — the clear leg is always valid.
    #[must_use]
    pub fn reminder_valid(&self) -> bool {
        !self.reminder_edited || self.reminder_clear || self.minted_reminder_override().is_some()
    }

    /// The `SegmentedControl` index (0 day / 1 week / 2 month).
    #[must_use]
    pub fn quota_kind_index(&self) -> usize {
        match self.target.kind() {
            taskboard_domain::PeriodKind::Day => 0,
            taskboard_domain::PeriodKind::Week => 1,
            taskboard_domain::PeriodKind::Month => 2,
        }
    }

    /// The weekday picker's selected slot (-1 = none set).
    #[must_use]
    pub fn weekday_index(&self) -> i32 {
        match self.anchor_day {
            Some(taskboard_domain::habit::AnchorDay::Weekday(wd)) => {
                i32::try_from(wd.num_days_from_monday()).unwrap_or(-1)
            }
            _ => -1,
        }
    }

    /// The count mint at the trust boundary (the edited text; an
    /// unparseable or zero count is invalid — the save gate stays
    /// greyed).
    #[must_use]
    pub(crate) fn parsed_count(&self) -> Option<taskboard_domain::NonZeroCount> {
        self.quota_count
            .trim()
            .parse::<u32>()
            .ok()
            .and_then(|n| taskboard_domain::NonZeroCount::new(n).ok())
    }

    /// The target the quota row names (only with a valid count).
    fn drafted_target(&self) -> Option<taskboard_domain::HabitTarget> {
        let count = self.parsed_count()?;
        Some(match self.quota_kind_index() {
            0 => taskboard_domain::HabitTarget::PerDay(count),
            1 => taskboard_domain::HabitTarget::PerWeek(count),
            _ => taskboard_domain::HabitTarget::PerMonth(count),
        })
    }

    /// The sparse changeset vs the stored row (D5): every field the
    /// draft moved names itself, the untouched stay `None` (clocks
    /// preserved). The quota row carries the kind change with a
    /// now-mismatched day inert (no silent destructive clearing).
    #[must_use]
    pub fn changes(
        &self,
        stored: &taskboard_domain::habit::Habit,
    ) -> taskboard_domain::HabitChanges {
        let mut changes = taskboard_domain::HabitChanges {
            continued_from: None,
            ..taskboard_domain::HabitChanges::default()
        };
        if self.title != stored.title {
            changes.title = Some(self.title.clone());
        }
        if self.description != stored.description {
            changes.description = Some(self.description.clone());
        }
        if let Some(target) = self.drafted_target()
            && target != stored.target
        {
            changes.target = Some(target);
        }
        if self.time_edited {
            changes.anchor_time = Some(self.anchor_time);
        }
        if self.day_edited {
            changes.anchor_day = Some(self.anchor_day);
        }
        // HF4 rows (W-6/W-7): the picker commits the minted set (a
        // set replaces; clearing = the full seven-day set via `all` —
        // the picker has no empty commit, the gate blocks it), the
        // override section commits the double option (the clear leg
        // rides `reminder_clear`).
        if self.schedule_edited
            && let Some(set) = taskboard_domain::habit::WeekdaySet::from_bits(self.schedule_bits)
        {
            changes.scheduled_days = Some(set);
        }
        if self.reminder_edited {
            changes.reminder = Some(if self.reminder_clear {
                None
            } else {
                self.minted_reminder_override()
            });
        }
        changes
    }

    /// The save gate (D5's empty-dispatch law): every parse holds AND
    /// the changeset names at least one field.
    #[must_use]
    pub fn can_save(&self, stored: &taskboard_domain::habit::Habit) -> bool {
        let parses_hold = self.quota_valid
            && self.time_valid
            && self.month_valid
            && self.reminder_valid()
            && (self.quota_count.trim().is_empty() || self.parsed_count().is_some());
        // US-5 AC-3: an edited-but-empty schedule selection blocks the
        // commit (the typed invalid draft — never a minted empty set).
        let schedule_holds = !self.schedule_edited || self.schedule_bits != 0;
        parses_hold && schedule_holds && !self.changes_is_empty(stored)
    }

    fn changes_is_empty(&self, stored: &taskboard_domain::habit::Habit) -> bool {
        let changes = self.changes(stored);
        changes.title.is_none()
            && changes.description.is_none()
            && changes.target.is_none()
            && changes.anchor_time.is_none()
            && changes.anchor_day.is_none()
            && changes.scheduled_days.is_none()
            && changes.reminder.is_none()
    }
}

/// The break-rule form's commit payload (T-14, #797): the trust
/// boundary's minted triple — the roaming board (resolved fresh at
/// commit from the section's host scope, never cached at arm), the
/// trimmed label, and the threshold through the `WorkThreshold` mint
/// (zero/negative unrepresentable).
#[derive(Debug, Clone, PartialEq)]
pub struct BreakRuleDraft {
    pub board: taskboard_domain::BoardId,
    pub label: String,
    pub threshold: taskboard_domain::WorkThreshold,
    /// The escalation rung (the timer form's convention): the GUI's
    /// insistence picker projects the two-rung enum's alarm bit.
    pub insistence: taskboard_domain::Insistence,
}

/// The focused-entity seam (D16): chunk 2's shell has no views, so
/// real sessions carry `None`s (keyboard task verbs resolve to
/// [`DispatchOutcome::NoTarget`]); chunk 3's focus architecture sets
/// them for real.
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // one flag per synced router gate (the shell's house shape)
pub struct DispatchCtx {
    pub focused_task: Option<TaskId>,
    pub focused_stack: Option<StackId>,
    /// The Add-task ghost's clicked column (r4 D4): set by the adapter
    /// when a ghost is clicked; `task.create` routes its
    /// `BeginCreate` payload from it (a keyboard dispatch carries
    /// none — the selection chain decides).
    pub create_target: Option<StackId>,
    /// The banner row's concrete reminder target (D4): set by the
    /// adapter when a banner row is activated; snooze/mute route from
    /// it (`NoTarget` without one).
    pub reminder_target: Option<ReminderTarget>,
    /// The board-bar segment's board (design §3.8): set by the adapter
    /// when a segment is activated; `board.select` routes from it
    /// (`NoTarget` without one — a menu invocation carries no target).
    pub board_target: Option<BoardId>,
    /// The rule-list row's rule (chunk 6, D11): set by the adapter when
    /// a row's remove affordance activates; `reminder.rule-remove`
    /// routes from it (`NoTarget` without one).
    pub rule_target: Option<ReminderRuleId>,
    /// The rule-list add entry's parsed timing input (chunk 6, D11;
    /// r4 D30 widens it to the three-form grammar): set by the adapter
    /// (preset or custom); `reminder.rule-add` routes from it together
    /// with the focused task (`NoTarget` without both).
    pub rule_add_when: Option<crate::presentation::RuleWhen>,
    /// The today day window (chunk 10 D4), set by the adapter per
    /// dispatch from its held `(day_start_time, offset)` pair — state-
    /// derived, never consumed; routes the today verbs' focus/window
    /// gate (`NoTarget` without focus or window).
    pub day_window: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    /// The today window's last covered instant (#893, the chunk plan
    /// D3.2): set by the adapter per dispatch from
    /// `days::last_covered_instant(day_window.1)` — the window-derived
    /// stamp fact, never `±24h` arithmetic and never the raw
    /// exclusive end; `task.due-today` and `task.schedule-today`
    /// commit it (`NoTarget` without it, the `day_window` backstop
    /// shape).
    pub today_last_instant: Option<chrono::DateTime<chrono::Utc>>,
    /// The next day window's end (UX-R8 chunk-5 D-3, US-1/S2): set by
    /// the adapter per dispatch from `days::next_day_window` — the
    /// landed day law, never `+24h` arithmetic;
    /// `task.schedule-tomorrow` commits to it.
    pub tomorrow_window_end: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether the focused task carries a scheduled day (UX-R8 D-3):
    /// `task.unschedule`'s route gate, synced by the adapter from the
    /// focused task (the `focused_task_done` shape).
    pub focused_task_scheduled: bool,
    /// The timers form's parsed draft (DR-3/3, D-P10): set by the
    /// adapter when the form commits; `timer.create` routes from it
    /// (`NoTarget` without one).
    pub timer_draft: Option<TimerDraft>,
    /// The timers row's alarm (DR-3/3, D-I8): set by the adapter when
    /// a cancel affordance activates (section row or awaiting row);
    /// `timer.cancel` routes from it (`NoTarget` without one).
    pub timer_target: Option<AdhocAlarmId>,
    /// The break-rule form's parsed draft (T-14, #797): set by the
    /// adapter when the form commits; `breakrule.create` routes from
    /// it (`NoTarget` without one).
    pub break_rule_draft: Option<BreakRuleDraft>,
    /// The alarm window's selected row (T-22, #805): set by the
    /// window's lanes when a keyboard verb dispatches; the three ids
    /// route the same commands the buttons build (the shared
    /// `presentation::alarm_command` constructor — `NoTarget`
    /// without a row).
    pub alarm_row: Option<crate::presentation::AlarmRow>,
    /// The armed break-rule row's one-shot target (T-14, #797): the
    /// arm and confirm seams resolve it through the roster's id
    /// table; `breakrule.remove`/`-confirm` route from it (`NoTarget`
    /// without one).
    pub break_rule_target: Option<taskboard_domain::BreakRuleId>,
    /// The entries form's parsed draft (r4 chunk 10, D-TS6): filled at
    /// the adapter's commit seam only when the live validity table
    /// passes; `task.track-entry-log`/`-adjust` route from it
    /// (`NoTarget` without one).
    pub track_entry_draft: Option<crate::presentation::TrackEntryDraft>,
    /// The armed entry row's one-shot target: row-activate (edit
    /// seed) and the delete confirm resolve through it (`NoTarget`
    /// without one).
    pub track_entry_target: Option<taskboard_domain::TimeEntryId>,
    /// The notification row's task (DR-5/4, D-TU5): set by the adapter
    /// when a row's Start Task affordance activates; `task.track-start`
    /// routes from it, falling back to the focused task (the context
    /// menu's and the detail button's seam — every card-focused
    /// invocation already addresses the focused task; `NoTarget` when
    /// neither is set).
    pub track_target: Option<TaskId>,
    /// The habits row's habit (DR-6/6, D-P13): set by the adapter when
    /// a row's +1 affordance activates; `habit.log` routes from it
    /// (`NoTarget` without one).
    pub habit_target: Option<taskboard_domain::HabitId>,
    /// The −1 recovery's one-shot row target (UX R-2 D-3, #786):
    /// `(habit, current window, current count)` — what you click is
    /// what decrements. The router owns the arithmetic: the dispatch
    /// carries `count: n − 1`; count 0 is the typed `NoTarget`.
    pub habit_undo: Option<(
        taskboard_domain::HabitId,
        taskboard_domain::WindowStart,
        u32,
    )>,
    /// The reorder's resolved move (DR-6/6; #1198 chunk A1 D1): set
    /// by the adapter when a reorder affordance activates — the habit
    /// plus a **target gap index** into the section's live sequence
    /// (the engine resolves the orders); `habit.move` routes
    /// `HabitReorder` from it (`NoTarget` without one).
    pub habit_move: Option<(taskboard_domain::HabitId, usize)>,
    /// The habit menu's reorder pair (H2 D4): both slots' target
    /// indices computed at menu open from the same row source the
    /// section renders; `habit.move-up`/`habit.move-down` route
    /// `HabitReorder` from theirs (`NoTarget` without one).
    pub habit_move_up: Option<(taskboard_domain::HabitId, usize)>,
    pub habit_move_down: Option<(taskboard_domain::HabitId, usize)>,
    /// The Today compact-move pair's one-shot subject (US-14, #1031):
    /// the focused task row, minted at the R-8 kind resolution (the
    /// shared Shift+Up/Down stroke flipped to its task twins).
    /// `task.move-up`/`task.move-down` route `StateCommand::TaskMoveUp`
    /// /`TaskMoveDown` from it (`NoTarget` without one).
    pub task_move: Option<TaskId>,
    /// The repair affordance's resolved target (issue #506, plan D-7):
    /// the `(habit, window, post-count)` triple presentation's
    /// `habit_repair_target` computed — what you click is what
    /// increments; `habit.repair` routes `AdjustHabitLog` from it
    /// (`NoTarget` without one).
    pub habit_repair: Option<(
        taskboard_domain::HabitId,
        taskboard_domain::WindowStart,
        u32,
    )>,
    /// The armed convert row's habit: `habit.convert-to-series` routes
    /// `ConvertHabitToSeries` from it together with the pattern draft
    /// (`NoTarget` without either).
    pub habit_convert_target: Option<taskboard_domain::HabitId>,
    /// The staged remove's armed subject (H4 D4): the confirm routes
    /// `DeleteHabit` from it (`None` = unarmed/stale — `NoTarget`).
    pub habit_remove_target: Option<taskboard_domain::HabitId>,
    /// The undo-family restore's one-shot armed subject (W-3).
    pub habit_restore_target: Option<taskboard_domain::HabitId>,
    /// The pause editor's one-shot subject (HF1, W-3): the commit and
    /// resume routes read it (`None` = stale — `NoTarget`).
    pub habit_pause_target: Option<taskboard_domain::HabitId>,
    /// The create form's one-shot payload (round-7 D-7.2): the exact
    /// `CreateHabit` triple the confirm seam minted (board resolved
    /// fresh at commit, the minted quota target) — `habit.create-commit`
    /// consumes it, then it is gone (the one-shot take law).
    pub habit_create_draft: Option<HabitCreateDraft>,
    /// The quota popover's one-shot draft (W-5, #961): minted at the
    /// commit seam, consumed by the `HabitQuotaCommit` dispatch
    /// (`NoTarget` without one — the one-shot take law).
    pub habit_quota_draft: Option<HabitQuotaDraft>,
    /// The conversion's pattern draft (issue #506, D-2): the quota's
    /// default mapping the adapter computed from the row; consumed by
    /// the confirm dispatch.
    pub pattern_draft: Option<taskboard_domain::RecurPattern>,
    /// The recur row's subject task: the pause/convert verbs' one-shot
    /// target (`NoTarget` without one).
    pub series_target: Option<TaskId>,
    /// The pause editor's parsed inclusive date: `series.pause-commit`
    /// routes `PauseSeries` from it (`NoTarget` without one).
    pub pause_draft: Option<chrono::NaiveDate>,
    /// The conversion's quota draft (issue #506, D-2): the pattern's
    /// default mapping the adapter computed from the rule; consumed by
    /// the confirm dispatch.
    pub convert_target: Option<taskboard_domain::HabitTarget>,
    /// The recur editor's one-shot subject (round-7 D-7.1): armed by
    /// the opener, consumed by the commit/detach dispatches
    /// (`NoTarget` without one — the stale-arm law).
    pub recur_edit_target: Option<TaskId>,
    /// The recur editor's minted draft (round-7 D-7.1, D-C1): the
    /// adapter's commit funnel re-mints the whole buffer through the
    /// field parsers and lands the rule here; `series.edit-commit`
    /// routes it (`NoTarget` without one — the button is disabled
    /// anyway, the seam re-checks; the trust boundary never trusts
    /// the widget).
    pub recur_draft_rule: Option<taskboard_domain::RecurRule>,
    /// The planner row's task (#622, D-6): set by the adapter when a
    /// planner row affordance activates; the placement/reorder verbs
    /// route from it (`NoTarget` without one).
    pub plan_target: Option<TaskId>,
    /// Whether the focused task is done (round 2, D19): the reopen
    /// verb's enablement gate — `task.reopen` routes only for a
    /// focused *completed* task (the `task.complete` machinery
    /// inverted; the router stays pure, the adapter syncs the stamp).
    pub focused_task_done: bool,
    /// The focused task's kind (issue #492): the complete verb's kind
    /// guard — the keyboard path has no widget between the key press
    /// and the engine, so this is the only suppression on that route
    /// (`NoTarget`-style inert dispatch; the engine rejection is the
    /// backstop).
    pub focused_task_tracker: bool,
    /// Whether the toggle target (the one-shot `track_target`, else
    /// the focused task) holds THE open tracking entry (J3 R-8,
    /// round 9 chunk 11): the smart toggle's state half — synced at
    /// dispatch time in the adapter's `route` (after the one-shot
    /// move, so the fact answers about the exact target).
    /// `task.track-toggle` reads it to route `StopTracking`;
    /// otherwise it routes `StartTracking` (the two existing verbs'
    /// outcomes — no new domain command).
    pub focused_task_tracking: bool,
    /// Whether the focused task carries an effort estimate (J3 US-4,
    /// round 9 chunk 11): `task.effort-clear`'s route gate — the
    /// `focused_task_scheduled` shape.
    pub focused_task_effort: bool,
}

/// The one-shot ctx fields a dispatched action may consume
/// (cross-surface friction plan D-6, #763 — the per-action consumption
/// set). Hand-rolled closed set (house A1): one private bit per
/// adapter one-shot; a new ctx one-shot whose bit is forgotten here
/// fails `one_shot_bits_are_total`, not production. Consumption is
/// declared per dispatched `ActionId` — an undeclared field stays in
/// the adapter untouched, so an interleaved dispatch can no longer eat
/// a confirm's target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OneShotSet(u32);

impl OneShotSet {
    pub(crate) const CREATE_TARGET: u32 = 1 << 0;
    pub(crate) const REMINDER_TARGET: u32 = 1 << 1;
    pub(crate) const BOARD_TARGET: u32 = 1 << 2;
    pub(crate) const RULE_TARGET: u32 = 1 << 3;
    pub(crate) const RULE_ADD_WHEN: u32 = 1 << 4;
    pub(crate) const TIMER_DRAFT: u32 = 1 << 5;
    pub(crate) const TIMER_TARGET: u32 = 1 << 6;
    pub(crate) const BREAK_RULE_DRAFT: u32 = 1 << 7;
    pub(crate) const BREAK_RULE_TARGET: u32 = 1 << 8;
    pub(crate) const TRACK_ENTRY_DRAFT: u32 = 1 << 9;
    pub(crate) const TRACK_ENTRY_TARGET: u32 = 1 << 10;
    pub(crate) const TRACK_TARGET: u32 = 1 << 11;
    pub(crate) const PLAN_TARGET: u32 = 1 << 12;
    pub(crate) const HABIT_TARGET: u32 = 1 << 13;
    pub(crate) const HABIT_UNDO: u32 = 1 << 14;
    pub(crate) const HABIT_MOVE: u32 = 1 << 15;
    pub(crate) const HABIT_REPAIR: u32 = 1 << 16;
    pub(crate) const HABIT_CONVERT_TARGET: u32 = 1 << 17;
    pub(crate) const HABIT_CREATE_DRAFT: u32 = 1 << 18;
    // The bits past 1 << 24 are the H2 additions — a duplicated bit
    // is invisible to the OR-based totalness pin, so the constants
    // never share a bit with an earlier field.
    pub(crate) const HABIT_MOVE_UP: u32 = 1 << 25;
    pub(crate) const HABIT_MOVE_DOWN: u32 = 1 << 26;
    // The H4 remove's armed subject: the first free bit past the H2
    // additions (every bit 0..=26 is taken; sharing one would
    // cross-consume the create form's draft — the OR-based pin can't
    // see it).
    pub(crate) const HABIT_REMOVE_TARGET: u32 = 1 << 27;
    // The undo-family restore's armed subject (W-3): the first free
    // bit past the quota addition (the same sharing hazard — the
    // OR-based pin can't see a shared draft).
    pub(crate) const HABIT_RESTORE_TARGET: u32 = 1 << 31;
    pub(crate) const HABIT_PAUSE_TARGET: u32 = 1 << 28;
    // The quota popover's one-shot draft (W-5, #961): the first free
    // bit past the HF1 pause addition (bit 28 landed with HF1; sharing
    // one would cross-consume — the OR-based pin can't see it).
    pub(crate) const HABIT_QUOTA_DRAFT: u32 = 1 << 29;
    // The Today compact-move pair's one-shot subject (US-14, #1031):
    // the R-8 task twin of the habit reorder pair — both ids
    // (`TaskMoveUp`/`TaskMoveDown`) consume the same cell; the routing
    // id carries the direction.
    pub(crate) const TASK_MOVE: u32 = 1 << 30;
    pub(crate) const PATTERN_DRAFT: u32 = 1 << 19;
    pub(crate) const SERIES_TARGET: u32 = 1 << 20;
    pub(crate) const PAUSE_DRAFT: u32 = 1 << 21;
    pub(crate) const CONVERT_TARGET: u32 = 1 << 22;
    pub(crate) const RECUR_EDIT_TARGET: u32 = 1 << 23;
    pub(crate) const RECUR_DRAFT_RULE: u32 = 1 << 24;
    const fn set(self, bit: u32) -> Self {
        Self(self.0 | bit)
    }

    pub(crate) const fn wants(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    pub(crate) const fn create_target(self) -> Self {
        self.set(Self::CREATE_TARGET)
    }
    pub(crate) const fn reminder_target(self) -> Self {
        self.set(Self::REMINDER_TARGET)
    }
    pub(crate) const fn board_target(self) -> Self {
        self.set(Self::BOARD_TARGET)
    }
    pub(crate) const fn rule_target(self) -> Self {
        self.set(Self::RULE_TARGET)
    }
    pub(crate) const fn rule_add_when(self) -> Self {
        self.set(Self::RULE_ADD_WHEN)
    }
    pub(crate) const fn timer_draft(self) -> Self {
        self.set(Self::TIMER_DRAFT)
    }
    pub(crate) const fn timer_target(self) -> Self {
        self.set(Self::TIMER_TARGET)
    }
    pub(crate) const fn break_rule_draft(self) -> Self {
        self.set(Self::BREAK_RULE_DRAFT)
    }
    pub(crate) const fn break_rule_target(self) -> Self {
        self.set(Self::BREAK_RULE_TARGET)
    }
    pub(crate) const fn track_entry_draft(self) -> Self {
        self.set(Self::TRACK_ENTRY_DRAFT)
    }
    pub(crate) const fn track_entry_target(self) -> Self {
        self.set(Self::TRACK_ENTRY_TARGET)
    }
    pub(crate) const fn track_target(self) -> Self {
        self.set(Self::TRACK_TARGET)
    }
    pub(crate) const fn plan_target(self) -> Self {
        self.set(Self::PLAN_TARGET)
    }
    pub(crate) const fn habit_target(self) -> Self {
        self.set(Self::HABIT_TARGET)
    }
    pub(crate) const fn habit_undo(self) -> Self {
        self.set(Self::HABIT_UNDO)
    }
    pub(crate) const fn habit_move(self) -> Self {
        self.set(Self::HABIT_MOVE)
    }
    pub(crate) const fn habit_move_up(self) -> Self {
        self.set(Self::HABIT_MOVE_UP)
    }
    pub(crate) const fn habit_move_down(self) -> Self {
        self.set(Self::HABIT_MOVE_DOWN)
    }
    pub(crate) const fn task_move(self) -> Self {
        self.set(Self::TASK_MOVE)
    }
    pub(crate) const fn habit_repair(self) -> Self {
        self.set(Self::HABIT_REPAIR)
    }
    pub(crate) const fn habit_convert_target(self) -> Self {
        self.set(Self::HABIT_CONVERT_TARGET)
    }
    pub(crate) const fn habit_remove_target(self) -> Self {
        self.set(Self::HABIT_REMOVE_TARGET)
    }
    pub(crate) const fn habit_restore_target(self) -> Self {
        self.set(Self::HABIT_RESTORE_TARGET)
    }
    pub(crate) const fn habit_pause_target(self) -> Self {
        self.set(Self::HABIT_PAUSE_TARGET)
    }
    pub(crate) const fn habit_create_draft(self) -> Self {
        self.set(Self::HABIT_CREATE_DRAFT)
    }
    pub(crate) const fn habit_quota_draft(self) -> Self {
        self.set(Self::HABIT_QUOTA_DRAFT)
    }
    pub(crate) const fn pattern_draft(self) -> Self {
        self.set(Self::PATTERN_DRAFT)
    }
    pub(crate) const fn series_target(self) -> Self {
        self.set(Self::SERIES_TARGET)
    }
    pub(crate) const fn pause_draft(self) -> Self {
        self.set(Self::PAUSE_DRAFT)
    }
    pub(crate) const fn convert_target(self) -> Self {
        self.set(Self::CONVERT_TARGET)
    }
    pub(crate) const fn recur_edit_target(self) -> Self {
        self.set(Self::RECUR_EDIT_TARGET)
    }
    pub(crate) const fn recur_draft_rule(self) -> Self {
        self.set(Self::RECUR_DRAFT_RULE)
    }
}

/// The per-action consumption declaration (the plan D-6 table): each
/// arm names exactly the one-shot fields its dispatch body reads —
/// derived from the dispatch arms' `ctx.` reads, with the existing
/// router and armed-journey tests as the oracle (AC-2: every
/// currently-passing pair passes unchanged). The arm effects
/// (`HabitConvertArm`, `SeriesConvertArm`) read adapter-side facts,
/// never ctx one-shots — they consume nothing.
#[must_use]
pub(crate) fn route_consumes(id: ActionId) -> OneShotSet {
    use ActionId as A;
    match id {
        A::TaskCreate => OneShotSet::default().create_target(),
        A::ReminderSnooze | A::ReminderMute => OneShotSet::default().reminder_target(),
        A::ReminderRuleAdd => OneShotSet::default().rule_add_when(),
        A::ReminderRuleRemove => OneShotSet::default().rule_target(),
        A::TimerCreate => OneShotSet::default().timer_draft(),
        A::TimerCancel => OneShotSet::default().timer_target(),
        A::BreakRuleCreate => OneShotSet::default().break_rule_draft(),
        A::BreakRuleRemoveConfirm => OneShotSet::default().break_rule_target(),
        A::HabitLog | A::HabitSkip => OneShotSet::default().habit_target(),
        A::HabitUndoLog => OneShotSet::default().habit_undo(),
        A::HabitMove => OneShotSet::default().habit_move(),
        A::HabitMoveUp => OneShotSet::default().habit_move_up(),
        A::HabitMoveDown => OneShotSet::default().habit_move_down(),
        // The Today compact-move pair (US-14): the R-8 twins share one
        // subject cell; the routing id is the direction.
        A::TaskMoveUp | A::TaskMoveDown => OneShotSet::default().task_move(),
        A::HabitRepair => OneShotSet::default().habit_repair(),
        A::HabitConvertToSeries => OneShotSet::default().habit_convert_target().pattern_draft(),
        A::HabitRemoveConfirm => OneShotSet::default().habit_remove_target(),
        A::HabitRestoreConfirm => OneShotSet::default().habit_restore_target(),
        A::HabitPauseCommit => OneShotSet::default().habit_pause_target().pause_draft(),
        A::HabitResume => OneShotSet::default().habit_pause_target(),
        A::HabitCreateCommit => OneShotSet::default().habit_create_draft(),
        A::HabitQuotaCommit => OneShotSet::default().habit_quota_draft(),
        A::SeriesPauseCommit => OneShotSet::default().series_target().pause_draft(),
        A::SeriesResume => OneShotSet::default().series_target(),
        A::SeriesConvertToHabit => OneShotSet::default().series_target().convert_target(),
        A::SeriesEditCommit => OneShotSet::default().recur_edit_target().recur_draft_rule(),
        A::SeriesEditDetach => OneShotSet::default().recur_edit_target(),
        // The smart toggle resolves the same target (J3 R-8): the
        // one-shot, else the focused task — and the adapter's
        // dispatch-time tracking fact answers about exactly it.
        A::TaskTrackStart | A::TaskTrackToggle => OneShotSet::default().track_target(),
        A::TaskTrackEntryLog => OneShotSet::default().track_target().track_entry_draft(),
        A::TaskTrackEntryAdjust => OneShotSet::default()
            .track_entry_target()
            .track_entry_draft(),
        A::TaskTrackEntryDelete => OneShotSet::default().track_entry_target(),
        A::PlanBeginPlace | A::PlanBoxEarlier | A::PlanBoxLater | A::PlanRemoveBox => {
            OneShotSet::default().plan_target()
        }
        A::BoardSelect => OneShotSet::default().board_target(),
        _ => OneShotSet::default(),
    }
}

/// The pure dispatch table. Every [`ActionId`] variant is matched —
/// adding an action without a routing decision is a compile error.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionRouter;

impl ActionRouter {
    /// One catalogue; splitting hides the table (the `plan_command`
    /// house style).
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn dispatch(id: ActionId, ctx: &DispatchCtx) -> DispatchOutcome {
        // The registry's enabled bit decides before any target check:
        // a disabled action is an absorbed no-op even with focus set.
        if !registry_entry(id).enabled {
            return DispatchOutcome::Disabled;
        }
        match id {
            // Application (enabled in chunk 2).
            ActionId::AppExit => DispatchOutcome::UiEffect(UiEffect::Exit),
            ActionId::AppMenu => DispatchOutcome::UiEffect(UiEffect::OpenMenu),
            // Undo v1 surfaces (ADR 0019, undo/2): target-free engine
            // verbs — availability greys from the published `UndoView`
            // (the adapter's `entry_states`), and the engine refuses
            // typed when the line cannot serve the verb. The dumb-client
            // pin: the router emits a `StateCommand`, nothing else.
            ActionId::AppUndo => DispatchOutcome::Command(StateCommand::Undo),
            ActionId::AppRedo => DispatchOutcome::Command(StateCommand::Redo),
            // The v2 extension (UF2, #353): the picker entry mounts the
            // sibling-branch selection menu — the v1 reflexes above keep
            // their exact semantics (trap b; US-3's fence).
            ActionId::AppUndoBranches => DispatchOutcome::UiEffect(UiEffect::UndoBranchesOpen),
            // Menu cursor (D3).
            ActionId::MenuNext => DispatchOutcome::UiEffect(UiEffect::MenuNext),
            ActionId::MenuPrevious => DispatchOutcome::UiEffect(UiEffect::MenuPrevious),
            ActionId::MenuInvoke => DispatchOutcome::UiEffect(UiEffect::InvokeSelected),
            // Level-aware (round-1 D6): Escape steps back level by
            // level. The full close is the dismissal layer's and the
            // toggle's — the adapter applies `CloseMenu` directly.
            ActionId::MenuDismiss => DispatchOutcome::UiEffect(UiEffect::MenuBack),
            // Mode flips (D10): ordinary ui-internal actions.
            ActionId::ModeInsert => DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Insert)),
            ActionId::ModeNormal => DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Normal)),
            // Task verbs complete without views (D16): the focused
            // task suffices — the chunk-2 end-to-end path.
            ActionId::TaskComplete => match (ctx.focused_task, ctx.focused_task_tracker) {
                (Some(id), false) => {
                    DispatchOutcome::Command(StateCommand::SetTaskDone { id, done: true })
                }
                // A tracker card's completion is impossible (issue
                // #492): an inert dispatch — the same NoTarget shape a
                // focusless keypress takes.
                _ => DispatchOutcome::NoTarget,
            },
            ActionId::TaskDelete => match ctx.focused_task {
                Some(id) => DispatchOutcome::Command(StateCommand::DeleteTask { id }),
                None => DispatchOutcome::NoTarget,
            },
            // The Q23 banner verbs (D4): concrete targets only.
            ActionId::ReminderSnooze => match ctx.reminder_target {
                Some(target) => DispatchOutcome::Command(StateCommand::SnoozeReminder {
                    reminder: target.reminder,
                    occurrence: target.occurrence,
                    postpone: SNOOZE_POSTPONE,
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::ReminderMute => match ctx.reminder_target {
                Some(target) => match target.anchor {
                    // Mute is task-shaped; an alarm anchor has no task to
                    // mute (unreachable via `banner_rows`).
                    taskboard_domain::ReminderAnchor::Task(task) => {
                        DispatchOutcome::Command(StateCommand::SetTaskRemindersMuted {
                            task,
                            muted: true,
                        })
                    }
                    // Mute is task-shaped; an alarm, break, or tracker
                    // anchor has no task to mute (the tracker's
                    // off-switch is clearing the anchor).
                    taskboard_domain::ReminderAnchor::Alarm(_)
                    | taskboard_domain::ReminderAnchor::Break(_)
                    | taskboard_domain::ReminderAnchor::Tracker(_) => DispatchOutcome::NoTarget,
                },
                None => DispatchOutcome::NoTarget,
            },
            // D31a (r4): the un-mute is section-shaped — the focused
            // task is always the target (the section names its
            // subject), unlike the banner's one-shot target machinery.
            ActionId::ReminderUnmute => match ctx.focused_task {
                Some(task) => DispatchOutcome::Command(StateCommand::SetTaskRemindersMuted {
                    task,
                    muted: false,
                }),
                None => DispatchOutcome::NoTarget,
            },
            // The rule-list verbs (chunk 6, D11): the banner-verb
            // pattern — concrete one-shot targets only; a menu
            // invocation carries neither.
            ActionId::ReminderRuleAdd => match (ctx.focused_task, ctx.rule_add_when) {
                (Some(task), Some(when)) => {
                    // The three-form grammar (r4 decision 2): the
                    // command stays additive — a due-offset draft rides
                    // the legacy field, an at/in draft rides `at` and
                    // the planner stamps `In`.
                    let (offset, at) = match when {
                        crate::presentation::RuleWhen::DueOffset(offset) => (offset, None),
                        crate::presentation::RuleWhen::At(when) => {
                            (taskboard_domain::ReminderOffset::at_due(), Some(when))
                        }
                    };
                    DispatchOutcome::Command(StateCommand::AddReminderRule {
                        task,
                        offset,
                        at,
                        insistence: taskboard_domain::Insistence::Normal,
                    })
                }
                _ => DispatchOutcome::NoTarget,
            },
            ActionId::ReminderRuleRemove => match ctx.rule_target {
                Some(rule) => DispatchOutcome::Command(StateCommand::RemoveReminderRule { rule }),
                None => DispatchOutcome::NoTarget,
            },
            // The timers verbs (DR-3/3, D-I8): the one-shot target
            // pattern — concrete drafts/targets only.
            ActionId::TimerCreate => match &ctx.timer_draft {
                Some(TimerDraft {
                    label,
                    when,
                    insistence,
                }) => DispatchOutcome::Command(StateCommand::CreateAdhocAlarm {
                    label: label.clone(),
                    when: *when,
                    insistence: *insistence,
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::TimerCancel => match ctx.timer_target {
                Some(alarm) => DispatchOutcome::Command(StateCommand::CancelAdhocAlarm { alarm }),
                None => DispatchOutcome::NoTarget,
            },
            // The break-rule verbs (T-14, #797): the timers verbs'
            // one-shot target pattern — concrete drafts/targets only.
            // The remove verb is the arm step (the armed-confirm
            // grammar): the adapter records the armed subject before
            // routing; the router arm itself dispatches nothing (the
            // staging lives adapter-side, arming touches no engine
            // state). Only the confirm routes `DeleteBreakRule`.
            ActionId::BreakRuleCreate => match &ctx.break_rule_draft {
                Some(BreakRuleDraft {
                    board,
                    label,
                    threshold,
                    insistence,
                }) => DispatchOutcome::Command(StateCommand::CreateBreakRule {
                    board: Some(*board),
                    label: label.clone(),
                    threshold: *threshold,
                    insistence: *insistence,
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::BreakRuleRemoveConfirm => match ctx.break_rule_target {
                Some(rule) => DispatchOutcome::Command(StateCommand::DeleteBreakRule { id: rule }),
                None => DispatchOutcome::NoTarget,
            },
            // The alarm window's verbs (T-22, #805): the routed
            // command is structurally the button's — both sides call
            // the shared `presentation::alarm_command` constructor.
            ActionId::AlarmDismiss => match &ctx.alarm_row {
                Some(row) => DispatchOutcome::Command(crate::presentation::alarm_command(
                    row,
                    crate::presentation::AlarmVerb::Dismiss,
                )),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::AlarmSnooze => match &ctx.alarm_row {
                Some(row) => DispatchOutcome::Command(crate::presentation::alarm_command(
                    row,
                    crate::presentation::AlarmVerb::Snooze,
                )),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::AlarmCancel => match &ctx.alarm_row {
                Some(row) => DispatchOutcome::Command(crate::presentation::alarm_command(
                    row,
                    crate::presentation::AlarmVerb::Cancel,
                )),
                None => DispatchOutcome::NoTarget,
            },
            // The habit verbs (DR-6/6, D-P13): +1 from the row target;
            // the reorder from the adapter-resolved (habit, order) —
            // the UI computes midpoints, the planner stamps the value.
            ActionId::HabitLog => match ctx.habit_target {
                Some(habit) => DispatchOutcome::Command(StateCommand::LogHabit { habit }),
                None => DispatchOutcome::NoTarget,
            },
            // The −1 recovery (UX R-2 D-3, #786): the audited
            // `AdjustHabitLog` valve over the row's own current
            // window — an exact inverse with no forged facts. The
            // decrement is never quota-enforced (D-P14
            // symmetrically); count 0 is the honest `NoTarget`.
            ActionId::HabitUndoLog => match ctx.habit_undo {
                Some((habit, window, n)) if n >= 1 => {
                    DispatchOutcome::Command(StateCommand::AdjustHabitLog {
                        habit,
                        window,
                        count: n - 1,
                    })
                }
                _ => DispatchOutcome::NoTarget,
            },
            // The habit reorder verbs (#1198 chunk A1 D1): the
            // one-shot cells carry target gap indices — the engine
            // resolves the orders (`HabitReorder`); the absolute-order
            // `MoveHabit` stays the CLI's verb.
            ActionId::HabitMove => match ctx.habit_move {
                Some((habit, target)) => {
                    DispatchOutcome::Command(StateCommand::HabitReorder { habit, target })
                }
                None => DispatchOutcome::NoTarget,
            },
            // The habit menu's reorder pair (H2 D4): the direction
            // rides the routing id, the slot's target index was
            // minted at menu open from the same row source the
            // section renders.
            ActionId::HabitMoveUp | ActionId::HabitMoveDown => {
                let cell = if matches!(id, ActionId::HabitMoveUp) {
                    ctx.habit_move_up
                } else {
                    ctx.habit_move_down
                };
                match cell {
                    Some((habit, target)) => {
                        DispatchOutcome::Command(StateCommand::HabitReorder { habit, target })
                    }
                    None => DispatchOutcome::NoTarget,
                }
            }
            // The Today compact-move pair (US-14, #1031): the R-8 task
            // twins of the shared Shift+Up/Down stroke — the direction
            // rides the routing id, the subject is the one-shot minted
            // at the kind resolution; the engine's shared floor-midpoint
            // rule resolves the slot and refuses the boundary typed.
            ActionId::TaskMoveUp | ActionId::TaskMoveDown => {
                let up = matches!(id, ActionId::TaskMoveUp);
                match ctx.task_move {
                    Some(task) => DispatchOutcome::Command(if up {
                        StateCommand::TaskMoveUp { id: task }
                    } else {
                        StateCommand::TaskMoveDown { id: task }
                    }),
                    None => DispatchOutcome::NoTarget,
                }
            }
            // The habit trio (issue #506, plan D-7/D-8): the make-up
            // valve dispatches the existing audited `AdjustHabitLog`;
            // the conversion arm is a view intent (the confirm strip),
            // the confirm dispatch routes the domain verb with the
            // D-2 default pattern.
            ActionId::HabitRepair => match ctx.habit_repair {
                Some((habit, window, count)) => {
                    DispatchOutcome::Command(StateCommand::AdjustHabitLog {
                        habit,
                        window,
                        count,
                    })
                }
                None => DispatchOutcome::NoTarget,
            },
            ActionId::HabitConvertArm => DispatchOutcome::UiEffect(UiEffect::HabitConvertArm),
            ActionId::HabitConvertCancel => DispatchOutcome::UiEffect(UiEffect::HabitConvertCancel),
            // The H4 editor opener: the TaskEdit shape — the adapter
            // resolves the one-shot habit subject and opens the
            // shared surface (subjectless dispatch is the adapter's
            // typed refusal).
            // HF4 (W-11): the schedule/override rows are the editor
            // opener's anchors — the sections live inside the editor
            // surface, so the key opens it (the click ≡ key law).
            ActionId::HabitScheduleEdit | ActionId::HabitReminderEdit => {
                DispatchOutcome::UiEffect(UiEffect::HabitEdit)
            }
            ActionId::HabitEdit => DispatchOutcome::UiEffect(UiEffect::HabitEdit),
            // The inline quota edit (W-5, #961): the opener is a view
            // intent (the adapter arms over the resolved subject); the
            // commit routes the domain verb from the one-shot draft
            // (`NoTarget` without one — the stale/disarmed popover
            // commits nothing).
            ActionId::HabitQuotaEdit => DispatchOutcome::UiEffect(UiEffect::HabitQuotaEdit),
            ActionId::HabitQuotaCommit => match ctx.habit_quota_draft.clone() {
                Some(draft) => DispatchOutcome::Command(StateCommand::UpdateHabit {
                    habit: draft.habit,
                    changes: taskboard_domain::messages::HabitChanges {
                        target: Some(draft.target),
                        ..Default::default()
                    },
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::HabitHistory => DispatchOutcome::UiEffect(UiEffect::HabitHistoryOpen),
            // The staged remove (H4 D4, #944): the arm routes the
            // adapter's staging effect; the confirm routes the domain
            // verb from the one-shot armed subject (`NoTarget`
            // unarmed/stale — the disarmed form commits nothing).
            ActionId::HabitRemove => DispatchOutcome::UiEffect(UiEffect::HabitRemoveArm),
            ActionId::HabitRemoveCancel => DispatchOutcome::UiEffect(UiEffect::HabitRemoveCancel),
            // The HF1 skip/pause verbs (#955/#958): the skip routes the
            // typed mark from the row's one-shot target (R-4: current
            // window only — the window is minted planner-side); the
            // pause open is a view intent; commit and resume route the
            // domain verb from the one-shot target (the SeriesPause
            // machinery's shape, reused not duplicated).
            ActionId::HabitSkip => match ctx.habit_target {
                Some(habit) => DispatchOutcome::Command(StateCommand::SkipHabit { habit }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::HabitPause => DispatchOutcome::UiEffect(UiEffect::HabitPauseOpen),
            ActionId::HabitPauseCommit => match (ctx.habit_pause_target, ctx.pause_draft) {
                (Some(habit), Some(until)) => DispatchOutcome::Command(StateCommand::PauseHabit {
                    habit,
                    until: Some(until),
                }),
                _ => DispatchOutcome::NoTarget,
            },
            ActionId::HabitResume => match ctx.habit_pause_target {
                Some(habit) => {
                    DispatchOutcome::Command(StateCommand::PauseHabit { habit, until: None })
                }
                None => DispatchOutcome::NoTarget,
            },
            ActionId::HabitRemoveConfirm => match ctx.habit_remove_target {
                Some(habit) => DispatchOutcome::Command(StateCommand::DeleteHabit { habit }),
                None => DispatchOutcome::NoTarget,
            },
            // The undo-family restore trio (W-3): the arm routes the
            // adapter's staging effect over the roster's restorable
            // habit; the confirm routes `RestoreHabit` from the armed
            // subject (`NoTarget` unarmed/stale — the disarmed form
            // commits nothing; the log is never touched, D26).
            ActionId::HabitRestore => DispatchOutcome::UiEffect(UiEffect::HabitRestoreArm),
            ActionId::HabitRestoreCancel => DispatchOutcome::UiEffect(UiEffect::HabitRestoreCancel),
            ActionId::HabitRestoreConfirm => match ctx.habit_restore_target {
                Some(habit) => DispatchOutcome::Command(StateCommand::RestoreHabit { id: habit }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::HabitConvertToSeries => {
                match (ctx.habit_convert_target, ctx.pattern_draft.clone()) {
                    (Some(habit), Some(pattern)) => {
                        DispatchOutcome::Command(StateCommand::ConvertHabitToSeries {
                            habit,
                            pattern,
                        })
                    }
                    _ => DispatchOutcome::NoTarget,
                }
            }
            // The create form (round-7 D-7.2): the opener is a view
            // intent (the adapter's toggle); the commit routes the
            // domain verb from the one-shot draft (`NoTarget` without
            // one — the stale/disarmed form commits nothing).
            ActionId::HabitCreate => DispatchOutcome::UiEffect(UiEffect::HabitCreateToggle),
            ActionId::HabitCreateCommit => match ctx.habit_create_draft.clone() {
                Some(draft) => DispatchOutcome::Command(StateCommand::CreateHabit {
                    board: draft.board,
                    title: draft.title,
                    target: draft.target,
                }),
                None => DispatchOutcome::NoTarget,
            },
            // The series verbs (issue #506, plan D-8): the pause open
            // is a view intent; commit and resume route the domain
            // verb from the recur row's one-shot target.
            ActionId::SeriesPause => DispatchOutcome::UiEffect(UiEffect::SeriesPauseOpen),
            // US-7 (W-8, #1032): the opener is a pure view intent —
            // the subject resolution (and its typed refusal) is the
            // adapter arm's.
            ActionId::RowMenu => DispatchOutcome::UiEffect(UiEffect::RowMenuRequested),
            // US-10 (W-11, #1091): the disarm is a view-state verb —
            // the armed/disarmed fork is the adapter arm's.
            ActionId::RowCursorDisarm => DispatchOutcome::UiEffect(UiEffect::RowCursorDisarm),
            // US-9 (W-10): the arm step is a view intent — the armed
            // toggle over the roster cursor is the adapter arm's (the
            // staged remove never dispatches engine-side here).
            ActionId::BreakRuleRemove => DispatchOutcome::UiEffect(UiEffect::BreakRuleRemoveArm),
            ActionId::SeriesPauseCommit => match (ctx.series_target, ctx.pause_draft) {
                (Some(head), Some(until)) => DispatchOutcome::Command(StateCommand::PauseSeries {
                    head,
                    until: Some(until),
                }),
                _ => DispatchOutcome::NoTarget,
            },
            ActionId::SeriesResume => match ctx.series_target {
                Some(head) => {
                    DispatchOutcome::Command(StateCommand::PauseSeries { head, until: None })
                }
                None => DispatchOutcome::NoTarget,
            },
            ActionId::SeriesConvertArm => DispatchOutcome::UiEffect(UiEffect::SeriesConvertArm),
            ActionId::SeriesConvertToHabit => match (ctx.series_target, ctx.convert_target) {
                (Some(head), Some(target)) => {
                    DispatchOutcome::Command(StateCommand::ConvertSeriesToHabit { head, target })
                }
                _ => DispatchOutcome::NoTarget,
            },
            // The recurrence editor's verbs (round-7 D-7.1, plan
            // D-C1/D-C2): the opener is a view intent; the commit and
            // the detach route the one `UpdateTask` from the one-shot
            // pair — and nothing else rides the detach's changeset
            // (the review's must-check).
            ActionId::SeriesEdit => DispatchOutcome::UiEffect(UiEffect::SeriesEditorOpen),
            // The priority pickers' openers (UX R-3 D-1, #788): view
            // intents — the kiosk/no-subject refusals live in the
            // adapter's arms (the entries-overlay gate's shape).
            ActionId::TaskImportanceEdit => {
                DispatchOutcome::UiEffect(UiEffect::PriorityPickerOpen(PriorityField::Importance))
            }
            ActionId::TaskUrgencyEdit => {
                DispatchOutcome::UiEffect(UiEffect::PriorityPickerOpen(PriorityField::Urgency))
            }
            // The labels overlay (UX R-3 D-2, #789): the opener is a
            // view intent; the dismissal is its ladder rung — the
            // entries pair's shape.
            ActionId::LabelManage => DispatchOutcome::UiEffect(UiEffect::LabelsOpen),
            ActionId::LabelOverlayDismiss => DispatchOutcome::UiEffect(UiEffect::LabelsDismiss),
            ActionId::SeriesEditCommit => {
                match (ctx.recur_edit_target, ctx.recur_draft_rule.clone()) {
                    (Some(id), Some(rule)) => DispatchOutcome::Command(StateCommand::UpdateTask {
                        id,
                        changes: taskboard_domain::TaskChanges {
                            recur: Some(Some(rule)),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    }),
                    _ => DispatchOutcome::NoTarget,
                }
            }
            ActionId::SeriesEditDetach => match ctx.recur_edit_target {
                Some(id) => DispatchOutcome::Command(StateCommand::UpdateTask {
                    id,
                    changes: taskboard_domain::TaskChanges {
                        recur: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::SeriesEditDetachArm => DispatchOutcome::UiEffect(UiEffect::SeriesDetachArm),
            ActionId::SeriesEditDetachCancel => {
                DispatchOutcome::UiEffect(UiEffect::SeriesDetachCancel)
            }
            // The tracking verbs (DR-5/4, D-T16): Start routes from the
            // explicit one-shot target, falling back to the focused
            // task (D-TU5's two honest sources); Stop is payload-free
            // and unconditional — nothing-open is the planner's total
            // NoOp, the engine owns the refusals.
            ActionId::TaskTrackStart => match ctx.track_target.or(ctx.focused_task) {
                Some(task) => DispatchOutcome::Command(StateCommand::StartTracking {
                    task,
                    kind: taskboard_domain::tracking::TimeEntryKind::Work,
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::TaskTrackStop => DispatchOutcome::Command(StateCommand::StopTracking),
            // The ONE smart tracking toggle (J3 R-8, round 9 chunk 11):
            // the state-correct half over the target — a running entry
            // ON THIS TASK routes `StopTracking`; anything else routes
            // `StartTracking` (whose own law closes any
            // running-elsewhere entry first). Exactly the two existing
            // verbs' outcomes — no new domain command (the fence); the
            // planner's idempotent NoOps own the refusals.
            ActionId::TaskTrackToggle => {
                match (
                    ctx.track_target.or(ctx.focused_task),
                    ctx.focused_task_tracking,
                ) {
                    (Some(_), true) => DispatchOutcome::Command(StateCommand::StopTracking),
                    (Some(task), false) => DispatchOutcome::Command(StateCommand::StartTracking {
                        task,
                        kind: taskboard_domain::tracking::TimeEntryKind::Work,
                    }),
                    (None, _) => DispatchOutcome::NoTarget,
                }
            }
            // The entries-overlay family (r4 chunk 10, D-TS6): the
            // open verb is a view intent; the three panel verbs are
            // one-shot — draft + target only, exactly the existing
            // domain commands (`LogTime`/`AdjustTimeEntry`/
            // `DeleteTimeEntry`), nothing new dispatches.
            ActionId::TaskTrackEntries => DispatchOutcome::UiEffect(UiEffect::TrackEntriesOpen),
            ActionId::TaskTrackEntryLog => {
                match (ctx.track_target.or(ctx.focused_task), ctx.track_entry_draft) {
                    (Some(task), Some(draft)) => match draft.end {
                        Some(end) => DispatchOutcome::Command(StateCommand::LogTime {
                            task,
                            start: draft.start,
                            end,
                            kind: taskboard_domain::tracking::TimeEntryKind::Work,
                        }),
                        // An open-end draft never routes an add (the
                        // parse refuses it live); defensive `NoTarget`.
                        None => DispatchOutcome::NoTarget,
                    },
                    _ => DispatchOutcome::NoTarget,
                }
            }
            ActionId::TaskTrackEntryAdjust => {
                match (ctx.track_entry_target, ctx.track_entry_draft) {
                    (Some(id), Some(draft)) => {
                        DispatchOutcome::Command(StateCommand::AdjustTimeEntry {
                            id,
                            start: Some(draft.start),
                            end: draft.end,
                            kind: None,
                        })
                    }
                    _ => DispatchOutcome::NoTarget,
                }
            }
            ActionId::TaskTrackEntryDelete => match ctx.track_entry_target {
                Some(id) => DispatchOutcome::Command(StateCommand::DeleteTimeEntry { id }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::TaskTrackEntryDismiss => {
                DispatchOutcome::UiEffect(UiEffect::TrackEntryDismiss)
            }
            // The concentration overlay's toggle (#496): presentation
            // only — no session verb rides it (the fence: the session
            // drives only the landed tracking verbs).
            ActionId::FocusModeToggle => DispatchOutcome::UiEffect(UiEffect::ToggleFocusMode),
            ActionId::NotificationDismiss => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::Dismiss))
            }
            ActionId::NotificationDone => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::Done))
            }
            ActionId::NotificationSnooze => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::Snooze))
            }
            ActionId::NotificationMute => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::Mute))
            }
            ActionId::NotificationCancel => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::Cancel))
            }
            ActionId::NotificationTrackStart => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::TrackStart))
            }
            ActionId::NotificationOpen => {
                DispatchOutcome::UiEffect(UiEffect::NotificationVerb(NotificationVerb::Open))
            }
            // The chunk-3 view intents (D6): the adapter resolves the
            // effects against the view-state model.
            ActionId::NavNextView => DispatchOutcome::UiEffect(UiEffect::NextView),
            ActionId::NavPrevView => DispatchOutcome::UiEffect(UiEffect::PrevView),
            ActionId::NavFocusBoard => DispatchOutcome::UiEffect(UiEffect::FocusBoard),
            ActionId::NavShowTask => DispatchOutcome::UiEffect(UiEffect::ShowTask),
            ActionId::NavNextControl => DispatchOutcome::UiEffect(UiEffect::ControlNext),
            ActionId::NavPrevControl => DispatchOutcome::UiEffect(UiEffect::ControlPrev),
            ActionId::ViewScrollUp => DispatchOutcome::UiEffect(UiEffect::ScrollUp),
            ActionId::ViewScrollDown => DispatchOutcome::UiEffect(UiEffect::ScrollDown),
            ActionId::PickerLeft => {
                DispatchOutcome::UiEffect(UiEffect::PickerKey(crate::due_edit::DatePickerKey::Left))
            }
            ActionId::PickerRight => DispatchOutcome::UiEffect(UiEffect::PickerKey(
                crate::due_edit::DatePickerKey::Right,
            )),
            ActionId::PickerUp => {
                DispatchOutcome::UiEffect(UiEffect::PickerKey(crate::due_edit::DatePickerKey::Up))
            }
            ActionId::PickerDown => {
                DispatchOutcome::UiEffect(UiEffect::PickerKey(crate::due_edit::DatePickerKey::Down))
            }
            ActionId::PickerPrevMonth => DispatchOutcome::UiEffect(UiEffect::PickerKey(
                crate::due_edit::DatePickerKey::PrevMonth,
            )),
            ActionId::PickerNextMonth => DispatchOutcome::UiEffect(UiEffect::PickerKey(
                crate::due_edit::DatePickerKey::NextMonth,
            )),
            ActionId::PickerMonthStart => DispatchOutcome::UiEffect(UiEffect::PickerKey(
                crate::due_edit::DatePickerKey::MonthStart,
            )),
            ActionId::PickerMonthEnd => DispatchOutcome::UiEffect(UiEffect::PickerKey(
                crate::due_edit::DatePickerKey::MonthEnd,
            )),
            ActionId::PickerCommit => DispatchOutcome::UiEffect(UiEffect::PickerKey(
                crate::due_edit::DatePickerKey::Commit,
            )),
            ActionId::NavNextItem => DispatchOutcome::UiEffect(UiEffect::FocusNext),
            ActionId::NavPrevItem => DispatchOutcome::UiEffect(UiEffect::FocusPrev),
            // The directional-focus family (J3 US-1, round 9 chunk 9)
            // and its selection-extend twin (#875): one effect per
            // direction — the per-view reading lives in the adapter.
            ActionId::NavFocusLeft => {
                DispatchOutcome::UiEffect(UiEffect::FocusDirectional(Direction::Left))
            }
            ActionId::NavFocusRight => {
                DispatchOutcome::UiEffect(UiEffect::FocusDirectional(Direction::Right))
            }
            ActionId::NavFocusUp => {
                DispatchOutcome::UiEffect(UiEffect::FocusDirectional(Direction::Up))
            }
            ActionId::NavFocusDown => {
                DispatchOutcome::UiEffect(UiEffect::FocusDirectional(Direction::Down))
            }
            ActionId::NavExtendNext => DispatchOutcome::UiEffect(UiEffect::ExtendSelectionNext),
            ActionId::NavExtendPrev => DispatchOutcome::UiEffect(UiEffect::ExtendSelectionPrev),
            ActionId::TaskCreate => DispatchOutcome::UiEffect(UiEffect::BeginCreate {
                target: ctx.create_target,
            }),
            ActionId::StackCreate => DispatchOutcome::UiEffect(UiEffect::BeginCreateStack),
            ActionId::TaskEdit => DispatchOutcome::UiEffect(UiEffect::Activate),
            ActionId::TaskTitleEdit => DispatchOutcome::UiEffect(UiEffect::BeginTitleEdit),
            // The detail editors' keyboard openers (keymap-parity plan
            // US-3, D-4): view intents over the subject task — the
            // funnel-level refusals live in the adapter's arms (the
            // subjectless detail is each arm's typed inert).
            ActionId::TaskDueEdit => DispatchOutcome::UiEffect(UiEffect::BeginDateEdit {
                field: crate::view_state::DateField::Due,
            }),
            ActionId::TaskScheduledEdit => DispatchOutcome::UiEffect(UiEffect::BeginDateEdit {
                field: crate::view_state::DateField::Scheduled,
            }),
            ActionId::TaskDescriptionEdit => {
                DispatchOutcome::UiEffect(UiEffect::BeginDescriptionEdit)
            }
            ActionId::TaskMove => DispatchOutcome::UiEffect(UiEffect::RequestMove),
            // Round 2 (D16–D21): the round-up verbs — pure view
            // intents; reopen routes from the focused task's done stamp.
            ActionId::TaskMoveCancel => DispatchOutcome::UiEffect(UiEffect::CancelMove),
            ActionId::StackNext => DispatchOutcome::UiEffect(UiEffect::StackNext),
            ActionId::StackPrev => DispatchOutcome::UiEffect(UiEffect::StackPrev),
            ActionId::StackDelete => DispatchOutcome::UiEffect(UiEffect::StackDelete),
            ActionId::StackRename => DispatchOutcome::UiEffect(UiEffect::StackRename),
            ActionId::StackMove => DispatchOutcome::UiEffect(UiEffect::StackMove),
            ActionId::BoardCreate => DispatchOutcome::UiEffect(UiEffect::BoardCreate),
            ActionId::BoardDelete => DispatchOutcome::UiEffect(UiEffect::BoardDelete),
            ActionId::BoardRename => DispatchOutcome::UiEffect(UiEffect::BoardRename),
            ActionId::BoardColorEdit => DispatchOutcome::UiEffect(UiEffect::BoardColorEdit),
            ActionId::BoardDefaultPreset => DispatchOutcome::UiEffect(UiEffect::BoardDefaultPreset),
            ActionId::BoardArchive => DispatchOutcome::UiEffect(UiEffect::BoardArchive),
            ActionId::BoardReset => DispatchOutcome::UiEffect(UiEffect::BoardReset),
            ActionId::BoardReconcile => DispatchOutcome::UiEffect(UiEffect::BoardReconcile),
            ActionId::NavFirstItem => DispatchOutcome::UiEffect(UiEffect::FocusFirst),
            ActionId::NavLastItem => DispatchOutcome::UiEffect(UiEffect::FocusLast),
            // The today re-commit (chunk 10, D-V4 rider): a fresh
            // `Some(Some(today_last_instant))` write, confirmation-
            // free; `NoTarget` without focus *or* window (the design's
            // exact wording). The stamp is the day's last covered
            // instant (#893) — the raw exclusive end is excluded by
            // the very `[now, end)` membership it would be judged by.
            ActionId::TaskDueToday => {
                match (ctx.focused_task, ctx.day_window, ctx.today_last_instant) {
                    (Some(id), Some(_), Some(stamp)) => {
                        DispatchOutcome::Command(StateCommand::UpdateTask {
                            id,
                            changes: taskboard_domain::TaskChanges {
                                duedate: Some(Some(stamp)),
                                ..taskboard_domain::TaskChanges::default()
                            },
                        })
                    }
                    _ => DispatchOutcome::NoTarget,
                }
            }
            // The scheduled field's quick verbs (UX-R8 chunk-5 D-3,
            // US-1/S1–S3): the exact sparse `scheduled` mask — today's
            // last covered instant rides the same stamp `task.due-today`
            // re-commits (the uniform day-boundary law, #893),
            // tomorrow's is the additive window fact, unschedule is
            // the `Some(None)` clear twin. Inapplicable subjects are
            // withheld upstream (D-4); the NoTarget arm is the
            // stale-absorb backstop.
            ActionId::TaskScheduleToday => {
                match (ctx.focused_task, ctx.day_window, ctx.today_last_instant) {
                    (Some(id), Some(_), Some(stamp)) => {
                        DispatchOutcome::Command(StateCommand::UpdateTask {
                            id,
                            changes: taskboard_domain::TaskChanges {
                                scheduled: Some(Some(stamp)),
                                ..taskboard_domain::TaskChanges::default()
                            },
                        })
                    }
                    _ => DispatchOutcome::NoTarget,
                }
            }
            ActionId::TaskScheduleTomorrow => match (ctx.focused_task, ctx.tomorrow_window_end) {
                (Some(id), Some(end)) => DispatchOutcome::Command(StateCommand::UpdateTask {
                    id,
                    changes: taskboard_domain::TaskChanges {
                        scheduled: Some(Some(end)),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }),
                _ => DispatchOutcome::NoTarget,
            },
            ActionId::TaskUnschedule => match (ctx.focused_task, ctx.focused_task_scheduled) {
                (Some(id), true) => DispatchOutcome::Command(StateCommand::UpdateTask {
                    id,
                    changes: taskboard_domain::TaskChanges {
                        scheduled: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }),
                _ => DispatchOutcome::NoTarget,
            },
            // The effort row's keyboard pair (J3 US-4, round 9 chunk
            // 11): edit is a *focus* verb — it arms nothing and
            // commits nothing (the M9 commit-on-focus-loss law; the
            // always-on field keeps owning the commit); clear routes
            // the sparse mask, confirmation-free (the TaskUnschedule
            // posture).
            ActionId::TaskEffortEdit => DispatchOutcome::UiEffect(UiEffect::BeginEffortEdit),
            ActionId::TaskEffortClear => match (ctx.focused_task, ctx.focused_task_effort) {
                (Some(id), true) => DispatchOutcome::Command(StateCommand::UpdateTask {
                    id,
                    changes: taskboard_domain::TaskChanges {
                        effort: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }),
                _ => DispatchOutcome::NoTarget,
            },
            // The today/inbox view intents + the capture flip (chunk
            // 10): pure view intents; the adapter interprets them.
            ActionId::ViewToday => DispatchOutcome::UiEffect(UiEffect::FocusToday),
            ActionId::ViewInbox => DispatchOutcome::UiEffect(UiEffect::OpenInbox),
            ActionId::CaptureQuick => DispatchOutcome::UiEffect(UiEffect::BeginCapture),
            ActionId::TaskReopen => match (ctx.focused_task, ctx.focused_task_done) {
                (Some(id), true) => {
                    DispatchOutcome::Command(StateCommand::SetTaskDone { id, done: false })
                }
                _ => DispatchOutcome::NoTarget,
            },
            // The kind toggle (issue #492): reads the focused task's
            // kind and inverts it through the plain kind edit — no new
            // domain command, the app-lane carries it.
            ActionId::TaskToggleTracker => {
                let inverted = match (ctx.focused_task, ctx.focused_task_tracker) {
                    (Some(_), true) => Some(taskboard_domain::entities::TaskKind::Standard),
                    (Some(_), false) => Some(taskboard_domain::entities::TaskKind::Tracker),
                    (None, _) => None,
                };
                match (ctx.focused_task, inverted) {
                    (Some(id), Some(kind)) => DispatchOutcome::Command(StateCommand::UpdateTask {
                        id,
                        changes: taskboard_domain::TaskChanges {
                            kind: Some(Some(kind)),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    }),
                    _ => DispatchOutcome::NoTarget,
                }
            }
            // The board dimension (design §3.8): pure view intents.
            ActionId::BoardNext => DispatchOutcome::UiEffect(UiEffect::BoardNext),
            ActionId::BoardPrevious => DispatchOutcome::UiEffect(UiEffect::BoardPrevious),
            // UX R-4 D-4 (#794): a target (the segment click's
            // one-shot) selects directly; the bare stroke opens the
            // switcher flyout — the target-free flow the finding
            // names (the old bare `NoTarget` was the asymmetry).
            ActionId::BoardSelect => match ctx.board_target {
                Some(board) => DispatchOutcome::UiEffect(UiEffect::BoardSelect(board)),
                None => DispatchOutcome::UiEffect(UiEffect::OpenBoardBarFlyout),
            },
            ActionId::StackWider => DispatchOutcome::UiEffect(UiEffect::StackWider),
            ActionId::StackNarrower => DispatchOutcome::UiEffect(UiEffect::StackNarrower),
            // Round-1 chunk 3 (D28b): the settings opener is live.
            ActionId::AppSettings => DispatchOutcome::UiEffect(UiEffect::OpenSettings),
            // The export dialog's opener (the J2 export plan, D-1):
            // the action-menu entry point rides the same adapter arm
            // the Settings row drives.
            ActionId::ExportDialogOpen => DispatchOutcome::UiEffect(UiEffect::ExportDialogOpen),
            ActionId::ExportDialogClose => DispatchOutcome::UiEffect(UiEffect::ExportDialogClose),
            // The status lane's remediation verbs and the export
            // lane's preview/clipboard verbs (the D2 chunk): the
            // dispatch seams are the panel's/dialog's own (the
            // export-Run precedent) — the router arms absorb, never
            // dispatch (the verb addresses exist for the census and
            // the context resolution).
            ActionId::StatusHolderKill
            | ActionId::StatusLockForceUnlock
            | ActionId::StatusDaemonRestart
            | ActionId::ExportPreview
            | ActionId::ExportClipboardCopy => DispatchOutcome::NoTarget,
            // The plan-next-day surface (#622, D-4/D-6): the opener is
            // the routed reachability seam; the row verbs resolve
            // their one-shot target (`NoTarget` without one).
            ActionId::ViewPlanNextDay => DispatchOutcome::UiEffect(UiEffect::OpenPlanNextDay),
            ActionId::PlanBeginPlace => match ctx.plan_target {
                Some(task) => DispatchOutcome::UiEffect(UiEffect::PlanBeginPlace(task)),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::PlanCommitPlace => DispatchOutcome::UiEffect(UiEffect::PlanCommitPlace),
            ActionId::PlanBoxEarlier => match ctx.plan_target {
                Some(task) => DispatchOutcome::UiEffect(UiEffect::PlanBoxEarlier(task)),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::PlanBoxLater => match ctx.plan_target {
                Some(task) => DispatchOutcome::UiEffect(UiEffect::PlanBoxLater(task)),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::PlanRemoveBox => match ctx.plan_target {
                Some(task) => DispatchOutcome::UiEffect(UiEffect::PlanRemoveBox(task)),
                None => DispatchOutcome::NoTarget,
            },
            // The review walk (DR-7 chunk 3, #638): the opener and the
            // in-walk verbs are pure view intents — the adapter's
            // interpretation table owns them (the planner family's
            // shape; the reference-date derivation and the kiosk
            // refusal live there).
            ActionId::ReviewOpen => DispatchOutcome::UiEffect(UiEffect::OpenReviewWalk),
            ActionId::ReviewWalkCursorUp => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::CursorUp))
            }
            ActionId::ReviewWalkCursorDown => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::CursorDown))
            }
            ActionId::ReviewWalkBack => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Back))
            }
            ActionId::ReviewWalkAdvance => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Advance))
            }
            ActionId::ReviewInboxFile => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::InboxFile))
            }
            ActionId::ReviewInboxKeep => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::InboxKeep))
            }
            ActionId::ReviewFileTargetPrev => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::FileTargetPrev))
            }
            ActionId::ReviewFileTargetNext => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::FileTargetNext))
            }
            ActionId::ReviewFileCommit => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::FileCommit))
            }
            ActionId::ReviewCarryTomorrow => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::CarryTomorrow))
            }
            ActionId::ReviewCarryPickDate => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::CarryPickDate))
            }
            ActionId::ReviewCarryLeave => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::CarryLeave))
            }
            ActionId::ReviewCarryDateCommit => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::CarryDateCommit))
            }
            ActionId::ReviewRecord => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Record))
            }
            ActionId::ReviewFinish => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Finish))
            }
            ActionId::ReviewWalkPlanner => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Planner))
            }
            ActionId::ReviewWalkPrev => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Prev))
            }
            ActionId::ReviewWalkComplete => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::Complete))
            }
            ActionId::ReviewAbortArm => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::AbortArm))
            }
            ActionId::ReviewAbortAccept => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::AbortAccept))
            }
            ActionId::ReviewAbortDismiss => {
                DispatchOutcome::UiEffect(UiEffect::ReviewWalk(WalkIntent::AbortDismiss))
            }
            // Reserved (§5.8) — PaletteSearch opens the palette; the
            // only disabled row left (D6) is AppHelp (the undo rows
            // dispatch at the top of the match).
            ActionId::PaletteSearch => DispatchOutcome::UiEffect(UiEffect::OpenPalette),
            // The status panel's opener (OP3 W-1): the toggle and the
            // snapshot-on-open query live in the adapter's arm.
            ActionId::StatusPanelOpen => DispatchOutcome::UiEffect(UiEffect::OpenStatusPanel),
            ActionId::AppHelp => DispatchOutcome::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::{HabitId, TaskId};
    use uuid::Uuid;

    // ---- HF4 fold (US-5 AC-4, R-6) ---------------------------------

    fn draft_for(target: taskboard_domain::HabitTarget) -> HabitEditDraft {
        HabitEditDraft::from_habit(&taskboard_domain::habit::Habit {
            id: HabitId::from_uuid(Uuid::now_v7()),
            board: taskboard_domain::BoardId::from_uuid(Uuid::now_v7()),
            title: "run".into(),
            description: String::new(),
            target,
            order: 0,
            anchor_time: None,
            anchor_day: None,
            paused_until: None,
            scheduled_days: None,
            reminder: None,
            clocks: taskboard_domain::habit::HabitClocks::all(chrono::DateTime::UNIX_EPOCH),
            continued_from: None,
            deleted: false,
            by: None,
        })
    }

    /// `editor_gates_the_weekday_picker_by_target_kind` (W-6's fold,
    /// US-5 AC-4): the picker mounts for week/month habits and is
    /// *absent* for `PerDay` (R-6 reversed — a daily habit happens every
    /// day; the field is absent by construction, not hidden).
    #[test]
    fn editor_gates_the_weekday_picker_by_target_kind() {
        assert!(
            !draft_for(taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(1).expect("1")
            ))
            .schedule_picker_visible()
        );
        assert!(
            draft_for(taskboard_domain::HabitTarget::PerWeek(
                taskboard_domain::NonZeroCount::new(1).expect("1")
            ))
            .schedule_picker_visible()
        );
        assert!(
            draft_for(taskboard_domain::HabitTarget::PerMonth(
                taskboard_domain::NonZeroCount::new(1).expect("1")
            ))
            .schedule_picker_visible()
        );
    }

    /// US-5 AC-3's draft half: an edited-but-empty selection is a
    /// legal draft whose commit the save gate blocks (the typed
    /// invalid draft), and a toggled set mints into the sparse
    /// changeset (clearing = the full set rides `WeekdaySet::all`).
    #[test]
    fn the_schedule_picker_blocks_the_empty_selection_and_mints_the_set() {
        let mut draft = draft_for(taskboard_domain::HabitTarget::PerWeek(
            taskboard_domain::NonZeroCount::new(1).expect("1"),
        ));
        let stored = draft.clone().stored_twin();
        // Toggle Mon on, then off: empty selection, commit blocked.
        draft.toggle_schedule_slot(0);
        assert_eq!(draft.schedule_bits, 1);
        draft.toggle_schedule_slot(0);
        assert_eq!(draft.schedule_bits, 0);
        assert!(
            !draft.can_save(&stored),
            "the edited empty selection blocks the save"
        );
        // The `all` leg: every slot on mints the full set (the clear
        // posture).
        for slot in 0..7_usize {
            draft.toggle_schedule_slot(slot);
        }
        let changes = draft.changes(&stored);
        assert_eq!(
            changes.scheduled_days,
            Some(taskboard_domain::habit::WeekdaySet::all()),
            "clearing = the full seven-day set"
        );
        assert!(draft.can_save(&stored));
    }

    fn ctx(task: bool) -> DispatchCtx {
        DispatchCtx {
            focused_task: task.then(|| TaskId::from_uuid(Uuid::now_v7())),
            focused_stack: None,
            habit_pause_target: None,
            habit_move_up: None,
            habit_move_down: None,
            task_move: None,
            create_target: None,
            reminder_target: None,
            board_target: None,
            rule_target: None,
            rule_add_when: None,
            timer_draft: None,
            track_entry_draft: None,
            track_entry_target: None,
            timer_target: None,
            break_rule_draft: None,
            break_rule_target: None,
            alarm_row: None,
            track_target: None,
            habit_target: None,
            habit_undo: None,
            habit_move: None,
            habit_repair: None,
            habit_convert_target: None,
            habit_remove_target: None,
            habit_restore_target: None,
            habit_create_draft: None,
            habit_quota_draft: None,
            pattern_draft: None,
            plan_target: None,
            series_target: None,
            pause_draft: None,
            convert_target: None,
            recur_edit_target: None,
            recur_draft_rule: None,
            day_window: None,
            today_last_instant: None,
            tomorrow_window_end: None,
            focused_task_done: false,
            focused_task_tracker: false,
            focused_task_scheduled: false,
            focused_task_tracking: false,
            focused_task_effort: false,
        }
    }

    fn banner_target() -> crate::presentation::ReminderTarget {
        crate::presentation::ReminderTarget {
            anchor: taskboard_domain::ReminderAnchor::Task(TaskId::from_uuid(Uuid::now_v7())),
            reminder: taskboard_domain::ReminderRef::Default(TaskId::from_uuid(Uuid::now_v7())),
            occurrence: chrono::Utc::now(),
        }
    }

    /// UX-R8 chunk-5 D-3 (US-1/S1–S3) — the scheduled field's quick
    /// verbs route the exact sparse `scheduled` mask: today's last
    /// covered instant (#893), tomorrow's window end, the `Some(None)`
    /// clear twin; every other mask stays `None`. The backstops: no
    /// focus → `NoTarget` for all three; unschedule on an unscheduled
    /// subject → `NoTarget` (the stale-absorb posture).
    #[test]
    fn the_schedule_trio_routes_the_exact_sparse_masks() {
        let end = chrono::Utc::now();
        let mut scheduled = ctx(true);
        scheduled.day_window = Some((end, end));
        scheduled.today_last_instant = Some(end);
        scheduled.tomorrow_window_end = Some(end);
        scheduled.focused_task_scheduled = true;
        let expected = |end: chrono::DateTime<chrono::Utc>| {
            DispatchOutcome::Command(StateCommand::UpdateTask {
                id: scheduled.focused_task.expect("focused"),
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(end)),
                    ..taskboard_domain::TaskChanges::default()
                },
            })
        };
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskScheduleToday, &scheduled),
            expected(end),
            "today commits the day's last covered instant"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskScheduleTomorrow, &scheduled),
            expected(end),
            "tomorrow commits the next window's end"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskUnschedule, &scheduled),
            DispatchOutcome::Command(StateCommand::UpdateTask {
                id: scheduled.focused_task.expect("focused"),
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(None),
                    ..taskboard_domain::TaskChanges::default()
                },
            }),
            "unschedule commits the clear twin"
        );

        // The backstops: target-free contexts refuse typed.
        let bare = ctx(false);
        for id in [
            ActionId::TaskScheduleToday,
            ActionId::TaskScheduleTomorrow,
            ActionId::TaskUnschedule,
        ] {
            assert_eq!(
                ActionRouter::dispatch(id, &bare),
                DispatchOutcome::NoTarget,
                "{id} without a focused task is NoTarget"
            );
        }
        // Unscheduling an unscheduled subject refuses (the withheld
        // row's router backstop).
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskUnschedule, &ctx(true)),
            DispatchOutcome::NoTarget
        );
        // #893's backstop shape: a window without the stamp fact
        // refuses too — the today verbs route from the unified mint,
        // never from the raw window end.
        let mut windowless_stamp = ctx(true);
        windowless_stamp.day_window = Some((end, end));
        for id in [ActionId::TaskDueToday, ActionId::TaskScheduleToday] {
            assert_eq!(
                ActionRouter::dispatch(id, &windowless_stamp),
                DispatchOutcome::NoTarget,
                "{id} without the last-covered-instant fact is NoTarget"
            );
        }
    }

    /// DR-3/3 — the timer verbs route exact typed commands from their
    /// one-shot targets, `NoTarget` without (the rule-verb pattern).
    #[test]
    fn timer_verbs_route_from_their_targets() {
        let mut ctx = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::TimerCreate, &ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TimerCancel, &ctx),
            DispatchOutcome::NoTarget
        );
        let when = taskboard_domain::AdhocWhen::In(chrono::TimeDelta::minutes(3));
        ctx.timer_draft = Some(TimerDraft {
            label: "tea".to_owned(),
            when,
            insistence: taskboard_domain::Insistence::Normal,
        });
        assert_eq!(
            ActionRouter::dispatch(ActionId::TimerCreate, &ctx),
            DispatchOutcome::Command(StateCommand::CreateAdhocAlarm {
                label: "tea".to_owned(),
                when,
                insistence: taskboard_domain::Insistence::Normal,
            })
        );
        // The alarm rung (r4 D-T7): the draft's rung routes verbatim —
        // the router never defaults.
        ctx.timer_draft.as_mut().expect("draft").insistence = taskboard_domain::Insistence::Alarm;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TimerCreate, &ctx),
            DispatchOutcome::Command(StateCommand::CreateAdhocAlarm {
                label: "tea".to_owned(),
                when,
                insistence: taskboard_domain::Insistence::Alarm,
            })
        );
        let alarm = taskboard_domain::AdhocAlarmId::from_uuid(Uuid::now_v7());
        ctx.timer_target = Some(alarm);
        assert_eq!(
            ActionRouter::dispatch(ActionId::TimerCancel, &ctx),
            DispatchOutcome::Command(StateCommand::CancelAdhocAlarm { alarm })
        );
    }

    /// T-14 (#797) — the break-rule verbs route exact typed commands
    /// from their one-shot targets: the create from the parsed draft
    /// (board included), the confirm from the armed target, the arm
    /// step never dispatching (staging lives adapter-side).
    #[test]
    fn break_rule_verbs_route_from_their_targets() {
        let mut ctx = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::BreakRuleCreate, &ctx),
            DispatchOutcome::NoTarget
        );
        // US-9 (W-10, #1066's roster half): the arm step re-rooted
        // onto its registry id as a view intent — the staging stays
        // adapter-side, the router routes the effect.
        assert_eq!(
            ActionRouter::dispatch(ActionId::BreakRuleRemove, &ctx),
            DispatchOutcome::UiEffect(UiEffect::BreakRuleRemoveArm),
            "the arm step is adapter-side staging — it dispatches nothing engine-side"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::BreakRuleRemoveConfirm, &ctx),
            DispatchOutcome::NoTarget
        );
        let board = taskboard_domain::BoardId::from_uuid(Uuid::now_v7());
        let threshold = taskboard_domain::WorkThreshold::try_new(chrono::TimeDelta::minutes(90))
            .expect("threshold");
        ctx.break_rule_draft = Some(BreakRuleDraft {
            board,
            label: "tea".to_owned(),
            threshold,
            insistence: taskboard_domain::Insistence::Alarm,
        });
        assert_eq!(
            ActionRouter::dispatch(ActionId::BreakRuleCreate, &ctx),
            DispatchOutcome::Command(StateCommand::CreateBreakRule {
                board: Some(board),
                label: "tea".to_owned(),
                threshold,
                insistence: taskboard_domain::Insistence::Alarm,
            })
        );
        let rule = taskboard_domain::BreakRuleId::from_uuid(Uuid::now_v7());
        ctx.break_rule_target = Some(rule);
        assert_eq!(
            ActionRouter::dispatch(ActionId::BreakRuleRemoveConfirm, &ctx),
            DispatchOutcome::Command(StateCommand::DeleteBreakRule { id: rule })
        );
    }

    /// T-22 (#805) — the alarm window's keyboard verbs route the
    /// same commands the pointer buttons build: both sides call the
    /// shared `presentation::alarm_command` constructor, so the
    /// routed command is structurally the button's (click ≡ key);
    /// `NoTarget` without a row.
    #[test]
    fn alarm_verbs_route_the_button_commands_from_the_selected_row() {
        let mut ctx = ctx(false);
        for verb in [
            ActionId::AlarmDismiss,
            ActionId::AlarmSnooze,
            ActionId::AlarmCancel,
        ] {
            assert_eq!(
                ActionRouter::dispatch(verb, &ctx),
                DispatchOutcome::NoTarget
            );
        }
        let row = crate::presentation::AlarmRow {
            alarm: taskboard_domain::AdhocAlarmId::from_uuid(Uuid::now_v7()),
            reminder: taskboard_domain::ReminderRef::Adhoc(
                taskboard_domain::AdhocAlarmId::from_uuid(Uuid::now_v7()),
            ),
            occurrence: chrono::Utc::now(),
            title: "tea".to_owned(),
        };
        ctx.alarm_row = Some(row.clone());
        assert_eq!(
            ActionRouter::dispatch(ActionId::AlarmDismiss, &ctx),
            DispatchOutcome::Command(crate::presentation::alarm_command(
                &row,
                crate::presentation::AlarmVerb::Dismiss
            ))
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::AlarmSnooze, &ctx),
            DispatchOutcome::Command(crate::presentation::alarm_command(
                &row,
                crate::presentation::AlarmVerb::Snooze
            ))
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::AlarmCancel, &ctx),
            DispatchOutcome::Command(crate::presentation::alarm_command(
                &row,
                crate::presentation::AlarmVerb::Cancel
            ))
        );
    }

    /// DR-5/4 — the tracking verbs route exact typed commands: Start
    /// prefers the explicit one-shot target over focus, falls back to
    /// focus, and is `NoTarget` with neither; Stop routes the exact
    /// payload-free command unconditionally.
    #[test]
    fn tracking_verbs_route_exact_commands_from_their_two_sources() {
        let task = TaskId::from_uuid(Uuid::now_v7());
        let focused = TaskId::from_uuid(Uuid::now_v7());

        let mut empty = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackStart, &empty),
            DispatchOutcome::NoTarget,
            "neither source set"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackStop, &empty),
            DispatchOutcome::Command(StateCommand::StopTracking),
            "Stop is unconditional (the planner's total NoOp owns nothing-open)"
        );

        empty.track_target = Some(task);
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackStart, &empty),
            DispatchOutcome::Command(StateCommand::StartTracking {
                task,
                kind: taskboard_domain::tracking::TimeEntryKind::Work
            }),
            "the explicit target wins"
        );

        let focused_ctx = ctx(true);
        let focused_task = focused_ctx.focused_task.expect("set");
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackStart, &focused_ctx),
            DispatchOutcome::Command(StateCommand::StartTracking {
                task: focused_task,
                kind: taskboard_domain::tracking::TimeEntryKind::Work
            }),
            "the context menu's and detail button's focused-task path"
        );

        let mut both = ctx(true);
        both.track_target = Some(task);
        both.focused_task = Some(focused);
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackStart, &both),
            DispatchOutcome::Command(StateCommand::StartTracking {
                task,
                kind: taskboard_domain::tracking::TimeEntryKind::Work
            }),
            "the notification row's target is table addressing, not focus"
        );
    }

    /// The smart toggle routes exactly the two tracking verbs'
    /// outcomes (J3 R-8, round 9 chunk 11 — no new domain command):
    /// a target holding the open entry stops; a target without one
    /// starts (the target = the one-shot, else the focused task — the
    /// card seams' resolution); no target anywhere is `NoTarget`.
    #[test]
    fn the_smart_toggle_routes_the_state_correct_tracking_half() {
        let task = TaskId::from_uuid(Uuid::now_v7());

        // Running on the target: the stop half.
        let mut running = ctx(true);
        running.track_target = Some(task);
        running.focused_task_tracking = true;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackToggle, &running),
            DispatchOutcome::Command(StateCommand::StopTracking),
            "the open entry on the target routes the stop"
        );

        // Idle: the start half, addressed at the resolved target.
        let mut idle = ctx(false);
        idle.track_target = Some(task);
        idle.focused_task_tracking = false;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackToggle, &idle),
            DispatchOutcome::Command(StateCommand::StartTracking {
                task,
                kind: taskboard_domain::tracking::TimeEntryKind::Work
            }),
            "an idle target routes the start over it"
        );

        // Running elsewhere (tracking fact false) still starts over
        // the target — the engine's own law closes the foreign entry.
        let mut elsewhere = ctx(true);
        elsewhere.track_target = Some(task);
        elsewhere.focused_task_tracking = false;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackToggle, &elsewhere),
            DispatchOutcome::Command(StateCommand::StartTracking {
                task,
                kind: taskboard_domain::tracking::TimeEntryKind::Work
            }),
            "a running-elsewhere target still routes the start"
        );

        // No target: the honest NoTarget.
        let none = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskTrackToggle, &none),
            DispatchOutcome::NoTarget,
            "no one-shot and no focused task is NoTarget"
        );
    }

    /// The effort pair's route gates (J3 US-4, round 9 chunk 11):
    /// edit is a pure focus effect (no command, ever); clear routes
    /// the sparse `Some(None)` mask only over a subject that actually
    /// carries an estimate — the scheduled-fact's gate shape.
    #[test]
    fn the_effort_pair_gates_on_the_subject_and_the_estimate() {
        // Edit: always the focus effect.
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskEffortEdit, &ctx(true)),
            DispatchOutcome::UiEffect(UiEffect::BeginEffortEdit),
            "edit is the focus verb, subject or not"
        );

        // Clear with an estimate: the sparse mask.
        let mut armed = ctx(true);
        armed.focused_task_effort = true;
        assert!(
            matches!(
                ActionRouter::dispatch(ActionId::TaskEffortClear, &armed),
                DispatchOutcome::Command(StateCommand::UpdateTask {
                    changes: taskboard_domain::TaskChanges {
                        effort: Some(None),
                        ..
                    },
                    ..
                })
            ),
            "an estimate-carrying subject clears confirmation-free"
        );
        // Clear without an estimate, or without a subject: NoTarget.
        let mut bare = ctx(true);
        bare.focused_task_effort = false;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskEffortClear, &bare),
            DispatchOutcome::NoTarget,
            "no estimate: nothing to clear"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskEffortClear, &ctx(false)),
            DispatchOutcome::NoTarget,
            "no subject: nothing to address"
        );
    }

    /// DR-6/6 — the habit verbs route exact typed commands from their
    /// row targets; a target-less dispatch is the honest `NoTarget`
    /// (the menu can never invoke them — section-scoped entries).
    #[test]
    fn habit_verbs_route_from_their_row_targets() {
        let habit = taskboard_domain::HabitId::from_uuid(Uuid::now_v7());
        let mut plus = ctx(true);
        plus.habit_target = Some(habit);
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitLog, &plus),
            DispatchOutcome::Command(StateCommand::LogHabit { habit })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitLog, &ctx(true)),
            DispatchOutcome::NoTarget
        );
        // The −1 recovery (UX R-2 D-3, #786): the router owns the
        // arithmetic — the dispatch carries `count: n − 1`; count 0
        // is the honest `NoTarget` (no negative log exists).
        let window = taskboard_domain::WindowStart::of_period(
            taskboard_domain::PeriodKind::Day,
            chrono::NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
        );
        let mut undo_ctx = ctx(true);
        undo_ctx.habit_undo = Some((habit, window, 2));
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitUndoLog, &undo_ctx),
            DispatchOutcome::Command(StateCommand::AdjustHabitLog {
                habit,
                window,
                count: 1,
            })
        );
        let mut zero_ctx = ctx(true);
        zero_ctx.habit_undo = Some((habit, window, 0));
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitUndoLog, &zero_ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitUndoLog, &ctx(true)),
            DispatchOutcome::NoTarget
        );
        // #1198 chunk A1 D1: the one-shot carries a target gap index
        // and routes the engine-side reorder; the absolute-order
        // `MoveHabit` stays the CLI's verb.
        let mut move_ctx = ctx(true);
        move_ctx.habit_move = Some((habit, 15));
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitMove, &move_ctx),
            DispatchOutcome::Command(StateCommand::HabitReorder { habit, target: 15 })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitMove, &ctx(true)),
            DispatchOutcome::NoTarget
        );
    }

    /// T22 — the enabled task verbs route to exact typed commands.
    #[test]
    fn task_verbs_route_to_exact_commands() {
        let ctx = ctx(true);
        let task = ctx.focused_task.expect("set");
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskComplete, &ctx),
            DispatchOutcome::Command(StateCommand::SetTaskDone {
                id: task,
                done: true
            })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskDelete, &ctx),
            DispatchOutcome::Command(StateCommand::DeleteTask { id: task })
        );
    }

    /// T22 — enabled task verbs without focus resolve to `NoTarget`,
    /// an absorbed no-op (chunk-2 sessions have no views).
    #[test]
    fn task_verbs_without_focus_are_no_target() {
        let ctx = DispatchCtx::default();
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskComplete, &ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskDelete, &ctx),
            DispatchOutcome::NoTarget
        );
    }

    /// Chunk 6 (D11): the rule-list verbs follow the banner-verb
    /// pattern — exact typed commands from the one-shot targets,
    /// `NoTarget` without them (a menu invocation carries neither).
    #[test]
    fn rule_verbs_route_from_their_targets() {
        let mut ctx = ctx(true);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::NoTarget,
            "no offset"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleRemove, &ctx),
            DispatchOutcome::NoTarget,
            "no rule target"
        );
        ctx.rule_add_when = Some(crate::presentation::RuleWhen::DueOffset(
            taskboard_domain::ReminderOffset::before(chrono::TimeDelta::try_minutes(-5).unwrap())
                .unwrap(),
        ));
        let task = ctx.focused_task.expect("set");
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::Command(StateCommand::AddReminderRule {
                task,
                offset: taskboard_domain::ReminderOffset::before(
                    chrono::TimeDelta::try_minutes(-5).unwrap()
                )
                .unwrap(),
                at: None,
                insistence: taskboard_domain::Insistence::Normal,
            })
        );
        // The at/in draft rides the `at` input (r4 decision 2); the
        // legacy offset field is the ignored at-due default.
        ctx.rule_add_when = Some(crate::presentation::RuleWhen::At(
            taskboard_domain::AdhocWhen::In(chrono::TimeDelta::try_minutes(90).unwrap()),
        ));
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::Command(StateCommand::AddReminderRule {
                task,
                offset: taskboard_domain::ReminderOffset::at_due(),
                at: Some(taskboard_domain::AdhocWhen::In(
                    chrono::TimeDelta::try_minutes(90).unwrap()
                )),
                insistence: taskboard_domain::Insistence::Normal,
            })
        );
        let rule = taskboard_domain::ReminderRuleId::from_uuid(Uuid::now_v7());
        ctx.rule_target = Some(rule);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleRemove, &ctx),
            DispatchOutcome::Command(StateCommand::RemoveReminderRule { rule })
        );
    }

    /// Chunk 6 (D11): the add verb needs focus AND offset — focus alone
    /// is `NoTarget`.
    #[test]
    fn rule_add_without_focus_or_offset_is_no_target() {
        let mut ctx = ctx(false);
        ctx.rule_add_when = Some(crate::presentation::RuleWhen::DueOffset(
            taskboard_domain::ReminderOffset::at_due(),
        ));
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::NoTarget
        );
    }

    /// T22 — the disabled bit wins before any target check (§5.8:
    /// reserved actions are honest no-ops even with focus set).
    /// D4/D6 — the banner verbs route exact typed commands from the
    /// reminder target, `NoTarget` without one.
    #[test]
    fn reminder_verbs_route_from_the_target() {
        let mut ctx = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderSnooze, &ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderMute, &ctx),
            DispatchOutcome::NoTarget
        );
        let target = banner_target();
        ctx.reminder_target = Some(target);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderSnooze, &ctx),
            DispatchOutcome::Command(StateCommand::SnoozeReminder {
                reminder: target.reminder,
                occurrence: target.occurrence,
                postpone: SNOOZE_POSTPONE,
            })
        );
        let taskboard_domain::ReminderAnchor::Task(task_id) = target.anchor else {
            unreachable!("the fixture builds a task anchor");
        };
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderMute, &ctx),
            DispatchOutcome::Command(StateCommand::SetTaskRemindersMuted {
                task: task_id,
                muted: true,
            })
        );
    }

    /// D6 — the staged view intents route to their named effects.
    #[test]
    fn view_intents_route_to_effects() {
        for (id, effect) in [
            (ActionId::NavNextView, UiEffect::NextView),
            (ActionId::NavPrevView, UiEffect::PrevView),
            (ActionId::NavFocusBoard, UiEffect::FocusBoard),
            (ActionId::NavNextItem, UiEffect::FocusNext),
            (ActionId::NavPrevItem, UiEffect::FocusPrev),
            (ActionId::TaskCreate, UiEffect::BeginCreate { target: None }),
            (ActionId::StackCreate, UiEffect::BeginCreateStack),
            (ActionId::TaskEdit, UiEffect::Activate),
            (ActionId::TaskTitleEdit, UiEffect::BeginTitleEdit),
            (ActionId::TaskMove, UiEffect::RequestMove),
            // Round 2 (D16–D21): the round-up verbs route to their
            // named effects.
            (ActionId::TaskMoveCancel, UiEffect::CancelMove),
            (ActionId::StackNext, UiEffect::StackNext),
            (ActionId::StackPrev, UiEffect::StackPrev),
            (ActionId::StackDelete, UiEffect::StackDelete),
            (ActionId::StackRename, UiEffect::StackRename),
            (ActionId::StackMove, UiEffect::StackMove),
            (ActionId::BoardCreate, UiEffect::BoardCreate),
            (ActionId::BoardDelete, UiEffect::BoardDelete),
            (ActionId::BoardArchive, UiEffect::BoardArchive),
            (ActionId::BoardReset, UiEffect::BoardReset),
            (ActionId::BoardReconcile, UiEffect::BoardReconcile),
            (ActionId::BoardRename, UiEffect::BoardRename),
            (ActionId::NavFirstItem, UiEffect::FocusFirst),
            (ActionId::NavLastItem, UiEffect::FocusLast),
            // The directional-focus family (J3 US-1, round 9 chunk 9):
            // one direction-bearing effect, plus the extend twin (#875).
            (
                ActionId::NavFocusLeft,
                UiEffect::FocusDirectional(Direction::Left),
            ),
            (
                ActionId::NavFocusRight,
                UiEffect::FocusDirectional(Direction::Right),
            ),
            (
                ActionId::NavFocusUp,
                UiEffect::FocusDirectional(Direction::Up),
            ),
            (
                ActionId::NavFocusDown,
                UiEffect::FocusDirectional(Direction::Down),
            ),
            (ActionId::NavExtendNext, UiEffect::ExtendSelectionNext),
            (ActionId::NavExtendPrev, UiEffect::ExtendSelectionPrev),
        ] {
            assert_eq!(
                ActionRouter::dispatch(id, &ctx(true)),
                DispatchOutcome::UiEffect(effect)
            );
        }
    }

    /// J3 US-1 (round 9 chunk 9, W-2): the directional family rides
    /// the walk's `EffectClass::Nav` class — no new matrix row, so a
    /// foreign flow settles-then-runs exactly as `FocusNext` does
    /// today. The class is the resolver's only contract with the
    /// effects; the interpretation table consumes the direction.
    #[test]
    fn the_directional_family_rides_the_nav_class() {
        for effect in [
            UiEffect::FocusDirectional(Direction::Left),
            UiEffect::FocusDirectional(Direction::Right),
            UiEffect::FocusDirectional(Direction::Up),
            UiEffect::FocusDirectional(Direction::Down),
            UiEffect::ExtendSelectionNext,
            UiEffect::ExtendSelectionPrev,
        ] {
            assert_eq!(effect.class(), EffectClass::Nav);
        }
    }

    /// Round 2 (D19, the dispatch (f) enablement pin): `task.reopen`
    /// routes the exact inverted `SetTaskDone` for a focused completed
    /// task, and absorbs as `NoTarget` for an open or absent one.
    #[test]
    fn task_reopen_routes_only_for_a_focused_done_task() {
        let mut ctx = ctx(true);
        ctx.focused_task_done = true;
        let task = ctx.focused_task.expect("set");
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskReopen, &ctx),
            DispatchOutcome::Command(StateCommand::SetTaskDone {
                id: task,
                done: false
            })
        );
        ctx.focused_task_done = false;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskReopen, &ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskReopen, &DispatchCtx::default()),
            DispatchOutcome::NoTarget
        );
    }

    /// Design §3.8 — the board verbs: next/previous route to their
    /// effects; `board.select` routes the parameterized target and
    /// absorbs as `NoTarget` without one. Never a `Command` — the
    /// dumb-client pin at the routing layer.
    #[test]
    fn board_verbs_route_to_effects_and_the_parameterized_target() {
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardNext, &ctx(true)),
            DispatchOutcome::UiEffect(UiEffect::BoardNext)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardPrevious, &ctx(true)),
            DispatchOutcome::UiEffect(UiEffect::BoardPrevious)
        );
        let board = taskboard_domain::BoardId::from_uuid(Uuid::now_v7());
        let mut ctx = ctx(true);
        ctx.board_target = Some(board);
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardSelect, &ctx),
            DispatchOutcome::UiEffect(UiEffect::BoardSelect(board))
        );
        ctx.board_target = None;
        // UX R-4 D-4 (#794): the bare stroke is no longer the useless
        // `NoTarget` — it opens the switcher flyout (the target-free
        // flow; the adapter owns the trigger).
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardSelect, &ctx),
            DispatchOutcome::UiEffect(UiEffect::OpenBoardBarFlyout)
        );
    }

    #[test]
    fn disabled_actions_are_absorbed_even_with_focus() {
        let ctx = ctx(true);
        // AppHelp is the only disabled row left (the undo rows flipped
        // live in undo/2).
        assert_eq!(
            ActionRouter::dispatch(ActionId::AppHelp, &ctx),
            DispatchOutcome::Disabled
        );
        // CaptureQuick flipped live (chunk 10, D-V6): it yields the
        // capture effect now, focus or not. PaletteSearch flipped the
        // same way (chunk 9).
        assert_eq!(
            ActionRouter::dispatch(ActionId::PaletteSearch, &ctx),
            DispatchOutcome::UiEffect(UiEffect::OpenPalette)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::CaptureQuick, &ctx),
            DispatchOutcome::UiEffect(UiEffect::BeginCapture)
        );
    }

    /// Undo v1 surfaces (undo/2, ADR 0019): the two rows dispatch the
    /// target-free engine verbs — no `NoTarget` case exists, so the
    /// outcome is focus-independent (the engine's typed refusals are
    /// the only "no", D-4).
    #[test]
    fn app_undo_and_redo_dispatch_the_engine_verbs() {
        for focused in [false, true] {
            let ctx = ctx(focused);
            assert_eq!(
                ActionRouter::dispatch(ActionId::AppUndo, &ctx),
                DispatchOutcome::Command(StateCommand::Undo)
            );
            assert_eq!(
                ActionRouter::dispatch(ActionId::AppRedo, &ctx),
                DispatchOutcome::Command(StateCommand::Redo)
            );
        }
    }

    /// T22 — the UI-internal effect table.
    #[test]
    fn ui_internal_actions_yield_effects() {
        let ctx = DispatchCtx::default();
        assert_eq!(
            ActionRouter::dispatch(ActionId::AppExit, &ctx),
            DispatchOutcome::UiEffect(UiEffect::Exit)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::AppMenu, &ctx),
            DispatchOutcome::UiEffect(UiEffect::OpenMenu)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::MenuDismiss, &ctx),
            DispatchOutcome::UiEffect(UiEffect::MenuBack),
            "menu.dismiss is the level-aware back (round-1 D6)"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::MenuNext, &ctx),
            DispatchOutcome::UiEffect(UiEffect::MenuNext)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ModeInsert, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Insert))
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ModeNormal, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Normal))
        );
    }

    // ---- the tracker task kind (backlog round 2 chunk 4.2, #492) ------

    /// U4 — the keyboard path's kind guard: a focused tracker task's
    /// complete dispatch is inert (`NoTarget` — the focusless shape),
    /// the reopen gate keeps its done gate, and the toggle dispatches
    /// the inverted plain kind edit (no new domain command).
    #[test]
    fn the_router_declines_complete_on_a_focused_tracker() {
        let mut tracker = ctx(true);
        tracker.focused_task_tracker = true;
        let id = tracker.focused_task.expect("the focused tracker");
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskComplete, &tracker),
            DispatchOutcome::NoTarget,
            "the tracker's complete is an inert dispatch"
        );
        // The standard path is byte-identical to the pre-kind pin.
        assert!(matches!(
            ActionRouter::dispatch(ActionId::TaskComplete, &ctx(true)),
            DispatchOutcome::Command(StateCommand::SetTaskDone { done: true, .. })
        ));

        let mut plus = tracker.clone();
        plus.focused_task_done = true;
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskReopen, &plus),
            DispatchOutcome::Command(StateCommand::SetTaskDone { id, done: false }),
            "the reopen gate is the done gate, not the kind gate"
        );
        // The toggle: a tracker resets, a standard marks, no focus is
        // NoTarget.
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskToggleTracker, &tracker),
            DispatchOutcome::Command(StateCommand::UpdateTask {
                id,
                changes: taskboard_domain::TaskChanges {
                    kind: Some(Some(taskboard_domain::entities::TaskKind::Standard)),
                    ..taskboard_domain::TaskChanges::default()
                }
            })
        );
        let standard = ctx(true);
        let sid = standard.focused_task.expect("the focused standard");
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskToggleTracker, &standard),
            DispatchOutcome::Command(StateCommand::UpdateTask {
                id: sid,
                changes: taskboard_domain::TaskChanges {
                    kind: Some(Some(taskboard_domain::entities::TaskKind::Tracker)),
                    ..taskboard_domain::TaskChanges::default()
                }
            })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskToggleTracker, &ctx(false)),
            DispatchOutcome::NoTarget
        );
    }

    // ---- the habit trio (issue #506, plan D-7/D-8): the router
    // routes exact typed commands from the one-shot targets ----------

    #[test]
    fn the_recur_and_repair_verbs_route_from_their_targets() {
        let mut ctx = ctx(false);
        for id in [
            ActionId::HabitRepair,
            ActionId::HabitConvertToSeries,
            ActionId::SeriesPauseCommit,
            ActionId::SeriesResume,
            ActionId::SeriesConvertToHabit,
        ] {
            assert_eq!(ActionRouter::dispatch(id, &ctx), DispatchOutcome::NoTarget);
        }
        // The two arm intents are view effects, even targetless.
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitConvertArm, &ctx),
            DispatchOutcome::UiEffect(UiEffect::HabitConvertArm)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesConvertArm, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SeriesConvertArm)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesPause, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SeriesPauseOpen)
        );
        // The make-up valve dispatches the exact audited payload.
        let (habit, window, count) = (
            HabitId::from_uuid(Uuid::now_v7()),
            taskboard_domain::WindowStart::of_period(
                taskboard_domain::PeriodKind::Day,
                chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
            ),
            2_u32,
        );
        ctx.habit_repair = Some((habit, window, count));
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitRepair, &ctx),
            DispatchOutcome::Command(StateCommand::AdjustHabitLog {
                habit,
                window,
                count,
            })
        );
        // The confirm dispatches carry the drafts verbatim.
        let head = TaskId::from_uuid(Uuid::now_v7());
        let pause = chrono::NaiveDate::from_ymd_opt(2026, 12, 24).unwrap();
        ctx.series_target = Some(head);
        ctx.pause_draft = Some(pause);
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesPauseCommit, &ctx),
            DispatchOutcome::Command(StateCommand::PauseSeries {
                head,
                until: Some(pause),
            })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesResume, &ctx),
            DispatchOutcome::Command(StateCommand::PauseSeries { head, until: None })
        );
        ctx.convert_target = Some(taskboard_domain::habit::HabitTarget::PerWeek(
            taskboard_domain::NonZeroCount::new(3).unwrap(),
        ));
        let outcome = ActionRouter::dispatch(ActionId::SeriesConvertToHabit, &ctx);
        assert!(matches!(
            outcome,
            DispatchOutcome::Command(StateCommand::ConvertSeriesToHabit { .. })
        ));
        let pattern = taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::EveryDays::new(3).unwrap(),
        };
        ctx.habit_convert_target = Some(habit);
        ctx.pattern_draft = Some(pattern.clone());
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitConvertToSeries, &ctx),
            DispatchOutcome::Command(StateCommand::ConvertHabitToSeries { habit, pattern })
        );
    }

    /// The recurrence editor's verbs (round-7 D-7.1, plan D-C1/D-C2):
    /// the opener is a view intent; the commit routes the assembled
    /// rule as the one recurrence write; the detach routes
    /// `recur: Some(None)` — and nothing else rides its changeset; a
    /// stale arm (no one-shots) degrades to `NoTarget` (the
    /// stale-index law).
    #[test]
    fn the_recur_editor_actions_route_their_writes() {
        let mut ctx = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesEdit, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SeriesEditorOpen)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesEditDetachArm, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SeriesDetachArm)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesEditDetachCancel, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SeriesDetachCancel)
        );
        // The stale arm: no target, no draft — typed NoTarget.
        for action in [ActionId::SeriesEditCommit, ActionId::SeriesEditDetach] {
            assert_eq!(
                ActionRouter::dispatch(action, &ctx),
                DispatchOutcome::NoTarget
            );
        }
        // The commit's exact command: the assembled rule verbatim.
        let task = TaskId::from_uuid(Uuid::now_v7());
        let rule = taskboard_domain::RecurRule {
            series: taskboard_domain::SeriesId::from_uuid(Uuid::now_v7()),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            pattern: taskboard_domain::RecurPattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            bound: taskboard_domain::RecurBound::None,
            continued_from: None,
            paused_until: None,
        };
        ctx.recur_edit_target = Some(task);
        ctx.recur_draft_rule = Some(rule.clone());
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesEditCommit, &ctx),
            DispatchOutcome::Command(StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    recur: Some(Some(rule)),
                    ..taskboard_domain::TaskChanges::default()
                },
            })
        );
        // The detach: a target routes the clear — Some(None) and the
        // default mask's nothing else.
        ctx.recur_edit_target = Some(task);
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesEditDetach, &ctx),
            DispatchOutcome::Command(StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    recur: Some(None),
                    ..taskboard_domain::TaskChanges::default()
                },
            })
        );
        // A draft without a target is still the stale arm.
        ctx.recur_edit_target = None;
        ctx.recur_draft_rule = Some(taskboard_domain::RecurRule {
            series: taskboard_domain::SeriesId::from_uuid(Uuid::now_v7()),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            pattern: taskboard_domain::RecurPattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            bound: taskboard_domain::RecurBound::None,
            continued_from: None,
            paused_until: None,
        });
        assert_eq!(
            ActionRouter::dispatch(ActionId::SeriesEditCommit, &ctx),
            DispatchOutcome::NoTarget
        );
    }

    /// The plan-next-day surface (#622, D-4/D-6): the opener is the
    /// routed reachability seam; the row verbs resolve their one-shot
    /// target (`NoTarget` without one) and never a parallel write
    /// path.
    #[test]
    fn the_plan_next_day_actions_route_their_effects() {
        let task = TaskId::from_uuid(Uuid::now_v7());
        let mut ctx = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ViewPlanNextDay, &ctx),
            DispatchOutcome::UiEffect(UiEffect::OpenPlanNextDay)
        );
        for action in [
            ActionId::PlanBeginPlace,
            ActionId::PlanBoxEarlier,
            ActionId::PlanBoxLater,
            ActionId::PlanRemoveBox,
        ] {
            assert_eq!(
                ActionRouter::dispatch(action, &ctx),
                DispatchOutcome::NoTarget,
                "{action} without a row target"
            );
            ctx.plan_target = Some(task);
            assert_eq!(
                ActionRouter::dispatch(action, &ctx),
                DispatchOutcome::UiEffect(match action {
                    ActionId::PlanBeginPlace => UiEffect::PlanBeginPlace(task),
                    ActionId::PlanBoxEarlier => UiEffect::PlanBoxEarlier(task),
                    ActionId::PlanBoxLater => UiEffect::PlanBoxLater(task),
                    _ => UiEffect::PlanRemoveBox(task),
                })
            );
            ctx.plan_target = None;
        }
        assert_eq!(
            ActionRouter::dispatch(ActionId::PlanCommitPlace, &ctx),
            DispatchOutcome::UiEffect(UiEffect::PlanCommitPlace)
        );
    }
}

#[test]
fn one_shot_bits_are_total() {
    // The totalness pin (cross-surface friction plan D-6, #763):
    // walking every registered action's declaration, the union of the
    // declared bits equals exactly the full field set — a new ctx
    // one-shot that forgets its declaration here (an action, or the
    // field's bit) fails this test, not production. Structural and
    // typed: the bit constants are private, the getters are the
    // vocabulary.
    let full = OneShotSet::default()
        .create_target()
        .reminder_target()
        .board_target()
        .rule_target()
        .rule_add_when()
        .timer_draft()
        .timer_target()
        .break_rule_draft()
        .break_rule_target()
        .track_entry_draft()
        .track_entry_target()
        .track_target()
        .plan_target()
        .habit_target()
        .habit_undo()
        .habit_move()
        .habit_repair()
        .habit_convert_target()
        .habit_remove_target()
        .habit_restore_target()
        .habit_pause_target()
        .habit_create_draft()
        .habit_move_up()
        .habit_move_down()
        .pattern_draft()
        .series_target()
        .pause_draft()
        .convert_target()
        .recur_edit_target()
        .recur_draft_rule()
        .habit_quota_draft()
        .task_move();
    let mut seen = OneShotSet::default();
    for entry in crate::actions::REGISTRY {
        let declared = route_consumes(entry.id);
        // No declaration may set a bit outside the field vocabulary
        // (the `wants` probe reads raw bits; equality pins the shape).
        seen = OneShotSet(seen.0 | declared.0);
    }
    assert_eq!(
        seen, full,
        "the consumption declarations cover exactly the field set"
    );
    // The bit-uniqueness pin (the H2 re-pair's bug class): the OR is
    // blind to a duplicated bit, so the popcount of the ORed
    // vocabulary must equal the field count — one private bit per
    // one-shot, no two constants share a bit.
    // 32 fields: bits 0..=31 (the D1i HABIT_RESTORE_TARGET took bit
    // 31 — the first free bit past the US-14 TASK_MOVE's 30).
    assert_eq!(
        full.0.count_ones(),
        32,
        "every one-shot field owns a distinct bit"
    );
    // The conversion confirm is the fix's flagship pair: exactly its
    // two fields, nothing more (the stage-3 over-broad trap check).
    let convert = route_consumes(ActionId::HabitConvertToSeries);
    assert_eq!(
        convert,
        OneShotSet::default().habit_convert_target().pattern_draft()
    );
    // The arm intents consume nothing (the effect arm reads adapter
    // facts, never ctx one-shots).
    assert_eq!(
        route_consumes(ActionId::HabitConvertArm),
        OneShotSet::default()
    );
    assert_eq!(
        route_consumes(ActionId::SeriesConvertArm),
        OneShotSet::default()
    );
}
