// SPDX-License-Identifier: AGPL-3.0-only

//! The widget-framework probe's G-catalogue suite (round-9 W1, plan
//! §5): the geometry/law assertions G-1..G-13 over the two base hosts
//! (`ModalDialogHost`, `AnchoredOverlayHost`) mounted on the probe
//! fixture. Every observation is a typed root seam (facts in logical
//! px), every intent a counter — no text assertions, no clocks, no
//! quality ceilings.

mod support;

use slint::ComponentHandle as _;

use support::driver::advance_probe_clock;

include!(concat!(env!("OUT_DIR"), "/widget_framework_probe.rs"));

fn run_probe(job: impl FnOnce(&WidgetFrameworkProbeWindow) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = WidgetFrameworkProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

fn open_modal(window: &WidgetFrameworkProbeWindow) {
    window.set_modal_open(true);
    advance_probe_clock(0);
    // A pointer move forces the testing backend's geometry pass so
    // the realized rects are current.
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(1.0, 1.0),
        });
    advance_probe_clock(0);
}

fn close_modal(window: &WidgetFrameworkProbeWindow) {
    window.set_modal_open(false);
    advance_probe_clock(0);
}

fn open_anchored(window: &WidgetFrameworkProbeWindow) {
    window.set_anchored_open(true);
    advance_probe_clock(0);
    // A pointer move forces the testing backend's geometry pass (the
    // modal opener's recorded idiom).
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(1.0, 1.0),
        });
    advance_probe_clock(0);
}

fn click(window: &WidgetFrameworkProbeWindow, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let p = slint::LogicalPosition::new(x, y);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: p,
            button: PointerEventButton::Left,
        });
    advance_probe_clock(0);
}

fn right_press(window: &WidgetFrameworkProbeWindow, x: f32, y: f32) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Right,
        });
    advance_probe_clock(0);
}

fn press_tab(window: &WidgetFrameworkProbeWindow) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    advance_probe_clock(0);
}

/// The probe's adapter half: arms the mask and pushes one focus
/// index (the real adapter owns `armed`; the nonce bump flushes the
/// host's consumption edge).
fn push_focus(window: &WidgetFrameworkProbeWindow, armed: bool, index: i32) {
    let mask = window.global::<MaskWalkFacts>();
    mask.set_focus_index(index);
    mask.set_armed(armed);
    mask.set_nonce(mask.get_nonce() + 1);
    advance_probe_clock(0);
}

// The scrim corner: inside the window, far from the centered panel
// and the anchored fixture — the underlay's body.
fn scrim_corner(window: &WidgetFrameworkProbeWindow) -> (f32, f32) {
    (6.0, window.get_viewport_floor() - 6.0)
}

// The panel's dead-space centre (over the absorbing root or inert
// body children — never an interactive child).
fn modal_center(window: &WidgetFrameworkProbeWindow) -> (f32, f32) {
    (
        window.get_panel_x() + window.get_modal_panel_width() / 2.0,
        window.get_panel_y() + window.get_modal_panel_height() / 2.0,
    )
}

/// G-1: the modal panel is centered horizontally; y sits within the
/// `[space-4, floor - panel-h - space-2]` band (no layout shift is
/// G-9's witness).
#[test]
fn g1_modal_panel_is_centered_within_the_floor_band() {
    run_probe(|window| {
        open_modal(window);
        let expected_x = (window.get_win_w() - window.get_modal_panel_width()) / 2.0;
        assert!(
            (window.get_panel_x() - expected_x).abs() < 0.01,
            "panel-x centered: {} vs {}",
            window.get_panel_x(),
            expected_x
        );
        assert!(window.get_panel_y() >= 16.0, "y respects the top margin");
        assert!(
            window.get_panel_y()
                <= window.get_viewport_floor() - window.get_modal_panel_height() - 8.0,
            "y respects the floor"
        );
    });
}

/// G-6: the sizing law — preferred below the floor token lifts to
/// dialog-min-width, preferred above the cap token drops to
/// dialog-max-width, preferred between the bounds wins verbatim.
#[test]
fn g6_width_law_bounds_the_preferred_between_the_tokens() {
    run_probe(|window| {
        window.set_probe_preferred_width(100.0);
        open_modal(window);
        assert!(
            window.get_modal_panel_width() >= 280.0,
            "the floor lifts a tiny preferred: {}",
            window.get_modal_panel_width()
        );
        close_modal(window);

        window.set_probe_preferred_width(900.0);
        open_modal(window);
        assert!(
            window.get_modal_panel_width() <= 560.0,
            "the cap bounds a huge preferred: {}",
            window.get_modal_panel_width()
        );
        close_modal(window);

        window.set_probe_preferred_width(400.0);
        open_modal(window);
        assert!(
            (window.get_modal_panel_width() - 400.0).abs() < 0.01,
            "the preferred wins between the bounds: {}",
            window.get_modal_panel_width()
        );
    });
}

/// G-7: height by content, never stretched — a 1-row body and a
/// 20-row body both stay under the `floor - 2·space-4` cap.
#[test]
fn g7_height_tracks_content_and_never_crosses_the_cap() {
    run_probe(|window| {
        window.set_probe_body_height(32.0);
        open_modal(window);
        let one_row = window.get_modal_panel_height();
        close_modal(window);

        window.set_probe_body_height(20.0 * 24.0);
        open_modal(window);
        let twenty_rows = window.get_modal_panel_height();
        let cap = window.get_viewport_floor() - 2.0 * 16.0;
        assert!(
            twenty_rows > one_row,
            "the taller body renders taller: {twenty_rows} vs {one_row}"
        );
        assert!(
            one_row <= cap && twenty_rows <= cap,
            "both stay under the cap {cap}: {one_row}, {twenty_rows}"
        );
    });
}

/// G-8: the scrim paints `Theme.scrim` full-window and sits below the
/// panel in the hit test — a click on the panel dead space never
/// reaches the scrim's dismissal.
#[test]
fn g8_scrim_paints_and_yields_to_the_panel() {
    run_probe(|window| {
        open_modal(window);
        let expected = slint::Color::from_argb_u8(128, 11, 13, 18);
        assert_eq!(
            window.get_modal_scrim(),
            expected,
            "the scrim token paints the scrim"
        );
        let (x, y) = modal_center(window);
        click(window, x, y);
        assert_eq!(
            window.get_dismissed_count(),
            0,
            "a panel click never reaches the scrim"
        );
    });
}

/// G-11: the dismissal law — a scrim click closes, a right-button
/// down on the scrim closes, a panel-root click does not.
#[test]
fn g11_dismissal_law_left_click_right_click_inside_safe() {
    run_probe(|window| {
        open_modal(window);
        let (x, y) = modal_center(window);
        click(window, x, y);
        assert!(
            window.get_modal_open(),
            "inside click never closes (US-1/S3)"
        );

        let (cx, cy) = scrim_corner(window);
        click(window, cx, cy);
        assert_eq!(window.get_dismissed_count(), 1, "scrim click dismisses");
        assert!(!window.get_modal_open());

        open_modal(window);
        let before = window.get_dismissed_count();
        let (cx, cy) = scrim_corner(window);
        right_press(window, cx, cy);
        assert_eq!(
            window.get_dismissed_count() - before,
            1,
            "right-down on the scrim dismisses (the r4 D2 idiom)"
        );
        assert!(!window.get_modal_open());
    });
}

/// G-11/US-1 AC-3 (kind half): an edit-kind dialog resolves the
/// outside click as accept (R-4) — the same scrim click routes
/// `accepted` instead of `dismissed`.
#[test]
fn g11_edit_kind_outside_click_accepts() {
    run_probe(|window| {
        window.set_probe_kind(DialogKind::Edit);
        open_modal(window);
        let (cx, cy) = scrim_corner(window);
        click(window, cx, cy);
        assert_eq!(window.get_accepted_count(), 1, "edit outside = accept");
        assert_eq!(window.get_dismissed_count(), 0);
        assert!(!window.get_modal_open());
    });
}

/// G-12: after the dismissal routing the opener holds focus again
/// (the presentation half of the restore handshake) and the unmount
/// handshake counted exactly one close.
#[test]
fn g12_dismissal_restores_the_opener_focus() {
    run_probe(|window| {
        open_modal(window);
        let (cx, cy) = scrim_corner(window);
        click(window, cx, cy);
        assert!(window.get_opener_focused(), "the opener holds focus again");
        assert_eq!(window.get_closed_count(), 1, "one unmount handshake");
    });
}

/// G-13: the button model — every click routes `button-invoked` with
/// the model index, and `initial-focus` lands on the declared button.
#[test]
fn g13_button_model_routes_indices_and_lands_the_initial_focus() {
    run_probe(|window| {
        window.set_probe_initial_focus(1);
        open_modal(window);
        assert_eq!(
            window.get_modal_focused_button(),
            1,
            "initial focus lands on the declared button"
        );

        // The Save button's centre: the cluster is at the panel's
        // bottom-right (buttons are space-6 tall, min-width 4·space-6).
        let save = (
            window.get_panel_x() + window.get_modal_panel_width() - 8.0 - 128.0 / 2.0,
            window.get_panel_y() + window.get_modal_panel_height() - 8.0 - 16.0,
        );
        click(window, save.0, save.1);
        assert_eq!(window.get_invoked_count(), 1);
        assert_eq!(window.get_invoked_index(), 1, "the index rides");

        // The Cancel button's centre (the cluster's left entry).
        let cancel = (
            window.get_panel_x() + window.get_modal_panel_width() - 8.0 - 128.0 - 8.0 - 128.0 / 2.0,
            save.1,
        );
        click(window, cancel.0, cancel.1);
        assert_eq!(window.get_invoked_count(), 2);
        assert_eq!(window.get_invoked_index(), 0);
        close_modal(window);
    });
}

/// G-13 (gating half): a disabled button neither takes the initial
/// focus nor invokes — the row never focuses the declared-but-disabled
/// entry.
#[test]
fn g13_a_disabled_button_never_focuses_nor_invokes() {
    run_probe(|window| {
        window.set_probe_buttons(
            std::rc::Rc::new(slint::VecModel::from(vec![
                DialogButtonModel {
                    label: "Cancel".into(),
                    role: DialogButtonRole::Dismiss,
                    enabled: true,
                    autofocus: false,
                },
                DialogButtonModel {
                    label: "Save".into(),
                    role: DialogButtonRole::Accept,
                    enabled: false,
                    autofocus: false,
                },
            ]))
            .into(),
        );
        window.set_probe_initial_focus(1);
        advance_probe_clock(0);
        open_modal(window);
        assert_eq!(
            window.get_modal_focused_button(),
            -1,
            "a disabled button never takes the initial focus"
        );
        let save = (
            window.get_panel_x() + window.get_modal_panel_width() - 8.0 - 128.0 / 2.0,
            window.get_panel_y() + window.get_modal_panel_height() - 8.0 - 16.0,
        );
        click(window, save.0, save.1);
        assert_eq!(
            window.get_invoked_count(),
            0,
            "a disabled click never invokes"
        );
        close_modal(window);
    });
}

/// G-10: walk containment — opening arms the mask with the declared
/// extent and a zeroed cursor; the adapter's focus push clamps into
/// `[0, walk-stops - 1]`; Tab from the body stop reaches the button
/// row (the walk stays inside the dialog).
#[test]
fn g10_the_mask_arms_on_open_and_clamps_the_push() {
    run_probe(|window| {
        window.set_probe_walk_stops(2);
        open_modal(window);
        assert_eq!(window.get_modal_mask_extent(), 2, "the extent is pushed");
        assert_eq!(window.get_modal_mask_cursor(), 0, "the cursor starts 0");

        push_focus(window, true, 7);
        assert_eq!(
            window.get_modal_mask_cursor(),
            1,
            "the push clamps at the last stop"
        );
        push_focus(window, true, -1);
        assert_eq!(
            window.get_modal_mask_cursor(),
            0,
            "a missing push clamps at the first stop"
        );
        push_focus(window, false, -1);
        close_modal(window);
    });
}

/// G-10 (walk half): Tab from the body stop walks into the button
/// row — the dialog's own stops hold the walk before any mask
/// arming.
#[test]
fn g10_tab_from_the_body_stop_reaches_the_buttons() {
    run_probe(|window| {
        window.set_probe_initial_focus(-1);
        open_modal(window);
        // The body stop's centre (panel top-left padding + half the
        // fixture body box): a click focuses it like a user's.
        let stop = (
            window.get_panel_x() + 8.0 + window.get_probe_body_width() / 2.0,
            window.get_panel_y() + 8.0 + window.get_probe_body_height() / 2.0,
        );
        click(window, stop.0, stop.1);
        assert!(
            window.get_body_stop_focused(),
            "the walk enters the body stop first"
        );
        press_tab(window);
        assert_eq!(
            window.get_modal_focused_button(),
            0,
            "the walk continues into the button row"
        );
    });
}

/// G-9: the layout-shift pin (J2 R-7) — the sibling witnesses' rects
/// are identical before, while, and after both overlays are open.
#[test]
fn g9_opening_either_overlay_shifts_no_layout() {
    run_probe(|window| {
        let (s0, s1) = (window.get_sib0_y(), window.get_sib1_y());
        open_anchored(window);
        assert_eq!(window.get_sib0_y(), s0);
        assert_eq!(window.get_sib1_y(), s1);
        open_modal(window);
        assert_eq!(window.get_sib0_y(), s0);
        assert_eq!(window.get_sib1_y(), s1);
        window.set_anchored_open(false);
        close_modal(window);
        assert_eq!(window.get_sib0_y(), s0);
        assert_eq!(window.get_sib1_y(), s1);
    });
}

/// G-2 (below): the overlay sits below the anchor with the gap and
/// never intersects the anchor rect (US-2 AC-1).
#[test]
fn g2_below_places_the_overlay_clear_of_the_anchor() {
    run_probe(|window| {
        let gap = 4.0;
        let anchor_bottom = window.get_anchor_y() + window.get_anchor_h();
        open_anchored(window);
        assert!(!window.get_a_flipped(), "the default resolves below");
        assert!(
            (window.get_a_panel_y() - (anchor_bottom + gap)).abs() < 0.01,
            "computed-y is anchor bottom + gap: {}",
            window.get_a_panel_y()
        );
        assert!(
            window.get_a_panel_y() >= anchor_bottom,
            "no intersection with the anchor (US-2 AC-1)"
        );
    });
}

/// G-2/G-3 (flip): with the anchor near the floor the overlay flips
/// above and still never intersects the anchor.
#[test]
fn g3_the_overlay_flips_above_near_the_floor() {
    run_probe(|window| {
        window.set_anchor_y(window.get_viewport_floor() - 40.0);
        let gap = 4.0;
        open_anchored(window);
        assert!(window.get_a_flipped(), "no room below flips above");
        assert!(
            window.get_a_panel_y() + window.get_a_panel_h() <= window.get_anchor_y(),
            "the flipped rect ends above the anchor top"
        );
        assert!(
            window.get_a_panel_y() <= window.get_anchor_y() - gap,
            "the gap is preserved above"
        );
    });
}

/// G-4: the clamp law — an anchor near the right edge keeps the
/// overlay inside the margins; the flip choice follows the
/// fits-below predicate exactly.
#[test]
fn g4_the_clamps_hold_at_both_edges() {
    run_probe(|window| {
        // Right-edge anchor (the dropdown-160 scenario): x clamps so
        // the panel never crosses `win-w - space-2`.
        window.set_anchor_x(window.get_win_w() - 24.0);
        open_anchored(window);
        assert!(
            window.get_a_panel_x() + window.get_a_panel_w() <= window.get_win_w() - 8.0,
            "x clamps at the right margin"
        );
        window.set_anchored_open(false);

        // Left-edge anchor: x clamps at the left margin.
        window.set_anchor_x(0.0);
        open_anchored(window);
        assert!(window.get_a_panel_x() >= 8.0, "x clamps at the left margin");
        window.set_anchored_open(false);

        // A partially off-window anchor (G-5) clamps the same way.
        window.set_anchor_x(window.get_win_w() + 10.0);
        open_anchored(window);
        assert!(
            window.get_a_panel_x() + window.get_a_panel_w() <= window.get_win_w() - 8.0,
            "the off-window anchor clamps to the margin"
        );
    });
}

/// G-5: the anchored dismissal — an outside click (scrim corner)
/// closes through the underlay, an inside click never does.
#[test]
fn g5_anchored_inside_safe_outside_closes() {
    run_probe(|window| {
        open_anchored(window);
        let inside = (
            window.get_a_panel_x() + window.get_a_panel_w() / 2.0,
            window.get_a_panel_y() + window.get_a_panel_h() / 2.0,
        );
        click(window, inside.0, inside.1);
        assert!(
            window.get_anchored_open(),
            "an inside click never closes the popover"
        );
        let (cx, cy) = scrim_corner(window);
        click(window, cx, cy);
        assert_eq!(window.get_anchored_dismissed_count(), 1);
        assert!(!window.get_anchored_open());
    });
}

/// G-10 (anchored half): the anchored host arms its mask too —
/// containment is a property of being an overlay (D-W1-4).
#[test]
fn g10_the_anchored_host_arms_the_mask_as_well() {
    run_probe(|window| {
        open_anchored(window);
        assert_eq!(window.get_a_mask_extent(), 1);
        window.set_anchored_open(false);
    });
}

/// G-2 in both placements plus the closed state: with both overlays
/// shut the probe fixture renders nothing over the anchors (the
/// mount gating works — no lingering anatomy).
#[test]
fn the_closed_hosts_leave_no_anatomy_behind() {
    run_probe(|window| {
        open_modal(window);
        let (cx, cy) = scrim_corner(window);
        click(window, cx, cy);
        assert!(!window.get_modal_open());
        // The closed host's dismissal layer is inert: the opener
        // click lands.
        click(window, 32.0, 32.0);
        assert_eq!(window.get_dismissed_count(), 1, "no lingering scrim");
        assert_eq!(window.get_anchored_dismissed_count(), 0);
    });
}

// ---- W3: the TextButton catalogue (plan §7 step 1; #950) ----------
// The button grammar's laws on the primitive: the tab stop + the
// `Return`/`Space` activation (the single keyboard copy), the one gate
// shared by pointer/keys/AT, the click-through ceremony (r2 D34),
// and the request-focus forward (the `DialogButton` autofocus half).

fn button_center(window: &WidgetFrameworkProbeWindow) -> (f32, f32) {
    (
        window.get_button_x() + window.get_button_w() / 2.0,
        window.get_button_y() + window.get_button_h() / 2.0,
    )
}

fn press_key(window: &WidgetFrameworkProbeWindow, key: slint::platform::Key) {
    use slint::platform::WindowEvent;
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    advance_probe_clock(0);
}

/// The `TextButton` is a real tab stop: from the fixture opener, one
/// synthetic `Tab` reaches the button and `Return` activates it (the
/// grammar's keyboard half — every absorbed variant's law, and the
/// gain B-1 records for `VerbButton`).
#[test]
fn g_button_tab_stop_and_return_activates() {
    run_probe(|window| {
        press_tab(window);
        assert!(
            window.get_opener_focused(),
            "precondition: the opener holds focus"
        );
        press_tab(window);
        advance_probe_clock(0);
        assert!(
            window.get_button_focused(),
            "one more tab lands on the `TextButton`"
        );
        let before = window.get_button_clicked_count();
        press_key(window, slint::platform::Key::Return);
        assert_eq!(
            window.get_button_clicked_count() - before,
            1,
            "`Return` activates the focused `TextButton`"
        );
    });
}

/// A disabled button refuses every channel with one gate: the
/// pointer click is swallowed and the `Return`/`Space` activations are
/// gated (the AT channel shares the same `enabled || click-through`
/// gate — the accessible-enabled + gated default-action shape).
#[test]
fn g_disabled_swallows_all_three_channels() {
    run_probe(|window| {
        window.set_probe_button_enabled(false);
        advance_probe_clock(0);

        let before = window.get_button_clicked_count();
        let (x, y) = button_center(window);
        click(window, x, y);
        assert_eq!(
            window.get_button_clicked_count(),
            before,
            "the pointer channel refuses while disabled"
        );

        window.invoke_probe_request_button_focus();
        advance_probe_clock(0);
        assert!(
            window.get_button_focused(),
            "the disabled button is still addressable"
        );
        press_key(window, slint::platform::Key::Return);
        press_key(window, slint::platform::Key::Space);
        assert_eq!(
            window.get_button_clicked_count(),
            before,
            "the key channels refuse while disabled"
        );
    });
}

/// The click-through ceremony (r2 D34): with `click-through` the
/// click fires regardless of enablement — pointer and keys both —
/// while the disabled affordance still renders.
#[test]
fn g_click_through_fires_while_disabled() {
    run_probe(|window| {
        window.set_probe_button_enabled(false);
        window.set_probe_button_click_through(true);
        advance_probe_clock(0);

        let (x, y) = button_center(window);
        click(window, x, y);
        assert_eq!(
            window.get_button_clicked_count(),
            1,
            "the pointer channel fires through"
        );

        window.invoke_probe_request_button_focus();
        advance_probe_clock(0);
        press_key(window, slint::platform::Key::Return);
        press_key(window, slint::platform::Key::Space);
        assert_eq!(
            window.get_button_clicked_count(),
            3,
            "the key channels fire through too"
        );
    });
}

/// The autofocus half: `request-focus()` forwards to the internal
/// scope and the `focused` out property observes it (`DialogButton`'s
/// mount-focus seam rides this).
#[test]
fn g_focus_forwards_to_request_focus() {
    run_probe(|window| {
        assert!(!window.get_button_focused());
        window.invoke_probe_request_button_focus();
        advance_probe_clock(0);
        assert!(
            window.get_button_focused(),
            "request-focus lands on the internal scope"
        );
    });
}

// ---- W3: the CheckBox catalogue (plan §7 step 3; #953) ------------
// The state-less check anatomy: the model bit is the single source of
// truth, the toggle intent is the only emission, and the focusable
// flag carries the card's no-tab-stop posture.

fn check_center(window: &WidgetFrameworkProbeWindow) -> (f32, f32) {
    (
        window.get_check_x() + window.get_check_w() / 2.0,
        window.get_check_y() + window.get_check_h() / 2.0,
    )
}

/// State-less rendering: a pointer toggle only emits the intent — the
/// rendered check bit follows the model property alone (a local
/// checked state anywhere would flip the render before the model).
#[test]
fn g_check_renders_model_bit_statelessly() {
    run_probe(|window| {
        assert!(!window.get_check_shows_check(), "precondition: unchecked");
        let (x, y) = check_center(window);
        click(window, x, y);
        assert_eq!(
            window.get_check_toggled_count(),
            1,
            "the click emits toggled"
        );
        assert!(
            !window.get_check_shows_check(),
            "the render does not move until the model bit does"
        );

        window.set_probe_check_checked(true);
        advance_probe_clock(0);
        assert!(
            window.get_check_shows_check(),
            "the model push is the single writer of the render"
        );
        assert_eq!(
            window.get_check_toggled_count(),
            1,
            "no intent from a model push"
        );
    });
}

/// The keyboard and pointer channels land on the same `toggled` seam:
/// `Return` and `Space` on the tab-reachable box, then the click.
#[test]
fn g_check_keyboard_and_pointer_toggled() {
    run_probe(|window| {
        press_tab(window);
        press_tab(window);
        press_tab(window);
        advance_probe_clock(0);
        assert!(
            window.get_check_focused(),
            "three tabs land on the view-side box"
        );

        press_key(window, slint::platform::Key::Return);
        press_key(window, slint::platform::Key::Space);
        assert_eq!(window.get_check_toggled_count(), 2, "both keys toggle");

        let (x, y) = check_center(window);
        click(window, x, y);
        assert_eq!(
            window.get_check_toggled_count(),
            3,
            "the pointer toggles too"
        );
    });
}

/// The card posture: `focusable: false` removes the box from the tab
/// walk — after the view-side box, no synthetic tab ever lands on it.
#[test]
fn g_unfocusable_variant_is_no_tab_stop() {
    run_probe(|window| {
        press_tab(window);
        press_tab(window);
        press_tab(window);
        advance_probe_clock(0);
        assert!(
            window.get_check_focused(),
            "precondition: the first box holds focus"
        );
        press_tab(window);
        advance_probe_clock(0);
        assert!(
            !window.get_check2_focused(),
            "the unfocusable variant never takes the tab"
        );
    });
}

/// The out-property seams: `focused` and `hovered` are observable —
/// the exact surface the view-side facts mirroring and the card's
/// done-hover re-expression consume.
#[test]
fn g_focus_and_hover_are_observable() {
    run_probe(|window| {
        assert!(!window.get_check_focused() && !window.get_check_hovered());
        press_tab(window);
        press_tab(window);
        press_tab(window);
        advance_probe_clock(0);
        assert!(window.get_check_focused(), "focus rides the tab walk");

        let (x, y) = check_center(window);
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x, y),
            });
        advance_probe_clock(0);
        assert!(
            window.get_check_hovered(),
            "hover is observable on the seam"
        );

        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(1.0, 1.0),
            });
        advance_probe_clock(0);
        assert!(!window.get_check_hovered(), "leave clears the hover seam");
    });
}

// ---- W3: the ToggleStrip catalogue (plan §7 step 5; #953) ---------
// The strip is a thin composition over the check IconButton row: the
// slot geometry is the canonical square plus the strip's spacing, and
// a click reports the picked index while the checked render follows
// the pushed mask alone.

/// Clicking slot k reports index k — one `picked` intent per click,
/// the index identifying the slot (the weekday-toggled seam).
#[test]
fn g_strip_reports_picked_index() {
    run_probe(|window| {
        // The slot square is space-6 (32 px at the probe's base
        // scale) and the strip's spacing is space-1 (4 px).
        let slot = 32.0_f32;
        let spacing = 4.0_f32;
        for k in 0..3 {
            // The probe runs at base scale; the small loop index's
            // f32 conversion is exact by construction.
            #[allow(clippy::cast_precision_loss)]
            let x = window.get_strip_x() + (slot + spacing) * k as f32 + slot / 2.0;
            let y = window.get_strip_y() + slot / 2.0;
            click(window, x, y);
            assert_eq!(
                window.get_strip_picked_count(),
                k + 1,
                "one intent per click"
            );
            assert_eq!(
                window.get_strip_picked_index(),
                k,
                "the click names its slot"
            );
        }

        // The checked render follows the pushed mask alone (the
        // strip is as stateless as the box): the mask flip re-tones
        // the slots without any intent.
        window.set_probe_strip_flip(true);
        advance_probe_clock(0);
        assert_eq!(
            window.get_strip_picked_count(),
            3,
            "a mask push emits nothing"
        );
    });
}
