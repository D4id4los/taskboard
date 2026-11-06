# [Plan] GUI Review Round 5, Chunk 4 — Focus Coverage + the Invariant

- **Date**: 2026-11-06
- **Status**: PROPOSED — stage-1 plan for chunk 4 of the round-5
  dispatch. Branch `feat/ui-slint-focus-coverage`, cut from the
  fetched `origin/main` (`d878749d`; `scripts/worktree-doctor.sh`
  passed pre-creation, 154 G free against the 25 G floor; worktree
  `../taskboard-WT-feat-ui-slint-focus-coverage` created with
  `git worktree add -b feat/ui-slint-focus-coverage
  ../taskboard-WT-feat-ui-slint-focus-coverage origin/main`).
  Note: the chunk was staged against `main` @ `58ae9c55` but chunks
  1–3 landed first — chunk 1's dead-seam fix, chunk 2's
  `MultiLineEditor` (its commit bar/scrollbar `TouchArea`s are
  inventoried below) and chunk 3's window-anchored flyouts (the
  `dropdown.slint` grammar moved to the overlay + dismissal
  underlay) are all in the base this plan audits. Gate
  preconditions: none beyond dispatch order (chunk 4 may parallel 3;
  3 landed first, so nothing rides the parallel lane).
- **Origin** (normative): round plan
  `2026-11-05-gui-review-round5-plan.md` §2 rows F13/F15/F19, §3 D5,
  §4.4; dispatch plan
  `2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1/§5 (acceptance: ring on every focused segment + Enter/Space
  activation; slider takes focus and steps with arrows;
  Save/Test/Bind/Remove/Detach are stops; the invariant test fails
  when a segment's `FocusScope` is removed and passes across all
  views; `journeys.rs:6424+` still passes; gates green. Non-goals:
  no keymap/preset changes, no view redesigns beyond the named
  widgets, no Today item-walk semantics change). Code-quality
  mandate per dispatch §1 applies at stage 3.
- **Scope guard**: `taskboard-ui-slint` only — three widget files
  (`segmented-control.slint`, `slider.slint`), two view files
  (`settings-view.slint` root + its local `SettingsButton`
  component), the task-view open-branch latch migration (a
  comment-flagged TODO this chunk retires), the facts global
  (`ui/walk-facts.slint`), one new test binary
  `tests/focus_coverage.rs`, probe additions in
  `tests/walk_probe.slint`/`_tests.rs`, and one journey each in
  `tests/journeys.rs` (segment activation, slider keyboard) plus a
  pixel-floor case in `tests/usability_components.rs`. The
  pinned task-view inventory walk stays untouched (the task
  view's stop set does not change). Empty diff over every other
  crate.
- **Kind note (the r2 D26 honesty clause's retirement)**:
  `segmented-control.slint` currently carries a recorded gap —
  "Keyboard operability, recorded honestly (r2 D26): AT activation
  … is the keyboard path … this chunk does not invent one". This
  chunk invents it; the header comment is rewritten as part of the
  widget change, not left contradicting the code.

---

## 0. Findings → mechanisms audit (what the code confirms)

All anchors read on the branch base `d878749d`.

| Fact | Anchor | Evidence |
|---|---|---|
| F19a — `SegmentedControl` has a per-segment `FocusScope` (`seg-focus`) with the radio a11y triple, but nothing binds the ring or focus to the visuals; it relies on `accessible-action-default` only (AT-invoked, not raw keys) | `ui/widgets/segmented-control.slint:56-97` | hover is the only state in the segment `background` ternary; the container's border is constant (`Theme.border`); `accessible-action-default` is the sole activation besides `seg-touch.clicked`; header comment records the gap verbatim |
| F19b — `Slider` is pointer-only plus AT increment/decrement; no `FocusScope`, no focus ring, no raw keys; the AT path calls `step-by` which fires `preview` + `committed` back-to-back | `ui/widgets/slider.slint:40-60`, `:112-149` | the a11y triple sits on the root `Rectangle` itself; `step-by` documents "one AT action = one commit"; `touch := TouchArea` owns the entire gesture |
| F19c — the IconButton grammar is the raw-key + ring template | `ui/widgets/icon-button.slint:33`, `:44-64` | `border-width: focus-surface.has-focus ? … : 0` on the root; a full-size `focus-surface := FocusScope` with `focus-on-tab-navigation`, the `accessible-*` trio, `accessible-action-default`, and the `key-pressed` half: `event.text == Key.Return || event.text == " "` → callback → `accept`, else `reject` |
| F13a — `SettingsButton` (settings-local component) is a bare `Rectangle` + `TouchArea` with an AT-only `accessible-action-default`; no FocusScope, no ring, no raw keys | `ui/views/settings-view.slint:270-299` | `border-width: touch.has-hover ? … : 0` — hover is the only visual state; 7 instantiation sites (Detach, Test/Bind/Remove per row, Save) |
| F13b — the settings view root lacks the D15 mount-focus parity: no `init => self.focus()`, no focus ring, while every other view has both | `ui/views/settings-view.slint:532-585` vs `task-view.slint:216-219` (+ board/review/stack/today/plan-next-day) | siblings: `focus-on-tab-navigation: true; accessible-role: region; accessible-label; init => self.focus(); ring := Rectangle { border-width: root.has-focus ? … }`; settings: only `init => { root.view-mounted(); }` — the C-n/C-p chords land in `key-root` (the app-wide capture scope, `app-window.slint:303-306`), so the chords *work*, but nothing marks the view as the holding scope and the walk starts cold from the window root |
| F15 — stops exist but are skipped/invisible: Slint tab traversal accepts only `is_visible_or_clipped_by_flickable()` candidates; conditionally mounted subtrees skip their `init` focus grab (P5); and settings is dense in non-stop widget classes (F13) | `i-slint-core-1.18.1/window.rs:1608` (`move_focus`'s `FocusReason::TabNavigation` arm), `:1660+` `ItemRc::try_scroll_into_visible`; probe P5 `tests/walk_probe_tests.rs:151-178` | the traversal filter is a *visibility* filter, not a registry — the systemic guarantee must come from the invariant test, not markup |
| The synthetic-Tab walk mechanism is view-agnostic and proven | `src/adapter.rs:2426-2461` (`dispatch_traversal`), `tests/walk_probe_tests.rs` P1/P4/P5 | deferred single-shot `slint::Timer` re-injects `"\t"` (or Shift-hold + `"\t"` backwards) on the window; `walk_reentry` guards `intercept`; P1 pins the mechanism, P4 pins container→child descent, P5 pins the conditional-mount negative |
| The walk-facts publication grammar exists (task-view-shaped) | `ui/walk-facts.slint`; `ui/widgets/activation-stop.slint:42-61`; `tests/journeys.rs:6406-6434` (`walk_stop`, `walk_steps`) | stops write `TaskViewWalkFacts.stop-id` on focus gain; journeys read the generated global after `advance_time(0)` |
| The walk journey pins today's inventory and wraps | `tests/journeys.rs:6443-6540` | expected vector `[0, 2, 3, 3, 5, 6, 6, 90, 91, 92, 93, 93, 11]` + wrap assertion |
| The path in front of a widget's raw `key-pressed` is unbound for the relevant strokes in Settings/TaskDetail | `src/adapter.rs:599-706` (`intercept`); `src/shell.rs:83-135` (`chain`); `src/keymap/presets.rs` (Settings/TaskDetail rows) | the resolver consumes a stroke only when a preset row binds it *in the active chain*; **no** preset binds `Return`, `Space`, `Left`, `Right`, `Up`, `Down`, `Home`, `End` in `ContextId::Settings`; TaskDetail binds only `Up/Down` (item walk) — arrows for a focused slider in task view are NOT safe raw keys (reserved), settings are; `Unhandled` ⇒ `intercept` returns false ⇒ delivery continues to the focused item. Settings is Standard-Tab+emacs-C-n/p only, and vi's Settings rows (presets.rs:629-655) add no Return/Space/arrows either |
| The reservation trap exists in the opposite direction | `src/adapter.rs:674-693` | while `EditingFacts.activation-focus-count > 0`, bare Return/Space that resolve to an Action yield to the focused cell. **Segments/buttons in settings must NOT publish the activation count** (no `ActivationStop`), or Save's Return would be eaten while no cell handler exists to act — and vi-Normal binds Return globally (`presets.rs:558`), so a count in Settings would also hijack vi's Return vocabulary. `ActivationStop` stays a task-view closed-cell mechanism |
| The settings surface inventory (everything reachable in the view) | `ui/views/settings-view.slint:648-1480` | pinned header `IconButton` (Back); `Dropdown` (board picker, `field-scope` FocusScope at `dropdown.slint:240-243`); 3 + 4 `LabeledField`s (fields are walk stops with inner `TextInput`s); `Slider` + scale `TextField` (commit-affordance); 7 `SegmentedControl`s; `SettingsButton` ×7; `InfoTip` ×6 (FocusScope at `info-tip.slint:44-50`); the general/reminder/focus minute fields |
| Clip hosts that exclude their children from traversal (`clip: true` outside any `Flickable` — the §4.4 trap list, re-verified) | `today-view.slint:235` (`row-viewport-clip`), `review-view.slint:243` (`ledger-clip`), `board-layout.slint:370` (stack header), `text-field.slint:126` (field border), `multi-line-editor.slint:114` (viewport), `dropdown-flyout.slint:37`, `quick-list-flyout.slint:39`, `stat-bars.slint:41,48` | all decorative or Flickable-internal content clips; **none** of them hosts a walk stop that must stay traversable mid-content, and none sits inside settings' column — the settings row (settings-view.slint:758) has no `clip`. The trap is enumerated so the invariant's scan is honest about it |
| Slider consumers today: exactly one (settings scale) | `grep -rn 'Slider {' ui` | timer setters use `SegmentedControl` + fields; no other Slider exists — the keyboard contract lands on the settings surface first, generally in the widget |
| `journeys.rs` drives settings through typed seams (`invoke_settings_scale_pressed/released/…`) and `click_by_label` | `tests/journeys.rs:3441-3538`, `:14106-14118` | the new journeys reuse the grammar; `find_by_label("Detach")` proves label-addressed buttons |
| No settings visual goldens exist | `ls tests/goldens` (board/today/task-view/review/track-entries/focus-overlay only) | adding a settings-root focus ring cannot invalidate a pinned image |

**Mechanism verdicts** (the plan's recorded decisions):

- **D-4.1 — the invariant is two independent halves in one test binary** (D5 verbatim): half (a) the stop *walk* — a generalized walk-facts publication + synthetic-Tab journey per view, pinning reading order and wrap; half (b) the *coverage scan* — a structural item-tree audit asserting every interactive element is enclosed by a walk stop, so a new interactive widget without a stop fails the build regardless of order. (b) is the guarantee; (a) pins the order the user tabs through.
- **D-4.2 — the scan runs on `ItemRc` (public `i-slint-core` item-tree API), not `ElementHandle`**: the scan needs (i) every element with its enclosing-item ancestor chain (exemption reasoning is *structural* — which widget owns the TouchArea), (ii) arbitrary predicate queries (`TouchArea` type + live `clicked` handler), (iii) geometry for the hybrid route. `ElementHandle` deliberately hides `ItemRc` (`is_same_element`/`identity_hint` are `#[cfg(system-testing/mcp)]`) and its `visit_descendants` yields elements flat without enclosing-item ancestry. `ItemRc::new_root(WindowInner::from_pub(window).component())` + `visit_descendants` (which materializes repeaters/conditionals via `ensure_item_tree_instantiated`) + `parent_item(StopAtPopups)` + `element_type_names_and_ids` + `geometry` + `downcast::<items::TouchArea>()` + `Callback::has_handler()` covers everything; the file-local `allow` documents the internal-API coupling as a probe-crate fact (the whole probe tier exists to absorb Slint-bump churn — driver.rs module doc).
- **D-4.3 — "interactive" = a live `TouchArea.clicked` handler; the accepted idiom is FocusScope+TouchArea siblings, so the stop test is *geometric containment* over the enumerated stop set; the exemption list is a typed registry with a reason per entry.** Full list in §2.3.
- **D-4.4 — no `Window::focus_item()` accessor exists on the pinned 1.18.1** (window.rs carries `focus_item` privately, no public read); the walk therefore reads stops through the generalized facts global (mechanism A), and the scan uses the geometry + wrap hybrid instead of a focused-item probe. Recorded as the reason a "focus chain enumeration" test is not possible today; a Slint bump exposing the accessor can retire the hybrid (comment at the scan).
- **D-4.5 — the Slider's keyboard ceremony reuses r4 D38 exactly**: arrows/Home/End *preview* (widget-internal thumb, `preview` telemetry), **release commits** (the action layer's release semantics — the keys are a degenerate gesture: one stroke = one gesture, commit on key-release, never on press); a held key auto-repeats steps (free; preview costs no re-layout); FocusLoss commits. The AT increment/decrement keep firing the existing back-to-back commit (r4's recorded "the AT has no gesture to release").
- **D-4.6 — `SettingsButton` converts to the IconButton grammar (FocusScope + TouchArea + raw keys), NOT `ActivationStop`**: the reservation count would make bare Return yield actions (adapter.rs:674-693) and break the vi-Normal Return vocabulary in Settings. The widget keeps its `enabled`/`click-through` gates verbatim on both paths; the focused scope publishes `WalkFocusFacts.view-id/stop-index` only.
- **D-4.7 — settings-root parity is exact D15 grammar**: `focus-on-tab-navigation: true` (the builtin default), `accessible-role: region`, `accessible-label: "Settings"`, `init => { root.view-mounted(); self.focus(); }`, `ring := Rectangle { border-width: root.has-focus ? … : 0 }`. Mount focus parity comes free: every conditional view re-runs `init` on mount (P5's mechanism is *post*-mount grabs; the fresh mount *is* attached by init on a fresh component instantiation — today-view/board-view journeys prove the pattern), and `refocus_view()` (app-window.slint:1137) already hands focus back to the active view on flow ends/view swaps.
- **D-4.8 — `SegmentedControl` gets the FocusScope sibling + raw-key half; the segment must also *publish* its focus so the segmented group is visible to the walk**: `seg-focus` publishes `WalkFocusFacts.view-id = 1` + `stop-index = GroupIndex * 16 + Slot` on focus gain — the generalizing move is *two numbers* (surface id + per-surface index), not the task view's single flat id.
- **D-4.9 — the key-pressed guards are the IconButton's exact shape extended with a shift guard** (activation on bare Return/Space only) and **a repeat guard on the slider's commit** (`!event.repeat` on release; release events cannot repeat, the guard is belt-and-braces and free).
- **D-4.10 — the settings `Dropdown`'s `field-scope` publishes the same facts pair** (view-id 1, its enumerated index); `InfoTip`/`TextField`/`MultiLineEditor` do **not** (rationale in §2.2; no markup or adapter change at those sites).

---

## 1. The walk-facts generalization (`ui/walk-facts.slint`)

`TaskViewWalkFacts` (one flat int) generalizes by **addition**, not
renaming: a new `WalkFacts` global carries the two-number surface
address (which view, which stop); `TaskViewWalkFacts` stays exactly
as today. Renaming would drag the r4 journeys' read helper and both
task-view stop-writer sets through a mechanical churn for zero
behavioral gain; the single-flat-id grammar and the pair grammar
must not merge anyway (the task view's stops would double-report
into both). The new global:

```slint
// ui/walk-facts.slint (appended; TaskViewWalkFacts unchanged)
export global WalkFacts {
    // The surface a reported stop belongs to (ViewId mirror):
    // 1 = settings (the only publisher this chunk wires). 0 with
    // stop-index 0 = "nothing reported".
    in-out property <int> view-id: 0;
    // The focused stop's index within the surface's enumeration
    // (§1's registration). 0 = nothing reported.
    in-out property <int> stop-index: 0;
}
```

- The task view keeps `TaskViewWalkFacts.stop-id` exactly as today
  (`activation-stop.slint:47`, `task-view.slint:305`) — the r4
  inventory journey is untouched. The task view's half (a) order
  pin *is* the existing journey; the new invariant binary walks
  only surfaces that publish `WalkFacts` (settings this chunk).
- Settings' stop writers are: `SettingsButton.focus-surface`, the
  segmented `seg-focus` (per segment), the settings `Dropdown`'s
  `field-scope`, and the view root scope itself.
- Publication idiom is the probe's recorded one (`changed` on a
  derived `property <bool> focused: self.has-focus;` — P2): writers
  set both fields on gain; loss writes nothing (the next gain
  overwrites; the initial 0/0 means "no report", which the walk
  treats as an unreported stop).

**Index registration (settings, tree order — normative for the
walk test)**: 1 root scope (self-report), 2 Back IconButton (adds
the publish line only), 3 Dropdown `field-scope`, 4 name field, 5
server field, 6 username field, 7 password field (each
`LabeledField`'s inner `TextField` publishes view-id 1 with its
index — a one-line `changed` addition *in settings-view.slint's
instantiation block*, not in the primitive), 8 Save, 9 keymap
segment group (slot = option index), 10 help-keymap InfoTip, 11
scale Slider, 12 scale field, 13 reminders group, 14..17 the
four general fields, 18 focus-enabled group, 19 focus-chain group,
20 focus-notify group, 21..24 the four focus-minute fields, 25
sink group, 26..31 the timer groups (timer type, work/break/long
minutes + the timer-enabled pair as authored), 32 the facts header
InfoTip, 33 Backend group, 34 picker row (`Detach`), 35.. the per-
connector rows (Test = base+0, Bind = +1, Remove = +2, help-rows
InfoTip = +3; row count is model-driven), then the Save of the
add/edit form — **the exact vector is re-derived at implementation
time from the instantiated tree order; the registration rule is
what this plan fixes: tree order, one index per stop, groups
enumerate per segment slot, rows enumerate per connector.** The
invariant assertion is *set equality + strict monotonicity*, not a
pinned literal vector — today's accidents (round-plan §6: "not pin
today's accidents beyond reading order") stay un-pinned.

---

## 2. The invariant test — `tests/focus_coverage.rs` (new binary)

TDD order: **the test binary lands first and fails** — that red is
F15/F19 formalized. The expected initial reds (predicted, to be
confirmed in the stage-2 report): settings `SettingsButton`s not
reached/`SettingsButton` invisible (F13a), slider not focusable
(F19b), segments not visibly focused + not raw-key operable (F19a),
settings root not the focus holder (F13b). The widget fixes then
turn it green view by view.

### 2.1 Half (a) — the stop walk (per view)

One test per view surface (settings, task detail; today/board
remain governed by their existing journeys per §1), reusing the
`journeys.rs` harness grammar (`Harness::mount[_with_settings]`,
`press_preset(NavNextControl)`, `advance_time(0)`) via a small
shared helper module (`tests/support/walk.rs`, new):

- Focus the view root first (mount init does it; the walk starts at
  the holding scope).
- Loop: press the preset's control-next stroke → `advance_time(0)`
  → read `WalkFacts.view-id/stop-index` → push. Terminate when the
  observed pair sequence *wraps* (the root pair repeats with the
  full cycle length > 1; a cycle-length counter with a deadline
  guard, never a wall-clock timeout — the walk length is finite by
  construction).
- Assertions: (1) the reported set ⊆ the registered stop set for
  the surface (no phantom reporters — catches a stray publish);
  (2) the unreported-step count matches the registration's
  unreported classes (fields, InfoTips, chrome — one step each,
  observed as a stale repeat of the previous pair); (3) first
  reported pair is the surface's stop 1; (4) the sequence is
  strictly the registration's tree order (no reordering, no
  skips between reported stops); (5) backwards walk (Shift+Tab via
  `NavPrevControl`) mirrors the order.
- Settings fixture: `golden_state()` + two connectors (the
  `connector_episodes_flip_the_row_status` grammar,
  journeys.rs:3196-3226) so the per-row buttons instantiate.

### 2.2 Half (b) — the coverage scan

A pure-Rust audit (no walk, no strokes), run for the same
fixtures mounted + `advance_time(0)`:

1. Root `ItemRc` = `ItemRc::new_root(WindowInner::from_pub(
   handle.window().window()).component())`.
2. `visit_descendants` (materializes repeaters + conditionals):
   for each `ItemRc`, `element_type_names_and_ids` over the
   element split; classify `TouchArea` elements by their enclosing
   widget's component-qualified type chain.
3. A `TouchArea` is **interactive** iff
   `downcast::<items::TouchArea>()` succeeds **and**
   `.clicked.has_handler()` is true **and** `enabled()` is true.
   Disabled click targets (the dropdown's F14 empty state,
   disabled segments) are not stops and not violations.
4. **Interactive ⇒ stop coverage**: the element's enclosing-item
   ancestor chain (via `parent_item(StopAtPopups)`, dedup'd) must
   contain a stop element **or** the recorded exemption must match.
   The stop set = enumerated `FocusScope`s the surface registered
   (identified structurally: enclosing-component type chain + the
   registration order), which is exactly the set half (a) walks.
   Geometric containment over the enumerated set: each interactive
   TouchArea's `geometry()` (mapped to window space) must sit
   inside exactly one registered stop's rect (the siblings-overlap
   fact; overlaps between stops are a registration error the
   assert names). **Hybrid completion for the pointer-only
   residual**: any interactive TouchArea neither exempted nor
   inside a stop rect must lie in an **excluded band** — outside
   every registered stop rect AND inside no `FocusScope`-bearing
   item's bounds (each checked by the same `is_visible` arithmetic
   the runtime uses, `absolute_clip_rect_and_geometry`) — i.e.
   provably unreachable by the tab walk; those become explicit,
   reasoned exemption entries. Anything else fails.
5. The failure message names: the enclosing component chain, the
   element id, geometry, and the exemption registry entries —
   actionable, structural, no text assertions on rendered strings.

The two halves stay separate `#[test]`s; half (b) runs for both
fixtures. The binary declares `mod support;` and pulls
`driver/fixture/guard` only.

### 2.3 The exemption registry (complete today; typed, reasoned)

The rule: a `TouchArea` is *not* a violation when its widget class
has a recorded reason. Registry (enclosing component chain →
reason):

| Enclosing widget (component chain) | Sites | Reason (not a stop because…) |
|---|---|---|
| `IconButton` | icon-button.slint:91 | stop-covered: `focus-surface` is the sibling stop (§2.2 stop coverage, not an exemption — listed to show the idiom, matches rule 4's first arm) |
| `SegmentedControl` (per segment) | segmented-control.slint:91 | stop-covered: `seg-focus` sibling |
| `Slider` | slider.slint:112 | stop-covered after the fix: the new `scope` sibling |
| `SettingsButton` | settings-view.slint:286 | stop-covered after the conversion |
| `Dropdown` (`field-touch`) | dropdown.slint:353 | stop-covered: `field-scope` sibling |
| `TextField` (`eye-touch`/`commit-touch`/`clear-touch`) | text-field.slint:228,284,324 | the field's own `TextInput` is the stop (plain `TextInput` accepts Tab focus, items.rs:1229-1243); the TouchAreas are pointer accelerators of the same cell; the recorded "TouchArea click never moves focus is the feature" comment (text-field.slint:210-213) is the house precedent |
| `MultiLineEditor` (`thumb-touch`, `commit-touch`) | multi-line-editor.slint:341,377 | scrollbar thumb = pointer accelerator over scroll (keyboard scroll exists via the viewport); commit bar ditto (the input is the stop) |
| `TaskCard` (`touch`, `done-hit`) | task-card.slint:94,150 | today/board rows are item-walk surfaces by design (F13 non-goal); the done box's own scope (`:145-150` FocusScope) is its stop where that tier applies |
| `CtaButton` (`cta-hit`), stack header (`header-hit`), `body-hit` (board columns) | board-layout.slint:45,371,308 | AT-operable via the AT action (the `accessible-action-default` path); board is an item-walk surface by design — the §4.1-equivalent guarantee is recorded as out of scope with the round plan's non-goal |
| `add-task-hit` (+compact), `add-col-hit` | board-layout.slint:586,669,876 | stop-covered (`add-task-focus`, `add-col-focus` siblings) |
| board-bar / compact switcher `segment-hover` | board-bar.slint:178, board-layout.slint:763 | hover-cue owners over the `tab`/`segment` FocusScope stops (sibling-covered) |
| column `divider` | board-layout.slint:629 | pointer accelerator over keyboard-owned verbs (`stack.wider/narrower` — the in-code comment is the reason verbatim); no click verb exists to walk to |
| `StatusBar` rows (`touch`), timers chip, `stop-touch`, tracking banner | status-bar.slint:146,249,300 | row `touch` is sibling-covered (`row-scope`); the chips deliberately reject the FocusScope (the D-T16 recorded deviation — a focused chrome FocusScope is a key sink; the in-code comment is the reason) — the Escape-hazard exemption |
| `Toolbar` `inbox-chip` | toolbar.slint:77 | same D-T16 class: chrome chip, AT-operable, pointer-only by the recorded deviation; the menu button beside it is the row's stop |
| `info-tip` `touch` | info-tip.slint:54 | pure hover trigger (no `clicked` handler — fails the `has_handler` predicate; listed for completeness) |
| `tracked-today` `total-touch` | tracked-today.slint:53 | tooltip-only hover owner (no `clicked`) |
| overlay dismissers (`focus-overlay`, `search-palette`, app-window underlays, `timer-setter` backdrop, `date-picker` backdrop) | app-window.slint:566-1075, focus-overlay.slint:103,113, search-palette.slint:64, timer-setter.slint backdrop, date-picker.slint:197 | modal dismissal surfaces: intentionally outside the tab walk (a walk must not tab into a dismissal backstop); their controls are stops inside the modal |
| `quick-list-flyout`/`dropdown-flyout` `row-touch`, `action-menu` `row-hit`/`back-hit`, `track-entries` rows | quick-list-flyout.slint:133, dropdown-flyout.slint:78, action-menu.slint:129,172, track-entries.slint:319,425 | flyout/menu rows are keyboard-operated through their surface's ladder (arrow + Return, the D-ladder contexts), not by tab stops — the recorded flyout keyboard grammar |
| `plan-next-day` box/pool `*-touch` | plan-next-day-view.slint:100,148,181,214,273 | sibling-covered (`box-activate`/`earlier-hit`/`later-hit`/`remove-hit`/`pool-hit` FocusScopes) |
| `rule-list` `touch` | rule-list.slint:56 | sibling-covered (`chip-focus := ActivationStop`) |
| `task-view` cell touches (`done-touch`, `title-touch`, `due-touch`, `scheduled-touch`, `description-touch`, `total-touch`) | task-view.slint:348-886 | sibling-covered (the r4 ActivationStops); `total-touch` is tooltip-only |
| `timer-setter` `commit-hit`, `timers` internals | timer-setter.slint:75 | modal form; commits ride Return in the modal context (the D-ladder), the TouchArea is the click twin |

Every exemption carries its one-sentence reason in code, directly
above its registry entry (the stage-3 mandate's "each entry names
why"). New interactive TouchAreas fail by default; adding an
exemption is a code-review-visible event.

### 2.4 The self-guard (the acceptance criterion's mechanism)

The scan must fail when coverage regresses. Proven inside the
binary itself: a `#[test]` that mounts a **deliberate negative
fixture** — a `walk_probe.slint` addition (`CoverageNegativePanel`:
one interactive TouchArea with no sibling FocusScope, no
exemption) instantiated in a tiny host window — and asserts the
scan reports it. This is the mutation test made permanent: it
cannot rot with the app views (which the widget fixes keep green)
and proves the scanner live on every run. A second, positive
self-test scans the probe window's covered panel shape and
asserts zero findings (guards against a scanner that only ever
says "fail").

---

## 3. The widget fixes

### 3.1 `SegmentedControl` (F19a)

Copy the IconButton grammar per segment (segmented-control.slint):

1. Container border: `border-width: 0` stays on the group;
   **per-segment ring**: the segment `Rectangle` grows
   `border-width: seg-focus.has-focus ? Theme.focus-ring-width : 0;
   border-color: Semantic.focus-ring;` (the sibling-scope read —
   the always-rendered ring language, icon-button.slint:33).
2. `seg-focus` gains the raw-key half, the ActivationStop guard
   shape (bare keys only):
   ```slint
   key-pressed(event) => {
       if ((event.text == Key.Return || event.text == " ")
               && !event.modifiers.control && !event.modifiers.alt
               && !event.modifiers.shift && !event.modifiers.meta) {
           if (root.enabled-at(index)) { root.picked(index); }
           accept
       } else { reject }
   }
   ```
   `accept` unconditionally (matching IconButton) so a disabled
   segment still absorbs the stroke (its label is dimmed; the pick
   is swallowed — r2 D32 preserved).
3. `seg-touch.clicked` additionally calls `seg-focus.focus()`
   (the click-to-focus seam, icon-button.slint:105 — keyboard
   continues from the clicked segment).
4. Publication: `changed focused` on `seg-focus` writes
   `WalkFacts.view-id = 1; WalkFacts.stop-index = <group-index> * 16 + index`
   when gaining focus. The group needs its registration index as a
   new `in property <int> walk-group-index` (default 0) set at
   settings' instantiation sites; toolbar/timer-setter/timers
   consumers default to 0 (view-id 0 is the task view's address
   space — their publications are inert for the settings walk and
   the facts global is test instrumentation, never consulted by
   the key path).
5. No import change: Slint's builtin key constants (`Key.Return`,
   `Key.Left`, …) live in the global namespace — icon-button.slint:58
   uses `Key.Return` without any import (the precedent).
6. Header comment: the r2 D26 honesty paragraph is replaced by the
   raw-key + ring description (the §Kind-note rewrite).

Double-fire check (stage-3 mandate): `accessible-action-default`
and `key-pressed` are disjoint paths (AT invocation vs a delivered
key) — a single Return cannot reach both (the resolver path and
the widget path are alternative consumers of one event); no guard
needed, the probe journey asserts exactly-once.

### 3.2 `Slider` (F19b)

Slider gains a `FocusScope` overlay (`scope`) — the IconButton
sibling grammar on a widget whose root cannot *be* the scope
(the a11y triple already sits on the root):

1. `scope := FocusScope { width/height: 100%;
   focus-on-tab-navigation: true; }` placed **before** `touch`
   (TouchArea keeps pointer priority by hit-test order).
2. Ring: root grows
   `border-width: scope.has-focus ? Theme.focus-ring-width : 0;
   border-color: Semantic.focus-ring; border-radius:
   Theme.radius-small;` — visible focus at the track's corners.
3. Keys (all through the existing ceremony — D-4.5):
   ```slint
   scope.key-pressed(event) => {
       if (event.text == Key.Left  || event.text == Key.Down) {
           root.gesture-value = root.stepped(-1);
           root.preview(root.gesture-value);
           accept
       } else if (event.text == Key.Right || event.text == Key.Up) {
           root.gesture-value = root.stepped(1);
           root.preview(root.gesture-value);
           accept
       } else if (event.text == Key.Home) {
           root.gesture-value = root.minimum;
           root.preview(root.gesture-value); accept
       } else if (event.text == Key.End) {
           root.gesture-value = root.maximum;
           root.preview(root.gesture-value); accept
       } else { reject }
   }
   scope.key-released(event) => {
       if (event.text in [Key.Left, Key.Right, Key.Up, Key.Down, Key.Home, Key.End]) {
           root.committed(root.gesture-value);
           accept
       } else { reject }
   }
   ```
   Preview mutates the widget-local `gesture-value` (thumb moves,
   no caller write — the r4 D38 mid-drag path verbatim; the
   settings binding `value: Theme.base-scale` is untouched until
   commit); release commits once through the same `committed`
   seam the drag uses — the adapter's
   `settings_scale_released` gate (adapter.rs:5037: out-of-gate
   reverts, in-gate commits with the pre-gesture revert point)
   applies unchanged. `pressed()` is **not** fired by keys: the
   adapter's `scale_drag` revert point only matters for the
   pointer gesture (the commit core receives `previous` from the
   last *pressed* record — see the adapter note below).
4. Focus-loss commit: `scope.changed has-focus` losing focus with
   a pending key gesture (a `property <bool> key-gesturing` set
   true on first key preview, cleared on release) fires the same
   release path once — the field's commit-on-loss grammar
   (text-field.slint's pattern) so Tab-ing away mid-steps never
   silently discards.
5. **Adapter seam note (the one Rust-side consideration, no code
   change predicted)**: `settings_scale_pressed` seeds
   `scale_drag` (the revert point); a keyboard-only gesture never
   presses, so `scale_drag` is `None` at release and
   `settings_scale_released` early-returns (adapter.rs:5041) —
   **keyboard steps would be silently dropped.** The fix lands in
   the widget-caller seam instead of the widget: the settings
   `Slider` instantiation routes `scope`'s key-gesture start by
   firing the existing `scale-pressed()` seam on the first key
   preview (`pressed()` in markup from the key arm: a
   widget-level `key-gesturing` transition calls `root.pressed()`
   once per gesture) — the widget then needs zero adapter
   changes and pointer/keyboard share the exact commit gate.
   *Decision: the widget fires `pressed()` on key-gesture start
   (first preview of a stroke sequence) — one ceremony, two
   input classes, the D38 contract intact.*
6. AT actions unchanged (`accessible-action-increment/decrement`
   → `step-by` → the commit path; r4 D38's recorded asymmetry).
7. Publication: `scope` writes `WalkFacts` view-id 1 /
   `stop-index` 11 via a new `in property <int> walk-stop-index`
   (default 0 = no publication; only settings sets it).

### 3.3 `SettingsButton` → stop (F13a)

Inside settings-view.slint's local component (settings-view.slint:270-299),
the IconButton sibling grammar (D-4.6 — *not* ActivationStop):

- `border-width: touch.has-hover || focus-surface.has-focus ?
  Theme.focus-ring-width : 0; border-color: Semantic.focus-ring;`
- `focus-surface := FocusScope { width/height: 100%;
  focus-on-tab-navigation: true; accessible-role: button;
  accessible-label: root.text; accessible-enabled: root.enabled; }`
  placed before `touch`; the root's existing
  `accessible-role/label` move onto the scope (a11y duties ride
  the stop).
- `touch.clicked` gains `focus-surface.focus()` and keeps the
  `enabled || click-through` gate; the scope gains the raw-key
  half with the same gate:
  ```slint
  key-pressed(event) => {
      if ((event.text == Key.Return || event.text == " ")
              && !event.modifiers.control && !event.modifiers.alt
              && !event.modifiers.shift && !event.modifiers.meta) {
          if (root.enabled || root.click-through) { root.clicked(); }
          accept
      } else { reject }
  }
  ```
- Publication: the scope writes `WalkFacts.view-id = 1;
  WalkFacts.stop-index = root.walk-stop-index;` (new
  `in property <int> walk-stop-index` set at every instantiation —
  the §1 registration numbers).

### 3.4 Settings root parity (F13b)

settings-view.slint:532-585 (D-4.7, exact sibling grammar):

- `focus-on-tab-navigation: true;` + `accessible-role: region;`
  + `accessible-label: "Settings";`
- `init => { root.view-mounted(); self.focus(); }` (the mount
  callback keeps its ordering contract — the adapter re-pushes
  the VM after children initialise; the focus grab appends after
  it in the same handler).
- `ring := Rectangle { width/height: 100%; border-width:
  root.has-focus ? Theme.focus-ring-width : 0; border-color:
  Semantic.focus-ring; }` declared first (z-under the content,
  the sibling views' pattern).
- Publication: the root scope writes `WalkFacts.view-id = 1;
  stop-index = 1` on focus gain (the walk's first reported pair).

Mount-focus parity needs no adapter change: the view swap path
(`refocus_view` → `key-root`) coexists with the fresh-mount init
grab exactly as today's views demonstrate; the P5 negative
(conditionally-mounted init grabs dropped on the *then-mounted*
tree) does not apply — a view mount instantiates a fresh
component whose init runs attached (today/board/review journeys
are the standing proof).

### 3.5 The task-view latch copy stays (decision)

task-view.slint:290-313 carries a verbatim copy of the
ActivationStop publish/latch grammar (the in-code comment mandates
keeping the copies in sync). Migrating it into `ActivationStop`
would need the done box's `AccessibleRole.checkbox` to come from a
property — and `ActivationStop`'s header records the runtime
constraint that `accessible-role` rejects non-constant values
(activation-stop.slint:20-23). A role-parameterization experiment
is exactly the trap class that constraint documents, and the
duplicated latch is already a recorded, commented debt outside
this chunk's findings (F13/F15/F19). **Decision: leave the copy
untouched this chunk; the retirement needs the role-binding
experiment as its own gated probe.**

### 3.6 Publication lines at non-widget sites (settings only)

- `Back` IconButton: the generic widget grows the optional
  publication pair (`in property <int> walk-view-id;
  in property <int> walk-stop-index;`, default 0/0 = silent)
  with the two-line publication in `focus-surface`'s
  `changed focused` — default zero keeps every other consumer
  silent; settings' instantiation sets view-id 1 / index 2.
- The settings `Dropdown` instantiation (settings-view.slint:414)
  sets the new `Dropdown.walk-view-id/walk-stop-index` pair (the
  same optional-publication pattern inside `dropdown.slint`'s
  `field-scope`).
- `LabeledField`'s TextField instantiation sites (4+4 fields):
  settings sets the pair per field (§1 indices).

The pattern is uniform: **widgets that are stops publish when
their walk ids are non-zero; the facts global is never read by
the shipped key path** (test instrumentation only, the
TaskViewWalkFacts precedent).

---

## 4. Journeys & probes (beyond the invariant binary)

1. **`tests/journeys.rs` — the segmented raw-key + ring journey**
   (mount settings via the existing grammar; label-address the
   keymap group by its `group-label` a11y label —
   `find_by_label("Keymap preset")` → the radio-button element →
   `click_at` its centre): assert
   `SettingsModel.preset-index` unchanged under a *disabled*-segment
   click (the backend segment), then focus the segment (real
   click), `stroke(Return)` → typed request
   `UiRequest::SetKeymapPreset` observed on the fake engine lane
   (the typed-outcome rule), `stroke(Space)` ditto; the ring is
   the pixel probe's job (below), the journey pins activation
   only. Also the vi-preset leg: switch preset to Vi, re-focus
   the segment, Return still activates (vi's global Return does
   not steal — the D-4.6 non-publication verified behaviorally).
2. **`tests/journeys.rs` — the slider keyboard journey** (the
   r4 `settings_scale_*` grammar at journeys.rs:3441-3538):
   focus the slider (real click at its centre), press Right ×2
   with `advance_time(0)` between → **no** request and **no**
   `Theme.base-scale` change (preview is widget-internal — the
   typed-outcome rule: the fake lane stays empty). Then release →
   exactly one `UiRequest::SetScale` with the stepped value.
   Harness fact the journey rests on: the driver's
   `press_stroke` dispatches only `KeyPressed`; the slider
   commits on `key-released`, so `tests/support/driver.rs` grows
   one method (`release_stroke`, dispatching `KeyReleased` for
   the stroke's key after a preceding press — the walk probe's
   raw `key()` helper is the in-repo precedent) in the sanctioned
   churn-isolation seam. Further legs: a second release with no
   new presses fires nothing (no commit without preview); a
   Tab-away mid-steps commits once (the focus-loss rule).
   Asserts stay typed (lane payloads + `Theme.base_scale` after
   the fake engine's echo).
3. **`tests/usability_components.rs` — the ring pixel floors**
   (the "visibly focused" acceptance criterion, the house
   pixel-floor idiom): mount settings at scale 1.0/1000 px,
   focus a keymap segment / the slider / Save (real clicks),
   snapshot, and floor the ring ink inside the element's rect
   (`count_off_background` ≥ floor against the raised-surface
   bg) — three focused elements, one test; an unfocused
   control's rect floors at ~0 ink for contrast... the
   no-quality-ceiling rule: assert only the *floor when
   focused*; the unfocused leg asserts the *floor is strictly
   smaller* than the focused one (a floor-vs-floor comparison,
   never a ceiling).
4. **`tests/walk_probe.slint`/`_tests.rs` — P6 (raw keys on a
   FocusScope) + the coverage-negative panel**: P6 pins that a
   delivered Return reaches a plain FocusScope's `key-pressed`
   (the mechanism under 3.1/3.3 — currently no probe covers
   raw-key delivery to a scope; the journeys would catch a
   regression but the probe pins the *mechanism*); the
   `CoverageNegativePanel` feeds §2.4's self-guard.
5. **The inventory walk stays pinned**: journeys.rs:6473-6478's
   expected vector is untouched — the task view's stop set does
   not change this chunk (§3.5 leaves the done box's grammar in
   place; no task-view edits). The only journeys.rs churn is the
   two new journeys plus the driver helper usage.

---

## 5. TDD sequencing & commits

Each commit standalone (rebase-merge hygiene, AGENTS §8):

1. **`test(ui-slint): the focus-coverage invariant (red)`** —
   `tests/focus_coverage.rs` (both halves + self-guards) +
   `tests/support/walk.rs` + the driver's release helper +
   the walk-probe P6/negative additions. Expected: half (b)
   red on settings (SettingsButton/Slider classes), half (a)
   red on the settings walk (no publications), the probe
   self-guards green, the task view untouched. This commit's
   red is the documented finding inventory (stage-2 reports
   it).
2. **`feat(ui-slint): the segmented control's focus ring and raw keys`**
   — §3.1 (+ the toolbar/timer consumers untouched by default).
   Half (b)'s segment class and the segment journey go green.
3. **`feat(ui-slint): the slider's keyboard ceremony`** — §3.2.
   The slider class green; slider journey green.
4. **`feat(ui-slint): the settings button becomes a walk stop`**
   — §3.3. SettingsButton class green.
5. **`feat(ui-slint): the settings view's mount-focus parity`**
   — §3.4 + §3.6 publications (Back/Dropdown/field pairs).
   The settings walk (half a) green end-to-end; ring pixel
   floors green.
6. **`test(ui-slint): the segmented and slider keyboard journeys`**
   — journeys 1–2 + pixel floors if not already in 5 (keep
   5 markup-only; this commit is the behavioral tier).
   Full green.

Gate order at handoff: `scripts/run-quality-gates.sh` wrapping
the §10 suite (fmt, clippy `-D warnings`, nextest, doc tests,
deny, llvm-cov + quarantine + report). The new binary must be
nextest-compatible (process-per-test: the driver's
platform-per-process contract — the existing journey binaries
are the pattern).

## 6. Test-quality compliance (AGENTS §5 map)

- **No text assertions**: all walks assert typed pairs/indices;
  activation asserts lane payloads (`matches!` on
  `UiRequest` variants); the scan asserts structural facts.
  Labels appear only as *addresses* (the driver's documented
  convention).
- **Floors not ceilings**: ring ink is a floor; the walk's
  cycle-length deadline is a structural bound (the walk is
  finite by construction), not a success ceiling; the scan's
  exemption registry is an explicit allowlist, never a count
  cap.
- **Deterministic**: every step lands via `advance_time(0)`
  handler passes on the mock clock; no sleeps; the walk
  terminates on the observed wrap (a property of the traversal,
  not the wall clock).
- **One behavior, one test, lowest layer**: raw-key *mechanism*
  pins in the probe (P6); *widget* behavior in the journeys;
  *coverage* in the invariant binary; the r4 inventory journey
  stays the task view's order pin (no duplication).
- **The invariant can't pin accidents**: half (a) asserts set
  membership + monotone order, never a literal vector; half
  (b) is a property (every interactive element covered), which
  any correct markup satisfies.
- **A new test failing can be a bug**: stage 2 treats initial
  reds as findings first (the plan §2 predicted red list is a
  hypothesis to verify, not a conclusion).

## 7. Acceptance checklist (dispatch §5 mapped)

- [ ] Tabbing through settings shows a ring on every segment and Enter/Space activates (journey 1 + pixel floors).
- [ ] The slider takes focus and steps with arrows (journey 2); commit only on release/focus-loss (r4 D38 honored — stage-3 review re-verifies "no live persistence per step" via the empty-lane mid-steps assertion).
- [ ] Save/Test/Bind/Remove/Detach are stops (walk half (a) registration + half (b) coverage).
- [ ] The invariant fails when a segment's FocusScope is removed (the stage-3 mutation drill: delete `seg-focus`, run the binary, expect the segment-class finding, restore).
- [ ] The invariant passes across all views (settings + task detail fixtures; the self-guards prove the scanner live).
- [ ] `journeys.rs:6424+` (the task-view inventory) passes unchanged.
- [ ] Segmented raw keys do not double-fire with `accessible-action-default` (journey 1's exactly-once assertion).
- [ ] Settings traversal order remains tree/reading order without newly skipped stops (half (a)'s monotonicity + the unchanged unreported-step classes).
- [ ] All gates green via `scripts/run-quality-gates.sh`.

## 8. Risks

- **R1 — the scan's `ItemRc` API surface shifts on a Slint bump**:
  mitigated by the probe-crate confinement (one file), the
  exact dev-pin (pin_guard.rs holds the testing backend; the
  core pin rides `=1.18.1` in the root manifest), and the
  module doc stating the coupling. The scan's needs are
  conservative (tree walk + names + geometry + downcast — all
  stable across 1.17→1.18).
- **R2 — hit-test z-order vs the scan's geometry rule**: an
  interactive TouchArea *under* a later sibling's stop (painted
  over) is reachable by walk but not by click. The scan asserts
  walk coverage; a buried-but-covered TouchArea is not a
  finding (walk coverage is the invariant; click reachability
  is the pointer tier's concern). Recorded so stage 3 doesn't
  "fix" it into a false positive.
- **R3 — the `geometry()` read mid-frame**: the scan runs after
  `advance_time(0)` on a settled tree (the journeys' standing
  discipline); a repeater row whose layout is unresolved reads
  a zero rect and would false-positive — mitigated by the
  P5-verified fact that the mock clock's instantiation pass
  resolves layout, plus the driver's existing geometry reads
  (the drag journeys) rely on the same settle.
- **R4 — publication defaults leak**: a widget shipping with a
  non-zero default walk id would pollute the facts global in
  production. Every publication is default 0/0 = silent, and
  the facts global is unread by the key path (grep-assertable:
  the scan's guard test asserts `WalkFacts` appears in no
  `src/` file — a source scan like seam_inventory's, one
  #[test]).
- **R5 — the focus-loss commit on the slider double-fires with
  the release commit** (Tab pressed while a stroke is held —
  physically impossible; programmatically guarded by the
  `key-gesturing` latch clearing on release first). The latch
  is the mitigation; the journey's second-release assertion
  pins it.
- **R6 — `Key.Down` in a task-detail-mounted slider would
  collide with the item walk** (TaskDetail binds Up/Down).
  No task-view Slider exists today; the widget's key arms are
  still safe (the capture resolver consumes Down *before*
  delivery there — the widget never sees it). Recorded as a
  known interaction for future consumers (the settings surface
  is unaffected).
- **R7 — settings' stop enumeration drifts as chunks 5/8 edit
  the view**: the registration is tree-order-derived, not a
  pinned vector (§1) — new sections re-derive indices
  mechanically; the invariant re-derives coverage structurally.
  Chunk 8's account cards inherit the grammar
  ("full keyboard operability via the chunk-4 stop grammar" —
  origin §4.8) by adding stops + walk ids, exactly this plan's
  pattern.

## 9. Estimated diff size

- Markup: `segmented-control.slint` (+25), `slider.slint` (+55),
  `settings-view.slint` (+45: SettingsButton conversion, root
  parity, publication pairs at instantiation sites),
  `icon-button.slint` (+8 publication pair), `dropdown.slint`
  (+8), `walk-facts.slint` (+15 rewrite).
- Rust: `tests/support/driver.rs` (+25 release helper),
  `tests/support/walk.rs` (+120 new), `tests/focus_coverage.rs`
  (+380 new binary incl. the registry), probe files (+90),
  journeys (+150), usability_components (+60).
- Total ≈ **+980 lines, 0 deletions beyond the rewritten
  walk-facts header comment and the segmented-control honesty
  paragraph**; no production Rust changes predicted (the
  adapter's scale seams serve the keyboard ceremony unchanged).
