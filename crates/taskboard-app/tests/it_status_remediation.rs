// SPDX-License-Identifier: MIT OR Apache-2.0

//! The status remediation executor (the D2 command plan W-7): the
//! typed decision logic over the effect seams (the chunk R-2 law —
//! no real process is ever spawned or signalled here; the signal /
//! liveness / removal / stop / spawn effects are injected fakes over
//! a real lock sidecar in a tempdir). Assertions are typed outcomes
//! and structural file facts; no text oracles, no sleeps.

use std::sync::Arc;

use taskboard_app::status_remediation::{
    RemediationEffects, RemediationWorkspace, StopLeg, run_remediation,
};
use taskboard_ui_slint::requests::{
    StatusRemediationAction as Action, StatusRemediationOutcome as Outcome,
    StatusRemediationRefusal as Refusal,
};

/// A tempdir lock sidecar carrying the given holder pid (the real
/// `lockfile` minter — the sidecar's shape is the production one).
fn sidecar_with(holder_pid: Option<u32>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("taskboard.db");
    let sidecar = taskboard_app::lockfile::lock_sidecar_path(&db);
    if let Some(pid) = holder_pid {
        std::fs::write(&sidecar, format!("pid={pid}\n")).expect("sidecar write");
    }
    (dir, db)
}

/// A workspace over the given sidecar database with every effect
/// fake: the effect log records what ran (the structural assertions'
/// oracle). `alive` answers the fake liveness probe.
fn fake_workspace(
    db: std::path::PathBuf,
    alive: Vec<u32>,
) -> (
    RemediationWorkspace,
    Arc<std::sync::Mutex<Vec<&'static str>>>,
) {
    let log: Arc<std::sync::Mutex<Vec<&'static str>>> = Arc::default();
    let alive = Arc::new(std::sync::Mutex::new(alive));
    let effects = RemediationEffects {
        pid_alive: {
            let alive = Arc::clone(&alive);
            Arc::new(move |pid| alive.lock().expect("lock").contains(&pid))
        },
        signal_term: {
            let log = Arc::clone(&log);
            Arc::new(move |_| {
                log.lock().expect("lock").push("signal");
                Ok(())
            })
        },
        remove_file: {
            let log = Arc::clone(&log);
            Arc::new(move |_| {
                log.lock().expect("lock").push("remove");
                Ok(())
            })
        },
        stop_daemon: Arc::new(|| Box::pin(async { StopLeg::NoDaemon })),
        spawn_daemon: {
            let log = Arc::clone(&log);
            Arc::new(move || {
                log.lock().expect("lock").push("spawn");
                Ok(424_242)
            })
        },
    };
    let workspace = RemediationWorkspace {
        db_path: Some(db),
        may_control_processes: true,
        effects,
    };
    (workspace, log)
}

/// W-7 (the plan's typed-outcome pin): each verb answers its typed
/// fact or refusal — a live holder kill signals, a stale holder kill
/// refuses `HolderNotAlive`, the self pid refuses `SelfPid`, and a
/// restart over the `NoDaemon` stop leg carries the spawn's pid.
#[tokio::test]
async fn remediation_outcomes_are_typed_per_action() {
    // The live-holder kill: one signal effect, the typed fact.
    let (_dir, db) = sidecar_with(Some(7_001));
    let (ws, log) = fake_workspace(db.clone(), vec![7_001]);
    assert_eq!(
        run_remediation(&ws, Action::KillHolder).await,
        Outcome::HolderSignalled,
        "a live holder is signalled (D-3)"
    );
    assert_eq!(&*log.lock().expect("lock"), &["signal"], "one SIGTERM ran");

    // The stale-holder kill: the typed refusal, no effect ran.
    let (_dir, db) = sidecar_with(Some(7_002));
    let (ws, log) = fake_workspace(db.clone(), vec![]);
    assert_eq!(
        run_remediation(&ws, Action::KillHolder).await,
        Outcome::Refused(Refusal::HolderNotAlive),
        "a stale holder refuses typed (D-3)"
    );
    assert!(log.lock().expect("lock").is_empty(), "no signal ran");

    // The self-pid kill: the more specific refusal, no effect ran.
    let (_dir, db) = sidecar_with(Some(std::process::id()));
    let (ws, log) = fake_workspace(db.clone(), vec![std::process::id()]);
    assert_eq!(
        run_remediation(&ws, Action::KillHolder).await,
        Outcome::Refused(Refusal::SelfPid),
        "the process's own pid refuses typed (D-7)"
    );
    assert!(log.lock().expect("lock").is_empty(), "no signal ran");

    // The restart over a NoDaemon stop leg: the stop is skipped (not
    // a refusal) and the spawn fact carries the new pid (D-5).
    let (_dir, db) = sidecar_with(None);
    let (ws, log) = fake_workspace(db.clone(), vec![]);
    assert_eq!(
        run_remediation(&ws, Action::RestartDaemon).await,
        Outcome::DaemonRestarted { pid: 424_242 },
        "the restart's outcome carries the new pid (D-5)"
    );
    assert_eq!(&*log.lock().expect("lock"), &["spawn"], "one spawn ran");

    // The typed-permission gate (the trap-a law): the kiosk posture
    // refuses typed before any effect runs.
    let (_dir, db) = sidecar_with(Some(7_003));
    let (mut ws, log) = fake_workspace(db.clone(), vec![7_003]);
    ws.may_control_processes = false;
    assert_eq!(
        run_remediation(&ws, Action::KillHolder).await,
        Outcome::Refused(Refusal::IpcUnavailable),
        "the forbidden posture refuses typed"
    );
    assert!(log.lock().expect("lock").is_empty(), "no effect ran");
}

/// US-2/S3 (the plan W-7 row's journey-named leg): a live holder
/// appearing between arm and confirm — the executor's authorization
/// re-check refuses typed (`HolderAlive`) and the sidecar file is
/// untouched (the enable flag is presentation, not authorization).
#[tokio::test]
async fn us2_s3_live_holder_refusal_folds_and_the_sidecar_is_untouched() {
    let (_dir, db) = sidecar_with(Some(7_100));
    let sidecar = taskboard_app::lockfile::lock_sidecar_path(&db);
    let (ws, log) = fake_workspace(db, vec![7_100]);
    assert_eq!(
        run_remediation(&ws, Action::ForceUnlock).await,
        Outcome::Refused(Refusal::HolderAlive),
        "a live holder refuses the unlock typed (D-4)"
    );
    assert!(log.lock().expect("lock").is_empty(), "no removal ran");
    assert!(
        sidecar.exists(),
        "the sidecar is untouched under the refusal (the typed law)"
    );
}

/// US-3/S3 (the plan W-7 row's journey-named leg): a failed spawn
/// leg answers the typed `SpawnFailed` refusal (the stop leg having
/// settled first — the letter's true-posture re-query is the UI
/// half).
#[tokio::test]
async fn us3_s3_spawn_failure_folds_the_typed_refusal() {
    let (_dir, db) = sidecar_with(None);
    let effects = RemediationEffects {
        stop_daemon: Arc::new(|| Box::pin(async { StopLeg::Stopped })),
        spawn_daemon: Arc::new(|| Err(std::io::Error::other("spawn refused"))),
        ..RemediationEffects::default()
    };
    let ws = RemediationWorkspace {
        db_path: Some(db),
        may_control_processes: true,
        effects,
    };
    assert_eq!(
        run_remediation(&ws, Action::RestartDaemon).await,
        Outcome::Refused(Refusal::SpawnFailed),
        "a failed spawn leg refuses typed (D-5/D-7)"
    );
}
