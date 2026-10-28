# [Plan] Series Surfaces — the `tasks repeat` CLI & the Recurring Badges (DR-6/4)

- **Date**: 2026-10-28
- **Kind**: stage-1 implementation plan (chunk DR-6/4 of the
  design-rounds dispatch). TDD-first; no production code in this
  commit. Normative inputs: the accepted design round
  `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  (§3.7 bounds & lifecycle — the verbs; §4.6's CLI conventions — the
  two-lane `with_core` recipe; §7 chunk 4 — the chunk row) and the
  dispatch plan
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (lanes, wave protocol, code-quality mandate) and §10.3 (this
  chunk's prompt and acceptance). DR-6 binding notes (dispatch §3.5):
  rulings D-C1–D-C13; `days.rs` is the only local-date vocabulary; no
  reminder changes; no pause/resume; A1 discipline for new numeric
  types.
- **Preconditions verified** (2026-10-28, `origin/main` @ `d956ca32`):
  DR-6/2 merged (`97a6ae54` feat: the completion spawn and the
  day-boundary roll, `6cfab03a` + `a802a79a` engine tests/insta corpus —
  the brief's #499; spawn/roll semantics exist to display) and DR-6/1
  merged (`7273f9a5`/`2688a752`/`1fa9c140` — the `RecurRule` grammar,
  arithmetic and `Task.recur` field family). Both are ancestors of the
  branch base (checked with `git merge-base --is-ancestor`). **Not** a
  shape-widener chunk: no new serialized state, no migration, no ADR —
  surfaces only, over shapes chunks 1–2 landed.
- **Branch / worktree**: `feat/series-surfaces` in
  `../taskboard-WT-feat-series-surfaces` (off the fetched `origin/main`
  @ `d956ca32` — local `main` was stale at `7355fcb3`; worktree-doctor
  passed: 245G free vs 25G floor). Plan commit is the branch's first
  commit; implementation commits follow it, each standalone.
- **Status**: READY FOR STAGE 2 (implementation agent). Deviations from
  this plan need a recorded reason per dispatch §1.

---

## 1. Verified anchors (branch base `d956ca32`, read 2026-10-28)

Numbered facts; the design sections below cite them. Line numbers are
the branch base's.

1. **The grammar exists, typed** (`crates/taskboard-domain/src/recur.rs`):
   `RecurRule { series, anchor, pattern, bound, continued_from }`;
   `RecurPattern::{Daily{every}, Weekly{interval, on}, Monthly{on}}`;
   `MonthlyAnchor::{OnDay(DayOfMonth), NthWeekday(NthWeek, Weekday)}`;
   `RecurBound::{None, Until(NaiveDate), Count(OccurrenceCount)}`. The
   A1 mints are the only construction paths: `EveryDays::new(u16)`,
   `EveryWeeks::new(u16)`, `WeekdaySet::new(iter)` (non-empty, dedup),
   `DayOfMonth::new(u32)` (1..=31), `NthWeek::new(u32)` (1..=5),
   `OccurrenceCount::new(u32)` (≥1) — all `Result<Self, InvalidRecur>`,
   whose variants are `ZeroInterval | EmptyWeekdaySet |
   DayOutOfRange{got} | WeekOutOfRange{got}` (recur.rs:33).
   Arithmetic: `next_after(pattern, anchor, from) -> NaiveDate`
   (strictly-after member), `occurs_on(pattern, anchor, d)`,
   `roll_target(pattern, anchor, stale, today)` (catch-up member in
   `(stale, today]`, else first member `> stale`; **bound checks are
   the caller's** — its doc says so), `schedule(rule, from, to)`
   (filters `Until`, not `Count`). **No text forms exist** — chunk 1's
   D12 explicitly deferred `parse_text_form`/`text_form` for the mints
   to this chunk ("CLI text forms are chunk 4", recur.rs, the `Effort`
   precedent).
2. **The reference chain exists but is private, and lives in the
   planner** (`crates/taskboard-domain/src/command.rs:401` and `:412`):
   `fn series_reference_date(head, day, now) -> NaiveDate` (calendar
   date of `scheduled`, else of `duedate`, else the window-date of
   `now`) and `fn roll_reference_date(task, day) -> Option<NaiveDate>`
   (the same chain minus the dateless fallback — `None` = never
   stale). Both are D-E5's ruled conventions. Two consumers outside
   `command.rs` need them now (§3, §5); re-homing is §3's refactor
   commit.
3. **`UpdateTask` is the whole write side** (`command.rs:819`): the arm
   applies `changes.recur` (`Some(Some(rule))` = set-and-stamp,
   `Some(None)` = clear-and-stamp, recur.rs field family from chunk 1)
   with a fresh `clocks.recur`, routes the appdata op via the
   `app_edit` predicate (`command.rs:879`), and rejects only
   unknown/deleted tasks plus the empty changeset (`changes ==
   TaskChanges::default()` → `no_op()`). There is **no value-unchanged
   skip** — a repeated identical edit re-stamps and re-enqueues, the
   house's benign same-value LWW posture (origin §3.4). No new
   `StateCommand` may exist for repeat/off (dispatch §10.3 review
   criterion: a new arm duplicates the field-family surface —
   blocking).
4. **The engine already owns spawn/roll**: the `SetTaskDone` arm
   spawns the successor (`command.rs:893` block, D-E1–E7) and
   `plan_day_rolls` (`command.rs:561`) rolls stale heads. Surfaces
   only read. The spawn leaves the done head carrying `recur`
   (copied verbatim to the successor; the head's rule clears only at
   the bound-exhausted terminal transition, D-E2) — so **done heads
   with rules are routine state**, and badge/guard decisions must
   respect it (§5 D-S7, §4 D-S6).
5. **The CLI conventions** (`crates/taskboard-app/src/cli.rs`): clap
   types only, dispatch lives in `commands::` (module doc). Value
   parsers are free functions returning `Result<T, String>` (a clap
   usage error, exit 2): `parse_board_id`, `parse_color`, `parse_due`,
   `parse_effort` (over `Effort::parse_text_form`), `parse_importance`
   /`parse_urgency` (vocabulary text forms), `parse_insistence_flag`,
   `parse_before_flag` — each with an inline accept/reject unit table.
   The `tasks remind` variant (`cli.rs:797`) is the mode-family
   precedent: one `Option<String>` positional + mode flags with
   `conflicts_with_all`/`requires` and a variant-level
   `#[group(required = true, multiple = true)]`; `reminders::collapse`
   folds the flags into a `RemindInvocation` enum (reminders.rs:483) —
   one definition site shared by both dispatch lanes.
6. **The two-lane recipe** (`crates/taskboard-app/src/commands/`): the
   one-shot lane boots via `tasks::with_core` (tasks.rs:43) into
   `*_on` seams; the session lane (`dispatch_line`, mod.rs:1419)
   routes `TasksCmd` arms to the same `*_on` seams over the booted
   core — no boot, no shutdown. `route_for_ipc` (mod.rs:1868) matches
   `Command::Tasks { .. }` **wholesale** as `IpcOrLocal` — a new
   `TasksCmd::Repeat` inherits the lane with no routing-table edit
   (the exhaustive match is compiler-forced; the lane unit test
   mod.rs:2049 wants a Repeat row). The receipts are typed serde
   structs in `commands/output.rs` (`ReminderAddReceipt` shape: ids +
   rendered-source fields + a fact boolean); errors are the exhaustive
   `CommandError` with `error_kind()`/`error_detail()` (mod.rs:122,
   :410). The app-layer guard precedent is the anchor-title rule
   (`edit_on`, tasks.rs:505: "the guard lives app-layer by ruling —
   the engine never learns of it").
7. **The hermetic CLI suite recipe**
   (`crates/taskboard-app/tests/cli_reminders.rs`): standalone
   tempdir configs (`standalone_config` + `forbidden_store`), seeding
   through one-shot verbs, typed receipts asserted as values, error
   kinds via `error_kind()` **never message text**, clap grammar
   pinned via `Cli::try_parse_from`, and the session lane driven
   end-to-end through `dispatch_line` with a `BasicSession` (r10).
   The repeat family gets its own `cli_repeat.rs` in this shape.
8. **The day calendar is reachable from both surfaces**: the app side
   has `AppConfig::day_calendar()` (config.rs:1254) — the validated
   `[preferences] day_start_time` plus the device's fixed UTC offset
   resolved once at boot, returned as `taskboard_state::DayRollConfig`
   whose `.calendar` is the domain `DayCalendar`. The UI side holds
   the identical pair as `UiSettings.day_start: (NaiveTime,
   FixedOffset)` (ui-slint lib.rs:95), from which the adapter builds
   `DayCtx` (adapter.rs:509 `day_ctx`). `DayCalendar` itself
   (days.rs:93) carries `today(now)` (the window-date — the night-owl
   `03:00 is still yesterday` rule), `calendar_date(t)` (the plain
   offset date), `instant(date, tod)`.
9. **The presentation catalogue conventions**
   (`crates/taskboard-ui-slint/src/presentation.rs`): pure
   display-domain mapping over domain types — no slint types, no IO
   (module doc); typed decision structs + bucket enums ("string
   assembly is trivial glue over the bucket — tests assert buckets,
   not text", `duration_bucket`/`due_phrase`); string functions
   asserted non-empty and deterministic, never on their copy
   (`resurrect_notice_phrase`); domain truth never reinvented
   (`due_kind` reads the domain's own sets). The DR-5/4 badge
   precedent (D-T16/D-T17, presentation.rs:476–533): a typed parts
   struct (`TrackingChip`), one shared derivation for chip and badge,
   a visibility function, and the kiosk posture — "the gate empties
   the ACTION, never the display".
10. **The card decoration slot is the badge ride**
    (`ui/widgets/task-card.slint:14`): `DecorationKind::{importance,
    urgency, habit, reminder, tracking}` — the rule-D vocabulary with
    `habit` already reserved (chunk 6 will claim it; the series badge
    gets its own kind, §5). `TaskDecoration { kind, label, tone }`
    with `Tone::{neutral, info, success, warning, danger, overdue,
    …}`. The today view (`ui/views/today-view.slint`) renders
    non-compact cards at every breakpoint (`compact` is a
    board-layout-only breakpoint) and `push_today`
    (models.rs:1396) feeds `decorations_model` →
    `field_decorations(task, tracked_task)` (models.rs:1342) —
    importance/urgency from domain text forms, tracking from the
    shared pick — unconditionally of kiosk. **Consequence: a
    decoration-slot badge is visible on kiosk passively** (the
    dispatch's "kiosk reads the badge passively" — no gate needed;
    the kiosk gate precedent empties actions, never displays). The
    decoration labels are diff-stable by design (the tracking badge
    doc: no moving number, so re-pushes don't churn diffs) — the
    recur badge must hold the same discipline (§5: day-granularity
    phrase, never wall-clock deltas).
11. **Test-side id minting precedent**: the app layer mints uuids
    where a payload needs one (`ui.rs:489`:
    `StackId::from_uuid(uuid::Uuid::now_v7())`); the ids' doc says
    "UUIDv7, minted client-side". The `repeat` verb's `SeriesId`
    mints the same way, app-side (§3 D-S9).

## 2. The `tasks repeat` grammar (origin §3.7's verbs, D-C5)

### 2.1 The clap surface

One new variant on `TasksCmd`, mirroring `Remind`'s mode-family shape:

```text
taskboard tasks repeat <task> --daily [N]      [--every-days is --daily's value]
                               --weekly [DAYS] [--every-weeks N]
                               --monthly [ANCHOR]
                               --until DATE | --count N        (bound, optional)
                               --off
```

- `<task>` — the standard task selector positional (`uuid | unique
  ≥8-char uuid prefix | exact title`), **required** (repeat has no
  list mode; unlike `Remind` there is no flag-less form, so the
  positional is a plain `String`, not an `Option`).
- `--daily [N]` — every N days; bare flag = 1 (`num_args(0..=1)`,
  `default_missing_value = "1"`). Value over `EveryDays::parse_text_form`.
- `--weekly [DAYS]` — on the given weekdays of every `--every-weeks`-th
  week; bare flag = the task's own weekday (resolved handler-side,
  D-S2). Value: comma-separated three-letter lowercase days
  (`mon,wed,fri`), set semantics (duplicates dedup, order
  canonicalized `Mon..Sun`), over `WeekdaySet::parse_text_form`.
- `--every-weeks N` — the weekly interval (≥1, `EveryWeeks`); requires
  `--weekly`, conflicts with the other patterns.
- `--monthly [ANCHOR]` — bare flag = the task's own day-of-month
  (`OnDay`, handler-side, D-S2). Value: `15` (`OnDay(15)`) or
  `<k>th-<day>` (`2nd-mon` = the second Monday; `1st`–`5th` only —
  `15th-mon` is a typed `WeekOutOfRange`, not a fifth meaning). A
  month lacking the k-th weekday contributes no member (chunk 1's
  D5 ruling — legal grammar, legal rule, documented at the flag).
- `--until DATE` — inclusive local bound, `YYYY-MM-DD`
  (`NaiveDate::parse_from_str`, the `parse_due` bare-date reading
  without the UTC midnight semantics: a bound is a pure local date).
  Conflicts with `--count`; **requires a pattern flag**.
- `--count N` — at most N completed occurrences (`OccurrenceCount`, ≥1;
  `0` is a usage error). Conflicts with `--until`; **requires a
  pattern flag**.
- `--off` — stop repeating (clear `recur`). Conflicts with all seven
  set-flags.

**Coherence is clap-enforced, never stringly-matched**: exactly one
pattern flag (an ArgGroup over `daily|weekly|monthly`, `multiple =
false`), bounds require the pattern group, `--off` exclusive. The
mechanism (variant-level group + a named ArgGroup + `requires`/`conflicts_with_all`,
the `Remind` precedent) is a **stage-2 first-hour verify item** (§8
R1) — clap 4 group-on-group `requires` inside a subcommand variant has
two known-good fallbacks, both still typed: flatten a
`#[derive(Args)]` struct carrying the group, or validate the coherence
handler-side as a `CommandError`.

### 2.2 The typed error taxonomy

Two layers, both typed at their own boundary:

- **The domain layer** (new, §3): `parse_text_form` on the mints and
  `MonthlyAnchor` returns `Result<Self, InvalidRecur>` — the grammar
  errors are the existing `InvalidRecur` variants plus one new
  `BadWeekday { got: String }` (the only input class with no variant;
  `DayOutOfRange{got}` is the carrying precedent). Structural tests
  assert `matches!(err, InvalidRecur::…)`.
- **The CLI layer**: each value parser wraps exactly one domain
  `parse_text_form` and renders its `Display` into the clap usage
  message (exit 2) — the `parse_effort` shape. No parser
  reimplements a mint's predicate; the taxonomy stays single-sourced.
  A `--until` that is not a calendar date is the `parse_due` failure
  class (string usage error at the edge; the domain never parses
  strings into `NaiveDate`).

### 2.3 The invocation collapse (the `reminders::collapse` shape)

```rust
pub enum RepeatInvocation {
    /// `--off <task>`: clear the rule; the head stays an ordinary task.
    Off { task: String },
    /// A pattern (exactly one, clap-enforced) plus an optional bound.
    Set {
        task: String,
        pattern: PatternSpec,
        bound: BoundSpec,
    },
}

pub enum PatternSpec {
    Daily(EveryDays),
    /// `on: None` = the task's own weekday (handler-side default).
    Weekly { interval: EveryWeeks, on: Option<WeekdaySet> },
    /// `on: None` = the task's own day-of-month (handler-side default).
    Monthly { on: Option<MonthlyAnchor> },
}

pub enum BoundSpec { None, Until(NaiveDate), Count(OccurrenceCount) }
```

`repeat::collapse(cmd: TasksCmd) -> Option<RepeatInvocation>` is the
one fold both lanes share (`unreachable!` in the arms selects it, the
remind pattern). The bare-flag defaults stay `Option` through the
collapse — they need the resolved task and the calendar, which exist
only handler-side (§3.2).

## 3. The anchor derivation (origin §3.7: "anchor = the task's
   scheduled/duedate local date, else today") and the one next-
   occurrence law

### 3.1 The re-home (refactor commit)

`series_reference_date` and `roll_reference_date` (fact 2) move from
`command.rs` to `recur.rs` and gain `pub` — they are series
vocabulary (D-E5's date conventions), not planner-internal detail,
and three consumers now need them: the CLI verb (§3.2), the engine's
existing call sites (unchanged, re-imported), and the domain's next-
occurrence law (§3.3). Pure move + visibility; zero behavior delta;
the existing spawn/roll engine suite is the pin. (`command.rs` keeps
`count_done_in_series`/`bound_admits` — engine-only, Count
evaluation stays history-side.)

### 3.2 The verb's anchor rule

For `Set`, handler-side, after `resolve_task`:

1. `day = app.config.day_calendar().calendar` (fact 8; the booted
   config's validated pair — never `chrono::Local` in the verb; the
   one `chrono::Local` grant stays `parse_at_flag`'s).
2. `anchor = series_reference_date(task, day, now)` — the ruled
   chain verbatim: scheduled calendar date → duedate calendar date →
   window-date of now. **No raw-UTC date is ever treated as local**
   (the review prompt's timezone bug class — the chain goes through
   `DayCalendar::calendar_date`/`today` or it does not exist).
3. `series`/`continued_from`: the task already carries a rule ⇒ keep
   **both** (D-S4 — the user is re-scheduling a series, not founding
   a new one; a minted id would reset `Count` history and orphan the
   done rows' attribution). Plain task ⇒ `SeriesId::from_uuid(Uuid::now_v7())`
   (fact 11) and `continued_from: None`.
4. `pattern`: bare-flag defaults resolve against the reference date —
   `Weekly { on: None }` ⇒ `WeekdaySet::new([reference.weekday()])`;
   `Monthly { on: None }` ⇒ `DayOfMonth::new(reference.day())` into
   `OnDay` (`DayOfMonth::new` cannot fail on a real date's day
   number — 1..=31 by construction; the `expect` carries that
   invariant comment).
5. `bound` maps from `BoundSpec` by identity.

The assembled `RecurRule` rides `TaskChanges { recur:
Some(Some(rule)), ..Default::default() }` through
`StateCommand::UpdateTask` — nothing else (fact 3). The verb builds
the changeset itself; it does **not** route through
`tasks::edit_on` (whose anchor-title guard is orthogonal) but mirrors
its resolve→execute→re-read steps (the `label_on` shared-body
precedent argues for a `repeat_on` that takes the invocation).

`--off` maps to `TaskChanges { recur: Some(None), .. }` — an
unconditional clear, no pre-check (fact 3: the engine has no
unchanged-skip; the same-value LWW write is the house's accepted
posture, and post-state truth is what the receipt reports).

### 3.3 The one next-occurrence law (new domain function)

Both surfaces need "what does this series advertise next" — the app
receipt (§4) and the badge (§5). The app crate must not depend on
ui-slint, so the law lives in the domain, composed from existing
parts (no new arithmetic — `next_after`/`roll_target`/`roll_reference_date`
plus the engine's own stale comparison):

```rust
/// What a head advertises as its next occurrence (surfaces, DR-6/4):
/// a stale head (open, dated, reference strictly before today's
/// window-date — the roll predicate's read side) advertises its
/// `roll_target`; a fresh head advertises `next_after` from its
/// reference date; a dateless head never is stale and projects from
/// the window-date. `date: None` = the Until bound excludes the next
/// member — this head is the series' last (the badge renders the
/// pattern; the engine's own spawn/roll will enforce the bound).
pub struct NextOccurrence {
    pub date: Option<NaiveDate>,
    pub stale: bool,
}
pub fn next_occurrence(rule: &RecurRule, head: &Task,
                       day: DayCalendar, now: DateTime<Utc>) -> NextOccurrence;
```

Gated on the caller (surfaces check `is_open()` themselves — the done
head's badge decision is a display policy, §5). `Count` is **not**
evaluated here: it is engine-evaluated from completion history (chunk
1's D6), and the badge shows schedule truth, not quota state (D-S7).
Pinned by unit tables + a proptest: for generated rules and head
shapes, `stale ⇒ date == roll_target(…)` (and `date > stale`),
`¬stale ∧ date.is_some() ⇒ date == next_after(…)` ∧ `Until` never
exceeded.

## 4. The verb family (`commands/repeat.rs`, the two-lane recipe)

New module `repeat.rs` (one module per subcommand family — the
`reminders.rs` naming convention), plus receipts in `output.rs`:

```rust
/// The `tasks repeat` receipt: `rule: None` names the cleared
/// post-state (`--off`, or a repeat that found nothing to clear) —
/// the `ReminderMuteReceipt` post-state-truth precedent.
pub struct RepeatReceipt {
    pub task_id: String,
    pub task_title: String,
    pub rule: Option<RecurRuleReceipt>,
}
pub struct RecurRuleReceipt {
    pub series: String,        // uuid
    pub anchor: NaiveDate,     // serde: ISO date
    pub pattern: String,       // canonical text form (§3, D-S8)
    pub bound: String,         // "" | "until YYYY-MM-DD" | "count N"
    pub next: Option<NaiveDate>, // next_occurrence(rule, task, …).date
}
```

Handlers (every one `#[must_use]`-shaped, `# Errors` doc, typed
`CommandError` only):

- `repeat_on(app, invocation)` — resolves the task
  (`tasks::resolve_task`), reads the calendar from the booted config
  (§3.2), and per arm:
  - `Off` → the clear changeset; **no done-guard** (clearing a dead
    rule from a completed head is honest cleanup, D-S6).
  - `Set` → **the done-guard**: the resolved task is done ⇒ typed
    refusal `CommandError::TaskDone { selector }` (new variant, kind
    `"task-done"`, detail = selector; the `AnchorTaskReserved`
    app-layer-guard precedent — the engine never learns of it, fact
    3 keeps the command surface untouched). Archived tasks are
    live and repeatable (D-E1's posture). Then the anchor/series/
    defaults assembly (§3.2) and the `UpdateTask` execute.
  - Post-execute re-read (the `edit_on` shape) projects
    `RepeatReceipt` — `rule` from post-state `task.recur` with
    `next` from `next_occurrence` (§3.3), `now = Utc::now()` read
    once at handler entry.
- One-shot wrappers `repeat(config, store, invocation)` over
  `with_core`; the lane switches `repeat_command` (one-shot,
  `run_core`) and `repeat_line` (session, `emit_result`) mirror
  `remind_command`/`remind_line` verbatim.
- `render_text_repeat(&RepeatReceipt) -> String` — UX text (the set
  arm names pattern, bound and next; the cleared arm names the
  stop), never asserted.
- Dispatch: the `Command::Tasks` arms in `dispatch` (mod.rs:649) and
  `tasks_line` (mod.rs:1699) gain the `TasksCmd::Repeat` arm through
  `repeat::collapse` — both lanes, one fold. `route_for_ipc` needs
  **no** edit (fact 6: `Tasks { .. }` is wholesale `IpcOrLocal`);
  the lane unit test gains a Repeat row (§6).

## 5. The recurring badge (today view first, every card surface by
   construction)

### 5.1 The pure derivation (presentation.rs)

```rust
/// The recurring badge's typed parts (DR-6/4, D-S7): the domain's
/// next_occurrence law, displayed. `next: None` renders the
/// pattern-only badge (the series' last head); `stale` is the
/// overdue-recurring display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurBadge {
    pub next: Option<chrono::NaiveDate>,
    pub stale: bool,
    pub pattern: taskboard_domain::RecurPattern,  // label glue input
}

#[must_use]
pub fn recur_badge(task: &Task, day: DayCalendar, now: DateTime<Utc>)
    -> Option<RecurBadge>
```

Visibility: `task.recur` present ∧ `task.is_open()` (fact 4: done
heads are closed occurrences — the successor carries the series
forward and gets its own badge; the done row shows none). The body
is `next_occurrence` (§3.3) + the rule echo — thin by design, the
`tracking_chip` shape: the structural contract is testable here,
without an `AppState` fixture, and the app receipt calls the same
domain law (no duplication).

Label glue: `recur_badge_label(&RecurBadge) -> String` — composes the
domain `RecurPattern::text_form` with a day-granularity next phrase
(today / tomorrow / in N d — the `duration_bucket` day bucket over
`(next − today)`, computed against the window-date, **never a
wall-clock delta**). Day granularity keeps the label diff-stable
within a window (fact 10's churn discipline: re-pushes mid-day write
identical labels). Tests assert non-empty + deterministic; the
bucket math is asserted through `duration_bucket`, not text.

### 5.2 The model arm (models.rs) and the slot (task-card.slint)

- `DecorationKind` gains `recur` (appended last — the rule-D
  vocabulary grows; the generated enum is name-matched, no ordering
  contract).
- `field_decorations(task, tracked_task)` gains a `BadgeCtx`-shaped
  tail (`day: DayCalendar, now: DateTime<Utc>` — mechanical threading
  from `push`, which itself gains the calendar parameter; the adapter
  passes `DayCalendar` built from its held `day_start` pair,
  adapter.rs:509's sibling). The arm:
  `recur_badge(task, day, now)` ⇒ `TaskDecoration { kind: recur,
  label: recur_badge_label(…), tone: stale ? Tone::Overdue :
  Tone::Neutral }` — the tone mapping is models-side (generated
  enums live there), the decision is presentation-side.
- **Consequence, recorded**: the badge rides the shared decorations
  pass, so it renders wherever task cards render — board view,
  stack detail, task detail, today. The dispatch's acceptance
  surface is the today view; the shared-pass rendering is the
  anti-fork rule (one derivation, no per-surface gates) and kiosk
  reads it passively with no gate (fact 10). Compact board layouts
  collapse tertiary decorations (existing rule-D behavior) — the
  today view never compacts.

## 6. Test catalogue (TDD-first; the stage-2 commit order)

Assertions: typed outcomes and structural facts only — `matches!(err,
InvalidRecur::…)`, `error_kind()` strings, date/equality/`Option`
shape, bucket values. **No text assertions anywhere** (AGENTS §5);
no `tokio::time::sleep`; the hermetic suites are standalone-tempdir
like R-series (fact 7).

| # | Layer | Test | Pins |
|---|---|---|---|
| T1 | domain (`recur.rs`) | mint text-form unit tables | `parse_text_form ∘ text_form` accept/reject edges: `0d`→`ZeroInterval`, `0w`→`ZeroInterval`, empty set / `mon,,`→`EmptyWeekdaySet` or `BadWeekday{..}`, day `0|32`→`DayOutOfRange`, nth `0|6`→`WeekOutOfRange`, count `0`→`ZeroInterval`; case-sensitivity (`Mon` rejected — the importance-vocabulary posture); dedup (`mon,mon` == `mon`). |
| T2 | domain | text-form round-trip proptest | for generated mints/anchors: `parse_text_form(text_form(x)) == Ok(x)`; `RecurPattern`/`RecurBound` `text_form` pinned by a render-only unit table (no parse — no stringly grammar, D-S8). |
| T3 | domain | `next_occurrence` unit tables + proptest | stale daily ⇒ today (`roll_target`'s own law); weekly stale Wednesday ⇒ next Monday (the origin's example); fresh ⇒ `next_after` from reference; dateless ⇒ never stale, from window-date; `Until` exhaustion ⇒ `None`; night-owl window-date vs calendar-date (the timezone class: 03:00 with a 07:30 start is *yesterday*). |
| T4 | domain | re-home regression | the existing `command.rs` spawn/roll suites pass untouched (the move commit's pin — no new test, the suite is the harness). |
| T5 | app (`cli.rs`) | grammar parse tests | each flag's accept/omit/bare/value table over `Cli::try_parse_from`; the conflict matrix (two patterns, `--off`+anything, `--until`+`--count`, `--every-weeks` without `--weekly`, bound without pattern) are usage errors; bad values (`--count 0`, `--daily 0`, `--weekly mon,x`, `--monthly 15th-mon`, `--until tomorrow`) are usage errors. |
| T6 | app (`repeat.rs`) | collapse unit | every flag tuple folds to exactly one `RepeatInvocation`; bare `--weekly`/`--monthly` keep `on: None` (handler-resolved later). |
| T7 | app (`cli_repeat.rs`, P-series) | p1 set + durable | `--daily` on a seeded task: receipt carries `series`/`anchor`/`pattern`/`next`; a fresh boot's `tasks repeat <task> --off` then re-read sees the clear (durability across invocations). |
| T8 | app | p2 anchor chain | seeded scheduled ⇒ anchor = its calendar date; duedate-only ⇒ anchor = its calendar date; dateless ⇒ anchor = `DayCalendar{config}.today(now)` (the domain constructor computes the expectation — no reimplementation); a `[preferences] day_start_time` config pins the window-date flavor. |
| T9 | app | p3 re-repeat | second `repeat` with a different pattern: receipt `series` **equal** to the first (D-S4), pattern/bound replaced; a `--count` series' quota is engine-side and untouched here. |
| T10 | app | p4 off | `--off` clears (receipt `rule: None`); a repeat `--off` stays exit 0 with `rule: None` (post-state truth, fact 3's no-skip documented). |
| T11 | app | p5 typed refusals | pattern on a done task ⇒ kind `task-done`; unknown/ambiguous selectors ⇒ their existing kinds (`unknown-task`/`ambiguous-task`) — `matches!`/`error_kind()`, never text. |
| T12 | app | p6 session lane | `dispatch_line` over a `BasicSession` serves set + off end-to-end (the r10 shape); `route_for_ipc` returns `IpcOrLocal` for a `Repeat` command (the lane-table row). |
| T13 | ui (presentation.rs) | badge visibility/derivation | the §5 table: no rule → `None`; done → `None`; open fresh → `Some`, `!stale`, `next == next_after`; stale → `stale`, `next == roll_target`; until-exhausted → `next == None`; label glue non-empty + deterministic + day-stable (two `now`s in one window ⇒ equal label). |
| T14 | ui (models.rs) | the decoration arm | extends `field_decorations_carry_kind_tone_and_the_domain_label`'s shape: a recur task gains exactly one `recur` decoration with the ruled tone per stale; ordering relative to importance/urgency/tracking pinned (append-last). |
| T15 | gates | full suite | AGENTS §10 through `scripts/run-quality-gates.sh` (the flock wrapper — wave-7 neighbours share it). |

## 7. Explicit non-goals (each a named seam, not an oversight)

- **No pause/resume** — origin §3.7 rules it out of the round;
  `paused_until` is follow-up F-2 (with the habit UX follow-ups).
- **No per-series settings surface** — no grace, no archive policy,
  no series registry (origin §3.8; D-C4 keeps the rule on the task).
- **No auto-archive** of aged occurrences — declined at design
  (D-C6), revisit only on kiosk-wave evidence.
- **No spawned-successor done-receipt UX** — D-E7 named the CLI's
  "spawned successor" line a surface concern, but dispatch §10.3's
  acceptance does not list it, and `tasks done`'s receipt is
  unchanged here. Recorded as a candidate follow-up (a state-diff
  after `SetTaskDone`), not built.
- **No `tasks repeat --list` and no `tasks list` recurrence column** —
  the badge is the read surface; the receipt is the write surface.
- **No web-dashboard badge** — the dashboard's task rendering is
  untouched (the chunk's surfaces are CLI + slint today view).
- **No habit surfaces** — chunk DR-6/6 owns them (`habit` stays
  reserved in `DecorationKind`).
- **No conversion verb, no tracker reminders** — F-1/F-2 seams stand.
- **No `StateCommand`/engine/storage/IPC changes whatsoever** —
  fact 3; the command-surface and merge lanes are other chunks'.
- **No Kani harnesses** — F-3 rides #426.

## 8. Risks

- **R1 — clap group mechanics**: `requires`-on-group and bare-flag
  `Option<Option<T>>` handling inside a subcommand variant have
  derive-layer quirks (the `DueSpec` doc already records one).
  Mitigation: verify in the first hour of stage 2 against the `Remind`
  precedent; the two fallbacks (flattened `Args` struct; handler-side
  typed coherence refusal) keep every error typed.
- **R2 — `main` moves under the branch** (it moved twice during this
  planning session — wave chunks are landing). The dispatch protocol
  expects it: stage 3 rebases per AGENTS §8's rebase dance; conflicts
  are unlikely (wave-7 neighbours are `habit-domain-engine` and
  `tracking-storage-roaming`, disjoint files, and the soft lane rule
  keeps other `cli.rs` touchers out of the wave).
- **R3 — decoration threading width**: the calendar parameter ripples
  through `push` → five `push_*` fns → `decorations_model` →
  `field_decorations` (~8 signatures, one param each; several already
  carry `#[allow(clippy::too_many_arguments)]`). Mechanical,
  compiler-driven.
- **R4 — dateless daily heads badge "tomorrow"**: a dateless
  `Daily{1}` head shows next = tomorrow while completion would spawn
  tomorrow — correct, but users may read the badge as "due
  tomorrow". Accepted: the badge advertises the *series*, the
  due/scheduled chips advertise the *occurrence*; revisit only on
  evidence (the round's posture).
- **R5 — same-value clock churn**: repeated identical `repeat` lines
  re-stamp `clocks.recur` and re-enqueue the appdata op (fact 3).
  Accepted house posture (§3.4's benign-race ruling); the receipt
  reports post-state truth, and scripting repeats are idempotent in
  effect.

## 9. Decision record (this plan)

| ID | Fork | Ruling |
|---|---|---|
| D-S1 | Grammar shape | Flag-mode family on `tasks repeat` (the `Remind` shape): exactly one pattern flag (clap ArgGroup), bounds require the pattern, `--off` exclusive; required `<task>` positional (no list mode); value parsers wrap domain text forms, one per flag. |
| D-S2 | Bare-flag defaults | `--daily` bare = 1; `--weekly` bare = the task's own weekday; `--monthly` bare = the task's own day-of-month — resolved handler-side against the reference date (the calendar does not exist at parse time). |
| D-S3 | Anchor derivation | `series_reference_date` (scheduled → duedate → window-today) through the booted config's `DayCalendar`; the helper is re-homed `pub` to `recur.rs` (pure move; the engine suite pins it). No raw-UTC-as-local anywhere. |
| D-S4 | Re-repeat semantics | Repeat on an already-recurring task keeps `series` and `continued_from` and replaces `pattern`/`bound`; the `anchor` is never touched (§3.1's fixed-at-creation ruling). A new series id only from a plain task (minted app-side, `now_v7`). |
| D-S5 | The next-occurrence law | One domain function `next_occurrence` (stale ⇒ `roll_target`, fresh ⇒ `next_after`, dateless ⇒ from window-date never stale, `Until`-exhausted ⇒ `None`); the app receipt and the badge are thin consumers — no duplication across crates that cannot see each other. |
| D-S6 | Guards | Setting a pattern on a **done** task is refused typed app-layer (`TaskDone`, kind `task-done` — the engine never learns of it); `--off` works on any live task (clearing a dead rule is honest cleanup); archived tasks repeat (D-E1's live-and-completable posture). |
| D-S7 | Badge policy | Open heads only; stale ⇒ the overdue-recurring display (overdue tone); `Until`-exhausted ⇒ pattern-only badge (`next: None`); `Count` never surfaced (engine-evaluated history, chunk 1's D6); rides the shared decorations pass → every card surface, kiosk passive, no gate (display never gated, D-T16). |
| D-S8 | Text forms | Component-level `parse_text_form` (`Result<_, InvalidRecur>`, new `BadWeekday{got}` variant) on the mints + `MonthlyAnchor`; `RecurPattern`/`RecurBound` render-only canonical forms (no parse — the flag grammar is the only input syntax); round-trip proptest on components. |
| D-S9 | SeriesId minting | App-side `SeriesId::from_uuid(Uuid::now_v7())` — the `ui.rs` client-mint precedent; the engine is never asked to mint for a CLI-supplied payload. |
| D-S10 | Receipt | `RepeatReceipt { task_id, task_title, rule: Option<RecurRuleReceipt> }` with post-state truth (`rule: None` = cleared), `next` from the domain law — the mute-receipt precedent; renders are UX text, never asserted. |

## 10. Commit plan & estimated diff size

| # | Subject | Content |
|---|---|---|
| 0 | `docs(plan): series CLI and today-view badges` | this plan |
| 1 | `refactor(domain): the series reference chain reads from recur` | the §3.1 re-home (move + `pub` + re-import; zero behavior) |
| 2 | `feat(domain): the recur text forms` | §2.2's taxonomy: mints + `MonthlyAnchor` `parse_text_form`/`text_form`, `BadWeekday`, T1/T2/T3 |
| 3 | `feat(app): the tasks repeat verb family` | `TasksCmd::Repeat` grammar + parse fns (T5), `repeat.rs` collapse/handlers/receipts/guard (T6, T7–T12), `output.rs` receipts, `CommandError::TaskDone`, dispatch arms + lane-table row |
| 4 | `feat(ui): the recurring badge on task cards` | `DecorationKind::recur`, presentation badge + label (T13), models arm + calendar threading (T14), adapter pass-through |

Estimated size: ≈ **1.3–1.7k insertions** across ~14 files —
`recur.rs` +~280 (text forms + re-home + tests), `command.rs` −~35/+10,
`cli.rs` +~180, `commands/repeat.rs` +~320, `output.rs` +~70,
`commands/mod.rs` +~60, `presentation.rs` +~150,
`models.rs` +~60, `adapter.rs` +~15, `task-card.slint` +~4,
`cli_repeat.rs` +~420. No migration, no ADR, no snapshot-shape change
(not a (w) chunk; the models decoration test extends in place).

## 11. References

- Origin (normative): `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §3.7 (the verbs), §4.6 (CLI conventions), §7 chunk 4, §5.4/§6
  (resources, D-C1–D-C13).
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (lanes, protocol, mandate), §3.5 (DR-6 binding notes), §10.3
  (this chunk).
- Sibling plans: `2026-10-27-series-domain-core-plan.md` (D1–D13 —
  D12 deferred the text forms here; D6 the Count posture),
  `2026-10-28-series-engine-plan.md` (D-E1–E12 — D-E5 the date
  conventions, D-E7 the surface note, D-E10 the stale predicate,
  D-E12 the born-stale presentation hand-off).
- Issue **#501** (`backlog: DR-6 chunk 4 — series surfaces: tasks
  repeat CLI + today-view badges`, open; labels p2/architecture/ui/dx).
