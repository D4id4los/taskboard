// SPDX-License-Identifier: AGPL-3.0-only

use super::Adapter;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;
use crate::actions::ActionId;

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
pub(in crate::adapter) fn install_export_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
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
