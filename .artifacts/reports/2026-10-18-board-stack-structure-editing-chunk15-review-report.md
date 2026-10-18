# [Report] Chunk 15 stage-3 review — board/stack structure editing
(`feat/board-stack-structure-editing`)

- **Date**: 2026-10-18
- **Verdict**: **PASS** — three findings, all fixed on the branch by
  folding them into the feature commit that introduced them (detach →
  amend → cherry-pick replay of the six later commits; no
  duplicate-subject fixups, nothing was pushed before the surgery);
  full gates re-run green on the final head; PR open and green; never
  merged — the user merges on GitHub.
- **PR**: https://github.com/D4id4los/taskboard/pull/334
  (non-stacked; branch off `8232a27`; `origin/main` moved to `3ad16bb`
  during review — PR stays MERGEABLE with no conflicts).
- **Closes**: #132, #171, #177, #304 (via the PR body).

## 1. Functional review (dispatch prompt 15.3 focus points)

- **Clock semantics per verb vs the plan (§4 D2)**: each edit verb
  stamps exactly its own clock (`SetBoardColor` → `clocks.color`,
  `SetBoardArchived` → `clocks.archived`, both unit-pinned with the
  other clocks asserted untouched; `DeleteBoard` → `clocks.deleted`).
  `merge_board` is now a per-field verdict table (`remote_wins`
  strictly, per field); the LWW spot-check is pinned by
  `merge_board_field_verdicts_are_independent` (newer local
  color/archived survive a newer remote title edit and vice versa) and
  `merge_board_epoch_clocks_lose_to_any_remote_stamp` (0002 parity).
  `adopt_board_after_push` advances all four clocks to
  `max(local, echo.lm)`; adoption paths stamp all four with the remote
  stamp — whole-entity behavior unchanged.
- **Tombstone/ceremony end-to-end walk (delete → tombstone → push →
  remote)**: command time — board tombstone with binding kept (R9c),
  cascade tombstones stacks/tasks/labels + per-task reminder-rule
  deletes, every queued op of the board or its children `FailOp`-ed
  (no 404 chase through the corpse), exactly one `DeleteBoard` op
  enqueued (bound or unbound; planner noop-completes the unbound
  group). Push time — `plan_board_groups` delete-dominance subsumes the
  board's other ops (bound gate on the *binding*, not `!deleted`, so
  the tombstoned board's DELETE still goes out); executor arm mirrors
  `DeleteStack` (`PushCtx { existing: true, delete: true }`, shared
  `classify`). Finalize — the echo arm routes to
  `adopt_board_after_delete_push` (tombstone kept, deleted clock
  max-stamped, binding cleared, `remote_seen` = echo stamp), pinned at
  the pipeline level (`board_delete_push_echo_finalizes_...`) and
  wiremock level (`delete_board_sends_the_delete_and_applies_the_board_
  echo`; 404 → `RemoteMissing`). Same-cycle pull races are guarded
  twice: `merge_board`'s deleted-clock defense
  (`merge_board_tombstone_stands_against_a_pulled_live_board`) and the
  pipeline's `pending_board_intent`/`keep_board_intent` delete intent
  (restores the local tombstone when the delete was queued after the
  read). `RemoteMissing` on a board delete push tombstones via
  `tombstone_board` (idempotent with the command-time tombstone).
- **Exhaustive-match totality (chunk-8 contract)**:
  `PersistenceAction::ClearBoardContent` landed in the same commit
  (1fae345, verified via `git show --stat` + the commit message) as its
  `apply_actions` arm, its `execute_batch` arm, and the contract-walk
  arm; both matches are compiler-exhaustive (grep: no `_ =>` in either
  match), so a hypothetical new variant breaks the domain and storage
  builds until persisted. Verified: no wildcard crept in anywhere in
  the diff.
- **UI stack creation goes through the command engine (D9)**:
  `ActionId::StackCreate` → router `BeginCreateStack` → adapter
  refuses with no `presentation::live_board()` (the TaskCreate refusal
  class) → `ViewState::begin_create_stack` (insert mode) →
  `commit_edit` dispatches `StateCommand::CreateStack { title, order:
  append_stack_order(state) }` (typed command equality tests; empty/
  whitespace buffer commits nothing). UI-crate grep: the only
  `AppState` writes are `#[cfg(test)]` fixtures (lib.rs:621/774 inside
  the test modules); production UI code only loads the `ArcSwap` and
  emits commands. No board field entered `ViewState`/`IdTables`
  (F7-05 guardrail). Empty-board case tested
  (`stack_create_flow_works_on_an_empty_board`, order 0) and the
  board-layout empty state now names the affordance.
- **Sync story per verb matches the plan (D4)**: color/archive/rename →
  one full-send `UpdateBoard` PUT (renamed from `RenameBoard`;
  `MaterializedOp` renamed likewise; storage tag backfilled by
  migration 0005, decode rejects the legacy tag — 0003's backfill
  precedent, no alias drift); delete → `LocalOp::DeleteBoard` +
  `DELETE boards/{id}`; reset → **no op, local-only** with the
  recorded reason (pushing tombstones would destroy remote content)
  and the mandatory validator clear (the 304 re-materialization trap —
  pinned by S10 and the contract walk). No verb touches
  `SyncStatus`/`last_updated` (F3-04 guardrail).
- **Edge cases**: no live board → `NoBoard` (all four verbs, typed);
  tombstoned board → `NoBoard` for edits (the live-board guard) and
  accepted no-op for a delete repeat; absent board delete → `NoBoard`;
  unbound-board delete still queues the op (uniformity, planner
  noop-completes); archived board stays commandable (unit-pinned);
  already-empty reset → `NoOp` (zero actions); delete 404 →
  `RemoteMissing` classification shared with stacks.
- **Wave-wide test rules**: the one added `tokio::time::sleep` line
  (S10) sits inside the file's established
  `timeout(60s, poll-predicate)` harness (main already carries four of
  these sites; the scenario's clock is paused and advanced) —
  sanctioned predicate-with-deadline polling asserting a terminal
  state, not a fixed wait. No text assertions: the new CLI tests
  assert typed receipts/errors and repo-snapshot facts; snapshots use
  the suite's fixed clock (deterministic; no transient/wall-clock
  pinning found in s8/s9/s10 or the churned snaps — they document the
  tombstone cascade, the cancelled ops, and the single queued
  `delete_board` op exactly).
- **#207 fold**: no gap — Deck's board GET/PUT round-trips
  title/color/archived (verified against `client.rs` wire facts);
  recorded in the plan §3.2 with the `restore_board` out-of-scope trail.

## 2. Findings & fixes (all folded into the feature commit)

- **F1 — `cannot_ride_a_task_group` missing `LocalOp::DeleteBoard`**
  (sync `push_exec/outcomes.rs`). Unreachable today (the planner
  partitions ops by kind before any task group is built), but the
  filter is exactly the non-exhaustive-defense trap this wave keeps
  closing: the executor arms' comment "nothing else can reach the
  match" assumed the list was complete. Fixed by adding the variant.
- **F2 — outbox strategy stale at 14 op kinds**
  (`domain/test_support/state.rs`): doc comments said "all 14
  `LocalOp` variants" while the enum grew to 15, and
  `DeleteBoard` was never generated (`0_usize..14`). Fixed: range
  widened, arm 14 added, both comments corrected — the new verb now
  rides the randomized pipeline/state proptests.
- **F3 — `ClearBoardContent` reminder-fact clears over-broad in SQL**
  (storage `repo.rs`): the deliveries/snoozes deletes matched
  `reminder IN (SELECT 'rule:' || rule_id FROM reminder_rules)` —
  *every* rule in the DB — while the in-memory `apply_actions` prunes
  only the board's tasks' rules (a memory↔sqlite parity divergence the
  single-board contract fixture cannot see; a latent multi-board
  hazard against the F3-04 board-scoping guardrail). Fixed: the rule
  arm resolves via
  `reminder_rules WHERE task_id IN (board tasks)`, dropping the
  redundant `LIKE` guard; comment now states the scoping constraint.
  The `.sqlx/` offline cache was regenerated with
  `scripts/sqlx-prepare.sh` (two query hashes swapped) and committed
  with the change.

All three were applied at the commit that introduced the drift
(`1fae345` → amended as `b7889d0`), then the six later commits were
replayed with `git cherry-pick` (clean, no conflicts); the final tree
differs from the pre-review tree exactly by the three fixes + the
cache delta (`git diff eab9741 HEAD`).

## 3. Code-quality mandate

- **Crate boundaries (the SRP lens)**: domain stayed pure — clocks are
  data, `now` is injected, no IO/async/network dependency appeared
  (domain diff touches only entities/messages/outbox/command/merge/
  push/pipeline/persistence + test support). The UI gained no logic:
  the only new "decision" is the presentation-layer refusal (no live
  board) and the append-order default — both presentation concerns,
  both dispatched as typed commands. Storage owns only SQL; sync owns
  only wire choreography; the CLI owns the confirmation policy.
- **Single level of abstraction**: verb planners read as flat
  guard→mutate→enqueue sequences; `destruct_on` orchestrates
  `read_scope` → refuse-or-dispatch; the SQL clear is a commented
  statement sequence whose order satisfies the FK graph. No function
  mixes primitive shoveling with orchestration beyond that shape.
- **Naming/comments**: comments state constraints (R9c binding kept,
  "never reuse `adopt_board_after_push` for a delete", the 304 trap,
  the epoch-default meaning); the two stale "14 variants" comments
  were the drift the review fixed. No dead code or speculative
  generality found; the ADR amendment records the intended-behavior
  edges (tombstone never clears until restore exists; entity-
  granularity hazard unchanged).

## 4. Gates (re-run on the final head, all green)

`cargo fmt --check` ✓ · `cargo clippy --workspace --all-targets -D
warnings` ✓ · `cargo nextest run --workspace` 1056 passed / 28 skipped
✓ · `cargo test --doc --workspace` ✓ · `cargo deny check` ✓ ·
coverage three-command sequence ✓ (quarantine absorbed one torn
profraw per the 2026-10-13 protocol) · `scripts/sqlx-prepare.sh
--check` ✓ (offline cache fresh).
