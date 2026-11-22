// SPDX-License-Identifier: AGPL-3.0-only

//! The journey/golden fixture (chunk plan §5.1): the two-stack board
//! shape of the §6 component tests, extracted for the integration
//! binaries (the lib's private copy stays — duplication across test
//! binaries is the cargo reality, noted in the plan).
//!
//! R7/R8: titles are ASCII-only so golden rendering can never fall
//! back to a system font for a missing glyph.

use chrono::TimeDelta;
use std::collections::BTreeMap;
use taskboard_domain::{
    AppState, Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId, SyncPhase, Task,
    TaskClocks, TaskId,
};

fn epoch() -> chrono::DateTime<chrono::Utc> {
    BoardClocks::epoch()
}

fn task_clocks() -> TaskClocks {
    TaskClocks {
        title: epoch(),
        description: epoch(),
        duedate: epoch(),
        scheduled: epoch(),
        done: epoch(),
        position: epoch(),
        effort: epoch(),
        importance: epoch(),
        urgency: epoch(),
        recur: epoch(),
        kind: epoch(),

        archived: epoch(),
        deleted: epoch(),
    }
}

fn stack_clocks() -> StackClocks {
    StackClocks {
        title: epoch(),
        order: epoch(),
        deleted: epoch(),
    }
}

fn task(id: TaskId, stack: StackId, order: i64) -> Task {
    Task {
        id,
        remote: None,
        title: format!("task-{order}"),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack,
        order,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,

        archived: false,
        deleted: false,
        clocks: task_clocks(),
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    }
}

fn stack(id: StackId, board: BoardId, title: &str, order: i64) -> Stack {
    Stack {
        id,
        remote: None,
        board,
        title: title.to_owned(),
        order,
        archived: false,
        deleted: false,
        clocks: stack_clocks(),
        authors: taskboard_domain::entities::StackAuthors::default(),
        remote_seen: None,
    }
}

fn board(id: BoardId) -> Board {
    Board {
        id,
        remote: None,
        title: "board".to_owned(),
        color: Color::new(""),
        archived: false,
        deleted: false,
        clocks: BoardClocks {
            title: epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::BoardAuthors::default(),
        remote_seen: None,
    }
}

/// The fixture's ids the journeys address at the typed seams (the
/// command payloads' stacks, and the task `journey_state` makes
/// overdue).
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    pub stack_a: StackId,
    pub stack_b: StackId,
    /// Stack "a"'s first task — the walk's `NavNextItem` focus target.
    pub task0: TaskId,
    /// Stack "a"'s second task; `journey_state` marks it overdue.
    pub task1: TaskId,
}

/// The timers-surface helpers (DR-3/3, D-P13): mint a live alarm into
/// `state` and hand back its id — the journeys address rows through
/// these, never hand-building the domain rows inline.
#[must_use]
pub fn add_alarm(
    state: &mut AppState,
    label: &str,
    fire_at: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::AdhocAlarmId {
    add_alarm_with_rung(state, label, fire_at, taskboard_domain::Insistence::Normal)
}

/// The insistence-rung variant (r4 D34 remainder #5): the badge and
/// escalation journeys mint `Alarm`-rung timers through it.
#[must_use]
pub fn add_alarm_with_rung(
    state: &mut AppState,
    label: &str,
    fire_at: chrono::DateTime<chrono::Utc>,
    insistence: taskboard_domain::Insistence,
) -> taskboard_domain::AdhocAlarmId {
    let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
    state.reminders.alarms.insert(
        id,
        taskboard_domain::AdhocAlarm {
            insistence,
            id,
            label: label.to_owned(),
            fire_at,
            created_at: fire_at - TimeDelta::minutes(1),
        },
    );
    id
}

/// The journal-ledger helper (T-15, #798): mints one live reflection
/// entry with the given text/attributes and hands back its id — the
/// journeys address ledger rows through these, never hand-building
/// the domain row.
#[must_use]
pub fn add_reflection(
    state: &mut AppState,
    text: &str,
    attributes: taskboard_domain::ReflectionAttributes,
    now: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::ReflectionId {
    // Deliberately NOT `harness_calendar()` (#884, plan
    // 2026-11-18-journey-fixture-seams-plan §D1.5): these 4:00 review
    // windows are entry *content* whose boundaries the green suite
    // already pins — changing the start would shift window boundaries
    // and re-anchor walk expectations. The divergence #884 names is
    // about the day-vocabulary derivations, not this helper's
    // explicit 4:00 review windows.
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: chrono::NaiveTime::from_hms_opt(4, 0, 0).expect("4:00"),
        offset: chrono::FixedOffset::east_opt(0).expect("utc"),
    };
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, yesterday, calendar);
    let entry = taskboard_domain::ReflectionEntry {
        id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: taskboard_domain::ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: text.to_owned(),
        attributes,
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    let id = entry.id;
    state.reflection.entries.insert(entry.id, entry);
    id
}

/// The break-rule roster helper (T-14, #797): mints one rule roaming
/// with the given board and hands back its id — the journeys address
/// roster rows through these, never hand-building the domain row.
#[must_use]
pub fn add_break_rule(
    state: &mut AppState,
    board: BoardId,
    label: &str,
    threshold: chrono::TimeDelta,
    insistence: taskboard_domain::Insistence,
) -> taskboard_domain::BreakRuleId {
    let id = taskboard_domain::BreakRuleId::from_uuid(uuid::Uuid::now_v7());
    state.reminders.break_rules.insert(
        id,
        taskboard_domain::BreakRule {
            id,
            board,
            label: label.to_owned(),
            threshold: taskboard_domain::WorkThreshold::try_new(threshold)
                .expect("the fixture threshold is strictly positive"),
            insistence,
            created_at: epoch(),
            by: None,
        },
    );
    id
}

/// The tracking-surface helper (DR-5/4, D-TU12): mints one open entry
/// into `state` and hands back its id — the journeys address the chip
/// and the badge through these, never hand-building the domain row.
#[must_use]
pub fn add_open_entry(
    state: &mut AppState,
    task: taskboard_domain::TaskId,
    title: &str,
    start: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::TimeEntryId {
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task,
            task_title: title.to_owned(),
            start,
            end: None,
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
            by: None,
        },
    );
    id
}

/// The reminder-roster helper (UF1, #354): mints one armed at-instant
/// rule over `task` and hands back its id — the journeys address the
/// rule rows through these, never hand-building the domain row.
#[must_use]
pub fn add_armed_rule(
    state: &mut AppState,
    task: taskboard_domain::TaskId,
    ordinal: u128,
) -> taskboard_domain::ReminderRuleId {
    use chrono::Timelike as _;
    let id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(ordinal));
    let fire =
        chrono::Utc::now().with_nanosecond(0).expect("whole second") + chrono::Duration::hours(24);
    let instant = taskboard_domain::AtInstant::whole_second(fire).expect("whole second");
    state.reminders.rules.insert(
        id,
        taskboard_domain::ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id,
            task,
            kind: taskboard_domain::ReminderKind::At(instant),
            created_at: fire,
            updated_at: fire,
            by: None,
        },
    );
    id
}

/// The calibration-surface helper (#214): mints one completed task —
/// optionally estimated, optionally with one closed Work entry — into
/// `state` and hands back its id. The review goldens/journeys address
/// the ledger through these, never hand-building the domain rows
/// inline.
#[allow(clippy::too_many_arguments)]
pub fn add_completed_task(
    state: &mut AppState,
    stack: StackId,
    title: &str,
    done_at: chrono::DateTime<chrono::Utc>,
    estimate_seconds: Option<u64>,
    work_span: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
) -> TaskId {
    let id = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut t = task(id, stack, 0);
    title.clone_into(&mut t.title);
    t.done = Some(done_at);
    t.effort = estimate_seconds.map(taskboard_domain::Effort::new);
    state.tasks.insert(id, t);
    if let Some((start, end)) = work_span {
        let entry_id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            entry_id,
            taskboard_domain::TimeEntry {
                id: entry_id,
                task: id,
                task_title: title.to_owned(),
                start,
                end: Some(end),
                created_at: start,
                updated_at: end,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
                by: None,
            },
        );
    }
    id
}

/// Records the delivery receipt for an alarm's occurrence (the fire
/// fact that makes the row awaiting).
pub fn deliver_alarm(
    state: &mut AppState,
    alarm: taskboard_domain::AdhocAlarmId,
    occurrence: chrono::DateTime<chrono::Utc>,
) {
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: taskboard_domain::ReminderRef::Adhoc(alarm),
            occurrence,
            delivered_at: occurrence,
        });
}

/// A delivered (fired ∧ ¬dismissed) task-anchored notification: an
/// open task gains a due instant plus an explicit at-due rule whose
/// delivery receipt exists — the notification surface's task row
/// (the anchor side that keeps all four task-shaped buttons).
pub fn deliver_task_notification(
    state: &mut AppState,
    task: taskboard_domain::TaskId,
    due: chrono::DateTime<chrono::Utc>,
) {
    if let Some(t) = state.tasks.get_mut(&task) {
        t.duedate = Some(due);
    }
    let rule_id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
    let now = chrono::Utc::now();
    state.reminders.rules.insert(
        rule_id,
        taskboard_domain::ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id: rule_id,
            task,
            kind: taskboard_domain::ReminderKind::DueOffset(
                taskboard_domain::ReminderOffset::at_due(),
            ),
            created_at: now,
            updated_at: now,
            by: None,
        },
    );
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: taskboard_domain::ReminderRef::Rule(rule_id),
            occurrence: due,
            delivered_at: due,
        });
}

/// The harness's day vocabulary (#884): *the* `DayCalendar` every
/// fixture-side day derivation must use — derived from the same two
/// sources the mounted adapter's calendar derives from, so fixture
/// and adapter read one calendar by construction, not by two
/// hand-maintained constants drifting apart (the parallel
/// hand-maintenance the issue names). The adapter's calendar is
/// `AppState::settings.day_calendar(boot_offset)` (`Adapter::calendar`
/// → `SettingsSnapshot::day_calendar`); the fixture harness mounts
/// `AppState::default()` (whose settings are
/// `SettingsSnapshot::default()` — the settings surface exposes no
/// `day_start_time`) with `UiSettings::default().boot_offset`, so
/// those exact defaults are read here. A default change on either
/// side flows into this constructor automatically; a harness that
/// overrides a mount value must pass the override here too.
#[must_use]
pub fn harness_calendar() -> taskboard_domain::DayCalendar {
    taskboard_domain::DayCalendar {
        day_start_time: taskboard_domain::SettingsSnapshot::default().day_start_time,
        offset: taskboard_ui_slint::UiSettings::default().boot_offset,
    }
}

/// The journey fixture (§5.1): one board, two stacks ("a" with tasks
/// 0 and 1, 1 overdue; "b" with task 2), sync `Offline` with two
/// pending ops — the step-9 status read's subject.
#[must_use]
pub fn journey_state() -> (Fixture, AppState) {
    journey_state_at(chrono::Utc::now())
}

/// [`journey_state`] seeded against an explicit `now`: the single-read
/// capture (`docs/testing_strategy.org` §6.1, tool 6). The overdue
/// stamp is derived through the same `DayCalendar`/`review_window`
/// vocabulary the harness's day window reads — anchored on the
/// *vocabulary*, not the pass's instant — so the fixture is
/// Today-nonempty by construction (#843): `task1`'s stamp is
/// yesterday's midnight in the fixture's UTC calendar, strictly
/// before any pass window's start (the overdue predicate
/// `duedate < now` is satisfied for every pass `now ≥ now`).
#[must_use]
pub fn journey_state_at(now: chrono::DateTime<chrono::Utc>) -> (Fixture, AppState) {
    let (fixture, mut state) = golden_state_at(now);
    let mut overdue = state.tasks.get(&fixture.task1).expect("task").clone();
    // Midnight-aligned (round-3 D9): the due phrases now carry the
    // UTC `· HH:MM` suffix for non-midnight instants — a wall-clock
    // stamp would make the goldens run-dependent. Midnight renders
    // exactly the date-only shape the goldens pin.
    let calendar = harness_calendar();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    overdue.duedate = Some(yesterday.and_hms_opt(0, 0, 0).expect("midnight").and_utc());
    state.tasks.insert(fixture.task1, overdue);
    // The projection is derived: two fabricated ops give the badge its
    // pending count; the phase carries the stored Offline.
    let board = *state.boards.keys().next().expect("fixture board");
    state.sync.set_stored(board, SyncPhase::Offline, None);
    let ops: Vec<taskboard_domain::PendingOp> = (0..2)
        .map(|i| taskboard_domain::PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(
                u128::try_from(0x9000 + i).expect("fixture op index fits"),
            )),
            op: taskboard_domain::LocalOp::UpdateTask(fixture.task1),
            queued_at: epoch(),
        })
        .collect();
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &ops,
        &mut state.sync,
    );
    (fixture, state)
}

/// The pinned render clock of every clock-exposed test in this
/// binary (`docs/testing_strategy.org` §6.2, #660): a fixed mid-day UTC
/// instant — the review-ledger template's capture class. Seeds derive
/// from it; the mount freezes `day_ctx` at it. Never read the wall
/// clock next to it.
pub const PINNED_NOW: chrono::DateTime<chrono::Utc> = {
    let Some(date) = chrono::NaiveDate::from_ymd_opt(2026, 11, 6) else {
        panic!("2026-11-06 is a calendar date");
    };
    let Some(instant) = date.and_hms_opt(12, 0, 0) else {
        panic!("12:00:00 is a wall time");
    };
    instant.and_utc()
};

/// The clean two-stack board (the golden tier's fixture, chunk plan
/// §7): no overdue banner (banner labels derive from wall-clock
/// relative time and would flake across hour boundaries), no flow
/// hint, sync idle.
#[must_use]
pub fn golden_state() -> (Fixture, AppState) {
    golden_state_at(chrono::Utc::now())
}

/// [`golden_state`] seeded against an explicit `now`: the running
/// timer's open entry is stamped `now − 12 m`, so a pass whose clock
/// is frozen at `now` renders the chip at the same age every run.
/// The review walk's surfacing card (#638) is state-derived with
/// wall-clock date text, so the pin lives here: a live Day entry for
/// the latest closed window clears the card for this run — the walk
/// journeys that need the due state clear the pin
/// (`journeys::walk_fixture_state`).
#[must_use]
pub fn golden_state_at(now: chrono::DateTime<chrono::Utc>) -> (Fixture, AppState) {
    let board_id = BoardId::from_uuid(uuid::Uuid::now_v7());
    let stack_a = StackId::from_uuid(uuid::Uuid::now_v7());
    let stack_b = StackId::from_uuid(uuid::Uuid::now_v7());
    let task0 = TaskId::from_uuid(uuid::Uuid::now_v7());
    let task1 = TaskId::from_uuid(uuid::Uuid::now_v7());
    let task2 = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut state = AppState::default();
    state.boards.insert(board_id, board(board_id));
    state
        .stacks
        .insert(stack_a, stack(stack_a, board_id, "a", 0));
    state
        .stacks
        .insert(stack_b, stack(stack_b, board_id, "b", 1));
    state.tasks.insert(task0, task(task0, stack_a, 0));
    state.tasks.insert(task1, task(task1, stack_a, 1));
    state.tasks.insert(task2, task(task2, stack_b, 0));
    // The seeded running timer (DR-5/4, D-TU12): task1 holds the open
    // entry, 12 minutes old at fixture construction — the chip and
    // the badge appear in the golden matrix (wall-clock-derived text
    // is the established golden class; a minute-boundary straddle is
    // the recorded residual risk).
    let open_task1 = add_open_entry(&mut state, task1, "task-1", now - TimeDelta::minutes(12));
    debug_assert!(state.tracking.entries.contains_key(&open_task1));
    // The review-card pin (#638): the latest closed Day window is
    // already recorded, so the derived card never leaks into a
    // golden (see `golden_state`'s doc).
    let calendar = harness_calendar();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, yesterday, calendar);
    let entry = taskboard_domain::ReflectionEntry {
        id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: taskboard_domain::ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: String::new(),
        attributes: taskboard_domain::ReflectionAttributes::default(),
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    state.reflection.entries.insert(entry.id, entry);
    (
        Fixture {
            stack_a,
            stack_b,
            task0,
            task1,
        },
        state,
    )
}

/// The enriched golden fixture (golden coverage wave, plan §5): the
/// clean two-stack board **plus** the report §6 enrichment, as a
/// superset constructor — `golden_state_at` itself stays byte-stable
/// for the 27 committed goldens (the wave plan's fixture decision).
/// Layers, all ASCII-titled and pinned/midnight-aligned:
///
/// - a label roster with chips incl. the `LABEL_CHIP_CAP` overflow
///   (four memberships on `task0`, three chips render, `+1` folds),
/// - one recurring task (`task2`, a daily series anchored at `now`'s
///   local date — midnight-aligned by construction, date-space),
/// - one reminder badge (an armed at-instant rule on `task1`, pinned
///   `now + 24 h` — never the wall clock),
/// - a second board ("dev", stack "c") whose task carries one
///   long/overflow ASCII title (the card's overflow ellipsis),
/// - one degraded sync state on the fixture board (`Offline` + two
///   pending ops, the `journey_state_at` precedent) with
///   `derive_sync_states` re-run last.
///
/// The review-card pin (#638) is inherited from [`golden_state_at`].
#[allow(clippy::too_many_lines)] // the enrichment layers are each load-bearing
#[must_use]
pub fn golden_state_enriched_at(now: chrono::DateTime<chrono::Utc>) -> (Fixture, AppState) {
    let (fixture, mut state) = golden_state_at(now);
    let board_id = *state.boards.keys().next().expect("fixture board");

    // The label roster + chip overflow on task0.
    let mut memberships = std::collections::BTreeMap::new();
    for (i, (title, hex)) in [
        ("urgent", "#e5534b"),
        ("reading", "#7aa2f7"),
        ("chores", "#4ea784"),
        ("someday", "#a6abbd"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = taskboard_domain::LabelId::from_uuid(uuid::Uuid::from_u128(
            u128::try_from(0xA000 + i).unwrap_or(0),
        ));
        state.labels.insert(
            id,
            taskboard_domain::Label {
                id,
                remote: None,
                board: board_id,
                title: title.to_owned(),
                color: Color::new(hex),
                deleted: false,
                clocks: taskboard_domain::LabelClocks {
                    title: epoch(),
                    color: epoch(),
                    deleted: epoch(),
                },
                authors: taskboard_domain::entities::LabelAuthors::default(),
                remote_seen: None,
            },
        );
        memberships.insert(
            id,
            taskboard_domain::entities::MembershipState::Added { at: epoch() },
        );
    }
    let mut task0 = state.tasks.get(&fixture.task0).expect("task0").clone();
    task0.label_memberships = memberships;
    state.tasks.insert(fixture.task0, task0);

    // The recurring series on stack b's task (daily, anchored at the
    // pin's local date — date-space, no wall clock).
    let task2_id = state
        .tasks
        .values()
        .find(|t| t.stack == fixture.stack_b)
        .expect("stack b's task")
        .id;
    let mut task2 = state.tasks.get(&task2_id).expect("task2").clone();
    task2.recur = Some(taskboard_domain::RecurRule {
        series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(0x5EED)),
        anchor: now.date_naive(),
        pattern: taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::EveryDays::new(1).expect("1 is a valid interval"),
        },
        bound: taskboard_domain::RecurBound::None,
        continued_from: None,
        paused_until: None,
    });
    state.tasks.insert(task2_id, task2);

    // The reminder badge on task1: an armed at-instant rule pinned a
    // day past `now` (the `add_armed_rule` shape without its wall
    // clock read).
    let rule_id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(0x9E1D));
    let fire = now + TimeDelta::try_hours(24).unwrap();
    let instant = taskboard_domain::AtInstant::whole_second(fire).expect("whole second");
    state.reminders.rules.insert(
        rule_id,
        taskboard_domain::ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id: rule_id,
            task: fixture.task1,
            kind: taskboard_domain::ReminderKind::At(instant),
            created_at: now,
            updated_at: now,
            by: None,
        },
    );

    // The second board ("dev") with one long-titled task — the
    // overflow ellipsis's pin, off the captured board's cards.
    let dev_board = BoardId::from_uuid(uuid::Uuid::from_u128(0xD001));
    let mut dev = board_fn(dev_board);
    "dev".clone_into(&mut dev.title);
    state.boards.insert(dev_board, dev);
    let stack_c = StackId::from_uuid(uuid::Uuid::from_u128(0xC001));
    state
        .stacks
        .insert(stack_c, stack(stack_c, dev_board, "c", 0));
    let long_task = TaskId::from_uuid(uuid::Uuid::from_u128(0x7001));
    let mut long = task(long_task, stack_c, 0);
    "a very long ascii task title that overflows the card column width".clone_into(&mut long.title);
    state.tasks.insert(long_task, long);

    // The degraded sync state (the journey_state_at precedent):
    // stored Offline + two fabricated pending ops, projection derived.
    state.sync.set_stored(board_id, SyncPhase::Offline, None);
    let ops: Vec<taskboard_domain::PendingOp> = (0..2)
        .map(|i| taskboard_domain::PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(
                u128::try_from(0x9100 + i).unwrap_or(0),
            )),
            op: taskboard_domain::LocalOp::UpdateTask(fixture.task1),
            queued_at: epoch(),
        })
        .collect();
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &ops,
        &mut state.sync,
    );
    (fixture, state)
}

/// The n-stack clean board (r4 chunk 2, the plan's
/// `golden_state_with_stacks`): one board, `n` stacks titled "a", "b",
/// "c", … with one task each — the divider journeys' ≥3-column
/// fixture (the two-stack `golden_state` cannot pin "exactly two
/// neighbouring columns move").
#[must_use]
pub fn golden_state_with_stacks(n: usize) -> (Vec<StackId>, AppState) {
    debug_assert!((1..=8).contains(&n), "the fixture titles run 'a'..'h'");
    let board_id = BoardId::from_uuid(uuid::Uuid::now_v7());
    let mut state = AppState::default();
    state.boards.insert(board_id, board(board_id));
    let mut stacks = Vec::with_capacity(n);
    for i in 0..n {
        let index = i64::try_from(i).expect("fixture stack index fits");
        let letter = u32::try_from(i)
            .ok()
            .and_then(|i| char::from_u32(u32::from(b'a') + i))
            .expect("the fixture titles run 'a'..");
        let title = letter.to_string();
        let id = StackId::from_uuid(uuid::Uuid::now_v7());
        state.stacks.insert(id, stack(id, board_id, &title, index));
        let task_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        state.tasks.insert(task_id, task(task_id, id, 0));
        stacks.push(id);
    }
    (stacks, state)
}

/// The deterministic board ids and the second board's stack of
/// `two_board_state` (board one < board two, so board one seeds).
#[derive(Debug, Clone, Copy)]
pub struct TwoBoardFixture {
    pub board_one: BoardId,
    pub board_two: BoardId,
    /// Board two's single stack ("c").
    pub stack_c: StackId,
}

/// The two-board journey fixture (chunk-7 plan D9): board one
/// ("board one") with stacks "a" (tasks 0, 1) and "b" (task 2),
/// board two ("board two") with stack "c" (task 3); sync rows
/// derived idle. Deterministic board ids fix the canonical order —
/// board one is the seed.
#[must_use]
pub fn two_board_state() -> (TwoBoardFixture, AppState) {
    let board_one = BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
    let board_two = BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
    let stack_a = StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
    let stack_b = StackId::from_uuid(uuid::Uuid::from_u128(0x2002));
    let stack_c = StackId::from_uuid(uuid::Uuid::from_u128(0x2003));
    let mut state = AppState::default();
    for (id, title) in [(board_one, "board one"), (board_two, "board two")] {
        let mut board = board(id);
        title.clone_into(&mut board.title);
        state.boards.insert(id, board);
    }
    state
        .stacks
        .insert(stack_a, stack(stack_a, board_one, "a", 0));
    state
        .stacks
        .insert(stack_b, stack(stack_b, board_one, "b", 1));
    state
        .stacks
        .insert(stack_c, stack(stack_c, board_two, "c", 0));
    for (id, st, order) in [
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3001)), stack_a, 0),
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3002)), stack_a, 1),
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3003)), stack_b, 0),
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3004)), stack_c, 0),
    ] {
        state.tasks.insert(id, task(id, st, order));
    }
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &[],
        &mut state.sync,
    );
    (
        TwoBoardFixture {
            board_one,
            board_two,
            stack_c,
        },
        state,
    )
}

/// The today-view fixture (chunk 10 D12): the two-board journey state
/// with overdue stamps on one row per board — overdue is the one
/// section whose membership is wall-clock-proof (due < now, any run),
/// so the journey's walk is deterministic without injecting a
/// `DayCtx`. Returns the two walked task ids (board one's first, then
/// board two's task in display order: overdue sorts (order, id)).
#[must_use]
pub fn today_overdue_state() -> (TwoBoardFixture, AppState, [TaskId; 2]) {
    today_overdue_state_at(chrono::Utc::now())
}

/// [`today_overdue_state`] seeded against an explicit `now`: the
/// overdue anchor stays midnight-aligned at the *parameter's*
/// yesterday and the chip entry sits at `now − 12 m`, so a pass
/// frozen at `now` renders the same rows and captions every run.
/// The review-card pin (#638) lives here too: the goldens must never
/// bake in the state-derived card (see `golden_state_at`).
#[must_use]
pub fn today_overdue_state_at(
    now: chrono::DateTime<chrono::Utc>,
) -> (TwoBoardFixture, AppState, [TaskId; 2]) {
    let (fixture, mut state) = two_board_state();
    let calendar = harness_calendar();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, yesterday, calendar);
    let entry = taskboard_domain::ReflectionEntry {
        id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: taskboard_domain::ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: String::new(),
        attributes: taskboard_domain::ReflectionAttributes::default(),
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    state.reflection.entries.insert(entry.id, entry);
    // Midnight-aligned (round-3 D9): the due phrases carry the UTC
    // `· HH:MM` suffix for non-midnight instants, and a minute-
    // relative stamp flips its caption bucket across minute
    // boundaries — midnight renders the date-only shape, stable for
    // the run's lifetime and identical across runs.
    // Routed through the shared day vocabulary (#884): the raw
    // `now.date_naive() − 1` read bypassed the calendar, so on a
    // non-UTC device "which day is yesterday" differed between
    // fixture and adapter. At the harness calendar (MIN + UTC) the
    // result is identical to the old read by construction.
    let overdue_stamp = calendar
        .today(now)
        .pred_opt()
        .expect("yesterday exists")
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc();
    let board_one_task = TaskId::from_uuid(uuid::Uuid::from_u128(0x3001));
    let board_two_task = TaskId::from_uuid(uuid::Uuid::from_u128(0x3004));
    for id in [board_one_task, board_two_task] {
        let task = state.tasks.get_mut(&id).expect("fixture task");
        task.duedate = Some(overdue_stamp);
    }
    // The seeded running timer (DR-5/4, D-TU12): the today matrix
    // carries the chip and the overdue row's badge, same minute-
    // relative stamp class the board goldens accepted.
    let _ = add_open_entry(
        &mut state,
        board_one_task,
        "task one",
        now - TimeDelta::minutes(12),
    );
    (fixture, state, [board_one_task, board_two_task])
}

/// The triage fixture (chunk 10 D12): the global Inbox board ("Inbox",
/// one stack, one open task) plus the two-board journey state's board
/// two as the move target.
#[must_use]
pub fn triage_state() -> (TwoBoardFixture, AppState, TaskId) {
    let (fixture, mut state) = two_board_state();
    let inbox_board = BoardId::from_uuid(uuid::Uuid::from_u128(0x1003));
    let inbox_stack = StackId::from_uuid(uuid::Uuid::from_u128(0x2004));
    state.boards.insert(inbox_board, board(inbox_board));
    title_board(&mut state, inbox_board, "Inbox");
    state
        .stacks
        .insert(inbox_stack, stack(inbox_stack, inbox_board, "Inbox", 0));
    let caught = TaskId::from_uuid(uuid::Uuid::from_u128(0x3005));
    let mut t = task(caught, inbox_stack, 0);
    "caught thought".clone_into(&mut t.title);
    state.tasks.insert(caught, t);
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &[],
        &mut state.sync,
    );
    (fixture, state, caught)
}

fn board_fn(id: BoardId) -> Board {
    board(id)
}

fn title_board(state: &mut AppState, id: BoardId, title: &str) {
    title.clone_into(&mut state.boards.get_mut(&id).expect("board").title);
}

/// The empty board (round-3 R5/F10): one live board, zero stacks —
/// the leg the create-stack field must render in.
#[must_use]
pub fn empty_board_state() -> AppState {
    let (_fixture, mut state) = golden_state();
    let stacks: Vec<StackId> = state.stacks.keys().copied().collect();
    for stack in stacks {
        state.stacks.remove(&stack);
    }
    state.tasks.clear();
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &[],
        &mut state.sync,
    );
    state
}
