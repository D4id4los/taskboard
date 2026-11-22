// SPDX-License-Identifier: AGPL-3.0-only

//! The armed-ring probe's typed pin (round 5 A2, #1204): the host's
//! arm path reaches the `MultiLineEditor`'s border — an
//! armed-but-not-focused editor shows the focus ring (the walk /
//! restore case, the mount handoff's moments), an unarmed one does
//! not. The typed `ring-visible` seam, never pixels.

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/armed_ring_probe.rs"));

fn run_probe(job: impl FnOnce(&ArmedRingProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = ArmedRingProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

#[test]
fn the_armed_flag_reaches_the_editor_ring() {
    run_probe(|window| {
        advance_probe_clock(0);
        assert!(
            !window.get_armed_ring_visible(),
            "the unarmed, unfocused editor shows no ring"
        );
        window.set_armed(true);
        advance_probe_clock(0);
        assert!(
            window.get_armed_ring_visible(),
            "the armed-but-not-focused editor shows the ring"
        );
    });
}
