<!--
Index for the recurring review templates. These files are EXECUTED IN
PLACE by unattended ZCode idle-time runs — the one folder in
templates/ that is never copied and filled in. Origin:
.artifacts/reports/2026-11-25-recurring-review-agents-evaluation-report.md
-->

# Recurring Review Templates

Four prompts for unattended, analysis-only idle-time runs. Each produces
one report under `.artifacts/reports/` and `backlog:` issues per
AGENTS.md §7, and fixes nothing.

| Template | Slug | Cadence | Asks |
|---|---|---|---|
| `test-strength-review.md` | `test-strength` | fortnightly | Has the suite kept up: coverage trend, undriven entry points, surviving mutants |
| `ui-ux-design-review.md` | `ui-ux-design` | fortnightly, one slice per run | Does the interface work for its user: contrast, labels, icons, layout, heuristics |
| `architecture-drift-review.md` | `architecture-drift` | monthly, one crate slice per run | Did new code keep the house architecture |
| `watchlist-flaky-pass.md` | `watchlist-pass` | fortnightly | What happened on the flaky-test watchlist, and what should happen to each entry |

All four bind `_common-rules.md` (safety frame, anchoring, ledger,
issue hygiene, report form).

## How to queue a run

1. Open a lean session in this workspace. An idle-time run continues
   the session that created it, so a long working session becomes
   unwanted context.
2. Queue an idle-time task with one of these prompts. Do not edit the
   template or `_common-rules.md` while the task is queued.
3. When the run ends, read the report's header (Status, Yield) and its
   Findings. Issues are already filed; fixes go through the normal
   plan → dispatch path.

- **Test strength**
  `Run the recurring review defined in .artifacts/templates/reviews/test-strength-review.md. It is an unattended, analysis-only idle-time run: that template, the common rules it binds, and AGENTS.md are the whole instruction. Do not ask questions — deliver the report and the issues the template specifies.`
- **Design review** (first run only after backlog-reduction round 7 has
  landed; the template checks and aborts otherwise)
  `Run the recurring review defined in .artifacts/templates/reviews/ui-ux-design-review.md. …`
  (same tail as above)
- **Architecture drift**
  `Run the recurring review defined in .artifacts/templates/reviews/architecture-drift-review.md. …`
- **Watchlist pass**
  `Run the recurring review defined in .artifacts/templates/reviews/watchlist-flaky-pass.md. …`

A run has no guaranteed start time. Every template anchors its window
on the previous report of its family, so a late run reviews everything
since the last one and nothing is missed or done twice.

## What a run may do

- Write its report and a `/tmp` scratch directory.
- `git fetch`, and fast-forward the `main` checkout when it is clean
  and only behind.
- Run gated cargo commands: everything through
  `scripts/memory-gate.sh`; the coverage trio through
  `scripts/run-quality-gates.sh`.
- Create `backlog:` issues and comment on issues.

It may not commit, push, open or touch pull requests, create branches
or worktrees, change any other file, close or reopen issues, create
labels, or stop a process by name.

## Rules of this folder

- The first run of a family is its baseline; the template names the
  predecessor it builds on.
- Each report carries a **Finding ledger**. The next run reconciles it
  before looking for anything new, so findings are filed once.
- A finding class a test or script could decide becomes a gate
  proposal (the report's *Mechanisation candidates*), not a recurring
  finding.
- To change a shared rule, edit `_common-rules.md`. The **binding
  prelude** at the top of each template restates the safety frame and
  must read identically in all four — change them together.
- Reports are untracked until committed with the next docs change.

## Tracking what the reviews found

The ledger of each report is the record. If labels named
`review-<family>` exist in the repository, every run adds its family's
label to the issues it files, and `gh issue list --label review-<family>
--state all` is the found-versus-realised count for that family. The
templates work without the labels and never create them.

## Deliberately not templates

- **Golden coverage** — a preflight of the design review.
- **Scheduled-lane liveness** — a gate, not a review.
- **Dependency updates and advisories** — Dependabot and `cargo deny`.
