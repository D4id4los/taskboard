# [Plan] GUI Review Round 5, Chunk 3 — Anchored Flyouts (F8/F9/F10 + the F14 dropdown half, D6)

- **Date**: 2026-11-06
- **Status**: STAGE-1 PLAN (dispatch protocol stage 1) — written for the
  stage-2 implementation agent; no code in this commit.
- **Origin (normative)**:
  `.artifacts/plans/2026-11-05-gui-review-round5-plan.md` §2 rows F8–F10
  and F14, §3 D6, §4.3; dispatch plan
  `.artifacts/plans/2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1 (shared execution model) and §4 (this chunk's covers/acceptance and
  prompts 3.1–3.3).
- **Branch / worktree**: `refactor/ui-slint-anchored-flyouts` in
  `../taskboard-WT-refactor-ui-slint-anchored-flyouts`, branched off
  `main` @ `8ea54d77` (worktree-doctor passed: 169G free, floor 25G).
- **Line-number drift note**: the dispatch prompt cites adapter/widget
  line numbers verified at `main @ 58ae9c55`; `main` has moved. Every
  anchor below was re-verified against the worktree tree
  (`8ea54d77`) and is the number stage 2 should trust.

---

## 1. Scope

Convert the `TimeField` quick-list flyout and the shared `Dropdown`
flyout to the **window-anchored overlay pattern** (D6): facts-global
geometry mirroring, window-level conditional mounts with the
date-picker's clamp/flip grammar, and a window-level dismissal underlay.
The displaced-underlay code in both widgets is **deleted, not repaired**
(D6 rules the overlay pattern; the coordinate-space defect F9 is
subsumed). On top of the conversion:

- **F8**: the quick-times chevron and the due-editor calendar toggle
  route through adapter seams that set the `due_focus_steal` latch, so
  the programmatic focus steal (the `IconButton` click grab,
  `icon-button.slint:103-107`) no longer settles the armed editor
  through `settle_date_editor`.
- **F10**: the quick-list filter seeds unfiltered at flyout open on all
  three `TimeField` surfaces (due picker, timer setter, track-entries
  form); typed-digit narrowing per keystroke is unchanged.
- **F14 (dropdown half)**: the flyout escapes the settings
  `Flickable`'s clip (window-level mount), and the empty-rows state
  becomes a visible disabled field with a consumer-owned caption
  instead of a dead control. The keyboard-open gate on non-empty rows
  stays.

### Non-goals (dispatch §4)

- No keymap/chain/intercept changes (chunk 7 owns the context stack and
  the ladder retirements). The `on_intercept_key` decline arm
  (`adapter.rs:5369-5391`) keeps its current shape and inputs.
- No settings-connections redesign (chunk 8) — the board picker keeps
  its rows model and seams; only the flyout's render location moves.
- No DatePicker overlay changes beyond its embedded `TimeField`'s
  flyout relocation (the picker panel, its grid, its ladder: untouched).
- No chunk-4 focus work (`SegmentedControl`/`Slider`/stop grammar).

## 2. Defect set, verified against the worktree tree

| Finding | Verified root cause | Anchor (worktree tree) |
|---|---|---|
| F8 | The chevron toggles widget-local `open` only; every adapter-routed picker interaction sets `due_focus_steal`, the chevron does not — the `IconButton` click grab steals focus from the armed date field and `settle_date_editor` ends the whole `EditDate` flow. Down-first hides it (the field already lost focus — no focus-lost fires). | `time-field.slint:126-133`; latch set `adapter.rs:647, 1891, 7071`; consumption `adapter.rs:7187-7190` |
| F9 | The flyout's underlay is displaced (`x: -anchor.absolute-position`) so underlay-local origin = window origin, but the panel's `x: 0; y: row-height` is authored in `TimeField`/`Dropdown`-local space — the menu renders wherever the host container happens to place it, clipped by any ancestor viewport. | `time-field.slint:142-246`; `dropdown.slint:250-375` |
| F10 | The quick list is filtered through the field's **value mirror** at all times: `filter_digits("14:30")` matches exactly one slot, so opening with a set time seeds a one-row list. The filter exists for typed-digit narrowing; open-time seeding is the bug. | `adapter.rs:1164-1172` (due), `:1318-1323` (setter), `:1428-1436` (track form); pure predicate `due_edit.rs:299-323` |
| F14 (this chunk's half) | The settings dropdown's flyout is clipped by the settings `Flickable` viewport; keyboard open is gated on `rows.length > 0` with no visible empty state (zero-height flyout, dead control). | `dropdown.slint:136`, `:180`, `:251-257`, `:258-375`; mount `settings-view.slint:406-470` |

## 3. Architecture decisions (the settling points)

### D-3.1 — One facts global per widget class (picked), with a site discriminator

**Decision**: two new globals, one per widget class, each defined in its
widget's own file (the `DropdownLadder` precedent of a global living
beside its widget):

- `TimeFieldFacts` in `ui/widgets/time-field.slint`:
  `open: bool`, `site: int`, `anchor-x/y/width/height: length`,
  `panel-x/panel-y: length` (realized panel geometry, the
  `MenuPanelFacts.flyout-*` precedent — the flip/clamp journeys'
  observation seam).
- `DropdownFacts` in `ui/widgets/dropdown.slint`: the same six fields
  **plus** `highlight: int` and `seek-nonce: int` (the dropdown's ladder
  stays in the widget's field scope, so the window-level panel must
  read the ladder's state through the global; the `TimeField` ladder
  moves into the panel, so its highlight/seek become panel-local and
  are **not** globalled).

Each widget gains `in property <int> flyout-site` — a mount-site
constant the mount passes (`TimeField`: 0 = due picker, 1 = timer
setter, 2 = track-form start, 3 = track-form end; `Dropdown`: 0 =
settings board picker, 1 = board-bar switcher). The discriminator is
what lets one global serve several mounted instances (the track form
mounts **two** `TimeField`s simultaneously — without it the mirrors
contest) and lets the window mount one panel per surface conditioned on
`Facts.open && Facts.site == K`.

**Rejected alternatives** (recorded so nobody re-derives):

- *One shared `AnchoredFlyoutFacts` for both classes*: the classes'
  payloads differ (rows vs slots; highlight ownership differs — see
  above); a shared global couples their close guards and buys nothing,
  since a dropdown and a quick list can never be open simultaneously
  (the window-level underlay enforces one-flyout-at-a-time per class,
  and the two classes never co-exist in one view's surfaces).
- *Per-mount-site globals* (the `SettingsBoardFacts`/`BoardBarFacts`
  precedent): six globals and six window panels with duplicated
  mirroring boilerplate; the decline arm would grow from two global
  reads to as many as six. Rejected for fan-out.

The existing per-site observation globals (`SettingsBoardFacts`,
`BoardBarFacts`) are **kept untouched** — the r4 journeys read them
(`journeys.rs:12365`, `:9389` area) and the decline arm keys on their
`open` flags (`adapter.rs:5380-5381`). They remain mirrors of the same
widget-local `open`; the new class globals are the *mount-truth*,
the old ones stay the *journey seams*. No double-truth risk: both are
written from the same widget property.

### D-3.2 — Where the window mounts live and the dismissal ordering

All new mounts go in `ui/app-window.slint` as two overlay groups
inserted **after the track-entries anchored mount** (ends `:727`) and
**before the menu group** (`:729`), in this sibling order:

1. `DropdownFacts.open` dismissal underlay (full-window `TouchArea`, no
   visual, clicked ⇒ `DropdownFacts.open = false`), then the two
   `DropdownFlyout` panel mounts (`Facts.open && Facts.site == K`).
2. `TimeFieldFacts.open` dismissal underlay (clicked ⇒
   `TimeFieldFacts.open = false`), then the four `QuickListFlyout`
   panel mounts.

Rationale, recorded:

- **Underlay before panel** — the later sibling is hit-tested first
  (probe P1's rule), so the panel absorbs its own clicks and the
  underlay receives exactly the outside clicks.
- **After the date-picker/timer-setter/track-entries groups** — the
  quick list's hosts are those panels' contents (the picker's embedded
  `TimeField`, the setter popover, the track form). Mounting above them
  means: with the picker open and the quick list opened from it, a click
  anywhere dismisses **only the quick list** (F8's "menu appears, picker
  stays open" acceptance), and a second outside click dismisses the
  picker — the standard stacked-overlay behavior, same as the menu's
  underlay stacking.
- **Before the menu group** — the menu is a modal takeover no dropdown
  or quick list can be open under; the relative order to the menu is
  therefore unobservable, and this slot keeps all flyout layers
  adjacent to the overlay family they belong to.
- Right-click dismissal rides the underlays too (the r4 D2
  `pointer-event` down-half idiom, same as every overlay underlay).

Panel geometry is the **date-picker mount's exact expression shape**
(`app-window.slint:575-588`): below the anchor when the panel fits
between the anchor and the status bar, else flipped above, both clamped
into the window margins. This is a deliberate upgrade for the dropdown,
which previously only rendered below — the flip is part of D6's
"window-level mount with clamp/flip" grammar. Width: the anchor width
(`Facts.anchor-width`) for both classes — unchanged from the in-widget
panels.

The two new panel components live in new files:
`ui/widgets/quick-list-flyout.slint` (`QuickListFlyout` — the
`TimeField` ladder's new home: FocusScope grab, Up/Down/Return/Escape/
Tab arms, the digit-forwarding shim, `ListView`, highlight/seek
panel-local re-anchored from `nearest-row` on mount) and
`ui/widgets/dropdown-flyout.slint` (`DropdownFlyout` — the row rendering
moved verbatim from `dropdown.slint:287-374`: busy dot, indicator/tone,
hover-union background, right-click context, `ListView` reading
`DropdownFacts.highlight`/`seek-nonce`). SPDX `AGPL-3.0-only` headers
per AGENTS §6 (UI crate).

Both panels mirror their **realized** `x`/`y` into their class global's
`panel-x`/`panel-y` (derived-property `changed` hooks — the P2 wrap
idiom; foreign-element `changed` does not deliver). The flip/clamp
journeys assert against these, exactly like the menu fly-out journeys
assert `MenuPanelFacts.flyout-*` (`journeys.rs:745-754`).

### D-3.3 — The widget-local `open` stays the driver; geometry moves window-side

`TimeField.open` / `Dropdown.open` remain the widgets' own `in-out`
state — every existing internal writer keeps working (chevron toggle,
pick arms, Tab arms, the dropdown's walk-away focus-loss seam
`dropdown.slint:192-196`, its `close()` refocus, its
`anchor-highlight`). The window learns the state through a **two-way
mirror loop**, both directions guarded by the site discriminator:

- *Widget → window (open)*: a derived-property wrap
  `property <bool> open-mirrored: root.open;` with
  `changed open-mirrored => { Facts.site = root.flyout-site;
  Facts.open = self.open-mirrored; }` (the `SettingsBoardFacts`
  mirroring idiom, moved inside the widget so all six mount sites get
  it for free).
- *Window → widget (close)*: a guard wrap
  `property <bool> fact-open: Facts.open && Facts.site == root.flyout-site;`
  with `changed fact-open => { if (!self.fact-open && root.open) {
  root.close(); } }` (`Dropdown` — `close()` refocuses the field before
  the unmount, the recorded walk contract) / `{ root.open = false; }`
  (`TimeField`).

The loop converges: window-side closes write `Facts.open = false`, the
widget's guard closes the widget, the widget's `changed open` writes
`Facts.open = false` again (no-op). A closed sibling instance's guard
is false (`site` mismatch or `open` already false), so no spurious
closes. Anchor geometry mirrors one-way from the widget's `flyout-anchor`
rect via `absolute-position` (window-space by definition — this is the
F9 fix; the settings `Flickable` scroll keeps the mirror live because
`absolute-position` tracks it).

`QuickListFlyout`'s focus grab: the panel grabs in `init` **and** on a
dedicated adapter-bumped nonce (`changed nonce => scope.focus()`) — the
TimerSetter nonce precedent (`timer-setter.slint:132-137`), because a
fresh conditional's init grab alone does not reliably take (the P5
record). Three new pushed integers: `date-quick-nonce` (task model),
`timer-setter-quick-nonce`, `track-form-quick-nonce` (window
properties), bumped by the corresponding quick-toggled seams (D-3.4).
The shared nonce per surface group is safe: only one panel mounts per
group at a time (`Facts.site` picks which).

Post-close focus restoration: after a panel close the focus owner
unmounts; today's in-widget version has the same shape (the flyout
scope unmounts) and no journey pins the landing spot. The conversion
preserves the fallback behavior; **if** the new journeys show the
keyboard dead-ending after close, the recorded contingency is a tiny
`public function grab()` on `text-field.slint`'s `TextField`
(delegating to its internal scope) called from the widget's guard arm —
decided by the red/green of the new journeys, not speculatively.

### D-3.4 — The two latch seams and the exact steal-latch contract

**Seam 1 — the quick-times chevron.** `TimeField` grows
`callback quick-toggled()` fired from the chevron's `clicked` body
alongside the local toggle (the toggle stays widget-local per D-3.3;
the callback is the notification half). Forwarding chain:

| Composition | New callback | Window callback | Adapter seam body |
|---|---|---|---|
| `DatePicker` | `quick-toggled()` | `task-date-quick-toggled()` | `due_focus_steal = true; time_filter_armed = false; refresh_forced` |
| `TimerSetter` | `quick-toggled()` | `timer-setter-quick-toggled()` | `setter_filter_armed = false; refresh_forced` |
| `TrackEntriesPanel` | `start-quick-toggled()` / `end-quick-toggled()` | `track-form-start-quick-toggled()` / `track-form-end-quick-toggled()` | disarm the field's flag + `refresh_forced` |

**Seam 2 — the due-editor calendar toggle.** The existing
`on_task_date_picker_toggled` seam (`adapter.rs:6895-6902`) gains the
latch set in **both** arms: today the calendar `IconButton`'s click
grab can settle the armed editor before/around the toggle arrives,
ending the flow the toggle is supposed to serve. The open arm
additionally requests the **field-focus hand-over**
(`FocusHandover::Field` — the push's `field_focus` flag, the same
mechanism `apply_picker_key`'s `K::Commit` uses) so the button's grab
doesn't strand focus on the button: the default-open contract is
"visible, click-armed, idle, the text field keeps focus".

**The exact steal-latch contract** (extends r4 D24, unchanged in
mechanism): `due_focus_steal` is a one-shot skip consumed by
`settle_date_editor` (`adapter.rs:7187-7190`) — a programmatic focus
steal caused by a flow-owning surface's control must not settle the
buffer the user is mid-way through. Every seam whose interaction can
steal focus from the armed field sets the latch **in the clicked
handler**, relying on the delivery order the month steppers already
prove (their latch set at `adapter.rs:7071` survives the same
`IconButton` grab — the r4 D24 journeys are the pin): the clicked seam
runs before the deferred focus-lost settle. The latch is consumed by
the first settle and never lingers: a subsequent genuine focus loss
settles normally. The setter and track surfaces have no settle funnel,
so their seams carry only the filter disarm — the asymmetry is
recorded, not an omission.

The **mount-inventory interplay** (chunk 1's test,
`tests/view_mount_inventory.rs`, landed on `main`): widgets are out of
its boundary, so `TimeField.quick-toggled` needs no exemption — but
`DatePicker`/`TimerSetter`/`TrackEntriesPanel` are covered compositions,
so each new callback **must** be forwarded at its `app-window.slint`
mount or the inventory fails. The forwarding lines are part of this
chunk's markup diff.

### D-3.5 — The open-time filter reset: the typed-filter vs the value mirror

**Decision**: the **value mirror stays the filter's input**; a
per-surface **armed flag** gates it.

- New adapter fields: `time_filter_armed`, `setter_filter_armed`,
  `track_start_filter_armed`, `track_end_filter_armed` (bools, default
  false).
- The three push sites keep their exact filter expression but gated:
  `let digits = if armed { filter_digits(mirror) } else { String::new() }`
  — empty digits match everything (`quick_list_matches`,
  `due_edit.rs:308-315`), so disarmed = the full 96-slot list.
- **Disarm** at the quick-toggled seams (D-3.4 — "open" is the only
  reachable direction: while the flyout is open the chevron is behind
  the window-level underlay) and, for hygiene, wherever the mirrors
  reset (the settle/commit funnel, `clear_timer_setter`, the track-form
  wipe — the "buffers never outlive their surface" invariant's new
  passenger).
- **Arm** at the surfaces' `time-edited` seams (`adapter.rs:6935-6954`
  due; the setter's `:7422`; the track-form start/end edited seams) —
  the first keystroke re-enables per-keystroke narrowing, whose
  semantics (filter = digits of the whole mirror text, echo-prefix
  strip included) are **byte-identical to today** from the first
  keystroke onward.

Recorded rejection: a *separate filter-string state* (digits typed
since open, independent of the mirror) is behaviorally identical — the
first keystroke's filter input is the seeded buffer plus the typed
digit either way — while duplicating the single-writer mirror rule
across four new strings. The flag design is the smaller diff and keeps
`filter_digits(&mirror)` the one filter source.

### D-3.6 — The empty-rows visible state and its copy source

When `rows.length == 0`, the `Dropdown` renders a **disabled** closed
field with a caption below it (the `TimeField` error-hint line's shape):
`field-touch.enabled: false`, `field-scope.enabled: false` (the walk
legitimately skips a disabled control — it is non-interactive), the
existing open gates (`dropdown.slint:136`, `:178-183`) keep their
`rows.length > 0` check and become unreachable-by-construction in the
empty state. This fixes F14's "dead control" half the way the round
plan §4.3 words it: the state becomes *visible*.

**Copy source**: consumer-owned, per the house rule that presentation
copy lives with the consumer and is never asserted by tests. `Dropdown`
gains `in property <string> empty-hint: ""` (empty = no caption, so the
board bar — never empty in dropdown mode — passes nothing and renders
byte-identical). The settings board picker passes a new
`SettingsBoardCopy.empty-hint` constant (`settings-view.slint:394-401`)
with wording in the "no local boards to connect yet" family — stage 2
owns the exact sentence.

### D-3.7 — The D6 override note (replaces the `dropdown.slint:250-257` decision)

The frugality comment block at the head of the dropdown's flyout
("no window-level overlay, no dismissal-layer interaction" — r4 D39's
recorded decision) is **deleted with the code it describes** and the
override is recorded where the grammar now lives. The replacement
comment, at the top of the new overlay group in `app-window.slint` (and
echoed in one line at the `Dropdown` widget's flyout-anchor):

> The dropdown/quick-list flyouts mount window-level (round-5 D6,
> 2026-11-05): this **overrides the r4 D39 "no window overlay"
> decision** that lived at the old in-widget flyout — the
> coordinate-space defect (F9: panel-local geometry under a displaced
> underlay) and the settings `Flickable` clip (F14) are subsumed by the
> date-picker overlay grammar (facts-global geometry, window-level
> mount, dismissal underlay).

### D-3.8 — Ladder preservation and the coarse-edge boundary

Both ladders move **verbatim**; no arm changes semantics:

- The `Dropdown` ladder never moves: it lives in the widget's
  `field-scope` (the field keeps focus — no grab), and its journeys
  drive it through `raw_key` + `SettingsBoardFacts`/`BoardBarFacts`
  (`journeys.rs:12303-12420`, the settings selector's matrix
  ~`:8877-9430`). Only the passive row rendering relocates.
- The `TimeField` ladder moves with the panel (it lives in the flyout
  scope, which grabs focus). Its Escape/Return arms are, today and
  after the conversion, shadowed in every context by an earlier
  consumer (the resolver's insert-mode Escape → the EditDate phase
  ladder; the intercept's timer-setter Escape at `adapter.rs:625`; the
  `TrackEntries` chain's dismiss) — that coarseness is **pinned
  today's behavior** and is explicitly chunk 7's territory (the
  printable monopoly + scope stack will hand Escape to the open
  flyout). This chunk neither fixes nor pins it deeper; the arms move
  so chunk 7 inherits them in the right place.
- No new decline arms: the quick-list ladder never had one (the
  ViInsert chains leave its keys Unhandled); the conversion does not
  change the chain, the focused element's *kind*, or any
  `EditingFacts` input, so the decline's inputs are untouched.

## 4. Implementation order (TDD)

Stage 2 implements in this order; every commit standalone
(rebase-merge hygiene). No sleeps; typed/structural assertions only —
geometry via the facts globals, list content via pushed model row
counts, states via model/facts fields; **never** label or caption text.

1. **The two red journeys** (they fail on `main` — that red is F8/F10
   formalized):
   - `the_quick_time_button_opens_the_menu_without_closing_the_picker`:
     fixture task with a **timed** duedate (e.g.
     `local_wall(2030, 1, 31, 14, 30)` — the timed value is what makes
     both halves red); mount, click "Edit due date", click the
     "Quick times" chevron by label; assert `date-picker-open` still
     true, `TimeFieldFacts.open` true, panel geometry below the anchor
     (`panel-y == anchor-y + anchor-height + Theme.space-1` via the
     facts), and the pushed `date-time-slots` row count == 96 (full
     list at open — the F10 half).
   - `the_quick_list_narrows_per_keystroke_after_an_open_with_a_set_time`:
     continue (or re-mount), drive `task-date-time-edited("9")`; assert
     the slots model narrowed (0 < count < 96) — typed narrowing
     survives the open-time reset.
2. **The conversion, widget by widget** — `Dropdown` first (its
   journeys are the denser pins): facts global + mirroring + site
   property, flyout block deletion, `DropdownFlyout` extraction,
   app-window overlay group + panel mounts wired to the existing
   window seams (`on_settings_board_selected` `adapter.rs:8044`,
   `board-bar-segment-activated` `app-window.slint:980`,
   `SettingsModel.board-options`, the bar's model property), per-surface
   tooltip copy bound as panel properties (the panel owns the
   `TooltipState` call; copy arrives via properties — consumer-owned).
   The bar-ladder and settings-selector journeys stay green throughout.
3. **`TimeField`**: same conversion + the ladder's relocation into
   `QuickListFlyout` (verbatim arms), the four site constants, the
   quick-toggled callback + forwarding lines (composition callbacks +
   app-window forwards — the inventory test enforces them), the focus
   nonces.
4. **The latch seams + filter flags** (D-3.4/D-3.5) — the two red
   journeys turn green here; the calendar-toggle latch lands with its
   own journey extension (the toggle opens the picker with the editor
   still armed and the field re-focused).
5. **The empty-rows state** + its journey (zero-board settings fixture:
   the picker's field renders structurally disabled and the caption
   element exists; Return/Space dispatch nothing — `drain_requests`
   empty).
6. **The per-mount-site geometry matrix** (the dispatch acceptance's
   journey-per-site): settings picker, board-bar switcher,
   timer-setter, track-entries start **and** end — each asserts open,
   panel below anchor (or the flip case for whichever site the harness
   can drive under a small window: the track-form fields sit low in a
   tall panel, so the flip case rides this matrix), dismissal by
   outside click, ladder unchanged.

## 5. Risks

- **R1 — focus-grab delivery order** (F8): the latch contract relies on
  the clicked-seam-before-focus-lost delivery the month steppers prove.
  If the chevron's focus-lost settle still wins the race, the symptom
  is the red journey staying red after seam wiring — the recorded
  fallback is setting the latch from the widget's `changed open`
  path via a pre-click seam (the adapter latches on `quick-toggled`
  *before* the local toggle flips), which is the same one-event window
  the D24 machinery already uses. Stage 2 reports which shape landed.
- **R2 — post-close focus landing** (D-3.3): Slint's fallback after the
  panel scope unmounts is unpinned; the contingency (a `grab()` on
  `TextField`) is recorded and gated on the new journeys, not applied
  speculatively.
- **R3 — init-grab drop on the fresh conditional** (P5 class): covered
  by the adapter-bumped nonces (D-3.3); if a nonce bump lands before
  mount on the first frame, the `changed` handler fires at mount — the
  TimerSetter precedent is the proof shape.
- **R4 — mirror-loop oscillation**: both directions of the D-3.3 loop
  are site-guarded and no-op on already-equal writes; the failure mode
  (open flapping) would show as the panel unmounting/remounting in the
  geometry journeys. The convergence argument in D-3.3 is the review
  checklist.
- **R5 — journey churn in the r4 pins**: the dropdown journeys read the
  per-site facts and `DropdownLadder`, all preserved; the picker
  two-phase and combobox journeys (`journeys.rs:8152-8267`, `:8269+`)
  read task-model fields, all preserved. The only expected edits are
  **additions**; any *modification* of an existing assertion is a
  finding to report, not to absorb silently.
- **R6 — chunk-4 parallel lane** (focus coverage): this chunk adds no
  stops and disables one (the empty-state field — legitimately
  non-interactive); if chunk 4's invariant lands first, the empty-state
  journey's disabled assertion must not read as a coverage hole.

## 6. Acceptance (dispatch §4, restated against this plan)

- Every flyout mounts under its anchor with clamp/flip — the six-site
  matrix (D-3.2's groups; four `TimeField` sites + two `Dropdown`
  sites) drives each and asserts realized geometry via the facts.
- The quick-time button opens the menu without closing the picker
  regardless of prior keyboard state (the F8 journey, mouse-only).
- A set time still shows the full quick list at open; typed-digit
  narrowing per keystroke still narrows (F10, both halves journeyed).
- The empty board list shows the disabled+caption state; keyboard open
  stays gated on non-empty rows.
- The dropdown ladder journeys (`journeys.rs:12303+`, the settings
  selector matrix) stay green untouched; the decline arm
  (`adapter.rs:5369-5391`) is byte-identical.
- Kiosk posture unchanged: no new windows, no renderer/platform code,
  headless-backend facts globals only (existing precedent).
- Full AGENTS §10 gate suite green via `scripts/run-quality-gates.sh`.

**Estimated diff**: ~750–950 net lines across ~15 files
(deletions ~200: the two displaced flyout blocks; additions: two panel
components ~260, app-window overlay groups + forwards ~90, widget
mirroring/empty-state ~80, adapter seams/flags/nonces ~90, model
pushes ~30, composition forwards ~15, copy constant ~5) plus ~350–450
lines of new/extended journeys. Consistent with the dispatch's **M**
size.
