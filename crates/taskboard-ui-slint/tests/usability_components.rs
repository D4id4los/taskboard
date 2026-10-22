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

use slint::ComponentHandle as _;

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

/// G4 — the tooltip state machine (plan D3): hover arms a 600 ms
/// delay, the mock clock fires it, unhover dismisses, an empty label
/// never arms, and a re-hover inside the delay window restarts it.
/// Typed state only — the `TooltipState` global's accessors.
#[test]
fn the_tooltip_delay_dismissal_and_restart_behave() {
    use slint::ModelRc;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let handle = mount_populated(driver, 1.0, 1000, 900);
            let tooltip = handle.window().global::<taskboard_ui_slint::TooltipState>();

            // The delay: visible only after 600 ms of hover. The arm
            // is invocation-driven, so the restart below is exact.
            tooltip.invoke_hover(
                "New stack".into(),
                "".into(),
                ModelRc::default(),
                10.0,
                10.0,
                32.0,
                32.0,
            );
            driver.advance_time(599);
            assert!(!tooltip.get_visible(), "still inside the delay window");
            driver.advance_time(1);
            assert!(tooltip.get_visible(), "the delay fired");

            // Dismissal: unhover hides and disarms.
            tooltip.invoke_unhover();
            assert!(!tooltip.get_visible());

            // An empty label never arms (a control without a label
            // never shows a tooltip).
            tooltip.invoke_hover(
                "".into(),
                "".into(),
                ModelRc::default(),
                10.0,
                10.0,
                32.0,
                32.0,
            );
            driver.advance_time(600);
            assert!(!tooltip.get_visible(), "empty label stays hidden");

            // Re-hover inside the delay window restarts it.
            tooltip.invoke_hover(
                "Command menu".into(),
                "".into(),
                ModelRc::default(),
                40.0,
                10.0,
                32.0,
                32.0,
            );
            driver.advance_time(599);
            tooltip.invoke_hover(
                "Command menu".into(),
                "".into(),
                ModelRc::default(),
                40.0,
                10.0,
                32.0,
                32.0,
            );
            driver.advance_time(599);
            assert!(!tooltip.get_visible(), "the restart reset the delay window");
            driver.advance_time(1);
            assert!(tooltip.get_visible());
            tooltip.invoke_unhover();

            // Placement (usability D3): a toolbar-anchored tooltip —
            // near the window's top — renders *below* its anchor, in
            // the toolbar strip between the anchor's bottom edge and
            // the toolbar's end. The pre-fix flip threshold compared
            // against the status bar's height instead of its top
            // edge, flipping this tooltip off the window's top. The
            // panel is the only Theme.surface-raised (#262a35) surface
            // in that strip at this state.
            tooltip.invoke_hover(
                "New stack".into(),
                "".into(),
                ModelRc::default(),
                10.0,
                10.0,
                32.0,
                32.0,
            );
            driver.advance_time(600);
            assert!(tooltip.get_visible());
            let snapshot = driver.snapshot();
            let below_anchor = (43..64).any(|y| row_run_of(&snapshot, SURFACE_RAISED, y) > 0);
            assert!(
                below_anchor,
                "the tooltip paints below its near-the-top anchor, not flipped off the window"
            );
            tooltip.invoke_unhover();
        },
    );
}

/// Round-1 D8/D10 — the generalized trigger's parts and the placement
/// hand-off: an over-wide label at a small window lands placed, inside
/// the window margins; the keys part is present exactly when handed a
/// non-empty model; `placed` never precedes the delay firing.
#[test]
fn the_tooltip_hand_off_places_inside_the_window_and_carries_parts() {
    use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let handle = mount_populated(driver, 1.0, 400, 300);
            let tooltip = handle.window().global::<taskboard_ui_slint::TooltipState>();

            // While pending: armed but neither visible nor placed.
            let keys = ModelRc::new(VecModel::from(vec![SharedString::from("Ctrl+K")]));
            tooltip.invoke_hover(
                "an absurdly long tooltip label that cannot possibly fit a 400 px window without wrapping somewhere"
                    .into(),
                "".into(),
                keys,
                200.0,
                20.0,
                32.0,
                32.0,
            );
            assert!(tooltip.get_pending());
            assert!(!tooltip.get_visible());
            assert!(!tooltip.get_placed(), "nothing places before the delay");

            driver.advance_time(600);
            driver.advance_time(0); // the size hand-off's change pass
            assert!(tooltip.get_visible());
            assert!(tooltip.get_placed(), "the placement landed");
            let (x, y) = (tooltip.get_tip_x(), tooltip.get_tip_y());
            assert!(x >= 8.0 && y >= 8.0, "tip at the margins or past: {x},{y}");
            tooltip.invoke_unhover();

            // The keys part is carried verbatim and empties on the
            // next hover without one (shown only when a binding
            // exists).
            assert_eq!(
                tooltip.get_keys().row_count(),
                1,
                "the handed model is carried"
            );
            tooltip.invoke_hover(
                "plain".into(),
                "a description".into(),
                ModelRc::default(),
                200.0,
                20.0,
                32.0,
                32.0,
            );
            assert_eq!(
                tooltip.get_keys().row_count(),
                0,
                "an empty keys model renders no hint part"
            );
            assert_eq!(tooltip.get_description(), "a description");
            tooltip.invoke_unhover();
        },
    );
}

/// Round-1 D7/D8 — the row tooltip: hovering somewhere over the open
/// menu's rows arms the generalized trigger whose description is
/// non-empty and whose keys are the row's binding hint (structural;
/// the text itself is never asserted).
#[test]
fn hovering_a_menu_row_arms_the_entry_tooltip() {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let handle = mount_populated(driver, 1.0, 1000, 900);
            let tooltip = handle.window().global::<taskboard_ui_slint::TooltipState>();
            handle.window().invoke_toolbar_menu_clicked();
            driver.advance_time(0);

            let facts = handle
                .window()
                .global::<taskboard_ui_slint::MenuPanelFacts>();
            let (px, pw) = (facts.get_x(), facts.get_width());
            let (py, ph) = (facts.get_y(), facts.get_height());
            let mut armed = false;
            // Scan the panel's interior: some enabled row engages
            // hover and arms the trigger.
            let mut y = py + 8.0;
            while y < py + ph - 8.0 {
                driver.hover_at(px + pw / 2.0, y);
                driver.advance_time(0);
                if tooltip.get_pending() {
                    armed = true;
                    break;
                }
                y += 6.0;
            }
            assert!(armed, "a row hover arms the tooltip");
            assert!(
                !tooltip.get_description().is_empty(),
                "the armed tooltip carries the entry's description"
            );
            tooltip.invoke_unhover();
        },
    );
}

/// The focus-ring colour, Theme.focus-ring #7aa2f7 — the menu panel's
/// border and the selected row's ring are the only renderers of it.
const FOCUS_RING: [u8; 3] = [0x7a, 0xa2, 0xf7];

/// Theme.surface-raised #262a35 — the tooltip panel's background.
const SURFACE_RAISED: [u8; 3] = [0x26, 0x2a, 0x35];

/// One horizontal run's length of exactly `rgb` in row `y`, and the
/// min/max y of any `rgb` pixel in the image — the pixel vocabulary
/// of the menu proofs (no element-geometry API exists, plan §1.7).
fn row_run_of(image: &Rgba8Image, rgb: [u8; 3], y: u32) -> u32 {
    let mut best = 0u32;
    let mut run = 0u32;
    for x in 0..image.width {
        let i = (y as usize * image.width as usize + x as usize) * 4;
        let matches =
            image.data[i] == rgb[0] && image.data[i + 1] == rgb[1] && image.data[i + 2] == rgb[2];
        if matches {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

fn vertical_extent_of(image: &Rgba8Image, rgb: [u8; 3]) -> (u32, u32) {
    let (mut min, mut max) = (image.height, 0u32);
    for y in 0..image.height {
        if row_run_of(image, rgb, y) > 0 {
            min = min.min(y);
            max = max.max(y);
        }
    }
    (min, max)
}

/// G6 — the menu never renders beyond the available space (plan D6,
/// dispatch issue 5): at the kiosk netbook's real geometries and the
/// golden matrix's scale extremes, the open menu's focus-ring border
/// box closes (a bottom border exists — the old unbounded menu ran
/// off the window) and stays above the status bar's top edge.
#[test]
fn the_open_menu_closes_its_box_above_the_status_bar_at_every_geometry() {
    for (scale, width, height) in [
        (1.0, 819, 480),  // kiosk netbook through winit's 1.25 — the report
        (1.0, 1024, 600), // the kiosk panel, raw
        (0.5, 600, 900),  // golden-matrix extremes
        (3.0, 600, 900),
    ] {
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |driver: &mut SlintDriver| {
                let handle = mount_populated(driver, scale, width, height);
                handle.window().invoke_toolbar_menu_clicked();
                driver.advance_time(0);
                let snapshot = driver.snapshot();
                let (top, bottom) = vertical_extent_of(&snapshot, FOCUS_RING);
                assert!(
                    top < bottom,
                    "the menu border box exists at {width}×{height}"
                );
                // The bottom border is a long closed run, not a stray
                // ring fragment: the panel is at least its min-width
                // minus clipping, and the border closes it.
                let bottom_run = row_run_of(&snapshot, FOCUS_RING, bottom);
                assert!(
                    bottom_run >= 100,
                    "the menu's bottom border closes the box at {width}×{height} (run {bottom_run})"
                );
                let status_top = handle.window().get_status_top();
                #[allow(clippy::cast_precision_loss)] // UI-scale px values
                let bottom_px = bottom as f32;
                assert!(
                    bottom_px < status_top,
                    "the menu bottom ({bottom}) stays above the status bar top ({status_top}) at {width}×{height}"
                );
            },
        );
    }
}

/// G7 — the cursor follows the selection into the scrolled region
/// (plan D6, §7.3): with the menu open at the kiosk geometry, jumping
/// the selection to the last entry scrolls it fully inside the
/// panel's border box.
#[test]
fn the_menu_cursor_follows_the_selection_into_the_scroll() {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            use slint::ComponentHandle as _;
            use slint::Model as _;
            let handle = mount_populated(driver, 1.0, 819, 480);
            handle.window().invoke_toolbar_menu_clicked();
            driver.advance_time(0);

            let keyboard = handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>();
            let rows = keyboard.get_menu_entries().row_count();
            assert!(rows > 1, "the populated board offers a multi-row menu");
            keyboard.set_menu_selected_index(i32::try_from(rows - 1).unwrap_or(i32::MAX));
            driver.advance_time(0); // the follow logic runs on the change pass

            let snapshot = driver.snapshot();
            let (_, panel_bottom) = vertical_extent_of(&snapshot, FOCUS_RING);
            assert!(panel_bottom > 0, "the bordered panel rendered");
            // The last row scrolled into view: its ring renders just
            // above the panel's bottom border, inside the box (the
            // ring and the border are the only focus-ring renderers).
            let near_bottom_run =
                row_run_of(&snapshot, FOCUS_RING, panel_bottom.saturating_sub(20));
            assert!(
                near_bottom_run > 0,
                "the selected row's ring stayed inside the panel near its bottom edge"
            );
        },
    );
}
