# [Plan] Flaky Watchlist Dedicated Pass (backlog-triage chunk 12)

- **Date**: 2026-10-26 (file name kept per the dispatch prompt's
  normative path; written a day later).
- **Kind**: chunk plan — stage-1 planning record for chunk 12 of the
  backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §13,
  prompts 12.1–12.3 binding). Test-infra only: two one-field test
  fixtures, one test-local options flip, one assert-message
  enrichment. **Zero production-code diffs** (dispatch 12.2 hard rule;
  see §4 non-goals).
- **Branch / worktree**: `test/flaky-watchlist-pass` /
  `../taskboard-WT-test-flaky-watchlist-pass` (doctor exit 0, 85 G
  free, floor 25 G; machine: 16 cores / 30 G RAM — the same shape the
  original observations happened on).
- **Base**: `6d5507e` = local `main` = fetched `origin/main` tip
  (identical; the literal "off main" holds with no deviation).
- **Normative origins**: AGENTS §5 (determinism, no-weakening,
  "a new test failing can be a bug", no text assertions, no quality
  ceilings) and §7 (flaky watchlist policy: one issue per distinct
  test, dedicated pass, never opportunistic); issues #384, #408,
  #417, #431, #287, #336 (each body normative for its leg's
  observation + suggested direction); dispatch §13 (covers /
  acceptance / stage-2 hard rules / stage-3 trap list).
- **Line numbers below are plan-day references** (base `6d5507e`),
  never load-bearing.

## 0. Gate verification (prompt 12.1's precondition — all open)

The prompt gates the pass on two merges and says to scope to the legs
whose gates are open. **Both gates are open; all six legs are in
scope; nothing is deferred.**

| Gate | Legs | Evidence (verified 2026-10-26) |
|---|---|---|
| Chunk 11 (`refactor/app-audit-p2`) merged | #384, #408, #417 | PR #472 merged 2026-10-26T14:05Z; hoist commit `3362d59` (`test(app): hoist wait_until, Child, and the daemon-spawn helper into tests/common`) verified `git merge-base --is-ancestor` of base `6d5507e`. The hoisted helpers this triage builds on live at `crates/taskboard-app/tests/common/mod.rs` (`wait_until` :480, `Child` :490, 30 s budget). |
| GUI r2 round merged | #431 | All r2 PRs merged: #418 (board surface), #421 (interaction repairs), #423 (actions/context menus), #424 (taskview textfields), #432 (settings form), #439 (connection management), #443 (visual identity), #453 (drag-to-move, 2026-10-25T20:27Z). `gh pr list --state open` shows no r2 branch remaining. |
| (no gate) | #287, #336 | Dispatchable anytime per dispatch §13. |

## 1. Verified mechanisms (plan-day code reads)

### 1.1 #408 + #417 — one family: the detach join budget races dead-host wind-down

- `ConnectorControl::detach_connector_named`
  (`crates/taskboard-app/src/bootstrap.rs:219–246`) removes the actor
  from the registry, sends `SyncCommand::Detach`, then
  `stop_actor(actor, Duration::from_secs(self.shutdown_timeout_secs))`
  (:236–237). `stop_actor` (:709–716) → `join_bounded` (:719–735):
  `tokio::time::timeout` on the actor's join handle; on expiry it
  **aborts the actor** and answers `DetachOutcome::TimedOut` (:240–241).
  The default budget is `app.shutdown_timeout_secs` = 5 s.
- The actor processes `Detach` **only between cycles**:
  `run`'s biased `select!` handles `SyncCommand::Detach` at
  `crates/taskboard-sync-nextcloud/src/actor.rs:238–244`, but while a
  cycle (or the startup probe) is in flight the command sits in the
  inbox; the absorber at :286–296 re-checks only *after* the cycle
  returns. `probe_features` runs **before** the select loop
  (:309+) and the first cycle fires immediately (`deadline = now`,
  :227).
- Both failing tests attach clients pointed at **non-existent
  hosts**: `served_nextcloud(..., "https://cloud.example.com")`
  (`crates/taskboard-app/tests/ipc.rs:490–523`, used by ip13 at
  :537–585) and `any_client()` = `https://nc.example`
  (`src/bootstrap.rs:1379–1381`, used by the control-handle test at
  :1390–1439). Neither URL is ever expected to answer — they are
  identity strings — but the actor still runs its startup capabilities
  probe and first cycle against them, so **detach latency = DNS
  resolution + TCP failure time on the loaded machine**, unbounded by
  construction. Under full-suite parallel load (16 cores of nextest
  processes, DNS storms), that once exceeded 5 s → abort →
  `TimedOut` where the tests pin `Detached`.
- This is **not** the `TimedOut` family fixed by `ed64ecc` (#283
  F11-04): that fix rewrote the `sync` *verb's* freshness wait
  (`commands/sync.rs`); the detach path's `TimedOut` is the join
  budget and is untouched by it.
- Dispatch §13's normative direction for both legs: "explicit/injected
  detach budgets or deadline polls". Stage-3 trap list: "a budget must
  be injected/derived, not enlarged" — i.e. through the existing
  config seam, never by editing `stop_actor`'s call sites or the
  default.

### 1.2 #384 — the issue's evidence is routine output; the real buffer content was never captured

- The L13 test (`crates/taskboard-app/tests/cli_login.rs:305–357`)
  dispatches `Login` (alias) then `Connect` in-process, asserting
  `!alias_err.is_empty()` (:355) and `connect_err.is_empty()` (:356).
- The connect ceremony **always** prompts for the app password
  (`gather_identity`, `src/commands/connect.rs:315–342` — no password
  flag exists), via `StdinPrompts::read_secret_sync`
  (:136–153), which writes the label with `write!` (**no newline**) to
  **`std::io::stderr()` — the process stream, not the `CommandIo.err`
  buffer the test asserts on**. Under nextest, stdin is /dev/null →
  EOF → empty password → `verify` against `mount_boards_ok()`
  (cli_login.rs:19–29: matches method+path only, no auth check) →
  success. So **every** run of this test puts exactly
  `App password: App password: ` (two ceremonies, concatenated, no
  newline) on the *captured process stderr* that nextest prints under
  a failure.
- Therefore the issue's reading ("the login prompt leaked into the
  stream the test asserts stays clean") misidentifies routine output
  as the failure cause: the panic at :356 proves the **buffer** was
  non-empty, but the assert message is a static string, so the buffer
  content was lost. No production path writes prompt text into
  `CommandIo.err` (its writers in this flow: the login deprecation
  warning `commands/mod.rs:1160–1163`, `attach_nudge_warning`
  `connect.rs:607–623`, and the JSON envelopes via `emit_to` /
  `report_to` — none contain "App password").
- Honest candidates for the buffer content under load: (a) a
  `report_to` error envelope — e.g. a transient loopback failure of
  `verify` against the in-process wiremock; (b) `ConfigWrite` from the
  overlay write under /tmp pressure; (c) anything else the capture
  will tell us. Dispatch §13 already rules: *"no repro yet, so the
  deliverable is a capture policy …, not a speculative fix"* — the
  code read confirms that ruling and adds the mechanism correction.

### 1.3 #431 — deterministic harness bug: the two self-tests violate their own harness's contract

- `support::driver::run` (`crates/taskboard-ui-slint/tests/support/driver.rs:339–384`)
  pins the first `DriverOptions` in a process-global `OnceLock`
  (`OPTIONS`, :345) and asserts agreement (:348–351) — "a process has
  one platform: journey binaries must agree on DriverOptions".
- The self-tests inside the same support module request **different**
  options: `driver_boots_mounts_and_addresses_the_board` uses
  `headless_renderer: true` (:402) and
  `driver_reads_element_geometry_and_queries_all_instances` uses
  `false` (:464). In one process (plain `cargo test`) whichever runs
  second panics; under nextest (per-process) both pass — which is why
  CI is green.
- Survey: **every** journey call site across `golden_visuals.rs`
  (:78, :166), `journeys.rs` (9 sites, e.g. :264, :934) and
  `usability_components.rs` (9 sites) uses `headless_renderer: true`.
  The geometry self-test's `false` is the only `false` in the tree —
  the sole effect of `false` in `run` is skipping the `"software"`
  renderer name (:357–359), and the geometry reads it pins
  (`absolute_position`/`size`/find-all) do not depend on that arm
  (the issue's quick experiment confirms reads work over the testing
  backend).
- Dispatch §13's normative direction: "make the two self-tests agree
  on `headless_renderer: true` … or move them into their own test
  binary". Decision in §4.1.

### 1.4 #287 + #336 — one unexplained family: transient loopback transport under load (hypothesis, unconfirmed)

- `a10_304_with_a_cold_cache_refetches_unconditionally`
  (`crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs:692–722`)
  failed once (plain nextest lane, 0.398 s, 2026-10-16); the count
  assertion `received_requests().len() == 8` is deterministic under
  the harness's check-then-advance loop (`next_report`,
  actor_wiremock.rs:73–86: `try_recv` **before** every
  `advance(10ms)`, so the report is always received before any
  reschedule timer can fire a second cycle). A second cycle therefore
  cannot explain it.
- `push_exec::tests::delete_404_re_resolves_and_deletes_at_the_fresh_binding`
  (`src/push_exec/tests.rs:3228–3286`) failed once in **21 ms** under
  full-workspace parallel load; it mounts `.expect(1)` mocks and calls
  `server.verify()` (:3269). 21 ms fits: server boot + mounts, then a
  panic before/without the request traffic — e.g. `verify()` panicking
  because a transport-level connect failure aborted the push before
  the 404 mock ever saw a request.
- Shared suspect for both (plus #287's `create_stack_transport_failure_aborts`
  coverage-lane failure, 2026-10-16): a transient loopback connect
  failure under machine load (accept-queue pressure from dozens of
  concurrent wiremock servers across nextest processes) → the
  executor/actor sees a transport error → abort/Failed-report → the
  test's structural assert fires. Notably `refused_client()`
  (`src/push_exec/tests.rs:194–221`) **already hardens** the
  port-squat TOCTOU (10× probe-and-regenerate + eprintln diagnostics)
  — that test's own failure output would name its arm, and was not
  captured at observation time.
- Dispatch §13's ruling: "single unexplained observations: capture
  policy + recurrence watch". Both tests are *correct* — a real
  transport failure is exactly what they should surface loudly; the
  gap is evidence, not assertions.

## 2. Dispositions (decision record)

| Leg | Disposition | Rationale in one line |
|---|---|---|
| #431 | **Fix** (root-caused, deterministic repro) | The mechanism is a harness-contract violation reproducible on demand; the one-line alignment is the issue's own first suggestion. |
| #408 + #417 | **Fix** (root-caused by code read; repro confirms) | The 5 s default is an implicit environmental pin; the production config seam exists precisely for this (`config.app.shutdown_timeout_secs`); assertions untouched. |
| #384 | **Capture-policy annotation + assert-message enrichment** | Dispatch §13 forbids a speculative fix; the enrichment makes the next occurrence self-diagnosing; the comment corrects the mechanism misreading. |
| #287 | **Repro attempt + capture-policy annotation** | One unexplained observation; the harness is deterministic by construction; only a reproduced failure earns a fix. |
| #336 | **Repro attempt + capture-policy annotation** | Same family, same ruling; `verify()`/`.expect(1)` behavior is correct as-is. |

## 3. Repro & stress protocols (binding for stage 2)

Load shapes (this machine: 16 cores — the observations' own context):

- **L1 full-suite parallel**: `cargo nextest run --workspace`
  (the shape of #336's and #417's observations).
- **L2 crate-parallel**: `cargo nextest run -p taskboard-app` full
  crate (#384's observation context), or
  `-p taskboard-sync-nextcloud` (the wiremock churn shape for
  #287/#336 — the crate's ~50 wiremock tests run concurrently on 16
  cores).
- **L3 instrumented lane**: the AGENTS §10 coverage sequence
  (`cargo llvm-cov nextest --workspace …` into
  `target/llvm-cov-target`) — #287's original lane and a §10 gate
  stage 2 runs anyway.
- Iteration counts: **3× L1**, **10× L2 (each crate)**, **1× L3**,
  and **20× targeted single-test loops** only for a leg that
  reproduced under L1/L2 (isolating afterwards). Negative results are
  results — record run counts, commit, and outcome per leg.

Per leg:

- **#408/#417 (post-fix verification, mandatory)**: after the §4.2
  injection, run **10× L2 `-p taskboard-app`** + **3× L1**, all green,
  recorded as the fix's stress evidence (dispatch 12.3 re-runs these).
  Pre-fix, the same profile runs once as confirmation if the flake
  cooperates — it is *not* required to reproduce before fixing (the
  mechanism is established by the §1.1 code read; the tests are
  known-good solo and on main).
- **#384**: the L2/L1 profile above doubles as its repro attempt —
  with §4.3's enrichment in place, any recurrence now carries the
  buffer content in the panic message. Record either "no recurrence in
  N runs" or the captured content.
- **#287/#336**: L1 ×3 + L3 ×1 + L2 `-p taskboard-sync-nextcloud`
  ×10. On any failure: preserve the **full nextest failure section**
  (both legs' diagnostics — `refused_client`'s eprintln arms, the
  exact panicking assert — ride the captured stdout/stderr), the
  commit, the lane, and the concurrent load. Then, and only then,
  root-cause per §2; a confirmed transient-loopback root cause is
  **environmental** → annotate + the follow-up issue of §6, not a
  code change in this chunk.

## 4. The fixes, precisely (stage-2 work order)

### 4.1 #431 — align the geometry self-test on the harness convention

`crates/taskboard-ui-slint/tests/support/driver.rs:464`:
`headless_renderer: false` → `true`. One line; update the self-test's
doc comment to record *why* (the one-platform-per-process rule means
one options set per binary; `true` is every journey's convention).
Recorded rejected alternative: moving the self-tests to their own
binary (heavier; solves nothing once options agree). If covering the
non-software renderer arm of `run` is ever wanted, that is a separate
binary's job — note it in the disposition comment as a non-goal, not
a follow-up issue (no backlog entry: no idea, just a deliberate
scope line).

- **Verification** (deterministic, not statistical):
  `cargo test -p taskboard-ui-slint --test golden_visuals -- --test-threads=1 support::`
  fails today on base, passes after the flip; the nextest lane stays
  green.
- **Regression test applicability**: none beyond the self-tests
  themselves — the collision is unobservable under nextest (per
  process), so no CI-visible new assertion exists to add; the two
  self-tests agreeing IS the pin. Recorded so dispatch §13's "fixes
  carry regression tests where applicable" reads as satisfied-with-
  reason.

### 4.2 #408 + #417 — inject an explicit detach budget through the config seam

- **Site 1** (`crates/taskboard-app/tests/ipc.rs`, fixture
  `served_nextcloud` :490–523): after building `config` from
  `standalone_config`, set `config.app.shutdown_timeout_secs = 60;`
  with a comment naming the issue (#408): the budget must cover the
  dead-host wind-down of the startup probe + first cycle under load;
  5 s was an implicit default, not a chosen headroom. Fixture-wide is
  deliberate: ip13 (the named flake) and every other `served_nextcloud`
  user share the same dead-host identity shape; ip14's
  identity-mismatch detach performs no join, so its semantics are
  untouched.
- **Site 2** (`src/bootstrap.rs`, test
  `the_control_handle_drives_attach_and_detach_without_an_app_reference`
  :1390–1439): set `config.app.shutdown_timeout_secs = 60;` beside the
  existing field pins (naming #417), same rationale via `any_client()`
  = `https://nc.example`.
- **Injected, not enlarged** (dispatch 12.3 trap): the value flows
  through the production seam `config.app.shutdown_timeout_secs`
  (the same seam `commands/sync.rs:466` already injects `2` through);
  `stop_actor`, `join_bounded`, and the 5 s default are untouched.
- **Value rationale**: 60 s ≈ 12× the failed default — real margin
  over pathological local DNS walks (resolver timeouts × search
  domains) while staying 5× under `LIVE_DEADLINE` (300 s,
  `tests/common/mod.rs:301`) that bounds the ip13 journey. Accepted
  trade-off, recorded: a *genuine* wedge now fails at 60 s (legibly,
  `TimedOut`) instead of 5 s; #417 has no journey wrapper, so nextest
  slow-test marking is the backstop.
- **Assertions**: byte-identical — `DetachOutcome::Detached`,
  `DetachWireOutcome::Detached`, `NoConnector`, `IdentityMismatch`
  all stay exactly as pinned today. No test in the workspace pins the
  `TimedOut` detach outcome (verified by grep §plan-day), so no
  coverage is displaced.
- **No-weakening application**: the tests' contract is "a graceful
  detach of the attached connector answers `Detached`" — the budget is
  scaffolding that decides *when an abort is honest*; choosing it
  explicitly removes environment sensitivity without touching what is
  asserted. The AGENTS §5 determinism rule is what *motivates* the
  change (an assertion whose outcome depends on machine load is the
  anti-pattern the rule bans); no sleeps are introduced; the join is
  already a bounded poll.

### 4.3 #384 — assert-message enrichment (capture, not assertion)

`crates/taskboard-app/tests/cli_login.rs:355–356`: keep both
assertions exactly as they are; extend the messages to carry the
captured bytes, e.g.

```rust
assert!(
    connect_err.is_empty(),
    "connect keeps stderr clean: {:?}",
    String::from_utf8_lossy(&connect_err)
);
```

(and symmetrically for :355). This is dispatch §13's capture policy
made executable: the next occurrence's panic message **is** the
missing evidence. Not a text assertion (AGENTS §5): the asserted fact
stays `is_empty()` — a structural property; the message is
diagnostics.

### 4.4 Explicit non-goals (the boundary stage 3 enforces)

- **No production-code changes.** If a repro lands a root cause in
  production (e.g. a confirmed non-transient transport bug), it is a
  TDD bug fix in its own commit per dispatch 12.2 — not reachable
  from today's evidence.
- **No actor detach-semantics change** (cancel-in-flight-cycle on
  Detach): that rewrites the documented graceful contract ("an
  in-flight cycle completes first", `bootstrap.rs:706–708`); it is a
  design decision, and nothing observed demands it.
- **No client-level retry/connect-timeout change** for the #287/#336
  family: a kiosk-relevant resilience question (#6 below), not a
  flaky-pass patch.
- **No harness binary split** for the driver self-tests (#4.1
  rejected alternative).
- **No edits to #287/#336 tests**: their assertions are correct;
  they lack evidence, not strength.

## 5. No-weakening ledger (dispatch 12.3's per-assertion review aid)

| Touched test | Assertion before | Assertion after | Weaken? |
|---|---|---|---|
| `driver_reads_element_geometry_and_queries_all_instances` | geometry reads + find-all work over *some* backend, pinned via a colliding options set | identical reads over the testing backend (the convention every journey uses) | No — the read set is unchanged; only the (previously self-contradictory) options choice moves to the house value. |
| `ip13_detach_drops_the_connector_and_the_daemon_still_serves` | detach answers `Detached`; status flips; sync refuses; local serves; idempotent — under a 5 s *implicit* join budget | identical assertions under a 60 s *explicit injected* budget | No — nothing asserted is loosened; the budget is config, not assertion. |
| `the_control_handle_drives_attach_and_detach_without_an_app_reference` | attach/detach through the handle answers `Detached`/`NoConnector` — implicit 5 s | identical — explicit 60 s | No — same seam argument as ip13. |
| `l13_alias_happy_path_writes_stderr_connect_keeps_it_empty` | `!alias_err.is_empty()` / `connect_err.is_empty()` | identical facts; messages gain the captured bytes | No — message enrichment is diagnostics; AGENTS §5's text-assertion ban governs *assertions*, and none changed. |
| `a10_304…`, `delete_404_re_resolves…`, `create_stack_transport_failure_aborts` | untouched | untouched | — (repro/annotate only) |

## 6. Disposition comments (stage 2 drafts; stage 3 posts per dispatch 12.3)

Every comment must stand alone (dispatch 12.3): evidence, command,
commit, outcome — a reader reconstructs the triage from the comment
alone.

- **#431** — `fixed`: the contract citation (driver.rs:348–351), the
  survey (only `false` in the tree), the flip, the verification
  command with before/after, the non-goal note. **Close as completed.**
- **#408 / #417** — `root-caused-and-fixed`: the §1.1 mechanism
  (dead-host wind-down vs. join budget; actor.rs detach-between-cycles),
  the injection sites + value rationale, the "injected not enlarged"
  note, the §3 stress evidence. **Close as completed.**
- **#384** — `annotated (capture policy)`: the §1.2 mechanism
  correction (the two-prompt stderr is routine; the buffer content was
  never captured), the enrichment commit, the protocol to run on the
  next occurrence (record the full panic message — it now carries the
  buffer — plus lane + load + commit), and the honest candidate list.
  **Stay open, watchlist intact.**
- **#287** — `annotated (capture policy + recurrence watch)`: the
  repro record (runs, lanes, outcomes — including negatives), the
  loopback-churn hypothesis with its confirming signal (transport-arm
  diagnostics / a Failed-report panic), the §4.4 boundary (no client
  change here). If the hypothesis confirms, open the follow-up issue
  (`backlog:` prefix, category `performance`/`architecture`): client
  resilience to transient loopback/network connect failures — a
  design question, linked to this issue. **Stay open either way**
  (one unexplained plain-lane observation remains historical fact;
  closure waits for either a root cause or a fixed window with zero
  recurrence — stage 3's judgement, recorded in the comment).
- **#336** — same shape as #287 (verify-panic signal named: `.expect(1)`
  unmet with zero requests issued). **Stay open** unless reproduced
  and root-caused in-chunk.

## 7. Acceptance checklist (maps to dispatch §13 acceptance)

- [ ] All six issues carry a stand-alone disposition comment (§6).
- [ ] Fixed legs: #431 flip + two budget injections, with §3's stress
      evidence recorded; every touched assertion identical (§5 ledger
      clean).
- [ ] #384 enrichment lands; capture policy posted; issue stays open.
- [ ] #287/#336 repro protocols executed with recorded outcomes
      (negatives included); capture policies posted; issues stay open
      unless in-chunk root-caused.
- [ ] Full §10 gate suite green; no production diff anywhere in the
      PR (unless a §4.4 escape clause fired, with its own TDD commit
      and the issue saying so).
- [ ] Determinism rules upheld: no sleeps added anywhere; budgets via
      config seam only.

## 8. Risks

- **The detach budget is finite**: a pathological DNS walk can exceed
  60 s in principle. Accepted (recorded in §4.2): the failure mode
  becomes slow-and-legible instead of flaky; further margin is a
  one-line follow-up on the same seam if the stress evidence ever
  disagrees.
- **The sync-nextcloud family may not reproduce** within the §3
  budget: that is the honest outcome (annotate + watch), not a
  failure of the pass — dispatch 12.2 names negative results as
  results.
- **#417's unwrapped 60 s**: a genuine wedge costs a minute of CI
  before failing; nextest slow-test marking surfaces it. Judged
  better than a 5 s flake.
- **Line-number drift** between plan day and implementation day:
  references are to base `6d5507e` and are finding aids, not
  anchors.
- **The #384 misdiagnosis correction** contradicts the issue body's
  reading; the disposition comment must carry the code evidence
  (§1.2) so the correction is verifiable, not asserted.
