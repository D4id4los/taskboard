// SPDX-License-Identifier: AGPL-3.0-only
//! H-series: the `habits` verb family over hermetic standalone
//! tempdirs (no wiremock — a standalone core performs no network I/O).
//! One invocation is a full boot/work/shutdown "process"; durability
//! is proven across invocations on one db. Receipts are asserted as
//! typed values; refusals via `matches!`/`error_kind()`, never
//! message text.

use chrono::Datelike as _;
use clap::Parser;

use taskboard_app::cli::{Cli, Command};
use taskboard_app::commands::CommandError;
use taskboard_app::commands::boards;
use taskboard_app::commands::habits::{self, HabitsInvocation};
use taskboard_app::commands::output::{
    HabitAddReceipt, HabitAdjustReceipt, HabitDeleteReceipt, HabitListView, HabitLogReceipt,
    HabitUpdateReceipt,
};
use taskboard_app::config::AppConfig;

use crate::common;

use common::{forbidden_store, standalone_config, tempdir};

/// Parses one `taskboard habits …` argv through the real clap surface
/// and collapses it (the dispatch entry's own shape).
fn parse(argv: &[&str]) -> HabitsInvocation {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Habits { cmd }) = cli.command else {
        panic!("the argv selects the habits group");
    };
    habits::collapse(cmd)
}

/// Seeds the fresh db with its board (the single-live-board default
/// the verbs resolve against) and returns the config.
async fn seeded_config(dir: &std::path::Path, db: &str) -> AppConfig {
    let config = standalone_config(dir, db);
    boards::create(config.clone(), forbidden_store(), "seed board", None)
        .await
        .expect("board create boots");
    config
}

async fn run_one(
    config: &AppConfig,
    invocation: HabitsInvocation,
) -> Result<habits::HabitOutcome, CommandError> {
    habits::run(config.clone(), forbidden_store(), invocation).await
}

/// H-1: `add` mints, the receipt's row round-trips through `list`'s
/// projection, and two adds render in stored append order — the D-C13
/// canonical order at the CLI surface.
#[tokio::test]
async fn h01_add_receipts_round_trip_through_list_in_stored_order() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h01.db").await;
    let first = run_one(&config, parse(&["habits", "add", "water", "--daily", "8"]))
        .await
        .expect("add");
    let habits::HabitOutcome::Added(HabitAddReceipt { habit: Some(row) }) = &first else {
        panic!("the add receipt carries the row: {first:?}");
    };
    assert_eq!(row.title, "water");
    assert_eq!(row.target_phrase, "8/day");
    assert_eq!(row.quota, 8);
    assert!(!row.met);

    run_one(&config, parse(&["habits", "add", "run", "--weekly"]))
        .await
        .expect("second add");

    // Durability: a fresh boot lists both, in stored append order.
    let listed = run_one(&config, parse(&["habits", "list"]))
        .await
        .expect("list");
    let habits::HabitOutcome::Listed(HabitListView { habits }) = listed else {
        panic!("the list outcome");
    };
    assert_eq!(
        habits.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
        vec!["water", "run"],
        "append order is the stored (order, id) order"
    );
    assert_eq!(habits[0].target_phrase, "8/day");
    assert_eq!(habits[1].target_phrase, "1/week");
    assert_eq!(habits[0].count, 0, "nothing logged yet");
}

/// H-2: `log` +1s today's window; the receipt names the window and the
/// stored post-increment count (state truth, not an echo); a second
/// log lands 2. `adjust` corrects a canonical window across a boot.
#[tokio::test]
async fn h02_log_receipts_name_window_and_count_and_adjust_corrects() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h02.db").await;
    let added = run_one(&config, parse(&["habits", "add", "water", "--daily", "3"]))
        .await
        .expect("add");
    let habits::HabitOutcome::Added(HabitAddReceipt { habit: Some(row) }) = added else {
        panic!("add receipt");
    };
    let selector = row.habit_id[..8].to_owned();

    // The natural daily window is today on the resolved day convention
    // (D-F6: midnight rollover on the device's fixed UTC offset — the
    // same calendar `AppConfig::resolved_day_calendar` builds), not the
    // UTC calendar. The wall clock may roll a day between the bracket
    // reads, so the window must be one of the two adjacent todays —
    // both legitimate for a log landing inside the bracket.
    let day = taskboard_domain::DayCalendar {
        day_start_time: chrono::NaiveTime::MIN,
        offset: *chrono::Local::now().offset(),
    };
    let before = day.today(chrono::Utc::now());
    let logged = run_one(&config, parse(&["habits", "log", &selector]))
        .await
        .expect("log");
    let after = day.today(chrono::Utc::now());
    let habits::HabitOutcome::Logged(HabitLogReceipt {
        habit_id,
        window,
        count,
        reported,
    }) = &logged
    else {
        panic!("the log receipt: {logged:?}");
    };
    assert_eq!(habit_id, &row.habit_id);
    assert_eq!(*count, 1, "the post-increment stored count");
    assert_eq!(reported, count);
    assert!(
        *window == before || *window == after,
        "the natural daily window is the local-calendar today"
    );

    run_one(&config, parse(&["habits", "log", "water"]))
        .await
        .expect("second log by title");

    // The correction lands in the same window, across a fresh boot.
    let adjusted = run_one(
        &config,
        parse(&[
            "habits",
            "adjust",
            &selector,
            "--window",
            &window.to_string(),
            "--count",
            "0",
        ]),
    )
    .await
    .expect("adjust");
    let habits::HabitOutcome::Adjusted(HabitAdjustReceipt { count: stored, .. }) = &adjusted else {
        panic!("the adjust receipt: {adjusted:?}");
    };
    assert_eq!(*stored, 0, "the audited emptying stored");

    let listed = run_one(&config, parse(&["habits", "list"]))
        .await
        .expect("list");
    let habits::HabitOutcome::Listed(HabitListView { habits }) = listed else {
        panic!("list");
    };
    assert_eq!(habits[0].count, 0, "the projection reads the same row");
}

/// H-3: `adjust` on a non-canonical window is the typed
/// `invalid-habit-window` refusal (exit 1, error kind only), never a
/// clap refusal — this is a weekly habit and the window is a Wednesday.
#[tokio::test]
async fn h03_a_non_canonical_window_is_the_typed_refusal() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h03.db").await;
    run_one(
        &config,
        parse(&["habits", "add", "weekly thing", "--weekly"]),
    )
    .await
    .expect("add");
    let wednesday = chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let err = run_one(
        &config,
        parse(&[
            "habits",
            "adjust",
            "weekly thing",
            "--window",
            &wednesday.to_string(),
            "--count",
            "1",
        ]),
    )
    .await
    .expect_err("non-canonical window");
    assert_eq!(err.error_kind(), "invalid-habit-window");
}

/// H-4: `update --order` routes `MoveHabit` (the order clock moves,
/// the title clock does not — one law, the planner stamps), and the
/// zero-flag update is the typed no-op. `rm` tombstones and the
/// receipt names the surviving log rows, which stay queryable.
#[tokio::test]
async fn h04_update_routes_move_and_rm_keeps_the_log_queryable() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h04.db").await;
    run_one(&config, parse(&["habits", "add", "water", "--daily"]))
        .await
        .expect("add");
    run_one(&config, parse(&["habits", "add", "run", "--daily"]))
        .await
        .expect("add");
    run_one(&config, parse(&["habits", "log", "water"]))
        .await
        .expect("log");

    // The zero-flag update: the typed no-op.
    let noop = run_one(&config, parse(&["habits", "update", "water"]))
        .await
        .expect("zero-flag update");
    let habits::HabitOutcome::Updated(HabitUpdateReceipt { changed: false, .. }) = noop else {
        panic!("the zero-flag mask is the no-op: {noop:?}");
    };

    // A real change + a reorder in one verb call.
    let updated = run_one(
        &config,
        parse(&[
            "habits",
            "update",
            "water",
            "--title",
            "hydration",
            "--order",
            "9",
        ]),
    )
    .await
    .expect("update");
    let habits::HabitOutcome::Updated(HabitUpdateReceipt {
        changed: true,
        habit,
    }) = &updated
    else {
        panic!("the changed receipt: {updated:?}");
    };
    let habit_id = habit.as_ref().expect("the row").habit_id.clone();

    // Reopen and verify both the rename and the stored absolute order.
    let listed = run_one(&config, parse(&["habits", "list"]))
        .await
        .expect("list");
    let habits::HabitOutcome::Listed(HabitListView { habits }) = listed else {
        panic!("list");
    };
    assert_eq!(
        habits.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
        vec!["run", "hydration"],
        "the moved row re-sorts behind order 9"
    );

    // `rm` keeps the log queryable: the receipt names the row count.
    let removed = run_one(&config, parse(&["habits", "rm", "hydration"]))
        .await
        .expect("rm");
    let habits::HabitOutcome::Removed(HabitDeleteReceipt {
        deleted: true,
        log_rows,
        ..
    }) = &removed
    else {
        panic!("the rm receipt: {removed:?}");
    };
    assert_eq!(*log_rows, 1, "the tombstone never touches the log");

    // The tombstoned habit stops resolving and stops rendering.
    let err = run_one(&config, parse(&["habits", "log", "hydration"]))
        .await
        .expect_err("a tombstoned habit never resolves");
    assert_eq!(err.error_kind(), "unknown-habit");
    let listed = run_one(&config, parse(&["habits", "list"]))
        .await
        .expect("list");
    let habits::HabitOutcome::Listed(HabitListView { habits }) = listed else {
        panic!("list");
    };
    assert_eq!(habits.len(), 1, "the dead row never renders");
    let _ = habit_id;
}

/// H-5: an unknown selector is the typed miss; the list on an empty
/// board is the empty view, a success.
#[tokio::test]
async fn h05_unknown_selectors_miss_typedly_and_empty_lists_succeed() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h05.db").await;
    let listed = run_one(&config, parse(&["habits", "list"]))
        .await
        .expect("list");
    let habits::HabitOutcome::Listed(HabitListView { habits }) = listed else {
        panic!("list");
    };
    assert!(habits.is_empty(), "the empty section just has zero rows");
    let err = run_one(&config, parse(&["habits", "log", "ffffffff"]))
        .await
        .expect_err("typed miss");
    assert_eq!(err.error_kind(), "unknown-habit");
}

/// The habits render family's structural pins (#1229): one line per
/// listed habit, the empty family's own line, and the row branch
/// fields (the description column's presence law, the met suffix)
/// must surface.
#[test]
fn habits_render_arity_and_distinguishment() {
    use taskboard_app::commands::output::{HabitListView, HabitRow};

    let row = |title: &str, description: &str, met: bool| HabitRow {
        habit_id: format!(
            "00000000-0000-0000-0000-00000000000{}",
            if title == "a" { 1 } else { 2 }
        ),
        title: title.to_owned(),
        description: description.to_owned(),
        target_phrase: "8/day".to_owned(),
        count: 0,
        quota: 8,
        streak: 0,
        chip: None,
        met,
    };
    let listed = |habits: Vec<HabitRow>| {
        habits::render_text(&habits::HabitOutcome::Listed(HabitListView { habits }))
    };
    assert_eq!(
        listed(vec![row("a", "", false), row("b", "", false)])
            .split('\n')
            .count(),
        2,
        "one line per habit"
    );
    assert_eq!(
        listed(Vec::new()).split('\n').count(),
        1,
        "the empty family renders its own line"
    );
    let plain = listed(vec![row("a", "", false)]);
    assert_ne!(
        plain,
        listed(vec![row("a", "desc", false)]),
        "the description column's presence law must surface"
    );
    assert_ne!(
        plain,
        listed(vec![row("a", "", true)]),
        "the met suffix must surface"
    );

    // The single-line receipt arm: the row (or its absence) surfaces.
    let added_some = habits::render_text(&habits::HabitOutcome::Added(HabitAddReceipt {
        habit: Some(row("a", "", false)),
    }));
    let added_none = habits::render_text(&habits::HabitOutcome::Added(HabitAddReceipt {
        habit: None,
    }));
    assert_eq!(added_some.split('\n').count(), 1);
    assert_ne!(added_some, added_none, "the row-less arm must surface");
}

// -- H-series extension: the five subcommands (#1368, h06–h09) -------

use taskboard_app::commands::output::{
    HabitHistoryView, HabitPauseReceipt, HabitSkipReceipt, HabitStatsView, HabitWindowStateView,
};

/// The fixture's captured clock: every seeded window derives from this
/// one capture (the relative-oracle rule, §6.1 — never a hard-coded
/// calendar date; cf. #606/#680).
fn anchor() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// Kills: the Skip routing closure (the `collapse` arm and `run_on`'s
/// skip invocation). `habits skip` marks the current window typed (the
/// receipt names the window and the surviving streak), the history
/// projection classifies that window `Skipped`, and an unknown
/// selector misses typed — never a panic.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one seeded grid, one assertion per arm
async fn h06_skip_marks_the_current_window_and_misses_typed_on_an_unknown_selector() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h06.db").await;
    run_one(&config, parse(&["habits", "add", "water", "--daily", "3"]))
        .await
        .expect("add");
    // A met yesterday (quota reached) gives the streak a run to
    // survive; R-4 says the typed planned skip keeps it alive.
    let now0 = anchor();
    let yesterday = (now0 - chrono::Duration::hours(24)).date_naive();
    let yesterday_text = yesterday.format("%Y-%m-%d").to_string();
    run_one(
        &config,
        parse(&[
            "habits",
            "adjust",
            "--window",
            &yesterday_text,
            "--count",
            "3",
            "water",
        ]),
    )
    .await
    .expect("met seed");
    // A row must exist for the window to render (the horizon walks the
    // log): one logged unit keeps the window under quota.
    run_one(&config, parse(&["habits", "log", "water"]))
        .await
        .expect("log");

    // Before the skip: the current window is under quota with grace
    // open — the walk's Pending arm.
    let history = run_one(&config, parse(&["habits", "history", "water"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = history else {
        panic!("history outcome");
    };
    let latest = windows.last().expect("the current window");
    assert_eq!(latest.state, HabitWindowStateView::Pending);
    assert_eq!(latest.count, 1, "the logged unit renders");

    let skipped = run_one(&config, parse(&["habits", "skip", "water"]))
        .await
        .expect("skip");
    let habits::HabitOutcome::Skipped(HabitSkipReceipt {
        habit_id,
        window,
        streak,
    }) = skipped
    else {
        panic!("the skip receipt: {skipped:?}");
    };
    assert!(uuid::Uuid::parse_str(&habit_id).is_ok(), "durable uuid");
    let now = anchor();
    assert!(
        window.num_days_from_ce() == now.date_naive().num_days_from_ce()
            || window.num_days_from_ce()
                == (now - chrono::Duration::hours(24))
                    .date_naive()
                    .num_days_from_ce(),
        "the marked window is the running day's window: {window}"
    );
    assert!(streak >= 1, "R-4: the planned skip survives the streak");

    // The history projection classifies the marked window Skipped.
    let history = run_one(&config, parse(&["habits", "history", "water"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = history else {
        panic!("history outcome");
    };
    let marked = windows
        .iter()
        .find(|row| row.window == window)
        .expect("the marked window renders");
    assert_eq!(marked.state, HabitWindowStateView::Skipped);

    // The typed miss.
    let err = run_one(&config, parse(&["habits", "skip", "ffffffff"]))
        .await
        .expect_err("typed miss");
    assert_eq!(err.error_kind(), "unknown-habit");
}

/// Kills: the Pause/Resume routing closures and the
/// `pause_covers_window` composition order (the walk's Paused arm
/// wins over every other classification while covered). The pause
/// receipt carries the stored through-date; resume clears it typed
/// (the badge-hygiene posture); a past through-date refuses typed.
#[tokio::test]
async fn h07_pause_covers_and_resume_clears_the_pause() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h07.db").await;
    run_one(&config, parse(&["habits", "add", "water", "--daily", "3"]))
        .await
        .expect("add");
    run_one(&config, parse(&["habits", "log", "water"]))
        .await
        .expect("log");

    let now = anchor();
    let until = (now + chrono::Duration::hours(48)).date_naive();
    let until_text = until.format("%Y-%m-%d").to_string();
    let paused = run_one(
        &config,
        parse(&["habits", "pause", "water", "--until", &until_text]),
    )
    .await
    .expect("pause");
    let habits::HabitOutcome::Paused(HabitPauseReceipt { until: stored, .. }) = paused else {
        panic!("the pause receipt: {paused:?}");
    };
    assert_eq!(stored, Some(until), "the through-date is stored verbatim");

    // The covered current window classifies Paused — the composing
    // arm's precedence over Pending.
    let history = run_one(&config, parse(&["habits", "history", "water"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = history else {
        panic!("history outcome");
    };
    assert_eq!(
        windows.last().expect("the current window").state,
        HabitWindowStateView::Paused,
        "the pause covers the current window"
    );

    let resumed = run_one(&config, parse(&["habits", "resume", "water"]))
        .await
        .expect("resume");
    let habits::HabitOutcome::Paused(HabitPauseReceipt { until: cleared, .. }) = resumed else {
        panic!("the resume receipt: {resumed:?}");
    };
    assert_eq!(cleared, None, "resume clears the pause typed");

    // After the resume the same window reads Pending again (under
    // quota, grace open — the pause's classification is gone).
    let history = run_one(&config, parse(&["habits", "history", "water"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = history else {
        panic!("history outcome");
    };
    assert_eq!(
        windows.last().expect("the current window").state,
        HabitWindowStateView::Pending,
        "the resumed window is pending again"
    );

    // A past through-date is the planner's typed refusal.
    let past = (now - chrono::Duration::hours(24)).date_naive();
    let past_text = past.format("%Y-%m-%d").to_string();
    let err = run_one(
        &config,
        parse(&["habits", "pause", "water", "--until", &past_text]),
    )
    .await
    .expect_err("the past-date pause refuses");
    assert_eq!(err.error_kind(), "invalid-habit-pause");
}

/// Kills: the History routing closure and each `window_state_view`
/// arm's inversion. The seed spreads the grid vocabulary across two
/// habits, all windows anchored relative to one captured `now` (§6.1):
/// Met (quota reached on a past window), Missed (past window under
/// quota, grace closed), Skipped (`h06`'s verb), Paused (the covered
/// current window), `PreSchedule` (a weekly habit whose only scheduled
/// day is tomorrow — first scheduled day strictly after today), and
/// Pending (the fresh current window). Chronological order pins the
/// rendering order.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one seeded grid, one assertion per arm
async fn h08_history_renders_the_grid_vocabulary_chronologically() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h08.db").await;
    let now = anchor();
    let today = now.date_naive();

    run_one(&config, parse(&["habits", "add", "water", "--daily", "3"]))
        .await
        .expect("add water");
    // Met: the yesterday window reached its quota.
    let met_window = (now - chrono::Duration::hours(24)).date_naive();
    let met_text = met_window.format("%Y-%m-%d").to_string();
    run_one(
        &config,
        parse(&[
            "habits", "adjust", "--window", &met_text, "--count", "3", "water",
        ]),
    )
    .await
    .expect("met seed");
    // Missed: the day-before-yesterday window stayed empty and its
    // grace closed (any now inside today's UTC day is past its grace
    // end).
    let missed_window = (now - chrono::Duration::hours(48)).date_naive();
    let missed_text = missed_window.format("%Y-%m-%d").to_string();
    run_one(
        &config,
        parse(&[
            "habits",
            "adjust",
            "--window",
            &missed_text,
            "--count",
            "0",
            "water",
        ]),
    )
    .await
    .expect("missed seed");
    // Skipped: the current window's typed planned skip (a logged row
    // must exist for the window to render, and it stays under quota).
    run_one(&config, parse(&["habits", "log", "water"]))
        .await
        .expect("current-window row");
    run_one(&config, parse(&["habits", "skip", "water"]))
        .await
        .expect("skip seed");

    // Paused: a second daily habit paused through the far side of the
    // running week; a logged row gives the window a render, and it
    // stays under quota so the Paused arm (not Met) classifies.
    run_one(
        &config,
        parse(&["habits", "add", "stretch", "--daily", "3"]),
    )
    .await
    .expect("add stretch");
    run_one(&config, parse(&["habits", "log", "stretch"]))
        .await
        .expect("stretch row");
    let until = (now + chrono::Duration::hours(96)).date_naive();
    let until_text = until.format("%Y-%m-%d").to_string();
    run_one(
        &config,
        parse(&["habits", "pause", "stretch", "--until", &until_text]),
    )
    .await
    .expect("pause seed");

    // PreSchedule: a weekly habit whose only scheduled day is
    // tomorrow's weekday — its current window's first scheduled day is
    // strictly after today, whatever day the wall clock says.
    run_one(&config, parse(&["habits", "add", "run", "--weekly", "2"]))
        .await
        .expect("add run");
    // A row (under quota) so the window renders at all.
    run_one(&config, parse(&["habits", "log", "run"]))
        .await
        .expect("run row");
    let tomorrow = (now + chrono::Duration::hours(24)).date_naive();
    let day_name = match tomorrow.weekday() {
        chrono::Weekday::Mon => "mon",
        chrono::Weekday::Tue => "tue",
        chrono::Weekday::Wed => "wed",
        chrono::Weekday::Thu => "thu",
        chrono::Weekday::Fri => "fri",
        chrono::Weekday::Sat => "sat",
        chrono::Weekday::Sun => "sun",
    };
    run_one(
        &config,
        parse(&["habits", "update", "run", "--days", day_name]),
    )
    .await
    .expect("schedule run");

    // The grid: water's windows chronological, one row per seeded arm.
    let history = run_one(&config, parse(&["habits", "history", "water"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = history else {
        panic!("history outcome");
    };
    let mut sorted = windows.clone();
    sorted.sort_by_key(|row| row.window);
    assert_eq!(
        windows.iter().map(|row| row.window).collect::<Vec<_>>(),
        sorted.iter().map(|row| row.window).collect::<Vec<_>>(),
        "chronological order (HF3)"
    );
    let state_of = |date: chrono::NaiveDate| -> HabitWindowStateView {
        windows
            .iter()
            .find(|row| row.window == date)
            .unwrap_or_else(|| panic!("window {date} renders"))
            .state
    };
    assert_eq!(state_of(missed_window), HabitWindowStateView::Missed);
    assert_eq!(state_of(met_window), HabitWindowStateView::Met);
    assert_eq!(
        windows.last().expect("the current window").state,
        HabitWindowStateView::Skipped,
        "the skipped current window"
    );
    assert!(
        windows.len() >= 3,
        "the three seeded windows render: {}",
        windows.len()
    );
    assert_eq!(
        windows
            .iter()
            .find(|row| row.window == met_window)
            .expect("met row")
            .count,
        3,
        "the Met row carries the adjusted count"
    );

    // Paused beats Pending on the covered stretch's current window.
    let paused_history = run_one(&config, parse(&["habits", "history", "stretch"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = paused_history else {
        panic!("history outcome");
    };
    assert_eq!(
        windows.last().expect("the current window").state,
        HabitWindowStateView::Paused,
    );

    // PreSchedule: tomorrow's only scheduled day is strictly after
    // today, so the current weekly window is pre-schedule.
    let preschedule = run_one(&config, parse(&["habits", "history", "run"]))
        .await
        .expect("history");
    let habits::HabitOutcome::History(HabitHistoryView { windows, .. }) = preschedule else {
        panic!("history outcome");
    };
    assert_eq!(
        windows.last().expect("the current window").state,
        HabitWindowStateView::PreSchedule,
        "the weekly window whose first scheduled day is tomorrow"
    );
    let _ = today;
}

/// Kills: the Stats routing closure (a Stats→History reroute returns
/// the wrong outcome variant) and the typed stats projection. The
/// daily habit has no weekly/monthly day set — the adherence
/// denominator's join law makes it the typed empty state, while the
/// per-window totals render the seeded log rows and the streak reads
/// back nonzero.
#[tokio::test]
async fn h09_stats_projects_the_typed_view_on_a_seeded_series() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "h09.db").await;
    let now = anchor();
    run_one(&config, parse(&["habits", "add", "water", "--daily", "1"]))
        .await
        .expect("add");
    let met_window = (now - chrono::Duration::hours(24)).date_naive();
    let met_text = met_window.format("%Y-%m-%d").to_string();
    run_one(
        &config,
        parse(&[
            "habits", "adjust", "--window", &met_text, "--count", "1", "water",
        ]),
    )
    .await
    .expect("met seed");
    run_one(&config, parse(&["habits", "skip", "water"]))
        .await
        .expect("skip seed");

    let stats = run_one(&config, parse(&["habits", "stats", "water"]))
        .await
        .expect("stats");
    let habits::HabitOutcome::Stats(HabitStatsView {
        habit_id,
        title,
        adherence_empty,
        adherence_met,
        adherence_scheduled,
        best_streak,
        totals,
    }) = stats
    else {
        panic!("the stats outcome: {stats:?}");
    };
    assert!(uuid::Uuid::parse_str(&habit_id).is_ok(), "durable uuid");
    assert_eq!(title, "water");
    assert!(
        !adherence_empty,
        "the seeded horizon is non-empty, so the ratio exists"
    );
    assert_eq!(adherence_met, 1, "the met window joins the numerator");
    assert_eq!(
        adherence_scheduled, 1,
        "the met window joins the denominator; the skipped one is transparent"
    );
    assert!(best_streak >= 1, "the met window is a real run");
    assert!(
        totals
            .iter()
            .any(|(window, count)| *window == met_window && *count == 1),
        "the met window's total renders: {totals:?}"
    );
}
