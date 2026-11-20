# Chunk Bui plan — UI residue bugs (#1004 #1026 #1033 #1036 #1053)

Round 3 dispatch chunk Bui, branch `fix/ui-r9-residue-bugs`.
Bases verified against landed `origin/main` @ `bca98b96` (2026-11-20).
Scope is normative: `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md` §B2
and the dispatch plan §8. Each issue's body is the fix's contract; every
root cause below was re-verified in the worktree before planning.

## 0. Re-anchors (round law [L](c))

The filings predate the last merges; the recorded causes were checked
against current main:

- **#1004** — cause intact. `ui/widgets/modal-dialog-host.slint`'s
  `panel := Rectangle` carries only `background: Theme.surface-raised`
  and `border-radius: Theme.radius-medium` — no `border-width`/
  `border-color`, no `drop-shadow-*`. `ui/compositions/capture-overlay.slint`
  (line 21) documents the anatomy promotion; the mount in
  `ui/app-window.slint` (capture arm, `walk-stops: 1`) wires only intents.
- **#1026** — cause intact, both halves. The palette mount
  (`ui/app-window.slint`, `if (root.palette-open)`) wires
  `accepted => root.palette-dismiss-requested()` under
  `kind: DialogKind.confirm`; the host fires `accepted` only from the
  edit-kind outside-click arm (R-4 in `modal-dialog-host.slint`) and the
  mount pushes an empty `buttons` model, so `accepted` is unreachable.
  The J2 rows: `.artifacts/plans/2026-11-18-widget-framework-ux-plan.md`
  §6 W-4 blanks the ActionId (`task.effort.set` → —) while §7 still
  lists `task.effort.set`. Code check: the effort verbs are **not** in
  the `ActionId` vocabulary on main (`src/actions.rs` carries no effort
  variant; a comment records "chunk 11 owns the effort edit/clear
  verbs"), so W-4's blanked row is the correct one and §7 must move.
- **#1033** — cause intact. `src/adapter.rs::resolve`: `(any, C::Nav) →
  SettleThenRun`; `UiEffect::StackNext/StackPrev` classify `Nav`
  (`src/router.rs`), so under `Flow::Move`/`Flow::MoveStack` they settle
  the flow then re-run against `Flow::None` (`interpret`'s
  `(Flow::None, StackNext/Prev)` arms → `step_stack`). Chunk 9's kept
  arms in `interpret_move` cover only `FocusNext|FocusPrev|
  FocusDirectional(_)`; the legacy stack-walk chords (`g l`/`g h`
  ViNormal, `Ctrl+Shift+Left/Right` Standard/emacs rows in
  `src/actions.rs`) still resolve `StackNext/Prev`.
- **#1036** — cause intact, **premise re-anchored**. The cause stands:
  `ui/views/plan-next-day-view.slint` line 61 carries
  `init => self.focus()` on the root `FocusScope`, and the walk's
  Planner stage (stage-kind 3) embeds the same view
  (`ui/views/review-walk-overlay.slint`, the `if (root.model.stage-kind == 3)`
  arm), so the embedded planner grabs slint focus at mount inside the
  walk dialog. The issue's premise that chunk 9 landed "a directional
  step table for `ContextId::ReviewWalk`" is **not** what landed:
  `interpret_directional` is keyed by `ViewId`, and
  `set_view_context` maps `ContextId::ReviewWalk` to *no view* (the
  non-routable-context return). The walk's keyboard semantics on main
  ride the mask walk (`MaskWalkFacts`) + the derived button row. This
  plan therefore fixes the focus ownership at the widget (decision D3
  below) and does **not** invent a context-keyed directional table —
  the planner-stage stop list rides the existing mask-walk stops (D4,
  #1053's exact count).
- **#1053** — cause intact. The walk mount's `walk-stops:
  1 + root.walk-model.rows.length` (`ui/app-window.slint` walk host arm)
  excludes the host's dialog button row, which is focusable
  (`TextButton` → `focus-on-tab-navigation`); the mask extent
  (`MaskWalkFacts.extent`) can under-count by `buttons.length`.

## 1. Fix order (shared-surface argument)

The walk grammar (`modal-dialog-host` mask arming, the walk host arm in
`app-window.slint`, `review-walk-overlay.slint`) serves #1036/#1053;
the host widget serves #1004 and the stops arithmetic's consumer. To
avoid churn:

1. **#1053** (walk-stops arithmetic — mount expression + probe pin)
2. **#1036** (planner focus gate — `plan-next-day-view.slint` + embed)
3. **#1033** (adapter kept arms — `interpret_move`)
4. **#1004** (host panel chrome — base widget styling)
5. **#1026** (dead seam removal + J2 doc reconciliation)

One branch, one commit per issue, each standalone (`fix(ui): ...`).

## 2. Per-issue decisions

### D1 — #1053: exact walk-stops arithmetic

**Fix**: the walk host arm's expression becomes
`walk-stops: 1 + root.walk-model.rows.length + root.walk-model.buttons.length;`
with a comment naming the D-5 button row as a counted stop (the host's
buttons mount `TextButton` with tab navigation). The leading `1` stays:
it is the body's own stop (the D-W1-4 declared body stop; the walk's
row list is the body's content and the cursor row's controls are
masked by the walk — recorded at the mount).

**Decision (open question resolved)**: count *all* buttons, not
"enabled" ones — the mount cannot know enabledness at the slint layer
(it is model metadata), and the mask extent is a declared bound (D-W1-4:
"declared stops"), not a realized focus census; the host's
`watch-mask-nonce` clamp already bounds the cursor to the declared
extent. Over-counting a stage whose buttons are fewer is impossible
here because the pass derives the model per stage — the expression
reads the same model the host renders.

**Pin (structural)**: extend `tests/probes/widget_framework_probe_tests.rs`
(G-10's walk-containment family): mount the host with a body stop count
and a 2-button model, assert `mask-extent == body-stops + buttons.length`.
Plus a mount-side pin in the walk probe
(`tests/probes/walk_probe_tests.rs`): the walk host arm's extent
reflects a pushed `walk-model` with rows + buttons. No text assertions;
assert the `mask-extent` out property.

### D2 — #1036: the embedded planner never grabs focus at mount

**Fix (two parts)**:

1. `plan-next-day-view.slint`: replace the unconditional
   `init => self.focus()` with a gate — new
   `in property <bool> take-focus-on-init: true;` and
   `init => { if (root.take-focus-on-init) { self.focus(); } }`.
   The standalone surface mount (`app-window.slint` `View.plan-next-day`
   arm) keeps the default (`true`) — standalone behavior byte-identical.
   The walk embed (`review-walk-overlay.slint` stage-kind 3 arm) sets
   `take-focus-on-init: false;` with the D-7 binding-site comment.
2. The walk keeps focus ownership through its existing machinery (the
   mask walk + the derived button row's initial focus); **no** new
   `ContextId::ReviewWalk` directional table is introduced — see the
   re-anchor in §0. When the journey reaches the planner stage, the
   user's Tab walks into the planner body (it keeps
   `focus-on-tab-navigation: true`); the planner still *receives*
   focus deliberately — it just never *steals* it at mount (the N-3
   restoration law composes).

**Pin (structural, journey semantics via the probe)**: extend
`tests/probes/walk_probe_tests.rs`: mount the walk overlay at
stage-kind 3 inside a host; assert the planner root's `has-focus` is
false after the mount transition and the host's focused-button seam
holds the walk's initial focus. The standalone counterpart (same probe
binary or `plan_window_max_probe_tests.rs` family): the standalone
mount *does* take focus on init (guards the property default). Both
assert `has-focus`/`focused-button` — structural facts, no text.

### D3 — #1033: StackNext/StackPrev join the move-flow kept arms

**Decision (the issue's open question resolved)**: add the kept arms —
the explicit stack verbs hop the drop column mid-move, mirroring
chunk 9's directional left/right arm. Rationale: J3 US-2's law is "the
move survives the hop"; the chords were the *original* #finding-2
repro shape, and ruling settle-then-run correct would leave two
contradictory semantics for the same column-hop gesture (Left/Right vs
`Ctrl+Shift+Left/Right`) on one live move. Symmetry wins.

**Fix**: in `src/adapter.rs::interpret_move`, extend the directional
left/right kept arm to also match `UiEffect::StackNext | StackPrev`
(same body: `cycle_move_stack_column` for `Flow::Move`,
`cycle_move_stack_target` for `Flow::MoveStack`, scroll hint on
non-empty outcomes) — map `StackNext → +1`, `StackPrev → -1`. Add a
comment naming the legacy-chord re-point. The resolver's `(any, C::Nav)`
cell is untouched: kept arms run before the resolver (the same shape the
`FocusNext/Prev` kept arm uses), so the settle never sees these strokes
under a move flow; every non-move context keeps `step_stack` verbatim
(the `(Flow::None, StackNext/Prev)` arms and the legitimate cancel path
for non-move flows stay pinned).

**Pin (journey)**: a new journey in
`tests/journeys/board_move_journeys.rs`: arm a `Flow::Move`, dispatch
the stack-walk chord's effect (`StackNext`), assert the flow is still
`Flow::Move` and the drop column stepped; the `MoveStack` counterpart
asserts the insertion slot stepped. Plus a negative pin (law [L](c):
the legitimate cancel stays): `StackNext` with `Flow::None` still steps
the stack walk (existing coverage asserts this — reference it; extend
only if no row covers the chord's effect enum directly).

### D4 — #1004: the host panel gains the dialog chrome (base-widget path)

**Fix**: in `ui/widgets/modal-dialog-host.slint`, style the `panel`
rectangle with the chrome the capture composition dropped —
`border-width`/`border-color: Semantic.focus-ring` (the capture
composition's pre-migration values) and a drop shadow
(`drop-shadow-blur`/`drop-shadow-color` per the theme's shadow token;
stage-2 picks the exact theme token — if no shadow token exists, add
one to `theme.slint` rather than inlining a literal, so the value stays
operator-tunable where the scrim token comment says tuning lives).
Base-widget styling only: **no** per-consumer restyle in the capture
mount or any other mount (the O-6 duplication ban; the scope report's
normative wording). All dialogs gain the chrome — that is the
issue's first option ("give the host panel a Theme border token (all
dialogs gain the chrome)") and it is **the recorded decision**.

**Pin (structural)**: extend
`tests/probes/widget_framework_probe_tests.rs` with a G-numbered test
asserting the host's new out properties (`panel-border-width > 0`,
`panel-border-color == Semantic.focus-ring`, `drop-shadow-blur > 0` —
expose observation seams on the host the way `panel-x/width` already
are). Structural property reads; no rendered-text or pixel assertions.

### D5 — #1026: the dead seam goes; the doc rows reconcile with the code

**Fix (two halves land in one commit)**:

1. `ui/app-window.slint` palette mount: delete the
   `accepted => root.palette-dismiss-requested();` line and leave a
   comment at the site: confirm-kind with an empty button model can
   never fire `accepted` (R-4: confirm outside-clicks route
   `dismissed`); the palette has no accept seam to wire.
2. `.artifacts/plans/2026-11-18-widget-framework-ux-plan.md` §7: the
   `task.effort.set` ActionId row is corrected to match W-4's blanked
   row (the effort commit dispatches `StateCommand::UpdateTask`
   directly; no `ActionId` exists), with a deferred note: chunk 11's
   effort edit/clear verbs, when they mint the ActionId, update W-4
   §6 and §7 **together** (the comment at `src/actions.rs` already
   records chunk 11's ownership). A landed historical plan file is
   amended with a dated addendum note rather than silent rewriting
   where §6/§7 are marked as landed inventories — stage-2 verifies the
   file's conventions and either edits the row in place with a
   `(corrected 2026-11-20, #1026)` marker or appends a delta note;
   the marker form is preferred so the disagreement itself stays
   auditable.

**Pin**: the existing G-11/US-1 AC-3 kind test
(`widget_framework_probe_tests.rs`) already pins confirm-kind
outside-click → `dismissed` — that is the unreachable-`accepted`
contract; no new test (a test asserting a handler's absence would pin
text/wiring shape, the round-law anti-pattern). The doc half is the
deliverable and is reviewed in the same diff.

## 3. Test inventory (post-R4 paths)

| Issue | Test file | Kind |
|---|---|---|
| #1053 | `crates/taskboard-ui-slint/tests/probes/widget_framework_probe_tests.rs` (+ `walk_probe_tests.rs`) | structural |
| #1036 | `crates/taskboard-ui-slint/tests/probes/walk_probe_tests.rs` | structural (focus facts) |
| #1033 | `crates/taskboard-ui-slint/tests/journeys/board_move_journeys.rs` | journey |
| #1004 | `crates/taskboard-ui-slint/tests/probes/widget_framework_probe_tests.rs` | structural |
| #1026 | (existing G-11 pin cited; doc reconciliation) | — |

New slint seams (`take-focus-on-init`, the host's panel-chrome out
properties) carry the `// slint: <callback/property>` comment per the
stage-2 prompt's convention.

## 4. Risks

- **R1 (#1004)**: chrome on the base widget changes every dialog. The
  G-catalogue geometry tests pin panel position/size — border-width
  paints inside the rectangle in Slint and does not shift layout, and
  a shadow affects neither, but stage-2 runs the full probe suite and
  any pinned pixel/golden that shifts is updated only if the pin is
  visual-tuning debt, never weakened structurally (law [L](c)).
- **R2 (#1033)**: the kept arm must not swallow the legitimate cancel.
  The resolver cells and the `(Flow::None, StackNext/Prev)` arms are
  untouched; the journey pin's negative half guards this.
- **R3 (#1036)**: removing the unconditional grab must not regress the
  standalone planner's initial focus — the property-default form keeps
  the standalone mount byte-identical, and the probe's standalone half
  pins it.
- **R4 (#1053)**: an exact count could break the containment journeys
  if any stage pushes fewer focusable stops than the declared extent —
  the clamp (`watch-mask-nonce` arm) bounds the cursor to
  `walk-stops - 1`, so over-declaration is safe; under-declaration is
  the bug being fixed. The r9 journey corpus must stay green
  (acceptance).
- **R5 (shared worktree state)**: main checkout sits on the pending
  `chore/artefact-sweep-2026-11-19` branch — this branch was cut off
  the fetched `origin/main` (`bca98b96`), not off that branch; a
  stage-2 rebase uses `git fetch origin --prune` first.

## 5. Gate & handoff

Stage-1 gate: pre-commit hooks on the plan commit +
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(background + poll; deferred per the [D] rule if the lock is held —
the plan commit lands before any wait). No push, no PR from this
stage; stage-2 implements per §2 in the §1 order, stage-3 opens the PR
referencing all five issues.
