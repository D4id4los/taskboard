# [Plan] M1-Exit WI-4 — Parted-style Interactive REPL Frontend

- **Date**: 2026-10-11
- **Target**: `taskboard-app` only (`main.rs`, `lib.rs`, new `repl.rs`,
  `cli.rs` unchanged in shape, `commands/{mod,tasks,stacks,boards,sync}.rs`,
  root `Cargo.toml` + `crates/taskboard-app/Cargo.toml` for two new
  workspace deps, tests: new R-series + unit suites, `dispatch_routing.rs`
  and `cli_tty.rs` extensions), docs (ADR 0011, `architecture.org`,
  README.org, CHANGELOG, roadmap amendment, backlog appends). **No
  changes** to `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud`,
  `taskboard-ui-slint` — the REPL is purely a new frontend over the
  existing core (ADR 0008 "one bootstrap, N frontends").
- **Goal**: implement M1-exit report item 4 (direction C): a
  `parted`-style interactive REPL — no arguments on a TTY drops the user
  into a shell where each line is parsed as one CLI invocation and
  dispatched against **one** booted core; `exit`/Ctrl-D leaves. Every
  core verb stops paying a bootstrap-per-command; SSH/kiosk
  administration and daily driving get one-process-per-session instead
  of one-process-per-command. The enabling refactor — extracting the
  `AsyncFnOnce(&App)` handler bodies behind `with_core` into
  `*_on(&App, …)` functions — is exactly the seam WI-5 (daemon IPC)
  reuses, so this phase pays WI-5's largest cost forward.
- **Status**: DRAFT — awaiting user review. Product decisions in §2 are
  **proposed** (marked [CONFIRM]); everything else is engineering shape.
- **Parent roadmap**: `.artifacts/reports/2026-10-11-m1-exit-pre-gui-analysis-report.md`
  §4C + §7 row 4; backlog entry "[2026-10-11] `parted`-style
  interactive REPL mode".
- **Predecessors**: Phase 6b merged (stand-alone-first; `main` @
  `13e1891`). Phase 6b's consumer-handoff map names this work "WI-4
  (REPL)".
- **Parallel-WI note**: WI-3 (CLI completeness, report item 3) is being
  planned concurrently on its own branch. This plan is authored against
  `main` **without** WI-3 and is deliberately shaped to rebase onto
  WI-3's merge cheaply (§6.2 playbook). Do **not** start implementation
  before WI-3 merges unless the user orders it; the plan is written so
  every step is either WI-3-independent or mechanically re-appliable.
- **Branch / worktree**: `feat/repl-interactive-frontend`, worktree
  `../taskboard-WT-feat-repl-interactive-frontend` (created from `main`
  @ `13e1891`). One PR for the plan + implementation (phase 4/5/6/6b
  precedent: plan revisions and implementation commits land on the same
  branch).

---

## 1. Context & Known Facts (do not re-derive; verified on `main` @ `13e1891`)

- **The `&App` seam is half-built already.** Three verb groups already
  split their "on a booted core" half from the boot/shutdown shell:
  `sync::run` → `run_locked(&app, …)` (sync.rs:57-85),
  `boards::publish` → `publish_locked(&app, client, …)` (boards.rs:122-202),
  `boards::select` → `select_locked(&app, …)` (boards.rs:227-311). The
  remaining verbs (`tasks::{list,add,done,move_}`, `stacks::add`,
  `boards::create`) inline an `AsyncFnOnce(&App)` closure into
  `with_core(config, store, async |app| …)` (tasks.rs:32-48) — the
  bodies are already `&App`-shaped, they just live in closures. The
  extraction is mechanical.
- **`with_core` semantics a REPL must not inherit per line**: boot →
  (nextcloud only) `wait_for_boot_pull` bounded wait (tasks.rs:58-86) →
  body → `shutdown()` (whose patient join drains the boot cycle's
  push). A REPL runs this sequence **once per session**, not per line:
  mutations stay durable-the-moment-`execute`-replies (outbox
  semantics, tasks.rs module docs), and the session's sync actor drains
  them on its `poll_interval_secs` cadence or when the user runs a
  `sync` line.
- **`dispatch(command, ctx) -> u8` never exits the process** (mod.rs:252-426);
  `main` performs `std::process::exit` (main.rs:79-80). Exit codes:
  0 success / 1 runtime failure / 2 clap usage / 130 second-signal force
  exit (ADR 0009). A REPL loop can therefore call the routing layer per
  line and simply not exit on nonzero codes.
- **`Command::Daemon` is not in `dispatch`** — `main` intercepts it and
  the arm is `unreachable!` (mod.rs:424; main.rs:72-81). A session
  dispatcher must intercept `Daemon` (and `Login`, §2.4) *before* any
  routing.
- **Pre-core verbs** (`login`, `boards list`) build config + a
  `DeckClient` via `client_for` with no actor graph, no lock
  (mod.rs:7-13, boards.rs:29-64). They can run per line unchanged —
  except `login`, whose `StdinPrompts` would fight the REPL's terminal
  (login.rs:23-66 `PromptReader` trait); §2.4 refuses it in-session.
- **The instance lock makes REPL and daemon mutually exclusive by
  construction**: `bootstrap()` acquires the per-database lock and the
  `App` handle owns it for its lifetime (lockfile.rs; typed
  `BootstrapError::InstanceLocked`, JSON kind `instance-locked`,
  mod.rs:169). A REPL session is exactly one lock epoch, like the
  daemon. Starting a REPL while the daemon runs fails typed at boot —
  existing behavior, to be documented (§8).
- **No-argument entry today**: bare `taskboard` resolves
  `[app] mode` (`AppMode::{Daemon (default), Desktop, Kiosk}`,
  config.rs:61-71); `Desktop`/`Kiosk` fail `ModeNotImplemented`
  (main.rs:97-106). The mode-resolution code path and its
  `require_connector` daemon refusal (main.rs:83-91) stay — the REPL
  claims only the TTY branch (§2.1).
- **The engine is a long-lived servant already exercised as one**: the
  daemon holds a booted `App` indefinitely; the REPL reuses the exact
  lifecycle (boot → serve → `shutdown()`), just with a line loop
  instead of `wait_for_signal`. `EngineHandle::{execute,
  shared_state, subscribe}` and `App::sync_commands` are the whole
  surface the REPL needs (verified across handlers).
- **Test harness precedent for every layer this plan adds**:
  - Hermetic standalone cores need no wiremock and no credentials
    (Phase 6b's ST-series, tests/cli_standalone.rs) — the R-series
    rides this win.
  - The D-series dispatch-routing suite drives `dispatch()` per
    `Command` variant hermetically (tests/dispatch_routing.rs) — the
    session dispatcher gets the same treatment.
  - `tests/cli_tty.rs` drives the **real binary under a `script(1)`
    pty** with scripted stdin and structural assertions (exit codes,
    files) — the pattern for the REPL's end-to-end pty journey.
  - `tests/common` provides `FakePrompts`, `FakeCredentialStore`,
    `valid_config`, `env_lock`, JSON fixtures (common/mod.rs).
- **Output contract is writer-hardcoded today**: `emit` uses
  `println!`, `report` uses `eprintln!` (mod.rs:457-492). Hermetic
  R-series assertions need injectable writers (§4.1) — which is also
  the shape WI-5's socket transport needs.
- **Dependencies**: `rustyline` 17 (MIT) for line editing/history,
  `shell-words` 1 (MIT) for POSIX-ish word splitting. Both pass
  `deny.toml`'s license allow-list (MIT/Apache-2.0 allowed,
  deny.toml:7-24); both get pinned once in root
  `[workspace.dependencies]` (AGENTS §4). No other new deps; nothing
  outside the approved-stack spirit (new deps are precedented:
  `figment`, `insta`, `wiremock` all arrived the same way, gated by
  `cargo deny`).
- **Coverage floor**: app crate measured 80.33% lines at the M1-exit
  report (2026-10-11, pre-WI-2); re-measure at branch point and treat
  that number as the floor the R-series must not regress (§9).

## 2. Proposed Product Decisions ([CONFIRM] at review)

1. **[CONFIRM] No-argument entry becomes TTY-sensitive** (recommended
   shape from the M1-exit report §4C): bare `taskboard` **with stdin a
   TTY** → REPL; **without a TTY** → today's `[app] mode` resolution
   (daemon default; kiosk/desktop arms unchanged). Explicit
   subcommands (including `daemon`) are unchanged in all cases.
   Rationale: parted semantics is the product ask; systemd/SSH-piped
   deployments (no TTY) keep today's behavior exactly; nothing ships
   pre-v0.1.0 so this is the cheapest moment to change the default.
   Accepted consequence: `[app] mode = "kiosk"` + a human console
   currently gets `ModeNotImplemented`; after this phase they get a
   REPL — strictly more useful pre-Phase-7, recorded in ADR 0011.
   Alternatives rejected: explicit `repl` subcommand only (discoverable
   but not parted-style; could be added later as an alias), always-REPL
   (breaks kiosk boot by default).
2. **[CONFIRM] One core per session, boot-once semantics**: the REPL
   boots the core at session start (including the one-time
   `wait_for_boot_pull` under the nextcloud backend), serves every
   line against that `App`, and shuts down at exit. Consequences
   (documented in ADR 0011 + README): mutations are durable locally
   the moment the line's `execute` replies; the outbox drains via the
   session sync actor's poll cadence or a `sync` line (no per-line
   shutdown-join push); reads are as fresh as the last completed
   cycle. Under `standalone` none of this involves a connector
   (Phase 6b handoff fact).
3. **[CONFIRM] The session holds the instance lock** for its lifetime:
   a running daemon makes REPL boot fail typed (`instance-locked`,
   exit 1 — existing behavior); conversely `daemon` cannot start while
   a REPL runs. Runbook documents "stop the daemon first". No
   lock-sharing mechanism is built (that is WI-5's IPC story).
4. **[CONFIRM] `login` is refused in-session** (REPL-level usage
   error, not a typed `CommandError` — it is routing, not a handler
   failure): mid-session login cannot activate a connector in the
   running session (credential + `DeckClient` wiring happens in
   `bootstrap`), so the honest UX is "exit, `taskboard login`,
   re-enter". This also sidesteps `StdinPrompts` fighting the rustyline
   terminal. Under a session booted `standalone`, connector verbs
   already refuse typed `sync-disabled` — consistent picture. `boards
   list` stays allowed (it is a stateless remote read; credentials
   exist whenever the session booted nextcloud).
5. **[CONFIRM] Per-line flags**: a line may carry `--json` (that line
   emits JSON; the session default comes from the launch flags); a
   line carrying `--config` is refused in-loop with guidance (the
   session's config is fixed at boot). Both are parse-level rules,
   unit-pinned (§4.4).
6. **[CONFIRM] Exit semantics**: builtins `exit`/`quit` and Ctrl-D
   (EOF) perform graceful `shutdown()` and exit code 0; Ctrl-C at the
   prompt cancels the current input line and re-prompts (rustyline
   `Interrupted`); **Ctrl-C while a command is running kills the
   session** (SIGINT, default disposition — no handler in v1): safe
   because state is durable per-`execute` and the OS releases the
   lock; graceful line-cancellation is a backlog entry (§7). Line
   failures never abort the session and do not affect the exit code.
7. **[CONFIRM] History is in-memory only in v1** (rustyline session
   history; no file writes): persistence needs the XDG data-dir story
   WI-3 owns, and coupling this branch to it would widen the rebase.
   Backlog entry appended (§7).
8. **Prompt and banner are minimal constants** (`taskboard> `; one
   startup line with version + resolved backend): UI text, never
   asserted (AGENTS §5).

## 3. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| Seam extraction | Every core verb gains `*_on(&App, …)` (tasks/stacks/boards-create closures promoted; `select_locked`/`publish_locked`/`run_locked` renamed `*_on` and made `pub(crate)`); `with_core` becomes the one-shot wrapper calling `_on`; behavior-preserving (existing suites green, unchanged) |
| Dispatch | Injectable writers (`CommandIo { out, err, json }` inside `CommandCtx`) + `emit_to`/`report_to`; new session router `dispatch_line` (intercepts `Daemon`/`Login`, routes everything else to `_on`/pre-core handlers) |
| REPL | New `crates/taskboard-app/src/repl.rs`: `LineEvent` stream architecture (blocking rustyline thread → bounded channel → async loop), `repl_loop`, session boot/shutdown, line parsing (`shell-words` split → `Cli::try_parse_from` → in-loop clap error printing), builtins `exit`/`quit`, banner |
| Entry | `entry_point(has_command, stdin_tty, mode) -> Entry` pure function + `main` wiring via `std::io::IsTerminal` |
| Deps | `rustyline = "17"`, `shell-words = "1"` pinned in root `[workspace.dependencies]` (§4.7) |
| Tests | Unit: entry-resolution table, line-parse rules, split/quote handling. R-series (`tests/repl.rs`): hermetic standalone-core session journeys via scripted `LineEvent` streams + injected writers, structural JSON assertions. D-series rows for `dispatch_line`. Pty E2E (`cli_tty.rs` extension): real binary under `script(1)`, scripted session, structural assertions. Lock-coexistence rows (daemon running → REPL exit 1 `instance-locked`) |
| Docs | ADR 0011; `architecture.org` frontend section gains the REPL; README REPL section; CHANGELOG; roadmap master-plan amendment (§8); backlog appends (§7) |

**Out of scope** (backlog or later work items): `login` inside a
session (§2.4); history persistence (§2.7, rides WI-3's XDG work);
tab-completion/hints over verbs and selectors; graceful Ctrl-C
line-cancellation (§2.6); config reload mid-session; multiline input;
REPL scripting/`-c` batch mode; daemon IPC (WI-5 — this phase only
pays its seam forward); anything in domain/state/storage/sync crates;
any UI work.

### 3.1 Entry surface after this phase

```
taskboard                          # stdin TTY  → REPL (NEW)
                                   # no TTY     → [app] mode resolution (unchanged)
taskboard daemon | login | boards | stacks | tasks | sync …   # unchanged one-shots
taskboard --json                   # TTY → REPL with session JSON default (NEW)

# REPL line grammar (one line = one invocation):
#   <any one-shot subcommand line>   — routed against the session core
#   --json <line>                    — JSON output for that line only
#   exit | quit                      — graceful shutdown, exit 0
#   <empty line>                     — re-prompt
#   Ctrl-D (EOF)                     — exit; Ctrl-C at prompt — cancel line
```

## 4. Design Decisions

1. **Two dispatchers, one verb seam — accept the thin routing
   duplication.** `dispatch(command, ctx)` keeps its signature and
   one-shot semantics verbatim (boot-per-verb through `with_core` /
   `core_ctx`); a new `dispatch_line(session, command, io) -> u8`
   routes the same `Command` enum against the session `&App`. Both are
   thin: parse-level match arms calling shared `_on` / pre-core
   functions. Unifying into one table parameterized by core-provision
   was considered and rejected: the one-shot arms must decide
   *per arm* whether a core boots at all (pre-core verbs must not take
   the lock), so a unified table smuggles control flow into data.
   Both tables get routing-only suites (D-series precedent; "higher
   layers test routing only", AGENTS §5). WI-5 may revisit when a
   third frontend exists.

2. **The extraction is behavior-preserving and compiler-checked.**
   Per verb: move the closure body to
   `pub(crate) async fn xxx_on(app: &App, …) -> Result<R, CommandError>`;
   the public `xxx(config, store, …)` becomes
   `with_core(config, store, async |app| xxx_on(app, …).await)`.
   Signatures carry only owned/borrowed data they already carry. The
   regression gate is **zero changes to existing test files** (L/V/K/O/
   ST/D-series stay green byte-for-byte). `wait_for_boot_pull` moves
   from `tasks.rs` private to `pub(crate)` (home: `bootstrap.rs`, next
   to the lifecycle it guards) because the REPL boot calls it once.

3. **REPL loop architecture: never block a runtime worker.** rustyline
   `Editor::readline` is synchronous; calling it inside an async task
   would pin (on a 1-core kiosk: deadlock) a tokio worker. Design:

   ```rust
   /// What the editor thread delivers per interaction.
   pub(crate) enum LineEvent {
       /// A submitted input line (already shell-split at the loop).
       Line(String),
       /// Ctrl-D on an empty line.
       Eof,
       /// Ctrl-C while editing (cancel the line, keep reading).
       Interrupted,
   }

   /// Spawns the dedicated line-editor thread (rustyline owns stdin
   /// there); lines stream back with bounded(1) backpressure so the
   /// reader never runs ahead of execution.
   pub(crate) fn spawn_line_reader() -> mpsc::Receiver<LineEvent>;

   /// The session loop: consume events, dispatch lines, honor exits.
   pub(crate) async fn repl_loop(
       app: App,
       session: ReplSession,
       lines: mut mpsc::Receiver<LineEvent>,
       io: CommandIo<'_>,           // injected writers (§4.1)
   ) -> ReplExit;                   // { Graceful } — boot errors exit earlier
   ```

   Production wires `spawn_line_reader()`; tests inject a
   `mpsc::Receiver` they feed from a script — the editor, the thread,
   and the terminal never appear in hermetic tests. The rustyline
   `Editor` is created inside and confined to the reader thread
   (sidesteps `Send` questions entirely).

4. **Line parsing is `Cli::try_parse_from` with REPL-level rules.**
   Order per event (unit-pinned, §6 step 4):
   `Interrupted` → new prompt; `Eof` → graceful exit; empty/whitespace
   line → re-prompt; `exit`/`quit` token → graceful exit;
   `shell_words::split` error (unbalanced quote) → in-loop usage line,
   continue; `Cli::try_parse_from(["taskboard", …words])`:
   - `Ok(cli)` with `cli.config.is_some()` → in-loop refusal (§2.5),
     continue;
   - `Ok(cli)` with `cli.command == None` (a bare flag line) →
     re-prompt;
   - `Ok(cli)` with `Command::Daemon | Command::Login` → in-loop
     refusal (§2.4), continue;
   - otherwise → `dispatch_line(session, command, line_io)` where
     `line_io.json = cli.json || session.json_default`;
   - `Err(clap_err)` → `clap_err.print()` (help/version to stdout,
     usage errors to stderr — clap routes by kind), continue. **The
     process never `exit(2)` from a REPL line.**
   REPL-level refusals are one small non-exhaustive enum with `Display`
   (UI text; tests assert routing/side effects, never wording).

5. **Session boot mirrors the one-shot prefix exactly**: config load →
   `bootstrap` (typed failures — `instance-locked`, config, storage —
   reported through `report_to` with the session JSON flag, exit 1) →
   one-time `wait_for_boot_pull` when the resolved backend is
   `nextcloud` → banner → loop → `app.shutdown()` → exit 0. Under
   `standalone` the session never touches a credential store or the
   network (Phase 6b guarantees). `daemon`'s `require_connector`
   refusal does **not** apply to the REPL (standalone REPL is a
   first-class journey; only the connector verbs refuse typed, per
   line, at handler entry — existing behavior).

6. **`ReplSession` owns the cross-line state**:
   `struct ReplSession { config: AppConfig, store: Arc<dyn
   CredentialStore>, json_default: bool }`. The store is the
   `production_store` pick from boot config — per-line client-building
   verbs (`boards list/select/publish`) construct fresh `DeckClient`s
   from it (`client_for`), unchanged from one-shot behavior.

7. **Dependencies** (root `Cargo.toml`, `[workspace.dependencies]`):
   `rustyline = "17"` (line editing, history, raw-mode Ctrl-C/D
   detection; MIT), `shell-words = "1"` (POSIX word splitting with
   quotes/escapes; MIT). Both consumed by `taskboard-app` via
   `dep.workspace = true`. `cargo deny check` gates the license tree
   (both land in the existing MIT/Apache allow list — verified against
   deny.toml:7-24). No feature flags beyond rustyline defaults needed
   for v1 (no file history → not `with-file-history`).

## 5. Normative Shapes (signatures binding; bodies indicative)

### 5.1 The verb seam (`commands/`)

```rust
// tasks.rs — representative; stacks::add_on, boards::create_on follow:
pub(crate) async fn list_on(
    app: &crate::App,
    stack: Option<&str>,
) -> Result<TaskList, CommandError> { /* closure body, unchanged */ }

pub async fn list(config: AppConfig, store: Arc<dyn CredentialStore>,
                  stack: Option<&str>) -> Result<TaskList, CommandError> {
    with_core(config, store, async |app| list_on(app, stack).await).await
}

// boards.rs / sync.rs — renames, bodies unchanged:
select_locked   -> pub(crate) select_on(app: &App, board: u64,
                        timeout: Duration, bind_local_board: bool)
publish_locked  -> pub(crate) publish_on(app: &App, client: &DeckClient,
                        timeout: Duration)
run_locked      -> pub(crate) sync_on(app: &App, no_wait: bool,
                        timeout: Duration)
// (one-shot wrappers keep their client pre-validation then boot + _on)
```

### 5.2 Injectable output (`commands/mod.rs`)

```rust
/// Where a dispatch writes; stdout/stderr in the binary, buffers in tests
/// and (WI-5) sockets. `json` rides per invocation/line.
pub struct CommandIo<'a> {
    pub out: &'a mut dyn std::io::Write,
    pub err: &'a mut dyn std::io::Write,
    pub json: bool,
}

pub struct CommandCtx<'a> {
    pub config_path: Option<PathBuf>,
    pub io: CommandIo<'a>,
}

pub(crate) fn emit_to<T: Serialize>(io: &mut CommandIo<'_>, payload: &T, text: &str);
pub fn report_to(io: &mut CommandIo<'_>, err: &CommandError) -> u8;
// `report` stays as the stdout/stderr convenience the daemon arm uses.
```

### 5.3 Session dispatch + REPL (`repl.rs`, `commands/mod.rs`)

```rust
// commands/mod.rs:
/// Route one parsed line against a booted session core. Never exits;
/// returns the line's code (informational — the loop ignores it for
/// the session exit code, §2.6).
pub(crate) async fn dispatch_line(
    session: &ReplSession,
    app: &App,
    command: Command,
    io: CommandIo<'_>,
) -> u8;

// repl.rs:
pub(crate) struct ReplSession { config: AppConfig,
    store: Arc<dyn CredentialStore>, json_default: bool }

pub(crate) enum ReplExit { Graceful }

/// No-argument entry resolution (unit-table-tested; main calls with
/// std::io::stdin().is_terminal()).
#[must_use]
pub fn entry_point(has_command: bool, stdin_tty: bool, mode: AppMode) -> Entry;
pub enum Entry { Subcommand, Repl, ModeResolution(AppMode) }

pub(crate) fn spawn_line_reader() -> tokio::sync::mpsc::Receiver<LineEvent>;
pub(crate) async fn repl_loop(app: App, session: ReplSession,
    lines: tokio::sync::mpsc::Receiver<LineEvent>, io: CommandIo<'_>)
    -> ReplExit;
```

### 5.4 `main.rs` wiring

```rust
// inside run(cli), replacing the current if-let shape:
match repl::entry_point(cli.command.is_some(), std::io::stdin().is_terminal(), mode)
{
    Entry::Subcommand => { /* today's dispatch + exit path, io over stdout/stderr */ }
    Entry::Repl       => { /* boot session → spawn_line_reader → repl_loop
                              → exit 0; boot failure → report_to + exit 1 */ }
    Entry::ModeResolution(mode) => { /* today's daemon/kiosk arms verbatim */ }
}
```

## 6. Worktree, Branch & TDD Implementation Order

Branch `feat/repl-interactive-frontend`, worktree
`../taskboard-WT-feat-repl-interactive-frontend` (already created from
`main` @ `13e1891`). One PR. TDD order — commit per step, every commit
passing the full gate (§9). Steps 1-2 are pure refactor with a
**zero-existing-test-changes** gate; steps 3-6 are additive.

1. **Writer seams** (`CommandIo`/`CommandCtx`/`emit_to`/`report_to`):
   behavior-preserving; the only existing-file edits are the D-series
   `ctx()` helper (mechanical) and `main`'s call site. Unit test: JSON
   vs human routing through injected buffers (structural asserts).
2. **`*_on` extraction** across `tasks/stacks/boards/sync` +
   `wait_for_boot_pull` → `pub(crate)` in `bootstrap.rs`: the
   regression gate is the untouched existing suites. (This is the
   commit WI-5 thanks.)
3. **Entry resolution**: `entry_point` + `Entry` + unit table
   ((command × tty × mode) → expected arm, incl. kiosk/desktop rows);
   `main` wired via `IsTerminal`. The REPL arm is a stub
   (`unimplemented!`-free: return today's mode-resolution behavior for
   `Repl` until step 5 lands — or land 3 after 5; commit order chosen
   so every commit builds and passes).
4. **Line parsing layer** (pure): split → parse → route-decision
   returning an internal `LineAction` enum (Execute(Command, json) /
   Refuse(kind) / PrintClap(err) / Exit / Reprompt). Unit tests:
   quoting survives (`tasks add "two words"`), `--json` per line,
   `--config` refused, bare-flag line re-prompts, `exit`/`quit`,
   garbage subcommand yields `PrintClap` not `Exit`, `daemon`/`login`
   refused. No I/O — the layer is a pure function over a line.
5. **Session loop + R-series** (`tests/repl.rs`): standalone-core
   sessions (hermetic sqlite tempdir, `env_lock`, no wiremock):
   create-board → add-stack → add-task → list journey asserting
   structural effects (engine state via a second read, JSON receipts
   parsed from the injected `out` buffer); error lines continue the
   session (unknown-task line then a good line); `exit`/Eof shutdown
   joins cleanly (engine quiesces — poll the state predicate with a
   deadline, no fixed sleeps); per-line `--json` mix; `sync` line
   under standalone reports `sync-disabled` and the session survives.
   Boot-failure rows: locked db → typed `instance-locked`, exit 1.
6. **rustyline wiring + pty E2E**: `spawn_line_reader` (editor on its
   thread, `bounded(1)` channel, Interrupted/Eof mapping), production
   `main` REPL arm, banner/prompt. `cli_tty.rs` gains the pty journey:
   real binary, scripted lines (`boards create … --json`, `tasks
   add`, `tasks list`, `exit`), structural asserts (exit 0, sqlite db
   exists and hydrates in a follow-up one-shot; skip-never-fail guard
   on `script` availability, house precedent). Lock coexistence:
   daemon running → bare `taskboard` on pty exits 1 typed.
7. **Docs & bookkeeping** (§8): ADR 0011, architecture.org, README,
   CHANGELOG, roadmap amendment, backlog appends; full gates; §9
   runbook handed to the user.

### 6.2 Rebase playbook: WI-3 lands first (expected)

WI-3 (CLI completeness) adds verbs **in the boot-per-call pattern**
(edit/delete, labels, logout, config print, selector upgrade, XDG
discovery) — textually overlapping files: `commands/mod.rs` (new
arms), `commands/tasks.rs` (new verbs + `resolve_task`), possibly new
modules (`labels.rs`), `cli.rs` (new subcommands), `output.rs`,
`config.rs`. After its rebase-merge onto `main`:

```sh
git fetch origin --prune
git rebase --onto origin/main <tip-of-the-wi3-branch-as-merged> feat/repl-interactive-frontend
# resolve: prefer both sides — WI-3's new arms/verbs stay, re-apply the
# _on-extraction shape to the new verbs (mechanical, step 2's recipe)
git push --force-with-lease origin feat/repl-interactive-frontend
```

- The expected rebase job: (a) convert WI-3's new core verbs to `_on`
  + one-shot wrapper; (b) add the matching `dispatch_line` arms; (c)
  extend D-series/R-series routing rows. Purely mechanical, no design
  rework — the REPL needs zero changes for new verbs (it parses
  whatever `Cli` contains).
- If WI-3's plan pre-extracts the `&App` seam itself (it was assigned
  to WI-4 by the M1-exit report, but plans drift): steps 1-2 collapse
  to a no-op; drop them and keep the rest unchanged.
- If WI-4 were to merge first instead, the mirrored job applies to
  WI-3 (its new verbs arrive on a seam that now expects `_on`) —
  either order survives; the report's sequencing (B before C) is about
  user value at release time, not implementation coupling, and this
  plan preserves it by merging second.

## 7. Out of Scope → Backlog (entries appended with this plan)

- **REPL history persistence** (file-backed, XDG data dir): blocked on
  WI-3's config-discovery work; v1 is in-memory by decision §2.7.
- **Tab-completion / hints** (verb, selector, and board/stack/title
  completion over live `ArcSwap` state): rustyline `Helper` trait is
  the seam; worthwhile once the verb surface settles post-WI-3.
- **Graceful Ctrl-C line-cancellation** (SIGINT handler during
  dispatch that aborts the line, not the session): v1 accepts
  SIGINT-kills-session (durable-by-construction makes it safe, §2.6);
  the handler interacts with WI-5's process-signal story — design once.
- **`login` inside a session** (would need a masked-secret reader over
  the rustyline terminal + honest "restart to activate" UX): revisit
  with WI-5/Phase 7 settings flows.
- (Existing entries stay: daemon IPC WI-5; REPL entry already in the
  backlog gets a "planned/landed" note in §8's bookkeeping pass.)

## 8. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0011 — "Interactive REPL frontend and the no-argument entry"**:
  the second frontend on the shared core seam; TTY-sensitive no-arg
  rule (and why kiosk/systemd deployments are unaffected); session
  semantics (one lock epoch, boot-once, outbox drained by poll cadence
  or `sync`, freshness contract); per-line clap parsing with in-loop
  usage errors (no exit 2 from a line); in-session refusals (daemon,
  login, per-line `--config`) with rationale; the channel architecture
  (runtime workers never block on stdin); rustyline + shell-words
  choice; SIGINT-kill acceptance; writer seams as the WI-5
  down-payment. Status: Accepted (post-review).
- **`docs/architecture.org`**: the frontends section gains the REPL
  next to CLI-one-shot and daemon; the no-arg entry resolution table;
  the session lifecycle paragraph.
- **Roadmap master plan**: insert the REPL phase between the
  CLI-completeness phase (WI-3, whatever phase number it registers)
  and Phase 7, one paragraph + link to this plan; note the M1-exit
  report §7 table as the umbrella ordering.
- **README.org**: "Interactive shell" section after the one-shot
  quickstart: bare `taskboard`, the line grammar (§3.1), the
  daemon-mutex note, the standalone-REPL journey.
- **CHANGELOG** `[Unreleased]` → Added: interactive REPL (no-arg +
  TTY), per-line `--json`; Changed: bare `taskboard` on a TTY now
  opens the REPL instead of the daemon (non-TTY unchanged).
- **Backlog**: append §7 entries.

## 9. Verification & Acceptance

- Full quality gate (AGENTS §10): `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo nextest run --workspace`, `cargo test --doc --workspace`,
  `cargo deny check` (new deps vetted), `SQLX_OFFLINE=true cargo
  llvm-cov --workspace` — app-crate line coverage ≥ the number
  recorded at branch point (§1; the R-series + unit layers arrive
  tested, the refactor re-lights nothing).
- Tier 2 (docker): the existing J-series must stay green untouched
  (the refactor gate); no new docker tests required — the REPL adds no
  sync-crate surface. Optionally one `script(1)`-over-ssh style manual
  pass against the docker Nextcloud (runbook).
- **REPL runbook (phase exit, user-run)**, clean machine/profile, no
  keyring, no network: (1) bare `taskboard` in a terminal → banner +
  prompt; (2) `boards create "Personal" --json` → receipt line;
  (3) `stacks add "Todo" --json`; (4) `tasks add "first task" --json`
  → exit-code-0-shaped receipt; (5) `tasks list` → board/stack/task;
  (6) `sync` → typed `sync-disabled` line, session continues;
  (7) garbage line → clap usage text, session continues; (8) `exit` →
  exit code 0; (9) `taskboard tasks list` (one-shot) shows the session's
  data (durable); (10) start the daemon, bare `taskboard` → typed
  `instance-locked`, exit 1; stop daemon → REPL boots again;
  (11) Ctrl-D exits; Ctrl-C at the prompt cancels the line. With
  network + keyring (nextcloud profile): (12) REPL session, `sync`
  line → cycle outcome; `tasks list` reflects a web-UI-created card
  after the next cycle. Acceptance: user confirms.

## 10. Consumer Handoff Map

| Consumer | Takes from WI-4 |
|---|---|
| WI-5 (daemon IPC) | The `*_on` seam + `dispatch_line` are the in-process handler table a socket server calls; `CommandIo` writers are the socket transport's sinks; the channel-vs-runtime lesson (never block a worker on input) transfers to frame readers |
| WI-3 (parallel, merges first) | Rebase playbook §6.2; new verbs gain `_on` + session arms for free routing |
| Phase 7 UI | The REPL is a dry run of the UI lifecycle: boot-once frontend, live `ArcSwap` reads between commands, graceful `shutdown()` on exit — the exact loop a Slint window runs |
| Reminders subsystem (post-M2) | A long-lived session frontend is where in-session notifications would surface |

## 11. Risks & Mitigations

- **WI-3 rebase breadth** (shared `commands/` files): §6.2 playbook;
  steps 1-2 isolated as behavior-preserving commits so the rebase
  re-applies mechanically; either merge order survives.
- **rustyline platform quirks** (raw-mode edge cases; Windows is a
  release target): editor confined to one thread; the pty E2E proves
  the real-terminal path on Linux; Windows validation rides the
  existing release-pipeline matrix (backlog note if it bites).
- **Runtime starvation**: impossible by construction — the only
  blocking read lives on the dedicated editor thread; the async loop
  consumes events (§4.3). R-series would catch a regression the moment
  a session stalls under a scripted feed.
- **Per-line clap surprises** (global args re-parsed per line,
  `--config` sneaking through): pinned by unit table (§6 step 4); the
  refusal rule is parse-level, not handler-level.
- **TTY detection drift** (CI pipes, `script(1)`, MSYS): non-TTY keeps
  today's behavior (safe default); the pty test covers the TTY path;
  `entry_point` is a pure function so environments only flip its
  input, never its logic.
- **Ctrl-C killing a session mid-command**: accepted v1 (§2.6) —
  durable-per-`execute` + OS-released lock make it safe; graceful
  cancellation backloged with WI-5's signal story.
- **Scope creep** (completions, history files, scripting flags):
  triaged to §7; the deliverable is the seam + loop + entry rule.
