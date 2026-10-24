# [Plan] GUI Review Round 2 — Chunk 4: Task view & text-field ergonomics (D22–D25)

- **Date**: 2026-10-23 (stage-1 planning agent run 2026-10-24)
- **Kind**: chunk plan (three-stage dispatch, chunk 4 of 8) — TDD-first,
  no implementation in the planning stage.
- **Origin**: design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §5 (D22–D25,
  normative), facts F-C4/F-C5/F-E5/F-E6, §10 testing strategy, §11 code
  inventory, §14-R2/R5; dispatch plan §1/§5 (chunk 4, branch
  `feat/ui-r2-taskview-textfields`, `taskboard-ui-slint` only).
- **Precondition gate (passed)**: chunk 3 (`feat/ui-r2-actions-context-menus`,
  PR #423) is merged into `origin/main` — verified via
  `git log origin/main --oneline` (tip `aba316e`, the D16–D21 changelog).
  Worktree created off fetched `origin/main` after a clean
  `scripts/worktree-doctor.sh` run (free 100 G, floor 25 G).
- **Scope guard**: `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud` diffs are
  **empty** — `TaskChanges.duedate: Option<Option<DateTime<Utc>>>`
  already exists with serde double-option semantics
  (`crates/taskboard-domain/src/messages.rs:263-276`, applied at
  `command.rs:358+`); reviewers assert the empty diff.

## 1. Verified facts (anchored, post-chunk-3 `main`)

- **F1 (D22 surface)** `ui/widgets/text-field.slint` wraps a raw
  `TextInput` in an inner `HorizontalLayout` with **no width binding on
  the input, no horizontal alignment, no outer min-width**: the
  TextInput's text-derived preferred width flows layout-ward through
  the HorizontalLayout → outer Rectangle → every ancestor layout
  (F-C4). Call sites inheriting the fix: task title
  (`ui/views/task-view.slint:86-93`), reminder custom offset
  (`ui/compositions/rule-list.slint:90-94`), settings fields, board
  create row, stack create row.
- **F2 (D23 surface)** `accepted(text)` is the single commit seam of
  the primitive; `edited(text)` mirrors keystrokes (F-C5). The
  IconButton vocabulary (`ui/widgets/icon-button.slint:14-24`) has
  **no check and no chevron-right/calendar glyph** — `IconKind` grows
  this chunk.
- **F3 (D24 surface)** Preset chips are borderless-when-idle Rectangles
  with `min-height: Theme.space-5`, **no padding, no min-width** —
  labels touch the chip edges; the custom-offset field is a separate
  row below (`rule-list.slint:69-88` vs `90-94`; F-E5). The custom
  parse handler silently absorbs a parse miss
  (`src/adapter.rs:1852-1865`: `ReminderOffset::parse_text_form(&text)`
  else `return`) — the buffer survives looking stuck (F-C5).
- **F4 (D25 surface)** The due cell is display-only with the R1-D13
  scope-cut comment at `task-view.slint:117-129`; `root.due-color()`
  and `root.task.due-phrase` are the existing DueKind projections.
  Presentation helpers to reuse unchanged:
  `due_kind`/`due_phrase`/`DueSets`/`duration_bucket`
  (`src/presentation.rs:26-161`). `commit_edit`
  (`src/view_state.rs:674-721`) is the flow-commit template.
- **F5 (seams)** `commit_text` (`src/adapter.rs:1671-1679`) drives
  `view_state.commit_edit`; `on_rule_add_custom_accepted`
  (`src/adapter.rs:1852`) is the parse-and-route pattern to extend.
  The testing backend renders real pixels — usability-components
  measures geometry from Rgba8Image runs
  (`the_hamburger_bar_…_square_at_every_geometry`,
  `tests/usability_components.rs:65-130`) — that is the D22 geometry
  proof pattern. Journeys can stroke raw keys
  (`harness.stroke(parse("Escape"))`, `tests/journeys.rs:1985+`).
- **F6** Slint is pinned `=1.18.1` with `i-slint-backend-testing =1.18.1`
  (root `Cargo.toml:38-51`) — all probes run on the pinned version.

## 2. D22 — TextField becomes a well-behaved layout citizen

### 2.1 Mechanism decision (probe-ordered, primary recorded)

**Primary (recorded)**: decouple the content from the outer
Rectangle's preferred width by *removing the inner layout term* —
the `HorizontalLayout` is replaced by an absolutely-positioned input:

```slint
input := TextInput {
    x: Theme.space-2;
    width: parent.width - 2 * Theme.space-2;   // explicit, not preferred-derived
    vertical-alignment: center;
}
```

and the outer Rectangle gains an explicit `min-width: Theme.space-15`
(new token, ≈ the settings-row / metadata-cell floor) so the field's
preferred width comes from a **token constraint, never from text
content**. `placeholder-text` stays an absolutely-positioned sibling.

**Fallback ladder** (only if the probe falsifies the primary):
(a) keep the HorizontalLayout, set `horizontal-stretch: 1` +
`min-width: 0` on the TextInput and the min-width token on the outer
Rectangle; (b) both, plus `max-width` in-property callers may pin.
Whichever branch wins is recorded in the probe test's comment.

### 2.2 The geometry proof test (first commit of the chunk, TDD)

`tests/usability_components.rs` gains
`the_text_field_keeps_its_layout_cell_while_text_grows` (pixel-run
measurement, the hamburger-square template): mount a minimal window
with the field in a fixed-width parent row at the {0.5, 1.0, 3.0}
scale × {600, 1000} width matrix, drive `edited` with a short then a
long string, assert the field's painted extent (and its right-hand
neighbour's start run) is **byte-identical** between the two states.
The test is written first and must fail on the current widget; it
doubles as the pinned-Slint probe record. No `preferred-width`
introspection APIs are relied on — rendered pixels only, so the pin
survives Slint internal changes (round-1 probe discipline).

### 2.3 Call sites

No call-site change required for the layout fix (one widget, all
fifteen-odd sites inherit). Settings-view F20 symptoms die here;
chunk 5's slider+field reuses the primitive unchanged.

## 3. D23 — Checkmark commit affordance

- **Anatomy**: `IconKind` gains `check` (a check-mark glyph drawn in
  the icon-button bar idiom) and, for D25, `chevron-right` and
  `calendar` (see §5). `TextField` gains
  `in property <bool> commit-affordance: false`.
- **Visibility rule (recorded)**: the trailing square renders iff
  `commit-affordance && input.has-focus && input.text != ""` — same
  `space-6` square language as IconButton, `radius-small`, hover
  surface, a11y label "Commit" with the field's label as description.
  It reserves no space when hidden (the field's inner width term is
  already content-free per D22; the button overlays the trailing
  padding).
- **Click seam**: `clicked => root.accepted(input.text)` — *the same
  callback Return fires*, so click ≡ key by construction; no new
  Rust callback, no new adapter code.
- **Enabled at**: task title field, reminder custom offset, board
  create row, stack create row, and (per design D23) the settings
  fields — the settings call sites are one-line property additions
  here; chunk 5's rework keeps them.
- **Tests** (journeys): visibility structure — the affordance's
  presence is observable only through its `clicked` routing, so the
  pin is behavioural: with a populated focused field, invoking the
  check seam dispatches exactly what Return dispatches
  (`assert_commands` equality between the two paths, the
  create-flow click≡key template `tests/journeys.rs:1193+`). No text
  assertions (§14-R5).

## 4. D24 — Reminder presets row absorbs the custom field

- **Chips**: `padding: Theme.space-1 Theme.space-2` +
  `min-width: Theme.space-6` (design's exact tokens), visual language
  otherwise unchanged.
- **Row composition**: the custom-offset `TextField` moves into the
  presets `HorizontalLayout` after the `for` chips with
  `horizontal-stretch: 1` and `min-width` floor; the row keeps
  `alignment: start` so the chips stay content-sized; the label
  collapses to the existing placeholder ("Custom offset, e.g. 45m")
  and the a11y `label` stays "Custom reminder offset".
- **Buffer-clear-on-parse-miss**: `on_rule_add_custom_accepted`
  (`src/adapter.rs:1852`) keeps the silent absorb (reminders-round
  D12 — UI owns no error chrome) but now resets the buffer on a miss.
  Mechanism: the field's `text` becomes two-way bound to a new
  `RuleListModel`-adjacent property is **rejected** (models are
  refresh-rebuilt; the buffer would be clobbered per refresh).
  Recorded mechanism: a dedicated window callback
  `rule-add-custom-rejected()` invoked by the adapter on parse miss;
  the rule-list composition connects it to clearing its own field
  instance's `text` (the field is remounted only on view switch, not
  per refresh — the buffer otherwise survives refreshes today, and
  that must not change).
- **Tests**: a journey typing an unparseable offset and accepting
  asserts (a) no command dispatched (`assert_commands(&[])`) and (b)
  a subsequent `edited` mirror sees the cleared buffer (observable
  via the adapter's `set_buffer` state — assert `view_state` buffer
  emptiness through the existing mirror seam). A parse-hit journey
  (e.g. "45m") still routes `ReminderRuleAdd` unchanged.

## 5. D25 — Due-date editor (closes #227; user-amended: both paths in scope)

### 5.1 The typed grammar (chrono; recorded decision)

New pure module `crates/taskboard-ui-slint/src/due_edit.rs` — parse,
picker state, no slint types, proptest-able. Grammar:

- `YYYY-MM-DD` — `chrono::NaiveDate::parse_from_str(.., "%Y-%m-%d")`,
  committed as **00:00 UTC** of that date (`DateTime::<Utc>` from the
  naive date; recorded anchoring: date-only entries mean "that day
  begins", `due_phrase` still renders the relative phrase).
- `+N<unit>` with `unit ∈ {m, h, d, w}` — relative from the refresh
  pass's `now` (the adapter's clock seam, `adapter.rs:350`), digits
  then one unit letter, mirroring the domain `ReminderOffset`
  text-form grammar (`reminders.rs:94-112`) so both text grammars in
  the UI read the same. Committed as `now + N·unit`.
- Empty input → **clear**: `TaskChanges { duedate: Some(None), .. }`.
- Anything else → typed `DueParseError` — **refuses**: no command,
  buffer kept (the field does not clear; the user sees their input).

Natural language ("next friday") stays deferred (design D25). The
grammar set is deliberately two forms; no `HH:MM` component this
chunk.

### 5.2 Calendar popover anatomy

- **Mount form (recorded)**: an **inline expanding panel** under the
  task-view metadata row's Due cell — not a window-level
  PopupWindow. Rationale: the task view is not refresh-remounted
  while open (stable models, r2-D9), the testing backend reaches
  inline elements reliably, and window-level popups fight the
  headless focus model. New composition
  `ui/compositions/due-editor.slint`.
- **Anatomy**: header row = prev/next month `IconButton`s
  (`chevron-left` + new `chevron-right`) flanking a month/year
  `RoleText` caption; a 7-column day grid (weekday initials header
  row); day cells are `radius-small` Rectangles with a TouchArea.
  - **today marker**: `Semantic.focus-ring` border (the
    add-stack/board-bar selected language);
  - **selected/current-due highlight**: `Semantic.success` border +
  `text-primary`;
  - **due tint reuse**: cells whose date is overdue / due-soon
  relative to `now` tint text via the existing DueKind→colour
  mapping (`root.due-color()`'s projection; the classification runs
  through `due_sets` membership — no predicate duplication).
- **State**: the picker cursor (a `NaiveDate`) and the displayed
  month live in `due_edit.rs` as pure `DuePickerState`
  (`cursor`, `month`, `advance_month`, `move_cursor(Dir)` with
  month rollover, `grid(month) -> Vec<DayCell>` with leading blanks
  and classification) — the Slint side renders model rows; all
  arithmetic is Rust and unit-tested.

### 5.3 Keyboard contract (recorded, fully mouse-free)

- Open: a `calendar`-glyph trailing `IconButton` on the field row
  (the picker toggle), or `Down` while the field holds focus.
- Arrows (Left/Right/Up/Down) move the cursor day/week;
  PageUp/PageDown switch month (cursor follows); Home/End go to the
  month's first/last day.
- `Enter` commits the cursor day; `Escape` closes **without**
  committing and without clearing (clean close; buffer/echo intact).
- The grid mounts a `FocusScope`; while open, the raw strokes are
  routed through a new window callback
  `due-picker-key(int /*slint PrivateKey via key handling in-slint*/)`
  — **rejected**: recorded mechanism is in-Slint `FocusScope`
  `key-pressed` handlers calling the same callbacks the TouchAreas
  do (click ≡ key by shared callback), with the Rust adapter only
  receiving the two semantic seams below. This keeps key routing in
  the composition, testable via `harness.stroke(parse("Right"))`.

### 5.4 The one shared commit seam

Both paths funnel into **one** window callback:
`task-due-committed(string)` — payload is either the empty string
(clear) or an RFC3339 UTC instant **produced by `due_edit`** (typed
parse result or the picker cursor's 00:00 UTC). The adapter resolves
the open task (the task-view target the title commit already uses),
dispatches `StateCommand::UpdateTask { changes: TaskChanges {
duedate: parsed, ..default } }` through the established
flow-outcome lane, and refreshes. No second dispatch path exists —
picker click, picker Enter, and typed Return all produce the same
string seam. Live echo: `task-due-edited(string)` renders
`due_phrase` under the field via a pure `due_edit::echo` helper
(bucket reuse, no text assertions in tests).

### 5.5 R1-D13 comment retirement

The `task-view.slint:117-129` scope-cut comment is replaced with a
pointer to this plan (W2 closure); the display-only `RoleText` cell
becomes the collapsed state of `DueEditor`.

## 6. Test catalogue (fix → test; design §10 discipline)

All in `taskboard-ui-slint` (journeys.rs / usability_components.rs /
inline `due_edit.rs` units + proptest); mock clock only, no sleeps,
typed outcomes, no text assertions:

- **D22**: `the_text_field_keeps_its_layout_cell_while_text_grows`
  (usability, pixel geometry, scale×width matrix) — written first,
  red on current main.
- **D23**: click≡Return journeys on the title field and the reminder
  offset (assert_commands equality); affordance inert on empty
  buffer (no command from the check seam when `text == ""`).
- **D24**: unparseable-offset accept → no command + buffer cleared;
  parse-hit routing unchanged (regression pin of the existing
  journey); chip geometry pin (painted width ≥ min-width token, the
  pixel-run pattern) at scale 1.0 only (the token scales linearly).
- **D25 typed**: ISO commit → `UpdateTask{duedate: Some(Some(t))}`
  with t at 00:00 UTC; `+2d` commit → now+2d under the mock clock;
  empty accept → `Some(None)` clear; invalid accept → no command,
  buffer retained (observable via a following `edited` mirror).
- **D25 picker (seam journeys)**: open renders the current month
  grid (model row count = month length + leading blanks);
  arrow-navigation moves the cursor (model projection);
  PageDown switches month; Enter on the cursor dispatches the same
  command the typed path produced for the same date (click ≡ key);
  day-cell click ≡ cursor+Enter; Escape closes with no command and
  the editor returns to the collapsed cell.
- **proptest** (`due_edit`): grammar totality — for arbitrary
  `NaiveDate` in ±10 y, `format!("%Y-%m-%d")` round-trips; for
  arbitrary (u32 ≤ 10^6, unit), `+N<unit>` parses and equals
  now + N·unit; arbitrary junk strings are refusals **or** valid
  (never panics, never a wrong instant); `DuePickerState::grid`
  invariants (7-divisibility, leading blanks < 7, cursor ∈ month).
- **Engine snapshots**: none — no domain/state diff (§ scope guard).

## 7. Tier-4 manual checklist (handed to the user in the PR body)

1. Checkmark appears while typing in the title/offset fields and
   feels immediate (hover surface, no layout shift).
2. Settings fields no longer grow/slode leftward while typing at
   scales 0.5 / 1.0 / 3.0 (F20 feel).
3. Calendar popover at the 480×400·scale floor and at scale 3.0:
   the grid fits the task view, no window resize on open/close.
4. Month switching and day clicking feel responsive; the today
   marker and overdue tint read correctly at both scales.

## 8. Code inventory & estimated size (`taskboard-ui-slint` only)

- `ui/widgets/text-field.slint` (D22 primary mechanism + D23
  affordance), `ui/widgets/icon-button.slint` (IconKind: `check`,
  `chevron-right`, `calendar`), `ui/compositions/rule-list.slint`
  (D24), `ui/views/task-view.slint` (D25 mount + D13 comment
  retirement), new `ui/compositions/due-editor.slint`,
  `ui/app-window.slint` (three new callbacks), `src/due_edit.rs`
  (new), `src/adapter.rs` (due seams, buffer-clear callback),
  `src/view_state.rs` (a `Flow::EditDue { task }` variant handled by
  a due-commit fn beside `commit_edit` — the flow lane template),
  `src/models.rs` (picker grid model rows), `src/lib.rs` (exports).
- Tests: `tests/journeys.rs` (+~250 lines), 
  `tests/usability_components.rs` (+~60), `src/due_edit.rs` inline
  units + proptest (+~200).
- **Estimated diff**: ~700–1000 lines, one crate, no public API
  change outside the crate's slint surface. Commit slicing (each
  standalone, rebase-merge hygiene): (1) D22 probe test red→green +
  widget fix, (2) D23 affordance + journeys, (3) D24 row + buffer
  clear, (4) D25 module + typed path, (5) D25 picker + journeys,
  (6) changelog.

## 9. Risks

- **R-a Slint layout semantics (§14-R2)**: the D22 primary mechanism
  rests on absolutely-positioned children not feeding a Rectangle's
  preferred width on 1.18.1 — mitigated by the probe-first red test
  and the recorded fallback ladder; if both branches fail, escalate
  with the probe evidence rather than a Window-level hack.
- **R-b Focus choreography (D25)**: `TextField.init` grabs focus; the
  picker FocusScope must take focus on open and hand it back on
  close without an init-loop. Pinned by the Escape-clean-close
  journey; Tier-4 item 3 covers the feel.
- **R-c Key routing in-Slint**: `key-pressed` on the composition
  must not shadow the task-view Escape (view close) while the picker
  is open — order: picker Escape consumed first, view Escape only
  when collapsed; journey asserts the collapsed-close still works
  after a picker open/close cycle.
- **R-d Refresh interaction**: the picker's cursor must survive a
  refresh while open (stable models, r2-D9) — cursor state lives in
  the adapter (`due_edit` state on the Adapter, pushed to models),
  not in Slint element state, so a model push cannot reset it.
- **R-e Text-bearing additions invite text assertions (§14-R5)**:
  echo phrases and a11y labels are pinned structurally only.
