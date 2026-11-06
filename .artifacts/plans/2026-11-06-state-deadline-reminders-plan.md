# [Plan] GUI Review Round 5, Chunk 6 — Deadline-Driven Reminder Firing

- **Date**: 2026-11-06
- **Kind**: stage-1 chunk plan (TDD-first) for dispatch chunk 6 of
  `.artifacts/plans/2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  (§7), executing round plan §4.6 + D2 (normative).
- **Branch**: `feat/state-deadline-reminders` off `main` @ `154167f4`
  (chunk 5 `feat/app-live-settings` verified merged: `git log origin/main`
  shows `bc22bde7..154167f4`; local `main` fast-forwarded, worktree
  `../taskboard-WT-feat-state-deadline-reminders` created after
  `scripts/worktree-doctor.sh` exited 0 — 147G free, floor 25G).
- **Problem (F16, with the premise correction)**: countdowns already
  refresh at 1 Hz UI-side; what lags is **firing**. The engine wakes at a
  fixed cadence (`next_tick = now + tick`, `actor.rs` tick arm), so a due
  reminder fires up to `DEFAULT_REMINDER_TICK_SECS = 30` s late. D2: the
  loop becomes deadline-driven — `sleep_until(min(next due instant,
  now + tick))`, re-armed on every command batch that can change dues —
  and the default tick drops to **1 s** so the tick is a cheap fallback
  sweep, never the firing mechanism.
- **Non-goals** (dispatch §7): the UI countdown cadence (already 1 Hz,
  untouched); the escalation/alarm minute-cadence pass (`taskboard-app/src/escalation.rs`);
  settings UI changes beyond the default/copy; any config-schema change.

---

## 1. Current machinery (read and verified on the branch point)

- `crates/taskboard-state/src/actor.rs` run loop: single
  `tokio::select!` with `biased` order (commands → reports → system →
  timer). `timer_armed = !draining && (day_roll.enabled ||
  core.settings().reminders_enabled)` is re-evaluated per iteration
  (chunk 5). The tick arm runs `reminder_tick` (live
  `reminders_enabled`, live day calendar) then `day_tick`
  (`day_roll.enabled`, the boot-fixed-offset calendar), then re-arms
  `next_tick = now + tick` from the live snapshot. Chunk 5's
  `SettingsApplied` path recomputes `next_tick` at the command arm —
  this chunk replaces that fixed-cadence recompute with a deadline
  recompute.
- `reminder_tick` (actor.rs ~1200): one clock read, live
  `reminder_defaults()` from the snapshot, pure
  `core.due_reminder_firings(...)`, then the **stale-delivery sweep**
  (`taskboard_domain::reminders::stale_deliveries`, issue #201) over
  in-memory state, and `repo.apply(...)` **only when firings ∪ stale is
  non-empty** (durable-then-notify). A quiet tick does zero IO, zero
  sends.
- `crates/taskboard-state/src/core.rs` `due_reminder_firings`
  (~124): zero-clone delegate to `taskboard_domain::due_firings`.
- `crates/taskboard-domain/src/reminders.rs`: `due_firings` (~1400)
  composes five families, all shaped "effective occurrence ≤ now ∧
  ¬delivered ∧ ¬dismissed":
  - task-side (default ref when no explicit rules, else per-rule) via
    `ReminderState::effective_occurrence` → `base_occurrence` +
    `walk_chain` (snoozes) — **now-independent**;
  - `at_rule_firings` (F25 pass) via `at_rule_occurrence` —
    **now-independent**;
  - alarms via `alarm_occurrence` — **now-independent**;
  - `break_firings` via `break_effective_occurrence(.., now)`
    (threshold crossing over tracking entries) — **now-dependent**;
  - `tracker_firings` via `tracker_effective_occurrence(habit, day,
    reminders, now)` (anchor inside the current day window + quota
    gate) — **now- and day-dependent**.
  Output sorted by `(occurrence, reminder)` — a total order.

## 2. Decision 1 — `next_due_instant`'s shape

New pure query in `taskboard-domain/src/reminders.rs`, the exact
mirror of `due_firings`' signature:

```rust
#[must_use]
pub fn next_due_instant(
    tasks: &BTreeMap<TaskId, Task>,
    tracking: &TrackingState,
    habits: &HabitState,
    day: DayCalendar,
    reminders: &ReminderState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>>
```

Semantics: the earliest instant `t ≥ now` at which `due_firings(.., t)`
can become non-empty, **or `Some(now)` when `due_firings(.., now)` is
already non-empty**, or `None` when no candidate exists (the engine
then rides the tick cadence alone). Construction (each filter mirrors
the family's gate in `due_firings` — one definition, the proptest is
the enforcement):

1. If `due_firings(...)` at `now` is non-empty → `Some(now)` (fire
   immediately; this also preserves today's boot behaviour — an
   undelivered reminder from before a reboot fires on the first loop
   pass, because the boot deadline computes to `now`).
2. Otherwise collect candidates — for every `(reminder, occurrence)`
   pair the firing evaluation could consult, with the same
   eligibility gates (remindable/unmuted task, live/open, alarm-map
   membership, live anchored habit, threshold-crossing existence) —
   and keep only pairs where `occurrence > now` **and**
   `!is_delivered(reminder, occurrence)` **and**
   `!is_dismissed(reminder, occurrence)` (delivery receipts are
   keyed by the effective occurrence and only change via engine
   writes, so the predicate is stable between wakes).
3. Add the **next day-boundary instant** as a floor candidate: the
   tracker pass's window and quota gate roll at the boundary, so a
   nudge can become due exactly there without any occurrence
   crossing. Computed from the live calendar (`day` +
   `day_start_time`); a small domain helper (`next_day_boundary(day,
   now)`) is added beside `DayCalendar` if no reader exists yet.
4. Return `min(candidates)` — `None` if the set is empty.

Where family occurrence reads are now-dependent (break, tracker),
they are evaluated **at `now`**; correctness across their future
shifts is guaranteed by the day-boundary floor plus P1 of the
proptest below, and any residual divergence self-heals within the
1 s tick (Decision 5's stance: the deadline is a latency
optimization; the tick is the correctness net).

The engine never calls `next_due_instant` directly on `AppState`
maps — it goes through an `EngineCore` capsule method beside
`due_reminder_firings` (same zero-clone delegate discipline, core
stays a state capsule, brain in the domain).

## 3. Decision 2 — the proptest tie (the two must never disagree)

Property tests in `taskboard-domain` (the same `proptest` grammar the
`plan_command` suites at `command.rs:10818+` use), over a shared
arbitrary-state generator (tasks incl. dues/live/open/mute, rules of
every kind, alarms, break rules + tracking entries, habits with
anchors/log, snoozes, delivery/dismissal receipts, arbitrary `now`,
day-start offsets):

- **P1 — never late (soundness)**: given
  `due_firings(now)` empty and `next = next_due_instant(...)`,
  for every sampled `t` in `[now, next)`:
  `due_firings(.., t)` is empty. Sampling: uniform instants in the
  half-open range plus probes at `next - 1 s` and `next` itself.
  This is the direction that matters: waking at `next` can never
  miss a firing that was due earlier.
- **P2 — prompt responsiveness**: if `due_firings(now)` is
  non-empty then `next_due_instant(...) == Some(now)`.
- **P3 — tightness with the day exception**: for every generated
  state, `next` is either the day-boundary instant or
  `due_firings(.., next)` is non-empty. A `next` that is both
  past the day boundary *and* quiet would be a divergence bug.

Unit tests pin the seam cases proptests under-sample: past-due
clamp (`Some(now)`), snooze-walked occurrences (the deadline follows
the chain), delivered/dismissed occurrences excluded, day-boundary
wake with a met prior-window quota, `None` on an all-quiet state,
mute/unmute and done/open flips flipping the answer.

## 4. Decision 3 — the re-arm list (exhaustive)

The loop recomputes the deadline from post-command state. For review
cross-checking, the exhaustive list of `CommandOutcome` variants that
can change a due, a rule, an eligibility input, or the cadence —
each is a re-arm:

`CreatedTask` (new due → implicit default), `Applied` (task edit:
dues, done/open, archive/live), `ReminderCleared`,
`ReminderRuleAdded { added: true }`, `ReminderRuleRemoved`,
`AdhocAlarmCreated`, `AdhocAlarmCancelled`, `BreakRuleCreated`,
`BreakRuleDeleted`, `TrackingStarted`, `TrackingStopped`,
`TimeLogged`, `EntryAdjusted`, `EntryDeleted` (break crossings read
entries), `HabitCreated`, `HabitLogged`, `HabitLogAdjusted` (tracker
quota/anchor), `ReminderMuted` (unmute un-suppresses), `Bound`,
`Unbound`, `Reconciled` (merges change dues/refs), `Undone`,
`Redone`, `TaskRestored` (journal inverses restore dues/rules),
`SettingsApplied` (tick, enabled, defaults, day-start — chunk 5's
existing re-arm point, now deadline-shaped).

**Not re-arms**: `NoOp`, `SyncRequested`, `ReminderRuleAdded {
added: false }` (dedup survivor — nothing changed), and every
rejected command (`Err` reply). **Also a re-arm, outside
`CommandOutcome`: every ingested `SyncReport`** — `merge_task` can
change dues and the sidecar folds (`fold_reminder_rules`,
`fold_reminder_dismissals`) change rules/receipts.

**Implementation posture**: rather than an allowlist enum-match in
the loop (a new outcome variant silently missing the list is the
missed-firing bug class), `execute_command`'s command arm recomputes
the deadline after **every** executed command and every ingested
report. Recompute is pure, in-memory, microseconds. The list above
is then the stage-3 review checklist (a variant that must *not*
re-arm would have to argue why recomputing harms it — none can, the
recompute is idempotent), and the exhaustive-list acceptance test is
a paused-clock journey per *group*: boot, a due edit, a rule add, an
alarm cancel, a tracking stop, a habit log, an unbind, an undo, a
sync report, a settings change (dispatch §7's minimum is a subset of
this).

## 5. Decision 4 — loop shape, first tick, day roll, no busy loop

Single timer arm, deadline-shaped (`next_wake` replaces
`next_tick`):

- **Boot**: `next_wake = min(now + tick, deadline)` with deadline
  from `next_due_instant` on the freshly loaded state — today's
  immediate first tick falls out (any due reminder pins the deadline
  to `now`; a quiet boot rides `now + tick`).
- **Wake**: run `reminder_tick` (if reminders enabled) and
  `day_tick` (if the roll is enabled) exactly as today, then
  `next_wake = min(now + tick, next_due_instant(post-tick state))`.
  The `now + tick` floor after every wake is the **anti-busy-loop
  clamp**: a deadline in the past wakes the select once (tokio
  timers fire immediately when the instant has passed), the tick
  floor then pushes the next wake at least one tick out — a
  permanently-unresolvable deadline degrades to exactly today's
  cadence, never a spin.
- **Day roll**: the day-boundary candidate (Decision 1 §3) keeps
  `day_tick` landing at the boundary even when the reminder deadline
  is far away. The boundary is recomputed per wake because
  `day_start_time` is live settings (chunk 5). The roll's watermark
  semantics (`last_roll`) are untouched — still exactly-once per
  boundary; paused-clock test pins the roll fires once across a
  boundary with reminders disabled.
- **Drain / disable**: `timer_armed` semantics unchanged (drain
  disarms; a fully-disabled engine constructs no sleep — the
  paused-time auto-advance-free suites stay intact). Deadline
  computation is skipped entirely while disarmed.
- Chunk 5's `settings_applied` re-arm point in the command arm
  becomes the general deadline recompute (Decision 4's posture).

## 6. Decision 5 — sweep gating: reasoned, recorded, no gating lands

D2 moves the stale-delivery sweep off a 1 s tick "if it touches the
repo every second"; round plan §8 makes it "measured in-chunk".
Measured-by-construction, recorded here as the audit note:

- The sweep is `stale_deliveries(...)`: a **pure** in-memory scan
  (tasks, tracking, habits, reminder rows) — no `repo` handle, no
  channel, no allocation beyond the prune set, and
  `repo.apply` runs **only when firings ∪ stale is non-empty**
  (`reminder_tick`'s early return). The idle per-tick path is
  exactly one clock read + `due_firings` + `stale_deliveries`
  + a `min()` — zero IO, zero sends, at any tick rate.
- The stage-2 run records a cheap measurement (a release-mode timed
  loop of `due_firings` + `stale_deliveries` over a synthetic
  1 000-task / 1 000-rule / full-receipt state; numbers written into
  this plan file as a follow-up note, **not** a committed timing
  assertion — wall-clock asserts are the flake class AGENTS §5
  bans). Expected order: low microseconds per evaluation.
- Decision: **the sweep stays on every tick; no gating lands.** If
  the measurement surprises (milliseconds, not microseconds), the
  fallback (a modulo-60 tick counter gate, sweep at ≤ 1/minute,
  unchanged journey coverage) is pre-authorized here and the
  deviation is reported.

## 7. Decision 6 — default 30 → 1 and the copy sites

- `crates/taskboard-app/src/config.rs:57`
  `DEFAULT_REMINDER_TICK_SECS: u64 = 30` → `1` (doc comment updated:
  the tick is the fallback sweep; firing is deadline-driven).
- **Domain default split (the discovered trap)**: chunk 5's
  `taskboard-domain/src/settings.rs` defines a single
  `DEFAULT_SECS = 30` as `TickSecs::default()` — shared by
  `[reminders] tick_secs` **and** `[sync] poll_interval_secs`
  (the serde-leniency target). The sync lane must stay 30. Split:
  `DEFAULT_TICK_SECS = 1` (reminder lane: snapshot default + tick
  leniency) and `DEFAULT_POLL_INTERVAL_SECS = 30`; `TickSecs::default`
  remains the sync lane's, the snapshot's tick field is minted
  explicitly. Chunk-5 tests asserting the tick default
  (`config.rs` defaults/snapshot rounds, `settings.rs` decode
  leniency) update 30 → 1 — churn is expected, none of those
  assertions are weakened (the 30-claims about *sync* stay).
- Copy sweep (the only places the number appears): the generated
  example config in `config.rs` (~line 1985, `tick_secs = 30` → 1);
  `settings_overlay.rs` test fixtures keep their literal 30/0 (they
  seed specific files, not the default). The settings UI's
  `HELP_TICK` ("How often the reminder engine checks, in whole
  seconds.") carries no number — unchanged; the field echoes the
  live value, there is no placeholder literal. A grep sweep for
  `tick_secs = 30` / "30 s" near tick closes the tail
  (`docs/architecture.org` engine section, below).
- **Docs** (dispatch §10 bookkeeping): `docs/architecture.org`'s
  tick-arm paragraph (~line 404) is amended: the wake is
  `sleep_until(min(next due instant, now + tick))`, deadline-driven
  with the fixed 1 s tick as the fallback sweep; the
  `[reminders]` settings-table row (if it cites the cadence) follows.

## 8. TDD stage order (stage 2's build order)

1. **Domain red**: `next_due_instant` unit tests + the P1/P2/P3
   proptest — written first, red (function absent).
2. **Domain green**: the query + `EngineCore` capsule method +
   `next_day_boundary` helper; proptest and units green.
3. **State red**: paused-clock loop tests in
   `crates/taskboard-state/tests/` (new
   `reminders_deadline.rs` or beside the existing reminder suites):
   fire **at** the due instant (advance exactly to it, not to the
   next tick) after boot; re-fire-latency journeys after a due edit
   at an arbitrary offset, a rule add, an alarm cancel, an unbind,
   an undo, a sync report, a settings change; tick stays
   live-adjustable through chunk 5's path; day-roll fires exactly
   once at the boundary with reminders off; the past-deadline clamp
   (advance 1 s repeatedly — bounded wake count asserted via the
   metrics counter, no busy spin); drain/disable disarm unchanged.
4. **State green**: the loop reshape (Decision 5), recompute on
   every command/report (Decision 4's posture).
5. **Audit**: the measurement run (Decision 6) — note added to this
   file in the implementation branch commit.
6. **Defaults/copy/docs** (Decision 7) with the 30 → 1 test churn.
7. Full gate suite via `scripts/run-quality-gates.sh`.

Test discipline: `tokio::time::pause()`/`advance()` only — no sleeps;
assert typed outcomes and structure (firing receipt rows, metrics
counters), never log/label text; each behavior one test at the
lowest layer (the pure query's properties live in the domain, the
loop tests own only routing/latency); no quality ceilings (the
anti-spin test bounds *pathological* wakes, which is a garbage-input
ceiling, not a success ceiling).

## 9. Risks

- **Divergence between the query and the evaluator** (the
  now-dependent break/tracker families are the sharp edge):
  contained by P1/P2/P3 against the *same* generator, by the
  day-boundary floor, and — ultimately — by the 1 s tick fallback:
  a missed deadline costs ≤ 1 s of latency, never a lost firing
  (the tick still evaluates every family). This layered stance is
  the recorded mitigation; the deadline machinery must never become
  load-bearing for correctness.
- **Chunk-5 test churn** from the domain default split (30 → 1 for
  the reminder lane): expected, bounded to the tick-asserting
  rounds; sync-lane 30s stay.
- **`next_day_boundary` freshness**: the boundary must be
  recomputed from the live snapshot each wake (day-start is live);
  a stale mint would skew the tracker wake by up to a day — the
  day-roll journey pins it.
- **Busy-loop regressions** if a re-arm forgets the tick floor:
  the clamp journey (bounded wakes under a stuck deadline) is the
  pin.
- **1 s default multiplies engine wakeups** on the slow path too
  (a firing batch every second while a reminder is overdue and its
  delivery keeps failing): pre-existing behaviour shape (the tick
  already retried per tick), now 30× more often; the durable-then-
  notify failure path is idempotent, and the storage-failure edge
  trigger (`note_storage_failure`) is transition-gated — accepted.

## 10. Estimated diff size

~450–550 lines: domain query + helper + proptest + units ≈ 250;
`EngineCore` delegate ≈ 15; loop reshape + recompute points ≈ 60;
state paused-clock suites ≈ 180; default/copy/docs ≈ 40 (net of
churn). One branch, commits per §8 stage (6–8, each standalone).
