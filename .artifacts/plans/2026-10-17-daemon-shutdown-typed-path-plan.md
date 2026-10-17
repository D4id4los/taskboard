# [Plan] Daemon Shutdown Typed Path — Chunk 7, Remediation Wave 1

- **Date**: 2026-10-17
- **Kind**: implementation plan (stage 1 of the three-stage dispatch protocol)
- **Origin**: issue #257 (normative; `backlog: Audit: typed path for the
  daemon-shutdown Arc::try_unwrap expect`), finding **F9-01** (P2, axis I.3+K)
  in `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md` with
  evidence in
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-concurrency-report.md`
  §7.1, dispatched as chunk 7 of
  `.artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md` (§1/§8).
- **Status**: READY — every open question settled below with a recorded
  decision. Evidence anchors verified against `main` @ `d8a1585`.

## 1. The finding, precisely

`crates/taskboard-app/src/main.rs:320`:

```rust
server.join(shutdown_timeout).await;
let app = Arc::try_unwrap(app).expect("the ipc server released the core");
app.shutdown().await
```

The `expect` is unreachable **today** but unenforced by the type system: if
any future `Arc<App>` clone exists at shutdown, **every** daemon shutdown
panics — mid-drain, skipping the entire graceful sequence (engine flush,
`Shutdown` broadcast, actor joins, storage drain) — exit 101 instead of a
clean, observable failure. On kiosk targets that is an unclean exit with a
data-durability window on every restart.

## 2. Today's strong-ref invariant (enumerated, @ d8a1585)

Production `Arc<App>` sites (workspace grep: `Arc<App>`, `app.clone()`,
`Arc::new(app`, `try_unwrap` — complete census):

| # | Holder | Acquired | Released |
|---|---|---|---|
| 1 | the daemon lifecycle's `app` | `main.rs:272` `Arc::new(app)` | consumed by `Arc::try_unwrap` at `main.rs:320` |
| 2 | the IPC dispatcher task's `dispatcher_app` | `main.rs:291` `spawn_server(app.clone(), …)` → moved into the task at `server.rs:160` | dispatcher task exit (drain path) or task abort (budget path) — **either way before `IpcServerHandle::join` returns** (`server.rs:104-134`) |

Verified non-holders: the accept-loop task (`server.rs:263-292` holds
`listener`, `in_tx`, `stop`), connection tasks (`server.rs:308` holds a
stream + `in_tx` clone — never `App`), `BasicSession` (config/store/path),
the system-event logger (a broadcast receiver only).

### What a stray second ref would mean, per teardown step

| Teardown step | With today's refcount | With a stray second ref |
|---|---|---|
| `server.join(budget)` (`server.rs:104-134`) | completes; the dispatcher's ref is released on both the drain and abort paths | **unaffected** — `join` only awaits/aborts its own two tasks; their refs release either way; the stray ref is invisible to it |
| `Arc::try_unwrap(app)` (`main.rs:320`) | `Ok(App)` — exclusive ownership handed to the tail | `Err(Arc<App>)` ⇒ the `.expect` **panics**: unwind tears through the `drain` async block and the force-exit `select!`, the process dies exit-101, dirty |
| `app.shutdown().await` (`bootstrap.rs:330`) | full graceful sequence runs | **never runs**: no engine flush (pending edits unpersisted), no `Shutdown` broadcast (listeners never told), no sync-actor/engine joins, no storage-actor drain |

The failure is total and permanent: the panic is on the unconditional
shutdown path, so the daemon could never shut down again cleanly until the
stray clone is removed.

## 3. Decision: typed error path with bounded salvage

**Chosen shape**: a new library function in `bootstrap.rs`,

```rust
pub async fn shutdown_from_arc(app: Arc<App>)
    -> Result<ShutdownOutcome, CoreStillShared>
```

which replaces the `expect` with a `match Arc::try_unwrap(app)`:

- `Ok(app)` → the owned graceful shutdown, unchanged —
  `Ok(app.shutdown().await)`.
- `Err(app)` → the typed failure path: `tracing::error!` with the observed
  strong count, then a **bounded borrow-safe salvage** — exactly the two
  steps of the graceful sequence that need no ownership:
  `app.engine.flush().await` (persist pending edits through the still-live
  engine; `EngineHandle::flush(&self)`, `state/actor.rs:157`) and
  `app.system.send(SystemEvent::Shutdown)` (notify listeners) — then
  `Err(CoreStillShared { strong_refs })`.

`main.rs`'s drain block becomes a call to this function; the second-signal
force-exit `select!` keeps wrapping it (130 semantics unchanged), and the
`Ok` arm keeps the `actor_aborted` warning exactly as today.

### Why the salvage stops after flush + broadcast

Everything past those two steps needs ownership: the actor joins depend on
the structural `drop(reports)` (`bootstrap.rs:365-370` — `App` must be the
*last* holder of the parked reports sender or the engine's linger-drain
cannot finish). A stray `Arc<App>` pins the whole `App`, **including
`reports`**, so the reports channel can never close while the stray lives:
attempting the engine join would stall into `join_bounded`'s abort for zero
benefit. The salvage therefore deliberately joins nothing; the doc comment
states this constraint. Salvage liveness = the owned shutdown's own flush
step (the engine is alive — it was never told to stop), and the second
signal still force-exits past it.

### Rejected alternatives (recorded per the dispatch prompt)

1. **Borrowed best-effort teardown** (`shutdown(self)` → `shutdown(&self)`):
   the ownership is load-bearing, not stylistic — `App` is documented
   `Not Clone` precisely so the sequence consumes it exactly once
   (`bootstrap.rs:126-127`), and the sequence is written as a destructuring
   (`bootstrap.rs:334-345`) whose `drop(reports)` ordering contract is
   *unimplementable* under `&self`. A borrowed variant would either hang the
   engine join (keeping the sender) or require re-plumbing the reports
   sender out of `App` — a redesign of the shutdown sequence, against this
   chunk's non-goals. It would also fork the sequence into owned/borrowed
   paths that can drift. Note the subset argument: the *feasible* part of a
   borrowed teardown is exactly the flush+broadcast salvage this design
   runs on `Err` anyway — the typed path strictly dominates.
2. **Refcount proof handed back by `spawn_server`** (dispatcher returns its
   `Arc` on exit): a returned clone does not prove the *absence of other*
   refs — the exclusivity check must exist somewhere regardless — and
   plumbing the returned `Arc` through `IpcServerHandle::join` changes its
   contract ("the caller's refcount is released either way" needs no
   proof object) and brushes the no-new-channel non-goal.

### The error type and where it surfaces

- **Type**: `CoreStillShared { strong_refs: usize }` — a `thiserror`
  struct (house style per AGENTS §4; single failure mode, precedent:
  `IpcBindError`) defined in `bootstrap.rs` beside `ShutdownOutcome`.
  `strong_refs` is the *advisory* `Arc::strong_count` at detection (racy by
  nature — evidence, not a lock; the doc comment says so).
- **Surfaces**: (a) `tracing::error!` at the detection site with the count
  as a structured field; (b) `Err` propagation out of `daemon_lifecycle` →
  `run` → `main`'s `color_eyre::Result` → process **exit code 1** — the
  existing runtime-failure class. **No new exit code** (the process
  contract 0/1/2/130 stays; supervisors already treat 1 as failure).
  `daemon_lifecycle`'s signature stays `color_eyre::Result<()>`; the typed
  error converts via `Into` like `IpcBindError` already does.

### Why the function lives in the library, not `main.rs`

`daemon_lifecycle` is private binary glue — unreachable from `tests/`.
`main.rs`'s own header states the rule: "All wiring lives in the
`taskboard_app` library so tests and future frontends reuse it." The
extraction is minimal: `shutdown_from_arc` takes only `Arc<App>` (no
`IpcServerHandle` — `join` stays in `main.rs`), so `bootstrap` gains **no**
`ipc` dependency and no module cycle appears (`ipc → bootstrap` already
exists; the reverse direction would have been smelly). Placement: beside
`run_until_signal` (`bootstrap.rs:715`), the owned-`App` lifecycle sibling.

## 4. Regression test (the dispatch's acceptance core)

New tests in `crates/taskboard-app/tests/ipc.rs` (next free numbers ip19 /
ip20), using a new fixture `served_with_core` that mirrors the daemon's
exact composition — it keeps the daemon-side `Arc<App>` and passes a
**clone** into `spawn_server`, exactly like `main.rs:291` (the existing
`served`/`serve_with_config` move the whole `Arc` in and cannot express
this; existing callers are not churned):

```rust
async fn served_with_core(dir: &Path, db_name: &str)
    -> (server::IpcServerHandle, Arc<App>, PathBuf)
```

**ip19 — the regression** (`ip19_a_stray_second_ref_across_join_is_a_typed_shutdown_failure`):
standalone boot (hermetic, credential-free per b8), clone the daemon-side
`Arc` (the "future bug" stray), `server.join(DRAIN_BUDGET).await` with the
stray held **across** the join, then:

```rust
let Err(shared) = taskboard_app::bootstrap::shutdown_from_arc(app).await
    else { panic!("a shared core must fail typed, not shut down") };
assert!(shared.strong_refs >= 2);   // floor on an advisory count, never exact
```

Typed result assertion only — **never** `catch_unwind` / panic-catching
(the dispatch prompt's explicit rule). Deterministic: no sleeps, no
intermediate states; `try_unwrap` is synchronous and the stray is held for
the whole call. After the `Err` returns, the core drops un-shutdown through
the stray's drop — the same detached-teardown precedent every existing ip
test already relies on (their `App` is consumed by the server and never
`shutdown()`-ed); `bounded(LIVE_DEADLINE)` wraps the body as everywhere in
the file.

**ip20 — the happy-path seam** (`ip20_the_shutdown_tail_takes_exclusive_ownership_after_the_drain`):
same fixture, **no** stray; `join` then `shutdown_from_arc` ⇒
`Ok(ShutdownOutcome { actor_aborted: false })` (standalone idle boot has no
sync actor — structural, per b8). This pins that the daemon's *own* ref
(the dispatcher's) is fully released by a real `join`, i.e. the
false-positive direction: a join that leaked its ref would turn every
normal shutdown into the typed failure and fail ip20.

**Layer justification** (AGENTS §5, one behavior one test lowest layer):
the behavior under pin is the daemon-tail *composition* (join + exclusivity
handoff + graceful handoff) — `tests/ipc.rs` is the lowest layer that can
compose it. The owned `shutdown()` internals stay pinned by the b-series;
nothing is duplicated. No new files: tests land in the existing module.

## 5. Happy-path invariance proof (existing tests, unmodified and green)

- `tests/bootstrap.rs`: b1 (graph cycle + shutdown), b3 (graceful shutdown
  reopens the db), b4 (mid-cycle bounded shutdown keeps the outbox), b6,
  b8 (standalone boot + clean shutdown), b9 (detach + shutdown), **b13**
  (the drop-ordering contract this chunk must not disturb).
- `tests/ipc.rs`: ip1, ip7 (shutdown ack → drain → socket removal), ip8,
  ip12 (a silent connection cannot wedge the drain).
- `src/ipc/server.rs` `drain_tests::join_bounds_the_drain_even_with_a_task_that_never_finishes`.
- `tests/cli_ipc.rs` — the binary-level daemon journeys; the real process
  path proves the `main.rs` rewiring behaves identically (same log spans on
  the happy path, same exit 0).
- Full AGENTS §10 gate suite.

## 6. Implementation shape & commit series

The production diff is exactly two files: `bootstrap.rs` (new
`CoreStillShared` + `shutdown_from_arc`, both beside the shutdown
vocabulary) and `main.rs` (the drain block calls the function; the outer
`select!` result becomes `Result<ShutdownOutcome, CoreStillShared>`; `Ok`
keeps the `actor_aborted` warning, `Err` converts to eyre). `server.rs`,
the dispatcher, the accept loop, and `App` itself are untouched.

1. **Commit 1 — `refactor(app): extract the daemon shutdown tail into the
   library`**: pure move of today's behavior into `shutdown_from_arc`
   (the `expect` carried over verbatim, signature momentarily
   `-> ShutdownOutcome`), `main.rs` rewired to call it. No behavior change;
   the §5 suite proves extraction invariance. ip20 may land here (it passes
   on this shape) or with commit 2 — implementer's choice, recorded.
2. **Red proof (worktree-only, never a committed red state)**: write ip19,
   run it against commit 1's shape — it **panics** (the `expect` fires).
   Capture the panic output verbatim for the stage-2 report. AGENTS §8
   forbids committing a state whose tests fail, so the red evidence is the
   report + the kept test, not a red commit.
3. **Commit 2 — `fix(app): typed failure path for a shared core at daemon
   shutdown`**: replace the `expect` with the `match` (typed path +
   salvage + error log), change the signature to
   `Result<ShutdownOutcome, CoreStillShared>`, update `main.rs`, land ip19
   (+ ip20 if not already in). Green.

After the chunk, `grep try_unwrap` across the workspace must show the tail
as a `match` with no `expect`/`unwrap` on the refcount path; the remaining
`expect`s in `server.rs` are the documented serde-serialization-totality
ones (different concern).

## 7. Non-goals (binding)

- No refactor of the dispatcher/accept loop; `IpcServerHandle::join` and
  its refcount-release contract are untouched.
- No new channel (no refcount-proof plumbing, no oneshot returning the Arc).
- No `shutdown(&self)` refactor; `App` stays `Not Clone`; the ownership
  semantics and the `drop(reports)` contract are preserved verbatim.
- No new exit code; the failure rides the existing 1 via color-eyre.
- No change to the GUI/`run_until_signal` path (owned `App`, unaffected).
- No second-signal/130 semantics change; no opportunistic fixes anywhere
  else on the tail (new issues instead, per the wave bookkeeping rule).

## 8. Risks

- **Extraction drift** (a subtle reorder in the move): mitigated by
  pure-move discipline + the §5 invariance list + the binary-level
  `cli_ipc` journeys.
- **`strong_count` is advisory/racy**: documented in the type; the test
  asserts a floor (`>= 2`), never an exact value (AGENTS §5: floors fine).
- **Salvage flush stall exposure**: identical to the owned shutdown's own
  flush step (engine alive, never told to stop); the second-signal
  force-exit still wraps the drain. Not a new hang surface.
- **Commit-hygiene vs TDD tension**: resolved by §6's sequencing (red
  observed in-worktree, test+fix land together).
- **Parallel-chunk collision**: chunk 6 (`fix/security-hardening`) also
  touches the app crate (incl. `server.rs` hardening); both branch off
  `main` independently, no stacking — at most a trivial rebase on the
  tests/ipc.rs fixture area.
- **Panic-freedom audit**: the new tail has no indexing/arithmetic/unwrap;
  `try_unwrap` cannot panic; `system.send` failure is explicitly ignored
  (`let _`) as in the owned sequence.

## 9. Acceptance mapping (dispatch §8)

| Dispatch acceptance | This plan |
|---|---|
| expect replaced by a typed, logged, non-panicking failure path | §3: `match Arc::try_unwrap` → `CoreStillShared`, `tracing::error!`, salvage, exit 1 |
| decision between refcount-check error vs borrowed teardown recorded | §3 + rejected alternatives |
| regression test: second ref held across join, typed outcome, no panic-catching | §4 ip19 |
| existing shutdown tests/behavior unchanged on the happy path | §5 invariance list + §6 commit 1 pure move |
| gates green | §5 (full AGENTS §10 suite in stage 2/3) |
