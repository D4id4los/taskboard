// SPDX-License-Identifier: AGPL-3.0-only
//! T-series: the `breaks` verb family over hermetic standalone
//! tempdirs (no wiremock — a standalone core performs no network I/O).
//! One invocation is a full boot/work/shutdown "process"; durability is
//! proven across invocations on one db. Receipts are asserted as typed
//! values; error kinds via `error_kind()`, never message text.

use std::sync::Arc;

use taskboard_app::cli::{BreaksCmd, Command};
use taskboard_app::commands::breaks::{self, BreaksInvocation};
use taskboard_app::commands::output::{BreakAddReceipt, BreakRemoveReceipt, BreaksList};
use taskboard_app::commands::{BasicSession, CommandIo, dispatch_line};
use taskboard_app::config::AppConfig;

mod common;

use common::{forbidden_store, standalone_config, tempdir};

/// Seeds the fresh db with its board (the single-live-board default the
/// verbs resolve against — `breaks add` roams via the board since #588)
/// and returns the config.
async fn seeded_config(dir: &std::path::Path, db: &str) -> AppConfig {
    seeded_config_with_board(dir, db).await.0
}

/// The seeding plus the board's id (the selector pin's token).
async fn seeded_config_with_board(
    dir: &std::path::Path,
    db: &str,
) -> (AppConfig, taskboard_domain::BoardId) {
    let config = standalone_config(dir, db);
    let receipt = taskboard_app::commands::boards::create(
        config.clone(),
        forbidden_store(),
        "seed board",
        None,
    )
    .await
    .expect("board create boots");
    let id = receipt
        .id
        .parse::<uuid::Uuid>()
        .expect("the receipt's uuid");
    (config, taskboard_domain::BoardId::from_uuid(id))
}

/// The `breaks list` payload over a fresh boot (the persistence read).
async fn list(config: &AppConfig) -> BreaksList {
    breaks::list(config.clone(), forbidden_store())
        .await
        .expect("list boots")
}

/// T1: `breaks add --after` mints the rule (threshold phrase in the
/// receipt, default label derived), and a fresh boot's `list` sees it —
/// the rule survives the restart on the 0018 table.
#[tokio::test]
async fn t1_add_mints_and_persists() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "t1.db").await;
    let receipt: BreakAddReceipt = match breaks::collapse(BreaksCmd::Add {
        after: taskboard_domain::WorkThreshold::parse_text_form("90m").expect("positive threshold"),
        label: None,
        insistence: None,
        board: None,
    })
    .expect("one invocation")
    {
        BreaksInvocation::Add {
            label,
            threshold,
            insistence,
            ..
        } => breaks::add(
            config.clone(),
            forbidden_store(),
            &label,
            threshold,
            insistence,
            None,
        )
        .await
        .expect("add boots"),
        _ => panic!("collapse yields the add"),
    };
    assert_eq!(receipt.threshold_seconds, 5_400);
    assert_eq!(receipt.threshold, "1h30m");
    assert!(
        receipt.label.contains("1h30m"),
        "the default label derives from the threshold phrase"
    );
    assert_eq!(receipt.insistence, taskboard_domain::Insistence::Normal);

    let listed = list(&config).await;
    assert_eq!(listed.rules.len(), 1);
    assert_eq!(listed.rules[0].rule_id, receipt.rule_id);
    assert_eq!(listed.rules[0].threshold, "1h30m");
}

/// T2: `breaks remove <selector>` by an 8-char id prefix — the typed
/// receipt, the row gone after a restart, and a repeat the typed
/// unknown-rule refusal (terminal-by-deletion, the `timers cancel`
/// law).
#[tokio::test]
async fn t2_remove_by_prefix_and_repeat_is_typed() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "t2.db").await;
    let added: BreakAddReceipt = match breaks::collapse(BreaksCmd::Add {
        after: taskboard_domain::WorkThreshold::parse_text_form("30m").expect("positive threshold"),
        label: Some("time for a break".into()),
        insistence: Some(taskboard_domain::Insistence::Alarm),
        board: None,
    })
    .expect("one invocation")
    {
        BreaksInvocation::Add {
            label,
            threshold,
            insistence,
            ..
        } => breaks::add(
            config.clone(),
            forbidden_store(),
            &label,
            threshold,
            insistence,
            None,
        )
        .await
        .expect("add boots"),
        _ => panic!("collapse yields the add"),
    };
    assert_eq!(added.insistence, taskboard_domain::Insistence::Alarm);

    let selector = added.rule_id[..8].to_owned();
    let removed: BreakRemoveReceipt = match breaks::collapse(BreaksCmd::Remove {
        selector: selector.clone(),
    })
    .expect("one invocation")
    {
        BreaksInvocation::Remove(selector) => {
            breaks::remove(config.clone(), forbidden_store(), &selector)
                .await
                .expect("remove boots")
        }
        _ => panic!("collapse yields the remove"),
    };
    assert_eq!(removed.rule_id, added.rule_id);
    assert_eq!(removed.label, "time for a break");
    assert!(list(&config).await.rules.is_empty(), "durable removal");

    // The repeat targets nothing: typed unknown-break-rule.
    let err = match breaks::collapse(BreaksCmd::Remove { selector }) {
        Some(BreaksInvocation::Remove(selector)) => {
            breaks::remove(config.clone(), forbidden_store(), &selector)
                .await
                .expect_err("a repeat is the typed refusal")
        }
        _ => panic!("collapse yields the remove"),
    };
    assert_eq!(err.error_kind(), "unknown-break-rule");
}

/// T3: a stranger selector is the typed refusal (no text matching).
#[tokio::test]
async fn t3_stranger_selector_is_typed() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "t3.db").await;
    let err = match breaks::collapse(BreaksCmd::Remove {
        selector: "no-such-rule".into(),
    }) {
        Some(BreaksInvocation::Remove(selector)) => {
            breaks::remove(config.clone(), forbidden_store(), &selector)
                .await
                .expect_err("a stranger is the typed refusal")
        }
        _ => panic!("collapse yields the remove"),
    };
    assert_eq!(err.error_kind(), "unknown-break-rule");
}

/// T4: the label-prefix selector resolves uniquely; an ambiguous one
/// is the typed ambiguous refusal.
#[tokio::test]
async fn t4_label_prefix_resolves_and_ambiguity_is_typed() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "t4.db").await;
    #[allow(clippy::items_after_statements)]
    async fn add(config: &AppConfig, label: &str) -> BreakAddReceipt {
        match breaks::collapse(BreaksCmd::Add {
            after: taskboard_domain::WorkThreshold::parse_text_form("45m")
                .expect("positive threshold"),
            label: Some(label.to_owned()),
            insistence: None,
            board: None,
        }) {
            Some(BreaksInvocation::Add {
                label,
                threshold,
                insistence,
                ..
            }) => breaks::add(
                config.clone(),
                forbidden_store(),
                &label,
                threshold,
                insistence,
                None,
            )
            .await
            .expect("add boots"),
            _ => panic!("collapse yields the add"),
        }
    }
    let _ = add(&config, "stretch legs").await;
    let _ = add(&config, "stretch eyes").await;

    // The shared prefix is ambiguous while both rules live.
    let err = match breaks::collapse(BreaksCmd::Remove {
        selector: "stretch".into(),
    }) {
        Some(BreaksInvocation::Remove(selector)) => {
            breaks::remove(config.clone(), forbidden_store(), &selector)
                .await
                .expect_err("an ambiguous selector is the typed refusal")
        }
        _ => panic!("collapse yields the remove"),
    };
    assert_eq!(err.error_kind(), "ambiguous-adhoc-alarm");

    // A longer prefix resolves uniquely.
    let removed = match breaks::collapse(BreaksCmd::Remove {
        selector: "stretch ey".into(),
    }) {
        Some(BreaksInvocation::Remove(selector)) => {
            breaks::remove(config.clone(), forbidden_store(), &selector)
                .await
                .expect("remove boots")
        }
        _ => panic!("collapse yields the remove"),
    };
    assert_eq!(removed.label, "stretch eyes");
}

/// T5: the session lane (`dispatch_line`) serves the family identically
/// (the two-lane shape), with typed `--json` envelopes.
#[tokio::test]
async fn t5_session_lane_serves_the_family() {
    let dir = tempdir();
    let config = seeded_config(dir.path(), "t5.db").await;
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
                BreaksCmd::Add {
                    after: taskboard_domain::WorkThreshold::parse_text_form("20m")
                        .expect("positive threshold"),
                    label: Some("session break".into()),
                    insistence: None,
                    board: None,
                },
                0,
            ),
            (BreaksCmd::List, 0),
            (
                BreaksCmd::Remove {
                    selector: "session".into(),
                },
                0,
            ),
            (
                BreaksCmd::Remove {
                    selector: "session".into(),
                },
                1,
            ),
        ] {
            let code = dispatch_line(&session, &app, Command::Breaks { cmd }, &mut io).await;
            assert_eq!(code, expected, "stderr: {}", String::from_utf8_lossy(&err));
        }
    }
    app.shutdown().await;

    // Two stdout JSON lanes (add receipt, then the list envelope); the
    // refusal rides stderr, typed.
    let lanes: Vec<serde_json::Value> = out
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("one json envelope per line"))
        .collect();
    // Three lanes: the add receipt, the list (1 rule), and the remove
    // receipt. The refusal rides stderr.
    assert_eq!(lanes.len(), 3, "add + list + remove envelopes");
    assert_eq!(
        lanes[1]["rules"].as_array().map(Vec::len),
        Some(1),
        "the list shows the session-created rule"
    );
}

/// T6 (#588, the board selector): an explicit `--board` rides the
/// payload (the receipt projects the rule over the named board); an
/// unknown board id is the typed unknown-board refusal; zero boards is
/// the ambiguity refusal (no default exists).
#[tokio::test]
async fn t6_board_selector_resolves_and_refuses_typed() {
    // The unknown-board refusal.
    let dir = tempdir();
    let (config, board_id) = seeded_config_with_board(dir.path(), "t6.db").await;
    let unknown = uuid::Uuid::now_v7();
    let err = match breaks::collapse(BreaksCmd::Add {
        after: taskboard_domain::WorkThreshold::parse_text_form("30m").expect("positive threshold"),
        label: Some("time for a break".into()),
        insistence: None,
        board: Some(taskboard_domain::BoardId::from_uuid(unknown)),
    }) {
        Some(BreaksInvocation::Add {
            label,
            threshold,
            insistence,
            board,
        }) => breaks::add(
            config.clone(),
            forbidden_store(),
            &label,
            threshold,
            insistence,
            board,
        )
        .await
        .expect_err("an unknown board is the typed refusal"),
        _ => panic!("collapse yields the add"),
    };
    assert_eq!(err.error_kind(), "unknown-board");

    // The explicit selector rides the payload: the add succeeds and the
    // rule lands on the named board's roaming section lane.
    let listed = list(&config).await;
    assert!(listed.rules.is_empty(), "nothing created yet");
    let receipt: BreakAddReceipt = match breaks::collapse(BreaksCmd::Add {
        after: taskboard_domain::WorkThreshold::parse_text_form("30m").expect("positive threshold"),
        label: Some("explicit board".into()),
        insistence: None,
        board: Some(board_id),
    }) {
        Some(BreaksInvocation::Add {
            label,
            threshold,
            insistence,
            board,
        }) => breaks::add(
            config.clone(),
            forbidden_store(),
            &label,
            threshold,
            insistence,
            board,
        )
        .await
        .expect("the explicit selector resolves"),
        _ => panic!("collapse yields the add"),
    };
    assert_eq!(receipt.label, "explicit board");

    // Zero boards: the ambiguity refusal (no default exists).
    let dir = tempdir();
    let config = standalone_config(dir.path(), "t6b.db");
    let err = match breaks::collapse(BreaksCmd::Add {
        after: taskboard_domain::WorkThreshold::parse_text_form("30m").expect("positive threshold"),
        label: Some("orphan".into()),
        insistence: None,
        board: None,
    }) {
        Some(BreaksInvocation::Add {
            label,
            threshold,
            insistence,
            board,
        }) => breaks::add(
            config.clone(),
            forbidden_store(),
            &label,
            threshold,
            insistence,
            board,
        )
        .await
        .expect_err("zero boards is the refusal"),
        _ => panic!("collapse yields the add"),
    };
    assert_eq!(err.error_kind(), "ambiguous-board");
}
