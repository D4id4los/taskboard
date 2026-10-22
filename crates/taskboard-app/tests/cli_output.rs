// SPDX-License-Identifier: AGPL-3.0-only
//! O-series: the `--json` output contract (plan §6). Structural
//! assertions only — parse with `serde_json::Value`, check kinds,
//! fields, and types; never message text (AGENTS §5).

#![allow(clippy::shadow_unrelated)]

use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

use taskboard_app::bootstrap::BootstrapError;
use taskboard_app::commands::CommandError;
use taskboard_app::commands::output::{
    BoardsListEntry, ConnectReceipt, ErrorEnvelope, MutationReceipt, SelectReceipt, SyncStatusView,
    TaskList, TaskRow,
};
use taskboard_app::config::ConfigError;
use taskboard_app::secrets::CredentialError;
use taskboard_domain::{SyncCounters, SyncErrorKind};
use taskboard_state::ExecuteError;
use taskboard_sync_nextcloud::DeckError;

fn task_row(id: &str, title: &str, done: bool, order: i64, stack: &str) -> TaskRow {
    TaskRow {
        id: id.to_owned(),
        title: title.to_owned(),
        done,
        order,
        duedate: None,
        due_marker: None,
        stack_id: stack.to_owned(),
        stack_title: "col".to_owned(),
        labels: vec![],
    }
}

/// O1: the task list payload nests stack groups with their tasks and a
/// sync footer; uuid fields are uuid-shaped, timestamps ISO-8601.
#[test]
fn o1_task_list_payload_is_structurally_correct() {
    let ts = Utc.timestamp_opt(1_700_000_000, 0).single().unwrap();
    let mut row = task_row(
        "0193a1a2-7b3f-7cc2-9c31-4f8a1c2d3e4f",
        "write the docs",
        false,
        3,
        "0193a1a2-0000-7000-8000-000000000001",
    );
    row.duedate = Some(ts);
    let list = TaskList {
        stacks: vec![taskboard_app::commands::output::StackGroup {
            id: "0193a1a2-0000-7000-8000-000000000001".to_owned(),
            title: "col".to_owned(),
            order: 0,
            tasks: vec![row],
        }],
        sync: SyncStatusView::new(&taskboard_domain::SyncStatus::default()),
    };

    let v: Value = serde_json::to_value(&list).unwrap();
    let stack = &v["stacks"][0];
    assert_eq!(stack["title"], "col");
    assert_eq!(stack["order"], 0);
    let task = &stack["tasks"][0];
    assert_eq!(task["title"], "write the docs");
    assert_eq!(task["done"], false);
    assert_eq!(task["order"], 3);
    // uuid-shaped ids
    uuid::Uuid::parse_str(task["id"].as_str().unwrap()).expect("task id is a uuid");
    uuid::Uuid::parse_str(task["stack_id"].as_str().unwrap()).expect("stack id is a uuid");
    // ISO-8601 due date (serde chrono default format)
    let due = task["duedate"].as_str().unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(due).expect("ISO-8601 due date");
    assert_eq!(parsed.timestamp(), 1_700_000_000);
    // The marker field is always present (null when neither overdue nor
    // due soon) — `TaskRow` never skips fields.
    assert!(task["due_marker"].is_null());
}

/// O1b: boards/mutation/login receipts serialize their documented
/// fields, with no extra secret-bearing surface.
#[test]
fn o1b_receipt_payloads_expose_documented_fields() {
    let entry = BoardsListEntry {
        id: 3,
        title: "board".to_owned(),
        archived: false,
        deleted: false,
    };
    let v = serde_json::to_value(&entry).unwrap();
    assert_eq!(
        v,
        json!({"id": 3, "title": "board", "archived": false, "deleted": false})
    );

    let mutation = MutationReceipt {
        labels: vec![],
        id: "0193a1a2-7b3f-7cc2-9c31-4f8a1c2d3e4f".to_owned(),
        title: "t".to_owned(),
        stack_id: "0193a1a2-0000-7000-8000-000000000001".to_owned(),
        stack_title: "col".to_owned(),
        order: 0,
        done: false,
    };
    let v = serde_json::to_value(&mutation).unwrap();
    for key in [
        "id",
        "title",
        "stack_id",
        "stack_title",
        "order",
        "done",
        "labels",
    ] {
        assert!(v.get(key).is_some(), "mutation receipt has {key}");
    }

    let connect = ConnectReceipt {
        server_url: "https://cloud.example.com".to_owned(),
        username: "alice".to_owned(),
        verified: true,
        stored: "keyring".to_owned(),
        backend_switched: false,
        repointed: false,
        attached: None,
        daemon_connector: None,
        cleartext_transport: false,
    };
    let v = serde_json::to_value(&connect).unwrap();
    assert_eq!(
        v,
        json!({
            "server_url": "https://cloud.example.com",
            "username": "alice",
            "verified": true,
            "stored": "keyring",
            "backend_switched": false,
            "repointed": false,
            "cleartext_transport": false
        })
    );

    let select = SelectReceipt {
        board: entry,
        sync: SyncStatusView::new(&taskboard_domain::SyncStatus::default()),
        wiped_refs: None,
    };
    let v = serde_json::to_value(&select).unwrap();
    assert_eq!(v["board"]["id"], 3);
    assert_eq!(v["sync"]["phase"], "idle");
}

/// O2: the `SyncStatusView` phase/error-kind casing table.
#[test]
fn o2_sync_status_view_phase_table() {
    fn view(phase: taskboard_domain::SyncPhase) -> Value {
        serde_json::to_value(SyncStatusView::new(&taskboard_domain::SyncStatus {
            counters: SyncCounters::default(),
            phase,
            last_success: None,
            pending_ops: 0,
            ..taskboard_domain::SyncStatus::default()
        }))
        .unwrap()
    }
    assert_eq!(view(taskboard_domain::SyncPhase::Idle)["phase"], "idle");
    assert_eq!(
        view(taskboard_domain::SyncPhase::Syncing)["phase"],
        "syncing"
    );
    assert_eq!(
        view(taskboard_domain::SyncPhase::Offline)["phase"],
        "offline"
    );
    let failed = view(taskboard_domain::SyncPhase::Failed {
        last_error: SyncErrorKind::BadRequest,
    });
    assert_eq!(failed["phase"], "failed");
    assert_eq!(failed["error_kind"], "bad-request");
    assert_eq!(
        view(taskboard_domain::SyncPhase::Idle)["error_kind"],
        Value::Null
    );
    for (kind, expected) in [
        (SyncErrorKind::Network, "network"),
        (SyncErrorKind::Auth, "auth"),
        (SyncErrorKind::Forbidden, "forbidden"),
        (SyncErrorKind::Server, "server"),
        (SyncErrorKind::BadRequest, "bad-request"),
        (SyncErrorKind::LocalData, "local-data"),
        (SyncErrorKind::NoBoard, "no-board"),
    ] {
        let v = view(taskboard_domain::SyncPhase::Failed { last_error: kind });
        assert_eq!(v["error_kind"], expected, "{expected} casing");
    }
}

/// O3: `CommandError::error_kind()` covers every variant (the match
/// below is exhaustive — the compiler forces new variants into the
/// table) and the stderr envelope carries kind + optional detail.
#[test]
#[allow(clippy::too_many_lines)] // one sample per variant; the list IS the table
fn o3_error_kinds_cover_every_variant() {
    fn expected(err: &CommandError) -> &'static str {
        match err {
            CommandError::Config(c) => c.kind(),
            CommandError::Bootstrap(b) if matches!(**b, BootstrapError::InstanceLocked { .. }) => {
                "instance-locked"
            }
            CommandError::Bootstrap(_) => "bootstrap",
            CommandError::Credential(_) => "credential",
            CommandError::Engine(e) => match &**e {
                ExecuteError::Rejected(
                    taskboard_domain::CommandError::UnknownBoard(_)
                    | taskboard_domain::CommandError::BoardDeleted(_),
                ) => "unknown-board",
                ExecuteError::Rejected(
                    taskboard_domain::CommandError::UnknownTask(_)
                    | taskboard_domain::CommandError::TaskDeleted(_),
                ) => "unknown-task",
                ExecuteError::Rejected(
                    taskboard_domain::CommandError::UnknownStack(_)
                    | taskboard_domain::CommandError::StackDeleted(_),
                ) => "unknown-stack",
                ExecuteError::Rejected(
                    taskboard_domain::CommandError::UnknownLabel(_)
                    | taskboard_domain::CommandError::LabelDeleted(_),
                ) => "unknown-label",
                _ => "engine",
            },
            CommandError::AnchorTaskReserved => "anchor-task-reserved",
            CommandError::InvalidCredentials => "invalid-credentials",
            CommandError::Unreachable => "unreachable",
            CommandError::UnknownBoard(_) => "unknown-board",
            CommandError::BoardAlreadyBound { .. } => "board-already-bound",
            CommandError::UnknownTask(_) => "unknown-task",
            CommandError::AmbiguousTask { .. } => "ambiguous-task",
            CommandError::UnknownStack(_) => "unknown-stack",
            CommandError::AmbiguousStack { .. } => "ambiguous-stack",
            CommandError::UnknownLabel(_) => "unknown-label",
            CommandError::AmbiguousLabel { .. } => "ambiguous-label",
            CommandError::NoStacks => "no-stacks",
            CommandError::AmbiguousBoard { .. } => "ambiguous-board",
            CommandError::SyncFailed(_) => "sync-failed",
            CommandError::SyncTimedOut => "sync-timed-out",
            CommandError::ConfigWrite(_) => "config-write",
            CommandError::Prompt(_) => "prompt",
            CommandError::Deck(_) => "deck",
            CommandError::SyncDisabled => "sync-disabled",
            CommandError::SyncUnavailable => "sync-unavailable",
            CommandError::LocalBoardExists => "local-board-exists",
            CommandError::NotConfigured => "not-configured",
            CommandError::NoDaemon => "no-daemon",
            CommandError::Attach(_) => "attach-failed",
            CommandError::BackendExplicitStandalone => "backend-explicit-standalone",
            CommandError::ConnectionExists => "connection-exists",
            CommandError::ConfirmationRequired { .. } => "confirmation-required",
            CommandError::UnbindConfirmationRequired { .. } => "unbind-confirmation-required",
            CommandError::ReconcileConfirmationRequired { .. } => "reconcile-confirmation-required",
            CommandError::IpcTransport => "ipc-transport",
            CommandError::IpcVersion { .. } => "ipc-version",
            CommandError::UnknownConnector { .. } => "unknown-connector",
            CommandError::SessionOnly => "session-only",
        }
    }

    let samples: Vec<CommandError> = vec![
        CommandError::from(ConfigError::InvalidServerUrl),
        CommandError::from(BootstrapError::InstanceLocked {
            db: "/tmp/x.db".into(),
            holder_pid: Some(1),
        }),
        CommandError::from(BootstrapError::StorageDir(std::io::Error::other("x"))),
        CommandError::from(CredentialError::NotFound),
        CommandError::from(ExecuteError::Rejected(
            taskboard_domain::CommandError::UnknownBoard(taskboard_domain::BoardId::from_uuid(
                uuid::Uuid::from_u128(0xB0A0),
            )),
        )),
        CommandError::from(ExecuteError::EngineGone),
        CommandError::AmbiguousBoard {
            candidates: vec!["b".to_owned()],
        },
        CommandError::InvalidCredentials,
        CommandError::Unreachable,
        CommandError::UnknownBoard(7),
        CommandError::BoardAlreadyBound {
            board: "b".to_owned(),
            current: 7,
        },
        CommandError::UnknownTask("abc".to_owned()),
        CommandError::AmbiguousTask {
            candidates: vec!["x".to_owned(), "y".to_owned()],
        },
        CommandError::UnknownLabel("lab".to_owned()),
        CommandError::AmbiguousLabel {
            candidates: vec!["l1".to_owned(), "l2".to_owned()],
        },
        CommandError::UnknownStack("col".to_owned()),
        CommandError::AmbiguousStack {
            candidates: vec!["a".to_owned(), "b".to_owned()],
        },
        CommandError::NoStacks,
        CommandError::SyncFailed(SyncErrorKind::Server),
        CommandError::SyncTimedOut,
        CommandError::ConfigWrite(std::io::Error::other("x")),
        CommandError::Prompt(std::io::Error::other("x")),
        CommandError::from(DeckError::BadRequest),
        CommandError::NotConfigured,
        CommandError::NoDaemon,
        CommandError::Attach(taskboard_app::bootstrap::AttachError::ConnectorNameInUse(
            taskboard_domain::ConnectorId::from("nextcloud"),
        )),
        CommandError::BackendExplicitStandalone,
        CommandError::ConnectionExists,
        CommandError::IpcTransport,
        CommandError::IpcVersion { server_v: 2 },
        CommandError::UnknownConnector {
            name: "gama".to_owned(),
        },
        CommandError::SessionOnly,
    ];
    for err in &samples {
        assert_eq!(err.error_kind(), expected(err), "{err}");
    }
    // Structural details on the stderr envelope.
    let bound = CommandError::BoardAlreadyBound {
        board: "local-board".to_owned(),
        current: 9,
    };
    let envelope = ErrorEnvelope {
        error: taskboard_app::commands::output::ErrorBody {
            kind: bound.error_kind().to_owned(),
            detail: bound.error_detail(),
        },
    };
    let v = serde_json::to_value(&envelope).unwrap();
    assert_eq!(v["error"]["kind"], "board-already-bound");
    assert_eq!(v["error"]["detail"], "local-board (bound remote 9)");
    let version_err = CommandError::IpcVersion { server_v: 2 };
    assert_eq!(
        version_err.error_detail().as_deref(),
        Some("2"),
        "the mismatched protocol version rides the detail field"
    );
    let sync_err = CommandError::SyncFailed(SyncErrorKind::Server);
    let envelope = ErrorEnvelope {
        error: taskboard_app::commands::output::ErrorBody {
            kind: sync_err.error_kind().to_owned(),
            detail: sync_err.error_detail(),
        },
    };
    let v = serde_json::to_value(&envelope).unwrap();
    assert_eq!(v["error"]["kind"], "sync-failed");
    assert_eq!(v["error"]["detail"], "server");
}

/// O4: leak floor — no projection of a login carrying a known password
/// ever formats the password (the sanctioned redaction-test class).
#[test]
fn o4_login_projection_never_carries_the_secret() {
    const SECRET: &str = "s3cret-app-password-o4";
    // The receipt is built from verified, non-secret values only; the
    // type has no secret field, so the floor holds by construction —
    // format everything a caller could see and assert absence.
    let receipt = ConnectReceipt {
        server_url: "https://cloud.example.com".to_owned(),
        username: "alice".to_owned(),
        verified: true,
        stored: "keyring".to_owned(),
        backend_switched: false,
        repointed: false,
        attached: None,
        daemon_connector: None,
        cleartext_transport: false,
    };
    let json = serde_json::to_string(&receipt).unwrap();
    assert!(!json.contains(SECRET));
    assert!(!format!("{receipt:?}").contains(SECRET));
    // And the payload type itself carries no secret-bearing field:
    // `stored` is only ever "keyring" | "env".
    assert!(receipt.stored == "keyring" || receipt.stored == "env");
}
