# Small-Residue Sweep (A6, compressed) — Chunk Plan

- **Date**: 2026-11-22
- **Kind**: chunk plan (compressed — dispatcher-owned per
  `docs/agent_dispatch.org` §7; single agent session = the dispatcher,
  no three-stage dispatch). Scope: the dispatch plan
  `2026-11-22-backlog-reduction-round5-dispatch-plan.md` §10.
- **Branch**: `chore/small-residue-sweep`.

## Items & recorded dispositions

1. **#1215 (ruled)** — amend
   `.artifacts/plans/2026-11-21-undo-ui-reminder-undo-plan.md`
   US-2-S3 to the ruling's verbatim wording ("the undo adds no
   delivery-addressing action; the action-layer prune is the standing
   delete contract") with the dated supersession note citing ruling
   comment 7079171410. No storage change. **Done in this commit.**
2. **#776** — dead-verb removal, **premise refuted on verification**:
   the dispatch grep shows `BoardsCmd::Use` is intercepted by the REPL
   loop (`crates/taskboard-app/src/repl.rs:1016` →
   `boards_use_line` at `:722`, the D6-6 session-selection semantics),
   and the one-shot/daemon paths refuse with the typed
   `SessionOnly`/exit-2 lane (`commands/mod.rs:1062`,
   `ipc/server.rs:649`). The verb is reachable and functional in the
   REPL; the issue's evidence (line numbers `:949`/`:1949`) predates
   the REPL implementation. **Disposition: removal is wrong — the
   verb is live; dated evidence comment posted on the issue; closes
   at merge with the comment as evidence.**
3. **#1160** — the idle-box discard rule documented: a law paragraph
   in `docs/agent_dispatch.org`'s measurement context (where
   wall-clock measurements are recorded: runtime reports under
   `.artifacts/reports/`, and the dispatch law's measurement notes
   must carry box-load context; measurements taken while another
   suite/build holds the box are discarded, not recorded).
4. **#892 (conditional)** — verify whether the byte-identical
   constraint lifted when R4's consolidation renamed binaries; check
   the #682 comment's reference; update the cross-reference or
   record-and-defer on the issue.
5. **#907** — the shellcheck gate added small: pre-commit hook over
   `scripts/*.sh` (CI-redundant avoidance: hooks run locally where
   agents work); triage the current failure list fix-or-waive per
   file with recorded rationale.
6. **#1103/#867** — operator notes posted: the unregistered
   `taskboard-WT-feat-reflection-domain` directory (verified not a
   registered worktree — the doctor WARNs; A2.1 already posted a
   dated recurrence, comment 7079413868); the hosted Deck curl matrix
   still pending — posted on #867.

## Acceptance

Each item's evidence on its issue; the PR diff confined to the named
files plus this plan; pre-commit hooks green (the shellcheck addition
self-applies to `scripts/*.sh`); #1215's amendment is the ruling's
verbatim recording.
