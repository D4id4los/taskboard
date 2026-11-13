// SPDX-License-Identifier: AGPL-3.0-only

//! The action registry (GUI Phase 7 chunk 2, design §5.2/§5.8): the
//! stable, namespaced vocabulary every keybinding, menu, and hint is
//! expressed in.
//!
//! Views bind to *actions*, never keys; the registry is the single
//! const table describing every v0.1.0 action — id, label,
//! menu-availability contexts, category, and the `enabled` bit. A
//! dumped registry (the insta snapshot in `tests/keymap_snapshots.rs`)
//! **is** the action documentation (§5.2).
//!
//! This module is pure: no slint types, no IO, no runtime (the §5.5
//! extraction seam — the module must stay liftable into its own crate).

use std::fmt;

/// A keymap resolution context (design §5.2): resolution walks an
/// ordered chain of these, first match wins; a context may mask the
/// contexts below it (declared in keymap data, never hardcoded).
///
/// `Global` terminates every chain. `Menu` is the modal menu context.
/// `ViNormal`/`ViInsert` are the vi-mode binding contexts — ordinary
/// contexts, not a subsystem (design §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextId {
    Global,
    /// The Today view (#346): subjectless, capture-is-the-create-path.
    Today,
    Board,
    StackDetail,
    TaskDetail,
    /// The review surface's context (#214): read-only — no bindings
    /// target it yet; the variant exists so the keymap context
    /// resolution stays total over the view set (the plan's D-8).
    Review,
    Settings,
    /// The plan-next-day surface (#622): a non-masking view context
    /// like `Today` — the planner's walking verbs live here; the
    /// placement form's text fields ride the existing `ViInsert`
    /// masking (no new mask context).
    PlanNextDay,
    Menu,
    /// The jump-to search palette (#348): a total mask like `Menu`.
    Palette,
    /// The concentration overlay (#496): a total mask like `Palette`
    /// (the modal context while the overlay holds).
    FocusMode,
    ViNormal,
    ViInsert,
    /// The date-picker's grid (round-3 D25): active exactly while the
    /// grid `FocusScope` holds slint focus; non-masking — Escape still
    /// resolves `ModeNormal` from `ViInsert` (the editor's clean
    /// close).
    DatePicker,
    /// The time-entries overlay (r4 chunk 10, D-TS2): a total mask
    /// like `Palette`/`FocusMode` — the modal context while the
    /// overlay holds; the form's fields ride `ViInsert` masking and
    /// the overlay's own rows (dismiss + the walk) live here.
    TrackEntries,
    /// The review walk overlay (DR-7 chunk 3, #638): a total mask like
    /// `TrackEntries` — the modal context while `Flow::ReviewWalk`
    /// holds; the prompt fields ride `ViInsert` masking, the walk's
    /// own verbs (cursor, row verbs, commit, back) live here.
    ReviewWalk,
    /// The multi-line editor's element scope (round-5 chunk 7, D-δ2):
    /// armed exactly while `EditingFacts.editor-focus-count > 0`.
    /// Non-masking and keymap-unbound in every preset — the scope's
    /// verbs are the editor command tables (consulted ahead of the
    /// resolver in `intercept`), never keymap rows; the context rides
    /// the stack head so the resolver's chain shape stays total.
    Editor,
}

impl ContextId {
    /// The stable config/snapshot name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Today => "today",
            Self::Board => "board",
            Self::StackDetail => "stack-detail",
            Self::TaskDetail => "task-detail",
            Self::Review => "review",
            Self::Settings => "settings",
            Self::PlanNextDay => "plan-next-day",
            Self::Menu => "menu",
            Self::Palette => "palette",
            Self::FocusMode => "focus-mode",
            Self::ViNormal => "vi-normal",
            Self::ViInsert => "vi-insert",
            Self::DatePicker => "date-picker",
            Self::TrackEntries => "track-entries",
            Self::ReviewWalk => "review-walk",
            Self::Editor => "editor",
        }
    }
}

impl fmt::Display for ContextId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The menu grouping of actions (the R8 seam: menus are generated from
/// the registry, ordered by category — design §5.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Application,
    Capture,
    Search,
    Navigation,
    Task,
    Menu,
    Mode,
    /// The picker keys (round-3 D25): never rendered as a menu
    /// section — picker rows are never chain-visible outside the
    /// `DatePicker` context, so they never render in menus (the
    /// `menu.*` masking argument).
    Picker,
}

impl Category {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Capture => "capture",
            Self::Search => "search",
            Self::Navigation => "navigation",
            Self::Task => "task",
            Self::Menu => "menu",
            Self::Mode => "mode",
            Self::Picker => "picker",
        }
    }

    /// The menu's section display label (usability pass D5): the
    /// declaration order remains the documented menu order; sections
    /// are derived from this mapping, never hand-written rows.
    #[must_use]
    pub const fn section_label(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::Capture => "Capture",
            Self::Search => "Search",
            Self::Navigation => "Navigation",
            Self::Task => "Task",
            Self::Menu => "Menu",
            Self::Mode => "Mode",
            Self::Picker => "Picker",
        }
    }
}

/// The two-tier menu's submenu vocabulary (GUI review round 1, D6/D10;
/// stage-1 finalization of "grouping stage 1 finalizes"). Declaration
/// order is the parent rows' order on the root page — presentation
/// only: keymap resolution never reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MenuGroup {
    Task,
    Stack,
    Board,
    ViewNav,
    Mode,
    Help,
}

impl MenuGroup {
    /// The parent row's label on the root page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Task => "Task actions",
            Self::Stack => "Stack",
            Self::Board => "Board",
            Self::ViewNav => "View & navigation",
            Self::Mode => "Mode",
            Self::Help => "Help",
        }
    }

    /// Every group in declaration order (the root page's parent-row
    /// order).
    #[must_use]
    pub const fn all() -> &'static [MenuGroup] {
        &[
            MenuGroup::Task,
            MenuGroup::Stack,
            MenuGroup::Board,
            MenuGroup::ViewNav,
            MenuGroup::Mode,
            MenuGroup::Help,
        ]
    }
}

/// A dispatchable action: the dotted `scope.verb` vocabulary (design
/// §5.2) as a closed field-less enum. The string form lives in the
/// registry table — string ↔ enum conversion goes through
/// [`REGISTRY`], so an unknown string is typed, and the router's
/// exhaustive match makes adding an action touch every dispatch site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionId {
    AppExit,
    AppMenu,
    AppUndo,
    AppRedo,
    AppSettings,
    AppHelp,
    CaptureQuick,
    ViewToday,
    ViewInbox,
    PaletteSearch,
    NavNextView,
    NavPrevView,
    NavFocusBoard,
    NavShowTask,
    NavNextControl,
    NavPrevControl,
    ViewScrollUp,
    ViewScrollDown,
    NavNextItem,
    NavPrevItem,
    BoardNext,
    BoardPrevious,
    BoardSelect,
    StackWider,
    StackNarrower,
    TaskCreate,
    StackCreate,
    TaskEdit,
    TaskTitleEdit,
    TaskComplete,
    TaskReopen,
    TaskDueToday,
    TaskMove,
    TaskMoveCancel,
    TaskDelete,
    StackNext,
    StackPrev,
    StackDelete,
    StackRename,
    StackMove,
    BoardCreate,
    BoardDelete,
    BoardRename,
    NavFirstItem,
    NavLastItem,
    ReminderSnooze,
    ReminderMute,
    ReminderUnmute,
    ReminderRuleAdd,
    ReminderRuleRemove,
    TimerCreate,
    TimerCancel,
    TaskTrackStart,
    TaskTrackStop,
    TaskToggleTracker,
    /// The entries-overlay open verb (r4 chunk 10, D-TS6): opens
    /// `Flow::TrackEntries` over the subject task; refuses on kiosk
    /// (the view intent's own gate).
    TaskTrackEntries,
    /// The entries form's add commit (r4 D-TS6): routes the parsed
    /// draft into `LogTime` (`NoTarget` without a valid draft + task).
    TaskTrackEntryLog,
    /// The entries form's edit commit: routes the parsed mask into
    /// `AdjustTimeEntry` (`NoTarget` without a valid draft + target).
    TaskTrackEntryAdjust,
    /// The armed delete's confirm: routes `DeleteTimeEntry` from the
    /// row's one-shot target (`NoTarget` without one).
    TaskTrackEntryDelete,
    /// The overlay's Escape ladder (D-TS2): a view intent resolved
    /// against the current flow (form → list; list → close; a
    /// focused field's Insert disengages first — the adapter reads
    /// the mode).
    TaskTrackEntryDismiss,
    /// The concentration overlay's toggle (issue #496): opens or
    /// closes `Flow::FocusMode`. Session verbs are NOT re-invented —
    /// start/stop stay `task.track-start`/`task.track-stop` — this
    /// action only moves presentation.
    FocusModeToggle,
    // The habit verbs (DR-6/6, D-P13): section-scoped like the
    // timers' seams — pointer-driven, no preset bindings; dispatch-time
    // gating only (the kiosk gate empties the affordances, the
    // planner's idempotent NoOps own the refusals). The deliberate
    // exception is `HabitCreate` (round-7 D-7.2): the accessibility
    // ledger demands keyboard parity for creation, so it ships bound
    // in every preset.
    HabitLog,
    HabitMove,
    /// The create form's opener (round-7 D-7.2): toggles the armed
    /// inline form under the Today section's header — open when
    /// closed, discard-and-close when armed. Opening requires an
    /// active board (the section's board IS the create board).
    HabitCreate,
    /// The create form's confirm dispatch (round-7 D-7.2): routes
    /// `CreateHabit` from the one-shot draft the confirm seam minted
    /// (`NoTarget` without one). Internal verb — the form's Confirm
    /// is its only affordance.
    HabitCreateCommit,
    /// The streak-repair affordance (issue #506, plan D-7): the
    /// make-up +1 into the previous window — routes `AdjustHabitLog`
    /// from the adapter-resolved `(habit, window, count)` (what you
    /// click is what increments).
    HabitRepair,
    /// The habit → series conversion's open-confirm step: arms the
    /// row's confirm strip (consequence phrase first, dispatch only
    /// on Confirm).
    HabitConvertArm,
    /// The habit → series confirm's dispatch: routes
    /// `ConvertHabitToSeries` with the quota's default pattern (D-2).
    HabitConvertToSeries,
    /// The task-view recur row's pause affordance: opens the inline
    /// pause editor (a view intent, the entries-overlay precedent).
    SeriesPause,
    /// The pause editor's commit: routes `PauseSeries` with the parsed
    /// inclusive date (`NoTarget` without target or valid draft).
    SeriesPauseCommit,
    /// The resume affordance: routes `PauseSeries { until: None }` —
    /// the field clears for badge hygiene (the expired pause is inert).
    SeriesResume,
    /// The series → habit conversion's open-confirm step: arms the
    /// recur row's confirm strip.
    SeriesConvertArm,
    /// The series → habit confirm's dispatch: routes
    /// `ConvertSeriesToHabit` with the pattern's default quota (D-2).
    SeriesConvertToHabit,
    /// The plan-next-day surface's opener (#622, D-4): the routed
    /// reachability seam — the same dispatchable intent the review
    /// walk's final leg takes. Toggle semantics (the settings
    /// opener's twin); a held flow keeps it inert (the modal
    /// discipline).
    ViewPlanNextDay,
    // The planner's pointer-grade row verbs (#622, D-6): registry
    // rows with no preset bindings, dispatch-time gating only (the
    // habits verbs' rule; the kiosk gate empties the affordances).
    /// Arm the placement form over the addressed candidate row.
    PlanBeginPlace,
    /// Commit the placement form: routes the parsed `UpdateTask`
    /// mask (both slots atomic; one undo step).
    PlanCommitPlace,
    /// Swap the addressed placed box's scheduled instant with the
    /// earlier temporal neighbor's (two `UpdateTask` writes).
    PlanBoxEarlier,
    /// Swap the addressed placed box with the later neighbor's.
    PlanBoxLater,
    /// Unplace the addressed box (`UpdateTask { scheduled:
    /// Some(None) }`); the task returns to the pool.
    PlanRemoveBox,
    // The review walk's verbs (DR-7 chunk 3, #638): the opener is the
    // global routed reachability seam (no preset binding — P7:
    // reachable, not promoted); the in-walk verbs are section-scoped
    // to `ContextId::ReviewWalk` (the #622 planner-verb rule) — the
    // row verbs are reached by focus + Enter on the mounted buttons
    // and the pointer; the cursor/back rows carry the preset bindings.
    /// The walk's opener: routes `OpenReviewWalk`; the adapter derives
    /// the reference (the due window's day when one is due, else the
    /// current day window) and gates on kiosk.
    ReviewOpen,
    /// Step the walk's cursor up a row.
    ReviewWalkCursorUp,
    /// Step the walk's cursor down a row.
    ReviewWalkCursorDown,
    /// The walk's Escape ladder: sub-state → stage-back → close.
    ReviewWalkBack,
    /// Advance the stage (Inbox → Carry forward; out of Carry forward
    /// this is the planner handoff — the adapter's shape-B arm).
    ReviewWalkAdvance,
    /// Arm the File-to picker on the cursor inbox row.
    ReviewInboxFile,
    /// Keep the cursor inbox row in the inbox (no command) and step.
    ReviewInboxKeep,
    /// Move the armed File-to pick to the previous candidate stack.
    ReviewFileTargetPrev,
    /// Move the armed File-to pick to the next candidate stack.
    ReviewFileTargetNext,
    /// Commit the armed File-to pick: routes `MoveTask` (end-of-stack).
    ReviewFileCommit,
    /// Carry the cursor row to the next window's opening instant
    /// (`UpdateTask { scheduled }`; due dates are never touched).
    ReviewCarryTomorrow,
    /// Arm the Pick-a-date field on the cursor carry row.
    ReviewCarryPickDate,
    /// Leave the cursor carry row as is (no command) and step.
    ReviewCarryLeave,
    /// Commit the date field's text (parsed by the due-edit parser;
    /// unparseable input dispatches nothing and keeps the text).
    ReviewCarryDateCommit,
    /// Commit the prompts as one `RecordReflection`; an all-blank
    /// record dispatches nothing (the gentle end) — both exits close
    /// the walk.
    ReviewRecord,
    /// End the walk without recording (the equal-prominence exit).
    ReviewFinish,
    MenuNext,
    MenuPrevious,
    MenuInvoke,
    MenuDismiss,
    ModeInsert,
    ModeNormal,
    PickerLeft,
    PickerRight,
    PickerUp,
    PickerDown,
    PickerPrevMonth,
    PickerNextMonth,
    PickerMonthStart,
    PickerMonthEnd,
    PickerCommit,
}

impl ActionId {
    /// The stable dotted id (config override keys, menu models).
    #[must_use]
    #[allow(clippy::too_many_lines)] // the registry's one id table
    pub const fn id(self) -> &'static str {
        match self {
            Self::AppExit => "app.exit",
            Self::AppMenu => "app.menu",
            Self::AppUndo => "app.undo",
            Self::AppRedo => "app.redo",
            Self::AppSettings => "app.settings",
            Self::AppHelp => "app.help",
            Self::CaptureQuick => "capture.quick",
            Self::ViewToday => "view.today",
            Self::ViewInbox => "view.inbox",
            Self::PaletteSearch => "palette.search",
            Self::NavNextView => "nav.next-view",
            Self::NavPrevView => "nav.prev-view",
            Self::NavFocusBoard => "nav.focus-board",
            Self::NavShowTask => "nav.show-task",
            Self::NavNextControl => "nav.next-control",
            Self::NavPrevControl => "nav.prev-control",
            Self::ViewScrollUp => "view.scroll-up",
            Self::ViewScrollDown => "view.scroll-down",
            Self::NavNextItem => "nav.next-item",
            Self::NavPrevItem => "nav.prev-item",
            Self::BoardNext => "board.next",
            Self::BoardPrevious => "board.previous",
            Self::BoardSelect => "board.select",
            Self::StackWider => "stack.wider",
            Self::StackNarrower => "stack.narrower",
            Self::TaskCreate => "task.create",
            Self::StackCreate => "stack.create",
            Self::TaskEdit => "task.edit",
            Self::TaskTitleEdit => "task.title-edit",
            Self::TaskComplete => "task.complete",
            Self::TaskReopen => "task.reopen",
            Self::TaskDueToday => "task.due-today",
            Self::TaskMove => "task.move",
            Self::TaskMoveCancel => "task.move.cancel",
            Self::TaskDelete => "task.delete",
            Self::StackNext => "stack.next",
            Self::StackPrev => "stack.prev",
            Self::StackDelete => "stack.delete",
            Self::StackRename => "stack.rename",
            Self::StackMove => "stack.move",
            Self::BoardCreate => "board.create",
            Self::BoardDelete => "board.delete",
            Self::BoardRename => "board.rename",
            Self::NavFirstItem => "nav.first-item",
            Self::NavLastItem => "nav.last-item",
            Self::ReminderSnooze => "reminder.snooze",
            Self::ReminderMute => "reminder.mute",
            Self::ReminderUnmute => "reminder.unmute",
            Self::ReminderRuleAdd => "reminder.rule-add",
            Self::ReminderRuleRemove => "reminder.rule-remove",
            Self::TimerCreate => "timer.create",
            Self::TimerCancel => "timer.cancel",
            Self::TaskTrackStart => "task.track-start",
            Self::TaskTrackStop => "task.track-stop",
            Self::TaskToggleTracker => "task.toggle-tracker",
            Self::TaskTrackEntries => "task.track-entries",
            Self::TaskTrackEntryLog => "task.track-entry-log",
            Self::TaskTrackEntryAdjust => "task.track-entry-adjust",
            Self::TaskTrackEntryDelete => "task.track-entry-delete",
            Self::TaskTrackEntryDismiss => "task.track-entry-dismiss",
            Self::FocusModeToggle => "focus.toggle",
            Self::HabitLog => "habit.log",
            Self::HabitMove => "habit.move",
            Self::HabitCreate => "habit.create",
            Self::HabitCreateCommit => "habit.create-commit",
            Self::HabitRepair => "habit.repair",
            Self::HabitConvertArm => "habit.convert-arm",
            Self::HabitConvertToSeries => "habit.convert-to-series",
            Self::SeriesPause => "series.pause",
            Self::SeriesPauseCommit => "series.pause-commit",
            Self::SeriesResume => "series.resume",
            Self::SeriesConvertArm => "series.convert-arm",
            Self::SeriesConvertToHabit => "series.convert-to-habit",
            Self::ViewPlanNextDay => "view.plan-next-day",
            Self::PlanBeginPlace => "plan.begin-place",
            Self::PlanCommitPlace => "plan.commit-place",
            Self::PlanBoxEarlier => "plan.box-earlier",
            Self::PlanBoxLater => "plan.box-later",
            Self::PlanRemoveBox => "plan.remove-box",
            Self::ReviewOpen => "review.open",
            Self::ReviewWalkCursorUp => "review.walk-cursor-up",
            Self::ReviewWalkCursorDown => "review.walk-cursor-down",
            Self::ReviewWalkBack => "review.walk-back",
            Self::ReviewWalkAdvance => "review.walk-advance",
            Self::ReviewInboxFile => "review.inbox-file",
            Self::ReviewInboxKeep => "review.inbox-keep",
            Self::ReviewFileTargetPrev => "review.file-target-prev",
            Self::ReviewFileTargetNext => "review.file-target-next",
            Self::ReviewFileCommit => "review.file-commit",
            Self::ReviewCarryTomorrow => "review.carry-tomorrow",
            Self::ReviewCarryPickDate => "review.carry-pick-date",
            Self::ReviewCarryLeave => "review.carry-leave",
            Self::ReviewCarryDateCommit => "review.carry-date-commit",
            Self::ReviewRecord => "review.record",
            Self::ReviewFinish => "review.finish",
            Self::MenuNext => "menu.next",
            Self::MenuPrevious => "menu.previous",
            Self::MenuInvoke => "menu.invoke",
            Self::MenuDismiss => "menu.dismiss",
            Self::ModeInsert => "mode.insert",
            Self::ModeNormal => "mode.normal",
            Self::PickerLeft => "picker.left",
            Self::PickerRight => "picker.right",
            Self::PickerUp => "picker.up",
            Self::PickerDown => "picker.down",
            Self::PickerPrevMonth => "picker.prev-month",
            Self::PickerNextMonth => "picker.next-month",
            Self::PickerMonthStart => "picker.month-start",
            Self::PickerMonthEnd => "picker.month-end",
            Self::PickerCommit => "picker.commit",
        }
    }

    /// Resolves a dotted id through the registry; `None` for anything
    /// not in the v0.1.0 table.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        REGISTRY
            .iter()
            .find(|entry| entry.id.id() == id)
            .map(|entry| entry.id)
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// One registry row (design §5.2: `{ id, label, contexts, category }`
/// plus the honest `enabled` bit of §5.8).
#[derive(Debug, Clone, Copy)]
pub struct ActionEntry {
    pub id: ActionId,
    pub label: &'static str,
    /// Menu-availability contexts (§5.2): `Global`-tagged actions are
    /// offered in every context's menu; the view contexts are chunk
    /// 3's. Binding contexts are a *preset-table* concern, not a
    /// registry one.
    pub contexts: &'static [ContextId],
    pub category: Category,
    /// Whether dispatching performs the action's effect in this
    /// build. Reserved actions (§5.8) and chunk-3 flows (§12 row 2)
    /// are `false`: they dispatch a disabled no-op and read as
    /// disabled in the dumped registry.
    pub enabled: bool,
    /// Presentation-only display priority (round-1 D5): lower =
    /// earlier on a page. Spaces of 10 leave room for insertions.
    /// Keymap resolution never reads it.
    pub menu_rank: u16,
    /// One or two sentences: what the function does and its effect;
    /// disabled rows state that they are not yet available (round-1
    /// D10). Rendered as the row tooltip (D8) and the a11y
    /// description. Empty is allowed by the type; the registry
    /// review requires every row to carry one.
    pub description: &'static str,
    /// The submenu that owns this row in the two-tier menu (D6/D10);
    /// `None` = top-level (quick rows, Quit, the menu cursor verbs).
    /// Presentation-only.
    pub menu_group: Option<MenuGroup>,
}

const GLOBAL: &[ContextId] = &[ContextId::Global];
const VIEW_NAV: &[ContextId] = &[
    ContextId::Today,
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
];
/// The control-navigation walk's binding/availability contexts
/// (round-3 D25): the five view contexts, never `Global` — Global is
/// chain-visible under insert mode, and Tab/PageUp/PageDown are
/// native text-field motions there.
pub const CONTROL_NAV: &[ContextId] = &[
    ContextId::Today,
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
    ContextId::Settings,
];

/// The board verbs' menu availability (design §3.8): global-capable
/// like the `nav.*` family — offered in every context's menu.
const GLOBAL_VIEW_NAV: &[ContextId] = &[
    ContextId::Global,
    ContextId::Today,
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
];

/// The v0.1.0 action registry (design §5.2 table, decisions D2/D4/D5):
/// const data, ordered for the documentation dump. `app.menu`,
/// `app.exit`, `task.complete`, `task.delete`, the `menu.*` cursor
/// actions, and the `mode.*` flips dispatch for real in this chunk;
/// everything else is reserved or chunk-3 staged and reads as
/// disabled. The round-1 curation (D5/D10) adds the presentation-only
/// `menu_rank` / `description` / `menu_group` columns: the two-tier
/// menu is generated from them, never hand-written.
pub const REGISTRY: &[ActionEntry] = &[
    ActionEntry {
        id: ActionId::AppExit,
        label: "Quit taskboard",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        // The Quit-pinned-last guarantee is the projection rule, not
        // this number (a Tier-1 test pins the position).
        menu_rank: 1000,
        description: "Quits the application; local state is already saved.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppMenu,
        label: "Command menu",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        menu_rank: 60,
        description: "Opens or closes the command menu, the keyboard's way into every action.",
        menu_group: Some(MenuGroup::Help),
    },
    ActionEntry {
        id: ActionId::AppUndo,
        label: "Undo",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        menu_rank: 30,
        description: "Undoes the last change. A change the server already received is undone by a new compensating change; the line head that cannot be undone is refused and named.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppRedo,
        label: "Redo",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        menu_rank: 40,
        description: "Re-applies the last undone change as a new edit. Content the server deleted is re-created as a new remote card.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppSettings,
        label: "Open settings",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        menu_rank: 10,
        description: "Opens or closes the settings view: sync connections, keymap preset, UI scale.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppHelp,
        label: "Open help",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
        menu_rank: 70,
        description: "Shows the keymap and usage help. Not yet available — reserved.",
        menu_group: Some(MenuGroup::Help),
    },
    ActionEntry {
        id: ActionId::CaptureQuick,
        label: "Quick capture",
        contexts: GLOBAL,
        category: Category::Capture,
        enabled: true,
        menu_rank: 20,
        description: "Captures a thought into the global inbox from anywhere; Enter commits and continues, Escape closes.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ViewToday,
        label: "Go to today",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        // Round-3 D27a re-rank: the go-to verbs above the cycle.
        menu_rank: 60,
        description: "Shows the today view: overdue, scheduled, due-today, and done-today rows across all boards.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::ViewInbox,
        label: "Go to inbox",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 70,
        description: "Jumps to the global Inbox board's first stack; quietly no-ops when the inbox is empty of boards.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::PaletteSearch,
        label: "Search",
        contexts: GLOBAL,
        category: Category::Search,
        enabled: true,
        menu_rank: 50,
        description: "Opens the jump-to search palette over all boards.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavNextView,
        label: "Next view",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 100,
        description: "Moves to the next view in the board → stack → task cycle.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavPrevView,
        label: "Previous view",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 110,
        description: "Moves to the previous view in the board → stack → task cycle.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFocusBoard,
        label: "Go to board",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 80,
        description: "Returns to the board view from any deeper view.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavShowTask,
        label: "Show task",
        // Palette-only availability: the row never renders in a menu
        // (the `menu.*` masking precedent) while staying first class
        // for config overrides, hints, and this snapshot. No preset
        // binding by recorded decision (round-3 D23): the trigger IS
        // the palette's Return (`menu.invoke`); a second chord would
        // be a synonym, and the no-second-class gate reads uniformly
        // unbound in all three presets.
        contexts: &[ContextId::Palette],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 90,
        description: "Shows the task picked in the search palette: selects its board and stack, focuses its card, and scrolls it into view.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavNextControl,
        label: "Next control",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 160,
        description: "Moves the keyboard focus to the next control (field or button) in reading order; wraps like the runtime's tab traversal.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavPrevControl,
        label: "Previous control",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 170,
        description: "Moves the keyboard focus to the previous control (field or button) in reading order; wraps like the runtime's tab traversal.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::ViewScrollDown,
        label: "Scroll down",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 180,
        description: "Pages the focused scroll area down one page (a column, a section, or the settings pane); inert without a focused container.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::ViewScrollUp,
        label: "Scroll up",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 190,
        description: "Pages the focused scroll area up one page (a column, a section, or the settings pane); inert without a focused container.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavNextItem,
        label: "Focus next item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 120,
        description: "Moves the focus to the next card or switcher entry in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFirstItem,
        label: "Focus first item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 140,
        description: "Moves the focus to the first card in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavLastItem,
        label: "Focus last item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 150,
        description: "Moves the focus to the last card in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavPrevItem,
        label: "Focus previous item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 130,
        description: "Moves the focus to the previous card or switcher entry in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::BoardNext,
        label: "Next board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 60,
        description: "Switches to the next board in canonical order.",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardPrevious,
        label: "Previous board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 70,
        description: "Switches to the previous board in canonical order.",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardSelect,
        label: "Select board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 80,
        description: "Switches to the board picked from the board bar (click a board segment).",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardCreate,
        label: "New board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 90,
        description: "Opens the new-board field and creates the board on commit.",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardRename,
        label: "Rename board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 100,
        description: "Opens the board title editor for the current board.",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardDelete,
        label: "Delete board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 110,
        description: "Deletes the current board and all its content (no confirmation step); typed refusals are the engine's.",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::StackNext,
        label: "Next stack",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 60,
        description: "Moves the stack selection to the next stack (wraps) and focuses its first task.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::StackPrev,
        label: "Previous stack",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 70,
        description: "Moves the stack selection to the previous stack (wraps) and focuses its first task.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::StackWider,
        label: "Widen stack",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 110,
        description: "Widens the focused stack's column; the width is saved per device.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::StackNarrower,
        label: "Narrow stack",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 120,
        description: "Narrows the focused stack's column; the width is saved per device.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::StackRename,
        label: "Rename stack",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 90,
        description: "Opens the stack title editor for the focused or selected stack.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::StackMove,
        label: "Move stack",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 100,
        description: "Starts a move flow for the focused or selected stack: pick an insertion slot, then confirm to reorder.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::StackDelete,
        label: "Delete stack",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 130,
        description: "Deletes the focused or selected stack and everything in it; the engine refuses typed when the target is gone.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::TaskCreate,
        label: "New task",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 60,
        description: "Opens the inline create row and puts it into insert mode.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::StackCreate,
        label: "New stack",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 80,
        description: "Opens the new-stack field on the current board.",
        menu_group: Some(MenuGroup::Stack),
    },
    ActionEntry {
        id: ActionId::TaskEdit,
        label: "Edit task",
        contexts: &[ContextId::Today, ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 70,
        description: "Opens the focused task's detail view.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskTitleEdit,
        label: "Edit title",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 72,
        description: "Arms the title editor for the viewed task.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskComplete,
        label: "Complete task",
        contexts: &[ContextId::Today, ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 80,
        description: "Marks the focused task as done.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskReopen,
        label: "Reopen task",
        contexts: &[ContextId::Today, ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 85,
        description: "Marks the focused completed task as not done.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskDueToday,
        label: "Due today (re-commit)",
        contexts: &[ContextId::Today],
        category: Category::Task,
        enabled: true,
        menu_rank: 88,
        description: "Re-commits the focused task to today by stamping its due date at the day window's end; confirmation-free.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskMove,
        label: "Move task",
        contexts: &[ContextId::Today, ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 90,
        description: "Starts the move flow: pick a target stack, then confirm to move the focused task.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskMoveCancel,
        label: "Cancel move",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 95,
        description: "Cancels the active move flow without moving anything (Escape; under emacs also Ctrl+g).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskDelete,
        label: "Delete task",
        contexts: &[ContextId::Today, ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 100,
        description: "Deletes the focused task immediately (no confirmation step).",
        menu_group: Some(MenuGroup::Task),
    },
    // Q23's banner verbs (chunk-3 D4): menu-reachable via `app.menu`;
    // preset bindings are recorded backlog debt with the reminders-UI
    // wave.
    ActionEntry {
        id: ActionId::ReminderSnooze,
        label: "Snooze reminder",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 110,
        description: "Pushes the selected reminder's due moment out by one hour.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::ReminderMute,
        label: "Mute reminders",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 120,
        description: "Silences all reminders on the selected task.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::ReminderUnmute,
        label: "Un-mute reminders",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 121,
        description: "Restores reminders on the focused task after a mute.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::ReminderRuleAdd,
        label: "Add reminder",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 130,
        description: "Adds one reminder rule to the subject task.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::ReminderRuleRemove,
        label: "Remove reminder",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 140,
        description: "Removes the selected reminder rule from the subject task.",
        menu_group: Some(MenuGroup::Task),
    },
    // The timers family (DR-3/3, D-I8): the `reminder.*` one-shot
    // precedent — pointer-driven, menu-reachable, no preset bindings.
    ActionEntry {
        id: ActionId::TimerCreate,
        label: "Create timer",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 150,
        description: "Creates a standalone alarm from the timers form's label and timing.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TimerCancel,
        label: "Cancel timer",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 160,
        description: "Cancels the selected standalone alarm before or after it fires.",
        menu_group: Some(MenuGroup::Task),
    },
    // The tracking verbs (DR-5/4, D-T16): the timers family's
    // pointer-driven precedent — menu-reachable, no preset bindings;
    // dispatch-time gating only (the planner's idempotent NoOps own
    // the refusals).
    ActionEntry {
        id: ActionId::TaskTrackStart,
        label: "Start tracking",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 170,
        description: "Starts tracking the focused task: any running timer closes, this task's opens.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskTrackStop,
        label: "Stop tracking",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 180,
        description: "Stops the running timer, closing its open time entry.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskToggleTracker,
        label: "Toggle tracker",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 185,
        description: "Marks or unmarks the focused task as a tracker: a tracked, never-completed card.",
        menu_group: Some(MenuGroup::Task),
    },
    // The entries overlay (r4 chunk 10, D-TS6/D-TS8): the one
    // menu-reachable row of the family (the DR-5 fence's
    // "menu-reachable" clause); the panel verbs below are
    // section-scoped — pointer-driven, no preset bindings, never
    // rendered as a menu section (the two-tier rule), with
    // dispatch-time gating only (the view intent's kiosk refusal).
    ActionEntry {
        id: ActionId::TaskTrackEntries,
        label: "Time entries",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 186,
        description: "Opens the focused task's time-entries overlay: the tracked history, add, edit, and delete.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskTrackEntryLog,
        label: "Log time entry",
        contexts: &[ContextId::TrackEntries],
        category: Category::Task,
        enabled: true,
        menu_rank: 220,
        description: "Commits the entries form's add draft as a new closed time entry.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::TaskTrackEntryAdjust,
        label: "Adjust time entry",
        contexts: &[ContextId::TrackEntries],
        category: Category::Task,
        enabled: true,
        menu_rank: 230,
        description: "Commits the entries form's edit mask onto the seeded entry.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::TaskTrackEntryDelete,
        label: "Delete time entry",
        contexts: &[ContextId::TrackEntries],
        category: Category::Task,
        enabled: true,
        menu_rank: 240,
        description: "Deletes the armed entry row (not undoable — the confirm is the affordance).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::TaskTrackEntryDismiss,
        label: "Dismiss entries overlay",
        contexts: &[ContextId::TrackEntries],
        category: Category::Task,
        enabled: true,
        menu_rank: 250,
        description: "Steps the entries overlay back: a focused field's Escape first, then the form, then the overlay.",
        menu_group: None,
    },
    // The concentration overlay (issue #496): the tracking verbs'
    // dispatch-time-gating template — menu-reachable, gated at
    // dispatch (`entry_states`: never kiosk, only while focus mode is
    // configured). The session verbs it surfaces stay the tracking
    // pair; this row only moves presentation.
    ActionEntry {
        id: ActionId::FocusModeToggle,
        label: "Focus mode",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        menu_rank: 210,
        description: "Opens or closes the concentration overlay: countdown, current task, task switcher, quick notes. The session itself is ordinary time tracking.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitLog,
        label: "Log habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 190,
        description: "+1 the addressed tracker's current window (surplus logs are first-class).",
        // Section-scoped (R-11): the row's own affordance is the only
        // invocation — the menu surface is untouched, like the plan's
        // "menu/keymap: nothing" ruling.
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitCreate,
        label: "Add habit…",
        contexts: &[ContextId::Today],
        category: Category::Task,
        enabled: true,
        menu_rank: 215,
        description: "Opens the habit create form under the Today section's header (a second press discards and closes it). The habit lands on the section's board.",
        // The accessibility ledger's keyboard parity (round-7 D-7.2):
        // bound in every preset — the habit family's deliberate
        // exception, recorded at the variant.
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitCreateCommit,
        label: "Create habit",
        contexts: &[ContextId::Today],
        category: Category::Task,
        enabled: true,
        menu_rank: 216,
        description: "Routes the create form's confirm: CreateHabit with the minted title and quota target.",
        // Internal dispatch verb (see HabitLog): the form's Confirm
        // button is its only affordance.
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitRepair,
        label: "Add make-up",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 200,
        description: "Adds the make-up +1 into the previous window while its overflow grace is open.",
        // Section-scoped (see HabitLog): the row's own affordance is
        // the only invocation.
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitConvertArm,
        label: "Convert habit to series",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 210,
        description: "Arms the row's conversion confirm (the habit retires; its history links to the new series).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitConvertToSeries,
        label: "Confirm habit conversion",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 220,
        description: "Routes the confirmed habit-to-series conversion with the quota's default pattern.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::SeriesPause,
        label: "Pause series",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 230,
        description: "Opens the series' pause editor (vacation pause: the covered members do not exist).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::SeriesPauseCommit,
        label: "Commit pause",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 240,
        description: "Routes the parsed inclusive pause-through date to the series pause verb.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::SeriesResume,
        label: "Resume series",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 250,
        description: "Clears the series' pause field (an expired pause is inert; the clear is badge hygiene).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::SeriesConvertArm,
        label: "Convert series to habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 260,
        description: "Arms the recur row's conversion confirm (the series stops repeating; the habit continues its history).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::SeriesConvertToHabit,
        label: "Confirm series conversion",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 270,
        description: "Routes the confirmed series-to-habit conversion with the pattern's default quota.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::ViewPlanNextDay,
        label: "Plan next day",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 65,
        description: "Opens or closes the plan-next-day surface: tomorrow's placed boxes and the candidate pool. Placement writes ride the ordinary scheduled/effort edits.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    // The planner's row verbs (#622, D-6): section-scoped (see
    // HabitLog) — the row's own affordance is the only invocation,
    // the menu surface is untouched.
    ActionEntry {
        id: ActionId::PlanBeginPlace,
        label: "Place task",
        contexts: &[ContextId::PlanNextDay],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 280,
        description: "Arms the placement form over the addressed pool row (re-opening a placed row edits that box).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PlanCommitPlace,
        label: "Commit placement",
        contexts: &[ContextId::PlanNextDay],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 290,
        description: "Routes the parsed time-of-day and optional effort as one scheduled/effort edit — atomic, one undo step.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PlanBoxEarlier,
        label: "Move box earlier",
        contexts: &[ContextId::PlanNextDay],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 300,
        description: "Swaps the addressed box's start instant with the earlier neighbor's (the instant set is invariant; no arithmetic).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PlanBoxLater,
        label: "Move box later",
        contexts: &[ContextId::PlanNextDay],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 310,
        description: "Swaps the addressed box's start instant with the later neighbor's.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PlanRemoveBox,
        label: "Remove box",
        contexts: &[ContextId::PlanNextDay],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 320,
        description: "Clears the addressed box's scheduled start; the task returns to the candidate pool.",
        menu_group: None,
    },
    // The review walk (DR-7 chunk 3, #638): the opener is
    // palette/menu-reachable with NO preset binding (P7: reachable,
    // not promoted; kiosk greys it in `entry_states` — layer 2 of the
    // three-layer exclusion). The in-walk verbs are section-scoped
    // (see the variant docs): no menu section, no preset bindings
    // beyond the cursor/back rows.
    ActionEntry {
        id: ActionId::ReviewOpen,
        label: "Review",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 75,
        description: "Opens the review walk over the latest closed, un-reviewed day (or the current day when none is open): inbox triage, carry forward, the plan-next-day surface, then the day's prompts.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::ReviewWalkCursorUp,
        label: "Walk cursor up",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 330,
        description: "Steps the walk's cursor to the previous row of the current stage (clamped; the ends hold).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewWalkCursorDown,
        label: "Walk cursor down",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 340,
        description: "Steps the walk's cursor to the next row of the current stage (clamped; the row verbs pass the last row).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewWalkBack,
        label: "Walk back",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 350,
        description: "Steps the walk back one level: an armed picker or date field disengages first, then the stage steps back, then the walk closes — never dispatching a command.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewWalkAdvance,
        label: "Advance the walk",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 360,
        description: "Moves the walk to its next stage; advancing out of Carry forward hands off to the plan-next-day surface.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewInboxFile,
        label: "File to…",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 370,
        description: "Arms the File-to picker on the cursor inbox row; the pick commits an ordinary end-of-stack move.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewInboxKeep,
        label: "Keep in inbox",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 380,
        description: "Keeps the cursor inbox row where it is and steps to the next row — dispatches nothing.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewFileTargetPrev,
        label: "Previous file target",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 390,
        description: "Moves the armed File-to pick to the previous candidate stack (wrapping).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewFileTargetNext,
        label: "Next file target",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 400,
        description: "Moves the armed File-to pick to the next candidate stack (wrapping).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewFileCommit,
        label: "Commit file",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 410,
        description: "Commits the armed pick: one MoveTask to the picked stack's end, the ordinary move verb.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewCarryTomorrow,
        label: "Move to tomorrow",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 420,
        description: "Reschedules the cursor row to the next reviewed window's opening instant — due dates are never touched.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewCarryPickDate,
        label: "Pick a date",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 430,
        description: "Arms the date field on the cursor carry row; the typed date commits the scheduled edit.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewCarryLeave,
        label: "Leave as is",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 440,
        description: "Leaves the cursor carry row unchanged and steps to the next row — dispatches nothing.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewCarryDateCommit,
        label: "Commit the picked date",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 450,
        description: "Commits the armed date field's text as the row's scheduled instant; an unparseable date dispatches nothing.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewRecord,
        label: "Record",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 460,
        description: "Commits the prompt answers as one reflection entry (an all-blank record commits nothing) and closes the walk.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewFinish,
        label: "Finish without recording",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 470,
        description: "Ends the walk without recording anything — the equal-prominence exit.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitMove,
        label: "Reorder habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 200,
        description: "Moves the addressed tracker to the reorder slot (the stored manual order).",
        // Section-scoped (see HabitLog).
        menu_group: None,
    },
    // The picker keys (round-3 D25): the date grid's ten hardcoded
    // key codes as registry rows — the DatePicker context carries
    // them exactly while the grid holds focus. The time steppers
    // stay button-only (no regression, no invented bindings). The
    // cursor-family rank style; never rendered as a menu section.
    ActionEntry {
        id: ActionId::PickerLeft,
        label: "Picker cursor left",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Moves the calendar picker's cursor one day back.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerRight,
        label: "Picker cursor right",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Moves the calendar picker's cursor one day forward.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerUp,
        label: "Picker cursor up",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Moves the calendar picker's cursor one week back.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerDown,
        label: "Picker cursor down",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Moves the calendar picker's cursor one week forward.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerPrevMonth,
        label: "Picker previous month",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Steps the calendar picker to the previous month, keeping the cursor on-grid.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerNextMonth,
        label: "Picker next month",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Steps the calendar picker to the next month, keeping the cursor on-grid.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerMonthStart,
        label: "Picker month start",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Moves the calendar picker's cursor to the first of the month.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerMonthEnd,
        label: "Picker month end",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Moves the calendar picker's cursor to the last day of the month.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PickerCommit,
        label: "Commit picked date",
        contexts: &[ContextId::DatePicker],
        category: Category::Picker,
        enabled: true,
        menu_rank: 900,
        description: "Commits the calendar picker's cursor date (at the picker's time) through the date flow's one commit funnel.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuNext,
        label: "Next menu entry",
        contexts: &[ContextId::Menu, ContextId::Palette, ContextId::FocusMode],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Moves the menu cursor to the next row of the active page.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuPrevious,
        label: "Previous menu entry",
        contexts: &[ContextId::Menu, ContextId::Palette, ContextId::FocusMode],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Moves the menu cursor to the previous row of the active page.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuInvoke,
        label: "Invoke menu entry",
        contexts: &[ContextId::Menu, ContextId::Palette, ContextId::FocusMode],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Activates the menu row under the cursor.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuDismiss,
        label: "Dismiss menu",
        contexts: &[ContextId::Menu, ContextId::Palette, ContextId::FocusMode],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Steps the menu back one level, or closes it at the top level.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ModeInsert,
        label: "Enter insert mode",
        contexts: GLOBAL,
        category: Category::Mode,
        enabled: true,
        menu_rank: 60,
        description: "Opens the view's editing flow in insert mode.",
        menu_group: Some(MenuGroup::Mode),
    },
    ActionEntry {
        id: ActionId::ModeNormal,
        label: "Return to normal mode",
        contexts: GLOBAL,
        category: Category::Mode,
        enabled: true,
        menu_rank: 70,
        description: "Ends the active editing flow and returns to normal mode (Escape).",
        menu_group: Some(MenuGroup::Mode),
    },
];

/// The registry row for an action.
///
/// # Panics
///
/// Never in a reviewed tree: panics only if the registry lacks a row
/// for the variant — a broken-table bug caught by
/// `registry_covers_every_action_exactly_once`.
#[must_use]
pub fn entry(id: ActionId) -> &'static ActionEntry {
    // Every ActionId variant has a row; the registry is the one
    // definition site (a missing row is a broken-table bug caught by
    // `registry_covers_every_action` below, not a runtime branch).
    REGISTRY
        .iter()
        .find(|entry| entry.id == id)
        .expect("registry has a row for every ActionId variant")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is the one definition site: every enum variant
    /// must have exactly one row.
    #[test]
    #[allow(clippy::too_many_lines)] // the enumerated variant list
    fn registry_covers_every_action_exactly_once() {
        const ALL: &[ActionId] = &[
            ActionId::AppExit,
            ActionId::AppMenu,
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::AppSettings,
            ActionId::AppHelp,
            ActionId::CaptureQuick,
            ActionId::ViewToday,
            ActionId::ViewInbox,
            ActionId::PaletteSearch,
            ActionId::NavNextView,
            ActionId::NavPrevView,
            ActionId::NavFocusBoard,
            ActionId::NavShowTask,
            ActionId::NavNextControl,
            ActionId::NavPrevControl,
            ActionId::ViewScrollUp,
            ActionId::ViewScrollDown,
            ActionId::NavNextItem,
            ActionId::NavPrevItem,
            ActionId::BoardNext,
            ActionId::BoardPrevious,
            ActionId::BoardSelect,
            ActionId::StackWider,
            ActionId::StackNarrower,
            ActionId::TaskCreate,
            ActionId::StackCreate,
            ActionId::TaskEdit,
            ActionId::TaskTitleEdit,
            ActionId::TaskComplete,
            ActionId::TaskReopen,
            ActionId::TaskDueToday,
            ActionId::TaskMove,
            ActionId::TaskMoveCancel,
            ActionId::TaskDelete,
            ActionId::StackNext,
            ActionId::StackPrev,
            ActionId::StackDelete,
            ActionId::StackRename,
            ActionId::StackMove,
            ActionId::BoardCreate,
            ActionId::BoardDelete,
            ActionId::BoardRename,
            ActionId::NavFirstItem,
            ActionId::NavLastItem,
            ActionId::ReminderSnooze,
            ActionId::ReminderMute,
            ActionId::ReminderUnmute,
            ActionId::ReminderRuleAdd,
            ActionId::ReminderRuleRemove,
            ActionId::TimerCreate,
            ActionId::TimerCancel,
            ActionId::TaskTrackStart,
            ActionId::TaskTrackStop,
            ActionId::TaskToggleTracker,
            ActionId::TaskTrackEntries,
            ActionId::TaskTrackEntryLog,
            ActionId::TaskTrackEntryAdjust,
            ActionId::TaskTrackEntryDelete,
            ActionId::TaskTrackEntryDismiss,
            ActionId::FocusModeToggle,
            ActionId::HabitLog,
            ActionId::HabitMove,
            ActionId::HabitCreate,
            ActionId::HabitCreateCommit,
            ActionId::HabitRepair,
            ActionId::HabitConvertArm,
            ActionId::HabitConvertToSeries,
            ActionId::SeriesPause,
            ActionId::SeriesPauseCommit,
            ActionId::SeriesResume,
            ActionId::SeriesConvertArm,
            ActionId::SeriesConvertToHabit,
            ActionId::ViewPlanNextDay,
            ActionId::PlanBeginPlace,
            ActionId::PlanCommitPlace,
            ActionId::PlanBoxEarlier,
            ActionId::PlanBoxLater,
            ActionId::PlanRemoveBox,
            ActionId::ReviewOpen,
            ActionId::ReviewWalkCursorUp,
            ActionId::ReviewWalkCursorDown,
            ActionId::ReviewWalkBack,
            ActionId::ReviewWalkAdvance,
            ActionId::ReviewInboxFile,
            ActionId::ReviewInboxKeep,
            ActionId::ReviewFileTargetPrev,
            ActionId::ReviewFileTargetNext,
            ActionId::ReviewFileCommit,
            ActionId::ReviewCarryTomorrow,
            ActionId::ReviewCarryPickDate,
            ActionId::ReviewCarryLeave,
            ActionId::ReviewCarryDateCommit,
            ActionId::ReviewRecord,
            ActionId::ReviewFinish,
            ActionId::MenuNext,
            ActionId::MenuPrevious,
            ActionId::MenuInvoke,
            ActionId::MenuDismiss,
            ActionId::ModeInsert,
            ActionId::ModeNormal,
            ActionId::PickerLeft,
            ActionId::PickerRight,
            ActionId::PickerUp,
            ActionId::PickerDown,
            ActionId::PickerPrevMonth,
            ActionId::PickerNextMonth,
            ActionId::PickerMonthStart,
            ActionId::PickerMonthEnd,
            ActionId::PickerCommit,
        ];
        for id in ALL {
            assert_eq!(REGISTRY.iter().filter(|e| e.id == *id).count(), 1);
        }
        assert_eq!(REGISTRY.len(), ALL.len());
    }

    /// Ids are unique and dotted `scope.verb` shaped (structural
    /// fact, not text matching — the two-part shape is the §5.2
    /// namespace contract).
    #[test]
    fn ids_are_unique_and_scoped() {
        for entry in REGISTRY {
            assert!(entry.id.id().contains('.'));
        }
        let unique: std::collections::BTreeSet<&str> = REGISTRY.iter().map(|e| e.id.id()).collect();
        assert_eq!(unique.len(), REGISTRY.len());
    }

    /// String ↔ enum conversion runs through the registry (D1): a
    /// known id round-trips, an unknown one is `None` (typed at the
    /// caller as `UnknownAction`).
    #[test]
    fn id_conversion_round_trips_through_the_registry() {
        assert_eq!(
            ActionId::from_id("task.complete"),
            Some(ActionId::TaskComplete)
        );
        assert_eq!(ActionId::from_id("mode.insert"), Some(ActionId::ModeInsert));
        assert_eq!(ActionId::from_id("nope.thing"), None);
        for entry in REGISTRY {
            assert_eq!(ActionId::from_id(entry.id.id()), Some(entry.id));
        }
    }

    /// The §5.8 reservation, asserted structurally: the reserved
    /// global actions exist with labels and are *not* enabled —
    /// exactly the set D4 names (the dumped snapshot shows them as
    /// disabled rows; this test locks the bit).
    #[test]
    fn reserved_globals_are_present_and_disabled() {
        // AppUndo and AppRedo left this set in the undo-surfaces flip
        // (undo/2): their enabled bits are pinned by
        // `undo_and_redo_rows_are_enabled_and_global`, the dispatch arms
        // by the router tests, and the dynamic greying by the adapter's
        // `entry_states`. CaptureQuick left it in the daily-driver
        // round (D-V6's flip); AppSettings left it in round-1 chunk 3
        // (D28b).
        let row = entry(ActionId::AppHelp);
        assert!(!row.enabled, "{} must read disabled", row.id);
        assert_eq!(row.contexts, GLOBAL);
    }

    /// Undo v1 surfaces (undo/2, ADR 0019): the two rows are live,
    /// global, and stay in their order-stable quick-row rank slots.
    #[test]
    fn undo_and_redo_rows_are_enabled_and_global() {
        let undo = entry(ActionId::AppUndo);
        let redo = entry(ActionId::AppRedo);
        assert!(undo.enabled);
        assert!(redo.enabled);
        assert_eq!(undo.contexts, GLOBAL);
        assert_eq!(redo.contexts, GLOBAL);
        assert_eq!((undo.menu_rank, redo.menu_rank), (30, 40));
    }

    /// Post-flip (chunk-3 D6): the *only* disabled rows are the
    /// reserved set — every other action dispatches for real. The
    /// reserved bits themselves are pinned by
    /// `reserved_globals_are_present_and_disabled`.
    #[test]
    fn only_reserved_actions_stay_disabled() {
        for row in REGISTRY {
            if reserved_globals_are_present_and_disabled_set().contains(&row.id) {
                assert!(!row.enabled, "{} is reserved", row.id);
            } else {
                assert!(row.enabled, "{} must be enabled", row.id);
            }
        }
    }

    /// Round-1 D7 totality: every registry row carries a non-empty
    /// description (the row tooltip / a11y description source), and
    /// the top-level quick rows' ranks are strictly spaced (the
    /// documented insertion room).
    #[test]
    fn every_row_carries_a_description_and_spaced_top_ranks() {
        for row in REGISTRY {
            assert!(
                !row.description.is_empty(),
                "{} lacks a description",
                row.id
            );
        }
        let top: Vec<u16> = REGISTRY
            .iter()
            .filter(|row| {
                row.menu_group.is_none()
                    && row.id != ActionId::AppExit
                    // The menu-cursor rows never render on a page; only
                    // the projected quick rows must be rank-ordered.
                    && !row.contexts.contains(&ContextId::Menu)
                    // The picker keys (round-3 D25) share the cursor
                    // rank family and never render anywhere.
                    && row.category != Category::Picker
            })
            .map(|row| row.menu_rank)
            .collect();
        for pair in top.windows(2) {
            assert!(
                pair[1].abs_diff(pair[0]) >= 10,
                "ranks spaced by 10: {pair:?}"
            );
        }
    }

    /// The reserved set as data for the post-flip diff (the original
    /// test's list, kept in one place).
    fn reserved_globals_are_present_and_disabled_set() -> std::collections::BTreeSet<ActionId> {
        std::collections::BTreeSet::from([ActionId::AppHelp])
    }
}
