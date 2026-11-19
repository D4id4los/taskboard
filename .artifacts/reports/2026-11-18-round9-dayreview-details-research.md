# Round 9 Research — Day-Review Journey + Task-Details Polish

- **Date**: 2026-11-18
- **Kind**: round-9 GUI-review research (root-cause analyses + journey
  redesign proposal; no production code)
- **Inputs**: operator round-9 findings (day-review cluster +
  task-details cluster), round-8 handoff
  `.artifacts/reports/2026-11-18-round8-handoff-to-round9-analysis.md`,
  the focus-restoration chunk
  `.artifacts/plans/2026-11-17-ui-r8-focus-restoration-plan.md` (PR
  #872), the task-detail-editor chunk
  `.artifacts/plans/2026-11-17-ui-r8-task-detail-editor-plan.md` (PR
  #922), the 2026-11-14 max-width incident report
  `.artifacts/reports/2026-11-14-task-view-window-max-width-collapse-report.md`.
- **UI crate**: `crates/taskboard-ui-slint` (paths below relative to
  the repo root unless absolute).

---

## 1. Day-review cluster — root causes

The "day review" is the **review walk overlay**
(`ui/views/review-walk-overlay.slint`, flow `Flow::ReviewWalk`, DR-7
chunk 3 / #638): three stages (0 Inbox, 1 Carry forward, 2 Prompts),
mounted window-level from `ui/app-window.slint:1550-1578`.

### 1.1 The walk overlay never adopted the round-8 mask/focus machinery (PR #872)

Two halves of the focus-restoration chunk landed; the walk overlay
adopted only one:

- **Keymap half — adopted.** `ContextId::ReviewWalk` is the "fifth
  modal context": `src/keyscope.rs:129-133` returns its scope head
  whenever `shell.review_walk` is set, so all *bound* keys are
  contained while the walk is open.
- **Focus-walk half — NOT adopted.** The Tab clamp (D-W2) works by
  `MaskWalkFacts` being **armed by the host**: each masked host
  (`ui/compositions/{capture-overlay,search-palette,timer-setter,
  labels-overlay,focus-overlay,recur-editor,track-entries,date-picker}.slint`,
  plus `icon-button.slint`) pushes `extent`/`cursor` into the global
  at mount. `adapter.rs:4368-4385` then either clamps
  (`mask_armed(window)` → `invoke_mask_focus(next)`) or injects a
  **synthetic Tab** (`dispatch_traversal`, adapter.rs:4596). The
  enumeration of armed hosts in the chunk plan (D-W2: "palette,
  capture, timer-setter, focus overlay, track entries, due editor,
  recur editor, labels overlay, notification surface") **omits the
  review walk overlay** — `review-walk-overlay.slint` contains no
  `MaskWalkFacts` reference at all. Result: while the walk is open,
  Tab synthesizes a raw Tab into the window, and slint's native
  traversal walks the Today view's widgets beneath the overlay.
- **Structural verdict**: this is not a day-review bug; it is the
  last un-migrated modal host. The per-host arming pattern (each
  composition hand-writes the same init/watch/mask-focus dance —
  compare `capture-overlay.slint:28-40` with
  `track-entries.slint:144-156` and `labels-overlay.slint:100-135`)
  is the actual defect generator: nothing forces a new modal host to
  opt in. **Fix path = a general ModalDialog base widget** (see §4)
  that arms the mask, sizes to contents, absorbs inside clicks, and
  owns the scrim, so containment is structural rather than
  opt-in-per-host. Coordinate with the parallel widget-framework
  research; do not patch the walk overlay in isolation unless the
  framework lands later (then a one-commit adoption is the
  migration).

### 1.2 The overlay is far larger than its contents

- `ui/app-window.slint:1555-1560` mounts the overlay with an explicit
  `width` cap (560·scale) but **no height**, and
  `ReviewWalkOverlay` (`ui/views/review-walk-overlay.slint:111`)
  declares no height either; its root `VerticalLayout` contains
  `rows-clip` with `vertical-stretch: 1` (line 160-162). With no
  content-derived height the panel stretches to (nearly) the full
  window.
- Contrast the house precedents: `SearchPalette` sets
  `height: content.preferred-height` (round-3 D24,
  `search-palette.slint:36`); `TimerSetter` sets
  `min-height: setter-layout.preferred-height + …`
  (`timer-setter.slint:164`); the app-window comment for the palette
  mount even states the discipline — "height by content, clamped
  above the status bar" (`app-window.slint:1435-1437`).
- **Fix**: the palette's D24 discipline — `height:
  min(content.preferred-height, <window bound above the status bar>)`
  — which is exactly what a ModalDialog base would provide.

### 1.3 Clicking inside the box closes it

- The walk scrim is a full-window `TouchArea` at
  `app-window.slint:1550-1554` mounted as the *earlier* sibling; the
  comment says "the later sibling wins the hit test". That is only
  true where the later sibling **has a hit surface**: `ReviewWalkOverlay`
  is a plain `Rectangle` whose only `TouchArea`s are the small
  candidate-row hit rects (`review-walk-overlay.slint:221-225`) and
  the `VerbButton` touches. A click on the panel's background/
  header/strip padding hits no TouchArea, falls through the
  Rectangle, and lands on the scrim → `walk-scrim-clicked()` → the
  Escape-ladder step-back (`back_review_walk`,
  `src/view_state.rs:2862-2882`) — the dialog closes/retreats.
- **Fix**: a covering `TouchArea` on the panel root (click-absorb,
  no action) — the same containment every real modal in the tree
  already carries implicitly via full-surface fields; this belongs
  in the ModalDialog base.

### 1.4 "Move to tomorrow" does nothing when no issues are shown

Exact chain:

1. With both `rows.inbox == 0` and `rows.carry == 0`, the clamp
   (`view_state.rs:2977-2981`) advances the empty Inbox to
   **CarryForward**, and nothing skips an empty Carry stage — so the
   overlay shows stage 1's verb strip ("Move to tomorrow / Pick a
   date… / Leave as is") over **zero rows**
   (`review-walk-overlay.slint:244-259` mounts on `stage-kind == 1`
   regardless of row count).
2. `WalkIntent::CarryTomorrow` (`adapter.rs:3174-3177`) resolves the
   cursor task via `walk_cursor_task` (`adapter.rs:3308-3326` →
   `presentation::walk_carry_rows(...).get(cursor)`), which is
   `None` on an empty list → `return Vec::new()` — a silent inert.
   `CarryPickDate` similarly mounts a date child over nothing.
3. Only `CarryLeave` works, because `step_walk_row` →
   `review_walk_next_row` (`view_state.rs:2822-2835`) with `len == 0`
   falls straight to `advance_review_walk()` → `WalkStep::Handoff`.

**Fix**: an empty Carry stage must be skipped symmetrically with the
Inbox (extend `clamp_review_walk` / `open_review_walk`), and the
verb strip should never render over zero rows (defensive UI guard).
Journey-level redesign in §3 supersedes the raw verb strip anyway.

### 1.5 "Leave it as is" → the mysterious "candidates" view

- `CarryLeave` on the last row (or an empty stage, §1.4) advances to
  `WalkStep::Handoff`; `interpret_walk`
  (`adapter.rs:3093-3097`) calls `hand_off_to_planner`
  (`adapter.rs:3055-3071`): the walk **closes** and
  `OpenPlanNextDay` navigates to the **PlanNextDay view**
  (`ui/views/plan-next-day-view.slint`) — the "candidates" view
  (Placed boxes + candidate pool).
- No guidance: the only copy is the EmptyState ("Tomorrow is empty /
  Pick from the pool once tomorrow has candidates.",
  `plan-next-day-view.slint:327-331`); nothing says the planner is a
  *step of the review* or that closing it returns to the review.
- The return path exists but is invisible: closing the planner
  (Escape / the toggle route, `adapter.rs:4235-4257`) consumes
  `walk_resume` and re-opens the walk at **Prompts**
  (`open_review_walk_at`, `view_state.rs:2763-2778`). The operator
  never found it — the journey's completion is gated on an
  undiscoverable action. (Cf. open issue #716, "the embedded planner
  stage as a walk child" — the round-9 journey design should decide
  against or build on it.)

### 1.6 The "candidates" view clamps the window's max width — the task-view bug class, unfixed here

- The task-view twin was fixed in commit `8698fc0f` (2026-11-14,
  report `.artifacts/reports/2026-11-14-task-view-window-max-width-collapse-report.md`):
  a conditional content-bounded row (the add-repeat `IconButton`)
  collapsed the view root's horizontal layout max, which winit
  applies as the OS window's maximum size; the fix is one line —
  `max-width: Theme.layout-severing-max-width;` at the view root
  (`task-view.slint:189`; token at `theme.slint:80`).
- `PlanNextDayView` **has no severing line** (grep over
  `ui/views/*.slint`: only task-view carries it), and it mounts the
  same bug-shaped rows — bare conditional content panes, e.g.
  `if (boxes == 0 && pool == 0 && !placing): EmptyState`
  (`plan-next-day-view.slint:327-331`) and the conditional placement
  form (line 296) — inside a stretched `VerticalLayout`. When the
  planner opens empty (exactly the "no candidates yet" state the
  operator lands in after §1.5), the root max collapses and the
  window is clamped narrow.
- **Generalization**: the fix is a *rule*, not a widget — every view
  root severs (`max-width: Theme.layout-severing-max-width` on
  Today/Board/Stack/Review/Settings/PlanNextDay roots), plus a
  regression probe family extending `tests/window_max_probe.slint`
  per view (the collapse only materializes through the event loop).
  If the widget-framework effort lands a shared view-shell/modal
  base, the severing belongs in it; otherwise a one-line-per-view +
  probe chunk is cheap and safe.

## 2. Task-details cluster — root causes

### 2.1 Description editor shrinks in edit mode

- Display branch: `description-display` is a `Rectangle` with
  `vertical-stretch: 1` whose `MultiLineEditor` gets
  `width/height: 100%` (`task-view.slint:1230-1274`) — fills the
  flexible section (r4 D16d).
- Edit branch: `if (description-edit-open): HorizontalLayout` with
  `vertical-stretch: 1` (`task-view.slint:1301-1319`) — but the
  `MultiLineEditor` inside it sets only `horizontal-stretch: 1`. In
  a `HorizontalLayout` a child without `vertical-stretch` sizes to
  its **preferred** height, and the widget's preferred height is its
  `min-height` floor (`multi-line-editor.slint:118`: three lines +
  padding). So the editor collapses to the 3-line floor while
  editing and re-expands on commit.
- **Fix**: `vertical-stretch: 1` on the edit-mount editor (and check
  the `InfoTip` sibling doesn't stretch — it should stay
  content-height). One line + a `window_max_probe`-style geometry
  assertion (or an `editor_viewport_probe` arm) pinning
  edit-mount-height == display-mount-height.

### 2.2 Effort row: label + always-editable textbox on one line

Current shape (`task-view.slint:891-936`): caption "Effort", then a
*conditional pair* — closed: echo text + edit `IconButton` (+ clear
`IconButton`); open: a swapping `TextField` (own block). The swap is
the round-8 chunk-3 D-8 seam (`effort-open`, adapter
`effort_editor`). Required: label **"Effort Estimate"**, one line,
`TextField` always mounted (echo/`initial-text` seeded from
`task.effort`), clear affordance retained. The swap machinery
(`effort-edit-opened`/`effort-cancelled`) collapses to
focus-lost/accepted commit — mostly a deletion; the census
`task.effort_edit` row (round-8 evidence) needs the pointer-cell
update citing this round.

### 2.3 Opening the "time picker" shifts the underlying layout

Two distinct in-flow mechanics, both in the due/scheduled region:

1. **The due/scheduled editor swap** (the primary shift): the
   collapsed one-line cell (`due-cell`, `task-view.slint:650-693`)
   is *replaced* by the multi-row in-flow `DueEditor`
   (`task-view.slint:694-706`, `:761-775`) — typed field row +
   conditional echo/hint rows (`due-editor.slint:76-92`). Everything
   below the metadata row jumps down on open and back on close. The
   DueEditor is deliberately *not* a window-level overlay today
   (round-3 D10 moved only the calendar panel window-level).
2. **The calendar+time panel itself is window-level**
   (`app-window.slint:805-818`, anchored via `DatePickerFacts`) — it
   does **not** shift layout. The same class of in-flow expansion
   does exist in the entries form (`track-entries.slint:213-227`
   mounts a `DatePicker` inside the form's `VerticalLayout`), which
   the round-9 chunk should note as the same-law sibling.

**Fix direction**: make the expanded editor a true overlay (or keep
the swap but reserve stable geometry — an overlay is the operator's
ask). The positioning mechanic (anchor facts → flip/clamp) already
exists; general overlay positioning is the parallel research
effort's deliverable — this chunk should consume it, not reinvent
it. Minimum in-scope fix: the swap must not reflow the rows beneath
(either overlay the editor over the metadata area, or fix the
row's height to the editor's while open so only the editor's own
echo row grows inside a stable envelope).

### 2.4 The time picker renders on top of its opening button/label

The window-level panels position from *mirrored anchor facts*, and
the mirrors have two recorded races:

- **Stale anchor at open** — the DatePicker anchor is mirrored via
  `changed date-anchor-*` handlers (`task-view.slint:315-333`,
  comment: "values are stale while the view is unmounted"), read
  from the *unconditional* columns. When the DueEditor swap (§2.3)
  changes the column's height in the same layout pass the picker
  opens in, the `anchor-height` used by the flip test
  (`app-window.slint:812-818`) can be the pre-swap value, choosing
  the wrong below/above branch and landing on the invoking row.
- **Panel-move race for the quick list** — the TimeField anchors the
  `QuickListFlyout` from `TimeFieldFacts.anchor-*`
  (`time-field.slint:92-126`), captured on the open flip and on
  `changed absolute-position`. When the host TimeField lives inside
  the DatePicker panel whose own `y` re-resolves after its height
  settles (`self.height` participates in its own `y` binding,
  `app-window.slint:812-818`; flyout twin `:1258-1268`), the
  `changed absolute-position` delivery on the inner field is not
  guaranteed within the same pass (the file's own
  "unreliable-mirroring record", `time-field.slint:97-99`) — the
  flyout stays at the pre-move spot, i.e. over the field/button.
- The last-resort clamp (`min(…, status.y - self.height - space)`,
  `app-window.slint:818`/`1268`) can additionally pull the panel up
  over the anchor when neither below nor above fits.

**Fix direction**: belongs to the general overlay-positioning
framework (re-anchor on realized geometry each frame / re-evaluate
on panel-move, not one-shot mirrors). In-scope for this round:
pin the chosen placement with a journey/probe asserting the panel's
`y` does not intersect the anchor rect (typed geometry assertion),
and re-mirror the anchor facts on the swap layout pass.

---

## 3. Proposed day-review journey redesign (UX-journey-plan feed)

Expressed per `.artifacts/templates/ux-journey-plan.md` shape: steps,
states, widget mapping. To be authored as
`.artifacts/plans/2026-11-XX-day-review-journey-ux-plan.md` after
operator acceptance of the decisions below.

### 3.1 Journey law

One guided modal, **one decision per screen**, explicit progress,
explicit end. The user is never left on a verb strip over zero rows,
and never handed to another view without being told how to come
back.

### 3.2 Steps / states

| # | Step | State / entry condition | UI | Exit |
|---|---|---|---|---|
| S1 | Entry | review due (menu `ReviewOpen`, Today card) or manual | Modal opens with header "Review of <day/window>", stage strip (1..n), **stage-intro sentence** | first issue, or straight to S5 when no issues |
| S2 | Inbox issues | `walk_inbox_rows > 0` | one issue card + verbs: *File to…* (candidates picker inline), *Keep in inbox*, (Escape = ladder) | last row → S3 |
| S3 | Carry-forward decisions | `walk_carry_rows > 0` | one task card + verbs: *Move to tomorrow*, *Pick a date…*, *Leave as is* | last row → S4 |
| S4 | Plan tomorrow (the handoff, made legible) | entering S4 | The walk **stays open**; the planner mounts as the walk's step (issue #716's embedded-planner child, or at minimum: planner opens with a banner "Step 3 of 4 — place tomorrow's boxes; Escape/back returns to the review") | planner close → S5 (existing `walk_resume` path, `adapter.rs:4247-4254`) |
| S5 | Prompts / reflection | always reached | existing prompt editors + Record/Finish | S6 |
| S6 | Completion | after Record/Finish | **Summary screen**: counts (filed N, moved N, left N), journal recorded yes/no, "Review complete — done" single exit | close → origin view, focus restored (FocusOrigin seam, census row 13) |

Skip rules (fix §1.4 structurally): a stage with zero rows renders
as a *checked* stage in the strip ("Inbox — nothing to file") and is
stepped past automatically — never a verb strip over nothing.

### 3.3 Widget mapping

- S1–S3, S5, S6: the existing `ReviewWalkOverlay` reshaped —
  size-to-contents (§1.2), click containment (§1.3), mask-walk
  arming (§1.1), one-issue-per-screen replaces the row list + cursor
  emphasis (`rows` ListView collapses to the single cursor row; the
  strip keeps the progress semantics).
- S4: either the #716 embedded-planner-child (preferred; keeps the
  mask and the ladder) or the PlanNextDay view with the guidance
  banner + window max-width fix (§1.6) as the fallback.
- S6 is new but pure presentation (counts derivable in
  `presentation.rs` beside `walk_*` helpers).

### 3.4 Census surface

New/updated rows: `review.walk_enter`, `review.stage_skip`,
`review.planner_return`, `review.walk_complete` (summary exit) —
evidence cells per surface sweep; the round-8 `review.*` rows keep
their verb bindings (the verbs are unchanged; only their mounting
grammar changes).

---

## 4. Reuse split: general framework vs day-review-specific

**General (ModalDialog base / overlay framework — coordinate with the
parallel widget-framework research; adopting it is the fix path for
§1.1–§1.3 and half of §2.3/§2.4):**

- scrim + later-sibling hit containment + covering panel TouchArea;
- size-to-contents with window bounds (palette D24 discipline);
- MaskWalkFacts arming + ordered control list + `mask-focus`
  (fold the per-host copy-paste from seven compositions into the
  base);
- Escape ladder hook + FocusOrigin record/restore (census rows
  already exist);
- anchor/flip/clamp placement with re-anchoring on host layout
  changes (the §2.4 mechanic);
- view-root layout severing (`max-width` rule) + probe family (§1.6).

**Day-review-specific (this journey):** stage machine + skip rules
(view_state `clamp_review_walk`/`advance_review_walk` extensions),
one-issue-per-screen presentation, the planner handoff legibility
(S4), summary screen (S6), all copy.

## 5. Open questions for the operator

1. S4: build the embedded planner child (#716) now, or ship the
   banner + resume-hint fallback over the existing view handoff?
2. Is the "time picker" of §2.3/§2.4 the due/scheduled editor's
   calendar+time panel, the quick-times flyout, or the entries-form
   grid? The mechanics differ slightly (§2.4's two races); a 30-second
   screen recording or the exact click path settles which anchor race
   to pin first.
3. Should the always-editable Effort Estimate textbox commit on
   focus-loss (description law) or on Return only?
4. Chunk ordering preference: details polish first (small, PR-able)
   or day-review journey first (needs the UX plan + possibly the
   ModalDialog base)?

## 6. Suggested chunk boundaries

1. **Chunk A — task-details polish** (§2.1, §2.2, §2.3 in-flow
   stability, §2.4 anchor re-mirror + geometry pin): one branch, TDD,
   ~small; blocked only on Q2/Q3 answers.
2. **Chunk B — severing generalization** (§1.6): per-view
   `max-width` severing + `window_max_probe` family extension
   (PlanNextDay first, others as the probe host allows). Tiny,
   independent, mergeable any time.
3. **Chunk C — ModalDialog base adoption** (§1.1–§1.3): depends on
   the parallel widget-framework research; if that lands a base,
   this chunk migrates the walk overlay (and ideally palette/
   capture) to it; if not, this chunk adds the three fixes directly
   to the walk overlay as stop-gap with a `backlog:` issue for the
   base.
4. **Chunk D — day-review journey** (§1.4, §1.5, §3): requires the
   accepted UX journey plan (mandatory per AGENTS §7 / phase-6
   mandate) before dispatch; rides on C's containment.

Chunks A and B are parallelizable (file-disjoint); C before D.
