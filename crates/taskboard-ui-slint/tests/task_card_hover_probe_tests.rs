// SPDX-License-Identifier: AGPL-3.0-only

//! The done checkbox's hover-flag probe suite (UX-R8 chunk-5 D-8,
//! US-6/AC-1): the card's `done-hover` out property resolves and
//! reads false at rest — the model fact the house hover treatment
//! consumes (the flag, never styling text; the transition itself is
//! platform-driven, the `TouchArea`'s has-hover).

mod support;

use slint::ComponentHandle as _;

use support::driver::{DriverOptions, SlintDriver};

include!(concat!(env!("OUT_DIR"), "/task_card_hover_probe.rs"));

fn run_probe(job: impl FnOnce(&TaskCardHoverProbe, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let window = TaskCardHoverProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

/// US-6/AC-1: the flag exists, resolves through the card's checkbox
/// seam, and reads false at rest (the treatment composes on it).
#[test]
fn us6_s1_the_done_checkbox_carries_the_hover_flag() {
    run_probe(|window, _driver| {
        assert!(!window.get_hover(), "the checkbox is unhovered at rest");
    });
}
