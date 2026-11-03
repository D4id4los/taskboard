# [Plan] GUI Review Round 4, Chunk 10 — Tracking Surfaces

- **Date**: 2026-11-03
- **Status**: PROPOSED — stage-1 plan for chunk 10 of the round-4
  dispatch. Branch `feat/ui-r4-tracking-surfaces`, cut from
  `origin/main` @ `9e1a03eb` (`scripts/worktree-doctor.sh` passed
  pre-creation, 193 G free against the 25 G floor; the local `main`
  ref was behind, so the branch was seeded off the fetched
  `origin/main` — the merge-gate chunks live there).
- **Gates verified (2026-11-03)**:
  - *Decision gate*: round-4 §2 decision 4 (backlog absorption of
    2.5/3.4) **CONFIRMED by the user 2026-11-01** — the §2 header
    ("all CONFIRMED by the user, 2026-11-01") and risk R6 both
    record it; the backlog round-2 dispatch table marks chunk 3.4
    ABSORBED ("do not dispatch").
  - *Merge gate*: chunk 3 (icons — `75ddc62f` vendor,
    `b1519684` colorize), chunk 5 (summary row — `c5c163a1`, tick
    seam `23161de5`), chunk 7 (time combobox — `db2e78d4` pure
    layer, `393a8828` picker mount) are all contained in
    `origin/main`.
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.10 (D35–D37), §1 finding
  F17, §2 decision 4, §4 (testing strategy: the timers/tracking
  bullet), §7 (R6); dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1 (blanket acceptance rule, code-quality mandate) and §11
  (acceptance a–f).
- **Absorbed work**: backlog round-2 chunk 3.4 = #490 (today-view
  Tracked section) + #491 (task-detail entry history) — owned
  end-to-end here; both close on merge (origin §8). The absorbed
  chunk's gates are honoured: **models.rs render-lane rebase
  friction is expected** — hunks kept isolated (risk R1).
- **Lifted fences** (tracking-ui plan `2026-10-28-tracking-ui-plan.md`
  §2): the "no `LogTime`/`AdjustTimeEntry`/`DeleteTimeEntry`
  affordances" and "#490/#491 fenced" clauses are lifted BY this
  chunk. The fences that **stay binding**: no `[tracking]` config;
  no repaint-driver change (chunk 5's per-second seam + the
  wall-minute re-push suffice); **no keymap preset bindings** for the
  new verbs (the timers precedent: pointer-driven, menu-reachable);
  no menu enablement machinery (dispatch-time gating only); no
  UI-held timer state (elapsed derives per frame from the ArcSwap
  snapshot). Per dispatch §11 non-goals: no respite/break features
  (backlog 3.5), no tracker task kind (backlog 4.2) — **the form has
  no kind field; every GUI-created entry is `Work`**.
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/`) +
  the two vendored Lucide SVGs under `ui/assets/icons/`.
  `taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`, `taskboard-app`: **empty diff** (stage-3
  greps the diff over `messages.rs`/`command.rs` — must be empty; the
  verbs dispatch exactly as they exist).
- **All `file:line` anchors are this worktree @ `9e1a03eb`**; the
  dispatch prompt's anchors predate chunks 3/5/7/9 and were
  re-derived — every one below was read at this base, not carried
  over.

## 0. Findings → mechanisms audit (what the code confirms)

- **F17's parts map onto the landed seams**: the summary row exists
  (`task-view.slint:600-642` — caption "Tracked" + mono total, hover
  tip, deliberately "not itself clickable"); D35's button joins this
  row. The entries carry everything the rows need
  (`tracking.rs:38-61`: `id/task/task_title/start/end: Option/
  created_at/updated_at/kind`; the open entry IS the running timer).
  The planner already rejects bad spans typedly
  (`command.rs:2306/2358` → `CommandError::InvalidSpan` via
  `ClosedSpan::try_new`, `tracking.rs:131`). Chunk 7's widgets are
  mount-ready: `time-field.slint:5` **names this chunk in its own
  header contract** ("the tracking editor (chunk 10) mounts the same
  widget").
- **Chunk 5's total is Work-only** (`presentation.rs:1034-1052`): the
  D-11 filter comment says "the summary is Work-only (the
  `work_in_window` semantics at lifetime scope)". The overlay total
  must reuse `tracked_total` verbatim — reconciliation by
  construction (stage-3 focus c).
- **The CLI defines "tracked today" as work-only**
  (`track.rs:762-778`, `status_view`): `work_in_window` with the
  comment "respite-marked entries count against the work streak,
  never toward the tracked-today total". This decides #490's
  aggregator (§5, D-TS7).
- **Delete is not undoable** (`undo.rs:221-225`): `DeleteTimeEntry`
  classifies `NotUndoable(NotUndoableReason::TrackingSurface)` — "no
  un-delete verb exists (`LogTime` mints fresh ids)". `LogTime` and
  `AdjustTimeEntry` ARE invertible (`undo.rs:639-645`, the
  round-trip tests `undo.rs:454/523`). This decides the delete
  affordance (§4, D-TS5): the board's bare-dispatch
  `TaskDelete` (`router.rs:398`) rides journal coverage; the entry
  delete cannot, so it carries its own confirmation.
- **Adjust is a change mask** (`messages.rs:378-396`,
  `command.rs:2345-2359`): `None` = unchanged; `end: None` never
  clears a stamped end; setting an end on an open entry closes it;
  an open entry adjusted start-only stays open; the all-unchanged
  mask is the accepted `NoOp`. The edit form's mask assembly follows
  (§4, D-TS4).
- **`LogTime` needs a live task** (`command.rs:2303` `live_task`);
  adjust/delete survive soft-delete (R-10, R-7). The overlay's
  subject gate makes the live-task rejection unreachable from the UI
  (§3, D-TS2's mount condition).
- **Modal contexts are shell bits + chain flips**
  (`shell.rs:77-116`): menu/palette/focus-overlay are the three
  precedents; the palette/focus shape
  `[Modal, ViInsert, Global]` is the form-bearing one (the fields'
  printables unbound, Return reaches `accepted`). Chunk 6's D20
  masking (EditingFacts counts, engage/disengage pulls) covers the
  form's TextFields with **zero new engage seams** — a new engage
  seam here is a review finding (chunk-6 §3.6 grep clause).
- **The overlay mount pattern** (`app-window.slint:497-551` at this
  base): the dismissal layer first (full-window `TouchArea`, no
  visual, right-click dismissal on the down half, r4 D2), the
  anchored panel after (later sibling wins the hit test, probe P1),
  shared mount condition so the layer can never linger, window-clamped
  anchor geometry from a facts singleton
  (`DatePickerFacts.anchor-x/y/height`).
- **The per-second seam exists** (chunk 5, decision 5): the summary
  row accrues while a seconds surface is mounted. The overlay's
  running row joins the same dirty condition (§3, D-TS9) — no driver
  change (the fence).
- **Icon vocabulary** (`icon.slint:14+`, 18 vendored assets): the
  clock asset is already the snooze affordance (F1 was exactly a
  shared-icon bug) — the entries button vendors a **distinct** Lucide
  icon; `pencil` (edit) exists; delete vendors `trash-2`.

## 1. D35 — the entries overlay: mount, dismissal, ladder

**D-TS1 (composition + mount).** New file
`ui/compositions/track-entries.slint` exporting
`TrackEntriesPanel` (+ its row struct `TrackEntryRowModel`). Mounted
window-level beside the date-picker block (`app-window.slint`, after
it, same pattern): dismissal layer first (`clicked` +
right-click-down dismiss), panel second, both gated on one mount
condition. Anchoring: a new facts singleton
`TrackPanelFacts { anchor-x, anchor-y, anchor-height }` (the
`DatePickerFacts` pattern, new file `ui/track-panel-facts.slint`
following `walk-facts.slint`'s single-purpose-globals precedent),
published by the summary-row button; the panel sits below the anchor
when it fits (above the status bar), else above, clamped into the
window — the date-picker's exact `y` expression shape. Width: the
panel's preferred (min 360·scale, cap `root.width - 2·space-2`).

**D-TS2 (the modal context + the subject gate).** The overlay is
modal while open: `ContextId::TrackEntries` ("track-entries",
`actions.rs:26`), a `track_entries: bool` shell field, and a chain
arm after the focus-overlay arm returning
`[TrackEntries, ViInsert, Global]` (the palette/focus shape — the
form's fields ride D20's Insert masking, Return reaches `accepted`,
Ctrl-chords keep working). Mirrored from the view-state flow by
`sync_view_context` (`adapter.rs:686`), the `palette` precedent.
View-state flows (two, mutually exclusive at any instant, layered in
the ladder):

- `Flow::TrackEntries { task: TaskId }` — the list surface;
- `Flow::TrackEntryForm { task: TaskId, seed: Option<TimeEntryId> }`
  — the add (`None`) / edit (`Some`) form.

Both flows carry the subject so the mount is subject-affine. Push
gating (the strand-proof, stage-3 focus d): the window-level mount
condition is
`current-view == task-detail && track-entries-open && subject ==
flow.task && subject is live`; the view-state open arm refuses on
kiosk (D-TS8's gate); navigating back (view change) drops the flow —
the overlay cannot outlive the view, and focus falls back through
the existing walk (no stranded focus). **The Escape/E ladder** (all
presets; emacs's alternates per its table shape):

1. Field focus (Insert, via D20): Escape disengages the field
   (`ModeNormal`) — the form stays; chunk 6's rule, zero new wiring.
2. Form open, no field focus: Escape closes the form → back to the
   list flow.
3. List open: Escape closes the overlay (chain falls back to the
   view).

Mechanism: a keymap row `(ActionId::TaskTrackEntryDismiss,
ContextId::TrackEntries, ["Escape"])` whose arm is a
`UiEffect::TrackEntryDismiss` view intent; `view_state` resolves it
against the current flow (form → list; list → close) — the level-
aware `MenuBack` precedent. The context's full keymap is exactly
three rows: that Escape row + the emacs walk rows
`(NavNextControl/NavPrevControl, TrackEntries, ["C-n"]/["C-p"])` (the
per-view row pattern). Outside click dismisses via the layer (one
step: form open → list; list → close — same ladder, pointer-entry).

**D-TS3 (rows + total).** The panel: total header, the entries
ListView, the footer add button. Row: date · start · end · spent ·
[running mark] · [edit] · [delete]. Order: **newest-first**,
`(start, id)` descending — the UI's convention (the chip's open list,
notifications), the reverse of the CLI's file-format order; recorded,
not inherited. Rendering: local zone (chunk 7's echo-local rule);
date = the start's local `YYYY-MM-DD`; start/end `HH:MM`; a
midnight-crossing end renders the `+1d` mark (copy constant) when the
end's local date differs. Spent per row = `duration(now)` through
`tracked_total_phrase` (seconds finest — F16's precision ruling, one
phrase vocabulary). The running row (`end: None`): a status-dot/Badge
mark + the end cell shows the running copy constant; elapsed accrues
via D-TS9. **The header total reuses `presentation::tracked_total`
verbatim** — the same Work-only reduction the summary row shows;
reconciliation is reuse, not a parallel implementation. Respite rows
(history from the CLI) render honestly as rows but contribute zero
to the total — the D-11 ruling, recorded beside the helper. New
`IdTables.time_entries: Vec<TimeEntryId>` (display order,
index-aligned with the pushed rows): row-activate (edit), the delete
arm, and the armed-delete resolve entry ids through it — the
index-seam rule, never a viewport-relative offset.

## 2. D36 — the add/edit form

**D-TS4 (fields, seeding, mask assembly, validity mapping).** The
form is the panel's second surface (replaces the list in-place; the
ladder steps back):

- **Fields**: date — a text field with an embedded month grid:
  chunk 7's `DatePicker` composition gains an additive
  `show-time: bool` in-property (default `true`; the entry form
  passes `false`, hiding the TimeField row — the due editor is
  byte-identical). The grid embeds **inside the panel** (no second
  window-level mount, no `DatePickerFacts`/`DatePicker`-context
  contest — the due editor cannot be open while the modal overlay
  holds the chain). Picker state reuses `due_edit::DuePickerState`
  (grid/cursor/month machinery; the time half unused) plus
  `grid()/day-clicked` projection — no new date machinery. Start/end
  — two `TimeField` widgets (D26): lenient parse
  (`parse_time_lenient`), steppers (seed-on-step), quick list.
- **Seeding**: add — date = today (local), both times empty
  (`--:--`, validity neutral until typed). Edit — the entry's local
  date + `HH:MM` start/end; **an open entry seeds the end empty**
  with the running hint constant — the end cell semantics: empty end
  + commit = `end: None` in the mask (stays open, well-defined);
  typed end = `end: Some` (closes it). Editing the running row is
  therefore **well-defined**, not prevented (stage-3 focus b); the
  journey pins both commits.
- **Commit**: Return on the end field (F24's Enter-commits rule; the
  absorbed-miss convention keeps text on `false`). The mask
  assembly: add → `LogTime { task, start, end }` (instants parsed
  **local → store UTC**, D28's zone rule; DST edges normalize
  through the local calendar); edit →
  `AdjustTimeEntry { id, start: Some(parsed_start), end: Some|None,
  kind: None }` — the kind mask is always `None` (the fence). The
  all-unchanged edit commit reaches the planner's accepted `NoOp`;
  the form just closes (the CLI's honest `changed: false` posture).
- **Validity mapping** (the typed-rejection → field table):

  | Source | Maps to |
  |---|---|
  | Unparseable start text (live) | start field `Invalid` + hint constant |
  | Unparseable end text (live) | end field `Invalid` + hint constant |
  | Both parse, `end <= start` (live) | **end field** `Invalid` + hint constant (the pair's rejection lands on the later boundary); commit refused |
  | `CommandError::InvalidSpan` reply (race/degenerate DST normalize) | end field `Invalid` + hint; the form stays open — never swallowed silence |
  | `CommandError::UnknownTimeEntry` reply | not a field error: the form closes, the overlay refreshes (the row vanished — the stale-index no-op precedent) |
  | `LogTime`'s live-task rejection | unreachable: the subject gate closes the overlay before the task can go non-live (journey-pinned) |

  The live table is authoritative (it gates commit); the reply arms
  are defensive (the entry could change under the modal). The
  mirrors ride the existing push shape: per-field
  `FieldValidity` + hint constants (r4 D27's wiring), the same
  tri-state the due editor pushes.

## 3. The delete affordance

**D-TS5 (confirm-on-second-activate — recorded against the undo
round).** The choice is forced by the journal's own classification:
`DeleteTimeEntry` is `NotUndoable` (`undo.rs:221-225`) while the
board's `TaskDelete` dispatches bare **because** it is
journal-covered — undo-adjacency is unavailable here, so the
affordance carries the confirmation. Mechanics: each row's delete
`IconButton` (new `trash` kind, `trash-2.svg`); first activate
**arms** (tone flips to danger; the label/description copy flips to
the confirm constant — the row model carries an `armed` bit, the
adapter holds the armed entry id); second activate dispatches
`TaskTrackEntryDelete` with the row's one-shot target. Disarm on:
any other row's activation, opening the form, overlay close, or the
armed row vanishing from a refresh (the armed id is resolved through
`IdTables.time_entries` — absent → render unarmed, the stale-index
no-op). **No timer-based disarm** — state-truth, nothing ticks (the
fence). After delete the overlay stays open (multi-delete); the row
vanishes and the summary row total re-derives on refresh. Repeat
delete of a synced-away row is the planner's accepted-absent `NoOp`
(R-7) — honest, not terminal-by-deletion.

## 4. ActionIds, router arms, registry, ctx targets

**D-TS6 (naming per the registry conventions).** Five new
`ActionId`s in the `task.track-*` family (`task.track-start/stop`
exist):

| ActionId | dotted id | kind | resolution |
|---|---|---|---|
| `TaskTrackEntries` | `task.track-entries` | `UiEffect::TrackEntriesOpen` view intent | view_state opens `Flow::TrackEntries { task }` from the subject/focused task; refuses on kiosk (D-TS8) |
| `TaskTrackEntryLog` | `task.track-entry-log` | command | `ctx.track_entry_draft` → `LogTime`; `NoTarget` without a valid draft |
| `TaskTrackEntryAdjust` | `task.track-entry-adjust` | command | `ctx.track_entry_target` + draft → `AdjustTimeEntry` mask; `NoTarget` without both |
| `TaskTrackEntryDelete` | `task.track-entry-delete` | command | `ctx.track_entry_target` → `DeleteTimeEntry`; `NoTarget` without |
| `TaskTrackEntryDismiss` | `task.track-entry-dismiss` | `UiEffect::TrackEntryDismiss` view intent | the ladder (D-TS2) |

- `DispatchCtx` gains two one-shot targets (the `timer_draft`/
  `timer_target` pattern, `router.rs:319` + arms `:472-487`):
  `track_entry_draft: Option<TrackEntryDraft>` (the parsed
  `{ start, end }` instants, filled at the adapter's commit seam only
  when the live validity table passes) and
  `track_entry_target: Option<TimeEntryId>`. Consumed per dispatch;
  `route()`'s consume rule covers them.
- **Registry rows** (the timers precedent): all five rows exist
  (the `ALL`-coverage test pins `REGISTRY` total). The menu row:
  **only** `task.track-entries` — `contexts: &[Global]`,
  `Category::Task`, `menu_group: Some(Task)`, rank after the
  tracking verbs, description mandatory; the DR-5 fence's
  "menu-reachable" is satisfied by this one row. The four panel
  verbs: `contexts: &[TrackEntries]`, `menu_group: None` (never
  rendered — the two-tier rule), no preset rows in any preset
  (pointer-driven). Keymap: the three `TrackEntries` rows of D-TS2
  (Escape, C-n, C-p); the registry-snapshot test
  (`keymap_snapshots.rs`) regenerates.
- **No command-surface additions**: the router arms assemble only
  `LogTime`/`AdjustTimeEntry`/`DeleteTimeEntry` — the stage-3 grep
  over `messages.rs`/`command.rs` stays empty.

## 5. D37 — the #490 today "Tracked" section

**D-TS7 (verdict: the issue's scope IS a section; it lands here;
no remainder to file).** #490's body asks exactly for per-task
tracked-today totals + the day total, following the today-view
section pattern, empty-collapsing, fed by the window projection.
All of that fits; the issue closes on merge with no remainder filed.

- **Shape**: a new composition `ui/compositions/tracked-today.slint`
  (`TrackedTodaySection`), mounted in `today-view.slint` **after the
  four task sections** (a day summary footer; timers/habits keep
  D-P18's head positions). It is deliberately **not** a fifth
  `TodaySectionModel`: the row shape differs (title + total, no
  card), so the fixed-arity `[usize; 4]` machinery
  (`models.rs:100`) stays untouched — the timers/habits
  separate-composition precedent. `IdTables.tracked_today:
  Vec<TaskId>` (index-aligned rows) backs row activation
  (click/Enter opens the task — the today-row-activated precedent).
  Empty (no tracked work today) collapses (`if rows.length > 0`).
- **Aggregator — a recorded deviation**: the issue says
  `tracked_in_window`; this chunk uses **`work_in_window`**
  (`tracking.rs:221` — the same clipping oracle plus D-11's kind
  filter). Rationale: the issue's wording predates the respite kind
  reaching the CLI view, where "tracked today" is already defined
  work-only (`track.rs:762-778`); chunk 5's summary made the same
  call; a GUI day total disagreeing with `track status` for the same
  window would be a bug class. New pure helpers beside chunk 5's
  (`presentation.rs`):
  `tracked_today_rows(state, window, now) -> Vec<(TaskId,
  TimeDelta)>` (Work-only per-task sums, canonical task order) and a
  day-total reduction — both thin folds over `work_in_window`;
  structural tests, never text pins.
- **Window**: `[preferences] day_start_time`'s window via the
  existing `day_window` helper (the CLI's D-F6 data-passed shape) —
  spanning entries split at the boundary exactly once (the domain
  oracle's own property tests carry the math; the UI-level journey
  pins one spanning fixture visually/structurally).
- **Kiosk**: the section renders (passive display — the habits
  precedent); rows filter to the viewed board (the `push_today`
  kiosk filter, `models.rs:2074-2080`) and **the day total sums the
  filtered set** (what you see is what's summed). Row activation is
  navigation, not editing — allowed on kiosk.
- The section's rows are walk stops and hover-carriers (blanket
  rule): title tip ("Tracked today on <task>: <phrase>"), the day
  total tip; the walk rides `focus-on-tab-navigation` (the plain
  content-sized layout page-scrolls — no internal ListView; plain
  text rows don't justify the #373 viewport machinery; recorded).

## 6. The affordance button, kiosk gate, repaint

**D-TS8 (the summary-row button + kiosk gate).** An `IconButton`
(new `history` kind ← vendored `history.svg`; **not** the clock —
the clock asset is the snooze affordance's, and sharing it would
recreate F1's duplicate-icon class) at the summary row's trailing
edge (`task-view.slint:606-642`). Hover tip (label "Time entries",
description, no hint keys — unbound); a walk stop (stop-id band
**40** — 3/5/6/11 and 90+ are taken). Kiosk gate, model-side: a new
`track-entries-visible` flag on the task model = `!kiosk && subject
present` — the DR-5/4 "display stays, action empties" posture (the
total keeps showing; the affordance hides); the menu row degrades
through the view_state open arm's same kiosk refusal (no-op —
dispatch-time gating, no menu machinery).

**D-TS9 (repaint).** The overlay's running row and its total accrue
through chunk 5's existing per-second seam: the overlay counts as a
seconds surface in the adapter's dirty condition (`lib.rs`'s
repaint conditions) — **no driver change** (the fence). With no
open entry the overlay is refresh-static like the summary row.

## 7. Walk-stop + hover inventory (blanket-rule compliance)

- **Overlay panel** (its own tab cycle; ids in the **200 band**,
  instrumented for the walk probe): rows 200+i (activate = edit;
  Enter activates), per-row delete buttons (arm/confirm — Enter
  re-activates the armed affordance), the add button (footer),
  form: date field + grid (grid is one stop with arrow motion, the
  due-grid grammar), start field, end field, (commit = the end
  field's Return; Escape = the ladder). The panel is a walk root
  while mounted: Tab enters it, the ladder exits, focus returns to
  the summary-row button on close (the menu's focus-restore
  posture).
- **Hover tips** (label + description + keys-where-bound): the
  entries button, the total (exists), each row (date/start/end
  summary), each row's delete (the armed copy flip), the add button,
  the form fields (TimeField carries label/description natively),
  the date grid (the due-grid's), the Tracked-today rows + day
  total. No empty-line tips (F5's machinery, inherited).

## 8. Test catalogue (TDD; red where the behaviour is new)

**Pure units** (`presentation.rs` / `due_edit.rs`-adjacent):

- `tracked_today_rows`: Work-only filter; per-task sums; a spanning
  entry counts only its in-window segment; open accrual clamps at
  `now`; canonical order; empty day → empty vec (the collapse's
  input).
- Entry-row projection: `(start, id)` descending; the running mark
  bit; the `+1d` mark predicate; per-row phrase via
  `tracked_total_phrase`.
- Form draft helpers: local→UTC normalization (fixed offset, a DST
  edge pinned with the explicit-offset trick); the validity table
  rows of D-TS4 (each maps to its field); the mask assembly
  (closed → `end: Some`; open+empty → `end: None`; open+typed →
  `Some`).
- Reconciliation property: overlay total ≡ `tracked_total` ≡
  `tracked_in_window` over the task's Work entries at lifetime
  scope (generated entry sets).

**Journeys** (`journeys.rs`, FakeEngine command sink; fixed/paused
clocks; typed outcomes, no text assertions):

- Open/close: button → rows pushed (structural: count, order, the
  running mark), total == the summary row's value; Escape closes on
  all three presets; outside click closes; right-click closes.
- Add: seed → type → commit → exactly one `LogTime` with the exact
  instants (fixed-clock local zone, explicit offset) → the rows grow
  by one and the summary total re-derives.
- Edit (closed): row-activate → seeded fields (structural values) →
  change end → `AdjustTimeEntry { id, start: Some, end: Some,
  kind: None }` asserted verbatim.
- Edit (running): empty-end commit → `end: None` mask (stays open,
  mark keeps); typed-end commit → closes. (Stage-3 focus b.)
- Invalid span: `end <= start` typed → end field structurally
  `Invalid`, **no command dispatched** (sink empty); the reply-arm
  mapping is driven at the adapter seam with a forced
  `InvalidSpan` reply (defensive arm, honestly tested at its own
  layer).
- Delete: arm → confirm dispatches `DeleteTimeEntry { id }`;
  single-activate + Escape disarms; a refresh removing the row
  disarms; absent-id repeat is the accepted NoOp (sink-level).
- Strand-proof: overlay open → back to board → the overlay is gone,
  the flow dropped, focus on the board (no stranded stop).
  (Stage-3 focus d.)
- Kiosk: `track-entries-visible` false; the open arm refuses (no
  flow, no command); the Tracked section renders board-filtered
  with the filtered day total.
- Walk: the panel's stop inventory (every element reached, Tab
  cycles, Enter activates); the emacs C-n/C-p rows walk the panel
  under emacs; D20 masking (hints masked while a form field holds
  focus).
- Tracked section: rows activate → the task opens (the today
  table resolves); the empty day collapses.

**Goldens/snapshots**: the task-view matrix regenerates (the button
in the summary row); a new overlay golden (fixture entries incl. a
running row); the today matrix regenerates with the Tracked section
(a tracked fixture variant; the empty-day fixture pins the collapse)
— the full `today-{scale}-{width}.png` set regenerates deliberately
(R6). `keymap_snapshots.rs` regenerates for the new rows; the
registry `ALL`-coverage test extends.

**Tier-4 (manual, on the reference machine)**: form feel (the two
TimeField flyouts + the embedded grid inside a panel — the flyout
underlay's coordinate-space math at panel depth; probe item), the
armed-delete visual, the Escape ladder rhythm, list scrolling with
30+ entries (the panel's ListView cap + scroll), the today page
scrolling with a long Tracked section, kiosk ambient unaffected.

## 9. Commit slices (stage 2; standalone, rebase-clean)

1. `feat(ui-slint)` — the pure helpers + the draft/validity table
   (units red→green).
2. `feat(ui-slint)` — IdTables rows + the task-detail entries push +
   the summary-row button + goldens regen.
3. `feat(ui-slint)` — the overlay composition + mount + dismissal +
   ladder + walk/hover inventory.
4. `feat(ui-slint)` — the add/edit form (fields, seeding, masks,
   validity mapping) + the LogTime/Adjust journeys.
5. `feat(ui-slint)` — the delete affordance + journeys.
6. `feat(ui-slint)` — ActionIds/registry/router/shell context +
   keymap snapshots + the menu row.
7. `feat(ui-slint)` — the Tracked-today section + kiosk gates +
   today goldens regen + the reconciliation property.
8. `docs(ui-slint)` — CHANGELOG + the fence-comment updates
   (acceptance f: in-code fence comments lift; the tracking-ui
   plan's prose is annotated in #490/#491's closing comments, never
   retro-edited).

## 10. Risks

- **R1 — models.rs render-lane friction** (the absorbed chunk's own
  gate; `push_task_detail`/`push_today` are hot lanes with chunks
  11 + backlog rounds in flight): keep every hunk isolated — new
  pure helpers beside chunk 5's, table fields appended at the
  `IdTables` tail, push edits as separate diffs per surface.
- **R2 — the `show-time` flag touches chunk 7's composition**: the
  default must leave the due editor pixel-identical (the due-editor
  goldens are the regression tripwire).
- **R3 — the TimeField flyout inside a panel**: the widget-internal
  underlay covers the window from the widget's coordinate space;
  inside a window-level overlay that math must still land — probe
  first, cap the quick list inside the panel bounds.
- **R4 — app-window.slint mount growth** (fourth overlay + chunk
  11's dropdown mounts in flight): keep the new mount block adjacent
  to the date-picker's, one commit.
- **R5 — DST/skipped-hour local parses**: normalization can produce
  `end <= start` legitimately — the mapping (not a panic, not a
  swallow) is the contract; pinned with an explicit-offset fixture.
- **R6 — today goldens ripple**: the fixture change regenerates the
  whole `today-*` matrix — one deliberate regen commit, no
  hand-tuned PNGs.

## 11. Bookkeeping

- #490 and #491 close via this chunk (stage 3 posts ✅ DONE + PR
  link; #490's comment records the `work_in_window` aggregator
  deviation and the no-remainder verdict).
- The tracking-ui plan's §2 fences: annotated in the issues (not
  retro-edited); the in-code fence comments lift with slice 8.
- CHANGELOG Unreleased: one feature entry (tracking surfaces).
- The undo interplay is documentation-only: entry delete is
  `NotUndoable` by classification; nothing in the undo surfaces
  changes.
