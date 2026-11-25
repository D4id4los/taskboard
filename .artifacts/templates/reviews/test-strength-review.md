<!--
Recurring review template — EXECUTED IN PLACE by an unattended ZCode
idle-time run. Unlike the other files in this folder it is never
copied and filled in. Queue it from a lean session with the one-line
prompt in README.md; the run reads this file, the common rules it
binds, and AGENTS.md. Cadence and mode are in §1. Shared rules live in
_common-rules.md; the binding prelude below is the one block restated
in every template — change all four copies together. Never edit this
file while a run that uses it is queued.
-->

# Test-Strength Review

Has the test suite kept up with the code that landed since the last
run? Three oracles, weakest first: the coverage trend, undriven entry
points, and surviving mutants. "Only the happy path is exercised" is
invisible to line coverage and is what the third oracle exists for.

> **Binding prelude (the common rules' safety frame, restated).**
> 1. Analysis only: write nothing but this run's report under
>    `.artifacts/reports/` and files in a `/tmp` scratch directory.
> 2. Git: `git fetch` and the §3.2 fast-forward are the only writes —
>    no commit, push, PR, merge, branch, stash, checkout, or reset.
> 3. Cargo: never bare — `scripts/memory-gate.sh` for every command,
>    `scripts/run-quality-gates.sh` for the coverage trio.
> 4. GitHub: create `backlog:` issues and comment; never close,
>    reopen, relabel, edit, or touch pull requests.
> 5. Nobody answers questions: record them in the report and proceed.
> 6. Processes: stop only PIDs this run recorded — never by name.

Read first: `AGENTS.md`, then
`.artifacts/templates/reviews/_common-rules.md` (all of it binds this
run), then §3 below.

## 1. Cadence and mode

- Slug: `test-strength`. One run per ticket, fortnightly.
- Report: `.artifacts/reports/YYYY-MM-DD-test-strength-report.md`.
- Previous report: the newest `*-test-strength-report.md`. With none,
  baseline mode: the predecessor is
  `.artifacts/reports/2026-11-24-test-coverage-health-report.md`
  (head commit `df5ae1a025d815823a76ab8f549654cd74d5cfc9`; its §6
  issues are the seed ledger).

## 2. Preconditions and stop conditions

- Anchoring per the common rules §3. Codecov holds what CI uploaded
  for every commit on `main`, so per-file coverage at the base and at
  the head comes from it, and no build is needed unless step 3's
  fallback applies.
- If neither Codecov nor a local run yields numbers, the run is
  PARTIAL, not aborted: the static census and survivor triage stand on
  their own.

## 3. Normative inputs

- `AGENTS.md` §5 (testing directives) and §10 (gates).
- `docs/testing_strategy.org`: §3 fakes, §4 property testing, §6 clock
  discipline, §7 UI tiers, §8 Nextcloud tiers, §9 hard rules, §13
  coverage and mutation.
- `codecov.yml` (why the patch status is informational).
- The Codecov API. Its schema, `https://api.codecov.io/api/v2/schema/`,
  is the authoritative list of endpoints. Base:
  `https://api.codecov.io/api/v2/github/D4id4los/repos/taskboard`.
  Reading this public repository needs no token.
- The baseline report: findings, structural blind spots, accepted
  residue classes. Its §1 note on the lcov format names the wrong
  mechanism — the common rules §11 have the right one.
- Dispositioned survivors, so none is triaged twice:
  `.artifacts/reports/2026-10-26-mutation-testing-ui-pure-modules-report.md`,
  `…-2026-11-17-cargo-mutants-taskboard-app-report.md`,
  `…-2026-11-21-cargo-mutants-commands-report.md`.

## 4. Method

**Step 1 — Anchor and reconcile the ledger** (common rules §3–§4).
Verify each carried finding in code, not only by issue state — a
backlog round may have closed the issue while the behavior is still
undriven, or the reverse.

**Step 2 — Codecov (no build).** Five reads, each one GET. Save the
JSON under `$RUN_DIR` and record the commit each answered for.

1. **Head.** `commits/?branch=main&page=1` — the newest commit that
   has coverage. Compare it with `origin/main` and record the distance
   in commits; a few behind is normal, since CI uploads about a
   quarter of an hour after a push.
2. **Trend.** `coverage/?branch=main&interval=1d&start_date=<the
   previous report's Date>` — `min`, `max`, `avg` per day. Record the
   window's minimum and the day it fell.
3. **Per file, base and head.** `totals/?sha=<base>` and
   `totals/?sha=<head>`: the commit totals plus one entry per file
   (`name`, `lines`, `hits`, `misses`, `coverage`). Then
   `compare/?base=<base>&head=<head>`, which carries both sides per
   file under `files[].totals.base` and `files[].totals.head`.
4. **Line level at head.** `report/?sha=<head>`, about 1.3 MB: per
   file its `name`, `totals`, and `line_coverage` as `[line, state]`
   pairs — `0` hit, `1` missed. The API has no per-function records,
   so a never-executed function is one whose every body line is
   missed: take the body's range from the source. `report/?sha=<head>&path=<file>`
   answers for one file.
5. **Rollups**, when a directory view helps: `report/tree?sha=<head>&depth=<n>`.

`file_report/<path>?sha=<head>` also answers for one file, but only
**without** the trailing slash — the documented form with the slash
returns 404.

The data is CI's own run. Compare Codecov with Codecov: a local run
differs whenever the local `rustc` is not CI's (#1421 — on 2026-11-25
the same commit measured 95.21% on CI's 1.100.0 and 95.53% on a local
1.99.0).

**Step 3 — Local instrumented run (fallback only).** Run it only when
Codecov is unreachable, has no report for `main` in the last day, or
has none for the base commit. If it runs, its per-file numbers replace
items 3 and 4 of step 2, and item 1 of step 5.

- Disk: if `df -Pk .` shows fewer than 45 GiB available, do not start
  it — say so and continue.
- Check `rustc --version` against the coverage job's log of the same
  commit (`gh run view <run id> --job <job id> --log`) and record
  both versions.
- Start it in the background and record the PID:

```sh
export RUN_DIR="$(mktemp -d /tmp/test-strength-XXXXXX)"
GATES_LOCK_TIMEOUT=1800 scripts/run-quality-gates.sh bash -c '
  set -euo pipefail
  cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
  scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
  cargo llvm-cov report --lcov --output-path "$RUN_DIR/lcov.info"
' >"$RUN_DIR/coverage.log" 2>&1
```

- On every exit path — success, failure, budget spent — run
  `rm -rf target/llvm-cov-target`. Budget: 75 minutes of wall time
  from start, lock wait included; then stop it by PID and move on.
- From the log, record the nextest summary (run, passed, failed,
  skipped) and the quarantine count. A failing test is a finding in its
  own right: name it, and do not retry it away.
- Parse with Python over the `DA:` records (common rules §11), not the
  `LF:`/`LH:` lines.

**Step 4 — Static census: wired but never driven (no build).** A test
reference means the identifier occurs in a file under a `tests/`
directory or inside a `#[cfg(test)]` module. Build the three lists with
a script, using `rg` or Python rather than the shell's `grep`:

- **CLI verbs** — the clap definitions in
  `crates/taskboard-app/src/cli.rs` and the dispatch closures in
  `crates/taskboard-app/src/commands/mod.rs`. Classify each verb:
  `driven` (argv parsed through clap to the typed invocation, or the
  binary driven), `handler-only` (the `*_on` handler has a suite, the
  argv routing has none), `undriven`.
- **Engine commands and actor messages** — every variant of
  `StateCommand` (`crates/taskboard-domain/src/messages.rs`) and of
  `UiRequest` (`crates/taskboard-ui-slint/src/requests.rs`) that no
  test tree references at all.
- **New since base** — mark each entry introduced in the window.

Report `undriven` entries individually and `handler-only` as a class
with its members. A variant built through a shared helper or a table
counts as driven; state the helpers you recognised. Check each entry
against the ledger and open issues before it becomes a finding.

**Step 5 — Coverage deltas.** From step 2's per-file data:

- files whose coverage fell from base to head, ordered by newly missed
  lines;
- files with missed lines for the first time;
- the lowest-covered files that have at least fifty lines;
- never-executed functions at head, from the line-level report.

For each candidate, decide before reporting: already in the ledger or
an open issue; documented accepted residue (logging arms per
`codecov.yml` and the baseline §4.1; `#[ignore]` tier gates;
subprocess-only paths per testing strategy §8; trait-mandated
boilerplate). Everything else is a finding.

**Step 6 — Mutation survivors.** `gh run list --workflow
mutation-testing.yml --limit 12`, then the newest run of each lane
that has an artifact. Artifact names: `mutation-testing-report-<run
id>`, `…-ui-pure-<run id>`, `…-app-pdf-<run id>`; `missed.txt` holds
the survivors. A run whose conclusion is `cancelled` was cut at its
time cap (#1401), so its artifact lists only the mutants evaluated
before the cut: say so in one line and triage what is present. Never
run a sweep.

Triage survivors not already dispositioned, at most 40 per run: a
missing assertion is a real gap (group by behavior, one issue per
behavior); an equivalent mutant is recorded with its reasoning; a
mutant that needs a slower tier is noted for that tier. List the rest
as counts per file.

**Step 7 — Findings, ledger, report** (common rules §4–§6, §9). Rank
by risk to behavior, not by missed-line count. Each finding names the
behavior missing, the file:line evidence, the introducing commit, and
a test approach that fits the strategy: lowest layer, typed outcomes,
fakes over mocks, the clock rules of §6.

## 5. Family metrics

A TOML block with: `date`, `head`, the Codecov head commit and its
distance, the trend minimum, one `[[crate]]` table rolled up from the
per-file data, one `[[regression]]` table per file that fell, the
census counts and lists, and the artifact run ids with survivor counts.
Then an appendix with every file as CSV —
`path,base_lines,base_misses,head_lines,head_misses` — which the next
run diffs against, and can rebuild for its own base from Codecov.

## 6. Out of scope

Writing or changing tests. Judging individual tests against the AGENTS
§5 rules unless that explains a coverage or mutant result. Running
mutants, Miri, or Kani. The keybinding-coverage audit (#1280 owns it).
Fixing anything.
