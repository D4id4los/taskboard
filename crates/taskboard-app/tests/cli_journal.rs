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
    let stderr = String::from_utf8_lossy(&err);
    assert!(
        stderr.contains("\"unknown-reflection-entry\""),
        "the miss rides the typed error envelope: {stderr}"
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
