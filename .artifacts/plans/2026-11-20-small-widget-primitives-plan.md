# Chunk 16 (round 9 W5) — Small Widget Primitives Plan

- **Date**: 2026-11-20
- **Kind**: stage-1 implementation plan (TDD-first), chunk 16 of round 9,
  branch `refactor/ui-small-widget-primitives`.
- **Scope (round plan §4 W5, O-6)**: `ActionChip`, `SectionHeader`,
  `StatusNotice`, `ConfirmStrip`, `GhostOpener`; `Theme.tone-color` /
  `Theme.tone-fill` deduplicating the 5× tone ladder (F-3, #952).
- **Normative inputs**: round plan §1 O-6 + §4 W5 row;
  `.artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md`
  (§3 small-widget family rows F-3..F-6/F-7/F-9/F-10, §4 sizing, §5
  migration order step 5 — W5 last of the centralization chunks before
  W6); J2 rulings R-1..R-9 + §6 O-6 addendum; chunks 13–15 plans (W3
  `2026-11-19-button-primitives-plan.md`, W4
  `2026-11-20-labeled-value-row-plan.md`, W5-ownership row:
  "Chunk 15 [of the dispatch] (W5) owns GhostOpener, ActionChip,
  SectionHeader, StatusNotice, ConfirmStrip" — the dispatch's
  ghost-variant reading resolved in favor of the inventory family list).
- **Precondition verified**: chunk 15 (PR #1114) merged on main
  2026-11-20T16:38:11Z; worktree cut from origin/main at e79f928c.
- **Status**: PLANNED — no implementation in this commit (stage 1).
- **Surfaces touched**: `ui/widgets/` (5 new files + `theme.slint`),
  `ui/compositions/` (rule-list, status-bar, timers, habits,
  tracked-today, board-layout, due-editor, labels-overlay, toolbar),
  `ui/views/` (task-view, settings-view), `ui/app-window.slint`,
  probe fixture + tests, `docs/ux-surface-census.org`.
- **Platform scope**: both (desktop + kiosk).

## 1. Re-anchor survey (current main, post-chunk-15)

All addresses are `file::symbol` census anchors (#775); line numbers are
the 2026-11-20 main tree and will drift — symbols are the anchor.

### 1.1 Tone ladders (F-3, #952) — five conditional chains, one mapping

| Ladder | Anchor | Shape |
|---|---|---|
| Badge | `widgets/badge.slint::Badge` (two chains: `background` fill with `.with-alpha(0.2)`, `tone-color()` text) | neutral→text-secondary, info→text-primary, success/warning/danger/overdue/sync-active→Semantic.*, else offline |
| ListRow | `widgets/list-row.slint::ListRow` (`text-color` chain) | **neutral→text-primary** (row-text step), info→text-primary, rest canonical |
| StatusDot | `widgets/status-dot.slint::StatusDot` (`background` chain) | neutral→text-secondary, info→text-primary, rest canonical |
| IconButton | `widgets/icon-button.slint::IconButton` (`icon-color()` function + hover-neutral lift) | neutral→hover-dependent text lift, info→text-primary, rest canonical |
| DropdownFacts | `widgets/dropdown.slint::DropdownFacts.tone-color` (+ mirror `::Dropdown.tone-color`) | **DropdownTone** vocabulary: success/warning/danger→Semantic.*, accent→focus-ring — a different enum, not a Tone ladder |

The four Tone-based chains share one canonical mapping:
`neutral→Theme.text-secondary, info→Theme.text-primary,
success/warning/danger/overdue/sync-active/offline→Semantic.<same>`.
The only per-consumer divergence is the neutral step (ListRow lifts it
to text-primary; IconButton makes it hover-dependent) and the fill
derivation (`with-alpha(0.2)`, Badge's `background`).

### 1.2 ActionChip family (inventory #16, F-4)

| Anchor | Shape | Sizing |
|---|---|---|
| `widgets/badge.slint::Badge` | centralized pure count chip — **stays** (Q-D two-primitive ruling) | space-1 pad, badge typography |
| `compositions/rule-list.slint::PresetChip` | reminder preset chip: ActivationStop + tooltip arm + mono text + focus-ring/hover border, `chip-clicked(int)` | min space-5/space-6 (its own token floor pin) |
| `views/task-view.slint` labels chip loop (`task.labels` `for` block, inside the labels section) | swatch square (space-3, hairline border) + caption title + remove IconButton + separate add IconButton; `chip-unassign-clicked(index)` | unpadded Rectangle, caption text |
| `views/task-view.slint` recur-paused chip (`recur-paused-chip != ""` block) | passive tinted caption pill, no affordance | plain surface rectangle |
| `compositions/status-bar.slint` timers chip (`timers-chip := Rectangle` block) | interactive: FocusScope (Return/Space, #665 grammar) + TouchArea + focus border + Badge inside; `timers-chip-clicked()` | Badge-wrapped, min-width from preferred |
| `compositions/status-bar.slint` tracking chip (`tracking-chip-badge := Badge`) | passive Badge — stays | — |
| `compositions/status-bar.slint` delivery chip / files-sync chip rows | caption-tone text (+ optional `!` Badge for files-sync) — composite lines, not chips | — |
| `compositions/toolbar.slint::inbox-chip` | TouchArea + caption count text + a11y button role; `inbox-clicked()` | unpadded |

### 1.3 SectionHeader family (inventory #20, F-6)

| Anchor | Shape |
|---|---|
| `compositions/timers.slint` header row ("Timers" caption + count caption + awaiting Badge (`timers-awaiting` gate) + spacer + setter/create IconButtons with the TimerSetterFacts click-time push) | the full template incl. mid-row badge |
| `compositions/habits.slint` header row ("Habits" caption + count + spacer + create IconButton) | the plain template |
| `compositions/tracked-today.slint` header row (caption + stretch Rectangle **carrying the a11y text role + mono total**) | a summary row, not an actions header |
| `compositions/rule-list.slint` caption row (derived-fact hint caption + spacer + mute/add affordances) | variant: caption is the hint text, no count |
| `views/review-view.slint::AggregateSection` / `::TrackingSection` / `::ReflectionSection` | vertical heading-over-StatCell-grid sections — heading captions without the header-row anatomy |

### 1.4 StatusNotice family (inventory #23, F-9)

| Anchor | Shape |
|---|---|
| `views/settings-view.slint::ErrorLine` (used at the connect-form error mount and the scale-error mount) | **boxed**: surface-raised box, border, danger caption, word-wrap, stretch |
| `compositions/habits.slint` quota-error-hint RoleText (danger caption under the create form) | **bare inline** danger caption |
| `compositions/due-editor.slint` echo row (`echo != ""` → secondary caption) and invalid row (`validity == invalid` → danger syntax hint) | **caption-row** pair, mutually exclusive |
| `widgets/time-field.slint` invalid hint (danger caption, absolutely positioned at `field.natural-height`) | bare inline, site-owned geometry |
| `compositions/status-bar.slint` resurrect notice (secondary caption, elide) | bare inline, neutral |
| `compositions/timers.slint` / habits / tracked-today kiosk absence captions (the #790 honest-absence signal) | bare inline, neutral/secondary |

### 1.5 ConfirmStrip family (inventory #21, F-7)

| Anchor | Message tone | Confirm tone |
|---|---|---|
| `compositions/board-layout.slint` column delete strip (`column.delete-armed` block) | danger, elide, stretch | danger (check) / neutral cancel |
| `compositions/timers.slint` break-rule remove strip (`rule-row.armed` block) | danger, elide, stretch | danger / neutral |
| `views/task-view.slint` recur convert strip (`task.recur-confirm-open` block) | **secondary** (not danger) | **success** / neutral |
| habit convert strip | **retired from the UI** — the confirm now rides the context-menu submenu (`src/menu.rs` convert-arm row; O-7 Q3). Nothing to absorb; recorded. |

### 1.6 GhostOpener family (inventory #22, F-10)

| Anchor | Shape |
|---|---|
| `compositions/board-layout.slint` add-task ghost (`add-task := Rectangle` block: min/max-height space-6+2·space-1, hover surface, FocusScope-**before**-TouchArea (the F8 two-click dodge), Return/Space-free scope (a11y action only), tooltip arm, centered PlusGlyph) | row orientation |
| `compositions/board-layout.slint` add-column ghost (full-height slim column `space-6+space-2` wide, same order dodge, tooltip, centered PlusGlyph) | column orientation |
| nine row-height `IconButton` plus-openers (habits header/form, labels-overlay ×2, time-field, task-view add-label, board-layout compact leg, timers create toggle) | **stay IconButton** — they are already the centralized primitive; GhostOpener is for the bespoke full-shape ghosts only (see §3.5) |

Chunk-15 reconciliation: the W5-ownership note in
`2026-11-20-priority-picker-dropdown-plan.md`'s stay list does not claim
ghosts; the W3 plan explicitly deferred them here ("GhostOpener is
chunk 15's [dispatch-chunk-16's] deliverable"). The #1104/#1105 dropdown
open items are untouched by this chunk — GhostOpener never mounts a
flyout and shares no facts global.

## 2. Primitive designs (customization points)

Layering: L1 under `ui/widgets/`, one file per component, callbacks-only
intents, presentation-only. Every file carries the SPDX header
(`AGPL-3.0-only` — taskboard-ui-slint) and the layer/decision-ref header
comment. `#![forbid(unsafe_code)]` is crate-level already.

### 2.1 `Theme.tone-color` / `Theme.tone-fill` (F-3) — do first

```slint
// theme.slint
public pure function tone-color(tone: Tone) -> color {
    tone == Tone.neutral ? text-secondary
    : tone == Tone.info ? text-primary
    : tone == Tone.success ? Semantic.success
    : tone == Tone.warning ? Semantic.warning
    : tone == Tone.danger ? Semantic.danger
    : tone == Tone.overdue ? Semantic.overdue
    : tone == Tone.sync-active ? Semantic.sync-active
    : Semantic.offline
}
public pure function tone-fill(tone: Tone) -> color {
    return Theme.tone-color(tone).with-alpha(0.2);
}
```

Adoption, per-arm diff recorded:

- **Badge**: both chains → `Theme.tone-color` / `Theme.tone-fill`.
  Zero-arm diff (its ladder already equals the canonical mapping).
- **StatusDot**: `background` → `Theme.tone-color`. Zero-arm diff.
- **IconButton**: `icon-color()` keeps its neutral hover lift and
  disabled step (interaction state, not the ladder) but delegates every
  non-neutral arm to `Theme.tone-color`. Zero visual diff.
- **ListRow**: delegates non-neutral arms; keeps its neutral→text-primary
  row-text step site-side (one conditional, one documented override —
  the ladder's synchronized arms are what the function owns). Zero
  visual diff.
- **DropdownFacts.tone-color**: **stays** — the DropdownTone vocabulary
  (incl. `accent→focus-ring`) is not a Tone ladder; recorded in the
  widget header + census. #952 closes on the four Tone ladders; a
  DropdownTone↔Tone unification is W6 scope at the earliest.
- New primitives (§2.3–2.6) consume only the functions — no new chains
  anywhere (the §5 lint law).

### 2.2 `ActionChip` (`ui/widgets/action-chip.slint`)

Properties/callbacks: `text: string` (paint + a11y label + tooltip
label), `tone: Tone` (default neutral), `swatch: color` (optional
leading square; a sentinel-free design: `has-swatch: bool`), `removable:
bool` (renders the trailing remove IconButton internally,
`removed()`), `interactive: bool` (default true; false renders the
passive posture — no scope, no touch, `invoked()` unreachable),
`value-role: TextRole` (caption default, mono for PresetChip parity),
`hint-keys: [string]`, `min-height: length` (default `Theme.space-5`,
the §4 compact tier), `enabled: bool`, `invoked()`, `removed()`.

Internals: the interactive posture carries ActivationStop (the
PresetChip grammar: publish + tab stop + stop-id seam) + the
TouchArea-after-scope order + the TooltipState.hover arm (blanket rule
a) + the focus border. Geometry identity: the removal target renders
the swatch square (space-3, hairline border), padding space-2/space-1,
radius-small — the label-chip block's exact rects; PresetChip keeps its
min space-5/space-6 pin via `min-height`/`min-width` properties.

### 2.3 `SectionHeader` (`ui/widgets/section-header.slint`)

Properties: `title: string`, `count: string` (empty hides the count
caption), `status-badge-text: string` (empty hides — the timers
awaiting badge's slot), `status-badge-tone: Tone`, `hint-keys: [string]`;
`actions` via `@children` (the header IconButtons, rendered after the
stretch spacer — the template's order). The row is a HorizontalLayout:
title caption, count caption, optional Badge, stretch, children.

Scope note: the timers header's TimerSetterFacts click-time push lives
in the consumer's IconButton `clicked` handlers — the widget adds no
facts coupling (the anchor-mirror absorption is W6/host scope).

### 2.4 `StatusNotice` (`ui/widgets/status-notice.slint`)

Properties: `text: string`, `tone: Tone` (danger default for error
mounts, neutral/secondary for notices), `shape: enum {inline, boxed,
caption-row}` (`NoticeShape`), `wrap: bool` (boxed: word-wrap; inline:
elide), `enabled/a11y` passthrough not needed (passive text —
`AccessibleRole.text` with the text as label, matching today's
captions). The **boxed** shape reproduces ErrorLine's exact rects
(surface-raised, focus-ring-width border, Theme.border, radius-small,
min-height from preferred, stretch). Inline/caption-row render the bare
RoleText. Consumers keep their own mount conditions and geometry
(time-field's positioned hint keeps its `y` site-side).

### 2.5 `ConfirmStrip` (`ui/widgets/confirm-strip.slint`)

Properties: `message: string`, `destructive: bool` —
destructive ⇒ message tone danger + confirm tone danger; otherwise
message tone secondary + confirm tone success (exactly the two observed
grammars) — `confirm-label: string`, `cancel-label: string`,
`horizontal-stretch` on the message with `overflow: elide` (the
board/timers shape; the recur strip's shorter message is unaffected),
callbacks `confirmed()`, `cancelled()`. Internals: HorizontalLayout,
spacing space-2, the two IconButtons (check/remove glyphs, labels from
the properties — the a11y labels stay caller-supplied strings, byte
identical at every site).

### 2.6 `GhostOpener` (`ui/widgets/ghost-opener.slint`)

Properties: `orientation: enum {row, column}` (`GhostOrientation`; row ⇒
min/max-height `space-6 + 2*space-1`, column ⇒ width
`space-6 + space-2`, height 100%), `label: string` (a11y + tooltip
label), `description: string` (tooltip text), `hint-keys: [string]`,
`enabled: bool`, `hovered: out bool` (the board's ghost-tracking
observation seam), `invoked()`. Internals bake the **FocusScope-first /
TouchArea-after** order (the F8 two-click dodge, currently hand-copied
with its warning comment at both mounts), `focus-on-tab-navigation`,
`AccessibleRole.button`, `accessible-action-default`, the
TooltipState.hover arm, and the centered PlusGlyph with the
hover-dependent glyph color. Rendering PlusGlyph stays internal; no
`Icon` variant until a consumer asks (no speculative generality).

Plus-IconButton openers: **stay** `IconButton` — already the
centralized primitive; converting nine working sites onto a second
widget that renders the same grammar is churn with a11y-regression
risk. Recorded as the Q-E resolution: mechanism unified for the ghost
shapes, affordance identity (board's full-shape ghosts vs row
IconButtons) preserved — #820's operator-visible mismatch stays closed
by the ruling text, not by code.

### 2.7 Sizing (§4 / R-6)

No new Theme tokens required: the sourced defaults this family needs
are the existing space tokens (chip compact tier = space-5, control row
= space-6, R-6 dialog/menu tokens already landed in chunks W1/W2).
Adoption law: every new widget expresses sizes as `Theme.space-*` /
`px * Theme.base-scale` (token lint); the absorbed sites' divergent
min-heights regularize to the widget defaults, per-site overrides only
where the geometry identity demands it (PresetChip's floor pin,
GhostOpener's two orientations). The 44/48 touch-target reservation
stays backlog (inventory Q-B; kiosk is passive).

## 3. Absorption map (convert / stay / why)

| # | Anchor (file::symbol) | Action | Why / notes |
|---|---|---|---|
| 1 | `theme.slint` — new `tone-color`/`tone-fill` | **add** | F-3 pattern-card, first commit |
| 2 | `widgets/badge.slint::Badge` | **convert (ladder only)** | delegates both chains; Badge itself stays the count primitive (Q-D) |
| 3 | `widgets/list-row.slint::ListRow` | **convert (ladder only)** | neutral row-text step stays site-side |
| 4 | `widgets/status-dot.slint::StatusDot` | **convert (ladder only)** | direct |
| 5 | `widgets/icon-button.slint::IconButton` | **convert (ladder only)** | hover/disabled steps stay, non-neutral arms delegate |
| 6 | `widgets/dropdown.slint::DropdownFacts.tone-color` | **stay** | DropdownTone vocabulary, accent arm; W6 note |
| 7 | `compositions/rule-list.slint::PresetChip` | **convert → delete** | ActionChip interactive, mono role, min-pin preserved; `chip-clicked(index)` seam byte-identical |
| 8 | `views/task-view.slint` labels chip loop | **convert → delete** | ActionChip swatch+removable; `chip-unassign-clicked(index)` and the trailing add IconButton unchanged |
| 9 | `views/task-view.slint` recur-paused chip | **convert → delete** | ActionChip interactive=false, tone warning — verify current paint (surface rect) maps 1:1; if the paused chip's paint is plain-surface (untinted), pass tone neutral + keep surface posture recorded |
| 10 | `compositions/status-bar.slint` timers chip block | **convert → delete** | ActionChip interactive absorbs the #665 FocusScope/TouchArea/border grammar; `timers-chip-clicked()` unchanged; the Badge-inside becomes the chip's own badge-typography text — **visual delta D-1** (§5) |
| 11 | status-bar tracking / delivery / files-sync chip rows | **stay** | passive Badge / composite caption+badge lines, not chip shapes; files-sync's caption-line noted as a StatusNotice-adjacent shape, left (cost > gain) |
| 12 | `compositions/toolbar.slint::inbox-chip` | **convert → delete** | ActionChip interactive, caption count text; `inbox-clicked()` unchanged |
| 13 | `compositions/timers.slint` header row | **convert → delete** | SectionHeader title+count+status-badge+actions; the awaiting-Badge gate becomes `status-badge-text` |
| 14 | `compositions/habits.slint` header row | **convert → delete** | SectionHeader plain template |
| 15 | `compositions/tracked-today.slint` header row | **stay** | summary row (a11y-text spacer + mono total) — not the actions-header anatomy; recorded |
| 16 | `compositions/rule-list.slint` caption row | **convert → delete** | SectionHeader with title = hint text, count empty; mute/add affordances ride `@children` |
| 17 | review-view `AggregateSection`/`TrackingSection`/`ReflectionSection` | **stay** | heading-over-grid sections, no header-row anatomy; recorded in census |
| 18 | `views/settings-view.slint::ErrorLine` (+ both mounts) | **convert → delete** | StatusNotice boxed; both mounts swap |
| 19 | habits quota-error-hint | **convert** | StatusNotice inline, danger |
| 20 | `compositions/due-editor.slint` echo + invalid rows | **convert** | StatusNotice caption-row ×2 (secondary echo, danger hint); mutual exclusion stays at the mount |
| 21 | `widgets/time-field.slint` invalid hint | **convert** | StatusNotice inline; positioned geometry stays site-side (widget-local, fine) |
| 22 | status-bar resurrect notice | **convert** | StatusNotice inline neutral, elide |
| 23 | timers/habits/tracked-today absence captions | **convert** | StatusNotice inline neutral (each keeps its own gate/label) |
| 24 | board-layout delete strip | **convert → delete** | ConfirmStrip destructive |
| 25 | timers break-rule strip | **convert → delete** | ConfirmStrip destructive |
| 26 | task-view recur convert strip | **convert → delete** | ConfirmStrip non-destructive (success confirm) |
| 27 | habit convert strip | **n/a — retired** | confirm lives in the context menu (O-7 Q3); recorded |
| 28 | board-layout add-task ghost | **convert → delete** | GhostOpener row |
| 29 | board-layout add-column ghost | **convert → delete** | GhostOpener column |
| 30 | nine plus IconButtons (§1.6) | **stay** | already centralized; Q-E ruling |

Behavior-preservation bar: every `clicked`/`invoked`/`confirmed` seam
routes the identical callback with identical payloads (the chunk-15 D-9
dispatch-identity law). Zero sanctioned behavior deltas are planned;
the only visual deltas are D-1 (timers chip costume, §5) and the
golden re-baseline set; both get stage-3 recorded reasons.

## 4. TDD-first sequence

Test fixture pattern: a `tests/small_widget_probe.slint` +
`small_widget_probe_tests.rs` pair, modeled on
`labeled_value_row_probe.slint` (mount the primitive at both
orientations/tones/shapes, expose geometry/seams as probe properties;
assert rects and typed outcomes — no text assertions, no sleeps; the
probe fixture needs no harness clock). Journey/fold coverage re-runs
unchanged: `journeys.rs` timers/habit/label flows, `walk_probe_tests`,
`dropdown_probe_tests` (neighbor check), the kiosk gates, goldens.

1. **T1 — tone functions (F-3)**: probe asserts all eight `Tone` arms
   of `Theme.tone-color`/`tone-fill` (typed color equality against the
   Semantic constants — pure-function assertions, no geometry).
   Then adopt in Badge/ListRow/StatusDot/IconButton. Goldens: expect
   zero pixel delta; any delta is a bug (the mapping is identical).
2. **T2 — StatusNotice**: probe asserts the boxed shape's rects equal
   ErrorLine's measured rects (border, min-height-from-preferred,
   stretch) and the inline/caption-row shapes render the tone ladder
   via `Theme.tone-color`. Then migrate mounts 18–23; delete ErrorLine.
3. **T3 — ConfirmStrip**: probe asserts both `destructive` postures'
   message/confirm tones (`matches!`-style tone assertions via exposed
   probe colors) and the confirm/cancel seams. Migrate 24–26.
4. **T4 — ActionChip**: probe asserts the swatch square's rect, the
   removable affordance's presence seam, interactive=false posture (no
   tab stop — the ActivationStop absence is the assertion), and the
   PresetChip min-pin. Migrate 7–10, 12. D-1 golden re-baseline here.
5. **T5 — SectionHeader**: probe asserts row anatomy order
   (title/count/badge/stretch/actions x-offsets) and the empty-count /
   empty-badge hides. Migrate 13–14, 16.
6. **T6 — GhostOpener**: probe asserts the row/column rects, the
   scope-before-touch hit order (the two-click regression probe:
   first physical click invokes), hover seam, tooltip arm. Migrate
   28–29.
7. **T7 — census + lint law**: `docs/ux-surface-census.org` rows
   re-pointed with symbol anchors; the anti-ad-hoc note (no
   chip/header/notice/strip/ghost grammar outside `ui/widgets/`)
   recorded in the census + widget headers (W7 grows the full doc).

Each step: red probe → minimal widget/migration → green →
`cargo fmt`/`clippy -D warnings`/`nextest -p taskboard-ui-slint`
incrementally; full gates through `scripts/run-quality-gates.sh` before
handoff (dispatcher owns the full trio per the dispatch discipline).

## 5. Behavior deltas & golden policy

- **D-1 (timers chip costume)**: the Badge-inside-Rectangle composite
  becomes the ActionChip's native badge-typography pill; the hover
  surface and focus border follow the ActionChip grammar. Recorded
  visual delta, journey `timers-chip-clicked` re-passes byte-identical.
- **Goldens**: T1 must be pixel-identical; T2–T6 re-baseline only the
  goldens whose windows contain migrated mounts (expect task-view,
  today, kiosk-today, board). Every re-blessed golden gets a per-file
  reason in the stage-3 report (chunk-14's ruling format).
- **No new keyboard paths**: ActionChip keeps ActivationStop's existing
  grammar (tab stop where PresetChip/label chips had one; the paused
  chip and tracking chip gain none — interactive=false). GhostOpener
  preserves the ghosts' current a11y posture exactly (button role,
  action-default; the scope's key behavior unchanged).

## 6. Risks

- **R-1 Hit-testing order**: the ghost/strip/chip TouchArea-vs-FocusScope
  order is load-bearing (F8). The probe asserts invocation-on-first-click
  at every converted interactive mount; the board journeys are the net.
- **R-2 PresetChip's index seam**: `chip-clicked(int)` / `removed(int)`
  carry loop indices; ActionChip's callbacks are parameterless — the
  consumer closes over the index in the `for` delegate (the label-chip
  loop's existing pattern). No seam change.
- **R-3 StatusNotice boxed stretch**: ErrorLine stretches
  (`horizontal-stretch: 1`) inside settings' forms; the widget must
  expose the stretch or the mount wraps it — probe asserts the
  connect-form width identity.
- **R-4 Golden churn breadth**: six migration steps touch windows
  covered by many goldens; stage 2 re-blesses per step, not in one
  sweep, so any regression bisects to a step.
- **R-5 Icon-Badge import cycles**: ActionChip imports IconButton
  (removable) and StatusNotice imports nothing new; keep widget→widget
  imports one-directional (chip→icon-button only) to avoid slint
  import cycles.

## 7. Diff estimate

New: 5 widget files (~450 lines) + probe fixture + tests (~450).
Theme: +15. Migrations: ~12 files edited, ~600 lines deleted
(the four strips' and ErrorLine's full bodies, both ghosts, PresetChip,
the chip blocks, the ladder chains). Net production ~±0 (≈ +470 / −600),
tests +450, census/docs +40. Roughly 6–8 commits, one PR.

## 8. Out-of-scope (recorded)

MenuShell/MenuRow + the anchor-facts host absorption (W6, chunk 17);
DropdownTone↔Tone unification; tracked-today summary header and
review-view sections' conversion; Badge folding into ActionChip (Q-D:
two primitives stand); files-sync/delivery composite chip-lines; touch
target mode (Q-B backlog); `docs/widget_interactions.org` full taxonomy
(W7).
