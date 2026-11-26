// SPDX-License-Identifier: AGPL-3.0-only

//! The slint adapter (chunk decision D18, design §5.6): the only code
//! that touches both worlds — slint key events in, keymap types out,
//! and shell state synced one-way onto window globals. The root
//! `FocusScope` is the single key path (§5.1); `KeyBinding` is unused
//! for any rebindable action (Q9); the keymap is applied one-way at
//! UI start (no live re-map machinery — §5.6).
//!
//! Boundary note (#1308 W-2, round 6): the alarm window's window-local
//! seams are NOT adapter installs — the lazily mounted second window
//! wires in `lib.rs::install_alarm_callbacks` (`on_drag-by` — slint
//! `drag-by` — plus the `drag-start` press arm and the tooltip
//! driver), so they are deliberately absent from this file's install
//! census and from `seam_inventory.rs`'s listed seams.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};

#[cfg(test)]
use slint::Model as _;
use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, ModelRc, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{AppState, BoardId, ReminderRuleId, StackId, StateCommand, TaskId};

use tokio::sync::oneshot;

use crate::actions::{ActionId, ContextId, REGISTRY};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::models::IdTables;
use crate::presentation::ReminderTarget;
use crate::requests::ConnectorInfo;
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{
    ActionRouter, Direction, DispatchOutcome, EffectClass, Mode, UiEffect, WalkIntent,
};
use crate::shell::KeymapShell;
use crate::view_state::{Flow, FlowOutcome, FocusEdge, ViewState, WalkStage, WalkStep};
use crate::{AppWindow, Breakpoint, KeyFeedback, KeyboardState, Responsive, UiStatus};

mod notices;
mod replies;
mod seams;
mod settings;
mod settings_vm;

use seams::{habit_open_entry, habit_undo_target, install_view_seams, settle_pointer_conflict};
pub use settings_vm::SettingsOutcome;
use settings_vm::{SettingsVm, StatusPanelVm, to_stroke};

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
/// triple (A5a D-2 widened the capture from the task alone; all-`None`
/// when the create flow held no landing) — the hint push's input.
#[derive(Debug, Clone)]
struct CreateFocusHandoff {
    before: crate::view_state::WalkFocus,
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
    /// The clip-focus law's absorb seam (#1419, operator ruling
    /// 2026-11-25, issue comment 7172599685): `true` from a push pass
    /// (or an `apply_outcomes` model write) until the first
    /// focus-lost or the first user-intent seam. The push's deferred
    /// `text_flow_focus_lost` echo is the synthetic unmount echo, not
    /// a user dismissal; the seam reads this flag and absorbs the
    /// echo. A `Cell` because the non-mut seam arms share the
    /// disarm.
    refresh_push_in_progress: std::cell::Cell<bool>,
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
    /// The recur editor's refused-commit flag (#1221): set when the
    /// commit funnel's re-check rejects the staged draft (the commit
    /// button and the outside-click accept share the seam — both
    /// accept paths surface through it), cleared on any draft field
    /// edit and on editor close. Mirrored onto
    /// `recur-draft-invalid` for the caption.
    recur_draft_invalid: bool,
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
    /// The undo-family restore's armed subject (W-3, #1290
    /// enumeration): the roster habit the deleted-habits section's
    /// strip would resurrect.
    habit_restore_armed: Option<taskboard_domain::HabitId>,
    /// The deleted-habits section's armed bulk restore (#1245): the
    /// confirm dispatches one `RestoreHabit` per roster row.
    habit_restore_all_armed: bool,
    /// The labels overlay's armed restore subject (W-2): the roster
    /// row the strip's Confirm would resurrect.
    label_restore_armed: Option<taskboard_domain::LabelId>,
    /// The labels overlay's armed bulk restore (#1245): the confirm
    /// dispatches one `RestoreLabel` per board-scoped roster row.
    label_restore_all_armed: bool,
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
    export_preview_reply: Option<oneshot::Receiver<crate::requests::ExportPreviewOutcome>>,
    /// The settled preview's stash between the poll and the
    /// `export_preview_shown` fold (the re-entrancy law).
    export_preview_settled: Option<crate::requests::ExportPreviewOutcome>,
    /// The in-flight clipboard copy's reply half (the preview plan
    /// W-2): the copy twin of the preview lane.
    export_copy_reply: Option<oneshot::Receiver<crate::requests::ExportClipboardOutcome>>,
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
        // The refused-commit mirror (#1221): rides the editor's open
        // state — the flag dies with the flow (buffers-never-outlive-
        // the-flow rule).
        window.set_recur_draft_invalid(recur_open && self.recur_draft_invalid);
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
        // The cursor mirror (#1227): the shell's picker-scoped
        // selection (0 while closed — the mount resets it).
        keyboard.set_undo_picker_cursor(i32::try_from(self.shell.menu.selected).unwrap_or(0));
        let undo = &self.state.load().undo;
        let rows = |labels: &[taskboard_domain::UndoLabel]| -> Vec<crate::UndoPickerRowModel> {
            labels
                .iter()
                .map(|row| crate::UndoPickerRowModel {
                    id: row.id.as_uuid().to_string().into(),
                    label: row.label.as_str().into(),
                    // D-3: empty string ↔ `None` across the seam —
                    // the row renders label-only, never a blank dash.
                    payload: row.payload.as_deref().unwrap_or("").into(),
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
                    // #1283: the Cow label projects verbatim — the
                    // owned (engine-derived) spelling survives the
                    // shell bridge.
                    label: entry.label.as_ref().to_owned().into(),
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
                    let before = self.view_state.walk_focus();
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
                // The batch twin of the router's today pair-write
                // (#1428, journey plan D-1): both fields in one mask —
                // the day's first covered instant (start) and its last
                // covered instant (due, #893). The domain mints, never
                // a local `end − ε` (#884).
                let first = self.day_ctx().window.0;
                let last = taskboard_domain::days::last_covered_instant(self.day_ctx().window.1);
                self.schedule_selection(&state, first, last);
            }
            ActionId::TaskScheduleTomorrow => {
                // The batch twin of the router's tomorrow pair-write
                // (D-2): the covered-instant law holds for the batch
                // path too — tomorrow's first instant and its last
                // covered instant, the raw exclusive end retired.
                let (first, end) = taskboard_domain::days::next_day_window(
                    self.day_ctx().now,
                    self.day_start.0,
                    self.day_start.1,
                );
                let last = taskboard_domain::days::last_covered_instant(end);
                self.schedule_selection(&state, first, last);
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
    /// `scheduled` and `duedate` set to the shared day-window pair
    /// (#1428, journey plan D-1), everything else untouched.
    fn schedule_selection(&mut self, state: &AppState, first: DateTime<Utc>, last: DateTime<Utc>) {
        for member in self.view_state.selection_anchor_order(state) {
            self.send(StateCommand::UpdateTask {
                id: member,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(first)),
                    duedate: Some(Some(last)),
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
                // The chord's subject mint (#1305, W-3/D-8): the
                // keyboard path mints the one-shot habit subject exactly
                // as the row seams do (the skip chord's shape).
                ActionId::HabitLog | ActionId::HabitSkip | ActionId::HabitTrackToggle
                    if self.habit_target.is_none() =>
                {
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
        // The schedule quick verbs' pair-write facts (#1428, journey
        // plan D-1/D-2): today's and tomorrow's first covered instants
        // (the day_start_time-aware window starts) and tomorrow's last
        // covered instant (the covered-instant law — the raw exclusive
        // end is retired). One domain mint per fact, no local
        // arithmetic.
        ctx.today_first_instant = Some(self.day_ctx().window.0);
        let tomorrow = taskboard_domain::days::next_day_window(
            self.day_ctx().now,
            self.day_start.0,
            self.day_start.1,
        );
        ctx.tomorrow_first_instant = Some(tomorrow.0);
        ctx.tomorrow_last_instant = Some(taskboard_domain::days::last_covered_instant(tomorrow.1));
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
        // The habit toggle's state half (#1305, the twin read): answers
        // about the exact resolved target — the one-shot `habit_target`
        // minted above, else the focused habit.
        if id == ActionId::HabitTrackToggle {
            let state = self.state.load();
            ctx.habit_tracking = ctx
                .habit_target
                .is_some_and(|habit| habit_open_entry(&state, habit).is_some());
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
            // The empty `before` triple arms unconditionally (the
            // row-restore's deliberate always-seek; A5a D-2 widened
            // the capture type only).
            self.push_focus_scroll_hint(window, crate::view_state::WalkFocus::default());
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
        // The clip-focus law's echo window for the OUTCOME pushes
        // (#1419 option A): applying outcomes writes the slint models
        // exactly like a refresh push (a flow opening here rebuilds
        // the field subtrees), so its deferred focus-lost echo is the
        // synthetic unmount echo too. Arm the flag unless a push pass
        // already holds it; like the pass arm, it disarms on the
        // first focus-lost or the first user-intent seam — never on
        // this function's exit.
        self.refresh_push_in_progress.set(true);
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
    /// The pinned day anchor the history dialog's month mounts at
    /// (the subject opener's computation, shared by the palette's
    /// search-row arm — one mechanism, W-10).
    pub(crate) fn today_date(&self) -> chrono::NaiveDate {
        taskboard_domain::DayCalendar {
            day_start_time: self.day_start.0,
            offset: self.day_start.1,
        }
        .today(self.day_ctx().now)
    }

    pub(crate) fn day_ctx(&self) -> crate::view_state::DayCtx {
        let now = self.frozen_now.unwrap_or_else(chrono::Utc::now);
        let window = taskboard_domain::days::day_window(now, self.day_start.0, self.day_start.1);
        crate::view_state::DayCtx {
            now,
            window,
            calendar: taskboard_domain::DayCalendar {
                day_start_time: self.day_start.0,
                offset: self.day_start.1,
            },
        }
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
        // The clip-focus law's absorb seam arm (#1419, operator
        // ruling 2026-11-25, issue comment 7172599685): the pass
        // below re-derives and pushes the slint models, and a model
        // push can unmount/remount the focused text field — the
        // unmount fires `text_flow_focus_lost` as a synthetic unmount
        // echo, not a user dismissal. The flag arms the seam's absorb
        // for the echo's delivery window (it extends past this body —
        // see the disarm note at the tail).
        self.refresh_push_in_progress.set(true);
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
        // The push-side kiosk belt (W-15, #1049): the reconcile's
        // dead-subject arm's twin — a habit-variant mount that slipped
        // past the dispatch gates steps back on the same pass, before
        // any model reads the posture.
        {
            let outcomes = self.view_state.kiosk_habit_belt(self.kiosk);
            if !outcomes.is_empty() {
                self.apply_outcomes(window, &outcomes);
            }
        }
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
        // The armed bulk restore absorbs when the roster empties (the
        // same stale law — nothing left to resurrect in bulk).
        if self.habit_restore_all_armed && state.roster.tombstoned_habits.is_empty() {
            self.habit_restore_all_armed = false;
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
        // The armed bulk label restore absorbs when the tombstone set
        // empties (the stale law inverted — nothing left in bulk).
        if self.label_restore_all_armed && state.roster.tombstoned_labels.is_empty() {
            self.label_restore_all_armed = false;
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
                // Aggregated over all boards since round-6 A1 D-3
                // (2026-11-24) — the same order the roster renders.
                crate::presentation::break_rule_rows(&state, None)
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
        // The deleted-habits section's pushed halves (#1290/#1245, D-4):
        // the roster's rows and armed mirrors; kiosk pushes empty — the
        // affordances render absent, the CLI carries the restore path.
        let (habit_deleted_rows, habit_restore_armed_index): (Vec<slint::SharedString>, i32) =
            if self.kiosk {
                (Vec::new(), -1)
            } else {
                let armed = self
                    .habit_restore_armed
                    .and_then(|id| {
                        state
                            .roster
                            .tombstoned_habits
                            .iter()
                            .position(|row| row.id == id)
                    })
                    .and_then(|p| i32::try_from(p).ok())
                    .unwrap_or(-1);
                (
                    state
                        .roster
                        .tombstoned_habits
                        .iter()
                        .map(|row| slint::SharedString::from(row.name.as_str()))
                        .collect(),
                    armed,
                )
            };
        window.set_habits_has_deleted(!habit_deleted_rows.is_empty());
        window.set_habits_deleted_rows(ModelRc::new(slint::VecModel::from(habit_deleted_rows)));
        window.set_habits_restore_armed_index(habit_restore_armed_index);
        window.set_habits_restore_all_armed(self.habit_restore_all_armed && !self.kiosk);
        // The labels overlay's bulk armed mirror (#1245).
        window.set_label_overlay_restore_all_armed(self.label_restore_all_armed && !self.kiosk);
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
        // The walk's embedded planner child is a live planner mount
        // too (US-22, #1234): the cursor survives there — the surface
        // check reads the chain, not just the root view.
        let planner_live = self.view_state.view() == crate::view_state::ViewId::PlanNextDay
            || self
                .view_state
                .chain_has_flow(|flow| matches!(flow, Flow::ReviewPlanner));
        if !planner_live
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
        // NOTE: the echo flag is intentionally NOT disarmed here. The
        // deferred focus-lost echo of this push lands after the
        // dispatch's synchronous processing ends, so a tail disarm
        // always precedes it. The flag disarms on the FIRST focus-lost
        // (the seam's absorb-and-disarm) or on any user-intent seam
        // (the macro arms) — a genuine interaction un-arms before its
        // own focus move can be mis-absorbed.
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
        // The echoed selection halves (#1306): the day + insistence
        // rows render these mirrors — reply-sourced echo (r2 D27),
        // one push path for both adapter-owned rows. The wipe rides
        // here too: `clear_timer_setter` reseeds both, so a fresh
        // open pushes 0/0.
        window.set_timer_setter_day_index(i32::try_from(self.setter_day_offset).unwrap_or(0));
        window.set_timer_setter_insist_index(i32::from(self.setter_insist));
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
                    self.view_state.clear_plan_cursor();
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
                    self.view_state.clear_plan_cursor();
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
                // The planner child's Cancelled rung clears the K4
                // cursor pane-scoped (US-22/S4 — the landed clear law).
                if self.view_state.walk_stage() == Some(WalkStage::Planner) {
                    self.view_state.clear_plan_cursor();
                }
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
                // The rating drafts mint at the trust boundary
                // (#1431, the T-15 grammar): blank is the legal
                // unrated; an invalid draft refuses the record —
                // nothing dispatches, the walk stays on the Prompts
                // stage with the flavours rendered.
                let walk_drafts = self
                    .view_state
                    .walk()
                    .map_or((String::new(), String::new()), |walk| {
                        (walk.mood_text.clone(), walk.energy_text.clone())
                    });
                let rating_draft = |draft: &str| -> Option<Option<u8>> {
                    if draft.is_empty() {
                        Some(None)
                    } else {
                        draft
                            .parse::<u8>()
                            .ok()
                            .and_then(|n| taskboard_domain::reflection::Rating::try_new(n).ok())
                            .map(|rating| Some(rating.get()))
                    }
                };
                let mood = rating_draft(&walk_drafts.0);
                let energy = rating_draft(&walk_drafts.1);
                let (Some(mood), Some(energy)) = (mood, energy) else {
                    let flavour = |parsed: Option<Option<u8>>, draft: &str| {
                        if draft.is_empty() {
                            crate::FieldValidity::Neutral
                        } else if parsed.is_some() {
                            crate::FieldValidity::Valid
                        } else {
                            crate::FieldValidity::Invalid
                        }
                    };
                    window.set_walk_mood_validity(flavour(mood, &walk_drafts.0));
                    window.set_walk_energy_validity(flavour(energy, &walk_drafts.1));
                    return Vec::new();
                };
                let mut outcomes = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::RecordReflection {
                        cadence,
                        day: window_bounds.day(),
                        window: window_bounds.bounds(),
                        text,
                        mood,
                        energy,
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
        let focus_before = self.view_state.walk_focus();
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
                // Round-6 A1 D-4 (2026-11-24, #1301): the today view is
                // board-free — the gate is "any live board exists"; the
                // commit mints into the first live board.
                if !taskboard_domain::order::live_boards(&state).is_empty() {
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
                    || self.label_restore_armed.take().is_some()
                    || self.label_restore_all_armed =>
            {
                // The bulk flag joins the disarm rung (#1245): the
                // armed strip is a staged state, the Escape ladder's
                // "an armed row disarms" rung owns it — the overlay
                // closes only on the last rung.
                self.label_restore_all_armed = false;
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
                // Escape's clean close (#1221): the refused-commit
                // flag dies with the flow.
                self.set_recur_draft_invalid(window, false);
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
                // The subject resolution (#1303): the menu rows'
                // close-leg mint (the one-shot `habit_target` — the
                // row's own subject captured BEFORE the disarm law
                // cleared the opener's focus facts) resolves first;
                // the chain (the walk cursor, the row seams' mints)
                // follows. The mint is liveness-filtered here: a dead
                // target falls through to the chain, never answers.
                let subject = self
                    .habit_target
                    .take()
                    .filter(|habit| {
                        self.state
                            .load()
                            .habits
                            .habits
                            .get(habit)
                            .is_some_and(taskboard_domain::habit::Habit::is_live)
                    })
                    .or_else(|| self.habit_edit_subject());
                // The draft-before-mount law (#1303 §1.3): the draft
                // seeds from the stored row FIRST (the editor opens
                // showing the habit as it is; it dies with the editor
                // — cancel/escape/close, no draft persistence, D1),
                // and the editor mounts ONLY over a `Some` draft — an
                // unseedable subject lands in the same typed refusal
                // as a subjectless one. No `open_habit_editor` call
                // exists without a `Some` draft afterwards.
                let draft = subject.as_ref().and_then(|habit| {
                    self.state
                        .load()
                        .habits
                        .habits
                        .get(habit)
                        .map(crate::router::HabitEditDraft::from_habit)
                });
                if let Some(draft) = draft {
                    let habit = draft.habit;
                    self.habit_edit_draft = Some(draft);
                    let outcomes = self.view_state.open_habit_editor(habit);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if
                // Only a SUBJECTLESS dispatch falls through to the
                // walk cursor's task landing. A resolved-but-unseedable
                // subject is unrepresentable — the liveness filter and
                // the seed read the same state load — and must not
                // reach the task landing either, so the guard excludes
                // it (a silent no-op, never the task editor).
                subject.is_none()
                    && !self.kiosk
                    && self.view_state.focused_task().is_some()
                {
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
                    // #1478: the refusal disarms the survivor too —
                    // the menu route's dispatch-scoped disarm leaves
                    // the quota exception's RESTORED pre-close focus
                    // alive past a refused dispatch; a dead subject
                    // minting nothing means the restore stands, so the
                    // clear happens here (the arm-refusal shape: the
                    // success path's mint above stays the only
                    // survivor writer).
                    self.focused_habit = None;
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
            // opener's twin, D-5): the open is a pushed view level,
            // and the snapshot query dispatches HERE — the route arm
            // is the query's sole dispatcher (one query per mount,
            // OP3 D-3; #1281 retired the doubled init seam).
            (Flow::None, UiEffect::OpenStatusPanel) => {
                if self.view_state.view() == crate::view_state::ViewId::Status {
                    self.view_state.close_status_panel()
                } else {
                    let outcomes = self.view_state.open_status_panel();
                    self.dispatch_status_query(window);
                    outcomes
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
            // The embedded planner's cursor arms (US-22, #1234): the
            // walk's planner child shares the standalone planner's K4
            // cursor — the verticals route to the SAME
            // `walk_plan_cursor` seam (R-9's one-mechanism law; the
            // class matrix's Nav settle would kill the child, which is
            // exactly the gap this kept arm closes), and an armed
            // cursor's Activate dispatches the cursor'd row's OWN
            // `PlanBeginPlace` seam (the `Flow::None` twin above; the
            // child keeps the form's mount via `open_plan_place_child`).
            // A horizontal is untouched — the walk's own grammar keeps
            // the class matrix's cell.
            (
                Flow::ReviewPlanner,
                effect @ UiEffect::FocusDirectional(Direction::Up | Direction::Down),
            ) => {
                let UiEffect::FocusDirectional(direction) = effect else {
                    unreachable!("arm matched")
                };
                let step = i32::from(direction == Direction::Down) * 2 - 1;
                self.walk_plan_cursor(step)
            }
            (Flow::ReviewPlanner, UiEffect::Activate)
                if self.view_state.plan_cursor().is_some() =>
            {
                self.activate_plan_cursor(window);
                Vec::new()
            }
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
                    Some(target) => self
                        .view_state
                        .palette_jump(&state, target, self.today_date()),
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
        let before = self.view_state.walk_focus();
        self.view_state.set_focused(Some(created));
        Some(CreateFocusHandoff { before })
    }

    fn push_focus_scroll_hint(&mut self, window: &AppWindow, before: crate::view_state::WalkFocus) {
        // The generalized no-change guard (A5a D-2, #1302): a landing
        // whose whole subject tuple (task, habit, roster) is unchanged
        // re-arms nothing — the ends-hold law, widened from the
        // task-only capture. `before == landing` covers the old
        // `focused.is_none() || focused == before` shape.
        let landing = self.view_state.walk_focus();
        if landing == before {
            return;
        }
        // A slint int carries the index; the tables' lengths are far
        // below i32 reach.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let hint = match self.view_state.view() {
            crate::view_state::ViewId::Board => landing
                .task
                .and_then(|task| self.tables.board_position(task))
                .map(|(column, row)| (to_hint_index(column), to_hint_index(row))),
            // The today walk's subject resolution (A5a D-2): task
            // precedence (the walk_subject law), then the habit row,
            // then the break-rule roster cursor — each against the
            // same `(section, row)` convention, the habit/roster rows
            // against the synthetic page slots
            // (`IdTables::TODAY_HABIT_SLOT`/`TODAY_ROSTER_SLOT`) that
            // no inner section ListView consumes.
            crate::view_state::ViewId::Today => {
                if let Some(task) = landing.task {
                    self.tables
                        .today_position(task)
                        .map(|(section, row)| (to_hint_index(section), to_hint_index(row)))
                } else if let Some(habit) = landing.habit {
                    self.tables
                        .today_habit_position(habit)
                        .map(|(section, row)| (to_hint_index(section), to_hint_index(row)))
                } else {
                    landing
                        .roster
                        .and_then(|rule| self.tables.today_roster_position(rule))
                        .map(|(section, row)| (to_hint_index(section), to_hint_index(row)))
                }
            }
            crate::view_state::ViewId::StackDetail => landing
                .task
                .and_then(|task| self.tables.stack_position(task))
                .map(|row| (-1, to_hint_index(row))),
            // The planner's pane walk (#1320, D-1320a/b): the cursor
            // task resolves `(pane, row)` — the #622 fixed-page
            // decision is NOT reopened (no paging hint; the narrowed
            // exception is the focus law only), the fixed surface's
            // own Flickable consumes the landing.
            crate::view_state::ViewId::PlanNextDay => landing
                .plan
                .and_then(|task| self.tables.plan_position(task))
                .map(|(pane, row)| (to_hint_index(pane), to_hint_index(row))),
            // The review page carries no focusable rows (D-8): the
            // focus-into-view seam never targets it, like the settings
            // facts path and the detail's own container.
            crate::view_state::ViewId::TaskDetail
            | crate::view_state::ViewId::Settings
            | crate::view_state::ViewId::Review
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
                // The picker's selectable-row count (#1227, I-1): the
                // two reflex rows plus the projection's rows — the
                // cursor machinery's wrapping bound (the shell stores
                // it at mount).
                let picker_len = {
                    let undo = &self.state.load_full().undo;
                    2 + undo.recent.len() + undo.branch_children.len()
                };
                self.shell.apply(&effect, picker_len);
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
                } else if let Some(index) = self.shell.picker_selected() {
                    // The picker's invoke (#1227): the cursor's row
                    // routes through the exact seams the click path
                    // uses (US-4/AC-2: invoke ≡ click, byte-identical
                    // dispatch) — the close-then-dispatch shape via
                    // `undo_picker_commit`.
                    self.shell.apply(&UiEffect::InvokeSelected, 0);
                    self.invoke_picker_row(window, index);
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
                    } else {
                        // The ladder's close leg (#1115): the root's
                        // MenuBack is the full close — it funnels
                        // through `apply_close_menu` (#1328, the
                        // helper's reset-at-every-close law), which
                        // carries the same one-shot hygiene this leg
                        // applied inline (#1115's staging disarm, the
                        // #1303 focus-fact clears) plus the page-model
                        // resets (`menu_entries`/`root_entries`/
                        // `menu_chain`/`menu_kind`/`menu_subject`/
                        // quick bar) the inline shape missed. The
                        // disarm order is load-bearing and preserved:
                        // the shell close first, then the helper's
                        // model reset → staging disarm → sync.
                        self.apply_close_menu(window);
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
                } else if self.shell.undo_picker {
                    // The picker cursor (#1227, I-1): the shell's
                    // menu-cursor machinery moves over the picker's
                    // row count (stored at mount); the sync mirrors
                    // the cursor onto the window global.
                    self.shell.apply(&effect, self.shell.menu.len);
                    self.sync_undo_picker_models(window);
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
        let before = self.view_state.walk_focus();
        let target = self.tables.palette.get(selected).copied();
        let state = self.state.load();
        let outcomes = match target {
            Some(target) => self
                .view_state
                .palette_jump(&state, target, self.today_date()),
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
        // The one-shot hygiene (#1303's disarm law): the opener's
        // focus facts die with the menu — a surviving
        // `focused_habit`/`menu_row_subject` would answer a later,
        // unrelated dispatch with a stale habit (the wrong-subject
        // editor). The menu-row route re-arms them for the ONE
        // dispatch (`route_menu_action`); a plain close keeps them
        // dead. The engine-sync re-derivation reads
        // `menu_row_subject` only while the menu is OPEN (the
        // main-menu page rebuild) — a close fires on a closed menu,
        // so the clear is safe there.
        self.focused_habit = None;
        self.menu_row_subject = None;
        self.quick_bar_entries.clear();
        // One rule (#1115): a menu close disarms every habit confirm
        // staging — the armed one-shot must not survive a hidden menu
        // (a later confirm on a closed menu is exactly the #1059
        // armed-confirm grammar's failure mode). The step-down leg of
        // `MenuBack` deliberately does NOT join: the menu is still
        // open there.
        self.disarm_habit_confirm_staging();
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
        self.route_menu_action(window, action);
    }

    /// The refused-commit flag's push (#1221): the field and the
    /// window mirror move together — the edit seams' `refresh_forced`
    /// pass does not run the shell `sync`, so the mutation sites push
    /// the half themselves.
    fn set_recur_draft_invalid(&mut self, window: &AppWindow, invalid: bool) {
        self.recur_draft_invalid = invalid;
        window.set_recur_draft_invalid(invalid);
    }

    /// The habit confirm staging's disarm (#1115): a menu close
    /// clears all three armed one-shots (convert, remove, restore) —
    /// the one-subject law's close leg.
    fn disarm_habit_confirm_staging(&mut self) {
        self.habit_convert_armed = None;
        self.habit_remove_armed = None;
        self.habit_restore_armed = None;
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
                // The tracking toggle greys with the rest (journey plan
                // W-2, #1305 — the greyed-affordance law: reading a menu
                // is passive; the control's own gate is habits.slint's
                // editing-enabled binding).
                ActionId::HabitTrackToggle,
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
        // #1311: the chain is the opener's — `open_habit_context_menu`
        // installs the Today-prefixed chain, so this projection, the
        // Convert submenu, and the arm-flip re-projection all resolve
        // the Today-bound verbs' hints through `self.menu_chain` (one
        // derivation, no per-consumer patching).
        let chain = self.menu_chain.clone();
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
        // The undo-family restore moved out of the menu (#1290's
        // enumeration: the deleted-habits section owns the roster
        // rows — the newest-only menu arm retired with it).
        let page = crate::menu::habit_context_page(&self.keymap, &chain, subject, &states);
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
        // #1429: the right-click is the row activation's twin — the
        // walk cursor seats too (`ViewState::activate_habit_row`, the
        // exclusivity law's landing write: a prior task cursor
        // clears), the same discipline the left-click seam and the
        // today-row context pattern apply. The menu opens only past
        // this point: focus landed exactly when the id resolved live
        // above (the stale/dead early returns), so the gate is the
        // resolution order itself.
        let outcomes = self.view_state.activate_habit_row(habit);
        self.apply_outcomes(window, &outcomes);
        self.menu_row_subject = Some(crate::presentation::RowSubject::Habit(habit));
        // #1311: the habit menu's context chain — Today first (the
        // menu opens exclusively over the Today view's habit rows, and
        // the habit verbs are bound exactly there), Board and Global
        // following for any verb bound above. The projection (this
        // open, the Convert submenu, the re-projections) reads it
        // through `self.menu_chain`.
        self.menu_chain = [ContextId::Today, ContextId::Board, ContextId::Global].to_vec();
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
        self.route_menu_action(window, action);
    }

    /// A menu leaf row's close-then-route (the `invoke_entry` /
    /// `invoke_root_entry` tails' shared body, #1303): the close leg
    /// disarms the opener's focus one-shots (the disarm law), and
    /// the row's ONE route re-arms them from the open-time capture —
    /// nothing survives the dispatch. The Edit habit row mints its
    /// subject into the one-shot `habit_target` INSTEAD: the habit
    /// context menu's row resolves the menu's own armed subject, the
    /// main menu's row resolves the current view context ONLY (the
    /// walk cursor) — a one-shot minted by a different surface's
    /// menu never answers with the wrong-subject editor (#1303's
    /// filed defect).
    fn route_menu_action(&mut self, window: &AppWindow, action: ActionId) {
        let focus = self.focused_habit;
        let row_subject = self.menu_row_subject;
        if action == ActionId::HabitEdit && !self.kiosk && self.habit_target.is_none() {
            let subject = match row_subject {
                Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
                _ => self.view_state.walk_subject().and_then(|walk| match walk {
                    crate::presentation::RowSubject::Habit(habit) => Some(habit),
                    crate::presentation::RowSubject::Task(_) => None,
                }),
            };
            if let Some(habit) = subject.filter(|habit| {
                self.state
                    .load()
                    .habits
                    .habits
                    .get(habit)
                    .is_some_and(taskboard_domain::habit::Habit::is_live)
            }) {
                self.habit_target = Some(habit);
            }
        }
        self.apply_close_menu(window);
        if action != ActionId::HabitEdit {
            self.focused_habit = focus;
            self.menu_row_subject = row_subject;
        }
        self.route(window, action);
        // The dispatch-scoped disarm (#1303): the re-armed facts die
        // with the dispatch — every habit-subject consumer reads them
        // at route/arm time. The quota opener is the ONE exception:
        // its arm mints `focused_habit` for the popover's later
        // commit leg (the commit resolves fresh through the chain),
        // so that mint must survive the route.
        self.menu_row_subject = None;
        if action != ActionId::HabitQuotaEdit {
            self.focused_habit = None;
        }
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
            // The echo-window disarm (the clip-focus law, #1419): a
            // user-intent seam's entry proves real interaction — the
            // push's deferred echo either already arrived (the
            // focus-lost seam absorbed it) or never will; the next
            // focus-lost is genuine and must settle.
            $a.refresh_push_in_progress.set(false);
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
            // The echo-window disarm — see the default arm.
            $a.refresh_push_in_progress.set(false);
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
            // The echo-window disarm — see the default arm.
            $a.refresh_push_in_progress.set(false);
            $body
        });
    }};
    (window: $window:ident . $on:ident, adapter: $adapter:expr; [bare_mut]
     bind mut ($a:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        $window.$on(move |$($arg)*| {
            let mut $a = seam_adapter.borrow_mut();
            // The echo-window disarm — see the default arm.
            $a.refresh_push_in_progress.set(false);
            $body
        });
    }};
    (window: $window:ident . $on:ident, adapter: $adapter:expr; [bare_ref]
     bind ($a:ident), args ($($arg:tt)*), $body:block) => {{
        let seam_adapter = ::std::rc::Rc::clone($adapter);
        $window.$on(move |$($arg)*| {
            let $a = seam_adapter.borrow();
            // The echo-window disarm — see the default arm.
            $a.refresh_push_in_progress.set(false);
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

pub(crate) use install_seam;
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
        recur_draft_invalid: false,
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
        refresh_push_in_progress: std::cell::Cell::new(false),
        habit_convert_armed: None,
        habit_remove_armed: None,
        habit_restore_armed: None,
        habit_restore_all_armed: false,
        label_restore_armed: None,
        label_restore_all_armed: false,
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

    /// #1429/#1478's typed read-out: the habit focus fact (the right-
    /// click seam's landing write and the quota refusal's clear leg).
    pub(crate) fn test_focused_habit(&self) -> Option<taskboard_domain::HabitId> {
        self.focused_habit
    }

    /// #1328's typed read-out: the close law's page-model half — every
    /// menu page fact reset (`apply_close_menu`'s body), not just the
    /// shell's open bit.
    pub(crate) fn test_menu_models_reset(&self) -> bool {
        self.menu_entries.is_empty()
            && self.root_entries.is_empty()
            && self.menu_chain.is_empty()
            && self.menu_kind == MenuKind::Main
            && self.menu_subject == MenuSubject::None
            && self.quick_bar_entries.is_empty()
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
mod menu_label_projection_tests {
    use super::*;
    use crate::menu::MenuEntry;

    /// The #1283 pin: an owned (engine-derived) label projects through
    /// the shell model verbatim — structural equality on the
    /// projected value, not text-in-DOM. The borrowed leg keeps
    /// projecting too (the source-compatibility half).
    #[test]
    fn an_owned_label_projects_through_the_shell_model() {
        let rows = vec![
            MenuEntry {
                action: Some(ActionId::HabitLog),
                label: std::borrow::Cow::Owned("Restore habit \u{201c}stretch\u{201d}".to_owned()),
                description: "engine-derived",
                enabled: true,
                hint: Vec::new(),
                submenu: None,
            },
            MenuEntry {
                action: Some(ActionId::AppExit),
                label: std::borrow::Cow::Borrowed("Quit taskboard"),
                description: "registry",
                enabled: true,
                hint: Vec::new(),
                submenu: None,
            },
        ];
        let model = Adapter::menu_model(&rows);
        assert_eq!(
            model.row_data(0).expect("row 0").label.as_str(),
            "Restore habit \u{201c}stretch\u{201d}",
            "the owned label survives the bridge verbatim"
        );
        assert_eq!(
            model.row_data(1).expect("row 1").label.as_str(),
            "Quit taskboard",
            "the borrowed label projects unchanged"
        );
    }
}

/// The round-8 B4 small-inventory seam pins (#1429, #1478, #1328,
/// #1449): typed seam-level outcomes at the lowest layer — the
/// adapter's effect/seam tier — never text oracles. A child module of
/// `adapter` so the #1478 direct construction can reach
/// `route_menu_action` (the arm-time dead subject is not reachable
/// through today's live-only menu openers).
#[cfg(test)]
mod ui_small_inventory_tests {
    use super::*;
    use crate::KeyboardState;
    use crate::actions::ActionId;
    use crate::router::UiEffect;
    use crate::test_platform::on_slint_thread;
    use crate::{UiHandle, UiSettings, mount};
    use std::sync::Arc;
    use taskboard_domain::{
        AppState, Board, BoardClocks, Color, HabitClocks, HabitId, HabitTarget, NonZeroCount,
        Stack, StackClocks, StackId, Task, TaskClocks, TaskId,
    };
    use tokio::sync::{broadcast, mpsc};

    // -- fixture -------------------------------------------------------------

    fn fixture_board(id: taskboard_domain::BoardId) -> Board {
        let now = BoardClocks::epoch();
        Board {
            id,
            remote: None,
            title: "fixture board".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: now,
                color: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        }
    }

    fn fixture_habit(board: taskboard_domain::BoardId) -> taskboard_domain::habit::Habit {
        taskboard_domain::habit::Habit {
            id: HabitId::from_uuid(uuid::Uuid::now_v7()),
            board,
            title: "water".to_owned(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(2).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(chrono::Utc::now()),
            continued_from: None,
            deleted: false,
            by: None,
            paused_until: None,
        }
    }

    /// One board, one stack, one titled task, one live habit — the
    /// smallest state every pin in this file can read.
    fn fixture() -> (AppState, TaskId, HabitId) {
        let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
        let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
        let task_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let now = BoardClocks::epoch();
        let mut state = AppState::default();
        state.boards.insert(board_id, fixture_board(board_id));
        let stack = Stack {
            id: stack_id,
            remote: None,
            board: board_id,
            title: "stack".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: now,
                order: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        };
        state.stacks.insert(stack_id, stack);
        let task = Task {
            id: task_id,
            remote: None,
            label_memberships: BTreeMap::new(),
            title: "fixture task".to_owned(),
            description: String::new(),
            duedate: Some(chrono::Utc::now() - chrono::TimeDelta::try_hours(2).unwrap()),
            scheduled: None,
            done: None,
            stack: stack_id,
            order: 0,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: now,
                description: now,
                duedate: now,
                scheduled: now,
                done: now,
                position: now,
                effort: now,
                importance: now,
                urgency: now,
                recur: now,
                kind: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        };
        state.tasks.insert(task_id, task);
        let habit = fixture_habit(board_id);
        let habit_id = habit.id;
        state.habits.habits.insert(habit_id, habit);
        (state, task_id, habit_id)
    }

    // -- rig -----------------------------------------------------------------

    struct Rig {
        handle: UiHandle,
        _commands_rx: mpsc::Receiver<taskboard_domain::StateCommand>,
    }

    fn rig(state: AppState) -> Rig {
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_system_tx, system_rx) = broadcast::channel(16);
        let (_signals_tx, signals_rx) = broadcast::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            Arc::new(arc_swap::ArcSwap::from_pointee(state)),
            signals_rx,
            system_rx,
            commands_tx,
            requests_tx,
            UiSettings::default(),
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        Rig {
            handle,
            _commands_rx: commands_rx,
        }
    }

    impl Rig {
        fn adapter(&self) -> std::rc::Rc<std::cell::RefCell<crate::adapter::Adapter>> {
            self.handle.test_adapter()
        }

        /// Routes one action through the adapter's dispatch seam (the
        /// src tests' `route_action` pattern).
        fn route(&self, action: ActionId) {
            self.adapter()
                .borrow_mut()
                .route(self.handle.window(), action);
        }
    }

    // -- #1429: the right-click seam seats the focus-first discipline -------

    /// The habit row's right-click is the row activation's twin: the walk
    /// cursor seats (`focused_habit` mints, the exclusivity law clears a
    /// prior task landing) and the menu opens only past the landed focus.
    #[test]
    fn habit_context_seats_the_focus_and_opens_the_menu() {
        on_slint_thread(|| {
            let (state, _task, habit) = fixture();
            let rig = rig(state);
            // The exclusivity precondition: a task landing holds the walk
            // (the walk's first row on Today is the fixture task — tasks
            // lead, habits follow).
            rig.route(ActionId::NavNextItem);
            assert!(
                rig.adapter().borrow().view_state.focused_task().is_some(),
                "precondition: the task landing holds the cursor"
            );

            rig.handle
                .window()
                .invoke_habits_context_requested(0, 120.0, 120.0);

            let adapter = rig.adapter();
            let a = adapter.borrow();
            assert_eq!(
                a.test_focused_habit(),
                Some(habit),
                "the right-click mints the habit focus fact"
            );
            assert!(
                a.view_state.focused_task().is_none(),
                "the exclusivity law clears the task cursor (#1429)"
            );
            assert!(
                rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the menu opens past the landed focus"
            );
        });
    }

    /// A stale right-click index is absorbed: no menu, no panic, no focus
    /// mint (the notification-row guard shape the left-click seam uses).
    #[test]
    fn habit_context_at_a_stale_index_absorbs_silently() {
        on_slint_thread(|| {
            let (state, _task, _habit) = fixture();
            let rig = rig(state);
            rig.handle
                .window()
                .invoke_habits_context_requested(9, 120.0, 120.0);
            let adapter = rig.adapter();
            let a = adapter.borrow();
            assert_eq!(
                a.test_focused_habit(),
                None,
                "a stale index never mints the focus fact"
            );
            assert!(
                !rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "no focus landed, no menu opens"
            );
        });
    }

    // -- #1478: the refused quota route disarms the restored focus ----------

    /// The quota menu route over an arm-time dead subject refuses (the
    /// typed notice lane), and the route's dispatch-scoped disarm now
    /// covers the survivor: the restored pre-close `focused_habit` dies
    /// with the refusal instead of outrunning it. The refusal path is
    /// constructed directly at the `route_menu_action` tier (the arm-time
    /// dead subject — no walkthrough through today's live-only openers can
    /// produce it).
    #[test]
    fn a_refused_quota_route_clears_the_restored_focus() {
        on_slint_thread(|| {
            let (state, _task, habit) = fixture();
            let rig = rig(state);
            // The row click seats the focus fact (the menu routes restore
            // exactly this capture).
            rig.handle.window().invoke_habits_row_activated(0);
            assert_eq!(
                rig.adapter().borrow().test_focused_habit(),
                Some(habit),
                "precondition: the row click seated the focus fact"
            );

            // The subject dies before the quota route (the arm-time dead
            // subject).
            let mut dead = rig.handle.test_state().load_full().as_ref().clone();
            let row = dead.habits.habits.get_mut(&habit).expect("fixture row");
            row.deleted = true;
            rig.handle.test_state().store(Arc::new(dead));

            // The quota menu route: the close leg restores the captured
            // focus, then the dispatch refuses on the dead subject.
            {
                let adapter_rc = rig.adapter();
                let mut a = adapter_rc.borrow_mut();
                a.route_menu_action(rig.handle.window(), ActionId::HabitQuotaEdit);
            }

            let adapter = rig.adapter();
            let a = adapter.borrow();
            assert!(
                !rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the route closes the menu"
            );
            assert!(a.test_notice_armed(), "the refusal surfaces typed");
            assert_eq!(
                a.test_focused_habit(),
                None,
                "the refusal disarms the restored focus (#1478)"
            );
        });
    }

    // -- #1328: the MenuBack root-close leg honours the reset law -----------

    /// The Escape ladder's final close resets the page models exactly like
    /// `apply_close_menu` — `menu_entries`/`root_entries`/`menu_chain`/
    /// `menu_kind`/`menu_subject`/`quick_bar_entries` never survive the
    /// close.
    #[test]
    fn the_menu_back_root_close_resets_the_page_models() {
        on_slint_thread(|| {
            let (state, _task, _habit) = fixture();
            let rig = rig(state);
            // The main menu open (the root page's chain and entries mount).
            rig.route(ActionId::AppMenu);
            assert!(
                rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the menu opened"
            );
            assert!(
                !rig.adapter().borrow().test_menu_models_reset(),
                "precondition: an open menu holds page models"
            );

            // The ladder's close leg: MenuBack at the root.
            rig.adapter()
                .borrow_mut()
                .apply_effect_for_test(rig.handle.window(), UiEffect::MenuBack);

            assert!(
                !rig.handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "the close closed"
            );
            assert!(
                rig.adapter().borrow().test_menu_models_reset(),
                "the close leg resets every page model (#1328)"
            );
        });
    }

    // -- #1315: the habit card's hint-keys ride the keymap projection -------

    /// The habit row card's tooltip hint-keys equal the keymap's
    /// first-binding projection for the row's edit verb over the visible
    /// chain — the task-card grammar's structural oracle (the rendered
    /// model is compared against the projection's own output, never label
    /// prose). Red pre-fix: the card model hardcoded the empty list.
    #[test]
    fn the_habit_card_hint_keys_match_the_keymap_projection() {
        on_slint_thread(|| {
            use slint::Model as _;
            let (state, _task, _habit) = fixture();
            let rig = rig(state);
            let chain = rig.adapter().borrow().shell.chain();
            let keymap = crate::keymap::Keymap::from_preset(crate::keymap::Preset::Standard);
            let projected: Vec<slint::SharedString> = keymap
                .first_binding_display(ActionId::HabitEdit, &chain)
                .unwrap_or_default()
                .iter()
                .map(|stroke| slint::SharedString::from(stroke.as_str()))
                .collect();

            let model = rig.handle.window().get_habits_model();
            let row = model.row_data(0).expect("the fixture habit row renders");
            let rendered: Vec<slint::SharedString> = (0..row.hint_keys.row_count())
                .filter_map(|i| row.hint_keys.row_data(i))
                .collect();
            assert_eq!(
                rendered, projected,
                "#1315: the habit card's hint-keys are the keymap projection"
            );
        });
    }

    // -- #1449: the task variant titles the open task -----------------------

    /// The `TaskDetail` task variant's toolbar title is the open task's
    /// STORED title — the habit arm's shape one arm over; the board's
    /// title never answers for the detail root (the fixture renames the
    /// board so the two oracles cannot collide).
    #[test]
    fn the_task_variant_titles_the_open_task() {
        on_slint_thread(|| {
            let (state, task, _habit) = fixture();
            let rig = rig(state);
            // The detail mounts over the fixture task (the walk lands, the
            // activation opens the detail root).
            rig.route(ActionId::NavNextItem);
            rig.route(ActionId::TaskEdit);
            assert_eq!(
                rig.handle.window().get_current_view(),
                crate::View::TaskDetail,
                "precondition: the task detail root mounts"
            );
            let stored = rig
                .handle
                .test_state()
                .load()
                .tasks
                .get(&task)
                .map(|t| t.title.clone())
                .expect("fixture task");
            assert_eq!(
                rig.handle.window().get_toolbar_title(),
                stored.as_str(),
                "the toolbar title is the open task's stored title (#1449)"
            );
        });
    }
}
