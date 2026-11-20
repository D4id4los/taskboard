// SPDX-License-Identifier: AGPL-3.0-only

//! The menu-shell probe suite (round-9 W6, plan §4 G-1..G-3): the
//! `MenuRow` anatomy contract, the `MenuShell` chrome + height cap,
//! and the `AnchorMirror` publish edges. Every assertion is a typed root
//! seam, a debug-info element query, or a structural relation — no
//! text assertions, no clocks (AGENTS §5).

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock, find_all_by_id_on, find_by_label_on};

include!(concat!(env!("OUT_DIR"), "/menu_shell_probe.rs"));

fn run_probe(job: impl FnOnce(&MenuShellProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = MenuShellProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// Forces the testing backend's geometry pass so the realized rects
/// are current (the widget-framework probe's recorded idiom).
fn settle(window: &MenuShellProbe) {
    advance_probe_clock(0);
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            // The bottom edge: never on a probe row (a (1,1) park
            // would engage the top row's hover and pollute the
            // counters).
            position: slint::LogicalPosition::new(1.0, 99999.0),
        });
    advance_probe_clock(0);
}

/// Dispatches a synthetic left click at the window coordinates (the
/// labeled-value-row probe's recorded dispatch pattern): the mock
/// platform delivers the click on the tick between press and
/// release.
fn click_at(window: &MenuShellProbe, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let p = slint::LogicalPosition::new(x, y);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
    advance_probe_clock(1);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    advance_probe_clock(1);
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: p,
            button: PointerEventButton::Left,
        });
    advance_probe_clock(1);
}

/// Dispatches a right-button press (the context seam's down half —
/// the row's `pointer-event` arm acts on the press, not the release).
fn right_press_at(window: &MenuShellProbe, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let p = slint::LogicalPosition::new(x, y);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
    advance_probe_clock(1);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Right,
    });
    advance_probe_clock(1);
}

// ---- G-1: MenuRow anatomy -------------------------------------------

/// The hover union: moving over the row arms `hovered` with the
/// row's realized window-space rect (the tooltip trigger's payload)
/// and leaving reports `unhovered` — the tooltip-outlives-row
/// contract. The background itself rides `highlighted` only (the
/// consumer's union binding), so a bare hover does not paint.
#[test]
fn the_row_hover_reports_the_rect_and_leaving_reports_unhovered() {
    run_probe(|window| {
        settle(window);
        assert_eq!(
            window.get_plain_row_background(),
            slint::Color::from_argb_encoded(0)
        );
        let (x, y, w, h) = (
            window.get_plain_row_x(),
            window.get_plain_row_y(),
            window.get_plain_row_width(),
            window.get_plain_row_height(),
        );
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x + w / 2.0, y + h / 2.0),
            });
        // The changed-callback pass is synchronous with the event
        // (the dropdown probe's recorded delivery law).
        assert!(window.get_plain_row_hovered(), "the row reports its hover");
        assert!(!window.get_plain_row_highlighted());
        // The payload is the row's realized rect (the tooltip arms on
        // exactly today's shape).
        assert_eq!(window.get_plain_hovered_count(), 1);
        assert_eq!(window.get_plain_hover_x(), x);
        assert_eq!(window.get_plain_hover_y(), y);
        assert_eq!(window.get_plain_hover_width(), w);
        assert_eq!(window.get_plain_hover_height(), h);
        // Leaving the row reports the unhover (the tooltip dies).
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                // Park off-surface: (1,1) is on the top probe row.
                position: slint::LogicalPosition::new(1.0, 99999.0),
            });
        assert_eq!(window.get_plain_unhovered_count(), 1);
        assert!(!window.get_plain_row_hovered());
    });
}

/// The `highlighted` driver paints the hover-union background: the
/// consumer's union binding (`self.hovered || ladder match`) is the
/// only input.
#[test]
fn the_highlighted_driver_paints_the_hover_background() {
    run_probe(|window| {
        settle(window);
        window.set_plain_row_highlight_driver(true);
        settle(window);
        assert_eq!(
            window.get_plain_row_background(),
            window.get_theme_surface_hover()
        );
        window.set_plain_row_highlight_driver(false);
        settle(window);
        assert_eq!(
            window.get_plain_row_background(),
            slint::Color::from_argb_encoded(0)
        );
    });
}

/// The click invokes (the pointer path) and the right-button press
/// routes the context seam with the pointer position in logical
/// pixels (the flyout rows' (/1px) conversion, verbatim).
#[test]
fn the_row_routes_invoked_and_context_edges() {
    run_probe(|window| {
        settle(window);
        let (x, y) = (window.get_plain_row_x(), window.get_plain_row_y());
        let (cx, cy) = (x + 8.0, y + 8.0);
        click_at(window, cx, cy);
        settle(window);
        assert_eq!(window.get_plain_invoked_count(), 1);
        right_press_at(window, cx, cy);
        settle(window);
        assert_eq!(window.get_plain_context_count(), 1);
        // The payload is the pointer position (the /1px conversion);
        // the comparison stays in the payload's integer space.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::similar_names
        )]
        let (press_x, press_y) = (cx as i32, cy as i32);
        assert_eq!(window.get_plain_context_x(), press_x);
        assert_eq!(window.get_plain_context_y(), press_y);
    });
}

/// The disabled law: the touch is dead (a click never invokes) and
/// hover cannot engage (`has-hover` needs an enabled `TouchArea`).
#[test]
fn the_disabled_row_never_invokes_and_never_hovers() {
    run_probe(|window| {
        settle(window);
        // The disabled row sits second in the probe column: one
        // space-6 row pitch below the plain row.
        let y = window.get_plain_row_y() + window.get_plain_row_height();
        click_at(window, window.get_plain_row_x() + 8.0, y + 8.0);
        settle(window);
        assert_eq!(window.get_disabled_invoked_count(), 0);
        assert!(!window.get_disabled_row_hoverable());
    });
}

/// The check glyph renders only where `selected` is pushed (the
/// dropdown class's law — menus pass nothing, D-12's glyph scope).
#[test]
fn the_check_glyph_is_selected_only() {
    run_probe(|window| {
        settle(window);
        let glyphs = find_all_by_id_on(window, "MenuRow::check-glyph");
        assert_eq!(glyphs.len(), 1, "the one selected row's glyph");
    });
}

/// The submenu chevron mounts only on `submenu` rows, and the
/// leading slot follows the busy-over-glyph precedence with the
/// collapse law (menus carry no slot at all).
#[test]
fn the_chevron_and_leading_slot_follow_their_laws() {
    run_probe(|window| {
        settle(window);
        let chevrons = find_all_by_id_on(window, "MenuRow::submenu-chevron");
        assert_eq!(chevrons.len(), 1, "the plain row's chevron");
        // The busy row carries the dot, never the glyph; the glyph
        // row carries the indicator; the plain row carries neither.
        let dots = find_all_by_id_on(window, "MenuRow::busy-dot");
        assert_eq!(dots.len(), 1, "the busy row's pulse dot");
        let glyph_icons = find_all_by_id_on(window, "MenuRow::leading-glyph-icon");
        assert_eq!(glyph_icons.len(), 1, "the glyph row's indicator");
    });
}

/// The a11y posture: the button-role row carries its accessible
/// label and default action (the driver finds and invokes it — the
/// action menu's surface); the list-item rows' posture is the
/// instantiation's declaration (not re-asserted here — the flyout
/// journeys cover it at the mount).
#[test]
fn the_button_row_carries_the_accessible_label_and_default_action() {
    run_probe(|window| {
        settle(window);
        let row = find_by_label_on(window, "Plain probe row").expect("the button row mounts");
        row.invoke_default_action();
        settle(window);
        assert_eq!(window.get_plain_invoked_count(), 1);
    });
}

/// The row's height law: one space-6 row pitch (the flyouts' pin).
#[test]
fn the_row_is_one_space_six_pitch() {
    run_probe(|window| {
        settle(window);
        assert_eq!(window.get_list_row_height(), window.get_theme_space_6());
        assert_eq!(window.get_plain_row_height(), window.get_theme_space_6());
    });
}

// ---- G-2: MenuShell chrome + cap ------------------------------------

/// The chrome: raised surface, focus-ring border, the consumer's
/// corner radius, clip, and the drop shadow (the flyouts' exact
/// paint).
#[test]
fn the_shell_paints_the_flyout_chrome() {
    run_probe(|window| {
        settle(window);
        assert_eq!(
            window.get_shell_background(),
            window.get_theme_surface_raised()
        );
        assert_eq!(
            window.get_shell_border_width(),
            window.get_theme_focus_ring_width()
        );
        assert_eq!(window.get_shell_radius(), window.get_theme_radius_small());
        assert!(window.get_shell_clips());
        // The drop shadow's blur is not a readable seam on the
        // testing backend (the getter reflects no value) — the
        // shadow's identity evidence is the verbatim chrome move plus
        // the flyout goldens, not a typed read here.
    });
}

/// The height cap: below the six-row threshold the shell hugs the
/// row count; at and above it the cap pins at space-6·6.
#[test]
fn the_shell_caps_at_six_rows() {
    run_probe(|window| {
        settle(window);
        let row = window.get_theme_space_6();
        // Below: 3 rows → 3·space-6.
        window.set_shell_row_count(3);
        settle(window);
        assert_eq!(window.get_shell_height(), row * 3.0);
        // At: 6 rows → 6·space-6 (the cap boundary).
        window.set_shell_row_count(6);
        settle(window);
        assert_eq!(window.get_shell_height(), row * 6.0);
        // Above: 9 rows → still 6·space-6.
        window.set_shell_row_count(9);
        settle(window);
        assert_eq!(window.get_shell_height(), row * 6.0);
    });
}

// ---- G-3: AnchorMirror edges ----------------------------------------

/// The publish edges: `published` fires once on init with the bound
/// rect (the mount-time publish), and on every source change (the
/// keep-alive contract) with the full four-tuple.
#[test]
fn the_mirror_publishes_on_init_and_on_every_source_change() {
    run_probe(|window| {
        settle(window);
        // Init: one publish with the initial binding values.
        assert_eq!(window.get_mirror_publish_count(), 1);
        assert_eq!(window.get_mirror_last_x(), window.get_mirror_source_x());
        assert_eq!(window.get_mirror_last_y(), window.get_mirror_source_y());
        assert_eq!(
            window.get_mirror_last_width(),
            window.get_mirror_source_width()
        );
        assert_eq!(
            window.get_mirror_last_height(),
            window.get_mirror_source_height()
        );
        // Each source change re-publishes the full rect (the value
        // rides the token arithmetic: one space-6 step per change).
        let step = window.get_theme_space_6();
        window.set_mirror_source_x(window.get_mirror_source_x() + step);
        settle(window);
        assert_eq!(window.get_mirror_publish_count(), 2);
        assert_eq!(window.get_mirror_last_x(), window.get_mirror_source_x());
        window.set_mirror_source_height(window.get_mirror_source_height() + step);
        settle(window);
        assert_eq!(window.get_mirror_publish_count(), 3);
        assert_eq!(
            window.get_mirror_last_height(),
            window.get_mirror_source_height()
        );
    });
}
