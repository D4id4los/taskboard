// SPDX-License-Identifier: AGPL-3.0-only
//! H-series: the `habits` verb family over hermetic standalone
//! tempdirs (no wiremock — a standalone core performs no network I/O).
//! One invocation is a full boot/work/shutdown "process"; durability
//! is proven across invocations on one db. Receipts are asserted as
//! typed values; refusals via `matches!`/`error_kind()`, never
//! message text.

use std::sync::Arc;

use clap::Parser;

use taskboard_app::cli::{Cli, Command};
use taskboard_app::commands::CommandError;
use taskboard_app::commands::boards;
use taskboard_app::commands::habits::{self, HabitsInvocation};
use taskboard_app::commands::output::{
    HabitAddReceipt, HabitAdjustReceipt, HabitDeleteReceipt, HabitListView, HabitLogReceipt,
    HabitUpdateReceipt,
};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::CredentialStore;

mod common;

use common::tempdir;

/// A standalone config over a tempdir db: no server, no username —
/// the fresh-install shape.
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

    let logged = run_one(&config, parse(&["habits", "log", &selector]))
        .await
        .expect("log");
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
    // The window is the UTC-midnight calendar's today (the standalone
    // default config): the receipt's window is that date.
    let today = chrono::Utc::now().date_naive();
    assert_eq!(*window, today, "the natural daily window is today");

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
            &today.to_string(),
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
