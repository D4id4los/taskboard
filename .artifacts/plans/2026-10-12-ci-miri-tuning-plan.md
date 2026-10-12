# [Plan] Miri Weekly Job — Green Within the 120-Minute Budget

- **Date**: 2026-10-12
- **Kind**: stage-1 iteration plan (WI-C of the 2026-10-12 backlog batch) —
  CI + docs only; no production code. The deliverable is a *measured*
  configuration, not a guessed one: stage 2 converges on it by driving
  `workflow_dispatch` runs from this branch.
- **Target**: `.github/workflows/miri.yml`, `docs/testing_strategy.org` §10,
  `.artifacts/taskboard-potential-improvements-backlog.md` (final commit).
- **Branch / worktree**: `ci/miri-tuning` in `../taskboard-WT-ci-miri-tuning`,
  off `main` @ `6a53645`.
- **Goal**: the weekly Miri job completes green inside its existing
  120-minute `timeout-minutes`, with every reduction being an
  environment/speed gate — **no assertion weakened, no test deleted, the
  native suite unchanged** (256-case properties, failure persistence, and
  the checked-in seed replay all stay exactly as they are outside the
  interpreter job).
- **Status**: DRAFT — awaiting user review; stage 2 implements only after
  approval.

---

## 1. Context & Findings (verified 2026-10-12 against `main` @ `6a53645`)

1. **`miri.yml` today**: weekly cron (Mon 03:00 UTC) + `workflow_dispatch`
   (already present, `miri.yml:9-10`); a single job `miri`
   ("cargo miri (domain & state)") with `timeout-minutes: 120` running
   `cargo +nightly miri test -p taskboard-domain -p taskboard-state`
   (`miri.yml:22-41`); workflow env `RUSTUP_TOOLCHAIN: nightly`; an apt
   step for `libdbus-1-dev pkg-config`; concurrency group per ref with
   `cancel-in-progress`. **The job has never run green.**
2. **Local timing anchors** (from the dispatch plan / backlog, Phase 3
   local verification): the state engine A-series runs 12/12 under Miri in
   ~9 s interpreted; the **domain suite takes ~45 min at 16 cases**; **one
   P-equivalence property exceeded a 9-minute local budget at a single
   case** (the P2/P3 family in `taskboard-state`). At the native 256-case
   default the job would need on the order of 12+ hours — the timeout is
   not a tuning target, it is a wall the current config hits by ~10x.
3. **`PROPTEST_CASES` genuinely caps every property in this repo** —
   verified against the pinned proptest 1.11.0 source
   (`Cargo.lock`; `~/.cargo/.../proptest-1.11.0`):
   - The `proptest!` macro wraps *every* test it generates — including
     ones carrying `#![proptest_config(...)]` — in
     `contextualize_config` (`sugar.rs:160-175, 226-244`).
   - `contextualize_config` unconditionally overwrites `cases` from the
     `PROPTEST_CASES` env var (`config.rs:20, 88-91`). This is what makes
     the env var win even over the raw `Config::with_cases(256)` sites
     (`with_cases` alone would beat the env var — the macro's
     contextualize pass is the decisive link in the chain).
   - The same function honors `PROPTEST_DISABLE_FAILURE_PERSISTENCE`
     (`config.rs:43-44, 130-131`): any value sets
     `failure_persistence = None` for every property in the process.
4. **The job aborts before cost even matters — failure persistence.**
   `TestRunner` calls `load_persisted_failures2` at the start of *every*
   property (`runner.rs:594-597`), which opens the regressions file
   (`failure_persistence/file.rs:105-108`). Under Miri's default
   isolation, file access is an unsupported operation → hard abort. The
   in-tree helper `test_support::proptest_config`
   (`taskboard-domain/src/test_support/mod.rs:83-93`) disables persistence
   under `cfg!(miri)` — but **three domain proptest blocks still use raw
   `Config::with_cases(256)`**: `entities.rs:239`, `state.rs:126`,
   `remote.rs:274`. The state crate even has a checked-in seed file
   (`tests/proptest.proptest-regressions`) that would be opened. So the
   first full domain run aborts on `entities_roundtrip_through_serde`
   (or either of the other two raw blocks) regardless of case caps.
   `PROPTEST_DISABLE_FAILURE_PERSISTENCE=1` in the job env fixes all of
   them globally **without touching a single test file**. (Detail for the
   record: the helper's doc comment attributes the abort to `getcwd`
   canonicalization; on Linux `file!()` is absolute so `getcwd` is
   skipped (`file.rs:201-205`) and the operative denial is the
   regressions-file open. Same conclusion, precise mechanism.)
5. **Environment-gate precedent already in tree** (the pattern any
   mid-ladder gate must follow — scoped, commented, documented in
   testing_strategy): `clock.rs:40` (`Utc::now`), `snapshots.rs` ×7
   (insta socketpair), `sqlite_smoke.rs:22` (tempdir + sqlite FFI),
   `proptest.rs:647` (P4/P5 — "runs the live tokio engine").
6. **The workload being budgeted** (counted 2026-10-12):

   | Crate | Suite | Tests | Cases (native) | Miri cost profile |
   |---|---|---|---|---|
   | domain | plain unit tests (`#[cfg(test)]` in src) | 168 | — | cheap (~min total; pure logic, cf. A-series ≈ 9 s / 12 tests) |
   | domain | properties (`proptest!` blocks: entities 3, state 3, remote 4, command 2, persistence 4) | 16 | 256 | dominates; ~linear in cases (§4) |
   | state | `engine.rs` (A-series a1–a17 + sync helpers) | 27 | — | cheap (~9 s interpreted, already verified) |
   | state | `proptest.rs` p2, p3 (equivalence pins) | 2 | 256 | **heavy**: >9 min for a single case of (at least) one of them |
   | state | `proptest.rs` p4/p5 | 1 | 64 | Miri-ignored (environment gate, finding 5) |
   | state | `snapshots.rs`, `sqlite_smoke.rs` | 8 | — | Miri-ignored (environment gates) |

7. **No caching in `miri.yml`, and none should be added**: rolling
   `nightly` moves ~daily, `rust-cache` keys off the rustc version, and a
   weekly cadence means near-zero hit rate. The Miri compile (~4–8 min
   both crates, est.) is accepted fixed overhead per job. The apt step is
   defensive cargo-cult from `ci.yml` (neither domain nor state pulls the
   keyring/dbus tree) — kept as-is; not this item's churn.
8. **Iteration mechanics are already in place**: `workflow_dispatch`
   runs the workflow definition *from the named ref*, so
   `gh workflow run miri.yml --ref ci/miri-tuning` exercises each ladder
   step pushed to the branch. `miri.yml` has no `push`/`pull_request`
   triggers, so pushing the branch triggers nothing else; the per-ref
   concurrency group means branch iteration never cancels `main`'s weekly
   run. The dispatch §2 WI-C exception authorizes stage 2 to push this
   branch (never a PR). WI-B (MSRV job, landed) touched `ci.yml` only —
   no interaction.
9. **Parallelism inside a step**: libtest runs a test binary's tests
   across the runner's vCPUs (ubuntu-latest ≈ 4). With 16 domain
   properties at one cap the wall time approaches total-CPU/4; with only
   two heavy properties (p2, p3) the wall time approaches
   max(p2, p3)·cases, not the sum. The math in §4 uses this.

## 2. Design Decisions

| # | Decision | Choice & rationale |
|---|---|---|
| D1 | **Zero-test-file ladder** | Steps 1–3 (§3) touch `miri.yml` only — env vars and step/job structure. `PROPTEST_DISABLE_FAILURE_PERSISTENCE` covers the raw-config blocks (finding 4); `PROPTEST_CASES` covers the caps (finding 3). No assertion, no native case count, no gate changes. The hard rule is satisfied mechanically: there is nothing in the diff *to* weaken. |
| D2 | **Isolation stays ON** | No `MIRIFLAGS` at all. The env-off persistence replaces both the helper's per-site job and any `-Zmiri-disable-isolation` temptation; default isolation keeps the interpreter's strongest checking. This *is* the policy decision the "Miri + proptest isolation friction" backlog entry asked for: isolation on, scoped gates as shipped, persistence env-disabled in the interpreter job only. Recorded in §10 (§6 below) and the entry marked SUBSUMED. |
| D3 | **Ladder order is fixed by the dispatch** | Case caps → job split → filter scope; escalate only when a measured run proves the current step cannot fit. No skipping ahead on estimates. |
| D4 | **Run 1 = per-crate steps, cheap-first** | Replace the single combined command with two steps in the existing job — domain first, state second — each with its own step-scoped `PROPTEST_CASES` (4 and 2). Two steps in one job share one compile (the second invocation rebuilds nothing), give per-crate runner timings from the very first run, and make the per-crate caps possible *before* any job split. Cheap-first ordering means a state-step timeout still leaves the domain measurement in the log. |
| D5 | **Convergence target: parallel jobs** | Expected final shape: `miri-domain` and `miri-state` jobs (§3 step 3), each `timeout-minutes: 120`, each with its own cap — domain as high as headroom allows (anchor: 16 cases ≈ 45 min locally), state low (anchored to the per-case P cost). Splitting buys each crate the full budget in parallel, which is the only way to give domain meaningful case counts while state keeps its heavy pair. Weekly cron and the dispatch trigger are unchanged and drive both jobs. |
| D6 | **Filter scope = documented last resort** | Only if a *single* p2/p3 case cannot fit a dedicated 120-min job even at `PROPTEST_CASES=1`: then a dedicated slow-property step at C=1 first; failing that, `-- --skip p2_ --skip p3_` on the state invocation, with §10 recording exactly what the interpreter skips and where that coverage lives natively (256-case runs in the PR pipeline + checked-in seed replay). Prefer *keeping* p2/p3 at C=1 over any skip — one interpreted case of each is still unique value (interpreter-only UB detection on the equivalence pins). |
| D7 | **New-abort triage protocol** | The job has never completed; a first full run may surface fresh interpreter-impossibilities (fs/time/env ops in tests not yet gated). Triage rule: a scoped `#[cfg_attr(miri, ignore)] // environment gate` **only** for interpreter-impossibility, mirroring finding 5's pattern, each one itemized in the PR description and recorded in §10. Never a gate for budget, never a changed assertion. This is the single sanctioned exception to D1's zero-test-file rule, and it is an *addition* of an environment gate, not a logic change. |
| D8 | **How "caps are honored" is verified** | Nothing in the output prints the effective case count. Two proofs: (a) source-level — finding 3, pinned proptest 1.11.0; (b) empirical — the property-step wall time scales ~linearly with the cap between consecutive runs (e.g. 4→8 cases ≈ 2× the domain step's property time). A typo'd value is silently ignored by proptest (eprintln only), so (b) also catches fat-fingered env values. |
| D9 | **Headroom rule** | The final config's green run must finish ≤ ~90 min (25% margin under the 120-min kill) — absorbs runner variance and nightly churn. A 118-minute "green" is a red config. |
| D10 | **Bookkeeping** | Tuning entry ("[2026-10-09] Miri CI tuning: verify and time-cap the weekly job") → ✅ DONE with the final-config green run URL; isolation-friction entry ("[2026-10-09] Miri + proptest isolation friction") → ✅ DONE as SUBSUMED, pointing at the §10 policy (D2). |
| D11 | **No rust-cache, no nightly pinning, apt untouched** | Finding 7. Pinning a dated nightly would trade reproducibility for staleness the weekly job shouldn't have; the cron *wants* fresh Miri. |
| D12 | **Green-run provenance** | Stage 2 iterates with interim commits, then squashes to the clean set (§7) — which rewrites the SHA the green run came from. After the squash (and after any stage-3 config fix), re-dispatch once on the final SHA and link *that* run in the PR. Content-identical is not enough; SHA-identical costs one run. |

## 3. The Config Ladder

Each step is one pushed commit (interim; squashed before the PR — §7) plus
one `workflow_dispatch` run; the run's measurements decide whether the
next rung is needed. Escalation is evidence-driven, never scheduled.

### Step 1 — run 1: persistence env gate + per-crate step caps (single job)

```yaml
env:
  CARGO_TERM_COLOR: always
  RUSTUP_TOOLCHAIN: nightly
  # Environment gate (testing_strategy §10): proptest's failure
  # persistence opens the regressions file at every property start,
  # which is unsupported under Miri's default isolation. Native runs
  # keep persistence, including the checked-in seed replay.
  PROPTEST_DISABLE_FAILURE_PERSISTENCE: "1"

jobs:
  miri:
    name: cargo miri (domain & state)
    # ... unchanged: runs-on, timeout-minutes: 120, checkout, apt,
    #     dtolnay/rust-toolchain@nightly {components: miri} ...
    steps:
      # ... unchanged setup steps ...
      - name: Run Miri on taskboard-domain (capped)
        env:
          PROPTEST_CASES: "4"
        run: cargo +nightly miri test -p taskboard-domain
      - name: Run Miri on taskboard-state (capped)
        env:
          PROPTEST_CASES: "2"
        run: cargo +nightly miri test -p taskboard-state
```

Purpose of run 1: (a) prove no aborts remain (the three raw-config blocks
from finding 4 plus any surprise from D7's triage list); (b) measure the
runner's per-crate cost curve. Expected ≈ 30–90 min total (§4). If it
reds by timeout, the domain step's completed timing still recalibrates
run 2 exactly.

### Step 2 — ratchet the caps on measured data (0–2 more runs)

Decision rule per crate: with the measured step time `t(N)` at cap `N`,
model `t(N) ≈ t₀ + k·N` (t₀ = plain tests + compile, k = per-case slope;
two runs give both). Pick the next cap so predicted `t ≤ 90 min` (D9).
Domain ratchets toward its 16-case anchor; state stays at 1–4 unless the
measured k is far below the 9-min lower bound. If attributing the state
cost between p2 and p3 is needed, split the state step diagnostically:

```yaml
      - name: Run Miri on taskboard-state — p2 (attribution)
        env: { PROPTEST_CASES: "1" }
        run: cargo +nightly miri test -p taskboard-state --test proptest -- --exact p2_interpret_matches_repository_load
      - name: Run Miri on taskboard-state — p3 (attribution)
        env: { PROPTEST_CASES: "1" }
        run: cargo +nightly miri test -p taskboard-state --test proptest -- --exact p3_interpret_reproduces_the_pipeline_state
```

(Name filters are libtest filters; the A-series and gated suites are
covered by the unfiltered state step, which stays.)

### Step 3 — job split (when serial wall time exceeds ~90 min at the caps worth keeping)

Same file, two jobs; each gets the full 120-min budget in parallel:

```yaml
jobs:
  miri-domain:
    name: cargo miri (domain)
    runs-on: ubuntu-latest
    timeout-minutes: 120
    env:
      RUSTUP_TOOLCHAIN: nightly
      PROPTEST_DISABLE_FAILURE_PERSISTENCE: "1"
      PROPTEST_CASES: "<measured, e.g. 16>"
    steps:
      # checkout / apt / toolchain+miri / miri setup — as today
      - name: Run Miri on taskboard-domain
        run: cargo +nightly miri test -p taskboard-domain
  miri-state:
    name: cargo miri (state)
    runs-on: ubuntu-latest
    timeout-minutes: 120
    env:
      RUSTUP_TOOLCHAIN: nightly
      PROPTEST_DISABLE_FAILURE_PERSISTENCE: "1"
      PROPTEST_CASES: "<measured, e.g. 2>"
    steps:
      # checkout / apt / toolchain+miri / miri setup — as today
      - name: Run Miri on taskboard-state
        run: cargo +nightly miri test -p taskboard-state
```

Workflow-level `env`/triggers/concurrency stay as they are (both jobs
share the per-ref group — a new dispatch cancels the whole prior run,
which is what iteration wants). Cost note: doubles runner-minutes and the
~2–5-min Miri setup; acceptable for a weekly job.

### Step 4 — filter scope (only on evidence that a single p2/p3 case cannot fit; D6)

Dedicated slow-property step at `PROPTEST_CASES: "1"` first; if even that
times out, `-- --skip p2_ --skip p3_` on the state invocation and §10
records the skipped scope + where it is covered natively. If we land
here, the PR description must show the timing evidence that forced it.

## 4. Budget Math

Fixed overhead per run (per job once split): checkout + apt ≈ 1 min;
nightly toolchain + Miri component + `miri setup` ≈ 2–5 min; compile of
the crates under the interpreter ≈ 4–8 min. Total fixed ≈ **7–14 min**.

Variable cost, per crate, with the anchors and their unknowns stated:

- **Domain**: T_d(N) ≈ t₀ + k_d·N with k_d ≈ 45 min / 16 cases ≈
  **2.8 min per case** (all 16 properties interleaved on ~4 vCPUs; the
  45-min figure is a local wall-clock and its box-to-runner ratio is
  unknown — treat ×1.0–2.0 until run 1 measures it). t₀ (168 plain
  tests) ≈ low single digits. Estimates: N=4 → ~12–25 min; N=8 →
  ~23–47; N=16 → ~45–93.
- **State**: engine + sync tests ≈ tens of seconds. p2/p3: the only hard
  number is a **lower bound** — one property exceeded 9 min for a single
  case locally; the true per-case cost and which of p2/p3 it is are both
  unknown. Wall time with 2 heavy properties ≈ max(p2, p3)·N: C=1 →
  ≳9–30 min; C=2 → ≳18–60; C=4 → ≳36–120 (no longer safely inside
  budget + headroom).

Serial single job at (4, 2): ≈ 7–14 + 12–25 + 18–60 ≈ **37–99 min** —
plausibly green with headroom, which is why run 1 starts there. At the
caps worth keeping long-term (domain 16, state 2–4) the serial sum
reaches ~70–150 min → that is the trigger for step 3, where each crate
owns 120 min in parallel: domain job ~52–107 min at N=16, state job
~25–70 at C=2 — both pass D9's ≤90-min bar in the mid estimates, and the
ratchet rule trims whichever misses.

Every number above is a prior, not a promise: run 1 replaces the two
unknowns (runner speed ratio; p2/p3 per-case cost) with measurements,
and the D9 rule — predicted ≤90, measured ≤90 — governs each subsequent
rung. The weekly cron on `main` runs the identical final config on the
identical runner class, so a green in-budget dispatch run transfers.

## 5. Validation Protocol (every ladder rung)

```sh
# in ../taskboard-WT-ci-miri-tuning, on ci/miri-tuning
git push origin ci/miri-tuning            # WI-C's §2 exception; never a PR
gh workflow run miri.yml --ref ci/miri-tuning
gh run watch <run-id> --exit-status       # or: gh run list --workflow=miri.yml
gh run view <run-id> --log > /tmp/run.log # step timings; triage via --log-failed
```

Per rung, record: run URL, per-step durations, computed k_d and p2/p3
per-case cost, and the D9 verdict. These go into the PR description —
the dispatch's acceptance criterion ("at least one green, in-budget run
of the final config, run URL in the PR") is satisfied by the final rung's
run (re-dispatched post-squash per D12), with the ladder's history as
its evidence trail. Note: dispatch runs on the branch do not interact
with `main`'s schedule (finding 8).

Sanity checks folded into every rung: `--exit-status` (conclusion),
total duration ≤ 120 min (and ≤ ~90 for the final config, D9), zero
abort-style failures (`--log-failed` shows "unsupported operation" only
if a D7 gate is missing), and the D8 timing-delta check when the cap
changed between runs.

## 6. `docs/testing_strategy.org` §10 — What Gets Documented

The section currently states the Miri scope as
`cargo miri test -p taskboard-domain -p taskboard-state` (§10 item 2) and
the out-of-band cadence (item 3). The final commit extends §10 with the
job's actual contract (exact final wording follows the converged config;
content, not boilerplate):

1. **Job shape**: the job(s), their per-crate commands, and their env —
   `PROPTEST_DISABLE_FAILURE_PERSISTENCE=1` and the per-job
   `PROPTEST_CASES` values, stated as *interpreter-only speed gates*:
   the native suite is unchanged (256 cases, failure persistence, and
   the checked-in seed replay everywhere outside the interpreter job).
2. **Why persistence is env-off under the interpreter**: the
   regressions-file open at every property start is unsupported under
   Miri's default isolation (finding 4); disabling it there loses
   nothing semantically — persistence only remembers seeds between
   runs, and native runs keep replaying them.
3. **The cap policy**: caps exist to fit the 120-min budget; they move
   only with a measured budget justification recorded in the PR that
   moves them; they are never a coverage statement. (This is §9's "no
   weakened tests" rule restated for CI env.)
4. **The exact scope, if step 4 was reached**: which families the
   interpreter skips (e.g. p2/p3) and where that coverage lives
   natively — with the timing evidence that forced the skip.
5. **Any D7 gates added mid-ladder**, one line each, in the same style
   as the existing §10 references on the in-tree gates.
6. **The local-run recipe**: `PROPTEST_DISABLE_FAILURE_PERSISTENCE=1
   cargo +nightly miri test -p taskboard-domain -p taskboard-state`,
   noting that a bare `cargo miri test` without the env aborts on the
   three raw-config blocks (finding 4) — with the migration of those
   three to `test_support::proptest_config` noted as §9 follow-up
   material.

## 7. Commit & Bookkeeping Plan (stage 2)

Iteration commits accumulate freely during the ladder (each pushed to
feed `workflow_dispatch`); before the PR they are squashed to a clean,
standalone set (AGENTS §8 rebase hygiene — no interim "try 8 cases"
noise lands on `main`):

1. `ci(workflows): miri weekly job — interpreter budget: per-crate jobs, proptest case caps, persistence env gate`
   — the final `miri.yml` (whatever rung it converged at; if the job
   split happened, this commit carries it).
2. `docs(testing): record the Miri interpreter scope and env gates in testing_strategy §10`
   — §6's content. Separate from (1) because it is reviewable prose, not
   config; together they are the deliverable.
3. `docs(backlog): mark miri tuning done, isolation-friction subsumed`
   — both entries per D10, with the final-config green run URL in the
   note.

After the squash: `git push --force-with-lease origin ci/miri-tuning`,
one fresh dispatch on the final SHA (D12), then hand off to stage 3.

## 8. Risks

- **The 9-minute P figure is a lower bound from an abandoned run.** The
  true per-case cost of p2/p3 (and which one is heavy) is unknown; if a
  single case exceeds ~60–70 min on the runner, step 4's skip becomes
  likely and the "P2/P3 at reduced cases" hope from the backlog dies at
  C=1. The ladder surfaces this in run 1–2 rather than at PR time.
- **Fresh interpreter aborts on first full run** (the suite has never
  completed under Miri). Handled by the D7 protocol — but each such gate
  is a test-file change that must survive the hard-rule review, so every
  one needs airtight "interpreter-impossibility, not budget"
  justification in the PR description.
- **Nightly churn**: rolling nightly Miri can regress or change timing
  week to week; a red weekly job may be upstream's fault. Mitigation is
  D9's headroom plus re-run-on-next-nightly triage; pinning a dated
  nightly is deliberately rejected (D11) and stays §9 material if churn
  proves chronic.
- **Runner variance near the ceiling**: ubuntu-linux runners vary ±10–20%
  in wall time; a config green at 115 min will flake. D9's ≤90-min bar
  exists to make this a non-issue; if the measured world forces >90, the
  cap comes down rather than the bar.
- **Env-var footguns**: a malformed `PROPTEST_CASES` is ignored with a
  stderr warning only (finding 3/D8) — the timing-delta check is the
  tripwire. And the caps ride proptest's `contextualize_config`
  semantics: a future proptest bump that changes env precedence would
  silently decouple them (§6 item 3's policy sentence is the guard; the
  lockfile pins 1.11.0 today).
- **Doubled runner-minutes after the split** — trivial for a weekly
  public-repo job; noted if the repo ever goes private/quota-capped.
- **Squash invalidates the green run's SHA** — handled by D12's
  mandatory post-squash re-dispatch; forgetting it fails the dispatch's
  "final config" acceptance on a technicality stage 3 is instructed to
  catch.

## 9. Out of Scope (backlog candidates, not this item)

- Migrating the three raw proptest configs (`entities.rs:239`,
  `state.rs:126`, `remote.rs:274`) to `test_support::proptest_config`
  so bare local `cargo miri` runs work without the env var — the job env
  makes it non-blocking; noted in §6 item 6.
- Pinning a dated nightly / adding rust-cache to `miri.yml` (D11).
- Extending Miri to `taskboard-sync-nextcloud` / `taskboard-storage-sqlite`
  (FFI + HTTP territory; §10 already scopes them out).
- nextest-under-Miri, `PROPTEST_VERBOSE` plumbing, per-property cap
  matrix beyond what §3's steps need.

---

**Estimated diff**: `miri.yml` ± ~40 lines (env + step/job restructure),
`testing_strategy.org` §10 + ~30 lines, backlog + ~6 lines. Triage said
size M — the mass is iteration time on GitHub runners, not diff size.
