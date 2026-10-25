# Worktree-doctor sweep robustness (#395) — implementation plan

- **Date**: 2026-10-25
- **Chunk**: 2 of the backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §3)
- **Branch**: `fix/worktree-doctor-sweep` (base: `origin/main` @
  `1e625b6` — the local `main` checkout was 3 commits behind at planning
  time, so the branch and this worktree were cut off the fetched tip)
- **Worktree**: `../taskboard-WT-fix-worktree-doctor-sweep` (flat
  suffix — the very convention this chunk records, §4)
- **Normative sources**: GitHub issue #395 (exit-128 trace, both fix
  directions, nested-layout question); report
  `.artifacts/reports/2026-10-21-kiosk-milestone-remaining-steps-report.md`
  §4; AGENTS §8 (the doctor is the only sanctioned bulk-removal tool);
  dispatch §3 (scope, non-goals, acceptance).

## 1. Scope

Files this chunk may touch:

1. `scripts/worktree-doctor.sh` — enumeration, failure containment,
   nothing else.
2. `AGENTS.md` §8 — exactly one new bullet recording the flat naming
   convention (verbatim draft in §4 below), inserted immediately after
   the "Create worktrees with `git worktree add -b …`" bullet in the
   Active-worktree safety list (currently AGENTS.md lines 232–236).

Non-goals (dispatch §3, binding):

- No other script changes — the coverage-scrub logic, the `pgrep`
  guard, and the env knobs (`WORKTREE_DOCTOR_ROOT`,
  `WORKTREE_DOCTOR_MIN_FREE_G`) keep their semantics.
- No changes to the KEEP predicate *semantics*: uncommitted changes,
  HEAD at `origin/main` tip, unmerged HEAD, primary/no branch, locked
  — all five protections behave exactly as before against every
  registered worktree.
- No test-harness infrastructure: no new script files, no bats/shell
  test framework. The scripted checks live in this plan (§7) and their
  transcripts go into the stage-2 report and PR body (dispatch
  acceptance: "new test or scripted check recorded in the PR").
- The observed `pgrep -f llvm-cov` false-positive class (the 2026-10-25
  baseline run skipped the scrub because an unrelated process matched
  the pattern) is *not* this chunk's problem; file a separate backlog
  issue if it recurs.

## 2. Evidence inventory (re-verified 2026-10-25)

### 2.1 The #395 failure mode, reproduced

With a throwaway plain directory `/home/viktoria/src/taskboard-WT-bogus`
present (created and removed in the same session; the two live neighbor
worktrees were never evaluated because the script dies at the first,
alphabetically-earliest glob entry):

```
$ mkdir /home/viktoria/src/taskboard-WT-bogus
$ bash scripts/worktree-doctor.sh
fatal: not a git repository (or any of the parent directories): .git
doctor exit: 128
$ rm -rf /home/viktoria/src/taskboard-WT-bogus
```

Matching #395's trace exactly. Note what is *missing* from the output:
no `KEEP`/`REMOVE` lines, no `free:` line — the run died at
`scripts/worktree-doctor.sh:33` (`git -C "$wt" branch --show-current`
under `set -euo pipefail`) before reaching the sweep body or the floor
check at line 83. The one nonzero exit the script is specified to
produce (AGENTS §8: "a nonzero exit means free disk is below the
floor: STOP and report") was never evaluated, and every agent that
honors §8 by running the doctor before creating a worktree would
wrongly STOP.

### 2.2 Baseline (exit 0 today, no bogus sibling)

```
$ bash scripts/worktree-doctor.sh; echo "doctor exit: $?"
KEEP   (uncommitted changes): …-WT-docs-reminder-rule-sync-design-round
KEEP   (in flight):           …-WT-feat-ui-r2-visual-identity
SKIP   coverage scrub (an llvm-cov run looks active)
free: 67G (floor: 25G)
doctor exit: 0
```

67 GiB free against the 25 GiB floor. The scrub skipped because
something matched `pgrep -f llvm-cov` (see non-goal above).

### 2.3 Exit-path audit of the current script (83 lines)

Every command that can abort the script under `set -euo pipefail`
*before* the final floor test, i.e. every path where a nonzero exit
means something other than "below the floor":

| line(s) | command | guarded today? | abort trigger |
|---|---|---|---|
| 29 | `git fetch origin --prune` | no | network loss, bad remote |
| 33 | `git -C $wt branch --show-current` | no | **#395**: `$wt` not a repo; also half-broken registration (`.git` file present, admin dir gone) |
| 38, 59 | `git -C $wt status --porcelain` | no | same triggers as line 33 |
| 48 | `git -C $wt rev-parse HEAD` | no | same triggers |
| 49 | `git -C $ROOT rev-parse origin/main` | no | no `origin/main` ref (fresh/no-remote clone) |
| 53 | `git merge-base --is-ancestor` | yes (`if !`) | — (failure = KEEP in-flight) |
| 63 | `git worktree remove` | yes (`if`) | — (failure = KEEP) |
| 70 | `pgrep -f llvm-cov` | yes (`if`) | — |
| 81 | `df --output=avail -k $ROOT` | no (but `tail` in pipeline masks it partially; `pipefail` re-exposes it) | df failure |
| 83 | `[ $free_g -ge $MIN_FREE_G ]` | **the** designed exit | below floor |

Lines 34/53/63/70's guards mean the KEEP *predicates* are individually
sound; the fragility is entirely in unguarded per-entry and
infrastructure calls. The fix must contain every row above, not just
line 33 — otherwise the "nonzero ⇔ floor" contract stays false and the
next odd sibling (or the next offline laptop) reintroduces #395's
caller-facing impact with a different exit code.

### 2.4 `git worktree list --porcelain` format (git 2.53.0, lab-verified)

Verified in a hermetic lab (`/tmp/doctor-lab`: bare origin + clone +
linked worktrees). Records are blank-line-separated; `worktree <path>`
is each record's first line and the path is the raw rest-of-line (no
quoting — a path containing a space round-trips through
`sed -n 's/^worktree //p'` + `IFS= read -r`). Observed record shapes:

```
worktree /tmp/doctor-lab/repo          ← anchor: listed first
HEAD 51da3a2…
branch refs/heads/master

worktree /tmp/doctor-lab/repo-WT-detached
HEAD …
detached                               ← no `branch` key

worktree /tmp/doctor-lab/repo-WT-gone
HEAD …
branch refs/heads/feat/gone
prunable gitdir file points to non-existent location   ← dir deleted; still listed

worktree /tmp/doctor-lab/repo-WT-locked
HEAD …
branch refs/heads/feat/locked
locked
```

A bare anchor (`WORKTREE_DOCTOR_ROOT` aimed at a bare repo) lists a
single record: `worktree <path>` + `bare`. After excluding the anchor
path, the sweep list is empty and only the floor check has work to do.

Implications the implementation must honor:

- Parse per record, key on `worktree` lines only; never join `branch`
  keys across records (detached records have none).
- A `prunable` entry's directory may not exist → keep the
  `[ -d "$wt" ]` guard, but now report the entry (`STALE`) instead of
  skipping silently — porcelain *knows* it is registered, which the
  glob never did.
- `locked` needs no special handling: the existing remove-refusal path
  (line 63's `if`) already lands it on KEEP.
- Enumeration order is registration order, not sorted — irrelevant to
  the sweep's semantics.
- Tool versions on the dev box: git 2.53.0, bash 5.3.9;
  `shellcheck` is **not installed** → the gate is `bash -n` plus a
  `bash -x` trace review (dispatch acceptance: "shellcheck (if
  configured)").

## 3. Decision D1 — enumeration mechanism: porcelain + per-entry containment

**Recommendation: `git worktree list --porcelain` from the anchor as
the enumeration source of truth, combined with a per-entry
tolerate-and-continue guard on every `git -C "$wt"` call.** The
dispatch frames this as porcelain *vs.* tolerated-glob; the lab showed
they are not actually exclusive — porcelain fixes *enumeration*
(plain dirs never enter the loop), the per-entry guard fixes
*evaluation* (a registered-but-broken worktree degrades to a KEEP line
instead of aborting). Porcelain alone leaves the second crash class
open; tolerated-glob alone leaves the first. The chunk takes both;
the two mechanisms are D1's content.

### 3.1 Why porcelain wins for enumeration

1. **The loop can only act on registered worktrees anyway.** Every
   predicate it evaluates (branch, status, merge-base) and the removal
   itself (`git worktree remove`, line 63) are defined for — and in
   the removal's case *require* — a registered worktree. The glob
   enumerates entities the loop cannot reason about; porcelain
   enumerates exactly the entities every later line was written for.
2. **Structural, not positional, crash prevention.** The
   tolerated-glob keeps glob matches as first-class loop inputs and
   patches over each `git` call individually; any future per-entry
   `git` invocation added above the guard silently reintroduces the
   #395 crash. With porcelain, a plain directory is structurally
   unable to enter the loop — there is no position left for the bug to
   return to.
3. **Nested layouts become visible for free** (the second half of
   #395's impact: the real worktree inside
   `taskboard-WT-ci/benchmark-lane-off-pr` was invisible to the flat
   glob). A registered worktree is swept at whatever path it lives at;
   no second glob level, no depth policy in the script.
4. **Stale registrations surface instead of festering.** `prunable`
   entries (dir deleted behind git's back) are reported; the glob
   variant never sees them again.
5. **Parse stability**: the porcelain format has been stable since git
   2.7 and is additive; this parse reads only `worktree` keys and
   ignores all other keys, so future keys cannot break it (verified
   shapes in §2.4).

### 3.2 Why tolerated-glob loses as the primary mechanism

- It *tolerates* the failure mode rather than removing its input;
  guard coverage must be re-audited on every future edit of the loop.
- Nested registered worktrees stay invisible (the glob is flat; the
  issue's second symptom survives the fix).
- The orphan-directory problem it handles (plain `taskboard-WT-*`
  dirs) is better served by an explicit cross-check that *warns* — see
  §3.3 — because a plain dir must never become a sweep target in the
  first place (§3.3's safety note).
- Per-entry `2>/dev/null` guards would also swallow genuine
  diagnostics (e.g. a corrupt repo's real error text) that the
  porcelain + guard hybrid prints verbatim in its SKIP line.

### 3.3 The orphan cross-check (keeping the glob, demoting it)

After the porcelain sweep, list glob entries that are *not* in the
registered set and print one `WARN` line each; they are never removal
or scrub targets:

```sh
comm -23 <(ls -d "$ROOT"-WT-* 2>/dev/null | sort) \
         <(git -C "$ROOT" worktree list --porcelain | sed -n 's/^worktree //p' | sort)
```

Safety note (bounds the whole design): an unregistered
`taskboard-WT-*` directory must **never** be removed or scrubbed by
the doctor. Its git predicates are unevaluable, so no KEEP protection
could be applied to it — deleting it would be `rm -rf` by pattern
match, exactly what AGENTS §8 ("ad-hoc `rm -rf` of a `*-WT-*`
directory is never acceptable") exists to forbid. Warn and move on;
cleanup of true orphans stays a human decision.

### 3.4 Structural shape (illustrative, not final code)

```sh
# resolve sweep list from the anchor's registry; anchor excluded by exact path
mapfile -t WORKTREES < <(git -C "$ROOT" worktree list --porcelain |
                         sed -n 's/^worktree //p' | grep -vxF "$ROOT")
for wt in "${WORKTREES[@]}"; do
  [ -d "$wt" ] || { echo "STALE (registered, dir gone): $wt"; continue; }
  if ! branch="$(git -C "$wt" branch --show-current 2>/dev/null)"; then
    echo "SKIP  (unreadable registered worktree): $wt"; continue
  fi
  … existing KEEP predicates verbatim, each git call's failure → KEEP …
done
# coverage scrub iterates "${WORKTREES[@]}", not the glob
# orphan WARN via the §3.3 comm
```

(`mapfile` + process substitution keeps `set -e` semantics intact and
avoids a subshell around the loop. A `while IFS= read -r` loop is an
equally acceptable spelling — stage 2 picks one; both are
space-safe.)

## 4. Decision D2 — nested-layout ruling: naming deviation, flat convention recorded

**Ruling: the nested `taskboard-WT-<parent>/<child>/` layout is a
naming deviation, not a sanctioned shape.** The doctor gains nothing
by scanning deeper (D1's porcelain enumeration already sweeps a
registered worktree at any depth — the safety question is moot), and
sanctioning variable-depth layouts would force every consumer of the
layout — the doctor's diagnostics, the 2026-10-18 disk-usage
accounting, human scanning of `../` — to reason about trees where a
flat sibling convention has always sufficed. The deviation was also
the *incubator* of the bug: a branch name used verbatim as the path
suffix produces a plain parent that masquerades as a worktree sibling.
The ruling is recorded where the deviation was made possible — the
naming rule in AGENTS §8.

**Verbatim draft** for AGENTS.md §8, Active-worktree safety list, as a
new bullet immediately after the "Create worktrees with `git worktree
add -b <branch> …`" bullet (currently lines 232–236):

```markdown
  - Spell the worktree path suffix flat: replace `/` in the branch
    name with `-` (`fix/worktree-doctor-sweep` →
    `../taskboard-WT-fix-worktree-doctor-sweep`), never use the
    verbatim branch name as the suffix — a branch containing a slash
    then nests the real worktree under a plain
    `taskboard-WT-<parent>/` directory that is not itself a worktree
    (#395), hiding it from path-glob tooling and masquerading as a
    worktree sibling to everything that scans the layout.
```

(Three-and-change wrapped lines; matches the list's voice and its
issue-citation habit. Stage 2 applies it verbatim unless the wrap
collides with future edits — reflow is fine, wording is not.)

## 5. Decision D3 — the exit-code contract, made literal

Dispatch §3 fixes the contract as "nonzero = free disk below the
floor, and *only* that" and the stage-2 hard rule adds "the floor
check must be reachable on every path". Today neither is literally
true (§2.3's unguarded rows abort with other meanings). This chunk
makes the contract literal. The exact contract the script must
exhibit after the fix:

1. **exit 0** ⇔ the sweep and scrub completed (whatever
   KEEP/REMOVE/SKIP/WARN/STALE lines they printed) **and** free space
   ≥ floor.
2. **exit nonzero** ⇔ the final floor test failed — free space below
   floor, **or** free space unmeasurable (treated as below floor; see
   row df below). No other nonzero path exists.
3. The final statement remains the unmodified form
   `[ "$free_g" -ge "$MIN_FREE_G" ]`; the floor verdict (`free: NG`
   line) remains the last output line.
4. **Containment direction**: every contained failure lands on a KEEP
   or SKIP diagnostic — never REMOVE. A worktree whose safety
   predicates could not all be evaluated is kept, by construction.

Per-row containment (rows from §2.3):

| failure | containment | diagnostic | sweep effect |
|---|---|---|---|
| `git fetch` (line 29) | warn, continue | `WARN  fetch failed — merge decisions use stale origin/main` | sweep proceeds; floor decides exit |
| enumeration `git worktree list` | warn, empty list | `WARN  cannot enumerate worktrees` | loop no-ops; floor decides exit |
| any per-entry `git -C $wt` | SKIP/KEEP line, continue | names the worktree + reason | that entry only |
| `rev-parse origin/main` (resolved **once** before the loop) | report-only sweep | `WARN  origin/main unresolvable — sweep is report-only` | every entry KEEP; no removals |
| `df` (line 81) | treat as below floor | `free: ? (floor: NG)` | exit nonzero via the normal final test |
| `pgrep`, `merge-base`, `worktree remove` | already guarded (§2.3) | unchanged | unchanged |

Two of these rows are deliberate behavior changes and must be called
out in the PR body for stage-3 review:

- **fetch: abort → warn-and-continue.** Mandated by "floor check
  reachable on every path". Safety of deciding merges on stale refs:
  a stale `origin/main` tip has a *smaller* ancestor set, so
  `merge-base --is-ancestor HEAD origin/main` (line 53) under-approximates
  merges — removals can only become *fewer*, never wrongful; the
  HEAD-at-tip guard (line 49) likewise errs toward KEEP. Offline runs
  (a project goal — kiosk targets) currently cannot run the doctor at
  all; after the fix they get a full sweep with the warning and a
  truthful floor verdict.
- **df: abort → below-floor verdict.** Keeps contract clause 2
  absolute: the only reason exit is nonzero remains "floor not
  verified as met". The `free: ?` line makes the unmeasurability loud.

## 6. Implementation notes

- `set -euo pipefail` stays (dispatch stage-2 hard rule).
- The coverage-scrub loop (lines 73–78) must iterate the same
  porcelain-derived list — otherwise it keeps the flat-glob blindness
  the sweep just lost, and a nested worktree's 13–25G coverage tree
  would survive the scrub.
- The orphan cross-check (§3.3) runs after the sweep, before the
  scrub; it needs the registered set anyway, so derive it once.
- No new dependencies beyond git core (dispatch); `mapfile`, `comm`,
  `sed` are POSIX/bash builtins or coreutils already used elsewhere in
  `scripts/`.
- The doctor must never dirty the anchor tree (stage-2 gate:
  `git status --short` empty after every run) — nothing in this design
  writes into `$ROOT`.

## 7. Verification checklist (stage 2 executes for real; transcripts into the stage-2 report and PR body)

Build the hermetic lab once (commands below are the exact lab used for
§2.4; reuse it) — **cases 2–9 run there**, so fixture worktrees can
never collide with the real checkout's live worktrees or with a
concurrent agent's doctor run. Only cases 1 and 10 touch the real
checkout.

```sh
L=/tmp/doctor-lab; rm -rf "$L"; mkdir -p "$L"
git init --bare -q "$L/origin.git"
git clone -q "$L/origin.git" "$L/repo"
git -C "$L/repo" -c user.email=l@l -c user.name=l commit -q --allow-empty -m c1
git -C "$L/repo" push -q origin HEAD:refs/heads/main
```

The lab anchors with `WORKTREE_DOCTOR_ROOT="$L/repo"`. The real
checkout's `origin/main` history is never rewritten and nothing is
pushed anywhere (fixtures branch off already-pushed commits, so their
HEADs can be made legitimately "merged" without pushing anything new).

| # | case | setup | expected observable |
|---|---|---|---|
| 1 | **bogus sibling (the #395 repro)** | `mkdir /home/viktoria/src/taskboard-WT-bogus`, run doctor on the real checkout, `rm` after | exit **0** (above floor); a `WARN` line names the bogus dir as unregistered; `free:` line present; both live worktrees still get their KEEP lines. Today: exit 128, no `free:` (§2.1) |
| 2 | dirty worktree | lab: worktree off `main`, touch a file | `KEEP (uncommitted changes)` |
| 3 | unmerged HEAD | lab: worktree off `main`, empty commit | `KEEP (in flight)` |
| 4 | worktree on `main` | lab **only** (AGENTS §8 forbids on-main worktrees in the real checkout; the lab exercises the same guard): `git -C $L/repo worktree add "$L/repo-WT-onmain" main` | `KEEP (no branch or primary branch)` |
| 5 | below floor | `WORKTREE_DOCTOR_MIN_FREE_G=100000`, lab and real checkout | exit nonzero; last line is the floor verdict; **repeat with a bogus sibling present** — still the floor exit, never 128 |
| 6 | detached HEAD | lab: `worktree add --detach` | `KEEP (no branch or primary branch)`; porcelain `detached` record does not crash the parse |
| 7 | locked | lab: `worktree lock`, clean + merged setup | `KEEP` (remove-refused path) |
| 8 | prunable (registered, dir gone) | lab: `worktree add` then `rm -rf` the dir | `STALE (registered, dir gone)`; exit still floor-only |
| 9 | merged + clean → REMOVE | lab: worktree branched off the pushed `c1` (= `origin/main`'s history, HEAD ≠ tip), clean tree; verify with an old tip on `main` if needed | `REMOVE (HEAD merged)`; dir gone; exit 0 |
| 10 | real-checkout end-to-end | doctor on the real checkout, no bogus sibling | exit 0; the two live worktrees KEEPed; `git -C /home/viktoria/src/taskboard status --short` empty (doctor never dirties the tree) |

Extended (run if cheap, not acceptance-blocking): nested registered
worktree in the lab is swept (proves D1 point 3); worktree path with a
space round-trips; `WORKTREE_DOCTOR_ROOT` at a bare repo → empty sweep,
floor still decides; fetch failure (point the lab origin aside) →
`WARN` + floor verdict (row D3/fetch).

Gates: `bash -n scripts/worktree-doctor.sh`; `shellcheck` if available
(it is not installed on the dev box — record that in the report);
`bash -x` spot-trace of one sweep pass showing no unguarded `git -C
"$wt"` call. No cargo surface (dispatch §3) — §10's cargo gates apply
only insofar as the tree stays clean.

## 8. Risks

- **Porcelain parse drift** across git versions — low: format stable
  and additive since 2.7; the parse keys only on `worktree` lines and
  ignores everything else (§2.4). Stage 3 re-dumps porcelain on the
  dev box's git per dispatch §3.
- **fetch warn-and-continue is a semantic change** reviewers must
  consciously accept (it is what makes "floor check reachable on every
  path" true); the stale-refs safety argument (removals can only get
  *fewer*) is in §5 and must be quoted in the PR body.
- **df → below-floor mapping** could mask a measurement bug as a disk
  problem — mitigated by the loud `free: ?` diagnostic; the caller's
  STOP behavior is identical either way.
- **Concurrent agents**: the fix changes what a *concurrent* doctor
  run does to neighbors only by making it *more* conservative (plain
  dirs no longer abort it mid-sweep; nothing new is removable). The
  two live worktrees observed at planning time (§2.2) must still KEEP
  after the fix — case 10 asserts it.
- **Regression surface is small but the contract is absolute**: the
  stage-3 hunt ("any nonzero that isn't the floor reintroduces #395")
  should re-run checklist case 5 *with* the bogus sibling — that
  combination is the chunk's whole point.

## 9. Estimated diff

- `scripts/worktree-doctor.sh`: ~50–70 lines changed of an 83-line
  file (enumeration, containment, scrub list, orphan warning) →
  ~115 lines after.
- `AGENTS.md`: one bullet, ~6 lines.
- This plan: docs, already written.
- No Rust surface; no Cargo.lock change; no changelog entry at this
  stage (the dispatch's docs/changelog round owns CHANGELOG promotion).

## 10. Stage-2/3 handoff

- Stage 2 implements §§3–6, runs §7's checklist for real, pastes
  transcripts, and flags any deviation from D1/D2/D3 with reasons.
- Stage 3 re-verifies per dispatch §3 prompt 2.3: the five core cases,
  the porcelain-shape traps (`bare`/`detached`/`prunable`), the AGENTS
  §8 wording against the active-worktree bullets, and the exit-code
  contract (§5) as THE regression target. Handoff at "PR open + `gh
  pr checks` green"; the user merges.
