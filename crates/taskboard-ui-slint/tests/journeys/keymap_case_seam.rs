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
//! names. (#1172) The former private raw-ArcSwap mount preamble is
//! the shared B4a harness now — one mount, no local wiring.

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;
use support::harness::run_journey;

use taskboard_ui_slint::View;
use taskboard_ui_slint::keymap::Stroke;

/// A delivered Ctrl-chorded letter press: the glyph a real keyboard
/// sends plus the modifier flags as Slint carries them.
fn delivered(glyph: char, ctrl: bool, shift: bool) -> Stroke {
    Stroke::from_event(glyph, ctrl, false, shift, false).expect("the delivery converts")
}

/// US-3/S1 — the shifted today-chord delivery (Ctrl+Shift+t, the
/// canonical spelling's physical press) opens Today from another
/// view. US-3/S2 — the shift-less Ctrl+t delivery is the distinct,
/// unbound binding: the view stays put.
#[test]
fn us3_s1_shifted_today_delivery_opens_today_and_us3_s2_bare_ctrl_t_does_not() {
    let (_fixture, state) = fixture::journey_state();
    run_journey(state, move |harness: &mut Harness| {
        // Leave Today the routed way (the board leg), so the
        // chord's effect is observable. (The harness mount's central
        // board escape already ran; the routed dispatch is kept so
        // the leg's posture is explicit.)
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);

        // S1: the shifted delivery resolves the re-spelled
        // `Ctrl+Shift+t` row.
        harness.driver.press_stroke(delivered('t', true, true));
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::Today);

        // Back to Board for the negative leg.
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);

        // S2: the shift-less delivery is `Ctrl+t` — unbound; the
        // view must not move.
        harness.driver.press_stroke(delivered('t', true, false));
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
    });
}
