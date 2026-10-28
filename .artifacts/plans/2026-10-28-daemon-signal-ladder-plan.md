# [Plan] Daemon Signal Ladder and Exit Codes (DR-2/D)

- **Date**: 2026-10-28
- **Kind**: stage-1 TDD plan for chunk DR-2/D of the 2026-10-26
  six-rounds dispatch (issue #471, branch `feat/daemon-signal-ladder`;
  closes #162 with the chunk PR).
- **Origin** (normative):
  `.artifacts/plans/2026-10-26-signal-handling-design-plan.md` §3
  D-6/D-7 (the rung table and the exit-code contract are binding
  verbatim), D-0/D-1 (the policy sentence, the helper's placement),
  §2 facts F1–F3 and F15, §5 (the exit-code-churn risk owns this
  chunk's `contrib/` duty), §6 chunk D; the dispatch plan §1/§7.2.
  This plan applies those decisions to the code at `9af0910` (which
  already carries DR-2/R's ladder machine and DR-2/I's IPC work) and
  resolves the implementation-level questions they leave open
  (§10 — every resolution is recorded there; none re-opens a
  D-decision).
- **Branch / worktree**: `feat/daemon-signal-ladder` in
  `../taskboard-WT-feat-daemon-signal-ladder`, off `origin/main` @
  `9af0910`. Precondition verified: DR-2/I landed
  (`0985044…2a801c1`), so the shared ladder in
  `crates/taskboard-app/src/signals.rs` is on `main`. Doctor passed
  (225G free, floor 25G).
- **Status**: PLANNED — stage 2 implements; stage 3 reviews and ships
  (closes #162/#471).

---

## 1. Mission & the diff-shape rule

The daemon lifecycle gets the three-rung signal ladder (D-6, verbatim
in §3 below) and the honest exit-code contract (0 clean / 1 degraded
/ 130·143 forced), and the GUI + kiosk tails migrate onto the same
shared ladder helper — the rung counting exists once, in
`signals.rs`'s machine; a local reimplementation in any tail is a
stage-3 finding (dispatch §7.2).

As a review-checkable rule (dispatch §7.2 + review prompt D.3):

- **Allowed**: edits in `crates/taskboard-app/src/{signals.rs,
  bootstrap.rs, main.rs, ui.rs, ipc/server.rs, web/mod.rs, lib.rs
  (visibility only, if needed)}`; a new
  `crates/taskboard-app/tests/daemon_signals.rs`; test-module
  extensions in `ipc/server.rs`, `web/mod.rs`, `bootstrap.rs`; the
  runbook (`docs/kiosk_bringup_runbook.org`); `contrib/
  taskboard-kiosk.service` + `contrib/taskboard.service`;
  `CHANGELOG.md`.
- **Forbidden**: domain/state/storage/sync diffs; any UI-slint diff;
  new dependencies; `unsafe` (rung 3's design text says `_exit` —
  under `#![forbid(unsafe_code)]` that lands as
  `std::process::exit`, §10 Q13); signal installation anywhere but
  the production lifecycle entries (D-7); sleeps standing in for
  state waits; text assertions.

## 2. Verified baseline — seam facts at `9af0910`

The design's F1–F3 hold, at today's line numbers (they moved when
DR-2/R and DR-2/I landed):

| # | Fact today | Evidence |
|---|---|---|
| B1 | The daemon tail still uses `wait_for_signal` (ctrl_c + SIGTERM, *no* SIGHUP) with a split-brain first/second-signal design: the serve-phase select waits for signal #1, then `drain_and_shutdown` re-arms a fresh `wait_for_signal` as the second-signal force | `main.rs:500-507`, `main.rs:532-537`, `bootstrap.rs:1156-1178` |
| B2 | Second signal during the drain force-exits a **flat 130** — regardless of the forcing kind | `main.rs:534-537` |
| B3 | A degraded drain (budget hit / sync actor aborted) only logs; the process exits **0** — the lie this chunk fixes | `main.rs:541-545`; `bootstrap.rs:568-605` (`ShutdownOutcome { actor_aborted }` only) |
| B4 | The ladder machine is shared and pure: `Ladder::new(middle_rungs)`, `Signal{Interrupt,Terminate,Hangup}` with `exit_code()` = 128+signum, `Event{Signal,Progress}` → `Action{Initiate,Escalate,Force}`; the daemon's `Ladder::new(1)` shape and the mixed-kind row are already unit-pinned | `signals.rs:41-150`, tests at `signals.rs:480-501` |
| B5 | The bounded drains **swallow** the budget-hit fact: both `join` fns warn+abort and return `()` | `ipc/server.rs:118-141`, `web/mod.rs:108-121` |
| B6 | `App::shutdown` = engine flush → `Shutdown` broadcast → bounded sync-actor joins → bounded authority/engine/notifier joins → storage drain; `join_bounded`/`stop_actor` return "had to abort" but only the sync-actor bit survives into `ShutdownOutcome` | `bootstrap.rs:535-606`, `bootstrap.rs:709-735` |
| B7 | All actor join handles are abortable through `&App` (`JoinHandle::abort(&self)`): `connectors` (each `SyncActor.join`), `authority_join`, `engine_join`, `notifier_join`, `storage_join` | `bootstrap.rs:340-372` |
| B8 | `EngineHandle::flush(&self)` is a command-envelope roundtrip; it needs the engine *and* storage actors alive to complete — the salvage flush must precede any actor abort that touches them | `taskboard-state/src/actor.rs:164-172` |
| B9 | The three `wait_for_signal` tail consumers to migrate: `serve_and_shutdown`, `drain_and_shutdown`'s force arm, the kiosk appliance watcher, and the desktop GUI watcher (`run_until_signal` keeps its own and stays) | `main.rs:501`, `main.rs:534`, `main.rs:690`, `ui.rs:433-437` |
| B10 | The pty tier already proves the real-signal recipe: wiremock-stalled sync cycle parks the drain, paced kills beat signal coalescing, exit code read from the reaped child | `tests/cli_tty.rs:1902-1989` (Case 4, the 143 pin) |
| B11 | The headless process tier exists: `common::binary_command()` (`CARGO_BIN_EXE_taskboard`), `write_test_config`, `wait_until` (poll-to-serving), and `cli_ipc.rs`'s daemon-spawn pattern | `tests/common/mod.rs:437`, `tests/cli_ipc.rs:14,49-63` |
| B12 | `daemon stop` is answered outside the drain (the accept-path control lane, DR-2/I): `server.stopped()` is the tail's second rung-1 trigger | `main.rs:504`, `ipc/server.rs:70-100` |
| B13 | A forced exit leaves the unix socket file behind; boot already probes-and-cleans a stale artifact, so rung 2/3 skipping socket removal is recovered at next boot | `ipc/server.rs:215` (stale probe budget) |

## 3. The rung table and exit-code contract (D-6, binding verbatim)

| Rung | Trigger | Behavior |
|---|---|---|
| 1 | first SIGINT/SIGTERM/`daemon stop` | today's graceful drain: stop accepting, bounded joins (`shutdown_timeout_secs`), socket removed, `shutdown_from_arc` |
| 2 | second signal while rung 1 is still running | skip the bounded joins, abort actors, one salvage attempt at an engine flush under a small fixed bound (2 s), exit `128+signum` (130/143) |
| 3 | third signal | immediate `_exit(128+signum)` — the "rung 2 itself wedged" escape |

| Code | Meaning |
|---|---|
| 0 | clean: full drain, no aborts — including signal-initiated graceful and `daemon stop` |
| 1 | **degraded drain**: budget hit and/or sync actor aborted (today this exits 0 with a log line, F3/B3 — invisible to supervision) |
| 130 / 143 | forced by SIGINT / SIGTERM at rung 2+ |

`SIGQUIT` keeps its kernel default on every surface (untouched);
`SIGHUP` is **not installed** on the daemon surface (§10 Q3): it
keeps today's default-disposition death. Platform shape per D-7:
unix-only streams `cfg`-gated; Windows gets the single ctrl_c bridge
(§10 Q12).

## 4. D-1 — the lifecycle watcher: `install_lifecycle_watcher` + `Event::StopRequest`

### 4.1 The pure machine's one new event

`daemon stop` is a rung-1 *trigger* per the table above, and the rung
counter must see it — otherwise a SIGINT arriving during a
stop-initiated drain is miscounted as the first trigger and the
operator needs one extra signal to force (§10 Q1/Q2). The counting
exists once: a new `Event::StopRequest` variant, wired by the tail,
with exactly three transitions added to `Ladder::on`:

- `(Fresh, StopRequest)` → `Armed { rung: 1 }`, answers `Initiate`,
  **does not record** `last_signal` (the forcing code must always
  name a real signal; `daemon stop` can never be the forcing event —
  it has no code and cannot fire rung 2/3).
- `(Armed { .. }, StopRequest)` → `None` (idempotent: the drain is
  already starting; a stop during a signal-initiated drain re-triggers
  nothing).
- `(Forced, StopRequest)` → `None`.

`Signal` and `Signal::exit_code` widen `pub(crate)` → `pub` (the
tails map escalate payloads to exit codes); `Event` stays
crate-internal.

### 4.2 The watcher (thin glue, untested per house rule)

`signals.rs` gains `pub fn install_lifecycle_watcher() ->
LifecycleSignals` — the daemon/kiosk/GUI analogue of
`install_oneshot_watcher`/`install_session_watcher`, driving **one
merged `Ladder::new(1)`** (the pure tier's
`mixed_kinds_count_together_on_one_instance` is this wiring's
contract):

- **Feeders**: unix — `SignalKind::interrupt()` + `terminate()`
  streams pumping `Event::Signal` into one mpsc pump (the
  session-watcher shape); Windows — the single ctrl_c bridge
  delivering one `Interrupt`, then ending (§10 Q12). A failed
  registration spawns no feeder (the `wait_for_signal` degrade
  precedent).
- **Two side channels** consumed in the same select: a
  `stop_request: watch::Receiver<()>` → `Event::StopRequest` (fed by
  the tail's `daemon stop` arm), and a `progress: watch::Receiver<()>
  ` → `Event::Progress` (fed by the tail when the drain future
  resolves — the de-arm that keeps a clean exit 0 from being flipped
  by the epilogue, §10 Q9). Dead senders mean *never act*
  (`changed_or_never` semantics — a dying tail must not fire an
  arm), so the watcher survives to rung 3 even after its tail is
  gone.
- **Actions**: `Initiate` → `begin_drain.send(())` (the tail's serve
  select listens); `Escalate` → `escalate.send(forcing_signal)` (the
  tail's rung-2 arm listens; the payload names the exit code);
  `Force` → `std::process::exit(last_signal.exit_code())` — no tty
  restore (this surface never saves one; the one-shot precedent),
  `std::process::exit` not `_exit` under `forbid(unsafe_code)`
  (§10 Q13).

```rust
pub struct LifecycleSignals {
    pub begin_drain: watch::Receiver<()>,   // rung 1 (signals)
    pub escalate: watch::Receiver<Signal>,  // rung 2 (carries the forcing kind)
    pub stop_request: watch::Sender<()>,    // daemon-stop arming (rung 1)
    pub progress: watch::Sender<()>,        // drain resolved; de-arm
}
```

### 4.3 Install points — once per lifecycle entry, after boot

The watcher installs immediately **after the core boot returns**:
`daemon_lifecycle` and `run_kiosk` (right after `boot_and_serve`, so
the fixed-board/mount error paths already hold a `LifecycleSignals`
for their `drain_and_shutdown` calls) and `run_desktop` (right after
its `bootstrap`). Signals during the boot itself keep today's
default-disposition death — D-7 scopes installation to the lifecycle
tails, and an install-at-top would *swallow* a boot-phase ^C into an
`Initiate` nobody listens to yet (a regression §10 Q11 rules out).
The `LifecycleSignals` value threads as a plain parameter through
`serve_and_shutdown`/`drain_and_shutdown`/the kiosk watcher — no
globals.

## 5. D-2 — rung 2: the salvage path (order, bound, failure)

**Ownership discovery that shapes the tail**: `drain_and_shutdown`'s
drain future consumes the `Arc<App>` (into `shutdown_from_arc`), but
the rung-2 salvage needs `&App` (connector aborts, engine flush). An
`Arc` clone for the escalate arm is fatal — it makes
`Arc::try_unwrap` fail on *every* clean drain (`CoreStillShared`).
The tail therefore splits into two interruptible phases, matching
D-6's rung-2 sequence exactly:

```rust
async fn drain_and_shutdown(app, server, web, signals) -> Result<()> {
    let t = Duration::from_secs(app.config.app.shutdown_timeout_secs);
    // Phase A: the bounded server drains — interruptible at full salvage.
    let (ipc_hit, web_hit) = tokio::select! {
        hits = async { let i = server.join(t).await; let w = web_join(web, t).await; (i, w) } => hits,
        () = changed_or_never(&mut signals.escalate) =>
            escalate_exit(&app, &mut signals.escalate).await,   // (i)
    };
    // An escalation retained across the phase boundary is consumed
    // here, while `app` is still owned.
    if let Some(forced) = take_escalation(&mut signals.escalate) {
        escalate_exit(&app, forced).await;                       // (ii)
    }
    // Phase B: the owned shutdown — its *first* step is the engine
    // flush (App::shutdown's order, B6), so an escalation landing
    // here finds the salvage already done or in flight.
    let outcome = tokio::select! {
        outcome = shutdown_from_arc(app) => outcome?,            // CoreStillShared → exit 1 via eyre (today's shape)
        () = changed_or_never(&mut signals.escalate) =>
            forced_exit(&mut signals.escalate),                  // (iii)
    };
    let _ = signals.progress.send(());
    exit_if_degraded(outcome, ipc_hit || web_hit);
    Ok(())
}
```

- **(i)/(ii) `escalate_exit`** — rung 2 in full: read the forcing
  `Signal` off the watch; abort the connector sync actors
  (`bootstrap::abort_connectors(&app)` — drain the registry,
  `join.abort()` each, *no* `Detach` send, *no* join wait: D-6 says
  "abort actors"; the sync actors are the wedgers an in-flight cycle
  pins); then the salvage flush,
  `bootstrap::salvage_flush(&app, SALVAGE_FLUSH_BUDGET)`; then
  `std::process::exit(forced.exit_code())`.
- **Abort-before-flush order** (§10 Q4): the flush roundtrips the
  engine and storage actors (B8), so D-6's list reads as "stop
  waiting on the wedging actors, then salvage what the engine still
  can" — aborting the engine/storage actors first would make the
  salvage unfireable. The engine/storage/notifier/authority actors
  are *not* aborted in phase (i): the flush needs them, and process
  exit is their terminator moments later.
- **(iii) `forced_exit`** — escalation during the owned shutdown:
  plain `exit(forced.exit_code())`. Justification recorded for
  review: `App::shutdown` flushes *first* (B6), so the salvage
  obligation is already satisfied or literally in flight; the wedged
  flush that outlives its budget here is the one case rung 2 cannot
  rescue (§10 Q5).
- **The salvage's bound and failure handling** (§10 Q5/Q6):
  `pub async fn salvage_flush(app: &Arc<App>, budget: Duration) ->
  bool` in `bootstrap.rs` = `tokio::time::timeout(budget,
  app.engine.flush())`; on expiry (or a send error from an engine
  already gone): one `tracing::warn!`, return `false`, and the exit
  proceeds at the forcing code — best-effort by design, the code is
  never shaped by the flush's fate. `SALVAGE_FLUSH_BUDGET:
  Duration = Duration::from_secs(2)` (the design's fixed small
  bound); the budget is a parameter for the hermetic zero-budget
  test (T8).
- **No `Shutdown` broadcast in the salvage**: D-6 names only the
  flush; `shutdown_from_arc`'s existing CoreStillShared salvage
  keeps its own shape untouched.
- **Consequences recorded**: rung 2/3 skip socket-artifact removal —
  the boot's stale-socket probe recovers it (B13). Rung 2 from a
  cold start (two rapid signals during the serve phase) is legal and
  lands in phase (i) via the retained escalation. At most one
  escalation can ever fire (the ladder is `Armed{2}` afterwards; the
  next signal is rung 3 in the watcher).

## 6. D-3 — the degraded-drain detection: what exactly flips 0 → 1

- **The predicate**: a drain is degraded iff
  `outcome.actor_aborted` (sync actor aborted at its budget) **or**
  `outcome.budget_hit` (any other bounded join aborted at its
  budget: authority, engine, notifier — `join_bounded`'s return
  value finally aggregated, B6) **or** the IPC/web server drain hit
  its budget (B5's swallowed fact, surfaced below). New
  `ShutdownOutcome { actor_aborted: bool, budget_hit: bool }` +
  `pub fn degraded(&self) -> bool` — the definition lives in exactly
  one place (§10 Q7).
- **The joins report their budget**: `IpcServerHandle::join` and
  `DashboardServerHandle::join` return `bool budget_hit` (two-state
  fact, the `actor_aborted: bool` precedent; `true` = aborted at the
  budget). All call sites are in-tree: `main.rs` and the two test
  modules.
- **The flip**: `bootstrap::exit_if_degraded(outcome:
  ShutdownOutcome, server_budget_hit: bool)` — consolidated logging
  (the per-tail "sync actor did not stop in time" warns fold into
  it): warn on the abort facts, `tracing::error!` + `exit(1)` when
  `degraded()`. Called from `drain_and_shutdown` (with
  `ipc_hit || web_hit`) and from `run_desktop`'s tail (with
  `false` — the GUI has no server joins). Production-path-only glue
  that exits the process, so it carries no test of its own; the
  predicate (T5) and the aggregation (T7) carry the behavior.
  `CoreStillShared` keeps its eyre route to exit 1 — consistent with
  code 1's meaning ("not a clean drain").
- **Clean stays clean**: a full drain with no aborts exits 0 whether
  it was triggered by `daemon stop` (the control lane arm) or by one
  signal (the `begin_drain` arm) — the contract's row 0, verbatim.

## 7. D-4 — the tail migrations (each surface's diff)

- **`serve_and_shutdown`** (`main.rs`): the serve select becomes
  `changed_or_never(begin_drain)` (signal-initiated rung 1) vs
  `server.stopped()` (the control lane) — the latter additionally
  feeds `stop_request` so a later signal escalates instead of
  re-initiating (§4.1). The obsolete split-brain comment ("the
  first-signal wait happens *before* arming the force-exit watcher")
  is deleted: one watcher owns every stream now, there is no
  first/second listener split to race.
- **`drain_and_shutdown`**: §5's two-phase shape; signature gains
  `&mut LifecycleSignals`. All four existing call sites thread it
  (the daemon tail, the kiosk headless fallback, and the kiosk
  fixed-board/mount error paths).
- **Kiosk appliance watcher** (`run_kiosk`, `main.rs:687-701`):
  `wait_for_signal()` → `changed_or_never(&mut signals.begin_drain)`
  in the watcher select; the `stopped_from(stop_rx)` arm sends
  `stop_request` exactly as `serve_and_shutdown` does. Everything
  else (the `quit_until_accepted` boot-window retry, the aborts, the
  shared drain) stays.
- **Desktop GUI** (`run_desktop`, `ui.rs:433-449`): install after
  bootstrap; the exit watcher waits on `changed_or_never(begin_drain)`
  → `quit_until_accepted` (retained-signal behavior rides the
  existing boot-window retry). The tail becomes
  `select! { outcome = app.shutdown() => outcome, () =
  changed_or_never(escalate) => exit(code) }` then
  `exit_if_degraded(outcome, false)` — the escalate arm needs
  nothing from `App` (it takes `self` by value; its flush already
  ran as step one), so there is no ownership trap on this surface.
  The GUI runs the same `new(1)` config: 2nd signal = prompt
  exit at the forcing code (the skip-joins role, played by this
  arm), 3rd = the watcher's `Force`.
- **Unchanged on purpose**: `wait_for_signal` and `run_until_signal`
  stay (B9) — the library convenience keeps its shape; its returned
  outcome simply gains the `budget_hit` field. SIGQUIT/SIGKILL/
  SIGHUP dispositions untouched (§3).

## 8. D-5/D-6 — runbook, contrib services, CHANGELOG

- **Runbook** (`docs/kiosk_bringup_runbook.org`): new section
  "9. Exit codes, the signal ladder, and supervision" — the rung
  table and the exit-code contract (§3, verbatim numbers), the
  signal inventory of the daemon surface (SIGINT/SIGTERM installed;
  SIGHUP/SIGQUIT/SIGKILL keep kernel defaults, `stty sane` as the
  tty-recovery one-liner per D-8), and the systemd mapping:
  - kiosk unit (`Restart=always`): every exit restarts; add
    `SuccessExitStatus=143` so a SIGTERM-forced stop *reads* as a
    clean stop in `systemctl` results.
  - daemon unit (`Restart=on-failure`): **no** `SuccessExitStatus`,
    deliberately — a forced (143) or degraded (1) exit must count as
    failure so supervision recovers the daemon; `systemctl stop`
    suppresses `Restart=` anyway, so an operator-initiated stop is
    not fought.
- **`contrib/taskboard-kiosk.service`**: the tail comment paragraph
  ("SIGTERM triggers the graceful drain…; the second signal
  force-exits (130)") rewritten to the three rungs + code table;
  `SuccessExitStatus=143` added.
- **`contrib/taskboard.service`**: same comment rewrite; no
  `SuccessExitStatus` (with the reason in the comment).
- **`CHANGELOG.md`**: Unreleased/`### Changed` entry (#162): the
  daemon/kiosk/GUI lifecycle joins the one signal policy — the
  three-rung ladder, exit codes 0/1/130/143, degraded drains now
  exit 1 (the exit-0 lie is dead), `daemon stop` counts as rung 1.

## 9. The test catalogue — TDD order

Every commit lands its tests with the minimal impl that satisfies
them (each commit standalone). No sleeps; `tokio::time::pause`/
`advance` where time is involved; typed outcomes only.

**Pure tier — `signals.rs` tests (T1–T3, commit 1):**
- **T1** `stop_request_arms_rung_1_without_a_kind`:
  Fresh+StopRequest → `Initiate` with `last_signal() == None`;
  Armed+StopRequest → `None`; Forced+StopRequest → `None`.
- **T2** daemon stop then signal escalates, then forces at the
  signal's code: StopRequest → Initiate; `Interrupt` → `Escalate`;
  `Terminate` → `Force`, `last_signal().exit_code() == 143`.
- **T3** the exhaustive sweep's alphabet gains `StopRequest`
  (4⁴ × 2 configs = 512 sequences). Extended invariants: StopRequest
  acts only off Fresh (and then exactly as trigger #1 — it *counts*
  toward the rung but never escalates, never forces, never records a
  kind); `Force` still only at the (2+middle)-th unacknowledged
  *signal*; forced ladders stay inert; the recorded kind survives
  de-arms. The existing three tables stay untouched.

**Predicate tier — `bootstrap.rs`/`ipc/server.rs`/`web/mod.rs`
tests (commits 2–3):**
- **T4** `shutdown_outcome_degraded_truth_table` (pure, 4 rows).
- **T5** `salvage_flush` happy path: a real bootstrapped App with a
  pending command — salvage returns `true` and the command is
  persisted (reopen the db through the storage actor).
- **T6** `salvage_flush` give-up branch: `budget = Duration::ZERO`
  (the flush is a channel roundtrip — it cannot complete inside the
  first poll slot, so the timeout is deterministic under both real
  and paused time) → returns `false`, no panic; the *caller* then
  exits — that proceed-anyway shape is the branch's behavior.
- **T7** the aggregation: extend the existing bootstrap shutdown
  tests — a clean shutdown yields `{actor_aborted: false,
  budget_hit: false}`; the existing wedged-sync-actor case (wiremock
  parked cycle, tiny `shutdown_timeout_secs`) yields both flags
  `true`.
- **T8** drain budget reporting: the `drain_tests` harness
  constructs `IpcServerHandle`s directly — the wedged-dispatcher
  case now asserts the returned `budget_hit == true`; a new
  completes-in-time case asserts `false`. Mirror pair in
  `web/mod.rs`'s tests module.

**Process tier — new `tests/daemon_signals.rs` (commit 4's
company):**

*Spawn justification (the dispatch prompt asks explicitly): exit
codes are process properties, and the ladder's handlers live in
watcher tasks on the production runtime — D-7 forbids installing
them on hermetic test paths, so no in-process test can pin a code.
Real signals are only sanctioned at the outermost tier (the pty
precedent, D-9). The daemon is headless, so the plain-child tier is
the correct lightweight form: `common::binary_command()` +
`write_test_config` + `wait_until`-polled serving (B11), `nix::sys::
signal::kill` to the child, `child.wait()` for the code — no pty
needed. Inter-kill pacing of 500 ms beats standard-signal
coalescing (the Case-4 comment sanctions exactly this; it is not a
wait-for-state sleep).*

- **T9** `sigterm_once_exits_0`: standalone config; SIGTERM after
  `wait_until` serving → code 0 (contract row 0, signal-initiated).
- **T10** `daemon_stop_exits_0`: `taskboard daemon stop` from a
  second child → the *daemon* exits 0 (contract row 0, control-lane
  row). If `cli_ipc.rs`'s teardown already observes the daemon's
  exit here, extend that assertion instead of adding a spawn.
- **T11** `second_sigterm_during_a_wedged_drain_exits_143`: the
  Case-4 recipe (connector config, wiremock 30 s parked boot sync,
  `shutdown_timeout_secs = 20` holds the drain); SIGTERM, 500 ms,
  SIGTERM → code 143 (rung 2: skip joins + salvage + force).
- **T12** `sigint_during_a_wedged_drain_exits_130`: T11 with
  SIGINT as the forcing kind — pins the unix interrupt stream's
  installation on this surface (the kind→code mapping itself is
  pure-tier).
- **T13** `third_sigterm_during_a_wedged_drain_still_exits_143`:
  three paced SIGTERMs → code 143, never a hang. The rung-2/rung-3
  distinction is not code-observable (both force at 128+signum);
  this pin's subject is the escape's existence — the third signal
  cannot make the shutdown *worse* than forced (the salvage's
  internal order is T5/T6's subject; the watcher's force mechanism
  is the pty tier's Case 4).
- **T14** `degraded_drain_exits_1`: connector config, wiremock
  parked boot sync, `shutdown_timeout_secs = 1`; one SIGTERM → the
  bounded sync-actor join aborts (the Detach cannot complete while
  the cycle is parked) → `actor_aborted` + `budget_hit` → code 1
  (the F3 lie's tombstone).
- Unix-only: the file is `#![cfg(unix)]` (Windows' single-bridge
  story is documented, not spawned).

**Structural checks (stage 3, no new tests):** grep the merged tree
for `wait_for_signal` (only `run_until_signal` may remain), for any
`Ladder` use outside `signals.rs` (the tails consume
`LifecycleSignals` only), and for `exit(1`/`exit(13`/`exit(14` —
only via `exit_if_degraded`/the ladder arms.

## 10. Resolved questions (the decision record)

- **Q1 — `daemon stop` arms the ladder via a machine event, not
  tail bookkeeping.** A wiring-side "drain in flight" flag would be
  local ladder logic in a tail (a stage-3 finding per dispatch
  §7.2). The machine owns all counting: `Event::StopRequest`.
- **Q2 — StopRequest never escalates, never forces, never records a
  kind.** The forcing code must name a real signal; `daemon stop`
  has no code and D-6 gives it only the rung-1 role. A signal during
  a stop-initiated drain escalates (trigger #2) and *its* kind names
  the code.
- **Q3 — no SIGHUP on the daemon surface.** D-6's table says
  SIGINT/SIGTERM verbatim; today's `wait_for_signal` installs
  exactly those (B1), so the surface is unchanged — an unhandled
  SIGHUP kills by default disposition, today-identical. The runbook
  says so.
- **Q4 — rung 2 aborts the connector sync actors first, then
  flushes.** The flush roundtrips engine+storage (B8); aborting
  those first would unfireable the salvage. "Abort actors" reads as
  the wedging sync actors; the rest die at process exit.
- **Q5 — salvage failure = warn + proceed at the forcing code.**
  Best-effort by design (the exit code is shaped by the forcing
  signal, never by the flush's fate). Escalation during the *owned*
  shutdown exits immediately: `App::shutdown` flushes first (B6),
  so the salvage is done or in flight; a flush wedged inside the
  owned path is the unrescuable case.
- **Q6 — the bound is `SALVAGE_FLUSH_BUDGET = 2 s`, passed as a
  parameter.** The design fixes the value; the parameter is T6's
  seam.
- **Q7 — degraded ≡ `actor_aborted ∨ budget_hit ∨ ipc/web budget
  hit`; exit 1 via `bootstrap::exit_if_degraded`.** The definition
  is one method; the flip is one function called from the two tails;
  `CoreStillShared`'s existing eyre route already lands on 1.
- **Q8 — the joins return `bool budget_hit`.** Two-state fact,
  `actor_aborted: bool` precedent; all call sites in-tree.
- **Q9 — `Progress` is sent when the drain future resolves.**
  Without the de-arm, a signal in the exit epilogue could flip a
  clean 0 into a forced code, breaking contract row 0. The GUI tail
  omits the send (its epilogue is two lines and the decided edge
  below covers it).
- **Q10 — the escape is unconditional.** After a de-arm, three
  fresh signals can still traverse to `Force` during the exit
  epilogue. D-1: the force rung is always armed; an operator
  hammering signals during exit wants death — forced outranks
  clean, and the contract's row 0 describes the drain's outcome,
  not an unforced epilogue.
- **Q11 — install after boot, once per lifecycle entry.** An
  install-at-top would swallow a boot-phase ^C into an `Initiate`
  with no listener (a regression against today's instant death);
  D-7 scopes installation to the lifecycle tails, and the boot was
  never promised signal coverage.
- **Q12 — Windows: the single ctrl_c bridge, rung 1 only.** One
  notification → graceful drain; the console's own second-^C
  disposition is the force world (identical to both existing
  watchers' documented story). Unix-only streams cfg-gated.
- **Q13 — rung 3 is `std::process::exit`, not `_exit`.**
  `#![forbid(unsafe_code)]` (the crate-wide rule, AGENTS §6) rules
  libc's `_exit` out; the design's *intent* — "immediate, no salvage,
  the escape" — is preserved: the difference between rungs 2 and 3
  is the salvage attempt, not the exit mechanism. Same for rung 2.
- **Q14 — the runbook documents 130/143.** Those are the only
  handled kinds on this surface; `Signal::exit_code` stays total
  (129 for HUP) for the pure tier's genericity.
- **Q15 — `wait_for_signal`/`run_until_signal` stay.** The exported
  convenience keeps its shape; only the four tail call sites
  migrate (B9).
- **Q16 — the process tier is a plain child spawn, no pty.**
  Justified above (§9): exit codes are process properties; D-7
  forbids handler installs on test paths; headless needs no tty.
  Reuses `common::binary_command`/`write_test_config`/`wait_until`
  and the Case-4 wiremock recipe; `nix` is already a direct
  dependency.

## 11. Risks

- **Exit-code churn for supervision** (design §5 — this chunk owns
  the mitigation): units keyed on "0 is the only good exit" will see
  1 (degraded) and 130/143 (forced). The runbook's systemd mapping
  and both `contrib/` units are updated in the same PR; the
  kiosk unit gets `SuccessExitStatus=143`, the daemon unit
  deliberately does not (an unattended forced daemon must restart).
  Residual: out-of-tree supervisor configs copied from the old
  comments — the runbook is the announcement.
- **The two-phase tail restructure** is the chunk's subtlest diff
  (the `Arc`-ownership trap in §5). Stage 3 should re-derive the
  ownership argument: no `Arc` clone may exist at
  `shutdown_from_arc`'s call, or every clean drain turns
  `CoreStillShared`.
- **Retained-watch semantics** carry escalations across phase
  boundaries; the plan handles them explicitly (phase (i) arm + the
  post-A `take_escalation` + phase (iii)). A missed path would show
  up as a swallowed escalation — T11/T12/T13 pin the observable
  outcomes.
- **Degraded exits 1 may surface latent budget hits** that always
  happened silently (kiosk units with slow disks could newly
  *restart* on a degraded drain under `Restart=on-failure`-style
  supervision). Accepted: that is the contract working as intended
  (F3's invisibility was the bug); the runbook documents the
  meaning.
- **Process-tier tests are the suite's slowest** (wiremock parks up
  to the budget; T11 holds a 20 s ceiling but exits at the second
  signal). They stay unix-gated and few (T9–T14, six spawns).

## 12. Diff estimate & commit plan

≈ 700–900 lines including tests and docs — inside the dispatch's
S/M:

| Commit | Content | ≈ lines |
|---|---|---|
| 1 `feat(signals): the daemon lifecycle watcher` | `Event::StopRequest`, `Signal` pub, `install_lifecycle_watcher` + `LifecycleSignals`, T1–T3 | ~300 |
| 2 `feat(bootstrap): the degraded-drain outcome and the bounded salvage` | `budget_hit`/`degraded()`/`exit_if_degraded`/`salvage_flush`/`abort_connectors`, T4–T7 | ~220 |
| 3 `feat(app): the drain joins report their budget` | both `join -> bool`, T8 | ~90 |
| 4 `feat(app): the daemon ladder and the honest exit codes` | `main.rs`/`ui.rs` migrations (§7), T9–T14 | ~380 |
| 5 `docs(runbook): the exit-code contract and supervision mapping` | runbook §9 + both contrib units | ~110 |
| 6 `docs(changelog): the daemon ladder round entry` | CHANGELOG Unreleased | ~15 |

Stage 2 runs the full gate suite (AGENTS §10) through
`scripts/run-quality-gates.sh` before handoff; stage 3 closes
#162/#471 with the PR.
