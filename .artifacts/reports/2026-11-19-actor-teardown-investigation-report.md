# Actor Teardown Investigation — 2026-11-19 (chunk R2, #1071)

- **Branch**: `test/shutdown-budget` (base `6bd0f4dd`), stage 2.
- **Method**: plan `2026-11-19-shutdown-budget-plan.md` §D6 — a
  `#[ignore]`d diagnostic harness
  (`cli_verbs::diag_teardown_who_burns_the_shutdown_budget`,
  commit `a59345da`) re-pins the ORIGINAL 10 s budget, boots the core
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
pays 10 s at boot), armed as **#1082**.

## Per-actor findings (the six inventoried join sites)

| # | Site (join) | Evidence | Verdict | Recommendation (exactly one) |
|---|---|---|---|---|
| 1 | Sync actor via `stop_actor` (`bootstrap.rs:1024`, `SyncCommand::Detach` → immediate `return` in the select loop, `actor.rs:289`) | `"sync actor stopping: connector detached"` lands within ~100 µs of the Shutdown broadcast in every traced run; D6's hypothesis 5 (a parked cycle out-waiting the graceful detach) is **refuted** — the wiremock cycles complete in ms, and Detach returns even mid-world | Prompt | No action — the graceful-detach path is correct; no code change |
| 2 | Engine flush (`flush_bounded`, `bootstrap.rs:2701`) + engine join | `"state engine entering shutdown drain"` → `"sync report channel closed; drain complete"` within 1 ms | Prompt | No action |
| 3 | Storage actor join (`bootstrap.rs:1451` tail) | `"storage actor stopped"` within ~0.5 ms of the drop | Prompt | No action |
| 4 | Files-sync join (`files_sync_join`, `bootstrap.rs:879`) | Not in play in the verb tests (feature off in `valid_config`'s world); exits on the same Shutdown broadcast by design | Not exercised | No action in scope; covered by the same Shutdown-broadcast posture the traced joins confirm |
| 5 | Web server joins (`main.rs:663/671`) | Exercised only by `web_dashboard`/daemon tests; `daemon_signals.rs:52` pins the drain-wedge contract deliberately (D3, untouched) | Not implicated | No action — the contract pin stands |
| 6 | Authority / notifier / escalation / focus joins (`bootstrap.rs:853–905`) | `"shutdown complete actor_aborted=false"` — the aggregate of all six bounded joins reports no abort, and the whole tail spans ~1 ms | Prompt | No action |

The one *non-actor* seam that does burn: the **boot-pull settle wait**
(cited above) → recommendation: **armed `backlog:` issue #1082** (the
typed skip-cycle fact needs a design decision; the fixture-side 1 s
override already removes the test-side cost, so the fix is neither
forced nor mechanically small — it fails the plan's
"conclusive-and-small" bar on the fix half).

## #968 disposition (plan D5)

- a12 fix landed: the 10 000-yield bounded poll is replaced by the
  join-await (commit `8c7d3770`, `actor_wiremock.rs::a12`).
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

Recorded by the gate run on this branch (see the table in the final
commit; the runtime report's 41-over-5 s table is the baseline). The
mechanism prediction: the 10 s-quantized family drops to ~1 s per
verb invocation (the boot wait now pays 1 s, not 10 s), and the
boot-failure family to ~2 s (`BOOT_FAILURE_JOIN_BUDGET` = 2 s;
spot-measured: `bootstrap b5` = 2.2 s, was 10–30 s). D3's pins
(30 s linger, 60 s dead-identity, 5 s wiremock delays, drain wedge)
are untouched and still green.

## Harness note

`diag_teardown_who_burns_the_shutdown_budget` ships `#[ignore]`d
(deliberately excluded from the gate); it needs `RUST_LOG=info` — the
subscriber uses `EnvFilter::from_default_env()`, which is silent at
the default level. Run:

```sh
RUST_LOG=info cargo test -p taskboard-app --test cli_verbs \
  diag_teardown -- --ignored --nocapture
```
