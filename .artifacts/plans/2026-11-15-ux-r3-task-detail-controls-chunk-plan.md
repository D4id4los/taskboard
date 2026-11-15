# [Plan] UX Remediation — Chunk 3: task-detail display-without-control (importance/urgency + labels GUI)

- **Date**: 2026-11-15
- **Kind**: stage-1 chunk plan for chunk 3 of the UX remediation phase
  (framework plan §6 phase 5; the dress rehearsal's third chunk and its
  one true feature chunk). Planning only — no application code in this
  stage.
- **Origin**: the remediation dispatch plan
  `.artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md` §1/§4
  (the shared execution model, the long-command discipline, and the
  dress-rehearsal mandate are binding); the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` §3 rows
  T-5 and T-6; home issues #788 (T-5), #789 (T-6), #781 (the census
  hole T-5 rides); the owning UX plan
  `2026-11-14-task-editing-ux-plan.md` (US-12 is the CLI journey that
  gains GUI siblings); the census `docs/ux-surface-census.org` (the
  Task detail slice's label rows + the missing importance/urgency
  rows); `docs/ux_planning.org` §2; the house type-driven design
  policy (`docs/coding_guidelines.org` §2.1 — checked: no new domain
  type is minted, so no A1 constructor work arises).
- **Status**: READY — preconditions verified by the dispatcher (chunks
  1–2 merged; the triage report accepted 2026-11-15; `main` carries
  the dispatch plan with the long-command discipline amendment);
  worktree `../taskboard-WT-feat-ux-r3-task-detail-controls` on branch
  `feat/ux-r3-task-detail-controls` off post-chunk-2 `main`.
- **Binding inputs of stage 2**: every decision D-1..D-10 below and
  the commit sequencing §7. Deviations need a recorded reason in the
  stage-2 report.

## 1. Posture (inherited, restated for the chunk)

The triage's B-c letter: a core verb renders but has no control, or
mutates user data no surface shows. T-5 — the card decorations render
importance/urgency (`models.rs::field_decorations`, live anchor; the
issue's `models.rs:2394-2422` anchor is stale on post-chunk-2 main)
while the only write path is the CLI's `tasks edit --importance/
--urgency`. T-6 — the whole label family (`CreateLabel`/`UpdateLabel`/
`DeleteLabel`/`AssignLabel`/`UnassignLabel`, `messages.rs:100-134`)
is CLI-only and labels render nowhere in the GUI. Both findings'
remediation is *control + display in the detail view*, the task
entity's owning surface (the census register's own scoping: "labels
bind to tasks; the Task detail slice carries the label rows").

The dispatch's non-goals bind: no card-inline label editing (T-26
stays P3); a missing domain verb would be a recorded delta — **the
domain needs nothing** (D-3). The CLI label family is the behavior
contract: the GUI surfaces engine semantics, it never re-derives them.

## 2. Code-seam inventory (verified at this worktree's HEAD)

| Seam | Live anchor | What it gives this chunk |
|---|---|---|
| `field_decorations` | `crates/taskboard-ui-slint/src/models.rs:2473-2500` | the display half of T-5: `DecorationKind::Importance/Urgency` chips with `level.text_form()` labels |
| `TaskChanges.importance/urgency` | `crates/taskboard-domain/src/messages.rs:655-668` | the write path: double-option masks (`Some(Some(level))` set, `Some(None)` clear, `None` untouched) |
| `Importance`/`Urgency` | `crates/taskboard-domain/src/entities.rs:208-286` | the 5-level vocabulary with text forms `very-low..very-high`; distinct types by ruling |
| Label commands | `crates/taskboard-domain/src/messages.rs:100-134` + `LabelChanges` `:731-738` | the whole T-6 verb family, journal-undoable except `DeleteLabel` (`NotUndoable(RestoreClass)`, `undo.rs:126-127`) |
| `Label` entity + `Task::live_labels` | `crates/taskboard-domain/src/entities.rs:830-860, 818-825` | roster rows (title, lenient `Color`, board-scoped) and the per-task live set (OR-set memberships, ADR 0004 #129) |
| CLI family semantics | `crates/taskboard-app/src/commands/labels.rs` (resolve `labels.rs:20`, create `:120`, edit `:173`, delete `:223`), `commands/tasks.rs:406-465` (`label_on` → `AssignLabel`/`UnassignLabel`), `cli.rs:1544-1572,1683,1690` | the parity contract (D-5) |
| Detail rows grammar | `ui/views/task-view.slint:27-133` (`TaskDetailModel`), the due/scheduled/recur/rules rows | the mounting surface |
| Due-editor buffer precedent | `ui/compositions/due-editor.slint`, `due_edit.rs` | the buffer+funnel pattern for text-bearing editors — consulted and **not needed** (D-1: the pickers are vocabulary choices, not text) |
| Time-combobox quick-list | `TaskDetailModel.date-time-slots/date-time-nearest-row`, `task-view.slint:127-131` | the house pushed-rows picker pattern D-1 reuses |
| Track-entries overlay | `task-view.slint:44-52` model flags, `IdTables.track_entries` `models.rs:108+` | the overlay precedent D-2 reuses (model-side open flag + id table) |
| Armed-confirm strip | chunk 2's landed grammar (`board-view.slint:25-30,85-127` `board-delete-armed`; the habits `convert-armed-row`; the settings armed-item strip) | the delete-label staging (D-2) |
| `ContextId::TaskDetail` registry | `actions.rs` (`contexts: &[ContextId::TaskDetail]` rows `:1046-1452`) | where the new ActionIds bind |
| Presets | `crates/taskboard-ui-slint/src/keymap/presets.rs` (Standard/Emacs/Vi; TaskDetail rows `:179-265`) | the free-key decision (D-4) |
| Kiosk posture | the detail view is a desktop surface (kiosk renders Today; the recur rows' "desktop-mounted" posture) | no new kiosk gating — census cells platform-qualified (D-6) |

## 3. Decisions

### D-1 — T-5 (#788): importance/urgency are two separate detail rows, each with an immediate-commit value picker

Candidates were: (a) one shared editor overlay holding both fields
(the due-editor's shared-composition precedent), (b) a single
"priority" picker mapping the importance×urgency quadrant, (c) two
separate rows. **Decision: (c), two separate rows** — an importance
row and an urgency row, mounted in the detail view directly below the
scheduled-date row (the metadata block where the due/scheduled echoes
live; the card decorations echo stays a card-side rendering and is
not duplicated into the detail view). Reasons:

- The fields are independent enums (distinct types by the domain's
  own ruling, `entities.rs:248-254`); a quadrant picker invents a
  coupling the domain does not have, and a shared buffered editor
  buys nothing because the commit is not text-bearing.
- The detail rows grammar is already one-row-per-field (due,
  scheduled, description, recur, rules) — two rows are the grammar's
  native shape (consistency & standards).

**Picker shape**: the house pushed-rows quick-list pattern (the
detail view's own time combobox: `date-time-slots` +
`date-time-nearest-row`). Each row renders its current level echo
(the domain text form — the same string `field_decorations` pushes to
the card chip) plus an edit affordance; activating it opens the
picker: six slot rows (`unset`, `very-low`, `low`, `normal`, `high`,
`very-high`), the current value pre-marked, arrow walk + Return, or
click. **Commit funnel**: no text buffer — the values are a closed
vocabulary, nothing parses, so the due-editor's buffer discipline does
not apply (recorded deviation-from-precedent with this reason).
Activation of a slot dispatches immediately:

- set: `StateCommand::UpdateTask { task, changes: TaskChanges {
  importance: Some(Some(Importance::High)), ..Default::default() } }`
  (urgency row symmetric);
- the `unset` slot commits `Some(None)` — the CLI's `none` twin
  (`cli.rs::parse_importance`, the parity contract);
- Escape or a click outside cancels: nothing dispatches.

Immediate commit is the rule-list preset buttons' posture
(`rule-add-preset-clicked` → dispatch, no staging) — the choice is
recoverable (`UpdateTask` is a journal PreImage; `AppUndo` is the
escape path), so staging would be ceremony, and the triage's B-c
letter asks for control, not confirmation.

One picker open at a time, riding the detail view's existing
editor-exclusivity posture (the due/scheduled open flags' shape): the
model carries `importance-open`/`urgency-open` exactly like
`due-open`/`scheduled-open`, and opening one settles the other.

### D-2 — T-6 (#789): the labels GUI is a detail-view labels section plus a labels overlay; delete stages through chunk 2's armed-strip grammar

Candidates for the roster's home: (a) an overlay over the detail view
(the track-entries precedent), (b) an always-mounted detail section,
(c) a board-surface manager. **Decision: both (a) and a light (b),
split by verb weight**:

- **Display + assign/unassign stay in the detail view** (the light
  section): a labels row block below the importance/urgency rows —
  one chip per live label (`Task::live_labels`), chip = color swatch
  echo + title, each chip carrying an unassign affordance (×), plus
  an "add label" affordance opening the overlay. Labels have rendered
  nowhere; the section is the `label.roster_display` remediation and
  the assign/unassign controls are one click from the display — the
  display-without-control kill is local.
- **Create / edit / delete live in the labels overlay** (the
  track-entries precedent: model-side open flag `labels-open`, the
  overlay mounts over the detail view, Escape ladders out). Roster
  rows: one per live label of the **subject's board** (labels are
  board-scoped; `Label.board`), each row = swatch + title + the
  subject's assignment toggle + edit + delete. The overlay's create
  form: title field + color swatch palette (D-5's parity note).

**Assign/unassign affordance**: the overlay row's toggle and the
detail chip's × both dispatch the same pair —
`StateCommand::AssignLabel { task, label }` /
`StateCommand::UnassignLabel { task, label }`. Both are idempotent
engine-side (the CLI's own doc: "idempotent when already attached",
`cli.rs:1682`) — the GUI gets that for free by dispatching the same
commands; no GUI-side guard re-derives it.

**Edit**: activating a row's edit arms an inline form seeded from the
row (title field + swatch palette), commit dispatches
`StateCommand::UpdateLabel { id, changes: LabelChanges { title, color }
}` with only the changed slots set (the sparse-mask discipline — an
untouched slot stays `None`).

**Delete**: the destructive verb stages through **chunk 2's
armed-confirm strip** (`D-1`/`D-2` of the r2 plan; the board-view
`board-delete-armed` grammar): first activation arms the row
(consequence-naming strip — the label detaches from every task that
wears it, the CLI's own contract "detaches it from all tasks;
idempotent", `cli.rs:1569`), Confirm dispatches
`StateCommand::DeleteLabel { id }`, Cancel/Escape disarms. Reason
(recorded): `DeleteLabel` is `NotUndoable(RestoreClass)`
(`undo.rs:126-127`) — T-1's lesson is exactly this class; staging is
required, not discretionary (the user's acceptance of the triage
report settled the confirmation posture to the floor). Assign/
Unassign/UpdateLabel stay unstaged (journal PreImages; `AppUndo`).

**Escape ladder** (the track-entries pattern): the overlay's Escape
first settles an armed row or an open edit form (disarm/discard),
then closes the overlay. The detail section's × and add affordances
are pointer-grade row verbs (the rule-row precedent — no ActionIds).

### D-3 — Domain delta: none (recorded)

Every verb the journeys dispatch exists:
`UpdateTask` (importance/urgency masks, `messages.rs:655-668`),
`CreateLabel`/`UpdateLabel`/`DeleteLabel`/`AssignLabel`/`UnassignLabel`
(`messages.rs:100-134`), `LabelChanges` (`messages.rs:731-738`). The
engine already journals them (PreImage class; `DeleteLabel`
RestoreClass). No new domain type is minted, so the A1 pattern
(`docs/coding_guidelines.org` §2.1) imposes nothing new; the
`Color` newtype stays lenient by design and the GUI never feeds it
free text (D-5). **The only new cross-layer shape is
presentation-side**: `IdTables` gains a `label_ids: Vec<LabelId>`
table (the subject board's live labels in canonical order) plus a
`detail_labels`/overlay row model — the `task_rules`/`track_entries`
pattern verbatim; no domain file changes.

### D-4 — ActionIds, contexts, and keymap preset totality

New ActionIds (registry entries in `actions.rs`, contexts bind
`ContextId::TaskDetail` unless named; hints are registry-generated,
never hand-written):

| ActionId | Context | Role | Preset bindings |
|---|---|---|---|
| `TaskImportanceEdit` | TaskDetail | opens the importance picker | Standard `F3`; Emacs/Vi per their own tables — candidates recorded, the keymap contract tests are the oracle |
| `TaskUrgencyEdit` | TaskDetail | opens the urgency picker | Standard `F4` (the CUA sequence continues after `F2`'s rename); Emacs/Vi per contract tests |
| `LabelManage` | TaskDetail | opens the labels overlay | Standard `Ctrl+L` (the labels mnemonic; verified unbound in all three presets at this HEAD); Emacs/Vi per contract tests |
| `LabelOverlayDismiss` | **`ContextId::Labels`** (new) | the overlay's Escape ladder step | `Escape` in all three presets (the `TaskTrackEntryDismiss` in `ContextId::TrackEntries` pattern verbatim); plus `NavNextControl`/`NavPrevControl` Tab/Shift+Tab rows in the new context, all three presets |

Rationale for the strokes: the detail view's TaskDetail context
currently binds F2 (title), Ctrl+R (series), arrows/Home/End, Tab
walk, PageUp/Down — F3/F4 and Ctrl+L are free in Standard; the
house rule is that retuning re-runs the contract tests and
regenerates the keymap snapshots, so **the totality tests' shape is
the existing keymap contract suite** (core-verb coverage per preset,
conflict detection, snapshot regeneration). Stage 2 verifies the
candidates against those tests and records any retune in the stage-2
report; a conflicting stroke is re-picked from the same context's
free set, never by weakening a test.

The pickers' in-picker walk (arrows/Return) and the overlay's row
verbs stay pointer/native-grade (the date-picker and rule-row
precedents): no per-slot ActionIds — the picker's keyboard path rides
the armed context the way `DatePicker` does only where the existing
native field handling covers it; recorded as a non-goal to mint
picker-walk ActionIds this chunk (reachable-not-promoted is the
menu-grade posture the census already records for row verbs).

### D-5 — CLI parity: the engine family is the contract; the GUI adds no semantics

- **Resolution**: the CLI resolves `uuid | exact title`
  (`commands/labels.rs::resolve_label`) — a CLI-boundary affordance.
  The GUI resolves ids structurally (adapter id tables), never by
  title; no GUI-side name matching is written.
- **Create's color**: the CLI accepts six lowercase hex digits or
  defaults to the Deck green (`cli.rs:478-482`). The GUI's create form
  offers a **fixed swatch palette** (the Deck default green plus a
  small fixed set) producing the same six-hex vocabulary — no free
  hex text input this chunk (the CLI covers arbitrary values; a free
  hex field would re-implement the boundary validator in the UI crate,
  violating the presentation-only rule). Recorded scope line; a
  free-form color field is a candidate backlog note, not work here.
- **Semantics inherited, never re-derived**: idempotent assign,
  delete-detaches-all, OR-set membership merges, tombstones — all
  engine-side; the GUI renders outcomes from `AppState` only.
- **Importance/urgency**: the GUI's set/clear masks mirror the CLI's
  `--importance/--urgency` set-or-`none` grammar exactly (D-1's
  `unset` slot = `none`).

### D-6 — Kiosk posture and census grammar

The detail view does not render on kiosk (kiosk renders Today; the
recur rows' desktop-mount posture is the census's recorded grammar).
Neither remediation mounts anything new on a kiosk-visible surface,
so **no visible-gating work arises** — the chunk-1 grammar's letter
("disabled where a widget exists, hint where the surface is fenced")
has no instance here. The census cells are platform-qualified
`E (desktop)` in the TaskDetail column exactly as the recur rows
carry it; the other GUI columns stay `U` with the finding-id
annotation where a genuine absence remains (below).

### D-7 — Amended journey steps (the task-editing plan's new US blocks)

The owning plan `2026-11-14-task-editing-ux-plan.md` is amended
(stage 2, per the dress-rehearsal mandate) with three new user
stories; US-12 stays as the CLI journey, its header annotated to
point at the new GUI siblings (its `E`/`C`/`D`/`X` CLI cells are
untouched):

- **US-13 — Set or clear importance/urgency (P1)**. Entry points: the
  two detail rows' edit affordances; `ActionId::TaskImportanceEdit`
  (F3) / `TaskUrgencyEdit` (F4). Steps: US-13/S1 open the picker;
  US-13/S2 pick a level → exact `UpdateTask` mask dispatch, echo
  re-renders (row echo + card chip on next pass); US-13/S3 pick
  `unset` → `Some(None)` clear; escape: Escape cancels, `AppUndo`
  reverts. Wiring row **W-11** (the two rows + pickers; Commands
  `StateCommand::UpdateTask`; row addressing subject-scoped).
- **US-14 — Assign / unassign labels on the open task (P1)**. Entry
  points: the detail labels section's chip × and the overlay rows'
  assignment toggles; `ActionId::LabelManage` (Ctrl+L) opens the
  overlay. Steps: US-14/S1 open the overlay; US-14/S2 toggle a row →
  `AssignLabel`/`UnassignLabel` exact dispatch, chips re-render;
  US-14/S3 chip × → `UnassignLabel`; escape: Escape ladders the
  overlay closed (`LabelOverlayDismiss`), `AppUndo` reverts both
  verbs. Wiring row **W-12** (section chips + overlay rows; id table
  `IdTables::label_ids`).
- **US-15 — Manage the board's labels: create, edit, delete (P1)**.
  Entry points: the overlay's create form and per-row edit/delete.
  Steps: US-15/S1 create (title + swatch) → `CreateLabel { board,
  title, color }`; US-15/S2 edit → sparse `UpdateLabel`; US-15/S3
  armed delete → strip mounts → `DeleteLabel`; escape: armed strip
  Cancel/Escape disarms (the staged delete's escape is the strip —
  the T-1 grammar), `AppUndo` reverts create/edit. Wiring row
  **W-13** (overlay form + armed rows; Commands
  `CreateLabel`/`UpdateLabel`/`DeleteLabel`).

Each block carries the §4 journey-step tables (escape columns real)
and the §6 wiring rows in the corrected PC-2 cell grammar (bare
`StateCommand::`/`ActionId::` tokens; hyphenated callbacks in the
Adapter-seam column). §7's command-contract delta gains the
**ActionId delta** (the four ids + the new context) with the
recorded "no new StateCommand" line.

### D-8 — Census updates (the #781 hole closes)

- **Mint** two new rows in the Task detail slice:
  `task.importance_edit` and `task.urgency_edit` — TaskDetail
  `E (desktop)` citing W-11/US-13 and the seam tests; CLI
  `E (pre-existing, tasks edit --importance/--urgency cli.rs:592-637)`;
  other GUI columns `U` (the card chips are display-only — the T-5
  letter; card-inline control would be the T-26 convenience class).
  These rows are the #781 hole's closure; the issue gets a closing
  comment citing the census commit.
- **Fill** the six label rows (`label.create/edit/delete/
  roster_display`, `task.label_assign/unassign`): TaskDetail becomes
  `E`/`D` per row (create/edit/delete `E` via the overlay; roster
  display `D` via the section; assign/unassign `E`), citing
  W-12/W-13/US-14/US-15; the other five GUI columns stay `U` with the
  recorded scoping note "labels surface from the detail view (the
  owning surface, D-2); board-surface/card-chip display is not this
  chunk's scope" (the #789 comment records the same).
- The stocktake/reconciliation fates: T-5 and T-6 resolve as
  remediated-by-code in this chunk's PR (the issues close on merge
  per the dispatch's completion bookkeeping); no prior ruling is
  altered.

### D-9 — Test catalogue (TDD-first; typed outcomes, never text)

- **Domain/engine**: none — no delta (D-3). The engine's existing
  label-command tests are the semantics authority; this chunk adds
  no engine test (the one-behavior-one-test-lowest-layer rule: the
  GUI layers test routing only).
- **Seam tests (exact commands)**, in the `flow_table_tests.rs`/
  `journeys.rs` house shapes:
  - the importance picker's slot click dispatches
    `UpdateTask { changes: { importance: Some(Some(level)) } }` with
    every other mask `None` (per level, incl. the `unset` slot's
    `Some(None)`); the urgency row symmetric (one parameterized pair
    of tests);
  - Escape on an open picker dispatches nothing and clears both
    open flags;
  - the overlay's assignment toggle dispatches `AssignLabel`/
    `UnassignLabel` with the row's `LabelId` resolved from
    `IdTables::label_ids` (stale-index absorb per the id-table law);
  - the create form's commit dispatches `CreateLabel { board:
    subject.board, title, color }` with the swatch's exact hex;
  - the edit form's commit dispatches `UpdateLabel` with only the
    changed slots set (sparse-mask assertion);
  - the armed delete: first activation dispatches nothing and sets
    the armed flag; Confirm dispatches `DeleteLabel { id }`;
    dismissal disarms and dispatches nothing (the r2
    `the_delete_affordance_arms_then_confirms_and_dismissal_disarms`
    shape verbatim);
  - `LabelManage` routes from the registry (the
    `router.rs::rule_verbs_route_from_their_targets` shape).
- **Presentation tests** (typed flags/rows, no text): the labels
  section derives from `Task::live_labels` + the board's live label
  set (chip count and id order, not strings); the picker slot rows
  enumerate the six-value vocabulary in order; the
  `importance-open`/`urgency-open` exclusivity (opening one settles
  the other and the date editors); the armed-strip flag is
  presentation-tested like `board-delete-armed`.
- **Keymap totality**: the existing contract suite covers the four
  new ActionIds (per-preset coverage + conflict detection + snapshot
  regeneration). No new test file is minted for this.
- **Kiosk gates**: none new (D-6) — recorded, so the absence is
  decided rather than overlooked.
- **Journey-step token rows**: the new US-13/14/15 blocks are
  remediation-era stories, so their step-token rows are REQUIRED
  gate rows (unlike the retrofit plans' documented MISSING posture):
  stage 2 runs `scripts/ux-coverage.sh` on the amended plan and every
  new wiring row + step token must come back FOUND.

### D-10 — Long-command discipline (restated as binding for stage 2)

The coverage trio is forbidden in the subagent; the stage-2 gate is
the pre-commit hooks plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe, ~4–6 min — background + poll with useful work between
polls; all landed work committed before any wait). The full AGENTS §10
suite is dispatcher-owned; CI's coverage check runs it authoritatively
on the PR.

## 4. Non-goals (recorded, with homes)

- No card-side label chips and no card-inline priority control
  (T-26's class; the labels section is detail-only) — recorded in
  D-8's census note and the #789 comment; if the user wants card
  chips it is a follow-up issue, not silent scope.
- No free-form hex color input in the GUI (D-5); the CLI covers it.
- No board-surface label manager (D-2's scoping; the detail view is
  the owning surface per the census register).
- No picker-walk ActionIds (D-4's recorded non-goal).
- No undo-class changes, no `UiRequest` lane work, no effort-field
  work (the effort observation is filed to backlog — see §8).

## 5. Risks (recorded for stages 2–3)

- **R-1 — overlay Escape interplay**: the detail view already layers
  editors (due/scheduled/recur/track-entries); the labels overlay
  must join the exclusivity posture without shadowing the view
  Escape. Mitigation: the `ContextId::Labels` masking declaration
  follows the `TrackEntries` precedent, and the settle tests pin the
  ladder (D-9's Escape tests).
- **R-2 — stale id tables**: the labels section and overlay resolve
  rows through `IdTables::label_ids`; a delete from another actor
  (sync) mid-interaction must absorb stale indices like every
  id-table seam. The seam tests include the stale-index absorb.
- **R-3 — keymap snapshot churn**: adding ActionIds regenerates the
  keymap snapshots; a retune against Emacs/Vi is possible and must be
  recorded, never silently dropped from a preset (the no-second-class
  contract).
- **R-4 — model growth**: `TaskDetailModel` is already large; the
  labels/picker fields push it further. Stage 2 keeps the pushed
  halves minimal (row models, flags, slot rows) and mirrors state
  adapter-side per the R-d discipline.

## 6. Estimated diff size

**L** (the dispatch's own sizing): ~1,300–1,800 lines — adapter
(+~400: labels pass, id table, seams, picker state), models
(+~150), `task-view.slint` (+~250: two rows + pickers + labels
section), a new `labels-overlay.slint` composition (+~200),
`actions.rs`/`presets.rs` (+~60), router (+~40), tests (+~400–600),
plan amendment + census (+~150).

## 7. Commit sequencing (every commit standalone; hooks green per commit)

1. `feat(ui): the task detail renders its labels and priority echoes`
   — presentation layer first (TDD): `IdTables::label_ids`, the
   labels section rows, the importance/urgency echo rows + picker
   slot models, presentation tests. No seams yet (display half
   lands first; the B-c kill is visible in the diff).
2. `feat(ui): importance and urgency pickers dispatch exact
   UpdateTask masks` — the picker seams + ActionIds
   (`TaskImportanceEdit`/`TaskUrgencyEdit`, registry + presets +
   snapshots) + seam tests.
3. `feat(ui): label assign and unassign route from the detail view`
   — chip ×, overlay skeleton with assignment toggles,
   `LabelManage` + `ContextId::Labels` + `LabelOverlayDismiss`,
   router + seam tests.
4. `feat(ui): label create and edit mount in the labels overlay`
   — create/edit forms, sparse `UpdateLabel`, seam tests.
5. `feat(ui): label delete stages an armed confirm strip` — the r2
   grammar on the overlay rows, seam tests (the armed triple).
6. `docs(ux): the task-editing plan gains the US-13..15 journeys and
   the census closes #781` — the amended task-editing plan (new US
   blocks, W-11..W-13, §7 ActionId delta), the census row mints +
   cell fills, the #781/#788/#789 citation notes. The
   `ux-coverage.sh --gate` green run precedes this commit (gate
   evidence in the stage-2 report).

Order rationale: domain → engine is vacuous here (no delta, D-3), so
the ladder is presentation → seams → docs, each commit compiling and
testing standalone (commit-hygiene rule for rebase merges).

## 8. Backlog findings filed during planning

- **Effort's detail-view seam absence** (new `backlog:` issue): the
  triage missed that `task.effort_edit` has no detail-view seam
  either — the GUI write path exists only through the review walk's
  atomic placement form (`plan.commit_place`,
  `adapter.rs:1270-1302`), while the effort chip renders on cards
  (`task.due_phrase_display`'s cell). Weaker than T-5 (a GUI path
  exists), so MINOR-class; filed, not fixed here (outside the
  chunk's T-5/T-6 covers).
- Comments on #788/#789/#781 recording the stage-1 decisions and the
  closing evidence path (D-8).
