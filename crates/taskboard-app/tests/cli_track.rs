// SPDX-License-Identifier: AGPL-3.0-only
//! T-series: the `track` verb family and the `export` verb over
//! hermetic standalone tempdirs (no wiremock — a standalone core
//! performs no network I/O). One invocation is a full boot/work/
//! shutdown "process"; durability is proven across invocations on one
//! db. Receipts are asserted as typed values; refusals via
//! `matches!`/`error_kind()`, never message text. Byte goldens live
//! over fixed-stamp `log` rows only — boot-stamped entries are
//! asserted structurally, never against the wall clock.

use std::sync::Arc;

use clap::Parser;

use taskboard_app::cli::{Cli, Command, ExportFormat, TrackCmd};
use taskboard_app::commands::output::{
    TrackAdjustReceipt, TrackDeleteReceipt, TrackListView, TrackLogReceipt, TrackStartReceipt,
    TrackStatusView, TrackStopReceipt,
};
use taskboard_app::commands::tasks;
use taskboard_app::commands::track::{self, ExportQuery, TrackInvocation};
use taskboard_app::commands::{BasicSession, CommandError, CommandIo, dispatch_line};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::CredentialStore;

mod common;

use common::tempdir;

/// A standalone config over a tempdir db: no server, no username — the
/// fresh-install shape.
fn standalone_config(dir: &std::path::Path, db: &str) -> AppConfig {
    AppConfig {
        nextcloud: taskboard_app::config::NextcloudSection {
            server_url: None,
            username: None,
            credential_store: CredentialStoreKind::Keyring,
        },
        storage: taskboard_app::config::StorageSection {
            db_path: Some(dir.join(db)),
        },
        ..AppConfig::default()
    }
}

/// A store that would fail any touch: standalone never consults it.
fn forbidden_store() -> Arc<dyn CredentialStore> {
    Arc::new(common::FakeCredentialStore::default())
}

/// A fixed, non-midnight day calendar — explicit offset, host-TZ
/// independent by construction (the D-F6 consumer contract).
fn day_calendar() -> taskboard_domain::DayCalendar {
    taskboard_domain::DayCalendar {
        day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("05:00"),
        offset: chrono::FixedOffset::east_opt(2 * 3_600).expect("+02:00"),
    }
}

/// Parses one `taskboard track …` argv through the real clap surface
/// and collapses it (the dispatch entry's own shape).
fn parse_track(argv: &[&str]) -> TrackInvocation {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Track { cmd }) = cli.command else {
        panic!("the argv selects the track group");
    };
    track::collapse(cmd).expect("one invocation")
}

/// Parses one `taskboard export …` argv into the query bag.
fn parse_export(argv: &[&str]) -> ExportQuery {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Export {
        format,
        since,
        until,
        task,
        board,
    }) = cli.command
    else {
        panic!("the argv selects export");
    };
    ExportQuery {
        format,
        since,
        until,
        task,
        board,
    }
}

/// The dbs the seed scaffold already ran on (tests share no db, but
/// `seed_task` may run several times against one).
fn seeded_dbs() -> &'static std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>> =
        std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Seeds one live task through a real boot: a fresh db gets its seed
/// board + stack first (`tasks add` targets a stack, which needs a
/// board), then the task — the single-board fresh shape. Returns the
/// task's uuid string.
async fn seed_task(config: &AppConfig, title: &str) -> String {
    let db = config.db_path().to_path_buf();
    if seeded_dbs().lock().expect("seed map").insert(db) {
        taskboard_app::commands::boards::create(
            config.clone(),
            forbidden_store(),
            "seed board",
            None,
        )
        .await
        .expect("board create boots");
        taskboard_app::commands::stacks::add(
            config.clone(),
            forbidden_store(),
            None,
            "seed stack",
            None,
        )
        .await
        .expect("stack add boots");
    }
    let receipt = tasks::add(config.clone(), forbidden_store(), title, Some("seed stack"))
        .await
        .expect("task add boots");
    receipt.id
}

/// Seeds a distinctly-titled stack on a named board and one task on it
/// (the multi-board shape: `--stack` resolves the stack by title).
async fn seed_task_on_board(config: &AppConfig, title: &str, board_id: &str) -> String {
    let board = taskboard_domain::BoardId::from_uuid(
        uuid::Uuid::parse_str(board_id).expect("the receipt uuid parses"),
    );
    let stack_title = format!("{title} stack");
    taskboard_app::commands::stacks::add(
        config.clone(),
        forbidden_store(),
        Some(board),
        &stack_title,
        None,
    )
    .await
    .expect("stack add boots");
    let receipt = tasks::add(config.clone(), forbidden_store(), title, Some(&stack_title))
        .await
        .expect("task add boots");
    receipt.id
}

async fn run_start(config: &AppConfig, task: &str) -> TrackStartReceipt {
    match parse_track(&["track", "start", task]) {
        TrackInvocation::Start { task } => track::start(config.clone(), forbidden_store(), &task)
            .await
            .expect("start boots"),
        _ => panic!("the argv selects start"),
    }
}

async fn run_stop(config: &AppConfig) -> TrackStopReceipt {
    match parse_track(&["track", "stop"]) {
        TrackInvocation::Stop => track::stop(config.clone(), forbidden_store())
            .await
            .expect("stop boots"),
        _ => panic!("the argv selects stop"),
    }
}

async fn run_log(config: &AppConfig, argv: &[&str]) -> Result<TrackLogReceipt, CommandError> {
    let mut cmdline = vec!["track", "log"];
    cmdline.extend_from_slice(argv);
    match parse_track(&cmdline) {
        TrackInvocation::Log { task, mode } => {
            track::log(config.clone(), forbidden_store(), &task, mode).await
        }
        _ => panic!("the argv selects log"),
    }
}

async fn run_adjust(config: &AppConfig, argv: &[&str]) -> Result<TrackAdjustReceipt, CommandError> {
    let mut cmdline = vec!["track", "adjust"];
    cmdline.extend_from_slice(argv);
    match parse_track(&cmdline) {
        TrackInvocation::Adjust { entry, start, end } => {
            track::adjust(config.clone(), forbidden_store(), &entry, start, end).await
        }
        _ => panic!("the argv selects adjust"),
    }
}

async fn run_delete(config: &AppConfig, selector: &str) -> TrackDeleteReceipt {
    match parse_track(&["track", "delete", selector]) {
        TrackInvocation::Delete { entry } => {
            track::delete(config.clone(), forbidden_store(), &entry)
                .await
                .expect("delete boots")
        }
        _ => panic!("the argv selects delete"),
    }
}

async fn run_status(config: &AppConfig, day: taskboard_domain::DayCalendar) -> TrackStatusView {
    match parse_track(&["track", "status"]) {
        TrackInvocation::Status => track::status(config.clone(), forbidden_store(), day)
            .await
            .expect("status boots"),
        _ => panic!("the argv selects status"),
    }
}

async fn run_list(config: &AppConfig, argv: &[&str]) -> TrackListView {
    let mut cmdline = vec!["track", "list"];
    cmdline.extend_from_slice(argv);
    match parse_track(&cmdline) {
        TrackInvocation::List { task, since } => {
            track::list(config.clone(), forbidden_store(), task.as_deref(), since)
                .await
                .expect("list boots")
        }
        _ => panic!("the argv selects list"),
    }
}

async fn run_export(config: &AppConfig, query: &ExportQuery) -> String {
    track::export(config.clone(), forbidden_store(), query.clone())
        .await
        .map(|payload| track::render_payload(&payload))
        .expect("export boots")
}

/// T1: `track start` stamps an open entry (state truth: `start` sits
/// in `[before, now]`), and the entry survives a fresh boot — the
/// crash-resume leg through the CLI (ADR 0017's open-row-is-the-intent,
/// pinned at the surface).
#[tokio::test]
async fn t1_start_stamps_an_open_entry_that_survives_a_fresh_boot() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t1.db");
    let title = seed_task(&config, "brew tea").await;
    let before = chrono::Utc::now();
    let receipt = run_start(&config, "brew tea").await;
    let now = chrono::Utc::now();
    assert!(receipt.started, "the fresh start lands");
    assert_eq!(receipt.entry.task_id, title);
    assert_eq!(receipt.entry.task_title, "brew tea", "the title snapshot");
    assert!(receipt.entry.end.is_none(), "the fresh entry is open");
    assert!(receipt.entry.start >= before && receipt.entry.start <= now);
    assert!(receipt.closed.is_empty(), "nothing was running before");

    // Fresh boot: the open entry is still there, and its elapsed time
    // can only have grown — nothing ticks, the duration is derived at
    // read (no fixed sleep: the monotone bound is the assertion).
    let view = run_status(&config, day_calendar()).await;
    assert_eq!(view.open.len(), 1, "durable across a fresh boot");
    assert_eq!(view.open[0].entry_id, receipt.entry.entry_id);
    assert!(
        view.open[0].duration_seconds >= receipt.entry.duration_seconds,
        "elapsed is derived at read and accrues"
    );
}

/// T2: starting B closes A — the receipt's `closed` names A's entry id
/// and exactly one open entry remains.
#[tokio::test]
async fn t2_starting_b_closes_a() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t2.db");
    seed_task(&config, "task a").await;
    seed_task(&config, "task b").await;
    let first = run_start(&config, "task a").await;
    let second = run_start(&config, "task b").await;
    assert!(second.started);
    assert_eq!(
        second.closed,
        vec![first.entry.entry_id],
        "the start closed the previously running entry"
    );
    let view = run_status(&config, day_calendar()).await;
    assert_eq!(view.open.len(), 1);
    assert_eq!(view.open[0].entry_id, second.entry.entry_id);
}

/// T3: the idempotent repeat — starting the already-tracked task again
/// yields `started: false`, the same entry id, empty `closed`.
#[tokio::test]
async fn t3_the_idempotent_start_repeat_is_a_typed_no_op() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t3.db");
    seed_task(&config, "repeat me").await;
    let first = run_start(&config, "repeat me").await;
    let repeat = run_start(&config, "repeat me").await;
    assert!(!repeat.started, "the repeat is the no-op");
    assert_eq!(repeat.entry.entry_id, first.entry.entry_id);
    assert_eq!(repeat.closed.len(), 0);
}

/// T4: `track stop` closes (the receipt carries the stamped entries);
/// the repeat yields `stopped: false` with no entries.
#[tokio::test]
async fn t4_stop_closes_and_the_repeat_is_the_no_op() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t4.db");
    seed_task(&config, "stop me").await;
    let started = run_start(&config, "stop me").await;
    let stop = run_stop(&config).await;
    assert!(stop.stopped);
    assert_eq!(stop.entries.len(), 1);
    assert_eq!(stop.entries[0].entry_id, started.entry.entry_id);
    assert!(stop.entries[0].end.is_some(), "the stamped row is closed");

    let repeat = run_stop(&config).await;
    assert!(!repeat.stopped, "nothing open, nothing changed");
    assert_eq!(repeat.entries.len(), 0);
}

/// T5: `track log` both modes land closed entries (`--for 90m`;
/// `--from/--to` fixed stamps — the receipt carries the exact span);
/// an inverted span is the typed `invalid-span` failure (the exit 1
/// class), never a boot-time refusal; a zero `--for` is the same kind.
#[tokio::test]
async fn t5_log_lands_both_modes_and_inverted_spans_refuse_typed() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t5.db");
    seed_task(&config, "logged work").await;

    let for_receipt = run_log(&config, &["logged work", "--for", "90m"])
        .await
        .expect("--for logs");
    let row = &for_receipt.entry;
    assert!(row.end.is_some(), "a logged entry is closed");
    assert_eq!(
        row.end.unwrap() - row.start,
        chrono::TimeDelta::try_minutes(90).unwrap(),
        "the receipt carries the exact --for span"
    );

    let from = chrono::Utc::now() - chrono::TimeDelta::try_hours(5).unwrap();
    let to = from + chrono::TimeDelta::try_hours(1).unwrap();
    let from_stamp = from.to_rfc3339();
    let to_stamp = to.to_rfc3339();
    let span_receipt = run_log(
        &config,
        &["logged work", "--from", &from_stamp, "--to", &to_stamp],
    )
    .await
    .expect("--from/--to logs");
    assert_eq!(span_receipt.entry.start, from, "fixed stamps are verbatim");
    assert_eq!(span_receipt.entry.end, Some(to));

    // Inverted span: the planner's typed rejection, after the boot.
    let err = run_log(
        &config,
        &["logged work", "--from", &to_stamp, "--to", &from_stamp],
    )
    .await
    .expect_err("an inverted span is refused");
    assert_eq!(err.error_kind(), "invalid-span");

    // A zero `--for` is the same typed invalid-span (D-TC8).
    let err = run_log(&config, &["logged work", "--for", "0s"])
        .await
        .expect_err("a zero --for is refused");
    assert_eq!(err.error_kind(), "invalid-span");

    let rows = run_list(&config, &[]).await;
    assert_eq!(rows.entries.len(), 2, "only the valid logs landed");
}

/// T6: `track adjust` on an open entry with `--end` closes it
/// (`changed: true`, the end set in post-state); a zero-flag adjust is
/// the typed no-op; `--start` moves a closed span; a prefix selector
/// resolves to the same row; a miss is the typed unknown-entry.
#[tokio::test]
async fn t6_adjust_closes_moves_and_no_ops_typedly() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t6.db");
    seed_task(&config, "adjust me").await;
    let started = run_start(&config, "adjust me").await;
    let entry_id = started.entry.entry_id.clone();

    // Zero-flag adjust: the typed no-op carrying the current row.
    let noop = run_adjust(&config, &[&entry_id])
        .await
        .expect("the zero-flag adjust executes");
    assert!(!noop.changed);
    assert_eq!(noop.entry.entry_id, entry_id);
    assert!(noop.entry.end.is_none(), "still open");

    // `--end` closes the open entry (a history edit can never reopen).
    let end = chrono::Utc::now() + chrono::TimeDelta::try_minutes(1).unwrap();
    let end_stamp = end.to_rfc3339();
    let closed = run_adjust(&config, &[&entry_id, "--end", &end_stamp])
        .await
        .expect("the --end adjust executes");
    assert!(closed.changed);
    assert_eq!(closed.entry.end, Some(end), "the end is state truth");

    // `--start` moves the closed span.
    let start = chrono::Utc::now() - chrono::TimeDelta::try_hours(3).unwrap();
    let start_stamp = start.to_rfc3339();
    let moved = run_adjust(&config, &[&entry_id, "--start", &start_stamp])
        .await
        .expect("the --start adjust executes");
    assert!(moved.changed);
    assert_eq!(moved.entry.start, start);
    assert_eq!(moved.entry.end, Some(end), "the end survived the mask");

    // An unknown entry selector is the typed miss.
    let err = run_adjust(&config, &["ffffffff", "--end", &end_stamp])
        .await
        .expect_err("no such entry");
    assert!(matches!(err, CommandError::UnknownTimeEntry { .. }));
    assert_eq!(err.error_kind(), "unknown-time-entry");
}

/// T7: `track delete` removes (`deleted: true`); the full-uuid repeat
/// is `deleted: false` — the honest no-op (D-TC9), deliberately *not*
/// terminal-by-deletion.
#[tokio::test]
async fn t7_delete_removes_and_the_repeat_is_the_honest_no_op() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t7.db");
    seed_task(&config, "log then delete").await;
    let logged = run_log(&config, &["log then delete", "--for", "30m"])
        .await
        .expect("logs");
    let entry_id = logged.entry.entry_id.clone();

    let deleted = run_delete(&config, &entry_id).await;
    assert!(deleted.deleted);
    assert_eq!(deleted.entry_id, entry_id);

    // The full-uuid repeat: absent is the accepted no-op.
    let repeat = run_delete(&config, &entry_id).await;
    assert!(!repeat.deleted, "absent is the accepted no-op");
    assert_eq!(repeat.entry_id, entry_id);

    let rows = run_list(&config, &[]).await;
    assert!(rows.entries.is_empty(), "the row is gone");
}

/// T8: entry selectors through the boot lane — an ≥8-char prefix
/// resolves to the same row the full uuid names; a miss is the typed
/// `unknown-time-entry` carrying the selector.
#[tokio::test]
async fn t8_entry_selectors_resolve_prefix_and_miss_typed() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t8.db");
    seed_task(&config, "selector target").await;
    let logged = run_log(&config, &["selector target", "--for", "15m"])
        .await
        .expect("logs");
    let entry_id = logged.entry.entry_id.clone();
    let prefix = entry_id[..8].to_owned();

    let by_prefix = run_adjust(
        &config,
        &[&prefix, "--end", &chrono::Utc::now().to_rfc3339()],
    )
    .await
    .expect("the prefix resolves");
    assert_eq!(by_prefix.entry.entry_id, entry_id);

    let err = run_adjust(&config, &["ffffffff", "--start", "2026-01-01T00:00:00Z"])
        .await
        .expect_err("a miss");
    let CommandError::UnknownTimeEntry { selector } = err else {
        panic!("the miss is typed");
    };
    assert_eq!(selector, "ffffffff");
}

/// T9: `track list` — canonical `(start, id)` order; `--task` filters
/// (the other task's entries vanish); `--since` cuts by start.
#[tokio::test]
async fn t9_list_orders_and_filters() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t9.db");
    seed_task(&config, "alpha").await;
    seed_task(&config, "beta").await;

    // Three fixed-stamp closed entries on distinct UTC days, oldest
    // first: two for alpha, one for beta.
    let stamps = [
        (
            "2026-01-01T08:00:00+00:00",
            "2026-01-01T09:00:00+00:00",
            "alpha",
        ),
        (
            "2026-01-02T08:00:00+00:00",
            "2026-01-02T09:00:00+00:00",
            "beta",
        ),
        (
            "2026-01-03T08:00:00+00:00",
            "2026-01-03T09:00:00+00:00",
            "alpha",
        ),
    ];
    for (from, to, task) in stamps {
        run_log(&config, &[task, "--from", from, "--to", to])
            .await
            .expect("logs");
    }

    let rows = run_list(&config, &[]).await;
    let starts: Vec<chrono::DateTime<chrono::Utc>> = rows.entries.iter().map(|r| r.start).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted, "canonical (start, id) ascending");

    let only_alpha = run_list(&config, &["--task", "alpha"]).await;
    assert_eq!(only_alpha.entries.len(), 2, "the other task's rows vanish");
    assert!(only_alpha.entries.iter().all(|r| r.task_title == "alpha"));

    // `--since` keeps entries starting on/after that day's midnight UTC.
    let since = run_list(&config, &["--since", "2026-01-02"]).await;
    assert_eq!(since.entries.len(), 2);
    assert!(since.entries.iter().all(|r| {
        r.start
            >= chrono::DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
    }));
}

/// T10: `track status` over a constructed day — a closed entry spanning
/// the day-window boundary contributes only its in-window part, the
/// open entry accrues to now, and the open set is listed newest-first.
/// The exact split-at-the-boundary sum is pinned at the pure layer
/// (`commands::track::tests::status_view_…`, fixed `now`); this
/// boot-tier leg pins the calendar-as-data routing through a real boot
/// with a structural floor on the total.
#[tokio::test]
async fn t10_status_over_a_constructed_day_window() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t10.db");
    seed_task(&config, "day window work").await;

    // The current window under this calendar — computed at test time
    // so the seeded stamps straddle its start.
    let day = day_calendar();
    let now = chrono::Utc::now();
    let (window_start, window_end) =
        taskboard_domain::day_window(now, day.day_start_time, day.offset);
    let step = chrono::TimeDelta::try_minutes(30).unwrap();
    let format = |t: chrono::DateTime<chrono::Utc>| t.to_rfc3339();

    // A closed entry spanning the window start: only its in-window
    // segment may count.
    let from = format(window_start - step);
    let to = format(window_start + step);
    run_log(&config, &["day window work", "--from", &from, "--to", &to])
        .await
        .expect("the spanning log lands");
    // A closed entry fully inside the window: its whole hour counts.
    let from = format(window_start + chrono::TimeDelta::try_hours(1).unwrap());
    let to = format(window_start + chrono::TimeDelta::try_hours(2).unwrap());
    run_log(&config, &["day window work", "--from", &from, "--to", &to])
        .await
        .expect("the inside log lands");
    // A closed entry entirely before the window: contributes nothing.
    let from = format(window_start - chrono::TimeDelta::try_hours(5).unwrap());
    let to = format(window_start - chrono::TimeDelta::try_hours(4).unwrap());
    run_log(&config, &["day window work", "--from", &from, "--to", &to])
        .await
        .expect("the outside log lands");
    // An open entry started inside the window: accrues to now.
    let started = run_start(&config, "day window work").await;
    assert!(
        started.entry.start > window_start && started.entry.start < window_end,
        "the open entry starts inside the window"
    );

    let view = run_status(&config, day).await;
    assert_eq!(view.open.len(), 1);
    assert_eq!(view.open[0].entry_id, started.entry.entry_id);
    // Floor: the fully-inside hour is there, plus the spanning entry's
    // 30-minute in-window segment, plus the open accrual (≥ 0). The
    // exact boundary split is the pure layer's pinned sum.
    assert!(
        view.tracked_today_seconds >= 5_400,
        "the inside hour + the spanning segment are both counted"
    );
}

/// T11: the session lane — every track verb and export route over the
/// `*_on` seams on one booted session (the timers-T7 shape): JSON
/// payload lines on stdout, the failure's typed envelope on stderr.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one row per routed verb; the list IS the routing pin
async fn t11_the_session_lane_routes_track_and_export() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t11.db");
    let store = forbidden_store();
    seed_task(&config, "session work").await;
    let app = taskboard_app::bootstrap::bootstrap(
        config.clone(),
        store.clone(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session boots");
    let session = BasicSession {
        config: config.clone(),
        store,
        config_path: None,
    };

    let mut out = Vec::new();
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let cases: Vec<(Command, u8)> = vec![
            (
                Command::Track {
                    cmd: TrackCmd::Start {
                        task: "session work".into(),
                    },
                },
                0,
            ),
            (
                Command::Track {
                    cmd: TrackCmd::Status,
                },
                0,
            ),
            (
                Command::Track {
                    cmd: TrackCmd::List {
                        task: None,
                        since: None,
                    },
                },
                0,
            ),
            (
                Command::Track {
                    cmd: TrackCmd::Stop,
                },
                0,
            ),
            (
                Command::Track {
                    cmd: TrackCmd::Log {
                        task: "session work".into(),
                        for_: Some(chrono::TimeDelta::try_minutes(20).unwrap()),
                        from: None,
                        to: None,
                    },
                },
                0,
            ),
            (
                Command::Track {
                    cmd: TrackCmd::Delete {
                        // A well-formed uuid naming nothing: the honest
                        // no-op rides exit 0 (D-TC9).
                        entry: "00000000-0000-0000-0000-00000000ffff".into(),
                    },
                },
                0,
            ),
            (
                Command::Track {
                    cmd: TrackCmd::Adjust {
                        entry: "ffffffff".into(),
                        start: None,
                        end: None,
                    },
                },
                // The unknown selector refuses: the typed exit code.
                1,
            ),
            (
                Command::Export {
                    format: Some(ExportFormat::Json),
                    since: None,
                    until: None,
                    task: None,
                    board: None,
                },
                0,
            ),
            (
                Command::Export {
                    format: None,
                    since: None,
                    until: None,
                    task: Some("no such task".into()),
                    board: None,
                },
                1,
            ),
        ];
        for (command, expected) in cases {
            let code = dispatch_line(&session, &app, command, &mut io).await;
            assert_eq!(code, expected, "stderr: {}", String::from_utf8_lossy(&err));
        }
    }
    app.shutdown().await;

    // Seven stdout payload lines (six receipts + the export array);
    // the refusal is an error envelope on stderr, typed, not stdout
    // text. Export writes raw bytes regardless of --json.
    let stdout_text = String::from_utf8_lossy(&out);
    let lines = stdout_text.split('\n').filter(|l| !l.is_empty()).count();
    assert_eq!(lines, 7);
    let stderr_text = String::from_utf8_lossy(&err);
    assert!(
        stderr_text.contains("\"unknown-time-entry\""),
        "the typed selector miss rides the stderr envelope"
    );
    assert!(
        stdout_text.contains("\"started\""),
        "the start receipt is a typed JSON payload"
    );
}

/// T12: clap rules without boots — the log mode group (both modes /
/// neither / unpaired flags are usage errors), malformed
/// stamps/dates/`--format` values are usage errors, `--format json`
/// parses as the `ExportFormat` `ValueEnum`.
#[test]
fn t12_clap_mode_and_value_rules() {
    // Both log modes: usage error.
    assert!(
        Cli::try_parse_from([
            "taskboard",
            "track",
            "log",
            "t",
            "--for",
            "10m",
            "--from",
            "2026-01-01T00:00:00Z",
            "--to",
            "2026-01-01T01:00:00Z",
        ])
        .is_err()
    );
    // Neither: usage error (the required group).
    assert!(Cli::try_parse_from(["taskboard", "track", "log", "t"]).is_err());
    // Unpaired --from/--to: usage errors.
    assert!(
        Cli::try_parse_from([
            "taskboard",
            "track",
            "log",
            "t",
            "--from",
            "2026-01-01T00:00:00Z"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "taskboard",
            "track",
            "log",
            "t",
            "--to",
            "2026-01-01T00:00:00Z"
        ])
        .is_err()
    );
    // Malformed stamps/dates/format values: usage errors (exit 2,
    // never a boot).
    for argv in [
        vec![
            "track",
            "log",
            "t",
            "--from",
            "yesterday",
            "--to",
            "tomorrow",
        ],
        vec!["track", "log", "t", "--for", "soonish"],
        vec!["track", "log", "t", "--from", "09:30", "--to", "10:30"],
        vec!["track", "list", "--since", "2026-13-40"],
        vec!["export", "--since", "tomorrow"],
        vec!["export", "--until", "24.12.2026"],
        vec!["export", "--format", "yaml"],
    ] {
        let mut cmdline = vec!["taskboard"];
        cmdline.extend_from_slice(&argv);
        assert!(
            Cli::try_parse_from(&cmdline).is_err(),
            "{argv:?} must be a clap usage error"
        );
    }
    // `--format json` parses as the ValueEnum; omission defaults to csv.
    let query = parse_export(&["export", "--format", "json"]);
    assert_eq!(query.format, Some(ExportFormat::Json));
    let query = parse_export(&["export"]);
    assert_eq!(query.format, None, "omission folds to the Csv default");
}

/// T13: the hermetic export journey — seed tasks + fixed-stamp logs +
/// one open entry → `export` (default CSV) as a full byte golden (the
/// ids come from the receipts; the nasty title is quoted, its embedded
/// newline stays inside the field); `--format json` → the array with
/// `"end": null`.
#[tokio::test]
async fn t13_export_byte_goldens_through_a_real_boot() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t13.db");
    let nasty_task = seed_task(&config, "pl\"ain nasty\ntitle").await;
    let open_task = seed_task(&config, "open work").await;
    let log1 = run_log(
        &config,
        &[
            "pl\"ain nasty\ntitle",
            "--from",
            "2026-01-01T08:00:00+01:00",
            "--to",
            "2026-01-01T10:00:00+01:00",
        ],
    )
    .await
    .expect("log 1");
    let log2 = run_log(
        &config,
        &[
            "open work",
            "--from",
            "2026-01-02T09:00:00Z",
            "--to",
            "2026-01-02T10:30:00Z",
        ],
    )
    .await
    .expect("log 2");
    let started = run_start(&config, "open work").await;

    // Default CSV: byte golden. The closed rows carry the fixed-stamp
    // truth; the open row's end cell is empty and its duration is
    // numeric (accrued to the export instant).
    let csv = run_export(&config, &parse_export(&["export"])).await;
    let header = "entry_id,task_id,task_title,start,end,duration_seconds\n";
    let closed_row_one = format!(
        "{},{},\"pl\"\"ain nasty\ntitle\",2026-01-01T07:00:00+00:00,2026-01-01T09:00:00+00:00,7200\n",
        log1.entry.entry_id, nasty_task
    );
    let closed_row_two = format!(
        "{},{},open work,2026-01-02T09:00:00+00:00,2026-01-02T10:30:00+00:00,5400\n",
        log2.entry.entry_id, open_task
    );
    let open_prefix = format!(
        "{},{},open work,{},",
        started.entry.entry_id,
        open_task,
        started.entry.start.to_rfc3339()
    );
    let open_tail = csv
        .strip_prefix(header)
        .and_then(|rest| rest.strip_prefix(&closed_row_one))
        .and_then(|rest| rest.strip_prefix(&closed_row_two))
        .and_then(|rest| rest.strip_prefix(&open_prefix))
        .expect("the CSV rows in canonical order with the exact golden bytes");
    // After the start stamp: the empty end cell, then the numeric
    // duration cell (",<empty>,<secs>\n" — the prefix consumed the
    // first comma).
    let duration = open_tail
        .strip_prefix(',')
        .and_then(|rest| rest.strip_suffix('\n'))
        .unwrap_or_else(|| panic!("the open row ends with an empty end cell: {open_tail:?}"));
    duration
        .parse::<u64>()
        .expect("the open duration is a numeric cell (accrued to the export instant)");

    // JSON: a single-line array; the open row's end is null.
    let json = run_export(&config, &parse_export(&["export", "--format", "json"])).await;
    assert_eq!(json.lines().count(), 1);
    let parsed: serde_json::Value = serde_json::from_str(json.trim()).expect("a JSON array");
    let rows = parsed.as_array().expect("array");
    assert_eq!(rows.len(), 3);
    let open_json = rows
        .iter()
        .find(|row| row["entry_id"] == started.entry.entry_id.as_str())
        .expect("the open row is in the array");
    assert_eq!(open_json["end"], serde_json::Value::Null, "the open marker");
    assert_eq!(open_json["task_title"], "open work");
}

/// T14: filters through the boot — `--task`, `--board`, and the
/// `--since`/`--until` inclusive boundary (an entry starting exactly at
/// `--since` midnight is in; one starting the day after `--until` is
/// out).
#[tokio::test]
async fn t14_export_filters_through_the_boot() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t14.db");
    let one = taskboard_app::commands::boards::create(
        config.clone(),
        forbidden_store(),
        "board one",
        None,
    )
    .await
    .expect("board one");
    let two = taskboard_app::commands::boards::create(
        config.clone(),
        forbidden_store(),
        "board two",
        None,
    )
    .await
    .expect("board two");
    seed_task_on_board(&config, "one task", &one.id).await;
    seed_task_on_board(&config, "two task", &two.id).await;

    // History: three fixed-stamp entries on "one task", one on
    // "two task".
    for (task, from, to) in [
        ("one task", "2026-02-01T08:00:00Z", "2026-02-01T09:00:00Z"),
        ("two task", "2026-02-02T08:00:00Z", "2026-02-02T09:00:00Z"),
        ("one task", "2026-02-03T00:00:00Z", "2026-02-03T01:00:00Z"),
        ("one task", "2026-02-05T08:00:00Z", "2026-02-05T09:00:00Z"),
    ] {
        run_log(&config, &[task, "--from", from, "--to", to])
            .await
            .expect("logs");
    }

    // --task: only the named task's rows.
    let rows = run_export(&config, &parse_export(&["export", "--task", "two task"])).await;
    assert_eq!(rows.lines().count() - 1, 1, "one row for the two-task");
    assert!(rows.contains("two task"));

    // --since inclusive at midnight: the 2026-02-03T00:00:00Z entry is
    // in; paired with the same --until, it is the only one.
    let rows = run_export(
        &config,
        &parse_export(&["export", "--since", "2026-02-03", "--until", "2026-02-03"]),
    )
    .await;
    assert_eq!(
        rows.lines().count() - 1,
        1,
        "the midnight-start boundary row is in"
    );
    assert!(rows.contains("2026-02-03T00:00:00+00:00"));

    // --until is inclusive of the named day: 02-02 keeps the 02-01 and
    // 02-02 rows; 02-01 keeps exactly the 02-01 row.
    let rows = run_export(&config, &parse_export(&["export", "--until", "2026-02-02"])).await;
    assert_eq!(rows.lines().count() - 1, 2, "the 02-02 row is inclusive");
    let rows = run_export(&config, &parse_export(&["export", "--until", "2026-02-01"])).await;
    assert_eq!(rows.lines().count() - 1, 1, "only the 02-01 row");
    assert!(rows.contains("2026-02-01T08:00:00+00:00"));

    // --board: the rows of the tasks whose stacks belong to the board.
    let rows = run_export(&config, &parse_export(&["export", "--board", "board two"])).await;
    assert_eq!(rows.lines().count() - 1, 1, "only the board-two task's row");
    assert!(rows.contains("two task"));

    // An unresolvable --board refuses typed.
    let err = track::export(
        config.clone(),
        forbidden_store(),
        parse_export(&["export", "--board", "no such board"]),
    )
    .await
    .expect_err("no such board");
    assert!(
        matches!(err, CommandError::UnknownBoardSelector { .. }),
        "the live-board selector miss is typed"
    );
}

/// T15: the envelope exception — the export payload bytes are
/// identical under `json: false` and `json: true`; a failing export
/// rides the stderr envelope with exit 1 in both modes; track
/// receipts, by contrast, still ride the `--json` envelope.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one leg per envelope fact; kept flat for review
async fn t15_the_export_payload_bypasses_the_envelope() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t15.db");
    seed_task(&config, "envelope work").await;
    run_log(
        &config,
        &[
            "envelope work",
            "--from",
            "2026-03-01T08:00:00Z",
            "--to",
            "2026-03-01T09:30:00Z",
        ],
    )
    .await
    .expect("logs");

    // The lane-level pin: export's bytes are json-flag-blind.
    let app = taskboard_app::bootstrap::bootstrap(
        config.clone(),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session boots");
    let query = parse_export(&["export"]);
    let mut out_plain = Vec::new();
    let mut err_plain = Vec::new();
    let mut out_json = Vec::new();
    let mut err_json = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out_plain,
            err: &mut err_plain,
            json: false,
        };
        assert_eq!(track::export_line(&app, &mut io, &query), 0);
    }
    {
        let mut io = CommandIo {
            out: &mut out_json,
            err: &mut err_json,
            json: true,
        };
        assert_eq!(track::export_line(&app, &mut io, &query), 0);
    }
    app.shutdown().await;
    assert_eq!(
        out_plain, out_json,
        "the payload bytes are identical under both --json modes"
    );
    assert!(err_plain.is_empty() && err_json.is_empty());

    // A failing export rides the stderr envelope with exit 1 in both
    // modes (the receipt-envelope lane is still the failure lane).
    let bad = parse_export(&["export", "--task", "no such task"]);
    for json in [false, true] {
        let app = taskboard_app::bootstrap::bootstrap(
            config.clone(),
            forbidden_store(),
            Arc::new(taskboard_app::notifications::LogSink),
        )
        .await
        .expect("session boots");
        let mut out = Vec::new();
        let mut err = Vec::new();
        {
            let mut io = CommandIo {
                out: &mut out,
                err: &mut err,
                json,
            };
            assert_eq!(track::export_line(&app, &mut io, &bad), 1);
        }
        app.shutdown().await;
        assert!(out.is_empty(), "nothing on stdout");
        assert!(!err.is_empty(), "the refusal reports on stderr");
        if json {
            let stderr_text = String::from_utf8_lossy(&err);
            assert!(stderr_text.contains("\"unknown-task\""));
        }
    }

    // Contrast pin: a track receipt still rides the --json envelope.
    let app = taskboard_app::bootstrap::bootstrap(
        config.clone(),
        forbidden_store(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    .expect("session boots");
    let session = BasicSession {
        config: config.clone(),
        store: forbidden_store(),
        config_path: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let command = Command::Track {
            cmd: TrackCmd::List {
                task: None,
                since: None,
            },
        };
        assert_eq!(dispatch_line(&session, &app, command, &mut io).await, 0);
    }
    app.shutdown().await;
    let stdout_text = String::from_utf8_lossy(&out);
    assert!(
        stdout_text.trim_start().starts_with('{') && stdout_text.contains("\"entries\""),
        "the receipt payload is the JSON envelope shape, not raw data"
    );
}
