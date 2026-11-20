// SPDX-License-Identifier: AGPL-3.0-only

//! The control-walk probe suite (round-3 D25, plan §12 P1/P2): the
//! synthetic-Tab traversal mechanism and the `FocusScope` focus seam,
//! hosted on the shared testing backend. P1's outcome picks the
//! walk's mechanism (synthetic event re-injection vs. the markup
//! fallback); P2 proves `changed has-focus` on a plain `FocusScope` —
//! the date-picker grid's context seam (a different element class
//! than the chunk-1 `TextField` probe).

use crate::support;

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

/// P3 — the in-house `ListView`'s page arithmetic (round-3 D26):
/// one page advances by visible-height minus one row of context (the
/// runtime's cached average), the page clamps at both content ends,
/// and the up direction mirrors. The function is driven directly —
/// the family/index filtering is the consumers' bindings.
#[test]
fn p3_listview_pages_by_the_runtime_average_and_clamps() {
    run_probe(|window| {
        window.set_page_total_rows(1000);
        window.set_page_content_height(1000.0_f32);
        // The viewport is whatever the probe window laid out; the
        // average row is content/total = 1.
        let viewport = window.get_page_visible_height();
        let delta = viewport - 1.0_f32;
        assert!(delta > 0.0, "the probe's viewport exceeds the row");

        // One page down.
        window.invoke_page_by(1);
        advance_probe_clock(0);
        assert_eq!(window.get_page_y(), -delta, "one page down");

        // Clamp at the content end: at most −(content − visible).
        window.set_page_y(-(1000.0_f32 - viewport - 10.0));
        window.invoke_page_by(1);
        advance_probe_clock(0);
        assert_eq!(
            window.get_page_y(),
            -(1000.0_f32 - viewport),
            "the down clamp holds"
        );

        // One page up mirrors; the top clamp holds.
        window.invoke_page_by(-1);
        advance_probe_clock(0);
        assert_eq!(
            window.get_page_y(),
            -(1000.0 - viewport) + delta,
            "one page up"
        );
        window.set_page_y(-10.0);
        window.invoke_page_by(-1);
        advance_probe_clock(0);
        assert_eq!(window.get_page_y(), 0.0, "the top clamp holds");
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

/// P5 — the app overlay's exact shape: a conditional COMPONENT
/// instantiation whose focused container holds an inner stop. The
/// fresh mount's init grab does not take (slint 1.17: a conditional
/// subtree's init runs before the window attaches); the grab lands
/// via the post-mount height transition instead, and Tab then
/// crosses the component boundary downward into the inner stop.
#[test]
fn p5_focus_lands_via_a_post_mount_height_transition() {
    run_probe(|window| {
        advance_probe_clock(0);
        advance_probe_clock(0);
        // The init grab does not take for a conditional instantiation
        // (P5's finding): has-focus reads false.
        assert!(
            !window.get_container_has_focus(),
            "the P5 container's init grab stays dropped (precondition)"
        );
        // The height transition (post-mount, from the pad flip) is
        // the grab's trigger: the handler runs on the attached
        // subtree.
        window.set_p5_pad(40.0_f32);
        advance_probe_clock(0);
        assert!(
            window.get_container_has_focus(),
            "the changed-height grab takes on the attached subtree"
        );
        key(window, "\t", true);
        advance_probe_clock(0);
        assert!(
            window.get_inner_has_focus(),
            "Tab descends into the conditional component's inner stop (container now: {})",
            window.get_container_has_focus()
        );
    });
}

/// The nested-descent probe (r4 chunk-10 review): the window's init
/// grabs the CONTAINER; the synthetic Tab must descend into the inner
/// stop (slint's `default_next_in_local_focus_chain` goes first-child
/// first).
#[test]
fn p4_tab_descends_from_a_focused_container_into_its_child() {
    run_probe(|window| {
        advance_probe_clock(0);
        key(window, "\t", true);
        advance_probe_clock(0);
        key(window, "\t", false);
        assert!(
            WalkProbeFacts::get(window).get_inner_gained() >= 1,
            "Tab descends from the focused container into the inner stop (container gains: {})",
            WalkProbeFacts::get(window).get_container_gained()
        );
    });
}

// ---- B-1 (round-9 W3, plan §5): the walk verb row's keyboard ------
// The walk overlay's verb buttons are the only button-grammar copy
// without a tab stop + Return/Space (inventory Q-A). The probe pins
// the sanctioned gain, named for the walk's tab order: from the
// overlay's first stop, one synthetic tab reaches the verb row and
// Return activates the focused verb.

/// B-1: the verb row is tab-reachable and Return-activatable.
#[test]
fn b1_walk_tab_reaches_the_verb_row_and_return_activates() {
    run_probe(|window| {
        window.invoke_focus_b1_first();
        advance_probe_clock(0);
        let before = window.get_b1_verb_clicked();
        key(window, "\t", true);
        advance_probe_clock(0);
        let ret: slint::SharedString = Key::into(slint::platform::Key::Return);
        key(window, ret.as_str(), true);
        key(window, ret.as_str(), false);
        advance_probe_clock(0);
        assert_eq!(
            window.get_b1_verb_clicked() - before,
            1,
            "the tab lands on the verb row and Return activates the focused verb"
        );
    });
}

// ---- #1053: the walk host arm's declared extent --------------------
// The app-window walk arm composes the D-W1-4 extent as the body's
// own stop + every row + the D-5 button row (the host mounts the
// buttons as `TextButton` tab stops). The probe mirrors the mount
// shape — host + overlay + the composed expression — and reads the
// armed mask extent through the host's seam after a pushed model.

/// The walk mount's armed extent reflects a pushed `walk-model` with
/// rows AND buttons: the extent is `1 + rows + buttons`, not the
/// rows-only under-declaration (#1053's bug shape).
#[test]
fn w4_walk_arm_extent_counts_rows_and_buttons() {
    run_probe(|window| {
        let mut model = window.get_walk_model();
        model.rows = std::rc::Rc::new(slint::VecModel::from(vec![
            WalkRowModel {
                title: "row one".into(),
                emphasis: false,
            },
            WalkRowModel {
                title: "row two".into(),
                emphasis: true,
            },
        ]))
        .into();
        model.buttons = std::rc::Rc::new(slint::VecModel::from(vec![
            DialogButtonModel {
                label: "Prev".into(),
                role: DialogButtonRole::Dismiss,
                enabled: true,
                autofocus: false,
            },
            DialogButtonModel {
                label: "Next".into(),
                role: DialogButtonRole::Accept,
                enabled: true,
                autofocus: false,
            },
        ]))
        .into();
        window.set_walk_model(model);
        window.set_walk_open(true);
        advance_probe_clock(0);
        assert_eq!(
            window.get_walk_mask_extent(),
            1 + 2 + 2,
            "the armed extent is the composed body + rows + buttons count"
        );
    });
}
