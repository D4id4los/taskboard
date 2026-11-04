// SPDX-License-Identifier: AGPL-3.0-only

//! The board-bar threshold probe's suite (r4 D40): below the segment
//! budget the reviewed strip stays; above it the shared dropdown takes
//! the switcher role — picking routes `segment-activated` (zero
//! adapter changes), the ladder switches boards, Escape closes.

mod support;

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/board_bar_probe.rs"));

use slint::platform::Key as K;

fn run_probe(job: impl FnOnce(&BoardBarProbeWindow) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = BoardBarProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

fn key(window: &BoardBarProbeWindow, k: K) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: k.into() });
}

fn click(window: &BoardBarProbeWindow, x: f32, y: f32) {
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

/// Below the budget the strip stays (no dropdown mount); above it the
/// dropdown replaces the strip.
#[test]
fn the_threshold_switches_the_switcher() {
    run_probe(|window| {
        assert!(window.get_seven_strip(), "seven segments keep the strip");
        assert!(
            window.get_nine_dropdown(),
            "nine segments take the dropdown"
        );
        assert!(!window.get_nine_open(), "closed at mount");
    });
}

/// Above the budget: the dropdown's click opens, the anchor is the
/// selected segment, the ladder moves, Return routes the existing
/// `segment-activated` seam, Escape closes without routing.
#[test]
fn the_dropdown_switcher_routes_the_strip_seam() {
    run_probe(|window| {
        // The nine-bar sits at y = space-2 + space-8 (72); its centre
        // is the click point (the bar's field spans the row).
        let x = window.get_nine_x() / 1.0 + 40.0;
        let y = window.get_nine_y() / 1.0 + 16.0;
        click(window, x, y);
        assert!(window.get_nine_open(), "the field click opens");
        assert_eq!(
            window.get_nine_highlight(),
            4,
            "the anchor is the selected segment"
        );

        key(window, K::DownArrow);
        assert_eq!(window.get_nine_highlight(), 5, "Down moves");
        key(window, K::Return);
        assert!(!window.get_nine_open(), "selecting closes");
        assert_eq!(
            window.global::<BoardBarProbe>().get_activated(),
            1,
            "exactly one route"
        );
        assert_eq!(
            window.global::<BoardBarProbe>().get_activated_index(),
            5,
            "the picked segment's index routed"
        );

        // Escape never routes.
        let before = window.global::<BoardBarProbe>().get_activated();
        click(window, x, y);
        assert!(window.get_nine_open());
        key(window, K::Escape);
        assert!(!window.get_nine_open(), "Escape closes");
        assert_eq!(
            window.global::<BoardBarProbe>().get_activated(),
            before,
            "Escape never routes"
        );
    });
}
