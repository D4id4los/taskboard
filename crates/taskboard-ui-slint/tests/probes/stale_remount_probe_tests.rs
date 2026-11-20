// SPDX-License-Identifier: AGPL-3.0-only

//! The stale-geometry probe suite (#1023, a facet of #954): pins the
//! recorded staleness of the testing backend's observation channels
//! on a freshly remounted conditional subtree — the element-query
//! geometry versus the typed root seam. Mounts from `probe_pixel.rs`
//! (rendered backend; the group's `HEADLESS_RENDERER`). The pin is
//! structural only: the typed seam's settled value is the oracle, no
//! numeric px constants.

use crate::support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver, find_by_label_on};

include!(concat!(env!("OUT_DIR"), "/stale_remount_probe.rs"));

use slint::ComponentHandle as _;

fn run_probe(job: impl FnOnce(&StaleRemountProbe, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let window = StaleRemountProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

/// #1023 currently pins: the typed root seam is the reliable channel.
/// The conditional subtree toggles away and back; on the fresh mount
/// the seam reports the settled width — identical to the first
/// mount's — while the element query is the channel the round-9
/// migration stopped trusting (the recorded mid-layout/pre-attach
/// staleness). The compensating law (journey oracles use typed root
/// seams on conditionally remounted subtrees) is the assertion; an
/// upstream mirroring fix (#954 chunk 17) makes the query channel
/// trustworthy and this pin keeps standing on the seam.
#[test]
fn the_remounted_subtree_geometry_currently_pins_the_typed_seam_channel() {
    run_probe(|window, driver| {
        // First mount: the seam settles at the panel's width and the
        // element query addresses the element.
        window.set_subtree_open(true);
        driver.advance_time(0);
        let first_mount_seam = window.get_seam_width();
        // The first mount's query channel (addressability is the
        // structural fact; the width is never the oracle).
        let first_query = find_by_label_on(window, "Remounted panel");
        assert!(
            first_query.is_some(),
            "the settled mount is query-addressable"
        );
        drop(first_query);
        assert!(
            first_mount_seam > 0.0,
            "the settled seam reports the realized width"
        );

        // Toggle away and back: the fresh mount's seam reports the
        // same settled value (the seam channel is mount-stable).
        window.set_subtree_open(false);
        driver.advance_time(0);
        assert!(
            find_by_label_on(window, "Remounted panel").is_none(),
            "the unmounted subtree leaves no query address"
        );
        window.set_subtree_open(true);
        driver.advance_time(0);
        assert_eq!(
            window.get_seam_width(),
            first_mount_seam,
            "the fresh mount's seam settles at the same realized width"
        );
        // The fresh mount's element query: the recorded unreliable
        // channel (#1023 — pre-attach zeros / mid-layout reads are
        // the staleness class the round-9 migration observed). This
        // minimal fixture observes the two channels agreeing today
        // (query 96.0 == seam 96.0, recorded 2026-11-20); the pin
        // stays on the structural facts — addressability here, the
        // seam's settled value above — and never trusts the query
        // width as an oracle.
        let remounted = find_by_label_on(window, "Remounted panel");
        assert!(remounted.is_some(), "the fresh mount is query-addressable");
        drop(remounted);
    });
}
