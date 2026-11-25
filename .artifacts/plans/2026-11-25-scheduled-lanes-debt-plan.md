# The scheduled lanes debt chunk plan (round 8, chunk D1)

- **Chunk**: D1, branch `ci/scheduled-lanes-debt`, stage 1
  (diagnose → decide → plan). Issues: #1403, #1402, #1401, #1421,
  #1413, #1424. Scope report
  `.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md`
  §D-D1; dispatch plan §5 (normative).
- **Triage-first evidence** collected 2026-11-25 via
  `gh run list --workflow <wf> --limit 6` per scheduled lane plus the
  per-job run views cited below; the six issue bodies carry the full
  tables. Workflow files read in full.
- **Stage split**: this plan; stage 2 implements; stage 3 reviews
  (runs `scripts/ux-coverage.sh` n/a — no UX plan; chunk is CI/docs).
- **No UX journey plan** (dispatch plan §1: "no UX plan").

## 1. Per-lane triage & disposition

### 1.1 #1403 — Nextcloud Integration stable leg (weekly Wed 04:00 UTC)

**Evidence.** Six consecutive scheduled `failure`s since 2026-10-21
(`gh run list --workflow nextcloud-integration.yml`), each ~2–3 min.
Latest run 45626380051 (2026-11-25): `--log-failed` shows the stable
leg dying in "Docker-tier integration tests (app daemon + CLI
journeys)" at `yeslogic-fontconfig-sys v6.0.1` → `pkg-config
fontconfig` exit 1. The workflow's apt step
(`.github/workflows/nextcloud-integration.yml:61-63`) installs only
`lld libdbus-1-dev pkg-config`; the Slint GUI deps in `taskboard-app`'s
tree need the six packages every other workflow installs. The floor
leg is green (it builds only `taskboard-sync-nextcloud`).

**Disposition: FIXED GREEN** (not mothball — the lane is the weekly
daemon+CLI drift detector against live Nextcloud, and the fix is one
line).

**Fix shape (stage 2)**: extend the apt line with
`libasound2-dev libgl1-mesa-dev libegl1-mesa-dev libxkbcommon-dev
libwayland-dev libfontconfig1-dev` (the exact set ci.yml installs).
Verification constraint: scheduled workflows cannot be re-run from a
branch — stage 2 records the change plus a *post-merge* verification
step (`gh workflow run nextcloud-integration.yml` — the workflow has
`workflow_dispatch` — after the PR lands, confirm both legs green;
that confirmation closes #1403). Local validation is YAML-lint +
diff-review only.

### 1.2 #1402 — Miri `domain — core` lane cut at 120 min (weekly Mon 03:00)

**Evidence.** Job-level view of run 45169987760 (2026-11-23):
`cargo miri (domain — core)` cancelled at exactly 120 min while the
three siblings finished 4 min / 41 min / 99 min (`state` took
1h38m — also within 21 min of the cap). Five consecutive weekly cuts
since 10-26; the last full verdict was 2026-10-19 (92 min — the lane
has since grown past 120).

**Disposition: SHRUNK WITH RATIONALE** — re-slice on a measurement,
not a blind cap raise (a bigger cap buys a slower failure; Miri is
10–100× native and the lane grows every wave).

**Fix shape (stage 2)**:
1. Measure: a capped local `cargo +nightly miri test -p
   taskboard-domain --lib --tests -- --skip pipeline:: --skip
   remote:: --skip state::` through `scripts/memory-gate.sh`
   (background + poll; the run takes tens of minutes — commit all
   other landed work first, then background it) to find which modules
   dominate the core slice.
2. Re-slice: split `miri-domain-core` into two matrix jobs along the
   measured seam (expected candidates: the scheduling/reminder
   property blocks vs the pure data modules), each
   `timeout-minutes: 120`.
3. Headroom: lower `PROPTEST_CASES` on the two hot lanes that ran
   109/111 min on 11-16 (`domain — remote/state properties` at 16 →
   measured choice) OR leave them and record the headroom rationale —
   decided by the same measurement pass. Cap raise (e.g. 120 → 180)
   is the *fallback* only if the re-slice cannot find a clean seam.
4. The `state` lane's PROPTEST_CASES=128 stays untouched (green,
   closest to cap; changing it without a fresh measurement would be
   guesswork).

### 1.3 #1401 — mutation sweep cut at 240 min (weekly Mon 04:00)

**Evidence.** Six consecutive scheduled `cancelled` at exactly
240 min since 10-19 (the last complete sweep: 149 min on 10-12). In
the latest run 45191860986 both `cargo mutants` lanes ran their full
4h and were cut; the third lane (`app commands, pdf-export`) never
started. `continue-on-error: true` + the `if: always()` upload keep
the runs looking routine while `mutants.out/` under-reports. The
executable surface grew ~32× over the window (2026-11-24 coverage
report): the budget was outgrown, not hit by a one-off.

**Disposition: SHRUNK WITH RATIONALE** — shard the sweep; retire-the-
lane (issue option 4) is rejected because the sweep's survivors →
report → triage loop is a standing testing-strategy input, and the
local-only route recreates the "nobody watches it" shape #1413
exists to kill.

**Fix shape (stage 2)**:
1. Shard each lane across matrix jobs by `--file` / module globs
   (domain, state, app-commands each become 2+ jobs sized so the
   largest fits 240 min with ≥25% headroom; sizing from the 10-12
   complete sweep's per-crate minutes plus the 32× growth ratio —
   a first CI run after merge is the true measurement, so shard
   boundaries go in the workflow as an easily-edited matrix list).
2. Make the cut loud instead of silent: drop `continue-on-error: true`
   in favour of an explicit timeout-handling step (the `if: always()`
   upload stays; a job that died at its cap is `failure`, which the
   dead-man's switch then reports) — the current shape is exactly the
   "reads as routine" trap.
3. The weekly `--in-diff` delta + monthly full-sweep cadence split
   (issue option 2) is recorded as the follow-up if sharding alone
   still doesn't fit — filed as a backlog issue, not implemented this
   chunk (one mechanism per chunk; #1338's bigger app lane rides
   after).

### 1.4 #1421 — local stable toolchain lags CI (1.99.0 vs 1.100.0)

**Evidence.** `rustup check` on this box: `stable 1.99.0 → 1.100.0
update available`; `rustc --version` = 1.99.0. CI compiles 1.100.0
(issue's Codecov-vs-local coverage delta: 4,850 executable lines
differ, ~1,500 hit/miss flipped). The #1424 measurement below was
taken under 1.99.0 while CI held 1.100.0 — the two diagnoses were
made on different compilers.

**Disposition: PROCEDURE DOCUMENTED + APPLIED** this chunk.

**Procedure (stage 2, in order)**:
1. Confirm no heavy gate run is in flight:
   `scripts/run-quality-gates.sh --status` (at stage-1 time the lock
   was held by `fix/clip-sentinel-law` since 19:52Z). `rustup update
   stable` is a ~2-min toolchain download, but it invalidates every
   concurrent worktree's `target/` incremental caches — apply it only
   when `--status` shows the lock free AND the dispatcher confirms no
   lane is mid-build; otherwise record it as the stage-2 handoff
   condition and apply at the quiet window. (Round-7 deferred this
   for exactly this reason; do not repeat the deferral silently.)
2. `rustup update stable` from the main checkout; verify
   `rustc --version` = 1.100.0.
3. Document in the plan: the sync is manual and drifts — the durable
   fix (pin `channel` to an exact version in `rust-toolchain.toml`
   and let CI read the file instead of `@stable`, plus a freshness
   probe) is filed as a **backlog issue** (out of scope: it changes
   every workflow's toolchain action in one sweep and deserves its
   own chunk).
4. The 1.100.0-vs-1.99.0 caveat is noted on any measurement in this
   chunk taken before the update (the #1424 fresh-worktree
   measurement was 1.99.0; re-verification after the update is
   unnecessary for the routing fix — path/cache detection is
   compiler-independent — but the note stands).

### 1.5 #1413 — the dead-man's switch (reporting workflow)

**Evidence.** Three lanes dead 5–6 weeks unnoticed (the §1.1–1.3
tables). Five scheduled workflows exist: miri (Mon 03:00), kani
(Mon 04:00), mutation-testing (Mon 04:00), nextcloud-integration
(Wed 04:00), benchmark (Thu 04:00). Kani and benchmark are green
(`gh run list`, latest: success). No watcher exists.

**Design — a reporting workflow + a testable script core.**

- **`scripts/lane-watch.sh`** holds the whole classification logic so
  it is runnable locally (the #1413 host option 2's coverage of the
  watcher's own death) and unit-testable without touching GitHub:
  - Input: a list of `workflow-file expectations` pairs (the five
    workflows + itself).
  - Per workflow, via `gh run list --workflow <wf> --limit N
    --json status,conclusion,createdAt,displayTitle`:
    - **staleness**: newest scheduled run older than 8 days → red
      (a weekly lane's next run is ≤7d out; 8d = one missed cycle).
    - **streak**: ≥2 consecutive *scheduled-event* runs whose
      conclusion ∉ {success} → red. `cancelled` is red only when the
      run's duration is within ~2 min of the workflow's known cap
      (timeout-shaped — the exact tell from #1401/#1402) ; a short
      `cancelled` (concurrency supersede) does not count. Scheduled
      events can't be superseded in practice (one run/week), so the
      duration rule is belt-and-braces, encoded as a per-lane
      `cap` column in the watch list.
    - `failure` counts regardless of duration.
  - Output (typed, machine + human): one row per lane
    `<wf> <verdict: green|red> <reason: fresh|stale|streak|timeout-shape>`
    plus, on any red, a rendered lane table. Exit 0 when all green,
    1 when any red, 2 on tooling error (gh auth/API failure) — never
    conflates "all green" with "could not check".
- **`.github/workflows/lane-watch.yml`** — the reporting wrapper:
  scheduled Mondays 06:00 UTC (after all three Monday lanes' 03:00/04:00
  crons have run to completion or cap), `workflow_dispatch` for
  testing; `permissions: { actions: read, issues: write }`; runs
  `scripts/lane-watch.sh`; on exit 1 renders the table into a step
  that comments per red lane (search-first per AGENTS §7: comment on
  the lane's existing issue — #1401/#1402/#1403 while open; otherwise
  open a `backlog:` issue). Exit 2 → the step fails loudly (the
  "could not check" state must not look green).
- **Its own failure path surfaces (the trap — never a second silent
  lane)**:
  1. **The watcher watches itself**: `lane-watch.yml` is in its own
     watch list; two dead/red watcher weeks are reported by the next
     live run.
  2. **A dead watcher is caught locally**: stage 2 appends a
     staleness probe to `scripts/run-quality-gates.sh` pre-flight
     (one `gh run list --workflow lane-watch.yml --limit 1` older
     than 8 days → a printed, non-fatal warning) — every quality-gate
     run on this box then double-checks the watcher liveness. (If the
     dispatcher rejects touching run-quality-gates.sh, the probe goes
     into `scripts/worktree-doctor.sh` instead — same pre-flight
     position; stage 2 decides by reading which script's contract the
     probe fits.)
  3. **A red watcher run notifies**: GitHub's default "failure"
     notification to the actor/triggering user — recorded in the
     workflow header comment as part of the design, not relied on
     alone.
- **Its own failure path tested (acceptance: "tested")**: stage 2
  runs `lane-watch.sh` locally against the live repo state (expect:
  #1401/#1402/#1403's lanes red today — the fixture *is* the current
  incident), verifies verdicts/exit codes against the §1.1–1.3
  evidence, and unit-tests the classification core with synthetic
  run-JSON fixtures (green fresh / stale / 2×failure /
  timeout-shaped cancel vs short cancel / API-error exit 2) as plain
  script tests, plus one `workflow_dispatch` of lane-watch.yml
  post-merge to see the wrapper end-to-end. A dry-run mode
  (`--dry-run`: report, never comment) guards the first dispatch.

### 1.6 #1424 — docs-only cold clippy outside the gate

**Evidence (measured, in the issue)**: fresh worktree off
`origin/main`, docs-only commit, `CARGO_BUILD_JOBS=8` through the
gate in hold mode: 75 s in, eight rustc at 13.8 GiB, MemAvailable
13.7 → 7.7 GiB (below the 8 GiB floor), pressure rising. Root cause
read from `scripts/commit-clippy-mode.sh:44-55`: the preflight
verdict depends *only* on the staged paths — it never asks whether
this worktree has a build cache. Fresh worktree ⇒ cold
`cargo clippy --workspace --all-targets` ⇒ a heavy command running
unlocked, at default job count, while another worktree's gated build
holds 15.7 GiB. This is the #1274 premise ("warm incremental cache ⇒
seconds") failing on its unstated precondition. This very chunk's
`docs(plan)` commit is a live instance (committed with
`CARGO_BUILD_JOBS` capped as a stopgap).

**Disposition: FIXED, proven per the #1274 method** (measured before
— the issue's numbers — fix — measured after).

**Fix shape (stage 2)**: admission reads the cache, not only the
diff (issue option 1):
1. `scripts/commit-clippy-mode.sh` gains a cache-warmth check after
   the docs-only classification: preflight only when the worktree's
   build cache exists and is non-empty (`target/debug/.fingerprint`
   present with entries — the worktree-local `target/`, so a fresh
   worktree naturally fails the check); otherwise `gated`. The
   conservative default rule is preserved and extended: undeterminable
   ⇒ gated. The script's header comment re-states the new premise:
   preflight = docs-only AND warm cache.
2. The hook path needs no change (it already routes on the script's
   verdict) — verify by reading `.git/hooks/pre-commit` /
   the hooks source that nothing else assumes preflight ⇒ unlocked.
3. **Proven per the #1274 method**: (a) reproduce the defect
   measurement shape on a fresh worktree — after the fix the same
   docs-only commit routes `gated` (verify by running the script;
   assert the verdict, not timing); (b) on this worktree (warm,
   docs-only plan commit) the script still says `preflight`; (c) a
   Rust-touching commit still says `gated` (regression); (d) the
   issue's floor-breach scenario is re-run *through* hold mode only
   (never un-gated) to confirm the gate holds it — with
   `CARGO_BUILD_JOBS` capped and the run stopped once the routing is
   demonstrated, since the point is the routing, not another 75 s of
   pressure. Assertions are typed (verdict strings, exit codes), no
   text/matching beyond the script's own contract.

## 2. Stage-2 work list (in order)

1. #1424 fix + tests (script verdict tests; no cargo build needed
   beyond the demonstration) — smallest, unblocks safer commits for
   the rest (after it, docs-only commits in *this* worktree still
   preflight; fresh-worktree commits gate correctly).
2. #1421: `scripts/run-quality-gates.sh --status` clear + dispatcher
   quiet-window confirmation → `rustup update stable`; record the
   procedure + the pin backlog issue.
3. #1403: the one-line apt fix.
4. #1402: the local Miri measurement (background through
   `scripts/memory-gate.sh`, poll) → the miri.yml re-slice while it
   runs.
5. #1401: mutation-testing.yml sharding + continue-on-error removal.
6. #1413: `scripts/lane-watch.sh` + fixture tests, then
   `lane-watch.yml`, then the gates-preflight staleness probe.
7. Full-gate pass: pre-commit hooks + `scripts/run-quality-gates.sh
   cargo nextest run --workspace` (background + poll beyond ~5 min;
   commit all landed work BEFORE any wait; flock-deferral per [D]).
   Scripts touched this chunk are shell — `cargo nextest` plus
   `shellcheck`-class review cover them; no Rust behaviour changes.
8. Commit series, one logical change per commit
   (`fix(ci): …` / `feat(ci): …`), PR opened by stage 3 (stage 3's
   scope ends at PR-open; the dispatcher owns the post-merge
   verification steps recorded in §1.1 and §1.5).

## 3. Stage-3 traps

- **#1413 must not become a second silent lane**: verify the watcher
  watches itself, the exit-2 path fails the workflow (not exits
  green), and the gates-preflight probe exists. A design that only
  reports when someone runs it locally is rejected.
- **No quality ceilings**: the Miri/mutation re-slices must not
  assert "fits in X min" as a guarantee — they size with headroom and
  rely on the dead-man's switch for the drift case. A test asserting
  a cap would itself violate the law.
- **Typed assertions only**: script tests assert verdicts/exit codes;
  no grep-for-message assertions.
- **The apt fix cannot be verified from the branch**: stage 3 checks
  the post-merge verification steps are recorded (workflow_dispatch
  after merge), not that a scheduled run went green during the chunk.
- **rustup update timing**: if stage 2 applied it under a busy box,
  that is a process violation — check the gate `--status` note in the
  commits/plan amendments.
- **continue-on-error removal**: the `if: always()` upload must
  survive; the survivors-report contract (mutants.out → report →
  triage) must not break.

## 4. Out-of-scope → backlog issues

Filed by stage 1 (numbers in the final report + listed here):

- The `rust-toolchain.toml` exact pin + CI-reads-the-file +
  freshness-probe durable fix (the follow-up to §1.4's manual sync).
- The weekly `--in-diff` delta + monthly full sweep cadence split for
  mutation testing (§1.3 follow-up option 2).

## 5. Disposition summary

| Lane | Issue | Disposition | One-line fix shape |
|---|---|---|---|
| Nextcloud stable | #1403 | fixed green | apt line + the six GUI deps; post-merge dispatch run confirms |
| Miri core | #1402 | shrunk w/ rationale | measured module re-slice; cap raise only as fallback |
| Mutation sweep | #1401 | shrunk w/ rationale | matrix sharding + loud timeout (drop continue-on-error) |
| Toolchain sync | #1421 | procedure documented + applied | rustup update in a quiet window; durable pin → backlog |
| Dead-man's switch | #1413 | live + own-failure tested | lane-watch.sh (testable core) + lane-watch.yml (reports; watches itself) |
| Cold-clippy admission | #1424 | fixed + proven per #1274 method | preflight = docs-only AND warm cache; else gated |
