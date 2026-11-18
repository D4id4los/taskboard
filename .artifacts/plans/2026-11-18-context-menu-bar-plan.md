# Context-Menu Quick-Bar Regression + Remainder — Implementation Plan (Round-9 Chunk 2, M1)

- **Date**: 2026-11-18
- **Branch**: `fix/ui-context-menu-bar` (worktree `../taskboard-WT-fix-ui-context-menu-bar`, off `origin/main` @ 7aaa3452)
- **Origin**: J5 UX journey plan `.artifacts/plans/2026-11-18-menus-remainder-ux-plan.md`
  (rulings R-2/R-3/R-4 normative), round plan §1 O-9, research
  `.artifacts/reports/2026-11-18-round9-menus-controls-research.md` §1–§3, §5.1.
- **Status**: PLAN (stage-1; no implementation in this commit).
- **Non-goals**: dropdown de-siting (J5 R-5 → M2 chunk), #938 Labels row
  (backlog), priority-picker dropdown (M2).

## 1. Scope

1. **The live regression (J5 R-2)**: the task context menu's quick bar
   (PR #933, confirmed in `origin/main`) does not render on the
   operator's post-#933 build. Root-cause via a structural slint-rig
   test BEFORE touching production code; fix; regression test lands.
2. **Multi-selection bar (J5 R-3)**: the multi branch
   (`adapter.rs` `open_task_context_menu` selection arm, ~:10381)
   passes `Vec::new()` — populate a multi-sensible bar.
3. **`hint-keys` on bar buttons (J5 US-4 AC-1)**: per-preset stroke
   lookup into `IconButton.hint-keys`.
4. **#927 disarm**: reconcile-driven close of a pinned `MenuKind::Object`
   menu whose subject vanished (research §5.1: bar + rows disarm together).
5. **Submenu chevron** in the `ActionMenu` row renderer
   (`action-menu.slint` row renderer) + golden re-baseline.

## 2. Seam inventory (verified by source read, 2026-11-18)

| Seam | Location | Fact |
|---|---|---|
| Bar model builder | `crates/taskboard-ui-slint/src/menu.rs:559` `task_quick_bar(scheduled, tracked_here, tracking_elsewhere)` | 5 entries; C-4 withheld via `visible:false`; singleton Start grey via `enabled` |
| Page composer | `menu.rs:342` `task_context_page` | returns `(bar, entries)`; builds bar from `TaskMenuSubject` facts |
| Slint bar row | `ui/compositions/action-menu.slint:135–153` | `if (root.quick-bar.length > 0): HorizontalLayout` of `IconButton`s; per-entry `visible`/`enabled` |
| Property plumbing | `ui/app-window.slint:1390` | root panel binds `quick-bar: KeyboardState.menu-quick-bar`; fly-out instance deliberately passes none (6721984a) |
| Adapter push | `crates/taskboard-ui-slint/src/adapter.rs:867` `sync_menu_models` → `quick_bar_model` (~:873) | maps `QuickBarIcon` → `IconKind`; sets `label` from registry, `enabled`, `visible` |
| Open paths | `adapter.rs:10328` `open_context_menu` (id variant, `Vec::new()` bar) / `:10410` `open_context_menu_with` (curated) | both set `MenuKind::Object`, call `sync_menu_models` |
| Multi arm | `adapter.rs:10381` | `selection_covers` → `open_context_menu(window, x, y, &TASK_CONTEXT_MULTI, Vec::new())` — **empty bar** |
| Card emission | `ui/widgets/task-card.slint:171–182` → `board-layout.slint:654/1083` → `board-view.slint:229` → `on_board_task_context_requested` | right-button pointer-event; passes `Some(task)` |
| Hint machinery | `menu.rs:138` `row()` → `keymap.first_binding_display(id, chain)`; `IconButton.hint-keys` (`ui/widgets/icon-button.slint:32`) | bar buttons currently pass no `hint-keys` |
| Pin | `menu_kind: MenuKind::Object` set at both open paths (`adapter.rs:10338/10424`); cleared only in `apply_close_menu` (~:5519) | D-1 built-once model; #927's staleness by design |
| Multi row set | `menu.rs:440` `TASK_CONTEXT_MULTI = [TaskComplete, TaskDelete]` | the current multi grammar |

## 3. Root-cause ladder — the regression

Method: repro as a structural slint-rig probe test FIRST
(`tests/action_menu_bar_probe.slint` + `tests/action_menu_bar_probe_tests.rs`,
the established `*_probe.slint` + `*_probe_tests.rs` pattern used by
`task_card_hover_probe`, `dropdown_probe`, …). Each hypothesis names the
test that eliminates it. No production edit until a hypothesis survives.

**H-1 (rank 1) — the multi branch shadows the single-card bar.**
`selection_covers` is true whenever the right-clicked card is in the
selection — which is the *common* case (left-click to focus, then
right-click; `activate_card` outcomes and
`settle_pointer_conflict_keeping_selection(.., true)` keep the
selection). The multi arm then opens via the id variant with
`Vec::new()` → the operator sees a task context menu with **no bar**
(and only Complete/Delete rows). Matches the screenshot symptom
exactly: menu renders, bar absent.
*Eliminated by*: adapter-level journey test driving
`on_board_task_context_requested` twice — (a) fresh card, no selection
→ bar present; (b) card previously selected (left-click then
right-click) → bar present. If (b) shows the multi branch firing,
H-1 confirmed; the fix is the multi bar (§4) plus a correct
single-vs-multi discriminator if the operator intends right-click on a
selected card to stay the curated menu (operator ruling sought at
stage-2 review if the two grammars must be distinguishable — e.g. a
modifier or multi-row-count caption; default per J5 R-3 is: bar in
both, so H-1's fix IS the multi bar).

**H-2 (rank 2) — slint-side mount/render defect**: the
`if (root.quick-bar.length > 0)` block never mounts (model
`length`-guard, `QuickBarEntryModel` field defaults, HorizontalLayout
squeeze to zero height under `available-height` clamping, or per-entry
`visible` default false).
*Eliminated by*: the probe mounts `ActionMenu` directly with a
hand-populated `quick-bar` model (5 entries, mixed visible/enabled) and
asserts structurally: bar element count == shown entries, each
`IconButton` icon/kind, enabled, and geometric height > 0. Green probe
eliminates everything slint-side; red probe localizes the defect to the
composition/adapter push.

**H-3 (rank 3) — adapter model wiring**: `quick_bar_model` push drops
rows (field mapping, `visible` not carried, registry label lookup empty
→ rows still render but wrong) or a post-open `sync`/`finish_mutation`
path re-pushes an empty model (only setter is `sync_menu_models`
~:867; `apply_close_menu` clears on close — audit for an
unintended close/reopen in the open seam's `apply_outcomes`).
*Eliminated by*: adapter-level test asserting
`KeyboardState.menu-quick-bar` length == shown entries immediately
after the open seam fires, and again after one full `finish_mutation`
cycle. Green + H-2 green ⇒ defect must be in the seam call chain
(H-4).

**H-4 (rank 4) — the open seam never reaches the curated path**: the
card emission chain (`task-card.slint:171` → `board-layout` →
`board-view` → `on_board_task_context_requested`) mis-fires (wrong
button gate, coordinates, or the today/stack variant
`adapter.rs:10480/10675` passing `None` subject with a different
composition). *Eliminated by*: journey test on the real seam with a
settled board; if the menu opens but `open_task_context_menu` receives
unexpected args, instrument via the existing seam inventory test.

**H-5 (rank 5, eliminated by ruling)**: stale pre-#933 build —
operator-confirmed post-#933 (J5 R-2). Not re-tested.

**Resolution record**: CONFIRMED — H-1, with H-2/H-3 eliminated by
baseline evidence and the repro verified red pre-fix (stage-2,
2026-11-18):

- **Baseline**: the pre-existing
  `journeys::us1_s1_schedule_today_commits_the_exact_mask` (fresh card,
  no selection) PASSES on `main` @ 36caae97 — the slint-side mount
  (H-2) and the adapter push (H-3) render the bar correctly whenever the
  single-card path fires. H-5 was ruling-eliminated (R-2).
- **Repro**: `us1_s1_the_selection_covered_card_menu_carries_the_quick_bar`
  (ctrl-click card → right-click it) FAILS on the pre-fix tree
  (multi arm passes `Vec::new()` → empty bar; verified by temporarily
  reverting the fix, FAIL reproduced, restored) and PASSES with it.
- **Mechanism**: the selection branch's `selection_covers` guard
  (adapter.rs, `open_task_context_menu`) is the only barless path a
  card right-click can take — and a member right-click is the common
  selection-covered posture (D-11 keeps the selection across the
  context funnel).
- **Fix**: the multi bar (§5) IS the H-1 fix, per J5 R-3: the multi arm
  composes `menu::multi_quick_bar` over the selection's aggregate
  facts; the schedule trio routes per member through the selection-verb
  intercept. Deviation from the plan's §5 signature: `multi_quick_bar`
  takes `(count, any_scheduled, tracked_members, tracking_elsewhere)` —
  the recorded `(count, uniformly_tracked_here, tracking_elsewhere)`
  cannot distinguish uniform-untracked from mixed, and the C-4
  unschedule rule needs the any-scheduled fact.
- Golden re-baseline: no menu goldens exist (`tests/goldens/` carries
  board/today/task-view/review only); the chevron renders only inside
  open menus, so no golden moves — §7's re-baseline is vacuous. The
  chevron's proof is the structural probe (`us3_s1`).
- #927's disarm predicate is soft-delete aware (`deleted` bit), not
  map-presence: the engine keeps deleted tasks in `state.tasks` (the
  FakeEngine settle and the first red run both caught this).
- Journeys appended to `tests/journeys.rs` (the harness is
  journeys-local — the plan's separate `menus_remainder_journeys.rs`
  would have duplicated the ~300-line Harness).

## 4. The bar-presence test design (TDD)

- **Unit (menu.rs)**: existing `task_quick_bar` verdict tests stand; add
  `multi_quick_bar` tests (§5) — shown/withheld per aggregate fact.
- **Structural probe** (`tests/action_menu_bar_probe.slint` +
  `_probe_tests.rs`): mount `ActionMenu`; drive `quick-bar` with a
  populated model; assert child count, icons, enabled, visible, and
  geometry (height > 0, above the Flickable). This is the AC-1 GUI
  proof for J5 US-1 and the H-2 eliminator.
- **Adapter journeys** (`tests/journeys.rs` or a new
  `menus_remainder_journeys.rs`): `us1_s1` single-card open → bar
  model populated + `KeyboardState.menu-quick-bar` carried;
  `us2_s1` multi-selection open → multi bar populated (§5);
  `us4_s2` reconcile disarm (§6).
- **Determinism**: no sleeps; poll shell state after seam dispatch;
  the open path is synchronous on the slint thread's adapter, so
  assertions run post-callback directly.
- **No text assertions**: assert entry counts, `ActionId`s, icon
  kinds, enabled/visible booleans — never labels/hints text.

## 5. Multi-selection bar (J5 R-3 — multi-sensible)

New `menu.rs` builder beside `task_quick_bar`:

```rust
pub fn multi_quick_bar(
    count: usize,
    uniformly_tracked_here: bool,   // every member tracked, or none
    tracking_elsewhere: bool,       // the singleton Start grey
) -> Vec<QuickBarEntry>
```

Action set (design freedom within multi-sensibility, R-3; reuses the
five existing `QuickBarIcon` glyphs — zero new assets):

- **Schedule today** (Calendar) — applies to every member; sparse
  `UpdateTask` masks are selection-addressable (research §2).
- **Schedule tomorrow** (CalendarPlus) — same.
- **Unschedule** (CalendarMinus) — shown only when at least one member
  is scheduled (the C-4 per-bar-row rule over aggregate facts).
- **Track start/stop twin** (Play/Stop) — shown only when the
  selection is uniformly tracked / untracked respectively; Start
  greyed while another task tracks (singleton precondition);
  **withheld entirely** on mixed selections (the twin is not
  multi-sensible there). `count == 1` degenerates to the single-card
  rule.
- **No Complete/Delete icons** — those stay menu ROWS
  (`TASK_CONTEXT_MULTI` unchanged); the bar is quick-verbs only
  (Complete has no icon glyph in the closed vocabulary; adding one is
  a vocabulary-growth decision out of scope, filed as backlog if the
  walkthrough wants it).

Adapter: the selection arm computes the aggregate facts from
`view_state.selection()` members via `state.load_full()` (same read
pattern as the single path) and passes
`menu::multi_quick_bar(..)` instead of `Vec::new()`. Bar buttons
address the selection through the existing multi-row routes. Built
once at open → inherits the D-1 pin and the §6 disarm unchanged.

## 6. `hint-keys` wiring + #927 disarm

**Hint-keys (J5 US-4 AC-1).** `QuickBarEntryModel` gains
`hint-keys: [string]` (or a single pre-formatted stroke string —
match `IconButton.hint-keys`'s `[string]` type). Adapter
`quick_bar_model` looks the stroke up the same way `row()` does:
`keymap.first_binding_display(entry.action, &self.menu_chain)`
split into the stroke tokens the tooltip expects. Empty when unbound
(the bar verbs' pointer-first posture; no invented hints). Fold test:
every shown bar entry with a preset binding carries non-empty keys;
unbound entries carry none.

**#927 disarm (J5 US-4 AC-2).** The MenuKind::Object pin freezes the
model; a subject deleted mid-open leaves dead rows. Design: on every
state reconciliation that reaches the menu path (the existing
`sync`/`finish_mutation` cycle — no new actor, no new channel), when
`menu_kind == MenuKind::Object` and the pinned subject id no longer
resolves in the current tables, route the same close path as
`apply_effect(UiEffect::CloseMenu)` (bar + rows disarm together,
research §5.1). The subject id must therefore be recorded at open
time — a new `menu_subject: Option<TaskId>` field beside
`menu_kind` (the multi selection records nothing: the multi menu
disarms when its members all vanish, same reconcile predicate over
`selection()`). Journey test: open the card menu, dispatch the
delete, reconcile → menu closed; assert terminal state
(`KeyboardState.menu-open == false`), not transient intermediates.

## 7. Chevron change + golden re-baseline (J5 US-3)

- `action-menu.slint` row renderer (the `RoleText` + `KbdHint` row,
  ~:256–274 and the compact back-row symmetry): after `KbdHint`, add
  `if entry.submenu: Icon { kind: IconKind.chevron-right; }` —
  `chevron-right.svg` is already vendored; zero new assets; one
  component, all menus inherit (main root page, task families,
  replace-mode, fly-out).
- A11y: `FocusScope` keeps `AccessibleRole.button`; description
  already names the subgroup (round-1 D7). Slint 1.18 has no
  `has-submenu` accessible state — recorded limitation, census line.
- Optional (walkthrough-graded): compact "Back" row gains
  `chevron-left`.
- **Golden re-baseline**: `tests/golden_visuals.rs` menu goldens
  change (new trailing glyph) — re-baseline with the review note
  "chevron affordance, J5 US-3"; no other golden should move (if one
  does, the change leaked — investigate, do not blanket re-baseline).
- Fold test: `entry.submenu == true` ⇒ chevron present (structural
  probe assert), `false` ⇒ absent.

## 8. Implementation order (TDD)

1. Red: bar-presence structural probe (§4) — expect the H-2 verdict.
2. Red: adapter journey `us1_s1` — expect the H-1 verdict
   (selection-covers shadow).
3. Fix per confirmed hypothesis (production edit, minimal).
4. Red: `multi_quick_bar` unit tests + `us2_s1` journey → implement
   §5.
5. Red: hint-keys fold test → implement §6 wiring.
6. Red: `us4_s2` disarm journey → implement §6 reconcile.
7. Red: chevron probe assert → implement §7 + golden re-baseline.
8. Quality gates via `scripts/run-quality-gates.sh` (worktree
   discipline); hand off at PR-green.

## 9. Risks

- **H-1's fix is also a product decision**: right-click on a
  selected card currently yields the multi grammar; if the operator
  expected the curated single menu there, the discriminator needs a
  ruling (default per R-3: bar rides both). Flagged for stage-2/3.
- **Golden churn**: menu goldens move by design (§7); anything beyond
  menus moving signals leakage.
- **Disarm timing**: the reconcile predicate must run on the same
  pass that removes the subject; running it a frame late flashes a
  dead menu. Test asserts terminal state only.
- **`available-height` clamping** near the window bottom can squeeze
  the bar to zero height (H-2 variant) — the probe asserts geometry,
  not just mount, to catch it.
- Slint 1.18.1 probe limits: structural asserts via element
  properties; no screenshot-based bar detection needed.

## 10. Definition of done

- [ ] Root-cause ladder §3 resolved with the confirmed hypothesis recorded
- [ ] `us1_s1`, `us2_s1`, `us3_s1`, `us4_s2` journeys + probe + fold tests green
- [ ] `scripts/ux-coverage.sh 2026-11-18-menus-remainder-ux-plan.md --gate` exits 0
- [ ] Goldens re-baselined with rationale; quality gates green via the flock wrapper
- [ ] J5 walkthrough findings homed; backlog issues filed (numbers in the stage report)
