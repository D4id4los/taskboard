// SPDX-License-Identifier: AGPL-3.0-only
//! T-series: the `timers` verb family over hermetic standalone
//! tempdirs (no wiremock — a standalone core performs no network I/O).
//! One invocation is a full boot/work/shutdown "process"; durability is
//! proven across invocations on one db. Receipts are asserted as typed
//! values; error kinds via `error_kind()`, never message text.

use std::sync::Arc;

use chrono::Utc;
use clap::Parser;

use taskboard_app::cli::{Cli, Command, TimersCmd};
use taskboard_app::commands::output::{TimerAddReceipt, TimerCancelReceipt, TimersList};
use taskboard_app::commands::timers::{self, TimersInvocation};
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

/// The `timers list` payload over a fresh boot (the persistence read).
async fn list(config: &AppConfig) -> TimersList {
    timers::list(config.clone(), forbidden_store())
        .await
        .expect("list boots")
}

/// T1: `timers add --in` stamps the fire instant from the engine clock
/// (`fire_at` is state truth, not an input echo), the receipt names the
/// fresh alarm, and a fresh boot's `list` sees it (durable).
#[tokio::test]
async fn t1_add_in_stamps_and_persists() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t1.db");
    let before = chrono::Utc::now();
    let receipt: TimerAddReceipt = match timers::collapse(TimersCmd::Add {
        timer_label: "steep the tea".into(),
        in_: Some(chrono::TimeDelta::try_minutes(30).unwrap()),
        at: None,
    })
    .expect("one invocation")
    {
        TimersInvocation::Add { label, when } => {
            timers::add(config.clone(), forbidden_store(), &label, when)
                .await
                .expect("add boots")
        }
        _ => panic!("collapse yields the add"),
    };
    let now = chrono::Utc::now();
    assert_eq!(receipt.label, "steep the tea");
    assert!(
        receipt.fire_at - chrono::TimeDelta::try_minutes(30).unwrap() >= before
            && receipt.fire_at - chrono::TimeDelta::try_minutes(30).unwrap() <= now,
        "fire_at = (boot-clock) now + 30m, stamped inside [before, now]"
    );

    let rows = list(&config).await.timers;
    assert_eq!(rows.len(), 1, "durable across a fresh boot");
    assert_eq!(rows[0].alarm_id, receipt.alarm_id);
    assert_eq!(rows[0].fire_at, receipt.fire_at);
    assert_eq!(rows[0].effective_at, receipt.fire_at, "no chain yet");
    assert!(!rows[0].overdue, "future alarm is not overdue");
    assert!(rows[0].remaining.is_some(), "remaining time is positive");
}

/// T2: `--at` accepts the three grammars — an RFC-3339 instant is
/// verbatim; a bare `HH:MM` resolves host-local and rolls to tomorrow
/// when already past (strictly future); `YYYY-MM-DD` resolves to that
/// date's host-local midnight.
#[tokio::test]
async fn t2_at_grammars() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t2.db");

    // (a) RFC-3339 verbatim.
    let receipt = run_add(&config, "rfc", &["--at", "2027-01-15T09:30:00Z"]).await;
    assert_eq!(
        receipt.fire_at,
        chrono::DateTime::parse_from_rfc3339("2027-01-15T09:30:00Z")
            .expect("rfc3339")
            .with_timezone(&Utc),
        "rfc3339 is verbatim"
    );

    // (b) HH:MM host-local, rolled forward when already past: the
    // instant's local wall clock equals the requested time and it is
    // strictly in the future.
    let receipt = run_add(&config, "time", &["--at", "09:30"]).await;
    let local: chrono::DateTime<chrono::Local> = receipt.fire_at.into();
    assert_eq!(local.format("%H:%M").to_string(), "09:30");
    assert!(receipt.fire_at > chrono::Utc::now(), "rolled to the future");

    // (c) YYYY-MM-DD host-local midnight: the local date matches and
    // the wall clock is 00:00.
    let receipt = run_add(&config, "date", &["--at", "2030-03-01"]).await;
    let local: chrono::DateTime<chrono::Local> = receipt.fire_at.into();
    assert_eq!(local.format("%Y-%m-%d").to_string(), "2030-03-01");
    assert_eq!(local.format("%H:%M").to_string(), "00:00");
}

/// Runs one `timers add --at …` through the real clap parse and the
/// one-shot lane, returning the receipt.
async fn run_add(config: &AppConfig, label: &str, args: &[&str]) -> TimerAddReceipt {
    let mut cmdline = vec!["taskboard", "timers", "add", label];
    cmdline.extend_from_slice(args);
    let cli = Cli::try_parse_from(&cmdline).expect("the flag grammar accepts the value");
    let Some(Command::Timers { cmd }) = cli.command else {
        panic!("the argv selects the timers group");
    };
    match timers::collapse(cmd).expect("add collapses") {
        TimersInvocation::Add { label, when } => {
            timers::add(config.clone(), forbidden_store(), &label, when)
                .await
                .expect("add boots")
        }
        _ => panic!("the argv selects add"),
    }
}

/// T3: `list` orders rows canonically `(fire_at, id)` and is a pure
/// read (empty on a fresh db).
#[tokio::test]
async fn t3_list_empty_then_canonical_order() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t3.db");
    assert_eq!(list(&config).await.timers.len(), 0, "fresh db is empty");

    let a = run_add(&config, "later", &["--in", "2h"]).await;
    let b = run_add(&config, "sooner", &["--in", "10m"]).await;
    let rows = list(&config).await.timers;
    assert_eq!(
        rows.iter().map(|r| r.alarm_id.as_str()).collect::<Vec<_>>(),
        vec![b.alarm_id, a.alarm_id],
        "canonical (fire_at, id) order, not insertion order"
    );
}

/// T4: cancel resolves all three selector kinds; a repeat is the typed
/// unknown-alarm refusal (terminal-by-deletion); ambiguous prefixes
/// refuse with the candidates; the receipt projects the pre-execute
/// row.
#[tokio::test]
async fn t4_cancel_selectors_and_typed_refusals() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t4.db");
    let a = run_add(&config, "unique-tea", &["--in", "1h"]).await;
    let b = run_add(&config, "unique-toast", &["--in", "2h"]).await;

    // (a) full uuid.
    let receipt = run_cancel(&config, &a.alarm_id).await.expect("uuid hit");
    assert_eq!(receipt.alarm_id, a.alarm_id);
    assert_eq!(receipt.label, "unique-tea");
    // The repeat is terminal-by-deletion: typed unknown.
    let err = run_cancel(&config, &a.alarm_id)
        .await
        .expect_err("a repeat targets nothing");
    assert!(matches!(err, CommandError::UnknownAdhocAlarm(_)));

    // (b) unique ≥8-char uuid prefix.
    let prefix = b.alarm_id[..8].to_owned();
    let receipt = run_cancel(&config, &prefix).await.expect("prefix hit");
    assert_eq!(receipt.alarm_id, b.alarm_id);

    // (c) case-folded label prefix.
    let c = run_add(&config, "Water the plants", &["--in", "3h"]).await;
    let receipt = run_cancel(&config, "water THE").await.expect("label hit");
    assert_eq!(receipt.alarm_id, c.alarm_id);

    assert!(list(&config).await.timers.is_empty(), "all cancelled");
}

async fn run_cancel(
    config: &AppConfig,
    selector: &str,
) -> Result<TimerCancelReceipt, CommandError> {
    timers::cancel(config.clone(), forbidden_store(), selector).await
}

/// T5: ambiguous selectors refuse typed with the candidates; misses are
/// the unknown-alarm kind.
#[tokio::test]
async fn t5_selector_errors_keep_their_kinds() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t5.db");
    let a = run_add(&config, "tea early", &["--in", "1h"]).await;
    let b = run_add(&config, "tea late", &["--in", "2h"]).await;

    // Same 8-char prefix is unlikely, but same label prefix "tea" hits
    // both: ambiguous with candidates.
    let err = timers::cancel(config.clone(), forbidden_store(), "tea")
        .await
        .expect_err("two labels share the prefix");
    let CommandError::AmbiguousAdhocAlarm { candidates } = err else {
        panic!("the shared prefix is typed ambiguous");
    };
    assert_eq!(candidates.len(), 2);
    assert!(candidates.contains(&a.alarm_id) && candidates.contains(&b.alarm_id));

    let err = timers::cancel(config.clone(), forbidden_store(), "nope")
        .await
        .expect_err("a miss");
    assert!(matches!(err, CommandError::UnknownAdhocAlarm(sel) if sel == "nope"));
}

/// T6: the clap rules — exactly one of `--in`/`--at`; a malformed
/// `--at` is a usage error (exit 2), never a boot.
#[test]
fn t6_clap_mode_and_value_rules() {
    // Both flags: usage error.
    let both = Cli::try_parse_from([
        "taskboard",
        "timers",
        "add",
        "tea",
        "--in",
        "10m",
        "--at",
        "09:30",
    ]);
    assert!(both.is_err(), "in/at conflict");

    // Neither: usage error (the required group).
    let neither = Cli::try_parse_from(["taskboard", "timers", "add", "tea"]);
    assert!(neither.is_err(), "one of --in/--at is required");

    // Garbage --at: usage error.
    let garbage = Cli::try_parse_from(["taskboard", "timers", "add", "tea", "--at", "soonish"]);
    assert!(garbage.is_err(), "the grammar rejects the value");
}

/// T7: the session lane routes the timers verbs over the `*_on` seams
/// (no boot churn between lines).
#[tokio::test]
async fn t7_session_lane_routes_timers() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t7.db");
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
    let mut err = Vec::new();
    {
        let mut io = CommandIo {
            out: &mut out,
            err: &mut err,
            json: true,
        };
        for (cmd, expected) in [
            (
                TimersCmd::Add {
                    timer_label: "tea".into(),
                    in_: Some(chrono::TimeDelta::try_minutes(5).unwrap()),
                    at: None,
                },
                0,
            ),
            (TimersCmd::List {}, 0),
            (
                TimersCmd::Cancel {
                    selector: "tea".into(),
                },
                0,
            ),
            // The repeat is terminal-by-deletion: typed refusal, the
            // typed-error exit code (the same table the reminders use).
            (
                TimersCmd::Cancel {
                    selector: "tea".into(),
                },
                1,
            ),
        ] {
            let code = dispatch_line(&session, &app, Command::Timers { cmd }, &mut io).await;
            assert_eq!(code, expected, "stderr: {}", String::from_utf8_lossy(&err));
        }
    }
    app.shutdown().await;

    // Three stdout JSON lanes (add, list, cancel); the refusal is an
    // error envelope on stderr, typed, not stdout text.
    let lines: Vec<&[u8]> = out
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 3);
    let stderr_text = String::from_utf8_lossy(&err);
    assert!(stderr_text.contains("\"unknown-adhoc-alarm\""));
}
