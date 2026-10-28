// SPDX-License-Identifier: AGPL-3.0-only
//! P-series (DR-6/4): the `tasks repeat` verb family over hermetic
//! standalone tempdirs (no network I/O; one invocation is a full
//! boot/work/shutdown "process"). Receipts are asserted as typed
//! values; error kinds via `error_kind()`, never message text; the
//! clap grammar is pinned via `Cli::try_parse_from` (usage errors are
//! exit 2). Label renderings are UX text and never asserted.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use clap::Parser;

use taskboard_app::cli::{Cli, Command, TasksCmd};
use taskboard_app::commands::output::RepeatReceipt;
use taskboard_app::commands::{BasicSession, CommandIo, dispatch_line, repeat};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::CredentialStore;
use taskboard_domain::{EveryDays, EveryWeeks, MonthlyAnchor, OccurrenceCount};

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

/// Seeds board → stack → task through one-shot verbs (each test gets a
/// fresh db; the board is minted only when the db has none — a second
/// live board would break the default resolution).
async fn seed(config: &AppConfig, stack: &str, title: &str) {
    use taskboard_app::commands::{boards, stacks, tasks};
    if let Err(taskboard_app::commands::CommandError::AmbiguousBoard { candidates }) =
        stacks::add(config.clone(), forbidden_store(), None, stack, None).await
        && candidates.is_empty()
    {
        let _ = boards::create(config.clone(), forbidden_store(), "Personal", None).await;
        let _ = stacks::add(config.clone(), forbidden_store(), None, stack, None).await;
    }
    let _ = tasks::add(config.clone(), forbidden_store(), title, Some(stack)).await;
}

/// The repeat invocation the grammar tests fold from: exactly one mode
/// flag plus optional bounds.
fn set_invocation(
    task: &str,
    pattern: repeat::PatternSpec,
    bound: repeat::BoundSpec,
) -> repeat::RepeatInvocation {
    repeat::RepeatInvocation::Set {
        task: task.to_owned(),
        pattern,
        bound,
    }
}

fn off_invocation(task: &str) -> repeat::RepeatInvocation {
    repeat::RepeatInvocation::Off {
        task: task.to_owned(),
    }
}

// ---- the clap grammar (T5) ----

#[test]
fn the_pattern_flags_parse_bare_and_valued() {
    let ok = Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--daily"])
        .expect("bare --daily parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(args.task, "t");
    assert_eq!(
        args.mode.daily,
        Some(EveryDays::new(1).unwrap()),
        "bare = 1"
    );
    assert_eq!(args.mode.weekly, None);
    assert!(!args.mode.off);

    let ok = Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--daily", "3"])
        .expect("valued --daily parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(args.mode.daily, Some(EveryDays::new(3).unwrap()));

    // Bare --weekly keeps the task-weekday default through the parse.
    let ok = Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--weekly"])
        .expect("bare --weekly parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(
        args.mode.weekly,
        Some(taskboard_app::cli::WeekdaysFlag::TaskWeekday)
    );
    let ok = Cli::try_parse_from([
        "taskboard",
        "tasks",
        "repeat",
        "t",
        "--weekly",
        "fri,mon",
        "--every-weeks",
        "2",
    ])
    .expect("--weekly with days and interval parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(args.every_weeks, Some(EveryWeeks::new(2).unwrap()));
    // Set semantics: the parse canonicalizes (dedup + order) via the mint.
    let Some(taskboard_app::cli::WeekdaysFlag::Days(days)) = args.mode.weekly else {
        panic!("valued --weekly");
    };
    assert_eq!(days.text_form(), "mon,fri");

    let ok = Cli::try_parse_from([
        "taskboard",
        "tasks",
        "repeat",
        "t",
        "--daily",
        "--until",
        "2026-12-31",
    ])
    .expect("--until parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(
        args.until,
        Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap())
    );

    let ok =
        Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--off"]).expect("--off parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert!(args.mode.off);
}

/// The `--monthly`/`--off` half of the flag table (split to keep each
/// test inside the function-length budget).
#[test]
fn the_monthly_and_off_flags_parse_bare_and_valued() {
    // Bare --monthly keeps the task-day default; valued forms parse.
    let ok = Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--monthly"])
        .expect("bare --monthly parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(
        args.mode.monthly,
        Some(taskboard_app::cli::MonthlyFlag::TaskDay)
    );
    let ok = Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--monthly", "2nd-mon"])
        .expect("nth-weekday --monthly parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert_eq!(
        args.mode.monthly,
        Some(taskboard_app::cli::MonthlyFlag::Anchor(
            MonthlyAnchor::parse_text_form("2nd-mon").unwrap()
        ))
    );

    let ok =
        Cli::try_parse_from(["taskboard", "tasks", "repeat", "t", "--off"]).expect("--off parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Repeat(args),
    }) = ok.command
    else {
        panic!("expected the Repeat variant");
    };
    assert!(args.mode.off);
}

#[test]
fn incoherent_flag_combinations_are_usage_errors() {
    let conflicts: [(&str, Vec<&str>); 8] = [
        ("two patterns", vec!["--daily", "--weekly"]),
        ("three patterns", vec!["--daily", "--monthly"]),
        ("off with a pattern", vec!["--off", "--daily"]),
        ("off with a bound", vec!["--off", "--count", "3"]),
        (
            "until with count",
            vec!["--daily", "--until", "2026-12-31", "--count", "3"],
        ),
        ("every-weeks without weekly", vec!["--every-weeks", "2"]),
        ("bound without a pattern", vec!["--until", "2026-12-31"]),
        ("count without a pattern", vec!["--count", "3"]),
    ];
    for (what, extra) in conflicts {
        let mut argv = vec!["taskboard", "tasks", "repeat", "t"];
        argv.extend(extra);
        let err = Cli::try_parse_from(argv).expect_err(what);
        assert_eq!(err.exit_code(), 2, "{what}");
    }
    // No mode at all is a usage error (the group is required).
    let err =
        Cli::try_parse_from(["taskboard", "tasks", "repeat", "t"]).expect_err("a mode is required");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn bad_flag_values_are_usage_errors() {
    for (value, argv_tail) in [
        ("count 0", vec!["--count", "0"]),
        ("daily 0", vec!["--daily", "0"]),
        ("unknown weekday", vec!["--weekly", "mon,x"]),
        ("capitalized weekday", vec!["--weekly", "Mon"]),
        ("nth out of range", vec!["--monthly", "15th-mon"]),
        ("non-date until", vec!["--daily", "--until", "tomorrow"]),
    ] {
        let mut argv = vec!["taskboard", "tasks", "repeat", "t"];
        argv.extend(argv_tail);
        let err = Cli::try_parse_from(argv).expect_err(value);
        assert_eq!(err.exit_code(), 2, "{value}");
    }
}

// ---- the collapse (T6) ----

#[test]
fn every_flag_tuple_folds_to_exactly_one_invocation() {
    use taskboard_app::cli::{MonthlyFlag, RepeatArgs, RepeatModeFlags, WeekdaysFlag};

    let fold = |args: RepeatArgs| {
        repeat::collapse(TasksCmd::Repeat(args)).expect("a clap-valid Repeat always collapses")
    };
    let mode = |daily, weekly, monthly, off| RepeatModeFlags {
        daily,
        weekly,
        monthly,
        off,
    };
    // Bare defaults survive the collapse as `None`s for the handler.
    match fold(RepeatArgs {
        task: "t".into(),
        mode: mode(None, Some(WeekdaysFlag::TaskWeekday), None, false),
        every_weeks: None,
        until: None,
        count: None,
    }) {
        repeat::RepeatInvocation::Set {
            pattern: repeat::PatternSpec::Weekly { interval, on },
            bound: repeat::BoundSpec::None,
            ..
        } => {
            assert_eq!(interval.get(), 1, "no --every-weeks = every week");
            assert!(on.is_none(), "bare --weekly stays handler-resolved");
        }
        other => panic!("weekly folds wrong: {other:?}"),
    }
    match fold(RepeatArgs {
        task: "t".into(),
        mode: mode(None, None, Some(MonthlyFlag::TaskDay), false),
        every_weeks: None,
        until: None,
        count: None,
    }) {
        repeat::RepeatInvocation::Set {
            pattern: repeat::PatternSpec::Monthly { on },
            ..
        } => assert!(on.is_none(), "bare --monthly stays handler-resolved"),
        other => panic!("monthly folds wrong: {other:?}"),
    }
    // The interval and the bounds ride.
    match fold(RepeatArgs {
        task: "t".into(),
        mode: mode(Some(EveryDays::new(2).unwrap()), None, None, false),
        every_weeks: None,
        until: Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()),
        count: None,
    }) {
        repeat::RepeatInvocation::Set {
            pattern: repeat::PatternSpec::Daily(every),
            bound: repeat::BoundSpec::Until(date),
            ..
        } => {
            assert_eq!(every.get(), 2);
            assert_eq!(date, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        }
        other => panic!("daily folds wrong: {other:?}"),
    }
    let folded = fold(RepeatArgs {
        task: "t".into(),
        mode: mode(None, None, None, true),
        every_weeks: None,
        until: None,
        count: None,
    });
    assert!(matches!(
        folded,
        repeat::RepeatInvocation::Off { ref task } if task == "t"
    ));
    // A count bound folds as itself (never misread as until).
    match fold(RepeatArgs {
        task: "t".into(),
        mode: mode(
            None,
            Some(WeekdaysFlag::Days(
                taskboard_domain::WeekdaySet::parse_text_form("mon").unwrap(),
            )),
            None,
            false,
        ),
        every_weeks: Some(EveryWeeks::new(2).unwrap()),
        until: None,
        count: Some(OccurrenceCount::new(5).unwrap()),
    }) {
        repeat::RepeatInvocation::Set {
            pattern:
                repeat::PatternSpec::Weekly {
                    interval,
                    on: Some(days),
                },
            bound: repeat::BoundSpec::Count(n),
            ..
        } => {
            assert_eq!(interval.get(), 2);
            assert_eq!(days.len(), 1);
            assert_eq!(n.get(), 5);
        }
        other => panic!("weekly+count folds wrong: {other:?}"),
    }
    // Non-Repeat commands do not collapse.
    assert!(repeat::collapse(TasksCmd::List { stack: None }).is_none());
}

// ---- the verb family over hermetic cores (T7–T11) ----

/// noon UTC on `d`: a scheduled/duedate instant whose *calendar date*
/// is `d` under any device offset in ±12h (the hermetic anchor pin —
/// the date is offset-independent by construction).
fn noon_utc(d: NaiveDate) -> chrono::DateTime<Utc> {
    d.and_hms_opt(12, 0, 0)
        .expect("noon is a valid time")
        .and_utc()
}

/// P1: `--daily` on a seeded task sets a rule (receipt carries
/// series/anchor/pattern/next), and across a fresh boot `--off` clears
/// it (the clear is durable: the next set mints a *new* series — a
/// surviving rule would have been re-scheduled under the old id).
#[tokio::test]
async fn p1_set_receipts_and_the_off_clear_is_durable() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "p1.db");
    seed(&config, "Todo", "Water plants").await;

    let receipt = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Water plants",
            repeat::PatternSpec::Daily(EveryDays::new(1).unwrap()),
            repeat::BoundSpec::None,
        ),
    )
    .await
    .expect("set");
    let RepeatReceipt {
        task_id,
        task_title,
        rule,
    } = receipt;
    assert_eq!(task_title, "Water plants");
    assert_ne!(task_id, "");
    let rule = rule.expect("the set left a rule in post-state");
    assert_ne!(rule.series, "", "the series id is minted");
    assert_eq!(rule.pattern, "daily");
    assert_eq!(rule.bound, "");
    assert!(rule.next.is_some(), "an unbounded daily series has a next");

    // A fresh boot's --off clears it, durably.
    let off = repeat::repeat(
        config.clone(),
        forbidden_store(),
        off_invocation("Water plants"),
    )
    .await
    .expect("off");
    assert_eq!(off.rule, None);

    // The proof the clear persisted: the next set mints a *new* series
    // (D-S4 keeps a surviving rule's id).
    let again = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Water plants",
            repeat::PatternSpec::Daily(EveryDays::new(2).unwrap()),
            repeat::BoundSpec::None,
        ),
    )
    .await
    .expect("set again");
    let again_rule = again.rule.expect("the re-set left a rule");
    assert_ne!(again_rule.series, rule.series, "the clear was durable");
    assert_eq!(again_rule.pattern, "every 2d");
}

/// P2: the anchor chain — scheduled calendar date, duedate calendar
/// date, else the window-date of now (the domain's own constructor
/// computes the dateless expectation; the day-start config pins the
/// window flavor). The instants sit at noon UTC, so every leg's
/// calendar date is offset-independent by construction.
#[tokio::test]
async fn p2_anchor_chain_scheduled_duedate_then_window_today() {
    use taskboard_domain::TaskChanges;

    let dir = tempdir();
    let config = standalone_config(dir.path(), "p2.db");
    seed(&config, "S1", "Scheduled one").await;
    seed(&config, "S2", "Due only").await;
    seed(&config, "S3", "Dateless one").await;

    let edit = |config: &AppConfig, title: &str, changes: TaskChanges| {
        let config = config.clone();
        let title = title.to_owned();
        async move {
            taskboard_app::commands::tasks::edit(config, forbidden_store(), &title, changes)
                .await
                .expect("edit");
        }
    };
    edit(
        &config,
        "Scheduled one",
        TaskChanges {
            scheduled: Some(Some(noon_utc(NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()))),
            ..TaskChanges::default()
        },
    )
    .await;
    edit(
        &config,
        "Due only",
        TaskChanges {
            duedate: Some(Some(noon_utc(NaiveDate::from_ymd_opt(2026, 4, 6).unwrap()))),
            ..TaskChanges::default()
        },
    )
    .await;

    let daily = || repeat::PatternSpec::Daily(EveryDays::new(1).unwrap());
    let receipt = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation("Scheduled one", daily(), repeat::BoundSpec::None),
    )
    .await
    .expect("scheduled anchor");
    assert_eq!(
        receipt.rule.expect("rule").anchor,
        NaiveDate::from_ymd_opt(2026, 3, 5).unwrap(),
        "scheduled wins the chain"
    );

    let receipt = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation("Due only", daily(), repeat::BoundSpec::None),
    )
    .await
    .expect("duedate anchor");
    assert_eq!(
        receipt.rule.expect("rule").anchor,
        NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
        "duedate is the fallback"
    );

    // Dateless: the window-date of now, computed by the same calendar
    // the verb reads (no reimplementation). The two clock reads flank
    // the verb call, so the answer must sit inside their window-date
    // neighborhood.
    let before = config.day_calendar().calendar.today(Utc::now());
    let receipt = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation("Dateless one", daily(), repeat::BoundSpec::None),
    )
    .await
    .expect("dateless anchor");
    let after = config.day_calendar().calendar.today(Utc::now());
    let anchor = receipt.rule.expect("rule").anchor;
    let neighbors = [
        before.pred_opt().expect("a date has a predecessor"),
        before,
        before.succ_opt().expect("a date has a successor"),
    ];
    assert!(
        neighbors.contains(&anchor),
        "dateless anchor {anchor} is the window-date of the call (window {before}..{after})"
    );

    // The `[preferences] day_start_time` config pins the window-date
    // flavor: the same structural check under a night-owl day start —
    // the verb's dateless anchor is *that* calendar's window-date.
    let mut owl = standalone_config(dir.path(), "p2.db");
    owl.preferences.day_start_time = "23:59".to_owned();
    seed(&owl, "Todo", "Owl one").await;
    let owl_before = owl.day_calendar().calendar.today(Utc::now());
    let receipt = repeat::repeat(
        owl.clone(),
        forbidden_store(),
        set_invocation("Owl one", daily(), repeat::BoundSpec::None),
    )
    .await
    .expect("dateless anchor under the night-owl window");
    let owl_anchor = receipt.rule.expect("rule").anchor;
    let owl_neighbors = [
        owl_before.pred_opt().expect("a date has a predecessor"),
        owl_before,
        owl_before.succ_opt().expect("a date has a successor"),
    ];
    assert!(
        owl_neighbors.contains(&owl_anchor),
        "night-owl anchor {owl_anchor} is the owl window-date of the call (window {owl_before})"
    );
}

/// P3: re-scheduling an already-recurring task keeps the series
/// identity and replaces pattern/bound (D-S4).
#[tokio::test]
async fn p3_re_repeat_keeps_the_series_and_replaces_the_shape() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "p3.db");
    seed(&config, "Todo", "Re-scheduled").await;

    let first = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Re-scheduled",
            repeat::PatternSpec::Daily(EveryDays::new(1).unwrap()),
            repeat::BoundSpec::None,
        ),
    )
    .await
    .expect("first set");
    let first_rule = first.rule.expect("rule");

    let second = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Re-scheduled",
            repeat::PatternSpec::Weekly {
                interval: EveryWeeks::new(2).unwrap(),
                on: Some(taskboard_domain::WeekdaySet::parse_text_form("mon,wed,fri").unwrap()),
            },
            repeat::BoundSpec::Count(OccurrenceCount::new(4).unwrap()),
        ),
    )
    .await
    .expect("second set");
    let second_rule = second.rule.expect("rule");
    assert_eq!(
        second_rule.series, first_rule.series,
        "the series identity survives the re-schedule"
    );
    assert_eq!(second_rule.pattern, "every 2w on mon,wed,fri");
    assert_eq!(second_rule.bound, "count 4");
    // The anchor is never touched by a re-schedule.
    assert_eq!(second_rule.anchor, first_rule.anchor);
}

/// P4: `--off` clears (receipt `rule: None`); a repeated `--off` stays
/// a success with `rule: None` — the post-state truth, no pre-check.
#[tokio::test]
async fn p4_off_clears_and_repeats() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "p4.db");
    seed(&config, "Todo", "Stop me").await;
    repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Stop me",
            repeat::PatternSpec::Daily(EveryDays::new(1).unwrap()),
            repeat::BoundSpec::None,
        ),
    )
    .await
    .expect("set");

    let off = repeat::repeat(config.clone(), forbidden_store(), off_invocation("Stop me"))
        .await
        .expect("off");
    assert_eq!(off.rule, None);

    // An `--off` on a task that never repeated is the same post-state.
    seed(&config, "Todo2", "Never repeated").await;
    let off = repeat::repeat(
        config.clone(),
        forbidden_store(),
        off_invocation("Never repeated"),
    )
    .await
    .expect("off on a plain task");
    assert_eq!(off.rule, None);
}

/// P5: typed refusals — a pattern on a done task is `task-done`;
/// selector failures keep their cascade kinds.
#[tokio::test]
async fn p5_typed_refusals() {
    use taskboard_app::commands::CommandError;

    let dir = tempdir();
    let config = standalone_config(dir.path(), "p5.db");
    seed(&config, "Todo", "Finished").await;
    taskboard_app::commands::tasks::done(config.clone(), forbidden_store(), "Finished", false)
        .await
        .expect("done");

    let err = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Finished",
            repeat::PatternSpec::Daily(EveryDays::new(1).unwrap()),
            repeat::BoundSpec::None,
        ),
    )
    .await
    .expect_err("done task refuses a pattern");
    assert!(matches!(err, CommandError::TaskDone { ref selector } if selector == "Finished"));
    assert_eq!(err.error_kind(), "task-done");
    assert_eq!(err.error_detail().as_deref(), Some("Finished"));

    // --off passes on the done head (clearing a dead rule is honest
    // cleanup).
    let off = repeat::repeat(
        config.clone(),
        forbidden_store(),
        off_invocation("Finished"),
    )
    .await
    .expect("off on a done task");
    assert_eq!(off.rule, None);

    let err = repeat::repeat(
        config.clone(),
        forbidden_store(),
        off_invocation("no-such-task"),
    )
    .await
    .expect_err("unknown selector");
    assert_eq!(err.error_kind(), "unknown-task");

    // An ambiguous title keeps its kind.
    seed(&config, "A", "Duplicate").await;
    seed(&config, "B", "Duplicate").await;
    let err = repeat::repeat(
        config.clone(),
        forbidden_store(),
        off_invocation("Duplicate"),
    )
    .await
    .expect_err("ambiguous selector");
    assert_eq!(err.error_kind(), "ambiguous-task");
}

/// P6: the session lane serves set + off end to end over a booted
/// session (`dispatch_line`, the r10 shape); the `--until` bound
/// projects into the receipt; durability across the session.
#[tokio::test]
async fn p6_session_lane_serves_repeat_end_to_end() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "p6.db");
    seed(&config, "Todo", "Session repeat").await;

    let store = forbidden_store();
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

    let repeat_cmd = |mode: taskboard_app::cli::RepeatModeFlags,
                      until: Option<NaiveDate>,
                      count: Option<OccurrenceCount>| {
        Command::Tasks {
            cmd: TasksCmd::Repeat(taskboard_app::cli::RepeatArgs {
                task: "Session repeat".to_owned(),
                mode,
                every_weeks: None,
                until,
                count,
            }),
        }
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let code = dispatch_line(
            &session,
            &app,
            repeat_cmd(
                taskboard_app::cli::RepeatModeFlags {
                    daily: Some(EveryDays::new(1).unwrap()),
                    weekly: None,
                    monthly: None,
                    off: false,
                },
                Some(NaiveDate::from_ymd_opt(2027, 1, 31).unwrap()),
                None,
            ),
            &mut io,
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let code = dispatch_line(
            &session,
            &app,
            repeat_cmd(
                taskboard_app::cli::RepeatModeFlags {
                    daily: None,
                    weekly: None,
                    monthly: None,
                    off: true,
                },
                None,
                None,
            ),
            &mut io,
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));
    }
    app.shutdown().await;

    // Both receipts emitted one JSON line each.
    let lines: Vec<&[u8]> = out
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 2);
    let set_receipt: RepeatReceipt =
        serde_json::from_slice(lines[0]).expect("the set receipt is the first line");
    let rule = set_receipt.rule.expect("the session set left a rule");
    assert_eq!(rule.bound, "until 2027-01-31");
    assert!(
        rule.next
            .is_some_and(|d| d <= NaiveDate::from_ymd_opt(2027, 1, 31).unwrap())
    );
    let off_receipt: RepeatReceipt = serde_json::from_slice(lines[1]).expect("the off receipt");
    assert_eq!(off_receipt.rule, None);
}

/// The route table picks the IPC-or-local lane for a `Repeat` command
/// (the lane-table row; pinned structurally in the routing unit test —
/// this is the same shape asserted there).
#[test]
fn repeat_routes_ipc_or_local() {
    use taskboard_app::commands::{IpcRoute, route_for_ipc};
    let command = Command::Tasks {
        cmd: TasksCmd::Repeat(taskboard_app::cli::RepeatArgs {
            task: "t".to_owned(),
            mode: taskboard_app::cli::RepeatModeFlags {
                daily: Some(EveryDays::new(1).unwrap()),
                weekly: None,
                monthly: None,
                off: false,
            },
            every_weeks: None,
            until: None,
            count: None,
        }),
    };
    assert_eq!(route_for_ipc(&command), IpcRoute::IpcOrLocal);
}

/// The `--until` bound and the next-member projection agree: a daily
/// series bounded before tomorrow advertises nothing past the bound
/// (the receipt's `next: None` shape) when the next member exceeds it.
#[tokio::test]
async fn p7_until_bound_exhaustion_projects_next_none() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "p7.db");
    seed(&config, "Todo", "Bounded one").await;

    // A tight bound *in the past*: the daily next member (tomorrow or
    // the roll target) is past it, so the receipt advertises `None` —
    // this head is the series' last.
    let receipt = repeat::repeat(
        config.clone(),
        forbidden_store(),
        set_invocation(
            "Bounded one",
            repeat::PatternSpec::Daily(EveryDays::new(1).unwrap()),
            repeat::BoundSpec::Until(
                Utc::now()
                    .date_naive()
                    .pred_opt()
                    .expect("yesterday exists"),
            ),
        ),
    )
    .await
    .expect("bounded set");
    let rule = receipt.rule.expect("rule");
    assert_eq!(rule.next, None, "the next member is past the Until bound");
    // And the domain law agrees with itself across a re-read: a
    // bounded series keeps its pattern render.
    assert_eq!(rule.pattern, "daily");
}
