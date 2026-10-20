# Chunk 7 stage-2 report — UI board dimension: view state, switcher actions, board bar

- **Date**: 2026-10-20
- **Branch**: `feat/ui-board-switcher` (worktree
  `../taskboard-WT-feat-ui-board-switcher`), base `885f72c` (chunk 6
  merged via #376), 6 commits, **not pushed** (handoff at gates-green;
  the user opens/merges the PR).
- **Contract**: `.artifacts/plans/2026-10-20-multi-board-ui-board-switcher-plan.md`
  — implemented in full (§5 TDD order T1–T14, §6 slices 1–6); no
  design §5 ACCEPTED row touched (MD-7 clean); no tracker issue
  touched (MD-4 clean); no push, no PR (stage-2 boundary).

## Commits

| Commit | Content |
|---|---|
| `eaf7947` | feat(ui): presentation board-order helper + per-board sync badge (T1/T2) |
| `e687aac` | feat(ui): ViewState board selection — seed, select_board, the no-wrap walk (T3–T7) |
| `a55551c` | feat(ui): board.next/previous/select actions, router rows, preset keys (T8/T9 + keymap dump regen) |
| `9c84384` | feat(ui): models push from the selected board + the board bar strip (T10/T11) |
| `6d384c1` | test(ui): the keyboard-journey board leg + click-mid-flow pin (T12/T13) |
| `7f8ad3b` | chore(ui): regenerate board goldens for the bar + the changelog line (T14/D13) |

## Gate results (AGENTS §10)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | green |
| `cargo clippy --workspace --all-targets -- -D warnings` | green |
| `cargo nextest run --workspace` | green — 1275 passed, 0 failed, 31 skipped |
| `cargo test --doc --workspace` | green |
| `cargo deny check` | green (advisories/bans/licenses/sources ok) |
| `cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report` | green — 1275 passed |
| `scripts/coverage-quarantine-profraw.sh target/llvm-cov-target` | green — all 1531 profraw files valid |
| `cargo llvm-cov report --lcov --output-path lcov.info` | green |
| trailing `rm -rf target/llvm-cov-target` | done |

## Structural mandates verified

- **Dumb-client rule (absolute)**: `select_board`/`next_board`/
  `previous_board` return only `RefreshNow`-class outcomes; the router's
  board rows never produce `DispatchOutcome::Command` (pinned in T5,
  T6, T10, T11, T12 at every layer); board switching touches no
  engine command channel anywhere.
- **Adapter ends with zero non-seed `live_board` reads** (the §2 trap):
  the BeginCreateStack guard moved into
  `view_state::begin_create_stack(&state)` (D6 item 7); the only
  presentation board-order reads left outside `view_state` are
  `live_boards` (the bar's segment set, models.rs) — `live_board`
  survives only as the seed rule's pick.
- **Every chunk-2 UI interim fill replaced**: both
  `INTERIM(multi-board-2 → chunk-7)` markers gone (grep clean); the
  CreateStack commit names `self.board`; `stack_ids`/`reading_order`
  are instance methods; the two lib.rs assoc-fn test uses migrated to
  reconcile-seeded instance reads.
- **`IdTables` no re-keying** (§4 of the plan): one added field
  `board_ids: Vec<BoardId>` (the bar's index table); the content
  tables stay single-board-per-refresh.
- **TDD order**: every test written/landed with its slice; typed /
  structural assertions only; deterministic (no sleeps, no wall-clock
  pins beyond the existing one-second banner tolerance the chunk
  doesn't touch).

## Deliberate regenerations (D10)

- **Keymap snapshots** (in `a55551c`, hand-reviewed): the registry
  dump grew exactly the three board rows (global/board/stack-detail/
  task-detail contexts, navigation, enabled); the resolved-preset dump
  grew exactly the six `b`/`B` bindings (two per preset). Nothing else
  moved.
- **Goldens** (`7f8ad3b`): the four non-compact PNGs (0.5/3.0 ×
  1000/1500) rewritten by the strip; the **two 600 px compact goldens
  hold untouched** — the compact-hides-the-strip pin at the pixel
  tier (the compare-only run passes). The PR's PNG diff is the review
  artefact.

## Deviations from the plan (MD-7, for the stage-3 reviewer)

1. **Keymap dump regen rides slice 3, not slice 6** (plan §6 put both
   regens in slice 6). Reason: the registry/preset growth of slice 3
   makes the snapshot tests fail, and "every slice passes the full
   gate set / every commit standalone" outranks the slice grouping;
   the goldens (which depend on slice 4's strip) stayed in slice 6 as
   planned. The D10(a) hand-review happened at the regen (three rows +
   six bindings, nothing else).
2. **`sync_badge` switched via a staged companion function.** Plan D8
   has the signature gain the board in slice 1, but the only consumer
   (`models::push_status_bar`) reads `vs.board()`, which exists only
   after slice 2 — an in-place signature change would have forced
   either a temporary `None` fill (an interim read, forbidden) or a
   red build. Slice 1 added `sync_badge_for_board` beside the
   aggregate-based `sync_badge` (doc-commented as the superseded read
   path); slice 4 switched the consumer and deleted the aggregate
   function. Net end-state is exactly the plan's: one per-board read,
   zero aggregate display reads in the crate.
3. **The moved `begin_create_stack` guard landed in slice 2, not
   slice 3** (plan §6 slice 3 lists D6 item 7). Reason: slice 2's
   signature change (`begin_create_stack(&state)`) forced the adapter
   call site to move in the same commit; the guard moved with it (one
   home, as D6 wants — the adapter's inline `live_board` pre-check
   became redundant and was dropped there).
4. **`Adapter::view_state` widened to `pub(crate)`** (slice 5). Reason:
   T13 (click-mid-flow) must seed the buffer mirror and read the flow;
   the pre-existing seam fields were already crate-visible
   (`quit_requests`). Test-seam only, no new production surface.
5. **Coverage gate cadence**: fmt/clippy/nextest ran per slice;
   the CI-identical llvm-cov three-command sequence ran once at the
   chunk end (final handoff gate), not per slice — the plan's §6
   per-slice full-gate reading is otherwise honoured.

## Backlog candidates recorded (D11, not built)

- Config-pinned fixed-board kiosk mode (out per plan §1).
- Force-hiding the board strip at all kiosk widths would need a kiosk
  global the design does not name — speculative prep, not built.

## Handoff

Branch `feat/ui-board-switcher` at `7f8ad3b`, gates green, worktree
intact for the stage-3 reviewer. No push, no PR, no merge (MD-1/MD-2);
CHANGELOG Unreleased line landed (D13); no tracker issue touched
(MD-4).
