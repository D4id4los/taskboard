<!--
Common rules for the four recurring review templates in this folder:
  test-strength-review.md · ui-ux-design-review.md
  architecture-drift-review.md · watchlist-flaky-pass.md
Every template binds this file. Edit shared rules here, not in the
templates. The binding prelude at the top of each template restates
§2's safety frame on purpose — an unattended run must not depend on
having read this file to know what it may not do — so change all four
copies together. Never edit this file while a run is queued.
Origin: .artifacts/reports/2026-11-25-recurring-review-agents-evaluation-report.md
-->

# Common Review Rules

## 1. What this run is

An unattended, analysis-only review. It runs in full-automatic mode
with nobody to answer questions, in the main checkout
(`/home/viktoria/src/taskboard`). Its only products are one report
under `.artifacts/reports/` and `backlog:` issues or issue comments
per AGENTS.md §7. It fixes nothing: every finding is filed with a
proposed fix, and fixes travel the normal plan → dispatch path.

## 2. Safety frame (non-negotiable)

**Files.** Write only this run's report under `.artifacts/reports/`
and files in a scratch directory you create with `mktemp -d` under
`/tmp`. Create, edit, or delete nothing else in the repository. The
only other writes permitted are build outputs that the gated cargo
commands produce under `target/` and their sanctioned cleanup
(`rm -rf target/llvm-cov-target` after a coverage run, AGENTS.md §10).

**Git.** No commits, pushes, pull requests, merges, rebases, stashes,
branches, or tags; no `git add`, `checkout --`, `restore`, or `reset`.
`git fetch origin --prune` is allowed. The single further write is the
fast-forward of §3.2, under its stated conditions only.

**Cargo.** Never run a bare `cargo` command. Every cargo invocation —
including `cargo tree` and single-test runs — goes through
`scripts/memory-gate.sh <command>`, except the coverage trio, which
goes through `scripts/run-quality-gates.sh` (AGENTS.md §10,
docs/agent_dispatch.org §2). Never run `cargo mutants`, `cargo miri`,
or a sanitizer anywhere.

**Processes.** Never kill or signal by name: no `pkill`, `killall`, or
`systemctl`. Stop only a PID this run recorded. Never launch the
taskboard binary against the real database or with a live connector
(docs/ui_testing_instrumentation.org §3.1).

**GitHub.** Create `backlog:` issues and comment on issues. Do not
close, reopen, relabel, or edit issues; do not create or edit labels;
do not comment on, review, or otherwise touch pull requests.

**Secrets.** Never print a credential, token, or the content of a
credential store. Redact anything that resembles one.

## 3. Anchoring

### 3.1 Where

Work in the main checkout. Record `git rev-parse HEAD` and the branch.

### 3.2 Freshness

After `git fetch origin --prune`:

- HEAD equals `origin/main` — proceed.
- HEAD is behind, the current branch is `main`, and `git status
  --porcelain` lists only untracked paths under `.artifacts/` —
  fast-forward with `git merge --ff-only origin/main`. If that fails
  for any reason, leave the checkout alone.
- Anything else (another branch, a detached HEAD, other local changes)
  — do not touch the checkout. Analyse HEAD as it stands and state the
  gap in the report header.

### 3.3 Window

The newest previous report of this family is the base. Its header
records `Head commit` and `Date`. No previous report means baseline
mode; the template names its predecessor. Record in the header: Base
commit, Head commit, commits in the window (`git rev-list --count
base..HEAD`), and the previous report's path. Never describe a window
as "the last N days" — an idle-time run has no fixed start date.

## 4. Ledger before findings

1. Open the previous report's **Finding ledger**. For each issue in
   it, read the state (`gh issue view <n> --json state,closedAt`) and
   check the code at HEAD. Classify: fixed · closed-unverified ·
   open-unchanged · open-changed (say what changed) · obsolete (the
   code it described is gone).
2. Carry the table forward with the new statuses.
3. State the yield in the header: new issues · comments on existing
   issues · duplicates suppressed · previously filed findings now
   fixed.
4. Search before filing (`gh issue list --state all --search "<key
   terms>"`). Already filed is a ledger entry, and one comment if this
   run has new evidence — never a second issue.

## 5. Issue hygiene (AGENTS.md §7)

- Title `backlog: <what, where>`. One issue per idea; same-theme polish
  on one surface may share one issue when it is small enough for one
  chunk.
- Body: a summary; the evidence (file:line, golden name, run URL); why
  it matters; a proposed fix or test approach, without code; and
  `Origin:` this run's report path with the section number.
- Labels: existing ones only — one category (`ui`, `testing`,
  `architecture`, `ci`, `documentation`, `accessibility`, `security`,
  `reliability`, `dx`, `refactoring`, `performance`, `sync`, `storage`,
  `design`) and a priority you can justify (`p1`–`p3`). If a label
  named `review-<family>` exists (`gh label list`), add it.
- Do not file for: something a CI gate already decides; residue a doc
  records as deliberate (name the doc under Non-findings); doubts
  about a recorded ruling (they go to Questions).
- Report every issue number in the report.

## 6. Lint-first

A finding class that a script or test could decide is a gate, not a
recurring finding. When the evidence supports one, add a line to
**Mechanisation candidates**: the rule, the oracle, and its home — the
house techniques are `scripts/check_slint_tokens.py`, the source-scan
inventories in `crates/taskboard-ui-slint/tests/inventory/` over
`source_scan.rs`, and `crates/taskboard-ui-slint/src/token_tests.rs`.
File at most one issue per distinct gate, after searching.

## 7. Unattended conduct

- Do not ask questions. Record assumptions and open questions in the
  report's Questions section and proceed.
- A failed precondition narrows the stated scope; it never invites
  improvisation. When a template's stop condition fires, write a short
  report with `Status: ABORTED` and the reason, file nothing, and end.
- Say "no findings" plainly when that is the result, and list what was
  checked so the reader can judge how far the check reached.
- Write the report incrementally: create the file with its header
  first, then append each section as it completes. An interrupted run
  must leave a readable partial report with `Status: PARTIAL`.

## 8. Waiting and budgets

- Any command expected to run longer than five minutes runs in the
  background (the Bash tool's `run_in_background`), with useful work
  between polls. Never block silently.
- A gate that refuses — memory floor, lock timeout — is a result:
  record its message and continue with the steps that do not need it.
- `scripts/run-quality-gates.sh --status` and
  `scripts/memory-gate.sh --status` show who holds each lock. Never
  signal a holder. When a lock is held and no useful work remains, stop
  waiting at the template's budget and report the step as not done.
- Each template states its budgets. When one is spent, stop that step,
  clean up per §2, and report what is missing.

## 9. Report

Path: `.artifacts/reports/YYYY-MM-DD-<family-slug>-report.md`, with
today's date and the slug the template fixes.

Header, in this order: Date · Family · Status (COMPLETE, PARTIAL, or
ABORTED) · Base commit · Head commit · Window · Previous report ·
Yield.

Sections, in this order:

1. **Verdict** — at most five sentences.
2. **Finding ledger** — carried forward and reconciled.
3. **Findings** — ordered by risk; each with evidence, why it matters,
   a proposed fix, and its issue number.
4. **Non-findings** — what was audited and found clean, so no later
   run re-litigates it.
5. **Mechanisation candidates.**
6. **Questions and assumptions.**
7. **Metrics** — the family's machine-readable block (§10).
8. **Reproduction** — commands run, tool versions, wall time, and what
   was not done.

## 10. Metrics blocks

TOML in a fenced block, or CSV in a fenced block for long tables. The
next run of the family diffs this block against its own, so keep the
key names stable across runs and add a comment when a key changes
meaning. No prose inside the block.

## 11. Tooling notes (each cost a run time once)

- The shell's `grep` is a wrapper around `ugrep -I`: it silently skips
  any file it classifies as binary (#1405). For counts and censuses use
  `command grep`, `rg`, or Python.
- A GitHub job that hits `timeout-minutes` reports `cancelled`, not
  `failed` (#1401, #1402).
- Coverage: in `cargo llvm-cov report --lcov` output the `LF:`/`LH:`
  summary lines overstate lines by about 4% — a line with several
  coverage segments is counted more than once. Count the `DA:` records
  per file; if a file ever appears in two `SF` records, merge per line
  number (a line is covered when any record hits it). Which lines are
  executable at all is the compiler's decision, so a local run compares
  with Codecov only when the local `rustc` is CI's (#1421).
- Never search under `target/`.
- GitHub writes: never retry a create or a comment blindly. A timed-out
  call may still have succeeded (observed on 2026-11-25: a 504 that had
  created the issue, and a comment posted three times because each
  read-back came back empty). Before every retry, read back — issues by
  title, comments by a marker unique to the text — and retry only when
  it is certainly absent.
- Python has `Pillow`, `numpy`, and `tomllib`; ImageMagick and `rg` are
  installed.
