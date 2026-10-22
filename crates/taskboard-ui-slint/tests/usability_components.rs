// SPDX-License-Identifier: AGPL-3.0-only

//! The usability-pass component tier (plan §3 D3/D4/D6, ledger G3/G4/
//! G6/G7): geometry, sizing, and tooltip-state proofs over the real
//! mounted window, driven through the shared `tests/support` harness.
//! Pixel assertions scan for uniform runs of known token colours —
//! no element-geometry API exists on the testing backend (plan §1.7).

mod support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fixture;
use support::golden_compare::Rgba8Image;

use std::sync::Arc;

use arc_swap::ArcSwap;
use taskboard_ui_slint::UiSettings;

/// The populated board fixture, mounted at `scale` and sized to
/// `(width, height)` — the shared arrange step of the geometry proofs.
fn mount_populated(
    driver: &mut SlintDriver,
    scale: f64,
    width: u32,
    height: u32,
) -> taskboard_ui_slint::UiHandle {
    let state = Arc::new(ArcSwap::from_pointee(fixture::golden_state().1));
    let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let handle = taskboard_ui_slint::mount(
        state,
        dummy_signals(),
        dummy_system(),
        commands,
        UiSettings {
            scale,
            ..UiSettings::default()
        },
    )
    .expect("mount succeeds on the testing backend");
    driver.bind(&handle);
    driver.advance_time(0); // instantiate conditionals
    driver.set_window_size(width, height);
    handle
}

fn dummy_signals() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The longest horizontal run of exactly `rgb` in the image's top
/// `strip_height` pixel rows. The hamburger's three bars are the only
/// long monochrome runs of the icon colour in the toolbar strip
/// (plan §1.4); glyph strokes stay far below the contract width.
fn longest_run_in_strip(image: &Rgba8Image, rgb: [u8; 3], strip_height: u32) -> u32 {
    let mut best = 0u32;
    for y in 0..strip_height.min(image.height) {
        let mut run = 0u32;
        for x in 0..image.width {
            let i = (y as usize * image.width as usize + x as usize) * 4;
            let matches = image.data[i] == rgb[0]
                && image.data[i + 1] == rgb[1]
                && image.data[i + 2] == rgb[2];
            if matches {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
    }
    best
}

/// G3 — the hamburger geometry contract (plan D4): the icon bars are
/// exactly `space-6 − 2×space-2` wide (±1 px rounding) at the kiosk
/// panel's raw and logical geometries, the golden-matrix base, and
/// the scale extremes. The committed goldens contain the stretched
/// bar (≈447 px of the row at 0.5/1000), so this fails before the
/// primitive's `max-width` pin — a production bug, not a test bug.
#[test]
fn the_hamburger_bar_holds_its_token_derived_square_at_every_geometry() {
    // (scale, width, height): the kiosk panel raw and logical, the
    // golden-matrix base, and the scale extremes.
    let geometries = [
        (1.0, 1024, 600),
        (1.0, 819, 480), // winit's 1.25 guess — the reported failing geometry
        (1.0, 1000, 900),
        (0.5, 1000, 900),
        (3.0, 1000, 900),
    ];
    for (scale, width, height) in geometries {
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |driver: &mut SlintDriver| {
                let _window = mount_populated(driver, scale, width, height);
                let snapshot = driver.snapshot();
                // Theme.text-secondary #a6abbd, the neutral icon colour;
                // the toolbar strip is Theme.space-8 tall.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let strip = (64.0 * scale).ceil() as u32;
                let run = longest_run_in_strip(&snapshot, [0xa6, 0xab, 0xbd], strip);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let expected = (16.0 * scale).round() as i64; // space-6 − 2×space-2
                let diff = (i64::from(run) - expected).abs();
                assert!(
                    diff <= 1,
                    "scale {scale} @ {width}×{height}: hamburger bar is {run} px, expected {expected} ±1"
                );
            },
        );
    }
}
