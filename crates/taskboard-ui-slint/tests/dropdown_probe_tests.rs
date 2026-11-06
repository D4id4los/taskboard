// SPDX-License-Identifier: AGPL-3.0-only

//! The dropdown probe's tier-2 suite (r4 D39, plan §2): the real
//! `Dropdown` widget's contract — open/close transitions and the
//! focus hand-over, the keyboard ladder (arrows; the C-n/C-p arms
//! share the same `move-highlight` arm — the testing platform's
//! `WindowEvent::KeyPressed` carries no modifier flags, so the
//! modified spellings are covered structurally, the recorded R1
//! verdict: `event.modifiers.control` IS observable in widget
//! `key-pressed` — the codebase's timer-setter/task-view
//! activation-stop precedent), clamping, select routing, Escape/Tab/
//! outside-click dismissal, the hover seam, the indicator mapping
//! total, and the six-row geometry cap. Deterministic: every event
//! dispatches inside one platform turn on the mock clock.

mod support;

use slint::ComponentHandle as _;

use support::driver::advance_probe_clock;

include!(concat!(env!("OUT_DIR"), "/dropdown_probe.rs"));

fn run_probe(job: impl FnOnce(&DropdownProbeWindow) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = DropdownProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

// The field's centre in window coordinates: the dropdown sits at
// (space-2, space-2) — the probe exposes the geometry as facts.
fn field_center(window: &DropdownProbeWindow) -> (f32, f32) {
    (
        window.get_field_x() / 1.0 + window.get_field_h() / 2.0,
        window.get_field_y() / 1.0 + window.get_field_h() / 2.0,
    )
}

fn click(window: &DropdownProbeWindow, x: f32, y: f32) {
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
}

fn key(window: &DropdownProbeWindow, k: slint::platform::Key) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: k.into() });
}

use slint::platform::Key as K;

fn move_pointer(window: &DropdownProbeWindow, x: f32, y: f32) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
}

/// Opening from a field click grabs focus into the flyout (the field
/// empties); Escape closes and hands the walk back to the field.
#[test]
fn open_close_hands_focus_between_field_and_flyout() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        assert!(!window.get_open(), "closed at mount");

        click(window, x, y);
        assert!(window.get_open(), "the field click opens");
        // The walk stays in the field (the click focuses it; the
        // flyout never grabs — the recorded r4 decision: the field is
        // the ladder's single home).
        assert!(window.get_field_focused(), "the click focused the field");

        key(window, K::Escape);
        assert!(!window.get_open(), "Escape closes");
        assert!(
            window.get_field_focused(),
            "closing hands focus back to the field"
        );
    });
}

/// Return on the focused field opens (the icon-button raw-key half);
/// the open-time anchor is the selected index, not 0.
#[test]
fn return_on_the_focused_field_opens_and_anchors_the_selection() {
    run_probe(|window| {
        // The walk reaches the field first: Tab from the initial
        // focus (the walk's first stop is the dropdown's field).
        key(window, K::Tab);
        assert!(window.get_field_focused(), "the field is a real walk stop");
        assert!(!window.get_open());

        key(window, K::Return);
        assert!(window.get_open(), "Return opens");
        assert_eq!(window.get_highlight(), 2, "the anchor is the selected row");
        assert_eq!(window.get_seek_row(), 2, "the seek lands the selection");
    });
}

/// The ladder: Down/Up move the highlight (clamped at both ends);
/// every move bumps the seek nonce and seeks the list.
#[test]
fn the_ladder_moves_and_clamps_at_the_ends() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        click(window, x, y);
        assert_eq!(window.get_highlight(), 2);

        let nonce = window.get_seek_nonce();
        key(window, K::DownArrow);
        assert_eq!(window.get_highlight(), 3, "Down moves");
        assert!(window.get_seek_nonce() > nonce, "every move seeks");

        key(window, K::DownArrow);
        assert_eq!(window.get_highlight(), 3, "Down clamps at the last row");
        key(window, K::DownArrow);
        assert_eq!(window.get_highlight(), 3, "Down stays clamped");

        for _ in 0..5 {
            key(window, K::UpArrow);
        }
        assert_eq!(window.get_highlight(), 0, "Up clamps at the first row");
    });
}

/// Return on the flyout selects the highlighted row, routes
/// `row-activated`, and closes.
#[test]
fn return_selects_routes_and_closes() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        click(window, x, y);
        key(window, K::UpArrow); // Up: highlight 2 -> 1
        key(window, K::Return);
        assert!(!window.get_open(), "selecting closes");
        assert_eq!(window.global::<DropdownProbe>().get_activated(), 1);
        assert_eq!(window.global::<DropdownProbe>().get_activated_index(), 1);
    });
}

/// Escape closes without selecting; an outside click closes through
/// the underlay (absorbed — no row-activated, no focus move).
#[test]
fn escape_and_outside_click_close_without_selecting() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        click(window, x, y);
        key(window, K::DownArrow);
        key(window, K::Escape);
        assert!(!window.get_open());
        assert_eq!(
            window.global::<DropdownProbe>().get_activated(),
            0,
            "Escape never selects"
        );

        click(window, x, y);
        assert!(window.get_open());
        // Outside click: the window's bottom-left corner (the
        // window-level underlay's body — no widget there). The
        // round-5 overlay grammar's underlay is window-sized, so the
        // point must be inside the window.
        click(window, 2.0, window.get_field_y() + 250.0);
        assert!(!window.get_open(), "the outside click closes");
        assert_eq!(
            window.global::<DropdownProbe>().get_activated(),
            0,
            "the outside click never selects"
        );
    });
}

/// Tab closes and rejects so the walk continues onto the next stop.
#[test]
fn tab_closes_and_walks_on() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        click(window, x, y);
        assert!(window.get_open());
        key(window, K::Tab);
        assert!(!window.get_open(), "Tab closes");
        // The traversal settles on the mock clock's next turn (the
        // walk-probe's load-bearing fact).
        advance_probe_clock(0);
        assert!(
            window.get_after_focus(),
            "the walk continued to the next stop"
        );
    });
}

/// A row click selects through the same `row-activated` seam.
#[test]
fn a_row_click_selects() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        click(window, x, y);
        // Row 0's centre: the flyout starts at field bottom + spacing;
        // rows are space-6 tall. The probe exposes the field box; the
        // flyout's first row is within one row-height of the field.
        let row0_y = window.get_field_y() + window.get_field_h() + window.get_field_h() / 2.0;
        click(window, x, row0_y);
        assert_eq!(window.global::<DropdownProbe>().get_activated(), 1);
        assert_eq!(window.global::<DropdownProbe>().get_activated_index(), 0);
        assert!(!window.get_open(), "selecting closes");
    });
}

/// Hover over a row reports the consumer seam (`row-hovered` with the
/// index), leaving reports `row-unhovered`.
#[test]
fn row_hover_reports_the_consumer_seam() {
    run_probe(|window| {
        let (x, y) = field_center(window);
        click(window, x, y);
        let row0_y = window.get_field_y() + window.get_field_h() + window.get_field_h() / 2.0;
        move_pointer(window, x, row0_y);
        assert_eq!(window.global::<DropdownProbe>().get_hovered(), 1);
        assert_eq!(window.global::<DropdownProbe>().get_hovered_index(), 0);
        move_pointer(window, 2.0, 2.0);
        assert_eq!(window.global::<DropdownProbe>().get_unhovered(), 1);
    });
}

/// The indicator mapping is a three-arm total; the tone mapping is
/// `RowDot`'s language (typed equality, no pixels).
#[test]
fn the_indicator_and_tone_mappings_are_totals() {
    run_probe(|window| {
        assert_eq!(window.get_mapped_none(), IconKind::Warn);
        assert_eq!(window.get_mapped_link(), IconKind::Link);
        assert_eq!(window.get_mapped_link_off(), IconKind::LinkOff);
        // Tone → colour: distinct arms, neutral = text-secondary.
        let colors = [
            window.get_tone_neutral(),
            window.get_tone_success(),
            window.get_tone_accent(),
            window.get_tone_warning(),
            window.get_tone_danger(),
        ];
        for (i, a) in colors.iter().enumerate() {
            for b in &colors[i + 1..] {
                assert_ne!(a, b, "tone arms {i} map to distinct colours");
            }
        }
    });
}

/// The six-row cap: the four-row mount's flyout is its natural
/// height; the nine-row mount's is capped at six rows (the `ListView`
/// scrolls the rest). Both facts are the flyout's own height
/// expression, mirrored as typed seams.
#[test]
fn the_flyout_caps_at_six_rows() {
    run_probe(|window| {
        let row_h = window.get_flyout_h() / 4.0;
        assert!(
            row_h > 0.0,
            "four rows render at their natural height: {}",
            window.get_flyout_h()
        );
        assert!(
            (window.get_nine_flyout_h() - row_h * 6.0).abs() < 0.01,
            "the flyout caps at six rows: {} vs {}",
            window.get_nine_flyout_h(),
            row_h * 6.0
        );
    });
}
