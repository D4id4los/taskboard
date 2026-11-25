# The move order-key parity chunk plan (round 7, chunk B3)

- **Issue**: #1351 (`backlog: re-home or parity-pin the UI's move order-key
  minting against the domain's floor_midpoint_order`), p3, labels
  `architecture`/`refactoring`. Branch: `fix/move-orderkey-parity`.
- **Lineage**: #1247 (the move-geometry trio / B3-fix) — already largely
  landed on main; #1356 (PR #1398, the CLI `--before/--after` gap-index
  grammar) — landed; this branch starts from that post-B6 main
  (97372132).
- **Scope law** (dispatch plan §9): one mint — either the UI calls the
  domain's (re-homed) mint, or a parity pin fails on divergence; stage-1
  records which and why. This plan records **re-home**, rationale below.
- **Round laws**: [L](a) the parity claim is pinned by a test that fails
  on divergence, never prose; [L](c) one mint; [L](d) out-of-scope
  findings become `backlog:` issues (numbers at the bottom).

## 1. Diagnosis — the drift surface, post-#1247/#1356

Issue #1351's premise (2026-11-24) described two competing *midpoint*
policies: the UI minting `a + (b - a) / 2` in `slot_order` while the
domain owned `floor_midpoint_order` (`command.rs:749`). **That surface no
longer exists on main.** The #1247 lineage (landed as the B3 2026-11-24
commits) retired every move-path midpoint on both sides:

- **UI (`crates/taskboard-ui-slint/src/view_state.rs:806-971`)** —
  `slot_order`'s between-ends midpoint arm is deleted ("No move path
  computes a midpoint anymore"); `move_aim_command` (the one move-aim
  mint, keyboard confirm and pointer drop alike) sends same-stack aims
  as `StateCommand::TaskReorder { id, target: gap }` and keeps
  `MoveTask` only for cross-stack, using the two end arms
  (`slot_order` gap-0 = `first.order - 1`, otherwise
  `append_order` = `max.order + 1`).
- **Domain (`crates/taskboard-domain/src/command.rs:1484-1527`)** —
  `TaskMoveUp`/`TaskMoveDown` delegate to the same gap-index resolution
  `TaskReorder` speaks (`task_reorder_plan`: engine resolves the gap
  over the canonical `(order, id)` row source and renumbers dense).
  The old floor-midpoint slot arithmetic is retired;
  `floor_midpoint_order` itself no longer exists in the domain (grep:
  zero non-test hits workspace-wide).
- **Habits** — `HabitReorder` mints gap indices (`habit_move_slots`,
  presentation.rs), engine renumbers; same grammar as tasks.
- **CLI (#1356, post-B6 main)** — `tasks move --before/--after`
  resolves to a gap index via `neighbour_gap_target`
  (`crates/taskboard-app/src/commands/tasks.rs:352`) consumed by the
  same `task_reorder_plan`; the absolute `--order` leg survives with a
  documented dense-stacks caveat.
- **The domain's one surviving midpoint law** is
  `insertion_order_after` (`command.rs:865-886`, D-E4): a *spawn
  insertion* (completion successor), not a move — the UI never
  duplicates it. Its adjacent-gap-1 tie lands the spawn level with its
  head, resolved by the `(order, id)` sort / UUIDv7 mint order.

**Residual drift surface (what this chunk actually closes):**

1. **`multi_move_commands`' block-splice arithmetic** (`view_state.rs`
   ~887-963): for a same-stack multi-select drop into an interior gap,
   the UI computes `step = (b - a) / (k + 1)` and mints absolute
   `MoveTask` orders `a + step·(i+1)` — an order-placement policy the
   engine cannot see, with a documented collision-lenient append
   fallback (`b - a <= k` ⇒ append at tail, the aim silently lost;
   #1355). The engine's `task_reorder_plan` law evolves independently;
   nothing ties the UI's slice arithmetic to it.
2. **The append/end mints duplicated in the UI**: `append_order`,
   `slot_order` (two end arms), and `append_stack_order` encode
   "canonical append position" laws in the UI crate while the domain
   encodes its own append law (`insertion_order_after`'s
   `head.order + 1`; the engine's dense renumber). Today they agree on
   every reachable state, but they are two homes for one law.
3. **Stale prose** describing the retired midpoint law: domain
   `habit.rs:531` ("the UI computes midpoints on drag"), UI
   `adapter.rs:13180-13181` ("slots minted … with the same
   floor-midpoint law"), UI `router.rs:1768-1770` ("the engine's shared
   floor-midpoint rule resolves the slot"), app `cli.rs:1711` ("the UI
   computes midpoints, this verb stores the absolute value").

**Live divergence check**: none found — on every input class reachable
today the two homes agree (the UI's midpoint class is unreachable;
`multi_move`'s fallback is the only aim-loss, tracked as #1355). The
drift risk is prospective, exactly the shape #1351 names.

## 2. Decision — re-home (round law [L](c), arm one)

**The UI calls the domain's re-homed mint.** Move the order-minting
family into a new `crates/taskboard-domain/src/order.rs` module:

| Function | From | Notes |
|---|---|---|
| `append_order(&AppState, StackId) -> i64` | view_state.rs:806 | verbatim body |
| `slot_order(&AppState, StackId, usize) -> i64` | view_state.rs:824 | the two end arms, doc verbatim |
| `move_aim_command(&AppState, TaskId, StackId, Option<usize>) -> StateCommand` | view_state.rs:852 | returns the domain's own command enum |
| `multi_move_commands(&AppState, &[TaskId], StackId, Option<usize>, &[StackId]) -> Vec<StateCommand>` | view_state.rs:887 | **signature change**: takes the move ring as a caller-supplied `&[StackId]` slice instead of calling `ViewState::move_ring` — avoids cascading the re-home into `move_ring`/`live_boards`/`stacks_in_board` (kept UI-side; see backlog below) |
| `append_stack_order(&AppState, BoardId) -> i64` | view_state.rs:963 | verbatim body |

The UI crate keeps thin `pub use taskboard_domain::order::{…}` re-exports
in `view_state.rs` so call sites (adapter, actions, router) stay
unchanged; `ViewState::move_ring` remains the ring constructor the UI
passes in. Rationale for re-home over a parity pin:

- A pin only *detects* divergence after someone writes a second copy;
  re-home makes divergence impossible by construction — strictly
  stronger, and it is #1351's stated preferred option.
- Every function in the family is already pure over domain types
  (`AppState` lives in `taskboard-domain/src/state.rs`;
  `tasks_in_stack` is a domain method) — no slint types, no IO, no
  time, so the move breaks no crate boundary rule (AGENTS §2).
- The A1 type-driven policy keeps domain as the home of pure minting
  constructors at trust boundaries; order-key minting is exactly that
  shape.

**Why the parity pin still exists ([L](a))**: re-homing cannot pin
*semantics*, only *location*. The chunk therefore also pins the moved
laws' semantics with a divergence-failing test block in
`taskboard-domain/src/order.rs` `#[cfg(test)]` (the domain crate is the
lowest layer for these pure functions — AGENTS §5 "one behavior, one
test, lowest layer"; the UI crate's existing tests over the moved fns
move with them or are thinned to routing-only):

- `multi_move_splice_never_collides_with_a_remaining_row_when_room_allows`
  — proptest (AGENTS §5): dense remaining orders, gap strictly inside,
  `b - a > k` ⇒ every minted order strictly between `a` and `b`, distinct,
  and colliding with no remaining row. Fails if anyone changes the splice
  arithmetic into the collision-lenient class.
- `multi_move_dense_fallback_appends_after_the_last_remaining` — pins the
  `b - a <= k` arm structurally (orders ≥ `last + 1`, monotonically
  increasing in member order) — the permanent-contract half of #1355's
  proposed direction (the semantic *replacement* stays #1355's call, out
  of scope here).
- `append_order_is_last_order_plus_one` /
  `slot_order_gap0_is_first_order_minus_one` /
  `slot_order_interior_gap_is_the_append` — table pins over the two end
  arms (the exact law the deleted midpoint arm used to violate).
- `move_aim_same_stack_is_a_gap_index_and_cross_stack_is_the_end_arm` —
  the one-mint law: same-stack aims carry `TaskReorder`, never an
  absolute `MoveTask` order.
- `append_stack_order_is_max_live_stack_order_plus_one` — including the
  deleted-stacks-excluded arm.

**Stale prose** (items 3 above) corrected in the same PR — they name a
law that no longer exists and would re-teach it.

## 3. Out of scope (recorded, not done)

- **#1355** (open, p2): replacing the multi-move append *fallback
  semantics* with per-member reorder gap indices. This chunk only
  re-homes and pins the arithmetic; the semantic change stays #1355's.
  Comment posted with the post-#1356 state.
- `move_ring`/`live_boards`/`stacks_in_board` stay UI-side (they are
  presentation-ring concepts); re-homing them is a separate candidate.
- `insertion_order_after`'s adjacency tie (spawn lands level with its
  head; UUIDv7 mint-order luck) — in-code documented as accepted;
  backlogged for a deliberate tie law.

## 4. Implementation order (stage 2)

1. `feat(domain)`: new `order.rs` with the five functions + the pin
   block; `#![forbid(unsafe_code)]` already crate-level; SPDX header
   `MIT OR Apache-2.0`. Unit tests moved/adapted from view_state's
   `#[cfg(test)]` blocks over these fns (the lowest-layer duplicates
   move; UI-layer routing tests stay).
2. `refactor(ui-slint)`: view_state delegates to the re-exports;
   `multi_move_commands` call sites pass `&ViewState::move_ring(state)`.
3. `docs(domain,ui-slint,app)`: the four stale prose sites.
4. Quality gates: pre-commit hooks + `scripts/run-quality-gates.sh
   cargo nextest run --workspace` (flock-safe; every cargo via
   `scripts/memory-gate.sh`).

**Estimated diff**: ~400 lines (one new domain module ≈180 incl. tests,
UI delegation ≈30, moved/thinned UI tests ≈120, prose ≈15). Size S per
the dispatch plan.

## 5. Risks

- **`multi_move_commands` signature change** touches every multi-select
  drop call site — mechanical, but the engine-snapshot journeys (insta)
  must stay byte-identical (acceptance: move/append command streams
  unchanged). The pin block proves the arithmetic itself is unchanged.
- **Test-layer churn**: view_state's test module has deep coverage of
  these fns; the move must not silently duplicate a behavior test
  across layers (AGENTS §5). Mitigation: enumerate the moved fns' test
  names before touching anything; move, don't copy.
- **Public-API churn is internal** (workspace-internal API; blame churn
  not a decision variable per the type-driven policy note).
- **Deferred gate**: if the flock is held at stage end, commit and
  report "gate deferred: lock held by <holder>" per [D]; the
  dispatcher owns the full §10 suite between stages.

## 6. Acceptance criteria (mirror of #1351 + the dispatch law)

- [ ] Exactly one order-minting home: the UI's move/append paths call
      `taskboard_domain::order::*` (re-home recorded here, per [L](c)).
- [ ] The pin block fails on divergence: splice collision-freedom, end
      arms, aim-command shape, stack append (property + table pins in
      the domain crate).
- [ ] Move/append command streams byte-identical pre/post (journeys +
      engine snapshots unchanged).
- [ ] Stale midpoint prose gone (four sites).
- [ ] Gates green or gate-deferred per [D]; PR open, no push to main.
