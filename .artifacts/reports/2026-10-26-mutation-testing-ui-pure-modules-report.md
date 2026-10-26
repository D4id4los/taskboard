# Mutation-testing survivor report — the UI pure modules (#427, dispatch chunk 8)

- **Date**: 2026-10-26
- **Branch**: `ci/mutants-ui-pure-modules` (worktree
  `../taskboard-WT-ci-mutants-ui-pure-modules`, branched off
  `origin/main` @ 05a766b)
- **Origin**: issue #427; plan
  `.artifacts/plans/2026-10-25-mutants-ui-pure-modules-plan.md`
  (§6 run protocol, §7 revert ladder, §8 triage rubric); dispatch plan
  §9 chunk 8.
- **Toolchain**: cargo-mutants 27.1.0, nextest 0.9.148, stable rust,
  local x86_64 Linux (16 cores), `SQLX_OFFLINE=true`.

## 1. Run outcome

| Metric | Value |
|---|---|
| Target mutant set | `resolver.rs`, `view_state.rs`, `presentation.rs` (`-p taskboard-ui-slint --file` ×3) |
| Mutants (refreshed pre-run) | **407** (`view_state` 303, `presentation` 96, `resolver` 8) — unchanged from plan §1 |
| Wall clock (capped run, cap 7200 s — **not hit**) | **58 min** |
| Caught | 308 |
| Missed (survivors) | **3** |
| Unviable (compile-fail) | 96 |
| Timeout | **0** |
| Baseline (unmutated) suite | green (after the nextest switch, §4) |

**Revert ladder (plan §7)**: W = 58 min ≤ 180 min → **rung 1: ship the
unsharded job** (`timeout-minutes: 240` untouched). The revert criterion
did **not** trip; the workflow change stands on this branch. CI wall
expectation: the job is serial (`--in-place`), matching the measured 58
min plus checkout/apt/cache overhead (~10–20 min warm), well under the
240-min job timeout; it runs on a parallel runner beside the core lane,
so the weekly workflow's wall-clock stays pinned to the core lane's
~2.5 h.

Per-mutant economics: ≈ 8.5 s average (6 s build + 4 s test typical),
which prices the weekly lane at ~1 runner-hour — proportionate for a p3
audit lane.

## 2. Survivor triage (plan §8 rubric, cluster-based)

Every survivor accounted for; 3 survivors → 2 clusters, both in
`presentation.rs`.

### Cluster A — provable-equivalent: `delta == 0` arm behind the `Now` early return

- Survivors: `presentation.rs:134:14` (`due_phrase`, `<` → `<=`),
  `presentation.rs:153:18` (`scheduled_phrase`, `<` → `<=`).
- Verdict: **annotate-only (bucket 2, provable-equivalent)**.
- Proof (one line, both sites): `delta == 0` ⇒ `duration_bucket(0)` =
  `Now` (presentation.rs:100, `seconds(0) < 60`) ⇒ both functions return
  the `Now` phrase at their match's first arm *before* the `<`
  comparison is consulted — so `<` and `<=` are equivalent on every
  reachable input.
- No issue, no config change (plan D5: no `.cargo/mutants.toml` edit in
  this chunk; nothing here recurs-and-proven-permanent yet).

### Cluster B — real gap: direction wording of `scheduled_phrase` is mutant-unobservable

- Survivor: `presentation.rs:153:18` (`<` → `>`): flips every direction
  arm — future inputs render "started {q} ago", past inputs render
  "starts in {q}" — and **survives the entire suite**.
- Verdict: **follow-up issue (bucket 4, real gap; no boundary smell —
  the logic is pure presentation formatting, correctly placed)**.
- Issue: [#458](https://github.com/D4id4los/taskboard/issues/458)
  (`backlog:`, labels `testing` + `p3`). It records why the existing
  directional test (future ≠ past at one bucket, presentation.rs:1016)
  cannot see the swap, and the two candidate fixes: a formatter-contract
  wording-class assertion, or typing the direction (house A1 style) so
  the kill is structural. No inline test churn in this chunk, per the
  dispatch non-goals.

### Not triaged as survivors

96 unviable mutants (compile-fail) and 0 timeouts — no TIMEOUT-class
noise (plan §10 risk 5 did not materialise: the 2.0 multiplier on the
nextest-derived baseline produced no hang-kills at all).

## 3. Workflow change applied (plan §5, rung 1)

- New `ui-pure-modules` job appended to
  `.github/workflows/mutation-testing.yml`; the core job, schedule,
  concurrency, and artifact contract are byte-identical (revert =
  delete the appended block, plan D1).
- Full ci.yml system-dep stack (plan D2), dedicated artifact
  `mutation-testing-report-ui-pure-${{ github.run_id }}` with
  `if-no-files-found: error` (plan D3), `continue-on-error: true`
  mirroring the core lane's report-outcome stance.
- Top-of-file comment amended with the #427 scoping line.

## 4. Deviations from the plan (with reasons)

1. **`-j4` was infeasible; the run is serial `--in-place`** (plan §6).
   cargo-mutants 27.1.0 rejects `--jobs` together with `--in-place`
   (CLI error, verified). Since `--in-place` is exactly what the CI job
   runs (serially, by the same constraint), the serial local run is a
   *direct* measurement of CI wall clock — a better estimate than the
   plan's `-j4` copy-mode emulation, not a weaker one. The 7200 s cap
   was not reached.
2. **`--test-tool nextest` added** (plan D4 said no `--test-tool`
   change; the workflow run command gains the flag). Reason: the first
   attempt with cargo-mutants' default `cargo test` **aborted at
   baseline** — `golden_visuals`'s tests disagree on `DriverOptions`
   (headless vs not) inside one process (`tests/support/driver.rs:348`
   `OnceLock` assertion); under plain `cargo test` the binary's tests
   share a process, under nextest each test gets its own. CI itself
   never runs this suite under plain `cargo test` (ci.yml: nextest
   everywhere). D4's concern was *narrowing the killing suite*; nextest
   runs the identical test set as the workspace's standard runner, so
   "caught" keeps its meaning while the lane becomes runnable at all.
   The YAML comment documents this. Follow-up observation recorded:
   the `cargo test`-mode collision is a latent test-support limitation —
   folded into #458's context, not a separate issue, since the suite is
   CI-defined as nextest-run.
3. **`--in-place` dirties two tracked proptest-regression files**
   (`presentation.txt`, `tooltip.txt`): proptest persists the failing
   seeds of *mutated* runs (expected kills) into tracked files. Restored
   via `git checkout --` after the run; post-run `git status` shows no
   tracked modification. **Note for the weekly lane**: on CI the tree is
   ephemeral, so this is harmless there, but local runs of this lane
   should expect and discard these two files.
4. **actionlint is not installed** on this machine — non-fatal per plan
   §9.6; the YAML was verified with a PyYAML parse (clean) and mirrors
   the existing job's structure.

## 5. Gates (plan §9.6)

- Workflow YAML: PyYAML parse OK; actionlint absent (recorded above).
- Canary: `cargo nextest run -p taskboard-ui-slint` — **394/394 passed**
  (suite untouched by this chunk).
- Tree integrity after the `--in-place` run: `git status --short`
  shows only untracked `mutants.out/` and build output; zero tracked
  differences.
- No push, no PR (stage 3 ships); `main` untouched.

## 6. Reproduction

```sh
cd ../taskboard-WT-ci-mutants-ui-pure-modules
env SQLX_OFFLINE=true cargo mutants \
  -p taskboard-ui-slint \
  --file 'crates/taskboard-ui-slint/src/resolver.rs' \
  --file 'crates/taskboard-ui-slint/src/view_state.rs' \
  --file 'crates/taskboard-ui-slint/src/presentation.rs' \
  --in-place --timeout-multiplier 2.0 --test-tool nextest
# expect ~58 min; then `git checkout -- crates/taskboard-ui-slint/proptest-regressions/`
```
