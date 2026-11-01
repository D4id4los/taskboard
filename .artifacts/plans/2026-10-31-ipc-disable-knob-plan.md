# [Plan] IPC disable knob — `[ipc] enabled` / `daemon --no-ipc` (chunk 1.6, #194) — stage-1 TDD implementation plan

- **Date**: 2026-10-31 (planning session executed 2026-11-01)
- **Kind**: stage-1 chunk plan for dispatch chunk **1.6** of backlog
  round 2 (`.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`).
  TDD-first; no production code in this commit.
- **Branch / worktree**: `feat/ipc-disable-knob` in
  `../taskboard-WT-feat-ipc-disable-knob` (off `main` @ `5bf90c63`;
  worktree-doctor passed 2026-11-01, 235 G free against the 25 G floor).
- **Normative origin**: GitHub issue **#194** (`backlog: IPC disable
  knob (`daemon --no-ipc` / `[ipc] enabled`)`, `dx`, `p3`), whose
  proposed approach is normative for the shape: *a
  `#[serde(default = "default_true")]` `[ipc] enabled` plus a
  `--no-ipc` override; documented as reducing `status`/`daemon stop` to
  typed `no-daemon`*. Context from the issue: IPC is always-on in v1;
  minimal-image hardening may want the daemon bound to nothing; when
  asked for, a flag/config key skips `spawn_server` — **the rest of the
  lifecycle is unchanged**.
- **Verification status**: all seam facts in §2 were read against
  `5bf90c63` on 2026-11-01. Nothing below is derived from memory.

## 1. Mission & binding constraints

Add an opt-**out** for the daemon's IPC surface: a config key
`[ipc] enabled` (default `true`) and a `taskboard daemon --no-ipc`
override (flag > file > default). With the knob off, the daemon boots
the full core — instance lock, engine, storage, connectors, system-event
logger, optional web dashboard — and **skips exactly one step**:
`spawn_server`. Every operator surface then degrades through the
*already-typed* no-socket paths that exist today; nothing about them is
new code.

Binding constraints:

- **"The rest of the lifecycle is unchanged" is the contract** (issue
  #194, verbatim). Bootstrap, the instance lock, the actors, the signal
  ladder (D-6/D-7), the drain/shutdown tail, and the kiosk appliance
  branch are all untouched. The only structural change is the server
  handle becoming `Option` — the `web` handle's existing shape
  (main.rs `Option<DashboardServerHandle>`).
- **The degradation is documentation, not invention**: `status`,
  `daemon stop`, one-shot verbs, and the connect/logout nudges already
  have typed no-socket outcomes (ADR 0012). This chunk pins them in
  tests and documents them; it does not add a new error path.
- **AGENTS §5 discipline**: no text assertions (the JSON envelope's
  typed `error.kind` field is the E3-precedented structural pin); no
  fixed sleeps waiting for state (poll a predicate with a deadline);
  assert terminal state. One behavior, one test, lowest layer.
- **AGENTS §2 boundaries**: everything lands in `taskboard-app`
  (`config.rs`, `cli.rs`, `main.rs`, integration tests). No
  domain/state/storage changes; no new dependencies.
- **Licensing**: the touched files all carry their headers already; no
  new files except none — this chunk adds no new source file.
- **`#![forbid(unsafe_code)]`** everywhere (already the crate norm).

## 2. Verified seam facts (do not re-derive; verified 2026-11-01 against `5bf90c63`)

- **The single `spawn_server` call site** is `boot_and_serve`
  (`crates/taskboard-app/src/main.rs:460-466`). It is the shared
  headless boot (ADR 0016): resolve credential store → `bootstrap`
  (typed boot failures, `instance-locked` included) → system-event
  logger → build `BasicSession` → `spawn_server` → web dashboard gate →
  return `(Arc<App>, IpcServerHandle, Option<DashboardServerHandle>)`.
  Both the daemon lifecycle (`daemon_lifecycle`, main.rs:410) and the
  kiosk appliance (`run_kiosk`, main.rs:653) enter it. Desktop/REPL
  never spawn a server — they are out of scope by construction.
- **The gate precedent one step below**: the web dashboard is spawned
  behind `if app.config.web.enabled` (main.rs:475-488) and its handle
  is `Option` through the whole tail — `web.join(budget)` with
  `None => false` (main.rs:575-579). `ipc_hit || web_hit` feeds
  `exit_if_degraded` (main.rs:605). The IPC knob mirrors this shape
  with the boolean inverted.
- **`IpcSection` already exists** (`config.rs:607-638`): the ADR 0012
  client-deadline table, `deny_unknown_fields`, `#[derive(...,
  Default, ...)]`, one `Option<u64>` field, and the section-level
  `skip_serializing_if = "IpcSection::is_default"` on the `AppConfig`
  field (`config.rs:97`) so a pin-less config serializes unchanged.
  `is_default()` currently checks only `client_read_timeout_secs.is_none()`.
- **The WebSection template** (`config.rs:511-597`): hand-written
  `Default` impl, field-level `#[serde(default = "...",
  skip_serializing_if = "...")]`, named `fn(&T) -> bool` skip helpers
  with `#[allow(clippy::trivially_copy_pass_by_ref)]`, and
  `is_default()` as `*self == Self::default()`. Note: `WebSection`'s
  `enabled` defaults **true inside a present table** (presence = opt-in)
  while its struct-default is `false` (absent table = off). The IPC
  section is the opposite on both counts: `enabled` defaults `true`
  (IPC is on for every existing install) and the struct-default is the
  enabled placeholder — hence the derive must go.
- **The `Daemon` command** (`cli.rs:188-191`): `Daemon { cmd:
  Option<DaemonCmd> }`; the run arm is `cmd: None` (matched at
  main.rs:103 and routed to `daemon_lifecycle` at the main.rs:240-244
  fall-through *after* the bare-invocation block returns early).
  `Command::Daemon { cmd: Some(DaemonCmd::Stop) }` is the IpcOnly stop
  lane (main.rs:133). Adding a field makes those two patterns
  non-exhaustive — both gain `..`.
- **The typed no-daemon outcomes exist and are pinned**:
  - `status` (IpcOnly, main.rs:313-353): `client::try_connect` →
    `None` → `StatusView::no_daemon(config.db_path())` → emitted, **exit
    0** ("an observation, not a failure"). The view carries
    `daemon: false` and the lock sidecar's diagnostic holder pid
    (`commands/status.rs:239`).
  - `daemon stop` (IpcOnly, main.rs:357-384): no connection →
    `report(json, &CommandError::NoDaemon)` → **exit 1**;
    `CommandError::NoDaemon` renders the typed kind `"no-daemon"`
    (`commands/mod.rs:307`, `:497`).
  - One-shot core verbs (IpcOrLocal, main.rs:141-171): no socket →
    in-process fallback → the running daemon holds the instance lock →
    typed `instance-locked`, exit 1.
  - connect/logout nudges: `NudgeOutcome::NoDaemon` →
    `DaemonConnectorState::None` (`commands/logout.rs:103,122`,
    `commands/connect.rs:602,620`) — already typed, untouched.
- **The instance lock** (`lockfile.rs`): `InstanceLock::acquire` inside
  `bootstrap` (`bootstrap.rs:982`), an advisory lock on a
  `<db>.lock` sidecar held for the process lifetime; the pid in the
  file body is diagnostic only. The sidecar **file persists** after
  exit — that is what `StatusView::no_daemon` reads.
- **`IpcServerHandle`** (`ipc/server.rs:60-123`): `request_shutdown()`,
  `async stopped(&self)` (via `stopped_from` on a `watch::Receiver`),
  `stopped_receiver() -> watch::Receiver`, `async join(self, budget)
  -> bool` (consumes; removes the socket sidecar). `serve_and_shutdown`
  selects on `server.stopped()` vs rung 1; the kiosk appliance watcher
  clones `server.stopped_receiver()` (main.rs:754-771) and feeds
  `stop_request` so a later signal escalates.
- **The binary test tier** (`tests/cli_ipc.rs`): the E-series owns
  daemon E2E against the real binary. The `Install` harness
  (`Install::new` → `write_test_config` → `taskboard.toml`), `run()`
  (one-shot child, `(exit code, stdout, stderr)`), `wait_ready()`
  polling `status --json` for `"daemon":true` — **a socket-side
  predicate that can never fire on the disabled path** (see D-Q12).
  E2 pins the no-daemon `status` shape (`view["daemon"] == false`);
  E3 pins the typed stop refusal via the JSON envelope's
  `error.kind == "no-daemon"` field (the sanctioned structural pin).
  `tests/daemon_signals.rs` pins exit codes against real signals.
  `tests/common/mod.rs` offers `binary_command()` (scrubs
  `TASKBOARD_*`), `spawn_daemon` (fixed arg list), `wait_until`
  (deadline poll), `Child` (kill-on-drop).
- **Docs homes**: the user-facing config reference is `README.org`
  ("The daemon and IPC" §, line ~553 — currently documents the socket
  as unconditional; the `[ipc]` table is documented nowhere in the
  README today). `docs/architecture.org`'s `ipc` module paragraph
  (line ~635) describes the bind/relay/refusal behavior. `CHANGELOG.md`
  has the `## [Unreleased]` / `### Added` block.

## 3. The design, decision by decision

### S-1 — The config key: `[ipc] enabled`, default `true`, inside the existing `IpcSection`

`crates/taskboard-app/src/config.rs` — the section exists; the key
lands as its second field. No new section, no rename, no deprecation:

```rust
/// `[ipc]`: the daemon's IPC surface plus the one-shot CLI client's
/// daemon-facing deadlines (ADR 0012, amended 2026-10-27). The table's
/// absence is today's behaviour — socket bound, derived budget — and
/// the struct's `Default` is the absent-table placeholder that keeps
/// section-less configs byte-identical on serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IpcSection {
    /// Binds the per-database IPC socket at daemon boot (ADR 0012).
    /// Defaults to `true` — every existing install keeps serving;
    /// `enabled = false` (or `daemon --no-ipc`) skips exactly
    /// `spawn_server`: the core, the instance lock, the actors, and
    /// the optional `[web]` dashboard are unchanged, while `status`
    /// reports the lock-holder pid with `daemon: false` (exit 0),
    /// `daemon stop` fails typed `no-daemon` (exit 1), and one-shot
    /// verbs fall back to the in-process boot (`instance-locked`
    /// against the running core). The disabled daemon stops on a
    /// signal only. Skipped when on so the absent-table placeholder
    /// serializes to nothing.
    #[serde(
        default = "default_ipc_enabled",
        skip_serializing_if = "ipc_enabled_is_on"
    )]
    pub enabled: bool,
    // `client_read_timeout_secs` verbatim, doc and all.
}

impl Default for IpcSection {
    fn default() -> Self {
        Self { enabled: true, client_read_timeout_secs: None }
    }
}

impl IpcSection {
    /// The absent-table test behind `skip_serializing_if`: the
    /// placeholder (socket on, no override) serializes to nothing, so
    /// a config without `[ipc]` renders unchanged.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn default_ipc_enabled() -> bool {
    true
}

// serde's `skip_serializing_if` demands the `fn(&T) -> bool` shape.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn ipc_enabled_is_on(enabled: &bool) -> bool {
    *enabled
}
```

Exact edits:

- Remove `Default` from the `IpcSection` derive (a derived default
  would mint `enabled: false` — the derive is the bug this design
  exists to avoid; `WebSection` is the precedent for the hand-written
  impl).
- `is_default()` switches from the field check to `*self == Self::default()`
  (PartialEq is already derived; the `WebSection::is_default` shape).
  Consequence: an explicit `enabled = true` in a config file is dropped
  on rewrite — semantically identical, the same class as
  `[web] enabled = true` serialization today; the field-level skip is
  what makes `is_default()`-skipped sections byte-identical.
- The `AppConfig.ipc` field doc (`config.rs:95-97`) gains half a
  sentence: the section also carries the daemon's socket toggle.
- **No `validate` arm**: a `bool` has no invalid state to refuse;
  `validate` guards value *grammars* (floors, URL shape, durations) and
  nothing else. Recorded in §7 (D-Q8).

The issue's `default_true` helper name is realized as
`default_ipc_enabled` — the house names its serde default fns after
their section (`default_web_enabled`, `default_shutdown_timeout`).

### S-2 — The override: `daemon --no-ipc`, flag > file > default

`crates/taskboard-app/src/cli.rs` — one field on the run variant:

```rust
/// Run the headless sync daemon (M1) — boots the core, binds the
/// per-database IPC socket, and serves commands until a signal or
/// `daemon stop`.
Daemon {
    #[command(subcommand)]
    cmd: Option<DaemonCmd>,
    /// Boot the core without binding the IPC socket (`[ipc] enabled
    /// = false` as a flag; the flag wins over the file). Only the run
    /// arm reads it — `daemon stop --no-ipc` parses and ignores it
    /// (a stop never spawns a server).
    #[arg(long)]
    no_ipc: bool,
},
```

`crates/taskboard-app/src/main.rs` — the precedence mechanics:

- `daemon_run` (main.rs:103) becomes
  `matches!(cli.command, Some(Command::Daemon { cmd: None, .. }))`.
- Before `cli.command.take()` (main.rs:104), extract the flag once:
  `let daemon_no_ipc = matches!(&cli.command, Some(Command::Daemon { no_ipc: true, .. }));`
- The stop lane pattern (main.rs:133) becomes
  `Command::Daemon { cmd: Some(DaemonCmd::Stop), .. }`.
- At the daemon-run fall-through (main.rs:240-244), after
  `load_migrated` succeeds:

  ```rust
  let config = if daemon_no_ipc {
      let mut config = config;
      config.ipc.enabled = false;
      config
  } else {
      config
  };
  daemon_lifecycle(config, cli.config, cli.json).await
  ```

That is the whole precedence story: the file key is decoded by
`AppConfig::load`/`load_migrated` (default `true` when the table is
absent — S-1), and the flag unconditionally overwrites it to `false`
after the load. `false` wins over `true` in both layers; there is no
tri-state, no extra figment provider, no re-read. The flag is a
presence, not a toggle: nothing re-enables a file-disabled socket from
the CLI (there is no `--ipc` — YAGNI; nobody's hardening story needs to
override a `false` back on, and the config file is the fix if one ever
does).

Scope of the flag: **the `daemon` verb only** (the issue title's
shape). Bare `taskboard` (no verb) and bare `taskboard kiosk` have no
verb to attach to — for those entries `[ipc] enabled = false` in the
file is the knob. Recorded as D-Q4.

### S-3 — The gate: one `if`, inside `boot_and_serve`

`crates/taskboard-app/src/main.rs` — the single `spawn_server` call
site (main.rs:460-466) becomes:

```rust
let server = if app.config.ipc.enabled {
    match taskboard_app::ipc::server::spawn_server(app.clone(), session, app.config.db_path())
        .await
    {
        Ok(server) => Some(server),
        Err(err) => return Err(err.into()),
    }
} else {
    None
};
if server.is_some() {
    tracing::info!(
        db = %app.config.db_path().display(),
        "{what} serving commands over ipc"
    );
} else {
    tracing::info!(
        db = %app.config.db_path().display(),
        "{what} running with ipc disabled; no command surface"
    );
}
```

`boot_and_serve`'s return type becomes
`(Arc<App>, Option<taskboard_app::ipc::server::IpcServerHandle>,
Option<taskboard_app::web::DashboardServerHandle>)`.

Everything above the gate is untouched by construction: `bootstrap`
(acquire → migrate → actors → connectors), the system-event logger, and
the session build run identically; the instance lock is held either
way; the `[web]` gate below it is independent (a no-ipc daemon may
still serve the read-only dashboard — that combination is legal and
needed as the E11 readiness instrument, §5).

### S-4 — The tail: `Option<IpcServerHandle>` through serve/drain/kiosk

The `web` handle's existing shape, applied once:

- `serve_and_shutdown(app, server: Option<IpcServerHandle>, web, signals)`
  (main.rs:500-516): the select's second arm becomes
  `server.as_ref()` matched — `Some(server) => server.stopped().await`,
  `None => std::future::pending::<()>().await`. On the disabled path
  `daemon stop` cannot arrive (there is no socket to receive it on), so
  `pending()` is the honest arm: the only exit is the signal ladder's
  rung 1.
- `drain_and_shutdown(app, server: Option<IpcServerHandle>, web, signals)`
  (main.rs:559-607): phase A's `ipc_hit` becomes
  `match server { Some(server) => server.join(shutdown_timeout).await,
  None => false }` — the literal `None => false` the web arm already
  has one line below. `exit_if_degraded(outcome, ipc_hit || web_hit)`
  is unchanged (an absent server contributes no degraded hit).
- The kiosk appliance watcher (main.rs:754-771):
  `let stop_rx = server.as_ref().map(taskboard_app::ipc::server::IpcServerHandle::stopped_receiver);`
  and the watcher's second select arm gets the same
  `Some(stopped_from(rx)) / None => pending()` treatment. `daemon
  stop` as the kiosk window's twin exit degrades to signals + window
  close — exactly the documented cost of the knob.
- All existing `drain_and_shutdown` call sites (serve tail, kiosk
  fixed-board error, kiosk mount error, kiosk appliance tail) pass the
  handle through unchanged in type — they already hold whatever
  `boot_and_serve` returned.

### S-5 — The kiosk headless fallback: continues, honestly

`KioskStart::Headless` (main.rs:687-693) calls the shared
`serve_and_shutdown`. With the knob off it keeps running as a
windowless core with no command surface — the same shape as a no-ipc
daemon, which is precisely what the issue ordered ("the rest of the
lifecycle is unchanged"). No behavior fork, no refusal: the fallback's
contract is *keeps running* (ADR 0016 Q3), and the S-3 log line is
where the honesty lives. The existing
`"no display available; continuing as headless daemon"` line stays as
is.

### S-6 — The degradation surfaces: already typed, now pinned and documented

No client-side code changes. The knob reduces the operator surface to
exactly today's no-socket outcomes (§2), which the tests pin and the
docs state:

| Surface | Disabled-path outcome | Where it lives today |
|---|---|---|
| `status [--json]` | exit 0; `daemon: false`; the lock sidecar's holder pid (the running daemon's own pid) | `StatusView::no_daemon`, main.rs:340 |
| `daemon stop` | typed `no-daemon` envelope, exit 1; the daemon stays alive | `CommandError::NoDaemon`, main.rs:367 |
| one-shot core verbs (`tasks add`…) | in-process fallback → typed `instance-locked`, exit 1 | IpcOrLocal lane, main.rs:141-171 |
| connect/logout nudges | `NudgeOutcome::NoDaemon` → `DaemonConnectorState::None` | `logout.rs:103`, `connect.rs:602` |
| stopping the process | signals only (the D-6 ladder; rung 1 → drain → owned shutdown → exit 0) | unchanged |

The one-line honesty summary for the docs: *a no-ipc daemon is a
lock-holding core with no command surface — nothing reaches it over
IPC, one-shot boots are refused by the lock, and a signal is the only
stop; the optional `[web]` dashboard keeps working.*

### S-7 — Docs & CHANGELOG

- **`README.org`**, "The daemon and IPC" § (after the socket paragraph,
  ~line 566): a new paragraph plus a toml block:

  > The IPC surface is optional (minimal-image hardening, #194):
  > `[ipc] enabled = false` in the config — or `taskboard daemon
  > --no-ipc` for one run (the flag wins over the file) — boots the
  > daemon core without binding the socket. Everything else is
  > unchanged: the instance lock, sync, reminders, and the optional
  > `[web]` dashboard all run. What degrades is every IPC client:
  > `status` reports `daemon: false` and the lock-holder pid (exit 0),
  > `daemon stop` fails with the typed `no-daemon` error (exit 1), and
  > one-shot verbs fall back to the in-process boot and hit
  > `instance-locked` while the daemon runs. A no-ipc daemon stops on a
  > signal only — `daemon stop` has no socket to reach it on.
  >
  > ```toml
  > [ipc]
  > # enabled = true                # the default: bind the per-db socket
  > # client_read_timeout_secs = 45 # one-shot client read budget override
  > ```

  (The block doubles as the first README documentation of the existing
  `client_read_timeout_secs` key — the table was undocumented user-side
  until now.)
- **`docs/architecture.org`**, the `ipc` module paragraph (line ~636,
  after "…`=status=/=daemon stop=` are the operator verbs over the
  wire."): one clause — "=The socket is optional: =[ipc] enabled= (or
  =daemon --no-ipc=, #194) skips =spawn_server= and the daemon runs
  lock + actors + web with no command surface; the operator verbs
  degrade to their typed no-socket outcomes and the kiosk headless
  fallback inherits the same shape." The bootstrap paragraph (line
  ~563: "the IPC inbound + stop watch in =spawn_server=") needs no
  edit — still true whenever the server exists.
- **`CHANGELOG.md`**, `## [Unreleased]` / `### Added`, one bullet:

  > - IPC disable knob (#194): `[ipc] enabled = false` in the config —
  >   or `taskboard daemon --no-ipc` for one run — boots the daemon
  >   core without binding the per-database socket. The instance lock,
  >   the actors, and the optional web dashboard are unchanged, while
  >   `status` degrades to the client-side no-daemon view (exit 0),
  >   `daemon stop` fails typed `no-daemon` (exit 1), and one-shot
  >   verbs hit `instance-locked` against the running core; a signal is
  >   the only stop. Default stays enabled; existing configs are
  >   untouched.

## 4. What does NOT change

- **`taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`, `taskboard-ui-slint`**: zero lines. The
  knob is app-layer process glue.
- **`bootstrap`** (`bootstrap.rs`): lock acquisition, migrations, actor
  spawns, connector registry, system-event logger — all run on the
  disabled path exactly as today.
- **The client side** (`ipc/client.rs`, `route_for_ipc`, the
  `IpcRoute` lanes, `client_read_budget`): untouched — the degradation
  *is* the existing no-socket routing. `client_read_timeout_secs`
  semantics unchanged (it bounds a connected exchange; with no daemon
  it never gets a chance to matter).
- **The signal ladder** (D-6/D-7): rung semantics, escalation, salvage,
  exit codes — untouched. `daemon_signals.rs` must stay green with zero
  edits.
- **The enabled path**: every existing E-test in `cli_ipc.rs`, every
  `ipc.rs` server unit test, and E1's stop-then-fallback journey pass
  byte-identically. A config without `[ipc]` serializes unchanged
  (S-1's byte-identity law).
- **`[web]`**: independent of the knob in both directions (web without
  ipc is legal — the minimal-image appliance reads its dashboard; ipc
  without web is today's default).
- **The lock** (`lockfile.rs`): the disabled daemon holds it, a second
  boot (daemon, gui, or one-shot verb) is refused `instance-locked`
  exactly as today.

## 5. Test plan (TDD order — AGENTS §5 discipline throughout)

Red-green per stage; the binary tests are written before the gate
lands and must fail for the *right* reason (socket present → E11's
absence assertion red; flag unknown → E12 fails at spawn).

### Config unit tier (`crates/taskboard-app/src/config.rs` tests, the `ipc_section_*` block)

- **T1 — update** `ipc_section_absent_serializes_nothing_and_decodes_default`:
  additionally asserts `config.ipc.enabled` is `true` after decoding an
  absent table, and (unchanged assertion) the value still serializes to
  nothing. Same behavior, one test, lowest layer — not a new test.
- **T2 — new** `ipc_section_enabled_false_decodes_and_serializes`:
  `[ipc]\nenabled = false\n` decodes to `enabled == false`,
  `!config.ipc.is_default()`, and the serialized value carries the key
  (`value["ipc"]["enabled"] == false`). The opt-out round-trips.
- **T3 — new** `ipc_section_enabled_true_round_trips_to_nothing`:
  explicit `enabled = true` → `is_default()` → serializes to nothing.
  The byte-identity law's positive side (an explicit default is
  indistinguishable from absence — the S-1 rewrite consequence, pinned
  as the *contract*).
- **T4 — new** `ipc_section_default_is_enabled`:
  `IpcSection::default()` has `enabled == true` and
  `client_read_timeout_secs == None`. Guards the hand-written `Default`
  against a future re-derive regression (the derive would silently
  mint `false`).

### CLI parse tier (`crates/taskboard-app/src/cli.rs` tests)

- **T5 — new** `daemon_no_ipc_flag_parses`:
  `Cli::try_parse_from(["taskboard", "daemon", "--no-ipc"])` → the
  `Daemon` arm with `no_ipc` true; `["taskboard", "daemon"]` → false.
- **T6 — new** `daemon_stop_accepts_no_ipc`:
  `["taskboard", "daemon", "stop", "--no-ipc"]` parses (the
  accept-and-ignore contract is parseable surface, pinned so a future
  clap change cannot silently turn it into a usage error).

### Binary tier (`tests/cli_ipc.rs`, the E-series — the real binary on both ends)

- **E11 — new** `e11_ipc_disabled_by_config_degrades_every_surface`:

  - Config: the `Install` fixture plus `[ipc]\nenabled = false\n` and
    `[web]\nenabled = true\n` (loopback default). The web row is the
    **readiness instrument** (D-Q12): the dashboard binds strictly
    after the IPC gate inside `boot_and_serve`, and the lifecycle
    watcher installs synchronously one function-call after
    `boot_and_serve` returns — so "HTTP answers" implies "the gate was
    evaluated and the watcher is armed", with the gap invisible at the
    harness's 50 ms poll beat. This is the same tier-sanctioned polled
    readiness the E-series uses today, moved one observable later
    because the socket-side predicate (`"daemon":true`) can never fire
    here.
  - Readiness: poll `http://127.0.0.1:8787` (the `[web]` default bind;
    a plain TCP connect suffices — `tokio::net::TcpStream::connect`)
    under `common::wait_until`.
  - Assert the socket sibling (`taskboard.sock` beside the db) **does
    not exist**.
  - `--json status`: exit `Some(0)`; parse the payload;
    `view["daemon"] == false`; `view["pid"]` equals the daemon child's
    pid (the no-daemon view reports the lock sidecar's holder —
    proving the core runs while nothing serves).
  - `--json daemon stop`: exit `Some(1)`; the stderr envelope's last
    line parses as JSON with `error.kind == "no-daemon"` (the E3
    pin's exact mechanism).
  - The daemon child is still alive after the refused stop
    (`try_wait().is_none()`) — the degradation's headline fact.
  - `tasks list`: exit `Some(1)` (the in-process fallback meets the
    held lock — `instance-locked`; assert the exit code and, under
    `--json`, the envelope's `error.kind == "instance-locked"`.
    Typed-kind only, no prose.)
  - Tail: `kill(SIGTERM)` to the child (the `daemon_signals.rs` nix
    pattern), reap under a deadline → exit code **0** (the ladder's
    clean drain is untouched by the knob — rung 1 fires, the drain
    finds no server to join, the owned shutdown runs). Afterwards the
    socket still never existed.

- **E12 — new** `e12_no_ipc_flag_overrides_an_enabled_file`:

  - Config: the `Install` fixture plus an explicit
    `[ipc]\nenabled = true\n` — the file says *on*.
  - Spawn `taskboard --config <cfg> daemon --no-ipc` via a local
    helper built on `common::binary_command()` (the shared
    `spawn_daemon` keeps its fixed arg list; one call site does not
    justify common churn).
  - Readiness: poll the `<db>.lock` sidecar's existence under
    `wait_until` (no signal is sent in this test, so the
    lock-to-watcher window is irrelevant — the pinned facts are
    time-independent on the disabled path).
  - Assert the socket sibling never exists; `--json daemon stop` →
    exit `Some(1)` + `error.kind == "no-daemon"`; child still alive.
  - This is the flag > file precedence pin: file on, flag off, socket
    off.

### Explicitly no tests for

- **The serve tail's `pending()` arm in isolation** — it has no
  observable other than "SIGTERM still exits 0", which E11's tail
  pins end-to-end; a synthetic handle stub would test the stub.
- **`status`'s no-daemon *view contents*** beyond `daemon`/`pid` —
  `commands/status.rs` unit-tests the `no_daemon` shape exhaustively
  already; E11 asserts only the binary-tier routing facts.
- **The kiosk paths** — both appliance and headless fallback enter the
  same `boot_and_serve` gate and the same tail the daemon tier
  exercises; a kiosk binary test would duplicate the gate pin through
  a second process shape (one behavior, one test, lowest layer). The
  architecture.org clause is the kiosk's documentation.
- **The enabled path** — owned by the existing E1–E10/T16, `ipc.rs`,
  `daemon_signals.rs` suites, which must pass with zero edits.

## 6. Implementation order (stage-2 guidance)

1. **S-1** config key + **T1–T4** (red → green). Commit-sized unit:
   pure serde, no behavior.
2. **S-2** flag + extraction + **T5–T6** (red → green; the flag is
   inert until step 3 — parse-only, so no behavior claims yet).
3. **S-3 + S-4** the gate and the `Option` tail threading (the
   main.rs pattern-site fixes from §2 included). Compile gate:
   `cargo clippy --workspace --all-targets` must be clean.
4. **E11 + E12** (red → green) — written *before* step 3 is believed
   done; if they pass before the gate exists, they are testing
   nothing and are wrong.
5. **S-7** docs: README, architecture.org, CHANGELOG.
6. Quality gates through `scripts/run-quality-gates.sh` (the parallel
   -worktree wrapper), then hand off at PR open + `gh pr checks` green.

Commit hygiene: each commit builds and carries a meaningful
`<type>(<module>)` subject (`feat(config): …`, `feat(cli): …`,
`feat(daemon): …`, `test(ipc-e2e): …`, `docs: …`) — rebase-merge
drops them verbatim on `main`.

## 7. Decision record (every open question, resolved)

- **D-Q1 — Section/key name?** `[ipc] enabled`, second field of the
  existing `IpcSection` (`config.rs`). The issue's shape verbatim; no
  new section (`[ipc]` is the ADR 0012 home; a sibling `[daemon]`
  section would fragment one subsystem's keys across tables).
- **D-Q2 — Default value?** `true`. The issue's `default_true`; IPC is
  always-on in v1 and every existing config must be untouched
  (byte-identity pinned by T1/T3). Realized as `default_ipc_enabled`
  per the house naming rule.
- **D-Q3 — Flag placement?** On the `Command::Daemon` variant. Only
  the run arm reads it; `daemon stop --no-ipc` parses and ignores
  (documented in the flag's doc comment; T6 pins the parse). Rejected:
  a clap-level rejection for the stop arm (over-engineering a
  meaningless combination) and a global `--no-ipc` on `Cli` (would
  promise effects on verbs that never spawn — desktop/REPL don't even
  have a server).
- **D-Q4 — Flag scope: daemon verb only?** Yes. Bare `taskboard` and
  bare `taskboard kiosk` have no verb to flag; the file key covers
  them. The issue titles the flag `daemon --no-ipc`; that is the whole
  scope.
- **D-Q5 — Override mechanism?** Post-load mutation
  (`config.ipc.enabled = false` after `load_migrated` at the run-arm
  fall-through). No tri-state enum, no extra figment provider, no
  second decode. Precedence flag > file > default falls out of the
  layering. There is deliberately no `--ipc` re-enable: a presence
  flag that can only turn the surface off matches the hardening use
  case; the file is the place for "on".
- **D-Q6 — Gate site?** `boot_and_serve` only — the single
  `spawn_server` call site. Daemon, kiosk appliance, and the kiosk
  headless fallback inherit; nothing else in the workspace spawns a
  server (verified by grep, §2).
- **D-Q7 — Tail shape?** `Option<IpcServerHandle>` through
  `serve_and_shutdown`/`drain_and_shutdown`/the kiosk watcher — the
  `web` handle's existing shape, including the `None => false` hit
  contribution. Rejected: a never-stopping dummy handle (it would hold
  a fake sidecar path and lie at `join`), and a separate disabled tail
  function (duplicates the ladder wiring the contract says is
  unchanged).
- **D-Q8 — `validate` arm?** None. A bool has no invalid state;
  `validate` guards value grammars (URL shape, floors, durations) and
  adding a tautology would be noise.
- **D-Q9 — Does the knob touch the client side?** No.
  `client_read_timeout_secs`, `route_for_ipc`, the lane table, and the
  nudge mapping are untouched; the degradation is the existing
  no-socket routing (§2, §4).
- **D-Q10 — Kiosk headless fallback with the knob off: refuse or
  continue?** Continue (issue: the rest of the lifecycle is
  unchanged). It becomes a windowless, socketless, signal-driven core
  — the same shape the operator explicitly chose for the daemon. The
  S-3 log line carries the honesty; no behavior fork, no new kiosk
  test (§5).
- **D-Q11 — `status` degradation semantics?** Exit **0** with the
  `no_daemon` view. The issue says "reducing `status`/`daemon stop` to
  typed `no-daemon`" — for `status` the typed outcome is the
  client-side view (an observation of "nothing serves", not a command
  failure; the existing contract at main.rs:309-312), for `daemon
  stop` it is the exit-1 envelope. Both are pinned (E2's shape, E3's
  mechanism) — nothing new to invent.
- **D-Q12 — Binary-test readiness predicate when the socket never
  exists?** E11: the `[web]` dashboard's loopback bind, polled by TCP
  connect — it binds strictly after the gate, and the lifecycle
  watcher installs synchronously after `boot_and_serve` returns, so
  the SIGTERM in the test's tail cannot race the boot (the
  lock-sidecar predicate would carry a real lock-acquired→watcher-
  armed window across migrations and actor spawns). E12 sends no
  signal, so the cheap lock-sidecar existence poll suffices there.
  The house's `"daemon":true` poll is unusable on this path by
  construction.
- **D-Q13 — New test file or extend `cli_ipc.rs`?** Extend: the
  E-series owns daemon E2E, the `Install` harness is already there,
  and E11/E12 reuse `run()`-style invocations. No `common/` changes.
- **D-Q14 — Explicit `enabled = true` is dropped on config rewrite —
  acceptable?** Yes: semantically identical to absence (the default),
  the same class as `[web] enabled = true` today, and the round-trip
  law (absent table renders unchanged) is the invariant actually
  pinned (T1/T3).

## 8. Risks

- **R-1 — The `Option` threading touches the shutdown tail** — the
  most safety-annotated code in the workspace (D-6 salvage ordering,
  §10 Q5/Q9/Q10 annotations). Mitigation: the tail's enabled path is
  exercised unchanged by E1 (stop over IPC), `daemon_signals.rs`
  (every ladder code), and the `ipc.rs` server suite — all of which
  must stay green with zero edits; the disabled arm is `pending()` +
  `false`, the two literal shapes the web arm already uses one line
  away.
- **R-2 — The hand-written `Default` regression class**: a future
  field added to `IpcSection` with the derive restored would mint
  `enabled: false` for absent tables — a silent behavior flip for
  every install. Mitigation: T4 pins the default shape; the section
  doc comment states why the derive is absent; `is_default()` via
  `*self == Self::default()` keeps the placeholder rule mechanical.
- **R-3 — The readiness predicate is load-bearing for E11's SIGTERM**:
  substituting the cheaper lock-sidecar poll would introduce a real
  boot race (default SIGTERM disposition mid-boot → death by signal →
  the reaper's `unwrap_or_else` panic). The web-bind instrument is
  recorded as non-negotiable in D-Q12; the plan's §5 text is the
  guard.
- **R-4 — User surprise: a no-ipc daemon is an unreachable core.** The
  lock still refuses one-shot boots (`instance-locked`), `daemon stop`
  fails, and only signals stop the process — an operator who disabled
  IPC "temporarily" may not expect the CLI to go deaf. Mitigation: the
  README paragraph states all three consequences in one breath; the
  CHANGELOG bullet repeats them; the S-3 log line says "no command
  surface" at every disabled boot. No code mitigation is proposed —
  this is the knob's honest meaning.
- **R-5 — `daemon stop --no-ipc` silently ignores the flag.** A
  documented wart (D-Q3): the alternative — a clap exclusion — spends
  machinery on a combination with no failure mode worse than
  confusion.
- **R-6 — E11's `[web] enabled = true` couples two knobs in one
  test.** The coupling is the readiness instrument, not the assertion
  surface (every assertion is about the IPC path); the web row also
  exercises the legal web-without-ipc combination for free. Accepted.
- **R-7 — Concurrent-round drift**: chunk 1.x siblings touch
  `taskboard-app` too (the dispatch plan runs six batches in
  parallel). Mitigation: this chunk's main.rs footprint is
  `boot_and_serve`/tail signatures and the two pattern sites; rebase
  conflicts, if any, are mechanical. The branch is based on
  `5bf90c63` and rebased onto fetched `origin/main` before the PR
  opens.

## 9. Estimated diff size

| File | Delta |
|---|---|
| `crates/taskboard-app/src/config.rs` | ~+45 −10 (field, helpers, hand-written `Default`, `is_default`, doc; T1–T4) |
| `crates/taskboard-app/src/cli.rs` | ~+14 −4 (flag + doc; T5–T6) |
| `crates/taskboard-app/src/main.rs` | ~+35 −22 (gate, log split, `Option` through 3 fns + watcher, 2 pattern sites) |
| `crates/taskboard-app/tests/cli_ipc.rs` | ~+130 (E11, E12, local no-ipc spawner) |
| `README.org` | ~+20 (daemon-and-IPC paragraph + toml block) |
| `docs/architecture.org` | ~+5 (one clause) |
| `CHANGELOG.md` | ~+9 (one bullet) |

Total ≈ **260 lines** across 7 files, one coherent deliverable → one
branch/PR (`feat/ipc-disable-knob`), no stacking.

## 10. References

- Issue #194 (normative): `backlog: IPC disable knob (daemon
  --no-ipc / [ipc] enabled)` — labels `dx`, `p3`.
- `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` — the
  round-2 dispatch this chunk belongs to.
- `.artifacts/plans/2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md`
  §7 — the issue's origin (WI-5, the IPC frontend plan).
- ADR 0012 (the IPC frontend), ADR 0016 (the kiosk appliance /
  headless fallback), ADR 0008/0009 (bootstrap, the instance lock) —
  `docs/adr/`.
- `docs/architecture.org` (the `ipc` module paragraph, the lifecycle
  vocabulary), `README.org` ("The daemon and IPC").
- Seam files read for this plan: `crates/taskboard-app/src/{config.rs,
  cli.rs, main.rs, lockfile.rs, bootstrap.rs, ipc/server.rs,
  ipc/client.rs, commands/{mod,status,logout,connect}.rs}`,
  `tests/{cli_ipc.rs, daemon_signals.rs, common/mod.rs}`.
