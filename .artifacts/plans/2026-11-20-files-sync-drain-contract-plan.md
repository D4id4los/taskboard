# Files-Sync Drain Contract — Options Plan (chunk R2c, round 3)

- **Date**: 2026-11-20
- **Chunk**: R2c of backlog-reduction round 3 (round amendment §18 of
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`)
- **Branch**: `fix/files-sync-drain-contract` (this worktree)
- **Issue**: #1085 (`backlog: files-sync actor holds the engine's
  user-data lane across an in-flight cycle — shutdown drain pins on a
  parked WebDAV read`)
- **Status**: GATE-PAUSED — operator gate. This plan records the three
  fix options with a recommendation; **stage 2 does not start until the
  operator picks one.** No code in this commit.

## 1. The defect (normative base: R2 investigation report, third finding)

`.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md`
(§ "Third finding"), measured trace 30.09 s = 3 × 10 s join budgets:

- The files-sync actor's event loop
  (`crates/taskboard-sync-files-nextcloud/src/actor.rs`, `run()`, the
  biased `tokio::select!`) checks `SystemEvent::Shutdown` **only
  between cycles** — the `select!` arms are the deadline tick and the
  broadcast receiver; while `run_cycle(&transport, &ctx, …)` is
  awaited, neither arm can fire.
- The first cycle fires immediately at boot
  (`deadline = Instant::now()`), so in the unit-test world (and on a
  slow link in production) Shutdown arrives while a cycle is parked on
  the transport (`propfind_devices` is the first request of both the
  attach and the steady cycle — the `ParkOnDevices` test staging,
  `actor.rs` tests, parks exactly there).
- The actor holds `report_tx: mpsc::Sender<UserDataReport>` — a clone
  of the engine's `user_data_tx` (`bootstrap.rs:1364`,
  `spawn_files_sync_actor`'s `report_tx` param) — plus
  `board_reports`. While the actor lives, the engine's drain condition
  `draining && !reports_open && !user_data_open`
  (`crates/taskboard-state/src/actor.rs:475`) can never satisfy
  (`user_data_open` stays true), so the engine join times out, the
  escalation driver's join times out behind it, and the files-sync
  join itself times out because its own loop never reaches the
  Shutdown arm.
- Ordering makes it worse: `App::shutdown` joins the **engine before
  the files-sync actor** (`bootstrap.rs:882` vs `:908`), so *any*
  in-flight files cycle — even a legitimately slow one — pins the
  whole drain for at most the full join budget, thrice over.

Related-but-distinct: #1082 (the boot-pull settle predicate, chunk
R2b, branch `fix/boot-pull-settle-predicate` — **not yet landed on
origin/main at planning time**; this chunk must not depend on it, but
stage 2 should rebase onto it if it lands first — both touch shutdown
teardown tests in `taskboard-app`).

## 2. The three options

Each option changes (or deliberately keeps) a contract; the operator's
pick is the accepted design decision for #1085.

---

### Option (a) — Mid-cycle Shutdown check (recommended)

**Semantics.** The cycle polls a cancellation token at its park
points and unwinds typed. Concretely:

- `SyncContext` gains a private `cancel: tokio_util::sync::CancellationToken`
  (minted in `spawn_files_sync_actor`, or — staying inside the
  approved stack — a `watch::Sender<bool>`/`tokio::sync::Notify`
  equivalent; `tokio-util` is **not** currently a workspace dependency,
  so the house shape is a `watch::channel(false)` the actor's
  Shutdown arm sets to `true`).
- The cycle's **between-request checkpoints** (before
  `propfind_devices` in `attach` and `steady_cycle`, and between the
  per-peer steps in `read_dirty_peers`/`read_peer`) check the token
  and return a new typed outcome — not a `TransportError` (that would
  miscount the failure streak and pollute the backoff curve), but a
  dedicated `CycleOutcome::Aborted` (or a `Cancelled` variant on the
  cycle's internal result) that:
  - does **not** advance `failure_streak`,
  - does **not** touch `last_success`,
  - records `status.last_cycle = Aborted` so the status surface shows
    the cycle ended mid-flight,
  - leaves `LoopState` otherwise untouched — the next boot's cycle
    re-derives everything from the server listing (the sync protocol
    is already crash-safe: every step is individually idempotent, the
    fork alarm + etag guards cover partial writes, and `device.json`
    is load-only at attach).

**Contract change (the real one).** A cycle can now *end mid-flight*.
That requires defining partial-state semantics explicitly:

- *Pull side* (read peers): discarding a half-read peer set is free —
  nothing was written.
- *Publish side* (`publish_appdata`/`rewrite_own_appdata`/
  `write_heartbeat`): the abort checkpoints sit **before** the first
  write of each step and the steps are atomic per-file (guarded PUT),
  so an abort lands either before or after a complete step — never
  mid-file. The etag/fork-alarm machinery already owns the
  crash-between-steps case (it is the same state a power loss leaves).
  The plan pins this: **abort points are only at cycle-step
  boundaries, never inside a transport call** — in-flight TCP finishes
  or the process dies; we do not cancel inside a write.
- *Reports*: an aborted cycle simply sends no `UserDataReport` — the
  engine's drain accepts "this cycle produced no evidence", which is
  exactly the semantics ADR 0008's drain already tolerates from a
  crashed kiosk.

**Risk.** Medium-low. The failure-streak/backoff interaction is the
main trap (an abort must not arm backoff — covered by the typed
outcome); the second trap is the `ParkOnDevices`-style tests asserting
request counts — the abort adds a path where a staged request never
fires (existing tests unaffected; new tests cover the abort path).

**Test shape.**
- Actor-level (`#[tokio::test(start_paused = true)]`, `ParkOnDevices`
  staging): arm the park, send `SystemEvent::Shutdown` while parked,
  release the park → the actor **returns** (join completes), the
  status watch shows `CycleOutcome::Aborted`, `failure_streak` is
  unchanged (assert via the reschedule of the never-taken next cycle
  or a direct `LoopState` observable if exposed), and the engine's
  `user_data_rx` closes.
- App-level (teardown budget family, `bootstrap.rs` tests): with a
  parked files transport, `App::shutdown` completes well inside one
  10 s budget — engine, escalation, and files joins all succeed
  (`actor_aborted=false`), the exact 30 s trace inverted.
- Engine-level: no change (the drain condition is untouched — the
  actor still owns the sender; it just exits promptly).

**Estimate.** ~150–250 lines production (`cycle.rs` checkpoints + one
typed outcome + actor loop wiring) + ~150 lines tests. Size M as
dispatched. One PR.

---

### Option (b) — Cooperative abort via transport cancellation

**Semantics.** The transport itself becomes cancellable: `FilesTransport`
verbs gain a cancellation handle parameter (or the transport is
constructed with one) so the shutdown path trips it and a parked HTTP
call unblocks immediately — cancelling the underlying reqwest future
rather than awaiting its completion.

Two sub-shapes:

- (b1) **Trait-parameter**: every verb takes `&CancellationToken`
  (house shape: `&watch::Receiver<bool>`) and the production
  `WebDAV` client wraps each request future in a
  `tokio::select!` against it. The test fake honors it at park points.
- (b2) **Constructor-injected**: the handle is bound at client
  construction (the `reqwest` per-call `timeout` seam becomes a
  per-call `select`), so the trait signature stays unchanged — but
  then the actor's own token can't reach a transport the *caller*
  built; the bootstrap would have to mint the token, pass it to both
  the client constructor and the actor. This is really (b1) with the
  plumbing moved to construction time; it keeps the trait closed.

**Contract change.** Transport calls gain a cancellation parameter
(b1) or the client construction gains a trip-wire seam (b2). Either
way the *transport* contract changes — the closed verb set's
signatures or the client's constructor — and every `FilesTransport`
implementation (production `client.rs`, `test_support.rs` fake, and
the ~3 hand-rolled test transports like `ParkOnDevices`/`ParkHandle`)
must carry it.

**Semantics for the cycle.** Identical to (a) from the cycle's point
of view — the cancelled call returns a typed `TransportError::
Cancelled` (new variant) which `run_cycle` maps to the same
no-streak, `Aborted`-status unwind. So (b) = (a)'s cycle semantics
plus a deeper seam change.

**Risk.** Medium. Bigger blast radius (trait + client + fake + every
test transport); cancellation *inside* a request reintroduces exactly
the partial-write hazard option (a) deliberately refuses — a
`put_if_match` cancelled mid-request is safe server-side (the etag
guard makes a retry detect it) but it is a new failure class the
transport error enum, the cycle error mapping, and the fork-alarm
disambiguation must all be argued safe against. Payoff over (a):
latency — abort lands without waiting for the in-flight call's own
completion; in practice the join budget (10 s) already covers any
single HTTP call's timeout, so the payoff is near zero against the
real defect.

**Test shape.** Same as (a) plus transport-level tests (each verb
honours the token; a cancelled `put_if_match` maps to the typed
`Cancelled` variant and the cycle does not count it).

**Estimate.** ~300–450 lines production + ~250 lines tests. Size M/L.
One PR, but the widest trait change of the three.

---

### Option (c) — Keep the pin, shrink the exposure (no contract change)

**Semantics.** Document the drain as intentionally *cycle-bounded*:
the engine waits for the in-flight cycle because ADR 0008's drain
wants the last cycle's evidence to land. The defect is then not the
pin but the *unbounded* cycle length; the fix caps the exposure:

- Give each cycle a wall of budget: `run_cycle` is wrapped in a
  `tokio::time::timeout(cycle_budget)` (a config seam, default well
  under the join budget, e.g. 30 s in production, driven to
  milliseconds in tests via the injected clock/paused time), timing
  out into the existing `CycleOutcome::Failed`-adjacent typed
  outcome (again not counting the failure streak — same special-case
  as (a)'s abort).
- Document in `run()` and the engine's drain comment that shutdown
  latency is bounded by `min(cycle_budget, join_budget)`.

Issue #1085's other two cheap shapes belong in this same "no real
contract change" family and are recorded here as evaluated:

- *Join reorder* (files-sync join before engine join): removes the
  cascading timeouts but **not** the pin — the files join itself
  still burns its full budget on a parked cycle, and the engine now
  drains concurrently with a cycle still publishing evidence, i.e.
  the drain-safety property ADR 0008 chose ("the sync actor's exit is
  exactly when the last cycle's evidence has landed") is silently
  weakened. Rejected as a standalone fix.
- *Drain exclusion* (engine ignores `user_data_open` once draining):
  removes the engine's timeout but the files-sync join still times
  out (its loop still never sees Shutdown), and the actor's final
  cycle's reports are dropped unread — evidence loss by construction.
  Rejected: it breaks ADR 0008's stated invariant rather than
  changing it deliberately.

**Risk.** Low to implement, but the teardown cost *stays* (bounded,
not eliminated): a shutdown during a slow cycle still waits up to the
cycle budget, and in the parked-forever test world the budget is what
saves it — i.e. the 30 s trace shrinks to ~1 budget, not to ~0.
Also a new config knob to justify and document.

**Test shape.** App-level teardown test with a parked transport:
shutdown completes within `cycle_budget + epsilon` (deterministic via
paused time), status shows the budget outcome. Actor-level: a cycle
that exceeds the budget is abandoned at the next await point without
advancing the streak.

**Estimate.** ~60–100 lines production + ~100 lines tests. Size S.

---

## 3. Recommendation

**Option (a).** Reasoning:

1. It fixes the actual defect (an unkillable in-flight cycle) with the
   smallest contract delta: the cycle's own outcome vocabulary grows
   one variant; the transport trait, the engine drain, and ADR 0008's
   drain-safety property are untouched.
2. Its partial-state story is the strongest: abort points only at
   step boundaries, per-step idempotence + etag guards already own
   crash-between-steps, so the new "cycle ended mid-flight" contract
   sits *on top of* an existing, proven crash-safety argument rather
   than beside a new one.
3. (b) buys only latency against an in-flight HTTP call, at the price
   of the widest seam change and a new partial-write failure class.
4. (c) leaves the operator-visible teardown cost in place and was
   effectively already evaluated and rejected in #1085's shapes
   (reorder/exclusion weaken ADR 0008 or just move the burn).

Fallback if the operator prefers zero contract change: (c), with the
explicit acceptance that the parked cycle still costs one cycle
budget at teardown.

## 4. Open questions, resolved per option

- *Q: Does an aborted cycle arm the failure backoff?* **No** in (a)
  and (b): the typed abort outcome skips the streak — an abort is not
  a transport failure and must not lengthen the next cadence. In (c)
  the budget outcome likewise skips the streak. Pinned in all three.
- *Q: What does the status surface show?* A dedicated
  `CycleOutcome::Aborted` (a/c) or the budget outcome (c);
  `last_success` untouched; `parked` map unchanged.
- *Q: Does the engine's drain condition change?* **No** in any option.
  The actor still owns `report_tx`; what changes is only how promptly
  it exits (a/b) or how long a cycle may run (c).
- *Q: Does `tokio-util` enter the workspace?* **No** — cancellation
  uses `tokio::sync::watch<bool>` (approved stack; §4 table), not
  `CancellationToken`. Pinned for (a)/(b); moot for (c).
- *Q: Where exactly are (a)'s checkpoints?* Before
  `propfind_devices` in `attach` (`cycle.rs:478`) and in
  `steady_cycle` (`cycle.rs:575`), and between peers in
  `read_dirty_peers`/`read_peer` — never inside or between the
  write-half steps of a publish. Exact list fixed at stage 2 by the
  call graph of `run_cycle`.
- *Q: Shutdown vs `SettingsChanged` inside the abort check?* Only
  `Shutdown` aborts; `SettingsChanged` still waits for the cycle
  (existing doorbell test `a_doorbell_during_a_cycle_applies_at_the_
  next_wake_only` pins this and must keep passing unmodified).
- *Q: Interaction with #1082/R2b?* Independent code paths (boot
  settle predicate vs teardown); ordering: either lands first, the
  later rebases. Noted for the dispatcher.
- *Q: Does (a) need the join reorder too?* No — with prompt actor
  exit the engine's drain completes and every join succeeds in the
  existing order. The reorder stays rejected (see (c)).

## 5. Stage-2 shape (after the operator picks)

- TDD: the actor-level abort test first (red against the pin), then
  the checkpoint wiring, then the app-level teardown-budget test
  inverting the 30 s trace.
- Gate per the dispatch discipline: pre-commit hooks +
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (defer per the flock-deferral rule); the full §10 suite is the
  dispatcher's.
- One commit series on this branch; no push, no PR from stage 2.
- At-merge closure: #1085.
