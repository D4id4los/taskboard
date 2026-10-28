// SPDX-License-Identifier: AGPL-3.0-only
//! DR-2/D process tier (design D-6/D-7/D-9): the daemon surface's
//! signal ladder and exit-code contract, pinned against the *real*
//! binary. Exit codes are process properties, and the ladder's
//! handlers live in watcher tasks on the production runtime — D-7
//! forbids installing them on hermetic test paths, so no in-process
//! test can pin a code. Real signals are only sanctioned at the
//! outermost tier (the pty precedent); the daemon is headless, so the
//! plain-child tier is the lightweight form: `binary_command()` +
//! `write_test_config` + polled serving, `kill` to the child,
//! `wait()` for the code — no pty needed.
//!
//! The inter-kill pacing of 500 ms beats standard-signal coalescing
//! (the pty Case-4 comment sanctions exactly this); it is a
//! determinism gap between two kills, never a wait-for-state sleep.
//! Assertions are typed outcomes (exit codes), never log text.

#![cfg(unix)]

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

mod common;

use common::{Child, binary_command, spawn_daemon, tempdir, write_test_config};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// A standalone-install (T9) or connector-install (T11–T14) config dir
/// with a live wiremock Deck server for the connector shape.
struct Install {
    dir: tempfile::TempDir,
    password: Option<&'static str>,
}

impl Install {
    /// A standalone install: no server url → auto `standalone`.
    fn standalone(name: &str) -> Self {
        let dir = tempdir();
        let config = dir.path().join(format!("{name}.toml"));
        write_test_config(&config, None, None).expect("write config");
        std::fs::rename(&config, dir.path().join("taskboard.toml")).expect("rename");
        Self {
            dir,
            password: None,
        }
    }

    /// A connector install: the boot sync cycle parks inside the
    /// mounted delayed route (the drain wedge), `shutdown_timeout_secs`
    /// names the bounded join that holds the drain open.
    fn wedged_connector(
        _name: &str,
        server: &wiremock::MockServer,
        shutdown_timeout_secs: u64,
    ) -> Self {
        let dir = tempdir();
        let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
        let _ = write!(
            toml,
            "server_url = '{}'\nusername = 'alice'\n",
            server.uri()
        );
        let _ = write!(
            toml,
            "[storage]\ndb_path = '{}'\n",
            dir.path().join("taskboard.db").display()
        );
        toml.push_str(
            "[sync]\npoll_interval_secs = 3600\nbackoff_initial_secs = 1\nbackoff_max_secs = 5\n",
        );
        let _ = write!(
            toml,
            "[app]\nshutdown_timeout_secs = {shutdown_timeout_secs}\n"
        );
        std::fs::write(dir.path().join("taskboard.toml"), toml).expect("write config");
        Self {
            dir,
            password: Some("daemon-signals-pw"),
        }
    }

    fn config(&self) -> String {
        self.dir
            .path()
            .join("taskboard.toml")
            .to_string_lossy()
            .into_owned()
    }

    fn spawn(&self) -> Child {
        spawn_daemon(Path::new(&self.config()), self.password)
    }

    /// Polls `status --json` until a daemon serves this install
    /// (blocking: process-tier tests drive real children).
    fn wait_ready(&self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !self
            .run(&["--json", "status"])
            .1
            .contains("\"daemon\":true")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "daemon never started serving status"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn run(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let output = binary_command()
            .args(["--config", self.config().as_str()])
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("run one-shot");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

/// Signals the child and reaps it, returning the exit code. The reaper
/// polls `try_wait` under a deadline — a forced drain can take its
/// salvage budget, so the wait is bounded, not instant.
fn signal_and_wait(daemon: &mut Child, sig: Signal, what: &str) -> i32 {
    kill(Pid::from_raw(daemon.0.id().cast_signed()), sig).expect("signal the daemon");
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = daemon.0.try_wait().expect("poll daemon") {
            return status
                .code()
                .unwrap_or_else(|| panic!("{what}: killed by signal, no exit code"));
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{what}: the daemon hung past the signal"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Pacing only: standard signals coalesce while pending, so consecutive
/// ladder triggers need a gap to be counted as distinct events (the pty
/// Case-4 recipe). Not a wait-for-state sleep.
fn pacing() {
    std::thread::sleep(Duration::from_millis(500));
}

/// The boards endpoint: the parked cycle that holds the drain open.
/// Method-only match (the pty Case-4 shape): any GET the boot cycle
/// fires parks — the exact route is the client's business.
async fn mount_parked_pull(server: &wiremock::MockServer) -> std::sync::mpsc::Receiver<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(ParkThenDelay {
            notify: tx,
            delay: Duration::from_secs(3600),
        })
        .mount(server)
        .await;
    rx
}

/// Notifies once the daemon's boot-sync request arrived (the
/// deterministic "the cycle is parked now" gate), then stalls the
/// request — the drain's bounded join holds against it.
struct ParkThenDelay {
    notify: std::sync::mpsc::Sender<()>,
    delay: Duration,
}

impl wiremock::Respond for ParkThenDelay {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let _ = self.notify.send(());
        wiremock::ResponseTemplate::new(200)
            .set_body_string(common::boards_json())
            .set_delay(self.delay)
    }
}

/// T9 (contract row 0, signal-initiated): one SIGTERM at the serve
/// phase → the full graceful drain → exit 0.
#[test]
fn sigterm_once_exits_0() {
    let install = Install::standalone("t9");
    let mut daemon = install.spawn();
    install.wait_ready();

    let code = signal_and_wait(&mut daemon, Signal::SIGTERM, "single SIGTERM");
    assert_eq!(code, 0, "the graceful signal-initiated drain is clean");
}

/// T11 (rung 2): a second SIGTERM while the wedged drain is still
/// running → the bounded joins are skipped, the salvage fires, and the
/// process exits at the forcing signal's code (143).
#[test]
fn second_sigterm_during_a_wedged_drain_exits_143() {
    let (server, parked) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            let parked = mount_parked_pull(&server).await;
            (server, parked)
        });
    let install = Install::wedged_connector("t11", &server, 20);
    let mut daemon = install.spawn();
    install.wait_ready();
    parked
        .recv_timeout(Duration::from_secs(30))
        .expect("the boot sync parked in the delayed route");

    let first = signal_and_return_while_running(&mut daemon, Signal::SIGTERM);
    assert_eq!(first, None, "rung 1 starts the drain, not an exit");
    pacing();
    let code = signal_and_wait(&mut daemon, Signal::SIGTERM, "second SIGTERM");
    assert_eq!(code, 143, "rung 2 forces at 128+SIGTERM");
}

/// Signals the child *without* reaping; returns whether it already
/// exited (rung 1 must not exit — the drain holds).
fn signal_and_return_while_running(daemon: &mut Child, sig: Signal) -> Option<i32> {
    kill(Pid::from_raw(daemon.0.id().cast_signed()), sig).expect("signal the daemon");
    std::thread::sleep(Duration::from_millis(100));
    daemon
        .0
        .try_wait()
        .expect("poll daemon")
        .and_then(|status| status.code())
}

/// T12: the forcing kind names the code — a SIGINT forcing signal
/// during the wedged drain exits 130 (the interrupt stream is
/// installed on this surface; the kind→code mapping is the pure tier's).
#[test]
fn sigint_during_a_wedged_drain_exits_130() {
    let (server, parked) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            let parked = mount_parked_pull(&server).await;
            (server, parked)
        });
    let install = Install::wedged_connector("t12", &server, 20);
    let mut daemon = install.spawn();
    install.wait_ready();
    parked
        .recv_timeout(Duration::from_secs(30))
        .expect("the boot sync parked in the delayed route");

    let first = signal_and_return_while_running(&mut daemon, Signal::SIGTERM);
    assert_eq!(first, None, "rung 1 starts the drain, not an exit");
    pacing();
    let code = signal_and_wait(&mut daemon, Signal::SIGINT, "SIGINT forcing");
    assert_eq!(code, 130, "rung 2 forces at 128+SIGINT");
}

/// T13: the escape's existence — three paced SIGTERMs still exit 143,
/// never a hang (rung 3 cannot make the shutdown worse than forced;
/// the rung-2/rung-3 distinction is not code-observable).
#[test]
fn third_sigterm_during_a_wedged_drain_still_exits_143() {
    let (server, parked) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            let parked = mount_parked_pull(&server).await;
            (server, parked)
        });
    let install = Install::wedged_connector("t13", &server, 20);
    let mut daemon = install.spawn();
    install.wait_ready();
    parked
        .recv_timeout(Duration::from_secs(30))
        .expect("the boot sync parked in the delayed route");

    let first = signal_and_return_while_running(&mut daemon, Signal::SIGTERM);
    assert_eq!(first, None, "rung 1 starts the drain, not an exit");
    pacing();
    // The second signal forces (promptly — the salvage is fast on a
    // wedged-sync graph); the third is paced *at* the dying process:
    // if it is ever observed to make the shutdown worse than forced
    // (a hang, a different code), the escape is broken. The reaper's
    // deadline is the hang pin.
    let _ = kill(Pid::from_raw(daemon.0.id().cast_signed()), Signal::SIGTERM);
    pacing();
    let code = signal_and_wait(&mut daemon, Signal::SIGTERM, "third SIGTERM");
    assert_eq!(code, 143, "rung 3 forces at the same 128+SIGTERM code");
}

/// T14 (the F3 lie's tombstone): a degraded drain — the bounded
/// sync-actor join aborts because the boot cycle is parked — exits 1,
/// not 0, so supervision can see it.
#[test]
fn degraded_drain_exits_1() {
    let (server, parked) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            let parked = mount_parked_pull(&server).await;
            (server, parked)
        });
    let install = Install::wedged_connector("t14", &server, 1);
    let mut daemon = install.spawn();
    install.wait_ready();
    parked
        .recv_timeout(Duration::from_secs(30))
        .expect("the boot sync parked in the delayed route");

    let code = signal_and_wait(&mut daemon, Signal::SIGTERM, "degraded drain");
    assert_eq!(code, 1, "a drain that aborted a join is not clean");
}
