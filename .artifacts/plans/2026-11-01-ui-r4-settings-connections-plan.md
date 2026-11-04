# [Plan] GUI Review Round 4, Chunk 11 — Settings: Scale + Connections

- **Date**: 2026-11-04
- **Status**: PROPOSED — stage-1 plan for chunk 11 of the round-4
  dispatch. Branch `feat/ui-r4-settings-connections`, cut from
  `origin/main` @ `6a044722` (`scripts/worktree-doctor.sh` passed
  pre-creation, 219 G free against the 25 G floor; the local `main`
  ref is behind, so the branch was seeded off the fetched
  `origin/main` — the merge-gate chunk lives there).
- **Gates verified (2026-11-04)**:
  - *Merge gate*: chunk 3 (icons) is merged — PR **#577**
    (2026-11-02). The D11-grown kinds this chunk consumes are on
    `origin/main`: `IconKind.link`, `IconKind.link-off`,
    `IconKind.chevron-down` (`ui/widgets/icon.slint`, each annotated
    "chunk 11") over the vendored `link.svg` / `unlink.svg` /
    `chevron-down.svg`. No decision gate binds this chunk (§2
    decisions 1–6 all confirmed 2026-11-01).
  - *Backlog chunk 2.4 (#412 window-geometry)*: **LANDED** — PR #579
    merged 2026-11-02 (`feat(app,ui-slint): the window geometry
    persists`). Issue closed. **No `app-window.slint` rebase
    expectation applies** (the gate only asks for one when #412 is
    still open). The geometry seam touches the window chrome lane,
    not the settings surface — no collision; this chunk's
    `app-window.slint` edits are the settings callback wiring only.
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.11 (D38–D40), §1
  findings F28/F29, §4 (testing strategy), §7 (risks), §8
  (bookkeeping); dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1 (blanket acceptance rule, shared execution model, code-quality
  mandate) and §12 (acceptance a–d). Background: round-3 §3.4 D20
  (the scale trio this chunk amends) and its stage-1 plan
  `2026-10-26-ui-r3-settings-polish-plan.md` §5; round-1 D23 (the
  segment deferral #411 lifts); GitHub issue #411 (OPEN — the
  board-switcher ruling this chunk resolves).
- **Line references** are from this branch @ `6a044722`. The design
  round's line numbers pre-date chunks 5–10 and have drifted; the
  mechanisms are unchanged.
- **Non-goals** (dispatch §12): no connection/bind semantics changes
  (Attach/Detach lanes keep their requests and replies); no keymap
  editing; **empty diff over `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud`,
  `taskboard-app`** (review focus (f)) — everything below is
  `taskboard-ui-slint`-only.

## 0. Findings → mechanisms audit (what the code confirms)

- **Scale**: `settings_scale_preview`
  (`adapter.rs:3774-3799`) clamps and live-writes
  `Theme.base_scale` **and** — when no typed draft exists —
  `SettingsModel.scale-echo` (writer point 2 of round-3 D20a). The
  release path `settings_scale_released` (`:3838-3860`) funnels the
  in-gate value through `apply_scale_commit` (`:3700-3729`), which
  captures the scroll anchor pre-layout (`request_scale_anchor`,
  `:2935-2944`), writes the theme, and sends one `SetScale` over the
  lane with the pre-gesture value as the revert point (the
  `PendingReply::Scale` Err arm re-applies it). Markup:
  `settings-view.slint:692-706` mounts the `Slider`
  (`value: Theme.base-scale`, one-way) and wires
  `pressed/preview/committed` → root callbacks → app-window
  (`app-window.slint:381-383, 1010-1012`). The widget
  (`slider.slint`) emits `preview` per move and `committed` once on
  pointer-up; the thumb/fill read the one-way `value`. **The F28
  flicker loop is exactly the preview arm**: every move re-lays the
  world out under the pointer. Only consumer of `Slider`: the
  settings scale row.
- **Connections board selector**: the per-board radio row list
  (`settings-view.slint:446-493`) renders `SettingsModel.boards`
  (struct `SettingsBoardRow`: id/name/bound-connector/bound/
  selected/busy); each bound row carries its own **Detach** button;
  selection routes `board-selected(id)` →
  `settings_board_selected` (`adapter.rs:3583-3591`) which writes
  `SettingsVm.selected_board` and pushes — **Bind-only today** (no
  board.select is ever dispatched; the seam does not and must not
  switch the viewed board). Bind
  (`settings_bind_row`, `:3594-3627`) resolves the selection into
  `AttachConnector`. Detach (`settings_detach_row`, `:3629+`)
  resolves a **board-row index** into `DetachConnector`.
- **Staleness**: `SettingsVm.boards` mounts once from the install
  parameter and afterwards refreshes **only** from
  `PendingReply::List` snapshots (`:3062`; the field comment at
  `:2803` says so). Boards created/deleted on the board surface
  while settings is closed are invisible until some settings reply
  lands — F29's "clicking a non-active board appears to do
  nothing" is partly this ghost list.
- **The app-side authority** (the mount-refresh key, §3.4): the
  reply snapshot's boards half is built by
  `current_connections` (`taskboard-app/src/ui_requests.rs:801-817`)
  **from the engine state** — `state.boards.values()` filtered
  `!deleted && !archived`, `bound_connector` from
  `remote.binding.connector.to_string()`. The adapter already holds
  that exact snapshot (`self.state.load()` in `push_settings`,
  `:3320`), and `connector_row_status`
  (`presentation.rs:4060-4084`) already trusts the same state for
  fold status. **The board projection can therefore live ui-side,
  derived per push pass, with an empty app diff.** No new
  `UiRequest` variant exists or is added (the only snapshot-bearing
  requests are Save/Remove/Attach/Detach — all mutating).
- **Connector rows**: `ConnectionRow` folds
  `connector_row_status` + the `liveness` down-override
  (`:3342-3367`); markup `settings-view.slint:494-553` renders
  dot + name + user@server + Test/Bind/Remove buttons and an
  InfoTip. No hover feedback on the row itself (F29).
- **Widget precedents**: `time-field.slint` (chunk 7) is the
  sanctioned flyout grammar — conditional sibling in the panel's
  coordinate space (no window-level overlay layer), window-sized
  underlay `TouchArea` as the outside-click closer (the recorded
  Slint negative: a TouchArea click never moves focus, so it is
  absorbed), flyout `FocusScope` grabs focus on open, widget-internal
  key ladder with `accept`/`reject`, `ListView` +
  `request-row`/`request-nonce` seek. `icon-button.slint`'s
  FocusScope raw-key half (Return/Space activate) is the closed-field
  precedent. `board-bar.slint` is the #411 adopter: tab segments
  (160·scale cap, elide), per-segment hover tooltip
  (`TooltipState.hover` with title/description/hint-keys),
  right-click `segment-context`, keyboard switching only through the
  action layer (`board.next/previous`). `Responsive.current-breakpoint`
  (compact < 720 px, expansive ≥ 1440 px, **unscaled**) is read
  directly in compositions (action-menu, app-window, board-layout —
  the established pattern).
- **Test state** (what the round-3 trio actually pins):
  `journeys.rs:3169 the_scale_echo_syncs_both_ways` pins the
  preview arm four ways (preview live-applies; echo follows the
  thumb; the draft survives a concurrent preview; focus-loss
  re-syncs to the *previewed* value 1.9). `:3284` pins the commit
  scroll anchor; `:3670` the parse-miss flavour; `:4079` scroll
  reach at scale 3. `slider_grab_probe_tests.rs` p1/p2/p3 pin the
  Flickable-steal mechanics and the pan-disable fix — probe-local,
  driven through the probe's own `GrabProbe` globals, **not** the
  settings adapter arm. `:8538` the selector renders/selects/
  refreshes-from-reply; `:8627` Bind; `:8775`ff Detach — all
  adapter-seam-driven (`invoke_settings_board_selected` et al.),
  so the markup rework does not touch their seams.

## 1. D38 — the preview dies; release-commit pins replace it

### 1.1 Removal inventory (one commit; each dead pin gets its successor in the same commit)

| Site | Fate |
|---|---|
| `adapter.rs` `settings_scale_preview` (whole fn) | **Deleted.** With it the last mid-gesture `Theme.base_scale` write and the `set_scale_echo` writer point 2. |
| `settings-view.slint:700-702` preview hook + `:301` root callback + `app-window.slint:382, 1011` wiring | **Deleted.** The `Slider.preview` callback itself **stays** on the widget (see 1.2). |
| `settings_view_mounted` / `settings_scale_focus_lost` / `settings_scale_pressed` / `settings_scale_released` / `apply_scale_commit` / `request_scale_anchor` + the Flickable's `changed content-height` consume | **Unchanged** (the r3 D20a draft hygiene, D20c anchor). The out-of-gate revert's `set_base_scale(previous)` in `released` stays: with the preview gone it writes the same value the theme already holds — kept so the release path keeps exactly one code shape and `apply_scale_commit`/revert remain the only theme writers, which is D38's wording. |
| `slider.slint` | **Reworked** (1.2): thumb-follows-pointer becomes widget-internal. |
| Round-3 pan-disable fix (`mouse-drag-pan-enabled: false` on the settings Flickable) | **Stays.** The grab probe's P2 proved the steal does not need the re-layout; killing the preview alone would not restore the gesture. |

### 1.2 The slider's widget-internal gesture

D38: "during a drag the thumb follows the pointer (widget-internal),
the app re-layouts once on release." Today the thumb reads the
one-way `value` (= `Theme.base-scale`), which only the preview arm
moved mid-gesture. After the removal the widget must hold the gesture
itself:

- `property <float> gesture-value` — written by `moved` (the same
  snapped, clamped `value-at(mouse-x)` the preview carried).
- `out property <float> rendered-value: touch.pressed ? gesture-value
  : root.value;` — the fill and thumb fractions and
  `accessible-value` read `rendered-value`. (The a11y value reading
  the gesture mid-drag is the honest reading, and it gives the probe
  a typed observation seam.)
- `preview(float)` **keeps firing per move** as the widget's gesture
  telemetry (the AT `step-by` path and probe P3 count it); the
  settings markup simply stops wiring it. Recorded decision: a
  widget API that no surface consumes is still the gesture's
  per-move contract — deleting it would orphan P3's telemetry
  assertions and foreclose the next preview consumer for no gain.
- On release `committed` fires once → `settings_scale_released` →
  `apply_scale_commit` → theme write → `value` re-renders the thumb
  at the committed spot. **Mid-gesture the theme never moves → no
  re-layout → the F28 flicker loop is dead by construction.**

### 1.3 The echo after the preview

Writer table (round-3 plan §5.1) after the removal: writers 1 (push
pass: draft ∨ `format_scale(committed)`), 3 (release gate-fail
revert), 4 (lane Err reply revert), 5 (field Return commit), 6
(field edited), 7 (focus loss) stand; **writer 2 is gone**. The
echo renders *draft-when-present, committed-scale-otherwise* and
**never tracks the thumb**. Focus loss still discards a Return-less
edit; what it re-syncs to is now the committed scale (unchanged by
the gesture), not a previewed one.

### 1.4 Round-3 D20 pins: die vs rewritten vs stay

| Pin | Verdict |
|---|---|
| `the_scale_echo_syncs_both_ways` preview halves ("preview live-applies", "echo follows the thumb", "preview never clobbers the typed draft", focus-loss → 1.9) | **Die.** Rewritten in the same commit as the release-commit pins below. |
| → `the_scale_drag_moves_nothing_until_release` (new journey) | Press + move ticks + release over the real widget seams: `Theme.base_scale` and `scale-echo` hold the committed value until release; release emits exactly one `SetScale`; the theme then moves once. |
| → `the_scale_echo_renders_the_committed_scale` (new journey) | Mount echo; commit echo; Err-revert echo; focus-loss discards the draft and re-renders the **committed** scale; a typed draft survives a concurrent drag (trivially — nothing writes the echo mid-gesture anymore). |
| `a_scale_commit_anchors_the_scroll_on_the_scale_row` | **Stays** (D38 keeps the anchor; the one-relayout-per-commit shape is what the anchor consumes). |
| `a_scale_parse_miss_renders_the_invalid_flavour_and_typed_detail` | **Stays** (the field's Return grammar is untouched). |
| `settings_scroll_reaches_the_scale_controls_at_scale_3` | **Stays.** |
| `slider_grab_probe_tests` p1/p2/p3 | **Stay green unchanged** — probe-local globals; p3's `previews == 5` keeps counting the widget's telemetry callback (1.2). |
| → p4 `the_thumb_follows_the_pointer_while_the_theme_holds` (new) | Same probe harness: drag with the preview telemetry counting and no theme write — `rendered-value` tracks the pointer mid-drag (the probe reads the new out property through its `GrabProbe` mirror), the theme global is untouched, release commits exactly once and `rendered-value` lands on it. This is the widget-internal half of D38. |
| `bind_routes_attach_and_the_typed_reply_lands`, the selector/detach journeys | **Stay** as adapter-seam journeys, with the detach journey rewired onto the new selection-driven seam (§3.3) and the selector journey's refresh-mechanism assertion updated (§3.4). |

CHANGELOG: the round-3 "live-apply" line gets its removal note
("scale applies on release/commit; the slider no longer previews").

## 2. D39 — the dropdown widget (`ui/widgets/dropdown.slint`)

Generic, surface-neutral (the time-field contract): the widget
renders what it is pushed and reports what the user did; every
intent leaves through callbacks; no inline side effects; both the
settings connections selector and the board bar mount it.

### 2.1 Model

```slint
export enum DropdownIndicator { none, link, link-off }
export enum DropdownTone { neutral, success, accent, warning, danger }
export struct DropdownRow {
    label: string,
    indicator: DropdownIndicator,
    tone: DropdownTone,
    selected: bool,
    busy: bool,
}
```

The indicator→glyph mapping is the widget's own three-arm total
(`none` → nothing; `link` → `IconKind.link`; `link-off` →
`IconKind.link-off`), colored via `Icon.colorize` by the tone. The
tone vocabulary mirrors the `RowDot` color language
(`success`/`accent`/`warning`/`danger`/`neutral` =
`Semantic.success`/`focus-ring`/`warning`/`danger`/`text-secondary`)
so the dropdown reads like the dots it replaces; `RowDot` itself is
untouched (the connector rows keep their dots). `busy` renders the
`BusyDot` ceremony in the row's dot slot (the r2 D34 precedent).

### 2.2 Anatomy

- **Closed field** (one walk stop): a `FocusScope`
  (`focus-on-tab-navigation: true`, `accessible-role:
  AccessibleRole.button` — the pinned Slint 1.18 role set exposes no
  combobox role; the a11y label is the caller's `field-label`
  property) showing the pushed `field-text` (the selected row's
  label, or the caller's empty-state placeholder), the busy dot when
  the selection is mid-ceremony, and an inline chevron
  (`IconKind.chevron-down`). The whole field is one click target; a
  raw Return/Space opens (the icon-button raw-key half precedent —
  `accept` keeps the action layer out of it).
- **Open flyout** (the time-field grammar verbatim): a conditional
  sibling in the panel's coordinate space — **not** a window-level
  overlay, no dismissal-layer interaction — with the window-sized
  underlay `TouchArea` as the outside-click closer. Flyout width =
  field width; height capped at six rows, `ListView` scrolls the
  rest; `drop-shadow` + `surface-raised` + focus-ring border as the
  time-field draws it. Rows carry hover feedback
  (`has-hover ∪ highlight` background — the feedback F29 demands)
  and fire `row-hovered(index, x, y, w, h)` / `row-unhovered()` so
  the **consumer** owns tooltip policy (blanket rule without
  teaching the widget about `TooltipState`).
- **Open/focus hand-over**: opening re-anchors the highlight to the
  selected row (or 0 when none), bumps the seek nonce, and grabs
  focus into the flyout `FocusScope` (`AccessibleRole.list`, rows
  `list-item`). Closing (select/Escape/outside) hands focus back to
  the field's FocusScope.
- **Keyboard ladder** (widget-internal, preset-independent; the
  action layer never sees these strokes): Return/Space on the closed
  field opens; Up/Down **and** Ctrl+N/Ctrl+P move the highlight
  (clamped at the ends, the time-field convention); Return on the
  flyout selects (`row-activated(index)`) and closes; Escape closes
  only; Tab closes and **rejects** so the walk continues.
  ⚠ **Probe first (risk R1)**: whether `KeyEvent.modifiers.control`
  is observable in a widget `key-pressed` on the pinned 1.18.1. If
  it is not, the C-n/C-p arms degrade to arrows-only and that is a
  **recorded deviation from D39's normative ladder** (reported, not
  silent) — tier-4 fallback.
- **Callbacks**: `row-activated(int)`, `row-context(int, float,
  float)` (optional; only the board bar wires it), `row-hovered(...)`,
  `row-unhovered()`. The widget never routes actions itself.

### 2.3 Mount-refresh of the board list (the staleness fix)

- New pure helper (beside `connector_row_status` in
  `presentation.rs`): `boards_of(&AppState) -> Vec<BoardInfo>` —
  the ui-side twin of `ui_requests.rs:801`'s board half: live
  boards (`!deleted && !archived`) in state order, `id`/`title`,
  `bound_connector` from `remote.binding.connector.to_string()`.
  Unit-tested against the domain state machine's existing fixtures.
- `push_settings` rewrites `SettingsVm.boards` from
  `self.state.load()` **every pass** — the list can no longer go
  stale by construction (D39's mount-refresh is subsumed; the mount
  itself pushes). `settings_view_mounted` is unchanged.
- The snapshot write at `:3062` (`self.settings.boards =
  snapshot.boards`) is **removed** (one-writer rule) together with
  the `:2803` staleness comment. The reply payload keeps its boards
  half — `ConnectionsSnapshot` is the app's reply contract and the
  app diff must stay empty (recorded: the app-side projection and
  `boards_of` are two projections of one authority; consolidation is
  a future issue if review objects, risk R3).
- `selected_board` pruning rides the same pass: a selection whose
  board no longer renders is dropped silently (the stale/foreign-id
  precedent at `:3583`) — otherwise Bind stays armed on a ghost.

## 3. The connections composition (settings-view)

### 3.1 Label + responsive placement rule

The label is the presentation constant
`SELECT_BOARD_LABEL = "Select Board to Connect/Disconnect:"`.
Placement reads `Responsive.current-breakpoint` the established way
(`compact` property pattern, action-menu/board-layout precedent):
`compact` → label above the dropdown (`VerticalLayout`), regular /
expansive → label left of it (`HorizontalLayout`, label
`vertical-alignment: center`). Unaffected by scale by construction
(breakpoints are unscaled).

### 3.2 The dropdown mount + the selector seam (unchanged semantics)

- The adapter pushes `SettingsModel.board-options: [DropdownRow]`
  plus `selected-board-label: string` (the selected board's name or
  the empty-state placeholder) — the new single selector model.
  Per option: `indicator = bound ? link : link-off`;
  `tone` = bound → the bound connector's folded status tone,
  unbound → `neutral`. The fold reuses `connector_row_status`
  (`&state`, the board's bound `ConnectorId`, the liveness
  down-override) — the same fold the connector rows run — mapped
  `Idle→success, Syncing→accent, Offline→warning,
  Failed/Down→danger` (RowDot's exact language).
- `SettingsModel.boards` / `SettingsBoardRow` /
  `push_settings_boards` / its `MirroredRows` handle **die**
  (nothing else consumes them). `busy` per option rides the
  derivation (`detach_busy && busy_target == board.name`).
- `row-activated` → the existing `board-selected(id)` callback →
  `settings_board_selected` — **byte-identical semantics**: writes
  the selection, pushes, dispatches nothing. New pin: selecting
  drains zero requests (Bind-only, review focus (c)).
- The "selected for binding" copy is presentation constants: the
  field's hover/a11y description ("Selects the board to bind to a
  connector; it does not switch the board view.") + the per-row
  tooltip description wired through `row-hovered` → `TooltipState`.
  No behaviour change to Bind.

### 3.3 Detach moves to the selection

The per-bound-row Detach button dies with the row list; the verb
stays (no semantics change — same `DetachConnector` request, same
reply, same busy ceremony). New seam `settings-detach-selected()`:
resolves `selected_board` → its `BoardInfo` (absorbed silently when
none or unbound — the stale-id precedent), then the existing
`DetachConnector` send with `PendingKind::Detach` and the board name
as `busy_target`. The button sits beside the dropdown, enabled =
selected board is bound && !busy. `settings_detach_row(index)`
**dies** with its markup; the app-window wiring arm follows. The
detach journey rewires onto the new seam (§1.4 table).

### 3.4 Connector rows: hover feedback only

Rows keep dot/name/buttons/InfoTip. The row surface gains a
`TouchArea`-driven hover background (`Theme.surface-hover`, the old
board-row precedent) and nothing else — no new verbs, no tooltip
machinery beyond what the buttons already carry.

### 3.5 Journeys (new/rewired)

- Open via click and via Return on the focused field; ladder: Down /
  Ctrl+N move the highlight (flyout-seam facts, the
  `get_flyout_*` fact pattern the timer popover journeys use),
  Return selects → `selected-board` + `selected-board-label` land,
  Bind's enablement follows; Escape closes; outside click closes;
  Tab closes and walks on.
- Indicator/tone: unbound option renders link-off/neutral; a bound
  board whose connector folds `Failed` renders link/danger (pure
  mapping tests at the presentation layer + a journey-level
  structural check).
- Stale-list repair: seed state with a board created after install
  → the options pass contains it (the F29 staleness pin, now
  by-construction); a selected board deleted elsewhere → selection
  pruned, Bind disabled.
- Detach-selected round trip (parity with the old detach journey:
  one `DetachConnector`, busy ceremony, reply lands).

## 4. D40 — the #411 adoption verdict (board bar)

**Verdict: threshold adoption.** The board bar renders the shared
dropdown **as its board switcher only past a segment budget**;
below it, the reviewed tab strip stays.

- **Reading of the ruling**: #411's scope is many-board
  scalability ("for many boards" in the title; round-1 D23 deferred
  exactly "a dropdown/popup redesign for many-board scalability");
  D40 says "the many-boards case". The strip is good UX for a
  handful of boards and carries affordances the dropdown must not
  have to re-earn (per-segment right-click → select + board context
  menu, r2 D21; per-segment tooltips). Replacing it universally
  would churn a stable, twice-reviewed surface for the common case
  to fix a degradation that only exists at many boards.
- **The threshold**: `BOARD_BAR_DROPDOWN_AT = 8`, a named
  presentation constant in `board-bar.slint` (markup-local; no
  adapter change, no config — Q22 freezes the `[ui]` surface).
  Rationale: segments cap at 160·scale and elide; past ~8 the row
  degrades into an elided sea with no reachability guarantee.
  Tier-4 revisits the number on the reference machine.
- **What the bar gains when over budget**: a switcher that scales
  (ListView rows), row hover feedback, and a keyboard-operable
  switch through the widget ladder — today switching is
  action-layer-only (`board.next/previous`). Rows are the board
  titles with `indicator: none` (recorded: connection status is the
  status-bar/connector surfaces' business, not the view switcher's).
- **Mechanics**: `bar.segments.length > BOARD_BAR_DROPDOWN_AT` →
  the dropdown replaces the strip (same one-control height rule,
  round-1 D20b); the field shows the selected segment's title;
  `row-activated(i)` → the existing `segment-activated(i)` seam —
  zero adapter changes (`BoardBarModel` already pushes segments +
  selected). The per-segment tooltip wiring moves to
  `row-hovered` (same `TooltipState.hover(title, "Switches to this
  board.", hint…)` shape). `row-context` stays **unwired** —
  recorded remainder (1) below: the flyout must not host the board
  context menu in this chunk (overlay-over-flyout dismissal
  ordering is unprobed); in dropdown mode the menu stays reachable
  through the action menu. Tab stops: dropdown mode collapses the
  N segment stops to one field stop (the walk inventory test
  covers both modes).
- **Remainder filed/annotated on #411 at stage 3** (dispatch §12:
  "✅ DONE or annotation"): (1) per-board context menu is
  strip-only; dropdown mode reaches it via the action menu;
  (2) threshold-vs-always is a taste follow-up — the constant makes
  `always` a one-line change if the user prefers it; (3) no
  connection indicators on the bar (deliberate). With those
  annotated, the issue's scalability scope lands → close with DONE +
  PR link per §13.

## 5. Blanket-rule compliance (hover + walk stops)

| Element | Hover tip | Walk stop | Dispatch |
|---|---|---|---|
| Dropdown closed field | label + description (consumer-pushed `field-label`/`field-description`) | yes (FocusScope) | Return/Space open (widget-internal) |
| Dropdown flyout rows | consumer-wired via `row-hovered` → `TooltipState` | flyout FocusScope holds focus; ladder operates | `row-activated` |
| Settings Detach-selected button | existing `SettingsButton` affordances | yes | `settings-detach-selected` |
| Board-bar dropdown field/rows | the segment tooltip shape via `row-hovered` | yes / flyout scope | `segment-activated` |

No inline side effects anywhere: every intent is a callback the
consumer routes (the time-field contract).

## 6. Test catalogue (TDD; red where the behaviour is new)

**Tier 1 — pure units** (`presentation.rs`): `boards_of` (live
filter, binding round-trip, selection prune helper); status→tone
mapping (typed `matches!`/equality, no text); the existing
`connector_row_status` suite untouched.

**Tier 2 — widget probes**: slider p4 (§1.4); a dropdown probe
composition (`dropdown_probe.slint`, the focus_seam/icon_probe
registration pattern) pinning: open/close transitions and focus
hand-over, ladder steps (arrows + C-n/C-p — R1's probe lands here
first), clamp-at-ends, select routes `row-activated` + closes,
Escape/Tab/outside-click dismissal, hover-union row background,
indicator glyph mapping (`none/link/link-off`), busy row ceremony,
six-row cap + ListView seek.

**Tier 3 — journeys** (`journeys.rs`): the §1.4 scale rewrites;
the §3.5 connection set; board-bar threshold journey via a probe
composition mounting the bar at 7 (strip) and 9 (dropdown) segments
— picked routes `segment-activated`, ladder switches boards,
Escape closes; the existing selector/Bind journeys stay green with
the detach one rewired; no-request-on-select pin; walk inventory
(settings gains one field stop; bar both modes).

**Goldens**: none today for settings/board-bar; none added — the
visual facts ride the probe assertions (consistent with the
timer-popover chunk's precedent).

**Regenerated**: any snapshot the echo semantics touch
(`keymap_snapshots` unaffected expected; verified at implementation).

## 7. Tier-4 (manual, reference machine)

- Dropdown feel: open latency, underlay click-through, flyout over a
  scrolled Flickable (underlay coordinate math under `content-y ≠
  0`), wheel over the open list vs page scroll.
- Ladder feel under all three presets (R1's verdict in vivo).
- Scale drag post-change: the F28 repro (fast thumb drags mid-list)
  produces zero flicker; thumb tracks pointer; exactly one re-layout
  on release; anchor still lands the row in view both directions.
- Board bar at 9+ boards: switch feel, elide threshold sanity.

## 8. Commit slices (stage 2; standalone, rebase-clean)

1. `refactor(ui-slint): the scale preview dies into a release commit`
   — widget gesture-value + p4, adapter arm + markup hook removal,
   the §1.4 pin rewrites, CHANGELOG scale note.
2. `feat(ui-slint): the shared dropdown widget` — `dropdown.slint` +
   probe + Tier-2 suite (structural tests before any consumer, per
   dispatch 11.2).
3. `feat(ui-slint): the connections selector is a dropdown` —
   derivation + options push + detach-selected + label/placement +
   indicator mapping + row hover + journeys.
4. `feat(ui-slint): the board bar adopts the dropdown past its
   segment budget` — threshold adoption + journeys.
5. CHANGELOG residue + (stage 3: rebase, review, ship, #411
   annotation/close, teardown).

## 9. Risks

- **R1 — Slint `KeyEvent.modifiers` in widget `key-pressed`**
  (C-n/C-p): unverified on the pinned 1.18.1. Probe first in slice
  2; an unreadable modifier is a recorded D39 deviation (arrows
  stay), never silent.
- **R2 — flyout geometry inside a scrolled Flickable**: the
  underlay's negative-offset trick depends on
  `absolute-position` math with `content-y ≠ 0`; the time-field
  precedent already lives in a scrolled panel (timers form), so
  this is a journey, not a probe.
- **R3 — the boards projection exists twice** (ui-side
  `boards_of`, app-side `current_connections` board half): the app
  copy is reply-payload construction and the app diff must stay
  empty; recorded; consolidation is a future issue if review
  objects.
- **R4 — the detach seam reshape** (index → selected): busy-keying
  (`busy_target` = board name) and the `PendingKind::Detach` reply
  path are reused; journey parity with the old detach journey is
  the gate.
- **R5 — settings-model churn** (`boards` → `board-options`,
  `MirroredRows` handle swap): pure-ui; the round's render lane has
  landed, so the models.rs rebase friction the dispatch warned
  about for chunks 9/10 does not apply here.
- **R6 — anchor/pan-disable interplay**: with the preview gone,
  `changed content-height` fires once per commit — the one-shot
  anchor consume is unchanged; the r3 pan-disable stays regardless
  (P2's exoneration).
- **R7 — rebase friction**: `settings-view.slint`/`adapter.rs`
  were touched by landed chunks 4/6/7; the branch is cut from fresh
  `origin/main`, so the dance starts from zero drift.

## 10. Diff size estimate

`slider.slint` ±30; new `dropdown.slint` ~+230; `dropdown_probe*`
~+200; `settings-view.slint` ~+100/−80; `board-bar.slint`
~+70/−25; `adapter.rs` ~+70/−60; `presentation.rs` +tests ~+90;
`models.rs` ~+25/−12; `app-window.slint` ~±12;
`journeys.rs` ~+280/−110. **Net ≈ +600 lines** — M-sized, matching
the dispatch table.
