// SPDX-License-Identifier: AGPL-3.0-only

//! The action registry (GUI Phase 7 chunk 2, design §5.2/§5.8): the
//! stable, namespaced vocabulary every keybinding, menu, and hint is
//! expressed in.
//!
//! Views bind to *actions*, never keys; the registry is the single
//! const table describing every v0.1.0 action — id, label,
//! menu-availability contexts, category, and the `enabled` bit. A
//! dumped registry (the insta snapshot in `tests/inventory/keymap_snapshots.rs`)
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
    /// The status panel (OP3, D-5): a non-masking view context like
    /// `Settings` — the Escape close resolves here (the settings
    /// close law's twin); the panel is read-only, so no other verb
    /// targets the context (R-3: no `StateCommand` is reachable).
    StatusPanel,
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
    /// The recurrence-rule editor overlay (round-7 D-7.1): a total
    /// mask like `TrackEntries` — the modal context while
    /// `Flow::EditRecur` holds; the form's fields ride `ViInsert`
    /// masking and the editor's own verbs (commit, cancel, the
    /// detach strip) live here.
    RecurEditor,
    /// The export dialog (the J2 export plan W-1): a total mask like
    /// `RecurEditor` — the modal context while the adapter's export
    /// VM holds; the dialog's fields ride `ViInsert` masking and the
    /// dialog's own verb (the Escape close) lives here.
    ExportDialog,
    /// The labels overlay (UX R-3 D-2, #789): a total mask like
    /// `TrackEntries` — the modal context while `Flow::Labels`
    /// holds; the form's fields ride `ViInsert` masking and the
    /// overlay's own verbs (dismiss + the row verbs) live here.
    Labels,
    /// The review walk overlay (DR-7 chunk 3, #638): a total mask like
    /// `TrackEntries` — the modal context while `Flow::ReviewWalk`
    /// holds; the prompt fields ride `ViInsert` masking, the walk's
    /// own verbs (cursor, row verbs, commit, back) live here.
    ReviewWalk,
    /// The alarm window (T-22, #805): a total mask like
    /// `Palette`/`FocusMode` — while the always-on-top window holds,
    /// it owns the keyboard exactly as a modal should (the window's
    /// own key grammar consumes; nothing underneath fires). The
    /// window's three verbs and the cursor walk live here, bound in
    /// every preset.
    AlarmWindow,
    /// The in-app notification surface (the focus-restoration plan
    /// US-6/W-6, D-N1): the r5 alarm grammar's twin over the surface's
    /// rows. **Engagement-armed** (the recorded posture deviation):
    /// the surface is ambient — its rows persist while the user keeps
    /// working — so a mask armed on visibility would paralyze every
    /// other journey; the context rides the chain head exactly while
    /// the surface is engaged (armed by its first walk/verb stroke,
    /// disarmed by the Escape close, the last row's exit, or rows
    /// emptying). AC-4's totality is the keymap contract (ids bound in
    /// every preset), which is posture-independent.
    NotificationSurface,
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
            Self::ExportDialog => "export-dialog",
            Self::Today => "today",
            Self::Board => "board",
            Self::StackDetail => "stack-detail",
            Self::TaskDetail => "task-detail",
            Self::Review => "review",
            Self::AlarmWindow => "alarm-window",
            Self::NotificationSurface => "notification-surface",
            Self::Settings => "settings",
            Self::StatusPanel => "status-panel",
            Self::PlanNextDay => "plan-next-day",
            Self::Menu => "menu",
            Self::Palette => "palette",
            Self::FocusMode => "focus-mode",
            Self::ViNormal => "vi-normal",
            Self::ViInsert => "vi-insert",
            Self::DatePicker => "date-picker",
            Self::TrackEntries => "track-entries",
            Self::RecurEditor => "recur-editor",
            Self::Labels => "labels",
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
    /// The undo branch picker's entry (UF2, #353): mounts the
    /// sibling-branch selection menu (desktop-only; R-2).
    AppUndoBranches,
    AppSettings,
    AppHelp,
    /// The export dialog's opener (the J2 export plan US-1, D-1):
    /// mounts the settings/menu-anchored export dialog. A kiosk
    /// dispatch refuses at the adapter (R-2 — no kiosk affordance).
    ExportDialogOpen,
    /// The export dialog's Escape close (W-1's close callback, the
    /// `LabelOverlayDismiss` precedent): the dialog context's total
    /// mask owns Escape; the close dispatches nothing (US-1/AC-2).
    ExportDialogClose,
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
    /// The directional-focus family (J3 US-1, round 9 chunk 9): the
    /// four directions move *focus* one step, never task selection —
    /// the interpretation is per-view (`adapter.rs`'s directional
    /// table). Contexts are the five view contexts, never `Global`
    /// (Global is chain-visible under insert and would fire while
    /// typing — the control walk's recorded rule).
    NavFocusLeft,
    NavFocusRight,
    NavFocusUp,
    NavFocusDown,
    /// The selection-extend twin of the item walk (#875 RIDER,
    /// round 9 chunk 9): the explicit route-level front for the
    /// printable-bound walk rows whose shift bit folds into the glyph
    /// (vi `J`/`K`) and the explicit Shift rows. Board only — the
    /// D-4 scoping: extension is a board surface. The named-key
    /// arrow intercept (`adapter.rs`) stays as the fallback front.
    NavExtendNext,
    NavExtendPrev,
    BoardNext,
    BoardPrevious,
    BoardSelect,
    StackWider,
    StackNarrower,
    TaskCreate,
    StackCreate,
    TaskEdit,
    TaskTitleEdit,
    /// The importance picker's opener (UX R-3 D-1, #788): opens the
    /// immediate-commit value picker over the viewed task's
    /// importance (a view intent; the kiosk/no-subject refusals are
    /// the adapter's arms).
    TaskImportanceEdit,
    /// The urgency picker's opener (UX R-3 D-1, #788): the
    /// importance row's twin — distinct types, one picker grammar.
    TaskUrgencyEdit,
    /// The labels overlay's opener (UX R-3 D-2, #789): opens
    /// `Flow::Labels` over the subject task (a view intent; the
    /// kiosk/no-subject refusals are the adapter's arms). The whole
    /// label family's GUI home — assign/unassign one click from the
    /// detail chips, create/edit/delete in the overlay.
    LabelManage,
    /// The overlay's Escape ladder (D-2): the labels context's
    /// dismiss — a focused field's Escape first (the mode read),
    /// then the overlay closes. The `TaskTrackEntryDismiss`
    /// pattern.
    LabelOverlayDismiss,
    TaskComplete,
    TaskReopen,
    TaskDueToday,
    /// The scheduled field's quick verbs (UX-R8 chunk-5 US-1, D-3):
    /// the quick bar's schedule twins — each routes the existing
    /// sparse `UpdateTask { scheduled }` mask. The *due* twin stays
    /// `TaskDueToday`, Today-only (R-1).
    TaskScheduleToday,
    TaskScheduleTomorrow,
    TaskUnschedule,
    TaskMove,
    TaskMoveCancel,
    /// The Today compact-move pair (US-14, #1031): the R-8 task twins
    /// of the shared Shift+Up/Down stroke — internal dispatch ids, no
    /// preset rows (the habit pair's strokes resolve to them by the
    /// focused row kind, never a second binding). Each routes
    /// `StateCommand::TaskMoveUp`/`TaskMoveDown`; the engine refuses
    /// the boundary typed (no wrap).
    TaskMoveUp,
    TaskMoveDown,
    TaskDelete,
    StackNext,
    StackPrev,
    StackDelete,
    StackRename,
    StackMove,
    BoardCreate,
    BoardDelete,
    BoardRename,
    /// The board color picker's opener (UX R-4 D-1, #791): arms the
    /// swatch strip over the selected board (a view intent; the
    /// board-less refusal is the adapter's arm). Click ≡ key: the
    /// board context menu's row routes the same id.
    BoardColorEdit,
    /// The empty-board preset CTA's action twin (UX R-4 D-5, #795):
    /// applies the default kanban preset to the selected board —
    /// click and key route the same id; the dispatch-time empty-board
    /// gate is the adapter's arm (the typed no-op on a non-empty
    /// board).
    BoardDefaultPreset,
    /// The GUI board-archive verb (O-5 parity chunk OP1, US-1, #945):
    /// arms the archive consent caption strip over the selected board
    /// — the consent caption is the letter's `ArchiveCaption` mode
    /// (D-1: a single row with a consent caption, no counts letter).
    /// The verb stays `NotUndoable(BoardCeremony)` in the engine (the
    /// r1 erratum law); its typed escape is the re-archive path (the
    /// archived roster's restore row), never `AppUndo`. Click ≡ key:
    /// the board context menu's row routes the same id.
    BoardArchive,
    /// The GUI board-reset verb (OP1, US-2, #946): arms
    /// `Flow::ConfirmBoardReset` — the armed strip mounts with the
    /// consequence letter (the live stack/task/label counts, D-3);
    /// Confirm routes the exact `ResetBoard`. The r1 erratum law
    /// holds: `NotUndoable(BoardCeremony)` — the typed escape is the
    /// re-pull path for a bound board. Click ≡ key with the menu row.
    BoardReset,
    /// The GUI board-reconcile verb (OP1, US-3, #947): arms
    /// `Flow::ConfirmBoardReconcile` and requests the oneshot preview
    /// (`UiRequest::BoardReconcilePreview`, D-4) — the strip mounts
    /// with the planner's match counts (the letter's `ReconcilePreview`
    /// mode). Confirm routes the exact `ReconcileByTitle`; the merge
    /// is irreversible (no undo), a zero-count pass is an accepted
    /// idempotent no-op. Click ≡ key with the menu row.
    BoardReconcile,
    NavFirstItem,
    NavLastItem,
    ReminderSnooze,
    ReminderMute,
    ReminderUnmute,
    ReminderRuleAdd,
    ReminderRuleRemove,
    TimerCreate,
    TimerCancel,
    /// The break-rule family (T-14, #797): open the create form —
    /// pointer-driven, timers-composition-mounted, no preset
    /// bindings (the timers family's grammar).
    BreakRuleCreate,
    /// The staged remove's arm step: records the armed subject
    /// (arming touches no engine state); the confirm routes the verb.
    BreakRuleRemove,
    /// The staged remove's confirm: routes `DeleteBreakRule` over the
    /// armed subject (the armed identity types the verb).
    BreakRuleRemoveConfirm,
    /// The alarm window's verbs (T-22, #805): routed over the window's
    /// selected row — the keyboard twins of the pointer buttons (the
    /// routed commands are structurally the buttons'; the shared
    /// `presentation::alarm_command` constructor guarantees it).
    AlarmDismiss,
    AlarmSnooze,
    AlarmCancel,
    /// The notification surface's per-verb ids (US-6/W-6, D-N1): the
    /// r5 alarm grammar's twins — routed over the walk's selected row,
    /// each dispatching exactly the command the row's button seam
    /// sends (the click ≡ key law; `us6_s2` pins the equality).
    NotificationDismiss,
    NotificationDone,
    NotificationSnooze,
    NotificationMute,
    NotificationCancel,
    NotificationTrackStart,
    NotificationOpen,
    TaskTrackStart,
    TaskTrackStop,
    TaskToggleTracker,
    /// The focused row's context-menu opener (US-7, W-8, #1032): the
    /// keyboard twin of the row right-click — the menu opens through
    /// the same `menu_row_subject` fact the pointer seam uses (click
    /// ≡ key by construction). A subjectless walk is the typed
    /// refusal. The R-7 supersession: the one recorded id the D1
    /// no-coining ban exempts for this chunk.
    RowMenu,
    /// The rule cursor's disarm chord (US-10, W-11, #1091): clears the
    /// `TaskDetail` rule-list cursor while armed — the dedicated escape
    /// hatch that returns the verticals to the chunk-9 control ladder
    /// without a view exit or foreign settle — and the typed inert
    /// while disarmed. Composes with the armed-flow grammar: the
    /// disarm latches the cursor's arming gate; the existing clear
    /// sites (view exit, foreign settle, global cancel) stay the only
    /// other clearers.
    RowCursorDisarm,
    /// The ONE smart tracking toggle (J3 R-8, round 9 chunk 11):
    /// start-if-idle, stop-if-running over the subject/one-shot
    /// target — distinct from `TaskToggleTracker` (the tracker-kind
    /// flag, #492). Routes exactly the two existing tracking ids'
    /// outcomes (no new domain command); the card play/pause seams
    /// re-root here (one semantics, one home).
    TaskTrackToggle,
    /// The entries-overlay open verb (r4 chunk 10, D-TS6): opens
    /// `Flow::TrackEntries` over the subject task; refuses on kiosk
    /// (the view intent's own gate).
    TaskTrackEntries,
    /// The effort row's keyboard opener (J3 US-4, round 9 chunk 11):
    /// a *focus* verb — arms nothing, commits nothing; the always-on
    /// M9 field keeps owning the commit (commit-on-focus-loss law;
    /// the D-5 guard absorbs the untouched-draft blur).
    TaskEffortEdit,
    /// The effort estimate's clear twin (J3 US-4, chunk 11):
    /// confirmation-free — the `TaskUnschedule` posture; routes the
    /// sparse `TaskChanges { effort: Some(None) }` mask.
    TaskEffortClear,
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
    /// The ONE smart habit-tracking toggle (journey plan 2026-11-24
    /// W-1/W-2/W-3, D-8, #1305 — the `task.track-toggle` twin, O-7's
    /// one semantics one seam): start-if-idle, stop-if-running over
    /// the row's/one-shot habit subject. Routes exactly the two new
    /// domain commands' outcomes; the planner's idempotent `NoOps` own
    /// the refusals. The dead play control's wiring and the context
    /// menu row re-root here.
    HabitTrackToggle,
    /// The row's "−1" recovery (UX R-2 D-3, #786): the mis-click
    /// class recovers through the audited `AdjustHabitLog` valve —
    /// routes the row's current window with the count decremented
    /// (`n − 1`); count 0 is the typed `NoTarget` (no negative log
    /// exists). Zero journal-class change: `LogHabit` keeps its
    /// recorded posture.
    HabitUndoLog,
    /// The inline quota edit's opener (habit-family UX plan US-4/W-5,
    /// #961): mounts the quota popover over the row's progress phrase
    /// — the click seam and the focused-row key route the same arming
    /// helper (click ≡ key). Commit routes the existing `UpdateHabit`
    /// with `HabitChanges { target: Some(..) }` (zero new commands;
    /// the period kind is locked to the current one — the inline edit
    /// is a one-number edit, UX plan §2 `habit.quota_edit`).
    HabitQuotaEdit,
    /// The quota popover's commit dispatch (the `HabitCreateCommit`
    /// internal-verb twin): routes `UpdateHabit` from the one-shot
    /// draft the commit seam minted (`NoTarget` without one). Internal
    /// verb — the popover's Confirm is its only affordance.
    HabitQuotaCommit,
    HabitMove,
    /// The habit menu's reorder pair (H2 D4, the menu stopgap until
    /// H3's drag/keys reorder): the direction rides the routing id —
    /// a single `HabitMove` id cannot carry the drop slot through the
    /// one-shot grammar — each id routes `HabitReorder` with its
    /// slot's target gap index (#1198 chunk A1 D1; the engine
    /// resolves the orders), minted at menu open by the row seam's
    /// one arithmetic (`presentation::habit_move_slots`).
    HabitMoveUp,
    HabitMoveDown,
    /// The armed conversion's cancel step (H2 D4, O-7 Q3): disarms
    /// the habit → series armed confirm INSIDE the open menu (the
    /// re-projected page greys the confirm again). Internal verb —
    /// the armed submenu's only affordance.
    HabitConvertCancel,
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
    /// The habit editor's opener (H4 D1, US-5): opens the shared
    /// task-detail surface over the focused/menu-row habit (the
    /// `TaskEdit` → Activate shape; the adapter resolves the one-shot
    /// subject).
    HabitEdit,
    /// The habit editor's schedule row (HF4, W-11, US-5): opens the
    /// editor over the focused habit anchored at the weekday picker —
    /// the picker is the editor's kind-gated row, the key is its
    /// Today-context anchor (the `HabitEdit` opener shape).
    HabitScheduleEdit,
    /// The habit editor's reminder row (HF4, W-11, US-6): opens the
    /// editor over the focused habit anchored at the override section
    /// (the same opener shape).
    HabitReminderEdit,
    /// The habit history dialog's mount (HF3 W-8, US-7/S1, #957):
    /// opens the month-grid heatmap + statistics projection over the
    /// one-shot habit subject. Read role — zero commands dispatch
    /// (the plan's no-command reason per US-7/S2–S4); desktop-only
    /// (R-3: the kiosk never arms the mount).
    HabitHistory,
    /// The GUI habit remove's arm step (H4 D4, #944): the staged
    /// consequence-first grammar (the task-delete/`BreakRuleRemove`
    /// shape) — arms the confirm inside the open habit menu; the
    /// dispatch itself waits for Confirm.
    HabitRemove,
    /// The staged remove's confirm dispatch: routes
    /// `StateCommand::DeleteHabit` (the tombstone: the log outlives).
    HabitRemoveConfirm,
    /// The armed remove's cancel step: disarms INSIDE the open menu;
    /// nothing was dispatched.
    HabitRemoveCancel,
    /// The row's typed planned skip (HF1, #955; plan
    /// 2026-11-21-habit-skip-pause W-1): marks the habit's current
    /// canonical window skipped — the streak survives an intentional
    /// gap (R-4: current window only; R-5: period grain).
    HabitSkip,
    /// The pause editor's opener (HF1, #958; W-3): opens the inline
    /// pause editor over the focused/menu-row habit (the
    /// `SeriesPause` shape; the adapter resolves the one-shot subject).
    HabitPause,
    /// The pause editor's commit: routes `PauseHabit` with the parsed
    /// inclusive date (`NoTarget` without target or valid draft).
    HabitPauseCommit,
    /// The resume affordance: routes `PauseHabit { until: None }` —
    /// the field clears for badge hygiene (the `SeriesResume` twin).
    HabitResume,
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
    /// The recurrence-rule editor's opener (round-7 D-7.1): opens the
    /// window-level editor overlay over the subject task — the add
    /// affordance for rule-less tasks and the edit affordance in the
    /// recur row both converge here (click ≡ key).
    SeriesEdit,
    /// The recurrence editor's commit: routes the assembled rule as
    /// the one `UpdateTask { recur: Some(Some(rule)) }` write from
    /// the one-shot target + draft (`NoTarget` without either — the
    /// stale-arm law).
    SeriesEditCommit,
    /// The recurrence editor's stop-repeating confirm: routes
    /// `UpdateTask { recur: Some(None) }` — the detach semantic, and
    /// nothing else in the changeset.
    SeriesEditDetach,
    /// The stop-repeating strip's arm step: shows the consequence
    /// caption (the conversion-strip pattern); the dispatch itself
    /// waits for the confirm.
    SeriesEditDetachArm,
    /// The stop-repeating strip's cancel.
    SeriesEditDetachCancel,
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
    /// The plan-next-day choice out of Carry (J4 US-3/S1): mounts the
    /// embedded planner step.
    ReviewWalkPlanner,
    /// The Prev control: one visited stage back (the dialog button).
    ReviewWalkPrev,
    /// The completion summary's exit: closes the walk, restores focus.
    ReviewWalkComplete,
    /// The abort arm (the dialog host's outside click; J4 R-2).
    ReviewAbortArm,
    /// The abort confirm's accept ("End review").
    ReviewAbortAccept,
    /// The abort confirm's dismiss ("Keep reviewing").
    ReviewAbortDismiss,
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
    /// The due-date editor's keyboard opener (keymap-parity plan
    /// US-3, D-4): routes the round-3 D11 `begin_date_edit` funnel —
    /// the exact seam the due row's edit affordance calls (click ≡
    /// key at the funnel). The subjectless/kiosk refusals are the
    /// adapter's arms.
    TaskDueEdit,
    /// The scheduled-date twin of [`ActionId::TaskDueEdit`] (the same
    /// plan row): one funnel, the `DateField` decides the mask.
    TaskScheduledEdit,
    /// The description editor's keyboard opener (keymap-parity plan
    /// US-3, D-4): routes the round-3 D15 description-editor arm
    /// exactly as the field's pointer affordance does; the
    /// `Editor`-scope TEXT settle rule applies unchanged.
    TaskDescriptionEdit,
    /// The status panel's opener (OP3 W-1, journey plan
    /// `2026-11-21-parity-restore-journeys-status-surface.md` §7):
    /// routes `UiEffect::OpenStatusPanel` — the menu-anchored
    /// diagnostics panel's entry point. Toggle semantics like the
    /// settings opener; the panel dispatches no `StateCommand` (R-3).
    StatusPanelOpen,
    /// The status panel's Kill-holder row (the D2 command plan W-1,
    /// §7's delta): no payload — arms the armed-confirm strip (the
    /// task-delete grammar); Confirm dispatches
    /// `UiRequest::StatusRemediate { KillHolder }`. The dispatch
    /// seam is the panel's own (the export-Run precedent: the verb
    /// address exists for the census/parity; never a rendered menu
    /// row).
    StatusHolderKill,
    /// The status panel's Force-unlock row (the D2 command plan W-2):
    /// arms the strip; Confirm dispatches
    /// `UiRequest::StatusRemediate { ForceUnlock }`.
    StatusLockForceUnlock,
    /// The status panel's Restart-daemon row (the D2 command plan
    /// W-3): arms the strip; Confirm dispatches
    /// `UiRequest::StatusRemediate { RestartDaemon }`.
    StatusDaemonRestart,
    /// The export dialog's Preview dispatch (the D2 preview plan
    /// W-1): one `UiRequest::ExportPreview` over the dialog's spec —
    /// never `ExportRun` (D-4: no file is written).
    ExportPreview,
    /// The export dialog's Copy dispatch (the D2 preview plan W-2):
    /// one `UiRequest::ExportClipboardCopy` over the dialog's spec
    /// (D-3: the payload regenerates app-side).
    ExportClipboardCopy,
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
            Self::AppUndoBranches => "app.undo-branches",
            Self::AppSettings => "app.settings",
            Self::StatusPanelOpen => "app.status-panel-open",
            Self::StatusHolderKill => "status.holder-kill",
            Self::StatusLockForceUnlock => "status.lock-force-unlock",
            Self::StatusDaemonRestart => "status.daemon-restart",
            Self::ExportPreview => "export.preview",
            Self::ExportClipboardCopy => "export.clipboard-copy",
            Self::AppHelp => "app.help",
            Self::ExportDialogOpen => "app.export-dialog",
            Self::ExportDialogClose => "app.export-dialog-close",
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
            Self::NavFocusLeft => "nav.focus-left",
            Self::NavFocusRight => "nav.focus-right",
            Self::NavFocusUp => "nav.focus-up",
            Self::NavFocusDown => "nav.focus-down",
            Self::NavExtendNext => "nav.extend-next",
            Self::NavExtendPrev => "nav.extend-prev",
            Self::BoardNext => "board.next",
            Self::BoardPrevious => "board.previous",
            Self::BoardSelect => "board.select",
            Self::StackWider => "stack.wider",
            Self::StackNarrower => "stack.narrower",
            Self::TaskCreate => "task.create",
            Self::StackCreate => "stack.create",
            Self::TaskEdit => "task.edit",
            Self::TaskTitleEdit => "task.title-edit",
            Self::TaskImportanceEdit => "task.importance-edit",
            Self::TaskUrgencyEdit => "task.urgency-edit",
            Self::LabelManage => "label.manage",
            Self::LabelOverlayDismiss => "label.overlay-dismiss",
            Self::TaskComplete => "task.complete",
            Self::TaskReopen => "task.reopen",
            Self::TaskDueToday => "task.due-today",
            Self::TaskScheduleToday => "task.schedule-today",
            Self::TaskScheduleTomorrow => "task.schedule-tomorrow",
            Self::TaskUnschedule => "task.unschedule",
            Self::TaskMove => "task.move",
            Self::TaskMoveCancel => "task.move.cancel",
            Self::TaskMoveUp => "task.move-up",
            Self::TaskMoveDown => "task.move-down",
            Self::TaskDelete => "task.delete",
            Self::StackNext => "stack.next",
            Self::StackPrev => "stack.prev",
            Self::StackDelete => "stack.delete",
            Self::StackRename => "stack.rename",
            Self::StackMove => "stack.move",
            Self::BoardCreate => "board.create",
            Self::BoardDelete => "board.delete",
            Self::BoardRename => "board.rename",
            Self::BoardColorEdit => "board.color-edit",
            Self::BoardDefaultPreset => "board.default-preset",
            Self::BoardArchive => "board.archive",
            Self::BoardReset => "board.reset",
            Self::BoardReconcile => "board.reconcile",

            Self::NavFirstItem => "nav.first-item",
            Self::NavLastItem => "nav.last-item",
            Self::ReminderSnooze => "reminder.snooze",
            Self::ReminderMute => "reminder.mute",
            Self::ReminderUnmute => "reminder.unmute",
            Self::ReminderRuleAdd => "reminder.rule-add",
            Self::ReminderRuleRemove => "reminder.rule-remove",
            Self::TimerCreate => "timer.create",
            Self::TimerCancel => "timer.cancel",
            Self::BreakRuleCreate => "breakrule.create",
            Self::BreakRuleRemove => "breakrule.remove",
            Self::BreakRuleRemoveConfirm => "breakrule.remove-confirm",
            Self::AlarmDismiss => "alarm.dismiss",
            Self::AlarmSnooze => "alarm.snooze",
            Self::AlarmCancel => "alarm.cancel",
            Self::NotificationDismiss => "notification.dismiss",
            Self::NotificationDone => "notification.done",
            Self::NotificationSnooze => "notification.snooze",
            Self::NotificationMute => "notification.mute",
            Self::NotificationCancel => "notification.cancel",
            Self::NotificationTrackStart => "notification.track-start",
            Self::NotificationOpen => "notification.open",
            Self::TaskTrackStart => "task.track-start",
            Self::TaskTrackStop => "task.track-stop",
            Self::TaskToggleTracker => "task.toggle-tracker",
            Self::RowMenu => "row.menu",
            Self::RowCursorDisarm => "row.cursor-disarm",
            Self::TaskTrackToggle => "task.track-toggle",
            Self::TaskTrackEntries => "task.track-entries",
            Self::TaskEffortEdit => "task.effort-edit",
            Self::TaskEffortClear => "task.effort-clear",
            Self::TaskTrackEntryLog => "task.track-entry-log",
            Self::TaskTrackEntryAdjust => "task.track-entry-adjust",
            Self::TaskTrackEntryDelete => "task.track-entry-delete",
            Self::TaskTrackEntryDismiss => "task.track-entry-dismiss",
            Self::FocusModeToggle => "focus.toggle",
            Self::HabitLog => "habit.log",
            Self::HabitTrackToggle => "habit.track-toggle",
            Self::HabitUndoLog => "habit.unlog",
            Self::HabitQuotaEdit => "habit.quota-edit",
            Self::HabitQuotaCommit => "habit.quota-commit",
            Self::HabitMove => "habit.move",
            Self::HabitMoveUp => "habit.move-up",
            Self::HabitMoveDown => "habit.move-down",
            Self::HabitConvertCancel => "habit.convert-cancel",
            Self::HabitCreate => "habit.create",
            Self::HabitCreateCommit => "habit.create-commit",
            Self::HabitRepair => "habit.repair",
            Self::HabitConvertArm => "habit.convert-arm",
            Self::HabitConvertToSeries => "habit.convert-to-series",
            Self::HabitEdit => "habit.edit",
            Self::HabitScheduleEdit => "habit.schedule-edit",
            Self::HabitReminderEdit => "habit.reminder-edit",
            Self::HabitHistory => "habit.history",
            Self::HabitRemove => "habit.remove",
            Self::HabitRemoveConfirm => "habit.remove-confirm",
            Self::HabitRemoveCancel => "habit.remove-cancel",
            Self::HabitSkip => "habit.skip",
            Self::HabitPause => "habit.pause",
            Self::HabitPauseCommit => "habit.pause-commit",
            Self::HabitResume => "habit.resume",
            Self::SeriesPause => "series.pause",
            Self::SeriesPauseCommit => "series.pause-commit",
            Self::SeriesResume => "series.resume",
            Self::SeriesConvertArm => "series.convert-arm",
            Self::SeriesConvertToHabit => "series.convert-to-habit",
            Self::SeriesEdit => "series.edit",
            Self::SeriesEditCommit => "series.edit-commit",
            Self::SeriesEditDetach => "series.edit-detach",
            Self::SeriesEditDetachArm => "series.edit-detach-arm",
            Self::SeriesEditDetachCancel => "series.edit-detach-cancel",
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
            Self::ReviewWalkPlanner => "review.planner",
            Self::ReviewWalkPrev => "review.walk-prev",
            Self::ReviewWalkComplete => "review.walk-complete",
            Self::ReviewAbortArm => "review.abort-arm",
            Self::ReviewAbortAccept => "review.abort-accept",
            Self::ReviewAbortDismiss => "review.abort-dismiss",
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
            Self::TaskDueEdit => "task.due-edit",
            Self::TaskScheduledEdit => "task.scheduled-edit",
            Self::TaskDescriptionEdit => "task.description-edit",
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
/// The task-schedule family's availability contexts (UX-R8 chunk-5
/// D-3, US-1): the `task.due-today` family's surface slice —
/// `Board`, `Today`, `StackDetail`, `TaskDetail` — the same set
/// `VIEW_NAV` names
/// (the slice is aliased, not re-pasted).
const TASK_CONTEXTS: &[ContextId] = VIEW_NAV;
const VIEW_NAV: &[ContextId] = &[
    ContextId::Today,
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
];
/// The item walk's availability contexts (J3 R-3, 2026-11-19):
/// `TaskDetail` left the set with the subject-walk retirement (the
/// census-dated round-8 D-2 supersession) — the detail never offers
/// the item walk again; the view slice above stays the task-schedule
/// family's (`TASK_CONTEXTS` aliases it).
const ITEM_WALK: &[ContextId] = &[ContextId::Today, ContextId::Board, ContextId::StackDetail];
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
        id: ActionId::AppUndoBranches,
        label: "Undo branches…",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        // The picker row rides directly behind the v1 pair (the
        // BF-W-N2 doctrine: the reflex rows first, the less-common
        // rows after).
        menu_rank: 50,
        description: "Opens the undo branch picker: abandoned redo branches and earlier line points, as rows. The plain undo and redo reflexes stay unchanged.",
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
        id: ActionId::ExportDialogOpen,
        label: "Export…",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        // The root page's data-verb row (the plan's D-1 action-menu
        // entry point); rank between the quick rows and the menu verb.
        menu_rank: 25,
        description: "Opens the export dialog: tracking csv/json and the pdf today sheet, with the CLI verb's filters.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ExportDialogClose,
        label: "Close export dialog",
        contexts: &[ContextId::ExportDialog],
        category: Category::Application,
        enabled: true,
        // Never a rendered menu row (the dialog's own verb; the
        // `LabelOverlayDismiss` shape).
        menu_rank: 990,
        description: "Closes the export dialog; nothing dispatches and any running export completes independently.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::StatusPanelOpen,
        label: "Status…",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        // The Help group's diagnostics row (the group-page entry
        // point): the root page's quick rows are the curated four
        // (menu.rs T-1) and the G6 geometry budget fits exactly those,
        // so a fifth quick row would clip the menu box at the kiosk
        // panel's 1024×600 — the diagnostics panel rides the Help
        // group page instead.
        menu_rank: 75,
        description: "Opens or closes the status panel: who serves the database, the sync posture, and the daemon/lock diagnostics. Read-only.",
        menu_group: Some(MenuGroup::Help),
    },
    // The status panel's remediation verbs (the D2 command plan
    // §7): scoped to the panel context, never rendered rows (the
    // menu budget law, R-3 of the chunk plan) — the dispatch seams
    // are the panel's own; the addresses exist for the census and
    // the keymap's context resolution.
    ActionEntry {
        id: ActionId::StatusHolderKill,
        label: "Kill lock holder",
        contexts: &[ContextId::StatusPanel],
        category: Category::Application,
        enabled: true,
        menu_rank: 1000,
        description: "Arms the confirm strip for signalling the displayed lock holder to terminate; nothing dispatches until confirmed.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::StatusLockForceUnlock,
        label: "Force unlock",
        contexts: &[ContextId::StatusPanel],
        category: Category::Application,
        enabled: true,
        menu_rank: 1010,
        description: "Arms the confirm strip for removing the stale lock sidecar; the app-side executor re-checks holder liveness before anything is removed.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::StatusDaemonRestart,
        label: "Restart daemon",
        contexts: &[ContextId::StatusPanel],
        category: Category::Application,
        enabled: true,
        menu_rank: 1020,
        description: "Arms the confirm strip for a graceful daemon stop plus a detached daemon spawn; nothing dispatches until confirmed.",
        menu_group: None,
    },
    // The export dialog's preview/clipboard verbs (the D2 preview
    // plan §7): scoped to the dialog context, never rendered rows.
    ActionEntry {
        id: ActionId::ExportPreview,
        label: "Preview export payload",
        contexts: &[ContextId::ExportDialog],
        category: Category::Application,
        enabled: true,
        menu_rank: 1030,
        description: "Generates the csv/json payload the export would write and renders it in the read-only pane; no file is written.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ExportClipboardCopy,
        label: "Copy export payload",
        contexts: &[ContextId::ExportDialog],
        category: Category::Application,
        enabled: true,
        menu_rank: 1040,
        description: "Regenerates the csv/json payload app-side and copies it to the clipboard; no file is written.",
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
        contexts: ITEM_WALK,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 120,
        description: "Moves the focus to the next card or switcher entry in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFirstItem,
        label: "Focus first item",
        contexts: ITEM_WALK,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 140,
        description: "Moves the focus to the first card in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavLastItem,
        label: "Focus last item",
        contexts: ITEM_WALK,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 150,
        description: "Moves the focus to the last card in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavPrevItem,
        label: "Focus previous item",
        contexts: ITEM_WALK,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 130,
        description: "Moves the focus to the previous card or switcher entry in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    // The directional-focus family (J3 US-1, round 9 chunk 9): the
    // five view contexts, never Global (the control walk's recorded
    // insert-mode rule). Interpretation is per-view — the adapter's
    // directional table owns what "left" means on each surface.
    ActionEntry {
        id: ActionId::NavFocusLeft,
        label: "Focus left",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 121,
        description: "Moves the focus one step left; the direction's meaning is per view (stack step on the board, control walk on the forms).",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFocusRight,
        label: "Focus right",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 122,
        description: "Moves the focus one step right; the direction's meaning is per view (stack step on the board, control walk on the forms).",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFocusUp,
        label: "Focus up",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 123,
        description: "Moves the focus one step up; the direction's meaning is per view (item walk on the lists, control walk on the forms).",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFocusDown,
        label: "Focus down",
        contexts: CONTROL_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 124,
        description: "Moves the focus one step down; the direction's meaning is per view (item walk on the lists, control walk on the forms).",
        menu_group: Some(MenuGroup::ViewNav),
    },
    // The selection-extend pair (#875 RIDER): Board only — extension
    // is a board surface (the D-4 scoping). The named-key arrow
    // intercept stays the fallback front for layouts that deliver
    // base-glyph named keys with the shift bit.
    ActionEntry {
        id: ActionId::NavExtendNext,
        label: "Extend selection to next item",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 125,
        description: "Extends the board selection to the next card in reading order (the shift-walk twin of focus-next-item).",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavExtendPrev,
        label: "Extend selection to previous item",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 126,
        description: "Extends the board selection to the previous card in reading order (the shift-walk twin of focus-previous-item).",
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
        id: ActionId::BoardDefaultPreset,
        label: "Apply default kanban preset",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 115,
        description: "Creates the default kanban stacks on the current board; inert while the board has any stacks (the empty-board CTA's key twin).",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardColorEdit,
        label: "Board color…",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 105,
        description: "Opens the board color picker: pick a swatch to set the current board's color; Escape or an outside click disarms.",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardDelete,
        label: "Delete board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 110,
        description: "Deletes the current board and all its content: the first press arms the confirm strip, the repeat or the strip's Confirm commits; typed refusals are the engine's.",
        menu_group: Some(MenuGroup::Board),
    },
    // The board ceremony verbs (O-5 parity chunk OP1, US-1/US-2/US-3,
    // #945/#946/#947): pointer-driven context-menu rows + keyboard ids,
    // no preset bindings (the habit verbs' rule). The ceremony fences
    // hold — all three stay `NotUndoable(BoardCeremony)` engine-side;
    // the surfaces are informed-consent confirmations (the r1 erratum
    // law, board-stack plan PC-5/PC-1). The rows' captions carry the
    // consent letter's typed facts (D-1 caption, D-3 counts, D-4
    // preview); the letters themselves mount in the armed strips.
    ActionEntry {
        id: ActionId::BoardArchive,
        label: "Archive board",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 112,
        description: "Archives the current board: the strip's caption carries the archive fact and the typed escape — the archived-boards roster's restore row brings the board back; the verb is not undoable and no undo affordance is offered. (The `ActionId::BoardArchive` id routes the exact `StateCommand::SetBoardArchived` (archived: true).)",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardReconcile,
        label: "Reconcile board…",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 114,
        description: "Arms the reconcile confirm for the current board: the strip's preview letter carries the same-title match counts the merge will adopt (the planner's facts, not prose); the merge is irreversible — survivors keep the local ids and no undo affordance is offered. (The `ActionId::BoardReconcile` id routes the exact `StateCommand::ReconcileByTitle`.)",
        menu_group: Some(MenuGroup::Board),
    },
    ActionEntry {
        id: ActionId::BoardReset,
        label: "Reset board…",
        contexts: &[ContextId::Board],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 119,
        description: "Arms the reset confirm for the current board: the strip's consequence letter carries the live stack, task, and label counts the reset clears; the board row and its binding survive and a bound board re-materializes on the next pull; not undoable. (The `ActionId::BoardReset` id routes the exact `StateCommand::ResetBoard`.)",
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
        description: "Deletes the focused or selected stack and everything in it: the first press arms the confirm strip, the repeat or the strip's Confirm commits; the engine refuses typed when the target is gone.",
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
    // The priority pickers (UX R-3 D-1, #788): the metadata rows'
    // openers — menu-reachable editing verbs over the viewed task
    // (the title-edit row's shape; the rank rides its tail).
    ActionEntry {
        id: ActionId::TaskImportanceEdit,
        label: "Edit importance",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 74,
        description: "Opens the importance picker for the viewed task: six levels, commit on pick.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskUrgencyEdit,
        label: "Edit urgency",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 76,
        description: "Opens the urgency picker for the viewed task: six levels, commit on pick.",
        menu_group: Some(MenuGroup::Task),
    },
    // The detail editors' keyboard openers (keymap-parity plan US-3,
    // D-4): the metadata rows' editing twins — menu-reachable over the
    // viewed task, preset-bound per plan D-1 (the ranks ride the
    // TaskTitleEdit tail's spacing).
    ActionEntry {
        id: ActionId::TaskDueEdit,
        label: "Edit due date",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 73,
        description: "Arms the due-date editor for the viewed task: type or pick, commit on Return.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskScheduledEdit,
        label: "Edit scheduled date",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 75,
        description: "Arms the scheduled-date editor for the viewed task: type or pick, commit on Return.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskDescriptionEdit,
        label: "Edit description",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 77,
        description: "Arms the description editor for the viewed task: Return commits, Escape discards.",
        menu_group: Some(MenuGroup::Task),
    },
    // The labels overlay (UX R-3 D-2, #789): the menu-reachable row
    // of the family (the entries-overlay fence's shape); the overlay
    // rows' verbs stay pointer-grade with the dismiss in its own
    // context.
    ActionEntry {
        id: ActionId::LabelManage,
        label: "Manage labels",
        // Tag-color plan D-7 widens the contexts to the Board surface
        // (the roster-only mount; the TaskDetail mount stands — one
        // action, two subjects).
        contexts: &[ContextId::TaskDetail, ContextId::Board],
        category: Category::Task,
        enabled: true,
        menu_rank: 78,
        description: "Opens the labels overlay: from the board surface the roster-only manager, from the task detail the assignment toggles too.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::LabelOverlayDismiss,
        label: "Dismiss labels overlay",
        contexts: &[ContextId::Labels],
        category: Category::Task,
        enabled: true,
        menu_rank: 270,
        description: "Closes the labels overlay: a focused field's Escape first, then the overlay.",
        menu_group: None,
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
    // The scheduled field's quick verbs (UX-R8 chunk-5 US-1, D-3):
    // the task.due-today family's contexts, at the reminder block's
    // head. Labels are the scheduled field's verbs — the *due* twin
    // stays `task.due-today` (R-1).
    ActionEntry {
        id: ActionId::TaskScheduleToday,
        label: "Schedule today",
        contexts: TASK_CONTEXTS,
        category: Category::Task,
        enabled: true,
        menu_rank: 101,
        description: "Stamps the focused task's scheduled day to today at the day window's end; confirmation-free.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskScheduleTomorrow,
        label: "Schedule tomorrow",
        contexts: TASK_CONTEXTS,
        category: Category::Task,
        enabled: true,
        menu_rank: 102,
        description: "Stamps the focused task's scheduled day to tomorrow at the next day window's start; confirmation-free.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskUnschedule,
        label: "Unschedule",
        contexts: TASK_CONTEXTS,
        category: Category::Task,
        enabled: true,
        menu_rank: 103,
        description: "Clears the focused task's scheduled day; confirmation-free.",
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
    // The Today compact-move pair (US-14, #1031): internal dispatch
    // ids like the habit pair's — the shared Shift+Up/Down stroke
    // resolves to them by focused row kind (R-8), so no preset rows
    // exist; the Task group page lists the twins themselves (the menu
    // rows mint the same one-shot at the dispatch entry).
    ActionEntry {
        id: ActionId::TaskMoveUp,
        label: "Move task up",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 96,
        description: "Moves the focused Today task one step up its stack order; the boundary refuses typed (no wrap).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskMoveDown,
        label: "Move task down",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 97,
        description: "Moves the focused Today task one step down its stack order; the boundary refuses typed (no wrap).",
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
    // The break-rule family (T-14, #797): mounted in the timers
    // composition, NOT the action menu (the stage-1 D-1 letter) —
    // pointer parity with the sibling timer rows is the bar, so no
    // menu rows and no preset bindings.
    ActionEntry {
        id: ActionId::BreakRuleCreate,
        label: "Add break rule",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        // The menu-less projection still carries a distinct rank (the
        // rank-spacing law owns every registry row).
        menu_rank: 710,
        description: "Opens the break-rule form: a work threshold that rings when tracked work crosses it.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::BreakRuleRemove,
        label: "Remove break rule",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 720,
        description: "Arms the roster row's removal: nothing dispatches until confirmed.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::BreakRuleRemoveConfirm,
        label: "Confirm remove break rule",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 730,
        description: "Deletes the armed break rule and prunes its recorded firings.",
        menu_group: None,
    },
    // The alarm window's verbs (T-22, #805): window-local, bound in
    // every preset's total-mask `AlarmWindow` context; never menu
    // rows (the window is its own surface).
    ActionEntry {
        id: ActionId::AlarmDismiss,
        label: "Dismiss alarm",
        contexts: &[ContextId::AlarmWindow],
        category: Category::Task,
        enabled: true,
        // The rank-spacing law owns every registry row (the
        // menu-less projection still carries a distinct rank).
        menu_rank: 740,
        description: "Marks the selected fired alarm's occurrence notified.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AlarmSnooze,
        label: "Snooze alarm",
        contexts: &[ContextId::AlarmWindow],
        category: Category::Task,
        enabled: true,
        menu_rank: 750,
        description: "Postpones the selected fired alarm by the window's snooze span.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AlarmCancel,
        label: "Cancel alarm",
        contexts: &[ContextId::AlarmWindow],
        category: Category::Task,
        enabled: true,
        menu_rank: 760,
        description: "Cancels the selected alarm outright; its row leaves the window.",
        menu_group: None,
    },
    // The notification surface's verbs (US-6/W-6, D-N1): window-local
    // like the alarm twins, bound in every preset over the
    // engagement-armed `NotificationSurface` context; never menu rows
    // (the surface is its own surface; the alarm block's letter).
    ActionEntry {
        id: ActionId::NotificationDismiss,
        label: "Dismiss notification",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 770,
        description: "Marks the selected notification's occurrence dismissed.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::NotificationDone,
        label: "Mark notification task done",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 780,
        description: "Completes the selected notification's task.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::NotificationSnooze,
        label: "Snooze notification",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 790,
        description: "Postpones the selected notification's reminder.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::NotificationMute,
        label: "Mute notification reminders",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 800,
        description: "Mutes the selected notification's task reminders.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::NotificationCancel,
        label: "Cancel notification timer",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 810,
        description: "Cancels the selected notification's timer.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::NotificationTrackStart,
        label: "Start notification task",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 820,
        description: "Starts tracking the selected notification's task.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::NotificationOpen,
        label: "Open notification task",
        contexts: &[ContextId::NotificationSurface],
        category: Category::Task,
        enabled: true,
        menu_rank: 830,
        description: "Opens the selected notification's task in the detail view.",
        menu_group: None,
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
    // The focused row's context-menu opener (US-7, W-8, #1032): the
    // keyboard twin of the row right-click. Not a menu row — the verb
    // OPENS menus; its strokes are the preset rows' truth (the
    // row-verb posture, the group-page geometry budget).
    ActionEntry {
        id: ActionId::RowMenu,
        label: "Open row menu",
        // The Today context (the row verbs' context, the
        // HabitScheduleEdit posture): the verb is never a root-page
        // quick row — the preset strokes are its truth, the curated
        // page stays curated.
        contexts: &[ContextId::Today],
        category: Category::Menu,
        enabled: true,
        menu_rank: 940,
        description: "Opens the focused row's context menu — the keyboard twin of the row right-click.",
        menu_group: None,
    },
    // The rule cursor's disarm chord (US-10, W-11, #1091). Not a menu
    // row — a cursor verb (the RowMenu posture).
    ActionEntry {
        id: ActionId::RowCursorDisarm,
        label: "Disarm row cursor",
        // The TaskDetail context (the cursor's home surface, the
        // RowMenu posture): never a root-page quick row.
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 930,
        description: "Disarms the rule list's selected-row cursor; the detail's vertical keys return to the control ladder.",
        menu_group: None,
    },
    // The ONE smart tracking toggle (J3 R-8, round 9 chunk 11): the
    // state-correct half of Start/Stop over the target — the card
    // play/pause control's routing re-rooted onto a registry id (one
    // semantics, one home; the two existing verbs' outcomes, no new
    // domain command).
    ActionEntry {
        id: ActionId::TaskTrackToggle,
        label: "Toggle tracking",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 187,
        description: "Starts tracking the target task, or stops its running timer if it holds one.",
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
    // The effort row's keyboard pair (J3 US-4, round 9 chunk 11): the
    // focus opener (commit stays the field's, the M9 law) and the
    // confirmation-free clear twin (the TaskUnschedule posture).
    ActionEntry {
        id: ActionId::TaskEffortEdit,
        label: "Edit effort estimate",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 104,
        description: "Focuses the viewed task's effort-estimate field: type the estimate, commit on Return.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskEffortClear,
        label: "Clear effort estimate",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 105,
        description: "Clears the viewed task's effort estimate, confirmation-free.",
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
        id: ActionId::HabitTrackToggle,
        label: "Track habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 191,
        description: "Start or stop a time tracker on the addressed habit (one running timer app-wide).",
        // The context menu row (journey plan W-2, US-31/S1): the
        // HABIT_CONTEXT carries it right after the log row (the
        // destructive-last grammar leaves it in the body — a toggle is
        // not destructive). Grouped Task (the HabitSkip shape): a
        // group-less registration would join the root quick rows and
        // break the #1184/#1289 budget census — the HABIT_CONTEXT page
        // filters the registry directly, so the group is its display
        // home, not its only surface.
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitUndoLog,
        label: "Take one habit log back",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 200,
        description: "−1 the addressed tracker's current window (the mis-click recovery; count 0 refuses typed as NoTarget).",
        // Section-scoped (the HabitLog R-11 ruling's exact twin):
        // the row's own affordance is the only invocation.
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitQuotaEdit,
        label: "Edit habit quota…",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 990,
        description: "Opens the inline quota editor over the focused tracker's progress phrase (a one-number edit; the period kind stays).",
        // Section-scoped (the HabitLog R-11 ruling's exact twin): the
        // row's own affordance is the invocation; the key (O-8 parity,
        // UX plan W-11) routes over the focused row. Group-page row
        // (the H4 family posture) — never the root quick page (the
        // G6 curation budget).
        menu_group: Some(MenuGroup::Task),
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
        id: ActionId::HabitQuotaCommit,
        label: "Save quota",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 995,
        description: "Routes the quota popover's confirm: UpdateHabit with the minted target over the habit's current period kind.",
        // Internal dispatch verb (the HabitCreateCommit twin): the
        // popover's Confirm button is its only affordance. Group-page
        // row (the H4 family posture).
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitRepair,
        label: "Add make-up",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 205,
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
        id: ActionId::HabitEdit,
        label: "Edit habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 240,
        description: "Opens the habit editor on the shared detail surface (title, description, quota, anchor time and day).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitScheduleEdit,
        label: "Edit habit schedule",
        // The Today context (W-11): the row verbs' context — the
        // editor anchors route over the focused habit there; not the
        // Menu context (the picker is a section of the editor surface,
        // never an action-menu row — the G6 geometry budget).
        contexts: &[ContextId::Today],
        category: Category::Task,
        enabled: true,
        description: "Opens the habit editor at the scheduled-days row (week/month habits only; a daily habit happens every day).",
        // Not a menu row: the picker is a *section of the editor
        // surface* (editor-anchored, W-11); the action menu's Task
        // group is the row verbs' page (the G6 geometry budget). The
        // rank sits in the cursor family's gap (unrendered rows are
        // rank-ordered only for the quick-row projection).
        menu_rank: 960,
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitReminderEdit,
        label: "Edit habit reminder",
        // The Today context (W-11; see the schedule row).
        contexts: &[ContextId::Today],
        category: Category::Task,
        enabled: true,
        description: "Opens the habit editor at the reminder-override row (base, offset, time, snooze).",
        // Editor-anchored, W-11 (see the schedule row).
        menu_rank: 970,
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitHistory,
        label: "Habit history",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 245,
        description: "Opens the habit's history: the month-grid heatmap and the statistics (adherence, best streak, per-window counts).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitRemove,
        label: "Remove habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 250,
        description: "Arms the habit-remove confirm (consequence first: the dispatch waits for Confirm).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitRemoveConfirm,
        label: "Confirm habit removal",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 252,
        description: "Routes the confirmed removal: the habit tombstones, its log history outlives it.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitSkip,
        label: "Skip today",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 206,
        description: "Marks the habit's current window skipped: the typed planned gap — the streak survives.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitPause,
        label: "Pause until…",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 207,
        description: "Opens the pause editor: covered windows are neither counted nor missed and no nudges fire.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitPauseCommit,
        label: "Commit pause",
        // Today-scoped (the HabitCreateCommit internal-verb shape): the
        // commit row must never join the root quick page — a Global
        // context with menu_group None would grow the toolbar menu by
        // one row and clip the panel's bottom border at the kiosk
        // geometry (the rank-spacing law caught the rank, this context
        // keeps the page shape).
        contexts: &[ContextId::Today],
        category: Category::Task,
        enabled: true,
        menu_rank: 230,
        description: "Routes the parsed inclusive pause-through date to the habit pause verb.",
        // Internal dispatch verb (see HabitLog): the editor's Confirm
        // is its only affordance.
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitResume,
        label: "Resume habit",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 209,
        description: "Clears the habit's pause field (the clear is badge hygiene; not-paused is the accepted no-op).",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::HabitRemoveCancel,
        label: "Cancel removal",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 254,
        description: "Disarms the armed habit-remove confirm; nothing was dispatched.",
        // Internal verb — the armed page's only affordance.
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitMoveUp,
        label: "Move habit up",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 226,
        description: "Moves the habit one slot up its section (the gap-index reorder; the engine resolves the orders).",
        // Internal dispatch verb (see HabitLog): the menu row is its
        // only affordance until H3 replaces it.
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitMoveDown,
        label: "Move habit down",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 236,
        description: "Moves the habit one slot down its section (the gap-index reorder; the engine resolves the orders).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::HabitConvertCancel,
        label: "Cancel conversion",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 246,
        description: "Disarms the armed habit-to-series conversion confirm; nothing was dispatched.",
        menu_group: None,
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
    // The recurrence-rule editor's verbs (round-7 D-7.1): the opener
    // is TaskDetail-scoped (a `TaskTitleEdit`-class editing verb, the
    // one series verb with preset bindings); the in-overlay verbs are
    // the editor context's pointer-grade rows (the picker-actions
    // rule — no preset bindings, focus + Enter on the mounted
    // buttons).
    ActionEntry {
        id: ActionId::SeriesEdit,
        label: "Edit repeat",
        contexts: &[ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 275,
        description: "Opens the recurrence-rule editor over the viewed task: the pattern, the bound, and the stop-repeating strip.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::SeriesEditCommit,
        label: "Commit repeat rule",
        contexts: &[ContextId::RecurEditor],
        category: Category::Task,
        enabled: true,
        menu_rank: 300,
        description: "Routes the assembled rule as the one recurrence write; an invalid draft dispatches nothing.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::SeriesEditDetach,
        label: "Stop repeating",
        contexts: &[ContextId::RecurEditor],
        category: Category::Task,
        enabled: true,
        menu_rank: 310,
        description: "Clears the rule (the task stays as an ordinary task); nothing else changes.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::SeriesEditDetachArm,
        label: "Arm stop repeating",
        contexts: &[ContextId::RecurEditor],
        category: Category::Task,
        enabled: true,
        menu_rank: 320,
        description: "Shows the stop-repeating consequence strip; the confirm dispatches.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::SeriesEditDetachCancel,
        label: "Cancel stop repeating",
        contexts: &[ContextId::RecurEditor],
        category: Category::Task,
        enabled: true,
        menu_rank: 330,
        description: "Disarms the stop-repeating strip.",
        menu_group: None,
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
        description: "Ends the reflection stage without recording — the guilt-free exit lands on the completion summary.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewWalkPlanner,
        label: "Plan next day",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 480,
        description: "Mounts the plan-next-day step inside the review walk (the embedded planner step).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewWalkPrev,
        label: "Walk previous stage",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 490,
        description: "Steps the walk back one visited stage (the Prev control).",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewWalkComplete,
        label: "Finish the review",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 500,
        description: "Leaves the completion summary: closes the walk and restores the view focus.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewAbortArm,
        label: "Arm the review abort confirm",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 510,
        description: "Asks before ending the review mid-walk; the Entry stage closes silently.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewAbortAccept,
        label: "End the review",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 520,
        description: "Accepts the abort confirm: closes the walk; decisions already made stay applied.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ReviewAbortDismiss,
        label: "Keep reviewing",
        contexts: &[ContextId::ReviewWalk],
        category: Category::Navigation,
        enabled: true,
        menu_rank: 530,
        description: "Dismisses the abort confirm: the walk stays exactly as it was.",
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
            ActionId::AppUndoBranches,
            ActionId::AppSettings,
            ActionId::StatusPanelOpen,
            ActionId::StatusHolderKill,
            ActionId::StatusLockForceUnlock,
            ActionId::StatusDaemonRestart,
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
            ActionId::NavFocusLeft,
            ActionId::NavFocusRight,
            ActionId::NavFocusUp,
            ActionId::NavFocusDown,
            ActionId::NavExtendNext,
            ActionId::NavExtendPrev,
            ActionId::BoardNext,
            ActionId::BoardPrevious,
            ActionId::BoardSelect,
            ActionId::StackWider,
            ActionId::StackNarrower,
            ActionId::TaskCreate,
            ActionId::StackCreate,
            ActionId::TaskEdit,
            ActionId::TaskTitleEdit,
            ActionId::TaskImportanceEdit,
            ActionId::TaskUrgencyEdit,
            ActionId::LabelManage,
            ActionId::LabelOverlayDismiss,
            ActionId::TaskComplete,
            ActionId::TaskReopen,
            ActionId::TaskDueToday,
            ActionId::TaskMove,
            ActionId::TaskMoveCancel,
            ActionId::TaskMoveUp,
            ActionId::TaskMoveDown,
            ActionId::TaskDelete,
            ActionId::StackNext,
            ActionId::StackPrev,
            ActionId::StackDelete,
            ActionId::StackRename,
            ActionId::StackMove,
            ActionId::BoardCreate,
            ActionId::BoardDelete,
            ActionId::BoardRename,
            ActionId::BoardColorEdit,
            ActionId::BoardDefaultPreset,
            ActionId::BoardArchive,
            ActionId::BoardReconcile,
            ActionId::BoardReset,
            ActionId::NavFirstItem,
            ActionId::NavLastItem,
            ActionId::ReminderSnooze,
            ActionId::ReminderMute,
            ActionId::ReminderUnmute,
            ActionId::ReminderRuleAdd,
            ActionId::ReminderRuleRemove,
            ActionId::TimerCreate,
            ActionId::TimerCancel,
            ActionId::BreakRuleCreate,
            ActionId::BreakRuleRemove,
            ActionId::BreakRuleRemoveConfirm,
            ActionId::AlarmDismiss,
            ActionId::AlarmSnooze,
            ActionId::AlarmCancel,
            ActionId::NotificationDismiss,
            ActionId::NotificationDone,
            ActionId::NotificationSnooze,
            ActionId::NotificationMute,
            ActionId::NotificationCancel,
            ActionId::NotificationTrackStart,
            ActionId::NotificationOpen,
            ActionId::TaskTrackStart,
            ActionId::TaskTrackStop,
            ActionId::TaskToggleTracker,
            // The focused row's context-menu opener (US-7, W-8, #1032).
            ActionId::RowMenu,
            // The rule cursor's disarm chord (US-10, W-11, #1091).
            ActionId::RowCursorDisarm,
            ActionId::TaskTrackToggle,
            ActionId::TaskTrackEntries,
            ActionId::TaskEffortEdit,
            ActionId::TaskEffortClear,
            ActionId::TaskTrackEntryLog,
            ActionId::TaskTrackEntryAdjust,
            ActionId::TaskTrackEntryDelete,
            ActionId::TaskTrackEntryDismiss,
            ActionId::FocusModeToggle,
            ActionId::HabitLog,
            ActionId::HabitUndoLog,
            ActionId::HabitMove,
            ActionId::HabitCreate,
            ActionId::HabitCreateCommit,
            ActionId::HabitRepair,
            ActionId::HabitConvertArm,
            ActionId::HabitConvertToSeries,
            ActionId::HabitSkip,
            ActionId::HabitPause,
            ActionId::HabitPauseCommit,
            ActionId::HabitResume,
            ActionId::SeriesPause,
            ActionId::SeriesPauseCommit,
            ActionId::SeriesResume,
            ActionId::SeriesConvertArm,
            ActionId::SeriesConvertToHabit,
            ActionId::SeriesEdit,
            ActionId::SeriesEditCommit,
            ActionId::SeriesEditDetach,
            ActionId::SeriesEditDetachArm,
            ActionId::SeriesEditDetachCancel,
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
            ActionId::ReviewWalkPlanner,
            ActionId::ReviewWalkPrev,
            ActionId::ReviewWalkComplete,
            ActionId::ReviewAbortArm,
            ActionId::ReviewAbortAccept,
            ActionId::ReviewAbortDismiss,
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
            // The keymap-parity plan US-3/D-4 additions.
            ActionId::TaskDueEdit,
            ActionId::TaskScheduledEdit,
            ActionId::TaskDescriptionEdit,
            // The scheduled field's quick verbs (UX-R8 chunk-5 D-3).
            ActionId::TaskScheduleToday,
            ActionId::TaskScheduleTomorrow,
            ActionId::TaskUnschedule,
            // The habit menu's reorder pair + armed-cancel step (H2 D4).
            ActionId::HabitMoveUp,
            ActionId::HabitMoveDown,
            ActionId::HabitConvertCancel,
            // The H4 family: the editor opener and the staged remove.
            ActionId::HabitEdit,
            ActionId::HabitHistory,
            ActionId::HabitRemove,
            ActionId::HabitRemoveConfirm,
            ActionId::HabitRemoveCancel,
            // The export dialog's pair (the J2 export plan D-1/W-1):
            // the opener routes the mount, the close is the dialog
            // context's Escape rung.
            ActionId::ExportDialogOpen,
            ActionId::ExportDialogClose,
            ActionId::ExportPreview,
            ActionId::ExportClipboardCopy,
            // The inline quota edit (HF2, W-5 #961) + its internal
            // commit verb.
            ActionId::HabitQuotaEdit,
            ActionId::HabitQuotaCommit,
            // The HF4 editor anchors (W-11): the schedule/override rows
            // open the editor at their sections (the HabitEdit shape).
            ActionId::HabitScheduleEdit,
            ActionId::HabitReminderEdit,
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
