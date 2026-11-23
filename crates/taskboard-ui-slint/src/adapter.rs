// SPDX-License-Identifier: AGPL-3.0-only

//! The slint adapter (chunk decision D18, design §5.6): the only code
//! that touches both worlds — slint key events in, keymap types out,
//! and shell state synced one-way onto window globals. The root
//! `FocusScope` is the single key path (§5.1); `KeyBinding` is unused
//! for any rebindable action (Q9); the keymap is applied one-way at
//! UI start (no live re-map machinery — §5.6).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};

use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, ModelRc, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{
    AppState, BoardId, ReminderOffset, ReminderRuleId, RemoteBoardId, StackId, StateCommand, TaskId,
};

use tokio::sync::oneshot;

use taskboard_domain::ConnectorId;

use crate::actions::{ActionId, ContextId, REGISTRY};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::models::IdTables;
use crate::presentation::ReminderTarget;
use crate::requests::{
    self, ConnectionProfile, ConnectorInfo, InvalidKind, KeymapUpdate, Secret, SettingsError,
    UiRequest,
};
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{
    ActionRouter, Direction, DispatchOutcome, EffectClass, Mode, UiEffect, WalkIntent,
};
use crate::settings_form::{self, ConnectionDraft};
use crate::shell::KeymapShell;
use crate::view_state::{Flow, FlowOutcome, FocusEdge, ViewState, WalkStage, WalkStep};
use crate::{AppWindow, Breakpoint, KeyFeedback, KeyboardState, Responsive, UiStatus};

/// The repaint cadence (D1): the UI-owned timer's tick, in
/// milliseconds — signal drain, minute boundary, then refresh.
pub const REPAINT_CADENCE_MS: u64 = 100;

/// The Q15 startup-warning summary handed over with the validated
/// keymap; the status bar renders it from `UiStatus` (chunk 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartupWarnings {
    pub keymap_findings: u32,
}

/// The open menu's anchor (round-1 D13): the hamburger rect (the
/// markup reads the Toolbar's out properties) or the context menu's
/// pointer point (synced onto `KeyboardState`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) enum MenuAnchor {
    #[default]
    Hamburger,
    Pointer(f32, f32),
}

/// The open menu's kind (UX-R8 chunk-5 D-1, N-11): the main menu's
/// pages re-derive on every refresh pass (round-3 D27c's re-grey),
/// an object/context menu's model is built once at open and nothing
/// re-derives it while open — the popup the user reads is the popup
/// that fires. A typed discriminator, never an anchor proxy: an
/// anchor shape would not survive future anchored main-menu variants.
/// The create-focus handoff's one-shot arm (UX-R8 chunk-5 D-7, US-5):
/// the created id is engine-minted, so the target derives from the
/// post-apply state diff — the subject stack's live ids at commit
/// time vs the next materialized refresh's. No new command, no new
/// request (the origin §7 constraint).
/// The handoff's payload (UX-R8 chunk-5 D-7): the pre-handoff focus
/// (`None` when the create flow held none) — the hint push's input.
#[derive(Debug, Clone)]
struct CreateFocusHandoff {
    before: Option<TaskId>,
}

#[derive(Debug, Clone)]
struct CreateFocusArm {
    stack: StackId,
    before: std::collections::BTreeSet<TaskId>,
    /// The state snapshot at arm time: the commit's own `RefreshNow`
    /// pass runs BEFORE the engine applies, so the consumption waits
    /// for the state pointer to actually change (the engine publish)
    /// — a zero-diff pass over the SAME snapshot keeps the arm.
    snapshot: Arc<AppState>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum MenuKind {
    #[default]
    Main,
    Object,
}

/// The object menu's recorded subject (J5 US-4 AC-2, #927): the
/// disarm reconcile's input. `Single` is the one pinned task; a multi
/// selection snapshots its members (the menu disarms only when every
/// member vanished — partial deletions leave live rows to dispatch);
/// `None` is the stack/board menus and every non-object menu, which
/// never disarm on task death.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum MenuSubject {
    #[default]
    None,
    Single(TaskId),
    Selection(Vec<TaskId>),
}

/// The flow kinds the settle matrix rows range over (round-3
/// W-SM-b; audit report §6). The mirror of the per-kind settle rule:
/// TEXT commits-or-discards, DUE clean-closes, MOVE/DRAG cancel,
/// the modals close, NONE is inert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowKind {
    None,
    Text,
    Due,
    Move,
    Drag,
    Palette,
    Capture,
}

/// What the resolver does with a foreign-flow pair (plan §3.1):
/// settle-then-run, cancel-only, replace, or a typed inert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolve {
    SettleThenRun,
    CancelOnly,
    Replace,
    Inert,
}

/// The effort row's inline-editor draft (task-detail-editor UX plan
/// US-3/D-8, #838): the addressed task and the buffer mirror (seeded
/// from the effort's domain text form; an empty buffer is the clear
/// seed). The date-editor buffer pattern over adapter state — one
/// editor at a time (the detail exclusivity posture).
#[derive(Debug, Clone)]
pub(crate) struct EffortEditorState {
    pub(crate) task: TaskId,
    pub(crate) buffer: String,
    /// The last committed value's trimmed text (D-5's guard): a
    /// commit of an unchanged value dispatches nothing — the
    /// always-on row's focus-loss fires on every blur, and the
    /// Return/blur pair must not double-dispatch.
    pub(crate) last_committed: String,
}

/// The effort row's parse predicate (US-3/D-8): the domain
/// [`taskboard_domain::Effort::parse_text_form`] verbatim — the walk's
/// placement parse (`commit_plan_place`) calls the same function, one
/// predicate, two fronts, no second grammar (AC-3). `None` is the
/// refusal (the buffer kept, nothing dispatches).
pub(crate) fn parse_effort_row(text: &str) -> Option<taskboard_domain::Effort> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    taskboard_domain::Effort::parse_text_form(trimmed)
}

/// The labels overlay's in-place form draft (UX R-3 D-2/D-5, #789):
/// the addressed label (`None` = create), the typed title, and the
/// swatch's exact hex (the palette's vocabulary — never free text).
#[derive(Debug, Clone)]
pub(crate) struct LabelFormState {
    pub seed: Option<taskboard_domain::LabelId>,
    pub title: String,
    pub color: String,
}

/// The status bar's transient notice line (#130): presentation text
/// plus expiry. Never blocking — a passive line wherever the status
/// bar renders.
pub(crate) struct Notice {
    /// The composed phrase (captured at event time).
    text: String,
    /// The expiry instant (event time + the presentation horizon).
    until: DateTime<Utc>,
}

/// The adapter state: the validated keymap (applied at UI start, one
/// way), the shell, the resolver, the router, the view-state machine
/// (D3), the lock-free state read (D1), and the command channel out.
/// The flow liveness the focus hand-off gates on (see
/// `Adapter::flow_open`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowLiveness {
    Open,
    Flowless,
}

/// The one-shot focus hand-over's phase (round-3 D10, r4 D25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FocusHandover {
    #[default]
    None,
    Grid,
    Field,
}

/// The census site a recorded focus origin belongs to (the
/// focus-restoration plan US-1/W-1, decision D-F1; the chunk plan's
/// §2 census table is the enumeration). One seam per site family; the
/// ids ride the `FocusFacts.seam-id` push (the journey tests' typed
/// oracle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusSeam {
    DueEditor,
    RecurEditor,
    LabelsOverlay,
    PriorityDue,
    PriorityScheduled,
    TrackEntries,
    ActionMenu,
    Palette,
    Capture,
    TimerSetter,
    FocusOverlay,
    ReviewWalk,
    TitleEditor,
    DescriptionEditor,
    MoveFlow,
    ConfirmDelete,
    BoardColorPick,
    PlanPlace,
    /// The scheduled-date editor (round 5 A2, #1205): the due editor's
    /// twin seam, split so the detail restore can land the scheduled
    /// cell (the flow's `DateField` read at open time — the restore
    /// never sees the flow).
    ScheduledEditor,
}

impl FocusSeam {
    /// The stable fact id (the `FocusFacts.seam-id` half).
    #[must_use]
    pub(crate) const fn id(self) -> i32 {
        match self {
            Self::DueEditor => 0,
            Self::RecurEditor => 1,
            Self::LabelsOverlay => 2,
            Self::PriorityDue => 3,
            Self::PriorityScheduled => 4,
            Self::TrackEntries => 5,
            Self::ActionMenu => 6,
            Self::Palette => 7,
            Self::Capture => 8,
            Self::TimerSetter => 9,
            Self::FocusOverlay => 10,
            Self::ReviewWalk => 11,
            Self::TitleEditor => 12,
            Self::DescriptionEditor => 13,
            Self::MoveFlow => 14,
            Self::ConfirmDelete => 15,
            Self::BoardColorPick => 16,
            Self::PlanPlace => 17,
            Self::ScheduledEditor => 18,
        }
    }

    /// Whether the site is subject-affine (its census anchor names the
    /// subject task even when no row is logically focused — the due
    /// editor opened from a detail whose walk focus is empty).
    #[must_use]
    pub(crate) const fn subject_affine(self) -> bool {
        matches!(
            self,
            Self::DueEditor
                | Self::ScheduledEditor
                | Self::RecurEditor
                | Self::LabelsOverlay
                | Self::TrackEntries
                | Self::TitleEditor
                | Self::DescriptionEditor
                | Self::PlanPlace
        )
    }
}

/// The restore target's anchor (US-1/AC-2): the invoking element's
/// identity class. A `Task` anchor resolves live-or-dead at restore
/// time; the vanished branch falls to the recorded container
/// (`FocusOrigin::fallback`), never to no-control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginAnchor {
    /// The flat-flow letter (today's D-H7 view-scope refocus).
    View,
    Task(TaskId),
    Stack(StackId),
}

/// The one-shot focus-origin fact (D-F1): recorded by the census
/// site's open pass (last-open-wins — a second open overwrites),
/// consumed by the next restore after the site closes. `fallback` is
/// the anchor row's container captured at record time (the only
/// moment the row tables can name a vanished row's parent — the
/// restore-time tables only list live rows).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FocusOrigin {
    seam: FocusSeam,
    anchor: OriginAnchor,
    fallback: OriginAnchor,
}

use crate::level_state::TrackFormState;

/// The reorder pair's minted one-shot cells (chunk 8 D4, reworked by
/// #1198 chunk A1 D1): the up and down `(habit, target gap index)`
/// pair, `None` where the slot does not exist (the typed `NoTarget`'s
/// input).
type HabitMoveSlots = (
    Option<(taskboard_domain::HabitId, usize)>,
    Option<(taskboard_domain::HabitId, usize)>,
);

#[allow(
    clippy::struct_excessive_bools // one flag per synced gate (the house shape)
)]
pub(crate) struct Adapter {
    keymap: Keymap,
    /// The device's fixed UTC offset (the one genuinely boot-fixed
    /// value the day derivations need; `UiSettings::boot_offset`).
    boot_offset: chrono::FixedOffset,
    /// The today window's derivation pair: the live
    /// `settings.day_start_time` paired with `boot_offset`, re-derived
    /// from the published state at every refresh pass
    /// ([`Self::sync_settings`], round-5 chunk 5) — never hand-over
    /// state; `day_ctx` recomputes the window per pass.
    day_start: (chrono::NaiveTime, chrono::FixedOffset),
    /// The due-soon horizon (round-3 D14/#230): the live
    /// `settings.due_soon_days`, re-derived per refresh — one horizon
    /// for the sets and the grid tint.
    pub(crate) due_soon_horizon: chrono::TimeDelta,
    /// The run mode's one UI bit (Q8): on kiosk the today pass filters
    /// to the focused board's subset; the desktop is cross-board.
    kiosk: bool,
    /// The clock seam (#660, from `UiSettings::clock`): `Some` freezes
    /// [`Self::day_ctx`]'s `now` so goldens over clock-derived content
    /// render time-independently; production always holds `None` and
    /// the pass reads the wall clock.
    frozen_now: Option<chrono::DateTime<chrono::Utc>>,
    /// The reminder defaults (R3 chunk C): the derivation input
    /// `active_notifications` needs for the implicit default's
    /// occurrence — the live `settings.reminder_before`, re-derived
    /// per refresh.
    pub(crate) reminder_defaults: taskboard_domain::ReminderDefaults,
    /// The focus timings (#496): `None` while the mode is off (or on
    /// kiosk — the hand-over contract that a kiosk never carries a
    /// timing) — the overlay action never lights and the countdown
    /// has no input. The live `settings.live_focus()`, re-derived per
    /// refresh.
    pub(crate) focus: Option<taskboard_domain::focus::FocusTiming>,
    /// The per-second tick's display-changes-per-second condition
    /// (r4 D17, decision 5): true only while a mounted surface's
    /// displayed value moves with the wall second. Computed once per
    /// derive in [`Self::compute_seconds_surfaces_active`]; the
    /// repaint driver consults it per tick. Kiosk is structurally and
    /// explicitly excluded.
    pub(crate) seconds_surfaces_active: bool,
    /// The review walk's abort-confirm arm (J4 D-6): `true` while the
    /// nested confirm is mounted; a stage change or the dismissal
    /// clears it.
    walk_abort_armed: bool,
    /// The last [`Self::apply_outcomes`] pass's flow liveness (the
    /// flowless enum variant replaces a bool — clippy's
    /// excessive-bools wall). The D15 focus hand-off (and the r4 D20
    /// flow-end reset) fire on the *transition* to flowless, not on
    /// every flowless pass — a flowless pass must never steal slint
    /// focus (the r4 chunk-6 control walk moves focus; an
    /// unconditional refocus would eat the step it just took).
    flow_open: FlowLiveness,
    /// pub(crate) for the walk-guard test seam only (the shell's
    /// view context is the guard test's chain input).
    #[cfg(test)]
    pub(crate) shell: KeymapShell,
    #[cfg(not(test))]
    shell: KeymapShell,
    resolver: KeymapResolver,
    /// pub(crate) for the lib tests' flow/buffer seams only.
    pub(crate) view_state: ViewState,
    state: Arc<ArcSwap<AppState>>,
    commands: mpsc::Sender<StateCommand>,
    /// The app request lane (round-1 D21): the adapter is the only
    /// sender (`try_send`, warn-and-drop — the F-3 convention). The
    /// lane stays outside the undo journal BY DESIGN — ruling R-17
    /// (2026-11-15, census Settings slice; chunk-5 plan D-4): staged
    /// confirm for the destructive rows, re-edit at the knob for the
    /// rest.
    requests: mpsc::Sender<crate::requests::UiRequest>,
    /// The ACTIVE menu page's entries (root page at the root level;
    /// the open group's page inside a submenu — round-1 D6). The
    /// cursor indexes into it and `InvokeSelected` re-dispatches it.
    menu_entries: Vec<MenuEntry>,
    /// The root page (never mutated while the menu is open): the
    /// fly-out/replace presentation and the `MenuBack` restore read it.
    root_entries: Vec<MenuEntry>,
    /// The pre-menu context chain (round-1 D6): every page projection
    /// resolves against it — the menu context must not mask the chain
    /// the pages are computed from.
    menu_chain: Vec<ContextId>,
    /// The open menu's anchor (round-1 D13).
    menu_anchor: MenuAnchor,
    /// The open menu's kind (UX-R8 D-1): the refresh-time re-derivation
    /// guard scopes itself to `Main`; reset to `Main` at every close.
    menu_kind: MenuKind,
    /// The open object menu's subject (J5 US-4 AC-2, #927): recorded
    /// at open, read by the disarm reconcile on every state pass.
    /// Reset to `None` at every close (`apply_close_menu`).
    menu_subject: MenuSubject,
    /// The task context menu's quick bar (UX-R8 chunk-5 D-2): built
    /// once at open, resolved by the bar seam against this vec (the
    /// `root_entries` index-resolver pattern; stale indices absorbed).
    /// Empty for every other menu consumer.
    quick_bar_entries: Vec<crate::menu::QuickBarEntry>,
    /// The create-focus handoff's armed diff (UX-R8 chunk-5 D-7):
    /// armed by `apply_outcomes`' `CreateTask` arm, consumed once by
    /// the next materialized refresh — zero or >1 new ids clear it
    /// with no handoff (the one-shot law, R-d).
    create_focus_arm: Option<CreateFocusArm>,
    /// The Add-task ghost's clicked stack (r4 D4); consumed by the
    /// next `task.create` dispatch, then cleared (one-shot, the
    /// `reminder_target` precedent).
    create_target: Option<StackId>,
    /// The banner row target set by row activation (D4); consumed by
    /// the next snooze/mute dispatch, then cleared (one-shot).
    reminder_target: Option<ReminderTarget>,
    /// The notification surface's engaged cursor (US-6/W-6, D-N1):
    /// `None` = disengaged; `Some(i)` = armed by the surface's first
    /// walk or verb stroke with row `i` selected (clamped to the row
    /// count at every pass; the last row's exit or rows emptying
    /// disarms).
    notification_cursor: Option<usize>,
    /// The board-bar segment's board (design §3.8); consumed by the
    /// next `board.select` dispatch, then cleared (one-shot, the
    /// `reminder_target` precedent).
    board_target: Option<BoardId>,
    /// The rule-list row's rule (chunk 6, D13); consumed by the next
    /// `reminder.rule-remove` dispatch, then cleared (one-shot, the
    /// banner-verb precedent).
    rule_target: Option<ReminderRuleId>,
    /// The rule-list add entry's parsed timing input (chunk 6, D13;
    /// r4 D30 widens it to the three-form grammar); consumed by the
    /// next `reminder.rule-add` dispatch, then cleared.
    rule_add_when: Option<crate::presentation::RuleWhen>,
    /// The timers form's parsed draft (DR-3/3, D-P10); consumed by the
    /// next `timer.create` dispatch, then cleared (one-shot).
    timer_draft: Option<crate::router::TimerDraft>,
    /// The timers row's alarm (DR-3/3, D-I8); consumed by the next
    /// `timer.cancel` dispatch, then cleared (one-shot).
    timer_target: Option<taskboard_domain::AdhocAlarmId>,
    /// The break-rule form's parsed draft (T-14, #797); consumed by
    /// the next `breakrule.create` dispatch, then cleared (one-shot).
    break_rule_draft: Option<crate::router::BreakRuleDraft>,
    /// The armed break-rule row's one-shot target (T-14, #797);
    /// consumed by the confirm dispatch, then cleared.
    break_rule_target: Option<taskboard_domain::BreakRuleId>,
    /// The staged remove's armed subject (T-14, the chunk-2
    /// one-armed-subject rule): arming touches no engine state; the
    /// confirm clears it BEFORE the send.
    break_rule_armed: Option<taskboard_domain::BreakRuleId>,
    /// The break-rule create form's open flag (pure presentation).
    break_rule_form_open: bool,
    /// The journal ledger's editor facts (T-15, #798): the active
    /// subject and the staged delete's armed subject (the one-armed
    /// rule), the editor's staged buffers, and the flag delta relative
    /// to the subject's live set (the commit folds them into
    /// `flags_add`/`flags_remove` — the mask, never whole-set
    /// replace).
    reflection_active: Option<taskboard_domain::ReflectionId>,
    reflection_armed: Option<taskboard_domain::ReflectionId>,
    reflection_text_draft: String,
    reflection_mood_draft: String,
    reflection_energy_draft: String,
    reflection_flags_add: BTreeSet<taskboard_domain::HealthFlag>,
    reflection_flags_remove: BTreeSet<taskboard_domain::HealthFlag>,
    /// The break-rule form's threshold staged buffer (the
    /// `timer_when_draft` convention).
    break_rule_threshold_draft: String,
    /// The entries form's parsed draft (r4 chunk 10, D-TS6); filled at
    /// the commit seam only when the live validity table passes,
    /// consumed by the next `-log`/`-adjust` dispatch (one-shot).
    track_entry_draft: Option<crate::presentation::TrackEntryDraft>,
    /// The armed entry row's one-shot target (edit seed + delete
    /// confirm); consumed by the next `-delete` dispatch and cleared
    /// on overlay mutations.
    track_entry_target: Option<taskboard_domain::TimeEntryId>,
    /// The notification row's task (DR-5/4, D-TU5); consumed by the
    /// next `task.track-start` dispatch, then cleared (one-shot, the
    /// timer-target precedent).
    track_target: Option<taskboard_domain::TaskId>,
    /// The habits row's habit (DR-6/6, D-P13): consumed by the next
    /// `habit.log` dispatch, then cleared (one-shot, the timer-target
    /// precedent).
    /// The planner row's task (#622, D-6): set by the adapter when a
    /// planner row affordance activates; the placement/reorder verbs
    /// route from it (the one-shot target pattern).
    plan_target: Option<TaskId>,
    habit_target: Option<taskboard_domain::HabitId>,
    /// The habit section's focus fact (cross-surface friction plan
    /// D-2, #832): the row the menu verbs address — the keyboard twin
    /// of the pointer seams' index resolution. Minted by the row's
    /// activation seam and refreshed by every row affordance click
    /// (click ≡ key composes both directions); a stale index is
    /// absorbed by the minting seam, never by this field's readers.
    focused_habit: Option<taskboard_domain::HabitId>,
    /// The −1 recovery's resolved `(habit, current window, current
    /// count)` (UX R-2 D-3, #786): from the same row source the
    /// section renders; consumed by the next `habit.unlog` dispatch,
    /// then cleared (one-shot).
    habit_undo: Option<(
        taskboard_domain::HabitId,
        taskboard_domain::WindowStart,
        u32,
    )>,
    /// The reorder's resolved (habit, target gap index) (#1198 chunk
    /// A1 D1 — the one-shot's semantic is a gap index into the
    /// section's live sequence; the engine resolves the orders):
    /// consumed by the next `habit.move` dispatch, then cleared.
    habit_move: Option<(taskboard_domain::HabitId, usize)>,
    /// The habit menu's reorder pair (H2 D4): both slots' target
    /// indices minted at menu open from the same row source the
    /// section renders (`presentation::habit_move_slots`); consumed by
    /// the next `habit.move-up`/`habit.move-down` dispatch, then
    /// cleared.
    habit_move_up: Option<(taskboard_domain::HabitId, usize)>,
    habit_move_down: Option<(taskboard_domain::HabitId, usize)>,
    /// The Today compact-move pair's one-shot subject (US-14, #1031):
    /// minted at the R-8 kind resolution (the shared Shift+Up/Down
    /// stroke flipped to its task twins under a task walk landing),
    /// consumed by the dispatch, then cleared.
    task_move: Option<taskboard_domain::TaskId>,
    /// The open object menu's subject (H2 D1): `RowSubject::Habit` for
    /// the habit context menu, `RowSubject::Task` for every task
    /// card's menu — the arm/cancel effects' re-projection reads it
    /// (the menu is built once at open, the D-1 pin; the re-projection
    /// after an arm is the one sanctioned rebuild).
    menu_row_subject: Option<crate::presentation::RowSubject>,
    /// The make-up valve's resolved `(habit, window, count)` (issue
    /// #506, plan D-7): presentation's `habit_repair_target` computed
    /// from the same row source the section renders; consumed by the
    /// next `habit.repair` dispatch, then cleared (one-shot).
    habit_repair: Option<(
        taskboard_domain::HabitId,
        taskboard_domain::WindowStart,
        u32,
    )>,
    /// The armed conversion row's habit (issue #506): consumed by the
    /// next `habit.convert-to-series` dispatch, then cleared.
    habit_convert_target: Option<taskboard_domain::HabitId>,
    /// The conversion's pattern draft (issue #506, D-2): the quota's
    /// default mapping computed at the arm seam; consumed by the
    /// confirm dispatch.
    pattern_draft: Option<taskboard_domain::RecurPattern>,
    /// The recur row's subject task (issue #506): consumed by the
    /// next `series.*` dispatch, then cleared (one-shot).
    series_target: Option<taskboard_domain::TaskId>,
    /// The pause editor's parsed inclusive date: consumed by the next
    /// `series.pause-commit` dispatch, then cleared.
    pause_draft: Option<chrono::NaiveDate>,
    /// The conversion's quota draft (issue #506, D-2): the pattern's
    /// default mapping computed at the arm seam; consumed by the
    /// confirm dispatch.
    convert_target: Option<taskboard_domain::HabitTarget>,
    /// The recur editor's one-shot subject (round-7 D-7.1, D-C1):
    /// armed by the opener effect, consumed by the commit/detach
    /// dispatches, cleared with the flow (the buffers-never-outlive-
    /// the-flow rule).
    recur_edit_target: Option<taskboard_domain::TaskId>,
    /// The recur editor's minted draft (round-7 D-7.1): the commit
    /// funnel re-mints the whole buffer and lands the rule here just
    /// before routing; consumed by the commit dispatch.
    recur_draft_rule: Option<taskboard_domain::RecurRule>,
    /// The recur row's adapter-owned flags (issue #506, plan D-8):
    /// the inline pause editor and the armed confirm strip — cleared
    /// when the subject stops recurring (the buffers-never-outlive-
    /// the-flow rule).
    recur_pause_open: bool,
    recur_confirm_armed: bool,
    /// The board-reconcile preview's pending oneshot (OP1 US-3, D-4):
    /// the armed subject's board plus the receiver — drained under the
    /// repaint cadence; a settled reply routes into the flow's
    /// `ReconcilePreviewState`. Cleared with the flow (the
    /// buffers-never-outlive rule).
    reconcile_preview: Option<(
        taskboard_domain::BoardId,
        tokio::sync::oneshot::Receiver<Option<taskboard_domain::ReconcileCounts>>,
    )>,
    /// The labels overlay's in-place form (UX R-3 D-2/D-5, #789):
    /// the create (`seed: None`) / edit (`Some`) draft — the adapter
    /// owns it across refreshes (the due buffers' pattern); cleared
    /// on every pass where the overlay flow is gone (the
    /// buffers-never-outlive rule).
    label_form: Option<LabelFormState>,
    /// The armed delete row (UX R-3 D-2, #789): the label id the
    /// consequence-naming strip renders for — `DeleteLabel` is
    /// `NotUndoable(RestoreClass)`, so staging is the r2 grammar's
    /// requirement, not a courtesy. One armed row; re-arming swaps.
    label_delete_armed: Option<taskboard_domain::LabelId>,
    /// The effort row's inline editor (US-3/D-8, #838): `Some` while
    /// armed — one editor at a time (the detail exclusivity posture).
    pub(crate) effort_editor: Option<EffortEditorState>,
    /// The priority pickers' adapter-owned flag (UX R-3 D-1, #788):
    /// which detail row's quick-list holds — one at a time; cleared
    /// on Escape/outside click, on any leaving settle, and when the
    /// subject dies (the recur flags' buffers-never-outlive rule).
    priority_open: Option<crate::router::PriorityField>,
    /// The habits section's armed conversion subject (issue #506;
    /// H2 D4 reworked): the armed HABIT — the menu's confirm row
    /// enables from it and the re-projection flips it — `None` =
    /// disarmed. One subject armed at a time; opening another
    /// subject's menu disarms.
    habit_convert_armed: Option<taskboard_domain::HabitId>,
    /// The staged habit remove's armed subject (H4 D4, #944): the
    /// task-delete/`BreakRuleRemove` grammar verbatim — the arm
    /// stages adapter-side, the confirm routes `DeleteHabit`.
    habit_remove_armed: Option<taskboard_domain::HabitId>,
    /// The undo-family restore's armed subject (W-3): the roster
    /// habit the open menu's confirm would resurrect; `take()`n by
    /// the confirm's one-shot collection.
    habit_restore_armed: Option<taskboard_domain::HabitId>,
    /// The labels overlay's armed restore subject (W-2): the roster
    /// row the strip's Confirm would resurrect.
    label_restore_armed: Option<taskboard_domain::LabelId>,
    /// The track panel's armed restore target (W-9): the journal id
    /// the strip's Confirm dispatches `RestoreTimeEntryFromJournal`
    /// over (the zero-input payload carries the id only).
    entry_restore_armed: Option<taskboard_domain::JournalId>,
    /// The rule list's armed clear (W-4): subject-scoped (the armed
    /// flag survives only while the same task stays the editor's
    /// subject and keeps ≥1 rule).
    rules_clear_armed: bool,
    /// The habit editor's staged draft (H4 D5): the field mirrors the
    /// editor rows push; dies with its subject (the editor's
    /// escape/cancel discipline — no draft persistence).
    habit_edit_draft: Option<crate::router::HabitEditDraft>,
    /// The create form's armed flag (round-7 D-7.2): the inline form
    /// mounts below the section header while true — adapter-owned,
    /// pushed per pass (the armed-strip doctrine; no transient UI
    /// state in Slint).
    habit_create_open: bool,
    /// The pause editor's armed flag (HF1, W-3): the inline editor
    /// mounts below the section header while true — adapter-owned,
    /// pushed per pass (the create-form's doctrine).
    habit_pause_open: bool,
    /// The pause editor's one-shot subject (HF1, W-3): resolved by
    /// the opener, consumed by the commit/resume routes (the
    /// `series_target` twin).
    habit_pause_target: Option<taskboard_domain::HabitId>,
    /// The create form's one-shot commit payload (round-7 D-7.2):
    /// minted at the confirm seam, consumed by the routed
    /// `habit.create-commit` dispatch.
    habit_create_draft: Option<crate::router::HabitCreateDraft>,
    /// The create form's quota field draft (round-7 D-7.2, the
    /// `timer_when_draft` twin): the per-keystroke validity pass
    /// reads it; commit and disarm clear it.
    habit_create_quota: String,
    /// The quota popover's stepper draft (W-5, #961): the count the
    /// +/− rungs edit, seeded at the stored quota on open. `0` is the
    /// typed invalid draft (the `NonZeroCount` mint's law surfaced at
    /// the UI boundary — commit stays blocked, no route); negative
    /// values are unreachable (the stepper's floor).
    habit_quota_count: i32,
    /// The quota popover's one-shot commit payload (W-5): minted at
    /// the commit seam (the trust boundary — the `NonZeroCount` mint),
    /// consumed by the routed `habit.quota-commit` dispatch.
    habit_quota_draft: Option<crate::router::HabitQuotaDraft>,
    /// The one-shot focus hand-over (round-3 D10, r4 D25): the phase
    /// step that owns the next push's focus — `None` keeps the status
    /// quo, `Grid` hands focus to the overlay's grid (the
    /// Down-in-field stroke, the Escape step-back), `Field` hands it
    /// back to the armed field (the grid's Enter). Mutually
    /// exclusive by construction — one hand-over per pass.
    focus_handover: FocusHandover,
    /// The one-shot focus-origin fact (the focus-restoration plan
    /// US-1/W-1, D-F1): `Some` from the census site's open pass until
    /// the site closes and the restore fires — one shot, last-open-
    /// wins. `None` = every exit keeps today's view-scope letter.
    focus_origin: Option<FocusOrigin>,
    /// The outcomes-application guard (US-1/W-1): while `true`, the
    /// sync funnel neither records nor restores — the pass's
    /// `flow_ended` tail owns the restore so the fact push lands
    /// *after* the outcomes' model push (a mid-loop push would race
    /// the remounted cell's init/changed edges).
    applying_outcomes: bool,
    /// The quick-list digit filters' armed flags (round-5 F10,
    /// D-3.5): armed by the field's own `edited` keystroke; disarmed
    /// at every open (the quick toggles), every picker-source mirror
    /// write, and the mirrors' reset funnel — the list seeds
    /// unfiltered at open and narrows per keystroke only. The due
    /// editor's latch lives in its level (`DateEditorState`); the
    /// timer setter's is a panel flag.
    setter_filter_armed: bool,
    /// The live keymap preset (round-5 chunk 7): seeded at install,
    /// updated by the keymap reply — the editor command tables'
    /// selection input (`editor_table`), kept in step with `keymap`.
    preset: crate::keymap::Preset,
    /// The editor scope's state (round-5 chunk 7, D-KS3/D4): the
    /// single-slot clipboard (no kill-ring — the documented
    /// leniency; journeys assert the slot through commits, never the
    /// platform), the mark, and the scope-local prefix accumulator.
    /// The mark and the accumulator self-heal (reset whenever the
    /// editor scope is not armed at the doorway) — no registration
    /// can outlive its surface.
    editor_clipboard: Option<String>,
    editor_mark: Option<usize>,
    editor_resolver: crate::keyscope::EditorResolver,
    /// The adapter-tracked cursor (byte offset) and the mirror value
    /// last seen: programmatic moves apply one event pass late in the
    /// widget, so the tracked value is authoritative until the mirror
    /// reports a NATIVE move (a change against the last seen value —
    /// typing, mouse clicks, native arrow keys).
    editor_cursor: usize,
    editor_cursor_seen: usize,
    /// The custom-offset field's typed draft (r4 D27): the validity
    /// probe reads it per pass; the accepted seam clears it (both
    /// outcomes clear the field).
    rule_offset_draft: String,
    /// The timers `when` field's typed draft (r4 D27).
    timer_when_draft: String,
    /// The timers setter popover's custom-row state (r4 D33): the
    /// picked/typed time, the typed buffer (the `TimeField`'s push
    /// re-seed mirror), the day offset, the insistence rung, and the
    /// composed grammar text + fire instant — the quick-fill presets
    /// and the custom row write the same pair. One draft, two views:
    /// the composed text lands in `timer_when_draft`, so the popover
    /// and the inline form share the validity pass.
    setter_time: Option<chrono::NaiveTime>,
    setter_time_text: String,
    setter_day_offset: u32,
    setter_insist: bool,
    setter_composed: Option<String>,
    setter_fire: Option<DateTime<Utc>>,
    /// The last snapshot the derive+diff materialized onto the models
    /// (the unchanged-refresh guard, #463): `Some` after every derived
    /// pass. `Arc::ptr_eq` against the fresh load ⇒ the models already
    /// carry this exact snapshot — the pass's expensive half would be
    /// a no-op. A miss (the engine republished identical values) costs
    /// one wasted pass, never a stale UI.
    last_derived: Option<Arc<AppState>>,
    /// The id tables of the last refresh pass (D2): index → id.
    tables: IdTables,
    /// The persistent model handles of this window (r2 D9): the
    /// refresh pass diffs the desired rows onto them, so unchanged
    /// refreshes notify nothing.
    models: crate::models::PushModels,
    /// The active column drag (r4 D6): the boundary's left and right
    /// stacks plus the pre-drag base width of the left column in
    /// rendered px, captured at press (the press materializes the
    /// resolved row, so the base is the sum-exact anchor). The drag
    /// deltas are absolute since press — idempotent against dropped
    /// events.
    width_drag: Option<(StackId, StackId, f32)>,
    /// The active scale-slider gesture (r2 D29): the pre-gesture
    /// base-scale — the revert point. Mirrors `width_drag`.
    scale_drag: Option<f64>,
    /// The settings view-model (round-1 D7/D8): draft mirror, row
    /// list, liveness overrides, and the pending reply half.
    settings: SettingsVm,
    /// The status panel's VM (OP3 W-1..W-3): the one in-flight
    /// `StatusQuery` reply half (D-3's snapshot-on-open + explicit
    /// refresh — never a polling timer; one query at a time).
    status_panel: StatusPanelVm,
    /// The status line's auto-clear driver (r2 D28): a one-shot slint
    /// timer, mock-clock driven. Lifetime held here alongside the
    /// window; the fire closure carries only weak handles.
    status_timer: Rc<slint::Timer>,
    /// The fire closure the choke point hands to `start` on every
    /// re-arm (built once in `install`, over the weak handles).
    status_fire: Rc<dyn Fn()>,
    /// The kiosk display-mode rotation (K13, issue #363): the pure
    /// state machine plus the hand-over's dwell. `None` on desktop —
    /// the byte-identical guarantee is a type, not a gate.
    /// The status bar's transient resurrect notice (#130): the
    /// composed line plus its expiry instant. Adapter-held
    /// presentation state (the liveness-map precedent); expiry rides
    /// the cadence tick's wall clock — no UI-held timer.
    notice: Option<Notice>,
    /// The `ConfigReload` lane's typed presentation (W-5, R-3): the
    /// last broadcast outcome's fold; `Quiet` until one lands. The
    /// lane is read-only forever — nothing here dispatches.
    reload_lane: crate::presentation::ConfigReloadPresentation,
    /// The degraded-boot badge's persistent flag (W-7, R-5): set by
    /// the first `JournalDegraded` broadcast, never cleared in-session
    /// (display-only; the next healthy boot's fresh session starts
    /// clean).
    journal_degraded: bool,
    /// The OS-delivery health lane (round-3 chunk 7, D31): the watch
    /// receiver the app hands over at mount; drained non-blocking
    /// beside the two broadcasts — a failure repaints within one
    /// cadence tick, no new thread or task in the UI.
    delivery_health: tokio::sync::watch::Receiver<crate::presentation::DeliveryHealth>,
    /// The files-sync chip lane (DR-8 chunk 4, D-FH5): `None` when the
    /// substrate actor is off — the chip stays hidden.
    files_sync: Option<tokio::sync::watch::Receiver<crate::presentation::FilesSyncChip>>,
    /// The export dialog's config defaults (the J2 export plan D-2/D-3):
    /// the hand-over's minted postures the dialog VM seeds from.
    export_defaults: crate::requests::ExportDialogDefaults,
    /// The export dialog's VM (W-1..W-5): `None` = unmounted — the
    /// mount bit the window's `export-dialog-open` flag mirrors. A
    /// kiosk never seeds it (R-2).
    export_vm: Option<crate::export_dialog::ExportVm>,
    /// The pending export run's reply half (W-4/W-5): `Some` between
    /// the Run dispatch and the settled reply; the close seam DROPS it
    /// (US-2/S2 — the app-side run completes or refuses
    /// independently, the drain consumes the reply). The install
    /// drain and the cadence tick poll it non-blocking.
    export_reply: Option<oneshot::Receiver<crate::requests::ExportRunOutcome>>,
    /// The settled outcome's stash between the reply poll and the
    /// `export-outcome-shown` fold (the re-entrancy law above).
    export_settled: Option<crate::requests::ExportRunOutcome>,
    /// The in-flight preview's reply half (the D2 preview plan W-1):
    /// `Some` between the Preview dispatch and the settled reply; the
    /// close seam drops it (the D-7 escape law, the export-reply
    /// twin).
    export_preview_reply:
        Option<oneshot::Receiver<crate::requests::ExportPreviewOutcome>>,
    /// The settled preview's stash between the poll and the
    /// `export_preview_shown` fold (the re-entrancy law).
    export_preview_settled: Option<crate::requests::ExportPreviewOutcome>,
    /// The in-flight clipboard copy's reply half (the preview plan
    /// W-2): the copy twin of the preview lane.
    export_copy_reply:
        Option<oneshot::Receiver<crate::requests::ExportClipboardOutcome>>,
    /// The settled copy's stash between the poll and the fold.
    export_copy_settled: Option<crate::requests::ExportClipboardOutcome>,
    /// The settled remediation outcome's stash between the reply poll
    /// and the `status_remediation_outcome_shown` fold (the same
    /// re-entrancy law).
    remediation_settled: Option<crate::requests::StatusRemediationOutcome>,
    rotation: Option<crate::view_state::KioskRotation>,
    /// The rotation flip's single-shot dwell timer (K13). Armed by
    /// `install` only when `rotation` is present, re-armed by the
    /// fire closure with the full dwell (the timer IS the dwell
    /// state). Lifetime held here alongside the window.
    #[allow(dead_code)]
    rotation_timer: Rc<slint::Timer>,
    /// The fire closure `rearm_rotation` hands to `start` on every
    /// re-arm (built once in `install`, over the weak handles).
    rotation_fire: Rc<dyn Fn()>,
    /// The exit-effect counter (the §6 component-test seam: journeys
    /// assert the quit request without killing the platform).
    pub(crate) quit_requests: usize,
    /// The last geometry the capture emitted a request for (#412):
    /// the dedupe baseline — an unchanged window re-samples every
    /// cadence tick but requests nothing.
    last_window_geometry: Option<crate::geometry::WindowGeometry>,
    /// The test seam (#463's pin): derive+diff passes executed since
    /// install. The guard's contract in one integer — incremented only
    /// where `models::push` is about to run.
    #[cfg(test)]
    pub(crate) derive_passes: usize,
    /// The control walk's re-entrancy flag (round-3 D25): held while
    /// the synthetic traversal event is in flight, so the injected
    /// `Tab` rejects out of `intercept-key` before the callback even
    /// borrows (the runtime then consumes it for traversal itself).
    /// Shared with the intercept closure — the guard must precede the
    /// `borrow_mut`, not live inside it.
    pub(crate) walk_reentry: Rc<std::cell::Cell<bool>>,
    /// The control walk's traversal one-shot (r4 chunk 6): the
    /// synthetic Tab cannot be dispatched re-entrantly from inside the
    /// key callback — the nested event is inert (the recorded probe) —
    /// so the re-injection defers to the next event-loop turn. Held
    /// because a dropped [`slint::Timer`] cancels itself.
    walk_timer: slint::Timer,
}

impl Adapter {
    /// The menu greying map (round-3 D27c): the enumerated
    /// state-dependent rows only — everything else keeps
    /// dispatch-time `NoTarget` machinery (the closed set; R-E).
    /// Built at menu open and in the models pass while the menu
    /// holds, so engine syncs re-grey a live menu.
    pub(crate) fn entry_states(
        &self,
        state: &taskboard_domain::AppState,
        flow: &Flow,
    ) -> BTreeMap<ActionId, bool> {
        use crate::actions::ActionId;
        let focused = self.view_state.focused_task();
        let focused_done = focused
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.done.is_some());
        let move_flow_live = matches!(
            flow,
            Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. }
        );
        let tracking_live = crate::presentation::tracking_chip(state).is_some();
        let subject = self.view_state.subject_task();
        BTreeMap::from([
            (ActionId::TaskMoveCancel, move_flow_live),
            (ActionId::TaskComplete, focused.is_some() && !focused_done),
            (ActionId::TaskReopen, focused.is_some() && focused_done),
            (ActionId::TaskMove, focused.is_some()),
            (ActionId::TaskEdit, focused.is_some()),
            (ActionId::TaskDelete, focused.is_some()),
            (ActionId::TaskTitleEdit, subject.is_some()),
            (
                ActionId::TaskTrackStart,
                focused.is_some() && !tracking_live,
            ),
            (ActionId::TaskTrackStop, tracking_live),
            // The concentration overlay (#496): desktop-only (kiosk's
            // three-layer exclusion, layer 2) and only while focus
            // mode is configured — no timing input, no overlay.
            (
                ActionId::FocusModeToggle,
                !self.kiosk && self.focus.is_some(),
            ),
            // The review walk's opener (#638): desktop-only (layer 2
            // of the exclusion, D-J14 — the `FocusModeToggle` gate).
            (ActionId::ReviewOpen, !self.kiosk),
            // Undo v1 surfaces (undo/2, ADR 0019): availability reads
            // the published `UndoView` (memory-only, derived at every
            // publish) — the whole v1 binding. A `NotUndoable` head
            // still shows `can_undo` (R-U6 refuses at planning, typed).
            (ActionId::AppUndo, state.undo.can_undo),
            (ActionId::AppRedo, state.undo.can_redo),
            // The v2 picker entry (UF2, #353): desktop-only (R-2 — the
            // structural exclusion), live when the line has anything to
            // pick (a reflex row or a branch row).
            (
                ActionId::AppUndoBranches,
                !self.kiosk && (state.undo.can_undo || state.undo.can_redo),
            ),
            // The live pause arm (UX-R8 chunk-5 D-4): the commit row
            // greys unless the pause editor's draft is armed — the
            // TaskMoveCancel flow-grey precedent.
            (ActionId::SeriesPauseCommit, self.pause_draft.is_some()),
        ])
    }

    /// Pushes the shell-derived state one-way onto the window
    /// globals (D18); nothing renders it in chunk 2 — chunk 3's
    /// status bar reads these.
    fn sync(&self, window: &AppWindow) {
        window.set_palette_open(self.shell.palette);
        // The capture overlay's window bit (chunk 10 D8): from the
        // view-state flow, the palette precedent.
        let capture = matches!(self.view_state.flow(), crate::view_state::Flow::Capture);
        window.set_capture_open(capture);
        // The field seeds at overlay init only (the rename-field rule):
        // the buffer rides while the flow holds, the slot clears on
        // close — a refresh never rewrites a live field's text (D9
        // rule 4).
        window.set_capture_initial_text(if capture {
            SharedString::from(self.view_state.buffer().to_owned())
        } else {
            SharedString::from(String::new())
        });
        // The recur editor's window-level mirrors (round-7 D-7.1): the
        // overlay mounts exactly while the flow holds; the model
        // re-seeds from the level's buffer every pass (the TimeField
        // contract — the widget never holds state).
        let recur_open = matches!(self.view_state.flow(), Flow::EditRecur { .. });
        window.set_recur_editor_open(recur_open);
        if recur_open {
            window.set_recur_editor_model(self.recur_editor_model());
        }
        // The export dialog's window-level mirrors (the J2 export
        // plan W-1..W-5): the mount bit rides the VM; the model
        // re-seeds every pass (the recur-editor contract — the
        // widget never holds state).
        let export_open = self.export_vm.is_some();
        window.set_export_dialog_mounted(export_open);
        if export_open {
            window.set_export_model(self.export_dialog_model());
        }
        let palette_selected = match self.view_state.flow() {
            crate::view_state::Flow::Palette { selected } => *selected,
            _ => 0,
        };
        window.set_palette_selected_index(i32::try_from(palette_selected).unwrap_or(i32::MAX));
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_vi_insert(self.shell.mode == crate::router::Mode::Insert);
        keyboard.set_menu_open(self.shell.menu.open);
        // The undo branch picker's mount bit (UF2, #353); the models
        // ride the open seam (a refresh never rewrites a live picker —
        // the stale-row refusal is the typed contract, W-6).
        keyboard.set_undo_picker_open(self.shell.undo_picker);
        // The picker grid's context mirror (round-3 D25): the harness
        // chain and the markup read the same fact.
        keyboard.set_date_picker_grid(self.shell.date_picker_grid);
        keyboard
            .set_menu_selected_index(i32::try_from(self.shell.menu.selected).unwrap_or(i32::MAX));
        keyboard.set_menu_root_selected_index(
            i32::try_from(self.shell.menu.root_selected).unwrap_or(i32::MAX),
        );
        match self.menu_anchor {
            MenuAnchor::Hamburger => {
                keyboard.set_menu_pointer_anchored(false);
            }
            MenuAnchor::Pointer(x, y) => {
                keyboard.set_menu_pointer_anchored(true);
                keyboard.set_menu_pointer_x(x);
                keyboard.set_menu_pointer_y(y);
            }
        }
        // The active page's parent-row index (round-1 D6; UX-R8 D-5
        // widens the shape): the fly-out/replace machinery keys off
        // this int for both the main groups and the task families —
        // the presentation reads the position, never the kind.
        keyboard.set_menu_active_group(match self.active_submenu() {
            None => -1,
            Some(sub) => self
                .root_entries
                .iter()
                .position(|entry| entry.submenu == Some(sub))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        });
    }

    /// The open submenu's target (UX-R8 chunk-5 D-5): the main
    /// group's page or the task family's — the ladder and the
    /// parent-row position sync read it.
    fn active_submenu(&self) -> Option<crate::menu::Submenu> {
        match (self.shell.menu.active_group, self.shell.menu.task_group) {
            (Some(group), _) => Some(crate::menu::Submenu::Group(group)),
            (None, Some(family)) => Some(crate::menu::Submenu::Task(family)),
            (None, None) => None,
        }
    }

    /// The undo branch picker's row models (UF2, #353 W-2/W-3): the
    /// typed projections pushed one-way onto the window global —
    /// sibling branches from `UndoView.branch_children`, undo-step
    /// rows from the `recent` window. Rows carry the journal entry id
    /// (string form across the seam; re-typed at the trust boundary)
    /// and the engine-derived label. A close empties the bit; the
    /// models stay (a refresh never rewrites a live picker — the
    /// stale-row refusal is the typed contract, W-6).
    fn sync_undo_picker_models(&self, window: &AppWindow) {
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_undo_picker_open(self.shell.undo_picker);
        if !self.shell.undo_picker {
            return;
        }
        let undo = &self.state.load().undo;
        let rows = |labels: &[taskboard_domain::UndoLabel]| -> Vec<crate::UndoPickerRowModel> {
            labels
                .iter()
                .map(|row| crate::UndoPickerRowModel {
                    id: row.id.as_uuid().to_string().into(),
                    label: row.label.as_str().into(),
                })
                .collect()
        };
        keyboard.set_undo_picker_branch_rows(ModelRc::new(slint::VecModel::from(rows(
            &undo.branch_children,
        ))));
        keyboard
            .set_undo_picker_target_rows(ModelRc::new(slint::VecModel::from(rows(&undo.recent))));
    }

    /// The two-tier menu models (round-1 D6): the root page model
    /// stays mounted for the whole open lifecycle; the submenu model
    /// carries the active group's page (empty at the root).
    fn sync_menu_models(&self, window: &AppWindow) {
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_menu_entries(Self::menu_model(&self.root_entries));
        keyboard.set_menu_submenu_entries(Self::menu_model(&self.menu_entries));
        keyboard.set_menu_quick_bar(Self::quick_bar_model(
            &self.quick_bar_entries,
            &self.keymap,
            &self.menu_chain,
        ));
    }

    /// The quick bar as the slint model rows (UX-R8 chunk-5 D-2): the
    /// Rust icon vocabulary maps onto the generated `IconKind` here —
    /// menu.rs stays slint-free. Each row carries its action's first
    /// chain-visible binding (J5 US-4 AC-1) — the same projection the
    /// menu rows resolve through `row()`; empty when unbound.
    fn quick_bar_model(
        entries: &[crate::menu::QuickBarEntry],
        keymap: &crate::keymap::Keymap,
        chain: &[ContextId],
    ) -> ModelRc<crate::QuickBarEntryModel> {
        ModelRc::from(
            entries
                .iter()
                .map(|entry| {
                    let hint = keymap
                        .first_binding_display(entry.action, chain)
                        .unwrap_or_default();
                    crate::QuickBarEntryModel {
                        action: entry.action.id().to_owned().into(),
                        // The registry label — the element's address / the
                        // a11y label (D16).
                        label: REGISTRY
                            .iter()
                            .find(|row| row.id == entry.action)
                            .map_or("", |row| row.label)
                            .into(),
                        icon: match entry.icon {
                            crate::menu::QuickBarIcon::Calendar => crate::IconKind::Calendar,
                            crate::menu::QuickBarIcon::CalendarPlus => {
                                crate::IconKind::CalendarPlus
                            }
                            crate::menu::QuickBarIcon::CalendarMinus => {
                                crate::IconKind::CalendarMinus
                            }
                            crate::menu::QuickBarIcon::Play => crate::IconKind::Play,
                            crate::menu::QuickBarIcon::Stop => crate::IconKind::Stop,
                        },
                        enabled: entry.enabled,
                        visible: entry.visible,
                        hint_keys: ModelRc::new(slint::VecModel::from(
                            hint.iter()
                                .map(|stroke| SharedString::from(stroke.as_str()))
                                .collect::<Vec<_>>(),
                        )),
                    }
                })
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    /// One page as the slint model rows.
    fn menu_model(entries: &[MenuEntry]) -> ModelRc<crate::MenuEntryModel> {
        ModelRc::from(
            entries
                .iter()
                .map(|entry| crate::MenuEntryModel {
                    action: entry
                        .action
                        .map_or(String::new(), |id| id.id().to_owned())
                        .into(),
                    label: entry.label.into(),
                    description: entry.description.into(),
                    enabled: entry.enabled,
                    submenu: entry.submenu.is_some(),
                    hint: ModelRc::new(slint::VecModel::from(
                        entry
                            .hint
                            .iter()
                            .map(|stroke| SharedString::from(stroke.as_str()))
                            .collect::<Vec<_>>(),
                    )),
                })
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    /// The capture-phase entry point (D18): slint event → `Stroke` →
    /// resolver. Accepts iff the resolver consumed the stroke;
    /// `Unhandled` rejects so delivery continues (Tab traversal).
    ///
    /// The D20 pulls run here, before the resolver feed, because every
    /// keystroke re-reads the editing-facts singleton (Slint globals
    /// do not notify Rust — pull-at-intercept is self-healing by
    /// construction): a focused text field engages insert mode, a
    /// fieldless insert disengages — and never under an armed flow or
    /// the picker grid: the flow outcomes own their mode transitions
    /// (a flow arms Insert before its field mounts and holds it to the
    /// settle, the grid phase holds it while the *field* lost focus to
    /// the grid; flipping Normal there would hand Escape to vi's
    /// move-cancel instead of the clean close). The engage pull's
    /// idempotence against the flow outcomes is the convergence.
    #[allow(clippy::too_many_lines)] // the resolver's dispatch arm + the scope arms, kept flat
    fn intercept(
        &mut self,
        window: &AppWindow,
        text: &SharedString,
        modifiers: KeyboardModifiers,
    ) -> bool {
        if self.walk_reentry.get() {
            return false;
        }
        let Some(stroke) = to_stroke(text, modifiers) else {
            return false;
        };
        {
            let facts = window.global::<crate::EditingFacts>();
            let text_focus = facts.get_text_focus_count();
            if text_focus > 0 && self.shell.mode != Mode::Insert {
                self.shell.apply(&UiEffect::SetMode(Mode::Insert), 0);
                self.sync(window);
            } else if text_focus == 0
                && self.shell.mode == Mode::Insert
                && !self.shell.date_picker_grid
                && matches!(self.view_state.flow(), Flow::None)
            {
                self.shell.apply(&UiEffect::SetMode(Mode::Normal), 0);
                self.sync(window);
            }
        }
        // The printable monopoly (round-5 chunk 7, D-KS2): while a
        // text field holds slint focus, it claims every bare
        // printable — and Space, printable text — outright, ahead of
        // every scope arm and the resolver. Chord-class strokes pass
        // to the scope stack: the field-phase picker scope serves its
        // navigation strokes mid-typing (F11) while the field keeps
        // every printable.
        {
            let facts = window.global::<crate::EditingFacts>();
            if crate::keyscope::monopoly_declines(
                self.shell.mode,
                usize::try_from(facts.get_text_focus_count()).unwrap_or(usize::MAX),
                self.resolver.has_pending(),
                stroke,
            ) {
                Self::set_feedback(window, KeyFeedback::None);
                return false;
            }
        }
        // The editor scope's table (D-δ2/D-KS3), then the retired
        // special cases' scope arms (D-KS4) — each ends the intercept
        // when it claims the stroke.
        if let Some(consumed) = self.editor_scope_arm(window, stroke) {
            return consumed;
        }
        if let Some(consumed) = self.special_scope_arms(window, stroke) {
            return consumed;
        }
        // The chain reflects the view-state machine, not a stale seam
        // write (D6: shell view/focus derive from view_state). The
        // key doorway arms the scope deltas (D-δ1 with its
        // Return/Escape exemption; D-δ2 comes with the editor scope).
        self.sync_view_context(window);
        let chain = self.chain_with(window, Some(stroke), true);
        // The activation reservation (special case C, r4 §2 D18):
        // while a task-view activation stop holds focus, bare
        // Return/Space belong to the focused cell — the resolver is
        // still fed first (prefix semantics preserved: a pending
        // prefix aborts on Return exactly as now), but an `Action`
        // answer or `Unhandled` yields, and the focused cell's
        // `key-pressed` activates. The modal surfaces reset the fact
        // anyway; the check is the belt.
        let reserved = {
            let facts = window.global::<crate::EditingFacts>();
            crate::keyscope::activation_reserved(
                facts.get_activation_focus_count() > 0,
                self.shell.menu.open,
                self.shell.palette,
                stroke,
            )
        };
        match self.resolver.feed(stroke, &chain, &self.keymap) {
            Resolution::Action(id) => {
                if reserved {
                    Self::set_feedback(window, KeyFeedback::None);
                    return false;
                }
                Self::set_feedback(window, KeyFeedback::None);
                // UX-R8 D-4's shift-walk extension: the named-key walk
                // rows with shift held extend the selection instead of
                // the plain focus step (the board surface; other
                // surfaces keep the plain step — R-1's scoping). The
                // printable-bound rows fold shift away (the recorded
                // leniency: arrows only).
                if stroke.shift
                    && matches!(id, ActionId::NavNextItem | ActionId::NavPrevItem)
                    && self.view_state.view() == crate::view_state::ViewId::Board
                {
                    let before = self.view_state.focused_task();
                    let state = self.state.load_full();
                    let direction = if id == ActionId::NavNextItem { 1 } else { -1 };
                    let outcomes = self.view_state.extend_selection_walk(&state, direction);
                    self.apply_outcomes(window, &outcomes);
                    self.push_focus_scroll_hint(window, before);
                    return true;
                }
                self.route(window, id);
                true
            }
            Resolution::Pending => {
                Self::set_feedback(window, KeyFeedback::PrefixPending);
                true
            }
            Resolution::Aborted => {
                Self::set_feedback(window, KeyFeedback::PrefixAbort);
                true
            }
            Resolution::Unhandled => {
                // The notification surface's engagement arm (US-6/W-6,
                // D-N1, the regression-bar revision): the grammar arms
                // the surface only as a FALLBACK — the stroke found no
                // binding in the visible chain (a dead key), so the
                // ambient claim wins everywhere the chain binds the
                // key first (the focus overlay's walk regression is
                // the recorded bar). Down/Up engage on the unbound
                // chains; the verb keys engage everywhere they are
                // dead.
                if !self.shell.notification_surface
                    && !self.tables.notifications.is_empty()
                    && let Some(gram) = crate::keyscope::notification_grammar_stroke(&stroke)
                {
                    match gram {
                        crate::keyscope::NotificationGrammar::Walk(direction) => {
                            Self::set_feedback(window, KeyFeedback::None);
                            self.notification_walk(window, direction);
                        }
                        crate::keyscope::NotificationGrammar::Verb(verb) => {
                            Self::set_feedback(window, KeyFeedback::None);
                            self.apply_notification_verb(window, verb);
                        }
                    }
                    return true;
                }
                Self::set_feedback(window, KeyFeedback::None);
                false
            }
        }
    }

    /// The editor scope's command executor (round-5 chunk 7, D-KS3):
    /// the pure arithmetic (`keyscope::apply_editor_command`) over the
    /// widget's mirrored buffer and the tracked cursor; the widget
    /// applies the outcome through the `EditorScope` seam (Rust never
    /// touches `TextInput` internals). Cut/copy/paste ride the single
    /// adapter slot — never the OS clipboard.
    fn editor_command(
        &mut self,
        window: &AppWindow,
        command: crate::keyscope::EditorCommand,
    ) -> bool {
        use crate::keyscope::EditorCommand;
        // The discard and the vi re-insert ride the existing mode
        // funnel (the same outcome as Escape today — no new settle).
        match command {
            EditorCommand::Discard => {
                self.apply_effect(window, UiEffect::SetMode(Mode::Normal));
                return true;
            }
            EditorCommand::EnterInsert => {
                self.apply_effect(window, UiEffect::SetMode(Mode::Insert));
                return true;
            }
            _ => {}
        }
        let scope = window.global::<crate::EditorScope>();
        let buffer = scope.get_buffer_mirror().to_string();
        let mirror = usize::try_from(scope.get_cursor_offset()).unwrap_or(0);
        // The mirror lags the widget's programmatic moves by an event
        // pass: adopt it only when it moved against BOTH the last
        // seen value and the tracked cursor (a genuine native move —
        // typing, mouse, native arrows); a lagging write equals one of
        // the two and yields to the tracked cursor.
        let cursor = if mirror != self.editor_cursor_seen && mirror != self.editor_cursor {
            mirror
        } else {
            self.editor_cursor
        };
        let edit = crate::keyscope::apply_editor_command(
            &crate::keyscope::EditorState {
                buffer: &buffer,
                cursor,
                mark: self.editor_mark,
                clipboard: self.editor_clipboard.as_deref(),
            },
            command,
        );
        self.editor_mark = edit.mark;
        self.editor_clipboard = edit.clipboard;
        scope.set_anchor_offset(i32::try_from(edit.mark.unwrap_or(edit.cursor)).unwrap_or(-1));
        scope.set_command_replaces(edit.text.is_some());
        scope.set_command_text(edit.text.unwrap_or_default().into());
        scope.set_command_cursor(i32::try_from(edit.cursor).unwrap_or(-1));
        scope.set_command_code(command.code());
        scope.set_command_generation(scope.get_command_generation() + 1);
        self.editor_cursor = edit.cursor;
        self.editor_cursor_seen = mirror;
        true
    }

    /// The editor scope's table arm (round-5 chunk 7, D-δ2/D-KS3):
    /// consulted before the resolver — but only when the ordinary
    /// resolver holds no pending prefix (the keymap's own prefixes,
    /// emacs' C-x family, own the stroke first, so C-x C-c still
    /// exits inside the editor). `Some(consumed)` ends the intercept.
    /// The mark and the accumulator self-heal whenever the scope is
    /// not armed — no registration outlives its surface.
    fn editor_scope_arm(
        &mut self,
        window: &AppWindow,
        stroke: crate::keymap::Stroke,
    ) -> Option<bool> {
        let editor_armed = matches!(self.view_state.flow(), Flow::EditDescription { .. })
            && window
                .global::<crate::EditingFacts>()
                .get_editor_focus_count()
                > 0;
        if !editor_armed || self.resolver.has_pending() {
            self.editor_resolver.reset();
            if !editor_armed {
                self.editor_mark = None;
            }
            return None;
        }
        let table = crate::keyscope::editor_table(self.preset);
        match self.editor_resolver.feed(stroke, &table) {
            crate::keyscope::EditorClaim::Command(command) => {
                Self::set_feedback(window, KeyFeedback::None);
                Some(self.editor_command(window, command))
            }
            crate::keyscope::EditorClaim::Consumed => {
                Self::set_feedback(window, KeyFeedback::PrefixPending);
                Some(true)
            }
            crate::keyscope::EditorClaim::Pass => None,
        }
    }

    /// The retired special cases' arms (round-5 chunk 7, D-KS4): each
    /// arming fact is the retired condition verbatim (declared in
    /// `keyscope`), each fires where its block fired — after the
    /// monopoly, before the resolver; each pin journey is the
    /// acceptance contract. `Some(consumed)` ends the intercept.
    fn special_scope_arms(
        &mut self,
        window: &AppWindow,
        stroke: crate::keymap::Stroke,
    ) -> Option<bool> {
        // The request-driven switcher flyout's walk (UX R-4 D-4,
        // #794): the flyout armed by the target-free BoardSelect
        // stroke has no focused field instance (the widget's own
        // ladder serves the pointer path), so the identical key
        // vocabulary rides here against the facts — the walk's state
        // seam. Escape closes with nothing dispatched; Return routes
        // the highlighted row's selection (the segment seam's route)
        // and closes. Runs ahead of the ladder-decline block (whose
        // decline would dead-end the keys without a focused field).
        if let Some(consumed) = self.switcher_flyout_key(window, stroke) {
            return Some(consumed);
        }
        // The dropdown ladders (special case D, r4 D39/D40, round-5
        // D6): while a flyout is open, its keys decline the single
        // key path — the focused field's modal arm owns them.
        let ladder_open = if window.global::<crate::SettingsBoardFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::SettingsBoard)
        } else if window.global::<crate::BoardBarFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::BoardBar)
        } else if window.global::<crate::TimeFieldFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::TimeField)
        } else if window.global::<crate::DropdownFacts>().get_open() {
            // The task detail's pickers (round-9 chunk 15 D-7): the
            // class facts global's open bit, LAST — the three legacy
            // globals above win their own flyouts first, so this arm
            // catches only the detail's dropdown ladders.
            Some(crate::keyscope::LadderGlobal::TaskDetailPriority)
        } else {
            None
        };
        if crate::keyscope::dropdown_ladder_declines(ladder_open, stroke) {
            Self::set_feedback(window, KeyFeedback::None);
            return Some(false);
        }
        // The timer setter's dismissal arm (special case A, r4 D33):
        // the panel is presentation state, not a flow — its Escape is
        // an explicit decline routed to the existing dismissal funnel.
        // Nothing commits; the draft clears with the panel.
        if crate::keyscope::timer_setter_dismisses(window.get_timer_setter_open(), stroke) {
            self.clear_timer_setter();
            window.set_timer_setter_open(false);
            self.refresh_forced(window);
            return Some(true);
        }
        // The habit create form's dismissal arm (round-7 D-7.2, the
        // timer setter's twin): bare Escape ahead of the resolver —
        // the armed form is presentation state; nothing commits.
        // (A Today-context Escape binding would statically shadow
        // Global TaskMoveCancel on the whole view — the funnel exists
        // for exactly this shape.)
        if crate::keyscope::habit_create_dismisses(window.get_habits_create_open(), stroke) {
            self.disarm_habit_create(window);
            return Some(true);
        }
        // The pause editor's dismissal arm (D3a W-7, #1208): the
        // create form's exact twin — bare Escape ahead of the
        // resolver. The editor's date field holds the keyboard's
        // insert phase while mounted (the D20 pull), so the global
        // escape/cancel preset is physically unreachable from this
        // surface — the funnel is the action layer's existing shape
        // for exactly this (no second Escape *binding*: the keymap
        // is untouched, the C-5 collision rule holds). The dismissal
        // resolves into the landed habits-pause-cancelled funnel —
        // cancel writes nothing (the destructive-role dismissal law).
        if crate::keyscope::habit_pause_dismisses(window.get_habits_pause_open(), stroke) {
            self.disarm_habit_pause(window);
            return Some(true);
        }
        // The due editor's Down arm (special case B, r2 D25, round-3
        // D10): consumed ahead of the resolver so the insert-mode
        // chain never has to bind it. With the picker already open
        // (the default-open), the same stroke is the grid's focus
        // hand-over — the one-shot flag rides the push, and the
        // panel's `changed` moves focus.
        if crate::keyscope::due_down_arms(
            matches!(self.view_state.flow(), Flow::EditDate { .. }),
            stroke,
        ) {
            let picker_open = self
                .view_state
                .date_editor()
                .is_some_and(|editor| editor.picker.is_some());
            if picker_open {
                // The field→grid hand-over is a programmatic steal —
                // the funnel must not settle on its focus loss (r4 D24).
                if let Some(editor) = self.view_state.date_editor_mut() {
                    editor.focus_steal = true;
                }
                self.focus_handover = FocusHandover::Grid;
                // The one-shot flag is a derive input the push consumes — forced.
                self.refresh_forced(window);
            } else {
                self.open_due_picker(window, true);
            }
            return Some(true);
        }
        None
    }

    /// The create form's disarm funnel (round-7 D-7.2): one path,
    /// three entry points — the toggle's discard branch, the Cancel
    /// seam, the Escape arm. Clears the armed flag, the one-shot
    /// draft, and the quota draft (the buffers never outlive the
    /// flow).
    fn disarm_habit_create(&mut self, window: &AppWindow) {
        self.habit_create_open = false;
        self.habit_create_draft = None;
        self.habit_create_quota.clear();
        window.set_habits_create_open(false);
        self.refresh_forced(window);
    }

    /// The pause editor's disarm funnel (HF1, W-3): one path — the
    /// commit's consumed subject, the Cancel seam, the subject-death
    /// absorb. Clears the armed flag and the one-shot subject (the
    /// create funnel's shape).
    fn disarm_habit_pause(&mut self, window: &AppWindow) {
        self.habit_pause_open = false;
        self.habit_pause_target = None;
        window.set_habits_pause_open(false);
        self.refresh_forced(window);
    }

    /// The quota popover's push (W-5, #961): the stepper draft and its
    /// typed validity ride the window properties — `0` is the invalid
    /// draft (the `NonZeroCount` mint's law at the UI boundary; the
    /// commit stays blocked, no route), every `≥ 1` value is valid.
    fn sync_quota_popover(&self, window: &AppWindow) {
        window.set_habits_quota_count(self.habit_quota_count);
        window.set_habits_quota_valid(self.habit_quota_count >= 1);
    }

    /// The quota popover's disarm (the escape rungs' funnel): the
    /// window flag drops and the one-shot draft dies with it (the
    /// buffers never outlive the flow).
    fn disarm_habit_quota(&mut self, window: &AppWindow) {
        window.set_habits_quota_open(false);
        self.habit_quota_draft = None;
    }

    fn set_feedback(window: &AppWindow, feedback: KeyFeedback) {
        window.global::<KeyboardState>().set_last_feedback(feedback);
    }

    /// The selection verbs' dispatch (UX-R8 D-6): Complete sends one
    /// `SetTaskDone` per member in anchor order (a tracker member's
    /// completion is impossible — the per-task absorb drops it and
    /// continues, the #492 shape); Delete arms the multi strip
    /// (`ConfirmDelete(Tasks)`, the anchor-order snapshot) — the
    /// armed repeat confirms, Escape/a settle disarm (the shared
    /// grammar). The schedule trio (J5 US-2, the multi bar's verbs)
    /// rides the same per-member shape: one sparse `UpdateTask` per
    /// member, the router's single-target masks over anchor order
    /// (the unschedule's per-task absorb skips already-unscheduled
    /// members).
    fn route_selection_verb(&mut self, window: &AppWindow, id: ActionId) {
        let state = self.state.load_full();
        match id {
            ActionId::TaskComplete => {
                let members = self.view_state.selection_anchor_order(&state);
                for member in members {
                    let Some(task) = state.tasks.get(&member) else {
                        continue; // the per-task absorb (the id-table law)
                    };
                    if task.kind == taskboard_domain::entities::TaskKind::Tracker {
                        continue; // the completion is impossible (#492)
                    }
                    self.send(StateCommand::SetTaskDone {
                        id: member,
                        done: true,
                    });
                }
            }
            ActionId::TaskDelete => {
                let members = self.view_state.selection_anchor_order(&state);
                let outcomes = self.view_state.arm_multi_delete(members);
                self.apply_outcomes(window, &outcomes);
            }
            ActionId::TaskScheduleToday => {
                // The batch twin of the router's today stamp (#893):
                // the day's last covered instant over the same frozen
                // day facts — the raw exclusive end is excluded by the
                // `[now, end)` membership the stamped rows are judged
                // by. The domain mint, never a local `end − ε` (#884).
                let end = taskboard_domain::days::last_covered_instant(self.day_ctx().window.1);
                self.schedule_selection(&state, end);
            }
            ActionId::TaskScheduleTomorrow => {
                let end = taskboard_domain::days::next_day_window(
                    self.day_ctx().now,
                    self.day_start.0,
                    self.day_start.1,
                )
                .1;
                self.schedule_selection(&state, end);
            }
            ActionId::TaskUnschedule => {
                let members = self.view_state.selection_anchor_order(&state);
                for member in members {
                    if state
                        .tasks
                        .get(&member)
                        .is_some_and(|task| task.scheduled.is_some())
                    {
                        self.send(StateCommand::UpdateTask {
                            id: member,
                            changes: taskboard_domain::TaskChanges {
                                scheduled: Some(None),
                                ..taskboard_domain::TaskChanges::default()
                            },
                        });
                    }
                }
            }
            _ => unreachable!("the intercept's guard owns the verb set"),
        }
        self.finish_mutation(window);
    }

    /// The schedule pair's per-member dispatch (the arms' shared body):
    /// one sparse `UpdateTask` per selection member in anchor order,
    /// `scheduled` set to the shared window end, everything else
    /// untouched.
    fn schedule_selection(&mut self, state: &AppState, end: DateTime<Utc>) {
        for member in self.view_state.selection_anchor_order(state) {
            self.send(StateCommand::UpdateTask {
                id: member,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(end)),
                    ..taskboard_domain::TaskChanges::default()
                },
            });
        }
    }

    /// One fire-and-forget command send (the route arm's convention):
    /// a full or closed inbox is logged and dropped — never a panic
    /// (D16's forwarder convention).
    pub(crate) fn send(&self, command: StateCommand) {
        if self.commands.try_send(command).is_err() {
            tracing::warn!("command channel full or closed; dropped");
        }
    }

    /// One action through the router (design §5 pipeline's last hop).
    #[allow(clippy::too_many_lines)] // the per-bit consumption table reads flat
    pub(crate) fn route(&mut self, window: &AppWindow, id: ActionId) {
        // The focused-subject key translation (chunk 8 D3, #934): a
        // task-row verb resolved under a habit-focused walk routes
        // its habit twin — one stroke, one binding per context (the
        // preset tables stay the stroke truth; click ≡ key holds in
        // both directions). The identity for every pointer path and
        // every task/focusless walk by construction.
        let mut id = crate::presentation::translate_for_subject(id, self.view_state.walk_subject());
        // The selection's verb intercept (UX-R8 D-6, US-4/S3–S4): a
        // non-empty selection turns Complete/Delete — and the multi
        // bar's schedule trio (J5 US-2) — into the per-task sequences
        // (click ≡ key — the menu rows, the bar buttons and the
        // preset strokes land here alike). The track twins stay
        // single-target (Stop is global, Start rides the focused
        // member — the R-3 grammar). Empty selection keeps the
        // existing single-target semantics untouched.
        if matches!(
            id,
            ActionId::TaskComplete
                | ActionId::TaskDelete
                | ActionId::TaskScheduleToday
                | ActionId::TaskScheduleTomorrow
                | ActionId::TaskUnschedule
        ) && !self.view_state.selection().is_empty()
        {
            self.route_selection_verb(window, id);
            return;
        }
        // The per-action consumption set (cross-surface friction plan
        // D-6, #763): only the dispatched action's declared one-shots
        // move into the ctx — an undeclared field stays in the adapter,
        // so an interleaved dispatch can no longer eat a confirm's
        // target. The declaration table is router-owned
        // (`route_consumes`); the totalness test pins the vocabulary.
        // The habit menu's verbs (H2 D4): a menu row routes the bare
        // id — the subject resolves HERE from the opener's focus fact
        // when no seam pre-minted the one-shot (the repair seam's
        // exact arithmetic; a subjectless dispatch stays NoTarget).
        // Chunk 8 (D4): the walk's habit landing feeds the same
        // resolution — the effective subject is the walk cursor's
        // when the walk has entered (the exclusivity law makes a
        // task landing a typed `None`), the pointer click fact
        // otherwise; a stale click fact can never answer a task
        // walk's stroke.
        //
        // The R-8 kind resolution (US-14, #1031): the shared
        // Shift+Up/Down stroke is bound to the habit pair (Today
        // context); under a TASK walk landing the stroke flips to its
        // task twins — the twin law, one stroke one binding (the
        // preset tables stay the stroke truth; no second binding is
        // coined). Today-scoped because the binding is. On kiosk the
        // pair is an editing verb (the habit mint's HAB-14 reading):
        // the flip refuses and the dispatch takes the typed NoTarget.
        // A habit or focusless landing keeps the habit pair's path
        // untouched (US-14/S3). The twins' own ids mint too: the Task
        // group page's menu rows route `TaskMoveUp`/`TaskMoveDown`
        // directly (click ≡ key — the same dispatch entry, no stroke
        // to flip), so the pair is live from the menu exactly as from
        // the shared stroke.
        if self.view_state.view() == crate::view_state::ViewId::Today
            && !self.kiosk
            && matches!(
                id,
                ActionId::HabitMoveUp
                    | ActionId::HabitMoveDown
                    | ActionId::TaskMoveUp
                    | ActionId::TaskMoveDown
            )
            && let Some(task) = self
                .view_state
                .walk_subject()
                .and_then(|subject| match subject {
                    crate::presentation::RowSubject::Task(task) => Some(task),
                    crate::presentation::RowSubject::Habit(_) => None,
                })
        {
            id = if matches!(id, ActionId::HabitMoveUp | ActionId::TaskMoveUp) {
                ActionId::TaskMoveUp
            } else {
                ActionId::TaskMoveDown
            };
            self.task_move = Some(task);
        }
        if let Some(habit) = self.effective_habit() {
            match id {
                ActionId::HabitLog | ActionId::HabitSkip if self.habit_target.is_none() => {
                    self.habit_target = Some(habit);
                }
                // HF1 (W-11, O-8 parity): the resume chord over the
                // walk's landing mints the pause editor's one-shot
                // subject exactly as the row seams do — a subjectless
                // dispatch stays the typed NoTarget.
                ActionId::HabitResume if self.habit_pause_target.is_none() => {
                    self.habit_pause_target = Some(habit);
                }
                ActionId::HabitUndoLog if self.habit_undo.is_none() => {
                    self.habit_undo = habit_undo_target(self, habit);
                }
                ActionId::HabitRepair if self.habit_repair.is_none() => {
                    let state = self.state.load_full();
                    let target = state.habits.habits.get(&habit).and_then(|row| {
                        crate::presentation::habit_repair_target(
                            row,
                            &state.habits.log,
                            self.day_ctx().now,
                            self.calendar(),
                        )
                    });
                    drop(state);
                    if let Some((target_window, count)) = target {
                        self.habit_repair = Some((habit, target_window, count));
                    }
                }
                // The keyboard reorder pair (chunk 8 D4, #934): the
                // walk's landing mints the same one-shots the menu
                // open mints — the shared slot arithmetic, one
                // mint site. On kiosk the mint refuses (the pair is
                // an editing verb, HAB-14): the binding stays
                // visible, the dispatch takes the typed NoTarget.
                ActionId::HabitMoveUp | ActionId::HabitMoveDown if self.kiosk => {}
                ActionId::HabitMoveUp if self.habit_move_up.is_none() => {
                    let state = self.state.load_full();
                    let (up, _) = Self::habit_move_slots(&state, habit);
                    self.habit_move_up = up;
                }
                ActionId::HabitMoveDown if self.habit_move_down.is_none() => {
                    let state = self.state.load_full();
                    let (_, down) = Self::habit_move_slots(&state, habit);
                    self.habit_move_down = down;
                }
                _ => {}
            }
        }
        // US-11 (W-12, #1067): the resume chord mints the series
        // one-shot from the viewed subject exactly as the recur row's
        // click seam does — click ≡ key; a subjectless dispatch stays
        // the typed NoTarget (the router arm's refusal). The guard
        // keeps a pre-minted seam target (the pointer twin) authoritative.
        // US-9 (W-10): the roster remove's arm toggle reads the
        // roster cursor inside the routed arm (below) — the pointer
        // twin re-roots through the same route, one semantics one
        // home; no route-side mint exists (the toggle owns the
        // armed slot).
        if id == ActionId::SeriesResume
            && self.series_target.is_none()
            && let Some(task) = self.view_state.subject_task()
        {
            self.series_target = Some(task);
        }
        let wants = crate::router::route_consumes(id);
        let mut ctx = self.shell.dispatch_ctx();
        if wants.wants(crate::router::OneShotSet::CREATE_TARGET) {
            ctx.create_target = self.create_target.take();
        }
        if wants.wants(crate::router::OneShotSet::REMINDER_TARGET) {
            ctx.reminder_target = self.reminder_target;
            self.reminder_target = None;
        }
        if wants.wants(crate::router::OneShotSet::BOARD_TARGET) {
            ctx.board_target = self.board_target;
            self.board_target = None;
        }
        if wants.wants(crate::router::OneShotSet::RULE_TARGET) {
            ctx.rule_target = self.rule_target;
            self.rule_target = None;
        }
        if wants.wants(crate::router::OneShotSet::RULE_ADD_WHEN) {
            ctx.rule_add_when = self.rule_add_when;
            self.rule_add_when = None;
        }
        if wants.wants(crate::router::OneShotSet::TIMER_DRAFT) {
            ctx.timer_draft = self.timer_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::TIMER_TARGET) {
            ctx.timer_target = self.timer_target.take();
        }
        if wants.wants(crate::router::OneShotSet::BREAK_RULE_DRAFT) {
            ctx.break_rule_draft = self.break_rule_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::BREAK_RULE_TARGET) {
            ctx.break_rule_target = self.break_rule_target.take();
        }
        if wants.wants(crate::router::OneShotSet::TRACK_ENTRY_DRAFT) {
            ctx.track_entry_draft = self.track_entry_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::TRACK_ENTRY_TARGET) {
            ctx.track_entry_target = self.track_entry_target.take();
        }
        if wants.wants(crate::router::OneShotSet::TRACK_TARGET) {
            ctx.track_target = self.track_target.take();
        }
        if wants.wants(crate::router::OneShotSet::PLAN_TARGET) {
            ctx.plan_target = self.plan_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_TARGET) {
            ctx.habit_target = self.habit_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_UNDO) {
            ctx.habit_undo = self.habit_undo.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_MOVE) {
            ctx.habit_move = self.habit_move.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_MOVE_UP) {
            ctx.habit_move_up = self.habit_move_up.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_MOVE_DOWN) {
            ctx.habit_move_down = self.habit_move_down.take();
        }
        if wants.wants(crate::router::OneShotSet::TASK_MOVE) {
            ctx.task_move = self.task_move.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_REPAIR) {
            ctx.habit_repair = self.habit_repair.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_CONVERT_TARGET) {
            ctx.habit_convert_target = self.habit_convert_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_REMOVE_TARGET) {
            // The staged remove's armed subject (H4 D4): the arm
            // recorded it; the confirm consumes the one-shot (the
            // armed state clears with the dispatch — a repeat confirm
            // is the disarmed `NoTarget`).
            ctx.habit_remove_target = self.habit_remove_armed.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_RESTORE_TARGET) {
            // The staged restore's armed subject (W-3): the arm
            // recorded it; the confirm consumes the one-shot (the
            // armed state clears with the dispatch — the #1187 law's
            // re-delete then leaves the habit tombstoned again, the
            // roster re-derives).
            ctx.habit_restore_target = self.habit_restore_armed.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_PAUSE_TARGET) {
            ctx.habit_pause_target = self.habit_pause_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_CREATE_DRAFT) {
            ctx.habit_create_draft = self.habit_create_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_QUOTA_DRAFT) {
            ctx.habit_quota_draft = self.habit_quota_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::PATTERN_DRAFT) {
            ctx.pattern_draft = self.pattern_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::SERIES_TARGET) {
            ctx.series_target = self.series_target.take();
        }
        if wants.wants(crate::router::OneShotSet::PAUSE_DRAFT) {
            ctx.pause_draft = self.pause_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::CONVERT_TARGET) {
            ctx.convert_target = self.convert_target.take();
        }
        if wants.wants(crate::router::OneShotSet::RECUR_EDIT_TARGET) {
            ctx.recur_edit_target = self.recur_edit_target.take();
        }
        if wants.wants(crate::router::OneShotSet::RECUR_DRAFT_RULE) {
            ctx.recur_draft_rule = self.recur_draft_rule.take();
        }
        // The today window rides every dispatch (chunk 10 D4): state-
        // derived from the held pair, never consumed.
        ctx.day_window = Some(self.day_ctx().window);
        // The today re-commit stamps' fact (#893, the chunk plan
        // D3.2): the window's last covered instant over the same
        // frozen day facts — the domain-owned mint, never a local
        // `end − ε` re-derivation (the #884 rule).
        ctx.today_last_instant = Some(taskboard_domain::days::last_covered_instant(
            self.day_ctx().window.1,
        ));
        // The tomorrow window's end (UX-R8 chunk-5 D-3): the additive
        // day fact, `days::next_day_window` — never `+24h` arithmetic.
        ctx.tomorrow_window_end = Some(
            taskboard_domain::days::next_day_window(
                self.day_ctx().now,
                self.day_start.0,
                self.day_start.1,
            )
            .1,
        );
        // The smart toggle's state half (J3 R-8, round 9 chunk 11):
        // synced at dispatch time — the fact must answer about the
        // exact resolved target, which only exists after the one-shot
        // move above (the shell sync cannot see it). The read is
        // `send_card_track_toggle`'s re-rooted (one semantics, one
        // home — the router arm consumes it).
        if id == ActionId::TaskTrackToggle {
            let open = crate::presentation::tracking_open_task(&self.state.load());
            ctx.focused_task_tracking =
                open.is_some_and(|open| Some(open) == ctx.track_target.or(ctx.focused_task));
        }
        let outcome = ActionRouter::dispatch(id, &ctx);
        match outcome {
            DispatchOutcome::Command(command) => {
                // T-41/T-43 (#824/#826): the acknowledgement rides the
                // status-notice lane (the apply_outcomes twin).
                if self.arm_outcome_notice(&command) {
                    self.push_notice(window);
                }
                // Non-blocking, human-input-scale (D16): a full inbox
                // is logged and dropped — the forwarder convention.
                if self.commands.try_send(command).is_err() {
                    tracing::warn!(action = %id, "command channel full or closed; dropped");
                }
            }
            DispatchOutcome::UiEffect(effect) => self.apply_effect(window, effect),
            // The typed refusal's uniform law (chunk 8 D6, #935 /
            // O-7 Q5): a target-less dispatch surfaces on the
            // status-notice lane from the action's registry
            // description — one law at one place, not per-verb spot
            // fixes (the subjectless habit verb, the stale one-shot
            // and the un-armed convert are covered by construction;
            // US-14's R-8 resolution gave the Shift+Up-on-a-task-row
            // stroke its task twins, so the residual NoTarget is a
            // focusless stroke). `Disabled` keeps its posture:
            // the disabled-row notice is already armed at the
            // registry arm (invoke_entry's T-19 twin).
            DispatchOutcome::Disabled => {}
            DispatchOutcome::NoTarget => {
                self.arm_no_target_notice(id);
                self.push_notice(window);
            }
        }
        self.finish_mutation(window);
    }

    /// The dismiss-all tooltip primitive (r2 D13): the global's
    /// existing `unhover()` — invoked from the effect-application
    /// points where the anchor is about to unmount or a modal context
    /// takes over. Rule: a tooltip never outlives the element that
    /// spawned it.
    fn dismiss_tooltip(window: &AppWindow) {
        window.global::<crate::TooltipState>().invoke_unhover();
    }

    // === The focus-origin fact (the focus-restoration plan US-1/W-1,
    // === D-F1; the chunk plan §2 census table is the site list) ====

    /// The shared post-mutation sync (r2 D12): the view-context seam
    /// realignment plus the shell → window global push. `route`'s
    /// trailing pair, factored out so the mouse menu paths
    /// (`activate_parent`, `apply_close_menu`, the `MenuBack` arm)
    /// update the same globals the keyboard path updates — the parent
    /// rows' `menu-active-group` used to stay stale (F7).
    /// The currently open census site, if any (the funnel-side read:
    /// recording and restoring both key on this, so no exit arm can
    /// be forgotten — the `apply_outcomes` pass-tail logic). The chain
    /// checks keep a nested child level (the due editor's picker
    /// phase, the walk's pickers) recognized as its parent site: the
    /// child's exit arms restore *into* the parent (the census's
    /// "the picker exits inward" letter).
    fn open_focus_site(&self, window: &AppWindow) -> Option<FocusSeam> {
        use crate::view_state::Flow;
        let chained = |pred: &dyn Fn(&Flow) -> bool| self.view_state.chain_has_flow(pred);
        // Child-bearing sites first (deeper levels own the keyboard).
        if chained(&|f| {
            matches!(
                f,
                Flow::ReviewWalk { .. }
                    | Flow::WalkFilePick { .. }
                    | Flow::WalkDatePick
                    | Flow::EntryDatePicker
            )
        }) {
            return Some(FocusSeam::ReviewWalk);
        }
        if chained(&|f| matches!(f, Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. })) {
            return Some(FocusSeam::TrackEntries);
        }
        if chained(&|f| matches!(f, Flow::Labels { .. })) {
            return Some(FocusSeam::LabelsOverlay);
        }
        if chained(&|f| matches!(f, Flow::EditRecur { .. })) {
            return Some(FocusSeam::RecurEditor);
        }
        if chained(&|f| {
            matches!(
                f,
                Flow::EditDate {
                    field: crate::view_state::DateField::Due,
                    ..
                }
            )
        }) {
            return Some(FocusSeam::DueEditor);
        }
        if chained(&|f| {
            matches!(
                f,
                Flow::EditDate {
                    field: crate::view_state::DateField::Scheduled,
                    ..
                }
            )
        }) {
            return Some(FocusSeam::ScheduledEditor);
        }
        if chained(&|f| matches!(f, Flow::EditTitle { .. })) {
            return Some(FocusSeam::TitleEditor);
        }
        if chained(&|f| matches!(f, Flow::EditDescription { .. })) {
            return Some(FocusSeam::DescriptionEditor);
        }
        if chained(&|f| matches!(f, Flow::PlanPlace { .. })) {
            return Some(FocusSeam::PlanPlace);
        }
        if chained(&|f| matches!(f, Flow::Palette { .. })) {
            return Some(FocusSeam::Palette);
        }
        if chained(&|f| matches!(f, Flow::Capture)) {
            return Some(FocusSeam::Capture);
        }
        if chained(&|f| matches!(f, Flow::FocusMode { .. })) {
            return Some(FocusSeam::FocusOverlay);
        }
        // The create/rename flows are deliberately NOT census sites
        // (the recorded deviation from the chunk plan's §2 row 16):
        // the inline create row's commit is the braindump grammar —
        // its exit re-seeds the walk through reconcile, and a restore
        // armed beside it fights that re-seed (the ten-step journey's
        // walk state). The rename cells' exits keep today's letter for
        // the same reason: the cell itself is the re-entry point the
        // flow arms in place.
        if chained(&|f| {
            matches!(
                f,
                Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. }
            )
        }) {
            return Some(FocusSeam::MoveFlow);
        }
        if chained(&|f| matches!(f, Flow::ConfirmDelete(_))) {
            return Some(FocusSeam::ConfirmDelete);
        }
        if chained(&|f| matches!(f, Flow::BoardColorPick { .. })) {
            return Some(FocusSeam::BoardColorPick);
        }
        match self.priority_open {
            Some(crate::router::PriorityField::Importance) => {
                return Some(FocusSeam::PriorityDue);
            }
            Some(crate::router::PriorityField::Urgency) => {
                return Some(FocusSeam::PriorityScheduled);
            }
            None => {}
        }
        if window.get_timer_setter_open() {
            return Some(FocusSeam::TimerSetter);
        }
        if self.shell.menu.open {
            return Some(FocusSeam::ActionMenu);
        }
        None
    }

    /// Whether the control walk clamps (US-2/W-2, D-W2): the masked
    /// host set is the census's modal hosts — the palette, capture,
    /// timer setter, focus overlay, track entries, due editor, recur
    /// editor, and labels overlay. The action menu and the review walk
    /// own their key grammars outright (their row walks are not the
    /// control walk), so they stay off this list.
    fn mask_armed(&self, window: &AppWindow) -> bool {
        matches!(
            self.open_focus_site(window),
            Some(
                FocusSeam::Palette
                    | FocusSeam::Capture
                    | FocusSeam::TimerSetter
                    | FocusSeam::FocusOverlay
                    | FocusSeam::TrackEntries
                    | FocusSeam::DueEditor
                    | FocusSeam::RecurEditor
                    | FocusSeam::LabelsOverlay
            )
        )
    }

    /// The open pass's recording half: the anchor is the invoking
    /// element's identity at record time — the logical walk focus,
    /// else the subject (subject-affine sites), else the view. The
    /// stack/board variants are the *restore* vocabulary (the census's
    /// header anchors resolve against them); the generic ladder never
    /// mints them — a plain view context with a seeded selection must
    /// restore to the view, not silently jump to a column.
    fn mint_focus_origin(&self, seam: FocusSeam) -> FocusOrigin {
        let state = self.state.load_full();
        let anchor = if let Some(task) = self.view_state.focused_task() {
            OriginAnchor::Task(task)
        } else if seam.subject_affine() && self.view_state.subject_task().is_some() {
            OriginAnchor::Task(self.view_state.subject_task().expect("checked above"))
        } else {
            OriginAnchor::View
        };
        // The container snapshot: a board row falls to its stack
        // header; every other anchor's container is the view scope
        // (the plan AC-2's letter — "the row's stack header / the
        // view root").
        let fallback = match anchor {
            OriginAnchor::Task(task) => {
                if self.view_state.view() == crate::view_state::ViewId::Board {
                    let stack_ids = self.view_state.stack_ids(&state);
                    self.tables
                        .board_position(task)
                        .and_then(|(column, _)| stack_ids.get(column).copied())
                        .map_or(OriginAnchor::View, OriginAnchor::Stack)
                } else {
                    OriginAnchor::View
                }
            }
            OriginAnchor::Stack(_) | OriginAnchor::View => OriginAnchor::View,
        };
        FocusOrigin {
            seam,
            anchor,
            fallback,
        }
    }

    /// One restore resolution: `Some` names the anchor family function
    /// to invoke (and the logical-focus writes the row anchor needs —
    /// rows carry no slint focus stop; the walk's emphasis look is the
    /// row focus, so a `Task` anchor re-arms the walk state and the
    /// scroll hint beside the fact push). `None` = the anchor is gone
    /// at every level; the caller falls to the view-scope refocus.
    #[allow(clippy::type_complexity)] // one resolution shape, local to the restore
    fn resolve_anchor(
        &self,
        state: &taskboard_domain::AppState,
        anchor: &OriginAnchor,
    ) -> Option<(i32, i32, i32)> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_index = |value: usize| value as i32;
        match *anchor {
            OriginAnchor::View => None,
            OriginAnchor::Task(task) => {
                if !state.tasks.contains_key(&task) {
                    return None;
                }
                match self.view_state.view() {
                    crate::view_state::ViewId::Board => {
                        let (column, row) = self.tables.board_position(task)?;
                        Some((1, to_index(column), to_index(row)))
                    }
                    crate::view_state::ViewId::Today => {
                        let (section, row) = self.tables.today_position(task)?;
                        Some((1, to_index(section), to_index(row)))
                    }
                    crate::view_state::ViewId::StackDetail => {
                        let row = self.tables.stack_position(task)?;
                        Some((1, -1, to_index(row)))
                    }
                    // The detail's own cell is the row anchor's host
                    // (census rows 14/15's "the title/description cell").
                    crate::view_state::ViewId::TaskDetail => Some((4, -1, -1)),
                    _ => None,
                }
            }
            OriginAnchor::Stack(stack) => {
                let column = self
                    .view_state
                    .stack_ids(state)
                    .iter()
                    .position(|s| *s == stack)?;
                Some((2, to_index(column), -1))
            }
        }
    }

    /// The restore's fact half: resolve the origin (anchor, then the
    /// recorded container), publish the `FocusFacts` tuple through the
    /// anchor family's seam, and land the logical half of a row
    /// restore. Returns whether an element-level restore happened —
    /// `false` means the caller's view-scope refocus fires (D-F1's
    /// ordering law: the view-scope call only runs when the origin
    /// resolved to the view).
    fn push_focus_restore(&mut self, window: &AppWindow, origin: &FocusOrigin) -> bool {
        let state = self.state.load_full();
        let fallback = origin.fallback;
        let Some((mut kind, a, b)) = self
            .resolve_anchor(&state, &origin.anchor)
            .or_else(|| self.resolve_anchor(&state, &fallback))
        else {
            return false;
        };
        // The detail's per-seam cell table (round 5 A2, #1205's
        // mechanism B): a `Task` anchor at `TaskDetail` resolves to
        // kind 4 — the census's single detail cell — and the opener's
        // identity used to be discarded there (every overlay restored
        // onto the title). The seam names the opener's own cell; the
        // view carries the matching claimants. Only the detail's
        // collapse is remapped: overlays opened from row contexts
        // keep their row anchors (kinds 1–3).
        if kind == 4 {
            kind = match origin.seam {
                FocusSeam::DescriptionEditor => 5,
                FocusSeam::DueEditor => 7,
                FocusSeam::ScheduledEditor => 8,
                FocusSeam::PriorityDue => 9,
                FocusSeam::PriorityScheduled => 10,
                FocusSeam::LabelsOverlay => 11,
                FocusSeam::RecurEditor => 12,
                FocusSeam::TrackEntries => 13,
                // The seams with no detail cell of their own (the
                // title editor — kind 4 is its own cell — the effort
                // opener's kind-6 push, the modals) keep the resolved
                // cell.
                _ => 4,
            };
        }
        let facts = window.global::<crate::FocusFacts>();
        facts.set_seam_id(origin.seam.id());
        match kind {
            1 => {
                // The logical half: the walked look lands on the row
                // (the emphasis is the row's focus render) and the
                // scroll hint brings it into view — the walk-stop
                // sibling the census names.
                let task = match (origin.anchor, fallback) {
                    (OriginAnchor::Task(task), _) | (_, OriginAnchor::Task(task)) => Some(task),
                    _ => None,
                };
                if let Some(task) = task.filter(|t| state.tasks.contains_key(t)) {
                    self.view_state.set_focused(Some(task));
                }
                window.invoke_focus_task_row(a, b);
            }
            2 => {
                // The stack header's selection look (the segment ring's
                // selected branch) — the header's own anchor.
                if let OriginAnchor::Stack(stack) = origin.anchor {
                    self.view_state.select_stack(&state, stack);
                }
                window.invoke_focus_stack_header(a);
            }
            3 => window.invoke_focus_board_cell(a),
            4..=13 => window.invoke_focus_detail_cell(kind),
            _ => return false,
        }
        // The row restore's scroll-into-view half (the FocusScrollHint
        // consumer pairs with the fact push).
        if kind == 1 {
            let before = None;
            self.push_focus_scroll_hint(window, before);
        }
        true
    }

    /// The sync-time funnel (`sync_view_context` runs at the end of
    /// every mutation pass): record on open, restore on close. The
    /// restore's `false` here needs no view-scope call — the closes
    /// that only ride this funnel (the flag/property sites) had no
    /// flow to end and no identity to swap; today's behavior for them
    /// was "nothing", and the element restore is the whole delta.
    fn sync_focus_origin(&mut self, window: &AppWindow) {
        if self.applying_outcomes {
            return;
        }
        // The walk clamp's armed push (US-2/W-2): the flag is the site
        // table's read, published every pass so the hosts and the
        // tests see the current mask without a walk dispatch.
        window
            .global::<crate::MaskWalkFacts>()
            .set_armed(self.mask_armed(window));
        if let Some(origin) = self.focus_origin.take() {
            if self.open_focus_site(window) == Some(origin.seam) {
                // Still open (a refresh pass mid-site): keep holding.
                self.focus_origin = Some(origin);
            } else {
                self.push_focus_restore(window, &origin);
            }
        }
        if let Some(seam) = self
            .open_focus_site(window)
            .filter(|open| self.focus_origin.as_ref().is_none_or(|o| o.seam != *open))
        {
            // Last-open-wins: a second site's open overwrites.
            self.focus_origin = Some(self.mint_focus_origin(seam));
        }
    }

    /// The exit funnels' focus half (the D-F1 ordering law): consume
    /// the origin when its site is gone; `true` = an element-level
    /// restore fired and the caller's view-scope refocus is suppressed.
    fn exit_focus_handoff(&mut self, window: &AppWindow) -> bool {
        if self.open_focus_site(window).is_none()
            && let Some(origin) = self.focus_origin.take()
        {
            return self.push_focus_restore(window, &origin);
        }
        false
    }

    fn finish_mutation(&mut self, window: &AppWindow) {
        self.sync_view_context(window);
        self.sync(window);
    }

    /// The context chain under the pulled scope facts (round-5 chunk
    /// 7, D-KS1): the declarative stack construction that replaced
    /// `KeymapShell::chain`'s boolean reconstruction. Every input is
    /// re-read at the call — no stored registrations can outlive
    /// their surface (the D-KS1 pull rule). The deltas arm only at
    /// the key doorway; hint/refresh passes call the `None`/`false`
    /// form and get the byte-legacy chain.
    fn current_chain(&self, window: &AppWindow) -> Vec<ContextId> {
        self.chain_with(window, None, false)
    }

    /// [`Self::current_chain`] with the key doorway's pending stroke
    /// and doorway flag (the D-δ1 exemption and the delta arms).
    fn chain_with(
        &self,
        window: &AppWindow,
        stroke: Option<crate::keymap::Stroke>,
        at_key_doorway: bool,
    ) -> Vec<ContextId> {
        let flow = self.view_state.flow().clone();
        let chain = self.view_state.chain_tags();
        let editor_focus = window
            .global::<crate::EditingFacts>()
            .get_editor_focus_count()
            > 0;
        crate::keyscope::scope_stack(&crate::keyscope::ScopeInputs {
            shell: &self.shell,
            picker_open: self
                .view_state
                .date_editor()
                .is_some_and(|editor| editor.picker.is_some()),
            editor_focus,
            flow: &flow,
            chain: &chain,
            stroke,
            at_key_doorway,
        })
    }

    /// Keeps the shell's view seam aligned with the view-state
    /// machine (the D16/D17 seam, now driven for real).
    fn sync_view_context(&mut self, window: &AppWindow) {
        // The focus-origin funnel (US-1/W-1): record on open, restore
        // on close — before the shell mirror, so the restore's logical
        // writes (the walked look, the stack selection) land in the
        // same push the pass derives.
        self.sync_focus_origin(window);
        let state = self.state.load();
        self.shell.view = Some(self.view_state.context());
        self.shell.focused_task = self.view_state.focused_task();
        self.shell.focused_stack = self.view_state.focused_stack(&state);
        // The modal masks' truth is chain-wide (chunk 3): a child
        // level holding the chain's top must not unmask its parent —
        // the walk's context stays armed while its File-to or
        // Pick-a-date child is on top.
        self.shell.palette = self
            .view_state
            .chain_has_flow(|flow| matches!(flow, crate::view_state::Flow::Palette { .. }));
        self.shell.focus_overlay = self
            .view_state
            .chain_has_flow(|flow| matches!(flow, crate::view_state::Flow::FocusMode { .. }));
        self.shell.track_entries = self.view_state.chain_has_flow(|flow| {
            matches!(
                flow,
                crate::view_state::Flow::TrackEntries { .. }
                    | crate::view_state::Flow::TrackEntryForm { .. }
            )
        });
        self.shell.review_walk = self
            .view_state
            .chain_has_flow(|flow| matches!(flow, crate::view_state::Flow::ReviewWalk { .. }));
        // The notification surface's engagement mirror (US-6/W-6,
        // D-N1): armed while the cursor holds AND rows exist; rows
        // emptying disarms (the posture's letter).
        if self.tables.notifications.is_empty() {
            self.notification_cursor = None;
        } else if let Some(cursor) = self.notification_cursor {
            self.notification_cursor = Some(cursor.min(self.tables.notifications.len() - 1));
        }
        self.shell.notification_surface = self.notification_cursor.is_some();
        // The export dialog's mirror (the J2 export plan W-1): the
        // adapter-owned VM's truth (not a flow), synced like the
        // modal booleans above.
        self.shell.export_dialog = self.export_vm.is_some();
        // Round 2 (D19): the reopen gate's done stamp, from the same
        // focused task — the router stays pure.
        self.shell.focused_task_done = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.done.is_some());
        // The kind flag (issue #492): the complete verb's router guard
        // and the toggle's inversion read the same focused task.
        self.shell.focused_task_tracker = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.kind == taskboard_domain::entities::TaskKind::Tracker);
        // The scheduled-day fact (UX-R8 chunk-5 D-3): task.unschedule's
        // route gate — the focused_task_done shape.
        self.shell.focused_task_scheduled = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.scheduled.is_some());
        // The effort estimate's presence (J3 US-4, round 9 chunk 11):
        // task.effort-clear's route gate — the scheduled-day fact's
        // shape.
        self.shell.focused_task_effort = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.effort.is_some());
    }

    /// The geometry capture (#412, D4): samples the window's normal
    /// state once per cadence tick and emits one full-snapshot
    /// request when it changed. Guard chain, in order: never under
    /// the kiosk profile (D6 — fullscreen owns the appliance's
    /// geometry; the path is bypassed, not special-cased); never
    /// maximized/minimized/fullscreen (a maximized window's bounds
    /// are the workspace's, not the user's normal geometry, and
    /// minimized geometry is stale/zero on several platforms); the
    /// mint refuses degenerate snapshots silently (which keeps the
    /// headless default 0×0 quiet); the dedupe baseline swallows
    /// unchanged re-samples. A full or closed request inbox drops the
    /// request with the baseline unset — the next tick retries.
    pub(crate) fn capture_window_geometry(&mut self, window: &AppWindow) {
        if self.kiosk {
            return;
        }
        let slint_window = window.window();
        if slint_window.is_maximized()
            || slint_window.is_minimized()
            || slint_window.is_fullscreen()
        {
            return;
        }
        let position = slint_window.position();
        let size = slint_window.size();
        let Some(geometry) = crate::geometry::WindowGeometry::new(
            i64::from(position.x),
            i64::from(position.y),
            i64::from(size.width),
            i64::from(size.height),
        ) else {
            return;
        };
        if self.last_window_geometry == Some(geometry) {
            return;
        }
        let request = crate::requests::UiRequest::WindowGeometryChanged { geometry };
        if self.requests.try_send(request).is_ok() {
            self.last_window_geometry = Some(geometry);
        } else {
            tracing::warn!("request channel full or closed; geometry capture dropped");
        }
    }

    /// Applies flow outcomes verbatim (D9): the adapter never
    /// re-decides. `RefreshNow` re-derives the models immediately —
    /// the timer-driven refresh (D1) covers the passive side. A mode
    /// flip is pushed onto the window global here (the shell's one-way
    /// sync, D18): the text-commit seams end in this method without a
    /// `route()` around them, and the visible chain (the status bar's
    /// mode read) must not go stale.
    #[cfg(test)]
    pub(crate) fn apply_outcomes_for_test(&mut self, window: &AppWindow, outcomes: &[FlowOutcome]) {
        self.apply_outcomes(window, outcomes);
    }

    fn apply_outcomes(&mut self, window: &AppWindow, outcomes: &[FlowOutcome]) {
        let mut mode_changed = false;
        self.applying_outcomes = true;
        let flow_ended =
            self.flow_open == FlowLiveness::Open && matches!(self.view_state.flow(), Flow::None);
        for outcome in outcomes {
            match outcome {
                FlowOutcome::Command(command) => {
                    // T-41/T-43 (#824/#826): the acknowledgement rides
                    // the status-notice lane — the typed fact arms at
                    // dispatch time (the disabled-row notice's
                    // event-stamp shape).
                    if self.arm_outcome_notice(command) {
                        self.push_notice(window);
                    }
                    // The create-focus handoff's arming (UX-R8 chunk-5
                    // D-7, US-5): the subject stack's live ids at
                    // commit time — the empty-buffer settle never
                    // emits the command (view_state's Flow::Create
                    // guard), so it arms nothing.
                    if let taskboard_domain::StateCommand::CreateTask { stack, .. } = command {
                        let state = self.state.load_full();
                        let before: std::collections::BTreeSet<TaskId> = state
                            .tasks
                            .values()
                            .filter(|task| task.stack == *stack)
                            .map(|task| task.id)
                            .collect();
                        let snapshot = Arc::clone(&state);
                        drop(state);
                        self.create_focus_arm = Some(CreateFocusArm {
                            stack: *stack,
                            before,
                            snapshot,
                        });
                    }
                    if self.commands.try_send(command.clone()).is_err() {
                        tracing::warn!("command channel full or closed; dropped");
                    }
                }
                FlowOutcome::SetMode(mode) => {
                    self.shell.apply(&UiEffect::SetMode(*mode), 0);
                    mode_changed = true;
                }
                // The view machine's re-derive verdict: forced — the
                // derive inputs are adapter-side, the pointer is not.
                FlowOutcome::RefreshNow => self.refresh_forced(window),
                FlowOutcome::StackWidthsChanged => {
                    // The persist-now signal (round-1 D21): the full
                    // per-board snapshot over the request lane,
                    // warn-and-drop on a full/closed inbox — the next
                    // event retries.
                    if let Some(board) = self.view_state.board() {
                        let request = crate::requests::UiRequest::StackWidthsChanged {
                            board,
                            widths: self.view_state.stack_widths_of(board),
                        };
                        if self.requests.try_send(request).is_err() {
                            tracing::warn!("request channel full or closed; dropped");
                        }
                    }
                }
            }
        }
        if mode_changed {
            self.sync(window);
        }
        // The D15 focus hand-off: a flow's end unmounts the field that
        // held slint focus; the active view scope takes it back (the
        // §7.3 journey caught the keyboard dead-end).
        self.flow_open = if matches!(self.view_state.flow(), Flow::None) {
            FlowLiveness::Flowless
        } else {
            FlowLiveness::Open
        };
        if flow_ended {
            // The D20 reset seam 1 (flow ends): the funnel that drives
            // `refocus_view` is where a focused field unmounts — the
            // runtime never fires its loss event, so the counts would
            // stick and mask the shell forever.
            Self::reset_editing_facts(window);
            // The focus-origin restore first (US-1/W-1, D-F1's
            // ordering law): the view-scope refocus only fires when
            // the origin resolved to the view — an element-level
            // restore lands on the origin, not the window top.
            if !self.exit_focus_handoff(window) {
                window.invoke_refocus_view();
            }
        }
        self.applying_outcomes = false;
    }

    /// The D20 reset seams (see the flow-end call): both counts to 0
    /// wherever the tree changes without delivering loss events. A
    /// field that mounts focused re-publishes on its init focus grab,
    /// so a reset never strands an engaged field.
    fn reset_editing_facts(window: &AppWindow) {
        let facts = window.global::<crate::EditingFacts>();
        facts.set_text_focus_count(0);
        facts.set_activation_focus_count(0);
        facts.set_editor_focus_count(0);
        // The generation bump tells every live publisher the counts
        // were re-baselined, so its next focus event re-syncs its
        // latch instead of paying a stale decrement.
        facts.set_generation(facts.get_generation() + 1);
    }

    /// The per-refresh settings mirror (round-5 chunk 5, D-5.7): every
    /// settings-derived input the pass reads comes from the published
    /// snapshot, so a committed setting is visible at the next refresh
    /// — the publish itself swaps the state Arc, which is what makes
    /// [`Self::refresh`] run the forced pass.
    fn sync_settings(&mut self, settings: &taskboard_domain::SettingsSnapshot) {
        self.day_start = (settings.day_start_time, self.boot_offset);
        self.due_soon_horizon = settings.due_soon_days.horizon();
        self.reminder_defaults = settings.reminder_defaults();
        // A kiosk never carries a timing (the spawn exclusion's first
        // layer; the overlay's action gate re-checks anyway).
        self.focus = if self.kiosk {
            None
        } else {
            settings.live_focus()
        };
    }

    /// The injected day-boundary vocabulary ([`Self::day_ctx`]'s
    /// sibling): the held `day_start` pair as the domain `DayCalendar`
    /// the recurring badge's derivations read (DR-6/4).
    pub(crate) fn calendar(&self) -> taskboard_domain::DayCalendar {
        taskboard_domain::DayCalendar {
            day_start_time: self.day_start.0,
            offset: self.day_start.1,
        }
    }

    /// The pass's time bundle (chunk 10 D4): a fresh `now` and the
    /// window derived from the held pair — recomputed on every pass,
    /// so the rollover rides the existing minute-boundary repaint.
    pub(crate) fn day_ctx(&self) -> crate::view_state::DayCtx {
        let now = self.frozen_now.unwrap_or_else(chrono::Utc::now);
        let window = taskboard_domain::days::day_window(now, self.day_start.0, self.day_start.1);
        crate::view_state::DayCtx { now, window }
    }
    /// The placement commit funnel (#622, D-3/D-8): parse the two
    /// buffers through the due-editor's parsers verbatim, then route
    /// ONE `UpdateTask` with both masks (the effort mask present
    /// only when entered — untouched otherwise), one undo step. A
    /// parse refusal dispatches nothing and keeps the typed text.
    fn commit_plan_place(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        let Some(place) = self.view_state.plan_place() else {
            return Vec::new();
        };
        let Some(time) = crate::due_edit::parse_time_lenient(&place.time_text) else {
            return Vec::new();
        };
        let effort = if place.effort_text.trim().is_empty() {
            None
        } else {
            match taskboard_domain::Effort::parse_text_form(&place.effort_text) {
                Some(effort) => Some(effort),
                None => return Vec::new(),
            }
        };
        let day = self.day_ctx();
        let cal = self.calendar();
        let (next_start, _) =
            taskboard_domain::days::next_day_window(day.now, self.day_start.0, self.day_start.1);
        let date = cal.calendar_date(next_start);
        let instant = cal.instant(date, time);
        // The commit's consume is the flow's end — the level drops
        // with its buffers (D-H2).
        self.view_state.end_plan_place();
        vec![
            FlowOutcome::Command(taskboard_domain::StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(instant)),
                    effort: effort.map(Some),
                    ..taskboard_domain::TaskChanges::default()
                },
            }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The box swap (#622, D-3): the addressed box's scheduled
    /// instant and the temporal neighbor's exchange — two honest
    /// `UpdateTask` writes (each journal-armed); the instant set is
    /// invariant, so reorder never creates or needs arithmetic. An
    /// edge box (no neighbor that way) or a boxless neighbor is
    /// inert.
    fn swap_plan_box(
        &mut self,
        task: TaskId,
        earlier: bool,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        let day = self.day_ctx();
        let placed = taskboard_domain::days::plan_next_day(
            &state.tasks,
            day.now,
            self.day_start.0,
            self.day_start.1,
        )
        .placed_scheduled;
        let Some(index) = placed.iter().position(|id| *id == task) else {
            return Vec::new();
        };
        let neighbor = if earlier {
            if index == 0 {
                return Vec::new();
            }
            placed[index - 1]
        } else {
            let Some(neighbor) = placed.get(index + 1) else {
                return Vec::new();
            };
            *neighbor
        };
        let mine = state.tasks.get(&task).and_then(|t| t.scheduled);
        let theirs = state.tasks.get(&neighbor).and_then(|t| t.scheduled);
        let (Some(mine), Some(theirs)) = (mine, theirs) else {
            return Vec::new();
        };
        let swap = |id: TaskId, instant: chrono::DateTime<chrono::Utc>| {
            FlowOutcome::Command(taskboard_domain::StateCommand::UpdateTask {
                id,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(instant)),
                    ..taskboard_domain::TaskChanges::default()
                },
            })
        };
        vec![
            swap(task, theirs),
            swap(neighbor, mine),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The per-second tick's single gate (r4 D17, §2 decision 5): the
    /// one place that decides whether any mounted surface's displayed
    /// value changes with the wall second. **This function is the
    /// single tick channel** — chunks 8/9 (countdown rows) extend THIS
    /// term; never introduce a second tick channel.
    ///
    /// D17's term today: the mounted task view's subject holds any
    /// open entry — only then does the displayed total change per
    /// second (a foreign task's open entry does not move the row). The
    /// verb button may show Start during the transient double-open
    /// while the subject's older open entry still accrues — the tick
    /// follows the arithmetic truth, the button follows the D-T8
    /// display pick; recorded. Chunks 8/9's terms (r4 D30/D32): the
    /// subject's At-kinded rule rows and the timers surfaces are the
    /// seconds-changing extensions — chunk 8 landed the rule-row term
    /// below.
    ///
    /// Kiosk exclusion, three layers: (a) structural — kiosk pins the
    /// view to Board, the task view never mounts; (b) the explicit
    /// `!self.kiosk` term — freezes the decision even if kiosk view
    /// gating ever changes; (c) the driver only consults the wall
    /// second while this flag holds. Layer (b) is the test-pinned one.
    fn compute_seconds_surfaces_active(&mut self, state: &AppState, now: DateTime<Utc>) {
        let mut active = false;
        if !self.kiosk {
            // The concentration overlay's countdown (#496): the
            // remaining-time text changes with the wall second while
            // the flow holds — THIS term is the mandated extension
            // point (the doc above); never a second tick channel.
            // Unreachable on kiosk by the three-layer exclusion (the
            // flow cannot open there), frozen by this explicit
            // `!self.kiosk` term as before.
            let focus_overlay_active = matches!(
                self.view_state.flow(),
                crate::view_state::Flow::FocusMode { .. }
            );
            // The task view's elapsed total (#463's gate, D17).
            let task_view_active = self.view_state.view() == crate::view_state::ViewId::TaskDetail
                && self.view_state.subject_task().is_some_and(|subject| {
                    state
                        .tracking
                        .open_entries()
                        .any(|entry| entry.task == subject)
                });
            // The rule rows' per-second countdown (r4 D30, F23): the
            // subject's At-kinded rules carry the only seconds-changing
            // text in the section. The doc-comment's named extension
            // point — never a second tick channel.
            let at_rule_rows = self.view_state.view() == crate::view_state::ViewId::TaskDetail
                && self.view_state.subject_task().is_some_and(|subject| {
                    state.reminders.rules.values().any(|rule| {
                        rule.task == subject
                            && matches!(rule.kind, taskboard_domain::ReminderKind::At(_))
                    })
                });
            // The timers rows' per-second countdown (r4 D32, §2
            // decision 5's amendment): the unfired set is the only
            // timers text that changes with the wall second — an
            // awaiting-only map renders clamped zeros plus a fixed
            // instant, static, no tick owed. The doc-comment's named
            // extension point, discharged — never a second tick
            // channel.
            let timer_rows = !crate::presentation::timer_rows(state, now).is_empty();
            active = task_view_active || focus_overlay_active || at_rule_rows || timer_rows;
        }
        self.seconds_surfaces_active = active;
    }

    /// The D1 refresh, guarded (#463): the derive+diff half
    /// (reconcile, `models::push`, the live-menu re-grey) runs only
    /// when the loaded snapshot is not the one the models already
    /// carry. The cheap half — the §8 buffer wipe and the settings
    /// push — runs on every call: flows can end without outcomes, and
    /// the settings rows fold the liveness map and the settled
    /// replies, none of which live in the state Arc.
    ///
    /// Invariant for future callers: anything the derive reads that is
    /// NOT the state Arc (`view_state`, shell, keymap, due/picker state,
    /// the wall clock) must reach this method via [`Self::refresh_forced`]
    /// or behind a state change — the audit table in
    /// `.artifacts/plans/2026-10-31-unchanged-refresh-guard-plan.md` §5
    /// is the reference classification.
    pub(crate) fn refresh(&mut self, window: &AppWindow) {
        let state = self.state.load_full();
        let unchanged = self
            .last_derived
            .as_ref()
            .is_some_and(|derived| Arc::ptr_eq(derived, &state));
        if unchanged {
            self.retire_levels(window);
            self.push_settings(window);
            return;
        }
        self.refresh_forced(window);
    }

    /// The date combobox's filtered quick-list rows (r4 D26): the
    /// armed gate (round-5 F10) — disarmed = the full 96-slot grid
    /// (empty digits match everything); the first keystroke re-arms
    /// the per-keystroke narrowing. No armed editor renders no slots.
    fn quick_time_slots(
        editor: Option<&crate::level_state::DateEditorState>,
        time_text: &str,
    ) -> Vec<chrono::NaiveTime> {
        if editor.is_some_and(|e| e.picker.is_some()) {
            let digits = if editor.is_some_and(|e| e.time_filter_armed) {
                crate::due_edit::filter_digits(time_text)
            } else {
                String::new()
            };
            crate::due_edit::quarter_slots()
                .into_iter()
                .filter(|slot| crate::due_edit::quick_list_matches(&digits, *slot))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// The unguarded D1 pass: reconcile + full re-derive + the cheap
    /// tail. Every caller whose inputs moved outside the state Arc
    /// calls this directly (the plan §5 audit's forced set).
    #[allow(clippy::too_many_lines)] // the pass is one linear pipeline
    pub(crate) fn refresh_forced(&mut self, window: &AppWindow) {
        // The create-focus handoff's consumption (UX-R8 chunk-5 D-7,
        // US-5/AC-1): the stack's live ids diffed against the commit-
        // time snapshot — exactly one new id moves the focus (the
        // walk handoff's existing setter) and arms the scroll hint;
        // zero or >1 (a refused create, a racing capture) clear the
        // arm with NO handoff — the one-shot law (R-d).
        let create_focus_before_focused = self.consume_create_focus_arm();
        self.sync_view_context(window);
        let state = self.state.load_full();
        self.sync_settings(&state.settings);
        self.view_state.reconcile(&state);
        // The pinned object menu's disarm (J5 US-4 AC-2, #927): the
        // built-once model's subject died while the menu held — close
        // on the same reconciliation pass (bar + rows disarm together,
        // the CloseMenu path), never a frame late. A `None` subject
        // (stack/board menus, the main menu) never disarms; a
        // partially-deleted selection keeps its live members.
        if self.menu_kind == MenuKind::Object && self.menu_subject_dead(&state) {
            self.apply_close_menu(window);
        }
        // The walk's refresh-time normalization (J4 D-3): the cursor
        // clamps against the pushed row count, an emptied stage
        // advances through the symmetric skip law, a dead candidate
        // disarms.
        if let Flow::ReviewWalk {
            cadence, reference, ..
        } = self.view_state.walk_flow().unwrap_or(Flow::None)
        {
            let (rows, _) = self.walk_rows(&state, cadence, reference);
            let candidates = crate::presentation::walk_file_candidates(&state);
            self.view_state.clamp_review_walk(&state, rows, &candidates);
        }
        // After the reconcile — the reconciled view-state is what the
        // term reads. The pass's clock read feeds the timers term (r4
        // D32: one derivation, no second list to drift).
        self.compute_seconds_surfaces_active(&state, chrono::Utc::now());
        self.retire_levels(window);
        let chain = self.current_chain(window);
        let day = self.day_ctx();
        // The time combobox's pushed half (r4 D26): the filtered
        // quick-list rows over the static slots, and the nearest-slot
        // seek index against the picker's current time.
        // The editor's push half reads its level; without one the
        // mirrors render exactly the wiped state the old enforce pass
        // produced (empty buffer, the `--:--` value echo).
        // The quick options' open-time seed (task-detail-editor UX
        // plan US-2/D-7, N-6): the FIRST pass of an armed editor
        // without a picker-held time captures the device now's local
        // time-of-day — captured once, so the per-pass derivation
        // never snaps the highlight back to "now" over the user's
        // ladder walk (a picker-held time wins for the whole session).
        if let Some(editor) = self.view_state.date_editor_mut()
            && editor.time_open_seed.is_none()
            && editor
                .picker
                .as_ref()
                .is_none_or(|picker| picker.time().is_none())
        {
            // The clock seam's jurisdiction includes the seed capture
            // (#921): under a mount's frozen clock the seed derives
            // from the frozen instant, so an exact-artifact test reads
            // one oracle instead of racing the capture's wall clock.
            // Production is unchanged — every production hand-over
            // passes `None` (`UiSettings::clock`'s contract).
            editor.time_open_seed = Some(match self.frozen_now {
                Some(now) => now.with_timezone(&chrono::Local).time(),
                None => chrono::Local::now().time(),
            });
        }
        let editor = self.view_state.date_editor();
        let time_text = editor.map_or("--:--", |e| e.time_text.as_str());
        let time_slots = Self::quick_time_slots(editor, time_text);
        // The quick options' focused slot (task-detail-editor UX plan
        // US-2/D-7, N-6): the flyout's open-time seed (captured once —
        // see the lazy capture above) — a picker-held time wins
        // (re-opening after a typed entry stays on the entry), else
        // the device now's local time-of-day at capture. The list
        // opens scrolled to the current time, never the list head, and
        // the ladder walk owns the highlight afterwards (no per-pass
        // recompute snap-back).
        let time_nearest_row =
            crate::due_edit::nearest_slot(&time_slots, editor.and_then(|e| e.time_open_seed));
        let now2 = chrono::Utc::now();
        let due = crate::models::DueEditorPush {
            text: editor.map_or("", |e| e.buffer.as_str()),
            echo: editor.map_or("", |e| e.echo.as_str()),
            picker: editor.and_then(|e| e.picker.as_ref()),
            focus_on_mount: self.focus_handover == FocusHandover::Grid,
            field_focus: self.focus_handover == FocusHandover::Field,
            horizon: self.due_soon_horizon,
            time_text,
            time_valid: crate::due_edit::time_field_valid(time_text),
            time_slots: &time_slots,
            time_nearest_row,
            date_valid: crate::due_edit::field_validity(
                editor.map_or("", |e| e.buffer.as_str()),
                now2,
                &chrono::Local,
            ),
            syntax_hint: crate::due_edit::DUE_SYNTAX_HINT,
            time_hint: crate::due_edit::TIME_SYNTAX_HINT,
            offset_valid: crate::presentation::rule_when_validity(
                &self.rule_offset_draft,
                now2,
                &chrono::Local,
            ),
            offset_hint: crate::presentation::RULE_WHEN_SYNTAX_HINT,
        };
        // The focus hand-over is one-shot: consumed by this push.
        self.focus_handover = FocusHandover::None;
        #[cfg(test)]
        {
            self.derive_passes += 1;
        }
        // The recur row's flags ride the pass (issue #506): they
        // clear when the subject stops recurring — the buffers never
        // outlive their surface.
        let subject_recurs = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|t| t.recur.is_some());
        if !subject_recurs {
            self.recur_pause_open = false;
            self.recur_confirm_armed = false;
        }
        // The armed habit subject absorbs when its habit dies (the
        // stale-index law at the id-table level — the armed flag is
        // id-keyed since H2, so a vanished subject simply disarms via
        // the open menu's re-projection).
        // The armed habit subject absorbs when its habit dies: a dead
        // subject disarms on the pass (the buffers-never-outlive rule).
        if !self.habit_convert_armed.is_some_and(|armed| {
            state
                .habits
                .habits
                .get(&armed)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        }) {
            self.habit_convert_armed = None;
        }
        // The armed remove absorbs when its habit dies (the same
        // stale-index law; the armed flag is id-keyed).
        if !self.habit_remove_armed.is_some_and(|armed| {
            state
                .habits
                .habits
                .get(&armed)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        }) {
            self.habit_remove_armed = None;
        }
        // The armed restore absorbs when its habit leaves the roster's
        // restorable set (the stale law inverted: a restored or
        // re-created habit has nothing left to resurrect).
        if !state
            .roster
            .tombstoned_habits
            .iter()
            .any(|row| Some(row.id) == self.habit_restore_armed)
        {
            self.habit_restore_armed = None;
        }
        // The pause editor's one-shot subject absorbs when its habit
        // dies (the same stale-index law; HF1) — the open editor
        // unmounts with it.
        if !self.habit_pause_target.is_some_and(|armed| {
            state
                .habits
                .habits
                .get(&armed)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        }) {
            self.habit_pause_open = false;
            self.habit_pause_target = None;
        }
        // The editor's draft dies with its subject (the editor's
        // escape/cancel discipline — the buffer-never-outlives rule).
        if self.view_state.subject_habit().is_none() {
            self.habit_edit_draft = None;
        }
        let recur = crate::models::RecurRowPush {
            pause_open: self.recur_pause_open,
            confirm_open: self.recur_confirm_armed,
        };
        // The priority pickers' flag (UX R-3 D-1): it never outlives
        // its subject (the recur flags' rule) — a dead/switched
        // subject clears it on the pass.
        if self.view_state.subject_task().is_none() {
            self.priority_open = None;
        }
        // The labels form dies with its overlay (the same rule): any
        // pass that reads a non-Labels flow clears the draft.
        if !matches!(
            self.view_state.flow(),
            crate::view_state::Flow::Labels { .. }
        ) {
            self.label_form = None;
        }
        let priority = crate::models::PriorityPickerPush {
            open: self.priority_open,
        };
        let label_form = self
            .label_form
            .as_ref()
            .map(|form| crate::models::LabelFormPush {
                open: true,
                title: form.title.clone(),
                color: form.color.clone(),
            });
        // M9: the always-on effort row's seed — the armed buffer when
        // the user typed (the lazy editor exists), the subject's
        // stored text form otherwise (empty when unset). The field is
        // always mounted; there is no open flag any more.
        let effort_stored = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .and_then(|t| t.effort.as_ref())
            .map(|e| e.text_form())
            .unwrap_or_default();
        let effort_editor = Some(crate::models::EffortEditorPush {
            buffer: self
                .effort_editor
                .as_ref()
                .map_or(effort_stored.as_str(), |editor| editor.buffer.as_str()),
        });
        // The armed strip's mirror (the track-entries armed-index
        // rule): the armed row's canonical roster position, or none.
        // US-6 pass-side clear (#840, task-detail-editor UX plan
        // D-11): an armed identity that no longer resolves against the
        // live roster cannot outlive its row — the lingering `Some`
        // would leave the strip unrenderable (mirror index −1) and eat
        // the next Escape rung. The `priority_open` subject-death
        // rule's shape.
        if let Some(armed) = self.label_delete_armed
            && state.labels.get(&armed).is_none_or(|label| label.deleted)
        {
            self.label_delete_armed = None;
        }
        // The armed label restore absorbs when its label leaves the
        // roster's tombstoned set (the stale law — a restored or
        // re-created label has nothing left to resurrect).
        if let Some(armed) = self.label_restore_armed
            && state
                .roster
                .tombstoned_labels
                .iter()
                .all(|row| row.id != armed)
        {
            self.label_restore_armed = None;
        }
        // The armed entry restore absorbs when the journal id left the
        // roster (restored, re-created, or evicted — the bounded
        // honesty of the retention law).
        if let Some(armed) = self.entry_restore_armed
            && state
                .roster
                .deleted_entries
                .iter()
                .all(|row| row.journal_id != armed)
        {
            self.entry_restore_armed = None;
        }
        // The armed rule clear absorbs when its subject has no rules
        // left (D-2: never ceremony a no-op).
        if self.rules_clear_armed
            && self
                .view_state
                .subject_task()
                .is_none_or(|task| state.reminders.rules.values().all(|rule| rule.task != task))
        {
            self.rules_clear_armed = false;
        }
        // The overlay roster's board (tag-color plan D-8): the board
        // mount names its subject directly; the detail mount derives
        // the subject task's board.
        let subject_board = match self.view_state.labels_subject() {
            Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
            _ => self
                .view_state
                .subject_task()
                .and_then(|t| state.tasks.get(&t))
                .and_then(|t| state.stacks.get(&t.stack))
                .map(|stack| stack.board),
        };
        let label_armed_index = self
            .label_delete_armed
            .and_then(|id| {
                state
                    .labels
                    .values()
                    .filter(|l| !l.deleted && Some(l.board) == subject_board)
                    .position(|l| l.id == id)
            })
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(-1);
        // The break-rule roster's armed index (T-14, the label form's
        // armed-index convention): the armed id's position in the
        // canonical `(threshold, id)` order this pass derives — a
        // vanished subject's index is -1 (no strip renders).
        let break_rule_armed_index = self
            .break_rule_armed
            .and_then(|armed| {
                crate::presentation::break_rule_rows(&state, self.view_state.board())
                    .iter()
                    .position(|row| row.rule == armed)
            })
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(-1);
        self.models.walk_abort_armed = self.walk_abort_armed;
        // The restore rosters' pushed halves (W-2/W-9, D-4): the
        // deleted sections' rows and armed mirrors. Kiosk pushes empty
        // sections — the affordances render absent, the CLI carries
        // the kiosk restore path.
        let (label_deleted_rows, label_restore_armed_index): (Vec<slint::SharedString>, i32) =
            if self.kiosk {
                (Vec::new(), -1)
            } else {
                let overlay_board = match self.view_state.labels_subject() {
                    Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
                    _ => self
                        .view_state
                        .subject_task()
                        .and_then(|t| state.tasks.get(&t))
                        .and_then(|t| state.stacks.get(&t.stack))
                        .map(|stack| stack.board),
                };
                let rows: Vec<&taskboard_domain::RestoreRow<taskboard_domain::LabelId>> = state
                    .roster
                    .tombstoned_labels
                    .iter()
                    .filter(|row| {
                        state.labels.get(&row.id).is_some_and(|label| {
                            label.deleted && Some(label.board) == overlay_board
                        })
                    })
                    .collect();
                let armed = self
                    .label_restore_armed
                    .and_then(|id| rows.iter().position(|row| row.id == id))
                    .and_then(|p| i32::try_from(p).ok())
                    .unwrap_or(-1);
                (
                    rows.into_iter()
                        .map(|row| slint::SharedString::from(row.name.as_str()))
                        .collect(),
                    armed,
                )
            };
        window.set_label_overlay_has_deleted(!label_deleted_rows.is_empty());
        window.set_label_overlay_deleted_rows(ModelRc::new(slint::VecModel::from(
            label_deleted_rows,
        )));
        window.set_label_overlay_restore_armed_index(label_restore_armed_index);
        let (entry_deleted_rows, entry_restore_armed_index): (Vec<slint::SharedString>, i32) =
            if self.kiosk {
                (Vec::new(), -1)
            } else {
                let armed = self
                    .entry_restore_armed
                    .and_then(|id| {
                        state
                            .roster
                            .deleted_entries
                            .iter()
                            .position(|row| row.journal_id == id)
                    })
                    .and_then(|p| i32::try_from(p).ok())
                    .unwrap_or(-1);
                (
                    state
                        .roster
                        .deleted_entries
                        .iter()
                        .map(|row| slint::SharedString::from(row.label.as_str()))
                        .collect(),
                    armed,
                )
            };
        window.set_track_has_deleted(!entry_deleted_rows.is_empty());
        window.set_track_deleted_rows(ModelRc::new(slint::VecModel::from(entry_deleted_rows)));
        window.set_track_restore_armed_index(entry_restore_armed_index);
        // The clear-all row's pushed halves (W-4): the subject's rule
        // count (the row's consequence names it), the clearable
        // verdict (D-2: zero rules is the disabled row, never a
        // ceremony), and the armed mirror.
        let clear_count = self.view_state.subject_task().map_or(0, |task| {
            state
                .reminders
                .rules
                .values()
                .filter(|rule| rule.task == task)
                .count()
        });
        if self.rules_clear_armed && clear_count == 0 {
            self.rules_clear_armed = false;
        }
        window.set_rules_clearable(clear_count > 0);
        window.set_rules_clear_count(i32::try_from(clear_count).unwrap_or(i32::MAX));
        window.set_rule_clear_armed(self.rules_clear_armed);
        self.tables = crate::models::push(
            window,
            &state,
            &self.view_state,
            &self.keymap,
            &chain,
            day,
            self.calendar(),
            self.kiosk,
            &self.reminder_defaults,
            self.focus.as_ref(),
            &mut self.models,
            &due,
            recur,
            priority,
            label_form.as_ref(),
            effort_editor.as_ref(),
            label_armed_index,
            break_rule_armed_index,
            (self.reflection_active, self.reflection_armed),
            self.habit_edit_draft.as_ref(),
        );
        // The planner cursor's clamp/prune (US-15, #1041 — the K4
        // law's refresh clamp, the rule cursor's reconcile arm): a
        // dead id, a foreign view, or a pane that no longer holds the
        // cursor clears it — never a stale row answering a stroke.
        if self.view_state.view() != crate::view_state::ViewId::PlanNextDay
            || !self
                .view_state
                .plan_cursor()
                .is_some_and(|cursor| self.plan_pane_rows().contains(&cursor))
        {
            self.view_state.clear_plan_cursor();
        }
        // The create-focus handoff's hint half (UX-R8 chunk-5 D-7):
        // the tables are fresh — edge-trigger the FocusScrollHint
        // global toward the newly focused card (the machinery chunk 4
        // extended; the board/compact ListViews consume it).
        if let Some(handoff) = create_focus_before_focused {
            self.push_focus_scroll_hint(window, handoff.before);
        }
        // Round-3 D27c: engine syncs re-grey a live menu — the pages
        // rebuild from the fresh state while the menu holds (the
        // cursor's length bound rides along). UX-R8 D-1 (N-11): the
        // re-derivation is scoped to the main menu — an object menu's
        // model is built once at open and never swapped (the repro
        // journey pins the swap this guard forbids).
        if self.shell.menu.open && self.menu_kind == MenuKind::Main {
            let flow = self.view_state.flow().clone();
            let states = self.entry_states(&state, &flow);
            self.root_entries = self.mint_root_entries(&states);
            self.menu_entries = match self.active_submenu() {
                Some(crate::menu::Submenu::Group(group)) => {
                    menu::group_page(&self.keymap, &self.menu_chain, &states, group)
                }
                Some(crate::menu::Submenu::Task(family)) => {
                    // The habit menu's submenu page (H2 D4): the
                    // Convert family reads the habit verdicts (the
                    // armed confirm's flip + the cancel row).
                    if matches!(
                        self.menu_row_subject,
                        Some(crate::presentation::RowSubject::Habit(_))
                    ) {
                        self.habit_submenu_page(family)
                    } else {
                        let subject = self.current_task_subject();
                        menu::task_submenu_page(
                            &self.keymap,
                            &self.menu_chain,
                            subject,
                            &states,
                            family,
                        )
                    }
                }
                None => self.root_entries.clone(),
            };
            self.shell.menu.len = self.menu_entries.len();
            self.sync_menu_models(window);
        }
        self.push_settings(window);
        // The setter popover's pushed halves (r4 D33): cheap, every
        // pass — the popover reads them only while open.
        self.push_timer_setter(window);
        // The entries form's pushed halves (r4 D-TS4): the same
        // every-pass shape; the wipe rides the push.
        self.push_track_form(window);
        // The create form's pushed halves (round-7 D-7.2): the armed
        // flag and the quota validity are adapter-owned; cheap every
        // pass (the setter popover's shape).
        window.set_habits_create_open(self.habit_create_open);
        // The pause editor's pushed halves (HF1, W-3): the armed flag
        // and the desktop mount gate are adapter-owned; cheap every
        // pass (the create form's shape).
        window.set_habits_pause_open(self.habit_pause_open);
        window.set_habits_pause_mountable(!self.kiosk);
        let (quota_validity, quota_hint) =
            crate::presentation::habit_quota_validity(&self.habit_create_quota);
        window.set_habits_quota_validity(match quota_validity {
            crate::presentation::QuotaValidity::Neutral => crate::FieldValidity::Neutral,
            crate::presentation::QuotaValidity::Valid => crate::FieldValidity::Valid,
            crate::presentation::QuotaValidity::Invalid => crate::FieldValidity::Invalid,
        });
        window.set_habits_quota_error_hint(SharedString::from(quota_hint));
        // The pass materialized: close the cache only now — a
        // re-entrant pass before this line would see the stale cache
        // and derive again, never skip a needed pass.
        self.last_derived = Some(state);
        // The focus-origin tail funnel (US-1/W-1): a reconcile that
        // dropped an overlay's flow (the vanished-subject rule) closed
        // the site *after* this pass's entry funnel ran — the restore
        // fires here, over the freshly pushed models, so the origin's
        // row is on the page the fact names.
        self.sync_focus_origin(window);
    }

    /// Recomposes the setter's grammar text + fire instant from the
    /// custom row's state (r4 D-T4): the composer emits existing-
    /// grammar text (`at <RFC-3339 local-offset>`) re-parsed by the
    /// one parser; a DST-gap pick composes `None` — the draft becomes
    /// the unparseable `"at "` (the invalid flavour, the honest
    /// refusal), and the funnel never emits a miss.
    fn recompose_timer_setter(&mut self) {
        let Some(time) = self.setter_time else {
            self.setter_composed = None;
            self.setter_fire = None;
            self.timer_when_draft.clear();
            return;
        };
        let now = chrono::Utc::now();
        let minutes = u32::try_from(crate::due_edit::minutes_of_day(time)).unwrap_or(0);
        self.setter_composed = crate::presentation::compose_timer_at(
            minutes,
            self.setter_day_offset,
            now,
            &chrono::Local,
        );
        self.setter_fire = crate::presentation::compose_timer_instant(
            minutes,
            self.setter_day_offset,
            now,
            &chrono::Local,
        );
        self.timer_when_draft = self
            .setter_composed
            .clone()
            .unwrap_or_else(|| "at ".to_owned());
    }

    /// Clears the setter's draft state (dismiss and commit share it —
    /// drafts are widget-local, nothing survives the panel).
    fn clear_timer_setter(&mut self) {
        self.setter_filter_armed = false;
        self.setter_time = None;
        "--:--".clone_into(&mut self.setter_time_text);
        self.setter_day_offset = 0;
        self.setter_insist = false;
        self.setter_composed = None;
        self.setter_fire = None;
        self.timer_when_draft.clear();
    }

    /// The setter popover's pushed halves (r4 D33): the `TimeField`'s
    /// contract — the adapter's canonical mirror re-seeds the widget
    /// buffer only when the picker state moved, never while the user
    /// types (the typed text echoes verbatim through the draft).
    fn push_timer_setter(&self, window: &AppWindow) {
        let digits = if self.setter_filter_armed {
            crate::due_edit::filter_digits(&self.setter_time_text)
        } else {
            String::new()
        };
        let slots: Vec<chrono::NaiveTime> = crate::due_edit::quarter_slots()
            .into_iter()
            .filter(|slot| crate::due_edit::quick_list_matches(&digits, *slot))
            .collect();
        let nearest = crate::due_edit::nearest_slot(&slots, self.setter_time);
        window.set_timer_setter_time_text(SharedString::from(self.setter_time_text.clone()));
        window.set_timer_setter_time_validity(
            match crate::due_edit::time_field_valid(&self.setter_time_text) {
                None => crate::FieldValidity::Neutral,
                Some(true) => crate::FieldValidity::Valid,
                Some(false) => crate::FieldValidity::Invalid,
            },
        );
        window.set_timer_setter_time_hint(SharedString::from(crate::due_edit::TIME_SYNTAX_HINT));
        // The fresh-VecModel-per-pass shape (the date editor's slot
        // push, models.rs:1614): the flyout reads it only while open.
        window.set_timer_setter_time_slots(ModelRc::new(slint::VecModel::from(
            slots
                .iter()
                .map(|slot| crate::TimeSlotRow {
                    label: SharedString::from(crate::due_edit::format_time_hm(*slot)),
                    minutes: crate::due_edit::minutes_of_day(*slot),
                })
                .collect::<Vec<crate::TimeSlotRow>>(),
        )));
        window.set_timer_setter_time_nearest_row(i32::try_from(nearest).unwrap_or(0));
        window.set_timer_setter_fire_time(SharedString::from(self.setter_fire.map_or(
            String::new(),
            |fire| {
                format!(
                    "@ {}",
                    crate::presentation::iso_local_fire_time(fire, &chrono::Local)
                )
            },
        )));
        window.set_timer_setter_commit_enabled(
            crate::presentation::timer_when_validity(
                &self.timer_when_draft,
                chrono::Utc::now(),
                &chrono::Local,
            ) == Some(true),
        );
    }

    /// The entries form's pushed halves (r4 D-TS4): the tri-state
    /// mirrors over the live parse (the due editor's shape). Runs
    /// every pass — cheap; the panel reads them only while mounted.
    /// The wipe rides here too: the buffers never outlive the flow
    /// (the §8 invariant's track-form twin).
    #[allow(clippy::too_many_lines)] // one pushed half per property, kept flat
    fn push_track_form(&mut self, window: &AppWindow) {
        // The buffers live in the form's level (W5b D-H2): the drop
        // is the wipe, the filter latches ride inside the state — no
        // per-pass wipe pass here.
        // The armed delete dies with the overlay (D-TS5's
        // disarm-on-close; state-truth, nothing ticks) — but it
        // survives the form surface's ladder step back to the list.
        if !matches!(
            self.view_state.flow(),
            Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. }
        ) {
            self.track_entry_target = None;
        }
        // The armed row's index resolves through the id table — a
        // refresh that drops the armed row renders unarmed (the
        // stale-index no-op).
        let armed_index = self
            .track_entry_target
            .and_then(|armed| {
                self.tables
                    .track_entries
                    .iter()
                    .position(|entry| *entry == armed)
            })
            .map_or(-1, |position| i32::try_from(position).unwrap_or(-1));
        window.set_track_armed_index(armed_index);
        if armed_index < 0 {
            self.track_entry_target = None;
        }
        let Some(form) = self.view_state.track_form() else {
            window.set_track_form_open(false);
            return;
        };
        let today = chrono::Local::now().date_naive();
        let parse = crate::presentation::parse_track_form(
            &form.date_text,
            &form.start_text,
            &form.end_text,
            form.seed.is_some(),
            today,
            &chrono::Local,
        );
        let validity = |valid: Option<bool>| match valid {
            None => crate::FieldValidity::Neutral,
            Some(true) => crate::FieldValidity::Valid,
            Some(false) => crate::FieldValidity::Invalid,
        };
        let to_slots = |slots: &[chrono::NaiveTime]| {
            ModelRc::new(slint::VecModel::from(
                slots
                    .iter()
                    .map(|slot| crate::TimeSlotRow {
                        label: SharedString::from(crate::due_edit::format_time_hm(*slot)),
                        minutes: crate::due_edit::minutes_of_day(*slot),
                    })
                    .collect::<Vec<crate::TimeSlotRow>>(),
            ))
        };
        let slots_for = |text: &str, armed: bool| -> Vec<chrono::NaiveTime> {
            let digits = if armed {
                crate::due_edit::filter_digits(text)
            } else {
                String::new()
            };
            crate::due_edit::quarter_slots()
                .into_iter()
                .filter(|slot| crate::due_edit::quick_list_matches(&digits, *slot))
                .collect()
        };
        let start_slots = slots_for(&form.start_text, form.start_filter_armed);
        let end_slots = slots_for(&form.end_text, form.end_filter_armed);
        let picker = self.view_state.picker().map(|level| &level.picker);
        let grid = picker.map_or_else(
            || ModelRc::new(slint::VecModel::from(Vec::new())),
            |picker| {
                use chrono::Datelike as _;
                let now = chrono::Local::now().date_naive();
                ModelRc::new(slint::VecModel::from(
                    picker
                        .grid()
                        .chunks(7)
                        .map(|chunk| {
                            ModelRc::new(slint::VecModel::from(
                                chunk
                                    .iter()
                                    .map(|cell| {
                                        let date = cell.date;
                                        crate::DueDayModel {
                                            label: SharedString::from(
                                                date.map_or(String::new(), |d| d.day().to_string()),
                                            ),
                                            blank: date.is_none(),
                                            cursor: date.is_some_and(|d| d == picker.cursor()),
                                            today: date.is_some_and(|d| d == now),
                                            kind: crate::models::to_slint_due_kind(date.map_or(
                                                crate::presentation::DueKind::None,
                                                |d| {
                                                    crate::due_edit::cell_kind(
                                                        d,
                                                        now,
                                                        self.due_soon_horizon,
                                                    )
                                                },
                                            )),
                                        }
                                    })
                                    .collect::<Vec<crate::DueDayModel>>(),
                            ))
                        })
                        .collect::<Vec<ModelRc<crate::DueDayModel>>>(),
                ))
            },
        );
        window.set_track_form_open(true);
        window.set_track_form_date_text(SharedString::from(form.date_text.clone()));
        window.set_track_form_date_validity(validity(parse.date_valid));
        window.set_track_form_date_hint(SharedString::from(crate::presentation::TRACK_DATE_HINT));
        // The grid's mirror follows the picker child's presence (N2):
        // the child's death closes the grid on the next pass.
        window.set_track_form_grid_open(picker.is_some());
        window.set_track_form_grid(grid);
        window.set_track_form_start_text(SharedString::from(form.start_text.clone()));
        window.set_track_form_start_validity(validity(parse.start_valid));
        window.set_track_form_start_hint(SharedString::from(crate::due_edit::TIME_SYNTAX_HINT));
        window.set_track_form_start_slots(to_slots(&start_slots));
        window.set_track_form_start_nearest_row(
            i32::try_from(crate::due_edit::nearest_slot(&start_slots, None)).unwrap_or(0),
        );
        window.set_track_form_end_text(SharedString::from(form.end_text.clone()));
        window.set_track_form_end_validity(validity(parse.end_valid));
        window.set_track_form_end_hint(SharedString::from(crate::due_edit::TIME_SYNTAX_HINT));
        window.set_track_form_end_slots(to_slots(&end_slots));
        window.set_track_form_end_nearest_row(
            i32::try_from(crate::due_edit::nearest_slot(&end_slots, None)).unwrap_or(0),
        );
    }

    #[cfg(test)]
    pub(crate) fn track_form_mut_for_test(&mut self) -> Option<&mut TrackFormState> {
        self.view_state.track_form_mut()
    }

    #[cfg(test)]
    pub(crate) fn test_armed_delete_is(&self, entry: taskboard_domain::TimeEntryId) -> bool {
        self.track_entry_target == Some(entry)
    }

    #[cfg(test)]
    pub(crate) fn test_armed_delete_is_none(&self) -> bool {
        self.track_entry_target.is_none()
    }

    #[cfg(test)]
    pub(crate) fn test_tracked_today_rows(&self) -> Vec<taskboard_domain::TaskId> {
        self.tables.tracked_today.clone()
    }

    /// Seeds the form's adapter state for the armed flow (the begin
    /// seam's shape: inside the armed window, before the outcomes'
    /// refresh). Add seeds today's date and two unset times; edit
    /// seeds the entry's local date and `HH:MM` times — an open
    /// entry's end seeds empty (the keep-open semantics, D-TS4).
    #[cfg(test)]
    pub(crate) fn seed_track_form_for_test(&mut self, seed: Option<taskboard_domain::TimeEntryId>) {
        self.seed_track_form(seed);
    }

    /// The track-form's clock jurisdiction (#1214, the #921/#660
    /// precedent): the parse anchor `today` derives through the
    /// frozen-clock seam when one is mounted (`UiSettings::clock`'s
    /// contract — every production hand-over passes `None` and reads
    /// the wall clock exactly as before), so a test pinning
    /// clock-derived form content reads one oracle and a local
    /// midnight straddling a run cannot invert the seeded span.
    fn track_form_today(&self) -> chrono::NaiveDate {
        self.frozen_now
            .map_or_else(chrono::Local::now, |now| now.with_timezone(&chrono::Local))
            .date_naive()
    }

    fn seed_track_form(&mut self, seed: Option<taskboard_domain::TimeEntryId>) {
        use chrono::TimeZone as _;
        let Flow::TrackEntryForm { .. } = self.view_state.flow() else {
            return;
        };
        let state = self.state.load_full();
        let today = self.track_form_today();
        let entry = seed.and_then(|id| state.tracking.entries.get(&id));
        let start_local = entry.map(|e| chrono::Local.from_utc_datetime(&e.start.naive_utc()));
        let Some(form) = self.view_state.track_form_mut() else {
            return;
        };
        *form = TrackFormState {
            seed,
            date_text: start_local.map_or(today.format("%Y-%m-%d").to_string(), |l| {
                l.format("%Y-%m-%d").to_string()
            }),
            start_text: start_local.map_or("--:--".to_owned(), |l| l.format("%H:%M").to_string()),
            end_text: entry.and_then(|e| e.end).map_or_else(
                || "--:--".to_owned(),
                |end| {
                    chrono::Local
                        .from_utc_datetime(&end.naive_utc())
                        .format("%H:%M")
                        .to_string()
                },
            ),
            start_filter_armed: false,
            end_filter_armed: false,
            anchor: crate::level_state::FocusAnchor::View,
        };
    }

    /// The commit funnel (D-TS4, F24): the live validity table is
    /// authoritative — the draft fills the one-shot ctx fields only
    /// when it passes (the form's subject arms `track_target`: the
    /// add resolves over the viewed task, never the focus fallback),
    /// the router dispatches the existing verb, and the surface steps
    /// back to the list. A refusal keeps the form open with the
    /// invalid flavour (never swallowed silence).
    #[cfg(test)]
    pub(crate) fn commit_track_form_for_test(&mut self, window: &AppWindow) -> bool {
        self.commit_track_form(window)
    }

    fn commit_track_form(&mut self, window: &AppWindow) -> bool {
        let Some(form) = self.view_state.track_form() else {
            return false;
        };
        // The form's subject rides the flow, not the focus: the
        // one-shot target seam (the tracking chip's rule — the subject
        // is not guaranteed `focused_task`, whose fallback would
        // swallow or retarget the add).
        let Flow::TrackEntryForm { task, .. } = self.view_state.flow() else {
            return false;
        };
        let subject = *task;
        let today = self.track_form_today();
        let parse = crate::presentation::parse_track_form(
            &form.date_text,
            &form.start_text,
            &form.end_text,
            form.seed.is_some(),
            today,
            &chrono::Local,
        );
        let Some(draft) = parse.draft else {
            self.refresh_forced(window);
            return false;
        };
        self.track_target = Some(subject);
        self.track_entry_draft = Some(draft);
        self.track_entry_target = form.seed;
        let action = if form.seed.is_some() {
            ActionId::TaskTrackEntryAdjust
        } else {
            ActionId::TaskTrackEntryLog
        };
        self.route(window, action);
        // The commit completes the form child (N1): the parent list
        // ignores the payload-less completion and stays — the overlay
        // stays open (the multi-add posture).
        let snapshot = self.state.load();
        let outcomes = self.view_state.complete_track_entry_form(&snapshot);
        self.apply_outcomes(window, &outcomes);
        self.finish_mutation(window);
        true
    }

    /// The flow-owned one-shots' reconciliation (cross-surface
    /// friction plan D-6, #763, AC-3): a one-shot left in the adapter
    /// after its owner flow ended is stale — the per-`Flow`-kind
    /// ownership table clears it at the retire drain. Armed-strip
    /// one-shots (`habit_convert_target`, `series_target`,
    /// `convert_target`, `pattern_draft`) are deliberately NOT
    /// flow-owned: their law is the arm seams' re-arm-before-every-
    /// route (defense-in-depth, unchanged) and their flags disarm via
    /// the existing Cancel/settle paths. The walk/picker children own
    /// none.
    fn reconcile_one_shots(&mut self, retired: &[crate::surfaces::Level]) {
        for level in retired {
            match level.flow() {
                Some(crate::view_state::Flow::EditRecur { .. }) => {
                    self.recur_edit_target = None;
                    self.recur_draft_rule = None;
                }
                Some(crate::view_state::Flow::TrackEntryForm { .. }) => {
                    self.track_entry_draft = None;
                    self.track_entry_target = None;
                }
                _ => {}
            }
        }
    }

    /// The retire drain (W5b D3): the retired levels' exit
    /// side-effects ride here, one consumption per level — never a
    /// per-flow wipe rule. The flat chain's only parent anchor is
    /// `View`, whose restore the flowless hand-off in
    /// `apply_outcomes` already performs, so the drain adds no focus
    /// call; the date editor's grid disarm is the first side-effect
    /// consumer (the date family's commit).
    fn retire_levels(&mut self, window: &AppWindow) {
        let retired = self.view_state.drain_retired();
        if retired.is_empty() {
            return;
        }
        // The stale one-shot reconciliation (cross-surface friction
        // plan D-6, #763): flow-owned one-shots die with their level —
        // consumption is not ownership.
        self.reconcile_one_shots(&retired);
        // The flat chain's levels never carry a non-view parent
        // anchor (D-H7); nested anchors consume at the drain from
        // chunk 3's ports on.
        debug_assert!(retired.iter().all(|level| matches!(
            level.state().anchor(),
            crate::level_state::FocusAnchor::None | crate::level_state::FocusAnchor::View
        )));
        // The picker dies with its level (§8): the grid cannot hold
        // focus any more, so the `DatePicker` context disarms here as
        // well as through the focus seam — a missed focus-loss on
        // unmount can never leave the context armed over the next
        // editor (R-C's exactness).
        if retired
            .iter()
            .any(|level| matches!(level.state(), crate::level_state::LevelState::DateEditor(_)))
        {
            self.shell.date_picker_grid = false;
            window.global::<KeyboardState>().set_date_picker_grid(false);
        }
        // The form's picker child dies with its level (N2): the grid
        // mirror cannot wait for a refresh that might not come.
        if retired
            .iter()
            .any(|level| matches!(level.state(), crate::level_state::LevelState::Picker(_)))
        {
            window.set_track_form_grid_open(false);
        }
    }

    /// §8 (W-SM-c), W5b D-H2: the date editor's buffers live in the
    /// `EditDate` level — non-empty ⇔ armed is a construction fact,
    /// the drop is the wipe, and the drain ([`Self::retire_levels`])
    /// owns the exit's grid disarm. There is no per-pass wipe pass
    /// any more.
    /// Whether the window is at the compact breakpoint (D17).
    fn is_compact(window: &AppWindow) -> bool {
        window.global::<Responsive>().get_current_breakpoint() == Breakpoint::Compact
    }

    /// The per-kind settle, adapter side (round-3 W-SM-b, plan §2):
    /// the pure settle fold applied, plus the adapter-owned
    /// attachments — the due picker dies with the due flow, the
    /// gesture buffers (`width_drag`/`scale_drag`, §8) die with the
    /// surfaces they address, and a settled drag tears the native
    /// runtime gesture down (`PointerExited` is the one public
    /// cancel, [`Self::teardown_runtime_drag`]). Every settle trigger
    /// funnels here — the L1 focus-loss and L2 pointer seams (LOCAL in
    /// the D-H4 classification), the leaving effects, the `OpenMenu`
    /// modal settle, and the resolver's CancelOnly/SettleThenRun/
    /// Replace cells (FOREIGN) — so the pure side applies the
    /// innermost-first fold ([`ViewState::settle_foreign`]); at the
    /// reachable depths (at most one flow level live) that fold
    /// degenerates to the local top-level settle, byte-identically.
    /// Idempotent on an empty chain.
    fn settle_flow(&mut self, window: &AppWindow, state: &taskboard_domain::AppState) {
        // §8: the settle step owns the gesture buffers — they are
        // flow-independent by construction, and a leaving effect
        // unmounts the surfaces that would consume them (the release
        // can never arrive). Dropped before the None early-return: a
        // settling pass with no flow armed still retires them.
        self.width_drag = None;
        self.scale_drag = None;
        // UX-R8 D-4's settle arm: every foreign settle clears the
        // selection alongside the flow settle — even with no flow
        // level live (the empty-selection early return below must
        // not skip it).
        if !self.view_state.selection().is_empty() {
            let outcomes = self.view_state.clear_selection();
            self.apply_outcomes(window, &outcomes);
        }
        let flow = self.view_state.flow().clone();
        if matches!(flow, Flow::None) {
            return;
        }
        let was_drag = matches!(flow, Flow::DragMove { .. });
        let outcomes = self.view_state.settle_foreign(state);
        self.apply_outcomes(window, &outcomes);
        if was_drag {
            Self::teardown_runtime_drag(window);
        }
    }

    // -- the review walk (DR-7 chunk 3, #638; plan D-W3) ----------------

    /// The per-pass row bundle for the walk (plan D-W2/D-W4): every
    /// row set derives from `AppState` + the flow's window on every
    /// read — nothing about the rows is stored.
    fn walk_rows(
        &self,
        state: &taskboard_domain::AppState,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
    ) -> (crate::view_state::WalkRows, taskboard_domain::ReviewWindow) {
        let calendar = self.calendar();
        let now = self.day_ctx().now;
        let window = taskboard_domain::review_window(cadence, reference, calendar);
        let rows = crate::view_state::WalkRows {
            inbox: crate::presentation::walk_inbox_rows(state).len(),
            carry: crate::presentation::walk_carry_rows(&state.tasks, cadence, window, now).len(),
        };
        (rows, window)
    }

    /// The opener's reference date (plan D-W6): the due window's day
    /// when `review_due` names one, else the current day window (the
    /// evening review before the boundary).
    fn walk_reference(&self, state: &taskboard_domain::AppState) -> chrono::NaiveDate {
        let calendar = self.calendar();
        let now = self.day_ctx().now;
        taskboard_domain::review_due(&state.reflection, calendar, now)
            .map_or_else(|| calendar.today(now), |due| due.window().day())
    }

    /// The abort-confirm helpers (J4 D-6): `End review` closes the
    /// walk (decisions already made are applied `StateCommand`s — the
    /// command journal keeps them; un-committed drafts die with the
    /// walk, the documented child-death rule) and restores focus;
    /// `Keep reviewing` drops the arm, the walk stays verbatim.
    fn abort_accept(&mut self, window: &AppWindow) -> Vec<FlowOutcome> {
        self.walk_abort_armed = false;
        let outcomes = self.view_state.close_review_walk();
        Self::reset_editing_facts(window);
        window.invoke_refocus_view();
        outcomes
    }

    fn abort_dismiss(&mut self) -> Vec<FlowOutcome> {
        self.walk_abort_armed = false;
        vec![FlowOutcome::RefreshNow]
    }

    /// The abort arm rung (J4 R-2): the Entry stage — nothing decided,
    /// nothing typed — closes silently (the one silent exit); any
    /// other live stage arms the confirm.
    fn abort_arm(&mut self) -> Vec<FlowOutcome> {
        if self.view_state.walk_stage() == Some(crate::view_state::WalkStage::Entry) {
            return self.view_state.close_review_walk();
        }
        if !self.walk_abort_armed {
            self.walk_abort_armed = true;
            return vec![FlowOutcome::RefreshNow];
        }
        Vec::new()
    }

    /// The walk's intents (the `ReviewWalk(WalkIntent)` carrier's
    /// interpretation): the cursor/back/advance verbs resolve against
    /// the view state with the pass-derived rows; the row verbs
    /// resolve the cursor row from the pass's row ids and route the
    /// honest commands. Typed inert outside its stage; a `Handoff`
    /// from the advance runs the planner transition.
    #[allow(clippy::too_many_lines)] // one verb table, kept flat
    fn interpret_walk(
        &mut self,
        window: &AppWindow,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
        intent: WalkIntent,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        let (rows, window_bounds) = self.walk_rows(state, cadence, reference);
        let calendar = self.calendar();
        match intent {
            WalkIntent::CursorUp => self.view_state.review_walk_cursor(-1, rows),
            WalkIntent::CursorDown => self.view_state.review_walk_cursor(1, rows),
            WalkIntent::Advance => {
                self.walk_abort_armed = false;
                // Completing the embedded planner ends the child
                // `Completed(None)` (D-2's exit contract): the parent's
                // consumption advances the walk to Prompts.
                if self.view_state.walk_stage() == Some(WalkStage::Planner) {
                    let state_guard = self.state.load();
                    return self.view_state.end_top_flow_with(
                        &state_guard,
                        crate::surfaces::GuestExit::Completed(None),
                    );
                }
                match self.view_state.advance_review_walk(rows) {
                    WalkStep::Advanced => vec![FlowOutcome::RefreshNow],
                    _ => Vec::new(),
                }
            }
            WalkIntent::Prev => {
                self.walk_abort_armed = false;
                // Back out of the planner ends the child `Cancelled`
                // (D-6's rung, J4 US-3/S1): the exit contract returns
                // the walk to Carry, the walk stays open.
                if self.view_state.walk_stage() == Some(WalkStage::Planner) {
                    let state_guard = self.state.load();
                    let outcomes = self
                        .view_state
                        .end_top_flow_with(&state_guard, crate::surfaces::GuestExit::Cancelled);
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                    return outcomes;
                }
                let outcomes = self.view_state.back_review_walk(rows);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                outcomes
            }
            WalkIntent::Planner => match self.view_state.walk_open_planner() {
                WalkStep::Planner => vec![FlowOutcome::RefreshNow],
                _ => Vec::new(),
            },
            WalkIntent::Back => {
                // The TrackEntryDismiss ladder's first rung: a focused
                // walk field (the armed date field, a prompt editor)
                // disengages Insert first — the field stays mounted,
                // the ladder stays put.
                if self.shell.mode == Mode::Insert {
                    // The disengage must also retire the focus facts:
                    // the armed field keeps its latch while the walk
                    // means "Insert is off" — the pre-arm would
                    // re-engage on the next stroke otherwise (the
                    // generation re-baseline absorbs the field's stale
                    // decrement).
                    Self::reset_editing_facts(window);
                    return vec![FlowOutcome::SetMode(Mode::Normal)];
                }
                // A confirm already armed: the second Escape dismisses
                // it ("Keep reviewing") — the walk stays verbatim.
                if self.walk_abort_armed {
                    return self.abort_dismiss();
                }
                // The ladder's sub-state rung is the generic Cancelled
                // rung (D-H4): an armed child — the File-to picker,
                // the Pick-a-date field, the embedded planner — ends
                // alone (the planner's Cancelled exit returns the walk
                // to Carry, D-2); without a child the abort law
                // applies (J4 R-2): Entry closes silently, any other
                // stage arms the confirm.
                let outcomes = match self.view_state.cancel_top_child(state) {
                    Some(outcomes) => outcomes,
                    None => self.abort_arm(),
                };
                // A rung that unmounted a field (the child's death,
                // the stage-back out of Prompts) or the walk itself
                // killed slint focus with it and fired no loss event —
                // the facts reset is the adapter's documented seam for
                // exactly that (the modal-open/view-swap precedent),
                // and the view scope re-anchors (the D15 hand-off).
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                outcomes
            }
            // -- the inbox row verbs --
            WalkIntent::InboxFile => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file(&candidates)
            }
            WalkIntent::InboxKeep | WalkIntent::CarryLeave => self.step_walk_row(window, rows),
            WalkIntent::FileTargetPrev => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file_target(-1, &candidates)
            }
            WalkIntent::FileTargetNext => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file_target(1, &candidates)
            }
            WalkIntent::FileCommit => {
                let Some(target) = self.view_state.walk_file_armed() else {
                    return Vec::new();
                };
                if !matches!(
                    self.view_state.walk_flow().as_ref(),
                    Some(Flow::ReviewWalk { .. })
                ) {
                    return Vec::new();
                }
                // The commit completes the child with the armed stack
                // (N3a); the walk's consumption builds the MoveTask
                // (the end-of-stack append the move flow computes —
                // plan D-W4), bumps the tally's `filed` (D-8) and
                // steps the row — R2's order preserved by
                // construction.
                self.view_state.end_top_flow_with(
                    state,
                    crate::surfaces::GuestExit::Completed(Some(
                        crate::surfaces::LevelResult::Stack(target),
                    )),
                )
            }
            // -- the carry row verbs --
            WalkIntent::CarryTomorrow => {
                let Some(task) = self.walk_cursor_task(state, cadence, reference, rows) else {
                    return Vec::new();
                };
                // "Tomorrow" is relative to the REVIEWED day (plan
                // D-W4): the task lands at the next window's opening
                // instant; the planner leg that follows re-times it.
                // Due dates are never touched.
                let next = taskboard_domain::review_window(
                    cadence,
                    reference.succ_opt().unwrap_or(reference),
                    calendar,
                );
                let step = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::UpdateTask {
                        id: task,
                        changes: taskboard_domain::TaskChanges {
                            scheduled: Some(Some(next.start())),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    },
                )];
                self.view_state
                    .walk_tally_note(crate::view_state::WalkTallyNote::Moved);
                self.finish_walk_row(window, rows, step)
            }
            WalkIntent::CarryPickDate => self.view_state.review_walk_pick_date(),
            WalkIntent::CarryDateCommit => {
                // The parse-keep-text rule lives in the child (N3b):
                // an unparseable date dispatches nothing and the child
                // stays live with the text (the due editor's parser
                // verbatim, plan D-W4).
                let Some(text) = self
                    .view_state
                    .walk_date()
                    .map(|child| child.date_text.clone())
                else {
                    return Vec::new();
                };
                let Ok(Some(instant)) =
                    crate::due_edit::parse(&text, self.day_ctx().now, &calendar.offset)
                else {
                    return Vec::new();
                };
                let Some(task) = self.walk_cursor_task(state, cadence, reference, rows) else {
                    return Vec::new();
                };
                // The commit completes the child with the parsed
                // instant — the honest payload (D3's "minted
                // adapter-side"). The walk parent's consumption is
                // the default ignore here: the cursor task's
                // derivation needs the adapter's day context, so the
                // UpdateTask is built above and the row advance (with
                // the handoff ask) rides `finish_walk_row` below —
                // R2's order preserved; the child's death IS the
                // flattened in-level clear.
                let state_guard = self.state.load();
                let _ = self.view_state.end_top_flow_with(
                    &state_guard,
                    crate::surfaces::GuestExit::Completed(Some(
                        crate::surfaces::LevelResult::Date(instant),
                    )),
                );
                let step = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::UpdateTask {
                        id: task,
                        changes: taskboard_domain::TaskChanges {
                            scheduled: Some(Some(instant)),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    },
                )];
                self.view_state
                    .walk_tally_note(crate::view_state::WalkTallyNote::Moved);
                self.finish_walk_row(window, rows, step)
            }
            // -- the prompt commit verbs --
            WalkIntent::Record => {
                let template = taskboard_domain::DEFAULT_TEMPLATES
                    .iter()
                    .find(|template| template.cadence == cadence)
                    .expect("a curated template per cadence");
                let answers = self
                    .view_state
                    .walk()
                    .map_or(Vec::new(), |walk| walk.answers.clone());
                let text = taskboard_domain::compose_reflection_text(template, &answers);
                if text.is_empty() {
                    // The gentle end (DR-7 D-J9): an all-blank record
                    // dispatches nothing — never relying on a NoOp.
                    // T-47 (#830): the no-op echoes its typed fact on
                    // the status-notice lane (the walk stage is no
                    // longer the only signal).
                    self.notice = Some(Notice {
                        text: crate::presentation::blank_record_notice_phrase(),
                        until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                    });
                    self.push_notice(window);
                    // The walk advances to the completion summary (D-8):
                    // `recorded: no` is presented honestly — never a
                    // silent close.
                    if let Some(Flow::ReviewWalk { stage, .. }) = self.view_state.walk_flow_mut() {
                        *stage = crate::view_state::WalkStage::Complete;
                    }
                    let outcomes = vec![FlowOutcome::RefreshNow];
                    self.apply_outcomes(window, &outcomes);
                    // The prompt editors died with the stage change (no
                    // loss events): reset the facts, re-anchor focus.
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                    return Vec::new();
                }
                let mut outcomes = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::RecordReflection {
                        cadence,
                        day: window_bounds.day(),
                        window: window_bounds.bounds(),
                        text,
                        mood: None,
                        energy: None,
                        flags: std::collections::BTreeSet::new(),
                    },
                )];
                self.view_state
                    .walk_tally_note(crate::view_state::WalkTallyNote::Recorded);
                if let Some(Flow::ReviewWalk { stage, .. }) = self.view_state.walk_flow_mut() {
                    *stage = crate::view_state::WalkStage::Complete;
                }
                outcomes.push(FlowOutcome::RefreshNow);
                self.apply_outcomes(window, &outcomes);
                // The prompt editors died with the stage change (no
                // loss events): reset the facts, re-anchor focus.
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            // The guilt-free exit (P12): lands on the completion
            // summary — `recorded: no` is presented honestly (D-8).
            WalkIntent::Finish => {
                if let Some(Flow::ReviewWalk { stage, .. }) = self.view_state.walk_flow_mut() {
                    *stage = crate::view_state::WalkStage::Complete;
                }
                let outcomes = vec![FlowOutcome::RefreshNow];
                self.apply_outcomes(window, &outcomes);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            // The completion summary's exit (D-8): close + restore.
            WalkIntent::Complete => {
                let outcomes = self.view_state.close_review_walk();
                self.apply_outcomes(window, &outcomes);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            // The abort ladder (J4 D-6): the arm (the host's outside
            // click; Entry closes silently), the accept (close + the
            // applied decisions stay), the dismiss (verbatim stay).
            WalkIntent::AbortArm => {
                let outcomes = self.abort_arm();
                if self.view_state.walk_flow().is_none() {
                    // The Entry silent exit closed the walk.
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                }
                outcomes
            }
            WalkIntent::AbortAccept => {
                let outcomes = self.abort_accept(window);
                self.apply_outcomes(window, &outcomes);
                Vec::new()
            }
            WalkIntent::AbortDismiss => {
                let outcomes = self.abort_dismiss();
                self.apply_outcomes(window, &outcomes);
                Vec::new()
            }
        }
    }

    /// The cursor row's task id in the Carry forward stage (typed
    /// inert outside it or past the rows).
    fn walk_cursor_task(
        &self,
        state: &taskboard_domain::AppState,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
        rows: crate::view_state::WalkRows,
    ) -> Option<TaskId> {
        let Some(Flow::ReviewWalk { cursor, .. }) = self.view_state.walk_flow() else {
            return None;
        };
        let window = taskboard_domain::review_window(cadence, reference, self.calendar());
        crate::presentation::walk_carry_rows(&state.tasks, cadence, window, self.day_ctx().now)
            .get(cursor)
            .copied()
            .filter(|_| rows.carry > 0)
    }

    /// The armed walk child dies with the row step (N3b): the
    /// flattened model's next-row clear — the child's Cancelled end
    /// through the one exit seam.
    fn end_armed_walk_child(&mut self, window: &AppWindow) {
        if self.view_state.walk_date_child() || self.view_state.walk_file_armed().is_some() {
            let state = self.state.load();
            let _ = self
                .view_state
                .end_top_flow_with(&state, crate::surfaces::GuestExit::Cancelled);
            self.refresh_forced(window);
        }
    }

    /// A commandless row step (`Keep in inbox`, `Leave as is`): the
    /// tally's `left` bump (D-8), then the cursor/stage step (the
    /// skip law lands on the next open stage).
    fn step_walk_row(
        &mut self,
        window: &AppWindow,
        rows: crate::view_state::WalkRows,
    ) -> Vec<FlowOutcome> {
        self.end_armed_walk_child(window);
        self.view_state
            .walk_tally_note(crate::view_state::WalkTallyNote::Left);
        match self.view_state.review_walk_next_row(rows) {
            WalkStep::Advanced => vec![FlowOutcome::RefreshNow],
            _ => Vec::new(),
        }
    }

    /// A row step that dispatches `step` first (the carry commits):
    /// the command lands, then the cursor advances (the skip law may
    /// pass straight through to Prompts).
    fn finish_walk_row(
        &mut self,
        window: &AppWindow,
        rows: crate::view_state::WalkRows,
        step: Vec<FlowOutcome>,
    ) -> Vec<FlowOutcome> {
        self.end_armed_walk_child(window);
        let mut outcomes = step;
        match self.view_state.review_walk_next_row(rows) {
            WalkStep::Advanced => {
                outcomes.push(FlowOutcome::RefreshNow);
                outcomes
            }
            _ => outcomes,
        }
    }

    /// The leaving rule (D9, factored out of the table): a view
    /// switch by effect settles the flow through the same per-kind
    /// rule every other trigger uses (round-3 R1–R3). Its r2 D13
    /// dismiss half moved to the `apply_effect` choke point (r4
    /// chunk 4, D14): this method's only caller is `interpret`'s
    /// leaving block, itself only reached from `apply_effect`, so the
    /// identity diff subsumes the dismiss on every path — one
    /// mechanism, one site.
    fn settle_flows_on_leaving(&mut self, window: &AppWindow, state: &taskboard_domain::AppState) {
        self.settle_flow(window, state);
        // The priority pickers are flow-free (UX R-3 D-1): the same
        // leaving rule closes them — a view switch never strands an
        // open quick-list (the recur flags' outlive rule).
        if self.priority_open.take().is_some() {
            self.refresh_forced(window);
        }
    }

    // -- test seams (the lib flow/buffer harnesses; never compiled
    //    into a release binary) ------------------------------------

    /// The pair-table harness drives the interpretation table
    /// directly, one `(Flow, effect)` pass at a time.
    #[cfg(test)]
    pub(crate) fn interpret_for_test(&mut self, window: &AppWindow, effect: UiEffect) {
        self.interpret(window, effect);
    }

    /// The dismiss-walk harness (r4 chunk 4, D14's structural walk):
    /// the tooltip seam lives on `apply_effect` — the choke point —
    /// not on `interpret`, so the walk drives the full pass.
    #[cfg(test)]
    pub(crate) fn apply_effect_for_test(&mut self, window: &AppWindow, effect: UiEffect) {
        self.apply_effect(window, effect);
    }

    /// The dismiss-walk harness, palette half: seeds the palette's
    /// resolved row targets (the walk's `ShowTask`/`InvokeSelected`
    /// preconditions).
    #[cfg(test)]
    pub(crate) fn test_seed_palette(&mut self, targets: Vec<taskboard_domain::SearchTarget>) {
        self.tables.palette = targets;
    }

    /// The control walk's guard seam (round-3 D25): drives one Tab
    /// stroke through the real intercept path so the re-entrancy
    /// rejection is asserted against the production guard, and the
    /// cleared flag resolves the bound walk row exactly once.
    #[cfg(test)]
    pub(crate) fn intercept_tab_for_test(&mut self, window: &AppWindow) -> bool {
        self.intercept(
            window,
            &SharedString::from("\t"),
            slint::language::KeyboardModifiers::default(),
        )
    }

    /// Resets every machine the table reads so pair passes are
    /// order-independent (the flow table and the buffer journeys).
    #[cfg(test)]
    pub(crate) fn test_reset(&mut self) {
        self.view_state = ViewState::new();
        self.shell = KeymapShell::default();
        self.menu_entries.clear();
        self.root_entries.clear();
        self.menu_anchor = MenuAnchor::default();
        self.width_drag = None;
        self.scale_drag = None;
        self.walk_abort_armed = false;
    }

    /// The date editor's adapter-owned buffers, for the §8 invariant
    /// journeys: non-empty ⇔ `Flow::EditDate` armed. (The journeys
    /// land with the buffer-invariants commit; the seam carries its
    /// allow until then.)
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn due_buffers_empty(&self) -> bool {
        self.view_state.date_editor().is_none_or(|editor| {
            editor.buffer.is_empty() && editor.echo.is_empty() && editor.picker.is_none()
        })
    }

    /// Seeds the date editor's buffers non-empty (the §8 invariant
    /// journeys).
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn test_seed_due_buffers(&mut self) {
        let editor = self
            .view_state
            .date_editor_mut()
            .expect("the \u{a7}8 journeys arm the date editor first");
        editor.buffer = "tomorrow".to_owned();
        editor.echo = "stale echo".to_owned();
        editor.picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            None,
            &chrono::Local,
        ));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn arm_width_drag(&mut self, left: StackId, right: StackId) {
        self.width_drag = Some((left, right, 100.0));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn width_drag_armed(&self) -> bool {
        self.width_drag.is_some()
    }

    /// The offer walk's answer (D-H5/D4): the effect is offered to
    /// the levels innermost-first — a flow level answers (its kept
    /// arms, else the total `resolve` table), a view level bubbles
    /// (chunk 1 gives view levels no kept arms; the root's
    /// `(Flow::None, _)` arms are total). The walk therefore always
    /// stops at the innermost flow level, or at the root when none is
    /// live, and a `Handled` effect never reaches an outer level —
    /// `Surfaces::offer` is the walk, the chain proptest instruments
    /// it. The interpretation table is keyed on this level's flow.
    fn responding_flow(&self) -> Flow {
        self.view_state
            .surfaces()
            .offer(|level| level.flow().cloned())
            .unwrap_or(Flow::None)
    }

    /// The recur editor's seeding seam (round-7 D-7.1, plan D-D1): the
    /// level's draft from the task's rule when one carries, else the
    /// anchor-derived bare-flag defaults — the anchor is
    /// `series_reference_date` (never raw-UTC, never "today" unless
    /// the chain lands there).
    fn seed_recur_editor(
        &self,
        state: &taskboard_domain::AppState,
        task: taskboard_domain::TaskId,
    ) -> crate::level_state::RecurEditorState {
        let now = self.day_ctx().now;
        let calendar = self.calendar();
        state.tasks.get(&task).map_or_else(
            || crate::presentation::seed_recur_editor(None, calendar.today(now)),
            |t| {
                let anchor = taskboard_domain::series_reference_date(t, calendar, now);
                crate::presentation::seed_recur_editor(t.recur.as_ref(), anchor)
            },
        )
    }

    /// The recur editor's pushed model (round-7 D-7.1): the buffer's
    /// mirrors, the per-field flavours, the live echo (the assembled
    /// rule's canonical text form, or the typed invalid-field hint),
    /// and the commit's gate. Computed per pass while the flow holds
    /// — the widget never holds state.
    fn recur_editor_model(&self) -> crate::RecurEditorModel {
        use crate::level_state::RecurPatternKind;
        use crate::presentation::{FieldFlavour, recur_field_flavours, recur_field_hint};

        let flavour = |f: FieldFlavour| match f {
            FieldFlavour::Neutral => crate::FieldValidity::Neutral,
            FieldFlavour::Valid => crate::FieldValidity::Valid,
            FieldFlavour::Invalid => crate::FieldValidity::Invalid,
        };
        let Some(draft) = self.view_state.recur_editor() else {
            return Self::recur_editor_model_closed();
        };
        let now = self.day_ctx().now;
        let day = self.calendar();
        let [
            every,
            weekly,
            // The weekday toggles carry no field mark (the echo names
            // an empty set); the flavour stays unused here.
            _weekdays,
            monthly_day,
            monthly_nth,
            until,
            count,
        ] = recur_field_flavours(draft, now, day);
        // The echo: the rule's canonical text form when the whole
        // draft mints over the (open) subject; the typed hint
        // otherwise. Without a subject the overlay is closing — the
        // closed model's shape.
        let head = self
            .view_state
            .subject_task()
            .and_then(|task| self.state.load().tasks.get(&task).cloned());
        let (echo, echo_valid) = match head.as_ref().filter(|t| t.is_open()) {
            Some(head) => match crate::presentation::assemble_recur_draft(draft, head, day, now) {
                Ok(rule) => (SharedString::from(rule.text_form()), true),
                Err(Some((field, invalid))) => {
                    (SharedString::from(recur_field_hint(field, &invalid)), false)
                }
                Err(None) => (SharedString::new(), false),
            },
            None => (SharedString::new(), false),
        };
        // The commit's gate is the plan's named validity fold (the
        // same parse, the flag-only view).
        let commit_enabled = crate::presentation::recur_field_validity(draft, now, day).is_ok()
            && head.as_ref().is_some_and(taskboard_domain::Task::is_open);
        crate::RecurEditorModel {
            pattern_kind: match draft.pattern {
                RecurPatternKind::Daily => 0,
                RecurPatternKind::Weekly => 1,
                RecurPatternKind::Monthly => 2,
            },
            every_days_text: SharedString::from(draft.every_days.clone()),
            every_days_validity: flavour(every),
            weekly_interval_text: SharedString::from(draft.weekly_interval.clone()),
            weekly_interval_validity: flavour(weekly),
            weekdays_checked: ModelRc::new(slint::VecModel::from(draft.weekdays.to_vec())),
            monthly_mode: match draft.monthly_mode {
                crate::level_state::MonthlyMode::OnDay => 0,
                crate::level_state::MonthlyMode::NthWeekday => 1,
            },
            monthly_day_text: SharedString::from(draft.monthly_day.clone()),
            monthly_day_validity: flavour(monthly_day),
            monthly_nth_text: SharedString::from(draft.monthly_nth.clone()),
            monthly_nth_validity: flavour(monthly_nth),
            monthly_weekday: i32::try_from(draft.monthly_nth_weekday.num_days_from_monday())
                .expect("a weekday index fits i32"),
            bound_kind: match draft.bound {
                crate::level_state::RecurBoundKind::Never => 0,
                crate::level_state::RecurBoundKind::Until => 1,
                crate::level_state::RecurBoundKind::Count => 2,
            },
            until_text: SharedString::from(draft.until_text.clone()),
            until_validity: flavour(until),
            count_text: SharedString::from(draft.count.clone()),
            count_validity: flavour(count),
            echo,
            echo_valid,
            commit_enabled,
            detach_armed: draft.detach_armed,
        }
    }

    /// The closed editor's neutral model (the mount gate's else half).
    fn recur_editor_model_closed() -> crate::RecurEditorModel {
        crate::RecurEditorModel {
            pattern_kind: 0,
            every_days_text: SharedString::new(),
            every_days_validity: crate::FieldValidity::Neutral,
            weekly_interval_text: SharedString::new(),
            weekly_interval_validity: crate::FieldValidity::Neutral,
            weekdays_checked: ModelRc::new(slint::VecModel::from(vec![
                false, false, false, false, false, false, false,
            ])),
            monthly_mode: 0,
            monthly_day_text: SharedString::new(),
            monthly_day_validity: crate::FieldValidity::Neutral,
            monthly_nth_text: SharedString::new(),
            monthly_nth_validity: crate::FieldValidity::Neutral,
            monthly_weekday: 0,
            bound_kind: 0,
            until_text: SharedString::new(),
            until_validity: crate::FieldValidity::Neutral,
            count_text: SharedString::new(),
            count_validity: crate::FieldValidity::Neutral,
            echo: SharedString::new(),
            echo_valid: true,
            commit_enabled: false,
            detach_armed: false,
        }
    }

    /// The D9 interpretation table: one `match (flow, effect)` whose
    /// arms route to `view_state` methods named by what they do.
    #[allow(clippy::too_many_lines)] // one decision table, kept flat for review
    #[allow(clippy::match_same_arms)] // one arm per matrix decision
    fn interpret(&mut self, window: &AppWindow, effect: UiEffect) {
        let compact = Self::is_compact(window);
        let state = self.state.load();
        let mut flow = self.responding_flow();
        let focus_walk = matches!(
            effect,
            UiEffect::FocusNext
                | UiEffect::FocusPrev
                // The directional steps ride the walk's hint/hand-off
                // halves (J3 US-1): up/down land on rows like
                // FocusNext does.
                | UiEffect::FocusDirectional(_)
                | UiEffect::ExtendSelectionNext
                | UiEffect::ExtendSelectionPrev
                | UiEffect::FocusFirst
                | UiEffect::FocusLast
        );
        // The show-task retarget arms the hint with the same
        // before/after discipline (round-3 D23 §3.3): `before` reads
        // the pre-effect focus so the jump always re-arms.
        let show_task = effect == UiEffect::ShowTask;
        let focus_before = self.view_state.focused_task();
        let leaving = matches!(
            effect,
            UiEffect::NextView
                | UiEffect::PrevView
                | UiEffect::FocusBoard
                | UiEffect::FocusToday
                | UiEffect::BoardNext
                | UiEffect::BoardPrevious
                | UiEffect::BoardSelect(_)
        );

        // Leaving the edit field by effect first commits the buffer;
        // a move flow ends without dispatch (D9's recorded rule).
        let mut drag_teardown = false;
        if leaving {
            self.settle_flows_on_leaving(window, &state);
            // The settle ended the flow; the table reads the
            // post-settle machine (a stale read would resolve the
            // re-run against a flow that is already gone).
            flow = self.responding_flow();
        }

        let outcomes = match (&flow, effect) {
            // The priority pickers' seams (UX R-3 D-1, #788): Escape
            // (CancelMove's routing) closes the open picker first —
            // nothing dispatches (the cancel is the whole effect);
            // outside click rides the view's own dismissal seam.
            (_, UiEffect::CancelMove) if self.priority_open.is_some() => {
                self.priority_open = None;
                self.refresh_forced(window);
                Vec::new()
            }
            // The openers: flow-free flag flips over the viewed
            // subject — one picker at a time (opening one settles the
            // other), the kiosk/no-subject dispatches refuse (the
            // entries-overlay gate's shape), and a held foreign flow
            // keeps them inert at this arm (the resolver's Nav class
            // settles first and re-runs against Flow::None).
            (Flow::None, UiEffect::PriorityPickerOpen(field)) => {
                if self.kiosk || self.view_state.subject_task().is_none() {
                    Vec::new()
                } else {
                    // The picker overlay is a foreign settle (D-2
                    // site 4, the opener sweep): the rule cursor never
                    // survives under a mask.
                    self.view_state.clear_rule_cursor();
                    self.priority_open = Some(field);
                    self.refresh_forced(window);
                    Vec::new()
                }
            }
            // -- the pointer drag (r2 chunk-8): every cancel-shaped
            //    effect ends it without dispatching (Escape / C-g via
            //    `CancelMove`, the toggle re-request, mode.normal);
            //    Activate confirms the hovered target (the same
            //    "Activate confirms a move flow" semantic); the
            //    pointer owns target cycling, so the keyboard ring
            //    walk is absorbed mid-drag --
            (
                Flow::DragMove { .. },
                effect @ (UiEffect::Activate
                | UiEffect::CancelMove
                | UiEffect::RequestMove
                | UiEffect::SetMode(Mode::Normal)),
            ) => {
                let outcomes = if effect == UiEffect::Activate {
                    self.view_state.confirm_drag_move(&state)
                } else {
                    self.view_state.cancel_move()
                };
                drag_teardown = true;
                outcomes
            }
            (
                Flow::DragMove { .. },
                UiEffect::FocusNext | UiEffect::FocusPrev | UiEffect::FocusDirectional(_),
            ) => Vec::new(),
            // The selection-extend twin (#875) has no kept arms under
            // a live flow: the move/drag flows own the selection
            // semantics, and the Nav settle would cancel the flow
            // (the exact settle-matrix bug class US-2 fixes) — the
            // named inert.
            (
                Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. },
                UiEffect::ExtendSelectionNext | UiEffect::ExtendSelectionPrev,
            ) => Vec::new(),
            // -- the move flows (D8/D9; round 2 adds the stack move;
            //    round 9 chunk 9 adds the directional kept arms — the
            //    same keys move the drop target mid-move, US-2;
            //    #1033 joins the legacy stack-walk verbs — the chords
            //    behind `StackNext`/`StackPrev` must hop the drop
            //    column mid-move, never settle the flow) --
            (
                flow @ (Flow::Move { .. } | Flow::MoveStack { .. }),
                effect @ (UiEffect::FocusNext
                | UiEffect::FocusPrev
                | UiEffect::FocusDirectional(_)
                | UiEffect::StackNext
                | UiEffect::StackPrev
                | UiEffect::Activate
                | UiEffect::RequestMove
                | UiEffect::CancelMove
                | UiEffect::SetMode(Mode::Normal)),
            ) => self.interpret_move(flow, effect, &state, window),
            // -- the edit flows (D5/D9): SetMode(Normal) discards,
            //    Activate commits, leaving settles first (above); the
            //    round-2 lifecycle flows join them --
            (
                Flow::Create { .. }
                | Flow::CreateStack
                | Flow::CreateBoard
                | Flow::EditTitle { .. }
                | Flow::RenameStack { .. }
                | Flow::RenameBoard { .. },
                effect @ (UiEffect::SetMode(Mode::Normal) | UiEffect::Activate),
            ) => self.interpret_edit(effect, &state),
            // The due editor (r2 D25): Escape discards (the clean
            // close — no commit, buffer/echo kept); the commit itself
            // rides `commit_date`, so Activate is a named inert here
            // (the field's `accepted` seam owns Return) — the r2
            // swallow-all arm is gone (round-3 F18): every other
            // effect settles the flow and re-runs via the resolver.
            (Flow::EditDate { .. }, UiEffect::SetMode(Mode::Normal)) => {
                // The Escape ladder (r4 D25): the phase the grid-focus
                // mirror reads decides — the picker phase's Escape is
                // the clean close (the view Escape still ends the
                // editor, R-c); the field phase's Escape is the phase
                // step back: the grid takes focus, the half-typed
                // buffer survives (the steal latch suppresses the
                // settle funnel).
                // No ladder without a picker: a dismissed overlay has
                // no grid to hand focus to — the only non-trapping
                // exit is the clean close (the machine cannot step
                // into a phase that no longer exists).
                let picker_open = self
                    .view_state
                    .date_editor()
                    .is_some_and(|editor| editor.picker.is_some());
                if !picker_open || self.shell.date_picker_grid {
                    self.view_state.discard_edit()
                } else {
                    if let Some(editor) = self.view_state.date_editor_mut() {
                        editor.focus_steal = true;
                    }
                    self.focus_handover = FocusHandover::Grid;
                    // The step-back holds insert mode — the grid
                    // phase's own shape (chunk 6's engage semantics):
                    // the editor is still armed, and the next Escape
                    // resolves as the grid phase's clean close.
                    self.shell.mode = Mode::Insert;
                    // The phase step pushes its one-shot flag itself
                    // (no outcomes ride it — the flow stays armed).
                    self.refresh_forced(window);
                    Vec::new()
                }
            }
            (Flow::EditDate { .. }, UiEffect::Activate | UiEffect::SetMode(Mode::Insert)) => {
                Vec::new()
            }
            // The description editor (round-3 D15): the field's
            // accepted seam owns Return (the checkmark composes);
            // Escape's mode.normal discards — the settle rule is the
            // text flows' (commit non-empty / discard empty). Insert
            // is the named inert, shared with the move flows.
            (
                Flow::EditDescription { .. },
                UiEffect::SetMode(Mode::Normal) | UiEffect::Activate,
            ) => self.interpret_description(effect, &state),
            // -- quick capture (chunk 10 D8): the GLOBAL action, from
            //    any flow — the XOR rule replaces without committing.
            //    The capture overlay is a modal takeover (r2 D13's own
            //    rule sentence): the tip dies into it even though the
            //    view identity may hold (the anchor stays mounted
            //    under the overlay).
            (_, UiEffect::BeginCapture) => {
                Self::dismiss_tooltip(window);
                // T-31 (#814): a live flow's replacement announces
                // itself (the kept arm's own replace, the resolver's
                // twin).
                if !matches!(self.view_state.flow(), Flow::None) {
                    self.arm_overlay_replacement_notice(window);
                }
                self.view_state.begin_capture(&state)
            }
            // -- the concentration overlay's toggle (#496): the
            //    matrix settles any foreign flow first (R3
            //    settle-then-run), then the overlay opens over it
            //    (the XOR rule); a second toggle closes. The kiosk
            //    dispatch no-ops (the exclusion's effect layer —
            //    layer 3: structurally unreachable even if a future
            //    surface bypasses the entry_states gate, layer 2).
            (_, UiEffect::ToggleFocusMode) if self.kiosk => Vec::new(),
            (Flow::FocusMode { .. }, UiEffect::ToggleFocusMode) => {
                Self::dismiss_tooltip(window);
                self.view_state.close_focus_mode()
            }
            (_, UiEffect::ToggleFocusMode) => {
                Self::dismiss_tooltip(window);
                self.view_state.open_focus_mode()
            }
            // -- the time-entries overlay (r4 chunk 10, D-TS2) --
            // The open request settles any foreign flow first (the
            // matrix's overlay cells), then opens over the viewed
            // task; the kiosk dispatch and a subjectless view refuse
            // (dispatch-time gating, D-TS8 — the menu row degrades
            // through this same arm). The modal-takeover half of the
            // r2 D13 rule: the tip dies into the overlay.
            (_, UiEffect::TrackEntriesOpen | UiEffect::UndoBranchesOpen) if self.kiosk => {
                Vec::new()
            }
            (
                Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. },
                UiEffect::TrackEntriesOpen,
            ) => Vec::new(),
            // -- the labels overlay (UX R-3 D-2, #789): the open
            //    request settles any foreign flow first (the entries
            //    overlay's matrix cells), then opens over the viewed
            //    task; kiosk and a subjectless view refuse; the
            //    already-open case is inert (the modal discipline).
            (_, UiEffect::LabelsOpen) if self.kiosk => Vec::new(),
            (Flow::Labels { .. }, UiEffect::LabelsOpen) => Vec::new(),
            (_, UiEffect::LabelsOpen) => {
                // The pickers are flow-free siblings: opening the
                // overlay settles an open quick-list (the
                // exclusivity's shape). The subject resolution is
                // adapter-side (tag-color plan D-4): the detail view's
                // task takes the assignment-toggles mount; otherwise
                // the selected board takes the roster-only mount
                // (inert without one — the no-subject refusal lives
                // in the arm, the arm doc's own precedent).
                if let Some(task) = self.view_state.subject_task() {
                    Self::dismiss_tooltip(window);
                    self.priority_open = None;
                    let outcomes = self
                        .view_state
                        .open_labels(crate::view_state::LabelOverlaySubject::Task(task));
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if !self.view_state.labels_board_mount_open() {
                    let outcomes = self.view_state.open_labels_board(&self.state.load());
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                }
                Vec::new()
            }
            (_, UiEffect::TrackEntriesOpen) => match self.view_state.subject_task() {
                Some(task) => {
                    Self::dismiss_tooltip(window);
                    self.view_state.open_track_entries(task)
                }
                None => Vec::new(),
            },
            // The recur row's intents (issue #506, plan D-8): the
            // pause open flips the adapter-owned flag over the
            // subject; the arm effects set their one-shot targets.
            // The kiosk dispatch is structurally inert (layer 3, the
            // entries-overlay precedent).
            (_, UiEffect::SeriesPauseOpen) if self.kiosk => Vec::new(),
            (_, UiEffect::SeriesPauseOpen) => {
                if let Some(task) = self.view_state.subject_task() {
                    // The recur-pause overlay is a foreign settle
                    // (D-2 site 4, the opener sweep).
                    self.view_state.clear_rule_cursor();
                    self.series_target = Some(task);
                    self.recur_pause_open = true;
                    self.recur_confirm_armed = false;
                    // The open echo rides adapter-owned flags outside
                    // the state Arc — the unchanged-refresh guard
                    // would eat the pass (the armed-create-form
                    // precedent, the HabitConvertArm twin; the US-11
                    // chord shares the arm, so click ≡ key holds).
                    self.refresh_forced(window);
                }
                Vec::new()
            }
            // The row-menu opener (US-7, W-8, #1032): the walk
            // subject's context menu through the pointer seams' own
            // openers — click ≡ key by construction; the keyboard
            // anchor is the toolbar fallback (no pointer point). The
            // subjectless walk is the typed refusal (the uniform law).
            (_, UiEffect::RowMenuRequested) => {
                self.open_row_menu_keyboard(window);
                Vec::new()
            }
            // The rule cursor's disarm chord (US-10, W-11, #1091):
            // armed — clear AND latch (the verticals return to the
            // control ladder; the flow table's `armed --> disarmed`
            // edge); disarmed — the typed inert (no state change).
            // The other clear sites stay the only other clearers (the
            // composition law: no second disarm site invented).
            // The roster remove's arm step (US-9, W-10, #1066's
            // roster half): the armed toggle over the roster cursor —
            // the same toggle the pointer seam has always run (one
            // armed subject, the repeat is not the confirm), now
            // cursor-addressed so click ≡ key holds. Cursorless (and
            // kiosk — the section's absence) is the typed refusal.
            (_, UiEffect::BreakRuleRemoveArm) if self.kiosk => Vec::new(),
            (_, UiEffect::BreakRuleRemoveArm) => {
                let live = self
                    .view_state
                    .break_rule_cursor()
                    .filter(|rule| self.tables.break_rules.contains(rule));
                if let Some(rule) = live {
                    self.break_rule_armed = if self.break_rule_armed == Some(rule) {
                        None
                    } else {
                        Some(rule)
                    };
                    self.refresh_forced(window);
                } else {
                    self.arm_no_target_notice(ActionId::BreakRuleRemove);
                    self.push_notice(window);
                }
                Vec::new()
            }
            (_, UiEffect::RowCursorDisarm) => {
                if self.view_state.rule_cursor().is_some() {
                    self.view_state.disarm_rule_cursor();
                    self.refresh_forced(window);
                } else {
                    self.arm_no_target_notice(ActionId::RowCursorDisarm);
                    self.push_notice(window);
                }
                Vec::new()
            }
            // The menu-routed conversion arms (cross-surface friction
            // plan D-2/D-3, #832): they route the real arming seams,
            // subject-resolved — the habit twin through the section's
            // focus fact, the series twin through the viewed subject.
            // A subjectless dispatch surfaces the typed refusal on the
            // status-notice lane (the T-19 twin; the verbs' global
            // silent `NoTarget` arm is the uniform law — chunk 8 D6,
            // #935's one-place pass).
            // The kiosk dispatch is structurally inert (the
            // entries-overlay precedent: the arming flags are desktop
            // strips a menu-routed arm must never flip invisibly).
            (_, UiEffect::HabitConvertArm | UiEffect::SeriesConvertArm) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitConvertArm) => {
                // The subject resolves through the effective walk /
                // click union (chunk 8 D4): the keyboard walk's habit
                // landing arms the conversion exactly as the pointer
                // row click does — click ≡ key.
                let armed = match self.effective_habit() {
                    Some(habit) => self.arm_habit_convert(window, habit),
                    None => false,
                };
                if !armed {
                    self.arm_no_target_notice(ActionId::HabitConvertArm);
                }
                self.push_notice(window);
                // The armed-confirm echo rides adapter-owned flags
                // outside the state Arc — the unchanged-refresh guard
                // would eat the pass (the armed-create-form
                // precedent); the re-projection inside the arm
                // rebuilt the open menu's page (O-7 Q3).
                self.refresh_forced(window);
                Vec::new()
            }
            // The armed conversion's cancel step (H2 D4, O-7 Q3):
            // disarms INSIDE the open menu — the re-projected page
            // greys the confirm again; nothing was dispatched.
            (_, UiEffect::HabitConvertCancel) => {
                self.habit_convert_armed = None;
                self.habit_convert_target = None;
                self.pattern_draft = None;
                self.reproject_habit_menu(window);
                self.refresh_forced(window);
                Vec::new()
            }
            // The staged remove's arm (H4 D4, #944): staging touches
            // no engine state — the armed subject rides the adapter
            // flag and the open menu's page re-projects (the confirm
            // flips enabled). Kiosk dispatch is structurally inert
            // (the convert arm's exact posture).
            (_, UiEffect::HabitRemoveArm | UiEffect::HabitRemoveCancel) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitRemoveArm) => {
                let armed = match self.focused_habit {
                    Some(habit) => {
                        let live = self
                            .state
                            .load()
                            .habits
                            .habits
                            .get(&habit)
                            .is_some_and(taskboard_domain::habit::Habit::is_live);
                        if live {
                            self.habit_remove_armed = Some(habit);
                            self.reproject_habit_menu(window);
                        }
                        live
                    }
                    None => false,
                };
                if !armed {
                    self.arm_no_target_notice(ActionId::HabitRemove);
                }
                self.push_notice(window);
                // The armed-confirm echo rides adapter-owned flags —
                // the unchanged-refresh guard would eat the pass
                // (the convert arm's precedent).
                self.refresh_forced(window);
                Vec::new()
            }
            // The armed remove's cancel step: disarms INSIDE the open
            // menu — the re-projected confirm greys again; nothing
            // was dispatched.
            (_, UiEffect::HabitRemoveCancel) => {
                self.habit_remove_armed = None;
                self.reproject_habit_menu(window);
                self.refresh_forced(window);
                Vec::new()
            }
            // The undo-family restore's arm (W-3): the roster's newest
            // tombstone is the armed subject (D-4: kiosk is
            // structurally inert — the CLI carries its restore path).
            // Staging touches no engine state; the open menu's page
            // re-projects (the confirm flips enabled). An empty roster
            // is the no-op arm (D-2: the row never rendered).
            (_, UiEffect::HabitRestoreArm) => {
                let armed = if self.kiosk {
                    None
                } else {
                    let state = self.state.load_full();
                    state.roster.tombstoned_habits.first().map(|row| row.id)
                };
                self.habit_restore_armed = armed;
                if armed.is_none() {
                    self.arm_no_target_notice(ActionId::HabitRestore);
                } else {
                    // The arm names its subject (the stage-3
                    // walkthrough's Manual item): the menu's arm row is
                    // registry-static, so the roster row's name rides
                    // the notice lane — the confirm's consequence is
                    // visible, not remembered.
                    let name = self
                        .state
                        .load()
                        .roster
                        .tombstoned_habits
                        .iter()
                        .find(|row| Some(row.id) == armed)
                        .map(|row| row.name.clone());
                    if let Some(name) = name {
                        self.notice = Some(Notice {
                            text: crate::presentation::habit_restore_arm_notice_phrase(&name),
                            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
                        });
                    }
                }
                self.push_notice(window);
                self.reproject_habit_menu(window);
                self.refresh_forced(window);
                Vec::new()
            }
            // The armed restore's cancel step: disarms INSIDE the open
            // menu — the re-projected confirm greys again; nothing was
            // dispatched (the remove cancel's twin).
            (_, UiEffect::HabitRestoreCancel) => {
                self.habit_restore_armed = None;
                self.reproject_habit_menu(window);
                self.refresh_forced(window);
                Vec::new()
            }
            (_, UiEffect::SeriesConvertArm) => {
                let armed = match self.view_state.subject_task() {
                    Some(task) => self.arm_series_convert(task),
                    None => false,
                };
                if !armed {
                    self.arm_no_target_notice(ActionId::SeriesConvertArm);
                }
                self.push_notice(window);
                self.refresh_forced(window);
                Vec::new()
            }
            // The create form's toggle (round-7 D-7.2): kiosk stays
            // inert (the action gate, D-T16 — the affordance never
            // mounts there), and opening requires an active board —
            // the section's board IS the create board, so without one
            // the dispatch degrades to silence (the stale-arm law).
            // Armed again = discard-and-close (the ViewPlanNextDay
            // toggle posture).
            // The pause editor's opener (HF1, W-3): resolves the
            // effective habit subject (the walk/click union — click ≡
            // key) and flips the adapter-owned flag; kiosk is
            // structurally inert (the R-3 gate: the actions render
            // disabled, the editor never mounts).
            (_, UiEffect::HabitPauseOpen) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitPauseOpen) => {
                if let Some(habit) = self.effective_habit() {
                    self.habit_pause_target = Some(habit);
                    self.habit_pause_open = true;
                    self.refresh_forced(window);
                } else {
                    self.arm_no_target_notice(ActionId::HabitPause);
                    self.push_notice(window);
                }
                Vec::new()
            }
            (_, UiEffect::HabitCreateToggle) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitCreateToggle) => {
                if self.view_state.board().is_some() {
                    self.habit_create_open = !self.habit_create_open;
                    if !self.habit_create_open {
                        self.habit_create_draft = None;
                        self.habit_create_quota.clear();
                    }
                    // The armed form's mount is a view-state change
                    // outside the state Arc — forced (the refresh
                    // invariant's classification).
                    self.refresh_forced(window);
                }
                Vec::new()
            }
            // -- the recurrence-rule editor (round-7 D-7.1) --
            // Kiosk and a subjectless/closed view refuse (the
            // entries-overlay precedent); already open is inert (the
            // modal discipline). The opener arms the one-shot target,
            // opens the flow, and seeds the level's draft from the
            // task's rule or the anchor (the seeding law, D-D1) —
            // insert mode, the form's fields.
            (_, UiEffect::SeriesEditorOpen) if self.kiosk => Vec::new(),
            (Flow::EditRecur { .. }, UiEffect::SeriesEditorOpen) => Vec::new(),
            (_, UiEffect::SeriesEditorOpen) => match self.view_state.begin_edit_recur(&state) {
                Some(task) => {
                    Self::dismiss_tooltip(window);
                    self.recur_edit_target = Some(task);
                    let seeded = self.seed_recur_editor(&state, task);
                    if let Some(editor) = self.view_state.recur_editor_mut() {
                        *editor = seeded;
                    }
                    vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
                }
                None => Vec::new(),
            },
            // The stop-repeating strip's flag flips: the
            // conversion-strip pattern over the editor level; inert
            // without the flow (a stale dispatch changes nothing).
            (_, UiEffect::SeriesDetachArm) => {
                if let Some(editor) = self.view_state.recur_editor_mut() {
                    editor.detach_armed = true;
                }
                Vec::new()
            }
            (_, UiEffect::SeriesDetachCancel) => {
                if let Some(editor) = self.view_state.recur_editor_mut() {
                    editor.detach_armed = false;
                }
                Vec::new()
            }
            // The Escape ladder (D-TS2), level-aware like `MenuBack`:
            // a focused form field disengages first (mode flip; the
            // overlay stays), then the form steps to the list, then
            // the overlay closes.
            (_, UiEffect::TrackEntryDismiss) if self.shell.mode == Mode::Insert => {
                vec![FlowOutcome::SetMode(Mode::Normal)]
            }
            // The ladder's second rung is the generic Cancelled rung
            // (D-H4): the form child ends alone, the list stays;
            // without the child the list's own close rung applies
            // (`SetMode(Normal)` rides it so a disengaged field never
            // re-masks — the `close_palette` shape).
            (_, UiEffect::TrackEntryDismiss) => match self.view_state.cancel_top_child(&state) {
                Some(outcomes) => outcomes,
                None => self.view_state.close_track_entries(),
            },
            // The labels ladder (D-2): a focused field's Insert
            // disengages first (the mode flip; the overlay stays),
            // then the in-place surfaces step out — an open form
            // discards, an armed row disarms — and the last rung
            // closes the overlay.
            (_, UiEffect::LabelsDismiss) if self.shell.mode == Mode::Insert => {
                vec![FlowOutcome::SetMode(Mode::Normal)]
            }
            (_, UiEffect::LabelsDismiss)
                if self.label_form.take().is_some()
                    || self.label_delete_armed.take().is_some()
                    || self.label_restore_armed.take().is_some() =>
            {
                self.refresh_forced(window);
                Vec::new()
            }
            (_, UiEffect::LabelsDismiss) => self.view_state.close_labels(),
            // -- the capture commit/close (the palette's modal peer);
            //    NOT `interpret_edit` — capture's non-empty commit
            //    deliberately keeps the flow --
            (Flow::Capture, UiEffect::Activate) => self.view_state.commit_capture(),
            (Flow::Capture, UiEffect::SetMode(Mode::Normal)) => self.view_state.close_capture(),
            // The recurrence editor's Escape ladder (#976 SB1,
            // US-2/S3): the mode.normal resolution under the live
            // editor is the explicit cancel — the clean close (the
            // draft dies, nothing dispatches; the settle rule, not
            // the commit funnel). Insert/Activate stay the named
            // inert (the class row): only Escape cancels.
            (Flow::EditRecur { .. }, UiEffect::SetMode(Mode::Normal)) => {
                self.view_state.end_edit_recur()
            }
            // vi's `i`/`a` open the view's editing flow rather than
            // flipping mode into the void (D5).
            (
                Flow::None | Flow::Move { .. } | Flow::MoveStack { .. },
                UiEffect::SetMode(Mode::Insert),
            ) => self.interpret_insert(&state),
            // The description editor's insert inert (round-3 D15):
            // re-entering insert inside the open editor changes
            // nothing (the field owns its printables).
            (Flow::EditDescription { .. }, UiEffect::SetMode(Mode::Insert)) => Vec::new(),
            // -- no flow: the intents begin things --
            (Flow::None, UiEffect::OpenPalette) => {
                // The palette is the second modal context (D8): the
                // modal-takeover half of the r2 D13 rule — the tip
                // dies into the overlay (the BeginCapture shape).
                Self::dismiss_tooltip(window);
                self.view_state.open_palette()
            }
            // The title action with no flow (round-3 W-SM-d): arms
            // the viewed task's editor; inert without a viewed
            // subject (the matrix's "no subject" cell).
            (Flow::None, UiEffect::BeginTitleEdit) => match self.view_state.subject_task() {
                Some(task) => self.view_state.begin_edit_title(&state, task),
                None => Vec::new(),
            },
            // The detail editors' keyboard openers (keymap-parity plan
            // US-3, D-4): the same funnels the row affordances call —
            // the round-3 D11 `begin_date_edit` body and the round-3
            // D15 description arm (click ≡ key at the funnel). The
            // subjectless detail is each funnel's typed inert.
            (Flow::None, UiEffect::BeginDateEdit { field }) => {
                self.begin_date_edit(window, field);
                Vec::new()
            }
            (Flow::None, UiEffect::BeginDescriptionEdit) => {
                self.begin_description_edit(window);
                Vec::new()
            }
            // The effort field's focus opener (J3 US-4, round 9 chunk
            // 11): a pure focus verb over the always-on M9 row — the
            // anchor family's detail-cell seam (kind 6, the effort
            // field's own anchor). Arms nothing, commits nothing (the
            // M9 commit-on-focus-loss law keeps the field as commit
            // owner); inert without a viewed subject (the matrix's
            // "no subject" cell).
            (Flow::None, UiEffect::BeginEffortEdit) => {
                if self.view_state.subject_task().is_some() {
                    window.invoke_focus_effort_opener();
                }
                Vec::new()
            }
            // The habit editor's opener (H4 D1, US-5): the Activate
            // shape over the one-shot habit subject — the shared
            // detail surface mounts the habit variant. Kiosk dispatch
            // is structurally inert (D7: the editor is unmountable; a
            // subjectless dispatch surfaces the typed refusal on the
            // notice lane — the T-19 twin).
            (_, UiEffect::HabitEdit) if self.kiosk => Vec::new(),
            (Flow::None, UiEffect::HabitEdit) => {
                if let Some(habit) = self.habit_edit_subject() {
                    // The draft seeds from the stored row (the editor
                    // opens showing the habit as it is); it dies with
                    // the editor (cancel/escape/close — no draft
                    // persistence, D1).
                    self.habit_edit_draft = self
                        .state
                        .load()
                        .habits
                        .habits
                        .get(&habit)
                        .map(crate::router::HabitEditDraft::from_habit);
                    let outcomes = self.view_state.open_habit_editor(habit);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if !self.kiosk && self.view_state.focused_task().is_some() {
                    // The walk cursor's fall-through (D3a W-16, #1202):
                    // the Return row at Today is the habit editor's —
                    // over a TASK walk landing the same stroke belongs
                    // to the focused row's OWN verb (the rule-cursor
                    // activation's parity law: the cursor is a pointer,
                    // not a new verb family). The task editor opens
                    // through the landed Activate shape, unchanged.
                    let outcomes = self.interpret_begin(UiEffect::Activate, &state);
                    self.apply_outcomes(window, &outcomes);
                } else {
                    self.arm_no_target_notice(ActionId::HabitEdit);
                    self.push_notice(window);
                }
                Vec::new()
            }
            // The quota popover's opener (W-5, #961): the one arming
            // site both invocation paths converge on (the row phrase
            // click seam routes the same effect — click ≡ key). Kiosk
            // is structurally inert (R-3's disabled posture gates the
            // affordance); a subjectless dispatch surfaces the typed
            // refusal (the T-19 twin).
            (_, UiEffect::HabitQuotaEdit) if self.kiosk => Vec::new(),
            (Flow::None, UiEffect::HabitQuotaEdit) => {
                if let Some(habit) = self.habit_edit_subject() {
                    let state = self.state.load();
                    let seed = state.habits.habits.get(&habit).map(|row| {
                        (
                            // The typed seed (D3a W-8, #1196): the
                            // stored quota, clamped at the draft
                            // domain's max when above it — never the
                            // silent floor of 1
                            // (presentation::quota_seed's law).
                            crate::presentation::quota_seed(row.target.quota().get()),
                            crate::presentation::habit_target_phrase(row.target),
                        )
                    });
                    if let Some((quota, phrase)) = seed {
                        drop(state);
                        // The draft starts AT the stored quota (the
                        // popover shows the current count — US-4/AC-1);
                        // the kind phrase is the row's own (the picker
                        // is locked to the current kind — the inline
                        // edit is a one-number edit).
                        self.habit_quota_count = quota;
                        window.set_habits_quota_kind_phrase(SharedString::from(phrase));
                        window.set_habits_quota_open(true);
                        self.sync_quota_popover(window);
                        // The focus fact composes both directions (the
                        // row activation's law).
                        self.focused_habit = Some(habit);
                    } else {
                        drop(state);
                    }
                } else {
                    self.arm_no_target_notice(ActionId::HabitQuotaEdit);
                    self.push_notice(window);
                }
                Vec::new()
            }
            // The habit history dialog's opener (HF3 W-8, US-7/S1):
            // the read-role month-grid projection mounts over the
            // resolved habit subject. Kiosk dispatch is structurally
            // inert (R-3 — the dialog is unmountable, D7); a
            // subjectless dispatch surfaces the typed refusal on the
            // notice lane (the T-19 twin). Zero commands route — the
            // dialog is a projection (US-7/S2–S4).
            (_, UiEffect::HabitHistoryOpen) if self.kiosk => Vec::new(),
            (Flow::None, UiEffect::HabitHistoryOpen) => {
                self.open_habit_history_from_subject(window);
                Vec::new()
            }
            // The rule cursor's activation (J3 US-6 D-3, round 9
            // chunk 12): over an armed cursor, Activate dispatches the
            // cursor'd row's OWN affordance — remove for an armed
            // rule, the due editor for an unarmed one (the row's
            // icon-button seams; the cursor is a pointer, not a new
            // verb family, R-5's parity law). Disarmed, the detail's
            // Activate falls through to `interpret_begin` untouched.
            (Flow::None, UiEffect::Activate)
                if self.view_state.view() == crate::view_state::ViewId::TaskDetail
                    && self.view_state.rule_cursor().is_some() =>
            {
                self.activate_rule_cursor(&state, window);
                Vec::new()
            }
            // The planner cursor's activation (US-15/S3, #1041): over
            // an armed cursor, Activate dispatches the cursor'd row's
            // OWN seam — the same `PlanBeginPlace` the row's
            // click/AT-action seam routes (click ≡ key; the rule
            // cursor's parity law). Disarmed, the planner's Activate
            // falls through to `interpret_begin` untouched.
            (Flow::None, UiEffect::Activate)
                if self.view_state.view() == crate::view_state::ViewId::PlanNextDay
                    && self.view_state.plan_cursor().is_some() =>
            {
                self.activate_plan_cursor(window);
                Vec::new()
            }
            (
                Flow::None,
                effect @ (UiEffect::Activate
                | UiEffect::RequestMove
                | UiEffect::BeginCreate { .. }
                | UiEffect::BeginCreateStack),
            ) => {
                // The ghost case (r4 chunk 4, D14): the identity tuple
                // does not move, but the hovered ghost itself unmounts
                // by construction of the flow the begin opens — an
                // explicit dismiss beside the flow's arming.
                if matches!(
                    effect,
                    UiEffect::BeginCreate { .. } | UiEffect::BeginCreateStack
                ) {
                    Self::dismiss_tooltip(window);
                }
                self.interpret_begin(effect, &state)
            }
            // -- no flow: the pure view-state defaults (the arms the
            //    resolver's settle-then-run re-dispatch resolves
            //    against; under a foreign flow the same effects
            //    settle first, audit §6 rows S) --
            (Flow::None, UiEffect::FocusToday) => self.view_state.focus_today(),
            (Flow::None, UiEffect::OpenInbox) => self.view_state.open_inbox(&state),
            // UX-R8 D-4's Escape cell: the move-cancel stroke with
            // no flow is the settle law's keyboard face — the
            // selection clears, nothing dispatches. The rule cursor's
            // D-2 site 5 joins: the global cancel stays
            // unambiguously a clear.
            (Flow::None, UiEffect::CancelMove) => {
                self.view_state.clear_rule_cursor();
                // The roster cursor joins (US-9, W-10): the global
                // cancel stays unambiguously a clear (the rule
                // cursor's D-2 site 5 twin).
                self.view_state.clear_break_rule_cursor();
                self.view_state.clear_plan_cursor();
                self.view_state.clear_selection()
            }
            (Flow::None, UiEffect::FocusNext) => {
                let day = self.day_ctx();
                self.view_state
                    .advance_focus(&state, compact, self.kiosk, &day)
            }
            (Flow::None, UiEffect::FocusPrev) => {
                let day = self.day_ctx();
                self.view_state
                    .retreat_focus(&state, compact, self.kiosk, &day)
            }
            // The directional-focus table (J3 US-1, round 9 chunk 9):
            // one declarative home for the per-view reading — the
            // `push_scroll_page_hint` precedent. The control-walk
            // views return no outcomes here: the post-table tail runs
            // the traversal injection for them (one walk
            // implementation, two dispatch fronts — the D-2
            // route-level rule). Every audited view carries a row or
            // an explicit exclusion comment in
            // [`Self::interpret_directional`].
            (Flow::None, UiEffect::FocusDirectional(direction)) => {
                let day = self.day_ctx();
                self.interpret_directional(&state, direction, compact, &day)
            }
            // The selection-extend route front (#875 RIDER): the same
            // guard the intercept branch carries — Board only, the
            // typed no-op elsewhere (the vi rows must not leak
            // selection semantics into other views through the
            // router).
            (
                Flow::None,
                effect @ (UiEffect::ExtendSelectionNext | UiEffect::ExtendSelectionPrev),
            ) => {
                if self.view_state.view() == crate::view_state::ViewId::Board {
                    let direction = if effect == UiEffect::ExtendSelectionNext {
                        1
                    } else {
                        -1
                    };
                    self.view_state.extend_selection_walk(&state, direction)
                } else {
                    Vec::new()
                }
            }
            (Flow::None, UiEffect::NextView) => self.view_state.next_view(&state),
            (Flow::None, UiEffect::PrevView) => self.view_state.prev_view(&state),
            (Flow::None, UiEffect::FocusBoard) => self.view_state.focus_board(),
            (Flow::None, UiEffect::BoardNext) => self.view_state.next_board(&state),
            (Flow::None, UiEffect::BoardPrevious) => self.view_state.previous_board(&state),
            (Flow::None, UiEffect::BoardSelect(board)) => {
                self.view_state.select_board(&state, board)
            }
            (Flow::None, effect @ (UiEffect::StackWider | UiEffect::StackNarrower)) => {
                let direction = if effect == UiEffect::StackWider {
                    1
                } else {
                    -1
                };
                // The resolved row plus the scaled step (r4 D8): the
                // same assembly the models push reads — one derivation.
                let row = crate::models::board_row(window, &state, &self.view_state);
                let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let step = (f64::from(crate::widths::STACK_WIDTH_STEP_PX) * scale).round() as i64;
                self.view_state.resize_focused_stack(
                    &state,
                    direction,
                    &crate::view_state::RowStep {
                        stacks: &row.stacks,
                        widths: &row.widths,
                        mins: &row.mins,
                        step,
                        scale,
                    },
                )
            }
            // Round 2 (D17/D20): the stack wrap-walk and the first/last
            // jumps — pure view state, the BoardNext precedent.
            (Flow::None, UiEffect::StackNext) => self.view_state.step_stack(&state, 1),
            (Flow::None, UiEffect::StackPrev) => self.view_state.step_stack(&state, -1),
            (Flow::None, effect @ (UiEffect::FocusFirst | UiEffect::FocusLast)) => {
                let edge = if effect == UiEffect::FocusFirst {
                    FocusEdge::First
                } else {
                    FocusEdge::Last
                };
                {
                    let day = self.day_ctx();
                    self.view_state
                        .jump_focus(&state, edge, compact, self.kiosk, &day)
                }
            }
            // The armed delete's repeat stroke (UX R-2 D-2, #784):
            // the invoking verb's second stroke confirms the armed
            // subject (or re-arms to a new one) — the two-step
            // invoke; the strip's Confirm seam is its pointer twin
            // (click ≡ key at every step).
            (Flow::ConfirmDelete(_), effect @ (UiEffect::StackDelete | UiEffect::BoardDelete)) => {
                self.interpret_lifecycle(&flow, effect, &state)
            }
            // Round 2 (D18/D21): the board/stack lifecycle verbs —
            // target resolution is the view state's; the engine owns
            // every delete refusal.
            (
                Flow::None,
                effect @ (UiEffect::StackDelete
                | UiEffect::StackRename
                | UiEffect::StackMove
                | UiEffect::BoardCreate
                | UiEffect::BoardDelete
                | UiEffect::BoardRename
                | UiEffect::BoardColorEdit
                | UiEffect::BoardDefaultPreset),
            ) => self.interpret_lifecycle(&flow, effect, &state),
            // The board ceremony verbs (OP1, US-1/2/3): the same
            // shape as the armed-delete row — the first stroke arms
            // (over `Flow::None`), the armed subject's repeat stroke
            // confirms (click ≡ key at every step; the reconcile's
            // pending-preview guard is the view state's).
            (
                Flow::None,
                effect @ (UiEffect::BoardArchive | UiEffect::BoardReset | UiEffect::BoardReconcile),
            ) => self.interpret_ceremony(effect, &state),
            (
                Flow::ConfirmBoardArchive { .. } | Flow::ConfirmBoardReset { .. },
                effect @ (UiEffect::BoardArchive | UiEffect::BoardReset),
            ) => self.interpret_ceremony(effect, &state),
            (Flow::ConfirmBoardReconcile { .. }, UiEffect::BoardReconcile) => {
                self.interpret_ceremony(UiEffect::BoardReconcile, &state)
            }
            // The status panel's toggle (OP3 W-1, the settings
            // opener's twin, D-5): the open is a pushed view level;
            // the snapshot-on-open query dispatches from the panel's
            // own mount seam (`status-panel-open` — one query per
            // mount, never a timer), so this arm is pure view state.
            (Flow::None, UiEffect::OpenStatusPanel) => {
                if self.view_state.view() == crate::view_state::ViewId::Status {
                    self.view_state.close_status_panel()
                } else {
                    self.view_state.open_status_panel()
                }
            }
            // Round-1 chunk 3 (D28b, D6.3): toggle semantics — the
            // menu row, `Ctrl+,`, and a second press all close an open
            // settings surface. No explicit dismiss: both toggle
            // directions swap the mounted view, the identity diff at
            // the choke point covers them (r4 chunk 4).
            (Flow::None, UiEffect::OpenSettings) => {
                if self.view_state.view() == crate::view_state::ViewId::Settings {
                    {
                        let mut outcomes = self.view_state.close_settings();
                        // The armed strip dies with the surface
                        // (cross-surface friction plan D-4, #836):
                        // leaving settings disarms, the strip is only
                        // visible while mounted.
                        self.settings_disarm_item();
                        // US-5 (#706): the restore's subject-liveness
                        // guard runs before the context sync, so the
                        // focus-restore funnel lands on the fallback.
                        outcomes.extend(self.view_state.repair_restored_root(&state));
                        outcomes
                    }
                } else {
                    self.view_state.open_settings()
                }
            }
            // -- the export dialog (the J2 export plan W-1) --
            // Kiosk: the dispatch is structurally inert (R-2 — the
            // dialog does not mount; the entry points are filtered
            // at the menu mint and the Settings view is unreachable).
            (_, UiEffect::ExportDialogOpen) if self.kiosk => Vec::new(),
            // The dialog is a Nav-class view intent (the settings
            // opener's shape): under a held flow the resolver settles
            // first and re-runs against Flow::None — the arms below
            // are the flowless cells only (the pair matrix's law).
            (Flow::None, UiEffect::ExportDialogOpen) if self.export_vm.is_some() => Vec::new(),
            (Flow::None, UiEffect::ExportDialogOpen) => {
                self.export_vm = Some(crate::export_dialog::ExportVm::seeded(
                    &self.export_defaults,
                ));
                // The mount never dispatches anything (US-1/S1 — pure
                // presentation state); the refresh pushes the seeded
                // model.
                vec![FlowOutcome::RefreshNow]
            }
            (Flow::None, UiEffect::ExportDialogClose) => self.export_dialog_close_outcomes(),
            // -- the plan-next-day surface (#622, D-4/D-3) --
            // Kiosk: the dispatch is structurally inert (the
            // entries-overlay refusal's layer 3 — D-5).
            (
                _,
                _effect @ (UiEffect::PlanBeginPlace(_)
                | UiEffect::PlanCommitPlace
                | UiEffect::PlanBoxEarlier(_)
                | UiEffect::PlanBoxLater(_)
                | UiEffect::PlanRemoveBox(_)),
            ) if self.kiosk => Vec::new(),
            (_, UiEffect::OpenPlanNextDay) if self.kiosk => Vec::new(),
            // A held flow keeps the planner intents inert (the modal
            // discipline: Escape out of the flow first) — the
            // placement form's own rung excepted: form → pane.
            (Flow::PlanPlace { .. }, UiEffect::OpenPlanNextDay) => {
                // The discard's wipe is the level's drop (D-H2).
                self.view_state.close_plan_place()
            }
            // The commit funnel: parse → mask → ONE command (the
            // view never sees a `StateCommand`); invalid input
            // dispatches nothing — the typed refusal keeps the text.
            (Flow::PlanPlace { task }, UiEffect::PlanCommitPlace) => self.commit_plan_place(*task),
            (Flow::None | Flow::ReviewPlanner, UiEffect::PlanCommitPlace) => Vec::new(),
            // -- the review walk (DR-7 chunk 3, #638) --
            // Kiosk: the dispatch is structurally inert (layer 2 of
            // the three-layer exclusion, D-J14 — the pinned gate).
            (_, UiEffect::OpenReviewWalk | UiEffect::ReviewWalk(_)) if self.kiosk => Vec::new(),
            // Already in the walk: the opener is not a toggle (the
            // modal discipline — Escape out first).
            (Flow::ReviewWalk { .. }, UiEffect::OpenReviewWalk) => Vec::new(),
            // The opener (the action, the Today card, the menu): the
            // reference derives from `review_due` when a window is
            // due, else the current day window (the evening review
            // before the boundary).
            (Flow::None, UiEffect::OpenReviewWalk) => {
                let reference = self.walk_reference(&state);
                let cadence = taskboard_domain::ReviewCadence::Day;
                self.view_state.open_review_walk(cadence, reference)
            }
            // The carrier's intents under any child level: the child
            // has no kept arms — the verb table runs against the walk
            // level beneath (inert without one; the fixture's depth-1
            // shape).
            (
                Flow::EntryDatePicker
                | Flow::WalkFilePick { .. }
                | Flow::WalkDatePick
                | Flow::ReviewPlanner,
                UiEffect::ReviewWalk(intent),
            ) => match self.view_state.walk_flow() {
                Some(Flow::ReviewWalk {
                    cadence, reference, ..
                }) => self.interpret_walk(window, cadence, reference, intent, &state),
                _ => Vec::new(),
            },
            // The carrier's intents: the verb table.
            (
                Flow::ReviewWalk {
                    cadence, reference, ..
                },
                UiEffect::ReviewWalk(intent),
            ) => self.interpret_walk(window, *cadence, *reference, intent, &state),
            // Toggle semantics (the settings opener's twin); the
            // planner-context Escape row rides the same route.
            (Flow::None, UiEffect::OpenPlanNextDay) => {
                if self.view_state.view() == crate::view_state::ViewId::PlanNextDay {
                    let mut outcomes = self.view_state.close_plan_next_day();
                    // US-5 (#706): the restore's subject-liveness
                    // guard (see the OpenSettings close arm).
                    outcomes.extend(self.view_state.repair_restored_root(&state));
                    outcomes
                } else {
                    self.view_state.open_plan_next_day()
                }
            }
            // The row verbs (dispatch-time gating: the rows only
            // render on the surface; a stale target is inert). Under
            // the embedded planner the form mounts as the planner's
            // own child level (J4 D-2 — replacing the top flow would
            // kill the planner step).
            (Flow::ReviewPlanner, UiEffect::PlanBeginPlace(task)) => {
                let outcomes = self.view_state.open_plan_place_child(task);
                // The child mount seeds nothing: a placement begun
                // from the pool carries no prior instant (the seed
                // rule reads the placed box, not the pool row).
                outcomes
            }
            (Flow::None, UiEffect::PlanBeginPlace(task)) => {
                let outcomes = self.view_state.open_plan_place(task);
                // Re-opening a placed box edits that box (D-3): the
                // form seeds from the row's instant — tomorrow's box
                // only (a stale scheduled stamp from any earlier day
                // seeds nothing; the commit would re-place to
                // tomorrow). The seed rides the freshly minted level.
                if let Some(scheduled) = state.tasks.get(&task).and_then(|t| t.scheduled) {
                    let (next_start, _) = taskboard_domain::days::next_day_window(
                        self.day_ctx().now,
                        self.day_start.0,
                        self.day_start.1,
                    );
                    if scheduled >= next_start {
                        let phrase = crate::presentation::plan_box_time_phrase(
                            Some(scheduled),
                            self.calendar(),
                        );
                        if let Some(place) = self.view_state.plan_place_mut() {
                            place.time_text = phrase;
                        }
                    }
                }
                outcomes
            }
            (Flow::None | Flow::ReviewPlanner, UiEffect::PlanRemoveBox(task)) => {
                // The unplace's wipe is the level's drop (D-H2).
                self.view_state.remove_plan_box(task)
            }
            (
                Flow::None | Flow::ReviewPlanner,
                effect @ (UiEffect::PlanBoxEarlier(_) | UiEffect::PlanBoxLater(_)),
            ) => {
                let (UiEffect::PlanBoxEarlier(task) | UiEffect::PlanBoxLater(task)) = effect else {
                    unreachable!("arm matched")
                };
                self.swap_plan_box(task, effect == UiEffect::PlanBoxEarlier(task), &state)
            }
            // nav.show-task (round-3 D23): acts on the palette's
            // selection — the Task retarget rides `palette_jump`'s
            // `show_task` arm, the other targets keep today's
            // retargets, a stale/absent cursor closes only. Inert
            // dispatched outside a palette flow (no preset binds it).
            (Flow::Palette { .. }, UiEffect::ShowTask) => {
                let selected = match self.view_state.flow() {
                    Flow::Palette { selected } => *selected,
                    _ => 0,
                };
                match self.tables.palette.get(selected).copied() {
                    Some(target) => self.view_state.palette_jump(&state, target),
                    None => self.view_state.close_palette(),
                }
            }
            (Flow::None, UiEffect::ShowTask) => Vec::new(),
            // A mode flip with no view meaning stays plain shell
            // state (D10) — e.g. Escape out of an empty insert.
            (Flow::None, UiEffect::SetMode(mode)) => {
                self.shell.apply(&UiEffect::SetMode(mode), 0);
                Vec::new()
            }
            // The mask's own walk (US-2/W-2, D-W2): the control walk
            // inside a masked site is the mask's traversal — kept
            // (no view mutation; the walk's clamp consumes it in this
            // table's post-tail), never a settle trigger. The masked
            // set is the clamp's host census: the palette, capture,
            // focus overlay, entries family, recur editor, and labels
            // overlay.
            (
                Flow::Palette { .. }
                | Flow::Capture
                | Flow::FocusMode { .. }
                | Flow::TrackEntries { .. }
                | Flow::TrackEntryForm { .. }
                | Flow::EditRecur { .. }
                | Flow::Labels { .. },
                UiEffect::ControlNext | UiEffect::ControlPrev,
            ) => Vec::new(),
            // Round 3 (W-SM-b): the wildcard arm is gone. Every
            // remaining pair — a foreign flow meeting an effect its
            // kept arms don't own — goes through the named resolver:
            // settle per the flow's kind rule, then re-run (R3/R4).
            // No `(Flow, effect)` pair is silent any more; the
            // compiler enforces the resolver's totality over
            // `EffectClass` and the pair-table test pins every cell
            // to the audit matrix.
            (_, effect) => {
                self.resolve_conflict(window, &flow, effect, &state);
                Vec::new()
            }
        };
        self.apply_outcomes(window, &outcomes);
        if focus_walk || show_task {
            self.push_focus_scroll_hint(window, focus_before);
        }
        if drag_teardown {
            // After the outcomes: the cancel/confirm has ended the
            // view-state flow; the synthetic exit now tears the
            // native drag down and its `drag-finished(none)` echo is
            // absorbed by the flow-guarded seam.
            Self::teardown_runtime_drag(window);
        }
        // The control walk (round-3 D25): the traversal re-injection
        // runs after the outcomes, outside the table — the walk never
        // mutates view state, it moves slint focus only. Under a
        // masked site the walk clamps instead (US-2/W-2, D-W2): the
        // cursor steps within the host's extent and wraps at the mask
        // receive focus. The degenerate extent (≤ 1) stays on 0.
        // J3 R-4 (round 9 chunk 9): the directional table's
        // control-walk views ride the same injection — Left/Up walk
        // backwards, Right/Down forwards. #1034: Tab/Shift-Tab is the
        // same decision's special case — the one
        // [`Self::control_walk_step`] function states the whole walk
        // half of the stop ladder, and the tail consumes its verdict
        // once (the old double `directional_control_step` call is
        // gone).
        if let Some(backwards) = self.control_walk_step(&state, effect) {
            if self.mask_armed(window) {
                let facts = window.global::<crate::MaskWalkFacts>();
                let extent = facts.get_extent();
                let cursor = facts.get_cursor();
                let next = if extent <= 1 {
                    0
                } else if backwards {
                    (cursor + extent - 1) % extent
                } else {
                    (cursor + 1) % extent
                };
                window.invoke_mask_focus(next);
            } else {
                let flag = self.walk_reentry.clone();
                Self::dispatch_traversal(window, &flag, backwards, &self.walk_timer);
            }
        }
        // The scroll actions (round-3 D26): the focused container
        // pages after the outcomes (the tables are fresh).
        if matches!(effect, UiEffect::ScrollUp | UiEffect::ScrollDown) {
            self.push_scroll_page_hint(window, effect == UiEffect::ScrollDown);
        }
    }

    /// The scroll actions' container resolution (round-3 D26): the
    /// focused task's column on the wide board, the selected stack's
    /// viewport on the compact board and the stack detail, the focused
    /// task's section on today, the settings pane adapter-side through
    /// the two-way facts, and a typed inert on the task detail (no
    /// container) or without a focused task (the ends hold).
    fn push_scroll_page_hint(&mut self, window: &AppWindow, down: bool) {
        const FAMILY_SETTINGS: i32 = 0;
        const FAMILY_BOARD: i32 = 1;
        const FAMILY_TODAY: i32 = 2;
        const FAMILY_STACK: i32 = 3;
        const FAMILY_REVIEW: i32 = 4;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let page = if down { 1 } else { -1 };
        let state = self.state.load_full();
        let hint: Option<(i32, i32)> = match (self.view_state.view(), Self::is_compact(window)) {
            (crate::view_state::ViewId::Board, false) => self
                .view_state
                .focused_task()
                .and_then(|focused| self.tables.board_position(focused))
                .map(|(column, _)| (FAMILY_BOARD, to_hint_index(column))),
            (crate::view_state::ViewId::Board, true) => {
                // The compact leg's selected viewport: the selected
                // column (the focused task's, when the walk seeded one).
                let selected = self.view_state.selected_stack();
                let column = selected.and_then(|stack| {
                    self.view_state
                        .stack_ids(&state)
                        .iter()
                        .position(|s| *s == stack)
                });
                column.map(|c| (FAMILY_STACK, to_hint_index(c)))
            }
            (crate::view_state::ViewId::StackDetail, _) => Some((FAMILY_STACK, -1)),
            // The planner's two panes are a fixed surface (#622): no
            // paging container — a typed inert like the task detail.
            (crate::view_state::ViewId::PlanNextDay | crate::view_state::ViewId::TaskDetail, _) => {
                None
            }
            // The status panel (OP3): one whole-page viewport, no
            // paging container — the same typed inert.
            (crate::view_state::ViewId::Status, _) => None,
            (crate::view_state::ViewId::Today, _) => self
                .view_state
                .focused_task()
                .and_then(|focused| self.tables.today_position(focused))
                .map(|(section, _)| (FAMILY_TODAY, to_hint_index(section))),
            (crate::view_state::ViewId::Settings, _) => {
                Self::page_settings(window, down);
                return;
            }
            // The review page (#214): one whole-page viewport, the
            // view's own ListView pages as family 4.
            (crate::view_state::ViewId::Review, _) => Some((FAMILY_REVIEW, -1)),
        };
        // The settings family only ever rides the facts path.
        let Some((family, index)) = hint.filter(|(family, _)| *family != FAMILY_SETTINGS) else {
            return;
        };
        let scroll = window.global::<crate::ScrollPageHint>();
        scroll.set_family(family);
        scroll.set_index(index);
        scroll.set_page(page);
        scroll.set_nonce(scroll.get_nonce() + 1);
    }

    /// The settings pane's adapter-side paging (round-3 D26): the
    /// facts global already carries the heights and the two-way
    /// `viewport-y` — the adapter writes the clamped target and no
    /// markup change is needed. Clamps at both ends by construction.
    fn page_settings(window: &AppWindow, down: bool) {
        let facts = window.global::<crate::SettingsScrollFacts>();
        let viewport = facts.get_viewport_height();
        let content = facts.get_content_height();
        if viewport <= 0.0 || content <= viewport {
            return;
        }
        let max = content - viewport;
        let target = if down {
            facts.get_viewport_y() + viewport
        } else {
            facts.get_viewport_y() - viewport
        };
        facts.set_viewport_y(target.clamp(0.0, max));
    }

    /// The focus-into-view seam (#373 design round §3a, D-5): after a
    /// keyboard focus move's outcomes — the refresh has rebuilt the
    /// models and the id tables — resolve the newly focused task
    /// through the tables (global model indices, never
    /// viewport-relative) and edge-trigger the `FocusScrollHint`
    /// global. The matching `ListView` instance consumes it and scrolls
    /// the row into view; pointer selections are visible by
    /// construction and hint nothing. A move that changed nothing
    /// (the walk's ends) re-arms nothing.
    /// The one-shot arm's consumption (UX-R8 chunk-5 D-7): `Some`
    /// = the handoff fired (the arm consumed with exactly one new
    /// id) — the hint push edge-triggers on the handoff, never on a
    /// prior focus. The arm always clears.
    fn consume_create_focus_arm(&mut self) -> Option<CreateFocusHandoff> {
        let arm = self.create_focus_arm.clone()?;
        let state = self.state.load_full();
        if Arc::ptr_eq(&state, &arm.snapshot) {
            // The engine has not applied yet (the commit's own
            // RefreshNow pass) — the arm holds for the publish.
            return None;
        }
        let live: std::collections::BTreeSet<TaskId> = state
            .tasks
            .values()
            .filter(|task| task.stack == arm.stack)
            .map(|task| task.id)
            .collect();
        drop(state);
        self.create_focus_arm = None;
        let mut new_ids = live.difference(&arm.before).copied();
        let (Some(created), None) = (new_ids.next(), new_ids.next()) else {
            // Zero or >1 new ids: a refused create or a racing capture
            // — no handoff (the one-shot law).
            return None;
        };
        let before = self.view_state.focused_task();
        self.view_state.set_focused(Some(created));
        Some(CreateFocusHandoff { before })
    }

    fn push_focus_scroll_hint(&mut self, window: &AppWindow, before: Option<TaskId>) {
        let focused = self.view_state.focused_task();
        if focused.is_none() || focused == before {
            return;
        }
        let focused = focused.expect("checked above");
        // A slint int carries the index; the tables' lengths are far
        // below i32 reach.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let hint = match self.view_state.view() {
            crate::view_state::ViewId::Board => self
                .tables
                .board_position(focused)
                .map(|(column, row)| (to_hint_index(column), to_hint_index(row))),
            crate::view_state::ViewId::Today => self
                .tables
                .today_position(focused)
                .map(|(section, row)| (to_hint_index(section), to_hint_index(row))),
            crate::view_state::ViewId::StackDetail => self
                .tables
                .stack_position(focused)
                .map(|row| (-1, to_hint_index(row))),
            // The review page carries no focusable rows (D-8): the
            // focus-into-view seam never targets it, like the settings
            // facts path and the detail's own container.
            crate::view_state::ViewId::TaskDetail
            | crate::view_state::ViewId::Settings
            | crate::view_state::ViewId::Review
            // The planner's panes are a fixed surface (#622): no
            // paging container — the same typed inert.
            | crate::view_state::ViewId::PlanNextDay
            // The status panel: no paging container (OP3).
            | crate::view_state::ViewId::Status => None,
        };
        let Some((target_a, target_b)) = hint else {
            return;
        };
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(target_a);
        scroll.set_target_b(target_b);
        scroll.set_nonce(scroll.get_nonce() + 1);
    }

    /// The move-flow walk's scroll hint (UX-R8 D-3): the armed
    /// `Flow::Move`'s (target, slot) as a `FocusScrollHint` edge —
    /// `target-a` = the aimed column (this pass's stack index),
    /// `target-b` = the gap's clamp row `min(gap, len-1)` (a gap
    /// after the last card seeks the last card). Inert without a
    /// live move flow (the stale-hint guard's shape).
    fn push_move_scroll_hint(&mut self, window: &AppWindow, state: &taskboard_domain::AppState) {
        let Flow::Move { target, slot, .. } = self.view_state.flow() else {
            return;
        };
        let (target, slot) = (*target, *slot);
        let Some(column) = self
            .view_state
            .stack_ids(state)
            .iter()
            .position(|stack| *stack == target)
        else {
            return;
        };
        let len = state.tasks_in_stack(target).len();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(to_hint_index(column));
        scroll.set_target_b(to_hint_index(slot.min(len.saturating_sub(1))));
        scroll.set_nonce(scroll.get_nonce() + 1);
    }

    /// The control walk's traversal re-injection (round-3 D25): the
    /// synthetic `Tab` key press drives the runtime's own tab
    /// traversal (`window.rs` key handler); the previous direction is
    /// the Shift-wrapped variant (the journey driver's modifier
    /// technique, the `teardown_runtime_drag` synthetic-event
    /// precedent). `walk_reentry` is held across the dispatch so the
    /// injected stroke rejects out of `intercept-key` before the
    /// callback borrows — one press = exactly one stop, never a loop.
    fn dispatch_traversal(
        window: &AppWindow,
        walk_reentry: &Rc<std::cell::Cell<bool>>,
        backwards: bool,
        timer: &slint::Timer,
    ) {
        // Deferred (see the field's doc): the callback fires outside
        // the current key event, where the synthetic Tab reaches the
        // window's traversal fallback instead of dying re-entrantly.
        let weak = window.as_weak();
        let reentry = Rc::clone(walk_reentry);
        timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(0),
            move || {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                reentry.set(true);
                let win = window.window();
                let tab: slint::SharedString = "\t".into();
                let inject = |win: &slint::Window, tab: &slint::SharedString| {
                    if backwards {
                        let shift: slint::SharedString =
                            slint::platform::Key::into(slint::platform::Key::Shift);
                        win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: shift.clone(),
                        });
                        win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: tab.clone(),
                        });
                        win.dispatch_event(slint::platform::WindowEvent::KeyReleased {
                            text: shift,
                        });
                    } else {
                        win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: tab.clone(),
                        });
                    }
                };
                // The text-input dead-end (#1203, round 5 A2): a
                // `TextInput` accepts the synthetic Tab without moving
                // the traversal — the step that LANDS on a field is a
                // legitimate stop (the fields are ladder stops, the
                // walk-coverage inventories' reading-order contracts),
                // but a step FROM a field went nowhere and the walk
                // dead-ended (the detail's lower half was unreachable
                // past the effort field). When the walk departs a
                // field, re-inject until the traversal actually moves
                // off text focus (bounded — one press stays one stop,
                // never a loop).
                let departing_a_field = window
                    .global::<crate::EditingFacts>()
                    .get_text_focus_count()
                    > 0;
                inject(win, &tab);
                if departing_a_field {
                    for _ in 0..3 {
                        if window
                            .global::<crate::EditingFacts>()
                            .get_text_focus_count()
                            == 0
                        {
                            break;
                        }
                        inject(win, &tab);
                    }
                }
                reentry.set(false);
            },
        );
    }

    /// The native drag's teardown seam (r2 chunk-8 D-G): a synthetic
    /// `PointerExited` — the runtime tears any in-flight drag down
    /// and fires `drag-finished(none)` — absorbed by the flow-guarded
    /// seam: the view-state flow is already gone, and `TouchArea`'s
    /// Exit arm clears `pressed`/hover so the real release cannot
    /// degrade into `clicked`. The one synthetic event: the runtime
    /// exposes no other public in-flight-drag cancel.
    fn teardown_runtime_drag(window: &AppWindow) {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
    }

    /// vi's insert intent (D5): `i`/`a` open the view's editing flow
    /// rather than flipping mode into the void; `TaskDetail` edits its
    /// subject in place.
    fn interpret_insert(&mut self, state: &taskboard_domain::AppState) -> Vec<FlowOutcome> {
        match self.view_state.view() {
            crate::view_state::ViewId::TaskDetail => self.view_state.edit_subject(state),
            _ => match self.view_state.create_target_stack(state) {
                Some(stack) => self.view_state.begin_create(stack),
                None => Vec::new(),
            },
        }
    }

    /// The no-flow begin intents (D5/D9), factored out of the table.
    /// The stack-create refusal (a never-booted session offers nothing
    /// to create a stack on) stays in the view-state method — the one
    /// home for the board-existence refusal.
    fn interpret_begin(
        &mut self,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::Activate => self.view_state.activate_focused(state),
            UiEffect::RequestMove => self.view_state.start_move(state),
            UiEffect::BeginCreateStack => self.view_state.begin_create_stack(state),
            // The ghost's clicked column (r4 D4): the payload wins.
            UiEffect::BeginCreate {
                target: Some(stack),
            } => self.view_state.begin_create(stack),
            // The keyboard's route: focused → selected → first,
            // byte-identical (r4 D4 acceptance e).
            _ => match self.view_state.create_target_stack(state) {
                Some(stack) => self.view_state.begin_create(stack),
                None => Vec::new(),
            },
        }
    }

    /// The rule cursor's activation (J3 US-6 D-3, round 9 chunk 12):
    /// the cursor'd row's own affordance set read off the projection's
    /// armed bit — `ReminderRuleRemove` for an armed rule (the one-shot
    /// `rule_target` minted exactly as `on_task_rule_remove_clicked`
    /// mints it) and the due editor for an unarmed one (the row's
    /// jump seam, `begin_date_edit`). Immediate, no staged confirm:
    /// the row's icon-button affordance is immediate today (R-5's
    /// strict parity law). A cursor whose rule died pre-dispatch
    /// re-resolves against the live roster and no-ops (the stale-id
    /// absorb). The cursor itself survives the verb — the roster the
    /// prune re-evaluates against changes under the dispatch, never
    /// by the verb's own hand.
    fn activate_rule_cursor(&mut self, state: &AppState, window: &AppWindow) {
        let Some(rule_id) = self.view_state.rule_cursor() else {
            return;
        };
        let Some(rule) = state.reminders.rules.get(&rule_id) else {
            return; // the stale-id absorb: the dead cursor holds, nothing dispatches
        };
        let subject_duedate = self
            .view_state
            .subject_task()
            .and_then(|task| state.tasks.get(&task))
            .and_then(|task| task.duedate);
        if crate::presentation::rule_armed(rule, subject_duedate) {
            self.rule_target = Some(rule_id);
            self.route(window, ActionId::ReminderRuleRemove);
        } else {
            // The unarmed row's jump affordance (r4 D30): the due
            // editor opens for the subject — the same funnel the
            // row's calendar icon drives.
            self.begin_date_edit(window, crate::view_state::DateField::Due);
        }
    }

    /// The focused planner pane's rows (US-15): the id tables' flat
    /// `plan_rows` split at `plan_box_count` — the exact derivation
    /// the two panes render, so the walk's landing and the rendered
    /// rows agree by construction.
    fn plan_pane_rows(&self) -> &[taskboard_domain::TaskId] {
        let boxes = self.tables.plan_box_count.min(self.tables.plan_rows.len());
        match self.view_state.plan_focus_pane() {
            0 => &self.tables.plan_rows[..boxes],
            _ => &self.tables.plan_rows[boxes..],
        }
    }

    /// The planner cursor's walk (US-15, #1041 — the rule cursor's
    /// `walk_rule_cursor` law over the pane's id table): the first
    /// vertical stroke over a non-empty pane arms the cursor at row 0
    /// (arm-then-step — the arming stroke does not move past row 0);
    /// each further stroke steps within the pane, clamping at both
    /// edges (no wrap — the ends hold). An empty pane is the typed
    /// no-op (never arm). The cursor state is the K4 field in the
    /// view state (R-9), the mechanics live here because the pane's
    /// row source is the adapter-owned id tables.
    fn walk_plan_cursor(&mut self, direction: i32) -> Vec<FlowOutcome> {
        let rows = self.plan_pane_rows().to_vec();
        if rows.is_empty() {
            return Vec::new();
        }
        let current = self
            .view_state
            .plan_cursor()
            .and_then(|cursor| rows.iter().position(|id| *id == cursor));
        let Some(current) = current else {
            self.view_state.select_plan_cursor(rows[0]);
            return vec![FlowOutcome::RefreshNow];
        };
        let next = i64::from(direction) + i64::try_from(current).unwrap_or(0);
        match rows.get(usize::try_from(next).unwrap_or(usize::MAX)) {
            Some(task) => {
                self.view_state.select_plan_cursor(*task);
                vec![FlowOutcome::RefreshNow]
            }
            None => Vec::new(), // clamp: no wrap, the ends hold
        }
    }

    /// The plan-place begin tail (#1237): mint the one-shot
    /// `plan_target`, route `PlanBeginPlace`, then the seed echo — a
    /// placed row's re-open seeds the form from the box's instant
    /// (D-3), a pool row's mounting field shows empty. One helper,
    /// three call sites (the cursor activation and the box/pool
    /// click/AT-action seams); behavior-identical to the inlined
    /// tails it replaces.
    fn begin_plan_place(&mut self, window: &AppWindow, task: TaskId, placed: bool) {
        self.plan_target = Some(task);
        self.route(window, ActionId::PlanBeginPlace);
        if placed {
            window.set_plan_time_text(SharedString::from(
                self.view_state
                    .plan_place()
                    .map_or(String::new(), |place| place.time_text.clone()),
            ));
        } else {
            window.set_plan_time_text(SharedString::new());
        }
    }

    /// The planner cursor's activation (US-15/S3): the cursor'd row's
    /// own seam — `PlanBeginPlace` with the one-shot `plan_target`
    /// minted exactly as the row's click/AT-action seam mints it, the
    /// placed row's instant seed and the pool row's empty seed riding
    /// verbatim. A cursor whose task died pre-dispatch absorbs (the
    /// stale-id rule): the cursor holds, nothing dispatches (the
    /// refresh clamp prunes it at the next pass). The cursor survives
    /// the verb — the clamp, never the verb's own hand, retires it.
    fn activate_plan_cursor(&mut self, window: &AppWindow) {
        let Some(task) = self.view_state.plan_cursor() else {
            return;
        };
        let Some(flat) = self.tables.plan_rows.iter().position(|id| *id == task) else {
            return; // the stale-id absorb
        };
        self.begin_plan_place(window, task, flat < self.tables.plan_box_count);
    }

    /// The edit flows' commit/discard arms (D5/D9; the round-2
    /// lifecycle flows included), factored out of the table.
    fn interpret_edit(
        &mut self,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::SetMode(Mode::Normal) => self.view_state.discard_edit(),
            // The table routes only the commit/discard pair here.
            _ => self.view_state.commit_edit(state),
        }
    }

    /// The description editor's arms (round-3 D15), factored out of
    /// the table: Escape discards; Activate commits through the
    /// description funnel (the buffer verbatim — empty is the clear).
    fn interpret_description(
        &mut self,
        effect: UiEffect,
        _state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::SetMode(Mode::Normal) => self.view_state.discard_edit(),
            _ => self.view_state.commit_description(),
        }
    }

    /// The control walk's decision function (#1034): the whole walk
    /// half of the stop ladder in one place. Tab steps forwards,
    /// Shift-Tab backwards (the round-3 D25 re-injection), and a
    /// directional stroke is the per-view table's verdict
    /// ([`Self::directional_control_step`]); every other effect is
    /// not a walk. Pure.
    fn control_walk_step(
        &self,
        state: &taskboard_domain::AppState,
        effect: UiEffect,
    ) -> Option<bool> {
        match effect {
            UiEffect::ControlNext => Some(false),
            UiEffect::ControlPrev => Some(true),
            UiEffect::FocusDirectional(_) => self.directional_control_step(state, effect),
            _ => None,
        }
    }

    /// The directional control-walk decision (J3 R-4 first cut, the
    /// per-view table's horizontal rows): `Some(backwards)` when the
    /// (view, direction) pair reads as a control step, `None` when
    /// the view's direction has outcomes of its own (the item walks,
    /// the board's stack step, the planner's pane switch) or the
    /// effect is not directional. Pure — the interpret table's
    /// [`Flow::None`] arm and the post-table tail both read it.
    fn directional_control_step(
        &self,
        state: &taskboard_domain::AppState,
        effect: UiEffect,
    ) -> Option<bool> {
        let UiEffect::FocusDirectional(direction) = effect else {
            return None;
        };
        let backwards = matches!(direction, Direction::Left | Direction::Up);
        match self.view_state.view() {
            // Today/StackDetail: left/right are the control walk
            // first cut (R-4); up/down are the item walks (the
            // `interpret_directional` outcomes own them — `None`
            // here, the tail must not inject a traversal step).
            crate::view_state::ViewId::Today | crate::view_state::ViewId::StackDetail => {
                matches!(direction, Direction::Left | Direction::Right).then_some(backwards)
            }
            // TaskDetail: the rule cursor owns the verticals whenever
            // the roster can arm one (J3 US-6 D-5, round 9 chunk 12 —
            // `interpret_directional` returns outcomes — arm, step,
            // or clamp — and the tail must not ALSO inject a traversal
            // step); an empty roster (nothing to arm) and every
            // horizontal stay the control walk (chunk 9's row,
            // byte-identical while disarmed).
            crate::view_state::ViewId::TaskDetail
                if matches!(direction, Direction::Up | Direction::Down)
                    && self.view_state.rule_walk_owns_stroke(state) =>
            {
                None
            }
            // TaskDetail/Settings: every other direction is the control
            // walk — the stop ladder is linear today (the recorded
            // degenerate horizontal: up/down ride the same ladder).
            crate::view_state::ViewId::TaskDetail
            | crate::view_state::ViewId::Settings
            // The status panel's ladder is linear too (OP3): the
            // Refresh button is the walk's one stop.
            | crate::view_state::ViewId::Status => Some(backwards),
            _ => None,
        }
    }

    /// The directional-focus per-view table (J3 US-1, round 9 chunk
    /// 9): the one declarative home for what a direction means on
    /// each surface — focus movement, never task selection. Every
    /// audited view carries a row or an explicit exclusion comment.
    /// The control-walk views return no outcomes (the post-table
    /// tail injects the traversal — see
    /// [`Self::directional_control_step`]).
    #[allow(clippy::match_same_arms)] // one arm per table decision, comments included
    fn interpret_directional(
        &mut self,
        state: &taskboard_domain::AppState,
        direction: Direction,
        compact: bool,
        day: &crate::view_state::DayCtx,
    ) -> Vec<FlowOutcome> {
        use crate::view_state::ViewId;
        match (self.view_state.view(), direction) {
            // Board: left/right step the stack walk (the
            // `UiEffect::StackNext/Prev` bodies); up/down walk the
            // task cards.
            (ViewId::Board, Direction::Left | Direction::Right) => {
                let step = i32::from(direction == Direction::Right) * 2 - 1;
                self.view_state.step_stack(state, step)
            }
            // The item walks (Board/Today/StackDetail up/down — the
            // FocusNext/Prev bodies; habit rows ride the Today
            // substrate).
            (ViewId::Board | ViewId::Today | ViewId::StackDetail, Direction::Down) => self
                .view_state
                .advance_focus(state, compact, self.kiosk, day),
            (ViewId::Board | ViewId::Today | ViewId::StackDetail, Direction::Up) => self
                .view_state
                .retreat_focus(state, compact, self.kiosk, day),
            // Today/StackDetail left/right: the control walk first cut
            // (R-4) — the tail owns it, no outcomes here.
            (ViewId::Today | ViewId::StackDetail, Direction::Left | Direction::Right) => Vec::new(),
            // TaskDetail's verticals walk the rule cursor (J3 US-6
            // D-5): the list's geometry is vertical, the declarative
            // arm sits beside the control-walk row. The arming gate
            // lives in `walk_rule_cursor` — a stroke over a non-empty
            // roster arms at row 0 then walks (the arming stroke does
            // not move past row 0, D-2's arm-then-step); an empty
            // roster is the typed no-op and the tail's control walk
            // proceeds (the D-5 negative — `directional_control_step`
            // returns `None` exactly when this arm owns the stroke).
            (ViewId::TaskDetail, Direction::Up | Direction::Down) => {
                let step = i32::from(direction == Direction::Down) * 2 - 1;
                self.view_state.walk_rule_cursor(state, step)
            }
            // TaskDetail/Settings: the control walk along the stop
            // ladder — NEVER the subject walk (J3 R-3: the viewed task
            // does not change from the detail at all). The ladder is
            // linear, so the horizontal degenerates onto it (the
            // recorded geometry; AC-1 accepts the per-geometry
            // reading) and both axes at Settings walk (r4 D19
            // behavior kept). The tail owns the rows.
            (ViewId::TaskDetail | ViewId::Settings | ViewId::Status, _) => Vec::new(),
            // The planner (J3 R-5): left/right switch the pane
            // (boxes ↔ pool); up/down arm and walk the K4 cursor over
            // the focused pane's rows (US-15, #1041 — the recorded
            // typed inert retires; R-9: the cursor rides the K4 state,
            // never a second cursor mechanism).
            (ViewId::PlanNextDay, Direction::Left) => self.view_state.switch_plan_pane(-1),
            (ViewId::PlanNextDay, Direction::Right) => self.view_state.switch_plan_pane(1),
            (ViewId::PlanNextDay, Direction::Up | Direction::Down) => {
                let step = i32::from(direction == Direction::Down) * 2 - 1;
                self.walk_plan_cursor(step)
            }
            // Review: no `NavFocus*` preset rows are bound in its
            // context (the CONTROL_NAV set excludes it) — the
            // explicit exclusion; the surface keeps its own cursors.
            (ViewId::Review, _) => Vec::new(),
        }
    }

    /// The move flows' arms (D8/D9; the round-2 stack move joins),
    /// factored out of the table.
    fn interpret_move(
        &mut self,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
        window: &AppWindow,
    ) -> Vec<FlowOutcome> {
        match (flow, effect) {
            // Focus next/prev cycle the target; Activate confirms;
            // every cancel-shaped effect (the dedicated verb — Escape
            // / emacs C-g —, the toggle re-request, and
            // SetMode(Normal), which no longer falls through to the
            // inert default leaving the flow intact) ends the flow
            // without dispatching (D16/F-E2). The directional
            // up/down kept arm (J3 US-2, round 9 chunk 9) rides the
            // same ring byte-identically.
            (
                _,
                effect @ (UiEffect::FocusNext
                | UiEffect::FocusPrev
                | UiEffect::FocusDirectional(Direction::Up | Direction::Down)),
            ) => {
                let direction = match effect {
                    UiEffect::FocusNext | UiEffect::FocusDirectional(Direction::Down) => 1,
                    _ => -1,
                };
                let outcomes = match flow {
                    Flow::Move { .. } => self.view_state.cycle_move_target(state, direction),
                    _ => self.view_state.cycle_move_stack_target(state, direction),
                };
                // The walk's scroll-into-view half (UX-R8 D-3, US-1/S2):
                // the aimed column's viewport seeks the aim point —
                // `target-a` = aimed column, `target-b` = the gap's
                // clamp row (`min(gap, len-1)`), nonce re-edge. The
                // hint filters per-ListView (both legs read it); no
                // new hint family.
                if !outcomes.is_empty() {
                    self.push_move_scroll_hint(window, state);
                }
                outcomes
            }
            // The directional left/right kept arms (J3 US-2): during
            // a card move the horizontal keys hop the drop *column*
            // (`cycle_move_stack_column` — the row index survives,
            // clamped); during a stack move they hop the insertion
            // slot (the same ring the FocusNext/Prev kept arm runs —
            // the slot domain is horizontal there). The move survives
            // every hop (US-2/AC-1). #1033: the legacy stack-walk
            // verbs (`StackNext`/`StackPrev` — the `g l`/`g h` ViNormal
            // chords and the Standard/emacs `Ctrl+Shift+Left/Right`
            // rows in `actions.rs`) join the kept arms with the same
            // reading: `StackNext` → +1, `StackPrev` → -1. The
            // `(Flow::None, StackNext/Prev)` arms below stay verbatim
            // — outside a move flow the stack walk steps as before.
            (
                _,
                effect @ (UiEffect::FocusDirectional(Direction::Left | Direction::Right)
                | UiEffect::StackNext
                | UiEffect::StackPrev),
            ) => {
                let direction = match effect {
                    UiEffect::FocusDirectional(Direction::Right) | UiEffect::StackNext => 1,
                    _ => -1,
                };
                let outcomes = match flow {
                    Flow::Move { .. } => self.view_state.cycle_move_stack_column(state, direction),
                    _ => self.view_state.cycle_move_stack_target(state, direction),
                };
                if !outcomes.is_empty() {
                    self.push_move_scroll_hint(window, state);
                }
                outcomes
            }
            (Flow::Move { .. }, UiEffect::Activate) => self.view_state.confirm_move(state),
            (Flow::MoveStack { .. }, UiEffect::Activate) => {
                self.view_state.confirm_move_stack(state)
            }
            _ => self.view_state.cancel_move(),
        }
    }

    /// The lifecycle verbs' arms (round 2, D18/D21), factored out of
    /// the table to keep it readable: deletes dispatch through the
    /// view state (the engine owns every typed refusal), renames and
    /// the board create open their fields, the stack move toggles.
    fn interpret_lifecycle(
        &mut self,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::StackDelete => self.view_state.delete_stack(state),
            UiEffect::StackRename => self.view_state.begin_rename_stack(state),
            UiEffect::StackMove => match flow {
                Flow::None => self.view_state.begin_move_stack(state),
                Flow::MoveStack { .. } => self.view_state.cancel_move(), // toggle
                _ => Vec::new(),
            },
            UiEffect::BoardCreate => self.view_state.begin_create_board(),
            UiEffect::BoardDelete => self.view_state.delete_board(state),
            UiEffect::BoardRename => self.view_state.begin_rename_board(state),
            UiEffect::BoardColorEdit => self.view_state.begin_board_color(state),
            UiEffect::BoardDefaultPreset => {
                // The dispatch-time empty-board gate (UX R-4 D-5,
                // #795): the CTA renders only on the empty-board leg,
                // so the pointer path cannot fire on content — the
                // key could; the typed no-op is the safe side (a sync
                // refresh mid-stroke flips the verdict to inert, the
                // recorded R-3 law). The board-less refusal stays
                // apply_board_preset's.
                let Some(board) = self.view_state.board() else {
                    return Vec::new();
                };
                let empty = !state
                    .stacks
                    .values()
                    .any(|stack| stack.board == board && !stack.deleted);
                if empty {
                    self.view_state
                        .apply_board_preset(state, &crate::board_presets::DEFAULT_KANBAN)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// The board ceremony verbs' arms (OP1, US-1/US-2/US-3): the view
    /// state owns the arm/confirm split (the `interpret_lifecycle`
    /// shape); the reconcile arm additionally sends the oneshot
    /// preview request when the arm is fresh — the reply drains into
    /// the flow's `ReconcilePreviewState` under the repaint cadence.
    fn interpret_ceremony(
        &mut self,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        // The kiosk gate (OP1 D-5, the structural exclusion): the
        // board surface does not mount on kiosk (board-stack plan
        // PC-4 §6), so the verbs are absent there — no armed letter,
        // no preview request, no engine command (the
        // `TrackEntriesOpen` refusal shape; no disabled affordances
        // are minted for a surface that does not exist).
        if self.kiosk {
            return Vec::new();
        }
        match effect {
            UiEffect::BoardArchive => self.view_state.begin_board_archive(state),
            UiEffect::BoardReset => self.view_state.begin_board_reset(state),
            UiEffect::BoardReconcile => {
                let outcomes = self.view_state.begin_board_reconcile(state);
                if let crate::view_state::Flow::ConfirmBoardReconcile {
                    board,
                    preview: crate::view_state::ReconcilePreviewState::Pending,
                } = self.view_state.flow().clone()
                    && self
                        .reconcile_preview
                        .as_ref()
                        .is_none_or(|(pending, _)| *pending == board)
                {
                    let (reply, reply_rx) = tokio::sync::oneshot::channel();
                    let request =
                        crate::requests::UiRequest::BoardReconcilePreview { board, reply };
                    if self.requests.try_send(request).is_ok() {
                        self.reconcile_preview = Some((board, reply_rx));
                    } else {
                        // The lane is full or closed (degraded app):
                        // the strip stays armed in Pending — the
                        // unlettered confirm is refused view-state
                        // side (D-4's staging law), and the user's
                        // Escape/Cancel disarms. The typed failure
                        // fact is the dead reply lane's (the drain's
                        // Failed arm), not the send's.
                        tracing::warn!(
                            "request channel full or closed; the reconcile preview was dropped"
                        );
                    }
                }
                outcomes
            }
            _ => Vec::new(),
        }
    }

    /// The `OpenMenu` settle hook (audit §5): the menu effects are
    /// applied in `apply_effect`, before the table — so the matrix's
    /// "`OpenMenu` settles a foreign flow" cell (the hamburger face
    /// of F9) lives here, at the application layer. The menu-internal
    /// effects run through unchanged (they only arrive while the
    /// menu is open, and an open menu masks flows via the chain).
    fn settle_foreign_flow_for_modal(&mut self, window: &AppWindow) {
        let state = self.state.load();
        self.settle_flow(window, &state);
    }

    /// The flow kind the matrix rows range over (audit §6).
    fn flow_kind(flow: &Flow) -> FlowKind {
        match flow {
            // The modals (the concentration overlay #496; the entries
            // overlay's two surfaces, r4 chunk 10) and the empty flow
            // read the same: the matrix's overlay cells cover them —
            // the kept arms own their opens/closes, settle leaves the
            // overlay (its close paths are its exits).
            Flow::FocusMode { .. }
            | Flow::TrackEntries { .. }
            | Flow::TrackEntryForm { .. }
            // The labels overlay (UX R-3 D-2, #789): the modals' row
            // — its kept arms own the open/dismiss; settle is its
            // close (the overlay family's row).
            | Flow::Labels { .. }
            // The color picker (UX R-4 D-1, #791): the armed-strip's
            // row — settle is its disarm; the strip's seams own the
            // commit.
            | Flow::BoardColorPick { .. }
            // The placement form (#622): the kept planner arms own
            // its intents and settle leaves the form (its exits are
            // its close paths) — the overlay cells' shape.
            | Flow::PlanPlace { .. }
            // The review walk (#638): the fifth modal — its kept arms
            // own the walk's intents; settle is its close (the
            // palette family's terminator).
            | Flow::ReviewWalk { .. }
            // The embedded planner child (J4 D-2): the modals' row —
            // its close paths are its exits (the Cancelled/Completed
            // exit contract advances the walk's stage through
            // `on_child_exit`).
            | Flow::ReviewPlanner
            // The recurrence editor (round-7 D-7.1): the sixth modal —
            // settle is its clean close (the overlay family's row; a
            // foreign settle never commits a draft).
            | Flow::EditRecur { .. }
            // The child levels (chunk 3's ports): the modals' row —
            // the children have no kept intents of their own; their
            // close paths are their exits and their settle arm is the
            // Cancelled end (D5).
            | Flow::EntryDatePicker
            | Flow::WalkFilePick { .. }
            | Flow::WalkDatePick
            // The armed-delete strip (UX R-2 D-2): the modals' row —
            // settle is its disarm (nothing dispatches); its kept
            // intents own the confirm/re-arm.
            | Flow::ConfirmDelete(_)
            // The board ceremony strips (OP1): the armed-delete's row
            // — the foreign settle resolves through the view state's
            // cancel arms; the strips' own seams own the confirm.
            | Flow::ConfirmBoardArchive { .. }
            | Flow::ConfirmBoardReset { .. }
            | Flow::ConfirmBoardReconcile { .. }
            | Flow::None => FlowKind::None,
            Flow::Create { .. }
            | Flow::CreateStack
            | Flow::CreateBoard
            | Flow::EditTitle { .. }
            // The description behaves as a text flow everywhere
            // (settle/leaving rules) — the kind vocabulary only.
            | Flow::EditDescription { .. }
            | Flow::RenameStack { .. }
            | Flow::RenameBoard { .. } => FlowKind::Text,
            Flow::EditDate { .. } => FlowKind::Due,
            Flow::Move { .. } | Flow::MoveStack { .. } => FlowKind::Move,
            Flow::DragMove { .. } => FlowKind::Drag,
            Flow::Palette { .. } => FlowKind::Palette,
            Flow::Capture => FlowKind::Capture,
        }
    }

    /// The R3/R4 conflict resolver (round-3 W-SM-b, plan §3): the
    /// named classifier that replaced the wildcard arm. Its match is
    /// exhaustive over [`EffectClass`] — the compiler enforces the
    /// totality, and the pair-table test pins every `(Flow, effect)`
    /// cell to the audit matrix.
    fn resolve_conflict(
        &mut self,
        window: &AppWindow,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) {
        if matches!(flow, Flow::None) {
            // Unreachable from the offer walk: every effect class has
            // a `(Flow::None, _)` arm above, and the menu/system
            // effects are applied in `apply_effect` before the table
            // (audit §5). The pair-table harness drives this resolver
            // with the empty flow directly, so the guard stays.
            return;
        }
        match Self::resolve(Self::flow_kind(flow), effect.class()) {
            Resolve::Inert => {}
            Resolve::CancelOnly => {
                // Settle only — the effect is consumed by the settle
                // (the K cells: the due editor's C-g clean close, the
                // modals' cancel). The cancel IS the site's own exit
                // arm — the origin survives it and the restore fires.
                self.settle_flow(window, state);
            }
            Resolve::Replace => {
                // The one X: the capture overlay's fire-drill replace
                // without committing (chunk-10 D8).
                debug_assert!(matches!(effect, UiEffect::BeginCapture));
                // T-31 (#814): the replacement announces itself — the
                // typed fact arms before the overlay mounts (the
                // settle rule still bounds data-at-risk; the notice
                // adds only the announcement).
                self.arm_overlay_replacement_notice(window);
                let was_drag = matches!(flow, Flow::DragMove { .. });
                let outcomes = self.view_state.begin_capture(state);
                self.width_drag = None;
                self.scale_drag = None;
                self.apply_outcomes(window, &outcomes);
                if was_drag {
                    Self::teardown_runtime_drag(window);
                }
            }
            Resolve::SettleThenRun => {
                // The foreign settle hands the focus to the re-run
                // effect (the walk carries its own target) — the
                // census restore arms (US-1/W-1) belong to the site's
                // own exits, so the origin is dropped when the settle
                // clears the recorded site (the walk's continuation
                // keeps today's letter).
                let settled_site = self.open_focus_site(window);
                // Settle per the flow's kind rule, then re-run the
                // effect once against the settled state — bounded
                // one-level re-dispatch (the `InvokeSelected`
                // precedent, `router.rs`). The settle-nothing skip is
                // retired (r4 D4b): `resolve_conflict` early-returns
                // on `Flow::None`, so the gate was provably dead —
                // and it was the only shape that could silently eat a
                // re-dispatch if the kept-arm set ever shifted.
                // T-31 (#814): the palette's settle-then-run announces
                // the flow it displaced (the capture twin; only the
                // overlay-open class announces — the other
                // settle-then-run effects run in place).
                if matches!(effect, UiEffect::OpenPalette) {
                    self.arm_overlay_replacement_notice(window);
                }
                self.settle_flow(window, state);
                if self
                    .focus_origin
                    .as_ref()
                    .is_some_and(|origin| Some(origin.seam) == settled_site)
                {
                    self.focus_origin = None;
                }
                self.interpret(window, effect);
            }
        }
    }

    /// The overlay-over-live-flow announcement's arming (T-31, #814):
    /// the typed fact (a live flow was replaced) on the status-notice
    /// lane, captured before the overlay mounts.
    fn arm_overlay_replacement_notice(&mut self, window: &AppWindow) {
        self.notice = Some(Notice {
            text: crate::presentation::flow_replacement_notice_phrase(),
            until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
        });
        self.push_notice(window);
    }

    /// The matrix (audit §6) as a pure function: no silent default —
    /// each arm names the decision (the arms deliberately share
    /// bodies where the matrix cells agree).
    #[allow(clippy::match_same_arms)] // one arm per matrix decision
    fn resolve(kind: FlowKind, class: EffectClass) -> Resolve {
        use EffectClass as C;
        use FlowKind as K;
        use Resolve as R;
        match (kind, class) {
            // The one Replace: the capture overlay's fire-drill (D8).
            (_, C::BeginCapture) => R::Replace,
            // CAPTURE row: one overlay — a palette request is a
            // named inert.
            (K::Capture, C::OpenPalette) => R::Inert,
            // CANCEL: clean close (K) everywhere a kept arm doesn't
            // own it; under a text flow the row's own seams own the
            // buffer, so C-g is a named inert there.
            (K::Text, C::Cancel) => R::Inert,
            (_, C::Cancel) => R::CancelOnly,
            // BEGIN/TITLE: R4 — dead buttons cannot exist; the
            // auto-settle-then-run is the visible feedback (the
            // keymap-parity D-4 editor arms share the cell).
            (_, C::Begin | C::Title) => R::SettleThenRun,
            // NAV / LIFECYCLE / overlay-open (palette, the focus
            // overlay, the entries overlay): R3 settle-then-run.
            (
                _,
                C::Nav
                | C::Lifecycle
                | C::OpenPalette
                | C::FocusOverlay
                | C::TrackOverlay
                | C::LabelsOverlay,
            ) => R::SettleThenRun,
            // The plan-next-day family (#622): a held flow keeps
            // every planner intent inert (the modal discipline
            // verbatim — Escape out of the flow first).
            (_, C::PlanSurface) => R::Inert,
            // The review walk family (#638): the overlay-family shape
            // — a foreign flow settles, then the walk opens over it
            // (R3 settle-then-run); the walk's kept intents are the
            // pair overrides.
            (_, C::ReviewWalk) => R::SettleThenRun,
            // EDIT residue (the effects the kept arms don't own —
            // SetMode(Insert) under a text/due flow): named inert.
            (_, C::Edit) => R::Inert,
            // MENU/SYSTEM/PICKER: applied in `apply_effect` before
            // the table (audit §5; the picker keys are the same
            // family, round-3 D25); unreachable here — typed inert,
            // never silent.
            (_, C::Menu | C::System | C::Picker) => R::Inert,
        }
    }

    #[allow(clippy::too_many_lines)] // the effect application switch, kept flat
    fn apply_effect(&mut self, window: &AppWindow, effect: UiEffect) {
        // The total-dismiss seam (r4 chunk 4, D14): an anchor that
        // unmounts while hovered never fires `unhover` — the callback
        // dies with the element. So the dismiss stops being a per-arm
        // memory and becomes a property of the pass: snapshot the
        // view identity at entry; if the pass swapped the mounted
        // view, board, or subject, the tooltip is gone. No effect —
        // present or future — can be forgotten, because there is
        // nothing left to remember. Identity-preserving effects
        // (focus steps, scrolls, the walk) survive by construction;
        // the structural walk test pins the partition.
        let before = self.view_state.view_identity();
        let palette_before = self.shell.palette;
        // The focus walk's single handoff (US-4/W-4, D-H4): a walk
        // step that lands on a row carries its own focus target — the
        // identity-diff tail's view-scope re-anchor is suppressed for
        // it (the double-handoff that parks the key root over the
        // walked row is the #670 fix). `Some → Some` only: a retreat
        // to the switcher (Some → None) and every other identity
        // change keep the tail call.
        let focus_walk = matches!(
            effect,
            UiEffect::FocusNext
                | UiEffect::FocusPrev
                // The directional steps ride the walk's hint/hand-off
                // halves (J3 US-1): up/down land on rows like
                // FocusNext does.
                | UiEffect::FocusDirectional(_)
                | UiEffect::ExtendSelectionNext
                | UiEffect::ExtendSelectionPrev
                | UiEffect::FocusFirst
                | UiEffect::FocusLast
        );
        match effect {
            UiEffect::Exit => {
                self.quit_requests += 1;
                let _ = crate::quit();
            }
            UiEffect::UndoBranchesOpen => {
                // The kiosk structural exclusion (R-2): the entry
                // renders absent, the effect is inert (the
                // TrackEntriesOpen shape; the desktop-qualified E).
                if self.kiosk {
                    return;
                }
                // The picker mount (UF2 W-1): the modal-takeover
                // trio — settle any foreign flow, kill the tooltip,
                // reset the editing facts (the `OpenMenu` arm's
                // exact shape) — then mount. Pure navigation: no
                // command dispatch on open (the plan's W-1 row).
                self.settle_foreign_flow_for_modal(window);
                Self::dismiss_tooltip(window);
                Self::reset_editing_facts(window);
                self.shell.apply(&effect, self.menu_entries.len());
                self.sync_menu_models(window);
                self.sync_undo_picker_models(window);
            }
            UiEffect::OpenMenu => {
                // Round 3 (the matrix's OpenMenu cell, audit §5): a
                // foreign flow settles first — the modal must not
                // sit on an armed flow that keeps masking keys after
                // dismiss (F9's hamburger face).
                self.settle_foreign_flow_for_modal(window);
                // The menu is the modal context now (r2 D13): no tip
                // survives the open.
                Self::dismiss_tooltip(window);
                // The D20 reset seam 3 (modal opens): the reservation
                // and the mask must not outlive the surface that
                // armed them.
                Self::reset_editing_facts(window);
                // The projection is computed for the pre-menu chain
                // (the active context's actions, §5.9); the menu
                // context itself masks the chain once open (D11).
                let chain = self.current_chain(window);
                self.menu_anchor = MenuAnchor::Hamburger;
                self.menu_chain = chain;
                {
                    let state = self.state.load_full();
                    let flow = self.view_state.flow().clone();
                    let states = self.entry_states(&state, &flow);
                    self.root_entries = self.mint_root_entries(&states);
                }
                self.menu_entries = self.root_entries.clone();
                self.shell.apply(&effect, self.menu_entries.len());
                self.sync_menu_models(window);
            }
            UiEffect::InvokeSelected => {
                // Palette first (defensively — `Palette` masks `Global`,
                // so the menu cannot be open while the flow holds):
                // Activate is jump-to.
                if self.shell.palette {
                    self.apply_palette_invoke(window);
                } else if let Some(index) = self.shell.selected_entry() {
                    // Close first, then one re-dispatch — bounded
                    // (D16); the close itself lives in `invoke_entry`
                    // so the mouse path shares it (round-1 D3).
                    self.invoke_entry(window, index);
                } else {
                    self.apply_close_menu(window);
                }
            }
            UiEffect::CloseMenu => {
                if self.shell.palette {
                    let outcomes = self.view_state.close_palette();
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else {
                    self.apply_close_menu(window);
                }
                // The picker's dismissal shares the close (the AC-4
                // law; the shell cleared the bit).
                self.sync_undo_picker_models(window);
            }
            UiEffect::MenuBack => {
                if self.shell.undo_picker {
                    // AC-4 (UF2): the picker's Escape closes with
                    // nothing dispatched (it is a pure selection
                    // menu).
                    self.shell.undo_picker = false;
                    self.sync_undo_picker_models(window);
                } else if self.shell.notification_surface {
                    // The surface's Escape (US-6/W-6, D-N1): closes
                    // with nothing dispatched (rows keep their
                    // delivery state) and restores per the focus
                    // contract (the census row 12).
                    self.notification_cursor = None;
                    window.set_notification_selected(-1);
                    self.sync_view_context(window);
                    if !self.exit_focus_handoff(window) {
                        window.invoke_refocus_view();
                    }
                } else if self.shell.palette {
                    // Esc / dismiss / mouse-out all mean
                    // close-without-effect (the discard_edit shape).
                    let outcomes = self.view_state.close_palette();
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else {
                    // The Escape ladder (UX-R8 chunk-5 D-5, US-2/AC-4):
                    // a task-family submenu steps down first, then a
                    // main group's page, then the close — Escape 1
                    // closes the submenu, Escape 2 the menu.
                    let was_submenu = self.active_submenu().is_some();
                    self.shell.apply(&effect, self.root_entries.len());
                    if was_submenu {
                        // Restored to the root page: the active page is
                        // the root page again.
                        self.menu_entries = self.root_entries.clone();
                        self.sync_menu_models(window);
                        self.finish_mutation(window);
                    }
                }
            }
            UiEffect::NotificationVerb(verb) => {
                // The notification verbs (US-6/W-6, D-N1): each routes
                // exactly the seam body the row's button callback
                // runs — the click ≡ key law (us6_s2 pins the
                // structural equality). A verb stroke arms the walk
                // at row 0 (the AC-1 pre-selection).
                self.apply_notification_verb(window, verb);
            }
            UiEffect::MenuNext | UiEffect::MenuPrevious => {
                if self.shell.notification_surface {
                    // The notification walk (US-6/W-6, D-N1): the r5
                    // grammar — armed, the step clamps at both ends
                    // (no wrap).
                    let direction: i32 = if matches!(effect, UiEffect::MenuNext) {
                        1
                    } else {
                        -1
                    };
                    self.notification_walk(window, direction);
                } else if self.shell.palette {
                    let visible = self.tables.palette.len();
                    let direction = matches!(effect, UiEffect::MenuNext);
                    let outcomes = self
                        .view_state
                        .palette_cursor(i32::from(direction) * 2 - 1, visible);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if matches!(self.view_state.flow(), Flow::FocusMode { .. }) {
                    // The focus overlay's switcher (the FocusMode
                    // preset's Down/Up): the palette-cursor ring over
                    // the rendered rows.
                    let visible = slint::Model::row_count(&window.get_focus_switcher_model());
                    let direction = matches!(effect, UiEffect::MenuNext);
                    let outcomes = self
                        .view_state
                        .focus_mode_cursor(i32::from(direction) * 2 - 1, visible);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else {
                    self.shell.apply(&effect, self.menu_entries.len());
                }
            }
            UiEffect::OpenBoardBarFlyout => {
                // The target-free BoardSelect flow (UX R-4 D-4, #794):
                // arm the switcher flyout directly over the facts —
                // the window-level panel mounts from them (the
                // site-1 group), the walk rides the adapter's key arm,
                // and the anchor geometry is the bar's mirrored
                // mirror. Zero live boards: inert (no rows to walk).
                // Pre-table, view-state blind — the tooltip tail and
                // the settle matrix do not apply.
                let facts = window.global::<crate::BoardBarFacts>();
                let rows = i32::try_from(slint::Model::row_count(&window.get_board_bar_model().rows)).unwrap_or(i32::MAX);
                if rows > 0 {
                    let selected = window.get_board_bar_model().selected;
                    facts.set_highlight(selected.clamp(0, rows - 1));
                    facts.set_switcher_request(true);
                    facts.set_open(true);
                    let dd = window.global::<crate::DropdownFacts>();
                    dd.set_consumer(crate::DropdownConsumer::BoardSwitcher);
                    dd.set_anchor_x(facts.get_anchor_x());
                    dd.set_anchor_y(facts.get_anchor_y());
                    dd.set_anchor_width(facts.get_anchor_width());
                    dd.set_anchor_height(facts.get_anchor_height());
                    dd.set_open(true);
                }
                return;
            }
            UiEffect::PickerKey(key) => self.apply_picker_key(window, key),

            UiEffect::OpenPalette
            | UiEffect::SetMode(_)
            | UiEffect::FocusNext
            | UiEffect::FocusPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::ShowTask
            | UiEffect::ControlNext
            | UiEffect::ControlPrev
            | UiEffect::ScrollUp
            | UiEffect::ScrollDown
            | UiEffect::BeginCreate { .. }
            | UiEffect::BeginCreateStack
            | UiEffect::BeginTitleEdit
            // The detail editors' arm verbs (keymap-parity plan D-4)
            // interpret like every begin intent — the funnel refusals
            // live in the arms.
            | UiEffect::BeginDateEdit { .. }
            | UiEffect::BeginDescriptionEdit
            | UiEffect::BeginEffortEdit
            | UiEffect::Activate
            | UiEffect::RequestMove
            | UiEffect::StackWider
            | UiEffect::StackNarrower
            | UiEffect::OpenSettings
            // The status panel's toggle (OP3 W-1) interprets like the
            // settings opener — its kept arm owns the view swap.
            | UiEffect::OpenStatusPanel
            | UiEffect::BoardNext
            | UiEffect::BoardPrevious
            | UiEffect::BoardSelect(_)
            // Round 2 (D16–D21): the round-up verbs interpret like
            // every other view intent; their (flow, effect) arms land
            // with the view-state commits.
            | UiEffect::CancelMove
            | UiEffect::StackNext
            | UiEffect::StackPrev
            | UiEffect::StackDelete
            | UiEffect::StackRename
            | UiEffect::StackMove
            | UiEffect::BoardCreate
            | UiEffect::BoardDelete
            | UiEffect::BoardRename
            | UiEffect::BoardColorEdit
            | UiEffect::BoardDefaultPreset
            // The board ceremony verbs (OP1): interpret like every
            // view intent — the arm/confirm split and the kiosk
            // refusal live in the arms.
            | UiEffect::BoardArchive
            | UiEffect::BoardReset
            | UiEffect::BoardReconcile
            | UiEffect::FocusFirst
            | UiEffect::FocusLast
            // The directional-focus family (J3 US-1, round 9 chunk 9)
            // and its extend twin (#875) interpret like every view
            // intent — the per-view readings live in the table.
            | UiEffect::FocusDirectional(_)
            | UiEffect::ExtendSelectionNext
            | UiEffect::ExtendSelectionPrev
            // Chunk 10: the today/inbox jumps and capture route
            // through the same interpretation table. The focus
            // overlay's toggle joins them (#496) — the kiosk no-op
            // lives in its interpret arm.
            | UiEffect::FocusToday
            | UiEffect::OpenInbox
            | UiEffect::BeginCapture
            | UiEffect::ToggleFocusMode
            // The entries overlay's open request and its Escape
            // ladder interpret like every view intent (r4 chunk 10,
            // D-TS2) — the kiosk/subject refusals live in the arms;
            // the panel's focus hand-off is the composition's
            // changed-height grab (the walk probe's P5).
            | UiEffect::TrackEntriesOpen
            | UiEffect::TrackEntryDismiss
            | UiEffect::LabelsOpen
            | UiEffect::LabelsDismiss
            // The habit trio's intents interpret like every view
            // intent (issue #506) — the kiosk/subject refusals live
            // in the arms.
            | UiEffect::SeriesPauseOpen
            // The row-menu opener (US-7, W-8, #1032) interprets like
            // every view intent — the subjectless refusal lives in
            // the arm.
            | UiEffect::RowMenuRequested
            // The rule cursor's disarm chord (US-10, W-11, #1091)
            // interprets like every view intent — the typed inert
            // lives in the arm.
            | UiEffect::RowCursorDisarm
            // The roster remove's arm step (US-9, W-10) interprets
            // like every view intent — the cursorless refusal lives
            // in the arm.
            | UiEffect::BreakRuleRemoveArm
            | UiEffect::HabitPauseOpen
            | UiEffect::HabitConvertArm
            | UiEffect::HabitConvertCancel
            | UiEffect::HabitRemoveArm
            | UiEffect::HabitRemoveCancel
            | UiEffect::HabitRestoreArm
            | UiEffect::HabitRestoreCancel
            | UiEffect::HabitCreateToggle
            | UiEffect::SeriesConvertArm
            // The H4 editor opener interprets like every view intent
            // (the kiosk/subject refusals live in the arm).
            | UiEffect::HabitEdit
            | UiEffect::HabitQuotaEdit
            // The HF3 history opener interprets like every view
            // intent (the kiosk/subject refusals live in the arm).
            | UiEffect::HabitHistoryOpen
            // The recur editor's intents (round-7 D-7.1): the opener
            // and the detach strip's flag flips interpret like every
            // view intent — the kiosk/subject refusals live in the
            // arms.
            | UiEffect::SeriesEditorOpen
            | UiEffect::SeriesDetachArm
            | UiEffect::SeriesDetachCancel
            // The priority pickers' openers (UX R-3 D-1, #788) — the
            // kiosk/no-subject refusals live in the arms.
            | UiEffect::PriorityPickerOpen(_)
            // The plan-next-day family (#622): the surface's intents
            // interpret like every view intent — the kiosk/modal
            // refusals live in the arms.
            | UiEffect::OpenPlanNextDay
            | UiEffect::PlanBeginPlace(_)
            | UiEffect::PlanCommitPlace
            | UiEffect::PlanBoxEarlier(_)
            | UiEffect::PlanBoxLater(_)
            | UiEffect::PlanRemoveBox(_)
            // The review walk family (#638): the opener and the
            // carrier intents interpret like every view intent — the
            // kiosk refusal and the walk arms live in the table.
            | UiEffect::OpenReviewWalk
            | UiEffect::ReviewWalk(_)
            // The export dialog's pair (the J2 export plan W-1): the
            // interpretation table owns the arms — the mount bit is
            // the adapter's VM.
            | UiEffect::ExportDialogOpen
            | UiEffect::ExportDialogClose => self.interpret(window, effect),
        }
        // The pass's tail: the identity diff decides (see the entry
        // comment). The explicit dismisses below (the menu's
        // modal-takeover, the ghost/modal takeovers) are idempotent
        // under this — an explicit call names a non-identity reason
        // (a hovered element that unmounts, or a modal that takes
        // over, without the mounted view moving).
        if self.view_state.view_identity() != before {
            Self::dismiss_tooltip(window);
            // The D20 reset seam 2 (view swaps): the same unmount
            // hazard as the flow end, one enumerate-once tail.
            Self::reset_editing_facts(window);
            // The swap unmounts whatever held slint focus (and any
            // stale focus must not survive into the new surface) — the
            // D15 hand-off rides the swap as it rode every dispatch
            // before the r4 chunk-6 gating. The focus-origin restore
            // runs first (US-1/W-1, D-F1's ordering law): the
            // view-scope call only when nothing element-level resolved.
            // The single handoff (US-4/W-4, D-H4): a walk step that
            // landed on a row carries its own focus target — the
            // view-scope re-anchor is suppressed for it (the
            // double-handoff that parks the key root over the walked
            // row is the #670 fix). A retreat to the switcher
            // (Some → None) and every other identity change keep it.
            let walk_carried = focus_walk && self.view_state.focused_task().is_some();
            if !self.exit_focus_handoff(window) && !walk_carried {
                window.invoke_refocus_view();
            }
        }
        if self.shell.palette && !palette_before {
            // The D20 reset seam 3's palette leg: the modal surface
            // refocuses its own field (which re-publishes); the armed
            // facts must not outlive what armed them.
            Self::reset_editing_facts(window);
        }
    }

    /// The full close shared by every path that means "close the
    /// menu": the shell state and both page models clear (round-1 D6).
    /// The palette's Activate (keyboard `menu.invoke` or a row
    /// click): resolve `tables.palette[selected]` against the fresh
    /// snapshot and jump; a stale index is inert (close only).
    fn apply_palette_invoke(&mut self, window: &AppWindow) {
        let selected = match self.view_state.flow() {
            crate::view_state::Flow::Palette { selected } => *selected,
            _ => 0,
        };
        let before = self.view_state.focused_task();
        let target = self.tables.palette.get(selected).copied();
        let state = self.state.load();
        let outcomes = match target {
            Some(target) => self.view_state.palette_jump(&state, target),
            // A row count that shrank past the cursor: close only.
            None => self.view_state.close_palette(),
        };
        self.apply_outcomes(window, &outcomes);
        // Round-3 D23 §3.3: the invoke's retarget arms the scroll
        // hint the same way the walk effects do (a stack-target jump
        // emits its StackDetail hint here; an inert close re-arms
        // nothing — the unchanged-focus guard holds).
        self.push_focus_scroll_hint(window, before);
        self.finish_mutation(window);
    }

    /// The per-keystroke palette pass (design D10): mirror the buffer,
    /// reset the cursor (every keystroke re-ranks — a stale mid-list
    /// cursor on a fresh ranking is a lie), then the one targeted
    /// [`crate::models::push_palette`] call — no full refresh, no
    /// debounce.
    fn apply_palette_query(&mut self, window: &AppWindow, text: &str) {
        self.view_state.set_buffer(text.to_owned());
        if let Some(crate::view_state::Flow::Palette { selected }) = self.view_state.flow_mut() {
            *selected = 0;
        }
        let state = self.state.load();
        self.tables.palette =
            crate::models::push_palette(window, &state, &self.view_state, &mut self.models);
        self.finish_mutation(window);
    }

    /// The disarm reconcile's predicate (J5 US-4 AC-2, #927): the
    /// pinned single subject no longer resolves (a soft-deleted task
    /// stays in the map — liveness is the `deleted` bit, the same
    /// reading the id tables derive), or a snapshotted selection lost
    /// every member. Partial losses keep the menu (its rows
    /// dispatch-time-gate per task, the stale-absorb law).
    fn menu_subject_dead(&self, state: &AppState) -> bool {
        let live = |id: &TaskId| state.tasks.get(id).is_some_and(|task| !task.deleted);
        match &self.menu_subject {
            MenuSubject::None => false,
            MenuSubject::Single(id) => !live(id),
            MenuSubject::Selection(members) => members.iter().all(|member| !live(member)),
        }
    }

    fn apply_close_menu(&mut self, window: &AppWindow) {
        // Every close (the effect, the dismissal layer, the toggle,
        // and both mouse leaf paths sharing it) dismisses the tooltip
        // (r2 D13).
        Self::dismiss_tooltip(window);
        self.shell.apply(&UiEffect::CloseMenu, 0);
        self.menu_entries.clear();
        self.root_entries.clear();
        self.menu_chain.clear();
        self.menu_anchor = MenuAnchor::Hamburger;
        self.menu_kind = MenuKind::Main;
        self.menu_subject = MenuSubject::None;
        self.quick_bar_entries.clear();
        self.sync_menu_models(window);
        // The direct callers (toggle, dismissal layer, mouse invoke)
        // bypass `route`'s trailing sync; the shared helper keeps the
        // globals identical to it (r2 D12).
        self.finish_mutation(window);
    }

    /// Activating a parent row (round-1 D6, keyboard `InvokeSelected`
    /// or a root-panel click): park the root cursor, open the group's
    /// page as the active one. A parent row never dispatches. UX-R8
    /// chunk-5 D-5: a task-family parent mounts the family's curated
    /// child page (`shell.menu.task_group` parks beside
    /// `active_group`; the Escape ladder consults it first).
    #[allow(clippy::too_many_lines)] // the two submenu kinds, kept flat
    fn activate_submenu(&mut self, window: &AppWindow, submenu: crate::menu::Submenu) {
        self.shell.menu.root_selected = self.shell.menu.selected;
        self.shell.menu.selected = 0;
        match submenu {
            crate::menu::Submenu::Group(group) => {
                self.shell.menu.active_group = Some(group);
                self.shell.menu.task_group = None;
                {
                    let state = self.state.load_full();
                    let flow = self.view_state.flow().clone();
                    let states = self.entry_states(&state, &flow);
                    self.menu_entries =
                        menu::group_page(&self.keymap, &self.menu_chain, &states, group);
                }
            }
            crate::menu::Submenu::Task(family) => {
                self.shell.menu.task_group = Some(family);
                self.shell.menu.active_group = None;
                if matches!(
                    self.menu_row_subject,
                    Some(crate::presentation::RowSubject::Habit(_))
                ) {
                    self.menu_entries = self.habit_submenu_page(family);
                } else {
                    let subject = self.current_task_subject();
                    {
                        let state = self.state.load_full();
                        let flow = self.view_state.flow().clone();
                        let states = self.entry_states(&state, &flow);
                        self.menu_entries = menu::task_submenu_page(
                            &self.keymap,
                            &self.menu_chain,
                            subject,
                            &states,
                            family,
                        );
                    }
                }
            }
        }
        self.shell.menu.len = self.menu_entries.len();
        self.sync_menu_models(window);
        // The F7 fix (r2 D12): the mouse parent path pushes the same
        // globals the keyboard path pushes — `menu-active-group` and
        // friends — so the fly-out/replace page actually mounts.
        self.finish_mutation(window);
    }

    /// The ACTIVE page's invocation (keyboard `menu.invoke`, and the
    /// rendered page's mouse rows): a parent row opens its group's
    /// page, a leaf closes the menu and re-dispatches through the same
    /// router (round-1 D3: close first, route second).
    fn invoke_entry(&mut self, window: &AppWindow, index: usize) {
        let page = self.menu_entries.clone();
        let Some(entry) = page.get(index) else {
            return;
        };
        if let Some(sub) = entry.submenu {
            self.activate_submenu(window, sub);
            return;
        }
        let Some(action) = entry.action else {
            return;
        };
        // T-19 (#802): the doctrine's letter — every click reacts. A
        // disabled row's invocation sets the status-line notice built
        // from the row's own registry description (the why-text, the
        // modality-neutral channel) instead of absorbing into the
        // router's Disabled arm.
        if !entry.enabled {
            self.arm_disabled_menu_notice(entry.description);
            self.push_notice(window);
            return;
        }
        self.apply_close_menu(window);
        self.route(window, action);
    }

    /// The dispatch-outcome notice's arming (T-41/T-43, #824/#826):
    /// the habit-log and created-timer commands echo their typed facts
    /// onto the status-notice lane (the +1's surplus delta; the
    /// created occurrence's target). `true` when a notice armed.
    fn arm_outcome_notice(&mut self, command: &taskboard_domain::StateCommand) -> bool {
        match command {
            taskboard_domain::StateCommand::LogHabit { .. } => {
                self.notice = Some(Notice {
                    text: crate::presentation::habit_log_notice_phrase(),
                    until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                });
                true
            }
            // HF1 (US-1/S2): the skip's acknowledgement rides the same
            // lane — the chip renders the standing state (US-1/S3),
            // the notice is the dispatch-time event stamp.
            taskboard_domain::StateCommand::SkipHabit { .. } => {
                self.notice = Some(Notice {
                    text: crate::presentation::habit_skip_notice_phrase(),
                    until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                });
                true
            }
            taskboard_domain::StateCommand::CreateAdhocAlarm { label, when, .. } => {
                self.notice = Some(Notice {
                    text: crate::presentation::adhoc_alarm_notice_phrase(label, when),
                    until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                });
                true
            }
            _ => false,
        }
    }

    /// The disabled-row notice's arming (T-19, #802): the typed fact
    /// (entry disabled) plus the registry description, captured at
    /// event time (the resurrect notice's event-stamp shape).
    fn arm_disabled_menu_notice(&mut self, description: &str) {
        self.notice = Some(Notice {
            text: crate::presentation::disabled_menu_notice_phrase(description),
            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
        });
    }

    /// The menu-invoked arm's typed refusal (cross-surface friction
    /// plan D-3, #832): the conversion arms' T-19 twin — the
    /// subjectless/no-target dispatch surfaces on the status-notice
    /// lane from the action's registry description. Structural fact
    /// only (`notice.is_some()`); the phrase is never asserted.
    /// The habit editor's one-shot subject (H4 D1): the menu row
    /// routes the bare id — the opener's focus fact mints the target
    /// (the habit menu's `focused_habit`; the repair seam's exact
    /// arithmetic). The convert arm's resolution order, habit-side.
    /// The habit history dialog's ONE opener (HF3 W-8, US-7/S1):
    /// the action arm and the editor's History row converge here —
    /// the subject resolution, the kiosk/subject refusals, and the
    /// view-state write happen in exactly one place (the arm
    /// convention). The editor variant's subject rides the
    /// view-state's habit subject when no row focus carries one.
    fn open_habit_history_from_subject(&mut self, window: &AppWindow) {
        let habit = self
            .habit_edit_subject()
            .or_else(|| self.view_state.subject_habit());
        if let Some(habit) = habit {
            let now = self.day_ctx().now;
            let day = taskboard_domain::DayCalendar {
                day_start_time: self.day_start.0,
                offset: self.day_start.1,
            };
            let today = day.today(now);
            let outcomes = self.view_state.open_habit_history(habit, today);
            self.apply_outcomes(window, &outcomes);
            self.finish_mutation(window);
        } else {
            self.arm_no_target_notice(ActionId::HabitHistory);
            self.push_notice(window);
        }
    }

    fn habit_edit_subject(&self) -> Option<taskboard_domain::HabitId> {
        // D3a (W-16, #1202): the walk cursor joins the subject chain —
        // the Return row's habit landing IS the subject (the
        // chord verbs' `effective_habit` resolution; the walk cursor
        // is the fallback, the row seams' mints stay first so the
        // menu path cannot drift).
        let habit = self
            .focused_habit
            .or(match self.menu_row_subject {
                Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
                _ => None,
            })
            .or_else(|| match self.view_state.walk_subject() {
                Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
                _ => None,
            });
        habit.filter(|habit| {
            self.state
                .load()
                .habits
                .habits
                .get(habit)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        })
    }

    fn arm_no_target_notice(&mut self, id: ActionId) {
        self.notice = Some(Notice {
            text: crate::presentation::no_target_notice_phrase(
                crate::actions::entry(id).description,
            ),
            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
        });
    }

    /// The habit conversion strip's ONE arming site (cross-surface
    /// friction plan D-2, #832, AC-1's identicality guarantee): the
    /// row seam and the menu-routed effect arm converge here — the
    /// strip flags and the window echo are written in exactly one
    /// place. `false` on a dead habit or an unresolvable row index
    /// (the caller surfaces the typed refusal; the pointer seam stays
    /// silent — its stale-index posture is unchanged).
    fn arm_habit_convert(&mut self, window: &AppWindow, habit: taskboard_domain::HabitId) -> bool {
        let target = {
            let state = self.state.load();
            let Some(row) = state.habits.habits.get(&habit) else {
                return false;
            };
            if !row.is_live() {
                return false;
            }
            row.target
        };
        self.focused_habit = Some(habit);
        self.habit_convert_armed = Some(habit);
        self.habit_convert_target = Some(habit);
        self.pattern_draft = Some(crate::presentation::convert_pattern_default(target));
        // The armed confirm lives INSIDE the open habit menu (O-7 Q3):
        // the one sanctioned rebuild of the built-once menu (the D-1
        // pin) — same anchor, same kind, the confirm flips to enabled.
        self.reproject_habit_menu(window);
        true
    }

    /// The habit menu's ONE re-projection (H2 D4): rebuilds the page
    /// over the open menu's subject and anchor — the arm flip
    /// (confirm greys → enables) and the cancel step's disarm both
    /// land here. No-op unless the open menu IS the habit menu of the
    /// still-armed subject.
    fn reproject_habit_menu(&mut self, window: &AppWindow) {
        let Some(crate::presentation::RowSubject::Habit(subject)) = self.menu_row_subject else {
            return;
        };
        if !self.shell.menu.open {
            return;
        }
        let entries = self.habit_menu_entries(subject);
        self.root_entries = entries;
        // The ACTIVE page rebuilds (the arm routes from the open
        // submenu — the flip must land in the page the user sees,
        // never yank the menu back to its root).
        self.menu_entries = match self.shell.menu.task_group {
            Some(family) => self.habit_submenu_page(family),
            None => self.root_entries.clone(),
        };
        self.shell.menu.len = self.menu_entries.len();
        self.shell.menu.selected = self.shell.menu.selected.min(self.menu_entries.len());
        self.sync_menu_models(window);
    }

    /// The habit menu's per-verb verdicts (H2 D4/D6/A3): the armed
    /// trio flips with the armed subject, the repair row greys
    /// without a make-up window (applicable, precondition-blocked —
    /// never withheld), and on kiosk EVERY habit verb greys from the
    /// states map (reading a menu is passive; the arm additionally
    /// refuses at dispatch).
    fn habit_menu_states(
        &self,
        state: &taskboard_domain::AppState,
        habit: taskboard_domain::HabitId,
    ) -> std::collections::BTreeMap<ActionId, bool> {
        let mut states = self.entry_states(state, &self.view_state.flow().clone());
        let armed = self.habit_convert_armed == Some(habit);
        states.insert(ActionId::HabitConvertArm, !armed);
        states.insert(ActionId::HabitConvertToSeries, armed);
        states.insert(ActionId::HabitConvertCancel, armed);
        // The staged remove's verdicts (H4 D4): the arm greys while
        // armed (the confirm replaces its enabled state), the confirm
        // flips on with the arm, the cancel joins armed-only.
        let remove_armed = self.habit_remove_armed == Some(habit);
        states.insert(ActionId::HabitRemove, !remove_armed);
        states.insert(ActionId::HabitRemoveConfirm, remove_armed);
        states.insert(ActionId::HabitRemoveCancel, remove_armed);
        // The undo-family restore's verdicts (W-3): the remove trio's
        // exact grammar over the roster-addressed subject.
        let restore_armed = self.habit_restore_armed.is_some();
        states.insert(ActionId::HabitRestore, !restore_armed);
        states.insert(ActionId::HabitRestoreConfirm, restore_armed);
        states.insert(ActionId::HabitRestoreCancel, restore_armed);
        let repair = state
            .habits
            .habits
            .get(&habit)
            .is_some_and(taskboard_domain::habit::Habit::is_live)
            && state.habits.habits.get(&habit).is_some_and(|row| {
                crate::presentation::habit_repair_target(
                    row,
                    &state.habits.log,
                    self.day_ctx().now,
                    self.calendar(),
                )
                .is_some()
            });
        states.insert(ActionId::HabitRepair, repair);
        if self.kiosk {
            for id in [
                ActionId::HabitLog,
                // The HF1 trio greys with the rest (R-3: visible-
                // disabled, never withheld).
                ActionId::HabitSkip,
                ActionId::HabitPause,
                ActionId::HabitResume,
                ActionId::HabitUndoLog,
                ActionId::HabitRepair,
                ActionId::HabitMoveUp,
                ActionId::HabitMoveDown,
                ActionId::HabitConvertArm,
                ActionId::HabitConvertToSeries,
                ActionId::HabitConvertCancel,
                ActionId::HabitEdit,
                // The HF3 history opener greys too (the visible-
                // disabled posture: the dialog is desktop-only, R-3 —
                // the row reads "exists, not for this device").
                ActionId::HabitHistory,
                ActionId::HabitRemove,
                ActionId::HabitRemoveConfirm,
            ] {
                states.insert(id, false);
            }
        }
        states
    }

    /// The habit menu's page over a subject (H2 D4): the curated core
    /// plus the verdicts; also mints both reorder one-shots from the
    /// row source (the row seam's exact arithmetic).
    fn habit_menu_entries(
        &mut self,
        habit: taskboard_domain::HabitId,
    ) -> Vec<crate::menu::MenuEntry> {
        let state = self.state.load_full();
        let chain = [ContextId::Board, ContextId::Global].to_vec();
        // The reorder pair's slots (H2 D4): the shared mint (chunk 8
        // D4 — one arithmetic, `habit_move_slots` over the section's
        // shared live order: menu open, keyboard walk, drag gap).
        let (up, down) = Self::habit_move_slots(&state, habit);
        self.habit_move_up = up;
        self.habit_move_down = down;
        let states = self.habit_menu_states(&state, habit);
        let subject = crate::presentation::task_menu_subject(
            &state,
            Some(crate::presentation::RowSubject::Habit(habit)),
        );
        // The restore row source (W-3, D-4): the roster's newest
        // tombstone gates the row; kiosk passes none (the affordance
        // renders absent — the CLI carries the kiosk restore path).
        let restore = if self.kiosk {
            None
        } else {
            state
                .roster
                .tombstoned_habits
                .first()
                .map(|row| row.name.as_str())
        };
        let page = crate::menu::habit_context_page(&self.keymap, &chain, subject, &states, restore);
        drop(state);
        page
    }

    /// The reorder pair's shared mint (chunk 8 D4, #934 — the
    /// arithmetic delegated to its one home,
    /// `presentation::habit_move_slots`): target gap indices over the
    /// section's live board-scoped row source (#1198 chunk A1 D1 —
    /// the engine resolves the orders). Two invocation paths converge
    /// here: the menu open and the keyboard walk's pre-mint. `None`
    /// for a stale or positionless habit (the consumers take the
    /// typed `NoTarget`).
    fn habit_move_slots(
        state: &taskboard_domain::AppState,
        habit: taskboard_domain::HabitId,
    ) -> HabitMoveSlots {
        crate::presentation::habit_move_slots(state, habit)
    }

    /// The habit verbs' effective subject (chunk 8 D4): the walk
    /// cursor's habit landing when the walk has entered — a task
    /// landing resolves to `None` (a stale click fact can never
    /// answer a task walk's stroke) — the pointer click fact
    /// otherwise.
    fn effective_habit(&self) -> Option<taskboard_domain::HabitId> {
        match self.view_state.walk_subject() {
            Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
            Some(crate::presentation::RowSubject::Task(_)) => None,
            None => self.focused_habit,
        }
    }

    /// The open habit menu's submenu page (H2 D4): the Convert
    /// family's children over the habit verdicts (the confirm flips
    /// with the armed subject, the cancel row joins while armed).
    /// Empty for any other family (a habit subject has no Reminders/
    /// Timers/Entries page).
    fn habit_submenu_page(
        &self,
        family: crate::menu::TaskMenuGroup,
    ) -> Vec<crate::menu::MenuEntry> {
        let Some(crate::presentation::RowSubject::Habit(habit)) = self.menu_row_subject else {
            return Vec::new();
        };
        let state = self.state.load_full();
        let states = self.habit_menu_states(&state, habit);
        let subject = crate::presentation::task_menu_subject(
            &state,
            Some(crate::presentation::RowSubject::Habit(habit)),
        );
        crate::menu::habit_convert_children(
            &self.keymap,
            &self.menu_chain,
            subject,
            &states,
            family,
        )
    }

    /// The habit row's context menu opener (H2 D4/B1): focus the
    /// habit (the row click's focus-fact discipline), then open the
    /// curated page anchored at the pointer. Stale indices absorb
    /// silently (the notification-row guard shape).
    /// The row-menu key's adapter half (US-7, W-8, #1032): the walk
    /// subject's context menu through the pointer seams' own openers
    /// (click ≡ key by construction). The keyboard anchor is the
    /// toolbar fallback — the `Hamburger` anchor flipped after the
    /// opener (the openers hard-code the pointer point; the keyboard
    /// has none) — re-synced so the projection reads the anchor the
    /// same pass. A subjectless walk (and a stale habit landing) is
    /// the typed refusal.
    fn open_row_menu_keyboard(&mut self, window: &AppWindow) {
        let today = self.view_state.view() == crate::view_state::ViewId::Today;
        match self.view_state.walk_subject() {
            Some(crate::presentation::RowSubject::Habit(habit)) => {
                let index = self.tables.habits.iter().position(|&row| row == habit);
                if let Some(index) = index {
                    self.open_habit_context_menu(window, 0.0, 0.0, index);
                    self.menu_anchor = MenuAnchor::Hamburger;
                    self.sync(window);
                } else {
                    self.arm_no_target_notice(ActionId::RowMenu);
                    self.push_notice(window);
                }
            }
            Some(crate::presentation::RowSubject::Task(task)) => {
                // The today surface carries the due-today extra (the
                // today context seam's curated row); the board's is
                // the bare page (the same split the pointer seams
                // speak).
                let extra: &[ActionId] = if today {
                    &[ActionId::TaskDueToday]
                } else {
                    &[]
                };
                self.open_task_context_menu(
                    window,
                    0.0,
                    0.0,
                    extra,
                    Some(crate::presentation::RowSubject::Task(task)),
                );
                self.menu_anchor = MenuAnchor::Hamburger;
                self.sync(window);
            }
            None => {
                self.arm_no_target_notice(ActionId::RowMenu);
                self.push_notice(window);
            }
        }
    }

    fn open_habit_context_menu(&mut self, window: &AppWindow, x: f32, y: f32, index: usize) {
        settle_pointer_conflict(self, window);
        let Some(&habit) = self.tables.habits.get(index) else {
            return;
        };
        let Some(row) = self.state.load().habits.habits.get(&habit).cloned() else {
            return;
        };
        if !row.is_live() {
            return;
        }
        // Opening another subject's menu disarms (H2 D4: one subject
        // armed at a time — the remove arm obeys the same law).
        if self.habit_convert_armed.is_some_and(|armed| armed != habit) {
            self.habit_convert_armed = None;
            self.habit_convert_target = None;
            self.pattern_draft = None;
        }
        if self.habit_remove_armed.is_some_and(|armed| armed != habit) {
            self.habit_remove_armed = None;
        }
        self.focused_habit = Some(habit);
        self.menu_row_subject = Some(crate::presentation::RowSubject::Habit(habit));
        let entries = self.habit_menu_entries(habit);
        self.open_context_menu_with(window, x, y, entries, Vec::new());
    }

    /// The series conversion strip's ONE arming site (the habit
    /// twin's pair): the recur row's seam and the menu-routed effect
    /// arm converge here. `false` without a live recur rule (the
    /// caller surfaces the typed refusal).
    fn arm_series_convert(&mut self, task: TaskId) -> bool {
        let pattern = {
            let state = self.state.load();
            let Some(rule) = state.tasks.get(&task).and_then(|t| t.recur.as_ref()) else {
                return false;
            };
            crate::presentation::convert_target_default(&rule.pattern)
        };
        self.series_target = Some(task);
        self.convert_target = Some(pattern);
        self.recur_confirm_armed = true;
        true
    }

    /// A root-panel row's invocation (round-1 D6): identical to
    /// [`Self::invoke_entry`] at the root level; inside a submenu the
    /// root panel stays mounted (fly-out), so its rows resolve against
    /// the root page — a parent switches the open group, a quick row
    /// like Quit dispatches.
    fn invoke_root_entry(&mut self, window: &AppWindow, index: usize) {
        let submenu = self.root_entries.get(index).and_then(|entry| entry.submenu);
        if let Some(sub) = submenu {
            self.shell.menu.selected = index;
            self.activate_submenu(window, sub);
            return;
        }
        // The disabled guard rides the root twin too (the keyboard
        // `menu.invoke` and the fly-out rows both land here).
        let disabled = self.root_entries.get(index).is_some_and(|entry| {
            entry.submenu.is_none() && entry.action.is_some() && !entry.enabled
        });
        if disabled {
            self.arm_disabled_menu_notice(self.root_entries[index].description);
            self.push_notice(window);
            return;
        }
        let action = self.root_entries.get(index).and_then(|entry| entry.action);
        let Some(action) = action else {
            return;
        };
        self.apply_close_menu(window);
        self.route(window, action);
    }
}

/// One in-flight settings operation: the reply half parked until the
/// drain polls it (the repaint timer's job, D25b — never a blocking
/// recv on the UI thread).
#[derive(Debug)]
enum PendingReply {
    /// `ListRemoteBoards`' rows, keyed by the listed connector (round-5
    /// chunk 8): the reply refreshes that connector's mapping rows.
    Remotes(
        String,
        oneshot::Receiver<Result<Vec<requests::RemoteBoardRow>, SettingsError>>,
    ),
    /// The list-carrying replies (`SaveConnection`/`RemoveConnection`/
    /// `AttachConnector`/`CreateLocalBoard`/`DetachConnector`) — the
    /// full pair (r2 D32).
    List(oneshot::Receiver<Result<requests::ConnectionsSnapshot, SettingsError>>),
    /// `SetKeymapPreset`'s rebuilt keymap.
    Keymap(oneshot::Receiver<Result<KeymapUpdate, SettingsError>>),
    /// `SetScale`, with the scale to revert to on an `Err` reply.
    Scale(oneshot::Receiver<Result<(), SettingsError>>, f64),
    /// One of the settings editor's general commits: the reply
    /// carries the fresh file-layer [`SettingsSnapshot`].
    Snapshot(oneshot::Receiver<Result<crate::requests::SettingsSnapshot, SettingsError>>),
}

/// Which operation is in flight (the form's busy marker; one at a
/// time — the buttons disable while pending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingKind {
    Save,
    Remove,
    /// `ListRemoteBoards` (round-5 chunk 8): the Deck row's marker.
    ListRemote,
    /// `AttachConnector` (r2 D33, explicit since round-5 chunk 8): the
    /// remote row's marker.
    Attach,
    /// `CreateLocalBoard` (round-5 chunk 8): the remote row's marker.
    CreateBind,
    /// `DetachConnector` (r2 D33): the remote row's marker.
    Detach,
    /// `PublishBoard` (round-6 D-6.9, #699): the Deck row's marker.
    Publish,
    /// `MoveBoardToFiles` (round-6 D-6.8 phase 2, #722): the Files
    /// row's marker.
    Move,
    /// `StopBoardFilesSync` (round-7 D-7.3, D9): the roster row's
    /// marker (`busy_board` names the row). Attribution only.
    StopFilesSync,
    /// `PublishBoardToFiles` (round-7 D-7.3, D9): the roster row's
    /// marker (`busy_board` names the row). Attribution only.
    PublishFiles,
    Preset,
    Scale,
    /// One of the settings editor's commits (the GUI settings editor
    /// chunk), named by the lane field it edits — text commits and
    /// choice picks alike (the kind is attribution only; no busy
    /// marker matches it).
    General(crate::requests::Field),
}

/// One rendered connections item's address (round-5 chunk 8): what a
/// callback's item index resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ItemAddr {
    /// The card header; Remove addresses the card's first connector
    /// (the app removes the whole identity).
    Account {
        connector: String,
    },
    /// The Files toggle row's address (round-6 D-6.6): the card's
    /// first Nextcloud connector — the re-anchor target when the
    /// toggle picks On on a non-anchor card.
    Files {
        connector: String,
    },
    /// The files roster's per-row action seam (round-7 D-7.3, W-3/D1
    /// — round-6's title-only visibility row grew the roster grammar,
    /// overturning round-6 D-6's "disconnect is a non-goal" ruling;
    /// authority: the round-7 plan §2 D-7.3). `connector` is the
    /// card's files connector — the busy marker's target and the
    /// lane-on fact's re-derivation source at the trust boundary.
    FilesBoard {
        board: BoardId,
        connector: String,
    },
    Deck {
        connector: String,
    },
    Remote {
        connector: String,
        remote: RemoteBoardId,
        bound: Option<BoardId>,
    },
}

/// The typed last-outcome marker the model renders structurally
/// (R5: never message text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsOutcome {
    #[default]
    None,
    Ok,
    Busy,
    /// The operation was refused (validation, verify rejected,
    /// persist rejected).
    Rejected,
    /// The server/store was unreachable.
    Unavailable,
    /// Everything else (store/attach failures).
    Failed,
}

/// The settings view-model: the adapter-side state the settings view
/// renders from. The draft is the `settings_form` mirror (the buffer
/// precedent); the password clears on send — write-only by
/// construction (D26b).
/// The auto-list backoff's typed state (US-3, D-4): consecutive
/// failures and the last failure's instant. The window doubles per
/// consecutive failure (`min(30s * 2^(n-1), 10min)`); the clock is
/// tokio's (pause/advance-testable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutoListBackoff {
    consecutive_failures: u32,
    last_failure: tokio::time::Instant,
}

impl AutoListBackoff {
    /// Records one more failure (the re-arm, AC-2).
    fn record_failure(&mut self, now: tokio::time::Instant) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_failure = now;
    }

    /// The suppression window: `min(30s * 2^(n-1), 10min)` — the
    /// shift caps at 5 so the doubling saturates at the 10-minute
    /// ceiling instead of overflowing.
    fn window(&self) -> std::time::Duration {
        let doubling = 1u32 << self.consecutive_failures.saturating_sub(1).min(5);
        std::time::Duration::from_secs(30)
            .saturating_mul(doubling)
            .min(std::time::Duration::from_secs(600))
    }

    /// Whether the mount auto-list stays suppressed right now.
    fn active(&self, now: tokio::time::Instant) -> bool {
        now.saturating_duration_since(self.last_failure) < self.window()
    }
}

/// The suppression predicate over the backoff map (the mount
/// auto-list's one gate; the seam the request-count tests resolve).
fn auto_list_suppressed(
    backoff: &BTreeMap<String, AutoListBackoff>,
    connector: &str,
    now: tokio::time::Instant,
) -> bool {
    backoff
        .get(connector)
        .is_some_and(|entry| entry.active(now))
}

/// The status panel's VM (OP3 W-1..W-3, journey plan
/// `2026-11-21-parity-restore-journeys-status-surface.md`): the single
/// in-flight `StatusQuery` reply half. The folded data lives slint-side
/// (the `StatusPanelFacts` global's typed properties — the panel
/// renders from its facts, the adapter only dispatches and folds).
#[derive(Default)]
struct StatusPanelVm {
    pending: Option<oneshot::Receiver<crate::requests::StatusSnapshot>>,
    /// The armed remediation verb (the D2 command plan W-4): pure
    /// presentation state — nothing dispatches until the strip's
    /// Confirm; Cancel disarms. One armed subject (the shared
    /// grammar: arming a different verb re-arms).
    armed: Option<crate::requests::StatusRemediationAction>,
    /// The in-flight remediation's reply half (W-6): `Some` between
    /// the Confirm dispatch and the settled reply.
    remediation_pending:
        Option<oneshot::Receiver<crate::requests::StatusRemediationOutcome>>,
}

#[derive(Debug, Default)]
struct SettingsVm {
    draft: ConnectionDraft,
    rows: Vec<ConnectorInfo>,
    /// Connector name → down-override (the `ConnectorLost`/
    /// `ConnectorRestored` episodes, D8); absent = no override.
    liveness: BTreeMap<String, bool>,
    /// The last listing per connector (round-5 chunk 8): the remote
    /// boards each Deck row maps; bindings render from the live state.
    remotes: BTreeMap<String, Vec<requests::RemoteBoardRow>>,
    /// The mount auto-lists' in-flight replies (round-6 D-6.7),
    /// connector → parked reply half. Deliberately NOT the shared
    /// `replies`/`pending` machine: that drain's settle block clears
    /// `pending`/`busy_target` and stamps a global outcome for every
    /// settled reply — a background listing landing mid-Save would
    /// wipe the user's op's busy marker and banner. Here a settled
    /// reply only fills `remotes` (silent by construction); the slot
    /// occupies the map until the reply lands, which is the rapid-
    /// remount double-fire guard. One listing request per connector
    /// until its reply lands.
    auto_listings:
        BTreeMap<String, oneshot::Receiver<Result<Vec<requests::RemoteBoardRow>, SettingsError>>>,
    /// The files watch's per-board states (US-8, D-8): updated by
    /// `drain_files_sync`, consumed by the Files roster's row fold.
    files_board_states:
        std::collections::BTreeMap<uuid::Uuid, crate::presentation::FilesBoardState>,
    /// The mount auto-list's typed backoff (the files-sync-status UX
    /// plan US-3, D-4): per-connector consecutive-failure record — a
    /// display posture, not a sync decision. The mount auto-list
    /// checks it before building the request; the manual refresh
    /// clears the entry (explicit intent) and failure re-arms.
    auto_list_backoff: BTreeMap<String, AutoListBackoff>,
    /// The rendered connections items' addresses, index-aligned with
    /// the pushed `connection-items` (the callbacks' resolution table).
    items: Vec<ItemAddr>,
    /// The armed-confirm strip's subject (UX R-2 D-4, #787): the one
    /// armed item surface-wide, keyed by `ItemAddr` (identity, not
    /// index — a roster re-derivation keeps the address; a vanished
    /// subject's strip is dead). Arming touches no lane state; the
    /// confirm clears the flag BEFORE the send.
    armed_item: Option<ItemAddr>,
    /// The pickers' option ids, index-aligned with `board-options`.
    local_choices: Vec<uuid::Uuid>,
    /// The board the in-flight Stop/Publish op addresses (round-7
    /// D-7.3, D9 — the `busy_remote` precedent): the specific roster
    /// row's busy marker; cleared with `pending`.
    busy_board: Option<BoardId>,
    /// The connector name the in-flight op addresses (r2 D34): set by
    /// the send seams, cleared with `pending`.
    busy_target: String,
    /// The remote id the in-flight bind/create/detach addresses.
    busy_remote: Option<RemoteBoardId>,
    pending: Option<PendingKind>,
    replies: Vec<PendingReply>,
    outcome: SettingsOutcome,
    /// The concrete error retained past the coarse fold (r2 D28):
    /// the status text's typed granularity rides it.
    detail: Option<SettingsError>,
    /// The last-known persisted preset (r2 D27): seeded at mount,
    /// changed only by `apply_keymap_update` — the reply, never the
    /// click, is the selection's source.
    preset: crate::keymap::Preset,
    /// The scale field's staged buffer (round-3 D20a): set per
    /// keystroke on the scale field, discarded on Return commit and
    /// on focus loss. While present it is the field's rendered echo
    /// and the flavour's input; the live-scale echo renders only in
    /// its absence.
    scale_draft: Option<String>,
    /// The general snapshot (the GUI settings editor): seeded from
    /// the hand-over, refreshed ONLY from write replies (the reply
    /// echoes the fresh file values). Its env-override set is
    /// hand-over state — replies carry it empty by contract and the
    /// VM preserves the standing set on every apply.
    snapshot: crate::requests::SettingsSnapshot,
    /// The general text fields' staged buffers (the `scale_draft`
    /// precedent): set per keystroke, discarded on Return commit and
    /// on focus loss.
    day_start_draft: Option<String>,
    tick_draft: Option<String>,
    before_draft: Option<String>,
    poll_draft: Option<String>,
    backoff_initial_draft: Option<String>,
    backoff_max_draft: Option<String>,
    due_soon_draft: Option<String>,
    /// The focus minutes' staged buffers (the focus settings editor,
    /// #625): the same grammar as the other general text fields.
    focus_work_draft: Option<String>,
    focus_break_draft: Option<String>,
    focus_long_break_draft: Option<String>,
    focus_every_draft: Option<String>,
    /// The Files row's poll cadence field's staged buffer (#1083, W-6):
    /// the general fields' grammar verbatim.
    files_poll_secs_draft: Option<String>,
}

impl SettingsVm {
    /// Whether the draft may save (the Save button's enablement).
    fn save_enabled(&self) -> bool {
        self.pending.is_none() && settings_form::draft_valid(&self.draft)
    }

    /// Sends one request with its reply half parked. Returns whether
    /// the lane accepted it — a full/closed inbox degrades to the
    /// typed outcome at the caller (the choke point), never a wedge.
    fn send(
        &mut self,
        requests: &mpsc::Sender<UiRequest>,
        request: UiRequest,
        kind: PendingKind,
        reply: PendingReply,
        busy_target: &str,
    ) -> bool {
        if requests.try_send(request).is_err() {
            tracing::warn!("request channel full or closed; settings op dropped");
            return false;
        }
        self.pending = Some(kind);
        busy_target.clone_into(&mut self.busy_target);
        self.replies.push(reply);
        true
    }

    fn outcome_of(err: &SettingsError) -> SettingsOutcome {
        match err {
            SettingsError::Invalid { .. }
            | SettingsError::NoSecretStored
            | SettingsError::StoreUnsupported
            | SettingsError::VerifyRejected
            | SettingsError::PersistRejected
            | SettingsError::Gone
            | SettingsError::BindRefused
            | SettingsError::EnvLocked { .. }
            // The busy lane's own posture (T-38, #821): the surface
            // runs one request at a time — a click under a pending
            // request is a refusal, not a failure.
            | SettingsError::RequestPending => SettingsOutcome::Rejected,
            SettingsError::VerifyUnreachable
            | SettingsError::StoreFailed
            // The publish budget expired with the ceremony still
            // moving (D-6.9): transient, not a failure — observe the
            // board's sync state instead of retrying the publish.
            | SettingsError::SyncTimedOut => SettingsOutcome::Unavailable,
            SettingsError::VerifyFailed
            | SettingsError::PersistFailed
            | SettingsError::AttachBusy
            | SettingsError::AttachFailed
            // The move's step-failure class (round-6 D-6.8 phase 2,
            // D-5) folds every ceremony step class; the concrete
            // `MoveError` rode the app's log. The undo window's lost
            // coordinates fold the same failed class (US-6).
            | SettingsError::MoveFailed
            | SettingsError::UndoCoordinatesLost => SettingsOutcome::Failed,
            // The move's refusal classes (D-5): a lane-off or
            // busy-outbox world refused the attempt; the undo
            // window's raced/closed stages refuse too (US-6).
            SettingsError::FilesSyncDisabled
            | SettingsError::OutboxNotQuiescent
            | SettingsError::NothingToUndo
            | SettingsError::UndoClosed => {
                SettingsOutcome::Rejected
            }
        }
    }
}

/// The settings editor's general text fields (the GUI settings
/// editor chunk): the seam addresses, their dotted config keys (the
/// env-override set's comparison term), the lane fields, the mirror
/// predicates, and the canonical echoes — one table, no per-field
/// fork in the seams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneralField {
    DayStart,
    /// The Files row's poll cadence (#1083, the small-gated journeys
    /// plan W-6): rendered on the Files card, but the general fields'
    /// staged-edit grammar verbatim — one machine, no second lane.
    FilesPollSecs,
    TickSecs,
    ReminderBefore,
    PollInterval,
    BackoffInitial,
    BackoffMax,
    DueSoonDays,
    FocusWorkMins,
    FocusBreakMins,
    FocusLongBreakMins,
    FocusLongBreakEvery,
}

impl GeneralField {
    /// The `field-edited` index contract the view's general rows
    /// carry (the connector form owns 0–3, the scale field 4, the
    /// general fields 5–11, the focus minutes 12–15).
    const FOCUS_WORK_INDEX: usize = 12;
    const FOCUS_BREAK_INDEX: usize = 13;
    const FOCUS_LONG_BREAK_INDEX: usize = 14;
    const FOCUS_EVERY_INDEX: usize = 15;
    /// The Files row's poll cadence field (#1083): rides the general
    /// fields' index contract (the focus minutes' 12–15 block's next
    /// slot).
    const FILES_POLL_INDEX: usize = 16;

    fn from_index(index: usize) -> Option<Self> {
        match index {
            5 => Some(Self::DayStart),
            6 => Some(Self::TickSecs),
            7 => Some(Self::ReminderBefore),
            8 => Some(Self::PollInterval),
            9 => Some(Self::BackoffInitial),
            10 => Some(Self::BackoffMax),
            11 => Some(Self::DueSoonDays),
            Self::FOCUS_WORK_INDEX => Some(Self::FocusWorkMins),
            Self::FOCUS_BREAK_INDEX => Some(Self::FocusBreakMins),
            Self::FOCUS_LONG_BREAK_INDEX => Some(Self::FocusLongBreakMins),
            Self::FOCUS_EVERY_INDEX => Some(Self::FocusLongBreakEvery),
            Self::FILES_POLL_INDEX => Some(Self::FilesPollSecs),
            _ => None,
        }
    }

    /// The dotted config key — the env-override set's comparison
    /// term and the apply map's key.
    fn key(self) -> &'static str {
        match self {
            Self::DayStart => "preferences.day_start_time",
            Self::TickSecs => "reminders.tick_secs",
            Self::ReminderBefore => "reminders.default_before",
            Self::PollInterval => "sync.poll_interval_secs",
            Self::BackoffInitial => "sync.backoff_initial_secs",
            Self::BackoffMax => "sync.backoff_max_secs",
            Self::DueSoonDays => "ui.due_soon_days",
            Self::FocusWorkMins => "focus.work_mins",
            Self::FocusBreakMins => "focus.break_mins",
            Self::FocusLongBreakMins => "focus.long_break_mins",
            Self::FocusLongBreakEvery => "focus.long_break_every",
            Self::FilesPollSecs => "files_sync.poll_secs",
        }
    }

    fn lane_field(self) -> crate::requests::Field {
        match self {
            Self::DayStart => crate::requests::Field::DayStart,
            Self::TickSecs => crate::requests::Field::TickSecs,
            Self::ReminderBefore => crate::requests::Field::ReminderBefore,
            Self::PollInterval => crate::requests::Field::PollInterval,
            Self::BackoffInitial => crate::requests::Field::BackoffInitial,
            Self::BackoffMax => crate::requests::Field::BackoffMax,
            Self::DueSoonDays => crate::requests::Field::DueSoonDays,
            Self::FocusWorkMins => crate::requests::Field::FocusWorkMins,
            Self::FocusBreakMins => crate::requests::Field::FocusBreakMins,
            Self::FocusLongBreakMins => crate::requests::Field::FocusLongBreakMins,
            Self::FocusLongBreakEvery => crate::requests::Field::FocusLongBreakEvery,
            Self::FilesPollSecs => crate::requests::Field::FilesPollSecs,
        }
    }

    /// The mirror predicate (`settings_form`): the commit seam's
    /// refusal source — the same function the flavour renders.
    fn invalid(self, text: &str) -> Option<settings_form::InvalidKind> {
        match self {
            Self::DayStart => settings_form::day_start_invalid_kind(text),
            Self::TickSecs
            | Self::PollInterval
            | Self::BackoffInitial
            | Self::BackoffMax
            | Self::FilesPollSecs => settings_form::secs_invalid_kind(text),
            Self::ReminderBefore => settings_form::before_invalid_kind(text),
            Self::DueSoonDays => settings_form::due_soon_invalid_kind(text),
            Self::FocusWorkMins | Self::FocusBreakMins | Self::FocusLongBreakMins => {
                settings_form::focus_mins_invalid_kind(text)
            }
            Self::FocusLongBreakEvery => settings_form::focus_every_invalid_kind(text),
        }
    }

    /// The committed counterpart this field's ordering rule checks
    /// against, when one exists (the `initial ≤ max` rule, plan D8).
    fn ordering_ceiling(self, snapshot: &crate::requests::SettingsSnapshot) -> Option<u64> {
        match self {
            Self::BackoffInitial => Some(snapshot.backoff_max_secs),
            Self::BackoffMax => Some(snapshot.backoff_initial_secs),
            _ => None,
        }
    }

    /// The canonical echo of the committed snapshot value
    /// (draft-when-present, committed-otherwise — the scale echo's
    /// grammar).
    fn echo_of(self, snapshot: &crate::requests::SettingsSnapshot) -> String {
        match self {
            Self::DayStart => snapshot.day_start_time.format("%H:%M").to_string(),
            Self::TickSecs => snapshot.reminder_tick_secs.to_string(),
            Self::ReminderBefore => snapshot.reminder_before.clone(),
            Self::PollInterval => snapshot.poll_interval_secs.to_string(),
            Self::BackoffInitial => snapshot.backoff_initial_secs.to_string(),
            Self::BackoffMax => snapshot.backoff_max_secs.to_string(),
            Self::DueSoonDays => snapshot.due_soon_days.to_string(),
            Self::FocusWorkMins => snapshot.focus_work_mins.to_string(),
            Self::FocusBreakMins => snapshot.focus_break_mins.to_string(),
            Self::FocusLongBreakMins => snapshot.focus_long_break_mins.to_string(),
            Self::FocusLongBreakEvery => snapshot.focus_long_break_every.to_string(),
            // The raw `Option` echo: unset renders empty (the honest
            // "the default cadence applies" fact), a value renders its
            // seconds.
            Self::FilesPollSecs => snapshot
                .files_sync_poll_secs
                .map_or(String::new(), |secs| secs.to_string()),
        }
    }
}

/// The picker's index contract (r2 D27): `[Standard, Emacs, Vi]` —
/// the same order the request seam maps back to presets
/// (`settings_preset_picked`). One source for the push; the slint
/// property is an `int`.
fn preset_index(preset: crate::keymap::Preset) -> i32 {
    match preset {
        crate::keymap::Preset::Standard => 0,
        crate::keymap::Preset::Emacs => 1,
        crate::keymap::Preset::Vi => 2,
    }
}

/// The form-mirror field → lane field mapping (r2 D34): the save
/// click-through renders the same typed lattice the app-side
/// validation would answer with.
fn map_form_field(field: settings_form::Field) -> requests::Field {
    match field {
        settings_form::Field::ServerUrl => requests::Field::ServerUrl,
        settings_form::Field::Username => requests::Field::Username,
        settings_form::Field::Password => requests::Field::Password,
    }
}

/// The kind mirror → lane kind mapping (the same one-for-one lattice;
/// `NotFinite`/`OutOfRange` joined with the scale field's parity,
/// round-3 D20a — the sentences already exist in `settings_status`).
fn map_form_kind(kind: settings_form::InvalidKind) -> InvalidKind {
    match kind {
        settings_form::InvalidKind::Empty => InvalidKind::Empty,
        settings_form::InvalidKind::NotKebab => InvalidKind::NotKebab,
        settings_form::InvalidKind::BadUrl => InvalidKind::BadUrl,
        settings_form::InvalidKind::NotFinite => InvalidKind::NotFinite,
        settings_form::InvalidKind::OutOfRange => InvalidKind::OutOfRange,
    }
}

/// Converts a delivered key event to a [`crate::keymap::Stroke`]
/// (D18): `None` for anything that is not exactly one bindable key —
/// modifier-only/control events are rejected outright so text input
/// and traversal keep flowing.
fn to_stroke(text: &SharedString, modifiers: KeyboardModifiers) -> Option<crate::keymap::Stroke> {
    let mut chars = text.chars();
    let key = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    crate::keymap::Stroke::from_event(
        key,
        modifiers.control,
        modifiers.alt,
        modifiers.shift,
        modifiers.meta,
    )
}

/// The scale commit's scroll-anchor capture (round-3 D20c): the
/// viewport offset of the scale row, read PRE-layout (the caller
/// writes `Theme.base_scale` right after, re-laying the content out
/// on the next pass), stored with the one-shot flag. The markup's
/// `changed content-height` hook consumes it by restoring the row's
/// viewport position — measured, not predicted.
fn request_scale_anchor(window: &AppWindow) {
    let facts = window.global::<crate::SettingsScrollFacts>();
    facts.set_anchor_offset(facts.get_scale_row_y() + facts.get_viewport_y());
    facts.set_anchor_requested(true);
}

/// The settings surface's adapter half (round-1 chunk 3, D7/D8/D9):
/// the form mirrors, the send seams, the reply drain, and the
/// liveness overrides. Every reply path is non-blocking — the drain
/// polls `try_recv` under the repaint timer, and a dead lane
/// degrades to a cleared pending state (R-reply).
impl Adapter {
    /// The drain-side batch form of
    /// [`Self::record_system_event`].
    pub(crate) fn record_system_events(&mut self, events: &[taskboard_domain::SystemEvent]) {
        for event in events {
            self.record_system_event(event);
        }
    }

    /// The notice's cadence-tick half: expires the line past its
    /// horizon and pushes both status-bar properties. Runs every tick
    /// (cheap: two property writes), so expiry needs no timer of its
    /// own.
    pub(crate) fn push_notice(&mut self, window: &AppWindow) {
        let now = chrono::Utc::now();
        if !crate::presentation::notice_visible(now, self.notice.as_ref().map(|n| n.until)) {
            self.notice = None;
        }
        let (text, visible) = match &self.notice {
            Some(notice) => (notice.text.as_str(), true),
            None => ("", false),
        };
        window.set_notice_text(text.into());
        window.set_notice_visible(visible);
    }

    /// The two signal lanes' push half (the small-gated journeys plan
    /// W-5/W-7): the `ConfigReload` lane's typed state and the
    /// degraded-boot badge's flag, one write each per pass. Both lanes
    /// are display-only by construction (R-3/R-5): no command, no
    /// callback, no dismiss.
    pub(crate) fn push_signal_lanes(&mut self, window: &AppWindow) {
        let settings = window.global::<crate::SettingsModel>();
        settings.set_reload_lane_state(match self.reload_lane {
            crate::presentation::ConfigReloadPresentation::Quiet => {
                crate::ConfigReloadLaneState::None
            }
            crate::presentation::ConfigReloadPresentation::Applied => {
                crate::ConfigReloadLaneState::Applied
            }
            crate::presentation::ConfigReloadPresentation::KeptLastKnownGood => {
                crate::ConfigReloadLaneState::Kept
            }
        });
        window.set_journal_degraded_visible(self.journal_degraded);
    }

    /// Records one system-event episode (the system drain's
    /// pre-refresh hook, D8): connector liveness and the resurrect
    /// notice are the recorded verdicts today.
    pub(crate) fn record_system_event(&mut self, event: &taskboard_domain::SystemEvent) {
        use taskboard_domain::SystemEvent as E;
        match event {
            E::ConnectorLost(id) => {
                self.settings.liveness.insert(id.to_string(), true);
            }
            E::ConnectorRestored(id) => {
                self.settings.liveness.insert(id.to_string(), false);
            }
            E::ConfigReload(outcome) => {
                // The read-only Settings lane's fold (W-5, R-3): the
                // outcome surfaces, nothing dispatches.
                self.reload_lane = crate::presentation::fold_config_reload(outcome);
            }
            E::JournalDegraded { .. } => {
                // The degraded-boot badge (W-7, R-5): display-only and
                // persistent — set once, never cleared in-session; the
                // next healthy boot's fresh session replaces it.
                self.journal_degraded = true;
            }
            E::TaskResurrected(id) => {
                // The title resolves once, at event time, against the
                // current snapshot — the resurrected task is live and
                // unbound by the time the cycle settles.
                let title = self
                    .state
                    .load()
                    .tasks
                    .get(id)
                    .map(|t| t.title.clone())
                    .unwrap_or_default();
                self.notice = Some(Notice {
                    text: crate::presentation::resurrect_notice_phrase(&title),
                    until: chrono::Utc::now() + crate::presentation::RESURRECT_NOTICE_HORIZON,
                });
            }
            _ => {}
        }
    }

    /// Polls every parked reply once (`try_recv` — never a blocking
    /// recv, F18). A closed receiver (the forwarder died) counts as an
    /// outcome too: the pending state clears either way (R-reply).
    /// A `KeymapUpdate` swaps the keymap live and refreshes; a failed
    /// scale reverts the theme value; persistence is never undone by
    /// any reply.
    /// The delivery chip's drain (round-3 chunk 7, D31): the
    /// non-blocking watch poll beside the two broadcasts — a changed
    /// health snapshot pushes the chip properties and reports the
    /// change so the caller marks the pass dirty (the settings-reply
    /// drain's shape). Healthy maps to an absent chip; degraded maps
    /// to the visible one via [`crate::presentation::delivery_chip`].
    /// Returns whether the snapshot changed.
    pub(crate) fn drain_delivery_health(&mut self, window: &AppWindow) -> bool {
        if !self.delivery_health.has_changed().unwrap_or(false) {
            return false;
        }
        let health = self.delivery_health.borrow_and_update().clone();
        let chip = crate::presentation::delivery_chip(&health);
        window.set_delivery_chip_visible(chip.is_some());
        window.set_delivery_chip(SharedString::from(chip.unwrap_or_default()));
        true
    }

    /// The files-sync chip's drain (DR-8 chunk 4, D-FH5): the same
    /// non-blocking watch poll as the delivery chip's. `None` (the
    /// substrate off) leaves the chip at its hidden default — no boot
    /// with the actor off ever shows it (the golden-stability pin).
    /// Returns whether the snapshot changed.
    pub(crate) fn drain_files_sync(&mut self, window: &AppWindow) -> bool {
        let Some(rx) = self.files_sync.as_mut() else {
            return false;
        };
        if !rx.has_changed().unwrap_or(false) {
            return false;
        }
        let chip = rx.borrow_and_update().clone();
        // The per-board states map (US-8, D-8): the settings rows'
        // adoption phase rides the same watch.
        self.settings.files_board_states = chip.board_states.clone();
        paint_files_sync_chip(window, &chip);
        true
    }

    /// The mount auto-list (round-6 D-6.7): one `ListRemoteBoards`
    /// per connector without a landed listing, fired only from
    /// `settings_view_mounted` — deferred to the first settings
    /// visit, never at boot. Guards against double-firing: a landed
    /// listing (`remotes`), an in-flight auto listing
    /// (`auto_listings`), and the connector's manual refresh riding
    /// the pending machine. A full/closed request lane degrades to a
    /// log line (the `send` choke-point precedent); the map slot is
    /// what keeps the guard, so a failed `try_send` inserts nothing.
    fn fire_auto_listings(&mut self) {
        for row in &self.settings.rows {
            let connector = row.name.clone();
            // The backoff gate (US-3 AC-1, D-4): inside the window no
            // request fires and the card renders the failed truth
            // with the retry posture.
            if auto_list_suppressed(
                &self.settings.auto_list_backoff,
                &connector,
                tokio::time::Instant::now(),
            ) {
                continue;
            }
            if self.settings.remotes.contains_key(&connector)
                || self.settings.auto_listings.contains_key(&connector)
                || (self.settings.pending == Some(PendingKind::ListRemote)
                    && self.settings.busy_target == connector)
            {
                continue;
            }
            let (reply, reply_rx) = oneshot::channel();
            let request = UiRequest::ListRemoteBoards {
                connector: connector.clone(),
                reply,
            };
            if self.requests.try_send(request).is_ok() {
                self.settings.auto_listings.insert(connector, reply_rx);
            } else {
                tracing::warn!(
                    "request channel full or closed; the settings auto-list was dropped"
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // -- the export dialog (the J2 export plan W-1..W-5) ---------------
    // ------------------------------------------------------------------

    /// The root page's mint (the kiosk gate, R-2): the export
    /// dialog's action-menu entry point is ABSENT on kiosk — the
    /// matrix ruling the plan §2 records (no kiosk affordance), not
    /// a greyed row (D-4's disabled-with-caption doctrine is the
    /// dialog's own posture, not the entry points').
    fn mint_root_entries(
        &self,
        states: &std::collections::BTreeMap<ActionId, bool>,
    ) -> Vec<MenuEntry> {
        let mut page = menu::root_page(&self.keymap, &self.menu_chain, states);
        if self.kiosk {
            // The desktop-only rows render absent on kiosk (the
            // ExportDialogOpen precedent; UF2 R-2 adds the picker).
            page.retain(|entry| {
                entry.action != Some(ActionId::ExportDialogOpen)
                    && entry.action != Some(ActionId::AppUndoBranches)
            });
        }
        page
    }

    /// The close's outcomes (the interpretation arm's body, shared
    /// with the seam): the VM and the pending reply drop (US-2/S2's
    /// escape law — nothing dispatches, the app-side run completes
    /// or refuses independently), the insert mode unwinds.
    fn export_dialog_close_outcomes(&mut self) -> Vec<FlowOutcome> {
        if self.export_vm.take().is_none() {
            return Vec::new();
        }
        self.export_reply = None;
        // The preview/copy lanes' escape law (the D2 preview plan
        // D-7): the app-side generation completes or refuses
        // independently — the drain consumes the reply, the stash
        // clears with the dialog.
        self.export_preview_reply = None;
        self.export_preview_settled = None;
        self.export_copy_reply = None;
        self.export_copy_settled = None;
        if self.shell.mode == Mode::Insert {
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        } else {
            vec![FlowOutcome::RefreshNow]
        }
    }

    /// The pickers' mirror (W-3): the typed index contract mapped at
    /// the trust boundary — a stale index is absorbed (the silent
    /// law). The family pick clears the outcome (a new family's run
    /// is a new fact).
    pub(crate) fn export_spec_picked(&mut self, control: i32, index: i32) {
        use crate::export_dialog::{FAMILY_PICKED, FORMAT_PICKED, LAYOUT_PICKED};
        use crate::requests::{ExportFamily, ExportFileFormat, PdfLayoutChoice};
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        match control {
            FAMILY_PICKED => {
                vm.family = match index {
                    0 => ExportFamily::Tracking,
                    1 => ExportFamily::Pdf,
                    _ => return,
                };
                vm.outcome = None;
                // The D-5 family-gating fold: leaving the csv/json leg
                // invalidates the pane and the copy fold (the stale
                // family's facts never render).
                if vm.family != crate::requests::ExportFamily::Tracking {
                    vm.preview_shown = false;
                    vm.preview_text.clear();
                    vm.preview_truncated = false;
                    vm.preview_settled_refusal = None;
                    vm.copy_settled = false;
                    vm.copy_refusal = None;
                }
            }
            FORMAT_PICKED => {
                vm.format = match index {
                    0 => ExportFileFormat::Csv,
                    1 => ExportFileFormat::Json,
                    _ => return,
                };
            }
            LAYOUT_PICKED => {
                vm.layout = match index {
                    0 => PdfLayoutChoice::Fold,
                    1 => PdfLayoutChoice::A4,
                    2 => PdfLayoutChoice::A5,
                    3 => PdfLayoutChoice::A6,
                    _ => return,
                };
            }
            _ => {}
        }
    }

    /// The fields' mirror (W-3): verbatim text into the buffer (the
    /// one truth); validity folds at the push pass.
    pub(crate) fn export_field_edited(&mut self, field: i32, text: &str) {
        use crate::export_dialog::{
            FIELD_BOARD, FIELD_DESTINATION, FIELD_SINCE, FIELD_TASK, FIELD_UNTIL,
        };
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        match field {
            FIELD_SINCE => vm.since = text.into(),
            FIELD_UNTIL => vm.until = text.into(),
            FIELD_TASK => vm.task = text.into(),
            FIELD_BOARD => vm.board = text.into(),
            FIELD_DESTINATION => vm.destination = text.into(),
            _ => {}
        }
    }

    /// The open-after checkbox's mirror (the pdf leg's D-2 control;
    /// the widget gates the dispatch on the feature flag).
    pub(crate) fn export_open_after_toggled(&mut self) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        vm.open_after = !vm.open_after;
    }

    /// The Run dispatch (W-4, D-5): the trust boundary re-checks the
    /// gate (never trusts the widget's enabled fold), mints the spec
    /// verbatim from the buffer, and sends over the request lane
    /// with the oneshot reply half. Busy posture on the send; a
    /// full/closed lane is the typed Failed outcome (the F-3
    /// warn-and-drop convention would orphan the wait).
    pub(crate) fn export_run_clicked(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if !vm.run_enabled(&self.export_defaults) {
            return;
        }
        let spec = vm.mint_spec();
        let (reply, reply_rx) = oneshot::channel();
        let request = crate::requests::UiRequest::ExportRun { spec, reply };
        if self.requests.try_send(request).is_err() {
            // The lane died or is full: a typed failure, never a
            // silent absorb (the busy posture must not strand).
            vm.busy = false;
            vm.outcome = Some(crate::requests::ExportRunOutcome::Refused(
                crate::requests::ExportRefusal::LaneDropped,
            ));
            self.finish_mutation(window);
            return;
        }
        vm.busy = true;
        vm.outcome = None;
        self.export_reply = Some(reply_rx);
        self.finish_mutation(window);
    }

    /// The reply poll (W-5, D-5): the non-blocking poll the other
    /// drains run — a settled reply lifts the busy posture and
    /// STASHES the outcome for the display fold; the caller then
    /// invokes the window's `export-outcome-shown` callback OUTSIDE
    /// the adapter borrow (the re-entrancy law — the fold handler
    /// re-enters). A closed lane counts as settled (the sender died —
    /// the app half dropped the reply). Returns whether the callback
    /// must fire.
    pub(crate) fn poll_export_run(&mut self) -> bool {
        let Some(rx) = self.export_reply.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                tracing::warn!("the export run reply lane died; the slot clears");
                Some(crate::requests::ExportRunOutcome::Refused(
                    crate::requests::ExportRefusal::LaneDropped,
                ))
            }
        };
        self.export_reply = None;
        if let Some(vm) = self.export_vm.as_mut() {
            vm.busy = false;
        }
        self.export_settled = settled;
        true
    }

    /// The W-5 display fold (the `export-outcome-shown` handler): the
    /// stashed settled outcome lands in the dialog's VM and the push
    /// re-renders the outcome area. No settled stash = absorbed (a
    /// stray dispatch).
    pub(crate) fn export_outcome_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.export_settled.take() else {
            return;
        };
        if let Some(vm) = self.export_vm.as_mut() {
            vm.outcome = Some(outcome);
        }
        self.finish_mutation(window);
    }

    /// The dialog's pushed model (the recur-editor contract: the
    /// adapter computes it per pass, the widget never holds state).
    fn export_dialog_model(&self) -> crate::ExportDialogModel {
        use crate::requests::ExportRunOutcome as Outcome;
        let vm = self
            .export_vm
            .as_ref()
            .expect("the model mints only while the dialog holds");
        let family = match vm.family {
            crate::requests::ExportFamily::Tracking => 0,
            crate::requests::ExportFamily::Pdf => 1,
        };
        let format = match vm.format {
            crate::requests::ExportFileFormat::Csv => 0,
            crate::requests::ExportFileFormat::Json => 1,
        };
        let layout = match vm.layout {
            crate::requests::PdfLayoutChoice::Fold => 0,
            crate::requests::PdfLayoutChoice::A4 => 1,
            crate::requests::PdfLayoutChoice::A5 => 2,
            crate::requests::PdfLayoutChoice::A6 => 3,
        };
        let (outcome_kind, outcome_path, refusal) = match &vm.outcome {
            None => (crate::export_dialog::OUTCOME_NONE, String::new(), -1),
            Some(Outcome::Written { path }) => (
                crate::export_dialog::OUTCOME_WRITTEN,
                path.display().to_string(),
                -1,
            ),
            Some(Outcome::Refused(kind)) => (
                crate::export_dialog::OUTCOME_REFUSED,
                String::new(),
                crate::export_dialog::ExportVm::refusal_index(*kind),
            ),
        };
        crate::ExportDialogModel {
            family,
            format,
            layout,
            since: SharedString::from(vm.since.clone()),
            since_validity: crate::export_dialog::ExportVm::date_validity(&vm.since),
            until: SharedString::from(vm.until.clone()),
            until_validity: crate::export_dialog::ExportVm::date_validity(&vm.until),
            task: SharedString::from(vm.task.clone()),
            board: SharedString::from(vm.board.clone()),
            destination: SharedString::from(vm.destination.clone()),
            // The config default home as the placeholder (D-2/D-3):
            // the engine resolves the dated file name at run time.
            destination_placeholder: SharedString::from(match vm.family {
                crate::requests::ExportFamily::Tracking => self.export_defaults.export_dir.clone(),
                crate::requests::ExportFamily::Pdf => self.export_defaults.pdf_dir.clone(),
            }),
            open_after: vm.open_after,
            pdf_available: self.export_defaults.pdf_available,
            busy: vm.busy,
            run_enabled: vm.run_enabled(&self.export_defaults),
            outcome_kind,
            outcome_path: SharedString::from(outcome_path),
            refusal,
            // The preview/clipboard lane's folds (the D2 preview plan
            // W-3/W-4): the pane's facts verbatim, the per-affordance
            // busy flags, and the D-5 family-gating flag.
            preview_enabled: vm.family == crate::requests::ExportFamily::Tracking,
            preview_busy: vm.preview_busy,
            copy_busy: vm.copy_busy,
            preview_settled: vm.preview_shown || vm.preview_settled_refusal.is_some(),
            preview_shown: vm.preview_shown,
            preview_text: SharedString::from(vm.preview_text.clone()),
            preview_truncated: vm.preview_truncated,
            preview_refusal: vm
                .preview_settled_refusal
                .map(crate::requests::refusal_index)
                .unwrap_or(-1),
            copy_settled: vm.copy_settled,
            copy_bytes: vm.copy_bytes,
            copy_refusal: vm
                .copy_refusal
                .map(crate::requests::refusal_index)
                .unwrap_or(-1),
        }
    }

    /// The auto-listings' drain (round-6 D-6.7): the same
    /// non-blocking poll the other drains run, called beside
    /// [`Self::drain_setting_replies`] in the same repaint pass. A
    /// settled reply fills `remotes` and nothing else — never
    /// `pending`, `busy_target`, or the outcome banner (a background
    /// listing is silent by construction; the banner belongs to
    /// user-initiated ops). A failed reply clears the slot and leaves
    /// the row unlisted, so the Deck row's refresh hint stays. A
    /// closed lane counts as settled (the sender died); the slot
    /// clears so a later mount may retry.
    pub(crate) fn drain_auto_listings(&mut self, window: &AppWindow) -> bool {
        let mut settled: Vec<(String, Result<Vec<requests::RemoteBoardRow>, SettingsError>)> =
            Vec::new();
        for (connector, rx) in &mut self.settings.auto_listings {
            match rx.try_recv() {
                Ok(outcome) => settled.push((connector.clone(), outcome)),
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(oneshot::error::TryRecvError::Closed) => {
                    tracing::warn!("the settings auto-list reply lane died; the slot clears");
                    settled.push((connector.clone(), Err(SettingsError::VerifyFailed)));
                }
            }
        }
        let mut dirty = false;
        for (connector, outcome) in settled {
            self.settings.auto_listings.remove(&connector);
            match outcome {
                Ok(rows) => {
                    // A successful listing clears the backoff (AC-3).
                    self.settings.auto_list_backoff.remove(&connector);
                    self.settings.remotes.insert(connector, rows);
                }
                Err(err) => {
                    // The failure re-arms the backoff, one more
                    // consecutive failure than before (AC-2).
                    let now = tokio::time::Instant::now();
                    let entry = self
                        .settings
                        .auto_list_backoff
                        .entry(connector.clone())
                        .or_insert(AutoListBackoff {
                            consecutive_failures: 0,
                            last_failure: now,
                        });
                    entry.record_failure(now);
                    tracing::info!(variant = ?err, "the settings auto-list failed");
                }
            }
            dirty = true;
        }
        if dirty {
            // The model is a derive input outside the state Arc —
            // forced. `refresh_forced` ends in `push_settings`.
            self.refresh_forced(window);
        }
        dirty
    }

    /// Dispatches one `UiRequest::StatusQuery` (OP3 W-1, D-3): the
    /// reply half parks in [`StatusPanelVm`]; the slint-side busy
    /// posture holds until the reply folds or the channel drops (the
    /// typed failure — W-3). A full/closed lane degrades to the
    /// failure posture, never a wedge.
    fn dispatch_status_query(&mut self, window: &AppWindow) {
        let (reply, reply_rx) = oneshot::channel();
        if self
            .requests
            .try_send(crate::requests::UiRequest::StatusQuery { reply })
            .is_ok()
        {
            self.status_panel.pending = Some(reply_rx);
            window.global::<crate::StatusPanelFacts>().set_busy(true);
        } else {
            tracing::warn!("request channel full or closed; status query dropped");
            window.invoke_status_failure_shown();
        }
    }

    /// The status reply drain (OP3 W-2/W-3): the same non-blocking
    /// poll the repaint cadence runs — a settled reply folds into the
    /// panel's typed facts; a dropped reply renders the failure
    /// posture.
    pub(crate) fn drain_status_reply(&mut self, window: &AppWindow) {
        let Some(rx) = self.status_panel.pending.as_mut() else {
            return;
        };
        match rx.try_recv() {
            Ok(snapshot) => {
                self.status_panel.pending = None;
                window.invoke_status_fields_folded(status_panel_data(&snapshot));
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
            Err(oneshot::error::TryRecvError::Closed) => {
                self.status_panel.pending = None;
                window.invoke_status_failure_shown();
            }
        }
    }

    // -- the status panel's remediation lane (the D2 command plan
    //    W-1..W-4/W-6) ---------------------------------------------

    /// The remediation rows' arm moments (W-1/W-2/W-3): arm-only —
    /// nothing dispatches; the armed strip mounts. The permission
    /// fold (`kiosk`) is the adapter-side typed-permission gate (the
    /// trap-a law: a forbidden posture absorbs the arm — the rows are
    /// disabled there anyway; the gate never trusts the widget).
    pub(crate) fn status_remediation_arm(&mut self, window: &AppWindow, action: crate::requests::StatusRemediationAction) {
        if self.kiosk {
            tracing::info!(?action, "the remediation arm refused: kiosk posture");
            return;
        }
        {
            let facts = window.global::<crate::StatusPanelFacts>();
            facts.set_remediation_armed(match action {
                crate::requests::StatusRemediationAction::KillHolder => 0,
                crate::requests::StatusRemediationAction::ForceUnlock => 1,
                crate::requests::StatusRemediationAction::RestartDaemon => 2,
            });
        }
        self.status_panel.armed = Some(action);
    }

    /// The armed strip's decision pair (W-4): `0` = Confirm (the
    /// armed verb's exact dispatch — one `StatusRemediate` with the
    /// oneshot reply; busy posture on the strip), `1` = Cancel
    /// (nothing dispatched, the strip unmounts). A stale strip
    /// (disarmed under a foreign settle) absorbs the confirm.
    pub(crate) fn status_remediation_decision(&mut self, window: &AppWindow, decision: i32) {
        if decision != 0 {
            // Cancel: disarm, nothing dispatched.
            self.status_panel.armed = None;
            window
                .global::<crate::StatusPanelFacts>()
                .set_remediation_armed(-1);
            return;
        }
        let Some(action) = self.status_panel.armed else {
            return;
        };
        let (reply, reply_rx) = oneshot::channel();
        if self
            .requests
            .try_send(crate::requests::UiRequest::StatusRemediate { action, reply })
            .is_err()
        {
            // The lane died or is full: the typed infrastructure
            // refusal folds (the busy posture must not strand).
            self.remediation_settled = Some(crate::requests::StatusRemediationOutcome::Refused(
                crate::requests::StatusRemediationRefusal::IpcUnavailable,
            ));
            window.invoke_status_remediation_outcome_shown();
            return;
        }
        self.status_panel.remediation_pending = Some(reply_rx);
        window
            .global::<crate::StatusPanelFacts>()
            .set_remediation_busy(true);
    }

    /// The remediation reply poll (W-6): the same non-blocking poll
    /// the other drains run. A settled reply stashes for the fold and
    /// returns true (the callback fires OUTSIDE the adapter borrow);
    /// a closed lane counts as settled (the typed infrastructure
    /// refusal).
    pub(crate) fn poll_status_remediation(&mut self) -> bool {
        let Some(rx) = self.status_panel.remediation_pending.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                tracing::warn!("the remediation reply lane died; the slot clears");
                Some(crate::requests::StatusRemediationOutcome::Refused(
                    crate::requests::StatusRemediationRefusal::IpcUnavailable,
                ))
            }
        };
        self.status_panel.remediation_pending = None;
        self.status_panel.armed = None;
        self.remediation_settled = settled;
        true
    }

    /// The W-6 display fold (the `status_remediation_outcome_shown`
    /// handler): the stashed outcome lands in the panel's facts, the
    /// busy posture clears (the callback body), and the post-fact
    /// refresh re-queries (D-3: the outcome posture is never stale —
    /// one `StatusQuery` per executed remediation or refusal). No
    /// stash = absorbed (a stray dispatch).
    pub(crate) fn status_remediation_outcome_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.remediation_settled.take() else {
            return;
        };
        {
            let facts = window.global::<crate::StatusPanelFacts>();
            let (kind, refusal) = match &outcome {
                crate::requests::StatusRemediationOutcome::HolderSignalled => (1, -1),
                crate::requests::StatusRemediationOutcome::LockReleased => (2, -1),
                crate::requests::StatusRemediationOutcome::DaemonRestarted { .. } => (3, -1),
                crate::requests::StatusRemediationOutcome::Refused(kind) => {
                    // The canonical refusal index (the facts' doc
                    // table — one order, no drift).
                    let index = match kind {
                        crate::requests::StatusRemediationRefusal::HolderNotAlive => 0,
                        crate::requests::StatusRemediationRefusal::HolderAlive => 1,
                        crate::requests::StatusRemediationRefusal::SelfPid => 2,
                        crate::requests::StatusRemediationRefusal::SpawnFailed => 3,
                        crate::requests::StatusRemediationRefusal::IpcUnavailable => 4,
                    };
                    (4, index)
                }
            };
            facts.set_remediation_outcome_kind(kind);
            facts.set_remediation_refusal(refusal);
        }
        // The post-fact refresh (D-3): one re-query rides the settle.
        self.dispatch_status_query(window);
    }

    // -- the export dialog's preview/clipboard lane (the D2 preview
    //    plan W-1..W-4) -------------------------------------------

    /// The Preview dispatch (W-1, D-4): the trust boundary re-checks
    /// the gate (never trusts the widget), mints the spec verbatim
    /// from the buffer, and sends `ExportPreview` — never `ExportRun`
    /// (no file is written; the written-path fact stays Run's
    /// exclusively). Per-affordance busy on the send; a full/closed
    /// lane folds the typed `LaneDropped` refusal.
    pub(crate) fn export_preview_clicked(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if vm.busy || !vm.run_enabled(&self.export_defaults) {
            return;
        }
        let spec = vm.mint_spec();
        let (reply, reply_rx) = oneshot::channel();
        let request = crate::requests::UiRequest::ExportPreview { spec, reply };
        if self.requests.try_send(request).is_err() {
            vm.preview_settled_refusal = Some(crate::requests::ExportRefusal::LaneDropped);
            self.finish_mutation(window);
            return;
        }
        vm.preview_busy = true;
        vm.preview_shown = false;
        vm.preview_settled_refusal = None;
        self.export_preview_reply = Some(reply_rx);
        self.finish_mutation(window);
    }

    /// The Copy dispatch (W-2, D-3): the same boundary law as the
    /// preview — the payload REGENERATES app-side; the pane's text is
    /// never read.
    pub(crate) fn export_copy_clicked(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if vm.busy || !vm.run_enabled(&self.export_defaults) {
            return;
        }
        let spec = vm.mint_spec();
        let (reply, reply_rx) = oneshot::channel();
        let request = crate::requests::UiRequest::ExportClipboardCopy { spec, reply };
        if self.requests.try_send(request).is_err() {
            vm.copy_settled = true;
            vm.copy_refusal = Some(crate::requests::ExportRefusal::LaneDropped);
            self.finish_mutation(window);
            return;
        }
        vm.copy_busy = true;
        vm.copy_settled = false;
        vm.copy_refusal = None;
        self.export_copy_reply = Some(reply_rx);
        self.finish_mutation(window);
    }

    /// The preview reply poll (W-3): the non-blocking poll; settled
    /// replies stash for the `export_preview_shown` fold. A closed
    /// lane counts as settled (the typed `LaneDropped`).
    pub(crate) fn poll_export_preview(&mut self) -> bool {
        let Some(rx) = self.export_preview_reply.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                Some(crate::requests::ExportPreviewOutcome::Refused(
                    crate::requests::ExportRefusal::LaneDropped,
                ))
            }
        };
        self.export_preview_reply = None;
        if let Some(vm) = self.export_vm.as_mut() {
            vm.preview_busy = false;
        }
        self.export_preview_settled = settled;
        true
    }

    /// The copy reply poll (the D-3 twin).
    pub(crate) fn poll_export_copy(&mut self) -> bool {
        let Some(rx) = self.export_copy_reply.as_mut() else {
            return false;
        };
        let settled = match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(oneshot::error::TryRecvError::Empty) => return false,
            Err(oneshot::error::TryRecvError::Closed) => {
                Some(crate::requests::ExportClipboardOutcome::Refused(
                    crate::requests::ExportRefusal::LaneDropped,
                ))
            }
        };
        self.export_copy_reply = None;
        if let Some(vm) = self.export_vm.as_mut() {
            vm.copy_busy = false;
        }
        self.export_copy_settled = settled;
        true
    }

    /// The W-3 preview fold (the `export_preview_shown` handler): the
    /// stashed outcome lands in the dialog's VM (payload + the typed
    /// cap fact, or the refusal kind). The dialog stays mounted with
    /// its filter state intact on any refusal.
    pub(crate) fn export_preview_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.export_preview_settled.take() else {
            return;
        };
        if let Some(vm) = self.export_vm.as_mut() {
            vm.preview_busy = false;
            match outcome {
                crate::requests::ExportPreviewOutcome::Payload { text, truncated, .. } => {
                    vm.preview_settled_refusal = None;
                    vm.preview_shown = true;
                    vm.preview_text = text;
                    vm.preview_truncated = truncated;
                }
                crate::requests::ExportPreviewOutcome::Refused(kind) => {
                    vm.preview_settled_refusal = Some(kind);
                    vm.preview_shown = false;
                }
            }
        }
        self.finish_mutation(window);
    }

    /// The W-3's copy twin fold: the copied-byte fact or the refusal
    /// kind lands in the VM.
    pub(crate) fn export_copy_fold(&mut self, window: &AppWindow) {
        let Some(outcome) = self.export_copy_settled.take() else {
            return;
        };
        if let Some(vm) = self.export_vm.as_mut() {
            vm.copy_busy = false;
            vm.copy_settled = true;
            match outcome {
                crate::requests::ExportClipboardOutcome::Copied { bytes } => {
                    vm.copy_refusal = None;
                    vm.copy_bytes = i32::try_from(bytes).unwrap_or(i32::MAX);
                }
                crate::requests::ExportClipboardOutcome::Refused(kind) => {
                    vm.copy_refusal = Some(kind);
                }
            }
        }
        self.finish_mutation(window);
    }

    /// The D-5 family-gating fold (the `export_preview_gated`
    /// handler): the family pick leaving the csv/json leg invalidates
    /// the preview pane and the copy fold (the gating is a fold flag,
    /// never hidden lanes; the pane never shows a stale family's
    /// payload). Same family = absorbed.
    pub(crate) fn export_preview_gated_fold(&mut self, window: &AppWindow) {
        let Some(vm) = self.export_vm.as_mut() else {
            return;
        };
        if vm.family != crate::requests::ExportFamily::Tracking {
            vm.preview_shown = false;
            vm.preview_text.clear();
            vm.preview_truncated = false;
            vm.preview_settled_refusal = None;
            vm.preview_settled_refusal = None;
            vm.copy_settled = false;
            vm.copy_refusal = None;
            self.finish_mutation(window);
        }
    }

    /// The board-reconcile preview's drain (OP1 US-3, D-4): the same
    /// non-blocking poll the other drains run, called beside them in
    /// the repaint pass. A settled reply routes into the armed flow's
    /// `ReconcilePreviewState` (Ready — the letter's counts land, or
    /// Failed — the typed dead-lane fact disarms); the receiver slot
    /// clears either way. A pending-but-flowless slot (the flow died
    /// under a settle) clears silently — the reply is discarded.
    pub(crate) fn drain_reconcile_preview(&mut self, window: &AppWindow) {
        // The flowless slot (the armed flow died under a settle or a
        // confirm): the pending reply is discarded — the flowless-slot
        // rule, checked before any poll.
        if !matches!(
            self.view_state.flow(),
            crate::view_state::Flow::ConfirmBoardReconcile { .. }
        ) {
            self.reconcile_preview = None;
            return;
        }
        let Some((board, rx)) = self.reconcile_preview.as_mut() else {
            return;
        };
        let settled = match rx.try_recv() {
            Ok(Some(counts)) => Some(Ok(counts)),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            // A reply with no counts (the state read failed) is the
            // same not-derivable disarm as a dead lane — an unlettered
            // confirm is not informed consent (D-4's staging law).
            Ok(None) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(Err(())),
        };
        let Some(outcome) = settled else {
            return;
        };
        let board = *board;
        self.reconcile_preview = None;
        let outcomes = self
            .view_state
            .settle_board_reconcile_preview(board, outcome);
        self.apply_outcomes(window, &outcomes);
    }

    #[allow(clippy::too_many_lines)] // one arm per reply variant, kept flat
    pub(crate) fn drain_setting_replies(&mut self, window: &AppWindow) {
        let mut i = 0;
        let mut drained = false;
        while i < self.settings.replies.len() {
            // The settled reply's outcome, computed borrow-local and
            // routed through the one choke point below (r2 D28).
            let mut landed: Option<(SettingsOutcome, Option<SettingsError>)> = None;
            let settled = match &mut self.settings.replies[i] {
                PendingReply::Remotes(connector, rx) => match rx.try_recv() {
                    Ok(Ok(rows)) => {
                        // A successful manual listing clears the
                        // backoff too (US-3 AC-3 — the same posture
                        // as the auto lane's success).
                        self.settings.auto_list_backoff.remove(connector.as_str());
                        self.settings.remotes.insert(connector.clone(), rows);
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        // The manual refresh's failure re-arms the
                        // backoff (US-3 AC-2 — the override cleared
                        // the entry before the send).
                        let now = tokio::time::Instant::now();
                        let entry = self
                            .settings
                            .auto_list_backoff
                            .entry(connector.clone())
                            .or_insert(AutoListBackoff {
                                consecutive_failures: 0,
                                last_failure: now,
                            });
                        entry.record_failure(now);
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::List(rx) => match rx.try_recv() {
                    Ok(Ok(snapshot)) => {
                        // The connector rows re-render from the reply
                        // (r2 D32). The bindings do NOT read the
                        // reply's boards half (r4 D39 §2.3): the
                        // remote rows and the pickers' options are
                        // re-derived from the engine state on every
                        // push pass, so they cannot go stale by
                        // construction. A removed connector's listing
                        // leaves with it.
                        self.settings.rows = snapshot.connectors;
                        let rows = &self.settings.rows;
                        self.settings
                            .remotes
                            .retain(|name, _| rows.iter().any(|row| &row.name == name));
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::Keymap(rx) => match rx.try_recv() {
                    Ok(Ok(update)) => {
                        self.apply_keymap_update(window, update);
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::Snapshot(rx) => match rx.try_recv() {
                    Ok(Ok(snapshot)) => {
                        // The reply echoes the fresh file values (r2
                        // D32's rows-rerender rule, generalized). The
                        // env set is the hand-over's standing state —
                        // replies carry it empty by contract; the VM
                        // preserves its set, never overwrites.
                        let env = self.settings.snapshot.env_overridden.clone();
                        self.settings.snapshot = snapshot;
                        self.settings.snapshot.env_overridden = env;
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
                PendingReply::Scale(rx, previous) => match rx.try_recv() {
                    Ok(Ok(())) => {
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        // The live-apply revert (D9): the theme value
                        // goes back to the previous setting, and the
                        // scroll re-anchors on the scale row (r3
                        // D20c) — the content height returns and the
                        // row stays where it was.
                        request_scale_anchor(window);
                        #[allow(clippy::cast_possible_truncation)]
                        window
                            .global::<crate::Theme>()
                            .set_base_scale(*previous as f32);
                        true
                    }
                    Err(oneshot::error::TryRecvError::Empty) => false,
                    Err(oneshot::error::TryRecvError::Closed) => {
                        landed = Some((SettingsOutcome::Failed, None));
                        true
                    }
                },
            };
            if settled {
                let _ = self.settings.replies.remove(i);
                self.settings.pending = None;
                self.settings.busy_target.clear();
                self.settings.busy_remote = None;
                self.settings.busy_board = None;
                if let Some((outcome, detail)) = landed {
                    self.set_settings_outcome(window, outcome, detail);
                }
                drained = true;
            } else {
                i += 1;
            }
        }
        if drained {
            // The keymap swap / reply settle are derive inputs outside
            // the state Arc — forced. `refresh_forced` ends in
            // `push_settings`, so no separate push here.
            self.refresh_forced(window);
        }
    }

    /// The single outcome choke point (r2 D28): every assignment of
    /// the settings outcome routes through here — assign outcome +
    /// detail, drive the auto-clear timer (a pending op stops it; a
    /// terminal outcome re-arms it), then push. No outcome write may
    /// bypass it.
    fn set_settings_outcome(
        &mut self,
        window: &AppWindow,
        outcome: SettingsOutcome,
        detail: Option<SettingsError>,
    ) {
        self.settings.outcome = outcome;
        self.settings.detail = detail;
        if outcome == SettingsOutcome::Busy {
            // A pending op must never auto-clear mid-flight.
            self.status_timer.stop();
        } else {
            // Every new terminal outcome re-arms the clear.
            self.status_timer.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(crate::settings_status::STATUS_VISIBLE_MS),
                {
                    let fire = Rc::clone(&self.status_fire);
                    move || fire()
                },
            );
        }
        self.push_settings(window);
    }

    /// The rotation flip (K13): tick the state machine, reset the
    /// `FocusScrollHint` global (the stale-hint guard — a
    /// board-column hint surviving into a Today flip can never seek;
    /// the nonce bump is a guarded no-op seek, `row < 0` returns),
    /// apply the flip's outcomes (never a Command — the state
    /// machine's unit sweep pins that), and return the full dwell for
    /// the re-arm.
    fn rotation_tick(&mut self, window: &AppWindow) -> std::time::Duration {
        let Some(rotation) = self.rotation.as_mut() else {
            return std::time::Duration::ZERO;
        };
        let mode = rotation.tick();
        let dwell = rotation.dwell();
        let state = self.state.load();
        let outcomes = self.view_state.enter_kiosk_mode(&state, mode);
        // The kiosk flip swaps the mounted view outside `apply_effect`
        // entirely (r4 chunk 4, D14's enumerate-once table row) — an
        // explicit dismiss; cheap totality even though a passive kiosk
        // display rarely hovers.
        Self::dismiss_tooltip(window);
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(-1);
        scroll.set_target_b(-1);
        scroll.set_nonce(scroll.get_nonce() + 1);
        self.apply_outcomes(window, &outcomes);
        dwell
    }

    /// The dwell re-arm: every flip re-arms the FULL dwell — no
    /// carry-over, no interaction pause (D-V3: the display is
    /// passive; rotation wins over operator navigation).
    fn rearm_rotation(&mut self, dwell: std::time::Duration) {
        let fire = Rc::clone(&self.rotation_fire);
        self.rotation_timer
            .start(slint::TimerMode::SingleShot, dwell, move || fire());
    }

    /// The auto-clear fire (r2 D28): a stale fire during a new Busy
    /// is a no-op; otherwise the status returns to hidden (and the
    /// next refresh cannot resurrect it — the VM clears, not the
    /// property).
    fn settings_status_expired(&mut self, window: &AppWindow) {
        self.status_timer.stop();
        if self.settings.pending.is_some() {
            return;
        }
        self.settings.outcome = SettingsOutcome::None;
        self.settings.detail = None;
        self.push_settings(window);
    }

    /// The live keymap swap (D9): the new map drives resolution, menu
    /// hints, and tooltips from the next refresh on; the findings
    /// counter updates the status read. The replied preset is the
    /// selection echo's source (r2 D27) — it lands on the model with
    /// the next `push_settings`, never optimistically at click time.
    fn apply_keymap_update(&mut self, window: &AppWindow, update: KeymapUpdate) {
        self.keymap = update.keymap;
        self.preset = update.preset;
        self.settings.preset = update.preset;
        window
            .global::<crate::UiStatus>()
            .set_startup_keymap_findings(i32::try_from(update.findings).unwrap_or(i32::MAX));
    }

    /// The validity flavour pass (round-3 D19): each field's `FieldStatus`
    /// recomputed from the form ladder — the same source of truth the
    /// Save gate reads — plus the per-field error string, which is the
    /// status line's own sentence for the same (field, kind) pair (zero
    /// new presentation code). The password's validity is computed from
    /// the draft, which clears on send: a successful save never leaves a
    /// stale red border behind. The scale field's flavour comes from its
    /// staged draft only: with no draft the field merely echoes the live
    /// scale, which is valid by construction — a permanent success border
    /// would be noise, so that state is neutral. The scale echo is
    /// draft-when-present, live-scale-otherwise (round-3 D20a, finding
    /// 23): written on every pass so the field can never drift from the
    /// applied value.
    fn push_settings_validity(global: &crate::SettingsModel, vm: &SettingsVm, live_scale: f64) {
        fn apply(
            global: &crate::SettingsModel,
            field: settings_form::Field,
            status: settings_form::FieldStatus,
        ) {
            let (validity, error) = match status {
                settings_form::FieldStatus::Neutral => (crate::FieldValidity::Neutral, ""),
                settings_form::FieldStatus::Valid => (crate::FieldValidity::Valid, ""),
                settings_form::FieldStatus::Invalid(kind) => (
                    crate::FieldValidity::Invalid,
                    crate::settings_status::invalid_text(
                        map_form_field(field),
                        map_form_kind(kind),
                    ),
                ),
            };
            match field {
                settings_form::Field::ServerUrl => {
                    global.set_validity_server(validity);
                    global.set_error_server(error.into());
                }
                settings_form::Field::Username => {
                    global.set_validity_username(validity);
                    global.set_error_username(error.into());
                }
                settings_form::Field::Password => {
                    global.set_validity_password(validity);
                    global.set_error_password(error.into());
                }
            }
        }
        for field in [
            settings_form::Field::ServerUrl,
            settings_form::Field::Username,
            settings_form::Field::Password,
        ] {
            apply(global, field, settings_form::field_status(field, &vm.draft));
        }
        let scale_status = vm.scale_draft.as_deref().map_or(
            settings_form::FieldStatus::Neutral,
            settings_form::scale_status,
        );
        let (scale_validity, scale_error) = match scale_status {
            settings_form::FieldStatus::Neutral => (crate::FieldValidity::Neutral, ""),
            settings_form::FieldStatus::Valid => (crate::FieldValidity::Valid, ""),
            settings_form::FieldStatus::Invalid(kind) => (
                crate::FieldValidity::Invalid,
                crate::settings_status::invalid_text(
                    crate::requests::Field::Scale,
                    map_form_kind(kind),
                ),
            ),
        };
        global.set_validity_scale(scale_validity);
        global.set_error_scale(scale_error.into());
        global.set_scale_echo(
            vm.scale_draft
                .clone()
                .unwrap_or_else(|| settings_form::format_scale(live_scale))
                .into(),
        );
    }

    /// The account cards' walk-ordered projection (round-5 chunk 8,
    /// D-8.1): per card the header, the Files toggle (the default
    /// connector's identity only — the D-SF10 contract), and per
    /// connector a Deck row plus its remote rows (the last listing's
    /// universe; bindings resolve from the live state). Returns the
    /// rendered items and their address table, index-aligned.
    #[allow(clippy::too_many_lines)] // one walk-ordered fold, one arm per item kind
    fn connection_items_model(
        state: &AppState,
        vm: &SettingsVm,
    ) -> (Vec<crate::ConnectionItem>, Vec<ItemAddr>) {
        use crate::presentation::ConnectorStatus as S;
        let row_status = |status: S| match status {
            S::Idle => crate::SettingsRowStatus::Idle,
            S::Syncing => crate::SettingsRowStatus::Syncing,
            S::Offline => crate::SettingsRowStatus::Offline,
            S::Failed => crate::SettingsRowStatus::Failed,
            S::Down => crate::SettingsRowStatus::Down,
        };
        let pending = vm.pending;
        let blank = crate::ConnectionItem {
            kind: crate::ConnectionItemKind::Account,
            title: SharedString::new(),
            detail: SharedString::new(),
            show_title: false,
            status: crate::SettingsRowStatus::Idle,
            busy: false,
            bound: false,
            listed: false,
            files_on: false,
            pending_phase: crate::models::publish_card_phase(None),
            restore_offerable: false,
            adoption_phase: crate::models::files_board_adoption(
                crate::presentation::FilesBoardAdoption::None,
            ),
            files_enabled_pinned: false,
            files_anchor_pinned: false,
            files_restart: false,
            backend_word: SharedString::new(),
            badge_text: SharedString::new(),
            badge_tone: crate::SettingsBadgeTone::Neutral,
            action_label: SharedString::new(),
            action_enabled: false,
            archived: false,
            armed: false,
        };
        // The backend word per connector (round 6, D-6.8 phase 1): the
        // adapter owns the mapping, the UI stays dumb. Standalone
        // connectors drop the "Deck" prefix — the row shows the
        // connector's own name.
        let backend_word = |name: &str| {
            vm.rows
                .iter()
                .find(|row| row.name == name)
                .map(|row| match row.backend {
                    crate::requests::ConnectorBackend::Nextcloud => SharedString::from("Deck"),
                    crate::requests::ConnectorBackend::Standalone => SharedString::new(),
                })
                .unwrap_or_default()
        };
        let mut items = Vec::new();
        let mut addrs = Vec::new();
        for card in crate::presentation::account_cards(&vm.rows) {
            let status = crate::presentation::account_status(state, &card, |name| {
                vm.liveness.get(name).copied().unwrap_or(false)
            });
            items.push(crate::ConnectionItem {
                kind: crate::ConnectionItemKind::Account,
                title: format!("{} @ {}", card.username, card.server_url).into(),
                status: row_status(status),
                busy: pending == Some(PendingKind::Remove)
                    && card.connectors.contains(&vm.busy_target),
                ..blank.clone()
            });
            addrs.push(ItemAddr::Account {
                connector: card.connectors[0].clone(),
            });
            // The Files row rides every Nextcloud account card
            // (round-6 D-6.6): the one global flag shown as per-card
            // truth — `files_on` = enabled ∧ this card holds the
            // resolved anchor, so exactly one card renders On. The
            // re-anchor target is the card's first Nextcloud
            // connector (deterministic walk order). Standalone cards
            // carry no Files row.
            let is_nextcloud = |name: &str| {
                vm.rows
                    .iter()
                    .find(|row| row.name == name)
                    .is_some_and(|row| row.backend == crate::requests::ConnectorBackend::Nextcloud)
            };
            if let Some(files_connector) = card
                .connectors
                .iter()
                .find(|name| is_nextcloud(name))
                .cloned()
            {
                let files_on = vm.snapshot.files_sync_enabled
                    && vm.snapshot.files_sync_anchor.as_deref() == Some(files_connector.as_str());
                items.push(crate::ConnectionItem {
                    kind: crate::ConnectionItemKind::Files,
                    files_on,
                    // The move ceremony rides the Files row (round-6
                    // D-6.8 phase 2, D-9): one BusyDot, no fake
                    // progress — the reply is the next truth.
                    busy: pending == Some(PendingKind::Move) && vm.busy_target == files_connector,
                    files_enabled_pinned: crate::presentation::files_enabled_pinned(
                        &vm.snapshot.env_overridden,
                    ),
                    files_anchor_pinned: crate::presentation::files_anchor_pinned(
                        &vm.snapshot.env_overridden,
                    ),
                    files_restart: vm.snapshot.applies.get("files_sync.enabled").is_some_and(
                        |semantics| semantics.apply == crate::requests::SectionApply::OnRestart,
                    ),
                    ..blank.clone()
                });
                addrs.push(ItemAddr::Files {
                    connector: files_connector.clone(),
                });
                // The per-account roster (round-7 D-7.3, W-3/D11): the
                // Files section's body — every live board once per
                // Nextcloud card, channel-tagged, sorted, one action
                // per row, all composed Rust-side by the fold. Binding
                // truth re-derived from the engine state every pass;
                // rendering is independent of the lane switch
                // (toggling Files off freezes the sync, the binding
                // persists — hiding the rows would fake the world).
                for row in
                    crate::presentation::files_board_roster(state, files_on, &files_connector)
                {
                    let busy = matches!(
                        pending,
                        Some(PendingKind::StopFilesSync | PendingKind::PublishFiles)
                    ) && vm.busy_board == Some(row.board);
                    items.push(crate::ConnectionItem {
                        kind: crate::ConnectionItemKind::FilesBoard,
                        title: row.title.clone().into(),
                        busy,
                        restore_offerable: row.restore_offerable,
                        adoption_phase: {
                            // The adoption phase (US-8, D-8): keyed by
                            // the files identity (the board's own
                            // domain uuid); boards absent from the map
                            // render the settled truth.
                            let files_uuid = state
                                .boards
                                .get(&row.board)
                                .and_then(|b| b.remote.as_ref())
                                .and_then(|binding| binding.board.as_files());
                            let adoption = files_uuid
                                .and_then(|uuid| vm.files_board_states.get(&uuid))
                                .map_or(crate::presentation::FilesBoardAdoption::None, |st| {
                                    st.adoption
                                });
                            crate::models::files_board_adoption(adoption)
                        },
                        badge_text: crate::presentation::files_board_badge_label(row.badge).into(),
                        badge_tone: crate::presentation::files_board_badge_tone(row.badge),
                        action_label: row
                            .action
                            .map(crate::presentation::files_board_action_label)
                            .unwrap_or_default()
                            .into(),
                        action_enabled: row.action_enabled,
                        archived: row.archived,
                        ..blank.clone()
                    });
                    addrs.push(ItemAddr::FilesBoard {
                        board: row.board,
                        connector: files_connector.clone(),
                    });
                }
            }
            let several = card.connectors.len() > 1;
            for connector in &card.connectors {
                let listing = vm.remotes.get(connector);
                items.push(crate::ConnectionItem {
                    kind: crate::ConnectionItemKind::Deck,
                    title: connector.clone().into(),
                    backend_word: backend_word(connector),
                    show_title: several,
                    busy: matches!(
                        pending,
                        Some(
                            PendingKind::ListRemote
                                | PendingKind::Attach
                                | PendingKind::CreateBind
                                | PendingKind::Detach
                                | PendingKind::Publish
                        )
                    ) && vm.busy_target == *connector,
                    // The ceremony's card-local phase (US-5, D-6):
                    // display-only, the addressed connector only.
                    pending_phase: {
                        let publishing =
                            pending == Some(PendingKind::Publish) && vm.busy_target == *connector;
                        let adopting = pending == Some(PendingKind::PublishFiles)
                            && vm.busy_target == *connector;
                        crate::models::publish_card_phase(crate::presentation::publish_card_phase(
                            (publishing || adopting).then_some((publishing, adopting)),
                        ))
                    },
                    listed: listing.is_some(),
                    ..blank.clone()
                });
                addrs.push(ItemAddr::Deck {
                    connector: connector.clone(),
                });
                // The listing's rows (round-5 chunk 8), then the
                // bindings-without-listing rows (round-6 D-6.7): a
                // live binding on this connector whose remote is not
                // among the landed listing still renders — fully
                // actionable — titled by the local board (the remote
                // title is unknown until a listing enriches it).
                let listed_remotes: Vec<_> = listing
                    .into_iter()
                    .flatten()
                    .map(|remote| remote.remote_id)
                    .collect();
                let live_bindings: Vec<_> = state
                    .boards
                    .values()
                    .filter(|board| {
                        !board.deleted
                            && !board.archived
                            && board
                                .remote
                                .as_ref()
                                .is_some_and(|binding| binding.connector.as_str() == connector)
                    })
                    .collect();
                let unlisted_bindings: Vec<_> = live_bindings
                    .iter()
                    .filter(|board| {
                        board
                            .remote
                            .as_ref()
                            .is_some_and(|binding| !listed_remotes.contains(&binding.board))
                    })
                    .collect();
                for remote in listing.into_iter().flatten() {
                    let bound = live_bindings
                        .iter()
                        .find(|board| {
                            board
                                .remote
                                .as_ref()
                                .is_some_and(|binding| binding.board == remote.remote_id)
                        })
                        .copied();
                    items.push(crate::ConnectionItem {
                        kind: crate::ConnectionItemKind::Remote,
                        title: remote.title.clone().into(),
                        detail: bound.map(|b| b.title.clone()).unwrap_or_default().into(),
                        bound: bound.is_some(),
                        busy: matches!(
                            pending,
                            Some(
                                PendingKind::Attach | PendingKind::CreateBind | PendingKind::Detach
                            )
                        ) && vm.busy_target == *connector
                            && vm.busy_remote == Some(remote.remote_id),
                        ..blank.clone()
                    });
                    addrs.push(ItemAddr::Remote {
                        connector: connector.clone(),
                        remote: remote.remote_id,
                        bound: bound.map(|b| b.id),
                    });
                }
                // The bindings whose remote is absent from the listing
                // (round-6 D-6.7): synthesized Remote rows, `bound` by
                // construction, titled by the local board, detail
                // empty (the remote title is unknown pre-listing).
                // The address is the real one, so detach/bind-pick
                // resolve unchanged — the row is fully actionable
                // before any listing exists.
                for board in &unlisted_bindings {
                    let binding = board
                        .remote
                        .as_ref()
                        .expect("unlisted_bindings filtered on a binding");
                    items.push(crate::ConnectionItem {
                        kind: crate::ConnectionItemKind::Remote,
                        title: board.title.clone().into(),
                        bound: true,
                        ..blank.clone()
                    });
                    addrs.push(ItemAddr::Remote {
                        connector: connector.clone(),
                        remote: binding.board,
                        bound: Some(board.id),
                    });
                }
            }
        }
        (items, addrs)
    }

    /// Pushes the settings view-model onto the window global (the
    /// refresh pass's settings half; rows fold liveness + the bound
    /// boards' phases, D8). The row lists ride their persistent
    /// handles through the positional diff (r2 D9).
    #[allow(clippy::too_many_lines)] // the per-pass push, one block per surface fact
    pub(crate) fn push_settings(&mut self, window: &AppWindow) {
        use crate::SettingsRowStatus as Row;
        let state = self.state.load();
        // The busy machine (r2 D34): the flag timeline is the model's;
        // the connector rows mark the Remove ceremony.
        let connector_busy = matches!(self.settings.pending, Some(PendingKind::Remove));
        // The pickers' options (r4 D39 §2.3's rule, kept): the unbound
        // live boards, re-derived from the engine state EVERY pass —
        // the mount-refresh by construction.
        let choices: Vec<requests::BoardInfo> = crate::presentation::boards_of(&state)
            .into_iter()
            .filter(|board| board.bound_connector.is_none())
            .collect();
        let options: Vec<crate::DropdownRow> = choices
            .iter()
            .map(|board| crate::DropdownRow {
                label: board.name.clone().into(),
                indicator: crate::DropdownIndicator::LinkOff,
                tone: crate::DropdownTone::Neutral,
                selected: false,
                busy: false,
            })
            .collect();
        self.settings.local_choices = choices.iter().map(|board| board.id).collect();
        let (mut items, addrs) = Self::connection_items_model(&state, &self.settings);
        self.settings.items = addrs;
        // The armed strip's pass rules (UX R-2 D-4): the busy lane
        // disarms (the outcome render for that lane — the disable
        // grammar prevents re-entry, the disarm keeps the grammar
        // honest), a vanished subject disarms (the stale-`ItemAddr`
        // dead-strip rule), and the armed flag mirrors per row.
        if self.settings.pending.is_some()
            || self
                .settings
                .armed_item
                .as_ref()
                .is_none_or(|armed| !self.settings.items.contains(armed))
        {
            self.settings.armed_item = None;
        }
        for (item, addr) in items.iter_mut().zip(&self.settings.items) {
            item.armed = self.settings.armed_item.as_ref() == Some(addr);
        }
        let rows: Vec<crate::ConnectionRow> = self
            .settings
            .rows
            .iter()
            .map(|row| {
                let id = ConnectorId::from(row.name.clone());
                let down = self.settings.liveness.get(row.name.as_str()).copied();
                let status =
                    crate::presentation::connector_row_status(&state, &id, down.unwrap_or(false));
                crate::ConnectionRow {
                    name: row.name.clone().into(),
                    server: row.server_url.clone().into(),
                    username: row.username.clone().into(),
                    busy: connector_busy && self.settings.busy_target == row.name,
                    status: match status {
                        crate::presentation::ConnectorStatus::Idle => Row::Idle,
                        crate::presentation::ConnectorStatus::Syncing => Row::Syncing,
                        crate::presentation::ConnectorStatus::Offline => Row::Offline,
                        crate::presentation::ConnectorStatus::Failed => Row::Failed,
                        crate::presentation::ConnectorStatus::Down => Row::Down,
                    },
                }
            })
            .collect();
        let global = window.global::<crate::SettingsModel>();
        self.models.push_settings_rows(&global, &rows);
        self.models.push_board_options(&global, &options);
        self.models.push_connection_items(&global, &items);
        // The page-usable guard (US-5 AC-1, D-6): the publish family
        // keeps the page's controls enabled — the addressed card's
        // phase badge carries the ceremony; the attach/remove family
        // keeps today's wholesale posture.
        global.set_busy(
            self.settings.pending.is_some_and(|kind| {
                !matches!(kind, PendingKind::Publish | PendingKind::PublishFiles)
            }),
        );
        global.set_busy_target(self.settings.busy_target.clone().into());
        global.set_draft_server(self.settings.draft.server_url.clone().into());
        global.set_draft_username(self.settings.draft.username.clone().into());
        global.set_save_enabled(self.settings.save_enabled());
        let live_scale = f64::from(window.global::<crate::Theme>().get_base_scale());
        Self::push_settings_validity(&global, &self.settings, live_scale);
        // The timers `when` field's validity flavour (r4 D27): the
        // same per-pass push, over the typed draft — the timer
        // grammar's own tz seam (`&chrono::Local`, the parse's
        // convention).
        let (when_validity, when_hint) = match crate::presentation::timer_when_validity(
            &self.timer_when_draft,
            self.day_ctx().now,
            &chrono::Local,
        ) {
            None => (crate::FieldValidity::Neutral, ""),
            Some(true) => (crate::FieldValidity::Valid, ""),
            Some(false) => (
                crate::FieldValidity::Invalid,
                crate::presentation::TIMER_SYNTAX_HINT,
            ),
        };
        window.set_timer_when_validity(when_validity);
        window.set_timer_when_error_hint(SharedString::from(when_hint));
        // The break-rule form's facts (T-14, #797): the form's open
        // flag and the threshold field's validity flavour (the
        // `when` field's convention — parse at the trust boundary
        // with the `WorkThreshold` mint's text grammar).
        window.set_break_create_open(self.break_rule_form_open);
        let (break_validity, break_hint) = if self.break_rule_threshold_draft.is_empty() {
            (crate::FieldValidity::Neutral, "")
        } else {
            match taskboard_domain::WorkThreshold::parse_text_form(&self.break_rule_threshold_draft)
            {
                Ok(_) => (crate::FieldValidity::Valid, ""),
                Err(_) => (
                    crate::FieldValidity::Invalid,
                    crate::presentation::BREAK_THRESHOLD_HINT,
                ),
            }
        };
        window.set_break_threshold_validity(break_validity);
        window.set_break_threshold_error_hint(SharedString::from(break_hint));
        // The journal ledger's editor facts (T-15, #798): open flag,
        // the staged echoes, the ratings' validity flavours (the
        // `Rating` mint's 1..=5; empty is neutral — the legal clear),
        // the curated flag chips' rendered membership, and the armed
        // strip's flag.
        window.set_reflection_editor_open(self.reflection_active.is_some());
        window.set_reflection_editor_text_echo(SharedString::from(
            self.reflection_text_draft.clone(),
        ));
        window.set_reflection_editor_mood_echo(SharedString::from(
            self.reflection_mood_draft.clone(),
        ));
        window.set_reflection_editor_energy_echo(SharedString::from(
            self.reflection_energy_draft.clone(),
        ));
        let rating_validity = |draft: &str| {
            if draft.is_empty() {
                crate::FieldValidity::Neutral
            } else if taskboard_domain::reflection::Rating::try_new(
                draft.parse::<u8>().unwrap_or(0),
            )
            .is_ok()
            {
                crate::FieldValidity::Valid
            } else {
                crate::FieldValidity::Invalid
            }
        };
        window.set_reflection_editor_mood_validity(rating_validity(&self.reflection_mood_draft));
        window
            .set_reflection_editor_energy_validity(rating_validity(&self.reflection_energy_draft));
        window.set_reflection_remove_armed(self.reflection_armed.is_some());
        let chips: Vec<crate::ReflectionFlagChip> = crate::presentation::HEALTH_FLAGS
            .iter()
            .map(|flag| {
                let current = self
                    .reflection_active
                    .and_then(|id| {
                        self.state
                            .load()
                            .reflection
                            .entries
                            .get(&id)
                            .map(|entry| entry.attributes.flags.contains(flag))
                    })
                    .unwrap_or(false);
                let member = if current {
                    !self.reflection_flags_remove.contains(flag)
                } else {
                    self.reflection_flags_add.contains(flag)
                };
                crate::ReflectionFlagChip {
                    label: SharedString::from(flag.text_form()),
                    member,
                }
            })
            .collect();
        window.set_reflection_editor_flags_model(ModelRc::new(slint::VecModel::from(chips)));
        global.set_preset_index(preset_index(self.settings.preset));
        global.set_status_text(SharedString::from(crate::settings_status::status_text(
            self.settings.outcome,
            self.settings.detail.as_ref(),
        )));
        global.set_last_outcome(match self.settings.outcome {
            SettingsOutcome::None => crate::SettingsOutcome::None,
            SettingsOutcome::Ok => crate::SettingsOutcome::Ok,
            SettingsOutcome::Busy => crate::SettingsOutcome::Busy,
            SettingsOutcome::Rejected => crate::SettingsOutcome::Rejected,
            SettingsOutcome::Unavailable => crate::SettingsOutcome::Unavailable,
            SettingsOutcome::Failed => crate::SettingsOutcome::Failed,
        });
        // The T-16 typed detail fact: the refusal's EnvLocked class
        // renders structurally (the tests' typed projection of the
        // detail variant — the sentence text is never asserted).
        global.set_last_error_env_locked(matches!(
            self.settings.detail,
            Some(SettingsError::EnvLocked { .. })
        ));
        // The T-38 typed detail fact (the env-locked fact's twin):
        // the busy lane's RequestPending class renders structurally —
        // the tests' typed projection of the detail variant.
        global.set_last_error_request_pending(matches!(
            self.settings.detail,
            Some(SettingsError::RequestPending)
        ));
        self.push_general_settings(&global);
    }

    /// The general editor's push half (the GUI settings editor): per
    /// field the echo (draft-when-present, committed-otherwise — the
    /// scale echo's grammar), the validity flavour (the same mirror
    /// predicate the commit seam refuses on), and the app-stated
    /// env-lock and apply facts; the choice echoes ride the snapshot;
    /// the read-only facts list re-renders per pass (the board
    /// options' re-derive precedent).
    #[allow(clippy::too_many_lines)] // per-field push, one arm per field
    fn push_general_settings(&mut self, global: &crate::SettingsModel) {
        let vm = &self.settings;
        for (gf, draft) in [
            (GeneralField::DayStart, &vm.day_start_draft),
            (GeneralField::TickSecs, &vm.tick_draft),
            (GeneralField::ReminderBefore, &vm.before_draft),
            (GeneralField::PollInterval, &vm.poll_draft),
            (GeneralField::BackoffInitial, &vm.backoff_initial_draft),
            (GeneralField::BackoffMax, &vm.backoff_max_draft),
            (GeneralField::DueSoonDays, &vm.due_soon_draft),
            (GeneralField::FocusWorkMins, &vm.focus_work_draft),
            (GeneralField::FocusBreakMins, &vm.focus_break_draft),
            (GeneralField::FocusLongBreakMins, &vm.focus_long_break_draft),
            (GeneralField::FocusLongBreakEvery, &vm.focus_every_draft),
            (GeneralField::FilesPollSecs, &vm.files_poll_secs_draft),
        ] {
            let predicate: fn(&str) -> Option<settings_form::InvalidKind> = match gf {
                GeneralField::DayStart => settings_form::day_start_invalid_kind,
                GeneralField::TickSecs
                | GeneralField::PollInterval
                | GeneralField::BackoffInitial
                | GeneralField::BackoffMax
                | GeneralField::FilesPollSecs => settings_form::secs_invalid_kind,
                GeneralField::ReminderBefore => settings_form::before_invalid_kind,
                GeneralField::DueSoonDays => settings_form::due_soon_invalid_kind,
                GeneralField::FocusWorkMins
                | GeneralField::FocusBreakMins
                | GeneralField::FocusLongBreakMins => settings_form::focus_mins_invalid_kind,
                GeneralField::FocusLongBreakEvery => settings_form::focus_every_invalid_kind,
            };
            let status = draft
                .as_deref()
                .map_or(settings_form::FieldStatus::Neutral, |text| {
                    settings_form::general_status(text, predicate)
                });
            let (validity, error) = match status {
                settings_form::FieldStatus::Neutral => (crate::FieldValidity::Neutral, ""),
                settings_form::FieldStatus::Valid => (crate::FieldValidity::Valid, ""),
                settings_form::FieldStatus::Invalid(kind) => (
                    crate::FieldValidity::Invalid,
                    crate::settings_status::invalid_text(gf.lane_field(), map_form_kind(kind)),
                ),
            };
            let echo: SharedString = draft
                .clone()
                .unwrap_or_else(|| gf.echo_of(&vm.snapshot))
                .into();
            let env_locked = vm.snapshot.env_overridden.contains(gf.key());
            let on_restart = vm
                .snapshot
                .applies
                .get(gf.key())
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart);
            match gf {
                GeneralField::DayStart => {
                    global.set_day_start_echo(echo);
                    global.set_validity_day_start(validity);
                    global.set_error_day_start(error.into());
                    global.set_day_start_env_locked(env_locked);
                    global.set_day_start_on_restart(on_restart);
                }
                GeneralField::TickSecs => {
                    global.set_tick_echo(echo);
                    global.set_validity_tick(validity);
                    global.set_error_tick(error.into());
                    global.set_tick_env_locked(env_locked);
                    global.set_tick_on_restart(on_restart);
                }
                GeneralField::ReminderBefore => {
                    global.set_before_echo(echo);
                    global.set_validity_before(validity);
                    global.set_error_before(error.into());
                    global.set_before_env_locked(env_locked);
                    global.set_before_on_restart(on_restart);
                }
                GeneralField::PollInterval => {
                    global.set_poll_echo(echo);
                    global.set_validity_poll(validity);
                    global.set_error_poll(error.into());
                    global.set_poll_env_locked(env_locked);
                    global.set_poll_on_restart(on_restart);
                }
                GeneralField::BackoffInitial => {
                    global.set_backoff_initial_echo(echo);
                    global.set_validity_backoff_initial(validity);
                    global.set_error_backoff_initial(error.into());
                    global.set_backoff_initial_env_locked(env_locked);
                    global.set_backoff_initial_on_restart(on_restart);
                }
                GeneralField::BackoffMax => {
                    global.set_backoff_max_echo(echo);
                    global.set_validity_backoff_max(validity);
                    global.set_error_backoff_max(error.into());
                    global.set_backoff_max_env_locked(env_locked);
                    global.set_backoff_max_on_restart(on_restart);
                }
                GeneralField::DueSoonDays => {
                    global.set_due_soon_echo(echo);
                    global.set_validity_due_soon(validity);
                    global.set_error_due_soon(error.into());
                    global.set_due_soon_env_locked(env_locked);
                    global.set_due_soon_on_restart(on_restart);
                }
                GeneralField::FocusWorkMins => {
                    global.set_focus_work_echo(echo);
                    global.set_validity_focus_work(validity);
                    global.set_error_focus_work(error.into());
                    global.set_focus_work_env_locked(env_locked);
                    global.set_focus_work_on_restart(on_restart);
                }
                GeneralField::FocusBreakMins => {
                    global.set_focus_break_echo(echo);
                    global.set_validity_focus_break(validity);
                    global.set_error_focus_break(error.into());
                    global.set_focus_break_env_locked(env_locked);
                    global.set_focus_break_on_restart(on_restart);
                }
                GeneralField::FocusLongBreakMins => {
                    global.set_focus_long_break_echo(echo);
                    global.set_validity_focus_long_break(validity);
                    global.set_error_focus_long_break(error.into());
                    global.set_focus_long_break_env_locked(env_locked);
                    global.set_focus_long_break_on_restart(on_restart);
                }
                GeneralField::FocusLongBreakEvery => {
                    global.set_focus_every_echo(echo);
                    global.set_validity_focus_every(validity);
                    global.set_error_focus_every(error.into());
                    global.set_focus_every_env_locked(env_locked);
                    global.set_focus_every_on_restart(on_restart);
                }
                GeneralField::FilesPollSecs => {
                    global.set_files_poll_echo(echo);
                    global.set_validity_files_poll(validity);
                    global.set_error_files_poll(error.into());
                    global.set_files_poll_env_locked(env_locked);
                    // The registry's verdict (R-4): poll_secs applies
                    // live through the whole-snapshot lane; the same
                    // app-stated fact every live row renders.
                    global.set_files_poll_on_restart(on_restart);
                }
            }
        }
        // The choice echoes and facts.
        global.set_reminders_enabled_index(i32::from(!vm.snapshot.reminders_enabled));
        let enabled_locked = vm.snapshot.env_overridden.contains("reminders.enabled");
        global.set_reminders_enabled_env_locked(enabled_locked);
        global.set_reminders_enabled_on_restart(
            vm.snapshot
                .applies
                .get("reminders.enabled")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        let sink_index = match vm.snapshot.reminder_sink {
            crate::requests::ReminderSink::Auto => 0,
            crate::requests::ReminderSink::Log => 1,
            crate::requests::ReminderSink::Desktop => 2,
        };
        global.set_sink_index(sink_index);
        global.set_sink_env_locked(vm.snapshot.env_overridden.contains("reminders.sink"));
        global.set_sink_on_restart(
            vm.snapshot
                .applies
                .get("reminders.sink")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        // The focus choice echoes (commit-on-pick, the reminders
        // grammar): index 0 = On, 1 = Off.
        global.set_focus_enabled_index(i32::from(!vm.snapshot.focus_enabled));
        global.set_focus_enabled_env_locked(vm.snapshot.env_overridden.contains("focus.enabled"));
        global.set_focus_enabled_on_restart(
            vm.snapshot
                .applies
                .get("focus.enabled")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        global.set_focus_chain_index(i32::from(!vm.snapshot.focus_chain));
        global.set_focus_chain_env_locked(vm.snapshot.env_overridden.contains("focus.chain"));
        global.set_focus_chain_on_restart(
            vm.snapshot
                .applies
                .get("focus.chain")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        global.set_focus_notify_index(i32::from(!vm.snapshot.focus_notify));
        global.set_focus_notify_env_locked(vm.snapshot.env_overridden.contains("focus.notify"));
        global.set_focus_notify_on_restart(
            vm.snapshot
                .applies
                .get("focus.notify")
                .is_some_and(|a| a.apply == crate::requests::SectionApply::OnRestart),
        );
        let rows: Vec<crate::SettingsFactRow> = vm
            .snapshot
            .facts
            .iter()
            .map(|fact| crate::SettingsFactRow {
                key: fact.key.into(),
                value: fact.value.clone().into(),
                on_restart: fact.apply == crate::requests::SectionApply::OnRestart,
                env_locked: vm.snapshot.env_overridden.contains(fact.key),
            })
            .collect();
        self.models.push_settings_facts(global, &rows);
    }

    /// Mirrors one edited field into the draft (the buffer precedent).
    /// Insert engagement is the D20 focus fact now — the field's
    /// focused publish masks the chain; this seam only mirrors.
    pub(crate) fn settings_field_edited(&mut self, field: usize, text: &str, window: &AppWindow) {
        let target = match field {
            1 => &mut self.settings.draft.server_url,
            2 => &mut self.settings.draft.username,
            3 => {
                // The password mirror: write-only — it never enters the
                // pushed model (the draft never renders it back).
                self.settings.draft.password.clear();
                self.settings.draft.password.push_str(text);
                self.push_settings(window);
                return;
            }
            4 => {
                // The scale field's staged buffer (round-3 D20a): the
                // draft becomes the rendered echo and the flavour's
                // input; nothing else writes the echo while the field
                // holds a draft (r4 D38: the drag no longer previews).
                self.settings.scale_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            // The general fields' staged buffers (the GUI settings
            // editor): the same grammar — draft per keystroke,
            // discarded on Return commit and on focus loss.
            5 => {
                self.settings.day_start_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            6 => {
                self.settings.tick_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            7 => {
                self.settings.before_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            8 => {
                self.settings.poll_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            9 => {
                self.settings.backoff_initial_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            10 => {
                self.settings.backoff_max_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            11 => {
                self.settings.due_soon_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_WORK_INDEX => {
                self.settings.focus_work_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_BREAK_INDEX => {
                self.settings.focus_break_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_LONG_BREAK_INDEX => {
                self.settings.focus_long_break_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FOCUS_EVERY_INDEX => {
                self.settings.focus_every_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            GeneralField::FILES_POLL_INDEX => {
                self.settings.files_poll_secs_draft = Some(text.to_owned());
                self.push_settings(window);
                return;
            }
            _ => return,
        };
        target.clear();
        target.push_str(text);
        self.push_settings(window);
    }

    /// The Sign-in seam (round-5 chunk 8: the Save ceremony with a
    /// derived name): mirror the final field texts (the commit
    /// callbacks carry them), then send the verify-first ceremony
    /// request with an empty name — the app derives it from the
    /// identity. The password clears off the draft after the send —
    /// write-only (D26b).
    pub(crate) fn settings_save(
        &mut self,
        window: &AppWindow,
        server: &str,
        username: &str,
        password: &str,
    ) {
        let draft = ConnectionDraft {
            server_url: server.to_owned(),
            username: username.to_owned(),
            password: password.to_owned(),
        };
        // The visible ceremony (r2 D34, F21): every click reacts. A
        // refused click is a *visible* typed refusal — never a silent
        // absorb, and never a routed request (the lane stays clean).
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if let Some(err) = settings_form::validate(&draft) {
            // The chunk-5 mapping renders the exact reason sentence
            // with zero new presentation code.
            let detail = match err {
                settings_form::DraftError::Invalid { field, kind } => SettingsError::Invalid {
                    field: map_form_field(field),
                    kind: map_form_kind(kind),
                },
            };
            self.set_settings_outcome(window, SettingsOutcome::Rejected, Some(detail));
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SaveConnection {
            profile: ConnectionProfile {
                name: String::new(),
                server_url: draft.server_url.clone(),
                username: draft.username.clone(),
                password: Secret(draft.password.clone()),
            },
            reply,
        };
        self.settings.draft.password.clear();
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Save,
            PendingReply::List(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// Sends one connections request whose reply is the fresh pair,
    /// with the busy ceremony on `(connector, remote)`.
    fn send_connections_op(
        &mut self,
        window: &AppWindow,
        request: UiRequest,
        reply_rx: oneshot::Receiver<Result<requests::ConnectionsSnapshot, SettingsError>>,
        kind: PendingKind,
        connector: &str,
        remote: Option<RemoteBoardId>,
    ) {
        let sent = self.settings.send(
            &self.requests,
            request,
            kind,
            PendingReply::List(reply_rx),
            connector,
        );
        if sent {
            self.settings.busy_remote = remote;
        }
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The account card's Remove seam (round-5 chunk 8; UX R-2 D-4,
    /// #787): the click ARMS — the card becomes the one armed subject
    /// (no lane state, no send); the Confirm button routes the exact
    /// `RemoveConnection`. A stale or non-card index is absorbed
    /// silently.
    pub(crate) fn settings_remove_item(&mut self, window: &AppWindow, item: usize) {
        self.settings_arm_item(item);
        self.push_settings(window);
    }

    /// The armed strip's arm step (UX R-2 D-4): records the addressed
    /// item as the one armed subject — arming touches no lane state,
    /// dispatches nothing. A stale index is absorbed silently; arming
    /// another item displaces the previous (one armed subject
    /// surface-wide).
    pub(crate) fn settings_arm_item(&mut self, item: usize) {
        let Some(addr) = self.settings.items.get(item).cloned() else {
            return;
        };
        self.settings.armed_item = Some(addr);
    }

    /// The armed strip's disarm (UX R-2 D-4): the Cancel buttons, a
    /// displaced arm, a stale confirm — clears the flag, dispatches
    /// nothing.
    pub(crate) fn settings_disarm_item(&mut self) {
        self.settings.armed_item = None;
    }

    /// The remove/disconnect confirm seam (UX R-2 D-4): matches the
    /// armed `ItemAddr` against the addressed item — the armed
    /// address types the verb (`RemoveConnection` for a card,
    /// `DetachConnector` for a bound remote row) — clears the flag
    /// BEFORE the send, then dispatches the exact existing request.
    /// A stale index (not the armed subject) disarms silently.
    pub(crate) fn settings_remove_confirmed(&mut self, window: &AppWindow, item: usize) {
        let Some(addr) = self.settings.items.get(item).cloned() else {
            self.settings_disarm_item();
            return;
        };
        let Some(armed) = self.settings.armed_item.take() else {
            return;
        };
        if armed != addr {
            return; // a stale confirm: already disarmed, nothing dispatches
        }
        match armed {
            ItemAddr::Account { connector } => self.settings_send_remove(window, &connector),
            ItemAddr::Remote {
                connector,
                remote,
                bound: Some(board),
            } => self.settings_send_detach(window, &connector, remote, board),
            _ => {}
        }
    }

    /// The Files roster's Stop-Sync confirm seam (UX R-2 D-4): the
    /// armed `FilesBoard` address types the verb; clears the flag
    /// BEFORE the send. A stale index disarms silently — the Move and
    /// Publish branches never arm (non-destructive, one-step).
    pub(crate) fn settings_files_action_confirmed(&mut self, window: &AppWindow, item: usize) {
        let Some(addr) = self.settings.items.get(item).cloned() else {
            self.settings_disarm_item();
            return;
        };
        let Some(armed) = self.settings.armed_item.take() else {
            return;
        };
        let ItemAddr::FilesBoard { board, connector } = armed.clone() else {
            return;
        };
        if armed != addr {
            return;
        }
        self.settings_stop_files_sync(window, board, &connector);
    }

    /// The exact `RemoveConnection` send (round-5 chunk 8's body,
    /// moved verbatim into the confirm path).
    fn settings_send_remove(&mut self, window: &AppWindow, connector: &str) {
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::RemoveConnection {
            name: connector.to_owned(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Remove,
            connector,
            None,
        );
    }

    /// The exact `DetachConnector` send (r2 D33's body, moved
    /// verbatim into the confirm path).
    fn settings_send_detach(
        &mut self,
        window: &AppWindow,
        connector: &str,
        remote: RemoteBoardId,
        board: BoardId,
    ) {
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::DetachConnector {
            board,
            connector: connector.to_owned(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Detach,
            connector,
            Some(remote),
        );
    }

    /// The exact `StopBoardFilesSync` send (round-7 D-7.3's Stop
    /// branch body, moved verbatim into the confirm path; the row
    /// marker set before the send).
    fn settings_stop_files_sync(&mut self, window: &AppWindow, board: BoardId, connector: &str) {
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::StopBoardFilesSync { board, reply };
        // The row marker is set BEFORE the send: `send_connections_op`
        // ends in the busy push that renders it.
        self.settings.busy_board = Some(board);
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::StopFilesSync,
            connector,
            None,
        );
    }

    /// The Deck row's Refresh seam (round-5 chunk 8, D-8.3): lists the
    /// connector's remote boards (the listing verifies the stored
    /// credentials). A stale or non-Deck index is absorbed silently.
    pub(crate) fn settings_refresh_item(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::Deck { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        // Explicit intent overrides the backoff (US-3 AC-2): the
        // entry clears before the send; failure re-arms it.
        self.settings.auto_list_backoff.remove(&connector);
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::ListRemoteBoards {
            connector: connector.clone(),
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::ListRemote,
            PendingReply::Remotes(connector.clone(), reply_rx),
            &connector,
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The Deck row's publish seam (round-6 D-6.9, #699): picking an
    /// unbound local board publishes it onto this connector's server.
    /// A stale item/option index is absorbed silently.
    pub(crate) fn settings_publish_picked(
        &mut self,
        window: &AppWindow,
        item: usize,
        option: usize,
    ) {
        let Some(ItemAddr::Deck { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        let Some(board) = self.settings.local_choices.get(option).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::PublishBoard {
            board: BoardId::from_uuid(board),
            connector: connector.clone(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Publish,
            &connector,
            None,
        );
    }

    /// The files roster's per-row action seam (round-7 D-7.3, W-3/D1):
    /// the row's one action, re-derived at the trust boundary — the
    /// binding's channel is read structurally from the engine state
    /// fresh (D-BF4; the widget never names the action), the lane-on
    /// fact re-derives from the snapshot exactly as the render pass
    /// computed it, and the D6 enabled rule (`files_on ∧ !archived`)
    /// gates the dispatch — a disabled or stale row's click is
    /// absorbed silently. The Deck branch keys on the binding's home
    /// connector (the files-sync-status plan US-10, D-11): own card →
    /// the Deck→Files move; a foreign card's rebind row needs no
    /// files lane (its gate is the archived filter alone).
    /// The undo-window Restore seam (the files-sync-status plan US-6,
    /// D-7): routes the exact `UndoBoardMove` — the ceremony's undo
    /// leg is the typed gate. The pending machine takes the move
    /// grammar (the row's busy dot rides `PendingKind::Move`).
    pub(crate) fn settings_files_board_restore(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::FilesBoard { board, connector }) =
            self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::UndoBoardMove { board, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Move,
            PendingReply::List(reply_rx),
            &connector,
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    pub(crate) fn settings_files_board_action(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::FilesBoard { board, connector }) =
            self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let files_on = self.settings.snapshot.files_sync_enabled
            && self.settings.snapshot.files_sync_anchor.as_deref() == Some(connector.as_str());
        // The binding's channel AND its home connector, re-derived at
        // the trust boundary: a Deck-bound board riding this card's
        // connector is the own-card switch; one riding another
        // connector is the rebind row (US-10, D-11 — needs no files
        // lane, so its gate is the archived filter alone).
        let bound = {
            let state = self.state.load();
            state
                .boards
                .get(&board)
                .filter(|row| !row.deleted && !row.archived)
                .and_then(|row| row.remote.as_ref())
                .map(|binding| {
                    (
                        crate::presentation::bound_channel_of(&binding.board),
                        binding.connector.as_str() == connector.as_str(),
                    )
                })
        };
        let channel = bound.map(|(channel, _)| channel);
        let deck_own = matches!(bound, Some((crate::requests::BoardChannel::Deck, true)));
        // The D6 enabled rule re-checked at the trust boundary: a
        // disabled posture (lane off, archived) or a stale row
        // (gone / re-bound mid-flight) absorbs the click silently —
        // nothing dispatches.
        let kind = match (channel, files_on) {
            // The destructive branch (UX R-2 D-4, #787): the click
            // ARMS — the row becomes the one armed subject; the
            // strip's Confirm routes the exact Stop request.
            (Some(crate::requests::BoardChannel::Files), true) => {
                self.settings_arm_item(item);
                self.push_settings(window);
                return;
            }
            (Some(crate::requests::BoardChannel::Deck), _) => PendingKind::Move,
            (None, true) => PendingKind::PublishFiles,
            _ => return,
        };
        let (reply, reply_rx) = oneshot::channel();
        let request = match kind {
            PendingKind::PublishFiles => UiRequest::PublishBoardToFiles { board, reply },
            // The move family, keyed by the binding's home (the fold's
            // own/action split re-derived): own card → the Deck→Files
            // ceremony reused verbatim (US-3); foreign card → the
            // Deck-to-Deck rebind (US-10), this card's connector the
            // target. Both ride `PendingKind::Move`'s busy grammar on
            // the target connector's Files row.
            PendingKind::Move => {
                if deck_own {
                    UiRequest::MoveBoardToFiles { board, reply }
                } else {
                    UiRequest::MoveBoardToDeck {
                        board,
                        target_connector: connector.clone(),
                        reply,
                    }
                }
            }
            _ => unreachable!("the roster action dispatches only its one-step verbs"),
        };
        // The row marker is set BEFORE the send: `send_connections_op`
        // ends in the busy push that renders it.
        if matches!(kind, PendingKind::PublishFiles) {
            self.settings.busy_board = Some(board);
        }
        self.send_connections_op(window, request, reply_rx, kind, &connector, None);
    }

    /// The remote row's picker seam (round-5 chunk 8, D-8.4): picking
    /// an unbound local board binds it to this remote explicitly. A
    /// stale item/option index, or a bound row, is absorbed silently.
    pub(crate) fn settings_remote_bind_picked(
        &mut self,
        window: &AppWindow,
        item: usize,
        option: usize,
    ) {
        let Some(ItemAddr::Remote {
            connector,
            remote,
            bound: None,
        }) = self.settings.items.get(item).cloned()
        else {
            return;
        };
        let Some(board) = self.settings.local_choices.get(option).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::AttachConnector {
            board: BoardId::from_uuid(board),
            connector: connector.clone(),
            remote,
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Attach,
            &connector,
            Some(remote),
        );
    }

    /// The remote row's Create seam (round-5 chunk 8, D-8.5): a new
    /// local board from the remote's title, bound to it. A stale index
    /// or a bound row is absorbed silently.
    pub(crate) fn settings_create_item(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::Remote {
            connector,
            remote,
            bound: None,
        }) = self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::CreateLocalBoard {
            connector: connector.clone(),
            remote,
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::CreateBind,
            &connector,
            Some(remote),
        );
    }

    /// The bound remote row's Disconnect seam (r2 D33's detach, moved
    /// onto the row by round-5 chunk 8; UX R-2 D-4, #787): the click
    /// ARMS — the row becomes the one armed subject; the Confirm
    /// button routes the exact `DetachConnector` + multi-board guard.
    /// A stale index or an unbound row is absorbed silently.
    pub(crate) fn settings_disconnect_item(&mut self, window: &AppWindow, item: usize) {
        self.settings_arm_item(item);
        self.push_settings(window);
    }

    /// A Files row's toggle seam (round-5 chunk 8 D-8.6; round-6
    /// D-6.6; commit-on-pick): 0 = On, 1 = Off. On a non-anchor card
    /// re-anchors to it and enables — one request carries both values
    /// (one user action, one overlay write); On the anchor card, or
    /// any Off, leaves the anchor key untouched. `files_on` is
    /// recomputed from the snapshot at the seam, never from the
    /// render-pass model. An env-locked row (either key) absorbs the
    /// pick (the app-side guard is the authority).
    pub(crate) fn settings_files_picked(&mut self, window: &AppWindow, item: usize, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        let Some(ItemAddr::Files { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        let env_locked = self
            .settings
            .snapshot
            .env_overridden
            .contains("files_sync.enabled")
            || self
                .settings
                .snapshot
                .env_overridden
                .contains("files_sync.anchor");
        if env_locked {
            // T-16 (#799): the doctrine's letter — never a silent
            // absorb. The pick mutates nothing (the lane stays clean)
            // but the refusal renders typed at the point of action.
            tracing::debug!("files sync pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FilesSyncEnabled,
                }),
            );
            return;
        }
        let files_on = self.settings.snapshot.files_sync_enabled
            && self.settings.snapshot.files_sync_anchor.as_deref() == Some(connector.as_str());
        let anchor = if enabled && !files_on {
            Some(connector.clone())
        } else {
            None
        };
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFilesSyncEnabled {
            enabled,
            anchor,
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FilesSyncEnabled),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The preset picker seam: the optimistic marker, then the request
    /// (the reply swaps the keymap live, D9).
    pub(crate) fn settings_preset_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(preset) = [
            crate::keymap::Preset::Standard,
            crate::keymap::Preset::Emacs,
            crate::keymap::Preset::Vi,
        ]
        .get(index)
        .copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetKeymapPreset { preset, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Preset,
            PendingReply::Keymap(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The scale commit core (r2 D29): live-apply first (the mount
    /// precedent), then persist over the lane; an `Err` reply reverts
    /// to `previous`. Both input paths (field Return, slider release)
    /// share it, so both clamp identically by construction.
    fn apply_scale_commit(&mut self, window: &AppWindow, scale: f64, previous: f64) {
        // The commit consumed the field's staged buffer (round-3
        // D20a): the canonical echo re-renders from the live value.
        self.settings.scale_draft = None;
        // The scroll anchor captures PRE-layout (r3 D20c): the row
        // offset and viewport offset are read before the write below
        // re-lays the content out.
        request_scale_anchor(window);
        #[allow(clippy::cast_possible_truncation)]
        window.global::<crate::Theme>().set_base_scale(scale as f32);
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetScale { scale, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Scale,
            PendingReply::Scale(reply_rx, previous),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The scale commit seam (field, Return-authoritative): live-apply
    /// first, then persist over the lane; an `Err` reply reverts (D9).
    pub(crate) fn settings_scale_committed(&mut self, window: &AppWindow, text: &str) {
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        // The typed refusal (round-3 D20a, recorded decision): the
        // same validator the flavour renders drives the status
        // detail, so the error line and the status line agree; the
        // draft is KEPT on failure — the typo stays visible under
        // its invalid flavour until focus loss or a good commit.
        if let Some(kind) = settings_form::scale_invalid_kind(text) {
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::Invalid {
                    field: crate::requests::Field::Scale,
                    kind: map_form_kind(kind),
                }),
            );
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let scale = text.trim().parse::<f64>().expect("validated parse");
        let previous = f64::from(window.global::<crate::Theme>().get_base_scale());
        self.apply_scale_commit(window, scale, previous);
    }

    /// The slider gesture's press (r2 D29): record the pre-gesture
    /// base-scale as the revert point. The pending gate is silent
    /// (the `settings_preset_picked` precedent).
    pub(crate) fn settings_scale_pressed(&mut self, window: &AppWindow) {
        if self.settings.pending.is_some() {
            return;
        }
        self.scale_drag = Some(f64::from(window.global::<crate::Theme>().get_base_scale()));
    }

    /// The scale field's focus-loss seam (round-3 D20a writer 7): a
    /// Return-less edit never applies — the staged draft dies and the
    /// push re-renders the echo from the live scale (the chunk-1
    /// focus-loss seam is the mechanism).
    pub(crate) fn settings_scale_focus_lost(&mut self, window: &AppWindow) {
        if self.settings.scale_draft.take().is_none() {
            return;
        }
        self.push_settings(window);
    }

    /// The general fields' commit seam (the GUI settings editor;
    /// Return-authoritative — the scale field's grammar): the mirror
    /// predicate refuses typed before the lane; a valid mint
    /// dispatches the field's typed intent; the reply echo re-renders
    /// the rows. Env-locked keys never dispatch — the row renders
    /// read-only, and this arm is the defence-in-depth refusal. The
    /// `initial ≤ max` ordering (plan D8) refuses at the seam against
    /// the committed counterpart, the error attributed to the
    /// offending field; the app-side gate re-proves the whole
    /// document authoritatively.
    // The minting match is one arm per field of the closed
    // GeneralField table — the arm count tracks the table, not the
    // seam's own complexity.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn settings_general_committed(
        &mut self,
        window: &AppWindow,
        field: usize,
        text: &str,
    ) {
        let Some(gf) = GeneralField::from_index(field) else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self.settings.snapshot.env_overridden.contains(gf.key()) {
            // T-16 (#799): the defence-in-depth arm renders the typed
            // refusal instead of absorbing (the doctrine: never a
            // silent absorb).
            tracing::debug!(
                key = gf.key(),
                "general commit on an env-locked row; refused typed"
            );
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: gf.lane_field(),
                }),
            );
            return;
        }
        let typed_refusal = |this: &mut Self, kind| {
            this.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::Invalid {
                    field: gf.lane_field(),
                    kind: map_form_kind(kind),
                }),
            );
        };
        if let Some(kind) = gf.invalid(text) {
            // The draft stays: the typo stays visible under its
            // invalid flavour until focus loss or a good commit (the
            // scale precedent).
            typed_refusal(self, kind);
            return;
        }
        let trimmed = text.trim();
        // The mirror's empty contract is Neutral (no flavour on an
        // untouched field), so an empty commit reaches here — and the
        // mints below `expect` a validated parse. `accepted` fires on
        // Return regardless of content, so an empty Return is a real
        // keyboard path: it refuses typed, never mints.
        if trimmed.is_empty() {
            typed_refusal(self, settings_form::InvalidKind::Empty);
            return;
        }
        if let Some(ceiling) = gf.ordering_ceiling(&self.settings.snapshot) {
            let crossing = match gf {
                GeneralField::BackoffInitial => trimmed
                    .parse::<u64>()
                    .is_ok_and(|initial| initial > ceiling),
                GeneralField::BackoffMax => trimmed.parse::<u64>().is_ok_and(|max| max < ceiling),
                _ => false,
            };
            if crossing {
                typed_refusal(self, settings_form::InvalidKind::OutOfRange);
                return;
            }
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = match gf {
            GeneralField::DayStart => UiRequest::SetDayStart {
                time: trimmed.parse().expect("validated day-start parse"),
                reply,
            },
            GeneralField::TickSecs => UiRequest::SetReminderTick {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::ReminderBefore => UiRequest::SetReminderBefore {
                before: taskboard_domain::ReminderOffset::parse_text_form(trimmed)
                    .expect("validated before parse"),
                reply,
            },
            GeneralField::PollInterval => UiRequest::SetPollInterval {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::BackoffInitial => UiRequest::SetBackoffInitial {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::BackoffMax => UiRequest::SetBackoffMax {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
            GeneralField::DueSoonDays => UiRequest::SetDueSoonDays {
                days: trimmed.parse().expect("validated days parse"),
                reply,
            },
            GeneralField::FocusWorkMins => UiRequest::SetFocusWorkMins {
                mins: trimmed.parse().expect("validated focus-minutes parse"),
                reply,
            },
            GeneralField::FocusBreakMins => UiRequest::SetFocusBreakMins {
                mins: trimmed.parse().expect("validated focus-minutes parse"),
                reply,
            },
            GeneralField::FocusLongBreakMins => UiRequest::SetFocusLongBreakMins {
                mins: trimmed.parse().expect("validated focus-minutes parse"),
                reply,
            },
            GeneralField::FocusLongBreakEvery => UiRequest::SetFocusLongBreakEvery {
                every: trimmed.parse().expect("validated focus-every parse"),
                reply,
            },
            GeneralField::FilesPollSecs => UiRequest::SetFilesSyncPollSecs {
                secs: trimmed.parse().expect("validated seconds parse"),
                reply,
            },
        };
        self.clear_general_draft(gf);
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(gf.lane_field()),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The reminders toggle's seam (commit-on-pick, the keymap
    /// preset grammar): index 0 = On, 1 = Off.
    pub(crate) fn settings_reminders_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("reminders.enabled")
        {
            // Choice commits are typed booleans — env-locked picks
            // refuse typed at the defence arm (the segments render
            // disabled; the row's source is named by the badge; the
            // T-16 refusal renders at the point of action).
            tracing::debug!("reminders pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::RemindersEnabled,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetRemindersEnabled { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::RemindersEnabled),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The focus mode toggle's seam (commit-on-pick, the reminders
    /// grammar): index 0 = On, 1 = Off.
    pub(crate) fn settings_focus_enabled_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.enabled")
        {
            tracing::debug!("focus enabled pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FocusEnabled,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFocusEnabled { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FocusEnabled),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The auto-chain toggle's seam (commit-on-pick): 0 = On, 1 = Off.
    pub(crate) fn settings_focus_chain_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.chain")
        {
            tracing::debug!("focus chain pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FocusChain,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFocusChain { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FocusChain),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The boundary-notification toggle's seam (commit-on-pick):
    /// 0 = On, 1 = Off.
    pub(crate) fn settings_focus_notify_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.notify")
        {
            tracing::debug!("focus notify pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::FocusNotify,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFocusNotify { enabled, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::FocusNotify),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The notifier's seam (commit-on-pick): 0 = auto, 1 = log,
    /// 2 = desktop.
    pub(crate) fn settings_sink_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(sink) = [
            crate::requests::ReminderSink::Auto,
            crate::requests::ReminderSink::Log,
            crate::requests::ReminderSink::Desktop,
        ]
        .get(index)
        .copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture — the
            // typed `RequestPending` detail rides the same lane as
            // every other refusal (never a silent generic).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("reminders.sink")
        {
            tracing::debug!("sink pick on an env-locked row; refused typed");
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::EnvLocked {
                    field: crate::requests::Field::ReminderSink,
                }),
            );
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetReminderSink { sink, reply };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::General(crate::requests::Field::ReminderSink),
            PendingReply::Snapshot(reply_rx),
            "",
        );
        self.set_settings_outcome(
            window,
            if sent {
                SettingsOutcome::Busy
            } else {
                SettingsOutcome::Failed
            },
            None,
        );
    }

    /// The general fields' focus-loss seam (the scale field's
    /// grammar): a Return-less edit never dispatches — the staged
    /// draft dies and the echo re-renders from the committed value.
    /// The Files row's poll cadence field's commit seam (#1083, W-6):
    /// the general fields' funnel verbatim, under the plan row's own
    /// seam address (`files-poll-secs-committed`) — one machine, the
    /// whole-snapshot lane's single writer (R-4).
    pub(crate) fn files_poll_secs_committed(
        &mut self,
        window: &AppWindow,
        field: usize,
        text: &str,
    ) {
        self.settings_general_committed(window, field, text);
    }

    pub(crate) fn settings_general_focus_lost(&mut self, window: &AppWindow, field: usize) {
        let Some(gf) = GeneralField::from_index(field) else {
            return;
        };
        let cleared = match gf {
            GeneralField::DayStart => self.settings.day_start_draft.take().is_some(),
            GeneralField::TickSecs => self.settings.tick_draft.take().is_some(),
            GeneralField::ReminderBefore => self.settings.before_draft.take().is_some(),
            GeneralField::PollInterval => self.settings.poll_draft.take().is_some(),
            GeneralField::BackoffInitial => self.settings.backoff_initial_draft.take().is_some(),
            GeneralField::BackoffMax => self.settings.backoff_max_draft.take().is_some(),
            GeneralField::DueSoonDays => self.settings.due_soon_draft.take().is_some(),
            GeneralField::FocusWorkMins => self.settings.focus_work_draft.take().is_some(),
            GeneralField::FocusBreakMins => self.settings.focus_break_draft.take().is_some(),
            GeneralField::FocusLongBreakMins => {
                self.settings.focus_long_break_draft.take().is_some()
            }
            GeneralField::FocusLongBreakEvery => self.settings.focus_every_draft.take().is_some(),
            GeneralField::FilesPollSecs => self.settings.files_poll_secs_draft.take().is_some(),
        };
        if cleared {
            self.push_settings(window);
        }
    }

    fn clear_general_draft(&mut self, gf: GeneralField) {
        match gf {
            GeneralField::DayStart => self.settings.day_start_draft = None,
            GeneralField::TickSecs => self.settings.tick_draft = None,
            GeneralField::ReminderBefore => self.settings.before_draft = None,
            GeneralField::PollInterval => self.settings.poll_draft = None,
            GeneralField::BackoffInitial => self.settings.backoff_initial_draft = None,
            GeneralField::BackoffMax => self.settings.backoff_max_draft = None,
            GeneralField::DueSoonDays => self.settings.due_soon_draft = None,
            GeneralField::FocusWorkMins => self.settings.focus_work_draft = None,
            GeneralField::FocusBreakMins => self.settings.focus_break_draft = None,
            GeneralField::FocusLongBreakMins => self.settings.focus_long_break_draft = None,
            GeneralField::FocusLongBreakEvery => self.settings.focus_every_draft = None,
            GeneralField::FilesPollSecs => self.settings.files_poll_secs_draft = None,
        }
    }

    /// The settings surface's mount (r3 D20a writer hygiene, plan
    /// §5.1 draft invariants): the scale field's staged buffer dies at
    /// the mount — a Return-less edit from a previous visit never
    /// survives the surface's unmount, and the re-push renders the
    /// canonical live echo. (The connection draft persists by design;
    /// only the scale buffer's lifetime is bound to its surface.)
    pub(crate) fn settings_view_mounted(&mut self, window: &AppWindow) {
        // The armed strip is surface-bound state (cross-surface
        // friction plan D-4, #836): the mount clears any identity a
        // missed close path left behind — the strip is only visible
        // while mounted (the scale_draft lifetime precedent, r3 D20a).
        self.settings_disarm_item();
        self.settings.scale_draft = None;
        // The general drafts share the scale buffer's lifetime: bound
        // to the surface, never surviving the unmount.
        self.settings.day_start_draft = None;
        self.settings.tick_draft = None;
        self.settings.before_draft = None;
        self.settings.poll_draft = None;
        self.settings.backoff_initial_draft = None;
        self.settings.backoff_max_draft = None;
        self.settings.due_soon_draft = None;
        self.settings.focus_work_draft = None;
        self.settings.focus_break_draft = None;
        self.settings.focus_long_break_draft = None;
        self.settings.focus_every_draft = None;
        // The mount auto-list (round-6 D-6.7): the ONLY call site —
        // the listing cost lands on the first settings visit, never
        // at boot.
        self.fire_auto_listings();
        self.push_settings(window);
    }

    /// The slider gesture's release (r2 D29, r4 D38): one gate, one
    /// commit — the ONLY mid-gesture seam the adapter observes (the
    /// drag itself lives inside the widget). An out-of-gate value
    /// reverts to the pre-gesture value with a seam-local Rejected;
    /// an in-gate value goes through the shared commit core with the
    /// pre-gesture value as the revert point, so a failed commit
    /// undoes the gesture too.
    pub(crate) fn settings_scale_released(&mut self, window: &AppWindow, value: f64) {
        let Some(previous) = self.scale_drag.take() else {
            return;
        };
        if self.settings.pending.is_some() {
            // T-38 (#821): the busy refusal names its posture (the
            // typed `RequestPending` detail, the other gates' shape).
            self.set_settings_outcome(
                window,
                SettingsOutcome::Rejected,
                Some(SettingsError::RequestPending),
            );
            return;
        }
        if !value.is_finite() || !(0.5..=3.0).contains(&value) {
            // The out-of-gate revert re-anchors too (r3 D20c).
            request_scale_anchor(window);
            #[allow(clippy::cast_possible_truncation)]
            window
                .global::<crate::Theme>()
                .set_base_scale(previous as f32);
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        self.apply_scale_commit(window, value, previous);
    }
}

/// The adapter seam scaffold (chunk 4.2, plan §5): splices the
/// compiler-forced `Rc::clone` + weak + upgrade-guard + borrow scaffold
/// in front of a seam body, so a seam's wiring is one line of shape
/// plus the body it exists for. Pure token splicing — slint keeps
/// receiving exactly the closure it always did; no generics, no
/// inference change, no added logic (index conversions, settles and
/// refreshes stay in bodies, never hidden in the macro).
///
/// Arms and their populations at conversion time (plan §5.3; no arm
/// exists for a shape with one occupant):
/// - default — weak mint + upgrade-guard + `borrow_mut()` (~137 sites);
/// - `[else <expr>]` — as default, the guard returns `<expr>` instead
///   (3 bool-returning seams);
/// - `[bare_mut]` — no window access, `borrow_mut()` only (11 mirror
///   seams);
/// - `[bare_ref]` — no window access, `borrow()` only (4 raw-send
///   seams);
/// - `[try]` — weak + guard + `try_borrow_mut()` absorb (the 2
///   teardown-echo seams).
///
/// The first binder group is caller-authored: macro hygiene keeps the
/// macro's own temporaries invisible to the body, so the body's `a` /
/// `window` names are spliced in as binders (write `_window` when the
/// body never touches the window). Closure args are the callback's own
/// parameters (`_` allowed).
///
/// Re-open trigger (plan §3.3): a future shape needing a sixth arm, or
/// per-site customization beyond the body tokens, re-adjudicates on
/// the same evidence form — do not grow arms ad hoc.
macro_rules! install_seam {
    (window: $window:ident . $on:ident, adapter: $adapter:expr;
     bind mut ($a:ident, $w:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        let seam_weak = $window.as_weak();
        $window.$on(move |$($arg)*| {
            let Some($w) = seam_weak.upgrade() else {
                return;
            };
            let mut $a = seam_adapter.borrow_mut();
            $body
        });
    }};
    // the non-mut spelling of the default arm: one occupant
    // (on_task_done_toggled — the guard-check drift site; its body
    // only reads through the borrow, and its original binding was
    // non-mut)
    (window: $window:ident . $on:ident, adapter: $adapter:expr;
     bind ($a:ident, $w:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        let seam_weak = $window.as_weak();
        $window.$on(move |$($arg)*| {
            let Some($w) = seam_weak.upgrade() else {
                return;
            };
            let $a = seam_adapter.borrow_mut();
            $body
        });
    }};
    (window: $window:ident . $on:ident, adapter: $adapter:expr; [else ($guard_return:expr)]
     bind mut ($a:ident, $w:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        let seam_weak = $window.as_weak();
        $window.$on(move |$($arg)*| {
            let Some($w) = seam_weak.upgrade() else {
                return $guard_return;
            };
            let mut $a = seam_adapter.borrow_mut();
            $body
        });
    }};
    (window: $window:ident . $on:ident, adapter: $adapter:expr; [bare_mut]
     bind mut ($a:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        $window.$on(move |$($arg)*| {
            let mut $a = seam_adapter.borrow_mut();
            $body
        });
    }};
    (window: $window:ident . $on:ident, adapter: $adapter:expr; [bare_ref]
     bind ($a:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        $window.$on(move |$($arg)*| {
            let $a = seam_adapter.borrow();
            $body
        });
    }};
    (window: $window:ident . $on:ident, adapter: $adapter:expr; [try]
     bind mut ($a:ident, $w:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        let seam_weak = $window.as_weak();
        $window.$on(move |$($arg)*| {
            let Some($w) = seam_weak.upgrade() else {
                return;
            };
            let Ok(mut $a) = seam_adapter.try_borrow_mut() else {
                return;
            };
            $body
        });
    }};
}

/// Installs the interception callbacks on a fresh window and returns
/// the shared adapter (also held by [`crate::UiHandle`] for the
/// chunk-3 focus seam).
#[allow(clippy::too_many_arguments)] // the mount hand-over, one bundle
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat for review
pub(crate) fn install(
    window: &AppWindow,
    keymap: Keymap,
    preset: crate::keymap::Preset,
    startup: StartupWarnings,
    commands: mpsc::Sender<StateCommand>,
    requests: mpsc::Sender<crate::requests::UiRequest>,
    stack_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    connectors: Vec<ConnectorInfo>,
    boot_offset: chrono::FixedOffset,
    kiosk: bool,
    frozen_now: Option<chrono::DateTime<chrono::Utc>>,
    rotation: Option<crate::view_state::KioskRotation>,
    general: crate::requests::SettingsSnapshot,
    export_defaults: crate::requests::ExportDialogDefaults,
    state: Arc<ArcSwap<AppState>>,
    status_timer: Rc<slint::Timer>,
    rotation_timer: Rc<slint::Timer>,
    delivery_health: tokio::sync::watch::Receiver<crate::presentation::DeliveryHealth>,
    files_sync: Option<tokio::sync::watch::Receiver<crate::presentation::FilesSyncChip>>,
) -> Rc<RefCell<Adapter>> {
    // The load path (round-1 D16): the uuid-keyed settings map
    // converts once here and seeds the fresh ViewState before the
    // first refresh — widths are applied before the first board
    // render. Unknown boards/stacks are kept (write-time pruning is
    // the app forwarder's job).
    let seeded = stack_widths
        .into_iter()
        .map(|(board, stacks)| {
            (
                BoardId::from_uuid(board),
                stacks
                    .into_iter()
                    .map(|(stack, px)| (StackId::from_uuid(stack), px))
                    .collect::<BTreeMap<StackId, u32>>(),
            )
        })
        .collect::<BTreeMap<BoardId, BTreeMap<StackId, u32>>>();
    // The rotation mount seed (K13): with a rotation the appliance
    // opens on the rotation's first declared mode; without it the
    // kiosk/Today split stands (the desktop-default split, chunk 10
    // D2 — the kiosk pin lands on Board).
    let start_view = rotation.as_ref().map_or(
        if kiosk {
            crate::view_state::ViewId::Board
        } else {
            crate::view_state::ViewId::Today
        },
        |rotation| rotation.active().view(),
    );
    let walk_flag = Rc::new(std::cell::Cell::new(false));
    // The settings mirror seeds from the state the hand-over carries
    // (the engine published its boot snapshot before any UI mounted)
    // and follows every later publish through `sync_settings`.
    let boot_settings = state.load().settings.clone();
    let adapter = Rc::new(RefCell::new(Adapter {
        keymap,
        boot_offset,
        day_start: (boot_settings.day_start_time, boot_offset),
        due_soon_horizon: boot_settings.due_soon_days.horizon(),
        kiosk,
        frozen_now,
        reminder_defaults: boot_settings.reminder_defaults(),
        focus: if kiosk {
            None
        } else {
            boot_settings.live_focus()
        },
        seconds_surfaces_active: false,
        shell: KeymapShell::default(),
        resolver: KeymapResolver::new(),
        view_state: {
            let mut vs = ViewState::new_starting(start_view);
            vs.seed_stack_widths(seeded);
            vs
        },
        state,
        commands,
        requests,
        menu_entries: Vec::new(),
        root_entries: Vec::new(),
        menu_chain: Vec::new(),
        menu_anchor: MenuAnchor::Hamburger,
        menu_kind: MenuKind::Main,
        menu_subject: MenuSubject::None,
        quick_bar_entries: Vec::new(),
        create_focus_arm: None,
        create_target: None,
        reminder_target: None,
        notification_cursor: None,
        focus_handover: FocusHandover::None,
        focus_origin: None,
        applying_outcomes: false,
        preset,
        editor_clipboard: None,
        editor_mark: None,
        editor_resolver: crate::keyscope::EditorResolver::new(),
        editor_cursor: 0,
        editor_cursor_seen: 0,
        setter_filter_armed: false,
        rule_offset_draft: String::new(),
        timer_when_draft: String::new(),
        setter_time: None,
        setter_time_text: String::from("--:--"),
        setter_day_offset: 0,
        setter_insist: false,
        setter_composed: None,
        setter_fire: None,
        board_target: None,
        rule_target: None,
        rule_add_when: None,
        timer_draft: None,
        timer_target: None,
        break_rule_draft: None,
        break_rule_target: None,
        break_rule_armed: None,
        break_rule_form_open: false,
        break_rule_threshold_draft: String::new(),
        reflection_active: None,
        reflection_armed: None,
        reflection_text_draft: String::new(),
        reflection_mood_draft: String::new(),
        reflection_energy_draft: String::new(),
        reflection_flags_add: BTreeSet::default(),
        reflection_flags_remove: BTreeSet::default(),
        track_entry_draft: None,
        track_entry_target: None,
        track_target: None,
        plan_target: None,
        habit_target: None,
        focused_habit: None,
        habit_undo: None,
        habit_move: None,
        habit_move_up: None,
        habit_move_down: None,
        task_move: None,
        menu_row_subject: None,
        habit_repair: None,
        habit_convert_target: None,
        pattern_draft: None,
        series_target: None,
        pause_draft: None,
        convert_target: None,
        recur_edit_target: None,
        recur_draft_rule: None,
        recur_pause_open: false,
        habit_pause_open: false,
        habit_pause_target: None,
        recur_confirm_armed: false,
        reconcile_preview: None,
        priority_open: None,
        label_form: None,
        label_delete_armed: None,
        effort_editor: None,
        walk_abort_armed: false,
        habit_convert_armed: None,
        habit_remove_armed: None,
        habit_restore_armed: None,
        label_restore_armed: None,
        entry_restore_armed: None,
        rules_clear_armed: false,
        habit_edit_draft: None,
        habit_create_open: false,
        habit_create_draft: None,
        habit_create_quota: String::new(),
        habit_quota_count: 1,
        habit_quota_draft: None,
        walk_reentry: Rc::clone(&walk_flag),
        walk_timer: slint::Timer::default(),
        flow_open: FlowLiveness::Flowless,
        tables: IdTables::default(),
        models: crate::models::PushModels::default(),
        width_drag: None,
        scale_drag: None,
        status_panel: StatusPanelVm::default(),
        settings: SettingsVm {
            rows: connectors,
            // The mount seed (r2 D27): the picker's initial selection
            // rides the VM — the first refresh's `push_settings` is
            // the one write path.
            preset,
            // The general snapshot (the GUI settings editor): the
            // hand-over's typed values, env set included (the reply
            // contract preserves it).
            snapshot: general,
            ..SettingsVm::default()
        },
        status_timer,
        status_fire: Rc::new(|| {}), // replaced below, before any fire can happen
        rotation,
        rotation_timer,
        rotation_fire: Rc::new(|| {}), // replaced below, before any fire can happen
        quit_requests: 0,
        last_window_geometry: None,
        last_derived: None,
        #[cfg(test)]
        derive_passes: 0,
        notice: None,
        reload_lane: crate::presentation::ConfigReloadPresentation::Quiet,
        journal_degraded: false,
        delivery_health,
        files_sync,
        export_defaults,
        export_vm: None,
        export_reply: None,
        export_settled: None,
        export_preview_reply: None,
        export_preview_settled: None,
        export_copy_reply: None,
        export_copy_settled: None,
        remediation_settled: None,
    }));
    // The status auto-clear's fire closure (r2 D28): weak handles
    // only — no Rc cycle. Armed by `set_settings_outcome` from the
    // first outcome on.
    {
        let adapter_weak = Rc::downgrade(&adapter);
        let window_weak = window.as_weak();
        adapter.borrow_mut().status_fire = Rc::new(move || {
            if let (Some(window), Some(adapter)) = (window_weak.upgrade(), adapter_weak.upgrade()) {
                adapter.borrow_mut().settings_status_expired(&window);
            }
        });
    }
    // The rotation dwell timer (K13): the fire closure upgrades the
    // window before borrowing the adapter (the repaint driver's
    // borrow shape — no RefCell double-borrow), flips, and re-arms
    // with the returned full dwell. Weak handles only — no Rc cycle.
    // Without a rotation nothing is armed (the desktop is structurally
    // inert; the mock clock fires the timer in the journeys).
    if adapter.borrow().rotation.is_some() {
        let adapter_weak = Rc::downgrade(&adapter);
        let window_weak = window.as_weak();
        adapter.borrow_mut().rotation_fire = Rc::new(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let Some(adapter) = adapter_weak.upgrade() else {
                return;
            };
            let dwell = adapter.borrow_mut().rotation_tick(&window);
            adapter.borrow_mut().rearm_rotation(dwell);
        });
        let dwell = adapter
            .borrow()
            .rotation
            .as_ref()
            .map(crate::view_state::KioskRotation::dwell)
            .unwrap_or_default();
        let fire = Rc::clone(&adapter.borrow().rotation_fire);
        adapter
            .borrow()
            .rotation_timer
            .start(slint::TimerMode::SingleShot, dwell, move || fire());
    }
    // Startup warnings are pure input (Q15): pushed once at mount.
    window
        .global::<UiStatus>()
        .set_startup_keymap_findings(i32::try_from(startup.keymap_findings).unwrap_or(i32::MAX));
    // The export row's mount gate (the J2 export plan W-2, R-2): the
    // kiosk hand-over's model-side absence — minted once at install.
    window
        .global::<crate::SettingsModel>()
        .set_export_affordance(!kiosk);
    // The D31 help-text constants: presentation constants, pushed once
    // at mount (they never vary at runtime).
    {
        use crate::settings_status as status;
        let global = window.global::<crate::SettingsModel>();
        global.set_help_server(status::HELP_SERVER.into());
        global.set_help_username(status::HELP_USERNAME.into());
        global.set_help_app_password(status::HELP_APP_PASSWORD.into());
        global.set_help_scale(status::HELP_SCALE.into());
        global.set_help_keymap(status::HELP_KEYMAP.into());
        global.set_help_rows(status::HELP_ROWS.into());
        // The general fields' help constants (the GUI settings editor).
        global.set_help_day_start(status::HELP_DAY_START.into());
        global.set_help_tick(status::HELP_TICK.into());
        global.set_help_before(status::HELP_BEFORE.into());
        global.set_help_poll(status::HELP_POLL.into());
        global.set_help_backoff(status::HELP_BACKOFF.into());
        global.set_help_due_soon(status::HELP_DUE_SOON.into());
        global.set_help_reminders(status::HELP_REMINDERS.into());
        global.set_help_sink(status::HELP_SINK.into());
        global.set_help_focus_enabled(status::HELP_FOCUS_ENABLED.into());
        global.set_help_focus_work(status::HELP_FOCUS_WORK_MINS.into());
        global.set_help_focus_break(status::HELP_FOCUS_BREAK_MINS.into());
        global.set_help_focus_chain(status::HELP_FOCUS_CHAIN.into());
        global.set_help_focus_long_break(status::HELP_FOCUS_LONG_BREAK_MINS.into());
        global.set_help_focus_every(status::HELP_FOCUS_LONG_BREAK_EVERY.into());
        global.set_help_focus_notify(status::HELP_FOCUS_NOTIFY.into());
        global.set_help_files_poll(status::HELP_FILES_POLL_SECS.into());
    }

    {
        let adapter = Rc::clone(&adapter);
        let walk_flag = Rc::clone(&walk_flag);
        let weak = window.as_weak();
        window.on_intercept_key(move |text, modifiers| {
            let Some(window) = weak.upgrade() else {
                return false;
            };
            // The control walk's re-entrancy guard (round-3 D25):
            // rejects the injected `Tab` before any borrow — the
            // runtime then consumes the stroke for traversal itself.
            // Checking the flag ahead of `borrow_mut` is what makes
            // the guard total: the injected event re-enters this
            // closure while the walk's own borrow is still held.
            if walk_flag.get() {
                return false;
            }
            // The dropdown ladders' decline retired into the scope
            // arms (round-5 chunk 7, D-KS4): the closure collapses to
            // the re-entrancy guard plus `intercept` — the arms
            // evaluate the same facts globals inside `intercept`,
            // after the monopoly and before the resolver.
            adapter.borrow_mut().intercept(&window, &text, modifiers)
        });
    }
    {
        install_seam!(window: window.on_menu_entry_invoked, adapter: &adapter;
        bind mut (a, window), args (index: i32), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a.invoke_root_entry(&window, index);

        });
    }
    {
        // The quick bar's invocation seam (UX-R8 chunk-5 D-2, US-1):
        // invoke_entry's posture — a disabled entry arms the T-19
        // disabled notice (its registry description), an enabled one
        // closes the menu first and routes (round-1 D3). Stale
        // indices against an older open's vec are absorbed.
        install_seam!(window: window.on_menu_quick_bar_invoked, adapter: &adapter;
        bind mut (a, window), args (index: i32), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let Some(entry) = a.quick_bar_entries.get(index).cloned() else {
                return;
            };
            if !entry.enabled {
                let description = REGISTRY
                    .iter()
                    .find(|row| row.id == entry.action)
                    .map_or("", |row| row.description);
                a.arm_disabled_menu_notice(description);
                a.push_notice(&window);
                return;
            }
            a.apply_close_menu(&window);
            a.route(&window, entry.action);

        });
    }
    {
        install_seam!(window: window.on_menu_submenu_entry_invoked, adapter: &adapter;
        bind mut (a, window), args (index: i32), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a.invoke_entry(&window, index);

        });
    }
    {
        install_seam!(window: window.on_menu_level_back, adapter: &adapter;
        bind mut (a, window), args (), {            a.apply_effect(&window, UiEffect::MenuBack);

        });
    }
    {
        // The D17 width push: the window reports logical widths, the
        // unscaled Rust mapping lands in the `Responsive` global
        // (widths `<= 0` are ignored inside the mapping — the null
        // platform reports 0×0).
        install_seam!(window: window.on_window_width_changed, adapter: &adapter;
        bind mut (a, window), args (width: f32), {            let breakpoint =
                crate::models::to_slint_breakpoint(crate::responsive::breakpoint_for_width(width));
            window
                .global::<crate::Responsive>()
                .set_current_breakpoint(breakpoint);
            // The explicit-width row re-resolves on every window width
            // change (r4 D5): the pushed numbers go stale the moment
            // the window moves a pixel, and `refresh()`'s `Arc::ptr_eq`
            // guard cannot see width changes — the input lives outside
            // the state Arc, exactly the forced set's charter. One
            // forced diff pass per width event during a live window
            // drag; the diff carries only `width_px` scalars (the
            // kiosk's window never resizes in service).
            a.refresh_forced(&window);

        });
    }
    install_view_seams(window, &adapter);
    adapter.borrow().sync(window);
    adapter
}

/// The view dispatch seams (D22): every index-carrying callback
/// resolves against the id tables of the last refresh pass (D2) and
/// lands in the view-state machine or the router — never in ad-hoc
/// logic. Each handler ends in a refresh so the models carry the new
/// state (the adapter applies `view_state` outcomes verbatim, D9).
fn install_view_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_toolbar_seam(window, adapter);
    install_status_panel_seam(window, adapter);
    install_menu_dismiss_seam(window, adapter);
    install_undo_seams(window, adapter);
    install_board_bar_seam(window, adapter);
    install_board_color_seams(window, adapter);
    install_board_bar_archived_seam(window, adapter);
    install_delete_strip_seams(window, adapter);
    install_stack_create_seams(window, adapter);
    install_board_seams(window, adapter);
    install_stack_seams(window, adapter);
    install_stack_done_seam(window, adapter);
    install_today_row_seams(window, adapter);
    install_task_seam(window, adapter);
    install_picker_grid_seam(window, adapter);
    install_due_seam(window, adapter);
    install_task_back_seam(window, adapter);
    install_settings_back_seam(window, adapter);
    install_board_context_seam(window, adapter);
    install_stack_context_seam(window, adapter);
    install_object_context_seams(window, adapter);
    install_stack_width_seams(window, adapter);
    install_drag_seams(window, adapter);
    install_banner_seams(window, adapter);
    install_notification_seams(window, adapter);
    install_timer_seams(window, adapter);
    install_tracking_seams(window, adapter);
    install_focus_seams(window, adapter);
    install_review_walk_seams(window, adapter);
    install_review_ledger_seams(window, adapter);
    install_habit_seams(window, adapter);
    install_recur_seams(window, adapter);
    install_recur_editor_seams(window, adapter);
    install_export_seam(window, adapter);
    install_habit_editor_seams(window, adapter);
    install_settings_seams(window, adapter);
}

/// The recur row's + the habits trio's seams (issue #506, plan
/// D-7/D-8): every new interactive element rides the blanket rule —
/// the hover tip is the button's label, the walk stop is the widget's
/// own focus machinery, and the dispatch goes through the action
/// layer (`route`) from a one-shot target the seam resolved from the
/// same row source the sections render.
#[allow(clippy::too_many_lines)] // one seam table; splitting hides the trio
fn install_recur_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The make-up valve: the exact `(habit, window, count)` from the
    // row source — what you click is what increments.
    install_seam!(window: window.on_habits_repair_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    let state = a.state.load();
    let Some(habit_row) = state.habits.habits.get(&habit) else {
        return;
    };
    let Some((target_window, count)) = crate::presentation::habit_repair_target(
        habit_row,
        &state.habits.log,
        a.day_ctx().now,
        a.calendar(),
    ) else {
        return; // the affordance armed against stale state
    };
    a.habit_repair = Some((habit, target_window, count));
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions.
    a.focused_habit = Some(habit);
    drop(state);
    a.route(&window, ActionId::HabitRepair);

    });

    // The habit row's focus fact (cross-surface friction plan D-2,
    // #832; reworked by #1200 chunk A1 D2): the row activation is the
    // task pointer path's shape — the pointer conflict settles first
    // (the `today_row_activated` discipline), the id resolves through
    // the tables, the walk cursor seats via
    // `ViewState::activate_habit_row` (the exclusivity law's landing
    // write; the emphasis ring repaints through the outcomes), and
    // the menu verbs' subject mints as before. Click ≡ key in both
    // directions: the keyboard truth composes with the pointer truth
    // (one selection cursor, never two). Stale index absorbed (the
    // notification-row guard shape); no dispatch, no pass — selection
    // only (habits have no click-to-open verb, H4 reserved).
    install_seam!(window: window.on_habits_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let _ = &window; // the fact mutates no window bit
        settle_pointer_conflict(&mut a, &window);
        if let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    {
        a.focused_habit = Some(habit);
        let outcomes = a.view_state.activate_habit_row(habit);
        a.apply_outcomes(&window, &outcomes);
    }

    });

    // The habit row's double-click editor opener (D3a W-4, #1201):
    // the task pointer path's activation shape — the conflict settles,
    // the walk cursor seats (single-click's selection truth is the
    // prefix of the double-click's, US-2/S3 composes), then the
    // `HabitEdit` opener routes over the minted subject. The kiosk
    // gate is the landed dispatch-time refusal (the UiEffect arm),
    // never a second seam-side copy. Stale index absorbed.
    install_seam!(window: window.on_habits_edit_asked, adapter: adapter; // slint: habits-edit-asked
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    a.focused_habit = Some(habit);
    let outcomes = a.view_state.activate_habit_row(habit);
    a.apply_outcomes(&window, &outcomes);
    a.route(&window, ActionId::HabitEdit);

    });

    // The habit list's gap drop (chunk 8 D5, US-4/AC-1 — the #918
    // board pattern over the flat list): the delegate-resolved gap
    // index is the one-shot verbatim (#1198 chunk A1 D1 — the UI
    // mints target indices only; the engine clamps and resolves the
    // orders) and routes `HabitMove` — the same dispatch seam the
    // menu rows speak. A stale index or gap is absorbed (the pointer
    // seams' posture); kiosk absorbs silently (the editing verb's
    // HAB-14 gate — the slint side never arms, the adapter doubles
    // the gate).
    // slint: habits-drop-asked
    install_seam!(window: window.on_habits_drop_asked, adapter: adapter;
    bind mut (a, window), args (row, gap), {        let _ = &window;
        if a.kiosk {
            return;
        }
        let (Ok(row), Ok(gap)) = (usize::try_from(row), usize::try_from(gap)) else {
            return;
        };
        let Some(&habit) = a.tables.habits.get(row) else {
            return;
        };
        let state = a.state.load_full();
        let Some(habit_row) = state.habits.habits.get(&habit) else {
            return;
        };
        if !habit_row.is_live() {
            return;
        }
        drop(state);
        // A gap past the end is the append slot — the engine clamps.
        a.habit_move = Some((habit, gap));
        a.route(&window, ActionId::HabitMove);
    });

    // The habit row's context menu (H2 D4/B1): the right-click seam —
    // focus mints inside the opener (the row activation's discipline),
    // the curated page opens anchored at the pointer. Stale index
    // absorbed (the notification-row guard shape).
    install_seam!(window: window.on_habits_context_requested, adapter: adapter; // slint callback: habits-context-requested (the habit context menu's right-click seam, H2 W-2)
    bind mut (a, window), args (index, x, y), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    a.open_habit_context_menu(&window, x, y, index);

    });

    // The recur row: pause open (view intent), the editor's typed
    // commit (the trust boundary parses the date strictly), resume,
    // and the conversion's arm/confirm/cancel.
    install_seam!(window: window.on_task_recur_pause_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::SeriesPause);

    });
    install_seam!(window: window.on_task_recur_pause_accepted, adapter: adapter;
    bind mut (a, window), args (text), {        // The typed date is the trust boundary: strict `%Y-%m-%d`, no
    // leniency — an unparseable draft dispatches nothing.
    let Ok(date) = chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d") else {
        return;
    };
    a.pause_draft = Some(date);
    a.recur_pause_open = false;
    a.route(&window, ActionId::SeriesPauseCommit);

    });
    install_seam!(window: window.on_task_recur_pause_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_pause_open = false;
    a.pause_draft = None;
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_task_recur_resume_clicked, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    a.series_target = Some(task);
    a.route(&window, ActionId::SeriesResume);

    });
    install_seam!(window: window.on_task_recur_convert_asked, adapter: adapter;
    // The arming is the shared helper (cross-surface friction plan
    // D-2, #832) — identicality with the menu-routed effect arm.
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    if !a.arm_series_convert(task) {
        return;
    }
    a.route(&window, ActionId::SeriesConvertArm);

    });
    install_seam!(window: window.on_task_recur_convert_confirmed, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    // The quota draft re-derives from the current rule (the arm's
    // draft may be stale against a mid-confirm pattern edit).
    let pattern = {
        let state = a.state.load();
        let Some(rule) = state.tasks.get(&task).and_then(|t| t.recur.as_ref()) else {
            return;
        };
        crate::presentation::convert_target_default(&rule.pattern)
    };
    a.series_target = Some(task);
    a.convert_target = Some(pattern);
    a.recur_confirm_armed = false;
    a.route(&window, ActionId::SeriesConvertToHabit);

    });
    install_seam!(window: window.on_task_recur_convert_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_confirm_armed = false;
    a.series_target = None;
    a.convert_target = None;
    a.refresh_forced(&window);

    });
}

/// The recurrence editor's seams (round-7 D-7.1): the affordances'
/// convergence routes the opener action over the viewed subject; the
/// field mirrors write the level's buffer the moment a keystroke or
/// pick happens (the adapter owns the buffer — one truth); the commit
/// funnel re-mints the whole draft and dispatches nothing on an
/// invalid parse (the trust boundary never trusts the widget); the
/// detach strip routes its two-step through the action layer.
/// The export dialog's seams (the J2 export plan W-1..W-5): the
/// pickers'/fields' mirrors, the Run dispatch, the close (Escape's
/// action rides the router — click ≡ key), and the Settings row's
/// opener (W-2 — the same mount arm the action routes).
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat
fn install_export_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // W-1's open callback (the window-level mount front): the same
    // action route the menu item drives (click ≡ key).
    install_seam!(window: window.on_export_dialog_open, adapter: adapter; // slint: export-dialog-open / export-dialog-close (the W-1 pair; the close seam below carries the dismissal half)
    bind mut (a, window), args (), {        a.route(&window, ActionId::ExportDialogOpen);

    });
    // W-2: the Settings view's Export… row (the same mount arm the
    // action-menu item routes — D-1's two entry points converge).
    install_seam!(window: window.on_export_settings_open, adapter: adapter; // slint: export-settings-open
    bind mut (a, window), args (), {        a.route(&window, ActionId::ExportDialogOpen);

    });
    // W-1's close: the host's Close button / outside click land here;
    // the route is the Escape rung's exact dispatch.
    install_seam!(window: window.on_export_dialog_close, adapter: adapter; // slint: export-dialog-close
    bind mut (a, window), args (), {        a.route(&window, ActionId::ExportDialogClose);

    });
    // W-3: the family/format/layout pickers — the typed index
    // contract mapped at the adapter (never free text).
    install_seam!(window: window.on_export_spec_picked, adapter: adapter; // slint: export-spec-picked
    bind mut (a, window), args (control, index), {        a.export_spec_picked(control, index);
        a.finish_mutation(&window);

    });
    // W-3: the filter/destination fields' two-way mirror.
    install_seam!(window: window.on_export_field_edited, adapter: adapter; // slint: export-field-edited
    bind mut (a, window), args (field, text), {        a.export_field_edited(field, &text);
        a.finish_mutation(&window);

    });
    // W-3/W-2 (D-2): the open-after-export checkbox.
    install_seam!(window: window.on_export_open_after_toggled, adapter: adapter; // slint: export-open-after-toggled
    bind mut (a, window), args (), {        a.export_open_after_toggled();
        a.finish_mutation(&window);

    });
    // W-4: the Run dispatch (the typed oneshot over the request lane).
    install_seam!(window: window.on_export_run, adapter: adapter; // slint: export-run
    bind mut (a, window), args (), {        a.export_run_clicked(&window);

    });
    // W-5: the settled outcome's display fold — the reply drain
    // invokes the callback outside the adapter borrow (the
    // re-entrancy law); the handler folds the stashed outcome into
    // the VM.
    install_seam!(window: window.on_export_outcome_shown, adapter: adapter; // slint: export-outcome-shown
    bind mut (a, window), args (), {        a.export_outcome_fold(&window);

    });
    // The preview/clipboard lane (the D2 preview plan W-1..W-4): the
    // dispatch seams, the display folds, and the D-5 family-gating
    // fold.
    install_seam!(window: window.on_export_preview, adapter: adapter; // slint: export_preview
    bind mut (a, window), args (), {        a.export_preview_clicked(&window);

    });
    install_seam!(window: window.on_export_clipboard_copy, adapter: adapter; // slint: export_clipboard_copy
    bind mut (a, window), args (), {        a.export_copy_clicked(&window);

    });
    install_seam!(window: window.on_export_preview_shown, adapter: adapter; // slint: export_preview_shown
    bind mut (a, window), args (), {        a.export_preview_fold(&window);
        a.export_copy_fold(&window);

    });
    install_seam!(window: window.on_export_preview_gated, adapter: adapter; // slint: export_preview_gated
    bind mut (a, window), args (), {        a.export_preview_gated_fold(&window);

    });
}

/// The recurrence editor's seams (round-7 D-7.1): the pattern/bound
/// picks, the field mirrors (invalid parse — the trust boundary
/// never trusts the widget), and the detach strip routes its
/// two-step through the action layer.
#[allow(clippy::too_many_lines)] // one seam table; splitting hides the editor
fn install_recur_editor_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    use crate::level_state::{MonthlyMode, RecurBoundKind, RecurPatternKind};

    // The affordances' convergence: the add row, the recur row's edit
    // button, and the Ctrl+R binding all land here (click ≡ key).
    install_seam!(window: window.on_series_edit_asked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::SeriesEdit);

    });

    // The pattern/bound/mode picks: the pushed index contracts
    // (0/1/2, 0/1) map to the draft kinds; an out-of-range index is a
    // stale widget degrading to a no-op.
    install_seam!(window: window.on_recur_pattern_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let kind = match index {
            0 => RecurPatternKind::Daily,
            1 => RecurPatternKind::Weekly,
            2 => RecurPatternKind::Monthly,
            _ => return, // a stale index degrades to a no-op
        };
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.pattern = kind;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_mode_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let mode = match index {
            0 => MonthlyMode::OnDay,
            1 => MonthlyMode::NthWeekday,
            _ => return, // a stale index degrades to a no-op
        };
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_mode = mode;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_bound_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let bound = match index {
            0 => RecurBoundKind::Never,
            1 => RecurBoundKind::Until,
            2 => RecurBoundKind::Count,
            _ => return, // a stale index degrades to a no-op
        };
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.bound = bound;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_weekday_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(slot) = usize::try_from(index).ok() else {
            return;
        };
        if slot > 6 {
            return; // a stale index degrades to a no-op
        }
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_nth_weekday = crate::presentation::weekday_of_slot(slot);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_weekday_toggled, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(slot) = usize::try_from(index).ok() else {
            return;
        };
        if slot > 6 {
            return; // a stale index degrades to a no-op
        }
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.weekdays[slot] = !editor.weekdays[slot];
        }
        a.refresh_forced(&window);

    });

    // The text mirrors: the typed text lands in the buffer verbatim
    // (no trim — the parsers are the funnel's business).
    install_seam!(window: window.on_recur_every_days_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.every_days = text.to_string();
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_weekly_interval_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.weekly_interval = text.to_string();
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_day_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_day = text.to_string();
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_nth_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_nth = text.to_string();
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_until_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.until_text = text.to_string();
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_count_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.count = text.to_string();
        }
        a.refresh_forced(&window);

    });

    // The commit funnel (plan D-C1): re-mint the whole buffer; an
    // invalid draft is a silent no-op (the button does not even mount
    // — the seam re-checks, the trust boundary never trusts the
    // widget); the done-subject re-check is the assembly
    // precondition's second gate.
    install_seam!(window: window.on_recur_commit_clicked, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a
            .view_state
            .subject_task()
            .filter(|task| a.state.load().tasks.get(task).is_some_and(taskboard_domain::Task::is_open))
        else {
            return;
        };
        let draft = a.view_state.recur_editor().cloned();
        let Some(draft) = draft else {
            return;
        };
        let now = a.day_ctx().now;
        let day = a.calendar();
        let head = a.state.load().tasks.get(&task).cloned();
        let Some(head) = head else {
            return;
        };
        let Ok(rule) = crate::presentation::assemble_recur_draft(&draft, &head, day, now) else {
            return; // an invalid draft dispatches nothing
        };
        a.recur_edit_target = Some(task);
        a.recur_draft_rule = Some(rule);
        a.route(&window, ActionId::SeriesEditCommit);
        // The commit's consume is the flow's end — the level drops
        // with its buffer, the one-shots were taken by the route.
        let outcomes = a.view_state.end_edit_recur();
        a.apply_outcomes(&window, &outcomes);

    });

    // The clean close: the cancel button, the scrim click, and
    // Escape's law all land here — the draft dies with the level,
    // nothing dispatches.
    install_seam!(window: window.on_recur_editor_cancel_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_edit_target = None;
        a.recur_draft_rule = None;
        let outcomes = a.view_state.end_edit_recur();
        a.apply_outcomes(&window, &outcomes);

    });

    // The detach strip's two-step (plan D-C1): the arm/cancel route
    // the flag effects; the confirm routes the detach command over
    // the opener's one-shot target and ends the flow.
    install_seam!(window: window.on_recur_detach_arm_clicked, adapter: adapter;
    bind mut (a, window), args (), {        // route() takes every one-shot; the strip's steps re-arm
        // the target from the flow (the convert seams' law) so the
        // confirm still routes.
        a.recur_edit_target = a.view_state.task_of_edit_recur();
        a.route(&window, ActionId::SeriesEditDetachArm);

    });
    install_seam!(window: window.on_recur_detach_cancel_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_edit_target = a.view_state.task_of_edit_recur();
        a.route(&window, ActionId::SeriesEditDetachCancel);

    });
    install_seam!(window: window.on_recur_detach_confirm_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_edit_target = a.view_state.task_of_edit_recur();
        a.route(&window, ActionId::SeriesEditDetach);
        // The detach's consume is the flow's end (the commit's shape).
        let outcomes = a.view_state.end_edit_recur();
        a.apply_outcomes(&window, &outcomes);

    });
}

/// The −1 recovery's one-shot target (UX R-2 D-3, #786): the row's
/// current window and count from the same row source the section
/// renders — what you click is what decrements. `None` when count 0
/// (the route's typed `NoTarget`) or against stale state.
fn habit_undo_target(
    adapter: &Adapter,
    habit: taskboard_domain::HabitId,
) -> Option<(
    taskboard_domain::HabitId,
    taskboard_domain::WindowStart,
    u32,
)> {
    let state = adapter.state.load();
    let habit_row = state.habits.habits.get(&habit)?;
    let now = adapter.day_ctx().now;
    let window = taskboard_domain::WindowStart::of_period(
        habit_row.target.kind(),
        adapter.calendar().today(now),
    );
    let count = state
        .habits
        .log
        .get(&taskboard_domain::HabitLogKey { habit, window })
        .map_or(0, |e| e.count);
    (count >= 1).then_some((habit, window, count))
}

/// The habits section's seams (DR-6/6, D-P13; H2 reworked): the +1
/// resolves the `IdTables.habits` index (stale index → no-op, the
/// notification-row guard shape) and routes `habit.log`; the −1
/// resolves the row's current window (the `habit_undo_target` above)
/// and routes `habit.unlog`; the right-click opens the habit context
/// menu (the reorder pair and the conversion live there now — the
/// slots minted at menu open with the same floor-midpoint law, the
/// UI computes midpoints, the planner stamps).
#[allow(clippy::too_many_lines)] // one seam block per wired callback
fn install_habit_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_habits_log_clicked, adapter: adapter; // slint callback: habits-log-clicked (the done slot ≡ strip +1 shared seam, H2 W-1)
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.habit_target = Some(habit);
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions.
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitLog);

    });

    // The −1 recovery (UX R-2 D-3, #786): the row's current window
    // and count from the same row source the section renders — what
    // you click is what decrements. Count 0 sets no target: the
    // route's typed `NoTarget` (nothing dispatches, no negative log
    // exists). Never quota-enforced (D-P14 symmetrically).
    install_seam!(window: window.on_habits_undo_log_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    a.habit_undo = habit_undo_target(&a, habit);
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions.
    a.focused_habit = Some(habit);
    if a.habit_undo.is_some() {
        a.route(&window, ActionId::HabitUndoLog);
    }

    });

    // The row seam's reorder pair retires with the row's vertical
    // button stack (H2 D3/D5): reorder lives in the habit context
    // menu now (the `habit.move-up`/`habit.move-down` ids, slots
    // minted at menu open by `habit_menu_entries`); the pointer
    // reorder parity is H3's drag/keys replacement.

    // The create flow (round-7 D-7.2): the header ask routes the
    // toggle intent; the quota keystrokes ride the per-keystroke
    // validity pass; the confirm seam is the trust boundary (board
    // resolved fresh at commit, the quota through the mint); the
    // cancel hits the shared disarm funnel.
    install_seam!(window: window.on_habits_create_asked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::HabitCreate);

    });
    // The HF1 skip/pause/resume seams (W-1/W-3): the row affordances
    // resolve the `IdTables.habits` index (stale index → no-op, the
    // notification-row guard shape); the pause editor's commit is the
    // trust boundary (strict %Y-%m-%d, no leniency — the recur-pause
    // seam's convention); cancel writes nothing (the destructive-role
    // dismissal law).
    install_seam!(window: window.on_habits_skip_asked, adapter: adapter; // slint: habits-skip-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.habit_target = Some(habit);
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitSkip);

    });
    install_seam!(window: window.on_habits_pause_asked, adapter: adapter; // slint: habits-pause-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.focused_habit = Some(habit);
    a.habit_pause_target = Some(habit);
    a.habit_pause_open = true;
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_habits_resume_asked, adapter: adapter; // slint: habits-resume-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.focused_habit = Some(habit);
    a.habit_pause_target = Some(habit);
    a.route(&window, ActionId::HabitResume);

    });
    install_seam!(window: window.on_habits_pause_until_picked, adapter: adapter; // slint: habits-pause-until-picked
    bind mut (a, window), args (text), {        let Ok(date) =
        chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
    else {
        return; // an unparseable draft dispatches nothing
    };
    // The draft gate (US-2/S2's escape leg; the quota-validity
    // posture): a through-date before today covers nothing and reads
    // as a stale badge — the draft is refused here (typed notice, the
    // editor stays open) AND the planner still validates
    // (`InvalidHabitPause` remains authoritative across a rollover).
    if date < a.calendar().today(a.day_ctx().now) {
        a.notice = Some(crate::adapter::Notice {
            text: crate::presentation::habit_pause_invalid_phrase(),
            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
        });
        a.push_notice(&window);
        return;
    }
    a.pause_draft = Some(date);
    a.habit_pause_open = false;
    a.route(&window, ActionId::HabitPauseCommit);

    });
    install_seam!(window: window.on_habits_pause_cancelled, adapter: adapter; // slint: habits-pause-cancelled
    bind mut (a, window), args (), {        a.disarm_habit_pause(&window);

    });
    install_seam!(window: window.on_habits_create_quota_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.habit_create_quota = text.to_string();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_habits_create_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.disarm_habit_create(&window);

    });
    install_seam!(window: window.on_habits_create_confirmed, adapter: adapter; [else ( false)]
    bind mut (a, window), args (title, quota_text, target_index), {        // The board resolves fresh at commit — never cached at arm
    // (what you see is what the habit lands on) and ahead of every
    // field refusal: a board that went away under the armed form
    // disarms it silently, whatever the fields hold (the stale-arm
    // law owns the seam's first word).
    let Some(board) = a.view_state.board() else {
        a.disarm_habit_create(&window);
        return false;
    };
    // A whitespace-empty title dispatches nothing (D-P14): the
    // raw text is never rewritten, an empty one never reaches the
    // engine.
    let title = title.trim();
    if title.is_empty() {
        return false;
    }
    // The quota is the trust boundary's mint: zero, junk, and
    // overflow all refuse structurally — an absorbed miss keeps the
    // text (the rule-add seam's return convention). Empty text is
    // the field's default: it commits as 1.
    let Ok(quota_count) = crate::presentation::parse_habit_quota(&quota_text) else {
        return false;
    };
    // The target index is structurally bounded by the
    // SegmentedControl, but the seam is total: out of range refuses.
    let target = match target_index {
        0 => taskboard_domain::HabitTarget::PerDay(quota_count),
        1 => taskboard_domain::HabitTarget::PerWeek(quota_count),
        2 => taskboard_domain::HabitTarget::PerMonth(quota_count),
        _ => return false,
    };
    a.habit_create_draft = Some(crate::router::HabitCreateDraft {
        board,
        title: title.to_string(),
        target,
    });
    // The form closes on commit (the timers form's shape); the
    // routed dispatch consumes the one-shot draft. The property is
    // set here — `route`'s trailing sync is not a derive pass (the
    // convert strip's direct-set precedent).
    a.habit_create_open = false;
    a.habit_create_quota.clear();
    window.set_habits_create_open(false);
    a.route(&window, ActionId::HabitCreateCommit);
    true

    });

    // The inline quota edit's seams (W-5, #961): the phrase click
    // resolves the row and routes the same opener effect the key
    // routes (click ≡ key — one arming site); the stepper rungs edit
    // the adapter-held draft (floor 0 — the typed invalid draft); the
    // commit mints the one-shot at the trust boundary (the
    // `NonZeroCount` mint over the habit's CURRENT kind — the inline
    // edit is a one-number edit, the kind is locked); Escape/outside
    // (the host's dismissal) hits the disarm funnel.
    install_seam!(window: window.on_habits_quota_edit_asked, adapter: adapter; // slint: habits-quota-edit-asked
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions. The
    // opener resolves the subject from this fact (one arming site).
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitQuotaEdit);

    });
    install_seam!(window: window.on_habits_quota_stepped, adapter: adapter; // slint: habits-quota-stepped
    bind mut (a, window), args (delta), {        // The stepper's floor is 0 — the typed invalid draft renders
    // (commit blocked); no upper clamp beyond i32 (the mint refuses
    // overflow at commit, and a u32 quota bounds the reachable
    // values).
    a.habit_quota_count = (a.habit_quota_count + delta).max(0);
    a.sync_quota_popover(&window);

    });
    install_seam!(window: window.on_habits_quota_committed, adapter: adapter; // slint: habits-quota-committed
    bind mut (a, window), args (), {        // The zero draft never routes (US-4/AC-3): the typed
    // invalid-draft state blocks the commit — the popover stays
    // open, the rungs remain the recovery.
    if a.habit_quota_count < 1 {
        return;
    }
    // The habit and its CURRENT kind resolve fresh at commit — never
    // cached at arm (what you see is what the habit lands on; the
    // kind lock reads the stored row, not the popover's stale echo).
    let Some(habit) = a.habit_edit_subject() else {
        a.disarm_habit_quota(&window);
        return;
    };
    let state = a.state.load();
    let Some(row) = state.habits.habits.get(&habit) else {
        drop(state);
        a.disarm_habit_quota(&window);
        return;
    };
    let Some(count) = u32::try_from(a.habit_quota_count).ok()
        .and_then(|count| taskboard_domain::NonZeroCount::new(count).ok())
    else {
        return; // unreachable past the guard above; the mint is total
    };
    let target = match row.target {
        taskboard_domain::HabitTarget::PerDay(_) => {
            taskboard_domain::HabitTarget::PerDay(count)
        }
        taskboard_domain::HabitTarget::PerWeek(_) => {
            taskboard_domain::HabitTarget::PerWeek(count)
        }
        taskboard_domain::HabitTarget::PerMonth(_) => {
            taskboard_domain::HabitTarget::PerMonth(count)
        }
    };
    drop(state);
    a.habit_quota_draft = Some(crate::router::HabitQuotaDraft { habit, target });
    // The popover closes on commit (the create form's shape); the
    // routed dispatch consumes the one-shot draft.
    window.set_habits_quota_open(false);
    a.route(&window, ActionId::HabitQuotaCommit);

    });
    install_seam!(window: window.on_habits_quota_dismissed, adapter: adapter; // slint: habits-quota-dismissed
    bind mut (a, window), args (), {        a.disarm_habit_quota(&window);

    });
}

/// The column resize divider seams (r4 D6): press materializes the
/// resolved row into the view-state overlay and captures the boundary
/// (left stack, right stack, rendered base of the left column), drag
/// moves exactly the pair at the boundary (the pure
/// [`crate::widths::drag_boundary`]) and re-renders, release persists
/// the full snapshot over the request lane. A stale column index
/// against an older tables pass is absorbed silently (the
/// `board_target` precedent).
fn install_stack_width_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_stack_width_press, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Some(&left) = a.tables.stack_ids.get(column) else {
        return;
    };
    let Some(board) = a.view_state.board() else {
        return;
    };
    let row = crate::models::board_row(&window, &a.state.load(), &a.view_state);
    // A boundary needs a right column; the last column has no
    // divider (the markup's conditional hides it anyway).
    let Some(&right) = row.stacks.get(column + 1) else {
        return;
    };
    let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
    // Press materializes the resolved row into the view-state
    // overlay (chunk-2 plan §7.2): from here the gesture runs on a
    // total, sum-exact basis — untouched columns cannot wiggle
    // mid-gesture and the released boundary cannot jump when the
    // next derive re-fits the row. View-state only; release still
    // sends the lane's one request.
    for (stack, &width) in row.stacks.iter().zip(&row.widths) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let stored = (f64::from(width) / scale).round() as i64;
        a.view_state
            .set_stack_width(board, *stack, u32::try_from(stored).unwrap_or(u32::MAX));
    }
    #[allow(clippy::cast_possible_truncation)] // a px value
    let base = f64::from(row.widths[column]) as f32;
    a.width_drag = Some((left, right, base));

    });
    install_seam!(window: window.on_board_stack_width_drag, adapter: adapter;
    bind mut (a, window), args (column, delta), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Some((left, right, base)) = a.width_drag else {
        return;
    };
    // A tables pass that reordered the columns makes the index
    // stale: only proceed when it still addresses the dragged
    // boundary's left column (the right StackId rides in the
    // buffer and needs no re-check against the row below).
    if a.tables.stack_ids.get(column) != Some(&left) {
        return;
    }
    let Some(board) = a.view_state.board() else {
        return;
    };
    let row = crate::models::board_row(&window, &a.state.load(), &a.view_state);
    let Some(boundary) = row.stacks.iter().position(|stack| *stack == left) else {
        return;
    };
    if row.stacks.get(boundary + 1) != Some(&right) {
        return;
    }
    let requested = f64::from(base) + f64::from(delta);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let d = (requested - f64::from(row.widths[boundary])).round() as i64;
    let next = crate::widths::drag_boundary(&row.widths, boundary, d, &row.mins);
    let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
    for index in [boundary, boundary + 1] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let stored = (f64::from(next[index]) / scale).round() as i64;
        a.view_state.set_stack_width(
            board,
            row.stacks[index],
            u32::try_from(stored).unwrap_or(u32::MAX),
        );
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_board_stack_width_release, adapter: adapter;
    bind mut (a, _window), args (column), {        let Ok(_column) = usize::try_from(column) else {
        return;
    };
    // Persist on release only (the drag path never sends): one
    // full-snapshot request, warn-and-drop on a full inbox.
    if a.width_drag.take().is_none() {
        return;
    }
    let Some(board) = a.view_state.board() else {
        return;
    };
    let request = crate::requests::UiRequest::StackWidthsChanged {
        board,
        widths: a.view_state.stack_widths_of(board),
    };
    if a.requests.try_send(request).is_err() {
        tracing::warn!("request channel full or closed; dropped");
    }

    });
}

/// The pointer drag-to-move seams (r2 chunk-8, D-B/D-F/D-G): the
/// `DragArea` arm, the `DropArea` hover/drop reports, and the
/// drag-finished outcome. Every handler is flow-guarded — a seam
/// arriving with no active `Flow::DragMove` is a post-teardown echo
/// and is absorbed (the cancel matrix's race arm).
#[allow(clippy::too_many_lines)] // one seam block family, kept flat (the install family's rule)
fn install_drag_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The payload minted once at mount (the plan's P-3): non-empty
    // (the runtime refuses to start a drag without one); the adapter
    // addresses drags through the id tables, never the payload.
    window
        .global::<crate::DragState>()
        .set_payload(slint::DataTransfer::from(SharedString::from(
            "taskboard-task",
        )));

    // The arm: a DragArea crossed the slop. Dismisses the source
    // card's possibly held-open tooltip (the open-menu precedent),
    // then opens the drag flow with the target seeded to the source
    // stack. A stale (column, row) against an older tables pass is
    // absorbed (the `focus_board_card` precedent).
    install_seam!(window: window.on_board_card_drag_armed, adapter: adapter;
    bind mut (a, window), args (column, row), {        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    if a.view_state.flow() != &Flow::None {
        return;
    }
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    Adapter::dismiss_tooltip(&window);
    let state = a.state.load_full();
    let outcomes = a.view_state.begin_drag_move(&state, task);
    a.apply_outcomes(&window, &outcomes);

    });

    // The hover report: a column DropArea sees the DragMove (the
    // runtime's DragMove hit-testing designated it — no geometry
    // crosses to Rust, D-E). Resolves the column against the id
    // tables and reports the target; inert without the drag flow.
    install_seam!(window: window.on_board_drag_hovered, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = a.view_state.drag_target(&state, stack);
    a.apply_outcomes(&window, &outcomes);

    });

    // The drop: re-assert the target from the drop column, then
    // confirm — the shared construction site builds exactly the
    // `MoveTask` the keyboard flow confirms (D-F, equivalence by
    // construction; the journey pins the command).
    install_seam!(window: window.on_board_card_dropped, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    // Round 3 (R2, F8): a drop that cannot resolve its column —
    // the stale-tables race — settles instead of returning: the
    // move dispatches nothing (a missed drop must not guess a
    // target) and no armed flow/ghost survives the gesture.
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
        return;
    };
    let state = a.state.load_full();
    let mut outcomes = a.view_state.drag_target(&state, stack);
    outcomes.extend(a.view_state.confirm_drag_move(&state));
    a.apply_outcomes(&window, &outcomes);

    });

    // The gap hover report (UX-R8 D-2, US-2/S1): a gap-zone DropArea's
    // hover address, resolved to (stack, gap) through the id tables.
    // An unresolvable (column, gap) — the stale-tables race — is
    // absorbed silently (hover asserts nothing).
    // slint: board-gap-hovered
    install_seam!(window: window.on_board_gap_hovered, adapter: adapter;
    bind mut (a, window), args (column, gap), {        let (Ok(column), Ok(gap)) = (usize::try_from(column), usize::try_from(gap)) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        return;
    };
    let state = a.state.load_full();
    if gap > state.tasks_in_stack(stack).len() {
        return; // a stale gap index: absorbed (hover asserts nothing)
    }
    let outcomes = a.view_state.drag_gap(&state, stack, gap);
    a.apply_outcomes(&window, &outcomes);

    });

    // The gap drop (UX-R8 D-2, US-2/S2): re-assert the (stack, gap)
    // aim, then confirm through the shared construction site. The
    // stale twins (the `board-card-dropped` F8 law verbatim): an
    // unresolvable column or gap on drop *settles* the flow — the
    // move dispatches nothing and no armed ghost survives.
    // slint: board-gap-dropped
    install_seam!(window: window.on_board_gap_dropped, adapter: adapter;
    bind mut (a, window), args (column, gap), {        let (Ok(column), Ok(gap)) = (usize::try_from(column), usize::try_from(gap)) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
        return;
    };
    let state = a.state.load_full();
    if gap > state.tasks_in_stack(stack).len() {
        a.settle_flow(&window, &state);
        return;
    }
    let mut outcomes = a.view_state.drag_gap(&state, stack, gap);
    outcomes.extend(a.view_state.confirm_drag_move(&state));
    a.apply_outcomes(&window, &outcomes);

    });

    // The compact segment's hover report (UX-R8 D-3, US-3/S1): the
    // switcher's DropArea address resolved to its stack — the
    // highlight rides the existing move-target paint (model-driven),
    // so the report is the column-level aim (no gap). A stale segment
    // index is absorbed silently.
    // slint: switcher-drop-hovered
    install_seam!(window: window.on_board_switcher_drop_hovered, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = a.view_state.drag_target(&state, stack);
    a.apply_outcomes(&window, &outcomes);

    });

    // The compact segment drop (UX-R8 D-3, US-3/S2): resolve the
    // segment's stack, then guard — the dragged task's *own* stack's
    // segment is the selected stack's segment in the compact
    // one-column leg (R-3's append-only rule makes a same-stack
    // segment drop meaningless), so it settles the flow and dispatches
    // nothing (AC-2). Otherwise confirm through the shared
    // construction site: the column-level aim appends (R-3 verbatim).
    // A stale segment index settles (never guesses, the F8 law).
    // slint: switcher-card-dropped
    install_seam!(window: window.on_board_switcher_card_dropped, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Flow::DragMove { task, .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
        return;
    };
    let state = a.state.load_full();
    if state.tasks.get(task).is_some_and(|t| t.stack == stack) {
        a.settle_flow(&window, &state);
        return;
    }
    let mut outcomes = a.view_state.drag_target(&state, stack);
    outcomes.extend(a.view_state.confirm_drag_move(&state));
    a.apply_outcomes(&window, &outcomes);

    });

    // The finished outcome (round 3, R2/F8): the flow settles
    // whenever it is still armed — `false` (a cancelled drag —
    // release outside any accepting column, the runtime's Exit
    // rewrite) cancels, and a `true` whose confirm was swallowed by
    // the stale-column race cancels too. `try_borrow_mut`: the
    // adapter's synthetic
    // `PointerExited` teardown (Escape / a leaving effect / an
    // Activate confirm mid-drag) fires `drag-finished(none)`
    // synchronously *inside* an `interpret` borrow; that echo is
    // absorbed here — the view-state flow is already gone, which the
    // flow guard would assert anyway.
    install_seam!(window: window.on_board_card_drag_finished, adapter: adapter; [try]
    bind mut (a, window), args (column, row, _completed), {        let _ = (column, row);
    // Round 3 (R2, F8): the `completed` early-out is gone — the
    // handler settles whenever the flow is still armed,
    // `completed` or not. `completed == true` with the flow still
    // armed means the drop seam's confirm was swallowed (exactly
    // the stale-column race); `completed == false` is today's
    // cancel. Either way the gesture is over and nothing stays
    // armed. The `try_borrow_mut` absorption stays: the synthetic
    // teardown echo fires inside an `interpret` borrow with the
    // flow already gone — absorption is correct there and can
    // never mask a legitimate settle.
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let state = a.state.load_full();
    a.settle_flow(&window, &state);

    });
}

/// The toolbar's menu button is a stateful toggle (round-1 D1): open
/// when closed (routing `app.menu`), a full close when open (routing
/// `menu.dismiss`) — the F1 fix, since the toggle means `OpenMenu` can
/// only ever fire while the menu is closed, so the pre-menu chain the
/// projection reads is the real one on every open.
/// The status panel's seams (OP3 W-1..W-3): the mount's
/// snapshot-on-open dispatch and the Refresh control's re-query ride
/// the `AppWindow` root callbacks (the seam census's manifest shape —
/// both listed in `seam_inventory.rs`). The fold/failure directions
/// are invokes (Rust → slint). The panel dispatches no `StateCommand`
/// — R-3's read-only contract is structural here: no seam body touches
/// the command lane.
fn install_status_panel_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // slint: status-panel-open — the mount's snapshot-on-open dispatch
    // (D-3, US-1/S1).
    install_seam!(window: window.on_status_panel_open, adapter: adapter;
    bind mut (a, window), args (), {        a.dispatch_status_query(&window);

    });
    // slint: status-panel-refresh — the explicit re-query (US-1/S4).
    install_seam!(window: window.on_status_panel_refresh, adapter: adapter;
    bind mut (a, window), args (), {        a.dispatch_status_query(&window);

    });
    // The remediation lane (the D2 command plan W-1..W-4/W-6): the
    // rows' arm moments (arm-only — nothing dispatches), the armed
    // strip's decision pair, and the outcome fold. The kiosk
    // permission gate lives in the arm (the trap-a law — the
    // structural inventory's no-dispatch assertion holds by
    // construction). The permission fold is minted here at install:
    // a kiosk lifecycle renders the rows disabled (D-9).
    {
        let kiosk = adapter.borrow().kiosk;
        window
            .global::<crate::StatusPanelFacts>()
            .set_remediation_permitted(!kiosk);
    }
    install_seam!(window: window.on_status_holder_kill, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_arm(&window, crate::requests::StatusRemediationAction::KillHolder);

    });
    install_seam!(window: window.on_status_lock_force_unlock, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_arm(&window, crate::requests::StatusRemediationAction::ForceUnlock);

    });
    install_seam!(window: window.on_status_daemon_restart, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_arm(&window, crate::requests::StatusRemediationAction::RestartDaemon);

    });
    // The armed strip's decision pair (W-4): Confirm routes the
    // armed verb's exact request; Cancel disarms with nothing
    // dispatched.
    install_seam!(window: window.on_status_remediation_armed, adapter: adapter;
    bind mut (a, window), args (decision), {        a.status_remediation_decision(&window, decision);

    });
    // The W-6 display fold: the reply drain invokes the callback
    // outside the adapter borrow (the re-entrancy law); the handler
    // folds the stashed outcome and re-queries (D-3's post-fact
    // refresh).
    install_seam!(window: window.on_status_remediation_outcome_shown, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_outcome_fold(&window);

    });
}

/// Folds one settled [`StatusSnapshot`] into the panel's presentation
/// data (OP3 W-2): the typed posture carries the D-2 rule verbatim;
/// absent facts render as -1 / empty strings. Presentation fold only —
/// no GUI-side fact is minted.
fn status_panel_data(snapshot: &crate::requests::StatusSnapshot) -> crate::StatusPanelData {
    let absent = -1_i32;
    let as_int = |value: Option<u32>| value.and_then(|v| i32::try_from(v).ok()).unwrap_or(absent);
    crate::StatusPanelData {
        posture: match crate::requests::fold_status_posture(snapshot) {
            crate::requests::StatusPosture::Serving => crate::StatusPanelPosture::Serving,
            crate::requests::StatusPosture::Draining => crate::StatusPanelPosture::Draining,
            crate::requests::StatusPosture::NoDaemon => crate::StatusPanelPosture::NoDaemon,
        },
        holder_pid: as_int(snapshot.holder_pid),
        version: snapshot.version.clone().unwrap_or_default().into(),
        backend: snapshot.backend.clone().unwrap_or_default().into(),
        pid: as_int(snapshot.pid),
        connector_present: snapshot.connector_attached.is_some(),
        connector_attached: snapshot.connector_attached.unwrap_or(false),
        draining: snapshot.draining == Some(true),
        sync_phase: snapshot.sync_phase.clone().unwrap_or_default().into(),
        sync_pending: as_int(snapshot.sync_pending_ops),
        engine_commands: snapshot
            .engine_commands
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(absent),
        engine_rejections: snapshot
            .engine_rejections
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(absent),
        engine_signals: snapshot
            .engine_signals
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(absent),
    }
}

fn install_toolbar_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_toolbar_menu_clicked, adapter: adapter;
    bind mut (a, window), args (), {        if a.shell.menu.open {
        // The toggle is a full close from any level (round-1 D6).
        a.apply_effect(&window, UiEffect::CloseMenu);
    } else {
        a.route(&window, ActionId::AppMenu);
    }

    });
    // The inbox chip's click routes `view.inbox` (§7.1: every click
    // has an action equivalent; chunk 10 D9).
    install_seam!(window: window.on_toolbar_inbox_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ViewInbox);

    });
    // The view switcher's clicks (round-3 D22): each routes the same
    // registry action the keyboard uses — the leaving-rule settle
    // applies unchanged (a switcher click under a live flow settles
    // it first, the `FocusBoard` precedent).
    install_seam!(window: window.on_toolbar_switcher_board_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::NavFocusBoard);

    });
    install_seam!(window: window.on_toolbar_switcher_today_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ViewToday);

    });
}

/// The undo branch picker's seams (UF2, #353): the W-1 mount ack, the
/// plain reflex rows (click ≡ key — the same `AppUndo`/`AppRedo` route
/// the keyboard uses), the W-2/W-3 row seams (the payload id re-typed
/// at the trust boundary; a stale or foreign id dispatches nothing —
/// the silent law), and the AC-4 outside-click dismissal.
fn install_undo_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_undo_branches_open, adapter: adapter;
        bind (a, window), args (), {
        // The mount ack: the models are already current (the open arm
        // minted them); this seam exists so the composition's init and
        // any later re-open share one sync (nothing dispatches on
        // open — the plan's W-1 row).
        a.sync_undo_picker_models(&window);
    });
    install_seam!(window: window.on_undo_line_undo, adapter: adapter;
        bind mut (a, window), args (), {
        a.undo_picker_commit(&window, |a, window| a.route(window, ActionId::AppUndo));
    });
    install_seam!(window: window.on_undo_line_redo, adapter: adapter;
        bind mut (a, window), args (), {
        a.undo_picker_commit(&window, |a, window| a.route(window, ActionId::AppRedo));
    });
    install_seam!(window: window.on_undo_target_picked, adapter: adapter;
        bind mut (a, window), args (id: slint::SharedString), {
        let Some(target) = Adapter::parse_journal_id(&id) else { return; };
        a.undo_picker_commit(&window, move |a, _window| {
            a.send(taskboard_domain::StateCommand::UndoTo { target });
        });
    });
    install_seam!(window: window.on_undo_branch_picked, adapter: adapter;
        bind mut (a, window), args (id: slint::SharedString), {
        let Some(target) = Adapter::parse_journal_id(&id) else { return; };
        a.undo_picker_commit(&window, move |a, _window| {
            a.send(taskboard_domain::StateCommand::RedoTo { target });
        });
    });
    install_seam!(window: window.on_undo_picker_dismissed, adapter: adapter;
        bind mut (a, window), args (), {
        // AC-4: close with nothing dispatched.
        a.shell.undo_picker = false;
        a.sync_undo_picker_models(&window);
    });
}

impl Adapter {
    /// Re-types a row-carried journal id at the trust boundary (the
    /// W-2/W-3 addressing law): a malformed string is absorbed (the
    /// silent law — the stale-row case surfaces as the engine's typed
    /// `TargetNotInLine` only for well-formed but moved ids).
    fn parse_journal_id(id: &slint::SharedString) -> Option<taskboard_domain::JournalId> {
        uuid::Uuid::parse_str(id)
            .ok()
            .map(taskboard_domain::JournalId::from_uuid)
    }

    /// The row-commit prelude: the picker closes first (the
    /// close-then-dispatch shape — the `InvokeSelected` precedent,
    /// bounded), then the closure dispatches.
    fn undo_picker_commit(
        &mut self,
        window: &AppWindow,
        dispatch: impl FnOnce(&mut Self, &AppWindow),
    ) {
        self.shell.undo_picker = false;
        self.sync_undo_picker_models(window);
        dispatch(self, window);
    }
}

/// The dismissal layer's full close (round-1 D2): an outside click
/// routes `menu.dismiss` — a dismissal, not a level-up.
fn install_menu_dismiss_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // An outside click is a full dismissal, not a level-up
    // (round-1 D6: `CloseMenu`, the layer and the toggle share it).
    install_seam!(window: window.on_menu_dismiss_requested, adapter: adapter;
        bind mut (a, window), args (), {
        a.apply_effect(&window, UiEffect::CloseMenu);
    });
}

/// The right-click context menu opener (round-1 D13, round-2 D21):
/// focus the addressed card (right-click selects like left-click),
/// then open the shared menu pipeline over the named object actions —
/// flat, anchored at the pointer. Over `[Board, Global]` (the one
/// chain every object menu opens with) every board-surface verb's
/// binding hint resolves — the F-B7 fix. Stale indices against an
/// older tables pass are absorbed silently.
impl Adapter {
    fn open_context_menu(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        ids: &[ActionId],
        bar: Vec<crate::menu::QuickBarEntry>,
        states: &std::collections::BTreeMap<ActionId, bool>,
    ) {
        let chain = [ContextId::Board, ContextId::Global].to_vec();
        self.menu_anchor = MenuAnchor::Pointer(x, y);
        self.menu_kind = MenuKind::Object;
        self.menu_subject = MenuSubject::None;
        self.menu_chain = chain;
        // The id-list menus have no subject (the multi set and the
        // stack/board surfaces' curated lists).
        self.menu_row_subject = None;
        self.root_entries = menu::object_entries(&self.keymap, &self.menu_chain, ids, states);
        self.menu_entries = self.root_entries.clone();
        // The quick bar (UX-R8 chunk-5 D-2): built once at open —
        // the D-1 pin covers it; every other menu consumer passes
        // [] (an empty model renders nothing).
        self.quick_bar_entries = bar;
        self.shell.menu = crate::shell::MenuState {
            open: true,
            selected: 0,
            len: self.menu_entries.len(),
            active_group: None,
            task_group: None,
            root_selected: 0,
        };
        self.sync_menu_models(window);
        self.sync(window);
    }

    /// The task card's menu list (round-2 D21 spelling of round-1
    /// D13): the Task group's rows, reopen and move-cancel included.
    /// `extra` extends the list for the calling surface (the
    /// tracker hide-set filter is the subtraction precedent) — the
    /// today seam passes `TaskDueToday` (UX R-4 D-8, #796: the
    /// verb binds `ContextId::Today` only, so board/stack cards'
    /// menus stay untouched).
    pub(crate) fn open_task_context_menu(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        extra: &[ActionId],
        subject: Option<crate::presentation::RowSubject>,
    ) {
        // The habit arm (H2 D1/B1): the union's habit subject routes
        // to the habit page's own opener — the task page's grammar is
        // untouched for task subjects.
        if let Some(crate::presentation::RowSubject::Habit(habit)) = subject {
            let index = self.tables.habits.iter().position(|&row| row == habit);
            if let Some(index) = index {
                self.open_habit_context_menu(window, x, y, index);
            }
            return;
        }
        let task = subject.and_then(|subject| match subject {
            crate::presentation::RowSubject::Task(task) => Some(task),
            crate::presentation::RowSubject::Habit(_) => None,
        });
        // The selection branch (UX-R8 D-6, US-4/S3): a member
        // invocation opens the multi row set (the count caption is
        // D-9); a non-member (or board-surface-foreign) invocation
        // opens the curated single-card menu.
        let selection_covers = task.is_some_and(|task| self.view_state.selection().contains(&task));
        if selection_covers {
            let ids: Vec<ActionId> = menu::TASK_CONTEXT_MULTI.to_vec();
            // The multi bar (J5 US-2, R-3's multi-sensibility): the
            // aggregate facts over the selection's members — this is
            // also the H-1 fix (a member right-click, the common
            // selection-covered case, used to open a barless menu).
            let bar = {
                let state = self.state.load_full();
                let members = self.view_state.selection_anchor_order(&state);
                let tracker = crate::presentation::tracking_open_task(&state);
                let tracked_members = members
                    .iter()
                    .filter(|member| Some(**member) == tracker)
                    .count();
                let any_scheduled = members.iter().any(|member| {
                    state
                        .tasks
                        .get(member)
                        .is_some_and(|task| task.scheduled.is_some())
                });
                let tracking_elsewhere = tracker.is_some_and(|open| !members.contains(&open));
                menu::multi_quick_bar(
                    members.len(),
                    any_scheduled,
                    tracked_members,
                    tracking_elsewhere,
                )
            };
            self.open_context_menu(window, x, y, &ids, bar, &std::collections::BTreeMap::new());
            // The disarm snapshot (#927): recorded after the opener
            // resets it — the multi menu disarms when every member
            // vanished.
            self.menu_subject = MenuSubject::Selection(
                self.view_state
                    .selection_anchor_order(&self.state.load_full()),
            );
            return;
        }
        // Opening another subject's menu disarms the armed habit
        // conversion (H2 D4: one subject armed at a time) — and the
        // armed remove (H4 D4, the same one-subject law).
        if self.habit_convert_armed.is_some() {
            self.habit_convert_armed = None;
            self.habit_convert_target = None;
            self.pattern_draft = None;
        }
        self.habit_remove_armed = None;
        // The curated grammar (UX-R8 chunk-5 D-4/D-5, US-2): the
        // quick bar + the applicable rows + the four families compose
        // through `task_context_page` over the per-subject facts (the
        // #492 tracker hide-set moved into `TaskMenuSubject::is_tracker`
        // — semantics unchanged, the reopen no-op stays).
        // The one chain every object menu opens with (round-2 D21) —
        // set before the composition so the projection resolves the
        // board-surface verbs' hints.
        self.menu_chain = [ContextId::Board, ContextId::Global].to_vec();
        let subject_id = task.or(self.view_state.focused_task());
        self.menu_row_subject = subject_id.map(crate::presentation::RowSubject::Task);
        let (bar, entries) = {
            let state = self.state.load_full();
            let flow = self.view_state.flow().clone();
            let states = self.entry_states(&state, &flow);
            let subject = crate::presentation::task_menu_subject(
                &state,
                subject_id.map(crate::presentation::RowSubject::Task),
            );
            menu::task_context_page(&self.keymap, &self.menu_chain, subject, &states, extra)
        };
        self.open_context_menu_with(window, x, y, entries, bar);
        // The disarm snapshot (#927): recorded after the opener resets
        // it — the pinned subject's death closes the menu on the same
        // reconciliation pass.
        self.menu_subject = subject_id.map_or(MenuSubject::None, MenuSubject::Single);
    }

    /// The open menu's subject's facts (UX-R8 chunk-5 D-4; H2 D1): the
    /// focused task on every card surface (the context seams'
    /// focus-first posture), or the open habit menu's habit — the
    /// submenu machinery reads the same union either way.
    fn current_task_subject(&self) -> crate::menu::TaskMenuSubject {
        let state = self.state.load_full();
        crate::presentation::task_menu_subject(&state, self.menu_row_subject)
    }

    /// The context-menu opener over pre-computed rows (UX-R8 chunk-5
    /// D-5): the curated builder's landing — the id-list variant
    /// delegates with the projected rows.
    fn open_context_menu_with(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        entries: Vec<crate::menu::MenuEntry>,
        bar: Vec<crate::menu::QuickBarEntry>,
    ) {
        self.menu_anchor = MenuAnchor::Pointer(x, y);
        self.menu_kind = MenuKind::Object;
        self.menu_subject = MenuSubject::None;
        // The one chain every object menu opens with (round-2 D21):
        // every board-surface verb's binding hint resolves over it.
        self.menu_chain = [ContextId::Board, ContextId::Global].to_vec();
        self.root_entries = entries;
        self.menu_entries = self.root_entries.clone();
        // The quick bar (UX-R8 chunk-5 D-2): built once at open —
        // the D-1 pin covers it; every other menu consumer passes
        // [] (an empty model renders nothing).
        self.quick_bar_entries = bar;
        self.shell.menu = crate::shell::MenuState {
            open: true,
            selected: 0,
            len: self.menu_entries.len(),
            active_group: None,
            task_group: None,
            root_selected: 0,
        };
        self.sync_menu_models(window);
        self.sync(window);
    }
}

/// A board card's right click (round-1 D13): resolve, focus, open.
fn install_board_context_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_task_context_requested, adapter: adapter;
    bind mut (a, window), args (column, row, x, y), {        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    settle_pointer_conflict_keeping_selection(&mut a, &window, true);
    let Some((stack, task)) = a.tables.board_task(column, row) else {
        return;
    };
    let outcomes = a.view_state.activate_card(task, stack);
    a.apply_outcomes(&window, &outcomes);
    a.open_task_context_menu(
        &window,
        x,
        y,
        &[],
        Some(crate::presentation::RowSubject::Task(task)),
    );

    });
}

/// A stack-detail card's right click (round-1 D13).
fn install_stack_context_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_stack_task_context_requested, adapter: adapter;
    bind mut (a, window), args (row, x, y), {        let Ok(row) = usize::try_from(row) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(task) = a.tables.stack_task(row) else {
        return;
    };
    let Some(stack) = a.view_state.subject_stack() else {
        return;
    };
    let outcomes = a.view_state.activate_card(task, stack);
    a.apply_outcomes(&window, &outcomes);
    // The stack-detail surface carries no selection (R-1's scoping).
    a.open_task_context_menu(&window, x, y, &[], None);

    });
}

/// The today view's row seams (chunk 10 D7): click, activation, and
/// context — every one focuses the row first (the board card's §7.1
/// discipline, with the selection-follows-focus retarget), then the
/// activation/context seams route through the action layer's verbs.
#[allow(clippy::too_many_lines)] // one seam block, kept flat
fn install_today_row_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_today_row_clicked, adapter: adapter;
    bind mut (a, window), args (section, row), {        settle_pointer_conflict(&mut a, &window);
    focus_today_row(&mut a, &window, section, row);

    });
    // -- the plan-next-day surface (#622, D-3/D-6) --
    install_seam!(window: window.on_plan_box_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.begin_plan_place(&window, task, true);

    });

    install_seam!(window: window.on_plan_pool_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(
            a.tables
                .plan_box_count
                .saturating_add(usize::try_from(index).unwrap_or(usize::MAX)),
        )
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.begin_plan_place(&window, task, false);

    });

    install_seam!(window: window.on_plan_box_earlier, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanBoxEarlier);

    });

    install_seam!(window: window.on_plan_box_later, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanBoxLater);

    });

    install_seam!(window: window.on_plan_box_remove, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a
        .tables
        .plan_rows
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanRemoveBox);

    });

    install_seam!(window: window.on_plan_time_edited, adapter: adapter;
    bind mut (a, _window), args (text), {        // The D9 mirror rule: the level-held buffers update per
    // keystroke; a refresh never clobbers them while the flow
    // holds them.
    if let Some(place) = a.view_state.plan_place_mut() {
        place.time_text = text.to_string();
    }

    });

    install_seam!(window: window.on_plan_effort_edited, adapter: adapter;
    bind mut (a, _window), args (text), {        if let Some(place) = a.view_state.plan_place_mut() {
        place.effort_text = text.to_string();
    }

    });

    install_seam!(window: window.on_plan_place_commit, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::PlanCommitPlace);
    if matches!(a.view_state.flow(), crate::view_state::Flow::PlanPlace { .. }) {
        // The parse refusal kept the form: echo the typed text back
        // (the due editor's validity flavour, minus the tint).
        window.set_plan_time_text(SharedString::from(
            a.view_state
                .plan_place()
                .map_or(String::new(), |place| place.time_text.clone()),
        ));
        window.set_plan_effort_text(SharedString::from(
            a.view_state
                .plan_place()
                .map_or(String::new(), |place| place.effort_text.clone()),
        ));
    } else {
        window.set_plan_time_text(SharedString::new());
        window.set_plan_effort_text(SharedString::new());
    }

    });

    install_seam!(window: window.on_plan_place_cancel, adapter: adapter;
    bind mut (a, _window), args (), {        // The cancel's wipe is the level's drop (D-H2).
    a.view_state.close_plan_place();

    });

    // #592: the live rows' extent reports feed the height cache
    // (deduped — a no-op report costs one map insert comparison).
    install_seam!(window: window.on_today_row_extent, adapter: adapter; [bare_mut]
    bind mut (a), args (id, extent), {
        a.models.today_row_extents.insert(id, extent);
    });

    install_seam!(window: window.on_today_row_activated, adapter: adapter;
    bind mut (a, window), args (section, row), {        settle_pointer_conflict(&mut a, &window);
    if focus_today_row(&mut a, &window, section, row).is_some() {
        a.route(&window, ActionId::TaskEdit);
    }

    });
    // The done checkbox (r3 D6): the toggled command, the board
    // cards' seam verbatim (a stale index absorbed).
    install_seam!(window: window.on_today_row_done_toggled, adapter: adapter; [bare_ref]
    bind (a), args (section, row), {        let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
        return;
    };
    if let Some(task) = a.tables.today_row(section, row) {
        send_toggled_done(&a, task);
    }

    });

    // The card play/pause control (UX-R8 chunk-5 D-6): the today
    // leg's addressing chain.
    install_seam!(window: window.on_today_row_track_toggled, adapter: adapter;
    bind mut (a, window), args (section, row), {        let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
        return;
    };
    if let Some(task) = a.tables.today_row(section, row) {
        send_card_track_toggle(&mut a, &window, task);
    }

    });
    // The Tracked section's activation (#490, D-TS7): resolves the
    // row through the tracked-today table and opens the task — the
    // today-row-activated arm verbatim (click ≡ key, §7.1).
    install_seam!(window: window.on_today_tracked_activated, adapter: adapter;
    bind mut (a, window), args (row), {        settle_pointer_conflict(&mut a, &window);
    if focus_tracked_today_row(&mut a, &window, row).is_some() {
        a.route(&window, ActionId::TaskEdit);
    }

    });
    install_seam!(window: window.on_today_row_context_requested, adapter: adapter;
    bind mut (a, window), args (section, row, x, y), {        settle_pointer_conflict(&mut a, &window);
    if focus_today_row(&mut a, &window, section, row).is_some() {
        // The today surface carries no selection (R-1's scoping).
        a.open_task_context_menu(&window, x, y, &[ActionId::TaskDueToday], None);
    }

    });
}

/// A today row at `(section, row)`: resolves the id tables (the
/// per-section prefix sums over `today_tasks`) and applies the click's
/// focus state. Returns the addressed task, or `None` for a stale
/// index against an older tables pass.
fn focus_tracked_today_row(adapter: &mut Adapter, window: &AppWindow, row: i32) -> Option<TaskId> {
    let Ok(row) = usize::try_from(row) else {
        return None;
    };
    let task = adapter.tables.tracked_today.get(row).copied()?;
    let stack = adapter.state.load().tasks.get(&task).map(|t| t.stack);
    if let Some(stack) = stack {
        adapter.view_state.activate_card(task, stack);
    } else {
        adapter.view_state.set_focused(Some(task));
    }
    adapter.finish_mutation(window);
    Some(task)
}

fn focus_today_row(
    adapter: &mut Adapter,
    window: &AppWindow,
    section: i32,
    row: i32,
) -> Option<TaskId> {
    let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
        return None;
    };
    let task = adapter.tables.today_row(section, row)?;
    let stack = {
        let state = adapter.state.load();
        state.tasks.get(&task).map(|t| t.stack)?
    };
    let outcomes = adapter
        .view_state
        .activate_today_row(&adapter.state.load(), task, stack);
    adapter.apply_outcomes(window, &outcomes);
    Some(task)
}

/// The stack-context arm, shared by the header and the body seams
/// (r3 D4) so they cannot drift: resolve the column's stack, apply
/// the selection outcomes, open the stack menu at the pointer.
/// Stale indices are absorbed silently (the tables guard).
fn open_stack_context(a: &mut Adapter, window: &AppWindow, column: i32, x: f32, y: f32) {
    let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = a.view_state.select_stack(&state, stack);
    a.apply_outcomes(window, &outcomes);
    a.open_context_menu(
        window,
        x,
        y,
        menu::STACK_CONTEXT,
        Vec::new(),
        &std::collections::BTreeMap::new(),
    );
}

/// The stack card's bare-surface body seams (r3 D4 / r4 D3): the
/// right-down context arm and the left-click select arm — one home so
/// they cannot drift from the header's pair.
fn install_body_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_body_context_requested, adapter: adapter;
    bind mut (a, window), args (column, x, y), {        open_stack_context(&mut a, &window, column, x, y);

    });
    install_seam!(window: window.on_board_body_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);

    });
}

/// The r2 D21 object-context seams: right-click selects first (the
/// card precedent), then opens the object's context menu at the
/// pointer. A right-clicked non-selected board-bar segment switches
/// to that board — "the menu addresses what you clicked" (the
/// recorded semantics). Stale indices are absorbed silently.
fn install_object_context_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The stack header (the regular header and the compact switcher's
    // segments share the callback).
    install_seam!(window: window.on_board_header_context_requested, adapter: adapter;
    bind mut (a, window), args (column, x, y), {        open_stack_context(&mut a, &window, column, x, y);

    });
    // The stack card's bare surface (r3 D4): context on right-down,
    // select on left-click (r4 D3) — the two body seams, one home.
    install_body_seams(window, adapter);
    // The board-bar segment.
    install_seam!(window: window.on_board_segment_context_requested, adapter: adapter;
    bind mut (a, window), args (index, x, y), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.board_ids.get(index).copied() else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = a.view_state.select_board(&state, board);
    a.apply_outcomes(&window, &outcomes);
    // The ceremony rows' liveness verdicts (OP1 US-1/S1): enabled for
    // a live board, disabled for a tombstone (an honest greyed row —
    // the engine's typed refusal stays the keyboard path's backstop).
    let live = state
        .boards
        .get(&board)
        .is_some_and(|row| !row.deleted);
    let states = std::collections::BTreeMap::from([
        (ActionId::BoardArchive, live),
        (ActionId::BoardReset, live),
        (ActionId::BoardReconcile, live),
    ]);
    a.open_context_menu(&window, x, y, menu::BOARD_CONTEXT, Vec::new(), &states);

    });
    // The stack-rename field (per-column; the flow names the stack —
    // the mirror only needs the buffer, the commit resolves the flow).
    install_seam!(window: window.on_board_stack_rename_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (_column, text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_stack_rename_committed, adapter: adapter;
    bind mut (a, window), args (_column, text), {        commit_text(&mut a, &window, &text);

    });
    // The board create/rename fields: the window-level slot's
    // mirror/commit seams (the create-row pattern).
    install_seam!(window: window.on_board_create_field_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_create_field_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
    install_seam!(window: window.on_board_rename_field_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_rename_field_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
}

/// The settings back affordance (round-1 D6.6): the dedicated seam
/// routing the same close path the Escape binding takes — an adapter
/// method, not a registry action (closing is not a routable intent).
fn install_settings_back_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_settings_back_clicked, adapter: adapter;
    bind mut (a, window), args (), {        Adapter::dismiss_tooltip(&window);
    let mut outcomes = a.view_state.close_settings();
    // The armed strip dies with the surface (cross-surface friction
    // plan D-4, #836): leaving settings disarms.
    a.settings_disarm_item();
    // US-5 (#706): the restore's subject-liveness guard (see the
    // OpenSettings close arm).
    {
        let state = a.state.load_full();
        outcomes.extend(a.view_state.repair_restored_root(&state));
    }
    a.apply_outcomes(&window, &outcomes);
    a.sync_view_context(&window);
    a.sync(&window);

    });
}

/// The task-detail back button (round-1 D12): routes
/// `nav.focus-board` — the existing `focus_board` outcome, no new
/// view-state semantics.
fn install_task_back_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_task_back_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::NavFocusBoard);

    });
}

/// The habit editor's seams (H4 D5): the draft mirrors, the per-kind
/// day row, and the save/cancel pair. Every mirror mutates only the
/// adapter-owned draft (no engine state — the commit computes the
/// sparse changeset); the validity flavours re-parse at the trust
/// boundary per edit (the count mints through `NonZeroCount`, the
/// month day through `MonthDay`, the time through the due editor's
/// lenient parser).
#[allow(clippy::too_many_lines)] // ten seams, one grammar
fn install_habit_editor_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_habit_title_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.title = t.to_string();
        }
    });
    install_seam!(window: window.on_habit_description_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.description = t.to_string();
        }
    });
    // The quota row: the kind re-seats the target (a now-mismatched
    // day is carried inert — the D5 decision), the count mints
    // per-keystroke (the create form's validity grammar).
    install_seam!(window: window.on_habit_quota_kind_picked, adapter: adapter;
    bind mut (a, window), args (index), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            let kind = match index {
                0 => taskboard_domain::PeriodKind::Day,
                1 => taskboard_domain::PeriodKind::Week,
                _ => taskboard_domain::PeriodKind::Month,
            };
            if let Some(count) = draft.parsed_count() {
                draft.target = match kind {
                    taskboard_domain::PeriodKind::Day => {
                        taskboard_domain::HabitTarget::PerDay(count)
                    }
                    taskboard_domain::PeriodKind::Week => {
                        taskboard_domain::HabitTarget::PerWeek(count)
                    }
                    taskboard_domain::PeriodKind::Month => {
                        taskboard_domain::HabitTarget::PerMonth(count)
                    }
                };
            }
        }
    });
    install_seam!(window: window.on_habit_quota_count_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.quota_count = t.to_string();
            draft.quota_valid =
                draft.parsed_count().is_some() || draft.quota_count.trim().is_empty();
            // Re-seat the target from the fresh parse when it holds.
            if let Some(count) = draft.parsed_count() {
                draft.target = match draft.quota_kind_index() {
                    0 => taskboard_domain::HabitTarget::PerDay(count),
                    1 => taskboard_domain::HabitTarget::PerWeek(count),
                    _ => taskboard_domain::HabitTarget::PerMonth(count),
                };
            }
        }
    });
    install_seam!(window: window.on_habit_time_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.time_text = t.to_string();
            draft.time_edited = true;
            match crate::due_edit::parse_time_lenient(t.trim()) {
                Some(time) => {
                    draft.time_valid = true;
                    draft.anchor_time = Some(time);
                    draft.time_set = true;
                }
                None => {
                    draft.time_valid = false;
                }
            }
        }
    });
    install_seam!(window: window.on_habit_time_cleared, adapter: adapter;
    bind mut (a, window), args (), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.time_text.clear();
            draft.anchor_time = None;
            draft.time_set = false;
            draft.time_valid = true;
            draft.time_edited = true;
        }
    });
    // The 7-slot weekday picker (the PerWeek variant's day row).
    install_seam!(window: window.on_habit_weekday_picked, adapter: adapter;
    bind mut (a, window), args (index), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            let wd = match index {
                0 => chrono::Weekday::Mon,
                1 => chrono::Weekday::Tue,
                2 => chrono::Weekday::Wed,
                3 => chrono::Weekday::Thu,
                4 => chrono::Weekday::Fri,
                5 => chrono::Weekday::Sat,
                _ => chrono::Weekday::Sun,
            };
            draft.anchor_day = Some(taskboard_domain::habit::AnchorDay::Weekday(wd));
            draft.day_edited = true;
        }
    });
    // The 1..=31 day field (the PerMonth variant's row) — the mint
    // rejects 0 and past 31 (the type's own trust boundary).
    install_seam!(window: window.on_habit_month_day_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.month_day = t.to_string();
            draft.day_edited = true;
            match t
                .trim()
                .parse::<u8>()
                .ok()
                .and_then(|n| taskboard_domain::habit::MonthDay::new(n).ok())
            {
                Some(day) => {
                    draft.month_valid = true;
                    draft.anchor_day =
                        Some(taskboard_domain::habit::AnchorDay::MonthDay(day));
                }
                None => {
                    draft.month_valid = false;
                }
            }
        }
    });
    // ---- HF4 (W-6): the schedule picker's toggle seams -----------------
    // The picker is a draft-backed multi-select; the draft holds the
    // raw mask (a mid-edit empty selection is legal — the save gate
    // blocks it, US-5 AC-3).
    install_seam!(window: window.on_habit_schedule_day_toggled, adapter: adapter;
    bind mut (a, window), args (index, checked), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            let bit = 1_u8
                << u32::try_from(index).unwrap_or(u32::MAX).min(6);
            if checked {
                draft.schedule_bits |= bit;
            } else {
                draft.schedule_bits &= !bit;
            }
            draft.schedule_edited = true;
        }
    });
    // ---- HF4 (W-7): the reminder override's field seams ---------------
    install_seam!(window: window.on_habit_reminder_base_picked, adapter: adapter;
    bind mut (a, window), args (index), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_base = usize::try_from(index).unwrap_or(0).min(1);
            draft.reminder_edited = true;
        }
    });
    install_seam!(window: window.on_habit_reminder_offset_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_offset = t.to_string();
            draft.reminder_edited = true;
        }
    });
    install_seam!(window: window.on_habit_reminder_time_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_time = t.to_string();
            draft.reminder_edited = true;
        }
    });
    install_seam!(window: window.on_habit_reminder_snooze_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_snooze = t.to_string();
            draft.reminder_edited = true;
        }
    });
    // The clear leg (US-6/S3): the explicit `Some(None)` reverts the
    // habit to the plain O-1 default nudge.
    install_seam!(window: window.on_habit_reminder_clear_clicked, adapter: adapter;
    bind mut (a, window), args (), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_clear = true;
            draft.reminder_edited = true;
        }
    });
    // The save: ONE sparse `UpdateHabit` computed against the stored
    // row, then the editor closes (the back grammar — the subject
    // dies with the root swap). An empty changeset is unreachable
    // (the gate greys); a stale draft's dead subject reads no row.
    install_seam!(window: window.on_habit_save_clicked, adapter: adapter; // slint callback: habit-save-clicked (H4 W-4)
    bind mut (a, window), args (), {
        let draft = a.habit_edit_draft.clone();
        if let Some(draft) = draft
            && let Some(stored) =
                a.state.load().habits.habits.get(&draft.habit).cloned()
            && draft.can_save(&stored)
        {
            a.send(taskboard_domain::StateCommand::UpdateHabit {
                habit: draft.habit,
                changes: draft.changes(&stored),
            });
            a.habit_edit_draft = None;
            a.route(&window, ActionId::NavFocusBoard);
            a.finish_mutation(&window);
        }
    });
    // The cancel (the back button's twin): discard-and-close —
    // nothing dispatches.
    install_seam!(window: window.on_habit_editor_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        a.habit_edit_draft = None;
        a.route(&window, ActionId::NavFocusBoard);
        a.finish_mutation(&window);
    });
}

/// The request-driven switcher flyout's open predicate (UX R-4 D-4,
/// #794): the panel's own facts, not the widget's — the walk's truth
/// is what the window mounted.
fn switcher_flyout_open(window: &AppWindow) -> bool {
    let dd = window.global::<crate::DropdownFacts>();
    dd.get_open()
        && dd.get_consumer() == crate::DropdownConsumer::BoardSwitcher
        && window
            .global::<crate::BoardBarFacts>()
            .get_switcher_request()
}

/// The flyout's close (the walk's exits): every fact resets, so the
/// strip returns in strip mode and the ladder declines stop.
fn close_switcher_flyout(window: &AppWindow) {
    {
        let facts = window.global::<crate::BoardBarFacts>();
        facts.set_open(false);
        facts.set_switcher_request(false);
    }
    window.global::<crate::DropdownFacts>().set_open(false);
}

impl Adapter {
    /// The flyout walk's key arm (the helper above's engine): the
    /// ladder vocabulary only; anything else is `None` (the resolver
    /// owns it). The clamped highlight moves; Return routes the
    /// highlighted row through the existing segment route.
    pub(crate) fn switcher_flyout_key(
        &mut self,
        window: &AppWindow,
        stroke: crate::keymap::Stroke,
    ) -> Option<bool> {
        // The self-heal: an outside click's underlay wrote the panel's
        // fact directly; the request flags fall in line on the next
        // key (the production loop's changed-delivery syncs them too).
        if window
            .global::<crate::BoardBarFacts>()
            .get_switcher_request()
            && !window.global::<crate::DropdownFacts>().get_open()
        {
            close_switcher_flyout(window);
        }
        if !switcher_flyout_open(window) {
            return None;
        }
        let rows = i32::try_from(slint::Model::row_count(&window.get_board_bar_model().rows))
            .unwrap_or(i32::MAX);
        if rows == 0 {
            return None;
        }
        let named = stroke.named_name();
        let up = named == Some("Up") || (stroke.ctrl && stroke.key == 'p');
        let down = named == Some("Down") || (stroke.ctrl && stroke.key == 'n');
        let ret = named == Some("Return");
        let esc = named == Some("Escape");
        if !(up || down || ret || esc) {
            return None;
        }
        Self::set_feedback(window, KeyFeedback::None);
        let facts = window.global::<crate::BoardBarFacts>();
        if up || down {
            let delta: i32 = if up { -1 } else { 1 };
            let next = (facts.get_highlight() + delta).clamp(0, rows - 1);
            facts.set_highlight(next);
            Some(true)
        } else if esc {
            close_switcher_flyout(window);
            Some(true)
        } else {
            let highlight = facts.get_highlight();
            close_switcher_flyout(window);
            if let Some(index) = usize::try_from(highlight).ok()
                && let Some(board) = self.tables.board_ids.get(index).copied()
            {
                self.board_target = Some(board);
                self.route(window, ActionId::BoardSelect);
            }
            Some(true)
        }
    }
}

/// A board-bar segment click (design §3.8): resolve the segment's
/// board against the tables pass that pushed the bar, set the one-shot
/// target, and route `board.select` — every click affordance has an
/// action equivalent (§7.1). A stale index against an older pass is
/// absorbed silently. The request-driven flyout's row activation
/// closes it first (the widget path's Return closes — click ≡ key).
fn install_board_bar_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_bar_segment_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    close_switcher_flyout(&window);
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.board_ids.get(index).copied() else {
        return;
    };
    a.board_target = Some(board);
    a.route(&window, ActionId::BoardSelect);

    });
}

/// The archived-boards roster's row activation (UX R-1 D-1, #785):
/// resolve the row's board against the same pass's archived-boards
/// table, dispatch `SetBoardArchived { archived: false }` + the
/// models refresh, and arm the one-shot re-select (consumed at
/// reconcile once the engine's apply makes the board live again). A
/// stale index against an older pass is absorbed silently.
fn install_board_bar_archived_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_bar_archived_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.archived_board_ids.get(index).copied() else {
        return;
    };
    // Layer 3 (the ToggleFocusMode precedent, UX R-1 D-1's no-new-
    // kiosk-capability non-goal): the kiosk dispatch refuses — the
    // roster renders the disabled affordance (layer 1's visual), the
    // refusal keeps the verb structurally unreachable.
    if a.kiosk {
        return;
    }
    let outcomes = a.view_state.unarchive_board(board);
    a.apply_outcomes(&window, &outcomes);

    });
}

/// The armed-delete strips' seams (UX R-2 D-2, #784): Confirm routes
/// the armed subject's own delete `ActionId` — with the strip armed,
/// the route's interpretation table arm confirms (click ≡ key at
/// every step); Cancel disarms without dispatching. The stack pair's
/// index is the mounting column (the pass the strip rendered from);
/// the confirm routes the armed subject regardless, since the strip
/// only mounts in the armed column of the current pass.
fn install_delete_strip_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // No `settle_pointer_conflict` on any of the four: the armed
    // flow IS the interaction's subject — a settle here would disarm
    // before the route re-arms (the confirm would never commit).
    // One flow lives at a time, so no foreign flow can be settled
    // by a strip interaction.
    // The armed multi-delete strip's pair (UX-R8 chunk-5 D-9, #882):
    // Confirm routes the armed verb (the second stroke's twin — the
    // armed repeat confirms), Cancel disarms with nothing dispatched.
    install_seam!(window: window.on_task_multi_delete_strip_confirmed, adapter: adapter;
    bind mut (a, window), args (), {
        a.route(&window, ActionId::TaskDelete);
    });
    install_seam!(window: window.on_task_multi_delete_strip_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        let outcomes = a.view_state.cancel_confirm_delete();
        a.apply_outcomes(&window, &outcomes);
    });
    install_seam!(window: window.on_board_delete_strip_confirmed, adapter: adapter;
    bind mut (a, window), args (), {
        a.route(&window, ActionId::BoardDelete);
    });
    install_seam!(window: window.on_board_delete_strip_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        let outcomes = a.view_state.cancel_confirm_delete();
        a.apply_outcomes(&window, &outcomes);
    });
    // The board ceremony strips' confirm/cancel pair (OP1, W-1/W-3/
    // W-6): one pair — the armed subject surface-wide decides which
    // verb routes (the ConfirmDelete strip's route-the-verb shape;
    // click ≡ key at every step). Cancel is the disarm, nothing
    // dispatched.
    install_seam!(window: window.on_board_ceremony_confirmed, adapter: adapter;
    bind mut (a, window), args (), {
        match a.view_state.flow() {
            crate::view_state::Flow::ConfirmBoardArchive { .. } => {
                a.route(&window, ActionId::BoardArchive);
            }
            crate::view_state::Flow::ConfirmBoardReset { .. } => {
                a.route(&window, ActionId::BoardReset);
            }
            crate::view_state::Flow::ConfirmBoardReconcile { .. } => {
                a.route(&window, ActionId::BoardReconcile);
            }
            _ => {}
        }
    });
    install_seam!(window: window.on_board_ceremony_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        let outcomes = match a.view_state.flow() {
            crate::view_state::Flow::ConfirmBoardArchive { .. } => {
                a.view_state.cancel_board_archive()
            }
            crate::view_state::Flow::ConfirmBoardReset { .. } => {
                a.view_state.cancel_board_reset()
            }
            crate::view_state::Flow::ConfirmBoardReconcile { .. } => {
                a.view_state.cancel_board_reconcile()
            }
            _ => Vec::new(),
        };
        a.apply_outcomes(&window, &outcomes);
    });
    install_seam!(window: window.on_stack_delete_strip_confirmed, adapter: adapter;
    bind mut (a, window), args (_column), {
        a.route(&window, ActionId::StackDelete);
    });
    install_seam!(window: window.on_stack_delete_strip_cancelled, adapter: adapter;
    bind mut (a, window), args (_column), {
        let outcomes = a.view_state.cancel_confirm_delete();
        a.apply_outcomes(&window, &outcomes);
    });
}

/// The board color strip's seams (UX R-4 D-1, #791): a swatch click
/// commits the armed board's exact `SetBoardColor` through the flow
/// (the palette's hex — never free text) and closes the strip; the
/// dismissal (the underlay's click) ends the flow with nothing
/// dispatched. A stale index against an older palette pass is
/// absorbed silently.
fn install_board_color_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_color_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    let Some(swatch) = crate::presentation::SWATCHES.get(index) else {
        return;
    };
    let outcomes = a.view_state.commit_board_color(swatch);
    a.apply_outcomes(&window, &outcomes);
    });
    install_seam!(window: window.on_board_color_strip_dismissed, adapter: adapter;
    bind mut (a, window), args (), {        let outcomes = a.view_state.cancel_board_color();
    a.apply_outcomes(&window, &outcomes);
    });
}

/// The on-board New-stack button routes the `stack.create` action —
/// the click equivalent of its keyboard binding (§7.1) — and the
/// flow's field mirrors/commits like every other text flow (D9).
fn install_stack_create_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_new_stack_clicked, adapter: adapter;
    bind mut (a, window), args (), {        settle_pointer_conflict(&mut a, &window);
    a.route(&window, ActionId::StackCreate);

    });
    // The empty-board preset CTA (r3 D5; UX R-4 D-5, #795): click ≡
    // key at every step — the click routes the `board.default-preset`
    // action, whose dispatch arm owns the empty-board gate (the
    // direct command dispatch is retired).
    install_seam!(window: window.on_board_default_preset_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::BoardDefaultPreset);

    });
    install_seam!(window: window.on_board_stack_create_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_stack_create_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
}

/// The stack-detail done checkbox (r3 D6): the toggled command, the
/// board cards' seam verbatim (a stale index absorbed).
fn install_stack_done_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_stack_task_done_toggled, adapter: adapter; [bare_ref]
    bind (a), args (row), {        let Ok(row) = usize::try_from(row) else {
        return;
    };
    if let Some(task) = a.tables.stack_task(row) {
        send_toggled_done(&a, task);
    }

    });

    // The card play/pause control (UX-R8 chunk-5 D-6): the stack
    // leg's addressing chain.
    install_seam!(window: window.on_stack_task_track_toggled, adapter: adapter;
    bind mut (a, window), args (row), {        let Ok(row) = usize::try_from(row) else {
        return;
    };
    if let Some(task) = a.tables.stack_task(row) {
        send_card_track_toggle(&mut a, &window, task);
    }

    });
}

/// The seam-wiring decision record (chunk 4.2 of backlog round 3,
/// plan `.artifacts/plans/2026-11-03-install-seam-macro-plan.md` §3.2;
/// closes #595): the standing note here read "a `Fn(&mut Adapter,
/// &AppWindow, Args)` wrapper shape is over generics' worth of closure
/// types; the per-seam helpers stay explicit". The re-adjudication
/// counted 163 installs over 28 `install_*` fns (the trigger of audit
/// finding F7-08 fired ~12.5× over) and re-derived 6 scaffold
/// variants × 9 behavior families. That variety kills the *generic
/// wrapper* — the objection above stands and is decisive against it —
/// but it never reached a declarative token-splicing macro, which
/// hands slint exactly the closure it always received: no generics,
/// no inference change. The scaffold is now `install_seam!` (above);
/// bodies and per-seam design comments stay verbatim at the call
/// site; the six genuinely singular installs stay explicit. Re-open
/// trigger: a seventh scaffold shape or per-site scaffold
/// customization re-adjudicates on the plan's evidence form.
fn install_board_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // A card click: focus the (column, row) card (selection state
    // only, §7.1 mouse discipline); activation routes `task.edit`
    // after it.
    install_seam!(window: window.on_board_task_clicked, adapter: adapter;
    bind mut (a, window), args (column, row), {        focus_board_card(&mut a, &window, column, row);

    });
    // The modifier-click grammar (UX-R8 D-5, US-4/S1): the card's
    // modifier release — shift builds the range span, ctrl toggles;
    // the card resolves via the id tables (a stale index absorbed).
    // Nothing dispatches: selection is presentation state (R-2).
    // slint: task-clicked-modifiers
    install_seam!(window: window.on_board_task_clicked_modifiers, adapter: adapter;
    bind mut (a, window), args (column, row, shift, control), {        // The selection's own seam: the pointer-conflict ritual settles
        // a live flow but KEEPS the selection — the gesture builds on
        // the anchor (the D-4 settle-clear law is for foreign settles:
        // Escape, the modal opens, the surface changes).
        settle_pointer_conflict_keeping_selection(&mut a, &window, true);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = if shift {
        a.view_state.selection_range(&state, task)
    } else if control {
        a.view_state.selection_toggle(task)
    } else {
        Vec::new()
    };
    a.apply_outcomes(&window, &outcomes);

    });
    install_seam!(window: window.on_board_task_activated, adapter: adapter;
    bind mut (a, window), args (column, row), {        if focus_board_card(&mut a, &window, column, row).is_some() {
        a.route(&window, ActionId::TaskEdit);
    }

    });

    // Header and switcher activation are the same selection move (D8:
    // one mechanism per breakpoint; D13: no stack management in UI).
    install_seam!(window: window.on_board_header_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);

    });
    install_seam!(window: window.on_board_switcher_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);

    });

    // The Add Task ghost (r2 D8): select the addressed stack, then
    // route task.create — the keyboard's exact action, so click ≡ key
    // by construction (one action, one target-resolution rule).
    install_seam!(window: window.on_board_add_task_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);
    settle_pointer_conflict(&mut a, &window);
    // The clicked column IS the create target (r4 D4): resolved
    // through the id tables (the D2 index-only convention — a
    // stale index yields `None`, never a wrong stack), so field
    // placement and selection can never diverge.
    if let Ok(index) = usize::try_from(column) {
        a.create_target = a.tables.stack_ids.get(index).copied();
    }
    a.route(&window, ActionId::TaskCreate);

    });

    // The done checkbox (r3 D6): resolve (column, row) against the id
    // tables (a stale index is absorbed, the drag-arm precedent) and
    // raw-send the toggled `SetTaskDone` — the notification Done
    // button's precedent (the registry's complete/reopen act on the
    // focused task; the checkbox addresses an arbitrary card). No
    // local checked state exists anywhere: the refresh push is the
    // single writer of `TaskCardModel.done`.
    install_seam!(window: window.on_board_task_done_toggled, adapter: adapter;
    bind mut (a, window), args (column, row), {        settle_pointer_conflict(&mut a, &window);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    send_toggled_done(&a, task);

    });

    // The card play/pause control (UX-R8 chunk-5 D-6, US-4): the
    // board leg's addressing chain (the regular and compact cards'
    // seam).
    install_seam!(window: window.on_board_task_track_toggled, adapter: adapter;
    bind mut (a, window), args (column, row), {        settle_pointer_conflict(&mut a, &window);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    send_card_track_toggle(&mut a, &window, task);

    });

    // The create-row field: keystrokes mirror into the Rust buffer
    // (D9 rule 1); Return commits.
    install_seam!(window: window.on_board_create_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (column, text), {        let _ = column;
    a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_create_committed, adapter: adapter;
    bind mut (a, window), args (column, text), {        let _ = column;
    commit_text(&mut a, &window, &text);

    });
}

/// The L2 pointer-seam settle (round-3 R1/R2, plan §3.3): every seam
/// that bypasses `interpret` settles a live flow through the same
/// per-kind rule before its own semantics; idempotent on
/// `Flow::None` (the double-settle guard).
fn settle_pointer_conflict(adapter: &mut Adapter, window: &AppWindow) {
    settle_pointer_conflict_keeping_selection(adapter, window, false);
}

/// The context-menu variant (UX-R8 D-11): opening a task's context
/// menu settles a live flow per the pointer rule but *keeps* the
/// selection — the multi-selection menu (US-4/S3) reads it, and the
/// exit restores the invoking card with the selection still driving
/// the menu's grammar on reopen.
fn settle_pointer_conflict_keeping_selection(
    adapter: &mut Adapter,
    window: &AppWindow,
    keep_selection: bool,
) {
    // The r4 D24 rule governs the date editor's own exits — a foreign
    // begin (the description field taking focus, journey 13's steal)
    // is one: settle per the commit/discard rule, not the
    // unrelated-settle clean close.
    if matches!(adapter.view_state.flow(), Flow::EditDate { .. }) {
        settle_date_editor(adapter, window);
    }
    // The M9 dirty-close law: the leaving settle commits the armed
    // effort draft (R-9's autosave on leaving the field; the guard
    // makes an unchanged value dispatch nothing). The focus-loss
    // seam's echo here is the `[try]`-absorbed shape — this direct
    // call is the commit's owner.
    commit_effort_editor(adapter, window);
    let state = adapter.state.load_full();
    let selection = adapter.view_state.take_selection();
    adapter.settle_flow(window, &state);
    if keep_selection {
        adapter.view_state.restore_selection(selection);
    }
}

/// A board card at (column, row): resolves the id tables (D2) and
/// applies the click's selection state. Returns the addressed
/// `(stack, task)`, or `None` for a stale index against an older
/// tables pass.
fn focus_board_card(
    adapter: &mut Adapter,
    window: &AppWindow,
    column: i32,
    row: i32,
) -> Option<(StackId, TaskId)> {
    settle_pointer_conflict(adapter, window);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return None;
    };
    let (stack, task) = adapter.tables.board_task(column, row)?;
    // UX-R8 D-4's plain-click cell: the left click resets the
    // selection (the focus still moves — the open posture rides the
    // action layer). The right-click context funnel calls
    // `activate_card` directly and keeps the selection (D-11).
    adapter.view_state.reset_selection();
    let outcomes = adapter.view_state.activate_card(task, stack);
    adapter.apply_outcomes(window, &outcomes);
    Some((stack, task))
}

/// Header/switcher activation: select the column's stack (D8).
fn select_column(adapter: &mut Adapter, window: &AppWindow, column: i32) {
    settle_pointer_conflict(adapter, window);
    let Some(stack) = adapter
        .tables
        .stack_ids
        .get(usize::try_from(column).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    let state = adapter.state.load_full();
    let outcomes = adapter.view_state.select_stack(&state, stack);
    adapter.apply_outcomes(window, &outcomes);
}

/// The text flows' commit path (D9): mirror the final buffer, commit
/// through the view state, apply the outcomes (a commit outside a
/// flow is inert — `commit_edit` ends nothing).
fn commit_text(adapter: &mut Adapter, window: &AppWindow, text: &str) {
    // The buffer mirrors first, then the settle: if a live flow is
    // the field's own, the settle commits exactly this text and the
    // trailing commit is the idempotent no-op.
    adapter.view_state.set_buffer(text.to_owned());
    settle_pointer_conflict(adapter, window);
    let state = adapter.state.load_full();
    let outcomes = adapter.view_state.commit_edit(&state);
    adapter.apply_outcomes(window, &outcomes);
}

fn install_stack_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // A card click: focus the row's task (selection state only, §7.1);
    // activation routes `task.edit` after it.
    install_seam!(window: window.on_stack_task_clicked, adapter: adapter;
    bind mut (a, window), args (row), {        activate_stack_row(&mut a, &window, row);

    });
    install_seam!(window: window.on_stack_task_activated, adapter: adapter;
    bind mut (a, window), args (row), {        activate_stack_row(&mut a, &window, row);
    a.route(&window, ActionId::TaskEdit);

    });

    install_seam!(window: window.on_stack_create_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });

    // The search palette (#348): per-keystroke re-rank, outside-click
    // close, and row activation sharing the keyboard Activate path.
    install_seam!(window: window.on_palette_query_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.apply_palette_query(&window, &text);

    });
    // Quick capture (chunk 10 D8): the buffer mirror, the Return
    // commit (keeps the flow), and the #976 outside-click accept
    // (`commit_capture`'s empty-buffer arm closes without commit —
    // the W1 proof-mount's discard-and-close outside click retired);
    // Escape stays the explicit discard seam below.
    install_seam!(window: window.on_capture_text_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_capture_accepted, adapter: adapter; // slint callback: capture-accepted (the Return commit AND the #976 edit-kind outside-click accept, US-1)
    bind mut (a, window), args (), {        let outcomes = a.view_state.commit_capture();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_habit_editor_history_clicked, adapter: adapter; // slint callback: habit-editor-history-clicked (the HF3 W-8 editor History row seam, US-7/S1 — the shared opener)
    bind mut (a, window), args (), {        a.open_habit_history_from_subject(&window);

    });
    install_seam!(window: window.on_habit_history_month_flipped, adapter: adapter; // slint callback: habit_history_month_flipped (the HF3 W-8 pager's flip seam, US-7/S3 — the adapter clamps against the can-page facts)
    bind mut (a, window), args (delta), {        let month_open = a.view_state.habit_history().1.is_some();
        if month_open {
            let clamped = {
                let (habit, month) = a.view_state.habit_history();
                let state = a.state.load();
                let day = taskboard_domain::DayCalendar {
                    day_start_time: a.day_start.0,
                    offset: a.day_start.1,
                };
                let now = a.day_ctx().now;
                month
                    .zip(habit)
                    .is_some_and(|(month, habit)| {
                        // The clamp reads the CURRENT month's can-page
                        // facts: a flip is legal exactly when the side
                        // being paged toward has room (US-7/S3 — no
                        // wrap, the ends hold).
                        let visible = crate::presentation::habit_history_month(
                            state.habits.habits.get(&habit).expect("open target"),
                            &state.habits.log,
                            month,
                            now,
                            day,
                        );
                        (delta < 0 && visible.can_page_back)
                            || (delta > 0 && visible.can_page_forward)
                    })
            };
            if clamped {
                let outcomes = a.view_state.flip_habit_history_month(delta);
                a.apply_outcomes(&window, &outcomes);
            }
        }

    });
    install_seam!(window: window.on_habit_history_close_requested, adapter: adapter; // slint callback: habit_history_close_requested (the HF3 W-8 read-role dismissal seam, US-7/S4 — zero state change)
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_habit_history();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_capture_dismiss_requested, adapter: adapter; // slint callback: capture-dismiss-requested (the ModalDialogHost mount's scrim dismissal seam, round-9 W1)
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_capture();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_palette_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_palette();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_palette_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let target = a
        .tables
        .palette
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied();
    let state = a.state.load();
    let outcomes = match target {
        Some(target) => a.view_state.palette_jump(&state, target),
        None => a.view_state.close_palette(),
    };
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_stack_create_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
}

/// A stack-detail card click: focus the row's task (selection state
/// only, §7.1).
fn activate_stack_row(adapter: &mut Adapter, window: &AppWindow, row: i32) {
    let Some(task) = adapter
        .tables
        .stack_task(usize::try_from(row).unwrap_or(usize::MAX))
    else {
        return;
    };
    let Some(stack) = adapter.view_state.subject_stack() else {
        return;
    };
    let outcomes = adapter.view_state.activate_card(task, stack);
    adapter.apply_outcomes(window, &outcomes);
}

/// The task-detail title field: mirror + commit (D14). The rule-list
/// composition's seams (chunk 6, D13): index/preset/custom resolution
/// against the id tables and the domain grammar, then the one-shot +
/// route — the banner-seam template.
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat for review
fn install_task_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The closed title's click (round-3 W-SM-d, F26): routes the
    // `task.title-edit` action — click ≡ key (§7.1); re-arming after
    // one close is always possible.
    // The focus-loss settle (round-3 L1, plan §7): every text flow's
    // field forwards here. The `try_borrow_mut` absorb discipline
    // (the drag_finished echo precedent): a settle's unmount fires
    // the teardown `focus-lost` synchronously inside
    // `apply_outcomes`' borrow — that echo must be absorbed, and it
    // can never mask a legitimate settle (a genuine focus move
    // happens outside any borrow, where the borrow succeeds).
    install_seam!(window: window.on_text_flow_focus_lost, adapter: adapter; [try]
    bind mut (a, window), args (), {        let state = a.state.load_full();
    a.settle_flow(&window, &state);

    });
    install_seam!(window: window.on_task_title_edit_activated, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::TaskTitleEdit);

    });
    // The priority pickers' seams (UX R-3 D-1, #788): the echo rows'
    // edit affordances route the landed actions (click ≡ key), the
    // slot click commits the exact `UpdateTask` mask immediately
    // (the rule-preset posture — recoverable through `AppUndo`), and
    // the dismissal closes without dispatching.
    install_seam!(window: window.on_task_importance_edit_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::TaskImportanceEdit);

    });
    install_seam!(window: window.on_task_urgency_edit_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::TaskUrgencyEdit);

    });
    install_seam!(window: window.on_priority_slot_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let Some(field) = a.priority_open else {
            return; // a stale click against a closed picker
        };
        let slot = usize::try_from(index).unwrap_or(usize::MAX);
        let command = match (field, slot) {
            // Slot 0 is the unset twin — the CLI's `none` (the
            // parity contract, D-5); 1..=5 are the domain levels in
            // declaration order.
            (crate::router::PriorityField::Importance, 0) => {
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        importance: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
            (crate::router::PriorityField::Urgency, 0) => {
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        urgency: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
            (crate::router::PriorityField::Importance, slot) => {
                let Some(level) = slot_importance(slot) else {
                    return; // a stale index against the closed vocabulary
                };
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        importance: Some(Some(level)),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
            (crate::router::PriorityField::Urgency, slot) => {
                let Some(level) = slot_urgency(slot) else {
                    return;
                };
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        urgency: Some(Some(level)),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
        };
        // The commit closes the picker (the choice is made — the
        // echo re-renders on the next pass). The forced pass carries
        // the flag flip (the armed form's rule: a view-state change
        // outside the state Arc never rides the identity guard).
        a.priority_open = None;
        if a.commands.try_send(command).is_err() {
            tracing::warn!("priority slot commit dropped: command channel full or closed");
        }
        a.refresh_forced(&window);

    });
    // The labels section/overlay seams (UX R-3 D-2, #789): the add
    // affordance routes `label.manage` (click == key); the chip x
    // and the overlay toggles dispatch the Assign/Unassign pair
    // over the VIEW'S SUBJECT, resolved through the pass's tables
    // (the id-table seam law — a stale index absorbs typed).
    install_seam!(window: window.on_labels_manage_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::LabelManage);

    });
    install_seam!(window: window.on_labels_overlay_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::LabelOverlayDismiss);

    });
    install_seam!(window: window.on_chip_unassign_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let Some(label) = a
            .tables
            .task_labels
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        if a
            .commands
            .try_send(StateCommand::UnassignLabel { task, label })
            .is_err()
        {
            tracing::warn!("chip unassign dropped: command channel full or closed");
        }
        a.finish_mutation(&window);

    });
    install_seam!(window: window.on_labels_overlay_row_toggled, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let Some(label) = a
            .tables
            .label_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // The row's pushed state names the pair's arm (D-2): the
        // engine's idempotence owns the rest — the CLI's own
        // contract, no GUI-side guard re-derives it.
        let assigned = a
            .state
            .load()
            .tasks
            .get(&task)
            .is_some_and(|t| t.live_labels().contains(&label));
        let command = if assigned {
            StateCommand::UnassignLabel { task, label }
        } else {
            StateCommand::AssignLabel { task, label }
        };
        if a.commands.try_send(command).is_err() {
            tracing::warn!("label toggle dropped: command channel full or closed");
        }
        a.finish_mutation(&window);

    });
    // The labels form's seams (UX R-3 D-2/D-5, #789): the row verbs
    // arm the in-place draft (the adapter owns it — the due buffers'
    // pattern), the commit funnels through the one sparse-mask
    // assembly (the changed slots only — the LabelChanges
    // discipline; a no-op edit closes without dispatching), and an
    // empty title is the typed inert (the form stays open — the
    // entries commit's absorbed-miss convention).
    install_seam!(window: window.on_labels_overlay_add_requested, adapter: adapter;
    bind mut (a, window), args (), {        if !matches!(a.view_state.flow(), crate::view_state::Flow::Labels { .. }) {
            return;
        }
        a.label_form = Some(LabelFormState {
            seed: None,
            title: String::new(),
            color: crate::presentation::SWATCHES[0].to_owned(),
        });
        a.label_delete_armed = None;
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_labels_overlay_edit_requested, adapter: adapter;
    bind mut (a, window), args (index), {        if !matches!(a.view_state.flow(), crate::view_state::Flow::Labels { .. }) {
            return;
        }
        let Some(label) = a
            .tables
            .label_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        let state = a.state.load_full();
        let Some(label) = state.labels.get(&label) else {
            return; // the roster pass's label died mid-interaction
        };
        a.label_form = Some(LabelFormState {
            seed: Some(label.id),
            title: label.title.clone(),
            color: label.color.as_str().to_owned(),
        });
        drop(state);
        a.label_delete_armed = None;
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_labels_overlay_title_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        if let Some(form) = a.label_form.as_mut() {
            form.title = text.to_string();
        }

    });
    // The free-form colour draft (tag-color plan D-5, #839): the
    // draft buffer takes the typed text, then the forced refresh
    // re-echoes the validity/preview halves — the field's text is
    // mount-only-read (never push-ward while typing), so the refresh
    // cannot fight the caret (the swatch-clicked shape; the
    // title-edited bare-mut stays bare because no pushed half reads
    // the title buffer).
    // slint: colour-edited
    install_seam!(window: window.on_labels_overlay_colour_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(form) = a.label_form.as_mut() {
            form.color = text.to_string();
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_labels_overlay_swatch_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(swatch) =
            crate::presentation::SWATCHES.get(usize::try_from(index).unwrap_or(usize::MAX))
        else {
            return; // a stale palette index
        };
        if let Some(form) = a.label_form.as_mut() {
            form.color = swatch.to_string();
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_labels_overlay_form_commit_requested, adapter: adapter; [else ( false)]
    bind mut (a, window), args (), {        let Some(form) = a.label_form.clone() else {
            return false; // a stale commit against a disarmed form
        };
        let title = form.title.trim().to_owned();
        if title.is_empty() {
            return false; // the typed inert — the form stays open
        }
        // The mint at the boundary (tag-color plan D-6, US-1/AC-2):
        // an invalid colour is the typed inert — the form stays open,
        // nothing dispatches (the empty-title inert's twin).
        let Ok(color) = taskboard_domain::Color::parse(&form.color) else {
            return false;
        };
        let state = a.state.load_full();
        // Labels are board-scoped (D-2/D-6, tag-color plan): the
        // create board is the flow subject's — the task mount derives
        // the subject task's board; the roster-only mount names its
        // board directly (no task subject needed).
        let board = if let Some(crate::view_state::LabelOverlaySubject::Board(board)) =
            a.view_state.labels_subject()
        {
            board
        } else {
            let Some(task) = a.view_state.subject_task() else {
                return false;
            };
            let Some(task) = state.tasks.get(&task) else {
                return false;
            };
            let Some(board) = state.stacks.get(&task.stack).map(|stack| stack.board) else {
                return false;
            };
            board
        };
        let command = match form.seed {
            None => StateCommand::CreateLabel {
                board,
                title: title.clone(),
                color: color.clone(),
            },
            Some(id) => {
                let Some(label) = state.labels.get(&id) else {
                    return false; // the seeded label died mid-edit
                };
                // The sparse-mask discipline (D-2): only the changed
                // slots set; a no-op edit closes without dispatching.
                let title_change = (label.title != title).then(|| title.clone());
                // The comparison reads the minted draft: a
                // case-only-different draft equals the stored
                // lowercase string, so the sparse mask stays honest
                // (the mint's normalization, plan D-6).
                let color_change = (label.color.as_str() != color.as_str())
                    .then(|| color.clone());
                if title_change.is_none() && color_change.is_none() {
                    a.label_form = None;
                    a.refresh_forced(&window);
                    return true;
                }
                StateCommand::UpdateLabel {
                    id,
                    changes: taskboard_domain::LabelChanges {
                        title: title_change,
                        color: color_change,
                    },
                }
            }
        };
        a.label_form = None;
        if a.commands.try_send(command).is_err() {
            tracing::warn!("label form commit dropped: command channel full or closed");
            return false;
        }
        a.finish_mutation(&window);
        true

    });
    install_seam!(window: window.on_labels_overlay_form_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_form.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The armed delete (UX R-3 D-2, #789): the r2 grammar on the
    // overlay rows — the first activation arms the row (the
    // consequence-naming strip; nothing dispatches), the Confirm
    // dispatches the exact `DeleteLabel` (the verb is
    // `NotUndoable(RestoreClass)` — staging is the requirement), and
    // Cancel/Escape disarms. Arming a different row re-arms.
    install_seam!(window: window.on_labels_overlay_delete_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(label) = a
            .tables
            .label_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // US-6 re-arm guard (#840, D-11): a stale-index/dead-label
        // activation — the id no longer resolving in the live roster —
        // never re-arms (the stale-index guard the edit seam already
        // uses); the fast double-click's second activation against the
        // deleted row cannot resurrect the armed identity.
        {
            let state = a.state.load_full();
            if state.labels.get(&label).is_none_or(|l| l.deleted) {
                return;
            }
        }
        // Arming swaps out an open form (the surfaces are
        // mutually exclusive).
        a.label_form = None;
        if a.label_delete_armed == Some(label) {
            // The second activation IS the confirm.
            a.label_delete_armed = None;
            if a
                .commands
                .try_send(StateCommand::DeleteLabel { id: label })
                .is_err()
            {
                tracing::warn!("label delete dropped: command channel full or closed");
            }
        } else {
            a.label_delete_armed = Some(label);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_labels_overlay_delete_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_delete_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The restore sections' seams (the undo-family journey plan
    // W-2/W-9): the armed-confirm grammar over the published roster's
    // row order — the index resolves against the same filtered row
    // list the push derived (the publish lockstep bounds the race,
    // I-2); a stale index is the typed inert (nothing dispatches).
    // The first activation arms (the strip mounts), the Confirm
    // dispatches the exact restore command, Cancel disarms (the
    // D-2 grammar).
    install_seam!(window: window.on_label_restore_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(index_us) = usize::try_from(index).ok() else {
            return;
        };
        // The same board-scoped row source the push derived.
        let overlay_subject = a.view_state.labels_subject();
        let subject_board = match overlay_subject {
            Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
            _ => a
                .view_state
                .subject_task()
                .and_then(|t| a.state.load_full().tasks.get(&t).cloned())
                .and_then(|t| a.state.load_full().stacks.get(&t.stack).cloned())
                .map(|stack| stack.board),
        };
        let state = a.state.load_full();
        let row_id = state
            .roster
            .tombstoned_labels
            .iter()
            .filter(|row| {
                state
                    .labels
                    .get(&row.id)
                    .is_some_and(|label| Some(label.board) == subject_board)
            })
            .nth(index_us)
            .map(|row| row.id);
        drop(state);
        let Some(label) = row_id else {
            return; // a stale index against an older roster pass
        };
        if a.label_restore_armed == Some(label) {
            // The second activation IS the confirm: the exact
            // `RestoreLabel` (the receipt rides the unchanged
            // outcome-notice lane, W-6).
            a.label_restore_armed = None;
            if a
                .commands
                .try_send(StateCommand::RestoreLabel { id: label })
                .is_err()
            {
                tracing::warn!("label restore dropped: command channel full or closed");
            }
        } else {
            a.label_restore_armed = Some(label);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_label_restore_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_restore_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_entry_restore_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(index_us) = usize::try_from(index).ok() else {
            return;
        };
        let state = a.state.load_full();
        let journal_id = state
            .roster
            .deleted_entries
            .get(index_us)
            .map(|row| row.journal_id);
        drop(state);
        let Some(from) = journal_id else {
            return; // a stale index against an older roster pass
        };
        if a.entry_restore_armed == Some(from) {
            // The second activation IS the confirm: the zero-input
            // payload carries the journal id only (D-5) — the engine
            // reads the pre-image and applies the pinned
            // `RestoreTimeEntry` internally (the `EntryRestored`
            // receipt rides the unchanged lane, W-12).
            a.entry_restore_armed = None;
            if a
                .commands
                .try_send(StateCommand::RestoreTimeEntryFromJournal { from })
                .is_err()
            {
                tracing::warn!("entry restore dropped: command channel full or closed");
            }
        } else {
            a.entry_restore_armed = Some(from);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_entry_restore_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.entry_restore_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The clear-all seam (W-4): the row affordance and the armed
    // strip's Confirm converge here — unarmed it arms (subject-scoped,
    // zero rules is the disabled row), armed it dispatches the exact
    // `ClearReminderRules { task }` once (UF1's pre-images make the
    // clear one undo step, W-7).
    install_seam!(window: window.on_rules_clear_asked, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let has_rules = {
            let state = a.state.load();
            state.reminders.rules.values().any(|rule| rule.task == task)
        };
        if !has_rules {
            return; // the disabled row's inert (D-2)
        }
        if a.rules_clear_armed {
            a.rules_clear_armed = false;
            if a
                .commands
                .try_send(StateCommand::ClearReminderRules { task })
                .is_err()
            {
                tracing::warn!("rules clear dropped: command channel full or closed");
            }
        } else {
            a.rules_clear_armed = true;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_rules_clear_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.rules_clear_armed {
            a.rules_clear_armed = false;
            a.refresh_forced(&window);
        }

    });
    // The effort row's seams (task-detail-editor UX plan US-3/D-8,
    // #838). The open funnel: settle any armed date editor (the
    // exclusivity posture — one editor at a time), seed the buffer
    // from the effort's domain text form (empty = the clear seed),
    // refresh. Both fronts land here — the row affordance's seam and
    // nothing else (pointer-grade row verb, the rule-row posture).
    // The buffer mirror (the date-editor buffer pattern verbatim).
    // slint: task-effort-edited
    install_seam!(window: window.on_task_effort_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The lazy editor (M9): the always-on row has no open seam —
        // the first keystroke arms the buffer over the subject, the
        // seed being the stored value (the D-5 guard's baseline).
        if a.effort_editor.is_none() {
            let Some(task) = a.view_state.subject_task() else {
                return; // the subjectless detail: the typed inert
            };
            let state = a.state.load_full();
            let seed = state
                .tasks
                .get(&task)
                .and_then(|t| t.effort.as_ref())
                .map(|e| e.text_form())
                .unwrap_or_default();
            a.effort_editor = Some(EffortEditorState {
                task,
                buffer: seed.clone(),
                last_committed: seed,
            });
        }
        if let Some(editor) = a.effort_editor.as_mut() {
            editor.buffer = text.to_string();
        }
        a.refresh_forced(&window);

    });

    // The commit funnel: a valid non-empty buffer dispatches the
    // sparse `UpdateTask { effort: Some(Some(e)) }`; the empty buffer
    // commits the clear twin `Some(None)` (the walk placement's
    // empty-clear posture); an invalid non-empty value keeps the
    // buffer and dispatches nothing (the date-editor refusal
    // posture). The commit's consume is the editor's close (D-H2).
    // slint: task-effort-committed
    install_seam!(window: window.on_task_effort_committed, adapter: adapter;
    bind mut (a, window), args (), {        commit_effort_editor(&mut a, &window);

    });

    // The focus-loss settle (the M9 always-on row's commit funnel:
    // parseable buffers commit, unparseable ones refuse in place —
    // the typed text stays for the user to fix).
    install_seam!(window: window.on_task_effort_focus_lost, adapter: adapter; [try] // slint callback: task-effort-focus-lost (the always-on row's commit seam, round-9 W-4)
    bind mut (a, window), args (), {        // The D-5 law: the focus-loss IS the commit — the guard makes a
        // blur of an unchanged value dispatch nothing, so Return +
        // blur never double-dispatch. An unparseable buffer
        // dispatches nothing (the refusal posture; the typed text
        // stays in the field for the user to fix).
        commit_effort_editor(&mut a, &window);

    });

    // `[try]` (round-9 chunk 15): the widget's walk-away close can
    // echo this seam while the adapter is mid-pass holding its own
    // borrow (the focus push that moved the walk away); the echo is
    // absorbed — the mid-pass context settles its own flag, and a
    // later user-driven close converges (the CancelMove arm reads the
    // flag directly).
    install_seam!(window: window.on_priority_dismiss_requested, adapter: adapter; [try]
    bind mut (a, window), args (), {        if a.priority_open.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_task_title_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_task_title_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });

    // The description editor's seams (round-3 D15/#228): the closed
    // body's click arms the multiline field (buffer seeded by the
    // view state); Return and the checkmark funnel through
    // `commit_description` — the accepted text is the field's own
    // buffer read (the funnel re-reads the view-state buffer, kept
    // current by the edited seam).
    install_seam!(window: window.on_task_description_edit_opened, adapter: adapter; // slint: task-description-edit-opened
    bind mut (a, window), args (), {        a.begin_description_edit(&window);

    });
    install_seam!(window: window.on_task_description_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_task_description_committed, adapter: adapter;
    bind mut (a, window), args (_text), {        let outcomes = a.view_state.commit_description();
    a.apply_outcomes(&window, &outcomes);

    });

    // The rule cursor's pointer seam (J3 US-6 D-4): a row-body click
    // moves the selected-rule cursor onto that row (id resolved from
    // the tables at the row's index — the remove seam's resolution
    // pattern, one helper shape, two callers). The forced refresh
    // carries the emphasis (the unchanged-refresh guard would eat the
    // pass — the offset-edited seam's posture).
    install_seam!(window: window.on_task_rule_row_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        if let Some(rule) = a
        .tables
        .task_rules
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    {
        a.view_state.select_rule_cursor(rule);
        a.refresh_forced(&window);
    }

    });

    install_seam!(window: window.on_task_rule_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
        .tables
        .task_rules
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index against an older tables pass
    };
    // The pointer coherence write (J3 US-6 D-4): the icon click moves
    // the cursor onto the clicked row first — pointer and cursor can
    // never disagree about which row is selected.
    a.view_state.select_rule_cursor(rule);
    a.rule_target = Some(rule);
    a.route(&window, ActionId::ReminderRuleRemove);

    });

    install_seam!(window: window.on_rule_add_preset_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(dur) = crate::presentation::RULE_PRESETS
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale preset index
    };
    a.rule_add_when = Some(crate::presentation::RuleWhen::DueOffset(
        ReminderOffset::before(-dur).expect("preset magnitudes negate into the domain"),
    ));
    a.route(&window, ActionId::ReminderRuleAdd);

    });

    install_seam!(window: window.on_rule_add_custom_accepted, adapter: adapter; [else ( false)]
    bind mut (a, window), args (text), {        // A parse miss dispatches nothing (absorbed — D12): the UI
    // owns no error chrome; the CLI owns typed failures. The miss
    // rides the seam's return (r2 D24): the composition clears its
    // own buffer, so the field never looks stuck. The three-form
    // grammar (r4 D30/D6): bare stays due-anchored, in/at mint the
    // absolute kind — and `In` carries a duration, the planner is
    // the only instant-stamping site.
    let Some(when) =
        crate::presentation::parse_rule_when(&text, a.day_ctx().now, &chrono::Local)
    else {
        a.rule_offset_draft.clear();
        return false;
    };
    a.rule_offset_draft.clear();
    a.rule_add_when = Some(when);
    a.route(&window, ActionId::ReminderRuleAdd);
    true

    });

    install_seam!(window: window.on_rule_offset_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.rule_offset_draft = text.to_string();
    a.refresh_forced(&window);

    });
}

/// The date editor's seams (r2 D25, round-3 D8). The commit funnel
/// is the one `commit_date` below: the typed Return path, the picker's
/// Enter, and the day cells' clicks all end in it — click ≡ key by
/// construction. The picker's cursor/month/time live on the adapter
/// (R-d) and only ride the refresh push. Due and scheduled share
/// every seam; the flow's `DateField` is read only where the
/// `TaskChanges` mask differs and where the anchor value is read.
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat for review
/// The picker grid's focus seam (round-3 D25, plan §5.3): the
/// grid `FocusScope`'s `changed has-focus` mirrors onto the shell —
/// the `DatePicker` context rides the chain head exactly while the
/// grid holds focus, and never masks (Escape resolves `ModeNormal`
/// from `ViInsert` regardless).
fn install_picker_grid_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_date_picker_grid_focus_changed, adapter: adapter;
    bind mut (a, window), args (focused: bool), {        a.shell.date_picker_grid = focused;
    // The mirror rides with the seam (the Down-in-field path
    // lands here without a `sync` around it).
    window
        .global::<KeyboardState>()
        .set_date_picker_grid(focused);

    });
}

#[allow(clippy::too_many_lines)] // the r2 D25 seam family, kept flat
fn install_due_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_task_due_edit_opened, adapter: adapter; // slint: task-due-edit-opened
    bind mut (a, window), args (), {        a.begin_date_edit(&window, crate::view_state::DateField::Due);

    });

    // The rule section's jump affordance (r4 D30): an unarmed row's
    // due-date repair opens the SAME due editor — the callback is the
    // metadata row's due-edit seam under a second name, no new focus
    // plumbing (chunk 6's machinery arms from there).
    install_seam!(window: window.on_task_rule_jump_to_due_clicked, adapter: adapter; // slint: task-rule-jump-to-due-clicked
    bind mut (a, window), args (), {        a.begin_date_edit(&window, crate::view_state::DateField::Due);

    });

    // D31a: the section's un-mute affordance — the focused task is the
    // target (the section always names its subject), the banner's
    // one-shot machinery is not needed.
    install_seam!(window: window.on_task_rule_unmute_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReminderUnmute);

    });

    install_seam!(window: window.on_task_scheduled_edit_opened, adapter: adapter; // slint: task-scheduled-edit-opened
    bind mut (a, window), args (), {        a.begin_date_edit(&window, crate::view_state::DateField::Scheduled);

    });

    install_seam!(window: window.on_task_date_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.buffer = text.to_string();
        editor.echo =
            crate::due_edit::echo(&editor.buffer, chrono::Utc::now(), &chrono::Local);
        // The Buffer-source live sync (r4 D25): a parseable buffer
        // re-seeds the picker per keystroke (the field wins while
        // typing); empty or junk leaves it untouched — the validity
        // flavour carries the state.
        let now = chrono::Utc::now();
        let seeded = editor.picker.as_ref().and_then(|picker| {
            crate::due_edit::sync_buffer_picker(
                &editor.buffer,
                picker,
                crate::due_edit::SyncSource::Buffer,
                now,
                &chrono::Local,
            )
        });
        if let Some((_, Some(seeded))) = seeded {
            editor.picker = Some(seeded);
        }
    }
    // The echo and the seeded picker both ride a full pass now.
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_task_date_accepted, adapter: adapter;
    bind mut (a, window), args (text), {        // The N-4 stamping law (task-detail-editor UX plan US-1/D-3):
        // the commit mint is `parse_stamp` — a bare date stamps the
        // named date's day-window end over the injected day facts.
        match crate::due_edit::parse_stamp(&text, chrono::Utc::now(), &a.calendar()) {
        // A refusal keeps the buffer and dispatches nothing.
        Err(crate::due_edit::DueParseError) => {}
        parsed => commit_date(&mut a, &window, parsed.expect("non-error parse")),
    }

    });

    install_seam!(window: window.on_task_date_picker_toggled, adapter: adapter;
    bind mut (a, window), args (), {        // Both arms set the steal latch (round-5 F8, D-3.4): the
    // calendar button's click grab can settle the armed editor
    // before/around the toggle arrives, ending the flow the toggle
    // is supposed to serve. The open arm additionally hands focus
    // back to the field (the default-open contract: visible,
    // click-armed, idle, the text field keeps focus).
    // Both arms set the steal latch; the take decides the arm.
    let mut picker_was_open = false;
    if let Some(editor) = a.view_state.date_editor_mut() {
        picker_was_open = editor.picker.take().is_some();
        editor.focus_steal = true;
    }
    if picker_was_open {
        a.refresh_forced(&window);
    } else {
        a.open_due_picker(&window, false);
        a.focus_handover = FocusHandover::Field;
        a.refresh_forced(&window);
    }

    });

    install_seam!(window: window.on_task_date_picker_key, adapter: adapter;
    bind mut (a, window), args (code), {        // The click seams carry the widget's int code space; the
    // registry actions arrive as typed `PickerKey` effects on the
    // same function (round-3 D25: one implementation, two
    // dispatch sites).
    let Some(key) = crate::due_edit::DatePickerKey::from_code(code) else {
        return;
    };
    a.apply_picker_key(&window, key);

    });

    install_seam!(window: window.on_date_picker_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        // Outside click (r4 D24, superseding the round-3 D10
    // picker-only rule): the editor settles per the rule —
    // parseable commits, empty/junk discards — and the picker
    // drops with it. In the picker phase the buffer mirrors the
    // picker, so click-away ≡ done, consistent with the field
    // phase.
    settle_date_editor(&mut a, &window);
    let picker_dropped = a
        .view_state
        .date_editor_mut()
        .and_then(|editor| editor.picker.take())
        .is_some();
    if picker_dropped {
        a.refresh_forced(&window);
    }

    });

    install_seam!(window: window.on_date_editor_focus_lost, adapter: adapter;
    bind mut (a, window), args (), {        settle_date_editor(&mut a, &window);

    });

    // The intra-picker click arm (task-detail-editor UX plan US-2/D-6,
    // N-5): the TimeField's field click arms the same one-shot
    // focus-steal latch the programmatic field→grid hand-over uses —
    // the settle funnel consumes it once, so the focus move into the
    // picker's time field never settles the editor (the reproduction
    // journey us2_s1's oracle).
    // slint: task-date-time-field-armed
    install_seam!(window: window.on_task_date_time_field_armed, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.focus_steal = true;
    }
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_task_date_time_edited, adapter: adapter;
    bind mut (a, window), args (text), {        apply_due_time_edited(&mut a, &window, &text);

    });

    // The quick list's typed-shim seam (round-5 D6): the flyout scope
    // forwards the raw key text; the adapter appends it to the mirror
    // and runs the field's own edited logic (the old in-widget shim
    // appended to the field buffer, which the adapter echoes verbatim
    // — the mirror is the same single writer).
    install_seam!(window: window.on_task_date_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        let mut text = a
        .view_state
        .date_editor()
        .map_or(String::from("--:--"), |editor| editor.time_text.clone());
    text.push_str(&key);
    apply_due_time_edited(&mut a, &window, &text);

    });

    // The quick-times chevron's seam (round-5 F8): the latch skips
    // the settle funnel the chevron's click-grab would otherwise
    // trigger, the digit filter disarms for this open (the full list
    // seeds), and the focus nonce re-grabs the fresh panel (the P5
    // record — an init grab alone does not take).
    install_seam!(window: window.on_task_date_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.focus_steal = true;
        editor.time_filter_armed = false;
    }
    a.refresh_forced(&window);
    let nonce = window.get_date_quick_nonce();
    window.set_date_quick_nonce(nonce.wrapping_add(1));

    });

    install_seam!(window: window.on_task_date_time_picked, adapter: adapter;
    bind mut (a, window), args (minutes), {        let Some(time) = crate::due_edit::time_of_minutes(minutes) else {
        return;
    };
    let Some(editor) = a.view_state.date_editor_mut() else {
        return;
    };
    let Some(picker) = editor.picker.as_mut() else {
        return;
    };
    picker.set_time(Some(time));
    // The pick is a picker-source write — the typing session's
    // filter disarms (round-5 F10: the next open seeds unfiltered).
    editor.time_filter_armed = false;
    // Picker-source sync (r4 D26): the canonical rendering wins
    // the field mirror, and the due buffer re-renders from the
    // selection.
    editor.time_text = crate::due_edit::format_time_hm(time);
    sync_buffer_from_picker(&mut a);
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_task_date_time_accepted, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.time_filter_armed = false;
    }
    match crate::due_edit::parse_time_lenient(&text) {
        // A refusal keeps the typed text — the validity flavour
        // makes it visible — and dispatches nothing.
        None => {
            if let Some(editor) = a.view_state.date_editor_mut() {
                editor.time_text = text.to_string();
            }
            a.refresh_forced(&window);
        }
        Some(time) => {
            let Some(editor) = a.view_state.date_editor_mut() else {
                return;
            };
            let Some(picker) = editor.picker.as_mut() else {
                return;
            };
            picker.set_time(Some(time));
            let Some(instant) = picker.commit_instant(&chrono::Local) else {
                a.refresh_forced(&window);
                return;
            };
                    commit_date(&mut a, &window, Some(instant));
        }
    }

    });

    install_seam!(window: window.on_task_date_day_clicked, adapter: adapter;
    bind mut (a, window), args (cell), {        let Some(editor) = a.view_state.date_editor_mut() else {
        return;
    };
    let Some(date) = editor
        .picker
        .as_ref()
        .and_then(|picker| {
            picker
                .grid()
                .get(usize::try_from(cell).unwrap_or(usize::MAX))
                .and_then(|c| c.date)
        })
    else {
        return; // a blank or a stale index
    };
    editor
        .picker
        .as_mut()
        .expect("checked above")
        .set_cursor(date);
    let picker_time = editor
        .picker
        .as_ref()
        .expect("checked above")
        .time();
    // The commit mint: a timed picker carries the picked/stepped time
    // (round-3 D9 — the typed time is data); a date-only picker stamps
    // the law (task-detail-editor UX plan US-1/D-3; #893 supersedes
    // the mint): the clicked day's last covered instant over the
    // injected day facts — the stamp survives the `[now, end)` today
    // membership the raw window end does not. The law applies exactly
    // when no time component exists anywhere in the entry path.
    let instant = if picker_time.is_none() {
        Some(a.calendar().day_last_instant(date))
    } else {
        editor
            .picker
            .as_ref()
            .expect("checked above")
            .commit_instant(&chrono::Local)
    };
    let Some(instant) = instant else {
        a.refresh_forced(&window);
        return;
    };
    commit_date(&mut a, &window, Some(instant));

    });
}

impl Adapter {
    /// The one date-begin funnel (round-3 D11; keymap-parity plan
    /// D-4): settle any live flow, arm `field`'s editor, seed the
    /// buffer with the field's *current value*, refresh, then open
    /// the picker default-open. Both fronts land here — the row
    /// affordance's callbacks (the `// slint:` seams) and the
    /// `task.due-edit`/`task.scheduled-edit` keyboard route — click
    /// ≡ key at the funnel; the subjectless detail is the typed inert
    /// (the flow never arms).
    fn begin_date_edit(&mut self, window: &AppWindow, field: crate::view_state::DateField) {
        settle_pointer_conflict(self, window);
        // The detail exclusivity posture (US-3/D-8): opening a date
        // editor closes the effort editor (one editor at a time).
        self.effort_editor = None;
        let state = self.state.load_full();
        let outcomes = self.view_state.begin_edit_date(&state, field);
        let value = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .and_then(|t| match field {
                crate::view_state::DateField::Due => t.duedate,
                crate::view_state::DateField::Scheduled => t.scheduled,
            });
        if let Some(editor) = self.view_state.date_editor_mut() {
            editor.buffer = crate::due_edit::format_entry(value, &chrono::Local);
            editor.echo = crate::due_edit::echo(&editor.buffer, chrono::Utc::now(), &chrono::Local);
        }
        self.apply_outcomes(window, &outcomes);
        // The picker is default-open on edit entry (round-3 D10) —
        // visible, click-armed, and idle: the text field keeps focus
        // (typing-first preserved). Inert when the begin refused (the
        // flow never armed).
        self.open_due_picker(window, false);
    }

    /// The description editor's begin funnel (round-3 D15; keymap-parity
    /// plan D-4): settle any live flow, then arm the subject's editor
    /// (the buffer seeds view-state side, the `EditDescription` flow's
    /// own rule). Both fronts land here — the closed body's click seam
    /// and the `task.description-edit` keyboard route.
    fn begin_description_edit(&mut self, window: &AppWindow) {
        settle_pointer_conflict(self, window);
        let state = self.state.load_full();
        let Some(task) = self.view_state.subject_task() else {
            return; // the subjectless detail: the typed inert
        };
        let outcomes = self.view_state.begin_edit_description(&state, task);
        self.apply_outcomes(window, &outcomes);
    }

    /// The picker's one key body (round-3 D25): the keyboard's
    /// `picker.*` actions and the widget's click seams both land
    /// here — the same `due_picker` mutations, the commit through the
    /// one `commit_date` funnel, the refresh after every move.
    fn apply_picker_key(&mut self, window: &AppWindow, key: crate::due_edit::DatePickerKey) {
        use crate::due_edit::DatePickerKey as K;
        let Some(editor) = self.view_state.date_editor_mut() else {
            return; // a stale stroke against an already-closed picker
        };
        let Some(picker) = editor.picker.as_mut() else {
            return; // a stale stroke against an already-closed picker
        };
        // A panel interaction may move focus (the IconButtons' click
        // grabs) — the settle funnel must skip that steal (r4 D24).
        editor.focus_steal = true;
        // Grid-Enter is the phase hand-over, never a commit
        // (r4 D25, review focus b): the armed field takes focus
        // via the one-shot flag; the typed path and the combobox
        // own the commits through the one funnel. Day-cell clicks
        // keep their direct `commit_date`.
        let mut hand_over = false;
        match key {
            K::Left => picker.move_days(-1),
            K::Right => picker.move_days(1),
            K::Up => picker.move_days(-7),
            K::Down => picker.move_days(7),
            K::PrevMonth => picker.move_months(-1),
            K::NextMonth => picker.move_months(1),
            K::MonthStart => picker.month_edge(true),
            K::MonthEnd => picker.month_edge(false),
            // The time steppers (round-3 D9, r4 D26): ±15 min,
            // wrapping — the date follows the wrap, an unset time
            // seeds midnight first (r4 D23). Button-only; the combobox
            // row's steppers land on the same codes.
            K::TimeBack => picker.step_time(-1),
            K::TimeForward => picker.step_time(1),
            K::Commit => hand_over = true,
        }
        if hand_over {
            self.focus_handover = FocusHandover::Field;
            self.refresh_forced(window);
            return;
        }
        // The Picker-source live sync (r4 D25): every move re-renders
        // the buffer/echo from the selection, and a stepped time wins
        // the combobox's mirror.
        if matches!(key, K::TimeBack | K::TimeForward)
            && let Some(picker) = editor.picker.as_ref()
        {
            // A picker-source write disarms the digit filter
            // (round-5 F10).
            editor.time_filter_armed = false;
            editor.time_text = picker
                .time()
                .map_or("--:--".to_owned(), crate::due_edit::format_time_hm);
        }
        sync_buffer_from_picker(self);
        self.refresh_forced(window);
    }
}

/// Opens the picker anchored on the armed field's current value (or
/// today) — the calendar button's, the Down-in-field toggle's, and
/// the default-open begin's shared path. `focus_grid` is the one-shot
/// `focus-on-mount` flag: `false` keeps the text field focused
/// (typing-first), `true` hands focus to the grid.
impl Adapter {
    fn open_due_picker(&mut self, window: &AppWindow, focus_grid: bool) {
        let state = self.state.load_full();
        let field = match self.view_state.flow() {
            Flow::EditDate { field, .. } => *field,
            _ => return,
        };
        let anchor = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .and_then(|t| match field {
                crate::view_state::DateField::Due => t.duedate,
                crate::view_state::DateField::Scheduled => t.scheduled,
            });
        self.focus_handover = if focus_grid {
            FocusHandover::Grid
        } else {
            FocusHandover::None
        };
        let Some(editor) = self.view_state.date_editor_mut() else {
            return;
        };
        editor.picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            anchor,
            &chrono::Local,
        ));
        // The combobox's mirror opens on the picker's seeded time —
        // canonical when a non-midnight anchor carries one, the
        // unset echo otherwise (r4 D23's display rule).
        editor.time_text = editor
            .picker
            .as_ref()
            .and_then(super::due_edit::DuePickerState::time)
            .map_or("--:--".to_owned(), crate::due_edit::format_time_hm);
        // A fresh open seeds the quick list unfiltered (round-5 F10):
        // the value mirror is the filter's input, but only a typed
        // keystroke arms it.
        editor.time_filter_armed = false;
        self.refresh_forced(window);
    }
}

/// The due combobox's edited logic (r4 D26), shared by the field's
/// `edited` seam and the quick list's typed-shim seam (round-5 D6 —
/// the shim appends to the mirror, then runs this same body).
fn apply_due_time_edited(a: &mut Adapter, window: &AppWindow, text: &str) {
    // The keystroke arms the digit filter (round-5 F10, D-3.5):
    // narrowing is per-keystroke only, never an open-time seed.
    let Some(editor) = a.view_state.date_editor_mut() else {
        a.refresh_forced(window);
        return;
    };
    editor.time_filter_armed = true;
    // The field's value echo seeds its buffer ("--:--" while no
    // time exists), so the first keystroke's text carries it as a
    // prefix — strip the echo, keep the typed digits.
    let mut text = text.to_string();
    if let Some(stripped) = text.strip_prefix("--:--") {
        text = stripped.to_owned();
    }
    editor.time_text.clone_from(&text);
    // Buffer-source seeding (r4 D26): a lenient-parse hit moves
    // the picker's time live; junk/partial leaves it untouched
    // (the validity flavour shows the state).
    if let Some(time) = crate::due_edit::parse_time_lenient(&text)
        && let Some(picker) = editor.picker.as_mut()
    {
        picker.set_time(Some(time));
    }
    a.refresh_forced(window);
}

/// The picker-source buffer sync (r4 D26, the §4.3 helper's Picker
/// arm in its adapter plumbing until the pure helper lands): the due
/// buffer/echo re-render from the picker's current selection.
fn sync_buffer_from_picker(a: &mut Adapter) {
    let now = chrono::Utc::now();
    let synced = a.view_state.date_editor_mut().and_then(|editor| {
        let picker = editor.picker.as_ref()?;
        crate::due_edit::sync_buffer_picker(
            &editor.buffer,
            picker,
            crate::due_edit::SyncSource::Picker,
            now,
            &chrono::Local,
        )
    });
    if let Some((buffer, None)) = synced
        && let Some(editor) = a.view_state.date_editor_mut()
    {
        editor.echo = crate::due_edit::echo(&buffer, now, &chrono::Local);
        editor.buffer = buffer;
    }
}

/// The D24 settle funnel (r4 D24): the one commit/discard rule for
/// the armed date editor's own seams (focus loss, the dismissal
/// click). A non-empty parseable buffer commits through the one
/// `commit_date`; empty or non-conforming discards — no command, the
/// prior value survives, the editor collapses. The programmatic
/// focus-steal latch (the phase steps, the panel's own controls)
/// consumes the skip: a hand-over never settles the text the user is
/// mid-way through. `true` = committed.
fn settle_date_editor(a: &mut Adapter, window: &AppWindow) -> bool {
    if !matches!(a.view_state.flow(), Flow::EditDate { .. }) {
        return false;
    }
    match a.view_state.date_editor_mut() {
        Some(editor) if editor.focus_steal => {
            editor.focus_steal = false;
            return false;
        }
        _ => {}
    }
    // The typing session ends with the flow (the armed filter never
    // outlives the buffer it filters — D-3.5's funnel passenger).
    if let Some(editor) = a.view_state.date_editor_mut() {
        editor.time_filter_armed = false;
    }
    let text = a
        .view_state
        .date_editor()
        .map_or(String::new(), |editor| editor.buffer.clone());
    let now = chrono::Utc::now();
    // Non-empty and parseable: click-away ≡ Return — the same
    // `parse_stamp` mint the typed Return path uses (the N-4 law, UX
    // plan US-1/D-3: one funnel, one mint; the date-only buffer stamps
    // the window end here too).
    if let Ok(Some(instant)) = crate::due_edit::parse_stamp(&text, now, &a.calendar()) {
        commit_date(a, window, Some(instant));
        return true;
    }
    // Empty never means clear on focus loss (a stray steal must not
    // nuke a due date) and junk cannot linger: both discard — the
    // flow ends clean, the prior value survives. The discard's wipe
    // is the level's drop (D-H2).
    let outcomes = a.view_state.end_edit_date();
    a.apply_outcomes(window, &outcomes);
    false
}

/// The one commit funnel (r2 D25, round-3 D8): dispatch the typed
/// `UpdateTask` change for the flow's field, then end the flow.
/// `None` clears (the empty-entry / x-clear semantics).
fn commit_date(
    a: &mut Adapter,
    window: &AppWindow,
    instant: Option<chrono::DateTime<chrono::Utc>>,
) {
    let task = match a.view_state.flow() {
        Flow::EditDate { task, field } => (*task, *field),
        _ => return,
    };
    let (duedate, scheduled) = match task.1 {
        crate::view_state::DateField::Due => (Some(instant), None),
        crate::view_state::DateField::Scheduled => (None, Some(instant)),
    };
    let command = StateCommand::UpdateTask {
        id: task.0,
        changes: taskboard_domain::TaskChanges {
            duedate,
            scheduled,
            ..taskboard_domain::TaskChanges::default()
        },
    };
    if a.commands.try_send(command).is_err() {
        tracing::warn!("command channel full or closed; dropped");
    }
    // The commit's consume is the flow's end — the level drops with
    // its buffers (D-H2), so the next pass renders the empty editor.
    let outcomes = a.view_state.end_edit_date();
    a.apply_outcomes(window, &outcomes);
}

/// The effort editor's one commit funnel (US-3/D-8): parse through
/// the shared predicate (`parse_effort_row` — the walk placement's
/// parse), dispatch the exact sparse mask, close the editor.
fn commit_effort_editor(a: &mut Adapter, window: &AppWindow) {
    let Some(editor) = a.effort_editor.as_ref() else {
        return;
    };
    let trimmed = editor.buffer.trim().to_owned();
    // The D-5 guard: a commit of an unchanged value dispatches
    // nothing (the always-on row blurs constantly; nothing ever
    // consumed the editor, so the buffer stays).
    if trimmed == editor.last_committed {
        return;
    }
    let effort_change = if trimmed.is_empty() {
        Some(None)
    } else if let Some(effort) = parse_effort_row(&trimmed) {
        Some(Some(effort))
    } else {
        // The refusal: the buffer kept, nothing dispatches
        // (the date-editor refusal posture, AC-2).
        a.refresh_forced(window);
        return;
    };
    let id = editor.task;
    if a.commands
        .try_send(StateCommand::UpdateTask {
            id,
            changes: taskboard_domain::TaskChanges {
                effort: effort_change,
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .is_err()
    {
        tracing::warn!("effort commit dropped: command channel full or closed");
        return;
    }
    // The row stays armed (always-on): the baseline moves to the
    // committed value, so the Return+blur pair never double-dispatches.
    if let Some(editor) = a.effort_editor.as_mut() {
        editor.last_committed = trimmed;
    }
    a.finish_mutation(window);
    a.refresh_forced(window);
}

/// The banner row at `index` (D2: the index resolves against the
/// refresh pass's row table).
fn banner_row(adapter: &Adapter, index: i32) -> Option<crate::presentation::BannerRow> {
    adapter
        .tables
        .banner
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
}

/// The banner seams (D12): row activation (click or Tab focus — the
/// markup's `changed has-focus` lands on the same seam) sets focus
/// state plus the one-shot reminder target (D4); snooze/mute set the
/// target and route the registry actions.
fn install_banner_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_banner_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.view_state.set_focused(Some(banner_row.task));
    a.reminder_target = Some(banner_row.target);
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_banner_snooze_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.reminder_target = Some(banner_row.target);
    a.route(&window, ActionId::ReminderSnooze);

    });

    install_seam!(window: window.on_banner_mute_clicked, adapter: adapter; // #1011 (SB1 US-3): direction-aware — the row folds `state.reminders.muted`, so a muted task's control dispatches the mute-clearing command directly (the #733 twin grammar on the banner surface); the unmuted direction keeps the routed ReminderMute verb.
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    if banner_row.muted {
        a.send(taskboard_domain::StateCommand::SetTaskRemindersMuted {
            task: banner_row.task,
            muted: false,
        });
        a.finish_mutation(&window);
    } else {
        a.reminder_target = Some(banner_row.target);
        a.route(&window, ActionId::ReminderMute);
    }

    });

    // D-6.1 (round 6): the row's double-click open — the today-row
    // activation's shape (focus, then the TaskEdit route). The click
    // that precedes it already focused and armed the one-shot target
    // (both idempotent); TaskEdit → Activate → `RefreshNow` performs
    // the push, so no extra refresh. A stale index stays inert.
    install_seam!(window: window.on_banner_row_open_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.view_state.set_focused(Some(banner_row.task));
    a.reminder_target = Some(banner_row.target);
    a.route(&window, ActionId::TaskEdit);

    });
}

/// The notification row at `index` (D2: the index resolves against the
/// refresh pass's row table).
impl Adapter {
    /// The notification walk's step (US-6/W-6, D-N1): the first
    /// stroke arms at row 0; a step clamps at both ends (no wrap).
    /// The selection is the surface's pushed highlight (the walk's
    /// render).
    fn notification_walk(&mut self, window: &AppWindow, direction: i32) {
        let len = self.tables.notifications.len();
        if len == 0 {
            return;
        }
        let cursor = match self.notification_cursor {
            Some(cursor) => {
                let next =
                    i64::from(u32::try_from(cursor).unwrap_or(u32::MAX)) + i64::from(direction);
                let max = i64::from(u32::try_from(len - 1).unwrap_or(u32::MAX));
                usize::try_from(next.clamp(0, max)).unwrap_or(0)
            }
            None => 0,
        };
        self.notification_cursor = Some(cursor);
        self.sync_view_context(window);
        window.set_notification_selected(i32::try_from(cursor).unwrap_or(0));
        self.finish_mutation(window);
    }

    /// The verb dispatch's shared body (US-6/W-6, D-N1): arms at row
    /// 0 when disengaged, resolves the selected row, and runs exactly
    /// the seam body the row's button callback runs.
    fn apply_notification_verb(
        &mut self,
        window: &AppWindow,
        verb: crate::router::NotificationVerb,
    ) {
        use crate::router::NotificationVerb as V;
        let cursor = *self.notification_cursor.get_or_insert(0);
        let cursor = i32::try_from(cursor).unwrap_or(i32::MAX);
        let Some(row) = notification_row(self, cursor) else {
            return;
        };
        match verb {
            V::Dismiss => self.notification_dismiss(&row),
            V::Done => self.notification_done(&row),
            V::Snooze => self.notification_snooze(window, &row),
            V::Mute => self.notification_mute(window, &row),
            V::Cancel => self.notification_cancel(window, &row),
            V::TrackStart => self.notification_track_start(window, &row),
            V::Open => self.notification_open(window, &row),
        }
        self.finish_mutation(window);
    }

    /// The dismiss seam's body (the row's button callback's dispatch).
    /// `&self` — the `bare_ref` seam install borrows non-mut.
    fn notification_dismiss(&self, row: &crate::presentation::NotificationRow) {
        self.send(taskboard_domain::StateCommand::DismissReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
        });
    }

    /// The done seam's body: the row's task completes; an alarm row
    /// has no task (the button's absence is the presentation's job —
    /// the stray degrades to a no-op).
    fn notification_done(&self, row: &crate::presentation::NotificationRow) {
        let Some(task) = row.task else { return };
        self.send(taskboard_domain::StateCommand::SetTaskDone {
            id: task,
            done: true,
        });
    }

    /// The snooze seam's body: the routed verb over the one-shot
    /// target (the row's anchor mirrors the ref).
    fn notification_snooze(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        self.reminder_target = Some(notification_target(row));
        self.route(window, ActionId::ReminderSnooze);
    }

    /// The mute seam's body (the snooze twin's routed verb).
    fn notification_mute(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        self.reminder_target = Some(notification_target(row));
        self.route(window, ActionId::ReminderMute);
    }

    /// The cancel seam's body: the alarm row's off-switch; a task row
    /// degrades to a no-op (the done-seam's guard shape).
    fn notification_cancel(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        let taskboard_domain::ReminderRef::Adhoc(alarm) = row.reminder else {
            return;
        };
        self.timer_target = Some(alarm);
        self.route(window, ActionId::TimerCancel);
    }

    /// The track-start seam's body: exactly one command leaves the UI
    /// (the implicit dismissal lives planner-side).
    fn notification_track_start(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        let Some(task) = row.task else { return };
        self.track_target = Some(task);
        self.route(window, ActionId::TaskTrackStart);
    }

    /// The row-open seam's body: focus, then the `TaskEdit` route.
    fn notification_open(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        let Some(task) = row.task else { return };
        self.view_state.set_focused(Some(task));
        self.route(window, ActionId::TaskEdit);
    }
}

fn notification_row(adapter: &Adapter, index: i32) -> Option<crate::presentation::NotificationRow> {
    adapter
        .tables
        .notifications
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .cloned()
}

/// The done checkbox's dispatch (r3 D6): the toggled `SetTaskDone`
/// for the addressed task — the raw-send the notification Done button
/// set (the registry's complete/reopen act on the focused task).
/// The importance picker's slot mapping (UX R-3 D-1, #788): slots
/// 1..=5 are the domain's declaration order; anything else is the
/// stale-index absorb (`None`). Slot 0 (unset) is handled at the
/// seam before these run.
fn slot_importance(slot: usize) -> Option<taskboard_domain::Importance> {
    match slot {
        1 => Some(taskboard_domain::Importance::VeryLow),
        2 => Some(taskboard_domain::Importance::Low),
        3 => Some(taskboard_domain::Importance::Normal),
        4 => Some(taskboard_domain::Importance::High),
        5 => Some(taskboard_domain::Importance::VeryHigh),
        _ => None,
    }
}

/// The urgency picker's slot mapping — the importance helper's twin
/// (distinct types by the domain's own ruling, one slot grammar).
fn slot_urgency(slot: usize) -> Option<taskboard_domain::Urgency> {
    match slot {
        1 => Some(taskboard_domain::Urgency::VeryLow),
        2 => Some(taskboard_domain::Urgency::Low),
        3 => Some(taskboard_domain::Urgency::Normal),
        4 => Some(taskboard_domain::Urgency::High),
        5 => Some(taskboard_domain::Urgency::VeryHigh),
        _ => None,
    }
}

/// The card play/pause control's routing (UX-R8 chunk-5 D-6, US-4;
/// re-rooted onto the smart toggle's registry id — J3 R-8, round 9
/// chunk 11): the seam only addresses the target (the one-shot
/// `track_target`, exact even off-focus) and routes
/// `TaskTrackToggle` — the state read and the Start/Stop outcome
/// choice live in the router arm (one semantics, one home; the
/// D-TU5 addressing is unchanged). A running-elsewhere subject's
/// Start refusal is the engine's typed `NoOps` (the dispatch-time
/// gating posture).
fn send_card_track_toggle(
    adapter: &mut Adapter,
    window: &AppWindow,
    task: taskboard_domain::TaskId,
) {
    adapter.track_target = Some(task);
    adapter.route(window, ActionId::TaskTrackToggle);
}

fn send_toggled_done(adapter: &Adapter, task: taskboard_domain::TaskId) {
    let done_now = adapter
        .state
        .load()
        .tasks
        .get(&task)
        .is_some_and(|task| task.done.is_some());
    adapter.send(taskboard_domain::StateCommand::SetTaskDone {
        id: task,
        done: !done_now,
    });
}

/// The timers surface's seams (DR-3/3, D-I8): the form's commit
/// parses the grammar at the seam (the rule-add absorbed-miss
/// convention) and routes; the section's cancel resolves the
/// `IdTables.timers` index; the chip click focuses the today view
/// (the existing `ViewToday` action — no new `UiEffect`).
#[allow(clippy::too_many_lines)] // the seam bundle: one block per intent, no second parser
fn install_timer_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_timers_when_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.timer_when_draft = text.to_string();
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_timers_create_accepted, adapter: adapter; [else ( false)]
    bind mut (a, window), args (label, when_text, alarm_rung), {        // A whitespace-empty label dispatches nothing (D-P14): the raw
    // label never gets rewritten, an empty one never reaches the
    // engine.
    if label.trim().is_empty() {
        return false;
    }
    // A parse miss keeps the timing text (the rule-add seam's
    // return convention; the UI owns no error chrome). The tz is
    // the untested glue edge — the CLI's "only the edge reads
    // Local" shape.
    let Some(when) =
        crate::presentation::parse_timer_when(&when_text, a.day_ctx().now, &chrono::Local)
    else {
        return false;
    };
    // Both outcomes clear the field — the validity draft follows.
    a.timer_when_draft.clear();
    // The rung rides the seam's structural bool (r4 D-T7): the
    // two-rung enum's alarm bit, minted at this trust boundary.
    let insistence = if alarm_rung {
        taskboard_domain::Insistence::Alarm
    } else {
        taskboard_domain::Insistence::Normal
    };
    a.timer_draft = Some(crate::router::TimerDraft {
        label: label.into(),
        when,
        insistence,
    });
    a.route(&window, ActionId::TimerCreate);
    true

    });

    install_seam!(window: window.on_timers_cancel_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        // A stale index degrades to a no-op (the notification_row
    // guard shape).
    let Some(alarm) = a
        .tables
        .timers
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.timer_target = Some(alarm);
    a.route(&window, ActionId::TimerCancel);

    });

    // The chip click focuses the today view: the section sits
    // top-of-page (D-P18), so the existing FocusToday lands it
    // visibly (D-P2).
    install_seam!(window: window.on_timers_chip_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ViewToday);

    });

    // The break-rule seams (T-14, #797): the timers family's own
    // grammar. The create form's commit parses at the trust boundary
    // (the `WorkThreshold` mint — zero/negative unrepresentable) and
    // routes `BreakRuleCreate` over the one-shot draft; the remove
    // stages through the armed confirm (the chunk-2 one-armed-subject
    // rule — arming touches no engine state; only the confirm routes
    // `DeleteBreakRule`, whose cascade is NotUndoable).
    install_seam!(window: window.on_timers_break_create_toggled, adapter: adapter;
    bind mut (a, window), args (), {        a.break_rule_form_open = !a.break_rule_form_open;
    if !a.break_rule_form_open {
        a.break_rule_threshold_draft.clear();
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timers_break_threshold_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.break_rule_threshold_draft = text.to_string();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timers_break_create_accepted, adapter: adapter; [else ( false)]
    bind mut (a, window), args (label, threshold_text, alarm_rung), {        // A whitespace-empty label dispatches nothing (the timer
        // form's D-P14 shape).
        if label.trim().is_empty() {
            return false;
        }
        let Ok(threshold) = taskboard_domain::WorkThreshold::parse_text_form(&threshold_text)
        else {
            // A parse miss keeps the text (the rule-add seam's
            // return convention; the validity flavour renders).
            return false;
        };
        // The roaming board is the section's host scope: the today
        // view is board-scoped, so the active board IS the
        // unambiguous default (the CLI `--board` default's letter,
        // made structural by the view's own scope — stage-2 note on
        // D-1). Board-less, nothing dispatches.
        let Some(board) = a.view_state.board() else {
            return false;
        };
        let insistence = if alarm_rung {
            taskboard_domain::Insistence::Alarm
        } else {
            taskboard_domain::Insistence::Normal
        };
        a.break_rule_draft = Some(crate::router::BreakRuleDraft {
            board,
            label: label.trim().to_owned(),
            threshold,
            insistence,
        });
        a.route(&window, ActionId::BreakRuleCreate);
        a.break_rule_threshold_draft.clear();
        a.break_rule_form_open = false;
        true

    });
    install_seam!(window: window.on_timers_break_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
            .tables
            .break_rules
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // US-9 (W-10, #1066's roster half): the seam re-rooted onto
        // the registry verb — the click moves the roster cursor (the
        // K4 coherence law) and the arm toggle runs in the routed
        // arm, one semantics one home (click ≡ key by construction;
        // the repeat-is-not-the-confirm letter preserved).
        a.view_state.select_break_rule_cursor(rule);
        a.route(&window, ActionId::BreakRuleRemove);

    });
    install_seam!(window: window.on_timers_break_remove_confirmed, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
            .tables
            .break_rules
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            a.break_rule_armed = None; // a stale confirm disarms
            return;
        };
        // The armed identity types the verb (the settings
        // `settings_remove_confirmed` letter): clears the flag BEFORE
        // the send; a stale index (not the armed subject) disarms
        // silently.
        let Some(armed) = a.break_rule_armed.take() else {
            return;
        };
        if armed != rule {
            return;
        }
        a.break_rule_target = Some(rule);
        a.route(&window, ActionId::BreakRuleRemoveConfirm);

    });
    install_seam!(window: window.on_timers_break_remove_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.break_rule_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });

    // The mouse setter's seams (r4 D33): open/dismiss flip the window
    // property (pure presentation, the palette-open precedent); every
    // intent updates the setter's draft state and re-runs the one
    // composer; the commit funnels through `timers-create-accepted` —
    // the popover adds zero parse paths (the one-parser grep
    // contract). Quick presets are quick-fill, not auto-commit (D-T6).
    install_seam!(window: window.on_timers_setter_opened, adapter: adapter;
    bind mut (a, window), args (), {        window.set_timer_setter_open(true);
    a.refresh_forced(&window);
    // The focus hand-off lands after the mount pass (the
    // date-picker's changed-focus idiom; an init grab alone is
    // dropped on a fresh conditional mount).
    let nonce = window.get_timer_setter_focus_nonce();
    window.set_timer_setter_focus_nonce(nonce.wrapping_add(1));

    });
    install_seam!(window: window.on_timer_setter_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.clear_timer_setter();
    window.set_timer_setter_open(false);

    });
    install_seam!(window: window.on_timer_setter_preset_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(minutes) = crate::presentation::TIMER_PRESETS_MIN
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return;
    };
    let text = crate::presentation::preset_when_text(*minutes);
    a.setter_composed = Some(text.clone());
    a.setter_fire = chrono::TimeDelta::try_minutes(i64::from(*minutes))
        .and_then(|step| a.day_ctx().now.checked_add_signed(step));
    a.timer_when_draft = text;
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_time_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The keystroke arms the digit filter (round-5 F10, D-3.5).
    a.setter_filter_armed = true;
    a.setter_time_text = text.to_string();
    a.setter_time = crate::due_edit::parse_time_lenient(&text);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });

    // The quick list's typed-shim seam (round-5 D6): append to the
    // mirror, run the field's own edited logic.
    install_seam!(window: window.on_timer_setter_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        let mut text = a.setter_time_text.clone();
    text.push_str(&key);
    a.setter_filter_armed = true;
    a.setter_time = crate::due_edit::parse_time_lenient(&text);
    a.setter_time_text = text;
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });

    // The quick-times chevron's seam (round-5 F8): the filter
    // disarms for this open; no settle funnel rides the setter
    // surface (the recorded asymmetry — the latch is the due
    // editor's). The nonce re-grabs the fresh panel.
    install_seam!(window: window.on_timer_setter_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        a.setter_filter_armed = false;
    a.refresh_forced(&window);
    let nonce = window.get_timer_setter_quick_nonce();
    window.set_timer_setter_quick_nonce(nonce.wrapping_add(1));

    });
    install_seam!(window: window.on_timer_setter_time_stepped, adapter: adapter;
    bind mut (a, window), args (n), {        // A picker-source write disarms the digit filter (round-5 F10).
    a.setter_filter_armed = false;
    // Seed-on-step (r4 D23's semantics): an unset time anchors at
    // midnight first — the first `+` yields 00:15, the first `−`
    // wraps to 23:45. The wrap stays within the picked day (the
    // day offset is an explicit choice; the step never flips it).
    let time = a.setter_time.unwrap_or(chrono::NaiveTime::MIN);
    let stepped = crate::due_edit::step_minutes(time, n);
    a.setter_time = Some(stepped);
    a.setter_time_text = crate::due_edit::format_time_hm(stepped);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_time_picked, adapter: adapter;
    bind mut (a, window), args (minutes), {        a.setter_filter_armed = false;
    let Some(time) = crate::due_edit::time_of_minutes(minutes) else {
        return;
    };
    a.setter_time = Some(time);
    a.setter_time_text = crate::due_edit::format_time_hm(time);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_day_picked, adapter: adapter;
    bind mut (a, window), args (index), {        a.setter_day_offset = u32::try_from(index).unwrap_or(0);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_insist_picked, adapter: adapter; [bare_mut]
    bind mut (a), args (index), {        a.setter_insist = index == 1;

    });
    let commit = Rc::clone(adapter);
    let commit_weak = window.as_weak();
    window.on_timer_setter_commit_requested(move |label| {
        let Some(window) = commit_weak.upgrade() else {
            return false;
        };
        // The composed grammar is the only text this seam submits —
        // and it rides `timers-create-accepted` verbatim: popover ≡
        // text form ≡ same command (the parity contract, acceptance
        // b). An unset composition never reaches the parser.
        let composed = {
            let a = commit.borrow();
            if label.trim().is_empty() {
                return false;
            }
            a.setter_composed.clone()
        };
        let Some(text) = composed else {
            return false;
        };
        let insist = commit.borrow().setter_insist;
        let ok = window.invoke_timers_create_accepted(label, SharedString::from(text), insist);
        if ok {
            commit.borrow_mut().clear_timer_setter();
            window.set_timer_setter_open(false);
        }
        ok
    });
}

/// The tracking seams (DR-5/4, D-TU11): the chip's stop and the detail
/// view's two verbs route the matching registry actions — the chip is
/// unconditional (`StopTracking` is total), the detail verbs ride the
/// focused task (every detail invocation has the subject focused,
/// D-TU5's fallback path). No seam for the context-menu arms: the
/// generic menu invocation already re-dispatches registry entries over
/// the focused task.
/// The steppers' and the quick-list picks' shared seed-on-step
/// semantics (r4 D23): the parse seeds midnight when unset, ±15
/// minutes wraps over midnight, a pick writes the slot verbatim.
fn apply_time(
    form: &mut TrackFormState,
    which: u8,
    mutation: impl Fn(chrono::NaiveTime) -> chrono::NaiveTime,
) {
    let current = if which == 0 {
        &form.start_text
    } else {
        &form.end_text
    };
    let base = crate::due_edit::parse_time_lenient(current)
        .unwrap_or_else(|| chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("midnight"));
    let text = crate::due_edit::format_time_hm(mutation(base));
    if which == 0 {
        form.start_text = text;
    } else {
        form.end_text = text;
    }
}

#[allow(clippy::too_many_lines)] // one seam per callback, kept flat
fn install_tracking_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_tracking_chip_stop_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStop);

    });

    install_seam!(window: window.on_task_track_start_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStart);

    });

    install_seam!(window: window.on_task_track_stop_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStop);

    });

    // The entries overlay's open affordance (r4 chunk 10, D-TS8):
    // routes `task.track-entries` over the VIEW'S SUBJECT — the
    // one-shot target seam (the notification row's track-start
    // precedent; the subject is not guaranteed focused). The open
    // arm's kiosk/subject refusals are the interpret arms'
    // dispatch-time gating.
    install_seam!(window: window.on_task_track_entries_clicked, adapter: adapter; [try] // slint callback: task-track-entries-clicked (the entries overlay's opener, r4 D-TS6)
    bind mut (a, window), args (), {        a.track_target = a.view_state.subject_task();
    a.route(&window, ActionId::TaskTrackEntries);

    });

    // The dismissal layer's seam (D-TS1): an outside click is a
    // ladder step (form → list; list → close) — the view-state
    // machine resolves it; the armed delete clears with the overlay
    // (the state-truth rule — nothing ticks).
    install_seam!(window: window.on_track_entries_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.track_entry_target = None;
    // The outside click is a ladder step (D-TS1): the form child ends
    // alone (N1's generic rung); the list closes the overlay.
    let state = a.state.load();
    let outcomes = match a.view_state.cancel_top_child(&state) {
        Some(outcomes) => outcomes,
        None => a.view_state.close_track_entries(),
    };
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });

    // The armed delete (D-TS5): the first activate arms (the adapter
    // holds the armed id; the push mirrors the index), the second
    // dispatches `task.track-entry-delete` — the router resolves the
    // one-shot target into the existing `DeleteTimeEntry`. Disarm
    // rides the push (a dropped row or a closed overlay renders
    // unarmed) and the dismissal seam (state-truth, nothing ticks).
    install_seam!(window: window.on_track_entry_delete_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(entry) = a
        .tables
        .track_entries
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index
    };
    if a.track_entry_target == Some(entry) {
        // The confirm: the target fills the one-shot ctx and the
        // router dispatches; the overlay stays open (the
        // multi-delete posture — the row vanishes on refresh).
        a.route(&window, ActionId::TaskTrackEntryDelete);
    } else {
        a.track_entry_target = Some(entry);
        a.refresh_forced(&window);
    }

    });

    // The add seam (D-TS4): opens the unseeded form over the subject;
    // the seeding lands in the same window as row-activation's.
    install_seam!(window: window.on_track_form_add_requested, adapter: adapter;
    bind mut (a, window), args (), {        let Some(subject) = a.view_state.subject_task() else {
        return;
    };
    let outcomes = a.view_state.open_track_entry_form(subject, None);
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);
    a.seed_track_form(None);
    a.refresh_forced(&window);

    });

    // The form's seams (r4 D-TS4): every intent mutates the
    // adapter-owned state and refreshes — the widget renders what it
    // is pushed and reports what the user did (§5.1). The commit
    // rides the one funnel (`commit_track_form`).
    install_seam!(window: window.on_track_form_date_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(form) = a.view_state.track_form_mut() {
        form.date_text = text.to_string();
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_grid_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if a.view_state.picker().is_some() {
        // The close half: the child ends `Cancelled` — the exit rides
        // the one seam and the form ignores it; the refresh is what
        // unmounts the grid.
        let state = a.state.load();
        let outcomes = a.view_state.close_entry_date_picker(&state);
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
        return;
    }
    let outcomes = a.view_state.open_entry_date_picker();
    a.apply_outcomes(&window, &outcomes);
    // Seed the fresh child's picker state (the level mints clean):
    // `date_text`'s anchor — the flattened toggle's noon arithmetic
    // verbatim.
    let date_text = a
        .view_state
        .track_form()
        .map_or(String::new(), |form| form.date_text.clone());
    let anchor = chrono::NaiveDate::parse_from_str(&date_text, "%Y-%m-%d").ok();
    let anchor = anchor.and_then(|d| {
        use chrono::TimeZone as _;
        chrono::Local
            .from_local_datetime(&d.and_hms_opt(12, 0, 0).expect("noon"))
            .single()
            .map(|local| local.with_timezone(&chrono::Utc))
    });
    if let Some(child) = a.view_state.picker_mut() {
        child.picker = crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            anchor,
            &chrono::Local,
        );
    }
    a.refresh_forced(&window);

    });
    // The date-grid overlay's light-dismiss (the overlay migration,
    // M4): the same close half the toggle's close arm rides — the
    // child ends `Cancelled`, the form and its mirrors stay, the
    // refresh unmounts the grid.
    // slint: track-form-grid-dismiss-requested
    install_seam!(window: window.on_track_form_grid_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        if a.view_state.picker().is_some() {
        let state = a.state.load();
        let outcomes = a.view_state.close_entry_date_picker(&state);
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
        // The drop is pure presentation (no command): the flag's
        // unmount is the forced pass's derive — the date-picker
        // dismissal's shape.
        a.refresh_forced(&window);
    }

    });
    install_seam!(window: window.on_track_form_grid_key, adapter: adapter;
    bind mut (a, window), args (code), {        use crate::due_edit::DatePickerKey as K;
        let Some(key) = crate::due_edit::DatePickerKey::from_code(code) else {
        return;
    };
    // The grid's Enter ends the child `Cancelled` (D4): a grid that
    // stayed open under a field-focused Enter would be a half-mounted
    // level — the exact shape the protocol forbids. Nothing rendered
    // changes but the grid closing: its mirror follows the child.
    // The commit is the click's or the typed field's.
    if key == K::Commit {
        let state = a.state.load();
        let outcomes = a.view_state.close_entry_date_picker(&state);
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
        return;
    }
    let Some(child) = a.view_state.picker_mut() else {
        return; // a stale stroke against a closed grid
    };
    let picker = &mut child.picker;
    match key {
        K::Left => picker.move_days(-1),
        K::Right => picker.move_days(1),
        K::Up => picker.move_days(-7),
        K::Down => picker.move_days(7),
        K::PrevMonth => picker.move_months(-1),
        K::NextMonth => picker.move_months(1),
        K::MonthStart => picker.month_edge(true),
        K::MonthEnd => picker.month_edge(false),
        // The grid's Enter was the Cancelled rung above; the time
        // half is unused in the form's date-only picker.
        K::Commit | K::TimeBack | K::TimeForward => {}
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_day_clicked, adapter: adapter;
    bind mut (a, window), args (cell), {        use chrono::TimeZone as _;
        let Some(date) = a
        .view_state
        .picker()
        .and_then(|child| {
            child
                .picker
                .grid()
                .get(usize::try_from(cell).unwrap_or(usize::MAX))
                .and_then(|c| c.date)
        })
    else {
        return; // a blank or a stale index
    };
    // The click completes the child with the picked day (N2): the
    // noon-anchored instant — the parent's consumption formats its
    // local date back (the `LevelResult::Date` round trip), so the
    // cursor poke and the direct write retire with the flattened
    // model.
    let noon = date.and_hms_opt(12, 0, 0).expect("noon");
    let resolved = chrono::Local
        .from_local_datetime(&noon)
        .single()
        .or_else(|| chrono::Local.from_local_datetime(&noon).latest());
    let instant = resolved.map_or_else(
        || chrono::Utc.from_utc_datetime(&noon),
        |local| local.with_timezone(&chrono::Utc),
    );
    let state = a.state.load();
    let outcomes = a.view_state.end_top_flow_with(
        &state,
        crate::surfaces::GuestExit::Completed(Some(
            crate::surfaces::LevelResult::Date(instant),
        )),
    );
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_track_form_start_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The keystroke arms the digit filter (round-5 F10, D-3.5).
    if let Some(form) = a.view_state.track_form_mut() {
        form.start_filter_armed = true;
        form.start_text = text.to_string();
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_end_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The keystroke arms the digit filter (round-5 F10, D-3.5).
    if let Some(form) = a.view_state.track_form_mut() {
        form.end_filter_armed = true;
        form.end_text = text.to_string();
    }
    a.refresh_forced(&window);

    });

    // The quick lists' typed-shim seams (round-5 D6): append to the
    // field's mirror, run the field's own edited logic.
    install_seam!(window: window.on_track_form_start_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        if let Some(form) = a.view_state.track_form_mut() {
        form.start_filter_armed = true;
        let mut text = form.start_text.clone();
        text.push_str(&key);
        form.start_text = text;
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_end_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        if let Some(form) = a.view_state.track_form_mut() {
        form.end_filter_armed = true;
        let mut text = form.end_text.clone();
        text.push_str(&key);
        form.end_text = text;
    }
    a.refresh_forced(&window);

    });

    // The quick-times chevrons' seams (round-5 F8): the filter
    // disarms for this open (the track surface has no settle funnel —
    // the recorded asymmetry). The shared nonce re-grabs the fresh
    // panel (one panel mounts per group at a time — the site
    // discriminator picks which).
    install_seam!(window: window.on_track_form_start_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(form) = a.view_state.track_form_mut() {
        form.start_filter_armed = false;
    }
    a.refresh_forced(&window);
    let nonce = window.get_track_form_quick_nonce();
    window.set_track_form_quick_nonce(nonce.wrapping_add(1));

    });
    install_seam!(window: window.on_track_form_end_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(form) = a.view_state.track_form_mut() {
        form.end_filter_armed = false;
    }
    a.refresh_forced(&window);
    let nonce = window.get_track_form_quick_nonce();
    window.set_track_form_quick_nonce(nonce.wrapping_add(1));

    });
    for (name, which, pick) in [
        ("start_stepped", 0u8, false),
        ("start_picked", 0u8, true),
        ("end_stepped", 1u8, false),
        ("end_picked", 1u8, true),
    ] {
        let seam = Rc::clone(adapter);
        let seam_weak = window.as_weak();
        let handler = move |window: &AppWindow, arg: i32| {
            let mut a = seam.borrow_mut();
            // A picker-source write disarms the field's digit filter
            // (round-5 F10).
            if let Some(form) = a.view_state.track_form_mut() {
                if which == 0 {
                    form.start_filter_armed = false;
                } else {
                    form.end_filter_armed = false;
                }
                if pick {
                    let minutes = arg;
                    apply_time(form, which, |_| {
                        crate::due_edit::time_of_minutes(minutes).unwrap_or_else(|| {
                            chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")
                        })
                    });
                } else {
                    let delta = arg;
                    apply_time(form, which, |time| {
                        crate::due_edit::step_minutes(time, delta)
                    });
                }
            }
            a.refresh_forced(window);
        };
        match name {
            "start_stepped" => {
                let w = seam_weak.clone();
                window.on_track_form_start_stepped(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
            "start_picked" => {
                let w = seam_weak.clone();
                window.on_track_form_start_picked(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
            "end_stepped" => {
                let w = seam_weak.clone();
                window.on_track_form_end_stepped(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
            _ => {
                let w = seam_weak;
                window.on_track_form_end_picked(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
        }
    }

    // The commit funnel (F24): the end field's Return. `true` steps
    // the surface back to the list; `false` keeps the form open with
    // the invalid flavour (the absorbed-miss convention keeps the
    // typed text).
    install_seam!(window: window.on_track_form_commit_requested, adapter: adapter; [else ( false)]
    bind mut (a, window), args (), {        a.commit_track_form(&window)

    });

    // The row-activate seam (D-TS3): opens the seeded edit form
    // (the commit funnel owns its Return); the seed resolves
    // through the id table (never a viewport offset).
    install_seam!(window: window.on_track_entry_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(subject) = a.view_state.subject_task() else {
        return;
    };
    let Some(entry) = a
        .tables
        .track_entries
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    let outcomes = a.view_state.open_track_entry_form(subject, Some(entry));
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);
    a.seed_track_form(Some(entry));
    a.refresh_forced(&window);

    });

    // The task view's done checkbox (r4 D16b/F15): raw-send the
    // toggled SetTaskDone over the VIEW'S SUBJECT — deliberately not
    // the registry route (`TaskComplete`/`TaskReopen` act on the
    // focused task; the cycle's fallback subjects
    // `first_task_of(subject_stack)` when focus is empty, so the
    // subject is not guaranteed focused — the card checkbox's recorded
    // reason, adapted to the view). A stale/dead subject degrades to a
    // no-op (the index-seam precedent). No
    // `settle_pointer_conflict`: the board checkbox settles because
    // the board owns the capture-settle lane; the view's other clicks
    // don't settle and neither does this one.
    // The card play/pause control (UX-R8 chunk-5 D-6): the detail
    // card's seam — the subject task.
    install_seam!(window: window.on_task_track_toggled, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    send_card_track_toggle(&mut a, &window, task);

    });
    install_seam!(window: window.on_task_done_toggled, adapter: adapter;
    bind (a, _window), args (), {            let Some(task) = a.view_state.subject_task() else {
        return;
    };
    if !a.state.load().tasks.contains_key(&task) {
        return;
    }
    send_toggled_done(&a, task);

    });
}

/// The concentration overlay's seams (#496): the scrim close, the
/// note's mirror + capture-commit funnel, the switcher's start (the
/// notification-row `track_target` precedent), and the session stop.
/// Every session verb routes the landed registry action — zero new
/// session commands (the fence).
fn install_focus_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The scrim / Escape / toggle close: presentation only — the
    // session (the open entry) is untouched by construction (the
    // close outcomes carry no command).
    install_seam!(window: window.on_focus_overlay_close_requested, adapter: adapter;
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_focus_mode();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });

    // The quick-note mirror (the capture field's discipline).
    install_seam!(window: window.on_focus_note_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });

    // The note commit: the landed inbox-capture command via the
    // flow's commit funnel — clears the buffer and KEEPS the overlay
    // open (rapid-fire capture semantics; an empty commit is a no-op,
    // never a close).
    install_seam!(window: window.on_focus_note_accepted, adapter: adapter;
    bind mut (a, window), args (), {        let thought = a.view_state.buffer().trim().to_owned();
    if thought.is_empty() {
        return; // no-op: the overlay stays as it is
    }
    let outcomes = vec![
        crate::view_state::FlowOutcome::Command(taskboard_domain::StateCommand::CaptureTask {
            title: thought,
            board: None,
        }),
        crate::view_state::FlowOutcome::RefreshNow,
    ];
    a.view_state.set_buffer(String::new());
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });

    // The switcher's start: the selected row's task as the one-shot
    // `track_target`, then the landed `task.track-start` action (the
    // notification-row Start Task precedent — the router arm maps it
    // to `StartTracking { kind: Work }`).
    install_seam!(window: window.on_focus_switcher_invoked, adapter: adapter;
    bind mut (a, window), args (index), {        let selected = usize::try_from(index).unwrap_or(usize::MAX);
    let state = a.state.load();
    let Some(task) = crate::presentation::focus_switcher_rows(&state)
        .get(selected)
        .map(|row| row.task)
    else {
        return; // a stale cursor past the pool: inert
    };
    a.track_target = Some(task);
    a.route(&window, ActionId::TaskTrackStart);

    });

    // The panel's stop: the payload-free landed stop. The overlay
    // stays open, showing the idle + switcher state (closing the
    // overlay and stopping the session are deliberately orthogonal).
    install_seam!(window: window.on_focus_stop_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStop);

    });
}

/// The review walk's seams (DR-7 chunk 3, #638): every button routes
/// the landed registry action — one dispatch path, the adapter's verb
/// table owns the semantics (the focus overlay's rule). The two text
/// mirrors (the answers, the date field) ride the plan buffers'
/// discipline; the candidate click arms the pick directly (the
/// one-shot target pattern).
/// The journal ledger's seams (T-15, #798): the row activation seeds
/// the inline editor from the subject's live state; the commit builds
/// the exact `journal edit` change mask (an all-unchanged commit is
/// the accepted no-op, clocks untouched); the delete stages through
/// the armed confirm (the tombstone is the family's inverse-of-record,
/// DR-7 D-J7 — the family's own verbs are the recovery surface, and
/// this editor IS that surface arriving in the GUI). No new `ActionId`s:
/// the ledger dispatches the engine commands directly (the labels
/// armed-delete precedent), keeping `ContextId::Review` binding-free.
#[allow(clippy::too_many_lines)] // the seam bundle: one block per intent
fn install_review_ledger_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_reflection_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(id) = a
            .tables
            .reflections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // Seed the buffers from the subject's live state (the echo is
        // the stored row verbatim — never an excerpt).
        let state = a.state.load();
        let Some(entry) = state.reflection.entries.get(&id) else {
            return;
        };
        a.reflection_active = Some(id);
        a.reflection_text_draft.clone_from(&entry.text);
        a.reflection_mood_draft = entry
            .attributes
            .mood
            .map_or_else(String::new, |rating| rating.get().to_string());
        a.reflection_energy_draft = entry
            .attributes
            .energy
            .map_or_else(String::new, |rating| rating.get().to_string());
        a.reflection_flags_add.clear();
        a.reflection_flags_remove.clear();
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_editor_text_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.reflection_text_draft = text.to_string();

    });
    install_seam!(window: window.on_reflection_editor_mood_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.reflection_mood_draft = text.to_string();

    });
    install_seam!(window: window.on_reflection_editor_energy_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.reflection_energy_draft = text.to_string();

    });
    install_seam!(window: window.on_reflection_editor_flag_toggled, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(flag) = crate::presentation::HEALTH_FLAGS
            .get(usize::try_from(index).unwrap_or(usize::MAX))
        else {
            return;
        };
        let Some(id) = a.reflection_active else {
            return;
        };
        let state = a.state.load();
        let Some(entry) = state.reflection.entries.get(&id) else {
            return;
        };
        // The delta is relative to the subject's live set (the mask's
        // halves, never whole-set replace): a member chip toggles the
        // remove half, a non-member the add half.
        if entry.attributes.flags.contains(flag) {
            if !a.reflection_flags_remove.remove(flag) {
                a.reflection_flags_remove.insert(*flag);
            }
        } else if !a.reflection_flags_add.remove(flag) {
            a.reflection_flags_add.insert(*flag);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_editor_committed, adapter: adapter;
    bind mut (a, window), args (), {        let Some(id) = a.reflection_active else {
            return;
        };
        let state = a.state.load();
        let Some(entry) = state.reflection.entries.get(&id) else {
            drop(state);
            a.reflection_active = None; // the subject went stale
            a.refresh_forced(&window);
            return;
        };
        // The text mask: a changed draft rides verbatim (the empty
        // text is the domain's legal clear).
        let text = (a.reflection_text_draft != entry.text)
            .then(|| a.reflection_text_draft.clone());
        // The rating masks: empty = clear (when set), a valid number =
        // set (when changed); an invalid draft refuses the commit (the
        // flavour renders; nothing dispatches).
        let rating_mask = |draft: &str,
                           current: Option<taskboard_domain::reflection::Rating>|
         -> Option<Option<taskboard_domain::reflection::Rating>> {
            if draft.is_empty() {
                current.map(|_| None)
            } else {
                match draft.parse::<u8>().ok().and_then(|n| {
                    taskboard_domain::reflection::Rating::try_new(n).ok()
                }) {
                    Some(rating) if Some(rating) != current => Some(Some(rating)),
                    _ => None,
                }
            }
        };
        let mood_valid = a.reflection_mood_draft.is_empty()
            || a
                .reflection_mood_draft
                .parse::<u8>()
                .ok()
                .and_then(|n| taskboard_domain::reflection::Rating::try_new(n).ok())
                .is_some();
        let energy_valid = a.reflection_energy_draft.is_empty()
            || a
                .reflection_energy_draft
                .parse::<u8>()
                .ok()
                .and_then(|n| taskboard_domain::reflection::Rating::try_new(n).ok())
                .is_some();
        if !mood_valid || !energy_valid {
            return; // an invalid draft never dispatches (the flavour renders)
        }
        // The command's payload carries the raw rating byte (the
        // planner re-mints — the trust boundary is the engine's).
        let mood = rating_mask(&a.reflection_mood_draft, entry.attributes.mood)
            .map(|inner| inner.map(taskboard_domain::reflection::Rating::get));
        let energy = rating_mask(&a.reflection_energy_draft, entry.attributes.energy)
            .map(|inner| inner.map(taskboard_domain::reflection::Rating::get));
        let flags_add = a.reflection_flags_add.clone();
        let flags_remove = a.reflection_flags_remove.clone();
        // An all-unchanged editor commits the accepted no-op (the
        // planner's contract — clocks untouched).
        let command = StateCommand::UpdateReflection {
            id,
            text,
            mood,
            energy,
            flags_add,
            flags_remove,
        };
        if a.commands.try_send(command).is_err() {
            tracing::warn!("journal edit dropped: command channel full or closed");
            return;
        }
        a.reflection_active = None;
        a.reflection_text_draft.clear();
        a.reflection_mood_draft.clear();
        a.reflection_energy_draft.clear();
        a.reflection_flags_add.clear();
        a.reflection_flags_remove.clear();
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_editor_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.reflection_active.take().is_some() {
            a.reflection_text_draft.clear();
            a.reflection_mood_draft.clear();
            a.reflection_energy_draft.clear();
            a.reflection_flags_add.clear();
            a.reflection_flags_remove.clear();
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_reflection_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(id) = a
            .tables
            .reflections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // Arming is a UI-state toggle (the settings armed-item
        // letter): one armed subject surface-wide, arming touches no
        // engine state.
        a.reflection_armed = if a.reflection_armed == Some(id) {
            None
        } else {
            Some(id)
        };
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_remove_confirmed, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(id) = a
            .tables
            .reflections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            a.reflection_armed = None; // a stale confirm disarms
            return;
        };
        // The armed identity types the verb: clears the flag BEFORE
        // the send; a stale index disarms silently.
        let Some(armed) = a.reflection_armed.take() else {
            return;
        };
        if armed != id {
            return;
        }
        if a
            .commands
            .try_send(StateCommand::DeleteReflection { id })
            .is_err()
        {
            tracing::warn!("journal delete dropped: command channel full or closed");
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_remove_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.reflection_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
}

fn install_review_walk_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_walk_abort_arm_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewAbortArm);
    });
    install_seam!(window: window.on_walk_abort_accept_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewAbortAccept);
    });
    install_seam!(window: window.on_walk_abort_dismiss_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewAbortDismiss);
    });
    install_seam!(window: window.on_walk_plan_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewWalkPlanner);
    });
    install_seam!(window: window.on_walk_button_invoked, adapter: adapter; // walk-button-invoked
    bind mut (a, window), args (code), {        let action = match code {
            0 => ActionId::ReviewWalkPrev,
            1 => ActionId::ReviewWalkAdvance,
            2 => ActionId::ReviewFinish,
            3 => ActionId::ReviewWalkComplete,
            _ => return, // a stale code past the pushed row: inert
        };
        a.route(&window, action);
    });
    install_seam!(window: window.on_walk_file_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewInboxFile);
    });
    install_seam!(window: window.on_walk_keep_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewInboxKeep);
    });
    install_seam!(window: window.on_walk_file_prev_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFileTargetPrev);
    });
    install_seam!(window: window.on_walk_file_next_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFileTargetNext);
    });
    install_seam!(window: window.on_walk_candidate_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let index = usize::try_from(index).unwrap_or(usize::MAX);
    let state = a.state.load();
    let candidates = crate::presentation::walk_file_candidates(&state);
    let Some(stack) = candidates.get(index).copied() else {
        return; // a stale index past the candidates: inert
    };
    if let Some(Flow::WalkFilePick { armed }) = a.view_state.flow_mut() {
        *armed = Some(stack);
    }
    a.apply_outcomes(&window, &[FlowOutcome::RefreshNow]);
    a.finish_mutation(&window);
    });
    install_seam!(window: window.on_walk_file_commit_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFileCommit);
    });
    install_seam!(window: window.on_walk_tomorrow_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryTomorrow);
    });
    install_seam!(window: window.on_walk_pick_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryPickDate);
    });
    install_seam!(window: window.on_walk_leave_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryLeave);
    });
    install_seam!(window: window.on_walk_date_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        if let Some(child) = a.view_state.walk_date_mut() {
        child.date_text = text.to_string();
    }
    });
    install_seam!(window: window.on_walk_date_commit_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryDateCommit);
    });
    install_seam!(window: window.on_walk_answer_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (index, text), {        let index = usize::try_from(index).unwrap_or(usize::MAX);
    let Some(walk) = a.view_state.walk_mut() else {
        return;
    };
    if index >= walk.answers.len() {
        walk.answers.resize(index + 1, String::new());
    }
    walk.answers[index] = text.to_string();
    });
    install_seam!(window: window.on_walk_record_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewRecord);
    });
    install_seam!(window: window.on_walk_finish_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFinish);
    });
    // The Today card's activation: the routed opener (the same
    // dispatch as the action — the reference derivation and the
    // kiosk refusal live in the adapter's arms).
    install_seam!(window: window.on_review_card_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewOpen);
    });
}
/// The notification surface's seams (R3 chunk C, design D-10): each
/// button dispatches exactly one engine command and nothing else (the
/// thin-client rule).
fn install_notification_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // Dismiss: the synced dismissal fact (the universal surface's one
    // write command).
    install_seam!(window: window.on_notification_dismiss_clicked, adapter: adapter; [bare_ref]
        bind (a), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_dismiss(&row);
    });

    // Mark Done: the existing done command — the planner emits the
    // dismissal rows for the task's active occurrences (R3.5.5). An
    // alarm row has no task to done (the button's absence is the
    // presentation's job; a stray index degrades to a no-op).
    install_seam!(window: window.on_notification_done_clicked, adapter: adapter; [bare_ref]
        bind (a), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_done(&row);
    });

    // Snooze / Mute: the banner's routed verbs over the one-shot
    // target — the same registry actions, unchanged. The target's
    // anchor mirrors the ref (task rows task-anchored; alarm rows
    // adhoc-anchored — mute has no button there anyway).
    // The snooze choice flyout's pick (H4 D3, US-6/S2): the row's
    // chosen span routes the typed delta (the keyboard verb keeps its
    // fixed 1-hour hop — the grammar gains no chord).
    install_seam!(window: window.on_notification_snooze_span_picked, adapter: adapter;
        bind mut (a, window), args (index, span), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.send(taskboard_domain::StateCommand::SnoozeReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
            postpone: crate::router::snooze_span(usize::try_from(span).unwrap_or(usize::MAX)),
        });
        a.finish_mutation(&window);
    });

    install_seam!(window: window.on_notification_mute_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_mute(&window, &row);
    });

    // Cancel (DR-3/3, D-I7): the alarm row's off-switch — the routed
    // TimerCancel verb over the one-shot target. A stray or task row
    // degrades to a no-op (the done-seam's guard shape); the row
    // disappears by derivation at the next refresh.
    install_seam!(window: window.on_notification_cancel_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_cancel(&window, &row);
    });

    // Start Task (DR-5/4, D-TU7): the routed TaskTrackStart over the
    // row's one-shot target — table addressing, not focus (D-TU5).
    // Exactly one command leaves the UI: the implicit dismissal lives
    // planner-side (D-T5's same-batch emission); a stray index or an
    // alarm row (no task behind the anchor table) degrades to a no-op,
    // the done-seam's guard shape. The issue #210 reservation
    // resolves here.
    install_seam!(window: window.on_notification_track_start_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_track_start(&window, &row);
    });
    // D-6.1 (round 6): the row's double-click open — focus, then the
    // TaskEdit route (the today-row activation's shape; Activate's
    // `RefreshNow` performs the push). Alarm rows never open: the
    // markup's `enabled: row.can-open` gates, and the seam re-gates on
    // the row's task (belt and braces). The one-shot reminder target
    // is deliberately not armed here — nothing on this path consumes
    // it; every snooze/mute seam arms it itself before routing.
    install_seam!(window: window.on_notification_row_open_requested, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_open(&window, &row);
    });
}

/// The one-shot reminder target a notification row feeds the routed
/// snooze/mute verbs with.
fn notification_target(
    row: &crate::presentation::NotificationRow,
) -> crate::presentation::ReminderTarget {
    let anchor = match row.reminder {
        taskboard_domain::ReminderRef::Adhoc(alarm) => {
            taskboard_domain::ReminderAnchor::Alarm(alarm)
        }
        _ => taskboard_domain::ReminderAnchor::Task(
            row.task
                .expect("a non-adhoc notification row carries its task"),
        ),
    };
    crate::presentation::ReminderTarget {
        anchor,
        reminder: row.reminder,
        occurrence: row.occurrence,
    }
}

/// The settings seams (round-1 chunk 3, D7/D9; the r2 D29 gesture
/// trio joins): the form's mirror and send callbacks resolve straight
/// onto the [`Adapter`] settings VM — never ad-hoc logic. One wiring
/// sequence, kept flat for review.
#[allow(clippy::too_many_lines)]
fn install_settings_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    {
        install_seam!(window: window.on_settings_field_edited, adapter: adapter;
        bind mut (a, window), args (field, text), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .settings_field_edited(field, &text, &window);

        });
    }
    {
        install_seam!(window: window.on_settings_save_clicked, adapter: adapter;
        bind mut (a, window), args (server, username, password), {            a
                .settings_save(&window, &server, &username, &password);

        });
    }
    {
        install_seam!(window: window.on_settings_remove_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_remove_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_refresh_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_refresh_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_create_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_create_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_disconnect_clicked, adapter: adapter;
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_disconnect_item(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_files_picked, adapter: adapter;
        bind mut (a, window), args (item, index), {            let (Ok(item), Ok(index)) = (usize::try_from(item), usize::try_from(index)) else {
                return;
            };
            a.settings_files_picked(&window, item, index);

        });
    }
    {
        install_seam!(window: window.on_settings_remote_bind_picked, adapter: adapter;
        bind mut (a, window), args (item, option), {            let (Ok(item), Ok(option)) = (usize::try_from(item), usize::try_from(option)) else {
                return;
            };
            a.settings_remote_bind_picked(&window, item, option);

        });
    }
    {
        install_seam!(window: window.on_settings_publish_picked, adapter: adapter;
        bind mut (a, window), args (item, option), {            let (Ok(item), Ok(option)) = (usize::try_from(item), usize::try_from(option)) else {
                return;
            };
            a.settings_publish_picked(&window, item, option);

        });
    }
    {
        install_seam!(window: window.on_settings_remove_confirmed, adapter: adapter; // slint: remove-confirmed
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_remove_confirmed(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_remove_cancelled, adapter: adapter; // slint: remove-cancelled
        bind mut (a, window), args (), {            a.settings_disarm_item();
            a.push_settings(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_files_action_confirmed, adapter: adapter; // slint: files-action-confirmed
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_action_confirmed(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_files_action_cancelled, adapter: adapter; // slint: files-action-cancelled
        bind mut (a, window), args (), {            a.settings_disarm_item();
            a.push_settings(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_files_board_action_clicked, adapter: adapter; // slint: files-board-action-clicked
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_board_action(&window, item);

        });
        install_seam!(window: window.on_settings_files_board_restore_clicked, adapter: adapter; // slint: files-board-restore-clicked (US-6, D-7)
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_board_restore(&window, item);

        });
    }
    {
        install_seam!(window: window.on_settings_preset_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a.settings_preset_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_view_mounted, adapter: adapter;
        bind mut (a, window), args (), {            a.settings_view_mounted(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_scale_committed, adapter: adapter;
        bind mut (a, window), args (text), {            a
                .settings_scale_committed(&window, &text);

        });
    }
    {
        install_seam!(window: window.on_settings_scale_pressed, adapter: adapter;
        bind mut (a, window), args (), {            a.settings_scale_pressed(&window);

        });
    }
    {
        install_seam!(window: window.on_settings_scale_released, adapter: adapter;
        bind mut (a, window), args (value: f32), {            a
                .settings_scale_released(&window, f64::from(value));

        });
    }
    {
        install_seam!(window: window.on_settings_scale_focus_lost, adapter: adapter;
        bind mut (a, window), args (), {            a.settings_scale_focus_lost(&window);

        });
    }
    // The settings editor's general seams (the GUI settings editor
    // chunk): the same weak-handle wiring shape, one block per seam.
    {
        // The Files row's poll cadence commit seam (#1083, W-6): the
        // general fields' funnel under the plan row's own address.
        install_seam!(window: window.on_files_poll_secs_committed, adapter: adapter; // files-poll-secs-committed
        bind mut (a, window), args (field, text), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .files_poll_secs_committed(&window, field, &text);

        });
        install_seam!(window: window.on_settings_general_committed, adapter: adapter;
        bind mut (a, window), args (field, text), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .settings_general_committed(&window, field, &text);

        });
    }
    {
        install_seam!(window: window.on_settings_general_focus_lost, adapter: adapter;
        bind mut (a, window), args (field), {            let Ok(field) = usize::try_from(field) else {
                return;
            };
            a
                .settings_general_focus_lost(&window, field);

        });
    }
    {
        install_seam!(window: window.on_settings_reminders_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_reminders_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_sink_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a.settings_sink_picked(&window, index);

        });
    }
    // The focus section's pick seams (the focus settings editor,
    // #625): the reminders grammar, one block per toggle.
    {
        install_seam!(window: window.on_settings_focus_enabled_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_focus_enabled_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_focus_chain_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_focus_chain_picked(&window, index);

        });
    }
    {
        install_seam!(window: window.on_settings_focus_notify_picked, adapter: adapter;
        bind mut (a, window), args (index), {            let Ok(index) = usize::try_from(index) else {
                return;
            };
            a
                .settings_focus_notify_picked(&window, index);

        });
    }
}

/// The chunk-3 focus seam surface (D16/D17): sets the view context /
/// focused entities and resets the pending accumulation (a context
/// change invalidates it — D12 rule 1). The writes land in the
/// view-state machine — the shell's seam fields are derived from it
/// on every route (D6); the component tests use this seam now.
///
/// This seam is permanent pub surface, deliberately not
/// `#[cfg(test)]`-gated (audit F7-04): the integration tier
/// (`tests/journeys/usability_components.rs`) compiles this crate externally and
/// drives the settings surface through it, so a `#[cfg(test)]` gate —
/// invisible to an externally-compiled crate — would break the
/// integration build. Production code calls neither method
/// (re-verified 2026-11-03 @ `5d8c0751`).
///
/// A view-context write is a derive input outside the state Arc, so
/// `set_view_context` ends in a forced pass (the unchanged-refresh
/// guard's invariant, #463) — the surfaced rows are material before
/// the caller's next read.
impl crate::UiHandle {
    pub fn set_view_context(&self, view: Option<ContextId>) {
        let mapped = match view {
            None => {
                self.adapter.borrow_mut().resolver.reset();
                return;
            }
            // The export dialog's context never reaches the view
            // seam (the adapter's VM owns the mount; the modal
            // contexts' view mapping is the flows' contract).
            Some(ContextId::ExportDialog) => {
                return;
            }
            Some(ContextId::Today) => crate::view_state::ViewId::Today,
            Some(ContextId::Board) => crate::view_state::ViewId::Board,
            Some(ContextId::StackDetail) => crate::view_state::ViewId::StackDetail,
            Some(ContextId::TaskDetail | ContextId::Labels) => {
                crate::view_state::ViewId::TaskDetail
            }
            Some(ContextId::Review) => crate::view_state::ViewId::Review,
            Some(ContextId::Settings) => crate::view_state::ViewId::Settings,
            Some(ContextId::StatusPanel) => crate::view_state::ViewId::Status,
            Some(ContextId::PlanNextDay) => crate::view_state::ViewId::PlanNextDay,
            // The overlay's context maps to no view: like the other
            // non-routable contexts, the adapter is left untouched
            // (the modal masks the chain; no derive rides it).
            Some(
                ContextId::FocusMode
                | ContextId::Global
                | ContextId::Menu
                | ContextId::Palette
                | ContextId::DatePicker
                | ContextId::TrackEntries
                | ContextId::ReviewWalk
                | ContextId::RecurEditor
                | ContextId::AlarmWindow
                | ContextId::NotificationSurface
                | ContextId::ViNormal
                | ContextId::ViInsert
                | ContextId::Editor,
            ) => return,
        };
        {
            let mut adapter = self.adapter.borrow_mut();
            adapter.view_state.set_view(mapped);
            adapter.resolver.reset();
        }
        self.force_derive();
    }

    pub fn set_focused_entities(
        &self,
        focused_task: Option<TaskId>,
        _focused_stack: Option<StackId>,
    ) {
        // Deliberately no forced pass (the `set_view_context` half
        // forces): the seam plants a focus target for the keystroke
        // grammar to read from the view-state machine directly — the
        // grammar's consumers never re-read it from a derived pass,
        // and a derive's `reconcile` would drop a focus the state
        // snapshot cannot confirm.
        let mut adapter = self.adapter.borrow_mut();
        adapter.view_state.set_focused(focused_task);
    }
}

impl crate::UiHandle {
    /// One unguarded derive pass over the current window (the seam
    /// tail; the forced verb behind the handle's borrow discipline).
    fn force_derive(&self) {
        if let Some(window) = self.window().as_weak().upgrade() {
            self.adapter.borrow_mut().refresh_forced(&window);
        }
    }
}

/// Shows the files-sync chip in the given state (the mount seed and
/// every drain paint through here).
pub(crate) fn paint_files_sync_chip(window: &AppWindow, chip: &crate::presentation::FilesSyncChip) {
    window.set_files_sync_visible(true);
    window.set_files_sync_state(crate::models::files_sync_chip_state(chip.state));
    let costume = crate::presentation::files_sync_chip_presentation(chip.state);
    window.set_files_sync_tone(crate::models::files_sync_chip_tone(costume.tone));
    window.set_files_sync_flagged(costume.flagged);
    window.set_files_sync_text(SharedString::from(
        crate::presentation::files_sync_chip_caption(chip.state),
    ));
}

/// The stage-2 seam tests' typed read-outs (cross-surface friction
/// plan #832/#763): the adapter's private flags, exposed as facts —
/// never as mutable seams.
#[cfg(test)]
impl Adapter {
    pub(crate) fn test_notice_armed(&self) -> bool {
        self.notice.is_some()
    }

    pub(crate) fn test_habit_convert_armed(&self) -> Option<taskboard_domain::HabitId> {
        self.habit_convert_armed
    }

    pub(crate) fn test_series_confirm_armed(&self) -> bool {
        self.recur_confirm_armed
    }

    pub(crate) fn test_recur_edit_target(&self) -> Option<taskboard_domain::TaskId> {
        self.recur_edit_target
    }
}

#[cfg(test)]
mod effort_parity_tests {
    // AC-3 (task-detail-editor UX plan US-3/D-8): the detail row and
    // the walk's placement parse (`commit_plan_place`) share ONE
    // predicate — the domain `Effort::parse_text_form`. The corpus
    // pins the agreement: both fronts accept/reject identically
    // because both call sites route the same function; the journeys
    // us3_s1/us3_s2 carry the funnel's behavioral proof.
    use super::parse_effort_row;

    #[test]
    fn us3_s3_the_effort_row_parse_agrees_with_the_walk_parse() {
        for (text, accepted) in [
            ("45", true),
            ("1h30m", true),
            ("2h", true),
            ("90s", true),
            ("0", true),
            ("junk", false),
            ("1x", false),
            ("-5", false),
            ("h", false),
        ] {
            assert_eq!(
                parse_effort_row(text).is_some(),
                accepted,
                "{text}: the row's parse must be the walk's parse"
            );
            assert_eq!(
                parse_effort_row(text).is_some(),
                taskboard_domain::Effort::parse_text_form(text).is_some(),
                "{text}: one predicate, two fronts"
            );
        }
    }
}

#[cfg(test)]
mod auto_list_backoff_tests {
    use super::*;
    use std::time::Duration;

    /// US-3 AC-1 — inside the window the mount auto-list is
    /// suppressed; past it the request fires again (typed fold over
    /// the backoff map, the request-count seam's gate).
    #[test]
    fn us3_ac1_backoff_suppresses_the_auto_list() {
        let now = tokio::time::Instant::now();
        let mut backoff = BTreeMap::new();
        backoff.insert(
            "deck".to_owned(),
            AutoListBackoff {
                consecutive_failures: 1,
                last_failure: now,
            },
        );
        assert!(auto_list_suppressed(
            &backoff,
            "deck",
            now + Duration::from_secs(10)
        ));
        assert!(
            !auto_list_suppressed(&backoff, "deck", now + Duration::from_secs(31)),
            "past the first-failure window the listing fires again"
        );
        assert!(
            !auto_list_suppressed(&backoff, "other", now),
            "an unbacked connector never suppresses"
        );
    }

    /// The window doubling: n failures widen the window up to the
    /// 10-minute ceiling (deterministic, no wall clock).
    #[test]
    fn the_window_doubles_and_caps() {
        let entry = |n: u32| AutoListBackoff {
            consecutive_failures: n,
            last_failure: tokio::time::Instant::now(),
        };
        assert_eq!(entry(1).window(), Duration::from_secs(30));
        assert_eq!(entry(2).window(), Duration::from_secs(60));
        assert_eq!(entry(3).window(), Duration::from_secs(120));
        assert_eq!(entry(6).window(), Duration::from_secs(600));
        assert_eq!(entry(50).window(), Duration::from_secs(600), "caps");
    }

    /// US-3 AC-2/AC-3 — the re-arm and the clear: a failure adds one
    /// consecutive failure; the map's remove (the refresh override
    /// and the success settle) resets the posture.
    #[test]
    fn us3_ac3_failure_re_arms_and_success_clears() {
        let now = tokio::time::Instant::now();
        let mut entry = AutoListBackoff {
            consecutive_failures: 0,
            last_failure: now,
        };
        entry.record_failure(now);
        assert_eq!(entry.consecutive_failures, 1);
        entry.record_failure(now);
        assert_eq!(entry.consecutive_failures, 2, "re-arms on each failure");
        let mut map = BTreeMap::new();
        map.insert("deck".to_owned(), entry);
        map.remove("deck");
        assert!(!auto_list_suppressed(&map, "deck", now), "clear resets");
    }
}
