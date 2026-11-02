# [Plan] App hardening + contracts (#276 + #279 + #280) — chunk 3.6, backlog round 2 — stage-1 TDD implementation plan

- **Date**: 2026-10-31 (planning session executed 2026-11-02)
- **Kind**: stage-1 chunk plan for dispatch chunk **3.6** of backlog
  round 2 (`.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`,
  §"Chunk 3.6 — app hardening + contracts"). TDD-first; no production
  code in this commit.
- **Branch / worktree**: `chore/app-hardening-contracts` in
  `../taskboard-WT-chore-app-hardening-contracts` (off `origin/main` @
  `aa9d415b`; worktree-doctor passed 2026-11-02, 230 G free against the
  25 G floor). Note: local `main` was behind `origin/main` when the
  worktree was created; the branch was fast-forwarded to the fetched
  tip before this plan was written. Stage-2 must re-fetch and rebase
  before starting — a concurrent agent landed commits mid-session.
- **Normative origins**: GitHub issue **#279** (five bounds/latency
  findings, `Reliability`, `p3`), **#280** (two convention-only
  contracts, `Testing`/`Refactoring`, `p3`), **#276** (three app
  small cleanups, `DX`, `p3`); findings by F-number from
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-concurrency-report.md`
  (F9-02..F9-08, §"Proposed actions" and §8) and
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-app-report.md`
  (F8-04/F8-09/F8-10, §6 remediation table). The issues' "Proposed
  Approach" lines are normative for the *shape* of each fix; where the
  current tree has moved past the audit's line numbers, this plan
  re-anchors every claim to a verified site (§2).
- **Gates verified (2026-11-02)**: chunk 1.6 → PR **#567** "feat(app):
  the IPC disable knob" MERGED (closes #194); chunk 1.7 → PR **#565**
  "fix(app): tasks and labels edit demand at least one flag" MERGED
  (closes #407); chunk 1.10 → PR **#573** "test(app): the pty harness
  budget seam" MERGED. All three merge commits are ancestors of
  `origin/main` @ `aa9d415b`.
- **Verification status**: every seam fact in §2 was read against
  `aa9d415b` on 2026-11-02. Nothing below is derived from memory. The
  2026-10-17 audit's file/line anchors are stale in both directions —
  modules renamed (`session_ceremonies.rs` → `connector_ceremonies.rs`),
  findings moved (`SYSLOG` logger now `spawn_system_event_logger`),
  and three findings drifted *further* (new unbounded sites, a second
  forwarder, a deeper parser fork) — see the table.

## 1. Mission & binding constraints

Ten P3 findings, three issues, one branch. Five shutdown/lifecycle
bounds (#279), two convention-only contracts pinned as tests/compilers
(#280), three DX cleanups (#276). Nothing here adds user-visible
behavior: every fix either bounds a wait that is already expected to
terminate, converts a documented convention into a compiler- or
test-enforced fact, or folds duplicated grammar/UX shapes into one
home.

Binding constraints:

- **The audit's per-finding "proposed action" lines are the default
  shape.** Deviations are decided in §4 and each deviation names the
  audit alternative it took instead, with the reason.
- **AGENTS §3 protocols untouched**: no new locks, no orchestrator;
  the fixes ride the existing typed channels (`timeout` wrappers,
  `spawn_blocking`, broadcast `Lagged` handling).
- **AGENTS §5 discipline**: no text assertions (warning/guidance text
  is asserted by *presence*, never content, where asserted at all);
  no sleeps — clock-adjacent tests use `tokio::time::pause()` +
  `advance()` or pure sync seams; one behavior, one test, lowest
  layer; a new test failing is a bug signal, not a test signal.
- **AGENTS §2 boundaries**: all production edits land in
  `taskboard-app` and `taskboard-state`, plus one small pure helper in
  `taskboard-sync-nextcloud` (the helper's home is #276's own ruling —
  recorded decision D-F8-09). No domain, storage, or UI-crate changes.
  No new dependencies (`tokio::time::timeout`, `spawn_blocking`,
  broadcast, and the existing `url` crate cover everything).
- **Licensing/headers**: no new source files except the sync crate's
  helper home (a new module or an extension of `client.rs` — decided
  in §4/D-F8-09 to extend `client.rs`, no new file); every touched
  file already carries its AGPL-3.0-only or MIT/Apache-2.0 header.
  `#![forbid(unsafe_code)]` already the crate norm.
- **Quality gates**: full `scripts/run-quality-gates.sh` sequence
  before handoff; PR open + `gh pr checks` green is the handoff.

## 2. Re-verification table (verified 2026-11-02 against `aa9d415b`)

Legend: **fixed-already** (cite commit) / **still-drifted** (finding
present, site moved) / **drifted-further** (finding present AND new
surface area since 2026-10-17).

| Finding | Audit anchor (2026-10-17) | Current anchor (`aa9d415b`) | Verdict |
|---|---|---|---|
| **F9-02** unbounded `engine.flush()` ahead of every bounded join | `bootstrap.rs:348` | `bootstrap.rs:570` — `engine.flush().await` unbounded, still ahead of every bounded join. Doc comment `bootstrap.rs:530-533` now claims the sequence is "Bounded" — **inaccurate**: the flush and the storage join are the two unbounded awaits. | **drifted-further**: a *second* unbounded flush exists in the salvage tail `shutdown_from_arc` (`bootstrap.rs:1336`); and a bounded-flush precedent to copy now exists (`salvage_flush`, `bootstrap.rs:1352-1361`, `SALVAGE_FLUSH_BUDGET`) |
| **F9-03** storage join is the sequence's only unbounded await | `bootstrap.rs:381-382` | `bootstrap.rs:627` — `storage_join.await` bare, while all five sibling joins use `join_bounded` (`:592, :607, :619, :623` and `stop_actor`'s `:762`). Its budget fact is also **not aggregated** into `ShutdownOutcome.budget_hit` (`:631-637` lists authority/engine/escalation/notifier only). | **drifted-further**: a *second* bare storage join exists in the boot-unwind tail (`bootstrap.rs:1238`) |
| **F9-04** keyring + config re-read park the serial IPC dispatcher | `session_ceremonies.rs:209-235` | `connector_ceremonies.rs` (module renamed): `AppConfig::load` at `:255`, credential `store.load` via `deck_client_from_spec` at `:228-230` (called from `:309`), all inside `attach_from_persisted` (`:242-356`) awaited inline by the serial dispatcher at `ipc/server.rs:487`. Crate-wide `spawn_blocking` census: `bootstrap.rs:893` (notify-rust probe) only — none in the attach path. | **still-drifted** (renamed home) |
| **F9-05** UI forwarder abort discards ≤64 queued intents | `ui.rs:202-203` | `ui.rs:462-463` — `forwarder.abort(); request_forwarder.abort();` after `run(handle)` returns. | **drifted-further**: a *second* forwarder now exists — `ui_requests::spawn_ui_request_forwarder` (cap 16, `UI_REQUEST_CAPACITY` `ui_requests.rs:37`), whose channel-close path carries a debounced-width **flush** (`ui_requests.rs:85-89`) that the abort skips; and the command channel's queued `StateCommand`s are *mutations* the engine never sees |
| **F9-06** SYSLOG exits on first `Lagged` | `bootstrap.rs:664-667` | `bootstrap.rs:1253-1272` — now a named fn `spawn_system_event_logger`; the `Lagged` arm at `:1264-1267` warns then `return`s. Contrast still holds: escalation warns-and-continues (`escalation.rs:494`), ENG/SYN actors continue. | **drifted-further**: the new authority subscriber (`authority.rs:153-171`) *deliberately adopted* exit-on-`Lagged` and cites "the system-event logger's convention" in its doc (`:149-152`) — the fix now has a doc ripple (D-F9-06) |
| **F9-07** publish's swap-then-signal order is convention-only | `state/actor.rs:482-488` | `actor.rs:604-613` — `shared.store(...)` at `:610`, `signals.send(...)` at `:612`, ordering enforced by the doc comment at `:606-607` only. No test pins it: the engine suites count signals (`drain_signals`, `tests/engine.rs:239`, `tests/reminders.rs:270`) and match `StateUpdated` (`engine.rs:2366`) but never assert that a woken subscriber observes the swapped snapshot. | **still-drifted** (unchanged in substance; the surrounding loop restructured into a `Seams` struct mid-week — `publish` itself untouched) |
| **F9-08** `IpcOnly` match vs `route_for_ipc` dual enumeration | `main.rs:96-105` ↔ `commands/mod.rs:1228-1244` | `main.rs:126-138` — `match route_for_ipc(&command)` then a *second* match on the command inside the `IpcOnly` arm, ending in `unreachable!("the IpcOnly lanes are status and daemon stop")` at `:135`; ↔ `route_for_ipc` (`commands/mod.rs:2055-2093`) pins `Command::Status \| Command::Daemon { .. }` → `IpcOnly` at `:2070`. `DaemonCmd` has exactly one variant today (`cli.rs:220-224`, `Stop`), so the `unreachable!` is not live — the hazard is exactly the audit's: a second `DaemonCmd` variant lands in `route_for_ipc`'s wildcard and panics on first use. A *third* enumeration exists (`dispatch_line`'s session-only `unreachable!` arm, `commands/mod.rs:1600-1613`) but it partitions a **different** set (session-intercepted ceremonies/refusals, including `Boards`-not-List) — out of scope, recorded decision D-F9-08. | **still-drifted** (reshaped by the ADR 0012 routing table, same hazard) |
| **F8-04** daemon-gate bail names "Phase 7" | `main.rs:252-258` | Bail **reworded**: `main.rs:435-438` reads `app mode {mode:?} has no daemon-side frontend` with the comment at `:429-434` naming the real gap (introduced by `e8257a6e` "feat(app): the kiosk appliance lifecycle"). **Residue**: `bootstrap.rs:953-956` is now *factually wrong* twice (claims the daemon/UI entry refuses `Desktop`/`Kiosk` "until Phase 7 supplies those frontends" — Phase 7 shipped, and Kiosk **passes** the gate as the headless maintenance entry per `main.rs:431-434`); `bootstrap.rs:1250-1251` still cites "the Phase 7 UI banner is the real consumer". Historical phase names elsewhere (`lib.rs:6`, `ui.rs:2`, `config.rs:45/115/117`) are *history, not claims* — leave. | **fixed-already** (`e8257a6e`) **+ stale-comment residue** |
| **F8-09** three ad-hoc URL-authority parsers of one grammar | `config.rs:717-728`, `secrets.rs:194-205`, sync `DeckClient::new` | All three present: `reqwest_host_ok` (`config.rs:1411-1427`, fork-admitting comment at `:1405-1410`), `credential_key` (`secrets.rs:233-245`, same split-authority shape plus @/port stripping), `DeckClient::new` (`sync-nextcloud/client.rs:368-384`, `Url`-crate parse). | **drifted-further**: F12-02's userinfo rejection has since landed **independently at two of the three sites with different mechanisms** (config: structural `@` check `:1420-1425`; sync: `Url::username()/password()` check `client.rs:377-381`), and `credential_key` deliberately keeps a *different* policy (tolerate-and-strip, documented at `secrets.rs:228-232`) — the fork the audit warned about deepened |
| **F8-10** handler-side UX write outside the warning shapes | `session_ceremonies.rs:176-181` | `connector_ceremonies.rs:174-190` — `refresh_session_config`'s `writeln!(io.err, "the session's config view could not be refreshed; …")` at `:184-188`. Still the only such site: the shared shape is text-as-data (`connect::attach_nudge_warning` `connect.rs:637-649` consumed at `commands/mod.rs:1404-1406`; `logout::nudge_warning` at `:1438-1440`; login's deprecation note `:1355-1359`). | **still-drifted** (renamed home) |

**Count**: 1 fixed-already (F8-04, with residue), 5 still-drifted
(F9-04, F9-07, F9-08, F8-10 — plus F8-04's residue), 4
drifted-further (F9-02, F9-03, F9-05, F9-06, F8-09 — five by
finding, of which four gained surface area; F8-09 counted here).

## 3. Verified seam facts (do not re-derive; 2026-11-02 @ `aa9d415b`)

- **`EngineHandle::flush` is a FIFO barrier, not a storage roundtrip**
  (`state/actor.rs:165-176`): it sends `EngineEnvelope::Flush { reply }`
  and awaits the oneshot; the loop's `Flush` arm replies *immediately*
  (`actor.rs:432-438`) — "everything before this was applied,
  interpreted, and published". Therefore the flush wedges **only**
  when the engine is stuck inside an *earlier* envelope — i.e. an
  `Execute` parked in `repo.apply(...).await` (`actor.rs:678`, `:926`,
  `:1095`, `:1160`, `:1209`). The engine's repo port is the storage
  actor's handle ("the handle *is* the port adapter",
  `bootstrap.rs:1044-1046`), so a wedged flush ⇔ a wedged storage
  apply. This drives the F9-02 test design (§5/T1): a fenced-repo fake
  is the deterministic wedge.
- **`join_bounded`** (`bootstrap.rs:766-782`): awaits a
  `JoinHandle<()>` under `tokio::time::timeout`, aborts and reaps at
  the budget, returns `bool` (true = aborted). Adopted by all five
  sibling joins; `stop_actor` (`:756-764`) composes it with a
  `SyncCommand::Detach` send.
- **`ShutdownOutcome`** (`bootstrap.rs:641-660`): `{ actor_aborted,
  budget_hit }`, `degraded()` is the D-6 exit-code predicate in one
  place; `exit_if_degraded` accepts the server-drain budget fact as a
  separate parameter (doc `:651-656`). The storage join's fact (§4
  D-F9-03) must fold into `budget_hit` at `App::shutdown`, not extend
  the outcome type.
- **The signal ladder's flush is already bounded**: rung 2's
  `salvage_flush` (`bootstrap.rs:1352-1361`) wraps
  `engine.flush()` in `timeout(SALVAGE_FLUSH_BUDGET)`; the salvage
  tail (`shutdown_from_arc`, `:1330-1342`) does not. Only the owned
  `App::shutdown` and the salvage tail are in scope.
- **The forwarders** (`ui.rs:63-77` forwarder; `ui_requests.rs:77-140`
  request forwarder): both are `while let Some(x) = rx.recv().await`
  loops whose **own loop is a drain** — `recv` returns every queued
  item before `None`, and the request forwarder's `None` arm runs the
  pending-width flush (`ui_requests.rs:85-89`). The UI-side senders
  die with the Slint component when `run(handle)` returns. The aborts
  at `ui.rs:462-463` preempt exactly that self-drain. The in-crate
  test module (`ui.rs:471+`) already has the `FakeSink`
  (`:476-490`) and a clean-exit test (`:514-524`).
- **`EngineSignal`** (`domain/messages.rs:585`): effectively the
  single `StateUpdated` variant; `EngineHandle::subscribe` exists
  (`actor.rs:200-202`). `publish` (`actor.rs:604-613`) is a private
  sync fn called from six sites inside the engine loop
  (`:637`-equivalent sites listed at `:647, :702, :921, :935, :1107,
  :1170` pre-restructure numbering — stage-2 greps `publish(`).
  `actor.rs` has **no `#[cfg(test)]` module today** — this chunk adds
  one.
- **`route_for_ipc`** (`commands/mod.rs:2055-2093`) is pure and
  exhaustively unit-pinned (`:2248-2288+`, `the_routing_table_picks_lanes_per_command`);
  `IpcRoute` (`:2043-2051`) is a plain three-variant enum. `DaemonCmd`
  (`cli.rs:220-224`) has one variant (`Stop`). main's `IpcOnly` arm
  (`main.rs:126-138`) dispatches `Status` → `status_command`,
  `Daemon { cmd: Some(Stop) }` → `daemon_stop_command`, else
  `unreachable!`.
- **The serial dispatcher** (`ipc/server.rs:2-11` module doc): accept
  concurrently, execute **serially**; the control lane (S1) answers
  `Shutdown` outside the serial queue (`:790`). `attach_from_persisted`
  is awaited inline in the response path (`server.rs:487-497`).
  `CredentialStore` is `Arc<dyn CredentialStore>` shared into spawned
  tasks today (`ui.rs:405-417` passes it into the request forwarder's
  `ConnectionWorkspace`), so it is `Send + Sync` — `spawn_blocking`
  closure capture is sound without new bounds.
- **`spawn_system_event_logger`** (`bootstrap.rs:1253-1272`) logs via
  `tracing` only — a test cannot observe it without a seam (§5/T5's
  observer-sink extraction).
- **`TaskRepository`** (`domain/persistence.rs:29-46`): two methods
  (`load`, `apply`), both `BoxFuture` — a fenced fake is ~30 lines.
  `InMemoryRepository` in the state crate's test support is the shape
  template.
- **The warning-shape family**: warning text as pure data returning
  `Option<&'static str>`, written by one `writeln!` at the command
  layer (`connect.rs:637-649` + `commands/mod.rs:1404-1406`); the
  tests pin **presence/absence per state**, never content
  (`connect.rs:776-777`).
- **`reqwest_host_ok`** (`config.rs:1411-1427`): scheme
  (`http`/`https`) + non-empty authority + no `@` in the authority;
  its comment (`:1405-1410`) admits the fork and names the fail-closed
  drift policy. `credential_key` (`secrets.rs:233-245`): scheme
  stripped if present, authority cut at `/ ? #`, `@`/`:port` stripped,
  falls back to the trimmed input when the host is empty —
  "Hostless/garbage inputs still produce a stable key (pure function,
  never panics)" (`:228-232`), unit-pinned at `secrets.rs:613-640`.
- **`secrets.rs` greps as binary to some tools** (non-ASCII in a test
  literal at `:725`): use `grep -a` when re-deriving anything there.
- **ADR 0012** (`docs/adr/0012-daemon-ipc-local-socket-command-transport.org`)
  has the attach/nudge section the F9-04 latency note belongs in
  (budget table around `:278-288`).

## 4. Fix decisions (one per finding; every open question resolved)

### D-F9-02 — bound both flushes, warn-and-continue
Extract one private helper in `bootstrap.rs`:

```rust
/// One bounded attempt at the engine flush inside a graceful shutdown.
/// Returns false when the budget expired (the flush was abandoned —
/// per-command durability means the abandon loses nothing the bounded
/// joins below don't already surface as a degraded drain).
async fn flush_bounded(engine: &EngineHandle, timeout: Duration) -> bool
```

`App::shutdown` (`:569-570`) and the salvage tail (`shutdown_from_arc`,
`:1336`) both adopt it; expiry logs one `warn!` and continues (the
audit's argument holds verbatim: per-command durability means a
timed-out flush loses nothing the bounds below don't already handle).
`salvage_flush` (rung 2) is already bounded and stays as-is. Also fix
the `:530-533` doc comment to state the flush's budget honestly (the
sequence is bounded *including* the flush after this fix — reword to
say so). **Decision**: this is the audit's primary proposal; the
ADR-documentation alternative is not taken (a timeout is two lines
cheaper than an accepted-latency ADR paragraph for a path every
shutdown walks).

### D-F9-03 — `join_bounded` the storage joins; aggregate the fact
`App::shutdown` (`:625-629`): `drop(storage)` then
`join_bounded(storage_join, timeout, "storage actor")`, and **fold the
returned bool into `ShutdownOutcome.budget_hit`** (`:633-636`) — a
wedged storage actor is exactly as much a degraded drain as a wedged
notifier; not folding it would under-report `degraded()`. The
boot-unwind tail (`:1237-1240`): same `join_bounded` call; the fact is
logged only (that path returns `BootstrapError` — a failed boot has no
degraded-drain contract). **Decision**: no new outcome-type field; the
existing `budget_hit` is the aggregate (AGENTS: the predicate lives in
exactly one place).

### D-F9-04 — `spawn_blocking` the attach ceremony's blocking half, plus one ADR sentence
Wrap steps 1 and 3 of `attach_from_persisted` — the `AppConfig::load`
re-read (`:255`) and the credential load + client build
(`deck_client_from_spec`, `:309`) — in one
`tokio::task::spawn_blocking` block returning a typed enum
(`ConfigReloadFailed` / `CredentialUnavailable` / `Built(DeckClient)` /
`NotConfigured`), keeping the identity match (step 2, pure) and the
swap-first attach + settle (steps 4-6, async) on the dispatcher.
**Honest scope note** (recorded, not hidden): the dispatcher is serial
by design, so `spawn_blocking` does **not** un-queue the IPC surface —
an in-flight attach still delays the next request. What it buys: a
prompting/locked keyring (FFI that can block for minutes) no longer
occupies a runtime worker thread, so the S1 control lane, the accept
loop, and every other task keep full worker capacity — the difference
between "attach is slow" and "one worker is hostage". **Decision**: do
the `spawn_blocking` (S-effort, strictly better) **and** add one
sentence to ADR 0012's attach section recording the accepted
serial-queue latency (the audit's alternative, adopted for the half
the code cannot fix). Standalone kiosks never hit the path (ADR 0010).

### D-F9-05 — drain, not document: retire the forwarders by bounded join, demote abort to backstop
**Decision: drain.** The queued `StateCommand`s are user intents
(mutations); silently discarding ≤64 at quit is a loss the UI contract
should not have, and the request forwarder's abort additionally skips
the debounced-width flush its own close path would land. The
mechanism needs **no new channel**: both forwarders' `while let Some(x)
= rx.recv().await` loops *are* a drain — `recv` yields every queued
item before `None`, and the UI-side senders drop with the Slint
component when `run(handle)` returns. Replace both `abort()` calls
(`ui.rs:462-463`) with a new `pub(crate)` helper:

```rust
/// Retires a forwarder after the UI event loop is gone: awaits its
/// natural close-drain under a budget, aborting only a wedged task.
/// Returns whether the join had to abort (≤capacity intents lost).
async fn retire_forwarder(join: JoinHandle<()>, timeout: Duration, what: &str) -> bool
```

(composed of `join_bounded` — same semantics as every actor join).
Each retire's budget fact logs a `warn!` and, if the GUI tail's
degraded plumbing takes it cheaply, folds into the exit decision
alongside `outcome.degraded()`; if not cheap, warn-only is accepted —
**recorded decision**: warn-only is acceptable, do not widen
`exit_if_degraded`'s signature for it. The residual loss window (a
leaked UI-side sender clone pinning the channel → budget abort → ≤64
intents) is now *bounded and signaled* instead of silent — that is the
documented half of "drain-or-document", achieved inside the drain.
Fallback shape if the leaked-sender case shows up in practice (a quit
always paying the full budget): a drain-signal `watch` per forwarder
(recorded here, not implemented). The request forwarder's natural
close path (`ui_requests.rs:85-89`) lands the width flush — the abort
skipped it; this fix restores it. **Decision vs the audit's letter**:
the audit sketched an external `try_recv` drain *before* the abort;
that requires the receiver outside the task and cannot reproduce the
request forwarder's debounce-merge — the natural-close drain is the
same outcome without the restructure.

### D-F9-06 — the logger continues; the authority keeps its exit; the doc ripple is fixed
`spawn_system_event_logger`'s `Lagged` arm (`:1264-1267`): warn and
**continue** (the loop's next `recv` resumes from the oldest retained
event) — the audit's proposal verbatim; a logger losing the rest of
uptime's observability is strictly worse than a resumed stream with a
noted gap. **Ripple (drifted-further's cost)**: `authority.rs:149-152`
cites "the system-event logger's convention" as part of its
exit-on-`Lagged` rationale. The authority's exit stands — its reason
is real and different (a missed 64-event window makes the *counted*
up/down set stale; it is a state consumer, not a logger) — but the doc
must say so on its own authority: reword `:149-152` to drop the
logger cross-reference and keep the staleness argument. While in the
file, fix the logger's stale doc line `:1250-1251` ("the Phase 7 UI
banner is the real consumer" → name the actual consumer: the UI
crate's system subscription mounted per lifecycle).

### D-F9-07 — pin the publish order with a spy sink, in a new `actor.rs` test module
The contract (`actor.rs:606-607` doc): *"swap first, signal after, so
a waking UI reads fresh state"*. A waking subscriber observes the
swapped snapshot ⇔ **at the moment the signal is sent, `shared.load()`
already returns the new state**. On any runtime, intra-call ordering
of two sync effects is unobservable from outside — a subscriber-side
assert (recv, then load) passes under *both* orders on a
`current_thread` runtime (the engine task runs publish to completion
before yielding) and is a race on `multi_thread`. **Decision: the
observation point moves inside the send.** Make `publish` generic over
its signal sink:

```rust
pub(crate) trait SignalSink { fn signal_updated(&self); }
impl SignalSink for broadcast::Sender<EngineSignal> {
    fn signal_updated(&self) { let _ = self.send(EngineSignal::StateUpdated); }
}
fn publish<S: SignalSink>(core: &EngineCore, shared: &ArcSwap<AppState>,
                          sink: &S, metrics: &EngineMetrics) { ... }
```

Production call sites pass the existing `&broadcast::Sender` unchanged
(monomorphized, zero cost). The new `#[cfg(test)] mod tests` in
`actor.rs` drives `publish` with a **spy sink** whose
`signal_updated()` loads `shared` and records whether the fabricated
effect is visible: correct order ⇒ visible; flipped order ⇒ the spy
runs *instead of* the send and the store has not happened ⇒ not
visible ⇒ assert fails — **deterministically, single-threaded, no
clock, no sleeps**. The fabricated core carries one marker (a task
with a known title over an in-memory repo fixture); the spy asserts
the marker is in `shared.load()`. **Load-bearing proof (mandatory,
dispatch acceptance)**: with the test green, temporarily reorder
`publish`'s body (send before store), re-run — the test must fail;
restore the order. Also update the doc comment to name the test as the
pin. Rejected alternatives (recorded): a subscriber-side integration
test (not load-bearing per the argument above); an async `publish`
(reshapes six call sites for no contract gain); an instrumented
`ArcSwap` wrapper (API churn in `EngineCore`).

### D-F9-08 — collapse the dual enumeration: the lane carries its verb
Make the `IpcOnly` lane **data-carrying** (the house A1 instinct: make
the drift a compile error, not a runtime panic):

```rust
pub enum IpcOnlyVerb { Status, DaemonStop }
pub enum IpcRoute { Local, IpcOrLocal, IpcOnly(IpcOnlyVerb) }
```

`route_for_ipc` arms become: `Command::Status => IpcOnly(Status)`;
`Command::Daemon { cmd: Some(DaemonCmd::Stop), .. } => IpcOnly(DaemonStop)`;
`Command::Daemon { cmd: None, .. } => Local` — **with a comment**: the
run arm is intercepted in main before routing (`daemon_run`,
`main.rs:103`); pinned `Local` keeps it off the wire, the same
convention as `Gui` (`:2061-2066`). main.rs's `IpcOnly` arm
(`:127-138`) then dispatches on the verb — `match verb { Status => …,
DaemonStop => … }` — and the `unreachable!` is deleted: a future
`DaemonCmd` variant must (a) pick a lane in `route_for_ipc`'s
exhaustive match — naming a new `IpcOnlyVerb` variant if `IpcOnly` —
and (b) be handled in main's verb match, or the binary does not
compile. **The compiler is the pin**; the existing routing-table test
updates its `IpcOnly` rows to assert the verb data (`assert_eq!(route_for_ipc(&Command::Status), IpcRoute::IpcOnly(IpcOnlyVerb::Status))`).
**Scope guard**: `dispatch_line`'s session-only `unreachable!` arm
(`commands/mod.rs:1600-1613`) partitions a *different* set
(session-intercepted ceremonies + refusals, six commands, including
`Config`/`Daemon`/`Gui` and excluding `Boards`-List which is
route-`Local` but inner-dispatchable). It is not the audit's finding;
leave it. **Decision**: the audit's "have `route_for_ipc` return the
lane *with* the matched command" option is taken over "dispatch the
IpcOnly arms inside it" (the latter would pull `status_command` /
`daemon_stop_command` behind the commands/mod table — a layering
inversion for no gain).

### D-F8-04 — bail is fixed-already; correct the two wrong comments
No code change at the gate (`main.rs:435-438` is correct; cite
`e8257a6e` in the commit message as the fixed-already evidence). Fix
the residue: `bootstrap.rs:953-956` reworded to the truth — the core
is mode-agnostic; main's daemon gate refuses only `Desktop` (no
daemon-served desktop frontend; `Kiosk` passes as the headless
maintenance entry, ADR 0015/0016) and the GUI is the desktop frontend.
`bootstrap.rs:1250-1251` reworded per D-F9-06's ripple. Comment-only
edits; no tests.

### D-F8-09 — one `authority_of` in the sync crate; both hand-rolled consumers adopt it; `DeckClient::new` stays
**Home: `taskboard-sync-nextcloud`, per #276's own ruling** — the
grammar describes a Nextcloud server URL, the sync crate already owns
`DeckClient::new`'s URL rules, and `taskboard-app` already depends on
the crate. Home *within* the crate: `client.rs`, beside
`DeckClient::new` (no new file — the helper is 8 lines; a new module
would out-rank its content). Signature — **recorded deviation from
the issue's sketch** (`pub fn authority_of(url: &str) -> &str`):
config validation also owns *scheme* validation (`http`/`https`,
`config.rs:1416-1418`), so one helper returning both halves is the
collapse the finding wants:

```rust
/// The (scheme, authority) of a URL-ish string: the lenient,
/// structural split both consumers share — scheme `None` when the
/// input has no `://` (garbage-tolerant, never panics); the authority
/// is everything after the scheme up to the first `/`, `?`, or `#`.
/// Policy (userinfo rejection, port stripping, scheme allow-lists)
/// lives with the consumers, not here.
#[must_use]
pub fn authority_of(raw: &str) -> (Option<&str>, &str)
```

Consumers: `reqwest_host_ok` (`config.rs:1411`) checks the scheme
against `http|https` and applies its no-`@`, non-empty policy to the
authority; `credential_key` (`secrets.rs:233`) ignores the scheme and
keeps its @/port-strip + trimmed-fallback policy *local* (that is
credential-key semantics, not URL grammar — its garbage-tolerance doc
`secrets.rs:228-232` and tests `:613-640` stay green unchanged, which
is the behavior-preservation proof). The fork-admitting comment at
`config.rs:1405-1410` is rewritten to name the shared helper.
**Decision**: `DeckClient::new` (`client.rs:368-384`) does **not**
adopt the helper — it parses and *normalizes* a full URL via the
`url` crate (paths, query, fragment); the issue names only the two
hand-rolled consumers. The three parsers of "one grammar" reduce to
one grammar helper + one `url`-crate parser with disjoint jobs.

### D-F8-10 — the guidance becomes data beside the nudge warnings
`refresh_session_config` (`connector_ceremonies.rs:174-190`): the
inline string literal moves into a named `pub(crate)` fn beside the
ceremony family,

```rust
/// The stderr UX for a degenerate session-config refresh (the view is
/// stale until a relaunch) — text-as-data per the nudge-warning shape
/// (`connect::attach_nudge_warning`); written by the ceremony site,
/// never by a handler buried below the command layer.
pub(crate) fn refresh_failure_guidance() -> &'static str
```

and the single `writeln!` references it. Output is byte-identical;
this is a shape-consistency fold (the audit's objection is the
*location* of the string, not the write). **Decision: no new test** —
no behavior changes, the error path is already pinned by the repl
suite's connect/config journeys, and AGENTS' "one behavior, one test"
forbids a duplicate. (The nudge family's `Option` shape is *not*
adopted here: the guidance is unconditional on this error path;
`Option` would add a false sense of selectivity.)

### Open questions the dispatch prompt named — all resolved above
- **Publish-order test design**: §D-F9-07 (spy sink; deterministically
  fails on flip; pause/advance unnecessary — the seam is pure sync).
- **Enumeration collapse shape**: §D-F9-08 (data-carrying lane; one
  match drives both sites; `Daemon { cmd: None }` pinned `Local`).
- **`authority_of` home**: §D-F8-09 (sync crate per #276; both
  consumers adopt; `DeckClient::new` stays on `url`).
- **Forwarder drain-or-document**: §D-F9-05 (drain via natural close +
  bounded join; residual abort loss bounded and signaled; watch-drain
  fallback recorded).

## 5. TDD execution order (test first at every step; each step green before the next)

Commit cadence: one `<type>(<module>): <desc>` commit per step group,
each standing alone (rebase-merge hygiene, AGENTS §8). Suggested:
`test(state): …`, `refactor(app): …`, `fix(app): …`, `refactor(sync): …`.

- **T1 — F9-07 publish-order pin (state crate, lowest layer, do
  first: it is the chunk's contract test).** Add `#[cfg(test)] mod
  tests` to `actor.rs`; introduce `SignalSink` + generic `publish`;
  write the spy-sink test (§D-F9-07). Prove load-bearing by the
  scratch flip (send-before-store ⇒ red), restore, commit green.
  No clock involvement — pure sync seam (AGENTS determinism rule
  satisfied by construction).
- **T2 — F9-08 enumeration collapse (compile-error-first).** Change
  `IpcRoute`/`route_for_ipc` (§D-F9-08); let the compiler enumerate
  every broken site (route arms, routing-table test rows, main's
  arm); update the routing-table test to pin verb data; delete
  main's `unreachable!`. Run the routing tests + `cargo nextest run
  -p taskboard-app` dispatch/routing suites (`tests/dispatch_routing.rs`).
- **T3 — F9-02 flush bound (app crate, unit level).** Write the test
  first: a `FencedRepository` fake (`apply` parks on a
  test-owned oneshot; `load` returns a seeded state — the
  `TaskRepository` two-method shape, `domain/persistence.rs:29-46`)
  feeding `spawn_state_engine` (public, the bootstrap path);
  execute one mutating command (the engine parks inside
  `repo.apply`); wrap `flush_bounded(&engine, budget)` under
  `tokio::time::pause()` + `advance(budget)` — assert `false` (missed)
  deterministically; release the fence, call again — assert `true`.
  The test carries an outer `tokio::time::timeout` safety net so a
  scratch-removal of the inner bound fails instead of hanging.
  Then implement `flush_bounded` and adopt at `:570` + `:1336` (§D-F9-02).
- **T4 — F9-03 storage joins (mechanical; existing tests are the
  pin).** Adopt `join_bounded` at `:627` (fold into `budget_hit`) and
  `:1238` (warn-only) per §D-F9-03. No new test: the wedge arm is the
  already-covered `join_bounded` machinery (B4 pins a bounded
  degraded shutdown end-to-end, `tests/bootstrap.rs:300-343`; B3/B13
  pin the healthy path). A dedicated storage-wedge test would need a
  new bootstrap repo seam — **recorded as out of scope** (consistency
  fix per the audit's own words); if wanted later, it is a follow-up
  issue, not this chunk.
- **T5 — F9-06 logger Lagged continuation (genuine red→green).**
  Extract the loop body to take an observer sink
  (`fn log_system_events(events: &mut broadcast::Receiver<SystemEvent>,
  observe: &mut impl FnMut(SystemEvent))`; production passes a
  tracing-logging closure). Test: capacity-1 broadcast; send e1 (recv
  it), send e2, send e3 → the next recv returns `Lagged(1)`; drive the
  loop; assert e3 **arrives on the observer** (the loop continued) —
  red against today's early-return, green after the fix. No text
  assertions (the observer receives typed `SystemEvent`s). Fix the
  arm (`:1264-1267`), reword the authority doc (`authority.rs:149-152`)
  and the logger doc (`bootstrap.rs:1250-1251`) per §D-F9-06.
- **T6 — F9-05 forwarder retirement (app crate, unit level).**
  Extract `retire_forwarder` (§D-F9-05); test in `ui.rs`'s existing
  test module using the `FakeSink`: spawn the real
  `spawn_ui_command_forwarder`, queue several commands, drop the
  sender, `retire_forwarder(handle, budget, "ui command forwarder")`
  — assert every command reached the sink in order and the join did
  **not** abort (the forwarder's own close-drain is the drain). Then
  switch `ui.rs:462-463` to the helper (budget =
  `shutdown_timeout_secs`), warn-and-fold-or-warn-only per §D-F9-05.
  The request forwarder gets the identical switch; its width-flush on
  natural close is already covered by `ui_settings.rs`/`ui_requests`
  tests — if not, one test: queue a width change, drop the sender,
  retire, assert the config file in the tempdir gained the width
  (structural TOML assertion, never text-match prose).
- **T7 — F9-04 attach spawn_blocking (red→green via the asserting
  fake).** Test through `attach_from_persisted`'s existing surface
  (the ipc attach test path in `tests/ipc.rs`, or a direct ceremony
  call if one exists there): a fake `CredentialStore` whose `load`
  asserts `tokio::task::try_current().is_none()` — inside
  `spawn_blocking` the closure runs outside any async task context;
  against today's inline call the assert fires ⇒ red. Implement the
  spawn_blocking block (§D-F9-04) ⇒ green. Add the one ADR 0012
  sentence (serial-queue latency accepted, kiosks unaffected).
- **T8 — F8-09 `authority_of` (helper-first).** Unit tests for
  `authority_of` in `client.rs`'s test module (scheme present/absent,
  authority cut at `/`, `?`, `#`, empty authority, no-panic garbage —
  mirroring `credential_key`'s garbage table). Implement; adopt the
  two consumers (§D-F8-09); run the consumers' existing tests
  unchanged (`secrets.rs:613-640`, config's `reqwest_host_ok` pins) as
  the behavior-preservation proof. Rewrite the fork-admitting comment.
- **T9 — F8-04 residue + F8-10 fold (comment/shape, no tests).**
  `bootstrap.rs:953-956` + `:1250-1251` rewording (§D-F8-04);
  `refresh_failure_guidance` extraction (§D-F8-10). Verify the repl
  connect/config journeys still green.
- **T10 — gates & handoff.** `cargo fmt --all -- --check`; clippy
  `-D warnings`; `cargo nextest run --workspace`; doc tests;
  `cargo deny check`; the coverage trio via
  `scripts/run-quality-gates.sh` (the shared flock wrapper — the box
  runs parallel agents). Push the branch, open the PR targeting
  `main` (nothing stacked behind it — no rebase dance needed), hand
  off at `gh pr checks` green. Cite in the PR body: #276, #279, #280,
  the fixed-already evidence (`e8257a6e`), and this plan.

**Non-goals (explicit)**: no runtime-flavor changes; no new
`ShutdownOutcome` fields; no dispatcher concurrency (serial is the
design, ADR 0012); no `dispatch_line` session-partition changes; no
`DeckClient::new` adoption of `authority_of`; no watch-drain
forwarders (fallback only); no flaky-test triage here (AGENTS §7 —
separate pass); no version bump.

## 6. Risks & mitigations

- **`actor.rs` churns fast** (the `Seams` struct restructure landed
  *during* this planning session; `publish` survived it unchanged but
  moved 594→604). Mitigation: stage-2 rebases onto a fresh fetch
  first; the spy-seam diff is small and mechanical (six call sites,
  type inference only). If `publish` moved again, the grep anchor is
  `fn publish(`.
- **The GUI tail's forwarder joins could regress quit latency** if a
  Slint-side sender leaks (every quit pays the full budget). Mitigation:
  budget = the same `shutdown_timeout_secs` the shutdown sequence
  already spends; the watch-drain fallback is specified (§D-F9-05) and
  is a one-commit swap if it bites.
- **F9-08 touches `main.rs`'s daemon arms** — the same block chunk
  1.6 just landed (`--no-ipc` override, `daemon_run` intercept).
  Nothing is stacked behind 1.6 (it merged), so no rebase dance — but
  stage-2 must keep the `daemon_run`/`daemon_no_ipc` intercepts
  exactly intact (they are gate-tested by `tests/cli_ipc.rs`).
- **The fenced-repo fake must match `TaskRepository`'s `BoxFuture`
  signatures** (`domain/persistence.rs:37-45`) — small but fiddly;
  copy the shape from the state crate's `InMemoryRepository` test
  support. The engine must be *executed* a mutating command before
  `flush` for the fence to bite (flush alone never touches the repo —
  §3 fact).
- **`secrets.rs` reads as binary to `grep`/Read in some tools**
  (non-ASCII test literal `:725`) — stage-2 uses `grep -a` /
  `sed` there; edits are unaffected.
- **`try_current()` as the spawn_blocking proof** asserts a tokio
  implementation detail (blocking-pool closures run outside the task
  context). It is stable, documented behavior (`tokio::task::try_current`
  returns `None` on the blocking pool); if tokio ever changes it, the
  test fails loudly rather than passing vacuously — acceptable.
- **Concurrent agents**: another chunk landed on `origin/main`
  mid-planning (`4d8be313..aa9d415b`); the seams were re-verified at
  the tip. Stage-2 repeats the re-verification table's spot checks
  (§2's "current anchor" column) before editing, and re-runs the
  doctor before recreating the worktree.

## 7. Acceptance checklist (dispatch plan's bar, itemized)

- [ ] Every §2 finding fixed or annotated (F8-04: residue comments
      fixed; the bail's fixed-already state cited in the PR).
- [ ] The publish-order test exists, is green, and its load-bearing
      proof (scratch flip ⇒ red) is recorded in the PR description.
- [ ] The dual enumeration is collapsed: `unreachable!` deleted from
      main.rs's `IpcOnly` arm; the lane carries the verb; the
      routing-table test pins verb data.
- [ ] Shutdown bounds pinned: flush budget test under pause/advance;
      storage join folded into `budget_hit`; B3/B4/B13 green.
- [ ] The forwarder retire helper's drain test green (intents land,
      width flush lands, no abort in the clean case).
- [ ] The logger's Lagged continuation test green (red documented
      against the old exit).
- [ ] `authority_of` unit-tested; both consumers adopted; consumer
      tests green unchanged.
- [ ] ADR 0012 carries the accepted-latency sentence.
- [ ] No text assertions added anywhere (grep the diff for `assert!
      (` against string literals — structural/typed pins only).
- [ ] Full quality gates green through `scripts/run-quality-gates.sh`;
      PR open, `gh pr checks` green; worktree removed after handoff
      (AGENTS §8 lifecycle).
