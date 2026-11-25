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
//! The fixture is the enriched golden fixture
//! (`fixture::golden_state_enriched_at`, the #1239 re-base; the today
//! captures layer overdue stamps via `today_overdue_enriched_at`) —
//! every wall-derived caption is pinned/midnight-aligned against the
//! frozen render clock, so no caption flakes across hour boundaries.
//! Titles are ASCII-only (system-font fallback can
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

/// The regen run's designed post-write failure (the goldens'
/// protocol): a regen run can never pass.
fn assert_regen_marker() {
    assert!(
        !regen_requested(),
        "goldens regenerated — review the PNG diff before landing"
    );
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
/// The `TodayClipFacts` halves as one tuple — the quiescence
/// comparison's unit (a report landing changes either half).
fn today_clip_facts(handle: &taskboard_ui_slint::UiHandle) -> [f32; 8] {
    let facts = handle
        .window()
        .global::<taskboard_ui_slint::TodayClipFacts>();
    [
        facts.get_e0(),
        facts.get_h0(),
        facts.get_e1(),
        facts.get_h1(),
        facts.get_e2(),
        facts.get_h2(),
        facts.get_e3(),
        facts.get_h3(),
    ]
}

/// The today sections' extent-report settle loop (the golden's and
/// the extent-law pin's shared oracle driver): advance on the mock
/// clock until the facts tuple holds unchanged across four
/// consecutive passes (two proved too tight under load, where an
/// in-flight report lands one pass later) or a 32-pass deadline.
fn settle_today_facts(driver: &mut SlintDriver, handle: &taskboard_ui_slint::UiHandle) {
    let mut previous = today_clip_facts(handle);
    let mut stable = 0;
    for _ in 0..32 {
        driver.advance_time(0);
        let current = today_clip_facts(handle);
        // Exact comparison is the point: the facts change only when a
        // report or a push lands, never by float noise (the quiescence
        // contract — a changed bit means work is still in flight).
        #[allow(clippy::float_cmp)]
        if current == previous {
            stable += 1;
        } else {
            stable = 0;
        }
        previous = current;
        if stable >= 4 {
            break;
        }
    }
}

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
    // Settle the today sections' extent-report pipeline (round-6 A1,
    // #1310/#1304: the clip heights converge over report -> refresh
    // -> push passes; the golden pins the SETTLED frame — terminal
    // state, never a transient per the clock discipline). Poll to a
    // deadline on the mock clock; the today view's facts global is
    // the oracle (no-op for non-Today captures: no populated
    // sections trivially settles).
    settle_today_facts(driver, &handle);

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
/// The encoder contract's corpus half (chunk G1, #1241): every
/// committed PNG re-encodes byte-identically through the harness
/// encoder. A regen over unchanged pixels can never produce different
/// bytes than committed — so an encoder-graph drift (`png`/`flate2`/
/// `miniz_oxide`/`fdeflate` version bump) fails here and forces a
/// deliberate whole-corpus re-encode commit (the drift law on
/// `golden_compare::encode_png`). Decode+encode only: no renderer, no
/// clock, fully deterministic.
#[test]
fn the_committed_corpus_reencodes_byte_identical() {
    let dir = PathBuf::from(format!("{}/tests/goldens", env!("CARGO_MANIFEST_DIR")));
    let mut checked = 0;
    for file in std::fs::read_dir(&dir).expect("the goldens dir is readable") {
        let path = file.expect("a dir entry").path();
        if path.extension().is_some_and(|extension| extension == "png") {
            let bytes = std::fs::read(&path).expect("the PNG is readable");
            let image: Rgba8Image =
                golden_compare::decode_png(&bytes).expect("the committed PNG decodes");
            let reencoded = golden_compare::encode_png(&image).expect("re-encodes");
            assert_eq!(
                reencoded,
                bytes,
                "{} re-encodes to different bytes — the encoder graph \
                 moved; run a whole-corpus regen (TASKBOARD_REGEN_GOLDENS=1) \
                 in its own commit",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the corpus is nonempty");
}

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
        fixture::golden_state_enriched_at(fixture::PINNED_NOW).1,
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
            fixture: "golden_state_enriched_at(PINNED_NOW)",
            state: "board view; two stacks, label chips + recur + reminder badges, degraded sync, second board in the bar, running-timer chip",
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
                fixture::golden_state_enriched_at(fixture::PINNED_NOW).1,
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
                        fixture: "golden_state_enriched_at(PINNED_NOW)",
                        state: "task detail; the D18 focus ring on the due row (enriched base)",
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
    let state = Arc::new(ArcSwap::from_pointee(fixture::today_overdue_enriched_at(
        fixture::PINNED_NOW,
    )));
    capture_golden(
        driver,
        state,
        scale_settings(scale),
        // The today goldens capture the TODAY view: no escape — the
        // desktop default is the capture subject.
        |_, _| {},
        &GoldenMeta {
            fixture: "today_overdue_enriched_at(PINNED_NOW)",
            state: "today view; overdue rows per board, tracked-time chip, enriched base",
            scale,
        },
        width,
        &today_golden_path(scale, width),
        regen,
    );
}

// D-1334d (#1334): the extent channel's coverage/mirroring law,
// pinned at the non-1.0 scale — no image diffing; the facts global is
// the typed oracle (the golden stays the visual evidence). The
// stage-2 diagnosis disproved the issue's unit-mismatch hypothesis:
// the row-extent reports and the clip heights are consistent logical
// lengths at every scale; the golden offset was layout slack (the
// constraint solver's surplus distribution, fixed by the content's
// layout sink). What stays pinned here is the pipeline law that the
// diagnosis rests on: every live row's report covers its section's
// extent, the clip mirrors the extent (the cap never binds on this
// fixture), and empty sections settle to zero.
#[test]
fn the_today_extent_channel_covers_its_live_rows_at_half_scale() {
    pin_utc_zone();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(
                fixture::today_overdue_state_at(fixture::PINNED_NOW).1,
            ));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = taskboard_ui_slint::mount(
                state,
                dummy_signals(),
                dummy_system(),
                commands,
                requests_tx,
                scale_settings(0.5),
                detached_health(),
                None,
            )
            .expect("mount succeeds on the testing backend");
            driver.bind(&handle);
            driver.set_window_size(1000, HEIGHT);
            driver.advance_time(0); // instantiate conditionals
            settle_today_facts(driver, &handle);
            let facts = today_clip_facts(&handle);
            let (e0, h0, e1, h1, e2, h2, e3, h3) = (
                facts[0], facts[1], facts[2], facts[3], facts[4], facts[5], facts[6], facts[7],
            );
            // The fixture's today view carries two live Overdue rows
            // (section 0) and nothing in scheduled/due-today/done. The
            // extent channel must have received EVERY live row's
            // report: the section's extent is the settled sum of its
            // rows' realized heights, strictly positive.
            assert!(
                e0 > 0.0,
                "the overdue extent must cover its live rows (e0 = {e0})"
            );
            // Empty sections settle to zero, and their clips mirror it
            // (the collapse is exact, not a free layout variable — the
            // free-variable shape was the #1334 offset's fuel).
            #[allow(clippy::float_cmp)]
            {
                assert_eq!(h0, e0, "the overdue clip mirrors its extent");
                assert_eq!(
                    (e1, e2, e3),
                    (0.0, 0.0, 0.0),
                    "empty sections settle to zero"
                );
                assert_eq!((h1, h2, h3), (0.0, 0.0, 0.0), "empty clips mirror zero");
            }
        },
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
        Arc::new(ArcSwap::from_pointee(fixture::today_overdue_enriched_at(
            fixture::PINNED_NOW,
        )))
    } else {
        Arc::new(ArcSwap::from_pointee(
            fixture::golden_state_enriched_at(fixture::PINNED_NOW).1,
        ))
    };
    let meta = if mode == "today" {
        GoldenMeta {
            fixture: "today_overdue_enriched_at(PINNED_NOW)",
            state: "kiosk rotation; today mode after one forced flip (enriched base)",
            scale: KIOSK_SCALE,
        }
    } else {
        GoldenMeta {
            fixture: "golden_state_enriched_at(PINNED_NOW)",
            state: "kiosk rotation; board mode (modes[0]), enriched base",
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
            let (fixture, mut state) = fixture::golden_state_enriched_at(fixture::PINNED_NOW);
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
                    fixture: "golden_state_enriched_at + deliver_task_notification + add_alarm",
                    state: "notification surface; one task row + one alarm row (enriched base)",
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
        fixture::golden_state_enriched_at(fixture::PINNED_NOW).1,
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
                fixture: "golden_state_enriched_at(PINNED_NOW)",
                state: "task detail; header/summary/description regions (enriched base)",
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
            let mut golden = fixture::golden_state_enriched_at(fixture::PINNED_NOW).1;
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
                        fixture: "golden_state_enriched_at + FocusTiming settings",
                        state: "concentration overlay; work caption + pool + quick note (enriched base)",
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
            let (fx, mut state) = fixture::golden_state_enriched_at(fixture::PINNED_NOW);
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
                        fixture: "golden_state_enriched_at + two closed fixed-instant entries",
                        state: "time-entries overlay; closed rows + armed deletes (enriched base)",
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
            // The #1239 re-base finding (round 7 G1): the review view's
            // header chrome projects the board bar (the second board
            // and the degraded-sync badge), so re-basing this capture
            // onto the enriched fixture changed its pixels — it stays
            // on the un-enriched base to keep the committed bytes
            // stable (the four-surface byte-identity law).
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
                    fixture: "golden_state; tracking + reflection entries cleared",
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
            // The #1239 re-base finding (round 7 G1): same as
            // review-empty's — the review header chrome projects the
            // board bar, so this capture stays on the un-enriched base
            // to keep the committed bytes stable.
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
// (`fixture::golden_state_enriched_at` — which, since the #1239
// re-base, is also the pre-wave goldens' base; the review-walk
// capture below deliberately keeps the un-enriched
// `golden_state_at` + its Inbox layer so its committed bytes stay
// stable). Every
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

/// The gap goldens' shared path shape: `<surface>-1.0-<width>.png`
/// (the width rides the actual capture axis; the #1390 law).
fn gap_golden_path(surface: &str, width: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/{surface}-1.0-{width}.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// The enriched fixture mounted at the wave's capture settings.
fn enriched_state() -> taskboard_domain::AppState {
    fixture::golden_state_enriched_at(fixture::PINNED_NOW).1
}

/// D2 — the quick-capture overlay open over the enriched board: the
/// draft field, the commit affordance, and the scrim over the cards.
fn capture_capture_overlay(driver: &mut SlintDriver, width: u32, regen: bool) {
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
        width,
        &gap_golden_path("capture-overlay", width),
        regen,
    );
}

/// The capture runner for the `capture_overlay` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_capture_overlay(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_capture_overlay(driver, width, regen),
    );
}

#[test]
fn the_capture_overlay_golden_holds() {
    run_capture_capture_overlay(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_capture_overlay_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_capture_overlay(width);
    }
    assert_regen_marker();
}

/// D4 — the two-tier action menu: the app menu open with a group's
/// page invoked (the fly-out beside the root panel). The context quick
/// bar's pixels stay with the action-menu-bar probe (the object menu
/// opens pointer-only; the two tiers are this capture's subject).
fn capture_action_menu(driver: &mut SlintDriver, width: u32, regen: bool) {
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
        width,
        &gap_golden_path("action-menu", width),
        regen,
    );
}

/// The capture runner for the `action_menu` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_action_menu(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_action_menu(driver, width, regen),
    );
}

#[test]
fn the_action_menu_golden_holds() {
    run_capture_action_menu(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_action_menu_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_action_menu(width);
    }
    assert_regen_marker();
}

/// D9 — the plan-next-day view over the enriched fixture: the pool
/// and the placement boxes for tomorrow.
fn capture_plan_next_day(driver: &mut SlintDriver, width: u32, regen: bool) {
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
        width,
        &gap_golden_path("plan-next-day", width),
        regen,
    );
}

/// The capture runner for the `plan_next_day` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_plan_next_day(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_plan_next_day(driver, width, regen),
    );
}

#[test]
fn the_plan_next_day_golden_holds() {
    run_capture_plan_next_day(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_plan_next_day_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_plan_next_day(width);
    }
    assert_regen_marker();
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
fn capture_review_walk(driver: &mut SlintDriver, width: u32, regen: bool) {
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
        width,
        &gap_golden_path("review-walk", width),
        regen,
    );
}

/// The capture runner for the `review_walk` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_review_walk(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_review_walk(driver, width, regen),
    );
}

#[test]
fn the_review_walk_golden_holds() {
    run_capture_review_walk(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_review_walk_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_review_walk(width);
    }
    assert_regen_marker();
}

/// D3 — the search palette with results: the palette open in insert
/// mode, the query typed through the same `press_stroke` path the
/// journeys use, the roster hits mounted. The fixture constructor and
/// the mount run in this test's own body (the typing seam needs the
/// bound driver before the snapshot).
fn capture_search_palette(driver: &mut SlintDriver, width: u32, regen: bool) {
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
    driver.set_window_size(width, HEIGHT);
    let snapshot = driver.snapshot();
    assert_eq!(
        (snapshot.width, snapshot.height),
        (width, HEIGHT),
        "renderer-presence contract at the palette golden"
    );
    let path = gap_golden_path("search-palette", width);
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
            width,
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
}

/// The capture runner for the `search_palette` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_search_palette(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_search_palette(driver, width, regen),
    );
}

#[test]
fn the_search_palette_golden_holds() {
    run_capture_search_palette(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_search_palette_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_search_palette(width);
    }
    assert_regen_marker();
}

/// D1 — the settings view, populated: the enriched fixture's two
/// boards in the roster, a bound Deck connection card whose listing
/// renders one bound row (the Disconnect stop) and one free row (the
/// picker + Create stops). The request lane stays alive: the mount's
/// auto-listings and the refresh are answered through it (the
/// component journeys' `settings_fixture` shape).
#[allow(clippy::too_many_lines)] // the request/reply dance is the capture's body
fn capture_settings(driver: &mut SlintDriver, width: u32, regen: bool) {
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
        let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. } = request
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
    driver.set_window_size(width, HEIGHT);
    let snapshot = driver.snapshot();
    assert_eq!(
        (snapshot.width, snapshot.height),
        (width, HEIGHT),
        "renderer-presence contract at the settings golden"
    );
    let path = gap_golden_path("settings", width);
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
            width,
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
}

/// The capture runner for the `settings` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_settings(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_settings(driver, width, regen),
    );
}

#[test]
fn the_settings_golden_holds() {
    run_capture_settings(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_settings_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_settings(width);
    }
    assert_regen_marker();
}

/// D10 — the alarm window with awaiting rows: the lazily mounted
/// second window, built directly over the enriched fixture + one
/// delivered alarm (the app window mounts first only to register the
/// bundled fonts — the same registration the pass relies on). The row
/// phrases derive from the pinned render clock (`notification_phrase`
/// against `fixture::PINNED_NOW`), so the elapsed captions are
/// byte-stable by construction.
fn capture_alarm_window(driver: &mut SlintDriver, width: u32, regen: bool) {
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
    let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&app_state));
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
    // The breakpoint variants widen the content-sized window
    // with the same rows (the height stays content-sized); the
    // captured PNG records the window's realized size — if the
    // surface clamps its own width, the golden name follows the
    // actual width (the #1390 law).
    if width != 1000 {
        window.window().set_size(slint::LogicalSize::new(
            f32::from(u16::try_from(width).expect("width fits u16")),
            64.0,
        ));
    }
    driver.advance_time(0);
    let buffer = window.window().take_snapshot().expect(
        "the software renderer captures the alarm window \
                     (renderer-presence contract)",
    );
    let snapshot = Rgba8Image::new(buffer.width(), buffer.height(), buffer.as_bytes().to_vec());
    // The stem carries the *rendered* PNG width, not the capture
    // window parameter: at the mid breakpoint the window opens at its
    // default (content-sized) width — 351px realized, not the 1000
    // axis the sibling breakpoints set (the #1390 law).
    let path = gap_golden_path("alarm-window", snapshot.width);
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
}

/// The capture runner for the `alarm_window` family: one driver environment
/// at the given breakpoint width (the #1242 breakpoint law — the
/// wave-1 surface's fixture/state at the 600/1500 axes).
fn run_capture_alarm_window(width: u32) {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| capture_alarm_window(driver, width, regen),
    );
}

#[test]
fn the_alarm_window_golden_holds() {
    run_capture_alarm_window(1000);
    assert_regen_marker();
}

/// The wave-1 surface's breakpoint variants (#1242, #1312): the SAME
/// fixture/state at the 600/1500 axes (the manifest's `state` field
/// is the parent row's; only the axes differ).
#[test]
fn the_alarm_window_goldens_hold_across_the_breakpoints() {
    for width in [600_u32, 1500] {
        run_capture_alarm_window(width);
    }
    assert_regen_marker();
}

// -- golden wave 2 (round 7 G2, #1312): the deferred editors -----------
//
// D5–D7 (the golden-coverage review report §6): the three editor
// states the wave-1 corpus deferred. The staging law (the plan §2.A):
// every editor golden shows the editor WITH content — the overdue
// banner over an actually-overdue task, the due editor on a task
// whose date the picker pre-selects, the labels overlay over a task
// already carrying chips. Every capture freezes the render clock and
// pins the zone (the banner phrase and the picker's pre-selected day
// are clock-derived text).

/// The overdue-stamped fixture, mounted at the wave's capture
/// settings (the today goldens' subject constructor).
fn today_overdue_state() -> taskboard_domain::AppState {
    fixture::today_overdue_enriched_at(fixture::PINNED_NOW)
}

/// D5 — the task detail over the overdue machinery: the walked-into
/// overdue task's detail with the status bar's overdue banner rows
/// (the mute/snooze affordances) mounted. The banner is board truth
/// in the status bar (app-window), so the capture stages the detail
/// AND the banner in one frame.
#[test]
fn the_task_view_overdue_banner_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(today_overdue_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    // The today view's walk idiom: focus the first row
                    // (an overdue row — the fixture stamps one per
                    // board), open its detail.
                    handle.dispatch_action(ActionId::NavFirstItem);
                    handle.dispatch_action(ActionId::TaskEdit);
                    driver.advance_time(0);
                    assert_eq!(
                        handle.window().get_current_view(),
                        View::TaskDetail,
                        "typed precondition: the overdue task's detail is mounted"
                    );
                    assert!(
                        handle.window().get_banner_model().row_count() > 0,
                        "typed precondition: the overdue banner rows are mounted"
                    );
                },
                &GoldenMeta {
                    fixture: "today_overdue_enriched_at(PINNED_NOW)",
                    state: "task detail over an overdue task; status-bar overdue banner rows",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("task-view-overdue-banner", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// Stages the daily series (the enriched fixture's task2 rule shape)
/// onto the walk's landing row, so the recur goldens' subject is
/// deterministic: the walks' `NavFirstItem` lands on the first task,
/// and the staged rule's fixed series id keeps the assembled echo's
/// JSON (which embeds the series uuid) byte-stable — a rule-less
/// subject mints a fresh v7 series per run (the drift this staging
/// closes; found by the regen-twice proof).
fn stage_daily_series_on_first_task(state: &mut taskboard_domain::AppState) {
    let first = state_first_task(state);
    let task = state
        .tasks
        .get_mut(&first)
        .expect("the fixture's first task");
    task.recur = Some(taskboard_domain::RecurRule {
        series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(0x5EED)),
        anchor: fixture::PINNED_NOW.date_naive(),
        pattern: taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::EveryDays::new(1).expect("1 is a valid interval"),
        },
        bound: taskboard_domain::RecurBound::None,
        continued_from: None,
        paused_until: None,
    });
}

/// The fixture board's first task (the walk's landing row): the
/// lowest-order task of the first stack (the models' display order).
fn state_first_task(state: &taskboard_domain::AppState) -> taskboard_domain::TaskId {
    let mut stacks: Vec<taskboard_domain::StackId> = state
        .stacks
        .values()
        .filter(|stack| !stack.deleted && !stack.archived)
        .map(|stack| stack.id)
        .collect();
    stacks.sort();
    let first_stack = stacks.first().expect("the fixture has a stack");
    state
        .tasks
        .values()
        .filter(|task| &task.stack == first_stack && !task.deleted)
        .min_by_key(|task| (task.order, task.id))
        .expect("the fixture stack has a task")
        .id
}

/// The wave-2 goldens' shared path shape: `<surface>-<scale>-<width>
/// .png` — the name axes are the capture's actual axes (the #1390
/// law: no stale width/height in a stem).
fn wave2_golden_path(surface: &str, scale: f64, width: u32) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/{surface}-{scale:.1}-{width}.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// D6 — the due editor with the date-picker grid open: the walked-into
/// enriched task's detail with a staged due date (tomorrow off the
/// frozen render clock), the due editor staged, the picker grid
/// overlay open over it with that day pre-selected (the staging law:
/// the grid pre-selects, never an empty mount).
#[test]
fn the_due_editor_datepicker_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            // The staging law (plan §2.A): the editor opens on a task
            // WITH a due date, so the picker grid pre-selects it. The
            // stamp is midnight-aligned off the pinned render clock
            // (the fixture convention; date-only phrase, run-stable).
            let mut staged = enriched_state();
            let calendar = fixture::harness_calendar();
            let day = calendar
                .today(fixture::PINNED_NOW)
                .succ_opt()
                .expect("tomorrow")
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc();
            let first = state_first_task(&staged);
            staged
                .tasks
                .get_mut(&first)
                .expect("the fixture's first task")
                .duedate = Some(day);
            let state = Arc::new(ArcSwap::from_pointee(staged));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    handle.dispatch_action(ActionId::NavFirstItem);
                    handle.dispatch_action(ActionId::TaskEdit);
                    assert_eq!(
                        handle.window().get_current_view(),
                        View::TaskDetail,
                        "typed precondition: the task detail is mounted"
                    );
                    handle.dispatch_action(ActionId::TaskDueEdit);
                    driver.advance_time(0);
                    let task = handle.window().get_task_model();
                    assert!(
                        task.due_open && task.date_picker_open,
                        "typed precondition: the due editor is staged with the picker grid open"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "due editor with the date-picker grid open over the task detail",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("due-editor-datepicker", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// D7 — the labels overlay with staged content: the enriched task's
/// detail (its label chips render beneath the overlay) with the
/// labels overlay open over it (the roster + assignment toggles).
#[test]
fn the_labels_overlay_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            // The staged due date: the detail's due row renders, and
            // the labels overlay opens over a populated detail. The
            // stamp is midnight-aligned off the pinned render clock
            // (the fixture convention; date-only, run-stable).
            let mut staged = enriched_state();
            let calendar = fixture::harness_calendar();
            let day = calendar
                .today(fixture::PINNED_NOW)
                .succ_opt()
                .expect("tomorrow")
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc();
            let first = state_first_task(&staged);
            staged
                .tasks
                .get_mut(&first)
                .expect("the fixture's first task")
                .duedate = Some(day);
            let state = Arc::new(ArcSwap::from_pointee(staged));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    handle.dispatch_action(ActionId::NavFirstItem);
                    handle.dispatch_action(ActionId::TaskEdit);
                    assert_eq!(
                        handle.window().get_current_view(),
                        View::TaskDetail,
                        "typed precondition: the task detail is mounted"
                    );
                    handle.dispatch_action(ActionId::LabelManage);
                    driver.advance_time(0);
                    assert!(
                        handle.window().get_task_model().labels_open,
                        "typed precondition: the labels overlay is open over the task"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "labels overlay open over the task detail; the detail's label chips beneath",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("labels-overlay", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

// -- golden wave 2 (round 7 G2, #1312): the review-critical states -----
//
// R1–R6 (the golden-coverage review report §6). Recorded substitutions
// (the plan §4 decision convention): R2b's "frequency dropdown flyout"
// does not exist — the recur editor's pattern picker is a
// SegmentedControl, and the window-level dropdown flyout's task-detail
// consumers are headless-unreachable (#1397) — so the anchored-over-
// modal invariant is captured on the recur editor's own detach-arm
// consequence strip (`recur-editor-detach-armed-1.0-1000`; the
// per-test doc comment carries the full record). R5's kinds wall is
// extended to the full vendored asset set first (the probe commit
// rides the same regen as `icon-probe-1.0`'s re-base).

/// The habit fixture (the habit-surface journeys' twin, pinned): two
/// live daily habits on the fixture board — one logged to its quota
/// (the streak chip's complete reading), one unlogged — so both
/// affordance postures render.
fn wave2_habits_state() -> taskboard_domain::AppState {
    use taskboard_domain::habit::{Habit, HabitClocks, HabitLogEntry, HabitLogKey, WindowStart};
    use taskboard_domain::{HabitId, NonZeroCount, PeriodKind};

    let (fixture, mut state) = fixture::golden_state_enriched_at(fixture::PINNED_NOW);
    let board = state
        .stacks
        .get(&fixture.stack_a)
        .expect("the fixture stack")
        .board;
    let mut ids = Vec::new();
    for (i, (title, quota)) in [("hydrate", 2_u32), ("stretch", 1)].into_iter().enumerate() {
        let habit = Habit {
            id: HabitId::from_uuid(uuid::Uuid::from_u128(
                u128::try_from(0xB000 + i).unwrap_or(0),
            )),
            board,
            title: title.to_owned(),
            description: String::new(),
            target: taskboard_domain::HabitTarget::PerDay(
                NonZeroCount::new(quota).expect("quota >= 1"),
            ),
            order: i64::try_from(i).expect("two habits"),
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(fixture::PINNED_NOW),
            continued_from: None,
            deleted: false,
            paused_until: None,
            by: None,
        };
        let id = habit.id;
        state.habits.habits.insert(id, habit);
        ids.push((id, quota));
    }
    // The logged habit: the first habit carried to its quota (the
    // streak chip's complete reading; the log row fixture shape).
    let (first, quota) = ids[0];
    state.habits.log.insert(
        HabitLogKey {
            habit: first,
            window: WindowStart::of_period(PeriodKind::Day, fixture::PINNED_NOW.date_naive()),
        },
        HabitLogEntry {
            count: quota,
            clock: fixture::PINNED_NOW,
            by: None,
            skip: false,
        },
    );
    state
}

/// R1a — the today view's habit rows at the desktop posture: streak
/// chips mounted and the editing affordances enabled (the
/// presentation law's enabled half).
#[test]
fn the_today_habits_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(wave2_habits_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    driver.advance_time(0);
                    assert!(
                        handle.window().get_habits_model().row_count() > 0,
                        "typed precondition: the habit rows are mounted"
                    );
                    assert!(
                        handle.window().get_habits_editing_enabled(),
                        "typed precondition: the desktop posture enables the habit affordances"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at + two daily habits (one at quota)",
                    state: "today view; habit rows + streak chips, desktop affordances enabled",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("today-habits", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R1b — the kiosk rotation's today mode with habit rows: the gated
/// posture renders the affordances disabled, not hidden (the
/// presentation law's kiosk half), at the 0.5 scale cell.
#[test]
fn the_today_habits_kiosk_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(wave2_habits_state()));
            capture_golden(
                driver,
                state,
                kiosk_rotation_settings(),
                |driver, handle| {
                    // One dwell step past the flip (the K13 idiom):
                    // modes[0] Board -> Today.
                    driver.advance_time(1_000);
                    driver.advance_time(0);
                    assert_eq!(
                        handle.window().get_current_view(),
                        View::Today,
                        "typed precondition: the rotation landed on the today mode"
                    );
                    assert!(
                        handle.window().get_habits_model().row_count() > 0,
                        "typed precondition: the habit rows are mounted in kiosk"
                    );
                    assert!(
                        !handle.window().get_habits_editing_enabled(),
                        "typed precondition: the kiosk posture disables the habit affordances"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at + two daily habits (one at quota)",
                    state: "kiosk rotation today mode; habit rows with the gated (disabled) affordances",
                    scale: 0.5,
                },
                1000,
                &wave2_golden_path("today-habits-kiosk", 0.5, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R2a — the recur editor modal over the enriched fixture's recurring
/// task: the walk reaches the daily series on stack b, the editor
/// opens with the staged daily rule (closed pattern picker).
#[test]
fn the_recur_editor_modal_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            // The recurring series staged on the walk's landing row
            // (`stage_daily_series_on_first_task`'s determinism law).
            let mut staged = enriched_state();
            stage_daily_series_on_first_task(&mut staged);
            let state = Arc::new(ArcSwap::from_pointee(staged));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    handle.dispatch_action(ActionId::NavFirstItem);
                    handle.dispatch_action(ActionId::TaskEdit);
                    // The subject's recurring badge is the staged
                    // rule's typed presence (a rule-less walk target
                    // would mint a fresh series per run — the drift
                    // law above).
                    assert!(
                        handle.window().get_task_model().recur_visible,
                        "typed precondition: the walked-into task carries the staged series"
                    );
                    handle.dispatch_action(ActionId::SeriesEdit);
                    driver.advance_time(0);
                    assert!(
                        handle.window().get_recur_editor_open(),
                        "typed precondition: the recur editor modal is open"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "recur editor modal over the daily series task; closed pattern picker",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("recur-editor-modal", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R2b — the recur editor's second staged state: the stop-repeating
/// consequence strip armed over the modal (the detach arm's
/// flyout-shaped consequence strip, anchored over the editor chrome).
///
/// Recorded substitution (the plan §4 convention, twice over): the
/// review report's "frequency dropdown flyout" does not exist — the
/// recur editor's pattern picker is a `SegmentedControl`, and the
/// window-level dropdown flyout's task-detail consumers are not
/// reachable in the headless harness (the pushed flag's changed
/// mirror is not delivered there — the unreliable-mirroring record;
/// both the dispatch route and the field's own open arms leave the
/// facts closed). The invariant "a consequence strip anchored over
/// its modal" is captured on the detach arm instead.
#[test]
fn the_recur_detach_strip_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            // The recurring series staged on the walk's landing row
            // (`stage_daily_series_on_first_task`'s determinism law).
            let mut staged = enriched_state();
            stage_daily_series_on_first_task(&mut staged);
            let state = Arc::new(ArcSwap::from_pointee(staged));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    handle.dispatch_action(ActionId::NavFirstItem);
                    handle.dispatch_action(ActionId::TaskEdit);
                    handle.dispatch_action(ActionId::SeriesEdit);
                    handle.dispatch_action(ActionId::SeriesEditDetachArm);
                    driver.advance_time(0);
                    assert!(
                        handle.window().get_recur_editor_open(),
                        "typed precondition: the recur editor modal is open"
                    );
                    assert!(
                        handle.window().get_recur_editor_model().detach_armed,
                        "typed precondition: the stop-repeating strip is armed"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "recur editor modal; the stop-repeating consequence strip armed",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("recur-editor-detach-armed", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R3a — the armed confirm strip in its danger tone: the board-reset
/// ceremony letter over the enriched board (the proven flow; the
/// strip's danger caption carries the live stack/task/label counts).
#[test]
fn the_confirm_strip_danger_golden_holds() {
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
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    handle.dispatch_action(ActionId::BoardReset);
                    driver.advance_time(0);
                    assert_eq!(
                        handle.window().get_ceremony_letter().mode,
                        taskboard_ui_slint::CeremonyLetterMode::ResetCounts,
                        "typed precondition: the reset ceremony letter is armed"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "board view; the armed reset ceremony strip (danger tone)",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("confirm-strip-danger", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R3b — the armed confirm strip in its success tone: the reconcile
/// ceremony letter with the planner's match counts (the oneshot
/// preview answered hermetically — the request lane the mount hands
/// the test, answered and drained through the public seam; the plan
/// §4 decision). The mount is bespoke for that lane.
#[test]
fn the_confirm_strip_success_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(enriched_state()));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, mut requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = taskboard_ui_slint::mount(
                Arc::clone(&state),
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
            handle.dispatch_action(ActionId::NavFocusBoard);
            handle.dispatch_action(ActionId::BoardReconcile);
            // Answer the preview oneshot: two stack pairs, one label
            // pair, three task pairs — the letter's merge counts.
            let Ok(taskboard_ui_slint::requests::UiRequest::BoardReconcilePreview {
                reply, ..
            }) = requests_rx.try_recv()
            else {
                panic!("the reconcile arm routed a preview request");
            };
            reply
                .send(Some(taskboard_domain::ReconcileCounts {
                    stacks: 2,
                    labels: 1,
                    tasks: 3,
                    cancelled_ops: 0,
                }))
                .expect("the reply lane is alive");
            handle.drain_reconcile_preview();
            driver.advance_time(0);
            assert_eq!(
                handle.window().get_ceremony_letter().mode,
                taskboard_ui_slint::CeremonyLetterMode::ReconcilePreview,
                "typed precondition: the reconcile ceremony letter is armed"
            );
            assert!(
                handle.window().get_ceremony_letter().merge_tasks > 0,
                "typed precondition: the letter carries the merge counts"
            );
            driver.set_window_size(1000, HEIGHT);
            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1000, HEIGHT),
                "renderer-presence contract at the success-strip golden"
            );
            let path = wave2_golden_path("confirm-strip-success", 1.0, 1000);
            if regen {
                let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
                std::fs::write(&path, bytes).expect("writes");
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_enriched_at(PINNED_NOW)",
                        state: "board view; the armed reconcile ceremony strip (success tone) with merge counts",
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

/// R4a — the empty board: zero stacks over the empty-board fixture,
/// the `EmptyState`'s alignment leg (round-3 R5/F10's surface).
#[test]
fn the_board_empty_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(fixture::empty_board_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    driver.advance_time(0);
                    let board = handle.window().get_board_model();
                    assert_eq!(
                        board.stacks.row_count(),
                        0,
                        "typed precondition: the board has no stacks"
                    );
                },
                &GoldenMeta {
                    fixture: "empty_board_state()",
                    state: "board view; zero stacks, the EmptyState alignment",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("board-empty", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R4b — the today view with zero rows: the no-tasks posture over the
/// same empty fixture (the desktop default is the subject).
#[test]
fn the_today_empty_golden_holds() {
    pin_utc_zone();
    let regen = regen_requested();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver| {
            let state = Arc::new(ArcSwap::from_pointee(fixture::empty_board_state()));
            capture_golden(
                driver,
                state,
                scale_settings(1.0),
                |driver, handle| {
                    driver.advance_time(0);
                    assert_eq!(
                        handle.window().get_current_view(),
                        View::Today,
                        "typed precondition: the today view is mounted"
                    );
                },
                &GoldenMeta {
                    fixture: "empty_board_state()",
                    state: "today view; zero rows, the no-tasks posture",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("today-empty", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}

/// R4c — the search palette's empty results: the palette open with a
/// typed query matching nothing (the empty-results body), the same
/// typing seam as the populated sibling golden.
#[test]
fn the_search_palette_empty_golden_holds() {
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
            for ch in "zzzz".chars() {
                driver.press_stroke(Stroke {
                    key: ch,
                    ctrl: false,
                    alt: false,
                    shift: false,
                    meta: false,
                });
            }
            driver.advance_time(0);
            assert_eq!(
                handle.window().get_palette_model().row_count(),
                0,
                "typed precondition: the query matches nothing"
            );
            driver.set_window_size(1000, HEIGHT);
            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1000, HEIGHT),
                "renderer-presence contract at the empty-palette golden"
            );
            let path = wave2_golden_path("search-palette-empty", 1.0, 1000);
            if regen {
                let bytes = golden_compare::encode_png(&snapshot).expect("encodes");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
                std::fs::write(&path, bytes).expect("writes");
                manifest_record(
                    &path,
                    &GoldenMeta {
                        fixture: "golden_state_enriched_at(PINNED_NOW)",
                        state: "search palette open; typed query with zero hits",
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

/// R6 — the board's insert indicator lit mid-move: the move flow
/// armed from the focused first card (the dispatch-first law; no
/// pointer synthesis), the aimed column's count badge flipped and the
/// insertion hairline at the aimed gap (the models' aim-gap path).
#[test]
fn the_board_drag_indicator_golden_holds() {
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
                    handle.dispatch_action(ActionId::NavFocusBoard);
                    handle.dispatch_action(ActionId::NavFirstItem);
                    handle.dispatch_action(ActionId::TaskMove);
                    // One slot step down: the gap sits between the
                    // first and second cards (a mid-list aim, not the
                    // trivial top edge).
                    handle.dispatch_action(ActionId::TaskMoveDown);
                    driver.advance_time(0);
                    let board = handle.window().get_board_model();
                    let aimed: Vec<_> = (0..board.stacks.row_count())
                        .filter_map(|i| board.stacks.row_data(i))
                        .collect();
                    assert!(
                        aimed.iter().any(|column| column.move_aim),
                        "typed precondition: a column is the armed move's aim"
                    );
                    assert!(
                        aimed.iter().any(|column| column.aim_gap >= 0),
                        "typed precondition: an insertion gap is aimed"
                    );
                },
                &GoldenMeta {
                    fixture: "golden_state_enriched_at(PINNED_NOW)",
                    state: "board view; the move flow armed, insertion indicator at the aimed gap",
                    scale: 1.0,
                },
                1000,
                &wave2_golden_path("board-drag-indicator", 1.0, 1000),
                regen,
            );
        },
    );
    assert!(
        !regen,
        "goldens regenerated — review the PNG diff before landing"
    );
}
