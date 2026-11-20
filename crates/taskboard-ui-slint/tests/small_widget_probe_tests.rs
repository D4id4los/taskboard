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

// ---- T2: StatusNotice ----------------------------------------------

/// The boxed shape reproduces the `ErrorLine` grammar: raised box,
/// focus-ring-width `Theme.border` border, radius, danger caption,
/// word-wrap, and the horizontal stretch inside a fixed-sibling row
/// (the R-3 width identity: the notice absorbs the leftover width).
#[test]
fn the_boxed_notice_reproduces_the_error_line_grammar_and_stretches() {
    run_probe(|window| {
        settle();
        assert_eq!(
            window.get_boxed_notice_background(),
            window.get_theme_surface_raised()
        );
        assert_eq!(
            window.get_boxed_notice_border_width(),
            window.get_theme_focus_ring_width()
        );
        assert_eq!(
            window.get_boxed_notice_text_color(),
            window.get_semantic_danger()
        );
        assert!(window.get_boxed_notice_wrapping());
        // The text inset (space-2 both sides) and the vertical
        // centring (the ErrorLine's y law).
        assert_eq!(window.get_boxed_notice_text_x(), window.get_theme_space_2());
        // The R-3 stretch: fixed space-15 sibling + the notice takes
        // the rest of the host row.
        assert!(
            window.get_boxed_notice_width() > window.get_theme_space_15() * 2.0,
            "the boxed notice absorbs the row's leftover width"
        );
        // The min-height-from-preferred law: the box is at least the
        // text's height plus the space-1 insets.
        assert!(window.get_boxed_notice_height() >= window.get_theme_space_1() * 2.0);
    });
}

/// The inline shape is the bare caption: no box paint, no border,
/// the tone ladder's colour per arm, elide by default and word-wrap
/// under `wrap: true` (the absence captions' law).
#[test]
fn the_inline_notice_is_the_bare_caption_with_the_ladder_colour() {
    run_probe(|window| {
        settle();
        assert_eq!(
            window.get_inline_notice_text_color(),
            window.get_semantic_danger()
        );
        assert_eq!(
            window.get_inline_notice_background(),
            slint::Color::from_argb_encoded(0)
        );
        assert_eq!(window.get_inline_notice_border_width(), 0.0);
        assert!(window.get_inline_notice_eliding());
        assert!(window.get_wrapped_notice_wrapping());
        // The neutral arm rides the ladder's secondary notice colour.
        assert_eq!(
            window.get_wrapped_notice_text_color(),
            window.get_theme_text_secondary()
        );
    });
}

// ---- T3: ConfirmStrip ----------------------------------------------

/// The destructive grammar: message danger, confirm danger (the
/// board/timers remove strips' exact tones), and the Confirm/Cancel
/// seams route their own counters.
#[test]
fn the_destructive_strip_paints_the_danger_grammar_and_routes_the_seams() {
    run_probe(|window| {
        settle();
        assert_eq!(
            window.get_destructive_message_color(),
            window.get_semantic_danger()
        );
        assert_eq!(
            window.get_destructive_confirm_color(),
            window.get_semantic_danger()
        );
        let confirm = support::driver::find_by_label_on(window, "Confirm delete stack")
            .expect("the confirm affordance mounts");
        let cancel = support::driver::find_by_label_on(window, "Cancel delete stack")
            .expect("the cancel affordance mounts");
        confirm.invoke_default_action();
        cancel.invoke_default_action();
        settle();
        assert_eq!(window.get_destructive_confirm_count(), 1);
        assert_eq!(window.get_destructive_cancel_count(), 1);
    });
}

/// The non-destructive grammar: message secondary, confirm success
/// (the recur convert strip's exact tones).
#[test]
fn the_convert_strip_paints_the_success_grammar() {
    run_probe(|window| {
        settle();
        assert_eq!(
            window.get_convert_message_color(),
            window.get_theme_text_secondary()
        );
        assert_eq!(
            window.get_convert_confirm_color(),
            window.get_semantic_success()
        );
    });
}
