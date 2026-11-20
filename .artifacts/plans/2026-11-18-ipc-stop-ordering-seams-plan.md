# IPC Stop Ordering + Mutation Survivors — Chunk N-D2 Plan

- **Date**: 2026-11-18
- **Kind**: stage-1 implementation plan (post-round-9 parallel round,
  dispatch plan `.artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md`
  §1/§6, chunk N-D2).
- **Branch**: `fix/ipc-stop-ordering-seams` (worktree
  `../taskboard-WT-fix-ipc-stop-ordering-seams`, branched off
  `origin/main` @ `226952f1` — includes #966/#973/#974).
- **Covers**: #886 (graceful-stop ordering — `daemon:false` client-visible
  before the instance lock releases) + #917 (join `is_finished`-guard
  mutation survivor) + #902 (try_connect error-class-guard survivor).
- **Scope crates**: `taskboard-app` only (`src/ipc/{server,client}.rs`,
  `src/lockfile.rs`, `src/main.rs`, and the tests in
  `crates/taskboard-app/tests/ipc.rs` + inline test modules). No other
  crate is touched; no protocol or wire change.
- **UX coverage**: journey-exempt — the only observable deltas are error
  *ordering*/latency on an existing typed failure class
  (`instance-locked` stays the envelope text; no new verb, view, badge,
  or setting). The e1 contract wording is updated in-code, not re-designed.
- **Status**: READY. Every open question resolved as D-1…D-8 below.

## 1. Landed base (read and verified)

- The window #886 names, end to end:
  - `IpcServerHandle::join`
    (`crates/taskboard-app/src/ipc/server.rs:129-170`) drains
    accept+dispatcher under a budget, then **removes the socket
    artifact** (`tokio::fs::remove_file(sidecar)`) as its last step —
    this is what turns `try_connect` into `None` (⇒ `daemon: false` on
    `status`, ⇒ in-process fallback for `IpcOrLocal` verbs).
  - The instance lock is `App._lock: InstanceLock`
    (`crates/taskboard-app/src/bootstrap.rs:616,803`), acquired inside
    `bootstrap` (line 1250) and released only when the `App` is dropped
    — i.e. inside `bootstrap::shutdown_from_arc` at the end of
    `drain_and_shutdown`
    (`crates/taskboard-app/src/main.rs:608-680`, Phase B), after
    `server.join(shutdown_timeout)` (Phase A) has already removed the
    artifact. The window is therefore *the entire Phase B owned
    shutdown* (engine flush, actor joins) — up to the
    `shutdown_timeout` escalation ladder — during which a fallback
    one-shot verb boots in-process and fails typed `instance-locked`
    against the draining daemon, violating cli_ipc e1's "fallback works
    after `daemon: false`" contract.
  - Fallback sites (all `try_connect → None → in-process boot`):
    `main.rs:139-165` (the `IpcOrLocal` transport arm),
    `commands/logout.rs:164`, `commands/connect.rs:665`;
    `main.rs:356` (`status`) renders `daemon: false` on `None`;
    `main.rs:399` (`daemon stop`) reports typed `NoDaemon`.
- The join guard (#917): `server.rs:145-152` — at budget hit both
  handles are `abort()`ed *before* the `is_finished` guards; the guarded
  `await` only reaps. The C7 report
  (`.artifacts/reports/2026-11-17-cargo-mutants-taskboard-app-report.md`
  §2 cluster D) already establishes **provable-equivalence**: `ip12`
  (`tests/ipc.rs:476`, the 300 ms-budget silent-peer test) passes with
  the mutant applied — the inverted guard first-polls an
  aborted/completed `JoinHandle` (a join error, swallowed) or skips the
  reaping of an already-aborted task (terminates regardless).
- The try_connect guard (#902): `client.rs:130-160` — `NotFound |
  PermissionDenied` fall through immediately; *every other* kind
  (ConnectionRefused included, #570) retries to the `CONNECT_BUDGET`
  deadline (1 s), then `None`. Constants: `CONNECT_BUDGET` (1 s),
  `CONNECT_RETRY` (50 ms) at `client.rs:39-44`. The deadline uses
  **`std::time::Instant`** (`client.rs:27,134,152`) — *not* pauseable.
  Existing tests: `a_refused_socket_that_starts_serving_inside_the_budget_connects`
  already uses `#[tokio::test(start_paused = true)]`; the two
  budget-consumption tests (`client.rs:598-627, 645-666`) assert real
  wall-clock elapsed ranges — clock-discipline debt this chunk retires
  since it owns the seam.
- The tail tests: `tests/ipc.rs` `ip19` (line 1026) and `ip20`
  (line 1048) replicate the drain tail piecewise (`join` +
  `shutdown_from_arc`) — the pattern the ordering pin rides. `j1`
  (line 1100) asserts `!sidecar.exists()` *right after `join`* — this
  assertion moves with the fix (§3 D-3). `ip12` (line 476) asserts
  rebinding after a budget-hit join — still valid, its comment updates
  (stale-artifact recovery is `bind_with_recovery`'s job, §3 D-4).

## 2. Decisions

### D-1 (#886 ordering semantics) — the artifact transition moves *after* the lock release (issue candidate (a))

`daemon stop`'s client-visible "the daemon is gone" signal — the socket
artifact's removal — must **follow** the instance-lock release, never
precede it. New invariant, pinned by test:

> artifact absent ⇒ instance lock free ⇒ in-process fallback boot is
> safe.

Concretely:

1. `IpcServerHandle::join` no longer removes the sidecar. It returns
   the not-yet-removed artifact path instead of dropping it silently —
   `pub async fn join(self, drain_budget: Duration) -> JoinOutcome`
   where `JoinOutcome { budget_hit: bool, socket_artifact:
   Option<PathBuf> }` (unix `Some`, non-unix `None`). `join`'s current
   callers all discard the return (`let _ = handle.join(...)`) — the
   `#[must_use]` budget-hit fact rides `JoinOutcome.budget_hit`.
2. `drain_and_shutdown` (`main.rs`) takes the artifact out of the join
   outcome and removes it **after** `shutdown_from_arc` completes
   (the `App` — and with it the lock — is consumed there), via a small
   pub helper `taskboard_app::ipc::remove_socket_artifact(Option<&Path>)`
   (best-effort `tokio::fs::remove_file`, log line preserved).
   Escalated exits (`process::exit` arms) skip the removal: a stale
   artifact + dead process is exactly the crash shape
   `bind_with_recovery` already heals on the next boot (§D-6 for the
   client-side handling of that shape).
3. Client-observed timeline after the fix (the contract, stated):
   - during Phase A drain: artifact present, listener live — verbs
     connect and are served; `daemon stop` acks.
   - after the drain resolves but before process exit (Phase B):
     artifact present, listener dead — connects answer
     `ConnectionRefused`; `try_connect` retries within the connect
     budget (#570 semantics, unchanged), expires to `None`.
   - after lock release + artifact removal: no artifact — `NotFound`
     ⇒ immediate `None` ⇒ `daemon: false` / fallback. Fallback boots
     only in this state, where the lock is provably free.

**Why not candidate (b)** (typed `stopping` state clients wait on): it
adds a wire state and a second source of truth for "draining" that the
artifact presence already expresses; (a) plus the existing #570
refusal-retry achieves the bounded-wait behavior without any protocol
addition. **Why not (c)** (document the window): the dispatch
acceptance demands a pinned ordering fix; the window is a real contract
violation, not a limitation.

**Pinned test — failure mode under the old order**: new
`tests/ipc.rs` test `the_socket_artifact_outlives_the_drain_until_the_lock_releases`
(§3 D-3): idle `served_with_core` server, `join(budget)` returns
`JoinOutcome` — assert `socket_artifact` is `Some` **and the file still
exists**. Under the old order `join` removed the file and no
`JoinOutcome` existed, so the test is red (compile-shape change makes
the break structural, the `exists()` assertion makes it behavioral).

### D-2 (#886 client-side) — the fallback gains an honest lock probe; no new error variant

During the residual refused-expiry window (Phase B longer than
`CONNECT_BUDGET`), a fallback verb would boot in-process and fail
`instance-locked` with a doomed bootstrap. Decision: the fallback sites
consult the truth directly instead of discovering it by a failed boot:

1. New `lockfile::lock_is_held(db_path: &Path) -> bool` — non-mutating
   probe: open `<db>.lock` and attempt a **shared** (`read`) fd-lock;
   `WouldBlock` ⇒ an exclusive holder is alive ⇒ `true`. Missing file /
   any open error ⇒ `false` (treat as free: the next boot recreates).
   This beats `holder_pid` (best-effort, stale after a crash): the
   flock *is* the truth, and a read attempt mutates nothing (no pid
   overwrite — `InstanceLock::acquire`'s create-without-truncate
   subtlety is untouched).
2. New `client::fallback_blocked_by_lock(db_path) -> bool` — the one
   choke point: `ipc::socket_sidecar(db_path).exists() &&
   lockfile::lock_is_held(db_path)`. (Artifact check first: absent ⇒
   free by the D-1 invariant, never probe.)
3. The three fallback sites (`main.rs` IpcOrLocal transport,
   `logout.rs:164`, `connect.rs:665`) consult it on `try_connect → None`
   and, when blocked, report the **existing** typed
   `CommandError::Bootstrap(Box::new(BootstrapError::InstanceLocked {
   db_path }))` (envelope text unchanged: `instance-locked`) instead of
   booting. No new `CommandError` variant, no `report`/exit-code
   change, no text assertions anywhere — the pin asserts the typed
   variant.
4. `status` (IpcOnly) keeps its current semantics: during the window it
   reports `daemon: false` + the lock-holder pid (documented
   `config.rs:898` behavior; "no daemon is *serving*" is honest). The
   e1 contract wording sharpens to: *fallback one-shot verbs after
   `daemon: false` either succeed (lock free) or report typed
   `instance-locked` naming the holder (drain window) — never a doomed
   boot race*.

### D-3 (tests for D-1/D-2)

All deterministic by construction — no sleeps, no wall-clock reads:

- `tests/ipc.rs` `the_socket_artifact_outlives_the_drain_until_the_lock_releases`:
  served_with_core → `join(DRAIN_BUDGET)` → assert `budget_hit == false`
  + `socket_artifact.is_some()` + `sidecar.exists()`; then
  `shutdown_from_arc(app)` (lock released) →
  `ipc::remove_socket_artifact` → assert `!sidecar.exists()`.
  Red under the old order (D-1). Lock-held side: while the test's
  `app` is alive, `lockfile::lock_is_held(&db)` is `true` — asserted
  between join and shutdown, dropped after.
- `lockfile.rs` unit tests (inline): held-in-process ⇒ `true`; after
  `InstanceLock` drop ⇒ `false`; missing lock file ⇒ `false`.
- `client.rs` unit test for `fallback_blocked_by_lock`: artifact
  present + lock acquired ⇒ `true`; artifact absent ⇒ `false`
  (short-circuit, never opens the lock file); artifact present + lock
  free (stale shape) ⇒ `false`.
- `j1` (`tests/ipc.rs:1100`): its post-join `!sidecar.exists()`
  assertion relocates into the D-3 ordering pin; j1 keeps the
  drain-abort and typed-transport assertions and consumes
  `JoinOutcome` structurally.
- `ip12`: comment updated (artifact outlives the drain; rebinding works
  through `bind_with_recovery`'s stale-artifact heal) — behavior and
  assertions unchanged, not weakened.

### D-4 (#902 latency seam) — pauseable deadline, not a clock object

The seam is the smallest one that makes the guard's *behavior*
(not-found ⇒ zero retries; anything else ⇒ bounded retries) observable
deterministically:

1. `try_connect`'s deadline switches from `std::time::Instant` to
   `tokio::time::Instant` (`client.rs:134,152`). Real-time semantics
   are identical (same constants, same comparison); paused-time tests
   can now drive the budget deterministically. This *is* the injected
   latency seam per testing_strategy §6.1–6.2 — no
   `Arc<dyn Clock>` parameter is warranted for a free function with a
   single call shape (A1 posture: no machinery the shape doesn't
   demand).
2. Killer test (inline, `#[cfg(unix)] #[tokio::test(start_paused = true)]`)
   `a_missing_socket_falls_through_without_consuming_the_budget`:
   no artifact on disk; `tokio::spawn(try_connect(&db))`; a handful of
   `tokio::task::yield_now().await` (current-thread runtime — the
   first connect attempt is a real immediate ENOENT, no timer involved);
   assert `task.is_finished()` and the outcome is `None`.
   Under the guard→false mutant the task parks on
   `tokio::time::sleep(CONNECT_RETRY)` against the paused clock and
   never finishes — the mutant is dead. (Round law (c): the assertion
   is the zero-retry *behavior*, not the guard's syntax.)
3. The two wall-clock budget tests (`client.rs:598-627` ambiguous
   ELOOP, `645-666` refused-throughout) convert to
   `start_paused = true`: spawn `try_connect`, drive
   `tokio::time::advance(CONNECT_BUDGET).await` (auto-advance wakeups),
   assert `None` after the advance and — for the refused/ambiguous
   arms — that the task is *not* finished before the budget elapses
   (`advance(CONNECT_BUDGET - CONNECT_RETRY)` first, assert not
   finished, then complete the advance). Same semantics, strictly
   stronger determinism: the `elapsed >= BUDGET` floor becomes an exact
   driven fact and the `elapsed < BUDGET + 5 s` ceiling disappears
   (it was a flake window, not a contract — AGENTS §5 litmus: if the
   code got faster the old test still passed the floor; the new test
   pins the exact budget shape). This is a *strengthening* rewrite of
   correct tests, not a weakening.

### D-5 (#917 disposition) — provable-equivalence recorded; no behavior change

The C7 report's cluster-D evidence stands: at the guard, both handles
have been `abort()`ed, so the inverted/skipped guard first-polls a
completed handle (swallowed join error) or skips reaping an
already-cancelled task — no observable state differs, and `ip12` (the
only test that reaches the branch) is green under the mutant. A test
that kills it would have to assert an internal completion order the
guard does not expose (any Drop-guard/oneshot instrumentation races
with the runtime's async abort-drop), i.e. a tautology or a flake —
round law (a) forbids both.

Decision: **kill nothing, record everything** — the dispatch acceptance
explicitly allows "an equivalent argument recorded". Deliverables:

1. A comment block at the `server.rs` guard citing the equivalence
   (abort-before-guard; reaping is a courtesy poll, never behavior)
   and pointing at the C7 report §2 cluster D.
2. The same argument recorded on #917 as a comment (stage 2/3), so the
   issue carries the disposition and does not look forgotten.
3. The drain-budget seam the issue asks for **already exists** —
   `join(self, drain_budget: Duration)` is a parameter; `ip12` already
   drives the budget-hit path with a silent peer at a 300 ms budget
   (an injected budget, not a sleep — law (b) compliant). No new
   harness is built; the chunk's D-1 work touches `join`'s return
   shape only, and the ordering pin (D-3) re-exercises the
   budget-hit-free path.
4. Deviation note: dispatch §6 lists #917 as "killed by tests"; this
   plan dispositionally amends that to the acceptance clause's
   equivalent-argument route, with the C7 report as the evidence base.
   Stage 3 reviews this as the recorded rationale, not as an
   unimplemented item.

### D-6 (stale artifacts vs the lock probe)

Post-crash the artifact exists with a free lock: `fallback_blocked_by_lock`
returns `false` (flock probe, not pid text), the fallback boots, and
`bind_with_recovery` heals the stale artifact on the next daemon boot —
unchanged behavior, now explicit. A *live* daemon always holds the
flock ⇒ `true`. No pid-parse dependency anywhere in the new path.

### D-7 (non-unix)

`socket_artifact: Option<PathBuf>` is `None` on non-unix (mirroring
`spawn_server`'s sidecar `None`); `remove_socket_artifact` no-ops on
`None`; `fallback_blocked_by_lock` on non-unix: no artifact ⇒ `false`.
No `#[cfg]` proliferation beyond what `spawn_server` already carries.

### D-8 (escalated exit leaves the artifact)

An escalated exit (`process::exit` in `escalate_exit`/`forced_exit`)
skips artifact removal — deliberate: escalation means the drain
misbehaved; the stale artifact + free lock is the crash shape the next
boot heals, and the fallback probe stays correct (lock freed by process
death). Documented in the `remove_socket_artifact` doc comment. No
change to the escalation ladder.

## 3. Implementation order (TDD)

1. **Red under the old order** (D-3 pin, written first):
   `the_socket_artifact_outlives_the_drain_until_the_lock_releases` —
   initially does not compile against the current `join` (the shape
   change is part of the fix); with only the `JoinOutcome` return
   added but removal still inside `join`, the `sidecar.exists()`
   assertion is red. Land the `join` change + tail removal together;
   the test goes green only with the full D-1 reorder.
2. `lockfile::lock_is_held` + its unit tests; then
   `client::fallback_blocked_by_lock` + tests (green-first helpers).
3. Wire the three fallback sites (D-2.3) — the typed-`instance-locked`
   outcome is asserted via `matches!` in a `cli_ipc`-style integration
   test if a hermetic harness can hold a live lock across an
   `exec_once`-shaped call; otherwise the helper unit tests + the
   D-3 pin carry the contract (the wiring is a three-line conditional;
   the review stage re-checks it against D-2.3 verbatim).
4. #902: switch `Instant` → `tokio::time::Instant`; add the killer
   test (D-4.2); convert the two wall-clock tests (D-4.3). Confirm the
   killer by reverting the guard mentally (guard→false ⇒ the
   `is_finished` assertion hangs/fails on the paused clock).
5. #917: guard comment + issue comment (D-5).
6. `j1`/`ip12` adjustments (D-3); full §10 gate via
   `scripts/run-quality-gates.sh` in the worktree.

## 4. Risks

- **R-1 (Phase B regression surface)**: moving artifact removal into
  `drain_and_shutdown` touches the daemon tail every lifecycle enters
  (daemon, kiosk appliance). Mitigation: the removal is a single
  best-effort call after `shutdown_from_arc`; `ip20`/`ip19` exercise
  the tail shape; the D-3 pin covers the new ordering.
- **R-2 (j1/ip12 churn)**: existing tests assert the old removal point;
  they are updated per D-3, assertions preserved or strengthened — no
  weakening (law (c)); the stage-3 review diffs them explicitly.
- **R-3 (paused-clock conversion of client.rs tests)**: std→tokio
  Instant touches a hot client path; behavior identical in real time
  (same constants). The refused-then-served paused test already exists
  as the pattern proof.
- **R-4 (#917 under-delivery optics)**: D-5 disposes rather than kills;
  the plan records the C7 evidence and the acceptance clause that
  authorizes it. If the reviewer disagrees, the fallback is a
  `#[cfg(test)]` injectable-handles harness — explicitly out of scope
  here (racy assertions, D-5).
- **R-5 (fallback site coverage)**: three sites must adopt the probe;
  a missed one keeps the doomed-boot race on that verb only. Mitigated
  by the shared helper being the only compile-shaped route to
  `Bootstrap(InstanceLocked)` without a boot — the review greps all
  `try_connect → None` arms.

## 5. Estimated diff

~120 production lines (`server.rs` ~20, `main.rs` ~25, `lockfile.rs`
~35, `client.rs` ~10, `logout.rs`/`connect.rs` ~10) + ~180 test lines
(new pins + paused-clock conversions + unit tests), all in
`taskboard-app`. Two files of test churn (`tests/ipc.rs`, inline
`client.rs` tests).

## 6. Out of scope (backlog, not blocking)

- `status`'s drain-window rendering (D-2.4 keeps documented semantics).
- Any wire-level `stopping` state (#886 candidate (b) — rejected for
  now; reopen only if the drain window grows past one budget).
- The C7 `commands/` mutation remainder (#327).

## Amendment (2026-11-20, #994) — the fallback-site count is corrected

D-2.3 (and the echoes at R-5 and at implementation step 3, "the wiring
is a three-line conditional") over-counted the fallback sites: **the
single site that can boot a fallback daemon is the `main.rs` IpcOrLocal
transport arm**, which probes `client::fallback_blocked_by_lock` and
exits typed `Bootstrap(InstanceLocked)` on a blocked fallback (landed
as `exit_blocked_fallback` on this plan's branch). The other two sites
the step named — `logout.rs` `nudge_daemon` and `connect.rs`
`nudge_daemon_attach` — are **best-effort nudges, not fallback sites**:
on `try_connect → None` they degrade to the honest `NoDaemon` /
`map_attach_nudge` outcome within their ~1 s budgets; no in-process
bootstrap exists on those paths, so there is nothing for the lock probe
to gate, and their contract (#570 semantics) is never to fail the
logout/connect ceremony. The D-2.3 acceptance clause (typed
`instance-locked`, never a doomed boot race) remains satisfied at the
one real site; the review grep of `try_connect → None` arms closes with
the nudge arms recorded as no-bootstrap by design. Per-site
disposition: `main.rs` — plan right, code right, no change;
`logout.rs` / `connect.rs` — code right, plan wrong, this amendment
corrects the plan.

Considered and closed alongside (#994's optional re-examination): a
distinct `NudgeOutcome` variant for "drain window hit" was **rejected**
— it would add a wire-adjacent variant whose only consumer is a warning
upgrade, while the nudge's contract is explicitly never-to-fail and its
budget burn lands on the honest `NoDaemon`. If a future UX round wants
the distinction, it is a fresh journey-gated decision, not a
ride-along. Related: the wire-level typed drain fact for `status` is
#991 (landed additively as `StatusView.draining`; the human-readable
line is backlog #1111).
