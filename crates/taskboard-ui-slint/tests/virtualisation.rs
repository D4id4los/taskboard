// SPDX-License-Identifier: AGPL-3.0-only

//! The virtualisation engagement suite (#373 stage-1 plan D-3): the
//! slint compiler's `ListView` contract is undocumented and
//! version-pinned — a lost element name or a lost length property
//! makes every wrapped `for` silently render **eager**, goldens and
//! journeys cannot see it, and every virtualisation win is forfeited.
//! These probes guard the contract at runtime, forever (the
//! `pin_guard.rs` discipline, applied at runtime instead of manifest
//! level): they must never be `#[ignore]`d or fixture-starved.
//!
//! The observable is the **live-instantiation count** via the
//! backend's per-instance element query (one entry per live
//! repetition instance) — no production-markup hooks. An eager shape
//! renders all N instances; an engaged shape renders only the
//! viewport window. The ratios asserted are failure detectors on the
//! eager garbage, deliberately not success ceilings: strictly better
//! virtualisation (a smaller window) still passes.

mod support;

use support::driver::{
    DriverOptions, SlintDriver, advance_probe_clock, find_all_by_id_on, find_by_label_on,
};

include!(concat!(env!("OUT_DIR"), "/virtualisation_probe.rs"));

use slint::ComponentHandle as _;

fn run_probe(job: impl FnOnce(&mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            job(driver);
        },
    );
}

fn model_of(n: usize) -> slint::ModelRc<slint::SharedString> {
    slint::ModelRc::new(slint::VecModel::from(
        (0..n)
            .map(|i| slint::format!("card-{i}"))
            .collect::<Vec<_>>(),
    ))
}

/// The eager control: 300 delegates in a 400 px viewport all exist —
/// the probe's counting path works and the eager shape is what we
/// think it is.
#[test]
fn the_eager_control_instantiates_every_delegate() {
    run_probe(|_driver| {
        let probe = EagerProbe::new().expect("the eager probe instantiates");
        probe.show().expect("show on the testing platform");
        probe.set_items(model_of(300));
        advance_probe_clock(0);

        let instances = find_all_by_id_on(&probe, "EagerProbe::item");
        eprintln!("eager instances: {}", instances.len());
        assert_eq!(
            instances.len(),
            300,
            "the plain `for` must instantiate every delegate (the eager control)"
        );
        let first = find_by_label_on(&probe, "card-0");
        assert!(first.is_some(), "the counting path addresses delegates");
    });
}

/// The engaged probe: the identical delegate inside the real
/// `ListView` renders a viewport-scale window, not 300 instances. A
/// lost element name or any lost length property re-arms the eager
/// shape and fails the ratio (a failure detector, never a success
/// ceiling: a smaller window passes). The labelled-element guard
/// rejects the degenerate "engaged but renders nothing" breakage.
#[test]
fn the_list_view_engages_the_virtual_repeater() {
    run_probe(|_driver| {
        let probe = ListViewProbe::new().expect("the list-view probe instantiates");
        probe.show().expect("show on the testing platform");
        probe.set_items(model_of(300));
        probe.set_total(300);
        advance_probe_clock(0);

        let instances = find_all_by_id_on(&probe, "ListViewProbe::item");
        assert!(
            !instances.is_empty(),
            "an engaged ListView still renders its viewport"
        );
        assert!(
            instances.len() * 4 < 300,
            "the ListView must engage the compiler's virtual repeater: got {} \
             live instances of 300 — the eager shape means the element name or \
             one of the five length properties (content-y/height/width, \
             visible-height/width) no longer satisfies the compiler contract",
            instances.len()
        );
        assert!(
            find_by_label_on(&probe, "card-0").is_some(),
            "the engaged viewport renders its first delegate"
        );
    });
}

/// The seek smoke: driving the nonce edge to a deep row makes that
/// row live and lands it inside the viewport (the focus-into-view
/// seam's runtime half — the top-anchor seek through the runtime's
/// own average-height divisor).
#[test]
fn the_seek_makes_a_deep_row_live_and_visible() {
    run_probe(|_driver| {
        let probe = ListViewProbe::new().expect("the list-view probe instantiates");
        probe.show().expect("show on the testing platform");
        probe.set_items(model_of(300));
        probe.set_total(300);
        advance_probe_clock(0);

        assert!(
            find_by_label_on(&probe, "card-250").is_none(),
            "before the seek the deep row is off-viewport (not instantiated)"
        );

        probe.set_req_row(250);
        probe.set_req_nonce(1);
        advance_probe_clock(0);

        let target = find_by_label_on(&probe, "card-250")
            .unwrap_or_else(|| panic!("the seek instantiates the target row"));
        let position = target.absolute_position();
        let size = target.size();
        assert!(
            position.y >= 0.0 && position.y + size.height <= 400.0,
            "the sought row sits fully inside the 400 px viewport, got y={} h={}",
            position.y,
            size.height
        );
    });
}
