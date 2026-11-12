// SPDX-License-Identifier: AGPL-3.0-only

//! The stat-bars severing probe suite (#725): pins the collapse
//! regression at the review view's reflection-section shape. `StatBars`
//! mounts unconditionally (D-9 tier 2 — the values collapse, never the
//! section); with `bars` empty the strip's layout reports
//! `max = min = 2·padding`, the section column's merge adopts it as its
//! own max-width, and the main-axis distribution freezes the column at
//! its children's minimum floor at every row width. All assertions are
//! structural relations over measured widths — no font-metric pins, no
//! text assertions (AGENTS §5).

mod support;

use std::rc::Rc;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};

include!(concat!(env!("OUT_DIR"), "/stat_bars_severing_probe.rs"));

use slint::{ModelRc, VecModel};

fn bars_model(bars: Vec<StatBarModel>) -> ModelRc<StatBarModel> {
    ModelRc::from(Rc::new(VecModel::from(bars)))
}

fn run_probe(job: impl FnOnce(&StatBarsSeveringProbe, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let window = StatBarsSeveringProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

fn one_bar() -> StatBarModel {
    StatBarModel {
        base: 0.4,
        stack: 0.2,
    }
}

/// The defect's signature: with the strip's bars empty, the section
/// froze at its caption floor (96 px at both the 250 px and the 500 px
/// row — the min-override when the strip's empty max clamped the
/// column's max below it). The section must claim the row's growth —
/// the D-9 skeleton promise covers the section's width, not just its
/// presence.
#[test]
fn the_empty_sections_twin_tracks_the_row_width() {
    run_probe(|window, driver| {
        window.set_probe_bars(bars_model(Vec::new()));
        window.set_col_w(250.0);
        driver.advance_time(0);
        let narrow = window.get_section_actual();
        window.set_col_w(500.0);
        driver.advance_time(0);
        let wide = window.get_section_actual();
        assert!(
            wide > narrow,
            "the section must grow with the row while the bars are \
             empty: {wide} <= {narrow}"
        );
    });
}

/// The severing remedy is zero-delta for the non-empty pass: a
/// populated strip must claim its stretch share and fill the row with
/// the witness — the cap fixes only the empty pass's collapse, nothing
/// else.
#[test]
fn a_populated_strip_claims_its_row_share() {
    run_probe(|window, driver| {
        window.set_probe_bars(bars_model(vec![one_bar()]));
        window.set_col_w(250.0);
        driver.advance_time(0);
        let narrow = window.get_section_actual();
        window.set_col_w(500.0);
        driver.advance_time(0);
        let wide = window.get_section_actual();
        let wide_witness = window.get_witness_actual();
        assert!(
            wide > narrow,
            "a populated section must grow with the row: {wide} <= {narrow}"
        );
        assert!(
            (wide + wide_witness - 500.0).abs() < 0.5,
            "the populated row must fill its width: {wide} + \
             {wide_witness} != 500"
        );
    });
}
