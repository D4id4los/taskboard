// SPDX-License-Identifier: MIT OR Apache-2.0

//! The status panel's remediation executor (the command plan W-7,
//! chunk D2): the app-side half of `UiRequest::StatusRemediate` —
//! three typed verbs over process/lock posture, never domain state
//! (the plan R-3: no `StateCommand` exists for the lane).
//!
//! The typed decision logic (D-3/D-4/D-5/D-7) is pure over the
//! [`RemediationEffects`] seams; every process effect — the SIGTERM,
//! the liveness probe, the sidecar removal, the IPC stop, the
//! detached spawn — rides a seam so tests drive the executor with
//! fakes and the effect layer stays thin (the chunk R-2 seam law).
//! The defaults are the real effects; the kiosk lifecycle's
//! [`RemediationWorkspace::may_control_processes`] is `false`, which
//! refuses the lane typed before any effect runs (the trap-a
//! typed-permission law: a passive kiosk display never shells out —
//! the row-disable flags are presentation, this flag is the
//! authorization's second half).

use std::sync::Arc;

use taskboard_ui_slint::requests::{
    StatusRemediationAction, StatusRemediationOutcome, StatusRemediationRefusal as Refusal,
};

/// The executor's process-effect seams (the chunk R-2 law): boxed
/// functions the tests substitute; the [`RemediationEffects::real`]
/// constructor carries the production effects.
#[derive(Clone)]
pub struct RemediationEffects {
    /// Is the pid alive? (the liveness probe — the kill's stale check
    /// and the unlock's authorization re-check).
    #[allow(clippy::type_complexity)]
    pub pid_alive: Arc<dyn Fn(u32) -> bool + Send + Sync>,
    /// Deliver SIGTERM to the pid (the D-3 verb's effect).
    #[allow(clippy::type_complexity)]
    pub signal_term: Arc<dyn Fn(u32) -> std::io::Result<()> + Send + Sync>,
    /// Remove a file (the stale lock sidecar's removal).
    #[allow(clippy::type_complexity)]
    pub remove_file: Arc<dyn Fn(&std::path::Path) -> std::io::Result<()> + Send + Sync>,
    /// The restart's stop leg: graceful `daemon stop` over IPC.
    /// `Stopped` = a daemon served and acknowledged the stop;
    /// `NoDaemon` = nothing serves the database (the stop leg is
    /// skipped — the D-5 fact, not a refusal); `Unavailable` = the
    /// IPC lane refused (the typed `IpcUnavailable`).
    #[allow(clippy::type_complexity)]
    pub stop_daemon: Arc<
        dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = StopLeg> + Send>>
            + Send
            + Sync,
    >,
    /// The restart's spawn leg: a detached `taskboard daemon run`,
    /// answering the new daemon's pid.
    #[allow(clippy::type_complexity)]
    pub spawn_daemon: Arc<dyn Fn() -> std::io::Result<u32> + Send + Sync>,
}

impl Default for RemediationEffects {
    fn default() -> Self {
        Self::real(None)
    }
}

/// The stop leg's three facts (see [`RemediationEffects::stop_daemon`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopLeg {
    Stopped,
    NoDaemon,
    Unavailable,
}

/// The remediation lane's workspace: the database home (the lock
/// sidecar's address), the typed process-control permission, and the
/// effect seams. Built once per UI lifecycle beside the
/// [`crate::ui_requests::ConnectionWorkspace`] it rides; the
/// `Default` is the forbidden/inert posture (no database home, no
/// process control — the kiosk posture).
#[derive(Default)]
pub struct RemediationWorkspace {
    /// The resolved database path (`None` — a platform with no data
    /// dir — degrades to the typed `IpcUnavailable` refusal).
    pub db_path: Option<std::path::PathBuf>,
    /// The lifecycle's typed permission (the trap-a law): the desktop
    /// lifecycle is `true`; the kiosk lifecycle is `false` and the
    /// whole lane refuses typed before any effect runs.
    pub may_control_processes: bool,
    /// The process-effect seams.
    pub effects: RemediationEffects,
}

impl RemediationEffects {
    /// The production effects: the nix SIGTERM and liveness probe
    /// (`kill(pid, None)` — `ESRCH` is the dead fact), the std
    /// sidecar removal, the IPC shutdown hop, and the detached
    /// `daemon run` spawn of this same executable.
    #[must_use]
    pub fn real(db_path: Option<std::path::PathBuf>) -> Self {
        Self {
            pid_alive: Arc::new(pid_alive_real),
            signal_term: Arc::new(signal_term_real),
            remove_file: Arc::new(|path| std::fs::remove_file(path)),
            stop_daemon: Arc::new(move || {
                let db_path = db_path.clone();
                Box::pin(async move {
                    let Some(db_path) = db_path else {
                        return StopLeg::Unavailable;
                    };
                    let Some(connection) = crate::ipc::client::try_connect(&db_path).await else {
                        return StopLeg::NoDaemon;
                    };
                    match connection.shutdown(None).await {
                        Ok(()) => StopLeg::Stopped,
                        Err(_) => StopLeg::Unavailable,
                    }
                })
            }),
            spawn_daemon: Arc::new(spawn_daemon_real),
        }
    }
}

/// The production liveness probe: signal 0 — `ESRCH` is dead,
/// `EPERM` is alive-but-unprivileged (alive), anything else probes
/// conservatively alive (the kill's typed refusal carries the
/// verdict either way).
fn pid_alive_real(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // `ESRCH` is the dead fact; every other outcome (delivered,
    // unprivileged) probes alive — the kill's typed refusal carries
    // the verdict either way.
    !matches!(
        kill(Pid::from_raw(i32::try_from(pid).unwrap_or(-1)), None),
        Err(nix::errno::Errno::ESRCH)
    )
}

/// The production SIGTERM (the D-3 verb's effect).
fn signal_term_real(pid: u32) -> std::io::Result<()> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    kill(
        Pid::from_raw(i32::try_from(pid).unwrap_or(-1)),
        Signal::SIGTERM,
    )
    .map_err(|err| std::io::Error::other(err.to_string()))
}

/// The production spawn leg: a detached `taskboard daemon run` of
/// this same executable — detached session (its own process group),
/// all three std handles null, so the child outlives the GUI process
/// that spawned it.
fn spawn_daemon_real() -> std::io::Result<u32> {
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new(exe);
    command.args(["daemon", "run"]);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let child = command.spawn()?;
    Ok(child.id())
}

/// The executor (the W-7 seam): one typed decision per action. The
/// permission gate is the authorization's first half (the kiosk
/// posture refuses before any effect); the liveness re-checks are
/// the second (the row flags are presentation, never authorization).
pub async fn run_remediation(
    workspace: &RemediationWorkspace,
    action: StatusRemediationAction,
) -> StatusRemediationOutcome {
    if !workspace.may_control_processes {
        return StatusRemediationOutcome::Refused(Refusal::IpcUnavailable);
    }
    match action {
        StatusRemediationAction::KillHolder => kill_holder(workspace),
        StatusRemediationAction::ForceUnlock => force_unlock(workspace),
        StatusRemediationAction::RestartDaemon => restart_daemon(workspace).await,
    }
}

/// The displayed holder pid (the sidecar's recorded fact — the same
/// read the `status` verb's diagnostic field uses).
fn holder_pid(workspace: &RemediationWorkspace) -> Option<u32> {
    workspace
        .db_path
        .as_deref()
        .and_then(crate::lockfile::holder_pid)
}

/// The D-3 verb: SIGTERM to the displayed pid. Refuses typed when the
/// pid is stale (`HolderNotAlive` — `ForceUnlock` is the right tool) or
/// is this process (`SelfPid`).
fn kill_holder(workspace: &RemediationWorkspace) -> StatusRemediationOutcome {
    let Some(pid) = holder_pid(workspace) else {
        return StatusRemediationOutcome::Refused(Refusal::HolderNotAlive);
    };
    if pid == std::process::id() {
        return StatusRemediationOutcome::Refused(Refusal::SelfPid);
    }
    if !(workspace.effects.pid_alive)(pid) {
        return StatusRemediationOutcome::Refused(Refusal::HolderNotAlive);
    }
    match (workspace.effects.signal_term)(pid) {
        Ok(()) => StatusRemediationOutcome::HolderSignalled,
        Err(_) => StatusRemediationOutcome::Refused(Refusal::IpcUnavailable),
    }
}

/// The D-4 verb: remove the stale lock sidecar. The authorization is
/// the executor's liveness re-check (never the row's enable flag): a
/// live holder refuses typed (`HolderAlive`; the process's own pid
/// is the more specific `SelfPid`) and the sidecar is untouched.
fn force_unlock(workspace: &RemediationWorkspace) -> StatusRemediationOutcome {
    if let Some(pid) = holder_pid(workspace)
        && (workspace.effects.pid_alive)(pid)
    {
        let kind = if pid == std::process::id() {
            Refusal::SelfPid
        } else {
            Refusal::HolderAlive
        };
        return StatusRemediationOutcome::Refused(kind);
    }
    let Some(db_path) = workspace.db_path.as_deref() else {
        return StatusRemediationOutcome::Refused(Refusal::IpcUnavailable);
    };
    let sidecar = crate::lockfile::lock_sidecar_path(db_path);
    match (workspace.effects.remove_file)(&sidecar) {
        Ok(()) => StatusRemediationOutcome::LockReleased,
        Err(_) => StatusRemediationOutcome::Refused(Refusal::IpcUnavailable),
    }
}

/// The D-5 verb: graceful stop over IPC when serving (`NoDaemon` is
/// the skip fact, not a refusal), then the detached spawn; the typed
/// outcome carries the new pid (`SpawnFailed` on the spawn leg's
/// refusal).
async fn restart_daemon(workspace: &RemediationWorkspace) -> StatusRemediationOutcome {
    match (workspace.effects.stop_daemon)().await {
        StopLeg::Stopped | StopLeg::NoDaemon => {}
        StopLeg::Unavailable => {
            return StatusRemediationOutcome::Refused(Refusal::IpcUnavailable);
        }
    }
    match (workspace.effects.spawn_daemon)() {
        Ok(pid) => StatusRemediationOutcome::DaemonRestarted { pid },
        Err(_) => StatusRemediationOutcome::Refused(Refusal::SpawnFailed),
    }
}

/// The desktop-terminal workspace over this machine's database (the
/// shared builder of both fronts — `taskboard unlock`/`daemon
/// restart` and the REPL's in-session unlock arm; #1348's hoist):
/// the desktop posture is the authorized one
/// (`may_control_processes: true`) and the effects are the production
/// seams verbatim, so every front gets the same typed semantics the
/// panel renders.
#[must_use]
pub fn real_workspace(db_path: std::path::PathBuf) -> RemediationWorkspace {
    RemediationWorkspace {
        db_path: Some(db_path.clone()),
        may_control_processes: true,
        effects: RemediationEffects::real(Some(db_path)),
    }
}

/// The typed outcome's terminal fold line (#1348): the human-readable
/// spelling both fronts print — one semantics, one renderer. The
/// refusal's kind is the payload (`{kind:?}` — the variant name, the
/// typed-outcomes law); the JSON spelling stays `serde_json` over the
/// outcome itself (see each front's emit path).
#[must_use]
pub fn outcome_line(outcome: &StatusRemediationOutcome) -> String {
    match outcome {
        StatusRemediationOutcome::LockReleased => "lock sidecar removed".to_owned(),
        StatusRemediationOutcome::HolderSignalled => "lock holder signalled".to_owned(),
        StatusRemediationOutcome::DaemonRestarted { pid } => {
            format!("daemon restarted (pid {pid})")
        }
        StatusRemediationOutcome::Refused(kind) => format!("refused: {kind:?}"),
    }
}

/// The fold's exit code: 0 executed / 1 refused — the refusal's kind
/// is the payload, never message text.
#[must_use]
pub fn outcome_exit_code(outcome: &StatusRemediationOutcome) -> u8 {
    u8::from(matches!(outcome, StatusRemediationOutcome::Refused(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    /// The shared fold's snapshot (#1348): every outcome variant's
    /// terminal line, over the exact `StatusRemediationOutcome` data —
    /// both fronts (`taskboard unlock`, the in-session arm) render
    /// this, so the snapshot pins the one semantics. Exit codes are
    /// asserted as typed facts beside it, never as text.
    #[test]
    fn the_outcome_fold_lines_snap_both_fronts_to_one_semantics() {
        let outcomes = [
            StatusRemediationOutcome::LockReleased,
            StatusRemediationOutcome::HolderSignalled,
            StatusRemediationOutcome::DaemonRestarted { pid: 4242 },
            StatusRemediationOutcome::Refused(Refusal::SelfPid),
            StatusRemediationOutcome::Refused(Refusal::HolderAlive),
        ];
        let mut rendered = String::new();
        for outcome in &outcomes {
            rendered += &outcome_line(outcome);
            rendered += "\texit=";
            rendered += &outcome_exit_code(outcome).to_string();
            rendered += "\n";
        }
        assert_snapshot!(rendered);
    }

    /// The refusal kind is the payload: every refusal exits 1, every
    /// executed verb exits 0 — typed facts, no text assertions.
    #[test]
    fn the_exit_code_folds_refusals_to_one() {
        for outcome in [
            StatusRemediationOutcome::LockReleased,
            StatusRemediationOutcome::HolderSignalled,
            StatusRemediationOutcome::DaemonRestarted { pid: 1 },
        ] {
            assert_eq!(outcome_exit_code(&outcome), 0, "executed verbs exit 0");
        }
        for kind in [
            Refusal::HolderNotAlive,
            Refusal::HolderAlive,
            Refusal::SelfPid,
            Refusal::SpawnFailed,
            Refusal::IpcUnavailable,
        ] {
            assert_eq!(
                outcome_exit_code(&StatusRemediationOutcome::Refused(kind)),
                1,
                "refusals exit 1"
            );
        }
    }
}

/// The hermetic decision-table suite (#1373): the executor's typed
/// decisions over fake effect seams — tier 1, the wiring stays with
/// the E2E suite. Every row asserts the typed outcome plus the
/// recorded-effect facts (what fired, in what order, addressed where);
/// no message text anywhere.
#[cfg(test)]
mod decision_tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// One recorded process effect (the fake seams append here).
    #[derive(Debug, PartialEq, Eq, Clone)]
    enum EffectCall {
        Signal(u32),
        Remove(PathBuf),
        Stop,
        Spawn,
    }

    type Log = Arc<Mutex<Vec<EffectCall>>>;

    /// Fake effects over a shared recording log (fakes over mocks):
    /// the liveness verdict, the signal/remove results, the stop leg,
    /// and the spawn result are each fixed by the scenario.
    #[allow(clippy::fn_params_excessive_bools)]
    fn fake_effects(
        alive: bool,
        signal_ok: bool,
        remove_ok: bool,
        stop_leg: StopLeg,
        spawn_ok: bool,
    ) -> (RemediationEffects, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let signal_log = Arc::clone(&log);
        let remove_log = Arc::clone(&log);
        let halt_log = Arc::clone(&log);
        let spawn_log = Arc::clone(&log);
        let effects = RemediationEffects {
            pid_alive: Arc::new(move |_| alive),
            signal_term: Arc::new(move |pid| {
                signal_log
                    .lock()
                    .expect("log")
                    .push(EffectCall::Signal(pid));
                if signal_ok {
                    Ok(())
                } else {
                    Err(std::io::Error::other("signal refused"))
                }
            }),
            remove_file: Arc::new(move |path| {
                remove_log
                    .lock()
                    .expect("log")
                    .push(EffectCall::Remove(path.to_path_buf()));
                if remove_ok {
                    Ok(())
                } else {
                    Err(std::io::Error::other("removal refused"))
                }
            }),
            stop_daemon: Arc::new(move || {
                halt_log.lock().expect("log").push(EffectCall::Stop);
                Box::pin(async move { stop_leg })
            }),
            spawn_daemon: Arc::new(move || {
                spawn_log.lock().expect("log").push(EffectCall::Spawn);
                if spawn_ok {
                    Ok(4321)
                } else {
                    Err(std::io::Error::other("spawn refused"))
                }
            }),
        };
        (effects, log)
    }

    fn recorded(log: &Log) -> Vec<EffectCall> {
        log.lock().expect("log").clone()
    }

    /// A desktop-posture workspace over a tempdir database, the lock
    /// sidecar seeded with the given recorded pid (`None` writes no
    /// sidecar). Returns the dir so the sidecar outlives the call.
    fn seeded_workspace(
        sidecar_pid: Option<u32>,
        effects: RemediationEffects,
    ) -> (RemediationWorkspace, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("board.sqlite");
        if let Some(pid) = sidecar_pid {
            std::fs::write(
                crate::lockfile::lock_sidecar_path(&db),
                format!("pid={pid}\n"),
            )
            .expect("seed sidecar");
        }
        (
            RemediationWorkspace {
                db_path: Some(db),
                may_control_processes: true,
                effects,
            },
            dir,
        )
    }

    fn refused_kind(outcome: &StatusRemediationOutcome) -> Option<Refusal> {
        match outcome {
            StatusRemediationOutcome::Refused(kind) => Some(*kind),
            _ => None,
        }
    }

    /// Trap-a: the kiosk posture refuses all three actions typed before
    /// any effect runs — the effect log stays empty.
    #[tokio::test]
    async fn kiosk_posture_refuses_every_action_before_any_effect() {
        for action in [
            StatusRemediationAction::KillHolder,
            StatusRemediationAction::ForceUnlock,
            StatusRemediationAction::RestartDaemon,
        ] {
            let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
            let workspace = RemediationWorkspace {
                db_path: None,
                may_control_processes: false,
                effects,
            };
            let outcome = run_remediation(&workspace, action).await;
            assert_eq!(
                refused_kind(&outcome),
                Some(Refusal::IpcUnavailable),
                "kiosk posture refuses {action:?}"
            );
            assert!(recorded(&log).is_empty(), "no effect fired for {action:?}");
        }
    }

    #[tokio::test]
    async fn kill_without_a_sidecar_pid_refuses_holder_not_alive() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(None, effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::KillHolder).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::HolderNotAlive));
        assert!(recorded(&log).is_empty(), "no signal without a pid");
    }

    #[tokio::test]
    async fn kill_of_our_own_pid_refuses_self_pid() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(Some(std::process::id()), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::KillHolder).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::SelfPid));
        assert!(recorded(&log).is_empty(), "no signal to self");
    }

    #[tokio::test]
    async fn kill_of_a_stale_pid_refuses_holder_not_alive_without_signalling() {
        let (effects, log) = fake_effects(false, true, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(Some(999_999), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::KillHolder).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::HolderNotAlive));
        assert!(
            recorded(&log).is_empty(),
            "the liveness re-check authorizes"
        );
    }

    #[tokio::test]
    async fn kill_of_a_live_foreign_pid_signals_the_recorded_pid() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(Some(4242), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::KillHolder).await;
        assert_eq!(outcome, StatusRemediationOutcome::HolderSignalled);
        assert_eq!(recorded(&log), vec![EffectCall::Signal(4242)]);
    }

    #[tokio::test]
    async fn kill_with_a_failing_signal_refuses_ipc_unavailable() {
        let (effects, log) = fake_effects(true, false, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(Some(4242), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::KillHolder).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::IpcUnavailable));
        assert_eq!(recorded(&log), vec![EffectCall::Signal(4242)]);
    }

    #[tokio::test]
    async fn force_unlock_with_a_live_foreign_pid_refuses_and_keeps_the_sidecar() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
        let (workspace, dir) = seeded_workspace(Some(4242), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::ForceUnlock).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::HolderAlive));
        assert!(recorded(&log).is_empty(), "no removal while a holder lives");
        let db = dir.path().join("board.sqlite");
        assert!(
            crate::lockfile::lock_sidecar_path(&db).exists(),
            "the sidecar is untouched"
        );
    }

    #[tokio::test]
    async fn force_unlock_of_our_own_pid_refuses_self_pid() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(Some(std::process::id()), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::ForceUnlock).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::SelfPid));
        assert_eq!(recorded(&log), []);
    }

    #[tokio::test]
    async fn force_unlock_of_a_stale_pid_releases_the_lock_at_the_sidecar_address() {
        let (effects, log) = fake_effects(false, true, true, StopLeg::Stopped, true);
        let (workspace, dir) = seeded_workspace(Some(999_999), effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::ForceUnlock).await;
        assert_eq!(outcome, StatusRemediationOutcome::LockReleased);
        let db = dir.path().join("board.sqlite");
        assert_eq!(
            recorded(&log),
            vec![EffectCall::Remove(crate::lockfile::lock_sidecar_path(&db))],
            "the removal address is the lock sidecar's derived path"
        );
    }

    #[tokio::test]
    async fn force_unlock_with_a_failing_removal_refuses_ipc_unavailable() {
        let (effects, log) = fake_effects(false, true, false, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(None, effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::ForceUnlock).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::IpcUnavailable));
        assert_eq!(recorded(&log).len(), 1, "exactly the failed removal fired");
    }

    /// No database home (a platform without a data dir) degrades to the
    /// typed `IpcUnavailable` refusal — nothing to remove, no address.
    #[tokio::test]
    async fn force_unlock_without_a_db_path_refuses_ipc_unavailable() {
        let (effects, log) = fake_effects(false, true, true, StopLeg::Stopped, true);
        let workspace = RemediationWorkspace {
            db_path: None,
            may_control_processes: true,
            effects,
        };
        let outcome = run_remediation(&workspace, StatusRemediationAction::ForceUnlock).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::IpcUnavailable));
        assert!(recorded(&log).is_empty(), "no removal address exists");
    }

    #[tokio::test]
    async fn restart_with_an_unavailable_stop_leg_refuses_and_never_spawns() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Unavailable, true);
        let (workspace, _dir) = seeded_workspace(None, effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::RestartDaemon).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::IpcUnavailable));
        assert_eq!(recorded(&log), vec![EffectCall::Stop], "spawn never called");
    }

    #[tokio::test]
    async fn restart_after_a_stopped_leg_spawns_and_reports_the_pid() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, true);
        let (workspace, _dir) = seeded_workspace(None, effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::RestartDaemon).await;
        assert_eq!(
            outcome,
            StatusRemediationOutcome::DaemonRestarted { pid: 4321 }
        );
        assert_eq!(recorded(&log), vec![EffectCall::Stop, EffectCall::Spawn]);
    }

    #[tokio::test]
    async fn restart_with_no_daemon_serving_skips_the_stop_leg_and_spawns() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::NoDaemon, true);
        let (workspace, _dir) = seeded_workspace(None, effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::RestartDaemon).await;
        assert_eq!(
            outcome,
            StatusRemediationOutcome::DaemonRestarted { pid: 4321 },
            "the stop skip is a fact, not a refusal"
        );
        assert_eq!(recorded(&log), vec![EffectCall::Stop, EffectCall::Spawn]);
    }

    #[tokio::test]
    async fn restart_with_a_failing_spawn_refuses_spawn_failed() {
        let (effects, log) = fake_effects(true, true, true, StopLeg::Stopped, false);
        let (workspace, _dir) = seeded_workspace(None, effects);
        let outcome = run_remediation(&workspace, StatusRemediationAction::RestartDaemon).await;
        assert_eq!(refused_kind(&outcome), Some(Refusal::SpawnFailed));
        assert_eq!(recorded(&log), vec![EffectCall::Stop, EffectCall::Spawn]);
    }
}
