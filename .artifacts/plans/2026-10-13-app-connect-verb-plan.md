# [Plan] Connect Verb + Login Alias (WI-J.2)

- **Date**: 2026-10-13
- **Kind**: stage-1 implementation plan (chunk 2 of 3), TDD-first. No
  production code in this commit.
- **Origin**: approved design round
  `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` (§3.1/§3.2
  normative — the verb shape, the D1 alias semantics, the D2
  flag+confirm and D3 confirm+force guards, the multi-connector
  forward-compatibility note; §7 the ceremony seam; §12 chunk WI-J.2),
  dispatched by
  `.artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md`
  §1/§3 (prompt 2.1).
- **Target**: `taskboard-app` only — `src/cli.rs`, `src/commands/
  {connect.rs (NEW), login.rs (deleted), mod.rs, output.rs}`,
  `src/{repl.rs, completion.rs}` (minimal, compile/behavior-forced —
  §5.4), plus the test tiers. **No engine, bootstrap, domain, sync,
  storage, or UI diff.**
- **Branch / worktree**: `feat/app-connect-verb`,
  `../taskboard-WT-feat-app-connect-verb`, off `main` @ `d80e84d`
  (chunk 1 — `feat/connect-engine-attach` — already merged into
  `main`, so the branch carries it for free; the chunks are
  order-independent and chunk 2 never calls `attach_connector`).
- **Goal**: the one-shot connector-introduction surface of design §3 —
  `taskboard connect` as the primary verb running the extracted
  ceremony (D2 backend flip: `--switch-backend` or a TTY y/N confirm;
  D3 re-point: y/N confirm or `--force`, old secret deleted after the
  new verify), `taskboard login` as its deprecated alias with a
  strictly narrower flag surface, both on the `Local` IPC lane, plus
  the recorded connect-while-daemon heads-up decision (§6).
- **Status**: READY for stage 2 (every open question resolved in §8).

---

## 1. Context & verified facts (anchored; do not re-derive)

All anchors verified on `main` @ `d80e84d` (the worktree base; chunk 1
landed in `f9f808a`/`80974f3`/`d80e84d` and touched none of these
files).

1. **The login ceremony is already a frontend-agnostic seam.**
   `login::run(config_path, raw, server, user, &mut dyn PromptReader,
   &store)` (`crates/taskboard-app/src/commands/login.rs:92-149`):
   gather identity → verify via `DeckClient::boards()` → persist the
   `toml_edit` overlay (`write_config_overlay`, login.rs:170-194 —
   read-modify-write, comment/unknown-table preserving, temp-file +
   `rename` atomic) → store the secret. Nothing persists unless the
   server verified the credentials. `PromptReader` (login.rs:23-39)
   has `read_line`/`read_secret`, both `Result<String, io::Error>`;
   `StdinPrompts` (login.rs:53-81) writes prompts to stderr and gates
   the masked read on `std::io::stdin().is_terminal()` (login.rs:64).
2. **The one-shot dispatcher pre-gates explicit standalone.**
   `login_command` (`commands/mod.rs:660-688`) resolves
   `AppConfig::write_path(config_path)` (config.rs:313-321),
   `extract_raw`s (config.rs:334-342 — tolerates a not-yet-valid
   file; a missing file feeds the empty default), refuses
   `raw.sync.backend == Some(SyncBackend::Standalone)` with
   `CommandError::SyncDisabled` **before any prompting** (pinned by
   L9: `server: None` must exit 1 rather than hang on stdin), else
   runs the ceremony with `StdinPrompts` + `production_store`.
3. **`logout` is the teardown mirror and the UX-warning precedent.**
   `logout::run` (logout.rs:27-71): requires the configured identity
   (`NotConfigured`), deletes the secret (`NotFound` idempotent),
   strips the config keys **iff** `raw.sync.backend.is_none()`
   (logout.rs:57 — an explicit backend word is never rewritten).
   `nudge_daemon` (logout.rs:159-168) probes
   `crate::ipc::client::try_connect(db_path).await` (per-db socket;
   `None` = no daemon) and `nudge_warning` (logout.rs:141-153) is the
   pure `DaemonConnectorState -> Option<&'static str>` stderr-warning
   seam, unit-pinned over every arm — the shape §6's decision mirrors.
4. **`CommandError` kinds are the `--json` contract.**
   `CommandError` (`commands/mod.rs:82-198`) with the exhaustive
   `error_kind()` table (`:224-265`, kebab-case; the compiler forces
   new variants to pick a kind) and optional `error_detail()`
   (`:270-284`). Refusal kinds today include `sync-disabled` (both
   the connector-verb prelude and login-under-standalone) and
   `not-configured`.
5. **Three exhaustive `Command` matches are compile-forced by any new
   variant.** `commands::dispatch` (`mod.rs:329-614`),
   `dispatch_line` (`mod.rs:826-858` — `Login`/`Logout`/`Config`/
   `Daemon` sit in an `unreachable!` arm: "the session loop
   intercepts … before routing"), and `route_for_ipc` (`mod.rs:
   1110-1125`, unit-pinned by `ipc_routing_tests` at `:1214-1289`).
   The IPC daemon wire (`ipc/protocol.rs`) has its own vocabulary and
   never matches on clap `Command` — no protocol diff.
6. **The session refuses `login`/`logout` at parse level.**
   `resolve_words` (`repl.rs:269-292`) maps parsed `Command::Login`/
   `Logout`/`Config`/`Daemon` to `LineRefusal` variants (`repl.rs:
   184-228`, Display guidance is UX, never asserted); completion's
   `REFUSED_VERBS = ["daemon", "login", "logout", "config"]`
   (`completion.rs:230`) keeps the refused surface out of the offered
   candidates while the clap-tree walk offers every other verb.
   Design §11 assigns the *replacement* ceremonies to chunk 3 — chunk
   2's additions there are refusal-shaped only (§5.4, decision D13).
7. **`LoginReceipt`** (`commands/output.rs:249-260`): `server_url`,
   `username`, `verified`, `stored` (`"keyring"`/`"env"`); the env
   store answers `Err(CredentialError::Unsupported)` on both `store`
   and `delete` (`secrets.rs:173-183`) — login maps store-`Unsupported`
   to `stored: "env"` **success**; any re-point deletion must tolerate
   the same or every env-store re-point fails (D7).
8. **The L-series pins the ceremony hermetically** (`tests/
   cli_login.rs`): L1 happy/verify-then-persist, L2a/b 401/403 →
   `invalid-credentials` nothing-persisted, L3 unreachable, L4 env
   store, L5 comment-preserving overlay, L6 prompt order via
   `FakePrompts` (queue-exhaustion proof, `tests/common/mod.rs:26-52`),
   L7 explicit-missing-config creation, L8 invalid URL, L9
   standalone-refuses-before-prompting (through `dispatch`), L10–L12
   the write-path resolution table (XDG/env-target/existing-XDG).
   The binary tiers: `run_cli` (`tests/it_cli.rs:34-57`) drives the
   real binary with piped stdin (docker tier, `#[ignore]`d, env
   gated); `tests/cli_tty.rs` drives the real binary under a
   `script(1)` pty (skipped when `script` is absent).
9. **`ConnectReceipt`'s JSON additions are pre-v0.1.0-sanctioned**
   (design §4.4, the WI-D2 kind-surfacing precedent): additive fields
   and new kinds may land before the first tagged release.

## 2. Scope

**In** (design §3.1/§3.2/§7 + dispatch §3):

- `cli.rs`: `Command::Connect { server, user, switch_backend, force
  }`; `Login` doc-marked as the deprecated alias (shape unchanged).
- `commands/connect.rs` (NEW): the frontend-agnostic ceremony —
  gather → guards (D2/D3) → verify → persist overlay (backend rewrite
  iff switched) → store secret (+ old-secret deletion iff re-point);
  `PromptReader` (+ `read_confirm`) and `StdinPrompts` moved here;
  `ConnectReceipt` + renders; the daemon heads-up helpers (§6).
- `commands/login.rs`: deleted — the body, prompts seam, overlay
  writer, and `deck_variant` move into `connect.rs`
  behavior-preserving; the alias becomes dispatch-level (mod.rs).
- `commands/mod.rs`: `connect_command` + the alias arm with the
  stderr deprecation notice; `CommandError::{BackendExplicitStandalone,
  ConnectionExists}` + kinds; `dispatch_line`'s forced arm;
  `route_for_ipc`'s `Connect → Local` row.
- `commands/output.rs`: `LoginReceipt` → `ConnectReceipt` (+ the two
  fields).
- `repl.rs` + `completion.rs`: the minimal refusal-shaped touches
  (D13).
- Tests: C-series hermetic ceremony matrix (`tests/cli_connect.rs`),
  L-series re-pointed to the alias/dispatch tier, routing/lane pins,
  the J4 binary journey (docker tier), the pty confirm leg.

**Out** (chunk 3, chunk 1-already-landed, or future rounds): any
session-level ceremony (`dispatch_line` routing of connect, the
disposition protocol, `ReplPrompts`, config refresh — chunk 3); the
`attached` receipt field (chunk 3 — chunk 2's connect is pre-core by
design §3.2); `App::attach_connector` calls (chunk 1 landed the
primitive; the session wiring is chunk 3); ADR 0014 +
architecture.org amendments and the backlog ✅ marks (chunk 3's final
commits); the daemon-side attach IPC verb and any receipt field for
daemon state (future round, design D5/§6 — see D11); `connect
<name>` subcommand targets, `connect list/remove` (multi-connector
round); hiding/removing `login` (v0.1.0 release prep).

## 3. The clap surface (`src/cli.rs`)

```rust
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
/// backend or a configured different identity refuses typed, naming
/// `connect`.
Login {
    /// Server URL (prompted when omitted).
    #[arg(long)]
    server: Option<String>,
    /// Login name (prompted when omitted).
    #[arg(long)]
    user: Option<String>,
},
```

- `Connect` is declared immediately before `Login` (the help list
  reads primary-first; the alias stays visible — hiding is part of
  the removal pass at v0.1.0, not the deprecation, D1).
- The bare-verb-with-flags shape is the multi-connector-surviving
  form (design §3.1): the future `connect <name> …` target slot does
  not collide with today's flags; no `connect add`/`connection set`
  subcommand family is implied (rejected alternatives, §3.1).
- No positional arguments; no `--password` flag ever (design §5.3 —
  secrets in argv are a security regression).

## 4. The ceremony (`src/commands/connect.rs`, NEW)

### 4.1 Signature and args

```rust
/// The connect ceremony's flag bundle. `login`'s dispatch arm builds
/// the same bundle with `switch_backend = false, force = false,
/// confirmable = false` — the alias is parse/dispatch-level, not a
/// second body (one behavior, one test home).
#[derive(Debug, Clone, Default)]
pub struct ConnectArgs {
    /// Server URL; prompted when `None`.
    pub server: Option<String>,
    /// Login name; prompted when `None`.
    pub user: Option<String>,
    /// D2: the scripted loud backend flip.
    pub switch_backend: bool,
    /// D3: the scripted re-point escape.
    pub force: bool,
    /// Whether the guards may escalate to an interactive y/N confirm.
    /// `connect` passes `true`; the deprecated `login` alias passes
    /// `false` — its surface is strictly narrower than its
    /// replacement (design D1): under an explicit `standalone`
    /// backend or a configured different identity it refuses, naming
    /// `connect`, instead of walking the user through a flip its own
    /// flags cannot express.
    pub confirmable: bool,
}

/// Runs the connect ceremony. `raw` is the pre-read, unvalidated
/// config at `config_path` (the dispatcher reads it once;
/// `extract_raw` tolerates a not-yet-valid config).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs. Nothing is
/// persisted (no config write, no store call) unless the server
/// verified the credentials; a declined or unconfirmable guard
/// refuses before any identity is gathered.
pub async fn run(
    config_path: &Path,
    raw: AppConfig,
    args: ConnectArgs,
    prompts: &mut dyn PromptReader,
    store: &Arc<dyn CredentialStore>,
) -> Result<ConnectReceipt, CommandError>
```

### 4.2 Decomposition — one level of abstraction per body

`run` orchestrates five named steps; every step's body works at one
level (guard clauses at the top of each, detail in named private
helpers — the review mandate's SRP/abstraction line):

```rust
pub async fn run(...) -> Result<ConnectReceipt, CommandError> {
    // 1. D2 guard (before gathering: a declined flip must not cost
    //    the user a typed password; a refusal before prompting is
    //    today's pinned behavior, L9).
    let backend_switched = decide_backend_switch(&raw, &args, prompts)?;

    // 2. Gather identity (flags | prompts) — unchanged from login.
    let identity = gather_identity(&args, prompts)?;

    // 3. D3 guard: needs the gathered identity to compare.
    let repoint = decide_repoint(&raw, &identity, &args, prompts)?;

    // 4. Verify before anything is persisted (unchanged).
    verify(&identity).await?;

    // 5. Persist: overlay (server/user, + backend word iff switched),
    //    then the secret (store new; delete the replaced identity's
    //    secret iff re-point).
    persist(config_path, &raw, &identity, backend_switched)?;
    let stored = store_secret(store, &identity, repoint.as_ref())?;

    Ok(ConnectReceipt { server_url: identity.server, username: identity.user,
                        verified: true, stored, backend_switched,
                        repointed: repoint.is_some() })
}
```

with:

- **`decide_backend_switch(&raw, &args, prompts) -> Result<bool,
  CommandError>`** — the D2 guard, run iff
  `raw.sync.backend == Some(SyncBackend::Standalone)`:
  `args.switch_backend` → `Ok(true)` (the loud act recorded in argv,
  no prompt); else `args.confirmable && prompts.read_confirm(…)?` →
  `Ok(true)` (the flag-absent terminal story); else
  `Err(CommandError::BackendExplicitStandalone)`. Under every other
  backend word (`Some(Nextcloud)`, auto) the guard is a no-op
  returning `Ok(false)` — and `false` means the overlay writes **no**
  backend key at all (D6).
- **`gather_identity(&args, prompts)`** — login.rs:100-114 moved
  verbatim (flags or the three labeled prompts; `CommandError::Prompt`
  on read failure).
- **`decide_repoint(&raw, &identity, &args, prompts) -> Result<
  Option<(String, String)>, CommandError>`** — the D3 guard, run iff
  the config carries a *complete* different identity
  (`server_url` and `username` both `Some`, and the pair differs from
  the gathered one — D8):
  `args.force` → proceed; else `args.confirmable &&
  read_confirm(…)?` → proceed; else
  `Err(CommandError::ConnectionExists)`. Proceeding returns the
  replaced `Some((old_server, old_user))`; a same-identity connect
  returns `None` and never prompts (the idempotent re-verify).
- **`verify(&identity).await`** — login.rs:117-124 moved verbatim:
  `DeckClient::new` then `boards()`, mapped through `map_deck_error`
  (`InvalidCredentials`/`Unreachable`/`Deck`), `tracing::info!` with
  `deck_variant` on failure.
- **`persist(config_path, &raw, &identity, backend_switched)`** —
  the in-memory merge + `validate()` + `write_config_overlay(path,
  server, user, backend_switched)` (§4.4), `CommandError::ConfigWrite`
  on failure.
- **`store_secret(store, &identity, repoint)`** — login.rs:136-140's
  mapping (`Ok(()) → "keyring"`, `Unsupported → "env"`) plus, iff
  `repoint.is_some()`, the old pair's deletion **after** the new
  store, tolerating `NotFound` (idempotent) and `Unsupported` (the
  env store has no secret to orphan — D7), erroring `Credential`
  otherwise.

### 4.3 `PromptReader::read_confirm` and the TTY rule

`PromptReader` (moved here from login.rs) grows one method — with a
`Result` shape, one recorded deviation from the design §5.4 sketch
(D3):

```rust
/// A y/N decision (D2/D3 guards). On a terminal: the label on
/// stderr, one line read, `y`/`yes` (case-insensitive, trimmed) is
/// consent. On non-terminal stdin: `Ok(false)` immediately — a
/// piped or `--json` run must never block on an interactive
/// decision, and the typed refusal (which names the flag) is the
/// scripted answer.
///
/// # Errors
///
/// [`std::io::Error`] when the confirm line cannot be read — mapped
/// by the ceremony to [`CommandError::Prompt`] exactly like a failed
/// identity prompt (a cancelled prompt is not a declined one; the
/// REPL's Ctrl-C disposition relies on the distinction).
fn read_confirm(&mut self, prompt: &str) -> Result<bool, std::io::Error>;
```

- `StdinPrompts::read_confirm`:
  `if !std::io::stdin().is_terminal() { return Ok(false); }` — the
  same detection `read_secret` uses (login.rs:64) — else prompt to
  stderr, read one line, `parse_confirm(&line)`.
- `fn parse_confirm(line: &str) -> bool` — pure: trimmed,
  ASCII-case-insensitive `"y"`/`"yes"` is consent; everything else
  (empty line, `n`, garbage) declines. One shot — no re-prompt loop.
  Unit-pinned; `read_confirm`'s non-TTY arm is unit-pinned by the
  test binary's own non-terminal stdin (nextest: the timely
  `Ok(false)` return is the proof — a blocking read would hang the
  test).
- The never-blocks rule keys on **stdin TTY-ness alone**; `--json` is
  orthogonal (a `--json` run with a terminal stdin is a human at a
  keyboard and may confirm; a `--json` run over pipes/cron gets the
  typed refusal). `FakePrompts` (tests/common) grows a separate
  scripted **confirm queue** (`VecDeque<bool>`; `read_confirm` pops
  it), so line/secret prompts and confirms are independently
  scriptable and "no confirm was asked" is an exhaustion assertion —
  the alias's narrower surface pins exactly that way.

### 4.4 The overlay mechanics (backend rewrite + re-point)

`write_config_overlay` moves from login.rs and generalizes:

```rust
/// Overlays `nextcloud.server_url`/`username` (and, iff
/// `switch_backend`, sets `sync.backend = "nextcloud"`) into the
/// config file via `toml_edit` read-modify-write (preserves
/// comments, formatting, and unknown tables — the file is operator
/// territory), writing atomically (temp file + rename). Creates the
/// file when absent; `toml_edit` creates the `[sync]` table when the
/// rewrite meets a file without one.
fn write_config_overlay(
    path: &Path,
    server: &str,
    user: &str,
    switch_backend: bool,
) -> Result<(), std::io::Error>
```

- **The backend word is written iff the D2 switch decided it**
  (flag or confirmed; never on the auto path — D6): `logout` strips
  the identity keys iff `backend.is_none()` (fact 3), so a connect
  that wrote `backend = "nextcloud"` onto an auto install would
  silently break the next logout's strip. The auto install stays
  auto; only an explicit `standalone` that was loudly switched gets
  the explicit `nextcloud` word.
- The re-point path writes the *same* overlay (server/user
  overwritten in place; the backend word untouched whatever it is).
  The old secret's deletion is **not** a config concern — it rides
  `store_secret` after the new verify (§4.2, D7).
- The in-memory `validate()` before the write sets
  `merged.sync.backend = Some(SyncBackend::Nextcloud)` iff switched,
  so the validated shape matches the file about to be written.
- Atomicity is the existing temp+`rename` verbatim; the write target
  is `AppConfig::write_path`'s answer (resolved by the dispatcher,
  fact 2 — L10–L12's table is untouched).

### 4.5 Receipt and renders

```rust
/// The `connect` (and deprecated `login` alias) receipt. No secret
/// ever appears in any projection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectReceipt {
    /// The verified server URL.
    pub server_url: String,
    /// The login name.
    pub username: String,
    /// The server accepted the credentials.
    pub verified: bool,
    /// Where the secret went: `"keyring"` or `"env"`.
    pub stored: String,
    /// An explicit `standalone` backend was loudly switched to
    /// `nextcloud` (flag or confirmed).
    pub backend_switched: bool,
    /// A configured different identity was replaced (confirmed or
    /// forced); its secret was deleted after the new one verified.
    pub repointed: bool,
}
```

`LoginReceipt` is renamed to this (one receipt, one home; the alias
emits the identical JSON — the two additive fields are the
sanctioned pre-v0.1.0 contract change, fact 9). `render_text` moves
from login.rs and extends its two arms with the new facts
(`— backend switched from standalone` / `— re-pointed from <old>`
suffixes; UX text, never asserted). The `attached` field is chunk 3's
(§2 out-of-scope).

### 4.6 Typed refusals (`CommandError` additions)

```rust
/// The config pins `sync.backend = "standalone"` and the flip was
/// neither flagged (`connect --switch-backend`) nor confirmed on a
/// terminal. Guidance names the flag (Display is UX, never asserted).
#[error("backend is explicitly stand-alone — pass --switch-backend to switch it")]
BackendExplicitStandalone,
/// A different connection is already configured and the re-point was
/// neither forced (`connect --force`) nor confirmed on a terminal.
#[error("a different connection is already configured — pass --force to re-point it")]
ConnectionExists,
```

- Kinds: `"backend-explicit-standalone"`, `"connection-exists"`
  (design §3.1 names both). No `error_detail` for either (the
  refusing caller knows both identities; the actionable next step is
  the flag, which the kind + guidance carry — D10).
- **Kind change, recorded**: `login` under explicit `standalone`
  moves from `sync-disabled` to `backend-explicit-standalone`. The
  design mandates the alias refusing with the new kind (§3.1 D1);
  `sync-disabled` remains the connector-verb prelude's kind
  (`sync`, `boards select`, …). L9's assertions (exit 1, nothing
  stored) are untouched; the new kind is pinned at the dispatch tier.

## 5. Dispatch, lanes, and the compile-forced session seams

### 5.1 One-shot arms (`commands/mod.rs`)

```rust
Command::Connect { server, user, switch_backend, force } => {
    connect_command(
        &mut ctx.io, ctx.config_path,
        ConnectArgs { server, user, switch_backend, force, confirmable: true },
    ).await
}
Command::Login { server, user } => {
    login_command(&mut ctx.io, ctx.config_path, server, user).await
}
```

- `connect_command` is `login_command`'s shape (fact 2) minus the
  pre-gate (the D2 guard now lives inside the ceremony, where the
  confirm seam exists): `write_path` → `extract_raw` →
  `production_store` → `connect::run(&path, raw, args, &mut
  StdinPrompts, &store)`. On `Ok(receipt)`: the §6 daemon heads-up
  (probe with `raw.db_path()` captured before `raw` moves), then
  `emit_to` + exit 0.
- `login_command` becomes the alias: one stderr line
  (`warning: \`login\` is a deprecated alias of \`connect\``) written
  to `io.err` **before** delegating to `connect_command` with
  `ConnectArgs { server, user, switch_backend: false, force: false,
  confirmable: false }`. The notice is first on stderr so the
  `--json` "last stderr line is the error envelope" convention
  survives; stdout's payload contract is untouched; the notice text
  is UX and never asserted (the structural pin is D-series: alias
  happy ⇒ non-empty stderr, connect happy ⇒ empty stderr).

### 5.2 Lanes

`route_for_ipc`: `Command::Connect { .. }` joins `Login`/`Logout`/
`Config`/`boards list` in the `Local` row (design §3.2: pre-core —
config + keyring + one verify call; boots nothing, takes no lock);
`ipc_routing_tests` grows the row. The daemon-liveness caveat is
documentation + the §6 warning, exactly the ADR 0012
logout-while-daemon pattern (design §3.2).

### 5.3 Session surfaces — refusal-shaped only (D13)

Chunk 3 owns the session ceremonies; chunk 2's enum addition still
forces three touches, all refusal-shaped and all replaced (not
widened) by chunk 3:

- `dispatch_line`: `Command::Connect { .. }` joins the `unreachable!`
  arm ("the session loop intercepts … before routing") — exhaustive
  match (fact 5).
- `repl.rs`: `LineRefusal::Connect` (Display names the one-shot
  verb — UX, never asserted) + the `resolve_words` arm
  `Some(Command::Connect { .. }) => Refuse(Connect)`. A `connect`
  line must not route into `dispatch_line`'s unreachable arm, and the
  ceremony cannot run in-session yet (no `ReplPrompts`, no
  disposition protocol, no config refresh — ADR 0011's parse-level
  refusal is the honest interim state, exactly `Login`'s today).
- `completion.rs`: `REFUSED_VERBS` grows to five (`+ "connect"`) —
  the clap-tree walk would otherwise offer a verb the session
  refuses; the completion pins update mechanically. Chunk 3 removes
  `connect` (and `login`/`logout`) from the list when the ceremonies
  land and adds the flag candidates.

## 6. The connect-while-daemon heads-up (recorded UX decision, design §3.2/§6)

**Decision: YES — a one-line stderr heads-up after a *successful*
one-shot ceremony, when the per-database IPC probe answers.** No
receipt field, no exit-code change, nothing on the failure path.

- **Why yes.** (1) *The surprise is real and otherwise silent*: the
  connect succeeds, the receipt says `verified`, and the daemon
  serving the same database keeps running connector-less until
  restart — an operator debugging "why isn't the daemon syncing" has
  zero local signal, because the daemon-side attach verb does not
  exist yet (design D5 defers it). (2) *Precedent*: WI-H established
  that connector-liveness surprises on the db get a stderr heads-up
  (`nudge_warning`, fact 3); this is the same event class, and the
  design explicitly leaves the mirror line as "a UX call for the
  implementation plan" (§6) with documentation as the only contract
  (§3.2). (3) *Cheap and safe*: one `try_connect` probe on the
  success path, best-effort, exit 0 regardless — the same
  never-fails-the-verb discipline as the logout nudge (D5 there).
- **Shape** (mirrors the logout module's house shape):
  - `pub(crate) async fn daemon_serves(db_path: &Path) -> bool` —
    `ipc::client::try_connect(db_path).await.is_some()`; a transport
    failure answers `false` (a dead socket sidecar means nothing
    serves — silent, not degraded; the same reading as
    `NudgeOutcome::NoDaemon`).
  - `#[must_use] pub fn daemon_warning(present: bool) ->
    Option<&'static str>` — `Some("warning: a daemon is serving this
    database; it keeps running without the new connection until
    restarted (or `taskboard daemon stop`)")` iff `present`, `None`
    otherwise. Pure, unit-pinned over both arms (`is_some`/`is_none`
    only — the text is UX, never asserted).
  - Wired in `connect_command` only (the ceremony is
    frontend-agnostic; a GUI or future daemon-side caller has no
    daemon to warn about, and the REPL session is its own lock
    holder). **The alias warns too** — the liveness gap is the
    operation's, not the verb spelling's.
- **Why no receipt field.** A field is contract; the design frames
  the warning as UX, "not this round's contract" (§3.2). `logout`'s
  `daemon_connector` field earns its place by reporting a *nudge
  outcome*; connect has no nudge — a boolean "a daemon existed"
  carries no actionable distinction. When the daemon-side attach
  verb lands (future round, riding WI-H's pattern), the warning
  upgrades to a real nudge with typed outcomes and the matching
  `daemon_connector`-shaped field — exactly WI-H's evolution path.
  Until then the gap is documented as ADR 0012 documented
  logout-while-daemon (chunk 3's docs commits).
- **`--json`**: the warning still goes to stderr (stdout stays one
  JSON payload; stderr warnings are the established channel). On
  success stderr holds only the warning (+ the alias notice); the
  last-line convention is unaffected.

## 7. Test catalogue (TDD — written first, all hermetic unless named)

Determinism by construction: scripted `FakePrompts` (line/secret
queue + confirm queue), `FakeCredentialStore`/`EnvCredentialStore`
fakes, wiremock servers, tempdirs, `env_lock` for env-mutating cases;
no TTY (the fake answers confirms), no fixed sleeps, typed outcomes
and structural file assertions only — no text assertions anywhere
(AGENTS §5; notice/guidance/prompt wording is UX).

### 7.1 C-series — the ceremony tier (`tests/cli_connect.rs`, NEW)

Direct `connect::run` calls, mirror of the L-series harness. Wiremock
`GET /index.php/apps/deck/api/v1.0/boards` for verify paths.

**D2 (explicit `standalone` in the config file):**

- **C1 — `--switch-backend` proceeds and rewrites the backend word.**
  Seed `backend = 'standalone'`; `switch_backend: true`; happy mock.
  Receipt `backend_switched: true`, `repointed: false`; the written
  TOML contains `backend` = `"nextcloud"` **and** server/user;
  store recorded; no confirm consumed (queue untouched).
- **C2 — the TTY story via the confirm seam: consent switches.**
  Same seed; flags false, `confirmable: true`, confirm queue `[true]`.
  Proceeds; `backend_switched: true`; exactly one confirm consumed;
  TOML carries the backend word.
- **C3 — a declined confirm refuses typed, nothing gathered.**
  Confirm queue `[false]` → `Err` kind `backend-explicit-standalone`;
  the line/secret queues are **untouched** (the guard ran before
  gather — the D5-ordering pin); no file written; no store call.
- **C4 — the alias shape never asks.** `confirmable: false`, confirm
  queue left `[true]` (a consent that must not be consumed) →
  `Err(BackendExplicitStandalone)`; the confirm queue is still
  non-empty afterwards — the strictly-narrower pin (D2).
- **C5 — non-TTY, no flag: the refusal without any prompt.**
  `confirmable: true` with an *empty* confirm queue → the refusal;
  nothing consumed, nothing persisted. (The real-stdin non-TTY arm is
  pinned at the unit tier: `StdinPrompts::read_confirm` returns
  `Ok(false)` under nextest's non-terminal stdin — the timely return
  is the proof — plus `parse_confirm` over `{y, Y, yes, YES, n, "",
  garbage}`.)

**D3 (a complete, different identity configured):**

- **C6 — `--force` re-points; the old secret dies after the new
  verify.** Seed config pointing at server A/alice, store holding
  A/alice's secret; connect to B/bob with `force: true`, happy mock.
  Receipt `repointed: true`, `backend_switched: false`; config now
  names B/bob; store has B/bob and **not** A/alice.
- **C7 — the confirmed re-point is the same outcome.** C6 with
  `force: false, confirmable: true`, queue `[true]`; one confirm
  consumed.
- **C8 — a declined re-point leaves everything untouched.**
  Queue `[false]` → `Err` kind `connection-exists`; the config file's
  bytes are unchanged; the store still holds only the old identity.
- **C9 — the alias shape refuses the re-point without asking.**
  `confirmable: false`, queue `[true]` untouched →
  `Err(ConnectionExists)` (the C4 pin, D3 arm).
- **C10 — same-identity re-verify is idempotent, no prompt.**
  Config already names the target server/user; empty confirm queue;
  proceeds (secret refresh: the store's value is the new password);
  `repointed: false`, `backend_switched: false`; zero confirms
  consumed.
- **C11 — a half-configured file is treated as unconfigured.**
  Seed `server_url` only (no `username`): no D3 fires; the overlay
  completes both keys; no deletion attempted (store untouched for any
  other pair).
- **C12 — the composition: standalone + a configured different
  identity + both flags.** `switch_backend: true, force: true`:
  proceeds; `backend_switched: true` **and** `repointed: true`; the
  old secret is gone; the file carries the `nextcloud` word and the
  new identity.

**Core ceremony (the L1–L8 behaviors, re-pinned at the new seam —
assertions carried over, never weakened):**

- **C13 — happy path** (was L1): verify-then-persist; receipt fields;
  secret-free file; store recorded.
- **C14 — 401/403 refuse without persisting** (was L2a/b) — **plus
  the re-point ordering trap**: the forced re-point with bad
  credentials leaves the old identity fully intact (config bytes
  unchanged, old secret still stored, new secret absent) — the
  deletion strictly follows the new verify.
- **C15 — unreachable** (was L3) and **invalid URL** (was L8): typed
  `unreachable`/`deck`, nothing persisted.
- **C16 — the env store** (was L4): `stored: "env"`; **and the
  env-store forced re-point succeeds** (`repointed: true` — the
  `Unsupported` deletion tolerance, D7).
- **C17 — overlay preservation** (was L5): comments, inline
  comments, unknown tables survive; **with the switch**: the
  `[sync]` table gains/updates `backend` preserving its decor; and
  the auto path writes **no** `backend` key at all (the D6/logout
  interaction pin — asserted as the key's absence in the parsed
  file).
- **C18 — prompt order** (was L6): server, user, secret, exactly
  consumed.
- **C19 — missing explicit config created** (was L7).

**Unit tier (in-file, `src/commands/connect.rs`):**
`parse_confirm` truth table; `StdinPrompts::read_confirm` non-TTY
decline; `daemon_warning` both arms; the `deck_variant`/`render`
helpers keep their existing pin style; the `PromptReader` doc-sketch
carries over from login.rs.

### 7.2 L-series — re-pointed to the alias/dispatch tier (`tests/cli_login.rs`)

- L1–L8's ceremony-body pins **move to C13–C19** (same assertions,
  new seam — stage 3 diffs every moved test for equivalence). What
  stays/lands in cli_login.rs is the *routing* surface the alias
  owns:
  - **L9 (updated)** — alias under explicit standalone through
    `dispatch`: exit 1; the `--json` envelope on `err` parses and
    its `error.kind` is `backend-explicit-standalone` (typed-kind
    assertion, not text); nothing stored; `server: None` keeps the
    timely-exit-is-the-proof shape (nextest stdin is non-terminal:
    the guard must refuse, never block).
  - **L9b (new)** — `Command::Connect` (flags, no
    `--switch-backend`) through `dispatch` under explicit standalone:
    exit 1, same kind — the verb's non-TTY refusal at the dispatch
    tier.
  - **L10–L12 (unchanged bodies)** — the write-path table keeps
    driving `Command::Login` (the alias shares `connect_command`'s
    resolution; the journeys stay green through the new path).
  - **L13 (new)** — the notice, structurally: alias happy path
    (wiremock + env store via `dispatch`) ⇒ exit 0, stdout JSON has
    the receipt fields, **stderr non-empty**; the identical
    `Command::Connect` happy path ⇒ **stderr empty**. No content
    assertions.
- `tests/dispatch_routing.rs`: connect/alias failure-routing rows
  (exit 1 through `dispatch`) mirroring the login rows.

### 7.3 Lane + session pins

- `ipc_routing_tests` (mod.rs): `Command::Connect { .. } → Local`
  (row added to the `local` array).
- `tests/repl.rs` line table: `refuse("connect --server http://x")
  == LineRefusal::Connect` (and `connect` bare); completion's
  refused/offered pins update for the five-entry `REFUSED_VERBS`.

### 7.4 The daemon heads-up

- Pure `daemon_warning` arms (unit, §7.1's in-file tier).
- The probe/wiring is structural only: a happy-path dispatch test
  with **no** daemon answers (hermetic — nothing listens) pins
  stderr-empty-under-no-daemon (the warning's `None` arm end to
  end); the daemon-present arm is exercised where a real daemon
  exists — the docker/IPC tiers (`cli_ipc.rs` boots daemons in
  child processes) get one leg: connect while a daemon serves the
  same db ⇒ exit 0, stdout receipt unchanged, stderr non-empty. If
  wiring that leg against the existing `cli_ipc` harness proves
  disproportionate in stage 2, the pure-fn pin + the probe seam's
  existing coverage stand, and the leg is recorded as skipped-noted
  — never silently dropped (AGENTS §5: report the gap).

### 7.5 Binary tiers (journeys)

- **J4 (docker tier, `tests/it_cli.rs`, `#[ignore]` + env gate)** —
  the pipe-stdin connect journey mirroring J1's login step, via
  `run_cli` with `TASKBOARD_APP_PASSWORD` + env store:
  1. fresh config (`write_test_config(None, None)`): `connect
     --server <url> --user <user>` with the token piped → exit 0;
     config written with server/user; stdout receipt:
     `backend_switched == false`, `repointed == false`.
  2. re-point leg: `connect --server <url> --user <other-user>
     --force` (a second docker user, or the same server + a
     different name — the identity pair differs) → exit 0;
     `repointed == true`; config names the new identity.
  3. standalone flip leg: fresh config seeded with `backend =
     'standalone'`: `connect --switch-backend …` → exit 0;
     `backend_switched == true`; the file carries
     `backend = 'nextcloud'`; a follow-up `boards list --json`
     works (the flipped install syncs).
- **Pty leg (`tests/cli_tty.rs`)** — the one thing hermetic fakes
  cannot cover: the real `is_terminal` confirm. Drive the real
  binary under `script(1)` with a standalone-seeded config, no
  flags; feed `y` after the confirm prompt arrives on the pty (the
  harness's prompt-gated feeding) → exit 0, `backend_switched ==
  true` in the stdout JSON. Skipped when `script` is absent (the
  file's existing guard); wording never asserted — only the prompt's
  arrival gates the input.

### 7.6 Coverage of the dispatch acceptance list

| Dispatch acceptance | Pinned by |
|---|---|
| every D2/D3 guard mode scripted (flag; confirm-yes/decline; no-TTY refusal) | C1–C5, C6–C9, L9/L9b (dispatch tier) |
| re-point confirm/`--force`/decline with old-secret deletion after the new verify | C6, C7, C8, C12, C14 (ordering trap) |
| same-identity re-verify idempotent, no prompt | C10 |
| alias routes to the same body, both flags false, notice on stderr | C4, C9, L13 (structural), `login_command`'s delegation |
| structural assertions on the written TOML | C1, C6, C17 (+ J4 legs) |
| a piped/`--json` invocation can never block on a confirm | C5, L9/L9b, `read_confirm` non-TTY unit |
| lanes pinned | §7.3 |
| E2E mirrors the L-series shape | J4 + pty leg |
| verify-before-persist holds in the extracted ceremony | C13, C14 (+ stage-3 grep: no persist path reachable without a verified client) |
| hermetic + E2E green; gates green | §10 |

## 8. Recorded decisions (stage 2 implements exactly these)

- **D1 — the alias stays visible in help, doc-marked.** Hiding is
  part of the removal pass (v0.1.0 release prep, design D1's
  timeline), not the deprecation; a visible deprecation notice in
  `--help` is the discoverability bridge to `connect`. `Connect` is
  declared before `Login` so the catalogue reads primary-first.
- **D2 — the alias never escalates to a confirm
  (`ConnectArgs::confirmable`).** Design D1: the alias's refusals
  are unconditional ("keeps refusing… naming `connect
  --switch-backend`") and its surface "strictly narrower than its
  replacement". A TTY `login` under explicit standalone that walked
  the user through a backend flip would contradict both — the
  narrower surface is modeled as an explicit policy bit the alias
  sets false, not as a second ceremony body (one behavior, one test
  home). The REPL's future `login` line (chunk 3) inherits the same
  bit. Pinned by C4/C9 (the unconsumed-consent queues).
- **D3 — `read_confirm` returns `Result<bool, io::Error>`**, not the
  design §5.4 sketch's bare `bool`. Recorded deviation: design §5.2
  requires Ctrl-C during a prompted read to "abort with the existing
  typed `CommandError::Prompt`" — a cancellation must be
  distinguishable from a decline, and a bare `bool` cannot carry
  both. The `Result` shape mirrors `read_line`/`read_secret` (`Err`
  → `CommandError::Prompt`), so chunk 3's disposition protocol plugs
  in without re-touching the seam. Chunk 2 defines the seam; it must
  be cancel-capable from day one.
- **D4 — the never-blocks rule keys on stdin TTY-ness alone**;
  `--json` is orthogonal (design §3.1: "pipes, `--json` scripts,
  cron" all share non-terminal stdin; a `--json` human at a keyboard
  may confirm). Non-TTY `read_confirm` answers `Ok(false)`
  immediately, without reading — a piped `y\n` is never consumed and
  never waits.
- **D5 — D2 runs before identity gathering** (D3 necessarily after:
  it compares identities). Recorded deviation from design §7's
  "gather → guards" comment ordering: L9's pin ("refuses login
  before any prompting") and UX economy — a declined structural
  refusal should not cost a typed password. The only prompt a
  declining D2 can have issued is the confirm itself. Both orders
  satisfy the invariant that matters (nothing persists unless
  verified); §7's comment is a summary, not a sequence contract.
- **D6 — the backend word is written iff the D2 switch decided it.**
  The auto path never materializes `backend = "nextcloud"`:
  `logout` strips the identity keys iff `backend.is_none()`
  (fact 3), so a materialized word would silently break the next
  logout. Only a loudly-switched explicit `standalone` becomes an
  explicit `nextcloud`. Pinned by C17 (the key's absence) and the
  J4 flip leg (its presence).
- **D7 — re-point secret sequence: store(new), then delete(old);
  the deletion tolerates `NotFound` and `Unsupported`.** Reasons:
  the new credential must be usable before the old one is removed
  (a crash between leaves an orphan, never a broken install —
  "overwrite without orphans" is the goal, orphan-on-crash is the
  tolerable residue, logged); `EnvCredentialStore::delete` answers
  `Unsupported` (fact 7), so an env-store re-point must tolerate it
  exactly as the store step tolerates the write's `Unsupported` —
  or every env-store re-point would fail. Pinned by C6/C16.
- **D8 — D3 keys on a complete configured identity** (both
  `server_url` and `username` present). A half-configured file is
  treated as unconfigured: the overlay completes it, no deletion is
  attempted (no complete old tuple exists). Pinned by C11.
- **D9 — one receipt: `ConnectReceipt` replaces `LoginReceipt`.**
  The alias emits the identical JSON (the ceremony is shared; a
  separate alias receipt would be a second home for one behavior).
  `backend_switched`/`repointed` are the design-§7 additions;
  `attached` is deferred to chunk 3 (this chunk's connect is
  pre-core — there is nothing to attach to in a one-shot).
- **D10 — the two kinds are `backend-explicit-standalone` and
  `connection-exists`, no `error_detail`** for either. Recorded
  kind change: login-under-explicit-standalone moves off
  `sync-disabled` (design §3.1 names the alias's refusal kind
  explicitly; pre-v0.1.0 changes sanctioned, fact 9). `sync-disabled`
  keeps its connector-prelude meaning. Detail stays `None`: the
  refusing caller knows both identities and the kind + Display
  guidance already name the actionable flag; a detail string would
  be message text in structural clothing.
- **D11 — the daemon heads-up: stderr-only, success-path-only,
  probe-mirrored, alias included, no receipt field, no exit-code
  change.** Full reasons in §6. Upgrade path: the future daemon-side
  attach verb replaces the warning with a nudge + typed receipt
  field (WI-H's shape); until then the gap is documented, not
  contracted.
- **D12 — `login.rs` is deleted; everything that survives moves to
  `connect.rs`.** `PromptReader`/`StdinPrompts`, the overlay writer,
  `deck_variant`, and the receipt render live with the ceremony (the
  seam is ceremony-shaped, not login-shaped); `connect` importing
  them from a deprecated-named module would read backwards. The
  alias is dispatch-level in `mod.rs` (notice + delegate), which is
  all D1 says it is. The L-series ceremony pins move with the seam
  (§7.2); the file's routing pins stay.
- **D13 — the three refusal-shaped session touches are chunk 2's**
  (`dispatch_line`'s forced arm, `LineRefusal::Connect`, the
  five-entry `REFUSED_VERBS`), recorded as a deviation from design
  §11's file assignment (repl.rs/completion.rs to chunk 3): the
  clap addition is compile-forced in `dispatch_line`/`route_for_ipc`
  (fact 5), and without the refusal + refused-verbs entries a
  `connect` line would reach an `unreachable!` arm or completion
  would offer a verb the session refuses. All three are replaced —
  not widened — by chunk 3's ceremonies (the refusal variant and
  the two list entries die there).
- **D14 — guard composition.** D2 evaluates first (the backend word
  is the structural choice; `Some(Nextcloud)` never fires D2), then
  D3 against the configured identity. A contradictory config
  (explicit standalone *with* a configured identity) walks both
  guards in sequence — flag/confirm each. Pinned by C12.

No other deviations from design §3.1/§3.2/§7: the verb shape, the
guards' three modes each, the alias's narrower surface and notice,
the `Local` lanes, and the verify→persist→store spine are implemented
exactly as written.

## 9. Risks

- **The kind change for login-under-standalone** (D10):
  `sync-disabled` → `backend-explicit-standalone`. Any script keying
  on the old kind for that one refusal sees the new one. Sanctioned
  (the design names the kind; pre-v0.1.0), but it is the one
  externally visible contract change in the chunk — flagged for the
  PR description.
- **The alias-never-confirms reading (D2)** is an interpretation of
  design D1's "strictly narrower… keeps refusing". If the user meant
  a TTY confirm for the alias too, the fix is deleting the
  `confirmable: false` wiring in `login_command` — one line, and C4/
  C9 invert. Localized either way; flagged at PR review.
- **Test-move equivalence (L1–L8 → C13–C19).** The largest mechanical
  risk is a moved test arriving subtly weakened (a dropped assert, a
  relaxed queue check). Mitigation: the plan names the mapping 1:1;
  stage 3 diffs every touched test file against its base and checks
  each moved pin's assertions survived verbatim (dispatch mandate:
  environment/speed changes only, never weakened assertions).
- **The confirm's TTY edge in the real binary.** Hermetic tiers fake
  it; only the pty leg exercises `is_terminal` truthfully. The
  mechanism mirrors `read_secret`'s existing TTY branch (fact 1),
  and the pty harness's prompt-gated feeding is the established
  pattern — but a regression here is invisible to nextest's default
  run. The pty leg is the pin; its `script`-absent skip is the
  remaining hole (accepted, pre-existing for login).
- **toml_edit table creation on the switch path.** Writing
  `sync.backend` into a file with no `[sync]` table creates it
  implicitly; decor around existing `[sync]` content must survive.
  Pinned by C17 (both the created and the updated-table cases).
- **The daemon probe on every successful connect.** One unix-socket
  connect per invocation — negligible next to the verify HTTP call —
  and `try_connect` is the seam the logout nudge already exercises.
  A *hung* socket (half-open) would bound the probe by the IPC
  client's own connect timeout — inherited, not new. If stage 2
  finds the probe's failure modes richer than `Option`, it stops and
  records rather than inventing outcomes (D11 stays boolean).
- **Temporary dead surface.** `LineRefusal::Connect` and the
  `REFUSED_VERBS` entry exist to be deleted by chunk 3. If chunk 3
  stalls, the surface is honest (the session refuses a verb it
  cannot serve) but the deprecation debt grows — noted for the
  backlog, not this chunk.

## 10. Commit plan (stage 2) & diff estimate

Each commit standalone-green (rebase-merge hygiene; tests land with
their behavior; no fixup subjects):

1. `refactor(app): extract the login ceremony into the connect
   module` — connect.rs lands with the moved body, `PromptReader` +
   `read_confirm` + `StdinPrompts`, the overlay writer, the renamed
   receipt, and the core C-series pins (C13–C19 + the unit tier);
   login.rs deleted; `login_command` delegates with alias args +
   notice; L-series ceremony pins move; L9's kind flips (covered).
2. `feat(app): connect verb with backend-switch and re-point guards`
   — the clap variant, `connect_command`, the D2/D3 guards +
   `BackendExplicitStandalone`/`ConnectionExists`, the lanes row,
   `dispatch_line`'s arm, the REPL refusal + `REFUSED_VERBS`, and
   the guard matrix (C1–C12, L9b, L13, routing/repl/completion
   pins).
3. `feat(app): daemon heads-up after one-shot connect` — the probe
   + pure warning + their pins (§7.4).
4. `test(app): connect pipe and pty journeys` — J4 (docker tier) +
   the cli_tty confirm leg.

Estimated diff: **~0.9–1.1k net lines (~1.3–1.4k churn) across ~12
files** — production: `cli.rs` +30, `commands/connect.rs` NEW ~+430
(in-file unit pins included), `commands/login.rs` −251 (deleted),
`commands/mod.rs` ~+95, `commands/output.rs` ±25, `repl.rs` +8,
`completion.rs` +2; tests: `cli_connect.rs` NEW ~+520,
`cli_login.rs` net ~−140 (moves minus the new routing pins),
`common/mod.rs` +18, `it_cli.rs` +95, `cli_tty.rs` +55, `repl.rs`
+6, `dispatch_routing.rs` +35. Size "M" as dispatched (test-dominated,
like the chunk's dispatch estimate). No new workspace dependencies;
no `Cargo.toml` changes; no other crate touched.

## 11. Gates (stage 2, AGENTS §10 — all green before handoff)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
cargo deny check
cargo llvm-cov --workspace
```

No push, no PR (stage 3 ships). Backlog origin entries untouched
(chunk 3 lands them). Never push to or merge `main`.
