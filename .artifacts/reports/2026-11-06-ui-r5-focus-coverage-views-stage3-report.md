# [Report] Round 5 Chunk 4 follow-up — the Focus-Coverage Invariant's Remaining Views (Stage 3 Review, #665)

- **Date**: 2026-11-06
- **Branch**: `test/ui-slint-focus-coverage-views` — rebased onto
  `origin/main` after #664 landed (the AGENTS §8 dance dropped the
  eleven already-merged commits; no conflicts). Plan:
  `.artifacts/plans/2026-11-06-ui-r5-focus-coverage-views-plan.md`.
- **Verdict**: PASS with findings fixed on the branch (§4) and three
  production gaps recorded as issues #668, #669, #670 (§3). Every
  view and modal surface in #665's table now has a scan test; every
  Tab-chain view has a walk test; the invariant was mutation-checked
  per view (§2).

## 1. What the scan found, per view

The stage-2 red inventory, checked against the plan's §3/§4
predictions. "Fix" names the landed commit's change.

| Surface | Scan finding | Predicted | Fix |
|---|---|---|---|
| Board (desktop) | `TaskCard::touch`/`done-hit`, `header-hit`, `body-hit`, `divider` per column | yes | item-walk arm (hosts `drop-zone`, `drag-card`) |
| Board (compact) | same under `compact-task` | yes | item-walk arm (host `compact-task`) |
| Board (empty) | `CtaButton::cta-hit` ×2 pointer-only | yes | `CtaButton` takes the IconButton grammar (FocusScope stop, Return/Space, ring) |
| Today | `TaskCard` touches under `today-row`; `StatusBar` timers chip pointer-only | yes | item-walk arm; the timers chip becomes a stop (stronger than the plan's exemption — #665's rule) |
| Stack detail | `TaskCard` touches under `detail-task` | yes | item-walk arm |
| Review | none | yes (green) | — |
| Plan-next-day | `box-earlier/later/remove-touch` (tab-nav off); **`TaskCard::done-hit` on planner cards** | verbs yes; done box **unpredicted** | verbs become tab stops with rings; the planner's row model sets `can_done: false` (the toggle was never wired) |
| Search palette | row `TouchArea` (anonymous) | yes | named `palette-row`/`row-touch`, item-walk arm |
| Focus overlay | switcher row (anonymous); `Stop session` pointer-only; **switcher arrows dead** | rows/stop yes; arrows **unpredicted** | row ids + item-walk arm; stop button takes the IconButton grammar; the adapter's MenuNext/Prev arm walks the FocusMode cursor |
| Timer setter | none | yes (green) | — |
| Date picker | day cell `touch` per cell | yes | `day-cell` id, item-walk arm (`picker.left/right`) |
| Action menu | none | yes (green) | — |
| Dropdown flyout | `row-touch` per row | yes | `flyout-row` id, item-walk arm |
| Quick-list flyout | `pick` per row; **the ladder lost Up/Down to `picker.up/down`** | rows yes; ladder **unpredicted** | `flyout-row` id, item-walk arm; the intercept declines the ladder keys while `TimeFieldFacts.open` |
| Track entries | none | yes (green) | — |
| Capture overlay | none (one TextField) | not audited (R5) | — |
| Backdrops (×9) | the window key root silently covered every full-window touch | **unpredicted scanner hole** | container stops cover nothing (`ContainerPanel` self-guard); backdrops get ids and the nine `EXEMPT` rows |

`EXEMPT` grew only by the nine modal dismissal backdrops, each with
its reason. No "sibling-covered" row of the chunk-4 §2.3 table became
an exemption: `BoardBar::segment-hover`, `add-task-hit`,
`add-col-hit`, `add-task-hit-compact`, the habits IconButtons,
`tracked-today::row-touch`, `box-touch`, `pool-touch`, `commit-hit`,
the menu rows and the track-entries rows all scan green through their
real sibling stops.

## 2. Mutation checks (the invariant guards each view)

One mutation per newly covered view, all applied in one build; each
scan test failed naming the expected `TouchArea`, then the markup was
restored (`git checkout`). Pre-existing Settings/TaskDetail (#664)
were not re-mutated.

| View | Mutation | Scan test failed naming |
|---|---|---|
| Board (desktop) | `add-task-focus` tab-nav → false | `BoardLayout::add-task-hit` ×3 |
| Board (compact) | `add-task-focus-compact` tab-nav → false | `BoardLayout::add-task-hit-compact` |
| Board (empty) | `CtaButton::focus` tab-nav → false | `CtaButton::cta-hit` ×2 |
| Today | `timers-chip-focus` tab-nav → false | `StatusBar::timers-chip-touch` |
| Stack detail | `detail-task` delegate id renamed | R1 guard: 0 of 2 delegates visible |
| Plan-next-day | `earlier-hit` tab-nav → false | `PlanNextDayView::box-earlier-touch` ×2 |
| Search palette | `palette-row` id renamed | `SearchPalette::row-touch` ×6 |
| Focus overlay | `stop-focus` tab-nav → false | `FocusOverlay::stop-touch` |
| Timer setter | `commit-focus` tab-nav → false | `CtaButtonArea::commit-hit` |
| Date picker | `day-cell` id renamed | R1 guard: 0 of 42 cells visible |
| Action menu | `row-focus` tab-nav → false | `ActionMenu::row-hit` ×10 |
| Dropdown flyout | `flyout-row` id renamed | `DropdownFlyout::row-touch` ×2 |
| Quick-list flyout | `flyout-row` id renamed | `QuickListFlyout::pick` ×6 |
| Track entries | `add-focus` tab-nav → false | `TrackEntriesPanel::add-hit` |

Review has no `TouchArea` of its own (nothing to mutate); the capture
overlay is one shared `TextField`.

## 3. Production gaps the walk half found (deferred, tracked)

The modal walk (`assert_modal_walk_stays_inside`) asserts the plan
§4 focus-trap contract. Stage 2 `#[ignore]`d the failing walk tests
citing a report that did not exist; this review classified every red,
fixed the two that were widget-level, and filed the rest:

- **F-1 — no modal focus trap (#668, p1)**: Tab leaves the palette,
  capture overlay, timer setter, focus overlay, track-entries panel
  and date picker into the key root, the chrome and the hosting view,
  and never returns. The fix is a modal-aware traversal bound — the
  chunk-7 key-scope stack's job. Six tests stay `#[ignore]`d with
  #668 as the reason; un-ignoring them is the acceptance.
- **F-2 — mount parity (#669, p2)**: the action menu opens with focus
  left on the view root (rows reached by `menu.next` only). The
  **track-entries** and **date-picker** halves of F-2 were not
  production design and are fixed here (§4.1, §4.2).
- **F-3 — Today cross-board refocus (#670, p2)**: `nav.next-item`
  onto a row of another board re-runs `refocus-view` and parks focus
  on the key root.

Ignoring a correct test is a weakening (AGENTS §5); the handoff
contract (PR green) leaves no third option, so each ignore names its
issue and the issue names the test as its acceptance.

## 4. Findings fixed on the branch

### 4.1 The empty Time-entries panel rendered 4 px tall and unfocused

`TrackEntriesPanel` sizes itself through a `changed preferred-height`
mirror seeded at `Theme.space-1`. With zero entries the list's
preferred height is final on its first evaluation, so `changed` never
fires: the panel stayed at its 4 px seed and the height-transition
focus grab (`changed height => panel-scope.focus()`) never ran. Real
bug for any task without entries. Fix: an `init` seed on the list
layout's mirror. The panel now lays out at 81 px with `panel-scope`
focused; the walk test moved from F-2 to F-1.

### 4.2 The date-picker "keyboard" fixture drove the wrong seam

`date_picker_fixture_by(true)` invoked the opener's default action and
expected grid focus — but the default open keeps the due field
focused by a recorded decision (round-3 D10, typing-first); the
hand-over to the grid is Down-in-field. The fixture now opens by
click and presses Down, asserting `date_grid_focus`. Mount parity
holds; the walk fails only on F-1.

### 4.3 A red commit in the history

`test(ui-slint): … (red)` landed the tests before the fixes, against
AGENTS §8 (every commit stands alone) and the plan's D-V9. The
history is reordered: the ids refactor and the seven widget/adapter
fixes first, then the test commit (reworded), each building and
passing on its own.

### 4.4 Test hygiene

- `board_item_walk_visits_every_card` seeked each column with an
  unbounded `while` — now one `stack.next` per column with an
  equality assertion (structural bound).
- The eight `#[ignore]` reasons cited a nonexistent "#665 report";
  they now cite #668/#669/#670.

## 5. Gates

`scripts/run-quality-gates.sh` (fmt, clippy `-D warnings`, nextest,
doctests, deny, llvm-cov) — green on the final tree; see the PR
checks. `target/llvm-cov-target` removed afterwards.
