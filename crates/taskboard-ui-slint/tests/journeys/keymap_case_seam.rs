// SPDX-License-Identifier: AGPL-3.0-only

//! The chorded-letter case seam at the window boundary (J3 US-3,
//! W-1; issue #855): the app's delivery path is the adapter's
//! `to_stroke` — a composed glyph plus modifiers — so these journeys
//! feed real `from_event` deliveries (exactly the inputs a Slint key
//! event carries) through the harness strokes and assert the view
//! state. Terminal state is the oracle; no text assertions (§5).
//!
//! The seam-level identity and delivery contracts live in
//! `keymap::stroke` / `keymap` (the `us3_s1`/`us3_s2` token tests);
//! this file is the window-level ride-along the W-1 wiring row
//! names.

use crate::support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::health::detached_health;

use arc_swap::ArcSwap;
use std::sync::Arc;
use taskboard_ui_slint::View;
use taskboard_ui_slint::keymap::Stroke;

/// A delivered Ctrl-chorded letter press: the glyph a real keyboard
/// sends plus the modifier flags as Slint carries them.
fn delivered(glyph: char, ctrl: bool, shift: bool) -> Stroke {
    Stroke::from_event(glyph, ctrl, false, shift, false).expect("the delivery converts")
}

/// Mounts `state` on the driver thread (the journeys' mount shape,
/// trimmed to what the view assertions need).
fn mount(
    driver: &mut SlintDriver,
    state: taskboard_domain::AppState,
) -> taskboard_ui_slint::UiHandle {
    let state = Arc::new(ArcSwap::from_pointee(state));
    let (commands, _engine) = FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = taskboard_ui_slint::mount(
        state,
        {
            let (_tx, rx) = tokio::sync::broadcast::channel(4);
            rx
        },
        {
            let (_tx, rx) = tokio::sync::broadcast::channel(4);
            rx
        },
        commands,
        requests_tx,
        taskboard_ui_slint::UiSettings::default(),
        detached_health(),
        None,
    )
    .expect("mount succeeds on the testing backend");
    driver.bind(&handle);
    handle
}

/// US-3/S1 — the shifted today-chord delivery (Ctrl+Shift+t, the
/// canonical spelling's physical press) opens Today from another
/// view. US-3/S2 — the shift-less Ctrl+t delivery is the distinct,
/// unbound binding: the view stays put.
#[test]
fn us3_s1_shifted_today_delivery_opens_today_and_us3_s2_bare_ctrl_t_does_not() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let handle = mount(driver, state);

            // Leave Today the routed way (the board leg), so the
            // chord's effect is observable.
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
            driver.advance_time(0);
            assert_eq!(handle.window().get_current_view(), View::Board);

            // S1: the shifted delivery resolves the re-spelled
            // `Ctrl+Shift+t` row.
            driver.press_stroke(delivered('t', true, true));
            driver.advance_time(0);
            assert_eq!(handle.window().get_current_view(), View::Today);

            // Back to Board for the negative leg.
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
            driver.advance_time(0);
            assert_eq!(handle.window().get_current_view(), View::Board);

            // S2: the shift-less delivery is `Ctrl+t` — unbound; the
            // view must not move.
            driver.press_stroke(delivered('t', true, false));
            driver.advance_time(0);
            assert_eq!(handle.window().get_current_view(), View::Board);
        },
    );
}
