# Outbox Stranded Ops — Chunk Plan (Round 5, A5 / #1150)

- **Date**: 2026-11-22
- **Kind**: chunk plan — stage-1 output for chunk A5 of the round-5
  dispatch plan (`.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`,
  prompt A5). Scope report §A (2026-11-22) is normative; issue #1150
  is the divergence report; origin context:
  `.artifacts/plans/2026-11-21-sync-report-attribution-plan.md` §1/§2
  (the S1 chunk that discovered the divergence).
- **Branch**: `fix/outbox-stranded-ops` (worktree
  `../taskboard-WT-fix-outbox-stranded-ops`, branched off `origin/main`
  @ `51d99b2a`).
- **Stage-2 gate**: pre-commit hooks +
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (flock-safe; background + poll; commit landed work before any wait;
  defer per the #879/#894 rule when the lock is held). No coverage
  trio inside the subagent.

## 1. The strand scenario, diagnosed end-to-end

`PendingOp::owning_board` (`crates/taskboard-domain/src/outbox.rs`
~243) documents itself as returning `None` when the owning entity row
no longer exists, but the five board-carried variants
(`UpdateBoard`, `DeleteBoard`, `DeleteRemoteCard`, `DeleteRemoteStack`,
`FlushBoardAppData`) return `Some(board)` **unconditionally** — the
board id rides the op, so no row is consulted.

Routing consequences when the board row is wiped while ops remain
queued (undo of a board creation removes the row
(`undo.rs` ~486); data-degradation postures and external state surgery
remove rows without the command-layer cancellation; note
`ClearBoardContent` explicitly never touches the board row
(`persistence.rs` ~715) and `DeleteBoard` tombstones with binding kept
until push confirm (R9c), so the *commanded* flows are safe — the
strand is the uncommanded-wipe class):

1. **No cycle matches.** The Deck connector plans one cycle per
   *owned, bound* board (`BoardOwnershipMap::from_boards(&persisted.boards)`;
   `owned_boards`). A wiped row is unbound → no cycle for that board →
   the cycle filter `owning_board(&tables) == Some(cycle_board)`
   (`taskboard-sync-nextcloud/src/actor.rs` ~562) never selects the op.
2. **The orphan lane skips it.** The lane filters
   `owning_board(&tables).is_none()` (`actor.rs` ~537) — the op
   returns `Some`, so it is neither an orphan nor cycle-addressed.
3. The op sits in the outbox forever: `pending_ops` inflates, and
   every cycle re-scans it without resolving it. The Files connector
   has the same shape (only a `== Some(board)` filter,
   `board_cycle.rs` ~1010; it has no orphan lane at all).

The planner is **not** the gap: for a board op whose row is absent,
`plan_board_groups` (`crates/taskboard-domain/src/push.rs` ~607)
already degrades the group to `MaterializedOp::Noop` (synthetic
completion), and for ref-deletes `plan_ref_card_groups` materializes
the remote delete from the ref alone — no local row needed. The
executor's `classify_push` answers `PushResult::RemoteMissing` on a
404 delete (`mapping.rs` ~609), so an attempted delete against an
already-gone remote row *resolves* the op. The whole failure is
**routing**: the op never reaches a planning pass.

## 2. Decision — the doc is true for all five variants

**`owning_board` returns `None` for every board-carried variant when
the referenced board row is absent from `EntityTables.boards`.**

```rust
Self::UpdateBoard(board)
| Self::DeleteBoard(board)
| Self::DeleteRemoteCard { board, .. }
| Self::DeleteRemoteStack { board, .. }
| Self::FlushBoardAppData(board) => {
    // Doc-truth (#1150): a wiped row routes the op to the orphan
    // lane; the planner degrades row-targeting groups to synthetic
    // no-ops and materializes ref-deletes from the remote ref alone.
    entities.boards.contains_key(&board).then_some(board)
}
```

Typed semantics per variant class:

- **Row-targeting board ops** (`UpdateBoard`, `DeleteBoard`,
  `FlushBoardAppData`): `Some(board)` iff the board row exists. A
  `None`-routed one rides the orphan lane (first owned board's
  planning pass, once per cycle) and `plan_board_groups`' existing
  missing-entity arm synthetically completes it (`MaterializedOp::Noop`
  — nothing is sent). This mirrors exactly how stack/label/task ops
  already behave when their entity rows are wiped: `owning_board`
  resolves through the entity graph and yields `None`.
- **Ref-deletes** (`DeleteRemoteCard`, `DeleteRemoteStack`): same
  predicate, different consequence. The `board` field is the
  Deck-tagged routing fact, and the *remote ref itself* is the op's
  payload — "the op carries enough to route" is already satisfied.
  Orphan-lane routing materializes the delete from the ref
  (`plan_ref_card_groups` needs no local rows); the executor
  addresses the remote row directly. Outcomes: `Applied` (deleted) or
  `RemoteMissing` (404/403 — already gone), both of which complete the
  op and drain it; `Transport`/`RateLimited` abort the cycle as usual
  and the op retries next cycle.

### Why this side wins — the convergence argument

The round frame offers: (a) the doc is right, board ops return `None`
on a wiped row; or (b) the orphan lane gains a board-op catch. The
test is *which choice can't strand an op and can't double-apply one*.

1. **Single source of truth.** Under (a) the row-existence predicate
   lives in exactly one place (`owning_board`) and every consumer —
   the Deck cycle filter, the Deck orphan lane, the Files
   `board_cycle` filter, any future lane — routes consistently for
   free. Under (b) each lane must re-implement "board op whose target
   row is absent" next to its `is_none()` filter, a second predicate
   site per connector that can drift (the Deck and Files connectors
   already drift today: only one has an orphan lane). Drift between
   lanes is precisely how this bug was born.
2. **Can't double-apply.** The strand-adjacent fear is that routing a
   board op into a foreign board's cycle re-executes it. It cannot:
   row-targeting groups on an absent row materialize `Noop` (nothing
   is sent; the op completes synthetically exactly once), and
   ref-deletes are idempotent deletes whose every terminal outcome
   (`Applied`, `RemoteMissing`) completes the op once, removing it
   from the outbox.
3. **Can't strand.** Under (a) every wiped-row board op acquires a
   lane: the orphan lane picks it up (predicate is now `true`), the
   first owned board's pass plans it, and the planner's existing
   degradation resolves it. Under (b) with a catch that only *drains*
   row-targeting ops, the ref-deletes would face a choice — attempt
   the remote delete (needs the lane to build an executor for a board
   that no longer exists locally) or stay queued (still stranded).
   (a) gives both classes a uniform answer with zero new planner
   code.
4. **The doc becomes true.** The documented contract ("`None` when
   the owning entity row no longer exists") is the invariant callers
   already code against; behavior aligning to documentation is
   strictly smaller than documenting a per-variant special case and
   teaching two connectors a second catch.

### Healthy-path invariant (the sync corpus is unchanged)

When the board row exists, `contains_key` is `true` and the return is
`Some(board)` — byte-identical routing on every healthy path: cycles
plan the same board ops, the orphan lane stays empty for live boards,
`plan_pushes` groups identically, and every existing planner /
pipeline / connector test's inputs and expected sync corpus are
untouched. The only behavior delta is the wiped-row class, which
previously had *no* behavior (strand).

### Rejected alternative, recorded

**Blanket orphan-lane catch (b)** — rejected per the convergence
argument above (per-lane predicate duplication; no uniform answer for
ref-deletes; doc stays false). **Cancelling wiped-row board ops at
wipe time** — unreachable in general: the uncommanded wipe class
(undo of board creation, degradation, import folds) has no single
choke point where "this board is going away" and "these ops address
it" meet, and retro-fitting one is a round-scale refactor, not a
routing fix.

## 3. Implementation

### 3.1 `owning_board` doc-truth (outbox.rs) — the whole fix

- Apply the §2 match arm; rewrite the doc comment to state the two
  consequences per class (row-targeting → synthetic no-op via the
  orphan lane; ref-deletes → remote-addressed delete via the orphan
  lane) and that a live row yields the unchanged `Some(board)`.
- No other production code changes: the planner arms, the Deck actor's
  orphan lane and cycle filter, and the Files `board_cycle` filter all
  consume `owning_board` unchanged.

### 3.2 Red-then-green pins (TDD; the strand scenario pinned first)

Order: write 3.2.a, watch it fail (red), then 3.1, watch it pass
(green), then 3.2.b/c as regression pins.

- **3.2.a — the strand pin (the red test).** Deck-connector tier
  (`taskboard-sync-nextcloud` tests, the established wiremock/fake
  transport pattern of the actor tests): seed a persisted state whose
  outbox holds a `UpdateBoard` op, a `DeleteBoard` op, a
  `FlushBoardAppData` op, a `DeleteRemoteCard`, and a
  `DeleteRemoteStack`, all addressing a board id whose row is
  *absent* from `persisted.boards`, plus one healthy owned board so a
  cycle exists. Run one cycle. Assert structural outcomes, not text:
  every seeded op is **gone from the outbox** (drained) —
  row-targeting ones resolved via synthetic completion, ref-deletes
  via `Applied` or `RemoteMissing`. Assert the healthy board's
  operations are unchanged. Red today: the five ops survive the
  cycle (the strand).
- **Fixture note (recorded deviation, stage 2)**: the I9/I10 pins
  landed in `tests/suite/e2e_hermetic.rs` (the full boot + wiremock
  harness with `seeded_state`/`boot`/`nudge_cycle` seams) rather than
  `tests/suite/actor_wiremock.rs` — same crate, same tier, same
  wiremock pattern; the hermetic suite already carries the I-series
  end-to-end pins and the seeded-state/boot fixtures these pins need,
  so it is the lower-friction home. Harness choice only, no contract
  change.
- **3.2.b — the `owning_board` unit pins (domain).** In the existing
  `outbox.rs` test module: for each of the five variants, a wiped row
  yields `None` and a present row yields `Some(board)` — including
  the presence case for ref-deletes (unchanged `Some`). One behavior,
  one test, lowest layer: this is the predicate's home; the connector
  test (3.2.a) tests routing only.
- **3.2.c — the no-double-apply pin.** Extend 3.2.a's scenario (or a
  sibling): a `DeleteRemoteCard` whose remote row the fake transport
  answers 404 for completes with `RemoteMissing` and is drained
  exactly once — a second cycle performs no further pushes for it
  (the outbox no longer holds it). Guards the "idempotent drain"
  half of the convergence argument.

### 3.3 Explicitly out of scope (recorded, with issues filed)

- **Empty-owned-set stranding**: the orphan lane rides "the first
  owned board's planning pass"; with zero owned boards, orphan ops —
  including our newly-routed ones — strand unattempted. Pre-existing,
  documented in the actor comment ("Empty set ⇒ they strand
  unattempted"). Backlog issue filed; not fixed here.
- **The Files connector has no orphan lane**: a wiped board row is
  unbound, so no files cycle exists for it either — net Files-tier
  behavior is unchanged by this chunk — but the entity-wiped orphan
  class (task/stack/label ops included) has never drained in the
  Files tier at all. Backlog issue filed; adding the Files-tier
  orphan catch is a separate chunk.

## 4. Open questions — all resolved

| Question | Decision |
|---|---|
| Doc-truth vs orphan-lane catch? | Doc-truth (§2): `owning_board` consults `entities.boards` for all five board-carried variants. |
| Do ref-deletes get `None` too, or keep unconditional `Some`? | `None` on a wiped row, same predicate (§2): the ref is the routing payload; the planner materializes from it; `RemoteMissing` resolves. Uniform predicate beats per-variant splits. |
| Can routing a wiped-row op through the orphan lane double-apply? | No: `Noop` sends nothing; ref-deletes are idempotent and terminal outcomes complete the op once (§2.2, pinned in 3.2.c). |
| Does the healthy-path sync corpus change? | No: row present ⇒ `Some(board)` ⇒ identical routing (§2 invariant; the full existing suite is the pin). |
| Does the Files connector change? | Its filter consumes the shared `owning_board`; for files-bound boards the wiped-row class was equally unrouted before (no cycle) and stays so (no orphan lane) — net zero delta; the gap is filed as backlog (§3.3). |
| Empty-owned-set stranding? | Pre-existing, documented; filed as backlog (§3.3), not in this chunk. |

## 5. Acceptance checklist (stage 2)

- [ ] 3.2.a red-then-green demonstrated (strand → drain).
- [ ] 3.2.b/3.2.c pins green.
- [ ] `cargo fmt/clippy/nextest/doc-test` clean via
  `scripts/run-quality-gates.sh`; `cargo deny check` clean.
- [ ] Diff confined to `crates/taskboard-domain/src/outbox.rs` +
  connector/domain test files.
- [ ] #1150 closed by the PR with the decision summary; backlog
  issues cross-referenced.
