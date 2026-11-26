// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use super::Adapter;
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;

/// The concentration overlay's seams (#496): the scrim close, the
/// note's mirror + capture-commit funnel, the switcher's start (the
/// notification-row `track_target` precedent), and the session stop.
/// Every session verb routes the landed registry action — zero new
/// session commands (the fence).
pub(in crate::adapter) fn install_focus_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
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
