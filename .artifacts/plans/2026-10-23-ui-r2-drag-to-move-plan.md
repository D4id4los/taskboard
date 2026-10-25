# [Plan] GUI Review Round 2 — Chunk 8: Pointer drag-to-move (D38, #226)

- **Date**: 2026-10-25
- **Kind**: stage-1 chunk plan (TDD-first; no implementation in this
  commit). Branch `feat/ui-r2-drag-to-move`, cut at `main` `b2656a9`
  (dispatch §1 precondition verified: chunk 7 `feat/ui-r2-visual-identity`
  is merged — PR #443, golden regen `30381b0` — and chunks 1–6 landed
  before it per the serial default). Worktree created with
  `git worktree add` after a green `scripts/worktree-doctor.sh` (100G
  free / 25G floor). *Incident note:* the first worktree instance was
  removed mid-session by the worktree-doctor sweep
  (`8176e94`/`68f110c` landed on `main` while this session read seams)
  — recreated per the AGENTS §8 resume rule on the same branch,
  fast-forwarded to the new `main`; no work was lost (the plan was not
  yet committed) and no `crates/` file changed between the two base
  points (`git diff 30381b0..origin/main -- crates/` is empty), so all
  seam line numbers below are valid.
- **Origin** (normative): design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §9 (D38),
  §10 (testing strategy), §14-R1/R2 (stable model identity; Slint
  fights back); issue #226 (its *Proposed Approach* is normative
  context: DIY drag on press/move with a ghost and column hit-testing,
  drop dispatches the same `MoveTask` the move flow confirms — **and**
  "Re-check Slint's DnD surface at each version bump before
  hand-rolling"); dispatch plan
  `.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`
  §1 (shared execution model + code-quality mandate) and §9 (this
  chunk's acceptance + prompts 8.1/8.2/8.3).
- **Non-goals** (dispatch §9): no touch/kiosk-specific gestures (touch
  parity rides the synthesized-mouse path, §3.7); no multi-select drag;
  no position-within-stack (drop = the keyboard flow's end-of-stack
  append, §3.5); no compact-mode drag (§3.4 — the compact leg mounts
  one column; the keyboard move flow stays its pointer-equivalent);
  no cross-board drops (the move *ring* is cross-board, but only the
  live board's stacks are mounted — a drop can only address a visible
  column); no StateCommands beyond what `confirm_move` already builds;
  `taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`: empty diff, asserted by reviewers.

## 0. Decisions index (the dispatch prompt's settlements)

| # | Decision | One-line form |
|---|---|---|
| D-A | DnD re-check (probe P-1) | **Slint 1.18.1 ships a native DnD surface** — `DragArea`/`DropArea` builtin elements with in-window fallback, 8-px slop, click preservation, and runtime-owned cancel semantics. **Native wins**; DIY is overruled by D38's own "hand-roll only if absent" and #226's re-check rule (§2). |
| D-B | State machine | Pointer mechanics = Slint/core natives; flow semantics = **`Flow::DragMove { task, target }` in `view_state`** (a distinct variant, additive to the keyboard `Flow::Move`); the adapter holds only flow-guarded seam handlers + a drag-teardown helper. Refresh-survival is model-push by construction (§3.1). |
| D-C | Slop | **Inherited 8 logical px** — Slint's `DISTANCE_THRESHOLD`, the same constant Flickable pans on; not configurable per-DragArea, so a bespoke threshold would mean DIY for no behavioural gain. Tier-4 judges the feel (§2.3). |
| D-D | Ghost | A real `TaskCard` with the dragged card's snapshot model at opacity 0.5, mounted in app-window's overlay stack; position fed by `can-drop` callbacks through a `DragState` global (with a board-wide rejecting catch-all `DropArea` for dead-zone coverage); model + open-flag ride the refresh push (§3.3). |
| D-E | Hit-testing | **Native `DropArea` per column — no geometry crosses to Rust at all.** The design's two candidate mechanisms (per-column out-properties vs Slint-side designation) are both overruled: compiled Slint cannot read per-instance geometry of `for` repetitions from Rust, and the runtime hit-tests DragMove internally. Highlight = the **existing `move-target` push** extended to `Flow::DragMove` (§3.4). |
| D-F | Drop semantics | End-of-stack append via the **same `MoveTask` command** `confirm_move` builds — one shared construction site in `view_state` makes it equivalence-by-construction, plus a pin test comparing the produced `StateCommand`s (§3.5). |
| D-G | Cancel matrix | Release-outside → runtime rewrite → `drag-finished(none)`; Escape → the existing key path (`CancelMove`) + a synthetic `PointerExited` teardown; leaving effect → `settle_flows_on_leaving`; vanish → the view-state reconcile rule. Every seam handler is flow-guarded (absorbs when no `Flow::DragMove`) (§3.6). |
| D-H | Click/double-click interplay | Untouched below the slop (native guarantee: children stay interactive; `clicked` needs an in-geometry release with `pressed()` held; DragMove explicitly skips hover updates). Proven by the unchanged existing suite + one sub-slop boundary journey (§3.7). |
| §5 | Tests | view-state unit suite (pure) + seam journeys on the headless driver (drop, interleaved refresh, three cancels, slop boundary, flow-busy refusal) + the unchanged round-1/2 suites as the regression gate (§5). |
| §6 | Tier-4 | Slop feel, ghost look/tracking, target-highlight contrast, Escape-cursor sanity, compact non-drag (§6). |

## 1. Verified facts (anchored; repo lines are `main` `b2656a9`, slint lines are the pinned crates)

### 1.1 The workspace side

- Pins: `slint`/`slint-build` `1.18.1`, `i-slint-backend-testing`
  `=1.18.1`, `i-slint-core` `=1.18.1` (root `Cargo.toml` `[workspace.dependencies]`).
- `ui/widgets/task-card.slint`: the card's `TouchArea` (71–95) owns
  hover/tooltip (74–82), `double-clicked` → `activated` (83),
  `clicked` → `clicked` (84), right-press → `context-requested`
  (88–94). `TaskCardModel` (29–40) is the stable display model; the
  card paints hover/done backgrounds (59–61) and the emphasis ring
  (63–64).
- `ui/compositions/board-layout.slint` (post-chunk-7): the regular leg
  renders each stack as a **card** — `card := Rectangle` (158) inside
  the columns row (153), containing `column-body` with the header
  (204) and the task list (335–341). The move-target highlight is the
  **header background arm** (281–287: `surface-raised` when
  `column-index == root.move-target`), the **badge tone flip** (323),
  and the **compact switcher arm** (456–461). `move-target` is an
  `in property <int>` (45, −1 = no flow) fed by `BoardModel`
  (`ui/views/board-view.slint`:20,85). The per-column resize handle
  (254–274) is the repo's existing drag precedent: press captures,
  `moved => if (self.pressed)` reports deltas, release persists.
- `ui/app-window.slint`: the overlay stack is ordered later-sibling =
  hit-tested first (the D2 probe P1 note, 304–309): menu dismissal
  layer (310), root `ActionMenu` (320), fly-out (370), tooltip panel
  (393), palette/capture dismissal layers + `CaptureOverlay` +
  `SearchPalette` (404–437). Geometry out-props: `window-w`/`window-h`
  (159–160) — **board-layout exposes no per-column geometry to Rust**
  (the D38 premise "geometry the board layout already exposes" was
  stale pre-chunk-7; post-chunk-7 it is definitively absent). The
  board callbacks route through `BoardView` (198–224) as index-only
  seams.
- `src/view_state.rs`: `Flow` (58–93) — at most one flow (D9 XOR);
  move flow: `start_move` (1201), the cross-board flat ring
  (`move_ring` 1221), `cycle_move_target` (1231), **`confirm_move`
  (1251–1265: `StateCommand::MoveTask { id, stack: target, order:
  append_order(state, target) }` + `RefreshNow`)**, `cancel_move`
  (1268–1274: flow-agnostic reset + `RefreshNow`).
- `src/adapter.rs`: the interpret table (564) routes
  `(Flow::Move|Flow::MoveStack, FocusNext|FocusPrev|Activate|
  RequestMove|CancelMove|SetMode(Normal))` → `interpret_move`
  (770–796: cycle / confirm / **every cancel-shaped effect ends
  without dispatching**); `settle_flows_on_leaving` (516–559) cancels
  the move flows on a leaving effect (533–536); the width-drag seams
  (2112–2193) are the adapter-local scratch precedent; the board card
  seams resolve `(column,row)` through the id tables
  (`focus_board_card` 2634, stale-index-absorbing); tooltip dismissal
  precedent `Self::dismiss_tooltip` (832).
- `src/models.rs`: the columns push diffs persistent per-column
  handles (612–629, chunk-2 D9 stable identity); `BoardModel` push
  computes `move_target` from the flow (658–674 — `Flow::Move` and
  `Flow::MoveStack` today); window-level flow flags push as root
  properties (`set_board_create_open` 631).
- Test harness: `tests/support/driver.rs` dispatches raw
  `WindowEvent`s (`click_at` 206–218, `hover_at` 220–226) through
  `Window::dispatch_event`; the round-1 D17 drag journey
  (`tests/journeys.rs`:1325, T-11) is the seam-level drag precedent;
  `src/test_platform.rs` is the headless `NullPlatform` (no backend
  DnD hooks — relevant to §2, all good, see below).

### 1.2 The Slint 1.18.1 side (probe P-1 evidence; registry paths elided to crate@version:line)

- **The elements exist**: `DragArea` and `DropArea` are builtin
  language elements (`i-slint-compiler@1.18.1/builtin_elements.rs:1779,
  1832`), both `@expands_to_parent_geometry`.
  - `DragArea`: `enabled`, `data` (`data-transfer`),
    `drag-image`+offsets, `allow-copy/move/link`, **`dragging`
    (out bool)**, **`drag-finished(action: DragAction)`**. "A click
    doesn't start a drag, so child elements like TouchArea stay
    interactive."
  - `DropArea`: `enabled`, **`can-drop(event: DropEvent) ->
    DragAction`**, **`dropped(event) -> DragAction`**, **`has-drag`
    (out bool)**, `current-action`.
- **Slop**: `exceeds_drag_threshold` compares against
  `DISTANCE_THRESHOLD` = **8 logical px**
  (`i-slint-core@1.18.1/items/drag_n_drop.rs:95-103`,
  `items/flickable.rs:382`) — the exact constant Flickable pans on;
  press-then-move only, **no hold timer**.
- **The filter preserves children**: `press_drag_filter`
  (`drag_n_drop.rs:51-91`) forwards everything to children until the
  slop is crossed; a left press only.
- **In-window fallback is automatic**: `offer_native_drag`
  (`input.rs:1490-1523`) asks the backend via `InternalToken::
  start_drag` and otherwise calls `arm_in_window_drag` on the shared
  `MouseInputState`. **The winit backend 1.18.1 has no `start_drag`
  implementation** (crate-wide grep: zero matches) — at runtime on
  every target platform the drag is the in-process state machine; no
  OS-DnD variability. Our headless `NullWindowAdapter` has no
  `InternalToken` either — the same fallback serves the tests.
- **Release semantics are runtime-owned** (`window.rs:840-940`):
  during a drag, `Released` rewrites to `Drop` (if a DropArea accepted
  the last DragMove — the `drop_target` bookkeeping) or `Exit`
  (tear-down + `drag-finished(none)`); the source's own comment:
  "so a non-accepting DropArea under the cursor doesn't fire
  `dropped`, and so the underlying Release doesn't reach hit-tested
  items as a spurious click" (`window.rs:896-898`). Ordering:
  the target's `dropped` fires **before** the source's
  `drag-finished` (`window.rs:862-864`, deferred on purpose).
- **Hover is suppressed mid-drag**: TouchArea's filter skips
  DragMove/Drop ("Someone else has the grab, don't handle hover",
  `items/input_items.rs:95-98`) and TouchArea's `input_event` returns
  `EventIgnored` for both (`input_items.rs:214-215`) — no hover
  backgrounds or tooltips fire on cards the pointer passes over, and
  **a top-mounted ghost element cannot shadow the column DropAreas**.
- **Teardown seam**: `WindowEvent::PointerExited` →
  `MouseEvent::Exit` (`api.rs:709-714`) → in-flight drag torn down +
  `drag-finished(none)` (`window.rs:927-936`); TouchArea's Exit arm
  clears `pressed` and fires `pointer-event(Cancel)`
  (`input_items.rs:139-152`) — so the *real* release after a teardown
  cannot degrade into `clicked` (`clicked` requires in-geometry
  release **and** `pressed()`, `input_items.rs:154-163`).
- **Payload & types**: `DataTransfer` is public Rust API
  (`slint::DataTransfer`, `From<SharedString>`, `set_plain_text`);
  DragArea refuses to start when `data` is empty
  (`drag_n_drop.rs:147`). `DropEvent { data, position
  (DropArea-local), proposed-action }`
  (`i-slint-common@1.18.1/builtin_structs.rs:116-131`).
  `DragAction { None, Copy, Move, Link }` (`enums.rs:652-666`).
- **Canonical test pattern**: slint's own
  `tests/native_drag.rs:178-207` drives an in-window drag with plain
  `PointerPressed` → `PointerMoved` (>8px) → `PointerReleased`
  dispatches and asserts `has-drag` / `dropped` / `drag-finished` —
  exactly our harness's vocabulary.

## 2. D-A — The DnD re-check (probe P-1) and the native decision

**Result: Slint 1.18.1 has a native, sufficient DnD surface (§1.2);
this chunk uses it.** The recorded approach of D38 and #226 is
"hand-roll only if absent" — the premise ("Slint 1.18 has no
widget-level DnD primitive", written against an earlier 1.18
assessment) is stale, and the re-check both normative texts demand
therefore overturns the DIY fallback, not the native preference.
Stage 3 restates this in the #226 ✅ DONE comment (the issue closes on
the behaviour, not on the implementation technique).

Why the native surface is *sufficient* for D38's contract, mechanism
by mechanism:

| D38 requirement | Native mechanism |
|---|---|
| press-and-hold-then-move lifts a ghost | left press + 8-px move arms the drag (`dragging` flips true); no hold timer — movement is the intent signal, matching the Flickable precedent. *Tier-4 owns the feel question.* |
| ghost follows the pointer | our overlay element fed by `can-drop` position reports (D-D, §3.3) — the only piece the native surface does not render for us (its own `drag-image` needs a raster `Image`, and there is no public render-element-to-image API; rejected) |
| column hit-testing | runtime DragMove hit-testing over per-column `DropArea`s (D-E, §3.4) |
| pointer release outside any column cancels | runtime `Released`→`Exit` rewrite + `drag-finished(none)`; no spurious click |
| drop dispatches the same `MoveTask` | `dropped` seam → the shared view-state construction (D-F, §3.5) |
| Escape (D16's cancel binding) cancels | existing key path; synthetic `PointerExited` teardown (D-G, §3.6) |
| click/double-click semantics unchanged | the filter's child preservation + `clicked`'s in-geometry rule (D-H, §3.7) |

**Not taken — DIY on the card TouchArea**: it would re-implement slop,
grab bookkeeping, out-of-area move tracking, click suppression after a
dragged release, and cancel semantics the runtime now owns, and its
Rust-side hit-testing would need geometry plumbing (per-column
out-props are unreadable from Rust for `for` repetitions in compiled
Slint — the adapter would need a global written per column
instance) — strictly more code over a surface the framework ships.
**Not taken — native with `drag-image`**: no public way to rasterize a
live `TaskCard`; a synthetic bitmap would diverge from the card visual.
**Not taken — reuse `Flow::Move` for drags**: see D-B.

### 2.3 The slop (D-C)

Inherited 8 logical px, one fact recorded: it scales with
`base-scale` only in the logical-px sense the runtime already applies
(every other repo gesture — Flickable pan, the native drag — shares
it). No theme token (nothing to style); `token_tests.rs` untouched.
Tier-4 item 1 judges eagerness; if the user wants a different slop the
fallback is the DIY filter (a recorded re-opening condition, not this
round).

## 3. The settled design

### 3.1 D-B — Where the state machine lives

Three layers, each owning exactly one concern:

1. **Pointer mechanics → Slint/core natives.** Slop, grab, click
   preservation, DragMove hit-testing, release rewriting. We own none
   of it (that is the point of D-A).
2. **Flow semantics → `view_state`**: new
   **`Flow::DragMove { task: TaskId, target: StackId }`**. A distinct
   variant, *not* a `Flow::Move` reuse, because (a) regression
   isolation — the keyboard move flow's arms (toggle-re-request
   cancels, ring cycling, Activate confirms) keep byte-identical
   semantics; (b) the drag flow has pointer-shaped intents with no
   `UiEffect` precedent (set-target-by-column) that would overload the
   table's Move arms; (c) `models.rs` pushes `move_target` and the
   ghost/dim flags from the flow — a distinct variant keeps the
   keyboard flow's push semantics untouched.
   Why `view_state` at all and not adapter-local scratch (the
   `width_drag` precedent): everything the drag renders — source dim,
   ghost, drop-target highlight — must **survive interleaved
   refreshes** (dispatch acceptance (d); the chunk-2 invariant), and
   the chunk-2 stable-models rule makes that true by construction only
   for state the *model push* derives. Adapter-local scratch dies on
   the next `push_models` unless re-derived; `width_drag` survives
   only because its effect is persisted per-event into
   `view_state.stack_widths`. Escape/leaving cancellation also
   composes through the existing single-key-path → interpret table →
   cancel arms with zero new key plumbing.
3. **Seam bookkeeping → `adapter.rs`**: flow-guarded handlers for the
   three new callbacks (arm, dropped, finished), a tooltip dismissal
   at arm, and a `teardown_runtime_drag` helper (§3.6). No new
   `StateCommands` — the drop path builds exactly `confirm_move`'s
   command (D-F).

The slint-side state machine is DragArea's internal
idle→pressed→dragging; our visible states are just `Flow::None` and
`Flow::DragMove` — "armed but pre-slop" never reaches Rust. New
`view_state` methods (all pure, `#[must_use]` outcomes):

- `begin_drag_move(&mut self, state, task: TaskId) -> Vec<FlowOutcome>`
  — sets `Flow::DragMove { task, target: task.stack }` (target seeded
  to the source stack), `RefreshNow`. Refuses (`Vec::new()`) when any
  other flow is active (the D9 XOR rule — the slint side additionally
  disables the DragAreas while a flow is open, belt and suspenders)
  or the task is unknown (the reconcile path re-checks).
- `drag_target(&mut self, state, stack: StackId) -> Vec<FlowOutcome>`
  — during `DragMove`, sets `target` when it changed, `RefreshNow`;
  inert otherwise. `Flow::None` → drop-target −1 on the next push.
- `confirm_drag_move(&mut self, state) -> Vec<FlowOutcome>` — builds
  the command through the shared construction site below, ends the
  flow, `RefreshNow`.
- `confirm_move` and `confirm_drag_move` both funnel into one private
  `append_move_command(state, task, target) -> FlowOutcome` — **one
  construction site** for the `MoveTask` command (equivalence by
  construction, D-F).
- `cancel_move` already resets any flow; `Flow::DragMove` needs no new
  cancel method — the table just gains DragMove arms routing to it.
- Reconcile: the refresh-time flow-repair pass (which today re-seats
  `Flow::Move` subjects against live state) extends to `DragMove` — a
  vanished task or vanished target stack cancels the flow (§3.6).

Interpret-table arms (additive; `interpret_move` extended):

- `(Flow::DragMove{..}, CancelMove | RequestMove | SetMode(Normal))`
  → `cancel_move` (Escape / C-g / the toggle, D16's bindings unchanged).
- `(Flow::DragMove{..}, Activate)` → `confirm_drag_move` + runtime
  teardown (Enter mid-drag confirms the hovered target — the same
  "Activate confirms a move flow" semantic; the adapter tears the
  native drag down alongside, §3.6).
- `(Flow::DragMove{..}, FocusNext | FocusPrev)` → absorbed
  (`Vec::new()`): the pointer owns target cycling during a drag;
  keyboard ring-cycling would fight it.
- `settle_flows_on_leaving` (`adapter.rs:533`): `Flow::DragMove`
  joins the `Flow::Move | Flow::MoveStack` arm (cancel, no dispatch)
  — plus runtime teardown.

### 3.2 The slint-side wiring (what wraps what)

In the **regular leg only** (`board-layout.slint:153-380`), inside the
per-column `for`:

```slint
DropArea {                       // per column (D-E)
    can-drop(event) => {
        // ghost position feed (D-D): window coords from local
        DragState.report(root.column-absolute-x(column-index) + event.position.x, ...);
        root.board-drag-hovered(column-index);   // → drag_target
        DragAction.move
    }
    dropped(event) => {
        root.board-card-dropped(column-index);
        return event.proposed-action;
    }
    card := Rectangle {          // the existing r2 card, unchanged
        // … header, task list …
        for task[task-index] in column.tasks: DragArea {   // per card
            allow-move: !root.flow-open;             // XOR belt
            data: DragState.payload;                 // non-empty, set once from Rust
            changed dragging => { if (self.dragging) {
                root.board-card-drag-armed(column-index, task-index); } }
            drag-finished(action) => {
                root.board-card-drag-finished(column-index, task-index,
                    action != DragAction.none);
            }
            TaskCard { … }      // the existing card, unchanged
        }
    }
}
```

- `DropArea` wraps the whole column card: drop anywhere on the column
  (header, body, gaps) targets it — generous targets, matching the
  keyboard flow's column-granular ring.
- `DragArea` wraps **only the TaskCard** — the header (activate/
  context/tooltip), rename field, create row, add-task ghost and the
  resize handle keep their exact seams untouched.
- `DragAction.move` member spelling in `.slint` is stage-2-confirmed
  at first compile; the fallback that avoids the question entirely is
  `action != DragAction.none` — semantically exact here because the
  source allows `move` only, so any non-`none` negotiated action *is*
  move. The slint closures return `event.proposed-action` (the
  spelling the shipped slint test uses).
- Modifier keys: `allow-copy`/`allow-link` stay `false`; the runtime
  clamps any Ctrl/Shift request to the first allowed action = move
  (`compute_proposed_action` fallback) — modifier chords cannot turn
  a drop into a copy. Recorded, not tested (runtime-owned).
- The **catch-all rejecting `DropArea`** (D-D's dead-zone coverage)
  wraps the columns row as an earlier sibling (lower z): it rejects
  (`can-drop => DragAction.none`) but its `can-drop` still reports
  position, so the ghost follows over gaps; release over it is the
  native no-accepted-target path = clean cancel. Stage-2 probe P-2
  (§7) verifies DragMove actually reaches it through non-DropArea
  subtrees (TouchArea ignores → `EventIgnored` continues the walk;
  the fallback if not: ghost updates only on column hover — freeze
  between columns is recorded feel debt, Tier-4 item 2 judges).
- **Compact leg untouched** (no `DragArea`/`DropArea`): one column
  mounted means no cross-column drop target exists; the keyboard move
  flow (with the switcher highlight) remains compact's move affordance.
  The compact switcher's `move-target` arm (456–461) therefore never
  paints from a drag — no change needed there.

### 3.3 D-D — The ghost

- **What renders**: a real `TaskCard` with the dragged card's snapshot
  `TaskCardModel` (title, due phrase, decorations — the exact pixels
  the source had at drag start), at `opacity: 0.5`, inside a
  borderless wrapper in app-window's overlay stack, mounted as the
  last sibling (after `SearchPalette`, `app-window.slint:404-437`).
  During a drag no other overlay can be open (menu: its dismissal
  layer is hit-tested before the board, so the press never reaches a
  card's DragArea; palette/capture: a flow is active, DragAreas are
  disabled) — the position is cosmetic z-order.
- **Following the pointer**: `can-drop` callbacks (per column + the
  catch-all) translate `event.position` (DropArea-local) by the
  DropArea's `absolute-position` into window coords and write a
  `DragState` global (`in-out property <length> pointer-x/y;`
  `in-out property <TaskCardModel> ghost-model;` — globals are the
  established Rust↔slint shared-state pattern, `TooltipState`
  precedent). The ghost's `x`/`y` bind to the pointer with the card
  **centred** under the cursor (no grab-offset seam; Tier-4 item 2).
- **Refresh-survival**: `board-drag-open` (a root flag, the
  `set_board_create_open` pattern) and `ghost-model` are pushed by
  `models.rs` on every refresh while `Flow::DragMove` holds — the
  push *rebuilds* them, which is the chunk-2 invariant made
  constructive. `DragState.pointer-x/y` are runtime-owned and never
  written by the push. The ghost's TouchArea is inert mid-drag by the
  runtime's hover-skip + DragMove-ignore (§1.2) — no interactive
  ghost, no shadowed drop targets.
- **Source dim**: `TaskCardModel` gains `dragging-source: bool`
  (default `false`, models.rs sets it from the flow); the card paints
  `opacity: 0.35` when set. Model-driven ⇒ survives refreshes. (A
  token is not introduced for either opacity literal — Tier-4 tunes
  the literals; no ladder change, `token_tests.rs` untouched.)
- **Tooltip**: the arm seam dismisses the source card's (possibly
  held-open) tooltip via `Self::dismiss_tooltip` (the open-menu
  precedent, `adapter.rs:832`); no new tooltips can fire mid-drag
  (hover skip, §1.2).

### 3.4 D-E — Column hit-testing (the mechanism, recorded)

**Native `DropArea` per column. No geometry crosses to Rust — the
design's two candidate mechanisms are both overruled by the probe:**
(a) per-column out-properties are unreadable from Rust: compiled Slint
exposes no per-instance accessors for `for` repetitions (only
globals and root properties are reachable), so "the `window-w`
out-property pattern extended per column" cannot be implemented as
sketched — it would need a slint-written geometry global with
per-instance writes on mount/geometry-change (races, change-callback
ordering) to feed an adapter-side rect walk; (b) a Slint-side
designation needs exactly the DragMove plumbing the runtime already
performs internally. The `DropArea` *is* the designation: `can-drop`
fires with the column identity captured in the slint closure.
The **highlight** reuses the existing `move-target` push
(`models.rs:658` extended to `Flow::DragMove`) — the regular header
arm (281–287) and badge tone flip (323) paint with the keyboard flow's
exact visuals, and the flow-driven push survives refreshes.
`DropArea.has-drag` is deliberately **not** bound to any paint (one
mechanism per behaviour; `has-drag` would flicker at DragMove
granularity and bypass the flow).

### 3.5 D-F — Drop semantics (command equivalence)

The drop dispatches the **same `MoveTask` the keyboard flow
confirms**: `StateCommand::MoveTask { id, stack: target, order:
append_order(state, target) }` — end-of-stack append, the keyboard
flow's exact semantics. Mechanism: `confirm_move` and
`confirm_drag_move` both call one private `append_move_command`
construction site (§3.1). Drop on the **source** column is valid and
reorders the task to its own stack's end — the keyboard ring allows
the same move (equivalence, not a bug). **The pin** (stage-3 trap (a)):
a test that drives both paths against one `AppState` and asserts the
produced `StateCommand::MoveTask`s compare `==` — commands, not
outcomes (§5 T-3).

### 3.6 D-G — The cancel matrix

| Arm | Trigger | Mechanism | Dispatch? |
|---|---|---|---|
| **Release outside any column** | `Released` with no accepting DropArea (catch-all/gutter/toolbar) | runtime rewrites → `Exit` → `drag-finished(none)` → the finished seam cancels the flow | no |
| **Escape** | `intercept-key` during DragMove | resolver → `CancelMove` (D16's binding, unchanged) → `interpret_move` → `cancel_move`; the adapter then calls `teardown_runtime_drag`: a synthetic `window.dispatch_event(WindowEvent::PointerExited)` — the runtime tears the drag down, fires `drag-finished(none)` (absorbed — flow already gone), and TouchArea's Exit arm clears `pressed`/hover so the real release cannot `clicked` | no |
| **Leaving effect** | NextView/PrevView/FocusBoard/BoardNext/BoardPrevious/BoardSelect | `settle_flows_on_leaving` (`adapter.rs:533`) gains DragMove → `cancel_move` + teardown | no |
| **Task-vanish reconcile** | a refresh drops the dragged task (or its target stack) | the view-state reconcile pass cancels `Flow::DragMove` (the subject-repair rule); ghost/dim/highlight unpush | no |
| **Race absorption** | `dropped`/`drag-finished` arriving with no active flow (post-cancel tear-down) | every drag seam handler is flow-guarded: `let Some(Flow::DragMove{..}) = flow else { return }` | no |

`teardown_runtime_drag` notes: `PointerExited` is a public
`WindowEvent` (`api.rs:709-714`); after it, the first real mouse move
re-establishes hover (cosmetic only — Tier-4 item 4); dispatching it
is our one synthetic event, justified because the runtime exposes no
other public in-flight-drag cancel. The same helper serves the
confirm-during-drag path (§3.1).

### 3.7 D-H — Touch & click/double-click interplay (regression risks)

- **Below the slop** the DragArea filter forwards everything to
  children: `clicked`, `double-clicked`, `context-requested`,
  hover/tooltip — byte-identical behaviour. The gate is the
  **unchanged existing suites** (round-1/2 card journeys run as-is,
  no edits) plus one new boundary journey (§5 T-6).
- **A dragged release** never fires `clicked`: either the runtime
  rewrites it (native path) or it lands out of the card's geometry
  (`clicked` requires containment, §1.2). The drop journey pins the
  no-click side-effect (the fake engine records no `task.edit`).
- **Touch** (`touch_finger_id` ≠ 0) synthesizes the same mouse path
  with `PointerEventButton::Left` — single-finger drag works like the
  mouse with zero extra code. No kiosk-specific gesture (non-goal).
  No Flickable conflict: the regular board does not pan today
  (#373 deferred), so drag-vs-pan arbitration never engages.
- **Hover**: other cards' `has-hover` cannot change mid-drag
  (§1.2) — no hover flashes, no tooltips. The source card's held
  tooltip is dismissed at arm (§3.3).
- **pointer-event(Cancel)** (post-teardown) passes through the
  card's handler unhandled (it matches only `down`+right, 88–94).

## 4. Acceptance mapping (dispatch §9 (a)–(f))

| Dispatch acceptance | Where satisfied |
|---|---|
| (a) re-check recorded; native absent/insufficient → DIY | §2 probe P-1: present **and** sufficient → native; DIY overruled by D38's own protocol |
| (b) press-hold beyond slop starts (ghost visible, source dimmed); move highlights; release dispatches the SAME `MoveTask` (drag seam ≡ `confirm_move`) | §3.1/3.3/3.4/3.5; journeys T-1/T-3/T-4 |
| (c) release outside, Escape, leaving effect cancel — no dispatch | §3.6; journeys T-5a/T-5b/T-5c |
| (d) ghost + highlight survive refreshes | §3.3 model-push construction; journey T-2 |
| (e) single/double-click semantics unchanged | §3.7; unchanged suites + journey T-6 |
| (f) Tier-4 feel/threshold/ghost-look | §6 |

## 5. TDD-first discipline & test catalogue

Order of work (every commit standalone-green):

1. **`feat(ui-slint): the Flow::DragMove view-state`** — the pure
   suite first (written against the planned API, red), then the
   variant + methods (green). No slint diff yet.
2. **`feat(ui-slint): the board's native drag wiring + ghost`** —
   slint wrappers, `DragState` global, adapter seams, model pushes;
   then the seam journeys (red where they need the wiring, green
   after; the commit lands them together, journeys authored first in
   the file).
3. **`docs(changelog): the pointer drag-to-move round (D38)`** — the
   Unreleased entry. (Stage 3 owns the #226 ✅ DONE comment; the
   backlog is otherwise untouched.)

The catalogue (T-numbers for the PR body):

**Unit — `view_state.rs` (pure, no slint):**
- T-1 `begin_drag_move` seeds `target` = the task's stack; refuses
  when another flow holds (XOR); refuses an unknown task.
- T-2 `drag_target` emits `RefreshNow` only on an actual change;
  inert without the flow.
- **T-3 the command-equivalence pin**: keyboard (`start_move` +
  `cycle_move_target` + `confirm_move`) vs drag (`begin_drag_move` +
  `drag_target` + `confirm_drag_move`) produce **equal
  `StateCommand::MoveTask`s** for the same (task → stack) transition —
  comparing the commands, not the outcome vectors' other effects.
- T-4 `cancel_move` resets a `DragMove` (pins the flow-agnostic
  reset).
- T-5 reconcile-on-vanish: a refresh without the dragged task (or its
  stack) cancels the flow.

**Seam — `tests/journeys.rs` (headless driver, raw pointer
dispatches per the `native_drag.rs` pattern):**
- T-6 the drag-drop journey: press a card, move past the slop into
  another column (assert the pushed `move_target` flipped through the
  model surface), release → the fake engine records exactly the
   `MoveTask` the keyboard journey records for the same transition;
  ghost closed, flow `None`, **no** `task.edit`/click side-effect.
- T-7 the interleaved-refresh journey: mid-drag, push a background
  refresh (new engine state, stable models) → the drag-open flag,
  ghost model and target highlight survive (asserted on the model
  surface); release still dispatches.
- T-8a release outside any column (over the catch-all/gutter) → no
  command, flow `None`, ghost closed.
- T-8b Escape mid-drag → same as T-8a (the teardown's
  `drag-finished(none)` is absorbed — asserts exactly one cancel, no
  dispatch).
- T-8c a leaving effect (NextView) mid-drag → same.
- T-9 the slop boundary: press, move 4 px (< 8), release inside the
  card → `clicked` fired (typed: the focus/selection outcome), no
  command; move 20 px, release → no `clicked`, the drag journey's
  assertions hold.
- T-10 the flow-busy refusal: with a create row open (flow active),
  press+move on a card → no drag arms (no `drag-armed` callback), the
  buffer survives.
- The **unchanged suites are the regression gate**: every round-1/2
  journey runs unedited (one behaviour, one test, lowest layer — the
  wrapper's click-preservation is proved by their continued
  greenness, not duplicated).

**No text assertions anywhere** (typed outcomes and model-surface
facts only); no `sleep` — the mock clock's handler pass
(`mock_elapsed_time(0)`) delivers property-change pushes, the
established pattern.

## 7. Stage-2 probes (compile/verify, recorded in the stage-2 report)

- **P-2** the catch-all `DropArea` receives DragMove over
  non-DropArea descendants (the ghost-feed coverage claim, §3.2) —
  one slint scratch or the T-8a journey itself; fallback recorded in
  §3.2.
- **P-3** `data:` bound to a global property satisfies the non-empty
  check; `slint::DataTransfer::from(...)` set once at mount.
- **P-4** `DragAction.move` member spelling / the
  `action != DragAction.none` translation compiles (§3.2 prefers the
  latter).
- **P-5** `changed dragging` on an instantiated DragArea fires into
  the slint closure (expected yes; T-6 covers it end-to-end anyway).

## 6. Tier-4 manual checklist (user runs; goes in the PR body)

1. **Slop feel**: 8-px lift — too eager on small nudges? too lazy on
   real drags? (A different slop re-opens D-C → DIY filter, recorded.)
2. **Ghost look & tracking**: snapshot-card fidelity at 0.5 opacity,
   centred-under-cursor at both scale breakpoints; smoothness across
   column gaps (P-2's fallback = freeze between columns).
3. **Target highlight**: the `surface-raised` header + badge tone
   flip readable against the r2 card surfaces.
4. **Escape sanity**: cursor/hover state after Escape-cancel; first
   move re-establishes hover; no stuck NoDrop cursor; a release after
   Escape never clicks.
5. **Compact**: pressing and dragging a card in compact mode does
   nothing (no DragArea there) — click/open still work.

## 8. Code inventory & estimated diff size

| File | Change |
|---|---|
| `ui/widgets/task-card.slint` | `TaskCardModel.dragging-source` + the dim paint; no DragArea here (the wrapper is the composition's) |
| `ui/compositions/board-layout.slint` | regular leg: per-column `DropArea`, per-card `DragArea`, catch-all rejecting DropArea, new callbacks, `DragState` import |
| `ui/app-window.slint` | `DragState` global (export), `board-drag-open` flag, ghost overlay sibling, the four new pure callbacks |
| `src/view_state.rs` | `Flow::DragMove`, begin/drag-target/confirm methods, `append_move_command` (confirm_move refactored onto it), reconcile arm, unit tests |
| `src/adapter.rs` | `install_drag_seams` (arm/dropped/finished, all flow-guarded), `teardown_runtime_drag`, interpret-table DragMove arms, settle arm, tooltip dismissal at arm |
| `src/models.rs` | `move_target` push extended, `board-drag-open` + ghost-model pushes, `dragging-source` on the source card |
| `CHANGELOG.md` | Unreleased entry |
| Tests | `view_state.rs` unit suite; `tests/journeys.rs` drag section |

**Estimated total ≈ 550–700 lines** including tests (slint ≈ 220,
adapter ≈ 160, view_state ≈ 140, models ≈ 90, journeys ≈ 130,
changelog/docs the rest). M-sized per dispatch §1 — the DIY design
would have been roughly double (slop, grab, hit-testing and
click-suppression all hand-rolled); the native surface is what keeps
this chunk at its M estimate.

## 9. Risks

- **R1 (design §14-R1) stable identity under the new gesture**: the
  ghost/dim/highlight are flow-push derived (survive refreshes by
  construction), T-7 pins it; the DragArea wrappers re-instantiate
  with the column/card handles exactly as cards do today (the
  repetition-probe pins from chunk 2 cover the mechanism).
- **R2 (design §14-R2) Slint fights back — new-surface edition**:
  DragArea/DropArea are young API; the sharp edges we lean on
  (release rewriting, hover skip, deferred `drag-finished`) are
  read from the pinned source (§1.2), and slint's own
  `native_drag.rs` pins the driving pattern. P-2 is the one open
  behavioural question, with its fallback recorded (§3.2).
- **R3 click/double-click regression**: guarded natively (§3.7) and
  by the unchanged-suite gate + T-9; the DragArea filter changes no
  focus semantics (it never touches focus).
- **R4 the interpret table grows arms** for a pointer flow: they are
  additive and cancel-shaped effects route to the existing
  `cancel_move`; `FocusNext/Prev` absorption is the only new
  non-routing behaviour (recorded §3.1).
- **R5 text assertions**: none — typed/model-surface only (§5).
- **R6 scope drift**: cross-board drops, position-within-stack,
  compact drag, kiosk gestures — all non-goals (header); anything
  discovered goes to `backlog:` per AGENTS §7.
- **R7 worktree hygiene**: this session's worktree was swept once
  already (header note); stage 2 recreates per the AGENTS §8 resume
  rule if missing, and stage 3 removes it at handoff as usual.

## 10. Deferred (recorded, out of scope)

- Position-within-stack drops (insert-at-index) — needs an
  insertion-position `MoveTask` shape; today's command is append-only.
- Compact-mode drag (switcher segments as drop targets) — a pointer
  affordance for a keyboard-first breakpoint.
- Cross-board drops (the ring's cross-board reach stays
  keyboard-only until boards can mount in one view).
- Ghost `drag-image` rasterization (native bitmap) — revisit if slint
  grows a render-element-to-image API.
- A bespoke slop token — only if Tier-4 rejects 8 px (D-C's
  re-opening condition).
