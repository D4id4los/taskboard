# Hygiene Doc Sweep (N-C) — Compressed Chunk Plan

- **Date**: 2026-11-18
- **Kind**: chunk plan — compressed execution (planner = implementer =
  dispatcher, `docs/agent_dispatch.org` §7), per the dispatch plan
  `.artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md` §4.
  **Compression stated**: the dispatcher executes this chunk directly in
  its own session instead of the three-stage agent protocol.

## Items & decisions

- **#904 (AGENTS.md × memory gate)** — do.
  §8 doctor paragraph: "free disk is below the floor" → "the
  resource floor (disk or MemAvailable)" — the doctor has gated on
  both since #889. §10: one added paragraph routing expected-heavy
  builds (≳1 min) through `scripts/memory-gate.sh` beside the
  existing `run-quality-gates.sh` paragraph.
- **#892 (merge.rs comment cross-reference)** — **condition not
  lifted**: `crates/taskboard-domain/src/merge.rs` was last touched by
  the C5 rename itself (482485b4); no later round edited the file, so
  the byte-identical constraint still binds. Record-and-defer comment
  on the issue; no code change.
- **#859 (UiHandle::refresh doc)** — do. Extend the `UiHandle::refresh`
  doc in `crates/taskboard-ui-slint/src/lib.rs` (~line 895) to restate
  the moved-outside-the-state-Arc ⇒ `refresh_forced` rule and point at
  the forced verbs (`set_view_context` / `force_derive`). Doc comment
  only; no behavior.
- **#851 (stray `#…#` backups)** — do. Inspection evidence (2026-11-18,
  main checkout): all three `#…plan.md#` files under
  `.artifacts/plans/` diff against their tracked counterparts as
  strictly older snapshots — each tracked file carries same-day
  amendments (disk-hygiene protocol additions) or typo fixes the backup
  predates; no unique content. Deleted: the three plan backups plus
  `.artifacts/#%2Aediff-merge%2A#6fjOuJ#` (ediff temp, 2026-10-11).
  Prevention: tracked `.artifacts/.gitignore` with Emacs auto-backup
  patterns (`#*#`, `.#*#`, `.*#` lockfiles).
- **#924 (oomd)** — verified 2026-11-18: `systemctl is-active
  systemd-oomd` → `active` (exit 0); `earlyoom` inactive (expected —
  one daemon suffices). Close the issue with this evidence.
- **#778 (upstream Slint draft)** — do, draft-only. In-repo draft at
  `.artifacts/reports/2026-11-18-slint-window-max-upstream-issue-draft.md`,
  built from
  `.artifacts/reports/2026-11-14-task-view-window-max-width-collapse-report.md`.
  **Posting is the operator's call** (external action); the issue stays
  open with the draft linked.

## Gates

Pre-commit hooks (memory-gate-wrapped clippy) on every commit; docs +
one doc-comment — no behavioral surface moved, so the flock-wrapped
nextest smoke is not required by the dispatch plan's §4 bar (no doc-
adjacent code moved). The full §10 suite runs on the PR via CI.

## Bookkeeping

PR body references #904 #892 #859 #851 #778. #924 closed directly
(no PR). At-merge closures per dispatch plan §10.
