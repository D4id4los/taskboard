# [Plan] Reminders Implementation — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-12
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the three implementation chunks of the
  approved reminders design round (WI-I follow-ups). No code in this
  commit.
- **Origin**: `.artifacts/plans/2026-10-12-reminders-design-plan.md`
  §14 (approved direction, 2026-10-12) and the user's chunking
  decision (3 dispatches, not one mega-branch).
- **Status**: READY — dispatch chunks 1 → 2 → 3 **serially** (or
  stacked, see §1). Plan 5 of the design round (desktop toasts +
  interactive snooze surface) is deliberately **not** dispatched here.

## 1. Chunks & ordering

| Chunk | Design plans | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | plan 1 | `feat/reminders-domain-model` | domain model, pure evaluation, `PersistenceAction` variants, proptests P1–P8 | M |
| 2 | plans 2 + 3 | `feat/reminders-engine-storage` | engine tick arm + `SnoozeReminder`, migration + repo, fake parity | M |
| 3 | plan 4 | `feat/reminders-app-wiring` | bootstrap wiring, `[reminders]` config, `LogSink`, `tasks remind` verbs, backlog ✅ | M |

**Ordering**: serial is the default — dispatch chunk N+1 only after
chunk N's PR has merged, so every branch starts from a `main` that
already contains its prerequisites and no PR is ever stacked (no
phantom-conflict rebase dance). If the user chooses to stack instead
(dispatch before merge), every branch targets `main`, each stage-3
agent expects the §8 rebase dance after an upstream merge
(`git rebase --onto origin/main <landed-tip> <branch>` +
`--force-with-lease`; see the 2026-10-12 backlog-batch dispatch plan
§2 for the full procedure).

**Shared execution model** (same as the backlog-batch dispatch plan
§2, binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` next to the project root (AGENTS §8) on
   a new branch off the current `main`, writes the TDD-first
   implementation plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. The
   design round §4/§5/§6/§9 sections are normative input, not
   suggestions — deviations need a recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), implements per the
   approved plan, every commit standalone (rebase-merge hygiene),
   runs the full gate suite (AGENTS §10: fmt, clippy `-D warnings`,
   nextest, doc-tests, deny, llvm-cov) and makes it green. Does
   **not** push or open a PR. Reports gate results and deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   its base against the chunk's acceptance criteria + AGENTS rules
   **plus the code-quality mandate below**, re-runs the gates,
   implements fixes for every issue found on the branch (amend/reword,
   never duplicate-subject fixups). Its last task is shipping: push
   the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`.

**Code-quality mandate (stage 3, every chunk — user-directed):**
beyond functional review, the stage-3 agent evaluates:

- **Code quality**: naming, comment discipline (comments state
  constraints, not narration), idiomatic use of the approved stack,
  no dead code or speculative generality, test quality per AGENTS §5.
- **Single Responsibility Principle**: every module, actor, function,
  and type has exactly one reason to change; domain stays pure (no
  IO, no runtime), the engine orchestrates without legislating, the
  notifier only delivers, storage only persists — the crate-role
  boundaries of AGENTS §2 are the SRP line, and a violation of the
  boundary is a violation of SRP.
- **Single level of abstraction per function body**: every function
  body works at one level — a function either orchestrates
  (calls named operations) or implements (works in primitive data),
  never mixes; guard clauses and the happy path at the top level,
  detail pushed into named private functions. A body that interleaves
  `chrono` arithmetic with channel sends and SQL-adjacent detail is a
  finding, even if it passes tests.

Findings from the mandate are fixed on the branch like any other
finding (refactor commits with proper `refactor(<module>):` subjects,
or amend where the commit is not yet shipped).

## 2. Chunk 1 — Domain model (`feat/reminders-domain-model`)

- **Covers**: design round plan 1 — `taskboard-domain/src/reminders.rs`
  (entities `ReminderRule`/`ReminderDelivery`/`ReminderSnooze`,
  `ReminderRef`, `ReminderDefaults`, `ReminderState`), pure functions
  (`occurrence_of`, `effective_occurrence`, `due_firings`,
  `overdue_tasks`, `due_soon`), the additive `PersistenceAction`
  variants and their one-semantics `apply_actions` definitions
  (incl. snooze-chain compaction and mute semantics), `AppState`
  grows `reminders: ReminderState` (insta snapshot update once),
  proptests P1–P8, serde roundtrips. **No outbox ops, no engine
  changes, no storage crate changes** (the sqlite repo's
  `apply`/`load` do change — the shared-semantics parity requires it;
  the migration does not happen in this chunk).
- **Acceptance**: P1–P8 green as proptests; existing insta snapshots
  updated exactly once and only by the mechanical
  `AppState.reminders` addition; `cargo mutants` targets reachable
  (pure functions in domain); gates green; no reminder data enters
  any outbox path.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the reminders implementation ("Domain model", branch feat/reminders-domain-model) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the approved design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§4 domain model is normative: entities, evaluation semantics, P1–P8, D-a…D-e; §9 persistence actions; §13 resolved decisions) and the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§2.
2. Read the current contracts you will extend: crates/taskboard-domain/src/{entities.rs,state.rs,persistence.rs,messages.rs,clock.rs} and crates/taskboard-storage-sqlite/src/{repo.rs,codec.rs} (read-side parity only — no migration in this chunk).
3. Create the worktree ../taskboard-WT-feat-reminders-domain-model on a new branch feat/reminders-domain-model off main.
4. Write a detailed TDD-first implementation plan at .artifacts/plans/<today>-reminders-domain-model-plan.md in the house [Plan] style: module layout, exact entity/function signatures, the PersistenceAction variants with their apply_actions semantics (delivery = set-union no-op on replay, snooze link replaces per occurrence, chain compaction rule, mute), how repo apply/load gain the shared semantics (fake + sqlite parity), the insta snapshot update, the P1–P8 proptest designs, and the implementation/commit order. Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): reminders domain model. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the reminders implementation ("Domain model").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§4, §9, §13), the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§2, and the stage-1 chunk plan (.artifacts/plans/*reminders-domain-model*-plan.md) in the worktree ../taskboard-WT-feat-reminders-domain-model (branch feat/reminders-domain-model). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD (proptests P1–P8 first where practical), every commit standalone (rebase-merge hygiene). The domain crate stays pure: no IO, no async runtime, no tokio; #![forbid(unsafe_code)] and the MIT OR Apache-2.0 SPDX header on new files.
3. Run the full gate suite from AGENTS.md §10 and make it green (cargo llvm-cov included; insta snapshots reviewed, not blindly accepted).
4. Do NOT mark the backlog origin entry done — it stays open until chunk 3 lands. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, proptest case counts, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the reminders implementation (branch feat/reminders-domain-model).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§4, §9, §13), the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§2 (including the code-quality mandate — it applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-reminders-domain-model.
2. Functional focus: P1–P8 present as proptests and meaningful (a mutant in due_firings would fail them); delivery/snooze/mute semantics defined ONCE in apply_actions and inherited by the sqlite repo and fakes — grep for any duplicated semantics; no reminder data enters outbox/LocalOp paths; the insta snapshot delta is exactly the mechanical reminders addition; sync-readiness commitments preserved (portable ids, additive occurrence-keyed rows, no single-device assumptions — design round §15 checklist).
3. Code-quality mandate: evaluate (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle — domain pure (no IO/runtime), storage only persists, engine untouched by this chunk, every type one reason to change; (c) single level of abstraction — every function body either orchestrates named operations or works in primitives, never mixes; mixed-level bodies are findings even when tests pass.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword per commit hygiene — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings (functional + quality mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — Engine scheduler + storage (`feat/reminders-engine-storage`)

- **Covers**: design round plans 2 + 3 — the engine's fourth
  `select!` arm (`sleep_until(next_tick)`, re-arming, drain-aware,
  `biased` order commands → reports → system → timer), the tick path
  (one `clock.now()` read → pure `due_firings` → `RecordReminderDelivery`
  batch → apply → interpret → publish → notify), the `SnoozeReminder`
  and mute `StateCommand`s, the injected
  `ReminderEngineConfig { enabled, tick, defaults }` (default-off in
  every existing test helper — the auto-advance guard test is
  mandatory), `spawn_state_engine`'s notifier `mpsc::Sender<Notification>`
  (closed-channel = `debug!` no-op, `forward_sync_now` precedent),
  migration `0004_reminders.sql` (4 tables, additive ADR 0005 policy,
  next free number — check whether WI-A's 0003 landed), sqlite
  batch/load support, fake parity. Pause/advance tests only — no
  `tokio::time::sleep` in tests.
- **Acceptance**: ticks fire due reminders at-most-once per
  occurrence with the delivery marker persisted before the
  notification send; snooze reschedules through the same evaluator;
  drain ignores timer wakes; the existing engine/sync suites pass
  unchanged with reminders disabled (the auto-advance guard pins
  this); migration upgrade path tested the way 0002's upgrade test
  does; gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the reminders implementation ("Engine scheduler + storage", branch feat/reminders-engine-storage) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the approved design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§5 scheduler — normative, Option A approved; §6 notification payload/port seam; §9 persistence/storage shape; §13 decisions D2/D3/D4/D5) and the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§3.
2. Read the current engine and sync-actor timing patterns: crates/taskboard-state/src/{actor.rs,core.rs,error.rs}, crates/taskboard-sync-nextcloud/src/actor.rs (sleep_until/reschedule pattern), crates/taskboard-storage-sqlite/src/{repo.rs,migrations/}, and one existing migration upgrade test.
3. Create the worktree ../taskboard-WT-feat-reminders-engine-storage on a new branch feat/reminders-engine-storage off main (if you stack instead of serializing: off chunk 1's branch — per dispatch plan §1).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-reminders-engine-storage-plan.md: the tick arm's exact select! integration and re-arm logic; the tick-path function decomposition (orchestrate vs. implement levels); SnoozeReminder + mute command planning in the domain; ReminderEngineConfig injection and the default-off test-helper policy; the mandatory auto-advance guard test design; the notifier channel + spawn-signature change; migration number check and SQL; repo batch/load extension + codec; the pause/advance test catalogue (fire, at-most-once, snooze re-fire, drain silence, closed notifier channel, storage-failure fires nothing). Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): reminders engine scheduler and storage. Do not implement; do not push.

Report: plan path, decisions, risks (esp. the auto-advance guard), estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the reminders implementation ("Engine scheduler + storage").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§5, §6, §9, §13), the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§3, and the stage-1 chunk plan (.artifacts/plans/*reminders-engine-storage*-plan.md) in the worktree ../taskboard-WT-feat-reminders-engine-storage (branch feat/reminders-engine-storage). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Time manipulation in tests exclusively via tokio::time::pause/advance (start_paused) and injected clocks — never sleep. New .rs files carry the correct SPDX header (taskboard-state/storage: MIT OR Apache-2.0) and forbid unsafe per crate convention.
3. Refresh the sqlx offline cache via scripts/sqlx-prepare.sh if compile-time queries changed; sqlx-prepare workflow documented in the plan.
4. Run the full gate suite from AGENTS.md §10 and make it green; insta snapshots reviewed, not blindly accepted.
5. Do NOT mark the backlog origin entry done — chunk 3 lands it. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the auto-advance guard test result, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the reminders implementation (branch feat/reminders-engine-storage).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§5, §6, §9, §13), the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§3 (including the code-quality mandate — it applies to you), and the stage-1 chunk plan; then review the full branch diff in ../taskboard-WT-feat-reminders-engine-storage (git diff against its base — main, or chunk 1's branch if stacked).
2. Functional focus: delivery marker persisted BEFORE the notification send (grep the tick path — ordering is a finding, not a style note); no test uses tokio::time::sleep; the auto-advance guard test exists and the pre-existing engine/sync suites are byte-identical (diff every touched test — the WI-C hard rule); drain ignores timer wakes; migration is additive per ADR 0005 with a tested upgrade path and the next free number; fake + sqlite parity rides the shared apply_actions (no duplicated semantics).
3. Code-quality mandate: evaluate (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5 (typed outcomes, no text assertions, terminal-state assertions); (b) the Single Responsibility Principle — the engine orchestrates and legislates nothing new (semantics live in the domain), the tick arm is the only timer, the notifier only delivers, storage only persists, no crate boundary crossed backwards; (c) single level of abstraction — every function body either orchestrates named operations or works in primitives, never mixes; the tick arm's happy path reads as orchestration with detail pushed into named private functions; mixed-level bodies are findings even when tests pass.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + quality mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — App wiring, config, CLI (`feat/reminders-app-wiring`)

- **Covers**: design round plan 4 — bootstrap wires the notifier
  channel + `LogSink` (the v1 `tracing::info!` sink; `DesktopSink`
  is explicitly out of scope here), the `[reminders]` figment table
  (`enabled`, `tick_secs`, `default_before` — defaults per §5.4,
  strict-table rule), `tasks remind` verbs
  (`<selector> [--before <dur>]`, `--list`, `--clear`, `--off`)
  with selector cascade reuse and `--json` typed projections
  (kebab-case error envelopes, ADR 0009), `tasks list`
  overdue/due-soon marker column, `CommandOutcome` additions,
  end-to-end hermetic test (fake sink at engine level, tempdir db,
  paused clock). **Final commit**: mark the backlog origin entry
  "[2026-10-08] Reminders/notifications subsystem" ✅ DONE with a
  one-line note.
- **Acceptance**: a hermetic E2E proves rule → tick → delivery →
  notification on the fake sink; config absent = defaults;
  `TASKBOARD_REMINDERS_*` overrides work; gates green; backlog entry
  ✅ DONE.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the reminders implementation ("App wiring, config, CLI", branch feat/reminders-app-wiring) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the approved design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§6 NotificationSink + LogSink; §5.4 config; §11 CLI sketch; §13 D3/D5) and the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§4.
2. Read the current app seams: crates/taskboard-app/src/{bootstrap.rs,config.rs,commands/} (verb handler + --json envelope patterns), the clap CLI definition, and an existing hermetic verb test to mirror.
3. Create the worktree ../taskboard-WT-feat-reminders-app-wiring on a new branch feat/reminders-app-wiring off main (if stacked: off chunk 2's branch — per dispatch plan §1).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-reminders-app-wiring-plan.md: bootstrap notifier channel + LogSink wiring; the [reminders] config struct (figment defaults → taskboard.toml → TASKBOARD_* env; strict tables; duration decoding) and how it reaches ReminderEngineConfig; the tasks remind verb surface (arg parsing, selector cascade reuse, typed outcomes, --json projections, kebab-case error envelopes); the tasks list marker column; the hermetic E2E test design (fake sink, tempdir, paused clock); the exact backlog ✅ DONE final commit. Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): reminders app wiring, config and CLI. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the reminders implementation ("App wiring, config, CLI").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§5.4, §6, §11, §13), the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§4, and the stage-1 chunk plan (.artifacts/plans/*reminders-app-wiring*-plan.md) in the worktree ../taskboard-WT-feat-reminders-app-wiring (branch feat/reminders-app-wiring). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hermetic tests only (tempdirs, env, fake sink, paused clock — no network, no TTY, no sleeps). New .rs files carry the AGPL-3.0-only SPDX header (app crate).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark the backlog entry "[2026-10-08] Reminders/notifications subsystem" ✅ DONE with a one-line note (the four WI-I deferred-idea entries stay as they are).
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the reminders implementation (branch feat/reminders-app-wiring).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-reminders-design-plan.md (§5.4, §6, §11, §13), the dispatch plan .artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md §1/§4 (including the code-quality mandate — it applies to you), and the stage-1 chunk plan; then review the full branch diff in ../taskboard-WT-feat-reminders-app-wiring (git diff against its base).
2. Functional focus: LogSink only (no notify-rust dependency appears); config absent = documented defaults, TASKBOARD_* overrides honored, strict tables enforced; CLI verbs reuse the selector cascade and typed outcomes with kebab-case --json envelopes; the E2E test is hermetic and asserts terminal state + the fake sink's payload set (no text assertions, AGENTS §5); the backlog origin entry is marked ✅ DONE in the final commit; the four deferred-idea backlog entries are untouched.
3. Code-quality mandate: evaluate (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle — the CLI layer only parses/dispatches (no business logic), config only resolves (no reminders knowledge beyond the table shape), bootstrap only wires (channel halves created there, ADR 0008), LogSink only delivers; (c) single level of abstraction — every function body either orchestrates named operations or works in primitives, never mixes; verb handlers read as dispatch + typed outcome, with detail in named helpers; mixed-level bodies are findings even when tests pass.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + quality mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 5. Completion bookkeeping

- Chunk 3's stage-2 final commit marks the origin backlog entry
  ("[2026-10-08] Reminders/notifications subsystem") ✅ DONE.
- The four deferred-idea entries added by the design round (reminder
  sync, insistence levels, multi-reminder UX, delivery pruning) stay
  open — they are future rounds, not part of these chunks.
- After chunk 3 merges, the user may optionally prune/archive
  completed backlog entries per the backlog's archive policy.
