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
        let facts = || window.global::<ProbeFacts>();
        let inner = facts().get_inner_height();
        let box_before = facts().get_box_height();
        assert_eq!(
            facts().get_viewport_height(),
            inner,
            "empty buffer: no overflow"
        );

        window.set_seed("l1\nl2\nl3\nl4".into());
        advance_probe_clock(0);
        let preferred = facts().get_input_preferred_height();
        assert!(
            preferred > facts().get_line_height(),
            "three embedded newlines exceed one line"
        );
        assert_eq!(
            facts().get_viewport_height(),
            inner.max(preferred),
            "the extent binding is exact"
        );
        assert_eq!(facts().get_box_height(), box_before, "the box never grows");

        window.set_seed("l1\nl2\nl3\nl4\nl5\nl6".into());
        advance_probe_clock(0);
        let preferred = facts().get_input_preferred_height();
        assert!(
            preferred > facts().get_viewport_height() - f32::EPSILON
                || facts().get_viewport_height() > inner,
            "appending lines grows the extent again"
        );
        assert_eq!(
            facts().get_viewport_height(),
            inner.max(preferred),
            "the extent binding stays exact"
        );
        assert_eq!(
            facts().get_box_height(),
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
        let facts = || window.global::<ProbeFacts>();
        let long = "word ".repeat(60);
        window.set_seed(long.as_str().into());
        set_window_width(window, 384.0);
        let wide_preferred = facts().get_input_preferred_height();
        let box_before = facts().get_box_height();

        set_window_width(window, 192.0);
        let narrow_preferred = facts().get_input_preferred_height();
        assert!(
            narrow_preferred > wide_preferred,
            "half the width re-wraps taller ({narrow_preferred} vs {wide_preferred})"
        );
        assert_eq!(
            facts().get_viewport_height(),
            facts().get_inner_height().max(narrow_preferred),
            "the extent tracked the re-wrap"
        );
        assert_eq!(facts().get_box_height(), box_before, "the box never grows");
    });
}

/// P3: with the buffer taller than the window, driving the cursor to
/// the buffer's end scrolls the viewport up (negative y) and the
/// cursor line lands inside the visible band.
#[test]
fn p3_the_cursor_follow_pulls_the_viewport() {
    run_probe(|window| {
        advance_probe_clock(0);
        let facts = || window.global::<ProbeFacts>();
        let body = (0..12).map(|i| format!("line {i}")).collect::<Vec<_>>();
        window.set_seed(body.join("\n").as_str().into());
        window.invoke_focus_probe();
        advance_probe_clock(0);
        assert_eq!(
            facts().get_viewport_y(),
            0.0,
            "the cursor starts at the top"
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
            facts().get_viewport_y() < 0.0,
            "the cursor's pull scrolled the viewport"
        );
        assert!(facts().get_cursor_ok(), "the cursor line is fully visible");
    });
}

/// P4a: Return inserts a newline and never fires the commit seam;
/// Shift+Return fires the commit exactly once and inserts nothing.
#[test]
fn p4a_return_newlines_and_shift_return_commits() {
    run_probe(|window| {
        advance_probe_clock(0);
        let facts = || window.global::<ProbeFacts>();
        window.invoke_focus_probe();
        advance_probe_clock(0);

        type_char(window, 'a');
        advance_probe_clock(0);
        key(window, K::Return, true);
        key(window, K::Return, false);
        advance_probe_clock(0);
        type_char(window, 'b');
        advance_probe_clock(0);
        assert_eq!(facts().get_buffer(), "a\nb", "Return inserted the newline");
        assert_eq!(facts().get_newline_count(), 1);
        assert_eq!(
            facts().get_accepted_count(),
            0,
            "plain Return never commits"
        );

        key(window, K::Shift, true);
        key(window, K::Return, true);
        key(window, K::Return, false);
        key(window, K::Shift, false);
        advance_probe_clock(0);
        assert_eq!(facts().get_accepted_count(), 1, "Shift+Return commits once");
        assert_eq!(facts().get_buffer(), "a\nb", "the commit inserted nothing");
    });
}

/// P4b: the focus latch fires exactly once per real focus loss — a
/// sibling `FocusScope` click steals it, a plain `TouchArea` click
/// does not (the D-MLE-5 replication pin).
#[test]
fn p4b_the_focus_latch_fires_once_per_real_loss() {
    run_probe(|window| {
        advance_probe_clock(0);
        let facts = || window.global::<ProbeFacts>();
        assert_eq!(facts().get_focus_lost_count(), 0, "no loss at mount");

        click(window, window.get_dead_cx(), window.get_dead_cy());
        advance_probe_clock(0);
        assert_eq!(
            facts().get_focus_lost_count(),
            0,
            "a TouchArea click never moves focus"
        );

        click(window, window.get_sibling_cx(), window.get_sibling_cy());
        advance_probe_clock(0);
        assert_eq!(facts().get_focus_lost_count(), 1, "one loss per steal");
    });
}

/// P4c: the strip appears exactly at overflow with the documented
/// thumb geometry (grabbable floor, proportional within rounding);
/// no overflow, no strip.
#[test]
fn p4c_the_scrollbar_appears_exactly_at_overflow() {
    run_probe(|window| {
        advance_probe_clock(0);
        let facts = || window.global::<ProbeFacts>();
        assert!(!facts().get_thumb_visible(), "no overflow, no strip");

        let body = (0..12).map(|i| format!("line {i}")).collect::<Vec<_>>();
        window.set_seed(body.join("\n").as_str().into());
        advance_probe_clock(0);
        assert!(facts().get_thumb_visible(), "overflow shows the strip");

        let inner = facts().get_inner_height();
        let extent = facts().get_viewport_height();
        let thumb = facts().get_thumb_height();
        let unfloored = inner * inner / extent;
        assert!(
            thumb >= unfloored - 0.5 && thumb <= inner + 0.5,
            "thumb {thumb} is the floored proportion of {unfloored}, within the inner {inner}"
        );
        let thumb_y = facts().get_thumb_y();
        assert!(
            thumb_y >= 0.0 && thumb_y + thumb <= inner + 0.5,
            "the thumb sits inside the strip band"
        );
    });
}
