# [Plan] REPL Session Ceremonies, Masked Prompts & ADR 0014 (WI-J.3)

- **Date**: 2026-10-14
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: design round
  `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` (direction
  approved 2026-10-12, round 3) §4.4 (session choreography), §4.6
  (in-session logout — the round-3 WI-H-D7 scope addition), §5 (the
  disposition handshake), §6 (GUI reuse constraints), §9 (ADR 0014
  draft), §12 chunk WI-J.3; dispatch plan
  `.artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md`
  §1/§4 (chunk 3).
- **Branch / worktree**: `feat/app-connect-session` in
  `../taskboard-WT-feat-app-connect-session`, off `main` @ `c29b679`
  (chunks 1 `feat/connect-engine-attach` and 2 `feat/app-connect-verb`
  verified merged on `origin/main` before planning started).
- **Goal**: the REPL session gains the connector ceremonies — a
  `connect` (or deprecated-alias `login`) line runs the chunk-2
  ceremony through a new editor-thread **disposition handshake**
  (masked secrets on the reader thread, never in history), then
  refreshes the session config and attaches chunk-1's
  `App::attach_connector`; a `logout` line runs §4.6's mirror (local
  teardown → `detach_connector` → config refresh). Both ADR 0011
  session refusals die. Final commits formalize ADR 0014, amend the
  docs, and mark both origin backlog entries ✅ DONE.
- **Status**: awaiting stage-2 implementation.

---

## 1. Verified starting point (anchored on main @ c29b679)

1. **Chunk 1 landed**: `App.connectors: Mutex<Vec<SyncActor>>` with the
   ≤ 1 guard, the permanently parked reports sender,
   `App::attach_connector(&self, client) -> Result<(), AttachError>`
   (`AlreadyAttached` / `Engine` / `Draining`; re-attach after detach
   permitted — B10–B13 pin it), `detach_connector`, `connector_attached`,
   and the shutdown drop-ordering step (bootstrap.rs).
2. **Chunk 2 landed**: `commands/connect.rs` owns the frontend-agnostic
   ceremony `run(config_path, raw, args, prompts, store)` (guards →
   gather → verify → persist → store), the `PromptReader` trait
   (`read_line` / `read_secret` / `read_confirm`, sync, used as
   `&mut dyn`), `ConnectArgs { server, user, switch_backend, force,
   confirmable }`, `ConnectReceipt`, the typed refusals, and the
   one-shot wrappers (`connect_command`, `login_command` with the
   stderr deprecation notice). `login.rs` is gone (body absorbed).
3. **The session loop owns `App` by value and `ReplSession` immutably**
   (repl.rs): `repl_loop(app: App, session: ReplSession, lines, io)`;
   `resolve_words` (shared with the daemon IPC server's `run_argv`,
   server.rs:476) refuses `connect`/`login`/`logout` at parse level via
   `LineRefusal::{Connect, Login, Logout}` (`Connect` is documented as
   chunk 3's interim dead surface), and `dispatch_line` holds an
   `unreachable!` arm for exactly `Connect|Login|Logout|Config|Daemon`.
4. **The editor thread** (`spawn_line_reader`) owns rustyline, adds
   every non-empty submitted line to history, and streams
   `LineEvent::{Line, Eof, Interrupted}` through `mpsc::channel(1)`.
   Tests inject their own channel via the `lines` factory of
   `run_session_with_lines` — the terminal never appears in hermetic
   suites.
5. **`run_argv` routes `LineAction::Execute` straight into
   `dispatch_line`** (ipc/server.rs:477–491). Lifting the parse-level
   refusals without a server-side guard would turn a relayed `connect`
   into that `unreachable!` panic — the interception design (§4) must
   cover both consumers of `resolve_words`.
6. **The main runtime is `new_multi_thread()`** (main.rs:71) — but a
   1-core kiosk gets a single worker from it, so "never park a runtime
   worker on input" (repl.rs header, ADR 0011) remains a hard design
   constraint for the prompt bridge (decision D1 below).
7. **Config seams**: `AppConfig::load(explicit_path)` (validated,
   figment: defaults → file → env) is the boot resolution;
   `AppConfig::extract_raw(path)` is the ceremony's pre-read seam
   (tolerates not-yet-valid files); `write_path`/`config_path` resolve
   the target. `ReplSession` carries no path today.
8. **`logout::run(config_path, raw, store)` is already
   process-shell-free** (mod.rs's `logout_command` adds the daemon
   nudge around it); the WI-H riders `CommandError::SyncUnavailable`
   (`sync` → engine `NoOp` on a dead channel) and the sync preludes'
   `sync-disabled` (config-level) both exist.
9. **Test assets**: `tests/common/mod.rs` has `FakePrompts` (scripted
   `PromptReader`), `FakeCredentialStore` (in-memory store/load/delete
   round-trips), `EnvOverride`/`env_lock`; `tests/repl.rs` (R-series)
   drives full sessions through injected line scripts; B-series proves
   wiremock + env-store boots; `tests/cli_tty.rs` wraps the binary in
   `script(1)` ptys (chunk 2's pty journeys use it).
10. **Completion** keeps `REFUSED_VERBS = [daemon, connect, login,
    logout, config]` (completion.rs:230) and pins that set in its spec
    -walk test.

## 2. Scope

**In**: repl.rs (session loop interception + disposition protocol +
config refresh), a new `session_ceremonies.rs`, `commands/connect.rs`
(prompt-seam signature, receipt field), `commands/logout.rs` +
`commands/output.rs` (session receipt field), `commands/mod.rs` (thin
plumbing), `completion.rs`, `ipc/server.rs` (terminal-bound guard),
`main.rs` (one argument), tests (repl.rs, common/mod.rs,
cli_connect.rs migration, ipc refusal pins, optional pty journey), and
the final docs commits (ADR 0014, architecture.org, ADR 0011/0012
amendment notes, backlog ✅).

**Out** (recorded, not silently dropped): daemon-side attach over IPC
(rides WI-H's additive-verb pattern, a future round); the
multi-connector lift (backlog entry stays open); in-session `status`
surfacing `connector_attached` (today's `StatusView::in_session()`
answers `None`; the rider named in design §4.6 materializes through
`sync`'s typed refusals, which the journeys pin — a one-line honesty
improvement to `status` is appended to the improvements backlog per
AGENTS §7, not smuggled into this chunk); `login`-alias removal
(v0.1.0 release prep).

## 3. The disposition protocol (design §5.2, exact)

### 3.1 Types (repl.rs, `pub` for the integration tests, like `LineEvent`)

```rust
/// What one terminal input the running handler needs.
pub enum PromptKind { Line, Masked, Confirm }

/// The answer the editor thread owes for a prompted read.
pub enum PromptValue { Text(String), Flag(bool) }

/// The session loop's per-line verdict to the editor thread.
pub enum Disposition {
    /// The line is fully handled; resume editing.
    Next,
    /// The running handler needs one more terminal input: perform the
    /// read on the editor thread and answer through the oneshot.
    Prompt {
        label: String,          // full prompt text, as PromptReader got it
        kind: PromptKind,
        reply: oneshot::Sender<Result<PromptValue, std::io::Error>>,
    },
}

/// What `spawn_line_reader` hands the session loop.
pub struct EditorIo {
    pub lines: mpsc::Receiver<LineEvent>,      // existing channel(1)
    pub dispositions: mpsc::Sender<Disposition>, // new channel(1)
}
```

The oneshot-inside-the-mpsc-message is AGENTS §3's own
request-response idiom; no new dependencies.

### 3.2 State machine (who blocks on what)

Editor thread (sync, terminal owner):

```
loop {
    match editor.readline(PROMPT) {
        Ok(line)   → history (non-empty only) → blocking_send(Line)  ─┐
        Interrupted → blocking_send(Interrupted)                     ─┤
        Eof / other → blocking_send(Eof); return                     ─┘ end
    }
    // unconditional: never re-arm readline before the loop's verdict
    match dispositions.blocking_recv() {
        None            → return,                 // loop dropped us (session over)
        Next            → continue,               // back to readline
        Prompt{label, kind, reply} → {
            let answer = match kind {              // StdinPrompts' sync cores
                Line    → prompts.read_line_sync(&label).map(PromptValue::Text),
                Masked  → prompts.read_secret_sync(&label).map(PromptValue::Text),
                Confirm → prompts.read_confirm_sync(&label).map(PromptValue::Flag),
            };
            let _ = reply.blocking_send(answer);   // loop gone ⇒ session ending
            // fall through to blocking_recv again: handlers prompt repeatedly
        }
    }
}
```

Session loop (async): for every `LineEvent` it consumes it **sends
exactly one verdict** — `Next` after a handled line or an `Interrupted`
`continue`; `Prompt{..}` per ceremony prompt (0..~5 per line), each
followed by further dispositions until the ceremony ends. `Eof` and
`Exit` end the loop; the dropped senders are the editor's exit signal
(`blocking_recv() → None`) — no final `Next` needed.

**Why race-free where a poll-between-lines design is not**: the naive
design (loop posts prompt requests to a channel the editor polls
between `readline` calls) races because after delivering `Line(…)` the
editor re-arms `readline` immediately — the request sits unanswered
behind the user's next keystrokes and the *next submitted line* would
be consumed as the answer. The handshake inverts control: the editor's
post-send wait is **unconditional**, so there is no interleaving in
which `readline` is re-armed while a prompt is outstanding. Every
message has exactly one sender and one receiver at a time; no polling,
no shared state. Corollaries pinned by tests: an `Eof`/`Interrupted`
*event* cannot arrive mid-ceremony from the real editor (it is not in
`readline` then), and the answer path never touches the `LineEvent`
channel or history (§3.5).

**Backpressure**: unchanged in spirit — one line in flight (the lines
channel keeps its capacity 1), the dispositions channel is capacity 1
(strict alternation: the loop only sends after receiving an event; the
editor is always parked on `blocking_recv` when the loop is free to
send), and at most one oneshot is outstanding. A ceremony that never
gets answered parks the session exactly as long as the user thinks —
by design (the loop is serial; nothing else may run a line).

**Ctrl-C during a prompted read** (design §5.2's cancellation): any
read failure on the editor thread answers `Err(io::Error)` through the
oneshot; `ReplPrompts` propagates it; the ceremony maps it to the
existing typed `CommandError::Prompt` — the line fails, the session
survives (ADR 0011). The loop-side mirror: a dropped oneshot or a
failed disposition send (editor gone) synthesizes
`io::ErrorKind::BrokenPipe` → the same typed path. `Line`/`Confirm`
reads are cooked-mode `read_line`s — SIGINT keeps its default
disposition (process death), exactly today's documented "Ctrl-C during
a command" precedent (repl.rs `ReplExit` doc); nothing is persisted
mid-ceremony, so death is safe. `Masked` (rpassword, raw mode) sees
Ctrl-C as a byte, not a signal: implementation verifies rpassword's
actual behavior on the pty tier (D7) — if it errors, the cancellation
contract holds as specified; if the byte lands in the secret, the
degraded-but-safe outcome is recorded in ADR 0014's consequences (the
secret never leaks; the ceremony simply proceeds with a wrong
password and fails typed at verify).

### 3.3 `ReplPrompts` over `PromptReader` (incl. `read_confirm`)

```rust
pub struct ReplPrompts { dispositions: mpsc::Sender<Disposition> }

impl PromptReader for ReplPrompts {
    async fn read_line(&mut self, label)    → self.ask(label, PromptKind::Line)    → Text
    async fn read_secret(&mut self, label)  → self.ask(label, PromptKind::Masked)  → Text
    async fn read_confirm(&mut self, label) → self.ask(label, PromptKind::Confirm) → Flag
}

impl ReplPrompts {
    async fn ask(&self, label: &str, kind: PromptKind)
        -> Result<PromptValue, std::io::Error>
    // (tx, rx) = oneshot; dispositions.send(Prompt{..}).await
    //   .map_err(|_| BrokenPipe)?; rx.await.map_err(|_| BrokenPipe)?
}
```

`read_confirm` always prompts (no TTY short-circuit): the REPL's
existence implies a terminal (entry rule), and hermetic tests are the
answering peer. The one-shot's never-blocks rule (D4 in chunk 2)
governs `StdinPrompts` only — the seam that can face a pipe.

### 3.4 The prompt trait becomes async-signatured — **recorded deviation**

**Decision D1**: `PromptReader`'s three methods change from sync fns
to RPITIT (`fn read_line(&mut self, ..) -> impl Future<Output =
Result<String, io::Error>> + Send`), and `connect::run` becomes
generic (`P: PromptReader`) instead of `&mut dyn PromptReader`.
Impls use `async fn` (AFIT implements RPITIT; edition 2024, rust
1.96 — no new dependency, no `dyn` needed at the three call sites:
one-shot connect, login alias, session ceremony).
- **Reason (deviation from design §7's literal signature)**: the REPL
  impl is a channel round-trip — genuinely awaitable. Under a sync
  trait, `ReplPrompts` would have to `blocking_send`/`blocking_recv`
  inside the async session loop, parking a multi-thread-runtime worker
  for *unbounded user-thinking time*; on a 1-core kiosk that runtime
  has a single worker, which is precisely the "fatal" pattern repl.rs's
  own header and ADR 0011 forbid. `block_in_place` was rejected as a
  runtime-flavor landmine (panics under `current_thread`).
- **`StdinPrompts` keeps one behavior**: its bodies move to
  `pub(crate) fn *_sync` (line/secret/confirm), the async trait impls
  delegate, and the editor thread calls the sync cores (D6) — one
  grammar (`parse_confirm`), one test home, two call shapes.
- **Churn**: `FakePrompts` (tests/common) and `cli_connect.rs`'s
  `&mut dyn` call sites migrate to async impls/generics — assertions
  unchanged (the pins keep their strength; only signatures move).

### 3.5 History/completion bypass (structural)

`add_history_entry` stays inside the `readline` arm only; prompted
answers ride the oneshot and never enter the `LineEvent` channel or
the completion helper (which lives behind rustyline's Tab). Pinned by:
(a) a protocol unit test — a scripted answer exchange leaves the lines
channel empty; (b) the stage-3 review greps that no history call
exists outside the readline arm.

## 4. Session-loop interception & the config refresh (design §4.4)

### 4.1 Parse level vs session layer (mirrors ADR 0011's routing rules)

- `resolve_words`/`resolve_line` **stop refusing** `connect`, `login`,
  `logout`: they parse to `LineAction::Execute(Command, json)` with
  flags intact. `LineRefusal::{Connect, Login, Logout}` (and their
  `Display` arms and doc rows) die; `Daemon`, `ConfigCommand`,
  `ConfigFlag`, `SplitError` stay parse-level refusals (no JSON
  envelope, no exit-code change — ADR 0011's rule).
- `repl_loop`'s `Execute` arm matches the command **before**
  `dispatch_line`:

```rust
match command {
    Command::Connect { server, user, switch_backend, force } =>
        connect_line(&mut session, &app, ConnectArgs { server, user,
            switch_backend, force, confirmable: true }, /*alias*/ false, &mut line_io).await,
    Command::Login { server, user } => {
        writeln!(io.err, "warning: `login` is a deprecated alias of `connect`");  // UX, never asserted
        connect_line(&mut session, &app, ConnectArgs { server, user,
            switch_backend: false, force: false, confirmable: false }, /*alias*/ true, &mut line_io).await,
    }
    Command::Logout => logout_line(&mut session, &app, &mut line_io).await,
    other => dispatch_line(&session, &app, other, &mut line_io).await,
}
```

  (`repl_loop` takes `mut session`; `dispatch_line` still sees `&`.)
  `dispatch_line`'s `unreachable!` arm stays, its comment updated: the
  session loop runs the ceremonies, the IPC server refuses them.
- **Daemon IPC server (the second `resolve_words` consumer,
  server.rs:476)**: `run_argv`'s `Execute` arm gains a guard before
  `dispatch_line` — `connect`/`login`/`logout` are terminal-bound
  ceremonies (`route_for_ipc` lane `Local`; they need the caller's
  masked-prompt terminal, which the socket is not) → usage-class
  refusal `(2, "", guidance)`; the guidance text is server-local UX,
  never asserted. Wire behavior is unchanged from today (code 2),
  only the text differs. `boards list` stays servable (its `Local`
  lane is about pre-core reads, not terminals) — the guard is the
  explicit three-variant match, not a lane-table overload.

### 4.2 The connect ceremony in-session (order per design §4.4)

`connect_line` (in the new `session_ceremonies.rs`, one level of
abstraction: named steps) — `config_path = session.config_path`:

1. **Ceremony**: `connect::run(config_path, extract_raw(config_path),
   args, &mut session.prompts, &session.store)` — chunk 2's body
   verbatim; guards, verify-before-persist, overlay write, secret
   store. Until it succeeds, *nothing about the running core changed*.
2. **Refresh the session config view**: `session.config =
   AppConfig::load(config_path)?` (the full validated figment
   resolution — the same path that booted the session, env overlays
   included). On the degenerate read-back failure: the typed config
   error, guidance "restart the session" (the file is verified
   content; the session stays honest-and-stale; nothing rolls back).
3. **Detach-first when a connector is live** (**decision D3**): if
   `app.connector_attached()`, `app.detach_connector().await` first.
   Rationale: a same-identity re-connect stores a *new* secret, and
   the running actor would keep its old client (the next cycle still
   401s) — only detach+re-attach rebuilds the client; a re-point (D3
   guard already made it loud) needs the swap anyway; a standalone
   boot skips the step. Uniform lifecycle, one code path;
   `AttachError::AlreadyAttached` stays as the primitive's guard for
   non-session callers, now unreachable from this ceremony by
   construction (documented, not separately tested).
4. **Attach**: build `DeckClient::new(server, user,
   store.load(server, user)?)` from the *refreshed* config + session
   store → `app.attach_connector(client).await` → on `Err` the typed
   attach refusal (recoverable: another `connect` line; the config is
   already refreshed).
5. **Settle** (**decision D8**): `wait_for_boot_pull(&app,
   Duration::from_secs(shutdown_timeout_secs.max(1)))` — the boot
   wait's exact mirror (bounded, offline-first, degrade-to-local).
6. **Receipt**: `ConnectReceipt` + `attached: Some(true)`; emit via
   the line's `CommandIo` (JSON under the line's `--json`, rendered
   text otherwise — a rendered suffix for the attached fact).

### 4.3 The logout ceremony in-session (design §4.6, §4.4 run backwards)

`logout_line`:

1. **Local ceremony**: `logout::run(config_path, &session.config,
   session.store.as_ref())` — chunk-2/WI-H's body verbatim
   (`NotConfigured` under standalone, secret delete, auto-key strip).
2. **Detach the session's own connector**:
   `app.detach_connector().await` → `DetachOutcome` (idempotent
   `NoConnector` on standalone boots — no new machinery).
3. **Refresh**: `session.config = AppConfig::load(config_path)?` —
   the stripped auto-install re-resolves standalone (preludes refuse
   `sync-disabled` again); an explicit `nextcloud` install keeps its
   keys (loud stays loud) and stays `nextcloud`.
4. **Receipt**: `LogoutReceipt` + `session_connector:
   Some(SessionConnectorState)` mapping `DetachOutcome`
   (`detached`/`no-connector`/`timed-out`, kebab-case — the WI-H IPC
   vocabulary, honestly named for the session; `daemon_connector`
   stays the one-shot's field). No daemon nudge: the session *is* the
   lock holder, so no daemon can serve this database (**decision
   D10**).

### 4.4 Session plumbing

- `ReplSession` gains `config_path: Option<PathBuf>` and `prompts:
  ReplPrompts` (the disposition sender); `CoreSession` is untouched
  (the path is REPL bookkeeping, not a dispatch seam).
- `run_session` / `run_session_with_lines` gain the `config_path`
  argument (main passes `cli.config` through — one-line change). The
  `lines` factory's return widens to `EditorIo`; the production
  factory is the reworked `spawn_line_reader`.
- **Test injection seam (decision D5)**:
  `run_session_with_parts(config, config_path, store, json_default,
  editor_io_factory, io)` — the lib-internal generalization that also
  takes the credential store; `run_session_with_lines` keeps its
  production-store derivation and delegates. Hermetic journeys inject
  `FakeCredentialStore` (the ceremony stores into it; the attach
  loads from it — the round-trip is the point).

## 5. Completion additions (design §11 row)

- `REFUSED_VERBS` shrinks to `["daemon", "config"]`: `connect`,
  `login`, `logout` become offered verbs; the clap-shape walk picks
  up their flags for free (`connect`: `--server`, `--user`,
  `--switch-backend`, `--force` + propagated globals; `login`:
  `--server`, `--user`; `logout`: none). Value kinds stay free text
  (`Kind::None`) — a server URL is not completable.
- The spec-walk test's refused/offered assertions update; new rows pin
  the `connect` flag menu and that `connect ` (subverb slot) completes
  nothing (flat verb with no subcommands — the routing already
  yields `Kind::None` for a filled flat verb).

## 6. Hermetic test design (the terminal never appears)

**Scripted disposition peer** (tests/repl.rs helper, mirroring the
injected-lines pattern): the test builds the disposition channel
itself, spawns a `ScriptedEditor` task that `recv()`s dispositions
and answers `Prompt{..}` from a `VecDeque` of scripted values
(`Text`/`Flag`/`Fail(io::Error)`), ignoring `Next`; lines are
pre-sent on the lines channel as today. Session journeys then compose
`run_session_with_parts` + `FakeCredentialStore` + a wiremock
`MockServer` for the ceremony's verify call and the post-attach cycle
(the B-series hermetic-HTTP pattern — "no network" means none beyond
loopback wiremock, the repo's established seam).

**Unit tier** (repl.rs in-module, red first):
- U1 parse table: `connect [--flags]`, `login`, `logout` →
  `Execute` with flags intact; refusal set is exactly
  `{Daemon, ConfigCommand, ConfigFlag, SplitError}`.
- U2 `ReplPrompts`: each `PromptReader` method emits the matching
  `PromptKind` and returns the scripted value (exhaustion order pins
  the ceremony's prompt sequence); a `Fail` script maps to the io
  error; a dropped peer (sender dropped / oneshot dropped) maps to
  `BrokenPipe`.
- U3 history bypass: during a scripted prompt exchange the lines
  channel stays empty (answers never ride it).
- U4 `StdinPrompts` sync cores keep chunk 2's pinned behavior
  (parse_confirm grammar, non-TTY confirm decline — tests unchanged).
- U5 IPC: `run_argv`-level (or server unit) — relayed
  `connect`/`login`/`logout` words answer code 2; relayed core verbs
  keep working.

**Journey tier** (tests/repl.rs, R-series continuation):
- J1 standalone→connect: scripted server/user/password against
  wiremock → receipt `attached: true` (JSON parse), config file
  written under the tempdir, `FakeCredentialStore` holds the secret,
  and a following `sync` line routes to the engine (prelude passes on
  the refreshed config) — the wiremock cycle settles (predicate poll
  with deadline).
- J2 failed ceremony: verify answers 401 → typed line error; a probe
  line still works (session survived); no config write, no secret,
  `connector_attached() == false`, sync phase unchanged.
- J3 guard routing in-session: explicit-`standalone` config +
  `confirmable` connect → the y/N arrives as a `Confirm` disposition;
  scripted decline → typed `backend-explicit-standalone`; scripted
  consent → proceeds. (The guard matrix itself stays chunk 2's test
  home — this pins only the disposition mapping, per one-behavior-
  one-test.)
- J4 detach-first/re-point: session booted connected (wiremock +
  env store) → `connect` to a second wiremock identity (scripted
  consent) → the receipt reports the re-point, the connector serves
  the *new* identity (the second mock's auth-pinned boards matcher
  proves the Basic header), `connector_attached()` stays true
  throughout the observable end state.
- J5 login alias: `login` line routes to the same ceremony
  (`confirmable: false`: under explicit-standalone it refuses typed —
  structural kind assert), the notice is stderr UX (never asserted).
- J6 logout (auto backend): connected session → `logout` line →
  receipt `session_connector: "detached"`, config file stripped,
  refreshed session resolves standalone, a following `sync` line
  refuses typed `sync-disabled`, and a subsequent `connect` line
  (same identity, empty prompt script — idempotent re-verify) re
  -attaches (`connector_attached()` true again; the guard permits
  re-attach after detach).
- J7 logout (explicit nextcloud): `config_cleared` false → refreshed
  config stays nextcloud → a following `sync` line passes the prelude
  and refuses typed **`sync-unavailable`** (WI-H's rider, now
  in-session) → re-connect re-attaches. (J6+J7 together realize
  design §4.6's "preludes refuse sync-disabled" *and* the rider.)
- J8 logout on a standalone session: typed `not-configured`; the
  session continues.
- J9 editor-protocol edge: `Interrupted` event → the loop answers
  `Next` and continues (a following line works); session end drops
  close the peer cleanly (no hang; guarded by the test's own
  deadline).

**Completion tier**: spec-walk + candidate updates per §5.

**Pty tier (optional, decision D17)**: extend `tests/cli_tty.rs`
with one in-session journey — bare binary under `script(1)`, drive
`connect` through the pty (cooked prompts + masked secret), assert
the persisted config and a subsequent in-session `sync` receipt. This
is the only tier exercising rpassword on the editor thread; it also
carries D7's Ctrl-C behavior verification.

**Testing rules honored**: no sleeps (predicate polls with deadlines,
join handles), no text assertions (kinds, JSON fields, structural
facts), typed outcomes only, fakes over mocks, deterministic by
construction (wiremock is request-verified).

## 7. Receipt & output changes (commands/output.rs)

- `ConnectReceipt.attached: Option<bool>` — `None` for the one-shot
  (`skip_serializing_if`, one-shot JSON byte-identical), `Some(true)`
  from the session ceremony; render gains an attached suffix.
- `LogoutReceipt.session_connector:
  Option<SessionConnectorState>` (`Detached | NoConnector | TimedOut`
  → kebab `"detached"/"no-connector"/"timed-out"`) — `None` for the
  one-shot; honest sibling of `daemon_connector` (which stays
  one-shot-only). Pre-v0.1.0 JSON additions (sanctioned, WI-D2
  pattern); unit-pinned serialization shapes like the existing
  `detach_tests`.

## 8. Final docs commits (design §9, §6; dispatch §4)

1. **`docs/adr/0014-connector-attach-session-login.org`** — formalize
   the §9 draft, updated to the shipped reality: date 2026-10-1x,
   status Accepted; decisions 1–7 as drafted with chunk-1/2 facts now
   landed; consequences add the chunk-3 specifics — the disposition
   handshake table (editor-thread states), the detach-first session
   rule (D3), the terminal-bound IPC guard, the async prompt seam
   (D1's recorded deviation and reason), and the rpassword Ctrl-C
   caveat per D7's verification outcome.
2. **`docs/architecture.org`** — comms-table rows: *"REPL loop ⇄
   editor thread (disposition)"* (`tokio::sync::mpsc` + embedded
   `oneshot` — per-line verdicts and prompted reads; masked reads on
   the reader thread; answers bypass history) and a connect/logout
   row for *"Session ⇄ core"* (`App::attach_connector`/
   `detach_connector` + config refresh after the ceremony). The
   CLI-section verb list gains `connect` (login-as-alias) and the
   in-session ceremony note; the rpassword row's description widens
   to cover the editor-thread masked read.
3. **ADR 0011 amendment note** (in-file, dated): the in-session
   refusal bullet shrinks to `daemon`/`config`/per-line `--config`;
   `connect`/`login`/`logout` are session-layer ceremonies whose
   failures are typed handler envelopes (the parse-level-vs-handler
   routing rule is restated for them); the "never blocks a runtime
   worker" section gains the disposition handshake as its third
   corollary.
4. **ADR 0012 lane table**: add `connect` to the `Local` row (chunk
   2's omission; one word + the terminal-bound note).
5. **Backlog ✅ (separate `docs(backlog)` commit)**: both origin
   entries — "[2026-10-11] CLI commands to add a Nextcloud connection"
   and "[2026-10-11] `login` inside a running session (REPL / GUI)" —
   get `**Status**: ✅ DONE (implemented 2026-10-1x …)` one-liners
   naming chunks 1–3 and ADR 0014. The "[2026-10-12] Multiple active
   sync connectors" entry stays open. Plus the AGENTS §7 improvement
   note from §2 (in-session `status` honesty).

## 9. Commit slicing (rebase-merge hygiene — each standalone, gates green)

1. `refactor(app): async prompt seam with sync terminal cores` — D1:
   trait signature, `StdinPrompts` sync cores, `FakePrompts` +
   `cli_connect.rs` migration; existing assertions unchanged.
2. `feat(app): editor-thread disposition handshake` — protocol types,
   reworked `spawn_line_reader`, `ReplPrompts`, `EditorIo`; U2–U4 +
   U3 history pin.
3. `feat(app): session connect and logout ceremonies` — interception
   (U1), `session_ceremonies.rs`, config refresh, receipts (§7),
   IPC guard (U5), plumbing; J1–J9.
4. `feat(app): completion offers the ceremony verbs` — §5.
5. `test(app): pty journey for in-session connect` — D17 (may fold
   into 3 if small).
6. `docs(adr): formalize ADR 0014 connector attach and session login`
   — §8.1–8.4.
7. `docs(backlog): mark the connect-flow entries done` — §8.5.

## 10. Decisions (D1–D17, resolved)

| # | Decision | Reason / deviation note |
|---|---|---|
| D1 | `PromptReader` → RPITIT async methods; ceremony generic over `P` | **Deviation** from §7's `&mut dyn` sketch: a sync REPL impl must park a runtime worker for unbounded user-thinking time (1-core kiosk ⇒ single worker ⇒ the ADR 0011 anti-pattern). No new deps (AFIT/RPITIT, rust 1.96). |
| D2 | Parse-level refusals for connect/login/logout die in `resolve_words`; REPL intercepts by `Command` match; IPC server refuses them as terminal-bound before `dispatch_line` | Both `resolve_words` consumers stay total; wire behavior for relayed verbs unchanged (code 2). |
| D3 | In-session connect detaches a live connector first | Same-identity secret refresh must rebuild the client; re-point needs the swap; uniform lifecycle. `AlreadyAttached` remains the primitive's guard. |
| D4 | Guards read `extract_raw(config_path)` (one-shot's seam); refresh = `AppConfig::load(config_path)` | One guard semantics across frontends; refresh uses the boot's exact resolution (env overlays included). |
| D5 | `run_session_with_parts` test seam (store + config_path + `EditorIo` factory) | Session journeys need store injection + a tempdir config file; production entry unchanged. |
| D6 | Editor thread answers prompts via `StdinPrompts`' sync cores | One terminal-impl body, two call shapes; one confirm grammar. |
| D7 | Ctrl-C contract: read failure → `CommandError::Prompt`; cooked reads keep default SIGINT (today's precedent); rpassword's byte behavior verified on the pty tier and recorded in ADR 0014 | Design §5.2's cancellation, specified failure-tolerantly. |
| D8 | Post-attach settle = `wait_for_boot_pull` with the session's `shutdown_timeout_secs` budget | The boot wait's mirror (§4.4 step 3's "optional" resolved to yes — the REPL's UX parity). |
| D9 | In-session `login` = same ceremony, `confirmable: false`, stderr notice | Mirrors the one-shot alias exactly (D1-of-the-design's narrower surface). |
| D10 | In-session logout skips the daemon nudge | The session holds the instance lock — no daemon can serve this db. |
| D11 | Channels: lines `mpsc(1)` (unchanged), dispositions `mpsc(1)`, reply `oneshot` in the message | AGENTS §3 idioms; strict alternation (§3.2). |
| D12 | IPC guidance text is server-local UX | Structural pins on the code only (AGENTS §5). |
| D13 | ADR 0012 `Local` row gains `connect` | Chunk-2 bookkeeping omission. |
| D14 | `REFUSED_VERBS` → `[daemon, config]`; no new value kinds | §5; login stays offered until its removal round. |
| D15 | Hermetic journeys use wiremock + `FakeCredentialStore` | B-series' established hermetic-HTTP pattern. |
| D16 | `Eof` is disposition-asymmetric (editor exits without awaiting a verdict); drops are the exit signal | §3.2; keeps the terminal case simple and the test case total. |
| D17 | One pty in-session connect journey (optional-fold) | Only tier covering rpassword-on-editor-thread; carries D7's verification. |

## 11. Risks

- **The editor-thread handshake is the load-bearing risk** (deadlock/
  liveness): mitigated by the strict-alternation audit (§3.2's table
  of who-blocks-on-what), U2/U3/U9, and the drop-semantics tests. The
  one residual hazard — a scripted peer that never answers hangs a
  test — is bounded by the harness's own deadlines.
- **D1 churn in chunk-2 test fakes**: assertions must not weaken; the
  stage-3 diff review checks the migration is signature-only.
- **rpassword Ctrl-C behavior** (D7): unspecified by the design;
  verified on the pty tier; both outcomes degrade safely (typed
  cancellation or wrong-password typed failure — never a leak).
- **Borrow shape in the loop** (`&mut session` + `&app` + io while
  awaiting ceremonies): expected to compile cleanly since `App` and
  `ReplSession` are disjoint owners; if not, the ceremonies take the
  fields they need (`&mut AppConfig`, `&ReplPrompts`) — recorded
  fallback, not a design change.
- **`wait_for_boot_pull`'s 200 ms settle poll** adds slight wall-time
  to J1/J4/J6/J7 (existing helper; deadline-guarded, deterministic
  end state).

## 12. Estimated diff

~+1,100 / −150 lines over ~15 files: repl.rs (+~350), new
session_ceremonies.rs (~200), connect.rs (+~60/−40), logout.rs +
output.rs (~+50), mod.rs (~+50), completion.rs (~+30/−10),
ipc/server.rs (~+25), main.rs (~+5); tests/repl.rs (+~450),
common/mod.rs + cli_connect.rs migration (~±100), ipc pins (~+30),
cli_tty.rs (~+60); docs: ADR 0014 (~140 new), architecture.org
(+~15), ADR 0011 (+~10), ADR 0012 (~2), backlog (+~10). Chunk size
M/L as dispatched.
