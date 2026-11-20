# Boot-Pull Settle Predicate — Chunk Plan (Round 3, Chunk R2b)

- **Date**: 2026-11-20
- **Chunk**: R2b of backlog-reduction round 3 (dispatch plan §18)
- **Issue**: #1082; branch `fix/boot-pull-settle-predicate`
- **Normative context**: dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  §1/§18; investigation report
  `.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md`
  (headline §); AGENTS §2/§5 (typed outcomes, determinism, no weakening).
- **Size**: S/M. Stages: this plan (stage 1) → implementation (stage 2) →
  review/PR (stage 3).

## 1. Problem (convicted seam)

`wait_for_boot_pull` (`crates/taskboard-app/src/bootstrap.rs`, the
`pub(crate) async fn` around line 1811 on `origin/main`) gives the boot
cycle's first pull a bounded chance to land before a verb reads state. Its
settle predicate accepts:

1. `SyncPhase::Syncing` → anything (in-flight cycle settled),
2. `sync.aggregate().last_success.is_some()`,
3. `SyncPhase::Offline | SyncPhase::Failed { .. }`,
4. a board-less db (`!state.boards.values().any(|b| !b.deleted)`).

The gap: a live board whose sync row is still the default (`Idle`,
`last_success: None`, zero counters) and that carries **no remote
binding** (`Board.remote: None` — every locally created board until a
connector ceremony binds it) matches none of the four. The sync actor's
boot cycle derives an empty owned set for the configured connector and —
per recorded decision 10 — a poll-tick cycle with an empty derived set is
a **silent skip**: no report, no phase transition, nothing on the wire
(`crates/taskboard-sync-nextcloud/src/actor.rs`, `cycle()`'s
`owned.is_empty()` branch). The predicate's deadline (`shutdown_timeout_secs`,
default 10 s) is the only exit: every verb invocation in that shape of
world pays the full budget at boot. Production-relevant (real
`taskboard add` with a configured connector + never-synced board) and
test-relevant (R2's landed 1 s fixture budgets still pay 1 s per verb
invocation as residual).

## 2. Decision D1 — the predicate fix: structural widening (typed state, not a new signal)

**Chosen**: widen the predicate with a *structural nothing-pullable*
check, evaluated from the typed state the predicate already loads:

```rust
// No live board carries a remote binding: no sync actor anywhere can
// pull anything for this world — the boot cycle provably has nothing
// to settle. (Locally created boards are `remote: None` until the
// connector ceremony binds them; their cycle is a silent poll-tick
// skip — decision 10.)
_ if !state
    .boards
    .values()
    .any(|b| !b.deleted && b.remote.is_some()) =>
{
    return;
}
```

Ordering: after the `Syncing`/`last_success`/`Offline|Failed` arms
(those are authoritative outcomes and must keep winning), before or
folding with the existing board-less arm (the board-less check is the
`remote`-less check's degenerate case — one combined arm is acceptable
if the comment states both facts). The deadline remains as the backstop.

**Why not the alternatives** (recorded for stage 3):

- *Typed skipped-cycle report (`SyncReport::Skipped` or report
  `Failed { NoBoard }` on poll ticks too)* — rejected. Decision 10 keeps
  poll-tick skips silent precisely so an empty world never stamps
  `Failed { NoBoard }` rows (UI badge pollution, failed-looks-broken);
  a new report variant is domain + engine fold + wire churn for a fact
  the state already encodes in typed form (`Board.remote`), and it
  violates the A1 type-driven policy's "no new shapes where existing
  typed data suffices" line. The issue's "which fact is authoritative"
  question is answered: **the derived board set's binding facts are the
  authoritative settle fact** — a cycle cannot pull a board that has no
  `remote` binding, so an all-unbound world is settled by construction,
  no signal required.
- *SystemEvent cycle-ran broadcast* — rejected. A second signal
  vocabulary beside the state reads re-introduces an ordering race (the
  broadcast can outrun the engine's report fold), which the predicate
  would then have to re-check against state anyway — the state check
  alone is simpler and deterministic.

**Documented deliberate leniency** (AGENTS §2 A1 pattern): a world whose
only live boards are bound to a *deconfigured* connector (binding
present, no actor owns it) still waits out the deadline — the predicate
cannot distinguish "bound and being synced" from "bound but orphaned"
from state alone, and fabricating connector ownership in the predicate
would duplicate the engine's binding-derivation logic. The deadline
backstop bounds it; if it ever matters in practice it becomes a follow-up
(backlog issue filed, see §6).

## 3. Decision D2 — test shape: paused-clock structural contract pin

New tests in `crates/taskboard-app/tests/core/bootstrap.rs` (the
predicate is an in-process seam; the engine-level integration suite
tests routing only — AGENTS §5 "one behavior, one test, lowest layer"):

1. **The settle test (the contract)**: build an `AppState` with a live
   (`deleted: false`), **unbound** (`remote: None`) board and a default
   sync row; mount it on the engine's shared state; call
   `wait_for_boot_pull(&engine, Duration::from_secs(10))` under
   `tokio::time::pause()`. Assert the wait returns with the virtual
   clock still below the deadline (capture `tokio::time::Instant::now()`
   before/after; assert `elapsed < timeout`). Deterministic by
   construction: no sleeps, no wall clock (AGENTS §5 clock discipline —
   the oracle is the *virtual* deadline, so the paused clock is the
   matching tool). If the predicate regressed, tokio's timer auto-advance
   carries the virtual clock past the deadline inside the poll loop and
   the elapsed assertion fails typed.
2. **The over-widening control**: same fixture but the board **bound**
   (`remote: Some(..)`) and the phase left `Idle`: the wait must NOT
   return early — advance the paused clock past the deadline and assert
   the warn-path exit (virtual elapsed ≥ timeout). This pins that the
   widening fires only on the all-unbound world, never on a world that
   could still receive a pull.
3. **The settled-board control exists implicitly** in the existing
   suite (the `Syncing→settled` / `last_success` arms are already
   covered); do not duplicate (AGENTS §5).

Implementation note for stage 2: if the auto-advance semantics of the
paused runtime make assertion 1 vacuous (the poll loop's 200 ms
`tokio::time::timeout` auto-fires and advances to the deadline
instantaneously even on the fixed predicate), fall back to spawning the
wait as a task and asserting it completes *before* any explicit
`tokio::time::advance` is issued — a `oneshot`/join-handle race that is
still fully deterministic (a broken predicate cannot complete without an
advance; the fixed one completes on the first state load). Record which
variant landed in the stage-2 report.

## 4. Decision D3 — budgets: margin, not load; the 5 s subprocess budgets drop

With the predicate fixed, the budget stops being consumed in the common
verb worlds, so it is pure margin:

- **Keep** R2's landed 1 s in-process fixture default
  (`tests/common/mod.rs:363`, `shutdown_timeout_secs: 1`) — it is now a
  genuine join-safety margin, untouched.
- **Drop** the subprocess config template's 5 s
  (`tests/common/mod.rs:632`, `write_test_config`'s
  `shutdown_timeout_secs = 5` → 1): its sizing was the boot-settle burn
  (issue #1082's named follow-up); the mechanical pass in scope.
- **Audit and drop** any `cli_tty` / `repl` explicit budgets sized
  against the same boot wait (issue #1082's follow-up names the family;
  the 10.1 s `tty_ctrl_c_during_a_stalled_line…` residual in the R2
  re-measurement table is the observable). Grep the tests for explicit
  `shutdown_timeout` / `from_secs(5)` fixtures whose sizing comment
  cites the boot wait; drop those only. **Untouched**: every D3 contract
  pin (the 5 s wiremock delay, 30 s linger, 60 s dead-identity, drain
  wedge), `dispatch_routing`'s real-delay test, `it_docker` budgets —
  those are delays in the system under test, not boot-wait margins
  (never weaken a correct test, AGENTS §5).
- The production default (`default_shutdown_timeout`, config.rs) is
  **unchanged**: it still bounds legitimate slow first pulls against a
  real server; this chunk fixes a misclassification of "nothing to
  settle", not the budget's size.

## 5. Implementation order (stage 2, TDD)

1. Write the two paused-clock tests (§3) against the current predicate —
   test 1 fails (deadline exit), test 2 passes. Commit
   `test(app): the boot-pull settle predicate pins the unbound-world skip`.
2. Widen the predicate (§2, D1) — both tests green. Commit
   `fix(app): the boot-pull settle predicate settles an all-unbound world`.
3. Budget mechanical pass (§4, D3). Commit
   `test(app): the subprocess fixture budgets drop to the 1 s margin`.
4. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
   run --workspace` (background + poll; defer per the flock rule).

Acceptance criteria:

- AC1: a verb-world state with live unbound boards settles the boot wait
  immediately (deterministic paused-clock pin, typed assertion).
- AC2: a bound-board world does not settle early (over-widening control).
- AC3: the R2 re-measurement table's residual family (`cli_verbs`
  v15–v18 at 5.1 s, `destruct_refusal` at 6.1 s) drops to sub-second
  wall time in the gate run (observable, reported, not asserted in CI).
- AC4: no existing pin weakened; the full suite is green.

## 6. Out-of-scope findings → backlog

- The bound-but-deconfigured-connector deadline leniency (§2): filed as
  a `backlog:` issue (performance, p3) — the predicate could consult the
  engine's connector-ownership derivation if it ever matters.
- Issue #1082 stays open for the dispatcher to close at merge (at-merge
  closure R2b → #1082 per §18); this chunk's comment on #1082 records
  the authoritative-fact decision.

## 7. Risks

- **Auto-advance vacuity** in the paused-clock test (§3 note) — fallback
  recorded; either variant is deterministic.
- **Over-widening** silently disabling real boot-pull waits — mitigated
  by the control test (AC2) and arm ordering (outcome arms first).
- **Subprocess budget drops** unmasking a real slow join — acceptable:
  a join needing >1 s in the hermetic world IS the finding (R2's
  precedent comment in `common/mod.rs:356-362`).
