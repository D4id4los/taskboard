# [Plan] UI R4 chunk 4 — hover machinery fixes (the tooltip empty line, the total dismiss seam, the hints-at-start investigation)

- **Date**: 2026-11-01 (stage-1 plan; anchored on `origin/main` @
  `67c0b531` — chunks 1–3 landed before this plan was written).
- **Status**: PLANNED — stage-1 plan for chunk 4 of GUI review round 4.
  No §2 decision gate binds this chunk (the round-4 plan's confirmed
  decisions 1–4 touch chunks 3/7/8/9/10, not 4); no PROPOSED residue.
- **Origin** (normative): round-4 plan §3.4 (D13 empty description
  line, D14 ghost tooltips on transitions, D15 hints-at-start
  investigation) + §1 findings F4/F5/F14 + §4 (investigation-item
  testing rule); dispatch plan §1 (ordering — chunk 4 has **no
  upstream gate**, parallel lane with chunk 3, which has landed) and
  §5 (chunk-4 acceptance (a)–(d) + prompts).
- **Branch**: `fix/ui-r4-hover-fixes` (this worktree:
  `../taskboard-WT-fix-ui-r4-hover-fixes`, created off `origin/main`
  `67c0b531` after `scripts/worktree-doctor.sh` passed — 230 G free,
  floor 25 G). Local `main` was 38 commits behind `origin/main` at
  worktree creation; the branch tracks the fetched tip.
- **Scope**: `crates/taskboard-ui-slint` only — `ui/widgets/tooltip.slint`,
  `src/adapter.rs`, `src/view_state.rs`, `src/models.rs` (one stale
  comment), `tests/` (probe + journeys). **Empty diff outside
  ui-slint** (stage-3 review criterion (e)).
- **Non-goals** (dispatch §5): no task-view hover coverage (chunk 6,
  D21's sweep); no tooltip content/wording changes beyond the layout
  fix; no delay/placement changes (`TOOLTIP_DELAY_MS`, `src/tooltip.rs`
  `place` untouched — acceptance (d)); no new hint sources beyond the
  keymap projection (the D19 rule holds).

## 1. The three findings, mapped to mechanisms

| Finding | User report | Mechanism this plan settles |
|---|---|---|
| F5 | hover boxes render an empty line when the description is empty | The tooltip panel's always-mounted `visible: false` rows still reserve layout space on the pinned Slint 1.18.1 (§2) |
| F14 | a hover message survived the board→edit-view transition; view transitions must not leave ghosts | Only a *named subset* of effects dismisses the tooltip; the board→task transition (`Activate` under `Flow::None`) is not in the set, and an anchor that unmounts while hovered never fires `unhover` (§3) |
| F4 | keyboard-shortcut hints missing from hover boxes early in the session, appeared later — investigate | Investigation item (§4): four design-round hypotheses, three pre-rejected by code evidence below, the repro protocol decides the verdict |

All `file:line` anchors below are on the branch base `67c0b531`.

## 2. D13 — the empty line (F5)

### 2.1 Static evidence: the probe's predicted verdict (recorded now, measured in stage 2)

The design round hedged ("under the pinned runtime the hidden rows
**can** reserve space — stage 1 records the probe"). The pinned
runtime's own sources decide the mechanism; the probe measures and
records the numbers.

1. **`visible` does not remove an element from layout.** The compiler
   lowers every `visible` *binding* into a wrapper `Clip` element
   (`i-slint-compiler-1.18.1/passes/visible.rs` — `handle_visible`
   wraps each bound child in `create_visibility_element`, setting
   `clip: !visible`); the wrapper stays a child of the parent.
   `Clip::layout_info` (`i-slint-core-1.18.1/items.rs`, the `Clip`
   impl) does not recurse into children — and, decisively, the
   **generated layout code never sees the wrapper at all**: the
   compiled `VerticalLayout` of the current `TooltipPanel`
   (generated `app-window.rs`, `solve_box_layout` cells) lists
   `caption`, `description_text`, `hint_row` directly, with **no
   visibility term anywhere in the cells**.
2. **An empty wrapped text's preferred height is one line.** The
   cell's vertical layout info for `description-text`
   (`i-slint-core-1.18.1/items/text.rs`, `text_layout_info`,
   `Orientation::Vertical`, `TextWrap::WordWrap` branch) is
   `implicit_size(Some(width), WordWrap).height` — for `text: ""`
   the renderer still measures one (empty) line at the caption role's
   line height.
3. **Consequence**: a tooltip with an empty description renders
   caption + one empty line (the hidden row's line height) + spacing;
   the empty `hint-row` contributes 0 preferred height (an empty
   `KbdHint` layout has no children) but still costs its spacing
   slot. The most common tooltips — task cards, stack headers, board
   segments — all pass `description: ""` (`task-card.slint:92`,
   `board-layout.slint:391`, `board-bar.slint:122`), so F5 fires on
   nearly every hover.

**Predicted verdict for the probe: "hidden rows reserve space — the
description row exactly one caption line, the keyless hint row only
its spacing slot."** Stage 2 measures the same fact through the
testing backend and records the numbers in the stage-2 report; if the
measurement *contradicts* §2.1 (the renderer returns 0 height for
empty text), the conditional-element fix below is still correct (it
removes the row from layout by construction) and §2.1 gets corrected
in the report — the fix does not depend on which way the probe lands.

### 2.2 The probe (dispatch acceptance (a): "probe first, then the conditional-element fix — probe + layout pin")

A composition probe on the established pattern
(`tests/icon_probe.slint` + `icon_probe_tests.rs` from chunk 3,
`slider_grab_probe_tests.rs` before it):

- `tests/tooltip_layout_probe.slint` — a probe window hosting the
  **real `TooltipPanel`** (not a copy) inside a fixed frame, plus a
  control column that drives `TooltipState.hover(...)` through the
  four content states; the panel's existing size hand-off
  (`tip-width`/`tip-height` mirrors, `tooltip.slint:36-39`) is the
  geometry oracle — no new measurement seam needed.
- `tests/tooltip_layout_probe_tests.rs` — the four-state matrix
  (dispatch stage-3 criterion (b): "layout pin both ways"):
  1. **neither** (label only — the F5 repro): `tip-height` == the
     caption-only height (one line + padding) — **red on today's
     code**, green after the fix (the TDD-red shape);
  2. **description-only**: `tip-height` == caption + description
     lines + one spacing — strictly greater than (1);
  3. **keys-only**: strictly greater than (1), and the relation of
     `the_tooltip_width_includes_the_hint_row_and_keeps_the_cap`
     (journeys.rs) keeps holding: width with keys > width without;
  4. **both**: strictly greater than (2) and (3) by the added
     content.
  All assertions are structural/relational (heights, orderings) —
  no font-metric pins, no text assertions (AGENTS §5).
- The probe runs on the testing backend like the existing probes;
  the live-renderer confirmation rides the D15 repro script's
  screenshots (§4.2) at zero extra cost.

### 2.3 The fix: conditional elements + off-layout measurers

`tooltip.slint`'s own comments name the reason the rows are
always-mounted: the panel `width` binding reads
`description-text.preferred-width` and `hint-row.preferred-width`
(`tooltip.slint:98-117`, the r2 D14 width terms), and "a conditional
element cannot be referenced from outside its branch". The fix must
therefore replace the *width sources*, not just the rows:

1. `description-text` and `hint-row` become conditional elements in
   the `VerticalLayout`:
   `if (TooltipState.description != "") : RoleText { ... }` and
   `if (TooltipState.keys.length > 0) : KbdHint { ... }` — invisible
   rows occupy no layout space by construction (D13's own wording).
2. Two **measurer** elements move out of the layout — direct zero-
   geometry children of the panel `Rectangle`, never laid out, never
   visible:
   `description-natural := RoleText { text: TooltipState.description; wrap: word-wrap; visible: false; width: 0px; height: 0px; }`
   and the same shape for `hint-natural := KbdHint { keys: TooltipState.keys; visible: false; }`.
3. The pure functions keep their signatures and semantics
   (`tooltip.slint:108-117`): `description-natural()` reads
   `description-natural.preferred-width` (0 when empty — the guard
   stays), `hint-natural()` reads `hint-natural.preferred-width`
   (0 when keyless). The P5 facts are unchanged: a word-wrapped
   `RoleText`'s preferred width is its unwrapped natural width, so
   the measurers return exactly the numbers the in-layout rows
   returned — the panel width formula
   (`tooltip.slint:98-102`) is untouched otherwise.
4. `caption` stays an unconditional layout child (a tooltip never
   exists without a label — `hover()` returns early on an empty
   label, `tooltip.slint:51-55`).

Recorded non-risk: conditional elements re-mount on content change
(i.e. per hover); the panel is a display-only overlay mounted once
per show with static content — no churn concern. The ADR comment
block at `tooltip.slint:86-117` is rewritten to describe the measurer
pattern (why the measurers exist off-layout, the P5 citations kept).

## 3. D14 — the total dismiss seam (F14)

### 3.1 The enumerate-once inventory (every transition that unmounts a tooltip's anchor)

The dismiss sites today (adapter.rs on this base):
`settle_flows_on_leaving` (adapter.rs:826-828, fed by the `leaving`
match at adapter.rs:920-931: `NextView | PrevView | FocusBoard |
FocusToday | BoardNext | BoardPrevious | BoardSelect(_)`), the
`OpenSettings` toggle arm (adapter.rs:1126), the `OpenMenu` arm
(adapter.rs:1614), and `apply_close_menu` (adapter.rs:1781). Against
the full inventory of mounted-view transitions (`view_state.rs`
assignments of `self.view` + the board/subject swaps):

| Transition | View-state writer | Effect(s) reaching it | Dismissed today? |
|---|---|---|---|
| View cycle (Today/Board/StackDetail/TaskDetail) | `step_view` (view_state.rs:515) | `NextView`/`PrevView` | yes (`leaving`) |
| → Board | `focus_board` (view_state.rs:583) | `FocusBoard` | yes (`leaving`) |
| → Today | `focus_today` (view_state.rs:590) | `FocusToday` | yes (`leaving`) |
| Board content swap | `select_board` (view_state.rs:659) | `BoardSelect`/`BoardNext`/`BoardPrevious` | yes (`leaving`) |
| → Settings, and back | `open_settings`/`close_settings` (view_state.rs:502-513) | `OpenSettings` (toggle, both directions) | yes (toggle arm) |
| **→ TaskDetail via card/task activation** | `activate_focused` (view_state.rs:1074) | `Activate` under `Flow::None` | **NO — F14's exact repro** |
| **→ StackDetail via the inbox jump** | `open_inbox` (view_state.rs:1456) | `OpenInbox` | **NO — missing from `leaving`** |
| → Board/StackDetail via the palette invoke | `show_task` (view_state.rs:603) / `palette_jump` (view_state.rs:1470) | `ShowTask` under `Flow::Palette`, `InvokeSelected` → `apply_palette_invoke` (adapter.rs:1739) | **NO** |
| → kiosk flip | `enter_kiosk_mode` (view_state.rs:1356) | not an effect — `rotation_tick` (adapter.rs:2285) | **NO** |
| The add-task / add-stack ghost replaced by its create row | `begin_create`/`begin_create_stack` | `BeginCreate`/`BeginCreateStack` | **NO — same root cause: the hovered TouchArea itself unmounts** (board-layout.slint:619-634) |
| Menu open/close | — | `OpenMenu`/`CloseMenu` | yes (modal rule, r2 D13) |
| Palette open / capture open | — | `OpenPalette`/`BeginCapture` | **NO** (the modal-takeover half of the r2 D13 rule statement, adapter.rs:510-514, is applied only to the menu) |

The root cause, per the design round: an anchor that unmounts while
hovered never fires `changed has-hover => unhover()` — the callback
dies with the element. Every row above shares it; the fix must not
depend on per-arm memory.

### 3.2 The fix: a view-identity diff at the one effect choke point — total by construction

Every `UiEffect` passes through exactly one method, `apply_effect`
(adapter.rs:1600): the table effects route to `interpret`
(adapter.rs:1775 tail), the menu family and picker land in named
arms. The dismiss therefore stops being a per-arm call and becomes a
property of the pass:

1. **`ViewState::view_identity()`** (new, pure, `view_state.rs`):
   `(view, board, subject_stack, subject_task)` — the identity of
   what the main view mounts. `selected_stack`/`focused_task` are
   deliberately *not* members: a selection/focus step leaves every
   card mounted, and dismissing on focus steps would kill tooltips
   during ordinary pointer hover (the over-fire guard; §6-R2).
2. **The choke point** (`apply_effect`): snapshot
   `let before = self.view_state.view_identity();` at entry; at the
   tail, `if self.view_state.view_identity() != before {
   Self::dismiss_tooltip(window); }`. Any effect — present or
   **future** — whose application swaps the mounted view, board, or
   subject dismisses the tooltip, regardless of which table arm or
   resolver re-dispatch did it. Nothing can be forgotten, because
   there is nothing left to remember.
3. **The per-arm dismisses that remain** (each with its own reason,
   all idempotent under the diff):
   - `OpenMenu`/`apply_close_menu` keep theirs — the modal-takeover
     rule (a tip must not survive *into* a modal), not a view swap;
   - `OpenSettings`' arm call becomes redundant (the diff covers both
     toggle directions) and is removed — one mechanism, one site;
   - **`BeginCreate`/`BeginCreateStack` arms** (via
     `interpret_begin`) get the explicit call — the ghost case: the
     identity tuple does not move, but the hovered ghost itself
     unmounts by construction of the flow it opens;
   - **`OpenPalette`/`BeginCapture`** get the explicit call in their
     table arms — the modal-takeover half of the r2 D13 rule
     (adapter.rs:510-514's own sentence "or a modal context takes
     over"), which today only the menu implements;
   - **`rotation_tick`** (adapter.rs:2285) gets the call — the kiosk
     flip swaps the view outside `apply_effect` entirely; cheap
     totality even though a passive kiosk display rarely hovers.
   - `settle_flows_on_leaving` keeps `dismiss_tooltip` (it also fires
     from non-effect settle paths) — or, if stage 2 finds it fully
     subsumed by the choke point on every path that matters, it may
     drop to a comment pointing at the choke point; the per-path
     analysis is recorded either way.
4. **No `leaving`-set growth**: the `leaving` match
   (adapter.rs:920-931) keeps its current membership (it drives the
   *settle* half) — the *dismiss* half moves to the choke point.

### 3.3 The structural test (dispatch acceptance (b): "walks every view-swapping effect arm and asserts the dismiss routing; no default arm to hide behind")

One test, two exhaustive lists — compile-enforced totality:

- **The lists** (the test's own exhaustive `match` over `UiEffect`,
  no wildcard — a new variant is a compile error until it is placed):
  - `identity_swapping(effects...)`: `NextView`, `PrevView`,
    `FocusBoard`, `FocusToday`, `OpenInbox`, `BoardNext`,
    `BoardPrevious`, `BoardSelect`, `Activate` (the `Flow::None`
    activation), `OpenSettings`, `ShowTask` (under the palette flow),
    `InvokeSelected`;
  - `anchor_or_modal_dismissals(effects...)`: `OpenMenu`,
    `CloseMenu`, `BeginCreate`, `BeginCreateStack`, `OpenPalette`,
    `BeginCapture`;
  - `identity_preserving(effects...)` — the explicit survive set, the
    over-fire guard: `FocusNext`, `FocusPrev`, `FocusFirst`,
    `FocusLast`, `ControlNext`, `ControlPrev`, `ScrollUp`,
    `ScrollDown`, `StackNext`, `StackPrev`, `StackWider`,
    `StackNarrower`, `RequestMove`, `CancelMove`, `SetMode(_)`,
    `BeginTitleEdit`, `PickerKey(_)`, `MenuBack`, `MenuNext`,
    `MenuPrevious`, `Exit`, and the lifecycle verbs
    (`StackDelete/StackRename/StackMove/BoardCreate/BoardDelete/BoardRename`
    — they arm flows or send commands; the engine's own content
    changes ride the refresh, the *considered-and-excluded* class
    below).
- **The behavioural walk** (same test, after the lists): for each
  effect in the first two sets, mount the harness, arm a *visible*
  tooltip (the `tooltip_state` harness + the delay fire, the
  established `a_visible_tooltip_does_not_survive_a_view_switch`
  pattern), apply the effect through `interpret_for_test` with the
  effect's precondition (a focused task for `Activate`, a second
  board for `BoardNext`, a palette flow for `ShowTask`, …), and
  assert `!visible && !pending && !placed`. For each effect in the
  survive set, the same setup asserts the tip **survives** — proving
  the choke point doesn't over-fire.
- **The engine-event exclusion** (recorded, not tested): a task
  deleted by a *sync/engine* event unmounts an anchor through
  `refresh`/`reconcile`, not through an effect — out of D14's scope
  (the design round's enumerate list is effect-driven); the next
  pointer movement re-arms or dismisses naturally. Recorded here so
  the exclusion is a decision, not an oversight.
- **Resolution rule for expectation conflicts** (§6-R2): if a
  listed-survive effect trips the identity diff, either the effect
  genuinely unmounts the anchor (move it to the dismiss list — a
  test-found bug in the list, the AGENTS §5 "new test failing can be
  a bug" rule) or the tuple carries a spurious member (fix the
  tuple). Either way the outcome is recorded in the stage-2 report.

### 3.4 The journeys (the user-facing pins)

1. **The F14 repro verbatim** (TDD-red first): hover a task card →
   delay fires (visible) → click the card (activate) → TaskDetail
   mounts → the tooltip is gone. Today this fails — the ghost tooltip
   rides into the task view.
2. **The inbox jump**: hover → visible → `OpenInbox` → StackDetail →
   gone (the currently-missing `leaving` arm).
3. **The palette invoke**: open the palette, hover a row (armed),
   invoke a Task target → the board mounts → gone (covers
   `apply_palette_invoke` through the choke point).
4. **The ghost**: hover the add-task ghost → visible → click → the
   create field opens → gone (the BeginCreate arm).
5. **The kiosk flip**: a rotation tick from a visible tip → gone
   (placed beside the existing kiosk rotation journeys).

## 4. D15 — the hints-at-start investigation (F4)

### 4.1 The hypotheses, sharpened by stage-1 code evidence

| # | Hypothesis (design round) | Stage-1 evidence | Standing |
|---|---|---|---|
| H-a | chain masking at mount | The preset mask sets are exactly `{Menu, Palette}` (presets.rs:52-58, all three presets) — `ViNormal` never masks, so the mount chain `[Board, ViNormal, Global]` (synced before the first push, adapter.rs:538/701) hides nothing | **pre-rejected** (code), repro confirms |
| H-b | preset-unbound actions rendering keyless | **`BoardSelect` is bound in no preset** (zero rows in presets.rs) — the board-bar segment tooltip (`models.rs` pushes `first_binding_display(ActionId::BoardSelect, …)`) can *never* show keys, on every preset, for the whole session. Under emacs every *other* projected action (AppMenu, NavFocusBoard, TaskEdit, StackWider, StackCreate, TaskCreate) is bound (emacs table). The models.rs:903 comment "empty under emacs until chunk 3 binds it" is **stale** — `StackCreate` = `Ctrl+c s` | **live candidate** — permanent, not temporal; the repro decides whether F4 reduces to this |
| H-c | the keymap swapped by the settings reply after mount | The desktop hand-over builds the full keymap **before** mount (`resolve_ui_settings` → `keymap::build` → `UiSettings.keymap`, taskboard-app/src/ui.rs:198-218 → `install`), and the reply swap (`apply_keymap_update`, adapter.rs:2329) fires only from `PendingReply::Keymap` — a settings *save*, not a fresh boot — and already ends in `refresh_forced` (adapter.rs:2240-2245, the audit note names keymap as a non-state derive input) | **pre-rejected** for the fresh-boot repro; revives only if the repro shows hints changing exactly at a settings visit (the review session may have included one) |
| H-d | hint-row visibility interacting with D13's layout | The empty description line (§2) inflates `tip-height`; the placement hand-off (`place_tooltip`, lib.rs) uses the mismeasured height — the keys row can land mispositioned near the status bar, and the *visual* reading of a keycap row under a phantom gap plausibly reads as "missing". The §2.2 probe's numbers + the repro screenshots decide | **live candidate** — the D13 fix is expected to change what F4 shows |

The protocol below is written so the verdict is **evidence, not
consensus**: each experiment discriminates at least one hypothesis,
and the repro's per-element observations map onto the table.

### 4.2 The fresh-boot repro script (Tier-1 live; Tier-4 headless fallback)

Script: `scripts/r4-hover-repro.sh` (new, committed with the chunk;
it documents the method even where it cannot run).

1. **Isolation**: a scratch config
   (`TASKBOARD_CONFIG=$TMP/taskboard.toml` with
   `[ui.keymap] preset = "emacs"` — config.rs:470-479, lowercase
   serde) and a scratch data/XDG dir so the boot is cold;
   `taskboard gui` from a release build. (Stage 2 confirms the
   data-dir env/config knob for full store isolation — Tier-4
   detail, recorded in the script header.)
2. **The sweep** — the named elements, in order, hovered one at a
   time with the pointer parked on each until the delay fires:
   toolbar menu button, board-bar segment, stack header, task card,
   add-task ghost, add-column ghost. For each: screenshot +
   structured dump of `TooltipState` (`text`, `keys` model length,
   `tip-width/height`) — never message-text parsing (the
   first-frame convention, lib.rs:338-342).
3. **The phases**: sweep A straight after the first-frame moment
   (no interaction); then one bounded interaction (a single bound
   key, e.g. the emacs next-item stroke, or one click on empty
   column background); then sweep B. The A↔B delta is F4's
   "appeared later" made observable.
4. **The settings probe** (discriminates H-c): sweep A → open
   settings → close → sweep C. A hint set that changes exactly here
   revives H-c.
5. **Tier-4 fallback** (no display): the same observation matrix
   becomes a harness test on the testing backend — mount → first
   refresh → read the seven pushed hint models (`toolbar_menu_hint`,
   `nav_back_hint`, `board_segment_hint`, `task_card_hint`,
   `stack_header_hint`, `stack_create_hint`, `task_create_hint`)
   for the emacs keymap, and compare them against
   `first_binding_display(action, [Board, ViNormal, Global])`
   computed independently. The models matching the projection at
   first frame is the seam-level form of "hints present at start";
   the verdict then rests on E1–E4 plus the stale-comment/B-
   evidence. The dispatch prompt sanctions exactly this ("Tier-4 if
   headless — then the verdict is seam-analysis with recorded
   evidence").

### 4.3 The discriminating experiments

- **E1 (H-a)**: the Tier-4 harness test above — the chain at first
  push is asserted `[Board, ViNormal, Global]` and the masks are
  asserted `{Menu, Palette}`; the pushed models equal the
  independent projection. Any mismatch = H-a confirmed with the
  diffing row named.
- **E2 (H-b)**: a pure enumeration test — for each preset × each
  projected action × the board chain, record
  `first_binding_display` (a table, asserted, snapshot-adjacent but
  structural: which (preset, action) pairs are empty). Expected
  empty set today: `(all presets, BoardSelect)` and nothing else.
  This is also the by-design verdict's evidence of record.
- **E3 (H-c)**: the settings probe phase of the script (§4.2 step
  4) plus the code citation (keymap built pre-mount; reply swap
  forces the refresh). A no-delta result closes H-c.
- **E4 (H-d)**: the §2.2 probe's measured heights (the phantom line
  exists / is gone) correlated with the repro screenshots before/
  after the D13 fix. If hints "appear" after the fix on a fresh
  boot, H-d was the whole story.

### 4.4 The fix per verdict, and the presentation decision

- **Verdict "by-design emptiness" (H-b; the expected outcome)**: the
  empty-keyed tooltip is *correct* per the D19 rule (hints are
  projections of the keymap; `BoardSelect` is mouse-only today —
  showing a fake chord would be hand-written text). The recorded
  presentation decision, per D15's two options:
  **render nothing** (the established empty-hint rendering) — the
  alternative (render the action's menu-path hint) is recorded and
  rejected: it would make the tooltip the *only* carrier of
  reachability information for an unbound action, violating the
  panel's own contract (tooltip.slint:7-9: the keymap reaches every
  action; the tooltip is redundant by construction), and menu-path
  strings are exactly the hand-written copy D19 forbids.
  Consequences to land: the stale models.rs:903 comment is
  corrected (StackCreate is bound; BoardSelect is the documented
  empty), and E2's enumeration test pins the empty set so the next
  binding addition flips the test deliberately. **If** the review
  round wants `BoardSelect` bound (a real preset row), that is a
  one-line presets.rs change *plus* the conflict-contract gates —
  recorded as a follow-up issue, not smuggled into this chunk (no
  keymap-semantics changes in a hover-machinery chunk).
- **Verdict H-d (layout)**: the §2 fix is the fix; the repro
  screenshots after it are the confirmation artifact.
- **Verdict H-a/H-c (seam)**: the fix rides the named seam (a forced
  refresh or a chain fix at the found site); the test is the
  failing harness case from E1/E3 turned green. No such site is
  known on this base — the protocol exists because the finding
  predates three landed rounds; if the repro on *this* base cannot
  reproduce F4 at all, that is the recorded verdict (cannot-reproduce
  + the H-b/H-d by-design records + the stale-comment fix), reported
  to the user for confirmation against the round's exact build.

## 5. Test catalogue (TDD order)

1. `the_tooltip_panel_does_not_reserve_space_for_hidden_parts` — the
   §2.2 probe, four-state matrix (red on today's code for state 1).
2. `a_tooltip_does_not_survive_activating_the_hovered_task` — the
   F14 journey (red today).
3. `a_tooltip_does_not_survive_the_inbox_jump` / `..._the_palette_invoke` /
   `..._the_ghost_click` / `..._a_kiosk_flip` — §3.4's 2–5.
4. `every_view_swapping_effect_dismisses_the_tooltip_and_every_preserving_effect_spares_it` —
   the §3.3 structural walk (red for `OpenInbox`/`Activate`/`ShowTask`
   arms today; green after the choke point).
5. `projected_hint_bindings_per_preset` — the E2 enumeration (green
   immediately; pins the by-design empty set).
6. `first_frame_hints_match_the_keymap_projection` — the E1/Tier-4
   harness (green on this base; the H-a closer).
7. Existing pins stay green untouched (acceptance (d)):
   `a_visible_tooltip_does_not_survive_a_view_switch`,
   `a_menu_row_click_unhovers_the_tooltip`,
   `the_tooltip_width_includes_the_hint_row_and_keeps_the_cap`,
   `an_add_task_ghost_hover_arms_the_tooltip`,
   `an_info_tip_hover_arms_the_generalized_tooltip`; the keymap
   snapshots (`keymap_snapshots.rs`) — untouched, no preset rows
   change; `tooltip.rs` placement tests — untouched.

## 6. Risks

- **R1 The measurer swap changes the panel width on some content**
  (D13): the width formula's terms are re-sourced, not recomputed —
  the P5 facts carry over. The four-state probe plus the existing
  width test are the gate; a golden spot-check (one card tooltip
  with keys) regenerates deliberately if the anti-aliasing shifts.
- **R2 The identity tuple over-fires** (D14): the survive set in the
  structural test is the guard; the §3.3 resolution rule decides
  every conflict with evidence, and the tuple's members
  (`view`, `board`, `subject_stack`, `subject_task`) are the only
  tunable. The known genuinely-ambiguous case — a focus step in
  TaskDetail that re-mounts a different subject task — is resolved
  by that rule in stage 2 and recorded.
- **R3 The D15 verdict may be "cannot reproduce on this base"**
  (three rounds landed since the review): the protocol's evidence
  chain (stale comment, unbound BoardSelect, pre-mounted keymap,
  forced reply refresh) is itself the deliverable; the report hands
  the user the by-design records and asks for confirmation against
  the original build if the live repro stays clean.
- **R4 Conditional-element churn on hover** (D13): recorded
  non-risk (§2.3) — the panel content is static while shown; re-mount
  happens per invocation, a human-scale event.
- **R5 Scope pull toward the ghost/modal siblings** (D14): the
  BeginCreate/OpenPalette/BeginCapture arms are *in* this chunk (the
  same root cause, tiny arms); keymap semantics (binding
  BoardSelect) are *out* (filed as an issue if the verdict points
  there).

## 7. Stage-2 commit series (each standalone)

1. `test(ui-slint): the tooltip layout probe pins the hidden-row reservation` —
   the probe, red state recorded (commit 1 may carry the red test
   with `#[ignore]`-free red only if the series is reverted-safe;
   preferred: probe + fix in one commit, the probe's red run
   recorded in the report — stage 2 picks per the
   standalone-commit rule, no red commits on the branch).
2. `fix(ui-slint): the tooltip panel mounts its parts conditionally` —
   the §2.3 fix + the probe green + the width-test relation intact.
3. `feat(ui-slint): the view-identity dismiss choke point` — §3.2 +
   the structural walk green (commits 3's tests land with it; the
   journeys may precede in a red-recorded run but land green).
4. `fix(ui-slint): the modal and ghost takeovers dismiss the tooltip` —
   the BeginCreate/BeginCreateStack/OpenPalette/BeginCapture/
   rotation_tick arms + journeys.
5. `test(ui-slint): the hint projections are pinned per preset` —
   E1/E2 tests + the models.rs stale-comment correction.
6. `docs(report): ui r4 hover machinery stage-2 report` — verdict +
   evidence, per-gate results, Tier-4 items.
7. CHANGELOG Unreleased entry rides the first fix commit.
