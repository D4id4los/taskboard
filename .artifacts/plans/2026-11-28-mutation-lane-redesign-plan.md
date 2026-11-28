# Mutation-Testing Lane Redesign — Implementation Plan & Record

- **Date**: 2026-11-28
- **Kind**: stage-1+2 compressed (design + implementation) chunk plan.
- **Origin**: issues #1480 (scheduled `mutation-testing.yml` red) and
  #1452 (split sweep cadence: weekly `--in-diff` delta vs complete
  sweep); the dispatcher's evidence comment on #1480 (run
  46117678610); the operator's 2026-11-28 ruling recorded on both
  issues (all three recommendations accepted; cap stays 240 min).
- **Scope**: `.github/workflows/mutation-testing.yml`, one helper
  script under `scripts/`, the accepted-mutant baseline data file.
  **No Rust code.**
- **Status**: implemented on branch `ci/mutation-lane-redesign`;
  stage 3 (review + PR) not started at the time of writing.

## 1. The operator's ruling (binding, 2026-11-28)

1. **Scope each shard to one crate** — a shard mutates and tests only
   what its name says.
2. **The weekly sweep becomes the `--in-diff` delta** of the previous
   complete sweep; **the complete sweep runs on demand
   (`workflow_dispatch`) and monthly**. The cap is NOT raised (240
   min per shard stays).
3. **The unkillable text-producing mutants are recorded as an accepted
   baseline**; a mutant on the accepted list does not fail the lane,
   a NEW survivor fails the lane loudly.

## 2. Root cause found in this chunk (new information)

The 13,563-mutant "domain" shards were **not a cargo-mutants
behaviour at all**: the matrix lane's run step referenced
`${{ matrix.shard.args }}`, but `matrix.shard` is the *string* value
of the `shard` matrix key (used successfully in the job `name:`), and
`matrix.shard.args` walks INTO that string, silently evaluates to the
empty string, and the executed command was literally
`cargo mutants  \ --in-place ...` — verified from the run 46117678610
step echo (`cargo mutants  \`), with every matrix shard reporting
`Found 13563 mutants to test`. The sibling key is `matrix.args`.
Every shard has therefore been sweeping the whole workspace (and
testing `-p taskboard-app` mutants) since the #1401 sharding landed.
The redesign removes the footgun entirely: shard arguments travel as
matrix-provided **env values** with a loud empty-guard, never as
interpolated shell text.

Comments filed on #1480 record this finding (standing authorization,
AGENTS §7).

## 3. Measured facts (the sizing basis)

Mutant counts: `cargo mutants --list` (no build) on
`origin/main` 97490154, this worktree, 2026-11-28:

| surface | mutants |
|---|---|
| workspace total | 13,563 |
| `-p taskboard-domain` | 3,460 (behavioural 17-file list 1,408 + data complement 2,052) |
| `-p taskboard-state` | 189 |
| `-p taskboard-ui-slint` total | 5,013 (pure 3-file window 1,725: presentation 916, view_state 796, resolver 13) |
| `-p taskboard-app` total | 3,214 (commands window 124) |
| not in the lane: sync-files 374, sync-nextcloud 530, storage-sqlite 280, sync-files-nextcloud 503 | — (backlog issue filed) |

Per-mutant cost, measured:

- **app window (CI, run 46117678610)**: 124 mutants in a 2h15m job ⇒
  ~62–80 s/mutant end-to-end (cargo check + rebuild + package-scoped
  test). This is the only complete-sweep measurement on a real
  surface.
- **ui window (CI, same run)**: unmutated baseline 443 s build + 99 s
  test; steady-state mutant ≈ 290 s build + ~98 s test with
  `CARGO_INCREMENTAL=0` (CI); two mutants observed (one 30-min
  outlier on the first mutant, then 6.3 min).
- **Local A/B of the rebuild lever** (this worktree, ui test binaries,
  `cargo nextest run --no-run -p taskboard-ui-slint` after touching
  `presentation.rs`): `CARGO_INCREMENTAL=1` ⇒ **79 s**, twice,
  reproduced; `CARGO_INCREMENTAL=0` ⇒ **282 s** (and the full cold
  build is 396 s). The CI runner's 290 s/mutant agrees with the local
  non-incremental figure — the CI 3.6x rebuild cost is exactly the
  toolchain action's `CARGO_INCREMENTAL=0`. **Every mutation job sets
  `CARGO_INCREMENTAL=1`** (step-level env overrides the toolchain
  action's job-level `0`; incremental artifacts live in the shared
  in-place `target/`, rust-cache handles the size).
- **Local, domain/state**: full domain+state test binary build 40 s;
  domain test run (1,514 tests incl. proptests) 38 s; state test run
  2 s.
- **Per-mutant test scope is already package-restricted by default**:
  verified in cargo-mutants 27.1.0 source (`options.rs`: with no
  `--test-workspace`/`--test-package` set, `TestPackages::Mutated`;
  `lab.rs`: `PackageSelection::Explicit(vec![mutant.source_file.package])`)
  and in the CI log (`test_packages=Explicit([Package { name:
  "taskboard-ui-slint" …`) — the redesigned lane pins
  `--test-workspace=false` explicitly to make the intent load-bearing
  and survive a default change upstream. `--test-tool nextest` is
  kept on every shard per the #1570 ruling: with package scope the
  ui shared-driver problem is gone, but nextest's fail-fast (kill
  confirmation stops at first failing test) and one-process-per-test
  isolation remain wins; the known caveat (nextest runs no doctests)
  is accepted — no doctest-only kills have been observed on the
  measured surfaces.

## 4. Per-shard design table (the arithmetic)

Assumptions, all named: per-shard **overhead 25 min** (checkout,
toolchain, rust-cache, cargo-mutants/nextest install, unmutated
baseline); **mutant budget = 240 − 25 = 215 min = 12,900 s**; per
crate per-mutant seconds are the conservative column above
(domain 90 s, state 45 s, ui 190 s = ~90 s incremental build + 98 s
test + check phase, app 80 s). Jobs are sized with headroom against
the cap, never as a guarantee — the dead-man's switch
(lane-watch.yml, matched by workflow *file* name, unchanged) reports
a shard that drifts past the cap. The first post-merge complete
dispatch is the true measurement; adjust slice counts by editing
ONLY the spec table in the workflow's matrix-setup step.

| shard family | scope | mutants | s/mutant | jobs (slices) | mutants/job | expected per job | vs 240 cap |
|---|---|---|---|---|---|---|---|
| domain-behavioural | `-p taskboard-domain` + 17 `--file` (scheduling/habit/reflection cluster) | 1,408 | 90 | 12 | ~117 | 117×90 s = 176 min + 25 = **201 min** | headroom 39 min |
| domain-data | `-p taskboard-domain` minus the 17 (exclude-list complement, as today) | 2,052 | 90 | 16 | ~128 | 128×90 s = 192 min + 25 = **217 min** | headroom 23 min |
| state | `-p taskboard-state` | 189 | 45 | 1 | 189 | 189×45 s = 142 min + 25 = **167 min** | headroom 73 min |
| ui-pure | `-p taskboard-ui-slint` + 3 pure-module `--file` | 1,725 | 190 | 28 | ~62 | 62×190 s = 196 min + baseline ~9 + setup ~5 = **210 min** | headroom 30 min |
| app-pdf | `-p taskboard-app` + commands `--file` window + `--features pdf-export` | 124 | 80 | 1 | 124 | 124×80 s = 165 min + 25 = **190 min** (measured 135 min) | headroom 50 min |

58 jobs in a monthly complete sweep. Weekly delta runs the same
matrix but every shard first runs the cheap `--list` pre-count with
`--in-diff`; a shard with zero diff-mutants exits after ~2 min
without building (see §6), so a typical weekly run costs minutes, not
hours. Slicing uses cargo-mutants' built-in `--shard k/n`
(`--sharding slice`, the default: consecutive source-order ranges —
maximises incremental reuse inside a slice). The k/n lists are
disjoint and complete by construction (verified in the tool source:
`shard.rs` `ensure!(k < n)`; `Sharding::Slice` "the first n/k on
shard 0, etc").

Why 28 ui slices can't be fewer: at the un-improved 390 s/mutant the
family needs ~50 jobs; incremental takes it to ~190 s and 28 jobs.
If the first complete dispatch measures slower than 190 s/mutant,
the honest moves are: raise the slice count in the spec table
(measured, like the miri re-slices) or file an operator decision —
the cap is never raised and a slice that would predictably exceed the
cap is never shipped (a planned cap-kill is a planned red lane).

## 5. Triggers, cadence, and the delta baseline mechanism

- Weekly `schedule` `0 4 * * 1` (Mondays 04:00 UTC, unchanged) ⇒
  **delta** scope.
- Monthly `schedule` `0 4 1 * *` ⇒ **complete** scope.
- `workflow_dispatch` input `scope` (`delta` | `complete`, default
  `delta`) — release-time manual sweeps dispatch `complete`.

**Baseline ref**: the complete sweep's `finalize` job (needs: all
shard jobs, only if all succeeded) pushes the swept commit to the
lightweight ref `refs/mutation-baseline/main` with the workflow
token (job-level `permissions: contents: write`; the ref is not a
branch, so branch protection does not apply). Every delta job
resolves the base the same way: fetch
`refs/mutation-baseline/main`; if the ref is missing (first run) the
job **falls back to complete** — first-run behaviour is a full sweep,
never a silent no-op. The diff is `git diff <base> HEAD > delta.diff`
(two-dot: the base is an ancestor by construction; merge-base
equals it). `--in-diff` semantics verified in the tool source
(`in_diff.rs`): a unified diff file; "Include only mutants in code
touched by this diff"; non-Rust changes are ignored;
`EmptyDiff`/`NoSourceFiles`/`NoMutants` are exit-Success conditions
with no mutants run. A week that touches no Rust (or no lane files)
therefore produces zero mutants per shard and every shard skips via
the pre-count — green, loud "0 mutants in delta" echoes.

Trade-off (why a ref and not an artifact): artifacts need run-id
lookup + retention management (30 days < possible scheduling skew; a
90-day artifact is a second thing to keep alive) and
`actions/download-artifact` cross-run plumbing in every shard; the
ref is one `git fetch` line, survives forever, and is exactly what
git diffs want. Failure mode of the ref approach: a deleted ref
re-runs one complete sweep — self-healing by the fallback.

## 6. Shard job flow (every matrix job)

1. **Resolve scope + diff** (delta only): fetch the baseline ref;
   missing ⇒ this job runs complete; write `delta.diff`; export
   `SCOPE`/`HAS_DIFF`.
2. **Pre-count** (cheap, no build): `cargo mutants --list --json
   $SHARD_ARGS --shard k/n [--in-diff delta.diff] | jq length`.
   `--list` honours both filters (verified in `main.rs`: in-diff
   filter → shard → list). Zero ⇒ loud skip: "0 mutants in delta for
   this shard" — the run, checker and upload steps are gated off;
   the job is green (nothing changed is a green fact, and the
   no-artifact-must-never-read-green law applies to *runs*, not to
   deliberately skipped shards).
3. **Run**: `cargo mutants $SHARD_ARGS --shard k/n [--in-diff
   delta.diff] --in-place --timeout-multiplier 2.0 --test-tool
   nextest --test-workspace=false` with `CARGO_INCREMENTAL=1`.
   `$SHARD_ARGS` arrives as a step **env** from the matrix with an
   empty-guard that hard-fails on the `matrix.shard.args` bug class
   (§2) — a silent empty expansion can never sweep the workspace
   again.
4. **Exit contract** (dead-man's switch preserved, `continue-on-error`
   stays absent): nonzero exit with no `mutants.out/missed.txt` ⇒
   infra failure, propagated (job red). A killed-by-cap job never
   returns ⇒ runner-failure, loud. Otherwise the **accepted-baseline
   checker** decides: `scripts/mutation-accepted-check.sh
   mutants.out/missed.txt scripts/mutation-accepted.txt` — exit 0
   (all survivors accepted) ⇒ green; exit 1 (≥1 NEW survivor, printed
   under a `::error::` annotation) ⇒ red, loudly. A new survivor is
   never absorbed.
5. **Collect + upload** (`if: always()`, only when the shard ran):
   logs + outcomes, artifact name
   `mutation-testing-report-<spec>-<k>of<n>-<run_id>`,
   `if-no-files-found: error` as today.

Apt sets split by measured need: the package-scoped baseline builds
only the mutated package's test closure, so the pure-crate shards
(domain ×2 families, state) need only `lld` (the `.cargo/config.toml`
lld link-arg) + `pkg-config`; the ui and app jobs keep the full
ci.yml lane-1 set (cpal/alsa + slint GL/EGL chain). This is not an
optimisation gamble: run 46079495348's failure chain was exactly the
workspace-wide baseline reaching alsa/fontconfig, and package scoping
removes those packages from the pure shards' build closure.

## 7. Accepted-baseline design

`scripts/mutation-accepted.txt`: one exact mutant name per line —
byte-equal to the `missed.txt` line, which is cargo-mutants
`mutant.name(true)` (verified `output.rs:221` writes exactly that,
and `--list` prints the same form) — `#` comments and blank lines
allowed; the reason and owning reference live in the trailing
comment on the same line. Initial contents (both text-class, ruling
3):

```
crates/taskboard-ui-slint/src/presentation.rs:110:5: replace files_sync_chip_caption -> &'static str with ""   # unkillable: caption constants are text under AGENTS §5 no-text-assertions; #1480
crates/taskboard-ui-slint/src/presentation.rs:110:5: replace files_sync_chip_caption -> &'static str with "xyzzy" # ditto; #1480
```

**The app window's two survivors are NOT accepted** — triage against
the mutated code (both read in this chunk):

- `crates/taskboard-app/src/commands/mod.rs:766` — deleting the
  `TargetNotInLine` arm in `CommandError::error_detail` falls through
  to `_ => unreachable!("guarded above")`, i.e. the mutant turns a
  graceful `None` detail into a **panic**. That is killable by a
  typed test routing a stale-row rejection through the detail path —
  a real coverage gap, filed as a `backlog:` issue and left OFF the
  accepted list. The lane reports it loudly until fixed, by design.
- `crates/taskboard-app/src/commands/mod.rs:2640` — `io.json |= json`
  in `TasksCmd::History`; `|=` → `&=` flips when `--json` mode is
  observable on `tasks history`. Killable by a structural assertion
  (parsed-JSON output on the flag path) — a real gap, filed, not
  accepted.

Consequence for the first post-merge complete dispatch: the app-pdf
shard is red with two named new survivors; everything else green.
That is the intended loop, not a lane defect.

Checker (`scripts/mutation-accepted-check.sh`): bash,
`set -euo pipefail`, shellcheck-clean; exits 0 all-accepted /
informational, 1 unaccepted survivor(s) (printed clearly as NEW),
2 malformed input (missing files, duplicate accepted lines, empty
line without comment), 64 usage; `--self-test` over mktemp fixtures
covering: empty missed.txt ⇒ 0; all-accepted ⇒ 0; one new survivor ⇒
1 with the NEW block; comment/blank parsing; duplicate detection;
missing file ⇒ 2. House `--self-test` precedent:
`scripts/ux-coverage.sh`.

Stale accepted entries (a mutant name on the list that no longer
appears in missed.txt) are reported informationally, never an error —
an accepted mutant that got fixed is good news, and pruning is
editorial.

## 8. Deliverables (commit order, one standalone commit each)

1. `docs(plan): the mutation-testing lane redesign (#1480, #1452)` —
   this file.
2. `ci(mutation-testing): scope shards per crate, delta/complete cadence, accepted-baseline gate (#1480, #1452)` — the workflow.
3. `ci(mutation-testing): the accepted-mutant baseline and its checker` — `scripts/mutation-accepted.txt` + `scripts/mutation-accepted-check.sh`.
4. `backlog:` issues for the two real app-window gaps (+ the lane-expansion and ui-cost-lever ideas), comments on #1480/#1452 with the root cause and the ruling implementation.

Stage 3 (REVIEW) does NOT push, open PRs or dispatch workflows beyond
what its prompt allows (PR-open only; every `gh workflow run` stays
with the dispatcher).

## 9. Stage-3 REVIEW prompt (paste-ready, verbatim)

You are the STAGE 3 (REVIEW) agent for the chunk "mutation-testing
lane redesign" in the taskboard repo (GitHub: D4id4los/taskboard):
issues #1480 (the scheduled lane `mutation-testing.yml` is red) and
#1452 (split the sweep cadence — a weekly `--in-diff` delta vs the
previous complete sweep). The design and implementation already
landed on branch `ci/mutation-lane-redesign` (stage 1+2 compressed);
you review it and open the PR. Read both issues in full with their
comments (`gh issue view N --repo D4id4los/taskboard --comments`) and
the plan/record `.artifacts/plans/2026-11-28-mutation-lane-redesign-plan.md`
— the operator's 2026-11-28 ruling recorded there is binding. You
change ONLY `.github/workflows/mutation-testing.yml`, the helper
script `scripts/mutation-accepted-check.sh`, the data file
`scripts/mutation-accepted.txt`, and the plan/record; no Rust code.

Your review mandate:

1. Validate the workflow YAML (parse it, e.g. with python3 yaml or
   actionlint if available) and walk every job end-to-end: the
   delta/complete trigger wiring, the baseline-ref fallback, the
   pre-count skip, the empty-guard on the shard args env, the
   survivor/infra exit contract, the checker wiring, artifact names
   and `if-no-files-found: error`.
2. Re-derive the sizing arithmetic in the plan's §4 table from the
   stated assumptions (mutants × s/mutant + overhead vs the 240-min
   cap) and from the mutant counts (re-run
   `scripts/memory-gate.sh --preflight cargo mutants --list --no-times
   -p <pkg>` per crate in the worktree — no build, cheap). Flag any
   slice whose expected time would predictably exceed the cap.
3. Check the checker script with its own `--self-test` plus at least
   one hand-run against a synthetic missed.txt (a new survivor must
   exit 1 and print; an all-accepted file must exit 0), verify it is
   bash `set -euo pipefail` and shellcheck-clean (run shellcheck if
   installed).
4. Verify every cargo-mutants flag used in the workflow against the
   pinned tool's own help (`cargo mutants --help`; the installed
   version is pinned `cargo-mutants@27.1.0`): `--package`,
   `--file`, `--exclude`, `--shard`, `--in-diff`, `--in-place`,
   `--test-tool`, `--test-workspace`, `--timeout-multiplier`,
   `--features`, `--list --json`. Confirm the §2 root-cause guard
   (no `${{ matrix.* }}` expansion reaches a shell line unprotected).
5. Re-check the accepted-baseline file's two entries against
   `cargo mutants --list` output byte-for-byte, and confirm the two
   app-window survivors are NOT on it and their `backlog:` issues
   exist and are referenced.

Discipline (binding — docs/agent_dispatch.org; restated verbatim): A
subagent is killed after ~600s without observable activity.
**Long-command discipline**: (1) *The coverage trio never runs inside
a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks
(automatic) plus `scripts/run-quality-gates.sh cargo nextest run
--workspace` (flock-safe; background + poll with useful work between
polls if needed); the dispatcher runs the full AGENTS §10 suite
between stage 2's landing and stage 3's ship. *Flock-deferral rule:
when the gate lock is contended and no other useful work remains,
never block — commit all landed work, report "gate deferred: lock
held by <holder>", and end the stage; the dispatcher runs the gate.*
(2) *Stage 3's scope ends at PR-open*: push, open the PR, report —
the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
teardown, at-merge issue closures, and EVERY `gh workflow run`.
(3) *Any command expected over ~5 minutes runs backgrounded* with
short interleaved polls and useful work between polls — commit all
landed work BEFORE any wait. Large scratch under /var/tmp, never
/tmp. (4) *Every agent-launched cargo invocation routes through
`scripts/memory-gate.sh`* — nothing bare (`cargo mutants --list`
included). (5) *Stage prompts restate this discipline verbatim.*
**Never commit with `--no-verify` or any hook bypass.** Never
dispatch a workflow. Out-of-scope findings become `backlog:` GitHub
issues (or comments) immediately (AGENTS §7; search first).

Environment: work in
`/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-ci-mutation-lane-redesign`
(existing worktree, branch `ci/mutation-lane-redesign`) by absolute
path; never modify the main checkout; never touch another
`taskboard-WT-*` directory. Fix findings on the branch as
`ci(mutation-testing): review fix: …` commits (or amend the plan
file for doc-level findings), then push and open ONE PR to `main`
(title `ci(mutation-testing): the lane redesign (#1480, #1452)`,
body: ruling summary, root cause §2, sizing table §4, what green
looks like, the two expected app-window red survivors and their
issues). Hand off at "PR open" and report the PR URL — do not watch
checks, do not merge, do not dispatch.

## 10. What the dispatcher must verify by dispatching after merge

1. Dispatch `mutation-testing` with `scope: complete`. **Green
   looks like**: every domain/state/ui slice passes its unmutated
   baseline and runs its slice (per-shard `Found N mutants` with N ≈
   mutants/job from §4); app-pdf red with exactly the two known
   survivors printed as NEW by the checker (their issues are filed);
   every job uploads its artifact. Also **measure** the actual
   per-slice wall times against the §4 expectations and adjust slice
   counts by editing ONLY the spec table.
2. Dispatch `scope: delta` afterwards: the finalize step has pushed
   `refs/mutation-baseline/main`, so delta shards whose files did not
   change skip loudly ("0 mutants in delta"); shards touching changed
   files run only those mutants.
3. The next Monday schedule run must read as delta; the 1st-of-month
   run as complete. lane-watch.yml (file-name matched) keeps
   reporting the lane without changes.
