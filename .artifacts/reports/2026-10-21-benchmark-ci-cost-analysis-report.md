# Benchmark CI Cost Analysis — Variants for a ≤5-minute PR lane

**Date:** 2026-10-21
**Scope:** The `Performance Benchmarking` job in `.github/workflows/ci.yml` —
why it costs ~16–19 minutes per PR, what it actually buys today, and the
candidate directions for getting every per-PR job to ≤5 minutes while
keeping regular performance uploads to Bencher.
**Status:** Analysis only; no workflow changes made. A decision is pending.

---

## 1. Measured situation

### 1.1 Job durations in the latest PR run (`39635201270`, 2026-10-21)

| Job | Duration |
|---|---|
| **Performance Benchmarking** | **16 m 38 s** |
| Code Coverage | 5 m 20 s |
| Test Execution | 4 m 31 s |
| Code Quality & Linting | 1 m 10 s |
| MCP Harness Smoke | 1 m 08 s |
| MSRV Check | 0 m 50 s |
| SonarCloud Analysis | 0 m 45 s |
| Security & License Audit | 0 m 19 s |

The benchmark job **is** the PR critical path; everything else already fits
in ~5:20. Remove it from the PR lane and the pipeline wall clock is the
coverage job.

### 1.2 Growth over the last few days (PR runs, benchmark job wall time)

| Run | PR | Benchmark job |
|---|---|---|
| 39292029520 | kiosk appliance lifecycle | **6.5 min** |
| 39339621733 | process smoke harnesses | 11 min |
| 39406187309 | *refresh+draw criterion bench added* | 14 min |
| 39418439449 | *K7 mapping criterion bench added* | 13 min |
| 39450192519 … 39624947574 | (no bench changes) | 13.8 – 18.8 min |
| 39635201270 (latest) | netbook test-env report | 16.6 min |

Three benchmark suites landed within two days (ui `refresh_draw`, sync
`mapping`, plus earlier sqlite `hot_path`) and the job tripled. The ±3–4 min
run-to-run scatter is `rust-cache` hit/miss on the bench-profile build.

### 1.3 Inner split of the job (from the run log)

| Phase | Duration | Notes |
|---|---|---|
| Setup (checkout, apt, toolchain, cache restore, bencher CLI) | ~35 s | |
| `cargo bench --workspace` **build** | **8 m 00 s** | "Finished `bench` profile [optimized] target(s) in 8m 00s" |
| **Criterion execution** (36 benchmarks) | **~7 m 26 s** | sqlite `hot_path` 1:43 · sync `mapping` 3:24 · ui `refresh_draw` 2:19 |
| Teardown | ~40 s | |

Execution cost is structural: each of the 36 benchmarks pays criterion's
default budget (3 s warm-up + 5 s measurement + analysis ≈ 9 s floor →
~5.4 min floor), and the N=10000 tiers are configured with 10 samples /
15 s measurement (`hot_path.rs` `configure_group`), plus one-off fixture
population (e.g. `draw-partial/10000` alone costs ~33 s wall; the two
10 000-row sqlite benches cost ~40 s each including population).

**The build phase is inflated by a profile surprise:** the workspace sets
`[profile.release] lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`
(kiosk runtime goals), and Cargo's `[profile.bench]` inherits
`"release"` — *including user overrides*. Every PR bench build therefore
runs thin LTO over the whole dependency graph with a single codegen unit.
The Cargo book documents `inherits = "release"` for the default bench
profile; overrides cascade because the bench profile doesn't re-specify
those keys.

### 1.4 What the PR lane buys today — less than it appears

On PRs the job runs plain `cargo bench --workspace` and **discards the
results**: no Bencher upload (that step is `if: push to main`), no
threshold comparison, and criterion exits 0 on slowdowns (it only fails if
a bench panics). So a PR can slow every benchmark by 10× and stay green.
The PR lane's actual value today is *benches still compile and run*, while
`clippy --all-targets` in the check job already type-checks bench sources
on every PR.

Meanwhile the Bencher lane — the thing the decision wants to preserve — is
**currently broken**: commit `33a9350` (2026-10-18) pinned the bencher CLI
at 0.6.13, which rejects the `BENCHER_API_TOKEN` env var the step still
passes (`error: invalid value '***' for '--token <TOKEN>': … Use
`--key`/`BENCHER_API_KEY` instead`). Every main-push CI run since 2026-10-19
has failed at this step (37 consecutive failures; 38 of the last 60 push
runs; last green push run `39113075050` on 2026-10-18). **Zero Bencher
uploads have happened in that window.**

Also relevant: no thresholds are configured anywhere in the repo (no
`--threshold-*` in the workflow, no criterion.toml), so even a working
upload lane records trends but does not *alert*.

---

## 2. Variants

Estimated durations combine the measured phases; build estimates for the
profile/cache variants are reasoned, not measured.

### Variant A — Move benchmarks off the PR lane (schedule + main pushes)

Mirror the house pattern already used by `miri.yml`, `mutation-testing.yml`,
and `nextcloud-integration.yml` (the Nextcloud lane was split out of PR CI
for exactly this reason: a slow/flaky required check):

- Trigger the benchmark job on `push: main` (the Bencher step, after the
  env fix) **+ weekly cron + `workflow_dispatch`**; drop the PR trigger.
- Update GitHub branch protection: remove "Performance Benchmarking" from
  the PR required checks (otherwise PRs cannot merge).

| | Effect |
|---|---|
| PR wall clock | **~5:20** (coverage becomes the max; already ≈ target) |
| Bencher uploads | Resume on every main push; weekly run catches silent gaps |
| Regression detection per PR | None — but there is **none today either**; regressions surface on main/weekly with main-branch history for cheap bisection |
| Effort | ~10 lines of workflow + the one-line env fix (§1.4) |

The one capability genuinely lost: the per-PR smoke proof that benches
*execute* (not just compile). If that matters, add
`cargo bench --workspace --no-run` as a compile-only step later — but note
it costs the same bench-profile build (~8 min) until Variant C lands.

### Variant B — Per-PR Bencher regression gate (quick mode)

The "real" Bencher CI pattern, on PRs:

```
bencher run \
  --branch pr-${{ github.event.number }} \
  --start-point "origin/${{ github.base_ref }}" \
  --adapter rust_criterion \
  --error-on-alert --github-actions \
  "cargo bench --workspace -- \
     --sample-size 10 --measurement-time 1 --warm-up-time 0.5"
```

This gives what the PR lane lacks today: an actual per-PR regression
verdict relative to `main`, a Bencher PR comment, and per-PR trend
branches. Quick mode cuts criterion execution to roughly 3–4 min (36 ×
~2 s floor, plus the 10 k-tier iteration costs and fixture population,
which do not shrink).

| | Effect |
|---|---|
| Execution on PR | ~3–4 min |
| Job total on PR | **~12–13 min with today's build** — misses the 5-minute goal on its own |
| Caveats | Low statistical power → alert noise; needs deliberately wide boundaries; PR-branch hygiene in Bencher (headless branches with `--start-point`, `--if-changed` optional) |

Only viable as the ≤5-minute solution **in combination with C** (and even
then it lands ~6–8 min for full suites; cutting the ui suite from the PR
quick lane, or CI-selecting suites by changed paths, closes the gap).

### Variant C — Attack the 8-minute bench build (orthogonal, stacks with A/B)

1. **`[profile.bench]` override** — `lto = false`, `codegen-units = 16`
   (i.e. stop inheriting the release-only settings). This undoes the
   biggest single build-cost factor; plausibly halves the build. Tradeoff:
   benchmarks would no longer measure the exact shipped profile
   (thin-LTO/1-CGU). For relative trending that is fine, but it is a
   one-time Bencher baseline discontinuity that should be recorded.
2. **sccache with the GitHub-Actions backend** — caches bench-profile
   dependency objects across PRs; warm runs reduce the build to the three
   bench binaries + linking (~2–3 min).
3. **Path-targeted benches** — run `cargo bench -p <crate>` for crates
   touched by the PR; docs-only PRs (a decent share of recent ones) skip
   benches entirely at zero cost.

Full-fidelity benches with C1+C2: ~16.5 → ~7–9 min. Still not ≤5 with the
full default criterion budget — C enables the goal, it doesn't reach it
alone while every PR runs all 36 benchmarks at full statistical budget.

### Variant D — More hardware

An 8-core paid runner or a self-hosted runner cuts the build ~2–3×
(build dominates; criterion execution is single-threaded per bench and
won't shrink much, and runner noise changes absolute numbers). Costs:
money, or ops burden + a kiosk-relevant question of runner comparability —
**the Bencher testbed label must stay `ubuntu-latest`-equivalent for the
main-branch trend to stay comparable**; changing runner hardware is a
de-facto baseline reset. Not recommended as the primary lever.

### Variant E — Methodology shift: instruction-count benches (iai-callgrind)

Deterministic, noise-free, one iteration per benchmark — execution time
collapses, and Bencher has an `iai` adapter. But it measures instructions,
not wall time (invisible: LTO/layout effects, I/O, lock contention), needs
valgrind in CI, and means migrating all three suites. A long-term option,
not a lever for this decision.

---

## 3. Recommendation

1. **Now, regardless of the decision:** fix the Bencher breakage — pass
   `BENCHER_API_KEY: ${{ secrets.BENCHER_API_TOKEN }}` (or rename the
   secret) in the `Track Benchmarks with Bencher` step. The upload lane has
   been dead since 2026-10-18.
2. **Adopt Variant A** to hit the ≤5-minute goal immediately: it matches
   the repo's existing treatment of expensive lanes, restores regular
   Bencher uploads (main push + weekly), and removes zero currently-working
   capability from PRs — the PR bench lane has no regression detection and
   discards its results today.
3. **Keep Variant C1 on the shelf** for the weekly/main lane if 16-minute
   scheduled runs are themselves annoying, and reach for **B + C** later
   only if per-PR regression alerts become a demonstrated need — accepting
   that a full-fidelity per-PR gate and a hard 5-minute budget are in
   tension unless suites are selected by changed paths.

## 4. Evidence

- Latest PR run: `gh run view 39635201270` (job table §1.1; phase split and
  per-benchmark wall times from the `Performance Benchmarking` step log).
- Duration trend: `gh run view <id> --json jobs` over the 15 most recent PR
  runs (§1.2).
- Bencher breakage: `gh run view 39628093885 --log`, failing step
  `Track Benchmarks with Bencher`; `gh run list --event=push --branch=main`
  outcome counts; commit `33a9350` (2026-10-18) introduced the 0.6.13 pin.
- Criterion budgets: `crates/taskboard-storage-sqlite/benches/hot_path.rs`
  (`configure_group`), no `criterion.toml` anywhere (defaults elsewhere).
- Profile inheritance: root `Cargo.toml` `[profile.release]`;
  Cargo book, Profiles — `[profile.bench] inherits = "release"`.
