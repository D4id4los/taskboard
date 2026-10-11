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
//! - **In-session refusals**: `daemon`, `login`, and per-line
//!   `--config` are refused at parse/route level — they would need a
//!   second process, a credential ceremony on the busy terminal, or a
//!   config different from the booted one.
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
use crate::commands::{CommandError, CommandIo, dispatch_line};
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
/// sidestepping its `Send` story entirely.
///
/// # Panics
///
/// Never in normal operation: the only `expect` guards the thread spawn
/// itself (an OS-level thread-creation failure at session start).
#[must_use]
pub fn spawn_line_reader() -> mpsc::Receiver<LineEvent> {
    let (tx, rx) = mpsc::channel(1);
    std::thread::Builder::new()
        .name("repl-line-reader".to_owned())
        .spawn(move || {
            let mut editor = match rustyline::DefaultEditor::new() {
                Ok(editor) => editor,
                Err(err) => {
                    // No line editor (e.g. no terminal): degrade to EOF —
                    // the session exits gracefully instead of spinning.
                    tracing::warn!(?err, "line editor unavailable; ending the session");
                    let _ = tx.blocking_send(LineEvent::Eof);
                    return;
                }
            };
            loop {
                match editor.readline(PROMPT) {
                    Ok(line) => {
                        let _ = editor.add_history_entry(line.as_str());
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

/// Parses one line: shell-word split → `Cli::try_parse_from` → the
/// REPL-level rules. The process never exits from a line.
#[must_use]
pub(crate) fn resolve_line(line: &str, json_default: bool) -> LineAction {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineAction::Reprompt;
    }
    let Ok(words) = shell_words::split(trimmed) else {
        return LineAction::Refuse(LineRefusal::SplitError);
    };
    let Some(first) = words.first() else {
        // A line of only quotes/escapes splits to nothing: re-prompt.
        return LineAction::Reprompt;
    };
    if matches!(first.as_str(), "exit" | "quit") {
        return LineAction::Exit;
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
        Some(Command::Daemon) => LineAction::Refuse(LineRefusal::Daemon),
        Some(Command::Login { .. }) => LineAction::Refuse(LineRefusal::Login),
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
                // clap routes by kind: help/version to stdout, usage
                // errors to stderr. A closed stream must not kill the
                // session.
                let _ = err.print();
            }
            LineAction::Refuse(kind) => {
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
/// exit 0.
pub async fn run_session_with_lines(
    config: AppConfig,
    json_default: bool,
    lines: mpsc::Receiver<LineEvent>,
    io: &mut CommandIo<'_>,
) -> u8 {
    let store: Arc<dyn CredentialStore> =
        crate::commands::production_store(config.nextcloud.credential_store);
    let backend = config.sync_backend();
    let app = match crate::bootstrap::bootstrap(config, store.clone()).await {
        Ok(app) => app,
        Err(err) => return crate::commands::report_to(io, &CommandError::from(err)),
    };
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
    let _ = writeln!(
        io.out,
        "taskboard {} — interactive session (backend: {})",
        crate::VERSION,
        match backend {
            SyncBackend::Nextcloud => "nextcloud",
            SyncBackend::Standalone => "standalone",
        }
    );
    let _exit = repl_loop(app, session, lines, io).await;
    0
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
    let lines = spawn_line_reader();
    run_session_with_lines(config, json_default, lines, &mut io).await
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
