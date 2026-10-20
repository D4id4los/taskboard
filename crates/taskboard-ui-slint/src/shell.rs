// SPDX-License-Identifier: AGPL-3.0-only

//! The keymap shell state (chunk decision D17): the pure half of the
//! adapter — context-chain construction (D11), menu cursor state, the
//! vi mode, and the focused-entity seam. No slint types; the lib
//! adapter is the only code that syncs this onto window properties.

use crate::actions::ContextId;
use crate::router::{Mode, UiEffect};

/// Menu cursor state: the generated entries live in the window model;
/// the shell tracks openness and the selection index only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    pub open: bool,
    pub selected: usize,
    /// The entry count of the open menu (wrapping bound for the
    /// cursor; 0 while closed).
    pub len: usize,
}

/// The shell state (D17): mode, menu, the active view context
/// (`None` in chunk-2 sessions — no views exist), and the focused
/// entities.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeymapShell {
    pub mode: Mode,
    pub menu: MenuState,
    pub view: Option<ContextId>,
    /// The [`crate::router::DispatchCtx`] carried to the router.
    pub focused_task: Option<taskboard_domain::TaskId>,
    pub focused_stack: Option<taskboard_domain::StackId>,
}

impl KeymapShell {
    /// The context chain for resolution under the current state
    /// (D11): menu open → `[Menu]` (the modal masks everything);
    /// insert mode → `[ViInsert, Global]` (D5: while a text field
    /// holds the flow, the view context is masked so printables reach
    /// the field untouched — the D18 text-input contract); otherwise
    /// view context, the always-present mode context (an empty member
    /// changes nothing, D10), then Global.
    #[must_use]
    pub fn chain(&self) -> Vec<ContextId> {
        if self.menu.open {
            return vec![ContextId::Menu];
        }
        if self.mode == Mode::Insert {
            return vec![ContextId::ViInsert, ContextId::Global];
        }
        let mut chain = Vec::with_capacity(3);
        if let Some(view) = self.view {
            chain.push(view);
        }
        chain.push(ContextId::ViNormal);
        chain.push(ContextId::Global);
        chain
    }

    /// The router context (D16).
    #[must_use]
    pub fn dispatch_ctx(&self) -> crate::router::DispatchCtx {
        crate::router::DispatchCtx {
            focused_task: self.focused_task,
            focused_stack: self.focused_stack,
            reminder_target: None,
            board_target: None,
        }
    }

    /// Applies a UI effect to the state (D17). `Exit` is *not*
    /// handled here — quitting the slint event loop is the adapter's
    /// job; it reports `false` for that case. Menu-opening needs the
    /// precomputed entry count (the entries themselves are a menu
    /// projection, D19).
    pub fn apply(&mut self, effect: &UiEffect, menu_len: usize) -> bool {
        match *effect {
            UiEffect::Exit => return false,
            UiEffect::OpenMenu => {
                self.menu = MenuState {
                    open: true,
                    selected: 0,
                    len: menu_len,
                };
            }
            UiEffect::CloseMenu => self.menu = MenuState::default(),
            UiEffect::MenuNext => {
                if self.menu.len > 0 {
                    self.menu.selected = (self.menu.selected + 1) % self.menu.len;
                }
            }
            UiEffect::MenuPrevious => {
                if self.menu.len > 0 {
                    self.menu.selected = (self.menu.selected + self.menu.len - 1) % self.menu.len;
                }
            }
            UiEffect::InvokeSelected => {
                // The adapter performs the close-then-redispatch (D16);
                // the shell only records the close.
                self.menu = MenuState::default();
            }
            UiEffect::SetMode(mode) => self.mode = mode,
            // The chunk-3 view intents (D6/D9) live in `view_state`;
            // the shell records nothing for them.
            UiEffect::FocusNext
            | UiEffect::FocusPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::BeginCreate
            | UiEffect::BeginCreateStack
            | UiEffect::Activate
            | UiEffect::RequestMove
            | UiEffect::BoardNext
            | UiEffect::BoardPrevious
            | UiEffect::BoardSelect(_) => {}
        }
        true
    }

    /// The currently selected menu entry, if any (the adapter's
    /// `InvokeSelected` re-dispatch source).
    #[must_use]
    pub fn selected_entry(&self) -> Option<usize> {
        self.menu
            .open
            .then_some(self.menu.selected)
            .filter(|_| self.menu.len > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T23 — chain construction (D11): menu open is the pure modal
    /// chain; otherwise view?, mode (always), Global.
    #[test]
    fn chain_construction_matches_the_context_model() {
        let mut shell = KeymapShell::default();
        assert_eq!(
            shell.chain(),
            vec![ContextId::ViNormal, ContextId::Global],
            "mode context always present, view absent in chunk 2"
        );

        shell.view = Some(ContextId::Board);
        assert_eq!(
            shell.chain(),
            vec![ContextId::Board, ContextId::ViNormal, ContextId::Global]
        );

        // D5: insert masks the view context — typing flows to fields.
        shell.mode = Mode::Insert;
        assert_eq!(
            shell.chain(),
            vec![ContextId::ViInsert, ContextId::Global],
            "insert mode masks the view context"
        );

        shell.menu.open = true;
        assert_eq!(
            shell.chain(),
            vec![ContextId::Menu],
            "menu is the pure modal"
        );
    }

    /// T23 — menu effects: open resets the cursor, next/previous wrap,
    /// close clears.
    #[test]
    fn menu_effects_drive_the_cursor() {
        let mut shell = KeymapShell::default();
        assert!(shell.apply(&UiEffect::OpenMenu, 3));
        assert_eq!(
            shell.menu,
            MenuState {
                open: true,
                selected: 0,
                len: 3
            }
        );
        assert_eq!(shell.selected_entry(), Some(0));

        shell.apply(&UiEffect::MenuNext, 3);
        shell.apply(&UiEffect::MenuNext, 3);
        assert_eq!(shell.selected_entry(), Some(2));
        shell.apply(&UiEffect::MenuNext, 3);
        assert_eq!(shell.selected_entry(), Some(0), "next wraps");
        shell.apply(&UiEffect::MenuPrevious, 3);
        assert_eq!(shell.selected_entry(), Some(2), "previous wraps");

        shell.apply(&UiEffect::CloseMenu, 0);
        assert_eq!(shell.menu, MenuState::default());
        assert_eq!(shell.selected_entry(), None);
    }

    /// T23 — mode flips are ordinary state (D10); exit is the
    /// adapter's job and reports `false`.
    #[test]
    fn mode_flips_are_plain_state_and_exit_is_reported() {
        let mut shell = KeymapShell::default();
        assert!(shell.apply(&UiEffect::SetMode(Mode::Insert), 0));
        assert_eq!(shell.mode, Mode::Insert);
        assert!(!shell.apply(&UiEffect::Exit, 0), "exit is not shell state");
    }

    /// T23 — a zero-length menu ignores cursor movements (nothing to
    /// select; division by zero guarded).
    #[test]
    fn cursor_movement_on_an_empty_menu_is_inert() {
        let mut shell = KeymapShell::default();
        shell.apply(&UiEffect::OpenMenu, 0);
        shell.apply(&UiEffect::MenuNext, 0);
        shell.apply(&UiEffect::MenuPrevious, 0);
        assert_eq!(shell.selected_entry(), None);
    }

    /// Design §3.8 — the board effects are view intents: the shell
    /// records nothing for them (they apply `true`, plain state
    /// unchanged; resolution happens in the view-state machine).
    #[test]
    fn board_effects_record_nothing_in_the_shell() {
        let mut shell = KeymapShell::default();
        let before = shell;
        for effect in [
            UiEffect::BoardNext,
            UiEffect::BoardPrevious,
            UiEffect::BoardSelect(taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7())),
        ] {
            assert!(shell.apply(&effect, 3), "a board effect is plain state");
        }
        assert_eq!(shell.menu, before.menu);
        assert_eq!(shell.mode, before.mode);
    }
}
