// SPDX-License-Identifier: AGPL-3.0-only
//! R-series: the `tasks remind` verb family over hermetic standalone
//! tempdirs (no wiremock — a standalone core performs no network I/O).
//! One invocation is a full boot/work/shutdown "process"; durability is
//! proven across invocations on one db. Receipts are asserted as typed
//! values; error kinds via `error_kind()`, never message text.

use std::sync::Arc;

use chrono::TimeDelta;
use clap::Parser;

use taskboard_app::cli::{Cli, Command, TasksCmd};
use taskboard_app::commands::output::{
    ReminderClearReceipt, ReminderMuteReceipt, ReminderSetReceipt, RemindersList,
};
use taskboard_app::commands::{BasicSession, CommandIo, dispatch_line, reminders};
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

/// Seeds board → stack → task through one-shot verbs (each test gets a
/// fresh db; re-seeds on one db tolerate the already-existing prefixes).
async fn seed(config: &AppConfig, stack: &str, title: &str) {
    use taskboard_app::commands::{boards, stacks, tasks};
    let _ = boards::create(config.clone(), forbidden_store(), "Personal", None).await;
    let _ = stacks::add(config.clone(), forbidden_store(), stack, None).await;
    let _ = tasks::add(config.clone(), forbidden_store(), title, Some(stack)).await;
}

/// The `remind --list` payload over a fresh boot (the persistence
/// read).
async fn list(config: &AppConfig) -> RemindersList {
    reminders::list(config.clone(), forbidden_store())
        .await
        .expect("list boots")
}

/// R1: `remind --before` sets exactly one rule; the receipt carries the
/// fresh rule id, and a fresh boot's `--list` sees it (durable).
#[tokio::test]
async fn r1_remind_sets_one_rule() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r1.db");
    seed(&config, "Todo", "Write report").await;

    let receipt = reminders::set(
        config.clone(),
        forbidden_store(),
        "Write report",
        Some(TimeDelta::try_minutes(10).expect("10m")),
    )
    .await
    .expect("set");
    let ReminderSetReceipt {
        rule_id,
        task_id,
        task_title,
        before,
    } = receipt;
    assert_ne!(rule_id, "");
    assert_ne!(task_id, "");
    assert_eq!(task_title, "Write report");
    assert_eq!(before, "10m");

    let listed = list(&config).await;
    assert_eq!(listed.rules.len(), 1);
    assert_eq!(listed.rules[0].id, rule_id);
    assert_eq!(listed.rules[0].task_title, "Write report");
    assert_eq!(listed.rules[0].before, "10m");
    assert!(!listed.rules[0].task_muted);
}

/// R2: a second `remind` replaces the rule (still one, new id and
/// offset).
#[tokio::test]
async fn r2_second_remind_replaces() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r2.db");
    seed(&config, "Todo", "File taxes").await;
    let first = reminders::set(
        config.clone(),
        forbidden_store(),
        "File taxes",
        Some(TimeDelta::try_minutes(10).expect("10m")),
    )
    .await
    .expect("first set");
    let second = reminders::set(
        config.clone(),
        forbidden_store(),
        "File taxes",
        Some(TimeDelta::try_hours(1).expect("1h")),
    )
    .await
    .expect("second set");

    assert_ne!(second.rule_id, first.rule_id);
    assert_eq!(second.before, "1h");
    let listed = list(&config).await;
    assert_eq!(listed.rules.len(), 1, "replace-all semantics");
    assert_eq!(listed.rules[0].id, second.rule_id);
    assert_eq!(listed.rules[0].before, "1h");
}

/// R3: with no `--before`, the config's `default_before` is the rule's
/// offset (here `"2m"`).
#[tokio::test]
async fn r3_before_default_comes_from_config() {
    let dir = tempdir();
    let mut config = standalone_config(dir.path(), "r3.db");
    config.reminders.default_before = "2m".to_owned();
    seed(&config, "Todo", "Water plants").await;

    let receipt = reminders::set(config.clone(), forbidden_store(), "Water plants", None)
        .await
        .expect("set with the config default");
    assert_eq!(receipt.before, "2m");
    let listed = list(&config).await;
    assert_eq!(listed.rules[0].before, "2m");
}

/// R4: `--list` on a fresh board is empty; after a set it shows the
/// row.
#[tokio::test]
async fn r4_list_empty_then_one_row() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r4.db");
    assert_eq!(list(&config).await.rules, []);

    seed(&config, "Todo", "Alone").await;
    reminders::set(config.clone(), forbidden_store(), "Alone", None)
        .await
        .expect("set");
    let listed = list(&config).await;
    assert_eq!(listed.rules.len(), 1);
    assert_eq!(listed.rules[0].task_title, "Alone");
    assert_eq!(listed.rules[0].before, "5m", "the built-in default");
    assert!(!listed.rules[0].task_muted);
}

/// R5: `--clear` reports the removed count; a repeat reports 0 and
/// still succeeds (the idempotent no-op).
#[tokio::test]
async fn r5_clear_reports_removed_and_repeats_zero() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r5.db");
    seed(&config, "Todo", "Sweep").await;
    reminders::set(config.clone(), forbidden_store(), "Sweep", None)
        .await
        .expect("set");

    let receipt = reminders::clear(config.clone(), forbidden_store(), "Sweep")
        .await
        .expect("clear");
    let ReminderClearReceipt {
        task_id: _,
        task_title,
        removed,
    } = receipt;
    assert_eq!(task_title, "Sweep");
    assert_eq!(removed, 1);

    let repeat = reminders::clear(config.clone(), forbidden_store(), "Sweep")
        .await
        .expect("repeat clear");
    assert_eq!(repeat.removed, 0);
    assert_eq!(list(&config).await.rules, []);
}

/// R6: `--off` mutes (the receipt reads the post-state membership),
/// `--on` unmutes; a repeat `--off` still succeeds.
#[tokio::test]
async fn r6_mute_toggle_and_repeat() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r6.db");
    seed(&config, "Todo", "Muted one").await;

    let receipt = reminders::mute(config.clone(), forbidden_store(), "Muted one", true)
        .await
        .expect("mute");
    let ReminderMuteReceipt { muted, .. } = receipt;
    assert!(muted);

    let repeat = reminders::mute(config.clone(), forbidden_store(), "Muted one", true)
        .await
        .expect("repeat mute is an accepted no-op");
    assert!(repeat.muted);

    let unmuted = reminders::mute(config.clone(), forbidden_store(), "Muted one", false)
        .await
        .expect("unmute");
    assert!(!unmuted.muted);
}

/// R7: selector failures keep their cascade kinds: unknown →
/// `unknown-task`, ambiguous title → `ambiguous-task`.
#[tokio::test]
async fn r7_selector_errors_keep_their_kinds() {
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r7.db");
    seed(&config, "Todo", "First").await;
    seed(&config, "Todo2", "First").await; // ambiguous duplicate title

    let err = reminders::set(config.clone(), forbidden_store(), "no-such-task", None)
        .await
        .expect_err("unknown");
    assert_eq!(err.error_kind(), "unknown-task");

    let err = reminders::set(config.clone(), forbidden_store(), "First", None)
        .await
        .expect_err("ambiguous");
    assert_eq!(err.error_kind(), "ambiguous-task");
}

/// R8: the clap surface — a bad duration is a usage error, `--list`
/// conflicts with a selector, a bare `remind` has no mode.
#[test]
fn r8_clap_mode_and_value_rules() {
    let err = Cli::try_parse_from(["taskboard", "tasks", "remind", "--before", "5min", "t"])
        .expect_err("bad duration is a usage error");
    assert_eq!(err.exit_code(), 2);

    let err = Cli::try_parse_from(["taskboard", "tasks", "remind", "--list", "t"])
        .expect_err("--list conflicts with a selector");
    assert_eq!(err.exit_code(), 2);

    let err =
        Cli::try_parse_from(["taskboard", "tasks", "remind"]).expect_err("a mode is required");
    assert_eq!(err.exit_code(), 2);

    let ok = Cli::try_parse_from([
        "taskboard",
        "tasks",
        "remind",
        "t",
        "--before",
        "1h30m",
        "--json",
    ])
    .expect("the flag form parses");
    let Some(Command::Tasks {
        cmd: TasksCmd::Remind {
            task, before, list, ..
        },
    }) = ok.command
    else {
        panic!("expected the Remind variant");
    };
    assert_eq!(task.as_deref(), Some("t"));
    assert_eq!(before, Some(TimeDelta::try_minutes(90).expect("90m")));
    assert!(!list);
}

/// R10: the session lane routes `Remind` end to end — one `dispatch_line`
/// call mutates the booted core (routing-only; semantics live in R1–R6).
#[tokio::test]
async fn r10_session_lane_routes_remind() {
    use taskboard_app::cli::Command;
    let dir = tempdir();
    let config = standalone_config(dir.path(), "r10.db");
    seed(&config, "Todo", "Session task").await;

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
        let code = dispatch_line(
            &session,
            &app,
            Command::Tasks {
                cmd: TasksCmd::Remind {
                    task: Some("Session task".to_owned()),
                    before: Some(TimeDelta::try_minutes(7).expect("7m")),
                    list: false,
                    clear: false,
                    off: false,
                    on: false,
                },
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&err));

        let code = dispatch_line(
            &session,
            &app,
            Command::Tasks {
                cmd: TasksCmd::Remind {
                    task: None,
                    before: None,
                    list: true,
                    clear: false,
                    off: false,
                    on: false,
                },
            },
            &mut io,
        )
        .await;
        assert_eq!(code, 0);
    }
    app.shutdown().await;

    // The line's mutation was durable, and the JSON lanes emitted twice.
    let listed = list(&config).await;
    assert_eq!(listed.rules.len(), 1);
    assert_eq!(listed.rules[0].before, "7m");
    let lines: Vec<&[u8]> = out
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(lines.len(), 2);
}

/// R9: the `tasks list` due-marker column — banner truth from the
/// task's own due date (live + open + due), ignoring mute, rules, and
/// completions.
#[tokio::test]
async fn r9_due_marker_column_follows_banner_truth() {
    use chrono::Utc;
    use taskboard_app::commands::output::DueMarker;
    use taskboard_app::commands::tasks;

    let dir = tempdir();
    let config = standalone_config(dir.path(), "r9.db");
    // Distinct stacks: the seed helper's one-shot adds resolve stacks
    // by title, and duplicate titles would go ambiguous.
    for (n, title) in [
        "Overdue task",
        "Due soon task",
        "Far future task",
        "Due-less task",
        "Muted overdue task",
        "Done overdue task",
    ]
    .into_iter()
    .enumerate()
    {
        seed(&config, &format!("S{n}"), title).await;
    }

    let now = Utc::now();
    let set_due = |title: &str, due: chrono::DateTime<chrono::Utc>| {
        let config = config.clone();
        let title = title.to_owned();
        async move {
            taskboard_app::commands::tasks::edit(
                config,
                forbidden_store(),
                &title,
                taskboard_domain::TaskChanges {
                    duedate: Some(Some(due)),
                    ..taskboard_domain::TaskChanges::default()
                },
            )
            .await
            .expect("edit due");
        }
    };
    set_due(
        "Overdue task",
        now - chrono::TimeDelta::try_hours(1).unwrap(),
    )
    .await;
    set_due(
        "Due soon task",
        now + chrono::TimeDelta::try_hours(2).unwrap(),
    )
    .await;
    set_due(
        "Far future task",
        now + chrono::TimeDelta::try_days(30).unwrap(),
    )
    .await;
    set_due(
        "Muted overdue task",
        now - chrono::TimeDelta::try_hours(2).unwrap(),
    )
    .await;
    set_due(
        "Done overdue task",
        now - chrono::TimeDelta::try_hours(2).unwrap(),
    )
    .await;
    reminders::mute(
        config.clone(),
        forbidden_store(),
        "Muted overdue task",
        true,
    )
    .await
    .expect("mute");
    taskboard_app::commands::tasks::done(
        config.clone(),
        forbidden_store(),
        "Done overdue task",
        false,
    )
    .await
    .expect("done");

    let list = tasks::list(config.clone(), forbidden_store(), None)
        .await
        .expect("list boots");
    let marker_of = |title: &str| {
        list.stacks
            .iter()
            .flat_map(|g| g.tasks.iter())
            .find(|t| t.title == title)
            .unwrap_or_else(|| panic!("{title} listed"))
            .due_marker
    };
    assert_eq!(marker_of("Overdue task"), Some(DueMarker::Overdue));
    assert_eq!(marker_of("Due soon task"), Some(DueMarker::DueSoon));
    assert_eq!(marker_of("Far future task"), None);
    assert_eq!(marker_of("Due-less task"), None);
    assert_eq!(
        marker_of("Muted overdue task"),
        Some(DueMarker::Overdue),
        "mute is nagging policy, not banner truth"
    );
    assert_eq!(marker_of("Done overdue task"), None, "done is not overdue");
}

// ---- E-series: rule → tick → delivery → notification (hermetic) ----

/// The fake sink (fakes over mocks): records every delivered
/// notification behind a mutex; assertions check the *set*.
#[derive(Debug, Default)]
struct RecordingSink {
    notifications: std::sync::Mutex<Vec<taskboard_domain::Notification>>,
}

impl RecordingSink {
    fn len(&self) -> usize {
        self.notifications.lock().expect("sink lock").len()
    }
}

impl taskboard_app::notifications::NotificationSink for RecordingSink {
    fn deliver<'a>(
        &'a self,
        notification: &'a taskboard_domain::Notification,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(), taskboard_app::notifications::NotifyError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.notifications
                .lock()
                .expect("sink lock")
                .push(notification.clone());
            Ok(())
        })
    }
}

/// A reminder-enabled config over a tempdir db with a fast logical tick.
fn e2e_config(dir: &std::path::Path, db: &str, default_before: &str) -> AppConfig {
    let mut config = standalone_config(dir, db);
    config.reminders.tick_secs = 1;
    config.reminders.default_before = default_before.to_string();
    config
}

/// Boots the observed core with the recording sink.
async fn e2e_boot(config: AppConfig) -> (taskboard_app::App, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    let app = taskboard_app::bootstrap::bootstrap(config, forbidden_store(), sink.clone())
        .await
        .expect("e2e boots");
    (app, sink)
}

/// Polls a predicate with a deadline (the bounded escape; the poll is
/// the mechanism — no fixed sleeps waiting for state).
async fn wait_for(predicate: impl Fn() -> bool, what: &'static str) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !predicate() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("deadline waiting for {what}"));
}

/// The bounded quiet window for a *negative* assertion: the poll runs
/// for the whole window and must never see the predicate fire. The
/// window spans several ticks at the E-series' 1 s cadence.
///
/// Deviation note: the plan's `start_paused` auto-advance instantly
/// exhausts sqlx's default pool-acquire timeout (the real sqlite tempdir
/// is DA12's other requirement), so the E-series runs on the real clock
/// and pays this window.
async fn await_quiet(violated: impl Fn() -> bool, what: &str, dump: impl Fn() -> String) {
    // Elapsed = quiet; `Ok(())` = the violation predicate fired.
    if tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !violated() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok()
    {
        panic!("the quiet window was violated: {what}; {}", dump());
    }
}

/// The sink dump for a quiet-window failure.
fn sink_dump(sink: &RecordingSink) -> String {
    format!("sink: {:?}", sink.notifications.lock().expect("sink lock"))
}

/// Seeds board/stack/task (no duedate) plus an explicit rule through
/// one-shot verbs; returns the rule id from the receipt. Nothing can
/// fire during the seeding boots: no duedate, no occurrence.
async fn e2e_seed_task_and_rule(config: &AppConfig, title: &str, before: TimeDelta) -> String {
    seed(config, "Todo", title).await;
    reminders::set(config.clone(), forbidden_store(), title, Some(before))
        .await
        .expect("rule set")
        .rule_id
}

/// E1: rule → tick → delivery → notification. The rule is persisted by
/// the seeding boots; the duedate lands *inside* the observed boot (the
/// immediate first tick must not fire anything before it).
#[tokio::test]
async fn e1_rule_fires_and_delivers_end_to_end() {
    let dir = tempdir();
    let config = e2e_config(dir.path(), "e1.db", "5m");
    let rule_id =
        e2e_seed_task_and_rule(&config, "E1 task", TimeDelta::try_minutes(10).expect("10m")).await;
    let rule_uuid = uuid::Uuid::parse_str(&rule_id).expect("rule id");

    let (app, sink) = e2e_boot(config.clone()).await;
    let now = chrono::Utc::now();
    let due = now - chrono::TimeDelta::try_minutes(10).expect("-10m");
    app.engine
        .execute(taskboard_domain::StateCommand::UpdateTask {
            id: task_id_by_title(&app, "E1 task"),
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(due)),
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .await
        .expect("duedate set");

    wait_for(|| sink.len() == 1, "the notification").await;

    // Snapshot and drop the guard: no lock spans an await point.
    let notifications = sink.notifications.lock().expect("sink lock").clone();
    let notification = &notifications[0];
    assert_eq!(
        notification.reminder,
        taskboard_domain::ReminderRef::Rule(taskboard_domain::ReminderRuleId::from(rule_uuid))
    );
    assert_eq!(notification.title, "E1 task");
    // occurrence == duedate + offset: the 10 m-before rule fires at
    // due − 10 m.
    assert_eq!(
        notification.occurrence,
        due - chrono::TimeDelta::try_minutes(10).expect("10m")
    );

    // Terminal state: the delivery proof is persisted before the send.
    let state = app.engine.shared_state().load_full();
    let delivered = state
        .reminders
        .deliveries
        .iter()
        .any(|d| d.occurrence == notification.occurrence && matches!(d.reminder, taskboard_domain::ReminderRef::Rule(id) if id.as_uuid() == rule_uuid));
    assert!(delivered, "the delivery row is in the terminal state");

    app.shutdown().await;
}

fn task_id_by_title(app: &taskboard_app::App, title: &str) -> taskboard_domain::TaskId {
    let state = app.engine.shared_state().load_full();
    state
        .tasks
        .values()
        .find(|t| t.title == title && t.is_live())
        .expect("seeded task")
        .id
}

/// E2: at-most-once — several ticks later the sink still holds exactly
/// one delivery, and a *fresh* boot's immediate first tick adds nothing
/// (the delivery proof persisted).
#[tokio::test]
async fn e2_delivery_is_at_most_once_across_restarts() {
    let dir = tempdir();
    let config = e2e_config(dir.path(), "e2.db", "5m");
    let rule_id =
        e2e_seed_task_and_rule(&config, "E2 task", TimeDelta::try_minutes(10).expect("10m")).await;
    let rule_uuid = uuid::Uuid::parse_str(&rule_id).expect("rule id");

    let (app, sink) = e2e_boot(config.clone()).await;
    let due = chrono::Utc::now() - chrono::TimeDelta::try_minutes(10).expect("-10m");
    app.engine
        .execute(taskboard_domain::StateCommand::UpdateTask {
            id: task_id_by_title(&app, "E2 task"),
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(due)),
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .await
        .expect("duedate set");
    wait_for(|| sink.len() == 1, "the first notification").await;

    // Several more ticks: still exactly one.
    await_quiet(
        || sink.len() != 1,
        "no repeat within the window",
        || sink_dump(&sink),
    )
    .await;
    app.shutdown().await;

    // A fresh boot on the same db: the immediate first tick adds
    // nothing.
    let (app, sink) = e2e_boot(config.clone()).await;
    await_quiet(
        || sink.len() != 0,
        "the refire stays suppressed",
        || sink_dump(&sink),
    )
    .await;
    assert_eq!(
        sink.len(),
        0,
        "the persisted delivery suppresses the refire"
    );
    let state = app.engine.shared_state().load_full();
    assert!(
        state
            .reminders
            .deliveries
            .iter()
            .any(|d| matches!(d.reminder, taskboard_domain::ReminderRef::Rule(id) if id.as_uuid() == rule_uuid)),
        "the delivery proof hydrated"
    );
    app.shutdown().await;
}

/// E3: `enabled = false` never arms the tick — the same setup stays
/// silent and the shutdown is clean.
#[tokio::test]
async fn e3_disabled_config_stays_silent() {
    let dir = tempdir();
    let mut config = e2e_config(dir.path(), "e3.db", "5m");
    config.reminders.enabled = false;
    let rule_id =
        e2e_seed_task_and_rule(&config, "E3 task", TimeDelta::try_minutes(10).expect("10m")).await;
    let _ = uuid::Uuid::parse_str(&rule_id).expect("rule id");

    let (app, sink) = e2e_boot(config.clone()).await;
    let due = chrono::Utc::now() - chrono::TimeDelta::try_minutes(10).expect("-10m");
    app.engine
        .execute(taskboard_domain::StateCommand::UpdateTask {
            id: task_id_by_title(&app, "E3 task"),
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(due)),
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .await
        .expect("duedate set");

    await_quiet(
        || sink.len() != 0,
        "a disabled engine never fires",
        || sink_dump(&sink),
    )
    .await;
    assert_eq!(sink.len(), 0, "a disabled engine never fires");
    app.shutdown().await;
}

/// E4: the implicit default reminder (no explicit rule) fires at
/// `duedate - default_before`; with `default_before = "0s"` the
/// occurrence is exactly the duedate — the config→domain mapping end
/// to end.
#[tokio::test]
async fn e4_default_reminder_fires_at_the_configured_offset() {
    let dir = tempdir();
    let config = e2e_config(dir.path(), "e4.db", "0s");
    seed(&config, "Todo", "E4 task").await;

    let (app, sink) = e2e_boot(config.clone()).await;
    let due = chrono::Utc::now() - chrono::TimeDelta::try_minutes(5).expect("-5m");
    app.engine
        .execute(taskboard_domain::StateCommand::UpdateTask {
            id: task_id_by_title(&app, "E4 task"),
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(due)),
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .await
        .expect("duedate set");

    wait_for(|| sink.len() == 1, "the default notification").await;
    // Snapshot and drop the guard: no lock spans an await point.
    let notifications = sink.notifications.lock().expect("sink lock").clone();
    let notification = &notifications[0];
    let task_id = task_id_by_title(&app, "E4 task");
    assert_eq!(
        notification.reminder,
        taskboard_domain::ReminderRef::Default(task_id)
    );
    assert_eq!(notification.occurrence, due, "0s before due == at due");
    app.shutdown().await;
}
