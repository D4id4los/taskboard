// SPDX-License-Identifier: AGPL-3.0-only

//! The control-walk probe suite (round-3 D25, plan §12 P1/P2): the
//! synthetic-Tab traversal mechanism and the `FocusScope` focus seam,
//! hosted on the shared testing backend. P1's outcome picks the
//! walk's mechanism (synthetic event re-injection vs. the markup
//! fallback); P2 proves `changed has-focus` on a plain `FocusScope` —
//! the date-picker grid's context seam (a different element class
//! than the chunk-1 `TextField` probe).

mod support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;
use slint::platform::{Key, WindowEvent};

include!(concat!(env!("OUT_DIR"), "/walk_probe.rs"));

fn run_probe(job: impl FnOnce(&WalkProbeWindow) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = WalkProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

fn key(window: &WalkProbeWindow, text: &str, pressed: bool) {
    let event = if pressed {
        WindowEvent::KeyPressed { text: text.into() }
    } else {
        WindowEvent::KeyReleased { text: text.into() }
    };
    window.window().dispatch_event(event);
}

/// P1 — the runtime's tab traversal runs on a received `"\t"` key
/// event: the synthetic dispatch moves focus from the first stop to
/// the second, wraps between the two stops, and the Shift-wrapped
/// variant moves backwards. This is the walk mechanism's
/// load-bearing fact.
#[test]
fn p1_synthetic_tab_moves_focus_between_tab_stops() {
    run_probe(|window| {
        window.invoke_focus_a();
        advance_probe_clock(0);
        assert!(
            window.get_first_focused(),
            "precondition: the first stop holds focus"
        );

        key(window, "\t", true);
        advance_probe_clock(0);
        assert!(
            window.get_second_focused(),
            "one synthetic tab moved focus to the second stop"
        );

        // Shift-wrapped: hold Shift, tab, release — the traversal
        // walks backwards (the driver's modifier technique).
        let shift: slint::SharedString = Key::into(Key::Shift);
        key(window, shift.as_str(), true);
        key(window, "\t", true);
        key(window, shift.as_str(), false);
        advance_probe_clock(0);
        assert!(
            window.get_first_focused(),
            "the shift-wrapped tab moved focus back to the first stop"
        );
    });
}

/// P2 — `changed has-focus` fires on a plain `focus-on-tab-navigation`
/// `FocusScope`, once per gain and once per loss: the date-picker
/// grid's DatePicker-context seam (mechanism A on a `FocusScope`).
#[test]
fn p2_changed_has_focus_fires_on_a_focus_scope() {
    run_probe(|window| {
        let facts = window.global::<WalkProbeFacts>();
        assert_eq!(facts.get_first_gained(), 0);

        window.invoke_focus_a();
        advance_probe_clock(0);
        assert_eq!(facts.get_first_gained(), 1, "one gain event");

        window.invoke_focus_b();
        advance_probe_clock(0);
        assert_eq!(facts.get_first_lost(), 1, "one loss event");
        assert_eq!(facts.get_second_gained(), 1);
    });
}
