// SPDX-License-Identifier: AGPL-3.0-only

//! The viewport-growth probe's suite (round-5 chunk 2, plan §5): the
//! four arms that decide the fence at `text-field.slint`'s multiline
//! flavour. P1 growth of the content extent with a constant box; P2
//! the width-dependent re-wrap pinned on the safe side of the box/
//! viewport split; P3 cursor-follow scroll; P4 the key grammar and
//! the focus/scrollbar seam arms. The build-log half of the verdict
//! is the zero-binding-loop-warning check over the same sources
//! (recorded in the slice-1 commit body; re-checked by stage 3).

mod support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;
use slint::platform::{Key as K, WindowEvent};

include!(concat!(env!("OUT_DIR"), "/editor_viewport_probe.rs"));

fn run_probe(job: impl FnOnce(&EditorViewportProbeWindow) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = EditorViewportProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

fn key(window: &EditorViewportProbeWindow, k: K, pressed: bool) {
    let event = if pressed {
        WindowEvent::KeyPressed { text: k.into() }
    } else {
        WindowEvent::KeyReleased { text: k.into() }
    };
    window.window().dispatch_event(event);
}

/// Types a string through the real key path — newlines ride Return
/// (the widget's native newline), the only buffer-seeding seam the
/// production widget exposes (no two-way text in v1, D-MLE-5).
fn type_text(window: &EditorViewportProbeWindow, text: &str) {
    for c in text.chars() {
        if c == '\n' {
            key(window, K::Return, true);
            key(window, K::Return, false);
        } else {
            type_char(window, c);
        }
    }
    advance_probe_clock(0);
}

fn type_char(window: &EditorViewportProbeWindow, c: char) {
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: c.to_string().into(),
    });
}

fn click(window: &EditorViewportProbeWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    let button = i_slint_core::items::PointerEventButton::Left;
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed { position, button },
        WindowEvent::PointerReleased { position, button },
    ] {
        window.window().dispatch_event(event);
    }
}

fn set_window_width(window: &EditorViewportProbeWindow, width: f32) {
    window
        .window()
        .set_size(slint::WindowSize::Logical(slint::LogicalSize::new(
            width, 288.0,
        )));
    advance_probe_clock(0);
}

/// P1: seeding embedded newlines grows the content extent, never the
/// box; appending grows both extent terms again; the extent binding
/// is exact (`max(inner, preferred)` — our own binding).
#[test]
fn p1_the_viewport_grows_with_the_buffer_and_the_box_stays() {
    run_probe(|window| {
        advance_probe_clock(0);
        let inner = window.get_inner_height();
        let box_before = window.get_box_height();
        assert_eq!(
            window.get_viewport_height(),
            inner,
            "empty buffer: no overflow"
        );

        type_text(window, "l1\nl2\nl3\nl4");
        advance_probe_clock(0);
        let preferred = window.get_input_preferred_height();
        assert!(
            preferred > window.get_line_height(),
            "three embedded newlines exceed one line"
        );
        assert_eq!(
            window.get_viewport_height(),
            inner.max(preferred),
            "the extent binding is exact"
        );
        assert_eq!(window.get_box_height(), box_before, "the box never grows");
        let previous_preferred = preferred;

        type_text(window, "\nl5\nl6");
        let preferred = window.get_input_preferred_height();
        assert!(
            preferred > previous_preferred,
            "appending lines grows the extent again"
        );
        assert_eq!(
            window.get_viewport_height(),
            inner.max(preferred),
            "the extent binding stays exact"
        );
        assert_eq!(
            window.get_box_height(),
            box_before,
            "the box stays constant"
        );
    });
}

/// P2: the width-dependent re-wrap — the very dependence the fence
/// recorded as a hazard — tracks safely on the viewport side of the
/// split: narrower window, taller extent, constant box.
#[test]
fn p2_narrowing_the_window_re_wraps_and_the_extent_tracks() {
    run_probe(|window| {
        advance_probe_clock(0);
        set_window_width(window, 384.0);
        let long = "word ".repeat(60);
        type_text(window, long.as_str());
        let wide_preferred = window.get_input_preferred_height();
        let box_before = window.get_box_height();

        set_window_width(window, 192.0);
        let narrow_preferred = window.get_input_preferred_height();
        assert!(
            narrow_preferred > wide_preferred,
            "half the width re-wraps taller ({narrow_preferred} vs {wide_preferred})"
        );
        assert_eq!(
            window.get_viewport_height(),
            window.get_inner_height().max(narrow_preferred),
            "the extent tracked the re-wrap"
        );
        assert_eq!(window.get_box_height(), box_before, "the box never grows");
    });
}

/// P3: with the buffer taller than the window, driving the cursor to
/// the buffer's end scrolls the viewport up (negative y) and the
/// cursor line lands inside the visible band.
#[test]
fn p3_the_cursor_follow_pulls_the_viewport() {
    run_probe(|window| {
        advance_probe_clock(0);
        window.invoke_focus_probe();
        advance_probe_clock(0);
        let body = (0..12).map(|i| format!("line {i}")).collect::<Vec<_>>();
        type_text(window, body.join("\n").as_str());
        // Typing itself moves the cursor; the follow may already be
        // scrolled — it must never scroll past the top, though.
        assert!(
            window.get_viewport_y() <= 0.0,
            "the follow never scrolls past the top"
        );

        // Jump to the last line: End-of-line, then Down through the
        // buffer's remaining lines.
        key(window, K::End, true);
        key(window, K::End, false);
        for _ in 0..14 {
            key(window, K::DownArrow, true);
            key(window, K::DownArrow, false);
        }
        advance_probe_clock(0);
        assert!(
            window.get_viewport_y() < 0.0,
            "the cursor's pull scrolled the viewport"
        );
        assert!(window.get_cursor_ok(), "the cursor line is fully visible");
    });
}

/// P4a: Return inserts a newline and never fires the commit seam;
/// Shift+Return fires the commit exactly once and inserts nothing.
#[test]
fn p4a_return_newlines_and_shift_return_commits() {
    run_probe(|window| {
        advance_probe_clock(0);
        window.invoke_focus_probe();
        advance_probe_clock(0);

        type_text(window, "a\nb");
        assert_eq!(window.get_buffer(), "a\nb", "Return inserted the newline");
        assert_eq!(window.get_accepted_count(), 0, "plain Return never commits");

        key(window, K::Shift, true);
        key(window, K::Return, true);
        key(window, K::Return, false);
        key(window, K::Shift, false);
        advance_probe_clock(0);
        assert_eq!(window.get_accepted_count(), 1, "Shift+Return commits once");
        assert_eq!(window.get_buffer(), "a\nb", "the commit inserted nothing");
    });
}

/// P4b: the focus latch fires exactly once per real focus loss — a
/// sibling `FocusScope` click steals it, a plain `TouchArea` click
/// does not (the D-MLE-5 replication pin).
#[test]
fn p4b_the_focus_latch_fires_once_per_real_loss() {
    run_probe(|window| {
        advance_probe_clock(0);
        assert_eq!(window.get_focus_lost_count(), 0, "no loss at mount");

        click(window, window.get_dead_cx(), window.get_dead_cy());
        advance_probe_clock(0);
        assert_eq!(
            window.get_focus_lost_count(),
            0,
            "a TouchArea click never moves focus"
        );

        click(window, window.get_sibling_cx(), window.get_sibling_cy());
        advance_probe_clock(0);
        assert_eq!(window.get_focus_lost_count(), 1, "one loss per steal");
    });
}

/// P4c: the strip appears exactly at overflow with the documented
/// thumb geometry (grabbable floor, proportional within rounding);
/// no overflow, no strip.
#[test]
fn p4c_the_scrollbar_appears_exactly_at_overflow() {
    run_probe(|window| {
        advance_probe_clock(0);
        assert!(!window.get_thumb_visible(), "no overflow, no strip");

        let body = (0..12).map(|i| format!("line {i}")).collect::<Vec<_>>();
        type_text(window, body.join("\n").as_str());
        assert!(window.get_thumb_visible(), "overflow shows the strip");

        let inner = window.get_inner_height();
        let extent = window.get_viewport_height();
        let thumb = window.get_thumb_height();
        let unfloored = inner * inner / extent;
        assert!(
            thumb >= unfloored - 0.5 && thumb <= inner + 0.5,
            "thumb {thumb} is the floored proportion of {unfloored}, within the inner {inner}"
        );
        let thumb_y = window.get_thumb_y();
        assert!(
            thumb_y >= 0.0 && thumb_y + thumb <= inner + 0.5,
            "the thumb sits inside the strip band"
        );
        // The typing pulled the cursor (and the follow) to the buffer's
        // end, so the thumb must have left the strip's top — the
        // position tracks the scroll, not a pinned 0.
        assert!(
            window.get_viewport_y() < 0.0 && thumb_y > 0.0,
            "the thumb tracks the scroll (y {}, viewport {})",
            thumb_y,
            window.get_viewport_y(),
        );
        let expected_y = (inner - thumb) * (-window.get_viewport_y()) / (extent - inner);
        assert!(
            (thumb_y - expected_y).abs() <= 0.5,
            "the thumb position is the scroll's proportion ({thumb_y} vs {expected_y})"
        );
    });
}
