// SPDX-License-Identifier: AGPL-3.0-only

//! The small-widget probe suite (round-9 W5, plan §4): the theme
//! tone-ladder functions' eight Tone arms (T1) and the five new
//! primitives' contracts. Every assertion is a typed root seam or a
//! structural relation — no text assertions, no clocks (AGENTS §5).

mod support;

/// The one-platform mode this binary standardizes on: the `support`
/// self-tests read it so their `DriverOptions` agree with the members
/// sharing the process (`driver::run`'s guard asserts one platform
/// per process; see #1016).
pub(crate) const HEADLESS_RENDERER: bool = true;

use support::driver::{DriverOptions, advance_probe_clock};

include!(concat!(env!("OUT_DIR"), "/small_widget_probe.rs"));

fn run_probe(job: impl FnOnce(&SmallWidgetProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let window = SmallWidgetProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

fn settle() {
    advance_probe_clock(0);
    advance_probe_clock(0);
}

// ---- T1: the tone ladder (F-3, #952) ------------------------------

/// `Semantic.tone-color` maps every `Tone` arm onto the canonical
/// colour: neutral→text-secondary, info→text-primary, the semantic
/// tones verbatim, offline→its own (text-secondary) mapping.
#[test]
fn tone_color_maps_every_arm_onto_the_canonical_colour() {
    run_probe(|window| {
        settle();
        assert_eq!(window.get_tone_neutral(), window.get_theme_text_secondary());
        assert_eq!(window.get_tone_info(), window.get_theme_text_primary());
        assert_eq!(window.get_tone_success(), window.get_semantic_success());
        assert_eq!(window.get_tone_warning(), window.get_semantic_warning());
        assert_eq!(window.get_tone_danger(), window.get_semantic_danger());
        assert_eq!(window.get_tone_overdue(), window.get_semantic_overdue());
        assert_eq!(
            window.get_tone_sync_active(),
            window.get_semantic_sync_active()
        );
        assert_eq!(window.get_tone_offline(), window.get_semantic_offline());
    });
}

/// `Semantic.tone-fill` is the 0.2-alpha derivation of the same
/// ladder (the Badge family's translucent chip fill).
#[test]
fn tone_fill_is_the_alpha_derivation_of_tone_color() {
    run_probe(|window| {
        settle();
        assert_eq!(window.get_fill_success(), window.get_literal_success_fill());
        assert_eq!(window.get_fill_danger(), window.get_literal_danger_fill());
    });
}
