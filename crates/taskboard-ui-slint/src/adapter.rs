// SPDX-License-Identifier: AGPL-3.0-only

//! The slint adapter (chunk decision D18, design §5.6): the only code
//! that touches both worlds — slint key events in, keymap types out,
//! and shell state synced one-way onto window globals. The root
//! `FocusScope` is the single key path (§5.1); `KeyBinding` is unused
//! for any rebindable action (Q9); the keymap is applied one-way at
//! UI start (no live re-map machinery — §5.6).

use std::cell::RefCell;
use std::collections::BTreeMap;
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

use crate::actions::{ActionId, ContextId, MenuGroup};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::models::IdTables;
use crate::presentation::ReminderTarget;
use crate::requests::{
    self, ConnectionProfile, ConnectorInfo, InvalidKind, KeymapUpdate, Secret, SettingsError,
    UiRequest,
};
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{ActionRouter, DispatchOutcome, EffectClass, Mode, UiEffect, WalkIntent};
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

use crate::level_state::TrackFormState;

/// The walk's planner-handoff return hint (see the field docs on
/// `Adapter`).
struct WalkResume {
    cadence: taskboard_domain::ReviewCadence,
    reference: chrono::NaiveDate,
}

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
    /// The review walk's cross-surface return hint (DR-7 chunk 3,
    /// #638; chunk 1 D9): the shape-B planner handoff. The walk's
    /// prompt answers and armed date field live in its level.
    walk_resume: Option<WalkResume>,
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
    /// sender (`try_send`, warn-and-drop — the F-3 convention).
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
    /// The Add-task ghost's clicked stack (r4 D4); consumed by the
    /// next `task.create` dispatch, then cleared (one-shot, the
    /// `reminder_target` precedent).
    create_target: Option<StackId>,
    /// The banner row target set by row activation (D4); consumed by
    /// the next snooze/mute dispatch, then cleared (one-shot).
    reminder_target: Option<ReminderTarget>,
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
    /// The reorder's resolved (habit, order) (DR-6/6): consumed by the
    /// next `habit.move` dispatch, then cleared.
    habit_move: Option<(taskboard_domain::HabitId, i64)>,
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
    /// The recur row's adapter-owned flags (issue #506, plan D-8):
    /// the inline pause editor and the armed confirm strip — cleared
    /// when the subject stops recurring (the buffers-never-outlive-
    /// the-flow rule).
    recur_pause_open: bool,
    recur_confirm_armed: bool,
    /// The habits section's armed conversion row (issue #506): the
    /// row index the confirm strip renders for; `None` = disarmed.
    habit_convert_armed: Option<usize>,
    /// The one-shot focus hand-over (round-3 D10, r4 D25): the phase
    /// step that owns the next push's focus — `None` keeps the status
    /// quo, `Grid` hands focus to the overlay's grid (the
    /// Down-in-field stroke, the Escape step-back), `Field` hands it
    /// back to the armed field (the grid's Enter). Mutually
    /// exclusive by construction — one hand-over per pass.
    focus_handover: FocusHandover,
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
    /// The OS-delivery health lane (round-3 chunk 7, D31): the watch
    /// receiver the app hands over at mount; drained non-blocking
    /// beside the two broadcasts — a failure repaints within one
    /// cadence tick, no new thread or task in the UI.
    delivery_health: tokio::sync::watch::Receiver<crate::presentation::DeliveryHealth>,
    /// The files-sync chip lane (DR-8 chunk 4, D-FH5): `None` when the
    /// substrate actor is off — the chip stays hidden.
    files_sync: Option<tokio::sync::watch::Receiver<crate::presentation::FilesSyncChip>>,
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
        let palette_selected = match self.view_state.flow() {
            crate::view_state::Flow::Palette { selected } => *selected,
            _ => 0,
        };
        window.set_palette_selected_index(i32::try_from(palette_selected).unwrap_or(i32::MAX));
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_vi_insert(self.shell.mode == crate::router::Mode::Insert);
        keyboard.set_menu_open(self.shell.menu.open);
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
        keyboard.set_menu_active_group(match self.shell.menu.active_group {
            None => -1,
            Some(group) => self
                .root_entries
                .iter()
                .position(|entry| entry.submenu == Some(group))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        });
    }

    /// The two-tier menu models (round-1 D6): the root page model
    /// stays mounted for the whole open lifecycle; the submenu model
    /// carries the active group's page (empty at the root).
    fn sync_menu_models(&self, window: &AppWindow) {
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_menu_entries(Self::menu_model(&self.root_entries));
        keyboard.set_menu_submenu_entries(Self::menu_model(&self.menu_entries));
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
        self.sync_view_context();
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
        // The dropdown ladders (special case D, r4 D39/D40, round-5
        // D6): while a flyout is open, its keys decline the single
        // key path — the focused field's modal arm owns them.
        let ladder_open = if window.global::<crate::SettingsBoardFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::SettingsBoard)
        } else if window.global::<crate::BoardBarFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::BoardBar)
        } else if window.global::<crate::TimeFieldFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::TimeField)
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

    fn set_feedback(window: &AppWindow, feedback: KeyFeedback) {
        window.global::<KeyboardState>().set_last_feedback(feedback);
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
    pub(crate) fn route(&mut self, window: &AppWindow, id: ActionId) {
        let mut ctx = self.shell.dispatch_ctx();
        ctx.create_target = self.create_target.take();
        ctx.reminder_target = self.reminder_target;
        self.reminder_target = None;
        ctx.board_target = self.board_target;
        self.board_target = None;
        ctx.rule_target = self.rule_target;
        self.rule_target = None;
        ctx.rule_add_when = self.rule_add_when;
        self.rule_add_when = None;
        ctx.timer_draft = self.timer_draft.take();
        ctx.timer_target = self.timer_target.take();
        ctx.track_entry_draft = self.track_entry_draft.take();
        ctx.track_entry_target = self.track_entry_target.take();
        ctx.plan_target = self.plan_target.take();
        ctx.track_target = self.track_target.take();
        ctx.habit_target = self.habit_target.take();
        ctx.habit_move = self.habit_move.take();
        ctx.habit_repair = self.habit_repair.take();
        ctx.habit_convert_target = self.habit_convert_target.take();
        ctx.pattern_draft = self.pattern_draft.take();
        ctx.series_target = self.series_target.take();
        ctx.pause_draft = self.pause_draft.take();
        ctx.convert_target = self.convert_target.take();
        // The today window rides every dispatch (chunk 10 D4): state-
        // derived from the held pair, never consumed.
        ctx.day_window = Some(self.day_ctx().window);
        let outcome = ActionRouter::dispatch(id, &ctx);
        match outcome {
            DispatchOutcome::Command(command) => {
                // Non-blocking, human-input-scale (D16): a full inbox
                // is logged and dropped — the forwarder convention.
                if self.commands.try_send(command).is_err() {
                    tracing::warn!(action = %id, "command channel full or closed; dropped");
                }
            }
            DispatchOutcome::UiEffect(effect) => self.apply_effect(window, effect),
            DispatchOutcome::Disabled | DispatchOutcome::NoTarget => {}
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

    /// The shared post-mutation sync (r2 D12): the view-context seam
    /// realignment plus the shell → window global push. `route`'s
    /// trailing pair, factored out so the mouse menu paths
    /// (`activate_parent`, `apply_close_menu`, the `MenuBack` arm)
    /// update the same globals the keyboard path updates — the parent
    /// rows' `menu-active-group` used to stay stale (F7).
    fn finish_mutation(&mut self, window: &AppWindow) {
        self.sync_view_context();
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
            stroke,
            at_key_doorway,
        })
    }

    /// Keeps the shell's view seam aligned with the view-state
    /// machine (the D16/D17 seam, now driven for real).
    fn sync_view_context(&mut self) {
        let state = self.state.load();
        self.shell.view = Some(self.view_state.context());
        self.shell.focused_task = self.view_state.focused_task();
        self.shell.focused_stack = self.view_state.focused_stack(&state);
        self.shell.palette = matches!(
            self.view_state.flow(),
            crate::view_state::Flow::Palette { .. }
        );
        self.shell.focus_overlay = matches!(
            self.view_state.flow(),
            crate::view_state::Flow::FocusMode { .. }
        );
        self.shell.track_entries = matches!(
            self.view_state.flow(),
            crate::view_state::Flow::TrackEntries { .. }
                | crate::view_state::Flow::TrackEntryForm { .. }
        );
        self.shell.review_walk = matches!(
            self.view_state.flow(),
            crate::view_state::Flow::ReviewWalk { .. }
        );
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
        let flow_ended =
            self.flow_open == FlowLiveness::Open && matches!(self.view_state.flow(), Flow::None);
        for outcome in outcomes {
            match outcome {
                FlowOutcome::Command(command) => {
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
                FlowOutcome::PlannerHandoff => {
                    // The walk's boundary ask (chunk 3's exit
                    // protocol): the shape-B transition, exactly the
                    // adapter-side half of `WalkStep::Handoff` — the
                    // walk is still open (the stage does not change on
                    // the handoff), so the window rides with it.
                    if let crate::view_state::Flow::ReviewWalk {
                        cadence, reference, ..
                    } = self.view_state.flow().clone()
                    {
                        self.hand_off_to_planner(window, cadence, reference);
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
            window.invoke_refocus_view();
        }
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

    /// The unguarded D1 pass: reconcile + full re-derive + the cheap
    /// tail. Every caller whose inputs moved outside the state Arc
    /// calls this directly (the plan §5 audit's forced set).
    #[allow(clippy::too_many_lines)] // the pass is one linear pipeline
    pub(crate) fn refresh_forced(&mut self, window: &AppWindow) {
        self.sync_view_context();
        let state = self.state.load_full();
        self.sync_settings(&state.settings);
        self.view_state.reconcile(&state);
        // The walk's refresh-time normalization (plan D-W2): the
        // cursor clamps against the pushed row count, an emptied
        // stage advances structurally, a dead candidate disarms. And
        // the return hint's abandon rule (plan D-W3): a view that
        // left the planner by any other route drops the hint — the
        // user went elsewhere; the walk never ambushes later.
        if let Flow::ReviewWalk {
            cadence, reference, ..
        } = self.view_state.flow().clone()
        {
            let (rows, _) = self.walk_rows(&state, cadence, reference);
            let candidates = crate::presentation::walk_file_candidates(&state);
            self.view_state.clamp_review_walk(rows, &candidates);
        }
        if self.walk_resume.is_some()
            && self.view_state.view() != crate::view_state::ViewId::PlanNextDay
        {
            self.walk_resume = None;
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
        let editor = self.view_state.date_editor();
        let time_text = editor.map_or("--:--", |e| e.time_text.as_str());
        let time_slots: Vec<chrono::NaiveTime> = if editor.is_some_and(|e| e.picker.is_some()) {
            // The armed gate (round-5 F10): disarmed = the full
            // 96-slot grid (empty digits match everything); the first
            // keystroke re-arms the per-keystroke narrowing.
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
        };
        let time_nearest_row = crate::due_edit::nearest_slot(
            &time_slots,
            editor
                .and_then(|e| e.picker.as_ref())
                .and_then(super::due_edit::DuePickerState::time),
        );
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
        self.habit_convert_armed = self
            .habit_convert_armed
            .filter(|row| *row < self.tables.habits.len());
        let recur = crate::models::RecurRowPush {
            pause_open: self.recur_pause_open,
            confirm_open: self.recur_confirm_armed,
        };
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
        );
        // Round-3 D27c: engine syncs re-grey a live menu — the pages
        // rebuild from the fresh state while the menu holds (the
        // cursor's length bound rides along).
        if self.shell.menu.open {
            let flow = self.view_state.flow().clone();
            let states = self.entry_states(&state, &flow);
            self.root_entries = menu::root_page(&self.keymap, &self.menu_chain, &states);
            self.menu_entries = match self.shell.menu.active_group {
                Some(group) => menu::group_page(&self.keymap, &self.menu_chain, &states, group),
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
        // The pass materialized: close the cache only now — a
        // re-entrant pass before this line would see the stale cache
        // and derive again, never skip a needed pass.
        self.last_derived = Some(state);
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
        let grid = form.picker.as_ref().map_or_else(
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
        window.set_track_form_grid_open(form.picker.is_some());
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

    fn seed_track_form(&mut self, seed: Option<taskboard_domain::TimeEntryId>) {
        use chrono::TimeZone as _;
        let Flow::TrackEntryForm { .. } = self.view_state.flow() else {
            return;
        };
        let state = self.state.load_full();
        let today = chrono::Local::now().date_naive();
        let entry = seed.and_then(|id| state.tracking.entries.get(&id));
        let start_local = entry.map(|e| chrono::Local.from_utc_datetime(&e.start.naive_utc()));
        let Some(form) = self.view_state.track_form_mut() else {
            return;
        };
        *form = TrackFormState {
            seed,
            picker: None,
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
        let today = chrono::Local::now().date_naive();
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
        // The commit steps the surface back to the list (the
        // ladder's forward direction); the overlay stays open (the
        // multi-add posture).
        let outcomes = self.view_state.open_track_entries(subject);
        self.apply_outcomes(window, &outcomes);
        self.finish_mutation(window);
        true
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

    /// The shape-B handoff (plan D-W3): the adapter-local return hint
    /// is set, the walk closes (no command — there is nothing to
    /// commit), and the planner opens through the EXISTING
    /// `(Flow::None, OpenPlanNextDay)` route — the walk reaches the
    /// planner exactly the way the action does, zero shared state
    /// beyond `AppState`. If the view is already `PlanNextDay` (the
    /// walk was opened over the planner) the route is skipped: the
    /// toggle would close it.
    fn hand_off_to_planner(
        &mut self,
        window: &AppWindow,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
    ) {
        self.walk_resume = Some(WalkResume { cadence, reference });
        // The close's wipe is the walk level's drop (D-H2).
        let outcomes = self.view_state.close_review_walk();
        // The planner opens BEFORE the close outcomes' refresh runs:
        // the refresh pass reads the post-open view (the abandon rule
        // must not mistake the inline handoff for a navigation away).
        if self.view_state.view() != crate::view_state::ViewId::PlanNextDay {
            self.interpret(window, UiEffect::OpenPlanNextDay);
        }
        self.apply_outcomes(window, &outcomes);
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
            WalkIntent::Advance => match self.view_state.advance_review_walk() {
                WalkStep::Handoff => {
                    self.hand_off_to_planner(window, cadence, reference);
                    Vec::new()
                }
                WalkStep::Advanced => vec![FlowOutcome::RefreshNow],
                WalkStep::Inert => Vec::new(),
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
                let outcomes = self.view_state.back_review_walk(rows);
                // A rung that unmounted a field (the sub-state clear,
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
            WalkIntent::InboxKeep | WalkIntent::CarryLeave => {
                self.step_walk_row(window, cadence, reference, rows)
            }
            WalkIntent::FileTargetPrev => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file_target(-1, &candidates)
            }
            WalkIntent::FileTargetNext => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file_target(1, &candidates)
            }
            WalkIntent::FileCommit => {
                let Flow::ReviewWalk {
                    stage:
                        WalkStage::Inbox {
                            filing: Some(target),
                        },
                    cursor,
                    ..
                } = self.view_state.flow()
                else {
                    return Vec::new();
                };
                let target = *target;
                let row = crate::presentation::walk_inbox_rows(state)
                    .get(*cursor)
                    .copied();
                let Some(task) = row else {
                    return Vec::new();
                };
                // The end-of-stack append the move flow computes — no
                // new arithmetic (plan D-W4).
                let order = crate::view_state::append_order(state, target);
                let mut outcomes = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::MoveTask {
                        id: task,
                        stack: target,
                        order,
                    },
                )];
                match self.view_state.review_walk_next_row(rows) {
                    WalkStep::Handoff => {
                        self.apply_outcomes(window, &outcomes);
                        self.hand_off_to_planner(window, cadence, reference);
                        return Vec::new();
                    }
                    WalkStep::Advanced => outcomes.push(FlowOutcome::RefreshNow),
                    WalkStep::Inert => {}
                }
                outcomes
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
                self.finish_walk_row(window, cadence, reference, rows, step)
            }
            WalkIntent::CarryPickDate => self.view_state.review_walk_pick_date(),
            WalkIntent::CarryDateCommit => {
                let Flow::ReviewWalk {
                    stage: WalkStage::CarryForward { picking_date: true },
                    ..
                } = self.view_state.flow()
                else {
                    return Vec::new();
                };
                let Some(task) = self.walk_cursor_task(state, cadence, reference, rows) else {
                    return Vec::new();
                };
                // The due editor's parser verbatim (plan D-W4); an
                // unparseable date dispatches nothing and keeps the
                // text (the planner-form rule).
                let Ok(Some(instant)) = crate::due_edit::parse(
                    self.view_state
                        .walk()
                        .map_or("", |walk| walk.date_text.as_str()),
                    self.day_ctx().now,
                    &calendar.offset,
                ) else {
                    return Vec::new();
                };
                let step = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::UpdateTask {
                        id: task,
                        changes: taskboard_domain::TaskChanges {
                            scheduled: Some(Some(instant)),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    },
                )];
                let outcomes = self.finish_walk_row(window, cadence, reference, rows, step);
                // The date field's commit keeps the walk alive — the
                // in-level clear (the one surviving per-field wipe).
                if let Some(walk) = self.view_state.walk_mut() {
                    walk.date_text.clear();
                }
                outcomes
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
                    let outcomes = self.view_state.close_review_walk();
                    self.apply_outcomes(window, &outcomes);
                    // The prompt editors died with the flow (no loss
                    // events): reset the facts, re-anchor the focus.
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                    return Vec::new();
                }
                let step = vec![FlowOutcome::Command(
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
                let mut outcomes = step;
                outcomes.extend(self.view_state.close_review_walk());
                self.apply_outcomes(window, &outcomes);
                // The prompt editors died with the flow (no loss
                // events): reset the facts, re-anchor the focus.
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            WalkIntent::Finish => {
                let outcomes = self.view_state.close_review_walk();
                self.apply_outcomes(window, &outcomes);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
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
        let Flow::ReviewWalk { cursor, .. } = self.view_state.flow() else {
            return None;
        };
        let window = taskboard_domain::review_window(cadence, reference, self.calendar());
        crate::presentation::walk_carry_rows(&state.tasks, cadence, window, self.day_ctx().now)
            .get(*cursor)
            .copied()
            .filter(|_| rows.carry > 0)
    }

    /// A commandless row step (`Keep in inbox`, `Leave as is`); a
    /// stage-passing step into the handoff runs the transition.
    fn step_walk_row(
        &mut self,
        window: &AppWindow,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
        rows: crate::view_state::WalkRows,
    ) -> Vec<FlowOutcome> {
        match self.view_state.review_walk_next_row(rows) {
            WalkStep::Handoff => {
                self.hand_off_to_planner(window, cadence, reference);
                Vec::new()
            }
            WalkStep::Advanced => vec![FlowOutcome::RefreshNow],
            WalkStep::Inert => Vec::new(),
        }
    }

    /// A row step that dispatches `step` first (the carry commits):
    /// the command lands, then the cursor advances (or the handoff
    /// runs after the write).
    fn finish_walk_row(
        &mut self,
        window: &AppWindow,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
        rows: crate::view_state::WalkRows,
        step: Vec<FlowOutcome>,
    ) -> Vec<FlowOutcome> {
        let mut outcomes = step;
        match self.view_state.review_walk_next_row(rows) {
            WalkStep::Handoff => {
                self.apply_outcomes(window, &outcomes);
                self.hand_off_to_planner(window, cadence, reference);
                Vec::new()
            }
            WalkStep::Advanced => {
                outcomes.push(FlowOutcome::RefreshNow);
                outcomes
            }
            WalkStep::Inert => outcomes,
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
        self.walk_resume = None;
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
            UiEffect::FocusNext | UiEffect::FocusPrev | UiEffect::FocusFirst | UiEffect::FocusLast
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
            (Flow::DragMove { .. }, UiEffect::FocusNext | UiEffect::FocusPrev) => Vec::new(),
            // -- the move flows (D8/D9; round 2 adds the stack move) --
            (
                flow @ (Flow::Move { .. } | Flow::MoveStack { .. }),
                effect @ (UiEffect::FocusNext
                | UiEffect::FocusPrev
                | UiEffect::Activate
                | UiEffect::RequestMove
                | UiEffect::CancelMove
                | UiEffect::SetMode(Mode::Normal)),
            ) => self.interpret_move(flow, effect, &state),
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
                self.view_state.begin_capture()
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
            (_, UiEffect::TrackEntriesOpen) if self.kiosk => Vec::new(),
            (
                Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. },
                UiEffect::TrackEntriesOpen,
            ) => Vec::new(),
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
                    self.series_target = Some(task);
                    self.recur_pause_open = true;
                    self.recur_confirm_armed = false;
                }
                Vec::new()
            }
            (_, UiEffect::HabitConvertArm | UiEffect::SeriesConvertArm) => Vec::new(),
            // The Escape ladder (D-TS2), level-aware like `MenuBack`:
            // a focused form field disengages first (mode flip; the
            // overlay stays), then the form steps to the list, then
            // the overlay closes.
            (_, UiEffect::TrackEntryDismiss) if self.shell.mode == Mode::Insert => {
                vec![FlowOutcome::SetMode(Mode::Normal)]
            }
            (_, UiEffect::TrackEntryDismiss) => self.view_state.step_back_track_entries(),
            // -- the capture commit/close (the palette's modal peer);
            //    NOT `interpret_edit` — capture's non-empty commit
            //    deliberately keeps the flow --
            (Flow::Capture, UiEffect::Activate) => self.view_state.commit_capture(),
            (Flow::Capture, UiEffect::SetMode(Mode::Normal)) => self.view_state.close_capture(),
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
                | UiEffect::BoardRename),
            ) => self.interpret_lifecycle(&flow, effect, &state),
            // Round-1 chunk 3 (D28b, D6.3): toggle semantics — the
            // menu row, `Ctrl+,`, and a second press all close an open
            // settings surface. No explicit dismiss: both toggle
            // directions swap the mounted view, the identity diff at
            // the choke point covers them (r4 chunk 4).
            (Flow::None, UiEffect::OpenSettings) => {
                if self.view_state.view() == crate::view_state::ViewId::Settings {
                    self.view_state.close_settings()
                } else {
                    self.view_state.open_settings()
                }
            }
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
            (Flow::None, UiEffect::PlanCommitPlace) => Vec::new(),
            // -- the review walk (DR-7 chunk 3, #638) --
            // Kiosk: the dispatch is structurally inert (layer 2 of
            // the three-layer exclusion, D-J14 — the pinned gate).
            (_, UiEffect::OpenReviewWalk | UiEffect::ReviewWalk(_)) if self.kiosk => Vec::new(),
            // Already in the walk: the opener is not a toggle (the
            // modal discipline — Escape out first; the walk-over-
            // planner leg rides the handoff's skip instead).
            (Flow::ReviewWalk { .. }, UiEffect::OpenReviewWalk) => Vec::new(),
            // The opener (the action, the Today card, the menu): the
            // reference derives from `review_due` when a window is
            // due, else the current day window (the evening review
            // before the boundary).
            (Flow::None, UiEffect::OpenReviewWalk) => {
                let reference = self.walk_reference(&state);
                let cadence = taskboard_domain::ReviewCadence::Day;
                let (rows, _) = self.walk_rows(&state, cadence, reference);
                self.view_state.open_review_walk(cadence, reference, rows)
            }
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
                    let outcomes = self.view_state.close_plan_next_day();
                    // The walk's return (plan D-W3): closing the
                    // planner is the built-in skip (P12) and resumes
                    // the same way — the walk re-opens at Prompts over
                    // the restored view. Placed tasks reach it as
                    // plain state (they are `UpdateTask` writes in the
                    // published snapshot).
                    if let Some(WalkResume { cadence, reference }) = self.walk_resume.take() {
                        let mut resumed = self.view_state.open_review_walk_at(
                            cadence,
                            reference,
                            WalkStage::Prompts,
                        );
                        resumed.extend(outcomes);
                        resumed
                    } else {
                        outcomes
                    }
                } else {
                    self.view_state.open_plan_next_day()
                }
            }
            // The row verbs (dispatch-time gating: the rows only
            // render on the surface; a stale target is inert).
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
            (Flow::None, UiEffect::PlanRemoveBox(task)) => {
                // The unplace's wipe is the level's drop (D-H2).
                self.view_state.remove_plan_box(task)
            }
            (Flow::None, effect @ (UiEffect::PlanBoxEarlier(_) | UiEffect::PlanBoxLater(_))) => {
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
        // mutates view state, it moves slint focus only.
        if matches!(effect, UiEffect::ControlNext | UiEffect::ControlPrev) {
            let backwards = effect == UiEffect::ControlPrev;
            let flag = self.walk_reentry.clone();
            Self::dispatch_traversal(window, &flag, backwards, &self.walk_timer);
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
            | crate::view_state::ViewId::PlanNextDay => None,
        };
        let Some((target_a, target_b)) = hint else {
            return;
        };
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(target_a);
        scroll.set_target_b(target_b);
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
                if backwards {
                    let shift: slint::SharedString =
                        slint::platform::Key::into(slint::platform::Key::Shift);
                    win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                        text: shift.clone(),
                    });
                    win.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: tab });
                    win.dispatch_event(slint::platform::WindowEvent::KeyReleased { text: shift });
                } else {
                    win.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: tab });
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

    /// The move flows' arms (D8/D9; the round-2 stack move joins),
    /// factored out of the table.
    fn interpret_move(
        &mut self,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match (flow, effect) {
            // Focus next/prev cycle the target; Activate confirms;
            // every cancel-shaped effect (the dedicated verb — Escape
            // / emacs C-g —, the toggle re-request, and
            // SetMode(Normal), which no longer falls through to the
            // inert default leaving the flow intact) ends the flow
            // without dispatching (D16/F-E2).
            (_, UiEffect::FocusNext | UiEffect::FocusPrev) => {
                let direction = if effect == UiEffect::FocusNext { 1 } else { -1 };
                match flow {
                    Flow::Move { .. } => self.view_state.cycle_move_target(state, direction),
                    _ => self.view_state.cycle_move_stack_target(state, direction),
                }
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
            // The placement form (#622): the kept planner arms own
            // its intents and settle leaves the form (its exits are
            // its close paths) — the overlay cells' shape.
            | Flow::PlanPlace { .. }
            // The review walk (#638): the fifth modal — its kept arms
            // own the walk's intents; settle is its close (the
            // palette family's terminator).
            | Flow::ReviewWalk { .. }
            // The child levels (chunk 3's ports): the modals' row —
            // the children have no kept intents of their own; their
            // close paths are their exits and their settle arm is the
            // Cancelled end (D5).
            | Flow::EntryDatePicker
            | Flow::WalkFilePick { .. }
            | Flow::WalkDatePick
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
                // modals' cancel).
                self.settle_flow(window, state);
            }
            Resolve::Replace => {
                // The one X: the capture overlay's fire-drill replace
                // without committing (chunk-10 D8).
                debug_assert!(matches!(effect, UiEffect::BeginCapture));
                let was_drag = matches!(flow, Flow::DragMove { .. });
                let outcomes = self.view_state.begin_capture();
                self.width_drag = None;
                self.scale_drag = None;
                self.apply_outcomes(window, &outcomes);
                if was_drag {
                    Self::teardown_runtime_drag(window);
                }
            }
            Resolve::SettleThenRun => {
                // Settle per the flow's kind rule, then re-run the
                // effect once against the settled state — bounded
                // one-level re-dispatch (the `InvokeSelected`
                // precedent, `router.rs`). The settle-nothing skip is
                // retired (r4 D4b): `resolve_conflict` early-returns
                // on `Flow::None`, so the gate was provably dead —
                // and it was the only shape that could silently eat a
                // re-dispatch if the kept-arm set ever shifted.
                self.settle_flow(window, state);
                self.interpret(window, effect);
            }
        }
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
            // auto-settle-then-run is the visible feedback.
            (_, C::Begin | C::Title) => R::SettleThenRun,
            // NAV / LIFECYCLE / overlay-open (palette, the focus
            // overlay, the entries overlay): R3 settle-then-run.
            (_, C::Nav | C::Lifecycle | C::OpenPalette | C::FocusOverlay | C::TrackOverlay) => {
                R::SettleThenRun
            }
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
        match effect {
            UiEffect::Exit => {
                self.quit_requests += 1;
                let _ = crate::quit();
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
                    self.root_entries =
                        menu::root_page(&self.keymap, &self.menu_chain, &states);
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
            }
            UiEffect::MenuBack => {
                if self.shell.palette {
                    // Esc / dismiss / mouse-out all mean
                    // close-without-effect (the discard_edit shape).
                    let outcomes = self.view_state.close_palette();
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else {
                    let was_submenu = self.shell.menu.active_group.is_some();
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
            UiEffect::MenuNext | UiEffect::MenuPrevious => {
                if self.shell.palette {
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
            | UiEffect::Activate
            | UiEffect::RequestMove
            | UiEffect::StackWider
            | UiEffect::StackNarrower
            | UiEffect::OpenSettings
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
            | UiEffect::FocusFirst
            | UiEffect::FocusLast
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
            // The habit trio's intents interpret like every view
            // intent (issue #506) — the kiosk/subject refusals live
            // in the arms.
            | UiEffect::SeriesPauseOpen
            | UiEffect::HabitConvertArm
            | UiEffect::SeriesConvertArm
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
            | UiEffect::ReviewWalk(_) => self.interpret(window, effect),
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
            // before the r4 chunk-6 gating.
            window.invoke_refocus_view();
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
        self.sync_menu_models(window);
        // The direct callers (toggle, dismissal layer, mouse invoke)
        // bypass `route`'s trailing sync; the shared helper keeps the
        // globals identical to it (r2 D12).
        self.finish_mutation(window);
    }

    /// Activating a parent row (round-1 D6, keyboard `InvokeSelected`
    /// or a root-panel click): park the root cursor, open the group's
    /// page as the active one. A parent row never dispatches.
    fn activate_parent(&mut self, window: &AppWindow, group: MenuGroup) {
        self.shell.menu.root_selected = self.shell.menu.selected;
        self.shell.menu.selected = 0;
        self.shell.menu.active_group = Some(group);
        {
            let state = self.state.load_full();
            let flow = self.view_state.flow().clone();
            let states = self.entry_states(&state, &flow);
            self.menu_entries = menu::group_page(&self.keymap, &self.menu_chain, &states, group);
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
        if let Some(group) = entry.submenu {
            self.activate_parent(window, group);
            return;
        }
        let Some(action) = entry.action else {
            return;
        };
        self.apply_close_menu(window);
        self.route(window, action);
    }

    /// A root-panel row's invocation (round-1 D6): identical to
    /// [`Self::invoke_entry`] at the root level; inside a submenu the
    /// root panel stays mounted (fly-out), so its rows resolve against
    /// the root page — a parent switches the open group, a quick row
    /// like Quit dispatches.
    fn invoke_root_entry(&mut self, window: &AppWindow, index: usize) {
        let submenu = self.root_entries.get(index).and_then(|entry| entry.submenu);
        if let Some(group) = submenu {
            self.shell.menu.selected = index;
            self.activate_parent(window, group);
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
    Files,
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
    /// The rendered connections items' addresses, index-aligned with
    /// the pushed `connection-items` (the callbacks' resolution table).
    items: Vec<ItemAddr>,
    /// The pickers' option ids, index-aligned with `board-options`.
    local_choices: Vec<uuid::Uuid>,
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
            | SettingsError::EnvLocked { .. } => SettingsOutcome::Rejected,
            SettingsError::VerifyUnreachable | SettingsError::StoreFailed => {
                SettingsOutcome::Unavailable
            }
            SettingsError::VerifyFailed
            | SettingsError::PersistFailed
            | SettingsError::AttachBusy
            | SettingsError::AttachFailed => SettingsOutcome::Failed,
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
        }
    }

    /// The mirror predicate (`settings_form`): the commit seam's
    /// refusal source — the same function the flavour renders.
    fn invalid(self, text: &str) -> Option<settings_form::InvalidKind> {
        match self {
            Self::DayStart => settings_form::day_start_invalid_kind(text),
            Self::TickSecs | Self::PollInterval | Self::BackoffInitial | Self::BackoffMax => {
                settings_form::secs_invalid_kind(text)
            }
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
        paint_files_sync_chip(window, &chip);
        true
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
                        self.settings.remotes.insert(connector.clone(), rows);
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
            files_locked: false,
            files_restart: false,
        };
        let default_identity = vm
            .rows
            .first()
            .map(|row| (row.server_url.as_str(), row.username.as_str()));
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
            if default_identity == Some((card.server_url.as_str(), card.username.as_str())) {
                items.push(crate::ConnectionItem {
                    kind: crate::ConnectionItemKind::Files,
                    files_on: vm.snapshot.files_sync_enabled,
                    files_locked: vm.snapshot.env_overridden.contains("files_sync.enabled"),
                    files_restart: vm.snapshot.applies.get("files_sync.enabled").is_some_and(
                        |semantics| semantics.apply == crate::requests::SectionApply::OnRestart,
                    ),
                    ..blank.clone()
                });
                addrs.push(ItemAddr::Files);
            }
            let several = card.connectors.len() > 1;
            for connector in &card.connectors {
                let listing = vm.remotes.get(connector);
                items.push(crate::ConnectionItem {
                    kind: crate::ConnectionItemKind::Deck,
                    title: connector.clone().into(),
                    show_title: several,
                    busy: matches!(
                        pending,
                        Some(
                            PendingKind::ListRemote
                                | PendingKind::Attach
                                | PendingKind::CreateBind
                                | PendingKind::Detach
                        )
                    ) && vm.busy_target == *connector,
                    listed: listing.is_some(),
                    ..blank.clone()
                });
                addrs.push(ItemAddr::Deck {
                    connector: connector.clone(),
                });
                for remote in listing.into_iter().flatten() {
                    let bound = state.boards.values().find(|board| {
                        !board.deleted
                            && !board.archived
                            && board.remote.as_ref().is_some_and(|binding| {
                                binding.connector.as_str() == connector
                                    && binding.board == remote.remote_id
                            })
                    });
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
            }
        }
        (items, addrs)
    }

    /// Pushes the settings view-model onto the window global (the
    /// refresh pass's settings half; rows fold liveness + the bound
    /// boards' phases, D8). The row lists ride their persistent
    /// handles through the positional diff (r2 D9).
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
        let (items, addrs) = Self::connection_items_model(&state, &self.settings);
        self.settings.items = addrs;
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
        global.set_busy(self.settings.pending.is_some());
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
        ] {
            let predicate: fn(&str) -> Option<settings_form::InvalidKind> = match gf {
                GeneralField::DayStart => settings_form::day_start_invalid_kind,
                GeneralField::TickSecs
                | GeneralField::PollInterval
                | GeneralField::BackoffInitial
                | GeneralField::BackoffMax => settings_form::secs_invalid_kind,
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
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

    /// The account card's Remove seam (round-5 chunk 8): removes the
    /// card's identity — the app refuses typed while a board is bound
    /// through it. A stale or non-card index is absorbed silently.
    pub(crate) fn settings_remove_item(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::Account { connector }) = self.settings.items.get(item).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::RemoveConnection {
            name: connector.clone(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Remove,
            &connector,
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
    /// onto the row by round-5 chunk 8): the same `DetachConnector`
    /// request and multi-board guard. A stale index or an unbound row
    /// is absorbed silently.
    pub(crate) fn settings_disconnect_item(&mut self, window: &AppWindow, item: usize) {
        let Some(ItemAddr::Remote {
            connector,
            remote,
            bound: Some(board),
        }) = self.settings.items.get(item).cloned()
        else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::DetachConnector {
            board,
            connector: connector.clone(),
            reply,
        };
        self.send_connections_op(
            window,
            request,
            reply_rx,
            PendingKind::Detach,
            &connector,
            Some(remote),
        );
    }

    /// The default account's Files toggle seam (round-5 chunk 8,
    /// D-8.6; commit-on-pick): 0 = On, 1 = Off. An env-locked row
    /// absorbs the pick (the app-side guard is the authority).
    pub(crate) fn settings_files_picked(&mut self, window: &AppWindow, index: usize) {
        let Some(enabled) = [true, false].get(index).copied() else {
            return;
        };
        if self.settings.pending.is_some() {
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("files_sync.enabled")
        {
            tracing::debug!("files sync pick on an env-locked row; absorbed");
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SetFilesSyncEnabled { enabled, reply };
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self.settings.snapshot.env_overridden.contains(gf.key()) {
            tracing::debug!(
                key = gf.key(),
                "general commit on an env-locked row; absorbed"
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("reminders.enabled")
        {
            // Choice commits are typed booleans — env-locked picks
            // are absorbed at the defence arm (the segments render
            // disabled; the row's source is named by the badge).
            tracing::debug!("reminders pick on an env-locked row; absorbed");
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.enabled")
        {
            tracing::debug!("focus enabled pick on an env-locked row; absorbed");
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.chain")
        {
            tracing::debug!("focus chain pick on an env-locked row; absorbed");
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("focus.notify")
        {
            tracing::debug!("focus notify pick on an env-locked row; absorbed");
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
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
        if self
            .settings
            .snapshot
            .env_overridden
            .contains("reminders.sink")
        {
            tracing::debug!("sink pick on an env-locked row; absorbed");
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
        }
    }

    /// The settings surface's mount (r3 D20a writer hygiene, plan
    /// §5.1 draft invariants): the scale field's staged buffer dies at
    /// the mount — a Return-less edit from a previous visit never
    /// survives the surface's unmount, and the re-push renders the
    /// canonical live echo. (The connection draft persists by design;
    /// only the scale buffer's lifetime is bound to its surface.)
    pub(crate) fn settings_view_mounted(&mut self, window: &AppWindow) {
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
        if self.settings.pending.is_some() || !value.is_finite() || !(0.5..=3.0).contains(&value) {
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
        create_target: None,
        reminder_target: None,
        focus_handover: FocusHandover::None,
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
        track_entry_draft: None,
        track_entry_target: None,
        track_target: None,
        plan_target: None,
        habit_target: None,
        habit_move: None,
        habit_repair: None,
        habit_convert_target: None,
        pattern_draft: None,
        series_target: None,
        pause_draft: None,
        convert_target: None,
        recur_pause_open: false,
        recur_confirm_armed: false,
        walk_resume: None,
        habit_convert_armed: None,
        walk_reentry: Rc::clone(&walk_flag),
        walk_timer: slint::Timer::default(),
        flow_open: FlowLiveness::Flowless,
        tables: IdTables::default(),
        models: crate::models::PushModels::default(),
        width_drag: None,
        scale_drag: None,
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
        delivery_health,
        files_sync,
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
    install_menu_dismiss_seam(window, adapter);
    install_board_bar_seam(window, adapter);
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
    install_habit_seams(window, adapter);
    install_recur_seams(window, adapter);
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
    drop(state);
    a.route(&window, ActionId::HabitRepair);

    });

    // The conversion flow: arm shows the consequence strip; Confirm
    // routes the domain verb with the D-2 default pattern.
    install_seam!(window: window.on_habits_convert_armed, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return;
    };
    let Some(target) = a.state.load().habits.habits.get(&habit).cloned() else {
        return;
    };
    a.habit_convert_armed = Some(usize::try_from(index.max(0)).unwrap_or(usize::MAX));
    a.habit_convert_target = Some(habit);
    a.pattern_draft = Some(crate::presentation::convert_pattern_default(target.target));
    window.set_habits_convert_armed_row(index);
    a.route(&window, ActionId::HabitConvertArm);

    });
    install_seam!(window: window.on_habits_convert_confirmed, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return;
    };
    a.habit_convert_target = Some(habit);
    a.habit_convert_armed = None;
    window.set_habits_convert_armed_row(-1);
    a.route(&window, ActionId::HabitConvertToSeries);

    });
    install_seam!(window: window.on_habits_convert_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.habit_convert_armed = None;
    a.habit_convert_target = None;
    a.pattern_draft = None;
    window.set_habits_convert_armed_row(-1);
    a.refresh_forced(&window);

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
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    let pattern = {
        let state = a.state.load();
        let Some(rule) = state.tasks.get(&task).and_then(|t| t.recur.as_ref()) else {
            return;
        };
        crate::presentation::convert_target_default(&rule.pattern)
    };
    a.series_target = Some(task);
    a.convert_target = Some(pattern);
    a.recur_confirm_armed = true;
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

/// The habits section's seams (DR-6/6, D-P13): the +1 resolves the
/// `IdTables.habits` index (stale index → no-op, the notification-row
/// guard shape) and routes `habit.log`; the reorder resolves the drop
/// slot's free interval over the same row source the section renders
/// (what you see is what reorders) and routes `habit.move` with the
/// floor-midpoint order — the UI computes midpoints, the planner
/// stamps.
fn install_habit_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_habits_plus_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.habit_target = Some(habit);
    a.route(&window, ActionId::HabitLog);

    });

    install_seam!(window: window.on_habits_reorder_asked, adapter: adapter;
    bind mut (a, window), args (row, slot), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(row).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    let Some(habit_row) = a.state.load().habits.habits.get(&habit).cloned() else {
        return;
    };
    // The same row source the section renders: the board's live
    // habits in stored `(order, id)` order; the drop slot's free
    // interval brackets it.
    let board = habit_row.board;
    let mut orders: Vec<i64> = a
        .state
        .load()
        .habits
        .habits
        .values()
        .filter(|h| h.is_live() && h.board == board)
        .map(|h| h.order)
        .collect();
    orders.sort_unstable();
    let slot = usize::try_from(slot.max(0)).unwrap_or(usize::MAX);
    let prev = slot.checked_sub(1).and_then(|i| orders.get(i).copied());
    let next = orders.get(slot).copied();
    a.habit_move = Some((habit, crate::presentation::habit_drop_order(prev, next)));
    a.route(&window, ActionId::HabitMove);

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
    fn open_context_menu(&mut self, window: &AppWindow, x: f32, y: f32, ids: &[ActionId]) {
        let chain = [ContextId::Board, ContextId::Global].to_vec();
        self.menu_anchor = MenuAnchor::Pointer(x, y);
        self.menu_chain = chain;
        self.root_entries = menu::object_entries(&self.keymap, &self.menu_chain, ids);
        self.menu_entries = self.root_entries.clone();
        self.shell.menu = crate::shell::MenuState {
            open: true,
            selected: 0,
            len: self.menu_entries.len(),
            active_group: None,
            root_selected: 0,
        };
        self.sync_menu_models(window);
        self.sync(window);
    }

    /// The task card's menu list (round-2 D21 spelling of round-1
    /// D13): the Task group's rows, reopen and move-cancel included.
    fn open_task_context_menu(&mut self, window: &AppWindow, x: f32, y: f32) {
        // The tracker hide-set (issue #492): a tracker card's menu
        // omits the impossible complete verb. Only the impossible verb
        // is hidden — the reopen no-op stays (dispatch-time semantics
        // unchanged); the toggle entry is always offered.
        let state = self.state.load();
        let tracker = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.kind == taskboard_domain::entities::TaskKind::Tracker);
        drop(state);
        let ids: Vec<ActionId> = menu::TASK_CONTEXT
            .iter()
            .copied()
            .filter(|id| !(tracker && *id == ActionId::TaskComplete))
            .collect();
        self.open_context_menu(window, x, y, &ids);
    }
}

/// A board card's right click (round-1 D13): resolve, focus, open.
fn install_board_context_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_task_context_requested, adapter: adapter;
    bind mut (a, window), args (column, row, x, y), {        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some((stack, task)) = a.tables.board_task(column, row) else {
        return;
    };
    let outcomes = a.view_state.activate_card(task, stack);
    a.apply_outcomes(&window, &outcomes);
    a.open_task_context_menu(&window, x, y);

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
    a.open_task_context_menu(&window, x, y);

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
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanBeginPlace);
    // A placed row's re-open seeds the form from the box's instant
    // (D-3): echo the seeded buffer so the mounting field shows it.
    window.set_plan_time_text(SharedString::from(
        a.view_state
            .plan_place()
            .map_or(String::new(), |place| place.time_text.clone()),
    ));

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
    a.plan_target = Some(task);
    a.route(&window, ActionId::PlanBeginPlace);
    window.set_plan_time_text(SharedString::new());

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
        a.open_task_context_menu(&window, x, y);
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
    a.open_context_menu(window, x, y, menu::STACK_CONTEXT);
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
    a.open_context_menu(&window, x, y, menu::BOARD_CONTEXT);

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
    let outcomes = a.view_state.close_settings();
    a.apply_outcomes(&window, &outcomes);
    a.sync_view_context();
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

/// A board-bar segment click (design §3.8): resolve the segment's
/// board against the tables pass that pushed the bar, set the one-shot
/// target, and route `board.select` — every click affordance has an
/// action equivalent (§7.1). A stale index against an older pass is
/// absorbed silently.
fn install_board_bar_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_bar_segment_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.board_ids.get(index).copied() else {
        return;
    };
    a.board_target = Some(board);
    a.route(&window, ActionId::BoardSelect);

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
    // The empty-board preset CTA (r3 D5): the direct command dispatch
    // — no text flow, the flow machine untouched.
    install_seam!(window: window.on_board_default_preset_clicked, adapter: adapter;
    bind mut (a, window), args (), {        let state = a.state.load_full();
    let outcomes = a
        .view_state
        .apply_board_preset(&state, &crate::board_presets::DEFAULT_KANBAN);
    a.apply_outcomes(&window, &outcomes);

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
    // The r4 D24 rule governs the date editor's own exits — a foreign
    // begin (the description field taking focus, journey 13's steal)
    // is one: settle per the commit/discard rule, not the
    // unrelated-settle clean close.
    if matches!(adapter.view_state.flow(), Flow::EditDate { .. }) {
        settle_date_editor(adapter, window);
    }
    let state = adapter.state.load_full();
    adapter.settle_flow(window, &state);
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
    // commit (keeps the flow), and the scrim's discard-and-close
    // (routed through the action layer's ModeNormal semantics).
    install_seam!(window: window.on_capture_text_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_capture_accepted, adapter: adapter;
    bind mut (a, window), args (), {        let outcomes = a.view_state.commit_capture();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_capture_dismiss_requested, adapter: adapter;
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
    install_seam!(window: window.on_task_description_edit_opened, adapter: adapter;
    bind mut (a, window), args (), {        settle_pointer_conflict(&mut a, &window);
    let state = a.state.load_full();
    let Some(task) = a.view_state.subject_task() else {
        return;
    };
    let outcomes = a.view_state.begin_edit_description(&state, task);
    a.apply_outcomes(&window, &outcomes);

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

    install_seam!(window: window.on_task_rule_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
        .tables
        .task_rules
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index against an older tables pass
    };
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
    install_seam!(window: window.on_task_due_edit_opened, adapter: adapter;
    bind mut (a, window), args (), {        begin_date_edit(&mut a, &window, crate::view_state::DateField::Due);

    });

    // The rule section's jump affordance (r4 D30): an unarmed row's
    // due-date repair opens the SAME due editor — the callback is the
    // metadata row's due-edit seam under a second name, no new focus
    // plumbing (chunk 6's machinery arms from there).
    install_seam!(window: window.on_task_rule_jump_to_due_clicked, adapter: adapter;
    bind mut (a, window), args (), {        begin_date_edit(&mut a, &window, crate::view_state::DateField::Due);

    });

    // D31a: the section's un-mute affordance — the focused task is the
    // target (the section always names its subject), the banner's
    // one-shot machinery is not needed.
    install_seam!(window: window.on_task_rule_unmute_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReminderUnmute);

    });

    install_seam!(window: window.on_task_scheduled_edit_opened, adapter: adapter;
    bind mut (a, window), args (), {        begin_date_edit(&mut a, &window, crate::view_state::DateField::Scheduled);

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
    bind mut (a, window), args (text), {        match crate::due_edit::parse(&text, chrono::Utc::now(), &chrono::Local) {
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
    // The click carries the picker's time (round-3 D9): cursor
    // date + time-of-day through the one funnel.
    let Some(instant) = editor
        .picker
        .as_ref()
        .expect("checked above")
        .commit_instant(&chrono::Local)
    else {
        a.refresh_forced(&window);
        return;
    };
    commit_date(&mut a, &window, Some(instant));

    });
}

/// The one begin seam (round-3 D11): settle any live flow, arm
/// `field`'s editor, then seed the buffer with the field's *current
/// value* — inside the armed window, before the outcomes' refresh, so
/// the §8 invariant (non-empty ⇔ armed) holds and an immediate Return
/// re-commits the value instead of clearing it.
fn begin_date_edit(a: &mut Adapter, window: &AppWindow, field: crate::view_state::DateField) {
    settle_pointer_conflict(a, window);
    let state = a.state.load_full();
    let outcomes = a.view_state.begin_edit_date(&state, field);
    let value = a
        .view_state
        .subject_task()
        .and_then(|id| state.tasks.get(&id))
        .and_then(|t| match field {
            crate::view_state::DateField::Due => t.duedate,
            crate::view_state::DateField::Scheduled => t.scheduled,
        });
    if let Some(editor) = a.view_state.date_editor_mut() {
        editor.buffer = crate::due_edit::format_entry(value, &chrono::Local);
        editor.echo = crate::due_edit::echo(&editor.buffer, chrono::Utc::now(), &chrono::Local);
    }
    a.apply_outcomes(window, &outcomes);
    // The picker is default-open on edit entry (round-3 D10) —
    // visible, click-armed, and idle: the text field keeps focus
    // (typing-first preserved). Inert when the begin refused (the
    // flow never armed).
    a.open_due_picker(window, false);
}

impl Adapter {
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
    // Non-empty and parseable: click-away ≡ Return.
    if let Ok(Some(instant)) = crate::due_edit::parse(&text, now, &chrono::Local) {
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

    install_seam!(window: window.on_banner_mute_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.reminder_target = Some(banner_row.target);
    a.route(&window, ActionId::ReminderMute);

    });
}

/// The notification row at `index` (D2: the index resolves against the
/// refresh pass's row table).
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
    install_seam!(window: window.on_task_track_entries_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.track_target = a.view_state.subject_task();
    a.route(&window, ActionId::TaskTrackEntries);

    });

    // The dismissal layer's seam (D-TS1): an outside click is a
    // ladder step (form → list; list → close) — the view-state
    // machine resolves it; the armed delete clears with the overlay
    // (the state-truth rule — nothing ticks).
    install_seam!(window: window.on_track_entries_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.track_entry_target = None;
    let outcomes = a.view_state.step_back_track_entries();
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
    bind mut (a, window), args (), {        let Some(form) = a.view_state.track_form_mut() else {
        return;
    };
    if form.picker.is_some() {
        form.picker = None;
    } else {
        let anchor = chrono::NaiveDate::parse_from_str(&form.date_text, "%Y-%m-%d").ok();
        let anchor = anchor.and_then(|d| {
            use chrono::TimeZone as _;
            chrono::Local
                .from_local_datetime(&d.and_hms_opt(12, 0, 0).expect("noon"))
                .single()
                .map(|local| local.with_timezone(&chrono::Utc))
        });
        form.picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            anchor,
            &chrono::Local,
        ));
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_grid_key, adapter: adapter;
    bind mut (a, window), args (code), {        use crate::due_edit::DatePickerKey as K;
        let Some(key) = crate::due_edit::DatePickerKey::from_code(code) else {
        return;
    };
    let Some(form) = a.view_state.track_form_mut() else {
        return;
    };
    let Some(picker) = form.picker.as_mut() else {
        return; // a stale stroke against a closed grid
    };
    match key {
        K::Left => picker.move_days(-1),
        K::Right => picker.move_days(1),
        K::Up => picker.move_days(-7),
        K::Down => picker.move_days(7),
        K::PrevMonth => picker.move_months(-1),
        K::NextMonth => picker.move_months(1),
        K::MonthStart => picker.month_edge(true),
        K::MonthEnd => picker.month_edge(false),
        // The grid's Enter hands focus to the date field (the
        // due grid's phase hand-over); the commit is the click's
        // or the typed field's.
        K::Commit | K::TimeBack | K::TimeForward => {}
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_day_clicked, adapter: adapter;
    bind mut (a, window), args (cell), {        let Some(date) = a
        .view_state
        .track_form()
        .and_then(|f| f.picker.as_ref())
        .and_then(|picker| {
            picker
                .grid()
                .get(usize::try_from(cell).unwrap_or(usize::MAX))
                .and_then(|c| c.date)
        })
    else {
        return; // a blank or a stale index
    };
    let form = a.view_state.track_form_mut().expect("checked above");
    if let Some(picker) = form.picker.as_mut() {
        picker.set_cursor(date);
    }
    form.date_text = date.format("%Y-%m-%d").to_string();
    a.refresh_forced(&window);

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
fn install_review_walk_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_walk_scrim_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewWalkBack);
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
    if let Some(Flow::ReviewWalk {
        stage: crate::view_state::WalkStage::Inbox { filing },
        ..
    }) = a.view_state.flow_mut()
    {
        *filing = Some(stack);
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
    bind mut (a), args (text), {        if let Some(walk) = a.view_state.walk_mut() {
        walk.date_text = text.to_string();
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
        a.send(taskboard_domain::StateCommand::DismissReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
        });
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
        let Some(task) = row.task else { return };
        a.send(taskboard_domain::StateCommand::SetTaskDone {
            id: task,
            done: true,
        });
    });

    // Snooze / Mute: the banner's routed verbs over the one-shot
    // target — the same registry actions, unchanged. The target's
    // anchor mirrors the ref (task rows task-anchored; alarm rows
    // adhoc-anchored — mute has no button there anyway).
    install_seam!(window: window.on_notification_snooze_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.reminder_target = Some(notification_target(&row));
        a.route(&window, ActionId::ReminderSnooze);
    });

    install_seam!(window: window.on_notification_mute_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.reminder_target = Some(notification_target(&row));
        a.route(&window, ActionId::ReminderMute);
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
        let taskboard_domain::ReminderRef::Adhoc(alarm) = row.reminder else {
            return;
        };
        a.timer_target = Some(alarm);
        a.route(&window, ActionId::TimerCancel);
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
        let Some(task) = row.task else { return };
        a.track_target = Some(task);
        a.route(&window, ActionId::TaskTrackStart);
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
        bind mut (a, window), args (item), {            let Ok(item) = usize::try_from(item) else {
                return;
            };
            a.settings_files_picked(&window, item);

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
/// (`tests/usability_components.rs`) compiles this crate externally and
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
            Some(ContextId::Today) => crate::view_state::ViewId::Today,
            Some(ContextId::Board) => crate::view_state::ViewId::Board,
            Some(ContextId::StackDetail) => crate::view_state::ViewId::StackDetail,
            Some(ContextId::TaskDetail) => crate::view_state::ViewId::TaskDetail,
            Some(ContextId::Review) => crate::view_state::ViewId::Review,
            Some(ContextId::Settings) => crate::view_state::ViewId::Settings,
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
    window.set_files_sync_text(SharedString::from(
        crate::presentation::files_sync_chip_caption(chip.state),
    ));
}
