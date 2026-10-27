# [Plan] IPC Client Cancellation, Deadlines, Control Lane (DR-2/I)

- **Date**: 2026-10-27
- **Kind**: stage-1 TDD plan for chunk DR-2/I of the 2026-10-26
  six-rounds dispatch (issue #470, branch `feat/ipc-client-cancel`;
  closes #196/#470 with the chunk PR).
- **Origin** (normative):
  `.artifacts/plans/2026-10-26-signal-handling-design-plan.md` §3
  D-5 — items 1–4 applied item by item in §3–§6 below — plus D-0
  (orphan completion), D-3 (the ceremony exclusion), D-7
  (placement/platform), D-9 (the seams), §2 facts F11–F15, §5 risks,
  §6 chunk I; the dispatch plan §1/§5.2. This plan applies those
  decisions to the code at `e67bbc3` and resolves the
  implementation-level questions they leave open (§9 — every
  resolution is recorded there; none re-opens a D-decision).
- **Precondition check**: DR-2/R merged — `feat/repl-signal-cancel`
  is fully contained in `origin/main` by patch-id (`git cherry`
  clean; the local branch's pre-rebase SHAs differ, the rebase-merge
  signature), and `signals.rs` is on `main` @ `e67bbc3`.
- **Branch / worktree**: `feat/ipc-client-cancel` in
  `../taskboard-WT-feat-ipc-client-cancel`, off `origin/main` @
  `e67bbc3`. Doctor passed (248G free, floor 25G).
- **Status**: PLANNED — stage 2 implements; stage 3 reviews and ships
  (closes #196/#470).

---

## 1. Mission & the diff-shape rule

The one-shot CLI surface joins the one signal policy (D-0) and the
IPC client grows the bounded stance ADR 0012 §4.8's amendment
prescribes: SIGINT during a one-shot operation cancels it (typed
`cancelled`, exit 130, tidy teardown); every daemon request carries a
read deadline derived from the verb's own budget so a *wedged daemon*
fails typed instead of hanging forever; and `daemon stop` is answered
outside the serial dispatcher queue so a wedged in-flight verb can no
longer starve it (F12). No `Cancel` wire verb (§6 records the
rationale verbatim). The wire is untouched: same variants, same
envelope, no `PROTOCOL_VERSION` bump.

As a review-checkable rule (dispatch §5.2 + review prompt I.3):

- **Allowed**: `crates/taskboard-app/src/{signals.rs,
  ipc/client.rs, ipc/server.rs, config.rs, repl.rs (only the
  `changed_or_never` move), commands/mod.rs, lib.rs (no change
  expected — the modules exist)}`; `crates/taskboard-app/src/main.rs`
  (one-shot arms **only**); `crates/taskboard-app/tests/{ipc.rs,
  cli_ipc.rs, cli_tty.rs, config.rs}`; dated amendment to
  `docs/adr/0012-*.org`; `CHANGELOG.md` Unreleased.
- **Forbidden**: any hunk to `taskboard-domain`/`taskboard-state`/
  `taskboard-storage-sqlite`/`taskboard-sync-nextcloud`/
  `taskboard-ui-slint`; any hunk to `protocol.rs` or
  `PROTOCOL_VERSION` (review rule: the file diffs only if a reviewer
  mistake did); any hunk to `main.rs`'s daemon tail
  (`daemon_lifecycle`/`boot_and_serve`/`serve_and_shutdown`/
  `drain_and_shutdown`/`run_kiosk` — chunk DR-2/D) or to the
  bare-invocation routing (`BareRoute` arms — R landed the REPL, D
  owns the rest); any new `Request`/`Response` variant, field, or
  envelope change; any new dependency; any `unsafe`; any
  `PROTOCOL_VERSION` bump.
- **The server's fairness model keeps its shape**: everything except
  `Shutdown` still executes one-in-flight on the single dispatcher
  task (ADR 0012 §2.8 pins the model; the lane is the one recorded
  exception).
- **Handlers are never on test paths (D-7)**: the chunk's only signal
  installation is `install_oneshot_watcher()`, called from the
  `main.rs` one-shot arms and the `status`/`daemon stop` commands —
  production paths only. Library code (`client.rs`, `server.rs`)
  never installs; hermetic tests inject watches and race no real
  signal; the pty tier is the only place a real SIGINT occurs in a
  test (design §5, last risk).

## 2. Verified baseline (seam facts at `e67bbc3`)

All anchors read at `e67bbc3`; the files this chunk touches are
byte-identical to what the design round verified at `cdc41c6`
(client.rs/server.rs/protocol.rs/config.rs), plus DR-2/R's landed
`signals.rs`.

- **Client, the unbounded reads (F11)**: `ipc/client.rs` — module
  doc states the policy verbatim ("There is deliberately no read
  deadline: every verb self-bounds and a wedged daemon is a
  supervision problem", `:7-14`) — **this paragraph is the in-code
  home of the §4.8 sentence the amendment replaces**. `request()`
  (`:129-140`) = serialize → `write_frame` → `read_frame` → decode →
  `validate_response`, every failure mapped to
  `CommandError::IpcTransport`. `exec`/`status`/`shutdown`
  (`:149-204`) call `request` unbudgeted. `detach`/`attach`
  (`:217-276`) already wrap their whole request in
  `tokio::time::timeout` with 30 s constants and map expiry to
  `IpcTransport`. `try_connect` (`:87-117`): concrete refusals →
  `None` immediately; ambiguous failures retried to a 1 s
  `CONNECT_BUDGET` (`tokio::time::sleep` between tries — cancellable
  by dropping).
- **Client callers (all of them)**: `main.rs:140-149` — the
  `IpcOrLocal` transport arm (`try_connect` → `conn.exec(&raw_argv)`,
  `Err` → report + `std::process::exit`); `main.rs:241-247` —
  `status_command` (`conn.status()`); `main.rs:268-274` —
  `daemon_stop_command` (`conn.shutdown()`); `commands/connect.rs:641`
  — the one-shot connect ceremony's attach step (`conn.attach`);
  `commands/logout.rs:164` — one-shot logout's detach step
  (`conn.detach`). The last two are ceremony-adjacent and stay on
  their bespoke budgets (§9 Q4).
- **Server, the serial dispatcher (F12)**: `ipc/server.rs:176-186` —
  the dispatcher loop receives `InboundRequest{req, reply}` from the
  mpsc queue, calls `handle()`, inspects the response for
  `Response::Shutdown` and fires the stop watch **inside the
  dispatcher**, then sends the reply. A `Shutdown` therefore queues
  behind any in-flight exec — the starvation F12 names.
- **Server, the accept path**: `serve_connection`
  (`server.rs:322-363`) — read frame → decode → **version gate**
  (`:337-344`, answered in the connection task, no engine
  involvement) → enqueue to the dispatcher → await the oneshot reply
  → write the response frame. The decode already happens here, which
  is why the control lane needs no wire change. `serve_connection`'s
  current signature takes only `stream` + `in_tx` — the control lane
  adds a `watch::Sender<bool>` clone.
- **Server, the drain**: `IpcServerHandle::join(budget)`
  (`server.rs:118-148`) stops accepting, bounds the wait on the
  accept+dispatcher tasks, aborts past the budget, removes the
  sidecar. `drain_tests` (`server.rs:678-704`) is the
  hand-constructed-handle + parked-task precedent D-9 names. IP12
  (`tests/ipc.rs:451`) pins that a silent connection cannot wedge the
  drain — the lane must not disturb it.
- **The one-shot routing** (`main.rs:100-167`): `route_for_ipc`
  (`commands/mod.rs:1859-1886`) — `Local`: `connect`/`login`/
  `logout`/`config`/`boards list` (`gui` is intercepted before
  routing); `IpcOnly`: `status`, `daemon stop`;
  `IpcOrLocal`: everything else (task/stack/label verbs, `timers`,
  `find`, `capture`, `boards select/create/rename/publish`, `sync`).
  `IpcOnly` dispatches to `status_command`/`daemon_stop_command`
  (`main.rs:236-275`); `IpcOrLocal` tries the transport arm and falls
  through to the shared in-process `dispatch(command, ctx).await`
  (`main.rs:165`) — the arm `Local` reaches directly. One-shot
  ceremonies prompt on the caller's terminal (D-3/F14) — the
  exclusion set; one-shot `logout` prompts **nothing** (verified: no
  prompt/confirm/read_line in `logout.rs`).
- **The verb budgets (F15)**: exactly three `--timeout-secs` flags
  exist, all `u64` seconds defaulting to 60: `sync`
  (`cli.rs:136`, "Wait budget for the cycle"),
  `boards select` (`cli.rs:211`, "Adoption wait budget"),
  `boards publish` (`cli.rs:250`, "Wait budget for the settling
  first sync cycle"). Their meaning is daemon-side wait budgets (HTTP
  + engine); the client deadline must never preempt them.
- **Config sections** (`config.rs:67-101`): `AppConfig` fields are
  `#[serde(default)]` sections; the pin-less-configs-serialize-
  unchanged pattern is `WebSection`'s
  `#[serde(default, skip_serializing_if = "WebSection::is_default")]`
  (`:93`). Sections are `deny_unknown_fields` + kebab-case. No
  `[ipc]` section exists yet.
- **The signal seam (R's dependency)**: `signals.rs` — the pure
  `Ladder` (`new(middle_rungs)`, `on(Event) -> Option<Action>`,
  `last_signal()`), `Signal::Interrupt.exit_code() == 130`;
  `install_session_watcher` is the REPL's production installer (tty
  world: restore + terminate family). The dead-peer helper
  `changed_or_never` is private in `repl.rs:728-735` (`Err` →
  `pending()` forever). `CommandError::Cancelled` exists
  (`commands/mod.rs:371`, kind `"cancelled"`, `error_detail()`
  carries the orphan-completion note `:500-503`) — the one-shot
  reuses it verbatim.
- **Error reporting**: `report_to`/`report` (`commands/mod.rs:1941+`)
  print the `{"error":{"kind","detail"}}` envelope under `--json`,
  Display text otherwise, **always exit 1**. A new variant is one
  enum arm + one `error_kind` arm (+ optional detail).

## 3. I-1 — the one-shot cancellation select (design item 1)

D-5 item 1 applied: the transport arm and the in-process dispatch
both wrap their await in the cancellation select (D-2's mechanism,
one consumer wider); on SIGINT the connection closes / the dispatch
future drops, the typed `cancelled` envelope prints, exit **130**;
ceremony verbs are excluded per D-3.

### 3.1 `install_oneshot_watcher` (signals.rs)

```rust
pub(crate) fn install_oneshot_watcher() -> tokio::sync::watch::Receiver<u64>
```

- **Interrupt family only** — unix
  `signal(SignalKind::interrupt())`, Windows `tokio::signal::ctrl_c()`
  (the two feeders of `install_session_watcher`'s SIGINT half,
  `signals.rs:244-265`; the termination streams are *not* installed —
  the one-shot has no tty to restore and D-4 is a terminal-world
  rule; SIGTERM/SIGHUP keep their default disposition on this
  surface, §9 Q6). Registration failure degrades to never-fires (the
  `wait_for_signal` precedent).
- Drives **one `Ladder::new(0)`**: `Initiate` → `epoch += 1`, send on
  the cancel watch (watcher → caller). `Force` →
  `std::process::exit(130)` (no tty restore — nothing was ever saved;
  `Signal::Interrupt.exit_code()` is the code source, not a literal).
- **No ack half**: the one-shot exits after reporting the
  cancellation, so the ladder never de-arms — a second SIGINT inside
  the exit window forces (same 130, minus the envelope print). This
  is D-1's "the force rung is always armed behind the first signal"
  with the simplest possible acknowledgement policy (§9 Q7).
- Doc comment states the D-0 orphan-completion sentence for this
  surface: cancelling a relayed request drops the *wait*; a command
  already delivered to the daemon still completes.

**Tests**: none in-module beyond what the pure `Ladder` tables
already pin — the watcher is thin glue (house rule); the pty tier
pins the end-to-end journey (§7, case P1). The SIGINT-only feeder
shape is `cfg`-mirrored from the landed watcher.

### 3.2 `changed_or_never` moves to signals.rs

The dead-peer helper (`repl.rs:728-735`) becomes `pub(crate)` in
`signals.rs` (one home; `repl.rs` imports it — a mechanical move, no
behavior change; §9 Q8). Both the loop and the one-shot arms consume
it, so a dropped watcher can never fire an arm.

### 3.3 The install decision and the three select sites

One install decision at the top of the one-shot dispatch block in
`run()` (`main.rs`, after the `gui` interception):

```rust
// D-3: the ceremony verbs keep the default disposition — their
// prompts are sync reads inside poll (F14); a handler would consume
// ^C and leave the read silently running.
let cancel = match command {
    Command::Connect { .. } | Command::Login { .. } => None,
    _ => Some(taskboard_app::signals::install_oneshot_watcher()),
};
```

| site | today | wrapped shape |
|---|---|---|
| `IpcOrLocal` transport arm (`main.rs:139-150`) | `try_connect` → `conn.exec` plain await | one `transport` future (`try_connect` **and** `exec` — the connect-retry loop is cancellable too) selected against `changed_or_never(cancel)`; `Ok`/`Err` arms as today, cancel arm → `report(cli.json, &CommandError::Cancelled)` + `std::process::exit(130)` |
| in-process dispatch (`main.rs:165`) | `dispatch(command, ctx).await` | selected against the same cancel receiver (fall-through and `Local` share the site); cancel arm → report + exit 130; completion arm → today's `exit(code)` |
| `status_command` (`main.rs:236-259`) | `try_connect` → `conn.status` | the transport part selected against cancel; cancel arm → report + **return 130** (the fn returns `u8`; the caller exits) |
| `daemon_stop_command` (`main.rs:263-275`) | `try_connect` → `conn.shutdown` | same shape; cancel → return 130 |

- `IpcOnly` verbs are never ceremonies — both fns install
  unconditionally.
- **A cancelled `status` exits 130** (not 0): the uniform ^C
  convention; an interrupted observation is not a completed one
  (§9 Q10).
- The envelope is `CommandError::Cancelled` verbatim — kind
  `"cancelled"`, the orphan-completion detail already in place. Under
  the relay the dropped request future closes the socket; the daemon
  completes or never saw the frame — orphan completion, named not
  hidden (D-0).
- The select covers the in-process boot: `dispatch` internally boots
  (`core_ctx`/`run_mutation`), so ^C during a boot drops the boot
  future (bootstrap is bounded and idempotent file work; §9 Q11).

**Tests**: the select arms are `main.rs` glue — untested per house
rule; pinned end-to-end by the pty case P1 (§7). The `Cancelled`
envelope shape is already pinned at the loop tier (R landed it);
nothing new to unit-test here beyond what §7's client tests cover.

## 4. I-2 — the client read deadline (design item 2)

D-5 item 2 applied: `exec`/`status`/`shutdown` gain a read budget;
expiry is the new typed `CommandError::IpcTimeout` (kind
`"ipc-timeout"`, distinct from transport garbage), exit 1.

### 4.1 The derivation table

| request | verb's own `--timeout-secs` | derived deadline |
|---|---|---|
| `exec` of `sync` | yes (default 60) | `t + 10 s` |
| `exec` of `boards select` | yes (default 60) | `t + 10 s` |
| `exec` of `boards publish` | yes (default 60) | `t + 10 s` |
| `exec` of every other verb | none | 30 s (default ceiling) |
| `status` | — (no flag exists) | 30 s |
| `shutdown` | — (no flag exists) | 30 s |
| `detach` / `attach` | (ceremony-adjacent) | **unchanged**: their existing 30 s `tokio::time::timeout` constants, expiry → `IpcTransport` (§9 Q4) |

- The 10 s fixed margin guarantees the design's "never fires before
  the verb's own honest timeout" for the flag world: derived =
  `t + 10 > t` for every `t` (pinned as a property, §7 test C1).
- `status`/`shutdown` carry no verb budget: the default ceiling
  catches only the wedged-daemon case (the ack/read of a healthy
  daemon is millisecond-scale; a draining daemon's `shutdown` ack is
  sent before the drain begins — the budget does not wait for the
  drain).

### 4.2 The pure derivation helpers

```rust
// commands/mod.rs, beside route_for_ipc (the per-Command table home):
pub(crate) fn verb_timeout_secs(command: &Command) -> Option<u64>
// Sync{timeout_secs, ..} | Boards{Select{timeout_secs, ..}|Publish{timeout_secs, ..}, ..} => Some(t); _ => None

// ipc/client.rs, beside the constants:
pub(crate) const DEFAULT_READ_BUDGET_SECS: u64 = 30;
pub(crate) const READ_BUDGET_MARGIN_SECS: u64 = 10;
pub(crate) fn client_read_budget(verb_secs: Option<u64>, configured: Option<u64>) -> Option<Duration>
```

`client_read_budget` — the full override lattice:

| `configured` (`[ipc] client_read_timeout_secs`) | outcome |
|---|---|
| `Some(0)` | `None` — the deadline is **disabled** (the recorded supervision stance, spelled as a value) |
| `Some(n)` | `Some(n secs)` — replaces the derivation **wholesale**, flag margin included (§9 Q3) |
| `None` (key absent) | `Some((verb_secs.unwrap_or(DEFAULT) + margin) secs)` |

main.rs computes `client_read_budget(verb_timeout_secs(&command),
config.ipc.client_read_timeout_secs)` once per arm and hands it to
the connection call. Both helpers are pure and unit-tested (§7 C1/C2).

### 4.3 The wrap: one place, three verbs

`Connection::request` gains a `read_budget: Option<Duration>`
parameter (`None` = disabled) and wraps its write→read→decode future
in an Option-aware `maybe_timeout` (a three-line local helper mirroring
`tokio::time::timeout` for `Option<Duration>`). Expiry maps to
`Err(CommandError::IpcTimeout)` — **inside** `request`, so all three
verbs inherit the deadline and exactly one expiry test pins the wrap
(§9 Q1). The public methods gain the parameter and pass through:

```rust
pub async fn exec(self, argv: &[String], read_budget: Option<Duration>) -> Result<ExecOutcome, CommandError>
pub async fn status(self, read_budget: Option<Duration>) -> Result<StatusView, CommandError>
pub async fn shutdown(self, read_budget: Option<Duration>) -> Result<(), CommandError>
```

- `detach`/`attach` pass `None` internally (their outer
  `tokio::time::timeout` constants and `IpcTransport` expiry mapping
  stay byte-for-byte — §9 Q4).
- **Wrapping scope = the whole request (write+read+decode), not the
  bare read** — matching the detach/attach precedent; on a local
  socket the write cannot wedged-hang meaningfully and one deadline
  over the whole exchange is the honest budget (§9 Q2).
- Internal-API churn (three call sites in `main.rs` + tests) is
  accepted — AGENTS §2: the API is internal, agility is free.
- The module doc's "deliberately no read deadline" paragraph
  (`client.rs:7-14`) is rewritten to the bounded stance: verbs still
  self-bound; the deadline catches only the wedged daemon; the
  override + disable exist for supervision; expiry is typed
  `ipc-timeout`, never transport garbage.

### 4.4 `IpcTimeout` and the config surface

- `commands/mod.rs`: `#[error("daemon ipc read timed out")]
  IpcTimeout` — `error_kind() → "ipc-timeout"`, `error_detail() →
  None` (the kind is the machine contract; the effective budget is
  client-side bookkeeping the caller derived from its own config and
  flag — §9 Q9). Exit 1 comes from `report` (verified: it always
  yields 1).
- `config.rs`: a new `IpcSection` following the `WebSection` shape —
  `#[serde(default, deny_unknown_fields)]` +
  `#[serde(rename_all = "kebab-case")]` on the section, one field
  `client_read_timeout_secs: Option<u64>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`, and
  `AppConfig.ipc` with
  `#[serde(default, skip_serializing_if = "IpcSection::is_default")]`
  so pin-less configs serialize unchanged. `TASKBOARD_IPC__CLIENT_READ_TIMEOUT_SECS`
  overrides via figment for free (the env lane). The field's doc
  comment carries the deliberate-leniency note: a configured value
  below a verb's honest budget **can** preempt an honest long verb —
  that is the operator's explicit choice, documented at the type
  (AGENTS §2 A1 posture; §9 Q3).

## 5. I-3 — the `Shutdown` control lane (design item 3, fixes F12)

D-5 item 3 applied: a decoded `Shutdown` is answered and triggers the
drain **directly in the accept path**, without entering the serial
dispatcher queue; the dispatcher keeps its one-in-flight fairness for
everything else.

### 5.1 The change (server.rs)

- `serve_connection` gains a `stop: watch::Sender<bool>` parameter
  (a clone per connection from `accept_loop`, which already holds
  one). After the **version gate** (unchanged position — a
  wrong-version `Shutdown` still gets `VersionMismatch`, exactly as
  today, §9 Q13), a new interception:

```rust
if matches!(req, Request::Shutdown { .. }) {
    // The control lane (ADR 0012 amendment): answered here, never
    // enqueued — a wedged in-flight verb cannot starve `daemon stop`
    // (F12). Stop fires before the ack is written: today's
    // dispatcher ordering, preserved verbatim.
    let _ = stop.send(true);
    let ack = serde_json::to_vec(&Response::Shutdown { v: PROTOCOL_VERSION })
        .expect("response serialization is total");
    let _ = codec::write_frame(&mut stream, &ack, codec::MAX_RESPONSE_BYTES).await;
    return;
}
```

- The dispatcher loop **loses** its `stop_for_dispatcher` clone and
  the `if let Response::Shutdown` inspection — it becomes a pure
  serial queue: receive → `handle()` → reply. Exactly one owner of
  the stop-from-wire decision remains (the connection task).
- `handle()`'s `Request::Shutdown` arm stays (the match must be
  total) and keeps answering the honest ack, plus a
  `tracing::warn!("shutdown request reached the dispatcher; the accept path owns it")`
  canary — unreachable from the wire after this change; the warn is
  what a future regression (someone re-enqueueing) trips first
  (§9 Q12).

### 5.2 Why this is wire-additive (the amendment's argument)

- **No new variant, no field, no envelope change**: the same
  `Request::Shutdown` bytes travel; only *which task* answers and
  *when* (earlier, outside the queue) changed. `Response::Shutdown`
  bytes are identical. `PROTOCOL_VERSION` stays 1; `protocol.rs` is
  not touched.
- **New daemon ↔ old client**: the old client sends `Shutdown` and
  awaits the ack — identical bytes come back (from the connection
  task instead of the dispatcher). Unaffected.
- **New client ↔ old daemon**: the old daemon answers from the
  dispatcher — today's behavior, including F12's starvation window.
  No client-visible protocol difference; the fix is daemon-side-only
  (the one-binary shipping rule makes the mixed window the
  post-upgrade case only, ADR 0012's own analysis).
- The acceptance lane rule (`daemon stop`'s drain semantics) is
  unchanged: the ack precedes the drain, the accept loop still stops
  on the watch, `join(budget)` still bounds the wedged in-flight
  exec's connection (its client sees a dropped socket → typed
  `IpcTransport`, or its own `IpcTimeout` first). The daemon ladder's
  rung table is chunk D's; nothing here reorders it.

## 6. I-4 — the no-`Cancel` recording & docs (design item 4, D-8)

### 6.1 The recording (verbatim draft for the ADR amendment)

> **No `Cancel` request exists, by decision (2026-10-26):** orphan
> completion makes server-side cancellation valueless for
> correctness — a durable command should complete, and a relayed
> `sync` finishing after its client left is harmless (the next
> command observes the same converged state). Preempting the serial
> dispatcher's in-flight future would restructure the engine-fairness
> model (§ "Serial execution; accept concurrently") for no gain. If
> a future verb grows genuinely expensive-and-pointless-when-unwatched
> work, that verb's design round re-opens this with the evidence.

### 6.2 Where the docs ride

- **ADR 0012** (`docs/adr/0012-*.org`): one dated amendment block
  (*Amendment (2026-10-27): client deadlines, the shutdown control
  lane, no Cancel verb*), appended after the wire-version-policy
  section, containing: (a) the bounded §4.8 stance — the derivation
  table's rule (flag + 10 s margin, else 30 s), the
  `[ipc] client_read_timeout_secs` override with 0-disables, and
  `ipc-timeout` as a kind distinct from `ipc-transport`; (b) the
  control lane — `Shutdown` answered in the accept path, wire-additive
  under the additive-variants policy, old-daemon behavior unchanged;
  (c) the §6.1 recording. The prior text stays visible below (D-8's
  amendment-not-rewrite rule).
- **client.rs module doc**: the bounded stance (§4.3).
- **CHANGELOG.md Unreleased**: one entry covering the
  user-observable set — ^C during a one-shot command cancels typed
  and exits 130; daemon requests carry read deadlines (`[ipc]
  client_read_timeout_secs`, `ipc-timeout`); `daemon stop` answers
  even while another request is in flight.

## 7. The test design (TDD; D-9's IPC rows + the pins this plan adds)

Naming: C = client hermetic, S = server hermetic, J = integration
journey (`tests/ipc.rs`), E = binary e2e (`tests/cli_ipc.rs`), P =
pty (`tests/cli_tty.rs`). No sleeps anywhere: the budget is a
`tokio::time::timeout`, so the clock is `pause()`/`advance()`
wherever the budget is under test; the binary journeys use bounded
real time per the suites' existing convention (IP11/IP12 precedent).

| id | tier | case | assertions (structural, typed) |
|---|---|---|---|
| C1 | unit (client.rs) | the derivation table + margin property | every row of §4.1/§4.2's lattice: flag verbs derive `t+10`, flag-less derive 30, `Some(n)` overrides wholesale, `Some(0)` → `None`; and for every `t`, `client_read_budget(Some(t), None) > t` (never preempts an honest verb) |
| C2 | unit (client.rs) | `verb_timeout_secs` table | `sync`/`boards select`/`boards publish` yield their field; every other variant yields `None` (exhaustive match — a future flag must land here, review-checkable) |
| C3 | unit (client.rs) | deadline expiry against a **scripted stalling server** | a local-socket listener that accepts and never answers; `tokio::time::pause()`; `conn.exec(argv, Some(small))` → `advance(budget + 1)` → `Err(IpcTimeout)`; the server observed the frame but wrote nothing. Under `#[tokio::test(start_paused = true)]` |
| C4 | unit (client.rs) | disabled deadline never fires | same stall, `read_budget = None`; pin the future, `advance(Duration::MAX-ish)` → still `Pending` (noop-waker poll), then drop — no wall clock, no sleeps |
| C5 | unit (client.rs) | `IpcTimeout` is distinct from transport garbage | `error_kind() == "ipc-timeout"`; a transport failure (server drops mid-frame) still maps `ipc-transport` from the same `request` path |
| S1 | unit (server.rs, the `drain_tests` pattern) | the control lane answers outside a parked queue | a real listener + `serve_connection(stream, in_tx, stop_clone)` where `in_rx` is held **un-polled** (the parked dispatcher; a spawned `pending()` stand-in per D-9's wording): write a `Shutdown` frame → the ack frame comes back and the stop watch fires, while the queue stays clogged; a second connection's non-Shutdown request stays `Pending` (noop-waker) — queued, unanswered (§7 fairness note) |
| S2 | unit (server.rs) | wrong-version Shutdown still gets VersionMismatch first | the gate's position is unchanged: a `Shutdown` with `v ≠ 1` answered `version-mismatch`, the stop watch **not** fired |
| J1 | integration (tests/ipc.rs) | `daemon stop` answers mid-flight (the F12 journey) | `served_nextcloud` + wiremock delayed boards route (30 s); client A starts `boards select` (holds the dispatcher); client B `conn.shutdown()` with the control lane → ack promptly (bounded wrapper), `handle.join(1 s)` completes, sidecar removed; client A's wait ends in the drain abort → typed `IpcTransport`. The stop's latency no longer depends on A's |
| J2 | integration (tests/ipc.rs) | IP8's serialization pin survives the lane | two concurrent non-Shutdown requests still serialize and both answer (the existing `ip8` journey extended or re-run as-is — the lane must not have widened the queue) |
| E1 | e2e (tests/cli_ipc.rs) | the deadline override end-to-end | a fake stalling daemon (in-test listener bound on the db's socket name) + the real binary with `[ipc] client_read_timeout_secs = 1`; `taskboard --config X sync --json` → exit **1**, stderr envelope kind `ipc-timeout` |
| E2 | e2e (tests/cli_ipc.rs) | `daemon stop` mid-wedge at the binary level | the E1 fake daemon answering a `shutdown` request from the accept path is unit/S1 territory — E2 instead pins the *client* of a real daemon: existing `daemon stop` journeys keep passing unchanged (wire-identity proof); no new case unless stage 2 finds the harness already spawns one (likely: `cli_ipc`'s stop journey) |
| P1 | pty (tests/cli_tty.rs) | ^C during a stalled one-shot relay cancels typed | R's pidfile fixture; the fake stalling daemon bound first; under `script(1)`: `taskboard --config X sync --json` (stalls in the transport read) → `kill -INT <pid>` → exit **130**, stderr envelope kind `cancelled`; the stalling listener saw the socket drop (client-side close — the tidy teardown) |

- **Exclusion pins**: no new case for the ceremony exclusion at the
  hermetic tier — the install decision is a two-arm match in
  `main.rs` glue; the pty tier already pins one-shot `connect`'s
  default death (R's case 7) and it keeps passing (a `connect` pty
  run exercises the no-install arm by construction). Stage 3's
  review rule (ceremony verbs excluded) is a diff-shape check (§1).
- **Wire identity**: `protocol.rs`'s `wire_shapes_are_pinned` stays
  green untouched — that *is* the no-bump proof; S2 pins the gate's
  precedence; the amendment records the matrix.
- **No quality ceilings / no text assertions**: every assertion is an
  exit code, a parsed envelope `kind`, a watch/queue state, or a
  typed `Result` — the house rules, verbatim.

## 8. The test-first order (stage 2's TDD sequence)

Each step lands red-then-green; every commit standalone green:

1. `CommandError::IpcTimeout` + `IpcSection` + the two pure
   derivation helpers + C1/C2/C5's kind pin (pure tier).
2. The `request` wrap + C3/C4 (the stalling-listener rig; the
   signatures change here — `main.rs` call sites updated in the same
   commit to pass `None`/derived, keeping the tree green).
3. The one-shot select: `install_oneshot_watcher` +
   `changed_or_never`'s move + the `main.rs` arms (glue; no hermetic
   test — P1 arrives in step 5).
4. The control lane + S1/S2 (+ J1/J2).
5. E1 (+ E2 if the harness wants it) + P1.
6. The ADR 0012 amendment + client.rs module doc + CHANGELOG.

Suggested subjects:
`feat(app): the one-shot signal watcher` →
`feat(ipc): client read deadlines and the ipc-timeout envelope` →
`feat(app): cancel a one-shot command on SIGINT` →
`feat(ipc): the shutdown control lane in the accept path` →
`test(tty): the one-shot cancellation and deadline pins` →
`docs(adr): the ipc deadline, control lane, and no-cancel recording`.

## 9. Resolved questions (the decision record)

| # | question | decision | why / evidence |
|---|---|---|---|
| Q1 | Where does the deadline wrap live — per method, or inside `request`? | Inside `Connection::request` (a `read_budget: Option<Duration>` parameter); the three methods pass their derived value through | One wrap, one expiry test (C3), three verbs covered; the methods stay one-line adapters. Per-method `tokio::time::timeout` would triplicate the mapping the detach/attach precedent already duplicates twice |
| Q2 | Does the budget bound the bare read or the whole request? | The whole request (write+read+decode) | Matches the detach/attach precedent (`client.rs:227-238`); the write cannot wedged-hang on a local socket at these frame sizes, and one number over the whole exchange is the honest, documentable budget |
| Q3 | Does `[ipc] client_read_timeout_secs` replace only the 30 s default, or the flag+margin derivation too? | **Wholesale** — the configured value is the budget for every covered request; `0` disables entirely | The design's sentence ("overridable by … (0 disables)") names one knob for the whole stance; a supervisor disabling it wants *no* client-side deadline, and one pinning a tight one wants exactly that. The preemption caveat is documented at the config field (deliberate leniency, A1 posture). Rejected default-only override: two interacting tiers with no use case |
| Q4 | Do `detach`/`attach` and `try_connect`'s connect budget join the new scheme? | No — unchanged | The design item 2 names `exec`/`status`/`shutdown` only. detach/attach are ceremony-adjacent with their own 30 s budgets and a *degrade-to-warning* expiry contract (the daemon still completes; remapping their expiry to `IpcTimeout` would change `logout`'s envelope for zero design intent). The 1 s connect budget is a fall-through signal, not a failure |
| Q5 | Does the deadline also cover a daemon's *queued* wait (dispatcher backlog)? | Yes — inevitably and correctly | The budget wraps the client's whole wait; a queued-behind-a-long-verb request may legitimately time out at the client while the daemon eventually answers into a dead socket (orphan completion). The margin + the override are the escape hatches; the alternative (a server-side queue-position header) is protocol surface the design did not ask for |
| Q6 | Does the one-shot watcher install the termination family (SIGTERM/SIGHUP restore-then-graceful)? | No — Interrupt only | D-4 is a tty-world rule (restore the line discipline); the one-shot never touches the tty (non-ceremony verbs run no raw mode). SIGTERM keeps the default disposition on this surface — today's behavior, unchanged; the daemon/GUI/kiosk tails are chunk D's. Recorded so the reviewer does not read the absence as a missed arm |
| Q7 | Ack policy for the one-shot ladder | No ack half — the ladder never de-arms; the second SIGINT in the exit window forces at 130 | The one-shot's cancellation *ends the process*; there is no "next line" to protect, so `Progress` has no consumer. The armed force rung is exactly D-1's "the escape exists" with zero extra machinery |
| Q8 | Where does `changed_or_never` live now that two surfaces consume it? | `pub(crate)` in `signals.rs`; `repl.rs` imports it | One dead-peer policy, one home — the module whose doc already owns the signal vocabulary. Mechanical move, no behavior change |
| Q9 | Does `IpcTimeout` carry a payload (the effective budget)? | No | The kind is the machine contract; the budget is derived from the caller's own config + flag — a script that set them knows the value. Rejected `secs` field: envelope churn with no consumer |
| Q10 | A cancelled `status` — exit 130 or 0? | 130 | The ^C convention is uniform across the one-shot surface (design item 1 names 130 for the cancellation, full stop); a partially-observed status is not a clean observation. The no-daemon client-side answer (0) is untouched — that path completed |
| Q11 | ^C during the in-process boot (inside `dispatch`) | Covered by the select — the boot future drops, exit 130 | `dispatch` internally boots; the select wraps the whole await. Dropping mid-bootstrap is safe (bounded idempotent file work; the durable-work rule is D-0's and the engine is not yet up to have received anything) |
| Q12 | What remains of `handle()`'s `Shutdown` arm and the dispatcher's stop inspection? | The dispatcher loses the stop clone + inspection (pure queue); `handle()`'s arm stays for match totality, answers the honest ack, and `warn!`s | Exactly one owner of the stop-from-wire decision (the connection task). The warn is the canary a future re-enqueue trips first; review rule I.3 states the invariant independently |
| Q13 | Does the version gate run before the control lane? | Yes — unchanged position | A wrong-version `Shutdown` got `VersionMismatch` before entering the queue today; the lane must not change that (S2 pins it). The lane intercepts only a well-versioned request |
| Q14 | Cancel-vs-timeout race at the same instant (SIGINT and expiry both ready) | Whichever `select!`/`timeout` arm tokio polls first — both are typed and safe | The cancel path reports `cancelled` + 130; the timeout path reports `ipc-timeout` + 1. No correctness difference; not worth pinning (a pin would assert a scheduling detail) |
| Q15 | Where does the `main.rs` budget live when config load failed? | It doesn't — a failed config load skips the transport arm entirely (today's `if let Ok(config)` guard) and the typed config error comes from the in-process dispatch, un-budgeted | Today's shape verbatim; a config failure is instant and typed, nothing to bound |

## 10. Risks

- **"Cancelled but it landed" on the relay** (D-0): a cancelled
  `exec` whose argv the daemon already dispatched still commits; the
  daemon's answer goes nowhere. Mitigated by the `Cancelled` detail
  text (already landed in R) and the ADR 0012 amendment naming
  orphan completion for this surface — never hidden behind a pretend
  rollback.
- **The override can preempt an honest long verb** (Q3): an operator
  setting `client_read_timeout_secs = 5` turns a legitimate 60 s
  `sync` into `ipc-timeout` while the daemon finishes the work.
  Documented at the config field and in the amendment; the
  flag+margin derivation is the default precisely so the honest
  world needs no override.
- **Queued-wait time-outs** (Q5): under load, a request behind a
  long verb can hit the client deadline before the daemon serves it.
  Same escape hatches; the serial queue is ADR 0012 §2.8's accepted
  model (a read-only fast lane remains the documented alternative if
  pain appears — unchanged by this chunk).
- **The dispatcher's lost stop inspection is a one-way door**: after
  this chunk, a `Shutdown` reaching the dispatcher means the lane was
  bypassed (the daemon would ack but not drain). The canary warn
  (Q12) plus review rule I.3 are the guards; `handle()`'s arm still
  answers honestly so the client never hangs even in the regression.
- **Stale-signal windows**: a SIGINT landing between dispatch
  completion and `process::exit` is inert (the epoch send has no
  observer; the process exits with its own code — no worse than
  today's default disposition); a SIGINT during the connect-retry
  loop cancels the whole transport future (an improvement, not a
  window).
- **Pty/e2e flakiness**: P1/E1 gate on blocking reads and bounded
  wrappers (suite rules, no sleeps); the fake stalling daemon is
  deterministic by construction (it never answers). Any observation
  goes to the `flaky-test` watchlist per AGENTS §7 — never fixed
  opportunistically here.

## 11. Diff estimate & commit plan

| file | shape |
|---|---|
| `src/signals.rs` | ~+55 (oneshot watcher + `changed_or_never` move-in) |
| `src/repl.rs` | ~−12/+2 (helper move) |
| `src/ipc/client.rs` | ~+70 (module doc rewrite, `read_budget` param + `maybe_timeout`, derivation fns + constants) + ~+120 tests (C1–C5) |
| `src/ipc/server.rs` | ~+25 (lane + dispatcher simplification) + ~+90 tests (S1/S2) |
| `src/commands/mod.rs` | ~+25 (`IpcTimeout` + kind + `verb_timeout_secs`) |
| `src/config.rs` | ~+35 (`IpcSection`) |
| `src/main.rs` | ~+65 (install decision, four select sites, exit-130 paths) |
| `tests/ipc.rs` | ~+80 (J1/J2) |
| `tests/cli_ipc.rs` | ~+60 (E1) |
| `tests/cli_tty.rs` | ~+60 (P1) |
| `tests/config.rs` | ~+25 (section decode/serialize pins) |
| `docs/adr/0012`, `CHANGELOG.md` | ~+35 |

**Estimated total: ≈ 720 additions / ≈ 40 deletions across 12 files —
size S/M**, matching the dispatch table. No migration, no
insta-shape change, no new dependencies, no (w) lane.

Six commits per §8's order; every commit standalone green.
