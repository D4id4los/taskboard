# [Plan] Round 5 Chunk 4 follow-up — the Focus-Coverage Invariant's Remaining Views (#665)

- **Date**: 2026-11-06
- **Status**: PROPOSED — stage-1 plan for issue #665 (p1, v0.2).
- **Branch**: `test/ui-slint-focus-coverage-views`, worktree
  `../taskboard-WT-test-ui-slint-focus-coverage-views`
  (`scripts/worktree-doctor.sh` exit 0, 168 G free against the 25 G floor).
  **Stacked on PR #664** (`feat/ui-slint-focus-coverage` @ `2968a53b`):
  #664 is still OPEN, so `tests/focus_coverage.rs` does not exist on
  `origin/main` yet. The worktree was created off the fetched
  `origin/main` as instructed, then reset to `origin/feat/ui-slint-focus-coverage`
  (upstream set to it). When #664 rebase-merges, run the AGENTS §8 dance:
  `git fetch origin --prune && git rebase --onto origin/main 2968a53b test/ui-slint-focus-coverage-views`.
- **Origin (normative)**: issue #665; round plan
  `2026-11-05-gui-review-round5-plan.md` D5 + §4.4 (the invariant is
  **per view, both halves**); dispatch plan
  `2026-11-05-gui-review-round5-implementation-dispatch-plan.md` §1/§5;
  chunk-4 plan `2026-11-06-ui-r5-focus-coverage-plan.md` §2.1–§2.4.
- **#665's ruling, applied here**: the chunk-4 §2.3 table is a list of
  *reasons*, not an exemption list. A "sibling-covered" row is a claim
  the scan checks. A "pointer-only" row is a bug to fix in the widget
  by adding a stop. `EXEMPT` holds only (i) the recorded D-T16
  chrome-chip deviation and (ii) modal dismissal backdrops.

---

## 0. What landed in #664 (the base this plan extends)

`crates/taskboard-ui-slint/tests/focus_coverage.rs` (458 lines):

- `mount(state, settings)` builds the window at a fixed 1000×6000 size.
  There are two fixtures, `settings_fixture` and `task_view_fixture`.
- `scan(root, exempt)` collects every visible item with a non-zero area
  under `root`. A **stop** is an enabled `FocusScope` with
  `focus-on-tab-navigation`, or an enabled `TextInput`. An element is
  **interactive** if it is an enabled `TouchArea` with a `clicked`,
  `double_clicked` or `pointer_event` handler. A touch is covered in two
  cases: an *enclosing* stop where the touch fills at least half the
  stop's area, or a *sibling-of-ancestor* stop whose rect contains the
  touch's centre. Exemptions are matched on the qualified
  `TouchArea:<Component>::<id>` string.
- `walk(mounted, backwards, budget)` presses Tab (or Shift+Tab) and
  reads `WindowInner::focus_item` directly until focus wraps back to
  the start. This retires the facts-global mechanism of chunk-4 §1.
- `assert_walk_covers_the_view(mounted, view_type)` checks four things:
  mount-focus parity (the view root holds focus), that the walk visits
  every in-view stop (a floor), reading order, and that Shift+Tab
  mirrors Tab.
- `EXEMPT` = `Toolbar::inbox-chip`, `StatusBar::stop-touch`.
- Self-guards: `CoverageNegativePanel` and `CoveragePositivePanel`.
- `tests/support/driver.rs` adds `release_stroke`, `focused_label` and
  `focused_rect`.

Two facts carry over into this plan. First, the scan runs over the
**whole window** (`root_item`), so chrome (toolbar, board bar, status
bar) is scanned on every fixture. Chrome findings therefore surface in
whichever fixture first makes them visible. Second, the walk only
asserts stops *inside* the view root. Chrome stops it visits are
tolerated but not checked.

---

## 1. Inventory: Tab chain vs item walk per surface

| Surface | Mount focus | Tab chain inside the view | Item walk (arrow / C-n/C-p, focus stays on the host) |
|---|---|---|---|
| Board (`BoardView` → `board-layout.slint`) | root `init => self.focus()` | board-bar `tab` segments, `add-task-focus` (per column), `add-col-focus`, compact `segment` switcher, compact `IconButton` (new stack), `add-task-focus-compact` | columns (`StackNext/StackPrev`), cards in the selected column (`NavNextItem/NavPrevItem/NavFirstItem/NavLastItem`); read via `get_board_model().selected_stack` + the column's selected task |
| Today (`today-view.slint`) | root `init => self.focus()` | habits `IconButton`s, `tracked-today` `row-focus`, timers-section stops | today rows across sections (`NavNextItem/NavPrevItem`); read via the today model's selected section/row |
| Stack detail (`stack-view.slint`) | root `init` | the create `TextField` when `create-open` | `TaskCard` rows (`NavNextItem`) |
| Review (`review-view.slint`) | root `init`, `focus-on-tab-navigation: true` | none (ledger is text; no TouchAreas in the view) | none on this base (`feat/review-walk` has not landed) |
| Plan-next-day | root `init` | `box-activate` per box, `pool-hit` per pool row; `earlier-hit/later-hit/remove-hit` have `focus-on-tab-navigation: false` | none |
| Search palette (modal) | `query-field` (TextField) | the query field only | result rows (`palette_selected_index`, arrows held by the field) |
| Focus overlay (modal) | its TextField | the TextField | switcher rows (arrows, `selected-index`) |
| Timer setter (modal) | `setter-scope` `init => self.focus()` | the declared walk: fields, segments, `commit-focus` | none |
| Date picker (modal) | `grid-scope` `init => self.focus()` | the two month `IconButton`s, the `TimeField` when `show-time` | day cells (raw arrows on `grid-scope`) |
| Action menu (modal) | its `row-focus` / `back-row` scopes | `back-row`, `row-focus` per entry | `MenuNext/MenuPrevious` (`menu_selected_index`) |
| Dropdown / quick-list flyouts | the owning field (`dropdown-flyout` is deliberately not a FocusScope); `quick-list` `flyout-scope` | none | rows via the owner's ladder (arrows + Return) |
| Track entries (task-view modal) | `panel-scope` / `form-scope` | `row-focus` per row, `add-focus`, `IconButton`s | — |

### Decision D-V1: how the walk half is defined for item-walk views

On a host where a delegate (row, card, column) can be reached by the
item walk, **two pins** apply:

1. **The Tab walk covers only the view root and the real tab stops.**
   This is the #664 `assert_walk_covers_the_view`, unchanged. Item-walk
   delegates are not Tab stops, and turning every card into a stop is
   ruled out by the round-5 F13 non-goal ("no Today item-walk semantics
   change"). It would also make Tab descend through hundreds of cards
   on a large board.
2. **The item walk is the order pin for the delegates.** A new helper,
   `assert_item_walk_covers(host, next, prev, read_selection,
   delegates)`, starts at the first delegate (`NavFirstItem` where the
   host binds it, otherwise from mount selection). It presses `next`
   until the selection stops advancing or repeats, and asserts:
   - every instantiated delegate is visited exactly once, in model
     (reading) order;
   - `prev` from the last delegate visits the same sequence mirrored;
   - focus never leaves the host, i.e. `focused()` equals the host
     root after every press.

   The terminal behaviour (clamp or wrap) is **not** asserted. The
   keymap contract owns it, and AGENTS §5 forbids pinning today's
   behaviour.

To keep the scan and the walk consistent, add a third coverage arm to
`scan`: **item-walk coverage**. A touch counts as covered when its
ancestor chain passes through a delegate of a registered
**item-walk host**. A host is a typed registry entry: a host root type
and a delegate element id, for example
`("TodayView", "today-row")`, `("BoardLayout", <column delegate>)`,
`("BoardLayout", <card delegate>)`, `("StackView", "detail-task")`,
`("SearchPalette", <row ListRow>)`, `("FocusOverlay", <row ListRow>)`,
`("DatePicker", <cell>)`, `("DropdownFlyout", "row-touch" parent)`,
`("QuickListFlyout", <row>)`. Each registration has a matching
`assert_item_walk_covers` test, and a test-side assertion fails if a
registered host has no item-walk test.

This is not an exemption. It names the *keyboard mechanism* that
reaches the control, and the walk test proves that mechanism works. The
registry lists hosts, not individual touches, so a new pointer-only
widget added outside a delegate still fails.

**Scope of the arm**: it covers verbs that act on the *selected
delegate*: card select/activate/done/context, column header select,
body select, and the column divider (its keyboard path is
`StackWider/StackNarrower` on the selected column, which the in-code
comment records). It does **not** cover a standalone button that
happens to sit inside a delegate but has its own verb, such as
`add-task-hit`. That one keeps its sibling stop, and the scan's earlier
arms already cover it before the item-walk arm is consulted.

---

## 2. Fixtures (state + navigation + window size)

All fixtures reuse `mount`. #664 hard-codes the size, so `mount` gains
a `size: (u32, u32)` parameter: settings and task keep `(1000, 6000)`.
Each fixture asserts `get_current_view()` as #664 does.

| Fixture | State | Navigation | Window | Why this size |
|---|---|---|---|---|
| `board_fixture` | `golden_state_with_stacks(3)` + golden cards | `NavFocusBoard` | `1800×2000` | desktop breakpoint, so every column is laid out side by side with no horizontal Flickable clip. Tall enough that each column's card Flickable shows all cards. |
| `board_compact_fixture` | same | `NavFocusBoard` | `480×2000` (below the `compact` breakpoint; confirm the px threshold in `responsive.slint` at implementation time) | the compact switcher (`segment`, `segment-hover`), `add-task-focus-compact`, and the compact `IconButton` only exist here |
| `board_empty_fixture` | `empty_board_state()` | `NavFocusBoard` | `1000×1200` | the two `CtaButton`s plus the stack-create field |
| `today_fixture` | `today_overdue_state()` + one habit row + one tracked-today entry (`add_completed_task`/`add_open_entry`) + one alarm (`add_alarm`, so the timers section and the **timers chip** mount) | `ViewToday` | `1000×6000` | settings precedent; the page Flickable then shows everything. The rows sit inside `row-viewport-clip` (§6 risk R1). |
| `stack_fixture` | `golden_state()` | `NavFocusBoard`, `NavShowTask`-equivalent `ActionId::StackDetail` path as in journeys | `1000×4000` | — |
| `review_fixture` | `golden_state()` + completed tasks | `ActionId::Review` | `1000×4000` | — |
| `plan_next_day_fixture` | `golden_state()` + two placed boxes + two pool rows (the journeys' plan-next-day grammar) | `ViewPlanNextDay` | `1000×4000` | — |
| modal fixtures (§4) | base fixture of the hosting view | open the modal by its action | the base size | — |

Every fixture ends with `advance_time(0)` so conditional subtrees
instantiate. Fixture helpers stay in `focus_coverage.rs` (D-V6).

---

## 3. Per view: expected red, fix, walk

TDD: commit 1 adds every scan and walk test, all red where predicted.
Each later commit fixes one widget and turns its rows green. The
predicted reds below come from the #664 scanner rules applied to the
markup on `2968a53b`. Stage 2 must confirm them and record each
divergence in its report: an unpredicted red is a finding, and an
unpredicted green means the prediction was wrong.

### 3.1 Board (`board_fixture`, `board_compact_fixture`, `board_empty_fixture`)

**Expected red (scan):**

| Touch | Why reported | Fix |
|---|---|---|
| `TaskCard::touch` (every card) | no stop: the card is a `list-item` with no FocusScope | item-walk arm (D-V1). No widget change. |
| `TaskCard::done-hit` | same | item-walk arm (`task.done` on the selection) |
| `BoardLayout::header-hit` (per column) | no stop (AT action only) | item-walk arm (column select = `StackNext/Prev`) |
| `BoardLayout::body-hit` (per column) | enclosing scope test fails; no sibling stop | item-walk arm (column select) |
| `BoardLayout::divider` (`pointer-event` only) | `is_interactive` counts `pointer_event` | item-walk arm (`StackWider/StackNarrower` on the selected column; the in-code deviation comment is the reason) |
| `CtaButton::cta-hit` ×2 (empty board) | **pointer-only**, AT action only | **Widget fix**: the IconButton grammar. Add a full-size `focus := FocusScope` sibling with `focus-on-tab-navigation: true`, the a11y trio, `key-pressed` (bare Return/Space with the modifier guard from `timer-setter` `commit-focus`) → `root.clicked()`, and a ring `border-width: focus.has-focus ? Theme.focus-ring-width : 0`. |
| `BoardBar::segment-hover`, compact `segment-hover` | expected **green** (sibling `tab`/`segment` FocusScope) | — |
| `add-task-hit`, `add-col-hit`, `add-task-hit-compact` | expected **green** (sibling scopes) | — |
| `Toolbar::inbox-chip` | exempt | — |

**Walk:** (a) Tab parity from the board root. The in-view stops are
the board-bar tabs, `add-task-focus` ×N and `add-col-focus` on desktop;
the switcher segments, the new-stack IconButton and
`add-task-focus-compact` on compact; the two CtaButtons and the create
field on the empty board. Assert reading order, wrap and the
Shift+Tab mirror. (b) Item walk: columns via `StackNext` (all stacks,
left to right), then cards of each column via `NavNextItem` (all
cards, top to bottom).

**Risk:** the stack header `clip: true` (`board-layout.slint:370`) is a
non-Flickable clip, but it hosts no stop, only `header-hit`, which is
item-walk covered. Card Flickables are Flickable clips, which tab
traversal tolerates.

### 3.2 Today (`today_fixture`)

**Expected red (scan):**

| Touch | Fix |
|---|---|
| `TaskCard::touch`, `TaskCard::done-hit` under `today-row` | item-walk arm |
| **timers chip**: `StatusBar` anonymous `if (root.timers-chip-visible): TouchArea` | It belongs to the recorded D-T16 deviation (the comment at `status-bar.slint:269` covers the chip row). **Markup: name it `timers-chip := TouchArea`** so the exemption has an address; add `TouchArea:StatusBar::timers-chip` to `EXEMPT` with the D-T16 reason. This is the only new chrome exemption, and it is in the class #665 allows. |
| habits `IconButton`s, `tracked-today` `row-touch` (sibling `row-focus`) | expected **green** |
| `tracked-today::total-touch` | hover-only, so not interactive; expected **green** |
| status-bar banner `row-scope`/`touch` + IconButtons, if an alarm is delivered | expected **green** |

**Walk:** (a) Tab from the root over the habits buttons, tracked rows
and timer-section stops, in reading order. (b) Item walk over all rows
in all sections in section order (`NavNextItem`).

**Risk R1:** `row-viewport-clip` (`today-view.slint:235`, `clip: true`
outside a Flickable) clips the rows. It hosts no tab stop, so the
traversal filter loses nothing. The item walk does not use tab
traversal. The scan's `is_visible` check runs inside the clip, so at
6000 px height every row is in the viewport. **Assert that the
fixture's row count equals the visible `today-row` count before
scanning**, so a too-small window cannot hide a row and give a false
green.

### 3.3 Stack detail (`stack_fixture`)

Expected red: `TaskCard::touch/done-hit` under `detail-task`. Fix:
item-walk arm (host `StackView`, delegate `detail-task`). Walk: (a)
root plus the create field when it is open. Add a second leg with
`StackCreate` dispatched so the conditional `TextField` mounts. (b)
`NavNextItem` over all cards.

### 3.4 Review (`review_fixture`)

Expected: **green scan** (no TouchArea in the view; chrome as on other
views). Walk: (a) the root is the only in-view stop, so the walk wraps
over chrome. Assert mount parity and that the root is visited. No item
walk on this base. If `feat/review-walk` lands first, register its host
in the same PR (§7 risk R6).

### 3.5 Plan-next-day (`plan_next_day_fixture`)

**Expected red (scan):**

| Touch | Why | Fix |
|---|---|---|
| `box-earlier-touch`, `box-later-touch`, `box-remove-touch` (per box) | the sibling FocusScopes have `focus-on-tab-navigation: false`, so `is_stop` rejects them. The D-6 comment calls them "pointer-grade row verbs". | **Widget fix**: set `focus-on-tab-navigation: true` on `earlier-hit`, `later-hit` and `remove-hit`. They already carry `key-pressed` Return/Space plus the a11y default. Add the focus ring (`border-width: self.has-focus ? …`) so a focused verb is visible. Update the D-6 comment: "no preset bindings" still holds, but the verbs are now Tab stops. This overrides D-6's tab omission per #665; the `PlanBoxEarlier/Later/RemoveBox` actions stay as they are. |
| `box-touch`, `pool-touch` | expected **green**: the enclosing `box-activate`/`pool-hit` scope is the same size as the card (the ≥ half rule) | — |

**Walk:** (a) Tab order per box is `box-activate → earlier → later →
remove`, then the next box, then the pool rows. The reading-order
assertion covers the same-line, left-to-right rule. Wrap and mirror
also apply.

### 3.6 Task detail

#664 left the walk to `journeys.rs` (the r4 inventory). That stays: one
behaviour, one test. No change here.

---

## 4. Modal surfaces

### Decision D-V2: scope of a modal scan

Mount the base view and open the modal with its action. `scan` runs
over the **modal's subtree** (`view_root(handle, "<ModalType>")`) and,
separately, over the **window**, with a backdrop exemption. Assert
that **the backdrop is the only exempt touch the window scan matched**.
`scan` gains a return field (or a sibling fn `exempt_hits`) that
reports which exemptions matched, so the assertion is "matched set ==
{this modal's backdrop}".

The backdrops are currently anonymous `if (...): TouchArea {` blocks
in `app-window.slint`. **Markup: give each one an id**:
`date-picker-backdrop`, `timer-setter-backdrop`,
`track-entries-backdrop`, `dropdown-backdrop`, `time-field-backdrop`,
`menu-backdrop`, `palette-backdrop`, `capture-backdrop`,
`focus-overlay-backdrop` (lines 566, 619, 670, 756, 819, 911, 1013,
1026, 1075 on the base). Add one `EXEMPT` entry per backdrop with the
reason "modal dismissal surface; a walk must not tab into a dismissal
backstop". Adding ids changes no behaviour and no golden.

Walk for a modal: (a) mount-focus parity means focus is inside the
modal subtree after opening. Then Tab wrap and the mirror run over the
modal's stops. Each Tab must keep focus inside the modal subtree, or
inside the declared modal walk where the modal has its own Tab handler
(timer setter, track entries). Any focus outside the subtree **fails**:
that is the focus-trap contract. (b) the item walk where §1 lists one.

| Modal | Open via | Expected red | Fix |
|---|---|---|---|
| Search palette | `PaletteSearch` (golden state; type a query so rows exist) | `ListRow` row `TouchArea` (anonymous; no stop) | item-walk arm (host `SearchPalette`, delegate the row `ListRow`). **Markup: name it `row-touch`.** Walk: field-only Tab wrap; item walk over `palette_selected_index`. |
| Focus overlay | `FocusModeToggle` with a tracked task | switcher row TouchArea (anonymous) → item-walk arm (name it `row-touch`). **`FocusOverlay::touch` (Stop session) is pointer-only** | **Widget fix**: rename the touch to `stop-touch` and give `stop-button` the IconButton grammar (FocusScope sibling, Return/Space → `root.stop-clicked()`, ring bound to focus; today's ring is a constant border, so bind it to focus and keep the constant as the unfocused colour). Walk: field → Stop session → wrap. |
| Timer setter | `TimerCreate` | expected **green** (`commit-focus` + `commit-hit` siblings; segments and fields are stops) | — |
| Date picker | task view + `DatePicker` open (the journeys' path) | day cell `touch` per cell (the `grid-scope` encloses it, but a cell is much smaller than half the grid) | item-walk arm (host `DatePicker`, delegate the cell `Rectangle`; the keyboard is `grid-scope`'s raw arrows). Item walk: Right ×N visits each in-month day in order. |
| Action menu | `AppMenu` | expected **green** (`row-focus`/`back-row` siblings) | — |
| Dropdown flyout | settings fixture, open the board-picker `Dropdown` | `row-touch` per row | item-walk arm (host `DropdownFlyout`; the owner field's arrow ladder) |
| Quick-list flyout | task view, open a `TimeField` | `pick` per row | item-walk arm (host `QuickListFlyout`, `flyout-scope`) |
| Track entries | task view, `TrackEntries` | expected **green** (`row-focus`, `add-focus`, IconButtons) | — |
| Capture overlay | its open action | re-derive at implementation time (not audited here, §7 R5) | per the rule: a stop, never an exemption |

For the item-walk modals, the selection reader goes through the
existing generated getters: `get_palette_selected_index`,
`KeyboardState.menu-selected-index`, the date-picker cursor property,
and the flyout `DropdownFacts`/`TimeFieldFacts` highlighted index.

---

## 5. Test binary shape

### Decision D-V6: helpers stay in `focus_coverage.rs`

No journey needs them: journeys pin behaviour, and this binary pins
coverage. Move them to `tests/support/focus.rs` only if a second binary
imports them.

One `#[test]` pair per surface:
`<surface>_has_no_pointer_only_controls` and
`<surface>_walk_reaches_every_stop_in_reading_order`, plus
`<surface>_item_walk_visits_every_delegate` for each item-walk host.
That gives:

- board ×3 fixtures (scan + Tab), plus 2 item walks (columns, cards)
- today: scan + Tab + item walk
- stack: scan, Tab, Tab-with-create, item walk
- review: scan + Tab
- plan-next-day: scan + Tab
- palette, focus overlay, timer setter, date picker, action menu,
  dropdown flyout, quick-list flyout, track entries, capture overlay:
  scan + backdrop assertion + walk, plus item walk where §4 lists one

New shared helpers:

- `mount(state, settings, size)`
- `ItemWalkHost` registry and `assert_item_walk_covers`
- `exempt_hits`
- `assert_modal_walk_stays_inside(root)`
- `assert_visible_count(root, delegate_id, expected)` (the R1 guard)

### Decision D-V7: self-guards gain two panels

1. `ItemWalkNegativePanel`: a touch inside an *unregistered* repeated
   delegate must be reported. This proves the item-walk arm checks
   host membership and does not just accept "in a repeater".
2. A registry-completeness test: every `ItemWalkHost` entry has a
   matching item-walk test. Implement it as a `const` array the tests
   index into, with an `assert_eq!` over the test-name list.

### Decision D-V8: why the item-walk arm is not an exemption

An exemption names a touch that is unreachable by design. The
item-walk arm names a touch that *is* reachable, through a mechanism
the binary proves in its own test. `EXEMPT` keeps D-T16 (inbox-chip,
stop-touch, timers-chip) and the nine backdrops. Nothing else goes in.

---

## 6. TDD sequence and commits

**Decision D-V9**: the red runs locally first. Stage 2 writes all the
tests, runs them, and records the observed red inventory against §3/§4
in its report. Each landed commit then carries one widget fix *with*
the tests it turns green, so every commit on `main` stands alone
(AGENTS §8). No red commit lands.

1. `test(ui-slint): the focus-coverage invariant's remaining views` —
   adds the `mount(size)` change, fixtures, the item-walk arm, the
   self-guards, and the tests that are already green on the base:
   review, timer setter, action menu, track entries, and the
   item-walk tests.
2. `fix(ui-slint): CtaButton becomes a walk stop` (+ the board scan and
   walk tests)
3. `fix(ui-slint): the plan-next-day row verbs are tab stops`
4. `fix(ui-slint): the focus overlay's stop button becomes a walk stop`
5. `refactor(ui-slint): name the dismissal backdrops and the timers chip`
   — ids, the matching `EXEMPT` entries, plus the today and modal
   tests. The palette `row-touch` id rides this commit. Commit order
   2–5 is free; each depends only on 1.

Quality gates: the full AGENTS §10 block via
`scripts/run-quality-gates.sh`. `journeys.rs` must stay green; the
markup renames touch no labels, and the CtaButton, plan-verb and
overlay changes keep their `accessible-action-default` and
click-by-label address. Goldens: the board-empty CTA ring and the
plan verb rings render only when focused, so no pinned golden changes.
If one does, stage 2 re-blesses it with a reason.

---

## 7. Risks

- **R1 — non-Flickable `clip: true` hides delegates**:
  `today-view.slint:235` (`row-viewport-clip`), `review-view.slint:243`
  (`ledger-clip`), `board-layout.slint:370` (stack header),
  `text-field.slint:126`, `multi-line-editor.slint:114`,
  `dropdown-flyout.slint:37`, `quick-list-flyout.slint:39`,
  `stat-bars.slint:41,48`. None of them hosts a Tab stop. The risk is
  the **scan's** visibility filter silently dropping clipped touches
  and producing a false green. Mitigation: the `assert_visible_count`
  guard per item-walk host, plus the generous window sizes.
- **R2 — Flickable-clipped stops**: tab traversal accepts them
  (`is_visible_or_clipped_by_flickable`), but `visible_items` uses
  `is_visible`. A stop scrolled out of the viewport could be missing
  from the scan's stop set while the walk still visits it. The floor
  assertion stays sound, but a touch whose covering stop is clipped
  would be reported as a finding. Mitigation: window sizes that lay
  every surface out unscrolled. If a fixture cannot achieve that,
  stage 2 records it rather than shrinking the assertion.
- **R3 — conditionally mounted subtrees skip init focus (probe P5)**:
  modals that mount under an already-focused window may not take
  focus via `init`. The modal mount-parity assertion will find this.
  Where it does, the fix is the existing `focus-nonce`/`refocus`
  grammar (`timer-setter.slint:137`), not a weaker assertion.
- **R4 — breakpoint threshold**: if `480 px` is not below the compact
  breakpoint, the compact fixture tests the desktop board again.
  Assert `compact-active` in the fixture.
- **R5 — un-audited surfaces**: `capture-overlay.slint`,
  `notification-surface.slint`, `due-editor.slint`, `timers.slint` and
  `rule-list` presets were not line-audited in this plan.
  `notification-surface` mounts in the today/board fixtures when a
  notification is delivered; add `deliver_task_notification` to
  `today_fixture` so the scan covers it. Any red found there follows
  #665's rule: add a stop, never an exemption.
- **R6 — `feat/review-walk` race**: if it lands first, its row walk
  must register as an item-walk host in this PR.
- **R7 — reading-order on wrapped rows**: board-bar tabs that wrap
  across lines, and plan verbs aligned `end`. The #664 same-line rule
  handles both. Any further reading-order red is a real finding.
- **R8 — the item-walk arm over-covers**: a new button placed inside a
  card would be accepted silently. Mitigation: the arm applies only
  when the touch's nearest delegate ancestor is the registered
  delegate *and* no own-verb sibling scope exists. Document the
  limitation at the registry. A follow-up issue can tighten it with a
  per-host allowed touch-id set if review asks for one.

---

## 8. Estimated diff size

| Area | Lines |
|---|---|
| `tests/focus_coverage.rs` (fixtures, item-walk arm, ~30 tests, modal helpers, self-guards) | +650 / −20 |
| `board-layout.slint` (CtaButton grammar) | +30 |
| `plan-next-day-view.slint` (3 flags, rings, comment) | +15 / −6 |
| `focus-overlay.slint` (stop button grammar, row id) | +30 / −4 |
| `app-window.slint` (9 backdrop ids) | +9 / −9 |
| `status-bar.slint`, `search-palette.slint` (ids) | +2 / −2 |
| **Total** | about **+740 / −40**, medium (M) |
