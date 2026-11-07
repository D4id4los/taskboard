// SPDX-License-Identifier: AGPL-3.0-only
//! J-series: the `journal` verb family over hermetic standalone
//! tempdirs (the `cli_timers.rs` shape — no wiremock, a standalone
//! core performs no network I/O). Receipts are asserted as typed
//! values; error kinds via `error_kind()`, never message text; the
//! wall clock enters only as containment brackets.

use std::sync::Arc;

use chrono::Utc;
use clap::Parser;

use taskboard_app::cli::{Cli, Command};
use taskboard_app::commands::output::{ReflectionAddReceipt, ReflectionRow, ReflectionsList};
use taskboard_app::commands::reflection::{self, ReflectionInvocation};
use taskboard_app::commands::{BasicSession, CommandError, CommandIo, dispatch_line};
use taskboard_app::config::AppConfig;
use taskboard_domain::reflection::ReviewCadence;

mod common;

use common::{forbidden_store, standalone_config, tempdir};

/// Runs one `journal …` argv through the real clap parse and the
/// one-shot lane, returning the receipt.
async fn run_add(config: &AppConfig, args: &[&str]) -> ReflectionAddReceipt {
    let cmd = parse(["journal", "add"], args);
    reflection::add(
        config.clone(),
        forbidden_store(),
        reflection::collapse(cmd).expect("add collapses"),
    )
    .await
    .expect("add boots")
}

/// Parses a `journal <sub> [args]` argv into its clap command.
fn parse(prefix: [&str; 2], args: &[&str]) -> taskboard_app::cli::ReflectionCmd {
    let mut cmdline = vec!["taskboard", prefix[0], prefix[1]];
    cmdline.extend_from_slice(args);
    let cli = Cli::try_parse_from(cmdline).expect("the grammar accepts the argv");
    let Some(Command::Reflection { cmd }) = cli.command else {
        panic!("the argv selects the journal group");
    };
    cmd
}

async fn run_list(config: &AppConfig, args: &[&str]) -> ReflectionsList {
    let cmd = parse(["journal", "list"], args);
    match reflection::collapse(cmd).expect("list collapses") {
        ReflectionInvocation::List { filters } => {
            reflection::list(config.clone(), forbidden_store(), filters)
                .await
                .expect("list boots")
        }
        _ => panic!("the argv selects list"),
    }
}

async fn run_edit(
    config: &AppConfig,
    args: &[&str],
) -> Result<taskboard_app::commands::output::ReflectionEditReceipt, CommandError> {
    let cmd = parse(["journal", "edit"], args);
    match reflection::collapse(cmd).expect("edit collapses") {
        ReflectionInvocation::Edit { selector, mask } => {
            reflection::edit(config.clone(), forbidden_store(), selector, mask).await
        }
        _ => panic!("the argv selects edit"),
    }
}

async fn run_delete(
    config: &AppConfig,
    args: &[&str],
) -> Result<taskboard_app::commands::output::ReflectionDeleteReceipt, CommandError> {
    let cmd = parse(["journal", "delete"], args);
    match reflection::collapse(cmd).expect("delete collapses") {
        ReflectionInvocation::Delete(selector) => {
            reflection::delete(config.clone(), forbidden_store(), selector).await
        }
        _ => panic!("the argv selects delete"),
    }
}

async fn run_show(config: &AppConfig, selector: &str) -> Result<ReflectionRow, CommandError> {
    let cmd = parse(["journal", "show"], &[selector]);
    match reflection::collapse(cmd).expect("show collapses") {
        ReflectionInvocation::Show(selector) => {
            reflection::show(config.clone(), forbidden_store(), selector).await
        }
        _ => panic!("the argv selects show"),
    }
}

/// J1: an explicit `--day` day entry — the receipt's row is read back
/// from the snapshot, the minted clocks sit inside the test's bracket,
/// and a fresh boot's `list` sees the entry (durable).
#[tokio::test]
async fn j1_add_stamps_and_persists() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j1.db");
    let before = Utc::now();
    let receipt = run_add(&config, &["--day", "2026-11-03", "slow", "morning"]).await;
    let now = Utc::now();
    assert!(receipt.created);
    let row = receipt.entry.as_ref().expect("the minted row");
    assert_eq!(row.cadence, "day");
    assert_eq!(
        row.day,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
    );
    assert_eq!(row.text, "slow morning");
    assert!(row.created_at >= before && row.created_at <= now);
    assert_eq!(row.created_at, row.updated_at);

    let rows = run_list(&config, &[]).await.entries;
    assert_eq!(rows.len(), 1, "durable across a fresh boot");
    assert_eq!(rows[0], *row);
}

/// J2: with no `--day`, the derived window contains the boot-time now
/// and its `day` label equals the calendar's `today(now)` reading
/// (midnight-straddle-safe: containment + label-vs-derivation
/// consistency, never wall-clock equality).
#[tokio::test]
async fn j2_default_day_derives_from_the_calendar() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j2.db");
    let now = Utc::now();
    let receipt = run_add(&config, &["tea"]).await;
    let row = receipt.entry.as_ref().expect("the minted row");
    assert!(
        row.window_start <= now && now < row.window_end,
        "the day window contains now"
    );
    let calendar = config.resolved_day_calendar();
    let expected = taskboard_domain::reflection::review_window(
        ReviewCadence::Day,
        calendar.today(now),
        calendar,
    );
    let (start, end) = expected.bounds();
    assert_eq!(row.window_start, start);
    assert_eq!(row.window_end, end);
    assert_eq!(row.day, expected.day());
}

/// J3: `--cadence week/month --day` mid-period floors to the period
/// (the canonical-mint law read through the stored row: the window is
/// a 168h week / a `[1st, next 1st)` month, stored verbatim).
#[tokio::test]
async fn j3_week_and_month_flooring() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j3.db");
    // 2026-11-04 is a Wednesday; the week floors to Monday 2026-11-02.
    let receipt = run_add(&config, &["--cadence", "week", "--day", "2026-11-04", "w"]).await;
    let row = receipt.entry.as_ref().expect("the minted row");
    assert_eq!(row.cadence, "week");
    assert_eq!(
        row.day,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
    );
    assert_eq!(
        row.window_end - row.window_start,
        chrono::TimeDelta::try_hours(168).unwrap()
    );

    let receipt = run_add(&config, &["--cadence", "month", "--day", "2026-11-20", "m"]).await;
    let row = receipt.entry.as_ref().expect("the minted row");
    assert_eq!(
        row.day,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
    );
    assert!(
        row.window_end > row.window_start,
        "the month window is monotone"
    );
}

/// J4: ratings and flags round-trip onto the row; repeated `--flag`
/// dedups (set semantics); an unknown flag name is a clap usage error
/// (exit-class 2), never a boot.
#[tokio::test]
async fn j4_attributes_round_trip_and_flag_rules() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j4.db");
    let receipt = run_add(
        &config,
        &[
            "--mood",
            "3",
            "--energy",
            "4",
            "--flag",
            "migraine",
            "--flag",
            "poor-sleep",
            "--flag",
            "migraine",
            "loud day",
        ],
    )
    .await;
    let row = receipt.entry.as_ref().expect("the minted row");
    assert_eq!(row.mood, Some(3));
    assert_eq!(row.energy, Some(4));
    assert_eq!(
        row.flags,
        vec!["migraine", "poor-sleep"],
        "set semantics dedup"
    );

    let bad = Cli::try_parse_from([
        "taskboard",
        "journal",
        "add",
        "--flag",
        "not-a-flag",
        "text",
    ]);
    assert!(bad.is_err(), "an unknown flag name is a usage error");
}

/// J5: the empty record — bare `journal add` stores nothing and is
/// success (`created: false`, `entry: None`); `list` stays empty.
#[tokio::test]
async fn j5_empty_record_is_the_accepted_noop() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j5.db");
    let receipt = run_add(&config, &[]).await;
    assert!(!receipt.created);
    assert!(receipt.entry.is_none());
    assert_eq!(
        run_list(&config, &[]).await.entries,
        [] as [taskboard_app::commands::output::ReflectionRow; 0]
    );
}

/// J6: positional words join with single spaces into the body.
#[tokio::test]
async fn j6_multiword_text_joins() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j6.db");
    let receipt = run_add(&config, &["three", "word", "body"]).await;
    assert_eq!(receipt.entry.as_ref().expect("row").text, "three word body");
}

/// J7: the edit change-mask — the delta lands exactly (untouched flag
/// survives), `updated_at` advances and `created_at` never, and a
/// second identical edit is `changed: false` with the row untouched.
#[tokio::test]
async fn j7_edit_change_mask_and_no_churn() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j7.db");
    let receipt = run_add(
        &config,
        &["--flag", "migraine", "--mood", "2", "first", "draft"],
    )
    .await;
    let row = receipt.entry.as_ref().expect("row");
    let created_at = row.created_at;

    let edited = run_edit(
        &config,
        &[
            &row.entry_id[..8],
            "--text",
            "second draft",
            "--mood",
            "4",
            "--flag",
            "poor-sleep",
            "--unflag",
            "migraine",
        ],
    )
    .await
    .expect("the edit lands");
    assert!(edited.changed);
    assert_eq!(edited.entry.text, "second draft");
    assert_eq!(edited.entry.mood, Some(4));
    assert_eq!(edited.entry.flags, vec!["poor-sleep"], "flag delta");
    assert_eq!(
        edited.entry.created_at, created_at,
        "created_at never moves"
    );
    assert!(edited.entry.updated_at > created_at);

    let edited_again = run_edit(&config, &["--text", "second draft", &row.entry_id[..8]])
        .await
        .expect("the repeat edit");
    assert!(!edited_again.changed, "the identical mask is a no-op");
    assert_eq!(edited_again.entry, edited.entry, "mask-no-churn");
    assert_eq!(edited_again.entry.updated_at, edited.entry.updated_at);
}

/// J8: the explicit clears — `--clear-mood`, `--clear-energy`,
/// `--clear-text` (the double-Option semantics read through the row).
#[tokio::test]
async fn j8_edit_clears() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j8.db");
    let receipt = run_add(&config, &["--mood", "2", "--energy", "3", "clearable"]).await;
    let row = receipt.entry.as_ref().expect("row");

    let edited = run_edit(
        &config,
        &[
            &row.entry_id[..8],
            "--clear-mood",
            "--clear-energy",
            "--clear-text",
        ],
    )
    .await
    .expect("the clears land");
    assert!(edited.changed);
    assert_eq!(edited.entry.mood, None);
    assert_eq!(edited.entry.energy, None);
    assert_eq!(edited.entry.text, "", "the legal empty-text clear");
}

/// J9: typed payload refusals — an out-of-range rating is the
/// planner's `invalid-rating`; a missing selector is
/// `unknown-reflection-entry` (both exit-class typed, never text).
#[tokio::test]
async fn j9_invalid_payloads_are_typed() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j9.db");
    let err = reflection::add(
        config.clone(),
        forbidden_store(),
        reflection::collapse(parse(["journal", "add"], &["--mood", "9", "body"]))
            .expect("collapses"),
    )
    .await
    .expect_err("mood 9 rejects");
    assert_eq!(err.error_kind(), "invalid-rating");

    let err = run_show(&config, "no-such-entry")
        .await
        .expect_err("a miss");
    assert!(matches!(
        err,
        CommandError::UnknownReflectionEntry { ref selector } if selector == "no-such-entry"
    ));
    assert_eq!(err.error_kind(), "unknown-reflection-entry");
}

/// J10: the selector grammar — exact uuid, ≥8-char prefix; a 7-char
/// prefix is a miss, not an ambiguity (the `resolve_time_entry` law).
#[tokio::test]
async fn j10_selector_grammar() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j10.db");
    let receipt = run_add(&config, &["the", "entry"]).await;
    let id = receipt.entry.as_ref().expect("row").entry_id.clone();

    let shown = run_show(&config, &id).await.expect("exact uuid hit");
    assert_eq!(shown.entry_id, id);
    let shown = run_show(&config, &id[..8]).await.expect("prefix hit");
    assert_eq!(shown.entry_id, id);
    let err = run_show(&config, &id[..7])
        .await
        .expect_err("7 chars is a miss");
    assert!(matches!(err, CommandError::UnknownReflectionEntry { .. }));
}

/// J11: the delete receipt carries the pre-state row; the deleted
/// entry's selector is a typed unknown afterwards — `show` and a
/// repeat `delete` alike (Q-7).
#[tokio::test]
async fn j11_delete_pre_state_then_typed_unknown() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j11.db");
    let receipt = run_add(&config, &["to", "delete"]).await;
    let row = receipt.entry.as_ref().expect("row").clone();

    let deleted = run_delete(&config, &[&row.entry_id])
        .await
        .expect("deletes");
    assert_eq!(deleted.entry_id, row.entry_id);
    assert_eq!(deleted.entry, row, "the pre-execute row");

    let err = run_show(&config, &row.entry_id).await.expect_err("gone");
    assert!(matches!(err, CommandError::UnknownReflectionEntry { .. }));
    let err = run_delete(&config, &[&row.entry_id])
        .await
        .expect_err("the repeat targets a tombstone");
    assert!(matches!(err, CommandError::UnknownReflectionEntry { .. }));
}

/// J12: `list` filters — `--cadence`, and `--since/--until` inclusive
/// on `window_start` (boundary entries land inside) — and canonical
/// `(window_start, entry_id)` order over a shuffled fixture.
#[tokio::test]
async fn j12_list_filters_and_canonical_order() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j12.db");
    let a = run_add(&config, &["--day", "2026-11-03", "a"]).await;
    let b = run_add(&config, &["--day", "2026-11-01", "b"]).await;
    let c = run_add(&config, &["--day", "2026-11-02", "--cadence", "week", "c"]).await;

    let (id_a, id_b, id_c) = (
        a.entry.as_ref().unwrap().entry_id.clone(),
        b.entry.as_ref().unwrap().entry_id.clone(),
        c.entry.as_ref().unwrap().entry_id.clone(),
    );
    let rows = run_list(&config, &[]).await.entries;
    let ids: Vec<&str> = rows.iter().map(|r| r.entry_id.as_str()).collect();
    assert_eq!(
        ids,
        vec![id_b.as_str(), id_c.as_str(), id_a.as_str()],
        "canonical (window_start, entry_id) order"
    );

    let rows = run_list(&config, &["--cadence", "week"]).await.entries;
    assert_eq!(rows.len(), 1, "the cadence filter");
    assert_eq!(rows[0].entry_id, id_c);

    // Inclusive day edges on window_start: the filter dates derive
    // from the entries' own window-start instants (timezone-blind) —
    // `since` names the earliest entry's UTC day, `until` the middle
    // entry's; the boundary entries land inside, the latest stays out.
    let day_of = |row: &ReflectionRow| row.window_start.date_naive().to_string();
    let since = day_of(b.entry.as_ref().unwrap());
    let until = day_of(c.entry.as_ref().unwrap());
    let rows = run_list(&config, &["--since", &since, "--until", &until])
        .await
        .entries;
    let ids: Vec<&str> = rows.iter().map(|r| r.entry_id.as_str()).collect();
    assert!(
        ids.contains(&id_b.as_str()) && ids.contains(&id_c.as_str()),
        "windows opening on the boundary days land inside"
    );
    assert!(
        !ids.contains(&id_a.as_str()),
        "windows opening after the until day stay out"
    );
}

/// J13: `show` returns the row `list` carries — projection parity.
#[tokio::test]
async fn j13_show_matches_the_list_row() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j13.db");
    let receipt = run_add(&config, &["shown"]).await;
    let row = receipt.entry.as_ref().expect("row").clone();
    let shown = run_show(&config, &row.entry_id).await.expect("show");
    assert_eq!(shown, row);
    let listed = run_list(&config, &[]).await.entries;
    assert_eq!(listed, vec![row]);
}

/// J14: the session lane under `--json` emits serde envelopes whose
/// typed fields equal the text-lane receipts (add/list/show/edit/
/// delete), and the refusal rides the typed error envelope.
#[tokio::test]
async fn j14_json_envelopes_on_the_session_lane() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j14.db");
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

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let codes = [
            dispatch_line(
                &session,
                &app,
                Command::Reflection {
                    cmd: parse(["journal", "add"], &["tea", "at", "two"]),
                },
                &mut io,
            )
            .await,
            dispatch_line(
                &session,
                &app,
                Command::Reflection {
                    cmd: parse(["journal", "list"], &[]),
                },
                &mut io,
            )
            .await,
            dispatch_line(
                &session,
                &app,
                Command::Reflection {
                    cmd: parse(["journal", "show"], &["nope"]),
                },
                &mut io,
            )
            .await,
        ];
        assert!(codes[..2].iter().all(|&c| c == 0), "add and list succeed");
        assert_eq!(codes[2], 1, "the typed miss fails");
    }
    app.shutdown().await;

    let lines: Vec<&[u8]> = out
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 2, "two stdout envelopes");
    let added: serde_json::Value =
        serde_json::from_slice(lines[0]).expect("the add envelope parses");
    assert_eq!(added["created"], serde_json::Value::Bool(true));
    assert_eq!(added["entry"]["text"], "tea at two");
    assert_eq!(added["entry"]["cadence"], "day");
    assert_eq!(added["entry"]["mood"], serde_json::Value::Null);
    let listed: serde_json::Value =
        serde_json::from_slice(lines[1]).expect("the list envelope parses");
    assert_eq!(
        listed["entries"][0]["entry_id"], added["entry"]["entry_id"],
        "the listed row is the added row"
    );
    let miss: serde_json::Value = serde_json::from_str(String::from_utf8_lossy(&err).trim())
        .expect("the miss rides the typed error envelope");
    assert_eq!(
        miss["error"]["kind"], "unknown-reflection-entry",
        "the typed discriminator, not prose"
    );
}

/// J15: session-lane parity — the same receipts the one-shot lane
/// returns arrive through `dispatch_line` (the two-lane equivalence
/// pin, the `cli_timers.rs` T-series shape).
#[tokio::test]
async fn j15_session_lane_matches_the_one_shot_lane() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j15.db");

    // The one-shot lane seeds two entries on fixed days.
    let one = run_add(&config, &["--day", "2026-11-03", "one"]).await;
    let two = run_add(&config, &["--day", "2026-11-01", "two"]).await;
    let one_row = one.entry.as_ref().expect("row");
    let two_row = two.entry.as_ref().expect("row");

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

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: false,
        };
        let code = dispatch_line(
            &session,
            &app,
            Command::Reflection {
                cmd: parse(["journal", "edit"], &[&two_row.entry_id, "--mood", "5"]),
            },
            &mut io,
        )
        .await;
        assert_eq!(
            code,
            0,
            "the session edit lands: {}",
            String::from_utf8_lossy(&err)
        );
        let code = dispatch_line(
            &session,
            &app,
            Command::Reflection {
                cmd: parse(["journal", "delete"], &[&one_row.entry_id]),
            },
            &mut io,
        )
        .await;
        assert_eq!(
            code,
            0,
            "the session delete lands: {}",
            String::from_utf8_lossy(&err)
        );
    }
    app.shutdown().await;

    // The edits landed in the same engine: the fresh-boot list sees the
    // session lane's mood edit and the tombstone (durable across the
    // lanes — the daemon-owns-the-rows contract).
    let listed = run_list(&config, &[]).await.entries;
    assert_eq!(listed.len(), 1, "the tombstone is out of the live set");
    assert_eq!(listed[0].entry_id, two_row.entry_id);
    assert_eq!(listed[0].mood, Some(5), "the session-lane edit landed");
}

/// Runs one `export …` argv through the session lane, returning the
/// raw stdout bytes (the payload lane — never an envelope).
async fn run_export_bytes(
    session: &impl taskboard_app::commands::CoreSession,
    app: &taskboard_app::App,
    io_out: &mut Vec<u8>,
    args: &[&str],
) -> u8 {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(args);
    let cli = Cli::try_parse_from(cmdline).expect("the export grammar accepts the argv");
    let Some(Command::Export { .. }) = cli.command else {
        panic!("the argv selects export");
    };
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: io_out,
            err: &mut err,
            json: true,
        };
        taskboard_app::commands::dispatch_line(session, app, cli.command.unwrap(), &mut io).await
    }
}

/// J16: the byte-compat pin — bare `export` and explicit
/// `--what tracking` produce byte-identical stdout over the same
/// state (CSV and JSON both); the default arm is the same code path,
/// structurally.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one seed per lane, kept flat
async fn j16_bare_export_is_byte_compatible_with_explicit_tracking() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j16.db");
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

    // Seed one tracking entry (a capture plus a fixed-stamp log) so
    // the tracking payload carries a row, and one journal entry.
    let mut out = Vec::new();
    {
        let mut err = Vec::new();
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let code = taskboard_app::commands::dispatch_line(
            &session,
            &app,
            Command::Capture {
                title: "seed task".into(),
                board: None,
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 0);
    }
    {
        let mut err = Vec::new();
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let cli = Cli::try_parse_from([
            "taskboard",
            "track",
            "log",
            "seed task",
            "--from",
            "2026-04-01T08:00:00Z",
            "--to",
            "2026-04-01T09:30:00Z",
        ])
        .expect("the track grammar accepts the argv");
        let code =
            taskboard_app::commands::dispatch_line(&session, &app, cli.command.unwrap(), &mut io)
                .await;
        assert_eq!(code, 0);
    }
    {
        let mut err = Vec::new();
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        let cli = Cli::try_parse_from([
            "taskboard",
            "journal",
            "add",
            "--day",
            "2026-11-03",
            "the",
            "journal",
            "row",
        ])
        .expect("the journal grammar accepts the argv");
        let code =
            taskboard_app::commands::dispatch_line(&session, &app, cli.command.unwrap(), &mut io)
                .await;
        assert_eq!(code, 0);
    }

    for format in ["csv", "json"] {
        let mut bare = Vec::new();
        let code =
            run_export_bytes(&session, &app, &mut bare, &["export", "--format", format]).await;
        assert_eq!(code, 0);
        let mut explicit = Vec::new();
        let code = run_export_bytes(
            &session,
            &app,
            &mut explicit,
            &["export", "--format", format, "--what", "tracking"],
        )
        .await;
        assert_eq!(code, 0);
        assert_eq!(
            bare, explicit,
            "default == explicit --what tracking, byte-for-byte ({format})"
        );
        assert!(!bare.is_empty(), "the tracking payload carries the row");
    }
    app.shutdown().await;
}

/// J17: the journal CSV golden — the ruled header, fixed-stamp rows
/// byte-for-byte (a full row with ratings, flags and quoted text; a
/// minimal row with empty cells), a tombstoned row absent, and the
/// `--since`/`--until` window-start edges boundary-inclusive. The
/// goldens are data, not labels: the only words are the ruled column
/// list and the entry payloads the test wrote itself.
#[tokio::test]
#[allow(clippy::too_many_lines)] // the golden's rows are spelled out
async fn j17_journal_csv_golden() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j17.db");
    let full = run_add(
        &config,
        &[
            "--day",
            "2026-11-02",
            "--mood",
            "3",
            "--energy",
            "4",
            "--flag",
            "migraine",
            "--flag",
            "poor-sleep",
            "a, \"quoted\" day",
        ],
    )
    .await;
    let full = full.entry.as_ref().expect("row");
    // The minimal row (no ratings, no flags, empty text) is reachable
    // only through an edit — a bare add with no text is the accepted
    // no-op (J5).
    let seeded = run_add(&config, &["--day", "2026-11-03", "seed"]).await;
    let minimal = run_edit(
        &config,
        &["--clear-text", &seeded.entry.as_ref().unwrap().entry_id],
    )
    .await
    .expect("the text clears")
    .entry;

    // A tombstone never exports (the live journal's data only).
    run_delete(&config, &[&minimal.entry_id])
        .await
        .expect("deletes");

    let query = taskboard_app::commands::track::ExportQuery {
        #[cfg(feature = "pdf-export")]
        layout: None,
        #[cfg(feature = "pdf-export")]
        out: None,
        #[cfg(feature = "pdf-export")]
        no_open: false,
        what: taskboard_app::cli::ExportWhat::Reflection,
        format: Some(taskboard_app::cli::ExportFormat::Csv),
        since: None,
        until: None,
        task: None,
        board: None,
    };
    let payload =
        taskboard_app::commands::track::export(config.clone(), forbidden_store(), query.clone())
            .await
            .expect("the export boots");
    let taskboard_app::commands::track::ExportPayload::Reflection { rows, .. } = &payload else {
        panic!("the journal leg yields journal rows");
    };
    assert_eq!(rows.len(), 1, "the tombstone is skipped");

    // The golden: assembled from the receipt's own state-truth fields
    // (fixed days, RFC-3339 stamps), one byte-exact block.
    // The flags cell is the JSON-array text (the storage encoding),
    // RFC-4180-quoted as a whole because it embeds quotes.
    let flags_cell = r#""[""migraine"",""poor-sleep""]""#;
    let expected = format!(
        "entry_id,day,cadence,window_start,window_end,mood,energy,flags,text,created_at,updated_at\n\
         {},2026-11-02,day,{},{},3,4,{},\"a, \"\"quoted\"\" day\",{},{}\n",
        full.entry_id,
        full.window_start.to_rfc3339(),
        full.window_end.to_rfc3339(),
        flags_cell,
        full.created_at.to_rfc3339(),
        full.updated_at.to_rfc3339(),
    );
    assert_eq!(
        taskboard_app::commands::track::render_payload(&payload),
        expected,
        "the journal CSV golden"
    );

    // The day edges are inclusive on window_start: `until` naming the
    // full row's own UTC day keeps it; the next day drops it.
    let day = full.window_start.date_naive().to_string();
    let query = taskboard_app::commands::track::ExportQuery {
        #[cfg(feature = "pdf-export")]
        layout: None,
        #[cfg(feature = "pdf-export")]
        out: None,
        #[cfg(feature = "pdf-export")]
        no_open: false,
        since: Some(full.window_start.date_naive()),
        until: Some(full.window_start.date_naive()),
        ..query.clone()
    };
    let payload =
        taskboard_app::commands::track::export(config.clone(), forbidden_store(), query.clone())
            .await
            .expect("the filtered export boots");
    let taskboard_app::commands::track::ExportPayload::Reflection { rows, .. } = &payload else {
        panic!("the journal leg yields journal rows");
    };
    assert_eq!(rows.len(), 1, "the boundary day lands inside");
    let _ = day;
    let query = taskboard_app::commands::track::ExportQuery {
        #[cfg(feature = "pdf-export")]
        layout: None,
        #[cfg(feature = "pdf-export")]
        out: None,
        #[cfg(feature = "pdf-export")]
        no_open: false,
        since: Some(full.window_start.date_naive().succ_opt().unwrap()),
        until: None,
        ..query.clone()
    };
    let payload =
        taskboard_app::commands::track::export(config.clone(), forbidden_store(), query.clone())
            .await
            .expect("the filtered export boots");
    let taskboard_app::commands::track::ExportPayload::Reflection { rows, .. } = &payload else {
        panic!("the journal leg yields journal rows");
    };
    assert!(rows.is_empty(), "the next day drops the entry");
}

/// J18: the journal JSON golden — a single-line array, `null` unset
/// ratings, real flag arrays, and the eleven ruled keys exactly (the
/// key-set pin; `extra_flags` is absent by ruling).
#[tokio::test]
async fn j18_journal_json_golden() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j18.db");
    let full = run_add(
        &config,
        &[
            "--day",
            "2026-11-02",
            "--mood",
            "3",
            "--flag",
            "migraine",
            "body",
        ],
    )
    .await;
    let full = full.entry.as_ref().expect("row");
    let seeded = run_add(&config, &["--day", "2026-11-03", "seed"]).await;
    let minimal = run_edit(
        &config,
        &["--clear-text", &seeded.entry.as_ref().unwrap().entry_id],
    )
    .await
    .expect("the text clears")
    .entry;

    let query = taskboard_app::commands::track::ExportQuery {
        #[cfg(feature = "pdf-export")]
        layout: None,
        #[cfg(feature = "pdf-export")]
        out: None,
        #[cfg(feature = "pdf-export")]
        no_open: false,
        what: taskboard_app::cli::ExportWhat::Reflection,
        format: Some(taskboard_app::cli::ExportFormat::Json),
        since: None,
        until: None,
        task: None,
        board: None,
    };
    let payload =
        taskboard_app::commands::track::export(config.clone(), forbidden_store(), query.clone())
            .await
            .expect("the export boots");
    let rendered = taskboard_app::commands::track::render_payload(&payload);
    let parsed: serde_json::Value =
        serde_json::from_str(rendered.trim()).expect("the payload parses");
    let rows_json = parsed.as_array().expect("a JSON array");

    let ruled = [
        "entry_id",
        "day",
        "cadence",
        "window_start",
        "window_end",
        "mood",
        "energy",
        "flags",
        "text",
        "created_at",
        "updated_at",
    ];
    assert_eq!(rows_json.len(), 2);
    for (row, expected_row) in rows_json.iter().zip([full, &minimal]) {
        let mut keys: Vec<&str> = row
            .as_object()
            .expect("row objects")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut ruled_sorted = ruled.to_vec();
        ruled_sorted.sort_unstable();
        assert_eq!(keys, ruled_sorted, "the ruled eleven keys exactly");
        assert_eq!(row["entry_id"], expected_row.entry_id);
    }
    assert_eq!(rows_json[0]["mood"], 3);
    assert_eq!(
        rows_json[0]["flags"],
        serde_json::Value::Array(vec![serde_json::Value::String("migraine".into())]),
        "flags are a real array, not a stringified one"
    );
    assert_eq!(rows_json[1]["mood"], serde_json::Value::Null);
    assert_eq!(rows_json[1]["flags"], serde_json::Value::Array(vec![]));
    assert!(
        rows_json[0].get("extra_flags").is_none(),
        "extra_flags is deliberately not exportable (Q-11)"
    );
}

/// J19: tracking-only filters under the journal leg are a typed
/// rejection; the tracking leg keeps accepting them.
#[tokio::test]
async fn j19_filters_not_applicable_is_typed() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j19.db");
    let task = taskboard_app::commands::track::ExportQuery {
        #[cfg(feature = "pdf-export")]
        layout: None,
        #[cfg(feature = "pdf-export")]
        out: None,
        #[cfg(feature = "pdf-export")]
        no_open: false,
        what: taskboard_app::cli::ExportWhat::Reflection,
        format: None,
        since: None,
        until: None,
        task: Some("no such task".into()),
        board: None,
    };
    let err =
        taskboard_app::commands::track::export(config.clone(), forbidden_store(), task.clone())
            .await
            .expect_err("the journal leg refuses --task");
    assert_eq!(err.error_kind(), "filters-not-applicable");

    let board = taskboard_app::commands::track::ExportQuery {
        what: taskboard_app::cli::ExportWhat::Reflection,
        board: Some("no such board".into()),
        ..task
    };
    let err = taskboard_app::commands::track::export(config.clone(), forbidden_store(), board)
        .await
        .expect_err("the journal leg refuses --board");
    assert_eq!(err.error_kind(), "filters-not-applicable");
}

/// J20's stdout half, pre-relay: `export --what journal` under
/// `--json` still writes the raw payload on stdout — the envelope
/// never wraps it (the byte-verbatim relay's source side).
#[tokio::test]
async fn j20_journal_export_is_raw_under_json() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "j20.db");
    let receipt = run_add(&config, &["raw", "payload"]).await;
    let row = receipt.entry.as_ref().expect("row");

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
    let mut out = Vec::new();
    let code = run_export_bytes(
        &session,
        &app,
        &mut out,
        &["--json", "export", "--what", "journal", "--format", "csv"],
    )
    .await;
    app.shutdown().await;
    assert_eq!(code, 0);
    let rendered = String::from_utf8(out).expect("utf8 stdout");
    assert!(
        rendered.starts_with(
            "entry_id,day,cadence,window_start,window_end,mood,energy,flags,text,created_at,updated_at\n"
        ),
        "the raw CSV header rides stdout even under --json"
    );
    assert!(
        rendered.contains(&row.entry_id),
        "the payload carries the row"
    );
    assert!(
        !rendered.contains("filters-not-applicable") && !rendered.contains("\"created\""),
        "no receipt envelope wrapped the payload"
    );
}
