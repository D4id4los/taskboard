# Directional Focus — Implementation Plan (round 9, chunk 9, K2)

- **Date**: 2026-11-19
- **Kind**: stage-1 dispatcher plan, TDD-first. Branch
  `feat/ui-directional-focus`, worktree
  `../taskboard-WT-feat-ui-directional-focus` (off `origin/main`
  8ac7230c).
- **Normative sources**: J3 `.artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md`
  (R-1..R-8, US-1/US-2; ACs quoted where binding), round plan §1 O-8,
  research `.artifacts/reports/2026-11-18-round9-keyboard-focus-research.md`
  §1–§4, dispatch plan §1/§10 (incl. the #875 RIDER).
- **Precondition verified**: chunk 1 (PR #969) merged — presets.rs
  carries the `Ctrl+Shift+<lowercase>` convention
  (`presets.rs:247-302`) and the chorded-letter case law
  (`stroke.rs::normalize_chorded_case`). Chunk 6 (PR #1028) merged —
  habit rows ride the task-card substrate behind
  `presentation.rs::RowSubject`; `adapter.rs::focused_habit` and
  `menu_row_subject` exist. Chunk 8 (`feat/ui-habit-keyboard-parity`)
  is **not merged** — its preset rows are a coordination risk, not a
  fact (§8 below).

## 1. The law (one paragraph stage 2 must not drift from)

The four directional moves are **focus movement, never task
selection**. They exist in every *view* context of every preset (R-1:
emacs `C-f/C-b/C-n/C-p`; vi `h/j/k/l` in ViNormal; Standard
`Left/Right/Up/Down` — all three families behind the existing
text-entry machinery, which is inherited for free by riding the
resolver path). During a board move the same keys move the *move
target* (kept arms; the move survives). In TaskDetail they walk the
widget stop ladder and **never write `subject_task`** (R-3: the
subject walk is removed, not re-homed — round-8 keymap-parity D-2 is
superseded by dated census entry). Today's left/right is a control
walk first cut (R-4). The planner gets the directional grammar (R-5).
Per the #875 RIDER, a dedicated selection-extend ActionId + explicit
Shift rows gives the printable-bound vi walk rows Shift parity; the
named-key arrow extension (adapter.rs:1097-1115) is **kept verbatim**
— not weakened, not re-routed.

## 2. New vocabulary (actions.rs / router.rs)

### 2.1 ActionIds (house `nav.*` convention, registry entries in `VIEW_NAV`-shaped context sets)

| ActionId | spelling | meaning | contexts in the registry |
|---|---|---|---|
| `NavFocusLeft` / `NavFocusRight` / `NavFocusUp` / `NavFocusDown` | `nav.focus-left` etc. | move the focus one step in the direction; interpretation is per-view (§4) | `VIEW_NAV` = Board, Today, StackDetail, TaskDetail, Settings (the five view contexts; **never** Global — Global is chain-visible under ViInsert and would fire while typing) |
| `NavExtendNext` / `NavExtendPrev` | `nav.extend-next` / `nav.extend-prev` | selection-extending twin of the item walk (#875) | Board only (the D-4 R-1 scoping: extension is a board surface) |

All six are plain `ActionId` enum variants with the standard registry
entry + `describe` row (actions.rs). `NavFocus*` route in router.rs to
one new effect; `NavExtend*` route to their own effects.

### 2.2 UiEffect + EffectClass

- `UiEffect::FocusDirectional(Direction)` — `Direction` is a tiny
  pure enum `Left | Right | Up | Down` (define in router.rs next to
  UiEffect, or in presentation.rs if models need it — stage 2 picks
  the home that keeps domain clean; taskboard-domain must not change).
- `UiEffect::ExtendSelectionNext | ExtendSelectionPrev`.
- `EffectClass`: both map to `EffectClass::Nav` (the walk family's
  class). This is load-bearing: a foreign flow (text/due editor) that
  meets a directional press settles-then-runs, exactly as
  `FocusNext/Prev` do today (`adapter.rs::resolve`, `(_, C::Nav)` arm).
  No new class — no new matrix row.

Router mapping (router.rs dispatch + the pair-table harness mirrors):

```
NavFocusLeft  → UiEffect::FocusDirectional(Direction::Left)
NavExtendNext → UiEffect::ExtendSelectionNext
```

## 3. TDD order (stage 2 follows this sequence; each red test names
its AC)

1. **Vocabulary fold test** (US-1): the four `NavFocus*` ids resolve
   in all five view contexts of all three presets; the pair-table
   harness asserts `FocusDirectional` for each id↔effect pair; the
   `presets_are_intra_context_conflict_free` gate stays green.
2. **Text-entry guard negatives** (US-1 AC-1): per preset family —
   emacs `C-n` while a field holds focus does not dispatch (editor
   scope in-field cursor command wins, keyscope.rs:431-434); vi `j`
   under ViInsert reaches the field (shadowing-without-masking);
   Standard arrows in a field move the cursor (printable/named-key
   monopoly). These are seam tests over the existing scopes — they
   must pass *unchanged* by construction; if one fails it is a bug in
   the vocabulary placement (contexts must never be Global), not in
   the scopes.
3. **Per-view interpretation journeys** (US-1 AC-1, tokens
   `us1_s1`..`us1_s3` per view): one journey per audited view,
   pressing the preset's four directional keys and asserting the
   focus/selection outcomes of §4's table (typed structural facts,
   never label text).
4. **Move kept-arms journey** (US-2/S1, token `us2_s1`): arm move →
   directional press → flow still live + drop target moved → Activate
   completes. Plus the DragMove absorb assertion.
5. **#875 parity tests**: vi `J`/`K` extend on Board; the named-key
   arrow extension test (already green) still green; the intercept
   extension does not double-fire when the row resolution *also*
   carries the shift bit.
6. **Subject-walk removal** (US-1 AC-2): the TaskDetail journeys
   assert the four keys move the control walk / never change
   `detail_title`; the retired round-8 tests are deleted **with** the
   census supersession entry landed in the same commit (§7).

## 4. The per-view interpretation table (adapter.rs — one declarative
home, the `push_scroll_page_hint` precedent, adapter.rs:4768)

Shape: a single `match` on `(self.view_state.view(), direction)`
(+ flow peek for the board rows) inside the `FocusDirectional` arm of
the interpret table — **no per-view ifs in the router** (stage-3
review mandate (c)). Every view in the research's audit table gets a
row or an explicit exclusion comment:

| View | Left/Right | Up/Down |
|---|---|---|
| Board | stack walk (the `step_stack` bodies behind `UiEffect::StackNext/Prev`) | task walk (`advance_focus`/`retreat_focus`) |
| Board + `Flow::Move` | **new** drop-column hop (`cycle_move_stack_column`, keep row index, clamp; `push_move_scroll_hint` fires) | `cycle_move_target` ring (the existing FocusNext/Prev arm's body) |
| Board + `Flow::MoveStack` | adjacent-slot hop (same shape over the stack reorder pick) | clamp/inert (no vertical domain — recorded in the arm comment) |
| Board + `Flow::DragMove` | absorb (`Vec::new()` — the 4052 precedent; the pointer owns the ring) | absorb |
| Today | **control walk** (R-4 first cut: the `NavNextControl/PrevControl` bodies) | item walk over `today_domain` (habit rows ride — chunk 6's substrate) |
| TaskDetail | control walk along the row's horizontal group | control walk along the `TaskViewWalkFacts` stop ladder (title → tracking → due → scheduled → priority → effort → labels → rules → recur → description). **Never `subject_task`.** |
| StackDetail | control walk | item walk (existing domain) |
| Settings | control walk | control walk (r4 D19 behavior kept) |
| PlanNextDay (open) | pane switch pool ↔ boxes (R-5) | walk within the active pane |
| Masked modals (Menu, Palette, ReviewWalk, FocusMode, TrackEntries, DatePicker, overlays) | **excluded — no `NavFocus*` preset rows in those contexts**; the masked grammars own their cursors (research §2) | same |

The planner arm (R-5) is the only genuinely new walk: reuse the
planner's existing selection seam (whatever `plan-next-day-view.slint`
exposes via the mounted model); if the planner has no in-pane cursor
seam today, land pane-switch only and file the in-pane walk as a
follow-up backlog issue (R-5 says "gets the directional grammar" — a
two-pane grammar with pane switch + in-pane rows is the acceptance;
do not invent a planner re-architecture in this chunk).

## 5. Move kept-arms fix (US-2, research §3 root cause)

Root cause stands as researched: `StackNext/Prev` are
`EffectClass::Nav` → `(any, C::Nav) → SettleThenRun` (adapter.rs
`resolve`) → the move cancels. Under the new vocabulary the preset
rows no longer bind `StackNext` to the directional chords, but the
kept-arm rows are still required in the interpret table *before* the
foreign fallthrough — the same table region that keeps
`FocusNext | FocusPrev` (adapter.rs:4055-4060 and the
`interpret_move` bodies at 5097-5111):

- `(Flow::Move, FocusDirectional(Left|Right))` → `cycle_move_stack_column`
  (new view_state method: keep the picked row index, hop
  `target_stack` to the adjacent stack with clamp; reuse
  `push_move_scroll_hint`'s `target-a`/`target-b` hint halves).
- `(Flow::Move, FocusDirectional(Up|Down))` → `cycle_move_target`
  (the ring; byte-identical to the FocusNext/Prev kept arm).
- `(Flow::MoveStack, FocusDirectional(Left|Right))` → adjacent-slot
  hop over the reorder pick.
- `(Flow::DragMove, FocusDirectional(_))` → `Vec::new()` (absorb).

Because `StackNext/Prev` remain bound (vi `g l/g h`, Standard
`Ctrl+Shift+Left/Right`, emacs `Ctrl+Shift+Left/Right`-family rows
where they exist), the *old* bug still reproduces through those
chords. That is accepted: the law's move-case is carried by the
directional keys (J3 US-2 names C-f/C-b, i.e. the preset's
*directional* strokes). The retained settle-then-run for `StackNext`
mid-move is today's behavior and stays — the US-2 journey asserts the
directional strokes, not the legacy ones.

## 6. #875 selection-extend fold (RIDER)

Facts settled on main: bare printables fold shift into the glyph
(`from_event`/`parse` D6, stroke.rs) so a Shift+j press resolves the
token `J` with **shift bit cleared** — the intercept's
`stroke.shift` extension (adapter.rs:1097-1115) can never see it.
Named keys keep the shift bit, which is why Shift+arrow already
works.

Design (the RIDER's "dedicated ActionId + explicit Shift rows"):

1. `NavExtendNext`/`NavExtendPrev` (§2.1) route to
   `ExtendSelectionNext/Prev`; the `FocusDirectional`-shaped
   interpret arm calls the existing
   `view_state.extend_selection_walk(&state, ±1)` on Board (the same
   method the intercept branch calls — one extension implementation,
   two dispatch fronts, the D-2 route-level rule).
2. Preset rows (Board context only):
   - vi: `"J"` / `"K"` (a Shift+j press delivers `J` exactly).
   - emacs: `"Ctrl+Shift+n"` / `"Ctrl+Shift+p"` — explicit rows per
     the RIDER; chorded case law makes the delivered event
     (`ctrl, shift, 'n'`) resolve them.
   - Standard: `"Shift+Down"` / `"Shift+Up"` explicit rows — yes,
     this duplicates the intercept's shift-bit reading for named
     keys. Decision: the explicit rows win (route-level, same front
     as the other presets, and the intercept branch stays as the
     **fallback** for layouts that deliver base-glyph named keys).
     To avoid double dispatch: the intercept branch keeps firing
     first (it returns before `route`), so a resolved row never
     reaches the router; the rows only cover the non-intercept path.
     Stage 2 must assert in a seam test that exactly one dispatch
     fires per press (count drained commands / focus moves).
3. The named-key arrow extension is not weakened: the
   adapter.rs:1097-1115 branch stays byte-identical except — if and
   only if the double-dispatch above needs it — a guard excluding ids
   the presets bind as extend rows. Prefer no guard; measure first.
4. Retire nothing here: the round-8 D-4 arrow journeys stay green.

## 7. Subject-walk removal (R-3) — blast radius

- **view_state.rs `step_focus`** (view_state.rs:1652-1665): delete the
  TaskDetail arm. `detail_item_domain` (view_state.rs:1804) loses its
  last walk caller — check `jump_focus` (1772): the
  `NavFirstItem`/`NavLastItem` TaskDetail arms go too (**R-3 is
  "not at all", so M-</M-> at TaskDetail also stop changing the
  viewed task**). If `detail_item_domain` becomes dead, delete it
  (no dead code).
- **`subject_task` the field stays** — it is the detail's snapshot
  subject read across adapter.rs/models; only the *walk writes*
  die. The stage-2 commit message and census entry must say "the
  subject *walk* is removed", not the field.
- **presets.rs rows removed**: emacs `NavNextItem/NavPrevItem/
  NavFirstItem/NavLastItem` TaskDetail member rows
  (presets.rs:948-955-region), Standard's TaskDetail member
  Down/Up/Ctrl+Home/Ctrl+End rows, vi's — wherever `VIEW_NAV`
  contexts put TaskDetail rows; the retune's conflict gate re-runs.
- **Journeys retired by census supersession** (not weakened, not
  silently deleted): `journeys.rs::us1_s1_emacs_motion_rows_step_the_walks`
  (journeys.rs:21241) asserts the subject changes — replaced by the
  new `us1_s1` TaskDetail journey (focus moves, subject pinned). The
  jump-pair assertions inside it die with the rows. The census
  (docs/ux-surface-census.org `nav.item_walk` rows, lines ~192/326)
  gains a dated SUPERSEDED note: "round-8 keymap-parity D-2
  superseded 2026-11-19 by J3 R-2/R-3 — the detail's item walk is
  retired; nav.focus-* carry the control walk (this plan)".
- **Detail-in-menu route note**: the D-2 fix was route-level (menu
  invocation lands on the same interpret arm). After removal the
  menu's item-walk rows at TaskDetail resolve `NavNextItem` in a
  context that no longer carries it → typed unbound no-op; verify no
  menu row references the detail item walk (grep menu.rs) and file an
  issue if one does.

## 8. Preset retune (R-1) — exact rows

Emacs (EMACS_S1 region), five view contexts × four rows:

- `C-f → NavFocusRight`, `C-b → NavFocusLeft`,
  `C-n → NavFocusDown`, `C-p → NavFocusUp` — **re-points** the
  existing rows (Board `StackNext/Prev` C-f/C-b; Today/StackDetail/
  TaskDetail/Settings `NavNextControl/PrevControl` C-f/C-b; Settings
  C-n/C-p control rows; StackDetail/TaskDetail C-n/C-p item rows).
  The superseded behaviors keep their homes where they exist as
  alternates: Board stack walk keeps `b`/`B`
  (`BoardNext/BoardPrevious`, presets.rs:868-869) — the stack walk
  stays reachable; the control walk keeps Tab (D28b's native
  traversal); Settings' arrows (Standard family) still walk.
- The intra-context conflict gate must stay green; C-f's second
  strokes (`Ctrl+c Ctrl+f` BoardColorEdit) are prefix members, not
  conflicts.

Vi (VI_S1/S2), ViNormal context:

- `j → NavFocusDown`, `k → NavFocusUp` (re-point the NavNextItem/
  NavPrevItem ViNormal rows, presets.rs:1078-1079),
  `h → NavFocusLeft`, `l → NavFocusRight` (free in ViNormal today;
  `g`-prefix rows are prefixes, not conflicts), `J/K → NavExtend
  rows` (§6). `C-f/C-b` keep `ViewScrollDown/Up` (R-1: not
  violations).

Standard (STANDARD_S1 region), five view contexts × four rows:

- `Left/Right/Up/Down → NavFocusLeft/Right/Up/Down`. Up/Down re-point
  the VIEW_NAV Down/Up `NavNextItem/PrevItem` rows
  (presets.rs:121-124 region + Today/StackDetail member rows);
  Left/Right are new. Shift+Down/Up extend rows per §6.
- `Ctrl+F` keeps `PaletteSearch` (R-1: preset conventions that
  conflict are not violations).

Cross-chunk collision space (chunk 8, **unmerged**
`feat/ui-habit-keyboard-parity` — its worktree exists with uncommitted
changes, so its rows are not yet facts):

- Chunk 8's planned habit reorder (`Shift+Up/Down`) and habit
  shortcuts (`Ctrl+Shift+c`/`u` per the dispatcher note) vs this
  chunk's `Shift+Down/Up` **Board-context** extend rows and view
  contexts `NavFocus*`: no overlap in context or chord **provided**
  chunk 9 binds `NavExtend*` only in Board and `NavFocus*` never in
  Global. Coordination rule recorded for stage 2: if chunk 8 lands
  on main mid-chunk, rebase and re-run the conflict gates before the
  final commit; a real collision (e.g. chunk 8 binds Shift+Up/Down in
  Board too) resolves by chunk 9 yielding the Board extend rows'
  *tokens* to chunk 8's reorder only if the operator rules habit
  reorder > selection extend — file it, do not negotiate silently.

## 9. Files touched (stage-2 checklist)

| File | Change |
|---|---|
| `crates/taskboard-ui-slint/src/actions.rs` | 6 new ids, registry entries, describe rows (~+90) |
| `crates/taskboard-ui-slint/src/router.rs` | `Direction`, `FocusDirectional`, `ExtendSelection*`, dispatch + pair-table harness rows (~+45) |
| `crates/taskboard-ui-slint/src/keymap/presets.rs` | retune per §8, remove detail item rows (~+70/−25) |
| `crates/taskboard-ui-slint/src/adapter.rs` | `FocusDirectional`/`ExtendSelection` interpret arms + per-view table (§4), move kept arms (§5), intercept fallback note (~+120/−10) |
| `crates/taskboard-ui-slint/src/view_state.rs` | `cycle_move_stack_column` (+ move-stack slot hop), delete `step_focus` TaskDetail arm + `detail_item_domain` if dead, unit tests (~+90/−45) |
| `crates/taskboard-ui-slint/src/keyscope.rs` | expected: **zero** (the guards are inherited; §3.2 proves it) |
| `crates/taskboard-ui-slint/tests/journeys.rs` | per-view `us1_s1..s3` + `us2_s1` + #875 seam tests; retire `us1_s1_emacs_motion_rows_step_the_walks` (~+280/−140) |
| `docs/ux-surface-census.org` | `nav.item_walk` supersession, `nav.focus-*` rows, D-2 dated supersession, `nav.extend-*` rows (~+30) |
| `crates/taskboard-ui-slint/src/menu.rs` | only if a detail item-walk menu row exists (§7) |

**Diff estimate: ~730 added / ~220 removed across 8-9 files** (±200:
the planner arm and the double-dispatch seam test are the two
unknowns).

## 10. Gates & bookkeeping (stage 2/3)

- Journey tokens embedded: `us1_s1`, `us1_s2`, `us1_s3` (per view
  where meaningful), `us2_s1`; `on_slint_thread` for all rig tests.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate`
  for US-1/US-2 (stage 2); stage 3 re-runs + walkthrough.
- Quality gates via `scripts/run-quality-gates.sh` (never in-subagent
  coverage trio); clippy `-D warnings`; no text assertions (typed
  `matches!` / structural facts only); census symbol anchors per
  #775.
- Stage 3 reviews the law: any view where a direction still mutates
  selection/subject instead of focus is a BLOCKER; retired round-8
  journeys must be retirement-by-supersession.
- Kiosk: desktop-only affordances (J3 platform scope) — kiosk gates
  n/a; the kiosk greyed-menu law must not regress (unchanged here).

## 11. Risks

1. **TaskViewWalkFacts granularity**: the stop ladder is a slint-side
   fact (`ui/walk-facts.slint`, `activation-stop.slint`); the
   adapter drives the control walk generically today. Up/down *along
   the ladder* is what ControlNext already does — the risk is low,
   but if the ladder proves purely linear, left/right on TaskDetail
   degenerates to the same walk (record it in the arm comment; J3
   AC-1 only requires "per geometry table" — a degenerate horizontal
   on a vertical-only surface is the recorded geometry).
2. **Planner in-pane cursor seam** may not exist (§4) — pane-switch
   only + backlog issue is the acceptable landing.
3. **Chunk 8 race** (§8) — rebase + conflict-gate re-run is the
   protocol; silent negotiation forbidden.
4. **Double-dispatch** on Standard Shift+arrows (§6.2) — measure
   before guarding.
5. **`extend_selection_walk` non-Board behavior**: today the
   intercept guards Board; the routed effect must carry the same
   guard (typed no-op elsewhere), or the vi rows leak selection
   semantics into other views via the router.
6. **Today horizontal degenerate** (R-4 trial): control walk first
   cut is accepted-by-ruling; do not "fix" it into inertness.
