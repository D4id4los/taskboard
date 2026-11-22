// SPDX-License-Identifier: AGPL-3.0-only

//! The small-gated-bundle journeys (round 4, J4's accepted plan
//! `.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md`,
//! chunk SB1's wiring rows): US-1 (the capture overlay's outside
//! click commits the draft), US-2 (the recur editor's outside click
//! accepts the staged pattern), US-3 (the banner mute control is
//! direction-aware). SB2/PR1/PR2 add their rows to this shared file.

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;

use chrono::TimeZone as _;
use taskboard_domain::StateCommand;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::stroke::parse;

/// A fixed past due date (local midnight, UTC-typed like the
/// domain's stamps): the overdue-banner predicate `duedate < now`
/// holds for every pass `now` — the fixture is anchored, never
/// wall-clock read (the clock discipline's relative-oracle rule).
fn past_due() -> chrono::DateTime<chrono::Utc> {
    chrono::Local
        .with_ymd_and_hms(2020, 1, 1, 0, 0, 0)
        .single()
        .expect("legal local instant")
        .with_timezone(&chrono::Utc)
}

// -- US-1: capture outside-click commits the draft -----------------------

/// US-1/AC-1 (`us1_s1_outside_click_commits_capture`): a non-empty
/// capture draft commits through the accept seam on a backdrop click
/// and the overlay closes (#976's R-4 edit-kind flip).
#[test]
fn us1_s1_outside_click_commits_capture() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_capture_open());

        harness.type_text("backdrop commit");
        // The scrim: the window corner, far from the centered panel
        // (the host's edit-kind resolution routes the accept seam).
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);

        assert!(
            !harness.handle.window().get_capture_open(),
            "the overlay closed on the outside-click commit"
        );
        harness.assert_commands(&[StateCommand::CaptureTask {
            title: "backdrop commit".into(),
            board: None,
        }]);
    });
}

/// US-1/AC-2 (`us1_s2_empty_draft_does_not_commit`): an empty draft's
/// backdrop click closes the overlay and dispatches nothing.
#[test]
fn us1_s2_empty_draft_does_not_commit() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_capture_open());

        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);

        assert!(
            !harness.handle.window().get_capture_open(),
            "the empty draft's outside click closes the overlay"
        );
        harness.assert_commands(&[]);
    });
}

/// US-1/AC-3 (`us1_s3_escape_still_discards`): a non-empty draft
/// discards on Escape — the explicit escape is never flipped.
#[test]
fn us1_s3_escape_still_discards() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_capture_open());

        harness.type_text("escape discards me");
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);

        assert!(
            !harness.handle.window().get_capture_open(),
            "Escape discards the draft and closes"
        );
        harness.assert_commands(&[]);
    });
}

// -- US-2: recur editor outside-click accepts ----------------------------

/// Arms the task-detail view over the fixture's subject task (the
/// recur editor's `SeriesEdit` verb addresses the focused subject —
/// the `label_journey_mount` shape, inlined).
fn mount_task_detail(harness: &mut support::harness::Harness) {
    harness.press(ActionId::NavFocusDown);
    harness.press(ActionId::TaskEdit);
    harness.driver.advance_time(0);
    assert_eq!(
        harness.handle.window().get_current_view(),
        taskboard_ui_slint::View::TaskDetail,
        "the fixture task owns the detail"
    );
}

/// A `Daily { every: 2 }` head rule on the fixture task (the recur
/// editor arms against a recurring task).
fn seed_daily_recur(state: &mut taskboard_domain::AppState, task: taskboard_domain::TaskId) {
    let recur = taskboard_domain::RecurRule {
        series: taskboard_domain::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(42)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 10, 5).expect("fixture date"),
        pattern: taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::EveryDays::new(2).expect("valid interval"),
        },
        bound: taskboard_domain::RecurBound::None,
        continued_from: None,
        paused_until: None,
    };
    state.tasks.get_mut(&task).expect("fixture task").recur = Some(recur);
}

/// US-2/AC-1 (`us2_s1_outside_click_accepts_recur`): a valid staged
/// pattern commits through the commit seam on a backdrop click and
/// the editor closes (the D-3 edit-kind law through the real mount).
#[test]
fn us2_s1_outside_click_accepts_recur() {
    let (fx, mut state) = fixture::golden_state();
    seed_daily_recur(&mut state, fx.task0);
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        mount_task_detail(harness);
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_recur_editor_open());

        // Stage a distinct valid interval: Daily every 5.
        harness
            .handle
            .window()
            .invoke_recur_every_days_edited(slint::SharedString::from("5"));
        harness.driver.advance_time(0);

        // The scrim corner: the host's edit-kind resolution routes
        // the commit seam.
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);

        assert!(
            !harness.handle.window().get_recur_editor_open(),
            "the editor closed on the outside-click accept"
        );
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::UpdateTask { changes, .. }]
                    if changes.recur.is_some()
            ),
            "the recurrence change reached the boundary, got {drained:?}"
        );
    });
}

/// US-2/AC-2 (`us2_s2_invalid_draft_stays_open`): an invalid staged
/// draft's backdrop click dispatches nothing and keeps the editor
/// open (the invalid-draft state is a no-op, never a forced accept).
#[test]
fn us2_s2_invalid_draft_stays_open() {
    let (fx, mut state) = fixture::golden_state();
    seed_daily_recur(&mut state, fx.task0);
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        mount_task_detail(harness);
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_recur_editor_open());

        // Stage an invalid interval: the empty field fails the
        // EveryDays grammar, the commit seam re-checks and dispatches
        // nothing.
        harness
            .handle
            .window()
            .invoke_recur_every_days_edited(slint::SharedString::from(""));
        harness.driver.advance_time(0);

        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);

        assert!(
            harness.handle.window().get_recur_editor_open(),
            "the invalid draft keeps the editor open"
        );
        harness.assert_commands(&[]);
    });
}

/// US-2/S3 (`us2_s3_escape_cancels_staged_draft`): Escape drops the
/// staged draft — the pattern is unchanged, nothing dispatches, the
/// editor closes (the explicit cancel the flip never touches).
#[test]
fn us2_s3_escape_cancels_staged_draft() {
    let (fx, mut state) = fixture::golden_state();
    seed_daily_recur(&mut state, fx.task0);
    support::harness::run_journey(state, move |harness| {
        mount_task_detail(harness);
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_recur_editor_open());

        // Stage a distinct valid interval, then cancel with Escape.
        // The Escape stroke rides the action layer as `ModeNormal`
        // (the Global/ViInsert binding the keymap snapshots pin); the
        // journey drives the routed action because the headless
        // backend drops raw key events when no item holds the mount's
        // focus (the panel-scope grab is a real-focus ritual — a
        // backend artifact, not this row's contract).
        harness
            .handle
            .window()
            .invoke_recur_every_days_edited(slint::SharedString::from("5"));
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ModeNormal);
        harness.driver.advance_time(0);

        assert!(
            !harness.handle.window().get_recur_editor_open(),
            "Escape cancels the editor"
        );
        harness.assert_commands(&[]);
        // The staged draft died with the level: re-arming reads the
        // head rule's own interval, not the cancelled draft.
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_recur_editor_open());
        let draft = harness
            .handle
            .window()
            .get_recur_editor_model()
            .every_days_text
            .to_string();
        assert_eq!(draft, "2", "the staged draft dropped; the head reads back");
    });
}

/// US-3/AC-1 (`us3_s1_banner_unmute_dispatches_false`): a muted
/// task's banner control dispatches the mute-clearing command
/// (engine truth folds the direction; the #733 twin grammar).
#[test]
fn us3_s1_banner_unmute_dispatches_false() {
    let (fx, mut state) = fixture::journey_state();
    let task1 = fx.task1;
    state.tasks.get_mut(&task1).expect("fixture task").duedate = Some(past_due());
    state.reminders.muted.insert(task1);
    support::harness::run_journey(state, move |harness| {
        harness.assert_commands(&[]);
        harness.handle.window().invoke_banner_mute_clicked(0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::SetTaskRemindersMuted {
            task: task1,
            muted: false,
        }]);
    });
}

/// US-3/AC-2 (`us3_s2_banner_mute_preserved`): an unmuted task's
/// banner control still dispatches the muting command (the existing
/// direction preserved).
#[test]
fn us3_s2_banner_mute_preserved() {
    let (fx, mut state) = fixture::journey_state();
    let task1 = fx.task1;
    state.tasks.get_mut(&task1).expect("fixture task").duedate = Some(past_due());
    support::harness::run_journey(state, move |harness| {
        harness.assert_commands(&[]);
        harness.handle.window().invoke_banner_mute_clicked(0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::SetTaskRemindersMuted {
            task: task1,
            muted: true,
        }]);
    });
}
