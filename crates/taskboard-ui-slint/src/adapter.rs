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

use taskboard_domain::{AppState, BoardId, StackId, StateCommand, TaskId};

use crate::actions::{ActionId, ContextId, MenuGroup};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::models::IdTables;
use crate::presentation::ReminderTarget;
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{ActionRouter, DispatchOutcome, Mode, UiEffect};
use crate::shell::KeymapShell;
use crate::view_state::{Flow, FlowOutcome, ViewState};
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

/// The adapter state: the validated keymap (applied at UI start, one
/// way), the shell, the resolver, the router, the view-state machine
/// (D3), the lock-free state read (D1), and the command channel out.
pub(crate) struct Adapter {
    keymap: Keymap,
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
    /// The id tables of the last refresh pass (D2): index → id.
    tables: IdTables,
    /// The exit-effect counter (the §6 component-test seam: journeys
    /// assert the quit request without killing the platform).
    pub(crate) quit_requests: usize,
}

impl Adapter {
    /// Pushes the shell-derived state one-way onto the window
    /// globals (D18); nothing renders it in chunk 2 — chunk 3's
    /// status bar reads these.
    fn sync(&self, window: &AppWindow) {
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

    /// One action through the router (design §5 pipeline's last hop).
    fn route(&mut self, window: &AppWindow, id: ActionId) {
        let mut ctx = self.shell.dispatch_ctx();
        ctx.reminder_target = self.reminder_target;
        self.reminder_target = None;
        ctx.board_target = self.board_target;
        self.board_target = None;
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

    /// The D1 refresh: reconcile, then one full re-derive of the
    /// display models from the current snapshot (lock-free load) and
    /// a fresh id tables pass. Deterministic, timer-free; the repaint
    /// timer and the tests call this.
    pub(crate) fn refresh(&mut self, window: &AppWindow) {
        self.sync_view_context();
        let state = self.state.load();
        self.view_state.reconcile(&state);
        let chain = self.shell.chain();
        let now = chrono::Utc::now();
        self.tables =
            crate::models::push(window, &state, &self.view_state, &self.keymap, &chain, now);
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
        let target = self.view_state.keyboard_resize_target(state);
        let others: u64 = stack_ids
            .iter()
            .filter(|stack| Some(**stack) != target)
            .map(|stack| {
                u64::from(
                    board
                        .and_then(|board| self.view_state.stack_width(board, *stack))
                        .unwrap_or(STACK_WIDTH_MIN_PX),
                )
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

    /// The D9 interpretation table: one `match (flow, effect)` whose
    /// arms route to `view_state` methods named by what they do.
    fn interpret(&mut self, window: &AppWindow, effect: UiEffect) {
        let compact = Self::is_compact(window);
        let state = self.state.load();
        let flow = self.view_state.flow().clone();
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
        if leaving {
            match flow {
                Flow::Create { .. } | Flow::CreateStack | Flow::EditTitle { .. } => {
                    let outcomes = self.view_state.commit_edit(&state);
                    self.apply_outcomes(window, &outcomes);
                }
                Flow::Move { .. } => {
                    let outcomes = self.view_state.cancel_move();
                    self.apply_outcomes(window, &outcomes);
                }
                Flow::None => {}
            }
        }

        let outcomes = match (&flow, effect) {
            // -- the move flow (D8/D9) --
            (Flow::Move { .. }, UiEffect::FocusNext) => {
                self.view_state.cycle_move_target(&state, 1)
            }
            (Flow::Move { .. }, UiEffect::FocusPrev) => {
                self.view_state.cycle_move_target(&state, -1)
            }
            (Flow::Move { .. }, UiEffect::Activate) => self.view_state.confirm_move(&state),
            (Flow::Move { .. }, UiEffect::RequestMove) => self.view_state.cancel_move(),
            // -- the edit flows (D5/D9): SetMode(Normal) discards,
            //    Activate commits, leaving commits first (above) --
            (Flow::Create { .. } | Flow::EditTitle { .. }, UiEffect::SetMode(Mode::Normal)) => {
                self.view_state.discard_edit()
            }
            (Flow::Create { .. } | Flow::EditTitle { .. }, UiEffect::Activate) => {
                self.view_state.commit_edit(&state)
            }
            // -- no flow: the intents begin things --
            (Flow::None, UiEffect::Activate) => self.view_state.activate_focused(&state),
            (Flow::None, UiEffect::RequestMove) => self.view_state.start_move(&state),
            (Flow::None, UiEffect::BeginCreate) => {
                match self.view_state.create_target_stack(&state) {
                    Some(stack) => self.view_state.begin_create(stack),
                    None => Vec::new(),
                }
            }
            (Flow::None, UiEffect::BeginCreateStack) => {
                // The refusal (a never-booted session offers nothing to
                // create a stack on) lives in the view-state method —
                // the one home for the board-existence refusal.
                self.view_state.begin_create_stack(&state)
            }
            // vi's `i`/`a` open the view's editing flow rather than
            // flipping mode into the void (D5).
            (Flow::None | Flow::Move { .. }, UiEffect::SetMode(Mode::Insert)) => {
                match self.view_state.view() {
                    crate::view_state::ViewId::TaskDetail => self.view_state.edit_subject(&state),
                    _ => match self.view_state.create_target_stack(&state) {
                        Some(stack) => self.view_state.begin_create(stack),
                        None => Vec::new(),
                    },
                }
            }
            // -- the defaults: focus walk and view navigation --
            (_, UiEffect::FocusNext) => self.view_state.advance_focus(&state, compact),
            (_, UiEffect::FocusPrev) => self.view_state.retreat_focus(&state, compact),
            (_, UiEffect::NextView) => self.view_state.next_view(&state),
            (_, UiEffect::PrevView) => self.view_state.prev_view(&state),
            (_, UiEffect::FocusBoard) => self.view_state.focus_board(),
            (_, UiEffect::BoardNext) => self.view_state.next_board(&state),
            (_, UiEffect::BoardPrevious) => self.view_state.previous_board(&state),
            (_, UiEffect::BoardSelect(board)) => self.view_state.select_board(&state, board),
            (_, UiEffect::StackWider) => {
                let ceiling = self.stack_width_ceiling(&state, window);
                self.view_state.resize_focused_stack(&state, 1, ceiling)
            }
            (_, UiEffect::StackNarrower) => {
                let ceiling = self.stack_width_ceiling(&state, window);
                self.view_state.resize_focused_stack(&state, -1, ceiling)
            }
            // A mode flip with no view meaning stays plain shell
            // state (D10) — e.g. Escape out of an empty insert.
            (_, UiEffect::SetMode(mode)) => {
                self.shell.apply(&UiEffect::SetMode(mode), 0);
                Vec::new()
            }
            (_, _) => Vec::new(),
        };
        self.apply_outcomes(window, &outcomes);
    }

    fn apply_effect(&mut self, window: &AppWindow, effect: UiEffect) {
        match effect {
            UiEffect::Exit => {
                self.quit_requests += 1;
                let _ = crate::quit();
            }
            UiEffect::OpenMenu => {
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
                // Close first, then one re-dispatch — bounded (D16);
                // the close itself lives in `invoke_entry` so the mouse
                // path shares it (round-1 D3).
                if let Some(index) = self.shell.selected_entry() {
                    self.invoke_entry(window, index);
                } else {
                    self.apply_close_menu(window);
                }
            }
            UiEffect::CloseMenu => self.apply_close_menu(window),
            UiEffect::MenuBack => {
                let was_submenu = self.shell.menu.active_group.is_some();
                self.shell.apply(&effect, self.root_entries.len());
                if was_submenu {
                    // Restored to the root page: the active page is the
                    // root page again.
                    self.menu_entries = self.root_entries.clone();
                    self.sync_menu_models(window);
                    self.sync(window);
                }
            }
            UiEffect::MenuNext | UiEffect::MenuPrevious => {
                self.shell.apply(&effect, self.menu_entries.len());
            }
            UiEffect::SetMode(_)
            | UiEffect::FocusNext
            | UiEffect::FocusPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::BeginCreate
            | UiEffect::BeginCreateStack
            | UiEffect::Activate
            | UiEffect::RequestMove
            | UiEffect::StackWider
            | UiEffect::StackNarrower
            | UiEffect::BoardNext
            | UiEffect::BoardPrevious
            | UiEffect::BoardSelect(_) => self.interpret(window, effect),
        }
    }

    /// The full close shared by every path that means "close the
    /// menu": the shell state and both page models clear (round-1 D6).
    fn apply_close_menu(&mut self, window: &AppWindow) {
        self.shell.apply(&UiEffect::CloseMenu, 0);
        self.menu_entries.clear();
        self.root_entries.clear();
        self.menu_chain.clear();
        self.menu_anchor = MenuAnchor::Hamburger;
        self.sync_menu_models(window);
        // The direct callers (toggle, dismissal layer, mouse invoke)
        // bypass `route`'s trailing sync.
        self.sync(window);
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

/// Installs the interception callbacks on a fresh window and returns
/// the shared adapter (also held by [`crate::UiHandle`] for the
/// chunk-3 focus seam).
pub(crate) fn install(
    window: &AppWindow,
    keymap: Keymap,
    startup: StartupWarnings,
    commands: mpsc::Sender<StateCommand>,
    requests: mpsc::Sender<crate::requests::UiRequest>,
    stack_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    state: Arc<ArcSwap<AppState>>,
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
    let adapter = Rc::new(RefCell::new(Adapter {
        keymap,
        shell: KeymapShell::default(),
        resolver: KeymapResolver::new(),
        view_state: {
            let mut vs = ViewState::new();
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
        board_target: None,
        tables: IdTables::default(),
        quit_requests: 0,
    }));
    // Startup warnings are pure input (Q15): pushed once at mount.
    window
        .global::<UiStatus>()
        .set_startup_keymap_findings(i32::try_from(startup.keymap_findings).unwrap_or(i32::MAX));

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
    install_task_seam(window, adapter);
    install_task_back_seam(window, adapter);
    install_board_context_seam(window, adapter);
    install_stack_context_seam(window, adapter);
    install_banner_seams(window, adapter);
}

/// The toolbar's menu button is a stateful toggle (round-1 D1): open
/// when closed (routing `app.menu`), a full close when open (routing
/// `menu.dismiss`) — the F1 fix, since the toggle means `OpenMenu` can
/// only ever fire while the menu is closed, so the pre-menu chain the
/// projection reads is the real one on every open.
fn install_toolbar_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let adapter = Rc::clone(adapter);
    let weak = window.as_weak();
    window.on_toolbar_menu_clicked(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut a = adapter.borrow_mut();
        if a.shell.menu.open {
            // The toggle is a full close from any level (round-1 D6).
            a.apply_effect(&window, UiEffect::CloseMenu);
        } else {
            a.route(&window, ActionId::AppMenu);
        }
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

/// The right-click context menu opener (round-1 D13): focus the
/// addressed card (right-click selects like left-click), then open the
/// shared menu pipeline over the task's context — the Task group's
/// flat page over `[TaskDetail, Global]`, anchored at the pointer.
/// Stale indices against an older tables pass are absorbed silently.
impl Adapter {
    fn open_task_context_menu(&mut self, window: &AppWindow, x: f32, y: f32) {
        let chain = [ContextId::TaskDetail, ContextId::Global].to_vec();
        self.menu_anchor = MenuAnchor::Pointer(x, y);
        self.menu_chain = chain;
        self.root_entries = menu::context_entries(&self.keymap, &self.menu_chain);
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
        route.borrow_mut().route(&window, ActionId::StackCreate);
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
    adapter.view_state.set_buffer(text.to_owned());
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

/// The task-detail title field: mirror + commit (D14).
fn install_task_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    let mirror = Rc::clone(adapter);
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
                ContextId::Board => crate::view_state::ViewId::Board,
                ContextId::StackDetail => crate::view_state::ViewId::StackDetail,
                ContextId::TaskDetail => crate::view_state::ViewId::TaskDetail,
                ContextId::Global | ContextId::Menu | ContextId::ViNormal | ContextId::ViInsert => {
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
