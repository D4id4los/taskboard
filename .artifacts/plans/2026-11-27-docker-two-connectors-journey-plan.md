# Docker two-connectors journey — diagnosis + fix record (#1538)

Date: 2026-11-27 · Branch: `fix/docker-two-connectors-journey` · Worktree:
`/home/viktoria/src/taskboard-WT-fix-docker-two-connectors-journey`
(Stage 1 + stage 2 compressed per the dispatch order: the diagnosis was
unambiguous and the fix small.)

## 1. Recorded reality

`it_nextcloud_docker_two_connectors_live_and_partial_outage`
(`crates/taskboard-app/tests/it_docker.rs`) failed deterministically:
2/2 local runs (404–422 s) + CI run 46057768521 on `main` (364 s),
panicking at the shared `eventually` helper. The issue (#1538) recorded
the panic as "phase (1) — both connectors settle Idle — never
satisfied". **That attribution was wrong**, and the correction is the
first finding of this chunk.

## 2. Diagnosis (evidence)

Instrumented reproduction (temporary, never committed: a DIAG print in
the `eventually` poll loop plus a `TWOCONN_NO_TEARDOWN` leak switch),
`docker events` recording, a red-handed process snapshot on the `kill`
event, and container access logs:

- **There is no external container killer.** The only `docker stop` of
  instance A is the test's *own* phase-(2) orchestration
  (`docker stop taskboard-it-nextcloud-c5a`, captured live with the
  test binary as the parent process). Instance A is healthy right up to
  that stop; the `nextcloud:35-apache` image sets
  `STOPSIGNAL=SIGWINCH`, which is why the stop logs as "caught SIGWINCH,
  shutting down gracefully".
- **Phase (1) passes.** Alpha's row reaches `Idle` with a `last_success`
  ~1 s after boot (DIAG: `alpha Idle last_success=21:34:28.17`, beta
  `Idle`, rows=2), the predicate passes within its 200 ms poll, and the
  journey proceeds to phase (2).
- **The failing predicate is phase (2)'s
  `eventually(alpha row phase == Offline)`.** Evidence chain:
  - Container trace (run of 21:15 UTC): exactly **one**
    `sync status upserted … phase="offline"` for alpha's board, against
    **48** `phase="failed"` upserts; `connector lost` logged exactly
    once. The row flickers Offline once and then reads
    `Failed { last_error: Network }` for the rest of the outage.
  - Mechanism: `fail_cycle`
    (`crates/taskboard-sync-nextcloud/src/actor.rs:1081`) fires
    `ConnectorLost` + latches `st.offline` **once per episode**, then
    sends a `SyncReport::Failed { kind: Network }` on **every** backoff
    cycle. The engine's `Failed` arm
    (`crates/taskboard-state/src/actor.rs`, the `pushes.is_empty()`
    arm) stamps `SyncPhase::Failed { last_error: kind }`
    unconditionally, and `go_offline` stamps `Offline` only on the
    `ConnectorLost` edge (idempotent "not already Offline" filter).
    Net: the row's steady state during an outage is
    `Failed { Network }`, with `Offline` visible for roughly one poll
    window.
  - The test awaits `stop_a.await` (the `docker stop` join) before
    starting the `eventually(Offline)`; apache's graceful drain is
    prolonged by the app's own keep-alive connections, so by the time
    the first poll runs, the single Offline window has closed. The
    deadline panic at the shared helper line followed — 4/4 runs with
    the corrected instrumentation.

## 3. Product-vs-test split (plan §2 decision 2 rule)

**Product defect** in `taskboard-state`. Canonical-behaviour evidence:
the accepted multi-board design's D-W7 projection
(`.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`,
"the map" section): *"per-board `Offline` phase is set by the engine
when a connector's boards go dark"* — the per-board row is the
per-connector health surface, and the journey's D5-18 exit criterion
("one connector down flips only its own boards Offline while the other
keeps cycling") reads exactly that row. A row that latches
`Failed { Network }` for the whole outage makes a down connector
indistinguishable from a transient error on every status surface. The
journey's assertion stands; the engine's verdict semantics were wrong.

## 4. Fix (TDD)

- RED: `k7_failed_network_verdicts_hold_offline_while_the_connector_is_lost`
  (`crates/taskboard-state/tests/suite/engine.rs`) — after
  `ConnectorLost` flips the row Offline, a `Failed { Network }` verdict
  from the lost connector must leave the phase Offline. Failed exactly
  as predicted (`left: Failed { last_error: Network }, right: Offline`).
- GREEN: `crates/taskboard-state/src/actor.rs` — the engine loop tracks
  the connectors currently in an offline episode
  (`lost_connectors`, inserted on `ConnectorLost`, removed on
  `ConnectorRestored`); the engine's board-less `Failed` arm stamps
  `SyncPhase::Offline` instead of `Failed { last_error: Network }` when
  the verdict is Network-classified **and** the reporting connector is
  lost. Non-network kinds and connected connectors keep the plain
  failure phase; the next `Completed` report restores `Idle` through
  the existing evidence path. ~25 lines + the test.
- Consumer 1 — the CLI `sync` verb's freshness classifier
  (`crates/taskboard-app/src/commands/sync.rs` `fresh_outcome`) read a
  landed Offline row as `Succeeded`; Offline after a landed report is
  now the outage verdict (`Failed { Network }`). Caught by the
  workspace gate (`repl::r10_a_connector_sync_failure_is_typed_and_survivable`
  went red: zero error lines for a sync against a dead server); pinned
  pure-side by `fresh_offline_is_a_network_failure` and end-to-end by
  r10 itself.
- Consumer 2 — the hermetic half of the v0.1.x upgrade journey
  (`it_docker.rs`
  `v0_1_x_additive_upgrade_boot_settles_offline_without_a_server`)
  expected the row to settle at `Failed { Network }`; under the
  corrected semantics it holds `Offline` (the refused server loses the
  connector). Expectation corrected to `Offline` on the same D-W7
  canonical-behaviour evidence — the old assertion pinned the transient
  flicker the fix removes.

## 5. Verification

| Gate | Result |
|---|---|
| `cargo nextest run -p taskboard-state` (full crate suite) | 224/224 passed (k7 included) |
| Journey run 1 (post-fix, docker tier) | PASS 168 s (`/var/tmp/twoconn-diag/fix-run1.log`) |
| Journey run 2 (post-fix, determinism) | PASS 120 s (`fix-run2.log`) |
| Full docker tier `it_docker` `--no-fail-fast` once | 9/9 PASS 280 s (`fix-tier.log`) |
| Workspace gate, run 1 | 1 red: `repl::r10` (consumer 1) — fixed |
| Workspace gate, run 2 | 5812/5813; 1 red: hermetic v0.1.x expectation (consumer 2) — corrected |
| Workspace gate, run 3 (`ws-gate3.log`) | **5813/5813 passed** |

## 6. Stage-3 REVIEW prompt (paste verbatim)

> You are the STAGE 3 (REVIEW) agent for the chunk "docker
> two-connectors journey" in the taskboard repo (GitHub:
> D4id4los/taskboard), branch `fix/docker-two-connectors-journey`,
> worktree `/home/viktoria/src/taskboard-WT-fix-docker-two-connectors-journey`.
> The change under review: issue #1538 — the docker two-connector
> journey's phase-(2) `Offline` assertion failing deterministically.
> Root cause + evidence: `.artifacts/plans/2026-11-27-docker-two-connectors-journey-plan.md`
> (§2–§4). The fix: `crates/taskboard-state/src/actor.rs` — the engine
> now tracks connectors in an offline episode (`ConnectorLost` /
> `ConnectorRestored` edges) and a `Network`-classified `Failed`
> verdict from a lost connector stamps/holds `SyncPhase::Offline`
> instead of overwriting it with `Failed { last_error: Network }`;
> regression coverage `k7_failed_network_verdicts_hold_offline_while_the_connector_is_lost`
> in `crates/taskboard-state/tests/suite/engine.rs`.
>
> Your job: (1) review the diff on the branch against AGENTS.md §2–§5
> (crate boundaries, typed primitives, test law — no weakening, no
> text assertions, determinism); (2) confirm the design grounding (the
> D-W7 projection sentence in
> `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`)
> actually supports "Offline holds for the episode"; (3) run
> `scripts/run-quality-gates.sh cargo nextest run --workspace` and
> `cargo fmt --all -- --check` + `cargo clippy --workspace
> --all-targets -- -D warnings` (route every cargo invocation through
> `scripts/memory-gate.sh`; never bare); (4) check the PR the
> dispatcher opened for a correct evidence-trail body.
>
> **Discipline (binding — docs/agent_dispatch.org; restated verbatim)**:
> A subagent is killed after ~600s without observable activity. **Long-command
> discipline**: (1) *The coverage trio never runs inside a subagent*: a
> stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus
> `scripts/run-quality-gates.sh cargo nextest run --workspace`
> (flock-safe; background + poll with useful work between polls if
> needed); the dispatcher runs the full AGENTS §10 suite between stage
> 2's landing and stage 3's ship. *Flock-deferral rule: when the gate
> lock is contended and no other useful work remains, never block —
> commit all landed work, report "gate deferred: lock held by
> <holder>", and end the stage; the dispatcher runs the gate.* (2)
> *Stage 3's scope ends at PR-open*: push, open the PR, report — the
> dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
> teardown, at-merge issue closures. (3) *Any command expected over ~5
> minutes runs backgrounded* with short interleaved polls and useful
> work between polls — commit all landed work BEFORE any wait. Large
> scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo
> invocation routes through `scripts/memory-gate.sh`** — nothing bare;
> a docker-tier test run is a self-capped long run: route
> `memory-gate.sh --preflight` only, never hold the gate lock across
> it. (5) *Stage prompts restate this discipline verbatim.*
> Out-of-scope findings become `backlog:` GitHub issues (or comments)
> immediately (AGENTS §7; search first).

## 7. Environment notes

- Docker teardown: every diagnostic run that leaked containers (via the
  temporary `TWOCONN_NO_TEARDOWN` switch) was cleaned by explicit
  `docker compose -p taskboard-it-c5a/-c5b down -v --remove-orphans` +
  `docker rm -f`; the two post-fix journey runs tear down through the
  test's own `Cleanup` drop. No `taskboard-it-*` containers remain.
- The sibling worktree `taskboard-WT-fix-ui-focus-remount-wrap` was
  never touched; the docker daemon is shared and all named resources
  used were the `-c5a`/`-c5b` suffixed projects this test owns.
