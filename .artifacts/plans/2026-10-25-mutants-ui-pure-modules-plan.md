# [Plan] Mutants sweep extension — the UI pure modules (#427)

- **Date**: written 2026-10-26 (filename date 2026-10-25 fixed by dispatch
  prompt 8.1 — kept verbatim).
- **Kind**: stage-1 plan for chunk 8 of the backlog-triage dispatch
  (execution chunk: one workflow-file edit + one capped local validation
  run + one survivor-triage report; no crate code changes).
- **Origin**: issue #427 (normative); suitability report
  `.artifacts/reports/2026-10-24-type-driven-formal-methods-suitability-report.md`
  (§3 C8d, §5 Pilot 3, §7 ruling 7); dispatch plan
  `.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §1 (chunk
  table, freeze rule) and §9 (prompts 8.1–8.3); precedents #274
  (permanent-noise ignore-list mechanics) and #327 (box-command template
  + runtime expectations); AGENTS §5 (testing directives), §7 (issue
  conventions), §8 (worktree rules), §10 (gates).
- **Status**: READY — the GUI-r2 gate is satisfied (see §2), the workflow
  edit is inert for PRs, and the triage half is dispatchable now.

## 1. Measured facts (2026-10-26, origin/main @ 05a766b)

All numbers measured locally with `cargo mutants@<local>` `--list` /
`--list-files` against the worktree tree, or read off CI:

| Fact | Value | Source |
|---|---|---|
| Mutants in the three target files | **407** (`view_state.rs` 303, `presentation.rs` 96, `resolver.rs` 8) | `--list` grep, this tree |
| Mutants in the whole `taskboard-ui-slint` crate | 1139 | `--list` grep |
| Default mutation surface of `-p taskboard-ui-slint` | `src/**` only — `tests/` files are **never** mutated | `--list-files` (0 non-src entries) |
| Inline killing tests in the three files | `mod tests` at `resolver.rs:92` (6 `#[test]`, proptests), `view_state.rs:1555` (64 `#[test]`, no proptests), `presentation.rs:448` (21 `#[test]`, proptests) | grep |
| Per-mutant test cost (CI) | the **whole package suite** runs per mutant, including Tier-2 `tests/journeys.rs`, `golden_visuals.rs`, `usability_components.rs`, `keymap_snapshots.rs` | cargo-mutants semantics; #427's disproportionality warning |
| Current weekly lane runtime | 2 h 29 min for domain+state (run 38244799170, 2026-10-12); job `timeout-minutes: 240` | `gh run view` |
| Sync one-off runtime reference | 281 mutants ≈ 120 min (#327 box template) | #327 |
| Runner cores | CI `ubuntu-latest` = 4 vCPU; local box = 16 | `nproc` |
| System deps for ui-slint tests | full Mesa/EGL/xkbcommon/wayland/fontconfig stack (`ci.yml:34-37`); the mutation workflow today installs only `libdbus-1-dev pkg-config` | `ci.yml` vs `mutation-testing.yml` |
| cargo-mutants version contract | pinned `cargo-mutants@27.1.0` — the output-dir/report-name contract the Collect/Upload steps depend on | workflow comment |

## 2. GATE-note resolution: r2 has merged; chunk 8 is wave-1-eligible

The dispatch §1 GATE note required stage 1 to verify GUI-r2 merge state.
Evidence on `origin/main` as of 2026-10-26: the drag-to-move round (r2's
last design decision, D38) landed at `f7b1ac8`/`094b393`/`66b5a6b`, and
backlog-triage chunk 4's UiScale leg — itself gated on "r2 chunks 6–8
merging" — landed at `7cdfb10`/`a4fb793`. **Timing consequence**: the
triage half may run now; `view_state.rs` remains the hottest UI file, but
the sweep is weekly and re-measures by construction — survivor evidence
does not age unused the way a one-off would (#327's F14-02 inversion).

## 3. Scoping feasibility (prompt step 2)

**Yes — cargo-mutants 27.1.0 supports file-level scoping within a `-p`
target**, via (verified against the installed CLI's `--help` and the
repo's `--list` output):

- `-f/--file <GLOB>` — examine only matching files; globs containing a
  slash match the **entire workspace-root-relative path** (the exact
  format `--list`/`--list-files` print, e.g.
  `crates/taskboard-ui-slint/src/view_state.rs`). Exact paths with no
  wildcard characters act as exact matches.
- `-e/--exclude <GLOB>` — file globs (applied after `--file`).
- `-F/--examine-re`, `-E/--exclude-re` — regexes over the names `--list`
  prints.
- `--shard N/K` (slice sharding by default), `--list`, `--list-files`,
  `-j/--jobs`.

**The load-bearing trap**: `--file`/`--exclude` filters are **global
across all `-p` packages** in one invocation. Appending
`-p taskboard-ui-slint --file '…resolver.rs' …` to the existing
domain+state command would zero out every domain/state mutant (their
files match no `--file` glob). Two designs follow; D1 picks one.

## 4. Decisions

- **D1 — a separate workflow job, not `--file` inside the core run.**
  The core job's YAML stays byte-identical (strongest possible revert
  story: revert = delete the appended lines); each job gets its own
  `mutants.out/` workspace, its own artifact, its own timeout, and the
  two lanes run **in parallel on separate runners** so the weekly
  workflow's wall-clock does not stack. Rejected alternative (Design A):
  one invocation over `-p domain -p state -p ui-slint` with `--file`
  globs re-enumerating the *entire* core surface
  (`crates/taskboard-domain/src/**` etc.) plus the three UI paths —
  semantically equivalent on paper, but it couples the lanes (any core
  file-layout change would silently need a glob edit) and turns the
  revert into surgery on a shared command line.
- **D2 — the UI job installs the full ci.yml system-dep stack** (D2 of
  §1: goldens/journeys link GL/EGL/xkbcommon/wayland/fontconfig; the
  current mutation lane's two-package apt line is not enough).
- **D3 — artifact naming**: `mutation-testing-report-ui-pure-${{
  github.run_id }}`; the core artifact name is untouched. The
  `if-no-files-found: error` upload contract is preserved — a silent
  no-artifact lane must never read green.
- **D4 — no test-surface narrowing** (no `--cargo-test-arg` filtering to
  skip journeys/goldens, no `--test-tool` change): narrowing the killing
  suite silently changes what "caught" means for every mutant; #427
  sanctions scope-by-*mutant-surface* only, with runtime
  disproportionality answered by **revert**, not by a weaker suite. If
  the lane proves valuable-but-slow, narrowing is a future maintainer
  decision, recorded as such — not a stage-2 improvisation.
- **D5 — no `.cargo/mutants.toml` changes in this chunk.** The config
  is workspace-global: an `exclude_re` entry must be path-anchored
  (e.g. `view_state\.rs:`) or it would silence mutants in the core
  crates too. Per #274's own history (verdicts recorded first, the
  ignore-list "rides the sweep-scope entry's fix"), a survivor class
  that recurs and is proven permanent noise earns a **follow-up issue**
  proposing the anchored entry — not an in-chunk config edit.
- **D6 — worktree branched off `origin/main` (05a766b), not the local
  `main` ref**: the local checkout was 7 commits behind `origin/main`
  when the worktree was created; passing `origin/main` as the start
  point satisfies dispatch §1 ("the then-current `main`") without
  moving the user's local `main`.
- **D7 — survivor report lands at
  `.artifacts/reports/YYYY-MM-DD-mutation-testing-ui-pure-modules-report.md`**
  (stage 2 stamps its run date), modelled on the sync-audit triage
  format (`.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md`
  §"Mutant-survivor triage — cluster-based"): every survivor accounted
  for in a named cluster, each cluster with a verdict and evidence
  anchors.
- **D8 — CHANGELOG**: untouched (workflow infra, not user-observable;
  dispatch §1 assigns CHANGELOG duty to chunks 1 and 9 only).

## 5. The workflow diff (exact)

Append to `.github/workflows/mutation-testing.yml` — nothing existing
is modified (schedule, concurrency, core job, artifact contract all
byte-identical). The `# v-pinned` comment records the version-pinned
`--file` glob-semantics assumption for future version bumps.

```yaml
  ui-pure-modules:
    name: cargo-mutants (ui pure modules)
    runs-on: ubuntu-latest
    timeout-minutes: 240
    steps:
      - uses: actions/checkout@v7

      - name: Install system deps (slint goldens/journeys link GL/EGL/wayland)
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update && sudo apt-get install -y \
            libdbus-1-dev pkg-config \
            libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev libwayland-dev \
            libfontconfig1-dev

      - uses: dtolnay/rust-toolchain@v1
        with:
          toolchain: stable

      - uses: Swatinem/rust-cache@v2

      - uses: taiki-e/install-action@v2
        with:
          # Same pin as the core lane: the output-dir/report-name contract.
          tool: cargo-mutants@27.1.0,nextest

      - name: Run Mutation Testing (ui pure modules)
        # Survivor classes are report outcomes, not infra failures —
        # mirrors the core lane's continue-on-error stance.
        # v-pinned: --file slash-globs match the whole workspace-relative
        # path in cargo-mutants 27.1.0; re-verify --list-files on bump.
        continue-on-error: true
        run: |
          cargo mutants \
            -p taskboard-ui-slint \
            --file 'crates/taskboard-ui-slint/src/resolver.rs' \
            --file 'crates/taskboard-ui-slint/src/view_state.rs' \
            --file 'crates/taskboard-ui-slint/src/presentation.rs' \
            --in-place --timeout-multiplier 2.0

      - name: Collect Surviving Mutants
        if: always()
        run: |
          if [ -d mutants.out ]; then
            cat mutants.out/*.log || true
            if [ -s mutants.out/missed.txt ]; then
              echo "Surviving mutants found — see artifacts."
            fi
          fi

      - name: Upload Mutation Report (ui pure modules)
        if: always()
        uses: actions/upload-artifact@v7
        with:
          name: mutation-testing-report-ui-pure-${{ github.run_id }}
          path: mutants.out/
          if-no-files-found: error
          retention-days: 30
```

Also amend the file's top comment block: one line — "*The ui-pure-modules
job sweeps the three pure presentation modules (#427); component-journey
tests stay out of scope by mutant-surface selection, not test
filtering.*"

**Sharded variant (ready, not applied by default)** — replaces the run
step's command and adds a strategy block to the job, only if the
validation ladder (§7) lands on rung 2/3:

```yaml
    strategy:
      fail-fast: false
      matrix:
        shard: [1, 2]        # or [1, 2, 3, 4] on rung 3
    steps:
      # … identical steps; the run command gains:
      #   --shard ${{ matrix.shard }}/2
      # and the artifact name gains the shard:
      #   name: mutation-testing-report-ui-pure-s{{ matrix.shard }}-${{ github.run_id }}
```

## 6. The local validation run (stage 2, capped)

Purpose: measure per-mutant wall cost **before** the weekly lane pays
(#427's own success criteria), and produce the survivor set for triage.

```sh
cd /home/viktoria/src/taskboard-WT-ci-mutants-ui-pure-modules

# 0. Refresh counts on the current tree (churn drifts them; §1 was 407).
cargo mutants -p taskboard-ui-slint --list \
  | grep -cE '^crates/taskboard-ui-slint/src/(resolver|view_state|presentation)\.rs'

# 1. Re-run the doctor — the slint build's target/ is tens of GB.
#    Nonzero exit (disk below floor) STOPs the chunk. (AGENTS §8)

# 2. Capped measurement run — box template (#327), CI-like parallelism:
#    -j4 emulates the 4-vCI runner so local wall ≈ CI wall.
timeout --signal=INT --kill-after=90s 7200 \
  env SQLX_OFFLINE=true cargo mutants \
    -p taskboard-ui-slint \
    --file 'crates/taskboard-ui-slint/src/resolver.rs' \
    --file 'crates/taskboard-ui-slint/src/view_state.rs' \
    --file 'crates/taskboard-ui-slint/src/presentation.rs' \
    -j4 --in-place --timeout-multiplier 2.0

# 3. Tree integrity after the --in-place run: untracked mutants.out/
#    and target/ are expected; NOT ONE tracked file may differ.
git status --short | grep -v '^??'   # must print nothing
```

Notes: `--baseline` stays at its default (`run`) — the unmutated suite
run doubles as the timeout calibration and a flake probe. `SQLX_OFFLINE`
is inherited from the #327 template (ui-slint has no sqlx in its
dependency closure, but the workspace metadata build does). The 7200 s
cap is the template's box value; if the run hits the cap, stage 2
extrapolates `W ≈ elapsed × M/M_done` (M = refreshed mutant count) and
marks the projection as extrapolated in the report.

## 7. Revert criterion and mechanics (hard numbers)

Let **W** = the measured (or extrapolated) wall-clock of the `-j4` capped
run for the full target set, **K** = shard count. The ladder:

| Rung | Condition | Action |
|---|---|---|
| 1 | `W ≤ 180 min` | ship the §5 diff unsharded (`timeout-minutes: 240`) |
| 2 | `W/2 ≤ 180 min` | ship the sharded variant with `shard: [1, 2]` |
| 3 | `W/4 ≤ 180 min` | ship the sharded variant with `shard: [1, 2, 3, 4]` |
| 4 | `W/4 > 180 min` | **revert**: disproportionate even at 4 shards |

Rationale for the 180-min ceiling: the job also pays checkout + apt +
rust-cache restore + baseline suite (~10–20 min warm), so 180 + 20 stays
under the 240-min lane timeout with margin; and since the two jobs run
on parallel runners, the weekly workflow's wall-clock stays pinned to
the core lane's ~2.5 h — the extension costs one extra runner-block per
week, proportional to a p3 audit lane.

**Revert mechanics** (per #427's second success criterion — the change
and its undo are both legitimate outcomes; an unmeasured merge is not):

- **Before PR open** (expected path): stage 2 does not apply the §5 diff
  (or removes it on the same branch); the report artefact records W, M,
  the ladder rung reached, and the scoping decision; #427 closes citing
  its own revert clause.
- **After merge** (only if the lane already ran): a new branch
  `ci/mutants-ui-pure-revert` deletes the appended job block; the PR
  body links the report section with the triggering numbers. Never a
  force-push, never a local main merge (AGENTS §8).

A secondary, **non-reverting** signal recorded in the report: if > 80%
of survivors fall in annotate-only classes (§8), the lane's marginal
information value is low — that number feeds the next quarterly review
but does not by itself revert anything (#427's criterion is runtime).

## 8. Triage rubric (annotate-only vs follow-up-issue)

Cluster-based over the survivor set (every survivor accounted for, sync
-audit style). Two verdict buckets; **never inline test churn** (chunk
non-goal) and **never text assertions** (AGENTS §5):

**Annotate-only** — a row in the report's cluster table with a one-line
proof/verdict; no issue, no config change (D5):

1. **Diagnostic-text class** — `Debug`/`Display` fmt bodies,
   `expecting`/message strings. Killing them requires the text
   assertions AGENTS §5 forbids. (#274 precedent.)
2. **Provable-equivalent class** — saturation/clamp guards,
   arm-deletions where the arm is unreachable or panics identically,
   boolean rewrites over literals, `Ok(default())` substitutions on
   infallible constructors. One-line equivalence proof required per
   cluster; no proof, no annotation (it goes to bucket 3 or 4).
   (#274's `poll.rs:24` precedent.)
3. **Unobservable-through-public-surface class** — mutations in private
   helpers whose change no public path can observe (documented leniency
   per the A1 policy). Must name the public surface audited.

A class that **recurs across ≥ 2 weekly runs** and is fully in bucket 1
or 2 earns a follow-up `backlog:` issue proposing the path-anchored
`exclude_re` entry for `.cargo/mutants.toml` (D5) — that issue, not
this chunk, carries the config change.

**Follow-up-issue** — one `backlog:` issue per cluster (not per
survivor), category label + `p2`/`p3`, body = summary + permalink to
the report section (AGENTS §7):

4. **Real gap** — behaviour change survives the suite and a feasible
   killing test exists at the lowest layer (unit/proptest in the
   module's `mod tests`). Label `testing`.
5. **Boundary smell** — a survivor indicating business logic hiding in
   the presentation layer (the dumb-client rule, AGENTS §1). This is
   the signal the suitability report actually wanted from this sweep;
   label `architecture`.

## 9. Stage-2 checklist

1. Read this plan, #427, dispatch §1/§9; recreate the worktree if
   missing (branch `ci/mutants-ui-pure-modules`; if its PR landed,
   `<branch>-v2` per AGENTS §8).
2. §6 steps 0–3 (doctor, count refresh, capped `-j4` run, tree check).
3. Apply §5 per the §7 ladder rung — including the **revert** rung
   (then skip to step 5 with the report documenting the scoping
   decision).
4. Triage the local run's `mutants.out/` survivors per §8; open the
   follow-up issues (`gh issue create`, `backlog:` prefix).
5. Write the report artefact (D7): runtime + counts (refreshed M, W,
   rung), cluster table, verdicts, issue links, scoping/revert record.
6. Gates: `actionlint` on the workflow if available (non-fatal if the
   tool is absent — say so in the report);
   `cargo nextest -p taskboard-ui-slint` canary (suite untouched and
   green); `git status --short | grep -v '^??'` clean.
7. Commit(s) standalone and meaningful (`ci(...)`/`docs(...)` as
   appropriate); **no push, no PR** — stage 3 ships.

## 10. Risks

1. **Runner cost**: one extra runner × up to 4 h weekly — bounded by
   the §7 ladder and the 240-min job timeout; the revert rung is a
   designed outcome, not a failure.
2. **Survivor flood**: 407 first-ever-swept mutants; the sync one-off
   saw 44/281 ≈ 16% survivors → expect ~40–80 here. The cluster rubric
   keeps triage effort ∝ classes, not survivors; issues per cluster.
3. **Flaky goldens/journeys on CI** would produce false-*caught*
   verdicts → the survivor report **understates** gaps (recall floor,
   not ceiling). No test changes are in scope; an observed flake goes
   to the `flaky-test` watchlist process (AGENTS §7), never fixed
   opportunistically here.
4. **Churn drift**: `view_state.rs` (303 of the 407 mutants) is the
   hottest UI file; §6 step 0 re-measures at run time, and the weekly
   lane re-measures by construction.
5. **Glob-semantics pin**: `--file` whole-path matching is a 27.1.0
   behaviour; the YAML comment flags re-verification on version bump
   (`--list-files` output must keep matching the three literal paths).
6. **rust-cache interaction**: two jobs in one workflow — Swatinem v2
   scopes its default key by job; worst case if not, the jobs share a
   cache (correct, merely less optimal).
7. **TIMEOUT-class noise**: cargo-mutants derives per-mutant timeouts
   from the baseline run of a suite that includes slow goldens, so
   hang-kill TIMEOUTs may inflate; the report must state the
   caught/survived/timeout split explicitly (sync audit precedent).
