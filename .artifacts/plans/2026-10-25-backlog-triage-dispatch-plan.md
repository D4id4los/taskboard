# [Plan] Backlog Triage Round — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-25
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation | execution → review) for the **12** chunks of the
  2026-10-25 backlog triage round: the maintenance, fix, tooling and
  docs backlog identified in a full survey of all open issues while
  the daily-driver wave closes and GUI review round 2 finishes chunks
  6–8. No code in this commit.
- **Origin**: the 2026-10-25 backlog-triage session (user-accepted
  lane assignment). Per-chunk normative origins are the GitHub issues
  themselves — each body carries its findings, proposed approach, and
  originating-report permalink per AGENTS §7 — plus the reports they
  cite: chiefly the v0.1.0 audit report set
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-findings-report.md`
  and the per-theme/per-crate reports), the daily-driver wave exit
  report (`.artifacts/reports/2026-10-25-daily-driver-wave-exit-report.md`),
  and the type-driven suitability report
  (`.artifacts/reports/2026-10-24-type-driven-formal-methods-suitability-report.md`,
  on branch `docs/type-driven-suitability-and-guidelines` until its PR
  lands). Plus the user's 2026-10-25 instruction to fold in the #390
  true-partial re-measure (whose fresh numbers re-justify #373's stale
  description) and the #311 clean-up (already executed outside this
  dispatch — see §15).
- **Status**: READY — wave 1 (chunks 1, 2, 3, 5, 6, 7 and, at the
  dispatcher's discretion, 8) is dispatchable immediately, in any
  order or in parallel: none modifies `crates/taskboard-ui-slint`, and
  r2 chunks 6–8 (in flight, e.g. `feat/ui-r2-connection-management`)
  own that crate. Wave 2 (chunks 9–11) starts once the wave-1 chunks
  sharing its crates have merged. Chunk 4 and the flaky pass (chunk
  12) are gated on the r2 round merging. Deliberately **not**
  dispatched here: the queued design-round families (§14), the
  trigger-gated perf/register/migration-locked issues, release/CI
  infrastructure, and roadmap waves (§15).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | #386 | `fix/config-write-preserves-mode` | `connect`'s config rewrite stops tightening a non-default-path config below 0644; unreadable config surfaces as a typed error, not `decode` — `taskboard-app` only | S |
| 2 | #395 | `fix/worktree-doctor-sweep` | doctor enumerates registered worktrees instead of globbing; floor check always runs; nested-layout ruling recorded — `scripts/` (+ one AGENTS §8 sentence) only | S |
| 3 | #434 + #435 (+ triage #367) | `test/tier2-convergence-fixtures` | `boot_convergence` awaits the first pull; capabilities pin becomes min-version; C-series re-run both tiers; #367 stamp collision triaged in the same harness pass — `taskboard-sync-nextcloud` tests only | S |
| 4 | #425 | `refactor/a1-gap-plug-newtypes` | `ConnectorId` fallible mint (kebab + uniqueness), `UiScale(f64)` minted at config validation and carried in `UiSettings`; A1 house pattern, no derive crates — **gated on r2 chunks 5–8 merging** | M |
| 5 | #267 + #268 + #269 + #278 + #281 + #437 + #283 docs leg | `docs/audit-drift-pass` | one bundled docs pass over architecture.org, testing_strategy.org, coding_guidelines.org, ADRs 0003/0004/0007-note/0010/0011, CHANGELOG, README, `--due` help — docs only, zero code | M |
| 6 | #426 | `test/kani-pilot` | ≥3 proven Kani harnesses over the pure domain core + weekly `kani.yml` mirroring `miri.yml`; honest-termination exit is acceptable | M |
| 7 | #390 + #373 body | `docs/bench-true-partial-remeasure` | re-run `benches/refresh_draw.rs` against current main (r2 D9 row-diff push landed); true-partial verdict report; #390 disposition + #373 body updated from fresh numbers — execution chunk, diff = report artefact only | S |
| 8 | #427 | `ci/mutants-ui-pure-modules` | weekly mutation-testing target list gains the UI pure modules; survivor report triaged; documented revert path — workflow file + report only | S |
| 9 | #283 code legs | `fix/time-dst-small-fixes` | `ReminderOffset` whole-second by construction; `sync` freshness detected structurally, not by wall-stamp comparison | S |
| 10 | #245 | `test/snapshot-published-narrowing` | 14 state snapshots → 7 persisted dumps + structural equality assertion + small tail snapshots | S |
| 11 | #253 + #254 | `refactor/app-audit-p2` | `session_ceremonies` rename/move; diverged `wait_until`/`Child`/daemon-spawn helpers hoisted to `tests/common/mod.rs` — serial within the branch | S |
| 12 | #384 + #408 + #417 + #431 (+ triage #287, #336) | `test/flaky-watchlist-pass` | the dedicated flaky pass AGENTS §7 mandates: triage every watchlist entry, fix where root-caused, annotate the rest — **#431 leg gated on r2 merging** | M |

**Dependency graph**:

- **Wave 1 = chunks 1, 2, 3, 5, 6, 7** — mutually independent (disjoint
  files, all branched off current `main`, no ui-slint changes; chunk 7
  only *runs* the ui-slint bench, never edits it). Dispatchable in
  parallel as separate sessions. **Chunk 8** is also wave-1-eligible
  (a workflow-file edit is inert for PRs), but its survivor-triage
  report is most meaningful after r2 merges — `view_state.rs` is r2's
  most-churned file; the dispatcher chooses the slot.
- **Chunk 3 enables a user-gated manual step, not a chunk**: after it
  merges, dispatch the weekly Nextcloud Integration lane on the
  daily-driver wave code — `gh workflow run nextcloud-integration.yml`
  (issue #436) — and triage its output against chunk 3's outcomes.
  Ruling recorded in #436: lane dispatch is the user's action, never
  autonomous.
- **Wave 2 = chunks 9, 10, 11** — order-independent of each other
  (disjoint files) with one caveat: 9 and 11 both touch
  `taskboard-app`, so concurrent dispatch inherits the AGENTS §8
  rebase dance; the default is serial 9 → 11. Wave 2 starts after the
  wave-1 chunks sharing its crates (1, 5) have merged.
- **Gated on GUI r2 chunks 6–8 merging**: chunk 4 (its `UiScale` leg
  changes `UiSettings`, whose scale consumer r2 chunk 5's settings
  form reworks; the `ConnectorId` leg alone has no ui-slint surface —
  splitting it early is a user decision, the default is one PR per
  the pilot's success criteria), chunk 12's #431 leg (ui-slint test
  support), and the #373 implementation decision (chunk 7's evidence
  plus user acceptance come first).
- **Freeze rule binding every chunk**: no changes under
  `crates/taskboard-ui-slint/` and no `.slint` edits until r2 chunks
  6–8 are merged — the only exceptions are chunk 8's workflow-file
  edit and chunk 7's read-only bench run. A chunk discovering it
  needs a ui-slint change STOPs and reports instead of expanding.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` (slashes as dashes)
next to the project root (AGENTS §8), branched off the then-current
`main`, no PR ever stacked. **Branch names are never reused after
their PR lands** (AGENTS §8, issue #415): a resume/second round works
on `<branch>-v2` (or re-branches off the fetched `origin/main`),
never on the landed branch. Create worktrees with `git worktree add
-b <branch> ../taskboard-WT-<branch>` (never without `-b`, never on
`main`). Disk hygiene (AGENTS §8): any agent creating or recreating a
worktree runs `scripts/worktree-doctor.sh` from the main checkout
first — nonzero exit (free disk below the floor) stops the chunk;
stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: runs the doctor, creates the
   worktree `../taskboard-WT-<branch>` on a new branch off the
   current `main`, writes the plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style — resolving every open question the chunk block names with a
   recorded decision (deviations from the origin need a recorded
   reason) — and commits it (`docs(plan): …`). No implementation, no
   push.
2. **Stage 2 — implementation | execution agent**: continues in that
   worktree (recreates it on the branch if missing; if the branch or
   worktree path's PR already landed, use `<branch>-v2` per AGENTS
   §8), executes per the approved plan — implementation chunks: TDD,
   every commit standalone (rebase-merge hygiene), full AGENTS §10
   gate suite green; execution chunks: the experiments and the
   deliverable artefact. Updates `CHANGELOG.md`'s Unreleased section
   only where the chunk changes user-observable behavior (chunks 1
   and 9; no other chunk touches it). Does **not** push or open a
   PR. Reports deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase the branch onto `origin/main` (AGENTS §8's rebase dance:
   drop already-merged duplicate commits; resolve only real
   conflicts), push `--force-with-lease` if history was rewritten —
   then reviews the entire branch diff versus the **post-rebase**
   base against the chunk's acceptance criteria + AGENTS rules plus
   the mandate below, re-runs whatever gates apply, implements fixes
   for every finding on the branch (amend/reword, never
   duplicate-subject fixups). Its last tasks are shipping: push the
   branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green — then verify the PR's merge state: `gh pr view --json
   mergeStateStatus` must report `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase
   onto `origin/main`, push `--force-with-lease`, let the checks
   re-run, repeat; any other state ⇒ diagnose before handing off.
   **Then teardown**: remove the now-disposable worktree — from the
   project root, `git -C /home/viktoria/src/taskboard worktree remove
   --force ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8 —
   never a bare relative path; confirm via `git worktree list` that
   the removed path was the intended one). The handoff contract is
   "PR open + checks green + merge state `CLEAN` + worktree removed".
   Reports a verdict. Never merges; never touches `main` (`main` is
   branch-protected — merges happen on GitHub by the user).

**Code-quality mandate** (stage 3, every implementation chunk — 1, 2,
3, 4, 6, 8, 9, 10, 11, 12): beyond functional review, evaluate (a)
code quality: naming, comment discipline (comments state constraints,
not narration), idiomatic use of the approved stack, no dead code or
speculative generality, test quality per AGENTS §5 (no text
assertions, no sleeps, deterministic by construction, no quality
ceilings, a failing new test may be a production bug); (b) the Single
Responsibility Principle: the crate-role boundaries of AGENTS §2 are
the SRP line — a boundary violation is an SRP violation; (c) a single
level of abstraction per function body: a function either
orchestrates named operations or works in primitive data, never
mixes. Findings are fixed on the branch like any other finding.

**Evidence & artefact mandate** (stage 3, the docs/execution chunks —
5 and 7): every factual claim carries evidence (the command run +
observed outcome, or a verifiable source + what it states) and
experiments are reproducible from the report's method section;
recommendations are decision-grade (options, trade-offs, exactly
**one** recommendation, consequences for later work); repo hygiene
(scratch code and captured payloads never land — evidence is excerpts
inside the report; the PR diff is exactly the chunk's artefacts);
scope discipline (out-of-scope ideas go to the backlog per its
template, not into the report).

## 2. Chunk 1 — `connect` config-write permissions (#386) (`fix/config-write-preserves-mode`)

- **Covers**: issue #386 (normative: symptom, repro, root cause,
  expected). Files: `crates/taskboard-app/src/commands/connect.rs`
  (rewrite path; note its tests at ~:656–698 currently **pin the
  bug** — "the written config must be 0600", "the rewrite must
  tighten to 0600"), `crates/taskboard-app/src/config.rs` write path
  (~:1183–1200, `options.mode(0o600)`), and the config-load error
  mapping that reports an EACCES read as `kind="decode"`. Contrast
  precedent: `crates/taskboard-app/src/commands/logout.rs:224-235`
  *loosens* to 0644 after its rewrite — the two paths already
  disagree. **Non-goals**: no config schema or figment changes; no
  secret/keyring handling changes; no runbook rewrite (a one-sentence
  docs touch, if the chosen semantics need stating, coordinates with
  chunk 5 when both are in flight — default: none).
- **Decisions stage 1 settles with recorded reasons**: (a) semantics —
  preserve the existing file's mode on rewrite vs. never tighten
  below 0644 outside the default XDG location vs. always 0644 for
  system paths; the issue accepts any of the three — recommend one
  (preserving the existing mode is the least surprising and matches
  the logout precedent). (b) The typed error for an unreadable config
  (distinct `ConfigError` kind or payload — the `decode` mislabel is
  the bug's second half). (c) What happens to the XDG-default
  single-user case (0600 there is defensible; decide, record, and pin
  whichever contract is chosen). (d) Rewrite of the bug-pinning tests
  to pin the new contract — this is *replacing a pin of the bug*, not
  weakening a correct test (AGENTS §5).
- **Acceptance**: `connect` on a 0644 config outside the default XDG
  location leaves it readable by the owning non-root user (the kiosk
  `taskboard` uid from the issue's repro); an unreadable config
  surfaces as a typed config error (assert `matches!(err, …)` — no
  text assertions); the chosen XDG-default contract pinned; the
  systemd restart-loop scenario from the issue can no longer be
  reproduced by the test suite's equivalents; gate suite green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the backlog-triage dispatch ("connect config-write permissions", branch fix/config-write-preserves-mode) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, then GitHub issue #386 (normative) and its cited report .artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md (finding F7).
2. Read the seams: crates/taskboard-app/src/commands/connect.rs (rewrite path and its two mode-pinning tests), crates/taskboard-app/src/config.rs (~:1183-1200 write path, and the load-error mapping that yields kind="decode" on EACCES), crates/taskboard-app/src/commands/logout.rs:224-235 (the loosen precedent). Note the contrib/taskboard-kiosk.service + docs/kiosk_bringup_runbook.org §6 deployment shape the bug breaks.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below disk floor: STOP and report), then: git worktree add -b fix/config-write-preserves-mode ../taskboard-WT-fix-config-write-preserves-mode off main (never without -b, never on main — AGENTS §8).
4. Write the TDD-first plan at .artifacts/plans/2026-10-25-connect-config-permissions-plan.md. Settle with recorded decisions: the mode semantics (preserve vs. floor-0644-vs-XDG vs. always-0644 — recommend one), the typed unreadable-config error shape and where EACCES maps today, the XDG-default contract, and the rewritten test catalogue (each new pin named). No implementation, no push.
5. Commit: docs(plan): connect config-write permission contract. 

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the backlog-triage dispatch ("connect config-write permissions").

1. Read AGENTS.md, issue #386, the dispatch plan .artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md §1/§2, and the stage-1 plan in ../taskboard-WT-fix-config-write-preserves-mode (branch fix/config-write-preserves-mode). Recreate the worktree on that branch if missing.
2. Implement per the approved plan, TDD: first the failing test that reproduces the kiosk scenario's equivalent (non-root-readable config at a non-default path survives `connect`), then the fix. Replace the two bug-pinning tests with pins of the new contract. Typed error: assert with matches!(err, …) — never error text (AGENTS §5). Every commit standalone.
3. Full gate suite (AGENTS §10) green: fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov three-command sequence + quarantine + rm.
4. CHANGELOG.md Unreleased: one bug-fix entry (user-observable behavior change).
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the backlog-triage dispatch (branch fix/config-write-preserves-mode).

1. Read AGENTS.md, issue #386, the dispatch plan §1/§2 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-fix-config-write-preserves-mode after rebasing onto origin/main per §1 shared model step 3.
2. Functional traps: the rewrite path must not regress the credential-persistence semantics connect exists for (server_url/username/[sync] backend flip); the XDG-default contract must be pinned exactly as stage 1 decided (no silent scope creep to "all configs 0644"); the typed error must not collapse unrelated ConfigError kinds; the rewritten tests must pin the new contract, not merely stop asserting (a missing pin is a finding).
3. Code-quality mandate: spell out §1's code-quality mandate in full and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open the PR to main with gh, gh pr checks --watch until green, mergeStateStatus CLEAN. Then teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-config-write-preserves-mode; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 3. Chunk 2 — worktree-doctor sweep robustness (#395) (`fix/worktree-doctor-sweep`)

- **Covers**: issue #395 (normative: the exit-128 trace, the flat-glob
  mechanism, both fix directions, the nested-layout question). Files:
  `scripts/worktree-doctor.sh` only, plus — if the ruling requires —
  one sentence in AGENTS §8 recording the sanctioned worktree naming
  shape (that docs edit is this issue's own ask, not scope creep).
  **Non-goals**: no other script changes; no changes to the doctor's
  exit contract (nonzero = free disk below the floor, and *only*
  that); no changes to the dirty/unmerged/on-main protections.
- **Decisions stage 1 settles**: (a) `git worktree list --porcelain`
  as the enumeration source of truth vs. tolerating per-entry `git`
  failure inside the glob sweep — recommend porcelain (it also makes
  the invisible nested `taskboard-WT-<parent>/<child>` layout
  visible); (b) the nested-layout ruling: scan one level deeper vs.
  declare nesting a naming deviation and record the flat
  `<root>-WT-<branch-with-dashes>` convention in AGENTS §8 — recommend
  the latter (porcelain enumeration makes the question moot for
  safety; the AGENTS sentence keeps naming honest).
- **Acceptance**: with a plain non-git `taskboard-WT-*` sibling
  directory present, the doctor still reaches the floor check and
  exits 0 above the floor (new test or scripted check recorded in the
  PR); the KEEP/REMOVE protections (unmerged HEAD, dirty tree, on
  `main`) behave exactly as before against a registered worktree;
  `shellcheck` (if configured) / `bash -n` clean.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the backlog-triage dispatch ("worktree-doctor sweep robustness", branch fix/worktree-doctor-sweep) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§7 flaky/backlog conventions and §8 worktree lifecycle — the doctor is §8's only sanctioned bulk-removal tool), then GitHub issue #395 (normative) and its cited report section (.artifacts/reports/2026-10-21-kiosk-milestone-remaining-steps-report.md §4).
2. Read scripts/worktree-doctor.sh end to end: the sweep loop, the double dirty-check, the unmerged-HEAD and on-main protections, the floor check, every exit path. Reproduce the #395 failure mode locally with a throwaway plain directory (mkdir ../taskboard-WT-bogus; run the doctor; observe exit 128; rm the directory) — evidence goes in the plan.
3. Run scripts/worktree-doctor.sh (expect exit 0 today), then: git worktree add -b fix/worktree-doctor-sweep ../taskboard-WT-fix-worktree-doctor-sweep off main.
4. Write the plan at .artifacts/plans/2026-10-25-worktree-doctor-sweep-plan.md: the enumeration mechanism (porcelain vs. tolerated-glob — recommend one with reasons), the nested-layout ruling and its AGENTS §8 sentence (verbatim draft), the reproduction/verification checklist (bogus-sibling case, registered-worktree protection cases, floor case), and the exact exit-code contract the script must preserve. No implementation, no push.
5. Commit: docs(plan): worktree-doctor sweep robustness.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the backlog-triage dispatch ("worktree-doctor sweep robustness").

1. Read AGENTS.md (§8 is the contract you are hardening), issue #395, the dispatch plan §1/§3, and the stage-1 plan in ../taskboard-WT-fix-worktree-doctor-sweep (branch fix/worktree-doctor-sweep). Recreate the worktree if missing.
2. Implement per the plan. Hard rules: `set -euo pipefail` stays; the floor check must be reachable on every path; a non-worktree directory must never abort the sweep; the KEEP protections run against each registered worktree exactly as before; no new dependencies beyond git core. Test the five cases from the plan's checklist for real (bogus sibling; dirty worktree; unmerged-HEAD worktree; worktree on main; below-floor via the script's own floor knob if it has one, else documented dry evidence) and paste the transcripts into the stage-2 report.
3. Gates: bash -n, shellcheck if available; the script has no cargo surface — the AGENTS §10 cargo gates apply only insofar as the tree must stay clean (git status short after every doctor run — the doctor must never dirty the tree).
4. No push, no PR. Never push to or merge main.

Report: case transcripts, any deviation from the plan and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the backlog-triage dispatch (branch fix/worktree-doctor-sweep).

1. Read AGENTS.md §8, issue #395, the dispatch plan §1/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §1 step 3.
2. Functional traps: exit-code contract drift is THE regression to hunt — a nonzero exit that means anything other than "below the floor" reintroduces #395 for every future agent; check the porcelain parse against `git worktree list --porcelain`'s actual format (blank-line-separated records, `worktree`/`HEAD`/`branch`/`bare`/`detached` keys); confirm a `bare`-adjacent or detached entry cannot crash the parse; confirm the AGENTS §8 edit (if any) states the naming convention without contradicting the active-worktree safety bullets.
3. Re-run the plan's five verification cases yourself; any divergence is a finding.
4. Fix findings on the branch (amend/reword). Ship per §1 step 3 (push, PR, checks green, mergeStateStatus CLEAN), then teardown the worktree with the absolute-anchor command and confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 4. Chunk 3 — tier-2 convergence fixtures (#434 + #435, triage #367) (`test/tier2-convergence-fixtures`)

- **Covers**: issues #434 (the `boot_convergence` scaffolding race —
  the unwrap of A's stacks map at `it_docker.rs:603` before the first
  pull completes; fix via the existing `eventually` helper per the
  issue), #435 (the capabilities fixture's literal `"1.19.0"` →
  parse-and-compare a minimum, so image refreshes survive), and the
  triage half of #367 (same file, same harness: the same-second stamp
  collision — suggested direction is a fixture-ordering guarantee,
  deterministic by construction, **no sleeps**). The C-series re-runs
  on both tiers (default image + `DECK_VERSION=v1.17.5`) with
  outcomes recorded. **Non-goals**: no production sync-code changes
  (the tier-1 wiremock f-series is green and stays untouched); no new
  fixtures; no CI workflow changes (the weekly lane dispatch is
  #436, user-gated, §15).
- **Chunk-specific note on gates**: the docker tier does not run per
  PR — stage 2/3 must bring the docker tier up locally (compose file
  in `docker/nextcloud-test/`) and record run transcripts; stage 3
  pastes the tier records into the PR description as the review
  evidence.
- **Acceptance**: C1–C4 pass on both tiers; the capabilities test
  passes on 1.19.x and demonstrably asserts a floor (a below-minimum
  decode fails it); #367 either fixed deterministically (stamp
  advance guaranteed by construction) or its watchlist issue carries
  the triage verdict with evidence; zero production-code diffs.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the backlog-triage dispatch ("tier-2 convergence fixtures", branch test/tier2-convergence-fixtures) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 testing directives — deterministic by construction, no sleeps, no text assertions), then issues #434, #435, #367 (normative) and .artifacts/reports/2026-10-25-daily-driver-wave-exit-report.md (findings F1, F2, F7).
2. Read the seams: crates/taskboard-sync-nextcloud/tests/it_docker.rs (boot_convergence ~:603, the C1-C4 fixtures, the capabilities fixture), tests/common/sync_pair.rs (the fire-and-forget SyncNow nudge), tests/common/mod.rs (~:560-580 await_card_stamp_advance and the #367 assertion), and the existing `eventually` helper the #434 fix is told to reuse.
3. Doctor, then: git worktree add -b test/tier2-convergence-fixtures ../taskboard-WT-test-tier2-convergence-fixtures off main.
4. Write the plan at .artifacts/plans/2026-10-25-tier2-convergence-fixtures-plan.md: the exact await predicate boot_convergence will use (which state, which deadline), the min-version assertion shape (parse + compare; what the floor is and why CARD_STARTDATE presence remains the seam), the #367 triage decision (fix by construction vs. annotate — decide from the harness reading), the two-tier re-run matrix and evidence format. No implementation, no push.
5. Commit: docs(plan): tier-2 convergence fixture repairs.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the backlog-triage dispatch ("tier-2 convergence fixtures").

1. Read AGENTS.md, issues #434/#435/#367, the dispatch plan §1/§4, and the stage-1 plan in ../taskboard-WT-test-tier2-convergence-fixtures (branch test/tier2-convergence-fixtures). Recreate the worktree if missing.
2. Implement per the plan. Hard rules: test-scaffolding changes only — git diff must show zero production files; the await uses the existing eventually helper with an explicit deadline (no sleeps anywhere); the min-version assertion parses, it does not string-compare; if the #367 fix is chosen, the stamp-advance guarantee is structural (fixture ordering/back-dating), never a sleep past a second boundary.
3. Bring the docker tier up locally and run the C-series + capabilities on both tiers (default, DECK_VERSION=v1.17.5); also run the untouched tier-1 f-series to prove no collateral. Paste transcripts into the report. If the docker tier cannot come up in your environment, STOP and report — do not merge unverified fixture changes.
4. Gates: the cargo gates for the crate (fmt, clippy, nextest for taskboard-sync-nextcloud at minimum; full workspace suite if time allows — it is the chunk's proof of no collateral).
5. Bookkeeping: comment the triage verdict on #367 (fixed-here or annotated-with-evidence). No push, no PR. Never push to or merge main.

Report: tier run records, gate results, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the backlog-triage dispatch (branch test/tier2-convergence-fixtures).

1. Read AGENTS.md §5, issues #434/#435/#367, the dispatch plan §1/§4 (code-quality mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Functional traps: the eventually predicate must test the real convergence condition (A's board/stack contents), not a proxy that passes early; the min-version floor must actually fail a below-floor decode (negative check or argued from the parse); no production diff; no sleep introduced anywhere in the harness; the DECK_VERSION=v1.17.5 leg genuinely ran (transcript, not assertion).
3. Re-run both tiers yourself (or the tier the environment allows, stating which); any red is a finding.
4. Fix findings on the branch. Ship per §1 step 3 — the PR description must carry the two-tier records. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, tier records location, worktree removed. Never merge; never touch main.
```

## 5. Chunk 4 — A1 gap-plug newtypes (#425) (`refactor/a1-gap-plug-newtypes`) — gated

- **GATE**: r2 chunks 5–8 are merged into `main` (stage 1 verifies
  `git fetch origin --prune && git log origin/main --oneline -8`
  shows the r2 visual-identity and drag-to-move chunks). If not
  merged: STOP and report. (User escape hatch: dispatching the
  `ConnectorId` leg alone early is allowed only by explicit user
  decision — the default is one PR per the pilot's success criteria.)
- **Covers**: issue #425 (normative: exact sites — the kebab +
  uniqueness validation at `crates/taskboard-app/src/config.rs`
  `AppConfig::validate` vs. the total `From<String>` mint at
  `crates/taskboard-domain/src/ids.rs:100-125`; the `ui.scale`
  range-check at `config.rs:959-960` travelling as bare `f64` through
  `UiSettings`; the plan sketch; success criteria) and the A1 house
  pattern of `docs/coding_guidelines.org` §2.1 (hand-rolled
  private-field newtypes, fallible mint at the trust boundary,
  documented leniency, no validate-and-forget, **no derive crates**).
  The ui-slint mount seam consumes the scale — post-r2 shape only.
  **Non-goals**: no branding/typestate/quantity machinery (A2–A4
  stays out per AGENTS §2); no other config keys; no
  deserialization-format changes.
- **Acceptance** (verbatim from the issue): no construction path
  outside deserialization bypasses the mint; `cargo mutants` finds no
  surviving mutant in the validators; existing config accept/reject
  semantics unchanged; documented leniency at the
  deserialization-only total `From`.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the backlog-triage dispatch ("A1 gap-plug newtypes", branch refactor/a1-gap-plug-newtypes) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify the GUI r2 chunks 5-8 are merged into origin/main (the settings-form, connection-management, visual-identity and drag chunks; look for their feat/ui-r2-* merges). If not merged: STOP and report.

1. Read AGENTS.md (§2 type-driven policy is normative), docs/coding_guidelines.org §2.1, GitHub issue #425 (normative), and .artifacts/reports/2026-10-24-type-driven-formal-methods-suitability-report.md §5 Pilot 1 (on main if its PR landed, else on branch docs/type-driven-suitability-and-guidelines).
2. Read the seams: crates/taskboard-domain/src/ids.rs:100-125 (the total From<String> and its callers), crates/taskboard-app/src/config.rs (AppConfig::validate's is_kebab_name check, the ui.scale check ~:959-960, UiSettings construction), every construction site of ConnectorId (grep — mint vs. deserialize), and the post-r2 ui-slint mount seam that consumes UiSettings' scale.
3. Doctor, then: git worktree add -b refactor/a1-gap-plug-newtypes ../taskboard-WT-refactor-a1-gap-plug-newtypes off main.
4. Write the plan at .artifacts/plans/2026-10-25-a1-gap-plug-newtypes-plan.md: the ConnectorId fallible mint signature (reusing is_kebab_name semantics — where the uniqueness half lives), which call sites mint vs. deserialize (the deserialization-only total From stays, with the A1-documented leniency comment), the UiScale(f64) shape and its journey through UiSettings to the mount seam, the mutant-kill test catalogue for both validators, and the accept/reject parity evidence (existing config tests must pass unchanged). No implementation, no push.
5. Commit: docs(plan): A1 gap-plug newtypes (ConnectorId, UiScale).

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the backlog-triage dispatch ("A1 gap-plug newtypes").

1. Read AGENTS.md §2, docs/coding_guidelines.org §2.1, issue #425, the dispatch plan §1/§5, and the stage-1 plan in ../taskboard-WT-refactor-a1-gap-plug-newtypes (branch refactor/a1-gap-plug-newtypes). Recreate the worktree if missing.
2. Implement per the plan. Hard rules: hand-rolled private-field newtypes only — no nutype/bounded-integer/refined (AGENTS §2 names them as out); the fallible mints return typed errors consistent with their crate's thiserror enums; every non-deserialization construction path goes through the mint; the deserialization-only total From carries the documented-leniency comment; API churn is pre-approved (maintainer ruling 2026-10-24, cited in #425) — do not preserve old constructors for compatibility.
3. Evidence: run the validator mutants locally (cargo mutants scoped to the two validators per the issue's success criteria) and record zero survivors; run the existing config accept/reject suite unchanged-green.
4. Full gate suite green (AGENTS §10). CHANGELOG Unreleased: one entry underChanged/internal only if the guide's convention includes internal refactors — otherwise none (API is internal; no user-observable change).
5. No push, no PR. Never push to or merge main.

Report: gate results, mutants evidence, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the backlog-triage dispatch (branch refactor/a1-gap-plug-newtypes).

1. Read AGENTS.md §2, coding_guidelines §2.1, issue #425, the dispatch plan §1/§5 (code-quality mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Functional traps: any construction path that bypasses the mint (grep every ConnectorId::from/Into and UiScale construction) is THE finding class; the deserialization leniency must be documented at the type, not in a distant comment; no derive-crate dependency may appear in any Cargo.toml; the ui-slint seam change must be the minimal handover (no presentation logic in app, no config knowledge in ui — AGENTS §2 boundary); accept/reject parity — a previously-rejected config must still be rejected and vice versa.
3. Re-run the gate suite and the validator mutant scoping yourself.
4. Fix findings on the branch. Ship per §1 step 3; teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 6. Chunk 5 — audit docs drift pass (#267, #268, #269, #278, #281, #437, #283-docs) (`docs/audit-drift-pass`)

- **Covers** (each issue is normative for its findings list): #267
  (architecture.org: SIG-bus subscriber set, crate dep lists,
  versioning example, the "task sorting" bench claim shared with
  testing_strategy §13), #268 (testing_strategy: DI description and
  type name, actor-testing example, proptest examples, mutants-report
  convention, docker journey file attribution), #269 (guidelines'
  foreign-project examples; ADR 0003/0010/0011 one-line amendments;
  CHANGELOG: merge the contradictory daemon-refusal pair, normalize
  categories per Keep-a-Changelog via the fixed `changelog.py`,
  optional MCP note), #278 (the color-eyre entry-path reading
  paragraph in coding_guidelines' error-handling section), #281 (ADR
  0004's tier-4 granularity citation, ADR 0010's Bootstrap paragraph
  and SyncDisabled list; ADR 0007's "one oneshot hop" wording — fix
  if trivial, else annotate on #281 pointing at #150's window
  correction), #437 (README Download section), and #283's
  documentation leg (UTC-only due-date semantics: `--due` CLI help +
  config guide). **Non-goals**: zero code changes (#283's code legs
  are chunk 9); no new ADRs; no architecture rewrites — one-line /
  few-paragraph fixes only.
- **Chunk-specific trap — re-verification**: the findings date to
  2026-10-17; the daily-driver wave and r2 churned all these files
  since. Each finding is re-checked against the current file before
  fixing; one already fixed upstream gets a one-line ✅ comment on its
  issue instead of a diff hunk; one made *worse* by the waves gets
  fixed to the current truth, not the October-17 text.
- **Deliverable**: the docs diff only. **Acceptance**: every listed
  finding addressed (fixed, or annotated on the issue as
  already-fixed/declined with a reason); the CHANGELOG normalization
  uses the repo's fixed `changelog.py` (per #269) and renders
  valid Keep-a-Changelog 1.1; pre-commit hooks green; no `.rs`/`.slint`
  file in the diff.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the backlog-triage dispatch ("audit docs drift pass", branch docs/audit-drift-pass) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§7 artefact rules), then issues #267, #268, #269, #278, #281, #437, #283 (normative — each finding by F-number), and the docs-drift theme report they cite (.artifacts/reports/2026-10-16-v0.1.0-audit-theme-docs-drift-report.md).
2. Re-verify every finding against the CURRENT files (docs/architecture.org, docs/testing_strategy.org, docs/coding_guidelines.org, docs/adr/0003|0004|0007|0010|0011, CHANGELOG.md, README.org, the tasks --due CLI help, the config guide): mark each finding fixed-already / still-drifted / drifted-further. This re-verification table IS the plan's core.
3. Doctor, then: git worktree add -b docs/audit-drift-pass ../taskboard-WT-docs-audit-drift-pass off main.
4. Write the plan at .artifacts/plans/2026-10-25-audit-docs-drift-pass-plan.md: the re-verification table, the per-finding fix (or annotate) decision, the CHANGELOG normalization recipe (changelog.py invocation + category mapping), the ADR amendment wording (one paragraph each, amendment-not-rewrite style), and the UTC-only documentation targets. No implementation, no push.
5. Commit: docs(plan): audit docs drift pass.

Report: plan path, the re-verification table summary, risks.
```

**Prompt 5.2 (execution):**

```text
You are the stage-2 EXECUTION agent for chunk 5 of the backlog-triage dispatch ("audit docs drift pass").

1. Read AGENTS.md, issues #267/#268/#269/#278/#281/#437/#283, the dispatch plan §1/§6 (the evidence & artefact mandate applies), and the stage-1 plan in ../taskboard-WT-docs-audit-drift-pass (branch docs/audit-drift-pass). Recreate the worktree if missing.
2. Execute per the plan: apply each still-drifted fix; amend ADRs with dated amendment notes (never silently rewrite ruled text); normalize CHANGELOG via changelog.py; keep the diff free of .rs/.slint files. Every change traceable to an F-number (commit messages cite them).
3. Verify: pre-commit hooks green; changelog renders/validates per the script; a grep sweep that each fixed claim no longer appears in its stale form. Zero cargo gates apply to docs, but run cargo fmt --check anyway to prove the tree is untouched code-wise (it must pass trivially).
4. Bookkeeping: comment ✅-with-permalink on each issue whose finding was already fixed upstream; the rest stay open until stage 3 ships (stage 3 closes them).
5. No push, no PR. Never push to or merge main.

Report: per-finding outcomes, any deviation from the plan and why.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the backlog-triage dispatch (branch docs/audit-drift-pass).

1. Read AGENTS.md, the seven issues, the dispatch plan §1/§6 (evidence & artefact mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Chunk-specific traps: amendment style — an ADR's ruled decision is annotated with a dated note, never rewritten to pretend it said otherwise; the CHANGELOG normalization must not lose entries (diff the entry count before/after); the UTC-only docs must state the actual implemented semantics (check the validation code's behavior before writing the help text — do not trust the issue's paraphrase); README's build-from-source recipe stays as the development alternative (only the stale "until the first release" framing goes).
3. Verify zero code files in the diff; verify every F-number is addressed (fixed or issue-annotated).
4. Ship per §1 step 3; close each covered issue in the PR's final bookkeeping (one close comment per issue naming the fixing commit), then teardown.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

## 7. Chunk 6 — Kani pilot (#426) (`test/kani-pilot`)

- **Covers**: issue #426 (normative: the five-harness
  smallest-to-hardest list — `BackoffPolicy::delay`,
  `ReminderOffset` text round-trip, `aggregate_rows` phase-dominance,
  `days::day_window` totality, bounded `plan_pushes` grouping — the
  CI shape (weekly `kani.yml` mirroring `miri.yml`: scheduled +
  manual dispatch, per-family jobs if walls demand, **never in the
  PR pipeline**), the approved triggers, and the success criteria
  **including the honest-termination path**). **Non-goals**: no
  production-code changes (harnesses are `#[cfg(kani)]`-gated); no
  Prusti/Lean work (#428/#429 own the monitoring); the stable
  toolchain pin is untouched locally — Kani manages its own nightly
  inside the workflow/container.
- **Acceptance** (verbatim intent from the issue): ≥3 harnesses
  *proven* within documented unwind bounds; counterexamples converted
  via concrete-playback into regression tests; dependency-induced
  failures contained per-harness (dropped/stubbed, documented), never
  project-blocking; OR the honest-termination finding written
  (proptests remain the right tool) with the evidence — both exits
  close the issue; the workflow file exists and its scheduled/manual
  shape mirrors `miri.yml`.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the backlog-triage dispatch ("Kani pilot", branch test/kani-pilot) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #426 (normative), and the suitability report's §3 C1 / §5 Pilot 2 (branch docs/type-driven-suitability-and-guidelines if not yet on main). Also read .github/workflows/miri.yml — the structural template kani.yml mirrors.
2. Probe feasibility BEFORE planning the harness catalogue: install/run kani (cargo kani or the container) on a throwaway harness over the smallest target (BackoffPolicy::delay) to learn the toolchain/version constraints on this machine. Record versions, wall times, and any chrono/serde symbolism limitations you hit — these decide the unwind bounds and the per-harness stub list.
3. Doctor, then: git worktree add -b test/kani-pilot ../taskboard-WT-test-kani-pilot off main.
4. Write the plan at .artifacts/plans/2026-10-25-kani-pilot-plan.md: the harness catalogue (which of the five targets, in which order, with per-harness unwind bounds and expected proof statements), the stub/containment list per harness, the kani.yml shape (schedule, dispatch, toolchain pin, artifact handling), the concrete-playback regression-test plan, and the termination criterion (what evidence ends the pilot either way). No implementation beyond the throwaway probe (which does not land), no push.
5. Commit: docs(plan): Kani pilot on the pure domain core.

Report: plan path, probe findings (versions/walls/limitations), decisions, risks.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the backlog-triage dispatch ("Kani pilot").

1. Read AGENTS.md, issue #426, the dispatch plan §1/§7, and the stage-1 plan in ../taskboard-WT-test-kani-pilot (branch test/kani-pilot). Recreate the worktree if missing.
2. Implement per the plan: #[cfg(kani)] harnesses in the named modules (crates/taskboard-sync-nextcloud/src/backoff.rs, crates/taskboard-domain/src/{reminders,state,days,push}.rs as far as the catalogue goes), each with documented unwind bounds; kani.yml mirroring miri.yml (weekly schedule + workflow_dispatch; per-family jobs only if the plan's walls demand; never a PR-pipeline job). Convert any counterexample via concrete-playback into a regular regression test. Hard rules: zero production-code edits (if a proof seems to need one, that is a finding to record, not a change to make); harnesses never run under cargo nextest/cargo test (cfg-gating must guarantee it).
3. Evidence: run every harness locally to its proven/failed/honest-termination outcome and paste the kani output summaries; record wall times per harness for the CI budget note.
4. Gates: the standard suite must stay green (harness code is cfg-gated out); bash -n / actionlint-style check on kani.yml if available.
5. No push, no PR. Never push to or merge main.

Report: per-harness outcomes (proven/failed/terminated), wall times, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the backlog-triage dispatch (branch test/kani-pilot).

1. Read AGENTS.md, issue #426, the dispatch plan §1/§7, and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Functional traps: a harness that passes because it under-constrains the symbolic input (missing preconditions/kani::assume drift) proves nothing — check each harness's assumptions against the function's real contract; unwind bounds must be stated at the harness and justified (not tuned until green); the cfg gate must make it impossible for the harness to enter the normal test builds; kani.yml must not run in the PR pipeline and must mirror miri.yml's structure.
3. The honest-termination exit is acceptable ONLY with written evidence (what was attempted, which symbolism limit blocked it, why proptests remain right) — a silent shortfall is a finding.
4. Re-run every harness yourself (or as many as the environment allows, stating which). Fix findings on the branch. Ship per §1 step 3; the PR closes #426 with the outcome comment. Teardown per §1 step 3.

Report: verdict (incl. pilot outcome per the success criteria), findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 8. Chunk 7 — true-partial re-measure + #373 re-justification (#390) (`docs/bench-true-partial-remeasure`) — execution chunk

- **Covers**: the open half of #390 and the stale-premise half of
  #373. Premise update (verified 2026-10-25): #390's fix direction —
  incremental model push instead of whole-`VecModel` replacement —
  **already landed** as GUI-r2 D9 (`70c812d`, "stable model identity
  via row-diff push"; `push_rows`/`diff_rows` in
  `crates/taskboard-ui-slint/src/models.rs`). What remains is
  measurement and bookkeeping: re-run
  `crates/taskboard-ui-slint/benches/refresh_draw.rs` against current
  `main` (both lanes, the bench's standard N grid), record the
  dirty-region instrument, and produce
  `.artifacts/reports/2026-10-25-refresh-draw-true-partial-remeasure-report.md`
  (method; fresh numbers vs. the 2026-10-20 chunk-6 and 2026-10-21
  netbook baselines; the verdict for perf-budget row 3's partial leg;
  what survives of #373's motivation). Then update the tracker:
  comment on #390 with the fresh numbers and the recommended
  disposition (expected: close as resolved-by-D9, residual = the
  on-device D250 measurement, carried by the kiosk wave's next
  session) and **edit #373's body** so the stale "97 % dirty region"
  figure and the pre-D9 lane table are replaced (or explicitly marked
  superseded with a pointer to the new report) — the ListView
  analysis *ask itself* stays intact; whether it is still worth
  analyzing is a fresh user decision on fresh numbers. **Non-goals**:
  no renderer/model/bench code changes; no on-device D250 session; no
  #373 implementation; no ui-slint edits (the bench run is read-only
  over the crate).
- **Deliverable**: the report artefact only — the PR diff is exactly
  that file. **Acceptance**: bench method reproducible from the
  report (commands, machine, commit); both issue updates done and
  cross-linked; the evidence & artefact mandate satisfied.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of the backlog-triage dispatch ("true-partial re-measure", branch docs/bench-true-partial-remeasure) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, issues #390 and #373 (normative), and the baselines you will compare against: .artifacts/reports/2026-10-20-kiosk-chunk6-refresh-draw-bench-stage2-report.md and .artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md (slot 2 numbers).
2. Read the bench: crates/taskboard-ui-slint/benches/refresh_draw.rs (its lanes, N grid, the dirty-region instrument) and confirm the r2 D9 premise in crates/taskboard-ui-slint/src/models.rs (push_rows/diff_rows; commit 70c812d). Verify the bench still builds against current main before planning around it (a quick cargo bench --no-run in a scratch state is acceptable evidence).
3. Doctor (this chunk builds a worktree with a full target/ — the floor check matters), then: git worktree add -b docs/bench-true-partial-remeasure ../taskboard-WT-docs-bench-true-partial-remeasure off main.
4. Write the plan at .artifacts/plans/2026-10-25-bench-true-partial-remeasure-plan.md: the exact bench invocations (lanes × N grid, warmup/repetition counts, machine state notes), the comparison table layout vs. the baselines, the report skeleton with its verdict section, and the exact #390/#373 update texts (drafts). No implementation, no push.
5. Commit: docs(plan): true-partial remeasure and render-budget issue refresh.

Report: plan path, bench-build status, decisions, risks.
```

**Prompt 7.2 (execution):**

```text
You are the stage-2 EXECUTION agent for chunk 7 of the backlog-triage dispatch ("true-partial re-measure").

1. Read AGENTS.md, issues #390/#373, the dispatch plan §1/§8 (evidence & artefact mandate applies), and the stage-1 plan in ../taskboard-WT-docs-bench-true-partial-remeasure (branch docs/bench-true-partial-remeasure). Recreate the worktree if missing.
2. Run the bench exactly per the plan (record machine load context — the numbers are dev-machine references, never gates). Write the report at .artifacts/reports/2026-10-25-refresh-draw-true-partial-remeasure-report.md per its skeleton: method, numbers, comparison vs. the 2026-10-20/21 baselines, the row-3 partial-leg verdict, and the one recommendation on #373's analysis-worthiness (decision-grade: options, trade-offs, one recommendation).
3. Tracker updates (in-scope bookkeeping): comment on #390 with the fresh numbers + recommended disposition; edit #373's body replacing/marking the stale figures with a pointer to the new report — the analysis ask stays. Do not close either issue; disposition happens at stage 3/user.
4. The PR diff must be exactly the report file. No code changes. Gates: none apply beyond a clean tree (cargo fmt --check as the canary).
5. No push, no PR. Never push to or merge main.

Report: the numbers, report path, issue-update diffs, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of the backlog-triage dispatch (branch docs/bench-true-partial-remeasure).

1. Read AGENTS.md, issues #390/#373, the dispatch plan §1/§8 (evidence & artefact mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Chunk-specific traps: methodology honesty — repetition counts, warmup, and machine-state caveats must be stated; the comparison must not cherry-pick (all baseline lanes reproduced or their absence explained); the verdict must respect the house rule that these are dev-machine references, never gates; the #373 body edit must preserve the issue's analysis ask and only fix stale facts; the diff contains ONLY the report.
3. Re-run one bench lane yourself as a spot-check (or state why not).
4. Ship per §1 step 3; on green, apply the recommended #390 disposition only if it is "close as resolved-by-D9 with the on-device residual noted" — anything beyond that is the user's call, report instead. Teardown per §1 step 3 (this worktree's target/ is large — the removal is mandatory, verify with git worktree list and a df check).

Report: verdict, findings, fixes, PR URL, green checks, issue updates applied, worktree removed. Never merge; never touch main.
```

## 9. Chunk 8 — mutants sweep extension (#427) (`ci/mutants-ui-pure-modules`)

- **GATE note**: the workflow edit is inert for PRs and dispatchable
  anytime; the survivor-triage half is most meaningful after r2
  merges (`view_state.rs` churn). The dispatcher picks the slot;
  stage 1 verifies which r2 chunks are merged and records the timing
  consequence in the plan.
- **Covers**: issue #427 (normative: add
  `crates/taskboard-ui-slint/src/resolver.rs`, `view_state.rs`,
  `presentation.rs` to the target list in
  `.github/workflows/mutation-testing.yml`, scoped to those pure
  modules; success criteria include the **documented revert path** if
  mutant runtime proves disproportionate). **Non-goals**: no test
  changes; no target-list changes beyond the three modules; no PR
  pipeline involvement (weekly lane stays weekly).
- **Acceptance**: the weekly lane produces the survivor report;
  triaged per house rules (diagnostic-text/permanent-noise classes
  annotated per #274's precedents; real gaps → follow-up issues, not
  inline test-churning); runtime recorded; revert executed and
  documented in the same chunk if the criterion trips.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of the backlog-triage dispatch ("mutants sweep extension", branch ci/mutants-ui-pure-modules) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #427 (normative), the suitability report §5 Pilot 3, and .github/workflows/mutation-testing.yml (current target list, schedule, artifact handling). Note #274's ignore-list precedent for permanent-noise survivor classes and #327's box-command template for runtime expectations.
2. Check whether cargo-mutants supports file-level scoping within a -p target (its file-filter options) and draft the exact workflow diff (or the config it needs).
3. Doctor, then: git worktree add -b ci/mutants-ui-pure-modules ../taskboard-WT-ci-mutants-ui-pure-modules off main.
4. Write the plan at .artifacts/plans/2026-10-25-mutants-ui-pure-modules-plan.md: the workflow diff, the local validation run (capped, to measure runtime before the weekly lane pays for it), the triage rubric (which survivor classes are annotate-only vs. follow-up-issue), the revert criterion and its mechanics, and where the survivor report lands (.artifacts/reports/). No implementation, no push.
5. Commit: docs(plan): extend the weekly mutants sweep to the UI pure modules.

Report: plan path, scoping feasibility, decisions, risks.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of the backlog-triage dispatch ("mutants sweep extension").

1. Read AGENTS.md, issue #427, the dispatch plan §1/§9, and the stage-1 plan in ../taskboard-WT-ci-mutants-ui-pure-modules (branch ci/mutants-ui-pure-modules). Recreate the worktree if missing.
2. Implement the workflow change per the plan. Validate locally with a capped run (the plan's timeout template; SQLX_OFFLINE=true; verify git status is clean after the --in-place run). Triage survivors per the rubric: annotate-only classes go into the report with reasons; real gaps become follow-up issues (backlog: prefix, category label — AGENTS §7), never inline test churn.
3. Write the survivor report artefact (runtime, counts, triage, follow-up issue links). If the revert criterion tripped, execute the revert and document it in the same report — the workflow change and its undo are both legitimate outcomes; an unmeasured merge is not.
4. Gates: workflow YAML lint (actionlint if available); workspace suite untouched and green (cargo nextest -p taskboard-ui-slint as the canary).
5. No push, no PR. Never push to or merge main.

Report: runtime, survivor counts, triage outcomes, deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of the backlog-triage dispatch (branch ci/mutants-ui-pure-modules).

1. Read AGENTS.md, issue #427, the dispatch plan §1/§9, and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Chunk-specific traps: the target list must be exactly the three pure modules (no component-journey creep — that is the issue's explicit scoping); the weekly schedule/cron must be untouched; the report's triage must not have weakened or text-asserted any test (AGENTS §5); the revert path is either executed-with-evidence or explicitly not tripped with the runtime numbers that show it.
3. Ship per §1 step 3; close #427 with the outcome comment (or, on the revert path, close with the scoping decision recorded per the issue's own success criteria). Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 10. Chunk 9 — time/DST code fixes (#283 code legs) (`fix/time-dst-small-fixes`)

- **Covers**: issue #283 legs 1–2 (the documentation leg rides chunk
  5 — do not duplicate it): (a) `ReminderOffset` whole-second by
  construction (`crates/taskboard-domain/src/reminders.rs` — the
  sub-second construction the storage column truncates, causing the
  reload-divergence double-fire; follow the A1 discipline: reject at
  the mint with a typed error, document the leniency boundary), and
  (b) the `sync` verb's freshness check detected structurally
  (generation counter or phase transition; `last_success` becomes
  display-only) so a backward clock step can not misreport
  `TimedOut` (`crates/taskboard-app/src/commands/` sync path).
  **Non-goals**: no storage schema change (whole-second is enforced
  at the type, not the column); no reminder evaluator changes; no
  DST semantics changes beyond the issue's three findings.
- **Acceptance**: the double-fire construction is impossible via the
  public API (pinned by a test asserting the typed rejection); a
  backward wall-clock step no longer yields a false `TimedOut`
  (structural assertion with the injected clock seam — no wall reads
  in tests); existing offset round-trip property tests updated, not
  weakened; gate suite green.

**Prompt 9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 9 of the backlog-triage dispatch ("time/DST code fixes", branch fix/time-dst-small-fixes) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #283 (normative, findings F11-03/04/05 — you own 03 and 04; 05 is chunk 5's), and the time/DST theme report it cites.
2. Read the seams: crates/taskboard-domain/src/reminders.rs (ReminderOffset's constructors and its text_form/parse_text_form), the storage codec's offset column handling in crates/taskboard-storage-sqlite (the truncation site), crates/taskboard-app/src/commands/ sync verb freshness comparison (the two wall stamps), and the existing clock-seam patterns in the state engine's tests for the structural-freshness design.
3. Doctor, then: git worktree add -b fix/time-dst-small-fixes ../taskboard-WT-fix-time-dst-small-fixes off main.
4. Write the TDD-first plan at .artifacts/plans/2026-10-25-time-dst-small-fixes-plan.md: the whole-second mint (reject vs. truncate — recommend one per A1; the typed error), every construction path audited (text form, API, deserialization — deserialization leniency documented if kept), the structural freshness mechanism (generation counter vs. phase transition — recommend one; where the counter lives; what last_success remains for), and the test catalogue (double-fire impossibility, backward-clock no-false-TimedOut with injected clock, round-trip properties updated). No implementation, no push.
5. Commit: docs(plan): whole-second offsets and structural sync freshness.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 9.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 9 of the backlog-triage dispatch ("time/DST code fixes").

1. Read AGENTS.md, issue #283, the dispatch plan §1/§10, and the stage-1 plan in ../taskboard-WT-fix-time-dst-small-fixes (branch fix/time-dst-small-fixes). Recreate the worktree if missing.
2. Implement per the plan, TDD, every commit standalone. Hard rules: typed errors via thiserror at the domain boundary; no wall-clock reads in tests (inject the clock seam); property tests updated to the whole-second domain — not deleted; `last_success` stays display-only (no behavior contract on its value).
3. Full gate suite green (AGENTS §10). CHANGELOG Unreleased: one bug-fix entry (the false-TimedOut fix is user-observable).
4. Bookkeeping: do NOT close #283 (chunk 5 owns the docs leg); comment on it that legs 1-2 landed with the PR reference — stage 3 closes it once both halves are merged.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 9 of the backlog-triage dispatch (branch fix/time-dst-small-fixes).

1. Read AGENTS.md, issue #283, the dispatch plan §1/§10 (code-quality mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Functional traps: truncation-vs-rejection must match stage 1's recorded decision everywhere (a stray accept path reintroduces the double-fire); the freshness mechanism must not add engine state whose persistence would need a migration (structural = runtime/phase-derived, or justify otherwise); the false-TimedOut test must fail on the pre-fix code (verify by argument or a temporary revert — record which); no test asserts wall-clock time.
3. Re-run the full gate suite. Fix findings on the branch. Ship per §1 step 3; teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 11. Chunk 10 — snapshot published-half narrowing (#245) (`test/snapshot-published-narrowing`)

- **Covers**: issue #245 (normative: F4-03; the exact target shape —
  keep each persisted half as the whole-state contract dump; replace
  the published half with one structural equality assertion
  (memory ≡ disk, now *checked* rather than eyeballed) plus a small
  `{sync, last_updated}` tail snapshot; 14 files → 7 dumps + 7
  tails) in `crates/taskboard-state/tests/snapshots.rs` + the
  snapshot files. **Non-goals**: no domain/state shape changes; no
  new snapshot scenarios; no engine behavior changes (the equality
  assertion must pass against today's engine unmodified — if it
  fails, that is a *finding about the engine*, escalate per AGENTS §5,
  do not loosen the assertion).
- **Acceptance**: the snapshot review surface is halved as specified;
  the structural equality is asserted in the suite (a deliberate
  memory/disk divergence would fail it — argue or demo this in
  review); insta gates green; the timing is the point: less review
  noise for every future domain shape change.

**Prompt 10.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 10 of the backlog-triage dispatch ("snapshot published-half narrowing", branch test/snapshot-published-narrowing) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 snapshot-testing directive), issue #245 (normative), and the state-crate audit report it cites (F4-03).
2. Read crates/taskboard-state/tests/snapshots.rs and the 14 snapshot files: confirm the persisted/published halves differ only in header and tail today; identify the equality seam (what "published equals persisted projection" means structurally); identify the tail fields ({sync, last_updated}).
3. Doctor, then: git worktree add -b test/snapshot-published-narrowing ../taskboard-WT-test-snapshot-published-narrowing off main.
4. Write the plan at .artifacts/plans/2026-10-25-snapshot-published-narrowing-plan.md: the equality assertion shape (which projection, compared how), the tail-snapshot format, the 14→7+7 file migration mechanics (insta regeneration is NOT acceptable as the review path — the diffs must be auditable), and the escalation rule if the equality fails against the current engine (production-bug suspicion per AGENTS §5). No implementation, no push.
5. Commit: docs(plan): snapshot published-half narrowing.

Report: plan path, decisions, risks.
```

**Prompt 10.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 10 of the backlog-triage dispatch ("snapshot published-half narrowing").

1. Read AGENTS.md, issue #245, the dispatch plan §1/§11, and the stage-1 plan in ../taskboard-WT-test-snapshot-published-narrowing (branch test/snapshot-published-narrowing). Recreate the worktree if missing.
2. Implement per the plan: the structural equality assertion, the tail snapshots, the file migration — every removed half accounted for (7 dumps + 7 tails, no orphan files). If the equality assertion fails against the current engine: STOP, treat it as a suspected production bug per AGENTS §5, and report — do not weaken the assertion to pass.
3. Gates: insta/nextest for the state crate, then the full suite. No CHANGELOG (test-infra only).
4. Bookkeeping: none beyond the PR (stage 3 closes #245).
5. No push, no PR. Never push to or merge main.

Report: gate results, the equality check's outcome, deviations.
```

**Prompt 10.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 10 of the backlog-triage dispatch (branch test/snapshot-published-narrowing).

1. Read AGENTS.md, issue #245, the dispatch plan §1/§11 (code-quality mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Functional traps: the equality assertion must be load-bearing (verify it actually compares memory-vs-disk projections — a tautology passes nothing); the 7 remaining dumps must be the persisted halves (the whole-state contracts), byte-auditable in the diff; no snapshot content change may hide inside the migration (every dump file's diff is either none or explainable header/tail churn); file count exactly 14 → 14 (7 dumps + 7 tails) unless the plan recorded a deviation.
3. Re-run the state suite + full gates. Fix findings on the branch. Ship per §1 step 3; close #245. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 12. Chunk 11 — app audit p2 batch (#253 + #254) (`refactor/app-audit-p2`)

- **Covers**: #253 (rename `crates/taskboard-app/src/session_ceremonies.rs` —
  the module hosts the frontend-agnostic `attach_from_persisted`
  called by the IPC server; rename the module, e.g.
  `connector_ceremonies`, or move the daemon attach body beside its
  only caller — stage 1 picks with a recorded reason) and #254 (hoist
  the diverged test scaffolding into
  `crates/taskboard-app/tests/common/mod.rs`: one `wait_until` with
  one budget and `FnMut` semantics, the kill-on-drop `Child` wrapper,
  the daemon-spawn helper). Serial within the branch (#253's commit
  first), one PR. **Non-goals**: no behavior changes; the flaky trio
  #384/#408/#417 is **not** fixed here (chunk 12 owns them) — but the
  hoisted `wait_until` must not silently change any caller's budget
  (the 10 s vs 30 s divergence gets one recorded decision, and the
  choice must be justified per call-site, not averaged).
- **Acceptance**: the Sonar duplication clusters the audit named are
  gone; the rename is complete (no stale paths in docs/comments —
  grep-audited); every former call site compiles against the single
  helper with its budget decision recorded; full app suite green with
  unchanged semantics; gate suite green.

**Prompt 11.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 11 of the backlog-triage dispatch ("app audit p2 batch", branch refactor/app-audit-p2) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, issues #253 and #254 (normative), and the app-crate audit report they cite (F8-03, F8-08).
2. Read crates/taskboard-app/src/session_ceremonies.rs end to end (its doc vs. its contents; attach_from_persisted and its only IPC caller), then the diverged scaffolding: every wait_until definition in crates/taskboard-app/tests/ (budgets, Fn vs FnMut), the duplicated Child wrapper files, the single daemon-spawn helper. Audit the budget divergence call-site by call-site (which callers rely on 10 s, which on 30 s) — this decides the one-budget answer.
3. Doctor, then: git worktree add -b refactor/app-audit-p2 ../taskboard-WT-refactor-app-audit-p2 off main.
4. Write the plan at .artifacts/plans/2026-10-25-app-audit-p2-plan.md: the rename/move decision with reasons, the per-call-site budget table and the single-budget decision, the hoisting order (rename commit first, scaffolding commit second), and the grep-audit checklist for stale references. No implementation, no push.
5. Commit: docs(plan): app audit p2 batch (rename + scaffolding hoist).

Report: plan path, decisions, risks.
```

**Prompt 11.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 11 of the backlog-triage dispatch ("app audit p2 batch").

1. Read AGENTS.md, issues #253/#254, the dispatch plan §1/§12, and the stage-1 plan in ../taskboard-WT-refactor-app-audit-p2 (branch refactor/app-audit-p2). Recreate the worktree if missing.
2. Implement per the plan: commit 1 = the #253 rename/move (mechanical; tests follow the move); commit 2 = the #254 hoist (one wait_until, FnMut, the recorded budget; Child wrapper; daemon-spawn fixture into tests/common/mod.rs). Hard rules: zero behavior changes (a test that changes outcome is a finding, not an adaptation); no opportunistic flaky fixes (chunk 12 owns those).
3. Gates: full workspace suite (the app crate's tests are the subject), then AGENTS §10 in full.
4. Bookkeeping: none beyond the PR (stage 3 closes both issues).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 11.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 11 of the backlog-triage dispatch (branch refactor/app-audit-p2).

1. Read AGENTS.md, issues #253/#254, the dispatch plan §1/§12 (code-quality mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Functional traps: the rename must leave zero stale references (grep the whole repo including .artifacts plan references only as notes — code/docs references must be updated); the single wait_until must preserve every caller's effective semantics (a caller that secretly needed 30 s now failing under load is exactly the flaky trio's territory — check the budget table was honored); the hoisted Child wrapper keeps kill-on-drop; no production behavior change anywhere in the diff.
3. Re-run the full gate suite. Fix findings on the branch. Ship per §1 step 3; close #253 and #254. Teardown per §1 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

## 13. Chunk 12 — flaky-watchlist dedicated pass (#384, #408, #417, #431, #287, #336) (`test/flaky-watchlist-pass`) — #431 leg gated

- **GATE**: the #431 leg requires the r2 round merged (its subject is
  ui-slint test support); the taskboard-app legs (#384/#408/#417) are
  dispatchable once chunk 11 has merged (they triage against the
  hoisted scaffolding); #287/#336 are dispatchable anytime.
- **Covers**: the six watchlist entries not owned by chunk 3
  (#367). AGENTS §7's rule is the frame: one issue per distinct test,
  dedicated pass, never opportunistic. Per issue: attempt
  reproduction (stress profile per the issue's own observation
  context), root-cause or annotate with a capture policy, fix **only
  where root-caused**, and land every disposition as a comment on the
  watchlist issue. Known directions (from the issues, normative):
  #431 — harness bug, not timing: make the two `support::driver`
  self-tests agree on `headless_renderer: true` (the issue's quick
  experiment says the geometry reads work over the testing backend)
  or move them into their own test binary; #408/#417 — load-sensitive
  bounded joins: explicit/injected detach budgets or deadline polls
  per each issue's proposed direction; #384 — prompt-routing under
  load: no repro yet, so the deliverable is a capture policy (what to
  record on the next occurrence), not a speculative fix; #287/#336 —
  single unexplained observations: capture policy + recurrence
  watch.
- **Acceptance**: every listed issue carries a disposition comment
  (fixed-with-root-cause / root-caused-and-fixed / annotated
  capture-policy); no test weakened to pass (AGENTS §5); fixes carry
  regression tests where applicable; the suite's determinism rules
  upheld (no sleeps — budget/poll seams per AGENTS §5).

**Prompt 12.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 12 of the backlog-triage dispatch ("flaky watchlist pass", branch test/flaky-watchlist-pass) in /home/viktoria/src/taskboard.

GATE: the taskboard-app legs need chunk 11 (refactor/app-audit-p2) merged (the triage builds on the hoisted helpers); the #431 leg needs the GUI r2 round merged. Verify what is merged; scope the pass to the legs whose gates are open and say so explicitly in the plan.

1. Read AGENTS.md §5 and §7 (the flaky policy is the frame), then issues #384, #408, #417, #431, #287, #336 (normative — each carries its observation, mechanism reading, and suggested direction).
2. For each leg: read the named test and its surrounding harness; assess reproducibility (stress profiles: full-suite parallel load, llvm-cov instrumentation per #287's context); decide fix vs. annotate with the evidence you have.
3. Doctor, then: git worktree add -b test/flaky-watchlist-pass ../taskboard-WT-test-flaky-watchlist-pass off main.
4. Write the plan at .artifacts/plans/2026-10-25-flaky-watchlist-pass-plan.md: per-leg disposition plan (fix-with-regression-test vs. capture-policy annotation), the repro protocol per leg (commands, load shape, iteration count), and the no-weakening rule's application to each candidate fix. No implementation, no push.
5. Commit: docs(plan): flaky watchlist dedicated pass.

Report: plan path, gate status per leg, decisions, risks.
```

**Prompt 12.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 12 of the backlog-triage dispatch ("flaky watchlist pass").

1. Read AGENTS.md §5/§7, the six watchlist issues, the dispatch plan §1/§13, and the stage-1 plan in ../taskboard-WT-test-flaky-watchlist-pass (branch test/flaky-watchlist-pass). Recreate the worktree if missing.
2. Execute per the plan: run the repro protocols (negative results are results — record them); implement the root-caused fixes with regression tests (injected budgets/deadline polls per AGENTS §5 — never sleeps); write the capture-policy comments for the annotate-only legs. Hard rules: a fix must make the failure mode impossible or observed-safe under the repro protocol, not merely pass once; no production-code changes unless a leg's root cause lands there (then it is a bug fix, TDD, its own commit, and the watchlist issue says so).
3. Gates: full workspace suite green; for load-sensitive fixes, the plan's stress profile re-run with the recorded outcome.
4. Bookkeeping: draft each disposition comment (stage 3 posts/closes after ship).
5. No push, no PR. Never push to or merge main.

Report: per-leg outcomes (repro attempts, fixes, annotations), deviations.
```

**Prompt 12.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 12 of the backlog-triage dispatch (branch test/flaky-watchlist-pass).

1. Read AGENTS.md §5/§7, the six issues, the dispatch plan §1/§13 (code-quality mandate applies), and the stage-1 plan; review the full diff after rebase per §1 step 3.
2. Chunk-specific traps: the classic flaky "fix" failures — replacing one timing constant with a bigger one (a budget must be injected/derived, not enlarged); poll-without-deadline; a test weakened to pass (compare each touched assertion against its issue's original semantics); production changes without their own TDD commit; watchlist issues closed without a disposition comment that stands alone (a reader must reconstruct the evidence from the comment alone).
3. Re-run the stress profiles the plan names for every fixed leg. Fix findings on the branch. Ship per §1 step 3; post the disposition comments and close the fully-disposed issues (annotate-only legs stay open with their capture policy posted). Teardown per §1 step 3.

Report: verdict, per-leg dispositions, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 14. Design rounds queued (not chunks — each produces a plan; a follow-up
dispatch round follows acceptance)

Dispatching these as implementation chunks here would violate the
house rule that dispatch plans chunk *accepted* work. Each entry is
one design round with its issue set; the round's plan is the
deliverable, its acceptance is the user's.

- **DR-1 — reminder-rule sync protocol (#202)**: the wave-daily-driver
  milestone's last open item. At round entry, also decide the
  milestone question (schedule the round under the milestone, or move
  the issue off so the wave can close). Audit inputs already
  verified: F16-15/16/17 (substrate readiness, the state-home
  requirement for anything that roams).
- **DR-2 — signal handling, once**: #174 (scoped SIGINT in the
  connect ceremony), #192 (REPL line cancellation), #196 (IPC client
  deadlines/cancellation), #200 (line-discipline restore on signal
  death), #162 (signal-escalation policy). Every issue names the same
  resolution: one signal design, designed together.
- **DR-3 — reminders surface pair**: #352 (timers & alarms UI) +
  #203 (insistence levels), ruled to be designed together (D-R9 /
  D-R12); note #203's F16-19 lane question (the `SystemEvent`
  broadcast's `Copy` derive vs. a dedicated lane) is the round's
  structural decision.
- **DR-4 — sync merge-policy family**: #128 (field-level
  granularity), #129 (OR-set labels), #130 (resurrect-as-new-card),
  #133 (clock-skew; wants the sync-actor `Clock` seam — F11-01 — as
  its first work item), with #150 (duplicate-create reconciliation)
  adjacent. One round, one agent: these share `merge.rs`/`pipeline.rs`
  semantics and must never be parallelized across agents.

## 15. Completion bookkeeping

- **Chunk → issue closures**: 1→#386 · 2→#395 · 3→#434, #435
  (+#367 disposition) · 4→#425 · 5→#267, #268, #269, #278, #281,
  #437 (+#283 docs leg; #283 itself closes when chunk 9 merges) ·
  6→#426 · 7→#390 (+#373 body update; #373 stays open for the
  implementation decision) · 8→#427 · 9→#283 · 10→#245 · 11→#253,
  #254 · 12→dispositions on #384, #408, #417, #431, #287, #336.
- **Already executed outside this dispatch (2026-10-25)**: #311
  closed as completed — the deny ignore-with-reason remediation
  (commit `d4a9cda`) verified green (`cargo deny check advisories` →
  `advisories ok` on main @ `90c947c`); the zbus-6 refresh tracks in
  successor issue **#440** (the `deny.toml` comment pointer moves
  #311 → #440 in this dispatch plan's own PR).
- **User-gated manual step after chunk 3 merges**: dispatch the
  weekly Nextcloud Integration lane on the daily-driver code —
  `gh workflow run nextcloud-integration.yml` (#436) — and triage its
  output against chunk 3's records. Never dispatched autonomously.
- **Deliberately not dispatched here** (with their homes): the §14
  design rounds; the trigger-gated perf/storage entries (#131, #135,
  #137, #138, #139, #141, #146, #148, #152, #153, #154, #155, #157 —
  measurement/growth triggers per their bodies); migration-locked
  renames (#140 — bundle with the next schema migration per ADR
  0005); registers and no-action verdicts (#274, #275, #277, #284,
  #285, #286); calendar monitoring (#428, #429); release/CI
  infrastructure (#113 remainder, #114, #115, #116, #117); deferred
  API surface (#121, #122, #127); roadmap waves & ADHD follow-ups
  (#210, #212, #213, #214, #215, #216, #217, #218 — #217/#218's
  Wave-1 gates are now satisfied but they are product decisions, not
  maintenance); org-mode epic (#184, #185 — behind the ADR 0010
  vocabulary fork shared with DR-1); undo v2 family (#176, #353,
  #354, #355, #356, #357 — needs-consideration rulings); IPC trio
  (#193, #194, #195 — #194 is the only small standalone one and can
  join a later wave on request); REPL/uuid DX entries (#180, #191,
  #224, #225, #198 — several ride DR-2 or #167's consumer trigger);
  the kiosk wave resume (#363, and the render-budget implementation
  decision that chunk 7's report feeds); the retry-after trio
  (#220, #221, #222 — additive, live-tier-triggered).
- **What opens next**: the §14 design rounds feed a follow-up
  dispatch plan once their plans are accepted; chunk 7's verdict
  gates the #373/#390 render-budget decision; chunk 3 plus the #436
  lane run close the daily-driver round's last tier-2 gap; with the
  r2 round merged, chunk 4 unblocks and the kiosk wave's own plan
  (`.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`,
  chunk K13 onward) becomes the next large front.
