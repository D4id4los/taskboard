# [Plan] ux-r8 chunk 3 — task detail editor: the day-boundary stamping law, picker pointer repairs, effort seam, #759/#706/#840

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan (TDD-first) for dispatch chunk 3 of the
  round-8 GUI review; implements the accepted UX journey plan
  `.artifacts/plans/2026-11-16-task-detail-editor-ux-plan.md`
  (US-1..US-6, W-1..W-7, rulings R-1..R-3 — all normative) per the
  dispatch plan `2026-11-16-gui-review-round8-dispatch-plan.md` §1/§4.
- **Worktree**: `../taskboard-WT-fix-ui-r8-task-detail-editor`, branch
  `fix/ui-r8-task-detail-editor`, off `origin/main` @ `e7a720dc`
  (chunk 2's merged focus-restoration work — its focus-origin/restore
  funnels in `adapter.rs`/`view_state.rs` are landed and are read
  as they exist now; US-5 rides them).
- **Status**: READY for stage 2.

## 1. Decisions (every open question the origin left, settled)

### D-1 — the domain law's exact name and signature

New pure function in `crates/taskboard-domain/src/days.rs`, mirroring
[`window_start`]'s free-fn shape (the origin §7 payload sketch's "date
→ day-window-end instant, given day start + offset"):

```rust
/// The instant that ends `date`'s local day window: the opening
/// instant of `date + 1`'s window. The N-4 stamping law's mint — a
/// date-only due/scheduled entry means "that whole day counts", so
/// the named date's stamp is its window end, never 00:00. Pure, total,
/// deterministic; with the default `"00:00"` start this is the named
/// date's next local midnight — one millisecond after the note's
/// literal `23:59:59.999`, identical classification at every
/// whole-second comparison (the origin §1 equivalence).
#[must_use]
pub fn day_window_end(
    date: NaiveDate,
    day_start_time: chrono::NaiveTime,
    offset: FixedOffset,
) -> DateTime<Utc>
```

Body: `window_start(date.checked_add_days(Days::new(1))?, ...)` —
composition only, no new arithmetic (the `None` at chrono's date
extremes maps to... see D-1a). Plus the consumer-facing one-liner on
the injected vocabulary (both consumers hold a `DayCalendar` — the GUI
adapter's `calendar()` at `adapter.rs:1809`, the CLI's
`AppConfig::resolved_day_calendar()`):

```rust
impl DayCalendar {
    /// [`day_window_end`] over this calendar (the stamping law's
    /// consumer face).
    #[must_use]
    pub fn day_end(&self, date: NaiveDate) -> DateTime<Utc>
}
```

- **D-1a — totality**: `checked_add_days` returns `None` only at
  chrono's `NaiveDate` extremes (year ±262k). `window_start` itself is
  total (fixed-offset subtraction); to keep the law total like its
  neighbours, `day_window_end` uses `date + Days::new(1)` saturating
  shape — decision: reuse the `+ Days::new(1)` arithmetic
  `window_start` already performs internally for `end` (which is the
  same operation and is documented total "for every representable
  `now` at least one day inside chrono's date range"); the law's doc
  carries the same domain-of-validity sentence. No unwrap path.
- **Purity gate**: `grep -rn "chrono::Local\|Utc::now"
  crates/taskboard-domain/src/days.rs` stays empty — the day facts
  arrive as data (the D4/D-F6 consumers contract inherited from the
  day-boundary plan §2 D4/D5; the proptest strategies
  `timestamp_strategy` / `day_start_time_strategy` / `offset_strategy`
  are reused verbatim).
- **Relationship to `day_window`**: for any `now` inside `date`'s
  window, `day_window(now, ..).1 == day_window_end(date, ..)` (the
  window is exactly 24h in a fixed offset). Pinned by proptest, not by
  implementation reuse.

### D-2 — the proptest catalogue (days.rs tests module)

Unit examples first (TDD, hand-computed like D7a–d):

1. `default_start_stamps_next_local_midnight` — `"00:00"` at a fixed
   offset: `day_window_end(D)` is D+1's local midnight.
2. `night_owl_start_stamps_next_morning` — `"03:00"` start, bare date
   `2026-11-16`: the instant is 2026-11-17 03:00 local (the "wander
   into the early morning" case, N-4's letter).
3. `the_end_equals_the_window_end_of_the_named_date` — see proptests.

Proptests (256-case config, the module's `proptest_config`):

1. `day_end_equals_the_window_end_of_the_named_date` — generated
   `(date, day_start, offset)`: construct `now = window_start(date,
   ..)`; `day_window(now, ..).1 == day_window_end(date, ..)` (the
   origin AC-1 round-trip law).
2. `day_end_is_strictly_after_every_instant_of_the_named_day` —
   `now = window_start(date) + t` for generated `t ∈ [0, 24h)`:
   `day_window_end(date, ..) > now` (the overdue-classification flip
   property — the whole configured day counts).
3. `day_end_is_exactly_24h_after_the_window_start` — the fixed-offset
   corollary (the `window_is_exactly_24_hours` twin).

### D-3 — the funnel plumbing (GUI)

`due_edit.rs`'s `parse` keeps its signature and its date-only →
midnight branch **untouched**: it is the *grammar*, and its other
consumer — the recur editor's Until bound
(`presentation.rs::parse_recur_draft`, `DraftBound::Until`) — reads a
calendar **date** back out via `day.calendar_date(instant)`; stamping
window-end there would advance every Until bound one day (window-end
of D is local D+1's day-start). One grammar, one behaviour.

The law gets a dedicated entry that composes the grammar with the
stamp (the "A1 constructor at the trust boundary" is the *commit*
seam, where the day facts live):

```rust
/// The due/scheduled stamp parse (N-4): `parse`'s grammar with the
/// date-only branch stamped at the named date's day-window end
/// (`days::day_window_end`). Explicit-time and relative forms pass
/// through unchanged (the law is date-only — origin AC-4). The recur
/// Until bound does NOT ride this (a bound is a date, not a stamp).
pub fn parse_stamp(
    text: &str,
    now: DateTime<Utc>,
    day: &taskboard_domain::days::DayCalendar,
) -> Result<Option<DateTime<Utc>>, DueParseError>
```

Implementation: `parse(text, now, &day.offset)`; on `Ok(Some(instant))`,
if the trimmed text parses as bare `%Y-%m-%d`, replace the instant
with `day.day_end(that date)`. The existing `iso_parses_to_midnight_utc`
and `iso_round_trips` tests stand unchanged (they pin `parse`, the
grammar). New `parse_stamp` tests: bare date → window-end; explicit
time → typed time; `none`-class empty → clear; junk → refusal.

Call sites (the only ones that change):

- `format_entry`/`echo` stay as-is — the phrase renders from the
  stored instant (a window-end stamp echoes e.g. "sets 2026-11-17
  00:00" under the default boundary; the display tells the truth —
  the r4 D28 posture; the TE-W-N3 staged-clear echo is untouched by
  this funnel change, the doctrine bar row the review checks).
- The adapter's accepted/picker/quick commit paths funnel through
  `commit_date` (`adapter.rs:~11185`) — the mint moves from midnight
  (via `parse`) to `parse_stamp` at the three parse call sites inside
  the `Flow::EditDate` funnels (`on_task_date_accepted`,
  `on_task_date_day_clicked` → picker commit,
  `on_task_date_time_accepted`); `commit_date` itself is unchanged
  (it routes a minted instant).
- **Picker day-click / commit-instant**: `DuePickerState::commit_instant`
  keeps its semantics for a *timed* picker (the typed time is data).
  The date-only commit (cursor, `time: None`) currently mints local
  midnight — the funnel that consumes `commit_instant` for a
  date-only picker applies the law instead: `day.day_end(picker.cursor())`
  (guarded on `picker.time().is_none()`, so a quick-list time pick is
  untouched). Mechanism recorded so AC-5 and AC-4 are both provable:
  the law applies exactly when no time component exists anywhere in
  the entry path.

### D-4 — the `+Nd` relative form keeps duration semantics (recorded
ruling)

N-4's letter names *date-naming* forms ("picking a bare day"); AC-1's
law is over "a local date D". `+Nd` names an offset from `now`
(`now + N·24h`), not a date — it keeps the current
`relative_is_now_plus_n` semantics and tests. The origin's US-1/S1
entry-points row lists `+Nd` among the date-only forms; this chunk
records the narrowing: the law covers the forms that name a day
(`YYYY-MM-DD`, the picker day click, today/tomorrow-class quick sets);
`+Nd`'s step token still resolves (the S1 journey exercises it as the
unchanged-grammar contrast arm). If the operator wants `+Nd` re-based
onto day ends, that is a separate decision — recorded here, not built
(speculative generality).

### D-5 — the CLI parity twin

`cli.rs::parse_due` (line 563) stays the clap `value_parser` (pure,
no config access — grammar unchanged, `--due`/`--scheduled`
acceptance forms unchanged per origin R-non-goal): the bare-date
branch mints a new variant instead of a UTC midnight:

```rust
pub enum DueSpec {
    Set(chrono::DateTime<chrono::Utc>),  // RFC-3339 with explicit offset — unchanged
    Date(chrono::NaiveDate),             // bare YYYY-MM-DD — minted at execution
    Clear,                               // `none` — unchanged
}
```

At the `tasks edit` execution seam (where `app.config.resolved_day_calendar()`
is available, the `habits.rs:323`/`reflection.rs:39` precedent), the
`Date(d)` arm mints `day.day_end(d)`; `Set`/`Clear` route unchanged
into the `TaskChanges { duedate/scheduled }` masks. The other
`parse_due`-family consumers (the repeat `--until` bound family at
cli.rs:751, the deliberately diverging span parser at cli.rs:876) are
**out of scope** — a bound is a date, not an instant (the module doc's
own ruling); only the `--due`/`--scheduled` set arms change.

Parity: `us1_s3` app-arm test — the same bare date through the GUI
funnel's `parse_stamp` and the CLI's execution mint produce the
identical instant (both `day_window_end` over the same day facts).

### D-6 — the HH:MM fix mechanism (N-5): focus-ordering, grammar
untouched

Read finding: the picker panel is the window-level `DatePicker`
overlay (`app-window.slint:~772`) — dismissal layer first, panel hit
first, so an intra-panel click does not hit the backdrop. The closing
path is the **settle funnel**: the due field's `focus-lost` seam
(r4 D24) fires when focus moves from the field into the picker's
`TimeField` (`date-picker.slint:170`), and the `EditDate` funnel
settles the flow — closing the whole editor+picker. The one-shot
guard exists only for the programmatic field→grid hand-over
(`adapter.rs:1105-1118`, `focus_steal` + `FocusHandover::Grid`).

Decision: the fix is a **funnel branch**, not a grammar change — the
`TimeField`'s intra-picker click arms the same one-shot focus-steal
hand-over (a `FocusHandover::TimeField` variant, or the Grid variant
broadened — stage 2 picks the smaller diff; the contract is "an
intra-picker focus move never settles the flow") before focus moves.
`parse_time_lenient` / `step_minutes` / `quarter_slots` stand (AC-3;
the existing truth-table tests are the regression proof). The
reproduction journey `us2_s1` is written FIRST and observes the
failure (the flow settles on the click) before the fix — the N-5
hypothesis (settle funnel, not backdrop hit-test) is verified by that
test: if the journey shows the backdrop path instead, the fix moves
to the ordering layer and the deviation is reported.

### D-7 — the quick-options focused slot (N-6)

The derivation exists (`due_edit::nearest_slot`, r4 D26) but the
pushed `date-time-nearest-row` (models.rs:~2440) seeds from the
picker's time (`None` → 0 → the list head, 00:00). Decision: when the
picker holds no time, the nearest row derives from the **device now's
local time-of-day** (truncated to the slot grid via `nearest_slot(slots,
Some(now_local))`) instead of `None`; a picker-held time still wins
(re-opening after a typed entry stays on the entry). Pure derivation:
the adapter computes `nearest_row` from `(slots, seed)` where seed =
picker time or now-local; the presentation test asserts the focused
**slot index** (typed), never geometry. The timer-setter/track-form
sites keep their current seeds (out of scope — N-6 names the
date-time picker's quick options; a sibling-filed decision, not a
silent change).

### D-8 — the effort detail row (#838, US-3)

- **Mount**: `task-view.slint`'s metadata block beside the priority
  rows (the r3 D-1 metadata-block grammar; the r3 D-6 posture — the
  detail view is the owning surface, R-2). Rows render the effort
  chip; an edit affordance opens the inline editor.
- **Adapter state**: `effort_editor: Option<EffortEditorState>` —
  `{ task: TaskId, buffer: String }`, seeded from
  `task.effort.map(Effort::text_form)` (empty buffer ⇔ clear seed).
  The date-editor buffer pattern verbatim.
- **Commit**: `UpdateTask { changes: { effort: Some(Some(e)) } }`
  sparse on a valid non-empty buffer (`Effort::parse_text_form`);
  empty buffer commits `Some(None)` (the walk placement's
  empty-clear posture, `adapter.rs:1836`); an invalid non-empty value
  keeps the buffer and dispatches nothing (the date-editor refusal
  posture, AC-2). Clear affordance mirrors the date fields' x-clear.
- **Exclusivity**: one editor at a time (the detail exclusivity
  posture) — opening the effort editor settles an armed
  `Flow::EditDate` (the `settle_flows_on_leaving` pattern);
  opening a date editor closes the effort editor. The editor arms
  Escape/outside per the shared settle rules.
- **Parse predicate home**: domain `Effort::parse_text_form` — the
  walk's placement parse (`adapter.rs:1839`) already calls it; the
  row calls the same function. Parity (AC-3) is a call-site test over
  one corpus asserting both fronts agree (accept/reject/None), not a
  second grammar.
- **Slint seams** (each carries `// slint: <callback>`): the row edit
  affordance (`task-effort-edit-opened`), the buffer mirror
  (`task-effort-edited`), commit (`task-effort-committed`), cancel —
  the pre-framework seam comments added where the gate rows must
  resolve (#782 fold).

### D-9 — #759: the drop layer is the MINT, reproduction test first

Root cause found at planning time (no search needed):
`taskboard_domain::recur.rs::assemble_rule` (line 791) stamps
`paused_until: None` in the mint body — its own doc comment says "a
re-set rule clears any pause ... recorded as backlog issue #759".
Both fronts (GUI `assemble_recur_draft` at
`presentation.rs:1328`, CLI `commands/repeat.rs:246/319`) mint
through this one function, so the fix is one line plus doc/test
updates: **preserve `head.recur.as_ref().and_then(|r| r.paused_until)`
instead of `None`** (option (a) of the issue's decision record — the
accepted UX plan US-4 made that choice).

TDD order (the dispatch law): the reproduction test
`us4_s1_rule_replace_preserves_paused_until` is written FIRST — a
fixture rule with `paused_until: Some(date)`, commit through
`assemble_recur_draft` (GUI) and the repeat verb's mint (CLI),
assert the minted rule carries the pause — watch it FAIL, then fix
the mint, then the round-trip over both fronts passes. The pinned
tests that assert today's clearing behaviour (the `assemble_rule`
tests, the `cli_repeat` p3 pin) are consciously updated as part of
the product change, each with a comment citing US-4 — never deleted
silently. AC-2 (resume not wedged): the existing resume tests stand
untouched — `PauseSeries` remains the only pause *clearer*; the
preservation only stops the silent lift.

### D-10 — US-5: the restoration guard's typed shape (#706)

Read finding: `view_state.rs::reconcile` already pops a `TaskDetail`
whose subject dies while it is the current view
(`reconcile_pops_task_detail_when_the_subject_is_deleted`), but the
**over-view restore** path (`close_settings`/`close_plan_next_day` →
`close_over_view`, truncation-exact) re-reveals a root `TaskDetail`
without a subject-liveness pass — the #706 shell.

Decision: a typed post-truncate repair, `repair_restored_root(&self,
state: &AppState) -> Vec<FlowOutcome>` (or the state parameter folded
into `close_over_view` — stage 2 picks the smaller diff), called from
every over-view restore seam (`close_settings`, `close_plan_next_day`,
and the dismissal action seam): if the restored root view is a
`TaskDetail` whose snapshot subject (`subject_task`, the D7 snapshot)
no longer resolves to a live task in `state`, the root falls back to
the subject's board's Board view (the `reconcile_seeds_the_lowest_live_board`
selection-repair pattern); when the board itself is gone, the app
root (the lowest live board). `StackDetail`'s dead-subject twin rides
the existing `reconcile_refalls_back_the_compact_selection_and_pops_stack_detail`
precedent where the truncation exposes it. The guard is inert for
live subjects (AC-2: the existing restoration tests stand) and
composes with chunk 2's focus-origin restore — the fallback lands the
focus on the restored container anchor via the landed
`push_focus_restore` funnel (the focus contract law (e)).

### D-11 — US-6: the armed-identity clear's exact seam (#840)

Root cause (issue #840, confirmed against the seam): the fast
double-click lands both activations in one pass window — the second
click dispatches `DeleteLabel` and clears `label_delete_armed`
(correct), but the label is now dead; the *next* rows pass
re-resolves the armed mirror against a roster without the label —
and the re-arm of a dead id (the issue's "second click re-arms the
now-deleted label id" within the same pass) leaves
`label_delete_armed = Some(dead_id)`: the strip never renders
(mirror index −1), the next Escape settles the disarm rung first.

Two seams, both fixed (the issue's proposed fix names both shapes):

1. **Re-arm guard** in `on_labels_overlay_delete_clicked`
   (`adapter.rs:~10533`): a stale-index/dead-label activation — the
   id no longer resolving in the live roster pass (`a.tables.label_ids`
   + `state.labels`) — returns early (the stale-index guard the
   edit seam already uses), never re-arms.
2. **Pass-side clear**: where the armed mirror is computed for the
   push (`adapter.rs:~2167`), an armed id that no longer resolves is
   cleared (`label_delete_armed = None`) — the `priority_open`
   subject-death rule's shape, so a lingering identity can never
   outlive its row even if a future seam forgets the guard.

Tests: `us6_s1` (the fast double-click path — the rung count observed
through typed flow/overlay state, never text) and `us6_s2` (the
cancel/Escape disarm path keeps the ladder). The Confirm-strip and
second-activation paths share the one seam, so one guard covers both.

## 2. Coverage mapping (W-1..W-7 → decisions)

| Row | Origin ID | Decision | Tests |
|---|---|---|---|
| W-1 | US-1 GUI stamping | D-1/D-3 | days.rs D-2 set; `us1_s1_bare_date_commits_the_window_end`, `us1_s2_explicit_time_commits_the_typed_time`, `us1_s5_quick_set_forms_follow_the_law` (journeys.rs) |
| W-2 | US-1 CLI | D-5 | app-arm `us1_s3_cli_bare_date_matches_the_gui_instant` (set_tests family) |
| W-3 | US-2 pointer repairs | D-6/D-7 | `us2_s1_time_field_click_focuses_and_stays_open`, `us2_s2_quick_options_open_on_the_current_slot` |
| W-4 | US-3 effort row | D-8 | `us3_s1_effort_commit_routes_the_sparse_mask`, `us3_s2_invalid_effort_keeps_the_buffer`, the parity test (AC-3) |
| W-5 | US-4 pause | D-9 | `us4_s1_rule_replace_preserves_paused_until` (fails first) |
| W-6 | US-5 guard | D-10 | `us5_s1_dead_subject_restores_the_board` |
| W-7 | US-6 armed clear | D-11 | `us6_s1_double_click_delete_leaves_no_armed_identity`, `us6_s2_cancel_disarm_keeps_the_ladder` |

Every origin §6 row resolves FOUND against these decisions; rows with
`—` in Commands carry their reason in the journey-steps column per
the origin's own reason law. Journey tests live in
`crates/taskboard-ui-slint/tests/journeys.rs` (the established home);
the CLI parity test in the app-arm/set_tests family; domain tests in
`days.rs`.

## 3. Implementation order & commit sequencing

Every commit standalone (builds, clippy-clean, hooks green); census
edits ride the code commits citing US/W ids (origin §9 list).

1. `feat(domain): the day-window-end stamping law`
   — `days::day_window_end` + `DayCalendar::day_end`, unit examples +
   proptests (D-1/D-2), doc contract; the purity grep gate passes.
2. `fix(ui): date-only due/scheduled entries stamp the day-window end`
   — `due_edit::parse_stamp` + its tests, the adapter funnel call
   sites (D-3), the picker date-only commit branch, journeys
   `us1_s1`/`us1_s2`/`us1_s5`; census `task.due_edit`/
   `task.scheduled_edit` evidence gains the law (W-1).
3. `fix(app): the CLI due/scheduled bare date mints the window end`
   — `DueSpec::Date`, the execution mint, `us1_s3` parity; census CLI
   cells note the shared law (W-2).
4. `fix(ui): the picker's HH:MM click keeps the editor open`
   — `us2_s1` reproduction first, then the focus-steal branch (D-6);
   `us2_s2` + the nearest-row seed (D-7); census
   `task.recur_preview`-adjacent evidence (W-3).
5. `feat(ui): the effort detail row` — the editor state, seams with
   `// slint:` comments, the sparse-mask commits, `us3_s1`/`us3_s2` +
   parity; census `task.effort_edit` TaskDetail cell minted `E (US-3)`
   closing #838 (W-4).
6. `fix(ui): re-setting a recur rule preserves the series pause`
   — `us4_s1` reproduction (fails first), the mint fix + pinned-test
   updates + doc (D-9); census `task.recur_set` evidence (#759) (W-5).
7. `fix(ui): a dead subject never restores as the detail root`
   — the typed guard, `us5_s1`; census `task.back` evidence (#706)
   (W-6).
8. `fix(ui): the labels armed identity cannot outlive its row`
   — both D-11 seams, `us6_s1`/`us6_s2`; census `label.delete`
   evidence (#840) (W-7).

## 4. Risks

- **N-5 hypothesis**: the settle-funnel reading is inferred from the
  seam code, not observed — `us2_s1` is the reproduction oracle; a
  backdrop-hit-test outcome moves the fix to the ordering layer
  (recorded deviation, not a redesign).
- **Product change**: the pause-preservation flips a pinned CLI p3
  behaviour (issue #759's own decision record) — the pinned tests are
  consciously updated with US-4 citations; the operator accepted this
  in the UX plan.
- **Echo/picker truthfulness**: a window-end stamp re-seeds the
  picker with the day-start time-of-day under a non-midnight boundary
  (non-midnight seeds the time by r4 D23) — accepted: the display
  tells the truth; the default boundary stays date-only.
- **Gate totality**: `us1_s5`'s "quick-set" mapping (the picker's
  time-list + date-only commit) is this plan's interpretation; stage
  3's walkthrough confirms the operator's N-6/AC-5 reading.
- The provenance fixture `scripts/fixtures/ux-coverage-fixture/` is
  not code — no coverage row may resolve against it.

## 5. Out-of-scope findings → backlog

- The date grammar's strptime leniency (`2030-01-3` parses as day 3;
  recorded in the `sync_directions_follow_the_pinned_rules` comment)
  — pre-existing, normalization is not this chunk: filed as a
  `backlog:` issue (number in the stage report).
- No retro-relabel pass for stored 00:00 stamps (origin R-1 — a
  backlog issue only if the walkthrough shows drift; not filed yet).
