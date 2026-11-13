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

/// The vi mode (D17): two plain shell states — ordinary state, not a
/// subsystem (D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
}

/// UI-internal effects (design §5: "ui-internal actions → focus
/// moves, view switches, mode flips"). The adapter applies them to
/// the shell/window; `Exit` maps to the slint event-loop quit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    Exit,
    OpenMenu,
    /// Open the jump-to search palette (#348, chunk 9). The adapter's
    /// interpretation table opens the flow; any other flow is inert
    /// (the modal discipline).
    OpenPalette,
    CloseMenu,
    /// Level-aware menu back (round-1 D6): a submenu steps back one
    /// level, the root closes.
    MenuBack,
    MenuNext,
    MenuPrevious,
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
    /// The adapter closes the menu and re-dispatches the selected
    /// entry — bounded (the menu is closed first, no recursion).
    InvokeSelected,
    /// Open the recur row's inline pause editor (issue #506): a view
    /// intent over the subject task — the adapter flips the model's
    /// `recur-pause-open` flag; any other flow/state is inert.
    SeriesPauseOpen,
    /// Arm the habit row's conversion confirm strip (issue #506): the
    /// adapter records the armed row; the dispatch itself waits for
    /// Confirm.
    HabitConvertArm,
    /// The create form's open/toggle (round-7 D-7.2): the adapter
    /// flips the armed flag — open when closed, discard-and-close
    /// when armed — only with an active board; kiosk stays inert
    /// (the action gate, D-T16).
    HabitCreateToggle,
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
    // --- chunk-7 board dimension (design §3.8): pure view state; the
    // adapter resolves them against the view-state model. Never a
    // `DispatchOutcome::Command`.
    /// Step to the next live board (canonical order, no wrap).
    BoardNext,
    /// Step to the previous live board.
    BoardPrevious,
    /// Select the parameterized board (`DispatchCtx::board_target`).
    BoardSelect(BoardId),
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
    /// Focus the first card in reading order.
    FocusFirst,
    /// Focus the last card in reading order.
    FocusLast,
    // --- round-1 chunk 3 (D28b): the settings surface — a `ViewId`
    // excluded from the view cycle; the adapter toggles it.
    /// Open the settings view (or close it — toggle semantics).
    OpenSettings,
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
    Back,
    Advance,
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
            | Self::FocusNext
            | Self::FocusPrev
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
            | Self::HabitConvertArm
            | Self::HabitCreateToggle
            | Self::SeriesConvertArm
            | Self::SeriesEditorOpen
            | Self::SeriesDetachArm
            | Self::SeriesDetachCancel => EffectClass::Nav,
            Self::StackDelete
            | Self::StackRename
            | Self::StackMove
            | Self::BoardCreate
            | Self::BoardDelete
            | Self::BoardRename => EffectClass::Lifecycle,
            Self::BeginCreate { .. } | Self::BeginCreateStack | Self::RequestMove => {
                EffectClass::Begin
            }
            Self::OpenPalette => EffectClass::OpenPalette,
            Self::ToggleFocusMode => EffectClass::FocusOverlay,
            Self::TrackEntriesOpen | Self::TrackEntryDismiss => EffectClass::TrackOverlay,
            Self::OpenPlanNextDay
            | Self::PlanBeginPlace(_)
            | Self::PlanCommitPlace
            | Self::PlanBoxEarlier(_)
            | Self::PlanBoxLater(_)
            | Self::PlanRemoveBox(_) => EffectClass::PlanSurface,
            Self::OpenReviewWalk | Self::ReviewWalk(_) => EffectClass::ReviewWalk,
            Self::BeginCapture => EffectClass::BeginCapture,
            Self::Activate | Self::SetMode(_) => EffectClass::Edit,
            Self::BeginTitleEdit => EffectClass::Title,
            Self::PickerKey(_) => EffectClass::Picker,
            Self::CancelMove => EffectClass::Cancel,
            Self::OpenMenu
            | Self::CloseMenu
            | Self::MenuBack
            | Self::MenuNext
            | Self::MenuPrevious
            | Self::InvokeSelected => EffectClass::Menu,
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

/// The focused-entity seam (D16): chunk 2's shell has no views, so
/// real sessions carry `None`s (keyboard task verbs resolve to
/// [`DispatchOutcome::NoTarget`]); chunk 3's focus architecture sets
/// them for real.
#[derive(Debug, Clone, Default, PartialEq)]
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
    /// derived, never consumed; `task.due-today` re-commits to its end
    /// (`NoTarget` without focus or window).
    pub day_window: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
    /// The timers form's parsed draft (DR-3/3, D-P10): set by the
    /// adapter when the form commits; `timer.create` routes from it
    /// (`NoTarget` without one).
    pub timer_draft: Option<TimerDraft>,
    /// The timers row's alarm (DR-3/3, D-I8): set by the adapter when
    /// a cancel affordance activates (section row or awaiting row);
    /// `timer.cancel` routes from it (`NoTarget` without one).
    pub timer_target: Option<AdhocAlarmId>,
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
    /// The reorder's resolved move (DR-6/6): set by the adapter when a
    /// reorder affordance activates — the habit plus the drop slot's
    /// floor-midpoint order computed from the row source; `habit.move`
    /// routes from it (`NoTarget` without one).
    pub habit_move: Option<(taskboard_domain::HabitId, i64)>,
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
    /// The create form's one-shot payload (round-7 D-7.2): the exact
    /// `CreateHabit` triple the confirm seam minted (board resolved
    /// fresh at commit, the minted quota target) — `habit.create-commit`
    /// consumes it, then it is gone (the one-shot take law).
    pub habit_create_draft: Option<HabitCreateDraft>,
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
            // The habit verbs (DR-6/6, D-P13): +1 from the row target;
            // the reorder from the adapter-resolved (habit, order) —
            // the UI computes midpoints, the planner stamps the value.
            ActionId::HabitLog => match ctx.habit_target {
                Some(habit) => DispatchOutcome::Command(StateCommand::LogHabit { habit }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::HabitMove => match ctx.habit_move {
                Some((habit, order)) => {
                    DispatchOutcome::Command(StateCommand::MoveHabit { habit, order })
                }
                None => DispatchOutcome::NoTarget,
            },
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
            ActionId::TaskCreate => DispatchOutcome::UiEffect(UiEffect::BeginCreate {
                target: ctx.create_target,
            }),
            ActionId::StackCreate => DispatchOutcome::UiEffect(UiEffect::BeginCreateStack),
            ActionId::TaskEdit => DispatchOutcome::UiEffect(UiEffect::Activate),
            ActionId::TaskTitleEdit => DispatchOutcome::UiEffect(UiEffect::BeginTitleEdit),
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
            ActionId::NavFirstItem => DispatchOutcome::UiEffect(UiEffect::FocusFirst),
            ActionId::NavLastItem => DispatchOutcome::UiEffect(UiEffect::FocusLast),
            // The today re-commit (chunk 10, D-V4 rider): a fresh
            // `Some(Some(window_end))` write, confirmation-free;
            // `NoTarget` without focus *or* window (the design's exact
            // wording).
            ActionId::TaskDueToday => match (ctx.focused_task, ctx.day_window) {
                (Some(id), Some((_, end))) => DispatchOutcome::Command(StateCommand::UpdateTask {
                    id,
                    changes: taskboard_domain::TaskChanges {
                        duedate: Some(Some(end)),
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
            ActionId::BoardSelect => match ctx.board_target {
                Some(board) => DispatchOutcome::UiEffect(UiEffect::BoardSelect(board)),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::StackWider => DispatchOutcome::UiEffect(UiEffect::StackWider),
            ActionId::StackNarrower => DispatchOutcome::UiEffect(UiEffect::StackNarrower),
            // Round-1 chunk 3 (D28b): the settings opener is live.
            ActionId::AppSettings => DispatchOutcome::UiEffect(UiEffect::OpenSettings),
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
            // Reserved (§5.8) — PaletteSearch opens the palette; the
            // only disabled row left (D6) is AppHelp (the undo rows
            // dispatch at the top of the match).
            ActionId::PaletteSearch => DispatchOutcome::UiEffect(UiEffect::OpenPalette),
            ActionId::AppHelp => DispatchOutcome::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::{HabitId, TaskId};
    use uuid::Uuid;

    fn ctx(task: bool) -> DispatchCtx {
        DispatchCtx {
            focused_task: task.then(|| TaskId::from_uuid(Uuid::now_v7())),
            focused_stack: None,
            create_target: None,
            reminder_target: None,
            board_target: None,
            rule_target: None,
            rule_add_when: None,
            timer_draft: None,
            track_entry_draft: None,
            track_entry_target: None,
            timer_target: None,
            track_target: None,
            habit_target: None,
            habit_move: None,
            habit_repair: None,
            habit_convert_target: None,
            habit_create_draft: None,
            pattern_draft: None,
            plan_target: None,
            series_target: None,
            pause_draft: None,
            convert_target: None,
            recur_edit_target: None,
            recur_draft_rule: None,
            day_window: None,
            focused_task_done: false,
            focused_task_tracker: false,
        }
    }

    fn banner_target() -> crate::presentation::ReminderTarget {
        crate::presentation::ReminderTarget {
            anchor: taskboard_domain::ReminderAnchor::Task(TaskId::from_uuid(Uuid::now_v7())),
            reminder: taskboard_domain::ReminderRef::Default(TaskId::from_uuid(Uuid::now_v7())),
            occurrence: chrono::Utc::now(),
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
        let mut move_ctx = ctx(true);
        move_ctx.habit_move = Some((habit, 15));
        assert_eq!(
            ActionRouter::dispatch(ActionId::HabitMove, &move_ctx),
            DispatchOutcome::Command(StateCommand::MoveHabit { habit, order: 15 })
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
            (ActionId::BoardRename, UiEffect::BoardRename),
            (ActionId::NavFirstItem, UiEffect::FocusFirst),
            (ActionId::NavLastItem, UiEffect::FocusLast),
        ] {
            assert_eq!(
                ActionRouter::dispatch(id, &ctx(true)),
                DispatchOutcome::UiEffect(effect)
            );
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
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardSelect, &ctx),
            DispatchOutcome::NoTarget
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
