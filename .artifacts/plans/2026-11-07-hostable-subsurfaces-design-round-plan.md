# [Plan] Hostable Sub-Surfaces — Design Round (W5b: the flow-nesting protocol)

- **Date**: 2026-11-07
- **Kind**: design-round doc — **W5b** (fenced by the GUI round-5 plan
  §3 D4 / §5, `.artifacts/plans/2026-11-05-gui-review-round5-plan.md`).
  Design only — no production code.
- **Scope**: `taskboard-ui-slint` only (`view_state.rs`, `adapter.rs`,
  `keyscope.rs`, `router.rs`, the hosting `.slint` compositions). No
  domain/state/engine change: guests dispatch ordinary
  `StateCommand`s; hosts see their effects through `AppState`.
- **Status**: **DRAFT — awaiting the user's rulings on Q1–Q5 (§5)**.
  Every decision below carries a recommendation; none is binding
  until ruled.

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

## 3. Decisions (recommendations — rule via §5)

### D-H1 — Representation: a closed `Guest` enum, depth exactly 1

```rust
pub enum Guest {
    DatePicker { field: GuestDateField, phase: PickerPhase },
    StackPicker { candidates_rev: u64, selected: usize },
    EntryForm { seed: Option<TimeEntryId> },
    // PlannerSurface { .. }  — only if Q1 = (a)
}
// host variants that admit guests gain one field:
Flow::ReviewWalk { cadence, reference, stage, cursor, guest: Option<Guest> }
```

- A guest is **not** a `Flow`: it cannot host (depth 1 by type), it
  cannot be opened outside a host, and `reconcile` reaches it only
  through its host.
- **Admission** (which host may carry which guest) is enforced at one
  minting site, `ViewState::mount_guest(guest) -> Result<_, GuestRefused>`
  (house A1: fallible constructor at the boundary), and pinned by the
  **admission table** test (D-H5).
- Alternatives recorded: (i) recursive `guest: Box<Flow>` — every
  illegal nesting representable, settle recursion unbounded;
  (ii) a `Vec<Flow>` stack — reintroduces exactly the "flow outlives
  its affordance" class the scope-stack audit retired; (iii) per-host
  guest enums (`WalkGuest`, `EntriesGuest`) — illegal pairs
  unrepresentable, but the protocol code (settle, resolve, scope)
  needs a trait over them; revisit only if the admission table grows
  past ~6 cells (A2 entry criterion).

### D-H2 — Guest state ownership: the guest owns everything, the host sees only the exit

- **Pure state** (phase, cursor, selection) lives in the `Guest`
  variant's data inside the host flow — refresh-survival is
  model-push by construction, like every flow today.
- **Adapter-side text** moves into **one** slot,
  `Adapter.guest_buffers: Option<GuestBuffers>`, created by the mount
  and dropped by the exit — the single wipe site replacing
  `TrackFormState`'s "wipe when not TrackEntryForm", `walk_date_text`,
  and the due picker's ad-hoc lifetimes. `GuestBuffers` is keyed by
  the guest's discriminant; a read with the wrong key is a typed miss.
- **The shared `ViewState.buffer` belongs to the host.** A guest never
  touches it (no text-flow host may lose its buffer to a guest).
- **Zero shared state beyond `AppState`** (the #622 contract,
  generalized): a guest that mutates the domain dispatches its own
  `FlowOutcome::Command`s; the host learns of them on the next push
  like any other state change.

### D-H3 — The exit protocol: `Completed(payload) | Cancelled`

```rust
pub enum GuestExit { Completed(GuestResult), Cancelled }
pub enum GuestResult { Date(NaiveDate), Stack(StackId), Entry, /* … */ }
fn on_guest_exit(&mut self, exit: GuestExit, state: &AppState) -> Vec<FlowOutcome>;
```

- The payload exists only for **host-local continuation** (File-to's
  picked `StackId` becomes the walk's `MoveTask`; the picked date
  becomes Carry forward's `UpdateTask`). Domain writes the guest
  makes itself (the entry form's `CreateTimeEntry`) carry no payload.
- The host is the only consumer of an exit; a host whose flow already
  ended discards it (idempotent, the double-settle guard).

### D-H4 — Settle composition: guest-first, two trigger classes

| Trigger | Effect |
|---|---|
| **Local** — L1 focus loss / L2 pointer seam *inside* the guest; the guest's own Escape rung | settle the **guest only**, by its own kind rule (text commit-non-empty/discard-empty; picker clean-close; form close-without-effect) → deliver the exit to the host → host stays open |
| **Foreign** — R3 settle-then-run (nav, lifecycle, overlay-open), leaving effects, `Replace` | settle the guest by its kind rule, deliver the exit, **then** settle the host by its kind rule; outcomes concatenated guest-first; one pass |

- The Escape ladder generalizes: guest's own rungs → guest
  `Cancelled` (one rung, never also closes the host in the same
  stroke) → the host's ladder. N1/N3's hand-written rungs retire into
  this rule; their journey tests are the acceptance pins.
- `settle_flow` stays the one taxonomy: it gains a guest prelude, not
  a second function.

### D-H5 — The pair-table extension

- **Production**: `resolve` keeps its signature. While a guest is
  live, the adapter resolves against the **guest's** `FlowKind`
  (pickers → a new `FlowKind::Picker`, the form → `None` like its
  overlay cells today); a `SettleThenRun`/`CancelOnly`/`Replace`
  verdict triggers the **composite** settle of D-H4 (foreign class)
  or the guest-only settle (`CancelOnly` from a guest = a local
  cancel). The host's kept arms never see an effect a live guest
  resolved.
- **Tests**: the pair-table harness grows one dimension —
  `(host, Option<guest>, EffectClass)` triples, each pinned to the
  audit matrix — plus the **admission table** (`host × guest →
  admitted | GuestRefused`). Both are totality-checked over the
  enums (adding a `Guest` variant fails the build of the test until
  its row is written).

### D-H6 — Key scope: the guest rides above the host's mask

- `ScopeInputs` gains `guest: Option<&Guest>` (pulled from the flow —
  still no stored registration; D-KS1's self-healing rule holds).
- Each modal branch becomes `[guest ctx…, HostMask, ViInsert, Global]`;
  non-modal hosts push the guest ctx at the element-scope head. The
  guest contexts are the existing ones where they exist
  (`DatePicker`), new ones only for new guest kinds.
- The printable monopoly is untouched: a text field in a guest keeps
  its printables by the same `text-focus-count` predicate.

### D-H7 — Rendering & focus

- The guest renders inside a slot the host's `.slint` composition
  declares (`if root.guest-kind == GuestKind.date-picker : …`);
  flyout guests keep the window-anchored overlay pattern (round-5 D6)
  — the protocol is about state, not placement.
- Mount → the existing `FocusHandover` one-shot targets the guest;
  exit → focus returns to the host's mount-time anchor (stored in the
  host flow's data, not the adapter).
- The view-mount inventory test (round-5 D8) covers guest slots.

### D-H8 — Implementation shape (for the later dispatch plan)

1. Protocol core + N2 (entries-form date picker) as the first,
   smallest consumer — proves D-H2's single wipe site.
2. N3 (walk File-to + Pick-a-date) — retires `WalkStage`'s sub-state
   fields; `WalkStage` becomes pure stages.
3. N1 (entries list → form) — two flow variants collapse to one host
   + one guest.
4. Only if Q1 = (a): the planner surface extraction + embedded stage.

## 4. Non-goals / fences

- Depth > 1 (a guest hosting a guest) — see Q2.
- Concurrent guests (two live guests in one host).
- Changing any engine command, `AppState`, or the #622 planner's
  reachable-by-command contract.
- Re-opening the round-5 W5a scope stack beyond D-H6's input.

## 5. Questions for the user

- **Q1 — The consumer (the load-bearing one).** W5b was fenced with
  "the journal walk's stage-3 planner embedding" as its named
  consumer, but DR-7 D-J9 *ruled shape B* (handoff) and that shipped
  (`649f6b32`). Options:
  - **(a)** Re-open D-J9: embed the planner as a rendered walk stage.
    Requires extracting the planner's interaction state out of
    `ViewId::PlanNextDay` into a surface struct mountable both as a
    view and as a `Guest::PlannerSurface` — the largest chunk here.
  - **(b) (Recommended)** Keep shape B; W5b's consumers are the four
    existing hand-rolled nestings (N1–N3 ported, N4 left as the
    handoff). Pure consolidation, no user-visible change, and it
    makes (a) a small follow-up if you ever want it.
  - **(c)** Shelve W5b until a new feature actually needs nesting.
- **Q2 — Depth.** Depth-1 means the entries list→form→picker chain
  (N1+N2) is modelled as *host = form, guest = picker*, with the
  list→form step staying a flow transition (as today) rather than a
  guest. OK, or do you want depth 2 so N1 also ports? (Recommend
  depth 1; N1 then drops out of D-H8 step 3.)
- **Q3 — Foreign settle of a live text guest.** When a navigation
  effect settles host+guest, should a text guest follow its kind rule
  (commit non-empty — e.g. a half-typed entry gets saved) or always
  discard because the user left? Recommend **its kind rule** (the
  round-3 "no accidental clears, no accidental losses" behaviour
  users already see on the top-level flows).
- **Q4 — Exit on host-subject death.** If the host's subject dies
  mid-guest (`reconcile` drops the host), the guest gets no exit
  (silently dropped with its host) — recommended — or a `Cancelled`
  delivered first for symmetry?
- **Q5 — Next step.** After your rulings: an implementation dispatch
  plan per D-H8 (3 chunks under (b)), plus a tracking issue for the
  round. Fine?
