// SPDX-License-Identifier: AGPL-3.0-only

//! The focus-loss seam's component pin (round-3 L1, plan §9.2.7):
//! the real `TextField`'s mechanism-A seam fires `focus-lost`
//! exactly once on a sibling `FocusScope` click, and never on a
//! plain `TouchArea` click — the recorded negative that keeps the L2
//! seam-settle necessary. Hosted on the shared testing backend (the
//! `virtualisation.rs` `run_probe` shape): the window must
//! instantiate headless, not on the winit backend.

mod support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/focus_seam_probe.rs"));

fn run_probe(job: impl FnOnce(&SeamProbeWindow) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = SeamProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

fn click(window: &SeamProbeWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    let button = i_slint_core::items::PointerEventButton::Left;
    for event in [
        slint::platform::WindowEvent::PointerMoved { position },
        slint::platform::WindowEvent::PointerPressed { position, button },
        slint::platform::WindowEvent::PointerReleased { position, button },
    ] {
        window.window().dispatch_event(event);
    }
}

#[test]
fn the_seam_fires_once_per_focus_loss_and_not_on_touch_area_clicks() {
    run_probe(|window| {
        let counter = || window.global::<SeamProbe>().get_lost();

        // The init-focus gain must not fire the seam (no spurious
        // loss at init-focus).
        assert_eq!(counter(), 0);

        // A plain TouchArea click does not move focus — the recorded
        // negative.
        click(window, 136.0, 40.0);
        advance_probe_clock(0);
        assert_eq!(counter(), 0, "a TouchArea click is invisible to the seam");

        // A sibling FocusScope click steals focus: exactly one loss.
        click(window, 100.0, 40.0);
        advance_probe_clock(0);
        assert_eq!(counter(), 1, "one loss per focus steal");
    });
}
