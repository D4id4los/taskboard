# Chunk C7 plan — #327: capped cargo-mutants one-off for taskboard-app

- **Date**: 2026-11-17
- **Branch**: `ci/cargo-mutants-capped` (worktree
  `../taskboard-WT-ci-cargo-mutants-capped`, branched off `origin/main`
  @ 44c6f32b via `git worktree add -b`; worktree-doctor exit 0, 54 G free)
- **Normative sources**: round plan
  `2026-11-17-backlog-reduction-round-plan.md` §3-C7; dispatch plan
  `2026-11-17-backlog-reduction-dispatch-plan.md` §1/§8; issue #327
  (its cap spec — the 7200 s `timeout` template, `commands/` + `ipc/`
  scope, `--in-place`, `--timeout-multiplier 2.0` — is binding).
- **Kind**: research (report + trivial-kill test fixes; no other diff).

## 1. Scope surface and mutant arithmetic (measured, this worktree, today)

Filter: `cargo mutants -p taskboard-app --list --file
'crates/taskboard-app/src/commands/*' --file
'crates/taskboard-app/src/ipc/*'` (cargo-mutants 27.1.0; slash-globs
match the whole workspace-relative path — same mechanism as the
ui-pure-modules lane, re-verified against `--help` and `--list`).

| Group | Mutants |
|---|---|
| `commands/` (20 files; largest: boards 124, track 121, mod 109, tasks 85) | **892** |
| `ipc/` (5 files; largest: server 54, client 22, codec 22) | **109** |
| **Total** | **1001** |

## 2. Decisions (all open questions resolved here)

**D1 — the cap (binding).** Per-invocation wall cap = the #327
template verbatim: `timeout --signal=INT --kill-after=90s 7200 …`
(2 h per run). Chunk budget cap = **4 backgrounded runs, ≤ 6 h total
wall**. The cap is the inactivity-cutoff defense, not a quality
target: a cap trip is a recorded outcome, not a failure.

**D2 — cap arithmetic (evidence).** Precedents: sync one-off
120 min / 281 mutants ≈ 25.6 s/mutant; ui-pure one-off 58 min / 407
mutants ≈ 8.5 s/mutant. #327 itself prices app tiers *worse* than
sync's (subprocess-heavy integration tests: each mutant re-runs
daemon-spawning tests). Projections for 1001 mutants: serial
8.5–25.6 s/mutant → 2.4–7.1 h; pessimistic 60 s/mutant → 16.7 h.
Serial exceeds the 7200 s cap 1.2–8×. Hence D3's sharding.

**D3 — tool invocation & sharding (recorded order).**

1. **Run 1 — `ipc/` first** (cheapest, validates the harness on the
   subprocess-heavy tier):
   `timeout --signal=INT --kill-after=90s 7200 env SQLX_OFFLINE=true
   cargo mutants -p taskboard-app --file
   'crates/taskboard-app/src/ipc/*' --in-place
   --timeout-multiplier 2.0` — 109 mutants ≈ 15–47 min serial:
   inside the cap with margin even at the pessimistic rate.
2. **Run 2 — `commands/` full, parallel**:
   same template with `--file 'crates/taskboard-app/src/commands/*'`
   and `-j 4` (16-core box; `-j 4` bounds concurrent
   daemon-spawning test processes to keep memory sane). 892
   mutants / 4 workers ≈ 1.0–3.8 h: straddles the cap — the trip is
   expected to be possible and is handled by runs 3–4.
3. **Runs 3–4 (only if run 2 trips the cap)**: split `commands/`
   into two alphabetical halves by explicit `--file` lists
   (half A: boards…mod; half B: output…track), each under its own
   7200 s cap. Completed shards' `mutants.out/` results accumulate;
   results already recorded before a trip stay valid.
4. After every run: `git status --short` must be clean (the #327
   `--in-place` cleanliness check); any residue is restored before
   the next run. `mutants.out/` lives in the worktree root and is
   **not committed** — stage 2 adds a one-line `mutants.out/` entry
   to `.gitignore` **before run 1** (verified absent today via
   `git check-ignore`).

**D4 — triage threshold for in-round trivial kills.** A survivor is
fixed in-round iff **both** hold: (a) the kill test is ≤ 10 lines
and lands in an existing test location (the file's inline
`#[cfg(test)]` module or the pre-existing `tests/cli_*.rs` file that
already covers that verb) reusing existing fixtures/fakes/helpers —
no new harness infrastructure; and (b) the fix is a test-only
change (no production edit) or follows TDD order: failing test
committed first, then the production change (law [L](c)). Survivors
needing new infrastructure, deeper harness work, or judged
provable-equivalent → one `backlog:` issue each (or a comment on an
existing issue when one already owns the gap); provable-equivalence
gets a one-line proof in the report, mirroring the ui-pure report's
Cluster A convention.

**D5 — long-command discipline.** Every run goes background in this
session; between polls: draft the report skeleton, triage completed
shards, write backlog issues, draft kill tests. All landed work is
committed before run 1 starts. The flock-wrapped nextest gate is
deferred per the #879 rule if contended; the pre-commit hooks are
the stage gate.

**D6 — baseline.** Before run 1: `SQLX_OFFLINE=true cargo nextest
run -p taskboard-app` must be green (cargo-mutants runs its own
baseline copy, but a locally-known-green baseline prevents burning
the cap on a pre-existing red suite). Backgrounded + polled like
everything else.

**D7 — deliverables.** The report
`.artifacts/reports/2026-11-17-cargo-mutants-taskboard-app-report.md`
(method + cap + full arithmetic, run outcomes per shard, every
survivor dispositioned with evidence — issue numbers or
failing-test-first commit refs), the `.gitignore` line, trivial-kill
tests + their TDD production fixes, backlog issues. Nothing else in
the diff. Raw logs stay out of git (excerpted in the report).

## 3. Report skeleton (D7)

Same shape as `2026-10-26-mutation-testing-ui-pure-modules-report.md`:

1. Header (date, branch/worktree/commit, origin issue #327 + plan
   links, toolchain versions).
2. §1 Run outcome table (per shard: mutant set, counts caught /
   missed / unviable / timeout, wall clock vs cap, baseline status).
3. §2 Survivor triage — clusters, verdicts (trivial kill / backlog
   issue / provable-equivalent with one-line proof), evidence links.
4. §3 Cap accounting — arithmetic from §1–§2 above, trips and
   fallbacks taken, unmutated remainder recorded (if any) with the
   follow-up issue number.
5. §4 Method & invocation log (exact commands, verbatim).

## 4. Risks

- **Memory under `-j 4`**: app integration tests spawn daemon
  subprocesses; concurrent shards may OOM the box. Mitigation: `-j 4`
  ceiling, monitor first shard, drop to `-j 2` or serial on pressure.
- **Wall-clock-sensitive tests inflating timeouts**: existing flaky
  watchlist tests may eat the per-mutant timeout budget. Mitigation:
  `--timeout-multiplier 2.0` (issue-bound); observed flake → comment
  on its `flaky-test` issue per AGENTS §7, never fix opportunistically.
- **Cap trip mid-shard**: partial `mutants.out/` is still valid
  evidence for completed mutants; the un-run remainder is recorded
  and tracked by a backlog issue rather than silently dropped.
- **Common-mod mutants in `commands/mod.rs` (109)** mutate shared
  routing used by every verb — kills depend on broad integration
  suites; expect a lower catch rate there. Triage expects
  equivalent-routing clusters; do not chase with new harness.
- **`mutants.out` size** (1001 mutant dirs with logs) stays in the
  worktree only; removed after the report lands (`rm -rf mutants.out`
  alongside the report handoff, like the coverage trio's trailing
  `rm`).

## 5. Stage-2 step order (summary)

1. Commit this plan (done by stage 1) + `.gitignore` line.
2. Baseline nextest green (D6), backgrounded.
3. Runs per D3, backgrounded, interleaving report drafting,
   triage, issue filing, kill tests between polls.
4. Cleanliness check after each run (D3.4).
5. Triage per D4; issues + TDD fixes; report per D7/§3.
6. Gate: pre-commit hooks; flock-wrapped nextest deferred if
   contended (D5).
