# [Plan] Recurring Tasks & Habits — Design Round (DR-6)

- **Date**: 2026-10-26
- **Kind**: design-round doc (design only — no production code in this
  commit). Lineage: issue **#212** is the roadmap's Wave-3 gate the
  backlog-triage dispatch deliberately left undispatched
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §15,
  "roadmap waves & ADHD follow-ups"); with DR-1–DR-4 accepted and DR-5
  (time tracking, #210) in planning, the user dispatched this round on
  2026-10-26. It is **DR-6** in the house design-round numbering.
- **Origin**: issue **#212** (normative: materialisation policy,
  occurrence identity, interaction with reminders/time tracking/sync,
  proptest strategy for recurrence arithmetic; habit streaks, pleasant
  reminders, tips/motivation build on the outcome). The analysis inputs:
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §3.1 + **D3** (the recorded bias: materialise-on-completion +
  re-arming rule) and §6 Wave-3 gate; the audit facts **F16-09**
  (branch pricing), **F16-10** (occurrence-key precedent),
  **F16-11** (clock-seam tick) from
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`;
  the ADHD research base
  (`.artifacts/reports/2026-10-14-adhd-support-research-report.md`,
  principles P3/P5/P7/P12/P13 — anti-configurability, forgiveness,
  low ceremony) via #215/#216/#218's citations.
- **Session rulings (2026-10-26, recorded as D-C1–D-C13 in §6)**: the
  user answered twelve design forks across three prompting rounds —
  materialisation (completion-spawn **plus** a day-boundary
  roll-forward tick, argued in-session), missed days (absence =
  missed), grammar (typed closed enum), template home (rule rides the
  task), Deck roaming (plain cards, no auto-archive), spawn races
  ((series, date) key + fold dedup), habit scope (**fully in-round**),
  habit home (split by nature — all habits are trackers, after an
  in-session pros/cons analysis incl. conversion costs), streak grace
  (window attribution, no knob), tracker pings (named follow-up),
  conversion (seam now, verb later), issue timing (open with
  the plan). D-C13 (manual habit ordering, optional time decoration)
  was added at plan review the same day.
- **Branch / worktree**: `docs/recurring-habits-design-round` in
  `../taskboard-WT-docs-recurring-habits-design-round` (off
  `origin/main` @ `82f8b79`).
- **Status**: **ACCEPTED** — the user reviewed and accepted the plan
  as designed (2026-10-26, in-session review; the D-C13 ordering
  amendment and the successor midpoint-insertion fix come from that
  review). Implementation is
  the chunk issues the round opens (§7); a follow-up dispatch round
  chunks them after acceptance.

---

## 1. Context & verified anchors (verified on this branch 2026-10-26)

Numbered facts; every design section below cites these.

1. **Task shape** (`crates/taskboard-domain/src/entities.rs`): `Task`
   carries `duedate`/`scheduled`/`done: Option<DateTime<Utc>>`,
   `stack`+`order`, `labels: BTreeSet<LabelId>`, `effort`,
   `importance`, `urgency`, `archived`, `deleted`, and per-field LWW
   write clocks (`TaskClocks`). `done` is one timestamp; a task is
   done exactly once (`is_open() = done.is_none()`).
2. **Field-family lockstep** (`entities.rs` `TASK_FIELDS` +
   `FieldFamily::{Native, AppData}`): every new `Task` field joins the
   manifest, a `TaskClocks` slot, and a sync-lane decision.
   `effort`/`importance`/`urgency` are the appdata-lane precedent
   (migration `0007_task_field_family.sql` added their columns).
3. **Sparse edits**: `TaskChanges` (`messages.rs`) uses
   double-`Option` (`Option<Option<T>>`) for clear-vs-untouched.
4. **Completion**: `StateCommand::SetTaskDone { id, done }`; the
   planner arm (`command.rs`, the block that stamps `done`/`clocks.done`
   and auto-records reminder dismissals) is the seam the spawn hook
   joins. Reopen (done→open) exists and reverses the stamp.
5. **Reminders occurrence model** (`reminders.rs`): the suppression
   guard is `(reminder, occurrence)` with `occurrence =
   DateTime<Utc>`; moving a due date re-arms by construction. Rules
   are due-anchored offsets (`ReminderRule { task, offset, … }`);
   `ReminderRef::{Rule, Default, Adhoc}`; the evaluator
   `due_firings` fires iff `effective ≤ now ∧ ¬delivered ∧
   ¬dismissed`. Storage enforces the pair as PK (migration `0004`).
6. **Engine tick**: `reminder_tick` (`taskboard-state/src/actor.rs`)
   reads the injected `Clock` once per tick, persists a batch, then
   notifies — the only mutating tick today (F16-11: the seam recurrence
   needs exists; no new clock work).
7. **Day window**: `days.rs::day_window` — the user-configurable
   "today" window from `[preferences] day_start_time` plus a device
   fixed UTC offset; pure, total, already consumed app/UI-side. This
   is the house vocabulary for "local date"; recurrence anchors ride
   it (no second timezone concept).
8. **Appdata substrate** (`appdata.rs`): the per-board sidecar
   document (D1 spike conclusion; landed per
   `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`) —
   versioned envelope, section registry, embedded-clock `union`,
   tombstones, verbatim relay for unknown rows/sections, GC hooks
   (`prune_absent_cards` is card-keyed: a board-level section is
   invisible to it — the "history outlives task deletion" property
   DR-5 relies on). Sections today: `tasks` (per-task appdata-lane
   fields), `reminder-rules`, `dismissals`; `proofs`/`settings`
   reserved.
9. **Duplicate-collapse precedent**: DR-4 (accepted 2026-10-26,
   `2026-10-26-sync-merge-policy-design-plan.md` §3 D-5) — detection
   predicate + survivorship (keep older) + suppressed adoption +
   tracing-only, *triggered only when an observed duplicate exists*.
   The recurrence fold-dedup (§3.6) reuses the pattern with a typed
   key instead of title heuristics.
10. **Storage**: migrations `0001`–`0009` on main; `0010` is claimed
    by DR-3 (insistence fields), `0011` by DR-5 (time entries) — this
    round takes **0012**. Conventions: additive-only plain SQL, UUID
    TEXT ids, rfc3339 TEXT stamps, no triggers/views, logic in Rust,
    sqlx offline cache at `.sqlx/`.
11. **Sync lanes**: native card fields push as Deck cards (full-send,
    id-less `LocalOp`); appdata-lane fields ride the sidecar document
    with `UpdateAppData` ops and flush materialization. Deck cards
    have **no recurrence concept** (wire `Card` in
    `taskboard-sync-nextcloud/src/model.rs`) — recurrence is an
    app-only field, so the rule rides the appdata lane while its
    occurrences (being real tasks) roam as cards for free.
12. **ADR numbering**: `docs/adr/` through 0016; DR-5 claims **0017** —
    this round writes **ADR 0018**.
13. **Undo**: designed not built (`2026-10-19-undo-architecture-design-plan.md`);
    D-U2's pushed-edit rule (undo after push = new edit) binds any
    future undo of recurrence edits. The implicit-dismissal side
    effect on completion is deliberately non-invertible (house
    precedent; recurrence adds spawn, which is likewise not
    auto-inverted — see §5.3).
14. **DR-5 time tracking (in flight, #210)**: `TimeEntry` + board-level
    `time-entries` appdata section keyed by entry UUID, 180-day wire
    retention, single open entry, migration 0011, ADR 0017. Occurrence
    tasks of a series are ordinary tasks — entries attach to them
    with zero new machinery (§5.1).

## 2. The two halves, one paragraph each

**Series (recurring tasks).** The open head task *is* the template:
it carries `recur: Option<RecurRule>` (appdata-lane field, §3.1).
Completing it spawns the successor inside the same pure plan (§3.3);
a day-boundary tick rolls a stale open head's scheduled/duedate
forward in place so the task "appears today" without ceremony (§3.4);
missed days leave no rows and are derived (§3.5); occurrences roam as
plain Deck cards; double spawns collapse at fold by
`(series, scheduled-date)` key (§3.6).

**Habits (trackers).** All habits — binary and counted — are a second
entity kind, the *habit tracker*: a never-done, board-scoped record
with a target (`n` per day/week/month) and a completion log keyed
`(habit, window)` — the `(reminder, occurrence)` shape applied
unchanged. Streaks are pure functions over the log with window
attribution and no knobs. Trackers ride the appdata sidecar only —
Deck sees nothing (§4).

The split is by nature, decided in-session (D-C9): things that must
move through the board on a schedule are series; things you want to
do N times are trackers. A `continued_from` seam on both makes
misfiling recoverable without data fabrication (D-C12).

## 3. Design — the series half

### 3.1 The rule and where it lives (D-C4, D-C5)

```rust
pub struct RecurRule {
    pub series:  SeriesId,        // new id kind; UUIDv7-minted, stable
                                    // across the whole occurrence chain
    pub anchor:  NaiveDate,       // first scheduled local date; fixed at
                                    // creation; roll/spawn never touch it
    pub pattern: RecurPattern,
    pub bound:   RecurBound,      // None | Until(NaiveDate) | Count(u32)
    pub continued_from: Option<HabitId>,   // §4.7 seam
}
// Task gains: pub recur: Option<RecurRule>   (appdata lane)
// TASK_FIELDS row ("recur", AppData); TaskClocks slot ck_recur;
// TaskChanges gains recur: Option<Option<RecurRule>>.
```

A1 discipline (AGENTS §2, `coding_guidelines.org` §2.1): every
numeric component is a hand-rolled newtype with a fallible mint
rejecting zero/empty (e.g. `EveryDays(u16)` via
`EveryDays::new(n) -> Result<_, RecurError>`; `WeekdaySet` minted
from a non-empty iterable, canonical `BTreeSet<Weekday>`); no derive
crates. Deserialization of stored/wire data is the documented-leniency
boundary (decode-then-validate, invalid rows fold as
`AppDataEvidence`-style typed failures rather than poisoning state —
same posture as `ReminderRuleId` on the wire, DR-1 D-4).

### 3.2 The grammar (D-C3) and its arithmetic

```rust
pub enum RecurPattern {
    Daily   { every: EveryDays },                 // every n days
    Weekly  { interval: EveryWeeks, on: WeekdaySet },
    Monthly { on: MonthlyAnchor },                // OnDay(1..=31) | NthWeekday(n, wd)
}
```

All dates are **device-local** `NaiveDate`s derived through
`days.rs`'s fixed-offset convention, converted to/from UTC only at
the task-field boundary (`scheduled`/`duedate` stay
`DateTime<Utc>`). Core total function:

```rust
pub fn next_after(p: &RecurPattern, from: NaiveDate) -> NaiveDate;
pub fn occurs_on(p: &RecurPattern, d: NaiveDate) -> bool;   // schedule membership
```

Rulings inside the arithmetic (each pinned by proptest, §3.9):

- **Clamping**: `Monthly::OnDay(29|30|31)` in a short month clamps
  to that month's last valid day (Feb 31 → Feb 28/29). The schedule
  is defined by the pattern, not by spawn history.
- `Daily` with `every: n` steps calendar days from `anchor`
  alignment (occurrences are `anchor + k·n`), so a rolled-forward
  head never drifts the phase.
- `Weekly`/`Monthly` are anchor-aligned by the same rule: membership
  `occurs_on` decides, `next_after` returns the next member strictly
  after `from`. Anchor alignment (not "step from current") is what
  makes roll-forward and offline catch-up gap-agnostic and
  deterministic — identical inputs give identical targets on every
  device (D-C7's no-dedup-for-rolls property).

### 3.3 Completion spawn (D-C1)

Inside the `SetTaskDone` planner arm (fact 4), after the done-stamp,
when `task.recur` is `Some(rule)` and the bound admits a next
occurrence (§3.7): the plan additionally emits a `CreateTask`-shaped
persistence sequence for the **successor**, in the same plan —
`plan_command` stays pure and total; no new tick, no async.

Successor contents (rulings):

- `title`, `description`, `labels` copied; `stack` copied; the
  successor lands **immediately after the head** in sort order:
  `order = head.order + 1` when that slot is free, else the midpoint
  between the head and the next task in the stack (midpoint insertion,
  the UI's own drag convention — the naive `+1` could tie an occupied
  slot; amended 2026-10-26 on review).
- `scheduled`/`duedate` = `next_after(pattern, head_scheduled_date)`
  converted through the day-window offset to UTC. If the head has no
  `scheduled`, `duedate`'s local date is the reference; if neither
  exists the spawn reference is the completion day.
- `recur` copied verbatim (same `series`, same `anchor`).
- Reminder rules **replicate**: the planner emits upserts for each of
  the head's `ReminderRule`s against the successor's task id (same
  offsets). Delivery/dismissal/snooze history does **not** copy — the
  successor's occurrences are new keys, so reminders re-arm by
  construction (fact 5; zero new reminder machinery).
- `SetTaskRemindersMuted` state replicates (a muted chore stays
  muted across occurrences).

The successor is an ordinary task from birth: it syncs as a card
(native fields), carries the appdata entry with its `recur` (so every
device that adopts the card also learns the series), accepts time
entries, moves stacks, and — when it in turn completes — spawns its
own successor. **Occurrence identity** is `(rule.series,
scheduled-local-date)`, derivable from any occurrence task via its
`recur.series` + its `scheduled`/`duedate` local date; it is not a
stored column (§3.6 uses it as a fold-time join key, not a FK).

### 3.4 Day-boundary roll-forward (D-C1, the in-session addition)

Requirement (user, verbatim intent): never force the user to mark
yesterday's task done so that it appears today. The rejected
alternative — a display-only roll that renders a stale head as
"today's" while the stored fields stay behind — creates two truths
for one field and leaks into reminders, Deck, and analysis; the
house's state-is-truth rule decides for a real edit.

Mechanics:

- The engine gains a second tick arm beside `reminder_tick`
  (`day_tick`), same shape: read the injected `Clock` once, compute a
  batch, persist-then-publish; a quiet day is a full no-op. It runs at
  the same cadence source but only does work when a rollover is
  detected (the window start moved since the last applied roll —
  engine-internal watermark; F16-11's seam).
- A head is **stale** iff it is open (`done: None`), live, carries a
  rule, and its scheduled local date is strictly before today's local
  date (per `day_window`).
- Roll target — a pure function, the round's second core law:

```rust
pub fn roll_target(p: &RecurPattern, anchor: NaiveDate,
                   stale: NaiveDate, today: NaiveDate) -> NaiveDate;
// = the latest member d with stale < d <= today (dailies: today);
//   if no member lies in that range (e.g. weekly case, today=Wed),
//   the first member d > stale (next Monday).
```

- The roll is a **field edit**: `scheduled`/`duedate` move to the
  target, fresh `clocks.scheduled`/`clocks.duedate`, ordinary outbox
  push (card PUT). Missed intermediate days are simply skipped —
  absence = missed (D-C2); nothing spawns, nothing is recorded.
- **Idempotent and convergent**: `roll_target` is deterministic in
  `(pattern, anchor, stale, today)`; two devices rolling the same
  head produce the same target; the LWW clocks make the race benign
  (same-value writes). Offline/kiosk catch-up is the same path — the
  first tick after boot rolls to the current window; no retroactive
  history is fabricated.
- Reminder interplay: the roll changes the due dates → new occurrence
  keys → due-anchored rules re-arm for free (fact 5).

### 3.5 Missed days and history (D-C2, D-C8)

History = real tasks (done stamps, stacks, entries — facts 1, 14).
The *schedule* is a pure function:

```rust
pub fn schedule(rule: &RecurRule, from: NaiveDate, to: NaiveDate)
    -> Vec<NaiveDate>;                       // members in [from, to]
pub fn occurrence_outcome(rule, day, completions) -> Outcome;
// Outcome::Done{at} | Missed | NotYetReachable — window attribution:
// a completion of the head between day's window start and the next
// member's window start attributes to `day`.
```

Late completions therefore still count (attribution window = until
the next occurrence) with **no knob** (D-C8 — the ADHD research's
anti-configurability finding, #215's evidence, applied). "Missed" is
never stored anywhere; every analysis function derives it.

### 3.6 Sync: roaming and fold dedup (D-C6, D-C7)

Roaming:

- Occurrences are native cards — they already push/pull as tasks.
- The `recur` field rides the existing per-task appdata entry (the
  `tasks` section row gains the rule + clock), so a device adopting
  an occurrence card learns the series with it. No new section for
  series facts.

Fold dedup (the spawn race): two devices can complete the same head
offline; each spawns a successor with the same
`(series, scheduled-date)`. At `apply_sync_report` fold time, after
the task-merge loop, a pass (DR-4 D-5's shape, typed key instead of
titles):

- **Trigger**: only when two *live, open* tasks share
  `recur.series` and the same scheduled local date. (The invariant is
  one open head per series; twins can only come from the race —
  structural, not configured.)
- **Survivorship**: keep the older-created task (DR-4 D-5c's rule —
  the first spawn owns any remote history the race window could not
  have), merge field clocks, neutralize the newer as a ref-less
  tombstone (the `reconcile.rs` neutralization shape), suppress any
  pending duplicate adoption.
- **Honesty surface**: `tracing` typed facts only (rule tag
  `R-recur-dup`); no SystemEvent, no UI churn.

### 3.7 Bounds and lifecycle (D-C5, D-C12)

- `Until(date)`: spawn/roll suppress when the next target exceeds the
  date; the then-current head completes/rolls like a plain task and
  the rule clears (`recur → None`) at its terminal transition — the
  series ends as an ordinary standalone task.
- `Count(n)`: the rule carries occurrences-so-far implicitly — the
  count is evaluated against the series' completion history (done
  tasks with this `series` id, derivable; no counter column). When
  completions ≥ n, the final completion spawns nothing and clears the
  rule.
- `task → series` ("repeat this task"): set `recur` via
  `UpdateTask`; `anchor` = the task's scheduled/duedate local date,
  else today. Full in-round command (CLI §7 chunk 4).
- `series → task` ("stop repeating"): clear `recur` via `UpdateTask`.
  The current occurrence remains an ordinary task.
- No pause/resume in this round (a `paused_until` on the rule is a
  named follow-up — §7 F-2); absence-derived misses cover the honest
  default.

### 3.8 What this deliberately does **not** build

No RRULE strings, no virtual-instance projections, no skip records,
no per-series settings surface (grace, pause, archive policy), no
series registry entity, no reminder changes at all.

### 3.9 Proptest catalogue (the issue's explicit ask)

Properties over generated `(pattern, anchor, from)` triples (pure
domain, `taskboard-domain`):

1. `next_after` total, strictly-after `from`, and a member
   (`occurs_on(next_after(..)) == true`).
2. Anchor alignment: repeated `next_after` enumerates exactly
   `schedule(anchor, ..)` — roll/spawn/completion paths agree on one
   schedule.
3. Clamping: `Monthly::OnDay(d)` yields exactly one member per month,
   the min(d, last-day) one.
4. `roll_target` laws: result > stale; result ≤ today ⇒ it is the
   latest member ≤ today, else it is the first member > stale; for
   `Daily{1}` it is always `today`; offline gaps (today ≫ stale)
   change nothing about the target except `today`.
5. Attribution: every completion stamp attributes to exactly one
   member (partition property); late completions attribute to the
   earlier member.
6. Streak (series): consecutive-member counts match a hand model on
   generated completion sequences; a single miss breaks it exactly
   after its grace-free window.
7. Fold-dedup predicate: a no-op whenever no twin exists
   (structural); survivorship is older-created regardless of arrival
   order (permutation invariance).
8. Round-trip: rule ↔ appdata JSON row ↔ decode is lossless;
   truncated views fold to tombstones, never to resurrected rows.

Kani note: `next_after`/`roll_target`/`occurs_on` are pure and total
— prime candidates to join the #426 Kani pilot's harness list once it
lands (recorded as follow-up F-4, not a dependency).

## 4. Design — the habit half (trackers)

### 4.1 Entity (D-C9)

A third nested state home in `AppState`, the `ReminderState` /
DR-5 `TrackingState` pattern:

```rust
pub struct HabitState {
    pub habits: BTreeMap<HabitId, Habit>,
    pub log:    BTreeMap<HabitLogKey, HabitLogEntry>,  // set-union facts
}
pub struct Habit {
    pub id: HabitId,               // roaming id family (like ReminderRuleId)
    pub board: BoardId,            // trackers are board-scoped (roam lane)
    pub title: String,
    pub target: HabitTarget,
    pub order: i64,                // manual list order (D-C13); UI drags
                                    // assign midpoints, à la MoveTask
    pub anchor_time: Option<NaiveTime>,  // optional local time-of-day:
                                    // display chip + F-1 nudge anchor;
                                    // NEVER a sort key
    pub clocks: HabitClocks,       // per-field LWW (title, target, order,
                                    // anchor_time, tombstone)
    pub continued_from: Option<SeriesId>,   // §4.7
}
pub enum HabitTarget {              // A1 mints; counts are NonZero
    PerDay(NonZeroCount), PerWeek(NonZeroCount), PerMonth(NonZeroCount),
}
pub struct HabitLogKey { pub habit: HabitId, pub window: WindowStart } // local period-start date
pub struct HabitLogEntry { pub count: u32, pub clock: DateTime<Utc> }
```

The log is the `(reminder, occurrence)` fact shape applied unchanged
(fact 5, F16-10): keyed, set-union-mergeable, monotone counts under
embedded clocks. **Never-done is native** — no spawn, no roll, no
done stamp; a window's state is purely its log row.

### 4.2 Commands

`StateCommand` gains the habit family (planner arms in the house
catalogue style):

- `CreateHabit { board, title, target }` — UUIDv7 mint.
- `UpdateHabit { habit, changes: HabitChanges }` — sparse rename /
  target / anchor-time edit (double-Option, like `TaskChanges`).
- `MoveHabit { habit, order: i64 }` — manual reorder (D-C13), mirroring
  `MoveTask`; the UI computes midpoint positions on drag so relative
  order survives arbitrary reorders.
- `LogHabit { habit }` — +1 into today's window (idempotent per
  invocation, monotone count under clocks).
- `AdjustHabitLog { habit, window, count }` — explicit correction /
  backdating (audited edit, fresh clock — the tuning valve analysis
  functions never use).
- `DeleteHabit { habit }` — tombstone (resurrection-safe on sync;
  log GC'd per §4.5 retention).

### 4.3 Streaks and attribution (D-C8, D-C10)

Pure projections, proptest-pinned:

- A **period counts** iff quota met by period end **+ 24 h overflow**
  (window attribution; no knob).
- **Streak** = consecutive counting periods ending at the current
  one. Binary habits (`PerDay(1)`) streak per day; weekly habits
  (`PerWeek(3)`) streak per ISO week; the attribution rule is one
  function parameterized by period kind.
- Missed periods are derived (absence = missed), identically to
  series days — one analytical vocabulary across both halves.

### 4.4 Storage (migration 0012, fact 10)

```sql
-- tasks: ALTER TABLE tasks ADD COLUMN recur TEXT NULL;      -- JSON row
--       ALTER TABLE tasks ADD COLUMN ck_recur TEXT ...;
-- habits:
CREATE TABLE habits (habit_id TEXT PRIMARY KEY, board_id TEXT NOT NULL,
    title TEXT NOT NULL, target_kind TEXT NOT NULL, target_n INTEGER NOT NULL,
    sort_order INTEGER NOT NULL, anchor_time TEXT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
    ck_title TEXT NOT NULL, ck_target TEXT NOT NULL,
    ck_sort_order TEXT NOT NULL, ck_anchor_time TEXT NOT NULL,
    ck_deleted TEXT NOT NULL);
CREATE TABLE habit_log (habit_id TEXT NOT NULL, window_start TEXT NOT NULL,
    count INTEGER NOT NULL, updated_at TEXT NOT NULL,
    PRIMARY KEY (habit_id, window_start));
```

One migration carries both halves (the round lands as one family;
`recur` is a single JSON column because the rule is a composite —
A1 validation happens at the decode boundary, documented leniency
there, like every appdata-lane field).

### 4.5 Sync / roaming (D-C6)

Trackers ride the appdata sidecar only — **zero Deck cards** (the
deciding D-C9 cost: a daily binary habit would otherwise cost 365
done cards/year on a shared board):

- New board-level section **`habits`**: definitions as the
  `RuleEntry` row pattern (`Habit { …, clock } | Tombstone { clock } |
  Verbatim`), union-merged on embedded clocks.
- New board-level section **`habit-log`**: events keyed
  `(habit, window_start)`; set-union on the pair, count LWW under the
  row clock. **180-day wire retention** mirroring DR-5's ruling — the
  document carries the recent window; local truth keeps everything;
  GC is document-side only.
- Board-level keys are invisible to `prune_absent_cards` (fact 8) —
  habit history outlives anything, by the same property time entries
  rely on.

### 4.6 Surface (today view, CLI)

v1 surface: a **Habits section in the today view** — one progress row
per tracker ("water 5/8"), tap/+1 dispatching `LogHabit`; streak
decoration per row. **Ordering (D-C13)**: the section renders in
stored manual order (`order`, midpoints on drag); a habit's optional
`anchor_time` is a display chip only — timed and untimed habits
interleave in *your* order, never clock-sorted (a one-shot "sort by
time" affordance, if ever wanted, is just a batch of `MoveHabit`
edits, not a mode). CLI: `taskboard habits add/list/log/adjust/rm`
family (the two-lane `with_core` recipe). **No reminders in v1**
(D-C11): trackers are today-view items; one-off pings remain the
existing adhoc timers; the `ReminderRef::Tracker(HabitId)` extension
is follow-up F-1 with its seams named there.

### 4.7 The conversion seam (D-C12)

`Habit.continued_from: Option<SeriesId>` and
`RecurRule.continued_from: Option<HabitId>` (§3.1, §4.1). Conversion
in v1 is two manual steps (create the other entity, stop/complete the
old one) + the link; **streak/history views union both sides at read
time** via the link — no fabricated done tasks, no collapsed logs.
The one-click verb is UI follow-up F-2. Task→series and series→task
(§3.7) are full in-round commands and are *not* conversions (same
entity, one field).

## 5. Cross-cutting

### 5.1 DR-5 time tracking

Occurrence tasks are ordinary tasks — time entries attach per
occurrence with zero new machinery, which is exactly the
"consistent history per occurrence (time tracking…)" the original
brainstorm demanded. The never-done tracker-*task* kind stays #210's
follow-on field work; habit trackers (this round) are a separate
entity, not a `TaskKind`.

### 5.2 Kiosk / offline

Boot catch-up is the day-tick path (§3.4) — no retroactivity, one
card PUT per stale series. Bandwidth: occurrences roam as cards
(accepted noise, D-C6); habits cost sidecar bytes only, GC-bounded.

### 5.3 Undo (future)

Spawn/roll/log are ordinary plans: D-U2's pushed-edit rule applies
unchanged; inverse synthesis of `SetTaskDone` must account for the
spawned successor (an undo of a completing-with-spawn is a reopen +
successor delete — record in the undo round's inputs, not built
here).

### 5.4 Resources claimed by this round

| Resource | Claim | Collision note |
|---|---|---|
| Migration | **0012** | 0010 = DR-3, 0011 = DR-5 |
| ADR | **0018** | 0017 = DR-5 |
| Appdata sections | **`habits`**, **`habit-log`** | new; `recur` rides existing `tasks` rows |
| Design round | **DR-6** | DR-5 in flight (#210) |

### 5.5 Risks

- **Fold-dedup false positives**: impossible-by-construction trigger
  (one open head per series+date invariant) — but a user *restoring*
  a completed occurrence by reopen while its successor lives creates
  two open tasks in one series (different dates ⇒ no trigger; same
  date impossible). Pinned by property 7.
- **day_tick is the engine's second mutating tick** — bounded by
  design: deterministic, idempotent, quiet-day no-op; the watermark
  is engine-internal (never persisted as user state).
- **Daily-series card volume** on Deck (accepted, D-C6); the
  auto-archive mitigation was explicitly declined — revisit only on
  evidence (kiosk wave measurements).
- **Appdata format churn**: two new sections + a `tasks`-row field
  land behind the existing format-version discipline; verbatim relay
  keeps older clients safe (fact 8).
- **`Count` evaluation scans series history** (done tasks by series
  id) — fine at human scale; revisit only if a series ever exceeds
  thousands of occurrences (not a v1 concern; recorded for honesty).

## 6. Decision record (session 2026-10-26)

| ID | Fork | Ruling |
|---|---|---|
| D-C1 | Materialisation | Completion-spawn **+** day-boundary roll-forward tick (in-session addition; display-only roll rejected — two truths for one field) |
| D-C2 | Missed days | Absence = missed; no skip records; derived by rule arithmetic + completions |
| D-C3 | Grammar | Typed closed enum (Daily/Weekly/Monthly + Until/Count); no RRULE |
| D-C4 | Template home | Rule rides the task (`Task.recur`, appdata lane); no registry entity |
| D-C5 | Identity | `SeriesId` stable across chain; occurrence key `(series, scheduled local date)`, derived not stored |
| D-C6 | Deck roaming | Occurrences roam as plain cards; no auto-archive; habits ride sidecar only |
| D-C7 | Spawn races | (series, date) key + fold-time dedup (DR-4 D-5 pattern); rolls need no dedup (deterministic target) |
| D-C8 | Streak grace | Window attribution, no knob (late counts until next occurrence; period + 24 h) |
| D-C9 | Habit home | Split by nature: all habits are trackers (pros/cons analysed in-session; conversion costs included); series only for board-moving work |
| D-C10 | Habit scope | Habits fully in-round (entity, log, streaks, surfaces); pings and convert-verb are follow-ups |
| D-C11 | Tracker pings | Follow-up with named seams (`ReminderRef::Tracker`, evaluator arm, DR-1 roam ruling) |
| D-C12 | Conversion | `continued_from` seam + read-side union now; one-click verb later; task↔series are full verbs, not conversions |
| D-C13 | Habit ordering (review 2026-10-26) | Manual stable order: `order` + `MoveHabit` (midpoints on drag); optional `anchor_time` is decoration + F-1 nudge anchor, never a sort key; timed/untimed interleave in stored order |

## 7. Implementation chunks (the issues this round opens)

All chunks: `backlog:` issues, unscheduled, body = summary +
acceptance + permalink to this plan (AGENTS §7; the plan rides this
branch until its PR merges). Suggested labels in brackets. A
follow-up dispatch round sequences them after acceptance; chunks 1→2→3
serial (same crates), 4 after 2, 5→6 serial, parallel to the series
chain where crates allow.

| # | Chunk | Scope | Size |
|---|---|---|---|
| 1 | Series domain core | RecurPattern/RecurRule A1 types, `next_after`/`occurs_on`/`schedule`/`roll_target` + proptests 1–5, `Task.recur` field-family integration (`TASK_FIELDS`, `TaskClocks`, `TaskChanges`) | M |
| 2 | Series engine | `SetTaskDone` spawn arm (rules/mute replication, bounds), `day_tick` roll arm + watermark, snapshot/insta coverage | M |
| 3 | Series storage + sync | Migration 0012 (recur columns), appdata `tasks`-row recur facts, fold dedup predicate + survivorship + property 7/8, wiremock round-trip journey | M |
| 4 | Series surfaces | CLI `tasks repeat` family (`--daily/--weekly/--monthly`, `--until/--count`, `--off`), today-view next-occurrence badge + overdue-recurring display | S |
| 5 | Habit trackers — domain & engine | `HabitState`/`Habit`/log, commands (§4.2), attribution + streak functions + proptests, `continued_from` | M |
| 6 | Habit trackers — storage, sync, surfaces | Migration 0012 habit tables, `habits`/`habit-log` sections + GC + retention, CLI `habits` family, today-view Habits section | M |

Follow-up issues (named seams, not implementation chunks):

- **F-1**: tracker reminder extension (`ReminderRef::Tracker`,
  evaluator arm, DR-1 roam-set ruling, DR-3 insistence interplay).
- **F-2**: habit UX follow-ups — one-click convert verb, series
  `paused_until`, streak-repair affordances.
- **F-3**: Kani harnesses for recurrence arithmetic (rides #426's
  pilot; `next_after`, `roll_target`, `occurs_on`).

ADR 0018 ("recurring series — materialise-on-completion with
day-boundary roll-forward; habits — window-keyed log trackers") is
written by chunk 2 (first code that realizes the rulings).

## 8. References

- Issue #212 (normative) and its origin
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (§3.1, D3, §6 Wave 3).
- Audit facts F16-09/10/11:
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`.
- Precedents: DR-4 §3 D-5 (`2026-10-26-sync-merge-policy-design-round-plan.md`),
  DR-5 (`2026-10-26-time-tracking-design-round-plan.md`, in flight),
  reminder-rule sync D-1–D-14 (`2026-10-25-reminder-rule-sync-design-plan.md`),
  appdata sidecar D-S1–D-S9 (`2026-10-18-appdata-sidecar-design-plan.md`),
  undo D-U1–D-U5 (`2026-10-19-undo-architecture-design-plan.md`).
- ADHD research principles via #215/#216/#218 citations
  (`.artifacts/reports/2026-10-14-adhd-support-research-report.md`).
