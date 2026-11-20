// SPDX-License-Identifier: AGPL-3.0-only

//! The labeled-value-row probe suite (round-9 W4, plan §3): the
//! widget contract's catalogue over the three modes — display
//! rendering + the em-dash placeholder, the role/colour forwarding,
//! the trailing slot + edit-asked intent, the cell's stop/ring/restore
//! law, the geometry seams, the tooltip arm, the editor's `@children`
//! field slot, and the elide/dash law. Every assertion is a typed root
//! seam or a structural relation — no text assertions, no clocks
//! (AGENTS §5).

mod support;

use slint::{ComponentHandle as _, Model as _};
use support::driver::{
    DriverOptions, advance_probe_clock, centre, find_all_by_label_on, find_by_label_on,
};

include!(concat!(env!("OUT_DIR"), "/labeled_value_row_probe.rs"));

use slint::platform::{PointerEventButton, WindowEvent};

fn run_probe(job: impl FnOnce(&LabeledValueRowProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let window = LabeledValueRowProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// Forces the testing backend's geometry pass (the om probe's idiom).
fn settle(window: &LabeledValueRowProbe) {
    advance_probe_clock(0);
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(0.0, 0.0),
    });
    advance_probe_clock(0);
}

fn pointer(window: &LabeledValueRowProbe, x: f32, y: f32) {
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    });
}

fn click(window: &LabeledValueRowProbe, x: f32, y: f32) {
    let p = slint::LogicalPosition::new(x, y);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    // The mock platform delivers the click on the tick between press
    // and release (the board-bar probe's recorded dispatch pattern).
    advance_probe_clock(1);
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: p,
            button: PointerEventButton::Left,
        });
    advance_probe_clock(1);
}

/// The display row (the tracked row's grammar: caption-beside, mono
/// echo, tooltip arm, a11y presence, trailing pencil) renders its
/// value region and carries the em-dash placeholder on an empty value.
#[test]
fn the_display_row_renders_the_value_region_and_dashes_the_empty_value() {
    run_probe(|window| {
        settle(window);
        assert!(
            window.get_display_value_width() > 0.0,
            "the value region renders"
        );
        assert!(
            window.get_display_value_height() > 0.0,
            "the value region has the value's height"
        );

        // The placeholder: an empty value arms the dash — the built-in
        // value claims the dash's width; the disarmed placeholder
        // keeps the raw text's (zero) width.
        window.set_display_value("".into());
        window.set_display_dash(true);
        window.set_display_hint("".into());
        window.set_display_a11y("".into());
        settle(window);
        let dashed = window.get_display_preferred_width();
        assert!(
            window.get_display_dash_out(),
            "the dash placeholder renders on the empty value"
        );
        window.set_display_dash(false);
        settle(window);
        let bare = window.get_display_preferred_width();
        assert!(
            dashed > bare,
            "the armed dash claims the placeholder's width: {dashed} <= {bare}"
        );
        assert!(
            !window.get_display_dash_out(),
            "the disarmed placeholder renders the raw text"
        );
    });
}

/// The value role and colour forward to the built-in value (the mono
/// echo row reads back the mono role's size and the mount's colour).
#[test]
fn the_value_role_and_colour_forward_to_the_built_in_value() {
    run_probe(|window| {
        settle(window);
        // The probe window rides the theme's base-scale default (1.0):
        // token arithmetic equals base px (the icon probe's recorded
        // convention).
        let size = window.get_display_role_size();
        assert_eq!(
            size, 12.5,
            "the mono role reaches the value: {size} != the mono token"
        );
        assert_eq!(
            window.get_display_color_out(),
            window.get_theme_text_secondary(),
            "the mount's colour reaches the value"
        );
    });
}

/// The trailing slot mounts the affordance and reports its intent;
/// the cell's activation (click ≡ the stop's default action) reports
/// `edit-asked` exactly once per gesture.
#[test]
fn the_trailing_slot_mounts_and_the_cell_reports_edit_asked() {
    run_probe(|window| {
        settle(window);
        assert_eq!(window.get_trailing_count(), 0, "no intent before any click");
        let pencil = find_by_label_on(window, "Probe trailing")
            .expect("the trailing affordance mounts inside the row");
        let (x, y) = centre(&pencil);
        // The pointer path: a raw click on the affordance (the mock
        // platform's move-tick-press-tick-release discipline).
        click(window, x, y);
        assert_eq!(window.get_trailing_count(), 1, "the trailing click routes");
        assert_eq!(
            window.get_edit_asked_count(),
            0,
            "the trailing affordance is not the cell's edit intent"
        );

        // The cell's pointer path: a raw click in the value region.
        let cx = window.get_cell_value_x() + window.get_cell_value_width() / 2.0;
        let cy = window.get_cell_value_y() + window.get_cell_value_height() / 2.0;
        click(window, cx, cy);
        assert_eq!(window.get_edit_asked_count(), 1, "the cell click asks");

        // The cell's keyboard path: the stop's accessible default
        // action (click ≡ key, §7.1).
        let stop = find_by_label_on(window, "Edit due date")
            .expect("the activation stop addresses the cell");
        stop.invoke_default_action();
        assert_eq!(
            window.get_edit_asked_count(),
            2,
            "the stop activation asks through the same seam"
        );
    });
}

/// The cell's focus law: the restore handshake focuses the internal
/// stop, the walk fact publishes the preserved stop id, and the ring
/// renders (the observation seams).
#[test]
fn the_cell_focus_publishes_the_stop_and_rings() {
    run_probe(|window| {
        let facts = window.global::<FocusFacts>();
        let walk = window.global::<TaskViewWalkFacts>();
        settle(window);
        assert!(!window.get_cell_focused(), "unfocused at mount");
        assert!(!window.get_cell_ringed(), "no ring at mount");
        assert_eq!(walk.get_stop_id(), 0, "no stop published at mount");

        // The focus-origin restore handshake (anchor kind 4 = the
        // title cell's restore target; the probe's cell arms it).
        facts.set_anchor_kind(4);
        let nonce = facts.get_nonce();
        facts.set_nonce(nonce + 1);
        settle(window);

        assert!(window.get_cell_focused(), "the restore focuses the stop");
        assert_eq!(
            walk.get_stop_id(),
            5,
            "the preserved stop id publishes on focus gain"
        );
        assert!(window.get_cell_ringed(), "the ring renders on focus");
    });
}

/// The geometry seams track the value region (the overlay-anchor
/// law): the cells' realized rects are live, ordered, and disjoint.
#[test]
fn the_geometry_seams_track_the_value_region() {
    run_probe(|window| {
        settle(window);
        assert!(window.get_cell_value_width() > 0.0);
        assert!(window.get_cell_value_height() > 0.0);
        // Vertical rows stack: the cell region sits strictly below the
        // display row's.
        assert!(
            window.get_cell_value_y() > window.get_display_value_y(),
            "the rows' seams track their regions"
        );
        // The beside axis: the editor's field slot starts right of the
        // caption side (the caption claims the left).
        assert!(
            window.get_editor_value_x() > window.get_editor_row_x(),
            "the editor's slot sits beside the caption"
        );
        assert!(window.get_editor_value_width() > 0.0);
        assert!(window.get_editor_value_height() > 0.0);
    });
}

/// The tooltip arm: hovering the display row's value region pushes
/// the state (the hint keys model + the region rect as the anchor);
/// leaving dismisses it.
#[test]
fn the_tooltip_arm_pushes_and_dismisses_on_hover() {
    run_probe(|window| {
        let tooltip = window.global::<TooltipState>();
        settle(window);
        assert!(!tooltip.get_pending(), "no tooltip at mount");

        let rx = window.get_display_value_x();
        let ry = window.get_display_value_y();
        let rw = window.get_display_value_width();
        let rh = window.get_display_value_height();
        pointer(window, rx + rw / 2.0, ry + rh / 2.0);
        advance_probe_clock(0);
        assert!(tooltip.get_pending(), "the hover arms the tooltip");
        assert_eq!(
            tooltip.get_anchor_x(),
            rx,
            "the anchor rect is the value region"
        );
        assert_eq!(tooltip.get_anchor_y(), ry);
        assert_eq!(tooltip.get_anchor_width(), rw);
        assert_eq!(tooltip.get_anchor_height(), rh);
        assert_eq!(
            tooltip.get_keys().row_count(),
            1,
            "the hint keys model forwards"
        );

        pointer(window, 1.0, 1.0);
        advance_probe_clock(0);
        assert!(!tooltip.get_pending(), "leaving dismisses the tooltip");
    });
}

/// The editor mode hosts the `@children` field beside the caption
/// (Rule E: the field stays at the mount) — the caption and the field
/// both address the row's label, and the slot stretches.
#[test]
fn the_editor_mode_hosts_children_beside_the_caption() {
    run_probe(|window| {
        settle(window);
        let addressed = find_all_by_label_on(window, "Effort Estimate");
        assert_eq!(
            addressed.len(),
            2,
            "the caption and the hosted field both address the row"
        );
        assert!(
            window.get_editor_value_width() > window.get_editor_row_x(),
            "the field slot stretches beyond the caption side"
        );
    });
}

/// The `StatCell` law (the re-host's customization points): the elide
/// knob reaches the built-in value, whose width bounds to the region
/// while the preferred width claims the full text.
#[test]
fn the_elide_law_bounds_the_value_to_the_region() {
    run_probe(|window| {
        settle(window);
        assert!(
            !window.get_display_elide_out(),
            "elide off by default (the echoes render preferred-width text)"
        );
        window.set_display_elide(true);
        settle(window);
        assert!(
            window.get_display_elide_out(),
            "the elide knob reaches the built-in value"
        );
        window.set_display_value("x".repeat(300).into());
        settle(window);
        let region = window.get_display_region_width();
        let preferred = window.get_display_preferred_width();
        assert!(
            preferred > region,
            "the long text claims more than the fixed host: {preferred} <= {region}"
        );
        // The elided value's layout width bounds to the region (the
        // StatCell elide law): the stretched region reads the fixed
        // host's width budget, never the text's preference.
        assert!(
            window.get_display_value_width() <= region,
            "the value width stays bounded by the region"
        );
    });
}

/// The differential echo pin: the widget's in-layout echo mount
/// solves to the hand-rolled echo grammar's geometry — the row
/// heights and the dash placement agree to the pixel (the golden
/// oracle's structural aid for the W4 re-host).
#[test]
fn the_echo_mount_matches_the_hand_rolled_geometry() {
    run_probe(|window| {
        settle(window);
        // The row body heights agree (the widget's echo mount solves
        // to the hand-rolled grammar's 32px inner row).
        assert_eq!(
            window.get_echo_widget_content_height(),
            window.get_echo_hand_height(),
            "the row body heights agree"
        );
        // The trailing slot right-aligns in the region (the hand
        // grammar's pencil rides the column's right end).
        let inner_right = window.get_echo_widget_inner_x() + window.get_echo_widget_inner_width();
        let content_right =
            window.get_echo_widget_content_x() + window.get_echo_widget_content_width();
        assert_eq!(inner_right, content_right, "the slot right-aligns");
    });
}
