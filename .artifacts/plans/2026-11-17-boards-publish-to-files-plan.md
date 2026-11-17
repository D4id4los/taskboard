# Chunk Plan — C11: `boards publish --to-files` (#764)

- **Date**: 2026-11-17
- **Branch**: `feat/cli-boards-publish-to-files`
- **Worktree**: `../taskboard-WT-feat-cli-boards-publish-to-files`
- **Origin**: round plan
  `.artifacts/plans/2026-11-17-backlog-reduction-round-plan.md`
  §3-C11; dispatch plan
  `.artifacts/plans/2026-11-17-backlog-reduction-dispatch-plan.md`
  §11; **journey plan (the gate, authoritative for the verb's
  contract): `.artifacts/plans/2026-11-17-boards-publish-to-files-ux-plan.md`**
  (DRAFT until operator acceptance); intake issue #764.
- **Status**: DRAFT — journey-gated; implementation starts only after
  the operator accepts the journey plan.

## 1. Goal

Add the CLI's direct local-to-Files publish verb
(`boards publish --to-files`) with typed refusals and a read-back
receipt, closing the GUI/CLI asymmetry of #764. Deck-first-then-move
stays the CLI's only remote path; no new engine verb — the ceremony
mints the existing `StateCommand::BindBoard` with the files payload.

## 2. Recorded decisions (from the journey plan; restated for the implementer)

- **D-1 (grammar)**: `--to-files` is a boolean direction flag on the
  existing `BoardsCmd::Publish`, not a new subcommand — mirrors the
  `boards move` family convention. It `conflicts_with` `--connector`
  (parse-time typed error, journey R-4). `--timeout-secs` keeps its
  existing default (60).
- **D-2 (semantics)**: the ceremony is the exact GUI mint —
  `BindBoard { board, remote: RemoteBoardId::files(board.as_uuid()),
  connector: FILES_SYNC_CONNECTOR_NAME }` — reusing the shared
  `publish_board_to_files` ceremony logic (extracted out of
  ui_requests.rs's GUI arm into a shared `*_on`-family helper; the
  GUI arm becomes a caller; zero behavior delta for the GUI path).
- **D-3 (guards)**: the files-lane guard
  (`files_sync_spawn_decision`) and the live-row check
  (`!deleted && !archived && remote.is_none()`) refuse before any
  command executes; refusal classes are the existing typed
  `CommandError` surface (lane-disabled, bind-refused, gone,
  exactly-one-live-board resolution). No new error variants unless
  the implementation proves a class is missing — if so, stop, extend
  the mapping deliberately, and record it in the PR body.
- **D-4 (receipt)**: read-back of the published snapshot; structural
  fields only (bound channel Files, files remote id). Reuse the
  boards-family receipt shape; render once.
- **D-5 (twins)**: standalone (`run_core`), REPL (`boards_line`), and
  daemon one-shot all route the same ceremony; no session-mutation
  refusal (journey R-5 — unlike `boards use`). REPL prefill fills
  `--board` from the session selection (add the `Publish` arm to
  `prefill_boards`).
- **D-6 (census)**: the census `board.publish_to_files` CLI cell and
  the R-2 ruling row flip to implemented citing the journey plan
  (journey W-5) — lands on this branch, not deferred.

## 3. Work items (TDD-first; every item embeds its journey-step token)

1. **Parse grammar + conflict** (journey US-1/S1, R-4): first the
   parse tests — `--to-files` parses; `--to-files --connector`
   refuses at parse; `--to-files` alone parses with `--connector` as
   `None`. Then the `cli.rs` delta (flag + help text naming the
   direct path and the Deck-free semantics).
2. **Happy-path ceremony** (US-1/S2–S4): first the standalone
   integration test (`publish_to_files_happy_path_us1_s1_us1_s4`):
   configure a files lane (the test fixture the files-sync tests
   already use), mint a local board, run the verb, assert the
   `BindBoard` payload (exact files mint — the GUI twin's
   `publish_board_to_files_sends_exact_files_bind_command` shape,
   CLI-side) and the receipt's structural read-back fields. Then the
   shared ceremony helper + dispatch arms (standalone `run_core` and
   REPL `boards_line`).
3. **Typed refusals** (US-2/S1–S4): one test per class
   (`us2_s1` lane disabled, `us2_s2` already bound, `us2_s3` gone
   (unknown, soft-deleted, archived — parameterized),
   `us2_s4` zero/ambiguous resolution), each asserting the typed
   outcome class (`matches!`), non-zero exit, and an unchanged state
   tree. Never message text.
4. **REPL twins** (US-1/S2 prefill; D-5): the `prefill_boards`
   `Publish`-arm unit test (token `us1_s2`), the completion route,
   and the daemon one-shot routing (structural test).
5. **Census flip** (W-5): the census row + R-2 overturn citing the
   journey plan.
6. **Gate self-check**: `scripts/ux-coverage.sh
   .artifacts/plans/2026-11-17-boards-publish-to-files-ux-plan.md
   --gate` exits 0 (Test-root `crates/taskboard-app/tests` is in the
   journey plan's front matter).

## 4. Layer discipline

- One behavior, one test, lowest layer: parse law lives in
  `cli.rs`'s parse tests; ceremony/refusal behavior lives in
  `cli_standalone.rs` integration tests; routing (prefill) in
  `repl.rs` unit tests. No duplication of the refusal matrix through
  the REPL loop (routing tests assert routing only).
- The GUI arm's existing seam test
  (`publish_board_to_files_sends_exact_files_bind_command`) must stay
  green unchanged — the extraction is behavior-neutral (its
  continued greenness *is* the extraction's regression proof).

## 5. Quality gates & dispatch law

- Pre-commit hooks gate every commit (docs-stage law); land work
  before any wait.
- Stage 2 gates on hooks + flock-wrapped
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (defer per the #879 rule if contended); the dispatcher owns the
  full §10 sequence and CI watch.
- Stage 3 runs both mandates (code-quality + UX completeness,
  dispatch plan §1) plus the walkthrough protocol over US-1 and US-2.

## 6. Risks & traps

- **Extraction regression**: moving `publish_board_to_files` out of
  ui_requests.rs must not move the lane guard's *timing* (it refuses
  before the state snapshot, journey US-2/S1) — the GUI seam test
  pins this; keep the helper's guard order verbatim.
- **Timeout semantics**: `--timeout-secs` on the Deck publish budgets
  the settling first sync; on the files path there is no settling
  Deck pull — keep the flag accepted-but-budgeting the bind await
  only (document in help text; no new semantics).
- **Prefill drift**: adding the `Publish` arm to `prefill_boards`
  touches the arm-enumeration unit tests in repl.rs — extend, never
  weaken, the enumeration (a new arm not in the list is a test
  failure, which is the point).
- **Census conflict**: parallel chunks (C10 et al.) may touch the
  census — rebase early, keep the flip to the two rows this verb
  owns.

## 7. Estimated diff

~250–400 lines across `cli.rs`, `commands/{mod,boards,output}.rs`,
`repl.rs`, `ui_requests.rs` (extraction), `docs/ux-surface-census.org`,
plus ~250 lines of tests in `tests/cli_standalone.rs`.
