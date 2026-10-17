# [Plan] v0.1.0 Audit Remediation Wave 1 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-17
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 15 chunks of the first audit
  remediation wave (45 tracker items from the v0.1.0 audit decision
  session). No code in this commit.
- **Origin**: the v0.1.0 audit findings report
  `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`
  (§findings evidence anchors, §agenda schedule proposals, and
  §agenda-outcomes rulings — all normative) as materialized in the
  GitHub tracker: every chunk below names its issue numbers; each
  issue body carries the finding ID, the evidence anchor
  (`file:line` @ v0.1.0), the decided remedy, and the effort grade.
  Plus the user's 2026-10-17 decision-session rulings (§agenda-outcomes)
  and the 2026-10-17 screen/scheduling rulings (milestone assignments).
- **Status**: READY — all 15 chunks are dispatchable now, off the
  current `main` (post PR #108 merge). Nothing in the wave is gated on
  a human decision. **Deliberately not dispatched here**: everything
  still unassigned in the tracker (P2 sync-correctness design debts
  #128/#129/#130/#133/#150 — they need a sync-correctness design round
  first; design-round inputs #207/#208/#212/#176/#202/#203/#204/#211/#213;
  perf/DX/UI P3s; #234/#156 kiosk items beyond the already-assigned
  wave-kiosk scope; the D9 full slice/arithmetic audit chunk —
  accepted-risk-with-trigger per #303).

## 1. Chunks & ordering

| Chunk | Origin (issues · findings) | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | #302 · PD-7 | `refactor/enginecommand-envelope-rename` | mechanical rename making the `StateCommand`→`EngineCommand` payload/envelope relation explicit; no behavior change | S/M |
| 2 | #292 #293 #294 · F6-01/F6-02/F6-03 | `test/sync-push-pull-pinning` | push_exec defer + move fast-path suite tests; three pull dead-board wiremock tests; no production changes | M |
| 3 | #239 #303 · F3-10/11/12, D9 caveat | `test/domain-gaps-overflow` | domain conflict-core test gaps + boundary tests; release `overflow-checks` + domain arithmetic proptests | S/M |
| 4 | #240 #241 · F4-01/F4-02, F5-01/F5-02 | `test/state-storage-gaps` | two engine-suite tests; storage decode-rejection extension + concurrency probe | S/M |
| 5 | #258 #259 · F10-02, F11-01/02 | `fix/sync-stale-binding-clock` | stale-binding 404 re-resolve-or-defer + ADR 0007 note; `Arc<dyn Clock>` seam into the sync actor | M |
| 6 | #297 #260 #261 · F12-01/F12-02/F12-03 | `fix/security-hardening` | owner-only db/lock/config-tmp modes + mode tests; userinfo reject/strip; cleartext http warning | M |
| 7 | #257 · F9-01 | `fix/daemon-shutdown-typed-path` | typed failure path for the shutdown `Arc::try_unwrap` expect + regression test | S/M |
| 8 | #243 · F5-03 | `refactor/execute-batch-exhaustive` | `execute_batch` match-all → exhaustive `match` | S/M |
| 9 | #248 #250 #249 · F6-19/F6-21/F6-20 | `refactor/sync-client-helpers` | `DeckError::is_transport()`; label-delta failure-arm helper; generic client `call<T>` choreography helper | M |
| 10 | #247 · F6-10 | `refactor/push-exec-module-tree` | decompose `push_exec.rs` into a module tree; behavior-neutral (pinned by chunks 2/5's tests) | M |
| 11 | #296 #255 #251 #252 · F13-07/F14-03/F13-08, F14-02, F7-01/F7-02 | `ci/guardrails-bencher-deny` | bench targets out of default nextest lane + Bencher wiring (install CLI, drop `continue-on-error`, dedupe double run); mutant-sweep upload fix + scope; `deny(unsafe_code)` in ui-slint; cargo-deny boundary configs + CI steps | M |
| 12 | #264 #266 #301 · F15-01/02/03, F15-20, F2-01 | `docs/arch-org-release` | architecture.org rewrite (channels, reminders subsystem, modes); ADR 0013 formalization; CHANGELOG reminders entries; flow-model promotion into `docs/` | M |
| 13 | #265 #295 #246 · F15-08/F15-14, F14-01, F4-04 | `docs/testing-docs-runbook` | testing_strategy + coding_guidelines corrections; keyring tier-3 manual runbook; stale state-config doc comments (comment-only code edits) | S |
| 14 | #235 #238 #236 · pre-audit | `fix/release-tooling-launch` | `bump_version.sh` release cut on protected `main`; `release.yml` pwsh `!` parse fix; file-manager launch starts the GUI, not a headless daemon | S/M |
| 15 | #132 #171 #177 #304 · pre-audit | `feat/board-stack-structure-editing` | local board-edit commands + `Board` clocks; board reset/delete ceremony; board color/archive verbs; GUI stack creation | L |

**Dependency graph** (default dispatch order = chunk number):

- **Chunk 1 first, alone** — the rename's wide mechanical ripple across
  state/app/sync/UI means every later code chunk should rebase onto its
  result rather than the reverse. Nothing else may start until chunk 1's
  PR is open (chunks 3/11/12/13 may plan concurrently — they don't
  touch renamed symbols' files — but must not merge before it).
- **Serial chains** (same files/crate — dispatch strictly in order,
  chunk N+1 starts after chunk N's PR is *open*, rebase if conflicts):
  2 → 6 → 9 → 10 (all touch the sync crate's `push_exec`/`actor.rs`
  surface); 4 → 8 (both rework `taskboard-storage-sqlite`).
- **Parallel-safe after chunk 1** (disjoint files): 3, 4, 7, 11, 12,
  13, 14, 15. Within the parallel set, chunk 14 touches
  `.github/workflows/release.yml` which chunk 11 also touches —
  dispatch 14 after 11's PR is open and rebase.
- Chunk 15 is the largest and depends only on chunk 1.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` next to the project
root (AGENTS §8), branched off the then-current `main`, no PR ever
stacked.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the approved
   plan — TDD, every commit standalone (rebase-merge hygiene), full
   AGENTS §10 gate suite green. Does **not** push or open a PR.
   Reports deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   its base against the chunk's acceptance criteria + AGENTS rules
   **plus the code-quality mandate below**, re-runs whatever gates
   apply, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Its last task is
   shipping: push the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`.
   (`main` is branch-protected — merges happen on GitHub by the user,
   after the required checks pass.)

**Code-quality mandate** (stage 3, every chunk): beyond functional
review, evaluate (a) code quality: naming, comment discipline
(comments state constraints, not narration), idiomatic use of the
approved stack, no dead code or speculative generality, test quality
per AGENTS §5; (b) the Single Responsibility Principle: the crate-role
boundaries of AGENTS §2 are the SRP line — a boundary violation is an
SRP violation; (c) a single level of abstraction per function body: a
function either orchestrates named operations or works in primitive
data, never mixes. Findings are fixed on the branch like any other
finding.

**Wave-wide test rules** (AGENTS §5, binding on chunks 2–5, 7, 15 —
restated because they are this wave's main traps): no
`tokio::time::sleep` — use `tokio::time::pause()`/`advance()` or
predicate-with-deadline polling; assert typed outcomes
(`matches!(err, E::Variant)`), never error/log/UI text; no quality
ceilings; one behavior, one test, lowest layer — the engine thread and
integration suites only test routing, not behaviors a unit test owns;
mutant-derived tests pin desired semantics, not today's behavior; a
new test failing is a production bug until proven otherwise — never
weaken it (leave failing + report if the fix is out of scope).

**Wave-wide bookkeeping** (binding on every chunk's stage 2/3): the PR
body must close its issues with one `Closes #N` line per issue — that,
not a backlog edit, is how tracker items complete. Anything noticed
out-of-scope becomes a new GitHub issue (AGENTS §7 conventions), never
a silent expansion.

## 2. Chunk 1 — `EngineCommand` envelope rename (`refactor/enginecommand-envelope-rename`)

- **Covers**: issue #302 (PD-7; facts in the findings report §agenda
  row (c) and §agenda-outcomes). The payload/envelope split is
  mechanism-accurate and documented (`taskboard-domain/src/messages.rs:20`
  `StateCommand` = pure-data payloads; `taskboard-state/src/actor.rs:43`
  `EngineCommand` = channel-bearing envelopes: `Execute{command, reply}`,
  `Flush`, sync read envelopes). The session ruled the teachability cost
  real: rename so the relation is self-evident.
- **Acceptance**: a single mechanical rename commit-series (choose the
  target name in the stage-1 plan — e.g. `EngineEnvelope` — with the
  recorded rationale), zero behavior change, all call sites and doc
  comments updated, `docs/architecture.org` and crate docs where they
  name the enum updated, full gates green, no test changes beyond
  mechanical name updates.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the remediation-wave-1 dispatch ("EngineCommand envelope rename", branch refactor/enginecommand-envelope-rename) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, .artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md §agenda row (c) + §agenda-outcomes row (c), and .artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md §1/§2.
2. `gh issue view 302` (normative). Read crates/taskboard-domain/src/messages.rs and crates/taskboard-state/src/actor.rs (the two enums), then census every use of `EngineCommand` across the workspace (grep) so the plan names the full ripple surface: state actor/core, app bootstrap/IPC, UI forwarder, all test modules.
3. Create the worktree ../taskboard-WT-refactor-enginecommand-envelope-rename on a new branch refactor/enginecommand-envelope-rename off main.
4. Write the plan at .artifacts/plans/<today>-enginecommand-rename-plan.md. It must settle and record: the new enum name (candidate: EngineEnvelope — evaluate against the payload/envelope wording the docs already use) and any variant renames needed for the relation to read clearly (e.g. Execute's field `command`); the exact mechanical procedure (one rename commit, then doc-comment sweep commit) so every commit stays standalone; the explicit non-goal: NO behavior, signature-shape, or serialization changes (StateCommand is Serialize/Deserialize — if it appears in any persisted/on-wire format, prove it does not before finalizing).
5. Commit: docs(plan): enginecommand envelope rename scope. Do not implement; do not push.

Report: plan path, chosen names, ripple census (files + use sites), risks.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the remediation-wave-1 dispatch ("EngineCommand envelope rename") in ../taskboard-WT-refactor-enginecommand-envelope-rename (branch refactor/enginecommand-envelope-rename).

1. Read AGENTS.md, the stage-1 plan (.artifacts/plans/*enginecommand-rename*-plan.md), and the dispatch plan .artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md §1/§2. Recreate the worktree on that branch if missing.
2. Execute the rename exactly per the plan: pure mechanical edits, one rename commit then one doc sweep commit, each standalone. No behavior changes, no drive-by fixes — anything noticed goes to a new GitHub issue (report it, do not fix).
3. Run the full gate suite (AGENTS §10) and make it green.
4. Do NOT touch the issue (#302 closes via the PR body, stage 3). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the remediation-wave-1 dispatch (branch refactor/enginecommand-envelope-rename).

1. Read AGENTS.md, the dispatch plan §1/§2 (the code-quality mandate applies to you), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-refactor-enginecommand-envelope-rename.
2. Functional focus (chunk-specific traps): zero-behavior-change proof — diff must contain no logic edits, only the rename + comment/doc updates; no half-renamed identifiers (grep both old and new names across the workspace, including string literals and doc comments that reference the type by name); architecture.org and crate-level docs must not promise a stale type name; the domain crate must have gained no dependency (AGENTS §2 boundary).
3. Code-quality mandate: spell it out in full from dispatch plan §1 and apply it (for a pure rename: verify no comment was "improved" into narration, no speculative generality added).
4. Re-run the full gate suite yourself; any failure is a finding.
5. Fix findings on the branch (amend/reword, no fixup commits).
6. Ship: push, open the PR to main with gh (body: summary + `Closes #302`), `gh pr checks --watch`; failing checks are findings — fix, push --force-with-lease, repeat until green. Never merge; never touch main.

Report: verdict, findings, fixes, PR URL, green-checks confirmation.
```

## 3. Chunk 2 — sync push/pull test pinning (P1 trio) (`test/sync-push-pull-pinning`)

- **Covers**: #292 (F6-01 defer path, `push_exec.rs:554-586, :707`,
  14-mutant survivor cluster), #293 (F6-02 move fast path,
  `push_exec.rs:776`), #294 (F6-03 dead-board arms,
  `actor.rs:553/568/571/602/705/755`). Test-only chunk: production
  code must not change.
- **Acceptance**: the six behaviors pinned exactly as the issues
  prescribe — defer → empty outcomes/zero requests + subsumed-label
  assertion at `:707`; fetched (7,3) → move (7,9) ⇒ exactly one
  reorder `order: 9`; detail 500 → Failed with no tombstone; 403
  while listed → tombstone; live-board stacks 404 → Failed. All new
  tests pass against unmodified production code (if one fails, that
  is a live bug: stop, report, do not weaken — AGENTS §5). Gates
  green; diff = tests + fixtures only.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the remediation-wave-1 dispatch ("sync push/pull test pinning", branch test/sync-push-pull-pinning) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 is normative for this chunk), the findings report §findings P1 rows F6-01/F6-02/F6-03 + §agenda rows P1-2/3/4, the dispatch plan §1/§3, and `gh issue view 292` / `293` / `294` (normative).
2. Read the seams: crates/taskboard-sync-nextcloud/src/push_exec.rs (defer/delta chain at :554-586 and the subsumed-label arm at :707; move fast path at :776) and src/actor.rs (dead-board determination arms :553/:568/:571/:602/:705/:755), plus the existing suite/wiremock tests these join (tests/ dir of the crate) — the plan must reuse the established fixture/hermetic patterns, not invent new ones.
3. Create the worktree ../taskboard-WT-test-sync-push-pull-pinning on branch test/sync-push-pull-pinning off main.
4. Write the plan at .artifacts/plans/<today>-sync-push-pull-pinning-plan.md: the test catalogue (six behaviors, one test each, lowest layer — suite tests for push_exec, wiremock for the pull arms), for each: fixture shape, the exact asserted outcome (typed, not text), and which production arm it pins. Traps to settle in the plan: how to assert "zero requests" hermetically; how the 403-while-listed wiremock scenario distinguishes tombstone-cascade from plain failure; no sleeps, no intermediate-state asserts (AGENTS §5).
5. Commit: docs(plan): sync push/pull pinning test catalogue. No implementation; no push.

Report: plan path, catalogue, risks (e.g. an arm needing fixture support that does not exist yet).
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the remediation-wave-1 dispatch ("sync push/pull test pinning") in ../taskboard-WT-test-sync-push-pull-pinning (branch test/sync-push-pull-pinning).

1. Read AGENTS.md, the findings report rows above, the dispatch plan §1/§3, the three issues, and the stage-1 plan in the worktree (recreate if missing).
2. Implement the six tests exactly per the catalogue, TDD in the sense that each test must pass against the untouched production code; run it, and if it fails, treat a production bug as equally likely (AGENTS §5): stop on a failing new test, do not fix production in this chunk, report the failure verbatim. Test rules: no sleeps, no text assertions, typed outcomes, one behavior per test, hermetic wiremock only.
3. Gates green (AGENTS §10).
4. No push, no PR. Issues close via the PR body later.

Report: per-test pass proof, any suspected live bug (with output), deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the remediation-wave-1 dispatch (branch test/sync-push-pull-pinning).

1. Read AGENTS.md, the findings report F6-01/02/03 rows, the dispatch plan §1/§3 + the wave-wide test rules, the three issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: production diff must be EMPTY (tests + fixtures only — any production edit is a finding); each of the six behaviors is asserted at the typed-outcome level, not via text; the defer test actually exercises the :707 subsumed arm (mutant-check it mentally: would flipping the arm fail the test?); the dead-board tests distinguish the three arms and pin tombstone-vs-Failed exactly as the issue prescribes; no test sleeps, no transient intermediates, fixtures reuse the crate's existing patterns.
3. Code-quality mandate: in full from §1 (test quality per AGENTS §5 is the heart here).
4. Re-run the full gate suite; any failure is a finding.
5. Fix findings on the branch (amend/reword).
6. Ship: push, open the PR (body: summary + `Closes #292`, `Closes #293`, `Closes #294`), watch checks to green (fix + --force-with-lease as needed). Never merge; never touch main.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 4. Chunk 3 — domain test gaps + overflow hardening (`test/domain-gaps-overflow`)

- **Covers**: #239 (F3-10/F3-11/F3-12 domain conflict-core test gaps +
  F3-08's boundary tests) and #303 (D9 caveat part 1+2: release
  `overflow-checks = true` in root `Cargo.toml`; proptests over the
  pure domain arithmetic — scheduling/reminder occurrence/merge clocks).
  Part 3 of #303 (the full slice/arithmetic audit chunk) is
  accepted-risk-with-trigger — explicitly NOT in this chunk.
- **Acceptance**: the tombstone/conflict-core arms and guard
  conjunctions pinned by targeted tests (one behavior each); proptest
  suites over the named arithmetic functions; `[profile.release]
  overflow-checks = true` landed with the runtime-cost caveat recorded
  in the plan; gates green **including a release-profile smoke**
  (the gate suite runs in dev — the plan must include one
  `cargo build --release` + targeted release-mode test run to prove
  the flag compiles clean).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the remediation-wave-1 dispatch ("domain test gaps + overflow hardening", branch test/domain-gaps-overflow) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 normative), the findings report §findings F3-10/F3-11/F3-12 + F3-08 and §agenda rows B-1/(e) + §agenda-outcomes row (e), the dispatch plan §1/§4, `gh issue view 239` / `303` (normative), and the owning chunk report .artifacts/reports/2026-10-16-v0.1.0-audit-crate-domain-report.md for the evidence chains.
2. Read the seams: the domain conflict/merge core (apply_sync_report and per-entity primitives in crates/taskboard-domain/src/) — identify the untested tombstone arm and guard conjunctions the findings name; the arithmetic functions eligible for proptests (scheduling/reminder occurrence via due_firings/effective_occurrence, merge clocks). Existing tests you must not break: the crate's proptest and unit suites.
3. Create worktree ../taskboard-WT-test-domain-gaps-overflow on branch test/domain-gaps-overflow off main.
4. Write the plan at .artifacts/plans/<today>-domain-gaps-overflow-plan.md settling: the test catalogue (per arm: fixture, typed assertion); the proptest strategy per function (value ranges that include the adversarial cases — negative offsets, DST boundaries, u64/i64 edges) with a recorded prop count; the exact `[profile.release]` edit and the release-smoke procedure; explicit non-goal: no slice/arithmetic site audit, no production fixes beyond what a genuinely failing test forces (and a failing test = suspected bug, report, do not weaken).
5. Commit: docs(plan): domain gaps + overflow hardening scope. No implementation; no push.

Report: plan path, catalogue, prop strategies, risks.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the remediation-wave-1 dispatch ("domain test gaps + overflow hardening") in ../taskboard-WT-test-domain-gaps-overflow (branch test/domain-gaps-overflow).

1. Read AGENTS.md, the findings report rows, dispatch plan §1/§4, issues #239/#303, and the stage-1 plan (recreate the worktree if missing).
2. Implement per plan: targeted unit tests for the conflict-core arms; proptest suites with the planned strategies; the root-Cargo.toml overflow-checks edit as its own standalone commit; run `cargo build --release` and the targeted tests in release mode as the smoke. If any new test fails against untouched production: suspected bug — stop, report verbatim, do not fix or weaken (AGENTS §5).
3. Gates green (AGENTS §10; proptest runs count).
4. No push, no PR.

Report: per-test/per-prop results, release smoke result, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the remediation-wave-1 dispatch (branch test/domain-gaps-overflow).

1. Read AGENTS.md, the findings report rows, dispatch plan §1/§4 + wave-wide test rules, issues #239/#303, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: the tombstone arm test actually kills the mutant (would inverting the branch fail it?); guard-conjunction tests cover the conjunction, not just one conjunct; proptest strategies really include the adversarial ranges (a strategy that cannot generate negative/DST/overflow-adjacent values is a finding); overflow-checks sits in [profile.release] only (not dev — dev already has it), release smoke evidence present; no production logic changed beyond nothing.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body with `Closes #239` + `Closes #303`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 5. Chunk 4 — state & storage test gaps (`test/state-storage-gaps`)

- **Covers**: #240 (F4-01 loop-exit, F4-02 diff-negative — two
  engine-suite tests, mutant-proven gaps) and #241 (F5-01 decode
  defense-in-depth rejections, F5-02 single-writer concurrency
  probe). Test-only.
- **Acceptance**: the four behaviors pinned; storage rejections
  extended where the issue prescribes; concurrency probe hermetic
  (no sleeps — pause/advance); gates green; diff = tests (+ rejection
  arms if F5-01's remedy prescribes them) only.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the remediation-wave-1 dispatch ("state & storage test gaps", branch test/state-storage-gaps) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 normative), findings report rows F4-01/F4-02/F5-01/F5-02 + §agenda rows B-2/B-3, dispatch plan §1/§5, `gh issue view 240` / `241` (normative), plus the owning reports (.artifacts/reports/2026-10-16-v0.1.0-audit-crate-state-report.md, ...-crate-storage-sqlite-report.md).
2. Read the seams: the StateEngine suite (crates/taskboard-state — the loop-exit and diff-negative paths the findings anchor, and the existing insta snapshot suite you must not disturb); storage decode/rejection layer and the writer/pool seams (crates/taskboard-storage-sqlite). Settle: is F5-01 "extend rejections" a test-only change or does it add typed rejection arms? Decide from the issue body and record.
3. Create worktree ../taskboard-WT-test-state-storage-gaps on branch test/state-storage-gaps off main.
4. Plan at .artifacts/plans/<today>-state-storage-gaps-plan.md: catalogue (per behavior: fixture, typed assertion, layer — unit vs engine-suite, honoring one-behavior-one-test-lowest-layer); the concurrency probe's hermetic design (tokio pause/advance or join-based, explicitly no sleeps); snapshot-suite impact (none expected — the new tests assert outcomes, not snapshots).
5. Commit: docs(plan): state & storage test gap catalogue. No implementation; no push.

Report: plan path, catalogue, the F5-01 scope decision, risks.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the remediation-wave-1 dispatch ("state & storage test gaps") in ../taskboard-WT-test-state-storage-gaps (branch test/state-storage-gaps).

1. Read AGENTS.md, findings rows, dispatch plan §1/§5, issues #240/#241, the stage-1 plan (recreate if missing).
2. Implement per catalogue. A failing new test against untouched production = suspected bug: stop, report, do not weaken. Hermetic only; no text assertions; typed outcomes.
3. Gates green (AGENTS §10), including `cargo nextest run` for the two crates in isolation and in-workspace.
4. No push, no PR.

Report: per-test results, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the remediation-wave-1 dispatch (branch test/state-storage-gaps).

1. Read AGENTS.md, findings rows, dispatch plan §1/§5 + wave-wide test rules, issues #240/#241, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: production diff limited to what F5-01's recorded scope decision allows (test-only otherwise); the loop-exit and diff-negative tests kill their mutants (invert the branch mentally — does the test fail?); the concurrency probe cannot pass by luck (it must assert terminal state, not a race window); no snapshot files touched; the engine-suite tests only test what the unit layer cannot (routing/integration), not duplicated unit behaviors.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #240` + `Closes #241`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 6. Chunk 5 — sync correctness + clock seam (`fix/sync-stale-binding-clock`)

- **Covers**: #258 (F10-02 — THE live sync bug: pre-write fetch 404
  on a stale binding misclassifies as `RemoteMissing`, silently
  losing a queued edit after a remote stack move;
  `push_exec.rs:547-550/606-614`, `mapping.rs:155-161`,
  `pipeline.rs:1046-1073`) and #259 (F11-01/F11-02 — inject
  `Arc<dyn Clock>` via `SyncActorConfig` so the seven direct wall
  reads become fakeable; prerequisite for any skew work).
- **Acceptance**: re-resolve-or-defer behavior implemented + a
  regression test reproducing the issue's scenario (remote move +
  queued edit survives); ADR 0007 note documenting the classification
  policy; clock seam injected with the actor's tests now using a fake
  clock (no wall reads in sync tests); gates green. Chunk 2's tests
  must still pass unmodified.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the remediation-wave-1 dispatch ("sync correctness + clock seam", branch fix/sync-stale-binding-clock) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report rows F10-02/F11-01/F11-02 + §agenda rows M-1/L-1, dispatch plan §1/§6, `gh issue view 258` / `259` (normative), the owning report (.artifacts/reports/2026-10-16-v0.1.0-audit-theme-sync-persistence-report.md), and docs/adr/0007* (the note lands there).
2. Read the seams: push_exec pre-write fetch classification (:547-550, :606-614), mapping.rs:155-161, pipeline.rs:1046-1073; the sync actor's seven wall reads (actor.rs:310/342/591/592/617/618/717) and SyncActorConfig. Chunk 2's tests in tests/ are the floor you must not break.
3. Create worktree ../taskboard-WT-fix-sync-stale-binding-clock on branch fix/sync-stale-binding-clock off main.
4. Plan at .artifacts/plans/<today>-sync-stale-binding-clock-plan.md settling: the re-resolve-or-defer shape (fresh listing re-resolve once before classifying, or classify Rejected{LocalData} to defer — decide from the issue's remedy and record why); which config knob (if any) guards it; the regression-test scenario (fetched binding → remote stack moved → queued edit must survive, typed assertion); the Clock trait shape (domain clock seam reuse vs sync-local — prefer the domain `clock` module if it fits; record), injection point, and the fake-clock test conversions; ADR 0007 note wording. Explicit non-goals: no skew-mitigation logic (F11-02's fix is a later round — this chunk only opens the seam), no other classification changes.
5. Commit: docs(plan): stale-binding re-resolve + clock seam scope. No implementation; no push.

Report: plan path, decisions, risks.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the remediation-wave-1 dispatch ("sync correctness + clock seam") in ../taskboard-WT-fix-sync-stale-binding-clock (branch fix/sync-stale-binding-clock).

1. Read AGENTS.md, findings rows, dispatch plan §1/§6, issues #258/#259, the stage-1 plan (recreate if missing).
2. Implement per plan, TDD: the regression test first (it should fail on current code — that is the bug proof, keep it), then the fix; then the clock seam with fakes replacing wall reads in tests. Every commit standalone. No sleeps; typed assertions; the repo/actor boundaries of AGENTS §2 hold (the Clock trait lives in domain if the plan says so).
3. Gates green (AGENTS §10); chunk 2's tests green unmodified.
4. No push, no PR.

Report: per-gate results, the failing-first regression test evidence, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the remediation-wave-1 dispatch (branch fix/sync-stale-binding-clock).

1. Read AGENTS.md, findings rows, dispatch plan §1/§6 + wave-wide test rules, issues #258/#259, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: the regression test reproduces the exact loss scenario and now passes (flip the fix off mentally — it must fail); no OTHER classification arm changed behavior (diff inspection); the deferred/Rejected path is asserted typed, not via log text; the clock seam has zero remaining direct wall reads in the actor (grep the clock sources — std::time / chrono now — inside actor paths); the fake clock is deterministic (no real-time dependence); ADR 0007 note states the policy without rewriting the ADR's decision; chunk 2's tests untouched.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #258` + `Closes #259`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 7. Chunk 6 — security hardening: permissions + transport hygiene (`fix/security-hardening`)

- **Covers**: #297 (F12-01 — owner-only modes: `mode(0o600)` before
  create for db/lock/config-tmp at `connect.rs:22-28`,
  `lockfile.rs:93-98`, `connect.rs:497-503`; db post-open
  `set_permissions` incl. `-wal`/`-shm`; mode tests modeled on
  `repl_history`'s) + #260 (F12-02 — reject/strip `user:pw@` userinfo
  in `server_url` validators, `config.rs:717-728`,
  `client.rs:336-343`, leak-floor test) + #261 (F12-03 — loud
  one-time cleartext `http://` warning, `client.rs:330/1401`).
- **Acceptance**: fresh artifacts land 0600 (db incl. `-wal`/`-shm`
  post-open); pre-existing world-readable files are not silently
  trusted (decide + record: tighten on open or warn); mode tests
  follow the `repl_history` pattern; userinfo rejected-or-stripped in
  BOTH validators with a leak-floor test formatting a `user:pw@host`
  URL through client construction + a log-capture check at typed level;
  cleartext warning fires once (log + connect receipt) without breaking
  LAN-http test setups; gates green.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the remediation-wave-1 dispatch ("security hardening", branch fix/security-hardening) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report rows F12-01/F12-02/F12-03 + §agenda row N-1, dispatch plan §1/§7, `gh issue view 297` / `260` / `261` (normative), the owning report (.artifacts/reports/2026-10-16-v0.1.0-audit-theme-security-report.md), ADR 0012 (the trust story this enforces).
2. Read the seams: app-side artifact creation (connect.rs, lockfile.rs, server.rs:240-249 and repl_history.rs:11 as the model), the sync client's URL validators and log sites. Settle and record: tighten-on-open vs warn for pre-existing permissive files; reject vs strip for userinfo (per-validator or both?); the warning's one-time mechanism (log + connect receipt) and its config escape hatch if any; whether any kiosk path depends on group-read (decide from ADR 0012; default: no).
3. Create worktree ../taskboard-WT-fix-security-hardening on branch fix/security-hardening off main.
4. Plan at .artifacts/plans/<today>-security-hardening-plan.md with the test catalogue (mode tests incl. -wal/-shm; leak-floor test; cleartext warning test — one-time semantics asserted structurally, not by matching log text), the exact creation sites touched, and non-goals (no keyring changes, no TLS enforcement — http stays allowed but loud).
5. Commit: docs(plan): security hardening scope. No implementation; no push.

Report: plan path, decisions, risks (e.g. tempfs breaking mode assertions in CI).
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the remediation-wave1 dispatch ("security hardening") in ../taskboard-WT-fix-security-hardening (branch fix/security-hardening).

1. Read AGENTS.md, findings rows, dispatch plan §1/§7, the three issues, the stage-1 plan (recreate if missing).
2. Implement per plan, TDD: mode tests first (fail on current umask-default creation), then the `mode(0o600)`/post-open fixes; validator changes with the leak-floor test; the warning with its structural one-time test. Every commit standalone. Guard mode assertions against root/ restrictive-umask environments (skip-with-reason pattern if the suite has one — else assert via the returned metadata, not the fs view).
3. Gates green (AGENTS §10).
4. No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the remediation-wave1 dispatch (branch fix/security-hardening).

1. Read AGENTS.md, findings rows, dispatch plan §1/§7, the three issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: EVERY creation site in the findings' sibling table is covered (grep for create/open of db, lock, config-tmp, wal, shm — none left umask-default); post-open set_permissions cannot race the first write (ordering in code); the userinfo change cannot break valid URL forms (test both http/https, IPv6 host, port, path, no-credentials — the common case must keep parsing); the cleartext warning is once-per-process, not per-request (structural proof); no log site still formats the raw server_url with credentials (grep the four log sites from the finding).
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #297` + `Closes #260` + `Closes #261`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 8. Chunk 7 — daemon shutdown typed path (`fix/daemon-shutdown-typed-path`)

- **Covers**: #257 (F9-01 — `main.rs:320`
  `Arc::try_unwrap(app).expect(...)`: any future `Arc<App>` clone
  converts every daemon shutdown into a panic; dispatcher refcount
  facts at `server.rs:160/104-134`).
- **Acceptance**: the expect replaced by a typed, logged, non-panicking
  failure path (refcount proof or borrowed best-effort teardown —
  decide in the plan); a regression test holding a second ref across
  join pins the path; existing shutdown tests/behavior unchanged on
  the happy path; gates green.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of the remediation-wave-1 dispatch ("daemon shutdown typed path", branch fix/daemon-shutdown-typed-path) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report row F9-01 + §agenda row I-1, dispatch plan §1/§8, `gh issue view 257` (normative), owning report (.artifacts/reports/2026-10-16-v0.1.0-audit-theme-concurrency-report.md §7.1).
2. Read the seams: main.rs shutdown tail, ipc/server.rs dispatcher lifecycle (:104-134, :160) — enumerate today's strong-ref invariant and what a second ref would mean for each teardown step, so the plan can pick: typed error path (refcount check → structured shutdown error) vs borrowed best-effort teardown. Decide and record, including the chosen error type and where it surfaces (exit code? log span?).
3. Create worktree ../taskboard-WT-fix-daemon-shutdown-typed-path on branch fix/daemon-shutdown-typed-path off main.
4. Plan at .artifacts/plans/<today>-daemon-shutdown-typed-path-plan.md: the chosen shape + rationale, the regression test (second ref held across join → typed outcome, no panic — assert via typed result, never by catching a panic), the happy-path invariance proof (existing shutdown tests list), non-goals (no refactor of the dispatcher, no new channel).
5. Commit: docs(plan): daemon shutdown typed path scope. No implementation; no push.

Report: plan path, decision, risks.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of the remediation-wave-1 dispatch ("daemon shutdown typed path") in ../taskboard-WT-fix-daemon-shutdown-typed-path (branch fix/daemon-shutdown-typed-path).

1. Read AGENTS.md, findings row, dispatch plan §1/§8, issue #257, the stage-1 plan (recreate if missing).
2. Implement per plan, TDD: regression test first (fails/panics on current code — keep as the proof), then the typed path. Every commit standalone. No sleeps; deterministic.
3. Gates green (AGENTS §10).
4. No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of the remediation-wave-1 dispatch (branch fix/daemon-shutdown-typed-path).

1. Read AGENTS.md, findings row, dispatch plan §1/§8, issue #257, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: no `expect`/`unwrap` remains on the shutdown refcount path (grep the tail); the typed path is reachable-and-tested (the regression test holds a real second ref — not a mock); happy path byte-equivalent behavior (existing shutdown/IPC tests untouched and green); the error surfaces somewhere observable (typed result/log span), not swallowed.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #257`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 9. Chunk 8 — `execute_batch` exhaustive match (`refactor/execute-batch-exhaustive`)

- **Covers**: #243 (F5-03 — the match-all in storage's
  `execute_batch` means a future `PersistenceAction` variant lands
  silently unpersisted; the audit's one P0-shaped structure risk,
  P2 today only because all 14 variants are handled).
- **Acceptance**: restructured to an exhaustive `match` (or the
  equivalent compiler-enforced totality the plan chooses — e.g.
  `#[non_exhaustive]`-aware arms + a compile-fail/probe for new
  variants); behavior neutral; a test or compile-level proof that
  adding a variant without persistence fails to compile; gates green.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of the remediation-wave-1 dispatch ("execute_batch exhaustive match", branch refactor/execute-batch-exhaustive) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report row F5-03 + §agenda row E-1, dispatch plan §1/§9, `gh issue view 243` (normative), owning report (...audit-crate-storage-sqlite-report.md). Note dependency: dispatch only after chunk 4's PR is open (same crate) — rebase onto it if needed.
2. Read the seam: the `execute_batch` match in crates/taskboard-storage-sqlite (all 14 `PersistenceAction` arms today), the enum in taskboard-domain/src/persistence.rs, and `apply_actions`. Settle: exhaustive `match` vs `_ =>` + runtime tripwire — decide (prefer the compiler: an exhaustive match in the same crate as the enum definition), and how totality is proven for a FUTURE variant (the enum lives in domain; if the match is in storage, a new domain variant must break storage's build or trip a test — name the mechanism, e.g. a domain-side test enumerating arms, or moving the match's totality proof).
3. Create worktree ../taskboard-WT-refactor-execute-batch-exhaustive on branch refactor/execute-batch-exhaustive off main.
4. Plan at .artifacts/plans/<today>-execute-batch-exhaustive-plan.md: chosen mechanism + why, the totality proof, the behavior-neutral diff shape, non-goals (no enum redesign, no new actions).
5. Commit: docs(plan): execute_batch exhaustiveness scope. No implementation; no push.

Report: plan path, decision, risks.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of the remediation-wave-1 dispatch ("execute_batch exhaustive match") in ../taskboard-WT-refactor-execute-batch-exhaustive (branch refactor/execute-batch-exhaustive).

1. Read AGENTS.md, findings row, dispatch plan §1/§9, issue #243, the stage-1 plan (recreate if missing).
2. Implement per plan; the totality proof (test or structural mechanism) lands in the same commit as the restructure so no intermediate commit is silently-unpersisted-unsafe. Every commit standalone.
3. Gates green (AGENTS §10).
4. No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of the remediation-wave-1 dispatch (branch refactor/execute-batch-exhaustive).

1. Read AGENTS.md, findings row, dispatch plan §1/§9, issue #243, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: the future-variant trap is actually closed — add a hypothetical variant mentally and verify the chosen mechanism breaks the build/tests; no arm's behavior changed (diff arms 1:1 against old); no new `_ =>` crept back in (grep); chunk 4's storage tests green unmodified.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #243`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 10. Chunk 9 — sync client helpers (`refactor/sync-client-helpers`)

- **Covers**: #248 (F6-19 `DeckError::is_transport()` helper),
  #250 (F6-21 label-delta failure-arm helper), #249 (F6-20 generic
  `call<T>` request choreography helper replacing ~300 lines of
  repeated boilerplate). Dispatch after chunks 2/5/6 have PRs open
  (same crate).
- **Acceptance**: the three helpers exist with the dedupe proven by
  the diff (net line reduction; call sites converted, no duplicated
  logic left); behavior neutral — chunk 2's tests and the crate's
  wiremock suites green unmodified; each helper is its own standalone
  commit-series.

**Prompt 9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 9 of the remediation-wave-1 dispatch ("sync client helpers", branch refactor/sync-client-helpers) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report rows F6-19/F6-20/F6-21 + §agenda rows H-1/H-2, dispatch plan §1/§10, `gh issue view 248` / `250` / `249` (normative), owning report (...audit-crate-sync-nextcloud-report.md).
2. Read the seams: the client's request choreography sites (the ~300 repeated lines), the label-delta failure arms, the transport-error classification sites. Chunk 2/5/6's merged shapes are the floor. Settle: helper signatures (generic bounds, error mapping), the conversion order (three independent commit-series: #248, #250, #249), and whether any call site's behavior subtly differs today (those are NOT silently unified — enumerate and preserve, or fix consciously with a recorded reason).
3. Create worktree ../taskboard-WT-refactor-sync-client-helpers on branch refactor/sync-client-helpers off main.
4. Plan at .artifacts/plans/<today>-sync-client-helpers-plan.md: signatures, per-helper call-site census, the behavior-difference inventory, non-goals (no retry policy changes, no new endpoints).
5. Commit: docs(plan): sync client helper scope. No implementation; no push.

Report: plan path, census, risks.
```

**Prompt 9.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 9 of the remediation-wave-1 dispatch ("sync client helpers") in ../taskboard-WT-refactor-sync-client-helpers (branch refactor/sync-client-helpers).

1. Read AGENTS.md, findings rows, dispatch plan §1/§10, the three issues, the stage-1 plan (recreate if missing).
2. Implement per plan: three standalone commit-series, mechanical conversions, no drive-bys. Any suspected behavior difference discovered mid-conversion: stop that series, record, decide consciously per plan.
3. Gates green (AGENTS §10) — especially the crate's wiremock suites, unmodified.
4. No push, no PR.

Report: per-gate results, line-count delta per helper, deviations.
```

**Prompt 9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 9 of the remediation-wave-1 dispatch (branch refactor/sync-client-helpers).

1. Read AGENTS.md, findings rows, dispatch plan §1/§10, the three issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: net dedupe proof (the boilerplate is GONE, not wrapped); error-classification behavior identical per call site (the behavior-difference inventory was honored — spot-check two sites against their old shape); the generic helper does not grow speculative generality (bounds only as wide as today's call sites need); chunk 2's tests + wiremock suites untouched and green.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #248` + `Closes #250` + `Closes #249`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 11. Chunk 10 — `push_exec` module-tree decomposition (`refactor/push-exec-module-tree`)

- **Covers**: #247 (F6-10 — the crate's largest file keeps accreting;
  decompose into a module tree). Dispatch LAST in the sync chain
  (after chunks 2/5/9 have PRs open) — it moves the code those
  chunks just stabilized.
- **Acceptance**: `push_exec.rs` becomes a module tree (plan names
  the split); `pub use` surface of the crate unchanged (no downstream
  edits); behavior neutral — chunk 2's six tests + wiremock suites
  green unmodified; file-size delta visible in the PR body.

**Prompt 10.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 10 of the remediation-wave-1 dispatch ("push_exec module-tree decomposition", branch refactor/push-exec-module-tree) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report row F6-10 + §agenda row E-2 (promoted to now by the session), dispatch plan §1/§11, `gh issue view 247` (normative), owning report.
2. Read the whole of crates/taskboard-sync-nextcloud/src/push_exec.rs (post-chunks-2/5/9 shape) and its consumers. Settle: the module split (by responsibility — e.g. defer/delta, move/reorder, classification, plumbing; name every module and its contents), the visibility policy (module-private by default; crate surface unchanged via existing pub use), and the move mechanics (git mv-style pure moves first, then minimal import fixes, so review can diff-follow).
3. Create worktree ../taskboard-WT-refactor-push-exec-module-tree on branch refactor/push-exec-module-tree off main.
4. Plan at .artifacts/plans/<today>-push-exec-module-tree-plan.md: the module map with per-module symbol inventory, the commit series (pure move commits + one import-fix commit per move), non-goals (no logic edits, no renames beyond what the split forces, no new abstractions).
5. Commit: docs(plan): push_exec module split scope. No implementation; no push.

Report: plan path, module map, risks (cyclic-visibility traps).
```

**Prompt 10.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 10 of the remediation-wave-1 dispatch ("push_exec module-tree decomposition") in ../taskboard-WT-refactor-push-exec-module-tree (branch refactor/push-exec-module-tree).

1. Read AGENTS.md, findings row, dispatch plan §1/§11, issue #247, the stage-1 plan (recreate if missing).
2. Execute the split exactly per the module map: pure-move commits, then import fixes; zero logic edits (a logic-looking edit is a finding against yourself — revert it and note it as an issue instead).
3. Gates green (AGENTS §10); chunk 2's tests green unmodified.
4. No push, no PR.

Report: per-gate results, before/after line counts, deviations.
```

**Prompt 10.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 10 of the remediation-wave-1 dispatch (branch refactor/push-exec-module-tree).

1. Read AGENTS.md, findings row, dispatch plan §1/§11, issue #247, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: every moved symbol is byte-identical (use git diff --find-renames and inspect the non-rename remainder — it must be imports only); crate public surface unchanged (cargo public-api if available, else grep pub use); no module grew a new abstraction or helper; tests green unmodified.
3. Code-quality mandate: in full from §1 (module boundaries = the SRP lens here).
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #247` (+ the before/after line counts), watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 12. Chunk 11 — CI & guardrails (`ci/guardrails-bencher-deny`)

- **Covers**: #296 (F13-07/F14-03 bench targets out of the default
  nextest lane + F13-08 Bencher wiring: install the Bencher CLI —
  today's step dies `command not found` exit 127 swallowed by
  `continue-on-error`, token is valid; drop `continue-on-error`;
  de-duplicate the double `cargo bench --workspace` per main push),
  #255 (F14-02 mutant-sweep: fix upload, scope decision against the
  2026-10-19 weekly run, one capped app one-off), #251 (F7-01
  `#![deny(unsafe_code)]` in ui-slint lib.rs + NOTE update), #252
  (F7-02 cargo-deny boundary configs for domain purity + dumb-UI +
  the two CI steps). #112 is closed — its criteria ride #296.
- **Acceptance**: the default `cargo nextest run --workspace` lane no
  longer executes bench harness targets (`bench_fixtures.rs` kept);
  the benchmark job stays the every-PR bench execution; Bencher
  uploads on main pushes with the CLI installed and failures loud
  (verify by the run's step log naming the CLI, not exit 127); mutant
  upload fixed per the issue; deny configs enforce domain purity +
  ui-slint no-unsafe with the two CI steps wired; gates green.

**Prompt 11.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 11 of the remediation-wave-1 dispatch ("CI & guardrails", branch ci/guardrails-bencher-deny) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report rows F13-07/F14-03/F13-08/F14-02/F7-01/F7-02 + §agenda rows P1-8/J-3/B-5/O-1 + §agenda-outcomes row (a), dispatch plan §1/§12, `gh issue view 296` / `255` / `251` / `252` (normative), owning reports (...theme-perf-budgets-report.md §5, ...theme-test-quality-report.md, ...crate-ui-slint-report.md §2.3).
2. Read the seams: .github/workflows/ci.yml (the nextest lane :166-ish and the benchmark job :279-315 incl. the broken Bencher step), mutation-testing.yml + the mutant upload mechanism, deny.toml, crates/taskboard-ui-slint/src/lib.rs NOTE, and how bench targets are declared (Cargo.toml [[bench]] entries) so the lane filter mechanism is concrete (nextest filter expression vs --exclude vs profile).
3. Create worktree ../taskboard-WT-ci-guardrails-bencher-deny on branch ci/guardrails-bencher-deny off main.
4. Plan at .artifacts/plans/<today>-ci-guardrails-bencher-plan.md settling: the lane-filter mechanism + proof it keeps bench_fixtures.rs; the Bencher fix (install action pinning strategy, drop continue-on-error, dedupe: which invocation remains on main pushes); the mutant-sweep scope decision (#255's body names the decision frame — record the call: fix upload now, weekly-run observations as the evidence source, the capped app one-off in-scope or deferred — decide); the deny configs' exact bans and the two CI steps; the ui-slint deny one-liner + NOTE edit. Non-goals: no bench tuning, no new benches, no workflow restructuring beyond the named steps.
5. Commit: docs(plan): CI guardrails scope. No implementation; no push.

Report: plan path, decisions, risks (Bencher project/branch existence, deny false positives — plan must include a local deny run before pushing).
```

**Prompt 11.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 11 of the remediation-wave-1 dispatch ("CI & guardrails") in ../taskboard-WT-ci-guardrails-bencher-deny (branch ci/guardrails-bencher-deny).

1. Read AGENTS.md, findings rows, dispatch plan §1/§12, the four issues, the stage-1 plan (recreate if missing).
2. Implement per plan: workflow edits, deny configs, the lib.rs one-liner, mutant upload fix — each concern its own standalone commit. Validate locally what CI would run: `cargo nextest run --workspace` proves the bench targets are out of the lane; `cargo deny check` green with the new configs; `cargo clippy --workspace --all-targets -- -D warnings` still green with the deny attribute.
3. Gates green (AGENTS §10).
4. No push, no PR.

Report: per-gate results, local validation evidence (nextest lane listing, deny output), deviations.
```

**Prompt 11.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 11 of the remediation-wave-1 dispatch (branch ci/guardrails-bencher-deny).

1. Read AGENTS.md, findings rows, dispatch plan §1/§12, the four issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: the lane filter cannot silently re-include benches on a future target addition (mechanism is structural, not a hardcoded list that rots — judge from the plan's choice); the Bencher step now fails LOUDLY (no continue-on-error anywhere in the diff) and the double run is actually deduped (count cargo bench invocations on the main-push path); deny bans match the boundary semantics (domain: no tokio/network/fs crates; ui-slint: unsafe denied — not merely forbidding in a comment); the ui-slint forbid-impossible caveat from the finding is respected (deny, not forbid, with the verified-clean note).
3. Code-quality mandate: in full from §1.
4. Re-run gates + `cargo deny check`; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #296` + `Closes #255` + `Closes #251` + `Closes #252`, watch checks to green — the CI run itself is the test bed here: a green 'Performance Benchmarking' job with a Bencher step log showing real CLI output is the acceptance evidence; capture it in the report. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation + Bencher step evidence.
```

## 13. Chunk 12 — architecture & release docs (`docs/arch-org-release`)

- **Covers**: #264 (F15-01/02/03 — architecture.org misdescribes
  channels, misses the reminders subsystem, oversells modes),
  #266 (F15-20 — CHANGELOG misses the reminders feature family),
  #301 (F2-01 — promote the audit's derived flow model into `docs/`
  as a release-anchored, tag-regenerated artifact). Docs-only chunk
  (plus the flow-model derivation script/notes if the origin report
  prescribes one).
- **Acceptance**: architecture.org states the true channel topology,
  documents the reminders subsystem, and stops overselling modes;
  ADR 0013 formalized from the reminders design plan §12/§13 (the
  decision record exists and cross-links architecture.org);
  CHANGELOG carries the reminders entries; `docs/` gains the flow
  artifact with its regeneration procedure written down (six greps +
  a reading pass — the method section must let a fresh agent
  regenerate it at the next tag); gates green (pre-commit hooks).

**Prompt 12.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 12 of the remediation-wave-1 dispatch ("architecture & release docs", branch docs/arch-org-release) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, the findings report rows F15-01/F15-02/F15-03/F15-20/F2-01 + §agenda rows P-1/P-3/(b) + §agenda-outcomes row (b), dispatch plan §1/§13, `gh issue view 264` / `266` / `301` (normative), the owning report (...theme-docs-drift-report.md) and the chunk-2 flow-model origin (.artifacts/reports/ — locate the F2-01 derivation, its edge table + method, normative for the promotion), plus the reminders design plan (§12/§13 → ADR 0013) and docs/adr/ index conventions.
2. Read docs/architecture.org, CHANGELOG.md, docs/adr/, and the current code reality the rewrites must match (channel topology in app bootstrap, the reminders subsystem's actual shape). Settle: the rewrite outline per finding (what exactly is wrong today, quote it), ADR 0013's decision list (from the design plan §12/§13 — amendments need recorded reasons), the flow artifact's home (docs/ filename convention) + its documented regeneration procedure, and the CHANGELOG entry style (match existing release-notes voice).
3. Create worktree ../taskboard-WT-docs-arch-org-release on branch docs/arch-org-release off main.
4. Plan at .artifacts/plans/<today>-arch-org-release-plan.md: per-file edit outlines, the ADR 0013 draft skeleton, the flow artifact's section map, non-goals (no code edits, no new architecture claims beyond what the audit verified — every claim traces to the findings or the code read).
5. Commit: docs(plan): architecture & release docs scope. No implementation; no push.

Report: plan path, outline, risks (claims needing re-verification against HEAD).
```

**Prompt 12.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 12 of the remediation-wave-1 dispatch ("architecture & release docs") in ../taskboard-WT-docs-arch-org-release (branch docs/arch-org-release).

1. Read AGENTS.md, findings rows, dispatch plan §1/§13, the three issues, the stage-1 plan (recreate if missing).
2. Implement per outline: architecture.org rewrites (each finding its own commit), ADR 0013 (formal house format, decision list per the design plan), CHANGELOG entries, the flow artifact + regeneration procedure. Every factual claim verified against HEAD as you write (the audit's evidence anchors may have drifted — re-check, do not transcribe stale line numbers). Org-mode file conventions per the existing docs.
3. Gates: pre-commit hooks green (fmt/clippy run on the tree regardless of docs-only diffs).
4. No push, no PR.

Report: per-file summary, claims re-verified, deviations.
```

**Prompt 12.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 12 of the remediation-wave-1 dispatch (branch docs/arch-org-release).

1. Read AGENTS.md, findings rows, dispatch plan §1/§13, the three issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: every rewritten claim matches the code (spot-check the channel topology and the reminders subsystem description against the bootstrap/source — a docs chunk that re-describes wrongly is worse than no chunk); the three original misdescriptions are actually gone (quote-contrast old vs new); ADR 0013's decisions trace to the design plan §12/§13 and any deviation is recorded in the ADR; the flow artifact's regeneration procedure is executable by a fresh agent (follow it mentally end-to-end); CHANGELOG voice matches the existing file; diff contains no code changes.
3. Evidence & artefact mandate (the docs-chunk variant of §1's mandate): every factual claim carries evidence (source pointer or verified read), the artefact set is exactly the chunk's docs, nothing scratch lands.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #264` + `Closes #266` + `Closes #301`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 14. Chunk 13 — testing docs & state doc comments (`docs/testing-docs-runbook`)

- **Covers**: #265 (F15-08/F15-14 — testing_strategy steers to
  unwired tools; coding_guidelines describes an unperformable merge
  procedure), #295 (F14-01 — keyring gated test tier-3 manual
  runbook entry in testing_strategy §8; the secretless
  `workflow_dispatch` variant is explicitly deferred), #246 (F4-04 —
  stale doc comments in state config; comment-only code edits).
- **Acceptance**: testing_strategy matches the actually-wired tooling
  (name what is wired: nextest, llvm-cov, cargo-deny, insta, proptest,
  mutant weekly); the merge procedure describes what is performable
  (protected main + PR + rebase-merge); §8 gains the keyring tier-3
  runbook (how to run `TASKBOARD_IT_KEYRING` safely, what it dirties,
  expected manual steps); state config doc comments state today's
  truth (comment-only diff); gates green.

**Prompt 13.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 13 of the remediation-wave-1 dispatch ("testing docs & state doc comments", branch docs/testing-docs-runbook) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, findings report rows F15-08/F15-14/F14-01/F4-04 + §agenda rows P-2/(d-adjacent)/P1-5, dispatch plan §1/§14, `gh issue view 265` / `295` / `246` (normative), owning reports (...theme-docs-drift-report.md, ...theme-test-quality-report.md).
2. Read docs/testing_strategy.org, docs/coding_guidelines.org (merge procedure section), crates/taskboard-state config doc comments (the F4-04 anchors), and the actually-wired CI (workflows) + test-support reality so every doc claim is verifiable. Settle: the per-section rewrite outlines; the runbook's step list (walk the gated test's setup yourself to write real steps); for #246, the per-comment correction list (old claim vs today's truth).
3. Create worktree ../taskboard-WT-docs-testing-docs-runbook on branch docs/testing-docs-runbook off main.
4. Plan at .artifacts/plans/<today>-testing-docs-runbook-plan.md: outlines + the comment-correction table; non-goals (no tooling changes, no new test tiers, the workflow_dispatch keyring variant explicitly out).
5. Commit: docs(plan): testing docs & runbook scope. No implementation; no push.

Report: plan path, outline, risks.
```

**Prompt 13.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 13 of the remediation-wave-1 dispatch ("testing docs & state doc comments") in ../taskboard-WT-docs-testing-docs-runbook (branch docs/testing-docs-runbook).

1. Read AGENTS.md, findings rows, dispatch plan §1/§14, the three issues, the stage-1 plan (recreate if missing).
2. Implement per outline: doc rewrites (one commit per finding), the §8 runbook (verify each step by actually running it or by reading the test's exact setup — no invented steps), the state doc-comment corrections (comments only — zero code-token changes; a diff touching non-comment code is a self-finding).
3. Gates: pre-commit hooks green.
4. No push, no PR.

Report: per-file summary, verification notes, deviations.
```

**Prompt 13.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 13 of the remediation-wave-1 dispatch (branch docs/testing-docs-runbook).

1. Read AGENTS.md, findings rows, dispatch plan §1/§14, the three issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: every tool named in testing_strategy is actually wired (cross-check each against .github/workflows + Cargo.toml); the merge procedure is performable by a fresh contributor verbatim; the runbook's steps match the test's real behavior (walk them against tests/secrets.rs:126-140); the state comments now state constraints/truth, not stale narration, and the diff is comment-only; no doc claim contradicts AGENTS.md.
3. Evidence & artefact mandate (docs variant, in full from the stage-1 shape of dispatch plan §1 — adapted: claims carry verifiable pointers).
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #265` + `Closes #295` + `Closes #246`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 15. Chunk 14 — release tooling & launch (`fix/release-tooling-launch`)

- **Covers**: #235 (`bump_version.sh` release cut must work with
  branch-protected `main` — today's direct-commit/tag flow cannot
  land), #238 (`release.yml` MCP-feature guard cannot parse on the
  Windows leg — pwsh vs bash `!`), #236 (running the release binary
  from a file manager (Dolphin) silently starts a headless daemon
  instead of the GUI). Dispatch after chunk 11's PR is open (both
  touch `.github/workflows`).
- **Acceptance**: the release cut is performable as documented
  (script + human steps match branch protection — decide the shape:
  cut a release branch → PR → tag after merge, and record it);
  `release.yml` parses and runs on the Windows leg (the guard logic
  ported to pwsh-safe syntax, validated locally if possible, else by
  actionlint + a dry-run dispatch); the GUI binary detects the
  file-manager/no-terminal context and starts the GUI session
  (decide the detection signal — no TTY vs env vars — and the kiosk
  interplay with the accepted #237 design; headless daemon mode
  stays reachable explicitly); tests/gates green where code changed.

**Prompt 14.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 14 of the remediation-wave-1 dispatch ("release tooling & launch", branch fix/release-tooling-launch) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§9 versioning normative), findings/dispatch context, `gh issue view 235` / `238` / `236` (normative), scripts/bump_version.sh, .github/workflows/release.yml (the pwsh `!` guard site), and the app's mode-selection entry (main.rs / the daemon-vs-GUI decision point) plus the accepted kiosk design (issue #237 body — control via CLI/ssh, daemon-owned core) so the launch fix does not fight it.
2. Settle and record: the release-cut procedure on protected main (exact git/gh steps the script now performs vs what the human does; the script's clean-tree/version/commit/tag contract from AGENTS §9 preserved); the pwsh-safe guard rewrite (and how it is validated without a Windows machine — actionlint, yaml parse, a workflow_dispatch dry-run if the workflow supports it); the GUI detection rule (enumerate the signals: TTY presence, parent process class, env vars — pick one, justify; define behavior for each context: terminal, file-manager, ssh, kiosk flag) and its test strategy (mode-selection unit tests, no display required).
3. Create worktree ../taskboard-WT-fix-release-tooling-launch on branch fix/release-tooling-launch off main.
4. Plan at .artifacts/plans/<today>-release-tooling-launch-plan.md with the above decisions, the test catalogue, non-goals (no installer work (#114), no new release targets (#115), no daemon feature changes).
5. Commit: docs(plan): release tooling & launch scope. No implementation; no push.

Report: plan path, decisions, risks (Windows-leg validation limits, GUI detection false positives).
```

**Prompt 14.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 14 of the remediation-wave-1 dispatch ("release tooling & launch") in ../taskboard-WT-fix-release-tooling-launch (branch fix/release-tooling-launch).

1. Read AGENTS.md, dispatch plan §1/§15, the three issues, the stage-1 plan (recreate if missing).
2. Implement per plan: script + workflow edits (one commit per issue), the mode-selection change TDD (unit tests for the detection rule across the enumerated contexts; no display, no sleeps). Validate release.yml mechanically (actionlint/yaml parse; note any residual Windows-only risk in the report rather than claiming proof you do not have).
3. Gates green (AGENTS §10).
4. No push, no PR.

Report: per-gate results, validation evidence, deviations.
```

**Prompt 14.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 14 of the remediation-wave-1 dispatch (branch fix/release-tooling-launch).

1. Read AGENTS.md, dispatch plan §1/§15, the three issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: walk the new release cut end-to-end mentally against branch protection (every step either succeeds as the invoking user or is an explicit human step — no step silently assumes push rights to main); the pwsh guard contains no bash-isms (scan the whole Windows leg, not just the fixed line — `!` inside if conditions, bash-only substitutions); the detection rule's truth table is tested per context and cannot regress kiosk/headless (the #237 contract holds: daemon-owned kiosk mode still starts headless under its flag/ssh path); AGENTS §9's script contract (clean tree, bump, commit, tag) is preserved in shape.
3. Code-quality mandate: in full from §1.
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body `Closes #235` + `Closes #238` + `Closes #236`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 16. Chunk 15 — board/stack structure editing (`feat/board-stack-structure-editing`)

- **Covers**: #132 (local board-edit commands + `Board` clocks — the
  domain/engine prerequisite), #171 (board reset/delete ceremony),
  #177 (board color/archive edit verbs + their clocks), #304 (GUI:
  create new stacks on a board). Dispatch order inside the chunk:
  #132 → #177/#171 → #304 (each layer consumes the previous).
  Depends only on chunk 1.
- **Acceptance**: board edit/reset/delete/color/archive verbs exist
  as `StateCommand`s with proper per-field clock semantics (which
  fields bump which clocks — decided in the plan against the merge
  model); outbox/local-op entries generated so sync can carry them
  (or the explicit sync-deferral recorded per verb with rationale);
  deck round-trip treatment for app-only fields decided per #207's
  folded guidance or explicitly deferred with the ADR/issue trail;
  the GUI offers stack creation dispatching the new command through
  the dumb-presentation-client path (no state mutation in the UI
  layer); engine snapshot tests (insta) cover each new command
  sequence; sync actor tests prove the new ops ride the existing
  push pipeline or are consciously deferred; CLI verbs where the
  issues prescribe; gates green.

**Prompt 15.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 15 of the remediation-wave-1 dispatch ("board/stack structure editing", branch feat/board-stack-structure-editing) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§2 boundaries, §4 stack, §5 testing — all normative), dispatch plan §1/§16, `gh issue view 132` / `171` / `177` / `304` (normative), the design context the fold map points to (multi-board wave inputs: findings F3-04/F7-05 summarized in the findings report §fold — read enough to avoid hard-coding global assumptions the wave will remove), docs/architecture.org (post-chunk-12 shape if merged; else current), and the existing patterns: how TaskCommands flow UI→engine→storage→outbox→sync (the create/edit path is your template).
2. Read the seams: taskboard-domain entities/state (Board shape, existing clocks, tombstone machinery — stack tombstones exist; board verbs are the new surface), state engine execute + snapshot machinery, storage persistence actions (note chunk 8's exhaustive-match contract: new PersistenceAction variants must be added totality-proof included), the outbox/LocalOp pipeline, the sync push planner (what a board verb must produce to be pushable — or why it is local-only), UI stack rendering + command dispatch (slint side), CLI command modules. This is an L chunk: the plan is the contract, be thorough.
3. Settle and record (each with rationale): the new StateCommand variants + payloads; per-verb clock semantics (board-level clock vs per-field clocks; what bumping means for LWW); reset/delete ceremony semantics (#171: what reset clears, what delete tombstones, confirmation UX if any); color/archive field representation + Deck mapping (Deck's board color endpoint shape; app-only fields' round-trip strategy — defer consciously if needed, cite #207); which verbs generate sync-able ops vs local-only (and the recorded reason per verb); the storage actions + their place in the exhaustive-match proof; the insta snapshot catalogue; the UI affordance (where stack creation lives in the UI, the dispatch path, empty-board edge case); CLI verbs (`boards edit/archive/delete/reset` or whatever the issues prescribe).
4. Create worktree ../taskboard-WT-feat-board-stack-structure-editing on branch feat/board-stack-structure-editing off main.
5. Plan at .artifacts/plans/<today>-board-stack-structure-editing-plan.md with all of the above, the test catalogue per layer (domain unit → engine snapshots → storage → sync pipeline → UI dispatch → CLI), and the internal milestone order (#132 domain/engine first, then storage/outbox, then sync, then UI/CLI).
6. Commit: docs(plan): board/stack structure editing design. No implementation; no push.

Report: plan path, the decision list, estimated diff size per crate, risks (merge-model edge cases, Deck mapping gaps).
```

**Prompt 15.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 15 of the remediation-wave-1 dispatch ("board/stack structure editing") in ../taskboard-WT-feat-board-stack-structure-editing (branch feat/board-stack-structure-editing).

1. Read AGENTS.md, dispatch plan §1/§16, the four issues, and the stage-1 plan — the plan's decision list is the contract (recreate the worktree if missing). Read .artifacts/reports/2026-10-17-backlog-to-github-issues-migration-report.md only if you need the old backlog entry pointers.
2. Implement in the planned internal order, TDD at each layer: domain verbs + clocks (unit tests first), engine execution + snapshots (insta), storage actions (totality proof updated in the same commit — chunk 8's contract), outbox/sync pipeline (or the recorded local-only marking), then UI stack creation and CLI verbs. Every commit standalone; crate boundaries strict (domain stays pure — clocks are data, time is injected; UI dispatches, never mutates).
3. Gates green (AGENTS §10) after each milestone, not just at the end.
4. No push, no PR.

Report: per-milestone gate results, snapshot catalogue, deviations from the plan's decisions (any drift = stop and record why).
```

**Prompt 15.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 15 of the remediation-wave-1 dispatch (branch feat/board-stack-structure-editing).

1. Read AGENTS.md, dispatch plan §1/§16 + wave-wide test rules, the four issues, the stage-1 plan; review git diff main...HEAD.
2. Functional focus: clock semantics match the plan per verb (spot-check LWW behavior: a remote edit to an unrelated field must not lose a local board rename and vice versa); tombstone/ceremony verbs cannot orphan stacks/cards silently (walk delete → tombstone → push → remote for one verb end-to-end); the exhaustive-match totality proof covers the new storage actions (adding a hypothetical variant breaks the build); UI stack creation goes command→engine (no AppState mutation in slint code — grep for direct state writes); the sync story per verb matches the plan's recorded local-only/pushable decisions; empty-board and missing-stack edge cases tested; no tokio sleeps; snapshots reviewed for accidental pinning of transient state.
3. Code-quality mandate: in full from §1 (crate boundaries are the main SRP risk in a cross-crate feature — verify domain gained no IO, UI gained no logic).
4. Re-run gates; failure = finding.
5. Fix on branch (amend/reword).
6. Ship: push, PR body summarizing the feature + `Closes #132` + `Closes #171` + `Closes #177` + `Closes #304`, watch to green. Never merge.

Report: verdict, findings, fixes, PR URL, green confirmation.
```

## 17. Completion bookkeeping

- **Issue closure**: every chunk's PR body carries one `Closes #N`
  per covered issue (listed per chunk above) — on merge, GitHub
  closes the issue and it leaves the `remediation-wave-1` milestone.
  No backlog-file edits anywhere in this wave (the backlog file is a
  retired pointer, AGENTS §7).
- **Milestone hygiene**: when the last chunk merges and the milestone
  reads zero open issues, close the milestone; the audit's
  `audit-v0.1.0` label items that remain open (none — all were
  assigned) would be triaged then.
- **Follow-ups opened by this wave** (already tracked, do not
  re-derive): clock-skew mitigation logic (#133) unblocked by chunk
  5's seam; skew + the P2 sync-correctness design round (#128/#129/#130/#150)
  are the next wave's core; the kiosk wave's prep (#262/#263/#299/#300)
  runs independently; #303's accepted-risk trigger (overflow-checks
  firing in the field) opens the full slice/arithmetic audit chunk.
- **Deliberately not dispatched here**: as listed in §Status —
  design-round inputs, product directions, perf/DX P3s, and the
  deferred variants named per chunk (#295's workflow_dispatch keyring
  job, #255's app one-off if deferred by its plan, #301's per-tag
  regeneration cadence beyond the initial promotion).
