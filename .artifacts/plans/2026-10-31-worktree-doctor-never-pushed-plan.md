# Worktree-doctor never-pushed blind spot (#450) — implementation plan

- **Date**: 2026-10-31
- **Chunk**: 1.5 of backlog round 2 (6 batches / 27 chunks;
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`,
  "Chunk 1.5" — Covers/Acceptance there are normative for this plan)
- **Branch**: `fix/worktree-doctor-never-pushed` (base: `main` ==
  `origin/main` @ `5bf90c63` — verified equal at planning time)
- **Worktree**: `../taskboard-WT-fix-worktree-doctor-never-pushed`
  (flat suffix; created off `main`, exists at planning handoff)
- **Normative sources**: AGENTS §8 (the doctor is the only sanctioned
  bulk-removal tool; exit contract "nonzero = free disk below the
  floor, and only that"); GitHub issue #450 (the swept-worktree
  incident); incident record
  `.artifacts/plans/2026-10-25-tier2-convergence-fixtures-plan.md`
  (Incident note + Risks §1); sweep-fix plan
  `.artifacts/plans/2026-10-25-worktree-doctor-sweep-plan.md`
  (porcelain enumeration + checklist this chunk must preserve);
  dispatch §Chunk 1.5 (scope, acceptance, stage-3 trap list).

## 1. Scope

Files this chunk may touch: **`scripts/worktree-doctor.sh` only**
(dispatch Covers; stage-3 checks this).

Non-goals (binding):

- **AGENTS.md is not touched** — the stage-3 prompt explicitly checks
  that no doc change lands in this chunk.
- **No CHANGELOG entry** — the dispatch's docs round owns changelog
  promotion; #450's close comment carries the record (D3).
- **No behavior change to any existing protection**: the porcelain
  enumeration, the tip guard, dirty/unmerged/on-main/locked KEEP
  paths, the remove-refusal path, the coverage scrub, the orphan
  WARN, the fetch/df containment, and the floor test all behave
  exactly as before (acceptance: "behave exactly as before"; stage-2
  hard rule: "all KEEP protections are preserved verbatim").
- **No network calls in the doctor** (stage-2 hard rule;
  `ls-remote` rejected in D1).
- The residual window of §2.3/D4 is **documented and pinned, not
  closed** — closing it is provably incompatible with the acceptance
  row "a pushed-and-merged leftover is still REMOVE" (§2.3).

Line numbers below refer to the script as of `5bf90c63` (145 lines);
they are orientation, never load-bearing.

## 2. Evidence (re-verified 2026-10-31 on the real checkout)

### 2.1 The decision path a sweepable worktree walks today

For a registered worktree on a non-`main` branch, readable, with a
clean tree, and `origin/main` resolvable, the script decides:

1. **line 92** — `HEAD == origin/main` tip → `KEEP (HEAD at
   origin/main tip — fresh or resume)` — the #415 guard (2026-10-23)
   for "fresh (just created, nothing committed yet) or a resume on a
   landed branch".
2. **line 96** — HEAD *not* an ancestor of `origin/main`
   (`git merge-base --is-ancestor` fails) → `KEEP (in flight)`.
3. **lines 100–114** — otherwise: liveness re-check, then plain
   `git worktree remove` → `REMOVE (HEAD merged)`; a refused removal
   (dirty/untracked/locked) lands on `KEEP (remove refused…)`.

Exit paths are already the sweep-fix contract: the only nonzero is
the final floor test; fetch/enumeration/per-entry failures degrade to
WARN/SKIP/KEEP lines. The 2026-10-25 plan's §5 contract is intact and
this chunk must not disturb it (D5).

The gap: step 1's protection is only as current as the run's
`origin/main`, which the sweep-start `git fetch origin --prune`
(line 37) refreshes *before* the loop. A worktree created at tip X is
protected only while the fetched tip is still X; once `origin/main`
advances to Y (another agent's PR lands — normal in this repo), the
worktree reads as "HEAD merged" (X is an ancestor of Y) and is swept
in step 3 — with **zero commits and a clean tree**, i.e. exactly
indistinguishable from a landed-PR leftover by the three predicates
the script consults.

### 2.2 The #450 failure mode, reproduced

Incident recap (#450 + the tier2 record): a stage-1 worktree created
with `git worktree add -b <new-branch>` off the then-current
`origin/main` tip was swept mid-session by a parallel agent's doctor
run; the tier2 record adds the mechanism — the worktree was
afterwards "re-created at **the new** tip", i.e. `origin/main` had
advanced between creation and sweep. The repro below replays both
phases on the real checkout (the throwaway branch is created, and
gone, within this session; nothing was pushed and no neighbor was
touched — all three live worktrees KEEPed throughout).

**Phase 1 — at the tip (the issue-title scenario):**

```
$ git worktree add -b throwaway/doctor-450-repro \
    ../taskboard-WT-throwaway-doctor-450-repro origin/main
Preparing worktree (new branch 'throwaway/doctor-450-repro')
branch 'throwaway/doctor-450-repro' set up to track 'origin/main'.
HEAD is now at 5bf90c63 docs(plan): every round-2 prompt fully fleshed out …

$ bash scripts/worktree-doctor.sh
KEEP   (in flight):           …-WT-fix-anchor-filename-fallback
KEEP   (in flight):           …-WT-perf-ui-unchanged-refresh-guard
KEEP   (uncommitted changes): …-WT-refactor-scheduled-phrase-typed-direction
KEEP   (HEAD at origin/main tip — fresh or resume): …-WT-throwaway-doctor-450-repro
SKIP   coverage scrub (an llvm-cov run looks active)
free: 207G (floor: 25G)
doctor exit: 0
```

The #415 tip guard covers the at-tip case **today** — #450's literal
scenario ("a fresh worktree … at origin/main's tip reads as
disposable") no longer reproduces. Note also the side effect in the
transcript: `worktree add -b … origin/main` **itself** set tracking
config (`set up to track 'origin/main'`) — this observation kills one
of the candidate fixes (D6.2).

**Phase 2 — the incident mechanism, emulated** (`origin/main~5` stands
in for "main advanced past the creation tip"; the real `origin/main`
obviously cannot be advanced on demand):

```
$ git -C ../taskboard-WT-throwaway-doctor-450-repro reset --hard -q origin/main~5
$ bash scripts/worktree-doctor.sh
KEEP   (in flight):           …-WT-fix-anchor-filename-fallback
KEEP   (in flight):           …-WT-perf-ui-unchanged-refresh-guard
KEEP   (uncommitted changes): …-WT-refactor-scheduled-phrase-typed-direction
REMOVE (HEAD merged):         …-WT-throwaway-doctor-450-repro
SKIP   coverage scrub (an llvm-cov run looks active)
free: 206G (floor: 25G)
doctor exit: 0
```

`REMOVE (HEAD merged)` on a **never-pushed, zero-commit, clean**
worktree — the sweep removed it during the run (confirmed: `git
worktree list` no longer lists it). That is the #450 blind spot as it
actually fires; in a real incident the run's own sweep-start fetch is
what moves `origin/main` past the creation tip (§2.1).

Cleanup after the repro: `git branch -D throwaway/doctor-450-repro`;
a follow-up doctor run exited 0 with the throwaway gone (the
instructed "expect exit 0 with the throwaway gone" state), and the
chunk worktree was then created off `main`.

### 2.3 The equivalence that bounds the fix (why the window can't be closed)

A zero-commit never-pushed worktree strictly below the *fetched*
`origin/main` tip is **byte-identical, in every git-state input the
doctor may consult locally**, to a pushed-and-landed leftover whose
remote ref was pruned — the rebase-dance shape AGENTS §8 produces
(a fully-landed branch is reset onto `origin/main`; after the remote
branch is deleted, `fetch --prune` removes
`refs/remotes/origin/<branch>`). Both shapes: branch ref below tip,
HEAD contained in `origin/main`, clean tree, **no remote ref**.
Candidate discriminants, each examined and rejected:

- **remote ref**: absent for both — that is the problem.
- **upstream config** (`branch.<name>.merge`): not a signal at all —
  `git worktree add -b X <path> origin/main` sets it *itself*
  (§2.2 phase 1 transcript), so a never-pushed branch carries it from
  birth, pointing at `refs/heads/main` regardless of any push.
- **reflog**: differs today (rebase/reset entries vs "Created from"),
  but reflogs are an expendable audit trail (gc expiry; AGENTS §8's
  own commit-hygiene discipline amends and resets branches). A
  removal decision must not read it.
- **worktree age (mtime/ctime)**: mutable, and the checkout lives
  under a sync'd filesystem; no semantics, arbitrary thresholds.

Therefore **any** local predicate either spares both shapes or
removes both. The dispatch's stage-3 trap — "a branch that IS pushed
but whose remote ref was pruned must not read as never-pushed unless
it is also not contained in `origin/main`" — makes the choice
explicit: remove both. The residual window (never-pushed + zero
commits + clean + strictly below the fetched tip) stays REMOVE-able
and is **pinned** as such by matrix row 2 (D4); the three mitigations
that already exist are documented in the script header (D4). Issue
#450's at-tip scenario remains covered by the tip guard, verified in
§2.2 phase 1.

## 3. Decision D1 — the spare rule: "never pushed", local refs only

**`never-pushed(branch)` ⟺ no `refs/remotes/origin/<branch>` exists
AND the worktree's HEAD is not fully contained in `origin/main`.**

- **Ref test**: `git show-ref --verify --quiet
  "refs/remotes/origin/$branch" 2>/dev/null` from the anchor — exit 0
  iff the exact ref exists; purely local, O(refs), offline-stable.
  `git rev-parse --verify --quiet` is an acceptable equivalent;
  stage 2 picks one. (`git ls-remote --dry-run` is **rejected**: a
  network round-trip per branch — unacceptable cost per the dispatch,
  and it would make the predicate nondeterministic offline, exactly
  the runs the fetch warn-and-continue was built to serve.)
- **Containment**: reuse the script's existing
  `git merge-base --is-ancestor "$head_commit" "$origin_main"`
  invocation and its result — **no second containment call** (stage-3
  DRY trap: "the containment check reuses the doctor's existing
  unmerged-HEAD logic rather than duplicating it"). Containment is
  non-strict: HEAD == tip counts as contained, so an at-tip branch is
  never-pushed = false and belongs to the tip guard, which already
  KEEPs it — consistent with the Covers text ("a fresh `-b` branch at
  `origin/main`'s tip … must be KEEP").
- **Why the conjunction** (both arms normative): "no remote ref"
  alone would spare the pruned-ref contained leftover — violating the
  acceptance row "a pushed-and-merged leftover is still REMOVE" and
  the stage-3 trap. "Not contained" alone is the existing in-flight
  check. The conjunction is what *names* the never-pushed class: a
  branch whose commits exist nowhere but this machine — a durable,
  fetch-proof property, robust to prunes and to future refactors of
  the surrounding labels.

**Honest behavioral note, for stage 3**: with the conjunction, the
spare fires exactly where `KEEP (in flight)` fires today — no
worktree changes from REMOVE to KEEP or back. The delta is the
durable, named label plus the documented stance (D2) and the pinned
residual (D4). The acceptance rows are satisfied: every protection
"behaves exactly as before", and the alternative expansion (sparing
the below-tip zero-commit case — the incident's exact shape) is
impossible without violating the pruned-leftover REMOVE requirement
(§2.3). Reviewers should review the *condition text* against the
trap, not expect a REMOVEd class to change.

### 3.1 Placement (illustrative, not final code)

Inside the existing containment branch — the single KEEP line becomes
a two-way label:

```sh
  if ! git -C "$ROOT" merge-base --is-ancestor "$head_commit" "$origin_main" 2>/dev/null; then
    if git -C "$ROOT" show-ref --verify --quiet "refs/remotes/origin/$branch" 2>/dev/null; then
      echo "KEEP   (in flight):           $wt"
    else
      echo "KEEP   (never pushed — local-only branch): $wt"
    fi
    continue
  fi
```

Nothing else in the loop changes: the tip guard, dirty check, SKIP
paths, liveness re-check, and removal line stay verbatim; the
porcelain enumeration, scrub, orphan WARN, and floor test stay
verbatim. Message wording is fixed ("never pushed — local-only
branch"); column padding is stage-2 cosmetic.

The script's header comment gains exactly three things:

1. item 1 of "What it does" lists the never-pushed spare among the
   never-touched protections;
2. the D2 stance sentence (verbatim below);
3. the D4 known-limitation paragraph (verbatim below).

## 4. Decisions D2–D6 (every open question, resolved)

**D2 — false-positive stance (verbatim draft for the header):**
"The spare errs toward KEEP by design: a deliberately abandoned
scratch branch — never pushed, with or without unique commits — is
pruned by hand (`git worktree remove --force`, then `git branch -D`),
not by the doctor; the doctor is not its tool."
Consequence, accepted: a stale never-pushed worktree costs disk until
a human prunes it; the doctor never automates that. This is the same
bias the 2026-10-25 plan fixed as D3's containment direction — every
ambiguous case lands KEEP, never REMOVE.

**D3 — #450 close-out framing (stage 3 executes):** the issue's
literal scenario (fresh worktree at the tip) has been guarded since
the #415 tip guard — re-verified §2.2 phase 1; this chunk adds the
named never-pushed spare, the recorded stance, and the characterized
residual. The close comment states all three, including the residual
window explicitly — nothing silently dropped.

**D4 — the residual window: pinned, not fixed (verbatim draft for
the header):** "Known limitation: a worktree that is never pushed,
has zero commits, a clean tree, and sits strictly below the *fetched*
origin/main tip is indistinguishable from a landed-PR leftover whose
remote ref was pruned, and is removed. Mitigations: the tip guard
covers the same-tip window; uncommitted or untracked files make
removal refused (KEEP); commit as the first action after creating a
worktree. A scratch worktree in that state is pruned by hand (see the
never-pushed stance above)." Matrix row 2 asserts the behavior and is
named as a pin — the scripted analogue of a
`..._currently_pins_...` test, so nobody mistakes it for an endorsed
contract.

**D5 — exit-code contract, restated (unchanged; from the 2026-10-25
plan §5, acceptance requires "exit semantics unchanged"):** exit 0 ⇔
sweep and scrub completed (whatever KEEP/REMOVE/SKIP/WARN/STALE lines
they printed) AND free space ≥ floor; exit nonzero ⇔ the final floor
test failed (below floor, or unmeasurable, which counts as below);
the `free:` verdict is the last output line. The new predicate adds
no exit path: both arms of the label decision end in KEEP/continue.

**D6 — rejected alternatives, recorded so they stay dead:**
1. *Single-condition spare* ("no remote ref" alone — issue #450's
   literal suggestion): spares the pruned-ref contained leftover →
   violates acceptance row "pushed-and-merged leftover is still
   REMOVE" and the stage-3 trap. Rejected.
2. *Upstream config as a "was pushed" signal*: inverted at birth —
   `worktree add -b … origin/main` sets `branch.<name>.merge`
   itself (§2.2). Rejected.
3. *Reflog forensics* (rebase entries betray the dance): expendable
   audit trail (gc; amend/reset hygiene). Rejected.
4. *Worktree-age heuristics*: mutable timestamps on a sync'd fs.
   Rejected.
5. *Pre-fetch `origin/main` for the tip guard*: the danced leftover
   is below the pre-fetch tip too — identical again (§2.3). Rejected.

## 5. Verification matrix (stage 2 executes for real; transcripts into the stage-2 report and PR body)

Hermetic lab per the 2026-10-25 plan §7 — cases run there unless
marked REAL, so fixtures can never collide with live worktrees; the
real checkout's `origin/main` is never rewritten and nothing is
pushed anywhere but the lab origin:

```sh
L=/tmp/doctor-lab; rm -rf "$L"; mkdir -p "$L"
git init --bare -q -b main "$L/origin.git"
git clone -q "$L/origin.git" "$L/repo"
git -C "$L/repo" -c user.email=l@l -c user.name=l commit -q --allow-empty -m c1
git -C "$L/repo" push -q origin main
git -C "$L/repo" -c user.email=l@l -c user.name=l commit -q --allow-empty -m c2
git -C "$L/repo" push -q origin main     # origin/main = c2; c1 is a strict ancestor
```

Anchor lab runs with `WORKTREE_DOCTOR_ROOT="$L/repo"`. The doctor's
sweep-start `fetch origin --prune` then talks to the lab origin. If
`git -C "$L/repo" rev-parse origin/main` does not resolve before the
fixtures, run one explicit `git -C "$L/repo" fetch origin`.

New rows (this chunk):

| # | case | setup (lab unless marked) | expected observable |
|---|---|---|---|
| 1 | fresh never-pushed at tip | `worktree add -b b1 <p> origin/main`, clean | `KEEP (HEAD at origin/main tip — fresh or resume)` |
| 2 | **residual window — PINNED (D4)** | `worktree add -b b2 <p> c1` (strict ancestor), clean, never pushed, no remote ref | `REMOVE (HEAD merged)` — asserted as the documented residual; named as a pin, not an endorsement |
| 3 | never-pushed with unique commits | `worktree add -b b3 <p> c1`, one empty commit, never push | `KEEP (never pushed — local-only branch)` — **the new label** |
| 4 | pushed, remote counterpart alive (PR-open shape) | `b4` off `c1` + empty commit, `push -u origin b4` | `KEEP (in flight)` — label unchanged |
| 5 | pushed-and-merged leftover, ref alive | `b5` off `c1` + empty commit, `push -u origin b5`, then `push origin HEAD:refs/heads/main` from b5 (fast-forward) | `REMOVE (HEAD merged)` — has ref ⇒ not never-pushed |
| 6 | pushed-and-merged leftover, ref **pruned** (the rebase-dance shape; the stage-3 trap) | row 5's fixture + `push origin --delete b5` + one `fetch --prune` | `REMOVE (HEAD merged)` — must **not** read never-pushed |
| 7 | pruned ref, NOT contained (the trap's "unless" arm) | `b7` off `c1` + empty commit, `push -u`, delete remote branch, `fetch --prune` | `KEEP (never pushed — local-only branch)` |
| 8 | dirty | worktree + touch a file | `KEEP (uncommitted changes)` |
| 9 | unmerged HEAD (pushed shape) | = row 4 | `KEEP (in flight)` |
| 10 | on-main worktree | lab **only** (AGENTS §8 forbids on-main worktrees in the real checkout): `git -C $L/repo worktree add <p> main` | `KEEP (no branch or primary branch)` |
| 11 | below floor | `WORKTREE_DOCTOR_MIN_FREE_G=100000`, lab **and** real checkout | exit nonzero; floor verdict is the last line |

REAL-checkout rows:

| # | case | setup | expected observable |
|---|---|---|---|
| 12 | bogus sibling tolerated (the #395 class) | `mkdir` a plain `taskboard-WT-bogus` dir next to the real checkout, run, `rm` after | `WARN  not a registered worktree (left alone)`; floor verdict still reached; exit 0 |
| 13 | tree stays clean | `git status --short` on the real checkout after every doctor run | empty, every time |
| 14 | exit-contract sweep | every lab row above | every exit is 0 or the floor exit — nothing else (D5) |

Carried regression rows from the 2026-10-25 checklist (re-run in the
lab; must not regress): detached HEAD → `KEEP (no branch or primary
branch)`; locked (merged+clean fixture) → `KEEP (remove refused…)`;
prunable (registered, dir gone) → `STALE`; unreadable registered →
`SKIP`; fetch failure (lab origin moved aside) → `WARN fetch failed`
+ floor verdict; nested registered worktree swept; space-in-path
round-trip; `WORKTREE_DOCTOR_ROOT` at a bare repo → empty sweep,
floor decides.

Gates: `bash -n scripts/worktree-doctor.sh`; `shellcheck` if
available (not installed on the dev box — record that, as the
2026-10-25 chunk did); a `bash -x` spot-trace of one never-pushed
pass showing exactly one `merge-base --is-ancestor` call and one
`show-ref` call, and no network-invoking binary in the new path.

## 6. Risks

- **"Diagnostic-only" reads as a no-op diff in review.** §2.3/§3 is
  the argument for why the behavioral space is closed under the
  acceptance; the PR body must quote §2.3 so stage 3 reviews the
  condition text against the trap instead of hunting for a REMOVEd
  class that changed.
- **Row 2 looks like "the bug is still there".** It is the bounded,
  documented residue of a proven impossibility (§2.3), pinned
  deliberately (D4); the PR body and the #450 close comment (D3) must
  say so explicitly.
- **Lab drift**: git defaults (branch name — `-b main` on init), the
  remote-tracking ref after the first push (one explicit `fetch`
  fixes it), `push -u` side effects. All lab-local; the real checkout
  is untouched by fixtures.
- **Concurrent agents**: the change makes nothing removable that
  wasn't removable; neighbor KEEP lines are unchanged (baseline run
  §2.2: three live worktrees KEEPed, exit 0, 207G free). The doctor
  remains safe to run by any agent at any time.
- **Regression surface is small but the contract is absolute**: the
  stage-3 hunt treats D5 as THE regression target — a nonzero exit
  that means anything but "below the floor" reintroduces the
  #395/#450 class of false STOP for every future agent.

## 7. Estimated diff

- `scripts/worktree-doctor.sh`: ~+12–16 lines (the label branch
  inside the containment `if` + three header-comment additions).
- No other files; no Rust surface; no `Cargo.lock`; no AGENTS.md; no
  CHANGELOG (§1 non-goals).
- This plan: docs, committed on the chunk branch.

## 8. Stage-2/3 handoff

- **Stage 2** implements §3.1 verbatim (wording fixed, padding free),
  runs §5 for real, pastes transcripts, flags any deviation with
  reasons. Worktree
  `../taskboard-WT-fix-worktree-doctor-never-pushed` exists on the
  branch (recreate on the branch if missing, per dispatch §1).
- **Stage 3** re-runs the matrix (or as much as the environment
  allows, stating which), checks the exact condition against the
  stage-3 trap (pruned + contained ⇒ REMOVE; pruned + not contained ⇒
  never-pushed label), verifies the new path makes no network calls,
  verifies AGENTS.md is untouched, and re-proves D5 as THE regression
  target. Handoff at "PR open + `gh pr checks` green" (PR title per
  dispatch: `fix(scripts): the doctor spares never-pushed branches`);
  close #450 per D3 with the transcripts summarized. Never merge;
  never touch `main`.
