// SPDX-License-Identifier: MIT OR Apache-2.0

# Chunk report — S2a: the nextest run-side wall regression tuning (#1090)

- **Date**: 2026-11-21
- **Branch**: `perf/nextest-run-tuning` (stage-2 chunk of round 4,
  dispatch plan §1 S2a). Normative plan:
  `.artifacts/plans/2026-11-21-nextest-run-tuning-plan.md`.
- **Verdict (Phase C)**: the #1090 run-side wall regression **does not
  reproduce**. The fresh R4-method baseline on the un-tuned branch base
  is **63.8 s** — inside the acceptance band (≤ ~90 s) — and the
  `--test-threads` sweep is flat across N ∈ {16, 24, 32, 48}
  (62.6–66.0 s, all green). **No lever lands**: the sweep's best point
  is the default N = 16, so L1 has nothing to change, the L3 trigger
  does not fire, and the acceptance verdict is "band met on the fresh
  base, no residual". Zero code or config changes; the landed delta is
  this report plus the #1090 erratum comment.

## 1. Fresh baseline (Phase A.1)

R4 method verbatim (§3 of the plan): suites pre-built once
(`cargo build --workspace --tests`, warm), `/usr/bin/time`-wrapped,
sequential, idle box (verified: gate lock free, MemAvailable 19.2 GiB,
no concurrent cargo processes), `cargo nextest run --workspace
--no-tests=warn`:

| Run | wall | CPU (user+sys) | result |
|---|---|---|---|
| fresh base @ c656d54b (code = origin/main f0c85359), N=16, 04:37 | **63.78 s** | 440.6 + 80.8 s | 4897 passed, 0 failed, 65 skipped |

The R4 recorded pair (86.9 s before / 152.7 s after the merge, measured
2026-11-19) does not reproduce: the current 56-binary tree runs ~40 %
faster than the pre-merge 98-binary measurement of two days ago, with a
larger suite (4897 vs 4730 entries). Per the plan's fresh-baseline
discipline, every sweep point below compares against the fresh 63.8 s.

## 2. Diagnosis (Phase A.2, junit per-test capture)

One instrumented run (`[profile.default.junit]` temporarily added to
`.config/nextest.toml`, reverted after capture; wall 98.1 s — junit
overhead, not comparable to the clean numbers): 4897 tests, 0 failed,
sum of per-test times 1495.7 s.

- **Tail profile**: the run wall is pinned by **two by-design long
  tests**, not by binary granularity:
  - `taskboard-app::app_services ipc::ip16_attach_from_persisted_state_
    attaches_and_serves` — 60.4 s (the test's own comment pins its 60 s
    dead-identity detach contract budget explicitly;
    `crates/taskboard-app/tests/services/ipc.rs:36`);
  - `taskboard-storage-sqlite::roundtrip persisted_state_roundtrips_
    through_sqlite` — 42.7 s.
  The third-longest test is 20.2 s; everything else ≤ 11 s.
- **Per-binary sums** (top): journeys 445.4 s (n=414), domain 191.3 s
  (n=1329), app_services 93.0 s — no binary's summed time concentrates
  the tail; under nextest's global one-process-per-test scheduling the
  long members pack across binaries exactly as the plan hypothesized.
- **Effective parallelism**: sum-of-test-times 1495.7 s over a 63.8 s
  wall ≈ 23× on 16 CPUs — only possible because the suite is largely
  wall-clock-bound (daemon boots, PTYs, polling budgets), i.e. CPU is
  nowhere near saturated; CPU total was 521 s (817 % of one CPU over
  the wall).

**#1090 mechanism erratum**: the issue's "47 scheduling units" reading
is disproven for nextest. Binary count does not gate nextest packing;
the R4 nextest doubling cannot have been caused by the merge's binary
reduction. (For plain `cargo test` the per-binary scheduling statement
remains exact.)

## 3. The `--test-threads` sweep (Phase A.3)

R4 method, `--test-threads=N` on the CLI (landed lever would go into
`.config/nextest.toml`), each point memory-gate-routed, watched for new
failures (the A.3 disqualification rule):

| N | wall | CPU (user+sys) | result | validity |
|---|---|---|---|---|
| 16 (fresh base) | 63.78 s | 440.6 + 80.8 | 4897/0 | clean (04:37, pre-contention) |
| 24 | 62.91 s (summary) | — | 4897/0 | **discarded** — 95 s memory-gate lock wait (another worktree held it) |
| 32 | 64.30 s | 460.9 + 85.6 | 4897/0 | clean |
| 48 | 65.95 s | 479.0 + 98.1 | 4897/0 | clean |
| 16 (re-run) | 115.79 s | 512.2 + 100.4 | 4897/0 | **discarded** — concurrent coverage suite (see §4) |
| 24 (re-run) | 63.94 s | — | 4897/0 | formally discarded (concurrent suite in build phase), kept as corroboration |

All sweep points are green — no disqualification rule fired — and the
wall is **flat**: every N within ~2 s of the default, because the wall
floor is the 60 s `ip16` test, which runs concurrently with everything
else and bounds the run from below at any N. Oversubscription adds CPU
(+8 % user at N=48) and buys nothing.

## 4. The accidental contention experiment (the strongest finding)

Discarded runs are usually noise; here they are the diagnosis. While
another agent's full coverage gate ran concurrently on the box, the
identical un-tuned suite measured **115.8 s instead of 63.8 s**
(1.8×) — CPU roughly flat (612 s vs 521 s). That is precisely the
#1090 signature (86.9 s → 152.7 s ≈ 1.8×, CPU flat in R4's own table).
The R4 "after" number was measured on a shared box the same day other
agents were landing round-4 chunks; a contended window is the
parsimonious explanation for the doubling, and the R4 report itself
records that a first parallel pair had to be discarded for contention.

Consequence: the #1090 regression is most likely **measurement
contention, not the binary merge**. The compile-side win is real and
unchallenged; the run-side "cost" attributed to it is not reproducible
on an idle box.

## 5. Decision-table walk (plan §4/§5)

- **L1** (profile `test-threads`): sweep's best point = 16 = the
  current default → landing a config override would be pure churn.
  Not landed. `.config/nextest.toml` remains byte-identical (Q6's
  docker-tier serialization untouched, verified).
- **L3 trigger** (tail dominated by one/two merged binaries' serial
  members > ~25 % of wall AND L1 fails the band): neither condition
  holds — the tail tests are two individual tests (one in a merged
  member, one in `taskboard-storage-sqlite`, which kept full binary
  granularity), and L1 did not fail the band (the fresh base is in
  band). Not attempted.
- **L4** (accepted residual): no residual to accept — the fresh base
  meets the acceptance band outright.
- **Phase C verdict encoding**: acceptance = recovered ≤ the 86.9 s
  baseline band, met on the fresh base (63.8 s ≤ ~90 s), green 4897/0.
- **S2b binding (#1087, plan Q5)**: verdict (i) — band recovered; S2b
  may proceed under the same levers. The phase-2 consolidation of the
  six small crates remains compile-side-positive; per the mechanism
  erratum the nextest run side is not binary-bound, so no per-root
  run-side gate is needed beyond the standard green run (plain
  `cargo test` scheduling remains per-binary and was always
  understood).

## 6. CI note (plan §5 L1 risk)

No config or workflow change landed, so there is no CI-runner
`test-threads` scale mismatch to validate. CI keeps the plain
`cargo nextest run --workspace --no-tests=warn` invocation reading the
untouched `.config/nextest.toml`.

## 7. Residual observations for the backlog

- The 60 s `ip16` test is the suite's wall floor at any parallelism.
  Its budget is contractual (deliberate), but a future change that
  reaches the terminal condition faster would cut the whole-suite wall
  floor from ~60 s to ~43 s (the next-longest test). Filed as a
  `backlog:` issue (see §9).
- Measurement hygiene: wall comparisons on this shared box are
  invalid under concurrent agent load (1.8× inflation observed). The
  R4 method's idle-box discard rule is load-bearing; worth a note in
  the measurement docs. Filed as a `backlog:` issue (see §9).

## 8. Deviations from the plan

- None of substance. The junit capture needed the config-file route
  (`--message-format junit -o` is not a `cargo nextest run` flag in
  nextest 0.9.148; the junit block is config-only) — the capture was
  reverted from the worktree immediately and never committed.
- Two sweep points were re-run/deduplicated per the idle-box discard
  rule; all raw numbers are in `/tmp/s2a/` on the session box and
  summarized in §3.

## 9. Backlog issues filed

- **#1159** — `backlog: ip16_attach_from_persisted_state pins the
  whole-suite wall floor at 60 s` (performance, p3).
- **#1160** — `backlog: document the idle-box discard rule for
  wall-clock measurements` (documentation, testing, p3).
- Erratum verdict comment posted on **#1090** (comment
  issuecomment-7056904398).
