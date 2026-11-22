// SPDX-License-Identifier: AGPL-3.0-only

//! The task-detail widget-inventory journeys (#1205, round 5 chunk
//! A2 — the chunk plan §4 table is the test surface): every form
//! widget one row, each row both an Enter-attribution leg (Return
//! with the widget focused activates the widget's own verb — click ≡
//! key) and an overlay-restore leg (dismissal lands slint focus back
//! on the opener's own cell, never the title collapse). Assertion
//! discipline is the journeys suite's: typed model facts + the
//! focused element's accessible address + the `FocusFacts` tuple;
//! never text oracles.

use crate::support;

use support::driver::JourneyDriver as _;
use support::focus_journey::{JourneyHarness, golden_state_journey};

use slint::ComponentHandle as _;
use taskboard_domain::{Importance, Urgency};
use taskboard_ui_slint::FocusFacts;
use taskboard_ui_slint::View;
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::Preset;
use taskboard_ui_slint::keymap::stroke::parse;

/// The `FocusFacts.anchor-kind` read (the restore leg's typed tuple
/// half — 4 title / 5 description / 6 effort / 7+ the A2 cells).
fn anchor_kind(harness: &JourneyHarness) -> i32 {
    harness
        .handle
        .window()
        .global::<FocusFacts>()
        .get_anchor_kind()
}

/// The task-detail model's `edit-open` flag (the title-editor flow's
/// observable — the stolen-Enter rows' red signature is exactly this
/// flip).
fn title_edit_open(harness: &JourneyHarness) -> bool {
    harness.handle.window().get_task_model().edit_open
}

/// A detail open over the fixture's first board card, through the
/// preset's own rows (the shared directional-journey preamble).
fn open_detail_first_card(harness: &mut JourneyHarness, preset: Preset) {
    harness.press_preset(preset, ActionId::NavFocusDown);
    harness.press_preset(preset, ActionId::TaskEdit);
    harness.advance();
    assert_eq!(
        harness.handle.window().get_current_view(),
        View::TaskDetail,
        "the detail is open"
    );
}

/// Walks the control ladder until the addressed element holds slint
/// focus (labels are element addresses, never assertion text). A
/// ladder step onto a text field arms insert mode; the walk exits it
/// (the mode flip only — focus stays) and keeps stepping. The guard
/// bounds the ladder; a miss is a structural finding, not a text
/// mismatch.
fn walk_to(harness: &mut JourneyHarness, preset: Preset, address: &str) {
    let mut guard = 0;
    while harness.driver.focused_label().as_deref() != Some(address) {
        assert!(
            guard < 60,
            "the walk never landed on {address:?} (at {:?})",
            harness.driver.focused_label()
        );
        if harness.chain().first() == Some(&ContextId::ViInsert) {
            harness
                .driver
                .press_stroke(parse("Escape").expect("Escape parses"));
            harness.advance();
        }
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        guard += 1;
    }
}

/// The enriched detail fixture: the first card carries a due date, a
/// scheduled date, both priority levels — every metadata cell mounts.
fn detail_fixture_state() -> taskboard_domain::AppState {
    let (_fixture, mut state) = support::fixture::golden_state();
    let mut task0 = state
        .tasks
        .values()
        .find(|t| t.title == "task-0")
        .expect("fixture task-0")
        .clone();
    task0.duedate = Some(support::fixture::PINNED_NOW);
    task0.scheduled = Some(support::fixture::PINNED_NOW);
    task0.importance = Some(Importance::Normal);
    task0.urgency = Some(Urgency::Low);
    let id = task0.id;
    state.tasks.insert(id, task0);
    state
}

/// Row 1 (the plan §4 table) — the title display cell: Return arms
/// the title editor (the context row's legitimate remaining role:
/// no activation claimant, the row fires). Green pre-fix by
/// construction; the pin keeps it green through the attribution
/// fix.
#[test]
fn row1_title_display_return_still_arms_the_title() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Edit title");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            title_edit_open(&harness),
            "the title cell's Return arms the title editor"
        );
    });
}

/// Row 3 — the due date cell: Return opens the due editor, never the
/// title.
#[test]
fn row3_due_cell_return_opens_the_due_editor() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Edit due date");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the due cell's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_task_model().due_open,
            "the due cell's Return opens the due editor"
        );
    });
}

/// Row 4 — the start date cell: Return opens the scheduled editor.
#[test]
fn row4_scheduled_cell_return_opens_the_scheduled_editor() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Edit start date");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the start cell's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_task_model().scheduled_open,
            "the start cell's Return opens the scheduled editor"
        );
    });
}

/// Row 5 — the start-tracking button: Return dispatches the same
/// command the click does (click ≡ key); the title never arms.
#[test]
fn row5_start_tracking_return_dispatches_the_tracking_verb() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Start tracking");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the tracking button's Return is not stolen by the title"
        );
        let drained = harness.engine.drain();
        assert!(
            drained
                .iter()
                .any(|c| matches!(c, taskboard_domain::StateCommand::StartTracking { .. })),
            "Return on the tracking button dispatches the tracking verb, got {drained:?}"
        );
    });
}

/// Row 7 — the importance cell: Return opens the priority picker.
#[test]
fn row7_importance_cell_return_opens_the_picker() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Importance");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the importance cell's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_task_model().importance_open,
            "the importance cell's Return opens the picker"
        );
    });
}

/// Row 8 — the urgency cell: Return opens the priority picker.
#[test]
fn row8_urgency_cell_return_opens_the_picker() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Urgency");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the urgency cell's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_task_model().urgency_open,
            "the urgency cell's Return opens the picker"
        );
    });
}

/// Row 9 — the add-label affordance: Return opens the labels overlay.
#[test]
fn row9_add_label_return_opens_the_labels_overlay() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Add label");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the add-label affordance's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_task_model().labels_open,
            "the add-label affordance's Return opens the labels overlay"
        );
    });
}

/// Row 11 — the time-entries opener: Return opens the entries overlay.
#[test]
fn row11_track_entries_opener_return_opens_the_entries_overlay() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Time entries");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the entries opener's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_task_model().track_entries_open,
            "the entries opener's Return opens the entries overlay"
        );
    });
}

/// Row 14 — the description display cell: Return opens the
/// description editor (the reservation's existing letter — stays).
#[test]
fn row14_description_display_return_opens_the_description_editor() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(&mut harness, preset, "Edit description");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the description cell's Return is not stolen by the title"
        );
        assert!(
            harness
                .handle
                .window()
                .get_task_model()
                .description_edit_open,
            "the description cell's Return opens the description editor"
        );
    });
}

/// The due cell's restore leg: dismissing the due editor lands slint
/// focus back on the due cell — not the title collapse (kind 4).
#[test]
fn restore_due_editor_lands_on_the_due_cell() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.driver.click_by_label("Edit due date");
        harness.advance();
        assert!(
            harness.handle.window().get_task_model().due_open,
            "the due editor opened through the click seam"
        );
        for _ in 0..4 {
            if !harness.handle.window().get_task_model().due_open {
                break;
            }
            harness
                .driver
                .press_stroke(parse("Escape").expect("Escape parses"));
            harness.advance();
        }
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "Escape dismissed the overlay"
        );
        assert!(
            !title_edit_open(&harness),
            "the dismissal never opens the title editor"
        );
        assert_eq!(
            anchor_kind(&harness),
            7,
            "the due restore publishes the due cell's anchor kind"
        );
        assert_eq!(
            harness.driver.focused_label().as_deref(),
            Some("Edit due date"),
            "slint focus lands back on the due cell"
        );
    });
}

/// The priority picker's restore leg: the importance cell.
#[test]
fn restore_importance_picker_lands_on_the_importance_cell() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        // The caption and the dropdown field share the accessible
        // address; the click lands on the button-class element (the
        // field's a11y default action — the open seam).
        let field = harness
            .driver
            .find_all_by_label("Importance")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("the importance dropdown field is an a11y button");
        field.invoke_default_action();
        harness.advance();
        assert!(
            harness.handle.window().get_task_model().importance_open,
            "the picker opened through the click seam"
        );
        for _ in 0..4 {
            if !harness.handle.window().get_task_model().importance_open {
                break;
            }
            harness
                .driver
                .press_stroke(parse("Escape").expect("Escape parses"));
            harness.advance();
        }
        assert!(
            !harness.handle.window().get_task_model().importance_open,
            "Escape dismissed the overlay"
        );
        assert_eq!(
            anchor_kind(&harness),
            9,
            "the importance restore publishes the importance cell's anchor kind"
        );
        assert_eq!(
            harness.driver.focused_label().as_deref(),
            Some("Importance"),
            "slint focus lands back on the importance cell"
        );
    });
}

/// The labels overlay's restore leg: the add-label affordance.
#[test]
fn restore_labels_overlay_lands_on_the_add_label_cell() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.driver.click_by_label("Add label");
        harness.advance();
        assert!(
            harness.handle.window().get_task_model().labels_open,
            "the overlay opened through the click seam"
        );
        for _ in 0..4 {
            if !harness.handle.window().get_task_model().labels_open {
                break;
            }
            harness
                .driver
                .press_stroke(parse("Escape").expect("Escape parses"));
            harness.advance();
        }
        assert!(
            !harness.handle.window().get_task_model().labels_open,
            "Escape dismissed the overlay"
        );
        assert_eq!(
            anchor_kind(&harness),
            11,
            "the labels restore publishes the add-label cell's anchor kind"
        );
        assert_eq!(
            harness.driver.focused_label().as_deref(),
            Some("Add label"),
            "slint focus lands back on the add-label affordance"
        );
    });
}

/// The track-entries overlay's restore leg: the entries opener.
#[test]
fn restore_track_entries_lands_on_the_entries_opener() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.driver.click_by_label("Time entries");
        harness.advance();
        assert!(
            harness.handle.window().get_task_model().track_entries_open,
            "the overlay opened through the click seam"
        );
        for _ in 0..4 {
            if !harness.handle.window().get_task_model().track_entries_open {
                break;
            }
            harness
                .driver
                .press_stroke(parse("Escape").expect("Escape parses"));
            harness.advance();
        }
        assert!(
            !harness.handle.window().get_task_model().track_entries_open,
            "Escape dismissed the overlay"
        );
        assert_eq!(
            anchor_kind(&harness),
            13,
            "the entries restore publishes the opener cell's anchor kind"
        );
        assert_eq!(
            harness.driver.focused_label().as_deref(),
            Some("Time entries"),
            "slint focus lands back on the entries opener"
        );
    });
}

/// The `enter_activates` shape's remaining table rows, folded one
/// leg per overlay family (the add-repeat affordance): Return opens
/// the recur editor.
#[test]
fn add_repeat_return_opens_the_recur_editor() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        walk_to(
            &mut harness,
            preset,
            "Add repeat: the task joins a recurring series",
        );
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the add-repeat affordance's Return is not stolen by the title"
        );
        assert!(
            harness.handle.window().get_recur_editor_open(),
            "the add-repeat affordance's Return opens the recur editor"
        );
    });
}

/// The `enter_activates` shape's stop-tracking row (the running
/// subject, task-1): Return stops the timer, never arms the title.
#[test]
fn stop_tracking_return_dispatches_the_stop_verb() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        // The detail over task-1 (the fixture's tracked subject): two
        // walk steps onto the second card, then the edit verb.
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.press_preset(preset, ActionId::TaskEdit);
        harness.advance();
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the detail is open"
        );
        walk_to(&mut harness, preset, "Stop tracking");
        harness
            .driver
            .press_stroke(parse("Return").expect("Return parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the stop button's Return is not stolen by the title"
        );
        let drained = harness.engine.drain();
        assert!(
            drained
                .iter()
                .any(|c| matches!(c, taskboard_domain::StateCommand::StopTracking)),
            "Return on the stop button dispatches the stop verb, got {drained:?}"
        );
    });
}

/// The fallback leg (the plan §4's vanished-anchor run, due family):
/// the subject dies under the open due editor; the dismissal falls
/// to the view scope — no cell claim fires, and the title editor
/// never arms. Structural outcome only: the dismissal is absorbed
/// (typed view read), never a dead keyboard.
#[test]
fn vanished_subject_restore_falls_to_the_view_scope() {
    golden_state_journey(|_state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(detail_fixture_state(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.driver.click_by_label("Edit due date");
        harness.advance();
        assert!(
            harness.handle.window().get_task_model().due_open,
            "the due editor opened through the click seam"
        );
        // The subject dies under the overlay (the fake engine's state
        // swap — the delete's terminal state, one pass).
        let mut dead = detail_fixture_state();
        dead.tasks.retain(|_, t| t.title != "task-0");
        harness.engine.replace_state(dead, &harness.handle);
        harness.advance();
        // The dismissal after the death: absorbed at the view scope —
        // the title editor never arms (the theft class), the view
        // stays a live, addressed surface.
        harness
            .driver
            .press_stroke(parse("Escape").expect("Escape parses"));
        harness.advance();
        assert!(
            !title_edit_open(&harness),
            "the vanished anchor's dismissal never arms the title"
        );
        assert!(
            harness.driver.focused_label().is_some() || anchor_kind(&harness) == 0,
            "the vanished anchor's restore lands on a live element or the view scope"
        );
    });
}
