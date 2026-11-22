// SPDX-License-Identifier: AGPL-3.0-only

//! The small-gated-bundle journeys (round 4, J4's accepted plan
//! `.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md`,
//! chunk SB1's wiring rows): US-1 (the capture overlay's outside
//! click commits the draft), US-2 (the recur editor's outside click
//! accepts the staged pattern), US-3 (the banner mute control is
//! direction-aware). SB2 adds US-4 (the read-only `ConfigReload`
//! lane),
//! US-5 (the Files row's `poll_secs` field), and US-6 (the degraded-boot
//! badge).

use crate::support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use support::fixture;
use support::harness::Harness;

use chrono::TimeZone as _;
use slint::{ComponentHandle as _, Model as _};
use taskboard_domain::{AppState, ConfigReloadOutcome, StateCommand, SystemEvent};
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::stroke::parse;
use taskboard_ui_slint::requests::{ApplySemantics, SettingsSnapshot, UiRequest};
use taskboard_ui_slint::{ConfigReloadLaneState, FieldValidity, SettingsModel, SettingsOutcome};

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

/// The merged binary's one-platform mode (the suite's invariant).
pub(crate) const HEADLESS_RENDERER: bool = true;

fn run(body: impl FnOnce(AppState) + Send + 'static) {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: HEADLESS_RENDERER,
        },
        move |_driver| body(state),
    );
}

fn harness_with(state: AppState, snapshot: SettingsSnapshot) -> Harness {
    let settings = taskboard_ui_slint::UiSettings {
        general: snapshot,
        ..taskboard_ui_slint::UiSettings::default()
    };
    Harness::mount_with_settings(SlintDriver::default(), state, settings)
}

fn plain_harness(state: AppState) -> Harness {
    Harness::mount_with_settings(
        SlintDriver::default(),
        state,
        taskboard_ui_slint::UiSettings::default(),
    )
}

/// The empty listing reply (the mount auto-lists' happy path).
fn no_rows() -> Vec<taskboard_ui_slint::requests::RemoteBoardRow> {
    Vec::new()
}

/// Answers the mount auto-lists so the request lane is clear for the
/// test's own requests (the general-editor journeys' helper shape).
fn settle_auto_lists(harness: &Harness) {
    for request in harness.drain_requests() {
        let UiRequest::ListRemoteBoards { reply, .. } = request else {
            panic!("the mount routed only auto-lists");
        };
        reply.send(Ok(no_rows())).expect("the reply lane is alive");
    }
    harness.handle.drain_auto_listings();
}

/// The lane fixture: typed values, the poll key's `Live` verdict (the
/// registry's own).
fn snapshot_with_poll(poll_secs: Option<u64>) -> SettingsSnapshot {
    let mut applies = std::collections::BTreeMap::new();
    applies.insert("files_sync.poll_secs".to_owned(), ApplySemantics::live());
    SettingsSnapshot {
        day_start_time: chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("fixture time"),
        reminders_enabled: true,
        reminder_tick_secs: 30,
        reminder_before: "5m".to_owned(),
        reminder_sink: taskboard_ui_slint::requests::ReminderSink::Auto,
        poll_interval_secs: 30,
        backoff_initial_secs: 5,
        backoff_max_secs: 300,
        due_soon_days: 1,
        focus_enabled: false,
        focus_work_mins: 25,
        focus_break_mins: 5,
        focus_chain: false,
        focus_long_break_mins: 15,
        focus_long_break_every: 4,
        focus_notify: false,
        files_sync_enabled: false,
        files_sync_poll_secs: poll_secs,
        files_sync_anchor: None,
        env_overridden: std::collections::BTreeSet::new(),
        facts: Vec::new(),
        applies,
    }
}

// -- US-4: the ConfigReload read-only lane (W-5, R-3) -----------------

/// US-4/S1 (AC-1): an applied reload outcome folds the lane to the
/// applied state — the typed presentation enum, never prose.
#[test]
fn us4_s1_reload_outcome_folds_applied() {
    run(|state| {
        let harness = plain_harness(state);
        harness
            .handle
            .observe_system_event(&SystemEvent::ConfigReload(ConfigReloadOutcome::Applied));
        let window = harness.handle.window();
        let model = window.global::<SettingsModel>();
        assert_eq!(
            model.get_reload_lane_state(),
            ConfigReloadLaneState::Applied,
            "the applied outcome folds to the applied lane state"
        );
    });
}

/// US-4/S2 (AC-2): a refused outcome folds to the kept-last-known-good
/// state — the kiosk operator's "why did my edit not take effect"
/// answer, as a typed fold.
#[test]
fn us4_s2_refused_folds_kept() {
    run(|state| {
        let harness = plain_harness(state);
        // Both refusal classes fold identically (the plan's rows name
        // the kept state, not the decode stage).
        harness
            .handle
            .observe_system_event(&SystemEvent::ConfigReload(
                ConfigReloadOutcome::RefusedDecode,
            ));
        let window = harness.handle.window();
        let model = window.global::<SettingsModel>();
        assert_eq!(
            model.get_reload_lane_state(),
            ConfigReloadLaneState::Kept,
            "a decode refusal folds to kept-last-known-good"
        );
        harness
            .handle
            .observe_system_event(&SystemEvent::ConfigReload(
                ConfigReloadOutcome::RefusedValidate,
            ));
        assert_eq!(
            model.get_reload_lane_state(),
            ConfigReloadLaneState::Kept,
            "a validate refusal folds to kept-last-known-good"
        );
    });
}

/// US-4 AC-3 (R-3, structural): the lane consumes the broadcast and
/// dispatches nothing — no command reaches the engine and no request
/// reaches the app lane, from either outcome class.
#[test]
fn us4_s3_lane_installs_no_command_callbacks() {
    run(|state| {
        let mut harness = plain_harness(state);
        harness
            .handle
            .observe_system_event(&SystemEvent::ConfigReload(ConfigReloadOutcome::Applied));
        harness
            .handle
            .observe_system_event(&SystemEvent::ConfigReload(
                ConfigReloadOutcome::RefusedValidate,
            ));
        harness.assert_commands(&[]);
        assert!(
            harness.drain_requests().is_empty(),
            "the read-only lane dispatches no app request"
        );
    });
}

// -- US-5: the Files row's poll_secs field (W-6, R-4) -----------------

/// US-5/S1 (AC-1's UI seam): committing the field routes exactly one
/// typed `SetFilesSyncPollSecs` intent with the parsed payload — the
/// same staged-edit grammar every general field rides; the
/// whole-snapshot `UpdateSettings` leg is pinned app-side
/// (`us5_s1_poll_secs_rides_update_settings` in taskboard-app).
#[test]
fn us5_s1_poll_secs_rides_update_settings() {
    run(|state| {
        let mut harness = harness_with(state, snapshot_with_poll(Some(60)));
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&harness);
        let window = harness.handle.window();
        let model = window.global::<SettingsModel>();

        // The echo renders the committed snapshot value.
        assert_eq!(model.get_files_poll_echo(), "60");
        // Stage per keystroke, commit on Return's seam.
        window.invoke_settings_field_edited(16, "90".into());
        assert_eq!(model.get_files_poll_echo(), "90", "the draft is the echo");
        window.invoke_files_poll_secs_committed(16, "90".into());
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one typed intent");
        let UiRequest::SetFilesSyncPollSecs { secs, reply } = requests.remove(0) else {
            panic!("the poll intent routed");
        };
        assert_eq!(secs, 90, "the minted payload rides the lane");
        // The reply echoes the fresh file value; the echo follows the
        // reply, never the click.
        reply
            .send(Ok(snapshot_with_poll(Some(90))))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(model.get_files_poll_echo(), "90");
        assert_eq!(model.get_last_outcome(), SettingsOutcome::Ok);
    });
}

/// US-5/S2 (the cancel escape): a staged draft discarded on focus loss
/// dispatches nothing and the echo reverts to the committed snapshot —
/// the staged-edit cancel path is the field's standard escape.
#[test]
fn us5_s2_cancel_keeps_snapshot() {
    run(|state| {
        let mut harness = harness_with(state, snapshot_with_poll(Some(60)));
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&harness);
        let window = harness.handle.window();
        let model = window.global::<SettingsModel>();
        window.invoke_settings_field_edited(16, "120".into());
        assert_eq!(model.get_files_poll_echo(), "120");
        window.invoke_settings_general_focus_lost(16);
        assert_eq!(
            model.get_files_poll_echo(),
            "60",
            "the staged value dropped; the snapshot's echo returned"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "a cancel dispatches nothing"
        );
    });
}

/// US-5 AC-2 (the typed `> 0` gate, the chunk plan's
/// `poll_secs_typed_refusal_nonpositive` row): a non-positive commit
/// refuses at the commit seam with the typed invalidity — the same
/// gate the CLI knob answers to — and dispatches nothing.
#[test]
fn us5_s3_poll_secs_typed_refusal_nonpositive() {
    run(|state| {
        let mut harness = harness_with(state, snapshot_with_poll(Some(60)));
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&harness);
        let window = harness.handle.window();
        let model = window.global::<SettingsModel>();
        // The typed edit is staged per keystroke (the field's grammar):
        // the draft is present under its invalid flavour after the
        // refusal.
        window.invoke_settings_field_edited(16, "0".into());
        window.invoke_files_poll_secs_committed(16, "0".into());
        assert!(
            harness.drain_requests().is_empty(),
            "a refused commit dispatches nothing"
        );
        assert_eq!(model.get_last_outcome(), SettingsOutcome::Rejected);
        assert_eq!(
            model.get_validity_files_poll(),
            FieldValidity::Invalid,
            "the typed refusal surfaces on the field's flavour"
        );
        assert_eq!(
            model.get_files_poll_echo(),
            "0",
            "the draft stays visible under its invalid flavour"
        );
    });
}

// -- US-6: the degraded-boot badge (W-7, R-5) -------------------------

/// US-6/S1 (AC-1): a degraded boot's broadcast renders the badge and it
/// persists (a later unrelated event never clears it — R-5's
/// persistent display).
#[test]
fn us6_s1_degraded_badge_renders() {
    run(|state| {
        let harness = plain_harness(state);
        harness
            .handle
            .observe_system_event(&SystemEvent::JournalDegraded {
                details: vec![taskboard_domain::persistence::CorruptionDetail::UNKNOWN],
                dropped: 2,
                head: None,
            });
        assert!(
            harness.handle.window().get_journal_degraded_visible(),
            "the badge renders on the degraded signal"
        );
        // Persistence: an unrelated episode does not dismiss it.
        harness
            .handle
            .observe_system_event(&SystemEvent::SettingsChanged);
        assert!(
            harness.handle.window().get_journal_degraded_visible(),
            "the badge persists (R-5: no dismiss, the next healthy boot replaces it)"
        );
    });
}

/// US-6/S2 (AC-2): a healthy boot — no degraded event arrived — shows
/// no badge.
#[test]
fn us6_s2_healthy_boot_no_badge() {
    run(|state| {
        let harness = plain_harness(state);
        assert!(
            !harness.handle.window().get_journal_degraded_visible(),
            "a healthy boot renders no badge"
        );
    });
}

/// US-6 AC-3 (R-5, structural): the badge lane is display-only — the
/// degraded broadcast dispatches no command and no app request.
#[test]
fn us6_s3_badge_installs_no_command_callbacks() {
    run(|state| {
        let mut harness = plain_harness(state);
        harness
            .handle
            .observe_system_event(&SystemEvent::JournalDegraded {
                details: Vec::new(),
                dropped: 1,
                head: None,
            });
        harness.assert_commands(&[]);
        assert!(
            harness.drain_requests().is_empty(),
            "the badge dispatches no app request"
        );
    });
}

// -- US-8: the HabitRepair chord -----------------------------------------

/// A daily quota-1 habit on the fixture board (the walk's habit-row
/// seed; the habits journey's `state_habit` shape, quota reduced so a
/// single log reaches the repair-closing quota).
fn seed_repair_habit(state: &mut taskboard_domain::AppState) -> taskboard_domain::HabitId {
    use taskboard_domain::NonZeroCount;
    use taskboard_domain::habit::{Habit, HabitClocks, HabitTarget};
    use taskboard_domain::ids::HabitId;
    let board = *state.boards.keys().next().expect("fixture board");
    let id = HabitId::from_uuid(uuid::Uuid::now_v7());
    state.habits.habits.insert(
        id,
        Habit {
            id,
            board,
            title: "repair-me".into(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(1).expect("quota 1")),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(chrono::Utc::now()),
            continued_from: None,
            deleted: false,
            by: None,
            paused_until: None,
        },
    );
    id
}

/// Focuses the fixture's habit row on Today (the walk domain is the
/// overdue task then the habit: two vertical strokes).
fn focus_habit_row(harness: &mut support::harness::Harness) {
    harness.press(ActionId::NavFocusDown);
    harness.press(ActionId::NavFocusDown);
    harness.driver.advance_time(0);
}

/// US-8/AC-1 (`us8_s1_habit_repair_chord_resolves`): a focused habit
/// with its make-up window open — Ctrl+Shift+d resolves
/// `ActionId::HabitRepair` through the card strip's one-shot mint
/// (the same `AdjustHabitLog` the repair button routes; click ≡ key).
#[test]
fn us8_s1_habit_repair_chord_resolves() {
    let (_fixture, mut state) = fixture::journey_state();
    let habit = seed_repair_habit(&mut state);
    support::harness::run_journey_no_escape(state, move |harness| {
        focus_habit_row(harness);
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+d")
                .expect("the US-8 spelling parses"),
        );
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::AdjustHabitLog { habit: logged, count: 1, .. }]
                    if *logged == habit
            ),
            "the repair chord routes the strip's make-up +1, got {drained:?}"
        );
    });
}

/// US-8/AC-2 (`us8_s2_repair_no_window_typed_refusal`): at quota the
/// make-up window is closed — the chord resolves the typed refusal
/// and dispatches nothing (the card's greyed repair is the pointer
/// twin).
#[test]
fn us8_s2_repair_no_window_typed_refusal() {
    let (_fixture, mut state) = fixture::journey_state();
    let habit = seed_repair_habit(&mut state);
    // Close the make-up window at the source: the previous canonical
    // window's count already meets the quota (the repair predicate's
    // own oracle — quota + grace), derived through the harness
    // calendar the pass clock reads (the anchored-relative-oracle
    // rule; never a wall-clock literal).
    {
        let calendar = fixture::harness_calendar();
        let today = calendar.today(chrono::Utc::now());
        let prev = taskboard_domain::habit::period_before(taskboard_domain::PeriodKind::Day, today);
        state.habits.log.insert(
            taskboard_domain::HabitLogKey {
                habit,
                window: taskboard_domain::WindowStart::of_period(
                    taskboard_domain::PeriodKind::Day,
                    prev,
                ),
            },
            taskboard_domain::habit::HabitLogEntry {
                count: 1,
                clock: chrono::Utc::now(),
                by: None,
                skip: false,
            },
        );
    }
    support::harness::run_journey_no_escape(state, move |harness| {
        focus_habit_row(harness);
        harness.assert_commands(&[]);
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+d")
                .expect("the US-8 spelling parses"),
        );
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
    });
}

// -- US-11: the series pause/resume chords -------------------------------

/// US-11/AC-1 (`us11_s1_series_pause_chord_resolves`): an active
/// series' Ctrl+Shift+p opens the recur row's inline pause editor over
/// the viewed subject — the chord twin of the pause affordance's view
/// intent. The spelling is the R-7 move (the plan's Ctrl+Shift+r is
/// `SeriesEdit`'s landed chord; the second move is the global-shadow
/// law — Ctrl+Shift+p is `ViewPlanNextDay`'s global stroke and the
/// shadow breaks the planner-reach contract, so the free `w` carries
/// the pair in every preset).
#[test]
fn us11_s1_series_pause_chord_resolves() {
    let (fx, mut state) = fixture::golden_state();
    seed_daily_recur(&mut state, fx.task0);
    support::harness::run_journey(state, move |harness| {
        mount_task_detail(harness);
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+w")
                .expect("the US-11 pause spelling parses"),
        );
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().recur_pause_open,
            "the chord opens the pause editor (the click seam's twin)"
        );
        harness.assert_commands(&[]);
    });
}

/// US-11/AC-2 (`us11_s2_series_resume_chord_resolves`): a paused
/// series' Ctrl+Shift+u routes the resume — `PauseSeries { until:
/// None }`, the field-clear twin the recur row's resume seam routes.
#[test]
fn us11_s2_series_resume_chord_resolves() {
    let (fx, mut state) = fixture::golden_state();
    seed_daily_recur(&mut state, fx.task0);
    if let Some(task) = state.tasks.get_mut(&fx.task0) {
        let rule = task.recur.as_mut().expect("seeded head rule");
        rule.paused_until =
            Some(chrono::NaiveDate::from_ymd_opt(2026, 10, 25).expect("fixture date"));
    }
    support::harness::run_journey(state, move |harness| {
        mount_task_detail(harness);
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+u")
                .expect("the US-11 resume spelling parses"),
        );
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::PauseSeries { until: None, .. }]
            ),
            "the resume chord clears the pause at the boundary, got {drained:?}"
        );
    });
}

// -- US-12: the tracker-kind toggle binding ------------------------------

/// US-12/AC-1 (`us12_s1_kind_toggle_chord_resolves`): the kind-toggle
/// key at the detail (the preset spelling — standard's Ctrl+Shift+k)
/// resolves `ActionId::TaskToggleTracker` — the tracker-kind flag, the
/// R-8 smart toggle's distinct neighbour.
#[test]
fn us12_s1_kind_toggle_chord_resolves() {
    let (fx, state) = fixture::golden_state();
    let task = fx.task0;
    support::harness::run_journey(state, move |harness| {
        mount_task_detail(harness);
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+k")
                .expect("the US-12 spelling parses"),
        );
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::UpdateTask { id, changes, .. }]
                    if *id == task && changes.kind.is_some()
            ),
            "the kind toggle flips the tracker flag at the boundary, got {drained:?}"
        );
    });
}

// -- US-7: the row context-menu key ---------------------------------------

/// The `KeyboardState` global (the menu-open fact's projection; the
/// habits journey's helper shape).
fn keyboard(harness: &support::harness::Harness) -> taskboard_ui_slint::KeyboardState<'_> {
    use slint::ComponentHandle as _;
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::KeyboardState>()
}

/// US-7/AC-1 (`us7_s1_row_menu_key_opens_task_menu`): a focused task
/// row's row-menu key opens the task context menu over the focused
/// subject — the `menu_row_subject` path the pointer right-click
/// uses (click ≡ key by construction; the quick bar's presence is
/// the task-menu page's structural fact).
#[test]
fn us7_s1_row_menu_key_opens_task_menu() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_no_escape(state, move |harness| {
        // The walk lands on the overdue task (the today domain's first
        // row).
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert!(!keyboard(harness).get_menu_open());
        harness.press(ActionId::RowMenu);
        harness.driver.advance_time(0);
        assert!(
            keyboard(harness).get_menu_open(),
            "the row-menu key opens the task context menu"
        );
        let bar = keyboard(harness).get_menu_quick_bar();
        assert!(
            bar.row_count() > 0,
            "the task menu page carries the quick bar (the curated page)"
        );
    });
}

/// US-7/AC-2 (`us7_s2_row_menu_key_opens_habit_menu`): a focused
/// habit row's key opens the habit context menu (the direct opener;
/// the convert-arm chord's arm-and-reproject posture is untouched).
#[test]
fn us7_s2_row_menu_key_opens_habit_menu() {
    use taskboard_domain::habit::{Habit, HabitClocks, HabitTarget};
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = taskboard_domain::ids::HabitId::from_uuid(uuid::Uuid::now_v7());
    state.habits.habits.insert(
        habit,
        Habit {
            id: habit,
            board,
            title: "menu-me".into(),
            description: String::new(),
            target: HabitTarget::PerDay(taskboard_domain::NonZeroCount::new(1).expect("quota 1")),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(chrono::Utc::now()),
            continued_from: None,
            deleted: false,
            by: None,
            paused_until: None,
        },
    );
    support::harness::run_journey_no_escape(state, move |harness| {
        // The walk: overdue task, then the habit row.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        harness.press(ActionId::RowMenu);
        harness.driver.advance_time(0);
        assert!(
            keyboard(harness).get_menu_open(),
            "the row-menu key opens the habit context menu"
        );
        let bar = keyboard(harness).get_menu_quick_bar();
        assert_eq!(
            bar.row_count(),
            0,
            "the habit menu page is the bare entries page (no quick bar)"
        );
    });
}

/// US-7/AC-3 (`us7_s3_row_menu_no_focus_typed_refusal`): a subjectless
/// walk's key resolves the typed refusal — no menu, nothing
/// dispatched.
#[test]
fn us7_s3_row_menu_no_focus_typed_refusal() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.press(ActionId::RowMenu);
        harness.driver.advance_time(0);
        assert!(
            !keyboard(harness).get_menu_open(),
            "no focused row, no menu"
        );
        harness.assert_commands(&[]);
    });
}

// -- US-10: the rule-cursor disarm chord ----------------------------------

/// Arms the rule cursor the walk's own way (a seeded one-rule roster:
/// the first vertical stroke arms at row 0) and returns the model
/// row.
fn arm_rule_cursor(harness: &mut support::harness::Harness) {
    harness.press(ActionId::NavFocusDown);
    harness.driver.advance_time(0);
}

/// The rule list's first row (the projection the cursor's emphasis
/// reads).
fn rule_row_zero(harness: &support::harness::Harness) -> taskboard_ui_slint::RuleRowModel {
    harness
        .handle
        .window()
        .get_task_model()
        .rules
        .row_data(0)
        .expect("the seeded rule row renders")
}

/// US-10/AC-1 (`us10_s1_disarm_chord_disarms_cursor`): with the rule
/// cursor armed, Ctrl+Shift+a disarms it — and the NEXT vertical no
/// longer re-arms (the latch returns the verticals to the chunk-9
/// control ladder without a view exit or foreign settle).
#[test]
fn us10_s1_disarm_chord_disarms_cursor() {
    let (fx, mut state) = fixture::golden_state();
    let rule = fixture::add_armed_rule(&mut state, fx.task0, 0x4242);
    let _ = rule;

    support::harness::run_journey(state, move |harness| {
        mount_task_detail(harness);
        harness.assert_commands(&[]);
        arm_rule_cursor(harness);
        assert!(
            rule_row_zero(harness).selected,
            "the arming stroke lands the cursor on row 0"
        );
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+a")
                .expect("the US-10 spelling parses"),
        );
        harness.driver.advance_time(0);
        assert!(
            !rule_row_zero(harness).selected,
            "the chord disarms the cursor"
        );
        // The latch: the next vertical is the control ladder's — the
        // cursor does not re-arm.
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert!(
            !rule_row_zero(harness).selected,
            "the disarmed verticals stay off the cursor (the latch)"
        );
        harness.assert_commands(&[]);
    });
}

/// US-10/AC-2 (`us10_s2_disarm_unarmed_inert`): with the cursor not
/// armed, the chord is the typed inert — no state change, nothing
/// dispatched.
#[test]
fn us10_s2_disarm_unarmed_inert() {
    let (fx, mut state) = fixture::golden_state();
    let rule = fixture::add_armed_rule(&mut state, fx.task0, 0x4243);
    let _ = rule;
    support::harness::run_journey(state, move |harness| {
        mount_task_detail(harness);
        harness.assert_commands(&[]);
        harness.stroke(
            taskboard_ui_slint::keymap::stroke::parse("Ctrl+Shift+a")
                .expect("the US-10 spelling parses"),
        );
        harness.driver.advance_time(0);
        assert!(
            !rule_row_zero(harness).selected,
            "the unarmed chord changes nothing"
        );
        harness.assert_commands(&[]);
    });
}

// -- US-9: the break-rules roster cursor ----------------------------------

/// Seeds two break rules on the fixture board (the roster walk's
/// domain) and hands back their ids in canonical order.
fn seed_break_rules(
    state: &mut taskboard_domain::AppState,
) -> (
    taskboard_domain::BoardId,
    [taskboard_domain::BreakRuleId; 2],
) {
    let board = *state.boards.keys().next().expect("fixture board");
    let r0 = fixture::add_break_rule(
        state,
        board,
        "lunch",
        chrono::TimeDelta::minutes(30),
        taskboard_domain::Insistence::Normal,
    );
    let r1 = fixture::add_break_rule(
        state,
        board,
        "stretch",
        chrono::TimeDelta::minutes(5),
        taskboard_domain::Insistence::Normal,
    );
    (board, [r0, r1])
}

fn roster_row(
    harness: &support::harness::Harness,
    index: usize,
) -> taskboard_ui_slint::BreakRuleRowModel {
    use slint::Model as _;
    harness
        .handle
        .window()
        .get_break_rules_model()
        .row_data(index)
        .expect("the roster row renders")
}

/// US-9/AC-1+AC-2 (`us9_s1_roster_cursor_arms` /
/// `us9_s2_roster_cursor_walks_clamped`): the today walk's landing on
/// the roster arms the K4 cursor at the first row; further verticals
/// walk with clamp at the end (no wrap — the ends hold); the armed
/// styling rides the row model's `selected` flag.
#[test]
fn us9_s1_roster_cursor_arms_and_us9_s2_roster_cursor_walks_clamped() {
    let (_fixture, mut state) = fixture::journey_state();
    seed_break_rules(&mut state);
    support::harness::run_journey_no_escape(state, move |harness| {
        // The today walk: overdue task, then the roster's first row
        // (no habit rows in the fixture — the roster follows).
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert!(
            roster_row(harness, 0).selected,
            "the landing stroke arms the cursor on roster row 0 (US-9/S1)"
        );
        // S2: the walk proceeds, clamps at the last row.
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert!(
            !roster_row(harness, 0).selected && roster_row(harness, 1).selected,
            "the second stroke walks to row 1"
        );
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert!(
            roster_row(harness, 1).selected && !roster_row(harness, 0).selected,
            "the cursor clamps at the last row (no wrap)"
        );
        harness.assert_commands(&[]);
    });
}

/// US-9/AC-3 (`us9_s3_roster_row_verb_uses_own_seam`): with the
/// cursor armed on a row, the remove verb routes the row's own seam —
/// the armed-confirm staging engages on the cursor'd row (no command:
/// the strip's Confirm is the only dispatching verb).
#[test]
fn us9_s3_roster_row_verb_uses_own_seam() {
    let (_fixture, mut state) = fixture::journey_state();
    seed_break_rules(&mut state);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert!(roster_row(harness, 0).selected);
        // The row verb: the remove arm, through the row's own seam —
        // the routed verb the re-rooted click seam rides.
        harness.handle.dispatch_action(ActionId::BreakRuleRemove);
        harness.driver.advance_time(0);
        let row = roster_row(harness, 0);
        assert!(row.armed, "the arm staging engages on the cursor'd row");
        assert!(
            row.selected,
            "pointer/cursor coherence: the cursor sits on the armed row"
        );
        harness.assert_commands(&[]);
        // The arm stroke's repeat is not the confirm — the toggle
        // disarms (the seam's letter, preserved through the route).
        harness.handle.dispatch_action(ActionId::BreakRuleRemove);
        harness.driver.advance_time(0);
        assert!(
            !roster_row(harness, 0).armed,
            "the repeat disarms; nothing dispatched"
        );
        harness.assert_commands(&[]);
    });
}
