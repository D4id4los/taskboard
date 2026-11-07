# [Plan] Hostable Sub-Surfaces — Design Round (W5b: the flow-nesting protocol)

- **Date**: 2026-11-07
- **Kind**: design-round doc — **W5b** (fenced by the GUI round-5 plan
  §3 D4 / §5, `.artifacts/plans/2026-11-05-gui-review-round5-plan.md`).
  Design only — no production code.
- **Scope**: `taskboard-ui-slint` only (`view_state.rs`, `adapter.rs`,
  `keyscope.rs`, `router.rs`, the hosting `.slint` compositions). No
  domain/state/engine change: guests dispatch ordinary
  `StateCommand`s; hosts see their effects through `AppState`.
- **Status**: **ACCEPTED** (2026-11-07, user) — Q1–Q7 ruled (§0);
  implementation is dispatched by
  `.artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md`.

## 1. Context & verified anchors (`main` @ `0448cceb`)

| Fact | Anchor |
|---|---|
| `Flow` is a flat XOR enum — "at most one" flow | `view_state.rs:220-326` |
| One per-kind settle taxonomy shared by every settle trigger (L1 focus loss, L2 pointer seams, leaving effects) | `ViewState::settle_flow`, `view_state.rs:2180-2235` |
| The conflict resolver: `resolve(FlowKind, EffectClass) -> Resolve {SettleThenRun, CancelOnly, Replace, Inert}`, total by compiler; the pair-table test pins every `(Flow, effect)` cell | `adapter.rs:3389-3430`, `flow_table_tests.rs` |
| Every modal/overlay flow (FocusMode, TrackEntries, TrackEntryForm, PlanPlace, ReviewWalk) collapses to `FlowKind::None` in the resolver; its kept arms own its intents | `adapter.rs:3295-3320` |
| The keybind scope stack is pulled per keystroke (no stored registrations); each modal is a hard-coded total-mask branch | `keyscope.rs:88-140` |
| The walk→planner leg is a cross-view **handoff** (shape B) via the adapter-local `walk_resume` hint | `adapter.rs:146,208,2051,2855`; review-walk plan D-W3 |
| DR-7 D-J9 **declined** the embedded planner: "merges two state machines … chunk 3 may ship the planner as a view, which a flow cannot embed" | `2026-11-05-journal-review-design-round-plan.md:441-515` |
| The planner shipped as a **view** (`ViewId::PlanNextDay`, `planner_return`), its form as `Flow::PlanPlace` with adapter-side `plan_time_text`/`plan_effort_text` | `view_state.rs:400,640-655`; `adapter.rs:201-202,1235` |

## 2. The problem: four hand-rolled nestings already exist

The XOR rule says "one flow"; the product already needs "a surface
inside a surface" and gets it by flattening, four different ways:

| # | Host → guest | How it is flattened today | Cost |
|---|---|---|---|
| N1 | Entries list → add/edit form | two sibling `Flow` variants (`TrackEntries`, `TrackEntryForm`), "layered in the Escape ladder" by convention | the ladder layering is per-arm code |
| N2 | Entries form → date picker | `TrackFormState.picker: Option<DuePickerState>` adapter-side, wiped "whenever the flow is not `TrackEntryForm`" | ad-hoc wipe rule, no settle of its own |
| N3 | Walk → File-to picker / Pick-a-date | sub-state baked into `WalkStage::{Inbox{filing}, CarryForward{picking_date}}` + adapter `walk_date_text` | the walk's stage machine carries a picker's phase; walk-specific ladder rung |
| N4 | Walk → planner | not nested at all: close walk, open view, `walk_resume` re-opens at Prompts | the handoff exists because nesting was impossible (D-J9's stated reason) |

Each copy re-invents the same four things: **who owns the guest's
state** (and where it is wiped), **how settle composes**, **which
resolver cell applies while the guest is live**, and **which key
scope wins**. W5b names one protocol for all four.

## 0. Session rulings (2026-11-07, user)

| ID | Ruling |
|---|---|
| Q1 | **(b)** — shape B stays; W5b's consumers are the existing hand-rolled nestings N1–N3 (N4 stays the handoff) |
| Q2 | **Superseded**: the user rejected the closed-guest / depth-1 shape — the protocol must be general and naturally recursive, with no host/guest annotations and no depth limit (revision 2 below) |
| Q3 | A foreign settle of a text level follows the **existing text rule** (commit non-empty, discard empty) |
| Q4 | A level whose subject dies **just disappears** (with everything above it; no exit delivered) |
| Q5 | Plan **approved** with the amendments; write the implementation dispatch plan |
| Q6 | **Accepted**: a surface kind at most once per chain — a limitation inherited from Slint's non-recursive components |
| Q7 | **The current view is the chain root, in this round** — views and flows are levels of one chain |

## 3. Decisions — revision 2: the recursive surface chain

The first draft's closed `Guest` enum, admission table, and depth-1
limit are **withdrawn**. Their stated reasons do not hold up:
admission is already structural (only a surface's own handler can
open a child on it), stale children cannot exist if closing a level
closes everything above it, and recursion depth is bounded by the
actual data.

### D-H1 — Representation: an owned chain of unchanged flows

```rust
pub enum Surface { View(ViewId), Flow(Flow) }
pub struct Level { surface: Surface, local: LevelState } // LevelState: buffers, focus anchor
pub struct Surfaces { chain: Vec<Level> }  // chain[0] is always a view; innermost last
```

- `Flow` variants are **unchanged**; no variant is marked host or guest.
  Any level may open a child; only the level's own handlers can (the
  structural admission rule — there is no other mount path).
- **Invariant (the only one)**: closing level *i* truncates the chain
  to *i*. A child never outlives its parent. Enforced by making
  `truncate` the sole removal API.
- The top-level XOR rule is the special case "replacing at depth 1"
  (the first level above the root).
- **The root is the current view (Q7)**: `chain[0]` is a view level
  (`Level::View(ViewId)`), every flow level sits above it. The ad-hoc
  return slots (`settings_return`, `planner_return`) retire: opening
  Settings or the planner over a view is a view level pushed above it,
  closing it truncates back — the exact restore by construction.
  Navigation that *replaces* the view (`view.today`, palette jump)
  truncates to the root and swaps it. The walk's `walk_resume`
  handoff (shape B, Q1) is unchanged in behaviour; whether it can be
  expressed as "walk level stays below the planner level" is a
  stage-1 decision of the core chunk, not a behaviour change.

### D-H2 — State ownership: per-level

- Pure interaction state stays in the `Flow` variant, as today.
- Adapter-side text (`TrackFormState`, `walk_date_text`,
  `plan_*_text`, the due buffers) moves into the level's
  `LevelState`, dropped with the level: one lifetime rule replaces
  every per-flow wipe rule.
- **`ViewState.buffer` becomes per-level** — the largest single
  migration in the round (every text flow's commit path).
- Zero shared state beyond `AppState` at every level: a level that
  writes the domain dispatches its own commands.

### D-H3 — Exit

`GuestExit { Completed(LevelResult), Cancelled }` delivered to the
parent's `on_child_exit`, **default: ignore**. A payload is used only
for parent-local continuation (File-to's `StackId`, Pick-a-date's
date). Subject death delivers nothing (Q4).

### D-H4 — Settle composition

- **Local** (focus loss / pointer seam / Escape rung inside a level):
  settle the **top level only** by its kind rule, deliver its exit,
  the parent stays.
- **Foreign** (R3 settle-then-run, leaving effects, `Replace`): fold
  the chain **innermost-first**, each level by its own kind rule
  (Q3), exits delivered as the fold goes; one pass, idempotent.
- The Escape ladder generalizes: a level's own rungs, then one
  `Cancelled` rung — never two levels in one stroke.

### D-H5 — Effect routing (the pair table stops growing)

- An effect is offered to the innermost level first; each level
  `Handled` or `Bubble`s it outward, consulting the **existing**
  `resolve(FlowKind, EffectClass)` table for its own kind. No
  (host, guest, effect) cells exist.
- Tests: the per-kind table stays as pinned; composition is a
  **`proptest`** over random chains asserting the invariants (child
  never outlives parent; foreign settle empties the chain innermost
  first; a `Handled` effect never reaches an outer level; settle is
  idempotent).

### D-H6 — Key scope

`scope_stack` concatenates each level's contexts innermost-first and
stops at the first masking context. The current modal branches become
each kind's declared contexts; still pulled per keystroke (D-KS1).

### D-H7 — Rendering & focus

- Slint cannot recurse (a component cannot instantiate itself), so
  each kind keeps fixed mount sites, gated on "this kind is in the
  chain". Window-anchored overlays (round-5 D6) make placement
  independent of depth.
- Consequence: a kind can be shown once per chain. **Proposed single
  rule (Q6)**: opening a kind already present in the chain is refused
  with a typed `AlreadyInChain` — not a depth limit, not a table.
- Focus: mount uses the existing `FocusHandover`; exit restores the
  anchor stored in the parent's `LevelState`.

### D-H8 — Implementation shape (for the dispatch plan)

1. `Surfaces` core with the view root (Q7) + routing + settle fold +
   proptest; `view`/`flow`/the return slots become the chain with
   behaviour byte-identical (all journeys and `flow_table_tests`
   green untouched).
2. Per-level `LevelState` + the `ViewState.buffer` migration.
3. N2 + N3: the entries-form date picker and the walk's File-to /
   Pick-a-date become child levels; `WalkStage` loses its sub-state
   fields.
4. N1: entries list → form becomes parent → child (no longer two
   sibling variants).

## 4. Non-goals / fences

- The embedded planner stage (Q1 = b): views-as-root makes it cheap
  later; not built here.
- Multiple live children per level (a tree): only one focus path is
  ever live; a chain suffices.
- Any engine/`AppState` change; the #622 contract is kept.

## 5. Open questions

None — all rulings in §0.
