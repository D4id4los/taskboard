// SPDX-License-Identifier: AGPL-3.0-only

//! The status-surface journeys (OP3, the accepted journey plan
//! `2026-11-21-parity-restore-journeys-status-surface.md` §6's W-1..W-3
//! rows): panel open dispatches exactly one `StatusQuery` (D-3's
//! snapshot-on-open), Refresh re-queries (US-1/S4), a dropped reply
//! renders the typed failure posture (W-3), the draining view folds to
//! the draining posture (US-2), and no `StateCommand` is ever
//! reachable from the panel (R-3/AC-4). The CLI leg (#1111) lives in
//! `crates/taskboard-app/tests/it_status_surface.rs`.

use crate::support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::focus_journey::JourneyHarness;
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::{Keymap, Preset};
use taskboard_ui_slint::requests::{StatusPosture, StatusSnapshot};
use taskboard_ui_slint::{StatusPanelFacts, StatusPanelPosture, UiSettings};

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The journey harness with the request lane *held* (the OP3 shape):
/// the test plays the app half — it drains the requests, answers the
/// oneshot (or drops it for the failure posture), then drives the
/// drain. Same preamble as [`JourneyHarness::mount`], with the
/// receiver half kept for the test instead of dropped.
fn mount_holding_requests(
    state: AppState,
    preset: Preset,
    kiosk: bool,
) -> (
    JourneyHarness,
    tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
) {
    let settings = UiSettings {
        keymap: Keymap::from_preset(preset),
        keymap_preset: preset,
        kiosk,
        ..UiSettings::default()
    };
    let state = Arc::new(ArcSwap::from_pointee(state));
    let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = taskboard_ui_slint::mount(
        state,
        dummy_receiver(),
        dummy_system_receiver(),
        commands,
        requests_tx,
        settings,
        detached_health(),
        None,
    )
    .expect("mount succeeds on the testing backend");
    let mut harness = JourneyHarness {
        driver: SlintDriver::default(),
        handle,
        engine,
    };
    harness.driver.bind(&harness.handle);
    harness.handle.dispatch_action(ActionId::NavFocusBoard);
    (harness, requests_rx)
}

fn run(body: impl FnOnce(AppState) + Send + 'static) {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            body(state);
        },
    );
}

/// US-1/S1 (OP3 W-1, journey plan AC-1) — the open dispatches exactly
/// one `StatusQuery` (snapshot-on-open, D-3), the reply folds into the
/// panel's typed facts (W-2), and no `StateCommand` is reachable (R-3,
/// AC-4's structural inventory through the engine boundary).
#[test]
fn us1_s1_panel_open_dispatches_status_query() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state, Preset::Standard, false);
        harness.handle.dispatch_action(ActionId::StatusPanelOpen);
        harness.advance();
        assert_eq!(
            harness.handle.window().get_current_view(),
            taskboard_ui_slint::View::Status,
            "the toggle opened the panel view"
        );

        // Exactly one request, and the panel holds the busy posture
        // until the reply folds.
        let request = requests
            .try_recv()
            .expect("the open dispatched one StatusQuery");
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } = request else {
            panic!("the open dispatched StatusQuery");
        };
        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert!(
            facts.get_busy(),
            "the mounting panel holds the busy posture"
        );

        // The app half answers with a plain no-daemon posture.
        reply
            .send(StatusSnapshot {
                daemon: false,
                draining: None,
                holder_pid: Some(999),
                ..StatusSnapshot::default()
            })
            .expect("reply accepted");
        harness.handle.drain_status_reply();

        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert!(!facts.get_busy(), "the fold clears the busy posture");
        assert!(!facts.get_failure(), "a settled reply is not a failure");
        assert_eq!(
            facts.get_data().posture,
            StatusPanelPosture::NoDaemon,
            "draining None folds to the plain no-daemon posture (B6's law)"
        );
        assert_eq!(facts.get_data().holder_pid, 999, "the holder pid folds");

        // R-3/AC-4: the panel dispatched no engine command.
        assert_eq!(
            harness.engine.drain(),
            Vec::<StateCommand>::new(),
            "the status panel dispatches no StateCommand"
        );
    });
}

/// US-1/S4 (OP3 W-1, journey plan AC-2) — Refresh re-queries: the
/// explicit control dispatches a second `StatusQuery` and the fresh
/// reply re-folds (no polling timer anywhere in the journey).
#[test]
fn us1_s4_refresh_requeries_and_refolds() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state, Preset::Standard, false);
        harness.handle.dispatch_action(ActionId::StatusPanelOpen);
        harness.advance();
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } =
            requests.try_recv().expect("the open dispatched one query")
        else {
            panic!("StatusQuery");
        };
        reply
            .send(StatusSnapshot {
                daemon: false,
                draining: None,
                ..StatusSnapshot::default()
            })
            .expect("reply accepted");
        harness.handle.drain_status_reply();

        // Refresh (the panel's own control): one re-query, and the
        // fresh reply re-folds into a different posture.
        harness
            .handle
            .window()
            .global::<StatusPanelFacts>()
            .invoke_status_panel_refresh();
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } =
            requests.try_recv().expect("refresh dispatched one query")
        else {
            panic!("StatusQuery");
        };
        reply
            .send(StatusSnapshot {
                daemon: true,
                version: Some("9.9.9".to_owned()),
                backend: Some("standalone".to_owned()),
                pid: Some(1),
                ..StatusSnapshot::default()
            })
            .expect("reply accepted");
        harness.handle.drain_status_reply();

        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert_eq!(
            facts.get_data().posture,
            StatusPanelPosture::Serving,
            "the fresh reply re-folds the fields"
        );
        assert_eq!(harness.engine.drain(), Vec::<StateCommand>::new());
    });
}

/// US-1/S3 (OP3 W-3, journey plan AC-3) — a dropped reply (the app
/// half gone / wedged past the reply) renders the typed failure
/// posture, never a blank panel; Refresh stays available after it.
#[test]
fn us1_s3_failed_query_renders_failure_posture() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state, Preset::Standard, false);
        harness.handle.dispatch_action(ActionId::StatusPanelOpen);
        harness.advance();
        // Drop the reply half without answering — the wedge shape.
        drop(requests.try_recv().expect("the open dispatched one query"));
        harness.handle.drain_status_reply();

        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert!(facts.get_failure(), "the failure posture renders");
        assert!(
            !facts.get_busy(),
            "the busy posture cleared with the failure"
        );

        // Refresh remains available (US-1/S3's escape cell).
        harness
            .handle
            .window()
            .global::<StatusPanelFacts>()
            .invoke_status_panel_refresh();
        let request = requests.try_recv().expect("refresh re-queries");
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } = request else {
            panic!("StatusQuery");
        };
        reply
            .send(StatusSnapshot::default())
            .expect("reply accepted");
        harness.handle.drain_status_reply();
        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert!(
            !facts.get_failure(),
            "a fresh reply clears the failure posture"
        );
    });
}

/// US-2/S1 (OP3 W-2, journey plan US-2/AC-1) — the draining view folds
/// to the draining posture with the holder pid: the same typed rule
/// the CLI renderer consumes (D-2/D-4), never a text oracle.
#[test]
fn us2_s1_draining_view_folds_to_draining_posture() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state, Preset::Standard, false);
        harness.handle.dispatch_action(ActionId::StatusPanelOpen);
        harness.advance();
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } =
            requests.try_recv().expect("the open dispatched one query")
        else {
            panic!("StatusQuery");
        };
        reply
            .send(StatusSnapshot {
                daemon: false,
                draining: Some(true),
                holder_pid: Some(4242),
                ..StatusSnapshot::default()
            })
            .expect("reply accepted");
        harness.handle.drain_status_reply();

        let facts = harness.handle.window().global::<StatusPanelFacts>();
        let data = facts.get_data();
        assert_eq!(
            data.posture,
            StatusPanelPosture::Draining,
            "daemon==false ∧ draining==Some(true) folds to the draining posture"
        );
        assert!(data.draining, "the draining display fact rides the fold");
        assert_eq!(data.holder_pid, 4242, "the lock holder pid folds");
        assert_eq!(harness.engine.drain(), Vec::<StateCommand>::new());
    });
}

/// US-2/S2 (OP3 W-2, journey plan US-2/AC-2) — `draining: None` folds
/// to the plain no-daemon posture (B6's law: never `Some(false)`); the
/// pure fold's unit pin rides beside it (one rule, both fronts).
#[test]
fn us2_s2_draining_none_folds_to_plain_no_daemon_posture() {
    // The fold itself (the lowest layer, pure): the B6 shapes.
    use taskboard_ui_slint::requests::fold_status_posture;
    let serving = StatusSnapshot {
        daemon: true,
        ..StatusSnapshot::default()
    };
    assert_eq!(fold_status_posture(&serving), StatusPosture::Serving);
    let draining = StatusSnapshot {
        daemon: false,
        draining: Some(true),
        ..StatusSnapshot::default()
    };
    assert_eq!(fold_status_posture(&draining), StatusPosture::Draining);
    let plain = StatusSnapshot {
        daemon: false,
        draining: None,
        ..StatusSnapshot::default()
    };
    assert_eq!(fold_status_posture(&plain), StatusPosture::NoDaemon);
    let never_minted = StatusSnapshot {
        daemon: false,
        draining: Some(false),
        ..StatusSnapshot::default()
    };
    assert_eq!(
        fold_status_posture(&never_minted),
        StatusPosture::NoDaemon,
        "Some(false) is the absent fact, not a posture (B6's law)"
    );

    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state, Preset::Standard, false);
        harness.handle.dispatch_action(ActionId::StatusPanelOpen);
        harness.advance();
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } =
            requests.try_recv().expect("the open dispatched one query")
        else {
            panic!("StatusQuery");
        };
        reply.send(plain).expect("reply accepted");
        harness.handle.drain_status_reply();
        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert_eq!(
            facts.get_data().posture,
            StatusPanelPosture::NoDaemon,
            "the plain no-daemon posture renders (US-2/S2)"
        );
    });
}

/// US-1/S1 kiosk leg (OP3 D-5) — the panel mounts read-only on kiosk
/// from the same action: one query, the fold, and no command dispatch
/// anywhere in the journey (the structural inventory's kiosk row).
#[test]
fn us1_s1_kiosk_panel_mounts_readonly() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state, Preset::Standard, true);
        harness.handle.dispatch_action(ActionId::StatusPanelOpen);
        harness.advance();
        let request = requests.try_recv().expect("the kiosk panel queries");
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } = request else {
            panic!("StatusQuery");
        };
        reply
            .send(StatusSnapshot {
                daemon: false,
                draining: None,
                holder_pid: Some(7),
                ..StatusSnapshot::default()
            })
            .expect("reply accepted");
        harness.handle.drain_status_reply();
        let facts = harness.handle.window().global::<StatusPanelFacts>();
        assert_eq!(facts.get_data().posture, StatusPanelPosture::NoDaemon);
        assert_eq!(
            harness.engine.drain(),
            Vec::<StateCommand>::new(),
            "the kiosk panel dispatches no StateCommand (D-5 read-only)"
        );
    });
}
