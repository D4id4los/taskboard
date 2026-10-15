// SPDX-License-Identifier: AGPL-3.0-only

//! The slint adapter (chunk decision D18, design §5.6): the only code
//! that touches both worlds — slint key events in, keymap types out,
//! and shell state synced one-way onto window globals. The root
//! `FocusScope` is the single key path (§5.1); `KeyBinding` is unused
//! for any rebindable action (Q9); the keymap is applied one-way at
//! UI start (no live re-map machinery — §5.6).

use std::cell::RefCell;
use std::rc::Rc;

use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, ModelRc, SharedString};
use tokio::sync::mpsc;

use taskboard_domain::{StackId, StateCommand, TaskId};

use crate::actions::{ActionId, ContextId};
use crate::keymap::Keymap;
use crate::menu::{self, MenuEntry};
use crate::resolver::{KeymapResolver, Resolution};
use crate::router::{ActionRouter, DispatchOutcome, UiEffect};
use crate::shell::KeymapShell;
use crate::{AppWindow, KeyFeedback, KeyboardState, UiStatus};

/// The Q15 startup-warning summary handed over with the validated
/// keymap; the status bar renders it from `UiStatus` (chunk 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartupWarnings {
    pub keymap_findings: u32,
}

/// The adapter state: the validated keymap (applied at UI start, one
/// way), the shell, the resolver, the router, and the command
/// channel out.
pub(crate) struct Adapter {
    keymap: Keymap,
    shell: KeymapShell,
    resolver: KeymapResolver,
    commands: mpsc::Sender<StateCommand>,
    /// The open menu's entries (the D19 projection snapshot the
    /// cursor indexes into and `InvokeSelected` re-dispatches).
    menu_entries: Vec<MenuEntry>,
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
        let outcome = ActionRouter::dispatch(id, &self.shell.dispatch_ctx());
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
        self.sync(window);
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
            UiEffect::SetMode(_) => {
                self.shell.apply(&effect, 0);
            }
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
) -> Rc<RefCell<Adapter>> {
    let adapter = Rc::new(RefCell::new(Adapter {
        keymap,
        shell: KeymapShell::default(),
        resolver: KeymapResolver::new(),
        commands,
        menu_entries: Vec::new(),
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
    adapter.borrow().sync(window);
    adapter
}

/// The chunk-3 focus seam surface (D16/D17): sets the view context /
/// focused entities and resets the pending accumulation (a context
/// change invalidates it — D12 rule 1).
impl crate::UiHandle {
    #[allow(dead_code)] // chunk-3's views drive this for real; tests use it now
    pub fn set_view_context(&self, view: Option<ContextId>) {
        let mut adapter = self.adapter.borrow_mut();
        adapter.shell.view = view;
        adapter.resolver.reset();
    }

    #[allow(dead_code)] // ditto
    pub fn set_focused_entities(
        &self,
        focused_task: Option<TaskId>,
        focused_stack: Option<StackId>,
    ) {
        let mut adapter = self.adapter.borrow_mut();
        adapter.shell.focused_task = focused_task;
        adapter.shell.focused_stack = focused_stack;
    }
}
