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
