# Actor Teardown Investigation — 2026-11-19 (chunk R2, #1071)

- **Branch**: `test/shutdown-budget` (base `6bd0f4dd`), stage 2.
- **Method**: plan `2026-11-19-shutdown-budget-plan.md` §D6 — a
  `#[ignore]`d diagnostic harness
  (`cli_verbs::diag_teardown_who_burns_the_shutdown_budget`,
  commit `7040e4d9`) re-pins the ORIGINAL 10 s budget, boots the core
  over wiremock, and installs a tracing subscriber so the
  per-join/per-phase spans are visible; cross-checked by re-running a
  real slow verb test (`v18_stack_add_scopes…`) with the subscriber
  added in an uncommitted experiment (reverted after the run).
  Command form: `RUST_LOG=info cargo test -p taskboard-app --test
  cli_verbs <name> -- --nocapture --test-threads=1`.

## Headline verdict

**No actor burns the teardown budget.** Zero
`"join timed out; aborting"` lines in any instrumented run. The
`shutdown()` tail completes in ~1 ms wall-clock once it starts (see
the traced sequence below). The 10 s quantization the runtime report
measured is **boot-time, not teardown**:

```
22:59:34.030  WARN capabilities probe failed: deriving conservative feature flags
22:59:44.088  WARN boot pull did not settle in time; serving local data   ← 10.06 s gap
22:59:44.108  INFO  shutdown: flushing the engine
22:59:44.108  INFO  shutdown: broadcasting Shutdown
22:59:44.108  INFO  sync actor stopping: connector detached                (sync actor: <100 µs)
22:59:44.108  INFO  state engine entering shutdown drain
22:59:44.109  INFO  sync report channel closed; drain complete
22:59:44.108  INFO  shutdown: stopping the storage actor
22:59:44.109  INFO  storage actor stopped                                  (storage: ~0.5 ms)
22:59:44.109  INFO  shutdown complete actor_aborted=false
```

The burn site is `wait_for_boot_pull`
(`crates/taskboard-app/src/bootstrap.rs:1741–1768`): its settle
predicate accepts `Syncing→settled`, `last_success.is_some()`,
`Failed`/`Offline`, or a board-less db — but the aggregate phase of a
live board whose sync row is still the default (`Idle`,
`last_success: None`) matches none of them, so the deadline is the
only exit. In the wiremock worlds of the slow verb tests (`two_boards`
mounts an empty boards list; locally created boards have no remote
binding), every boot cycle provably has nothing to settle, and the
test pays `shutdown_timeout_secs` **per verb invocation**: v18
(6 invocations) burned 5 × 10.05 s = 50.6 s, reproducing the runtime
report's 10 s multiples exactly.

**Answer to the commissioning question**: the 10 s budget is *not* a
safety margin over a live stall, and *not* a teardown inefficiency —
it is a boot-pull grace window whose predicate cannot recognize the
"cycle ran, nothing to settle" state, so it always runs to the
deadline in that shape of world. This is production-relevant (a real
`taskboard add` with a configured connector and a never-synced board
pays 10 s at boot), armed as **#1082**. Two further findings fell out
of the gate sweep: the boot-failure unwind's sender leaks (fixed in
scope, commit `8230d259`) and the files-sync cycle pinning the drain
(armed as **#1085**).

## Per-actor findings (the six inventoried join sites)

| # | Site (join) | Evidence | Verdict | Recommendation (exactly one) |
|---|---|---|---|---|
| 1 | Sync actor via `stop_actor` (`bootstrap.rs:1024`, `SyncCommand::Detach` → immediate `return` in the select loop, `actor.rs:289`) | `"sync actor stopping: connector detached"` lands within ~100 µs of the Shutdown broadcast in every traced run; D6's hypothesis 5 (a parked cycle out-waiting the graceful detach) is **refuted** — the wiremock cycles complete in ms, and Detach returns even mid-world | Prompt | No action — the graceful-detach path is correct; no code change |
| 2 | Engine flush (`flush_bounded`, `bootstrap.rs:2701`) + engine join | `"state engine entering shutdown drain"` → `"sync report channel closed; drain complete"` within 1 ms in every normal-shutdown trace; the ONLY timed-out joins were the boot-failure unwind's, caused by the unwind's own sender leaks (fixed, `8230d259`) | Prompt after the fix | Fixed in scope (`8230d259`) — see §second finding |
| 3 | Storage actor join (`bootstrap.rs:1451` tail) | `"storage actor stopped"` within ~0.5 ms of the drop | Prompt | No action |
| 4 | Files-sync join (`files_sync_join`, `bootstrap.rs:879`) | Joins prompt when idle, but an in-flight cycle at shutdown pins the whole drain (the 30.09 s trace: engine, escalation AND files-sync joins all timed out behind one parked cycle) | Stalls under a parked cycle | Armed: **#1085** (design decision: cancel-on-shutdown vs join-order vs drain exclusion) |
| 5 | Web server joins (`main.rs:663/671`) | Exercised only by `web_dashboard`/daemon tests; `daemon_signals.rs:52` pins the drain-wedge contract deliberately (D3, untouched) | Not implicated | No action — the contract pin stands |
| 6 | Authority / notifier / escalation / focus joins (`bootstrap.rs:853–905`) | `"shutdown complete actor_aborted=false"` — the aggregate of all six bounded joins reports no abort in normal shutdown; the escalation join timed out only in the failure unwind (frame-held `EngineHandle` clone — fixed, `8230d259`) and behind the files-sync pin (#1085) | Prompt after the fixes | Fixed in scope (`8230d259`); residual #1085 |

The one *non-actor* seam that does burn: the **boot-pull settle wait**
(cited above) → recommendation: **armed `backlog:` issue #1082** (the
typed skip-cycle fact needs a design decision; the fixture-side 1 s
override already removes the test-side cost, so the fix is neither
forced nor mechanically small — it fails the plan's
"conclusive-and-small" bar on the fix half).

## #968 disposition (plan D5)

- a12 fix landed: the 10 000-yield bounded poll is replaced by the
  join-await (commit `e5958aa3`, `actor_wiremock.rs::a12`).
- D5's closure bar requires the investigation to produce a root cause
  in the *same seam* (shutdown-path latency under load). The
  investigation **refuted** shutdown-path latency entirely (zero join
  timeouts; teardown ~1 ms) — the quantization is boot-time, a
  different seam. Condition 2 of D5 is therefore **not met**.
- Disposition: #968 **stays OPEN** under `flaky-test`+`watchlist`,
  with a dated evidence comment (the deterministic test-side fix
  removes the poll-lottery mechanism; the production root-cause link
  the issue hypothesized does not exist per the evidence above).
  Stage 3 re-checks.

## Re-measurement (runtime-report method: nextest `-j 4`, per-test
`exec_time` via `--message-format libtest-json-plus`)

Final gate run on this branch (4995 tests, 0 failed):
**suite wall 350 s → 169.5 s (−52 %)**; **41 tests over 5 s → 13**.

| Test | Before | After | Verdict |
|---|---|---|---|
| `cli_verbs v15–v18` family (5 × 10 s burns) | 40–50.6 s | 5.1 s | fixed — 5 verb invocations × 1 s boot-settle residual |
| `dispatch_routing` failing-credential family (16) | 10.1 s | 0.1–0.4 s | fixed — boot-failure unwind sender leaks (`8230d259`) |
| `dispatch_routing::sync_routes_the_timeout…` | ~10 s | 6.1 s | D3 pin: real 5 s wiremock delay + 1 s teardown — contract, untouched |
| `cli_verbs destruct_refusal…` | 60.6 s | 6.1 s | fixed — 6 × 1 s residual |
| `bootstrap b5` failure paths | 10–30 s | 2.2 s | fixed — `BOOT_FAILURE_JOIN_BUDGET` 2 s |
| `ipc::ip16_attach_from_persisted_state…` | 60.3 s | 60.25 s | D3 pin: explicit 60 s dead-identity detach budget — untouched |
| `enabled_files_sync_spawns…` | 30.1 s | 30.07 s | **armed #1085** (files-sync cycle pins the drain) |
| `status::files_sync_is_present_iff…` | — | 7.7 s | same #1085 family |
| `cli_tty tty_ctrl_c_during_a_stalled_line…` | — | 10.1 s | named residual: subprocess fixtures' explicit 5 s budgets are D2-untouchable; revisit after #1082/#1085 (plan §4) |
| `it_docker v0_1_x…` | — | 9.9 s | named: docker-tier, explicit budgets |
| `storage-sqlite roundtrip` | 14.1 s | 13.7 s | sqlite crate's own joins — out of this chunk's inventory; record only |
| `domain pipeline proptest` | 75 s | 80 s | known R3 chunk (load variance) |

D3's pins (30 s linger, 60 s dead-identity, 5 s wiremock delays,
drain wedge) are untouched and green. The 1 s fixture residual is the
boot-settle wait now paying 1 s per verb invocation — the production
cost is #1082.

## Second finding: the boot-failure unwind burned TWO budgets (fixed
in scope)

The gate's own sweep surfaced a second 10 s-quantized family: 16
`dispatch_routing::*_routes_a_failed_credential_load_to_exit_one`
tests at 10.1 s each (a boot-FAILURE path, invisible to the
teardown-oriented inventory). Instrumented:

```
bootstrap failed: connector module did not start  err=Credential(NotFound)
state engine entering shutdown drain
join timed out; aborting actor="state engine"        (budget 1)
join timed out; aborting actor="escalation driver"   (budget 2)
```

`unwind_after_module_failure` (`bootstrap.rs:1625–1668`) leaked two
senders, defeating the engine's drain condition
(`draining && !reports_open && !user_data_open`,
`taskboard-state/src/actor.rs:475`):

1. the user-data lane's sender stayed alive past the join;
2. the call site passed `engine.clone()`, so the bootstrap frame's
   own `EngineHandle` kept the signal broadcast open — the escalation
   driver parked forever instead of seeing `Closed`.

**Fixed in scope** (commit `8230d259`, 17 lines: pass `user_data_tx`
and drop it before the joins; pass the handle by value) — the
diagnosis was conclusive (typed drain condition cited) and the fix
mechanical. Measured effect: the failing-credential routing family
10.1 s → 0.1–0.4 s per test.

## Third finding: the files-sync actor pins the drain across an
in-flight cycle (armed: #1085)

`enabled_files_sync_spawns_the_actor_and_the_status_watch` measured
30.09 s = 3 × 10 s (engine, escalation, files-sync joins each timing
out). The files-sync actor (`taskboard-sync-files-nextcloud/src/
actor.rs:159`, biased select) checks `Shutdown` **only between
cycles**; its first cycle fires immediately at boot and, parked on
the transport, never reaches the Shutdown arm. It holds the engine's
`user_data` sender, so the engine drain cannot complete — the whole
shutdown tail times out behind one parked cycle. The engine join
precedes the files-sync join in `App::shutdown`, so ANY in-flight
files cycle pins the drain. Fix shapes (cancel-on-shutdown /
reorder / drain exclusion) each change a contract → **armed as
#1085**, exactly one recommendation: design-decision first.

## Harness note

`diag_teardown_who_burns_the_shutdown_budget` ships `#[ignore]`d
(deliberately excluded from the gate); it needs `RUST_LOG=info` — the
subscriber uses `EnvFilter::from_default_env()`, which is silent at
the default level. Run:

```sh
RUST_LOG=info cargo test -p taskboard-app --test cli_verbs \
  diag_teardown -- --ignored --nocapture
```
