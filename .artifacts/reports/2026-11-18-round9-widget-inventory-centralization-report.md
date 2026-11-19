# Round 9 Whole-Widget Centralization Audit — Inventory, Taxonomy, Plan

- **Date**: 2026-11-18
- **Kind**: round-9 GUI-review research (extends
  `.artifacts/reports/2026-11-18-round9-widget-framework-research.md`
  from overlays/dialogs to ALL widget classes). Read-only audit + report;
  no production code written.
- **Operator mandate (2026-11-18)**: "The analysis of the current UI code
  regarding ad-hoc overlay and dialog widgets is extended to OTHER types
  of widgets, with the goal of systematically centralizing all our
  widget definitions, identifying what base widgets we need and
  implementing them not as special ad-hoc implementations but as general
  reusable widgets with customization points."
- **Inputs**: full sweep of `crates/taskboard-ui-slint/ui/` (17,759 lines
  of .slint across widgets/, compositions/, views/, app-window.slint),
  the J2 journey plan rulings R-1..R-7, the round-9 round plan (W1/W2),
  `docs/ux-surface-census.org`.
- **Slint version**: 1.18.1 (no Dialog/anchor APIs; PopupWindow stands
  rejected per R-1/round-5 D6).

All paths below are relative to `crates/taskboard-ui-slint/ui/` unless
absolute. Line numbers are from the working tree at audit time
(2026-11-18, main).

---

## 1. Full widget inventory

Legend: **C** = centralized under `ui/widgets/`; **A** = ad-hoc/inline in
a view or composition; counts are distinct implementations of the same
concept (not call sites).

| # | Widget class | Impls | Locations | Centralized? | Divergence notes |
|---|---|---|---|---|---|
| 1 | Single-line text field | 1 (+2 wrappers) | `widgets/text-field.slint:28` (TextField); `widgets/tipped-field.slint:29` (TippedField = field+InfoTip); `widgets/time-field.slint:61` (TimeField = typed date/time field + quick list) | C | Well-factored; one L1 exception class. TippedField/TimeField are legit specializations |
| 2 | Multi-line editor | 1 | `widgets/multi-line-editor.slint:38` | C | — |
| 3 | Icon button | 1 | `widgets/icon-button.slint:19` | C | Square space-6, FocusScope tab stop, Return/Space, tone ladder, tooltip arm. The house gold standard |
| 4 | **Text/label button** | **4** | `compositions/board-layout.slint:33` (CtaButton), `views/settings-view.slint:372` (SettingsButton), `views/review-walk-overlay.slint:86` (VerbButton), `compositions/timer-setter.slint:32` (CtaButtonArea) | **A** | Each re-declares FocusScope + Return/Space + TouchArea + a11y-default (~50–80 lines). Heights: space-6 / label+2·space-2 / space-5 / space-6·CTA. Backgrounds: raised / raised+disabled / surface+hover / CTA pair. Diverging disabled, focus-ring and min-width rules |
| 5 | **Checkbox** | **3** | task-view done-box `views/task-view.slint:402-503`; TaskCard done checkbox `widgets/task-card.slint` (r3 D6, ~40 lines); recur weekday toggles `compositions/recur-editor.slint:163-210` (7 check-icon IconButtons) | **A** (task-card's lives in a widget file but is card-private) | Three grammars: verbatim ActivationStop copy (45 lines, its own comment admits "keep the copies in sync"), card anatomy, tone-flipped IconButtons. All state-less by design (good) but with no shared primitive |
| 6 | Segmented control | 1 (+2 tab-strip cousins) | `widgets/segmented-control.slint:22`; cousins: BoardBar tab strip `compositions/board-bar.slint:78` (baseline hairline + segments), compact stack switcher `compositions/board-layout.slint` (~:930-1000, DropArea+FocusScope segments) | C (control) / A (cousins) | The two tab-strip cousins re-implement "one selected of N" with different chrome (underline vs raised fill) and drop-target wrappers |
| 7 | Dropdown (closed field + flyout) | 1 (+2 near-twin wrappers) | `widgets/dropdown.slint:114` (+ `DropdownFacts` global); `views/settings-view.slint:551` RemotePicker, `:600` DeckPublishPicker | C | The two pickers duplicate ~40 lines verbatim (facts mirroring + tooltip wiring), differing only in copy constants and flyout-site |
| 8 | Dropdown flyout panel | 1 | `widgets/dropdown-flyout.slint:18` (window-mounted) | C | Width pinned to anchor-width (160·scale cap inherited — prior report Q6/R-6) |
| 9 | Quick-list flyout | 1 | `widgets/quick-list-flyout.slint:16` (4 mount sites via TimeFieldFacts) | C | The house flyout done right |
| 10 | **Menu / menu rows** | 1 panel + ~5 row grammars | `compositions/action-menu.slint:44` (FocusScope+TouchArea row pairs, keyboard ladder); row grammars also in DropdownFlyout rows, QuickListFlyout rows, labels roster rows `compositions/labels-overlay.slint` (~:380-425), priority slot rows `views/task-view.slint:804-839` | Partly C | Five row grammars share the shape (hover surface, selected ring, index callback) but not the keyboard/a11y treatment; only ActionMenu has the full ladder |
| 11 | Date picker | 1 | `compositions/date-picker.slint:31` (window-mounted, grid FocusScope) | A (composition) | Prior report: fall-through dismissal, 280 floor |
| 12 | Timer setter | 1 | `compositions/timer-setter.slint:117` | A | Prior report: best-behaved anchored panel |
| 13 | Color picker / swatch row | 1 (+ strip) | labels-overlay swatch row (~:230-260); board color strip inline `app-window.slint:1200-1244` | A | No flip branch on the strip; swatch row bespoke |
| 14 | Priority picker | 1 | task-view inline slot list `views/task-view.slint:804-839` | **A** | Six-row quick list hand-inlined in the view; check-glyph Path duplicated from the checkbox |
| 15 | Modal dialogs | 6 mounts | recur, labels ×2, review-walk, focus, capture, palette (prior report §1) | **A** | Covered by prior research → ModalDialog |
| 16 | **Chips / badges** | **1 + 4 variants** | `widgets/badge.slint:8` (Badge, centralized); PresetChip `compositions/rule-list.slint:37`; label swatch chips `views/task-view.slint:952-981`; recur paused chip `views/task-view.slint:1103-1115`; status-bar chip family (sync/timers/tracking/files-sync, `compositions/status-bar.slint:70+`) | C (Badge) / A (variants) | Badge covers tone-tinted counts; the variants add: swatch square, removable ×, ActivationStop chip, mixed tone+icon — four one-off shapes with drifting min-heights (space-5 vs space-6) |
| 17 | **Label+value rows** | **10+ ad-hoc** | task-view: Stack :623-635, Due :636-707, Start :711-776, State :777-788, Importance :843-865, Urgency :866-889, Effort :898-937, Tracked :1009-1067, recur pattern echo :1093+, Description caption :1223+; StatCell `views/review-view.slint:135`; LabeledField `views/settings-view.slint:455` | **A** | Three edit-affordance grammars coexist: (a) pencil IconButton beside a mono echo, (b) clickable closed cell (ActivationStop + hover surface + tooltip arm, ~40 lines each), (c) inline editor swap. Value typography drifts (mono vs body); captions always re-declared |
| 18 | List rows | 1 | `widgets/list-row.slint:9` (text+meta+tone) | C | Consumers overlay their own TouchArea (habits, status-bar banner, notifications) — by design (Rule E) |
| 19 | Cards | 1 | `widgets/task-card.slint:92` | C | Board-specific but shared by board/stack/today surfaces |
| 20 | **Section headers** | **~5 hand-rolled** | timers `compositions/timers.slint` (~:150-183: caption + count caption + spacer + IconButtons), habits `compositions/habits.slint` (~:118-140, same shape), tracked-today, rule-list caption, review-view Aggregate/Tracking/Reflection sections (`views/review-view.slint:158/200/240`: heading + StatCell grid) | **A** | The "caption + count + stretch spacer + header actions" row is copy-pasted per section |
| 21 | **Armed-confirm strips** | **4** | board column delete strip (board-layout, `delete-armed`), recur convert strip `views/task-view.slint:1185-1205`, break-rule remove strip (timers), habit convert strip (habits) | **A** | Same consequence-sentence + confirm/cancel pair grammar, four inline implementations |
| 22 | **Ghost/opener affordances ("+")** | PlusGlyph C + **3 bespoke mounts** | `widgets/plus-glyph.slint` (C); add-task ghost `compositions/board-layout.slint` (~:752-800, bespoke FocusScope+TouchArea+glyph, ~45 lines); trailing add-column (~:847-880, same again); plus IconButton openers ×9 sites (habits:131,254,274; labels-overlay:155,406; time-field:216; task-view:983; board-layout:984; timers:176) | Glyph C / mounts A | Two full-height ghost shapes and nine row-height IconButton openers; issue #820 already tracks the affordance-shape mismatch (board create) |
| 23 | **Status lines / notices** | **5+** | ErrorLine `views/settings-view.slint:436`; quota error hint (habits, bare danger RoleText ~:230); due-editor echo/error rows `compositions/due-editor.slint:99-108`; TimeField invalid hint `widgets/time-field.slint:177-182`; status-bar notice lines + kiosk absence captions | **A** | Five shapes for "tone-coloured inline message under/above a field or section": boxed, bare, caption-row, positioned overlay, chip-line |
| 24 | Progress / streak indicators | 1 + inline | `widgets/stat-bars.slint:32` (C); habit streak is inline "🔥N" appended to meta (habits ListRow meta composition); slider `widgets/slider.slint:26` (C, settings) | Partly C | Streak has no widget (text emoji in a meta string) |
| 25 | Status dots | 1 + 3 local | `widgets/status-dot.slint:7` (C); settings RowDot :327, StatusDot :343, BusyDot :358 (pulsing) | C + A | The settings trio are private re-implementations keyed to local enums instead of Tone |
| 26 | Empty states | 1 | `widgets/empty-state.slint:7` | C | — |
| 27 | Tooltips / info tips | 1 + 1 | `widgets/tooltip.slint` (TooltipState global + window panel); `widgets/info-tip.slint:14` (inline "?" affordance) | C | — |
| 28 | Kbd hints | 1 | `widgets/kbd-hint.slint:9` | C | — |
| 29 | Scroll containers | 1 + ad-hoc Flickables | `widgets/listview.slint:7` (ListView wrapper, virtualization seam); raw Flickables in ActionMenu, settings view, review | Partly C | Raw Flickable use where the ListView seam is not needed is acceptable; no drift risk beyond scroll-key handling (tracked by #870) |
| 30 | Focus stops / activation cells | 1 + 1 verbatim copy | `widgets/activation-stop.slint:14` (C); task-view done-box re-states the grammar verbatim (:425-466, comment admits the copy) | C + A | The one sanctioned copy; a CheckBox primitive deletes it |
| 31 | Icon set | 1 | `widgets/icon.slint:73` (IconKind vocabulary gate) | C | — |
| 32 | Steppers (+/− value) | 0 dedicated | habits use chevron/plus IconButtons; no stepper anywhere | n/a | Not currently needed as a primitive (IconButton pairs suffice); note for taxonomy |
| 33 | Sliders | 1 | `widgets/slider.slint:26` | C | — |
| 34 | Rosters | 1 | board-bar archived roster (dropdown rows + indicator) | A (inside BoardBar) | Absorbed by MenuShell + ActionChip |
| 35 | Notification rows | 1 | `compositions/notification-surface.slint:34` (ListRow + IconButton column) | A (composition) | Fine as composition once chips/buttons centralize |
| 36 | Overlay hosts / facts globals | 7 globals | DatePickerFacts, TimerSetterFacts, DropdownFacts, TimeFieldFacts, TrackPanelFacts, MenuPanelFacts, BoardBarFacts (`menu-facts.slint`, `track-panel-facts.slint`, per-widget files) | A | The anchor-mirror `changed` idiom copy-pasted ~7× (issue #954) |

**Totals**: 24 widget classes have a centralized implementation in
`ui/widgets/` (23 files); **14 classes remain ad-hoc** (text buttons,
checkboxes, label+value rows, chips-variants, section headers,
armed-confirm strips, ghost openers, status notices, pickers,
menu-row grammars, tab-strip cousins, status dots (settings),
steppers(none), roster). The overlay family (15 mounts) is already
planned onto ModalDialog/AnchoredOverlay by the prior research + J2.

---

## 2. Duplication & divergence findings (evidence)

**F-1 — The button grammar is written eight times.** The block
"FocusScope overlay + focus-on-tab-navigation + AccessibleRole.button +
accessible-action-default + Return/Space key-pressed with the four
modifier guards" appears verbatim in: IconButton (icon-button.slint:52-77),
CtaButton (board-layout.slint:46-65), SettingsButton (settings-view.slint:391-415),
VerbButton (review-walk-overlay.slint:86-105), SegmentedControl segments
(segmented-control.slint:96-120), the task-view done-box (:425-466), the
add-task ghost and add-column (board-layout.slint ~:765, ~:870), and the
board-bar/switcher segments. Each copy has drifted: VerbButton has **no
tab stop** (plain TouchArea + a11y only), the ghosts declare FocusScope
*before* TouchArea to dodge the two-click bug (F8/r4 D4) while CtaButton
declares it after, SettingsButton adds click-through, the done-box adds
the whole EditingFacts latch. This is the single largest duplication in
the crate (~500 lines) and the largest behavioral-divergence risk.

**F-2 — Ten label+value rows, three edit grammars.** task-view.slint's
metadata block (lines 622-1067) hand-rolls every row. The Importance and
Urgency rows (:841-889) are near-identical twins (copy-paste with names
swapped); the Due and Start columns (:650-693 vs :717-760) likewise. The
"clickable closed cell" grammar (ActivationStop + hover Rectangle +
tooltip arm + RoleText) costs ~40 lines per instance and appears 5×
(title, due, start, description, tracked-entries). The Effort row uses
grammar (a) (pencil beside echo) while Due uses grammar (b) — the same
user gesture ("edit this field") behaves differently per row.

**F-3 — Five tone→colour ladders.** Badge (:31), ListRow (:29), StatusDot
(:14), DropdownFacts.tone-color (dropdown.slint:99), IconButton
icon-color (:82) each re-implement Tone→Semantic colour as a conditional
chain. Adding a Tone arm requires five synchronized edits (issue #953).

**F-4 — Chip shapes drift.** Badge (space-1 padding, badge typography)
vs PresetChip (space-5/6 min sizes, mono text, ActivationStop) vs label
swatch chips (task-view :952-981, caption text, hairline-bordered swatch)
vs the paused chip (task-view :1103-1115, plain surface rectangle). Four
one-off shapes for "small tinted pill carrying a fact".

**F-5 — The anchor-mirror idiom ×7.** Private derived absolute-position
properties + `changed` handlers writing a facts global: timers.slint:107-116,
task-view.slint:1060-1065, board-bar.slint:89-101, settings-view.slint:576-588
**and** 625-637 (RemotePicker/DeckPublishPicker are ~40-line verbatim
twins), plus the date-picker/time-field/dropdown sites inside their
widgets. AnchoredOverlay's host absorbs the idiom once (issue #954).

**F-6 — Header rows copy-pasted per section.** "caption RoleText +
secondary count caption + `Rectangle { horizontal-stretch: 1 }` spacer +
header IconButtons" appears in timers (~:150), habits (~:118),
tracked-today, and (variant) rule-list; the review view hand-rolls three
heading+StatCell sections (:158, :200, :240) that differ only in cell
lists.

**F-7 — Armed-confirm strips ×4.** Board delete, recur convert
(task-view:1185-1205), break-rule remove, habit convert — same
consequence caption + confirm(success)/cancel(neutral) IconButton pair,
four inline copies with different tone choices.

**F-8 — Checkbox anatomy ×3 plus a duplicated check Path.** The stroke
Path "M 2 6 L 5 9 L 10 3" is hand-declared in the done-box (:467-478),
the TaskCard checkbox, and the priority slot rows (:821-830). The
done-box re-states ActivationStop's publish/latch machinery verbatim
with a comment obligating manual sync (:418-424) — a known-bad copy.

**F-9 — Status-notice shapes ×5.** Boxed ErrorLine, bare danger caption,
echo rows, positioned hint, chip-line — no shared tone/copy/tone-shape
convention (see inventory #23).

**F-10 — Ghost openers diverge in shape and reachability.** Full-height
column ghosts (add-task, add-column) vs row IconButton openers; VerbButton
in the walk overlay has no keyboard path at all. Issue #820 already
records the operator-visible symptom (two affordance shapes for board
create).

---

## 3. Base-widget taxonomy

Layering: `theme.slint` tokens → **L1 primitives** (`ui/widgets/`) →
L2 compositions (`ui/compositions/`) → L3 views. Every widget below is
presentation-only, callbacks-only intents, no key handlers beyond the
sanctioned leaf raw-key half (Return/Space on buttons, ladder keys in
dropdown), per AGENTS §1/§4 and the prior report §5.1.

### 3.1 Already centralized (keep, no work beyond token hygiene)

TextField, TippedField, TimeField, MultiLineEditor, IconButton,
SegmentedControl, Dropdown (+DropdownFlyout, QuickListFlyout), Badge,
ListRow, TaskCard, EmptyState, StatusDot, KbdHint, PlusGlyph, Icon,
Tooltip, InfoTip, StatBars, Slider, ListView, ActivationStop.

### 3.2 New base widgets (the absorption set)

| Base widget | Customization points (properties/callbacks) | Absorbs | Migration risk |
|---|---|---|---|
| **TextButton** | `label`, `tone: Tone`, `emphasis: enum {normal, cta}`, `size: enum {compact(space-5), regular(space-6)}`, `enabled`, `click-through`, `clicked()` | CtaButton, SettingsButton, VerbButton (gains tab stop — behavior change, journey-test), CtaButtonArea, the J2 ModalDialog button model (US-4/W-1) | Low-med: VerbButton gaining keyboard focus alters the walk overlay's tab order; needs a journey probe |
| **CheckBox** | `checked: bool` (model-pushed, state-less), `label`, `description`, `hint-keys`, `tone` (success when checked), `toggled()`; shares ActivationStop internally | done-box (deletes the verbatim copy + check Path), TaskCard checkbox, weekday toggles (via ToggleStrip), priority-slot check glyph | Low: geometry-identical replacements; card keeps its compact variant via a `compact` flag |
| **ToggleStrip** | `options: [string]`, `checked-mask: [bool]`, `picked(int)` | recur-editor 7 weekday IconButtons (:163-210), date-picker/time-field quick presets if they grow | Low |
| **LabeledValueRow** | `label`, `value`, `value-role: Typography`, `tone`, `placeholder-em-dash`, `mode: enum {display, cell, editor}`, `trailing: @children` (pencil/clear affordances), `hint-keys`, `edit-asked()`, `value-hint` tooltip | task-view metadata rows ×10 (F-2), StatCell (as the read-only mode), review sections' cells | **Med-high**: three interaction grammars must be modeled; do mode-by-mode, display-mode first. Wire into W-3/W-4 (J2 US-3 already touches the description/effort rows) |
| **ActionChip** | `text`, `tone`, `swatch: color` (optional leading square), `removable: bool`, `selectable/checked`, `min-height`, `invoked()`, `removed()` | PresetChip, label chips, paused chip, status-bar chip family, toolbar inbox chip, files-sync chip | Low: Badge stays for pure count chips; ActionChip is the interactive/swatch tier |
| **SectionHeader** | `title`, `count: int`, `absence-hint: string`, `actions: @children` (IconButtons) | timers/habits/tracked-today headers, rule-list caption | Low |
| **StatusNotice** | `tone`, `text`, `shape: enum {inline, boxed, caption-row}`, `wrap` | ErrorLine, quota hint, echo rows, TimeField hint, absence captions, status-bar notice lines | Low-med: the positioned TimeField hint keeps its own geometry |
| **ConfirmStrip** | `message`, `confirm-label`, `cancel-label`, `destructive: bool`, `confirmed()`, `cancelled()` | armed strips ×4 (F-7) | Low |
| **GhostOpener** | `orientation: enum {row, column}`, `label`, `hint-keys`, `invoked()`; renders PlusGlyph or Icon | add-task ghost, add-column, plus IconButtons at 9 sites | Low-med: unifies affordance shape (helps #820) |
| **MenuShell + MenuRow** | MenuShell: `rows: [MenuRowModel]`, `highlight`, `available-height`, keyboard ladder (Up/Down/Return/Esc/Tab), `row-activated(int)`, `row-hovered(...)`; MenuRow: label, indicator glyph, tone, selected, busy, submenu marker | ActionMenu rows, DropdownFlyout rows, QuickListFlyout rows, labels roster, priority picker (F-1's ladder + inventory #10) | **Med**: the ladder must stay widget-internal per dropdown.slint's contract; migrate one consumer first (priority picker is the smallest) |
| **ModalDialog / AnchoredOverlay(+hosts)** | per prior report §4 (a)-(d); adds R-4 role-based dismissal, R-6 width floors/caps | the 15 overlay mounts, the 7 facts-mirror sites (host absorbs mirroring, #954) | Already chunked W1/W2 — this audit adds the inventory classes it should also carry (MenuShell renders inside AnchoredOverlay) |
| **Theme.tone-color(tone) / tone-fill(tone)** | pure functions on Semantic | the five ladders (F-3, #953) | Trivial; do first as the pattern-card |

Not needed now (recorded, not proposed): StepperField (no consumer
beyond icon pairs), Tabs (SegmentedControl + BoardBar cover it; unify
only if a third strip appears), Toast (notification surface exists),
color-picker dialog (swatch row suffices until asked).

---

## 4. Sizing conventions (operator Q6 / ruling R-6) — sourced defaults

Framework survey:

- **Minimum click/touch targets**: Material 3 — "at least 48 × 48dp"
  ([touch targets](https://m3.material.io/foundations/designing/structure));
  Apple HIG — "a hit region of at least 44×44 pt"
  ([Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons));
  WCAG 2.2 AA — 24×24 px floor (44 enhanced). Our IconButton square is
  space-6 = 32 px × base-scale: above WCAG AA, below the touch-platform
  bars. **Recommendation**: keep 32 px (space-6) as the pointer minimum —
  the kiosk is a passive display (no touch input in scope per AGENTS §1),
  and base-scale raises everything on high-density screens; document the
  44/48 upgrade path in `docs/widget_interactions.org` if touch becomes
  a target. Ghost column openers already exceed it (full height).
- **Control heights**: Qt derives per-control heights from QStyle
  `sizeFromContents` + font metrics
  ([QStyle](https://doc.qt.io/qt-6/qstyle.html)); Material button
  heights are 40dp regular / 32dp compact. **Recommendation**: TextButton
  regular = space-6 (32 px, current), compact = space-5 (24 px, matches
  VerbButton), CTA = space-6 with min-width 4·space-6 (current CtaButton).
- **Text fields**: single-line min-width = space-15 (96 px) — keep; it
  already follows the r2 D22 content-free floor rule. Settings
  LabeledField label column min space-8 (64 px) — keep.
- **Dropdowns/menus**: Qt QComboBox `sizeHint()` sizes to the widest
  item ([source](https://code.qt.io/cgit/qt.qtbase.git/tree/src/widgets/widgets/qcombobox.cpp));
  QMenu sizes to contents up to screen bounds
  ([forum](https://forum.qt.io/topic/133076/height-of-menu-item)).
  Material menus: min-width 112dp, max 280dp. **Recommendation**: flyout/
  menu min-width 200 px (readable row, fixes the 160-capped board
  flyout), max-width 360 px, row height space-6, cap visible rows at 6
  (current dropdown convention), panel max-height = available-height.
- **Dialogs**: Qt QDialog sizes to sizeHint bounded by the screen
  ([QDialog](https://doc.qt.io/qt-6/qdialog.html)); HTML `<dialog>`
  shrink-wraps; Material dialog min-width 280dp. **Recommendation**
  (encodes R-6): min-width 280 px, max-width 560 px, preferred =
  content; height = content capped at viewport − 2·space-4 (kills the
  review-walk stretch); never fixed caps.
- **Popovers/anchored**: web Popover/anchor-positioning convention —
  flip + clamp, shrink to available space
  ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Popover_API/Using),
  [Chrome](https://developer.chrome.com/blog/anchor-positioning-api)).
  Already the AnchoredOverlay law.

All values land as Theme token expressions (`Npx * base-scale`), never
bare pixels, per the token lint.

---

## 5. Centralization + migration plan

### Library structure

Stays `ui/widgets/` (one file per widget, current convention; no new
directory needed beyond the prior report's optional `ui/overlays/` for
the two hosts). Naming: PascalCase components, file-per-component, file
header comment states the layer + decision refs, as today. The theming/
token layer already exists (`theme.slint`); extend it with
`tone-color`/`tone-fill` pure functions and the R-6 size-convention
tokens (dialog-min/max-width, menu-min/max-width, row-height). A
naming/lint note goes into `docs/widget_interactions.org`: ad-hoc
re-declaration of the button/checkbox/row grammars outside `ui/widgets/`
is forbidden (lint-by-review, same status as the overlay rule).

### Migration order (lowest-risk absorptions first)

0. **Theme ladder functions** (F-3) — pure refactor, zero geometry
   change, deletes 5 duplications. Can ride any chunk.
1. **W1 (existing chunk)** — AnchoredOverlay + hosts + facts-mirror
   absorption (#954's mechanism lands here as hosts absorb mirroring).
2. **W2 (existing chunk)** — ModalDialog migrations + sizing law (R-6
   values from §4) + VerbButton's keyboard gap noticed by TextButton
   migration.
3. **New chunk W3 — button & checkbox unification**: TextButton (absorbs
   4 + the ModalDialog button model), CheckBox + ToggleStrip (deletes
   the done-box copy + check Paths + weekday row). Large line-count win
   (~700 lines), behavior changes are additive (keyboard gains).
4. **New chunk W4 — LabeledValueRow**: display-mode first (Stack/State/
   Tracked/StatCell), then cell-mode (Due/Start/Title/Description —
   coordinates with J2's R-7 DueEditor overlay migration), then editor
   mode (Effort; already partially W-4 in J2's wiring inventory).
5. **New chunk W5 — chips, headers, notices, strips**: ActionChip,
   SectionHeader, StatusNotice, ConfirmStrip, GhostOpener. Mechanical,
   many call sites.
6. **New chunk W6 — MenuShell**: unify menu-row grammars + priority
   picker onto the shell; Dropdown/QuickList flyouts render MenuRow.
7. **Doc chunk W7 (extends prior chunk 4)** — `docs/widget_interactions.org`
   grows from "overlay taxonomy" to the **full widget taxonomy**: per-
   class contract (intent surface, keyboard/a11y law, sizing tokens,
   anti-ad-hoc rule), the §4 sizing table, and the absorption map.
   Census + AGENTS cross-references.

Chunks W3–W6 are each UX-journey-plan-bearing (AGENTS §7: user-visible
capability changes — keyboard gains, unified affordance shapes). They
should be appended to the round-9 round plan as W-chunk extensions or
become follow-up dispatch rounds; the plan-amendments in §7 below are
the minimal J2 deltas, with W3–W6 proposed as new journey-plan sections
or a J2b plan.

---

## 6. Open questions for the operator

1. **Q-A — VerbButton keyboard**: the walk overlay's buttons have no tab
   stops today. TextButton grants them — confirm the walk's tab order
   may change (recommended) or whether the walk stays pointer-only.
2. **Q-B — Touch targets**: 32 px pointer-minimum is defensible for a
   passive kiosk; should the base widgets reserve a `touch-friendly`
   sizing mode (44/48) now, or is that backlog until touch hardware is
   a target?
3. **Q-C — Priority picker placement**: it is the smallest MenuShell
   consumer; migrating it moves it from in-flow rows to an anchored
   flyout (consistent with the picker family, changes the layout around
   the metadata block). Confirm the flyout placement.
4. **Q-D — Chip scope**: keep Badge (static counts) and ActionChip
   (interactive/swatch) as two primitives, or fold Badge into ActionChip
   with `interactive: false`? Two widgets is recommended (Rule E).
5. **Q-E — GhostOpener shape unification**: full-height column openers
   vs row IconButtons are visually different on purpose (board
   affordance identity). Unify the mechanism but keep both shapes —
   confirm.

## 7. Proposed amendments to J2's wiring inventory

- **W-1 amend**: ModalDialog's button model is rendered by **TextButton**
  (W3 dependency noted; if W1 lands first, the button model renders
  CtaButtonArea-style rows and swaps to TextButton in W3).
- **W-2 amend**: AnchoredOverlayHost also absorbs the per-site facts
  mirroring (the `changed` idiom) — the host publishes anchor geometry;
  settings' RemotePicker/DeckPublishPicker collapse to copy constants
  (issue #954).
- **W-3 amend (US-3)**: the description/effort rows migrate onto
  **LabeledValueRow** (mode `editor`), so the "edit-button swap machinery
  deleted" acceptance becomes "LabeledValueRow mode swap".
- **W-4 amend**: the effort row is LabeledValueRow mode `editor` with a
  `trailing` clear affordance.
- **W-5 amend**: migrated mounts include the priority picker's MenuShell
  placement if Q-C is approved (else it stays W6).
- **New W-6/W-7 rows** for MenuShell journeys and the doc chunk's
  taxonomy extension, if the round plan adopts chunks W3–W6.

## Backlog issues opened by this audit

- #950 — no text Button primitive (four bespoke implementations)
- #951 — task-view metadata rows: ten hand-rolled label+value variants
- #952 — Tone→colour ladder duplicated across five widgets
- #953 — checkbox grammar hand-rolled three ways, no CheckBox primitive
- #954 — anchor-facts changed-handler mirroring duplicated at ~7 sites

(Pre-existing related: #762 Dropdown-in-modal coupling, #820 board-create
affordance shapes, #870 menu panel key scope, #934 habit-row keyboard
traversal, #880 effort-row Escape rung.)

## Sources

- Material 3 touch targets: https://m3.material.io/foundations/designing/structure
- Apple HIG Buttons (44×44 pt): https://developer.apple.com/design/human-interface-guidelines/buttons
- Qt QStyle pixel metrics: https://doc.qt.io/qt-6/qstyle.html
- Qt QDialog: https://doc.qt.io/qt-6/qdialog.html
- QComboBox sizeHint source: https://code.qt.io/cgit/qt.qtbase.git/tree/src/widgets/widgets/qcombobox.cpp
- QMenu item sizing: https://forum.qt.io/topic/133076/height-of-menu-item
- MDN Popover API: https://developer.mozilla.org/en-US/docs/Web/API/Popover_API/Using
- Chrome anchor positioning: https://developer.chrome.com/blog/anchor-positioning-api
- MDN `<dialog>`: https://developer.mozilla.org/en-US/docs/Web/HTML/Element/dialog
