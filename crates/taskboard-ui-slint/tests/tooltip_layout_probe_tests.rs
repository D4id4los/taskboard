// SPDX-License-Identifier: AGPL-3.0-only

//! The tooltip layout probe suite (r4 chunk 4, plan §2.2): the
//! four-state content matrix over the real `TooltipPanel`. The D13
//! pin (the F5 repro): a tooltip with neither description nor keys
//! measures exactly the caption-only height — hidden parts reserve no
//! layout space. The remaining states pin the ordering relations
//! (description and keys each add content; both add both) and the
//! width relation (keys widen, the r2 D14 fact). All assertions are
//! structural/relational — no font-metric pins, no text assertions
//! (AGENTS §5). The red measurement against the always-mounted rows
//! is recorded in the stage-2 report.

mod support;

use slint::{ModelRc, SharedString, VecModel};
use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};

include!(concat!(env!("OUT_DIR"), "/tooltip_layout_probe.rs"));

use slint::ComponentHandle as _;

fn run_probe(job: impl FnOnce(&TooltipLayoutProbeWindow, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let window = TooltipLayoutProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

/// One hover invocation, then the settle ticks the journeys use (the
/// delay window plus a layout pass — the mirrors write on the size
/// change; the probe has no delay callback connected, so the extra
/// window is inert).
fn hover(
    window: &TooltipLayoutProbeWindow,
    driver: &mut SlintDriver,
    description: &str,
    keys: &[&str],
) {
    let keys_model = if keys.is_empty() {
        ModelRc::default()
    } else {
        ModelRc::new(VecModel::from(
            keys.iter()
                .map(|k| SharedString::from(*k))
                .collect::<Vec<_>>(),
        ))
    };
    window.global::<TooltipState>().invoke_hover(
        "short".into(),
        description.into(),
        keys_model,
        200.0,
        150.0,
        32.0,
        32.0,
    );
    driver.advance_time(600);
    driver.advance_time(0);
}

/// The four-state matrix. State 1 is the F5 pin (caption-only height);
/// states 2–4 are the ordering relations.
#[test]
fn the_tooltip_panel_does_not_reserve_space_for_hidden_parts() {
    run_probe(|window, driver| {
        let state = window.global::<TooltipState>();
        let tolerance = 0.5;

        // State 1 — neither: the F5 repro. The panel must measure the
        // caption line plus its own padding, nothing else.
        hover(window, driver, "", &[]);
        let caption_only = window.get_one_line_height() + window.get_panel_content_pad();
        let neither_height = state.get_tip_height();
        let neither_width = state.get_tip_width();
        assert!(
            (neither_height - caption_only).abs() <= tolerance,
            "a caption-only tooltip is caption + padding: got {neither_height}, \
             expected {caption_only} (hidden rows reserve layout space)"
        );
        assert!(neither_width > 0.0, "the panel measured its width");
        state.invoke_unhover();

        // State 2 — description only: strictly taller than neither.
        hover(window, driver, "a help sentence", &[]);
        let description_height = state.get_tip_height();
        assert!(
            description_height > neither_height + tolerance,
            "the description adds height: {description_height} vs {neither_height}"
        );
        state.invoke_unhover();

        // State 3 — keys only: strictly taller than neither, and the
        // keys widen the panel (the r2 D14 width fact, relation only).
        hover(window, driver, "", &["Ctrl", "K"]);
        let keys_height = state.get_tip_height();
        let keys_width = state.get_tip_width();
        assert!(
            keys_height > neither_height + tolerance,
            "the hint row adds height: {keys_height} vs {neither_height}"
        );
        assert!(
            keys_width > neither_width,
            "the hint row widens the tip: {keys_width} vs {neither_width}"
        );
        state.invoke_unhover();

        // State 4 — both: taller than either single-part state.
        hover(window, driver, "a help sentence", &["Ctrl", "K"]);
        let both_height = state.get_tip_height();
        assert!(
            both_height > description_height + tolerance,
            "the keys add height over the description: {both_height} vs \
             {description_height}"
        );
        assert!(
            both_height > keys_height + tolerance,
            "the description adds height over the keys: {both_height} vs \
             {keys_height}"
        );
        state.invoke_unhover();
    });
}
