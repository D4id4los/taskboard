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
use support::health::detached_health;

use std::path::{Path, PathBuf};
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

fn scale_settings(scale: f64) -> UiSettings {
    UiSettings {
        scale: taskboard_ui_slint::UiScale::new(scale).expect("valid"),
        ..UiSettings::default()
    }
}

/// The shared golden capture: mount the given app state under
/// `settings`, bind, let `frame` do the capture-specific steps (a
/// view escape, the kiosk rotation dwell), snapshot at `width` ×
/// HEIGHT, and regen-or-compare against `path`.
fn capture_golden(
    driver: &mut SlintDriver,
    state: Arc<ArcSwap<taskboard_domain::AppState>>,
    settings: UiSettings,
    frame: impl FnOnce(&mut SlintDriver, &taskboard_ui_slint::UiHandle),
    width: u32,
    path: &Path,
    regen: bool,
) {
    let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = taskboard_ui_slint::mount(
        state,
        dummy_signals(),
        dummy_system(),
        commands,
        requests_tx,
        settings,
        detached_health(),
    )
    .expect("mount succeeds on the testing backend");
    driver.bind(&handle);
    frame(driver, &handle);
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
         must capture at the exact window size ({path})",
        path = path.display(),
    );

    if regen {
        let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, bytes).expect("writes");
        return;
    }
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("golden {} readable: {error}", path.display()));
    let golden: Rgba8Image =
        golden_compare::decode_png(&bytes).expect("the committed golden decodes as RGBA8");
    golden_compare::matches(&golden, &snapshot)
        .unwrap_or_else(|error| panic!("golden {} drifted: {error}", path.display()));
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
    capture_golden(
        driver,
        state,
        scale_settings(scale),
        // The board goldens capture the board view: escape the desktop
        // Today default (chunk 10 D2) before the first frame.
        |_driver, handle| {
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
        },
        width,
        &golden_path(scale, width),
        regen,
    );
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
    capture_golden(
        driver,
        state,
        scale_settings(scale),
        // The today goldens capture the TODAY view: no escape — the
        // desktop default is the capture subject.
        |_, _| {},
        width,
        &today_golden_path(scale, width),
        regen,
    );
}

// -- K13: the kiosk ambient rotation's two mode goldens ------------------

use std::time::Duration;

use taskboard_ui_slint::view_state::{KioskMode, KioskRotation};

/// The kiosk rotation hand-over for the goldens: a 1 s dwell so the
/// today capture needs one clock step past the flip.
fn kiosk_rotation_settings() -> UiSettings {
    UiSettings {
        kiosk: true,
        kiosk_rotation: Some(
            KioskRotation::new(
                vec![KioskMode::Board, KioskMode::Today],
                Duration::from_secs(1),
            )
            .expect("two distinct modes mint"),
        ),
        ..UiSettings::default()
    }
}

/// The one matrix cell each kiosk golden captures (the mid
/// breakpoint, a scale from the existing matrix).
const KIOSK_SCALE: f64 = 0.5;
const KIOSK_WIDTH: u32 = 1000;

fn kiosk_golden_path(mode: &str, scale: f64, width: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/kiosk-{mode}-{scale:.1}-{width}.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn capture_kiosk(driver: &mut SlintDriver, mode: &str, regen: bool) {
    // The board mode captures the clean board fixture; the today mode
    // captures the overdue-stamped fixture (the desktop today goldens'
    // phrase-stable subject) — its pinned-board degeneration is the
    // golden's point.
    let state = if mode == "today" {
        Arc::new(ArcSwap::from_pointee(fixture::today_overdue_state().1))
    } else {
        Arc::new(ArcSwap::from_pointee(fixture::golden_state().1))
    };
    capture_golden(
        driver,
        state,
        kiosk_rotation_settings(),
        |driver, _| {
            if mode == "today" {
                // One dwell step: the timer's flip is the only way a mode
                // changes — the capture documents the rotation's output, not
                // a navigation action.
                driver.advance_time(1_000);
            }
        },
        KIOSK_WIDTH,
        &kiosk_golden_path(mode, KIOSK_SCALE, KIOSK_WIDTH),
        regen,
    );
}

/// The kiosk board-mode golden (K13): the appliance's modes[0] — the
/// pinned board through the kiosk mount, one matrix cell.
#[test]
fn the_kiosk_board_golden_holds() {
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            capture_kiosk(driver, "board", regen);
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// The kiosk today-mode golden (K13): captured after one forced
/// timer flip — the passive upcoming reading of the pinned board.
#[test]
fn the_kiosk_today_golden_holds() {
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            capture_kiosk(driver, "today", regen);
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

// -- r4 chunk 3: the notification-surface golden (D11) ------------------

/// The notification surface's icon row golden (r4 D11, plan §6): one
/// delivered task notification (the can-done anchor row) plus one
/// delivered alarm (the can-cancel row) through the existing fixture
/// helpers. The row then renders dismiss(circle-x) ∥ done(check) ∥
/// play ∥ snooze ∥ mute ∥ remove — the dismiss/done distinctness
/// evidence (F1). Scale 3.0 for glyph detail, the matrix's mid
/// breakpoint. Phrase stability rides the today fixture's protocol:
/// occurrences are stamped −30 min off fixture-build `now`, so every
/// run renders the identical "fired 30m ago" captions.
#[test]
fn the_notification_golden_holds() {
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let (fixture, mut state) = fixture::golden_state();
            // The task's due (and with it the at-due rule's whole
            // firing — the row's occurrence IS the effective
            // occurrence) is midnight-aligned yesterday: due phrases
            // render date-only (no wall-clock suffix), and the
            // notification row's elapsed sits in [24 h, 48 h) all day,
            // so "fired 1d ago" never crosses a bucket mid-day.
            let due = (chrono::Utc::now() - chrono::TimeDelta::days(1))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc();
            fixture::deliver_task_notification(&mut state, fixture.task0, due);
            // The alarm carries no due phrase; its −30 min stamp rides
            // the today fixture's minute-bucket stability.
            let fired = chrono::Utc::now() - chrono::TimeDelta::minutes(30);
            let alarm = fixture::add_alarm(&mut state, "alarm-0", fired);
            fixture::deliver_alarm(&mut state, alarm, fired);
            capture_golden(
                driver,
                Arc::new(ArcSwap::from_pointee(state)),
                scale_settings(3.0),
                |_, _| {},
                1000,
                &PathBuf::from(format!(
                    "{}/tests/goldens/notification-3.0-1000.png",
                    env!("CARGO_MANIFEST_DIR")
                )),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}
