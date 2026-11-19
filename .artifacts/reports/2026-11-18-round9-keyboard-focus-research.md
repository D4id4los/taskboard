# Round-9 GUI Review Research — Keyboard & Focus Architecture

- **Date**: 2026-11-18
- **Kind**: round-9 analysis (operator pass → root-cause research). No
  production code written; this report is the input to the round-9
  planning stage.
- **Scope**: the operator's five keyboard/focus findings against
  `taskboard-ui-slint` at post-round-8 main (census Status: PR #943).
- **Authorities read**: AGENTS.md, `docs/ux-surface-census.org`,
  `.artifacts/reports/2026-11-18-round8-handoff-to-round9-analysis.md`,
  `.artifacts/plans/2026-11-16-keymap-parity-ux-plan.md`,
  `.artifacts/plans/2026-11-16-focus-restoration-ux-plan.md`,
  `.artifacts/plans/2026-11-17-ui-r8-keymap-parity-plan.md` (+ the
  focus-restoration chunk plan), issue #855 (+ PR #936's directional
  partner map).

## 1. The dispatch pipeline (orientation for every finding)

```
slint key-pressed (app-window FocusScope)
 → Adapter::intercept            adapter.rs:939
    → to_stroke (text+mods → Stroke, composed glyph only)  adapter.rs:6031
    → text-field monopoly + editor scope + special scopes   keyscope.rs
    → resolver.feed(stroke, chain, keymap)                  resolver.rs:60
    → Resolution::Action(id) → route(id) → ActionRouter::dispatch  router.rs:690
    → UiEffect → Adapter::interpret (flow × effect table)   adapter.rs:3657
    → resolve_conflict → Resolve matrix (FlowKind × EffectClass) adapter.rs:4971
```

Key structural facts used below:

- The **context chain** is `[viewContext, ViNormal|ViInsert, Global]`
  (`view_state.rs:1240` maps `ViewId → ContextId`); first-match over
  the visible (masked) chain wins (`keymap/mod.rs:99`).
- The **flow-settle matrix** (`adapter.rs:4971`): a foreign flow
  meeting an effect class its kept arms don't own does
  `SettleThenRun` for `C::Nav | C::Lifecycle | overlay-open` — i.e.
  **Nav-class effects cancel a live move flow first, then run**.
  The move flows' kept arms are only
  `FocusNext | FocusPrev | Activate | RequestMove | CancelMove |
  SetMode(Normal)` (`adapter.rs:3742-3749`).
- **Text-entry coexistence already exists** and sits *before* the
  resolver: the printable monopoly (`keyscope.rs:211`,
  `monopoly_declines`), the editor scope (`keyscope.rs:396-454` —
  emacs binds `Ctrl+f/b/n/p` as in-field cursor commands while a
  field holds focus), `ViInsert` shadowing-without-masking, and the
  activation reservation (`adapter.rs:1008`). Any new focus
  dispatcher riding the resolver path inherits all of it for free.

## 2. Per-view audit of C-f / C-b / C-n / C-p (finding 1)

The **central law** (operator): C-f/C-b/C-n/C-p move THE FOCUS
(directional alternative to Tab) in every view; they must not change
the active task; they must not fire while typing.

### Emacs preset (the operator's preset)

| Surface (context) | C-f | C-b | C-n | C-p | Law verdict |
|---|---|---|---|---|---|
| Board (`Board`) | `StackNext` — stack→stack walk | `StackPrev` | `NavNextItem` — task walk | `NavPrevItem` | **Conformant** (the operator's reference behavior) |
| Board **during move** (`C-c C-w`) | `StackNext` → `EffectClass::Nav` → **settles (cancels) the move**, then steps stack selection | `StackPrev` same | `FocusNext` → `cycle_move_target` (moves the drop ring) ✓ | `FocusPrev` ✓ | **Bug** — see §3 |
| Today (`Today`) | `NavNextControl` (Tab walk) | `NavPrevControl` | `NavNextItem` — flat row walk | `NavPrevItem` | Conformant-ish (vertical has a clear direction; horizontal = control walk, no item-level horizontal exists — acceptable, see §8 Q3) |
| Task detail (`TaskDetail`) | `NavNextControl` (Tab walk) | `NavPrevControl` | `NavNextItem` → **changes `subject_task`** (the viewed task) | same, backwards | **Violation** — see §4 |
| Stack detail (`StackDetail`) | `NavNextControl` | `NavPrevControl` | `NavNextItem` — stack list walk | `NavPrevItem` | Conformant |
| Settings (`Settings`) | `NavNextControl` | `NavPrevControl` | `NavNextControl` (r4 D19) | `NavPrevControl` | Conformant (all four move focus) |
| Day-review (`ReviewWalk`, masked) | unbound | unbound | `ReviewWalkCursorDown` | `ReviewWalkCursorUp` | Conformant (masked modal owns its grammar) |
| Search palette (`Palette`, masked) | unbound | unbound | `MenuNext` | `MenuPrevious` | Conformant |
| Menus (`Menu`, masked) | unbound | unbound | `MenuNext` | `MenuPrevious` | Conformant |
| Planner (`PlanNextDay`) | unbound (falls to Global; emacs Global C-f unbound) | unbound | unbound | unbound | Gap — planner has no directional grammar at all (pointer surface; needs a decision, §8 Q4) |
| Track-entries overlay (`TrackEntries`, masked) | unbound | unbound | `NavNextControl` | `NavPrevControl` | Conformant |
| Notification surface (ambient) | unbound | unbound | unbound | unbound (Up/Down + verb keys) | Out of law scope (ambient, not focus) |

### Standard preset

| Surface | C-f | C-b | C-n | C-p | Verdict |
|---|---|---|---|---|---|
| All views | **`PaletteSearch` (Global `Ctrl+F`)** — opens search in *every* view, including Board | unbound | unbound (arrows are the preset's convention) | unbound | **Law violation** if the law is preset-independent (§8 Q2): C-f is not focus movement anywhere |
| Menus/palette | — | — | unbound (arrows) | unbound | fine |

### Vi preset

`Ctrl+f`/`Ctrl+b` = `ViewScrollDown`/`ViewScrollUp` in `ViNormal`
(`presets.rs VI_S2`) — scroll, not focus. `Ctrl+n/p` unbound. Under a
preset-independent law this is also a violation, but vi's convention
is hjkl motions; see §8 Q2.

**Root cause of the non-uniformity (finding 1):** there is no
directional concept anywhere in the action/effect vocabulary. The
four chords are bound to *four unrelated action families per context*
(`StackNext`, `NavNextItem`, `NavNextControl`, `ReviewWalkCursor…`),
each with its own routing. The only "focus between interactable
widgets" machinery that exists is `ControlNext/ControlPrev`
(`adapter.rs:4362-4386`) — a **linear** re-injection of Slint Tab
traversal (plus the masked-site cursor), with no notion of direction.
Uniformity was impossible to express; each round added rows context by
context (r3 D25, r4 D19, r8 keymap-parity D-1/D-2) and the seams
diverged.

### The architectural seam: a uniform directional-focus dispatcher

Proposed (design sketch, no code):

1. **Four new ActionIds** `nav.focus-left/-right/-up/-down` (registry
   entries available in every view context), routed to one new
   `UiEffect::FocusDirectional(Direction)` — `EffectClass::Nav`, but
   with **kept arms in the move flows** (§3).
2. **One adapter-side per-view interpretation table**, the shape of
   `push_scroll_page_hint` (`adapter.rs:4400`, the existing per-view
   container-resolution precedent — one home, one match per view):
   - Board: left/right = stack walk (the `StackNext/StackPrev`
     bodies), up/down = task walk (`step_focus`).
   - Board during `Flow::Move`/`MoveStack`: left/right = move the
     drop target to the adjacent stack (same row index), up/down =
     the existing `cycle_move_target` ring.
   - Today/StackDetail: up/down = item walk; left/right = control
     walk (no item-level horizontal domain exists).
   - TaskDetail: **all four = control walk segments** — up/down move
     focus along the detail's vertical stop ladder
     (`TaskViewWalkFacts` stop order), left/right along the row's
     horizontal group. Never `subject_task` (the §4 fix).
   - Settings: all four = control walk (current behavior kept).
   - Menus/palette/review/alarm/notification keep their own masked
     grammars (the law is about views, not modals).
3. **Preset rows**: bind the four chords to the four directional ids
   in the five view contexts of every preset (or emacs-only, per
   §8 Q2). The existing per-context rows they replace
   (`StackNext` on C-f etc.) move to alternates or stay as-is in
   contexts the directional table routes identically — the intra-
   context conflict contract (`presets_are_intra_context_conflict_free`)
   governs the retune.
4. **Text-entry coexistence for free**: the dispatcher rides the
   resolver path, so the monopoly (`monopoly_declines`), the editor
   scope (emacs in-field `Ctrl+f/b/n/p` cursor commands,
   `keyscope.rs:431-434`), `ViInsert` shadowing, and the activation
   reservation all keep the chords from firing while typing. No new
   guard is needed — this is the strongest argument for putting the
   dispatcher behind the resolver rather than in a FocusScope.

The `ControlNext` Tab re-injection machinery stays as the Tab
fallback; the directional table can eventually subsume it (Tab =
"next in reading order" over the same stop ladder), but that is a
follow-up, not a round-9 must.

## 3. Move-mode × C-f/C-b (finding 2) — root cause

Repro path: board view, `C-c C-w` arms `Flow::Move`; `C-f` resolves
`StackNext` (emacs Board row, `presets.rs EMACS_S1`). `StackNext` is
`EffectClass::Nav` (`router.rs:415`); the flow-settle matrix maps
`(any FlowKind, C::Nav) → R::SettleThenRun` (`adapter.rs:4992-5000`);
`SettleThenRun` **settles the move flow** (a move settles = cancel
without dispatch, `view_state.rs settle_foreign`), then re-runs the
effect against `Flow::None` — the stack *selection* steps. Net: the
move is silently discarded and the board's selected stack moved.
Exactly "breaks the move instead of moving stacks".

**Fix respecting the law**: give the move flows kept arms for the
directional left/right — either

- (a) add `UiEffect::StackNext | UiEffect::StackPrev` to the move
  flows' kept-arm tuple (`adapter.rs:3742`) and route them in
  `interpret_move` to a new `cycle_move_stack_target`-shaped view
  state method that hops the drop target column-wise (keep row
  index, clamp), or
- (b) the §2 directional dispatcher: `FocusDirectional(Left/Right)`
  is a kept arm of `Flow::Move` by construction.

(b) is preferred — it makes the law's move-case ("during a move,
directional keys move the move-target") the same code path as the
normal case. The fix needs a journey: arm move → C-f → assert the
flow is still live and the target column advanced (and the existing
`push_move_scroll_hint` fires). Note `Flow::DragMove` already absorbs
FocusNext/Prev mid-drag (`adapter.rs:3739`) — the keyboard ring walk
is pointer-owned there; directional keys should follow the same
absorb, not retarget the pointer's hover.

## 4. Task detail: C-n/C-p change the active task (finding 1's sharp edge)

`step_focus` has an explicit TaskDetail arm (`view_state.rs:1652-1665`,
the round-8 keymap-parity D-2 route fix): the walk domain is the
subject's stack list, the cursor is `subject_task`, and each step
**writes `subject_task`** — i.e. C-n/C-p swap which task the detail
view renders. That was a deliberate round-8 decision ("the item walk
is semantically dead here" was overturned by making the detail's item
table live), but it is exactly what the operator's law forbids: the
chords are task-*selection* verbs there, not focus movement.

**Fix**: under the §2 dispatcher, TaskDetail's up/down become control
walk along the detail's stop ladder (title → tracking verbs → due →
scheduled → priority rows → effort → labels → rules → recur →
description — the `TaskViewWalkFacts` stop order), and the
subject-change walk is re-homed on explicit verbs (`M-n/M-p`-style or
the existing `NavNextItem` on arrows-only), or dropped to the board
view (return-to-board-then-walk). §8 Q1 asks the operator which.

## 5. Ctrl+Shift+T / today view (finding 5) — root cause

- The binding is `ViewToday = Global "Ctrl+T"` in **all three
  presets** (`presets.rs:264/593/1054`); no preset binds
  `Ctrl+Shift+T` anywhere. The menu row (menu.rs:992, position 60)
  routes `ActionId::ViewToday` directly (`route()`), which is why the
  menu works regardless of the key seam.
- The key path dies at the **chorded-letter case seam**, a second
  dimension of #855: `parse("Ctrl+T")` mints `key: 'T'` (chorded
  printables keep their case — the D6 fold applies only to *bare*
  printables, `stroke.rs:402-407`), but a real Ctrl+T press delivers
  Slint `text: "t"`; `to_stroke` → `from_event('t', ctrl…)` keeps
  `'t'` (`stroke.rs:256-264`). `'t' ≠ 'T'` → no exact match, and
  `chord_shift_partners` deliberately returns nothing for letters
  (`stroke.rs:189-195` — the rule that keeps `Ctrl+Z ≠ Ctrl+Shift+Z`).
  So **neither Ctrl+T nor Ctrl+Shift+T can ever resolve a
  uppercase-spelled chorded-letter token**. Every standard-preset
  Global row spelled `Ctrl+Z/Q/K/J/T/F` shares the defect; the emacs
  preset spells its letters lowercase (`Ctrl+x`, `Ctrl+n`) and is
  unaffected — matching the operator's emacs-centric report history.
- Tests never catch it: journeys feed `parse()` tokens
  (`journeys.rs:5530` `parse("Ctrl+T")`), never `from_event`
  deliveries of cased letters.

**Proposed fix** (relates to #936's directional partner map as the
punctuation twin of the same seam):

1. In `Keymap::resolve`'s normalization pass (or
   `Stroke::chord_shift_partners`, extended), add a **case-fold
   candidate for chorded ASCII letters**: an event letter folds to
   the token's case (shift stays a real modifier, so `Ctrl+Shift+Z`
   remains ≠ `Ctrl+Z` — the fold only heals case, never the shift
   bit). Exact-match-first keeps every existing resolution
   byte-identical.
2. Rebind or double-bind `ViewToday` to `Ctrl+Shift+T` if the
   operator wants that spelling (it matches the `Ctrl+Shift+H`
   habit-create family and frees bare Ctrl+T, which terminals/Browsers
   often steal); §8 Q5.
3. Add a contract test feeding `from_event` deliveries (both cases,
   shift on/off) for every preset row spelled with a chorded letter —
   the seam's regression bar. (Comment with this finding already
   posted on #855: issuecomment-6979361380.)

## 6. Task-details shortcut inventory (finding 3)

Operator philosophy: every regularly-used activatable widget in task
details gets a dedicated shortcut; Tab is the backup; rare verbs
(conversion) exempt. Inventory from `ui/views/task-view.slint`
(callbacks at lines 196-305, stops at 425/540/657/724/1243):

| Widget / verb | ActionId | Std | Emacs | Vi | Tooltip hint | Verdict |
|---|---|---|---|---|---|---|
| Done checkbox (`task-done-toggled`, :13417) | `TaskComplete` | — (Space bound only Board/Today) | — | — | `task-done-hint` pushed | **Gap**: no TaskDetail-context binding in any preset |
| Title edit (:540 stop 3) | `TaskTitleEdit` | F2 | Return | — | yes | vi gap (Return is `TaskEdit` in ViNormal) |
| Start tracking (:610) | `TaskTrackStart` | F9 | C-c C-i | I | yes (`task-track-hint`) | landed r8, TaskDetail+Today only |
| Stop tracking (:618) | `TaskTrackStop` | F10 | C-c C-o | O | yes | landed r8, same scope |
| Tracker toggle (`task-track-toggled`, :13410, UX-R8 D-6) | `TaskToggleTracker` | — | — | — | ? | **Gap**: action exists (`actions.rs:360`, router routes it), bound nowhere |
| Track-entries overlay (:1057) | `TaskTrackEntries` | — | — | — | yes | **Gap**: menu-reachable only |
| Due edit (:657 stop 5) | `TaskDueEdit` | F5 | C-c C-d | d | yes | landed r8 |
| Scheduled edit (:726 stop 6) | `TaskScheduledEdit` | F6 | C-c C-s | s | yes | landed r8 |
| Description (:1243 stop 11) | `TaskDescriptionEdit` | F7 | C-c ' | e | yes | landed r8 |
| Importance picker (:862) | `TaskImportanceEdit` | F3 | F3 | F3 | yes | landed r4 |
| Urgency picker (:885) | `TaskUrgencyEdit` | F4 | F4 | F4 | yes | landed r4 |
| Effort edit / clear (:917/:924) | — no ActionId (view seams `effort-edit-opened`/`-cleared`) | — | — | — | yes | **Gap**: no registry id, no binding |
| Labels manage (:987) | `LabelManage` | Ctrl+L | Ctrl+L | Ctrl+L | yes | landed |
| Chip unassign (:978, per-chip) | — (indexed seam) | — | — | — | yes | pointer/Tab (indexed targets, like rule rows) — acceptable |
| Rule add (F8 family) | `ReminderRuleAdd` | F8 | C-c C-u | a | row-level | landed r8 |
| Rule remove / presets / unmute / jump-to-due (rule-list) | — indexed/view seams | — | — | — | row-level | pointer/Tab (row-indexed; a shortcut needs a cursor over the rule list — §8 Q6) |
| Series edit (:1000/:1124) | `SeriesEdit` | Ctrl+R | C-c r | r | yes | landed r7 |
| Series pause/resume (:1165/:1172) | `SeriesPause`/`SeriesResume` | — | — | — | yes | pointer/Tab (moderate use — candidate) |
| Series/habit conversion (:1179+) | `SeriesConvertArm`… | — | — | — | yes | exempt (operator's rare-verb carve-out) |
| Back (:390) | — | — | — | — | yes | **Gap**: no close-subject verb bound (Escape is move-cancel Global) |
| Priority dismiss (:371) | via `CancelMove` routing | Esc | Esc/C-g | Esc | n/a | works |

**Proposed assignment table** (avoiding round-8 collisions — verified
against `presets.rs` current inventory; final spellings are a
stage-1 D-table decision):

| Verb | Std | Emacs | Vi | Note |
|---|---|---|---|---|
| `TaskComplete` @TaskDetail | Ctrl+Space? — **taken by TaskReopen @Board only**; TaskDetail-free → `Ctrl+Space` | C-c C-t (free at TaskDetail) | Space (free at TaskDetail) | mirrors each preset's Board spelling |
| `TaskToggleTracker` | Ctrl+Shift+K (Ctrl+K is capture) | C-c k | g k / K | tracker-kind toggle |
| `TaskTrackEntries` | F12 (F11 family exhausted: F11+mods taken) | C-c e (e taken by description @vi only; free in emacs TaskDetail) | E | entries overlay opener |
| Effort edit (new id `TaskEffortEdit`) | Ctrl+E? — check: free in std TaskDetail | C-c f? — taken (board color @Board only, free @TaskDetail; but f is load-bearing) → `C-c C-e` | f (free at TaskDetail) | effort inline editor |
| Effort clear (new id) | Ctrl+Shift+E | C-c C-x? — **taken** (unschedule) → `C-c X` | X — **taken** (unschedule @TaskDetail) → `Shift-F`? → stage-1 table | sparse-clear twin |
| Subject close/back (new id `NavSubjectClose` or reuse `FocusBoard`) | Alt+Left / Backspace? | Alt+Left | Alt+Left | "back" convention; needs a decision |

## 7. Start/stop/toggle tracking bindings (finding 4)

Partially stale finding: round 8 (#862, keymap-parity US-3 AC-3)
bound `TaskTrackStart`/`TaskTrackStop` at **TaskDetail and Today**
(std F9/F10, emacs C-c C-i/C-c C-o, vi I/O — `presets.rs:737-756`,
`1211-1214`, `422-425`), hints projected onto the buttons. The real
gaps:

1. **Board and StackDetail contexts** — the card quick-actions bar
   (r8 chunk 5) mounts start/stop there with no key route.
2. **`TaskToggleTracker`** (the detail card's tracker toggle,
   UX-R8 chunk-5 D-6) is bound in no preset.
3. **`TaskTrackEntries`** (overlay opener) is menu-only.

Proposed: extend the start/stop pair to Board/StackDetail (same
spellings, different context rows — the intra-context contract is
per-context so no retune of existing rows); bind the toggle and the
entries opener per the §6 table. Alternative single verb: a
`TaskTrackToggle` smart verb (start-if-stopped/stop-if-running over
the focused task) — one binding instead of two; §8 Q7.

## 8. Open questions for the operator

1. **TaskDetail subject walk**: after C-n/C-p become focus movement,
   is changing the viewed task from the detail needed at all
   (return-to-board-and-walk suffices), or keep it on a distinct
   chord (e.g. M-n/M-p)?
2. **Is the law preset-independent?** Standard's C-f = search and
   vi's C-f/C-b = scroll violate a universal reading. Retune all
   three presets to the directional ids, or emacs-only with
   standard/vi keeping their conventions?
3. **Today horizontal**: no item-level left/right domain exists — is
   control-walk an acceptable left/right there, or should C-f/C-b be
   inert on Today?
4. **Planner (`PlanNextDay`)**: should the surface get the
   directional grammar (pool/boxes two-pane), or stay pointer-first?
5. **Today-view spelling**: keep `Ctrl+T` (with the case-seam fix) or
   rebind to `Ctrl+Shift+T` (the habit-create family; frees the
   terminal-familiar bare chord)?
6. **Rule-list cursor**: rule rows are index-targeted; a shortcut
   family needs a selected-rule cursor (new walk state). Worth a
   chunk, or pointer/Tab acceptable?
7. **Track toggle**: two verbs (start/stop) everywhere, or one smart
   `task.track-toggle` over the focused task?

## 9. Suggested chunk boundaries

1. **Chunk K1 — the chorded-letter case seam** (§5): the
   `Stroke`/`Keymap::resolve` case-fold candidate + the
   `from_event`-delivery contract test over every preset's
   chorded-letter rows + the ViewToday spelling decision (Q5).
   Small, test-dense, unblocks the operator's today-view complaint
   immediately. (Backlog note already on #855.)
2. **Chunk K2 — the directional-focus dispatcher** (§2 + §3 + §4):
   four ActionIds, `UiEffect::FocusDirectional`, the per-view
   interpretation table, move-flow kept arms (the finding-2 fix), the
   TaskDetail subject-walk re-homing (Q1), preset rows retune (Q2),
   journeys per view. The round's core; UX journey plan mandatory
   (user-visible key behavior change).
3. **Chunk K3 — task-details shortcut completion** (§6 + §7): the
   done/toggle/entries/effort/back assignment table, the two new
   ActionIds (`TaskEffortEdit`, effort-clear / subject-close), the
   start/stop context extension, hint projections. Keymap-parity
   shaped (registry + presets + funnels + journeys).
4. Optional **K4** (if Q6 says yes): the rule-list cursor walk.

Each chunk = one branch/PR per the dispatch protocol; K1 can run
parallel to K2 (file-disjoint: `stroke.rs`/`keymap/mod.rs`+tests vs
`actions.rs`/`presets.rs`/`adapter.rs`/`view_state.rs` — note both
touch `presets.rs`; serialize K2 after K1 or coordinate the retune).

## 10. Issue ledger

- Commented on **#855** (closed, partially): the chorded-letter case
  dimension + the standard-preset Global family exposure
  (issuecomment-6979361380).
- No new backlog issues opened: every finding here is in-scope for
  round 9 and chunked in §9. If the operator defers any chunk, its
  items become `backlog:` issues at triage.
