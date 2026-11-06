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

/// The golden tier's zone pin (r4 D28): due/scheduled phrases now
/// render the **device-local** wall-clock, and a golden PNG is
/// compared on machines in any zone — the capture pins the process to
/// UTC so the committed pixels are machine-independent (the timezone
/// fixture convention; cargo-nextest runs each test as its own
/// process, so the set cannot race the lib's lazy `Local`).
fn pin_utc_zone() {
    unsafe { std::env::set_var("TZ", "UTC") };
}

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
        // The render pass's content clock frozen at the binary's pin
        // (`docs/testing_strategy.org` §6.2): every caller is a
        // clock-exposed golden, so the chip's minute phrase is
        // byte-stable by construction.
        clock: Some(fixture::PINNED_NOW),
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
    pin_utc_zone();
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
    let state = Arc::new(ArcSwap::from_pointee(
        fixture::golden_state_at(fixture::PINNED_NOW).1,
    ));
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

/// The focused closed cell's golden (r4 chunk 6, plan §6.A): the
/// due cell focused via the control walk pins the D18 ring visual —
/// the walk's active element is visibly indicated at a closed cell.
#[test]
fn the_focused_cell_golden_pins_the_ring() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let path = PathBuf::from(format!(
                "{}/tests/goldens/task-view-focus-cell-1.0-900.png",
                env!("CARGO_MANIFEST_DIR")
            ));
            let state = Arc::new(ArcSwap::from_pointee(
                fixture::golden_state_at(fixture::PINNED_NOW).1,
            ));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = taskboard_ui_slint::mount(
                state,
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                UiSettings {
                    clock: Some(fixture::PINNED_NOW),
                    ..UiSettings::default()
                },
                detached_health(),
            )
            .expect("mount succeeds on the testing backend");
            // The journeys' walk idiom into the detail, then five walk
            // steps: back, done, title, tracking, due.
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFirstItem);
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::TaskEdit);
            driver.bind(&handle);
            driver.advance_time(0);
            driver.set_window_size(TASK_VIEW_WIDTH, 900);
            for _ in 0..5 {
                driver.press_stroke(
                    taskboard_ui_slint::keymap::stroke::parse("Tab").expect("tab parses"),
                );
                driver.advance_time(1);
            }
            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (TASK_VIEW_WIDTH, 900),
                "renderer-presence contract at the focused-cell golden"
            );
            if regen {
                let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
                std::fs::write(&path, bytes).expect("writes");
                return;
            }
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|error| panic!("golden {} readable: {error}", path.display()));
            let golden: Rgba8Image =
                golden_compare::decode_png(&bytes).expect("the committed golden decodes as RGBA8");
            golden_compare::matches(&golden, &snapshot)
                .unwrap_or_else(|error| panic!("golden {} drifted: {error}", path.display()));
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
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
/// pinned render clock (`fixture::PINNED_NOW`) and the pass's clock
/// is frozen at the same instant (`scale_settings`), so every run
/// renders the identical overdue captions — byte-stable by
/// construction, not by run brevity.
#[test]
fn the_today_goldens_hold_across_the_scale_breakpoint_matrix() {
    pin_utc_zone();
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
    let state = Arc::new(ArcSwap::from_pointee(
        fixture::today_overdue_state_at(fixture::PINNED_NOW).1,
    ));
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
        // The render pass's content clock frozen at the binary's pin
        // (`docs/testing_strategy.org` §6.2) — same capture class as
        // `scale_settings`; cadence stays on the mock clock.
        clock: Some(fixture::PINNED_NOW),
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
        Arc::new(ArcSwap::from_pointee(
            fixture::today_overdue_state_at(fixture::PINNED_NOW).1,
        ))
    } else {
        Arc::new(ArcSwap::from_pointee(
            fixture::golden_state_at(fixture::PINNED_NOW).1,
        ))
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
    pin_utc_zone();
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
    pin_utc_zone();
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
/// breakpoint. Phrase stability rides the pinned render clock:
/// occurrences are stamped −30 min off `fixture::PINNED_NOW` and the
/// pass's clock is frozen at the same instant, so every run renders
/// the identical "fired 30m ago" captions — byte-stable by
/// construction.
#[test]
fn the_notification_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let (fixture, mut state) = fixture::golden_state_at(fixture::PINNED_NOW);
            // The task's due (and with it the at-due rule's whole
            // firing — the row's occurrence IS the effective
            // occurrence) is midnight-aligned yesterday at the pin:
            // due phrases render date-only (no wall-clock suffix),
            // and the notification row's elapsed sits in [24 h, 48 h)
            // at the frozen now, so "fired 1d ago" never crosses a
            // bucket.
            let due = (fixture::PINNED_NOW - chrono::TimeDelta::try_days(1).unwrap())
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc();
            fixture::deliver_task_notification(&mut state, fixture.task0, due);
            // The alarm carries no due phrase; its −30 min stamp
            // derives from the same pinned instant the pass renders
            // with.
            let fired = fixture::PINNED_NOW - chrono::TimeDelta::try_minutes(30).unwrap();
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

// -- r4 chunk 5 (G1): the task-view goldens -------------------------------

/// The task-view matrix's scale extremes plus the mid scale (D16's
/// Tier-4 row-shape checks run at 0.5/1.5/3.0).
const TASK_VIEW_SCALES: [f64; 3] = [0.5, 1.5, 3.0];
/// The two window heights of the matrix: tall (the description region
/// visibly absorbs the leftover) and short (the region floors at its
/// content height and the page scrolls).
const TASK_VIEW_HEIGHTS: [u32; 2] = [900, 500];
/// The fixed width (the regular breakpoint's interior).
const TASK_VIEW_WIDTH: u32 = 1000;

fn task_view_golden_path(scale: f64, height: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/task-view-{scale:.1}-{height}.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// The task-view golden matrix (r4 chunk 5, G1): scale × window
/// height over the walked-into task detail. Pins one content-height
/// header row, the summary row, and the description region as the
/// flexible element. Phrase stability: the fixture's open entry is
/// stamped against the pinned render clock (`fixture::PINNED_NOW`)
/// and the pass's clock is frozen at the same instant, so the
/// second-precise total phrase is byte-stable by construction (the
/// today goldens' pin argument, tighter).
#[test]
fn the_task_view_goldens_hold_across_the_scale_height_matrix() {
    pin_utc_zone();
    let regen = regen_requested();
    for scale in TASK_VIEW_SCALES {
        for height in TASK_VIEW_HEIGHTS {
            support::driver::run(
                DriverOptions {
                    headless_renderer: true,
                },
                move |driver| {
                    capture_task_view(driver, scale, height, regen);
                },
            );
        }
    }
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

fn capture_task_view(driver: &mut SlintDriver, scale: f64, height: u32, regen: bool) {
    let state = Arc::new(ArcSwap::from_pointee(
        fixture::golden_state_at(fixture::PINNED_NOW).1,
    ));
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
            clock: Some(fixture::PINNED_NOW),
            ..UiSettings::default()
        },
        detached_health(),
    )
    .expect("mount succeeds on the testing backend");
    // Escape the desktop Today default, then walk into the first
    // task's detail (the journeys' walk idiom).
    handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
    handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFirstItem);
    handle.dispatch_action(taskboard_ui_slint::actions::ActionId::TaskEdit);
    driver.bind(&handle);
    driver.advance_time(0); // instantiate conditionals
    driver.set_window_size(TASK_VIEW_WIDTH, height);

    let snapshot = driver.snapshot();
    assert_eq!(
        (snapshot.width, snapshot.height),
        (TASK_VIEW_WIDTH, height),
        "renderer-presence contract at the task-view goldens"
    );

    let path = task_view_golden_path(scale, height);
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

// -- #496: the concentration overlay's golden ----------------------------

/// The concentration overlay's golden (issue #496): the open panel at
/// the mid scale/breakpoint — phase caption, the switcher pool, the
/// quick-note field, and the stop affordance, over the board fixture.
///
/// The captured phase is a **running work block**: the board
/// fixture's open entry (12 min old at `fixture::PINNED_NOW`) is the
/// newest-open pick, so the overlay renders the Work caption and the
/// `mm:ss` countdown to `start + 25 min` — `13:00` at the pinned
/// render clock. The seeds and the mount share the one instant
/// (`docs/testing_strategy.org` §6.2), so the second-precise digits
/// are byte-stable by construction; the zone pin matches the sibling
/// goldens.
#[test]
fn the_focus_overlay_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let mut golden = fixture::golden_state_at(fixture::PINNED_NOW).1;
            // The timings ride the published settings (round-5 chunk
            // 5), not the hand-over.
            golden.settings.focus_enabled = true;
            golden.settings.focus = Some(
                taskboard_domain::focus::FocusTiming::try_new(
                    chrono::TimeDelta::minutes(25),
                    chrono::TimeDelta::minutes(5),
                    chrono::TimeDelta::minutes(15),
                    4,
                )
                .expect("positive timings"),
            );
            let state = Arc::new(ArcSwap::from_pointee(golden));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = taskboard_ui_slint::mount(
                state,
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                UiSettings {
                    clock: Some(fixture::PINNED_NOW),
                    ..UiSettings::default()
                },
                detached_health(),
            )
            .expect("mount succeeds on the testing backend");
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::FocusModeToggle);
            driver.bind(&handle);
            driver.advance_time(0); // instantiate conditionals
            driver.set_window_size(1000, HEIGHT);

            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1000, HEIGHT),
                "renderer-presence contract at the overlay golden"
            );

            let path = PathBuf::from(format!(
                "{}/tests/goldens/focus-overlay-1.0-1000.png",
                env!("CARGO_MANIFEST_DIR")
            ));
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
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

// -- r4 chunk 10: the entries-overlay golden -----------------------------

/// The time-entries overlay's golden (r4 chunk 10, D35): the open
/// panel over the task view — the Work-only header total, the
/// newest-first rows with the `+1d` midnight-crossing mark, the armed
/// delete affordances, and the footer add affordance.
///
/// The fixture's entries are **closed spans at fixed absolute
/// instants**: every rendered cell (dates, `HH:MM` walls, spent
/// phrases, the total) is wall-clock independent. A running row is
/// deliberately absent — its spent phrase is second-precise, the
/// #496 golden's recorded wall-second exclusion; the running mark's
/// structure stays with the pure-layer and journey pins.
#[test]
fn the_track_entries_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let (fx, mut state) = fixture::golden_state_at(fixture::PINNED_NOW);
            // Two closed spans at fixed absolute instants (UTC-pinned
            // zone): an evening span crossing local midnight (the
            // `+1d` mark, newest-first) over an afternoon span.
            let mint = |state: &mut taskboard_domain::AppState,
                        start: chrono::DateTime<chrono::Utc>,
                        end: chrono::DateTime<chrono::Utc>| {
                let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
                state.tracking.entries.insert(
                    id,
                    taskboard_domain::TimeEntry {
                        id,
                        task: fx.task0,
                        task_title: "task-0".to_owned(),
                        start,
                        end: Some(end),
                        created_at: start,
                        updated_at: start,
                        kind: taskboard_domain::tracking::TimeEntryKind::Work,
                    },
                );
            };
            let day = |h: u32, m: u32| {
                chrono::NaiveDate::from_ymd_opt(2026, 3, 15)
                    .expect("fixed day")
                    .and_hms_opt(h, m, 0)
                    .expect("fixed wall-clock")
                    .and_local_timezone(chrono::Utc)
                    .single()
                    .expect("resolvable fixed instant")
            };
            let next_day = |h: u32, m: u32| {
                chrono::NaiveDate::from_ymd_opt(2026, 3, 16)
                    .expect("next day")
                    .and_hms_opt(h, m, 0)
                    .expect("fixed wall-clock")
                    .and_local_timezone(chrono::Utc)
                    .single()
                    .expect("resolvable fixed instant")
            };
            mint(&mut state, day(22, 30), next_day(0, 30));
            mint(&mut state, day(13, 0), day(14, 30));
            let state = Arc::new(ArcSwap::from_pointee(state));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = taskboard_ui_slint::mount(
                state,
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                UiSettings {
                    clock: Some(fixture::PINNED_NOW),
                    ..UiSettings::default()
                },
                detached_health(),
            )
            .expect("mount succeeds on the testing backend");
            // The capture_task_view idiom: escape Today, walk into the
            // first task's detail, then route the overlay open (the
            // menu-reachable path).
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFirstItem);
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::TaskEdit);
            handle.dispatch_action(taskboard_ui_slint::actions::ActionId::TaskTrackEntries);
            driver.bind(&handle);
            driver.advance_time(0); // instantiate conditionals
            driver.set_window_size(1000, HEIGHT);

            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1000, HEIGHT),
                "renderer-presence contract at the overlay golden"
            );

            let path = PathBuf::from(format!(
                "{}/tests/goldens/track-entries-1.0-1000.png",
                env!("CARGO_MANIFEST_DIR")
            ));
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
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

// -- #214: the review-view goldens -----------------------------------------

/// The review surface's escape: the cold start is the board, and the
/// cycle wraps backwards through Today into Review (#214 D-1).
fn enter_review(handle: &taskboard_ui_slint::UiHandle) {
    handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavPrevView);
    handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavPrevView);
}

/// The empty-review golden (D-9 tier 1): the whole-view collapse over
/// the clean board fixture — no completed tasks anywhere, the ledger
/// and both sections folded into the `EmptyState`.
#[test]
fn the_review_empty_golden_holds() {
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let (_fixture, mut state) = fixture::golden_state();
            // D-S9's tier-1 extension (#623 slice A): tracked time
            // alone keeps the view standing, and `golden_state` seeds
            // a running timer — so the empty tier's fixture must be
            // genuinely tracking-free (the board chrome stays; the
            // toolbar's timer chip goes with the entry).
            state.tracking.entries.clear();
            let state = Arc::new(ArcSwap::from_pointee(state));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |_, handle| enter_review(handle),
                1000,
                &PathBuf::from(format!(
                    "{}/tests/goldens/review-empty-1.0-1000.png",
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

/// The populated-ledger golden (D-8): two compared rows over the
/// board fixture — a compared completion (estimate + tracked actual),
/// an untracked estimated completion, and an unestimated tracked one.
/// Phrase stability: closed entries and date-only completion phrases
/// are deterministic within a run (the second-granular phrases never
/// cross a second between build and snapshot).
///
/// Clock discipline (#660): the journey pins the review pass's clock
/// through `UiSettings::clock` and seeds every stamp relative to that
/// same fixed instant — the render (bars, date phrases) is identical
/// at any wall-clock time, so the golden never drifts with the hour
/// or the calendar.
#[test]
fn the_review_ledger_golden_holds() {
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            // The journey's clock: the binary's pinned mid-day UTC
            // instant (`fixture::PINNED_NOW`), held by the mount (the
            // settings below) and by the seeds alike — the `_at`
            // builder stamps the inherited chip seed against the same
            // instant (#663), so no seed can land in the frozen day's
            // future.
            let now = fixture::PINNED_NOW;
            let (fixture, mut state) = fixture::golden_state_at(now);
            let span = |start: chrono::TimeDelta, secs: i64| {
                (
                    now + start,
                    now + start + chrono::TimeDelta::try_seconds(secs).unwrap(),
                )
            };
            // Newest completion first: the unestimated tracked task.
            let _ = fixture::add_completed_task(
                &mut state,
                fixture.stack_a,
                "Calibrate the reading nook",
                now - chrono::TimeDelta::try_hours(2).unwrap(),
                None,
                Some(span(chrono::TimeDelta::try_hours(-3).unwrap(), 5_400)),
            );
            // The compared row: 90m estimated, 60m actual.
            let _ = fixture::add_completed_task(
                &mut state,
                fixture.stack_a,
                "Refit the shelf brackets",
                now - chrono::TimeDelta::try_hours(26).unwrap(),
                Some(5_400),
                Some(span(chrono::TimeDelta::try_hours(-30).unwrap(), 3_600)),
            );
            // The untracked estimated row.
            let _ = fixture::add_completed_task(
                &mut state,
                fixture.stack_b,
                "Sort the label drawer",
                now - chrono::TimeDelta::try_days(4).unwrap(),
                Some(1_800),
                None,
            );
            let state = Arc::new(ArcSwap::from_pointee(state));
            let settings = UiSettings {
                clock: Some(now),
                ..scale_settings(1.0)
            };
            capture_golden(
                driver,
                state,
                settings,
                |_, handle| enter_review(handle),
                1000,
                &PathBuf::from(format!(
                    "{}/tests/goldens/review-ledger-1.0-1000.png",
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
