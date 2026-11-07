# [Plan] Chunk 1 — the surface chain with the view root

- **Date**: 2026-11-07
- **Kind**: stage-1 implementation plan for chunk 1 of the W5b dispatch
  (`.artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md`
  §2). Branch `refactor/ui-slint-surface-chain` off `main` @ `5274ecc3`,
  worktree `../taskboard-WT-refactor-ui-slint-surface-chain`.
- **Normative inputs**: the design round's §0 rulings Q1–Q7 and
  D-H1, D-H4, D-H5, D-H6, D-H7 (state side); §4 fences (no engine
  change, no `.slint` change, no embedded planner, no tree).
- **Acceptance (dispatch §2)**: every journey, `flow_table_tests`, the
  keyscope tests, `tests/seam_inventory.rs` and `tests/view_mount_inventory.rs`
  pass **without edits to their assertions** (moved call sites fine;
  changed expectations are a finding). New: the chain proptest with the
  six invariants; Settings/planner exact-restore unit tests;
  `settings_return`/`planner_return` gone. Gates green.

## 1. Verified anchors (`main` @ `5274ecc3`)

| Fact | Anchor |
|---|---|
| `ViewState` holds `view: ViewId`, `flow: Flow`, `settings_return`, `planner_return` as flat fields | `view_state.rs:369-401` |
| The four `set_view`-shaped writes are `enter` (cycle), `focus_board`, `focus_today`, `show_task`, plus `activate_focused`, `palette_jump`, `enter_kiosk_mode`, `reconcile`'s subject pops | `view_state.rs:679-762, 1128-1135, 2002, 1509-1527, 2405-2419` |
| `open_/close_settings` and `open_/close_plan_next_day` are snapshot-return pairs | `view_state.rs:627-654` |
| `settle_flow` is the one per-kind taxonomy shared by every trigger | `view_state.rs:2180-2236` |
| `reconcile` drops dead-subject flows and pops `StackDetail`/`TaskDetail` to Board | `view_state.rs:2390-2477` |
| The adapter's `(Flow, UiEffect)` table with the `(_, effect)` fallthrough into `resolve_conflict` | `adapter.rs:2700-2941` |
| `resolve_conflict`/`resolve` — the total `(FlowKind, EffectClass)` table, `Flow::None` early-return | `adapter.rs:3336-3430` |
| `flow_kind` maps the five modals + `None` to `FlowKind::None` | `adapter.rs:3295-3329` |
| The adapter's `settle_flow` wrapper; callers: leaving effects (`settle_flows_on_leaving` :2360, refresh :2489), the OpenMenu modal settle (:3289-3292), `CancelOnly` (:3355), `SettleThenRun` (:3379) | `adapter.rs:1964-1990` |
| The walk→planner handoff (`WalkResume` stored adapter-side; close-planner re-opens the walk at Prompts) | `adapter.rs:146-152, 2051, 2855-2875` |
| `scope_stack`'s five modal total-mask branches keyed on `KeymapShell` booleans | `keyscope.rs:88-146` |
| The shell booleans are mirrored from `view_state.flow()` per pass | `adapter.rs:1029-1050` |
| `ScopeInputs` is a plain struct literal in the keyscope tests (T-S1 checklist + ordering proptest) | `keyscope.rs:615-988` |
| The `FocusMode`/`Capture` close paths, the walk's `Handoff` step, the palette cursor-in-flow | `view_state.rs:1529-1745` |
| `docs/flow_model.org` + `docs/architecture.org:527` describe the view machine | docs touch target |

## 2. Decision record

### D1 — the `Surfaces` types (house A1; design D-H1 verbatim)

New file `crates/taskboard-ui-slint/src/surfaces.rs`
(`// SPDX-License-Identifier: AGPL-3.0-only`; the crate is UI — the
`#![forbid(unsafe_code)]` does not apply to `taskboard-ui-slint`):

```rust
pub enum Surface { View(ViewId), Flow(Flow) }

#[derive(Debug, Clone, PartialEq)]
pub struct Level { surface: Surface }   // private field; chunk 2 adds LevelState

pub struct Surfaces { chain: Vec<Level> } // chain[0] always Surface::View; innermost last

#[derive(Debug, thiserror::Error)]
#[error("surface kind already in chain")]
pub struct AlreadyInChain;               // asserted by matches!, never text
```

- `Level`'s field is private; the only mutators are `Surfaces`' methods.
- **Truncate is the sole removal API**: `truncate_to(depth)`; closing
  level *i* is `truncate_to(i)` — a child can never outlive its parent,
  by construction. No `pop`, no `retain`, no in-place surface swap above
  depth 0. The root swap (`replace_root(ViewId)`) is a *replacement*
  primitive on `chain[0]` (navigation), not a removal, and keeps depth.
- **The one fallible minting constructor**: `push(surface) -> Result<
  NonZeroUsize-ish depth, AlreadyInChain>` — actually
  `Result<usize, AlreadyInChain>` returning the new level's index; it is
  the only place a level enters the chain, and it checks the
  once-per-chain rule (D2) before appending.
- **The top-level XOR replace** is expressed at the call sites that open
  flows: `open_flow(f)` = `truncate_to(1)` then `push(Surface::Flow(f)).
  expect("flow kinds are unique after truncation to a view root")` —
  after truncation only the root view remains, so the push cannot
  collide with a flow; the `expect` documents the invariant (a view kind
  and a flow kind are distinct `SurfaceKind`s, so the root cannot
  collide either).
- `Flow` variants are **unchanged** (dispatch hard rule). `Surface`/`Level`/
  `Surfaces` live beside `Flow`; `ViewState` gains `surfaces: Surfaces` and
  loses `view`, `settings_return`, `planner_return` (D6).
- Chunk 1 keeps `Level` = a bare `Surface` wrapper (the dispatch's
  non-goal: no per-level buffers, `buffer` stays on `ViewState`). Chunk 2
  widens `Level` with `LevelState` — the field is private, so widening is
  a crate-local change.

### D2 — kind identity: a hand-written tag, discriminant not data (Q6)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowTag { Create, CreateStack, EditTitle, EditDate, EditDescription,
    Move, Palette, Capture, FocusMode, CreateBoard, RenameStack, RenameBoard,
    MoveStack, TrackEntries, TrackEntryForm, DragMove, PlanPlace, ReviewWalk }

impl Flow { pub fn tag(&self) -> FlowTag { /* one arm per variant */ } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind { View(ViewId), Flow(FlowTag) }
impl Surface { pub fn kind(&self) -> SurfaceKind { ... } }
```

- The tag is the **variant identity, not its data** (`TrackEntries` and
  `TrackEntryForm` are distinct tags — chunk 3's N1 needs them distinct;
  `FlowKind` in the adapter is deliberately *not* reused: it collapses
  the five modals to `FlowKind::None` for the resolver, which is the
  resolver's vocabulary, not identity).
- The once-per-chain rule: `push` refuses with `AlreadyInChain` when
  `chain.iter().any(|l| l.kind() == surface.kind())`. With `ViewId` as
  the view kind, a Settings-inside-Settings is refused too — the D7
  corner records the one behavioural consequence.

### D3 — view navigation: the full transition inventory and classification

Every site that writes a view today, classified. **Replace** =
`replace_root(v)` (depth-preserving swap of `chain[0]`); **PushOver** =
`push(Surface::View(v))` (may refuse); **CloseOver** = `truncate_to(1)`
(restores the root exactly); **InertGuard** = inert while an over-view
is pushed.

| Transition | Site(s) | Class | Notes |
|---|---|---|---|
| Cycle `nav.next/prev-view` | `next_view`/`prev_view`/`step_view`/`enter` | Replace; **InertGuard** | Guard reads "a pushed view level exists" instead of `matches!(self.view, Settings|PlanNextDay)` — byte-equal: the only pushed view levels are exactly those two |
| `open_settings` | :627 | PushOver | D7 corner: if already in chain → refuse → no-op (see D7) |
| `close_settings` | :635 | CloseOver | exact restore by truncation; the `.unwrap_or(ViewId::Board)` fallback retires (the root always exists) |
| `open_plan_next_day` / `close_plan_next_day` | :643-654 | PushOver / CloseOver | the `open_settings` twin |
| `focus_board`, `focus_today` | :733-746 | Replace | |
| `show_task` (`nav.show-task`, palette retarget) | :753-770 | Replace | sets `TaskDetail` or `Board`; subjects untouched |
| `activate_focused` (Enter → TaskDetail) | :1226-1238 | Replace | |
| `activate_today_row` | :1078 | Replace | lands on `TaskDetail` |
| `palette_jump` (per `SearchTarget`) | :2002 | Replace (every arm) | Task→`show_task`'s classification, Stack/Board arms set the view directly |
| `enter_kiosk_mode` | :1509-1527 | Replace | |
| `reconcile` subject pops (dead stack/task → Board) | :2405-2419 | Replace **of the top view level** | D8: the pop applies to the *top* view level — that is the level today's `self.view ==` check reads |
| `reconcile`'s stale-subject clear without a view pop | :2405-2419 | root in-place fact update | `subject_stack`/`subject_task` stay root-level `ViewState` fields in chunk 1 |

`next_view`/`prev_view`'s existing early return is re-expressed as
`if self.surfaces.depth() > 1 { return Vec::new() }` — today's guard is
exactly "the view is Settings or PlanNextDay", and those are the only
two view kinds that ever sit above the root. `enter`'s
`Settings | PlanNextDay => false` arm keeps its shape (it is the cycle's
refusal to *enter* the over-views, preserved as data-independent code).

### D4 — routing: innermost-first over the unchanged `resolve` (D-H5)

- The unchanged `resolve(FlowKind, EffectClass) -> Resolve` table and
  `flow_kind` stay byte-for-byte (`flow_table_tests.rs` pins the cells —
  untouched). `EffectClass` (`router.rs:241-283`) untouched.
- Shape: `Route { Handled(Vec<FlowOutcome>), Bubble }`. An effect is
  offered to the levels innermost-first; a level **handles** it when
  (a) it has a kept arm for the effect — today's `(Flow::Variant,
  effect)` table arms become that level's handler, keyed on the level's
  `Flow` (root level: the `(Flow::None, effect)` arms), or (b) it has no
  kept arm and the total `resolve` table answers for its kind:
  `Inert` → `Handled(vec![])` (consumed, no-op — *not* Bubble: the
  `(Capture, OpenPalette)` and `(Text, Cancel)` cells must stay silent,
  and bubbling them to the root's kept arms would open the palette /
  run a cancel — a behaviour change), `CancelOnly`/`SettleThenRun`/
  `Replace` → the existing `resolve_conflict` bodies, scoped to this
  level (settle *this* level by its kind rule, then re-run the effect
  once against the shortened chain for `SettleThenRun`; the `Replace`
  body's `begin_capture` keeps its truncating flow-open semantics).
- **Recorded decision — `Bubble` is declared but unreachable in chunk
  1**: `resolve` is total over `(FlowKind, EffectClass)` and the root's
  kept arms are total over effects (today's fallthrough's `Flow::None`
  early-return is unreachable *because* every class has a root arm), so
  the walk always stops at the first level it offers the effect to.
  With real chains at depth ≤ 2 this is byte-identical to the flat
  table: a flow level present → exactly today's flow arms/resolve; no
  flow level → exactly today's `(Flow::None, _)` arms. `Bubble` exists
  as the routing vocabulary D-H5 names and becomes producible in chunk
  3 when parents keep arm subsets (the exit protocol); introducing it
  now with no producing site would be dead code (the code-quality
  mandate), so **chunk 1 ships the walk without the `Bubble` variant**,
  with the offer-walk factored so chunk 3 adds the variant at one seam.
  This is the D-H5 shape minus the (currently) uninhabitable arm — the
  invariant "a `Handled` effect never reaches an outer level" holds
  trivially and is asserted by the proptest.
- `resolve_conflict`'s `Flow::None` early-return retires with the flat
  `flow` field (the root level's kept arms cover it); its comment's
  unreachability proof moves to the walk.
- The adapter's dispatch-table *arms* move minimally: each
  `(Flow::X, effect)` arm's body reads the level's `Flow` from the
  routing walk's matched level instead of `self.view_state.flow()`.
  Arms whose bodies call `view_state` mutators change only in that the
  mutator now names the chain method. `walk_resume` sites (:2051,
  :2855) keep their shapes (D9).

### D5 — the settle fold and the trigger classification (D-H4)

- `ViewState::settle_flow`'s **match body is reused per level, not
  duplicated**: it becomes `Surfaces::settle_level(level, state,
  buffers)`-shaped logic — the per-kind rule match moves to a function
  over a `Flow` + the settle helpers; the fold calls it per level
  innermost-first. (Chunk 1 keeps the rule in `view_state.rs` beside
  its helpers; the fold lives in `Surfaces`/`ViewState` — the point is
  one copy of the taxonomy, exercised per level.)
- **Foreign** = fold the chain innermost-first, each *flow* level by its
  kind rule; **local** = settle the top flow level only. View levels
  are never settled by the fold (today's settle never touches the
  view; Settings/planner close only via their close paths). Idempotent:
  an empty flow set folds to `vec![]`.
- Trigger inventory (every current `settle_flow` entry point):

| Trigger | Site | Class |
|---||---|
| L1 focus loss / L2 pointer seams (the field-blur and drag-seam callbacks) | the adapter's focus/blur and drag seams feeding `settle_flow` (:1973) | **local** (top flow level only) |
| Leaving effects (nav/refresh that strand a flow) | `settle_flows_on_leaving` :2360, the refresh pass :2489 | **foreign** (fold) |
| The OpenMenu modal settle (F9 hamburger face) | `settle_foreign_flow_for_modal` :3289 | **foreign** (fold) |
| R3 `SettleThenRun` (foreign effect over a held flow) | `resolve_conflict` :3379 | **foreign** (fold), then the bounded re-run |
| R3/K `CancelOnly` | `resolve_conflict` :3355 | **foreign** (fold) |
| The one `Replace` (capture fire-drill) | `resolve_conflict` :3357-3369 | **foreign** (fold) then the capture open |

  With chain depth ≤ 2 (root + ≤1 flow) the fold degenerates to
  today's single settle — byte-identical; the fold's generality is
  exercised by the proptest.
- The Q3 text rule rides for free: the fold reuses the existing text
  arm (commit non-empty trimmed / discard empty), which reads
  `ViewState.buffer` — unchanged in chunk 1 (D6).
- Q4 (subject death drops the subtree silently) is `reconcile`'s
  truncate: a dead-subject flow truncates to its index; no settle rule
  runs, no exit is delivered (chunk 3 wires exits; here the drop is
  silent exactly as today's `self.flow = Flow::None`).

### D6 — what stays on `ViewState`, what moves

- Moves into the chain: `view` (root + pushed view levels),
  `settings_return`, `planner_return` (**retired** — restore is
  truncation), `flow` (flow levels).
- Stays on `ViewState` in chunk 1 (chunk 2 moves the text-shaped ones):
  `board`, `subject_stack`, `subject_task`, `focused_task`,
  `selected_stack`, `buffer`, `stack_widths`.
- The accessor surface the adapter sees — kept name-compatible so the
  diff outside `view_state.rs` stays small:
  - `view()` → the **top view level's** `ViewId` (root when no
    over-view is pushed; `Settings` while Settings is open — identical
    to today's `self.view`). `view_identity()` reads the same top view;
    the dismiss-seam diff semantics are unchanged.
  - `flow()` → the top flow level's `Flow`, else `Flow::None`
    (`&Flow` — a `Flow::None` static for the empty case keeps the
    signature; or `Flow` is `Clone` and the getter returns a clone —
    the existing signature `&Flow` is kept via a lazily-shared `None`,
    implementation detail for stage 2).
  - `flow_mut()` → `Option<&mut Flow>` (the top flow level's data —
    the palette-cursor re-rank path); callers `None`-gate. The only
    external callers are the adapter's cursor re-rank and the walk's
    cursor clamp — mechanical moves.
  - Everything else (`board()`, `focused_task()`, `set_buffer()`, the
    domain walks) keeps its exact signature.
- `set_view` (pub, the seam the kiosk/install uses) becomes
  `replace_root` + the fact reset — call sites unchanged.

### D7 — the open-over corner: double-open and `AlreadyInChain` (Q6)

Today `open_settings` while Settings is open re-records
`settings_return = Some(Settings)` and closing lands on Settings (the
self-return). Under the chain, a second `push(View(Settings))` is the
`AlreadyInChain` refusal. **Decision**: `open_settings`/
`open_plan_next_day` map the refusal to a no-op returning
`vec![FlowOutcome::RefreshNow]` (the observable "view stays, one
refresh"). The only divergence is the double-open *close* destination:
today it lands on Settings (self), the chain lands on the root. That
path is a pathological synthetic sequence (the Settings surface binds
no `OpenSettings` stroke; the planner's kept arms route `OpenPlanNextDay`
to its toggle, whose already-open branch is the close — **checked:
the planner's `(Flow::None, OpenPlanNextDay)` toggle arm re-opens via
`view_state.open_plan_next_day()` only when the view is not
PlanNextDay** (`adapter.rs:2850-2860`), so the real UI cannot
double-open either surface). The plan pins the corner with a unit test
asserting the refusal (`matches!(.., Err(AlreadyInChain))`) and the
no-op mapping; if any journey or snapshot pins the self-return close,
stage 2 stops and reports (the dispatch's hard rule).

### D8 — `reconcile` over the chain

- The subject-pop predicates read the **top view level** (today's
  `self.view ==` reads exactly that) and swap *that* level's surface to
  `Board`. When no over-view is pushed this is the root — identical.
  When Settings is pushed over TaskDetail and the task dies: today the
  subject clears and `close_settings` restores TaskDetail (which the
  next reconcile then pops to Board); the chain version pops the top
  view level (TaskDetail → Board) *in place inside the stack*? — no:
  the top view level here **is** Settings, whose id doesn't match, so
  the predicate falls to... today's code pops `self.view` only when it
  is the dead subject's view; with Settings open, `self.view` is
  Settings, the pop does not fire, and the restore-then-pop happens on
  close. **Byte-identical chain encoding**: the pop fires on the level
  whose view matches — i.e. scan view levels topmost-first, pop the
  first whose `ViewId` is the dead subject's view (root TaskDetail
  under pushed Settings). One-refresh-earlier Board landing vs today is
  *not* observable (today's restore-then-pop sequence is not a mounted
  frame; both end at Board before the next push). Pinned by a unit
  test; journeys are the guard.
- Dead-subject flow arms (`TrackEntries`, `PlanPlace`, the editor
  family, drag…) become truncation to the flow level's index (Q4,
  silent). The TrackEntries view-strand rule (:2469-2476) reads the
  root view and truncates the overlay's level — same predicate, same
  outcome.

### D9 — `walk_resume` stays adapter-side (recorded decision)

The shape-B handoff is **not** chain-expressed in chunk 1. Expressing
"walk level stays below the planner level" would keep the walk's modal
key context live under the planner — observable in `scope_stack` and
the menu chain — which is a behaviour change and out of chunk 1's
byte-identical bar (design Q1: N4 stays the handoff; the dispatch
explicitly leaves this to stage 1 *to decide*). The `WalkResume` hint,
`hand_off_to_planner` (:2051), and the close-planner resume
(:2855-2875) keep their exact shapes; `close_review_walk` truncates the
walk's flow level like any flow close. Journeys J10–J13, J20 are the
guard.

### D10 — keyscope: the declarations exist, the branches keep their arming facts (D-H6, deviation recorded)

- **Per-kind declarations are introduced**: each modal `FlowTag`
  declares its total-mask context vector
  (`FlowTag::scope_head() -> &'static [ContextId]`-shaped:
  Palette→`[Palette, ViInsert, Global]`, FocusMode→`[FocusMode,
  ViInsert, Global]`, TrackEntries/TrackEntryForm→`[TrackEntries,
  ViInsert, Global]`, ReviewWalk→`[ReviewWalk, ViInsert, Global]`) —
  the five branches' *bodies* become this one declaration table.
- **The branches' arming facts stay the `KeymapShell` booleans** in
  chunk 1. `scope_stack`'s signature, `ScopeInputs`' fields, and the
  chain-walk shape are byte-unchanged, so the T-S1 checklist, the
  ordering proptest, and every construction site in the keyscope tests
  compile untouched (their struct literals pin the field set). The
  adapter keeps mirroring the booleans from the chain (the mirror site
  reads `view_state.flow()` — same value).
- The full concatenation ("each level's contexts innermost-first,
  stop at the first mask") needs real nested levels to be meaningful —
  it lands in chunk 3 with the consumer ports; doing it here would
  force either a `ScopeInputs` field change (breaking the pinned test
  literals — a changed-expectation finding by the dispatch's own bar)
  or a dual legacy/chain path (belt-and-braces dead code). Recorded as
  the D-H6 slice this chunk ships: *the declarations, fed by the same
  facts*.

### D11 — the proptest strategy and invariants (dispatch §2 Acceptance)

`proptest` over a command sequence strategy drawn from a small universe
(flow kinds of each `FlowTag` family with fixed ids; the two over-views;
close-top; local settle; foreign settle; a routing offer of a few
effect classes), length 0..40:

1. `chain[0]` is always `Surface::View` — after every command.
2. A child never outlives its parent: the shadow model mirrors the
   sequence; after every command the real chain is a prefix-preserving
   truncation of the model (any level the model dropped has no
   descendant in the real chain). Concretely: depth never increases
   past a live parent's index +1, and every close leaves the chain
   shorter from its index.
3. A foreign settle empties the flow levels innermost-first: the fold
   on a random chain yields the flow levels gone, view levels intact,
   and (shadow) the per-level outcome lists concatenate in
   innermost-first order.
4. A `Handled` effect never reaches an outer level: instrument the
   walk (test-only level-visit recorder) — an effect resolved at level
   *i* is offered to no level < i.
5. Settle is idempotent: a second foreign settle immediately after the
   first yields no outcomes and no state change.
6. A duplicate-kind push is `Err(AlreadyInChain)` — the strategy
   includes re-pushes of every live kind.

No upper-bound/ceiling assertions (AGENTS §5); the strategy generates
nested chains of unbounded depth within the length budget — the
generator draws flow kinds freely so depth-3+ chains occur.

### D12 — unit tests (TDD, first commits)

- Settings/planner exact restore: open from each cycle view, push,
  mutate nothing, close → the root view is the exact pre-open view
  (all five cycle views as parameters).
- Open-over→close-over interleavings: Settings over TaskDetail,
  planner over Settings (push over push), close closes only the top.
- The double-open refusal (D7) via `matches!`.
- `reconcile` pops: dead subject under a pushed over-view (D8), dead
  flow truncation (Q4 silent).
- The settle fold: a two-flow chain is impossible via real opens (XOR
  at depth 1) — the fold's multi-level case is the proptest's (the
  unit suite keeps to reachable states; the proptest exercises the
  general shape through `Surfaces`' public API).

## 3. Commit sequence (each standalone green)

1. `feat(ui-slint): the surface chain core` — `surfaces.rs`: `Surface`,
   `Level`, `Surfaces`, `FlowTag`, `SurfaceKind`, `AlreadyInChain`,
   push/truncate/replace-root/settle-fold primitives, `Flow::tag()` +
   the declarations of D10; the proptest (D11) and the chain unit
   tests. Pure addition — nothing wired, everything green.
2. `refactor(ui-slint): the view root rides the chain` — `ViewState`
   loses `view`/`settings_return`/`planner_return`, gains `surfaces`;
   the D3 transition table implemented (`replace_root`/push-over/
   close-over); the accessor surface (D6); D7/D8 unit tests.
   `view_state.rs`-internal + its tests; adapter still compiles via
   the kept accessors.
3. `refactor(ui-slint): settle folds the chain innermost-first` — the
   settle taxonomy reused per level; the trigger sites classified
   (D5) — local top-only, foreign fold; adapter settle call sites
   moved, not changed.
4. `refactor(ui-slint): effects route over the chain` — the routing
   walk (D4); the dispatch-table arms re-keyed to the walk's matched
   level; `resolve`/`flow_kind`/`EffectClass`/`flow_table_tests`
   untouched; `walk_resume` untouched.
5. `refactor(ui-slint): the scope stack reads the declarations` — the
   D10 declaration table replaces the five branch bodies; keyscope
   tests untouched.
6. `docs(ui-slint): the surface chain documents itself` —
   `docs/flow_model.org` and the `docs/architecture.org` UI section
   (:527 region) gain the chain-over-flat-XOR description; the module
   docs of `view_state.rs`/`surfaces.rs` settled.

Full gates via `scripts/run-quality-gates.sh` before handoff.

## 4. Risks

- **R1 — the D7 double-open corner**: the one place the chain's
  implicit restore differs from the recorded-return slots (close after
  double-open lands on the root, not the self). Mitigated: unit-pinned,
  real-UI-unreachable analysis in D7; any pinned journey stops the
  chunk (dispatch hard rule).
- **R2 — the `&Flow` accessor**: `flow()` returning `&Flow` for a
  possibly-empty chain needs a shared `Flow::None` static or a clone
  return; both are mechanical, but the signature choice touches every
  `flow()` caller (~dozens). Keep `&Flow` + a `const FLOW_NONE:
  Flow` if `Flow:`'s `None` variant allows it (it does — a unit
  variant); else fall back to a returned `&Flow` borrowed from a
  `Cow`-like mirror. Stage 2 picks the cheaper; either is
  behaviour-invisible.
- **R3 — hidden `settings_return` readers**: `switcher_selection`
  (:773-790) reads `settings_return.or(planner_return)` — re-expressed
  as "a pushed view level exists" (its id feeds the switcher's index).
  Byte-equal because those two are the only pushed view kinds; pinned
  by the keymap snapshots.
- **R4 — proptest runtime**: the chain strategy is small (few id
  sources, cheap invariants); no interaction with the wall clock; the
  deterministic-by-construction rules all hold (pure state machine).

## 5. Estimated diff

`view_state.rs` ~600 lines touched (transitions + settle + reconcile +
accessors), new `surfaces.rs` ~450 (types + primitives + tests),
`adapter.rs` ~250 (routing walk + re-keyed arms + settle call sites),
`keyscope.rs` ~60, docs ~40. Total ≈ **1.2–1.5k lines**, dominated by
mechanical re-keying; `resolve`/`EffectClass`/`flow_table_tests` and
every journey file: **zero assertion changes**.

## 6. Backlog issues

None opened at planning time: every out-of-scope observation (the D7
corner, the D-H6 chunk-3 remainder, the `Bubble`-variant deferral) is
in-scope for an already-dispatched chunk or recorded as a decision
above, not new work.
