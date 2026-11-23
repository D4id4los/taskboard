// SPDX-License-Identifier: AGPL-3.0-only

//! The browse-parity CLI seam tests (round 5 chunk D1ii, §3's
//! nominated file): `tasks history [--json]` (W-5, D-4/I-4) and the
//! `tasks undo to`/`tasks redo to` bounded jumps (W-6) over hermetic
//! standalone tempdirs — one invocation is a full boot/work/shutdown
//! process on one db. Receipts are asserted as typed values;
//! refusals via `error_kind()`, never message text. The bounded
//! windows are the honest id universe (I-3): every selector here
//! resolves against what `tasks history` publishes.

use clap::Parser;

use taskboard_app::cli::{Cli, Command, TasksCmd};
use taskboard_app::commands::boards;
use taskboard_app::commands::output::JournalVerb;
use taskboard_app::commands::stacks;
use taskboard_app::commands::tasks;
use taskboard_app::config::AppConfig;
use taskboard_domain::JournalId;

use crate::common::{forbidden_store, standalone_config, tempdir};

mod common;

/// Parses one `taskboard tasks …` argv through the real clap surface.
fn parse_tasks(argv: &[&str]) -> TasksCmd {
    let mut cmdline = vec!["taskboard"];
    cmdline.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&cmdline).expect("the argv parses");
    let Some(Command::Tasks { cmd }) = cli.command else {
        panic!("the argv selects the tasks group");
    };
    cmd
}

/// Seeds board + stack + `n` tasks; each step is one full boot
/// (durability rides the db).
async fn seeded_config(dir: &std::path::Path, db: &str, titles: &[&str]) -> AppConfig {
    let config = standalone_config(dir, db);
    boards::create(config.clone(), forbidden_store(), "seed board", None)
        .await
        .expect("board create boots");
    stacks::add(config.clone(), forbidden_store(), None, "seed stack", None)
        .await
        .expect("stack add boots");
    for title in titles {
        tasks::add(config.clone(), forbidden_store(), title, Some("seed stack"))
            .await
            .expect("task add boots");
    }
    config
}

/// The live task titles, read off a fresh boot (terminal state).
async fn live_titles(config: &AppConfig) -> Vec<String> {
    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list boots");
    let mut titles: Vec<String> = list
        .stacks
        .iter()
        .flat_map(|group| group.tasks.iter().map(|row| row.title.clone()))
        .collect();
    titles.sort();
    titles
}

/// The history row whose payload names `subject` (the typed lookup —
/// the window is newest first).
async fn history_row_for(config: &AppConfig, subject: &str) -> JournalId {
    let view = tasks::history(config.clone(), forbidden_store())
        .await
        .expect("history boots");
    view.recent
        .iter()
        .find(|row| row.payload.as_deref() == Some(subject))
        .map(|row| row.id)
        .expect("the named row is in window")
}

// -- US-2: `tasks history` --------------------------------------------

/// US-2/S1 (W-5): the history read publishes the three windows as the
/// typed projection — the head fact, the `recent` rows naming the
/// seeded creates (newest first, payloads carried), and the (empty)
/// branch window. A read: no command crosses the boundary, so the
/// terminal state is untouched.
#[tokio::test]
async fn us2_s1_history_prints_the_three_windows() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "us2_s1.db", &["alpha", "beta"]).await;
    let view = tasks::history(config.clone(), forbidden_store())
        .await
        .expect("history boots");
    assert_eq!(
        view.recent[0].payload.as_deref(),
        Some("beta"),
        "the newest row's payload names its subject"
    );
    assert!(
        view.recent
            .iter()
            .any(|row| row.payload.as_deref() == Some("alpha")),
        "the older create is in window too"
    );
    assert!(
        view.branch_children.is_empty(),
        "no fork: the branch window is the typed empty fact"
    );
    assert!(view.can_undo, "a head exists and is undoable");
    assert!(!view.can_redo);
    assert!(view.head_label.is_some(), "the head fact is present");
}

/// US-2/S2 (`--json`): the history projection decodes against the
/// widened `UndoLabel` — every row round-trips with the additive
/// `payload` key (`null` for a subjectless arm, the string form
/// otherwise). The serde lane is the same shape `emit_to` writes.
#[tokio::test]
async fn us2_s2_json_history_decodes_the_projection() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "us2_s2.db", &["alpha"]).await;
    let view = tasks::history(config.clone(), forbidden_store())
        .await
        .expect("history boots");
    let line = serde_json::to_string(&view).expect("the projection serializes");
    let decoded: serde_json::Value = serde_json::from_str(&line).expect("the line decodes");
    let rows = decoded["recent"].as_array().expect("recent rows");
    assert!(!rows.is_empty(), "the seeded creates are in window");
    assert!(
        rows[0]["payload"].is_string(),
        "a subjectful row decodes with its string payload"
    );
    assert!(rows[0]["id"].is_string(), "rows address by typed id");

    // The grammar: the subcommand's own `--json` parses (the machine
    // lane's CLI door).
    assert!(matches!(
        parse_tasks(&["tasks", "history", "--json"]),
        TasksCmd::History { json: true }
    ));
    assert!(matches!(
        parse_tasks(&["tasks", "history"]),
        TasksCmd::History { json: false }
    ));
}

/// US-2/S3 (D-4's floor): an empty journal is the typed empty fact —
/// all windows empty, the head fact absent, nothing undoable.
#[tokio::test]
async fn us2_s3_empty_journal_is_the_typed_empty_fact() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "us2_s3.db");
    let view = tasks::history(config, forbidden_store())
        .await
        .expect("history boots");
    assert_eq!(view.recent.len(), 0);
    assert_eq!(view.branch_children.len(), 0);
    assert!(view.head_label.is_none());
    assert!(!view.can_undo);
    assert!(!view.can_redo);
}

// -- US-3: the bounded jumps ------------------------------------------

/// US-3/S1 (W-6): `tasks undo to <prefix>` resolves the prefix over
/// the bounded windows and lands the head exactly on the named row —
/// the receipt is the typed `Undo` verb naming that entry, and the
/// terminal state reflects the jump (the newest create retracted).
#[tokio::test]
async fn us3_s1_undo_to_dispatches_and_receipts() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "us3_s1.db", &["alpha", "beta"]).await;
    // Jumping to the *alpha* row retracts everything after it (the
    // newest create, beta); the beta row IS the head — a jump to it
    // is the typed no-op walk.
    let target = history_row_for(&config, "alpha").await;
    // A unique prefix: v7 ids minted in one session share their
    // leading characters, so the selector takes enough of the uuid to
    // be unique (the grammar's uniqueness law — the typed ambiguity
    // refusal guards the rest).
    let prefix = target.as_uuid().to_string()[..12].to_owned();

    let receipt = tasks::undo_to(config.clone(), forbidden_store(), &prefix)
        .await
        .expect("the jump lands");
    assert_eq!(receipt.verb, JournalVerb::Undo);
    // The walk's receipt names the FINAL entry it undid (the walk
    // contract) — the retracted beta create, not the landing row.
    let retracted = history_row_for(&config, "beta").await;
    assert_eq!(receipt.entry, retracted.as_uuid().to_string());

    let titles = live_titles(&config).await;
    assert!(
        !titles.contains(&"beta".to_owned()),
        "the newest create retracted"
    );
    assert!(
        titles.contains(&"alpha".to_owned()),
        "the older create stays"
    );
}

/// US-3/S2 (W-6): after a fork (an undo, then a diverging write), the
/// abandoned sibling is a real branch row; `tasks redo to <prefix>`
/// walks the chosen branch — the receipt is the typed `Redo` verb and
/// the head lands on the branch tip (the named row: the walk's own
/// contract).
#[tokio::test]
async fn us3_s2_redo_to_lands_the_branch_tip() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "us3_s2.db", &["alpha", "beta"]).await;
    let beta = history_row_for(&config, "beta").await;

    // One undo parks beta (head at alpha); the diverging write forks
    // the line: alpha now has the abandoned beta and the live gamma
    // as its children.
    tasks::undo(config.clone(), forbidden_store())
        .await
        .expect("undo one");
    tasks::add(
        config.clone(),
        forbidden_store(),
        "gamma",
        Some("seed stack"),
    )
    .await
    .expect("the diverging write forks");
    // The abandoned sibling stays addressable at the bounded
    // resolution — it rides the windows (the resolution universe's
    // law); the head's DIRECT children stay the redo surface.
    let view = tasks::history(config.clone(), forbidden_store())
        .await
        .expect("history boots");
    let window_ids: Vec<_> = view
        .recent
        .iter()
        .chain(view.branch_children.iter())
        .map(|row| row.id)
        .collect();
    assert!(
        window_ids.contains(&beta),
        "the abandoned sibling stays in window"
    );

    let prefix = beta.as_uuid().to_string()[..12].to_owned();
    let receipt = tasks::redo_to(config.clone(), forbidden_store(), &prefix)
        .await
        .expect("the branch jump lands");
    assert_eq!(receipt.verb, JournalVerb::Redo);
    // The head lands on the branch TIP and the receipt names it (the
    // walk's contract): here the tip is the beta row itself.
    assert_eq!(receipt.entry, beta.as_uuid().to_string());

    let titles = live_titles(&config).await;
    assert!(
        titles.contains(&"alpha".to_owned()),
        "the common trunk is live"
    );
    assert!(
        titles.contains(&"beta".to_owned()),
        "the chosen branch's content is live"
    );
    assert!(
        !titles.contains(&"gamma".to_owned()),
        "the abandoned newer branch stays parked"
    );
}

/// US-3/S3 (D-5's parity law): a well-formed uuid that is outside the
/// bounded windows (a stale or foreign row) refuses typed before any
/// dispatch — the terminal state is untouched.
#[tokio::test]
async fn us3_s3_stale_target_refuses_typed() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "us3_s3.db", &["alpha"]).await;
    let stale = uuid::Uuid::new_v4().to_string();
    let err = tasks::undo_to(config.clone(), forbidden_store(), &stale)
        .await
        .expect_err("the out-of-window id refuses");
    assert_eq!(err.error_kind(), "unknown-journal-target");
    let titles = live_titles(&config).await;
    assert!(titles.contains(&"alpha".to_owned()), "nothing dispatched");
}

/// US-3/S4's CLI-side floor (D-4): a prefix shorter than the 8-char
/// floor refuses typed (the honest bound is the grammar) — the
/// ambiguity arm itself is the pure resolution unit test
/// (`us3_s4_ambiguous_prefix_refuses_before_dispatch`, predicate
/// suite over a synthetic projection).
#[tokio::test]
async fn us3_s4_ambiguous_prefix_refuses_before_dispatch() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "us3_s4.db", &["alpha", "beta"]).await;
    let short = "a".repeat(4);
    let err = tasks::undo_to(config.clone(), forbidden_store(), &short)
        .await
        .expect_err("the sub-floor prefix refuses");
    assert_eq!(err.error_kind(), "unknown-journal-target");
    // Nothing dispatched: the full line is still intact.
    let titles = live_titles(&config).await;
    assert_eq!(titles.len(), 2);
}
