// SPDX-License-Identifier: AGPL-3.0-only

use super::Adapter;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;

/// The toolbar's menu button is a stateful toggle (round-1 D1): open
/// when closed (routing `app.menu`), a full close when open (routing
/// `menu.dismiss`) — the F1 fix, since the toggle means `OpenMenu` can
/// only ever fire while the menu is closed, so the pre-menu chain the
/// projection reads is the real one on every open.
/// The status panel's seams (OP3 W-1..W-3): the mount's
/// snapshot-on-open dispatch retired (#1281: the route arm is the
/// sole dispatcher) and the Refresh control's re-query rides the
/// `AppWindow` root callback (the seam census's manifest shape —
/// listed in `seam_inventory.rs`). The fold/failure directions
/// are invokes (Rust → slint). The panel dispatches no `StateCommand`
/// — R-3's read-only contract is structural here: no seam body touches
/// the command lane.
pub(in crate::adapter) fn install_status_panel_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    // #1281: the `status-panel-open` seam retired — the route arm is
    // the snapshot query's sole dispatcher (one query per mount).
    // slint: status-panel-refresh — the explicit re-query (US-1/S4).
    install_seam!(window: window.on_status_panel_refresh, adapter: adapter;
    bind mut (a, window), args (), {        a.dispatch_status_query(&window);

    });
    // The remediation lane (the D2 command plan W-1..W-4/W-6): the
    // rows' arm moments (arm-only — nothing dispatches), the armed
    // strip's decision pair, and the outcome fold. The kiosk
    // permission gate lives in the arm (the trap-a law — the
    // structural inventory's no-dispatch assertion holds by
    // construction). The permission fold is minted here at install:
    // a kiosk lifecycle renders the rows disabled (D-9).
    {
        let kiosk = adapter.borrow().kiosk;
        window
            .global::<crate::StatusPanelFacts>()
            .set_remediation_permitted(!kiosk);
    }
    install_seam!(window: window.on_status_holder_kill, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_arm(&window, crate::requests::StatusRemediationAction::KillHolder);

    });
    install_seam!(window: window.on_status_lock_force_unlock, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_arm(&window, crate::requests::StatusRemediationAction::ForceUnlock);

    });
    install_seam!(window: window.on_status_daemon_restart, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_arm(&window, crate::requests::StatusRemediationAction::RestartDaemon);

    });
    // The armed strip's decision pair (W-4): Confirm routes the
    // armed verb's exact request; Cancel disarms with nothing
    // dispatched.
    install_seam!(window: window.on_status_remediation_armed, adapter: adapter;
    bind mut (a, window), args (decision), {        a.status_remediation_decision(&window, decision);

    });
    // The W-6 display fold: the reply drain invokes the callback
    // outside the adapter borrow (the re-entrancy law); the handler
    // folds the stashed outcome and re-queries (D-3's post-fact
    // refresh).
    install_seam!(window: window.on_status_remediation_outcome_shown, adapter: adapter;
    bind mut (a, window), args (), {        a.status_remediation_outcome_fold(&window);

    });
}

/// Folds one settled [`StatusSnapshot`] into the panel's presentation
/// data (OP3 W-2): the typed posture carries the D-2 rule verbatim;
/// absent facts render as -1 / empty strings. Presentation fold only —
/// no GUI-side fact is minted.
pub(in crate::adapter) fn status_panel_data(
    snapshot: &crate::requests::StatusSnapshot,
) -> crate::StatusPanelData {
    let absent = -1_i32;
    let as_int = |value: Option<u32>| value.and_then(|v| i32::try_from(v).ok()).unwrap_or(absent);
    crate::StatusPanelData {
        posture: match crate::requests::fold_status_posture(snapshot) {
            crate::requests::StatusPosture::Serving => crate::StatusPanelPosture::Serving,
            crate::requests::StatusPosture::Draining => crate::StatusPanelPosture::Draining,
            crate::requests::StatusPosture::NoDaemon => crate::StatusPanelPosture::NoDaemon,
        },
        holder_pid: as_int(snapshot.holder_pid),
        version: snapshot.version.clone().unwrap_or_default().into(),
        backend: snapshot.backend.clone().unwrap_or_default().into(),
        pid: as_int(snapshot.pid),
        connector_present: snapshot.connector_attached.is_some(),
        connector_attached: snapshot.connector_attached.unwrap_or(false),
        draining: snapshot.draining == Some(true),
        sync_phase: snapshot.sync_phase.clone().unwrap_or_default().into(),
        sync_pending: as_int(snapshot.sync_pending_ops),
        engine_commands: snapshot
            .engine_commands
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(absent),
        engine_rejections: snapshot
            .engine_rejections
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(absent),
        engine_signals: snapshot
            .engine_signals
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(absent),
    }
}
