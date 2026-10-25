// SPDX-License-Identifier: AGPL-3.0-only

//! The golden visual tier (design §10, chunk plan §7/D7): the board
//! view at the scale extremes × breakpoints, captured with the
//! software renderer against bundled fonts, compared behind the
//! renderer-presence assertion.
//!
//! # Matrix
//!
//! `Theme.base-scale` ∈ {0.5, 3.0} (the config clamp's bounds, riding
//! `UiSettings.scale` → the mount's global setter) × breakpoint ∈
//! {600 compact, 1000 regular, 1500 expansive} (riding the driver's
//! window-size push). Height is fixed at 900 logical px. Six goldens
//! live in `tests/goldens/board-<scale>-<breakpoint>.png`.
//!
//! # Determinism
//!
//! The fixture is the clean two-stack board (no overdue banner — its
//! labels derive from wall-clock relative time and would flake across
//! hour boundaries; banner structure stays covered by the §6
//! component tests). Titles are ASCII-only (system-font fallback can
//! never differ); fonts are the bundled Inter/JetBrains Mono
//! registered by `mount`; the mock clock means no animation or timer
//! state advances between capture and compare; the software
//! rasterizer is CPU-deterministic for the pinned Slint version.
//!
//! # Comparison
//!
//! Exact dimensions, then the box-averaged luma-cell grid of
//! `support::golden_compare` (32×24 cells, per-cell budget 8/255 —
//! the calibration and tolerance budget are recorded there). Drift
//! within tolerance logs a warning; beyond it fails.
//!
//! # Regeneration — an explicit, reviewed step
//!
//! `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
//! --test golden_visuals` rewrites the PNGs and *fails* the run
//! afterwards, so a regen run can never be mistaken for a pass. CI
//! never sets the variable: it compares only. The PR's PNG diff **is**
//! the review artefact (GitHub renders the images).

mod support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fixture;
use support::golden_compare::{self, Rgba8Image};

use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use taskboard_ui_slint::UiSettings;

/// The breakpoint widths of the matrix (logical px).
const BREAKPOINTS: [u32; 3] = [600, 1000, 1500];
/// The scale extremes of the matrix (`UiSettings.scale`).
const SCALES: [f64; 2] = [0.5, 3.0];
/// The fixed window height (logical px).
const HEIGHT: u32 = 900;

fn golden_path(scale: f64, width: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/board-{scale:.1}-{width}.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn regen_requested() -> bool {
    std::env::var_os("TASKBOARD_REGEN_GOLDENS").is_some_and(|v| v == "1")
}

#[test]
fn the_board_goldens_hold_across_the_scale_breakpoint_matrix() {
    let regen = regen_requested();
    for scale in SCALES {
        for width in BREAKPOINTS {
            support::driver::run(
                DriverOptions {
                    headless_renderer: true,
                },
                move |driver: &mut SlintDriver| {
                    capture_and_compare(driver, scale, width, regen);
                },
            );
        }
    }
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

fn capture_and_compare(driver: &mut SlintDriver, scale: f64, width: u32, regen: bool) {
    let state = Arc::new(ArcSwap::from_pointee(fixture::golden_state().1));
    let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = taskboard_ui_slint::mount(
        state,
        dummy_signals(),
        dummy_system(),
        commands,
        requests_tx,
        UiSettings {
            scale: taskboard_ui_slint::UiScale::new(scale).expect("valid"),
            ..UiSettings::default()
        },
    )
    .expect("mount succeeds on the testing backend");
    // The board goldens capture the board view: escape the desktop
    // Today default (chunk 10 D2) before the first frame.
    handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
    driver.bind(&handle);
    driver.advance_time(0); // instantiate conditionals
    driver.set_window_size(width, HEIGHT);

    // The renderer-presence assertion FIRST (§10/stress §3.5's
    // silent-fallback warning): the snapshot must exist and match the
    // set window size exactly — the software renderer's rasterizer is
    // the contract, silence is not success.
    let snapshot = driver.snapshot();
    assert_eq!(
        (snapshot.width, snapshot.height),
        (width, HEIGHT),
        "renderer-presence contract: the headless software renderer \
         must capture at the exact window size"
    );

    let path = golden_path(scale, width);
    if regen {
        let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, bytes).expect("writes");
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("golden {} readable: {error}", path.display()));
    let golden: Rgba8Image =
        golden_compare::decode_png(&bytes).expect("the committed golden decodes as RGBA8");
    golden_compare::matches(&golden, &snapshot)
        .unwrap_or_else(|error| panic!("golden {} drifted: {error}", path.display()));
}

fn dummy_signals() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The today-view golden matrix (chunk 10 D13): the same scale ×
/// breakpoint extremes as the board's, through the same mount path
/// (the bare-window model push does not produce the mounted render).
/// Phrase stability: the fixture stamps its rows relative to the
/// fixture-build `now` (−30 min), so every run renders the identical
/// "30m overdue" captions — a run never lives long enough to cross
/// the minute bucket.
#[test]
fn the_today_goldens_hold_across_the_scale_breakpoint_matrix() {
    let regen = regen_requested();
    for scale in SCALES {
        for width in BREAKPOINTS {
            support::driver::run(
                DriverOptions {
                    headless_renderer: true,
                },
                move |driver| {
                    capture_today(driver, scale, width, regen);
                },
            );
        }
    }
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

fn today_golden_path(scale: f64, width: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/today-{scale:.1}-{width}.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn capture_today(driver: &mut SlintDriver, scale: f64, width: u32, regen: bool) {
    let state = Arc::new(ArcSwap::from_pointee(fixture::today_overdue_state().1));
    let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = taskboard_ui_slint::mount(
        state,
        dummy_signals(),
        dummy_system(),
        commands,
        requests_tx,
        UiSettings {
            scale: taskboard_ui_slint::UiScale::new(scale).expect("valid"),
            ..UiSettings::default()
        },
    )
    .expect("mount succeeds on the testing backend");
    // The today goldens capture the TODAY view: no escape — the
    // desktop default is the capture subject.
    driver.bind(&handle);
    driver.advance_time(0); // instantiate conditionals
    driver.set_window_size(width, HEIGHT);

    let snapshot = driver.snapshot();
    assert_eq!(
        (snapshot.width, snapshot.height),
        (width, HEIGHT),
        "renderer-presence contract at the today goldens"
    );

    let path = today_golden_path(scale, width);
    if regen {
        let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, bytes).expect("writes");
        return;
    }
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("golden {} readable: {error}", path.display()));
    let golden: Rgba8Image =
        golden_compare::decode_png(&bytes).expect("the committed golden decodes as RGBA8");
    golden_compare::matches(&golden, &snapshot)
        .unwrap_or_else(|error| panic!("golden {} drifted: {error}", path.display()));
}
