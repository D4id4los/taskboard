# [Plan] Series Engine — Completion Spawn & Day-Boundary Roll (DR-6/2)

- **Date**: 2026-10-28
- **Kind**: implementation plan (stage-1 draft of the dispatch protocol;
  TDD-first; no production code in this commit).
- **Origin** (normative, decision by decision):
  `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §3.3 (completion spawn), §3.4 (day-boundary roll-forward), §3.5 (missed
  days — derived only; nothing to build here beyond the derived-vocabulary
  guarantee), §3.7 (bounds & lifecycle), §1 facts 4–6 (planner seam,
  reminder occurrence model, engine tick), §5.3 (undo interplay), §7
  chunk 2; plus the dispatch plan
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (lanes, wave protocol, code-quality mandate) and §8.1 (this chunk's
  prompt and acceptance). DR-6 binding notes (dispatch §3.5): rulings
  D-C1–D-C13; days.rs is the only "local date" vocabulary; no reminder
  changes; no pause/resume; A1 discipline for new numeric types.
- **Preconditions verified** (2026-10-28, `origin/main` @ `22b5f5d`):
  DR-6/1 merged (PR **#525**, "recurring series domain core" — the brief's
  #498 is the pre-rebase number; `RecurRule` grammar/arithmetic,
  `Task.recur` field family, storage migration `0010_recur_field_family`
  all present) and DR-5/1 merged (PR **#519**, "time tracking domain &
  engine" — the brief's #486 is carried in #519's title; the
  `SetTaskDone` tracking-close block is in the arm). Both are ancestors
  of the branch base. **Not** a shape-widener chunk: no new serialized
  state (successor/roll output rides existing `PersistenceAction`s and
  the `Task.recur` shape DR-6/1 landed); the wave-5 table marks no (w).
- **Branch / worktree**: `feat/series-engine` in
  `../taskboard-WT-feat-series-engine` (off `origin/main` @ `22b5f5d`;
  worktree-doctor passed: 239G free vs 25G floor).
- **Status**: READY FOR IMPLEMENTATION (stage 2 continues in this
  worktree; deviations from this plan need a recorded reason per dispatch
  §1).

---

## 1. Verified anchors (branch base `22b5f5d`, read 2026-10-28)

Numbered facts; the design sections below cite them. Line numbers are
the branch base's.

1. **The planner arm** (`crates/taskboard-domain/src/command.rs:650`):
   `StateCommand::SetTaskDone { id, done }` — guard `live_task` (deleted
   is a rejection; **archived tasks are live and completable**, entities
   `is_live() = !deleted`), idempotence no-op when
   `task.done.is_some() == done`, then the done stamp
   (`updated.done = done.then_some(now)`, `clocks.done = now`), the
   `applied_task` base plan (`UpsertTask` + `LocalOp::UpdateTask`,
   outcome `CommandOutcome::Applied`), then — only when `done` —
   DR-5/1's tracking closes (`task_tracking_closes` + one
   `UpdateAppData(head)` op) and the implicit dismissals
   (`active_notifications` filtered to the head + one
   `RecordReminderDismissal` row each + one `UpdateAppData(head)` op).
   Reopen emits nothing. The spawn block joins **after the dismissals,
   inside the same `Ok(plan)`** — `plan_command` stays pure and total;
   no async, no tick, no IO.
2. **`plan_command` signature** (`command.rs:415`): today
   `(app, outbox, command, ids, defaults, now)` →
   `Result<CommandPlan, CommandError>`. Production caller: the state
   actor only (`taskboard-state/src/actor.rs:614`); every other caller
   is a domain test. **This chunk adds one parameter** (§2 D-E5) — a
   mechanical ripple over the domain test module (~30–40 call sites,
   one-arg each).
3. **The tick arm** (`crates/taskboard-state/src/actor.rs:540`): one
   `tokio::select!` timer arm — `sleep_until(next_tick)`, precondition
   `timer_armed = reminder_cfg.enabled`; on wake,
   `reminder_tick(...)` then re-arm at `reminder_cfg.tick`. The
   disabled-engine policy: the sleep is *never constructed* when
   disarmed ("the paused-time suites stay auto-advance-free").
   `reminder_tick` (`actor.rs:1085`) is the house tick shape: read the
   injected `Clock` once → compute the batch purely → empty ⇒ silent
   return → `repo.apply` → on failure `note_storage_failure` and
   nothing fires → else `core.interpret` + `publish` iff changed, then
   notify. The tick seams tuple is `(repo, clock, system_out, notifier)`
   — **no `ids`**: the roll needs the id generator for outbox ops (§3
   D-E9).
4. **The pure arithmetic exists** (`crates/taskboard-domain/src/recur.rs`,
   DR-6/1): `RecurRule { series, anchor, pattern, bound, continued_from }`;
   `RecurBound::{None, Until(NaiveDate), Count(OccurrenceCount)}`;
   `next_after(pattern, anchor, from) -> NaiveDate` (total, strictly
   after); `roll_target(pattern, anchor, stale, today) -> NaiveDate`
   (catch-up to the latest member in `(stale, today]`, else the first
   member `> stale`; **bound checks are the caller's** — its doc says
   so); `schedule`/`occurs_on`/`occurrence_outcome` (Until filters,
   Count does not — engine-evaluated). Proptests 1–5 ride DR-6/1; none
   of this chunk's work re-opens the arithmetic.
5. **Reminder machinery** (`reminders.rs`): rules are
   `ReminderRule { id, task, offset, created_at, updated_at }` in
   `app.reminders.rules` (BTreeMap, id-ordered); suppression keys are
   `(reminder, occurrence)` with `occurrence = DateTime<Utc>` — a moved
   due date re-arms by construction. The upsert shape to replicate
   (`command.rs:1494`, the `AddReminderRule` arm):
   `PersistenceAction::UpsertReminderRule` + one
   `enqueue(LocalOp::UpdateAppData(task))` flush trigger; the emitted
   rows sync only when the watched task's document group forms. Muted
   state is `app.reminders.muted: BTreeSet<TaskId>`; the mute verb
   (`command.rs:1428`) emits **only** `SetTaskRemindersMuted(task, bool)`
   — no outbox op (device-local by design).
6. **Labels are the OR-set now** (wave 4, #532, merged after the design
   round was written): `Task.label_memberships:
   BTreeMap<LabelId, MembershipState>` with
   `MembershipState::{Added{at}, Removed{at}}` (removals remembered);
   `Task::live_labels()` is the read shape. The `AssignLabel` verb
   (`command.rs:977`) stamps `Added { at: now }` into the map and
   enqueues a dedicated `LocalOp::AssignLabel(task, label)` —
   memberships roam per-pair, and a freshly created card starts
   label-less, so **copied labels need their own ops** (§2 D-E6).
   `TaskClocks` no longer has a `labels` slot (`all_task_clocks`,
   `command.rs:1971`, is the newborn-clock source).
7. **Canonical order** (`state.rs:473`): `order` asc, tie-break id asc.
   `tasks_in_stack` filters `is_live() && stack == stack` — archived
   and done tasks still occupy order slots. The UI's current move flow
   only appends (`append_order` = max+1, `view_state.rs:263`); **no
   midpoint convention exists in the codebase yet** — the design's
   "the UI's own drag convention" names the convention this chunk
   defines for the spawn (and D-C13's later habit drags reuse it).
8. **Storage round-trips everything this chunk writes**: migration
   `0010_recur_field_family.sql` (recur + ck_recur on `tasks`),
   `0012_task_label_memberships.sql` (memberships); codec
   `recur_to_text`/`recur_from_text` (`storage-sqlite/src/codec.rs:75`).
   **No storage work in this chunk.** (Ladder note: recur took
   migration 0010 at its actual landing; the dispatch §2 ladder's
   "next free at rebase, record deviations" clause already covers the
   shift — chunk DR-6/3 takes the next free number, now 0013.)
9. **The day-window vocabulary** (`days.rs`): `window_start(date,
   day_start_time, offset)`, `day_window(now, day_start_time, offset)`
   (the consumer contract D-F6: app bootstrap parses
   `[preferences] day_start_time` and resolves the device's fixed UTC
   offset once; consumers receive the pair explicitly — "later engine
   features via bootstrap injection"). The app-side derivation pair
   exists: `resolve_day_start` (`taskboard-app/src/ui.rs:250`), used by
   the UI hand-over today.
10. **Test harness** (`taskboard-state/tests/`): `common::FixedClock`
    (logical clock, `advance(secs)`), `CountingIds`,
    `InMemoryRepository`; the S-series insta corpus
    (`tests/snapshots.rs` + `tests/snapshots/snapshots__s*_*.snap`) ends
    at **s36a/b**; scenarios boot the live engine with
    `ReminderEngineConfig::disabled()` (timer disarmed); tick tests opt
    in with tiny tick durations + `tokio::time::advance` under paused
    time (engine.rs:2807 pattern) — never sleeps-as-waits. ADRs run
    through **0017**; this chunk writes **0018** (org-mode, the 0017
    shape: title/Date/Status/Depends + Context/Decision/Consequences).

## 2. The completion spawn (origin §3.3, inside `plan_command`)

### D-E1 — Placement and eligibility

The spawn block sits in the `SetTaskDone` arm after the dismissals
block, gated on `done == true` (completing direction only; reopen never
spawns and never clears) and on `let Some(rule) = &task.recur` (a plain
task's `SetTaskDone` plan is **byte-identical to today's** — the
existing suite is the pin). No additional liveness gate: an archived
head is completable today and spawns like any other (its successor is a
fresh, unarchived task — archiving is not copied; recorded D-E9). The
block is a named private helper
(`fn completion_spawn(app, head, rule, day, ids, now) -> Vec<PersistenceAction>`)
so the arm body stays at one abstraction level (dispatch code-quality
mandate).

### D-E2 — Bound checks and the terminal rule

One helper `fn bound_admits(rule, app, completions_after) -> bool` used
by the spawn (and, for `Until`, mirrored in the roll, §3):

- **`Until(u)`**: compute the successor's member date first
  (§2 D-E4's reference chain feeding `next_after`); suppressed iff
  `next_date > u` (inclusive bound — a member *on* the until date still
  spawns/rolls, matching `schedule`'s `d <= u` filter).
- **`Count(n)`**: the count is derived, never stored:
  `count_done(series) = app.tasks.values().filter(|t| t.recur.series ==
  series && t.done.is_some()).count()` — **over all rows including
  soft-deleted ones** (a completion happened; deleting the row later
  must not extend the quota; history outlives deletion is the house
  property). The completing head itself is open pre-plan, so the count
  *including this completion* is `count_done(series) + 1`. Suppressed
  iff `count_done(series) + 1 >= n.get()`. (`Count(1)` series end on
  their first completion.)
- **The terminal rule (origin §3.7 verbatim)**: suppression happens at
  the *completion* — the head still completes (done stamp, closes,
  dismissals, all unchanged) and the same batch additionally stamps
  `updated.recur = None` with a fresh `clocks.recur = now` plus one
  `enqueue(LocalOp::UpdateAppData(head))` op (recur is an appdata-lane
  field — the UpdateTask arm's lane rule). The series ends as an
  ordinary standalone task.
- **Roll-path suppression writes nothing** (§3 D-E11): a stale head
  whose `roll_target` exceeds `Until` (or whose derived count is
  already exhausted — unreachable in practice, total by construction)
  keeps `recur` and stays stale; the rule clears only at its terminal
  completion. Accepted consequence: a bound-exhausted open head shows
  as overdue-recurring until the user completes or deletes it; the
  escape hatches are `UpdateTask` recur-clear ("stop repeating") and
  `DeleteTask` (both exist).

### D-E3 — Successor field inventory (every field, its source)

| Field | Source |
|---|---|
| `id` | `ids.new_task_id()` — fresh (UUIDv7 production) |
| `remote`, `remote_seen`, `remote_baseline` | `None` — newborn local task (the `CreateTask` arm's shape) |
| `title`, `description` | copied from the head |
| `stack` | copied (the head's stack id, verbatim) |
| `order` | the midpoint rule (§2 D-E4) |
| `done` | `None` — the successor is open |
| `scheduled` | if the head has `scheduled`: `calendar.instant(next_date, head_scheduled_local_tod)`; else `None` (mirror) |
| `duedate` | if the head has `duedate`: `calendar.instant(next_date, head_duedate_local_tod)`; else `None` (mirror) |
| `label_memberships` | the head's **live** labels only, re-stamped: `Added { at: now }` (the head's `Removed` tombstones are its own history; a fresh task never referenced those labels) |
| `effort`, `importance`, `urgency` | copied from the head (see the deviation note below) |
| `recur` | the rule copied verbatim — same `series`, same `anchor`, same `pattern`, same `bound`, same `continued_from` |
| `archived` | `false` |
| `deleted` | `false` |
| `clocks` | `all_task_clocks(now)` — every slot at the spawn instant (newborn; DR-6/1's `TaskClocks` has no labels slot) |

*Deviation note (recorded reason)*: origin §3.3's copy list names
title/description/labels/stack only. `effort`/`importance`/`urgency`
are copied anyway: they are the appdata-lane content decorations of the
same class as labels ("the head's content carries to the successor"),
the round's list predates the labels OR-set rework that reshaped the
field family, and not copying them would silently degrade every second
occurrence. `archived: false` is the newborn rule, not an omission.

### D-E4 — Insertion order: the midpoint rule (the amended ruling)

Define `next_order` over the head's stack, candidates =
`is_live() && stack == head.stack` (archived and done included — they
occupy slots; the `tasks_in_stack` filter), the task with the least
`order > head.order`:

1. No candidate (head is last) ⇒ `order = head.order + 1` — the
   append slot is free by definition of "last".
2. Candidate at `next.order` with `gap = next.order − head.order`:
   - `gap == 1` (adjacent): `order = head.order` — a tie **with the
     head only**, never with a third task (all orders strictly between
     two adjacent tasks' orders are free). The canonical
     `(order, id)` sort plus UUIDv7's time-ordered minting resolves
     the tie with the successor rendering immediately after the head;
     when the head is later completed it leaves the open lists and the
     tie vanishes.
   - `gap >= 2`: `order = head.order + gap / 2` (integer floor) —
     strictly inside the free interval, so a fresh free slot.

Invariant pinned by a planner test: the successor's order is either a
free slot or ties *only* with the head. The helper is a pure domain
function (`fn insertion_order_after(app, head) -> i64`), the single
source the later habit-drag work (D-C13) reuses.

### D-E5 — Date conventions (the open question this plan resolves)

The design says "converted through the day-window offset" without
fixing time-of-day or the reference-date flavor. Recorded decisions:

- **`plan_command` grows one parameter**:
  `day: DayCalendar` (new, `days.rs`) — a `Copy` struct
  `{ day_start_time: NaiveTime, offset: FixedOffset }` with three
  pure methods: `today(now) -> NaiveDate` (the **window-date** of now —
  the date whose `day_window` contains `now`; the night-owl
  `03:00 is still yesterday` rule), `calendar_date(t) -> NaiveDate`
  (the **plain offset-calendar date** of an instant), and
  `instant(date, tod) -> DateTime<Utc>` (offset-local → UTC; a fixed
  offset has no transitions, so the conversion is total). `DayCalendar`
  is `days.rs`'s vocabulary made explicit — no second timezone concept
  (binding note), no A1 mint (no invariant to guard: any
  `(NaiveTime, FixedOffset)` pair is well-formed).
- **Field references use plain calendar dates**:
  `series_reference_date(head) = calendar_date(head.scheduled)`, else
  `calendar_date(head.duedate)`, else the completion day
  (`day.today(now)`). Plain dates make the spawn drift-free: a
  03:00-scheduled head references the date its instant literally sits
  on, and the successor's own reference reproduces it exactly — no
  generation-to-generation backward creep, which the window-date flavor
  would introduce for pre-`day_start_time` times.
- **"Today" is the window-date** (`day.today(now)`), everywhere a
  decision needs the current local day — the spawn's dateless fallback
  and the roll's stale comparison. This is origin §3.4's "(per
  day_window)" mandate verbatim.
- **Time-of-day is preserved per field**: the successor's
  `scheduled`/`duedate` keep their head field's local time-of-day at
  the new member date (a 09:00 chore recurs at 09:00; reminder anchors
  stay stable). The successor **mirrors the head's population**: a
  head with only `duedate` spawns a due-only successor; a head with
  neither spawns a dateless successor (its spawn reference was the
  completion day; at *its* completion the same fallback applies — the
  series survives with no stored dates, exactly as the user kept it).
  A dateless head is never stale (§3): nothing anchors it to a day.

### D-E6 — The spawn batch (ordered action layout)

Batch order follows the house rule — entity upserts first, ops after
(the `CaptureTask` comment's contract; persistence applies in order):

1. `UpsertTask(head)` — done stamp etc. (the existing `applied_task`
   actions, unchanged);
2. tracking closes + their `UpdateAppData(head)` op (unchanged);
3. dismissal rows + their `UpdateAppData(head)` op (unchanged);
4. *(bound-suppressed only, D-E2)* — the head's `recur → None` is part
   of action 1's upsert (the helper mutates the updated head before the
   plan is assembled), plus one `UpdateAppData(head)` op here;
5. `UpsertTask(successor)` — the D-E3 inventory;
6. `enqueue(LocalOp::CreateTask(successor))`;
7. one `enqueue(LocalOp::AssignLabel(successor, label))` per live head
   label, label-id ascending (freshly created cards start label-less —
   the ops are what make the labels roam; mirrors the `AssignLabel`
   verb's own emission);
8. one `UpsertReminderRule` per head rule, rule-id ascending —
   `ReminderRule { id: ids.new_reminder_rule_id(), task: successor,
   offset: copied, created_at: now, updated_at: now }` (the
   `AddReminderRule` arm's shape);
9. one `enqueue(LocalOp::UpdateAppData(successor))` — the single
   document-group trigger covering the successor's `recur` and its new
   rules (coalescing is push-side; one op is the flush trigger, the
   `AddReminderRule` op's exact role);
10. `SetTaskRemindersMuted(successor, true)` iff
    `app.reminders.muted.contains(&head)` (mute replication; device-
    local like the verb — no op).

**Delivery/dismissal/snooze history does not copy** — there is nothing
to copy: the successor's reminders are new `(reminder, occurrence)`
keys (new rule ids, new occurrence instants), so they re-arm by
construction (fact 5). Adhoc alarms are *not* replicated (origin §3.3
replicates rules only; a one-shot timer was the head occurrence's
affair — recorded in §6 non-goals).

### D-E7 — Outcome and receipts unchanged

`CommandOutcome::Applied` stays. No new outcome variant, no IPC wire
change, no `StateCommand` change (the command-surface lane's *shape*
duty was DR-6/1's). The CLI's future "spawned `successor`" UX is chunk
DR-6/4's surface concern; the undo round reads the plan's actions, not
the receipt (§5.3 note recorded).

## 3. The day-boundary roll (origin §3.4)

### D-E8 — Config carrier and the shared wake

- New `DayRollConfig` in `taskboard-state/src/config.rs`, mirroring the
  `ReminderEngineConfig` pattern:
  `DayRollConfig { enabled: bool, calendar: DayCalendar }` plus
  `DayRollConfig::disabled()` — the state crate's test-helper default
  (the auto-advance policy: a disabled engine constructs no sleep).
  There is **no user-facing knob**: the roll is D-C1's core
  materialization feature and cannot be coupled to `[reminders]
  enabled` (a kiosk with reminders off still needs its series to
  appear); anti-configurability (D-C8's finding) rules out a new
  switch. Production boots it `enabled: true` from
  `AppConfig::day_calendar()` (new, `config.rs`: the validated
  `preferences.day_start_time` + the device's fixed offset at boot —
  the `resolve_day_start` convention; the UI keeps its own
  `resolve_day_start`).
- `spawn_state_engine` gains the `day_roll: DayRollConfig` parameter
  (mechanical ripple: bootstrap + every test constructor passes
  `DayRollConfig::disabled()` except the new tick tests).
- The timer arm's precondition widens to
  `reminder_cfg.enabled || day_roll.enabled`; on wake each tick runs
  under its own flag, then `next_tick` re-arms at `reminder_cfg.tick`.
  *Accepted coupling*: an engine with reminders disabled but the roll
  armed wakes on `[reminders] tick_secs` — one clock read + date
  compare per wake, invisible at kiosk scale; the watermark (D-E9)
  bounds the real work to one scan per local day.

### D-E9 — Watermark mechanics

`run_loop` holds `let mut last_roll: Option<NaiveDate> = None` —
engine-internal edge state exactly like `storage_failing`; **never
persisted as user state** (a restart re-scans once, which is the boot
catch-up path and is idempotent). `day_tick` shape (mirrors
`reminder_tick`, `actor.rs:1085`):

1. `let now = clock.now()` — the clock is read once per tick;
2. `let today = cfg.calendar.today(now)`; if
   `last_roll == Some(today)` → return (the quiet fast path: nothing
   computed, nothing written, nothing published);
3. compute the batch purely:
   `plan_day_rolls(&core.app(), today, &cfg.calendar, ids, now)` (§3
   D-E10/D-E11) — note the **tick seams grow `ids`** for the outbox
   ops;
4. empty batch ⇒ set `*last_roll = Some(today)` and return — a quiet
   day is a full no-op (no repo write, no publish, no signal; the
   watermark advance is engine bookkeeping, not user state);
5. `repo.apply(batch)` → on failure: `note_storage_failure(...)` and
   return **without advancing the watermark** (the next tick retries
   the same rollover — durable-then-advance, the reminder_tick permit
   rule);
6. else `core.interpret` + `publish` iff changed, then
   `*last_roll = Some(today)`.

Restart-mid-day: the watermark is `None`, the scan runs once, already-
rolled heads are no longer stale (their plain date ≥ today, or they are
future-dated), so catch-up converges in one pass — "rolls once, not per
tick" is a *property of the stale predicate*, not of persisted state.

### D-E10 — The stale predicate

A head is stale iff all of:

- `is_live()` (not deleted; archived included, as in the spawn), and
- `is_open()` (`done: None`), and
- `task.recur` is `Some(rule)`, and
- its **reference date** (the D-E5 chain: plain calendar date of
  `scheduled`, else of `duedate`; a dateless head is never stale) is
  strictly before `today` (the window-date of the tick's clock read).

*Refinement over the origin text (recorded reason)*: §3.4 names only
"scheduled local date"; the due-only series is the evident intent
(its spawn already uses the duedate fallback, and "never force the user
to mark yesterday's task done" does not care which date field carries
the day). The dateless-never-stale rule keeps the predicate total.

Iteration order: `app.tasks` is a BTreeMap — task-id ascending, one
roll per stale head, deterministic per (state, today).

### D-E11 — The roll batch

Per stale head, in task-id order:

- `target = roll_target(&rule.pattern, rule.anchor, reference_date,
  today)` — the pure law, unchanged (DR-6/1's proptests own it);
- **bound suppression**: if `rule.bound` is `Until(u)` and
  `target > u`, skip the head entirely (no write, no clear — D-E2's
  terminal rule puts the clear at the completion); a `Count(n)` head
  with derived completions already ≥ n is skipped by the same helper
  (unreachable, total);
- the roll is an **ordinary field edit**: clone the task;
  `scheduled`, if present, moves to `calendar.instant(target, its
  preserved local tod)` with fresh `clocks.scheduled = now`; `duedate`
  likewise with `clocks.duedate = now`; missed intermediate days are
  skipped by `roll_target`'s catch-up — absence = missed (D-C2),
  nothing spawns, nothing is recorded;
- ops mirror the `UpdateTask` arm's lane split: the head's board
  features (`board_features`, the planner's own walk) decide —
  `duedate` moved ⇒ always `LocalOp::UpdateTask`; `scheduled` moved on
  a board without `card_startdate` ⇒ additionally
  `LocalOp::UpdateAppData(head)` (D-V2's conservative routing; a mixed
  batch enqueues both).

Batch layout: all `UpsertTask`s (task-id ascending) first, then their
enqueues in the same order. Determinism: two devices rolling the same
head on the same local day produce identical targets (the pure law);
LWW makes the race benign (same-value writes) — origin §3.4's
convergent property, unchanged.

Reminder interplay: the moved due date is a new occurrence key, so
due-anchored rules re-arm for free (fact 5) — pinned by test, no
reminder code (origin §3.8: no reminder changes at all).

## 4. Interplays

- **Undo (origin §5.3, recorded for the undo round, not built here)**:
  spawn/roll are ordinary plans; D-U2's pushed-edit rule applies
  unchanged. Inverse synthesis of a completing-with-spawn must be
  *reopen head + `DeleteTask(successor)`* — the delete's own cascade
  removes the successor's replicated reminder rules (`task_rule_deletes`),
  and the undo round must decide the mute row's fate (the mute verb has
  no inverse row today; `SetTaskRemindersMuted(successor, false)` is
  the obvious inverse but the D-U catalogue owns the ruling). The
  completion's implicit dismissals stay non-invertible (house
  precedent), and the roll's field edit inverts as any field edit does.
  The undo/1 stage-1 prompt should be handed this paragraph.
- **Sync**: occurrences roam as plain cards (the `CreateTask` +
  `AssignLabel` + `UpdateTask` ops); the successor's `recur` rides the
  per-task appdata entry (the `UpdateAppData` trigger). The document
  *codec's* recur facts are chunk DR-6/3's ("appdata tasks-row recur
  facts") — until then the trigger op is emitted and flushes with recur
  simply absent from the wire row; local truth (sqlite, migration 0010)
  is already complete. The spawn race's fold-time dedup is likewise
  chunk 3's — this chunk only guarantees the deterministic
  `(series, scheduled-date)` twin shape the fold keys on.
- **Tracking (DR-5/1)**: entries attach per occurrence; the completing
  head's open entries close via the existing block. Zero interplay code.
- **Boot catch-up**: the first armed tick after boot always scans
  (watermark `None`) and rolls every stale head to the current window —
  the offline/kiosk path, one card PUT per stale series, no
  retroactive history (origin §5.2).

## 5. Deliberately not built (origin §3.8 echoed, + this plan's rulings)

No RRULE, no virtual instances, no skip records, no series registry,
no reminder machinery changes (rules replicate through the existing
`UpsertReminderRule`; nothing in `reminders.rs` changes), no
pause/resume, no alarm replication, no new `StateCommand`/`LocalOp`/
`PersistenceAction` variants, no migration, no storage codec change,
no user-facing knob for the roll. The `tasks repeat` CLI verbs and the
today-view badges are chunk DR-6/4's.

## 6. Test catalogue (TDD-first; the stage-2 commit order)

Determinism rules throughout: fixed/stepping clocks (`FixedClock` +
`tokio::time::advance` under paused time for the live-timer tests);
no sleeps-as-waits — deadline-guarded polling or join points; typed
outcome assertions (`matches!`), never text; no quality ceilings; one
behavior, one test, lowest layer.

**Domain planner tests** (`command.rs` tests module; red first):

1. `completing_a_recurring_head_spawns_the_successor` — full D-E3
   inventory item by item (title/description/stack/labels-re-stamped/
   effort/importance/urgency/recur-verbatim/done-None/archived-false/
   clocks-all-`now`/fresh ids), successor scheduled+duedate at the next
   member with preserved times-of-day.
2. `the_successor_lands_immediately_after_the_head` — the three
   midpoint cases: free `+1` slot; `gap ≥ 2` midpoint; `gap == 1` ties
   the head only (assert no third task shares the order, and the
   (order, id) canonical order puts the successor after the head).
3. `until_bound_suppresses_and_clears_at_the_terminal_completion` —
   next member past the until date: no successor, `recur == None`,
   fresh `clocks.recur`, the head still done-stamped with closes/
   dismissals intact; inclusive-bound case (member *on* the until date
   spawns).
4. `count_bound_evaluates_derived_history` — seed N done tasks in the
   series; completing at `N == n-1` spawns, at `N == n` suppresses and
   clears; a soft-deleted done occurrence still counts; no counter
   column anywhere (structural).
5. `reminder_rules_replicate_without_history` — per head rule: fresh
   id, same offset, `created_at == updated_at == now`, `task ==
   successor`; zero delivery/dismissal/snooze rows touch the successor;
   the mute flag replicates; a head with neither rules nor mute emits
   neither.
6. `reopening_never_spawns_and_plain_completion_is_untouched` —
   `done: false` emits nothing extra; a non-recur head's plan equals
   today's actions exactly (the byte-identical pin, structural not
   textual).
7. `dateless_and_dueonly_successors_mirror_the_head` — the D-E5 mirror
   rule incl. the completion-day fallback and the window-date
   computation.
8. `plan_day_rolls` unit tests — stale predicate matrix (open/live/
   recur/reference-date), the roll writes both date fields with fresh
   clocks and preserved tods, Until-suppressed head untouched,
   non-stale/future head untouched, task-id-ascending batch order, the
   lane split with and without `card_startdate`.

**State engine tests** (`tests/engine.rs` + a `DayRollConfig` fixture
in `common`):

9. `rollover_rolls_stale_heads_once` — armed tiny tick + paused time:
   head scheduled yesterday; `tokio::time::advance` past the first
   tick; assert the roll applied (repo + published state), then
   repeated advances within the same local day change nothing
   (watermark fast path).
10. `a_quiet_day_is_a_full_no_op` — no stale heads across a rollover:
    repo state unchanged, no `StateUpdated` signal (deadline-guarded
    receive asserting emptiness), watermark internal (no persisted
    trace).
11. `boot_catchup_rolls_once_not_per_tick` — date jump on the fixed
    clock mid-scenario (advance 3 days): the first tick rolls to the
    catch-up target (`roll_target`'s skip-ahead case for a weekly),
    subsequent ticks are quiet.
12. `storage_failure_defers_the_roll` — a failing fake repo: nothing
    applies, `StorageFailed` fires once, the watermark did not advance;
    healing the repo rolls on the next tick.
13. `the_roll_re_arms_reminders_by_construction` — after a roll, the
    next reminder tick fires the head's rule at the *new* occurrence
    instant (the fact-5 pin).
14. The spawn end-to-end: `SetTaskDone` over the live engine persists
    the whole batch and publishes once (the S-series structural
    memory ≡ disk check covers it; one explicit engine-level assertion
    on the created-successor receipt side, typed).

**Insta scenarios** (`tests/snapshots.rs`; the corpus's next free
numbers are s37+; each adds the persisted dump + published tail pair):

- `s37a` completion spawn, full contents (rules + labels + mute);
- `s37b` Until suppression + terminal clear;
- `s37c` Count final completion;
- `s38a` day-boundary roll of one stale head (timer armed);
- `s38b` quiet-day idempotence (two ticks, one rollover);
- `s38c` boot catch-up over a multi-day gap.

Existing snapshots must not shift at all (day roll and spawn scenarios
opt in via their own fixtures; `ReminderEngineConfig::disabled()`
scenarios never arm the timer).

**Not re-tested here** (one-behavior-one-test, lowest layer):
`next_after`/`roll_target`/`occurs_on` laws (DR-6/1's proptests), the
`UpdateTask` recur arm (DR-6/1), reminder evaluation internals
(`reminders.rs` suite), sqlite round-trip of recur (DR-6/1's codec
tests).

**Stage-2 commit sequence** (each standalone, rebase-merge hygiene):

1. `feat(domain): the completion spawn` — `DayCalendar`, the spawn
   helper, the arm extension, planner tests 1–8;
2. `feat(state): the day-boundary roll tick` — `DayRollConfig`,
   `plan_day_rolls` wiring, watermark, timer widening, engine tests
   9–13 (test 14 rides here or with 1, wherever it compiles
   standalone);
3. `feat(app): the day calendar injection` — `AppConfig::day_calendar`,
   bootstrap param;
4. `test(state): the series insta corpus` — s37–s38 scenarios + snaps;
5. `docs(adr): the recurring series materialization` — ADR 0018
   (§8's draft), final commits.

Gates: the full AGENTS §10 suite through
`scripts/run-quality-gates.sh` (parallel wave-5 worktrees share the
flock), green before handoff.

## 7. Risks

- **`plan_command` signature ripple**: every domain test call site
  gains `day` (~30–40 mechanical edits) — noise, but it must not leak
  into behavior commits.
- **Born-stale successors** (a multi-day-stale head completed mid-day
  spawns a successor dated in the past): it converges at the *next*
  rollover, not instantly — schedule-truth over display comfort, the
  overdue-recurring presentation is chunk DR-6/4's; revisit only on
  evidence (the round's posture). Recorded, accepted.
- **Terminal clear drops the final occurrence's series attribution**:
  after the bound-exhausted completion, the done head no longer carries
  `recur.series`, so derived analysis (streaks, `occurrence_outcome`)
  cannot see that final completion. Origin §3.7 rules the clear
  explicitly ("the series ends as an ordinary standalone task"); the
  analysis surfaces (chunk 4 / follow-ups) must read the ruling the
  same way. Flagged so nobody "fixes" it silently.
- **Offset fixed at boot**: a DST flip mid-run shifts wall-clock labels
  ≤1h — already accepted by the day-window ruling (days.rs doc); the
  roll inherits it.
- **Parallel wave-5 chunks** (`feat/sync-resurrect-contention`,
  `feat/tracking-ui` in flight): disjoint files per the wave table,
  but quality gates must go through the flock wrapper, and the
  `crates/taskboard-WT-feat-tracking-ui/` tree sitting inside the main
  checkout is a stale unregistered copy (the #395 nesting pattern) —
  hygiene flag for the user, untouched by this chunk.

## 8. ADR 0018 draft (stage 2 materializes at `docs/adr/0018-`)

`0018-recurring-series-completion-spawn-day-boundary-roll.org` —
title: *ADR 0018: Recurring Series — Materialise-on-Completion with a
Day-Boundary Roll-Forward*. Date 2026-10-28; Status: Accepted (engine
lands with chunk DR-6/2; storage/sync DR-6/3, surfaces DR-6/4, habits
DR-6/5–6/6 follow); Depends on: ADR 0006 (the interpret loop), ADR 0004
(LWW/sync policy), ADR 0013 (the `(reminder, occurrence)` key
precedent). Context: issue #212, the open head as template, the two
rejected alternatives (display-only roll — two truths for one field;
virtual-instance projection — sync and history lies). Decision,
numbered: (1) completion spawns the successor inside the same pure
plan — successor inventory per this plan's D-E3 (with the
effort/importance/urgency deviation note and the OR-set label
re-stamp); (2) midpoint insertion after the head, the gap-1 tie
resolved by (order, id) over UUIDv7; (3) `Until` is arithmetic,
`Count` derives from completion history — suppression happens at the
completion, which clears the rule (the terminal-transition ruling);
roll-path suppression writes nothing; (4) the day tick beside the
reminder tick — watermark engine-internal and never persisted, quiet
day = full no-op, boot catch-up = the same path; (5) date conventions:
plain calendar dates for field references, window-date for "today",
per-field time-of-day preservation, the scheduled→duedate→completion-
day reference chain; (6) occurrences roam as cards, `recur` rides the
appdata tasks-row, fold dedup is chunk 3's and keys
`(series, scheduled-date)`. Consequences: the born-stale convergence
delay; the terminal attribution drop; daily-series card volume
(accepted, D-C6); no reminder machinery changes.

## 9. Decision record (this plan)

| ID | Fork | Ruling |
|---|---|---|
| D-E1 | Spawn placement | Inside the `SetTaskDone` arm, after dismissals, completing-direction only; helper fn; archived heads spawn |
| D-E2 | Bound terminal rule | Suppression + `recur → None` at the completion; roll-path suppression writes nothing; Count includes the completing head (`count_done + 1 ≥ n`) and counts soft-deleted done rows |
| D-E3 | Successor inventory | Per the table; effort/importance/urgency copied (recorded deviation); archived not copied; Removed label tombstones not copied |
| D-E4 | Insertion order | Free `+1`, else floor midpoint; `gap == 1` ties the head only — (order, id) + UUIDv7 resolves it |
| D-E5 | Date conventions | `DayCalendar` param on `plan_command`; plain calendar dates for references, window-date for "today", time-of-day preserved per field, scheduled→duedate→completion-day chain, dateless mirrors dateless |
| D-E6 | Spawn batch | Entities then ops; per-label `AssignLabel` ops; one `UpdateAppData(successor)` trigger; mute replicates device-locally |
| D-E7 | Receipts | Outcome unchanged; no wire change |
| D-E8 | Tick config | `DayRollConfig` + always-on production (no knob); shared wake, `reminder_cfg.tick` cadence; `spawn_state_engine` +1 param |
| D-E9 | Watermark | `Option<NaiveDate>` loop-local; advance only after a successful apply; restart = one idempotent re-scan |
| D-E10 | Stale predicate | open ∧ live ∧ recur ∧ reference-date (sched→due chain) strictly before window-date-today; dateless never stale |
| D-E11 | Roll batch | `roll_target` verbatim; Until-suppressed skipped silently; lane split per board features; task-id-ascending, entities-then-ops |
| D-E12 | Born-stale successors | Accepted, converge next rollover; presentation is chunk 4's |

## 10. Estimated diff size

~1.4–1.9k lines including tests and generated snaps, across ~14 files:
`days.rs` (+~80), `command.rs` (+~250 production, +~450 tests, signature
ripple), `recur.rs` (0), `config.rs` both crates (+~70), `actor.rs`
(+~110), `bootstrap.rs` (+~10), test `common` (+~50), `snapshots.rs`
(+~200), engine suite (+~300), 12 new `.snap` files, ADR 0018 (+~120).
No migration, no new dependencies, no storage/IPC/UI changes.
