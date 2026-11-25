// SPDX-License-Identifier: AGPL-3.0-only

//! The empty-board stub suite (round 8 L1, the B-empty C1 class):
//! the real `BoardLayout` on the shared testing backend with zero
//! stacks. The severing bound: the empty-board CTAs never paint
//! flush to the canvas edges — each CTA's window rect stays strictly
//! inside the viewport on both sides. Typed geometry assertions over
//! the element-geometry seam; no pixel/text oracles. The pre-fix
//! fill put both buttons' borders at x=0 / x=window-width (the
//! analyzer's 28 px flush runs on `board-empty-1.0-1000`).

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock};

include!(concat!(env!("OUT_DIR"), "/board_empty_stub_probe.rs"));

fn run_probe(job: impl FnOnce(&BoardEmptyStubProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = BoardEmptyStubProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            window
                .window()
                .set_size(slint::LogicalSize::new(1000.0, 900.0));
            advance_probe_clock(0);
            job(&window);
        },
    );
}

/// Each CTA sits strictly inside the viewport: x > 0 and
/// x + width < window width (the pre-fix fill painted x=0 and
/// x=window-width borders).
#[test]
fn the_empty_board_ctas_stay_off_the_canvas_edges() {
    run_probe(|window| {
        let width = window
            .window()
            .size()
            .to_logical(window.window().scale_factor())
            .width;
        for id in [
            "BoardLayout::new-stack-cta",
            "BoardLayout::default-preset-cta",
        ] {
            let elements = support::driver::find_all_by_id_on(window, id);
            assert_eq!(elements.len(), 1, "{id} mounts exactly once");
            let el = &elements[0];
            let pos = el.absolute_position();
            let size = el.size();
            assert!(
                pos.x > 0.0,
                "{id} left edge must sit off the canvas (x = {})",
                pos.x
            );
            assert!(
                pos.x + size.width < width,
                "{id} right edge must sit off the canvas (x + w = {} vs {width})",
                pos.x + size.width
            );
        }
    });
}
