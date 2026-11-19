# Round 9 Widget Framework / Structural UI Foundation — Research Report

- **Date**: 2026-11-18
- **Kind**: round-9 GUI-review research (audit + industry standards + Slint
  mapping + base-widget specs). No production code written.
- **Inputs**: operator verdict ("the app builds ad-hoc overlay/dialog widgets
  that violate industry-standard behavior"), the round-8 handoff
  (`.artifacts/reports/2026-11-18-round8-handoff-to-round9-analysis.md`),
  the focus-restoration machinery (PR #872, `MaskWalkFacts`/`FocusFacts`),
  and a full read of the UI crate's overlay/composition layer.
- **Slint version in scope**: `slint = 1.18.1` (root `Cargo.toml`, pinned with
  `unstable-winit-030`).

---

## 1. Audit: every ad-hoc overlay/dialog widget, current behavior × violation

All overlays live in `crates/taskboard-ui-slint/ui/app-window.slint` as
conditional siblings under `key-root` (the root FocusScope), mounted after the
chrome `VerticalLayout` (lines 446–777). The house "overlay grammar" is: a
full-window dismissal `TouchArea` (the "backdrop") declared *before* the panel
(the later sibling is hit-tested first), then the panel with hand-written
`x`/`y` clamp expressions. Six facts-globals (`DatePickerFacts`,
`TimerSetterFacts`, `DropdownFacts`, `TimeFieldFacts`, `TrackPanelFacts`,
`MenuPanelFacts`, `BoardBarFacts`) mirror anchor geometry from views the
window cannot name.

| Widget (file) | Opens / positions | Shifts underlying layout? | Click inside closes? | Tab focus escapes? | Sizing | Verdict vs standard |
|---|---|---|---|---|---|---|
| **Date/time picker, task details** (`compositions/date-picker.slint`, mount app-window:779–835) | Window-level overlay anchored to the due-editor field via `DatePickerFacts`; below-if-fits/else-above, clamped above status bar | The panel itself does not (it is a `key-root` sibling, not in a layout). The *editor* it belongs to does: the due editor's echo/error caption rows (`due-editor.slint:99–108`) and the task-view inline edit flows grow/shrink the detail column while the picker is open — the visible "layout moves under the overlay" effect. **Needs a screenshot journey to pin the exact frame the operator saw** (open question Q1) | **Yes — by fall-through.** The panel is a `Rectangle` with day-cell `TouchArea`s and a grid `FocusScope` but **no panel-level absorbing TouchArea**; a click on dead space (header row, weekday row, gaps) misses the panel and lands on the dismissal backdrop → closes | Partly: `grid-scope` FocusScope holds focus only while grid-focused; the default-open leaves the *field* focused in the underlying view; no `MaskWalkFacts` clamp on this overlay | `width: min(window−2m, max(preferred, 280·scale))` — floor wider than contents in narrow fields | Anchoring/flip/clamp is right; dismissal fall-through and no structural focus containment are violations |
| **Timer setter popover** (`compositions/timer-setter.slint`, mount :845–886) | Same grammar, anchored via `TimerSetterFacts` | No (key-root sibling) | **Yes — fall-through** (same missing absorb TouchArea; only `CtaButtonArea`, presets, fields absorb) | Contained *structurally*: one `setter-scope` FocusScope + `MaskWalkFacts` extent=1 degenerate clamp (US-2/W-2, D-W2) — the walk stays on the scope but is not per-control | `min-width: 300·scale`, content-height | Best-behaved of the set; still fall-through dismissal |
| **Repeat settings (recur editor)** (`compositions/recur-editor.slint`, mount :1489–1523) | Centered modal, mount bit `recur-editor-open` | No | **Yes — dead-space click lands on backdrop → `recur-editor-cancel-clicked()` discards the whole draft.** Worst instance: a miss inside the dialog silently destroys work | Contained: `panel-scope` FocusScope (line 93) | Fixed `width: min(window−2m, 560·scale)` regardless of content | Centered + focus OK; no visual scrim (backdrop is invisible), fixed width, destructive fall-through |
| **Add Label / Labels overlay** (`compositions/labels-overlay.slint`, mounts :972–1058, two sites) | Centered "modal reads over the whole detail surface" (own comment) | No | **Yes — fall-through** → `dismiss-requested()` (edit form state survives adapter-side, so milder than recur) | Contained: `panel-scope` FocusScope (line 135) | Capped `360·scale`, content height | Same trio of issues |
| **Search palette** (`compositions/search-palette.slint`, mount :1446–1536) | Centered panel, `palette-open` | No | Row TouchAreas absorb; dead space falls through → dismiss (acceptable for a palette — matches spotlight semantics, but accidental) | **Escapes**: no FocusScope, no mask clamp; the query `TextField` holds focus, but Tab from it walks into the *underlying view* (masked only if the adapter happens to arm `MaskWalkFacts` — it does not for the palette) | 560 cap, fixed-count rows | Focus escape is the violation |
| **Day-review overlay (review walk)** (`views/review-walk-overlay.slint`, mount :1550–1578) | Centered, `walk-model.open` | No | **Yes — confirmed.** `ReviewWalkOverlay` has **zero TouchAreas outside the verb buttons/candidate rows and zero FocusScope**; any click on the panel background tunnels to the scrim → `walk-scrim-clicked()` "steps the walk back one level" — the operator's exact complaint | **Escapes — confirmed.** No FocusScope, no mask: Tab from the armed TextField/date field walks into the dimmed-under? no — into the *live underlying view* | **Far larger than contents — confirmed.** `width: min(window−2m, 560·scale)` fixed; `rows-clip` has `vertical-stretch: 1` so the panel stretches to near-full-height even with 2 rows | The reference violation: all four defect classes at once |
| **Board dropdown (narrow layout)** (`widgets/dropdown-flyout.slint`, site 1 mount :1142–1166) | Window-level anchored via `DropdownFacts`, flip/clamp grammar | No | Rows absorb; dead space falls through (fine for a flyout) | Highlight/cursor is keyboard-driven via the action layer; no tab stops inside (acceptable) | `width: DropdownFacts.anchor-width` — the board-bar segment caps at `160·scale` (`board-bar.slint:276`), so the flyout is pinned narrow regardless of row content | Narrow-width violation; positioning grammar otherwise correct |
| **Context menus / ActionMenu** (`compositions/action-menu.slint`, mounts :1337–1429) | Anchored to hamburger or pointer point (`menu-pointer-anchored`), two-tier flyout to the LEFT edge, `available-height` cap + Flickable | No | Backdrop full-close; rows absorb; the panel root — `ActionMenu` builds rows as FocusScope+TouchArea pairs, dead space (header/padding) falls through → close (menu semantics: acceptable-ish) | Menu is a modal takeover at the action layer (KeyboardState); not tab-walked | floor 320·scale, cap 560·scale, `available-height` fill | Closest to standard (QMenu-like); dead-space close is defensible for menus |
| **Focus (concentration) overlay** (`compositions/focus-overlay.slint`, mount :1579–1600) | Centered | No | **Yes — fall-through** onto scrim → close (session keeps running, so cheap but still wrong) | `stop-focus` FocusScope only for the stop button; the note field and switcher rows are not walk-clamped (`MaskWalkFacts` not used here) | 560 cap | Same trio |
| **Quick capture** (`compositions/capture-overlay.slint`) | Centered | No | Fall-through → discard-and-close (destructive: typed text is discarded per the comment) | `MaskWalkFacts` extent=1 clamp — contained | 560 cap, content | Destructive fall-through |
| **Track-entries panel** (`compositions/track-entries.slint`, mount :895–962) | DatePicker grammar twin, anchored | No | Fall-through (form state adapter-held) | Panel walk per widget | 360 floor | Same trio |
| **Time-field quick list** (`widgets/quick-list-flyout.slint` + `TimeFieldFacts`, 4 sites) | Window-level anchored, own focus hand-off (`focus-nonce`) | No | Backdrop close; rows absorb | Flyout FocusScope grabs focus on open; not tab-reached by contract | anchor-width | This is the house flyout done right — the pattern to generalize |
| **Board color strip** (inline at :1200–1244) | Anchored below bar only (no flip) | No | Fall-through | No FocusScope | Fixed formula | Minor; no flip branch |

### The two mechanics behind every violation

1. **Click-inside-closes (fall-through dismissal).** In Slint, a plain
   `Rectangle` (or `FocusScope`) does not absorb pointer events. The house
   grammar's dismissal backdrop is *below* the panel in sibling order, so any
   click that misses an interactive child inside the panel falls through the
   panel's dead space onto the backdrop and dismisses/cancels/discards. Every
   panel above that lacks a full-surface absorbing `TouchArea` inside the
   panel root has this bug (day-review, recur, labels, focus, capture,
   date-picker, timer-setter). The industry law is the inverse: *clicks inside
   the overlay's bounds never close it; only outside clicks / Escape do.*
2. **No structural focus containment.** Slint has no native focus trap (see
   §3). The landed containment is **per-widget opt-in**: either a hand-rolled
   `FocusScope` + `MaskWalkFacts` extent=1 degenerate clamp (timer-setter,
   capture, focus-overlay) or nothing at all (review-walk, search palette,
   color strip). Tab focus therefore escapes into the still-interactive
   underlying view whenever a composer forgot the ritual. PR #872 built the
   machinery (`MaskWalkFacts`, `FocusFacts`, `mask_focus` seam, the adapter's
   walk arithmetic) but it is a per-host handshake, not a property of "being
   an overlay".

### The layout-shift mechanic (operator's time-picker complaint)

Window-level panels do not participate in layout (they are `key-root`
siblings with explicit `x`/`y`). The shifts the operator can observe come
from **conditionally-mounted elements inside layout trees**:

- the due editor's echo / error caption rows (`due-editor.slint:99–108`) —
  appearing/disappearing text rows reflow the detail column while the picker
  is open;
- the task-view inline edit flows (title/description/priority/effort rows
  that mount over their subject);
- the window-level-slot board create/rename `TextField`s that mount *inside*
  the chrome `VerticalLayout` (app-window:714–732) — an intended inline row,
  but the same mechanic;
- the `TimeField`'s invalid hint (`time-field.slint:177–182`, positioned at
  `y: field.natural-height` — overlaps rather than shifts, but grows the
  panel's preferred height inside its host).

The base-widget set must therefore carry a law, not just a component: **an
overlay may never be a conditional child of a layout; it mounts as a
window-level sibling, and nothing inside a layout may mount in response to an
overlay opening.** A screenshot journey pinning the exact frame the operator
saw is chunk 1's first task (Q1).

### Sizing violations summary

- Fixed caps regardless of content: 560 (recur, palette, walk, focus,
  capture), 360 (labels, track-entries), 280 floor (date-picker),
  anchor-width (dropdown flyouts — 160 cap inherited from the board bar).
- `ReviewWalkOverlay` additionally *stretches*: `vertical-stretch: 1` on the
  rows clip makes the dialog near-full-height with two rows.
- Industry: dialogs size to content, bounded by viewport (Qt `QDialog`
  `sizeHint`; web `dialog` shrink-wraps; popovers `max-height` to available
  space). Cap ≠ target width.

---

## 2. Industry-standards digest (with sources)

### Modal dialogs

- **Qt `QDialog`**: modal via `exec()`/`open()` blocks input to other windows
  in the application (`Qt::ApplicationModal`/`WindowModal`); `open()` shows
  window-modal without a nested event loop; default/escape-button wiring;
  dialogs size to their `sizeHint` (content). Qt widgets do not draw a scrim
  (platform-style effect), but QML's `Dialog`/`Overlay` does (dim + modal
  grab). Sources: [Qt 6 QDialog docs](https://doc.qt.io/qt-6/qdialog.html),
  [Qt 4.7 archive](https://doc.qt.io/Archives/qt-4.7/qdialog.html),
  [Qt Forum on stacking modals](https://forum.qt.io/topic/115715/how-to-make-a-modal-dialog-stay-on-top-of-an-existing-modal-dialog).
- **HTML `<dialog>` + `showModal()`**: the normative modern bundle — renders
  in the top layer, paints `::backdrop`, makes the rest of the document
  **inert** (unfocusable, unclickable), **traps focus** (first focusable
  descendant gets focus; Tab cycles inside), Escape closes (`cancel` event),
  dialog shrink-wraps to content. This is the WAI-ARIA APG "modal dialog
  pattern" implemented natively. Sources:
  [MDN `<dialog>`](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/dialog),
  [ARIA a11y/UX writeup](https://jaredcunha.com/blog/html-dialog-getting-accessibility-and-ux-right),
  [deep dive](https://medium.com/@beiselanja/stop-rebuilding-modals-a-deep-dive-into-the-dialog-element-4580cdbb7b20),
  [focus-trap discussion](https://stackoverflow.com/questions/44452084/how-to-keep-focus-within-modal-dialog).

**Derived modal laws**: centered; visible scrim/backdrop; size-to-content
bounded by viewport; focus enters first field and is trapped until close;
background inert (no clicks, no focus, no hover steal); Escape = cancel;
outside-click = cancel (explicitly *not* clicks inside); explicit
accept/dismiss buttons, accept on the entry side of the button row per
platform, Escape never bound to the destructive default.

### Anchored overlays / popovers

- **Popcorn standard (web)**: the Popover API gives top-layer rendering,
  **light dismiss** (outside click + Escape close an `popover=auto`), and the
  invoker button is the anchor; the CSS Anchor Positioning API provides
  `position-area` placement plus **`position-try-fallbacks: flip-block,
  flip-inline`** to flip near viewport edges and `position-visibility:
  anchors-visible`. Sources:
  [MDN Using the Popover API](https://developer.mozilla.org/en-US/docs/Web/API/Popover_API/Using),
  [Chrome anchor-positioning blog](https://developer.chrome.com/blog/anchor-positioning-api).
- **Qt `QMenu`/`QComboBox` popup**: popup grabs mouse+keyboard (popup grab),
  closes on any outside press, positions below the trigger and flips when the
  screen edge is near, sizes to contents up to screen bounds, and the parent
  widget keeps logical focus lineage (the popup is a focus *scope* of the
  parent window). `QProxyStyle`/focus-scope machinery is how composed widgets
  integrate into the parent's tab chain without escaping it.

**Derived popover laws**: anchored to the invoking widget; flip
below/above (and shrink) at viewport edges; light-dismiss on outside click
and Escape; **inside clicks never dismiss**; focus moves into the popover and
returns to the invoker on close.

### Focus-walk containment for composed widgets

- WAI-ARIA APG: while a modal is open, focus must not leave it (the
  "focus trap" is a hard requirement, 2.4.3/2.1.2 keyboard); the inert
  attribute is the structural implementation.
- Qt: every widget is part of the parent's tab chain by construction (the
  tab chain is a property of the widget tree, not of each widget's code).
- The web converged on "make the background inert" rather than "each dialog
  traps actively" — containment is a *structural* property of the overlay
  layer, exactly what the operator is asking for.

---

## 3. Slint capability mapping (v1.18.1)

What Slint gives natively vs what must be built:

| Need | Slint native | Verdict |
|---|---|---|
| Dialog widget | **None.** No Dialog element; the std-widgets set has no modal | Build `ModalDialog` in `.slint` |
| Top-layer popup | `PopupWindow` (child of any element, `show()`; `close-on-click` default true, `close-on-click: false` + `close()`; positioned near parent, historically clamped to window; `forward-focus` supported) | **Rejected by the project already** (round-5 D6 record): geometry is not reliably anchorable, close-on-click semantics are all-or-nothing, focus-loss on window deactivate closes silently (issues [#1143](https://github.com/slint-ui/slint/issues/1143), [#2911](https://github.com/slint-ui/slint/issues/2911)). The window-level conditional-mount grammar is the right call; keep it and factor it |
| Scrim | No native backdrop; draw a `Rectangle` with translucent color under the panel sibling | Build (one line, but must be in the shared component) |
| Focus trap | **None.** `FocusScope` + `focus-on-tab-navigation` + `focus()`/`clear-focus()`/`has-focus`; tab traversal visits focusable elements in tree order; no containment primitive ([FocusScope docs](https://docs.slint.dev/latest/docs/slint/guide/development/focus), traversal gaps e.g. [#10321](https://github.com/slint-ui/slint/issues/10321)) | Build on `MaskWalkFacts` (already landed, PR #872) — promote it from per-host opt-in to overlay-layer automatic |
| Anchored positioning | No anchor API. Hand-written `x`/`y` with `min`/`max` clamp expressions; `absolute-position` for realizing window-space geometry | Build `AnchoredOverlay` helper (the clamp expression is copy-pasted ~10 times in app-window already) |
| Size-to-content | `preferred-width`/`preferred-height` + `min()`/`max()` bounds | Build (caps must become *bounds*, not widths) |
| Inert background | No inert property; the dismissal TouchArea blocks clicks but the *underlying view stays focusable/hoverable* | The walk mask is the equivalent seam; hover/`has-hover` of covered chrome is still live (minor, note in doc) |
| Accessibility | `AccessibleRole.dialog` exists? — unverified for 1.18 (Q4); `accessible-*` properties otherwise fine | Verify in chunk 1 |
| Escape handling | The project's law: the root `key-root` capture + action layer owns keys; widget-internal Escape only for leaf flyouts (time-field) | Keep — the base widgets must not install key handlers (§5.1) |

Conclusion: everything the operator asked for is buildable as pure `.slint`
compositions over existing primitives, and the project has already invented
every ingredient once (window-level mounts, facts-globals, `MaskWalkFacts`,
focus-restore). The work is *deduplication into a base layer + laws*, not new
mechanics.

---

## 4. Proposed base-widget set

All live in `crates/taskboard-ui-slint/ui/widgets/` (or a new
`ui/overlays/`), presentation-only per AGENTS.md §1, callbacks-only intents
(§5.1), no key handlers. They plug into the existing machinery: mount sites
stay in `app-window.slint` (one per class, facts-global driven where anchors
live in views), focus containment rides `MaskWalkFacts` (PR #872), focus
restore rides `FocusFacts` + the `focus-*` window functions.

### (a) `ModalDialog` — the general centered dialog

```slint
component ModalDialog inherits Rectangle {
    // -- content --
    in property <string> title;              // heading; dialog is labelled by it
    in property <@children> content;         // the caller's body (default children)
    // -- buttons: extensible, ordered left→right --
    in property <[DialogButtonModel]> buttons; // { label, role: none/accept/dismiss/destructive, enabled, autofocus }
    callback button-invoked(int);            // index into `buttons`; adapter maps to verbs
    callback dismissed();                    // outside click / Escape (the adapter routes; widget never keys)
    // -- containment (automatic, not opt-in) --
    // init: MaskWalkFacts.extent = <ordered tab-stop count>;
    //       MaskWalkFacts.cursor = 0; focus first tab stop (accept default)
    // -- geometry --
    // width: min(parent.width - 2*margin, max(self.preferred-width, min-width-floor))
    // height: content (no stretch) capped at parent - margins
    // -- chrome (in the mount site, not per-dialog) --
    // scrim Rectangle (visible, translucent) + absorbing TouchArea → dismissed()
    // panel root TouchArea (absorbs all inside clicks — the dismissal law)
}
```

- Scrim + dismissal layer + absorbing panel root live in a `ModalDialogHost`
  wrapper mounted once per open dialog in `app-window.slint` — the dialog
  body never re-declares them.
- Buttons: large (the house `space-6` CTA height), arrayed left/right by role
  (accept cluster right, dismiss left — align with existing CtaButtonArea),
  extensible via the model; the accept button is the default focus target.
- Dismissal law enforced structurally: inside clicks hit the panel's own
  TouchArea and go nowhere.

### (b) `AnchoredOverlay` — the positioning helper

```slint
// Pure geometry: given an anchor rect (facts-global mirror) and the panel's
// realized size, compute the clamped, flipping x/y. Tested via the existing
// flip/clamp journey seam (the facts `panel-x/panel-y` observation pattern).
component AnchoredOverlay inherits Rectangle {
    in property <length> anchor-x; in property <length> anchor-y;
    in property <length> anchor-width; in property <length> anchor-height;
    in property <length> viewport-width;   // window width
    in property <length> viewport-bottom;  // status.y — the usable floor
    in property <Placement> placement: Placement.bottom; // bottom|top|point
    out property <length> computed-x; out property <length> computed-y;
    // law: prefer below (placement bottom); flip above when the panel does
    // not fit between anchor-bottom and viewport-bottom; clamp x/y into
    // margins; width = min(viewport - 2m, preferred) — caps are bounds.
}
```

- Replaces the ten copy-pasted clamp expressions in app-window
  (date-picker, timer-setter, track-entries, dropdown ×4, time-field ×4,
  menu, color strip).
- The QuickListFlyout/DropdownFlyout mount sites become one-liners.

### (c) Overlay focus containment — structural, not per-widget

- Law: **mounting through `ModalDialogHost` or `AnchoredOverlayHost` arms the
  walk mask automatically.** The host publishes `MaskWalkFacts.extent` from
  the mounted panel's declared tab-stop count (the panel declares
  `walk-stops` — an ordered list pattern the timer-setter already renders by
  hand), sets `cursor` on focus moves, and the adapter's existing walk
  arithmetic (US-2/W-2, D-W2) clamps Tab/arrow walks to the overlay. No
  composed or nested widget inside the overlay can opt out, and no new
  overlay can forget the ritual.
- Focus restore on close stays with the adapter's `FocusFacts` machinery
  (PR #872) — the host calls the existing `focus-*` window functions; nothing
  new.
- Underlying-view inertness beyond the walk (hover on covered chrome) is
  noted as a known limitation in the widget document (Slint has no inert
  property), with a backlog issue if it bites.

### (d) The dismissal law (encoded in the hosts)

1. Clicks inside the panel bounds never close — the panel root's absorbing
   TouchArea guarantees it structurally.
2. Outside click (left release or right down — the r4 D2 idiom) closes.
3. Escape closes via the action layer (the root capture tunnel), never a
   widget key handler (§5.1 unchanged).
4. Outside-click semantics are declared per host: `dismiss` (flyouts,
   palette), `cancel` (draft dialogs — must route the same seam as the cancel
   button, and drafts discard *only* through that seam), `step-back` (the
   review walk's ladder).

### Migration targets (audit → base widget)

- `ModalDialog`: recur editor, labels overlay (both mounts), review-walk,
  focus overlay, capture overlay, search palette (palette variant: no
  buttons, dismiss-on-outside).
- `AnchoredOverlay`: date-picker, timer-setter, track-entries, dropdown
  flyouts ×4, quick-list flyouts ×4, action menu (pointer + anchor modes),
  color strip (gains the flip branch).
- Sizing law fixes: review-walk loses `vertical-stretch: 1` (content height,
  ListView capped by available space); fixed 560/360 widths become caps over
  `preferred-width`; the dropdown flyout's width decouples from the 160·scale
  anchor cap (min of anchor-width and preferred, floored readable).

---

## 5. The "widget interaction document" outline

Location: `docs/widget_interactions.org` (new; AGENTS.md §7 keeps plans in
`.artifacts/`, but `docs/` is where living engineering law lives —
`docs/coding_guidelines.org`, `docs/ux_planning.org` precedent). It should be
normative and cited by AGENTS.md §1/§4 like the other docs. Outline:

1. **Purpose & scope** — every overlay/dialog/flyout/menu in the UI crate is
   an instance of a base widget in this document; ad-hoc overlays are
   forbidden (lint-by-review rule).
2. **Widget taxonomy** — modal dialog; anchored popover/flyout; context
   menu; palette (search); inline editor flow (the in-layout family, with its
   own law: layout shifts are *its* contract, everything else's is not);
   toast/notification; tooltip.
3. **Per-type interaction contract** — opening (anchor vs centered vs
   pointer), positioning + flip/clamp law, sizing law (content-bounded,
   caps-are-bounds), dismissal law (inside-safe, outside/Escape), focus law
   (enter target, trap via mask, restore target), keyboard grammar (which
   strokes the action layer binds per type), scrim/inertness expectations.
4. **Base implementations** — `ModalDialog`, `AnchoredOverlay`, hosts,
   `MaskWalkFacts` integration; file map; how to mount a new one (the
   checklist).
5. **Usage rules** — when a surface is a dialog vs a popover vs inline;
   destructive-outside-click prohibition; button roles/placement; the
   one-open-overlay exclusivity table (extend the existing flow exclusivity).
6. **Testing** — the journey patterns per type: flip/clamp observation via
   facts `panel-x/y`, dismissal fall-through probe (click dead space inside —
   must NOT close), walk-containment probe (Tab never lands under the mask),
   focus-restore probe (PR #872's suite extended).
7. **Known limitations & backlog** — no native inert, PopupWindow rejection
   record, hover on covered chrome.

---

## 6. Open questions for the operator

1. **Q1 — the exact layout-shift frame**: code reading shows window-level
   panels cannot shift layout; the candidates are the due-editor's echo/error
   rows, the task-view inline flows, and the in-layout board create/rename
   rows. A screenshot journey at chunk-1 start pins it. Is the operator's
   "time picker shifts layout" the detail-column reflow, or the picker panel
   itself resizing (280·scale floor)?
2. **Q2 — scrim visibility**: should modal dialogs get a visible translucent
   scrim (web/`::backdrop` standard) — a visual change to every modal — or
   keep the invisible dismissal layer? Recommendation: visible, tone from
   `Theme`.
3. **Q3 — outside-click on draft dialogs**: currently discard (recur,
   capture). Standard is cancel-by-outside-click; keeping the draft alive on
   outside click (dismiss = cancel-button seam without discarding) is safer.
   Which law?
4. **Q4 — `AccessibleRole.dialog`**: does Slint 1.18 expose a dialog role,
   and do we adopt `accessible-modal` semantics on the hosts? Verify during
   chunk 1.
5. **Q5 — search palette containment**: palette becomes a focus-trapped
   modal (Tab cycles rows) — confirm the desired grammar (spotlight-style
   Tab-into-list vs trap).
6. **Q6 — dropdown narrow width**: decouple flyout width from the 160·scale
   anchor cap — confirm the desired floor (readable row width).

## 7. Suggested chunk boundaries

1. **Chunk 1 — foundations**: `AnchoredOverlay` helper + `ModalDialog`/
   hosts + absorbing-panel + mask-arming; migrate one anchored family
   (dropdown + quick-list flyouts) and one modal (capture) as proofs; the
   screenshot journey pinning Q1; the dismissal/focus journey probes.
2. **Chunk 2 — modal migration**: recur, labels (both mounts), focus
   overlay, palette; sizing law (caps-are-bounds); review-walk loses stretch.
3. **Chunk 3 — anchored migration + menu**: date-picker, timer-setter,
   track-entries, color-strip flip, action menu pointer/anchor unify.
4. **Chunk 4 — the document**: `docs/widget_interactions.org` written from
   the landed law; census + AGENTS.md cross-references; the taxonomy table.

Each chunk is a UX-journey-plan-bearing dispatch chunk (AGENTS.md §7: new
user-visible capability — scrim, widths, dismissal semantics — needs an
accepted UX journey plan before dispatch).

---

## Sources

- Qt: [QDialog (Qt 6)](https://doc.qt.io/qt-6/qdialog.html), [QDialog (Qt 4.7 archive)](https://doc.qt.io/Archives/qt-4.7/qdialog.html), [modal stacking thread](https://forum.qt.io/topic/115715/how-to-make-a-modal-dialog-stay-on-top-of-an-existing-modal-dialog)
- Web dialogs: [MDN `<dialog>`](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/dialog), [a11y/UX practices](https://jaredcunha.com/blog/html-dialog-getting-accessibility-and-ux-right), [dialog deep dive](https://medium.com/@beiselanja/stop-rebuilding-modals-a-deep-dive-into-the-dialog-element-4580cdbb7b20), [focus containment SO](https://stackoverflow.com/questions/44452084/how-to-keep-focus-within-modal-dialog)
- Popovers/anchoring: [MDN Popover API](https://developer.mozilla.org/en-US/docs/Web/API/Popover_API/Using), [Chrome anchor positioning](https://developer.chrome.com/blog/anchor-positioning-api)
- Slint: [PopupWindow docs](https://docs.slint.dev/latest/docs/slint/builtin-elements/popupwindow), [focus handling guide](https://docs.slint.dev/latest/docs/slint/guide/development/focus), [issue #1143](https://github.com/slint-ui/slint/issues/1143), [issue #2911](https://github.com/slint-ui/slint/issues/2911), [issue #10321](https://github.com/slint-ui/slint/issues/10321)
