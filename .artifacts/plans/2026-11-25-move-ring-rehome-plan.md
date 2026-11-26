# The move-ring re-home chunk plan (round 8, chunk E2)

- **Issues**: #1399 (re-home the move ring family into
  `taskboard-domain`), #1400 (the `insertion_order_after` adjacency tie
  resolves by UUIDv7 mint-order luck — the determinism law), both p3,
  labels `architecture`. Branch: `fix/move-ring-rehome`.
- **Lineage**: #1351 (the order-mint family re-home,
  `taskboard_domain::order`, commit 331d7a86) — landed; this chunk
  finishes the re-home it deferred. #1247 (the move-geometry trio) —
  landed; `insertion_order_after` is a *spawn insertion*, deliberately
  outside that retirement. #1355 (the multi-move fallback decision) —
  open, sequenced separately; NOT this chunk.
- **Scope law** (dispatch plan §9): the family lives in domain with the
  UI consuming it; the tie is deterministic and pinned; the corpus is
  behavior-identical. Stage 2's gate is the pre-commit hooks plus the
  flock-wrapped full `cargo nextest run --workspace`.
- **Round laws**: [L](a) typed outcomes (no text assertions — the tie
  pin asserts the spawn's position relative to its head, a structural
  fact); [L](c) the A1 pattern / no new validate-and-forget; [L](d)
  out-of-scope findings become `backlog:` issues (numbers at §6).

## 1. Diagnosis

### 1.1 The move ring family — current homes and consumers

All three functions are pure over domain `AppState` with zero slint
types — the exact argument #1351 used for the mint family.

| Function | Current home | Domain-pure? | Consumers |
|---|---|---|---|
| `move_ring(&AppState) -> Vec<StackId>` | `taskboard-ui-slint/src/view_state.rs:4113` (assoc. fn on `ViewState`) | yes | `ViewState::move_slots` (view_state.rs:4128), the multi-move aim (view_state.rs:4317 — the ring is `multi_move_commands`' wrap-order argument), view_state tests (:5368); adapter/models reach it only through those |
| `move_slots(&AppState) -> Vec<(StackId, usize)>` | `view_state.rs:4128` (assoc. fn) | yes (the flattened gap enumeration over `move_ring` + `state.tasks_in_stack`) | `cycle_move_target` (view_state.rs:4146), tests (:5504/:5519) |
| `live_boards(&AppState) -> Vec<&Board>` | `taskboard-ui-slint/src/presentation.rs:2683` | yes | the ring family; *and* broad UI use (models.rs:1107/1255/1344/3675, adapter.rs:5256, view_state.rs:2113) |
| `stacks_in_board(&AppState, BoardId) -> Vec<&Stack>` | `presentation.rs:2668` | yes | the ring family; lib.rs:4506, view_state.rs:1701, presentation tests (:3697/:5909) |

`move_ring` composes the latter two: `live_boards(state)` then
`stacks_in_board` per board, flattening to stack ids — the flat
cross-board candidate list wrapping across the board edge (chunk 10 D6),
no Inbox special-casing.

Domain-side land already in place: `taskboard_domain::order`
(`lib.rs:56`, commit 331d7a86) hosts `append_order`, `slot_order`,
`move_aim_command`, `multi_move_commands`, `append_stack_order`;
`multi_move_commands` takes the ring as a caller-supplied `&[StackId]`
— the seam this re-home plugs from the other side.
`AppState::tasks_in_stack` (state.rs) is the canonical `(order, id)`
row source the ring's slot walk reads.

### 1.2 The `insertion_order_after` tie — exact mechanism

`crates/taskboard-domain/src/command.rs:872-886` (private fn; sole
caller `completion_spawn` at :918, the completion successor's spawn):

- least live order strictly above `head.order` in the head's stack
  decides;
- head last ⇒ append at `head.order + 1`;
- `gap == 1` ⇒ the spawn lands **level with its head**
  (`successor.order == head.order`);
- otherwise floor midpoint `head.order + gap / 2`.

The tie: two live tasks share `order`. Every downstream read sorts by
the canonical `(order, id)` key (e.g. `AppState::tasks_in_stack`),
so the tie's *visible* outcome is decided by the id leg — UUIDv7
lexical order = mint order. That is #1400's luck class.

**The mechanism, precisely**: the spawn's id is minted inside
`completion_spawn` (`successor.id = ids.new_task_id()`, command.rs:914)
via the injected `dyn IdGenerator`; the head's id pre-exists. The
canonical sort then reads `spawn.id > head.id` as "spawn below head".
Whether that holds depends on the generator, not on any contract:

- **Tests/injected fakes**: deterministic ordered counters — the tie
  resolves deterministically in every fixture.
- **Production (`idgen.rs:51`)**: `Uuid::now_v7()` — and per the uuid
  1.24 source, `now_v7` uses `Timestamp::now(NoContext)`: **no
  monotonic counter**. Ids minted within the same millisecond are
  ordered by random low bits. A head completed within the same
  millisecond as its spawn's mint (vanishingly rare in production,
  but *routine in fast test walls* relying on real clocks) can invert
  the reading. This is a stronger form of the luck #1400 documents.

### 1.3 The deterministic key to record

The tiebreak key is the **canonical `(order, id)` pair**, with the id
leg read as *mint order*. For the tie to be contract-not-luck, the
contract that must hold is: **a spawn's id is minted strictly after
its head's**. This chunk records that key explicitly and pins the
reading; it does NOT change any persisted order value (a dense
renumber or a mint-before/after scheme would change the corpus —
out of scope, see §3.3).

## 2. Decision — the re-home map

**Home**: `taskboard_domain::order` (the family *is* an ordering
reader — `order.rs` already hosts the write-side mint family; the
ring is the read side of the same canonical law). No new module.

| Item | From | To | Notes |
|---|---|---|---|
| `move_ring` | `view_state.rs:4113` (`ViewState` assoc. fn) | `taskboard_domain::order::move_ring` | free fn over `&AppState` (the domain module's established shape — cf. `append_order`); doc comment (the chunk-10 D6 law, wrap, no Inbox special-casing) travels verbatim |
| `move_slots` | `view_state.rs:4128` | `taskboard_domain::order::move_slots` | pure gap enumeration; doc travels |
| `live_boards` | `presentation.rs:2683` | `taskboard_domain::order::live_boards` | canonical board reader (BTreeMap order, live = not deleted, not archived); doc travels |
| `stacks_in_board` | `presentation.rs:2668` | `taskboard_domain::order::stacks_in_board` | the `(order, id)` stack reader; doc travels |

**UI consumption**: call sites switch imports to
`taskboard_domain::order::{...}` — direct, no re-export shim through
`presentation` (a shim would keep the false impression that
presentation owns the readers; the crate boundary law wants the UI as
a dumb client reading domain pure fns). Affected call sites:
view_state.rs:22 (the `use`), :1701, :2113, :4114-4116 (the moved
bodies), lib.rs:4506, models.rs (5 call sites), adapter.rs:5256 —
plus the `ViewState::move_ring`/`Self::move_slots` internal callers
(:4129/:4146/:4317) which keep their `ViewState` method surface? **No
— decision: the `ViewState` assoc. fns are deleted**, callers use the
domain fns directly. `ViewState` hosts no domain-pure logic after this
chunk (the #1399 acceptance, stated flatly).

**Tests that move with their behavior** (one behavior, one test,
lowest layer — AGENTS §5): `stacks_in_board_filters_and_orders`
(presentation.rs:3697) and `live_boards_filters_to_live_boards_in_
canonical_order` (:3741) move into `order.rs`'s test module;
`the_move_ring_is_the_flat_cross_board_candidate_list`
(view_state.rs:5368) and the `move_slots` tests (:5504/:5519) move
likewise. The presentation.rs:5909 reference-sort cross-check
(`stacks_in_board_matches_reference_sort`) moves with it. UI-layer
tests that exercise the ring *through* flows (keyboard cycle,
multi-move) stay put and change only the call path.

**`archived_boards` (presentation.rs:2695) stays** — #1399 names
exactly the three-family; the archived reader is the archived-roster's
UI concept with no ring consumer. Recorded as a deliberate boundary
(documented leniency at the call site is already present).

**Behavior identity**: pure code motion + import rewrites; no
arithmetic, filter, or sort predicate changes. G1's deterministic
goldens are untouched by definition (no render-path input changes);
the corpus proof is stage-3's (§5).

## 3. Decision — the tie determinism

**Chosen: pin the mint-order reading with typed tests + an explicit
recorded key** (#1400's lighter option), because the heavier options
(dense renumber on tie; mint the spawn strictly-after via order
scheme) change persisted orders and/or minting — not behavior-
identical, and the in-code comment already accepts the landing as
deliberate ("a spawn landing level with its head, not the move
geometry defect").

Stage 2 delivers:

1. **The recorded key** — `insertion_order_after`'s doc comment states
   the tie law as contract: the adjacent-gap tie lands the spawn level
   with its head and the *canonical `(order, id)` sort* breaks it; the
   contract is that the spawn's id is minted strictly after its head's,
   so the spawn deterministically renders below its head; the
   production generator's same-millisecond caveat is named and pointed
   at the backlog issue (§6, #1488).
2. **The tie pin (typed, lowest layer)** — a domain test over
   `insertion_order_after`'s visible outcome via the completion-spawn
   plan: a stack with two adjacent tasks, complete the upper, assert
   the successor's `(order, id)` sorts immediately after its head in
   `tasks_in_stack` order, using an injected ordered `IdGenerator`
   (the deterministic-by-construction law; no sleeps, no wall clock).
   A second leg pins the append arm and the midpoint arm unchanged
   (the corpus guard — if stage 2 accidentally alters `gap >= 2`, this
   fails).
3. **No production-code change to the tie arithmetic.** If the pin
   fails, that is a bug per AGENTS §5 — but the diagnosis says it
   cannot fail under ordered generators.

**Residual (recorded, not fixed here)**: production `Uuid::now_v7()`
(NoContext) is not monotonic within a millisecond — the contract's
production leg rests on mint instants being distinct, which is
empirically true at human completion speed but not enforced. The fix
(a process-global `ContextV7` counter in `idgen.rs`, making v7
mint-order a real total order workspace-wide) is **#1488**, out of
scope: it changes every tie in the corpus, not just this one, and
deserves its own behavior-identity proof.

## 4. Stage-2 work list (in order)

1. Move the four fns into `taskboard_domain::order` (free fns over
   `&AppState`, `#[must_use]`, docs verbatim); one commit
   `refactor(domain): re-home the move ring family from the UI`
   including the moved tests.
2. Rewrite UI call sites/imports (view_state, presentation callers,
   lib.rs:4506, models.rs, adapter.rs:5256); delete the `ViewState`
   assoc. fns; commit `refactor(ui): consume the re-homed move ring
   from the domain`.
3. The tie contract: doc-comment rewrite on `insertion_order_after`
   + the two typed pin legs; commit
   `test(domain): pin the insertion_order_after adjacency tie reading`.
4. Gates: pre-commit hooks per commit; the full suite through
   `scripts/run-quality-gates.sh cargo nextest run --workspace` (all
   cargo through `scripts/memory-gate.sh`; background + poll; commit
   landed work before any wait; defer per the #879/#894 rule if the
   lock is contended). Every cargo invocation routed through the gate
   (the 2026-11-23 systemd-oomd law).

Every commit stands alone (build + subject + no WIP noise — the
rebase-merge hygiene law).

## 5. Stage-3 traps (review focus)

- **Behavior-identical corpus proof**: diff the *moved function
  bodies* token-for-token against their pre-move selves (the move
  commits must show pure relocation — a changed filter/sort predicate
  inside a moved body is a shipping blocker, not a drive-by fix).
- **Test identity without duplication**: the moved tests assert the
  same facts at the new home; the UI flow tests keep passing
  unchanged in *assertions* (call-path edits only). A UI test
  deleted-and-re-added at the domain layer is duplication per AGENTS
  §5, not a move.
- **The tie pin must be typed**: `tasks_in_stack` position facts, no
  text/log assertions; the generator must be injected (a wall-clock
  fixture here is a clock-discipline violation — §6.1).
- **Boundary check**: zero new `taskboard-ui-slint → IO` edges; the
  domain additions use no slint/async types (the #1351 argument,
  re-verified per fn).
- **No silent `archived_boards` move** — the scope is the named
  family; scope creep shows up as an unexplained fourth function.

## 6. Backlog issues opened at stage 1

- **#1488** — `backlog:` `Uuid::now_v7()` mints with `NoContext` (no
  monotonic counter): same-millisecond ids are not order-guaranteed;
  adopt a process-global `ContextV7` in `taskboard_domain::idgen` so
  `(order, id)` tiebreaks read mint order as a contract
  (labels: `architecture`, `p3`).

## 7. Acceptance restated

- The family (`move_ring`/`move_slots`/`live_boards`/`stacks_in_board`)
  lives in `taskboard_domain::order`; `ViewState` hosts none of it; the
  UI consumes the domain fns directly (#1399).
- The tie's deterministic key — canonical `(order, id)`, spawn-id
  strictly after head-id — is recorded in the doc contract and pinned
  by typed tests over an injected generator (#1400).
- The corpus is behavior-identical: pure moves, unchanged arithmetic,
  goldens untouched, full suite green through the flock-wrapped gate.
