# Scheduled CI lanes — third triage: diagnosis & fix plan (2026-11-28)

Lane triage round 3 for #1479 (miri) and #1480 (mutation testing).
Evidence run: miri 46079491657, mutation 46079495348 (both dispatched
2026-11-27 22:55 UTC after PR #1551).

## Lane 1 — miri (#1479): timeout shape again, both core jobs

### Root cause (measured from run 46079491657)

Both core jobs ran 22:55:18 → 00:55:33 and were cancelled exactly at the
120-min job cap, inside their `Run Miri …` steps. The `vocabulary_guards`
failure fixed by #1551 is gone. The 526be42a two-way core re-slice is
simply too coarse: each job was ~72% done when the cap hit.

Because libtest ran the modules strictly sequentially in these runs,
module spans are additive and give a direct per-module measurement:

Behavioural job (535 of 742 tests done at kill):

| module span | duration |
|---|---|
| start → first command test (calibration etc.) | ~9 min |
| command:: (256 tests, incl. 400–960 s proptests) | 28 min |
| days:: (53) | 24 min |
| find:: (14) | 31 min |
| habit:: (68, incl. the new habit_event derivations) | 10.5 min |
| recur:: (67) | 4.5 min |
| reflection:: partial (25/49, killed mid-module at 00:55) | ~8 min |
| **never started:** reminders(108), tracking(26), settings(20), tracking_stats(10), reflection_stats(9), roster(4), time(3), test_support(4) | — |

Data job (390 of 572 tests seen; last ok `persistence::tests::persisted_state_maps_into_app_state` 00:42:16, then 13 min of silence on the in-flight test until the kill):

| module span | duration |
|---|---|
| merge(169)+appdata(90)+journal(20)+entities(18)+habit_event(16)+ids(15)+messages/outbox/capabilities/ownership/idgen | **~4.5 min** (22:57:54→23:02:18) |
| board_move:: (11 tests) | **54 min** (23:02→23:56; `a_full_rekey_leaves_no_old_id_and_no_dangling_reference` alone 1047 s, `rewriting_nothing_is_lossless` 783 s) |
| persistence:: (20 of 41 done) | 46 min (~2.3 min/test: gaps 402–529 s per proptest) |
| **never started:** undo(125), push(28), reconcile(19) | — |

Quoted kill evidence (both jobs, post-cancel orphan cleanup):
`Terminate orphan process: pid (3808) (miri)` at 00:55:32, i.e. the
runner killed the interpreter at `timeout-minutes: 120`
(`.github/workflows/miri.yml`, both core jobs).

### Fix (compressed, branch `ci/scheduled-lanes-triage-3`)

Re-sliced the two core jobs into seven, on the measured numbers above;
cap NOT raised; PROPTEST_CASES stays 16 everywhere (`.github/workflows/miri.yml`):

| job | filter | projected |
|---|---|---|
| core: command | `command::` | ~28 min (measured) |
| core: days/find | `days:: find:: clock:: calibration::` | ~58 min (24+31+3 measured) |
| core: behavioural | `reminders:: recur:: habit:: habit_time:: tracking:: tracking_stats:: reflection:: reflection_stats:: focus:: roster:: settings:: time:: order::` | ~40–60 min (measured parts + unmeasured unit-heavy remainder) |
| core: data/substrate | skip-complement of everything above **plus** `board_move:: persistence:: undo:: push:: reconcile::` | ~5 min (measured) |
| core: board_move | `board_move::` | ~54 min (measured) |
| core: persistence | `persistence::` | ~94 min (2.3 min/test × 41 — tightest job; lane-watch reports drift) |
| core: undo substrate | `undo:: push:: reconcile::` | unmeasured (172 tests; sibling merge module = 169 tests in ~4.5 min suggests cheap) |

Completeness/disjointness was verified mechanically against
`cargo test -p taskboard-domain --lib --tests -- --list` (1474 tests):
256/85/401/11/41/172 to the six include-list jobs, 348 to the data
complement, zero cross-job duplicates; the only tests outside the core
slice are `pipeline::`/`remote::`/`state::`, owned by the three
unchanged jobs. The disjoint-and-complete doctrine is preserved: the
data job stays the exact complement (by skip-list), so a newly added
data module still lands in exactly one job.

Commit: `ci(miri): re-slice the domain-core jobs on the run-46079491657 measurement`.

### Verification

Dispatcher dispatches `miri`. Green = all 10 jobs conclude `success`
within 120 min each; the measurement to watch: `core: persistence`
(projected ~94 min) and `core: undo substrate` (unmeasured). If either
drifts past the cap, move tests between filter lists only — do not
raise the cap.

## Lane 2 — mutation testing (#1480): baseline build exit, again

### Root cause (measured from run 46079495348)

Not a cap-kill and not surviving mutants — the #1551 `libfontconfig1-dev`
fix cured fontconfig but the baseline then died one dependency earlier,
~2–4 min in, in all four non-UI shards (identical in each):

```
2026-11-27T22:59:25Z FAILED  Unmutated baseline in 193s build
error: failed to run custom build command for `alsa-sys v0.4.0`
  thread 'main' panicked at alsa-sys-0.4.0/build.rs:13:19:
  pkg-config exited with status code 1
  > pkg-config --libs --cflags alsa
  Package alsa was not found in the pkg-config search path.
2026-11-27T22:59:26Z ERROR cargo build failed in an unmutated tree, so no mutants were tested
##[error]Process completed with exit code 4.
```

(shard timings: app-pdf 98 s, domain-data 122 s, state 149 s,
domain-scheduling 193 s baseline attempts.)

Why: cargo-mutants' unmutated baseline compiles the workspace test
closure (`cargo test --no-run --package=taskboard-app …`), which reaches
taskboard-app → cpal/alsa-sys and the slint GL/EGL/xkbcommon/wayland
chain. The two non-UI apt steps
(`.github/workflows/mutation-testing.yml`, mutants matrix job and
`app-pdf-commands`) install only `lld libdbus-1-dev pkg-config
libfontconfig1-dev`; ci.yml's lane 1 installs the full set. The
`ui-pure-modules` job (which installs GL/EGL/xkb/wayland but not alsa)
got past its baseline and was genuinely mutating — still in progress at
diagnosis time (~4 h in, its own 240-min cap applies; that job is NOT
part of this failure class).

### Fix (compressed, same branch)

Copy ci.yml lane 1's package list verbatim
(`lld libdbus-1-dev libasound2-dev pkg-config libgl1-mesa-dev
libegl1-mesa-dev libxkbcommon-dev libwayland-dev libfontconfig1-dev`)
into both non-UI apt steps. `ui-pure-modules` untouched (its baseline
passes; ui-slint does not pull cpal/alsa).

Commit: `ci(mutation-testing): install the full lane-1 system dep set in both non-UI shards`.

### Verification

Dispatcher dispatches `mutation-testing`. Green = the four non-UI
shards get past the baseline build into actual mutation testing
(`Found N mutants to test` followed by per-mutant progress, runtimes in
tens of minutes to hours, not minutes). Survivor-shaped exits stay a
green report outcome per the in-file contract; a loud `failure` after
hours is a cap-kill or genuine finding, triaged separately. NOTE:
after the baseline passes for the first time, surviving-mutant
outcomes will surface for the first time on this surface — that is the
loop working, not a lane defect; the accept/file decision is the
operator's.

## Operator rulings needed

1. **#1480 baseline acceptance**: once the dep fix lands, the first
   clean run will report the real surviving-mutant baseline. Accepting
   that baseline (or filing survivors) is an operator decision — not
   made here.
2. **#1479 persistence headroom**: `core: persistence` projects ~94 min
   against the 120-min cap (~22% headroom). If the first post-merge run
   shows drift, the options are moving persistence tests between filter
   lists or a per-job PROPTEST_CASES reduction (a budget statement, as
   with the pipeline job's 2) — operator's call, per the measured data.
3. **ui-pure-modules shard duration**: still running ~4 h at diagnosis
   time against its 240-min cap. If it cap-kills, that is a new
   sizing finding for a follow-up triage (not addressed here).

## Stage-3 handoff

Stage 2 (this agent) implemented both fixes and stops here: no push, no
PR, no workflow dispatch. The dispatcher verifies landed state, runs the
full AGENTS §10 suite, dispatches stage 3 with the prompt below.

### Paste-ready stage-3 REVIEW prompt (verbatim)

> You are the stage-3 REVIEW agent for chunk "scheduled CI lanes — third
> triage" in the taskboard repo (D4id4los/taskboard), branch
> `ci/scheduled-lanes-triage-3`, worktree
> `../taskboard-WT-ci-scheduled-lanes-triage-3`. Scope: review the
> landed commits, push the branch, open ONE PR to `main`, report. You do
> NOT dispatch workflows; the dispatcher owns `gh workflow run`,
> `gh pr checks --watch`, mergeStateStatus CLEAN, teardown, and
> at-merge issue closures.
>
> **Discipline (binding — docs/agent_dispatch.org; restated verbatim)**:
> A subagent is killed after ~600s without observable activity.
> **Long-command discipline**: (1) *The coverage trio never runs inside
> a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks
> (automatic) plus `scripts/run-quality-gates.sh cargo nextest run
> --workspace` (flock-safe; background + poll with useful work between
> polls if needed); the dispatcher runs the full AGENTS §10 suite
> between stage 2's landing and stage 3's ship. *Flock-deferral rule:
> when the gate lock is contended and no other useful work remains,
> never block — commit all landed work, report "gate deferred: lock
> held by <holder>", and end the stage; the dispatcher runs the gate.*
> (2) *Stage 3's scope ends at PR-open*: push, open the PR, report —
> the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
> teardown, at-merge issue closures, and every `gh workflow run`.
> (3) *Any command expected over ~5 minutes runs backgrounded* with
> short interleaved polls and useful work between polls — commit all
> landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp.
> (4) **Every agent-launched cargo invocation routes through
> `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate
> this discipline verbatim.* **Never commit with `--no-verify` or any
> hook bypass.** Never let a test or binary you run touch the user's
> real taskboard data. Out-of-scope findings become `backlog:` GitHub
> issues (or comments) immediately (AGENTS §7; search first).
>
> Review inputs: `.artifacts/plans/2026-11-28-scheduled-lanes-triage-3-plan.md`
> (this branch), issues #1479/#1480, the two changed workflow files only
> (`.github/workflows/miri.yml`, `.github/workflows/mutation-testing.yml`).
> The change is YAML-only: verify the matrix/filter-list edits against
> the plan's measured split table, confirm YAML validity, confirm no
> source files changed, push, open the PR (`ci(...)` body quoting the
> run evidence), and report. Gate: pre-commit hooks (automatic) — the
> dispatcher owns the full suite.
