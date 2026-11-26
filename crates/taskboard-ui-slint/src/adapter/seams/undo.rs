// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::AppWindow;
use crate::actions::ActionId;

use super::Adapter;
use crate::adapter::install_seam;

/// The undo branch picker's seams (UF2, #353): the W-1 mount ack, the
/// plain reflex rows (click ≡ key — the same `AppUndo`/`AppRedo` route
/// the keyboard uses), the W-2/W-3 row seams (the payload id re-typed
/// at the trust boundary; a stale or foreign id dispatches nothing —
/// the silent law), and the AC-4 outside-click dismissal.
pub(in crate::adapter) fn install_undo_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
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
    /// The row-commit prelude: the picker closes first (the
    /// close-then-dispatch shape — the `InvokeSelected` precedent,
    /// bounded), then the closure dispatches.
    pub(in crate::adapter) fn undo_picker_commit(
        &mut self,
        window: &AppWindow,
        dispatch: impl FnOnce(&mut Self, &AppWindow),
    ) {
        self.shell.undo_picker = false;
        self.sync_undo_picker_models(window);
        dispatch(self, window);
    }
}
