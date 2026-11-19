# Chunk R1 stage-2 report — lld linker (#1069, `ci/dev-lld-linker`)

Implementation record for the stage-1 plan
`.artifacts/plans/2026-11-19-dev-lld-linker-plan.md`. All work in
`taskboard-WT-ci-dev-lld-linker`, commit `45cfebce` on top of the stage-1
plan commit `49fe9aaf`.

## 1. What landed

- `.cargo/config.toml` (new): the D1 stanza verbatim —
  `[target.x86_64-unknown-linux-gnu] rustflags = ["-Clink-arg=-fuse-ld=lld"]`
  with the constraint/scope comment block.
- D2 apt additions (`lld` first in the install list):
  `ci.yml` 5 steps, `benchmark.yml` 1, `nextcloud-integration.yml` 1,
  `mutation-testing.yml` 2, `release.yml` linux 1 — 10 lines across 5
  files. `miri.yml` / `kani.yml` untouched per D2.
- Diff shape matches the plan's estimate (~20 lines, 6 files).

## 2. AFTER measurement (identical command per round law (a))

Environment: rustc 1.99.0, 4 logical CPUs, warm incremental; `ld.lld`
resolved via `PATH=/usr/lib/llvm-21/bin:$PATH` (D4-equivalent until the
operator runs `sudo apt install lld`). Warm-build note: the rustflags
change invalidates *every* unit including dependencies, so an untimed
full `cargo build --workspace --all-targets` (5m 04s) ran first to
restore the before-run's "deps cached" discipline; then:

`find crates -name '*.rs' -exec touch {} +` →
`scripts/memory-gate.sh /usr/bin/time -v cargo build --workspace --all-targets --timings`

| Metric | BEFORE (bfd) | AFTER (lld) | Δ |
|---|---|---|---|
| Wall | 3:09.36 | **1:50.12** | **−1:19 (−42%)** |
| CPU (User+Sys) | 692.98 s (494.46 + 198.52) | **633.54 s** (450.94 + 182.60) | **−59.4 s (−8.6%)** |
| CPU utilization | 365% | 575% | +210 pp |
| Max RSS | — | 3.88 GB | (recorded, no before value) |

Per-unit sums from `target/cargo-timings/cargo-timing-*.html`
(123 units, same buckets as stage-1):

| Bucket | BEFORE | AFTER |
|---|---|---|
| integration-test binaries | 98 units, 967.1 s (70.0%) | 98 units, 973.4 s (81.5% of a smaller total) |
| lib-as-test | 8 units, 217.3 s | 8 units, 109.8 s |
| lib/bin | 11 units, 145.8 s | 11 units, 67.5 s |
| other-test (benches/examples) | 24.2 s | 26.8 s |
| build-script | 27.3 s | 16.2 s (only 1 run rebuilt — touch scope, see caveat) |

Caveats on comparability: (1) the per-unit sum is parallel-work across
cores and compresses non-linearly with wall time — the *link* savings
show up as the wall drop (per-unit compile+link durations shrink mainly
where linking dominates: lib-as-test −49%, lib/bin −54%). (2) The
build-script bucket is not comparable: touching only workspace `.rs`
files leaves all but one build script cached in both runs, and the
stage-1 27.3 s figure covered 27 units while this run rebuilt 1 (16.2 s
is slint's build script alone, which reruns when the crate rebuilds).
The headline comparison — same touch, same command, same warm
discipline — is wall 3:09 → 1:50 and CPU 693 → 634 s.

Acceptance: measured improvement on both wall and CPU; no regression.

## 3. App-bin edit loop check

`touch crates/taskboard-app/src/main.rs && cargo build`, steady state:
**3.42 / 3.34 / 3.27 s** (and 3.90 s in the timings-instrumented run) —
inside the acceptance floor "≤ baseline + 1s" (baseline ≈3 s). Two
intermediate runs of 11.4/10.0 s (and one 225.8 s outlier immediately
after the full --all-targets rebuild) were load-contended artifacts of
back-to-back measurement, not steady state; the isolated repeats above
are the recorded number. Loop unaffected.

## 4. D3 release verification

`scripts/memory-gate.sh cargo build --release -p taskboard-app` linked
successfully through lld (13m 34s full release build, fresh release
profile units); `./target/release/taskboard --version` →
`taskboard 0.1.0` and `--help` ran. Release/linux links with lld and
produces a working binary; windows-msvc/mac untouched (target-scoped
stanza). release.yml's linux job installs `lld` (D2), so the release
lane cannot break.

## 5. Gates

- Pre-commit hooks (fmt, slint token lint, memory-gated clippy
  `--workspace --all-targets -D warnings`): **pass** at commit
  `45cfebce`.
- `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock,
  PATH-prefixed): **pass — 4995 tests run, 4995 passed, 64 skipped**.
  Full §10 suite (fmt/clippy/doc/deny/cov) remains with the dispatcher
  (clippy and fmt already green via the pre-commit hooks).

## 6. Operator action required

**`sudo apt install lld` on this machine** — until it runs, every local
build in this worktree needs `PATH=/usr/lib/llvm-21/bin:$PATH` (the
standing config now routes links through lld and fails loudly
otherwise; that is D1's designed behavior). Nothing else on the machine
was observed to break; other checkouts will hit the same PATH
requirement only once they pull this config.

## 7. Deviations

- None against the plan. The measurement protocol added an untimed full
  warm build before the timed run (§2 note) — required to keep the
  before/after comparison honest given the rustflags change invalidates
  dependency units too; it strengthens, not changes, the recorded method.
