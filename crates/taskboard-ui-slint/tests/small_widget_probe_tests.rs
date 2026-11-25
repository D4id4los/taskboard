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

/// Dispatches a synthetic left click at the window coordinates (the
/// labeled-value-row probe's recorded dispatch pattern): the mock
/// platform delivers the click on the tick between press and
/// release.
fn click_at(window: &SmallWidgetProbe, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let p = slint::LogicalPosition::new(x, y);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
    advance_probe_clock(1);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    advance_probe_clock(1);
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: p,
            button: PointerEventButton::Left,
        });
    advance_probe_clock(1);
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

// ---- T4: ActionChip -------------------------------------------------

/// The swatch chip (the label-chip grammar): the space-3 echo square
/// rides the ladder's text colour, the trailing remove affordance
/// routes `removed`, and the passive body never invokes.
#[test]
fn the_swatch_chip_renders_the_echo_square_and_routes_removed() {
    run_probe(|window| {
        settle();
        assert_eq!(window.get_chip_swatch_width(), window.get_theme_space_3());
        assert_eq!(
            window.get_chip_text_color(),
            window.get_theme_text_primary()
        );
        let remove = support::driver::find_by_label_on(window, "Remove label probe")
            .expect("the remove affordance mounts");
        remove.invoke_default_action();
        settle();
        assert_eq!(window.get_chip_removed_count(), 1);
        assert_eq!(window.get_chip_invoked_count(), 0);
    });
}

/// The preset chip (the `ActivationStop` grammar): the token floor pin
/// holds and the FIRST physical click invokes (the F8 hit-test law —
/// the touch hit-tests before the scope overlay).
#[test]
fn the_preset_chip_holds_its_floor_pin_and_invokes_on_the_first_click() {
    run_probe(|window| {
        settle();
        assert!(window.get_preset_chip_height() >= window.get_theme_space_5());
        assert!(window.get_preset_chip_width() >= window.get_theme_space_6());
        click_at(
            window,
            window.get_preset_chip_x() + window.get_preset_chip_width() / 2.0,
            window.get_preset_chip_y() + window.get_preset_chip_height() / 2.0,
        );
        settle();
        assert_eq!(window.get_chip_invoked_count(), 1);
    });
}

/// The plain-scope chip (the timers-chip grammar): the first click
/// invokes through the raw scope posture too.
#[test]
fn the_plain_scope_chip_invokes_on_the_first_click() {
    run_probe(|window| {
        settle();
        click_at(
            window,
            window.get_plain_chip_x() + window.get_plain_chip_width() / 2.0,
            window.get_plain_chip_y() + window.get_plain_chip_height() / 2.0,
        );
        settle();
        assert_eq!(window.get_plain_scope_chip_count(), 1);
    });
}

/// The passive chip (the paused-chip posture): no scope, no touch —
/// a click lands on nothing and the invoked counter stays at zero.
#[test]
fn the_passive_chip_never_invokes() {
    run_probe(|window| {
        settle();
        click_at(
            window,
            window.get_passive_chip_x() + window.get_passive_chip_width() / 2.0,
            window.get_passive_chip_y() + window.get_passive_chip_height() / 2.0,
        );
        settle();
        assert_eq!(window.get_chip_invoked_count(), 0);
        assert_eq!(window.get_plain_scope_chip_count(), 0);
    });
}

// ---- T5: SectionHeader ----------------------------------------------

/// The header row's anatomy order: title < count < badge < trailing
/// action; the bare fixture (the habits grammar) hides the count and
/// the badge (the driver finds neither label).
#[test]
fn the_header_row_orders_title_count_badge_actions_and_hides_empties() {
    run_probe(|window| {
        settle();
        let title_x = window.get_header_title_x();
        let count_x = window.get_header_count_x();
        let badge_x = window.get_header_badge_x();
        let actions_x = window.get_header_actions_x();
        assert!(title_x < count_x, "the count follows the title");
        assert!(count_x < badge_x, "the badge follows the count");
        assert!(badge_x < actions_x, "the actions follow the stretch");
        // The empty count/badge slots hide on the bare fixture (the
        // conditional hide's structural read).
        assert!(window.get_full_count_shown());
        assert!(window.get_full_badge_shown());
        assert!(!window.get_bare_count_shown());
        assert!(!window.get_bare_badge_shown());
    });
}

// ---- T6: GhostOpener -------------------------------------------------

/// The ghost fixtures' orientation geometry (§2.6): the row band is
/// the space-6 + 2·space-1 control row; the column sliver is
/// space-6 + space-2 wide and fills its host's height.
#[test]
fn the_ghosts_carry_their_orientation_geometry() {
    run_probe(|window| {
        settle();
        assert_eq!(
            window.get_row_ghost_height(),
            window.get_theme_space_6() + 2.0 * window.get_theme_space_1()
        );
        assert_eq!(
            window.get_column_ghost_width(),
            window.get_theme_space_6() + window.get_theme_space_2()
        );
    });
}

/// The F8 first-click law (R-1): the first physical click on each
/// ghost invokes — the touch hit-tests before the scope overlay.
#[test]
fn the_ghosts_invoke_on_the_first_physical_click() {
    run_probe(|window| {
        settle();
        click_at(
            window,
            window.get_row_ghost_x() + window.get_row_ghost_width() / 2.0,
            window.get_row_ghost_y() + window.get_row_ghost_height() / 2.0,
        );
        click_at(
            window,
            window.get_column_ghost_x() + window.get_column_ghost_width() / 2.0,
            window.get_column_ghost_y() + window.get_column_ghost_height() / 2.0,
        );
        settle();
        assert_eq!(window.get_row_ghost_count(), 1);
        assert_eq!(window.get_column_ghost_count(), 1);
    });
}

/// The mode-selector's icon grammar (#1430, US-12/S3): the habit
/// editor's Nudge day / Period start strip resolves its pushed
/// per-slot icons — calendar-days for the nudge day, alarm-clock for
/// the period start (the issue's Lucide family) — and a slot past the
/// pushed icons still falls to the founding check glyph. The oracle
/// is the typed enum, never a pixel.
#[test]
fn the_mode_strip_resolves_its_pushed_icons() {
    run_probe(|window| {
        assert_eq!(
            window.get_mode_icon_nudge(),
            IconKind::CalendarDays,
            "the Nudge day slot carries the calendar-days glyph"
        );
        assert_eq!(
            window.get_mode_icon_period(),
            IconKind::AlarmClock,
            "the Period start slot carries the alarm-clock glyph"
        );
        assert_eq!(
            window.get_fallback_icon(),
            IconKind::Check,
            "a slot past the pushed icons falls to the check glyph"
        );
    });
}
