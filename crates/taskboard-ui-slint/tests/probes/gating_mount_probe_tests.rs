// SPDX-License-Identifier: AGPL-3.0-only

//! The gating-mount probe suite (#1464, the §5 `visible:`→`if` law's
//! structural tier): per surface, the converted gates must UNMOUNT
//! (an `if` gate removes the element; the old `visible: false` kept
//! the layout slot). Typed mount/geometry assertions only — element
//! id lookups, realized heights, out-property reads; no text oracles,
//! no pixel oracles, no sleeps.

use crate::support;

use slint::{ModelRc, VecModel};

use support::driver::{DriverOptions, advance_probe_clock, find_all_by_id_on};

include!(concat!(env!("OUT_DIR"), "/gating_mount_probe.rs"));

fn run_probe(job: impl FnOnce(&GatingMountProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = GatingMountProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            advance_probe_clock(0);
            job(&window);
        },
    );
}

/// The today model with one populated section (a header, no rows —
/// the count drives the gate, not the rows) and one empty section.
fn today_model_with_empty_section() -> TodayModel {
    let sections = vec![
        TodaySectionModel {
            label: "Overdue".into(),
            count: 1,
            rows: ModelRc::new(VecModel::from(std::vec::Vec::new())),
        },
        TodaySectionModel {
            label: "Done".into(),
            count: 0,
            rows: ModelRc::new(VecModel::from(std::vec::Vec::new())),
        },
    ];
    TodayModel {
        sections: ModelRc::new(VecModel::from(sections)),
        empty: false,
        tracked_today: ModelRc::new(VecModel::from(std::vec::Vec::new())),
        tracked_day_total: "".into(),
    }
}

/// The palette's rows-present, no-overflow model (neither caption
/// gate holds).
fn palette_rows() -> ModelRc<PaletteRowModel> {
    let rows = vec![PaletteRowModel {
        text: "task-0".into(),
        meta: "board › stack".into(),
        emphasis: false,
        history: false,
        tombstone: false,
    }];
    ModelRc::new(VecModel::from(rows))
}

/// (i) today: the empty section contributes zero height — its header
/// is UNMOUNTED (exactly one header resolves: the populated
/// section's) and the empty delegate's realized height is zero while
/// the populated one renders.
#[test]
fn the_empty_today_section_mounts_no_header_and_zero_height() {
    run_probe(|window| {
        window.set_today(today_model_with_empty_section());
        advance_probe_clock(0);
        let headers = find_all_by_id_on(window, "TodaySectionBlock::section-header");
        assert_eq!(
            headers.len(),
            1,
            "the populated section mounts exactly one header; the empty one mounts none"
        );
        let blocks = find_all_by_id_on(window, "TodayView::section-block");
        assert_eq!(blocks.len(), 2, "both section delegates mount");
        let mut zero = 0;
        let mut live = 0;
        for block in &blocks {
            if block.size().height == 0.0 {
                zero += 1;
            } else {
                live += 1;
            }
        }
        assert_eq!(zero, 1, "the empty section collapses to zero height");
        assert_eq!(live, 1, "the populated section renders");
    });
}

/// (iv) palette: with rows present and no overflow, neither caption
/// resolves (both gates unmount).
#[test]
fn the_palette_captures_neither_caption_when_unneeded() {
    run_probe(|window| {
        window.set_palette_rows(palette_rows());
        window.set_palette_overflow(0);
        advance_probe_clock(0);
        for id in [
            "SearchPalette::overflow-caption",
            "SearchPalette::empty-state-caption",
        ] {
            let elements = find_all_by_id_on(window, id);
            assert!(
                elements.is_empty(),
                "{id} must stay unmounted with rows present and no overflow"
            );
        }
    });
}
