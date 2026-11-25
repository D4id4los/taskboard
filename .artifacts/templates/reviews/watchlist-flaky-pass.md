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

# Watchlist and Flaky-Test Pass

The dedicated pass AGENTS.md §7 asks for over the `flaky-test` +
`watchlist` issues: what was observed since the last pass, what each
entry's mechanism is, and what should happen to it. It gathers evidence
and proposes dispositions; it fixes nothing. Its "proposed fix batch"
is the input a later flaky-fix round plans from.

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

- Slug: `watchlist-pass`. Report:
  `.artifacts/reports/YYYY-MM-DD-watchlist-pass-report.md`. One run per
  ticket, fortnightly.
- Previous report: the newest `*-watchlist-pass-report.md`. With none,
  baseline mode: the last pass is
  `.artifacts/reports/2026-11-04-watchlist-pass-stage2-report.md`
  (2026-11-04), and the window starts there. Its two predecessors show
  the house's evidence standard:
  `.artifacts/reports/2026-10-17-flaky-test-watchlist-investigation-report.md`
  and `.artifacts/reports/2026-10-26-flaky-watchlist-pass-dispositions-draft.md`.

## 2. Preconditions

Anchoring per the common rules §3. The window is dated, not
commit-based: it runs from the previous report's Date to today.

## 3. Normative inputs

- `AGENTS.md` §5 (time and determinism rules), §7 (the flaky-test law:
  one issue per distinct test, each observation a comment), §10.
- `docs/testing_strategy.org` §6 (deterministic time and the seam
  inventory), §8, §9.
- `.github/workflows/ci.yml` — the upload steps for `pty-gap-logs`
  (30-day retention) and `proptest-regressions` (90 days).
- `docs/agent_dispatch.org` §2 — why a silent wait is a stall, and the
  two gate locks.

## 4. Method

**Step 1 — Collect the watchlist.** Open issues labelled `flaky-test`
and open issues labelled `watchlist`; take the union by number. For
each, read the body and comments and extract: the test id (binary and
test name), the lanes it failed in, first and last observation,
observation count, the standing hypothesis, and any closure rule the
issue states. An issue carrying only one of the two labels, or a
`watchlist` issue that is not a flaky test, is a Hygiene row.

**Step 2 — CI evidence in the window.** For each workflow file in
`.github/workflows/`: `gh run list --workflow <file> --limit 100
--json databaseId,conclusion,createdAt,headSha,attempt,event,headBranch`,
filtered to the window.

- `cancelled` is not a flake. Classify it: superseded by a newer run on
  the same branch, or cut at a job's time cap (#1401 and #1402 are
  known). Neither counts.
- `failure`: `gh run view <id> --log-failed`. Extract the failing tests
  — nextest's `FAIL [ … ]` lines, libtest's `… FAILED`, `panicked at`,
  timeouts, and kills — and map each to a test id. A match is an
  observation: date, run URL, workflow and job, commit, attempt, and
  one signature line (the panic or assertion). Pull-request runs count
  only when the test id matches the watchlist or step 3 makes it a
  candidate; a failure on a feature branch may be that branch's bug.
- Budget: at most 60 log fetches, newest first. State the cut-off.

**Step 3 — Same-commit discordance.** Group the window's runs by head
SHA. A commit with both a failed and a successful run makes every test
that failed in one and passed in the other a flake candidate — an
earlier attempt of a rerun included.

**Step 4 — Artifacts.** For failed CI runs in the window that carry
`pty-gap-logs` or `proptest-regressions` (`gh api
repos/D4id4los/taskboard/actions/runs/<id>/artifacts`), download them
(`gh run download <id> --name <artifact> --dir "$RUN_DIR/…"`) and attach
the relevant lines to the matching issue.

**Step 5 — Local probes (bounded; pure reruns).** Change no file to
make a test fail or pass — a probe is a rerun and nothing else.

- Worth running when a test failed recently and its mechanism is
  unknown, or once for an entry that has been quiet for two windows.
  Skip when the mechanism is known and a fix is pending.
- Solo: `scripts/memory-gate.sh cargo nextest run -p <crate> --test
  <binary> -E 'test(<name>)'`, at most 20 iterations per issue.
- Under load — several entries reproduce only inside the full suite:
  `GATES_LOCK_TIMEOUT=1800 scripts/run-quality-gates.sh cargo nextest
  run --workspace`, at most 3 iterations per run in total.
- Background and poll (common rules §8). A pass proves nothing about a
  flake; a failure is an observation with its log excerpt.

**Step 6 — Dispose each entry.** One class per issue:

- `FIXED-UNVERIFIED` — a change addressing it has landed; name it and
  propose the closure condition (N consecutive clean observations).
- `ROOT-CAUSED` — the mechanism is identified, with evidence; propose
  the fix shape. These form the proposed fix batch.
- `HYPOTHESIS` — evidence points at a mechanism without settling it;
  state the observation that would.
- `HARNESS-ONLY` — the test's oracle or harness is at fault and
  production is correct.
- `QUIET` — no observation in the window; since when. Two consecutive
  quiet passes earn a closure or demotion proposal.
- `DUPLICATE` — the same test as another issue; propose consolidating
  onto the fuller one.

**Comment rule.** Post one dated comment on an issue only when there
is new information: an observation, a changed class, or a proposal that
needs a decision. An unchanged `QUIET` entry gets no comment — the
report's table carries it. Keep a comment under fifteen lines: class,
observations in the window, evidence pointers, proposal. No log dumps.

**Step 7 — New candidates.** A candidate is filed only with proof of
flakiness — it passed on the same commit elsewhere, or on a rerun. Then
per the house convention: title `flaky-test: <binary> <test name>
(<context>)`; body with `**Test:** \`path\` — \`name\``, the first
observation (date, lane, run URL, signature line), and `**Why
filed:**`; labels `flaky-test`, `watchlist`, `testing`. A test that
fails deterministically on `main` is not a flake: file it as a `bug`
with priority `p1` and name it first in the Verdict.

**Step 8 — Report** (common rules §9), with two extra sections before
Metrics: **Proposed fix batch** — one row per `ROOT-CAUSED` entry:
issue, mechanism, fix shape, evidence; and **Proposed closures**, each
with its grounds.

## 5. Family metrics

CSV, one row per issue: `issue,test_id,lanes,first_observed,last_observed,observations_in_window,class,proposed_action`.
Then counts: candidates found, issues filed, comments posted, log
fetches used, probe iterations run.

## 6. Out of scope

Fixing a test or its production code. Closing or reopening an issue.
Commenting on pull requests.
