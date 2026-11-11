// SPDX-License-Identifier: AGPL-3.0-only

//! The task-card title-width probe suite (round-6 R6-4): pins the
//! collapse regression — the text column's width must track the card,
//! never freeze at the widest chip or the elide floor. The defect: an
//! empty decorations row (stretch-aligned, zero items) reported
//! `max = min = 0`, and the column merge clamped the whole column's
//! max to it, so an elided title kept a constant width from 250 px to
//! 500 px card widths. All assertions are structural relations over
//! measured widths — no font-metric pins, no text assertions
//! (AGENTS §5).

mod support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};

include!(concat!(env!("OUT_DIR"), "/task_card_title_probe.rs"));

use slint::ComponentHandle as _;

fn run_probe(job: impl FnOnce(&TaskCardTitleProbe, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let window = TaskCardTitleProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

/// The collapse's signature: the title's width was invariant in the
/// card width (108 px at 250, 340, and 500 in the recorded defect).
/// The title must claim the card's growth.
#[test]
fn the_title_tracks_the_card_width() {
    run_probe(|window, driver| {
        window.set_with_chips(true);
        window.set_card_w(250.0);
        driver.advance_time(0);
        let narrow = window.get_title_actual();
        window.set_card_w(500.0);
        driver.advance_time(0);
        let wide = window.get_title_actual();
        assert!(
            wide > narrow,
            "the title must grow with the card: {wide} <= {narrow}"
        );
    });
}

/// The whole name renders once the card is wide enough: the resolved
/// width reaches the title's intrinsic width (pre-fix the width froze
/// at the widest chip — 150 px against a 268 px intrinsic).
#[test]
fn a_wide_card_shows_the_whole_title() {
    run_probe(|window, driver| {
        window.set_with_chips(true);
        window.set_card_w(500.0);
        driver.advance_time(0);
        let resolved = window.get_title_actual();
        let intrinsic = window.get_title_preferred();
        assert!(
            resolved >= intrinsic,
            "the title must not elide inside a 500px card: \
             {resolved} < intrinsic {intrinsic}"
        );
    });
}

/// The no-chips card (the screenshot's focused card) is the sharpest
/// pre-fix signature: the column froze at the elide floor itself.
/// Wide card, long title: strictly above the floor.
#[test]
fn the_chipless_card_grows_past_the_elide_floor() {
    run_probe(|window, driver| {
        window.set_with_chips(false);
        window.set_card_w(500.0);
        driver.advance_time(0);
        let resolved = window.get_title_actual();
        let intrinsic = window.get_title_preferred();
        assert!(
            resolved >= intrinsic,
            "a chipless card must show the whole title: \
             {resolved} < intrinsic {intrinsic}"
        );
    });
}
