// SPDX-License-Identifier: AGPL-3.0-only
//! The exit-code table (#1230, #1175): the one-shot `*_command`
//! dispatchers propagate the `run_core` exit contract — success
//! receipt = 0, typed error = `report_to`'s code (1). One behavior,
//! one table: each row drives a real `*_command` over a standalone
//! tempdir config, `CommandIo` into `Vec` sinks, and asserts the
//! typed code only (never stderr text). Success legs kill the
//! `-> 1` replacements; failure legs kill the `-> 0` replacements.

use taskboard_app::cli::Command;
use taskboard_app::commands::boards;
use taskboard_app::commands::breaks::{self, BreaksInvocation};
use taskboard_app::commands::habits::{self, HabitsInvocation};
use taskboard_app::commands::reminders::{self, RemindInvocation};
use taskboard_app::commands::repeat::{self, RepeatInvocation};
use taskboard_app::commands::tasks;
use taskboard_app::commands::timers::{self, TimersInvocation};
use taskboard_app::commands::track::{self, TrackInvocation};
use taskboard_app::commands::{BasicSession, CommandIo, dispatch_line, logout_command};
use taskboard_app::config::AppConfig;

use crate::common;

use common::{forbidden_store, tempdir};

/// A standalone config bound to `dir`, mirrored to a real
/// `taskboard.toml` on disk (the `*_command` tier resolves through
/// `AppConfig::load(Some(path))`, not an injected `AppConfig`).
/// Returns (config, config path) over one shared db name.
fn disk_config(dir: &std::path::Path, db: &str) -> (AppConfig, std::path::PathBuf) {
    let config = common::standalone_config(dir, db);
    let config_path = dir.join("taskboard.toml");
    let toml = format!("[storage]\ndb_path = '{}'\n", dir.join(db).display());
    std::fs::write(&config_path, toml).expect("config written");
    (config, config_path)
}

/// The `Vec`-backed `CommandIo` every row drives its dispatcher into.
fn vec_io<'a>(out: &'a mut Vec<u8>, err: &'a mut Vec<u8>) -> CommandIo<'a> {
    CommandIo {
        out,
        err,
        json: true,
    }
}

/// Success legs: the pure-read modes over an empty standalone db each
/// yield the success receipt and exit 0. Kills every `*_command -> 1`
/// replacement (the mutated dispatcher cannot return 0).
#[tokio::test]
async fn exit_code_table_read_modes_all_exit_zero() {
    let dir = tempdir();
    let (config, config_path) = disk_config(dir.path(), "exit0.db");
    // The verbs resolve against the single-live-board default; seed it
    // so the pure reads are genuinely on their success receipt.
    boards::create(config.clone(), forbidden_store(), "seed board", None)
        .await
        .expect("board create boots");

    let mut cases: Vec<(String, u8)> = Vec::new();

    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code =
            breaks::breaks_command(&mut io, Some(config_path.clone()), BreaksInvocation::List)
                .await;
        cases.push(("breaks list".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code =
            timers::timers_command(&mut io, Some(config_path.clone()), TimersInvocation::List)
                .await;
        cases.push(("timers list".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code =
            reminders::remind_command(&mut io, Some(config_path.clone()), RemindInvocation::List)
                .await;
        cases.push(("remind list".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code = habits::habits_command(
            &mut io,
            Some(config_path.clone()),
            HabitsInvocation::List { board: None },
        )
        .await;
        cases.push(("habits list".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code =
            track::track_command(&mut io, Some(config_path.clone()), TrackInvocation::Status).await;
        cases.push(("track status".into(), code));
    }

    for (verb, code) in &cases {
        assert_eq!(*code, 0, "{verb} must exit 0 on the success receipt");
    }
}

/// `repeat --off` success leg: a seeded head whose rule is cleared —
/// the typed receipt exits 0 (kills `repeat_command -> 1`). D2 fork
/// recorded: `Off` on a rule-less head is a typed no-op receipt, so
/// the seeded head carries no rule; the seed exists only to prove the
/// task resolution + receipt path.
#[tokio::test]
async fn repeat_off_on_seeded_head_exits_zero() {
    let dir = tempdir();
    let (config, config_path) = disk_config(dir.path(), "repeat0.db");
    boards::create(config.clone(), forbidden_store(), "seed board", None)
        .await
        .expect("board create boots");
    tasks::add(config.clone(), forbidden_store(), "seeded", None)
        .await
        .expect("task seeded");

    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = vec_io(&mut out, &mut err);
    let code = repeat::repeat_command(
        &mut io,
        Some(config_path),
        RepeatInvocation::Off {
            task: "seeded".into(),
        },
    )
    .await;
    assert_eq!(code, 0, "the off receipt exits 0");
}

/// Failure legs: an unknown selector is a typed `CommandError`, and
/// `report_to` yields exit 1. Kills every `*_command -> 0`
/// replacement (the mutated dispatcher cannot return 1).
#[tokio::test]
async fn exit_code_table_unknown_selectors_exit_one() {
    let dir = tempdir();
    let (config, config_path) = disk_config(dir.path(), "exit1.db");
    boards::create(config.clone(), forbidden_store(), "seed board", None)
        .await
        .expect("board create boots");

    let mut cases: Vec<(String, u8)> = Vec::new();

    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        // UnknownHabit — the typed refusal for an unresolvable selector.
        let code = habits::habits_command(
            &mut io,
            Some(config_path.clone()),
            HabitsInvocation::Log {
                habit: "zzzzzzzz".into(),
            },
        )
        .await;
        cases.push(("habits log unknown".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code = breaks::breaks_command(
            &mut io,
            Some(config_path.clone()),
            BreaksInvocation::Remove("zzzzzzzz".into()),
        )
        .await;
        cases.push(("breaks remove unknown".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        let code = timers::timers_command(
            &mut io,
            Some(config_path.clone()),
            TimersInvocation::Cancel("zzzzzzzz".into()),
        )
        .await;
        cases.push(("timers cancel unknown".into(), code));
    }
    {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut io = vec_io(&mut out, &mut err);
        // UnknownTask — `--clear` on a task that does not exist.
        let code = reminders::remind_command(
            &mut io,
            Some(config_path.clone()),
            RemindInvocation::Clear("no such task".into()),
        )
        .await;
        cases.push(("remind clear unknown".into(), code));
    }

    for (verb, code) in &cases {
        assert_eq!(*code, 1, "{verb} must exit 1 on the typed refusal");
    }
}

/// The session-lane failure leg (`habits_line -> 0` kill): a booted
/// app's `dispatch_line` over an unknown-habit edit refuses with the
/// typed exit code 1 — the mutated `habits_line` returning 0 cannot.
#[tokio::test]
async fn session_lane_unknown_habit_edit_exits_one() {
    let dir = tempdir();
    let (config, _) = disk_config(dir.path(), "lane1.db");
    let store = forbidden_store();
    let app = taskboard_app::bootstrap::bootstrap_in(
        config.clone(),
        store.clone(),
        std::sync::Arc::new(taskboard_app::notifications::LogSink),
        taskboard_app::bootstrap::BootIdentity::At(dir.path().to_owned()),
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
    let mut io = vec_io(&mut out, &mut err);
    let code = dispatch_line(
        &session,
        &app,
        Command::Habits {
            cmd: taskboard_app::cli::HabitsCmd::Log {
                habit: "zzzzzzzz".into(),
            },
        },
        &mut io,
    )
    .await;
    app.shutdown().await;
    assert_eq!(code, 1, "the session lane's typed refusal exits 1");
}

/// The logout leg (#1175, D4): the one-shot dispatcher deletes (or —
/// the file-store leg recorded per plan §9 — idempotently accepts the
/// already-absent secret), the daemon nudge degrades typedly (no
/// daemon socket exists in the test env), and the exit code stays 0.
/// The nudge never moves the exit code: the mutation is on the
/// success receipt, which the degraded nudge cannot redirect. Kills
/// `logout_command -> 1`. The no-daemon degradation maps to the clean
/// `DaemonConnectorState::None` arm (`nudge_warning`'s contract: `None`
/// for the clean arms), so the sink stays empty by design — asserted
/// nowhere.
#[tokio::test]
async fn logout_with_degraded_nudge_still_exits_zero() {
    let dir = tempdir();
    let config_path = dir.path().join("taskboard.toml");
    let toml = format!(
        "[nextcloud]\nserver_url = 'https://cloud.example.com'\nusername = 'alice'\n\
         credential_store = 'file'\n[storage]\ndb_path = '{}'\n",
        dir.path().join("logout.db").display()
    );
    std::fs::write(&config_path, toml).expect("config written");

    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut io = vec_io(&mut out, &mut err);
    let code = logout_command(&mut io, Some(config_path.clone())).await;
    assert_eq!(
        code, 0,
        "logout succeeds (idempotent empty store) and the degraded nudge never moves the code"
    );
    // The identity keys left the file: the teardown really ran.
    let text = std::fs::read_to_string(&config_path).expect("config readable");
    assert!(!text.contains("server_url"));
    assert!(!text.contains("username"));
}
