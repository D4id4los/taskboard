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

use tokio::sync::mpsc;

use clap::Parser as _;

use crate::cli::Command;
use crate::commands::{CommandError, CommandIo, CoreSession, dispatch_line};
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

/// Spawns the dedicated line-editor thread: rustyline owns stdin there
/// (raw mode, history, Ctrl-C/D mapping), lines stream back through a
/// bounded channel so the reader never runs ahead of execution. When
/// the loop drops the receiver, the next send fails and the thread
/// exits. The `Editor` is created and confined to this thread,
/// sidestepping its `Send` story entirely; the tab-completion helper
/// ([`completion::ReplHelper`]) lives and dies with it and reads the
/// session state only through one lock-free `load_full()` per Tab.
///
/// # Panics
///
/// Never in normal operation: the only `expect` guards the thread spawn
/// itself (an OS-level thread-creation failure at session start).
#[must_use]
pub fn spawn_line_reader(
    state: Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>,
) -> mpsc::Receiver<LineEvent> {
    let (tx, rx) = mpsc::channel(1);
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
                        // noise, not recallable entries.
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
            }
        })
        .expect("spawn the REPL line-reader thread");
    rx
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
/// and shut the core down exactly once at the end.
pub async fn repl_loop(
    app: crate::App,
    session: ReplSession,
    mut lines: mpsc::Receiver<LineEvent>,
    io: &mut CommandIo<'_>,
) -> ReplExit {
    while let Some(event) = lines.recv().await {
        let action = match event {
            LineEvent::Interrupted => continue,
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
    }
    let outcome = app.shutdown().await;
    if outcome.actor_aborted {
        tracing::warn!("sync actor did not stop in time and was aborted");
    }
    ReplExit::Graceful
}

/// Runs one session to completion against injected line events and
/// writers: boot (typed failures reported, exit 1) → one-time boot-pull
/// wait under a connector backend → banner → loop → graceful shutdown →
/// exit 0. The `lines` factory runs immediately after a successful
/// `bootstrap` and receives the session's shared-state handle — the
/// production factory spawns the editor thread with it, so completion
/// always has a valid snapshot and tests inject their channel untouched.
pub async fn run_session_with_lines(
    config: AppConfig,
    json_default: bool,
    lines: impl FnOnce(Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>) -> mpsc::Receiver<LineEvent>,
    io: &mut CommandIo<'_>,
) -> u8 {
    let store: Arc<dyn CredentialStore> =
        crate::commands::production_store(config.nextcloud.credential_store);
    let backend = config.sync_backend();
    let app = match crate::bootstrap::bootstrap(config, store.clone()).await {
        Ok(app) => app,
        Err(err) => return crate::commands::report_to(io, &CommandError::from(err)),
    };
    let lines = lines(app.engine.shared_state());
    let session = ReplSession {
        config: app.config.clone(),
        store,
        json_default,
    };
    // The one-shot prefix's boot-pull wait, once per session (a
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
    let _exit = repl_loop(app, session, lines, io).await;
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
