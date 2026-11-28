// SPDX-License-Identifier: AGPL-3.0-only

//! The task-detail fit probe suite (#1504): at the tight-fit cell
//! (the 1.5x/900 px golden's logical size — 667×600 at base-scale
//! 1.5) no description content may paint flush at the window floor,
//! and the page scroll's affordance tracks the content's demand
//! (live at the tight fit, idle at a loose fit). Typed geometry over
//! the view's out seams; no pixel oracles, no text oracles. The
//! pre-fix geometry pinned the description viewport's clip edge AT
//! the window floor (the analyzer's 44/106 px real-clip runs on
//! `task-view-1.5-900`).

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock};

include!(concat!(env!("OUT_DIR"), "/task_detail_fit_probe.rs"));

/// The tight arm's seeded demand (the fixture-robustness law): the
/// reminder-rule rows grow the page's preferred height by token-floor
/// row heights, not text metrics.
const TIGHT_FIT_RULE_ROWS: usize = 12;
/// The overflow floor the tight arm asserts (px): three orders above
/// the observed cross-machine font-metric swing (the bare demand sat
/// 8 px over the viewport locally and at/below it on the runner).
const TIGHT_FIT_OVERFLOW_FLOOR: f32 = 100.0;

fn run_at(
    scale: f32,
    width: f32,
    height: f32,
    rule_count: usize,
    job: impl FnOnce(&TaskDetailFitProbe) + Send + 'static,
) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = TaskDetailFitProbe::new().expect("the probe window instantiates");
            window.global::<Theme>().set_base_scale(scale);
            window.set_task(TaskDetailModel {
                title: "Suppe fertigkochen".into(),
                stack_title: "To Do".into(),
                due_phrase: "in 9h · 23:45".into(),
                due_kind: DueKind::Soon,
                tracked_total: "0s".into(),
                rules: slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(
                    (0..rule_count)
                        .map(|i| RuleRowModel {
                            before: format!("in {}m", 10 + i).into(),
                            created: "in 12m".into(),
                            detail: "fires at 09:42".into(),
                            armed: false,
                            selected: false,
                        })
                        .collect::<std::vec::Vec<_>>(),
                ))),
                can_done: true,
                can_track_start: true,
                ..Default::default()
            });
            window.show().expect("show on the testing platform");
            window
                .window()
                .set_size(slint::LogicalSize::new(width, height));
            advance_probe_clock(0);
            job(&window);
        },
    );
}

/// The loose fit (the D2(a) rest state): the page's viewport is the
/// absorber, so the description geometry stays inside the window
/// floor's inset and nothing scrolls (the tight fit scrolls instead
/// — the affordance test below). The pre-fix geometry pinned the
/// description viewport's clip boundary AT the floor at the tight
/// fit (the analyzer's 44/106 px real-clip runs on
/// `task-view-1.5-900`); the typed discriminator here is the
/// scroll-affordance pair.
#[test]
fn the_description_section_stops_at_its_content_at_a_loose_fit() {
    run_at(0.5, 2000.0, 1800.0, 0, |window| {
        let height = window
            .window()
            .size()
            .to_logical(window.window().scale_factor())
            .height;
        let floor_inset = window.global::<Theme>().get_space_2();
        let bottom = window.get_description_bottom();
        // Inside the floor's inset (the #1435 padding leg is the
        // limit): at a loose fit the page's viewport is the absorber,
        // so nothing clips past the floor and nothing scrolls.
        assert!(
            bottom <= height - floor_inset,
            "the description viewport must stay inside the floor's inset (bottom = {bottom} vs limit {})",
            height - floor_inset
        );
    });
}

/// The scroll affordance exists exactly when needed: at the tight
/// fit the page's content extent exceeds the viewport (the page
/// scrolls); at a loose fit the viewport absorbs and the content
/// extent stays within it (nothing scrolls).
#[test]
fn the_page_scroll_affordance_tracks_the_content_demand() {
    run_at(1.5, 667.0, 600.0, TIGHT_FIT_RULE_ROWS, |window| {
        let overflow = window.get_page_content_height() - window.get_page_height();
        // A FLOOR on the overflow, not a boundary-equal pair: the bare
        // fixture's demand at this cell sits within single-digit px of
        // the viewport (608 vs 600 on the dev box) — inside the
        // cross-machine font-metric swing, so the strict `>` read an
        // exact equality on the CI runner (content == viewport == 600:
        // a Flickable stretches a non-demanding content to the
        // viewport). Each seeded rule row is one token-height row plus
        // its caption (~53 px, Theme token floors — not text
        // metrics), so twelve rows hold the overflow above 100 px even
        // under the smallest plausible line height — an order of
        // magnitude above the observed swing, and the floor keeps the
        // assertion a fixture-contract, not a boundary read.
        assert!(
            overflow >= TIGHT_FIT_OVERFLOW_FLOOR,
            "the tight fit demands the page scroll by a metric-robust margin \
             (overflow {overflow}, floor {TIGHT_FIT_OVERFLOW_FLOOR})"
        );
    });
    run_at(0.5, 2000.0, 1800.0, 0, |window| {
        assert!(
            window.get_page_content_height() <= window.get_page_height(),
            "the loose fit absorbs without scrolling (content {} vs viewport {})",
            window.get_page_content_height(),
            window.get_page_height()
        );
    });
}
