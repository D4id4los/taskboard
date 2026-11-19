# Chunk R2 — Shutdown Budget + Teardown Investigation (#1071, #968) — Plan

- **Date**: 2026-11-19
- **Branch**: `test/shutdown-budget` (worktree
  `../taskboard-WT-test-shutdown-budget`, branched off `origin/main`
  @ `6bd0f4dd`)
- **Normative sources**: scope report
  `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md` §A;
  dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md` §1/§3;
  runtime report
  `.artifacts/reports/2026-11-19-build-test-runtime-analysis-report.md` §B;
  issues #1071, #968.
- **Round laws binding this plan**: (a) typed outcomes only in the
  determinism fix; (b) the a12 fix is deterministic-by-construction
  (join-await or deadline poll — never a bigger yield count);
  (c) no delay-test contract weakened — proportional-assertion
  arguments recorded per changed test; (d) out-of-scope findings
  become `backlog:` issues immediately.

## 0. Open questions → recorded decisions

### D1. The fixture budget value: **1 s** (recorded decision)

Justification against the slowest legitimate verb work in the test
hermetic world:

- The verb path's work is: local wiremock roundtrips (sub-ms each),
  a storage flush through the sqlite actor over a tempdir (tens of
  ms), and the sync actor's detach (in-flight cycle completes — the
  cycle's remote calls are wiremock-local).
- Precedent already proves 1 s suffices: `cli_standalone.rs:254`
  sets 1, `commands/sync.rs:522` and `commands/status.rs:691/1232`
  set 2, `tests/bootstrap.rs:323/916` set 1 — those tests pass today.
- A 1 s budget cannot mask a real hang as a fast pass: a join that
  needed >1 s in this hermetic world *is* the finding the
  investigation hunts; the test fails (or the abort branch fires) and
  stage 2 diagnoses it rather than widening the budget.

Alternative considered and rejected: 2 s (the commands tests'
precedent). It doubles the per-verb cost for margin 1 s does not
need — no verb test today exercises work measured in whole seconds
against local infrastructure.

### D2. Inventory: which fixtures/tests get the override (mechanical sweep)

The root fix is **one line in `common::valid_config`**
(`crates/taskboard-app/tests/common/mod.rs:349`): add
`config.app.shutdown_timeout_secs = 1`. It cascades to every fixture
that builds on it:

| Fixture | File | Cascade |
|---|---|---|
| `common::valid_config` | `tests/common/mod.rs:349` | the one-line root fix |
| `config_for` | `tests/cli_verbs.rs:23` | inherits |
| `config_for` | `tests/cli_attach.rs:23` | inherits |
| `config_for` | `tests/lockfile.rs:16` | inherits |
| `config_for` | `tests/bootstrap.rs:42` | inherits (tests that pin the budget themselves — see D3 — override after the fixture and are untouched) |

Direct-construction fixtures that bypass `valid_config` and need an
explicit line:

| Fixture | File | Note |
|---|---|---|
| `config_for` | `tests/web_dashboard.rs:37` | `AppConfig::default()`-based; add `shutdown_timeout_secs = 1` unless a test there pins a timeout contract (stage 2 checks) |

Explicitly **not** changed (contract pins, law (c)):

| Site | Why untouched |
|---|---|
| `tests/bootstrap.rs:690` (30 s) + its doc comment | pins the linger-drain contract — the budget *is* the oracle |
| `tests/ipc.rs:544` (60 s) | #408's dead-identity-host detach: waits out a real connect-failure timeout; 60 s is the explicit chosen budget (its own comment says so) |
| `tests/cli_tty.rs` / `tests/repl.rs` / `common::write_test_config` (5 s / 2 s) | subprocess fixtures; process spawn + full boot under `-j 4` load makes 1 s risky tightness for zero measured need — they already carry an explicit budget, not the default. Revisit only after the production fix lands (see the follow-up in §4) |
| `tests/it_docker.rs`, `daemon_signals.rs`, `cli_export_pdf.rs` | already carry explicit budgets, including the drain-wedge contract at `daemon_signals.rs:52` |
| `bootstrap.rs:91` `BOOT_FAILURE_JOIN_BUDGET` | per the dispatch plan this constant is in scope — decision: **drop 10 s → 2 s** (the doc comment says "an actor that should drain and exit the moment its inbox closes"; 2 s is generous for that structural expectation and still 5× headroom over 1 s; the boot-failure tests drop from 10–30 s to 2–6 s) |

Completeness sweep for stage 2 (mechanical, ordered):

1. `grep -rn "config_for\|valid_config" crates/*/tests/` — confirm no
   fixture constructor beyond the tables above.
2. `grep -rn "shutdown_timeout" crates/*/tests/` — confirm every hit
   is either in D2's changed set or D3's untouched set.
3. Re-run the slow-test measurement and diff against the runtime
   report's 41-over-5s table: any test still >5 s must be either a
   contract pin (named) or a finding.

### D3. Budget-pin inventory (never touched, law (c))

Recorded so the sweep cannot weaken a correct test: the
linger-drain 30 s pin (`bootstrap.rs:676–690`), the drain-wedge
contract (`daemon_signals.rs`), the dead-host 60 s detach budget
(`ipc.rs:544`), the wiremock 5 s delay tests pinning real timeout
contracts (`dispatch_routing` — the 5 s delay stays; only the
teardown share of their 10.1 s shrinks via the fixture), and any
test whose assertion *is* a duration. If a delay shortening is ever
proposed for one of these, it needs the recorded
proportional-assertion argument — the default is no.

### D4. The a12 determinism fix (issue #968)

The test (`crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`
~836) polls `actor.join.is_finished()` in a 10 000-yield bounded
loop; under load the shutdown path can lose that race 10k yields in
a row.

Decision: **await the join handle directly** — the structural fact
the test names ("actor exits on Shutdown without any timer") — and
keep the typed outcome (`actor.join.await.expect(...)` already
asserts no panic):

```rust
actor.system_tx.send(SystemEvent::Shutdown).unwrap();
// Deterministic by construction: awaiting the join handle IS the
// fact under test ("the actor exits on Shutdown"). A bounded yield
// count is a poll-lottery, not a fact.
actor.join.await.expect("actor exits without panicking");
```

Rationale over a deadline poll: the test runs `start_paused = true`,
so a wall-clock deadline poll does not elapse without explicit clock
driving; awaiting the handle is the exact assertion, needs no
oracle, and cannot flake under load — the law-(b) preferred arm.
Trade-off recorded: if the actor ever *fails* to exit the test hangs
rather than fails; under a paused clock a hang cannot be a load
flake (all non-IO time is deterministic), so a hang is a genuine
regression signal and acceptable. Typed outcomes only — no log/text
assertions enter this test (law (a)).

### D5. #968 disposition rule

**Close-on-evidence, else armed.** #968 closes (with a dated comment
linking the investigation report and the a12 fix) iff BOTH hold:

1. the a12 join-await fix is landed, and
2. the teardown investigation produces a root cause that explains
   the failure mode (shutdown-path latency under load) with
   reproducible evidence — the same seam, per the scope report.

If the production root cause stays "armed" (diagnosed but the fix
deferred to a backlog issue), #968 gets the dated evidence comment
and stays OPEN under `flaky-test`+`watchlist` until the armed fix
lands. The stage-2 agent applies the rule; stage 3 re-checks it.

### D6. Investigation method (the teardown question: who burns the 10 s)

Goal: name the actor(s) that do not exit promptly at teardown, what
each waits on, and whether `shutdown_timeout_secs = 10` is a safety
margin or hides a live stall. Reproducible, tracing-based:

1. **Observation seam** (already exists): `join_bounded`
   (`bootstrap.rs:~1028`) logs `"join timed out; aborting"` per
   actor name, and `stop_actor` logs "already gone". The gap: the
   tests never install a tracing subscriber, so the evidence is
   invisible.
2. **Diagnostic harness** (a `#[ignore]`d test, not a permanent
   fixture): a small `#[tokio::test] #[ignore]` in
   `tests/bootstrap.rs` (or a `tests/diag_teardown.rs`) that boots
   the core with `common::valid_config` + a wiremock server, runs
   one full verb cycle, installs `tracing_subscriber` to stdout, and
   shuts down. Run with
   `RUST_LOG=trace cargo test -p taskboard-app -- --ignored
   diag_teardown --nocapture` — the per-join spans show which actor
   consumed the budget.
3. **Cross-check** with the cheap no-code probe first: run one slow
   verb test today with `--nocapture` and time the phases; the
   10 s-quantized `exec_time` already says *some* join times out —
   the harness identifies *which*.
4. **Candidates to instrument** (from the spawn/join inventory in
   `bootstrap.rs` / `main.rs:650–675` / `connector_ceremonies.rs:130,384`):
   the sync actor (`stop_actor`), the engine flush
   (`flush_bounded`, `bootstrap.rs` shutdown tail), the storage
   actor join, the files-sync join (`files_sync_join`), the web
   server join (`main.rs:663/671`), the UI retire budget
   (`ui.rs:870`), and the REPL/connector-ceremony joins
   (`repl.rs:1136`, `connector_ceremonies.rs:130/384`).
5. **Likely-mechanism hypothesis to confirm or refute** (stage 2
   verifies, never assumes): the sync actor's select loop
   (`actor.rs:302–308`) answers `SystemEvent::Shutdown` when
   subscribed — but the verb paths go through
   `detach_connector`/`stop_actor`, which send `SyncCommand::Detach`
   (graceful: "an in-flight cycle completes first, decision D4").
   If a cycle is parked on a read the wiremock did not answer (or a
   `poll_interval`-scaled sleep — `valid_config` leaves the 30 s
   default in place), the graceful detach waits out the budget while
   the `Shutdown` broadcast arm is never reached. The harness output
   arbitrates.
6. **Report**: `.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md`
   — per-actor findings, exactly one recommendation each
   (fix-now-in-scope vs armed `backlog:` issue), per the dispatch
   plan's acceptance. Production fixes only where the diagnosis is
   conclusive and the fix is small; everything else arms.

## 1. Implementation order (stage 2)

1. The one-line `valid_config` override + the `web_dashboard`
   explicit line (D2) — commit first, mechanical.
2. `BOOT_FAILURE_JOIN_BUDGET` 10 s → 2 s (D2) — with its doc comment
   updated; the boot-failure tests re-run green.
3. The a12 join-await fix (D4).
4. The diagnostic harness + instrumented run (D6), the investigation
   report; production fix only if conclusive-and-small.
5. The slow-test re-measurement (identical method to the runtime
   report: nextest `-j 4`, `exec_time` per test) and the delta
   recorded in the investigation report.
6. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo
   nextest run --workspace` (flock-safe; background + poll; defer
   per the rule if contended). Commit all landed work BEFORE any
   wait.

## 2. Acceptance (from the dispatch plan, verbatim obligations)

- The 10 s-quantized family drops to ~1 s per test; the suite wall
  drops accordingly — the re-measured table records it.
- The fixture override is total: no verb test left paying the 10 s
  default (the D2 sweep's step 3 is the proof).
- The investigation report exists with per-actor findings and exactly
  one recommendation each.
- #968 dispositioned per D5.
- No test weakened (D3's pins all still present and green).
- Diffs confined to: the fixtures, `BOOT_FAILURE_JOIN_BUDGET`, the
  a12 test, and any diagnosed production seam with a cited diagnosis
  line.

## 3. Risks

| Risk | Mitigation |
|---|---|
| A test hangs instead of failing after the budget drops (work legitimately needed >1 s) | The D2 sweep's step 3 catches it in the re-measurement; the budget's justification (D1) is the defense — such a test is a finding, not a budget bump |
| The a12 join-await turns a rare flake into a rare hang | Recorded in D4; paused-clock determinism makes a hang a genuine signal, not load noise |
| `BOOT_FAILURE_JOIN_BUDGET` 2 s is tight on a loaded box for a boot-unwind storage join | 2 s is 5× the 1 s verb budget and the join's structural expectation is instant drain; if the gate shows otherwise the constant is reconsidered with evidence, not silently widened |
| The investigation's harness adds a permanent slow test | It ships `#[ignore]`d and excluded from the gate; documented in the report |
| Wiremock 5 s delay tests: someone "helpfully" shortens the delay | D3 names them untouchable; stage 3's functional focus (a) re-checks |
| Gate lock contention | The flock-deferral rule: commit, report "gate deferred: lock held by <holder>", end the stage |

## 4. Follow-ups recorded

- After the production teardown fix (whichever actor the diagnosis
  convicts) lands, the subprocess fixtures' explicit 5 s budgets
  (`cli_tty`, `repl`, `write_test_config`) can drop in one
  mechanical pass — a follow-up chunk, not this one (their 5 s is an
  explicit budget, not the default; #1071's scope is the default).
  If the investigation arms the production fix as a backlog issue,
  this note rides inside that issue.

## 5. Estimated diff size

- Fixture sweep: ~2 files / ~4 lines (`valid_config`,
  `web_dashboard.rs`, plus any sweep-discovered stragglers).
- `BOOT_FAILURE_JOIN_BUDGET`: 1 line + doc comment.
- a12: net deletion of ~8 lines (the loop + the `is_finished`
  assert), 1 line added.
- Diagnostic harness: ~60–80 lines, `#[ignore]`d.
- Investigation report + re-measurement: artefact files only.
- Production diff (if the diagnosis is conclusive and small):
  targeted to the convicted seam, expected <30 lines.

Total code diff: ~100–120 lines, mostly tests + one constant.
