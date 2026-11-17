# Chunk C1 — #879: codify the flock-deferral rule — Chunk Plan

- **Date**: 2026-11-17
- **Origin**: round plan
  `.artifacts/plans/2026-11-17-backlog-reduction-round-plan.md` §3-C1
  (normative); dispatch plan §2. Issue: #879.
- **Compression statement**: this chunk runs **compressed**
  (`docs/agent_dispatch.org` §7): docs-only, ≤4 files, one coherent
  edit set — the dispatcher acts as planner and implementer directly
  (this plan committed first, the edits, the full gate suite, the PR).
  The compression choice is recorded here per the law.

## The rule being codified

A subagent never blocks on the `run-quality-gates.sh` flock. When the
gate lock is contended **and no other useful work remains**, the agent
defers the gate run to the dispatcher: commit all landed work, report
"gate deferred: lock held by <holder>", end the stage cleanly —
instead of waiting silently into the ~600s inactivity cutoff (four
killed stage agents before this fix; the round-2 learning behind
#879).

## Changes

1. `docs/agent_dispatch.org` §2 (the long-command discipline): add
   the deferral rule with its trigger condition verbatim.
2. `docs/agent_dispatch.org` §8 (failure playbook): one row —
   signal: an agent died while the gate lock was held by another
   session; resolution: landed work intact, the dispatcher re-runs
   the gate.
3. `.artifacts/templates/dispatch-plan.md`, the long-command
   discipline block: the one-line deferral clause so every future
   dispatch plan inherits it.
4. `AGENTS.md` §7: unchanged (already points at the law; no
   non-negotiable added — per the round decision).

## Acceptance

- The rule's wording is consistent across both files: same trigger
  condition ("lock contended AND no other useful work remains"),
  same deferral action (commit landed work, report the holder, end
  the stage; the dispatcher runs the gate).
- No other law wording drift.
- Pre-commit hooks green (docs-only); the dispatcher still runs the
  full AGENTS §10 suite before handing the PR over.

## Commits

- `docs(plan): the dispatch-law flock-deferral chunk plan`
- `docs(dispatch): the flock-deferral rule (#879)`
