# [Plan] Benchmark Lane off PRs + Bencher Upload Fix + Bench Build Cost — Chunk 1

- **Date**: 2026-10-21
- **Kind**: stage-1 chunk plan (TDD-first where testable; this chunk is
  workflow/manifest/docs — its "tests" are the §10 gates plus the live
  evidence procedure §5) for dispatch chunk 1
  (`ci/benchmark-lane-off-pr`); stage-2 implements exactly this,
  stage-3 reviews against it.
- **Origin**: `.artifacts/reports/2026-10-21-benchmark-ci-cost-analysis-report.md`
  (§2-A, §1.4, §2-C1/C2 normative) + dispatch plan §1/§2
  (`.artifacts/plans/2026-10-21-benchmark-lane-dispatch-plan.md`).
- **Base**: worktree `../taskboard-WT-ci/benchmark-lane-off-pr`, branch
  `ci/benchmark-lane-off-pr` off **`origin/main` `241402a`** (2026-10-21;
  local `main` was 1 behind — the delta touches no seam of this chunk:
  docs + `docker/i686-cross-build` + `scripts/i686-cross-build.sh` only).
- **Status**: READY for stage 2. Every open question from the dispatch
  plan is settled in §3 with a recorded decision; deviations from
  normative origin text carry a reason (§3, §5.2).

## 1. Scope

Move the `Performance Benchmarking` lane out of per-PR CI into a
standalone `benchmark.yml` (main pushes + weekly Thursday 04:00 UTC +
manual dispatch), fix the dead Bencher upload (`BENCHER_API_KEY` env on
the 0.6.13 CLI — dead since 2026-10-18, 37 consecutive main-push
failures, **zero uploads ever succeeded**), cut the bench-profile build
cost (`[profile.bench] lto = false, codegen-units = 16` + sccache), and
align the docs.

**Non-goals** (dispatch plan §2, verbatim bar): no benches on PRs in any
form (no compile-only smoke); no Bencher thresholds/alerts/quick-mode/PR
branches (variant B is future); no sccache on other `ci.yml` jobs; no
bench-source or criterion-config changes; no `--testbed`/`--branch main`
changes; no branch-protection changes from inside the repo (required-check
removal is the user's GitHub-side action, surfaced loudly in the PR body).

## 2. Verified facts (stage-1 evidence base)

| Fact | Evidence (2026-10-21) |
|---|---|
| Bencher breakage mode | Live log `gh run view 39628093885 --log`: `error: invalid value '***' for '--token <TOKEN>': You supplied a Bencher API key to --token/BENCHER_API_TOKEN. Use --key/BENCHER_API_KEY instead.` — the 0.6.13 CLI contract is `--key`/`BENCHER_API_KEY` |
| Main pushes still failing | `gh run list --workflow=ci.yml --event=push --branch=main`: 3 latest all `failure` (39648295352, 39628093885, 39620127478) |
| Secret exists | `gh secret list`: `BENCHER_API_TOKEN` (updated 2026-10-07) — name unchanged; the *env var* the CLI reads changes to `BENCHER_API_KEY` |
| sccache-action pin | GitHub releases API: latest = **`v0.0.11`**, published 2026-07-29; README usage pins `@v0.0.11` |
| sccache env contract | sccache `src/config.rs` (main): `bool_from_env_var("SCCACHE_GHA_ENABLED")` accepts `"true" \| "on" \| "1"` (case-insensitive); README spells `"true"` + `RUSTC_WRAPPER: sccache` |
| Free weekly cron slot | miri Mon 03:00, mutation Mon 04:00, nextcloud Wed 04:00 (verified in the workflow files) — **Thursday 04:00 UTC is free** |
| Profile cascade | Root `Cargo.toml` has only `[profile.dev]` + `[profile.release]` (`lto = "thin"`, `codegen-units = 1`, `strip`, `overflow-checks`); no `[profile.bench]` today; Cargo default: bench inherits release incl. overrides (origin report §1.3) |
| CHANGELOG practice for ci: work | 14 of the last 15 `ci:` commits touch no CHANGELOG; the closest analog — the nextcloud lane split `8d42cf7` — added none. The only `ci:` CHANGELOG touch ever is `1690df2` (bencher step introduction, pre-v0.1.0-cut); its lines live in the immutable `## [0.1.0]` history section |
| Only ci.yml references the job | `grep 'Performance Benchmarking\|benchmark:'` across `.github/`, `docs/`, `README.org`, `sonar-project.properties`: hits only in `ci.yml:296-297` (+ CHANGELOG v0.1.0 history) |
| Dispatch-on-branch blocked | GitHub: a `workflow_dispatch` workflow must exist on the **default branch** before it can be dispatched at all; `gh workflow run <file> --ref <branch>` resolves workflows registered from `main` (docs "Manually running a workflow"; community discussions #25219/#8140). `benchmark.yml` does not exist on `main` → the dispatch plan's pre-merge live-dispatch evidence step **cannot work** (settled in D10) |

Seam shapes: today's `benchmark:` job is `ci.yml:296-353` (checkout →
apt → toolchain → rust-cache → bencher CLI 0.6.13 pin + rationale
comment → `bencher --version` proof → plain `cargo bench` on
`ref != main || event != push` → `bencher run` on `ref == main &&
event == push` with the broken `BENCHER_API_TOKEN` env). The `Run Tests`
comment referencing it is `ci.yml:177-183`.

## 3. Settled decisions

**D1 — Workflow identity, triggers, cron, concurrency.** New
`.github/workflows/benchmark.yml`, `name: Benchmark` (house naming: CI,
Miri, Nextcloud Integration). Triggers: `push: branches: ["main"]`,
`schedule: cron "0 4 * * 4"` (Thursdays 04:00 UTC — the verified free
slot; in-file comment lists the occupied ones), `workflow_dispatch`.
Workflow-level header comment in the nextcloud/miri style states why the
lane is off-PR (14–19 min per PR with results discarded; clippy
`--all-targets` still compile-checks bench targets on PRs; cadence:
main push + weekly + dispatch). Concurrency:
`group: ${{ github.workflow }}-${{ github.ref }}`,
`cancel-in-progress: true` — the exact house pattern of `ci.yml`,
`nextcloud-integration.yml`, `miri.yml`. *Deviation from the dispatch
plan's literal `benchmark-${{ github.ref }}`*: `github.workflow`
self-namespacing (`Benchmark-…`) is the established sibling convention
and the same uniqueness domain (workflow × ref); a fresh literal prefix
would be the only hand-rolled group name in the repo.
`env: CARGO_TERM_COLOR: always` at workflow level (house pattern).

**D2 — Job shape and step ordering.** One job `benchmark:`, `name:
Performance Benchmarking` (**kept identical** — the check name is the
branch-protection reference and the main-branch status continuity;
PR-required-check removal is the user's action, not a rename).
`runs-on: ubuntu-latest`. Steps, in order (today's lane + sccache;
nothing runs benches twice — see D3):

1. `actions/checkout@v7`
2. apt set (verbatim today's: libdbus-1-dev, pkg-config, Mesa, xkbcommon, wayland, fontconfig)
3. `dtolnay/rust-toolchain@stable`
4. `mozilla-actions/sccache-action@v0.0.11` (D4) — before any compile
5. `Swatinem/rust-cache@v2` (registry/git/target cache; kept — the
   documented pairing with sccache, registry hits stay rust-cache's job)
6. `Install Bencher CLI`: `bencherdev/bencher@v0.6.13` + `version:
   0.6.13`, **rationale comment carried over verbatim** from
   `ci.yml:313-319` (pin policy; skew fails loudly on purpose)
7. `Verify Bencher CLI`: `bencher --version` (proof step kept, comment kept)
8. `Run Criterion Benchmarks` — `if: github.ref != 'refs/heads/main'` (D3)
9. `Track Benchmarks with Bencher` — `if: github.ref == 'refs/heads/main'` (D3)

**D3 — Upload guard and the env fix (the chunk's core).** The two
mutually exclusive ref guards (`!= 'refs/heads/main'` plain
`cargo bench --workspace`; `== 'refs/heads/main'` the `bencher run`)
replace today's ref∧event pair. **The guard keys on the ref ONLY, not
the event name**: a manual dispatch on a feature branch runs the suite
and uploads nothing — it can never write to Bencher branch `main`; a
scheduled run or a manual dispatch *on main* uploads (desired: the
dispatch plan's post-merge verification path relies on exactly that).
The two conditions are exhaustive and disjoint → **benches execute
exactly once per event**; PRs never trigger this workflow at all (no
`pull_request` trigger — the point of the chunk). The upload step env is
`BENCHER_API_KEY: ${{ secrets.BENCHER_API_TOKEN }}` — secret name
unchanged (exists, verified), env name = the 0.6.13 CLI contract
(verified from the live failure log). The
"failures are loud — never re-add `continue-on-error`" comment is kept,
reworded to name `BENCHER_API_KEY` and to record that the former
`BENCHER_API_TOKEN` env name is the exact bug that killed every upload
since 2026-10-18. `bencher run` invocation unchanged otherwise:
`--project taskboard --branch main --testbed ubuntu-latest --adapter
rust_criterion "cargo bench --workspace"`; the adapter/nightly-flag
note comment moves with the plain-bench step.

**D4 — sccache pin and env placement.** Pin
`mozilla-actions/sccache-action@v0.0.11` with a house-policy comment:
"v0.0.11 = latest release at resolve time (published 2026-07-29,
resolved 2026-10-21); bump deliberately". Job-level env on the
benchmark job: `SCCACHE_GHA_ENABLED: "true"` and `RUSTC_WRAPPER:
sccache`. *Value note*: sccache's `src/config.rs` accepts
`true|on|1` — the dispatch plan's `"on"` and the README's `"true"` are
both valid; **`"true"` chosen** (the pinned action's README spelling).
Job-level (not step-level) so every cargo invocation in the job,
including inside `bencher run`, compiles through the wrapper.

**D5 — Job timeout: `timeout-minutes: 45`.** Not in the dispatch plan;
added because every off-PR house workflow bounds its jobs (nextcloud 30,
miri 120) and today's lane inherits the 360-min default — a hung
criterion bench would burn six runner-hours. 45 min ≈ 2.4× the worst
observed run (18.8 min, origin §1.2) incl. cold sccache; it is a hang
guard, not a performance gate (no quality ceiling applies — nothing
asserts on duration).

**D6 — `[profile.bench]` in root `Cargo.toml`.** New section directly
after `[profile.release]` (beside `dev`/`release`), strictly additive,
exactly two keys:

```toml
[profile.bench]
# `cargo bench` builds with this profile, which inherits `release` —
# and with it the kiosk runtime's lto = "thin" / codegen-units = 1, the
# dominant cost of the 8-minute bench-profile CI build (2026-10-21
# benchmark CI cost analysis §1.3). Release serves the shipped binary;
# benches trend relative numbers and must not pay its build cost. No
# Bencher upload has ever succeeded (2026-10-21), so this breaks no
# trend baseline. Only the two build-cost keys are overridden; `strip`
# and `overflow-checks` still inherit from `release` (no build-cost
# impact; overflow panics in benches stay loud).
lto = false
codegen-units = 16
```

No `inherits` key (bench already inherits release by Cargo default;
restating it documents nothing the section's absence doesn't).
`[profile.dev]`/`[profile.release]` untouched; profiles live only in the
workspace-root manifest (we are it); `Cargo.lock` unaffected (profiles
are not locked). Gate impact: none on outcomes — fmt/clippy/nextest/doc
build with dev/test profiles.

**D7 — ci.yml deletion + comment fix; nothing else.** Delete the whole
`benchmark:` job (`ci.yml:296-353`). Reword the dangling `Run Tests`
step comment (`ci.yml:177-183`) from "bench targets are the benchmark
job's lane (every PR)" to the new reality — proposed wording:

```yaml
        # Default target set (lib+bins+tests): bench targets are the
        # benchmark workflow's lane (main pushes + weekly; off PRs since
        # 2026-10-21); clippy --all-targets keeps them compiling here.
        # tests/bench_fixtures.rs stays — it is a regular integration
        # test, not a bench target.
```

The `--no-tests=warn` line and the fixtures sentence are unchanged. No
other `ci.yml` edit — triggers, concurrency, and all other jobs stay
exactly as they are (the rest of CI still runs per PR).

**D8 — Docs: one bullet amended, one deliberately untouched.**
- `docs/testing_strategy.org` §13 (`:506-507`), the *Benchmarks:* bullet,
  currently "across commits to catch performance regressions early" —
  amend to state the lane and cadence honestly (trends, not alerts; no
  thresholds are configured):

```org
- *Benchmarks:* =criterion= tracks hot paths (SQLite caching actor, task
  sorting) in the =benchmark.yml= lane — every =main= push plus a weekly
  Thursday 04:00 UTC run and manual dispatch (off the PR lane since
  2026-10-21: a 14–19 min job whose per-PR results were discarded; PRs
  keep =clippy --all-targets= compiling the bench targets). Uploads feed
  the Bencher =main= trend (=BENCHER_API_TOKEN= secret via the CLI's
  =BENCHER_API_KEY= env); no thresholds are configured — trends, not alerts.
```

- `docs/testing_strategy.org:225` (`BENCHER_API_TOKEN` among Actions
  secrets): **unchanged** — the sentence states where the *secret*
  lives; the secret name is unchanged, and it names no env var, so it
  cannot contradict the fix. No other doc states a bench cadence
  (grepped: only `:225`, `:506`, `:517` mention benches; `:517` is the
  fixture builder, cadence-free). CHANGELOG v0.1.0 history lines that
  describe the old cadence are released history — immutable.

**D9 — CHANGELOG: no `[Unreleased]` entry.** House practice (§2 table):
`ci:` commits do not touch the CHANGELOG — including the directly
analogous nextcloud-lane split `8d42cf7`. The workflow lane is
developer-infrastructure, not a user-visible product change. The
existing v0.1.0 Bencher lines stay as history.

**D10 — Acceptance-evidence procedure (deviation from dispatch plan,
with reason).** The dispatch plan's pre-merge live dispatch
(`gh workflow run benchmark.yml --ref <branch>`) is **blocked by
GitHub**: `workflow_dispatch` workflows are registered from the default
branch only; `benchmark.yml` does not exist on `main` until this PR
merges (verified 2026-10-21, §2 table). Working around it with a
temporary `pull_request` trigger is forbidden — it would put benches
back on PRs, the exact behavior this chunk removes. Settled procedure:

1. *Stage 2 (static validation)*: YAML parse of the new workflow
   (`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/benchmark.yml'))"`),
   structure inspection against the sibling workflows, full §10 gate
   suite green.
2. *Stage 3 (pre-merge)*: attempt the dispatch once and cite the
   expected registration error in the PR body (documents why live
   evidence is post-merge); review per the dispatch plan's mandate;
   ship the PR (body checklist below).
3. *Post-merge (user-triggered, agent-assisted)*: the merge push to
   `main` **is** the first live run of `benchmark.yml` — full suite
   inside `bencher run`, first-ever Bencher upload. PR body carries the
   loud checklist: (a) remove "Performance Benchmarking" from the PR
   required checks in branch protection **at merge time** — until then
   every PR blocks on a check that no longer reports; (b) after merge,
   verify the `Track Benchmarks with Bencher` step green and the data in
   the Bencher UI; (c) record the run duration (evidence against the
   ≤16 → ~10 min expectation — evidence, not a gate; no quality
   ceiling). After the file is on `main`, future manual runs work
   (`gh workflow run benchmark.yml --ref <any-branch>` then runs that
   branch's file version — branch dispatches stay upload-free by D3).

**D11 — Commit plan for stage 2** (rebase-merge hygiene: every commit
standalone, subject `<type>(<module>):`):

1. `ci(benchmark): the standalone benchmark lane off the PR path` — new
   `benchmark.yml` + `ci.yml` job deletion + comment fix (one coherent
   motion; the workflow must appear exactly as the job disappears).
2. `build(profile): stop bench builds inheriting the kiosk release LTO` —
   root `Cargo.toml` `[profile.bench]` + the testing-strategy §13 bullet
   (the doc states the lane's new economics; D9 keeps CHANGELOG clean).
3. `docs(plan): …` — this plan (already committed by stage 1).

Stage 3 amends/rewords on the branch; never duplicate-subject fixups.

## 4. Diff shape (stage-2 contract)

| File | Change | ~Size |
|---|---|---|
| `.github/workflows/benchmark.yml` | new (§3 D1–D4 sketch) | +~100 lines |
| `.github/workflows/ci.yml` | delete `benchmark:` job + reword `Run Tests` comment | −58 / +5 lines |
| `Cargo.toml` | `[profile.bench]` section after `[profile.release]` | +12 lines |
| `docs/testing_strategy.org` | §13 *Benchmarks:* bullet | ~6 lines |
| `.artifacts/plans/2026-10-21-benchmark-lane-plan.md` | this plan (stage-1 commit) | — |

Total code-side: ≈ +120 / −60 across four files; nothing else in the
diff (acceptance: "exactly these files" per dispatch plan).

## 5. Gates, evidence, risks

### 5.1 Gates (stage 2 and stage 3, AGENTS §10)

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D
warnings`; `cargo nextest run --workspace`; `cargo test --doc --workspace`;
`cargo deny check`; the CI-identical coverage three-command sequence +
quarantine + `rm -rf target/llvm-cov-target`. Expected impact of this
chunk's diff on outcomes: none (no `.rs` file changes; profiles outside
dev/test/release; workflow YAML not compiled locally — D10.1 covers
static validation). Workflow YAML validation is inspection + parse; the
live run is D10.3.

### 5.2 Deviations from normative origin text (all recorded, none silent)

1. Concurrency group literal (D1) — house pattern equivalence.
2. `SCCACHE_GHA_ENABLED` spelling `"true"` vs `"on"` (D4) — both valid
   per sccache source; README spelling chosen.
3. `timeout-minutes: 45` added (D5) — house style for off-PR lanes.
4. Pre-merge live dispatch → post-merge evidence (D10) — GitHub platform
   limitation, verified; workaround rejected as scope-violating.

### 5.3 Risks (ranked; first three are the first-main-push-upload set)

1. **The Bencher token has never successfully authenticated.** The
   original step shipped with `continue-on-error: true` and no CLI
   install; the pinned-CLI era died on flag parsing *before any API
   call*. Token validity, `--project taskboard` existence/access, and
   account state are all unproven until the first upload. No pre-merge
   probe is possible (the secret is CI-only; agents never exfiltrate
   it). Mitigation: failures are loud by design; the PR-body checklist
   (D10.3) makes the first merge-push run an explicit verification step;
   diagnosis is by bencher's error classes (401/403 token, 404 project,
   network). Worst case: the merge push shows a red Benchmark run —
   diagnosable, fixable in a one-line follow-up (token rotation or CLI
   bump), and it cannot block PRs (the lane is off them by then).
2. **Required-check stranding.** Between merge and the user's
   branch-protection edit, every open PR waits forever on
   "Performance Benchmarking" (a required check that no longer reports).
   Mitigation: the PR body states it as a merge-time action, first item.
3. **CLI/Cloud skew.** The 0.6.13 pin is correct today; a Bencher Cloud
   server change can require a newer CLI. Accepted by the existing pin
   rationale (fails loudly, bump deliberately).
4. **sccache first run is cold** — no failure, just no speedup yet; GHA
   cache is 10 GB LRU, so warm-hit benefit degrades gracefully. Never a
   correctness risk (a cache miss is a plain compile).
5. **Cron slot collisions** — none today (verified); future workflows
   should keep the Thursday 04:00 slot free or move deliberately.
6. **Dispatch-on-branch expectation** — anyone re-running the dispatch
   plan's original evidence step pre-merge hits the registration error;
   D10 documents it so stage 3 cites rather than debugs it.

## 6. Stage-1 record

- `scripts/worktree-doctor.sh`: exit 0 (free 116G, floor 25G; kept
  in-flight worktree `taskboard-WT-docs-kiosk-dashboard-impl-plan`).
- Worktree `../taskboard-WT-ci/benchmark-lane-off-pr` created on
  `ci/benchmark-lane-off-pr` off `origin/main` `241402a`.
- This plan is the only stage-1 commit; no implementation, no push.
