# Worktree-doctor sweep robustness (#395) — stage-2 report

- **Date**: 2026-10-25
- **Chunk**: 2 of the backlog-triage dispatch (dispatch §3)
- **Branch**: `fix/worktree-doctor-sweep`, commit `032f181`
  (on top of stage-1's plan commit `95c0aa6`)
- **Inputs**: stage-1 plan
  `.artifacts/plans/2026-10-25-worktree-doctor-sweep-plan.md`;
  issue #395; AGENTS §8.
- **Deliverable**: `scripts/worktree-doctor.sh` rewritten per plan
  D1/D3 (porcelain enumeration + per-entry containment + literal
  floor-only exit contract), AGENTS §8 bullet per D2. No push, no PR
  (per dispatch stage-2 scope; stage 3 ships).

## 1. Deviations from the plan

None of substance. Three spelling-level notes:

1. `merge-base --is-ancestor "$head_commit" "$origin_main"` compares
   against the once-resolved SHA instead of the `origin/main` ref
   (plan's original line 53 form) — required so the report-only mode
   (D3 row 4) holds when the ref is unresolvable. Identical behavior
   when fetch succeeds.
2. The df-failure diagnostic prints `free: ? (floor: NG)` with the
   real floor number and a trailing explanation, where the plan's
   table wrote `free: ? (floor: NG)` — the number, not the literal
   characters "NG". Intent identical.
3. The lab used an extra plain **file** `repo-WT-bogus2` alongside the
   plain directory: the old glob matched files too (`[ -d ]` skipped
   them), the orphan WARN now surfaces them as well. Bonus coverage,
   not a plan case.

## 2. Gates

- `bash -n scripts/worktree-doctor.sh` — clean.
- `shellcheck` — **not installed** on the dev box (plan §2.4 predicted
  this; dispatch acceptance is "shellcheck (if configured)").
- `bash -x` spot trace of one sweep pass (below, §3.12) — every
  per-entry `git -C "$wt"` call is inside an `if !` assignment; no
  unguarded abort candidates remain before the final floor test.
- Pre-commit hooks ran `cargo fmt` and `cargo clippy --workspace
  --all-targets -- -D warnings` — clean (no cargo surface touched).
- Tree cleanliness: `git status --short` empty in the branch worktree
  and unchanged in the main checkout (only the pre-existing untracked
  `.zcodeignore`) after every doctor run — the doctor never dirties
  the tree.

## 3. Verification transcripts

Lab build (plan §7) at `/tmp/doctor-lab` — note `/tmp` is a separate
filesystem with ~15G free, which made the below-floor cases natural;
the real checkout sits on a filesystem with ~96G free.

### 3.1 Full lab sweep (cases 2, 3, 4, 6, 7, 8, 9 + extended nested case at once)

Fixtures: dirty (uncommitted file), inflight (unmerged empty commit),
onmain (worktree on `main`), detached, locked, stale (dir deleted),
donewt (clean + HEAD ancestor of origin/main tip, branched off pushed
c1), nested (registered at `/tmp/doctor-lab/nested/repo-WT-feat-nested`
— a nested layout the flat glob never saw), plus bogus plain dir/file.

```
$ WORKTREE_DOCTOR_ROOT=/tmp/doctor-lab/repo bash .../worktree-doctor.sh; echo "doctor exit: $?"
KEEP   (HEAD at origin/main tip — fresh or resume): /tmp/doctor-lab/nested/repo-WT-feat-nested
KEEP   (no branch or primary branch): /tmp/doctor-lab/repo-WT-detached
KEEP   (uncommitted changes): /tmp/doctor-lab/repo-WT-dirty
REMOVE (HEAD merged):         /tmp/doctor-lab/repo-WT-donewt
KEEP   (in flight):           /tmp/doctor-lab/repo-WT-inflight
KEEP   (remove refused — likely locked or in use): /tmp/doctor-lab/repo-WT-locked
KEEP   (no branch or primary branch): /tmp/doctor-lab/repo-WT-onmain
STALE (registered, dir gone): /tmp/doctor-lab/repo-WT-stale
WARN  not a registered worktree (left alone): /tmp/doctor-lab/repo-WT-bogus
WARN  not a registered worktree (left alone): /tmp/doctor-lab/repo-WT-bogus2
free: 15G (floor: 25G)
doctor exit: 1
```

Every KEEP predicate fired correctly; the one legitimately removable
worktree (donewt) was REMOVEd; the nested registered worktree was
swept (D1 point 3 proven); `repo-WT-bogus2` is a plain file — warned,
untouched. Exit 1 is the floor verdict alone (15G < 25G), not a crash.

### 3.2 Case 5: below floor via the script's own knob, with and without bogus sibling

```
$ WORKTREE_DOCTOR_ROOT=/tmp/doctor-lab/repo WORKTREE_DOCTOR_MIN_FREE_G=100000 bash .../worktree-doctor.sh; echo "doctor exit: $?"
[same KEEP/STALE/WARN lines as 3.1, donewt already removed]
free: 15G (floor: 100000G)
doctor exit: 1
```

Repeated verbatim with the bogus sibling present — identical output,
exit 1 with the floor verdict as the last line. **Never 128**; the
combination of #395's trigger and a below-floor disk is the chunk's
whole point and it holds. Above-floor run (`MIN_FREE_G=10`): exit 0.

### 3.3 Case 1: bogus sibling on the real checkout (#395 repro, before/after)

Before (old script still on the main checkout):

```
$ mkdir /home/viktoria/src/taskboard-WT-bogus
$ bash /home/viktoria/src/taskboard/scripts/worktree-doctor.sh
fatal: not a git repository (or any of the parent directories): .git
doctor exit: 128
```

After (fixed script, `WORKTREE_DOCTOR_ROOT=/home/viktoria/src/taskboard`):

```
$ bash .../worktree-doctor.sh; echo "doctor exit: $?"
KEEP   (in flight):           /home/viktoria/src/taskboard-WT-docs-reminder-rule-sync-design-round
KEEP   (HEAD at origin/main tip — fresh or resume): /home/viktoria/src/taskboard-WT-feat-ui-r2-drag-to-move
KEEP   (uncommitted changes): /home/viktoria/src/taskboard-WT-fix-worktree-doctor-sweep
WARN  not a registered worktree (left alone): /home/viktoria/src/taskboard-WT-bogus
free: 96G (floor: 25G)
doctor exit: 0
```

Exit 0 above floor, bogus dir named and left alone, all three live
worktrees KEEPed (including this chunk's own worktree, whose
uncommitted stage-2 changes were correctly honored).

### 3.4 Case 10: real checkout end-to-end, no bogus sibling

```
$ WORKTREE_DOCTOR_ROOT=/home/viktoria/src/taskboard bash .../worktree-doctor.sh; echo "doctor exit: $?"
KEEP   (in flight):           /home/viktoria/src/taskboard-WT-docs-reminder-rule-sync-design-round
KEEP   (HEAD at origin/main tip — fresh or resume): /home/viktoria/src/taskboard-WT-feat-ui-r2-drag-to-move
KEEP   (uncommitted changes): /home/viktoria/src/taskboard-WT-fix-worktree-doctor-sweep
free: 96G (floor: 25G)
doctor exit: 0
$ git -C /home/viktoria/src/taskboard status --short
?? .zcodeignore          ← pre-existing, untouched by the doctor
```

### 3.5 Extended: bare anchor

```
$ WORKTREE_DOCTOR_ROOT=/tmp/doctor-lab/origin.git WORKTREE_DOCTOR_MIN_FREE_G=10 bash .../worktree-doctor.sh; echo "doctor exit: $?"
WARN  fetch failed — merge decisions use stale origin/main
WARN  origin/main unresolvable — sweep is report-only
free: 15G (floor: 10G)
doctor exit: 0
```

Bare-record porcelain shape handled: empty sweep list (anchor only),
both WARNs fire, floor decides. (The bare lab origin has no `main`
ref, hence both containment rows exercised at once.)

### 3.6 Extended: fetch failure (origin pointed aside)

```
$ git -C /tmp/doctor-lab/repo remote set-url origin /tmp/doctor-lab/gone.git
$ WORKTREE_DOCTOR_ROOT=/tmp/doctor-lab/repo WORKTREE_DOCTOR_MIN_FREE_G=10 bash .../worktree-doctor.sh; echo "doctor exit: $?"
WARN  fetch failed — merge decisions use stale origin/main
[full sweep proceeds, same KEEP/STALE/WARN lines as 3.1]
free: 15G (floor: 10G)
doctor exit: 0
```

D3's fetch row verified: offline runs now get a full sweep with the
warning and a truthful floor verdict instead of an abort.

### 3.7 Extended: worktree path with a space

```
$ git -C /tmp/doctor-lab/repo worktree add -q -b spacey "/tmp/doctor-lab/repo-WT-with space"
$ ... WORKTREE_DOCTOR_MIN_FREE_G=10 bash .../worktree-doctor.sh | grep space
KEEP   (HEAD at origin/main tip — fresh or resume): /tmp/doctor-lab/repo-WT-with space
[full run exit 0]
```

Space round-trips through the porcelain parse (sed rest-of-line +
mapfile).

### 3.8 bash -x spot trace (per-entry calls all guarded)

```
+ branch=feat/nested
+ status=
+ head_commit=084e0a0ec9fbbe8e04b68a172978f54529da3363
+ branch=
+ branch=dirty
+ status='?? f.txt'
+ branch=inflight
+ status=
+ head_commit=4063a2c6b30759fe7a7d5449ef1203f7a262a20a
+ branch=lockme
...
```

Every `branch=`/`status=`/`head_commit=` assignment appears as
`+ var=$(...)` inside an `if !` construct; the detached record's empty
`branch=` flowed into the no-branch KEEP predicate, not into an error.

## 4. Handoff to stage 3

- Re-verify per dispatch prompt 2.3: the five core cases, the
  bare/detached/prunable porcelain shapes, the AGENTS §8 wording
  against the active-worktree bullets, and the exit-code contract
  (§5 of the plan) as THE regression target. Checklist case 5 *with*
  the bogus sibling (§3.2 here) is the combination to repeat.
- PR body should quote D3's two deliberate behavior changes (fetch
  warn-and-continue with the stale-refs under-approximation safety
  argument; df → below-floor verdict) and lift §3.1–§3.4 transcripts.
