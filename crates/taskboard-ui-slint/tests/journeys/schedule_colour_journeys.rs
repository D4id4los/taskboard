// SPDX-License-Identifier: AGPL-3.0-only

//! The schedule pair-write + colour UX journeys (issues #1428, #1440,
//! #1439; journey plan `.artifacts/plans/2026-11-25-sound-today-
//! colour-journeys-schedule-colour-ux-plan.md`) — the plan §8's
//! step-ID-named evidence file.
//!
//! Assertion discipline: typed outcomes only — the exact sparse
//! `UpdateTask`/`CreateLabel`/`SetBoardColor` masks at the fake-engine
//! boundary and the pushed picker projection properties. Colour
//! values are asserted as hex strings through the presentation
//! conversion the mint owns; no label or status text is ever matched.
//! Keyboard discipline note (US-28/S4): the picker's arrows re-emit
//! through the same `hue-picked`/`sat-val-picked` seams the drags
//! ride (the widget holds no separate keyboard path — D-4), so the
//! nudge legs drive those seams at the arrow steps; headless tab-focus
//! simulation into the widget's `FocusScope`s is not drivable here.

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::Model as _;
use taskboard_domain::StateCommand;
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;

/// The default day law's pair over the harness's hand-over (the
/// default day start + boot offset, single-read capture — the
/// clock-discipline law).
fn default_day_pair(
    day: chrono::NaiveDate,
) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let day_start = taskboard_domain::SettingsSnapshot::default().day_start_time;
    let offset = UiSettings::default().boot_offset;
    (
        taskboard_domain::days::day_first_instant(day, day_start, offset),
        taskboard_domain::days::day_last_instant(day, day_start, offset),
    )
}

/// One `UpdateTask` pair mask matcher: `scheduled` = first,
/// `duedate` = last, everything else untouched (#1428's D-1 shape).
fn assert_pair_mask(
    changes: &taskboard_domain::TaskChanges,
    first: chrono::DateTime<chrono::Utc>,
    last: chrono::DateTime<chrono::Utc>,
    context: &str,
) {
    assert_eq!(
        changes.scheduled,
        Some(Some(first)),
        "{context}: start stamp"
    );
    assert_eq!(changes.duedate, Some(Some(last)), "{context}: due stamp");
}

/// The labels-overlay create-form preamble (the tag-color journeys'
/// shape): the board roster overlay + one open create form seeded
/// with the palette's first base.
fn open_label_form(harness: &mut Harness) {
    let window = harness.handle.window();
    window.invoke_labels_manage_asked();
    harness.driver.advance_time(0);
    window.invoke_labels_overlay_add_requested();
    harness.driver.advance_time(0);
}

// ---- US-27 — the schedule quick verbs set start+due -----------------

/// US-27/S1 (AC-1) — the context menu's "Schedule today" routes the
/// exact PAIR mask: `scheduled` = today's first covered instant,
/// `duedate` = today's last covered instant (#893), one sparse mask,
/// nothing else set.
#[test]
fn us27_s1_schedule_today_writes_start_and_due() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        window.invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);

        let today = chrono::Utc::now()
            .with_timezone(&UiSettings::default().boot_offset)
            .date_naive();
        let (first, last) = default_day_pair(today);

        harness.driver.click_by_label("Schedule today");
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes }] => {
                assert_eq!(*id, fixture.task1, "the focused subject");
                assert_pair_mask(changes, first, last, "today");
            }
            other => panic!("the today verb routes one pair-write, got {other:?}"),
        }
    });
}

/// US-27/S2 (AC-2, D-2) — "Schedule tomorrow" routes the reconciled
/// pair: the start is tomorrow's first covered instant and the due is
/// tomorrow's LAST covered instant — the raw exclusive window end is
/// retired (the end is excluded by the day's own `[start, end)`
/// membership).
#[test]
fn us27_s2_schedule_tomorrow_reconciles_to_covered_instant() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        window.invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);

        let now = chrono::Utc::now();
        let day_start = taskboard_domain::SettingsSnapshot::default().day_start_time;
        let offset = UiSettings::default().boot_offset;
        let (tomorrow_first, tomorrow_end) =
            taskboard_domain::days::next_day_window(now, day_start, offset);
        let tomorrow_last = taskboard_domain::days::last_covered_instant(tomorrow_end);
        // D-2's reconciliation oracle: the raw exclusive end must NOT
        // be the commit — the due stamp is its covered predecessor.
        assert!(
            tomorrow_last < tomorrow_end,
            "the covered instant is strictly below the raw exclusive end"
        );

        harness.driver.click_by_label("Schedule tomorrow");
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes }] => {
                assert_eq!(*id, fixture.task1, "the focused subject");
                assert_pair_mask(changes, tomorrow_first, tomorrow_last, "tomorrow");
            }
            other => panic!("the tomorrow verb routes one pair-write, got {other:?}"),
        }
    });
}

/// US-27 (AC-3) — the day-start setting is honoured: with
/// `day_start_time` = 05:30 the today verb stamps 05:30 local as the
/// start (the window's opening instant), never 00:00.
#[test]
fn us27_s3_day_start_time_is_honoured() {
    let (fixture, mut state) = fixture::journey_state();
    state.settings.day_start_time = chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("05:30");
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        window.invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);

        let offset = UiSettings::default().boot_offset;
        let today = chrono::Utc::now().with_timezone(&offset).date_naive();
        let (first, last) = {
            let day_start = chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("05:30");
            (
                taskboard_domain::days::day_first_instant(today, day_start, offset),
                taskboard_domain::days::day_last_instant(today, day_start, offset),
            )
        };
        // AC-3's face: the start stamps 05:30 local on the named day.
        let local = first.with_timezone(&offset);
        assert_eq!(
            local.time(),
            chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("05:30")
        );
        assert_eq!(local.date_naive(), today);

        harness.driver.click_by_label("Schedule today");
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes }] => {
                assert_eq!(*id, fixture.task1, "the focused subject");
                assert_pair_mask(changes, first, last, "05:30 day start");
            }
            other => panic!("one pair-write expected, got {other:?}"),
        }
    });
}

/// US-27/S3 (AC-4) — the multi-selection quick bar fires the
/// per-subject pair-write: every selected member receives the same
/// day pair, one sparse `UpdateTask` each.
#[test]
fn us27_s4_multi_selection_writes_every_subject() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        // Focus the column's first card via its context menu (the
        // click ≡ key grammar), close the menu, extend the selection
        // to the next card, fire the quick verb.
        window.invoke_board_task_context_requested(0, 0, 300.0, 300.0);
        harness.driver.advance_time(0);
        harness.stroke(taskboard_ui_slint::keymap::stroke::parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.press(ActionId::NavExtendNext);
        harness.driver.advance_time(0);
        harness.press(ActionId::TaskScheduleToday);
        harness.driver.advance_time(0);

        let today = chrono::Utc::now()
            .with_timezone(&UiSettings::default().boot_offset)
            .date_naive();
        let (first, last) = default_day_pair(today);

        let drained = harness.engine.drain();
        assert!(
            drained.len() >= 2,
            "every selected member receives its pair-write, got {drained:?}"
        );
        let mut subjects = std::collections::BTreeSet::new();
        for command in &drained {
            match command {
                StateCommand::UpdateTask { id, changes } => {
                    subjects.insert(*id);
                    assert_pair_mask(changes, first, last, "selection member");
                }
                other => panic!("only pair-writes dispatch, got {other:?}"),
            }
        }
        assert_eq!(
            subjects,
            std::collections::BTreeSet::from([fixture.task0, fixture.task1]),
            "both selection members were written"
        );
    });
}

/// US-27 (AC-5) — the typed `NoTarget` stale-absorb posture: a
/// subjectless dispatch writes nothing (no partial write, no
/// command).
#[test]
fn us27_s5_subjectless_dispatch_is_typed_notarget() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // The mount's board holds no card focus; the verb routes the
        // typed refusal and the boundary stays empty.
        harness.press(ActionId::TaskScheduleToday);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        harness.press(ActionId::TaskScheduleTomorrow);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
    });
}

// ---- US-28 — the picker ↔ HEX field two-way link --------------------

/// US-28/S1 (AC-1) — a picker gesture rewrites the draft to the
/// projected hex (the hue seam; the SV seam rides the identical path):
/// the untouched axes ride the last-valid draft's own reading.
#[test]
fn us28_s1_picker_drag_writes_the_draft_hex() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness: &mut Harness| {
        open_label_form(harness);
        let window = harness.handle.window();
        // The seeded form's draft is the palette's first base.
        let seed = window.get_label_overlay_form_colour().to_string();
        let (_, sat, val) =
            taskboard_ui_slint::presentation::hex_to_hsv(&seed).expect("a minted seed");
        window.invoke_labels_overlay_hue_picked(210.0);
        harness.driver.advance_time(0);
        let projected = taskboard_ui_slint::presentation::hsv_to_hex(210.0, sat, val);
        assert_eq!(
            window.get_label_overlay_form_colour().to_string(),
            projected,
            "the picker's hue rewrite is the draft's single truth"
        );

        // The SV seam: a sat/val gesture keeps the projected hue.
        window.invoke_labels_overlay_sat_val_picked(0.5, 0.8);
        harness.driver.advance_time(0);
        let (hue, _, _) = taskboard_ui_slint::presentation::hex_to_hsv(&projected)
            .expect("the projected draft mints");
        assert_eq!(
            window.get_label_overlay_form_colour().to_string(),
            taskboard_ui_slint::presentation::hsv_to_hex(hue, 0.5, 0.8),
            "the SV gesture keeps the hue axis"
        );
    });
}

/// US-28/S2 (AC-2) — a valid typed hex re-projects the picker: hue /
/// sat / val carry the typed colour's HSV reading and the SV square's
/// hue fill follows.
#[test]
fn us28_s2_valid_hex_reprojects_the_picker() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness: &mut Harness| {
        open_label_form(harness);
        let window = harness.handle.window();
        let typed = "3a70d6";
        window.invoke_labels_overlay_colour_edited(slint::SharedString::from(typed));
        harness.driver.advance_time(0);

        let (h, s, v) =
            taskboard_ui_slint::presentation::hex_to_hsv(typed).expect("minted typed body");
        assert!(
            (f64::from(window.get_label_overlay_picker_hue()) - h).abs() < 1e-3,
            "the picker's hue re-projects"
        );
        assert!(
            (f64::from(window.get_label_overlay_picker_sat()) - s).abs() < 1e-3,
            "the picker's sat re-projects"
        );
        assert!(
            (f64::from(window.get_label_overlay_picker_val()) - v).abs() < 1e-3,
            "the picker's val re-projects"
        );
        // The SV square's base fill: the hue's pure colour (s = v = 1).
        let pure = taskboard_ui_slint::presentation::hsv_to_hex(h, 1.0, 1.0);
        let expected = parse_hex_color(&pure);
        assert_eq!(
            window.get_label_overlay_picker_hue_colour(),
            expected,
            "the hue fill is the pure hue colour"
        );
    });
}

/// US-28/S2 (AC-3) — an invalid intermediate hex never clobbers the
/// picker: the projection holds the last valid draft's reading while
/// the field's validity echo turns invalid.
#[test]
fn us28_s3_invalid_hex_holds_the_picker_projection() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness: &mut Harness| {
        open_label_form(harness);
        let window = harness.handle.window();
        window.invoke_labels_overlay_colour_edited(slint::SharedString::from("3a70d6"));
        harness.driver.advance_time(0);
        let hue = window.get_label_overlay_picker_hue();
        let sat = window.get_label_overlay_picker_sat();
        let val = window.get_label_overlay_picker_val();

        window.invoke_labels_overlay_colour_edited(slint::SharedString::from("zz"));
        harness.driver.advance_time(0);
        assert!(!window.get_label_overlay_colour_valid(), "the invalid echo");
        assert_eq!(window.get_label_overlay_picker_hue(), hue, "hue holds");
        assert_eq!(window.get_label_overlay_picker_sat(), sat, "sat holds");
        assert_eq!(window.get_label_overlay_picker_val(), val, "val holds");
    });
}

/// US-28/S3 — the overlay commit persists the pick: the projected
/// draft funnels through the pre-existing commit command unchanged.
#[test]
fn us28_s3_overlay_commit_persists_the_pick() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness: &mut Harness| {
        open_label_form(harness);
        let window = harness.handle.window();
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("picked"));
        window.invoke_labels_overlay_hue_picked(120.0);
        harness.driver.advance_time(0);
        let seed = window.get_label_overlay_form_colour().to_string();
        let (_, sat, val) =
            taskboard_ui_slint::presentation::hex_to_hsv(&seed).expect("minted draft");
        let projected = taskboard_ui_slint::presentation::hsv_to_hex(120.0, sat, val);

        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                &drained[..],
                [StateCommand::CreateLabel { color, .. }]
                    if color.as_str() == projected
            ),
            "the commit persists the projected draft, got {drained:?} (wanted {projected})"
        );
    });
}

/// US-28/S4 (AC-5) — the keyboard nudges reach any colour: the
/// arrows ride the same seams (D-4), so a strip-wrap nudge sequence
/// (359° → +3° wraps to 2°) and clamped square nudges each rewrite
/// the draft exactly — every reachable-by-drag colour is reachable
/// by the nudge path.
#[test]
fn us28_s4_keyboard_nudges_reach_any_colour() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness: &mut Harness| {
        open_label_form(harness);
        let window = harness.handle.window();

        // The strip's wrap nudge: seed near the top of the dial, then
        // nudge past the seam.
        window.invoke_labels_overlay_hue_picked(359.0);
        harness.driver.advance_time(0);
        let seed = window.get_label_overlay_form_colour().to_string();
        let (_, sat, val) =
            taskboard_ui_slint::presentation::hex_to_hsv(&seed).expect("minted draft");
        window.invoke_labels_overlay_hue_picked((359.0 + 3.0) % 360.0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_form_colour().to_string(),
            taskboard_ui_slint::presentation::hsv_to_hex(2.0, sat, val),
            "the wrapped nudge lands on the projected colour"
        );

        // The square's clamped nudges: the corners are reachable.
        window.invoke_labels_overlay_sat_val_picked(0.0, 1.0);
        harness.driver.advance_time(0);
        let seed = window.get_label_overlay_form_colour().to_string();
        let (hue, _, _) =
            taskboard_ui_slint::presentation::hex_to_hsv(&seed).expect("minted draft");
        window.invoke_labels_overlay_sat_val_picked(1.0, 0.0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_form_colour().to_string(),
            taskboard_ui_slint::presentation::hsv_to_hex(hue, 1.0, 0.0),
            "the opposite square corner is reachable by nudges"
        );
    });
}

// ---- US-29 — the tone-ladder palette --------------------------------

/// US-29/S1 (AC-1) — the swatch row renders the derived 9×3 palette
/// (wrapped grid model) and a tone click re-seeds the draft to that
/// tone's exact value.
#[test]
fn us29_s1_swatch_row_shows_the_tone_ladder_grid() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness: &mut Harness| {
        open_label_form(harness);
        let window = harness.handle.window();
        let derived = taskboard_ui_slint::presentation::derived_swatches();
        assert_eq!(derived.len(), 27, "nine bases × three tones");
        assert_eq!(
            window.get_label_overlay_swatches().row_count(),
            27,
            "the row mounts the derived grid"
        );
        // The blue base's darker edge: the click re-seeds the exact
        // tone (the pre-existing affordance, wider palette).
        window.invoke_labels_overlay_swatch_clicked(3);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_form_colour().to_string(),
            derived[3],
            "the draft carries the clicked tone's exact hex"
        );
    });
}

/// US-29/S2 (AC-3, D-6) — the board colour strip presents the same
/// derived palette and a tone click commits the exact `SetBoardColor`.
#[test]
fn us29_s2_board_strip_presents_the_same_palette() {
    let (_fixture, state) = fixture::journey_state();
    let board = state.boards.values().next().expect("fixture board").id;
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::BoardColorEdit);
        let window = harness.handle.window();
        let derived = taskboard_ui_slint::presentation::derived_swatches();
        assert_eq!(
            window.get_board_color_swatches().row_count(),
            derived.len(),
            "the strip mounts the identical palette"
        );
        window.invoke_board_color_picked(6);
        let orange_dark = derived[6].clone();
        harness.assert_commands(&[StateCommand::SetBoardColor {
            board,
            color: taskboard_domain::Color::new(&orange_dark),
        }]);
    });
}

/// Parses a minted 6-hex body into a `slint::Color` (the test-side
/// mirror of the mint's render half).
fn parse_hex_color(hex: &str) -> slint::Color {
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("a minted body's byte");
    slint::Color::from_rgb_u8(byte(0), byte(2), byte(4))
}
