// SPDX-License-Identifier: AGPL-3.0-only
//! Interactive-tty login test: drives the *real* binary under a
//! pseudo-terminal (via `script`), with no flags — all three values
//! prompted. This is the regression harness for the interactive path
//! the seam fakes cannot cover: prompt labels reach the terminal, the
//! masked `*` feedback read completes on a real tty (it once waited
//! forever there), and the ceremony persists.
//!
//! Structural assertions only (exit codes, written files); prompt
//! wording is UI text and never asserted (AGENTS §5). Skipped when
//! `script` (util-linux) is unavailable.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod common;

use std::time::Duration;

use common::{binary_command, tempdir, write_test_config};

const BOARDS_PATH: &str = "/index.php/apps/deck/api/v1.0/boards";

/// The session prompt (kept in sync with `repl::PROMPT`; wording is
/// never asserted — only its *arrival* gates the scripted input).
const SESSION_PROMPT: &str = "taskboard> ";

/// The scripted session journey.
const SESSION_LINES: [&str; 4] = [
    "boards create \"Personal\"",
    "stacks add \"Todo\"",
    "tasks add --stack Todo \"first task\"",
    "exit",
];

// ---------------------------------------------------------------------------
// The pty harness progress-deadline seam (the pty flake pass, 2026-11-01).
//
// Every session's only pre-pass failure bound was the external `timeout`
// total-wall-clock constant: legitimate sessions under load exceeded it
// (issues #521/#524/#549) and the exit-124 kill landed in unbounded blocking
// reads as a wedged-session failure. The seam replaces the unbounded blocks
// with idle-progress waits: one reader thread per session pumps output
// chunks into a shared transcript and announces progress; every wait is
// `predicate || progress || idle-timeout`. A wedge is *silence*; a loaded
// machine is slowness with continued progress — the idle budget is
// load-immune for exactly the recorded failure class, and the marker/prompt
// strings stay synchronization gates, never asserted.
// ---------------------------------------------------------------------------

/// The idle-progress budget — derived, never tuned (AGENTS §5: never a bare
/// larger constant). Formula: `PTY_IDLE_BUDGET = max(30 s, 3 × G_max)` with
/// `G_max` the largest legitimate inter-chunk silence under the recorded
/// stress profile (16-core dev box, sampled load 37–52 via bounded shell
/// spinners plus a parallel scratch cargo build, 2026-11-01). The only
/// *designed* silent window is the pty flake pass's own 10 s delayed
/// wiremock route (the stalled-line leg); the validation legs' gap
/// telemetry (`TASKBOARD_PTY_TEST_GAP_LOG`) recorded no legitimate gap
/// beyond it, so the formula lands on the 30 s floor — 3× headroom over
/// `G_max` = 10 s. That is thin headroom by the plan's own definition and is
/// recorded as a finding in the stage-2 report; re-derivations follow this
/// formula with fresh telemetry, never a nudge.
const PTY_IDLE_BUDGET: Duration = Duration::from_secs(30);

/// The total-wall-clock backstop — the last-resort bound for a *runaway*
/// (progressing-but-never-terminating) session, not the wedge detector (the
/// idle budget is). Formula: `max(120 s, 3 × T_max)` with `T_max` the largest
/// legitimate per-test total under the recorded profiles — 62 s, observed
/// repeatedly on loaded CI runners (#521/#549) and as 32 s locally under
/// the stress profile — so the formula lands on 3 × 62 s. The pre-pass
/// constant was 60 s; this upward re-derivation is a documented consequence
/// of the seam (plan decision D4), not the fix: with waits no longer
/// unbounded, legitimate runs no longer exercise the backstop at all, while
/// a real runaway must still die and fail the `assert_ne!(code, Some(124))`
/// wedge pins with their meaning intact.
const PTY_SESSION_BACKSTOP: Duration = Duration::from_secs(186);

/// The reader thread's progress announcement.
enum PtyProgress {
    /// Output flowed; the waiter's deadline restarts.
    Bytes,
    /// The pty closed; the caller keeps its exact on-EOF behavior.
    Eof,
}

/// The outcome of a seam wait (see [`PtyOutput::wait_for`]).
enum WaitOutcome {
    /// The predicate held.
    Satisfied,
    /// The pty closed first.
    Eof,
    /// No output for [`PTY_IDLE_BUDGET`] — wedge or starvation.
    Idle,
}

/// Env-gated gap telemetry (`TASKBOARD_PTY_TEST_GAP_LOG=<path>`): the
/// reader thread appends one `<micros-since-spawn>\t<chunk-len>` line per
/// chunk — the data the budget derivation consumes. Diagnostic-only, off by
/// default, no behavior change.
struct GapLog(Option<std::fs::File>, Instant);

impl GapLog {
    fn open() -> Self {
        let file = std::env::var("TASKBOARD_PTY_TEST_GAP_LOG")
            .ok()
            .map(|path| std::fs::File::create(path).expect("open the pty gap log"));
        Self(file, Instant::now())
    }

    fn record(&mut self, len: usize) {
        if let Some(file) = &mut self.0 {
            use std::io::Write as _;
            let _ = writeln!(file, "{}\t{}", self.1.elapsed().as_micros(), len);
        }
    }
}

/// The session's captured output, owned by its reader thread: the shared
/// transcript (single pipe, single reader — byte order unchanged) plus the
/// progress channel every wait drains.
struct PtyOutput {
    seen: Arc<Mutex<Vec<u8>>>,
    progress_rx: std_mpsc::Receiver<PtyProgress>,
}

impl PtyOutput {
    /// Starts the per-session reader thread that owns the pipe.
    fn spawn(out_pipe: std::process::ChildStdout) -> Self {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (progress_tx, progress_rx) = std_mpsc::channel();
        let seen_for_thread = Arc::clone(&seen);
        std::thread::spawn(move || {
            let mut pipe = out_pipe;
            let mut gap_log = GapLog::open();
            loop {
                let mut chunk = [0u8; 4096];
                match std::io::Read::read(&mut pipe, &mut chunk) {
                    Ok(0) | Err(_) => {
                        let _ = progress_tx.send(PtyProgress::Eof);
                        break;
                    }
                    Ok(n) => {
                        seen_for_thread
                            .lock()
                            .expect("pty transcript lock")
                            .extend_from_slice(&chunk[..n]);
                        gap_log.record(n);
                        if progress_tx.send(PtyProgress::Bytes).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Self { seen, progress_rx }
    }

    fn transcript(&self) -> Vec<u8> {
        self.seen.lock().expect("pty transcript lock").clone()
    }

    fn transcript_lossy(&self) -> String {
        String::from_utf8_lossy(&self.seen.lock().expect("pty transcript lock")).into_owned()
    }

    /// The idle-progress wait: `Satisfied` once `predicate` holds on the
    /// transcript, `Eof` on pty close, `Idle` after [`PTY_IDLE_BUDGET`]
    /// of silence (no outcome the caller can assert on — the session
    /// owner aborts it typed).
    fn wait_for(&self, mut predicate: impl FnMut(&[u8]) -> bool) -> WaitOutcome {
        loop {
            {
                let guard = self.seen.lock().expect("pty transcript lock");
                if predicate(&guard) {
                    return WaitOutcome::Satisfied;
                }
            }
            match self.progress_rx.recv_timeout(PTY_IDLE_BUDGET) {
                Ok(PtyProgress::Bytes) => {}
                Ok(PtyProgress::Eof) | Err(RecvTimeoutError::Disconnected) => {
                    return WaitOutcome::Eof;
                }
                Err(RecvTimeoutError::Timeout) => return WaitOutcome::Idle,
            }
        }
    }
}

/// A spawned interactive session under `script(1)`: the inner shell
/// records its pid (`exec` keeps it — the pidfile names the session
/// process, the signal target) and pipes the session's streams back.
/// Output rides the progress-deadline seam (the module comment); stderr
/// is drained by a reader thread from spawn (a full stderr pipe would
/// stall the session). The caller drops `stdin`/drains via
/// [`finish_session`] to end the run.
struct PtySession {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    out: PtyOutput,
    stderr_thread: std::thread::JoinHandle<Vec<u8>>,
    pid_path: Option<std::path::PathBuf>,
}

impl PtySession {
    /// The session process's pid (read lazily: the inner shell writes
    /// it before the banner can appear, and every caller gates on
    /// session output first).
    fn pid(&self) -> u32 {
        std::fs::read_to_string(
            self.pid_path
                .as_ref()
                .expect("pidfile written by the inner shell"),
        )
        .expect("pidfile written by the inner shell")
        .trim()
        .parse()
        .expect("pidfile holds a pid")
    }

    fn transcript(&self) -> Vec<u8> {
        self.out.transcript()
    }

    fn transcript_lossy(&self) -> String {
        self.out.transcript_lossy()
    }

    /// The idle-progress wait for a transcript predicate: true once it
    /// holds, false on EOF (the caller keeps its exact on-EOF behavior).
    /// Silence for [`PTY_IDLE_BUDGET`] kills and reaps the session *before*
    /// the typed panic, so a failing assertion cannot leak it.
    fn wait_for(&mut self, predicate: impl FnMut(&[u8]) -> bool) -> bool {
        match self.out.wait_for(predicate) {
            WaitOutcome::Satisfied => true,
            WaitOutcome::Eof => false,
            WaitOutcome::Idle => {
                let transcript = self.out.transcript_lossy();
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!(
                    "no pty output for {PTY_IDLE_BUDGET:?} (progress budget): \
                     the session wedged or starved; transcript: {transcript}"
                );
            }
        }
    }
}

/// Spawns the one true pty wrapper: `timeout <backstop> script -qec <inner>
/// /dev/null` with all streams piped, the test's own `XDG_DATA_HOME`, and
/// the per-test env additions (`TERM=xterm` exactly where the fixture needs
/// it — the marker-gated pacing — per-test extras verbatim). The wrapper's
/// backstop is the runaway bound derived at [`PTY_SESSION_BACKSTOP`]; wedge
/// detection is the seam's idle budget, not this number.
fn spawn_pty_session(
    inner: &str,
    data_home: &std::path::Path,
    term_xterm: bool,
    extra: &[(&str, &str)],
) -> PtySession {
    let mut command = std::process::Command::new("timeout");
    command
        .arg(PTY_SESSION_BACKSTOP.as_secs().to_string())
        .arg("script")
        .arg("-qec")
        .arg(inner)
        .arg("/dev/null")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // History persistence must never touch the developer's real
        // data dir: every pty session's XDG data home is the test's own
        // tempdir (see `run_login_under_pty`).
        .env("XDG_DATA_HOME", data_home);
    // A supported TERM is part of the fixture where marker-gated pacing is
    // used: rustyline treats `dumb` (the CI/agent default) as an
    // unsupported terminal and falls back to plain line reads where Tab is
    // never a key and no readline marker is emitted.
    if term_xterm {
        command.env("TERM", "xterm");
    }
    for (key, value) in extra {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("spawn script(1) pty wrapper");
    let stdin = child.stdin.take().expect("stdin piped");
    let out = PtyOutput::spawn(child.stdout.take().expect("stdout piped"));
    let mut err_pipe = child.stderr.take().expect("stderr piped");
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err_pipe, &mut buf);
        buf
    });
    PtySession {
        child,
        stdin,
        out,
        stderr_thread,
        pid_path: None,
    }
}

/// Waits for EOF on the seam's progress channel (the drain), then reaps.
/// A silent wedge at drain time has no caller-side gate left to reach, so
/// the idle budget kills and reaps the session and the drain *returns*:
/// the post-drain status assertions keep their exact semantics, and the
/// forced death lands exactly where the external `timeout` kill used to
/// (a real wedge still fails every `assert_eq!(code, Some(0))` pin with
/// the transcript attached; the one-shot leg whose assertions tolerate the
/// killed outcome by design keeps that design).
fn conclude_session(
    mut child: std::process::Child,
    holding: Option<std::process::ChildStdin>,
    out: &PtyOutput,
    stderr_thread: std::thread::JoinHandle<Vec<u8>>,
) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    // The stdin handle is held until after the wait: `script` tears
    // the pty down when *its* stdin EOFs, which would kill the shell
    // (and the session) mid-run (see `feed_session_lines`' note).
    let _holding = holding;
    loop {
        match out.progress_rx.recv_timeout(PTY_IDLE_BUDGET) {
            Ok(PtyProgress::Bytes) => {}
            Ok(PtyProgress::Eof) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let seen = out.transcript();
    let stderr = stderr_thread.join().expect("stderr drained");
    let status = child.wait().expect("pty run completes");
    (status, seen, stderr)
}

/// Finishes the session: drains the seam, joins the stderr reader, returns
/// (status, transcript, stderr).
fn finish_session(session: PtySession) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    let PtySession {
        child,
        stdin,
        out,
        stderr_thread,
        ..
    } = session;
    conclude_session(child, Some(stdin), &out, stderr_thread)
}

/// Writes each line only after its prompt was echoed (a prompt on the
/// wire means raw mode is active and that readline is reading). Two
/// pty hazards make batch writes unusable: bytes parked in the input
/// queue while rustyline switches the terminal in or out of raw mode
/// are flushed by the termios change (the whole script or every line
/// after the first was silently lost), and `script` tears the pty
/// down when *its* stdin EOFs. CR is the Enter key in raw mode (LF is
/// not a key at all with ICRNL off); in canonical mode ICRNL maps CR
/// to NL, so CR works on both sides of the switch.
fn feed_session_lines(session: &mut PtySession, lines: &[&str]) {
    let prompt_count = |seen: &[u8]| -> usize {
        seen.windows(SESSION_PROMPT.len())
            .filter(|w| *w == SESSION_PROMPT.as_bytes())
            .count()
    };
    for (index, line) in lines.iter().enumerate() {
        let target = index + 1;
        session.wait_for(|seen| prompt_count(seen) >= target);
        assert!(
            prompt_count(&session.transcript()) >= target,
            "the session never reached prompt {target}: {}",
            session.transcript_lossy()
        );
        session
            .stdin
            .write_all(format!("{line}\r").as_bytes())
            .expect("write scripted session line");
    }
}

/// `script` wraps the binary in a pty; the seam bounds the hang
/// regression (a wedged prompt must fail the test, not stall it).
/// Returns (exit status, session transcript, stderr).
fn run_login_under_pty(
    config: &std::path::Path,
    url: &str,
    input: &str,
    data_home: &std::path::Path,
) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!(
        "{} --config {} login --server {} --user alice",
        binary,
        config.display(),
        url
    );
    let mut session = spawn_pty_session(&inner, data_home, false, &[]);
    session
        .stdin
        .write_all(input.as_bytes())
        .expect("write scripted input");
    finish_session(session)
}

#[test]
fn tty_login_prompts_all_three_values_and_persists() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    // The pty run below is blocking; mount the wiremock surface via a
    // tiny current-thread runtime and keep the server alive for the
    // whole test.
    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(BOARDS_PATH))
                .respond_with(
                    wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()),
                )
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // login is pre-core: the config only needs to decode (env store so
    // no keyring backend is touched in CI).
    write_test_config(&config, None, None).expect("write config");

    // Scripted interactive input: server URL, username, password —
    // exactly what a human types at the three prompts.
    let started = Instant::now();
    let (status, _seen, stderr) = run_login_under_pty(
        &config,
        &server.uri(),
        &format!("{}\nalice\ns3cret-tty-pw\n", server.uri()),
        dir.path(),
    );

    // The hang guard: a wedged prompt dies on the seam's idle budget (or
    // the runaway backstop), never silently.
    assert_ne!(
        status.code(),
        Some(124),
        "the interactive login hung (prompt never completed)"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "interactive login failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "interactive login must finish promptly"
    );

    // Persisted non-secrets, secret-free by construction.
    let written = std::fs::read_to_string(&config).expect("config written");
    assert!(written.contains(&server.uri()));
    assert!(written.contains("alice"));
    assert!(!written.contains("s3cret-tty-pw"));
}

/// Blocks until the marker string has appeared on the pty output. The
/// masked-secret prompt of the in-session ceremony (ADR 0014) gates the
/// scripted input on the secret prompt's *arrival* — the read runs on
/// the editor thread via `rpassword`, the only tier that exercises
/// rpassword outside the one-shot path. Wording is a synchronization
/// gate, never asserted.
fn wait_for_marker(session: &mut PtySession, marker: &str) {
    assert!(
        session.wait_for(|seen| seen.windows(marker.len()).any(|w| w == marker.as_bytes())),
        "the pty ended before {marker:?} appeared: {}",
        session.transcript_lossy()
    );
}

/// The in-session `connect` journey under a real pty (ADR 0014, D17):
/// a bare invocation opens the session, a `connect` line runs the
/// ceremony on the running session — server/user come from the line,
/// the secret rides the masked prompt on the editor thread — and the
/// config persists secret-free.
#[test]
fn tty_in_session_connect_prompts_masked_and_persists() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(BOARDS_PATH))
                .respond_with(
                    wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()),
                )
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut session = spawn_pty_session(&inner, dir.path(), false, &[]);
    feed_session_lines(
        &mut session,
        &[&format!("connect --server {} --user alice", server.uri())],
    );
    wait_for_marker(&mut session, "App password: ");
    session
        .stdin
        .write_all(b"tty-in-session-pw\r")
        .expect("write the masked secret");
    // The ceremony ends; the editor re-arms and the session can exit.
    feed_session_lines(&mut session, &["exit"]);
    let (status, seen, stderr) = finish_session(session);

    assert_ne!(
        status.code(),
        Some(124),
        "the in-session connect hung: {}",
        String::from_utf8_lossy(&seen)
    );
    assert_eq!(
        status.code(),
        Some(0),
        "in-session connect failed: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );

    // Persisted non-secrets, secret-free by construction.
    let written = std::fs::read_to_string(&config).expect("config written");
    assert!(written.contains(&server.uri()));
    assert!(written.contains("alice"));
    assert!(!written.contains("tty-in-session-pw"));
}

/// D7's pty-tier verification (ADR 0014): Ctrl-C during the masked
/// read. `rpassword` runs the tty with ISIG off, sees the ^C byte as
/// data, and re-raises SIGINT itself; which of the two observed
/// outcomes materializes is the *environment's* SIGINT disposition,
/// and the app is correct in both:
/// - default disposition (an interactive shell): the run dies
///   mid-ceremony — the house "Ctrl-C exits" precedent;
/// - ignored/inherited (CI runners): rpassword's error returns through
///   the oneshot, the ceremony fails typed (`prompt`) and the session
///   survives (ADR 0011's failed-line rule).
///
/// The invariant pinned in both branches: nothing persists (every
/// prompt precedes verify → persist → store) and the run terminates —
/// the ^C never wedges the session behind an unanswered read.
#[test]
fn tty_ctrl_c_during_the_masked_read_never_persists_and_never_wedges() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    // The 401 server makes any persisted identity impossible: the pin
    // is that the file is untouched, whatever the read answered.
    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(BOARDS_PATH))
                .respond_with(wiremock::ResponseTemplate::new(401))
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut session = spawn_pty_session(&inner, dir.path(), false, &[]);
    feed_session_lines(
        &mut session,
        &[&format!("connect --server {} --user alice", server.uri())],
    );
    wait_for_marker(&mut session, "App password: ");
    // The ^C byte in raw mode: rpassword aborts the read.
    session.stdin.write_all(b"\x03").expect("write the ^C byte");

    // Which outcome: EOF (the process died) or the session prompt
    // re-armed (the typed-cancellation path). Idle-progress waits only —
    // no sleeps; a wedged middle state flows no data, so the seam's idle
    // budget aborts it typed and the status pins name it.
    let prompt_count = |seen: &[u8]| -> usize {
        seen.windows(SESSION_PROMPT.len())
            .filter(|w| *w == SESSION_PROMPT.as_bytes())
            .count()
    };
    let survived = session.wait_for(|seen| prompt_count(seen) >= 2);

    if survived {
        // The typed-cancellation environment: the failed line is over,
        // the session continues (ADR 0011) — exit gracefully.
        feed_session_lines(&mut session, &["exit"]);
    }
    let (status, seen, _stderr) = finish_session(session);

    assert_ne!(
        status.code(),
        Some(124),
        "the ^C byte wedged the session: {}",
        String::from_utf8_lossy(&seen)
    );
    if !survived {
        assert_ne!(
            status.code(),
            Some(0),
            "the ^C byte completed the ceremony gracefully: {}",
            String::from_utf8_lossy(&seen)
        );
    }
    // Death/cancellation is pre-persist by construction.
    let written = std::fs::read_to_string(&config).expect("config untouched");
    assert!(!written.contains(&server.uri()), "nothing persisted");
    assert!(!written.contains("alice"));
}

/// `script` wraps the bare binary in a pty: stdin is a TTY, so the
/// no-argument invocation opens the interactive REPL (ADR 0011). The
/// scripted lines are one session: create the board, add a stack and a
/// task, leave via `exit`.
#[test]
fn tty_bare_invocation_opens_a_repl_session_and_persists() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // Standalone shape: no server, no user, no credentials — the
    // session must boot and work with none of them.
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut session = spawn_pty_session(&inner, dir.path(), false, &[]);
    // Feed the script one line at a time, each only after its prompt
    // was echoed (a prompt on the wire means raw mode is active and
    // that readline is reading). Two pty hazards make batch writes
    // unusable: bytes parked in the input queue while rustyline
    // switches the terminal in or out of raw mode are flushed by the
    // termios change (the whole script or every line after the first
    // was silently lost), and `script` tears the pty down when *its*
    // stdin EOFs. CR is the Enter key in raw mode (LF is not a key at
    // all with ICRNL off); in canonical mode ICRNL maps CR to NL, so
    // CR works on both sides of the switch.
    feed_session_lines(&mut session, &SESSION_LINES);
    // The stdin handle is held by `finish_session` until after the
    // wait: `script` tears the pty down when *its* stdin EOFs, and a
    // session still booting would lose every queued line after the
    // first. The session ends itself on `exit`, so nothing hangs.
    let (status, seen, stderr) = finish_session(session);

    // The hang guard: `timeout 60` exits 124 when the session wedges.
    assert_ne!(
        status.code(),
        Some(124),
        "the REPL session hung (exit never completed)"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "REPL session failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(db.exists(), "the session's core created its database");

    // Durability beyond the session: a one-shot (no TTY) hydrates the
    // session's data and exits 0 — the session released the lock.
    let mut follow_up = binary_command();
    follow_up.args([
        "--config",
        config.to_str().expect("utf8 path"),
        "tasks",
        "list",
        "--json",
    ]);
    let outcome = follow_up.output().expect("one-shot runs");
    assert_eq!(
        outcome.status.code(),
        Some(0),
        "one-shot after the session: {}; session stdout: {}; session stderr: {}",
        String::from_utf8_lossy(&outcome.stderr),
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    let list: serde_json::Value =
        serde_json::from_slice(&outcome.stdout).expect("one list payload");
    assert_eq!(
        list["stacks"][0]["title"],
        serde_json::json!("Todo"),
        "session stdout: {}; session stderr: {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(
        list["stacks"][0]["tasks"][0]["title"],
        serde_json::json!("first task")
    );
}

/// Regression (the tmux no-echo breakage): a session left via `exit`
/// hands the terminal line discipline back exactly as it found it. The
/// editor thread loops straight back into `readline()` (raw mode) after
/// handing a line over; a shutdown that exits the process mid-readline
/// skips the editor's own restore, and the leftover raw mode (IEXTEN
/// off among others) leaves the pane's shell reading input without
/// displaying it. The guard in `run_session` must make the hand-back
/// byte-exact: after the session, the pty is canonical again (echo,
/// isig, iexten on) — the state the shell's own line editor expects.
///
/// Observed through an inner `sh` (a non-interactive shell does not
/// touch the termios), so `stty -a` right after the session reports the
/// exact state the session handed back.
#[test]
fn tty_repl_exit_hands_the_line_discipline_back_intact() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // Standalone shape: the session needs no connector (ADR 0010).
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    // The non-interactive `sh` between `script` and the binary leaves
    // the termios alone, so this snapshot is the session's hand-back.
    let stty_after = dir.path().join("line-discipline-after.txt");
    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!(
        "{} --config {}; stty -a > {}",
        binary,
        config.display(),
        stty_after.display()
    );
    let mut session = spawn_pty_session(&inner, dir.path(), false, &[]);

    // Gate the `exit` line on the first prompt: bytes parked in the
    // input queue while rustyline enters raw mode are flushed (the
    // pty hazard documented on `feed_session_lines`).
    if !session.wait_for(|seen| {
        seen.windows(SESSION_PROMPT.len())
            .any(|w| w == SESSION_PROMPT.as_bytes())
    }) {
        panic!("the session's pty closed before the first prompt");
    }
    session
        .stdin
        .write_all(b"exit\r")
        .expect("write the exit line");

    let (child, stdin, out, stderr_thread) = into_parts(session);
    drop(stdin);
    let (status, _seen, stderr) = conclude_session(child, None, &out, stderr_thread);
    assert_ne!(
        status.code(),
        Some(124),
        "the REPL session hung (exit never completed)"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "REPL session failed: {}",
        String::from_utf8_lossy(&stderr)
    );

    // The hand-back: every line-discipline mode the shell's line editor
    // relies on is enabled again (the pty's pre-session defaults).
    let stty = std::fs::read_to_string(&stty_after).expect("stty snapshot written");
    let tokens: Vec<&str> = stty.split_whitespace().collect();
    for flag in ["echo", "icanon", "isig", "iexten"] {
        assert!(
            tokens.contains(&flag),
            "line discipline handed back with {flag} disabled: {stty}"
        );
    }
}

/// The instance-lock mutex runs both ways (ADR 0011): with the daemon
/// holding the database, the pty REPL boot fails typed and exits 1;
/// after the daemon stops, the database is servable again.
#[test]
fn tty_repl_refuses_while_the_daemon_holds_the_database() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    // The daemon needs a connector (the standalone gate) but never a
    // reachable server: an unreachable URL idles it, cycles failing.
    write_test_config(&config, Some("http://127.0.0.1:1"), Some("alice")).expect("write config");
    let lock_sidecar = dir.path().join("taskboard.lock");

    let mut daemon = binary_command();
    daemon
        .args(["--config", config.to_str().expect("utf8 path"), "daemon"])
        .env("TASKBOARD_APP_PASSWORD", "tty-lock-secret")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(
            std::fs::File::create(dir.path().join("daemon.err")).expect("daemon err file"),
        ));
    let mut daemon = daemon.spawn().expect("spawn the daemon");
    // If the daemon exits early, surface its stderr in the panic.
    let early = daemon.try_wait().expect("probe daemon");
    assert!(early.is_none(), "daemon exited at boot; see daemon.err");

    // Deterministic-enough cross-process wait: the daemon holds the
    // lock the moment its sidecar exists. A deadline keeps a wedged
    // daemon a test failure, not a stall.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !lock_sidecar.exists() {
        assert!(
            Instant::now() < deadline,
            "the daemon never acquired its instance lock"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut session = spawn_pty_session(&inner, dir.path(), false, &[]);
    // The session refuses at boot and never reads a line; a dead pty
    // stdin is not a test failure by itself.
    let _ = session.stdin.write_all(b"exit\n");
    let (status, _seen, _stderr) = finish_session(session);
    assert_ne!(
        status.code(),
        Some(124),
        "the REPL boot hung instead of failing on the held lock"
    );
    assert_eq!(
        status.code(),
        Some(1),
        "the REPL must refuse while the daemon holds the lock"
    );

    // Release: the daemon dies, its lock goes with it, and a one-shot
    // core boots on the same database again.
    daemon.kill().expect("kill the daemon");
    daemon.wait().expect("reap the daemon");
    let mut follow_up = binary_command();
    follow_up
        .args([
            "--config",
            config.to_str().expect("utf8 path"),
            "tasks",
            "list",
            "--json",
        ])
        .env("TASKBOARD_APP_PASSWORD", "tty-lock-secret");
    let outcome = follow_up.output().expect("one-shot runs");
    assert_eq!(
        outcome.status.code(),
        Some(0),
        "one-shot after the daemon stopped: {}",
        String::from_utf8_lossy(&outcome.stderr)
    );
}

/// Regression (review 2026-10-11): `taskboard daemon` under a pty must
/// run the daemon, not the TTY-sensitive no-argument entry — the
/// explicit subcommand wins over the REPL in every environment. Since
/// WI-5 the daemon is backend-agnostic (ADR 0010 amendment), so the
/// observable outcome under a standalone config is a *serving* daemon:
/// it binds the per-database IPC socket (the REPL boots no socket) and
/// answers `daemon stop` with a graceful exit 0.
#[test]
fn tty_explicit_daemon_stays_the_daemon_and_serves_standalone() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[sync]\nbackend = 'standalone'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    std::fs::write(&config, toml).expect("write config");
    let socket = dir.path().join("taskboard.sock");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {} daemon", binary, config.display());
    let session = spawn_pty_session(&inner, dir.path(), false, &[]);
    let (child, stdin, out, stderr_thread) = into_parts(session);
    // The daemon never reads stdin; dropping it EOFs `script`'s stdin
    // exactly as the pre-seam fixture did.
    drop(stdin);

    // The daemon arm boots and serves (the REPL would sit at a prompt
    // and never bind the socket); then the remote stop shuts it down.
    let ready = Instant::now() + Duration::from_secs(30);
    while !socket.exists() {
        assert!(
            Instant::now() < ready,
            "the daemon never bound the ipc socket"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let stop = std::process::Command::new(binary)
        .args([
            "--config",
            config.to_str().expect("utf8 path"),
            "daemon",
            "stop",
        ])
        .output()
        .expect("daemon stop runs");
    assert_eq!(
        stop.status.code(),
        Some(0),
        "daemon stop gracefully exits the serving daemon: {}",
        String::from_utf8_lossy(&stop.stderr)
    );
    let (status, _seen, stderr) = conclude_session(child, None, &out, stderr_thread);
    assert_ne!(
        status.code(),
        Some(124),
        "the daemon hung instead of stopping gracefully"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the daemon exits 0 on the remote stop, stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
}

/// Tab completion through the real editor (WI-G): a Tab character in
/// the scripted input drives rustyline's completion. Two structural
/// proofs — a verb expansion and a live stack-title expansion — both
/// ride on the same invariant: the JSON payload only appears if the
/// Tab produced a *parseable* line; an unexpanded stem (`task`,
/// `--stack=To`) is a usage error / typed `unknown-stack` failure with
/// no such payload. Skipped when `script` is unavailable.
#[test]
#[allow(clippy::too_many_lines)] // the pacing table, one row per scripted line
fn tty_repl_completion_expands_verbs_and_live_titles() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());
    let mut session = spawn_pty_session(&inner, dir.path(), true, &[]);
    // Readline-cycle pacing: each scripted line is written only once
    // the editor's Nth readline() is provably reading (see
    // `wait_for_readline`), so no feed can land in a termios switch —
    // the queue-flush wedge — however the session's own output
    // interleaves under load. `\t` is the Tab key: the fourth line
    // proves verb completion (`task` + Tab → `tasks`), the fifth
    // proves live stack-title completion through the attached
    // `--stack=` form.
    feed_line(&mut session, 1, "boards create \"Personal\"");
    feed_line(&mut session, 2, "stacks add \"Todo\"");
    feed_line(&mut session, 3, "tasks add --stack Todo \"first task\"");
    feed_line(&mut session, 4, "task\t list --json");
    feed_line(&mut session, 5, "tasks list --stack=To\t --json");
    feed_line(&mut session, 6, "exit");
    let (status, seen, stderr) = finish_session(session);

    // The hang guard: `timeout 60` exits 124 when the session wedges.
    assert_ne!(
        status.code(),
        Some(124),
        "the REPL session hung (exit never completed)"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "REPL session failed: {}; stdout: {}",
        String::from_utf8_lossy(&stderr),
        String::from_utf8_lossy(&seen)
    );

    // Both completed lines ran as real commands: exactly two JSON list
    // payloads in the stream (the plain `--json` lines; the setup lines
    // are human and skipped). Each payload is the full board listing —
    // the verb-expanded line because only `tasks list` produces that
    // shape, the title-expanded line because an unexpanded `To` would
    // be an unknown-stack *error*, not a payload.
    let payloads = json_list_payloads(&seen);
    assert_eq!(
        payloads.len(),
        2,
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    for payload in &payloads {
        assert_eq!(payload["stacks"][0]["title"], serde_json::json!("Todo"));
        assert_eq!(
            payload["stacks"][0]["tasks"][0]["title"],
            serde_json::json!("first task")
        );
    }
    // No errors at all: both expansions produced parseable lines. The
    // merged pty stream is the place to look — `script(1)` attaches the
    // inner process's stderr to the pty, so every session write (JSON
    // payloads and JSON errors alike) lands in `seen` and the child's
    // own stderr pipe stays empty by construction.
    assert!(
        json_error_lines(&seen).is_empty(),
        "session stream: {}",
        String::from_utf8_lossy(&seen)
    );
}

/// The JSON list payloads of a captured session stream. The session's
/// writes interleave with the line reader's repaints, so a payload can
/// share its stream line with a prompt or a typed echo — extraction
/// keys on the payload's own leading shape and takes the rest of the
/// stream line, which nothing can follow: the payload write ends the
/// line.
fn json_list_payloads(buf: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(buf)
        .lines()
        .filter_map(|line| {
            let start = line.find("{\"stacks\":")?;
            serde_json::from_str(&line[start..]).ok()
        })
        .collect()
}
/// The JSON error objects of a captured session stream (an error line
/// parses as an object with an `error` key; human text never does) —
/// the same repaint-interleaving rule as [`json_list_payloads`]. The
/// merged stream is the place to look: `script(1)` attaches the inner
/// process's stderr to the pty, so every session write (JSON payloads
/// and JSON errors alike) lands in the captured bytes and the child's
/// own stderr pipe stays empty by construction.
fn json_error_lines(buf: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(buf)
        .lines()
        .filter_map(|line| {
            let start = line.find("{\"error\"")?;
            serde_json::from_str::<serde_json::Value>(&line[start..]).ok()
        })
        .filter(|value| value.get("error").is_some())
        .collect()
}

/// rustyline's readline-entry marker: every `readline()` opens with the
/// bracketed-paste enable sequence and closes with its disable, and
/// *nothing else* emits the opening bytes — refreshes (a Tab
/// completion's full repaint emits the synchronized-update marker
/// instead) and the session's own stdout/stderr/tracing writes never
/// do. Counting them is therefore exact readline-cycle counting:
/// marker N on the wire proves the editor enabled raw mode for line N
/// and is blocked reading it, so a line fed then can never land in a
/// termios switch (the queue-flush wedge), however the session's
/// receipts and payloads interleave around the prompts under load.
const READLINE_MARKER: &[u8] = b"\x1b[?2004h";

/// Drains the session pipe until the Nth readline-entry marker
/// appeared (the pacing gate). A reader that hits EOF before the
/// marker appears fails the test via the assertion, never hangs
/// (`timeout` guards the wedge case).
fn wait_for_readline(session: &mut PtySession, n: usize) {
    session.wait_for(|seen| readline_marker_count(seen) >= n);
    assert!(
        readline_marker_count(&session.transcript()) >= n,
        "the session never reached readline {n}: {}",
        session.transcript_lossy()
    );
}

/// The exact readline-cycle count of a captured stream (see
/// [`READLINE_MARKER`]).
fn readline_marker_count(haystack: &[u8]) -> usize {
    if haystack.len() < READLINE_MARKER.len() {
        return 0;
    }
    haystack
        .windows(READLINE_MARKER.len())
        .filter(|w| *w == READLINE_MARKER)
        .count()
}

/// Blocks until the session prompt appeared on the pty (the first-feed
/// gate: raw mode is provably active and `readline` is reading — see
/// the hazard note on `feed_session_lines`). Wording is a
/// synchronization gate, never asserted.
fn wait_for_prompt(session: &mut PtySession) {
    assert!(
        session.wait_for(|seen| {
            seen.windows(SESSION_PROMPT.len())
                .any(|w| w == SESSION_PROMPT.as_bytes())
        }),
        "the pty ended before the session prompt: {}",
        session.transcript_lossy()
    );
}

/// Waits for the Nth readline cycle, then writes one scripted line
/// (CR is the Enter key in raw mode — see `feed_session_lines`).
fn feed_line(session: &mut PtySession, n: usize, line: &str) {
    wait_for_readline(session, n);
    session
        .stdin
        .write_all(format!("{line}\r").as_bytes())
        .expect("write scripted session line");
}

/// `script` wraps the binary in a pty for the connect confirm (the one
/// thing hermetic fakes cannot cover: the real `is_terminal` y/N read).
/// A standalone-seeded config, no flags: the D2 guard must confirm on
/// the terminal; the scripted `y` after the confirm prompt arrives
/// flips the backend (plan §7.5 pty leg).
#[test]
fn tty_connect_confirms_the_backend_switch_and_persists() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        // Skip, never fail: util-linux `script` unavailable here.
        return;
    }

    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(BOARDS_PATH))
                .respond_with(
                    wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()),
                )
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, None, None).expect("write config");
    // Seed the explicit standalone backend the guard fires on (the
    // config only needs to decode; the env store keeps CI keyring-free).
    let seeded = std::fs::read_to_string(&config).expect("read");
    std::fs::write(
        &config,
        seeded.replacen("[sync]", "[sync]\nbackend = 'standalone'", 1),
    )
    .expect("seed standalone backend");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!(
        "{} --config {} connect --server {} --user alice",
        binary,
        config.display(),
        server.uri()
    );
    let mut session = spawn_pty_session(&inner, dir.path(), false, &[]);
    // The flags supply server/user, and the D2 guard runs before the
    // identity gather (D5): the tty reads are exactly the y/N confirm
    // and then the masked secret prompt. The confirm's wording is
    // never asserted — the flip persisting is the proof the consent
    // was read on the real terminal.
    session
        .stdin
        .write_all(b"y\ns3cret-tty-pw\n")
        .expect("write scripted input");
    let (status, _seen, stderr) = finish_session(session);

    // The hang guard: a wedged confirm dies on the seam's idle budget
    // (or the runaway backstop), never silently.
    assert_ne!(
        status.code(),
        Some(124),
        "the interactive connect hung (confirm never completed)"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "interactive connect failed: {}",
        String::from_utf8_lossy(&stderr)
    );

    // The flipped word persisted (parsed structurally — toml_edit
    // quotes it its own way); the secret did not.
    let written = std::fs::read_to_string(&config).expect("config written");
    let doc = written.parse::<toml_edit::DocumentMut>().expect("parses");
    assert_eq!(
        doc["sync"]["backend"].as_str(),
        Some("nextcloud"),
        "the flipped word persisted: {written}"
    );
    assert!(written.contains(&server.uri()));
    assert!(!written.contains("s3cret-tty-pw"));
}

/// File-backed history across sessions: session 1 records a submitted
/// line into the data dir's history file and leaves; session 2 (a fresh
/// process, same data home) recalls it with Up-arrow alone — the
/// recalled line must execute and produce the same structural JSON
/// payload session 1 produced (payload-parse assertion, never output
/// text). Proves the load→recall→execute wiring the hermetic unit
/// suite cannot reach. Skipped when `script` is unavailable.
#[test]
#[allow(clippy::too_many_lines)] // two full pty sessions, one journey
fn tty_repl_history_recalls_the_previous_sessions_line() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!("{} --config {}", binary, config.display());

    // Session 1: one recorded line, then a clean exit.
    let mut session = spawn_pty_session(&inner, dir.path(), true, &[]);
    // The first feed gates on the *prompt* (raw mode provably active
    // and reading), not on the readline marker: the marker is printed
    // before the raw-mode switch, and bytes parked in the input queue
    // through that switch are flushed (see `feed_session_lines`).
    wait_for_prompt(&mut session);
    session
        .stdin
        .write_all(b"tasks list --json\r")
        .expect("write the recorded line");
    feed_line(&mut session, 2, "exit");
    let (status, seen, _stderr) = finish_session(session);
    assert_eq!(
        status.code(),
        Some(0),
        "session 1 failed: {}",
        String::from_utf8_lossy(&seen)
    );
    assert_eq!(
        json_list_payloads(&seen).len(),
        1,
        "session 1 produced its payload: {}",
        String::from_utf8_lossy(&seen)
    );

    // Session 2: a fresh process over the same data home sends only the
    // Up-arrow key and Enter — no typed line. The recalled line is the
    // proof that the history file loaded.
    let mut session = spawn_pty_session(&inner, dir.path(), true, &[]);
    // Same first-feed gating as session 1: the prompt, not the marker.
    // Two Up-arrows: session 1's LAST recorded line is its `exit`; the
    // recall targets the line under test, one entry above it.
    wait_for_prompt(&mut session);
    session
        .stdin
        .write_all(b"\x1b[A\x1b[A\r")
        .expect("write the Up-arrow recall");
    // The recalled line runs; the only other input is the exit line
    // (pacing: the editor's second readline proves the first completed).
    feed_line(&mut session, 2, "exit");
    let (status, seen, _stderr) = finish_session(session);
    assert_ne!(
        status.code(),
        Some(124),
        "session 2 hung on the recalled line: {}",
        String::from_utf8_lossy(&seen)
    );

    let payloads = json_list_payloads(&seen);
    assert_eq!(
        payloads.len(),
        1,
        "the Up-arrow recalled exactly the recorded line: {}",
        String::from_utf8_lossy(&seen)
    );
    assert!(
        payloads[0].get("stacks").is_some(),
        "the recalled line produced the list payload shape: {}",
        String::from_utf8_lossy(&seen)
    );
    assert!(
        json_error_lines(&seen).is_empty(),
        "the recalled line executed cleanly: {}",
        String::from_utf8_lossy(&seen)
    );
}

// ---------------------------------------------------------------------------
// Signal & prompt-grammar pins (DR-2/R): the only tier where real
// signals occur. Handlers live in the production session only — the
// hermetic suites stay hermetic (design D-7) — so these cases prove
// what the injected watches cannot: real SIGINT/SIGTERM delivery,
// restore-before-exit ordering (the stty probe), and the force codes.
// ---------------------------------------------------------------------------

use std::sync::mpsc as std_mpsc;

/// A wiremock responder that notifies a channel when the request
/// arrives, then delays — the deterministic gate for "the dispatch is
/// in flight" (blocking recv, no sleeps) before a real signal is
/// delivered.
struct NotifyThenDelay {
    notify: std_mpsc::Sender<()>,
    delay: Duration,
}

impl wiremock::Respond for NotifyThenDelay {
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let _ = self.notify.send(());
        wiremock::ResponseTemplate::new(200)
            .set_body_string(common::boards_json())
            .set_delay(self.delay)
    }
}

/// Spawns the session pty with the `exec` inner (clean signal target,
/// clean exit code: the pty's foreground group is the session process
/// alone): the inner shell records its pid (`exec` keeps it — the
/// pidfile names the session process, the signal target). The session
/// type and its progress-deadline seam live in the module's seam
/// section. The env carries the test's own `XDG_DATA_HOME`; `extra`
/// adds per-test variables.
fn spawn_session_pty_exec(
    config: &std::path::Path,
    data_home: &std::path::Path,
    extra: &[(&str, &str)],
) -> PtySession {
    let dir = config
        .parent()
        .expect("config in its tempdir")
        .to_path_buf();
    let pid_path = dir.join("session.pid");
    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!(
        "sh -c 'echo $$ > {}; exec {} --config {}'",
        pid_path.display(),
        binary,
        config.display()
    );
    let mut session = spawn_pty_session(&inner, data_home, true, extra);
    session.pid_path = Some(pid_path);
    session
}

/// Drains the session pipe until the Nth readline-entry marker appeared
/// (the raw-mode-is-active gate for the next scripted write).
fn wait_readline(session: &mut PtySession, n: usize) {
    assert!(
        session.wait_for(|seen| readline_marker_count(seen) >= n),
        "the pty ended before readline {n}: {}",
        session.transcript_lossy()
    );
}

/// Waits for the Nth readline cycle, then writes one line (CR is the
/// Enter key in raw mode).
fn feed_at(session: &mut PtySession, n: usize, line: &str) {
    wait_readline(session, n);
    session
        .stdin
        .write_all(format!("{line}\r").as_bytes())
        .expect("write scripted session line");
}

/// Takes the child, its stdin, and the seam's output half apart (for the
/// tests that must drop `stdin` or signal before draining).
fn into_parts(
    session: PtySession,
) -> (
    std::process::Child,
    std::process::ChildStdin,
    PtyOutput,
    std::thread::JoinHandle<Vec<u8>>,
) {
    let PtySession {
        child,
        stdin,
        out,
        stderr_thread,
        ..
    } = session;
    (child, stdin, out, stderr_thread)
}

/// The session process's exit status, as recorded by the inner shell
/// (`script`/`timeout`'s own relay proved unreliable here — the inner
/// wait is the structural record).
fn session_exit_status(rc_path: &std::path::Path) -> i32 {
    std::fs::read_to_string(rc_path)
        .expect("rc file written by the inner shell")
        .lines()
        .next()
        .expect("the session's exit status recorded")
        .trim()
        .parse()
        .expect("rc file holds a number")
}

/// Sends a real signal to the session process.
fn signal_session(session: &PtySession, sig: &str) {
    let pid = session.pid().to_string();
    let status = std::process::Command::new("kill")
        .arg(sig)
        .arg(&pid)
        .status()
        .expect("kill runs");
    assert!(status.success(), "kill {sig} {pid} failed");
}

// `finish_session` and the stderr reader thread live in the seam
// section: the constructor spawns the stderr reader at spawn time (a
// full stderr pipe would stall the session), and `finish_session`
// concludes through `conclude_session`.

/// Case 1 (rewritten): handler world makes the masked-read ^C
/// deterministic — rpassword's re-raised SIGINT is consumed by the
/// installed handler and surfaces as its typed `Err` (or arms the
/// cancel epoch mid-dispatch; both are typed cancellations, plan
/// §11 Q11). The session survives, reports one typed error envelope,
/// and nothing persists.
#[test]
fn tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    // The 401 server makes any persisted identity impossible: the pin
    // is that the file is untouched, whatever the read answered.
    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(wiremock::ResponseTemplate::new(401))
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let mut session = spawn_session_pty_exec(&config, dir.path(), &[]);
    feed_at(
        &mut session,
        1,
        &format!("connect --server {} --user alice --json", server.uri()),
    );
    wait_for_marker(&mut session, "App password: ");
    // The ^C byte in rpassword's raw mode: the typed cancellation.
    session.stdin.write_all(b"\x03").expect("write the ^C byte");

    // The editor re-arms (readline 2) — the session survived the ^C.
    feed_at(&mut session, 2, "exit");
    let (status, seen, stderr) = finish_session(session);

    assert_ne!(
        status.code(),
        Some(124),
        "the ^C byte wedged the session: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the session did not survive the masked-read ^C: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    // Exactly one typed error envelope: `prompt` (rpassword's own
    // `Err(Interrupted)`) or `cancelled` (the select-cancel race for
    // the same keystroke) — both correct, §11 Q11.
    let envelopes = json_error_lines(&seen)
        .into_iter()
        .chain(json_error_lines(&stderr));
    let kinds: Vec<String> = envelopes
        .filter_map(|value| value["error"]["kind"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(kinds.len(), 1, "one typed failure: {kinds:?} / {stderr:?}");
    assert!(
        kinds[0] == "prompt" || kinds[0] == "cancelled",
        "a typed cancellation, not anything else: {kinds:?}"
    );
    // Pre-persist by construction.
    let written = std::fs::read_to_string(&config).expect("config untouched");
    assert!(!written.contains(&server.uri()), "nothing persisted");
    assert!(!written.contains("alice"));
}

/// Case 2: ^C during a stalled line cancels it — the typed `cancelled`
/// envelope reports, the session continues, and the next line executes.
/// The stall is a delayed wiremock boards route; the notify-responder
/// gates the ^C on "the dispatch is in flight" (by then the editor's
/// raw mode is long gone, so the ^C byte is a real SIGINT).
#[test]
fn tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let (server, in_flight) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            let (tx, rx) = std_mpsc::channel();
            // Any request proves the sync actor is pulling.
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(NotifyThenDelay {
                    notify: tx,
                    delay: Duration::from_secs(10),
                })
                .mount(&server)
                .await;
            (server, rx)
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(
        toml,
        "server_url = '{}'\nusername = 'alice'\n",
        server.uri()
    );
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str(
        "[sync]\npoll_interval_secs = 3600\nbackoff_initial_secs = 1\nbackoff_max_secs = 5\n",
    );
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let mut session = spawn_session_pty_exec(
        &config,
        dir.path(),
        &[("TASKBOARD_APP_PASSWORD", "tty-stall-pw")],
    );
    feed_at(&mut session, 1, "boards list --json");
    // The in-flight gate: the wiremock notified on the request's
    // arrival — the dispatch is parked in the delayed HTTP await, the
    // editor's raw mode is long gone, and the ^C byte below is a real
    // SIGINT where the environment delivers one. Budgeted recv, no
    // sleeps.
    in_flight
        .recv_timeout(PTY_IDLE_BUDGET)
        .expect("the stalled request arrived");
    session.stdin.write_all(b"\x03").expect("write the ^C byte");
    // The editor re-arms either way: instantly when the SIGINT
    // cancelled the line, or when the 10 s delay expires and the line
    // completes (an environment that ignores SIGINT — the inherited
    // dispositions of rust test harnesses vary). Bounded either way.
    feed_at(&mut session, 2, "boards create B --json");
    feed_at(&mut session, 3, "exit");
    let (status, seen, stderr) = finish_session(session);

    assert_ne!(
        status.code(),
        Some(124),
        "the ^C wedged the stalled line: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the session did not survive the stalled line: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    // The next line executed after the stall resolved, and the only
    // error the session may report is the typed cancellation.
    let created: Vec<serde_json::Value> = String::from_utf8_lossy(&seen)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["title"] == "B")
        .collect();
    assert_eq!(
        created.len(),
        1,
        "the line after the stalled one executed: {}",
        String::from_utf8_lossy(&seen)
    );
    let kinds: Vec<String> = json_error_lines(&seen)
        .into_iter()
        .chain(json_error_lines(&stderr))
        .filter_map(|value| value["error"]["kind"].as_str().map(str::to_owned))
        .collect();
    assert!(
        kinds.iter().all(|kind| kind == "cancelled"),
        "the only permitted failure is the typed cancellation: {kinds:?} / {stderr:?}"
    );
}

/// Spawns the session pty with the probe inner: the binary runs in the
/// background (its pid — the signal target — lands in the pidfile), the
/// inner shell waits for it and then snapshots the line discipline the
/// session handed back (`stty -a`), the `tty_repl_exit_hands...`
/// pattern plus a signal target.
fn spawn_session_pty_probe(
    config: &std::path::Path,
    extra: &[(&str, &str)],
) -> (PtySession, std::path::PathBuf) {
    let dir = config
        .parent()
        .expect("config in its tempdir")
        .to_path_buf();
    let pid_path = dir.join("session.pid");
    let stty_path = dir.join("line-discipline-after.txt");
    let binary = env!("CARGO_BIN_EXE_taskboard");
    let rc_path = dir.join("rc.txt");
    let inner = format!(
        "sh -c 'echo $$ > {}; exec {} --config {}'; echo $? > {}; stty -a > {}; echo $? >> {}",
        pid_path.display(),
        binary,
        config.display(),
        rc_path.display(),
        stty_path.display(),
        rc_path.display()
    );
    let mut session = spawn_pty_session(&inner, &dir, true, extra);
    session.pid_path = Some(pid_path);
    (session, stty_path)
}

/// The hand-back probe: every line-discipline mode the shell's line
/// editor relies on is enabled in the `stty -a` snapshot.
fn assert_discipline_handed_back(stty_path: &std::path::Path) {
    let stty = std::fs::read_to_string(stty_path).expect("stty snapshot written");
    let tokens: Vec<&str> = stty.split_whitespace().collect();
    for flag in ["echo", "icanon", "isig", "iexten"] {
        assert!(
            tokens.contains(&flag),
            "line discipline handed back with {flag} disabled: {stty}"
        );
    }
}

/// Case 3: SIGTERM mid-readline restores the terminal FIRST, then ends
/// the session gracefully — exit 0 and a sane pane (D-4's ordering,
/// proven by the chained stty probe).
#[test]
fn tty_sigterm_mid_readline_restores_the_pane_and_exits_gracefully() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let (mut session, stty_path) = spawn_session_pty_probe(&config, &[]);
    // At the prompt (raw mode mid-readline): the signal target is the
    // session process.
    if !session.wait_for(|seen| {
        seen.windows(SESSION_PROMPT.len())
            .any(|w| w == SESSION_PROMPT.as_bytes())
    }) {
        panic!(
            "the pty ended before the prompt: {}",
            session.transcript_lossy()
        );
    }
    signal_session(&session, "-TERM");
    let (child, stdin, out, stderr_thread) = into_parts(session);
    drop(stdin);
    let (status, seen, _stderr) = conclude_session(child, None, &out, stderr_thread);
    assert_ne!(
        status.code(),
        Some(124),
        "the session hung past SIGTERM: {}",
        String::from_utf8_lossy(&seen)
    );
    assert_eq!(
        session_exit_status(&dir.path().join("rc.txt")),
        0,
        "SIGTERM did not end the session gracefully: {}",
        String::from_utf8_lossy(&seen)
    );
    // Restore-before-graceful ordering: the pane the shell inherits is
    // canonical — the session restored the discipline before exiting.
    assert_discipline_handed_back(&stty_path);
}

/// Case 4: the force rung, in its one deterministic terminal form — a
/// second external SIGTERM while the graceful drain is still running.
/// The drain is held open by a sync request parked on a delayed route
/// (the join is bounded by `shutdown_timeout_secs`, so the process is
/// alive well past the second signal); the termination family has no
/// `Progress`, so the watcher's ladder forces: restore + 128+signum,
/// proven by the chained stty probe. (An external double-SIGINT at the
/// prompt — the SIGINT family's force form — is not deliverable
/// deterministically: while `readline` is active the line editor owns
/// SIGINT, the same F5 grammar as the ^C byte, and the at-keyboard
/// double-press during dispatch races the acknowledgement; the ladder
/// machinery forcing is this pin's subject, and the pure tier pins the
/// SIGINT family's counts.)
#[test]
fn tty_second_sigterm_during_the_drain_forces_143_and_the_pane_is_sane() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let (server, in_flight) = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            let (tx, rx) = std_mpsc::channel();
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(NotifyThenDelay {
                    notify: tx,
                    delay: Duration::from_secs(30),
                })
                .mount(&server)
                .await;
            (server, rx)
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(
        toml,
        "server_url = '{}'\nusername = 'alice'\n",
        server.uri()
    );
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str(
        "[sync]\npoll_interval_secs = 3600\nbackoff_initial_secs = 1\nbackoff_max_secs = 5\n",
    );
    // The drain's bounded join holds the process alive against the
    // 30 s request — the second SIGTERM's window.
    toml.push_str("[app]\nshutdown_timeout_secs = 20\n");
    std::fs::write(&config, toml).expect("write config");

    let (mut session, stty_path) =
        spawn_session_pty_probe(&config, &[("TASKBOARD_APP_PASSWORD", "tty-force-pw")]);
    // At the prompt, with the boot sync parked in the delayed request.
    if !session.wait_for(|seen| {
        seen.windows(SESSION_PROMPT.len())
            .any(|w| w == SESSION_PROMPT.as_bytes())
    }) {
        panic!(
            "the pty ended before the prompt: {}",
            session.transcript_lossy()
        );
    }
    in_flight
        .recv_timeout(PTY_IDLE_BUDGET)
        .expect("the stalled sync request arrived");
    signal_session(&session, "-TERM");
    // Pacing only: standard signals coalesce while pending, so the two
    // kills need a gap to be counted as two ladder events — this is
    // not a wait-for-state sleep.
    std::thread::sleep(Duration::from_millis(500));
    signal_session(&session, "-TERM");
    let (child, stdin, out, stderr_thread) = into_parts(session);
    drop(stdin);
    let (status, seen, stderr) = conclude_session(child, None, &out, stderr_thread);
    assert_ne!(
        status.code(),
        Some(124),
        "the session hung past the double SIGTERM: {}",
        String::from_utf8_lossy(&seen)
    );
    assert_eq!(
        session_exit_status(&dir.path().join("rc.txt")),
        143,
        "the force rung exited at 128+15: {} / stderr: {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    // Restore-before-exit ordering: the pane the shell inherits is
    // canonical even though the exit was forced.
    assert_discipline_handed_back(&stty_path);
}

/// Case 5: the ^C byte at the prompt still clears the line (F5 — the
/// byte world is untouched by the handler), the prompt re-arms, and
/// the next line executes.
#[test]
fn tty_ctrl_c_at_the_prompt_clears_the_line_and_the_session_continues() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let mut session = spawn_session_pty_exec(&config, dir.path(), &[]);
    // ^C at readline 1: rustyline answers Interrupted, the line clears,
    // the editor re-arms (readline 2).
    wait_readline(&mut session, 1);
    session.stdin.write_all(b"\x03").expect("write the ^C byte");
    feed_at(&mut session, 2, "boards create B --json");
    feed_at(&mut session, 3, "exit");
    let (status, seen, stderr) = finish_session(session);

    assert_ne!(
        status.code(),
        Some(124),
        "the ^C byte wedged the prompt: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the session did not survive the ^C: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    let created: Vec<serde_json::Value> = String::from_utf8_lossy(&seen)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["title"] == "B")
        .collect();
    assert_eq!(
        created.len(),
        1,
        "the next line executed after the cleared one: {}",
        String::from_utf8_lossy(&seen)
    );
    assert!(
        json_error_lines(&seen).is_empty(),
        "a cleared line is not an error: {}",
        String::from_utf8_lossy(&seen)
    );
}

/// Case 6: the in-session ceremony's flag-less `connect` rides the
/// editor's prompt grammar — URL, username, masked secret, gated on
/// each prompt's arrival — the ceremony persists, and the session's
/// history file contains neither the URL nor the secret (prompt
/// answers never enter history or completion).
#[test]
fn tty_in_session_flagless_connect_prompts_ride_the_editor_and_stay_out_of_history() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .and(wiremock::matchers::path(BOARDS_PATH))
                .respond_with(
                    wiremock::ResponseTemplate::new(200).set_body_string(common::boards_json()),
                )
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    let db = dir.path().join("taskboard.db");
    let mut toml = String::from("[nextcloud]\ncredential_store = 'env'\n");
    let _ = write!(toml, "[storage]\ndb_path = '{}'\n", db.display());
    toml.push_str("[app]\nshutdown_timeout_secs = 5\n");
    std::fs::write(&config, toml).expect("write config");

    let mut session = spawn_session_pty_exec(&config, dir.path(), &[]);
    feed_at(&mut session, 1, "connect");
    // The three prompts ride the editor (D-3): the labels gate each
    // scripted answer (wording is a synchronization gate, never
    // asserted).
    wait_for_marker(&mut session, "Server URL");
    session
        .stdin
        .write_all(format!("{}\r", server.uri()).as_bytes())
        .expect("write the URL");
    wait_for_marker(&mut session, "Username");
    session
        .stdin
        .write_all(b"alice\r")
        .expect("write the username");
    wait_for_marker(&mut session, "App password: ");
    session
        .stdin
        .write_all(b"tty-grammar-pw\r")
        .expect("write the secret");
    // The ceremony persisted; the editor re-arms for the exit.
    feed_at(&mut session, 2, "exit");
    let (status, seen, stderr) = finish_session(session);

    assert_ne!(
        status.code(),
        Some(124),
        "the flag-less connect hung: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the flag-less connect failed: {} / {}",
        String::from_utf8_lossy(&seen),
        String::from_utf8_lossy(&stderr)
    );
    // The ceremony persisted the non-secrets, secret-free.
    let written = std::fs::read_to_string(&config).expect("config written");
    assert!(written.contains(&server.uri()));
    assert!(written.contains("alice"));
    assert!(!written.contains("tty-grammar-pw"));
    // The session's history file: prompt answers are excluded at the
    // file level (the submitted `connect`/`exit` lines are in).
    let history = dir.path().join("taskboard/history");
    let history = std::fs::read_to_string(&history).expect("history file written");
    assert!(history.contains("connect"), "submitted lines recorded");
    assert!(!history.contains(&server.uri()), "no prompt URL in history");
    assert!(!history.contains("alice"), "no prompt user in history");
    assert!(
        !history.contains("tty-grammar-pw"),
        "no prompt secret in history"
    );
}

/// Case 7: the one-shot ceremony keeps the default disposition (no
/// handler on one-shot paths, D-3/F14): ^C at its first prompt kills
/// the process (the shell's 130 convention) and nothing persists.
#[test]
fn tty_one_shot_ceremony_keeps_the_default_sigint_death() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let server = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(async {
            let server = wiremock::MockServer::start().await;
            wiremock::Mock::given(wiremock::matchers::method("GET"))
                .respond_with(wiremock::ResponseTemplate::new(401))
                .mount(&server)
                .await;
            server
        });

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, None, None).expect("write config");

    // The first ^C byte lands at the cooked first prompt (the
    // `StdinPrompts` read). Where the environment delivers SIGINT, the
    // one-shot dies pre-persist by the default disposition (the signal
    // status itself, or the shell's 128+2 convention). An
    // SIGINT-ignoring environment (rust test harnesses inherit SIG_IGN)
    // discards the byte and the read keeps waiting — the seam's idle
    // budget bounds that world at the same semantic position (a killed
    // session whose `code()` is not a completed exit); the app is
    // correct in both (the one-shot installs no handler by design,
    // D-3/F14).
    let (status, _seen, _stderr) = run_login_under_pty(&config, &server.uri(), "\x03", dir.path());
    if let Some(code) = status.code() {
        assert!(
            code == 130 || code == 124,
            "the one-shot ^C is the default death, got {code}"
        );
    }
    // Pre-persist in every world.
    let written = std::fs::read_to_string(&config).expect("config untouched");
    assert!(!written.contains(&server.uri()));
}

/// Spawns a one-shot invocation under `script(1)` with the session
/// fixture's pidfile/rc machinery (the pid names the one-shot process —
/// the signal target; the rc file is the inner shell's structural exit
/// record).
fn spawn_oneshot_pty(config: &std::path::Path, args: &str) -> (PtySession, std::path::PathBuf) {
    let dir = config
        .parent()
        .expect("config in its tempdir")
        .to_path_buf();
    let pid_path = dir.join("oneshot.pid");
    let rc_path = dir.join("oneshot-rc.txt");
    let binary = env!("CARGO_BIN_EXE_taskboard");
    let inner = format!(
        "sh -c 'echo $$ > {}; exec {} --config {} {}'; echo $? > {}",
        pid_path.display(),
        binary,
        config.display(),
        args,
        rc_path.display()
    );
    let mut session = spawn_pty_session(&inner, &dir, true, &[]);
    session.pid_path = Some(pid_path);
    (session, rc_path)
}

/// P1 (D-5 item 1): ^C during a stalled one-shot relay cancels typed —
/// the typed `cancelled` envelope on the wire, exit 130, and the
/// stalling peer observed the socket drop (the tidy teardown; orphan
/// completion is named, D-0). The watcher overrides an inherited
/// `SIG_IGN` (the rust-harness world), so delivery is deterministic.
#[test]
fn tty_ctrl_c_during_a_stalled_one_shot_cancels_typed_and_exits_130() {
    if std::process::Command::new("script")
        .arg("--help")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir();
    let config = dir.path().join("taskboard.toml");
    write_test_config(&config, None, None).expect("write config");
    let db = dir.path().join("taskboard.db");

    // The stalling daemon: binds the db's socket name, accepts, reads
    // the client's frame, then never answers — and reports the client's
    // drop (the teardown proof) over the channel.
    let (framed_tx, framed_rx) = std_mpsc::channel();
    let (dropped_tx, dropped_rx) = std_mpsc::channel();
    let listener_thread = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("listener runtime")
            .block_on(async move {
                use interprocess::local_socket::traits::tokio::Listener as _;
                use taskboard_app::ipc::codec;
                let name = taskboard_app::ipc::socket_name(&db).expect("socket name");
                let listener = interprocess::local_socket::ListenerOptions::new()
                    .name(name)
                    .create_tokio()
                    .expect("bind the stalling listener");
                let mut peer = listener.accept().await.expect("accept the relay");
                // One frame (the request), then EOF when the cancelled
                // client's connection closes.
                let _frame = codec::read_frame(&mut peer, codec::MAX_REQUEST_BYTES)
                    .await
                    .expect("the relayed request frame");
                let _ = framed_tx.send(());
                // EOF/reset: the cancelled client's connection closed
                // — the tidy teardown, observed.
                if codec::read_frame(&mut peer, codec::MAX_REQUEST_BYTES)
                    .await
                    .is_err()
                {
                    let _ = dropped_tx.send(());
                }
                // The thread ends here: the listener drops, the test's
                // `join` returns.
            });
    });

    let (session, rc_path) = spawn_oneshot_pty(&config, "sync --json");
    framed_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the relay stalled inside the transport read");
    signal_session(&session, "-INT");

    // The `script`/`timeout` wrapper's own relay proved unreliable
    // (the suite's note on `session_exit_status`): the inner shell's
    // rc file is the structural record, the pty bytes the envelope's.
    let (_status, seen, _stderr) = finish_session(session);
    assert_eq!(
        session_exit_status(&rc_path),
        130,
        "the cancelled one-shot exits at the shell's ^C code: {}",
        String::from_utf8_lossy(&seen)
    );
    // The typed envelope rode the pty (stderr merges into it).
    let cancelled = json_error_lines(&seen)
        .iter()
        .any(|envelope| envelope["error"]["kind"] == "cancelled");
    assert!(
        cancelled,
        "typed cancellation: {}",
        String::from_utf8_lossy(&seen)
    );
    // The teardown proof: the stalling peer saw the socket drop.
    dropped_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the stalling peer observed the client's socket drop");
    listener_thread.join().ok();
}
