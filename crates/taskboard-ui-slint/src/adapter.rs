// SPDX-License-Identifier: AGPL-3.0-only

//! The slint adapter (chunk decision D18, design §5.6): the only code
//! that touches both worlds — slint key events in, keymap types out,
//! and shell state synced one-way onto window globals. The root
//! `FocusScope` is the single key path (§5.1); `KeyBinding` is unused
//! for any rebindable action (Q9); the keymap is applied one-way at
//! UI start (no live re-map machinery — §5.6).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use arc_swap::ArcSwap;

use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, ModelRc, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{AppState, StackId, StateCommand, TaskId};

use crate::actions::{ActionId, ContextId};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::presentation::ReminderTarget;
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{ActionRouter, DispatchOutcome, Mode, UiEffect};
use crate::shell::KeymapShell;
use crate::view_state::{Flow, FlowOutcome, ViewState};
use crate::{AppWindow, Breakpoint, KeyFeedback, KeyboardState, Responsive, UiStatus};

/// The Q15 startup-warning summary handed over with the validated
/// keymap; the status bar renders it from `UiStatus` (chunk 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartupWarnings {
    pub keymap_findings: u32,
}

/// The adapter state: the validated keymap (applied at UI start, one
/// way), the shell, the resolver, the router, the view-state machine
/// (D3), the lock-free state read (D1), and the command channel out.
pub(crate) struct Adapter {
    keymap: Keymap,
    shell: KeymapShell,
    resolver: KeymapResolver,
    view_state: ViewState,
    state: Arc<ArcSwap<AppState>>,
    commands: mpsc::Sender<StateCommand>,
    /// The open menu's entries (the D19 projection snapshot the
    /// cursor indexes into and `InvokeSelected` re-dispatches).
    menu_entries: Vec<MenuEntry>,
    /// The banner row target set by row activation (D4); consumed by
    /// the next snooze/mute dispatch, then cleared (one-shot).
    reminder_target: Option<ReminderTarget>,
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
    /// re-decides. `RefreshNow` only marks the repaint need — the
    /// timer-driven `refresh()` (D1) picks it up.
    fn apply_outcomes(&mut self, outcomes: &[FlowOutcome]) {
        for outcome in outcomes {
            match outcome {
                FlowOutcome::Command(command) => {
                    if self.commands.try_send(command.clone()).is_err() {
                        tracing::warn!("command channel full or closed; dropped");
                    }
                }
                FlowOutcome::SetMode(mode) => {
                    self.shell.apply(&UiEffect::SetMode(*mode), 0);
                }
                FlowOutcome::RefreshNow => {}
            }
        }
    }

    /// The D1 refresh's view-state half: reconcile subjects and focus
    /// against the current snapshot (the model pushes grow with the
    /// views).
    pub(crate) fn reconcile(&mut self, state: &AppState) {
        self.view_state.reconcile(state);
    }

    /// Whether the window is at the compact breakpoint (D17).
    fn is_compact(window: &AppWindow) -> bool {
        window.global::<Responsive>().get_current_breakpoint() == Breakpoint::Compact
    }

    /// The D9 interpretation table: one `match (flow, effect)` whose
    /// arms route to `view_state` methods named by what they do.
    fn interpret(&mut self, window: &AppWindow, effect: UiEffect) {
        let compact = Self::is_compact(window);
        let state = self.state.load();
        let flow = self.view_state.flow().clone();
        let leaving = matches!(
            effect,
            UiEffect::NextView | UiEffect::PrevView | UiEffect::FocusBoard
        );

        // Leaving the edit field by effect first commits the buffer;
        // a move flow ends without dispatch (D9's recorded rule).
        if leaving {
            match flow {
                Flow::Create { .. } | Flow::EditTitle { .. } => {
                    let outcomes = self.view_state.commit_edit(&state);
                    self.apply_outcomes(&outcomes);
                }
                Flow::Move { .. } => {
                    let outcomes = self.view_state.cancel_move();
                    self.apply_outcomes(&outcomes);
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
            // A mode flip with no view meaning stays plain shell
            // state (D10) — e.g. Escape out of an empty insert.
            (_, UiEffect::SetMode(mode)) => {
                self.shell.apply(&UiEffect::SetMode(mode), 0);
                Vec::new()
            }
            (_, _) => Vec::new(),
        };
        self.apply_outcomes(&outcomes);
    }

    fn apply_effect(&mut self, window: &AppWindow, effect: UiEffect) {
        match effect {
            UiEffect::Exit => crate::quit(),
            UiEffect::OpenMenu => {
                // The projection is computed for the pre-menu chain
                // (the active context's actions, §5.9); the menu
                // context itself masks the chain once open (D11).
                let chain = self.shell.chain();
                self.menu_entries = menu::entries(&self.keymap, &chain);
                let models: Vec<crate::MenuEntryModel> = self
                    .menu_entries
                    .iter()
                    .map(|entry| crate::MenuEntryModel {
                        action: entry.action.id().into(),
                        label: entry.label.into(),
                        enabled: entry.enabled,
                        hint: ModelRc::new(slint::VecModel::from(
                            entry
                                .hint
                                .iter()
                                .map(|stroke| SharedString::from(stroke.as_str()))
                                .collect::<Vec<_>>(),
                        )),
                    })
                    .collect();
                window
                    .global::<KeyboardState>()
                    .set_menu_entries(ModelRc::from(models.as_slice()));
                self.shell.apply(&effect, self.menu_entries.len());
            }
            UiEffect::InvokeSelected => {
                let selected = self.shell.selected_entry();
                self.shell.apply(&UiEffect::CloseMenu, 0);
                // Close first, then one re-dispatch — bounded (D16).
                if let Some(index) = selected {
                    self.invoke_entry(window, index);
                }
            }
            UiEffect::CloseMenu | UiEffect::MenuNext | UiEffect::MenuPrevious => {
                self.shell.apply(&effect, self.menu_entries.len());
            }
            UiEffect::SetMode(_)
            | UiEffect::FocusNext
            | UiEffect::FocusPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::BeginCreate
            | UiEffect::Activate
            | UiEffect::RequestMove => self.interpret(window, effect),
        }
    }

    /// The `menu.invoke` projection (D16): the entry at `index` is
    /// re-dispatched through the same router. The mouse click path
    /// lands here too — every click affordance has an action
    /// equivalent (§7.1).
    fn invoke_entry(&mut self, window: &AppWindow, index: usize) {
        let Some(entry) = self.menu_entries.get(index) else {
            return;
        };
        let action = entry.action;
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
    state: Arc<ArcSwap<AppState>>,
) -> Rc<RefCell<Adapter>> {
    let adapter = Rc::new(RefCell::new(Adapter {
        keymap,
        shell: KeymapShell::default(),
        resolver: KeymapResolver::new(),
        view_state: ViewState::new(),
        state,
        commands,
        menu_entries: Vec::new(),
        reminder_target: None,
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
            adapter.borrow_mut().invoke_entry(&window, index);
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
    adapter.borrow().sync(window);
    adapter
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
