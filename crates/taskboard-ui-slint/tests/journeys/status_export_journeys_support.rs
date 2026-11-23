// SPDX-License-Identifier: AGPL-3.0-only

//! The status/export journey pair's shared preamble (the D2 chunk's
//! two evidence suites): the broadcast dummies, the held-request-lane
//! mount over caller-built `UiSettings`, and the standard `run`
//! wrapper over the golden fixture. Included once per
//! `journey_suites` binary; both suites route through it so the
//! mount shape is one construction site.

use crate::support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::focus_journey::JourneyHarness;
use support::health::detached_health;

use arc_swap::ArcSwap;
use std::sync::Arc;
use taskboard_domain::AppState;
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;

pub fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

pub fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The journey harness with the request lane *held*: the test plays
/// the app half. The caller builds the `UiSettings` (the kiosk flag,
/// the export defaults — the suites' only divergence).
pub fn mount_holding_settings(
    state: AppState,
    settings: UiSettings,
) -> (
    JourneyHarness,
    tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
) {
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

pub fn run(body: impl FnOnce(AppState) + Send + 'static) {
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
