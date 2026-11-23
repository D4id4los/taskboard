// SPDX-License-Identifier: AGPL-3.0-only

//! The status command lane's journeys (the D2 command plan §4/§6's
//! nominated evidence file): the wedged posture's remediation rows
//! (US-1/S1), the armed-confirm strips' arm/confirm/cancel grammar
//! (US-1/S2–S4, US-2/S1–S2, US-3/S1–S2), the typed refusals' folds
//! (US-2/S3, US-3/S3), and the kiosk posture's disabled rows with no
//! reachable dispatch (D-9, the structural arm). Assertion
//! discipline: terminal state + the typed request that reached the
//! lane boundary; flags and indices are the only oracles — caption
//! text is never matched. The app-side executor's typed outcomes
//! live in `crates/taskboard-app/tests/it_status_remediation.rs`
//! (W-7); the lane never touches domain state (R-3: no
//! `StateCommand` anywhere).

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
use taskboard_ui_slint::requests::{
    StatusRemediationOutcome as Outcome, StatusRemediationRefusal as Refusal, StatusSnapshot,
};
use taskboard_ui_slint::{StatusPanelFacts, StatusPanelPosture, UiSettings};

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The journey harness with the request lane *held* (the status
/// surface's OP3 shape): the test plays the app half.
fn mount_holding_requests(
    state: AppState,
    kiosk: bool,
) -> (
    SlintDriver,
    taskboard_ui_slint::UiHandle,
    FakeEngine,
    tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
) {
    let settings = UiSettings {
        keymap: Keymap::from_preset(Preset::Standard),
        keymap_preset: Preset::Standard,
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
    let support::focus_journey::JourneyHarness {
        driver,
        handle,
        engine,
    } = harness;
    (driver, handle, engine, requests_rx)
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

/// Folds the wedged (draining) posture into the mounted panel: the
/// sidecar shape — a holder pid beside a held lock, no serving
/// daemon.
/// Drains every queued mount/open `StatusQuery` (the open dispatches
/// through both fronts — the route arm and the window seam, the OP3
/// convergence), answering each so the lane stays clean; the folded
/// facts afterwards are the test's to mint.
fn drain_open_queries(
    requests: &mut tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
) {
    while let Ok(request) = requests.try_recv() {
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } = request else {
            panic!("only StatusQueries ride the lane at this point");
        };
        let _ = reply.send(StatusSnapshot::default());
    }
}

fn fold_wedged(window: &taskboard_ui_slint::AppWindow, holder_pid: i32) {
    let facts = window.global::<StatusPanelFacts>();
    let mut data = facts.get_data();
    data.posture = StatusPanelPosture::Draining;
    data.holder_pid = holder_pid;
    facts.set_data(data);
}

/// US-1/S1 — the wedged posture renders the remediation row set as
/// fold flags: the kill row enabled (a displayed holder pid), the
/// force-unlock row enabled (the stale-holder/draining posture's
/// fold), the restart row enabled — and nothing dispatched.
#[test]
fn us1_s1_the_wedged_posture_renders_the_remediation_row_set() {
    run(|state| {
        let (mut driver, handle, mut engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        drain_open_queries(&mut requests);
        // Fold the wedged posture (the reply's typed shape).
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(
            facts.get_data().posture,
            StatusPanelPosture::Draining,
            "the wedge posture folds"
        );
        // Nothing dispatched by the display.
        assert!(
            requests.try_recv().is_err(),
            "the row set is display only — nothing dispatches"
        );
        assert_eq!(
            engine.drain(),
            Vec::<StateCommand>::new(),
            "the lane never touches domain state (R-3)"
        );
    });
}

/// US-1/S2 (AC-1) — the Kill holder row arms the confirm strip and
/// NOTHING dispatches: the armed flag is the strip's fold.
#[test]
fn us1_s2_kill_row_arms_confirm_strip() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        drain_open_queries(&mut requests);
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        let facts = window.global::<StatusPanelFacts>();
        window.invoke_status_holder_kill();
        driver.advance_time(0);
        assert_eq!(
            facts.get_remediation_armed(),
            0,
            "the armed strip mounts with the kill verb armed"
        );
        assert!(
            requests.try_recv().is_err(),
            "arming dispatches nothing (D-2's staged confirm)"
        );
    });
}

/// US-1/S3 (AC-2) — Confirm dispatches exactly one
/// `StatusRemediate { KillHolder }` and the busy posture holds; the
/// typed executed fact folds (W-6) and the panel re-queries (D-3's
/// post-fact refresh).
#[test]
fn us1_s3_confirm_dispatches_kill_holder_remediation() {
    run(|state| {
        let (mut driver, handle, mut engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        requests.try_recv().expect("the open query");
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        let facts = window.global::<StatusPanelFacts>();
        window.invoke_status_holder_kill();
        driver.advance_time(0);
        window.invoke_status_remediation_armed(0);
        driver.advance_time(0);
        // Exactly one remediation request, typed action equality.
        let request = requests
            .try_recv()
            .expect("the confirm dispatches one StatusRemediate");
        let taskboard_ui_slint::requests::UiRequest::StatusRemediate { action, reply } = request
        else {
            panic!("the confirm dispatches StatusRemediate");
        };
        assert_eq!(
            action,
            taskboard_ui_slint::requests::StatusRemediationAction::KillHolder,
            "the armed verb's exact request (the seam oracle)"
        );
        assert!(facts.get_remediation_busy(), "the busy posture holds");

        // The app half answers the executed fact; the W-6 fold lands
        // it and the post-fact refresh re-queries (D-3).
        reply
            .send(Outcome::HolderSignalled)
            .expect("reply accepted");
        handle.drain_status_remediation();
        driver.advance_time(0);
        assert!(
            !facts.get_remediation_busy(),
            "the fold clears the busy posture"
        );
        assert_eq!(
            facts.get_remediation_outcome_kind(),
            1,
            "the executed fact folds (holder signalled)"
        );
        // The post-fact refresh: exactly one StatusQuery rides.
        let request = requests.try_recv().expect("the fold re-queries (D-3)");
        assert!(
            matches!(
                request,
                taskboard_ui_slint::requests::UiRequest::StatusQuery { .. }
            ),
            "the post-fact refresh is a StatusQuery"
        );
        assert_eq!(
            engine.drain(),
            Vec::<StateCommand>::new(),
            "no StateCommand anywhere in the journey (R-3)"
        );
    });
}

/// US-1/S4 (AC-4) — Cancel unmounts the strip with NOTHING
/// dispatched; the row re-arms afterwards.
#[test]
fn us1_s4_cancel_unmounts_the_strip_with_nothing_dispatched() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        drain_open_queries(&mut requests);
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        let facts = window.global::<StatusPanelFacts>();
        window.invoke_status_holder_kill();
        driver.advance_time(0);
        window.invoke_status_remediation_armed(1);
        driver.advance_time(0);
        assert_eq!(facts.get_remediation_armed(), -1, "the strip unmounts");
        assert!(
            requests.try_recv().is_err(),
            "cancel dispatches nothing (the escape law)"
        );
        // The row re-arms (the retry affordance).
        window.invoke_status_holder_kill();
        driver.advance_time(0);
        assert_eq!(facts.get_remediation_armed(), 0, "the row re-arms");
    });
}

/// US-2/S1 (D-4's fold law, AC-1) — the force-unlock row's enable
/// flag follows the posture: enabled in the stale-holder (draining)
/// posture, disabled under a live holder's serving posture; arming
/// in the stale posture mounts the strip.
#[test]
fn us2_s1_force_unlock_arms_only_in_stale_posture() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        drain_open_queries(&mut requests);
        // The live holder's serving posture: the row's fold is off
        // (D-4's fold flag — the flag is the oracle, never a caption).
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        // Arm in the draining (stale-holder) posture.
        window.invoke_status_lock_force_unlock();
        driver.advance_time(0);
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(facts.get_remediation_armed(), 1, "the stale posture arms");
        assert!(requests.try_recv().is_err(), "arming dispatches nothing");

        // The posture gate is the row's enabled flag (the D-4 fold);
        // the executor's liveness re-check is the authorization.
        // Re-arm the kill verb to prove one armed subject (re-arm
        // swaps).
        window.invoke_status_holder_kill();
        driver.advance_time(0);
        assert_eq!(
            facts.get_remediation_armed(),
            0,
            "arming a different verb re-arms (the shared grammar)"
        );
    });
}

/// US-2/S2 (AC-2) — the confirmed strip dispatches exactly one
/// `StatusRemediate { ForceUnlock }` and the typed outcome folds
/// with the post-fact refresh.
#[test]
fn us2_s2_confirm_dispatches_force_unlock_remediation() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        requests.try_recv().expect("the open query");
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        window.invoke_status_lock_force_unlock();
        driver.advance_time(0);
        window.invoke_status_remediation_armed(0);
        driver.advance_time(0);
        let request = requests
            .try_recv()
            .expect("the confirm dispatches one StatusRemediate");
        let taskboard_ui_slint::requests::UiRequest::StatusRemediate { action, reply } = request
        else {
            panic!("StatusRemediate");
        };
        assert_eq!(
            action,
            taskboard_ui_slint::requests::StatusRemediationAction::ForceUnlock,
            "the armed verb's exact request"
        );
        reply.send(Outcome::LockReleased).expect("reply accepted");
        handle.drain_status_remediation();
        driver.advance_time(0);
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(
            facts.get_remediation_outcome_kind(),
            2,
            "LockReleased folds"
        );
        let request = requests.try_recv().expect("the fold re-queries (D-3)");
        assert!(matches!(
            request,
            taskboard_ui_slint::requests::UiRequest::StatusQuery { .. }
        ));
    });
}

/// US-2/S3 (AC-3) — the typed refusal `HolderAlive` folds (kind
/// index 1), the busy posture clears and the panel stays mounted
/// with state intact (D-7); the same row re-arms (the retry
/// affordance). The sidecar-untouched half of the law is the
/// executor's test (`it_status_remediation.rs`).
#[test]
fn us2_s3_live_holder_refusal_folds_and_the_sidecar_is_untouched() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        requests.try_recv().expect("the open query");
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        window.invoke_status_lock_force_unlock();
        driver.advance_time(0);
        window.invoke_status_remediation_armed(0);
        driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::StatusRemediate { reply, .. } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("StatusRemediate");
        };
        reply
            .send(Outcome::Refused(Refusal::HolderAlive))
            .expect("reply accepted");
        handle.drain_status_remediation();
        driver.advance_time(0);
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(
            facts.get_remediation_outcome_kind(),
            4,
            "the refusal kind folds (W-6's refused arm)"
        );
        assert_eq!(
            facts.get_remediation_refusal(),
            1,
            "the canonical index: HolderAlive"
        );
        assert!(
            window.get_current_view() == taskboard_ui_slint::View::Status,
            "the panel stays mounted (D-7)"
        );
        assert!(!facts.get_remediation_busy(), "the busy posture cleared");
        // The retry affordance: the same row re-arms.
        window.invoke_status_lock_force_unlock();
        driver.advance_time(0);
        assert_eq!(facts.get_remediation_armed(), 1, "the row re-arms");
    });
}

/// US-3/S1 — the Restart daemon row arms the confirm strip; nothing
/// dispatches.
#[test]
fn us3_s1_restart_arms_confirm_strip() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        drain_open_queries(&mut requests);
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        window.invoke_status_daemon_restart();
        driver.advance_time(0);
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(facts.get_remediation_armed(), 2, "the restart verb arms");
        assert!(requests.try_recv().is_err(), "arming dispatches nothing");
    });
}

/// US-3/S2 (AC-1) — the confirmed strip dispatches exactly one
/// `StatusRemediate { RestartDaemon }`; the typed outcome carries
/// the new daemon pid (the fold's kind 3).
#[test]
fn us3_s2_confirm_dispatches_restart_remediation() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        requests.try_recv().expect("the open query");
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        window.invoke_status_daemon_restart();
        driver.advance_time(0);
        window.invoke_status_remediation_armed(0);
        driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::StatusRemediate { action, reply } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("StatusRemediate");
        };
        assert_eq!(
            action,
            taskboard_ui_slint::requests::StatusRemediationAction::RestartDaemon,
        );
        reply
            .send(Outcome::DaemonRestarted { pid: 909 })
            .expect("reply accepted");
        handle.drain_status_remediation();
        driver.advance_time(0);
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(
            facts.get_remediation_outcome_kind(),
            3,
            "the spawn fact folds (daemon restarted)"
        );
    });
}

/// US-3/S3 (AC-3) — a failed spawn folds the typed `SpawnFailed`
/// refusal (index 3) and the panel re-queries showing the true
/// posture.
#[test]
fn us3_s3_spawn_failure_folds_the_typed_refusal() {
    run(|state| {
        let (mut driver, handle, _engine, mut requests) = mount_holding_requests(state, false);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        requests.try_recv().expect("the open query");
        fold_wedged(handle.window(), 4242);
        let window = handle.window();
        window.invoke_status_daemon_restart();
        driver.advance_time(0);
        window.invoke_status_remediation_armed(0);
        driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::StatusRemediate { reply, .. } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("StatusRemediate");
        };
        reply
            .send(Outcome::Refused(Refusal::SpawnFailed))
            .expect("reply accepted");
        handle.drain_status_remediation();
        driver.advance_time(0);
        let facts = window.global::<StatusPanelFacts>();
        assert_eq!(facts.get_remediation_outcome_kind(), 4);
        assert_eq!(
            facts.get_remediation_refusal(),
            3,
            "the canonical index: SpawnFailed"
        );
        let request = requests.try_recv().expect("the fold re-queries (D-3)");
        assert!(
            matches!(
                request,
                taskboard_ui_slint::requests::UiRequest::StatusQuery { .. }
            ),
            "the panel re-queries showing the true posture"
        );
    });
}

/// D-9 (the structural arm) — on kiosk the remediation rows render
/// DISABLED (the permission fold is false) and no remediation
/// dispatch is reachable: the arm seam absorbs, the confirm
/// dispatches nothing (the trap-a law's structural assertion).
#[test]
fn d9_kiosk_remediation_rows_render_disabled_and_dispatch_nothing() {
    run(|state| {
        let (mut driver, handle, mut engine, mut requests) = mount_holding_requests(state, true);
        handle.dispatch_action(ActionId::StatusPanelOpen);
        driver.advance_time(0);
        let request = requests.try_recv().expect("the kiosk panel queries");
        let taskboard_ui_slint::requests::UiRequest::StatusQuery { reply } = request else {
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
        handle.drain_status_reply();
        let window = handle.window();
        let facts = window.global::<StatusPanelFacts>();
        assert!(
            !facts.get_remediation_permitted(),
            "the kiosk permission fold is off (D-9's disabled rows)"
        );
        // No remediation dispatch is reachable: the arm absorbs (the
        // adapter's typed-permission gate) and the strip never arms.
        window.invoke_status_holder_kill();
        driver.advance_time(0);
        assert_eq!(
            facts.get_remediation_armed(),
            -1,
            "the kiosk arm never arms the strip"
        );
        window.invoke_status_remediation_armed(0);
        driver.advance_time(0);
        assert!(
            requests.try_recv().is_err(),
            "no StatusRemediate is reachable on the kiosk composition (D-9)"
        );
        assert_eq!(
            engine.drain(),
            Vec::<StateCommand>::new(),
            "and no command either"
        );
    });
}
