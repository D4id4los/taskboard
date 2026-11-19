# [Plan] Row Cursor Walk — the rule-list selected-rule cursor

- **Date**: 2026-11-19
- **Kind**: implementation plan — round 9 chunk 12 (J3 US-6 + R-7),
  stage-1 output; TDD-first, no implementation in this commit.
- **Origin**: J3 `2026-11-18-keyboard-focus-ux-plan.md` (US-6 + R-7
  normative); dispatch plan §1/§13; keyboard research §5 finding 6 +
  §9 K4; AGENTS.md.
- **Status**: READY for stage 2.
- **Branch**: `feat/ui-row-cursor-walk` (this worktree,
  `../taskboard-WT-feat-ui-row-cursor-walk`, off `origin/main`
  be5403c7; precondition verified: chunk 9 merged as PR #1061,
  2026-11-19).
- **Worktree doctor**: exit 0 (156G free, 18298 MiB avail), run
  2026-11-19.

## 1. Scope

The rule list (`ui/compositions/rule-list.slint`, mounted inside the
TaskDetail editor surface, `ui/views/task-view.slint`) gains a
selected-rule cursor: walkable by the directional keys, verbs act on
the selected rule, pointer and cursor stay coherent. The app-wide
interactable-row cursor heuristic (J3 R-7) is **recorded** as a note
in `docs/widget_interactions.org` for chunk 18 to formalize — no
other widget converts this round.

**Naming correction (recorded finding)**: the dispatch prompt says
"the rule-list surfaces (settings rules)". The rule list actually
mounts in **TaskDetail** (`task-view.slint:1227`); the settings view
has no reminder-rule rows. The break-rules list (Today's
`TimersSection`) is a different roster with its own armed-row
machinery (`break_rule_target`/`break_rule_armed`) — out of scope
here; it is the heuristic note's first named non-adopter and is
backlog-filed as the R-7 rollout remainder.

Non-goals: break-rules/File-to/other row widgets (backlog), preset
chip walking (chips are add affordances, not rows — Tab stops
already), any change to the TaskDetail **disarmed** directional
grammar (chunk 9's rows are law, byte-identical while disarmed).

## 2. Decisions

### D-1 — Cursor state shape: id-keyed `Option<ReminderRuleId>` in `ViewState`

```rust
/// The rule list's selected-rule cursor (J3 US-6/R-7, round 9
/// chunk 12): `None` = disarmed; `Some(rule)` = the walked row.
/// Adapter-owned presentation state like `focused_habit` (the
/// task.select precedent) — never synced, never journaled. Id-keyed,
/// NOT index-keyed: the row model carries no ids across the seam
/// (rule-list.slint D2) and indices go stale on any roster change;
/// `reconcile` clamps the cursor against `IdTables.task_rules` every
/// pass (the `notification_cursor` clamp law).
rule_cursor: Option<taskboard_domain::ReminderRuleId>,
```

Home: `ViewState` (`view_state.rs`), beside `focused_habit` — the
same crate-internal, pure-presentation shelf as `BoardSelection`,
`focused_task`, `focused_habit`, and `plan_focus_pane`. Id-keyed over
index-keyed because every other cursor of the walk family keys on the
stable id and prunes at refresh (`focused_task` reconcile death pass,
`notification_cursor` clamp); a raw `usize` would survive roster
reorder pointing at the wrong rule, which id-keying makes
impossible. Accessors: `rule_cursor()`, private `arm_rule_cursor`/
`step_rule_cursor(state, direction)` (clamp, no wrap — the walk
ladder's law), `clear_rule_cursor()`.

Rejected: `Option<usize>` (stale-index hazard above); an
`enum RuleWalkState` wrapping armed+index (speculative generality —
one id answers every read).

### D-2 — Lifecycle (the task.select law)

Arm: first `NavFocusUp/Down` (the directional ids — `Direction::Up |
Down` arriving at `interpret_directional`'s TaskDetail arm) while the
subject's rule roster is non-empty arms the cursor **at row 0**
(arm-then-step: the arming stroke itself does not move past row 0 on
arm; subsequent strokes walk). Walk: each further vertical stroke
steps the cursor within `IdTables.task_rules` order, clamping at both
edges (no wrap — the `step_flat_domain` law).

Clear (all sites):
1. **View exit** — the leaving rule (`view_state.rs` leaving/D9
   drain): TaskDetail exit clears (same shelf as the one-subject
   law's death pass).
2. **Foreign settle** — any reconcile pass where the cursor'd rule is
   absent from `IdTables.task_rules` (delete from CLI, sync, sibling
   remove) clamps-to-None (prune, not re-point: a dead id never
   re-anchors to a live neighbour — the `pending_board_select`
   prune law).
3. **Subject change** — opening a different task's detail is a new
   roster: the open path clears (arm is roster-scoped).
4. **Overlay opens** — the due editor, labels, entries, any
   MaskWalkFacts host mount is a foreign settle: the opener path
   clears (the D-4 board-selection "Escape and every foreign settle
   clear" law, applied to overlay mounts).
5. **Global cancel** — the cancel-shaped effects' arm (the move
   flows' `cancel_move` family) clears, typed and cheap; Escape stays
   unambiguously a clear.

Never cleared by: refresh alone (live rule + same subject keeps the
cursor — the cursor survives a countdown-row text change), pointer
hover, or the horizontal control walk.

### D-3 — Which verbs act on the selected rule (the minimal set)

The cursor makes the **row-level verbs** keyboard-reachable; the
row-affordance seams stay the one dispatch path (parity = the cursor
is a pointer, not a new verb family):

| Verb | Trigger | Dispatches | Seam |
|---|---|---|---|
| Rule remove | `Activate` (Enter) on the armed cursor | `ActionId::ReminderRuleRemove` after minting `rule_target = Some(cursor)` | the existing one-shot `rule_target` (adapter.rs::rule_target) — the cursor activation mints it exactly as `on_task_rule_remove_clicked` does today (adapter.rs::on_task_rule_remove_clicked) |
| Jump-to-due | `Activate` on an **unarmed** (`armed == false`) cursor'd row | the `rule-jump-to-due-clicked` seam's existing dispatch (due editor open for the subject) | existing seam; the cursor selects *which* row's affordance fires — the jump is subject-scoped today, so the cursor'd armed bit gates remove vs jump |
| Unmute / preset chips / add-custom | **not row-level** | unchanged | list-level affordances stay Tab/pointer (R-7 names rows, not toolbars) |

Resolution rule (recorded, D-3): `Activate` on the cursor'd row
dispatches **remove** for an armed rule and **jump-to-due** for an
unarmed one — the row's own affordance set, read off the
projection's `armed` bit (`RuleRowModel.armed`). Remove stays
immediate (no staged confirm): the row's icon-button affordance is
immediate today, and R-5's strict parity law forbids giving the
keyboard a heavier grammar than the pointer for the same verb. (Undo
remains the safety net; the one-shot `rule_target` consumption is
unchanged.)

Edge: empty roster — `Activate` with the cursor disarmed is the
typed no-op (never mints a target); `Activate` with an armed cursor
whose rule died pre-dispatch re-resolves against the tables and
no-ops (the stale-index absorb, now the stale-id absorb).

### D-4 — Pointer/cursor coherence (click moves the cursor)

The rule row body gains a click affordance that sets
`rule_cursor = Some(row's rule)` (id resolved from `tables.task_rules`
at the index — the row handler's existing resolution pattern).
Clicking a row's remove icon ALSO moves the cursor there before the
dispatch (one site, the row's index handler) — pointer and cursor can
never disagree about which row is selected. No hover-follow: hover is
not selection (the `focused_task` precedent).

Render: `RuleRowModel` gains a `selected: bool` — derived in the
projection from `view_state.rule_cursor()` (never stored in the
model), rendered as the emphasis treatment (the `models.rs::habit_card`
`emphasis` pattern: the existing focus-ring/emphasis tokens, no new
Theme surface).

### D-5 — The TaskDetail directional row's amendment (scoped, recorded)

Chunk 9's table row reads `(TaskDetail, _) => control walk on every
direction`. Amendment: **while armed**, `Up/Down` walk the rule
cursor (the list's geometry is vertical; this row joins
`interpret_directional` as the declarative arm
`(ViewId::TaskDetail, Direction::Up | Direction::Down)` returning
`step_rule_cursor` outcomes when armed, falling through to the
tail's ladder when not). `Left/Right` stay the control walk in both
states. Disarmed behavior is byte-identical to chunk 9 (its
`focus_law_journeys` rows never arm the cursor, so they pass
untouched — the negative journey pins it).

The heuristic note (lands in `docs/widget_interactions.org`, chunk 18
formalizes):

> Interactable-row cursor heuristic (J3 R-7): every widget with
> interactable rows carries a selected-row cursor. The cursor is
> adapter-owned presentation state keyed by the row's stable id
> (never an index), armed by the first vertical directional stroke,
> clamped at refresh against the pass's id table, cleared on view
> exit and every foreign settle; pointer activation moves it; row
> verbs dispatch through the row's own seams, acting on the cursor'd
> row. Adopted: the rule list (round 9 chunk 12). Pending adopters:
> break-rules roster, File-to candidate rows (#1030), the planner's
> in-pane walk (#1041).

## 3. TDD plan (test-first order)

Evidence home: `focus_law_journeys.rs` (the directional family's
nominated file — the cursor walk is the directional ids' TaskDetail
arm). Tokens per J3 US-6; `on_slint_thread` for rig-based rows; no
sleeps, no text assertions, typed outcomes only.

1. **us6_s1 — the walk** (`us6_s1_rule_cursor_walks_the_roster`):
   seed a subject with 3 rules; disarmed vertical stroke arms at row
   0 (`selected` true on row 0 only, via the model projection);
   down/down/up steps the emphasis; clamp at both edges (extra
   strokes are the typed no-op); roster order == `IdTables.task_rules`
   order.
2. **us6_s2 — verbs act** (`us6_s2_activate_acts_on_the_selected_rule`):
   armed cursor on an armed rule + `Activate` → the engine's rule
   count drops by one (typed outcome: the `StateCommand` stream /
   post-state, not text); cursor on an unarmed rule + `Activate` →
   the due editor opens (the walk-facts/editing-facts structural
   read); cursor cleared after the one-shot consumption (remove
   consumes `rule_target` as today; the cursor itself survives the
   verb — D-2's "never cleared by refresh", the roster changed so
   the prune re-evaluates: assert the post-remove cursor state
   explicitly).
3. **us6_s3 — lifecycle** (`us6_s3_foreign_settles_and_exits_clear_the_cursor`):
   arm → exit TaskDetail → disarmed on re-entry; arm → open the due
   editor (foreign settle) → disarmed; arm → CLI-shaped delete of
   the cursor'd rule → pruned to `None` after reconcile; arm →
   subject change → cleared.
4. **us6_s4 — coherence**
   (`us6_s4_pointer_click_moves_the_cursor`): click row 2 → row 2
   emphasized; its Activate dispatches for row 2's rule.
5. **Negative (chunk-9 guard)**
   (`us6_n1_disarmed_taskdetail_vertical_stays_the_ladder`): no arm
   → vertical strokes move the control-walk stop exactly as
   `focus_law_journeys.rs` pins today; zero rules + vertical stroke
   → no arm, ladder continues (empty-roster gate).
6. **Fold** (`us6_fold_cursor_is_presentation_state`): the cursor
   lives in `ViewState`, appears in no `StateCommand`/journal/
   sync payload — asserted structurally (the round-trip fixture
   replays a walk and asserts the command stream contains no
   cursor-bearing variant).
7. **Heuristic note check**: `docs/widget_interactions.org` contains
   the heuristic note (doc check, US-6/AC-2's chunk-12 half; the
   W7 doc check stays chunk 18's).

Coverage gate (stage 2/3): `scripts/ux-coverage.sh
.artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate` for the
US-6 rows; MISSING on us6_s1 is not done.

## 4. Implementation order (stage 2)

1. Failing `us6_s1` (state shape + accessor + walk math in
   `view_state.rs` — pure, unit-testable before any slint).
2. Failing `us6_s2` (the `interpret_directional` TaskDetail arm +
   the Activate resolution; `IdTables` reads).
3. Projection: `RuleRowModel.selected` + reconcile clamp/prune
   (models.rs pass) — `us6_s3`'s prune cases.
4. slint: the row emphasis + click affordance + `selected` wiring
   (rule-list.slint, task-view.slint forward) — `us6_s4`.
5. Lifecycle clear sites (leaving rule, subject change, overlay
   openers, cancel arm) — `us6_s3`, `us6_n1`.
6. Fold test + the heuristic note edit + census row (symbol anchors
   per #775: `adapter.rs::interpret_directional`,
   `view_state.rs::rule_cursor`, `rule-list.slint::RuleList`).
7. Census entry: the rule-list cursor rides the existing
   reminder-rules surfaces row as an appended E cell + a dated
   comment (chunk 12, J3 R-7); no supersessions (chunk 9's rows
   stay law).

## 5. Risks

- **Armed-state hijack of Up/Down at TaskDetail**: while armed the
  control ladder is unreachable by vertical keys until a clear site
  fires. Recorded as the D-5 reading of R-7; if the operator
  dislikes it, the escape hatch is a dedicated arm/disarm chord
  (follow-up, not built now). The chunk-9 journeys are disarmed and
  unaffected.
- **Overlay-opener enumeration**: every TaskDetail overlay opener
  path must clear; a missed opener leaves a cursor armed under a
  mask. Stage 3 sweeps the opener list (due editor, labels, entries,
  effort, recur) — each is a D-2 site 4.
- **Refresh prune vs. walk race**: a sync refresh mid-walk with the
  rule still live must NOT clear (clamped by id presence, not
  position) — `us6_s3` pins the live-rule refresh keeps the cursor.
- **Index-handler coupling**: the row icon handlers resolve
  index→id; the cursor mint must use the same resolution site to
  stay coherent (one helper, two callers).

## 6. Diff estimate

Small (S): ~120 lines `view_state.rs` (state + walk math + clear
sites), ~80 `adapter.rs` (table arm + activate resolution + clear
wiring), ~40 `models.rs` (projection + clamp), ~50 slint
(rule-list.slint + task-view.slint), ~250 tests (journeys + fold),
~15 docs (census + heuristic note). ≈ 550 lines across 8–9 files.
