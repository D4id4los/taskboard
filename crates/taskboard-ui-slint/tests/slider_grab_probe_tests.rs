// SPDX-License-Identifier: AGPL-3.0-only

//! The slider-grab probe's matrix (round-3 D20b, plan §5.2): the
//! pinned `i-slint-core` Flickable intercepts a drag inside it once
//! the gesture accumulates ≥ 8 logical px on the pannable axis within
//! 500 ms of the press (`DISTANCE_THRESHOLD`/`DURATION_THRESHOLD`),
//! and holds the press back from children for the first 100 ms
//! (`FORWARD_DELAY`). On the settings surface that interception is
//! what steals the slider's drag.
//!
//! Matrix: **P1** reproduces the steal on today's shape; **P2**
//! exonerates the preview re-layout (the steal persists when the
//! preview writes nothing); **P3** proves fix A —
//! `mouse-drag-pan-enabled: false` — restores the full gesture while
//! wheel scrolling stays alive. Deterministic throughout: the
//! dispatch sequence lands inside the thresholds by construction, no
//! sleeps, the mock clock advances explicitly.

mod support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/slider_grab_probe.rs"));

fn run_probe(job: impl FnOnce(&GrabProbeWindow) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = GrabProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// The gesture of finding 24: press on the slider track, then drag
/// with ≥ 8 px of vertical wobble per move inside the 500 ms window —
/// the intercept signature's input. All events dispatch inside one
/// platform turn (well under every threshold on the mock clock).
fn slider_drag(window: &GrabProbeWindow) {
    // The track's centre at base-scale 1.0: x = 64 + 96 (half the
    // 192 px track), y = 3·48 (fillers) + 24 (row centre).
    let start = slint::LogicalPosition::new(160.0, 168.0);
    let button = i_slint_core::items::PointerEventButton::Left;
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position: start });
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: start,
            button,
        });
    // Five moves, each 10 px of vertical delta — every one clears the
    // 8 px distance threshold, all inside the 500 ms duration window.
    // The drag runs UPWARD: the flick starts at content-y 0 (its
    // upper bound), so a downward drag clamps to the bound and pans
    // nothing — the gesture of the finding drags toward new content.
    for i in [1.0f32, 2.0, 3.0, 4.0, 5.0] {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(200.0, 168.0 - i * 10.0),
            });
    }
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(160.0, 118.0),
            button,
        });
}

/// The flick's `content-y` after the event turn and any panning
/// animation ticks have settled on the mock clock.
fn settled_flick_y(window: &GrabProbeWindow) -> f32 {
    advance_probe_clock(600);
    window.global::<GrabProbe>().get_flick_y()
}

/// P1 — the recorded reproduction (plan §5.2): the Flickable steals
/// the gesture — the slider's `TouchArea` never sees press or moves,
/// zero commits land, and the flick pans instead.
#[test]
fn p1_the_flickable_steals_a_wobbling_slider_drag() {
    run_probe(|window| {
        slider_drag(window);
        let y = settled_flick_y(window);
        assert_eq!(
            window.global::<GrabProbe>().get_pressed(),
            0,
            "the press never reached the slider (the 100 ms forward delay ate it)"
        );
        assert_eq!(
            window.global::<GrabProbe>().get_previews(),
            0,
            "no preview survived the interception"
        );
        assert_eq!(
            window.global::<GrabProbe>().get_committed(),
            0,
            "no commit survived the interception"
        );
        assert!(y < 0.0, "the flick panned instead (content-y moved)");
    });
}

/// P2 — the preview re-layout is exonerated (plan §5.2): the same
/// drag with the preview callback writing a counter *instead of* the
/// theme still steals — the steal does not need a re-layout.
#[test]
fn p2_the_steal_persists_without_the_preview_re_layout() {
    run_probe(|window| {
        window.global::<GrabProbe>().set_preview_writes_theme(false);
        slider_drag(window);
        let y = settled_flick_y(window);
        assert_eq!(window.global::<GrabProbe>().get_previews(), 0);
        assert_eq!(window.global::<GrabProbe>().get_committed(), 0);
        assert!(y < 0.0, "the flick panned with no re-layout in flight");
    });
}

/// P3 — fix A works and keeps the legit scroll paths (plan §5.2):
/// with `mouse-drag-pan-enabled: false` the same drag runs the full
/// preview chain and commits exactly once on release; a drag on the
/// filler area pans not at all via mouse; wheel scrolling still pans.
#[test]
fn p3_pan_disabled_restores_the_gesture_and_keeps_the_wheel() {
    run_probe(|window| {
        window.global::<GrabProbe>().set_pan_enabled(false);
        slider_drag(window);
        assert_eq!(
            window.global::<GrabProbe>().get_pressed(),
            1,
            "the press reached the slider"
        );
        assert_eq!(
            window.global::<GrabProbe>().get_previews(),
            5,
            "every move landed as a preview"
        );
        assert_eq!(
            window.global::<GrabProbe>().get_committed(),
            1,
            "release committed exactly once"
        );
        assert_eq!(
            settled_flick_y(window),
            0.0,
            "the mouse drag panned nothing"
        );

        // The wheel stays alive: a scroll event pans the flick even
        // with mouse-drag panning dead (the builtin's documented
        // independence, the kiosk/desktop scroll paths).
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(160.0, 96.0),
                delta_x: 0.0,
                delta_y: -40.0,
            });
        let wheel_y = settled_flick_y(window);
        assert!(
            wheel_y < 0.0,
            "the wheel pans while mouse-drag panning is off"
        );
    });
}
