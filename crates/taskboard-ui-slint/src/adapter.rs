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

use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, ModelRc, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{
    AppState, BoardId, ReminderOffset, ReminderRuleId, StackId, StateCommand, TaskId,
};

use tokio::sync::oneshot;

use taskboard_domain::ConnectorId;

use crate::actions::{ActionId, ContextId, MenuGroup};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::models::IdTables;
use crate::presentation::ReminderTarget;
use crate::requests::{
    self, ConnectionProfile, ConnectionTarget, ConnectorInfo, InvalidKind, KeymapUpdate, Secret,
    SettingsError, UiRequest,
};
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{ActionRouter, DispatchOutcome, EffectClass, Mode, UiEffect};
use crate::scale::UiScale;
use crate::settings_form::{self, ConnectionDraft};
use crate::shell::KeymapShell;
use crate::view_state::{Flow, FlowOutcome, FocusEdge, ViewState};
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

/// The adapter state: the validated keymap (applied at UI start, one
/// way), the shell, the resolver, the router, the view-state machine
/// (D3), the lock-free state read (D1), and the command channel out.
pub(crate) struct Adapter {
    keymap: Keymap,
    /// The today window's derivation pair (chunk 10 D4): held, never
    /// re-read from config; `day_ctx` recomputes the window per pass.
    day_start: (chrono::NaiveTime, chrono::FixedOffset),
    /// The run mode's one UI bit (Q8): on kiosk the today pass filters
    /// to the focused board's subset; the desktop is cross-board.
    kiosk: bool,
    /// The injected reminder defaults (R3 chunk C): the derivation
    /// input `active_notifications` needs for the implicit default's
    /// occurrence — held, never re-read from config (the `day_start`
    /// consumers contract).
    reminder_defaults: taskboard_domain::ReminderDefaults,
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
    /// The rule-list add entry's parsed offset (chunk 6, D13); consumed
    /// by the next `reminder.rule-add` dispatch, then cleared.
    rule_add_offset: Option<ReminderOffset>,
    /// The timers form's parsed draft (DR-3/3, D-P10); consumed by the
    /// next `timer.create` dispatch, then cleared (one-shot).
    timer_draft: Option<crate::router::TimerDraft>,
    /// The timers row's alarm (DR-3/3, D-I8); consumed by the next
    /// `timer.cancel` dispatch, then cleared (one-shot).
    timer_target: Option<taskboard_domain::AdhocAlarmId>,
    /// The date editor's live state (r2 D25/R-d, round-3 D8): the
    /// typed buffer, its echo, and the picker cursor/month/time —
    /// adapter-owned so a refresh push can never reset them. A commit
    /// consumes all three; the clean close keeps the buffer/echo (D25)
    /// and closes the picker. One set for both date fields (the flow
    /// names the field; the editor state stays singular).
    due_buffer: String,
    due_echo: String,
    due_picker: Option<crate::due_edit::DuePickerState>,
    /// The one-shot grid focus flag (round-3 D10): `false` on the
    /// default-open (the text field keeps focus), `true` on the
    /// Down-in-field stroke — consumed by the next push.
    picker_focus_on_mount: bool,
    /// The id tables of the last refresh pass (D2): index → id.
    tables: IdTables,
    /// The persistent model handles of this window (r2 D9): the
    /// refresh pass diffs the desired rows onto them, so unchanged
    /// refreshes notify nothing.
    models: crate::models::PushModels,
    /// The active column drag (round-1 D17): the dragged stack and the
    /// pre-drag base width in rendered px, captured at press. The
    /// drag deltas are absolute since press — idempotent against
    /// dropped events.
    width_drag: Option<(StackId, f32)>,
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
}

impl Adapter {
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
    fn intercept(
        &mut self,
        window: &AppWindow,
        text: &SharedString,
        modifiers: KeyboardModifiers,
    ) -> bool {
        let Some(stroke) = to_stroke(text, modifiers) else {
            return false;
        };
        // The due editor's Down-in-field opens the picker (r2 D25):
        // consumed here, ahead of the resolver, so the insert-mode
        // chain never has to bind it. The stroke is the grammar's own
        // Down row — never a private-use literal.
        if matches!(self.view_state.flow(), Flow::EditDate { .. })
            && self.due_picker.is_none()
            && Ok(stroke) == crate::keymap::stroke::parse("Down")
        {
            self.open_due_picker(window, true);
            return true;
        }
        // The chain reflects the view-state machine, not a stale seam
        // write (D6: shell view/focus derive from view_state).
        self.sync_view_context();
        let chain = self.shell.chain();
        match self.resolver.feed(stroke, &chain, &self.keymap) {
            Resolution::Action(id) => {
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
        ctx.reminder_target = self.reminder_target;
        self.reminder_target = None;
        ctx.board_target = self.board_target;
        self.board_target = None;
        ctx.rule_target = self.rule_target;
        self.rule_target = None;
        ctx.rule_add_offset = self.rule_add_offset;
        self.rule_add_offset = None;
        ctx.timer_draft = self.timer_draft.take();
        ctx.timer_target = self.timer_target.take();
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
        // Round 2 (D19): the reopen gate's done stamp, from the same
        // focused task — the router stays pure.
        self.shell.focused_task_done = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.done.is_some());
    }

    /// Applies flow outcomes verbatim (D9): the adapter never
    /// re-decides. `RefreshNow` re-derives the models immediately —
    /// the timer-driven refresh (D1) covers the passive side. A mode
    /// flip is pushed onto the window global here (the shell's one-way
    /// sync, D18): the text-commit seams end in this method without a
    /// `route()` around them, and the visible chain (the status bar's
    /// mode read) must not go stale.
    fn apply_outcomes(&mut self, window: &AppWindow, outcomes: &[FlowOutcome]) {
        let mut mode_changed = false;
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
                FlowOutcome::RefreshNow => self.refresh(window),
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
        if matches!(self.view_state.flow(), Flow::None) {
            window.invoke_refocus_view();
        }
    }

    /// The pass's time bundle (chunk 10 D4): a fresh `now` and the
    /// window derived from the held pair — recomputed on every pass,
    /// so the rollover rides the existing minute-boundary repaint.
    fn day_ctx(&self) -> crate::view_state::DayCtx {
        let now = chrono::Utc::now();
        let window = taskboard_domain::days::day_window(now, self.day_start.0, self.day_start.1);
        crate::view_state::DayCtx { now, window }
    }

    /// The D1 refresh: reconcile, then one full re-derive of the
    /// display models from the current snapshot (lock-free load) and
    /// a fresh id tables pass. Deterministic, timer-free; the repaint
    /// timer and the tests call this.
    pub(crate) fn refresh(&mut self, window: &AppWindow) {
        self.sync_view_context();
        let state = self.state.load();
        self.view_state.reconcile(&state);
        // §8 (W-SM-c): the date editor's buffers never outlive the
        // flow — non-empty ⇔ `Flow::EditDate` armed. Every outcome
        // path and settle flows through here, so this is the one
        // wipe point (commit, discard, leaving-settle and
        // reconcile-drop all end the flow before this read). The D11
        // buffer seeding happens inside the armed window (the begin
        // seam, before `apply_outcomes`).
        if !matches!(self.view_state.flow(), Flow::EditDate { .. }) {
            self.due_buffer.clear();
            self.due_echo.clear();
            self.due_picker = None;
        }
        debug_assert!(
            matches!(self.view_state.flow(), Flow::EditDate { .. })
                || (self.due_buffer.is_empty()
                    && self.due_echo.is_empty()
                    && self.due_picker.is_none()),
            "the date editor's buffers outlived the flow"
        );
        let chain = self.shell.chain();
        let day = self.day_ctx();
        let due = crate::models::DueEditorPush {
            text: &self.due_buffer,
            echo: &self.due_echo,
            picker: self.due_picker.as_ref(),
        };
        self.tables = crate::models::push(
            window,
            &state,
            &self.view_state,
            &self.keymap,
            &chain,
            day,
            self.kiosk,
            &self.reminder_defaults,
            &mut self.models,
            &due,
        );
        self.push_settings(window);
    }

    /// Whether the window is at the compact breakpoint (D17).
    fn is_compact(window: &AppWindow) -> bool {
        window.global::<Responsive>().get_current_breakpoint() == Breakpoint::Compact
    }

    /// The keyboard resize's rendered-px ceiling (round-1 D18): the
    /// window width minus the *other* live columns' effective minimums
    /// (user width if set, else the floor) minus the row's fixed
    /// furniture (one `space-3` gap per inter-column boundary plus the
    /// add-column affordance's `space-6 + space-2`), via the pure
    /// [`crate::widths::width_ceiling`]. All values are rendered slint
    /// px — the space `window-w` and the drag deltas share.
    fn stack_width_ceiling(&self, state: &AppState, window: &AppWindow) -> u32 {
        let target = self.view_state.keyboard_resize_target(state);
        self.stack_width_ceiling_excluding(state, window, target)
    }

    /// [`Self::stack_width_ceiling`] over an explicit excluded column
    /// (the drag path excludes the dragged column, which need not be
    /// the keyboard target).
    fn stack_width_ceiling_excluding(
        &self,
        state: &AppState,
        window: &AppWindow,
        excluded: Option<StackId>,
    ) -> u32 {
        use crate::widths::{STACK_WIDTH_MIN_PX, width_ceiling};
        // The furniture tokens are Theme constants — logical px scaled
        // by the root multiplier the window already carries (reading
        // `base-scale`, not slint lengths).
        let scale = f64::from(window.global::<crate::Theme>().get_base_scale());
        let stack_ids = self.view_state.stack_ids(state);
        let board = self.view_state.board();
        let count = stack_ids.len();
        let gaps = count.saturating_sub(1);
        #[allow(clippy::cast_precision_loss)] // a px sum, far below f64 mantissa reach
        let chrome: f64 = scale * (12.0 * gaps as f64 + 32.0 + 8.0);
        let others: u64 = stack_ids
            .iter()
            .filter(|stack| Some(**stack) != excluded)
            .map(|stack| {
                // The stored widths are logical px — the same
                // `× base-scale` conversion the model push applies
                // (`stack_width_px`); everything in this sum is
                // rendered px, like `window_w` and `chrome`.
                #[allow(clippy::cast_precision_loss)] // a px value
                let effective = scale
                    * f64::from(
                        board
                            .and_then(|board| self.view_state.stack_width(board, *stack))
                            .unwrap_or(STACK_WIDTH_MIN_PX),
                    );
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                u64::from(effective.max(0.0) as u32)
            })
            .sum();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let window_w = (f64::from(window.get_window_w()).max(0.0)) as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let chrome_px = chrome.max(0.0) as u32;
        width_ceiling(
            window_w,
            u32::try_from(others).unwrap_or(u32::MAX),
            chrome_px,
        )
    }

    /// The per-kind settle, adapter side (round-3 W-SM-b, plan §2):
    /// the pure [`ViewState::settle_flow`] rule applied, plus the
    /// adapter-owned attachments — the due picker dies with the due
    /// flow, the gesture buffers (`width_drag`/`scale_drag`, §8) die
    /// with the surfaces they address, and a settled drag tears the
    /// native runtime gesture down (`PointerExited` is the one
    /// public cancel, [`Self::teardown_runtime_drag`]). One taxonomy
    /// for all three trigger families — L1 focus loss, L2 pointer
    /// seams, the leaving effects — so they cannot diverge.
    /// Idempotent on `Flow::None`; returns whether a flow settled.
    fn settle_flow(&mut self, window: &AppWindow, state: &taskboard_domain::AppState) -> bool {
        // §8: the settle step owns the gesture buffers — they are
        // flow-independent by construction, and a leaving effect
        // unmounts the surfaces that would consume them (the release
        // can never arrive). Dropped before the None early-return: a
        // settling pass with no flow armed still retires them.
        self.width_drag = None;
        self.scale_drag = None;
        let flow = self.view_state.flow().clone();
        if matches!(flow, Flow::None) {
            return false;
        }
        let was_drag = matches!(flow, Flow::DragMove { .. });
        let was_date = matches!(flow, Flow::EditDate { .. });
        let outcomes = self.view_state.settle_flow(state);
        if was_date {
            self.due_picker = None;
        }
        self.apply_outcomes(window, &outcomes);
        if was_drag {
            Self::teardown_runtime_drag(window);
        }
        true
    }

    /// The leaving rule (D9, factored out of the table): a view
    /// switch by effect settles the flow through the same per-kind
    /// rule every other trigger uses (round-3 R1–R3) and dismisses
    /// the tooltip (r2 D13 — the anchor view is about to unmount).
    fn settle_flows_on_leaving(&mut self, window: &AppWindow, state: &taskboard_domain::AppState) {
        Self::dismiss_tooltip(window);
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

    /// Resets every machine the table reads so pair passes are
    /// order-independent (the flow table and the buffer journeys).
    #[cfg(test)]
    pub(crate) fn test_reset(&mut self) {
        self.view_state = ViewState::new();
        self.shell = KeymapShell::default();
        self.menu_entries.clear();
        self.root_entries.clear();
        self.menu_anchor = MenuAnchor::default();
        self.due_buffer.clear();
        self.due_echo.clear();
        self.due_picker = None;
        self.width_drag = None;
        self.scale_drag = None;
    }

    /// The date editor's adapter-owned buffers, for the §8 invariant
    /// journeys: non-empty ⇔ `Flow::EditDate` armed. (The journeys
    /// land with the buffer-invariants commit; the seam carries its
    /// allow until then.)
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn due_buffers_empty(&self) -> bool {
        self.due_buffer.is_empty() && self.due_echo.is_empty() && self.due_picker.is_none()
    }

    /// Seeds the date editor's buffers non-empty (the §8 invariant
    /// journeys).
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn test_seed_due_buffers(&mut self) {
        self.due_buffer = "tomorrow".to_owned();
        self.due_echo = "stale echo".to_owned();
        self.due_picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Utc::now().date_naive(),
            None,
        ));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn arm_width_drag(&mut self, stack: StackId) {
        self.width_drag = Some((stack, 100.0));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn width_drag_armed(&self) -> bool {
        self.width_drag.is_some()
    }

    /// The D9 interpretation table: one `match (flow, effect)` whose
    /// arms route to `view_state` methods named by what they do.
    #[allow(clippy::too_many_lines)] // one decision table, kept flat for review
    fn interpret(&mut self, window: &AppWindow, effect: UiEffect) {
        let compact = Self::is_compact(window);
        let state = self.state.load();
        let mut flow = self.view_state.flow().clone();
        let focus_walk = matches!(
            effect,
            UiEffect::FocusNext | UiEffect::FocusPrev | UiEffect::FocusFirst | UiEffect::FocusLast
        );
        let focus_before = self.view_state.focused_task();
        let leaving = matches!(
            effect,
            UiEffect::NextView
                | UiEffect::PrevView
                | UiEffect::FocusBoard
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
            flow = self.view_state.flow().clone();
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
                self.due_picker = None;
                self.view_state.discard_edit()
            }
            (Flow::EditDate { .. }, UiEffect::Activate | UiEffect::SetMode(Mode::Insert)) => {
                Vec::new()
            }
            // -- quick capture (chunk 10 D8): the GLOBAL action, from
            //    any flow — the XOR rule replaces without committing --
            (_, UiEffect::BeginCapture) => self.view_state.begin_capture(),
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
            // -- no flow: the intents begin things --
            (Flow::None, UiEffect::OpenPalette) => self.view_state.open_palette(),
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
                | UiEffect::BeginCreate
                | UiEffect::BeginCreateStack),
            ) => self.interpret_begin(effect, &state),
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
                let ceiling = self.stack_width_ceiling(&state, window);
                self.view_state
                    .resize_focused_stack(&state, direction, ceiling)
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
            // settings surface.
            (Flow::None, UiEffect::OpenSettings) => {
                Self::dismiss_tooltip(window);
                if self.view_state.view() == crate::view_state::ViewId::Settings {
                    self.view_state.close_settings()
                } else {
                    self.view_state.open_settings()
                }
            }
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
        if focus_walk {
            self.push_focus_scroll_hint(window, focus_before);
        }
        if drag_teardown {
            // After the outcomes: the cancel/confirm has ended the
            // view-state flow; the synthetic exit now tears the
            // native drag down and its `drag-finished(none)` echo is
            // absorbed by the flow-guarded seam.
            Self::teardown_runtime_drag(window);
        }
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
            crate::view_state::ViewId::TaskDetail | crate::view_state::ViewId::Settings => None,
        };
        let Some((target_a, target_b)) = hint else {
            return;
        };
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(target_a);
        scroll.set_target_b(target_b);
        scroll.set_nonce(scroll.get_nonce() + 1);
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
            Flow::None => FlowKind::None,
            Flow::Create { .. }
            | Flow::CreateStack
            | Flow::CreateBoard
            | Flow::EditTitle { .. }
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
            // Unreachable by construction: every effect class has a
            // Flow::None arm above, and the menu/system effects are
            // applied in `apply_effect` before the table (audit §5).
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
                // precedent, `router.rs`), guarded on the settle
                // having actually changed the flow.
                if self.settle_flow(window, state) {
                    self.interpret(window, effect);
                }
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
            // NAV / LIFECYCLE / palette-open: R3 settle-then-run.
            (_, C::Nav | C::Lifecycle | C::OpenPalette) => R::SettleThenRun,
            // EDIT residue (the effects the kept arms don't own —
            // SetMode(Insert) under a text/due flow): named inert.
            (_, C::Edit) => R::Inert,
            // MENU/SYSTEM: applied in `apply_effect` before the
            // table (audit §5); unreachable here — typed inert,
            // never silent.
            (_, C::Menu | C::System) => R::Inert,
        }
    }

    fn apply_effect(&mut self, window: &AppWindow, effect: UiEffect) {
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
                // The projection is computed for the pre-menu chain
                // (the active context's actions, §5.9); the menu
                // context itself masks the chain once open (D11).
                let chain = self.shell.chain();
                self.menu_anchor = MenuAnchor::Hamburger;
                self.menu_chain = chain;
                self.root_entries = menu::root_page(&self.keymap, &self.menu_chain);
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
                } else {
                    self.shell.apply(&effect, self.menu_entries.len());
                }
            }
            UiEffect::OpenPalette
            | UiEffect::SetMode(_)
            | UiEffect::FocusNext
            | UiEffect::FocusPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::BeginCreate
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
            // through the same interpretation table.
            | UiEffect::FocusToday
            | UiEffect::OpenInbox
            | UiEffect::BeginCapture => self.interpret(window, effect),
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
        let target = self.tables.palette.get(selected).copied();
        let state = self.state.load();
        let outcomes = match target {
            Some(target) => self.view_state.palette_jump(&state, target),
            // A row count that shrank past the cursor: close only.
            None => self.view_state.close_palette(),
        };
        self.apply_outcomes(window, &outcomes);
        self.finish_mutation(window);
    }

    /// The per-keystroke palette pass (design D10): mirror the buffer,
    /// reset the cursor (every keystroke re-ranks — a stale mid-list
    /// cursor on a fresh ranking is a lie), then the one targeted
    /// [`crate::models::push_palette`] call — no full refresh, no
    /// debounce.
    fn apply_palette_query(&mut self, window: &AppWindow, text: &str) {
        self.view_state.set_buffer(text.to_owned());
        if let crate::view_state::Flow::Palette { selected } = self.view_state.flow_mut() {
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
        self.menu_entries = menu::group_page(&self.keymap, &self.menu_chain, group);
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
    /// `TestConnection`'s typed outcome.
    Test(oneshot::Receiver<Result<(), SettingsError>>),
    /// The list-carrying replies (`SaveConnection`/`RemoveConnection`/
    /// `AttachConnector`/`DetachConnector`) — the full pair (r2 D32).
    List(oneshot::Receiver<Result<requests::ConnectionsSnapshot, SettingsError>>),
    /// `SetKeymapPreset`'s rebuilt keymap.
    Keymap(oneshot::Receiver<Result<KeymapUpdate, SettingsError>>),
    /// `SetScale`, with the scale to revert to on an `Err` reply.
    Scale(oneshot::Receiver<Result<(), SettingsError>>, f64),
}

/// Which operation is in flight (the form's busy marker; one at a
/// time — the buttons disable while pending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingKind {
    Test,
    Save,
    Remove,
    /// `AttachConnector` (r2 D33): the connector row's busy marker.
    Attach,
    /// `DetachConnector` (r2 D33): the board row's busy marker.
    Detach,
    Preset,
    Scale,
}

/// The typed last-outcome marker the model renders structurally
/// (R5: never message text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsOutcome {
    #[default]
    None,
    Ok,
    Busy,
    /// The operation was refused (validation, legacy config, verify
    /// rejected, persist rejected).
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
    /// The board-selector rows (r2 D32): mounted once from
    /// `UiSettings.boards`, refreshed only from reply snapshots.
    boards: Vec<requests::BoardInfo>,
    /// Connector name → down-override (the `ConnectorLost`/
    /// `ConnectorRestored` episodes, D8); absent = no override.
    liveness: BTreeMap<String, bool>,
    /// The selector's radio state (r2 D32): the selected board's uuid.
    selected_board: Option<uuid::Uuid>,
    /// The connector/board name the in-flight op addresses (r2 D34):
    /// set by the send seams, cleared with `pending`.
    busy_target: String,
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
            SettingsError::LegacyConfig
            | SettingsError::Invalid { .. }
            | SettingsError::NoSecretStored
            | SettingsError::StoreUnsupported
            | SettingsError::VerifyRejected
            | SettingsError::PersistRejected
            | SettingsError::Gone
            | SettingsError::NoRemoteMatch
            | SettingsError::BindRefused => SettingsOutcome::Rejected,
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
        settings_form::Field::Name => requests::Field::Name,
        settings_form::Field::ServerUrl => requests::Field::ServerUrl,
        settings_form::Field::Username => requests::Field::Username,
        settings_form::Field::Password => requests::Field::Password,
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

/// The settings surface's adapter half (round-1 chunk 3, D7/D8/D9):
/// the form mirrors, the send seams, the reply drain, and the
/// liveness overrides. Every reply path is non-blocking — the drain
/// polls `try_recv` under the repaint timer, and a dead lane
/// degrades to a cleared pending state (R-reply).
impl Adapter {
    /// The drain-side batch form of
    /// [`Self::record_connector_liveness`].
    pub(crate) fn record_connector_liveness_batch(
        &mut self,
        events: &[taskboard_domain::SystemEvent],
    ) {
        for event in events {
            self.record_connector_liveness(event);
        }
    }

    /// Records one connector-liveness episode (the system drain's
    /// pre-refresh hook, D8).
    pub(crate) fn record_connector_liveness(&mut self, event: &taskboard_domain::SystemEvent) {
        use taskboard_domain::SystemEvent as E;
        match event {
            E::ConnectorLost(id) => {
                self.settings.liveness.insert(id.to_string(), true);
            }
            E::ConnectorRestored(id) => {
                self.settings.liveness.insert(id.to_string(), false);
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
    pub(crate) fn drain_setting_replies(&mut self, window: &AppWindow) {
        let mut i = 0;
        let mut drained = false;
        while i < self.settings.replies.len() {
            // The settled reply's outcome, computed borrow-local and
            // routed through the one choke point below (r2 D28).
            let mut landed: Option<(SettingsOutcome, Option<SettingsError>)> = None;
            let settled = match &mut self.settings.replies[i] {
                PendingReply::Test(rx) => match rx.try_recv() {
                    Ok(outcome) => {
                        landed = Some(match &outcome {
                            Ok(()) => (SettingsOutcome::Ok, None),
                            Err(err) => (SettingsVm::outcome_of(err), Some(err.clone())),
                        });
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
                        // Both surfaces re-render from the same reply
                        // (r2 D32): rows and the board selector never
                        // go stale against each other (F18).
                        self.settings.rows = snapshot.connectors;
                        self.settings.boards = snapshot.boards;
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
                PendingReply::Scale(rx, previous) => match rx.try_recv() {
                    Ok(Ok(())) => {
                        landed = Some((SettingsOutcome::Ok, None));
                        true
                    }
                    Ok(Err(err)) => {
                        landed = Some((SettingsVm::outcome_of(&err), Some(err.clone())));
                        // The live-apply revert (D9): the theme value
                        // goes back to the previous setting.
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
                if let Some((outcome, detail)) = landed {
                    self.set_settings_outcome(window, outcome, detail);
                }
                drained = true;
            } else {
                i += 1;
            }
        }
        if drained {
            self.push_settings(window);
            self.refresh(window);
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
        self.settings.preset = update.preset;
        window
            .global::<crate::UiStatus>()
            .set_startup_keymap_findings(i32::try_from(update.findings).unwrap_or(i32::MAX));
    }

    /// Pushes the settings view-model onto the window global (the
    /// refresh pass's settings half; rows fold liveness + the bound
    /// boards' phases, D8). The row list rides the persistent
    /// settings-row handle through the positional diff (r2 D9).
    pub(crate) fn push_settings(&mut self, window: &AppWindow) {
        use crate::SettingsRowStatus as Row;
        let state = self.state.load();
        // The busy machine (r2 D34): the flag timeline is the model's;
        // the connector rows mark Save/Test/Remove/Attach ceremonies,
        // the board rows mark Detach.
        let connector_busy = matches!(
            self.settings.pending,
            Some(PendingKind::Save | PendingKind::Test | PendingKind::Remove | PendingKind::Attach)
        );
        let detach_busy = matches!(self.settings.pending, Some(PendingKind::Detach));
        let boards: Vec<crate::SettingsBoardRow> = self
            .settings
            .boards
            .iter()
            .map(|board| crate::SettingsBoardRow {
                id: board.id.to_string().into(),
                name: board.name.clone().into(),
                bound_connector: board.bound_connector.clone().unwrap_or_default().into(),
                bound: board.bound_connector.is_some(),
                selected: self.settings.selected_board == Some(board.id),
                busy: detach_busy && self.settings.busy_target == board.name,
            })
            .collect();
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
                    read_only: row.read_only,
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
        self.models.push_settings_boards(&global, &boards);
        global.set_selected_board(
            self.settings
                .selected_board
                .map_or(String::new(), |id| id.to_string())
                .into(),
        );
        global.set_busy(self.settings.pending.is_some());
        global.set_busy_target(self.settings.busy_target.clone().into());
        global.set_draft_name(self.settings.draft.name.clone().into());
        global.set_draft_server(self.settings.draft.server_url.clone().into());
        global.set_draft_username(self.settings.draft.username.clone().into());
        global.set_save_enabled(self.settings.save_enabled());
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
    }

    /// Mirrors one edited field into the draft (the buffer precedent);
    /// engages insert mode once — the D18 text-input contract (a
    /// focused field must receive printables untouched).
    pub(crate) fn settings_field_edited(&mut self, field: usize, text: &str, window: &AppWindow) {
        let target = match field {
            0 => &mut self.settings.draft.name,
            1 => &mut self.settings.draft.server_url,
            2 => &mut self.settings.draft.username,
            3 => {
                // The password mirror: write-only — it never enters the
                // pushed model (the draft never renders it back).
                self.settings.draft.password.clear();
                self.settings.draft.password.push_str(text);
                self.settings_field_insert_engage(window);
                return;
            }
            _ => return,
        };
        target.clear();
        target.push_str(text);
        self.settings_field_insert_engage(window);
    }

    fn settings_field_insert_engage(&mut self, window: &AppWindow) {
        if self.shell.mode != crate::router::Mode::Insert {
            self.shell
                .apply(&UiEffect::SetMode(crate::router::Mode::Insert), 0);
            self.sync(window);
        }
        self.push_settings(window);
    }

    /// Leaves the insert engagement (a field commit): back to Normal
    /// so the settings chain resolves again.
    pub(crate) fn settings_field_committed(&mut self, window: &AppWindow) {
        if self.shell.mode == crate::router::Mode::Insert {
            self.shell
                .apply(&UiEffect::SetMode(crate::router::Mode::Normal), 0);
            self.sync(window);
        }
    }

    /// The Save seam: mirror the final field texts (the commit
    /// callbacks carry them), then send the verify-first ceremony
    /// request. The password clears off the draft after the send —
    /// write-only (D26b).
    pub(crate) fn settings_save(
        &mut self,
        window: &AppWindow,
        name: &str,
        server: &str,
        username: &str,
        password: &str,
    ) {
        self.settings_field_committed(window);
        let draft = ConnectionDraft {
            name: name.to_owned(),
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
                settings_form::DraftError::Invalid {
                    field,
                    kind: settings_form::InvalidKind::Empty,
                } => SettingsError::Invalid {
                    field: map_form_field(field),
                    kind: InvalidKind::Empty,
                },
                settings_form::DraftError::Invalid {
                    field,
                    kind: settings_form::InvalidKind::NotKebab,
                } => SettingsError::Invalid {
                    field: map_form_field(field),
                    kind: InvalidKind::NotKebab,
                },
                settings_form::DraftError::Invalid {
                    field,
                    kind: settings_form::InvalidKind::BadUrl,
                } => SettingsError::Invalid {
                    field: map_form_field(field),
                    kind: InvalidKind::BadUrl,
                },
            };
            self.set_settings_outcome(window, SettingsOutcome::Rejected, Some(detail));
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::SaveConnection {
            profile: ConnectionProfile {
                name: draft.name.clone(),
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
            &draft.name,
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

    /// The per-row Test seam: verify the row's stored secret (DEV-3 —
    /// no retyping). A stale row index is absorbed silently.
    pub(crate) fn settings_test_row(&mut self, window: &AppWindow, row: usize) {
        let Some(entry) = self.settings.rows.get(row).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::TestConnection {
            target: ConnectionTarget {
                server_url: entry.server_url,
                username: entry.username,
            },
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Test,
            PendingReply::Test(reply_rx),
            &entry.name,
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

    /// The per-row Remove seam (a stale index is absorbed silently).
    pub(crate) fn settings_remove_row(&mut self, window: &AppWindow, row: usize) {
        let Some(entry) = self.settings.rows.get(row).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::RemoveConnection {
            name: entry.name.clone(),
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Remove,
            PendingReply::List(reply_rx),
            &entry.name,
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

    /// The board selector's seam (r2 D32): radio selection — the uuid
    /// string from the row converts once here; a stale/foreign id is
    /// absorbed silently.
    pub(crate) fn settings_board_selected(&mut self, window: &AppWindow, id: &str) {
        match uuid::Uuid::parse_str(id) {
            Ok(id) => self.settings.selected_board = Some(id),
            Err(_) => return,
        }
        self.push_settings(window);
    }

    /// The Bind seam (r2 D33): bind the selected board to this
    /// connector row. The pending gate routes nothing (the row's
    /// enabled gate is markup; the VM gate is the authority).
    pub(crate) fn settings_bind_row(&mut self, window: &AppWindow, row: usize) {
        let Some(entry) = self.settings.rows.get(row).cloned() else {
            return;
        };
        let Some(board) = self.settings.selected_board else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::AttachConnector {
            board: BoardId::from_uuid(board),
            connector: entry.name.clone(),
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Attach,
            PendingReply::List(reply_rx),
            &entry.name,
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

    /// The Detach seam (r2 D33): unbind this board row from its
    /// connector. A stale index is absorbed silently.
    pub(crate) fn settings_detach_row(&mut self, window: &AppWindow, row: usize) {
        let Some(entry) = self.settings.boards.get(row).cloned() else {
            return;
        };
        if self.settings.pending.is_some() {
            return;
        }
        let (reply, reply_rx) = oneshot::channel();
        let request = UiRequest::DetachConnector {
            board: BoardId::from_uuid(entry.id),
            connector: entry.bound_connector.clone().unwrap_or_default(),
            reply,
        };
        let sent = self.settings.send(
            &self.requests,
            request,
            PendingKind::Detach,
            PendingReply::List(reply_rx),
            &entry.name,
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
        self.settings_field_committed(window);
        let Ok(scale) = text.parse::<f64>() else {
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        };
        if self.settings.pending.is_some() || UiScale::new(scale).is_err()
        // presentation mirror; the authoritative check is the app-side mint
        {
            self.set_settings_outcome(window, SettingsOutcome::Rejected, None);
            return;
        }
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

    /// The slider gesture's preview (r2 D29): clamp into the range and
    /// live-apply onto the theme — pure visual feedback. It dispatches
    /// nothing, writes no outcome, pushes nothing; `Theme` is
    /// seam-written only, so **one request per gesture holds by
    /// proof**: no lane write exists in this arm.
    pub(crate) fn settings_scale_preview(&mut self, window: &AppWindow, value: f64) {
        if self.scale_drag.is_none() {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        window
            .global::<crate::Theme>()
            .set_base_scale(value.clamp(0.5, 3.0) as f32);
    }

    /// The slider gesture's release (r2 D29): one gate, one commit.
    /// An out-of-gate value reverts the whole preview (base-scale
    /// back to the pre-**gesture** value) with a seam-local Rejected;
    /// an in-gate value goes through the shared commit core with the
    /// pre-gesture value as the revert point, so a failed commit
    /// undoes the whole preview too.
    pub(crate) fn settings_scale_released(&mut self, window: &AppWindow, value: f64) {
        let Some(previous) = self.scale_drag.take() else {
            return;
        };
        if self.settings.pending.is_some() || !value.is_finite() || !(0.5..=3.0).contains(&value) {
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
    day_start: (chrono::NaiveTime, chrono::FixedOffset),
    kiosk: bool,
    rotation: Option<crate::view_state::KioskRotation>,
    reminder_defaults: taskboard_domain::ReminderDefaults,
    boards: Vec<requests::BoardInfo>,
    state: Arc<ArcSwap<AppState>>,
    status_timer: Rc<slint::Timer>,
    rotation_timer: Rc<slint::Timer>,
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
    let adapter = Rc::new(RefCell::new(Adapter {
        keymap,
        day_start,
        kiosk,
        reminder_defaults,
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
        reminder_target: None,
        due_buffer: String::new(),
        due_echo: String::new(),
        due_picker: None,
        picker_focus_on_mount: false,
        board_target: None,
        rule_target: None,
        rule_add_offset: None,
        timer_draft: None,
        timer_target: None,
        tables: IdTables::default(),
        models: crate::models::PushModels::default(),
        width_drag: None,
        scale_drag: None,
        settings: SettingsVm {
            rows: connectors,
            // The mount seed (r2 D32): the board-selector rows ride
            // the same hand-over — the first refresh's push lands both.
            boards,
            // The mount seed (r2 D27): the picker's initial selection
            // rides the VM — the first refresh's `push_settings` is
            // the one write path.
            preset,
            ..SettingsVm::default()
        },
        status_timer,
        status_fire: Rc::new(|| {}), // replaced below, before any fire can happen
        rotation,
        rotation_timer,
        rotation_fire: Rc::new(|| {}), // replaced below, before any fire can happen
        quit_requests: 0,
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
        global.set_help_name(status::HELP_NAME.into());
        global.set_help_server(status::HELP_SERVER.into());
        global.set_help_username(status::HELP_USERNAME.into());
        global.set_help_app_password(status::HELP_APP_PASSWORD.into());
        global.set_help_scale(status::HELP_SCALE.into());
        global.set_help_keymap(status::HELP_KEYMAP.into());
        global.set_help_rows(status::HELP_ROWS.into());
    }

    {
        let adapter = Rc::clone(&adapter);
        let weak = window.as_weak();
        window.on_intercept_key(move |text, modifiers| {
            let Some(window) = weak.upgrade() else {
                return false;
            };
            adapter.borrow_mut().intercept(&window, &text, modifiers)
        });
    }
    {
        let adapter = Rc::clone(&adapter);
        let weak = window.as_weak();
        window.on_menu_entry_invoked(move |index: i32| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            adapter.borrow_mut().invoke_root_entry(&window, index);
        });
    }
    {
        let adapter = Rc::clone(&adapter);
        let weak = window.as_weak();
        window.on_menu_submenu_entry_invoked(move |index: i32| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            adapter.borrow_mut().invoke_entry(&window, index);
        });
    }
    {
        let adapter = Rc::clone(&adapter);
        let weak = window.as_weak();
        window.on_menu_level_back(move || {
            let Some(window) = weak.upgrade() else { return };
            let mut a = adapter.borrow_mut();
            a.apply_effect(&window, UiEffect::MenuBack);
        });
    }
    {
        // The D17 width push: the window reports logical widths, the
        // unscaled Rust mapping lands in the `Responsive` global
        // (widths `<= 0` are ignored inside the mapping — the null
        // platform reports 0×0).
        let weak = window.as_weak();
        window.on_window_width_changed(move |width: f32| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let breakpoint =
                crate::models::to_slint_breakpoint(crate::responsive::breakpoint_for_width(width));
            window
                .global::<crate::Responsive>()
                .set_current_breakpoint(breakpoint);
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
    install_settings_seams(window, adapter);
}

/// The column resize drag seams (round-1 D17): press captures the
/// pre-drag base width against the addressed stack, drag clamps
/// base+delta into the live bounds and re-renders (the models push the
/// new width), release persists the full snapshot over the request
/// lane. A stale column index against an older tables pass is absorbed
/// silently (the `board_target` precedent).
fn install_stack_width_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let press = Rc::clone(adapter);
    let press_weak = window.as_weak();
    window.on_board_stack_width_press(move |column, base| {
        if press_weak.upgrade().is_none() {
            return;
        }
        let Ok(column) = usize::try_from(column) else {
            return;
        };
        let mut a = press.borrow_mut();
        let Some(stack) = a.tables.stack_ids.get(column).copied() else {
            return;
        };
        a.width_drag = Some((stack, base));
    });
    let drag = Rc::clone(adapter);
    let drag_weak = window.as_weak();
    window.on_board_stack_width_drag(move |column, delta| {
        let Some(window) = drag_weak.upgrade() else {
            return;
        };
        let Ok(column) = usize::try_from(column) else {
            return;
        };
        let mut a = drag.borrow_mut();
        let Some((stack, base)) = a.width_drag else {
            return;
        };
        // A tables pass that reordered the columns makes the index
        // stale: only proceed when it still addresses the dragged
        // stack.
        if a.tables.stack_ids.get(column) != Some(&stack) {
            return;
        }
        let Some(board) = a.view_state.board() else {
            return;
        };
        let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
        let min_rendered = f64::from(crate::widths::STACK_WIDTH_MIN_PX) * scale;
        let ceiling = a.stack_width_ceiling_excluding(&a.state.load(), &window, Some(stack));
        let ceiling_rendered = f64::from(ceiling);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let requested_px = (f64::from(base) + f64::from(delta)) as i64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let min_px = min_rendered as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let max_px = ceiling_rendered as u32;
        let rendered = crate::widths::clamp_width(requested_px, min_px, max_px);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let stored =
            u32::try_from((f64::from(rendered) / scale).round() as i64).unwrap_or(rendered);
        a.view_state.set_stack_width(board, stack, stored);
        a.refresh(&window);
    });
    let release = Rc::clone(adapter);
    let release_weak = window.as_weak();
    window.on_board_stack_width_release(move |column| {
        let Some(_window) = release_weak.upgrade() else {
            return;
        };
        let Ok(_column) = usize::try_from(column) else {
            return;
        };
        let mut a = release.borrow_mut();
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
    let armed = Rc::clone(adapter);
    let armed_weak = window.as_weak();
    window.on_board_card_drag_armed(move |column, row| {
        let Some(window) = armed_weak.upgrade() else {
            return;
        };
        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
            return;
        };
        let mut a = armed.borrow_mut();
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
    let hovered = Rc::clone(adapter);
    let hovered_weak = window.as_weak();
    window.on_board_drag_hovered(move |column| {
        let Some(window) = hovered_weak.upgrade() else {
            return;
        };
        let Ok(column) = usize::try_from(column) else {
            return;
        };
        let mut a = hovered.borrow_mut();
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
    let dropped = Rc::clone(adapter);
    let dropped_weak = window.as_weak();
    window.on_board_card_dropped(move |column| {
        let Some(window) = dropped_weak.upgrade() else {
            return;
        };
        let Ok(column) = usize::try_from(column) else {
            return;
        };
        let mut a = dropped.borrow_mut();
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
    let finished = Rc::clone(adapter);
    let finished_weak = window.as_weak();
    window.on_board_card_drag_finished(move |column, row, _completed| {
        let Some(window) = finished_weak.upgrade() else {
            return;
        };
        let _ = (column, row);
        let Ok(mut a) = finished.try_borrow_mut() else {
            return;
        };
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
    let menu = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_toolbar_menu_clicked(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut a = menu.borrow_mut();
        if a.shell.menu.open {
            // The toggle is a full close from any level (round-1 D6).
            a.apply_effect(&window, UiEffect::CloseMenu);
        } else {
            a.route(&window, ActionId::AppMenu);
        }
    });
    // The inbox chip's click routes `view.inbox` (§7.1: every click
    // has an action equivalent; chunk 10 D9).
    let inbox = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_toolbar_inbox_clicked(move || {
        let Some(window) = weak.upgrade() else { return };
        inbox.borrow_mut().route(&window, ActionId::ViewInbox);
    });
}

/// The dismissal layer's full close (round-1 D2): an outside click
/// routes `menu.dismiss` — a dismissal, not a level-up.
fn install_menu_dismiss_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_menu_dismiss_requested(move || {
        let Some(window) = weak.upgrade() else { return };
        // An outside click is a full dismissal, not a level-up
        // (round-1 D6: `CloseMenu`, the layer and the toggle share it).
        adapter
            .borrow_mut()
            .apply_effect(&window, UiEffect::CloseMenu);
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
        self.open_context_menu(window, x, y, menu::TASK_CONTEXT);
    }
}

/// A board card's right click (round-1 D13): resolve, focus, open.
fn install_board_context_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_board_task_context_requested(move |column, row, x, y| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
            return;
        };
        let mut a = adapter.borrow_mut();
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
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_stack_task_context_requested(move |row, x, y| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let Ok(row) = usize::try_from(row) else {
            return;
        };
        let mut a = adapter.borrow_mut();
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
fn install_today_row_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let click = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_today_row_clicked(move |section, row| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = click.borrow_mut();
        settle_pointer_conflict(&mut a, &window);
        focus_today_row(&mut a, &window, section, row);
    });
    let activated = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_today_row_activated(move |section, row| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = activated.borrow_mut();
        settle_pointer_conflict(&mut a, &window);
        if focus_today_row(&mut a, &window, section, row).is_some() {
            a.route(&window, ActionId::TaskEdit);
        }
    });
    // The done checkbox (r3 D6): the toggled command, the board
    // cards' seam verbatim (a stale index absorbed).
    let done = Rc::clone(adapter);
    window.on_today_row_done_toggled(move |section, row| {
        let a = done.borrow();
        let (Ok(section), Ok(row)) = (usize::try_from(section), usize::try_from(row)) else {
            return;
        };
        if let Some(task) = a.tables.today_row(section, row) {
            send_toggled_done(&a, task);
        }
    });
    let context = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_today_row_context_requested(move |section, row, x, y| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = context.borrow_mut();
        settle_pointer_conflict(&mut a, &window);
        if focus_today_row(&mut a, &window, section, row).is_some() {
            a.open_task_context_menu(&window, x, y);
        }
    });
}

/// A today row at `(section, row)`: resolves the id tables (the
/// per-section prefix sums over `today_tasks`) and applies the click's
/// focus state. Returns the addressed task, or `None` for a stale
/// index against an older tables pass.
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

/// The r2 D21 object-context seams: right-click selects first (the
/// card precedent), then opens the object's context menu at the
/// pointer. A right-clicked non-selected board-bar segment switches
/// to that board — "the menu addresses what you clicked" (the
/// recorded semantics). Stale indices are absorbed silently.
fn install_object_context_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The stack context arm, shared by the header and the body seams
    // (r3 D4) so they cannot drift: resolve the column's stack, apply
    // the selection outcomes, open the stack menu at the pointer.
    // Stale indices are absorbed silently (the tables guard).
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
    // The stack header (the regular header and the compact switcher's
    // segments share the callback).
    let header = Rc::clone(adapter);
    let header_weak = window.as_weak();
    window.on_board_header_context_requested(move |column, x, y| {
        let Some(window) = header_weak.upgrade() else {
            return;
        };
        let mut a = header.borrow_mut();
        open_stack_context(&mut a, &window, column, x, y);
    });
    // The stack card's bare surface (r3 D4): the identical arm.
    let body = Rc::clone(adapter);
    let body_weak = window.as_weak();
    window.on_board_body_context_requested(move |column, x, y| {
        let Some(window) = body_weak.upgrade() else {
            return;
        };
        let mut a = body.borrow_mut();
        open_stack_context(&mut a, &window, column, x, y);
    });
    // The board-bar segment.
    let segment = Rc::clone(adapter);
    let segment_weak = window.as_weak();
    window.on_board_segment_context_requested(move |index, x, y| {
        let Some(window) = segment_weak.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut a = segment.borrow_mut();
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
    let rename_mirror = Rc::clone(adapter);
    window.on_board_stack_rename_edited(move |_column, text| {
        rename_mirror
            .borrow_mut()
            .view_state
            .set_buffer(text.to_string());
    });
    let rename_commit = Rc::clone(adapter);
    let rename_commit_weak = window.as_weak();
    window.on_board_stack_rename_committed(move |_column, text| {
        let Some(window) = rename_commit_weak.upgrade() else {
            return;
        };
        commit_text(&mut rename_commit.borrow_mut(), &window, &text);
    });
    // The board create/rename fields: the window-level slot's
    // mirror/commit seams (the create-row pattern).
    let board_mirror = Rc::clone(adapter);
    window.on_board_create_field_edited(move |text| {
        board_mirror
            .borrow_mut()
            .view_state
            .set_buffer(text.to_string());
    });
    let board_commit = Rc::clone(adapter);
    let board_commit_weak = window.as_weak();
    window.on_board_create_field_committed(move |text| {
        let Some(window) = board_commit_weak.upgrade() else {
            return;
        };
        commit_text(&mut board_commit.borrow_mut(), &window, &text);
    });
    let board_rename_mirror = Rc::clone(adapter);
    window.on_board_rename_field_edited(move |text| {
        board_rename_mirror
            .borrow_mut()
            .view_state
            .set_buffer(text.to_string());
    });
    let board_rename_commit = Rc::clone(adapter);
    let board_rename_commit_weak = window.as_weak();
    window.on_board_rename_field_committed(move |text| {
        let Some(window) = board_rename_commit_weak.upgrade() else {
            return;
        };
        commit_text(&mut board_rename_commit.borrow_mut(), &window, &text);
    });
}

/// The settings back affordance (round-1 D6.6): the dedicated seam
/// routing the same close path the Escape binding takes — an adapter
/// method, not a registry action (closing is not a routable intent).
fn install_settings_back_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_settings_back_clicked(move || {
        let Some(window) = weak.upgrade() else { return };
        Adapter::dismiss_tooltip(&window);
        let mut a = adapter.borrow_mut();
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
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_task_back_clicked(move || {
        let Some(window) = weak.upgrade() else { return };
        adapter.borrow_mut().route(&window, ActionId::NavFocusBoard);
    });
}

/// A board-bar segment click (design §3.8): resolve the segment's
/// board against the tables pass that pushed the bar, set the one-shot
/// target, and route `board.select` — every click affordance has an
/// action equivalent (§7.1). A stale index against an older pass is
/// absorbed silently.
fn install_board_bar_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_board_bar_segment_activated(move |index| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut a = adapter.borrow_mut();
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
    let route = Rc::clone(adapter);
    let route_weak = window.as_weak();
    window.on_board_new_stack_clicked(move || {
        let Some(window) = route_weak.upgrade() else {
            return;
        };
        let mut a = route.borrow_mut();
        settle_pointer_conflict(&mut a, &window);
        a.route(&window, ActionId::StackCreate);
    });
    // The empty-board preset CTA (r3 D5): the direct command dispatch
    // — no text flow, the flow machine untouched.
    let preset = Rc::clone(adapter);
    let preset_weak = window.as_weak();
    window.on_board_default_preset_clicked(move || {
        let Some(window) = preset_weak.upgrade() else {
            return;
        };
        let mut a = preset.borrow_mut();
        let state = a.state.load_full();
        let outcomes = a
            .view_state
            .apply_board_preset(&state, &crate::board_presets::DEFAULT_KANBAN);
        a.apply_outcomes(&window, &outcomes);
    });
    let mirror = Rc::clone(adapter);
    window.on_board_stack_create_edited(move |text| {
        mirror.borrow_mut().view_state.set_buffer(text.to_string());
    });
    let commit = Rc::clone(adapter);
    let commit_weak = window.as_weak();
    window.on_board_stack_create_committed(move |text| {
        let Some(window) = commit_weak.upgrade() else {
            return;
        };
        commit_text(&mut commit.borrow_mut(), &window, &text);
    });
}

/// The stack-detail done checkbox (r3 D6): the toggled command, the
/// board cards' seam verbatim (a stale index absorbed).
fn install_stack_done_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let done = Rc::clone(adapter);
    window.on_stack_task_done_toggled(move |row| {
        let a = done.borrow();
        let Ok(row) = usize::try_from(row) else {
            return;
        };
        if let Some(task) = a.tables.stack_task(row) {
            send_toggled_done(&a, task);
        }
    });
}

/// A `Fn(&mut Adapter, &AppWindow, Args)` wrapper shape is over
/// generics' worth of closure types; the per-seam helpers below stay
/// explicit instead.
fn install_board_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // A card click: focus the (column, row) card (selection state
    // only, §7.1 mouse discipline); activation routes `task.edit`
    // after it.
    let (card, activated) = (Rc::clone(adapter), Rc::clone(adapter));
    let (click_weak, act_weak) = (window.as_weak(), window.as_weak());
    window.on_board_task_clicked(move |column, row| {
        let Some(window) = click_weak.upgrade() else {
            return;
        };
        let mut a = card.borrow_mut();
        focus_board_card(&mut a, &window, column, row);
    });
    window.on_board_task_activated(move |column, row| {
        let Some(window) = act_weak.upgrade() else {
            return;
        };
        let mut a = activated.borrow_mut();
        if focus_board_card(&mut a, &window, column, row).is_some() {
            a.route(&window, ActionId::TaskEdit);
        }
    });

    // Header and switcher activation are the same selection move (D8:
    // one mechanism per breakpoint; D13: no stack management in UI).
    let (header, switcher) = (Rc::clone(adapter), Rc::clone(adapter));
    let (head_weak, switch_weak) = (window.as_weak(), window.as_weak());
    window.on_board_header_activated(move |column| {
        let Some(window) = head_weak.upgrade() else {
            return;
        };
        let mut a = header.borrow_mut();
        select_column(&mut a, &window, column);
    });
    window.on_board_switcher_activated(move |column| {
        let Some(window) = switch_weak.upgrade() else {
            return;
        };
        let mut a = switcher.borrow_mut();
        select_column(&mut a, &window, column);
    });

    // The Add Task ghost (r2 D8): select the addressed stack, then
    // route task.create — the keyboard's exact action, so click ≡ key
    // by construction (one action, one target-resolution rule).
    let ghost = Rc::clone(adapter);
    let ghost_weak = window.as_weak();
    window.on_board_add_task_activated(move |column| {
        let Some(window) = ghost_weak.upgrade() else {
            return;
        };
        let mut a = ghost.borrow_mut();
        select_column(&mut a, &window, column);
        settle_pointer_conflict(&mut a, &window);
        a.route(&window, ActionId::TaskCreate);
    });

    // The done checkbox (r3 D6): resolve (column, row) against the id
    // tables (a stale index is absorbed, the drag-arm precedent) and
    // raw-send the toggled `SetTaskDone` — the notification Done
    // button's precedent (the registry's complete/reopen act on the
    // focused task; the checkbox addresses an arbitrary card). No
    // local checked state exists anywhere: the refresh push is the
    // single writer of `TaskCardModel.done`.
    let done = Rc::clone(adapter);
    let done_weak = window.as_weak();
    window.on_board_task_done_toggled(move |column, row| {
        let Some(window) = done_weak.upgrade() else {
            return;
        };
        let mut a = done.borrow_mut();
        settle_pointer_conflict(&mut a, &window);
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
    let mirror = Rc::clone(adapter);
    window.on_board_create_edited(move |column, text| {
        let _ = column;
        mirror.borrow_mut().view_state.set_buffer(text.to_string());
    });
    let commit = Rc::clone(adapter);
    let commit_weak = window.as_weak();
    window.on_board_create_committed(move |column, text| {
        let Some(window) = commit_weak.upgrade() else {
            return;
        };
        let _ = column;
        commit_text(&mut commit.borrow_mut(), &window, &text);
    });
}

/// The L2 pointer-seam settle (round-3 R1/R2, plan §3.3): every seam
/// that bypasses `interpret` settles a live flow through the same
/// per-kind rule before its own semantics; idempotent on
/// `Flow::None` (the double-settle guard).
fn settle_pointer_conflict(adapter: &mut Adapter, window: &AppWindow) {
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
    let (card, activated) = (Rc::clone(adapter), Rc::clone(adapter));
    let (click_weak, act_weak) = (window.as_weak(), window.as_weak());
    window.on_stack_task_clicked(move |row| {
        let Some(window) = click_weak.upgrade() else {
            return;
        };
        let mut a = card.borrow_mut();
        activate_stack_row(&mut a, &window, row);
    });
    window.on_stack_task_activated(move |row| {
        let Some(window) = act_weak.upgrade() else {
            return;
        };
        let mut a = activated.borrow_mut();
        activate_stack_row(&mut a, &window, row);
        a.route(&window, ActionId::TaskEdit);
    });

    let mirror = Rc::clone(adapter);
    window.on_stack_create_edited(move |text| {
        mirror.borrow_mut().view_state.set_buffer(text.to_string());
    });

    // The search palette (#348): per-keystroke re-rank, outside-click
    // close, and row activation sharing the keyboard Activate path.
    let palette = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_palette_query_edited(move |text| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        palette.borrow_mut().apply_palette_query(&window, &text);
    });
    // Quick capture (chunk 10 D8): the buffer mirror, the Return
    // commit (keeps the flow), and the scrim's discard-and-close
    // (routed through the action layer's ModeNormal semantics).
    let capture = Rc::clone(adapter);
    window.on_capture_text_edited(move |text| {
        capture.borrow_mut().view_state.set_buffer(text.to_string());
    });
    let capture = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_capture_accepted(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = capture.borrow_mut();
        let outcomes = a.view_state.commit_capture();
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
    });
    let capture = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_capture_dismiss_requested(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = capture.borrow_mut();
        let outcomes = a.view_state.close_capture();
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
    });
    let palette = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_palette_dismiss_requested(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = palette.borrow_mut();
        let outcomes = a.view_state.close_palette();
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
    });
    let palette = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_palette_row_activated(move |index| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut a = palette.borrow_mut();
        let target = a
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
    let commit = Rc::clone(adapter);
    let commit_weak = window.as_weak();
    window.on_stack_create_committed(move |text| {
        let Some(window) = commit_weak.upgrade() else {
            return;
        };
        commit_text(&mut commit.borrow_mut(), &window, &text);
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
fn install_task_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let mirror = Rc::clone(adapter);
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
    let focus_lost = Rc::clone(adapter);
    let focus_lost_weak = window.as_weak();
    window.on_text_flow_focus_lost(move || {
        let Some(window) = focus_lost_weak.upgrade() else {
            return;
        };
        let Ok(mut a) = focus_lost.try_borrow_mut() else {
            return;
        };
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
    });
    let title_click = Rc::clone(adapter);
    let title_click_weak = window.as_weak();
    window.on_task_title_edit_activated(move || {
        let Some(window) = title_click_weak.upgrade() else {
            return;
        };
        title_click
            .borrow_mut()
            .route(&window, ActionId::TaskTitleEdit);
    });
    window.on_task_title_edited(move |text| {
        mirror.borrow_mut().view_state.set_buffer(text.to_string());
    });
    let commit = Rc::clone(adapter);
    let commit_weak = window.as_weak();
    window.on_task_title_committed(move |text| {
        let Some(window) = commit_weak.upgrade() else {
            return;
        };
        commit_text(&mut commit.borrow_mut(), &window, &text);
    });

    let remove = Rc::clone(adapter);
    let remove_weak = window.as_weak();
    window.on_task_rule_remove_clicked(move |index| {
        let Some(window) = remove_weak.upgrade() else {
            return;
        };
        let mut a = remove.borrow_mut();
        let Some(rule) = a
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

    let preset = Rc::clone(adapter);
    let preset_weak = window.as_weak();
    window.on_rule_add_preset_clicked(move |index| {
        let Some(window) = preset_weak.upgrade() else {
            return;
        };
        let mut a = preset.borrow_mut();
        let Some(dur) = crate::presentation::RULE_PRESETS
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale preset index
        };
        a.rule_add_offset =
            Some(ReminderOffset::before(-dur).expect("preset magnitudes negate into the domain"));
        a.route(&window, ActionId::ReminderRuleAdd);
    });

    let custom = Rc::clone(adapter);
    let custom_weak = window.as_weak();
    window.on_rule_add_custom_accepted(move |text| {
        let Some(window) = custom_weak.upgrade() else {
            return false;
        };
        let mut a = custom.borrow_mut();
        // A parse miss dispatches nothing (absorbed — D12): the UI
        // owns no error chrome; the CLI owns typed failures. The miss
        // rides the seam's return (r2 D24): the composition clears its
        // own buffer, so the field never looks stuck.
        let Some(offset) = ReminderOffset::parse_text_form(&text) else {
            return false;
        };
        a.rule_add_offset = Some(offset);
        a.route(&window, ActionId::ReminderRuleAdd);
        true
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
fn install_due_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let open = Rc::clone(adapter);
    let open_weak = window.as_weak();
    window.on_task_due_edit_opened(move || {
        let Some(window) = open_weak.upgrade() else {
            return;
        };
        let mut a = open.borrow_mut();
        begin_date_edit(&mut a, &window, crate::view_state::DateField::Due);
    });

    let open_sched = Rc::clone(adapter);
    let open_sched_weak = window.as_weak();
    window.on_task_scheduled_edit_opened(move || {
        let Some(window) = open_sched_weak.upgrade() else {
            return;
        };
        let mut a = open_sched.borrow_mut();
        begin_date_edit(&mut a, &window, crate::view_state::DateField::Scheduled);
    });

    let edited = Rc::clone(adapter);
    let edited_weak = window.as_weak();
    window.on_task_date_edited(move |text| {
        let Some(window) = edited_weak.upgrade() else {
            return;
        };
        let mut a = edited.borrow_mut();
        a.due_buffer = text.to_string();
        a.due_echo = crate::due_edit::echo(&a.due_buffer, chrono::Utc::now());
        // Live echo without a full refresh: mutate the pushed struct's
        // echo in place (the grid/caption are untouched).
        let mut model = window.get_task_model();
        model.due_echo = SharedString::from(a.due_echo.clone());
        model.scheduled_echo = SharedString::from(a.due_echo.clone());
        window.set_task_model(model);
    });

    let accepted = Rc::clone(adapter);
    let accepted_weak = window.as_weak();
    window.on_task_date_accepted(move |text| {
        let Some(window) = accepted_weak.upgrade() else {
            return;
        };
        let mut a = accepted.borrow_mut();
        match crate::due_edit::parse(&text, chrono::Utc::now()) {
            // A refusal keeps the buffer and dispatches nothing.
            Err(crate::due_edit::DueParseError) => {}
            parsed => commit_date(&mut a, &window, parsed.expect("non-error parse")),
        }
    });

    let toggle = Rc::clone(adapter);
    let toggle_weak = window.as_weak();
    window.on_task_date_picker_toggled(move || {
        let Some(window) = toggle_weak.upgrade() else {
            return;
        };
        let mut a = toggle.borrow_mut();
        if a.due_picker.take().is_some() {
            a.refresh(&window);
        } else {
            a.open_due_picker(&window, false);
        }
    });

    let key = Rc::clone(adapter);
    let key_weak = window.as_weak();
    window.on_task_date_picker_key(move |code| {
        let Some(window) = key_weak.upgrade() else {
            return;
        };
        let mut a = key.borrow_mut();
        let Some(picker) = a.due_picker.as_mut() else {
            return; // a stale stroke against an already-closed picker
        };
        match code {
            0 => picker.move_days(-1),
            1 => picker.move_days(1),
            2 => picker.move_days(-7),
            3 => picker.move_days(7),
            4 => picker.move_months(-1),
            5 => picker.move_months(1),
            6 => picker.month_edge(true),
            7 => picker.month_edge(false),
            // Enter: the cursor's date at the picker's time through
            // the one funnel.
            8 => {
                let instant = picker.commit_instant();
                commit_date(&mut a, &window, Some(instant));
                return;
            }
            // The time steppers (round-3 D9): ±15 min, wrapping —
            // the date follows the wrap.
            9 => picker.step_time(-1),
            10 => picker.step_time(1),
            _ => return,
        }
        a.refresh(&window);
    });

    let day = Rc::clone(adapter);
    let day_weak = window.as_weak();
    window.on_task_date_day_clicked(move |cell| {
        let Some(window) = day_weak.upgrade() else {
            return;
        };
        let mut a = day.borrow_mut();
        let Some(picker) = a.due_picker.as_ref() else {
            return;
        };
        let Some(date) = picker
            .grid()
            .get(usize::try_from(cell).unwrap_or(usize::MAX))
            .and_then(|c| c.date)
        else {
            return; // a blank or a stale index
        };
        a.due_picker
            .as_mut()
            .expect("checked above")
            .set_cursor(date);
        // The click carries the picker's time (round-3 D9): cursor
        // date + time-of-day through the one funnel.
        let instant = a
            .due_picker
            .as_ref()
            .expect("checked above")
            .commit_instant();
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
    a.due_buffer = crate::due_edit::format_entry(value);
    a.due_echo = crate::due_edit::echo(&a.due_buffer, chrono::Utc::now());
    a.apply_outcomes(window, &outcomes);
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
        self.picker_focus_on_mount = focus_grid;
        self.due_picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Utc::now().date_naive(),
            anchor,
        ));
        self.refresh(window);
    }
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
    a.due_buffer.clear();
    a.due_echo.clear();
    a.due_picker = None;
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
    let row = Rc::clone(adapter);
    let row_weak = window.as_weak();
    window.on_banner_row_activated(move |index| {
        let Some(window) = row_weak.upgrade() else {
            return;
        };
        let mut a = row.borrow_mut();
        let Some(banner_row) = banner_row(&a, index) else {
            return;
        };
        a.view_state.set_focused(Some(banner_row.task));
        a.reminder_target = Some(banner_row.target);
        a.refresh(&window);
    });

    let snooze = Rc::clone(adapter);
    let snooze_weak = window.as_weak();
    window.on_banner_snooze_clicked(move |index| {
        let Some(window) = snooze_weak.upgrade() else {
            return;
        };
        let mut a = snooze.borrow_mut();
        let Some(banner_row) = banner_row(&a, index) else {
            return;
        };
        a.reminder_target = Some(banner_row.target);
        a.route(&window, ActionId::ReminderSnooze);
    });

    let mute = Rc::clone(adapter);
    let mute_weak = window.as_weak();
    window.on_banner_mute_clicked(move |index| {
        let Some(window) = mute_weak.upgrade() else {
            return;
        };
        let mut a = mute.borrow_mut();
        let Some(banner_row) = banner_row(&a, index) else {
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

impl Adapter {
    /// The timers form's insert engagement (DR-3/3): the permanently
    /// mounted fields must never feed the Normal-mode chain (Space is
    /// task.complete there) — the settings fields' engage precedent.
    fn timers_form_engage(&mut self, window: &AppWindow) {
        if self.shell.mode != Mode::Insert {
            self.shell.apply(&UiEffect::SetMode(Mode::Insert), 0);
            self.sync(window);
        }
    }

    /// Leaves the engagement (a committed form): back to Normal so
    /// the chain resolves again (the settings commit precedent).
    fn timers_form_committed(&mut self, window: &AppWindow) {
        if self.shell.mode == Mode::Insert {
            self.shell.apply(&UiEffect::SetMode(Mode::Normal), 0);
            self.sync(window);
        }
    }
}

/// The timers surface's seams (DR-3/3, D-I8): the form's commit
/// parses the grammar at the seam (the rule-add absorbed-miss
/// convention) and routes; the section's cancel resolves the
/// `IdTables.timers` index; the chip click focuses the today view
/// (the existing `ViewToday` action — no new `UiEffect`).
fn install_timer_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let create = Rc::clone(adapter);
    let create_weak = window.as_weak();
    window.on_timers_create_accepted(move |label, when_text| {
        let Some(window) = create_weak.upgrade() else {
            return false;
        };
        let mut a = create.borrow_mut();
        // A whitespace-empty label dispatches nothing (D-P14): the raw
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
        a.timer_draft = Some(crate::router::TimerDraft {
            label: label.into(),
            when,
        });
        a.route(&window, ActionId::TimerCreate);
        a.timers_form_committed(&window);
        true
    });

    let edited = Rc::clone(adapter);
    let edited_weak = window.as_weak();
    window.on_timers_form_edited(move || {
        let Some(window) = edited_weak.upgrade() else {
            return;
        };
        edited.borrow_mut().timers_form_engage(&window);
    });

    let cancel = Rc::clone(adapter);
    let cancel_weak = window.as_weak();
    window.on_timers_cancel_clicked(move |index| {
        let Some(window) = cancel_weak.upgrade() else {
            return;
        };
        let mut a = cancel.borrow_mut();
        // A stale index degrades to a no-op (the notification_row
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
    let chip = Rc::clone(adapter);
    let chip_weak = window.as_weak();
    window.on_timers_chip_clicked(move || {
        let Some(window) = chip_weak.upgrade() else {
            return;
        };
        chip.borrow_mut().route(&window, ActionId::ViewToday);
    });
}

/// The notification surface's seams (R3 chunk C, design D-10): each
/// button dispatches exactly one engine command and nothing else (the
/// thin-client rule). No Start Task button — deferred to the
/// time-tracking subsystem (issue #210), which is charged with adding
/// it and carrying the implicit-dismissal semantic.
fn install_notification_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // Dismiss: the synced dismissal fact (the universal surface's one
    // write command).
    let dismiss = Rc::clone(adapter);
    window.on_notification_dismiss_clicked(move |index| {
        let a = dismiss.borrow();
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
    let done = Rc::clone(adapter);
    window.on_notification_done_clicked(move |index| {
        let a = done.borrow();
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
    let snooze = Rc::clone(adapter);
    let snooze_weak = window.as_weak();
    window.on_notification_snooze_clicked(move |index| {
        let Some(window) = snooze_weak.upgrade() else {
            return;
        };
        let mut a = snooze.borrow_mut();
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.reminder_target = Some(notification_target(&row));
        a.route(&window, ActionId::ReminderSnooze);
    });

    let mute = Rc::clone(adapter);
    let mute_weak = window.as_weak();
    window.on_notification_mute_clicked(move |index| {
        let Some(window) = mute_weak.upgrade() else {
            return;
        };
        let mut a = mute.borrow_mut();
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
    let cancel = Rc::clone(adapter);
    let cancel_weak = window.as_weak();
    window.on_notification_cancel_clicked(move |index| {
        let Some(window) = cancel_weak.upgrade() else {
            return;
        };
        let mut a = cancel.borrow_mut();
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        let taskboard_domain::ReminderRef::Adhoc(alarm) = row.reminder else {
            return;
        };
        a.timer_target = Some(alarm);
        a.route(&window, ActionId::TimerCancel);
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
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_field_edited(move |field, text| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(field) = usize::try_from(field) else {
                return;
            };
            adapter
                .borrow_mut()
                .settings_field_edited(field, &text, &window);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_save_clicked(move |name, server, username, password| {
            let Some(window) = weak.upgrade() else { return };
            adapter
                .borrow_mut()
                .settings_save(&window, &name, &server, &username, &password);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_test_clicked(move |row| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            adapter.borrow_mut().settings_test_row(&window, row);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_remove_clicked(move |row| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            adapter.borrow_mut().settings_remove_row(&window, row);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_board_selected(move |id| {
            let Some(window) = weak.upgrade() else { return };
            adapter.borrow_mut().settings_board_selected(&window, &id);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_bind_clicked(move |row| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            adapter.borrow_mut().settings_bind_row(&window, row);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_detach_clicked(move |row| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            adapter.borrow_mut().settings_detach_row(&window, row);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_preset_picked(move |index| {
            let Some(window) = weak.upgrade() else { return };
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            adapter.borrow_mut().settings_preset_picked(&window, index);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_field_committed(move || {
            let Some(window) = weak.upgrade() else { return };
            adapter.borrow_mut().settings_field_committed(&window);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_scale_committed(move |text| {
            let Some(window) = weak.upgrade() else { return };
            adapter
                .borrow_mut()
                .settings_scale_committed(&window, &text);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_scale_pressed(move || {
            let Some(window) = weak.upgrade() else { return };
            adapter.borrow_mut().settings_scale_pressed(&window);
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_scale_preview(move |value: f32| {
            let Some(window) = weak.upgrade() else { return };
            adapter
                .borrow_mut()
                .settings_scale_preview(&window, f64::from(value));
        });
    }
    {
        let adapter = Rc::clone(adapter);
        let weak = window.as_weak();
        window.on_settings_scale_released(move |value: f32| {
            let Some(window) = weak.upgrade() else { return };
            adapter
                .borrow_mut()
                .settings_scale_released(&window, f64::from(value));
        });
    }
}

/// The chunk-3 focus seam surface (D16/D17): sets the view context /
/// focused entities and resets the pending accumulation (a context
/// change invalidates it — D12 rule 1). The writes land in the
/// view-state machine — the shell's seam fields are derived from it
/// on every route (D6); the component tests use this seam now.
impl crate::UiHandle {
    pub fn set_view_context(&self, view: Option<ContextId>) {
        let mut adapter = self.adapter.borrow_mut();
        if let Some(view) = view {
            let mapped = match view {
                ContextId::Today => crate::view_state::ViewId::Today,
                ContextId::Board => crate::view_state::ViewId::Board,
                ContextId::StackDetail => crate::view_state::ViewId::StackDetail,
                ContextId::TaskDetail => crate::view_state::ViewId::TaskDetail,
                ContextId::Settings => crate::view_state::ViewId::Settings,
                ContextId::Global
                | ContextId::Menu
                | ContextId::Palette
                | ContextId::ViNormal
                | ContextId::ViInsert => {
                    return;
                }
            };
            adapter.view_state.set_view(mapped);
        }
        adapter.resolver.reset();
    }

    pub fn set_focused_entities(
        &self,
        focused_task: Option<TaskId>,
        _focused_stack: Option<StackId>,
    ) {
        let mut adapter = self.adapter.borrow_mut();
        adapter.view_state.set_focused(focused_task);
    }
}
