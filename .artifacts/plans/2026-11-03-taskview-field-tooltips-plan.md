# [Plan] Backlog Round 3, Chunk 3.1 — Task-View Field Tooltips (#590)

- **Date**: 2026-11-04 (authored; file name reserved by the dispatch
  prompt as 2026-11-03 — kept verbatim so stages 2/3 find it).
- **Status**: PROPOSED — stage-1 plan for chunk 3.1 of the backlog
  round 3 dispatch. Branch `feat/taskview-field-tooltips`, cut from
  `origin/main` (c5dddaea; `scripts/worktree-doctor.sh` passed
  pre-creation, 217 G free against the 25 G floor; gate chunk r4-10
  verified merged — PR #613, rebase-merged 2026-11-04).
- **Origin** (normative): issue #590 (the runtime limitation + the
  LabeledField direction); origin report
  `2026-11-03-ui-r4-taskview-focus-stage3-report.md` §5 (the recorded
  deviation and the follow-up ruling); r4 design plan
  `2026-11-01-gui-review-round4-plan.md` D21 (the sweep inventory and
  the blanket rule); dispatch plan `2026-11-03-backlog-round3-dispatch-plan.md`
  §4 chunk 3.1 (acceptance + the three stage prompts).
- **Scope guard**: `taskboard-ui-slint` markup, its tests, and the
  plan/CHANGELOG artifacts only. **Zero Rust `src/` changes** (§4's
  copy policy is what makes this true — no model fields, no adapter
  pushes, no keymap rows). No tooltip-machinery changes (TooltipState,
  TooltipPanel, InfoTip internals: the r4 chunk-4 hover fixes are
  built on, never reworked — stage-2 hard rule). Not in scope: the
  today timers' timing field and the palette input (different view
  surfaces; #590 is the task view — decision D8), the settings form
  (already tipped), closed cells/chips (tipped by r4 chunks 5/6).
- **All `file:line` anchors are this worktree at c5dddaea**; every one
  below was read, not carried over.

## 0. Findings → mechanisms audit (what the code confirms)

- **The killed arm's mechanism, grounded.** The original r4 §5
  mechanism (a tip arm inside `TextField`) died at stage 3 (report
  §5.1; the sweep journey's doc-comment,
  `tests/journeys.rs:1629-1632`). The code shows why: the inner
  `TextInput` covers the field's whole rect minus padding
  (`widgets/text-field.slint:192-199` — `x`, `width: parent.width -
  2*space-2`, `height: parent.height`), and on the pinned runtime that
  `TextInput` consumes its own hover moves (pointer caret placement).
  Hence a sibling `TouchArea` inside the field rect never hovers (the
  input wins the pointer), and one layered above steals the click /
  drag-select events — caret handling breaks. **Any mechanism that
  puts a hover sensor inside or over the field's rect is dead on
  arrival; the tip trigger must be a sibling layout cell beside the
  field.** That is exactly how settings does it.
- **The settings precedent (`LabeledField`,
  `ui/views/settings-view.slint:218-271`).** A form row: visible
  label `RoleText` + `InfoTip` + `TextField` in a HorizontalLayout,
  with the single `help` fact driving BOTH the InfoTip's tip
  (`about`/`help`) and the field's `description` (the a11y delivery —
  the D31 single-source rule; `settings-view.slint:226-227, 250-253`).
  The InfoTip square itself (`widgets/info-tip.slint`) is
  AT-transparent by ruling (no accessible-role, not focusable — the
  widget header records why: a focusable tip button would be an AT
  dead-end), mounts a `TouchArea` that arms the generalized
  `TooltipState` delay/panel, and is already the shared primitive.
- **The task view's post-r4 tip state.** The closed cells
  (title/due/scheduled/description displays), the done box, the
  tracking buttons, the header IconButtons and the tracked total arm
  tips via their own `TouchArea`s (task-view.slint:314-328, 389-403,
  479-498, 545-563, 624-639); the preset chips via `PresetChip::touch`
  (rule-list.slint:55-77). **The tip-less surface is exactly the five
  open-editor field sites**:
  1. title editor — `task-view.slint:340-350` (`TextField`, mounted
     while `task.edit-open`);
  2. description editor — `task-view.slint:730-741` (multiline
     `TextField`, mounted while `task.description-edit-open`);
  3. due editor field — `compositions/due-editor.slint:70-89` (the
     composition row: field stretch 1 + calendar `IconButton`; mounted
     while `task.due-open`);
  4. scheduled editor field — the same `DueEditor` composition
     (`task-view.slint:571-584`, `field-label: "Start date entry"`);
  5. custom reminder offset — `compositions/rule-list.slint:198-221`
     (the presets row: chips + field stretch 1; **always mounted** —
     the only tip square visible in the closed view).
- **The caret-guard journeys exist and are load-bearing.**
  `return_never_hijacks_on_a_focused_field_under_any_preset`
  (`journeys.rs:6431`) runs the 12-cell matrix (standard/emacs/vi ×
  custom-offset/title/description/timers-timing), and its offset leg
  *clicks the field centre* (`click_field_centre`, `:6457`) before
  typing — a caret break or a geometry change flips it red. The walk
  journey (`the_task_view_stop_inventory_walks_in_reading_order`,
  `:6124`) pins the stop inventory including both open-editor
  branches. Both are the regression guards and pass **unmodified**
  (§7).
- **The walk is safe by construction.** `InfoTip` carries no
  `accessible-role` and no `focus-on-tab-navigation` — it adds no tab
  stop, so the stop inventory (ids 2/3/5/6/90-93/11 + the unreported
  fields) is unchanged at every site, including the DueEditor's
  open-branch leg whose step-counting guard (`journeys.rs:6213-6222`)
  only skips focusable affordances.
- **Goldens capture the closed state only.**
  `the_task_view_goldens_hold_across_the_scale_height_matrix` (6
  cells, `golden_visuals.rs:479-522`) and
  `the_focused_cell_golden_pins_the_ring` (`:190-252`) walk into the
  detail with **no editor open**. Four of the five tip squares mount
  only inside open-editor branches — but the **custom-offset square
  mounts always**, so exactly the seven task-view PNGs drift (§7,
  decision D7). No other golden (board/today/kiosk/notification/
  review/overlay) renders the task view's presets row.

## 1. The wrapper (D1) — shared `TippedField`, not a LabeledField migration

**Decision: one new shared widget `ui/widgets/tipped-field.slint`;
all five task-view sites consume it. Settings' `LabeledField` stays
as-is.**

Shape (the settings LabeledField *pair*, minus the form-row
furniture):

```slint
export component TippedField inherits HorizontalLayout {
    // label drives the field's a11y label AND the tip's `about`;
    // help drives the field's a11y description AND the tip's
    // description slot — one source per help fact (the D31 rule).
    in property <string> label;
    in property <string> help;
    in property <string> placeholder;
    in property <string> initial-text;
    in property <bool> selected-on-focus: false;
    in property <bool> multiline: false;
    in property <bool> commit-affordance: false;
    in property <bool> clear-affordance: false;
    in property <FieldValidity> validity: FieldValidity.neutral;
    in-out property <string> text <=> field.text;
    callback edited(string);
    callback accepted(string);
    callback focus-lost();
    public function grab-focus() { field.grab-focus(); }
    spacing: Theme.space-2;
    field := TextField {
        placeholder: root.placeholder;
        label: root.label;
        description: root.help;          // ← the AT delivery of the tip fact
        initial-text: root.initial-text;
        selected-on-focus: root.selected-on-focus;
        multiline: root.multiline;
        commit-affordance: root.commit-affordance;
        clear-affordance: root.clear-affordance;
        validity: root.validity;
        horizontal-stretch: 1;
        edited(t)  => { root.edited(t); }
        accepted(t)=> { root.accepted(t); }
        focus-lost() => { root.focus-lost(); }
    }
    help-tip := InfoTip { about: root.label; help: root.help; }
}
```

The forwarding set is the **union of what the five sites actually
use**, not the whole `TextField` surface: `secret`/`autofocus`/
`natural-height` stay unreached (a site needing them uses a bare
`TextField` — the wrapper is for tipped fields only; the boundary is
recorded on the widget).

Why this shape and not the alternatives:

- **Why not migrate settings' `LabeledField` onto it (D2)?**
  `LabeledField` is a different composite: a *form row* with a visible
  label column, the settings validity/error machinery, the secret eye,
  and the `field-index` seam convention. It predates this chunk (r2
  D31, r3 D19 restructure), its help single-sourcing already holds
  internally, and its tip ids (`LabeledField::help-tip`) are pinned by
  `every_settings_field_carries_its_info_tip` (`journeys.rs:4206-4210`).
  Migrating it buys zero behavior and churns a stable, journey-pinned
  surface. What generalizes is the *pair discipline* (field ↔ tip, one
  label/help source, a11y description as the AT delivery) — the new
  widget expresses it; both wrappers share the `InfoTip` primitive.
  The generalization direction is one-way: the task view must not
  re-implement `LabeledField` (stage-3 focus) — it doesn't; it
  consumes the smaller shared kernel. Migration of `LabeledField`
  itself is a recorded follow-up idea, not this chunk (file a
  `backlog:` issue if raised at review).
- **Why not per-site markup wiring (row properties at each enclosing
  component, no new widget)?** It would re-state the pair grammar
  (field + square + the two single-source bindings) in four files —
  exactly the duplication the acceptance rules out ("the wrapper
  generalizes rather than duplicating the settings shape"). Slint
  1.18 has no child-slot mechanism, so a shared wrapper must bake the
  `TextField` in and forward — that cost is bounded (~13 members,
  declarative forwards, no behavior copied; the §4 latch-triplication
  finding in the origin report was about copy-pasted *behavior*, which
  this is not) and it buys one grammar, one id
  (`TippedField::help-tip`) for the journeys, and one home for the
  sibling-placement rule.
- **Why not the r4 §5 arm (tip properties inside `TextField`)?** Dead
  by the recorded runtime finding (§0): the trigger would sit inside
  the field rect. Not revisited.

## 2. Why the wrapper cannot break caret handling (the mechanism argument)

1. **The trigger is a sibling cell, never a sensor over the input.**
   `TippedField` is a HorizontalLayout whose two cells are the
   `TextField` (stretch 1) and the `InfoTip` square (pinned
   `space-3`, min==max on both axes). The `InfoTip`'s `TouchArea` is
   inside the *square's* bounds; the square occupies new row space
   and does not intersect the field's rect. The field's geometry
   contract is untouched: the inner `TextInput` keeps its full
   hover/click surface (`text-field.slint:192-199`), so click-to-caret
   and drag-select reach the input exactly as before — by layout
   construction, not by z-order care.
2. **The trigger machinery is the settings-proven one.** The
   `InfoTip` is the same widget the settings fields have hovered
   since r2 D31 (`an_info_tip_hover_arms_the_generalized_tooltip`,
   `journeys.rs:4227`); `TooltipState`/delay/panel are untouched
   (stage-2 hard rule). Nothing new can steal the pointer or the
   keyboard: the tooltip panel is display-only by design
   (`tooltip.slint:77-80` — no TouchArea, no focusability).
3. **The guard set makes the argument load-bearing.** The unmodified
   journeys *exercise* the caret paths after the change:
   `return_never_hijacks…` clicks the offset/title/description field
   centres and types under all three presets; the commit-affordance
   journeys click inside the focused fields; the walk journey walks
   out of the open editors. If the wrapper disturbed the field's
   pointer or focus surface, these fail red — the guard is the proof,
   not a claim.
4. **The structural pin of the mechanism.** The new journey asserts
   per site that the tip square's rect does not intersect the field's
   rect (`help-tip.x ≥ field.x + field.width` at every site — all
   squares trail the field; §7). A future refactor that slides the
   square over the input flips this red before any caret breaks.

## 3. Per-site inventory (site → label + help + keys; D4/D6)

Labels are the field's a11y label verbatim (the house rule: the
tooltip never says what the a11y tree doesn't — r4 §5). Help
sentences are new; wording below is the proposed copy (stage 2 may
polish grammar, not scope). Keys are `[]` everywhere (D5). "Sees
square in closed view" = whether the golden/closed-state renders
change.

| # | Site (enclosing component, mount condition) | Tip label (= TextField.label) | Tip help (new; = TextField.description) | Keys | Square visible closed? |
|---|---|---|---|---|---|
| 1 | Title editor (`task-view.slint`, `task.edit-open`) | `Task title` | `The task's name as it appears on the board and lists.` | `[]` | no |
| 2 | Description editor (`task-view.slint`, `task.description-edit-open`) | `Description` | `The task's longer body text; rendered as plain text.` | `[]` | no |
| 3 | Due editor field (`DueEditor`, `task.due-open`; `field-label` default `Due date entry`) | `Due date entry` | `The task's deadline. Type a date, a relative offset like +2d, or use the calendar button.` | `[]` | no |
| 4 | Scheduled editor field (`DueEditor`, `task.scheduled-open`; label `Start date entry`) | `Start date entry` | `When work on the task starts. Same typed grammar as the due date.` | `[]` | no |
| 5 | Custom reminder offset (`RuleList` presets row; always mounted) | `Custom reminder offset` | `Adds a reminder the given time before the due date.` | `[]` | **yes** |

Site 5's label + help are the r4 §5 table's ruling carried out (the
plan pre-ruled exactly this pair; this chunk implements it).

## 4. Copy policy (D9): markup literals at the sites, single-sourced by the wrapper

The five help sentences land as **markup literals at each call site**,
fed once into `TippedField { label: …, help: … }` — one literal per
fact, and the wrapper derives both the tip and the a11y description
from it. No `TaskDetailModel` fields, no adapter pushes, no Rust
constants — **zero Rust `src/` churn**.

This follows the landed r4 precedent: chunk 5/6's sweep copy (labels,
descriptions, hint projections aside) lives as markup literals in
task-view.slint/rule-list.slint despite the identical
"presentation constants" directive in that dispatch — the reviewed
interpretation being that presentation-layer string literals *are*
the presentation constants (copy lives in markup, never pushed state,
never asserted by tests). The settings `HELP_*` Rust consts exist
because settings copy is assembled in `settings_status.rs` mappings;
the task view's copy is view-static. The alternative (five new consts
in `presentation.rs` + five new model string fields pushed per
refresh) is recorded as rejected: it buys nothing (tests assert
non-emptiness either way — no text assertions) and costs model churn.

## 5. Hint keys (D5): `[]` for all five sites

The editors' verbs are field-native, not keymap actions: commit is
Return (the field's `accepted` seam, the click ≡ key grammar), and
cancel is the shared insert-exit (Escape → `ModeNormal`, bound in the
`ViInsert` context of every preset, `presets.rs:190/422/554` — a
shared chain verb, not an editor-specific action). No editor-specific
`ActionId` rows exist, so there is nothing to project. The house
pattern passes `[]` exactly here: the closed due/scheduled/
description cells, the chips (even with `ReminderRuleAdd` menu-only),
and the tracked total all pass `[]`; only elements whose primary verb
is a bound action (closed title → `task-title-edit-hint`, done box,
tracking buttons) project keys. The settings `LabeledField` — the
shape being generalized — passes `[]` too.

Rejected alternative: a new `push_field_cancel_hint` projection over
`ModeNormal` (mechanically small, one more `HintModels` entry). Not
taken: it introduces keymap-adjacent Rust surface for zero acceptance
value, and the Escape semantics are shared by *all* focused fields
(the D20 rule), so per-editor hints would under-state it. Revisit
only if a future sweep asks for editor-cancel discoverability.

## 6. Implementation sketch (stage 2; TDD order in §7)

1. **`ui/widgets/tipped-field.slint`** (new): the §1 shape. Import
   `TextField`/`FieldValidity` from `text-field.slint`, `InfoTip`
   from `info-tip.slint`, `Theme` tokens. The widget header comment
   records the sibling-placement rule (§2.1) and the forwarding-set
   boundary.
2. **`ui/views/task-view.slint`**: sites 1–2 swap `TextField` →
   `TippedField`. Site 1 keeps `commit-affordance`, `initial-text`,
   `selected-on-focus`, the three seams; `horizontal-stretch: 1`
   moves to the wrapper (the header-row cell). Site 2 keeps
   `multiline`, `commit-affordance`, `initial-text`, seams;
   `horizontal-stretch` and `vertical-stretch: 1` move to the wrapper
   (the flexible-element contract, D16d — the wrapper is the section's
   stretch cell; the field fills the row height, the HorizontalLayout
   default).
3. **`ui/compositions/due-editor.slint`**: the row becomes
   `TippedField` (stretch 1) + the calendar `IconButton` (unchanged).
   New `in property <string> field-help` beside `field-label`
   (default `""`); `due-field.grab-focus()` is call-compatible (the
   wrapper exposes it — the r4 D25 phase hand-over and the
   `changed field-focus-on-mount` arm keep working);
   `due-field.text` stays readable for the `changed initial-text`
   re-seed (the r4 D25 live sync) via the wrapper's two-way `text`.
4. **`ui/compositions/rule-list.slint`**: new `in property <string>
   offset-help`; `custom-field` swaps to `TippedField`; the
   accepted-miss buffer reset (`if (!root.add-custom-accepted(text))
   { custom-field.text = ""; }`) keeps working over the wrapper's
   `text`. The validity + error-line row is unchanged.
5. **`ui/views/task-view.slint` (wiring)**: pass `help:` at sites 1–2,
   `field-help:` at both `DueEditor` instantiations, `offset-help:`
   into `RuleList` — the §3 literals.
6. **`CHANGELOG.md`**: Unreleased entry (stage 2).

Untouched by construction: `TooltipState`/`TooltipPanel`/`InfoTip`
(machinery rule), `TextField` (the primitive — the wrapper composes,
never edits), all Rust `src/`, the keymap presets, the walk facts,
`EditingFacts`.

## 7. Test catalogue (TDD; red-first)

### A. New journeys (`tests/journeys.rs`)

- **`every_task_view_field_editor_carries_its_info_tip`** (red
  before the widget exists — the id query finds nothing):
  - **Presence per state**: the `distinct_spots` mechanism (the
    settings sweep's, `journeys.rs:4193-4205`) over
    `TippedField::help-tip` — closed task view: exactly 1 (site 5,
    always mounted); after opening the title editor: 2; after opening
    the description editor: 3; after opening the due editor: 3 (due
    replaces the closed cell — title/description may be closed again
    per the flow's exclusivity; the exact state sequence is stage 2's,
    the invariant is: **+1 distinct square per open field site, never
    0 and never duplicated**).
  - **The mechanism pin (§2.4)**: per site, the square's rect does
    not intersect the field's rect —
    `tip.x ≥ field.x + field.width` (every square trails its field;
    `find_by_label` resolves the field via its a11y label, which the
    wrapper keeps on the `TextField`).
  - **Functional legs per site** (the sweep's leg grammar): hover the
    square → `TooltipState.pending`; advance the mock clock 600 ms →
    `visible` and the **description slot non-empty** (structural —
    content never asserted); hover away → pending/visible cleared.
  - Sites need no new seams: editors open via the existing
    closed-cell activation (`click_by_label("Edit title")` etc., the
    walk journey's idiom); the offset field is always mounted.
- **`a_field_tip_hover_leaves_the_caret_intact`** (red-first in the
  same commit — its first assertion is the square's presence):
  click the offset field centre (the guard-grid idiom), type `45`,
  hover the tip square, advance the clock (the tip shows mid-edit),
  type `m`, Return → the drained engine command is exactly
  `AddReminderRule { offset: -45m, … }` (the full typed content
  committed — no keyboard steal, no buffer corruption) and the
  insert-mask fact still reads true (`keyboard().get_vi_insert()` —
  focus never left the field). Typed outcomes + structural facts only.

### B. The unmodified guard set (the acceptance's "caret handling
unbroken"; every test below must pass with a **zero-line diff**)

- `return_never_hijacks_on_a_focused_field_under_any_preset` (the
  12-cell matrix; `click_field_centre` recomputes against the new
  row geometry live — the click still lands on the input).
- `the_task_view_stop_inventory_walks_in_reading_order` (the
  `InfoTip` is not a tab stop — the inventory is unchanged,
  open-branch legs included).
- `the_control_walk_reaches_fields_buttons_and_the_picker` (Tab out
  of the due field still lands on the grid; the calendar button is
  one stop further only if the wrapper added a stop — it must not).
- `the_commit_affordance_click_equals_return_on_the_title_field` and
  `…_is_inert_on_an_empty_buffer`.
- `the_editing_facts_count_lifecycle_tracks_focus`.
- `the_custom_offset_field_stretches_into_the_presets_row` — expected
  green with margin (the square costs ~16 px of the ~1000 px row; the
  pin is `width > 160` at window width 1000). If it ever bites, the
  protocol is: re-measure, re-pin with the mechanism comment, report
  as a deviation — never silently weaken.
- `every_task_view_cell_and_chip_carries_its_info_tip` — body
  unchanged; **its doc-comment is rewritten** in the implementation
  commit: the "field sites stay tip-less by a recorded finding" note
  becomes false once this chunk lands, and a lying record is worse
  than none (D10). The comment will point at this plan + the origin
  report §5 for the history.
- `every_settings_field_carries_its_info_tip` and the other settings
  journeys (settings untouched; no id collision —
  `LabeledField::help-tip` vs `TippedField::help-tip`).

### C. Goldens (D7)

- **Regenerate exactly seven PNGs** with `TASKBOARD_REGEN_GOLDENS=1`
  (`golden_visuals.rs:79-81`): the six
  `the_task_view_goldens_hold_across_the_scale_height_matrix` cells
  (`task-view-{0.5,1.5,3.0}-{500,900}.png`) and
  `task-view-focus-cell-1.0-900.png`. The only expected pixel delta
  per cell: the new `space-3` circle beside the custom-offset field
  in the presets row (site 5 is the one always-mounted square). Regen
  in the same commit as the markup that changes the render (the
  keymap-snapshot precedent: artifacts regenerate with their rows).
  **Any other pixel delta in any cell is a finding** (stage 3's lens,
  verbatim).
- **No new golden** for open-editor states (rejected alternative):
  no existing golden captures an open editor; the open-editor render
  carries a focused selection whose pixel determinism is unproven on
  the headless renderer, and the tip presence/geometry is already
  structurally pinned by §A. A pixel golden for the open-editor
  composition can ride a future visual round.
- Board/today/kiosk/notification/review/overlay goldens: untouched —
  none renders the task view's presets row. An unexpected drift
  anywhere is a finding.

### D. Blanket-rule application (the acceptance's "blanket acceptance
rule applies")

The blanket rule ("every new interactive element ships hover +
walk-stop + action-layer dispatch") applies to the **sites**, and
they already comply: hover ✓ (the new tip arms per site), walk-stop ✓
(the fields are the existing tab stops; the inventory journey guards
it), action-layer dispatch ✓ (the fields' verbs ride the existing
seams — this chunk adds no seam and no verb). The `InfoTip` square
itself is not a new *control*: by the r2 D31 ruling it routes no
action, takes no click, and stays AT-transparent — its content's AT
delivery is the annotated field's `accessible-description` (§1), and
its discoverability for keyboard users is the field's own tab stop.
This is the established reading, not a new exception.

### E. Not regenerated

Keymap snapshots (no keymap rows), INSTA state snapshots (no state
changes), `cargo test --doc` (no doc changes).

## 8. Decisions register

- **D1 — wrapper shape**: shared `ui/widgets/tipped-field.slint`
  (field + sibling `InfoTip`, single-sourced label/help, forwarding
  union of the five sites incl. two-way `text` + `grab-focus()`);
  all five sites consume it. §1.
- **D2 — settings `LabeledField` stays**: different composite
  (form row + validity/error/secret/field-index machinery),
  journey-pinned ids, help single-sourcing already holds internally;
  the shared kernel is the pair discipline + the `InfoTip`
  primitive. One-way generalization: the task view must not
  re-implement `LabeledField`. Migration is a follow-up idea.
- **D3 — trigger placement**: sibling layout cell beside the field,
  never inside/over the field rect — the only mechanism compatible
  with the recorded runtime limitation. §0/§2; structurally pinned
  (§7.A).
- **D4 — hint keys**: `[]` at all five sites; the editors' verbs are
  field-native (Return commit) or the shared insert-exit (Escape),
  not editor-specific keymap actions; the chips/closed-date-cells/
  settings precedents pass `[]` exactly here. Rejected: a
  `ModeNormal` cancel projection. §5.
- **D5 — labels**: each tip's label is the field's a11y label
  verbatim (`Task title`, `Description`, `Due date entry`,
  `Start date entry`, `Custom reminder offset`). §3.
- **D6 — copy**: five new help sentences as markup literals at the
  sites, single-sourced through the wrapper into tip + a11y
  description; proposed wording in §3 (stage 2 may polish grammar,
  not scope); zero Rust churn. §4.
- **D7 — goldens**: regenerate exactly the seven task-view PNGs;
  expected delta = the presets-row square only; no new open-editor
  golden; any other drift anywhere is a finding. §7.C.
- **D8 — scope boundary**: the today timers' timing field and the
  palette input stay tip-less (different view surfaces, outside
  #590); candidates for a future sweep, not this chunk.
- **D9 — a11y contract**: the square stays AT-transparent; the help
  rides the field's `accessible-description`; the row carries no
  accessible-label (the field is the labelled element —
  `find_by_label`/AT resolution unchanged). §1, §7.A.
- **D10 — the stale record**: the sweep journey's doc-comment (the
  field-sites-stay-tip-less finding) is rewritten in the
  implementation commit, pointing at the history. §7.B.
- **D11 — the offset-stretch pin**: expected green with margin;
  if red, re-measure → re-pin with mechanism comment → report as
  deviation. §7.B.

## 9. Risks

- **R1 — the presets-row square shrinks the offset field's leftover**
  (site 5 is always mounted): the stretch pin has ~10× margin at the
  pinned window width; the risk is a threshold re-pin, not a
  redesign. D11.
- **R2 — stretch migration subtleties**: the title row and the
  description section move `horizontal-/vertical-stretch` from the
  field onto the wrapper; the field must still fill the wrapper
  (HorizontalLayout default) and keep the `space-15` min-width floor
  (inside `TextField`). Guarded by the type/edit journeys (functional)
  and the goldens (visual, closed state); the open-editor geometry
  has no pixel pin — the §A presence/overlap pins are the guard.
  Listed, low.
- **R3 — DueEditor plumbing**: `grab-focus()` and the
  `changed initial-text` re-seed must survive the swap (the r4 D25
  phase hand-over + picker live-sync depend on them). Guarded by
  `the_control_walk_reaches_fields_buttons_and_the_picker` and the
  r4 picker journeys (unmodified). Listed, low.
- **R4 — golden-regen review burden**: seven binary PNGs must be
  eyeballed per cell (the stage-3 lens); the delta is tiny and
  mechanical, but the review step is mandatory, not optional.
- **R5 — InfoTip vertical alignment in the editor rows**: the square
  is height-capped inside stretch rows; settings' `LabeledField` is
  the alignment precedent (same layout shape) — whatever it does
  there, this does here; uniformity is the contract, polish is a
  review finding at most.

## 10. Estimated diff size & commit sequence

- `ui/widgets/tipped-field.slint` (new): ~80–100 lines with the house
  comment density.
- `ui/views/task-view.slint`: ~35-line delta (two site swaps + wiring).
- `ui/compositions/due-editor.slint`: ~20-line delta.
- `ui/compositions/rule-list.slint`: ~15-line delta.
- `tests/journeys.rs`: ~150 lines added (two journeys) + the
  doc-comment rewrite.
- `tests/goldens/*.png`: 7 regenerated binaries.
- `CHANGELOG.md`: one Unreleased entry.
- **Total ≈ 300–340 lines of text + 7 binaries. Zero Rust `src/`
  changes.** S/M, as dispatched.

Commit sequence (each standalone; rebase-merge hygiene):

1. `test(ui-slint): the field-tip journeys, red-first` — the two new
   journeys only (red exactly where the widget is missing; all guard
   set members green at this commit).
2. `feat(ui-slint): the task-view fields carry tips` — the widget,
   the five site swaps, the wiring, the sweep journey's doc-comment
   rewrite, and the seven regenerated goldens (same commit as the
   markup that changes the render).
3. `docs(changelog): the task-view field tooltips`.

No push, no PR from this planning stage; stage 2 owns the branch
through gates, stage 3 ships the PR
(`feat(ui-slint): the task-view fields carry tips`) and closes #590.
