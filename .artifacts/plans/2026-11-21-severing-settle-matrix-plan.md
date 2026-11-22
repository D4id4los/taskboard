# Plan] The Severing + Settle-Matrix Chunk — View-Root Max-Width Sweep & Per-Variant Flow-Table Kept Arms

- **Date**: 2026-11-21
- **Kind**: chunk plan — implementation spec for chunk S9 of backlog-reduction
  round 4 (the two bigger refactors). Behavior-identical refactors; the
  existing corpus is the oracle.
- **Origin**: issues #1029 and #1037 (bodies normative); round-4 scope
  report `.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`
  (§B-S9 framing: "the two bigger refactors finally unblocked"); dispatch
  plan `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  §1 S9 row, §4 prompt S9.x; precedents `8698fc0f` (task-view severing)
  and the plan-next-day twin (`tests/plan_window_max_probe*.slint/rs`,
  J4 US-3 AC-2); investigation report
  `.artifacts/reports/2026-11-14-task-view-window-max-width-collapse-report.md`.
- **Status**: READY — both items are self-contained refactors in
  `taskboard-ui-slint`; no cross-crate surface delta; no UX plan in
  origin (both are engine/test-side; #1029 is a latent-bug prevention
  sweep with no user-visible change today).
- **Branch**: `refactor/severing-settle-matrix` (one branch, one PR —
  the two items are independent; if stage 2 finds item 2 ballooning,
  splitting into two stacked PRs is permitted, item 1 first).

## 1. Scope

Two items, each mechanical with a hard oracle:

1. **#1029 — the layout-severing max-width rule at every view root**
   plus a `window_max_probe`-family regression probe per view. The
   collapse class (a bounded-max row collapses the view root's
   horizontal layout max; Slint's winit backend applies it as the OS
   window's max size) is proven for `TaskView` (`8698fc0f`) and
   `PlanNextDayView` (round-9 chunk 7). The remaining view roots carry
   the same latent exposure; unit-level assertions never catch it
   (the collapse only materializes through the event loop), so the
   fix is a uniform sever + a per-view probe.
2. **#1037 — the flow-settle matrix's FK kind vocabulary** cannot
   express per-variant kept arms within one variant family: two flows
   of the same `FK` kind cannot carry different pair-table rows, which
   is why the J4 chunk minted `FK::PlanChild` plus ~20 mirrored rows.
   Give the pair layer a per-variant key so the next family-splitting
   flow does not repeat the dance.

Non-goals (homes recorded in §6): upstreaming the Slint constraint
behavior (#8698fc0f already opened it — do not reopen); changing any
production matrix semantics (every `Exp` cell stays value-identical);
rethinking the class layer's coarse-kind vocabulary (only the pair
layer gains resolution); severing overlays/widgets (the R6-4 idiom
already lives where needed there: `task-card.slint`, `stat-bars.slint`,
`action-chip.slint`, `ghost-opener.slint`).

## 2. Item 1 — #1029: sever every view root + per-view probe family

### 2.1 Current state (verified on the worktree)

`Theme.layout-severing-max-width` (`ui/theme.slint:80`, `99999px`)
exists; severing currently applied only at:

- `ui/views/task-view.slint:243` (`TaskView`)
- `ui/views/plan-next-day-view.slint:79` (`PlanNextDayView`)

Unsevered view roots (the sweep's targets):

| View | File | Root |
|---|---|---|
| `TodayView` | `ui/views/today-view.slint:58` | `FocusScope` |
| `BoardView` | `ui/views/board-view.slint:36` | `FocusScope` |
| `StackView` | `ui/views/stack-view.slint:20` | `FocusScope` |
| `ReviewView` | `ui/views/review-view.slint:283` | `FocusScope` |
| `SettingsView` | `ui/views/settings-view.slint:648` | `FocusScope` |
| `HabitHistoryView` | `ui/views/habit-history-view.slint:61` | `Rectangle` |

**Decision D1**: the sweep covers all six, including
`HabitHistoryView` even though the issue text predates it and names
only "Today/Board/Stack/Review/Settings". The issue's law is "every
view root"; the habit-history view is a view root that landed after
the issue was written. Deliberate leniency documented here, not in
the issue.

**Decision D2**: `review-walk-overlay.slint` is NOT a view root (it is
an overlay surface mounted inside the review journey, not a windowed
view root that bounds a window's max) — out of scope. If stage 2 finds
it is in fact a windowed root, sever it too and note the correction
in the PR body; it is the same one-line idiom.

### 2.2 The change

Per view root: one line at the component root's property block,

```slint
max-width: Theme.layout-severing-max-width;
```

matching the exact form of the two precedents (task-view line 243,
plan-next-day line 79). No other layout changes.

### 2.3 The probe family

Follow the plan-next-day twin verbatim (it is the newest house
pattern; `8698fc0f` is the original):

- One probe `.slint` per view under `crates/taskboard-ui-slint/tests/`:
  `today_window_max_probe.slint`, `board_…`, `stack_…`, `review_…`,
  `settings_…`, `habit_history_…`. Each imports its real view, mounts
  it `width: 100%; height: 100%` inside a bare `Window`, carries the
  view's model properties as `in property`s, sets NO explicit window
  width/height (the existing probes' load-bearing comment: an explicit
  width replaces the root's own layout info — the probe would read
  back its canvas, not the constraint merge under test).
- One test module per probe under `tests/probes/`, composed through
  `tests/probe_composition.rs` (`#[path = …] mod …;` — the existing
  two probes' wiring), each with one test in the exact shape of
  `the_plan_next_day_view_never_bounds_the_window_max_width`:
  `run_probe` via `support::driver::run` with
  `headless_renderer: false`, `advance_probe_clock(0)`, then
  `ensure_item_tree_instantiated` + `layout_info(Horizontal)` and the
  `h.max >= 99_999.0` floor assertion (a floor on garbage, not a
  ceiling on success — quality-gate clean).
- Wire each probe into `build.rs`'s probe list (currently
  `window_max_probe`, `plan_window_max_probe` at lines 46–47).

**Decision D3 — mount fixtures**: each probe mounts the view's model
properties at their `Default` (the plan probe's
`PlanModel::default()` pattern; Slint structs derive `Default`).
Where a view's default mount leaves a conditional row unmounted
(e.g. `TodayView`'s timers/habit sections behind `timers-visible:
false`), that is acceptable: the probe's assertion is the severing
invariant at the root, not the presence of any specific row — the
severed root bounds the merge regardless of which rows are live.
Stage 2 should not chase "interesting" mount states; default mounts
keep the family uniform. Exception: if a view's default mount panics
or fails `show()` on the testing backend, arm the minimal properties
that make it mount and record the choice in a probe comment.

**Decision D4 — probe granularity**: one probe file + one test module
per view (six files each side), mirroring the precedent layout rather
than a parameterized mega-probe. The precedent's per-view split keeps
each mount's fixture local and each failure named; a shared harness
would couple six views' model imports into one file for no gain.

### 2.4 Verification

- TDD where observable: for at least ONE of the six views, do the
  two-way check the precedents did — run the probe before applying
  that view's sever line and record the collapsed `h.max` in the PR
  body, then apply and record the pass. The other five ride the same
  mechanical form; their probes still must pass post-sever.
  (Pre-fix failure is not guaranteed for every view — a view may have
  no bounded-max row mounted under the default fixture today. The
  probe is the guard for future rows, exactly like the issue says:
  "the collapse only materializes through the event loop, so
  unit-level assertions never catch the remaining roots.")
- The probes run on the testing backend inside `cargo nextest run
  --workspace` (the existing two probes already do — no new gate
  machinery).

## 3. Item 2 — #1037: per-variant kept arms in the flow-settle matrix

### 3.1 Current shape (verified)

`crates/taskboard-ui-slint/src/flow_table_tests.rs` (5793 lines):

- `FK` (line 38): the coarse flow-kind enum — 16 variants; `kind_of`
  (line 99) maps every `Flow` variant onto one, total.
- `class_table()` (line 482): `(FK, EC) → Exp` — the class-level
  defaults.
- `pair_table()` (line 783): `(FK, EK) → Exp` — the kept explicit
  arms; `lookup` (line 1404) resolves pair first, class fallback.
- The collision: `Flow::EntryDatePicker | WalkFilePick | WalkDatePick
  → FK::Child`, and the J4 embedded `Flow::ReviewPlanner` needed its
  own arms → a new `FK::PlanChild` kind plus a mirrored block of ~20
  rows (`FK::PlanChild, E::…` rows at lines 1374–1400 and the class
  rows at 660–689). The next split repeats the dance.

### 3.2 The change — Decision D5: a per-variant override layer on top, not a key rename

Introduce a `FlowTag`-shaped per-variant key and a third lookup rung;
leave `FK`, `kind_of`, `class_table`, and every existing `Exp` cell
value-identical:

```rust
/// One arm per `Flow` variant — the pair layer's per-variant key
/// (#1037): two flows sharing a coarse `FK` kind may now carry
/// different kept arms; each is declared once at the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FlowTag { None, Create, CreateStack, /* … one per Flow variant, total */ }
fn tag_of(flow: &Flow) -> FlowTag { /* compiler-total mirror */ }
fn kind_of_tag(tag: FlowTag) -> FK { /* tag → its coarse kind */ }
```

Lookup becomes three rungs, precedence order unchanged in spirit
(most specific first):

1. **variant rows**: `(FlowTag, EK) → Exp`
2. **pair rows**: `(FK, EK) → Exp` (existing table untouched)
3. **class rows**: `(FK, EC) → Exp` (existing table untouched)

```rust
fn lookup(tag: FlowTag, key: EK, class: EC) -> Exp {
    if let Some(e) = variant_table().find(|(t, k, _)| *t == tag && *k == key) { return e; }
    let kind = kind_of_tag(tag);
    // … existing pair → class fallback verbatim, keyed on `kind`
}
```

- **Migration**: move the `Flow::ReviewPlanner`-specific kept arms —
  the rows that exist only because of the `PlanChild`/`Child`
  collision (the `FK::PlanChild` pair rows that diverge from the
  `FK::Child` family: `WalkAdvance/WalkExit/WalkVerb/WalkFileArm`,
  `PlanBegin`, `PlanRemove`, and the divergent mirrored block at
  1374–1400) — into `variant_table()` keyed on
  `FlowTag::ReviewPlanner`. `FK::PlanChild` survives as a *kind*
  (it still drives the class-layer defaults and the `Post::Kind`
  assertions); it simply stops being the pair layer's collision
  workaround.
- **Totality law preserved**: `tag_of` and `kind_of_tag` are
  compiler-total mirrors exactly like `kind_of` — a new `Flow`
  variant without an arm fails at compile time in the mirror and at
  the driver via the missing-row panic. The existing
  `the_matrix_covers_every_class_cell` totality test (line 2001)
  gains a variant-layer coverage check (every `(FlowTag, EK)` variant
  row names a real tag — or, simpler, stage 2 extends the driver to
  assert every variant row's tag+key resolves) so the new rung cannot
  silently rot.
- **The driver** (`every_flow_effect_pair_resolves_to_its_matrix_row`,
  line 1824) iterates `all_flows()` — it switches to arming by tag
  (it already arms by `Flow` value; only the lookup call sites change
  to pass `tag_of(&flow)`).

**Decision D6 — why overlay-rung instead of renaming the pair key to
`FlowTag` outright**: renaming would force rewriting every existing
pair row to per-variant keys — ~500 mechanical row edits that touch
cells whose values are already correct, with real merge-conflict
surface against the branches that land around this one, and zero
semantic gain (homogeneous families like `FK::Text` legitimately
share their kept arms across their variants). The overlay rung adds
resolution exactly where the collision lives and costs ~40 lines of
mirror + table. If stage 3 finds the variant table growing past the
pair table, that is a recorded follow-up, not this chunk.

**Decision D7 — assertion oracle unchanged**: `Post::Kind(FK)` stays
the post-flow assertion vocabulary; the corpus (every current `Exp`
value) is the oracle — `cargo nextest run` before/after must produce
identical pass sets with zero `Exp` value edits. The only permitted
test-file edits: the new `FlowTag` mirror, the variant table, lookup,
and the moved `PlanChild`-collision rows (moved, not re-valued).

### 3.3 Verification

- Full `cargo nextest run --workspace` through
  `scripts/run-quality-gates.sh` (the S9 gate; flock-safe; background
  + poll beyond ~5 min per the dispatch law [D]).
- Pre/post identity: run the flow-table tests before the refactor,
  capture the pass list, compare after. Any difference = a cell was
  re-valued = a bug in the refactor, not a test.

## 4. Traps

- **Behavior-identical law [L](c)**: both items are M/L refactors
  with the corpus as oracle. No `Exp` value changes (item 2), no
  layout change beyond the sever line (item 1).
- **The probe's no-explicit-width rule**: an explicit window
  width/height in a probe `.slint` silently neuters the assertion
  (reads the canvas, not the merge). Copy the precedent comment
  verbatim into every new probe.
- **Slint struct Defaults**: verify each view's model struct derives
  `Default` before relying on `::default()`; the plan probe proves
  `PlanModel` does. Where one does not, construct the minimal fixture
  (Decision D3's exception path).
- **`FK::PlanChild` must not be deleted** — it remains the class-layer
  kind and the `Post` vocabulary for `Flow::ReviewPlanner`. Only its
  pair-layer role is retired.
- **Text-assertion law**: the probe assertions assert the structural
  fact (`h.max >= 99_999.0`), never Slint error text.
- **No sleeps**: probes use the existing `support::driver` +
  `advance_probe_clock(0)` pattern; the collapse rides the explicit
  `ensure_item_tree_instantiated` walk, not a wait.

## 5. Sizing

- Item 1: 6 one-line `.slint` edits + 6 probe `.slint` files (~20
  lines each) + 6 test modules (~60 lines each) + build.rs/composition
  wiring → **~550 lines, near-all additive**.
- Item 2: `FlowTag` mirror + variant table + lookup + row migration
  in one file → **~150 lines, mostly moves**.
- Total: **~700 lines, one branch, one PR** (`refactor/ui-severing-
  settle-matrix` branch name already fixed by the dispatch plan row;
  keep `refactor/severing-settle-matrix`).

## 6. Out-of-scope findings → backlog homes

Recorded per the standing authorization if discovered during stage 2;
known at planning time:

- The upstream Slint bounded-max-row collapse behavior already has its
  issue from `8698fc0f`'s out-of-scope note — do not reopen.
- If the three-rung lookup leaves `pair_table` visibly shrinkable
  into variant rows (Decision D6's follow-up), open a `backlog:`
  issue at stage 2/3 rather than widening this chunk.

## 7. Stage-2 prompt seed

Implement both items on `refactor/severing-settle-matrix` per this
plan §§2–3, Decisions D1–D7 binding; gates = pre-commit hooks +
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(commit landed work before any wait; on lock contention report
"gate deferred: lock held by <holder>" and end the stage). Commit
style: `refactor(ui-slint): the view-root severing sweep + probes`
and `refactor(ui-slint): the flow table's per-variant kept-arm rung`
(two commits, one per item). Hand off at PR-open; the dispatcher owns
the merge and the at-merge closures (#1029, #1037).
