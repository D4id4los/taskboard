# [Plan] Menu shell — round 9 chunk 17 (W6, O-6)

- **Date**: 2026-11-20
- **Kind**: stage-1 implementation plan (TDD-first) — behavior-preserving
  centralization: `MenuShell` + `MenuRow` unify the menu-row grammars;
  `AnchoredOverlayHost` absorbs the anchor-facts mirroring residue
  (F-5 / #954). Deviations from today's behavior only where a recorded
  reason and a new test say otherwise.
- **Origin** (normative): round plan
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §1 O-6 +
  the W6 row (branch `refactor/ui-menu-shell`); whole-widget audit
  `.artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md`
  §1 row 10 + §3.2 MenuShell/MenuRow + F-5 (read from local branch
  `chore/artefact-sweep-2026-11-19`; PR not yet merged);
  `2026-11-18-widget-framework-ux-plan.md` (J2) rulings + the §6 O-6
  addendum (rows W-6/W-7); menus research
  `2026-11-18-round9-menus-controls-research.md` §3/§4; chunk-15 plan
  `2026-11-20-priority-picker-dropdown-plan.md` (the de-sited mount +
  click-time mirror precedent); chunk-16 plan
  `2026-11-20-small-widget-primitives-plan.md` (the DropdownTone W6
  note); `docs/widget_interactions.org` ("Menu — MenuShell in W6").
- **Preconditions verified 2026-11-20**: chunks 13 (PR #1089-class),
  14 (#1102), 15 (#1114), 16 (#1123) all MERGED on `origin/main`
  (`704a54e8`); `scripts/worktree-doctor.sh` exit 0 (140G free /
  19.3G available); worktree `../taskboard-WT-refactor-ui-menu-shell`
  created with `-b refactor/ui-menu-shell` off `origin/main`.
- **Status**: READY for stage 2.

## 0. Re-anchoring note (the audit's map vs current main)

The inventory report's line numbers and F-5 site list predate chunks
5/13/14/15/16. On current main:

- The **twin pickers are gone as mirror sites**: `RemotePicker` /
  `DeckPublishPicker` are copy-constant `inherits Dropdown` wrappers
  (`settings-view.slint::RemotePicker` / `::DeckPublishPicker`); their
  anchor mirroring lives ONCE inside the Dropdown widget (chunk 15's
  click-time push + keep-alives, `dropdown.slint::DropdownFacts`).
- The **priority slot rows are gone** (chunk 15 adopted the Dropdown;
  the audit's "migrate the priority picker onto MenuShell first"
  sequencing is superseded — the check glyph survives as the dropdown
  class's selected-row law, `dropdown-flyout.slint::check-glyph`).
- Several hand clamp/flip mounts became `AnchoredOverlayHost` mounts
  (chunk 5's overlay migration M1–M6). The residue is enumerated in
  §3 below — verified against `origin/main` on 2026-11-20.

## 1. Decisions (all settled; no open questions)

| ID | Decision | Reason |
|---|---|---|
| D-1 | **MenuShell** (`ui/widgets/menu-shell.slint`, new) is the panel chrome: `surface-raised` background, focus-ring border, `radius-medium`, drop shadow, `clip`, and the two flyouts' row-count height cap (`min(row-count·space-6, space-6·6)`) behind a `cap-rows: bool` (the flyouts pass true; ActionMenu passes false — its fill-to-floor height law is its own, §D-3). `@children` renders the rows area. NO keyboard ladder, NO dismissal, NO facts mirroring — those stay at their contract owners | the ladder is widget-internal per consumer contract (dropdown: field scope; quick-list: panel FocusScope; ActionMenu: none — the cursor is actions, D3); a shell that owned keys would break all three contracts at once |
| D-2 | **MenuRow** (`ui/widgets/menu-row.slint`, new) absorbs the shared row anatomy: leading slot (busy pulse dot OR tone-coloured indicator glyph, both collapsible), label RoleText, KbdHint trailing, submenu chevron (`IconKind.chevron-right`), check glyph on `selected` (the D-5 class-wide law generalized), hover-union background, `enabled` grey, a11y (label + description + default action), tooltip hover seam. Properties: `label`, `description`, `enabled`, `highlighted`, `selected`, `submenu`, `busy`, `leading-glyph: IconKind`, `leading-tone: DropdownTone`, `hint-keys: [string]`, `in-list: bool` (accessible-role list-item vs button — both consumers exist), `text-role: Typography` (mono for the quick-list's time labels; body-emphasized elsewhere). Callbacks: `invoked()`, `hovered(x,y,w,h)`, `unhovered()`, `context(x,y)` (unassigned = no-op, the D-6 precedent). Consumers close over their `for` index | the union with every knob backed by ≥1 live consumer (the no-speculative-generality law; chunk 16's `grab()` precedent). `leading-tone` keeps the DropdownTone vocabulary — §D-7 |
| D-3 | **ActionMenu adopts MenuRow internally**; its public API is untouched (`entries`/`submenu-entries`/`quick-bar`/`page`/`page-cursor`/`invoke-row`/compact back row/quick bar/Facts mirror/height law/scroll indicator all stay at the composition). The scroll-into-view `watch` mirror stays at ActionMenu's `for` (it needs the row's frame inside its own Flickable; MenuRow exposes no geometry) | the app-window mounts, the Rust menu registry, and every menu journey read the composition's surface; reshaping it would churn dispatch for zero centralization gain (dispatch byte-identical is §5's bar) |
| D-4 | **DropdownFlyout and QuickListFlyout become MenuShell mounts**: MenuShell chrome + their existing ladder scope + their existing `ListView` (the virtualization seam stays — row counts are model-driven and unbounded; ActionMenu's Flickable stays at ActionMenu per D-3). Their rows render MenuRow. QuickList's mono label rides `text-role`; the `typed` key forwarding and the `TimeFieldFacts` ladder mirroring are untouched | the two flyout panels are already chrome-identical (8 lines ×2) and row-grammar siblings (~40 lines ×2); the shells' remaining differences (ladder scope, seek seams) are exactly the parts that must NOT merge (D-1) |
| D-5 | The labels roster rows **stay** bespoke (`labels-overlay.slint::LabelRow`): they are form rows carrying two pointer-grade IconButtons + an armed strip + a swatch square — not menu rows; the audit's row-10 listing was a shape cousin, not a grammar twin. Recorded at the census | absorbing them would either strip their verbs (a behavior change with no ruling) or grow MenuRow to a form-row primitive (a different widget class, O-6's taxonomy keeps distinct) |
| D-6 | The **ActionMenu window mount stays hand-positioned** (`app-window.slint::KeyboardState.menu-open` arm): the menu has a pointer-anchored mode (round-1 D13 context menus) and a fill-to-floor height law (`available-height` binding), both outside the host's anchor-rect contract; converting would redesign the menu placement law under a refactor chunk. Recorded as a stay; the general "host grows a pointer-anchor + fill mode" idea is backlog (§9) | behavior preservation; the menu's clamp expressions are already single-copy |
| D-7 | **DropdownTone stays a distinct enum** (settles chunk 16's census note): `Theme.tone-color`'s Tone ladder has no `accent` arm (`DropdownTone.accent → Semantic.focus-ring`); unification would grow the Tone vocabulary for exactly one consumer — the anti-speculative-generality law says stay. Recorded at `docs/widget_interactions.org`'s dropdown section; backlog note filed (§9) | chunk-16 plan line 150: "W6 scope at the earliest" — decided: not even W6 |
| D-8 | **AnchorRect→global mirroring absorbs the idiom once**: `widgets/anchored-overlay.slint` gains `AnchorMirror` — a zero-render component the producer binds its anchor's realized window-space rect into (`source-x/y/width/height`), which invokes ONE callback `published(x, y, w, h)` from `init` + `changed` on each source (the init + keep-alive + stale-while-closed + testing-backend lore written once). Slint 1.18.1 cannot parameterize globals, so each family's producer assigns `published` to its facts-global writes — one assignment block per producer, down from ~8–20 lines of hand-written handler plumbing each | F-5's letter ("the host absorbs the mirroring") is physically a producer-side idiom on current main (the hosts already read facts via plain bindings); the honest absorption is one copy of the idiom plus the two remaining hand mounts converted (D-9) |
| D-9 | **The last two hand mounts convert to `AnchoredOverlayHost`**: (a) the track-entries panel (`app-window.slint` track-entries-open arm — hand clamp/flip + hand backdrop, `TrackPanelFacts.anchor-*`), (b) the board color strip (`app-window.slint` board-color-strip-open arm — same shape, `BoardBarFacts.anchor-*`). Both become host mounts; their backdrops die (the host's dismissal layer) | the overlay-migration law (M1–M6): hand clamp/flip + hand underlay are the duplication; both panels are plain below/flip/clamped rects — exactly the host's contract. Track-entries' width law (`max(preferred, 360·scale)`) rides `panel-preferred-width` |
| D-10 | **Widget-internal mirrors stay**: `DropdownFacts` (dropdown.slint, chunk-15 pattern) and `TimeFieldFacts` (time-field.slint, chunk-5 pattern) keep their in-widget push — each is already a single copy inside its widget; re-routing through AnchorMirror would add a component per widget for no dedup. `SettingsBoardFacts.field-*` and `MenuPanelFacts` are journey observation seams, not mount inputs — they stay verbatim | the #954 close criterion is "the idiom is written once where it is still hand-repeated", not "every global is rewritten" |
| D-11 | **Dispatch byte-identical**: zero Rust adapter changes. Every seam keeps its name and payload: `menu-entry-invoked` / `menu-submenu-entry-invoked` / `menu-quick-bar-invoked` / `menu-level-back`, the dropdown class callbacks, `track-entry-*` / `track-form-*`, `board-color-picked` / `board-color-strip-dismissed`. No `ActionId`, no `StateCommand`, no `UiRequest` delta | the behavior-preservation bar; menu journeys and goldens re-pass unchanged (§5) |
| D-12 | **Visual deltas: none intended.** MenuRow renders each consumer's current anatomy byte-identically (the check glyph only where `selected` is pushed — dropdown rows only; menus pass nothing). Any golden drift is a bug hunt first, a re-baseline with a per-file recorded reason second (§14 law) | geometry-identical replacement is the chunk's contract |
| D-13 | **The habit context page rides for free**: `menu.rs::habit_context_page` projects into the same `KeyboardState` model the main menu uses; one ActionMenu renders both. The H2/H4 journeys (`tests/journeys/habits_journey.rs` us1/us2/h4 rows, the B6 task-menu pin, the kiosk greyed-menu gate) re-pass untouched and are this chunk's proof that the context-menu family survived the absorption | the habit page is a menu projection, not a separate grammar |
| D-14 | Census + doc writes use symbol anchors per #775: `docs/ux-surface-census.org`'s widget-layer section gains the W6 landing entry (MenuShell/MenuRow/AnchorMirror with `file::symbol` anchors, the absorption map, the stays incl. D-5/D-6/D-10 with reasons); `docs/widget_interactions.org`'s "Menu" bullet flips to the landed law (MenuShell chrome / MenuRow anatomy / ladder ownership per contract / DropdownTone ruling per D-7) | round-plan §6 census law |
| D-15 | **UX-plan authority**: chunk 17 is behavior-preserving; its UX journey authority is the accepted J2 plan + the O-6 addendum (rows W-6/W-7). Stage 3 runs `scripts/ux-coverage.sh` on J2 plus the walkthrough protocol for the menu journeys re-verified; the gate is never CI | AGENTS §7 (no new user-visible capability ⇒ no new journey plan; the stays D-5/D-6/D-10 are non-changes) |

## 2. The absorption map (the §14 law: before any code)

### 2.1 Menu-row grammars → MenuShell/MenuRow

| Instance | Converts? | Onto | Why |
|---|---|---|---|
| ActionMenu rows (`compositions/action-menu.slint::ActionMenu` row loop — main menu root page, object/task/board/habit context pages, compact replace page, fly-out instance) | YES — rows only | `MenuRow` (D-3: composition machinery stays) | the audit's row-10 primary grammar; one component, every menu inherits |
| DropdownFlyout rows (`widgets/dropdown-flyout.slint::DropdownFlyout`) | YES — rows + chrome | `MenuRow` + `MenuShell` | the six consumer kinds' shared panel (settings ×2, board bar ×2, task importance/urgency) |
| QuickListFlyout rows (`widgets/quick-list-flyout.slint::QuickListFlyout`) | YES — rows + chrome | `MenuRow` + `MenuShell` | the time combobox STAYS bespoke as a widget (chunk-15 D-10, J5 R-5) but its row rendering is the same grammar; converging the rows is not the ladder convergence the ruling declined |
| Labels roster rows (`compositions/labels-overlay.slint::LabelRow`) | NO — stays | — | form rows with pointer verbs + armed strip + swatch (D-5) |
| Priority slot rows | GONE (chunk 15) | — | re-anchoring note §0 |
| Recur weekday toggles | NO — stays | — | ToggleStrip's consumer (chunk 13); never a menu grammar |
| SegmentedControl / board-bar tab strips | NO — stays | — | distinct widget classes (audit §3.1) |

### 2.2 Anchor-facts mirroring → AnchorMirror / AnchoredOverlayHost (#954)

| Site (current main) | Converts? | Onto | Why |
|---|---|---|---|
| Track-entries panel mount (`app-window.slint`, track-entries-open arm: hand backdrop + hand clamp/flip over `TrackPanelFacts.anchor-*`) | YES | `AnchoredOverlayHost` (D-9a) | the last large-panel hand mount of the M1–M6 family |
| Board color strip mount (`app-window.slint`, board-color-strip-open arm over `BoardBarFacts.anchor-*`) | YES | `AnchoredOverlayHost` (D-9b) | same shape, same law |
| TimerSetterFacts producer (`compositions/timers.slint::setter-anchor-*` changed handlers + init push) | YES | `AnchorMirror` (D-8) | the idiom's oldest copy (F-5 site 1) |
| DatePickerFacts producer (`views/task-view.slint::date-anchor-*` keep-alives + the due/scheduled column pushes) | YES | `AnchorMirror` (D-8) | F-5 site 2 |
| TrackPanelFacts opener producer (`views/task-view.slint` tracked-entries opener mirror) + form-grid producer (`compositions/track-entries.slint::form-grid-anchor-*`) | YES | `AnchorMirror` (D-8) | F-5 sites 3a/3b — one family, two producers |
| BoardBarFacts anchor producer (`compositions/board-bar.slint` changed keep-alives) | YES | `AnchorMirror` (D-8) | the strip's feed (D-9b's input) |
| TimeFieldFacts (`widgets/time-field.slint` in-widget push + keep-alives) | NO — stays | — | single copy inside the widget (D-10, chunk-5 pattern) |
| DropdownFacts (`widgets/dropdown.slint` in-widget push + keep-alives) | NO — stays | — | single copy inside the widget (D-10, chunk-15 pattern) |
| SettingsBoardFacts field-* / MenuPanelFacts (observation seams) | NO — stays | — | journeys' seams, not mount inputs (D-10) |
| ActionMenu's MenuPanelFacts self-mirror | NO — stays | — | the menu mount stays hand-positioned (D-6); the mirror is its observation seam |

## 3. Concrete shapes

### 3.1 MenuRow (new, `ui/widgets/menu-row.slint`)

```slint
export component MenuRow inherits Rectangle {   // height: Theme.space-6
    in property <string> label;  in property <string> description: "";
    in property <bool> enabled: true;
    in property <bool> highlighted: false;      // the hover-union bg (F29)
    in property <bool> selected: false;         // check glyph (dropdown law)
    in property <bool> submenu: false;          // chevron-right (chunk-2 law)
    in property <bool> busy: false;             // the pulse dot slot
    in property <IconKind> leading-glyph;       // `none` collapses the slot
    in property <DropdownTone> leading-tone: DropdownTone.neutral;
    in property <[string]> hint-keys: [];
    in property <bool> in-list: false;          // a11y list-item vs button
    in property <Typography> text-role: Typography.body-emphasized;
    callback invoked();  callback hovered(length, length, length, length);
    callback unhovered();  callback context(length, length);  // no-op default
}
```

Leading slot precedence: `busy` wins over the glyph (the r2 D34
ceremony); both collapse the slot when absent. The hover seam fires the
same payload shape the flyout rows report today (absolute window-space
rect) so `TooltipState.hover` call sites survive verbatim. The row's
disabled grey (`text-secondary`), hover gating (`TouchArea.enabled`),
and the tooltip-outlives-row unhover (r2 D13) move in verbatim.

### 3.2 MenuShell (new, `ui/widgets/menu-shell.slint`)

```slint
export component MenuShell inherits Rectangle {
    in property <int> row-count: 0;
    in property <bool> cap-rows: true;   // min(rows·space-6, space-6·6)
    height: cap-rows ? min(root.row-count * Theme.space-6, Theme.space-6 * 6)
                     : self.preferred-height;
    @children
}
```

Chrome (bg/border/radius/shadow/clip) is the shell's body; height stays
overridable at the mount for cap-rows: false consumers.

### 3.3 AnchorMirror (new, in `ui/widgets/anchored-overlay.slint`)

```slint
export component AnchorMirror {
    in property <length> source-x;  in property <length> source-y;
    in property <length> source-width;  in property <length> source-height;
    callback published(length, length, length, length);
    init => { self.emit(); }
    changed source-x / source-y / source-width / source-height => { self.emit(); }
    public function emit() { root.published(self.source-x, ..., self.source-height); }
}
```

Producer-side residue per site: bind the four sources (the
`absolute-position`-backed properties the site already declares) and
assign `published` to the facts-global writes. The changed/init
plumbing, the stale-while-closed note, and the testing-backend
keep-alive rationale exist once.

### 3.4 Flyout re-shape (example: DropdownFlyout)

`DropdownFlyout` becomes `MenuShell { row-count: rows.length; ListView { for
row[i] in rows: MenuRow { ... } } }` — the ListView, the seek seams
(`DropdownFacts.highlight/seek-nonce`), the select-close write, and the
ladder contract comments move over verbatim; the chrome lines and row
anatomy lines are deleted (they are the shell's and the row's now).
`QuickListFlyout` likewise (its FocusScope ladder + `typed` forwarding
stay at the composition).

## 4. TDD order (test-first at each step; probes before migrations)

New probe file `crates/taskboard-ui-slint/tests/probes/menu_shell_probe_tests.rs`
(fixture pattern per `widget_framework_probe_tests.rs`):

1. **G-1 MenuRow anatomy** — hover-union bg on `highlighted`/hover,
   disabled grey + dead TouchArea, check glyph only when `selected`,
   chevron only when `submenu`, KbdHint presence from `hint-keys`,
   busy-dot precedence over the glyph, slot collapse (width 0), the
   hover/unhover/invoked/context callback edges, both a11y roles.
2. **G-2 MenuShell chrome + cap** — border/shadow/radius/clip set;
   `cap-rows` height math (below/at/above the 6-row threshold);
   `cap-rows: false` passes height through.
3. **G-3 AnchorMirror edges** — `published` fires on init with the
   bound rect and on every source change (the keep-alive contract).
4. **G-4 Host conversions** — track-entries panel + color strip on the
   host: below placement, flip above near the floor, clamp, anchor
   non-intersection (the host probe's law re-exercised at the two new
   mounts), dismissal-layer routing per arm.

Then migrations, one family per commit, journeys + goldens after each:

5. ActionMenu → MenuRow (D-3). Menu journeys + habit-context journeys
   re-pass; menu goldens byte-identical (D-12).
6. DropdownFlyout → MenuShell+MenuRow; QuickListFlyout likewise (D-4).
   Dropdown/quick-list probe tests re-pass; `us1_s1` picker oracle and
   the time-field journey ladder unchanged.
7. AnchorMirror + the four producer conversions (§2.2 rows 3–6).
8. The two host conversions (D-9) with the M-law geometry tests.
9. Census + `widget_interactions.org` writes (D-14); #954 comment at
   PR-open; stage 3: ux-coverage + walkthrough per D-15.

Test-law compliance: no text assertions (typed/structural outs only);
no sleeps (deadline polls / join handles); goldens assert committed
renderings (frozen clock per §6.1 where any clock-derived text could
appear — none is introduced); one behavior one test at the probe layer,
routing pairs only at journey layer.

## 5. Behavior preservation & verification

- Dispatch: byte-identical (D-11) — verified by the full journeys
  suite re-passing with zero Rust edits (`git diff --stat` shows no
  `src/` changes outside none at all).
- Goldens: menu family byte-identical expected; any drift per D-12.
- Journeys of record: `journeys.rs` menu routing pairs (us2/us3
  context-menu family), `us6_s1..s4` priority dropdowns,
  `om_us2_s3`/quick-time pairs, `habits_journey.rs` (us1/us2/h4/B6
  pin/kiosk greyed gate), settings/board-bar dropdown journeys, the
  board-color-strip and track-entries journey rows, `us1_s1`
  picker-close oracle.
- Quality gates via `scripts/run-quality-gates.sh` (fmt, clippy -D
  warnings, nextest, doc tests, deny, the CI-identical coverage trio);
  heavy builds through `scripts/memory-gate.sh`.

## 6. Risks

| Risk | Mitigation |
|---|---|
| MenuRow's internal element order shifts hit-testing or focus (the F8 scope-first record) | the row is a leaf TouchArea/FocusScope pair moved verbatim; G-1 asserts the callback edges; menu journeys re-run before any golden re-baseline |
| ActionMenu's scroll-into-view `watch` mirror depends on the row's frame inside its Flickable | the mirror stays at ActionMenu's `for` (D-3); MenuRow exposes no geometry — the walk-scroll test is the guard |
| Track-entries panel's height/fill law vs the host's content-height panel | the panel already sizes by content (`TrackEntriesPanel` preferred); G-4 asserts the placed rect against the old expression's outputs at the probe fixture before the mount flips |
| `DropdownTone` crossing into `widgets/` (MenuRow's leading-tone) — a widget importing the dropdown vocabulary | accepted and recorded: it IS the menu family's tone vocabulary pending D-7's stay ruling; the import direction (widgets→widgets) is already the norm (DropdownFlyout imports Icon) |
| Test backend `changed`-delivery unreliability during the producer conversions (the recorded mirror lore) | the lore lands in AnchorMirror once; journeys of record re-run per family, not batched |
| Golden churn from MenuShell's drop-shadow on panels that had it (flyouts) vs those that did not (menus — untouched per D-3) | cap-rows consumers are exactly today's shadowed panels; byte-identity expected and asserted |

## 7. Diff estimate

New: `menu-shell.slint` (~40), `menu-row.slint` (~140),
`menu_shell_probe_tests.rs` (~300). Edits: `action-menu.slint` −60/+40,
`dropdown-flyout.slint` −60/+25, `quick-list-flyout.slint` −35/+20,
`anchored-overlay.slint` +35, `app-window.slint` −70/+30,
`timers.slint` −18/+8, `task-view.slint` −30/+12,
`track-entries.slint` −30/+10, `board-bar.slint` −10/+6, census/docs
±40. Net production LOC ≈ −150; tests +~300. One PR, one branch
(`refactor/ui-menu-shell`), commits per §4 step.

## 8. Out-of-scope (filed/homed, not here)

Menu-mount host mode (D-6), DropdownTone unification (D-7), labels
roster convergence (D-5), #927 disarm, #1008 re-projection seam — see
§9 issue numbers at PR-open.
