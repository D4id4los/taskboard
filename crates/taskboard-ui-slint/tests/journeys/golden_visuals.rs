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
//! Exact dimensions, then the scale-aware box-averaged cell grids of
//! `support::golden_compare` (luma + chroma, cells of ~24 physical px,
//! per-channel budget 8/255 — the calibration and budgets are recorded
//! there). Drift within tolerance logs a warning; beyond it fails.
//!
//! # Regeneration — an explicit, reviewed step
//!
//! `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
//! --test journey_suites` rewrites the PNGs and *fails* the run
//! afterwards, so a regen run can never be mistaken for a pass. CI
//! never sets the variable: it compares only. The PR's PNG diff **is**
//! the review artefact (GitHub renders the images). Each regen also
//! upserts the golden's `tests/goldens/manifest.toml` entry (`support
//! ::golden_manifest`, wave plan §4): the checked-in manifest is
//! harness-generated, and the `the_manifest_bijects_with_the_golden_
//! corpus` test verifies it against the committed file set.

use crate::support;

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

/// The manifest metadata a capture site supplies (wave plan §4): the
/// fixture constructor (not prose), a structural state note, and the
/// mounted scale.
struct GoldenMeta<'a> {
    fixture: &'a str,
    state: &'a str,
    scale: f64,
}

/// The regen path's manifest upsert: name = the PNG stem, axes from
/// the capture. No-op on the compare path.
fn manifest_record(path: &Path, meta: &GoldenMeta, width: u32, height: u32) {
    let name = path
        .file_stem()
        .expect("the golden path has a stem")
        .to_string_lossy()
        .into_owned();
    support::golden_manifest::record(&support::golden_manifest::GoldenEntry {
        name,
        fixture: meta.fixture.to_owned(),
        state: meta.state.to_owned(),
        scale: meta.scale,
        width,
        height,
    });
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
#[allow(clippy::too_many_arguments)] // each capture-site parameter is load-bearing
fn capture_golden(
    driver: &mut SlintDriver,
    state: Arc<ArcSwap<taskboard_domain::AppState>>,
    settings: UiSettings,
    frame: impl FnOnce(&mut SlintDriver, &taskboard_ui_slint::UiHandle),
    meta: &GoldenMeta,
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
        None,
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
        manifest_record(path, meta, width, HEIGHT);
        return;
    }
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("golden {} readable: {error}", path.display()));
    let golden: Rgba8Image =
        golden_compare::decode_png(&bytes).expect("the committed golden decodes as RGBA8");
    golden_compare::matches(&golden, &snapshot)
        .unwrap_or_else(|error| panic!("golden {} drifted: {error}", path.display()));
}

/// The manifest ↔ corpus bijection (wave plan §4, #1140): the lowest
/// layer for the manifest's own behavior — every committed PNG has
/// exactly one manifest entry and vice versa, and the width/height
/// axes match the decoded PNG dimensions (the scale axis is capture
/// metadata, not PNG-derivable — recorded, not PNG-verified).
#[test]
fn the_manifest_bijects_with_the_golden_corpus() {
    let entries = support::golden_manifest::read_entries().expect("the manifest parses");
    assert!(!entries.is_empty(), "the golden corpus is nonempty");
    let mut pngs: Vec<(String, u32, u32)> = Vec::new();
    let dir = PathBuf::from(format!("{}/tests/goldens", env!("CARGO_MANIFEST_DIR")));
    for file in std::fs::read_dir(&dir).expect("the goldens dir is readable") {
        let path = file.expect("a dir entry").path();
        if path.extension().is_some_and(|extension| extension == "png") {
            let stem = path
                .file_stem()
                .expect("the PNG has a stem")
                .to_string_lossy()
                .into_owned();
            let bytes = std::fs::read(&path).expect("the PNG is readable");
            let image: Rgba8Image =
                golden_compare::decode_png(&bytes).expect("the committed PNG decodes");
            pngs.push((stem, image.width, image.height));
        }
    }
    support::golden_manifest::verify_corpus(&entries, &pngs)
        .expect("the manifest bijects with the corpus");
    assert_eq!(entries.len(), pngs.len(), "bijective on both sides");
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
        &GoldenMeta {
            fixture: "golden_state_at(PINNED_NOW)",
            state: "board view; two stacks, three tasks, running-timer chip",
            scale,
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
                None,
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
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_at(PINNED_NOW)",
                        state: "task detail; the D18 focus ring on the due row",
                        scale: 1.0,
                    },
                    TASK_VIEW_WIDTH,
                    900,
                );
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
        &GoldenMeta {
            fixture: "today_overdue_state_at(PINNED_NOW)",
            state: "today view; overdue rows, tracked-time chip",
            scale,
        },
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
    let meta = if mode == "today" {
        GoldenMeta {
            fixture: "today_overdue_state_at(PINNED_NOW)",
            state: "kiosk rotation; today mode after one forced flip",
            scale: KIOSK_SCALE,
        }
    } else {
        GoldenMeta {
            fixture: "golden_state_at(PINNED_NOW)",
            state: "kiosk rotation; board mode (modes[0])",
            scale: KIOSK_SCALE,
        }
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
        &meta,
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
                &GoldenMeta {
                    fixture: "golden_state_at + deliver_task_notification + add_alarm",
                    state: "notification surface; one task row + one alarm row",
                    scale: 3.0,
                },
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
        None,
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
        manifest_record(
            &path,
            &GoldenMeta {
                fixture: "golden_state_at(PINNED_NOW)",
                state: "task detail; header/summary/description regions",
                scale,
            },
            TASK_VIEW_WIDTH,
            height,
        );
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
                None,
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
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_at + FocusTiming settings",
                        state: "concentration overlay; work caption + pool + quick note",
                        scale: 1.0,
                    },
                    1000,
                    HEIGHT,
                );
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
#[allow(clippy::too_many_lines)] // the fixture's fixed-instant minting is the point
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
                        by: None,
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
                None,
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
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_at + two closed fixed-instant entries",
                        state: "time-entries overlay; closed rows + armed deletes",
                        scale: 1.0,
                    },
                    1000,
                    HEIGHT,
                );
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
            // Tier 1's D-B9 extension (#623 slice B): stored journal
            // entries alone keep the view standing now — and
            // `golden_state` seeds the review-card pin entry (#638) —
            // so the empty tier's fixture must be genuinely
            // reflection-free too (the two-leg clearing above is the
            // recorded fixture discipline).
            state.reflection.entries.clear();
            let state = Arc::new(ArcSwap::from_pointee(state));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |_, handle| enter_review(handle),
                &GoldenMeta {
                    fixture: "golden_state_at; tracking + reflection entries cleared",
                    state: "review view; empty tier (both sections folded)",
                    scale: 1.0,
                },
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
            // The reflection sections (#623 slice B): one rated Day
            // entry anchored to the same pinned instant — its stored
            // window_start sits inside both trailing windows and the
            // running day's bucket, so the sections and the journal
            // row render deterministically under the frozen clock.
            let mut journal = taskboard_domain::ReflectionEntry {
                id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::from_u128(0x623)),
                cadence: taskboard_domain::ReviewCadence::Day,
                day: (now - chrono::TimeDelta::try_hours(3).unwrap()).date_naive(),
                window_start: now - chrono::TimeDelta::try_hours(3).unwrap(),
                window_end: now + chrono::TimeDelta::try_hours(21).unwrap(),
                text: "Refit plan held; the reading nook is done.".to_owned(),
                attributes: taskboard_domain::ReflectionAttributes::default(),
                created_at: now,
                updated_at: now,
                deleted: false,
            };
            journal.attributes.mood = taskboard_domain::Rating::try_new(4).ok();
            journal.attributes.energy = taskboard_domain::Rating::try_new(2).ok();
            journal
                .attributes
                .flags
                .insert(taskboard_domain::HealthFlag::PoorSleep);
            state.reflection.entries.insert(journal.id, journal);
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
                &GoldenMeta {
                    fixture: "golden_state_at + add_completed_task x3 + one rated Day journal",
                    state: "review view; ledger rows + reflection sections",
                    scale: 1.0,
                },
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

// -- golden coverage wave (r4 S7): the seven gap goldens ------------------
//
// One golden per zero-coverage surface at the doc-publishable sweet
// spot (scale 1.0, 1000×900) over the enriched fixture
// (`fixture::golden_state_enriched_at` — the wave plan's superset
// decision: `golden_state_at` stays byte-stable for the 27). Every
// wall-derived caption rides the frozen `UiSettings::clock` and the
// fixture's pinned/midnight-aligned stamps; every overlay open asserts
// a typed precondition (the mount's view/state field) before the
// renderer-presence snapshot.

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::Color;
use taskboard_ui_slint::View;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::Stroke;

/// The gap goldens' shared path shape: `<surface>-1.0-1000.png`.
fn gap_golden_path(surface: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/{surface}-1.0-1000.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// The enriched fixture mounted at the wave's capture settings.
fn enriched_state() -> taskboard_domain::AppState {
    fixture::golden_state_enriched_at(fixture::PINNED_NOW).1
}

/// D2 — the quick-capture overlay open over the enriched board: the
/// draft field, the commit affordance, and the scrim over the cards.
#[test]
fn the_capture_overlay_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(enriched_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::CaptureQuick);
                    driver.advance_time(0);
                    assert!(
                        handle.window().get_capture_open(),
                        "typed precondition: the capture overlay is open"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "capture overlay open over the board",
                    scale: 1.0,
                },
                1000,
                &gap_golden_path("capture-overlay"),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// D4 — the two-tier action menu: the app menu open with a group's
/// page invoked (the fly-out beside the root panel). The context quick
/// bar's pixels stay with the action-menu-bar probe (the object menu
/// opens pointer-only; the two tiers are this capture's subject).
#[test]
fn the_action_menu_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(enriched_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::AppMenu);
                    driver.advance_time(0);
                    // Walk the cursor to the first submenu-parent row
                    // (the journeys' two-tier idiom), then invoke it.
                    let entries = handle
                        .window()
                        .global::<taskboard_ui_slint::KeyboardState>()
                        .get_menu_entries();
                    let parent = (0..entries.row_count())
                        .find(|i| entries.row_data(*i).is_some_and(|row| row.submenu))
                        .expect("the root page carries a submenu parent");
                    for _ in 0..parent {
                        handle.dispatch_action(ActionId::MenuNext);
                    }
                    handle.dispatch_action(ActionId::MenuInvoke);
                    driver.advance_time(0);
                    let keyboard = handle
                        .window()
                        .global::<taskboard_ui_slint::KeyboardState>();
                    assert!(
                        keyboard.get_menu_open() && keyboard.get_menu_active_group() >= 0,
                        "typed precondition: the menu's second tier is open"
                    );
                    assert!(
                        keyboard.get_menu_submenu_entries().row_count() > 0,
                        "typed precondition: the submenu model is mounted"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "action menu; root panel + invoked group page",
                    scale: 1.0,
                },
                1000,
                &gap_golden_path("action-menu"),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// D9 — the plan-next-day view over the enriched fixture: the pool
/// and the placement boxes for tomorrow.
#[test]
fn the_plan_next_day_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(enriched_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::ViewPlanNextDay);
                    driver.advance_time(0);
                    assert_eq!(
                        handle.window().get_current_view(),
                        View::PlanNextDay,
                        "typed precondition: the plan view is mounted"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "plan-next-day view; pool + placement boxes",
                    scale: 1.0,
                },
                1000,
                &gap_golden_path("plan-next-day"),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// D8 — the review-walk overlay: the deliberate-due variant the walk
/// journeys use (the reflection pin cleared, an Inbox board with
/// triage rows, `task0` scheduled inside yesterday's window so the
/// Day walk carries a row), walk open over the board. Phrase
/// stability: the carry stamp is midnight/calendar-aligned off
/// `fixture::PINNED_NOW`, the pass's clock frozen at the same instant.
fn walk_golden_state() -> taskboard_domain::AppState {
    let (fixture, mut state) = fixture::golden_state_at(fixture::PINNED_NOW);
    // The surfacing card cleared (the pin law's deliberate due shape).
    state.reflection.entries.clear();
    let inbox_board = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x1900));
    let mut inbox = taskboard_domain::Board {
        id: inbox_board,
        remote: None,
        title: "Inbox".to_owned(),
        color: Color::new(""),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: taskboard_domain::BoardClocks::epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::BoardAuthors::default(),
        remote_seen: None,
    };
    inbox.id = inbox_board;
    state.boards.insert(inbox_board, inbox);
    let inbox_stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::from_u128(0x1901));
    state.stacks.insert(
        inbox_stack,
        taskboard_domain::Stack {
            id: inbox_stack,
            remote: None,
            board: inbox_board,
            title: "Inbox".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                order: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        },
    );
    for (i, title) in ["triage-me", "triage-me-too"].into_iter().enumerate() {
        let id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::from_u128(
            u128::try_from(0x1902 + i).unwrap_or(0),
        ));
        let mut triage = taskboard_domain::Task {
            id,
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: inbox_stack,
            order: 0,
            label_memberships: std::collections::BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::TaskClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                description: taskboard_domain::BoardClocks::epoch(),
                duedate: taskboard_domain::BoardClocks::epoch(),
                scheduled: taskboard_domain::BoardClocks::epoch(),
                done: taskboard_domain::BoardClocks::epoch(),
                position: taskboard_domain::BoardClocks::epoch(),
                effort: taskboard_domain::BoardClocks::epoch(),
                importance: taskboard_domain::BoardClocks::epoch(),
                urgency: taskboard_domain::BoardClocks::epoch(),
                recur: taskboard_domain::BoardClocks::epoch(),
                kind: taskboard_domain::BoardClocks::epoch(),
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        };
        // The carry anchor: task0 scheduled inside yesterday's window
        // (the walk journeys' midnight-aligned convention).
        if i == 0 {
            let calendar = fixture::harness_calendar();
            let yesterday = calendar
                .today(fixture::PINNED_NOW)
                .pred_opt()
                .expect("yesterday");
            triage.scheduled = Some(calendar.instant(
                yesterday,
                chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
            ));
        }
        state.tasks.insert(id, triage);
    }
    let _ = fixture;
    state
}

/// D8's capture: the walk overlay (guidance + inbox stage) over the
/// deliberate-due fixture.
#[test]
fn the_review_walk_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(walk_golden_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::ReviewOpen);
                    driver.advance_time(0);
                    assert!(
                        handle.window().get_walk_model().open,
                        "typed precondition: the review walk is open"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_at + cleared pin + Inbox board + carry-scheduled task0",
                    state: "review walk overlay; guidance + inbox stage rows",
                    scale: 1.0,
                },
                1000,
                &gap_golden_path("review-walk"),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// D3 — the search palette with results: the palette open in insert
/// mode, the query typed through the same `press_stroke` path the
/// journeys use, the roster hits mounted. The fixture constructor and
/// the mount run in this test's own body (the typing seam needs the
/// bound driver before the snapshot).
#[test]
fn the_search_palette_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(enriched_state()));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = taskboard_ui_slint::mount(
                state,
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                scale_settings(1.0),
                detached_health(),
                None,
            )
            .expect("mount succeeds on the testing backend");
            driver.bind(&handle);
            handle.dispatch_action(ActionId::PaletteSearch);
            driver.advance_time(0);
            assert!(
                handle.window().get_palette_open(),
                "typed precondition: the palette is open"
            );
            for ch in "task".chars() {
                driver.press_stroke(Stroke {
                    key: ch,
                    ctrl: false,
                    alt: false,
                    shift: false,
                    meta: false,
                });
            }
            driver.advance_time(0);
            assert!(
                handle.window().get_palette_model().row_count() > 0,
                "typed precondition: the query hits the roster"
            );
            driver.set_window_size(1000, HEIGHT);
            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1000, HEIGHT),
                "renderer-presence contract at the palette golden"
            );
            let path = gap_golden_path("search-palette");
            if regen {
                let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
                std::fs::write(&path, bytes).expect("writes");
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_enriched_at(PINNED_NOW)",
                        state: "search palette open; typed query with roster hits",
                        scale: 1.0,
                    },
                    1000,
                    HEIGHT,
                );
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

/// D1 — the settings view, populated: the enriched fixture's two
/// boards in the roster, a bound Deck connection card whose listing
/// renders one bound row (the Disconnect stop) and one free row (the
/// picker + Create stops). The request lane stays alive: the mount's
/// auto-listings and the refresh are answered through it (the
/// component journeys' `settings_fixture` shape).
#[allow(clippy::too_many_lines)] // the request/reply dance is the capture's body
#[test]
fn the_settings_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let (_fx, mut state) = fixture::golden_state_enriched_at(fixture::PINNED_NOW);
            let bound = state
                .boards
                .values_mut()
                .find(|board| !board.deleted && !board.archived)
                .expect("a live board");
            bound.remote = Some(taskboard_domain::RemoteBoardBinding {
                board: taskboard_domain::RemoteBoardId::deck(900),
                connector: taskboard_domain::ConnectorId::from("nextcloud"),
            });
            let state = Arc::new(ArcSwap::from_pointee(state));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, mut requests_rx) = tokio::sync::mpsc::channel(16);
            let settings = UiSettings {
                clock: Some(fixture::PINNED_NOW),
                connectors: ["nextcloud", "legacy"]
                    .into_iter()
                    .map(|name| taskboard_ui_slint::requests::ConnectorInfo {
                        name: name.into(),
                        server_url: format!("https://{name}.example"),
                        username: "alice".into(),
                        backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
                    })
                    .collect(),
                ..UiSettings::default()
            };
            let handle = taskboard_ui_slint::mount(
                Arc::clone(&state),
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                settings,
                detached_health(),
                None,
            )
            .expect("mount succeeds on the testing backend");
            driver.bind(&handle);
            // The mount's auto-listings: answered empty, lane drained.
            while let Ok(request) = requests_rx.try_recv() {
                let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. } =
                    request
                else {
                    panic!("the mount routed only auto-lists");
                };
                reply.send(Ok(Vec::new())).expect("the reply lane is alive");
            }
            handle.drain_auto_listings();
            handle.dispatch_action(ActionId::AppSettings);
            driver.advance_time(0);
            assert_eq!(
                handle.window().get_current_view(),
                View::Settings,
                "typed precondition: the settings view is mounted"
            );
            // The Deck connector's refresh, answered with one bound and
            // one free remote row (the connection card's populated state).
            let items = handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_connection_items();
            let deck = (0..items.row_count())
                .find(|&i| {
                    items.row_data(i).is_some_and(|item| {
                        item.kind == taskboard_ui_slint::ConnectionItemKind::Deck
                            && item.title == "nextcloud"
                    })
                })
                .expect("the connector's Deck row renders");
            handle
                .window()
                .invoke_settings_refresh_clicked(i32::try_from(deck).expect("the row index fits"));
            let Ok(taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. }) =
                requests_rx.try_recv()
            else {
                panic!("the refresh routed a listing");
            };
            let rows = [("server board", 900_u64)]
                .into_iter()
                .map(|(title, id)| taskboard_ui_slint::requests::RemoteBoardRow {
                    remote_id: taskboard_domain::RemoteBoardId::deck(id),
                    title: title.to_owned(),
                })
                .collect();
            reply.send(Ok(rows)).expect("the reply lane is alive");
            handle.drain_setting_replies();
            driver.advance_time(0);
            driver.set_window_size(1000, HEIGHT);
            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1000, HEIGHT),
                "renderer-presence contract at the settings golden"
            );
            let path = gap_golden_path("settings");
            if regen {
                let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
                std::fs::write(&path, bytes).expect("writes");
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_enriched_at + bound Deck connection",
                        state: "settings view; connection cards + board roster rows",
                        scale: 1.0,
                    },
                    1000,
                    HEIGHT,
                );
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

/// D10 — the alarm window with awaiting rows: the lazily mounted
/// second window, built directly over the enriched fixture + one
/// delivered alarm (the app window mounts first only to register the
/// bundled fonts — the same registration the pass relies on). The row
/// phrases derive from the pinned render clock (`notification_phrase`
/// against `fixture::PINNED_NOW`), so the elapsed captions are
/// byte-stable by construction.
#[test]
fn the_alarm_window_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let mut state = enriched_state();
            let fired = fixture::PINNED_NOW - chrono::TimeDelta::try_minutes(30).unwrap();
            // The Alarm insistence rung: the alarm window's rows are
            // the alarm-insistent awaiting set (the presentation
            // filter), so the fixture mints through the rung variant.
            let alarm = fixture::add_alarm_with_rung(
                &mut state,
                "tea is ready",
                fired,
                taskboard_domain::Insistence::Alarm,
            );
            fixture::deliver_alarm(&mut state, alarm, fired);
            // The font registration rides the app window's mount.
            let app_state = Arc::new(ArcSwap::from_pointee(enriched_state()));
            let (commands, _engine) =
                support::fake_engine::FakeEngine::channel(Arc::clone(&app_state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let _app = taskboard_ui_slint::mount(
                app_state,
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                scale_settings(1.0),
                detached_health(),
                None,
            )
            .expect("mount succeeds on the testing backend");
            let defaults = state.settings.reminder_defaults();
            let calendar = state
                .settings
                .day_calendar(taskboard_ui_slint::UiSettings::default().boot_offset);
            let rows = taskboard_ui_slint::presentation::alarm_window_rows(
                &state,
                &defaults,
                calendar,
                fixture::PINNED_NOW,
            );
            assert!(
                !rows.is_empty(),
                "typed precondition: the alarm has an awaiting row"
            );
            let window = taskboard_ui_slint::AlarmWindow::new()
                .expect("the alarm window instantiates on the testing backend");
            let model: Vec<taskboard_ui_slint::AlarmRowModel> = rows
                .iter()
                .map(|row| taskboard_ui_slint::AlarmRowModel {
                    title: row.title.clone().into(),
                    phrase: taskboard_ui_slint::presentation::notification_phrase(
                        row.occurrence,
                        fixture::PINNED_NOW,
                    )
                    .into(),
                })
                .collect();
            window.set_rows(slint::ModelRc::new(slint::VecModel::from(model)));
            window.show().expect("show on the testing platform");
            driver.advance_time(0);
            let buffer = window.window().take_snapshot().expect(
                "the software renderer captures the alarm window \
                         (renderer-presence contract)",
            );
            let snapshot =
                Rgba8Image::new(buffer.width(), buffer.height(), buffer.as_bytes().to_vec());
            let path = gap_golden_path("alarm-window");
            if regen {
                let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
                std::fs::write(&path, bytes).expect("writes");
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_enriched_at + add_alarm/deliver_alarm",
                        state: "alarm window; awaiting rows with elapsed phrases",
                        scale: 1.0,
                    },
                    snapshot.width,
                    snapshot.height,
                );
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
