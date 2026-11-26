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
use slint::{ComponentHandle as _, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{AppState, BoardId, ReminderRuleId, StackId, StateCommand, TaskId};

use tokio::sync::oneshot;

use crate::actions::{ContextId, REGISTRY};
use crate::keymap::Keymap;
use crate::menu::MenuEntry;
use crate::models::IdTables;
use crate::presentation::ReminderTarget;
use crate::requests::ConnectorInfo;
use crate::resolver::KeymapResolver;
use crate::router::UiEffect;
use crate::shell::KeymapShell;
use crate::view_state::ViewState;
use crate::{AppWindow, UiStatus};

#[cfg(test)]
use crate::actions::ActionId;

mod effects;
mod interpret;
mod notices;
mod refresh;
mod replies;
mod seams;
mod settings;
mod settings_vm;
mod sync;
mod walk;

use seams::install_view_seams;
pub use settings_vm::SettingsOutcome;
use settings_vm::{SettingsVm, StatusPanelVm};

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
