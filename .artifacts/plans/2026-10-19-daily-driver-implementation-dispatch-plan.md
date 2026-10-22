# [Plan] Daily-Driver Implementation Round — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-19
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 11 implementation chunks of the
  accepted daily-driver wave. No code in this commit.
- **Origin**: the wave implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  — §2 design facts DDF-1…DDF-15, §3 chunks 1–11 (branch names,
  files, behaviours, non-goals), §4 quality gates, §5 sequencing &
  exit criteria (incl. the round gate and the interim rule), §6
  completion bookkeeping, all normative. It dispatches the five
  ACCEPTED design docs that landed via PR #359 with rulings baked
  in: `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`
  (D-S1…D-S9), `2026-10-18-field-family-design-plan.md`
  (D-F1…D-F6), `2026-10-19-reminders-surfaces-design-plan.md`
  (D-R1…D-R13), `2026-10-19-views-finding-design-plan.md`
  (D-V1…D-V10), `2026-10-19-undo-architecture-design-plan.md`
  (D-U1…D-U5 — complete; design only, no chunk). Plus the user's
  2026-10-19 dispatch request.
- **Status**: **GATED — not dispatchable yet.** The round gate
  (implementation plan §Status/§5) is the **multi-board
  implementation round completing**: milestone `wave-multi-board`
  fully closed (#125, #187, #179, #170 — as of 2026-10-19 only its
  chunk 1 landed via PR #358 and #298 is closed). Chunk 1's stage-1
  prompt carries the gate check inline; every later chunk's
  precondition includes its predecessors. **Deliberately not
  dispatched here**: the implementation plan §1 Out list and every
  design doc's backlog-candidates section (§13).

## 1. Chunks & ordering

| Chunk | Impl-plan slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §3 chunk 1 | `feat/domain-task-field-family` | the four fields + clocks + `TaskChanges` slots + guard bools, `Effort`/`Importance`/`Urgency` types, `TASK_FIELDS` manifest + assertions 1–2, migration 0007 (incl. the rule-clock column, codec untouched), native `startdate` end-to-end (DTO + `merge_task` verdict + push shapes + client encode + wire pins), strategies + the Task-widening insta pass; **closes #208** | L |
| 2 | §3 chunk 2 | `feat/domain-day-boundary` | `[preferences] day_start_time` config + typed validation, domain `days.rs::day_window` + proptests; **closes #345** | S |
| 3 | §3 chunk 3 | `feat/sync-appdata-transport` | the dockerized **Deck 1.17.5** contract-fixture gate (attachment surface pins + the 1.17.5 `startdate`-unsupported pin), run green on both tiers via the **capability seam** (2026-10-22 ruling; addendum `2026-10-22-deck-version-capabilities-design-plan.md`: probe + `CARD_STARTDATE` flag, unconditional additive attachment-create `data` field, echo-decode fallback, capability-gated card conversion), anchor ceremony (constants, discovery, lazy creation, adoption), domain `appdata.rs` document codec + union + GC + verbatim preservation (`scheduled` = fourth modeled key), `AppDataEvidence` on `SyncReport::Completed`, `LocalOp::UpdateAppData` arm + codec tag, `LoopState` metadata cache, download-during-pull, manifest assertion 3 (reworded: four modeled keys, `scheduled` emission/fold capability-gated) | L |
| 4 | §3 chunk 4 | `feat/sync-appdata-fold` | `fold_task_app_data` + rules fold in `apply_sync_report`, the guard mirror (`UpdateAppData` arm, value+clock copy), flush between pull and report + rebuild mode + fences, `plan_pushes` document group, `ReminderRule.updated_at` + stamping + storage wiring, enqueue sites (EditTask app slots + rule verbs), per-board sidecar health, CLI `status` warning + the anchor-title CLI guard, manifest assertion 4; **closes #207 + #244** | L |
| 5 | §3 chunk 5 | `feat/app-task-field-surfaces` | `tasks edit` flags + `parse_effort`, `TaskRow` + phrase rendering, REPL completion, UI decoration carriers + phrase helpers, the anchor exact-title filter over list rows/counts, the sidecar-health badge; **closes #209** | M |
| 6 | §3 chunk 6 | `feat/state-reminder-rule-verbs` | retire `SetReminderRule`; `AddReminderRule` (dedup by offset) + `RemoveReminderRule` + kept `ClearReminderRules`; CLI group `--remove` + selectors + receipts; UI task-editor rule-list composition; **closes #204** | M |
| 7 | §3 chunk 7 | `feat/domain-adhoc-alarms` | `ReminderRef::Adhoc` + `AdhocAlarm` + the D-R6 ripple site table (incl. the `ReminderAnchor` payload widening), `due_firings` alarms pass, create/cancel commands + settlement prune, migration 0008, `timers add/list/cancel` CLI+REPL, `ReminderTarget` widening; **closes #211** | M |
| 8 | §3 chunk 8 | `feat/app-desktop-notifications` | `notify-rust` 4.18.x workspace pin (stack granted 2026-10-19), `DesktopSink` behind the unmoved port, `[reminders] sink = auto/log/desktop` with `auto` by `AppMode`, payload-mapping units, `cargo deny` over the new tree; **closes #344** | M |
| 9 | §3 chunk 9 | `feat/domain-find` | domain `find.rs` + the property suite + the crate-swap firewall, top-level `find` CLI/REPL verb + `FindRow`, the palette (`Flow::Palette`, `ContextId::Palette`, `search-palette.slint`, `IdTables.palette`, jump-to), palette anchor filter, journey leg; **closes #348** | M |
| 10 | §3 chunk 10 | `feat/ui-today-inbox` | `days.rs::today_sections` + proptests, `ViewId::Today` + default desktop view + `view.today`/`task.due-today`/`view.inbox` + `ContextId::Today`, `today-view.slint`, `CaptureTask` + the Inbox/Backlog resolution rules + `capture-overlay.slint` + the `capture` verb + `tasks add` Backlog default, the flat cross-board move ring, the inbox chip, anchor filter on today rows/counts, journeys + golden regen; **closes #346 + #347** | L |
| 11 | §3 chunk 11 | `docs/daily-driver-wave` | `docs/architecture.org` + `README.org` amendments, CHANGELOG promotion, the wave exit verification report, the #202 milestone-clear comment | S |

**Dependency graph** (implementation plan §5): default dispatch order
`1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11`, serial, each chunk's
precondition its predecessor's merge. Hard dependencies the order
encodes: 3 needs 1 (the document codec serializes the field types);
4 needs 3; 5 needs 4 (badge reads the health) and 1; 6 needs 4 (the
enqueue/tombstone machinery); 7 needs 6 (the `ReminderRef` ripple
lands once, with the verbs); 8 needs 7 (the payload anchor shape);
9 needs 3 (the palette's anchor filter reads the domain const); 10
needs 1 + 2 + 3 (fields, `day_window`, the anchor const); 11 needs
all. **Parallel options** (user-invocable, never sharing worktrees):
after 4 merges, chunks 5, {6→7→8}, and 9/10 are mutually
order-independent except **9 and 10 share ui `view_state.rs`/
`actions.rs`/`models.rs` and app `cli.rs`/`repl.rs`** — dispatch
them serially in either order, or accept AGENTS §8's rebase dance;
2 may run any time after 1 and before 10 (it is S — dispatching it
right after 1 is the default).

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks under the
parallel options may run concurrently as separate sessions; never
two agents share a worktree. Every chunk works in
`../taskboard-WT-<branch>` next to the project root (AGENTS §8),
branched off the **then-current `main`** — its predecessor has
merged, so **no PR is ever stacked** (MD-2). Disk hygiene (AGENTS
§8 worktree lifecycle): any agent creating or recreating a worktree
runs `scripts/worktree-doctor.sh` from the main checkout first —
nonzero exit (free disk below the floor) stops the chunk; stage 3
removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, re-locates the chunk's named symbols at its own start
   (implementation plan §2: finding IDs and symbols are the stable
   keys; line numbers anywhere in the plans are plan-day
   references), writes the TDD-first plan at
   `.artifacts/plans/<today>-daily-driver-<slug>-plan.md` (MD-6) in
   the house `[Plan]` style, resolving every open question with a
   recorded decision, commits it (`docs(plan): …`). No
   implementation, no push. The implementation plan's §2/§3/§4/§5/§6
   and the design sections each chunk names are normative input, not
   suggestions — deviations need a recorded reason, and a deviation
   from an ACCEPTED ruling (any design doc's decision record) goes
   back to the user instead (MD-7).
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the
   approved plan — TDD per `docs/testing_strategy.org`, every
   commit standalone (rebase-merge hygiene), the full AGENTS §10
   gate suite green. Does **not** push or open a PR. Reports
   deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`** —
   `git fetch origin --prune`, then rebase the branch onto
   `origin/main` (AGENTS §8's rebase dance: resolve nothing unless
   real conflicts appear; drop already-merged duplicate commits),
   and push `--force-with-lease` if the rebase rewrote history; the
   review examines the diff against the **post-rebase** base. Then:
   reviews the entire branch diff versus that base against the
   chunk's acceptance criteria + AGENTS rules **plus the mandate
   below**, re-runs whatever gates apply, implements fixes for every
   finding on the branch (amend/reword, never duplicate-subject
   fixups). Its last tasks are shipping: push the branch, open the
   PR to `main` with `gh`, watch `gh pr checks --watch` to
   completion — any failing check is a finding; diagnose, fix, push
   `--force-with-lease`, repeat until green — **and then verify the
   PR's merge state**: `gh pr view --json mergeStateStatus` must
   report `CLEAN`; `BEHIND` or `DIRTY` ⇒ rebase onto `origin/main`,
   push `--force-with-lease`, let the checks re-run, and repeat; any
   other state ⇒ diagnose before handing off. **Then teardown**:
   remove the now-disposable worktree — from the project root,
   `git worktree remove --force ../taskboard-WT-<branch>` (everything
   precious is pushed; a later round that resumes the chunk
   recreates it on the branch). The handoff contract is **"PR open +
   checks green + merge state `CLEAN` + worktree removed"**. Reports
   a verdict. Never merges; never touches `main` (`main` is
   branch-protected — the user merges on GitHub).

**Code-quality mandate** (stage 3, chunks 1–10 — implementation
chunks; the house text): beyond functional review, the stage-3
agent evaluates:

- **Code quality**: naming, comment discipline (comments state
  constraints, not narration), idiomatic use of the approved stack,
  no dead code or speculative generality, test quality per AGENTS
  §5.
- **Single Responsibility Principle**: every module, actor,
  function, and type has exactly one reason to change; domain stays
  pure (no IO, no runtime), the engine orchestrates without
  legislating, the connector actor only syncs, the CLI only
  parses/dispatches — the crate-role boundaries of AGENTS §2 are
  the SRP line, and a violation of the boundary is a violation of
  SRP.
- **Single level of abstraction per function body**: every function
  body works at one level — a function either orchestrates (calls
  named operations) or implements (works in primitive data), never
  mixes; guard clauses and the happy path at the top level, detail
  pushed into named private functions.

Findings from the mandate are fixed on the branch like any other
finding.

**Evidence & artefact mandate** (stage 3, chunk 11 — the docs
chunk): every factual claim carries evidence (the command run +
observed outcome, or a verifiable source + what it states);
recommendations are decision-grade; repo hygiene (scratch code and
captured payloads never land — evidence is excerpts inside the
artefact; the PR diff is exactly the chunk's artefacts); scope
discipline (out-of-scope ideas go to the report's
backlog-candidates section for the user to promote — **no
autonomous issue creation, no tracker edits beyond the §13
bookkeeping, no production code**).

**Round-wide evidence clauses** (bind all eleven chunks): the PR
diff is exactly the chunk's artefacts — scratch code, probe crates,
and captured payloads never land; out-of-scope ideas discovered
mid-chunk go to the chunk's stage report for the user to promote,
never into autonomous issue creation; the only tracker edits in the
round are the §13 bookkeeping (MD-4).

**Recorded method decisions**:

- **MD-1 — the round gate**: chunk 1's stage-1 prompt verifies the
  multi-board implementation round is complete on `origin/main`
  (command inline). Until it is, **no chunk of this round
  dispatches**. Rationale: implementation plan §Status — chunks 1–10
  touch the files multi-board chunks 2–7 rewrite, and every design
  doc states post-multi-board vocabulary.
- **MD-2 — stacking policy**: no PR is ever stacked by default —
  each chunk branches off then-current `main` after its predecessor
  merged. If the user stacks anyway (or runs 9 ∥ 10), every stacked
  branch does AGENTS §8's rebase dance proactively after each
  merge: `git fetch origin --prune; git rebase --onto origin/main
  <tip-of-the-branch-that-just-landed> <my-branch>; git push
  --force-with-lease` — never plain `--force`.
- **MD-3 — mandate split**: code-quality mandate for chunks 1–10;
  evidence & artefact mandate for chunk 11; the round-wide evidence
  clauses for all eleven.
- **MD-4 — tracker-edit mechanics**: closures ride the PR body —
  stage 3 writes `Closes #N` per the implementation plan §6 table so
  GitHub closes on merge; annotations are exactly **one progress
  comment per mapped issue per chunk**, posted by stage 3 when the
  PR opens. Two explicit exceptions the plan orders: chunk 9 posts
  the D-V9 amendment note on #348 (top-level `find` verb, departing
  the issue's `tasks find` wording); chunk 11 posts the #202
  milestone-clear comment (rules half landed; proofs → the D5
  wave). Nothing else: no label/milestone edits, no new issues.
- **MD-5 — tier honesty**: chunk 3's Deck 1.17.5 fixtures and
  chunk 4's tier-2 convergence reuse the existing harness
  conventions — no new mechanics beyond chunk 3's pinned variant.
  A runner without docker **records that honestly** (stage report +
  PR body), never a silent skip; chunk 11's exit report cites those
  records. A 1.17.5 **behavioral divergence** is a STOP-and-report
  finding (DDF-10): the design's expected-degraded stance stands
  until the user rules — never a silent adaptation.
- **MD-6 — stage-1 plan paths**: `.artifacts/plans/<today>-daily-driver-<slug>-plan.md`,
  slugs `task-field-family`, `day-boundary`, `appdata-transport`,
  `appdata-fold`, `task-field-surfaces`, `reminder-rule-verbs`,
  `adhoc-alarms`, `desktop-notifications`, `find`, `today-inbox`,
  `wave-exit-docs`. This dispatch plan ships without a separate
  stage-1 work plan — this MD section is its recorded-decisions
  artefact.
- **MD-7 — normative rank**: binding, in order: the implementation
  plan §2 (DDF-1…DDF-15) / §3 chunk sections / §4 / §5 / §6, then
  each design doc's chunk-named sections and its ACCEPTED decision
  record. Doc 4 §7's reconciliation list and doc 1 §7's chunk
  interfaces are part of the normative input. Recorded-reason
  deviations are allowed against prose; a deviation from an
  ACCEPTED ruling goes back to the user before it is implemented.
- **MD-8 — anchor discipline**: stage-1 agents re-locate every
  named symbol at their own start; symbols are the stable keys,
  line numbers are plan-day references.
- **MD-9 — precondition mechanics**: each chunk's stage-1 prompt
  verifies its predecessor's branch is merged on `origin/main`
  (command inline); chunk 1 additionally carries the MD-1 round
  gate. A failed precondition = STOP and report, never proceed on a
  stale base.
- **MD-10 — the interim rule**: between chunks 1 and 4 the four
  fields exist but app-field edits enqueue no sync op (local-only
  interim, invisible until chunk 5's surfaces). No chunk "fixes"
  this out of order — the enqueue sites land in chunk 4 and only
  there; a chunk 1–3 diff touching `outbox.rs` enqueue paths or the
  engine's op routing for app fields is scope creep and a finding.

Agent-session count: **33** (11 chunks × 3 stages), plus this
docs-only dispatch-plan chunk (the round's chunk 0), plus the user's
merges between chunks.

## 2. Chunk 1 — Domain field family + lockstep manifest + 0007 + native `startdate` (`feat/domain-task-field-family`)

- **Covers** (implementation plan §3 chunk 1 — normative; doc 2
  §3.1 D-F1, §3.2 D-F2 branch A (RULED), §3.3 D-F3 assertions 1–2,
  §3.4 D-F4; doc 1 D-S6 guard flags; DDF-1/2/3): domain
  `entities.rs` — the four `Task` fields, four `TaskClocks` slots,
  `Effort` (u64-seconds newtype, `#[serde(transparent)]`, codec
  `≤ i64::MAX` check), `Importance`/`Urgency` (distinct five-level
  enums, serde unit names, derived `Ord` = severity order), the
  exported `TASK_FIELDS` manifest + `FieldFamily`; `messages.rs` —
  four double-option `TaskChanges` slots; `pipeline.rs` —
  `PendingTaskIntent` app-field bools + `reminder_rules` flag +
  serde derives (assertion 1's needs); `merge.rs` — the `scheduled`
  verdict (the `duedate` verdict copied); `push.rs` —
  `NewCardShape`/`CardShape` + `from_task` sites; `test_support`
  strategies; storage `0007_task_field_family.sql` (4 value columns
  NULL + 4 epoch-defaulted clock columns + `reminder_rules.
  updated_at` epoch-defaulted, codec untouched by it) + row
  struct/decode/upsert/batch; sync `model.rs` `RemoteTask.startdate`
  + `client.rs` card write encoding + contract fixtures pinning the
  wire key both directions; assertions 1 (domain serde bijection)
  and 2 (storage `PRAGMA` columns vs the manifest); the one
  deliberate Task-widening insta pass. **Non-goals**: no
  `UpdateAppData` arm, no enqueue, no document keys, no
  `ReminderRule.updated_at` domain field, no CLI/UI, no preferences
  (MD-10's interim).
- **Acceptance**: twelve-clock skeleton with assertions 1–2 green;
  `Some(None)` clears **and stamps**; `scheduled` round-trips
  create/update/read at tier 1 wire-pinned and merges per-field
  LWW; 0007 is one file, one transaction, 0001/0005 conventions,
  epoch defaults preserve old rows' merge behavior bit-for-bit;
  migration round-trip test; the insta diff is the widening, nothing
  else, no `.pending` files; gates green; PR body carries
  `Closes #208`; annotations on #209/#244.
- **Traps for stage 3**: a field without its clock (or clock without
  field) fails assertion 1 — the assertion must exist and actually
  enumerate; a `scheduled` key appearing in any document-shaped type
  is premature (assertion 3 is chunk 3's — its absence here is
  correct); `Importance`/`Urgency` as one shared type is a ruling
  violation (two distinct enums); `RemoteTask.startdate` decode must
  be null-tolerant like `duedate`; the rule-clock column existing
  without any codec read/write is **correct** (chunk 4 wires it);
  snapshot diffs beyond the Task widening are findings; any outbox/
  enqueue diff is scope creep (MD-10).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the daily-driver implementation round ("Domain field family + lockstep manifest + 0007 + native startdate", branch feat/domain-task-field-family) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (the round gate, MD-1/MD-9): the multi-board implementation round is complete — git fetch origin --prune, then gh issue list --milestone wave-multi-board --state open must return EMPTY (its issues #125/#187/#179/#170 closed by merged PRs). If it is not empty, STOP and report instead of planning. Also verify this dispatch plan is on origin/main (git cat-file -e origin/main:.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md).

1. Read AGENTS.md, then the implementation plan .artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md (§2 DDF-1/2/3, §3 chunk 1 — normative: files, behaviours, non-goals; §4 gates; §5 incl. the interim rule; §6 bookkeeping), the design docs .artifacts/plans/2026-10-18-field-family-design-plan.md (§3.1 D-F1, §3.2 D-F2 branch A RULED, §3.3 D-F3, §3.4 D-F4, §5 ACCEPTED record) and .artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md (§3.6 D-S6 — the guard flags; §3.2 D-S2 — TaskEntry shapes this chunk's serde encodings must match), and the dispatch plan §1/§2.
2. Read the current seams (re-locate every symbol yourself; line numbers in the plans are plan-day references): crates/taskboard-domain/src/{entities.rs (Task, TaskClocks, BoardClocks::epoch, Color's newtype precedent, task_strategy), messages.rs (TaskChanges, deserialize_double_option, EditTask), pipeline.rs (PendingTaskIntent, pending_task_intents, keep_task_intent), merge.rs (merge_task, remote_wins, the duedate verdict), push.rs (NewCardShape/CardShape/from_task), reminders.rs (ReminderRule — read-only context)}, crates/taskboard-storage-sqlite/src/{repo.rs (row struct, decode_task, upsert, whole-state batch), codec.rs (op tags — context only)}, migrations/0001_init.sql + 0005 (conventions + the epoch-default precedent), crates/taskboard-sync-nextcloud/src/{model.rs (RemoteTask.duedate), client.rs (card create/update encoding), tests/contract.rs + tests/fixtures/deck}, plus the suites you must not break: the domain proptest corpus, the state-engine insta corpus, the storage round-trip tests.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = STOP and report), then create the worktree ../taskboard-WT-feat-domain-task-field-family on a new branch feat/domain-task-field-family off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-daily-driver-task-field-family-plan.md, settling at minimum: the exact type shapes (Effort newtype + its codec bound check, the two enums' variant/serde/Ord contracts, the manifest const + FieldFamily); the TaskChanges slot semantics (Some(None) stamps — the handler rule citation); the assertion-1 mechanics (how PendingTaskIntent serde key sets are compared; skip_serializing_if handling); the assertion-2 mechanics (PRAGMA column name mappings incl. stack/order/labels); the final 0007 SQL against the live schema; the storage codec's treatment of reminder_rules.updated_at (untouched — default applies); the merge verdict + push shape + client encode for startdate (null-tolerance both directions); the contract-fixture additions; the strategy extensions; the insta-pass catalogue and the review rule for the accepted diff; the full typed-outcome test matrix. Resolve every open question with a recorded decision; deviations need a recorded reason; a deviation from an ACCEPTED ruling goes back to the user — STOP and report.
5. Commit it as: docs(plan): task field family, manifest, and native startdate. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the daily-driver implementation round ("Domain field family + lockstep manifest + 0007 + native startdate").

1. Read AGENTS.md, the implementation plan (§2 DDF-1/2/3, §3 chunk 1, §4), both design docs' named sections, the dispatch plan §1/§2, and the stage-1 plan (.artifacts/plans/*daily-driver-task-field-family*-plan.md) in the worktree ../taskboard-WT-feat-domain-task-field-family (branch feat/domain-task-field-family). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: the twelve-clock skeleton grows together; Importance and Urgency are two distinct types; Some(None) clears AND stamps; the manifest + assertions 1–2 land in this chunk; 0007 is one file/one transaction/epoch defaults; the rule-clock column gets no codec read/write; RemoteTask.startdate is null-tolerant both directions; NO UpdateAppData arm, NO enqueue-site change, NO document keys (the interim stands — MD-10); typed outcomes, never text assertions; no sleeps — pause()/advance() and deadline polls; fakes over mocks; SPDX headers per crate; #![forbid(unsafe_code)] except ui.
3. Run the full gate suite from AGENTS.md §10 and make it green (the CI-identical three-command llvm-cov sequence included). The insta pass is ONE deliberate regeneration for the Task widening — review the accepted diff as "the shape change, nothing else"; no .pending snapshots ship.
4. Bookkeeping: this chunk's user-visible CHANGELOG.md Unreleased line. Do NOT touch any tracker issue (MD-4). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the daily-driver implementation round (branch feat/domain-task-field-family).

1. First, rebase onto main: in ../taskboard-WT-feat-domain-task-field-family run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicate commits; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-1/2/3, §3 chunk 1), the design docs' named sections (field-family §3.1–§3.4 + §5 rulings; sidecar §3.2/§3.6), the dispatch plan §1/§2 (the code-quality mandate applies in full), and the stage-1 plan.
3. Functional focus — this chunk's traps: assertions 1–2 exist, enumerate the real structures, and pass (a half-run ceremony is exactly #244's failure mode); a scheduled key in any document-shaped type is premature; one shared enum for importance/urgency violates the ruling; Some(None) does not stamp = a finding; RemoteTask.startdate decode/encode null-tolerance; 0007 lands whole (both the tasks columns and the rule-clock column) with epoch defaults and a round-trip test; reminder_rules.updated_at has no codec read/write yet (a wiring diff here is scope creep into chunk 4); ANY outbox/enqueue/op-routing diff is scope creep (MD-10 interim); the insta diff is the Task widening only; no test weakened to pass (AGENTS §5).
4. Code-quality mandate: (a) naming/comment discipline/no speculative generality (no document-lane scaffolding — chunk 3/4's); (b) SRP — domain pure, storage persists only, sync maps wire only; (c) one level of abstraction per body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (PR body carries "Closes #208"), then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green. Verify gh pr view --json mergeStateStatus reports CLEAN (BEHIND/DIRTY ⇒ rebase, push --force-with-lease, re-check). Post the annotation comments: #209 (fields landed; decorations are chunk 5's), #244 (manifest + assertions 1–2; 3–4 ride chunks 3/4) — one progress note each with the PR URL (MD-4). Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-domain-task-field-family.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 3. Chunk 2 — Day-boundary preference (`feat/domain-day-boundary`)

- **Covers** (implementation plan §3 chunk 2 — normative; doc 2
  §3.6 D-F6; DDF-11): app `config.rs` — the `[preferences]` section
  (`day_start_time`, default `"00:00"`), strict-table decode, typed
  validation (an unparseable time-of-day is a config error, never a
  silent default); domain `days.rs` — new module, `day_window(now,
  NaiveTime, FixedOffset) -> (start, end)` pure/total
  fixed-offset arithmetic, the ruled proptest set
  (`start ≤ now < start+24h`; stepping past `start` flips; +24h/day
  in fixed offset), the consumers contract on the doc comment.
  **Non-goals**: no consumer wiring, no `AppState`/storage/engine
  growth, no sync, no `UserPreferences` entity.
- **Acceptance**: config round-trips with typed failures; the
  proptest set green; no other crate's behavior changes; gates
  green; PR body carries `Closes #345`.
- **Traps**: `chrono::Local` or any environment read inside the
  domain (the offset is a **parameter**); a silent default on
  invalid config; proptest gaps (the flip-at-boundary property);
  scope creep into consumers.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the daily-driver implementation round ("Day-boundary preference", branch feat/domain-day-boundary) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 1's branch feat/domain-task-field-family is merged on origin/main (git fetch origin --prune, then git branch -r --contains origin/feat/domain-task-field-family || git cat-file -e origin/main:<a file only chunk 1 added, per the stage-1 plan's catalogue>). If not merged, STOP and report.

1. Read AGENTS.md, the implementation plan (§2 DDF-11, §3 chunk 2, §4–§6), the design doc .artifacts/plans/2026-10-18-field-family-design-plan.md §3.6 D-F6 (+ §5's ruling incl. the fixed-offset DST ruling and the #351 note), and the dispatch plan §1/§3.
2. Read the seams: crates/taskboard-app/src/config.rs (the section structs, typed-validation precedents, deny_unknown_fields), crates/taskboard-domain/src/clock.rs (the purity rule), the domain lib.rs module list.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-domain-day-boundary on a new branch feat/domain-day-boundary off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-day-boundary-plan.md settling: the config section's exact shape/defaults/validation error type; day_window's signature + algorithm (offset-local date → set time → back-convert → step-back-one-day rule) + the proptest catalogue; the consumers-contract doc text. Record every decision; deviations from the ruling go back to the user.
5. Commit as: docs(plan): day-boundary preference and day_window. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the daily-driver implementation round ("Day-boundary preference").

1. Read AGENTS.md, the implementation plan (§2 DDF-11, §3 chunk 2, §4), the design doc §3.6, the dispatch plan §1/§3, and the stage-1 plan in ../taskboard-WT-feat-domain-day-boundary (branch feat/domain-day-boundary). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: the domain function takes time and offset as data (no chrono::Local, no Utc::now in taskboard-domain); invalid config is a typed error; no state/storage/engine/consumer changes; typed outcomes; SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the daily-driver implementation round (branch feat/domain-day-boundary).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-11, §3 chunk 2), the design doc §3.6 + its ruling, the dispatch plan §1/§3 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: no environment reads in the domain function; the fixed-offset ruling implemented as stated (no tz-database lookups); the proptest set complete incl. the boundary-flip property; typed config validation (a silent default is a finding); the diff touches nothing else.
4. Code-quality mandate (naming/comments/no generality; SRP; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #345"), gh pr checks --watch to green, mergeStateStatus CLEAN (rebase+force-with-lease if BEHIND/DIRTY). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-domain-day-boundary.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 4. Chunk 3 — Sidecar transport: 1.17.5 gate + anchor ceremony + document codec + evidence (`feat/sync-appdata-transport`)

- **Covers** (implementation plan §3 chunk 3 — normative; doc 1
  §3.1 D-S1, §3.2 D-S2, §3.4 D-S4 items 1–3, §3.5 D-S5 (arm + tag
  only), §4 the ruled live-tier gate; doc 2 D-F2's folded wire pins;
  DDF-4/5/10; **amended 2026-10-22 by the version-capabilities
  addendum** `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`
  — the gate ran, found two divergences (measured:
  `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`),
  and the ruling replaced expected-degraded with version-specific
  parity): the **capability seam** — one capabilities probe
  (`deck.version`), a typed `DeckFeatures` flag table
  (`CARD_STARTDATE`; newest-known-wins for unknown versions;
  conservative fallback on probe failure), the unconditional additive
  attachment-create `data` field (required by 1.17.5, tolerated by
  1.19.0), the echo-decode fallback (`fileName` → `info.filename`),
  and capability-gated card conversion (`scheduled` → `None` on card
  writes while the flag is false); the dockerized **Deck 1.17.5**
  gate fixtures **green on both tiers** (attachment lifecycle
  identical once the wire changes land; `…_deck1175_startdate_unsupported`
  pins the measured 1.17.5 behavior; capabilities decode probe on
  both tiers; a *new* divergence is still STOP-and-report per MD-5);
  domain `appdata.rs` (new module: envelope/sections/`TaskEntry`/
  `RulesEntry` types with the D-S2 shapes, the codec, the union +
  snapshot-absence GC as pure functions, verbatim unknown-content
  preservation; **`scheduled` = fourth modeled key** per D-V2); the
  exported anchor-title constant in the domain crate (`[taskboard]
  app-data`); `outbox.rs` `LocalOp::UpdateAppData(TaskId)` arm;
  storage `codec.rs` tag `update_app_data`; `messages.rs`
  `AppDataEvidence {Document(Box<AppDataDocument>) | Absent |
  Unreadable(AppDataUnreadable)}` on `SyncReport::Completed` (Failed
  never carries it); sync `actor.rs` — anchor discovery (exact-title
  scan of active+archived, detail-read confirmation, newest-stamp/
  lowest-id pick), lazy actor-side creation, single-match adoption,
  metadata capture from the already-paid bound-card detail refresh,
  `LoopState` `(lastModified, filesize)` + cached-document store
  (in-memory only), download-during-pull after `pull_snapshot`,
  evidence construction; manifest assertion 3 (**reworded**: modeled
  key set = `{effort, importance, urgency, scheduled}`, with
  `scheduled`'s emission/fold sites capability-gated — the gate lives
  in the sync/pipeline layers, the pure codec stays version-blind);
  the `SyncReport`-fixture insta pass. **Non-goals**: no fold, no
  health derivation, no flush/rebuild, no enqueue sites, no
  emission/fold gating *code* (the gating itself is chunk 4's —
  chunk 3 lands the flag seam + the codec's fourth key + the gated
  card-write encoding only); no CLI/UI.
- **Acceptance**: the gate fixtures green on **both** tiers before
  the transport commits land (the harness + fixture commits
  `84ab03b`/`332c89b` are already on the branch; revise fixture
  bodies per the addendum, never weaken); anchor ceremony behaviors
  unit/tier-2 pinned (creation, adoption, deterministic pick); a
  downloaded document reaches the engine as typed evidence; `Absent`
  is silent; unknown sections/keys/entries survive a codec round-trip
  verbatim (property test); assertion 3 (reworded) green; no
  persisted validator kind added; gates green; annotations on
  #207/#244.
- **Traps**: the anchor title scan normalized (trim/casefold) — it
  is exact-match, deliberately unlike `ReconcileByTitle`; multiple
  unconfirmed matches touched instead of creating own; anchor
  creation via an engine command (it is actor-side `create_card`,
  adopted by the next pull); evidence as a bare `Option` (the typed
  enum is the ruling); change detection persisted anywhere (LoopState
  only); a filename-rename flow anywhere (renames 500 — no rename
  exists); create-against-a-tombstone depended on (restore-then-
  replace is the designed path — chunk 4's, referenced not built);
  the document codec dropping unknown content on re-encode; fold/
  flush code appearing early (scope creep into chunk 4); raw version
  comparisons scattered through the connector instead of the typed
  flag table; gating the `data` field behind a probe (it is
  unconditional — verified tolerated by 1.19.0); gating the *codec*
  on the flag (the pure codec always models `scheduled`; only the
  sync/pipeline sites gate); a `scheduled` document key that folds or
  emits while `CARD_STARTDATE` is true (that gating is chunk 4's and
  must not appear early — chunk 3 ships the key in the codec and the
  gated card-write encoding only).

**Prompt 3.1 (planning — RE-RUN 2026-10-22, after the version-capabilities ruling):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the daily-driver implementation round ("Sidecar transport: 1.17.5 gate + anchor ceremony + document codec + evidence"), RE-RUN after the 2026-10-22 version-capabilities ruling, branch feat/sync-appdata-transport in the taskboard repo at /home/viktoria/src/taskboard.

Preconditions (MD-9): chunks 1 and 2 merged on origin/main (verify). Additionally: this branch ALREADY CARRIES commits from the first run — the harness parameterization (compose image interpolation + DECK_VERSION pin), the fixture set, the stage-2 STOP report, and the 2026-10-22 design addendum + characterization report + doc amendments (all to ride this chunk's PR). Verify with git log; do not redo or revert them. Recreate the worktree ../taskboard-WT-feat-sync-appdata-transport if missing (run scripts/worktree-doctor.sh first — nonzero = STOP).

1. Read AGENTS.md; the implementation plan (§2 DDF-2/4/5/10 as amended, §3 chunk 3, §4–§6); the sidecar design doc's named sections + §6 rulings (its 2026-10-22 amendment note); THE NORMATIVE NEW INPUTS: .artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md (D-V1 capability seam, D-V2 the scheduled lane rule, D-V3 the unconditional wire changes + revised fixture set, §4 obligations) and .artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md (the measured evidence); the field-family design doc §3.2 D-F2 (as amended); the dispatch plan §1/§4 (chunk 3, as amended 2026-10-22).
2. Read the seams (symbols, not line numbers): sync client.rs (attachment surface + AttachmentUpload/multipart_form + card create/encode + DeckClient::new for the probe), model.rs (ExtendedData decode — the fileName→info.filename fallback site), actor.rs (cycle, pull_snapshot, LoopState), tests/{contract.rs, common/mod.rs, it_docker.rs}; domain {messages.rs, outbox.rs, entities.rs TASK_FIELDS, pipeline.rs context only}; storage codec.rs.
3. REVISE the existing stage-1 plan .artifacts/plans/2026-10-22-daily-driver-appdata-transport-plan.md in place (it is partially superseded): keep what stands (D6–D16: appdata.rs types, anchor.rs, evidence, op arm, LoopState cache, commit ladder shape); rewrite what the addendum changes (D4/D5 fixtures → the revised gate set green on BOTH tiers; new decisions for D-V1/D-V2/D-V3 wiring: probe+flags types and their home, the capabilities client method, card-conversion gating, the codec's fourth modeled key + reworded assertion 3, the deck1175 startdate fixture renamed to …_unsupported pinning the measured behavior). Record every decision; a NEW divergence against the characterized behavior is STOP-and-report (MD-5), not a design input.
4. Commit as: docs(plan): app-data sidecar transport, version-capability revision. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size (the remaining ladder on top of the already-landed commits).
```

**Prompt 3.2 (implementation — RE-RUN 2026-10-22, after the version-capabilities ruling):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the daily-driver implementation round ("Sidecar transport: 1.17.5 gate + anchor ceremony + document codec + evidence"), RE-RUN after the 2026-10-22 version-capabilities ruling.

1. Read AGENTS.md, the implementation plan (§2 DDF-2/4/5/10 as amended, §3 chunk 3, §4), the sidecar design doc's named sections + §6 rulings + its amendment note, the NORMATIVE addendum .artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md + the characterization report .artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md, the dispatch plan §1/§4 (as amended), and the REVISED stage-1 plan in ../taskboard-WT-feat-sync-appdata-transport (branch feat/sync-appdata-transport — it already carries the harness/fixture commits, the STOP report, the addendum, and your revised plan; recreate the worktree if missing, worktree-doctor first).
2. Implement exactly per the revised plan, TDD, commits standalone. ORDER: (a) the capability seam — capabilities probe method + typed DeckFeatures flag table + card-conversion gating + the unconditional attachment-create data field + the echo-decode fallback, then the REVISED fixtures; run the gate GREEN ON BOTH TIERS (default + deck1175) before any transport commit — the deck1175 startdate fixture pins the measured unsupported behavior (…_unsupported); a NEW divergence vs the characterized behavior is STOP-and-report (MD-5). (b) domain appdata.rs (scheduled = fourth modeled key) + the anchor const + the evidence enum + the op arm; (c) the storage op tag; (d) the sync actor's ceremony + download + evidence. Hard rules: exact-title scan (no normalization); lazy actor-side creation (never an engine command); newest-stamp/lowest-id pick; the typed evidence enum (never a bare Option); LoopState in-memory cache only; unknown content preserved verbatim; NO fold/health/flush/enqueue and NO scheduled emission/fold gating code (chunk 4's — scope creep; chunk 3 ships the flag seam + codec key + gated card-write encoding only); flags derived from the typed table, never raw version comparisons in the connector; typed outcomes; no sleeps (pause/advance, deadline polls); tier-2 tests record what ran honestly; SPDX headers.
3. Run the full gate suite (AGENTS §10) green, incl. cargo deny over any tree change.
4. Bookkeeping: the CHANGELOG Unreleased line (cover the capability seam + sidecar transport). No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the 1.17.5 gate outcomes explicitly (both tiers), deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the daily-driver implementation round (branch feat/sync-appdata-transport).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-2/4/5/10, §3 chunk 3), the sidecar design doc §3.1/§3.2/§3.4(1–3)/§3.5/§4 + §6 rulings, the dispatch plan §1/§4 (code-quality mandate in full), the stage-1 plan.
3. Functional focus — this chunk's traps (as amended 2026-10-22): the gate fixtures run green on BOTH tiers (or their absence is honestly recorded) and pin all attachment behaviors + the 1.17.5 startdate-unsupported behavior; any adaptation beyond the ruled capability seam (addendum D-V1/D-V3) is a critical finding; the flag table is the only version switch (raw version comparisons in the connector are a finding); the additive data field is unconditional; the anchor scan is exact-match and the pick rule deterministic; adoption only on exactly-one-unconfirmed-match; creation is actor-side and lazy; the evidence enum carries the reason (Unreadable variants used, not stringly errors); Absent stays silent; the LoopState cache is in-memory only (any persisted validator kind for document content is a finding — no HTTP validators exist); the codec preserves unknown sections/keys/entries verbatim under re-encode (the property test proves it); null = cleared and tasks-section tombstones do not exist; assertion 3's key set is exactly {effort, importance, urgency, scheduled} with the reworded gate (codec version-blind; a codec-level capability branch is a finding); NO fold/flush/health/enqueue/scheduled-emission code (chunk 4 scope creep); the SyncReport insta pass is the widening only.
4. Code-quality mandate (naming/comments/no speculative generality — no flush scaffolding; SRP — domain appdata.rs is pure, the actor only transports; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings. If docker is unavailable, record honestly (MD-5) — the fixtures must still be shown wired and locally green or explicitly marked.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR, gh pr checks --watch to green, mergeStateStatus CLEAN (rebase+force-with-lease if BEHIND/DIRTY). Post the annotations: #207 (transport half landed; fold/flush is chunk 4's), #244 (assertion 3 landed) — one progress note each (MD-4). No Closes line. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-sync-appdata-transport.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 5. Chunk 4 — Sidecar fold + flush + fences + rules section + op routing + surfacing (`feat/sync-appdata-fold`)

- **Covers** (implementation plan §3 chunk 4 — normative; doc 1
  §3.3 D-S3, §3.4 D-S4 item 4, §3.5 D-S5 planning/completion,
  §3.6 D-S6, §3.7 D-S7, §3.8 D-S8, §3.9 D-S9; DDF-6/7/8/9): domain
  `merge.rs` `fold_task_app_data` + the rules fold (embedded-clock
  LWW, clock = max, absence ≠ deletion, no R3 feeding, resolution
  via `task_by_board_card`); `pipeline.rs` — the fold's position in
  `apply_sync_report` (after the task merge loop), the
  `pending_task_intents` `UpdateAppData` arm (sets all app-field
  flags + rules flag), the value+**embedded-clock** copy inside the
  fold, the two v1 fence comments **amended** (native path only;
  rules ride the document); `push.rs` — the document group in
  `plan_pushes` (coalesced per board, after `CreateTask` groups,
  outcome handling incl. dead-letter on permanent 4xx);
  `reminders.rs` — `ReminderRule.updated_at` + stamping on every
  mutation; storage `repo.rs`/`codec.rs` — rule-clock read/write;
  sync `actor.rs` — the flush between pull and report (union of
  cached/downloaded + group-local entries, never blind), rebuild
  mode (all local entries), restore-then-replace for tombstoned
  attachments, the D-S7 fence table behaviors; state
  `actor.rs`/`core.rs` — per-board sidecar health derived from the
  evidence; app — CLI `status` warning rendering + the
  **anchor-title CLI guard** (typed refusal of rename/delete on an
  anchor-titled task, app layer only); the enqueue sites — `EditTask`
  app-field slots and the existing rule verbs; manifest assertion 4
  (per-row op routing). **Amended 2026-10-22 (version-capabilities
  addendum §2/§4)**: the scheduled lane gating lives here — the fold
  materializes document `scheduled` keys only while `CARD_STARTDATE`
  is false, the native `scheduled` verdict is skipped while false
  (the always-absent remote must never clobber local state), the
  flush emits none and the union lane-GC drops stale `scheduled`
  keys while true, and assertion 4 routes `EditTask`'s scheduled slot
  per the flag. **Non-goals**: no N-rule verbs (chunk 6),
  no UI badge (chunk 5), no adhoc (chunk 7), no cross-board repair
  (#350), no proofs/settings sections (reserved names only).
- **Acceptance**: a tier-2 convergence test — app-field edit →
  flush → a second client folds it; a rule add → tombstone → fold
  on the second client; the guard mirror holds a post-`read_at`
  edit through the same cycle's adoption (value AND clock); the
  fences exercised per the D-S7 table (failed download ⇒ no upload;
  failed upload ⇒ ops queue + cycle still `Completed`; permanent
  4xx ⇒ counted `FailOp`s); format-too-new renders the loud warning
  through CLI `status`; the CLI guard refuses anchor-titled
  rename/delete typedly; assertion 4 green; the interim closed
  (app-field edits enqueue); gates green; PR body carries
  `Closes #207` + `Closes #244`; annotation on #202.
- **Traps**: the fold reading the card stamp anywhere (disjoint
  clock domains — a finding); absence treated as deletion; the fold
  feeding R3/presence; the guard copying values without their
  embedded clocks; flush before the pull or after the report (the
  position is load-bearing: after card pushes, before report, bump
  lands next cycle); a blind overwrite instead of
  read-modify-write-union; rebuild flushing only the group's tasks;
  the fence comments deleted instead of amended; rule mutations not
  stamping the clock; enqueue of `UpdateTask` for reminder data
  (the fence violation); the CLI guard implemented in the engine
  (it is app-layer by ruling); ops completing on a failed upload.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the daily-driver implementation round ("Sidecar fold + flush + fences + rules section + op routing + surfacing", branch feat/sync-appdata-fold) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 3 merged on origin/main (verify; else STOP and report).

1. Read AGENTS.md, the implementation plan (§2 DDF-6/7/8/9, §3 chunk 4, §4–§6 incl. the interim this chunk closes), the sidecar design doc (§3.3 D-S3, §3.4 D-S4 item 4 + the D-S7 fence table, §3.5 D-S5, §3.6 D-S6, §3.7, §3.8, §3.9, §5 the binding merge-policy clauses, §6 rulings), .artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md §3.2 D-R2 (the enqueue-site enumeration), and the dispatch plan §1/§5.
2. Read the seams: crates/taskboard-domain/src/{merge.rs (merge_task, keep-order), pipeline.rs (apply_sync_report, pending_task_intents, keep_task_intent, apply_push_report's dead-letter precedent), push.rs (plan_pushes, PushGroup), outbox.rs, reminders.rs (ReminderRule + the v1 fence comments), appdata.rs + the evidence enum (chunk 3's)}, crates/taskboard-state/src/{actor.rs, core.rs (status surfaces)}, crates/taskboard-sync-nextcloud/src/actor.rs (the cycle, the chunk-3 ceremony/cache, the attachment client), crates/taskboard-storage-sqlite/src/{repo.rs, codec.rs}, crates/taskboard-app/src/commands/{status.rs, tasks.rs or boards.rs (the rename/delete verb paths for the guard)}, the tier-2 corpus conventions.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-sync-appdata-fold on a new branch feat/sync-appdata-fold off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-appdata-fold-plan.md settling at minimum: fold_task_app_data's exact signature/semantics and the rules fold's; the guard arm + in-fold copy mechanics (per-field AND rules, value+clock); the enqueue wiring list (every EditTask app-slot site + the two rule verbs); the document group's planning/positioning/completion semantics in plan_pushes; the flush's placement + union + rebuild-mode mechanics incl. metadata refresh from the upload response; the fence test matrix (each D-S7 row as a test with its typed observable); the sidecar-health derivation + its CLI status rendering; the CLI guard's placement and typed refusal; assertion 4's mechanics; the tier-2 convergence test design (two clients or client+state pairs per the existing corpus). Record every decision; deviations from ACCEPTED rulings go back to the user.
5. Commit as: docs(plan): app-data sidecar fold, flush, and rules section. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the daily-driver implementation round ("Sidecar fold + flush + fences + rules section + op routing + surfacing").

1. Read AGENTS.md, the implementation plan (§2 DDF-6/7/8/9, §3 chunk 4, §4), the sidecar design doc's named sections + §5 binding clauses + §6 rulings, the dispatch plan §1/§5, and the stage-1 plan in ../taskboard-WT-feat-sync-appdata-fold (branch feat/sync-appdata-fold). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: the two clock domains stay disjoint (the fold never reads the card stamp); absence is never deletion; R3 stays snapshot-only; the guard copies values AND embedded clocks; the flush sits between pull and report and is a read-modify-write union (never blind); rebuild mode flushes ALL local entries; the fence comments are amended, not deleted; no reminder data enters the native path (UpdateTask is never enqueued for rules); the CLI guard is app-layer; permanent 4xx dead-letters as counted FailOps; typed outcomes; no sleeps; tier-2 runs recorded honestly. SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the tier-2 convergence outcomes explicitly, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the daily-driver implementation round (branch feat/sync-appdata-fold).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-6/7/8/9, §3 chunk 4, §5 the interim), the sidecar design doc §3.3–§3.9 + §5 + §6 rulings, the dispatch plan §1/§5 (code-quality mandate in full), the stage-1 plan.
3. Functional focus — this chunk's traps: any fold read of the card stamp is a critical finding; absence-as-deletion or document-fed presence anywhere; the guard copying values without embedded clocks; flush position drift (before the pull / after the report / inside the push phase); blind overwrite instead of union; rebuild limited to the group; the fence rows each provably tested (failed download ⇒ no upload; failed upload ⇒ no outcome + cycle Completed; permanent 4xx ⇒ counted FailOps; format-too-new ⇒ loud warning end-to-end through CLI status); the anchor CLI guard in the app layer with a typed refusal; rule mutations stamp updated_at and emit tombstones per removed rule; NO UpdateTask enqueue for reminder data (the fence); assertion 4 routes every manifest row; the interim is closed exactly here (MD-10 satisfied, not early).
4. Code-quality mandate (naming/comments/no generality; SRP — the fold is a pure domain function, the actor transports, the engine derives health; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR (PR body carries "Closes #207" and "Closes #244"), gh pr checks --watch to green, mergeStateStatus CLEAN (rebase+force-with-lease if BEHIND/DIRTY). Post the annotation on #202 (rules half landed via this chunk; proofs stay with the D5 wave per the issue's scope comment) — one note (MD-4). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-sync-appdata-fold.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 6. Chunk 5 — Field surfaces + anchor hiding on lists + sidecar badge (`feat/app-task-field-surfaces`)

- **Covers** (implementation plan §3 chunk 5 — normative; doc 2
  §3.5 D-F5; doc 1 §7's hiding/warning rulings; doc 4 §7 item 3;
  DDF-12): app `cli.rs` — `--scheduled` (the `parse_due`
  vocabulary reused), `--effort` (hand-rolled `parse_effort`:
  bare number = minutes, `s`/`m`/`h` compose, `none` clears,
  overflow = clap usage error), `--importance`/`--urgency`
  (`very-low|low|normal|high|very-high|none`); `commands/tasks.rs`
  both lanes; `commands/output.rs` `TaskRow` + the compact renders
  (effort in the CLI's own duration vocabulary via one shared
  formatter); `completion.rs`; ui `models.rs` — the decoration
  carriers (`scheduled_phrase`/`effort_phrase`/level strings) on
  the D-F5 seams + `TaskCardModel.decorations`;
  `presentation.rs` — the pure phrase helpers (the `due_phrase`
  template), the anchor exact-title filter over list rows/counts,
  the sidecar-health badge rendering. **Non-goals**: no new verbs;
  no today/search filtering (chunks 9/10 own their surfaces); no
  `tasks add` flags; no slint-side logic.
- **Acceptance**: all four flags round-trip CLI/REPL incl. `none`
  clearing; decorations render from presentation only; the anchor
  task disappears from list surfaces and counts; the badge renders
  the warning when the health says so; typed usage errors for bad
  levels/effort; gates green; PR body carries `Closes #209`.
- **Traps**: field-specific verbs creeping in; `parse_effort`
  inventing a second grammar instead of the `parse_due` failure
  shape; semantics (e.g. "starts today") computed in slint instead
  of domain/presentation; the anchor filter implemented in the
  domain (it is presentation-layer by ruling); the badge reading
  anything but the derived health; text assertions in tests.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the daily-driver implementation round ("Field surfaces + anchor hiding on lists + sidecar badge", branch feat/app-task-field-surfaces) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 4 merged on origin/main (verify; else STOP and report).

1. Read AGENTS.md, the implementation plan (§2 DDF-12 + the DDF-5 filter/badge, §3 chunk 5, §4–§6), the design docs .artifacts/plans/2026-10-18-field-family-design-plan.md §3.5 D-F5 (+ §5 ruling), .artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md §7 (the hiding + warning rulings), .artifacts/plans/2026-10-19-views-finding-design-plan.md §7 item 3 (the decorations slot), and the dispatch plan §1/§6.
2. Read the seams: crates/taskboard-app/src/{cli.rs (TasksCmd::Edit flags, DueSpec/parse_due, clap groups), commands/tasks.rs (edit/edit_on), commands/output.rs (TaskRow, due_marker), completion.rs}, crates/taskboard-ui-slint/src/{models.rs (push, TaskCardModel.decorations, due_phrase wiring), presentation.rs (due_phrase, due_sets, the SyncBadge seam, the chunk-4 health surface)}, the anchor-title const from chunk 3.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-app-task-field-surfaces on a new branch feat/app-task-field-surfaces off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-task-field-surfaces-plan.md settling: parse_effort's grammar/errors (clap usage-error shape) and the shared formatter's contract; the TaskRow columns (fields on the row are the contract, layout is polish); the phrase helpers' names/shapes; the anchor filter's application points (which list/count surfaces) and its exclusion rule (exact title equality); the badge's read of the per-board health (selected board). Record every decision.
5. Commit as: docs(plan): task field surfaces, anchor hiding, sidecar badge. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the daily-driver implementation round ("Field surfaces + anchor hiding on lists + sidecar badge").

1. Read AGENTS.md, the implementation plan (§2 DDF-12, §3 chunk 5, §4), the named design sections + rulings, the dispatch plan §1/§6, and the stage-1 plan in ../taskboard-WT-feat-app-task-field-surfaces (branch feat/app-task-field-surfaces). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: no new top-level verbs; one effort grammar shared by parser and formatter; semantic phrases live in presentation/domain — never slint; the anchor filter is presentation-layer exact-title; the badge reads the derived health only; typed outcomes, no text assertions; SPDX headers (app/ui AGPL).
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the daily-driver implementation round (branch feat/app-task-field-surfaces).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-12, §3 chunk 5), the named design sections + rulings, the dispatch plan §1/§6 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: no field-specific verbs; parse_effort's failure shape matches parse_due's; none clears and stamps; decorations/presentation split clean (any slint-side classification is a finding); the anchor filter applied to list rows AND counts, exact-title only; the badge wired to the health, not to raw evidence; no today/search filter creep; typed outcomes throughout.
4. Code-quality mandate (naming/comments; SRP — app parses/dispatches, ui presents; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #209"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-app-task-field-surfaces.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 7. Chunk 6 — N-reminder verbs + surfaces (`feat/state-reminder-rule-verbs`)

- **Covers** (implementation plan §3 chunk 6 — normative; doc 3
  Part A D-R1–D-R4; DDF-13): domain `command.rs`/`messages.rs` —
  **retire** `SetReminderRule` (variant + handler + app mirror, one
  motion), `AddReminderRule {task, offset}` (dedup by offset: same
  offset ⇒ no actions, receipt `ReminderRuleAdded {rule, added:
  false}` naming the survivor; else mint + `UpsertReminderRule`),
  `RemoveReminderRule {rule}` (typed `UnknownReminderRule`;
  `DeleteReminderRule` cascade + one tombstone per removed rule
  through the chunk-4 sites), `ClearReminderRules` stays; app
  `commands/reminders.rs` (`RemindInvocation::Remove`, the receipt
  family), `cli.rs` (the one-mode-per-line group + `--remove`
  selectors: exact UUID | unique ≥8 prefix | the offset text form),
  `repl.rs`/`completion.rs`; ui — the task-editor rule-list
  composition (rows per rule with remove affordance + the add
  affordance with offset presets), dispatching through the action
  layer. **Non-goals**: banner/`snooze_target` unchanged (D-R4); no
  reminder center; no alarms (chunk 7).
- **Acceptance**: first-remind flow behaviorally identical;
  a second rule at a different offset adds; same-offset add = no
  actions + `added: false`; remove by all three selector kinds with
  typed resolution failures; rules sync (the chunk-4 lane);
  `SetReminderRule` gone (compile-enforced, no alias); the
  CHANGELOG flags the semantic change; gates green; PR body carries
  `Closes #204`.
- **Traps**: `SetReminderRule` kept as an alias or silently
  reinterpreted; dedup planning a second rule anyway; selectors
  erroring as text instead of typed outcomes; the tombstone path
  missed for `ClearReminderRules` (N tombstones); the banner's
  snooze machinery touched; engine snapshots not regenerated
  deliberately.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the daily-driver implementation round ("N-reminder verbs + surfaces", branch feat/state-reminder-rule-verbs) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 4 merged on origin/main (verify; else STOP and report).

1. Read AGENTS.md, the implementation plan (§2 DDF-13 verbs + DDF-8 enqueue, §3 chunk 6, §4–§6), the design doc .artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md (Part A §3.1–§3.4 D-R1–D-R4, §4 rulings incl. the retirement rationale, §5 test hooks), and the dispatch plan §1/§7.
2. Read the seams: crates/taskboard-domain/src/{command.rs (SetReminderRule/ClearReminderRules handlers, UnknownReminderRule, SnoozeReminder), messages.rs, reminders.rs (ReminderRule, occurrence_of)}, crates/taskboard-app/src/{commands/reminders.rs (set_on, RemindInvocation, collapse, the two-lane shape), cli.rs (the remind group), repl.rs, completion.rs}, crates/taskboard-ui-slint/src/{actions.rs, router.rs, the task editor compositions}, the chunk-4 enqueue sites.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-state-reminder-rule-verbs on a new branch feat/state-reminder-rule-verbs off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-reminder-rule-verbs-plan.md settling: the retirement motion's exact removal set; Add's dedup mechanics + receipt shape; Remove's planner/cascade/tombstone wiring; the CLI selector cascade + its typed failures; the receipts/--json shapes (the ADR 0009 envelope); the rule-list composition's data + actions; the snapshot/receipt regeneration scope. Record every decision.
5. Commit as: docs(plan): N-reminder verbs replacing replace-with-one. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the daily-driver implementation round ("N-reminder verbs + surfaces").

1. Read AGENTS.md, the implementation plan (§2 DDF-13, §3 chunk 6, §4), the design doc Part A + §4 rulings, the dispatch plan §1/§7, and the stage-1 plan in ../taskboard-WT-feat-state-reminder-rule-verbs (branch feat/state-reminder-rule-verbs). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: SetReminderRule is retired in one motion (no alias, no silent reinterpretation); dedup plans no actions and names the surviving rule; removes/clears emit one tombstone per removed rule through the chunk-4 sites; selectors resolve typedly; the banner and snooze_target are untouched; the CHANGELOG Unreleased line flags the semantic change explicitly; typed outcomes, no text assertions; no sleeps. SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG line (above). No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the daily-driver implementation round (branch feat/state-reminder-rule-verbs).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-13, §3 chunk 6), the design doc Part A + rulings, the dispatch plan §1/§7 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: SetReminderRule fully gone (a kept alias or a reinterpreted arm is a ruling violation); the dedup path plans zero actions and the receipt's added=false names the survivor; Clear emits N tombstones; selector failures typed; the banner path untouched; the sync lane exercised (a rule added here converges through the document); the semantic-change CHANGELOG line present; snapshot/receipt regens deliberate.
4. Code-quality mandate (naming/comments; SRP; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #204"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-state-reminder-rule-verbs.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 8. Chunk 7 — Standalone timers & alarms + 0008 (`feat/domain-adhoc-alarms`)

- **Covers** (implementation plan §3 chunk 7 — normative; doc 3
  Part B D-R5–D-R9 with the D-R6 ripple site table; DDF-13):
  domain `reminders.rs` — `AdhocAlarmId` (the `local_id!`
  registry), `AdhocAlarm`, `ReminderRef::Adhoc` declared **after
  `Default`**, `text_form`/`parse_text_form` `"alarm:<uuid>"`,
  `base_occurrence` arm, the `due_firings` alarms pass (membership =
  liveness; no mute), the `ReminderAnchor {Task | Alarm}` payload
  widening on `ReminderFiring`/`Notification` +
  `notifications_for`'s label fallback; `command.rs` —
  `CreateAdhocAlarm {label, when: AdhocWhen::{At, In}}` (planner
  stamps `In` = now + dur; `dur ≤ 0` typed rejection),
  `CancelAdhocAlarm` (`UnknownAdhocAlarm`), the `SnoozeReminder`
  `Adhoc` arm (no `live_task` check), `ClearBoardContent`'s
  `Adhoc(_) => false`; `persistence.rs` — `Upsert/DeleteAdhocAlarm`
  + the settlement prune **after** snooze compaction (prune-unobservability
  property); storage `0008_adhoc_alarms.sql` + codec
  (deliveries/snoozes ride the text codec unchanged); app
  `commands/timers.rs` (new, two-lane `add_on`/`list_on`/
  `cancel_on`), `cli.rs` `TimersCmd` (`--in`/`--at`, host-local
  parsing at the edge; selectors UUID/prefix/label-prefix),
  `repl.rs`; ui `presentation.rs` `ReminderTarget` widening (banner
  task-only). **Non-goals**: no UI surface (#352), no alarm sync
  (device-local: no op, no section), no sub-second timers, no
  insistence.
- **Acceptance**: the D-R6 site table walked arm by arm (every
  exhaustive `ReminderRef` match deliberate); timers fire within one
  tick in paused-time engine tests; settlement prunes
  (property-pinned); `ReminderState` widening is this chunk's insta
  pass; `timers` verbs round-trip typedly; gates green; PR body
  carries `Closes #211`.
- **Traps**: `Adhoc` declared before `Default` (the ordering
  ruling); a mute check applied to alarms; the prune pass ordered
  before compaction (the property catches it); `CreateAdhocAlarm`
  accepted `In(dur ≤ 0)`; an outbox op enqueued for alarms
  (device-local ruling); `--at` parsed in the domain (CLI edge);
  UI chrome creeping in.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of the daily-driver implementation round ("Standalone timers & alarms + 0008", branch feat/domain-adhoc-alarms) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 6 merged on origin/main (verify; else STOP and report).

1. Read AGENTS.md, the implementation plan (§2 DDF-13 timers, §3 chunk 7, §4–§6), the design doc .artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md (Part B §3.5–§3.9 D-R5–D-R9 — the D-R6 site table is the chunk's checklist, §4 rulings, §5 test hooks), and the dispatch plan §1/§8.
2. Read the seams: crates/taskboard-domain/src/{reminders.rs (ReminderRef, text_form, base_occurrence, due_firings, ReminderFiring, notifications_for, the compaction pass), command.rs (SnoozeReminder, ClearBoardContent's board_reminders), persistence.rs (PersistenceAction, apply_actions, the DeleteReminderRule cascade precedent), ids.rs (local_id! registry)}, crates/taskboard-state/src/actor.rs (reminder_tick), crates/taskboard-storage-sqlite/src/{codec.rs (the reminder text-form delegation), migrations/ + repo.rs (the 0004 reminder tables)}, crates/taskboard-app/src/{cli.rs, repl.rs, commands/ (the two-lane pattern)}, crates/taskboard-ui-slint/src/presentation.rs (ReminderTarget).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-domain-adhoc-alarms on a new branch feat/domain-adhoc-alarms off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-adhoc-alarms-plan.md settling: the D-R6 site-table walk (each site's deliberate arm); AdhocAlarm's serde/storage shape + 0008's SQL; the anchor payload widening's consumer list (sink mapping, tick correlation, proptest re-derivations); the prune pass's exact position and its property; AdhocWhen's stamping rule + rejection; the timers CLI grammar + selector cascade + receipts; the strategy extensions (referentially consistent alarm rows). Record every decision.
5. Commit as: docs(plan): standalone timers and alarms. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of the daily-driver implementation round ("Standalone timers & alarms + 0008").

1. Read AGENTS.md, the implementation plan (§2 DDF-13, §3 chunk 7, §4), the design doc Part B + rulings, the dispatch plan §1/§8, and the stage-1 plan in ../taskboard-WT-feat-domain-adhoc-alarms (branch feat/domain-adhoc-alarms). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: Adhoc declared after Default; alarms ignore mute; the prune runs after snooze compaction and the unobservability property lands with it; In(dur ≤ 0) is a typed rejection; alarms are device-local (no outbox op, no document section); --at parses host-local at the CLI edge; no UI surface; typed outcomes, no text assertions; no sleeps (paused-time tick tests). SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of the daily-driver implementation round (branch feat/domain-adhoc-alarms).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-13, §3 chunk 7), the design doc Part B + the D-R6 site table + rulings, the dispatch plan §1/§8 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: every ReminderRef match site grew a deliberate arm (walk the site table against the diff); the declaration order; no mute application to alarms; the prune's ordering + its property; AdhocWhen stamping/rejection; no outbox op or document key for alarms; the anchor widening applied at ReminderFiring AND Notification with the label fallback; 0008 conventions; the ReminderState insta pass is the widening only; no UI chrome.
4. Code-quality mandate (naming/comments; SRP; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #211"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-domain-adhoc-alarms.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 9. Chunk 8 — Reminder toast sink (`feat/app-desktop-notifications`)

- **Covers** (implementation plan §3 chunk 8 — normative; doc 3
  Part C D-R10 (stack GRANTED) / D-R11–D-R13; DDF-13 sink): root
  `Cargo.toml` — `notify-rust` 4.18.x pinned once in
  `[workspace.dependencies]` (default features, no image feature;
  `z-with-tokio` decided by measurement, recorded either way);
  app `notifications.rs` — `DesktopSink` implementing the
  **unmoved** `NotificationSink` port (payload mapping: title =
  `Notification.title` (task snapshot or alarm label), body = the
  occurrence rendering, anchor carried for future click-through);
  `config.rs` — `[reminders] sink = "auto"|"log"|"desktop"` strict
  decode; `bootstrap.rs` — `auto` resolves by `AppMode`
  (Desktop→DesktopSink; Daemon/Kiosk→LogSink); the mapping
  unit-tested at the port boundary. **Non-goals**: no port/signature
  move, no action buttons (GNOME drops `actions`), no sound/
  escalation (#203), no retry/fallback-send, no live-swap seam.
- **Acceptance**: desktop toasts with zero config under `auto`;
  daemon/kiosk byte-for-byte today's LogSink path; a failed
  delivery is a logged `NotifyError` — never retried, never
  fallback-sent; `cargo deny check` green over the new per-target
  tree; the `z-with-tokio` decision recorded with its measurement
  or its deferral reason; gates green; PR body carries
  `Closes #344`.
- **Traps**: the sink chosen at runtime / live-swapped; a
  fallback-to-log after an ambiguous failure (double-delivery
  risk — the ruling forbids it); toasts carrying action buttons;
  the dependency pinned anywhere but the workspace root; `deny`
  allowances added without the gate demanding them; the port
  widened (it must not move).

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of the daily-driver implementation round ("Reminder toast sink", branch feat/app-desktop-notifications) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 7 merged on origin/main (verify; else STOP and report).

1. Read AGENTS.md, the implementation plan (§2 DDF-13 sink, §3 chunk 8, §4–§6), the design doc .artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md (Part C §3.10–§3.13 D-R10–D-R13 + the granted-stack ruling + the evidence table, §5 test hooks), and the dispatch plan §1/§9.
2. Read the seams: crates/taskboard-app/src/{notifications.rs (the port, LogSink, spawn_notifier), config.rs (RemindersSection, AppMode), bootstrap.rs (the sink wiring point)}, root Cargo.toml (the workspace pinning discipline), deny.toml.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-app-desktop-notifications on a new branch feat/app-desktop-notifications off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-desktop-notifications-plan.md settling: the exact pin + features; DesktopSink's structure (what stays behind the port; the payload mapping as pure functions testable without a bus); the config decode + auto resolution; the z-with-tokio measurement plan (what is measured, or why deferral is recorded); the deny expectations (expected transitives). Record every decision.
5. Commit as: docs(plan): desktop notification toast sink. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of the daily-driver implementation round ("Reminder toast sink").

1. Read AGENTS.md, the implementation plan (§2 DDF-13, §3 chunk 8, §4), the design doc Part C + the granted-stack ruling, the dispatch plan §1/§9, and the stage-1 plan in ../taskboard-WT-feat-app-desktop-notifications (branch feat/app-desktop-notifications). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: the NotificationSink signature does not move; the pin lives once in [workspace.dependencies]; no action buttons, no sound, no escalation; failure = logged NotifyError, never retried, never fallback-sent; sink choice is boot-time config; auto resolves by AppMode; the mapping is unit-tested at the port boundary; typed outcomes. SPDX header (AGPL).
3. Run the full gate suite (AGENTS §10) green — cargo deny over the new per-target tree explicitly.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the deny tree outcome, the z-with-tokio decision + evidence, deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of the daily-driver implementation round (branch feat/app-desktop-notifications).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-13, §3 chunk 8), the design doc Part C + the granted-stack ruling, the dispatch plan §1/§9 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: the port signature byte-identical; everything platform-specific behind DesktopSink; the pin exactly as granted (4.18.x, default features, no image, single workspace entry); no retry/fallback path exists; boot-time resolution only; auto's mode matrix tested; deny green with allowances only where the gate demanded; the z-with-tokio decision recorded.
4. Code-quality mandate (naming/comments; SRP — the sink maps payloads, the consumer owns policy; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #344"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-app-desktop-notifications.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 10. Chunk 9 — Fuzzy search: `find.rs` + top-level `find` + palette (`feat/domain-find`)

- **Covers** (implementation plan §3 chunk 9 — normative; doc 4
  Part C D-V8–D-V10 (D-V9 as amended); DDF-14): domain `find.rs` —
  new module: `SearchTarget {Task, Stack, Board}`,
  `MatchQuality {Exact, Prefix, WordStart, Substring, Subsequence}`
  (declaration order = rank), `SearchHit`, `find(state, query)` —
  case-folded subsequence classification, ranking key
  `(quality, title_len, canonical_id)`, live boards/stacks +
  `is_live()` tasks (done included), title-only, empty query = no
  hits, the in-module proptest suite (determinism, case-fold
  invariance, empty-query floor, exact-title invariant, class
  monotonicity, growth monotonicity, type-filter partition,
  totality), the **crate-swap firewall** (public surface =
  contract; internals + algorithm-shaped properties private and
  in-module); app — top-level `CliCommand::Find {query, --type,
  --limit}` + `commands/find.rs` (`FindRow`: kind, id, title,
  context) + REPL `find <QUERY>` (exit 0 on well-formed queries,
  empty table on no hits); ui — flip reserved `PaletteSearch`,
  `Flow::Palette {selected}` + `ContextId::Palette` (the chain
  flip), `search-palette.slint` (text field + 8-row capped result
  list + overflow count, calling domain `find` per keystroke, no
  debounce), `IdTables.palette` indices-back, menu-cursor action
  context widening, jump-to per hit kind (board select + focus +
  view), the anchor exact-title filter on result rows
  presentation-side, Esc/`MenuDismiss` closes; the palette journey
  leg; registry/keymap snapshot regen (this chunk's insta pass).
  **Non-goals**: no description/label search, no typed query
  grammar, no debounce machinery, no recent-jumps, no kiosk surface.
- **Acceptance**: the property suite green; `find` exits 0 with
  empty tables for no hits; the palette jumps and the journey
  asserts focus/board structurally; the anchor task never a visible
  palette row; **stage 3 posts the amendment note on #348** (the
  top-level verb departing the issue's `tasks find` wording, per
  D-V9); gates green; PR body carries `Closes #348`.
- **Traps**: a numeric score introduced anywhere; the totality
  tiebreak missing (two entities sharing the full key);
  algorithm-shaped properties leaking into the public contract
  (the firewall's boundary); ranking duplicated UI-side (the
  palette must call the domain function); the anchor filter
  applied inside `find` (domain stays convention-blind — the
  ruling); `--type` validated loosely; the query debounced
  (speculative); results unbounded (the 8-row cap is the design).

**Prompt 9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 9 of the daily-driver implementation round ("Fuzzy search: find.rs + top-level find + palette", branch feat/domain-find) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 3 merged on origin/main (the palette's anchor filter consumes chunk 3's domain anchor-title const — verify the const exists on origin/main). Chunks 4–8 may be in any state; this chunk does not depend on them. If the precondition fails, STOP and report.

1. Read AGENTS.md, the implementation plan (§2 DDF-14, §3 chunk 9, §4–§6), the design doc .artifacts/plans/2026-10-19-views-finding-design-plan.md (Part C §3.8–§3.10 D-V8–D-V10 + the firewall paragraph + §5's amendment record, §6 chunk mapping, §7 item 5), and the dispatch plan §1/§10.
2. Read the seams: crates/taskboard-domain/src/{state.rs (AppState maps), entities.rs (is_live/is_open, canonical ordering precedents)}, crates/taskboard-app/src/{cli.rs (the top-level verb family), commands/output.rs (row projections), repl.rs, completion.rs}, crates/taskboard-ui-slint/src/{actions.rs (the reserved PaletteSearch row + menu-cursor actions), router.rs (reserved arms, DispatchOutcome), view_state.rs (Flow, the buffer mirror, begin_create's opening), models.rs (push, IdTables, BANNER_ROW_CAP), router/keymap presets, app-window.slint (the overlay mount pattern), tests/journeys.rs}.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-domain-find on a new branch feat/domain-find off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-find-plan.md settling: the classifier's exact class rules + the totality tiebreak; the property catalogue split contract-shaped vs algorithm-shaped; the firewall's module layout; the CLI's flag validation + FindRow contract; the palette's flow/context/cursor wiring + the per-keystroke call path; IdTables.palette's shape; the jump-to matrix per hit kind; the anchor filter's row-level application; the journey leg design. Record every decision.
5. Commit as: docs(plan): fuzzy search core, find verb, and palette. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 9.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 9 of the daily-driver implementation round ("Fuzzy search: find.rs + top-level find + palette").

1. Read AGENTS.md, the implementation plan (§2 DDF-14, §3 chunk 9, §4), the design doc Part C + the amendment record, the dispatch plan §1/§10, and the stage-1 plan in ../taskboard-WT-feat-domain-find (branch feat/domain-find). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: no numeric score exists anywhere; the public surface is exactly the four contract items; internals + algorithm-shaped properties stay in find.rs's own test module; the palette calls the domain find (no UI-side matching); the anchor filter is presentation-side only (find itself stays convention-blind); 8-row cap + overflow count; no debounce; empty query = no hits; exit 0 on well-formed queries; typed outcomes, no text assertions (hit orders and focus targets asserted structurally). SPDX headers.
3. Run the full gate suite (AGENTS §10) green. The registry/keymap insta pass is this chunk's one deliberate regeneration.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4 — stage 3 posts the amendment note). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 9 of the daily-driver implementation round (branch feat/domain-find).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-14, §3 chunk 9), the design doc Part C + §5's amendment record + §7 item 5, the dispatch plan §1/§10 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: the ranking key is the tuple — any numeric score is a finding; the totality tiebreak proven by the property; the firewall holds (no consumer touches internals; the property suite lives in-module); find excludes nothing by convention (the anchor filter is only in the palette row mapping — and it IS there); the CLI validates --type/--limit typedly and exits 0 on no hits; the palette wires the chain-flip context + menu cursors + jump-to per kind + Esc close; the journey leg asserts structurally; the 8-row cap present; registry/keymap regen deliberate.
4. Code-quality mandate (naming/comments; SRP — domain ranks, app prints, ui presents; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #348"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Post the amendment note on #348 (one comment: the verb is the top-level `find`, departing the issue's `tasks find` wording per the accepted D-V9 amendment; PR URL — MD-4's ordered exception). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-domain-find.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 11. Chunk 10 — Today view + inbox + capture + move ring (`feat/ui-today-inbox`)

- **Covers** (implementation plan §3 chunk 10 — normative; doc 4
  Parts A/B D-V1–D-V7 (D-V5 **v2**); DDF-15): domain `days.rs` —
  `today_sections(tasks, now, window)` + the partition/bounds/
  first-match/done-move proptests; `messages.rs`/`command.rs` —
  `CaptureTask {title, board: Option<BoardId>}` with the v2
  resolution rules (global Inbox: live board titled exactly
  `Inbox`, canonical `(order,id)` first, none ⇒ create unbound
  board + one `Inbox` stack, append to canonical-first live stack;
  `Some(board)` ⇒ its `Backlog` stacks, canonical-first, created on
  demand; never errors on the happy path); ui — `ViewId::Today`
  (subjectless, always-enterable, in the cycle, **default desktop
  start view**), `ContextId::Today` + the task-verb registry rows'
  context widening, `view.today` (`UiEffect::FocusToday`),
  `task.due-today` (re-commit `Some(Some(window_end))`,
  confirmation-free, via `DispatchCtx.day_window`),
  `view.inbox` (quietly no-op with no Inbox board), the adapter
  holding `(day_start_time, offset)` per the consumers contract +
  window computation at refresh/dispatch, `today-view.slint`
  (section headers + task-cards + done-count caption + EmptyState,
  empty sections collapse), cross-board rows (board badge when >1
  live board; selection-follows-focus retargets `ViewState.board`),
  kiosk = focused-board subset compact passive, `Flow::Capture` +
  `capture-overlay.slint` (Enter commits-and-continues to the
  **global Inbox**, Esc closes, empty-Enter closes), the **flat
  cross-board move ring** (`board › stack` hint — a deliberate
  behavior change to ordinary moves), `inbox_count` + the toolbar
  chip (desktop only), `IdTables.today_tasks`, the anchor filter on
  today rows + counts; app — the `capture <TITLE> [--board]` verb
  + `commands/capture.rs`, the `tasks add` Backlog default,
  `repl.rs`; the today + triage journey legs; golden-visual +
  registry regen (this chunk's insta pass). **Non-goals**: no
  time-ruler layout, no capture overlay fields, no PDF/`tasks
  today` verbs, no kiosk chrome, no cross-board app-field repair
  (#350), no new undo semantics.
- **Acceptance**: the section proptests green; day rollover proven
  by stepped-`now` `push` calls (no sleeps); the re-commit writes
  the window end; first capture creates the unbound Inbox board +
  stack + task (engine snapshots); an existing `Inbox`-titled board
  is adopted; `Some(board)` lands in Backlog; the move ring crosses
  board edges with readable hints and the journey pins it; the chip
  reads zero after a triage move (structural assertion); gates
  green; PR body carries `Closes #346` + `Closes #347`.
- **Traps**: "today" computed anywhere but `day_window` +
  `today_sections` (a stored is-today bit is a finding); the
  today view reading config mid-flight (the adapter holds the
  pair); the default-view change leaking to kiosk/daemon; the
  re-commit asking for confirmation (ruled confirmation-free) or
  restoring old clocks (it is a fresh `Some(Some(_))` write);
  capture landing in the focused board (the v1 — explicitly
  rejected); the Inbox auto-bound (binding is the user's ceremony);
  the move ring special-casing the Inbox board; the kiosk filter
  implemented as mode-specific code paths instead of a
  presentation filter; anchor rows visible in today/counts.

**Prompt 10.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 10 of the daily-driver implementation round ("Today view + inbox + capture + move ring", branch feat/ui-today-inbox) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunks 1, 2, and 3 merged on origin/main (the fields, day_window, and the anchor-title const; verify each — else STOP and report). Chunks 4–9 do not gate this chunk, but the default dispatch order lands them first; if any of 4–9 is still unmerged, note it and proceed only if the stage-1 plan's file inventory does not collide (ui view_state/actions/models + app cli/repl — chunks 5 and 9 collide; re-verify their merge state explicitly before proceeding).

1. Read AGENTS.md, the implementation plan (§2 DDF-15 + DDF-11 consumers contract, §3 chunk 10, §4–§6), the design doc .artifacts/plans/2026-10-19-views-finding-design-plan.md (Parts A/B §3.1–§3.7 D-V1–D-V7 — D-V5 as the v2 rewrite + its ruling, §4's decision record incl. the riders, §6 chunk mapping, §7 the ten-item reconciliation list — items 1/2/3/5/6/7/8/10 bind this chunk), and the dispatch plan §1/§11.
2. Read the seams: crates/taskboard-domain/src/{days.rs (chunk 2's day_window), entities.rs, messages.rs (CreateTask, UpdateTask, TaskChanges), command.rs (CreateBoard/CreateTask/CreateStack handlers — the resolution rules' building blocks)}, crates/taskboard-ui-slint/src/{view_state.rs (ViewId cycle, enter/step_view, Flows, the buffer mirror, the move target ring, commit_edit), actions.rs (registry + reserved rows), router.rs (DispatchCtx, DispatchOutcome), models.rs (push, IdTables, due_phrase wiring, decorations), presentation.rs (due_sets, banner_rows, live_board), adapter.rs (the repaint driver + REPAINT_CADENCE_MS, where (day_start_time, offset) lives), app-window.slint + ui/compositions (the overlay mount, toolbar, empty-state), tests/journeys.rs}, crates/taskboard-app/src/{cli.rs (the verb family), commands/, repl.rs, config.rs (AppMode)}, the multi-board chunk-7 board-bar/selection seams (ViewState.board, select_board).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-ui-today-inbox on a new branch feat/ui-today-inbox off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-daily-driver-today-inbox-plan.md settling at minimum: today_sections' exact section rules + proptest catalogue; the ViewId/context/action additions and the default-view change (desktop only — how the mode reaches ViewState::new); the adapter's window custody + its refresh/dispatch paths; DispatchCtx.day_window's plumbing; CaptureTask's handler design (resolution helpers as pure functions over AppState; the create-compose-apply sequence; receipts); the move-ring generalization (canonical board order, the hint model, the journeys it regenerates — the deliberate behavior change); the capture overlay's flow semantics; inbox_count + chip; the kiosk filter rule; the anchor filter points; the CLI verbs + the tasks add Backlog default; the golden/registry regen scope; the two journey legs. Record every decision; deviations from ACCEPTED rulings go back to the user.
5. Commit as: docs(plan): today view, global inbox, quick capture, move ring. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 10.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 10 of the daily-driver implementation round ("Today view + inbox + capture + move ring").

1. Read AGENTS.md, the implementation plan (§2 DDF-15, §3 chunk 10, §4), the design doc Parts A/B + §4 rulings + §7 reconciliation items, the dispatch plan §1/§11, and the stage-1 plan in ../taskboard-WT-feat-ui-today-inbox (branch feat/ui-today-inbox). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: every "today" question routes through day_window/today_sections — nothing derived is stored; the adapter holds (day_start_time, offset) and nothing reads figment mid-flight; the default-view change is desktop-only; re-commit is confirmation-free and a fresh Some(Some(window_end)) write; capture commits to the GLOBAL Inbox (board: None) always — --board is the explicit Backlog variant; the Inbox board is created UNBOUND (never auto-bound); resolution is deterministic and never errors on the happy path; the move ring is the flat cross-board ring with board › stack hints (the behavior change is deliberate — regenerate the affected journeys/snapshots in this chunk's pass); the kiosk rule is a presentation filter; anchor rows/counts filtered presentation-side; typed outcomes, no text assertions, no sleeps (stepped-now push calls prove rollover). SPDX headers.
3. Run the full gate suite (AGENTS §10) green. The golden-visual + registry pass is this chunk's one deliberate regeneration.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 10.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 10 of the daily-driver implementation round (branch feat/ui-today-inbox).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DDF-15, §3 chunk 10), the design doc Parts A/B + §4 rulings + §7 reconciliation items, the dispatch plan §1/§11 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: no stored is-today/derived bits; first-match placement (a task both scheduled and due renders once, in Scheduled); Done ordered by (done, id) with the count caption; the default-view change scoped to desktop; selection-follows-focus retargets ViewState.board (and the multi-board routing must NOT be "fixed" to re-scope the today view — the flagged Q8 departure is the design); re-commit confirmation-free via DispatchCtx.day_window (NoTarget without focus/window); capture always global-Inbox; the Inbox created unbound with adoption of an existing exact-titled board; Backlog canonical-first; the flat ring crosses board edges with the hint; chip counts structurally; kiosk = focused-board subset presentation filter; anchor task invisible in today rows/counts; rollover proven by stepped now (any sleep is a finding); journeys extended.
4. Code-quality mandate (naming/comments; SRP — domain sections, ui presents, app parses; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #346" and "Closes #347"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-today-inbox.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 12. Chunk 11 — Docs + wave exit verification (`docs/daily-driver-wave`)

- **Covers** (implementation plan §3 chunk 11 — normative):
  `docs/architecture.org` (the field family's two sync lanes + the
  sidecar subsystem + reminders surfaces + find/today/inbox +
  `[preferences]`), `README.org` (new verbs `find`/`capture`/
  `timers`, the `tasks edit` flags, `[preferences]` +
  `[reminders] sink`, the anchor card's Deck-UX note),
  `CHANGELOG.md` (promotion of the per-chunk Unreleased lines into
  the wave's release-note shape), the exit verification record at
  `.artifacts/reports/<date>-daily-driver-wave-exit-report.md`
  with evidence pointers into the verifying chunks' tests per the
  implementation plan §5's table (incl. the tier records of MD-5).
- **Deliverable**: docs + the report + the one ordered tracker
  comment (below). No code.
- **Acceptance** (evidence & artefact mandate): every exit-criterion
  row cites its verifying test/PR; the tier-2/1.17.5 records are
  quoted honestly (including anything that could not run); the
  **#202 milestone-clear comment** posted (rules half landed via
  chunk 4; proofs ride the future D5 wave — framing D-D2's record);
  no other tracker edits; gates green (docs-only).
- **Traps**: exit-criteria claims without evidence pointers;
  drift between the docs and the landed behavior; promoting
  CHANGELOG lines that misstate the semantic changes (the remind
  verb, the move ring, the default view).

**Prompt 11.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 11 of the daily-driver implementation round ("Docs + wave exit verification", branch docs/daily-driver-wave) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunks 1–10 ALL merged on origin/main (verify each branch tip is contained in origin/main). If any is missing, STOP and report.

1. Read AGENTS.md, the implementation plan (§3 chunk 11, §5 exit-criteria table, §6 bookkeeping), the five design docs' decision records, and the dispatch plan §1/§12.
2. Survey the landed round: every chunk PR (bodies, stage reports, tier records), the CHANGELOG Unreleased section, docs/architecture.org + README.org as they stand.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-docs-daily-driver-wave on a new branch docs/daily-driver-wave off main.
4. Write the chunk plan at .artifacts/plans/<today>-daily-driver-wave-exit-docs-plan.md: the doc-amendment outline per file, the exit-report skeleton mapped row-by-row to the implementation plan §5's table with each row's intended evidence source, the CHANGELOG promotion plan. Record every decision.
5. Commit as: docs(plan): daily-driver wave exit docs. Do not implement; do not push.

Report: plan path, decisions, risks.
```

**Prompt 11.2 (implementation):**

```text
You are the stage-2 EXECUTION agent for chunk 11 of the daily-driver implementation round ("Docs + wave exit verification").

1. Read AGENTS.md, the implementation plan (§3 chunk 11, §5, §6), the dispatch plan §1/§12, and the stage-1 plan in ../taskboard-WT-docs-daily-driver-wave (branch docs/daily-driver-wave). Recreate the worktree if missing.
2. Execute exactly per the approved plan: amend architecture.org/README.org; promote the CHANGELOG lines; write .artifacts/reports/<today>-daily-driver-wave-exit-report.md with every exit-criterion row carrying its evidence pointer (test path/PR/tier record — quote honestly what could not run). Every factual claim carries evidence; no production code; scratch notes never land.
3. Run the gate suite (AGENTS §10) — docs-only changes must keep every gate green.
4. No tracker edits from stage 2 (stage 3 posts the one ordered comment). No push, no PR.

Report: what was written, evidence coverage per exit row, deviations.
```

**Prompt 11.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 11 of the daily-driver implementation round (branch docs/daily-driver-wave).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§3 chunk 11, §5, §6), the dispatch plan §1/§12 — the evidence & artefact mandate applies to you in full.
3. Functional focus: every exit-criterion row has a real evidence pointer (verify the pointers resolve — the cited tests exist and are the cited behavior's); tier records quoted honestly; the docs match the landed code (spot-check each new verb/flag/config key against the source); the CHANGELOG promotion states the three semantic changes (remind add-semantics, the move ring, the desktop default view) accurately.
4. Evidence & artefact mandate: claims carry evidence; the diff is exactly the docs + report; no scope creep.
5. Re-run the gates; failures are findings. Fix everything on the branch.
6. Ship: push, open the PR, checks to green, mergeStateStatus CLEAN. Post the one ordered tracker comment on #202 (milestone-clear: rules half landed via chunk 4; proofs ride the future D5 wave — framing D-D2; PR URL). No other tracker edits. Never merge; never touch main.
7. Teardown: git worktree remove --force ../taskboard-WT-docs-daily-driver-wave.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed, the #202 comment URL.
```

## 13. Completion bookkeeping

- Closures ride the PR bodies (`Closes #N` per the implementation
  plan §6): chunk 1 → #208; 2 → #345; 4 → #207 + #244; 5 → #209;
  6 → #204; 7 → #211; 8 → #344; 9 → #348; 10 → #346 + #347.
- Annotations: one progress comment per mapped issue per chunk
  (impl §6 table), posted by stage 3 at PR open.
- Two ordered exceptions: chunk 9's #348 amendment note; chunk 11's
  #202 milestone-clear comment (rules half done; proofs → the D5
  wave). Nothing else — no label/milestone edits, no new issues.
- **#349 closed 2026-10-19** with the design round's completion
  (PR #359); #176 stays the undo implementation carrier,
  unscheduled; #350–#357 are the design rounds' ordered tracker
  edits, all deliberately unscheduled.
- **Deliberately not dispatched here**: the implementation plan §1
  Out list (task-kind/respite/recurrence fields — the document
  schema reserves their key space; proofs/settings sections — D5;
  insistence #203; timers UI #352; the settings editor #351; the
  cross-board repair #350; decision support; `tasks add` flag
  parity) and every design doc's backlog-candidates section — the
  user promotes them per AGENTS §7.
- **What opens next**: once chunk 11 merges, the wave is exited —
  the milestone `wave-daily-driver` closes complete (all its issues
  closed through the chunks), and the roadmap's next waves (kiosk
  implementation, or Wave 2 "focus & reflection") are the user's
  scheduling call.
