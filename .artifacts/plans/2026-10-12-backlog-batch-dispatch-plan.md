# [Plan] Backlog Triage Batch — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-12
- **Kind**: dispatch plan — no code in this commit. It records the
  2026-10-12 full-backlog triage outcome and carries the copy-pasteable
  prompts for the user's 3-stage agent process (planning →
  implementation → review) per item.
- **Origin**: backlog review session 2026-10-12; facts verified against
  `main` @ `12b4819` and
  `.artifacts/reports/2026-10-11-m1-exit-pre-gui-analysis-report.md`.
- **Status**: READY — dispatch per the waves in §3.

## 1. Scope

| ID | Item | Branch | Kind | Size |
|----|------|--------|------|------|
| WI-A | Outbox `board_id` column — kill the `RenameBoard` StackId cast | `fix/storage-outbox-board-id` | code | M |
| WI-B | MSRV CI job + float policy | `ci/msrv-job` | CI | S |
| WI-C | Make the Miri weekly job run green in budget | `ci/miri-tuning` | CI | M |
| WI-D | App config DX: XDG write-target flip + `CommandError::Config` kinds | `feat/app-config-dx-followups` | code | S/M |
| WI-F | `tracing` pass for `taskboard-domain` | `feat/domain-tracing` | code/docs | M |
| WI-G | REPL tab-completion | `feat/app-repl-completion` | code | M |
| WI-H | `logout` vs running daemon (gated on semantics pick) | `feat/app-logout-daemon-liveness` | code | M |
| WI-I | Reminders design round | `docs/reminders-design` | docs only | M |
| WI-J | Connector attach + session-level login design round | `docs/connect-flow-design` | docs only | M |

Deliberately **not** in this batch (triggers unmet): merge-policy
refinements (field-level merge, add-wins labels, resurrect policy,
clock-skew bias — wait for observed M1 real-server symptoms), the
performance family (measured need), tombstone purge (db growth),
OAuth2, multi-board, encrypted-file store, board delete/unbind
ceremonies, undo/redo, org-mode, IPC v2 + the grouped
signal/cancellation design round, release-pipeline extras, Nextcloud
major-version matrix, Deck-client logging-arm coverage. Flaky
watchlist: 1 entry — below the ~5 review threshold.

## 2. Execution model (applies to every item)

The user's process, three agents per item, each in its own fresh
session:

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` (next to the project root, AGENTS §8) on
   a new branch off `main`, writes a detailed implementation plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style (see existing plans), TDD-first, **resolving every open
   question of the item's §4 section with a recorded decision**,
   commits it (`docs(plan): …`). No implementation, no push.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), implements per the
   approved plan, every commit standalone (rebase-merge hygiene),
   runs the full gate suite (AGENTS §10: fmt, clippy `-D warnings`,
   nextest, doc-tests, deny, llvm-cov), marks the item's backlog
   entry ✅ DONE in the final commit. Does **not** push or open a PR
   (WI-C's documented exception aside). Reports gate results,
   deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   `main` against the item's acceptance criteria + AGENTS rules,
   re-runs the gates, **implements fixes for every issue found** on
   the branch (amend/reword, never duplicate-subject fixups). Its
   **last task is shipping**: push the branch, open the PR to `main`
   with `gh`, then watch the CI jobs to completion
   (`gh pr checks --watch`) — any failing check is a finding:
   diagnose, fix on the branch, push `--force-with-lease`, repeat
   until every check is green. Reports a verdict. Never merges.

Authorization granted by this dispatch: the stage-3 agent pushes the
feature branch, opens and updates its PR, and iterates until CI is
green. Stage 2 stays local by default; the single exception is WI-C,
whose stage-2 agent may push the branch (never a PR) because its
workflow_dispatch iteration needs the branch on the remote.
**Nothing ever pushes to or merges into `main`** — the user reviews
and merges.

Conflict expectations (land serially or accept trivial rebases):
WI-D ∩ WI-G both touch `taskboard-app` (different files); WI-B ∩ WI-C
both touch `.github/workflows/` (different files); **every** item's
final commit edits `.artifacts/taskboard-potential-improvements-backlog.md`,
and the design rounds append to it too.

## 3. Waves

- **Wave 1 (dispatch together, independent)**: WI-A, WI-B, WI-C, WI-D,
  WI-F.
- **Wave 2 (after wave 1 lands, or anytime if you accept trivial
  rebases)**: WI-G; WI-H (only after the §4 semantics pick).
- **Anytime, docs-only, serialize their docs/backlog commits**: WI-I,
  WI-J (stage 1 only until you approve the direction).

## 4. Work items

### WI-A — Outbox `board_id` column (the `RenameBoard` StackId cast)

- **Origin**: backlog "[2026-10-11] Outbox `board_id` column for
  board-scoped ops".
- **Current state (verified on `main` @ `12b4819`)**:
  `LocalOp::RenameBoard` persists its `BoardId` by casting through a
  `StackId` into the outbox `stack_id` column — write side
  `crates/taskboard-storage-sqlite/src/codec.rs:101-108`, read side
  `codec.rs:151-153`. Outbox schema (`migrations/0001_init.sql:86-94`):
  `op_seq, op_id, op_kind, task_id, stack_id, label_id, queued_at` —
  no board column. Migrations shipped: 0001, 0002.
- **Task**: migration `0003_outbox_board_id.sql` per ADR 0005's
  additive policy — add `board_id TEXT NULL`; backfill
  `UPDATE outbox SET board_id = stack_id WHERE op_kind = 'rename_board'`.
  Codec: `OpColumns` gains `board_id`; the `rename_board` tag uses the
  board column in both directions; the cast and its workaround
  comment die. Urgency: the open board color/archive and unbind
  entries would each widen the hack — this is the cheap window.
- **Open questions for the planner**: (1) how migration 0002 tested
  upgrade-from-0001 — reuse that harness; ideally cover a fixture db
  with a queued `rename_board` row surviving the backfill;
  (2) whether to NULL `stack_id` on `rename_board` rows after the
  backfill; (3) the sqlx offline workflow — CI pins `SQLX_OFFLINE=true`;
  if compile-time macros are involved, refresh prepare data the way
  0002 did; (4) whether `OpColumns` keeps
  `#[allow(clippy::struct_field_names)]`.
- **Acceptance**: no `BoardId`↔`StackId` casts remain anywhere in the
  workspace; roundtrip test extended through the board column; full
  gates green; backlog entry ✅ DONE.

**Prompt A1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-A ("Outbox board_id column — kill the RenameBoard StackId cast") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read /home/viktoria/src/taskboard/AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 (execution model) and §4 WI-A (context, constraints, open questions, acceptance).
2. Create the worktree ../taskboard-WT-fix-storage-outbox-board-id on a new branch fix/storage-outbox-board-id off main.
3. Write a detailed implementation plan at .artifacts/plans/<today>-storage-outbox-board-id-plan.md in the house [Plan] style (see existing plans), TDD-first, resolving every WI-A open question with a recorded decision.
4. Commit it as: docs(plan): storage outbox board_id column — rename_board stops riding stack_id. Do not implement; do not push.

Report: plan path, the decision for each open question, risks, estimated diff size.
```

**Prompt A2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-A ("Outbox board_id column").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2/§4 WI-A, and the stage-1 item plan (.artifacts/plans/*storage-outbox-board-id*-plan.md) in the worktree ../taskboard-WT-fix-storage-outbox-board-id (branch fix/storage-outbox-board-id). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark the backlog entry "[2026-10-11] Outbox `board_id` column for board-scoped ops" ✅ DONE with a one-line note.
5. Do not push and do not open a PR — shipping the branch (push, PR, CI watch) is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt A3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-A (branch fix/storage-outbox-board-id).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-A (acceptance criteria), and the item plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-fix-storage-outbox-board-id.
2. Focus: migration + backfill correctness (incl. the upgrade-from-0002 path); zero BoardId↔StackId casts left (grep the workspace); test quality per AGENTS §5 (typed outcomes, no text assertions, no weakened tests); commit hygiene; backlog bookkeeping.
3. Re-run the full gate suite yourself; any failure is a review finding.
4. Fix every issue directly on the branch (amend/reword per commit hygiene — no duplicate-subject fixups).
5. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

### WI-B — MSRV CI job + float policy

- **Origin**: backlog "[2026-10-07] MSRV policy and CI job".
- **Context**: `rust-version = "1.96"` is declared per crate but
  nothing verifies it; accidental newer-stdlib use would only fail on
  user toolchains.
- **Task**: add a `check (msrv)` job to `.github/workflows/ci.yml`
  (`dtolnay/rust-toolchain@1.96` + `cargo check --workspace
  --all-features --locked`; exact flag set and whether cargo-deny
  rides along is the planner's call). Decide + document the float
  policy (recommended: MSRV moves only by explicit bump, recorded in
  ADR 0003 or `docs/architecture.org`).
- **Acceptance**: the job is green on the PR's own CI run; the policy
  sentence lands in docs; `miri.yml` untouched (WI-C owns it);
  backlog entry ✅ DONE.

**Prompt B1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-B ("MSRV CI job + float policy") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-B.
2. Create the worktree ../taskboard-WT-ci-msrv-job on a new branch ci/msrv-job off main.
3. Write a detailed implementation plan at .artifacts/plans/<today>-ci-msrv-job-plan.md (house [Plan] style): job shape, trigger scope (push/PR), flag set, where the float policy is recorded, and how the job's own PR run validates it.
4. Commit it as: docs(plan): MSRV CI job and float policy. Do not implement; do not push.

Report: plan path, job YAML sketch, policy wording, risks.
```

**Prompt B2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-B ("MSRV CI job").

1. Read AGENTS.md, the dispatch plan §2/§4 WI-B, and the stage-1 item plan (.artifacts/plans/*ci-msrv-job*-plan.md) in ../taskboard-WT-ci-msrv-job (branch ci/msrv-job). Recreate the worktree on that branch if missing.
2. Implement the ci.yml job and the docs policy line exactly per the plan.
3. Gates: the workflow YAML must pass actionlint if the repo uses it (check), plus the standard suite where applicable.
4. Final commit: mark the backlog entry "[2026-10-07] MSRV policy and CI job" ✅ DONE.
5. Do not push and do not open a PR — shipping the branch (push, PR, CI watch with the msrv job green) is stage 3's last task. Never push to or merge main.

Report: what was implemented, deviations.
```

**Prompt B3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-B (branch ci/msrv-job).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-B, and the item plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-ci-msrv-job.
2. Focus: the job actually pins 1.96 (not latest); check flags match the plan; policy recorded where the plan says; no unrelated workflow churn.
3. Fix every issue directly on the branch.
4. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch) — the new msrv job itself must be green before handoff; any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, the green msrv run URL. Never merge; never touch main.
```

### WI-C — Make the Miri weekly job run green in budget

- **Origin**: backlog "[2026-10-09] Miri CI tuning: verify and time-cap
  the weekly job" (+ "[2026-10-09] Miri + proptest isolation friction"
  — decide whether it is subsumed and mark it accordingly).
- **Context**: the weekly job has **never run green**; proptest under
  the interpreter is the cost. Local data: A-series 12/12 in ~9 s
  interpreted; a single P-equivalence case exceeded 9 min; domain
  suite ~45 min at 16 cases. In-tree Miri gates already exist
  (`test_support::proptest_config` disables failure-persistence under
  `cfg!(miri)`; `#[cfg_attr(miri, ignore)]` on the insta S-series and
  `sqlite_smoke`).
- **Task**: ensure `miri.yml` has `workflow_dispatch`; iterate from
  the branch until a full run is green inside the 120-min budget: cap
  `PROPTEST_CASES` for the job (proptest reads the env natively),
  consider splitting domain/state into parallel jobs; if throughput
  still cannot fit, scope the job's test filter and document the exact
  scope in `docs/testing_strategy.org` §10.
- **Hard rule (non-negotiable)**: every reduction is an
  environment/speed gate — never a weakened assertion; no test is
  deleted or loosened to make the job fit.
- **Acceptance**: at least one green, in-budget run of the final
  config (run URL in the PR); testing_strategy §10 documents the
  scope; no test logic changed; backlog entries updated.

**Prompt C1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-C ("Miri weekly job: run green in budget") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-C (includes local timing data and the hard rule).
2. Create the worktree ../taskboard-WT-ci-miri-tuning on a new branch ci/miri-tuning off main.
3. Write the iteration plan at .artifacts/plans/<today>-ci-miri-tuning-plan.md: candidate config ladder (PROPTEST_CASES caps → job split → filter scope), how each step is validated via workflow_dispatch from the branch, the budget math, and what gets documented in testing_strategy.org §10.
4. Commit it as: docs(plan): Miri weekly job — green within the 120-minute budget. Do not implement; do not push.

Report: plan path, the config ladder, risks.
```

**Prompt C2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-C ("Miri weekly job tuning").

1. Read AGENTS.md, the dispatch plan §2/§4 WI-C, and the stage-1 item plan (.artifacts/plans/*ci-miri-tuning*-plan.md) in ../taskboard-WT-ci-miri-tuning (branch ci/miri-tuning). Recreate the worktree on that branch if missing.
2. Iterate per the plan's ladder: adjust .github/workflows/miri.yml, drive workflow_dispatch runs from the branch (gh run watch), and converge on a green full run inside the 120-minute budget. The hard rule from §4 WI-C applies: environment/speed gates only — never weaken or delete an assertion.
3. Document the final scope in docs/testing_strategy.org §10.
4. Final commit: update the "[2026-10-09] Miri CI tuning" backlog entry ✅ DONE (and the isolation-friction entry if subsumed).
5. Pushing the branch is authorized for this item only to feed workflow_dispatch iteration (the §2 WI-C exception) — never open a PR; the PR and CI watch are stage 3's last task. Never push to or merge main.

Report: green in-budget run URL, final config summary, deviations.
```

**Prompt C3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-C (branch ci/miri-tuning).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-C, and the item plan; review the full PR diff in ../taskboard-WT-ci-miri-tuning.
2. Focus: no test file lost assertions or coverage (diff every touched test — the hard rule); the green run stage 2 linked used the final config and finished in budget; testing_strategy §10 matches the shipped scope; workflow YAML sanity.
3. Fix every issue directly on the branch. If the config changed during review, drive a fresh workflow_dispatch run yourself and re-verify green-in-budget.
4. Last task — ship: push the branch (--force-with-lease if updating the stage-2 push), open the PR to main with gh — the description must link the green in-budget run — then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, green in-budget run URL. Never merge; never touch main.
```

### WI-D — App config DX follow-ups (two standalone commits)

- **Origin**: backlog "[2026-10-11] XDG write-target flip for `login`"
  and "[2026-10-11] `CommandError::Config` kind refinement".
- **D1 — XDG write-target flip**: today `login` on a machine with no
  config anywhere writes `./taskboard.toml`. The read side already
  resolves `--config` > `TASKBOARD_CONFIG` > XDG-when-present
  (`crates/taskboard-app/src/config.rs:278-290`; helpers
  `xdg_candidate`/`xdg_if_present` at `config.rs:192-199`). Flip the
  write: when no config exists anywhere, persist to
  `dirs::config_dir()/taskboard/taskboard.toml` (create the
  directory). Existing installs' resolution must not change.
  Planner decides + pins in tests: what `login` writes when
  `TASKBOARD_CONFIG` points at a *missing* file (write there vs XDG).
- **D2 — Config error kinds**: the JSON error envelope collapses every
  config failure to `"config"` (`commands/mod.rs:225`) while
  `ConfigError::kind()` (`config.rs:242-251`) already distinguishes
  `file-not-found`/`decode`/`invalid-server-url`/`empty-username`/
  `invalid-duration`/`empty-db-path`. Surface the specific kinds in
  `error_kind()`. Sanctioned pre-v0.1.0: `--json` payloads are
  explicitly unversioned within M1 (versioned-schema backlog status
  note). Planner sweeps JSON consumers/tests for impact.
- **Acceptance**: hermetic tests (tempdirs + env, no network), typed
  outcome assertions per AGENTS §5; both backlog entries ✅ DONE.

**Prompt D1-stage1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-D ("App config DX: XDG write-target flip + CommandError::Config kinds") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-D.
2. Create the worktree ../taskboard-WT-feat-app-config-dx-followups on a new branch feat/app-config-dx-followups off main.
3. Write a detailed plan at .artifacts/plans/<today>-app-config-dx-followups-plan.md: two standalone commits (D1 XDG write flip, D2 config error kinds), each TDD-first; resolve the D1 missing-env-file semantics and the D2 consumer/test impact sweep with recorded decisions.
4. Commit it as: docs(plan): app config DX follow-ups — XDG write target + config error kinds. Do not implement; do not push.

Report: plan path, decisions, risks.
```

**Prompt D2-stage2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-D ("App config DX follow-ups").

1. Read AGENTS.md, the dispatch plan §2/§4 WI-D, and the stage-1 item plan (.artifacts/plans/*app-config-dx-followups*-plan.md) in ../taskboard-WT-feat-app-config-dx-followups (branch feat/app-config-dx-followups). Recreate the worktree on that branch if missing.
2. Implement per the plan as two standalone commits (D1 then D2), TDD, hermetic tests only.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark both origin backlog entries ("XDG write-target flip for login", "CommandError::Config kind refinement") ✅ DONE.
5. Do not push and do not open a PR — shipping the branch (push, PR, CI watch) is stage 3's last task. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt D3-stage3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-D (branch feat/app-config-dx-followups).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-D, and the item plan; review the full PR diff in ../taskboard-WT-feat-app-config-dx-followups.
2. Focus: existing installs' config resolution is unchanged (D1 must only affect the no-config-anywhere write path); error-kind change is complete across commands and consistent with ConfigError::kind(); no text-based test assertions (AGENTS §5); both commits stand alone; backlog bookkeeping.
3. Re-run the full gate suite; any failure is a finding.
4. Fix every issue directly on the branch (amend/reword, no duplicate-subject fixups).
5. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

### WI-F — `tracing` pass for `taskboard-domain`

- **Origin**: backlog "[2026-10-09] Logging statements for
  taskboard-domain".
- **Context (verified)**: zero `tracing` in `taskboard-domain` today;
  the entry's own trigger ("before or during Phase 3 work") long
  passed, and M1 real-server use needs bug-report localisation
  through the pure core (coding guidelines §4).
- **Task**: add `tracing` to the domain crate (workspace-pinned;
  planner decides exact feature inheritance — the workspace pin
  carries `features = ["log"]`, the entry said "no features" — and
  documents the choice) plus a one-line amendment to ADR 0004 or
  `docs/architecture.org` relaxing the domain dependency cap. Then
  instrument the public entry points: `trace!` at each merge primitive
  and `apply_sync_report` (identities + chosen outcome per rule,
  typed values only, user content only at `trace`), `debug!` for rule
  decisions (R1–R9 as enum/variant facts, not prose).
- **Constraints**: no log-text assertions anywhere (AGENTS §5);
  uncovered logging arms are the accepted precedent (see the sync
  crate's coverage-rationale report) — mirror whatever exemption
  convention exists; `cargo deny check` must stay green with tracing's
  license tree in the domain crate.
- **Acceptance**: a `trace`-level filter suffices to follow one
  entity's merge decision end-to-end (demonstrated in the plan, not
  asserted by tests); dep-cap amendment lands; gates green; backlog
  entry ✅ DONE.

**Prompt F1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-F ("tracing pass for taskboard-domain") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-F.
2. Create the worktree ../taskboard-WT-feat-domain-tracing on a new branch feat/domain-tracing off main.
3. Write a detailed plan at .artifacts/plans/<today>-domain-tracing-plan.md: exact span/event inventory per public entry point (merge primitives, apply_sync_report, R1–R9 decisions), the tracing feature decision, the ADR/architecture.org dependency-cap amendment wording, and the coverage-gate interaction.
4. Commit it as: docs(plan): domain crate tracing pass. Do not implement; do not push.

Report: plan path, event inventory size, feature decision, risks.
```

**Prompt F2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-F ("domain tracing pass").

1. Read AGENTS.md, the dispatch plan §2/§4 WI-F, and the stage-1 item plan (.artifacts/plans/*domain-tracing*-plan.md) in ../taskboard-WT-feat-domain-tracing (branch feat/domain-tracing). Recreate the worktree on that branch if missing.
2. Implement per the plan: dependency + docs amendment first (own commit), then the instrumentation (own commit). No log-text assertions in tests; keep the crates pure (no I/O, no runtime deps beyond the amended cap).
3. Run the full gate suite from AGENTS.md §10 (cargo deny included) and make it green.
4. Final commit: mark the backlog entry "[2026-10-09] Logging statements for taskboard-domain" ✅ DONE.
5. Do not push and do not open a PR — shipping the branch (push, PR, CI watch) is stage 3's last task. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt F3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-F (branch feat/domain-tracing).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-F, and the item plan; review the full PR diff in ../taskboard-WT-feat-domain-tracing.
2. Focus: domain purity preserved (diff Cargo.toml carefully — no incidental deps pulled in); no user content above trace level; no log-text assertions added; the dep-cap amendment actually landed in docs; deny green.
3. Re-run the full gate suite; any failure is a finding.
4. Fix every issue directly on the branch (amend/reword).
5. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

### WI-G — REPL tab-completion

- **Origin**: backlog "[2026-10-11] REPL tab-completion / hints"; its
  stated trigger ("verb surface settles post-WI-3") is met — WI-3
  landed the full editing surface. `rustyline` is already a workspace
  dep of `taskboard-app`.
- **Task**: a rustyline `Helper`: verb/flag completion from the static
  clap CLI shape; board/stack/task-title completion from the live
  `ArcSwap<AppState>` (lock-free read on the editor thread). Completion
  stays strictly read-only (UI-layer rule). Extract candidate
  computation as pure functions testable without a TTY.
- **Acceptance**: pure candidate functions unit-tested with structural
  assertions (no TTY in CI); a manual smoke walkthrough documented in
  the plan; no business logic in the REPL layer; gates green; backlog
  sub-entry ✅ DONE.

**Prompt G1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-G ("REPL tab-completion") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-G.
2. Create the worktree ../taskboard-WT-feat-app-repl-completion on a new branch feat/app-repl-completion off main (rebase first if wave-1 app items already landed — see §3).
3. Write a detailed plan at .artifacts/plans/<today>-app-repl-completion-plan.md: the pure candidate-function seam, how the clap CLI shape is walked for verbs/flags, how live state feeds titles (lock-free ArcSwap read on the editor thread), and the test design.
4. Commit it as: docs(plan): REPL tab-completion. Do not implement; do not push.

Report: plan path, seam design, risks.
```

**Prompt G2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-G ("REPL tab-completion").

1. Read AGENTS.md, the dispatch plan §2/§4 WI-G, and the stage-1 item plan (.artifacts/plans/*app-repl-completion*-plan.md) in ../taskboard-WT-feat-app-repl-completion (branch feat/app-repl-completion). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD on the pure candidate functions; no TTY-dependent tests; completion is read-only.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark the backlog sub-entry "[2026-10-11] REPL tab-completion / hints" ✅ DONE.
5. Do not push and do not open a PR — shipping the branch (push, PR, CI watch) is stage 3's last task. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt G3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-G (branch feat/app-repl-completion).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-G, and the item plan; review the full PR diff in ../taskboard-WT-feat-app-repl-completion.
2. Focus: no business logic or state mutation in the REPL layer; the editor thread only ever reads the ArcSwap (no channel sends, no awaits of engine work); candidate functions genuinely pure and tested; no TTY in CI tests.
3. Re-run the full gate suite; any failure is a finding.
4. Fix every issue directly on the branch (amend/reword).
5. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

### WI-H — `logout` vs running daemon (GATED — pick semantics at plan review)

- **Origin**: backlog "[2026-10-11] `logout` while a daemon runs:
  credential-liveness warning" (documented behavior: ADR 0012).
- **Context**: `logout` is permitted while a daemon serves the db; the
  daemon keeps its booted credentials until restart — honest but
  surprising.
- **Gate — two candidate semantics**: (a) `status` surfaces
  `credentials_revoked: true` when the daemon's store load now fails
  (observation only); (b) `logout` nudges the daemon to drop its
  connector over IPC (reuses the sync actor shutdown path — the
  cleaner semantic; needs a new IPC verb + envelope change). The
  planning agent evaluates both and recommends; **the user picks at
  plan review**; stage 2 implements only the approved one.
- **Acceptance**: chosen semantics implemented and pinned by
  IPC-level tests; envelope version bump if (b) changes the wire; ADR
  0012 amendment; backlog entry ✅ DONE.

**Prompt H1 (planning):**

```text
You are the stage-1 PLANNING agent for work item WI-H ("logout vs running daemon") in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-H (note the GATE: semantics (a) status-observation vs (b) logout-nudges-daemon).
2. Create the worktree ../taskboard-WT-feat-app-logout-daemon-liveness on a new branch feat/app-logout-daemon-liveness off main.
3. Write a plan at .artifacts/plans/<today>-app-logout-daemon-liveness-plan.md that designs BOTH options to decision-ready depth, then recommends one with reasons (wire impact, test surface, kiosk semantics). Do not choose for the user — the plan must state the recommendation prominently for plan-review approval.
4. Commit it as: docs(plan): logout vs running daemon — credential-liveness semantics. Do not implement; do not push.

Report: plan path, recommendation + one-paragraph rationale, risks per option.
```

**Prompt H2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for work item WI-H ("logout vs running daemon").

Precondition: the user has approved ONE semantics option at plan review — noted in the plan file's Status line. If the Status line does not name the approved option, STOP and report instead of implementing.

1. Read AGENTS.md, the dispatch plan §2/§4 WI-H, and the item plan (.artifacts/plans/*app-logout-daemon-liveness*-plan.md) in ../taskboard-WT-feat-app-logout-daemon-liveness (branch feat/app-logout-daemon-liveness). Recreate the worktree on that branch if missing.
2. Implement ONLY the approved option per the plan, TDD (IPC-level tests).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: amend ADR 0012 as the plan specifies and mark the backlog entry ✅ DONE.
5. Do not push and do not open a PR — shipping the branch (push, PR, CI watch) is stage 3's last task. Never push to or merge main.

Report: which option was implemented, per-gate results, deviations.
```

**Prompt H3 (review):**

```text
You are the stage-3 REVIEW agent for work item WI-H (branch feat/app-logout-daemon-liveness).

1. Read AGENTS.md, the dispatch plan §2/§4 WI-H, and the item plan (its Status line names the user-approved option); review the full PR diff in ../taskboard-WT-feat-app-logout-daemon-liveness.
2. Focus: only the approved option is implemented; IPC wire changes version-bumped per ADR 0012 conventions; tests pin the chosen semantics at the IPC level; ADR 0012 amendment consistent with the code.
3. Re-run the full gate suite; any failure is a finding.
4. Fix every issue directly on the branch (amend/reword).
5. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

### WI-I — Reminders design round (docs only — stage 1 only for now)

- **Origin**: backlog "[2026-10-08] Reminders/notifications subsystem" +
  M1-exit pre-GUI report §6 (run the design round **before** the Phase 7
  UI plan is finalized so the UI is not reshaped after reminders land).
- **Task (stage-1 planning agent only)**: a design round covering
  reminder rules as pure domain functions (proptest-able scheduling),
  a scheduler actor on the injected clock (no sleeps; pause/advance
  tests), notification output as a Port, the kiosk overdue-banner
  path, and the `AppState`/`SystemEvent` surface the UI reads.
  Deliverable: a plan doc with a draft ADR inside, ready for user
  review. Stages 2–3 (formalize ADR + docs amendments, then review)
  run only after the user approves the direction; implementation of
  reminders is future work-item plans, not this round.

**Prompt I1 (planning — the design round):**

```text
You are the stage-1 PLANNING agent for work item WI-I ("Reminders/notifications design round") in the taskboard repo at /home/viktoria/src/taskboard. This is a DESIGN round — no production code.

1. Read AGENTS.md (especially §1 identity: "task management AND reminder system", §3 actor protocols, §5 time-manipulation rules), docs/architecture.org, then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-I.
2. Create the worktree ../taskboard-WT-docs-reminders-design on a new branch docs/reminders-design off main.
3. Write the design round at .artifacts/plans/<today>-reminders-design-plan.md: domain model of reminder rules (pure, proptest-able), scheduler actor over the injected clock, notification Port, kiosk overdue-banner path, the AppState/SystemEvent surface the Phase 7 UI will read, and a draft ADR. Anchor every claim in the existing merge/state-engine/sync contracts; list open decisions explicitly for the user.
4. Commit it as: docs(plan): reminders/notifications design round. Do not implement; do not push.

Report: plan path, the 3–5 biggest open decisions, estimated follow-up plan count.
```

### WI-J — Connector attach & session-level login design round (docs only — stage 1 only for now)

- **Origin**: backlog "[2026-10-11] CLI commands to add a Nextcloud
  connection" + "[2026-10-11] `login` inside a running session (REPL /
  GUI)". Stand-alone installs have no CLI path to introduce a
  Nextcloud connection without hand-editing `taskboard.toml`, and the
  Phase 7 GUI will need login-while-running anyway — one design covers
  both (and the REPL's masked-prompt interaction).
- **Task (stage-1 planning agent only)**: one design covering
  (1) connector-management verbs (`connect add`-shape or a relaxed
  `login` accepting a server argument under the auto rule — must
  respect ADR 0010's loudness rule: no silent backend flips),
  (2) the session-level attach flow (bootstrap's `sync:
  Option<SyncActor>` becomes attachable, or a sync-channel rewiring),
  (3) the rustyline masked-secret prompt interaction. Deliverable: a
  plan + draft ADR for user review; stages 2–3 after approval.

**Prompt J1 (planning — the design round):**

```text
You are the stage-1 PLANNING agent for work item WI-J ("Connector attach & session-level login design round") in the taskboard repo at /home/viktoria/src/taskboard. This is a DESIGN round — no production code.

1. Read AGENTS.md, docs/architecture.org, ADRs 0010/0011/0012 (docs/adr/), then .artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md §2 and §4 WI-J.
2. Create the worktree ../taskboard-WT-docs-connect-flow-design on a new branch docs/connect-flow-design off main.
3. Write the design at .artifacts/plans/<today>-connect-flow-design-plan.md covering: connector-management CLI verbs (ADR 0010 loudness rule respected), the session-level attach flow onto a booted core (bootstrap's sync Option becomes attachable or channel rewiring — compare both), the REPL masked-prompt interaction, and how the Phase 7 GUI reuses the same flow. Include a draft ADR and explicit open decisions.
4. Commit it as: docs(plan): connector attach and session-level login design round. Do not implement; do not push.

Report: plan path, the 3–5 biggest open decisions, which existing code the design would touch.
```

## 5. Completion bookkeeping

When an item merges, its final in-branch commit has already marked the
backlog entry ✅ DONE (per-item acceptance criteria). After each wave,
the user may optionally prune completed entries per the backlog's
archive policy — not automated here.
