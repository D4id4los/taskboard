# Worktree-doctor never-pushed (#450) — stage-2 implementation report

- **Date**: 2026-11-01
- **Chunk**: 1.5 of backlog round 2; branch `fix/worktree-doctor-never-pushed`
  (base `main` @ `5bf90c63`), worktree
  `../taskboard-WT-fix-worktree-doctor-never-pushed`
- **Implementation commit**: `f583d46a` "fix(scripts): the doctor spares
  never-pushed branches" — `scripts/worktree-doctor.sh` only (+22/−1)
- **Files changed**: exactly `scripts/worktree-doctor.sh` (verified via
  `git diff --stat`). No AGENTS.md, no CHANGELOG, no other files.
- **Handoff state**: per the stage-2 prompt, **no push, no PR** — the
  branch carries the stage-1 plan commit (`ff16bf71`) and the
  implementation commit (`f583d46a`).

## 1. What was implemented

Per plan §3.1 verbatim:

- Inside the existing containment branch, the single `KEEP (in flight)`
  line became a two-way label: an existing
  `refs/remotes/origin/<branch>` (checked with
  `git show-ref --verify --quiet`, local refs only) keeps the
  `KEEP (in flight)` label; an absent ref prints the new
  `KEEP (never pushed — local-only branch)` label. Both arms
  KEEP + `continue`; the containment check is the existing
  `git merge-base --is-ancestor` call, reused, not duplicated.
- Header gains exactly the three planned additions: the never-pushed
  spare in "What it does" item 1, the D2 false-positive stance
  paragraph, the D4 known-limitation paragraph (both verbatim from the
  plan).

Untouched, verified by reading the final diff: `set -euo pipefail`,
porcelain enumeration, tip guard, dirty/unmerged/on-main/locked KEEP
paths, liveness re-check, remove-refusal path, coverage scrub, orphan
WARN, fetch containment, floor test. The floor check remains the last
statement and the only nonzero exit (D5). No network call was added —
the only `fetch` is the pre-existing sweep-start one.

## 2. Verification transcripts

### 2.1 Hermetic lab (plan §5)

Setup per the plan, plus fixture drift noted in §3 below. Lab lived in
`/tmp/doctor-lab` (tmpfs, 16G — see §3 note on floors); lab torn down
afterwards (`rm -rf`, confirmed no `/tmp` leftovers).

Matrix rows (single run, floor knob at default 25G; `exit=1` is the
D5 floor exit — 12G free on tmpfs, see §3):

```
KEEP   (never pushed — local-only branch): /tmp/doctor-lab/wt spaced   (carried: space-in-path round-trip)
KEEP   (HEAD at origin/main tip — fresh or resume): /tmp/doctor-lab/wt-b1   (row 1)
REMOVE (HEAD merged):         /tmp/doctor-lab/wt-b2                    (row 2 — PINNED residual, D4)
KEEP   (never pushed — local-only branch): /tmp/doctor-lab/wt-b3        (row 3 — the new label)
KEEP   (in flight):           /tmp/doctor-lab/wt-b4                    (rows 4/9 — pushed, ref alive)
REMOVE (HEAD merged):         /tmp/doctor-lab/wt-b5                    (row 6 — pruned ref, contained ⇒ REMOVE, NOT never-pushed: stage-3 trap passes)
REMOVE (HEAD merged):         /tmp/doctor-lab/wt-b5r                   (row 5 — ref alive after merge ⇒ REMOVE)
KEEP   (never pushed — local-only branch): /tmp/doctor-lab/wt-b7        (row 7 — pruned ref, NOT contained ⇒ never-pushed: trap's "unless" arm)
KEEP   (uncommitted changes): /tmp/doctor-lab/wt-b8                    (row 8)
KEEP   (no branch or primary branch): /tmp/doctor-lab/wt-detached       (carried)
KEEP   (remove refused — likely locked or in use): /tmp/doctor-lab/wt-locked (carried)
KEEP   (no branch or primary branch): /tmp/doctor-lab/wt-main           (row 10)
STALE (registered, dir gone): /tmp/doctor-lab/wt-prunable/inner        (carried)
SKIP  (unreadable registered worktree): /tmp/doctor-lab/wt-unreadable/wt (carried)
free: 12G (floor: 25G)
exit=1
```

Post-run `git worktree list` confirmed wt-b2/wt-b5/wt-b5r were actually
removed and every other fixture survived.

Row 11 (below floor, lab) via the script's own knob
`WORKTREE_DOCTOR_MIN_FREE_G=100000`:

```
free: 12G (floor: 100000G)
exit=1
```

Row 11 inverse + row 14 (exit-contract sweep): same lab, knob=10 →
`free: 12G (floor: 10G)`, `exit=0`, all KEEP lines unchanged. Every
observed exit across the whole matrix was 0 or the floor exit — nothing
else (D5 holds).

`bash -x` spot-trace (plan gate): per never-pushed worktree exactly one
`git merge-base --is-ancestor` and one
`git show-ref --verify --quiet refs/remotes/origin/<branch>`; excerpt:

```
+ git -C /tmp/doctor-lab/repo merge-base --is-ancestor e84da43… a6ad361…
+ git -C /tmp/doctor-lab/repo show-ref --verify --quiet refs/remotes/origin/b3
+ echo 'KEEP   (never pushed — local-only branch): /tmp/doctor-lab/wt-b3'
```

Network-binary scan over the full trace: 0 hits; the only `fetch` in
the trace is the pre-existing sweep-start one.

### 2.2 Real checkout

Row 12 (bogus `taskboard-WT-bogus` plain dir created next to the real
checkout, then `rm`'d — the 2026-10-25/#395 class):

```
KEEP   (never pushed — local-only branch): …taskboard-WT-fix-anchor-filename-fallback
KEEP   (never pushed — local-only branch): …taskboard-WT-fix-worktree-doctor-never-pushed
KEEP   (never pushed — local-only branch): …taskboard-WT-perf-ui-unchanged-refresh-guard
KEEP   (uncommitted changes): …taskboard-WT-refactor-scheduled-phrase-typed-direction
WARN  not a registered worktree (left alone): …taskboard-WT-bogus
free: 204G (floor: 25G)
exit=0
```

Floor verdict reached and bogus sibling only warned about. (Note the
three sibling branches legitimately carry the new label: agents in this
repo do not push without explicit instruction, so their branches have
no `origin/<branch>` ref; their KEEP outcome is unchanged — plan §3's
"honest behavioral note".)

Row 11 (real checkout, knob=100000): `free: 205G (floor: 100000G)`,
`exit=1`, floor verdict last line.

Row 13: `git status --short` after every real-checkout run showed only
the pre-existing `?? .zcodeignore` (present in the session's starting
snapshot) — the doctor dirtied nothing.

Gates: `bash -n` clean. `shellcheck`: **not installed** on this box
(recorded, same as the 2026-10-25 chunk). No Rust surface → no
cargo gates apply to this diff.

## 3. Deviations from the plan (all environmental, none behavioral)

1. **Lab floor**: the plan's lab expectations assume a floor-satisfiable
   filesystem; `/tmp` here is a 16G tmpfs (12G free), so default-floor
   lab runs end in the floor exit instead of 0. This is itself D5-compliant
   ("every exit is 0 or the floor exit"); exit-0 lab behavior was
   demonstrated with the script's own knob (`MIN_FREE_G=10`). Real-checkout
   rows ran at the default floor with exit 0 as planned (204–205G free).
2. **Fixture drift, fixed in-lab**: `origin/main` advancing past b1 and
   b5 during fixture construction moved them off their planned shapes
   (b1 no longer at tip; b5's HEAD *became* the tip, where the tip guard
   would have fired instead of REMOVE). Fixed by re-tipping b1 onto the
   new `origin/main`, adding a c3 commit on top, and adding a `b5r`
   twin (merged + ref alive) alongside b5 (merged + ref pruned). Both
   REMOVE rows then exercised as designed.
3. **`worktree add <path> main` needs `--force`** for a second checkout
   of `main` (git refuses by default); used in the lab only, per the
   plan's own "lab only" note for row 10.
4. **Space-in-path fixture**: modern git rejects spaces in *branch*
   names; the space went in the *path* only (`b-spaced` at
   `…/wt spaced`), which is what the carried row ("space-in-path
   round-trip") actually tests. It printed the new never-pushed label
   and round-tripped fine.
5. **Pre-existing `comm: not in sorted order` stderr warnings in the
   lab**: emitted identically by `main`'s script in the same lab
   (verified side-by-side), and the orphan-scan lines are untouched by
   this diff; they did not appear in real-checkout runs. Out of scope
   here — worth a future issue if it reproduces outside this lab.
6. **Cleanup method**: the lab is a disposable scratch repo in `/tmp`,
   so teardown was `rm -rf` of the whole lab dir (after `chmod -R u+rwX`
   to undo the unreadable-fixture's mode 000), not per-worktree
   `git worktree remove`; confirmed gone. No throwaway worktree was
   created in the real checkout; the bogus sibling was a plain dir,
   `rm`'d. `git worktree list` shows only the five legitimate
   worktrees, all pre-existing.

## 4. For stage 3

- Review the condition text against the trap: pruned ref + contained ⇒
  REMOVE (row 6); pruned ref + not contained ⇒ never-pushed label
  (row 7). §2.3 of the plan is the argument for why no REMOVE class
  changed.
- D5 is the regression target: exits observed were only 0 or the floor
  exit across all runs.
- AGENTS.md untouched; one file changed; no network call in the new
  path.
