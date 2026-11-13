# [Plan] UI R7 Chunk 2 — the recurrence-rule editor

- **Date**: 2026-11-13
- **Status**: STAGE-1 PLAN (TDD-first implementation spec for dispatch-plan
  chunk 2). No code in this commit. Stage 2 implements exactly this plan;
  deviations get recorded in stage 2's report.
- **Origin**: `.artifacts/plans/2026-11-13-gui-review-round7-plan.md`
  §2 row R7-1 + §3 D-7.1 + §3 accessibility ledger (normative), and
  `.artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md` §1/§3
  (acceptance criteria normative).
- **Branch**: `feat/ui-r7-recur-editor` (this worktree). Anchors below were
  re-verified against `main` @ `35e6b0a1`; the origin plan's
  `taskboard-state/src/command.rs` pointer has drifted — the engine's
  `UpdateTask` recur arm now lives at
  `crates/taskboard-domain/src/command.rs:1072-1076` (the apply step moved
  into the domain pipeline; read-only truth, unchanged in content).
- **Non-goals** (binding, from the origin §4 + dispatch §3): no message or
  schema change; no at-create scheduling; no pause/resume relocation (the
  recur row keeps them); no new `Tone` variants; the CLI verb's
  user-visible behaviour unchanged (bit-for-bit dispatch order and outcome
  mapping).

---

## 1. Verified seams (re-verified @ `35e6b0a1`)

| Seam | Where | What this chunk does |
|---|---|---|
| Rule grammar + mints | `crates/taskboard-domain/src/recur.rs` (`InvalidRecur` ~34, `EveryDays`/`EveryWeeks`/`WeekdaySet`/`DayOfMonth`/`NthWeek`/`OccurrenceCount`/`MonthlyAnchor`/`RecurPattern`/`RecurBound`/`RecurRule` ~123-701, `series_reference_date` ~714) | gains `assemble_rule` + the two draft-part enums (§2) |
| CLI assembly laws | `crates/taskboard-app/src/commands/repeat.rs:300-356` (`assemble_rule`), verb dispatch `repeat_on` ~210-296 | refactored onto the domain fn, verbatim laws |
| CLI behaviour pin | `crates/taskboard-app/tests/cli_repeat.rs` (p1–p8, 1012 lines) | **untouched** — they pin the refactor (§3) |
| `TaskChanges.recur` | `crates/taskboard-domain/src/messages.rs:676-680` — `Option<Option<RecurRule>>`, `Some(None)` detaches | the commit/detach payloads |
| Engine arm | `crates/taskboard-domain/src/command.rs:1072-1076` — stamps clock/author per `Some` entry; enqueue split rides the app-lane | read-only truth; no change |
| Recur row | `crates/taskboard-ui-slint/ui/views/task-view.slint:726-831` — echo + paused chip + pause/resume/convert, each `IconButton` with `label` (the hover tip), dispatch via callbacks | gains the "Edit repeat…" affordance; gains the "Add repeat…" sibling near the due/sched rows (~486-636) |
| Overlay + context precedents | `ui/compositions/date-picker.slint` (window-level overlay, no `Key.*` literals); `ContextId::TrackEntries` (`actions.rs:56-60`), `FlowTag::scope_head` (`surfaces.rs:455-473`), `presets.rs:52-63` `masks()`, `keyscope.rs` chain tests, `adapter.rs:9922-9947` `set_view_context` | `ContextId::RecurEditor` joins the same six sites |
| Buffer/commit precedent | `adapter.rs` `settle_date_editor`/`commit_date` ~8613-8679; `due_edit::parse` (`due_edit.rs:112-116`); `install_recur_seams` ~6800-6898 (the `pause_draft` one-shot + `route` shape); recur flags ~285-294 | the draft buffer + commit funnel (§4) |
| Series verb family | `actions.rs:303-318` (`SeriesPause`/`SeriesPauseCommit`/`SeriesResume`/`SeriesConvertArm`/`SeriesConvertToHabit` — pointer-grade, no preset bindings today); `router.rs:664-692` arms | gains `SeriesEdit*` (§6) |
| Keymap | `keymap/presets.rs` (`rows()` tables STANDARD/EMACS/VI; `masks()`; the totality invariant tests in the keymap module) | the `SeriesEdit` binding per preset + `RecurEditor` masked + in-context verbs unbound (registry-only) |

**Tests that must not break**: `tests/cli_repeat.rs` (the pin), the engine
recur tests/snapshots (`taskboard-state/tests/engine.rs`,
`snapshots.rs`, `proptest.rs`), the due-editor tests (`due_edit.rs`
inline + the adapter date-flow tests), the keymap totality tests, the
`surfaces.rs` `scope_head` test (§5 amends it — that is a *completion*
of the assertion to six modals, not a weakening: the new context must
appear in the same list shape, and `FlowTag::Capture`/`None` still
declare nothing).

## 2. Domain extraction — `recur::assemble_rule`

### D-E1 — module placement and shape

Both new items live in `crates/taskboard-domain/src/recur.rs`, directly
below `series_reference_date` (they consume it):

```rust
/// The caller-collapsed pattern with its bare-flag defaults still
/// open (`None` = derive from the reference date — the assembly law).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftPattern {
    Daily { every: EveryDays },
    Weekly { interval: EveryWeeks, on: Option<WeekdaySet> },
    Monthly { on: Option<MonthlyAnchor> },
}

/// The caller-collapsed bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftBound {
    None,
    Until(NaiveDate),
    Count(OccurrenceCount),
}

/// The shared rule-assembly law (CLI `tasks repeat` + the GUI editor
/// mint through the same function — one behaviour, one implementation).
/// Series identity is D-S4: an existing rule's `series`/`continued_from`
/// carry; a plain task mints a fresh `SeriesId` (v7). The anchor is
/// `series_reference_date(head, day, now)` — never raw-UTC. The
/// bare-flag defaults derive from the anchor (weekly: its weekday;
/// monthly: its day-of-month). `paused_until` is `None`: the pause is
/// the `PauseSeries` verb's field alone (see D-E3).
///
/// # Precondition
///
/// `head.is_open()` — a pattern on a done head is the *caller's*
/// typed refusal by ruling (the engine never learns of it; the CLI
/// maps to `CommandError::TaskDone`, the GUI never mounts the editor
/// for a done task and re-checks at commit).
#[must_use]
pub fn assemble_rule(
    head: &Task,
    pattern: DraftPattern,
    bound: DraftBound,
    day: crate::days::DayCalendar,
    now: chrono::DateTime<chrono::Utc>,
) -> RecurRule
```

Body: the verbatim laws from `repeat.rs:317-354` — the
`series`/`continued_from` carry, the anchor, the `Option` defaults
(`WeekdaySet::new([anchor.weekday()]).expect("one weekday is a
non-empty set")`, `DayOfMonth::new(anchor.day()).expect("a real date's
day number is 1..=31")` — the same two documented expects, moved
unchanged), `paused_until: None`. Zero new crate deps (`Task`,
`DayCalendar`, `SeriesId`, `uuid` already in scope or already
depended-on by `recur.rs` — `uuid` is a `taskboard-domain` dep; verify
`uuid::Uuid::now_v7()` is reachable — `ids.rs` mints v7 today, so it
is).

**Recorded deviation (signature)**: the origin plans specify
`-> Result<RecurRule, InvalidRecur>`. The extracted function is
**infallible**: every fallible mint (`EveryDays`, `WeekdaySet`,
`DayOfMonth`, `OccurrenceCount`, the date parse) sits at the *callers'*'
trust boundaries (CLI flag collapse; the GUI field parsers), and the two
in-body defaults are total by construction (the documented expects).
A `Result` that can never be `Err` would be a lie about the failure
modes and would force `unwrap`s at both callers. `InvalidRecur` stays
the per-field mint error type; the GUI maps it (D-D4). The behaviour
the origin wanted (typed refusal of invalid drafts) is fully preserved —
one layer out, where the untrusted text actually enters.

### D-E2 — the CLI refactor

`repeat.rs` `assemble_rule` (private fn, lines 300-356) collapses to:

```rust
let pattern = DraftPattern::from(match pattern { /* the existing
    PatternSpec match, dropping only the default-derivation arms */ });
```

i.e. `PatternSpec`/`BoundSpec` (CLI types, stay CLI-side) map onto
`DraftPattern`/`DraftBound` in the verb; the `TaskDone` guard, the
`--off` `Ok(None)` arm, and the `Some(...)` wrapping stay in
`repeat.rs`. Dispatch order in `repeat_on` is unchanged. The two
`expect` docs in `repeat_on`/`assemble_rule` doc-comments lose their
anchors (the expects moved) — reword to reference the domain fn.

### D-E3 — recorded deviation (the `paused_until` wording)

The origin D-7.1 lists "`paused_until` preserved" among the laws to
extract. The **actual CLI law** (`repeat.rs:347-354`, pinned by
`cli_repeat.rs` p3) is `paused_until: None` — a re-set rule *clears* any
pause. Behaviour preservation (dispatch hard rule: bit-for-bit) wins:
the extraction moves `None`, and the GUI commit adopts the same. The
consequence for the GUI — editing a paused series silently lifts the
pause — is recorded as backlog (see §9, issue filing); it is the
pre-existing CLI semantic, not a new decision, and changing it here
would diverge GUI and CLI, the exact drift this chunk exists to kill.

## 3. TDD order and the domain test family

TDD: the domain tests are written **first** against the new signature
(failing — `assemble_rule` does not exist), then the extraction lands,
then the CLI refactor compiles and `cli_repeat.rs` passes **unchanged**
(the pin), then the UI layers.

New `mod assemble_rule_tests` at the bottom of `recur.rs` (mirroring the
existing `next_occurrence_tests` shape, reusing its `head()`/`date()`
fixtures where they fit):

1. `a_plain_task_mints_a_fresh_v7_series_and_no_continued_from`.
2. `re_assembling_a_ruled_task_carries_series_and_continued_from`
   (the D-S4 pin — the review's must-find).
3. `the_anchor_chain_is_scheduled_then_duedate_then_window_today`
   (the three legs, one test each inside one test fn or three — follow
   the module's existing granularity).
4. `weekly_without_an_explicit_set_defaults_to_the_anchor_weekday`.
5. `monthly_without_an_explicit_anchor_defaults_to_the_anchor_day`.
6. `the_bound_and_pattern_round_trip_the_minted_parts`
   (Daily 3d / every 2w on mon,fri / monthly on 2nd-mon ×
   None/Until/Count — assert the assembled `RecurRule` structurally,
   `PartialEq` on the mints).
7. `paused_until_is_none_in_every_assembled_rule` (the D-E3 pin —
   naming the semantic explicitly so a future change is a conscious
   one).

New proptest (`proptest` already a domain dev-dep — verify; the
`taskboard-state` proptest exists, domain may not have the dep: if not,
the property lives as a loop-based deterministic test instead and the
gap is recorded): **for arbitrary minted drafts and arbitrary open
heads, `assemble_rule` reproduces `series`/`continued_from` from
`head.recur` when present and yields distinct series ids for distinct
plain heads** — the identity law is the invariant worth randomizing.

**Tests move vs stay**: nothing moves. `cli_repeat.rs` is an
end-to-end pin of the verb, not unit tests of the private fn — it stays
byte-identical and passes against the refactored verb. `repeat.rs` has
no inline unit tests of `assemble_rule` to migrate (verified: no
`#[cfg(test)]` module in the file).

## 4. The editor's draft model (adapter-owned, the due-editor precedent)

### D-D1 — flow and buffer

- New `Flow::EditRecur { task: TaskId }` in `view_state.rs` (sibling of
  `Flow::EditDate`), with the matching `FlowTag::EditRecur`, an
  `arm_flow`/`end_edit_recur` pair, and a `recur_editor()` accessor.
- The buffer is a plain struct in `view_state.rs` (the
  `DateEditorState` precedent — text buffers plus armed flags, **no
  minted types inside the buffer**; raw field text only):

```rust
pub struct RecurEditorState {
    pub task: TaskId,
    pub pattern: RecurPatternKind,       // Daily | Weekly | Monthly
    pub every_days: String,              // Daily interval text
    pub weekly_interval: String,         // Weekly interval text
    pub weekdays: [bool; 7],             // Mon..Sun toggle state
    pub monthly_mode: MonthlyMode,       // OnDay | NthWeekday
    pub monthly_day: String,             // 1..=31 text
    pub monthly_nth: String,             // 1..=5 text
    pub monthly_nth_weekday: Weekday,    // dropdown state (always valid)
    pub bound: RecurBoundKind,           // Never | Until | Count
    pub until_text: String,              // due-edit grammar text
    pub count: String,                   // 1.. text
    pub detach_armed: bool,              // the stop-repeating strip
    pub focus_steal: bool,               // the settle latch (D24 twin)
}
```

- Seeding (`adapter.rs`, the seam behind the opener): from
  `task.recur` when present — pattern kind, interval `.get()`
  stringified, weekday `contains` per slot, monthly anchor arms, bound
  arms (`until.to_string()`, `count.get().to_string()`); from the
  **anchor** for a rule-less task — weekly pre-checks
  `series_reference_date(task, day, now).weekday()`'s slot only (never
  "today" unless the anchor chain lands there — the review's explicit
  check), monthly pre-fills the anchor's day number; intervals default
  to `1`; bound defaults to `Never`.

### D-D2 — the editor composition

New `ui/compositions/recur-editor.slint`, mounted **window-level** in
`app-window.slint` (the date-picker mount pattern; visible while
`Flow::EditRecur` holds). No `Key.*` literals; every control is
walk-ordered with a `label` (the hover tip); the live echo and the
validity hints are pushed strings (the UI stays dumb):

- Pattern `SegmentedControl` (Daily/Weekly/Monthly) → `pattern-kind-changed(int)`.
- Daily: numeric `TippedField` ("every N days").
- Weekly: interval `TippedField` + seven toggle `IconButton`s
  (Mon..Sun, `Tone` flips with checked state — the weekday-toggle
  grammar is a plain check/clear pair of tones, no new widgets).
- Monthly: mode `SegmentedControl` (On day / On the Nth weekday) +
  day `TippedField` or nth `TippedField` + weekday `Dropdown` (the
  existing dropdown widget; always-valid, no hint row).
- Bound `SegmentedControl` (Never/Until/Count); Until reuses the
  `DueEditor`'s typed-grammar field conventions (`YYYY-MM-DD` or
  `+Nd`, the `due_edit::parse` grammar — see D-D4); Count is a
  numeric `TippedField`.
- Live echo: `RoleText` caption with the assembled `rule.text_form()`
  when the whole draft mints; otherwise the typed invalid-field hint.
- Footer: commit `IconButton` (check, `Tone.success`, enabled only
  when the draft mints), cancel `IconButton` (remove), and the
  **detach strip**: disarmed shows "Stop repeating…" (`IconButton`);
  armed shows the consequence caption ("The series stops repeating;
  the task stays as an ordinary task.") + confirm/cancel pair (the
  recur row's armed-convert precedent, `task-view.slint:806-831`).

Callbacks up through `task-view.slint` (nothing — the editor is
window-level) → `app-window.slint` → `adapter.rs` `install_recur_editor_seams`.

### D-D3 — per-field parse/validity mapping (the trust boundary)

Each field's `edited(text)` seam parses immediately and stores the
validity beside the raw text (computed, not cached minted values —
re-mint at echo/commit from the buffer so the buffer stays the single
truth):

| Field | Parser | `InvalidRecur` mapping | Empty-text rule |
|---|---|---|---|
| every-days | `EveryDays::parse_text_form` | verbatim (`BadNumber`/`ZeroInterval`) | invalid while Daily |
| weekly-interval | `EveryWeeks::parse_text_form` | verbatim | invalid while Weekly |
| weekday toggles | `WeekdaySet::new` over the checked slots | `EmptyWeekdaySet` | n/a (toggles) |
| monthly-day | `DayOfMonth::parse_text_form` | verbatim | invalid while OnDay |
| monthly-nth | `NthWeek::parse_text_form` | verbatim | invalid while NthWeekday |
| until | `due_edit::parse(text, now, &Local)` → `Ok(Some(instant))` → `day.calendar_date(instant)`; `Ok(None)`/`Err` = invalid | a draft-local `DraftInvalid::BadDate` (the date grammar's errors are `DueParseError`, not `InvalidRecur` — the union enum below) | invalid while Until |
| count | `OccurrenceCount::parse_text_form` | verbatim | invalid while Count |

Presentation-side union for the hint row:

```rust
enum DraftField { EveryDays, WeeklyInterval, Weekdays, MonthlyDay,
                  MonthlyNth, Until, Count }
enum DraftInvalid { Recur(InvalidRecur), BadDate }
// presentation::recur_field_validity(&RecurEditorState, ...)
//   -> Result<(), Option<(DraftField, DraftInvalid)>>
```

`recur-editor.slint` renders whatever the adapter pushes into
`field-validity`/`hint` slots (`FieldValidity`, the `DueEditor`'s
widget). **No text assertions**: tests assert the `(DraftField,
DraftInvalid)` pair; the hint *strings* live in one presentation
function and are never matched in tests.

### D-D4 — recorded decision (the Until parser)

The origin says Until "reuses the due-editor's date parser
(`due_edit::parse`, `YYYY-MM-DD` or `+Nd`)". `due_edit::parse` yields a
`DateTime<Utc>` instant while `RecurBound::Until` carries a
`NaiveDate`. Decision: parse through `due_edit::parse` (grammar parity
with the date fields the user already knows, `+Nd` included) and
convert the resolved instant with `DayCalendar::calendar_date` — the
same never-raw-UTC discipline `series_reference_date` codifies. The
echo shows the resulting bound via `RecurBound::text_form()`, so the
user sees the resolved date, not their shorthand.

## 5. `ContextId::RecurEditor` — the six registration sites

The `TrackEntries` inventory, all six touched:

1. `actions.rs` — the `ContextId::RecurEditor` variant + `name() =>
   "recur-editor"`, doc-commented as the total-mask modal (the
   `TrackEntries` wording).
2. `surfaces.rs` — `FlowTag::EditRecur` added; `scope_head()` returns
   `&[ContextId::RecurEditor, ContextId::ViInsert, ContextId::Global]`
   (the fifth/sixth masking modal). The `scope_head_declares_the_five…`
   test is **completed** to six (renamed `…_the_masking_modals…`): the
   new head asserted in the same shape, `Capture`/`None` still `&[]`.
3. `keymap/presets.rs` — `masks()` gains `ContextId::RecurEditor`
   (every preset shares the one mask list — the modal must mask in all
   three).
4. `keyscope.rs` — the flow-armed chain derives from `Flow::tag` →
   `scope_head`, so no table edit lands here; the chain-shape tests
   gain one `Flow::EditRecur` row asserting
   `[RecurEditor, ViInsert, Global]` (the T-S1 checklist's sibling).
5. `adapter.rs` `set_view_context` — `RecurEditor` joins the
   non-routable match arm (modal contexts map to no view).
6. `adapter.rs`'s other `ContextId` exhaustive matches (grep
   `ContextId::TrackEntries` — `adapter.rs:9936` and any derived
   matches) gain the variant; the compiler's exhaustiveness is the
   checklist.

Mask semantics: while `Flow::EditRecur` holds, the walk stops at
`RecurEditor` (total mask, like `TrackEntries`); the form's text fields
ride `ViInsert` masking; commit/cancel/detach are registry rows in the
`RecurEditor` context (pointer-first, like the picker's actions — no
preset keybindings, so the totality tests are untouched by them); the
global Escape law resolves `ModeNormal` from `ViInsert` and closes the
flow (the date-editor clean close — cancel discards, nothing
dispatches).

## 6. The commit funnel and the detach strip

### D-C1 — one-shot commit draft (the `pause_draft` shape)

The commit seam (the check button / Return-on-commit) re-mints the
whole buffer through the per-field parsers; any invalid field is a
silent no-op (the button is disabled anyway — the seam re-checks, the
trust boundary never trusts the widget). On success the minted
`RecurRule` lands in the adapter's one-shot:

```rust
recur_edit_target: Option<TaskId>,      // armed by both affordances
recur_draft_rule: Option<RecurRule>,    // consumed by SeriesEditCommit
```

Router arm:

```rust
ActionId::SeriesEditCommit => match (ctx.recur_edit_target,
                                    ctx.recur_draft_rule.clone()) {
    (Some(id), Some(rule)) => DispatchOutcome::Command(StateCommand::UpdateTask {
        id, changes: TaskChanges { recur: Some(Some(rule)),
                                   ..TaskChanges::default() },
    }),
    _ => DispatchOutcome::NoTarget,
},
ActionId::SeriesEditDetach => match ctx.recur_edit_target {
    Some(id) => DispatchOutcome::Command(StateCommand::UpdateTask {
        id, changes: TaskChanges { recur: Some(None),
                                   ..TaskChanges::default() },
    }),
    None => DispatchOutcome::NoTarget,
},
```

After either dispatch the adapter ends `Flow::EditRecur`, clears both
one-shots and the buffer (the buffers-never-outlive-the-flow rule), and
the standard refresh renders the recur row from post-state truth. The
engine stamps clock/author and enqueues the sidecar lane exactly as the
CLI's identical command does today — nothing downstream changes.

Detach arming: `SeriesEditDetachArm` / `SeriesEditDetachCancel` are
`UiEffect` view intents flipping `detach_armed` (the
`SeriesConvertArm` pattern); only the confirmed dispatch sends the
command — and `Some(None)` **and nothing else** in the changeset (the
review's must-check).

### D-C2 — recorded decision (commit without an armed flow is a NoOp)

`SeriesEditCommit`/`SeriesEditDetach` with `None` one-shots (a stale
arm after the subject changed under the editor) degrade to
`DispatchOutcome::NoTarget` — the stale-index law the recur seams
already codify (`install_recur_seams`' `stale index degrades to a
no-op`).

## 7. The affordances, `SeriesEdit`, and free keys

### D-A1 — mount sites

- **Add repeat…**: in the task detail facts region, under the
  due/sched row block (`task-view.slint` ~486-636) — one
  `IconButton` (`IconKind.timer` is the recur row's class; reuse it
  for family identity), `label: "Add repeat: the task joins a
  recurring series"`, mounted `if (!root.task.recur-visible &&
  root.task.series-edit-visible)`. Not mounted for done tasks (the
  assembly precondition made visible — `series-edit-visible` is
  model-side and already false for done subjects; stage 2 asserts
  that in the presentation test).
- **Edit repeat…**: in the recur row's echo layout
  (`task-view.slint:736-746`), beside the pattern caption —
  `label: "Edit repeat: change the pattern or bound"`, mounted
  `if (root.task.series-edit-visible)`.
- Both converge on one callback `series-edit-asked()` → the seam arms
  `recur_edit_target` from the **task row table's own subject** (the
  what-you-click-is-what-you-increment law) and routes
  `ActionId::SeriesEdit`; the router arm is the `SeriesPause` twin:
  `UiEffect(SeriesEditorOpen)` opening `Flow::EditRecur` over the
  armed target. **Kiosk**: `series-edit-visible` is computed
  presentation-side under the same editing gate as the other
  TaskDetail action affordances (the model-side gate empties the
  affordance, never the row data — D-T16).

### D-A2 — `ActionId::SeriesEdit` and the in-context verbs

- `ActionId::SeriesEdit` (`"series.edit"`), registry row in
  `ContextId::TaskDetail` alongside the series family.
- `SeriesEditCommit` / `SeriesEditDetach` / `SeriesEditDetachArm` /
  `SeriesEditDetachCancel` (`"series.edit-commit"` etc.), registry
  rows in `ContextId::RecurEditor` — pointer-grade, **no preset
  bindings** (the picker-actions and habit-verbs rule; the totality
  tests bind nothing for them).
- `UiEffect::SeriesEditorOpen` joins the effect enum; the adapter's
  effect fold opens the flow.

### D-A3 — free keys per preset (recorded decision, verify-at-implementation)

`SeriesEdit` gets one binding in **each** preset's `TaskDetail` rows
(the origin demands key parity; the sibling series verbs are
pointer-only, but this one opens an editor — it is a
`TaskTitleEdit`-class verb, not a row action):

- Standard: `Ctrl+r` (mnemonic *repeat*; free in the STANDARD
  `TaskDetail` rows — stage 2 verifies against the table and the
  totality test before relying on it).
- Vi: `r` in `ContextId::TaskDetail` (vi-normal's repeat mnemonic;
  free there — verify; if taken, `R`).
- Emacs: `C-c r` (the emacs `C-c <letter>` user-namespace; free —
  verify).

Stage 2's first keymap commit runs the preset totality tests; any
collision is resolved by re-picking **in this plan's spirit** (a
repeat-mnemonic chord, recorded in stage 2's report) — never by
removing the binding from a preset.

## 8. Test catalogue (TDD order)

| # | Layer | Test | Pins |
|---|---|---|---|
| T1 | domain | `assemble_rule_tests` (§3's 7 + the property) | the laws, D-S4, D-E3 |
| T2 | app | `tests/cli_repeat.rs` — **unchanged**, green after the refactor | bit-for-bit CLI |
| T3 | ui presentation | `recur_field_validity`: the full field×invalid matrix → `(DraftField, DraftInvalid)` pairs; the seeding fn (rule-seeded vs anchor-seeded; the weekly pre-check from the **anchor**) | D-D3, D-D1 |
| T4 | ui presentation | `series-edit-visible`: open task true, done task false, kiosk-gated like its siblings; `recur-visible` ⇒ the Edit affordance's flag | D-A1 |
| T5 | ui adapter | seam test asserting the **exact** `UpdateTask` command with the assembled rule on commit (the `banner_actions_send_exact_commands` shape); the detach test asserting `recur: Some(None)` **and nothing else**; the stale-target NoOp; the D-S4 edit-path pin (edit an existing rule → the dispatched rule carries the task's `series`/`continued_from`/`paused_until: None`) | §4/§6, the review's must-checks |
| T6 | ui view_state | `Flow::EditRecur` arm/end/focus-steal latch; the buffer drops with the flow | D-D1 |
| T7 | ui keymap/surfaces/keyscope | `scope_head` six-modals test; the `EditRecur` chain-shape row; preset totality green with the three `SeriesEdit` bindings; masks contain `RecurEditor` per preset | §5, D-A3 |
| T8 | engine | none — untouched (the arm is read-only truth; the existing recur snapshots cover it) | — |

No sleeps anywhere; no text assertions (echo/hint strings are
pushed, never matched); deterministic by construction (the seeding
fn takes `now`/`DayCalendar` as parameters — the injected-clock seam).

## 9. Backlog issues filed by this chunk

Filed at planning time (see the final report for numbers):

- **`paused_until` semantics on rule re-set**: the CLI (and now the
  GUI, by shared law) silently clears a pause when a rule is re-set.
  Pre-existing, arguably surprising for the GUI user who sees a paused
  chip and edits the pattern. Recorded so a conscious product decision
  can follow; the extraction preserves today's behaviour either way.

## 10. File inventory and estimated size

| File | Change |
|---|---|
| `crates/taskboard-domain/src/recur.rs` | +`DraftPattern`/`DraftBound`/`assemble_rule` + tests (~+220) |
| `crates/taskboard-app/src/commands/repeat.rs` | refactor onto the domain fn (~±40, net −20) |
| `crates/taskboard-ui-slint/src/view_state.rs` | `Flow::EditRecur`, `FlowTag::EditRecur`, `RecurEditorState`, arm/end/accessors (~+120) |
| `crates/taskboard-ui-slint/src/presentation.rs` | seeding, `recur_field_validity`, `series-edit-visible`, echo assembly (~+120) |
| `crates/taskboard-ui-slint/src/adapter.rs` | `install_recur_editor_seams`, flags/one-shots, effect fold, context-match arms (~+200) |
| `crates/taskboard-ui-slint/src/actions.rs` | `ContextId::RecurEditor` + 5 action ids + registry rows (~+60) |
| `crates/taskboard-ui-slint/src/router.rs` | 3 dispatch arms + NoOp laws (~+40) |
| `crates/taskboard-ui-slint/src/surfaces.rs` / `keyscope.rs` / `keymap/presets.rs` | the six-site registration + tests (~+60) |
| `crates/taskboard-ui-slint/ui/compositions/recur-editor.slint` | new (~+300) |
| `crates/taskboard-ui-slint/ui/views/task-view.slint` + `ui/app-window.slint` | the two affordances + mount/forwarding (~+60) |
| adapter/presentation/keymap test modules | T3–T7 (~+280) |

**Estimated total: ~1350–1550 lines**, of which ~450 are tests.
Largest single risk mass: the `ContextId` exhaustiveness ripples
(compiler-driven, mechanical) and the seeding/validity matrix (T3).

## 11. Risks

1. **`cli_repeat.rs` is the pin, not a suggestion** — any behavioural
   delta in the CLI refactor (dispatch order, `TaskDone` mapping,
   receipt projection) is a stage-3 finding. Mitigated by TDD order:
   the refactor lands while T2 is the only red/green oracle.
2. **`paused_until` origin/code discrepancy (D-E3)** — resolved
   verbatim; stage 3 must not "fix" the assembly to preserve a pause
   (that is the filed backlog issue's decision, not this chunk's).
3. **`ContextId` exhaustiveness** touches derived matches across the
   keymap machinery; all six sites listed in §5, the compiler finds
   any stragglers.
4. **Key collisions in presets** (D-A3) — verify-at-implementation
   with recorded alternates; never drop a preset's binding.
5. **Anchor drift**: file/line anchors here were read @ `35e6b0a1`;
   chunk 1 and 3 run in parallel and could land first — stage 2
   re-verifies anchors after its rebase.
