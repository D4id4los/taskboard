# [Plan] Backlog Round 3 — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-03
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **12 chunks + 1 bookkeeping task**
  of backlog round 3, grouped into **4 batches** plus a no-branch
  finale. Chunks within a batch are parallel-safe under the lane rules
  of §1 and dispatch greedily in listed order; batches are sequential
  (a batch begins when every PR of the previous batch is merged).
  **Every chunk carries its three prompts fully fleshed out for direct
  copy-paste dispatch — one agent session per prompt** (user
  requirement, standing since round 2). No code in this commit.
- **Origin**:
  `.artifacts/reports/2026-11-03-backlog-round3-readiness-triage-report.md`
  (§7 commissioning record: ready set approved as recommended; #504,
  #592, #600 deferred to milestone `v0.2` "focus & reflection" with
  ruling comments). Per-chunk normative origins are the GitHub issue
  bodies plus the plans/reports they cite.
- **Goal (user-stated)**: after this round plus the GUI review round 4
  tail (chunks 9–11 own #352, #490/#491, #411 — **out of scope here**),
  no currently tackleable backlog issue remains open; every open issue
  carries a recorded reason (triage report §5).
- **Round-4 coordination (binding)**: GUI review round 4 is at chunk 9
  of 11 (`.artifacts/plans/2026-11-01-gui-review-round4-implementation-dispatch-plan.md`).
  Round-3 batches 1–2 touch no ui-slint territory and may run while
  round 4 finishes; batch 3 is gated on round-4 chunk 10's PR merge,
  batch 4 on chunk 11's. ui chunks must expect `models.rs`/`adapter.rs`
  rebase friction with the round-4 tail — standard rebase dance.
- **Status**: READY — batch 1 is dispatchable immediately.

## 1. Batches, chunks & ordering

| Batch | Chunk | Issue(s) | Branch | Covers (one line) | Size |
|---|---|---|---|---|---|
| 1 | 1.1 | #572 (+ rider #524) | `fix/repl-editor-rearm-race` | post-^C editor re-arm race fix; #524 recurrences re-triaged against it | M |
| 1 | 1.2 | #588 | `feat/break-rule-roaming` | board-level break-rules sidecar section; dismissals wire admits `break:` keys; proptest carve-out | M |
| 1 | 1.3 | #580 | `refactor/storage-codec-text-form` | codec.rs collapses to the delegation pattern; 18× clock-parse helper folds | M |
| 1 | 1.4 | #505 | `test/kani-recurrence-harnesses` | `#[cfg(kani)]` harnesses over the recurrence arithmetic | S |
| 1 | 1.5 | #327 | `test/app-mutants-oneoff` | capped cargo-mutants sweep (commands/ + ipc/) + survivor triage | M |
| 1 | 1.6 | #560 + #570 (+ scans #287/#336/#384) | `test/watchlist-pass-2026-11` | habits window contract ruling + fix; relay export race triage; recurrence scans | S/M |
| 2 | 2.1 | #599 | `feat/start-tracking-kind-aware-noop` | same-task different-kind closes and reopens per D-3 | S |
| 2 | 2.2 | #282 | `fix/dead-board-drain` | symmetric dead-board cancellation / write-side carve-out (wire-verified premise) | S/M |
| 2 | 2.3 | #506 | `feat/habit-ux-trio` | series↔habit convert verb, `paused_until`, streak-repair affordances | M |
| 3 | 3.1 | #590 | `feat/taskview-field-tooltips` | labelled-field wrapper tips the text-field sites (r4 D21 remainder) | S/M |
| 4 | 4.1 | #351 | `feat/gui-settings-editor` | settings view over a typed config-write port (logout write generalized) | M |
| 4 | 4.2 | #595 | `refactor/install-seam-macro` | `install_seam!` over the 110 adapter installs; call-shape catalogue re-derived | M/L |
| — | fin | #274, #277, #284, #285, #286 | *(no branch)* | register verification pass (bookkeeping) | S |

**Single-flight lanes and gates (binding when picking parallel sets
within a batch):**

1. **Domain/state + sync lane (hard).** Chunks touching the domain or
   state source or the sync crate — 1.2, 2.1, 2.2, 2.3 — are strictly
   serial, in that order; the batch table already spreads them across
   batches 1–2. Chunk 1.4 is `#[cfg(kani)]` test-only and exempt.
   Chunk 1.6 is expected test-only; **if** its #560 stage 1 rules the
   *production* calendar wrong (UTC is the contract), the production
   fix serializes behind 1.2's merge — stage 1 records which way it
   went and the dispatch honors it.
2. **App-crate lane (soft).** 1.1 (session/pty/tty machinery), 1.5
   (commands/ + ipc/ production + their mutants), and 1.6 (app test
   files) touch disjoint file families and may share batch 1; if
   reality diverges, the later chunk slips one batch.
3. **UI territory rule (soft, as round 2).** Two ui-slint chunks may
   share a batch only with disjoint view territories; chunk 4.2
   rewrites `adapter.rs` wholesale and goes **last of the ui chunks**
   (after 3.1 and 4.1 land).
4. **Round-4 gates.** Batch 3 requires round-4 chunk 10's PR merged
   into `origin/main`; batch 4 requires chunk 11's. A stage-1 agent
   finding its gate unmerged STOPs and reports.
5. **Migrations.** No chunk is expected to carry a schema migration
   (break rules roam via the appdata sidecar, not sqlite; the codec
   refactor is shape-identical). If a stage 1 proves otherwise, it
   takes the next free number (**0020** — 0018 respite and 0019
   tracker-kind both landed), and only one migration-carrying chunk
   may be in flight.
6. **Batch protocol** (unchanged from round 2): a batch begins when
   every PR of the previous batch is merged (the user merges on
   GitHub; `main` is branch-protected). Before dispatching any chunk,
   re-check its gates against the *actual* `origin/main`
   (`git fetch origin --prune && git log origin/main --oneline | head
   -60`); a not-yet-merged gate ⇒ STOP and report. Free disk bounds
   actual concurrency: each worktree costs 36–55G; the doctor is the
   gate.

**Dispatch mechanics** (unchanged from round 2): one agent session per
prompt, in order (N.1 → N.2 → N.3 within a chunk); never two agents
share a worktree. Every chunk works in `../taskboard-WT-<branch>`
(slashes as dashes) next to the project root (AGENTS §8), branched off
the then-current `main`, no PR ever stacked. **Branch names are never
reused after their PR lands** (AGENTS §8, issue #415): a resume/second
round works on `<branch>-v2` or re-branches off the fetched
`origin/main`. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never without `-b`, never on `main`). Any
agent creating or recreating a worktree runs `scripts/worktree-doctor.sh`
from the main checkout first — nonzero exit stops the chunk; stage 3
removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: runs the doctor (nonzero exit ⇒ STOP
   and report), verifies the chunk's gates against `origin/main`
   (gated chunks: unmerged gate ⇒ STOP and report), creates the
   worktree on a new branch off the current `main`, writes the
   TDD-first plan at `.artifacts/plans/<today>-<topic>-plan.md` in the
   house `[Plan]` style, resolving every open question with a recorded
   decision (deviations from the origin need a recorded reason),
   commits it (`docs(plan): …`). No implementation, no push.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch's PR already
   landed, use `<branch>-v2` per AGENTS §8), implements per the
   approved plan — TDD, every commit standalone (rebase-merge
   hygiene), full AGENTS §10 gate suite green via
   `scripts/run-quality-gates.sh`, `CHANGELOG.md` Unreleased entries
   where behavior is user-observable. Does **not** push or open a PR.
   Reports deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`**
   (`git fetch origin --prune`; rebase onto `origin/main`, dropping
   already-merged duplicate commits; resolve only real conflicts; push
   `--force-with-lease` if history was rewritten), then reviews the
   full branch diff versus the **post-rebase** base against the
   chunk's acceptance criteria + AGENTS rules + the mandate below,
   re-runs the full gate suite (any failure is a finding), and fixes
   every finding on the branch (amend/reword — never
   duplicate-subject fixups). Then ships: push, open the PR to `main`
   with `gh`, `gh pr checks --watch` to green (any failing check is a
   finding; fix, push `--force-with-lease`, repeat), verify `gh pr view
   --json mergeStateStatus` = `CLEAN` (`BEHIND`/`DIRTY` ⇒ rebase,
   push, re-run). **Then teardown**: from the project root, `git -C
   /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8; confirm
   via `git worktree list`). Handoff contract: "PR open + checks green
   + merge state `CLEAN` + worktree removed". Never merges; never
   touches `main`.
4. **Issue closures** land as stage-3 bookkeeping after the PR is
   green: one close comment per issue, self-contained (evidence +
   fixing commit/PR).

**Code-quality mandate** (stage 3, every chunk — verbatim from the
round-2 plan §1): beyond functional review — (a) code quality: naming,
comment discipline (comments state constraints, not narration),
idiomatic approved stack, no dead code or speculative generality, test
quality per AGENTS §5 (no text assertions, no sleeps, deterministic by
construction, no quality ceilings, a failing new test may be a
production bug and may not be weakened); (b) SRP: the crate-role
boundaries of AGENTS §2 are the SRP line — domain pure, engine
orchestrating, sync actor only syncing, CLI parsing/dispatching, UI a
dumb presentation client; a boundary violation is an SRP violation;
(c) a single level of abstraction per function body. Findings are
fixed on the branch.

**Evidence & artefact mandate** (chunks 1.5, 1.6, and any chunk
producing a report artefact): every factual claim carries evidence
(command + observed outcome, or verifiable source + what it states);
experiments reproducible from the method section; scratch never lands
— the PR diff is exactly the chunk's artefacts.

## 2. Batch 1 — open field (six parallel chunks)

### Chunk 1.1 — REPL editor re-arm race (#572, rider #524) (`fix/repl-editor-rearm-race`)

- **Covers** (normative): #572 (p1) — after a ^C during the masked
  read, the select-cancel race leaves the session without re-entering
  `readline()`; the editor never re-arms and the pty goes silent
  forever. The issue carries the reproduction recipe (bounded spinners
  + parallel build; ~1/25 loaded runs), the 600 s control excluding
  descheduling, and the suspect area (the race window between the
  select-cancel acknowledgement and the editor thread's return to
  `readline()`; ADR 0011/0014, plan §11 Q11). **Rider**: re-triage
  #524's two post-fix observations (2026-11-02 PR #586 coverage run;
  2026-11-03 chunk-7 gate run — both "no pty output for 30 s" under
  load) against the fix: same-mechanism hypothesis (the sibling test
  shares the masked-read ^C scene). Confirmed ⇒ attribute and close
  #524; disproven ⇒ record the capture policy outcome and leave #524
  open with the evidence. **Non-goals**: no harness changes (the
  progress-deadline seam is working as designed — it found this bug);
  no unrelated session-machinery refactors.
- **Acceptance**: root cause identified with evidence (a deterministic
  or reliably-loaded reproduction of the race, the mechanism
  explained from the code, not guessed); the fix makes the loaded
  reproduction pass (the #572 recipe's loop, ≥ the issue's sample
  size, plus the sabotage control); the session-survives invariant
  pins stay unweakened; #524's rider disposition recorded with
  evidence; existing cli_tty suite green.

**Prompt 1.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.1 of backlog round 3 ("REPL editor re-arm race", branch fix/repl-editor-rearm-race) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, then GitHub issues #572 (normative — signature, evidence, reproduction recipe, suspect area) and #524 (the rider — its two post-fix observations, 2026-11-02/03) and #524's close-history comment (the pty flake pass root cause, so you know what is already fixed).
2. Read the seams: the masked-read/session/editor machinery in crates/taskboard-app/src (grep for readline, masked, select-cancel, the ^C handling path — the ADR 0011/0014 files under docs/ for the session contract), the cli_tty test tier (tests/cli_tty.rs — the fails_typed_and_the_session_survives and never_persists_and_never_wedges tests, the progress-deadline seam, feed_at/wait_readline helpers), and the DR-2 signal-handling plan §11 Q11 (.artifacts/plans/ — the recorded select-cancel race question).
3. Gates: none (batch 1). Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b fix/repl-editor-rearm-race ../taskboard-WT-fix-repl-editor-rearm-race off main (AGENTS §8: never without -b, never on main).
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-repl-editor-rearm-race-plan.md: the reproduction protocol (the #572 recipe, its sample size, and how you will know the fix works — a loaded-loop pass rate plus a mechanism-level pin, e.g. a deterministic interleaving seam if one can be injected without changing production semantics); the root-cause hypothesis tree over the re-arm path (select-cancel acknowledgement vs editor-thread return to readline — enumerate the interleavings and what evidence discriminates them); the fix shape once the root cause is confirmed (no speculative fixes — plan says: evidence first, fix second); the #524 rider protocol (which observations count as same-mechanism, what attribution evidence looks like, the close-vs-stay decision rule); the test catalogue (no weakening of the session-survives pins; no timing assertions; deterministic-by-construction where possible). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the REPL editor re-arm race. No implementation, no push.

Report: plan path, the root-cause hypothesis tree summary, decisions, risks, estimated diff size.
```

**Prompt 1.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.1 of backlog round 3 ("REPL editor re-arm race").

1. Read AGENTS.md, issues #572/#524, the dispatch plan .artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md §1/§2 chunk 1.1, and the stage-1 plan in ../taskboard-WT-fix-repl-editor-rearm-race (branch fix/repl-editor-rearm-race). Recreate the worktree on that branch if missing.
2. Execute the plan's reproduction protocol first and record the evidence (transcripts into the report, not the repo); root-cause the re-arm race from evidence, then fix TDD: a failing pin that captures the mechanism (deterministic seam or the loaded loop per the plan), then the production fix. Hard rules: no test weakening (AGENTS §5); no sleeps; the progress-deadline seam itself is out of scope; zero unrelated production diffs.
3. Validate per the plan's loaded-loop protocol (the #572 recipe at ≥ the planned sample size) and run the sabotage control. Record outcomes.
4. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entry (user-observable fix). No push, no PR. Never push to or merge main.

Report: root cause with evidence, per-gate results, loaded-loop validation numbers, deviations.
```

**Prompt 1.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.1 of backlog round 3 (branch fix/repl-editor-rearm-race).

1. Read AGENTS.md, issues #572/#524, the dispatch plan §1/§2 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 shared model step 3.
2. Functional focus — this chunk's traps: the fix must address the demonstrated interleaving, not paper over it (check the fix against the recorded root cause — a fix that would also "work" for a different mechanism is suspicious); the re-arm must happen on every cancellation path, not just the reproduced one (audit the cancellation/abort arms of the session loop); no pin was weakened and no budget was silently raised (budgets are derived numbers per the flake pass conventions — any change to them is a finding unless the plan recorded it); the loaded validation must have actually run (numbers in the report, not assertions).
3. Re-run the loaded loop yourself at the plan's sample size (or state which subset the environment allowed) and the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled fix(app): the REPL editor re-arms after the masked-read cancel, checks green, mergeStateStatus CLEAN). Close #572 with the root cause + validation evidence + PR link. Execute the #524 rider disposition per the plan's decision rule — close with attribution, or comment with the evidence and leave open. Teardown per §1 step 3 (absolute anchor, git worktree list confirmation).

Report: verdict, findings, fixes, PR URL, green checks, #572 closed, #524 disposition, worktree removed. Never merge; never touch main.
```

### Chunk 1.2 — break-rule roaming (#588) (`feat/break-rule-roaming`)

- **Covers** (normative): #588 (p2) — the chunk-3.5 D-15 fence, recipe
  in the issue: (i) a board-level `break-rules` sidecar section
  following the reminder-rules section's recipe so rules roam;
  (ii) widening the dismissals wire grammar to admit `break:<uuid>`
  keys (the D-4 widening) so break dismissals sync; (iii) the
  roaming-id carve-out in the `no_local_ids_reach_the_dismissals_wire`
  proptest for the synced family (rule-row ids stop being
  device-local). `BreakRuleId` today sits in the device-local family
  (the `AdhocAlarmId` precedent) — the roaming move is the design
  change; the D-15 ruling and R-2/R-4 notes in the origin plan are
  normative. **Non-goals**: no change to entry roaming (already
  coherent); no respite-evaluation semantics change; no sqlite
  migration (sidecar is appdata).
- **Acceptance**: a rule created on one device is available on
  another via the sidecar (tier-1 wiremock pin + engine snapshot); a
  break dismissal on one device silences the break on another
  (journey); the proptest carve-out is minimal and typed (roaming
  family only — device-local ids still rejected); the fail-closed
  filter behavior for devices without the rule is unchanged and
  pinned; existing reminder-rules roaming untouched.

**Prompt 1.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.2 of backlog round 3 ("break-rule roaming", branch feat/break-rule-roaming) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, then GitHub issue #588 (normative — the recipe, the D-15 fence ruling) and the origin plan .artifacts/plans/2026-10-31-respite-break-reminders-plan.md (D-15, R-2/R-4) plus DR-5's plan section on the reminder-rules sidecar recipe (.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md) and DR-1's roam-set ruling it cites.
2. Read the seams: the reminder-rules sidecar section (sync crate — the section recipe to imitate: fold/merge grammar, conflict policy), ReminderState.break_rules and ReminderRef::Break in crates/taskboard-domain/src/reminders.rs, the dismissals wire grammar and its decode (where Rule|Default|Adhoc refs are keyed), the no_local_ids_reach_the_dismissals_wire proptest, and the respite/break evaluation filter (fail-closed wire filter).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b feat/break-rule-roaming ../taskboard-WT-feat-break-rule-roaming off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-break-rule-roaming-plan.md: the id-family move (BreakRuleId from device-local to roaming — what the type/enum membership change ripples through, and whether a new id family or the Rule family's grammar is the home; one call, recorded); the sidecar section shape (mirror the reminder-rules recipe exactly — versioning, merge-on-fold, resurrection carve-out if the OR-set applies); the dismissals-wire grammar widening (grammar version/compat story for peers that don't know break: keys — fail-closed skip, per the D-4 precedent); the proptest carve-out shape (typed family membership, not a string allowlist); the test catalogue (tier-1 wiremock roaming pin, engine snapshots, cross-device dismissal journey, fail-closed pin). Resolve every open question with a recorded decision.
5. Commit: docs(plan): break-rule roaming. No implementation, no push.

Report: plan path, the id-family decision, decisions, risks, estimated diff size.
```

**Prompt 1.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.2 of backlog round 3 ("break-rule roaming").

1. Read AGENTS.md, issue #588, the dispatch plan §1/§2 chunk 1.2, and the stage-1 plan in ../taskboard-WT-feat-break-rule-roaming (branch feat/break-rule-roaming). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: the failing roaming/dismissal pins first, then the id-family move, sidecar section, and wire widening. Hard rules: the fail-closed behavior for peers without the section is a contract — pin it, do not break it; the proptest carve-out is typed, minimal, and commented with the constraint; no sqlite migration (sidecar is appdata — if stage 1 recorded otherwise, follow its recorded decision and take migration 0020); zero diffs outside the plan's file map.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entry (cross-device behavior change). No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 1.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.2 of backlog round 3 (branch feat/break-rule-roaming).

1. Read AGENTS.md, issue #588, the dispatch plan §1/§2 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: the sidecar section must follow the reminder-rules recipe verbatim (versioning, merge, resurrection) — a hand-rolled variant is a finding; the wire widening must fail closed for old peers (pin exists and actually constrains); device-local families must still be rejected by the proptest (temporarily widen the carve-out and confirm it fails, then restore — record that you did); the break evaluation semantics are untouched (respite pins green unmodified).
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled feat(sync): break rules and their dismissals roam, checks green, mergeStateStatus CLEAN). Close #588 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.3 — storage codec text-form unification (#580) (`refactor/storage-codec-text-form`)

- **Covers** (normative): #580 — F5-10, re-verified 2026-11-02: the
  storage codec hand-writes TEXT forms for op tags, sync phases/error
  kinds, validator keys, importance/urgency/insistence, recur/baseline
  — collapse them to the `ReminderRef` delegation pattern
  (domain-owned `text_form`/`parse_text_form`, `codec.rs:397`) and
  fold the 18× clock-parse helper (`decode::clock`). Zero behavior
  change: every stored TEXT form round-trips byte-identically.
- **Acceptance**: codec.rs delegates for every family the audit
  listed; the clock-parse helper is folded; round-trip pins exist per
  family (existing value byte-identical — no migration, no format
  drift); the suite green with no test weakened.

**Prompt 1.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.3 of backlog round 3 ("storage codec text-form unification", branch refactor/storage-codec-text-form) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #580 (normative — families list, delegation precedent at codec.rs:397, the 18× decode::clock helper), the audit finding F5-10 (.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md, Axis H), and the re-verification D5 in .artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md §2.
2. Read the seams: crates/taskboard-storage-sqlite/src/codec.rs end to end (every hand-written TEXT encode/decode family + the ReminderRef delegation), the domain types owning each family's serde knowledge, and the existing codec round-trip tests.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b refactor/storage-codec-text-form ../taskboard-WT-refactor-storage-codec-text-form off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-storage-codec-text-form-plan.md: the per-family inventory (family → current site → delegation target in domain → round-trip pin), which families genuinely lack a domain text_form home and what gets added where (domain owns the knowledge; storage only delegates — SRP per AGENTS §2), the clock-helper fold (one parse site, shared), and the byte-identity evidence protocol (each family's existing stored form asserted unchanged before/after). Resolve every open question with a recorded decision.
5. Commit: docs(plan): storage codec text-form unification. No implementation, no push.

Report: plan path, the family inventory summary, decisions, risks, estimated diff size.
```

**Prompt 1.3.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.3 of backlog round 3 ("storage codec text-form unification").

1. Read AGENTS.md, issue #580, the dispatch plan §1/§2 chunk 1.3, and the stage-1 plan in ../taskboard-WT-refactor-storage-codec-text-form (branch refactor/storage-codec-text-form). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: byte-identity pins first (they must pass before AND after the refactor), then the per-family delegation moves. Hard rules: zero behavior change — a stored TEXT form that changes is a bug in the refactor, not a migration (no migration in this chunk); the domain gains pure text_form/parse_text_form knowledge only, no storage imports (AGENTS §2 boundaries); no test weakening.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh. No CHANGELOG (no user-observable change). No push, no PR. Never push to or merge main.

Report: per-gate results, the byte-identity evidence, deviations.
```

**Prompt 1.3.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.3 of backlog round 3 (branch refactor/storage-codec-text-form).

1. Read AGENTS.md, issue #580, the dispatch plan §1/§2 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: byte-identity is THE contract — verify every family has a before/after pin and that the pins are load-bearing (mentally flip an encoder and check a pin would fail); no serde/format drift anywhere (grep the diff for format!, serialization changes); the domain additions are pure (no storage/sync imports into domain — AGENTS §2); the clock-helper fold did not change parse semantics at edges (leap seconds absent, sub-second precision, negative offsets — whatever the existing helper's domain is, it is preserved).
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled refactor(storage): the codec delegates text forms to the domain, checks green, mergeStateStatus CLEAN). Close #580 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.4 — Kani recurrence harnesses (#505) (`test/kani-recurrence-harnesses`)

- **Covers** (normative): #505 — `#[cfg(kani)]` harnesses for the
  recurrence arithmetic (`next_after`, `roll_target`, `occurs_on`):
  pure, total over small domains, ideal bounded-model-check targets
  per the landed #426 Kani pilot's conventions; unwind bounds
  documented at each harness; never in the PR pipeline. Test-only —
  zero production diffs.
- **Acceptance**: harnesses follow the pilot's conventions exactly
  (attribute style, bound documentation, proof-vs-assume discipline);
  at least one harness finds its intended property provable (or the
  counterexample is triaged with a recorded disposition); the normal
  suite and gate run are untouched by the cfg gating.

**Prompt 1.4.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.4 of backlog round 3 ("Kani recurrence harnesses", branch test/kani-recurrence-harnesses) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #505 (normative), the landed #426 Kani pilot (its issue body, close comment, and its harness conventions in the domain crate — the cfg(kani) module layout), and the DR-6 plan §3.9 note (.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md).
2. Read the seams: the recurrence arithmetic (next_after, roll_target, occurs_on) in crates/taskboard-domain/src (their input domains — calendar types, period enums), the pilot's existing harnesses for style, and the workspace's kani toolchain setup (rust-toolchain/kani version pinning, how the pilot ran proofs).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b test/kani-recurrence-harnesses ../taskboard-WT-test-kani-recurrence-harnesses off main.
4. Write the plan at .artifacts/plans/2026-11-03-kani-recurrence-harnesses-plan.md: the harness catalogue (function → property → bounded domain → unwind bound and its derivation), the pilot's conventions restated as binding rules (no unwritten bounds, no proof-by-assume of the property under test), the run protocol (local proof runs, how results are recorded), and the counterexample triage rule (a found violation becomes evidence on the issue, not a silently weakened harness). Zero production diffs. Resolve every open question with a recorded decision.
5. Commit: docs(plan): Kani recurrence harnesses. No implementation, no push.

Report: plan path, the harness catalogue, decisions, risks.
```

**Prompt 1.4.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.4 of backlog round 3 ("Kani recurrence harnesses").

1. Read AGENTS.md, issue #505, the dispatch plan §1/§2 chunk 1.4, and the stage-1 plan in ../taskboard-WT-test-kani-recurrence-harnesses (branch test/kani-recurrence-harnesses). Recreate the worktree on that branch if missing.
2. Implement per the plan: the #[cfg(kani)] harness module with every planned harness, unwind bounds documented at each, pilot-convention style. Run every proof and record the outcome per harness (VERIFIED / counterexample). A counterexample is triaged per the plan's rule — never weaken the harness to pass; if it exposes a real arithmetic bug, STOP and report it (a failing new test may be a production bug — AGENTS §5).
3. Verify zero production diffs (git diff origin/main --stat must show test/cfg-gated files only) and that the normal suite ignores the module (cargo nextest -p taskboard-domain green, no kani in the default build).
4. Full gate suite via scripts/run-quality-gates.sh. No CHANGELOG. No push, no PR. Never push to or merge main.

Report: per-harness proof outcomes, gate results, deviations.
```

**Prompt 1.4.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.4 of backlog round 3 (branch test/kani-recurrence-harnesses).

1. Read AGENTS.md, issue #505, the dispatch plan §1/§2 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: each harness must actually constrain its function (a harness that would verify under an inverted property proves nothing — mentally flip one and check); unwind bounds are documented and derived, not magic numbers; no assume() that smuggles the property; the module is invisible to the default build (cfg discipline); zero production diffs.
3. Re-run a representative subset of the proofs yourself (or all, if time allows — state which) and the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled test(domain): Kani harnesses over the recurrence arithmetic, checks green, mergeStateStatus CLEAN). Close #505 with the proof-outcome table + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.5 — app mutants one-off (#327) (`test/app-mutants-oneoff`)

- **Covers** (normative): #327 — the capped cargo-mutants one-off for
  `taskboard-app`, scoped to `commands/` + `ipc/` first (the issue's
  command template is normative: `timeout --signal=INT
  --kill-after=90s 7200 env SQLX_OFFLINE=true cargo mutants -p
  taskboard-app --in-place --timeout-multiplier 2.0`, file-filtered;
  clean tree after the in-place run). The sweep runs **fresh at chunk
  start** (running it earlier ages the evidence — F14-02). Survivor
  triage follows the #427/#458 precedent: fix (production or test) or
  fence with a recorded reason per survivor; ignore-list additions
  only for provable equivalents, recorded.
- **Acceptance**: the sweep ran to completion under the cap with the
  run recorded; every survivor has a disposition (fixed with a
  load-bearing test, or fenced with a recorded reason); the tree is
  clean after the in-place run; no test weakened; the weekly sweep's
  scope (domain+state) untouched.

**Prompt 1.5.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.5 of backlog round 3 ("app mutants one-off", branch test/app-mutants-oneoff) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #327 (normative — churn facts, command template, scope cap), the prior one-off conventions (.artifacts/reports/ — the storage and sync mutants one-off reports: capping, in-place hygiene, survivor triage format), and the #427/#458 precedent (.artifacts/reports/2026-10-26-mutation-testing-ui-pure-modules-report.md for the triage discipline).
2. Read the seams: crates/taskboard-app/src/commands/ and src/ipc/ (the sweep scope — their test coverage shape, which tiers are subprocess-heavy and therefore expensive per mutant), the workspace cargo-mutants config, and how the previous one-offs scoped file filters at run time.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b test/app-mutants-oneoff ../taskboard-WT-test-app-mutants-oneoff off main.
4. Write the plan at .artifacts/plans/2026-11-03-app-mutants-oneoff-plan.md: the exact invocation (issue's template + the file-filter flags for commands/ and ipc/), the machine budget (cap, kill-after semantics, what a TIMEOUT survivor means in this tier — subprocess tests inflate it), the evidence format (mutants.out summary into the report; survivors listed with file:function:line), and the triage rules per survivor class (kill-with-test / production-fix / fenced-equivalent with reason; ignore-list only for provable equivalents, each recorded). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the app-crate mutants one-off. No implementation, no push.

Report: plan path, decisions, risks, expected runtime.
```

**Prompt 1.5.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.5 of backlog round 3 ("app mutants one-off").

1. Read AGENTS.md, issue #327, the dispatch plan §1/§2 chunk 1.5, and the stage-1 plan in ../taskboard-WT-test-app-mutants-oneoff (branch test/app-mutants-oneoff). Recreate the worktree on that branch if missing.
2. Run the sweep fresh per the plan's invocation (in-place, capped; the run IS the chunk's first act — do not pre-triage). Verify git status is clean after the run; record the summary. Then triage every survivor per the plan's rules: fixes are TDD (a killed mutant needs a load-bearing test — verify it fails with the mutant re-applied before claiming the kill); fences carry recorded reasons; no test weakening, no speculative refactors beyond what a fix requires.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh. CHANGELOG only if a production fix landed. No push, no PR. Never push to or merge main.

Report: the sweep record (counts, duration), the survivor disposition table, per-gate results, deviations.
```

**Prompt 1.5.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.5 of backlog round 3 (branch test/app-mutants-oneoff).

1. Read AGENTS.md, issue #327, the dispatch plan §1/§2 (code-quality mandate + evidence mandate apply), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: every claimed kill is real (spot-check by re-applying 2-3 mutants and confirming the new tests fail — record which); every fence has a reason that would survive re-derivation (a fence that is really "didn't want to" is a finding); the ignore-list gained only provable equivalents; no budget/scope inflation quietly narrowing the sweep (the filter is exactly commands/ + ipc/); the weekly sweep config untouched.
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled test(app): the capped mutants one-off over commands and ipc, checks green, mergeStateStatus CLEAN). Post the survivor disposition table in the PR body. Close #327 with the sweep record summary + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.6 — watchlist pass (#560 + #570, scans over #287/#336/#384) (`test/watchlist-pass-2026-11`)

- **Covers** (normative): #560 — the habits daily-window oracle asserts
  the UTC date (`cli_habits.rs:125`) while production resolves the
  local calendar; fails deterministically while the local date leads
  UTC. The chunk rules the contract (which calendar owns the natural
  daily window) with a recorded decision, then fixes oracle or
  production accordingly — no weakening to tolerate both. #570 — first
  observation of the relay-export instance-locked race (cli_ipc t16);
  bounded triage only (repro attempt under load per the #572 recipe
  shape, capture policy, disposition — no speculative fix without
  repro). Plus the standing recurrence scans over #287/#336/#384
  (bounded `gh run list` + comment-thread pass; disposition comments
  only). **Lane note**: #560 is expected test-only; if its ruling
  makes it a production fix in domain code, that fix serializes
  behind chunk 1.2's merge (§1 lane 1) — stage 1 records which.
- **Acceptance**: #560's contract ruling recorded with its reasoning;
  the fix lands (oracle or production) with a pin that fails under
  the wrong calendar; #570 carries a triage disposition (repro
  evidence or bounded-negative record + capture policy) — closed only
  if root-caused/fixed, else stays open with the evidence; #287/
  #336/#384 get dated scan comments; no test weakened anywhere.

**Prompt 1.6.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.6 of backlog round 3 ("watchlist pass 2026-11", branch test/watchlist-pass-2026-11) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§5, §7 flaky rules), then GitHub issues #560 (normative — deterministic window, the contract question), #570 (first-observation triage), and #287/#336/#384 (their standing capture policies and last scan comments — 2026-11-01).
2. Read the seams: crates/taskboard-app/tests/cli_habits.rs (the h02 oracle at :125 and the window it asserts), the production daily-window resolution it oracles (grep the natural-window/habit-receipt code — likely domain habits), the recent house direction on local time (commits c228159f, 3aa7cb96 — the due grammar and goldens moved to local-calendar contracts), tests/cli_ipc.rs t16 (the relay export flow, instance-lock handling), and the #572 reproduction recipe for the load-shape.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b test/watchlist-pass-2026-11 ../taskboard-WT-test-watchlist-pass-2026-11 off main.
4. Write the plan at .artifacts/plans/2026-11-03-watchlist-pass-plan.md: the #560 contract ruling framework (what the natural daily window means for a habit user — the local-calendar reading and its UTC alternative, with the house's recent local-time direction as evidence; ONE recommendation, the user-visible semantics argument, not convenience), the fix shape for either ruling (oracle+comment vs production window resolution + its pin), the repro protocol for #570 (bounded: N loaded iterations of the t16 test, capture policy on failure), and the scan protocol for #287/#336/#384 (gh run list window since 2026-11-01 + thread read; comment format). Resolve every open question with a recorded decision; if the #560 ruling implies domain production changes, record the 1.2-serialization explicitly.
5. Commit: docs(plan): the November watchlist pass. No implementation, no push.

Report: plan path, the #560 recommendation, decisions, risks.
```

**Prompt 1.6.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.6 of backlog round 3 ("watchlist pass 2026-11").

1. Read AGENTS.md, issues #560/#570 (+ #287/#336/#384 for the scans), the dispatch plan §1/§2 chunk 1.6, and the stage-1 plan in ../taskboard-WT-test-watchlist-pass-2026-11 (branch test/watchlist-pass-2026-11). Recreate the worktree on that branch if missing.
2. #560: execute the ruling — if test-only, fix the oracle and its comment (the pin must fail under the wrong calendar: prove it by temporarily flipping the oracle's calendar and observing the failure, then restore); if production, implement per the plan and honor the 1.2-serialization gate (STOP and report if 1.2 is unmerged). #570: run the bounded repro protocol; record evidence or the bounded negative. Hard rules: no weakening to tolerate both calendars (AGENTS §5); no sleeps; #570 gets no speculative fix.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh. CHANGELOG only if a production fix landed. No push, no PR. Never push to or merge main.

Report: the #560 ruling outcome and evidence, #570 repro record, per-gate results, deviations.
```

**Prompt 1.6.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.6 of backlog round 3 (branch test/watchlist-pass-2026-11).

1. Read AGENTS.md, issues #560/#570/#287/#336/#384, the dispatch plan §1/§2 (code-quality + evidence mandates apply), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: the #560 pin is calendar-load-bearing (verify the flip-proof was actually done and recorded); the ruling's reasoning is semantic, not convenience; #570's record states exactly what ran (iterations, load shape, outcome) — no overclaiming; if production changed, the 1.2 gate was honored and the pin covers the window boundary (the 00:00–01:00 CET shape); no capture policy was deleted from any watchlist issue.
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled test(app): the November watchlist pass — the habits window contract, checks green, mergeStateStatus CLEAN). Close #560 with the ruling + evidence + PR link. Comment #570's disposition on the issue (close only if root-caused and fixed — otherwise stays open with the evidence). Post dated scan comments on #287/#336/#384. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, dispositions, worktree removed. Never merge; never touch main.
```

## 3. Batch 2 — the domain/sync lane (three serial chunks; batch opens when 1.2's PR is merged)

### Chunk 2.1 — kind-aware StartTracking no-op (#599) (`feat/start-tracking-kind-aware-noop`)

- **Covers** (normative): #599 — the landed planner arm
  (`open.len() == 1 && open[0].task == *task` ⇒ `NoOp`) is kind-blind;
  make it kind-aware per chunk 3.5's D-3 intent: same task + same
  kind ⇒ `NoOp`; same task + different kind ⇒ close the open entry
  and open the new kind. The focus runtime's two-verb workaround
  stays correct either way; the CLI `track start --kind respite`
  stops being a silent no-op. A landed-behavior change in the
  respite family's scope.
- **Acceptance**: the four combinations pinned (none open, same-kind
  repeat, different-kind switch closes-and-opens, other-task open);
  the focus runtime's journeys green (the workaround remains valid —
  no forced change there); undo classification of the new close+open
  path follows the existing tracking-arm rules (ADR 0017/0019
  exclusions stand); CHANGELOG entry (behavior change).

**Prompt 2.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.1 of backlog round 3 ("kind-aware StartTracking no-op", branch feat/start-tracking-kind-aware-noop) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify chunk 1.2's PR (feat/break-rule-roaming) merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #599 (normative — the planner arm, the two observed consequences, the proposal), the pomodoro plan §6 D-F2 (.artifacts/plans/2026-10-31-pomodoro-focus-plan.md), and the respite plan's D-3 (.artifacts/plans/2026-10-31-respite-break-reminders-plan.md).
2. Read the seams: the StartTracking planner arm in crates/taskboard-domain/src/command.rs (the open.len()==1 no-op), the tracking entry open/close machinery, the focus runtime's StopTracking→StartTracking workaround (what breaks if anything — nothing should), the CLI track start dispatch, and ADR 0017/0019's tracking-arm undo exclusions (what the close+open path classifies as).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b feat/start-tracking-kind-aware-noop ../taskboard-WT-feat-start-tracking-kind-aware-noop off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-start-tracking-kind-aware-plan.md: the planner arm's new shape (comparison on task+kind; the close+open as one verb application or an internal sequence — one call, recorded, with the journal/undo consequences spelled out), the pin catalogue (the four combinations + the respite-while-working journey + the focus hand-off unchanged), and the CHANGELOG wording (the silent no-op becomes a switch — a user-visible fix). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the kind-aware tracking no-op. No implementation, no push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.1 of backlog round 3 ("kind-aware StartTracking no-op").

1. Read AGENTS.md, issue #599, the dispatch plan §1/§3 chunk 2.1, and the stage-1 plan in ../taskboard-WT-feat-start-tracking-kind-aware-noop (branch feat/start-tracking-kind-aware-noop). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: the four-combination pins first, then the planner change. Hard rules: the focus runtime is untouched (its workaround stays correct — if a journey breaks, that is a deviation to report, not a workaround to delete); undo exclusions unchanged (Start/StopTracking stay NotUndoable per ADR 0017 — the internal close+open follows the same classification the plan recorded); no text assertions.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entry. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 2.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.1 of backlog round 3 (branch feat/start-tracking-kind-aware-noop).

1. Read AGENTS.md, issue #599, the dispatch plan §1/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: all four open-entry combinations are pinned and the pins discriminate (a pin that passes under the old kind-blind behavior is not a pin — mentally apply the old arm and check); the respite CLI journey now opens the break while Work runs; the undo journal entries for the switch path match the plan's recorded classification; no tracking-report/format drift.
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled feat(domain): the tracking no-op knows its kind, checks green, mergeStateStatus CLEAN). Close #599 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 2.2 — dead-board drain (#282) (`fix/dead-board-drain`)

- **Covers** (normative): #282 — wire-verified premise (chunk 1.1's
  rig probe: writes on a confirmed-dead board answer 403 ⇒ Forbidden
  classifies retryable; only `BadRequest` dead-letters; queued
  stack/label ops retry forever). Fix direction per the issue and the
  recorded evidence: symmetric `cascade_board` cancellation of
  stack/label ops (one outbox pass) or the write-side dead-board
  carve-out in the executor classification — stage 1 picks one with
  reasons. **Non-goals**: no carve-out changes for live boards; no
  Deck-version-specific behavior.
- **Acceptance**: ops queued on a board that dies stop retrying and
  finalize (typed, pinned at the engine/pipeline tier with the
  recorded 403 shape as fixture input); live-board write retries
  (e.g. transient 403 on a live board) still retry — the carve-out
  keys on confirmed-dead board state, not on the 403 alone; the rig
  probe's evidence is cited, not re-derived unless stage 1 wants
  fresh confirmation.

**Prompt 2.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.2 of backlog round 3 ("dead-board drain", branch fix/dead-board-drain) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify chunk 2.1's PR merged into origin/main (and 1.2's before it). If not: STOP and report.

1. Read AGENTS.md, GitHub issue #282 (normative — F10-05, the proposed approaches) and its rig-evidence comment (the measured 403 write answers, the classify_push consequence — chunk 1.1's rider, PR #562).
2. Read the seams: crates/taskboard-domain/src/pipeline.rs (cascade_board at :571/:1999 — what it cancels today, the confirmed-dead board state it keys on), the sync executor's classify_push table (where Forbidden lands retryable, BadRequest dead-letters), the outbox op kinds (task/stack/label arms), and the existing cascade/dead-board tests.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b fix/dead-board-drain ../taskboard-WT-fix-dead-board-drain off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-dead-board-drain-plan.md: the fix-shape decision (symmetric cascade cancel vs executor write-side carve-out — one call with reasons: which is testable at which tier, which avoids classifying on HTTP codes in domain, which respects the existing read-carve-out precedent), the liveness contract statement (a confirmed-dead board's queued ops finalize within one cycle), the pin catalogue (dead-board stack/label ops finalize; live-board transient 403 still retries — the anti-overreach pin), and whether the tier-2 rig re-verification is in scope (default: no — the 403 evidence is recorded; decide and record). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the dead-board drain. No implementation, no push.

Report: plan path, the fix-shape decision, decisions, risks, estimated diff size.
```

**Prompt 2.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.2 of backlog round 3 ("dead-board drain").

1. Read AGENTS.md, issue #282 (+ its evidence comment), the dispatch plan §1/§3 chunk 2.2, and the stage-1 plan in ../taskboard-WT-fix-dead-board-drain (branch fix/dead-board-drain). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: the dead-board finalization pin and the live-board-still-retries anti-overreach pin first, then the fix. Hard rules: the carve-out/cascade keys on confirmed-dead board state, never on the 403 alone; domain stays pure (no HTTP-code knowledge enters taskboard-domain — if the plan chose the executor route, the classification lives in sync); no changes to live-board retry semantics.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entry (reliability fix). No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 2.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.2 of backlog round 3 (branch fix/dead-board-drain).

1. Read AGENTS.md, issue #282, the dispatch plan §1/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: the anti-overreach pin is the trap — a fix that dead-letters any Forbidden strand (live-board transients included) is a regression dressed as a fix; the liveness pin asserts finalization within one cycle (typed terminal state, not absence-of-retry-observation); domain purity holds (grep the domain diff for error-code knowledge); the read-path carve-out is untouched.
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled fix(sync): the dead-board drain cancels queued stack and label ops, checks green, mergeStateStatus CLEAN). Close #282 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 2.3 — habit UX trio (#506) (`feat/habit-ux-trio`)

- **Covers** (normative): #506 — the DR-6 D-C10/D-C12 follow-ups:
  (i) one-click series↔habit conversion verb on the `continued_from`
  seam (create-new + link + stop-old in one guided flow); (ii) series
  `paused_until` (vacation pause without absence=missed); (iii)
  streak-repair affordances over the existing window-attribution
  functions (no new semantics under the hood). Bundle or split at
  dispatch — the plan decides with reasons. UI surfaces in the
  today/habit territory plus domain verbs.
- **Acceptance**: each of the three has a verb + pin + UI affordance
  with the r4 blanket acceptance rule (hover tip, tab-walk, action
  layer dispatch); conversion round-trips both directions with
  history preserved on the seam; `paused_until` stops miss-attribution
  during the window and resumes cleanly (pinned at the attribution
  layer); streak repair changes only affordance, not attribution
  math; CHANGELOG entries.

**Prompt 2.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.3 of backlog round 3 ("habit UX trio", branch feat/habit-ux-trio) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify chunk 2.2's PR merged into origin/main. Also check the round-4 dispatch state (chunks 9–11): if a round-4 chunk with today-view territory is unmerged, record the rebase-friction expectation (the ui territory rule) — do not block on it unless files literally collide.

1. Read AGENTS.md, GitHub issue #506 (normative — the three items, their DR-6 rulings D-C10/D-C12), and the DR-6 plan §3.7/§3.8/§4.7 (.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md).
2. Read the seams: the continued_from seam and its read-side history union (domain habits), the series/recurrence model (absence=missed default), the window-attribution/streak functions (the research-aligned surfaces ride them), the habit UI surfaces in crates/taskboard-ui-slint (today view's habit rows, the habit commands in taskboard-app), and the r4 blanket acceptance rule (round-4 dispatch plan §1).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b feat/habit-ux-trio ../taskboard-WT-feat-habit-ux-trio off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-habit-ux-trio-plan.md: the bundle/split decision (three verbs in one chunk vs ordered commits — one call with reasons); per item — the verb shape (domain command + state application + undo classification per the v1/v2 catalogue rules), the pin catalogue, and the UI affordance (which surface, the blanket rule's three requirements); the paused_until semantics (who sets it, what the attribution layer does inside the window, resume behavior — no miss, no credit, or the plan's recorded choice with reasons); the streak-repair scope fence (affordances only — attribution functions untouched). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the habit UX trio. No implementation, no push.

Report: plan path, the bundle/split decision, decisions, risks, estimated diff size.
```

**Prompt 2.3.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.3 of backlog round 3 ("habit UX trio").

1. Read AGENTS.md, issue #506, the dispatch plan §1/§3 chunk 2.3, and the stage-1 plan in ../taskboard-WT-feat-habit-ux-trio (branch feat/habit-ux-trio). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: domain verbs + pins first, then the UI affordances (blanket rule: hover tip, tab-walk, action-layer dispatch for every new interactive element). Hard rules: the attribution/window functions are untouched by item (iii) — an attribution change there is a deviation to report, not make; undo classifications follow the existing catalogue table's rules; new user-visible strings are presentation constants; no text assertions; time-independent tests (the fake clock seams).
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entries. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 2.3.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.3 of backlog round 3 (branch feat/habit-ux-trio).

1. Read AGENTS.md, issue #506, the dispatch plan §1/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: conversion preserves the seam's history in BOTH directions (round-trip pin exists and is load-bearing); paused_until does not silently rewrite history outside its window (the attribution pins for pre-pause and post-pause periods unchanged); item (iii) added no attribution semantics (diff the attribution files — must be empty); every new interactive element meets the blanket rule (spot-check each in the journeys); the UI dispatches through the action layer (no inline side effects).
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled feat(habits): the convert verb, series pause, and streak repair, checks green, mergeStateStatus CLEAN). Close #506 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 4. Batch 3 — task-view tooltips (gated on round-4 chunk 10)

### Chunk 3.1 — task-view field tooltips (#590) (`feat/taskview-field-tooltips`)

- **Covers** (normative): #590 — the r4 D21 remainder: the task view's
  text-field sites (title/description editors, due/scheduled editors,
  custom reminder offset) are tip-less; the planned TextField tip arm
  proved infeasible on the pinned runtime (inner `TextInput` accepts
  its own hover moves). Likely mechanism: a labelled-field wrapper in
  the settings `LabeledField` shape. Origin report
  `.artifacts/reports/2026-11-03-ui-r4-taskview-focus-stage3-report.md`
  §5.
- **Acceptance**: every field site has a label + description tip via
  the wrapper (journeys/goldens updated); caret handling unbroken
  (type/edit journeys green — the original infeasibility is not
  reintroduced); the wrapper generalizes rather than duplicating the
  settings shape where they are the same problem (stage 1 decides);
  blanket acceptance rule applies.

**Prompt 3.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.1 of backlog round 3 ("task-view field tooltips", branch feat/taskview-field-tooltips) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify round-4 chunk 10's PR (feat/ui-r4-tracking-surfaces) merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #590 (normative — the runtime limitation, the LabeledField direction), the origin report .artifacts/reports/2026-11-03-ui-r4-taskview-focus-stage3-report.md §5, and the r4 plan's D21 (.artifacts/plans/2026-11-01-gui-review-round4-plan.md).
2. Read the seams: the settings LabeledField wrapper (how settings fields get info tips), the task view's field sites (title/description editors, due/scheduled editors, the custom reminder offset — post-round-4 state), the tooltip/hint machinery (tip keys, hint bindings), and the caret-handling constraint that killed the original arm (inner TextInput hover).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b feat/taskview-field-tooltips ../taskboard-WT-feat-taskview-field-tooltips off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-taskview-field-tooltips-plan.md: the wrapper shape (shared with settings vs local — one call with reasons), the per-site inventory (site → label + description + hint keys), why the wrapper cannot break caret handling (the mechanism argument, grounded in the runtime constraint), and the test catalogue (journeys + regenerated goldens + the blanket rule). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the task-view field tooltips. No implementation, no push.

Report: plan path, the wrapper decision, decisions, risks, estimated diff size.
```

**Prompt 3.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.1 of backlog round 3 ("task-view field tooltips").

1. Read AGENTS.md, issue #590, the dispatch plan §1/§4 chunk 3.1, and the stage-1 plan in ../taskboard-WT-feat-taskview-field-tooltips (branch feat/taskview-field-tooltips). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: the wrapper + per-site tips; regenerate affected goldens. Hard rules: caret handling is the regression to guard — the existing type/edit journeys must pass unmodified; no tooltip machinery changes (chunk 4 of r4 landed the hover fixes — build on them, do not rework them); new strings are presentation constants; no text assertions.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entry. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 3.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.1 of backlog round 3 (branch feat/taskview-field-tooltips).

1. Read AGENTS.md, issue #590, the dispatch plan §1/§4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: every field site from the inventory is tipped (a site missed is the classic half-landing); the type/edit journeys are untouched and green (caret); the golden diffs are exactly the tip additions (any layout shift beyond the tips is a finding); no duplication of the settings shape if the plan ruled shared.
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled feat(ui-slint): the task-view fields carry tips, checks green, mergeStateStatus CLEAN). Close #590 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 5. Batch 4 — the ui tail (gated on round-4 chunk 11; 4.2 goes last)

### Chunk 4.1 — GUI settings editor (#351) (`feat/gui-settings-editor`)

- **Covers** (normative): #351 — a settings view reading the resolved
  `AppConfig` and writing changes through a typed config-write port
  that generalizes the logout write path (`AppConfig::write_path`,
  same fallback rules: existing CWD file or platform config dir).
  Figment precedence honored honestly: env-var overrides surfaced
  read-only with their source, or the editor edits the file layer and
  shows the effective resolved value (stage 1 picks). Secrets
  (`[nextcloud]` credentials) read-only or out of scope.
- **Acceptance**: view + edit round-trip for the non-secret sections
  (journeys); precedence honesty pinned (an env override is visible
  and read-only, or the layer model is displayed — per the stage-1
  ruling); the write port reuses the logout machinery's fallback
  rules (pinned); secrets never editable; blanket acceptance rule;
  CHANGELOG entry.

**Prompt 4.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4.1 of backlog round 3 ("GUI settings editor", branch feat/gui-settings-editor) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify round-4 chunk 11's PR (feat/ui-r4-settings-connections) merged into origin/main (the settings surface territory must be quiet). If not: STOP and report.

1. Read AGENTS.md, GitHub issue #351 (normative — scope, precedence honesty, secrets rule), and the origin ruling (.artifacts/plans/2026-10-18-field-family-design-plan.md §3.6 D-F6).
2. Read the seams: the config machinery (crates/taskboard-app/src/config.rs — figment assembly, the [ui]/[reminders]/[sync]/[preferences] sections), the logout write path (AppConfig::write_path — the precedent to generalize), the settings view after r4 chunk 11 (its layout, the scrap-scale preview, the connections dropdown — the editor joins this surface), the UI's action-layer dispatch pattern, and how the daemon/UI would apply changed settings (restart-required vs live — inventory per section honestly).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b feat/gui-settings-editor ../taskboard-WT-feat-gui-settings-editor off main.
4. Write the TDD-first plan at .artifacts/plans/2026-11-03-gui-settings-editor-plan.md: the precedence-honesty model (one call: read-only env surfacing vs file-layer editing with resolved display — with reasons), the typed config-write port shape (generalizing write_path; what writes mean for a running daemon — per-section restart semantics stated honestly, no pretend-live application), the section inventory (editable now / read-only / hidden, secrets rule), the secrets boundary (typed enforcement if feasible, not just UI omission), and the test catalogue (journeys + write-port pins + precedence pins). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the GUI settings editor. No implementation, no push.

Report: plan path, the precedence decision, decisions, risks, estimated diff size.
```

**Prompt 4.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4.1 of backlog round 3 ("GUI settings editor").

1. Read AGENTS.md, issue #351, the dispatch plan §1/§5 chunk 4.1, and the stage-1 plan in ../taskboard-WT-feat-gui-settings-editor (branch feat/gui-settings-editor). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: the write port + precedence pins first, then the view. Hard rules: the UI stays a dumb presentation client (AGENTS §2) — writes dispatch through the action layer to the app side; secrets never editable (enforce per the plan, not by hiding a widget); no silent partial application of settings (restart-required is stated, not faked); blanket acceptance rule on every new interactive element; no text assertions; time-independent tests.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh; CHANGELOG Unreleased entry. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 4.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 4.1 of backlog round 3 (branch feat/gui-settings-editor).

1. Read AGENTS.md, issue #351, the dispatch plan §1/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: the write path cannot corrupt a config it did not fully parse (round-trip pin over a file with comments/unknown keys — the plan's disposition on unknown-key preservation is honored); precedence is pinned at the figment boundary, not re-derived in the UI; the secrets boundary holds under keyboard navigation (tab-walk cannot reach an editable secret field); restart-required semantics match the daemon's actual behavior exactly (nothing promises live application that does not happen).
3. Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled feat(ui-slint): the settings editor over the typed config port, checks green, mergeStateStatus CLEAN). Close #351 with the evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 4.2 — install_seam! macro (#595) (`refactor/install-seam-macro`)

- **Covers** (normative): #595 — F7-08's trigger fired ~9× over: 110
  `window.on_*` installs across 26 `install_*` fns in
  `crates/taskboard-ui-slint/src/adapter.rs`. Stage 1 re-derives the
  call-shape catalogue (route-action, raw-send, settings-scale-verb,
  due-edit, multi-Rc/weak) and re-adjudicates the standing rationale
  at `adapter.rs:4288–4290` ("over generics' worth of closure types")
  with 110 sites on the table — the macro must win on evidence, not
  on the audit's aging premise. Mechanical refactor; the full
  journey/golden suite is the safety net. **Goes last of the ui
  chunks** (after 3.1 and 4.1 land; `adapter.rs` must be quiet).
- **Acceptance**: either the conversion lands (all installs via the
  macro or the recorded mixed verdict, zero behavior change,
  journeys/goldens byte-identical) **or** stage 1's re-adjudication
  concludes the macro loses and the issue closes with the evidence —
  both outcomes are a completed chunk; the decision is recorded on
  the issue.

**Prompt 4.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4.2 of backlog round 3 ("install_seam! macro", branch refactor/install-seam-macro) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune; verify round-4 chunk 11's AND chunk 4.1's PRs merged into origin/main (adapter.rs and the settings surface must be quiet — this chunk rewrites adapter.rs wholesale). If not: STOP and report.

1. Read AGENTS.md, GitHub issue #595 (normative), the audit finding F7-08 (.artifacts/reports/2026-10-16-v0.1.0-audit-crate-ui-slint-report.md §8 — the "three concrete call-shapes" premise and the 13th-seam trigger), and the #275 close comment's summary (the 110-install count, the deferral).
2. Read the seams: crates/taskboard-ui-slint/src/adapter.rs end to end — the 26 install_* fns, the actual call-shape variety (route-action, raw-send, settings-scale-verb, due-edit, multi-Rc/weak — count each), the standing generics rationale at :4288–4290, and what the journey/golden suite observes about these installs.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = STOP and report), then: git worktree add -b refactor/install-seam-macro ../taskboard-WT-refactor-install-seam-macro off main.
4. Write the plan at .artifacts/plans/2026-11-03-install-seam-macro-plan.md: the re-derived call-shape catalogue with counts; the adjudication (macro vs status quo — the honest comparison: boilerplate saved vs macro complexity vs type-inference costs vs the closure-variety argument; ONE recommendation, and the do-nothing outcome is legitimate if the evidence says so); if converting — the macro design (arm per call shape, hygiene rules), the migration order over the 26 fns, and the zero-behavior-change protocol (journeys/goldens byte-identical, no install silently dropped — a dropped install is the catastrophic failure mode; plan how the net catches it, e.g. a count assertion or per-fn inventory check). Resolve every open question with a recorded decision.
5. Commit: docs(plan): the install_seam! adjudication and conversion. No implementation, no push.

Report: plan path, the adjudication recommendation, decisions, risks, estimated diff size.
```

**Prompt 4.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4.2 of backlog round 3 ("install_seam! macro").

1. Read AGENTS.md, issue #595, the dispatch plan §1/§5 chunk 4.2, and the stage-1 plan in ../taskboard-WT-refactor-install-seam-macro (branch refactor/install-seam-macro). Recreate the worktree on that branch if missing.
2. If the plan adjudicated do-nothing: write the evidence record as the plan's artefact (no production diff), run the gates, and report — stage 3 ships the docs-only PR. Otherwise implement per the plan: macro first with two pilot fns converted (journeys green), then the migration in the plan's order, commit per plan stage. Hard rules: zero behavior change — journeys/goldens byte-identical throughout; no install dropped (the plan's net — count assertion or inventory check — runs in CI-visible form); no macro arm added for a shape with one occupant unless the plan recorded why.
3. Run the full AGENTS §10 gate suite via scripts/run-quality-gates.sh. No CHANGELOG (no user-observable change). No push, no PR. Never push to or merge main.

Report: per-gate results, the conversion inventory (fn count, install count before/after), deviations.
```

**Prompt 4.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 4.2 of backlog round 3 (branch refactor/install-seam-macro).

1. Read AGENTS.md, issue #595, the dispatch plan §1/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional focus: the install count is conserved (110 before = 110 after, or the recorded mixed verdict accounts for every delta); a dropped install would be invisible in review and catastrophic in use — verify the net actually exists and fails when you temporarily delete one install (do it, observe, restore, record); the macro arms match the re-derived catalogue (no speculative arms); the adjudication evidence (if do-nothing) would survive re-derivation.
3. Re-run the full gate suite AND the journey/golden suites explicitly; any failure or golden drift is a finding. Fix findings on the branch.
4. Ship per §1 step 4 (PR titled refactor(ui-slint): the install_seam! macro over the adapter installs — or docs(...): the install_seam! adjudication, per the outcome; checks green, mergeStateStatus CLEAN). Close #595 with the outcome evidence + PR link. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 6. Finale — register verification pass (#274, #277, #284, #285, #286; no branch)

**Prompt F.1 (bookkeeping):**

```text
You are the BOOKKEEPING agent for the backlog round 3 finale (the audit accepted-risk registers) in /home/viktoria/src/taskboard. No branch, no code — tracker comments only.

1. Read AGENTS.md §7, the triage report .artifacts/reports/2026-11-03-backlog-round3-readiness-triage-report.md §5 (the register verdict), and the five register issues #274, #277, #284, #285, #286 with their originating audit reports.
2. For each register, verify its recorded triggers against what has landed since 2026-10-17: #274 — did the sweep-scope F1-03 fix land (the ignore-list rider)? #277 — the kiosk wave landed (the Kiosk-mode verdict), round-2/r4 churned the app crate (the split-later triggers: any second frontend/consumer appeared?). #285 — did the kiosk soak/census or a release asset ever materialize (F13-02/06/09)? #286 and #284 — any consumed verdicts (e.g. the design rounds that were to consume them have run).
3. Action per register: post a dated verification comment updating fired/moved verdicts with evidence (commits/issues); close the register ONLY if every recorded trigger is consumed or migrated to a fresher home (say where each went); otherwise it stays open as the living record. Do not delete history, do not reopen closed issues.
4. Report: per-register verdict (updated / unchanged / closed) with the evidence lines.
```

## 7. Post-round checklist (the drain-goal audit)

When every chunk above plus round-4 chunks 9–11 have merged, the open
backlog set must be exactly: the watchlist-by-policy entries (#287,
#336, #384, and #524 if it stayed open), the surviving registers, the
12 external/deferred, the 29 trigger-gated, and the 12 design/usage-
gated issues — each with its triage-report §5 reason line. Any issue
outside that set is either tackleable (a round-3 failure to fix) or
lacks a recorded reason (a bookkeeping gap) — fix or record before
starting the v0.2 "focus & reflection" wave planning (#504, #592,
#600 are already assigned to `v0.2`).
