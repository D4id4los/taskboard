# [Plan] GUI Review Round 3, Chunk 2 — Board Surface & Empty Board

- **Date**: 2026-10-26 (planned 2026-10-27)
- **Status**: PROPOSED — stage-1 plan for chunk 2 of the round-3
  dispatch. Branch `feat/ui-r3-board-surface`, cut from fetched
  `origin/main` (5ab0fd1) **after chunk 1's merge was verified**
  (`fix/ui-r3-flow-state-machine` deleted from origin; its commits
  e95a9a3…5ab0fd1 on `origin/main`).
- **Origin** (normative): design round
  `2026-10-26-gui-review-round3-plan.md` §3.2 (D1–D6), §1 findings
  R3-F1-half/2/4/5/6/9-residue/10/11/12/41, §2 fold-ins (checkbox
  half of #210, slint deprecations), §4 testing, §7 risks; dispatch
  plan `2026-10-26-gui-review-round3-implementation-dispatch-plan.md`
  §1/§3 (acceptance a–h). Chunk-1 artefacts this plan builds on: the
  visible-flow rule R5 and the focus-loss seam
  (`text-flow-focus-lost`, `board-layout.slint:111`) and the audit
  report `2026-10-26-ui-flow-state-audit-report.md`.
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/`).
  `taskboard-domain`, `taskboard-state`, storage, sync, app: **empty
  diff** — the preset machinery dispatches the existing
  `StateCommand::CreateStack` (`messages.rs:29-36`), done-toggling
  dispatches the existing `StateCommand::SetTaskDone`
  (`messages.rs:48-55`). No flow-machine changes (chunk 1 owns); no
  palette/menu (chunk 6).
- **All `file:line` anchors are post-chunk-1** (this worktree at
  5ab0fd1). The design round's older anchors were re-derived; every
  one below was read, not carried over.

## 1. D1 — the drag ghost is card-sized (R3-F4)

**Fact**: the ghost (`app-window.slint:490-497`) mounts a TaskCard
with no `width`/`height` binding inside a full-window Rectangle → the
card's default geometry fills the window; the user drags a
window-sized ghost. The ghost's *model* snapshot is already pushed
(`models.rs:748-756`); only the size is missing.

**Decision — capture on the slint side of the arm seam, store in
DragState, never cross geometry to Rust.**

- What is measured at arm time: the source card's rendered logical
  size — `drag-card.width` and `drag-card.height` (the card is fully
  laid out at the moment its DragArea crosses the slop; the DragArea
  height even binds `drag-card.preferred-height`,
  `board-layout.slint:440`).
- Where captured: the DragArea's existing `changed dragging` arm
  callback (`board-layout.slint:443-447`) — when `self.dragging`
  becomes true, alongside `board-card-drag-armed`. This is the *same
  lifecycle moment* the design names (`adapter.rs:2426-2446` — the
  Rust arm handler runs because of this callback); the capture simply
  stays on the presentation side, honouring the r2 D-E invariant that
  no column/card geometry crosses to Rust. The Rust drag arm is
  untouched.
- Where stored: `DragState` (`drag-state.slint`) gains
  `in-out property <length> ghost-width; in-out property
  <length> ghost-height;` — runtime-written like `pointer-x/y`
  (the established "never touched by the model push" class), so the
  refresh-survival invariant holds by construction: the models pass
  rebuilds `ghost-model` every pass but never the size.
- Binding: the ghost TaskCard (`app-window.slint:493-497`) gains
  `width: DragState.ghost-width; height: DragState.ghost-height;`
  (x/y centring unchanged).
- Ordering safety: the arm callback writes the size *before* Rust
  opens the flow and the refresh push mounts the ghost → the size is
  set no later than the ghost's first frame. No zero-size flash.
- Deviation note for the design text ("`DragState` gains the size
  … captured at drag-arm, `adapter.rs:2336`"): same seam, same
  moment, slint side; recorded here so stage 3 does not read it as a
  missed requirement.

## 2. D2 — the plus glyphs halve (R3-F5)

**Facts**: `PlusGlyph` locks a `space-6` (32 px) square
(`plus-glyph.slint:18-19`); `IconButton` is a `space-6` square with
`space-2` padding (`icon-button.slint:51-53,69`), so its plus bars
span `space-6 − 2·space-2` = 16 px (`icon-button.slint:235-250`).
32 → 16 is exactly the user's "~halve". Three sites: add-column strip
(`board-layout.slint:546`), add-task ghost regular (`:499`) and
compact (`:717`).

**Decision (one call, recorded): shrink PlusGlyph to the IconButton
visual.** `plus-glyph.slint` width/height become
`Theme.space-6 - 2 * Theme.space-2` with the square-contract comment
updated (why an expression and not a token: it *is* the IconButton
icon-box's derived size; a new theme token for one use is ladder
churn the token policy reserves for chunk 7's pass).

- Why not "adopt IconButton outright": the add-column affordance is a
  full-height slim strip whose generous hit target is an r2 D5
  decision — an IconButton (fixed 32 px square) cannot host it; the
  add-task ghosts keep their full-row hover paint + FocusScope
  scaffolding, and nesting an IconButton inside would create double
  hit targets. Shrinking the glyph is a one-component change that all
  three sites pick up automatically; D3's tooltip wiring is
  orthogonal and lands on the ghosts' own TouchAreas.
- The hover colour flip (`text-secondary` → `text-primary`) and
  centring stay per-site, unchanged.

## 3. D3 — Add Task ghosts speak (R3-F12)

**Facts**: New Stack's add-column has the generalized tooltip trigger
(`board-layout.slint:523-533`); both Add Task ghosts have bare
TouchAreas (`:488-490`, `:706-708`) — no tooltip.

**Decision:**

- New binding-hint projection `task-create-hint` (the `task.create`
  binding, `ActionId::TaskCreate`, `actions.rs:180`): window property
  `in-out property <[string]> task-create-hint`
  (`app-window.slint`), models push mirroring the `stack_create`
  row (`models.rs:649-657`, `rows(ActionId::TaskCreate)`), threaded
  window → BoardView → BoardLayout like its siblings.
- Both ghost TouchAreas gain the `changed has-hover` enter-arms/
  leave-dismiss wiring calling
  `TooltipState.hover(label, description, keys, x, y, w, h)`
  (`tooltip.slint:51-67`; empty keys render no key part).
- **Tooltip texts are presentation constants**: slint-side inline
  literals beside their wiring — the r2 D6 precedent ("New stack" /
  "Adds a new stack to this board.", `board-layout.slint:525-526`).
  The tooltip *label* reuses the ghost's existing accessible-label
  ("Add task", `:496`/`:714` — the tooltip never says anything the
  accessible tree doesn't); the descriptions are new literals
  ("Creates a task in this stack." — stage 2 owns exact copy; tests
  never assert it, the no-text-assertions rule). No Rust string
  constants, no theme strings global — the plan records this as the
  house convention rather than inventing a new constants module for
  four strings.

## 4. D4 — right-click on a stack's empty card space (R3-F6)

**Facts**: the stack card Rectangle (`board-layout.slint:245-251`)
hosts the column body; only the header (`:289`) and the compact
switcher segments (`:617`) carry `header-context`. The bare surface —
an empty stack's whole body, a populated stack's space below the last
card — does nothing on right-click.

**Decision — a body-context seam routed to the existing stack context
menu.**

- Signature (the header pattern exactly):
  `callback body-context(int, float, float);` on BoardLayout
  (column index + window-coordinate pointer), threaded as
  `pure callback board-body-context-requested(int, float, float);`
  through BoardView → app-window, landing in
  `install_object_context_seams` (`adapter.rs:2976-2999`) with the
  **identical body** as the header arm: resolve `stack_ids[column]`,
  `select_stack` outcomes, then
  `open_context_menu(&window, x, y, menu::STACK_CONTEXT)`. Extracted
  into one shared closure/fn so header and body cannot drift.
- Mount point: a full-size TouchArea declared as the **first** child
  of the stack-card Rectangle (lowest z — later siblings are
  hit-tested first, so header/fields/ghost/cards keep every touch
  they get today; only bare surface falls through). Its
  `pointer-event` fires on right-down only (the card/header pattern,
  `task-card.slint:95-101`); no click handler — left-clicks on bare
  surface stay no-ops (today's behaviour).
- Recorded probe **P-B1**: the ListView's viewport absorbs the task
  area's surplus height (`board-layout.slint:407-423` recorded
  deviation) — the journey probes a right-click *below the last
  card*. `mouse-drag-pan-enabled: false` (`:430`) means the Flickable
  has no pan gesture to steal clicks; if the probe still shows the
  viewport eating right-clicks, the fallback (recorded, in order):
  (a) move the TouchArea into the task-area VerticalLayout behind
  `lv`'s rows is *not* possible (no overlay in a layout) →
  (b) shrink `lv`'s `min-height: 0` viewport to its rows and let the
  TouchArea own the surplus — a geometry change gated on the probe's
  actual verdict, not applied speculatively.
- Scope: regular/expansive only — the compact leg has no column card;
  its switcher segments already carry the context seam. Recorded as
  accepted scope, not an omission.

## 5. D5 — the empty-board page & board presets (R3-F10/11/41)

**Facts**: the empty leg (`board-layout.slint:149-174`) already
carries chunk 1's minimal fixes — the New-stack IconButton and the
working create field with its `focus-lost` settle seam (`:166-173`).
The board bar renders only in the populated regular leg
(`:192-197`) → F11's tabs-less view. F41 wants a centered labelled
CTA plus "Create Default Kanban Setup".

**Decision — composition (per leg):**

```
BoardBar                     ← NEW in this leg (F11); width: 100%
  (conditional elements get no default fill — the :177-183 lesson)
EmptyState (hint text)       ← keeps R5: flows stay visible
CtaButton "New stack"        ← labelled (icon+text), centered
CtaButton "Create Default Kanban Setup"
[stack-create field]         ← chunk 1's, unchanged incl. focus-lost
```

- The leg keeps no compact gate (as today): at compact with zero
  stacks the centered block renders, the bar stays hidden — the
  existing breakpoint rule (compact hides the strip, keyboard
  switching works) applied consistently. Recorded.
- **Labelled CTA**: no generic Button primitive exists in the widget
  set. `board-layout.slint` declares
  `export component CtaButton inherits Rectangle` (label property +
  clicked callback; one control row tall — `space-6`; radius-small;
  surface-hover on hover; RoleText body label). Explicitly `export`
  because a file-local component *used by* an exported component
  triggers the implicit-export deprecation (the `ListView` warning is
  the live proof, §8). Two uses (New stack, preset) justify the
  component; a third use promotes it to `ui/widgets/` (rule-E note).
- The chunk-1 journey `stack_create_has_a_visible_click_equal_flow`
  (journeys.rs:1227) must stay green verbatim — the field keeps its
  seams; the leg only grows around it.
- Bar model: `board-bar` is board-keyed (pushed per live board, not
  per stack) → zero stacks renders the one tab. F11's repro
  (click a fresh empty board's tab → tabs-less view) is fixed at the
  root. The empty-board leg keeps the EmptyState text/hint literals
  (presentation; stage 2 may reword the hint freely — nothing
  asserts it).

**Decision — the preset table (`board_presets.rs`, new module in
`crates/taskboard-ui-slint/src/`):**

```rust
pub struct BoardPreset {
    pub name: &'static str,          // "default-kanban"
    pub stacks: &'static [&'static str], // Backlog / To Do / In Progress / Done
}
pub const DEFAULT_KANBAN: BoardPreset = …;
```

- Data-only module (pure, `#[forbid(unsafe_code)]` + MIT/Apache
  header apply — it's a library-crate member). The stack titles are
  *domain payloads* (they become `CreateStack.title`), not
  presentation strings — tests assert them as command payloads.
- Dispatch: adapter fn `apply_board_preset(adapter, window, preset)`
  → for `(order, title)` in `preset.stacks.iter().enumerate()`:
  `FlowOutcome::Command(StateCommand::CreateStack { board: selected,
  title, order })` + one trailing `RefreshNow`. Orders are
  **explicit 0..n** (not `append_stack_order`) — the four commands
  are dispatched before the engine applied any of them. The
  board-less refusal mirrors `begin_create_stack`
  (`view_state.rs:1079-1087`): no board → empty vec, no dispatch.
  The flow machine is untouched: preset creation is direct
  command dispatch, no text flow, so R1–R5 have no new surface.
- Wiring: `pure callback board-default-preset-clicked();` threaded
  window → BoardView → BoardLayout's second CTA →
  `apply_board_preset(…, &DEFAULT_KANBAN)`. Deliberately *not*
  index-parameterised: exactly one preset ships; the extensibility
  point is the table + the preset-parameterised dispatch fn (the
  second-preset test below proves it). If a second preset ever ships,
  the callback grows its index then (recorded decision).
- Race note: the button lives in the zero-stack leg; a sync merge
  adding stacks mid-flight could land 4 stacks onto a now-populated
  board. Out of scope (single-user kiosk/desktop; recorded).
- Bookkeeping: on merge, #210 gets the "checkbox half shipped"
  annotation (dispatch plan §9) — stage 3's job, listed here so it
  isn't lost.

## 6. D6 — the task-card checkbox (R3-F1-half, #210 fold-in)

**Facts**: `TaskCardModel.done` exists (`task-card.slint:37`, fed
from `task.done.is_some()`, `models.rs:1161`); done cards already
carry the strike/dim treatment (caption role + `text-secondary`
title `task-card.slint:118-123`, `background` rest fill `:63-65`).
`StateCommand::SetTaskDone { id, done }` exists
(`messages.rs:48-55`) with a live raw-send precedent
(`adapter.rs:3779-3789`, the notification Done button).

**Decision — anatomy:**

- **Left-edge gutter**: TaskCard's root VerticalLayout becomes
  `HorizontalLayout { done-box, VerticalLayout(unchanged content) }`.
  The honest anatomy (an overlay checkbox would underlap the
  space-2-padded title). Geometry shift ≈ one gutter width — golden
  regen is expected and recorded (§9), not fought.
- The checkbox: `space-4` (16 px) square, radius-small,
  `focus-ring-width` border in `text-secondary`; when `model.done`:
  `Semantic.success` border + check stroke (the `IconKind.check`
  path language — `icon-button.slint:217-233` — at gutter scale,
  drawn inline; TaskCard does not grow an IconButton dependency).
  A TouchArea `clicked => root.done-toggled()`.
- **Target semantics**: the callback carries no state — the wiring
  chain supplies the address:
  `done-toggled` (TaskCard) → `task-done-toggled(column-index,
  task-index)` / `(effective-selected(), task-index)` (BoardLayout
  delegates, both legs) → BoardView thread →
  `pure callback board-task-done-toggled(int, int);` → adapter:
  resolve `tables.board_task(column, row)` (stale index absorbed,
  the drag-arm precedent), load state,
  `a.send(SetTaskDone { id, done: !task.done.is_some() })`.
- **Why raw send, not the registry actions**: `task.complete`/
  `task.reopen` (`actions.rs:689-699`) operate on the *focused* task;
  the checkbox targets an arbitrary card. The notification Done
  button set the precedent (same command, raw send, same rationale).
  "click ≡ task.complete" is command-shape equivalence — pinned by
  the journey (§8) comparing the checkbox dispatch to the action's.
- **Announce semantics**: `accessible-role: checkbox` (Slint's
  AccessibleRole.checkbox) with a state-derived accessible-label
  (the done/undone verb pair — presentation literal, unasserted);
  the role + label pair is what the a11y tree announces. No local
  checked state exists anywhere — the box renders `model.done` only,
  so the acceptance criterion "cannot desync from the model" holds
  **by construction** (the refresh push is the single writer).
- **Done styling**: reused, untouched (`model.done` already drives
  title role/colour and card fill) — the checkbox adds no second
  done visual.
- Interaction edges (recorded): press-on-checkbox + move past the
  slop is a card drag (the wrapping DragArea owns the gesture) —
  correct and consistent with the card; a click toggles. On the
  ghost's TaskCard the checkbox is inert (the runtime suppresses
  hover/press over the ghost, `app-window.slint:495-497` comment).
- Scope: board cards both legs. Stack-detail/today rows use other
  primitives (no TaskCard import) — untouched, recorded.

## 7. Deprecation sweep — the complete inventory (R3-F2, fold-in)

`cargo build -p taskboard-ui-slint` on this worktree emits **17
slint warnings**: 15 deprecated-property uses, 1 implicit-export
component (emitted twice — two importers), plus 1 non-deprecation
warning that is a latent intent bug. The sweep's acceptance bar:
**zero slint warnings in the build log** (the gate evidence), whole
crate, not just touched files. Fixes:

| # | Site | Warning | Fix |
|---|---|---|---|
| 1 | `action-menu.slint:204,205,206,207` | `viewport-y` deprecated → `content-y` | mechanical rename (menu keyboard-scroll arithmetic) |
| 2 | `action-menu.slint:237,252,255` | `viewport-height` → `content-height` | mechanical rename |
| 3 | `action-menu.slint:254` | `viewport-y` → `content-y` | mechanical rename |
| 4 | `settings-view.slint:268` | `viewport-y` → `content-y` | rename **LHS only**: `content-y <=> SettingsScrollFacts.viewport-y` — the *global's* property name is ours and keeps its name; zero Rust churn (chunk 4's scroll anchoring reads the global, untouched) |
| 5 | `settings-view.slint:281,596,609,612` | `viewport-height` → `content-height` | mechanical rename |
| 6 | `settings-view.slint:611` | `viewport-y` → `content-y` | mechanical rename |
| 7 | `listview.slint:7` | "Component is implicitly marked for export" (emitted twice) | `component ListView` → `export component ListView` (additive generated-API surface; build+clippy prove no consumer breaks) |
| 8 | `board-layout.slint:250` | **"padding only has effect on layout elements"** — not a deprecation, but in-scope: the r2 D35 comment documents an intended inner padding that has silently never applied (plain Rectangle ignores `padding`) | move `padding: Theme.space-2` onto `column-body := VerticalLayout` (`:254`) where it has effect — the sweep's **one behavioural fix**; goldens regen + Tier-4 visual check flagged |

The proof stage 2 pastes: `cargo build -p taskboard-ui-slint 2>&1 |
grep -c "warning"` → 0 (after the touches invalidate the slint
build cache; a cached no-op build proves nothing).

## 8. Testing catalogue (TDD-first)

House rules carry: no sleeps (mock/paused clock where time
appears — none does here), no text assertions (labels/tooltip copy
are never asserted; strings are located/addressed by
`click_by_label`, which is structural addressing, not text
assertion), typed outcomes (`matches!` on `StateCommand`), one
behaviour one test lowest layer, deterministic by construction.

**Unit (pure logic):**

- `board_presets` + adapter dispatch: `apply_board_preset` on an
  empty board → exactly 4 `CreateStack` commands, orders 0..3,
  titles in table order, board = selected. **Second-preset test**
  (the extensibility proof the acceptance names): a synthetic
  `BoardPreset { name: "test-duo", stacks: &["A", "B"] }` → 2
  commands, orders 0..1. Board-less state → empty vec (refusal
  parity with `begin_create_stack`).
- Checkbox seam resolution: (column,row) → `SetTaskDone` with the
  toggled value, both directions; stale index → no command.

**Journeys (headless driver, `tests/journeys.rs`; harness API:
`press`/`type_text`/`stroke`/`click_by_label`/`engine.drain()`/
window-global reads):**

1. Ghost size: arm a drag on a known card → `DragState.ghost-width/
   ghost-height` equal the source card's rendered size (window-global
   read); the ghost TaskCard's bound width/height match (structural —
   no pixel assertions). Drop settles; existing drag journeys stay
   green.
2. Body context: right-click (probe P-B1) a task-less stack's bare
   space → `select_stack` applied + context menu open (menu-open /
   pointer-anchored state facts — typed, untextual).
3. Empty board: `fixture::empty_board_state()` → the board bar is
   addressable (its segment click target exists — F11 pinned by
   addressing, the strongest structural proof available); New-stack
   CTA opens the same flow state (`stack_create_open`) as the
   keyboard's `S`; the preset CTA drains 4 `CreateStack` commands.
4. Checkbox: click a card's checkbox → `SetTaskDone { id, done: true }`;
   click again → `done: false`; the focused-task keyboard route
   (`TaskComplete`) produces the same command shape (the click ≡ key
   pin); the model round-trip flips `TaskCardModel.done` on the next
   push.
5. Regression set: chunk-1's `stack_create_has_a_visible_click_equal_flow`
   verbatim; the r2 drag/interleaved-refresh pins; the flow
   transition-table harness (untouched — no new flows).

**Goldens/visual:** expected drift recorded up front — checkbox
gutter (D6), empty-leg composition (D5), column padding fix (§7 #8),
glyph shrink (D2). Regen in the same commits as their causes; no
stale-golden commits.

**Tier-4 (manual, this machine):**

- Ghost size *feel*: drag cards of one-line and multi-chip heights —
  the ghost tracks the cursor at source-card size at scale 1.0 and
  ≥2.0; no window-sized flash on arm; drop-miss cancels visibly.
- Checkbox *feel*: hit target comfort at scale 1.0/3.0; hover cue;
  toggle both directions; done card styling consistent with today's.
- Empty-board *feel*: fresh board → tabs visible → New Stack works
  (field, commit, first stack) → preset button yields
  Backlog/To Do/In Progress/Done; compact-width window: centered
  block usable, no bar (per rule); chunk-1 flow rules hold (Escape,
  click-out, field visible).

## 9. Commit slices (stage 2; standalone, rebase-clean)

1. `feat(ui-slint): the drag ghost captures the card's size` (D1)
2. `feat(ui-slint): the plus glyphs shrink to the IconButton visual` (D2)
3. `feat(ui-slint): the add-task ghosts grow tooltips` (D3, incl.
   `task-create-hint` projection)
4. `feat(ui-slint): the column body answers right-clicks` (D4)
5. `feat(ui-slint): the task card grows a done checkbox` (D6)
6. `feat(ui-slint): the empty board renders bar, CTAs and presets` (D5)
7. `chore(ui-slint): the slint deprecation sweep` (§7, incl. the
   padding fix + its golden regen note; gate log proof)
   — CHANGELOG Unreleased updated across 5–7 (user-visible items);
   order of 2–6 freely permutable after 1.

## 10. Risks & recorded probes

- **R-A ListView hit-testing vs body-context** (P-B1): the viewport
  absorbs the task area's surplus; if right-clicks below the last
  card never reach the new TouchArea, apply recorded fallback (b)
  (§4) — geometry change gated on the probe verdict, never applied
  speculatively.
- **R-B the padding fix is behavioural** (§7 #8): the column gains
  its documented-but-never-applied inset; goldens shift. The r2
  comment is the intent evidence; stage 3 reviews it against that
  comment, not against today's pixels.
- **R-C checkbox gutter regoldens every card** (D6): largest visual
  delta of the chunk; Tier-4 checklist covers density at min/max
  scale and compact cards.
- **R-D explicit exports grow the generated Rust API** (`ListView`,
  `CtaButton`): additive; `cargo clippy --workspace --all-targets`
  proves no consumer breakage.
- **R-E preset double-dispatch** (button re-click while commands are
  in flight): the leg unmounts only after the engine applies the
  stacks and the refresh push lands; a double-click inside that
  window appends twice. Accepted for v1 (single-user, engine
  latency ≪ human double-click); an issue may record it if stage 2
  can reproduce.
- **R-F `content-y` rename semantic drift**: the rename is
  Slint-sanctioned 1:1 (same property, new name); the scroll-pinned
  journeys (menu keyboard-scroll, settings scroll facts) are the
  regression harness — they must pass unchanged.

## 11. Bookkeeping

- Closes on merge: (nothing directly; #210 is annotated, not closed —
  its tracking half stays with DR-5/#489.)
- No new issues planned; stage discoveries go to `backlog:` per
  AGENTS §7.
- What opens next: chunk 3 (`feat/ui-r3-taskview-dates`) is gated on
  this chunk's merge (dispatch plan §1).
