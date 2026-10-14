// SPDX-License-Identifier: AGPL-3.0-only
//! The interactive REPL frontend (ADR 0011, WI-4): a `parted`-style
//! session over **one** booted core. No arguments on a TTY drop the user
//! into a shell where each line is parsed as one CLI invocation and
//! dispatched against the session core; `exit`/`quit`/Ctrl-D leave.
//!
//! Session semantics (the contract the plan pins):
//!
//! - **Boot once**: the core boots at session start (with the one-time
//!   boot-pull wait under a connector backend) and shuts down at exit —
//!   never per line. Mutations are durable locally the moment the
//!   line's `execute` replies (outbox semantics); the session sync
//!   actor drains them on its poll cadence or on a `sync` line.
//! - **One lock epoch**: the session holds the per-database instance
//!   lock for its lifetime, exactly like the daemon — a running daemon
//!   makes the session boot fail typed (`instance-locked`), and
//!   vice versa.
//! - **In-session refusals**: `daemon`, `login`, `logout`, `config`,
//!   and per-line `--config` are refused at parse/route level — they
//!   would need a second process, a credential ceremony on the busy
//!   terminal, or a config different from the booted one.
//! - **Lines never abort the session**: failures report and return to
//!   the prompt; the process exit code is the session's, not any
//!   line's.
//!
//! Architecture: the only blocking stdin read lives on a dedicated
//! editor thread ([`spawn_line_reader`], rustyline confined there) and
//! streams [`LineEvent`]s through a bounded channel — an async runtime
//! worker never blocks on input (fatal on a 1-core kiosk). Tests inject
//! their own channel instead, so the terminal never appears in hermetic
//! suites.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use clap::Parser as _;

use crate::cli::Command;
use crate::commands::{CommandError, CommandIo, CoreSession, connect, dispatch_line};
use crate::completion;
use crate::config::{AppConfig, SyncBackend};
use crate::secrets::CredentialStore;

/// The session prompt (a minimal constant; UI text, never asserted).
const PROMPT: &str = "taskboard> ";

/// What a bare `taskboard` invocation does (ADR 0011): an explicit
/// subcommand stays a one-shot; no arguments on a TTY open the REPL;
/// no arguments without a TTY keep the `[app] mode` resolution (the
/// daemon default — systemd/SSH-piped deployments are unaffected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A subcommand was given: one-shot dispatch, unchanged.
    Subcommand,
    /// No subcommand, stdin is a TTY: run the interactive session.
    Repl,
    /// No subcommand, no TTY: resolve `[app] mode` (daemon/kiosk/desktop).
    ModeResolution(crate::config::AppMode),
}

/// Resolves the entry arm for a `taskboard` invocation. A pure function
/// so environments only flip its inputs (CI pipes, `script(1)` ptys),
/// never its logic.
#[must_use]
pub fn entry_point(has_command: bool, stdin_tty: bool, mode: crate::config::AppMode) -> Entry {
    if has_command {
        Entry::Subcommand
    } else if stdin_tty {
        Entry::Repl
    } else {
        Entry::ModeResolution(mode)
    }
}

/// What the editor thread delivers per interaction.
#[derive(Debug, Clone)]
pub enum LineEvent {
    /// A submitted input line (split at the loop, not here).
    Line(String),
    /// Ctrl-D on an empty line (or the terminal went away).
    Eof,
    /// Ctrl-C while editing (cancel the line, keep reading).
    Interrupted,
}

/// What one terminal input the running line handler needs (ADR 0014,
/// design §5.2). The editor thread performs the read; the handler only
/// ever sees the typed answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// One cooked line (server URL, username).
    Line,
    /// A masked read (`rpassword`, echo off) on the editor thread.
    Masked,
    /// A y/N decision reduced to a bool on the editor thread.
    Confirm,
}

/// The answer the editor thread owes for a prompted read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptValue {
    /// A read line (the `Line` and `Masked` kinds).
    Text(String),
    /// A confirmed y/N (the `Confirm` kind).
    Flag(bool),
}

/// The session loop's per-line verdict to the editor thread. The
/// handshake's load-bearing rule: after delivering a [`LineEvent`], the
/// editor thread **always** waits for the next verdict before re-arming
/// `readline` — so a mid-handler prompt can never race the user's next
/// keystrokes, and the next submitted line can never be consumed as a
/// prompt answer. One verdict per consumed event; `Prompt`s repeat
/// until the handler is done, then one `Next` resumes editing.
pub enum Disposition {
    /// The line is fully handled (or cancelled); resume editing.
    Next,
    /// The running handler needs one more terminal input: perform the
    /// read on the editor thread and answer through the oneshot.
    Prompt {
        /// The full prompt text, exactly as [`connect::PromptReader`]
        /// got it from the ceremony.
        label: String,
        /// Which kind of read to perform.
        kind: PromptKind,
        /// The answer channel (AGENTS §3's oneshot-inside-mpsc idiom).
        reply: oneshot::Sender<Result<PromptValue, std::io::Error>>,
    },
}

/// What `spawn_line_reader` hands the session loop: the line stream it
/// consumes and the disposition channel it answers on. Tests inject
/// both halves (a scripted disposition peer mirrors the injected-lines
/// pattern) — the terminal never appears in hermetic suites.
pub struct EditorIo {
    /// Submitted lines, Ctrl-C and Ctrl-D events (capacity 1).
    pub lines: mpsc::Receiver<LineEvent>,
    /// The loop's verdicts (capacity 1 — strict alternation with the
    /// lines channel).
    pub dispositions: mpsc::Sender<Disposition>,
}

/// Spawns the dedicated line-editor thread: rustyline owns stdin there
/// (raw mode, history, Ctrl-C/D mapping), lines stream back through a
/// bounded channel so the reader never runs ahead of execution. When
/// the loop drops the receiver, the next send fails and the thread
/// exits. The `Editor` is created and confined to this thread,
/// sidestepping its `Send` story entirely; the tab-completion helper
/// ([`completion::ReplHelper`]) lives and dies with it and reads the
/// session state only through one lock-free `load_full()` per Tab.
///
/// After each delivered event the thread unconditionally waits for the
/// loop's [`Disposition`] before re-arming `readline` (ADR 0014): a
/// `Prompt` verdict is answered *here* — [`connect::StdinPrompts`]' sync
/// cores, the one terminal-impl body — and the answer bypasses history
/// and completion by construction (secrets never enter either). The
/// thread performs reads only; no handler logic ever runs on it.
///
/// # Panics
///
/// Never in normal operation: the only `expect` guards the thread spawn
/// itself (an OS-level thread-creation failure at session start).
#[must_use]
pub fn spawn_line_reader(state: Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>) -> EditorIo {
    let (tx, rx) = mpsc::channel(1);
    let (verdicts_tx, mut verdicts_rx) = mpsc::channel::<Disposition>(1);
    std::thread::Builder::new()
        .name("repl-line-reader".to_owned())
        .spawn(move || {
            let mut editor = match rustyline::Editor::<
                completion::ReplHelper,
                rustyline::history::DefaultHistory,
            >::new()
            {
                Ok(editor) => editor,
                Err(err) => {
                    // No line editor (e.g. no terminal): degrade to EOF —
                    // the session exits gracefully instead of spinning.
                    tracing::warn!(?err, "line editor unavailable; ending the session");
                    let _ = tx.blocking_send(LineEvent::Eof);
                    return;
                }
            };
            editor.set_helper(Some(completion::ReplHelper::new(state)));
            loop {
                match editor.readline(PROMPT) {
                    Ok(line) => {
                        // Empty and whitespace-only lines are prompt
                        // noise, not recallable entries. History is
                        // added here only — prompted answers ride the
                        // oneshot below and never reach this arm.
                        if !line.trim().is_empty() {
                            let _ = editor.add_history_entry(line.as_str());
                        }
                        if tx.blocking_send(LineEvent::Line(line)).is_err() {
                            return;
                        }
                    }
                    Err(rustyline::error::ReadlineError::Interrupted) => {
                        if tx.blocking_send(LineEvent::Interrupted).is_err() {
                            return;
                        }
                    }
                    Err(rustyline::error::ReadlineError::Eof) => {
                        let _ = tx.blocking_send(LineEvent::Eof);
                        return;
                    }
                    Err(err) => {
                        tracing::warn!(?err, "line reader failed; ending the session");
                        let _ = tx.blocking_send(LineEvent::Eof);
                        return;
                    }
                }
                // Unconditional: never re-arm `readline` while a
                // verdict (or a prompt chain) is outstanding. A dropped
                // verdict channel is the loop's exit signal.
                loop {
                    match verdicts_rx.blocking_recv() {
                        None => return,
                        Some(Disposition::Next) => break,
                        Some(Disposition::Prompt { label, kind, reply }) => {
                            let answer = match kind {
                                PromptKind::Line => connect::StdinPrompts::read_line_sync(&label)
                                    .map(PromptValue::Text),
                                PromptKind::Masked => {
                                    connect::StdinPrompts::read_secret_sync(&label)
                                        .map(PromptValue::Text)
                                }
                                PromptKind::Confirm => {
                                    connect::StdinPrompts::read_confirm_sync(&label)
                                        .map(PromptValue::Flag)
                                }
                            };
                            // A failed send means the loop is gone —
                            // the session is ending; the outer recv
                            // observes it next.
                            let _ = reply.send(answer);
                        }
                    }
                }
            }
        })
        .expect("spawn the REPL line-reader thread");
    EditorIo {
        lines: rx,
        dispositions: verdicts_tx,
    }
}

/// The session-owned [`connect::PromptReader`]: each ceremony prompt is
/// shipped to the editor thread as a [`Disposition::Prompt`] and awaited
/// through the oneshot — genuinely async, so no runtime worker is ever
/// parked on user input (ADR 0011). The dropped-peer failure maps to
/// `BrokenPipe`, which the ceremony already maps to the typed
/// `CommandError::Prompt` (a vanished editor ends the line, not the
/// session logic).
#[derive(Debug, Clone)]
pub struct ReplPrompts {
    dispositions: mpsc::Sender<Disposition>,
}

impl ReplPrompts {
    /// Builds the session-side reader over the disposition sender.
    #[must_use]
    pub(crate) fn new(dispositions: mpsc::Sender<Disposition>) -> Self {
        Self { dispositions }
    }

    /// The per-line verdict for a fully handled line: the editor thread
    /// resumes editing.
    pub(crate) async fn resume(&self) {
        let _ = self.dispositions.send(Disposition::Next).await;
    }

    /// One prompt round-trip: request the read, await the answer.
    async fn ask(&self, label: &str, kind: PromptKind) -> Result<PromptValue, std::io::Error> {
        let (reply, answer) = oneshot::channel();
        self.dispositions
            .send(Disposition::Prompt {
                label: label.to_owned(),
                kind,
                reply,
            })
            .await
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        answer
            .await
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?
    }

    fn text(value: Result<PromptValue, std::io::Error>) -> Result<String, std::io::Error> {
        match value? {
            PromptValue::Text(text) => Ok(text),
            // The editor thread answers `Flag` only to a `Confirm`
            // request, and the session loop only ever sends `Next`.
            PromptValue::Flag(_) => Err(std::io::Error::other("confirm answer to a line prompt")),
        }
    }

    fn flag(value: Result<PromptValue, std::io::Error>) -> Result<bool, std::io::Error> {
        match value? {
            PromptValue::Flag(flag) => Ok(flag),
            PromptValue::Text(_) => Err(std::io::Error::other("line answer to a confirm prompt")),
        }
    }
}

impl connect::PromptReader for ReplPrompts {
    async fn read_line(&mut self, prompt: &str) -> Result<String, std::io::Error> {
        Self::text(self.ask(prompt, PromptKind::Line).await)
    }

    async fn read_secret(&mut self, prompt: &str) -> Result<String, std::io::Error> {
        Self::text(self.ask(prompt, PromptKind::Masked).await)
    }

    async fn read_confirm(&mut self, prompt: &str) -> Result<bool, std::io::Error> {
        // Always prompts (no TTY short-circuit): the REPL's existence
        // implies a terminal (entry rule), and hermetic tests are the
        // answering peer. The never-blocks rule governs `StdinPrompts`
        // only — the seam that can face a pipe.
        Self::flag(self.ask(prompt, PromptKind::Confirm).await)
    }
}

/// The cross-line session state: the resolved config, the production
/// credential store it selected, and the launch-time `--json` default.
/// Per-line client-building verbs (`boards list/select/publish`)
/// construct fresh `DeckClient`s from it — unchanged from one-shot
/// behavior.
pub struct ReplSession {
    /// The resolved, validated config the session booted with.
    pub config: AppConfig,
    /// The credential store picked from the config at boot.
    pub store: Arc<dyn CredentialStore>,
    /// The `--json` flag from the launch invocation; lines may override
    /// per line, not per session.
    pub json_default: bool,
}

impl CoreSession for ReplSession {
    fn session_config(&self) -> &AppConfig {
        &self.config
    }
    fn session_store(&self) -> &Arc<dyn CredentialStore> {
        &self.store
    }
}

/// Why a line was refused without reaching a handler (routing-level,
/// not a typed `CommandError`). `Display` is UX text — never asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineRefusal {
    /// Unbalanced quotes: the line never parses as one invocation.
    SplitError,
    /// The session's config is fixed at boot.
    ConfigFlag,
    /// A nested daemon inside the session makes no sense.
    Daemon,
    /// Mid-session login cannot activate a connector in the running
    /// session (credential + client wiring happens at boot).
    Login,
    /// Mid-session `connect` is chunk 3's in-session ceremony; until
    /// it lands, the honest interim is the parse-level refusal (the
    /// one-shot verb owns the ceremony). Temporary dead surface: the
    /// in-session ceremony replaces this variant.
    Connect,
    /// The credential store and connector are fixed for the session's
    /// lock epoch, exactly like `login`.
    Logout,
    /// Config commands read and write the config file; the session's
    /// config is fixed at boot and `config print` is the one-shot
    /// diagnostic.
    ConfigCommand,
}

impl std::fmt::Display for LineRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SplitError => f.write_str("unbalanced quotes — one line, one invocation"),
            Self::ConfigFlag => {
                f.write_str("--config is fixed for this session; exit and relaunch to change it")
            }
            Self::Daemon => f.write_str("the daemon cannot run inside the session; exit first"),
            Self::Login => f.write_str(
                "login is not available mid-session; exit, run `taskboard login`, re-enter",
            ),
            Self::Connect => f.write_str(
                "connect is not available mid-session; exit, run `taskboard connect`, re-enter",
            ),
            Self::Logout => f.write_str(
                "logout is not available mid-session; exit, run `taskboard logout`, re-enter",
            ),
            Self::ConfigCommand => f.write_str(
                "config commands act on the config file; run them one-shot outside the session",
            ),
        }
    }
}

/// The parse-level decision for one line (a pure function; unit-pinned).
#[derive(Debug)]
pub(crate) enum LineAction {
    /// Route the parsed command against the session core; the bool is
    /// the line's JSON flag (line `--json` OR the session default).
    Execute(Command, bool),
    /// A REPL-level refusal — print the guidance, keep the session.
    Refuse(LineRefusal),
    /// A clap parse failure — clap prints it (help/version to stdout,
    /// usage errors to stderr) and the loop continues.
    PrintClap(clap::Error),
    /// `exit`/`quit`: graceful shutdown.
    Exit,
    /// Nothing to do (empty line, bare flags): re-prompt.
    Reprompt,
}

/// Parses one line: shell-word split → the REPL-only exits
/// (`exit`/`quit`) → [`resolve_words`]. The process never exits from a
/// line.
#[must_use]
pub(crate) fn resolve_line(line: &str, json_default: bool) -> LineAction {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineAction::Reprompt;
    }
    let Ok(words) = shell_words::split(trimmed) else {
        return LineAction::Refuse(LineRefusal::SplitError);
    };
    if words
        .first()
        .is_some_and(|first| matches!(first.as_str(), "exit" | "quit"))
    {
        return LineAction::Exit;
    }
    resolve_words(&words, json_default)
}

/// Parses already-split words into a [`LineAction`] — everything in
/// [`resolve_line`] after the shell-word split, shared with the daemon
/// IPC server (ADR 0012): the argv words *are* the wire, so the server
/// feeds `std::env::args().skip(1)` straight in. `exit`/`quit` are
/// deliberately *not* exits here: they are not subcommands, so
/// `resolve_words(["exit"])` is a clap usage error — unit-pinned.
#[must_use]
pub(crate) fn resolve_words(words: &[String], json_default: bool) -> LineAction {
    if words.is_empty() {
        // A line of only quotes/escapes splits to nothing: re-prompt.
        return LineAction::Reprompt;
    }
    let cli = match crate::cli::Cli::try_parse_from(
        std::iter::once("taskboard").chain(words.iter().map(String::as_str)),
    ) {
        Ok(cli) => cli,
        Err(err) => return LineAction::PrintClap(err),
    };
    if cli.config.is_some() {
        return LineAction::Refuse(LineRefusal::ConfigFlag);
    }
    match cli.command {
        // A bare global-flag line (`--json`) has nothing to execute.
        None => LineAction::Reprompt,
        Some(Command::Daemon { .. }) => LineAction::Refuse(LineRefusal::Daemon),
        Some(Command::Connect { .. }) => LineAction::Refuse(LineRefusal::Connect),
        Some(Command::Login { .. }) => LineAction::Refuse(LineRefusal::Login),
        Some(Command::Logout) => LineAction::Refuse(LineRefusal::Logout),
        Some(Command::Config { .. }) => LineAction::Refuse(LineRefusal::ConfigCommand),
        Some(command) => LineAction::Execute(command, cli.json || json_default),
    }
}

/// How the session loop ended. v1 has exactly one ending: every line
/// failure stays in the session, and Ctrl-C *during* a command is the
/// default SIGINT disposition (kills the process — safe: state is
/// durable per `execute` and the OS releases the lock).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplExit {
    /// `exit`/`quit`/Ctrl-D: the core was shut down gracefully.
    Graceful,
}

/// The session loop: consume line events, dispatch lines, honor exits,
/// and shut the core down exactly once at the end. Every consumed event
/// gets exactly one disposition — `Next` after a handled line or an
/// `Interrupted`; `Eof` and `Exit` end the loop, and the dropped
/// disposition sender is the editor thread's exit signal (D16: no final
/// verdict needed).
pub async fn repl_loop(
    app: crate::App,
    session: ReplSession,
    editor_io: EditorIo,
    io: &mut CommandIo<'_>,
) -> ReplExit {
    let EditorIo { mut lines, .. } = editor_io;
    // The loop answers every consumed event with exactly one verdict
    // through the same sender the session-side prompt reader uses.
    let verdicts = ReplPrompts::new(editor_io.dispositions);
    while let Some(event) = lines.recv().await {
        let action = match event {
            LineEvent::Interrupted => {
                verdicts.resume().await;
                continue;
            }
            LineEvent::Eof => break,
            LineEvent::Line(raw) => resolve_line(&raw, session.json_default),
        };
        match action {
            LineAction::Reprompt => {}
            LineAction::Exit => break,
            LineAction::PrintClap(err) => {
                // clap's routing, replicated through the injected
                // writers: help/version to `out`, usage errors to
                // `err`. A closed sink must not kill the session.
                tracing::debug!(kind = %err.kind(), "line did not parse; showing clap output");
                let rendered = err.render().to_string();
                if matches!(
                    err.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) {
                    let _ = writeln!(io.out, "{rendered}");
                } else {
                    let _ = writeln!(io.err, "{rendered}");
                }
            }
            LineAction::Refuse(kind) => {
                tracing::debug!(refusal = %kind, "line refused");
                let _ = writeln!(io.err, "{kind}");
            }
            LineAction::Execute(command, json) => {
                let mut line_io = CommandIo {
                    out: &mut *io.out,
                    err: &mut *io.err,
                    json,
                };
                // Informational only: a failed line never aborts the
                // session nor shapes its exit code (ADR 0011).
                let _ = dispatch_line(&session, &app, command, &mut line_io).await;
            }
        }
        // The line's one verdict: editing resumes. A ceremony that
        // prompted mid-handler has already been answered through the
        // same channel; this closes its chain.
        verdicts.resume().await;
    }
    let outcome = app.shutdown().await;
    if outcome.actor_aborted {
        tracing::warn!("sync actor did not stop in time and was aborted");
    }
    ReplExit::Graceful
}

/// Runs one session to completion against injected editor I/O and
/// writers: boot (typed failures reported, exit 1) → one-time boot-pull
/// wait under a connector backend → banner → loop → graceful shutdown →
/// exit 0. The `editor_io` factory runs immediately after a successful
/// `bootstrap` and receives the session's shared-state handle — the
/// production factory spawns the editor thread with it, so completion
/// always has a valid snapshot and tests inject their channels
/// untouched.
pub async fn run_session_with_lines(
    config: AppConfig,
    json_default: bool,
    editor_io: impl FnOnce(Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>) -> EditorIo,
    io: &mut CommandIo<'_>,
) -> u8 {
    let store: Arc<dyn CredentialStore> =
        crate::commands::production_store(config.nextcloud.credential_store);
    let backend = config.sync_backend();
    let app = match crate::bootstrap::bootstrap(
        config,
        store.clone(),
        Arc::new(crate::notifications::LogSink),
    )
    .await
    {
        Ok(app) => app,
        Err(err) => return crate::commands::report_to(io, &CommandError::from(err)),
    };
    let editor_io = editor_io(app.engine.shared_state());
    let session = ReplSession {
        config: app.config.clone(),
        store,
        json_default,
    }; // The one-shot prefix's boot-pull wait, once per session (a
    // connector-less core can never run a cycle; skip the wait).
    if backend != SyncBackend::Standalone {
        let timeout = Duration::from_secs(app.config.app.shutdown_timeout_secs.max(1));
        crate::bootstrap::wait_for_boot_pull(&app, timeout).await;
    }
    // The banner is human surface: under a `--json` session default it
    // goes to stderr so stdout stays a pure machine stream (every
    // payload line parses).
    let banner = format!(
        "taskboard {} — interactive session (backend: {})",
        crate::VERSION,
        match backend {
            SyncBackend::Nextcloud => "nextcloud",
            SyncBackend::Standalone => "standalone",
        }
    );
    if json_default {
        let _ = writeln!(io.err, "{banner}");
    } else {
        let _ = writeln!(io.out, "{banner}");
    }
    let _exit = repl_loop(app, session, editor_io, io).await;
    0
}

/// The terminal line discipline as it was when the session started,
/// restored on drop. The session's shutdown can race a *fresh*
/// `readline()` on the editor thread (`exit` hands the line over and
/// the thread loops straight back into raw mode); if
/// `std::process::exit` then wins, the process dies mid-`readline` and
/// rustyline's per-call restore never runs. The leftover raw mode
/// (IEXTEN off among others) leaves the pane's shell unable to echo
/// typed input until `stty sane`. Restoring the exact pre-session
/// state makes the hand-back correct regardless of which thread wins
/// the race. Non-terminal stdin is a no-op: `tcgetattr` fails there.
///
/// A signal kill mid-`readline` still cannot restore (no destructors
/// run) — accepted, tracked in the improvement backlog.
#[cfg(unix)]
struct TtyStateGuard {
    saved: Option<nix::sys::termios::Termios>,
}

#[cfg(unix)]
impl TtyStateGuard {
    fn take() -> Self {
        Self {
            saved: nix::sys::termios::tcgetattr(std::io::stdin()).ok(),
        }
    }
}

#[cfg(unix)]
impl Drop for TtyStateGuard {
    fn drop(&mut self) {
        if let Some(saved) = &self.saved {
            let _ = nix::sys::termios::tcsetattr(
                std::io::stdin(),
                nix::sys::termios::SetArg::TCSADRAIN,
                saved,
            );
        }
    }
}

/// Non-unix line editing does not run through a termios line
/// discipline; nothing to save or restore.
#[cfg(not(unix))]
struct TtyStateGuard;

#[cfg(not(unix))]
impl TtyStateGuard {
    fn take() -> Self {
        Self
    }
}

/// The production session: real terminal (editor thread) and std
/// streams.
pub async fn run_session(config: AppConfig, json_default: bool) -> u8 {
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut err,
        json: json_default,
    };
    // Held for the whole session, before the editor thread exists:
    // dropping at scope end (before main's `process::exit`) hands the
    // terminal back exactly as found.
    let _tty = TtyStateGuard::take();
    run_session_with_lines(config, json_default, spawn_line_reader, &mut io).await
}

#[cfg(test)]
mod disposition_tests {
    use std::collections::VecDeque;

    use tokio::sync::mpsc;

    use super::{Disposition, PromptKind, PromptValue, ReplPrompts};
    use crate::commands::connect::PromptReader as _;

    /// Spawns a scripted answering peer: consumes dispositions, ignores
    /// `Next`, answers `Prompt`s from the script in order.
    fn scripted_peer(
        mut rx: mpsc::Receiver<Disposition>,
        script: VecDeque<Result<PromptValue, std::io::Error>>,
    ) -> tokio::task::JoinHandle<usize> {
        tokio::spawn(async move {
            let mut script = script;
            let mut answered = 0;
            while let Some(disposition) = rx.recv().await {
                match disposition {
                    Disposition::Next => {}
                    Disposition::Prompt { reply, .. } => {
                        let _ =
                            reply.send(script.pop_front().expect("script must cover every prompt"));
                        answered += 1;
                    }
                }
            }
            answered
        })
    }

    fn channel() -> (ReplPrompts, tokio::task::JoinHandle<usize>) {
        let (tx, rx) = mpsc::channel(1);
        let script = VecDeque::from(vec![
            Ok(PromptValue::Text("server".to_owned())),
            Ok(PromptValue::Text("secret".to_owned())),
            Ok(PromptValue::Flag(true)),
            Err(std::io::Error::other("scripted failure")),
        ]);
        (ReplPrompts::new(tx), scripted_peer(rx, script))
    }

    /// U2: each `PromptReader` method emits its matching kind (the
    /// `Line`/`Masked`/`Confirm` kinds carry the label through the
    /// disposition) and returns the scripted value; a `Fail` script
    /// maps to the io error. Exhaustion order pins the ceremony's
    /// prompt sequence.
    #[tokio::test]
    async fn the_prompt_reader_round_trips_every_kind() {
        let (mut prompts, peer) = channel();

        assert_eq!(
            prompts.read_line("Server URL: ").await.expect("line"),
            "server"
        );
        assert_eq!(
            prompts.read_secret("App password: ").await.expect("secret"),
            "secret"
        );
        assert!(prompts.read_confirm("ok? [y/N] ").await.expect("confirm"));
        assert!(matches!(
            prompts.read_line("again: ").await,
            Err(err) if err.kind() == std::io::ErrorKind::Other,
        ));
        // Drop the sender so the peer's recv loop can end.
        drop(prompts);

        let answered = peer.await.expect("peer joins");
        assert_eq!(answered, 4);
    }

    /// U2: a dropped peer (the answering side gone) maps to
    /// `BrokenPipe` — the ceremony's typed-cancellation path.
    #[tokio::test]
    async fn a_dropped_peer_maps_to_broken_pipe() {
        let (tx, rx) = mpsc::channel::<Disposition>(1);
        drop(rx);
        let mut prompts = ReplPrompts::new(tx);
        assert!(matches!(
            prompts.read_line("probe: ").await,
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe,
        ));
    }

    /// U3 (the history-bypass pin, protocol level): during a scripted
    /// prompt exchange the lines channel stays empty — answers ride the
    /// oneshot and never enter the line stream, so they can never enter
    /// history or completion by construction.
    #[tokio::test]
    async fn prompt_answers_bypass_the_lines_channel() {
        let (verdict_tx, verdict_rx) = mpsc::channel(1);
        let (lines_tx, mut lines_rx) = mpsc::channel::<super::LineEvent>(1);
        let (prompts, peer) = {
            let script = VecDeque::from(vec![Ok(PromptValue::Text("secret".to_owned()))]);
            (
                ReplPrompts::new(verdict_tx),
                scripted_peer(verdict_rx, script),
            )
        };
        drop(lines_tx);
        let mut prompts = prompts;
        assert_eq!(
            prompts.read_secret("App password: ").await.expect("answer"),
            "secret"
        );
        // Drop the sender so the peer's recv loop can end.
        drop(prompts);
        peer.await.expect("peer joins");
        // The exchange touched only the disposition channel: the line
        // stream closed untouched (its sender was dropped before the
        // exchange) — an answer could not have ridden it.
        assert!(lines_rx.recv().await.is_none());
    }

    /// U2: the `resume` verdict delivers exactly `Next` (the loop's
    /// per-line contract with the editor thread).
    #[tokio::test]
    async fn resume_sends_next() {
        let (tx, mut rx) = mpsc::channel(1);
        let prompts = ReplPrompts::new(tx);
        prompts.resume().await;
        assert!(matches!(rx.recv().await, Some(Disposition::Next)));
    }

    /// U2: the disposition carries the kind — the peer observes a
    /// `Masked` request for `read_secret` (the pin that the secret
    /// never rides a cooked path).
    #[tokio::test]
    async fn the_secret_prompt_travels_as_masked() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut prompts = ReplPrompts::new(tx);
        let ask = tokio::spawn(async move {
            let _ = prompts.read_secret("App password: ").await;
        });
        match rx.recv().await {
            Some(Disposition::Prompt { label, kind, reply }) => {
                assert_eq!(label, "App password: ");
                assert_eq!(kind, PromptKind::Masked);
                let _ = reply.send(Ok(PromptValue::Text("pw".to_owned())));
            }
            _other => panic!("expected a Prompt disposition"),
        }
        ask.await.expect("ask joins");
    }
}

#[cfg(test)]
mod tests {
    use super::{LineAction, LineRefusal, entry_point};
    use crate::cli::{BoardsCmd, Command};
    use crate::config::AppMode;
    use crate::repl::Entry;

    #[test]
    fn the_entry_table_resolves_command_tty_and_mode() {
        // A command always wins, whatever the terminal and mode.
        assert_eq!(entry_point(true, true, AppMode::Kiosk), Entry::Subcommand);
        assert_eq!(entry_point(true, false, AppMode::Daemon), Entry::Subcommand);
        // Bare + TTY is the REPL, whatever `[app] mode` says (the
        // pre-REPL ModeNotImplemented kiosk console becomes useful).
        assert_eq!(entry_point(false, true, AppMode::Kiosk), Entry::Repl);
        assert_eq!(entry_point(false, true, AppMode::Desktop), Entry::Repl);
        assert_eq!(entry_point(false, true, AppMode::Daemon), Entry::Repl);
        // Bare + no TTY keeps the mode resolution (deployments piped
        // through systemd/SSH are unaffected by the REPL).
        assert_eq!(
            entry_point(false, false, AppMode::Daemon),
            Entry::ModeResolution(AppMode::Daemon)
        );
        assert_eq!(
            entry_point(false, false, AppMode::Kiosk),
            Entry::ModeResolution(AppMode::Kiosk)
        );
    }

    /// Structural line-parse table: quoting survives, per-line `--json`,
    /// the refusal and exit rules, and garbage → clap-print not exit.
    #[test]
    fn the_line_table_parses_routes_and_refuses() {
        use crate::repl::resolve_line;

        // Quoted titles survive the split as one argument.
        let execute = |action: LineAction| match action {
            LineAction::Execute(command, json) => (command, json),
            _ => panic!("expected Execute"),
        };
        let (command, json) = execute(resolve_line("tasks add \"two words\" --json", false));
        assert!(json);
        assert!(matches!(
            command,
            Command::Tasks {
                cmd: crate::cli::TasksCmd::Add { .. }
            }
        ));
        let (command, json) = execute(resolve_line("tasks add 'two words'", true));
        assert!(json, "the session default applies");
        assert!(matches!(command, Command::Tasks { .. }));

        // Session default off + no line flag → human output.
        let (_, json) = execute(resolve_line("tasks list", false));
        assert!(!json);
        // A line-level `--json` overrides the human session default.
        let (_, json) = execute(resolve_line("--json tasks list", false));
        assert!(json);

        // Escaped quotes inside a quoted argument survive.
        let (command, _) = execute(resolve_line(r#"tasks add "say \"hi\" now""#, false));
        let Command::Tasks {
            cmd: crate::cli::TasksCmd::Add { title, .. },
        } = command
        else {
            panic!("expected tasks add");
        };
        assert_eq!(title, "say \"hi\" now");

        // Refusals: routing-level, not handler errors.
        let refuse = |line: &str| match resolve_line(line, false) {
            LineAction::Refuse(kind) => kind,
            other => panic!("expected Refuse, got {other:?}"),
        };
        assert_eq!(refuse("daemon"), LineRefusal::Daemon);
        assert_eq!(refuse("connect"), LineRefusal::Connect);
        assert_eq!(refuse("connect --server http://x"), LineRefusal::Connect);
        assert_eq!(refuse("login --server http://x"), LineRefusal::Login);
        assert_eq!(refuse("logout"), LineRefusal::Logout);
        assert_eq!(refuse("config print"), LineRefusal::ConfigCommand);
        assert_eq!(
            refuse("tasks list --config other.toml"),
            LineRefusal::ConfigFlag
        );
        assert_eq!(refuse("unbalanced 'quote"), LineRefusal::SplitError);

        // Exits and re-prompts.
        assert!(matches!(resolve_line("exit", false), LineAction::Exit));
        assert!(matches!(resolve_line("quit", false), LineAction::Exit));
        assert!(matches!(resolve_line("   ", false), LineAction::Reprompt));
        assert!(matches!(resolve_line("", false), LineAction::Reprompt));
        assert!(matches!(
            resolve_line("--json", false),
            LineAction::Reprompt
        ));

        // Garbage is a clap print-and-continue, never an exit.
        assert!(matches!(
            resolve_line("frobnicate", false),
            LineAction::PrintClap(_)
        ));
        assert!(matches!(
            resolve_line("tasks frobnicate", false),
            LineAction::PrintClap(_)
        ));
        // The boards subcommands route with their payload intact.
        let (command, _) = execute(resolve_line("boards create Personal", false));
        assert!(matches!(
            command,
            Command::Boards {
                cmd: BoardsCmd::Create { .. }
            }
        ));
    }
}

#[cfg(test)]
mod argv_tests {
    use crate::cli::Command;
    use crate::repl::{LineAction, resolve_words};

    /// The argv path (daemon IPC): the same parse rules as REPL lines,
    /// except the REPL-only exits — `exit`/`quit` are not subcommands,
    /// so they land in clap's usage-error arm (unit-pinned by the
    /// server's refusal table).
    #[test]
    fn argv_words_parse_like_lines_and_exit_is_a_usage_error() {
        let execute = |action: LineAction| match action {
            LineAction::Execute(command, json) => (command, json),
            other => panic!("expected Execute, got {other:?}"),
        };
        // Core argv parses and routes.
        let (command, json) = execute(resolve_words(
            &["tasks".to_owned(), "list".to_owned(), "--json".to_owned()],
            false,
        ));
        assert!(json);
        assert!(matches!(command, Command::Tasks { .. }));

        // The refusals are the same kinds the REPL gets.
        let refuse = |words: &[&str]| match resolve_words(
            &words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>(),
            false,
        ) {
            LineAction::Refuse(kind) => kind,
            other => panic!("expected Refuse, got {other:?}"),
        };
        assert_eq!(refuse(&["daemon"]), crate::repl::LineRefusal::Daemon);
        assert_eq!(
            refuse(&["daemon", "stop"]),
            crate::repl::LineRefusal::Daemon
        );
        assert_eq!(refuse(&["connect"]), crate::repl::LineRefusal::Connect);
        assert_eq!(
            refuse(&["connect", "--server", "http://x"]),
            crate::repl::LineRefusal::Connect
        );
        assert_eq!(refuse(&["login"]), crate::repl::LineRefusal::Login);
        assert_eq!(refuse(&["logout"]), crate::repl::LineRefusal::Logout);
        assert_eq!(
            refuse(&["config", "print"]),
            crate::repl::LineRefusal::ConfigCommand
        );
        assert_eq!(
            refuse(&["tasks", "list", "--config", "x"]),
            crate::repl::LineRefusal::ConfigFlag
        );

        // `exit` is not a subcommand: a clap usage print, never `Exit`.
        assert!(matches!(
            resolve_words(&["exit".to_owned()], false),
            LineAction::PrintClap(_)
        ));

        // Empty argv re-prompts (nothing to do), bare flags too.
        assert!(matches!(resolve_words(&[], false), LineAction::Reprompt));
        assert!(matches!(
            resolve_words(&["--json".to_owned()], false),
            LineAction::Reprompt
        ));

        // Garbage is a clap print, never an exit.
        assert!(matches!(
            resolve_words(&["frobnicate".to_owned()], false),
            LineAction::PrintClap(_)
        ));
    }
}
