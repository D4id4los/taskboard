# [Plan] Backlog Reduction Round 2026-11-17 ("hygiene r2") — Round Plan

- **Date**: 2026-11-17
- **Kind**: stage-1 round plan — the round's scope authority, per-chunk
  normative specs, recorded decisions, sequencing, and chunking
  proposal (dispatch input). The dispatch plan is written only after
  the operator accepts this plan (AGENTS §7: a dispatch plan chunks an
  accepted round, never instead of it).
- **Origin**: the operator's 2026-11-17 ruling (include the
  second-tier set #327/#356/#355/#845/#764/#843) plus the
  correctness-hygiene round's dispatch-agent briefing (fresh issues
  #856/#857/#860/#865/#867/#868/#876/#877/#879; flaky recurrence state
  #697/#570/#650; the #879 process learning). Issue bodies are
  normative for their chunks; this plan records the round-level
  decisions the bodies left open.
- **Status**: DRAFT — awaiting operator acceptance. On acceptance,
  `2026-11-17-backlog-reduction-dispatch-plan.md` is written from §6.
- **Verified against**: `main` @ the sidecar Axis A merge (#878,
  2026-11-17). Symbol anchors re-anchor by symbol at stage-1 time.

## 1. Scope boundary

**In scope**: the briefing's nine fresh issues, the two armed flaky
diagnoses (#697, #570), #650's mechanism (via #857), the second-tier
set, and the #879 dispatch-law codification.

**Explicitly excluded (so nobody re-derives the boundary):**

- **#336, #287, #384** — the briefing's bounded loaded negatives
  (15/15, scan, scan) with no fresh CI recurrence; watchlist stands,
  not re-dispatched this round.
- **Round 8's remaining chunks** (task-detail-editor,
  board-interaction, task-card-controls, files-sync-status,
  tag-color, cross-surface-friction) and every issue in their §4
  coverage table — r8 chunks 1–2 landed (#862, #872), six are in
  flight; their surfaces are closed to this round except the single
  carve-out recorded in C4 for #860.
- **#353/#354** (undo-v2 UI legs) — user-visible, journey plans
  mandatory, and the r8 UI churn is still landing; they wait for the
  round after r8 completes. **#357** stays exploratory-open.
- The perf-architecture seven, the feature-scale items (#122, #218,
  #639–#642, #700/#701/#718/#723, #185/#184, #213, #215–#217, #494,
  #444, #121, #117, #193/#195, #440, the CLI/dx polish set
  #167/#180/#191/#198/#224/#225) — deferred per the standing
  recommendation report.
- **Dependabot backlog** (8 open PRs, #734–#739/#616/#617) — flagged
  to the operator as an optional compressed chore sweep outside this
  round's dispatch; not chunked here.

## 2. Family map

| Chunk | Branch seed | Issues | Size | Journey plan |
|---|---|---|---|---|
| C1 dispatch-law codification | `docs/dispatch-law-flock-deferral` | #879 | S (compressed) | exempt |
| C2 test-infra determinism | `test/proptest-persistence-fixture-clock` | #857, #843 (+ #650 mechanism) | M | exempt |
| C3 flaky diagnosis r2 | `test/flaky-diagnosis-r2` | #697, #570 | M/L | exempt |
| C4 audit-finding seams | `fix/audit-finding-seams` | #860, #868 | S/M | exempt |
| C5 domain same-ref collapse | `fix/domain-same-ref-collapse` | #856 | S | exempt |
| C6 app/storage hygiene | `test/app-storage-hygiene` | #865, #876, #877 | M | exempt |
| C7 cargo-mutants capped | `ci/cargo-mutants-capped` | #327 | M | exempt |
| C8 undo-v2 foundation | `feat/state-restore-stack` | #356 | L | exempt (engine-side) |
| C9 per-board undo scoping | `feat/state-per-board-undo` | #355 | M | exempt (engine-side) |
| C10 config-set propagation | `feat/cli-config-set-propagation` | #845 | M | **mandatory** (stage-1 writes; gate) |
| C11 boards publish --to-files | `feat/cli-boards-publish-to-files` | #764 | M | **mandatory** (stage-1 writes; gate) |

## 3. Chunk specs (normative)

### C1 — #879: codify the flock-deferral rule (`docs/agent_dispatch.org`)

The round's one process learning, already proven in the field (four
killed stage agents before the fix): **a subagent never blocks on the
`run-quality-gates.sh` flock** — when the lock is contended and no
other useful work remains, the agent defers the gate run to the
dispatcher (commit landed work, report "gate deferred: lock held by
X", exit cleanly) instead of waiting silently into the inactivity
cutoff.

Changes: `docs/agent_dispatch.org` §2 gains the deferral rule (and §8
a playbook row); the discipline block in
`.artifacts/templates/dispatch-plan.md` gains the one-line deferral
clause so every future dispatch plan inherits it. AGENTS.md §7's
inline two-liner stays as-is (it already points at the law; no
non-negotiable is added). ≤4 files, docs-only → **runs compressed**
(dispatcher-owned per `agent_dispatch.org` §7, compression stated in
the chunk plan). Acceptance: the law text states the deferral rule
and its trigger condition verbatim-consistently in both files; no
other wording drift in the law.

### C2 — #857 + #843: test-infrastructure determinism

- **#857 (CI-ephemeral proptest persistence)**: round-level decision —
  CI persists counterexamples by uploading the
  `proptest-regressions/` state as a workflow artifact **on failure**
  (retention ≥ 30 days) with the artifact link surfaced in the job
  summary; stage-1 may substitute an equivalent mechanism (e.g. a
  fail-comment bot) if it beats artifact latency, recorded with
  rationale. The mechanism is proven by a deliberately-failing
  canary run (merged as a disabled/skipped test or removed after the
  evidence capture — stage-1 records which). #650's dependence: once
  landed, the next CI occurrence of `no_local_ids_reach_the_dismissals_wire`
  persists its counterexample; C2 records the mechanism + the armed
  state on #650 (the issue stays open — the counterexample has not
  occurred yet).
- **#843 (journey_state Today-empty)**: decision — **option A**: anchor
  `journey_state`'s overdue stamp on the same clock the harness's day
  window reads (the `testing_strategy.org` §6.1 anchor-for-relative-oracles
  rule); the fixture becomes Today-nonempty by construction and the
  chunk-4-origin test
  (`us6_s2_today_row_menu_and_preset_key_recommit`) passes against
  `journey_state` itself. Option B (document + redirect journeys to
  `today_overdue_state`) is the recorded fallback only if stage-1
  finds the harness clock is not injectable at the fixture layer — a
  finding, not a silent switch.
- Acceptance: canary evidence for #857; #843's journey test passes
  against `journey_state` (or the recorded fallback); no production
  code change beyond the fixture/harness seams; #650 carries the
  mechanism comment.

### C3 — #697 + #570: flaky diagnosis r2 (the two armed ones)

- **#697 (post-stall wait path)**: the captured CI evidence (10.09 s
  failure inside the wiremock route's own legitimate 10 s window)
  arms a concrete fix direction: **reorder the budget inequality** —
  the delayed route's silent window must be strictly shorter than the
  post-stall assertion's wait budget by a named multiple (retiring the
  thin-headroom finding: budgets currently derived at exactly 3× the
  designed windows with zero margin between window and wait); plus
  the CI job for `cli_tty` runs with `TASKBOARD_PTY_TEST_GAP_LOG`
  enabled so the next occurrence (if any) captures gap telemetry.
  Stage-1 verifies the side question (test determinism vs production
  race) with the reordered budgets in hand; a production race found
  is the deliverable instead.
- **#570 (relay instance-locked export)**: two recurrences with
  captured output = a real diagnosis pass. Stage-1 reproduces the
  serialized-export assumption against the relay's actual locking;
  the fix lands on whichever side owns the bug (test premise vs
  relay race), typed-outcome asserted.
- Acceptance: per-issue disposition with evidence; no sleep-based
  stabilization; no weakened assertions; both issues close on fix
  evidence or carry dated armed-hypothesis comments if the round
  exhausts without a fix (honest not-fixed-yet).

### C4 — #860 + #868: audit-finding seams

- **#860**: `refresh`'s unchanged early-return skips `push_notice`,
  so `observe_connector_episode` can never surface `TaskResurrected`
  on the test seam. Fix per the issue's prompt: the early-return path
  runs `push_notice` (matching the tick loop's unconditional call) —
  or the seam decouples notices from refresh entirely if stage-1's
  read of the adapter contract favors it; a test proves the notice
  surfaces through `observe_connector_episode`. **r8 carve-out**: this
  touches `taskboard-ui-slint` (`lib.rs`/`adapter.rs`) — C4's stage-1
  verifies file disjointness against the six in-flight r8 branches and
  lands early; alternatively the operator may fold #860 into r8's
  friction chunk at dispatch time (default: this round).
- **#868**: the row-5 Deck sidecar validator-equality skip is
  correct-as-is today, but its optional skew-band hardening needs the
  prerequisite seams: a fetch-time seam on `CachedAppData` + an actor
  clock. C4 lands the seams **and** the guard behind them, with a
  time-controlled wiremock test proving the guard fires inside the
  skew band and stays silent outside it.
- Acceptance: #860's notice test; #868's band test (pause/advance or
  injected clock); diffs confined to the named seams; no behavior
  change outside the guard band.

### C5 — #856: same-ref collapse cannot rewind

Decision (the issue's proposed direction, confirmed as binding):
`collapse_for_task` treats a candidate whose `RemoteCardRef` equals
the bound member's ref as a **structural no-op** — the survivor stays
the bound member, `rebind` is false, `remote_seen` never rolls
backwards. Stage-1 verifies against `merge.rs` consumers that no
legitimate path relies on the older-stamp-wins shape before pinning
it; the unit test pins the degenerate shape and the
rewind-forbidden invariant; the #682 proptest premise stays exactly
as landed (#864). Acceptance: pinned unit tests; property suite
green; no other merge behavior change.

### C6 — #865 + #876 + #877: app/storage hygiene

- **#865**: decision — **option (2)**: the ~12 `AppConfig::load`
  round-trip unit tests assert through the file-layer decode
  (`extract_file_only`/`extract_raw`), and exactly one
  integration-suite twin carries the composed-load (file + env)
  behavior with the existing env lock. This matches the recorded
  house pattern ("unit tests avoid env mutation entirely; the real
  env scan is pinned in the integration suite", `ui_requests.rs:3133`)
  and adds no crate and no `unsafe`. Option (1) (test-support env
  crate) is rejected at round level: it reintroduces env mutation to
  escape edition-2024 `unsafe` — the house forbids the pattern, not
  the syntax. Option (3) stays rejected (the issue exists because
  accept-and-document was the interim).
- **#876**: #708 is closed (Axis A merged #878, catalogue green) —
  the "stays dead after the next round" condition is met
  unconditionally. Drop the dead `author` parameters from
  `apply_sync_report`/`apply_files_board_snapshot`, update the call
  sites (the state actor passes `Some(author)` today), remove the
  `#[allow(unused_variables)]` + comment. Mechanical; the stage-1
  diff inventory is the acceptance.
- **#877**: decision — the **shared fixture helper** owning
  "rewind to migration N" from a declarative strip table (one table
  per fixture, generated from the migration ledger where feasible);
  the cross-check test (fixture strip lists vs migration ledger) is
  the minimum acceptable fallback if stage-1 finds the generated
  table infeasible. The four hand-edited fixtures
  (connect.rs 0004/0015/0017, roundtrip.rs pre-0023) migrate onto the
  helper; the next ALTER migration costs zero fixture edits
  (demonstrated by a throwaway migration in the PR, removed after
  the evidence run — or kept as a canary per stage-1's call).
- Acceptance: hostile-env full-workspace run green (the #865
  acceptance from the previous round, now at the unit layer);
  clippy-clean removal of the allow attrs; the throwaway-migration
  evidence recorded on #877.

### C7 — #327: capped cargo-mutants one-off for taskboard-app

Scope per the issue: `commands/` + `ipc/` first, capped (stage-1
reads the issue's cap spec — a time/budget bound, recorded in the
chunk plan before the run). Deliverable: the mutants report
(`.artifacts/reports/<today>-cargo-mutants-taskboard-app-report.md`)
with survived-mutant triage: each survivor becomes a `backlog:` issue
or an in-round fix when the kill test is trivial (stage-1 records the
trivial-kill threshold, e.g. ≤ 10-line test). The run is
backgrounded + polled per the dispatch law (it is long); the cap is
the inactivity-cutoff defense. Acceptance: report present with
method + cap; every survivor dispositioned; no mutant-driven
production change without a failing test first (TDD order).

### C8 + C9 — #356 → #355: undo-v2 state-side (serial)

- **C8 (#356, RestoreStack + cascaded-task restore)**: the foundation
  chunk. Engine-side only (`taskboard-state` + domain message types)
  — exempt from journey plans; the UI legs (#353/#354) are explicitly
  out (§1). Stage-1 designs against the undo v2 family's recorded
  ordering (cleanup round A8: #356 → #355 → #354 → #353) and the
  existing journal/undo machinery (`journal.rs`, the NotUndoable
  ceremony fences); the RestoreStack semantics (stack depth, cascade
  granularity, interaction with the recent-window undo) are the
  chunk plan's core decisions. TDD per §5: snapshot tests over
  command sequences (insta), proptest for cascade invariants where
  pure.
- **C9 (#355, per-board undo line scoping)**: serial after C8 lands —
  it scopes the stack C8 built. Same laws.
- Acceptance (both): typed undo outcomes; no `RwLock`/`Mutex`
  (AGENTS §3); ceremony-guarded verbs stay `NotUndoable` (the r1
  erratum's ruling stands — #842's supersession note); snapshot
  corpus extended, not rewritten.

### C10 — #845: config-set propagation (journey-plan gated)

User-visible CLI semantics change → the journey plan is **mandatory**
(activated framework). Chunk shape: **stage 1 writes the UX journey
plan** (from `.artifacts/templates/ux-journey-plan.md`) covering the
config-set semantics (write-through to the running app vs explicit
restart contract vs divergence surfacing — the journey plan's design
round settles which; the issue records the divergence), the dispatcher
**pauses at the journey-acceptance gate**, and stages 2–3 proceed
only after the operator accepts. Implementation: the CLI write path +
whatever engine/config seam the accepted journey names; typed
outcomes; the divergence window the issue describes closes or is
surfaced per the accepted design.

### C11 — #764: `boards publish --to-files` (journey-plan gated)

New CLI verb → same gated shape as C10. The journey plan covers the
verb's contract (local-to-Files direct path; Deck-first-then-move
stays the CLI's only remote path per the issue's recorded boundary),
its verb grammar (`boards publish --to-files`), and its surfaces
(help text, errors — typed). Stage 2 implements behind the accepted
journey; the REPL twin verbs follow the existing command-family
conventions.

## 4. Operator-manual items (not dispatched — credentials/external state)

- **Hosted evidence session** (one sitting, curl, the Tier-3 account):
  (a) re-run #695's WebDAV matrix to confirm the verify-GET +
  bounded-412 give-up behavior against the real server (the previous
  round's recorded follow-up), and (b) produce #867's Deck REST
  validator retention matrix (same second/size rewrite discipline as
  #695's WebDAV one). Output: one report
  (`.artifacts/reports/<today>-hosted-validator-evidence.md`) —
  #867 closes on it; the #695 follow-up note updates. If the client's
  assumptions break on either surface, the findings become `backlog:`
  issues (sync-crate work, next round).
- **Mixed-fleet residual** (sidecar design §6): a release-notes/docs
  line rides the next release, not a backlog issue (briefing's
  ruling, confirmed).
- **Dependabot sweep** (optional): 8 open PRs; say the word and it
  runs as a compressed chore pass (rebase, CI-watch, per-PR verdicts)
  outside this dispatch.

## 5. Sequencing & parallelism

Default order (dependency- and conflict-driven):

1. **C1** first — tiny, and every later chunk's stage agents benefit
   from the codified deferral rule.
2. **C4** early — the r8 carve-out wants the seam landed before the
   friction chunk rebases.
3. Parallel wave (disjoint crates/files, stage-1 disjointness check
   against in-flight r8 branches where UI-adjacent): **C2 ∥ C3 ∥ C5 ∥
   C7**; **C10/C11 stage-1 journeys** may also start here (gates
   pause independently).
4. **C6** after the wave (touches app tests + state/storage call
   sites; avoids overlapping C2/C3's app-test files).
5. **C8 → C9** serial (same crate; C9 scopes what C8 builds).
6. **C10 → C11 implementation** after their journey gates clear.

The dispatcher owns stage-exit verification, full-suite runs between
stage-2 landing and stage-3 ship, CI watches, teardown, and at-merge
closures, per `docs/agent_dispatch.org` (as amended by C1).

## 6. Chunking proposal (dispatch input — written only after acceptance)

Eleven chunks, one branch/PR each; C10/C11 each carry a journey-plan
human gate between their stage 1 and stage 2. On the operator's
acceptance of this plan (with any per-chunk amendments), the dispatch
plan `2026-11-17-backlog-reduction-dispatch-plan.md` is written with
verbatim prompts carrying the long-command discipline **including the
C1 flock-deferral rule** (C1 lands first precisely so the rest of the
round dispatches under the fixed law).
