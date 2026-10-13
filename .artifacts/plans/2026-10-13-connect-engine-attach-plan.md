# [Plan] Connector Attach — Engine Topology + Attach Primitive (WI-J.1)

- **Date**: 2026-10-13
- **Kind**: stage-1 implementation plan (chunk 1 of 3), TDD-first. No
  production code in this commit.
- **Origin**: approved design round
  `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` (§4.2
  normative; §1 facts 4/6/14/15 load-bearing; §4.5 "fixed now" bounds
  the scope; §12 chunk WI-J.1), dispatched by
  `.artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md`
  §1/§2 (prompt 1.1).
- **Target**: `taskboard-state` (`src/actor.rs`, one `lib.rs` re-export)
  and `taskboard-app` (`src/bootstrap.rs`), plus their test suites.
  **No domain diff, no sync-crate diff** (design §11: WI-H already
  merged everything they need).
- **Branch / worktree**: `feat/connect-engine-attach`,
  `../taskboard-WT-feat-connect-engine-attach`, off `main` @ `09cabf4`.
- **Goal**: the whole multi-connector-ready topology of design §4.2 —
  the engine's additive nudge registry (fan-out + lazy prune), the
  `AttachConnector` envelope variant with its FIFO ack, `App`'s
  `Mutex<Vec<SyncActor>>` widening, the permanent fan-in reports inbox
  with its sender parked in `App`, the shutdown
  drop-halves-before-engine-join step, and `App::attach_connector` as
  the merged `detach_connector`'s mirror (typed `AlreadyAttached` guard
  = the v1 app-layer count-≤-1 invariant; re-attach after detach
  permitted).
- **Status**: READY for stage 2 (every open question resolved in §6).

---

## 1. Context & verified facts (anchored; do not re-derive)

All anchors verified on `main` @ `09cabf4` (the worktree base). The
reminder-round commits since `bdd9d97` did not touch any file listed
here; the design round's anchors for these files still read true.

1. **The engine's nudge seam today is one sender.**
   `spawn_state_engine` takes `sync_out: mpsc::Sender<SyncCommand>`
   (`crates/taskboard-state/src/actor.rs:184`), parks it in
   `EngineParts` (`:234`), and `run_loop` destructures it into a
   loop-local (`:255`) re-borrowed per iteration through the `Seams`
   tuple (`:387-394`, `&'a mpsc::Sender<SyncCommand>`).
   `forward_sync_now` (`:479-496`) `try_send`s `SyncNow`: `Ok` and
   `Full` count as forwarded; only `Closed` means "no connector". It is
   called from three sites: the `RequestSync` fast path (`:409`), the
   post-apply nudge (`:468`), and the `NetworkRestored` arm (`:358`).
2. **Nudge semantics key on channel liveness** (design fact 4): a
   successful forward marks the memory-only `Syncing` transient; a
   merely *parked-open* command receiver would flip every standalone
   mutation into a perpetual `Syncing` — the reason option A (parked
   command halves) was rejected and why the registry's standalone entry
   must be *dead* (receiver dropped at boot), never parked-open.
3. **The engine loop can grow a command arm in place** (design fact 6):
   `EngineCommand` (`:41-67`) is the engine crate's channel-bearing
   envelope type; `Execute`/`Flush`/`ReadState` carry oneshot replies;
   the arms at `:283-313` are inline match arms; `Flush`'s empty body
   is the FIFO durability precedent (`:304-310`).
4. **The linger-drain gates on the reports channel closing** (design
   fact 14): the loop returns only when `draining && !reports_open`
   (`:272-274`); `reports_open` flips to `false` only on
   `recv() → None` (`:328-339`) — i.e. when *every* reports sender has
   dropped. Today that is guaranteed by construction: the only sender
   lives in the sync actor's task and dies with it before the engine
   join (`App::shutdown` joins the actor at `bootstrap.rs:251-255`,
   then the engine at `:256`). **Any design that parks a reports
   sender inside `App` must drop it before awaiting the engine join.**
5. **`EngineHandle` wraps request-response over the command inbox**
   (`execute`/`flush`, `:89-134`): send, map a send failure to the
   typed gone-error, await the oneshot. An attach ack rides the same
   pattern (design fact 7).
6. **`App`'s connector slot is interior-mutable since WI-H**
   (`sync: std::sync::Mutex<Option<SyncActor>>`,
   `bootstrap.rs:141`; `SyncActor` = `{commands, join}` at `:120-123`).
   `sync_commands()` (`:169-177`) clones the actor's sender or answers
   a fail-closed fresh pair; `connector_attached()` (`:189-191`);
   `detach_connector` (`:207-225`) takes the actor out, sends
   `SyncCommand::Detach`, joins bounded by `shutdown_timeout_secs`
   (default 10 s, `config.rs:42`), outcome `Detached | NoConnector |
   TimedOut`. The mutex is never held across `.await`.
7. **Bootstrap's channel graph** (`bootstrap.rs:388-390`): one system
   broadcast, one sync-command pair, one reports pair. The engine
   spawn takes `sync_commands.clone()` + `report_rx` (`:395-403`). The
   nextcloud arm moves `sync_rx` and `reports` into
   `spawn_sync_actor` (`:418-430`); the standalone arm drops both
   (`drop(sync_rx); drop(reports)`, `:431-438`) — "the engine sees the
   closed-channel world it already handles". The reports-sender drop
   is the exact line this chunk removes (parked instead).
8. **`spawn_sync_actor` already has the signature attach needs**
   (`taskboard-sync-nextcloud/src/actor.rs:114-123`): it receives its
   reports sender by value — a `clone()` of a parked sender is the same
   call. The `Detach` arm (`:201-207`) and the post-cycle nudge
   absorber's `Detach` (`:253-258`) are WI-H's, reused as-is. A
   freshly spawned actor self-targets from the persisted binding and
   fires its first cycle immediately (design fact 5) — attach needs no
   `SetBoard` ceremony.
9. **WI-H suites that must pass unchanged**: engine
   `h1_engine_continues_without_sync_after_the_reports_channel_closes`
   + A/G-series (`crates/taskboard-state/tests/engine.rs`); app B1–B9
   (`crates/taskboard-app/tests/bootstrap.rs`), including B8 (the
   standalone shape: `RequestSync → NoOp`, phase `Idle`, clean
   shutdown) and B9 (detach: graceful, idempotent, fail-closed nudge).
   The `sync_commands()` signature is load-bearing for
   `commands/boards.rs:277` and the IPC/daemon surfaces — it is
   preserved verbatim.
10. **The engine test harness** (`engine.rs:116-166`) owns all channel
    halves and passes a single `sync_out` into
    `spawn_state_engine`; `eventually`/`wait_draining` poll under
    timeout guards. The suite header pins Miri cleanliness — new
    engine tests mirror the existing helpers only.

## 2. Scope

**In** (design §4.5 "fixed now", engine + app halves):

- `taskboard-state`: nudge registry (`Vec<mpsc::Sender<SyncCommand>>`)
  with fan-out and lazy pruning; `EngineCommand::AttachConnector`
  (push, never replace) + `AttachReceipt` + handle method.
- `taskboard-app`: `connectors: Mutex<Vec<SyncActor>>` widening; the
  permanent reports inbox (pair created once at boot, sender parked in
  `App`, cloned per attach); the shutdown
  drop-connector-halves-before-engine-join step;
  `App::attach_connector(&self, client) -> Result<(), AttachError>`;
  `sync_commands()`/`connector_attached()`/`detach_connector()`/
  `shutdown()` carried over the new shape.
- Tests: engine j-series + app B10–B14 (§5); pre-existing suites
  untouched.

**Out** (chunk 2/3 or the future round): any CLI/REPL/IP C surface for
attach; the ceremony; `wait_for_boot_pull` wiring after attach (the
caller's job, design §4.4); ADR 0014 + docs + backlog ✅ (chunk 3's
final commits); daemon-side attach verb; multi-connector lift
(`SetBoard` routing, per-connector status, config shape — design §4.5
deferred list); any change to `taskboard-domain` or
`taskboard-sync-nextcloud`.

## 3. Engine side — `taskboard-state/src/actor.rs`

### 3.1 The registry and the fan-out/prune seam

- `EngineParts.sync_out` keeps its name and type; `run_loop`
  destructures it as today and wraps it once:
  `let mut sync_outs = vec![sync_out];` — **`spawn_state_engine`'s
  signature is unchanged** (D1), so the boot path (and every existing
  test harness) is byte-identical. Under `standalone` the loop's
  initial entry is dead by construction (bootstrap still drops the
  receiver, fact 7).
- `Seams` becomes
  `(&Arc<dyn TaskRepository>, &dyn IdGenerator, &dyn Clock, &mut Vec<mpsc::Sender<SyncCommand>>, &broadcast::Sender<SystemEvent>)`
  (doc comment updated). Three call sites reborrow the same loop-local;
  sequential arms, no borrow conflict.
- `forward_sync_now` is rewritten as the fan-out **with the prune
  fused in** via `Vec::retain`:

```rust
/// Fire-and-forget nudge, fanned out to every registered connector.
/// Forwarded iff any send lands (`Ok`, or `Full` — a full inbox means
/// a cycle is already pending); `Closed` entries are pruned right
/// here, one mechanism covering both dead paths: a standalone boot's
/// dropped receiver and a detached actor's. A dead connector is a
/// `debug!` event — no connector exists in engine-only tests, and the
/// Phase 4 actor owns dedupe.
fn forward_sync_now(sync_outs: &mut Vec<mpsc::Sender<SyncCommand>>) -> bool {
    let mut forwarded = false;
    sync_outs.retain(|out| match out.try_send(SyncCommand::SyncNow) {
        Ok(()) => {
            tracing::debug!("sync nudge forwarded");
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            tracing::debug!("sync nudge coalesced: channel full");
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            tracing::debug!("sync nudge dropped: no sync actor");
            false
        }
    });
    forwarded
}
```

  With ≤ 1 live connector this is observably identical to today's
  single `try_send` (same outcomes, same log events, same
  forwarded/`Syncing` semantics); with N it is the "someone will sync
  this" rule; on an empty registry (post-prune standalone) it answers
  `false` without sending.

### 3.2 `EngineCommand::AttachConnector` — shape and ack semantics

```rust
/// Registers a connector's command sender with the nudge registry:
/// push, never replace (the engine is connector-count-agnostic; the
/// app layer owns the at-most-one guard). The reply settles after the
/// push — FIFO over the sequential loop makes the ack the proof that
/// every later nudge fans out to this sender (the `Flush` durability
/// story).
AttachConnector {
    /// The connector's `SyncCommand` inbox sender.
    sync_out: mpsc::Sender<SyncCommand>,
    /// Ack; see [`AttachReceipt`].
    reply: oneshot::Sender<AttachReceipt>,
},
```

```rust
/// The engine's answer to a connector attach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachReceipt {
    /// The sender is on the nudge registry.
    Attached,
    /// Declined: the engine is draining (post-`Shutdown`); the sender
    /// was not registered.
    Draining,
}
```

(D2 records why the ack payload is a bare receipt rather than a
`Result`, and why the handle-level transport failure is a dedicated
one-variant `AttachError::EngineGone` — the same event class
`ExecuteError::EngineGone` models for `execute`, kept separate because
attach is not a `StateCommand`.)

The loop arm, inline beside `Flush`/`ReadState` (commands arm,
`actor.rs:304-313`):

```rust
Some(EngineCommand::AttachConnector { sync_out, reply }) => {
    if draining {
        // Mirrors Execute's decline: no new work — and no new
        // connectors — after Shutdown.
        let _ = reply.send(AttachReceipt::Draining);
    } else {
        sync_outs.push(sync_out);
        let _ = reply.send(AttachReceipt::Attached);
    }
}
```

Push-then-reply inside one sequential arm **is** the FIFO ack: the
next message the loop services already sees the registry entry, and a
reply received by the caller implies the push happened.

### 3.3 The handle method

```rust
/// Registers a connector's command sender with the engine's nudge
/// registry (push, never replace). The receipt settles after the push
/// (FIFO): once it arrives, every later nudge fans out to this sender.
///
/// # Errors
///
/// [`AttachError::EngineGone`] when the engine loop is stopped (or
/// died before the ack).
pub async fn attach_connector(
    &self,
    sync_out: mpsc::Sender<SyncCommand>,
) -> Result<AttachReceipt, AttachError> {
    let (reply, rx) = oneshot::channel();
    self.commands
        .send(EngineCommand::AttachConnector { sync_out, reply })
        .await
        .map_err(|_| AttachError::EngineGone)?;
    rx.await.map_err(|_| AttachError::EngineGone)
}
```

`lib.rs` re-exports grow:
`pub use actor::{AttachError, AttachReceipt, EngineCommand, EngineHandle, spawn_state_engine};`.

The engine gains **no connector knowledge**: no ids, no counts, no
policy — only the channel topology (the SRP line the review mandate
draws).

## 4. App side — `crates/taskboard-app/src/bootstrap.rs`

### 4.1 The registry widening

- `App.sync: Mutex<Option<SyncActor>>` →
  `App.connectors: Mutex<Vec<SyncActor>>` (field doc rewritten: the
  v1 invariant is length ≤ 1, enforced by `attach_connector`'s guard
  and by bootstrap — zero-or-one at boot; the mutex discipline carries
  over verbatim: lifecycle bookkeeping only, never held across
  `.await`).
- `sync_commands()`: lock, `first()` → clone its sender, else the same
  fail-closed fresh pair as today (D8 records the v1 reading: with
  length ≤ 1, `first()` *is* "the" connector; N-shape routing is the
  multi round's).
- `connector_attached()`: `!lock().expect("connector lock").is_empty()`.
- `detach_connector()`: drain all actors under one bounded critical
  section, then — outside the lock — `stop_actor` each (extracted
  helper, D7):

```rust
/// Sends one connector the graceful stop and joins it bounded: the
/// `Detach` command (an in-flight cycle completes first), the abort
/// at budget. Returns whether the join had to abort.
async fn stop_actor(actor: SyncActor, timeout: Duration) -> bool {
    if actor.commands.send(SyncCommand::Detach).await.is_err() {
        // The actor already exited on its own (e.g. a Shutdown
        // broadcast): the connector is gone all the same.
        tracing::info!("detach: sync actor already gone");
    }
    join_bounded(actor.join, timeout, "sync actor").await
}
```

  Outcome mapping over the Vec (v1 length 0 or 1, so unchanged
  observably): empty → `NoConnector`; any abort → `TimedOut`;
  otherwise `Detached`.

### 4.2 The permanent reports inbox

- Bootstrap keeps creating `(reports, report_rx)` exactly once
  (`:390`) and hands the engine `report_rx` exactly as today. The
  nextcloud arm passes `reports.clone()` into `spawn_sync_actor` and
  keeps the original; the standalone arm **no longer drops `reports`**
  — both backends park it in the new `App` field:

```rust
/// The twin of the engine's permanent reports receiver (design
/// §4.2.3): every attached actor spawns with a `clone()`, N actors
/// sharing one fan-in inbox, so the engine's select loop needs no
/// `reports_open` juggling. Parking a *sender* is semantically inert
/// (an empty open channel never fires `recv()`) — but it is the last
/// thing holding the linger-drain open, so [`App::shutdown`] drops it
/// before the engine join or the drain deadlocks (fact 14).
reports: mpsc::Sender<SyncReport>,
```

  A plain field, no mutex (D4): `Sender` is `Sync`, per-attach clones
  need only `&self`, and the single lifetime-ending drop belongs to
  `shutdown`, which owns `self`.

### 4.3 `App::shutdown` — the drop-ordering step

`shutdown` destructures `self` up front so the ordering is structural,
not mutex-juggling (D5):

```rust
pub async fn shutdown(self) -> ShutdownOutcome {
    let App { config, engine, system, connectors, reports, storage,
              storage_join, engine_join, _lock } = self;
    let timeout = Duration::from_secs(config.app.shutdown_timeout_secs);
    engine.flush().await;
    let _ = system.send(SystemEvent::Shutdown);

    // Bound the lock to the take: the guard must never span a join.
    let actors: Vec<SyncActor> =
        connectors.lock().expect("connector lock").drain(..).collect();
    let mut actor_aborted = false;
    for actor in actors {
        actor_aborted |= stop_actor(actor, timeout).await;
    }

    // THE ordering contract (design fact 14): App is the last holder
    // of the reports inbox's parked sender — every actor's clone died
    // with its join above — so this drop is what closes the reports
    // channel and lets the engine's linger-drain finish. Keeping the
    // sender past this point would hang the engine join to its bounded
    // abort.
    drop(reports);

    join_bounded(engine_join, timeout, "state engine").await;
    drop(storage);
    if let Err(join_err) = storage_join.await {
        tracing::warn!(?join_err, "storage actor join failed");
    }
    tracing::info!(actor_aborted, "shutdown complete");
    ShutdownOutcome { actor_aborted }
}
```

The sync-actor joins already ran before the engine join (fact 4), so
the step slots in as one explicit statement between them. The
`Mutex<Option<…>>` → `Mutex<Vec<…>>` widening changes `shutdown`'s
take from `.take()` to `.drain(..).collect()` — the join logic itself
is the "zero changes" WI-H fact 13 promised, plus this one added drop
step.

### 4.4 `App::attach_connector` — orchestration over named steps

```rust
/// Attaches a fresh sync connector to the running core: guards the
/// at-most-one invariant, rewires the engine's nudge registry (push +
/// FIFO ack), spawns the actor over a fresh command pair and a clone
/// of the permanent reports sender, and registers it. The actor
/// self-targets from the persisted board binding and fires its first
/// cycle immediately; awaiting settling (bounded, offline-first) is
/// the caller's job, not the primitive's.
///
/// # Errors
///
/// [`AttachError::AlreadyAttached`] when a connector is attached
/// (detach first — re-attach after detach is permitted);
/// [`AttachError::Engine`] when the engine could not take the push;
/// [`AttachError::Draining`] when it declined one (a raced shutdown).
pub async fn attach_connector(&self, client: DeckClient) -> Result<(), AttachError> {
    // 1. Guard: the v1 count-≤-1 invariant (lock released at the
    //    statement's end — never across the awaits below).
    ensure_vacant(&self.connectors)?;

    // 2. Fresh command pair for the new actor (the boot pair belongs
    //    to the boot actor; a detached one is dead).
    let (commands, commands_rx) = mpsc::channel(SYNC_COMMAND_CAPACITY);

    // 3. Engine rewire: push + ack (FIFO — nudges fan out to this
    //    sender from the ack onward).
    match self.engine.attach_connector(commands.clone()).await? {
        AttachReceipt::Attached => {}
        AttachReceipt::Draining => return Err(AttachError::Draining),
    }

    // 4. Spawn over the fan-in clone (the actor is self-configuring).
    let join = spawn_sync_actor(
        client,
        Arc::new(self.engine.clone()) as Arc<dyn SyncStateReader>,
        commands_rx,
        self.reports.clone(),
        self.system.clone(),
        self.config.sync_actor_config(),
    );

    // 5. Register — or, if a concurrent attach won the vacancy race,
    //    unwind by stopping the actor just spawned (the engine's
    //    now-dead entry self-prunes on the next fan-out).
    let actor = SyncActor { commands, join };
    let mut registry = self.connectors.lock().expect("connector lock");
    if registry.is_empty() {
        registry.push(actor);
        Ok(())
    } else {
        drop(registry);
        stop_actor(actor, Duration::from_secs(self.config.app.shutdown_timeout_secs)).await;
        Err(AttachError::AlreadyAttached)
    }
}
```

with

```rust
/// Typed attach refusals (the app layer's count-≤-1 policy lives
/// here, not in the engine).
#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    /// A connector is already attached (v1: at most one; detach
    /// first — re-attach after detach is permitted).
    #[error("a connector is already attached")]
    AlreadyAttached,
    /// The state engine could not take the push (stopped).
    #[error("state engine unavailable")]
    Engine(#[from] taskboard_state::AttachError),
    /// The state engine declined the push (draining — a raced
    /// shutdown).
    #[error("state engine is shutting down")]
    Draining,
}

fn ensure_vacant(connectors: &std::sync::Mutex<Vec<SyncActor>>) -> Result<(), AttachError> {
    if connectors.lock().expect("connector lock").is_empty() {
        Ok(())
    } else {
        Err(AttachError::AlreadyAttached)
    }
}
```

The mutex discipline: two short critical sections (step 1, step 5),
both released before any `.await`. The step-5 re-check closes the
TOCTOU window the design's one-line guard leaves open under
concurrent callers (D7) — the future daemon-side verb (design §6)
calls this exact method from a serial dispatcher, but a GUI double
fire or an IPC race must not produce a two-connector v1 App. The
unwind reuses `stop_actor` + the engine's lazy prune — both designed
paths, nothing new.

### 4.5 Bootstrap body diff

- Standalone arm: `drop(sync_rx);` stays; `drop(reports);` is deleted
  (parked instead — the comment now states the parking role and the
  shutdown ordering constraint).
- Nextcloud arm: `reports` → `reports.clone()` into the spawn.
- `App` construction: `connectors:
  std::sync::Mutex::new(vec-or-empty)`, `reports` moved in (after the
  clone). `SyncActor` itself is unchanged.

## 5. Test catalogue (TDD — written first, all hermetic)

Determinism by construction throughout: oneshot acks and command
receipts as completion signals, `eventually` predicate polls under
timeout guards, join-handle completion as the terminal signal, no
fixed sleeps, no time manipulation, no wall-clock assertions
(AGENTS §5). Engine tests mirror the existing harness helpers only
(the suite's Miri-cleanliness header keeps holding); app tests reuse
the B-series harness (`boot`, `eventually`, wiremock, env/fake
stores). No text assertions anywhere — receipts and typed variants
only.

### 5.1 Engine tier — j-series (`crates/taskboard-state/tests/engine.rs`)

New section banner: `WI-J: the connector registry and attach`.

- **j1 — attach registers the sender; later nudges fan out through it
  (the ack's FIFO proof).** Harness; fresh pair `(tx, rx)`;
  `attach_connector(tx)` → `Ok(AttachReceipt::Attached)`; then
  `RequestSync` → `CommandOutcome::SyncRequested`; `rx` receives
  `SyncNow` (timeout-guarded `recv`). The nudge issued *after* the ack
  reaching the attached sender is exactly the durability story.
- **j2 — the standalone dead entry prunes; the registry keeps
  working.** Spawn, `drop(h.sync_in)` (the boot pair's receiver — the
  standalone shape, with the reports channel *open*, matching the
  parked-sender world): `RequestSync` → `NoOp` and the published phase
  stays `Idle` (no `Syncing` transient — fact 4's trap, now with an
  open reports channel); then attach a fresh sender → `RequestSync` →
  `SyncRequested` (pruning emptied the registry; it still accepts
  entries).
- **j3 — fan-out forwards iff any send lands; `Closed` prunes lazily
  over both dead paths.** Attach senders A and B; drop A's receiver
  (the *detached-actor* dead path — dead after attach);
  `RequestSync` → `SyncRequested` and B receives `SyncNow` (forwarded
  on the live entry, dead one pruned in the same pass); drop B's
  receiver → `RequestSync` → `NoOp` (all dead, registry empty). With
  j2's dead-at-boot origin this pins both origins through the one
  mechanism, and the multi-entry tolerance the re-attach-after-detach
  app journey relies on (a stale dead entry beside a fresh live one).
- **j4 — a full connector inbox still counts as forwarded.** Attach a
  sender whose receiver has capacity 1 and is never drained: first
  `RequestSync` → `SyncRequested` (`Ok`), second `RequestSync` →
  `SyncRequested` (`Full` — coalesced, still forwarded). Pins the
  coalesce rule through the new fan-out.
- **j5 — attach during drain is declined typed.** `Shutdown` →
  `wait_draining`; `attach_connector(tx)` → `Ok(AttachReceipt::
  Draining)`; drop `report_tx` → the engine join completes
  timeout-guarded (the decline left the loop's exit conditions
  untouched).

### 5.2 App tier — B10–B14 (`crates/taskboard-app/tests/bootstrap.rs`)

New section banner: `WI-J: the connector attach`. Wiremock servers
where a URL is needed; the boardless journeys never issue HTTP (the
`NoBoard` report precedes any request), so no mocks are mounted.

- **B10 — standalone boot is fail-closed; attach makes it live; the
  actor's report (a clone of the parked sender) is ingested.** B8-shaped
  standalone config + `FakeCredentialStore`; before: `!
  connector_attached()` and `RequestSync` → `NoOp` (the byte-identical
  standalone pin in the parked-sender world); build a `DeckClient`
  against an un-mocked wiremock URI; `attach_connector(client)` →
  `Ok(())`; `connector_attached()` → true; `RequestSync` →
  `SyncRequested` (the nudge forwards through the registry);
  `eventually(phase == Failed { NoBoard })` — the spawned actor's
  commanded cycle reported through its clone of the parked sender and
  the engine ingested it (this is the acceptance bullet "a report sent
  on a clone of the parked sender is ingested" — the actor's own
  report is that clone; the field stays private, no direct-sender
  test). `shutdown()` → `!actor_aborted`.
- **B11 — double-attach refused typed.** Nextcloud boot (`boot`
  helper, boardless); `attach_connector(client)` →
  `Err(AttachError::AlreadyAttached)`; `connector_attached()` still
  true; `RequestSync` still `SyncRequested` (the booted connector was
  untouched by the refusal).
- **B12 — detach → re-attach (the connect→logout→connect lifecycle).**
  Nextcloud boot; `detach_connector()` → `Detached`;
  `attach_connector(fresh client)` → `Ok(())`; `connector_attached()`
  → true; `RequestSync` → `SyncRequested`; `eventually(Failed {
  NoBoard })`; `shutdown()` → `!actor_aborted`. Exercises the stale
  dead registry entry beside the fresh live one (j3's engine-tier
  tolerance, routed end to end).
- **B13 — the shutdown ordering contract (standalone).** Standalone
  boot with `shutdown_timeout_secs = 30` (a generous engine-join
  budget); `tokio::time::timeout(Duration::from_secs(10),
  app.shutdown())` completes; `!actor_aborted`. The test's comment
  states the prevented failure exactly: *a deliberately kept parked
  sender makes the engine join block for the whole
  `shutdown_timeout_secs` budget and then abort the engine — this
  deadline guard is the pin.* Why an explicit guard and not just
  `!actor_aborted`: on regression, `ShutdownOutcome.actor_aborted`
  tracks only the *sync* actor — the engine's bounded abort is
  invisible to it, so B1–B9-style assertions degrade silently
  (slow-but-green); the guard turns the hang into a failure.
- **Pre-existing suites unchanged**: B1–B9, the A/G/H-series, and the
  repl/ipc/cli_ipc suites pass as-is (`sync_commands()`'s signature is
  preserved; `App`'s private-field reshape touches no external
  surface). Stage 2 does not modify them; stage 3 diffs every touched
  test file to confirm.

### 5.3 Coverage of the chunk's acceptance list

| Dispatch acceptance | Pinned by |
|---|---|
| standalone boot byte-identical (nudge fails closed, no `Syncing`) | B8 unchanged + j2 + B10's pre-assertion |
| attach → nudges forward | j1, B10 |
| report on a clone of the parked sender ingested | B10 (the actor's own report) |
| dead entries prune on the next fan-out | j2 (dead at boot), j3 (dead after attach/detach) |
| `AlreadyAttached` refused typed | B11 |
| detach → re-attach works | B12 |
| standalone `App::shutdown` completes, `actor_aborted == false` (ordering contract; kept-sender variant documents the hang) | B13 (+ every B1–B9 shutdown, which regress to slow-but-green — hence B13's explicit guard) |
| pre-existing engine/bootstrap/WI-H suites unchanged | §5.2 last bullet |
| gates green | §9 |

## 6. Recorded decisions (stage 2 implements exactly these)

- **D1 — `spawn_state_engine`'s signature is unchanged.** The registry
  is a loop-internal `vec![sync_out]` over the existing single
  boot-sender argument. Reason: design §4.2.1 mandates a byte-identical
  boot path; every existing harness and doc-test keeps compiling; the
  engine's count-agnosticism needs no constructor change.
- **D2 — ack shape: `AttachReceipt { Attached, Draining }` rides the
  oneshot; a dedicated one-variant `AttachError::EngineGone` is the
  handle-level transport failure.** Reasons: design §4.2.2 names the
  `oneshot::Sender<AttachReceipt>` payload literally; a declined
  drain-mode attach is a *receipt* (the `CommandOutcome::NoOp`
  precedent), not an exception; `ExecuteError` is not reused because
  attach is not a `StateCommand` and the naming would lie. Drain-mode
  decline mirrors `Execute`'s decline arm — no new work, no new
  connectors after `Shutdown`.
- **D3 — fan-out and prune are one function** (`forward_sync_now` over
  `&mut Vec`, prune via `retain`). Reason: design §4.2.1's "pruned
  lazily right there" — the prune *is* the fan-out's `Closed` arm; a
  separate sweep pass would be a second mechanism for one fact.
- **D4 — the parked reports sender is a plain `App` field, not mutex
  bookkeeping.** Reason: `mpsc::Sender` is `Sync`; per-attach clones
  need only `&self`; exactly one lifetime-ending drop exists and it
  belongs to `shutdown(self)`. A mutex would add lock discipline for
  zero contention. (Refinement of the design's "parked in `App`"
  wording, same semantics.)
- **D5 — `shutdown` destructures `self`** so the parked sender's drop
  is one explicit, ordered statement between the actor joins and the
  engine join. Reason: the ordering is the contract (fact 14); a
  structural drop cannot be reordered by accident the way a
  field-access cleanup can, and the drop's comment carries the
  invariant.
- **D6 — app-side `AttachError { AlreadyAttached, Engine, Draining }`.**
  `AlreadyAttached` is D4-of-the-design-round (the app-layer count-≤1
  guard); `Engine` wraps the state crate's `AttachError` (`#[from]`);
  `Draining` maps the receipt (a raced shutdown — `shutdown(self)`
  consumes the `App`, so this is only reachable from a concurrent
  `&self` caller mid-shutdown, e.g. the future IPC verb). All typed;
  no message-text contracts.
- **D7 — the vacancy race is closed at registration, unwinding via
  `stop_actor`.** The design's guard is stated as a one-line check;
  under concurrent `&self` callers (GUI double fire, future IPC verb)
  a TOCTOU could register two actors in a v1 App. The re-check at
  step 5 plus a `Detach` + bounded join of the just-spawned actor
  returns `AlreadyAttached` and leaves the engine's stale entry to the
  designed lazy prune. Both mechanisms already exist; ~10 lines.
  Recorded as an addition beyond the design text, reason: correctness
  of the invariant the design itself mandates.
- **D8 — `sync_commands()` answers the first live sender.** With the
  v1 registry length ≤ 1, `first()` is exactly "the" connector and the
  method's contract (fail-closed when connector-less) is unchanged —
  `commands/boards.rs` and the IPC surfaces compile untouched.
  N-connector command routing is the multi round's (design §4.5
  deferred: *`SyncCommand` routing*).
- **D9 — `attach_connector` does not wait for settling.** The design
  (§4.2.6) makes the first cycle immediate and *optional* settling the
  caller's (`wait_for_boot_pull`, bounded, offline-first) — chunk 3's
  choreography. The primitive returns at registration.
- **D10 — zero diff in `taskboard-domain` and
  `taskboard-sync-nextcloud`.** The rewire is envelope-level (ADR
  0004); `spawn_sync_actor` already receives its reports sender by
  value; the `Detach` arm is WI-H's, reused verbatim.

No deviations from design §4.2 beyond the recorded refinements D4/D5
and the recorded addition D7; §4.5's "fixed now" list is implemented
exactly and its deferred list is touched not at all.

## 7. Risks

- **The drain-ordering interaction (the load-bearing one).** The
  parked sender holds the engine's linger-drain open for the App's
  whole lifetime; any shutdown path that forgets the drop (or orders
  it after the engine join, or leaks a clone elsewhere) turns *every*
  shutdown into a full-`shutdown_timeout_secs` wait followed by a
  bounded abort of the engine. The nastiest part: `ShutdownOutcome`
  does not report the engine abort, so the symptom is silent slowness,
  not a failed assert. Mitigations: the structural drop step (D5), the
  invariant comment at the field and at the drop, and B13's explicit
  deadline guard that converts the hang into a test failure. Related
  leak audit for stage 3: the only `reports` clones are App's parked
  original and the per-actor spawn clones — grep for any other
  `reports.clone()` before sign-off.
- **Mutex discipline in `attach_connector`.** Two critical sections
  with two `.await` points between them; a future edit that holds the
  guard across the engine ack would block `sync_commands()`/
  `connector_attached()` callers for the ack's duration (and violate
  the WI-H-established discipline the reviewers grep for). The named
  steps make the sections visually small; stage 3 checks no `.await`
  sits under a lock.
- **Biased-select fairness.** The attach arm rides the existing
  commands inbox — it inherits `Flush`'s priority, adds no new select
  branch, and cannot starve reports/system (the arm is O(1): one push,
  one reply).
- **`sync_commands()` v1 semantics.** `first()` is only "the"
  connector while the ≤ 1 invariant holds; a future regression that
  lets two actors register would make `SetBoard` ceremonies talk to
  one of two connectors nondeterministically. B11 + D7's re-check pin
  the invariant; the multi round owns the N-shape (D8).
- **Engine aborted mid-drain edge.** An attach ack awaited while a
  concurrent shutdown broadcasts `Shutdown` resolves `Draining` →
  typed `AttachError::Draining`, no actor spawned (nothing to leak).
  j5 pins the engine receipt; the app-side mapping is three lines in
  `attach_connector` exercised implicitly by j5 + B-series (a direct
  app-tier race test would be non-hermetic flake — deliberately not
  written; recorded here so stage 3 does not flag the gap as untested
  by oversight).
- **Miri cleanliness.** The j-series adds no timers and no new
  primitives beyond `oneshot`/`mpsc` — mirroring the existing helpers
  keeps the engine suite's Miri contract (header at `engine.rs:10-12`)
  intact.

## 8. Commit plan (stage 2) & diff estimate

Each commit standalone-green (rebase-merge hygiene; tests land with
their behavior; no fixup subjects):

1. `feat(state): connector attach command and nudge fan-out registry` —
   §3 in full + j1–j5.
2. `feat(app): park the reports sender and widen the connector slot` —
   §4.1, §4.2, §4.3, §4.5 + B13 (the topology is green and pinned
   before the primitive exists; `attach_connector` is not yet called
   by anything).
3. `feat(app): App::attach_connector mirror of detach` — §4.4 + B10–B12
   + the `AttachError` exports.

Estimated diff: **~450–550 lines** across 5 files
(`taskboard-state/src/actor.rs` ~+150, `lib.rs` ±1,
`taskboard-app/src/bootstrap.rs` ~+150, engine tests ~+120, app tests
~+140). Size "M" as dispatched. No new workspace dependencies; no
`Cargo.toml` changes.

## 9. Gates (stage 2, AGENTS §10 — all green before handoff)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
cargo deny check
cargo llvm-cov --workspace
```

No push, no PR (stage 3 ships). Backlog origin entries untouched
(chunk 3 lands them).
