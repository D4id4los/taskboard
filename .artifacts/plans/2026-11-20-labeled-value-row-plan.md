# [Plan] LabeledValueRow — round 9 chunk 14 (W4, `refactor/ui-labeled-value-row`)

- **Date**: 2026-11-20
- **Kind**: implementation plan — TDD-first centralization of the
  label+value row family onto one `LabeledValueRow` base widget
  (O-6, #951). Stage-1 plan; no implementation in this commit.
- **Origin**: round plan
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §1 O-6 /
  §W4; whole-widget inventory
  `.artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md`
  (§3.2 LabeledValueRow row — normative for the customization points;
  §4 sizing; §5 migration order); J2 rulings
  `.artifacts/plans/2026-11-18-widget-framework-ux-plan.md` (R-6, R-7 +
  the §6 O-6 addendum: W-3/W-4 amends — the effort/description rows
  become LabeledValueRow modes); chunk-10 compatibility ruling
  `.artifacts/plans/2026-11-19-habit-editing-plan.md` §D6.
- **Status**: READY for stage 2. Preconditions verified 2026-11-20:
  chunk 5 (PR #1025, merged 2026-11-19) and chunk 13 (PR #1089,
  merged 2026-11-20) are both on origin/main (`4db35ebc`). Worktree
  `../taskboard-WT-refactor-ui-labeled-value-row`, branch
  `refactor/ui-labeled-value-row` off origin/main;
  `scripts/worktree-doctor.sh` exit 0 (130G free / 13G avail).
- **Strictly-serial O-6 ordering**: chunk 15 (priority-picker
  dropdown, W6-adjacent) comes after this chunk. Nothing here may
  block it: this chunk touches only the labeled-value family + the
  habit weekday strip; it does NOT migrate the priority picker, the
  dropdowns, or any flyout (all W5/W6 scope — untouched).

## 0. Line-ref caveat

task-view.slint refs below are from the branch worktree at origin/main
`4db35ebc` (file is 1419 lines after chunks 1–13). The inventory
report's refs (2026-11-18, pre-chunk-5/10/13) are stale; this §1 is
the re-anchored seam list and is the authoritative map for stage 2/3.

## 1. Scope — the re-anchored seam inventory

### 1.1 The ten hand-rolled task-view rows (inventory F-2, re-anchored)

| # | Row | Seam (worktree refs) | Grammar today | Target mode |
|---|-----|----------------------|---------------|-------------|
| R1 | Stack | task-view.slint `stack-column` :733–743 | display (caption + body RoleText) | `display` |
| R2 | Due date | `due-column` :744–809 (`due-cell` :761, `due-stop` stop-id 5) | (b) clickable closed cell: ActivationStop + hover surface + tooltip arm + mono coloured value + em-dash; anchor source for the DueEditor overlay (`date-anchor-*` :375–405, `push-due-anchor`) | `cell` |
| R3 | Start date | `scheduled-column` :813–869 (`scheduled-stop` stop-id 6) | (b), the Due column's twin; anchor source for the scheduled editor | `cell` |
| R4 | State | `state-column` :870–881 | display (conditional success/text-primary value colour) | `display` |
| R5 | Importance | importance column :939–961 | (a) pencil IconButton beside a mono echo + em-dash | `display` + `trailing` pencil |
| R6 | Urgency | urgency column :962–984 | (a), the Importance twin | `display` + `trailing` pencil |
| R7 | Effort Estimate | `effort-row` :994–1024 | (c) always-on editor (M9 posture): caption + `TextField` + the re-seed idiom (:1018–1023); commit on Return/focus-loss (R-9) | `editor` |
| R8 | Tracked | tracked row :1097–1156 | display + hover tooltip (no edit) + `AccessibleRole.text` value + trailing entries anchor (TrackPanelFacts mirror :1149–1154) | `display` + `trailing` |
| R9 | Title | `title-display` :628–705 (`title-stop` stop-id 3, focus-restore nonce :648–660 region) | (b) closed cell (the grammar's origin) | `cell` |
| R10 | Description | `description-section` :1313–1415 (display `description-stop` stop-id 11 + M8 stretch; edit mount :1392–1414) | (b) display + (c) editor swap; THE flexible element (vertical-stretch 1, D16d) | `cell` (display branch) / `editor` (edit branch) |

### 1.2 The three divergent edit grammars (F-2) → the widget's modes

| Grammar | Instances | Mode |
|---|---|---|
| (a) pencil-beside-echo | R5, R6 | `display` + `trailing` affordance slot; `edit-asked()` |
| (b) clickable closed cell | R2, R3, R9, R10-display, (R8's tooltip-only cousin) | `cell`: internalized ActivationStop + hover surface + focus ring + tooltip arm + `edit-asked()` |
| (c) inline/always-on editor | R7, R10-edit | `editor`: caption + `@children` value slot (Rule E: the field stays at the mount — all commit/re-seed/validity semantics remain view-side) |

The same user gesture ("edit this field") thus gets one mechanism;
the gesture *routes* stay per-row (cell click vs pencil click vs
always-on) — behavior preservation, not affordance redesign.

### 1.3 Family members outside task-view

| Instance | Seam | Ruling |
|---|---|---|
| StatCell | review-view.slint `component StatCell` :135–153 (10 mounts in Aggregate/Tracking/Reflection sections :158–266) | **Convert internals**: StatCell re-hosts on LabeledValueRow `display` (label/value/collapse-dash/elide become properties); call sites untouched |
| LabeledField | settings-view.slint `component LabeledField` :401+ (editor rows :605, :885, :897, :909) | **Stay.** It is a form-field wrapper (validity, error line, secret, staged buffers, body-role label column min space-8), not a value display — a different family. Recorded, not absorbed |
| Habit editor field rows | task-view.slint :460–552 (`is-habit` variant: Title/Description TextFields, quota SegmentedControl+TextField, anchor-time TextField + clear) | **Stay** (see D-4) — labels ride the fields' own `label:` property, not a caption/value pair; converting would fork the surface. D6's chunk-14-compatibility obligation is discharged by this ruling |
| Habit anchor-day weekday strip | task-view.slint :520–528 (`kind == 1`: seven check-glyph IconButtons) | **Convert to ToggleStrip** (D-4) — a verbatim copy of recur-editor's retired pre-W3 grammar |
| Priority slot list | task-view.slint :897–933 | Stay — picker rows, chunk 15's scope (W6/MenuShell) |
| Recur pattern echo row | task-view.slint :1183–1216 | Stay — not a label+value shape (the pattern caption IS the content; no label column) |
| Labels chip row | task-view.slint :1031–1076 | Stay — chips (W5 ActionChip scope) |

### 1.4 Chunk-13 primitives + tokens already on main (verified)

- `ui/widgets/text-button.slint` (TextButton, emphasis/size axes,
  `focused` seam), `check-box.slint` (CheckBox — the task-view done-box
  already mounts it at :577), `toggle-strip.slint` (ToggleStrip:
  options/checked-mask/picked; consumed by recur-editor :157).
- Theme R-6 tokens :124–129 (`dialog-min/max-width`,
  `menu-min/max-width`, ×base-scale). **No new sizing tokens**: the
  rows are content-sized by contract (D16d — only the description
  stretches); the widget fixes the caption/value gap at
  `Theme.space-1` (today's value everywhere) and documents the
  content-sized law in `docs/widget_interactions.org`.

## 2. Recorded decisions

### D-1 Widget contract (the customization points, per inventory §3.2)

`ui/widgets/labeled-value-row.slint` — `component LabeledValueRow`:

```
mode: enum RowMode { display, cell, editor }
label: string                       // caption RoleText, text-secondary
value: string                       // display/cell value text
value-role: Typography              // body (default) | mono | caption
value-color: color                  // default Theme.text-primary
placeholder-em-dash: bool           // true: value == "" renders "—" (default true)
trailing: @children                 // pencil / clear / anchor affordances
hint-keys: [string]                 // tooltip arm keys
// cell mode only:
stop-label / stop-description       // ActivationStop surface
stop-id: int                        // walk identity (preserved: 3/5/6/11)
edit-asked()                        // cell activation + trailing intent
value-hint-label / value-hint-description   // tooltip arm copy
// geometry seams (the overlay-anchor law):
out value-x/value-y/value-width/value-height: length
out focused: bool                   // observation seam (walk/focus restore)
```

- `display`: caption + value RoleText; `trailing` renders after the
  value; tooltip arm fires when `value-hint-label` non-empty (R8).
- `cell`: the closed cell — internal `ActivationStop` (width/height
  100%), hover surface, focus ring (`radius-small`), tooltip arm,
  pointer cursor; `activated`/click → `edit-asked()`. The widget
  re-uses ActivationStop (no verbatim re-statement — the F-8 lesson).
- `editor`: caption + `@children` value slot, `spacing: space-2`,
  vertical-alignment center on the caption. One child per the house
  law; the description edit branch mounts its pair in a child
  HorizontalLayout. No field semantics inside the widget (Rule E).
- Geometry seams: the date-anchor mirrors read the value rect
  through the `out value-*` properties instead of the anonymous cell
  Rectangles; `push-due-anchor`/`push-scheduled-anchor` keep their
  click-time-push semantics verbatim.

### D-2 Absorption map (convert / stay / why)

**Convert (12 mounts):** R1–R10 (task-view), StatCell internals
(review-view), and the habit anchor-day weekday strip → ToggleStrip
(see D-5). Every converted mount is geometry-identical: same
caption/typography/spacing/hover/ring/tooltip/walk-stop behavior —
the goldens and journeys are the zero-delta oracle.

**Stay (6, with reasons):** LabeledField (different family — form
wrapper, §1.3), habit editor field rows (labels live in the fields'
own `label:` property; D6's no-new-grammar law already held; a
conversion would fork the shared surface), priority slot list (chunk
15), recur pattern echo (not a label+value shape), labels chip row
(W5), badges/chips elsewhere (W5).

### D-3 Behavior-preservation guarantees and their oracles

| Guarantee | Oracle (must re-pass UNCHANGED) |
|---|---|
| Row geometry identical | goldens (`tests/goldens/`, `golden_visuals.rs`) — zero image delta accepted |
| Walk order + stop ids 2/3/5/6/11 intact | `walk_probe_tests.rs`, `focus_coverage.rs` (addresses may rename — address-only edits per the CheckBox precedent, W3 93463c35) |
| Due/scheduled overlay anchors + sibling rects | `overlay_migration_probe_tests.rs` (om_us1 family incl. the R-7 sibling-rect assertion) |
| Effort commit-once-on-focus-loss (R-9) + re-seed | `journeys.rs::om_us3_s2_effort_commits_once_on_focus_loss_and_never_twice` |
| Description height law across edit mount (M8) | `om_us3_s1_description_section_keeps_its_height_across_the_edit_mount` |
| Importance/urgency picker routing + dismiss | existing task-detail journeys (picker block stays view-side, untouched) |
| Habit editor unchanged (quota/time/day rows) | `habits_journey.rs` gates; H4 D6 shapes |
| Review sections unchanged | review journeys + goldens |

**Intentional changes: none sanctioned.** This chunk is pure
centralization: no affordance reshapes, no keyboard gains (unlike
W3's B-1). Any red journey is a regression to diagnose, per AGENTS §5.

### D-4 Habit editor: convert without forking the surface

H4 D6 kept the habit rows in the hand-rolled grammar precisely so
this chunk could absorb or decline them freely. Ruling: **decline the
field rows** (their labels are the fields' own `label:` property —
a LabeledValueRow conversion would duplicate the label or fork the
TextField anatomy; the create form and the editor must stay one
vocabulary). **Accept the weekday strip**: the `kind == 1` row
(task-view.slint :520–528) is a verbatim copy of recur-editor's
pre-W3 grammar (7 check-glyph IconButtons) — exactly what chunk 13's
ToggleStrip retired (F-8/§10 law: no ad-hoc re-declaration of
primitive grammars outside `ui/widgets/`). Convert to
`ToggleStrip { options: Mon..Sun; checked-mask from
root.habit.weekday == i; picked(i) → root.habit-weekday-picked(i) }`.
Single-select-over-multi is the same mask mechanism (one true slot).
The `kind == 2` day field stays a plain TextField.

### D-5 Migration order (lowest-risk first, TDD per phase)

1. **Widget + probe first (display mode)**: `labeled_value_row.slint`
   + `tests/labeled_value_row_probe.slint`/`_probe_tests.rs` — probe
   proves caption/value/em-dash/role/colour/trailing/tooltip-arm
   before any consumer moves (red first).
2. **Display conversions**: R1 Stack, R4 State, R8 Tracked; StatCell
   re-host. Goldens + review journeys green.
3. **Cell mode**: probe proves focus ring, ActivationStop activation
   → `edit-asked()`, hover surface, geometry seams. Convert R2 Due,
   R3 Start, R9 Title, R10-display. Re-point the date-anchor mirrors
   + push functions + focus-restore nonce to the widget's `out`
   seams; walk registry address-only re-anchors. Overlay probes +
   walk probe green.
4. **Editor mode + trailing**: probe proves the `@children` slot and
   the pencil trailing slot. Convert R5/R6 (trailing pencil,
   `edit-asked()`), R7 Effort (TextField stays at the mount, re-seed
   idiom verbatim), R10-edit. Effort/description journeys green —
   this discharges the J2 W-3/W-4 amends.
5. **Habit weekday strip → ToggleStrip** (D-4). Habit journeys +
   walk probes green.
6. **Docs + census**: `docs/widget_interactions.org` LabeledValueRow
   contract row (mode law, content-sized sizing note, anti-ad-hoc
   rule); `docs/ux-surface-census.org` rows re-pointed with symbol
   anchors per #775 (task-view row symbols →
   `ui/widgets/labeled-value-row.slint::LabeledValueRow`; the stay
   rows record their reason); B-record: zero behavior deltas.

### D-6 Risks

- **Slint `@children` + mode duality**: a component cannot introspect
  whether children were given, so display/cell render the built-in
  RoleText and editor renders the slot — the mode enum is the only
  switch. Mitigation: the probe catalogue pins all three modes; any
  need for "editor with built-in value" is declined (stay-recorded).
- **Geometry-seam re-pointing** (date anchors, effort/description
  rects, TrackPanelFacts mirror): absolute-position reads move from
  named Rectangles to widget `out` properties. Mitigation: keep the
  changed-handler mirror idioms verbatim at the view; the om probes
  assert sibling rects unchanged (R-7).
- **Walk registry addressing**: element addresses change when
  Anatomy moves into the widget (CheckBox precedent: address-only
  renames, closed set + mechanism unchanged).
- **StatCell collapse-dash + elide**: the `collapsed` dash and
  `overflow: elide` must survive as widget properties (`wrap:
  no-wrap` + elide on the value RoleText; the em-dash property
  covers the dash). Golden oracle.
- **Golden churn risk is the acceptance gate**: any golden delta is
  a bug in the re-host, not a re-baseline.

## 3. TDD sequence (tests first, per phase)

Phase gates in D-5 order; every phase: red probe → convert → green
probe + untouched oracle suite (`journeys.rs`, `habits_journey.rs`,
`walk_probe_tests.rs`, `overlay_migration_probe_tests.rs`,
`focus_coverage.rs`, goldens).

New probe catalogue (`labeled_value_row_probe_tests.rs`):

1. display mode renders caption + value, em-dash on empty value
2. value-role/colour forward (mono + custom colour visible)
3. trailing slot mounts an affordance and reports `edit-asked()`
4. cell mode: focus ring on focus, activation → `edit-asked()`,
   stop-id published (the walk facts push)
5. cell mode geometry seams track the value rect (anchor law)
6. tooltip arm fires with the hint copy + keys (state observation)
7. editor mode hosts children beside the caption (mount + geometry)
8. StatCell re-host: collapse dash + elide pinned by probe

No journey changes; no new census assertions beyond the re-pointed
rows.

## 4. Diff estimate

| File | Delta |
|---|---|
| `ui/widgets/labeled-value-row.slint` (new) | ~+230 |
| `tests/labeled_value_row_probe.slint` + `_probe_tests.rs` (new) | ~+260 |
| `ui/views/task-view.slint` | ~−430 / +170 (ten rows collapse; anchor seams re-point) |
| `ui/views/review-view.slint` | ~−18 / +14 (StatCell internals) |
| `ui/views/task-view.slint` habit strip | ~−8 / +8 (ToggleStrip) |
| `docs/widget_interactions.org`, `docs/ux-surface-census.org` | ~+70 |
| **Total** | ~−1050 / +1050 touched, **net ≈ −60** (probe adds, view deletes dominate) |

Quality gates: full `scripts/run-quality-gates.sh` sequence before
handoff; stage 2 gates on pre-commit hooks + flock-wrapped nextest;
the dispatcher owns the full-suite runs (dispatch discipline).
