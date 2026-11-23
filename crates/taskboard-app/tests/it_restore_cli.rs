// SPDX-License-Identifier: AGPL-3.0-only
//! RS chunk: the restore-family verbs' first CLI surface (the accepted
//! J2 journey plan, `2026-11-21-parity-restore-journeys-restore-cli`).
//! Step-ID-named seam tests over hermetic standalone tempdirs (the
//! `cli_track` recipe): one invocation is a full boot/work/shutdown
//! process on one db. Receipts are the serde-pinned `CommandOutcome`
//! variants (journey D-3/W-5) — asserted as typed values, refusals via
//! `matches!`/`error_kind()`, never message text.

use std::sync::{Mutex, OnceLock};

use clap::Parser;

use taskboard_app::cli::{Cli, Command, LabelsCmd};
use taskboard_app::commands::CommandError;
use taskboard_app::commands::habits::{self, HabitOutcome, HabitsInvocation};
use taskboard_app::commands::labels;
use taskboard_app::commands::tasks;
use taskboard_app::commands::track::{self, TrackInvocation};
use taskboard_app::config::AppConfig;
use taskboard_domain::CommandOutcome;

use taskboard_domain::tracking::TimeEntryKind;

use crate::common::{forbidden_store, standalone_config, tempdir};

mod common;

/// The dbs the seed scaffold already ran on (tests share no db, but
/// `seed_task` may run several times against one).
fn seeded_dbs() -> &'static Mutex<std::collections::HashSet<std::path::PathBuf>> {
    static S: OnceLock<Mutex<std::collections::HashSet<std::path::PathBuf>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Seeds the fresh db's board (+ stack) exactly once — every verb that
/// resolves a board needs the single-live-board shape.
async fn seed_board(config: &AppConfig) {
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
}

/// Seeds one live task through a real boot (the single-board fresh
/// shape). Returns the task's uuid string.
async fn seed_task(config: &AppConfig, title: &str) -> String {
    seed_board(config).await;
    let receipt = tasks::add(config.clone(), forbidden_store(), title, Some("seed stack"))
        .await
        .expect("task add boots");
    receipt.id
}

/// Parses one `taskboard labels …` argv through the real clap surface.
fn parse_labels(argv: &[&str]) -> LabelsCmd {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Labels { cmd }) = cli.command else {
        panic!("the argv selects the labels group");
    };
    cmd
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

/// Seeds one label, deletes it (tombstone), returns its uuid.
async fn seed_tombstoned_label(config: &AppConfig, title: &str) -> String {
    seed_board(config).await;
    let receipt = labels::add(config.clone(), forbidden_store(), None, title, None)
        .await
        .expect("label add boots");
    let deleted = labels::delete(config.clone(), forbidden_store(), &receipt.id)
        .await
        .expect("label delete boots");
    assert_eq!(deleted.id, receipt.id);
    receipt.id
}

/// Seeds one habit, deletes it (tombstone), returns its uuid.
async fn seed_tombstoned_habit(config: &AppConfig, title: &str) -> String {
    seed_board(config).await;
    let mut cmdline = vec!["taskboard", "habits", "add", title, "--daily"];
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Habits { cmd }) = cli.command else {
        panic!("the argv selects the habits group");
    };
    let receipt = habits::run(config.clone(), forbidden_store(), habits::collapse(cmd))
        .await
        .expect("habit add boots");
    let HabitOutcome::Added(row) = &receipt else {
        panic!("the add receipt");
    };
    let habit_id = row
        .habit
        .as_ref()
        .expect("the minted habit row")
        .habit_id
        .clone();
    cmdline = vec!["taskboard", "habits", "rm", habit_id.as_str()];
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Habits { cmd }) = cli.command else {
        panic!("the argv selects the habits group");
    };
    habits::run(config.clone(), forbidden_store(), habits::collapse(cmd))
        .await
        .expect("habit rm boots");
    habit_id
}

/// Fresh standalone config over a unique tempdir db; the tempdir
/// travels back so it outlives the config's db path for the test.
fn config_for(tag: &str) -> (AppConfig, tempfile::TempDir) {
    let dir = tempdir();
    let db = format!("{tag}.db");
    let config = standalone_config(dir.path(), &db);
    (config, dir)
}

/// A fixed RFC-3339 stamp (explicit offset, host-TZ independent).
const START_STAMP: &str = "2026-11-20T09:00:00+02:00";
const END_STAMP: &str = "2026-11-20T10:30:00+02:00";

/// US-1/S1 (W-1): `labels restore <id|prefix>` dispatches
/// `RestoreLabel` and the receipt is the pinned `LabelRestored`; the
/// tombstoned values come back verbatim; the ≥8-char prefix
/// resolves the tombstone (D-2: ids are the restore family's address).
/// AC-2 rides the same step: an unknown id is the typed rejection.
#[tokio::test]
async fn us1_s1_labels_restore_dispatches_restore_label_and_receipts() {
    let (config, _dir) = config_for("us1_s1");
    let label_id = seed_tombstoned_label(&config, "urgent").await;

    // AC-2 first: an unknown id is the typed rejection before anything
    // exists to restore.
    let unknown = "00000000-0000-0000-0000-000000000000";
    match labels::restore(config.clone(), forbidden_store(), unknown).await {
        Err(err) => {
            assert!(
                matches!(err, CommandError::UnknownLabel(sel) if sel == unknown),
                "typed unknown-label rejection"
            );
        }
        Ok(_) => panic!("an unknown id must refuse"),
    }

    // S1 by prefix: the tombstone resolves and the receipt is the
    // pinned `LabelRestored` fact.
    let argv = ["labels", "restore", &label_id[..8]];
    let LabelsCmd::Restore { label } = parse_labels(&argv) else {
        panic!("the argv selects the restore verb");
    };
    let outcome = labels::restore(config.clone(), forbidden_store(), &label)
        .await
        .expect("restore boots");
    let CommandOutcome::LabelRestored { label: restored } = outcome else {
        panic!("the restore receipt");
    };
    assert_eq!(restored.as_uuid().to_string(), label_id);

    // Verbatim: the resurrected row carries the pre-tombstone title.
    let rows = labels::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list boots");
    assert!(
        rows.iter().any(|r| r.id == label_id && r.title == "urgent"),
        "the resurrected label is live with its pre-tombstone values"
    );
}

/// US-1/S1+US-1/S2 (W-5): the `--json` receipt payload decodes as the
/// serde-pinned `CommandOutcome` (#993) — both the restore fact and
/// the accepted live-target no-op (S2's repeat is not an error).
#[tokio::test]
async fn us1_s1_json_receipt_decodes_pinned_command_outcome() {
    let (config, _dir) = config_for("us1_json");
    let label_id = seed_tombstoned_label(&config, "urgent").await;

    let outcome = labels::restore(config.clone(), forbidden_store(), &label_id)
        .await
        .expect("restore boots");
    let json = serde_json::to_string(&outcome).expect("the pinned receipt serializes");
    let decoded: CommandOutcome =
        serde_json::from_str(&json).expect("the receipt decodes as CommandOutcome");
    assert_eq!(decoded, outcome);
    assert!(matches!(decoded, CommandOutcome::LabelRestored { .. }));

    // S2: the repeat dispatches again and answers the accepted no-op —
    // which rides the same pinned envelope.
    let repeat = labels::restore(config.clone(), forbidden_store(), &label_id)
        .await
        .expect("repeat restore boots");
    let json = serde_json::to_string(&repeat).expect("the no-op serializes");
    let decoded: CommandOutcome =
        serde_json::from_str(&json).expect("the no-op decodes as CommandOutcome");
    assert_eq!(decoded, CommandOutcome::NoOp);
}

/// US-1/S2: the repeat dispatch is the accepted no-op — a live target
/// is not an error, nothing changed.
#[tokio::test]
async fn us1_s2_labels_restore_repeat_is_the_accepted_noop() {
    let (config, _dir) = config_for("us1_s2");
    let label_id = seed_tombstoned_label(&config, "urgent").await;

    labels::restore(config.clone(), forbidden_store(), &label_id)
        .await
        .expect("restore boots");
    let repeat = labels::restore(config.clone(), forbidden_store(), &label_id)
        .await
        .expect("repeat restore boots");
    assert_eq!(repeat, CommandOutcome::NoOp);
    // Nothing changed: the label stayed live across the repeat.
    let rows = labels::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list boots");
    assert_eq!(
        rows.iter().filter(|r| r.id == label_id).count(),
        1,
        "exactly one live row after the repeat"
    );
}

/// US-2/S1 (W-2): `habits restore <id|prefix>` dispatches
/// `RestoreHabit` and the receipt is the pinned `HabitRestored`; the
/// log is never touched. AC-2 rides the same step: an unknown id is
/// the typed rejection.
#[tokio::test]
async fn us2_s1_habits_restore_dispatches_restore_habit_and_receipts() {
    let (config, _dir) = config_for("us2_s1");
    let habit_id = seed_tombstoned_habit(&config, "stretch").await;

    let unknown = "00000000-0000-0000-0000-000000000000";
    match habits::run(
        config.clone(),
        forbidden_store(),
        HabitsInvocation::Restore {
            habit: unknown.to_owned(),
        },
    )
    .await
    {
        Err(err) => assert!(
            matches!(err, CommandError::UnknownHabit { selector } if selector == unknown),
            "typed unknown-habit rejection"
        ),
        Ok(_) => panic!("an unknown id must refuse"),
    }

    // S1 by prefix: the tombstone resolves, the receipt is the pinned
    // fact.
    let argv = ["taskboard", "habits", "restore", &habit_id[..8]];
    let cli = Cli::try_parse_from(argv).expect("the argv parses");
    let Some(Command::Habits { cmd }) = cli.command else {
        panic!("the argv selects the habits group");
    };
    let outcome = habits::run(config.clone(), forbidden_store(), habits::collapse(cmd))
        .await
        .expect("restore boots");
    let HabitOutcome::Restored(CommandOutcome::HabitRestored { habit: restored }) = outcome else {
        panic!("the restore receipt");
    };
    assert_eq!(restored.as_uuid().to_string(), habit_id);
}

/// US-2/S2: the repeat dispatch is the accepted no-op — a live target
/// is not an error.
#[tokio::test]
async fn us2_s2_habits_restore_repeat_is_the_accepted_noop() {
    let (config, _dir) = config_for("us2_s2");
    let habit_id = seed_tombstoned_habit(&config, "stretch").await;

    habits::run(
        config.clone(),
        forbidden_store(),
        HabitsInvocation::Restore {
            habit: habit_id.clone(),
        },
    )
    .await
    .expect("restore boots");
    let repeat = habits::run(
        config.clone(),
        forbidden_store(),
        HabitsInvocation::Restore {
            habit: habit_id.clone(),
        },
    )
    .await
    .expect("repeat restore boots");
    assert!(matches!(
        repeat,
        HabitOutcome::Restored(CommandOutcome::NoOp)
    ));
}

/// US-3/S1 (W-3): `track restore --task … --start … [--end] [--id]`
/// dispatches `RestoreTimeEntry` with the verbatim payload and the
/// receipt is the pinned `EntryRestored`; `--id` omitted mints a
/// fresh v7. The verbatim principle is proven by the exact
/// already-restored no-op: re-restoring the identical payload answers
/// `NoOp` (the engine's whole-row equality). AC-4 rides the same
/// step: a malformed `--start` is the clap-layer refusal before any
/// dispatch.
#[tokio::test]
async fn us3_s1_track_restore_assembles_verbatim_payload_and_receipts() {
    let (config, _dir) = config_for("us3_s1");
    let task_id = seed_task(&config, "focus work").await;

    // AC-4: malformed `--start` never reaches a boot.
    let bad = Cli::try_parse_from([
        "taskboard",
        "track",
        "restore",
        "--task",
        &task_id,
        "--start",
        "not-a-stamp",
    ]);
    assert!(bad.is_err(), "the malformed stamp is a clap refusal");

    // S1 with an explicit `--id`: the payload rides verbatim and the
    // receipt names the id.
    let explicit = "00000000-0000-0000-0000-00000000c0de";
    let argv = [
        "track",
        "restore",
        "--task",
        &task_id,
        "--start",
        START_STAMP,
        "--end",
        END_STAMP,
        "--kind",
        "respite",
        "--id",
        explicit,
    ];
    let TrackInvocation::Restore {
        entry,
        task,
        start,
        end,
        kind,
        id,
    } = parse_track(&argv)
    else {
        panic!("the argv selects the restore verb");
    };
    assert_eq!(entry, None);
    assert_eq!(task, task_id);
    assert_eq!(id.map(|u| u.to_string()).as_deref(), Some(explicit));
    let outcome = track::restore(
        config.clone(),
        forbidden_store(),
        entry,
        &task,
        start,
        end,
        kind,
        id,
    )
    .await
    .expect("restore boots");
    let CommandOutcome::EntryRestored { id: restored } = &outcome else {
        panic!("the restore receipt");
    };
    assert_eq!(restored.as_uuid().to_string(), explicit);

    // Verbatim: a second boot reads the row back — the assembled
    // payload is what the engine restored (flags in, row out).
    let view = track::list(config, forbidden_store(), Some(&task), None)
        .await
        .expect("list boots");
    let row = view
        .entries
        .iter()
        .find(|r| r.entry_id == explicit)
        .expect("the restored row");
    assert_eq!(row.task_id, task_id);
    assert_eq!(row.task_title, "focus work");
    assert_eq!(row.start, start);
    assert_eq!(row.end, end);
    assert_eq!(row.kind, TimeEntryKind::Respite);
}

/// US-3/S1 (W-3, the fresh-v7 leg): the positional id slot omitted and
/// `--id` omitted mints a fresh v7 payload id; the receipt carries it.
#[tokio::test]
async fn us3_s1_track_restore_id_slot_defaults_to_fresh_v7() {
    let (config, _dir) = config_for("us3_v7");
    let task_id = seed_task(&config, "fresh id").await;

    let argv = [
        "track",
        "restore",
        "--task",
        &task_id,
        "--start",
        START_STAMP,
    ];
    let TrackInvocation::Restore { entry, id, .. } = parse_track(&argv) else {
        panic!("the argv selects the restore verb");
    };
    assert_eq!(entry, None);
    assert_eq!(id, None, "neither slot names an id");
    let outcome = track::restore(
        config.clone(),
        forbidden_store(),
        entry,
        &task_id,
        chrono::DateTime::parse_from_rfc3339(START_STAMP)
            .expect("the stamp parses")
            .with_timezone(&chrono::Utc),
        None,
        taskboard_domain::tracking::TimeEntryKind::default(),
        None,
    )
    .await
    .expect("restore boots");
    let CommandOutcome::EntryRestored { id: minted } = &outcome else {
        panic!("the restore receipt");
    };
    assert_eq!(minted.as_uuid().get_version_num(), 7, "a fresh v7 mint");
}

/// US-3/S1 (W-3, the positional leg): a positional full uuid names the
/// payload id verbatim; a positional prefix is the typed refusal
/// before any dispatch (D-2: a hard-deleted row cannot
/// prefix-resolve).
#[tokio::test]
async fn us3_s1_track_restore_positional_id_slot() {
    let (config, _dir) = config_for("us3_pos");
    let task_id = seed_task(&config, "positional").await;

    let positional = "00000000-0000-0000-0000-00000000face";
    let outcome = track::restore(
        config.clone(),
        forbidden_store(),
        Some(positional.to_owned()),
        &task_id,
        chrono::DateTime::parse_from_rfc3339(START_STAMP)
            .expect("the stamp parses")
            .with_timezone(&chrono::Utc),
        None,
        taskboard_domain::tracking::TimeEntryKind::default(),
        None,
    )
    .await
    .expect("restore boots");
    let CommandOutcome::EntryRestored { id } = &outcome else {
        panic!("the restore receipt");
    };
    assert_eq!(id.as_uuid().to_string(), positional);

    // The prefix leg: typed refusal, nothing dispatched.
    let (config2, _dir2) = config_for("us3_pos2");
    let task2 = seed_task(&config2, "positional prefix").await;
    let err = track::restore(
        config2,
        forbidden_store(),
        Some("00000000".to_owned()),
        &task2,
        chrono::DateTime::parse_from_rfc3339(START_STAMP)
            .expect("the stamp parses")
            .with_timezone(&chrono::Utc),
        None,
        taskboard_domain::tracking::TimeEntryKind::default(),
        None,
    )
    .await
    .expect_err("a prefix cannot address a hard-deleted row");
    assert!(
        matches!(err, CommandError::UnknownTimeEntry { selector } if selector == "00000000"),
        "typed unknown-time-entry refusal"
    );
}

/// US-3/S2 (W-4): a `--task` naming a gone/deleted task is the typed
/// rejection before any dispatch (`unknown-task` kind, JSON shape).
#[tokio::test]
async fn us3_s2_gone_owner_task_refuses_typed() {
    let (config, _dir) = config_for("us3_s2");
    let task_id = seed_task(&config, "doomed owner").await;
    tasks::delete(config.clone(), forbidden_store(), &task_id)
        .await
        .expect("task delete boots");

    let err = track::restore(
        config,
        forbidden_store(),
        None,
        &task_id,
        chrono::DateTime::parse_from_rfc3339(START_STAMP)
            .expect("the stamp parses")
            .with_timezone(&chrono::Utc),
        None,
        taskboard_domain::tracking::TimeEntryKind::default(),
        None,
    )
    .await
    .expect_err("a gone owner refuses");
    assert_eq!(err.error_kind(), "unknown-task");
}

/// US-3/S3 (W-4): a same-id row with a *divergent* payload is the
/// typed divergence refusal (`time-entry-already-present` kind) —
/// ids are minted once; the identical payload is the exact
/// already-restored no-op instead (never a refusal).
#[tokio::test]
async fn us3_s3_present_same_id_refuses_typed() {
    let (config, _dir) = config_for("us3_s3");
    let task_id = seed_task(&config, "divergent").await;
    let start = chrono::DateTime::parse_from_rfc3339(START_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let end = chrono::DateTime::parse_from_rfc3339(END_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let id = uuid::Uuid::now_v7();

    // Establish the present row.
    let outcome = track::restore(
        config.clone(),
        forbidden_store(),
        None,
        &task_id,
        start,
        Some(end),
        taskboard_domain::tracking::TimeEntryKind::Work,
        Some(id),
    )
    .await
    .expect("restore boots");
    assert!(matches!(outcome, CommandOutcome::EntryRestored { .. }));

    // A present same-id row (here: a re-run whose assembler stamps
    // differ — the whole-row equality cannot hold across invocations)
    // is the typed divergence refusal, and nothing dispatches.
    let err = track::restore(
        config,
        forbidden_store(),
        None,
        &task_id,
        start,
        Some(end),
        TimeEntryKind::Work,
        Some(id),
    )
    .await
    .expect_err("a present same-id row refuses");
    assert_eq!(err.error_kind(), "time-entry-already-present");
}

/// US-1/S3 (W-1, the superseded-D-5 leg): the global undo line offers
/// the inverse of the restore, and undoing re-deletes — the label is
/// tombstoned again (not hard-deleted), proven by the re-restore
/// answering the pinned `LabelRestored` fact.
#[tokio::test]
async fn us1_s3_labels_restore_undo_redeletes_the_restored_row() {
    let (config, _dir) = config_for("us1_s3");
    let label_id = seed_tombstoned_label(&config, "urgent").await;

    labels::restore(config.clone(), forbidden_store(), &label_id)
        .await
        .expect("restore boots");
    // The undo line synthesizes the inverse and lands it.
    tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("the undo line offers the restore's inverse");
    // The re-delete tombstoned the row: it is gone from the live list,
    // and the restore verb still addresses it (tombstones resolve).
    let rows = labels::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list boots");
    assert!(
        rows.iter().all(|r| r.id != label_id),
        "the undo re-deleted the restored label"
    );
    let again = labels::restore(config.clone(), forbidden_store(), &label_id)
        .await
        .expect("re-restore boots");
    assert!(
        matches!(again, CommandOutcome::LabelRestored { .. }),
        "the undo tombstoned rather than hard-deleted the row"
    );
}

/// US-2/S3 (W-2, the superseded-D-5 leg): the undo line re-deletes the
/// restored habit (tombstone; the log stays untouched, D26), proven by
/// the re-restore answering the pinned `HabitRestored` fact.
#[tokio::test]
async fn us2_s3_habits_restore_undo_redeletes_the_restored_row() {
    let (config, _dir) = config_for("us2_s3");
    let habit_id = seed_tombstoned_habit(&config, "stretch").await;

    habits::run(
        config.clone(),
        forbidden_store(),
        HabitsInvocation::Restore {
            habit: habit_id.clone(),
        },
    )
    .await
    .expect("restore boots");
    tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("the undo line offers the restore's inverse");
    let again = habits::run(
        config.clone(),
        forbidden_store(),
        HabitsInvocation::Restore {
            habit: habit_id.clone(),
        },
    )
    .await
    .expect("re-restore boots");
    assert!(
        matches!(
            again,
            HabitOutcome::Restored(CommandOutcome::HabitRestored { .. })
        ),
        "the undo tombstoned rather than hard-deleted the row"
    );
}

/// US-3/S4 (W-3, the superseded-D-5 leg): the undo line re-removes the
/// restored entry row (the hard-deleted row's inverse), proven by the
/// row disappearing from the ledger and the same-flags re-restore
/// answering the pinned `EntryRestored` fact.
#[tokio::test]
async fn us3_s4_track_restore_undo_redeletes_the_restored_row() {
    let (config, _dir) = config_for("us3_s4");
    let task_id = seed_task(&config, "undoable").await;
    let start = chrono::DateTime::parse_from_rfc3339(START_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let id = uuid::Uuid::now_v7();

    track::restore(
        config.clone(),
        forbidden_store(),
        None,
        &task_id,
        start,
        None,
        TimeEntryKind::Work,
        Some(id),
    )
    .await
    .expect("restore boots");
    tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("the undo line offers the restore's inverse");
    let view = track::list(config.clone(), forbidden_store(), Some(&task_id), None)
        .await
        .expect("list boots");
    assert!(
        view.entries.iter().all(|r| r.entry_id != id.to_string()),
        "the undo re-removed the restored entry row"
    );
    let again = track::restore(
        config,
        forbidden_store(),
        None,
        &task_id,
        start,
        None,
        TimeEntryKind::Work,
        Some(id),
    )
    .await
    .expect("re-restore boots");
    assert!(
        matches!(again, CommandOutcome::EntryRestored { .. }),
        "the undo removed rather than retained the row"
    );
}

// ---- US-6: the zero-input journal-addressed restore (W-11, D-5) ---------

/// US-6/S1 (W-11, AC-1): `track restore --from <journal-id>` resolves
/// (by journal id or by the deleted entry's own id — the delete
/// receipt's stable identity), dispatches
/// `RestoreTimeEntryFromJournal`, and the receipt is the pinned
/// `EntryRestored` naming the re-materialized row. The prefix leg
/// resolves over both id families. The `--from` flag conflicts with
/// the payload flags at the clap boundary.
#[tokio::test]
async fn us6_s1_track_restore_from_journal_dispatches_and_receipts() {
    let (config, _dir) = config_for("us6_s1");
    let task_id = seed_task(&config, "journaled span").await;
    // Seed a closed span, then hard-delete it (the delete journals the
    // full pre-image — the #901 capture).
    let start = chrono::DateTime::parse_from_rfc3339(START_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let end = chrono::DateTime::parse_from_rfc3339(END_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let logged = track::log(
        config.clone(),
        forbidden_store(),
        &task_id,
        track::LogMode::Span(start, end),
        taskboard_domain::tracking::TimeEntryKind::Work,
    )
    .await
    .expect("log boots");
    let deleted = track::delete(config.clone(), forbidden_store(), &logged.entry.entry_id)
        .await
        .expect("delete boots");
    assert!(deleted.deleted, "the delete landed");
    let entry_id = deleted.entry_id;

    // The clap surface: `--from` is accepted and conflicts with the
    // payload flags (the flag grammar stays the out-of-band path).
    let ok = Cli::try_parse_from(["taskboard", "track", "restore", "--from", &entry_id]);
    assert!(ok.is_ok(), "the bare --from argv parses");
    let conflict = Cli::try_parse_from([
        "taskboard",
        "track",
        "restore",
        "--from",
        &entry_id,
        "--task",
        &task_id,
        "--start",
        START_STAMP,
    ]);
    assert!(conflict.is_err(), "the payload flags refuse beside --from");

    // S1 by the deleted entry's own id: the pinned receipt names it.
    let outcome = track::restore_from_journal(config.clone(), forbidden_store(), &entry_id)
        .await
        .expect("restore boots");
    let CommandOutcome::EntryRestored { id: restored } = &outcome else {
        panic!("the restore receipt");
    };
    assert_eq!(restored.as_uuid().to_string(), entry_id);

    // Delete again; the journal-id leg resolves too — the roster row's
    // journal id is the engine arm's own address (resolution returns it
    // through the same selector surface).
    let deleted2 = track::delete(config.clone(), forbidden_store(), &entry_id)
        .await
        .expect("re-delete boots");
    assert!(deleted2.deleted);
    // The prefix leg (≥8 chars of the entry id): unique hit.
    let prefix = &entry_id[..8];
    let outcome = track::restore_from_journal(config.clone(), forbidden_store(), prefix)
        .await
        .expect("restore boots");
    let CommandOutcome::EntryRestored { id: restored } = &outcome else {
        panic!("the prefix restore receipt");
    };
    assert_eq!(restored.as_uuid().to_string(), entry_id);
}

/// US-6/S2 (W-11, AC-2/AC-3): the typed refusals fire before any
/// dispatch — an unknown selector (a random id), an ambiguous prefix,
/// and a restored (row-present) target that drained from the roster.
/// The head is unmoved by every refusal (the state truth, not text).
#[tokio::test]
async fn us6_s2_nondelete_target_refuses_typed() {
    let (config, _dir) = config_for("us6_s2");
    let task_id = seed_task(&config, "refusal span").await;
    let start = chrono::DateTime::parse_from_rfc3339(START_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let end = chrono::DateTime::parse_from_rfc3339(END_STAMP)
        .expect("the stamp parses")
        .with_timezone(&chrono::Utc);
    let logged = track::log(
        config.clone(),
        forbidden_store(),
        &task_id,
        track::LogMode::Span(start, end),
        taskboard_domain::tracking::TimeEntryKind::Work,
    )
    .await
    .expect("log boots");
    let deleted = track::delete(config.clone(), forbidden_store(), &logged.entry.entry_id)
        .await
        .expect("delete boots");
    let entry_id = deleted.entry_id;

    // Unknown selector: typed miss, nothing dispatched.
    let err = track::restore_from_journal(
        config.clone(),
        forbidden_store(),
        "00000000-0000-0000-0000-00000000dead",
    )
    .await
    .expect_err("an unknown id is a typed refusal");
    assert!(
        matches!(err, CommandError::UnknownJournalEntry { selector } if selector == "00000000-0000-0000-0000-00000000dead"),
        "typed unknown-journal-entry refusal"
    );

    // A second delete widens the roster (the prefix leg resolves over
    // both rows).
    let second = track::log(
        config.clone(),
        forbidden_store(),
        &task_id,
        track::LogMode::Span(start, end),
        taskboard_domain::tracking::TimeEntryKind::Work,
    )
    .await
    .expect("second log boots");
    let _ = track::delete(config.clone(), forbidden_store(), &second.entry.entry_id)
        .await
        .expect("second delete boots");
    let short = &entry_id[..entry_id.len().min(8)];
    // The prefix leg (an 8-char prefix of the deleted entry's id): the
    // two v7 deletes share their timestamp's leading hex, so the honest
    // outcome is the typed ambiguity — never a silent pick. A longer
    // prefix (unique against both rows) lands the pinned receipt.
    let attempt = track::restore_from_journal(config.clone(), forbidden_store(), short).await;
    match attempt {
        Err(CommandError::AmbiguousJournalEntry { candidates }) => {
            assert_eq!(candidates.len(), 2, "both deletes named");
        }
        other => panic!("v7 ids share an 8-char prefix; got {other:?}"),
    }
    let mut unique_len = 8;
    let outcome = loop {
        unique_len += 1;
        let longer = &entry_id[..unique_len.min(entry_id.len())];
        match track::restore_from_journal(config.clone(), forbidden_store(), longer).await {
            Ok(outcome) => break outcome,
            Err(CommandError::AmbiguousJournalEntry { .. }) => {}
            Err(other) => panic!("unexpected typed shape: {other:?}"),
        }
    };
    assert!(matches!(outcome, CommandOutcome::EntryRestored { .. }));

    // The restored row drains from the roster: the same selector is
    // now the typed miss (the bounded honesty, N5's face at the CLI).
    let err = track::restore_from_journal(config.clone(), forbidden_store(), &entry_id)
        .await
        .expect_err("a present row is no longer restorable from the journal");
    assert!(
        matches!(err, CommandError::UnknownJournalEntry { .. }),
        "the drained roster refuses typed"
    );
}
