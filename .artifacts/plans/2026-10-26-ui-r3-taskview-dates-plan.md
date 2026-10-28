# [Plan] GUI Review Round 3, Chunk 3 — Task View & Dates

- **Date**: 2026-10-26 (planned 2026-10-27)
- **Status**: PROPOSED — stage-1 plan for chunk 3 of the round-3
  dispatch. Branch `feat/ui-r3-taskview-dates`, cut from fetched
  `origin/main` (03049e1) **after chunks 1 and 2 were verified
  merged** (both branches deleted from origin; `git cherry origin/main
  <branch>` = zero `+` commits for each — rebase-merge rewrote the
  SHAs, so the patch-id check is the proof). Local `main` was
  fast-forwarded e67bbc3 → 03049e1 before the worktree was created
  (`scripts/worktree-doctor.sh` exit 0, 243 G free / 25 G floor).
- **Origin** (normative): design round
  `2026-10-26-gui-review-round3-plan.md` §3.3 (D7–D15), §1 findings
  R3-F13/14/15/16/17/28/29, §2 fold-ins (#230, #228), §4 testing, §7
  risks (R2 binds this chunk); dispatch plan
  `2026-10-26-gui-review-round3-implementation-dispatch-plan.md`
  §1/§4 (acceptance a–i). Chunk-1 artefacts this plan builds on: the
  total interpret table + `resolve_conflict`
  (`flow_table_tests.rs`), the §8 buffer-lifetime invariant with its
  explicit stage-3 hook ("Chunk 3's D11 buffer seeding builds on this
  wipe", `adapter.rs:483`), the focus-loss seam
  (`text-field.slint:44-49`), the settle-then-run fix for F18
  (audit `2026-10-26-ui-flow-state-audit-report.md`, DUE row).
- **Scope guard**: `taskboard-ui-slint` + two `taskboard-app` files
  (`src/config.rs` — the `[ui]` knob; `src/ui.rs` — the typed
  hand-over). `taskboard-domain` empty diff: **verified** —
  `TaskChanges` already carries `scheduled:
  Option<Option<DateTime<Utc>>>` (`messages.rs:377`) and
  `description: Option<String>` (`messages.rs:361`); `Task` already
  carries `scheduled` (`entities.rs:416`) and `description`
  (`entities.rs:410`). No flow-machine *rules* change (chunk 1 owns
  the machine; this chunk adds table rows and uses its rules). No
  settings fields (chunk 4), no picker preset bindings / control-nav
  (chunk 6 — the boundary is recorded in §6).
- **All `file:line` anchors are post-chunk-2** (this worktree at
  03049e1). The design round's older anchors were re-derived; every
  one below was read, not carried over.

## 0. The description verify verdict (stage-1 gate, D15/#228)

**SUPPORTED — the chunk ships the description editor.** The design's
drop-on-verify condition does not fire:

- `TaskChanges.description: Option<String>` (`messages.rs:361`) — a
  change mask: `Some(text)` sets (an empty string *is* the clear),
  `None` untouched. Note the shape difference from the date fields
  (plain `Option`, not double) — the commit funnel wraps accordingly.
- `Task.description: String` exists (`entities.rs:410`); the task
  view already displays it read-only (`task-view.slint:227-240`).
- The engine's `UpdateTask` path needs no change; the journeys'
  `fake_engine` does (it applies only title/duedate/archived today,
  `tests/support/fake_engine.rs:113-125` — test-support change,
  allowed).

D8's scheduled half is supported by the same verification
(`TaskChanges.scheduled` double-Option at `messages.rs:377`).

## 1. D7 — the due cell reads interactive (R3-F13)

The collapsed cell (`task-view.slint:171-185`) has the click seam and
the AT grammar but zero hover language — it renders as static text.

- `due-cell`'s TouchArea gains `mouse-cursor: MouseCursor.pointer`.
- The Rectangle gains `background: touch.has-hover ?
  Theme.surface-hover : transparent` (the IconButton hover token).
- The new scheduled cell (§2) ships with the identical grammar — one
  look, two fields.

## 2. D8 — the start-date editor beside due (R3-F14)

The domain carries `Task.scheduled` and the presentation already has
`scheduled_phrase` (`presentation.rs:147`, currently unused by the
UI); the view never surfaces either.

**One seam, not two.** The r2 due machinery generalizes instead of
duplicating:

- `Flow::EditDue { task }` → `Flow::EditDate { task, field: DateField
  }` with `pub enum DateField { Due, Scheduled }` (`view_state.rs:174`
  — the enum-with-data A1 shape). Every `(Flow::EditDate, effect)`
  outcome is *identical* for both fields; only the commit funnel's
  `TaskChanges` mask differs. The total-table rows rename
  (`flow_table_tests.rs` regenerates; no outcome changes).
- The adapter's editor state stays singular and renames:
  `due_buffer`/`due_echo`/`due_picker` → `date_buffer`/`date_echo`/
  `date_picker`. The §8 invariant restates: non-empty ⇔
  `Flow::EditDate` armed — the refresh wipe (`adapter.rs:484-497`)
  and its `debug_assert` rename with it; `settle_flow`'s `was_due`
  becomes `was_date` (`adapter.rs:608`).
- `begin_edit_due` → `begin_edit_date(state, field)`
  (`view_state.rs:922`); `commit_due` → `commit_date`
  (`adapter.rs:3745`) reading the field off the flow and setting
  `duedate` or `scheduled` (`Some(Some(v))` set / `Some(None)` clear).
  The capture-phase Down-in-field open (`adapter.rs:296-306`) matches
  `Flow::EditDate`.
- Slint: a second metadata column "Start date" between due and state
  — closed cell (hover/click/AT, phrase `scheduled_phrase`), open
  state mounts the *same* `DueEditor` composition (label "Start date
  entry", placeholder "YYYY-MM-DD or +2d"). `TaskDetailModel` gains
  `scheduled-open`/`scheduled-text`/`scheduled-echo`; the picker push
  fields rename to the shared `date-picker-open`/`date-caption`/
  `date-grid` (the flow exclusivity guarantees one open editor).
  Window callbacks: `scheduled-edit-opened`, and the existing
  `due-*` callbacks become the shared `date-*` set — one seam each.

Non-goal kept: no scheduling *semantics* (reminders anchored on
scheduled are DR-3's territory); this chunk only edits the field.

## 3. D9 — the optional time component (R3-F15)

**Exact accepted grammar** (`due_edit.rs::parse`, `:37`; total on
arbitrary input, unchanged contract):

| Form | Meaning | Notes |
|---|---|---|
| *(empty)* | clear (`Ok(None)`) | unchanged |
| `YYYY-MM-DD` | 00:00 UTC of that date | unchanged (`:42-45`) |
| `YYYY-MM-DD HH:MM` | that UTC wall-clock | **new** — exactly one space; `%H` ∈ 00–23, `%M` ∈ 00–59 enforced by `NaiveDateTime::parse_from_str` (out-of-range refuses, never clamps) |
| `+N<unit>`, unit ∈ m/h/d/w | `now + N·unit` | unchanged; a time suffix on a relative form (`+2d 14:30`) is **not** accepted — deliberate, documented at the grammar |
| anything else | `DueParseError` | buffer kept, nothing dispatched |

Leading/trailing whitespace trims first (existing). Seconds are not
accepted (`HH:MM` only — the design's form; `at most second-precision`
is a display bound, not an input one).

- **UTC is the grammar's zone** (today's rule, `iso_parses_to_midnight_utc`):
  `14:30` means 14:30 UTC. The review focus' date-shift risk is
  pinned by proptest: `parse(format_entry(x)) == x` truncated to the
  minute, ±10 y (§10) — the seed formatter (below) and the parser
  cannot disagree by an offset because both are UTC-fixed and the
  crate has no tz database (the existing presentation rule).
- **Echo**: `sets 2030-01-31` / `sets 2030-01-31 14:30` (time shown
  only when present) — `echo()` (`:77`) gains the time arm.
- **Closed-cell phrase**: `due_phrase`/`scheduled_phrase` append `
  · HH:MM` when the instant's UTC time ≠ 00:00 (helper
  `at_time_suffix(Option<DateTime<Utc>>) -> String` in presentation,
  composed inside both phrases — one place; midnight instants render
  exactly as today). Relative phrasing stays the lead ("in 2d ·
  14:30"); the tint carries overdue/done as before.
- **Seed formatting** (feeds §5): `format_entry(Option<DateTime<Utc>>)
  -> String` — `None` → `""`; `Some` → `%Y-%m-%d` (midnight) or
  `%Y-%m-%d %H:%M`; UTC.
- **Picker time steppers** (the stage-1 widget decision): a row
  between the month caption and the weekday header — `−` IconButton,
  a mono readout, `+` IconButton; each press steps the picker's
  time-of-day by **15 minutes**, wrapping over midnight (the date
  follows — a step is an instant move, not a wall-clock edit).
  `DuePickerState` gains `time: Option<NaiveTime>` (None = date-only;
  the readout shows `--:--`). `open()` seeds the time from the
  anchor's non-midnight time. Enter and day-clicks commit cursor-date
  + picker time (None → midnight — today's shape, so existing
  journeys keep their instants when the steppers are untouched).

## 4. D10 — the picker is a default-open overlay (R3-F16)

**Mount point**: the window-level overlay stack
(`app-window.slint`) — dismissal layer + panel, `if (task-view
.date-picker-visible)`, declared after the view chrome beside the
ActionMenu/palette overlays (the later-sibling hit-test, the menu's
probe-P1 shape, `app-window.slint:363-373`).

**New composition** `ui/compositions/date-picker.slint`: the month
header, weekday row, day grid — and the new time-stepper row — move
out of `due-editor.slint`'s inline `picker-panel` (`:79-181`). The
FocusScope and its raw-key map (codes 0–8) move with it, plus
`in property <bool> focus-on-mount` (`init => if (focus-on-mount)
{ self.focus(); }`). `due-editor.slint` keeps field + echo + calendar
toggle, and exposes `out property <length> field-anchor-x/y/width/
height` from its field row; `TaskView` forwards the open editor's
anchor (`date-anchor-*`) plus the visibility/grid/caption fields.

**Anchor geometry source**: `absolute-position` — the toolbar's
proven idiom (`toolbar.slint:33-36`, `menu-button.absolute-position.x`
et al.). Live bindings, not snapshots: layout shifts re-evaluate them.

**Clamps** (the main-menu formula, `app-window.slint:386-405`):
below the field when it fits
(`anchor-y + anchor-height + Theme.space-1`), else above
(`anchor-y - self.height - Theme.space-1`), both clamped into
`[Theme.space-2, root.height - self.height - Theme.space-2]`
(status bar included); `x` clamped the same way off `anchor-x`;
width = `min(root.width - 2 * Theme.space-2, max(natural-min,
self.preferred-width))` — the min-viewport fallback is the menu's
narrow-window shape. **Geometry pin (acceptance d)**: a journey
asserts the metadata row's y-position is unchanged while the picker
is open — it pushes nothing.

**Dismiss layer**: full-window, no visual, mounted iff the picker is
open; `clicked => root.date-picker-dismiss-requested()` → the adapter
drops the picker only (editor and flow stay armed, buffer intact). A
consumed outside click is the established overlay contract here
(menu/palette/capture); the trap-risk the review focus names is a
*lingering* layer — impossible by construction, the layer's mount is
the same condition as the panel's.

**Default-open on edit entry** (acceptance d) and focus:

- The begin seam (`on_task_due_edit_opened`, `adapter.rs:3610`, and
  its scheduled twin) opens the picker right after
  `begin_edit_date`, with `focus-on-mount = false` — **the text field
  keeps focus**; typing-first is preserved (the picker is visible,
  click-armed, and idle).
- The Down-in-field stroke (capture-phase, keyboard intent) sets the
  one-shot `focus-on-mount = true` flag — the grid takes focus and
  the arrows drive, exactly today's behaviour.
- Escape: unchanged grammar — the panel never handles it; the action
  layer's insert-discard ends the whole editor (the r2 clean close).
- Closing the editor closes the picker on *every* path: the commit
  funnel, discard, leaving-settle (`was_date`), and the reconcile
  wipe already own this — chunk 1's rules are load-bearing here.

**Interplay with chunk-1 flow rules** (the dispatch's interplay item):
R1 focus-loss settles the flow (picker dies with it); R3
settle-then-run keeps F18 fixed (Settings under the open editor
settles first — the audit's DUE row); R5 visible affordance = field +
mounted panel. No rule changes, only table-row renames.

## 5. D11 — the value survives entry; the x clears it (R3-F17)

- **Seeding**: the begin seam seeds `date_buffer =
  format_entry(current_value)` (+ `date_echo` from `echo()`) after
  `begin_edit_date` returns, before `refresh` — inside the armed
  window, so the §8 invariant (non-empty ⇔ armed, wiped otherwise)
  holds untouched. The hook comment at `adapter.rs:483` names this
  exact seam.
- **Enter with untouched value re-commits unchanged** (acceptance e):
  the seed parses back to the same instant; `commit_date` dispatches
  the same-value `UpdateTask` — explicitly blessed by the acceptance
  (the engine treats it as a content no-op). It can never clear.
  Click-out still *settles* without committing (`settle_flow` →
  `end_edit_date`, `view_state.rs:1537`) — no spurious writes.
- **The x-clear variant API** (`text-field.slint`): alongside
  `commit-affordance` (`:34`), an independent
  `in property <bool> clear-affordance: false` rendering an "×" Path
  in the trailing slot under the same visibility rule as the
  checkmark (`:112` — focused + non-empty; the two flags are
  independent, a field carries one in practice). Click ≡ select-all +
  delete, **not** ≡ Return: it empties the buffer and fires
  `edited("")` (staged clear — the echo flips to "clears the due
  date"; Return commits it). AT: button role, label "Clear",
  description = the field's label, `accessible-action-default` on the
  same seam. The due and scheduled fields enable it; the title keeps
  `commit-affordance`; the settings reset-x is chunk 4's flavour on
  this mechanism (out of scope here).
- **Recorded semantics change**: the journey leg "Empty accept
  clears" (`journeys.rs:4755`) is *rewritten by design* — the new
  legs are "open → Return untouched ⇒ same-value re-commit (no
  clear)" and "open → x → Return ⇒ `duedate: Some(None)`". The old
  leg pinned the accidental-clear limitation D11 removes; per AGENTS
  §5 this is a designed redefinition recorded here, not a weakened
  test.

## 6. D12 — month-follow + keyboard month buttons (R3-F28)

- **Month-follow fix**: `move_days` (`due_edit.rs:132`) syncs the
  displayed month to the moved cursor (`self.month =
  first_of_month(moved)`) — the cursor can never leave the grid; the
  "grid simply shows no cursor" desync dies. `set_cursor` already
  syncs; `move_months`' clamp stays; `month_edge` stays. The pinned
  unit test `arrows_move_days_without_dragging_the_month`
  (`due_edit.rs:358` — its own comment says *deliberately* desyncs)
  is rewritten; the grid proptest gains the cursor-in-displayed-month
  invariant. Recorded decision: D12/F28 redefines the semantics.
- **Month buttons keyboard-activatable**: `icon-button.slint`'s
  FocusScope (`:57`) gains `key-pressed` — Return/Enter/Space →
  `root.clicked(); accept`, else `reject`. Direct keys, widget-level
  (every IconButton becomes fully keyboard-operable; the tab stop and
  AT activation already existed — this adds the raw-key half).
- **Boundary with chunk 6 (recorded)**: the picker's `Key.*` map
  stays hardcoded (codes 0–8 over `picker-key`); preset-adapted
  picker bindings, the control-nav walk (`D25`), and the emacs-mode
  arrow conflict (F27: the chooser hardbinds arrows even in emacs
  mode) are chunk 6's registry work. This chunk delivers: the
  existing arrows/Enter/Escape untouched, month buttons on
  Space/Return now.

## 7. D13 — the reminder rows humanize (R3-F29)

- `RuleRowModel.created` (`models.rs:935`) drops `to_rfc3339()` for
  `rule_created_phrase(created_at, now)` — a presentation helper
  returning the UTC calendar date, short form `%Y-%m-%d` (created is
  bookkeeping metadata; "at most the second" is a ceiling — date
  precision is below it, and the `T`/offset noise is the finding).
- `before` stays `offset.text_form()` — "d/h/m as authored" already
  holds; `rule-list.slint` renders the same two columns, no layout
  change.
- Tests are structural only (the no-text-assertions rule): the
  created string contains no `T`, no `+`, length ≤ 10.

## 8. D14/#230 — `due_soon_days` plumbs from config to pixels

- `taskboard-app/src/config.rs` — `UiSection` (`:369`) gains
  `due_soon_days: u32`, serde default 1, validated `0..=3652`
  (ten years); out-of-range → a new typed `ConfigError` variant (the
  `InvalidScale` fail-fast precedent — never a silent clamp). `0` is
  legal and means "nothing is due soon" (an empty soon set).
- Typed hand-over (`ui.rs:212`): `UiSettings.due_soon_horizon:
  chrono::TimeDelta`, derived app-side (`TimeDelta::try_days`) —
  the crate receives typed values, nothing reads config mid-flight
  (the `day_start` consumers contract). `Default` derives 24 h from
  the existing constant — `presentation::DUE_SOON_HORIZON`
  (`presentation.rs:27`) stays the single magic number.
- Adapter holds the horizon; `push` passes it to
  `due_sets(state, now, horizon)` (`models.rs:486` — the only
  call site) and to `due_edit::cell_kind(date, today, horizon)`
  (`:205` — the grid tint follows the same knob: one horizon
  everywhere). The domain call inside `due_sets`
  (`taskboard_domain::due_soon`, `presentation.rs:57`) takes the
  parameter; no domain change.

## 9. D15/#228 — the description edits in place

Verdict: ships (§0). Widget and seams:

- **Entry grammar = the title's** (W-SM-d): the read-only display
  block (`task-view.slint:225-240`) gains hover + pointer +
  click-to-edit (`description-edit-opened`); the task context menu's
  edit path still exists (chunk 1's re-arm rule — always re-armable).
- **Flow**: new variant `Flow::EditDescription { task }` — total-table
  rows in `flow_table_tests.rs` (new rows, outcomes per the edit
  rules: SetMode(Normal) discards, Activate commits, leaving settles).
  Buffer = the shared `view_state.buffer` (the title's). The commit
  funnel dispatches `UpdateTask { changes: TaskChanges { description:
  Some(buffer.clone()), ..Default } }` — empty buffer commits
  `Some("")`, which is the clear (documented at the funnel). R5
  affordance = the field; R1 focus-loss settles (the field wires
  `focus-lost`).
- **Widget**: `TextField` gains `in property <bool> multiline:
  false` — when set: `input.wrap: word-wrap` and the field grows
  with content (height from the input's preferred-height, floored at
  today's min-height); `commit-affordance` (checkmark) composes with
  it. Commit-on-Return *and* checkmark (the design's words).
  **Stage-2 probe (recorded)**: on the pinned Slint 1.18.1, Enter in
  a wrapped TextInput fires `accepted` without inserting a newline,
  and embedded `\n` (Deck/CLI-authored descriptions) render and
  round-trip in the buffer. Fallback if Enter inserts newlines:
  checkmark-only commit + a recorded deviation note in the stage-2
  report (the design's commit-on-Return would then be impossible
  without a custom key filter — probe decides, house probe rules
  apply).
  **Stage-3 deviation (recorded, supersedes the growth clause
  above)**: the grown height binding (`min-height` from
  `input.preferred-height`) is a width-dependent layout cycle on the
  pinned Slint — a window-level binding-loop deprecation warning
  ("may cause panic at runtime"; it regressed chunk 2's
  warning-free-build gate). The multiline editor is a fixed
  three-line window instead: `clip: true`, text top-aligned, the
  typed content scrolls/clips inside; zero binding-loop warnings is
  the gate.
- Markup rendering stays deferred (POST-1.0/R10 note unchanged).

## 10. Testing catalogue (TDD-first; one behaviour, one test, lowest layer)

**Unit/prop — `due_edit.rs`** (the pure core):
- Time-form table: `2030-01-31 14:30` → the exact UTC instant;
  refusals: `2030-01-31 24:00`, `2030-01-31 12:60`,
  `2030-01-31  14:30` (double space), `2030-01-3114:30`, `+2d 14:30`.
- Proptest, canonical round-trip + **date-shift guard**: for
  instants ±10 y, `parse(format_entry(x)) == x` truncated to the
  minute (pins the UTC handling; the review-focus (d) pin).
- Proptest extensions: junk-never-panics covers the new arm;
  relative-is-now-plus-n unchanged.
- Echo: time arm ("sets 2030-01-31 14:30").
- `format_entry`: None/date/midnight/date+time forms.
- Picker: stepper moves time ±15 min and wraps midnight (date
  follows); `open` seeds a non-midnight anchor's time; Enter commits
  cursor-date + time; day-click carries the time; untouched steppers
  keep midnight commits (existing instants stable).
- Month-follow: `move_days` syncs the month; proptest — after any
  move sequence, the cursor (when on-calendar) is inside the
  displayed month; the rewritten former desync pin.
- `cell_kind` takes the horizon parameter (boundary flips with it).

**Unit — `presentation.rs`**: `at_time_suffix` (midnight → empty,
else ` · HH:MM`); `due_phrase`/`scheduled_phrase` suffix composition;
`due_sets(state, now, horizon)` (a task at horizon+1 is not soon);
`rule_created_phrase` structural facts (no `T`, no `+`, ≤ 10 chars).

**Config — `taskboard-app/tests/config.rs`**: absent → 1; `14`
round-trips; `0` accepted; `4000` → the typed rejection.

**Journeys — `tests/journeys.rs`** (boundary pins, typed commands):
- Scheduled parity: open start-date editor → typed ISO reaches the
  boundary as `scheduled: Some(Some(_))`; x → Return ⇒
  `scheduled: Some(None)` (acceptance b).
- Seeding/no-accidental-clear (acceptance e, the rewritten leg): due
  set → open → Return untouched ⇒ the *same-value* `UpdateTask`; open
  → x → Return ⇒ `Some(None)`.
- Time entry (acceptance c): typed `2030-02-28 14:30` reaches the
  exact instant through the funnel.
- Overlay geometry pin (acceptance d): picker open ⇒ panel mounted at
  window level AND the metadata row's y unchanged (pushes nothing).
- Dismiss layer (review focus c): outside click closes the picker
  only — editor still armed, buffer intact, no dispatch; Escape still
  ends the whole editor.
- Default-open: entering due edit ⇒ `date-picker-open` true with the
  text field focused; Down-in-field ⇒ the grid holds focus.
- Month-follow journey: arrow across the month edge ⇒ the caption
  month flips and a cursor cell exists in the pushed grid; month
  button keyboard: Tab to "Next month" + Return ⇒ month advances
  (acceptance f).
- View-switch under the editor settles then runs (F18 guard; one
  journey — the table pins the rest).
- Description: click-to-edit → type → Return ⇒
  `description: Some(_)`; Escape ⇒ no dispatch (acceptance i).

**Flow table — `flow_table_tests.rs`**: rows renamed for
`Flow::EditDate { Due, Scheduled }` + new `Flow::EditDescription`
rows; totality re-proven by the harness.

**Test support**: `fake_engine.rs` applies `changes.scheduled` and
`changes.description`.

**Tier-4 manual (this machine, stage-2 report checklist)**:
- *Overlay feel*: panel sits under the field; at minimum viewport
  (~600 px) the clamps keep it inside; outside-click dismissal feels
  immediate; no dead zones after close.
- *Time-entry feel*: typing date+time, the live echo, the 15-min
  stepper cadence; steppers reachable by Tab; readout legible.
- *Keyboard feel*: the full walk (field → calendar button → panel;
  Down into the grid; month buttons via Space/Return).
- *Description feel*: wrap, growth, checkmark + Return, focus-loss
  commit.
- *Side-by-side*: due + start-date columns read as one system.

## 11. Commit slices (stage 2; standalone, rebase-clean)

1. `feat(ui-slint): the date grammar grows the time component` —
   parse/echo/format_entry + tests (§3).
2. `feat(ui-slint): the picker follows the cursor and gains time
   steppers` — month-follow, `time` state, steppers, `cell_kind`
   param (§3, §6a).
3. `feat(ui-slint): the trailing affordance gains a clear variant` —
   `text-field.slint` + tests (§5).
4. `feat(ui-slint): the date editors unify on one seam` —
   `Flow::EditDate`, seeding, scheduled column, x-clear wiring, flow
   rows (§2, §5; depends 1–3).
5. `feat(ui-slint): the date picker mounts as a clamped overlay` —
   `date-picker.slint`, app-window mount, dismiss layer, default-open,
   geometry pin (§4).
6. `feat(ui-slint): the icon buttons answer the keyboard` —
   key-pressed (§6b).
7. `feat(ui-slint): the reminder rows humanize the created stamp` —
   presentation + models + structural tests (§7).
8. `feat(app): the due-soon horizon becomes configuration` — config,
   hand-over, plumbing (§8; the app-crate slice).
9. `feat(ui-slint): the description edits in place` — flow variant,
   multiline variant, seams + probe record (§9).

Each slice keeps the tree green (`cargo nextest run --workspace` at
minimum); CHANGELOG Unreleased accumulates; stage 2 runs the full
AGENTS §10 gate suite through `scripts/run-quality-gates.sh`.

## 12. Risks & recorded probes

- **R2 (design §7) — overlay anchoring/gesture behaviour**: probe
  P-D10 opens stage-2 slice 5: mount from `absolute-position`
  anchors, pin panel-under-field by test. Named fallback (design
  pre-approved): keep the inline mount in `due-editor.slint` (the
  r2 shape) and record the deviation — the dismissal layer and
  default-open ship regardless.
- **Multiline Enter probe** (§9): decides commit-on-Return vs
  checkmark-only; recorded either way.
- **Flow-table churn**: renaming `EditDue` touches chunk 1's fresh
  harness — mechanical, no outcome changes; the harness's totality
  proof is the regression net.
- **Journey rewrite** ("Empty accept clears", §5): a designed
  redefinition, recorded; the stage-3 reviewer reads this plan.
- **Same-value re-commit traffic**: Enter-untouched dispatches a
  content no-op `UpdateTask` (clocks bump, LWW no-op) — blessed by
  acceptance (e) verbatim; noted so nobody "fixes" it into a skip.
- **`due_soon_days = 0`**: legal; empty soon set; documented at the
  type. Grid tint flips overdue-only at 0 — consistent, tested.
- **`absolute-position` staleness**: bindings, not snapshots;
  task-view has no inner scroll today. Tier-4 covers kiosk rotation.

## 13. Bookkeeping

- Closes on merge: #230 (D14), #228 (D15 — verify passed, this
  section §0 is the stage-1 gate record).
- Milestones per round-3 §8: the merge decision is the user's.
- No new issues planned; stage discoveries go to `backlog:` per
  AGENTS §7.
- Estimated diff: ~+1,400/−250 over ~15 files (`taskboard-ui-slint`
  slint + src + tests, `taskboard-app` config.rs + ui.rs) — L-sized,
  as sliced.

## 14. Precondition record

- Chunk 1 (`fix/ui-r3-flow-state-machine`): merged — remote branch
  deleted; `git cherry origin/main <branch>` = 0 unmerged patch-ids;
  its commits (through 5ab0fd1) are on `origin/main`.
- Chunk 2 (`feat/ui-r3-board-surface`): merged — same proof; tip
  commits (through 03049e1) on `origin/main`.
- Gate: `scripts/worktree-doctor.sh` exit 0 (243 G ≥ 25 G floor) at
  2026-10-27, before `git worktree add -b feat/ui-r3-taskview-dates
  ../taskboard-WT-feat-ui-r3-taskview-dates` (flat suffix, new
  branch, never a primary branch).
