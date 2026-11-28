// SPDX-License-Identifier: AGPL-3.0-only
//! The CLI surface (ADRs 0008/0009): clap types only — dispatch lives in
//! `commands::dispatch` (the lib); `main` is a thin process shell.

use std::path::PathBuf;

use clap::Subcommand;

/// The `taskboard` command line.
#[derive(Debug, clap::Parser)]
#[command(name = "taskboard", version = crate::VERSION, about = "Taskboard: offline-first task sync for desktop and kiosk")]
pub struct Cli {
    /// Path to the config file. Resolution order: this flag >
    /// `TASKBOARD_CONFIG` > `$XDG_CONFIG_HOME/taskboard/taskboard.toml`
    /// (when it exists) > `./taskboard.toml` (absent tolerated).
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Machine-readable output on stdout; errors as JSON on stderr.
    #[arg(long, global = true)]
    pub json: bool,
    /// The subcommand; `None` resolves `[app] mode` (the daemon until
    /// the UI phases land).
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommand catalogue.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Connect this installation to a Nextcloud server: verify the
    /// credentials against the server, then persist the connection (the
    /// non-secrets to the config, the secret to the credential store).
    Connect {
        /// Server URL (prompted when omitted).
        #[arg(long)]
        server: Option<String>,
        /// Login name (prompted when omitted).
        #[arg(long)]
        user: Option<String>,
        /// Rewrite an explicit `sync.backend = "standalone"` to
        /// `nextcloud` — the scripted loud flip. Without it a terminal
        /// asks (y/N); a non-interactive run refuses typed.
        #[arg(long)]
        switch_backend: bool,
        /// Re-point an already-configured connection to a different
        /// server/user without the interactive confirm (the old
        /// identity's secret is deleted after the new one verifies).
        #[arg(long)]
        force: bool,
    },
    /// Deprecated alias of `connect` (removal revisited at v0.1.0):
    /// the same ceremony under the old name, without the
    /// `--switch-backend`/`--force` escapes — an explicit `standalone`
    /// backend or a configured different identity refuses typed,
    /// naming `connect`.
    Login {
        /// Server URL (prompted when omitted).
        #[arg(long)]
        server: Option<String>,
        /// Login name (prompted when omitted).
        #[arg(long)]
        user: Option<String>,
    },
    /// Board discovery, stand-alone board creation, and the sync
    /// binding/publish ceremonies.
    Boards {
        #[command(subcommand)]
        cmd: BoardsCmd,
    },
    /// Stack verbs over the in-process core.
    Stacks {
        #[command(subcommand)]
        cmd: StacksCmd,
    },
    /// Task verbs over the in-process core.
    Tasks {
        #[command(subcommand)]
        cmd: TasksCmd,
    },
    /// Label verbs over the in-process core (uuid | exact title selectors).
    Labels {
        #[command(subcommand)]
        cmd: LabelsCmd,
    },
    /// Fuzzy title search across all live boards, stacks, and tasks
    /// (done included); prints ranked rows — an exact title first, then
    /// prefixes, word starts, substrings, subsequences. The scripting
    /// bridge over the same ranking the GUI palette uses.
    // The palette's spelling twin (cross-surface friction plan
    // D-5, #772): `search` resolves to this same variant — one
    // ranking engine, two fronts; `find` stays primary, the alias
    // carries the variant's arg validation verbatim.
    #[command(alias = "search")]
    Find {
        /// The query: case-insensitive, trimmed; an empty (or
        /// whitespace-only) query matches nothing.
        query: String,
        /// Restrict the results to one entity kind. An unknown word is
        /// a clap usage error, never a runtime string compare.
        #[arg(long = "type")]
        kind: Option<FindType>,
        /// Cap the ranked output (applied after ranking).
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        limit: Option<u64>,
    },
    /// Quick capture (#347): lands the thought in the global Inbox
    /// (created unbound on first capture); with `--board`, in that
    /// board's Backlog stack (created on demand). Never errors on the
    /// happy path.
    Capture {
        /// The captured thought, verbatim (the engine does not
        /// validate titles; the UI trims at its edge).
        title: String,
        /// Target board (exact uuid | exact title among the live
        /// boards). Omit for the global Inbox.
        #[arg(long)]
        board: Option<String>,
    },
    /// Standalone timer/alarm verbs over the in-process core: fire once
    /// at `--in`/`--at`, die by settlement. Device-local — never synced.
    Timers {
        #[command(subcommand)]
        cmd: TimersCmd,
    },
    /// Break-reminder rule verbs over the in-process core (#495): add
    /// a rule firing after an unbroken tracked-work threshold, list,
    /// remove. Device-local — never synced (the `timers` family's
    /// exact situation).
    Breaks {
        #[command(subcommand)]
        cmd: BreaksCmd,
    },
    /// Time tracking over the in-process core (D-T14): start/stop the
    /// running timer, log retroactive blocks, adjust/delete history
    /// (ADR 0017 — the open entry *is* the timer; nothing ticks).
    Track {
        #[command(subcommand)]
        cmd: TrackCmd,
    },
    /// Habit trackers (DR-6/6): add/list/log/adjust/update/rm over the
    /// in-process core. Trackers are board-scoped today-view items;
    /// `anchor_time` is a display chip and never an ordering input.
    Habits {
        #[command(subcommand)]
        cmd: HabitsCmd,
    },
    /// Review-journal verbs over the in-process core: capture a
    /// reflection entry with ratings and health flags, browse and edit
    /// them, tombstone them. Device-local — never synced. (The clap
    /// name is the user-facing verb; the code symbols stay
    /// `Reflection*` — the D-J1 vocabulary ruling, ADR 0021.)
    #[command(name = "journal")]
    Reflection {
        #[command(subcommand)]
        cmd: ReflectionCmd,
    },
    /// Export time entries to stdout as data (D-T15): CSV by default,
    /// JSON optional. Pipes are the contract — no file writing, no
    /// clipboard — and the payload deliberately bypasses the `--json`
    /// receipt envelope (the recorded D-T15 exception).
    Export {
        /// Which data family to export (`tracking` by default — bare
        /// `export` is byte-compatible with today).
        #[arg(long, value_enum)]
        what: Option<ExportWhat>,
        /// Output format (csv by default).
        #[arg(long, value_enum)]
        format: Option<ExportFormat>,
        /// Only entries starting on/after this UTC date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        since: Option<chrono::NaiveDate>,
        /// Only entries starting before the day after this UTC date —
        /// inclusive of the named day (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        until: Option<chrono::NaiveDate>,
        /// Only entries of this task (uuid | unique ≥8-char uuid
        /// prefix | exact live title).
        #[arg(long)]
        task: Option<String>,
        /// Only entries whose task's stack belongs to this board
        /// (exact uuid | exact title among the live boards).
        #[arg(long)]
        board: Option<String>,
        /// The print layout (pdf leg only; `fold` by default).
        #[cfg(feature = "pdf-export")]
        #[arg(long, value_enum)]
        layout: Option<PdfLayout>,
        /// The pdf destination file (pdf leg only; the config default
        /// when omitted — `[export] pdf_dir` + the dated file name).
        #[cfg(feature = "pdf-export")]
        #[arg(long)]
        out: Option<PathBuf>,
        /// Skip opening the written pdf in the system viewer (pdf leg
        /// only; overrides `[export] open_after_export`).
        #[cfg(feature = "pdf-export")]
        #[arg(long)]
        no_open: bool,
    },
    /// Delete the stored Nextcloud credentials (and, under the auto
    /// backend, the config's server/user keys). Idempotent; the board
    /// binding and local db are untouched.
    Logout,
    /// Run one full sync cycle now and report the status.
    Sync {
        /// Return immediately after nudging the sync actor.
        #[arg(long)]
        no_wait: bool,
        /// Scope the reported status (and the wait's watched aggregate)
        /// to this connector's bound boards; the nudge stays broadcast.
        /// The default (first) connector when omitted.
        #[arg(long)]
        connector: Option<String>,
        /// Wait budget for the cycle, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Effective-configuration diagnostics.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Who serves this database and how is sync (never boots a core,
    /// never triggers a sync cycle). With a running daemon: the live
    /// view over IPC; without: the lock sidecar's diagnostic PID.
    Status,
    /// Remove the stale lock sidecar (#1249): the GUI status panel's
    /// force-unlock verb at the terminal — the same remediation
    /// executor, the same typed semantics (the stale-holder liveness
    /// re-check before the sidecar removal, the `SelfPid` refusal),
    /// never hand-rolled `rm`. Refused typed (exit 1) under a live
    /// holder — kill it first (`status` names the pid).
    Unlock,
    /// Run the headless sync daemon (M1) — boots the core, binds the
    /// per-database IPC socket, and serves commands until a signal or
    /// `daemon stop`.
    Daemon {
        #[command(subcommand)]
        cmd: Option<DaemonCmd>,
        /// Boot the core without binding the IPC socket (`[ipc] enabled
        /// = false` as a flag; the flag wins over the file). Only the
        /// run arm reads it — `daemon stop --no-ipc` parses and ignores
        /// it (a stop never spawns a server). `global` so the flag is
        /// accepted after the subcommand too, where it is inert.
        #[arg(long, global = true)]
        no_ipc: bool,
    },
    /// Run the desktop UI in this process: explicit frontend selection
    /// (the `daemon` verb's mirror) — boots the core and opens the
    /// desktop window regardless of `[app] mode` and of TTY. A running
    /// instance on the same database fails typed `instance-locked`.
    Gui,
}

/// The `find --type` values: the one entity kind a result row may name.
/// The workspace's first clap `ValueEnum` — validation is the parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FindType {
    /// Task cards.
    Task,
    /// Stacks (columns).
    Stack,
    /// Boards.
    Board,
    /// Habits — live and tombstoned alike (#1218: a tombstoned
    /// habit's retained log stays reachable).
    Habit,
}

/// `daemon` subcommands.
#[derive(Debug, Subcommand)]
pub enum DaemonCmd {
    /// Gracefully stop a running daemon over its IPC socket (typed
    /// `no-daemon` failure, exit 1, when nothing serves this database).
    Stop,
    /// Restart the daemon (#1249): the GUI status panel's
    /// restart verb at the terminal — the same remediation executor's
    /// graceful-stop-then-detached-spawn semantics (a not-serving
    /// database skips the stop leg and spawns; the typed outcome
    /// carries the new pid). Never a forked kill/relaunch.
    Restart,
}

/// `tasks rules` subcommands (#1246): the verb-shaped front of the
/// `tasks remind` flag grammar.
#[derive(Debug, Subcommand)]
pub enum RulesCmd {
    /// Remove all of the task's reminder rules — the verb spelling of
    /// `tasks remind <task> --clear` (#1246, O-5 parity): the same
    /// `ClearReminderRules` dispatch through the same handler, never a
    /// forked path.
    Clear {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
    },
}

/// `boards` subcommands.
#[derive(Debug, Subcommand)]
pub enum BoardsCmd {
    /// Move a Deck board onto the files tier (resumable: rerun after any
    /// interruption). `--undo` restores the Deck binding until the
    /// files binding commits; `--to-connection` rebinds Deck-to-Deck.
    #[command(group(clap::ArgGroup::new("move_direction").required(true)))]
    Move {
        /// The local board (uuid).
        #[arg(value_parser = parse_board_id)]
        board: taskboard_domain::BoardId,
        /// Run the ceremony towards the files tier.
        #[arg(long, group = "move_direction")]
        to_files: bool,
        /// Undo a move that has not committed.
        #[arg(long, group = "move_direction")]
        undo: bool,
        /// Deck-to-Deck rebind: move this Deck-bound board onto the
        /// named Deck connector (the files-sync-status plan US-10) —
        /// the remote Deck board id is preserved verbatim.
        #[arg(long, group = "move_direction")]
        to_connection: Option<String>,
        /// Wait budget for the final Deck pull, in seconds (the
        /// shared `DEFAULT_MOVE_TIMEOUT`, seconds half).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// List the server's boards (remote ids feed `boards select`).
    List {
        /// The connector whose server is listed; the default (first)
        /// connector when omitted.
        #[arg(long)]
        connector: Option<String>,
    },
    /// Bind this installation to one board (refuses a rebind; migrates a
    /// local board with `--bind-local-board`; pins the local board with
    /// `--board` when several are live).
    Select {
        /// The remote Deck board id (from `boards list`). The internal
        /// clap id is `remote_board` (completion routes it as server
        /// state); the displayed syntax is unchanged.
        #[arg(id = "remote_board", value_name = "board")]
        board: u64,
        /// The connector whose listing pre-validates the id and whose
        /// binding tag the bind mints; the default (first) connector
        /// when omitted.
        #[arg(long)]
        connector: Option<String>,
        /// The local board (uuid) to bind; the single live board when
        /// omitted, a freshly minted one on a board-less install.
        #[arg(long, value_parser = parse_board_id)]
        local_board: Option<taskboard_domain::BoardId>,
        /// Adoption wait budget, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        /// Bind the existing local board onto the remote board
        /// (migration path for stand-alone data).
        #[arg(long)]
        bind_local_board: bool,
    },
    /// Create a local board (stand-alone first-run command; always
    /// mints — titles are not uniqueness keys).
    Create {
        /// The board title.
        title: String,
        /// Board color as six hex digits (no `#`); a default applies
        /// when omitted.
        #[arg(long)]
        color: Option<String>,
    },
    /// Rename a local board's title (both backends; the rename pushes
    /// when the board is bound, and rides the next `publish` otherwise).
    Rename {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The new board title.
        title: String,
    },
    /// Publish a local board as a new Nextcloud board and bind it
    /// (the migration path for stand-alone data; sync-only). With
    /// `--to-files`, never touches Deck: the board binds straight onto
    /// the local files-sync tier (the GUI roster's publish ceremony).
    Publish {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Bind directly onto the files-sync tier instead of creating
        /// a Deck board (requires the `[files_sync]` lane; no remote
        /// board is ever created).
        #[arg(long, conflicts_with = "connector")]
        to_files: bool,
        /// The connector whose server the remote board is created on
        /// and whose binding tag the bind mints; the default (first)
        /// connector when omitted. (Meaningless with `--to-files` —
        /// the files tier has exactly one connector — and refused at
        /// the parse boundary.)
        #[arg(long)]
        connector: Option<String>,
        /// Wait budget in seconds (default 60). The Deck path spends
        /// it on the settling first sync; the `--to-files` path keeps
        /// it as the bind await's budget only — the files actor picks
        /// the board up on its next cycle.
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Set the session's board selection (REPL-only): later board-scoped
    /// lines pre-fill `--board` from it. Without a uuid, answers the
    /// current selection. One-shot and daemon-socket invocations refuse.
    Use {
        /// The local board (uuid); omit to query the selection.
        #[arg(value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Set a board's color (pushes as a full-send when the board is
    /// bound).
    Color {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Board color as six hex digits (no `#`).
        #[arg(value_parser = parse_color)]
        color: String,
    },
    /// Archive a board (archived boards stay commandable; the GUI
    /// stops listing them).
    Archive {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Unarchive a board.
    Unarchive {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Delete a board: tombstone-cascades its stacks, tasks, and
    /// labels, cancels the pending ops of the doomed entities, and
    /// queues the remote delete (a bound board's remote content is
    /// destroyed server-side on the next cycle).
    Delete {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the destruction (the refusal without it is typed and
        /// states what would be destroyed).
        #[arg(long)]
        confirm: bool,
    },
    /// Reset a board's local content (stacks, tasks, labels, outbox,
    /// validators, reminder facts) in one local-only repair; the board
    /// row and binding survive. A bound board re-materializes from the
    /// next pull.
    Reset {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the repair (the refusal without it is typed and
        /// states what would be cleared).
        #[arg(long)]
        confirm: bool,
    },
    /// Detach the board's remote binding: content and queued ops stay
    /// (the ops strand honestly and drain on a re-bind); the binding
    /// and the board's sync-status row go. Repeat is an idempotent
    /// no-op.
    Unbind {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the unbind (the refusal without it is typed and
        /// states what will strand).
        #[arg(long)]
        confirm: bool,
    },
    /// Deterministically merge same-title duplicate entities the bind
    /// ceremony's pull adopted alongside pre-existing local rows
    /// (survivors keep the local ids; duplicate rows neutralize).
    /// Idempotent.
    Reconcile {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the merge (the refusal without it is typed and
        /// states the match counts).
        #[arg(long)]
        confirm: bool,
    },
}

/// `stacks` subcommands. The stack selector is the uuid or the exact
/// live title (prefixes are a task-only affordance).
#[derive(Debug, Subcommand)]
pub enum StacksCmd {
    /// Create a stack on a board (appended by default).
    Add {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The stack title.
        title: String,
        /// Explicit sort order; default appends to the end of the board.
        #[arg(long)]
        order: Option<i64>,
    },
    /// Rename a stack.
    Rename {
        /// The stack (uuid | exact title).
        stack: String,
        /// The new title.
        title: String,
    },
    /// Delete a stack and tombstone-cascade its tasks (no confirmation;
    /// repeats are idempotent).
    Delete {
        /// The stack (uuid | exact title).
        stack: String,
    },
    /// Reorder a stack within the board (no-op when unchanged).
    Move {
        /// The stack (uuid | exact title).
        stack: String,
        /// The new sort position.
        #[arg(long)]
        order: i64,
    },
}

/// The default label color: Deck's UI default green. `labels add`
/// applies it when `--color` is omitted.
pub const DEFAULT_LABEL_COLOR: &str = "31cc7c";

/// The clap value-parser wrapper over the domain's six-hex mint
/// (tag-color UX plan US-1/D-2, #839): the grammar lives in
/// `taskboard_domain::Color::parse` (one behavior, one
/// implementation — the GUI commits through the same fn); this arm
/// only maps the typed refusal onto a clap usage error. Labels and
/// boards (an invalid color is a permanent push rejection; the lenient
/// domain value stays for adopted remote content, new input gets this
/// boundary validation).
fn parse_color(raw: &str) -> Result<String, String> {
    taskboard_domain::Color::parse(raw)
        .map(|color| color.as_str().to_owned())
        .map_err(|taskboard_domain::InvalidColor| {
            "expected exactly six hex digits (e.g. 31cc7c, no '#')".to_owned()
        })
}

/// A local board uuid or a usage error — the `--board` flag of the
/// board-scoped verbs. A malformed value is a clap usage error (exit 2),
/// the `parse_color` precedent.
fn parse_board_id(raw: &str) -> Result<taskboard_domain::BoardId, String> {
    uuid::Uuid::parse_str(raw)
        .map(taskboard_domain::BoardId::from_uuid)
        .map_err(|_| "expected a board uuid".to_owned())
}

/// A bare uuid — the `track restore --id` flag. A malformed value is a
/// clap usage error (exit 2), the `parse_board_id` precedent: the id
/// must be typed before any dispatch, never mid-assembly.
fn parse_uuid_flag(raw: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(raw).map_err(|_| "expected a uuid".to_owned())
}

#[test]
fn boards_move_demands_exactly_one_direction() {
    // The collapsed grammar (D-1 of the cli-verb-grammar-hygiene
    // plan): exactly one of `--to-files` / `--undo` /
    // `--to-connection <name>`; every pairing is a typed clap
    // conflict; `--to-deck` no longer exists (it never admitted a
    // working invocation, so no deprecation shim).
    use clap::Parser as _;
    use clap::error::ErrorKind;
    const BOARD: &str = "0193b400-0000-7c89-ab01-2345abcdef01";
    let parse = |flags: &[&str]| {
        let mut argv = vec!["taskboard", "boards", "move", BOARD];
        argv.extend_from_slice(flags);
        Cli::try_parse_from(argv)
    };
    for ok in [
        vec!["--to-files"],
        vec!["--undo"],
        vec!["--to-connection", "alpha"],
    ] {
        assert!(parse(&ok).is_ok(), "{ok:?} must parse");
    }
    assert!(parse(&[]).is_err(), "the group is required");
    for conflict in [
        vec!["--to-files", "--undo"],
        vec!["--to-files", "--to-connection", "alpha"],
        vec!["--undo", "--to-connection", "alpha"],
    ] {
        let err = parse(&conflict).expect_err("two directions refuse");
        assert!(
            matches!(err.kind(), ErrorKind::ArgumentConflict),
            "{conflict:?}: {:?}",
            err.kind()
        );
    }
    let dead = parse(&["--to-deck"]).expect_err("the flag was deleted");
    assert!(matches!(dead.kind(), ErrorKind::UnknownArgument));
}

#[test]
fn parse_board_id_accepts_uuids_and_rejects_garbage() {
    let good = "00000000-0000-0000-0000-000000000001";
    assert_eq!(
        parse_board_id(good),
        Ok(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(good).unwrap()
        ))
    );
    for bad in ["", "not-a-uuid", "3"] {
        assert!(parse_board_id(bad).is_err(), "{bad} must be rejected");
    }
}

// The `Color::parse` corpus lives in the domain (the contract moved
// with the grammar — one behavior, one test, lowest layer); the CLI
// keeps only the wrapper's mapping (tag-color plan D-2).
#[test]
fn parse_color_maps_the_domain_mint_onto_usage_errors() {
    // One accepted case: the wrapper unwraps the minted, normalized hex.
    assert_eq!(parse_color("31CC7C"), Ok("31cc7c".to_owned()));
    // The typed domain refusal maps onto the clap usage error.
    assert!(parse_color("#fff").is_err());
}

/// `--due` grammar: set (RFC-3339 with an explicit offset, normalized
/// to UTC), a bare `YYYY-MM-DD` (minted at the execution seam — see
/// [`DueSpec::Date`]), or clear (`none`). Due dates are stored and
/// displayed in UTC only; offset-less timestamps are refused.
#[derive(Debug, Clone, PartialEq)]
pub enum DueSpec {
    /// Set the due date (an explicit-offset instant, normalized to UTC).
    Set(chrono::DateTime<chrono::Utc>),
    /// A bare `YYYY-MM-DD` date — the date-only form of the N-4
    /// stamping law (task-detail-editor UX plan US-1/D-5; #893
    /// supersedes the mint). clap's value parser is pure (no config
    /// access), so the *mint* happens at the `tasks edit` execution
    /// seam: `day.day_last_instant(date)` over
    /// `AppConfig::resolved_day_calendar()` — the identical instant
    /// the GUI funnel's `parse_stamp` produces (one law, two fronts).
    Date(chrono::NaiveDate),
    /// Clear the due date.
    Clear,
}

/// Parses the `--due` value: RFC-3339 with an explicit offset
/// (normalized to UTC), a bare `YYYY-MM-DD` (midnight UTC), or the
/// literal `none`. Due dates are stored and displayed in UTC only;
/// offset-less timestamps are refused. A malformed value is a clap
/// usage error (exit 2).
///
/// Deliberate leniency boundary: chrono's `%Y-%m-%d` parser accepts
/// non-padded day/month fields, so `2026-2-3` parses as the same
/// [`chrono::NaiveDate`] as `2026-02-03` — the bare date is committed
/// in its canonical padded form. Non-padded input is neither refused
/// nor echoed verbatim (the D-4 decision of the cli-verb-grammar-hygiene
/// plan; the same documented leniency as the GUI `due_edit` grammar).
fn parse_due(raw: &str) -> Result<DueSpec, String> {
    if raw == "none" {
        return Ok(DueSpec::Clear);
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| DueSpec::Set(dt.with_timezone(&chrono::Utc)))
        .or_else(|_| {
            // The bare date is minted at the execution seam
            // (`DueSpec::Date`'s doc) — the value parser stays pure.
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map(DueSpec::Date)
        })
        .map_err(|_| "expected RFC-3339 (2026-10-24T09:00:00Z), YYYY-MM-DD, or 'none'".to_owned())
}

/// `--effort` grammar: set (the domain text form) or clear (`none`) —
/// the [`DueSpec`] shape, because clap's derive wraps `Option<Option<T>>`
/// one layer too many for a wrapper returning `Option`.
#[derive(Debug, Clone, PartialEq)]
pub enum EffortSpec {
    /// Set the effort.
    Set(taskboard_domain::Effort),
    /// Clear the effort.
    Clear,
}

/// `--importance` grammar: set (a vocabulary level) or clear (`none`).
#[derive(Debug, Clone, PartialEq)]
pub enum ImportanceSpec {
    /// Set the level.
    Set(taskboard_domain::Importance),
    /// Clear the level.
    Clear,
}

/// `--urgency` grammar: set (a vocabulary level) or clear (`none`) —
/// distinct from [`ImportanceSpec`] by the ruled type separation.
#[derive(Debug, Clone, PartialEq)]
pub enum UrgencySpec {
    /// Set the level.
    Set(taskboard_domain::Urgency),
    /// Clear the level.
    Clear,
}

/// `--kind` grammar: set (`standard`|`tracker`) or reset (`none`) —
/// the same set-or-clear shape as the other app-lane field specs.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskKindSpec {
    /// Set the kind.
    Set(taskboard_domain::entities::TaskKind),
    /// Reset to the `Standard` default.
    Clear,
}

/// Parses the `--effort` value over the domain text form
/// ([`taskboard_domain::Effort::parse_text_form`]): bare digits are
/// minutes, `s`/`m`/`h` components compose (`1h30m`). `none` clears;
/// anything unparsable is a clap usage error (exit 2).
fn parse_effort(raw: &str) -> Result<EffortSpec, String> {
    if raw == "none" {
        return Ok(EffortSpec::Clear);
    }
    taskboard_domain::Effort::parse_text_form(raw)
        .map(EffortSpec::Set)
        .ok_or_else(|| {
            "expected <n><unit> components (s|m|h; bare digits = minutes), e.g. 30 or 1h30m, or 'none'"
                .to_owned()
        })
}

/// Parses `--importance` over the domain's hyphenated level vocabulary;
/// `none` clears. Anything unmatched is a clap usage error (exit 2).
fn parse_importance(raw: &str) -> Result<ImportanceSpec, String> {
    if raw == "none" {
        return Ok(ImportanceSpec::Clear);
    }
    taskboard_domain::Importance::parse_text_form(raw)
        .map(ImportanceSpec::Set)
        .ok_or_else(|| "expected very-low|low|normal|high|very-high|none".to_owned())
}

/// Parses `--urgency` over the domain's hyphenated level vocabulary;
/// `none` clears. Anything unmatched is a clap usage error (exit 2).
fn parse_urgency(raw: &str) -> Result<UrgencySpec, String> {
    if raw == "none" {
        return Ok(UrgencySpec::Clear);
    }
    taskboard_domain::Urgency::parse_text_form(raw)
        .map(UrgencySpec::Set)
        .ok_or_else(|| "expected very-low|low|normal|high|very-high|none".to_owned())
}

/// Parses `--kind` over the domain serde unit names (`standard`|
/// `tracker`; `none` resets to the default — the domain enum itself
/// carries no invariant, the parse is the boundary rejection).
/// Anything unmatched is a clap usage error (exit 2).
fn parse_task_kind_flag(raw: &str) -> Result<TaskKindSpec, String> {
    if raw == "none" {
        return Ok(TaskKindSpec::Clear);
    }
    taskboard_domain::entities::TaskKind::parse_text_form(raw)
        .map(TaskKindSpec::Set)
        .ok_or_else(|| "expected standard|tracker|none".to_owned())
}

/// Parses `--insistence` over the reminder rung vocabulary
/// (`normal`|`alarm`; no `none` — a rule is created at one rung, the
/// default folds at the collapse boundary). Anything unmatched is a
/// clap usage error (exit 2), the `parse_urgency` shape.
fn parse_insistence_flag(raw: &str) -> Result<taskboard_domain::Insistence, String> {
    taskboard_domain::Insistence::parse_text_form(raw)
        .ok_or_else(|| "expected normal|alarm".to_owned())
}

/// Parses `--kind` over the time-entry kind vocabulary (`work` |
/// `respite`). Anything unmatched is a clap usage error (exit 2) — a
/// boundary parse rejection, never a domain error variant (the
/// `parse_insistence_flag` shape).
fn parse_kind_flag(raw: &str) -> Result<taskboard_domain::tracking::TimeEntryKind, String> {
    taskboard_domain::tracking::TimeEntryKind::parse_text_form(raw)
        .ok_or_else(|| "expected work|respite".to_owned())
}

/// The `breaks add --after` flag: the same `<int><unit>` grammar minted
/// straight into the domain newtype — a zero or negative total is a clap
/// usage error (exit 2) at this boundary, never a downstream panic (the
/// `parse_kind_flag` shape).
fn parse_after_flag(raw: &str) -> Result<taskboard_domain::WorkThreshold, String> {
    taskboard_domain::WorkThreshold::parse_text_form(raw).map_err(|_| {
        "expected a positive <int><unit> duration (s|m|h|d), e.g. 90m or 1h30m".to_owned()
    })
}

/// The `tasks remind --before` flag: the same `[reminders]
/// default_before` grammar ([`crate::config::parse_before`]). A
/// malformed value is a clap usage error (exit 2), the `parse_due`
/// precedent.
fn parse_before_flag(raw: &str) -> Result<chrono::TimeDelta, String> {
    crate::config::parse_before(raw)
        .ok_or_else(|| "expected <int><unit> components (s|m|h|d), e.g. 10m or 1h30m".to_owned())
}

/// The `--weekly` value: the empty default-missing value is the bare
/// flag's task-weekday default; anything else goes through the
/// domain's weekday-set text form — one parser, one taxonomy (the
/// `parse_effort` shape).
fn parse_weekdays_flag(raw: &str) -> Result<WeekdaysFlag, String> {
    if raw.is_empty() {
        return Ok(WeekdaysFlag::TaskWeekday);
    }
    taskboard_domain::WeekdaySet::parse_text_form(raw)
        .map(WeekdaysFlag::Days)
        .map_err(|err| err.to_string())
}

/// The `--monthly` value: the empty default-missing value is the bare
/// flag's task-day default; anything else is a
/// [`taskboard_domain::MonthlyAnchor`] text form.
fn parse_monthly_flag(raw: &str) -> Result<MonthlyFlag, String> {
    if raw.is_empty() {
        return Ok(MonthlyFlag::TaskDay);
    }
    taskboard_domain::MonthlyAnchor::parse_text_form(raw)
        .map(MonthlyFlag::Anchor)
        .map_err(|err| err.to_string())
}

/// Wraps the domain's day-interval text form (`3` | `3d`).
fn parse_every_days_flag(raw: &str) -> Result<taskboard_domain::EveryDays, String> {
    taskboard_domain::EveryDays::parse_text_form(raw).map_err(|err| err.to_string())
}

/// Wraps the domain's week-interval text form (`2` | `2w`).
fn parse_every_weeks_flag(raw: &str) -> Result<taskboard_domain::EveryWeeks, String> {
    taskboard_domain::EveryWeeks::parse_text_form(raw).map_err(|err| err.to_string())
}

/// Wraps the domain's occurrence-count text form (bare digits, ≥ 1).
fn parse_count_flag(raw: &str) -> Result<taskboard_domain::OccurrenceCount, String> {
    taskboard_domain::OccurrenceCount::parse_text_form(raw).map_err(|err| err.to_string())
}

/// The `tasks repeat --until` bound: an inclusive **local** calendar
/// date (`YYYY-MM-DD`) — a pure date, deliberately without
/// `parse_due`'s UTC-midnight semantics (a bound is not an instant).
/// Anything else is a clap usage error (exit 2).
fn parse_until_flag(raw: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| "expected a local date YYYY-MM-DD".to_owned())
}

/// The `--weekly` flag's value shape: the bare flag (the empty
/// `default_missing_value`) names the task's own weekday — resolved
/// handler-side, the calendar does not exist at parse time — and an
/// explicit value carries the weekday set.
#[derive(Debug, Clone, PartialEq)]
pub enum WeekdaysFlag {
    /// Bare `--weekly`: the task's own weekday.
    TaskWeekday,
    /// `--weekly mon,wed,fri`.
    Days(taskboard_domain::WeekdaySet),
}

/// The `--monthly` flag's value shape: the bare flag names the task's
/// own day-of-month (handler-side resolution), an explicit value is a
/// [`taskboard_domain::MonthlyAnchor`] text form.
#[derive(Debug, Clone, PartialEq)]
pub enum MonthlyFlag {
    /// Bare `--monthly`: the task's own day-of-month.
    TaskDay,
    /// `--monthly 15` | `--monthly 2nd-mon`.
    Anchor(taskboard_domain::MonthlyAnchor),
}

/// The `tasks repeat` pattern/mode flags: exactly one of the four
/// (`--daily`/`--weekly`/`--monthly` set a pattern, `--off` clears the
/// rule) — clap-enforced by the `repeat-mode` arg group (`multiple =
/// false`, `required = true`).
#[derive(Debug, Clone, clap::Args)]
#[group(id = "repeat-mode", multiple = false, required = true)]
pub struct RepeatModeFlags {
    /// Every N days (bare flag = 1).
    #[arg(
        long,
        num_args(0..=1),
        default_missing_value = "1",
        value_parser = parse_every_days_flag
    )]
    pub daily: Option<taskboard_domain::EveryDays>,
    /// On the given weekdays (`mon,wed,fri`) of every
    /// `--every-weeks`-th week; bare flag = the task's own weekday.
    /// A month never lacks a weekday — no monthly-style clamp question.
    #[arg(
        long,
        num_args(0..=1),
        default_missing_value = "",
        value_parser = parse_weekdays_flag
    )]
    pub weekly: Option<WeekdaysFlag>,
    /// Monthly on a day-of-month (`15`, clamped in short months) or an
    /// nth weekday (`2nd-mon`; a month lacking it contributes no
    /// member — legal grammar, legal rule). Bare flag = the task's own
    /// day-of-month.
    #[arg(
        long,
        num_args(0..=1),
        default_missing_value = "",
        value_parser = parse_monthly_flag
    )]
    pub monthly: Option<MonthlyFlag>,
    /// Stop repeating (clear the rule; the head stays an ordinary
    /// task). Exclusive with every set flag and bound.
    #[arg(long, conflicts_with_all = ["every_weeks", "until", "count"])]
    pub off: bool,
    /// Pause the series through this inclusive local date (issue #506):
    /// the covered members do not exist — no miss, no credit. Exclusive
    /// with every set flag and bound.
    #[arg(
        long,
        conflicts_with_all = ["every_weeks", "until", "count"],
        value_parser = parse_until_flag
    )]
    pub pause: Option<chrono::NaiveDate>,
    /// Resume a paused series (clear the pause; the badge's dead-pause
    /// chip goes with it). Exclusive with every set flag and bound.
    #[arg(long, conflicts_with_all = ["every_weeks", "until", "count"])]
    pub resume: bool,
}

/// The `tasks repeat` flags (the pattern family above, plus bounds) —
/// a flattened `Args` struct so the bounds can require the pattern by
/// the group id.
#[derive(Debug, Clone, clap::Args)]
pub struct RepeatArgs {
    /// The task (uuid | unique ≥8-char uuid prefix | exact title).
    pub task: String,
    /// Exactly one of `--daily`/`--weekly`/`--monthly`/`--off`.
    #[command(flatten)]
    pub mode: RepeatModeFlags,
    /// The weekly interval (≥ 1); requires `--weekly`.
    #[arg(
        long,
        requires = "weekly",
        conflicts_with_all = ["daily", "monthly", "pause", "resume"],
        value_parser = parse_every_weeks_flag
    )]
    pub every_weeks: Option<taskboard_domain::EveryWeeks>,
    /// No member after this inclusive local date; requires a pattern.
    #[arg(
        long,
        requires = "repeat-mode",
        conflicts_with_all = ["count", "pause", "resume"],
        value_parser = parse_until_flag
    )]
    pub until: Option<chrono::NaiveDate>,
    /// At most N completed occurrences (≥ 1); requires a pattern.
    #[arg(
        long,
        requires = "repeat-mode",
        conflicts_with_all = ["until", "pause", "resume"],
        value_parser = parse_count_flag
    )]
    pub count: Option<taskboard_domain::OccurrenceCount>,
}

/// The `timers add --at` flag (D-R9's host-local ruling, applied at the
/// CLI edge — this is the only place the app reads `chrono::Local`).
/// Accepted, in order: (1) full RFC-3339 with offset → absolute instant;
/// (2) `YYYY-MM-DD` → that date's midnight **host-local**, resolved to
/// UTC (deliberately diverging from `parse_due`'s UTC bare-date reading:
/// a wake-up alarm is a wall-clock promise); (3) `HH:MM[:SS]` → today
/// host-local at that time, rolled to **tomorrow** when already past (a
/// "09:30" set at 22:00 means tomorrow 09:30). DST-ambiguous local
/// times resolve to one offset (chrono's local resolution); the alarm
/// is absolute thereafter. Anything else is a clap usage error
/// (exit 2), the `parse_due` failure shape.
fn parse_at_flag(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    use chrono::TimeZone as _;
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let midnight = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
        return chrono::Local
            .from_local_datetime(&midnight)
            .single()
            .map(|local| local.with_timezone(&chrono::Utc))
            .ok_or_else(|| "that local midnight does not resolve to one instant".to_owned());
    }
    let time = chrono::NaiveTime::parse_from_str(raw, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(raw, "%H:%M:%S"));
    if let Ok(time) = time {
        let today = chrono::Local::now().date_naive();
        let mut local = chrono::Local
            .from_local_datetime(&today.and_time(time))
            .single()
            .ok_or_else(|| "that local time does not resolve to one instant".to_owned())?;
        if local <= chrono::Local::now() {
            let tomorrow = today.succ_opt().expect("tomorrow exists");
            local = chrono::Local
                .from_local_datetime(&tomorrow.and_time(time))
                .single()
                .ok_or_else(|| "that local time does not resolve to one instant".to_owned())?;
        }
        return Ok(local.with_timezone(&chrono::Utc));
    }
    Err("expected RFC-3339 (2026-10-24T09:00:00Z), YYYY-MM-DD, or HH:MM[:SS]".to_owned())
}

/// `timers` subcommands: the standalone alarm family (D-R9).
#[derive(Debug, Subcommand)]
pub enum TimersCmd {
    /// Create a standalone alarm: fires once at the stamped instant,
    /// then dies by settlement. Exactly one of `--in`/`--at`.
    Add {
        /// The notification label (no task to snapshot a title from).
        /// The clap id is `timer_label` — `label` is the task-label
        /// selector positional's id, and the completion walk types by
        /// id (a collision would offer task-label titles here).
        #[arg(value_name = "label")]
        timer_label: String,
        /// Fire this long after now (`30s|5m|1h30m|2d`); must be
        /// positive. Exactly one of `--in`/`--at`.
        #[arg(
            long,
            name = "in",
            required_unless_present = "at",
            conflicts_with = "at",
            value_parser = parse_before_flag
        )]
        in_: Option<chrono::TimeDelta>,
        /// Fire at this instant (RFC-3339 | YYYY-MM-DD | HH:MM[:SS]
        /// host-local; a bare time already past means tomorrow).
        /// Exactly one of `--in`/`--at`.
        #[arg(
            long,
            required_unless_present = "in",
            conflicts_with = "in",
            value_parser = parse_at_flag
        )]
        at: Option<chrono::DateTime<chrono::Utc>>,
        /// How loudly the alarm insists (`normal|alarm`).
        #[arg(long, value_parser = parse_insistence_flag)]
        insistence: Option<taskboard_domain::Insistence>,
    },
    /// List the standalone alarms in canonical `(fire_at, id)` order
    /// (pure read).
    List,
    /// Cancel a standalone alarm (uuid | unique ≥8-char uuid prefix |
    /// unique case-folded label prefix). Cancel deletes the row, so a
    /// repeat is a typed unknown-alarm refusal.
    Cancel {
        /// The alarm selector.
        selector: String,
    },
}

/// `breaks` subcommands (#495): the break-rule family, the `timers`
/// recipe over duration triggers.
#[derive(Debug, Subcommand)]
pub enum BreaksCmd {
    /// Create a break rule: fires when accumulated tracked work since
    /// the last respite-marked entry reaches `--after` (the crossing
    /// instant is the occurrence; a respite re-arms).
    Add {
        /// The accumulated-work threshold (`30s|5m|1h30m|2d`); strictly
        /// positive — a zero or negative value is a usage error here.
        #[arg(long, value_parser = parse_after_flag)]
        after: taskboard_domain::WorkThreshold,
        /// The notification title. Defaults to the threshold phrase.
        #[arg(long)]
        label: Option<String>,
        /// How loudly the reminder insists (`normal|alarm`).
        #[arg(long, value_parser = parse_insistence_flag)]
        insistence: Option<taskboard_domain::Insistence>,
        /// The board the rule roams with (uuid); the single live board
        /// when omitted (#588 — break rules roam via the board's
        /// synced `break-rules` section).
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// List the break rules in canonical `(threshold, id)` order (pure
    /// read).
    List,
    /// Remove a break rule (uuid | unique ≥8-char uuid prefix | unique
    /// case-folded label prefix). Removal deletes the row and prunes
    /// its deliveries/dismissals/snooze links, so a repeat is a typed
    /// unknown-rule refusal (the `timers cancel` law).
    Remove {
        /// The rule selector.
        selector: String,
    },
}

/// Which data family `export` emits (D-J12). `Tracking` is the
/// default, so bare `export` compiles to today's code path untouched
/// (the byte-compat ruling).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ExportWhat {
    /// The time entries (the default; the D-T15 payload).
    #[default]
    Tracking,
    /// The live reflection entries (the ruled eleven columns;
    /// tombstones are bookkeeping and stay out).
    #[value(name = "journal")]
    Reflection,
    /// The printable today sheet (`export --what pdf`, #624 stage B):
    /// the export family's explicit file-writing leg. Compiled only
    /// with the `pdf-export` feature — a default build rejects
    /// `--what pdf` as a clap usage error (exit 2), so there is no
    /// runtime "feature disabled" path.
    #[cfg(feature = "pdf-export")]
    Pdf,
}

/// The export payload format. `Csv` is the default; a malformed value
/// is a clap usage error (exit 2) — the `FindType` `ValueEnum`
/// precedent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ExportFormat {
    /// RFC-4180 CSV with a header row (the default).
    #[default]
    Csv,
    /// A single-line JSON array of entry objects (`jq` is the
    /// consumer).
    Json,
}

/// The print layout of `export --what pdf` (feature-gated; the clap
/// usage error for a default build rides `--what pdf`'s absence).
#[cfg(feature = "pdf-export")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum PdfLayout {
    /// The foldable A4 sheet → four-page A6 booklet (the default).
    #[default]
    Fold,
    /// Single-sided A4, two columns.
    A4,
    /// Single-sided A5.
    A5,
    /// Single-sided A6.
    A6,
}

/// Span-stamp grammar (the `parse_due` shape minus `none`): RFC-3339
/// with an explicit offset (normalized to UTC) or bare `YYYY-MM-DD`
/// (that date's midnight UTC). No `none` — a span boundary is never
/// "clear" (there is no payload that reopens an entry, ADR 0017) — and
/// no bare `HH:MM` (D-TC7: a time-only boundary is day-ambiguous; its
/// wake-up-rollover reading is exactly wrong for retroactive history).
fn parse_span_stamp(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map(|date| {
                date.and_hms_opt(0, 0, 0)
                    .expect("midnight is valid")
                    .and_utc()
            })
        })
        .map_err(|_| {
            "expected RFC-3339 with an offset (2026-10-24T09:00:00Z) or YYYY-MM-DD".to_owned()
        })
}

/// Strict `YYYY-MM-DD` — the UTC-day filter grammar of `track list
/// --since` and `export --since/--until`. A malformed value is a clap
/// usage error (exit 2).
fn parse_date_flag(raw: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| "expected YYYY-MM-DD".to_owned())
}

/// A positive quota (`--daily/--weekly/--monthly [N]`): zero-quota is
/// unrepresentable in the domain, so the parse rejects it as a usage
/// error (the mint's law surfaced at the CLI boundary).
fn parse_quota_flag(raw: &str) -> Result<u32, String> {
    let n: u32 = raw.parse().map_err(|_| "expected a count".to_owned())?;
    if n == 0 {
        return Err("the count must be at least 1".to_owned());
    }
    Ok(n)
}

/// `HH:MM[:SS]` — the anchor-chip grammar.
fn parse_time_of_day_flag(raw: &str) -> Result<chrono::NaiveTime, String> {
    chrono::NaiveTime::parse_from_str(raw, "%H:%M:%S")
        .or_else(|_| chrono::NaiveTime::parse_from_str(raw, "%H:%M"))
        .map_err(|_| "expected HH:MM or HH:MM:SS".to_owned())
}

/// `--at <HH:MM[:SS]>` / `--at none` — the chip's change-mask value.
fn parse_anchor_edit(raw: &str) -> Result<AnchorEdit, String> {
    if raw.eq_ignore_ascii_case("none") {
        return Ok(AnchorEdit::Clear);
    }
    parse_time_of_day_flag(raw).map(AnchorEdit::Set)
}

/// `--description <text>` / `--description none` — the description
/// change-mask value (`none` records the empty string; the
/// `TaskChanges.description` shape). Infallible, but clap's
/// `value_parser` seam keeps the `Result` shape.
#[allow(clippy::unnecessary_wraps)] // clap's value_parser fn shape
fn parse_habit_description(raw: &str) -> Result<String, String> {
    if raw.eq_ignore_ascii_case("none") {
        return Ok(String::new());
    }
    Ok(raw.to_owned())
}

/// `--day <mon..sun | 1..31>` / `--day none` — the anchor day's
/// change-mask value. A bare 1..31 is unambiguous at this layer (the
/// weekday names never parse as integers), so the variant choice is
/// total; the KIND match against the habit's cadence is checked
/// against the concurrent target flags (the typed refusal), never
/// guessed here.
fn parse_day_edit(raw: &str) -> Result<DayEdit, String> {
    if raw.eq_ignore_ascii_case("none") {
        return Ok(DayEdit::Clear);
    }
    let weekdays = [
        ("mon", chrono::Weekday::Mon),
        ("tue", chrono::Weekday::Tue),
        ("wed", chrono::Weekday::Wed),
        ("thu", chrono::Weekday::Thu),
        ("fri", chrono::Weekday::Fri),
        ("sat", chrono::Weekday::Sat),
        ("sun", chrono::Weekday::Sun),
    ];
    let lower = raw.to_lowercase();
    for (name, weekday) in weekdays {
        if lower == name || lower == format!("{weekday:?}").to_lowercase() {
            return Ok(DayEdit::Set(taskboard_domain::habit::AnchorDay::Weekday(
                weekday,
            )));
        }
    }
    let n: u8 = raw
        .parse()
        .map_err(|_| "expected mon..sun, 1..31, or 'none'".to_owned())?;
    match taskboard_domain::habit::MonthDay::new(n) {
        Ok(day) => Ok(DayEdit::Set(taskboard_domain::habit::AnchorDay::MonthDay(
            day,
        ))),
        Err(_) => Err("the month day must be 1..=31".to_owned()),
    }
}

/// `--continued-from <series uuid>` / `--continued-from none`.
/// The `habits update --days` value (HF4): `all` is the full
/// seven-day set (the clear posture); anything else is a comma list
/// of weekday names, deduplicated, never empty. The empty list and an
/// unknown name are typed refusals at parse time (the trust boundary —
/// the mint's law made visible).
fn parse_days_flag(raw: &str) -> Result<taskboard_domain::habit::WeekdaySet, String> {
    if raw.eq_ignore_ascii_case("all") {
        return Ok(taskboard_domain::habit::WeekdaySet::all());
    }
    let days = raw
        .split(',')
        .map(|name| {
            let name = name.trim().to_ascii_lowercase();
            match name.as_str() {
                "mo" | "mon" | "monday" => Ok(chrono::Weekday::Mon),
                "tu" | "tue" | "tuesday" => Ok(chrono::Weekday::Tue),
                "we" | "wed" | "wednesday" => Ok(chrono::Weekday::Wed),
                "th" | "thu" | "thursday" => Ok(chrono::Weekday::Thu),
                "fr" | "fri" | "friday" => Ok(chrono::Weekday::Fri),
                "sa" | "sat" | "saturday" => Ok(chrono::Weekday::Sat),
                "su" | "sun" | "sunday" => Ok(chrono::Weekday::Sun),
                other => Err(format!("unknown weekday `{other}`")),
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    taskboard_domain::habit::WeekdaySet::new(days).map_err(|_| {
        String::from("the empty day set is unmintable — name at least one weekday, or `all`")
    })
}

/// The `habits update --reminder` value (HF4): comma-separated
/// `key=value` segments; omitted keys take the defaults (`base`
/// nudge-day, `offset` 0, no `time` — the #1212 anchor-time-only
/// posture; a named `time=` mints `Some` — never the retired 09:00
/// fallback, `snooze` 1 hour). Unknown keys, a duplicated key, or an
/// unparseable value is a typed refusal.
fn parse_reminder_override(
    raw: &str,
) -> Result<taskboard_domain::habit::HabitReminderOverride, String> {
    use taskboard_domain::habit::{HabitReminderOverride, HabitSnooze, NudgeBase};
    let mut base = NudgeBase::NudgeDay;
    let mut offset = 0_i32;
    let mut time = None;
    let mut snooze = HabitSnooze::Hours(1);
    let mut seen = std::collections::BTreeSet::<String>::new();
    let insert =
        |key: &'static str, seen: &mut std::collections::BTreeSet<String>| -> Result<(), String> {
            if !seen.insert(String::from(key)) {
                return Err(format!("duplicated key `{key}`"));
            }
            Ok(())
        };
    for segment in raw.split(',') {
        let (key, value) = segment
            .split_once('=')
            .ok_or_else(|| format!("segment `{segment}` is not key=value"))?;
        let value = value.trim();
        match key.trim() {
            "base" => {
                insert("base", &mut seen)?;
                base = match value {
                    "nudge-day" => NudgeBase::NudgeDay,
                    "period-start" => NudgeBase::PeriodStart,
                    other => return Err(format!("unknown base `{other}`")),
                };
            }
            "offset" => {
                insert("offset", &mut seen)?;
                offset = value
                    .parse::<i32>()
                    .map_err(|_| format!("offset `{value}` is not an integer day count"))?;
            }
            "time" => {
                insert("time", &mut seen)?;
                time = Some(parse_time_of_day_flag(value)?);
            }
            "snooze" => {
                insert("snooze", &mut seen)?;
                let digits = value.trim_end_matches(['h', 'H', 'd', 'D']);
                let n: u8 = digits
                    .parse()
                    .map_err(|_| format!("snooze `{value}` is not a count"))?;
                snooze = if value.ends_with('h') || value.ends_with('H') {
                    HabitSnooze::hours(n).ok_or_else(|| {
                        format!(
                            "snooze `{value}` is degenerate — a zero-duration \
                                 snooze moves the occurrence by zero; use a count \
                                 of at least 1"
                        )
                    })?
                } else if value.ends_with('d') || value.ends_with('D') {
                    HabitSnooze::days(n).ok_or_else(|| {
                        format!(
                            "snooze `{value}` is degenerate — a zero-duration \
                                 snooze moves the occurrence by zero; use a count \
                                 of at least 1"
                        )
                    })?
                } else {
                    return Err(format!("snooze `{value}` needs an `h` or `d` unit"));
                };
            }
            other => return Err(format!("unknown key `{other}`")),
        }
    }
    Ok(HabitReminderOverride {
        base,
        offset_days: offset,
        time,
        snooze,
    })
}

fn parse_seam_edit(raw: &str) -> Result<SeamEdit, String> {
    if raw.eq_ignore_ascii_case("none") {
        return Ok(SeamEdit::Unlink);
    }
    let uuid =
        uuid::Uuid::parse_str(raw).map_err(|_| "expected a series uuid or 'none'".to_owned())?;
    Ok(SeamEdit::Link(taskboard_domain::SeriesId::from(uuid)))
}

#[test]
fn parse_span_stamp_normalizes_to_utc_and_rejects_bare_times() {
    let stamped = parse_span_stamp("2026-10-24T11:00:00+02:00").expect("explicit offset");
    let expected = chrono::DateTime::parse_from_rfc3339("2026-10-24T09:00:00Z")
        .expect("the same instant in UTC")
        .with_timezone(&chrono::Utc);
    assert_eq!(stamped, expected, "the offset normalizes to UTC");
    let bare = parse_span_stamp("2026-10-24").expect("bare date");
    let midnight = chrono::NaiveDate::from_ymd_opt(2026, 10, 24)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("valid midnight")
        .and_utc();
    assert_eq!(bare, midnight, "a bare date is that midnight UTC");
    // No `none`, no bare time-of-day: both are usage errors.
    for bad in ["none", "09:30", "tomorrow", "", "2026-10-24T09:00:00"] {
        assert!(parse_span_stamp(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn parse_days_flag_mints_lists_and_all_and_refuses_empty() {
    use taskboard_domain::habit::WeekdaySet;
    let days = parse_days_flag("mo,we,fr").expect("three days");
    assert_eq!(
        days,
        WeekdaySet::new([
            chrono::Weekday::Mon,
            chrono::Weekday::Wed,
            chrono::Weekday::Fri
        ])
        .expect("three days")
    );
    assert_eq!(parse_days_flag("all").expect("all"), WeekdaySet::all());
    // The mint's law at the boundary: the empty list is a typed
    // refusal, never an inert mint.
    for bad in ["", ",", "monday-morning"] {
        assert!(parse_days_flag(bad).is_err(), "{bad} must be rejected");
    }
    let only_unknown = parse_days_flag(",");
    assert!(only_unknown.is_err(), "no weekday named");
}

#[test]
fn parse_reminder_override_refuses_zero_snooze_at_the_mint() {
    use taskboard_domain::habit::HabitSnooze;
    // The typed refusal (#1211): `snooze=0h`/`snooze=0d` is a
    // degenerate snooze — the parse refuses, it never mints a zero
    // duration. Structural outcome only (no message text).
    for bad in ["snooze=0h", "snooze=0d", "snooze=0H", "snooze=0D"] {
        assert!(
            parse_reminder_override(bad).is_err(),
            "{bad} must be rejected by the mint"
        );
    }
    // The mint's own law backs the refusal (the boundary consumes it).
    assert_eq!(HabitSnooze::hours(0), None);
    assert_eq!(HabitSnooze::days(0), None);
    // Positive counts keep minting through the same seam.
    assert_eq!(
        parse_reminder_override("snooze=1h")
            .expect("one hour")
            .snooze,
        HabitSnooze::Hours(1)
    );
}

#[test]
fn parse_reminder_override_defaults_fill_and_refusals_are_typed() {
    use taskboard_domain::habit::{HabitSnooze, NudgeBase};
    let ovr = parse_reminder_override("base=period-start,offset=-1,time=07:30,snooze=1d")
        .expect("full grammar");
    assert_eq!(ovr.base, NudgeBase::PeriodStart);
    assert_eq!(ovr.offset_days, -1);
    assert_eq!(
        ovr.time,
        Some(chrono::NaiveTime::from_hms_opt(7, 30, 0).expect("valid time"))
    );
    assert_eq!(ovr.snooze, HabitSnooze::Days(1));
    // Defaults: a bare `snooze=45m`? No — hours/days only (O-1); the
    // defaults fill the omitted keys.
    let minimal = parse_reminder_override("snooze=2h").expect("minimal");
    assert_eq!(minimal.base, NudgeBase::NudgeDay, "R-8 default base");
    assert_eq!(minimal.offset_days, 0);
    assert_eq!(minimal.snooze, HabitSnooze::Hours(2));
    // #1212 (operator ruling comment 7079171625): an omitted `time=`
    // mints `None` — the anchor-time-only posture; the retired 09:00
    // fallback is never filled in. A named time mints `Some`.
    assert_eq!(minimal.time, None, "blank = anchor-time-only");
    for bad in [
        "snooze=45m",
        "base=tuesday",
        "offset=",
        "base=nudge-day,base=period-start",
        "wat",
    ] {
        assert!(
            parse_reminder_override(bad).is_err(),
            "{bad} must be rejected"
        );
    }
}

#[test]
fn parse_date_flag_is_strict_ymd() {
    let date = parse_date_flag("2026-10-24").expect("ymd");
    assert_eq!(chrono::NaiveDate::from_ymd_opt(2026, 10, 24), Some(date));
    for bad in ["none", "2026-10-24T09:00:00Z", "24.10.2026", ""] {
        assert!(parse_date_flag(bad).is_err(), "{bad} must be rejected");
    }
}

/// `track` subcommands: the time-tracking family (D-T14).
#[derive(Debug, Subcommand)]
pub enum TrackCmd {
    /// Start tracking a live task: whatever runs closes at the
    /// engine-stamped now, a fresh open entry is minted (title
    /// snapshotted). Idempotent when this task is already tracked.
    Start {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// What the open entry counts as (`work|respite`): `--kind
        /// respite` opens the *running break*. `work` when omitted.
        #[arg(long, value_parser = parse_kind_flag)]
        kind: Option<taskboard_domain::tracking::TimeEntryKind>,
    },
    /// Close every open entry at the engine-stamped now. Nothing open
    /// is the typed no-op.
    Stop,
    /// The open entries (newest-first) plus the tracked-today total
    /// over the `[preferences] day_start_time` window — spanning
    /// entries contribute only their in-window part (the pure
    /// projection; rows are never physically split).
    Status,
    /// Time entries in canonical `(start, id)` order.
    List {
        /// Only entries of this live task.
        #[arg(long)]
        task: Option<String>,
        /// Only entries starting on/after this UTC date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        since: Option<chrono::NaiveDate>,
    },
    /// Log a retroactive, closed block: `--for <dur>` (ending now) or
    /// `--from/--to` absolute stamps — exactly one mode. An inverted
    /// or empty span is the typed `invalid-span` failure (the
    /// planner's law, not clap's).
    Log {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// Length of the block ending now (`30s|5m|1h30m|2d`); must be
        /// positive. Exactly one of `--for`/`--from --to`.
        #[arg(
            long,
            name = "for",
            required_unless_present_all = ["from", "to"],
            conflicts_with_all = ["from", "to"],
            value_parser = parse_before_flag
        )]
        for_: Option<chrono::TimeDelta>,
        /// Span start (RFC-3339 with an explicit offset, or YYYY-MM-DD
        /// = midnight UTC). Requires `--to`.
        #[arg(long, requires = "to", value_parser = parse_span_stamp)]
        from: Option<chrono::DateTime<chrono::Utc>>,
        /// Span end, strictly after `--from`. Requires `--from`.
        #[arg(long, requires = "from", value_parser = parse_span_stamp)]
        to: Option<chrono::DateTime<chrono::Utc>>,
        /// What the block counts as (`work|respite`): `--kind respite`
        /// logs a retroactive break block. `work` when omitted.
        #[arg(long, value_parser = parse_kind_flag)]
        kind: Option<taskboard_domain::tracking::TimeEntryKind>,
    },
    /// Adjust an entry's span. Each flag is a *change mask*: omitted =
    /// unchanged; there is deliberately no way to clear an end (a
    /// history edit can never resurrect a running timer, ADR 0017).
    /// A zero-flag adjust is the typed no-op.
    Adjust {
        /// The entry (uuid | unique ≥8-char uuid prefix).
        entry: String,
        /// New span start.
        #[arg(long, value_parser = parse_span_stamp)]
        start: Option<chrono::DateTime<chrono::Utc>>,
        /// New span end (stamping one on an open entry closes it).
        #[arg(long, value_parser = parse_span_stamp)]
        end: Option<chrono::DateTime<chrono::Utc>>,
        /// New kind (`work|respite`) — the retroactive marking/remarking
        /// of history. Omitted = unchanged.
        #[arg(long, value_parser = parse_kind_flag)]
        kind: Option<taskboard_domain::tracking::TimeEntryKind>,
    },
    /// Delete one entry — explicit id, no confirmation. Repeats are
    /// the typed no-op (an unknown id is *not* terminal, unlike
    /// `timers cancel`: a synced tombstone may have folded the row
    /// already).
    Delete {
        /// The entry (uuid | unique ≥8-char uuid prefix).
        entry: String,
    },
    /// Re-materialize one hard-deleted time entry from its verbatim
    /// facts (the #901 restore-family verb's first surface): whatever
    /// the flags carry is what the engine restores. A present same-id
    /// row is the typed divergence refusal; a gone owner task the
    /// typed rejection before any dispatch. The restore verbs are the
    /// recovery path — undoable via the journal pre-images (the #1187
    /// ruling): the undo line's inverse re-deletes the restored row.
    Restore {
        /// The zero-input journal-addressed restore (the undo-family
        /// amendment D-5): the journaled `DeleteTimeEntry` whose
        /// captured pre-image is the verbatim payload (full uuid |
        /// unique ≥8-char uuid prefix over the restorable deletes).
        /// Hazard (#1284): journal ids are v7 uuids, and ids minted in
        /// the same millisecond share a 12-hex-char prefix — prefer
        /// the full uuid or a prefix beyond 12 chars; an ambiguous
        /// prefix refuses typed and lists the candidate uuids.
        /// When present, the payload flags are not read and must be
        /// omitted — the flag grammar stays the out-of-band recovery
        /// path, never the undo path.
        #[arg(long, conflicts_with_all = ["task", "start", "end", "kind", "id", "entry"])]
        from: Option<String>,
        /// The restored entry's id slot: a full uuid names the payload
        /// id verbatim; a prefix is refused typed (a hard-deleted row
        /// cannot prefix-resolve — ids are the restore family's
        /// address). `--id` overrides; a fresh v7 mints when neither
        /// names one.
        entry: Option<String>,
        /// The owner task (uuid | unique ≥8-char uuid prefix | exact
        /// live title); a gone owner is the typed rejection. Not read
        /// (nor required) beside `--from`.
        #[arg(long, required_unless_present = "from")]
        task: Option<String>,
        /// Span start (RFC-3339 with an explicit offset, or
        /// YYYY-MM-DD = midnight UTC). Not read (nor required) beside
        /// `--from`.
        #[arg(
            long,
            value_parser = parse_span_stamp,
            required_unless_present = "from"
        )]
        start: Option<chrono::DateTime<chrono::Utc>>,
        /// Span end; omitted = the entry re-materializes open.
        #[arg(long, value_parser = parse_span_stamp)]
        end: Option<chrono::DateTime<chrono::Utc>>,
        /// What the entry counts as (`work|respite`); `work` when
        /// omitted.
        #[arg(long, value_parser = parse_kind_flag)]
        kind: Option<taskboard_domain::tracking::TimeEntryKind>,
        /// The restored entry's id (overrides the positional slot);
        /// a fresh v7 when neither names one.
        #[arg(long, value_parser = parse_uuid_flag)]
        id: Option<uuid::Uuid>,
    },
}

/// `habits` subcommands (D-12's six verbs). The target grammar
/// `--daily/--weekly/--monthly [N]` mirrors the `tasks repeat` family:
/// exactly one flag, the count defaulting to 1 (zero-quota is
/// unrepresentable — the [`taskboard_domain::NonZeroCount`] mint).
#[derive(Debug, Subcommand)]
pub enum HabitsCmd {
    /// Add a tracker to a board (appended last in manual order).
    Add {
        /// The display title.
        title: String,
        /// Count per day window (default 1).
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            required_unless_present_any = ["weekly", "monthly"],
            conflicts_with_all = ["weekly", "monthly"],
            value_parser = parse_quota_flag
        )]
        daily: Option<u32>,
        /// Count per ISO week (default 1).
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            conflicts_with_all = ["monthly"],
            value_parser = parse_quota_flag
        )]
        weekly: Option<u32>,
        /// Count per calendar month (default 1).
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            value_parser = parse_quota_flag
        )]
        monthly: Option<u32>,
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The display anchor chip (`HH:MM[:SS]`) — presentation only,
        /// never an ordering input.
        #[arg(long, value_parser = parse_time_of_day_flag)]
        at: Option<chrono::NaiveTime>,
        /// A free-text description; `none` records an empty one. Routed
        /// as the immediate sparse edit (the `--at` precedent).
        #[arg(long, value_parser = parse_habit_description)]
        description: Option<String>,
        /// The display/nudge anchor day (`mon`..`sun` for a weekly
        /// habit, `1`..`31` for a monthly one); `none` records unset.
        /// Routed as the immediate sparse edit (the `--at` precedent).
        #[arg(long, value_parser = parse_day_edit)]
        day: Option<DayEdit>,
    },
    /// The board's trackers in stored manual `(order, id)` order, with
    /// current-window progress and streak (the conversion seam unions a
    /// linked series' completed members).
    List {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// +1 into today's window (surplus logs are first-class). The
    /// receipt names the window the increment landed in and the
    /// post-increment count — a surplus repair is user-visible, never
    /// silent.
    Log {
        /// The habit (uuid | unique ≥8-char uuid prefix | exact live
        /// title).
        habit: String,
    },
    /// Explicit count correction for one window (`YYYY-MM-DD`, the
    /// canonical period start of the habit's current target kind). A
    /// non-canonical window is the typed `invalid-habit-window`
    /// refusal, never a clap refusal. An audited count above the quota
    /// is capped by the attribution rule when the predecessor window
    /// is unmet — the receipt reports the events actually minted
    /// (#1549: the limit is accepted; the receipt is the typed truth).
    Adjust {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
        /// The window's period-start date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        window: chrono::NaiveDate,
        /// The corrected count (0 is legal — an audited emptying).
        #[arg(long)]
        count: u32,
    },
    /// Edit a tracker's fields. Each flag is a change mask: omitted =
    /// unchanged; a zero-flag update is the typed no-op. `--order`
    /// routes `MoveHabit` (the manual reorder — this verb stores the
    /// absolute value, the engine renumbers dense).
    Update {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New per-day quota.
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            conflicts_with_all = ["weekly", "monthly"],
            value_parser = parse_quota_flag
        )]
        daily: Option<u32>,
        /// New per-week quota.
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            conflicts_with_all = ["monthly"],
            value_parser = parse_quota_flag
        )]
        weekly: Option<u32>,
        /// New per-month quota.
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            value_parser = parse_quota_flag
        )]
        monthly: Option<u32>,
        /// New anchor chip (`HH:MM[:SS]`), or `none` to clear it.
        #[arg(long, value_parser = parse_anchor_edit)]
        at: Option<AnchorEdit>,
        /// New free-text description, or `none` to clear it.
        #[arg(long, value_parser = parse_habit_description)]
        description: Option<String>,
        /// New display/nudge anchor day (`mon`..`sun` weekly, `1`..`31`
        /// monthly), or `none` to clear it. A kind mismatch against the
        /// concurrent `--weekly/--monthly` flag is the typed
        /// `habit-day-kind-mismatch` refusal; without a concurrent
        /// cadence flag the field is accepted (the domain's documented
        /// inertness answers).
        #[arg(long, value_parser = parse_day_edit)]
        day: Option<DayEdit>,
        /// The scheduled weekdays *inside* the canonical window
        /// (weekly/monthly habits only, HF4/R-6): a comma list of
        /// `mon..sun`, or `all` (the clear value — "no schedule" and
        /// "every day" are one posture). A daily target is the typed
        /// `scheduled-days-on-daily-habit` refusal at the planner —
        /// the field is a weekly/monthly extension by construction.
        #[arg(long, value_name = "MO,WE,FR|ALL", value_parser = parse_days_flag)]
        days: Option<taskboard_domain::habit::WeekdaySet>,
        /// The reminder override's fields (HF4, #959), one per `key=`
        /// segment, comma-separated: `base=nudge-day|period-start`
        /// (default `nudge-day`, R-8), `offset=N` days (may be
        /// negative), `time=HH:MM[:SS]` (the fallback time-of-day when
        /// the habit carries no `--at` chip), `snooze=NH|ND` (hours or
        /// days, O-1's grammar).
        #[arg(long, value_name = "BASE=..,OFFSET=..,TIME=..,SNOOZE=..", value_parser = parse_reminder_override)]
        reminder: Option<taskboard_domain::habit::HabitReminderOverride>,
        /// Clear the reminder override (the `Some(None)` clear leg) —
        /// the habit reverts to the plain O-1 default nudge.
        #[arg(long, conflicts_with = "reminder")]
        reminder_clear: bool,
        /// Absolute manual-order slot (the `MoveHabit` semantics).
        /// Dense-section caveat (#1356): on a densely ordered section
        /// an absolute order can only name an occupied slot and the
        /// tie resolves by id order — prefer `--before/--after`.
        #[arg(long, conflicts_with_all = ["before", "after"])]
        order: Option<i64>,
        /// Place the habit immediately BEFORE this neighbour (a habit
        /// selector) in its board section's sequence (#1356) — the
        /// deterministic relative address. The neighbour must live on
        /// the habit's board.
        #[arg(long, conflicts_with = "after")]
        before: Option<String>,
        /// Place the habit immediately AFTER this neighbour (a habit
        /// selector) in its board section's sequence (#1356).
        #[arg(long)]
        after: Option<String>,
        /// Link the §4.7 conversion seam (series uuid), or `none` to
        /// unlink. The read-side streak/history views union the link.
        #[arg(long, value_parser = parse_seam_edit)]
        continued_from: Option<SeamEdit>,
    },
    /// Delete a tracker (tombstone; the log outlives it — history stays
    /// queryable, and the receipt names the surviving row count).
    Rm {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
    },
    /// Mark the habit's current window skipped (the typed planned skip
    /// — the streak survives an intentional gap; R-4: current window
    /// only). The receipt names the marked window.
    Skip {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
    },
    /// Pause a habit through an inclusive local date (`YYYY-MM-DD`) —
    /// covered windows are neither counted nor missed and no nudges
    /// fire. A past-date through-date is the typed
    /// `invalid-habit-pause` refusal.
    Pause {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
        /// The inclusive pause-through date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        until: chrono::NaiveDate,
    },
    /// Resume a paused habit (clears the pause; badge hygiene). A
    /// not-paused habit is the accepted no-op.
    Resume {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
    },
    /// Resurrect a tombstoned habit by id (the #901 restore-family
    /// verb's first surface): pre-tombstone values come back verbatim,
    /// the log is never touched (D26). A live target is the accepted
    /// no-op; an unknown id is the typed rejection. The restore verbs
    /// are the recovery path — undoable via the journal pre-images
    /// (the #1187 ruling): undo re-deletes the tombstone.
    Restore {
        /// The habit (uuid | unique ≥8-char uuid prefix; tombstones
        /// resolve — no title leg, tombstoned rows may share titles).
        habit: String,
    },
    /// The habit's history: the canonical log windows with typed
    /// states (the grid vocabulary), chronological (HF3, US-7). A
    /// tombstoned habit's log still renders (US-7 AC-5).
    History {
        /// The habit (uuid | prefix | exact title — tombstones match
        /// too).
        habit: String,
    },
    /// The habit's statistics: adherence (met/scheduled windows),
    /// best streak, per-window totals (HF3, US-9). Empty history is
    /// the typed empty state — no 0%-of-nothing posture.
    Stats {
        /// The habit (uuid | prefix | exact title — tombstones match
        /// too).
        habit: String,
    },
}

/// The review-journal verb family (DR-7 D-J11) over the in-process
/// core: capture-at-speed `add`, `list`/`show` reads, change-mask
/// `edit`, tombstone `delete`. Device-local — never synced. (The clap
/// name is the user-facing verb; the code symbols stay `Reflection*` —
/// the D-J1 vocabulary ruling, ADR 0021.)
#[derive(Debug, Subcommand)]
pub enum ReflectionCmd {
    /// Record one reflection entry: positional text words join into the
    /// body; ratings and flags are optional. An empty line records
    /// nothing (the accepted no-op, never an error — P1
    /// capture-at-speed).
    Add {
        /// The reviewed window's cadence (`day` when omitted).
        #[arg(long, value_enum)]
        cadence: Option<CadenceSpec>,
        /// The reference date the window derives from (a mid-week day
        /// floors to its week; today when omitted).
        #[arg(long, value_parser = parse_date_flag)]
        day: Option<chrono::NaiveDate>,
        /// Mood rating, `1..=5`; an out-of-range number is the typed
        /// `invalid-rating` refusal (the domain's law, not the parse's).
        #[arg(long)]
        mood: Option<u8>,
        /// Energy rating, `1..=5`.
        #[arg(long)]
        energy: Option<u8>,
        /// A curated health flag (repeatable; set semantics dedup).
        #[arg(long, value_parser = parse_health_flag)]
        flag: Vec<taskboard_domain::reflection::HealthFlag>,
        /// The entry body's words (joined with single spaces).
        #[arg(value_name = "TEXT", num_args = 0..)]
        text_words: Vec<String>,
    },
    /// List live entries in canonical `(window_start, id)` order
    /// (pure read).
    List {
        /// Only this cadence.
        #[arg(long, value_enum)]
        cadence: Option<CadenceSpec>,
        /// Only entries whose window opens on/after this UTC date
        /// (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        since: Option<chrono::NaiveDate>,
        /// Only entries whose window opens before the day after this
        /// UTC date — inclusive of the named day (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        until: Option<chrono::NaiveDate>,
    },
    /// Show one live entry (uuid | unique ≥8-char uuid prefix).
    Show {
        /// The entry selector.
        selector: String,
    },
    /// Edit one live entry. Every flag is a *change mask*: omitted =
    /// unchanged; each value has an explicit `--clear-*` form. Flags
    /// move by delta (`--flag`/`--unflag`), never whole-set replace.
    /// A zero-flag edit is the typed no-op.
    Edit {
        /// The entry selector.
        selector: String,
        /// New body text.
        #[arg(long, conflicts_with = "clear_text")]
        text: Option<String>,
        /// Clear the body (the payload's legal empty text).
        #[arg(long)]
        clear_text: bool,
        /// New mood rating, `1..=5`.
        #[arg(long, conflicts_with = "clear_mood")]
        mood: Option<u8>,
        /// Clear the mood rating.
        #[arg(long)]
        clear_mood: bool,
        /// New energy rating, `1..=5`.
        #[arg(long, conflicts_with = "clear_energy")]
        energy: Option<u8>,
        /// Clear the energy rating.
        #[arg(long)]
        clear_energy: bool,
        /// Flags to add (repeatable).
        #[arg(long, value_parser = parse_health_flag)]
        flag: Vec<taskboard_domain::reflection::HealthFlag>,
        /// Flags to remove (repeatable).
        #[arg(long, value_parser = parse_health_flag)]
        unflag: Vec<taskboard_domain::reflection::HealthFlag>,
    },
    /// Delete one live entry — the row tombstones in place (history
    /// stays in storage; a deleted entry's selector is a typed unknown).
    Delete {
        /// The entry selector.
        selector: String,
    },
}

/// The `--cadence` values (the `FindType` `ValueEnum` precedent:
/// validation is the parse; the kebab names are the domain's serde
/// unit names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CadenceSpec {
    /// One day window.
    Day,
    /// One ISO week (Monday-start).
    Week,
    /// One calendar month.
    Month,
}

impl CadenceSpec {
    /// The domain cadence this spec names.
    #[must_use]
    pub fn cadence(self) -> taskboard_domain::reflection::ReviewCadence {
        match self {
            Self::Day => taskboard_domain::reflection::ReviewCadence::Day,
            Self::Week => taskboard_domain::reflection::ReviewCadence::Week,
            Self::Month => taskboard_domain::reflection::ReviewCadence::Month,
        }
    }
}

/// A curated health-flag name (`--flag`/`--unflag`): the mint's typed
/// rejection is consumed at the clap boundary (chunk-1 plan §3.3's
/// declared consumer) — an unknown name is a usage error (exit 2),
/// never a planner rejection.
fn parse_health_flag(raw: &str) -> Result<taskboard_domain::reflection::HealthFlag, String> {
    taskboard_domain::reflection::HealthFlag::try_new(raw)
        .map_err(|_| "unknown health flag (see the curated nine)".to_owned())
}

/// The `habits add/update --day` value: an anchor day, or `none`
/// (clear).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayEdit {
    /// Set the anchor day.
    Set(taskboard_domain::habit::AnchorDay),
    /// Clear the anchor day.
    Clear,
}

impl DayEdit {
    /// The change-mask value (`Some(None)` = clear).
    #[must_use]
    pub fn change(self) -> Option<Option<taskboard_domain::habit::AnchorDay>> {
        match self {
            Self::Set(day) => Some(Some(day)),
            Self::Clear => Some(None),
        }
    }
}

/// The `habits update --at` value: a time-of-day, or `none` (clear).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorEdit {
    /// Set the chip.
    Set(chrono::NaiveTime),
    /// Clear the chip.
    Clear,
}

/// The `habits update --continued-from` value: a series uuid, or
/// `none` (unlink).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeamEdit {
    /// Link the series.
    Link(taskboard_domain::SeriesId),
    /// Unlink.
    Unlink,
}

/// `config` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Print the effective config (secret-free by construction).
    Print,
    /// Print the resolved config read path and the connect/login write
    /// target — a side-effect-free resolution diagnostic: no decode,
    /// no core boot.
    Path,
    /// Set one or more knobs in ONE atomic write (T-18, #801; live
    /// propagation #845; batch #910). Each pair is `key value`; the
    /// whole list lands as one document — one prove verdict, one
    /// write, one apply — so either every pair is in effect or
    /// nothing is (`config set reminders.tick_secs 45
    /// focus.work_mins 50`). `key` is the dotted config key —
    /// exactly the vocabulary the GUI settings editor uses
    /// (`reminders.tick_secs`, `focus.long_break_every`,
    /// `files_sync.enabled`, ...); `value` parses per key against the
    /// same predicates the settings form applies. While the app runs,
    /// the batch applies to it immediately (and an external edit of
    /// the config file is picked up live, too); without a running
    /// app the write lands in the toml layer only and applies at the
    /// next start. Defaults and `TASKBOARD_*` env are never touched;
    /// an env-overridden key, an unknown key, a malformed value, or
    /// a duplicated key is a typed refusal naming the key — nothing
    /// writes.
    Set {
        /// An even list of dotted-key/value pairs (one pair is the
        /// single-knob form; zero pairs and odd counts are typed
        /// refusals, nothing touched).
        #[arg(value_name = "KEY VALUE", num_args = 0..)]
        pairs: Vec<String>,
    },
    /// Remove one or more knobs' overrides in ONE atomic write (T-18,
    /// #801; live propagation #845; batch #1014) — the way set values
    /// return to their defaults (or env layers). Same vocabulary,
    /// layering, atomicity, and lane behavior as `config set`: the
    /// whole key list lands as one document — one prove verdict, one
    /// write, one apply — so either every override is gone or nothing
    /// changed (`config unset reminders.tick_secs focus.work_mins`).
    /// A running app drops the overrides immediately, otherwise the
    /// removals apply at the next start. Removing an already-absent
    /// key succeeds idempotently; a zero-key invocation, an unknown
    /// key, an env-overridden key, or a duplicated key is a typed
    /// refusal naming the key — nothing writes.
    Unset {
        /// One or more dotted config keys (the settings editor's
        /// vocabulary); zero keys is a typed refusal.
        #[arg(value_name = "KEY", num_args = 0..)]
        keys: Vec<String>,
    },
    /// Reload the config file into the running app NOW (#1015) — the
    /// live-reload watcher's apply half with a human finger on the
    /// trigger. The escape hatch for a session whose watcher could not
    /// start (no inotify descriptor, an exotic filesystem): an
    /// external edit reaches the running app without a restart.
    /// Daemon-only: needs a running app (`no-daemon` otherwise — a
    /// stopped session's file is already its boot authority). Reports
    /// the same typed outcome the watcher lane reports (`applied`,
    /// `unchanged`, or the typed refusal class when the file is
    /// mid-save or unbootable; the running app keeps its
    /// last-known-good settings then).
    Reload,
}

/// The `labels edit` flags: at least one is required per line —
/// clap-enforced by the `label-edit-flags` arg group (`required =
/// true`, `multiple = true`). The positional label selector stays
/// outside the group: a variant-level group would let it satisfy the
/// requirement (`clap_derive` groups all variant arguments, positionals
/// included).
#[derive(Debug, Default, clap::Args)]
#[group(id = "label-edit-flags", required = true, multiple = true)]
pub struct LabelEditFlags {
    /// New title.
    #[arg(long)]
    pub title: Option<String>,
    /// New color (six hex digits).
    #[arg(long, value_parser = parse_color)]
    pub color: Option<String>,
}

/// `labels` subcommands.
#[derive(Debug, Subcommand)]
pub enum LabelsCmd {
    /// List a board's labels (with live-task assignment counts).
    List {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Create a label (color defaults to the Deck default when omitted).
    Add {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The label title.
        title: String,
        /// Six hex digits (no `#`); a default applies when omitted.
        #[arg(long, value_parser = parse_color)]
        color: Option<String>,
    },
    /// Edit a label (at least one flag required).
    Edit {
        /// The label (uuid | exact title).
        label: String,
        #[command(flatten)]
        flags: LabelEditFlags,
    },
    /// Delete a label (detaches it from all tasks; idempotent).
    Delete {
        /// The label (uuid | exact title).
        label: String,
    },
    /// Resurrect a tombstoned label by id (the #901 restore-family
    /// verb's first surface): pre-tombstone values come back verbatim
    /// with a fresh tombstone clock, which re-opens the membership
    /// recovery surface (`tasks label`). A live target is the accepted
    /// no-op; an unknown id is the typed rejection. The restore verbs
    /// are the recovery path — undoable via the journal pre-images
    /// (the #1187 ruling): undo re-deletes the tombstone.
    Restore {
        /// The label (uuid | unique ≥8-char uuid prefix; tombstones
        /// resolve — no title leg, tombstoned rows may share titles).
        label: String,
    },
}

/// The `tasks edit` field flags: at least one is required per line —
/// clap-enforced by the `task-edit-flags` arg group (`required =
/// true`, `multiple = true`). The positional task selector stays
/// outside the group (see [`LabelEditFlags`] for why).
#[derive(Debug, Default, clap::Args)]
#[group(id = "task-edit-flags", required = true, multiple = true)]
pub struct TaskEditFlags {
    /// New title.
    #[arg(long)]
    pub title: Option<String>,
    /// New description.
    #[arg(long)]
    pub description: Option<String>,
    /// RFC-3339 with an explicit offset (normalized to UTC),
    /// YYYY-MM-DD (midnight UTC), or `none` (clears). Due dates
    /// are stored and displayed in UTC only; offset-less
    /// timestamps are refused.
    #[arg(long, value_parser = parse_due)]
    pub due: Option<DueSpec>,
    /// Scheduled start — the `--due` vocabulary (RFC-3339,
    /// YYYY-MM-DD, or `none` clears). One parser, two flags.
    #[arg(long, value_parser = parse_due)]
    pub scheduled: Option<DueSpec>,
    /// Effort estimate — bare digits = minutes, `s`/`m`/`h`
    /// components compose (`1h30m`), `none` clears.
    #[arg(long, value_parser = parse_effort)]
    pub effort: Option<EffortSpec>,
    /// Importance level (`very-low|low|normal|high|very-high`;
    /// `none` clears).
    #[arg(long, value_parser = parse_importance)]
    pub importance: Option<ImportanceSpec>,
    /// Urgency level (`very-low|low|normal|high|very-high`; `none`
    /// clears).
    #[arg(long, value_parser = parse_urgency)]
    pub urgency: Option<UrgencySpec>,
    /// Task kind — `standard` (the completable default) or `tracker`
    /// (a tracked, never-completed card; the engine rejects `tasks
    /// done` on it); `none` resets to the default.
    #[arg(long, value_parser = parse_task_kind_flag)]
    pub kind: Option<TaskKindSpec>,
    /// Archive the task.
    #[arg(long, conflicts_with = "unarchive")]
    pub archive: bool,
    /// Unarchive the task.
    #[arg(long)]
    pub unarchive: bool,
}

/// `tasks` subcommands.
#[derive(Debug, Subcommand)]
pub enum TasksCmd {
    /// List the bound board's stacks and tasks.
    List {
        /// Restrict the listing to one stack (exact uuid or title).
        #[arg(long)]
        stack: Option<String>,
    },
    /// Create a task (appended to the end of the target stack).
    Add {
        /// The task title.
        title: String,
        /// Target stack (exact uuid or title); required to be unambiguous
        /// when omitted.
        #[arg(long)]
        stack: Option<String>,
    },
    /// Complete (or with `--undo`, reopen) a task.
    Done {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// Reopen the task instead of completing it.
        #[arg(long)]
        undo: bool,
    },
    /// Move a task to a stack and/or position (one composite intent).
    Move {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// Target stack (exact uuid or title); required to be unambiguous
        /// when omitted. Collisions across boards resolve through
        /// `--board`.
        #[arg(long)]
        stack: Option<String>,
        /// Scope the `--stack` selector to this board (exact uuid |
        /// exact title among the live boards) when stack titles
        /// collide across boards.
        #[arg(long)]
        board: Option<String>,
        /// Explicit sort order; default appends to the end of the target
        /// stack. Dense-stacks caveat (#1356): on a densely ordered
        /// stack an absolute order can only name an occupied slot and
        /// the tie resolves by id order — prefer `--before/--after`.
        #[arg(long, conflicts_with_all = ["before", "after"])]
        order: Option<i64>,
        /// Place the task immediately BEFORE this neighbour (a task
        /// selector) in the target stack's sequence (#1356) — the
        /// deterministic relative address over a densely ordered
        /// stack. The neighbour must live in the target stack.
        #[arg(long, conflicts_with = "after")]
        before: Option<String>,
        /// Place the task immediately AFTER this neighbour (a task
        /// selector) in the target stack's sequence (#1356).
        #[arg(long)]
        after: Option<String>,
    },
    /// Edit task fields (at least one flag required).
    Edit {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        #[command(flatten)]
        flags: TaskEditFlags,
    },
    /// Delete (tombstone) a task; no confirmation (scripting-first —
    /// repeats are idempotent, the tombstone is permanent).
    Delete {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
    },
    /// Attach a label to a task (idempotent when already attached).
    Label {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// The label (uuid | exact title).
        label: String,
    },
    /// Detach a label from a task (idempotent when absent).
    Unlabel {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// The label (uuid | exact title).
        label: String,
    },
    /// Manage a task's reminder rules: add one with `--before`, remove
    /// one with `--remove`, remove all with `--clear`, enumerate with
    /// `--list`, mute/unmute the task's firings with `--off`/`--on`.
    /// Exactly one mode per line.
    #[group(required = true, multiple = true)]
    Remind {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: Option<String>,
        /// The new rule's offset before due (`30s|5m|1h30m|2d`); the
        /// `[reminders] default_before` config default applies when
        /// omitted. A rule at this offset is unique (repeats are no-ops).
        /// List rendering glyphs: `!` marks overdue and `~` due-soon in
        /// `tasks list`.
        #[arg(long, requires = "task", value_parser = parse_before_flag)]
        before: Option<chrono::TimeDelta>,
        /// How loudly the new rule insists (`normal|alarm`; the default
        /// reminder stays normal). Composes with the add mode only.
        #[arg(
            long,
            requires = "task",
            value_parser = parse_insistence_flag,
            conflicts_with_all = ["clear", "remove", "off", "on", "list"]
        )]
        insistence: Option<taskboard_domain::Insistence>,
        /// List the board's reminder rules (pure read).
        #[arg(long, conflicts_with_all = ["task", "clear", "off", "on"])]
        list: bool,
        /// Remove all of the task's reminder rules.
        #[arg(long, requires = "task", conflicts_with_all = ["before", "off", "on", "list"])]
        clear: bool,
        /// Remove one of the task's rules (rule uuid | unique ≥8-char
        /// uuid prefix | the offset text form, e.g. 30m).
        #[arg(
            long,
            requires = "task",
            conflicts_with_all = ["before", "clear", "off", "on", "list"]
        )]
        remove: Option<String>,
        /// Mute all firings of the task (the overdue banner is
        /// unaffected).
        #[arg(long, requires = "task", conflicts_with_all = ["before", "clear", "on", "list"])]
        off: bool,
        /// Unmute the task's firings (inverse of `--off`).
        #[arg(long, requires = "task", conflicts_with_all = ["before", "clear", "off", "list"])]
        on: bool,
    },
    /// Manage a task's reminder rules by verb spelling (#1246, O-5
    /// parity): the verb-shaped front of the `tasks remind` flag
    /// grammar. Each verb dispatches through the same handler as its
    /// flag twin — one semantics, two fronts (the `find`/`search`
    /// alias law).
    Rules {
        #[command(subcommand)]
        cmd: RulesCmd,
    },
    /// Repeat a task on a schedule (`--daily`/`--weekly`/`--monthly`
    /// with optional `--until`/`--count` bounds) or stop repeating
    /// (`--off`). Exactly one mode per line; the anchor is the task's
    /// scheduled/duedate local date, else today.
    Repeat(RepeatArgs),
    /// Browse the journal history at the recorded bounded resolution
    /// (D1ii, #1155/#1156): the head plus the `recent` and
    /// `branch-children` windows — the exact typed rows the undo
    /// picker presents, the exact ids the `to` jump forms resolve. A
    /// read over the published snapshot; it dispatches nothing (a
    /// browse is not an intent).
    History {
        /// Machine-readable JSON against the widened `UndoLabel`
        /// projection.
        #[arg(long)]
        json: bool,
    },
    /// Undo the last change (the journal line head, ADR 0019), or with
    /// `to <id/prefix>` jump the undo line back to a `tasks history`
    /// row. Refuses typed when the line is empty, the head is not
    /// undoable, or a later write diverged. Zero flags: the line has
    /// no selectors.
    Undo {
        /// Bare (absent) = the plain reflex; `to <id/prefix>` = the
        /// bounded jump form (the picker's ancestor verb).
        #[command(subcommand)]
        mode: Option<JournalJump>,
    },
    /// Redo the last undone change (the head's newest child, ADR 0019),
    /// or with `to <id/prefix>` redo down a chosen abandoned branch.
    /// Zero flags: the line has no selectors.
    Redo {
        /// Bare (absent) = the plain reflex; `to <id/prefix>` = the
        /// bounded branch form (the picker's sibling verb).
        #[command(subcommand)]
        mode: Option<JournalJump>,
    },
}

/// The journal jump's shared `to <id/prefix>` grammar (D1ii W-6): a
/// full uuid or a unique ≥8-char prefix over the bounded windows only
/// — exactly what `tasks history` printed (D-4's honest bound; a
/// prefix beyond the windows refuses typed, never widens). Hazard
/// (#1284): journal ids are v7 uuids — ids minted in the same
/// millisecond share a 12-hex-char prefix, so prefer the full uuid or
/// a prefix beyond 12 chars; an ambiguous prefix refuses typed and
/// lists the candidate uuids.
#[derive(Debug, clap::Subcommand)]
pub enum JournalJump {
    /// The bounded jump form.
    To {
        /// The target row: full uuid | unique ≥8-char prefix over the
        /// `tasks history` windows.
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;
    use clap::error::ErrorKind as ClapErrorKind;

    #[test]
    fn zero_flag_tasks_edit_is_a_clap_usage_error() {
        let err = Cli::try_parse_from(["taskboard", "tasks", "edit", "draft"])
            .expect_err("a zero-flag edit is rejected at the parse boundary");
        assert_eq!(err.kind(), ClapErrorKind::MissingRequiredArgument);
        assert!(err.use_stderr(), "a usage error exits nonzero");
    }

    // -- the `config set` pair grammar (journey W-6, US-2/S1) -------------

    /// `us2_s1_batch_arity_parse`: the clap boundary passes the whole
    /// argv through as pairs — one pair is the single-knob form,
    /// several pairs ride as a flat list, and zero pairs reach the
    /// typed `BadArity` refusal (the grammar is even-list-shaped at
    /// the parse boundary; the even-count *law* lives one layer up in
    /// the command surface, where the refusal is typed, not a clap
    /// usage error).
    #[test]
    fn us2_s1_batch_arity_parse() {
        let cli = Cli::try_parse_from(["taskboard", "config", "set", "focus.work_mins", "50"])
            .expect("the single pair parses");
        let Some(Command::Config {
            cmd: ConfigCmd::Set { pairs },
        }) = cli.command
        else {
            panic!("a config set parses as ConfigCmd::Set");
        };
        assert_eq!(pairs, vec!["focus.work_mins".to_owned(), "50".to_owned()]);

        let cli = Cli::try_parse_from([
            "taskboard",
            "config",
            "set",
            "reminders.tick_secs",
            "45",
            "focus.work_mins",
            "50",
            "ui.due_soon_days",
            "14",
        ])
        .expect("three pairs parse as one flat list");
        let Some(Command::Config {
            cmd: ConfigCmd::Set { pairs },
        }) = cli.command
        else {
            panic!("a config set parses as ConfigCmd::Set");
        };
        assert_eq!(pairs.len(), 6, "the pairs ride the argv verbatim");

        // Zero pairs parse (empty list) so the typed refusal — not a
        // clap usage error — carries the arity law.
        let cli = Cli::try_parse_from(["taskboard", "config", "set"]).expect("zero pairs parse");
        let Some(Command::Config {
            cmd: ConfigCmd::Set { pairs },
        }) = cli.command
        else {
            panic!("a config set parses as ConfigCmd::Set");
        };
        assert!(pairs.is_empty(), "the empty list reaches the typed refusal");
    }

    #[test]
    fn zero_flag_labels_edit_is_a_clap_usage_error() {
        let err = Cli::try_parse_from(["taskboard", "labels", "edit", "urgent"])
            .expect_err("a zero-flag edit is rejected at the parse boundary");
        assert_eq!(err.kind(), ClapErrorKind::MissingRequiredArgument);
        assert!(err.use_stderr(), "a usage error exits nonzero");
    }

    // The `boards publish --to-files` parse law (US-1/S1, the R-4
    // conflict) lives in the journey Test-root
    // (`tests/cli/cli_standalone.rs::us1_s1_..`) — one behavior, one test.

    #[test]
    fn parse_due_accepts_rfc3339_bare_date_and_none() {
        let set = parse_due("2026-10-24T09:00:00Z").expect("rfc3339");
        assert!(matches!(set, DueSpec::Set(_)));
        // US-1/D-5: the bare date parses to the `Date` variant — the
        // instant mint (day-window end over the resolved day calendar)
        // happens at the execution seam, where the config is reachable.
        let bare = parse_due("2026-10-24").expect("bare date");
        assert_eq!(
            bare,
            DueSpec::Date(chrono::NaiveDate::from_ymd_opt(2026, 10, 24).expect("valid date"))
        );
        assert_eq!(parse_due("none"), Ok(DueSpec::Clear));
    }

    #[test]
    fn parse_insistence_flag_matches_the_domain_vocabulary() {
        assert_eq!(
            parse_insistence_flag("normal"),
            Ok(taskboard_domain::Insistence::Normal)
        );
        assert_eq!(
            parse_insistence_flag("alarm"),
            Ok(taskboard_domain::Insistence::Alarm)
        );
        for raw in ["critical", "wake_up", "", "ALARM"] {
            assert!(
                parse_insistence_flag(raw).is_err(),
                "{raw} must be a usage error"
            );
        }
    }

    #[test]
    fn the_insistence_flags_parse_and_respect_the_coherence_sets() {
        // Remind: the flag composes with the add mode only.
        let Cli {
            command:
                Some(Command::Tasks {
                    cmd: TasksCmd::Remind { insistence, .. },
                }),
            ..
        } = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "remind",
            "tea",
            "--before",
            "5m",
            "--insistence",
            "alarm",
        ])
        .expect("the add-mode flag parses")
        else {
            panic!("expected remind");
        };
        assert_eq!(insistence, Some(taskboard_domain::Insistence::Alarm));
        let parsed = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "remind",
            "tea",
            "--clear",
            "--insistence",
            "alarm",
        ]);
        assert!(parsed.is_err(), "--insistence conflicts with --clear");
        // Timers add: no conflicts.
        let Cli {
            command:
                Some(Command::Timers {
                    cmd: TimersCmd::Add { insistence, .. },
                }),
            ..
        } = Cli::try_parse_from([
            "taskboard",
            "timers",
            "add",
            "tea",
            "--in",
            "2m",
            "--insistence",
            "alarm",
        ])
        .expect("the timers flag parses")
        else {
            panic!("expected timers add");
        };
        assert_eq!(insistence, Some(taskboard_domain::Insistence::Alarm));
    }

    #[test]
    fn rules_clear_verb_parses_to_the_shared_handler_shape() {
        // #1246: the verb spelling of `tasks remind <task> --clear` —
        // the parse boundary proves the front exists; the handler is
        // the flag grammar's own (r5 covers its semantics).
        let Cli {
            command:
                Some(Command::Tasks {
                    cmd:
                        TasksCmd::Rules {
                            cmd: RulesCmd::Clear { task },
                        },
                }),
            ..
        } = Cli::try_parse_from(["taskboard", "tasks", "rules", "clear", "Sweep"])
            .expect("the verb spelling parses")
        else {
            panic!("expected tasks rules clear");
        };
        assert_eq!(task, "Sweep");
    }

    #[test]
    fn daemon_restart_parses() {
        // #1249: the restart arm joins the daemon subcommands.
        let Cli {
            command:
                Some(Command::Daemon {
                    cmd: Some(DaemonCmd::Restart),
                    ..
                }),
            ..
        } = Cli::try_parse_from(["taskboard", "daemon", "restart"])
            .expect("the restart arm parses")
        else {
            panic!("expected daemon restart");
        };
    }

    #[test]
    fn unlock_parses_bare() {
        // #1249: the top-level force-unlock verb.
        let Cli {
            command: Some(Command::Unlock),
            ..
        } = Cli::try_parse_from(["taskboard", "unlock"]).expect("unlock parses")
        else {
            panic!("expected unlock");
        };
    }

    #[test]
    fn daemon_no_ipc_flag_parses() {
        let Cli {
            command: Some(Command::Daemon { cmd: None, no_ipc }),
            ..
        } = Cli::try_parse_from(["taskboard", "daemon", "--no-ipc"])
            .expect("the flag parses — the run arm stays the run arm")
        else {
            panic!("expected bare daemon");
        };
        assert!(no_ipc);
        let Cli {
            command: Some(Command::Daemon { no_ipc, .. }),
            ..
        } = Cli::try_parse_from(["taskboard", "daemon"]).expect("bare daemon parses")
        else {
            panic!("expected daemon");
        };
        assert!(!no_ipc);
    }

    #[test]
    fn daemon_stop_accepts_no_ipc() {
        // The accept-and-ignore contract is parseable surface: a stop
        // never spawns a server, so the flag is inert there — but it
        // must parse, not become a usage error.
        let Cli {
            command:
                Some(Command::Daemon {
                    cmd: Some(DaemonCmd::Stop),
                    no_ipc,
                }),
            ..
        } = Cli::try_parse_from(["taskboard", "daemon", "stop", "--no-ipc"])
            .expect("stop --no-ipc parses")
        else {
            panic!("expected daemon stop");
        };
        assert!(no_ipc);
    }

    #[test]
    fn parse_due_rejects_garbage_as_a_usage_error() {
        for raw in ["tomorrow", "2026-13-40", ""] {
            assert!(parse_due(raw).is_err(), "{raw} must be rejected");
        }
    }

    /// US-4/AC-1 (cross-surface friction plan D-5, #772): the
    /// `search` alias resolves to the same `Find` variant with
    /// identical fields — the typed outcome, never help text. The
    /// alias carries the variant's own refusals (the arg grammar
    /// rides the variant).
    #[test]
    fn us4_ac1_search_alias_matches_find() {
        let Cli {
            command:
                Some(Command::Find {
                    query: fq,
                    kind: fk,
                    limit: fl,
                }),
            ..
        } = Cli::try_parse_from(["taskboard", "find", "needle"]).expect("find parses")
        else {
            panic!("expected find");
        };
        let Cli {
            command:
                Some(Command::Find {
                    query: sq,
                    kind: sk,
                    limit: sl,
                }),
            ..
        } = Cli::try_parse_from(["taskboard", "search", "needle"])
            .expect("the alias parses to the same variant")
        else {
            panic!("expected find via the alias");
        };
        assert_eq!((fq, fk, fl), (sq, sk, sl), "one engine, two spellings");
    }

    #[test]
    fn find_flags_parse_typedly_and_refuse_garbage() {
        // Well-formed combinations parse.
        let Cli {
            command: Some(Command::Find { query, kind, limit }),
            ..
        } = Cli::try_parse_from([
            "taskboard",
            "find",
            "needle",
            "--type",
            "board",
            "--limit",
            "3",
        ])
        .expect("find with flags parses")
        else {
            panic!("expected find");
        };
        assert_eq!(query, "needle");
        assert_eq!(kind, Some(FindType::Board));
        assert_eq!(limit, Some(3));

        // Defaults: no kind filter, no cap.
        let Cli {
            command: Some(Command::Find { query, kind, limit }),
            ..
        } = Cli::try_parse_from(["taskboard", "find", "needle"]).expect("bare find parses")
        else {
            panic!("expected find");
        };
        assert_eq!(query, "needle");
        assert_eq!(kind, None);
        assert_eq!(limit, None);

        // An unknown kind is a clap usage error, never a runtime compare.
        assert!(Cli::try_parse_from(["taskboard", "find", "needle", "--type", "bogus"]).is_err());
        // A zero limit is a usage error — "print nothing" is not a
        // meaningful invocation.
        assert!(Cli::try_parse_from(["taskboard", "find", "needle", "--limit", "0"]).is_err());
    }

    #[test]
    fn parse_before_flag_reuses_the_config_grammar() {
        assert_eq!(
            parse_before_flag("10m"),
            Ok(chrono::TimeDelta::try_minutes(10).expect("10m"))
        );
        assert!(parse_before_flag("5min").is_err());
    }

    #[test]
    fn parse_effort_none_clears_and_vocabulary_parses() {
        assert_eq!(parse_effort("none"), Ok(EffortSpec::Clear));
        assert_eq!(
            parse_effort("1h30m"),
            Ok(EffortSpec::Set(taskboard_domain::Effort::new(5400)))
        );
        assert_eq!(
            parse_effort("30"),
            Ok(EffortSpec::Set(taskboard_domain::Effort::new(30 * 60)))
        );
        assert!(
            parse_effort("1h30").is_err(),
            "bad grammar is a usage error"
        );
        assert!(parse_effort("").is_err());
    }

    #[test]
    fn parse_level_flags_none_clears_and_vocabulary_parses() {
        assert_eq!(parse_importance("none"), Ok(ImportanceSpec::Clear));
        assert_eq!(parse_urgency("none"), Ok(UrgencySpec::Clear));
        assert_eq!(
            parse_importance("very-high"),
            Ok(ImportanceSpec::Set(taskboard_domain::Importance::VeryHigh))
        );
        assert_eq!(
            parse_urgency("low"),
            Ok(UrgencySpec::Set(taskboard_domain::Urgency::Low))
        );
        for bad in ["medium", "very_low", ""] {
            assert!(parse_importance(bad).is_err(), "{bad} must be rejected");
            assert!(parse_urgency(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn a_single_flag_satisfies_the_edit_group_on_each_arm() {
        let cli = Cli::try_parse_from(["taskboard", "tasks", "edit", "draft", "--title", "t"])
            .expect("one field flag satisfies the group");
        let Some(Command::Tasks {
            cmd: TasksCmd::Edit { flags, .. },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        assert_eq!(flags.title.as_deref(), Some("t"));

        let cli = Cli::try_parse_from(["taskboard", "tasks", "edit", "draft", "--archive"])
            .expect("a bare bool flag counts as a group member");
        let Some(Command::Tasks {
            cmd: TasksCmd::Edit { flags, .. },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        assert!(flags.archive);

        let cli =
            Cli::try_parse_from(["taskboard", "labels", "edit", "urgent", "--color", "0f0f0f"])
                .expect("one parser-backed flag satisfies the group");
        let Some(Command::Labels {
            cmd: LabelsCmd::Edit { flags, .. },
        }) = cli.command
        else {
            panic!("expected labels edit");
        };
        assert_eq!(flags.color.as_deref(), Some("0f0f0f"));
    }

    #[test]
    fn an_edit_naming_every_flag_together_still_parses() {
        // `multiple = true`: any number of members together is legal.
        // `--unarchive` is necessarily omitted — it conflicts with
        // `--archive` (a member-level rule orthogonal to group arity).
        let cli = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "edit",
            "draft",
            "--title",
            "t",
            "--description",
            "d",
            "--due",
            "2026-11-02",
            "--scheduled",
            "2026-11-02",
            "--effort",
            "1h",
            "--importance",
            "high",
            "--urgency",
            "low",
            "--archive",
        ])
        .expect("all non-conflicting members together parse");
        let Some(Command::Tasks {
            cmd: TasksCmd::Edit { task, flags },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        assert_eq!(task, "draft");
        assert!(flags.archive);
        // Consciously updated with US-1/D-5 (task-detail-editor UX
        // plan, 2026-11-17): the bare --due now parses to `Date` — the
        // instant mint moved to the execution seam.
        assert!(matches!(flags.due, Some(DueSpec::Date(_))));

        let cli = Cli::try_parse_from([
            "taskboard",
            "labels",
            "edit",
            "urgent",
            "--title",
            "t",
            "--color",
            "0f0f0f",
        ])
        .expect("both label flags together parse");
        let Some(Command::Labels {
            cmd: LabelsCmd::Edit { flags, .. },
        }) = cli.command
        else {
            panic!("expected labels edit");
        };
        assert_eq!(flags.title.as_deref(), Some("t"));
        assert_eq!(flags.color.as_deref(), Some("0f0f0f"));
    }

    #[test]
    fn the_edit_group_keeps_the_archive_unarchive_conflict() {
        let err = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "edit",
            "draft",
            "--archive",
            "--unarchive",
        ])
        .expect_err("the member-level conflict survives the group");
        assert_eq!(err.kind(), ClapErrorKind::ArgumentConflict);
    }

    #[test]
    fn the_field_flags_land_on_the_edit_arm() {
        let cli = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "edit",
            "draft",
            "--scheduled",
            "2026-10-24",
            "--effort",
            "1h30m",
            "--importance",
            "high",
            "--urgency",
            "very-low",
        ])
        .expect("field flags parse");
        let Some(Command::Tasks {
            cmd:
                TasksCmd::Edit {
                    flags:
                        TaskEditFlags {
                            scheduled,
                            effort,
                            importance,
                            urgency,
                            ..
                        },
                    ..
                },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        // Consciously updated with US-1/D-5: a bare --scheduled parses
        // to `Date` (the mint lives at the execution seam).
        assert!(matches!(scheduled, Some(DueSpec::Date(_))));
        assert_eq!(
            effort,
            Some(EffortSpec::Set(taskboard_domain::Effort::new(5400)))
        );
        assert_eq!(
            importance,
            Some(ImportanceSpec::Set(taskboard_domain::Importance::High))
        );
        assert_eq!(
            urgency,
            Some(UrgencySpec::Set(taskboard_domain::Urgency::VeryLow))
        );
    }

    #[test]
    fn the_field_flags_clear_on_none_and_error_on_garbage() {
        let cli = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "edit",
            "draft",
            "--scheduled",
            "none",
            "--effort",
            "none",
            "--importance",
            "none",
            "--urgency",
            "none",
        ])
        .expect("none clears each field flag");
        let Some(Command::Tasks {
            cmd:
                TasksCmd::Edit {
                    flags:
                        TaskEditFlags {
                            scheduled,
                            effort,
                            importance,
                            urgency,
                            ..
                        },
                    ..
                },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        assert_eq!(scheduled, Some(DueSpec::Clear));
        assert_eq!(effort, Some(EffortSpec::Clear));
        assert_eq!(importance, Some(ImportanceSpec::Clear));
        assert_eq!(urgency, Some(UrgencySpec::Clear));

        for bad in [
            ["--effort", "90min"],
            ["--importance", "medium"],
            ["--urgency", "very_high"],
            ["--scheduled", "tomorrow"],
        ] {
            let mut argv = vec!["taskboard", "tasks", "edit", "draft"];
            argv.extend(bad);
            assert!(
                Cli::try_parse_from(&argv).is_err(),
                "{bad:?} must be a clap usage error"
            );
        }
    }

    #[test]
    fn connector_flags_parse_on_the_targeting_verbs() {
        // `--connector` lands in the arm on boards list/select/publish
        // and sync (D6-5).
        let cli = Cli::try_parse_from(["taskboard", "boards", "list", "--connector", "beta"])
            .expect("boards list --connector parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::List { connector },
        }) = cli.command
        else {
            panic!("expected boards list");
        };
        assert_eq!(connector.as_deref(), Some("beta"));

        let cli = Cli::try_parse_from([
            "taskboard",
            "boards",
            "select",
            "42",
            "--connector",
            "beta",
            "--bind-local-board",
        ])
        .expect("boards select --connector parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Select {
                board, connector, ..
            },
        }) = cli.command
        else {
            panic!("expected boards select");
        };
        assert_eq!(board, 42);
        assert_eq!(connector.as_deref(), Some("beta"));

        let cli = Cli::try_parse_from(["taskboard", "boards", "publish", "--connector", "beta"])
            .expect("boards publish --connector parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Publish { connector, .. },
        }) = cli.command
        else {
            panic!("expected boards publish");
        };
        assert_eq!(connector.as_deref(), Some("beta"));

        let cli = Cli::try_parse_from(["taskboard", "sync", "--connector", "beta"])
            .expect("sync --connector parses");
        let Some(Command::Sync { connector, .. }) = cli.command else {
            panic!("expected sync");
        };
        assert_eq!(connector.as_deref(), Some("beta"));

        // Omission is `None` — the default (first) connector.
        let cli = Cli::try_parse_from(["taskboard", "boards", "list"]).expect("bare list");
        let Some(Command::Boards {
            cmd: BoardsCmd::List { connector },
        }) = cli.command
        else {
            panic!("expected boards list");
        };
        assert_eq!(connector, None);
    }

    #[test]
    fn boards_use_parses_with_and_without_a_uuid() {
        let good = "00000000-0000-0000-0000-000000000001";
        let cli = Cli::try_parse_from(["taskboard", "boards", "use", good]).expect("use parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Use { board },
        }) = cli.command
        else {
            panic!("expected boards use");
        };
        assert_eq!(
            board,
            Some(taskboard_domain::BoardId::from_uuid(
                uuid::Uuid::parse_str(good).unwrap()
            ))
        );
        let cli = Cli::try_parse_from(["taskboard", "boards", "use"]).expect("query parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Use { board },
        }) = cli.command
        else {
            panic!("expected boards use");
        };
        assert_eq!(board, None);
        // Malformed uuids are usage errors, like --board.
        assert!(Cli::try_parse_from(["taskboard", "boards", "use", "nope"]).is_err());
    }

    #[test]
    fn the_select_positional_id_rename_is_invisible_to_the_syntax() {
        // The internal clap id is `remote_board` (completion routes it
        // as server state); the displayed value name stays `board`.
        let cli = Cli::try_parse_from(["taskboard", "boards", "select", "42"])
            .expect("the positional still parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Select { board, .. },
        }) = cli.command
        else {
            panic!("expected boards select");
        };
        assert_eq!(board, 42);
    }

    #[test]
    fn gui_parses_as_a_first_class_subcommand_with_no_arguments() {
        let cli = Cli::try_parse_from(["taskboard", "gui"]).expect("gui parses");
        assert!(matches!(cli.command, Some(Command::Gui)));

        // Global flags compose with the verb in both orders.
        let cli = Cli::try_parse_from(["taskboard", "--json", "gui"]).expect("--json gui");
        assert!(cli.json);
        assert!(matches!(cli.command, Some(Command::Gui)));
        let config: &str = "some.toml";
        let cli =
            Cli::try_parse_from(["taskboard", "--config", config, "gui"]).expect("--config gui");
        assert_eq!(cli.config.as_deref(), Some(std::path::Path::new(config)));
        assert!(matches!(cli.command, Some(Command::Gui)));

        // The verb takes no arguments of its own.
        assert!(Cli::try_parse_from(["taskboard", "gui", "extra"]).is_err());
    }

    /// `us1_s1_batch_unset_arity_parse` (US-1/S1, W-4): the widened
    /// `config unset` grammar — one key parses exactly as before, more
    /// keys parse positionally, and zero keys parses too (the typed
    /// `BadArity` refusal is the command layer's, not clap's).
    #[test]
    fn us1_s1_batch_unset_arity_parse() {
        let cli = Cli::try_parse_from(["taskboard", "config", "unset", "focus.work_mins"])
            .expect("the one-key form parses");
        let Some(Command::Config {
            cmd: ConfigCmd::Unset { keys },
        }) = cli.command
        else {
            panic!("config unset parses");
        };
        assert_eq!(keys, vec!["focus.work_mins".to_owned()]);

        let cli = Cli::try_parse_from([
            "taskboard",
            "config",
            "unset",
            "focus.work_mins",
            "reminders.tick_secs",
        ])
        .expect("the batch form parses");
        let Some(Command::Config {
            cmd: ConfigCmd::Unset { keys },
        }) = cli.command
        else {
            panic!("config unset parses");
        };
        assert_eq!(
            keys,
            vec![
                "focus.work_mins".to_owned(),
                "reminders.tick_secs".to_owned()
            ]
        );

        let cli = Cli::try_parse_from(["taskboard", "config", "unset"]).expect("zero keys parses");
        let Some(Command::Config {
            cmd: ConfigCmd::Unset { keys },
        }) = cli.command
        else {
            panic!("config unset parses");
        };
        assert!(
            keys.is_empty(),
            "arity is the command layer's typed refusal"
        );
    }

    /// `us2_s1_reload_parses_bare` (US-2/S1, W-4): the `config reload`
    /// verb parses with no arguments and nothing else.
    #[test]
    fn us2_s1_reload_parses_bare() {
        let cli = Cli::try_parse_from(["taskboard", "config", "reload"]).expect("bare reload");
        assert!(matches!(
            cli.command,
            Some(Command::Config {
                cmd: ConfigCmd::Reload,
            })
        ));
        assert!(Cli::try_parse_from(["taskboard", "config", "reload", "extra"]).is_err());
    }
}
