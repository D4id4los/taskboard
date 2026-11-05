# [Plan] v0.2 Chunk 7 — the tracker reminders (#504)

- **Date**: 2026-11-05
- **Kind**: implementation plan (stage-1 of the three-stage dispatch
  protocol; TDD-first, no production code in this commit). Lineage:
  issue **#504** (its seam list is normative input) ← DR-6 rulings
  **D-C11** (tracker pings = named follow-up) and **D-C13**
  (`anchor_time` is decoration + the F-1 nudge anchor, never a sort
  key) in
  `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §4/§6, follow-up **F-1** (§7); the v0.2 implementation plan §5.7 with
  verified facts **VF-2/VF-4/VF-5/VF-7/VF-10**
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`);
  the break-rule round's occurrence-key law **D-8** ("a duration
  trigger's occurrence is the threshold-crossing instant — stable while
  history is stable, never the tick stamp")
  `.artifacts/plans/2026-10-31-respite-break-reminders-plan.md` §4; the
  dispatch chunk brief
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md` §8.
- **Branch / worktree**: `feat/tracker-reminders` in
  `../taskboard-WT-feat-tracker-reminders` (off `main` @ `58ae9c55`;
  doctor passed 2026-11-05, 219 G free vs 25 G floor).
- **Status**: PLAN — stage-2 implements exactly this; deviations are
  findings, recorded in the stage-2/3 reports.

---

## 0. Findings → mechanisms audit (verified on the branch's base 2026-11-05)

Numbered facts; every decision below cites them.

1. **`ReminderRef` has no `Tracker` arm yet**
   (`crates/taskboard-domain/src/reminders.rs:318`): today
   `Rule | Default(TaskId) | Adhoc(AdhocAlarmId) | Break(BreakRuleId)`.
   The `Break` arm is the direct precedent for a reminder family whose
   row lives outside `ReminderState.rules`: it carries a
   `text_form` prefix (`"break:<uuid>"`), a dedicated pure pass
   (`break_firings`, reminders.rs:1264), a dedicated
   effective-occurrence reader (`break_effective_occurrence`,
   reminders.rs:1192) with a totality-`None` arm in `base_occurrence`
   (reminders.rs:1184), a `stale_deliveries` unconsulted arm
   (reminders.rs:991), a `notifications_for` payload arm that skips on
   a missing row (reminders.rs:807 — the documented totality rule),
   and a planner-cascade receipt prune (`remove_break_rule`,
   reminders.rs:874). The tracker arm clones this shape end to end.

2. **The evaluator is a pass composition**
   (`due_firings` reminders.rs:1303, `active_notifications`
   reminders.rs:1382): task pass + alarms pass + break pass +
   `At` pass, concatenated then sorted by the total order
   `(occurrence, reminder)`. Every pass shares the eligibility law —
   fire iff `effective ≤ now ∧ ¬delivered ∧ ¬dismissed` (due) or
   `effective ≤ now ∧ delivered ∧ ¬dismissed` (active) — and the
   discipline "one definition of eligibility, both readers" (the
   `delivered: bool` parameter on `break_firings`/`at_rule_firings`).
   `due_firings`/`active_notifications` signatures already grew once
   for the break pass (`tracking: &TrackingState`, D-9's
   compiler-forced sweep of four call sites); the tracker pass grows
   them again (`habits: &HabitState, day: DayCalendar`), forcing the
   same sweep (fact 10).

3. **`anchor_time` exists, stored, decoration-only**
   (`crates/taskboard-domain/src/habit.rs:204`): `Option<NaiveTime>`,
   per-field LWW clock slot already in `HabitClocks` (habit.rs:161),
   edited through the existing sparse `HabitChanges.anchor_time`
   double-`Option` (`UpdateHabit`), and roams in the synced board-level
   `habits` appdata section (`HabitRecord` row,
   `crates/taskboard-domain/src/appdata.rs:838`). The doc comment says
   "drives no reminder" — that sentence is this chunk's flip (with the
   D-C13 law *kept*: it stays out of every ordering path; the
   `anchor_time_never_influences_the_canonical_order` proptest
   survives unchanged).

4. **The day vocabulary is complete for the derivation**
   (`crates/taskboard-domain/src/days.rs`): `DayCalendar {
   day_start_time, offset }` (the D-F6 injected value) with
   `today(now)` → window-date, `calendar_date`, and — the exact
   primitive — `instant(date, time_of_day)` → UTC (days.rs:145, total
   by the fixed-offset rule). `HabitState { habits, log }` (habit.rs:293)
   keys the log `(habit, window)` with `WindowStart` canonical period
   starts and `period_start`/`period_end` arithmetic (habit.rs:310,
   383); the quota of "the current period" is
   `log[HabitLogKey { habit, window: WindowStart::of_period(kind, D) }]`
   — the `habit_rows` CLI projection reads it exactly this way
   (`crates/taskboard-app/src/commands/habits.rs:589`).

5. **Suppression keys ride the ref text form through storage**
   (`reminder_deliveries`/`reminder_dismissals`/`reminder_snoozes`
   tables all store `reminder TEXT` keyed `(reminder, occurrence)`,
   migrations `0004`/`0009`; the codec delegates to
   `ReminderRef::text_form`/`parse_text_form`
   (`crates/taskboard-storage-sqlite/src/codec.rs:353-363`)). A
   `"tracker:<uuid>"` prefix therefore round-trips with **zero storage
   schema change** — no migration. **Next free migration at this
   base: 0022** (0001–0021 taken; re-verify at implementation time —
   this plan claims none).

6. **Planner arms match `ReminderRef` exhaustively**
   (`SnoozeReminder` command.rs:1880, `DismissReminder`
   command.rs:1950): per-arm row-existence guards (typed
   `UnknownReminderRule`/`UnknownAdhocAlarm`/`UnknownBreakRule`
   rejections), task-anchored refs additionally run the
   `live_task` guard. `DismissReminder`'s `Break` arm is the
   precedent for a synced dismissal (it resolves the row's board and
   rides it into the document); the tracker arm rules device-local
   instead (D-5) — no board ride, no `UpdateAppData` op. The
   dismissal *emission filters* skip non-roaming refs by returning
   `None` per arm (`appdata.rs:1703/1720` for per-card entries,
   `appdata.rs:1749/1765` for the board-level `break` entry) — the
   new arm joins the `None` arms, which is exactly the "Default/
   Adhoc never serialize" split enforced today.

7. **Surfaces are anchor-aware already**: `NotificationRow { task:
   Option<TaskId>, reminder, occurrence, title }`
   (`crates/taskboard-ui-slint/src/presentation.rs:650`) hides the
   Done/Mute targets for non-task anchors; the router maps
   `ReminderAnchor::Task` to navigation and
   `Alarm | Break → DispatchOutcome::NoTarget`
   (`router.rs:488-497`); the OS-toast logger matches
   `ReminderAnchor` exhaustively
   (`crates/taskboard-app/src/notifications.rs:82-96`); the Mark Done
   planner's anchor→task extraction returns `None` for
   `Alarm | Break` (persistence.rs:1854-1856). Adding
   `ReminderAnchor::Tracker` forces these arms — the same
   compiler-forced sweep the break pass documented (D-10 there), and
   the stage-3 trap (d) checklist.

8. **The escalation ladder is insistence-generic**
   (`awaiting_alarms`, `crates/taskboard-app/src/escalation.rs:255`):
   it filters `active_notifications` by
   `note.insistence == Insistence::Alarm` — over *any* `ReminderRef`.
   A Normal-pinned nudge flows through the driver untouched
   (one passive toast + the in-app row, today's whole behavior);
   nothing in the ladder needs to know trackers exist (DR-3
   interplay resolved by D-6 below, not by code).

9. **The engine already injects the day calendar**
   (`crates/taskboard-state/src/config.rs:50`: `DayRollConfig {
   enabled, calendar: DayCalendar }` is an actor field;
   `reminder_tick` (`crates/taskboard-state/src/actor.rs:1198`) reads
   `ReminderEngineConfig` only — the tick grows a `day: DayCalendar`
   parameter fed from the actor's existing `day_roll.calendar`; **no
   new config, no new tick** (the dispatch's "nothing ticks new"
   rule). `stale_deliveries` is called in the same tick
   (actor.rs:1218) and grows the same parameters.

10. **Call sites of the two composed readers** (the D-9 sweep list,
    re-verified): state `core.rs:129` (`due_reminder_firings`, which
    grows parameters), app `escalation.rs:260` (`awaiting_alarms`),
    ui `presentation.rs:675` (`notification_rows`) and
    `presentation.rs:1817` (the alarm-awaiting filter), domain
    `persistence.rs` property tests, `pipeline.rs`/`lib.rs`
    re-exports, plus the test suites
    (`taskboard-state/tests/reminders.rs`,
    `taskboard-storage-sqlite/tests/roundtrip.rs`,
    `tests/sqlite_smoke.rs`). All are compiler-forced.

11. **The habits CLI already edits the anchor**
    (`crates/taskboard-app/src/commands/habits.rs`):
    `habits add --at HH:MM` (routed as the immediate sparse edit,
    habits.rs:285) and `habits update <id> --at HH:MM|--at none`
    (`AnchorEdit::Set/Clear`, habits.rs:437). The nudge's edit
    surface **exists**; this chunk adds no verb, no flag.

12. **ADR/CHANGELOG posture**: the break-rule chunk took no ADR
    (`docs/adr/` stops at 0020, none for break rules — the
    precedent for a reminder family riding existing machinery);
    this chunk likewise claims none. Every chunk lands its own
    `CHANGELOG.md` Unreleased entry (impl-plan §7.2).

## 1. Decisions (stage-1 settlements)

### D-1 — Anchor semantics: `anchor_time` is the *daily nudge* anchor, not a period-boundary occurrence

`anchor_time: Some(t)` **arms** a once-per-day-window nudge at local
time-of-day `t`; `None` disarms (the decoration stays optional —
opt-in nudge, zero behavior change for the chipless habits that exist
today). The alternative — occurrences at period boundaries (fire at
each window's opening) — is **rejected**: a window-open ping double-
surfaces with the today view's Habits section the user is already
looking at (P12 gentle defaults; the nudge's value is a time the user
*chose*, not a boundary the calendar imposed), and it would silently
arm every habit (boundary = always defined; anchor = opt-in).

For `PerWeek`/`PerMonth` targets the nudge is still **daily** — the
*gate* (D-3) is what carries the period: the daily prompt fires only
while the current period's quota is unmet, so a met weekly quota
silences the nudge for the rest of the week. One rule, parameterized
by period kind (the §4.3 attribution vocabulary), instead of three
kind-specific nudge schedules. Recorded consequence: a `PerWeek(1)`
habit with `09:00` anchor pings every morning until the week's quota
is met — intended (that is what "remind me about my weekly habit at
09:00" asks for), and a dismissal ends *that day's* ping (the
occurrence is per-day; D-2), so the noise ceiling is one dismissal per
day per habit.

`anchor_time` stays out of every ordering surface (D-C13 verbatim);
the nudge changes no ordering test.

### D-2 — Occurrence-key derivation: the anchor reading inside the current day-window span (a stable instant)

For the current window-date `D = day.today(now)` (VF-4's night-owl
vocabulary), the occurrence is the **anchor reading inside `D`'s
window span** `[window_start(D), window_start(D+1))`:

```text
T*(D) = day.instant(D, anchor_time)   if anchor_time ≥ day.day_start_time
      = day.instant(D + 1, anchor_time) otherwise
```

Pure function in `habit.rs`
(`tracker_anchor_occurrence(anchor_time, window_date, day) ->
DateTime<Utc>`), sitting beside `period_end` (the same window-span
arithmetic family). Properties that make this the right key (each
pinned by test, §2):

- **Stable within the window**: `T*` derives purely from `(D, anchor,
  day)`; `day.today(now)` is constant across a window (days.rs's
  pinned contract), so every tick inside window `D` sees the same key
  → the delivery receipt suppresses the next tick → **at-most-once
  per window** — the break-rule D-8 law, tracker-shaped. A tick-stamp
  key is the known double-fire bug and stays pinned against (stage-3
  trap (a)).
- **Re-arms on period rollover by construction**: window `D+1` derives
  `T*(D+1) > now` at rollover — a fresh key, no stored arm/disarm
  state, no new tick (the issue's named acceptance, deterministic
  under FixedClock).
- **Honors the wall-clock intent in the night-owl corner**: with
  `day_start 05:00` and anchor `03:00`, `T*(D) = instant(D+1, 03:00)`
  — the nudge fires at the *chosen* 03:00 (in the window's tail), not
  at window open. The `anchor ≥ day_start` branch is the common case:
  `T* = instant(D, anchor)`, same calendar morning.
- **In-window by construction**: `T*(D)` always lies inside `D`'s
  window span (fixed offsets are gap-free — days.rs's DST ruling),
  so `T*(D) ≤ now` iff the anchor reading has passed *in this
  window*.
- **No retroactivity**: a window whose anchor passed while the app
  was off produces no backlog firing — at boot only the *current*
  window's nudge can fire, once (the DR-6 §5.2 day-tick posture:
  catch-up is the day-tick path, missed windows stay missed). The
  evaluator is a now-surface, not a history replay.

Documented leniency (the due-date-move precedent, break-plan D-8
"history edits move the base honestly"): editing `anchor_time` or the
device's `day_start_time`/offset moves every future `T*` — the old
keys' receipt rows orphan and age out through the existing
`stale_deliveries` + 90-day sweep; a *shifted* anchor can legitimately
re-fire the current window's nudge at the new time (an honest re-arm,
bounded to once per (window, anchor value)).

### D-3 — The quota gate: fire/surface iff the current period's quota is unmet (one gate, both readers)

The nudge exists to prompt an unmet quota (the F-1 intent); firing it
at a met quota is noise. The gate reads the canonical current window
`WindowStart::of_period(kind, D)`:

```text
count = habits.log.get(HabitLogKey { habit, window }).map_or(0, |e| e.count)
eligible ⇔ habit.is_live() ∧ anchor_time.is_some() ∧ count < quota
```

The **same** predicate gates both readers — `tracker_firings(…
delivered: false)` for due eligibility, `delivered: true` for the
active surface — the break pass's "one definition of eligibility,
both readers" discipline. Consequences, each pinned:

- **Met quota hides the surface row at read time**: logging the final
  +1 removes the delivered nudge row from `active_notifications`
  (derived, never stored — the house no-derived-bit law; no Withdraw
  machinery, no `LogHabit` side effect). The OS toast already
  delivered is *not* withdrawn (withdraw rides dismissals only,
  reminders.rs:747) — a stale toast until manual close, recorded and
  accepted.
- **Un-metting re-surfaces honestly**: an `AdjustHabitLog` correction
  back below quota re-shows the delivered∧¬dismissed row (the receipt
  still blocks a *second* toast — at-most-once per (window, anchor)
  stands).
- **Dismissal still wins**: a dismissed occurrence never returns (no
  un-dismiss, house law), quota met or not — the gate adds a
  suppression, it never cancels one.
- **Surplus-repair interleave is correct by construction**: a repair
  +1 lands in the *previous* window (`log_window`'s rule); the gate
  reads the *current* window — repairing yesterday does not silence
  today's nudge.

Mute does **not** appear in the gate: `muted: BTreeSet<TaskId>` is
task-shaped (reminders.rs:665), and alarms/breaks are the precedent —
the tracker off-switch is clearing the anchor (D-7).

### D-4 — The evaluator arm and the totality extension (the `Break` shape, cloned)

`reminders.rs` gains, in the break pass's image:

- `ReminderRef::Tracker(HabitId)` — `text_form`/`parse_text_form`
  prefix `"tracker:<uuid>"` (storage round-trips free, fact 5).
- `ReminderAnchor::Tracker(HabitId)` — the typed anchor; consumers
  match without re-matching the ref (the `Break` arm's rationale).
- `tracker_firings(habits: &HabitState, reminders: &ReminderState,
  day: DayCalendar, now, delivered: bool) -> Vec<ReminderFiring>` —
  per live anchored habit in id order: `effective =
  tracker_effective_occurrence(...)`, fire iff the D-3 gate ∧
  `effective ≤ now` ∧ `is_delivered == delivered` ∧ `¬is_dismissed`.
- `tracker_effective_occurrence(habit, log, day, reminders, now)` —
  `walk_chain(T*(D), Tracker(id), &reminders.snoozes)`; the shared
  cycle-bounded walk, snooze-aware like every family.
- `base_occurrence`'s `Tracker` arm stays the totality `None` (the
  `Break` arm's exact precedent at reminders.rs:1184 — the derived
  occurrence needs inputs the shared signature doesn't carry; the
  dedicated reader owns it). `effective_occurrence`'s public contract
  is untouched.
- `stale_deliveries`' `Tracker` arm: unconsulted iff the habit row is
  missing/tombstoned, the anchor is cleared, the quota gate fails at
  the *current* window, or the current effective ≠ the row's
  occurrence (the `Break` arm's gate parity, reminders.rs:991).
- `notifications_for`'s `Tracker` arm: title snapshot from the habit
  row, insistence per D-6; a missing/tombstoned row **skips** the
  payload (the documented totality rule, reminders.rs:807).
- Composition: `due_firings`/`active_notifications` extend their pass
  list with the tracker pass and re-sort — the `(occurrence,
  reminder)` total order preserved across the concatenation, output
  fully deterministic. Signatures grow
  `(habits: &HabitState, day: DayCalendar)`; the sweep is
  compiler-forced across the fact-10 call sites. Alternative
  considered and rejected: stashing `HabitState`/`DayCalendar` inside
  `ReminderState` to keep signatures stable — smuggles device config
  into synced reminder state and breaks the explicit-injection
  pattern (`ReminderDefaults`); the break pass set the growth
  precedent.

Hard stage-2 rule (dispatch verbatim): the evaluator decides WHEN
only — the pass reads the log for the gate (a when-decision, like
`break_crossing` reading entries) and never reads insistence; the
existing four passes' shapes are untouched.

### D-5 — Roam ruling: the habit row *is* the rule (it already roams); receipts and dismissals stay per-device

**Rules roam**: there is no separate tracker-rule entity. The nudge's
entire configuration is `Habit.anchor_time` (D-1), which roams in the
synced board-level `habits` section today (fact 3) — the DR-6
habit-facts recipe, already landed. **Zero new sidecar sections,
zero wire-format growth**: a device that never learns this chunk
relays the `habits` rows verbatim as it does now; the nudge simply
appears when the code lands. (The issue's "tracker reminder rules
ride the same sidecar family" leaning, resolved to its cheapest
consistent reading: the family *is* the habits section.)

**Receipts (deliveries) per-device**: universal R3, unchanged.

**Dismissals per-device — a deliberate deviation from the `Break`
precedent, with the deciding argument**: break-rule dismissals roam
because their occurrence keys derive from *synced* time entries — the
crossing instant is fleet-stable, so a synced dismissal composes on
every device. Tracker occurrence keys derive from the **device's**
`DayCalendar` (offset + `day_start_time` are per-device config, VF-4's
D-F6 contract): two devices whose day configs differ derive
*different* instants for the same habit-window, so a synced dismissal
keyed on device A's instant would suppress nothing on device B —
synced tracker dismissals cannot compose unless configs match, and
config-matching is not a thing the fleet can rely on. Per-device is
therefore not merely acceptable but correct by construction. The
`DismissReminder` tracker arm records **no** synced op; the
appdata emission filters return `None` for the new arm (the
Default/Adhoc split, fact 6).

Alternatives + consequences, recorded for the stage-3 review:

- *Synced board-level dismissals (the `break` entry recipe)*:
  fleet-wide silencing, one dismissal everywhere — bought with a new
  document section, an emission filter arm, and the key-instability
  hazard above (stale rows keyed on foreign devices' instants).
  Rejected for v1; revisit only if per-device day configs converge
  (single-timezone fleets) AND multi-device double-nudge is observed
  in practice.
- *Consequence of the ruling*: two devices each nudge once per day
  for the same habit and each dismissal is local (per-device day of
  silence). The nudge is a gentle daily prompt — double gentle
  pinging across a phone+kiosk fleet is the accepted worst case
  (P12; the same posture as per-device alarm toasts).
- *Consequence for the keys*: per-device dismissal keys need no
  fleet-stable instants — the one place where D-2's config-derived
  keys are a *feature*, not a hazard.

### D-6 — DR-3 insistence interplay: pinned `Insistence::Normal` for v1 (the implicit-default precedent)

The notification payload stamps `Insistence::Normal` for Tracker
anchors — no ladder rung, one passive toast + the in-app row.
Precedent: the task's implicit default reminder has no row and "is
always Normal" (design D-I3, reminders.rs:786-794); a reminder family
without an insistence datum is house shape. Rationale: P3
anti-configurability (no knob without a need) + P12 gentle defaults —
a habit nudge that re-toasts and chimes is aversive by exactly the
mechanism the ADHD research base warns about; the Alarm rung belongs
to alarms and break rules (explicit "interrupt me" intent). Buying
the datum (an `insistence` habit field) would cost a `HabitClocks`
slot, a `HabitChanges` slot, an appdata wire field, a sqlite column
(migration 0022), CLI/UI surface — for a knob nobody asked for.

The escalation driver needs **zero** changes either way (fact 8 — it
filters Alarm-insistent notifications generically). Recorded
follow-up seam: if a rung is ever wanted, the field joins the habit
field family (clocks + wire + column + `--insistence` flag) and the
ladder applies as-is — no evaluator or driver work. This settles the
issue's "ladder levels on habit nudges" seam for v1: *none*, by
pinning, with the extension path named.

### D-7 — Snooze, mute, and the off-switches

- **Snooze composes**: `SnoozeReminder` gains the Tracker arm —
  existence guard: the habit row must resolve *live* (the Break
  arm's guard shape; a snooze of a dead habit's nudge is an edit-style
  rejection, typed `UnknownHabit` — reusing the existing CLI-side
  variant, or a planner-side `UnknownReminderTarget`-style variant if
  the compiler survey prefers a dedicated one; decided at
  implementation by which keeps the error catalogue honest), then the
  standard `RecordReminderSnooze`. The chain walk in
  `tracker_effective_occurrence` moves the occurrence for both
  readers; chains compose (a snooze target is snoozable).
- **Mute does not apply** (task-shaped; the alarms/breaks precedent
  verbatim). The notification row renders Done/Mute-less for Tracker
  anchors (`task: None`, fact 7) — dismiss and snooze remain.
- **Off-switches**: clear the anchor (`habits update <id> --at none`
  → disarm, the pass skips) or `DeleteHabit`. The `DeleteHabit`
  planner cascade extends with the Tracker-ref receipt prune —
  deliveries, dismissals, snoozes of `Tracker(id)` retained away (the
  `remove_break_rule` shape, one definition site: a new `pub(crate)
  fn remove_habit_nudge(…)` in reminders.rs shared by the planner).
  Recorded consequence: a tombstoned habit resurrected by the sync
  fold re-arms its current-window nudge (its pruned receipts are
  gone) — an honest re-arm, at-most-once per window still bounded;
  the habit *log* is untouched by the prune (history is never purged
  — that law scopes to count facts, not device receipts).

### D-8 — CLI verbs and surfaces: zero new verbs; the exhaustive-match sweep

The edit surface exists (`habits add --at`, `habits update --at
Set/Clear`, fact 11); the list row already renders the chip. Changes
are the compiler-forced arms + doc flips only:

| Site | Change |
|---|---|
| `app/notifications.rs:82-96` | `ReminderAnchor::Tracker(habit)` tracing arm (habit id + title-less label path) |
| `ui/router.rs:488-497` | `Tracker → DispatchOutcome::NoTarget` (the `Alarm | Break` arm's shape) |
| `domain/persistence.rs:1854-1856` | Mark-Done anchor→task extraction: `Tracker → None` |
| `ui/presentation.rs` | `NotificationRow` carries `task: None` for Tracker rows (no struct change); push-pass/presentation row tests |
| `app/escalation.rs` | signature sweep only (fact 10) |
| doc comments | `habit.rs:204` "drives no reminder" → the nudge contract; `reminders.rs` module docs name the fifth family |

Kiosk posture (impl-plan §5.7): firing surfaces render as they do for
alarms/breaks (the notifier pipeline is anchor-agnostic); nudge
*editing* is desktop/CLI-side and already exists. No kiosk-specific
work.

### D-9 — Storage, undo, ADR: nothing claimed

- **No migration** — 0022 stays free (fact 5; the dispatch's "migration
  arms if ruled in" resolves to *none*; stage-2 re-verifies the
  number at its base for the record but claims nothing).
- **No new `StateCommand`, no new `PersistenceAction`** — snooze/
  dismiss/deliver ride the existing actions; `apply_actions` and the
  storage `PersistenceAction` exhaustive match force no new arms.
- **No undo classification change** (VF-7): no new command family is
  minted; the habit family's arms are already classified (#555) and
  `UpdateHabit`'s anchor edit needs no new decision. Recorded, not
  tested.
- **No ADR** — the break-rule precedent (fact 12); this plan is the
  ruling record.
- **CHANGELOG**: one Unreleased `### Added` entry for #504
  (stage-2 bookkeeping).

## 2. TDD order and test catalogue (red first; one behavior, one test, lowest layer)

House rules in force: typed outcomes, no text assertions, no quality
ceilings, deterministic (`tokio::time::pause`/`advance` or injected
`DayCalendar` + explicit `now` — no sleeps), proptest domain bounds
per `test_support`. The existing
`anchor_time_never_influences_the_canonical_order` proptest and every
evaluator property extend; none weaken.

### Phase 1 — the derivation + the pure pass (domain, `habit.rs` + `reminders.rs`; everything below consumes it)

1. `tracker_anchor_occurrence` pins (unit, hand-computed instants):
   common case (`anchor ≥ day_start`) → `instant(D, anchor)`;
   night-owl corner (`anchor < day_start`) → `instant(D+1, anchor)`;
   `anchor == day_start` → window open.
2. Proptest: `T*(D)` lies inside window `D`'s span for generated
   `(anchor, day_start, offset, D)`; distinct window-dates yield
   distinct keys (windows are disjoint, so keys cannot collide).
3. Proptest: monotone in `D` (`T*(D+1) > T*(D)`).
4. Ref round-trip: `"tracker:<uuid>"` text form round-trips;
   `parse_text_form` rejects garbage (extends the codec's
   `reminder_ref_text_roundtrips_and_rejects_garbage` too — storage
   crate, Phase 4).
5. `due_firings` fires once at the anchor: fixed state, `now` walks
   the window in ticks (before / at / after `T*` / next window) —
   exactly one Tracker firing, at `T*`, only once `now ≥ T*`; the
   second tick in the same window fires nothing **after the delivery
   batch is applied** (the double-fire guard pin — stage-3 trap (a);
   the property form: the receipt settles the key the first firing
   minted).
6. Quota gate: unmet → fires; `LogHabit` to quota → next tick fires
   nothing and `active_notifications` drops the row (read-time
   auto-clear); `AdjustHabitLog` back below quota → the surface row
   returns, and no second toast (the delivery receipt still blocks
   `due_firings`).
7. Period-kind gate: `PerWeek(1)` met this week → no daily nudge;
   FixedClock journey across the week boundary → fires again
   (re-arm on period rollover — the issue's acceptance, deterministic).
8. Suppression: dismissal of `(Tracker(id), T*)` silences the window;
   the next window re-fires (per-device permanence + re-arm);
   dismissal of a foreign occurrence is the accepted no-op (planner
   parity with every family).
9. Snooze: `SnoozeReminder` on a live habit's Tracker ref plans
   `RecordReminderSnooze`; both readers walk the chain (due pass
   silent until the target, surface row at the target); a dead
   habit's ref is the typed rejection.
10. Totality: tombstoned habit / cleared anchor / missing row → no
    firing, no surface row, and `stale_deliveries` marks the row
    unconsulted (the arm-parity test: consult-only-at-current-
    effective-behind-the-gates).
11. `notifications_for`: Tracker payload snapshots the habit title,
    insistence Normal; missing row skips (the totality rule).
12. `DeleteHabit` cascade: the plan prunes the Tracker-ref receipts;
    the log rows survive (count facts ≠ receipts).
13. Persistence properties (extend, never weaken):
    compaction- and settled-prune-unobservability in
    `due_firings`/`active_notifications` with tracker-armed habits
    present in the generated state; `≤ 1 delivery per
    (Tracker, occurrence)` under `apply_actions`.

### Phase 2 — the planner arms (domain `command.rs`)

14. `SnoozeReminder`/`DismissReminder` Tracker arms: existence
    guards (typed), the no-synced-op pin for Tracker dismissals
    (the plan's `UpdateAppData`-free action list — structural
    assertion on the actions, not on op text), idempotent repeat
    no-ops.
15. The Mark-Done planner is untouched for Tracker anchors (no task
    to auto-dismiss) — the extraction's `None` arm pinned.

### Phase 3 — engine + surfaces (state, app, ui — routing only, per the one-behavior-one-layer rule)

16. Engine journey (state crate, paused clock): reminder_tick
    delivers the nudge batch, persists, then notifies — the
    durable-then-notify order pinned with a Tracker firing in the
    mix; the stale sweep consult test passes with orphaned
    tracker receipts (anchor cleared) aging in.
17. App: `awaiting_alarms` signature sweep compiles; an
    Alarm-insistence filter test stays green with a Normal tracker
    row present (genericity pin, fact 8).
18. UI: presentation row tests — a Tracker notification renders a
    taskless row (`task: None`), title carried; router seam maps
    Tracker to `NoTarget` (the Break parity); dismiss/snooze seams
    address the row by ref+occurrence unchanged.

### Phase 4 — storage + round trip (codec, sqlite tests)

19. Codec: the `tracker:` prefix round-trips (extends fact 5's test).
20. sqlite round-trip/smoke: a persisted state carrying tracker
    deliveries/dismissals/snoozes loads and re-evaluates identically
    (the delivery suppresses the second toast across a reload — the
    durable-guard journey).

### Phase 5 — bookkeeping

21. `CHANGELOG.md` Unreleased entry (#504).
22. Doc-comment flips (fact 7's table) land with the code that
    invalidates them — not as a trailing docs commit.

## 3. Implementation order (commits on `feat/tracker-reminders`)

Each commit standalone (rebase-merge hygiene): build green, clippy
clean, subject `<type>(<module>)`.

1. `test(domain)` — Phase 1 items 1–4 red (derivation + ref).
2. `feat(domain)` — the derivation fn + `ReminderRef::Tracker`/
   `ReminderAnchor::Tracker` + `tracker_firings`/
   `tracker_effective_occurrence` + the `stale_deliveries`/
   `notifications_for` arms + the composed-reader signature growth —
   green, with the compiler-forced planner/surface arms kept
   *minimal-typed* in the same commit so the workspace builds
   (items 5–13 green).
3. `feat(domain)` — planner arms + `DeleteHabit` cascade + the
   dismissal no-synced-op pin (items 14–15).
4. `feat(state,app,ui-slint)` — the tick param plumbing +
   `awaiting_alarms`/`notification_rows` sweeps + router/presentation
   arms (items 16–18).
5. `test(storage-sqlite)` — codec + round-trip journeys (items 19–20).
6. `docs(changelog)` + doc-comment flips if any straggle (21–22).

Handoff: PR `feat(domain,app): the tracker reminders`, body
"Closes #504", gates green via `scripts/run-quality-gates.sh`, no
merge (the user's).

## 4. Risks

- **Signature-sweep breadth** (two new params through four crates):
  mechanical but wide; the compiler forces completeness and stage-3
  trap (d) audits it — a wildcard anywhere is a finding.
- **The weekly-nudge-daily consequence** (D-1): a weekly habit with an
  anchor pings each morning until met. Argued intended; the
  dismissal-per-day bound and the clear-anchor off-switch are the
  valves. If the user rules otherwise post-hoc, the fix is a
  PerDay-only gate clause — one line in the gate, one test flip, no
  structural change.
- **"Drives no reminder" pins**: doc comments and the D-C13 ordering
  proptest say the chip is inert. The ordering law *stands* (the
  nudge never orders); only the reminder-inertness sentence flips —
  stage-2 must flip sentences, not weaken the ordering proptest.
- **Night-owl key semantics** (D-2's tail-anchored `T*`): once per
  window, at the chosen wall-clock time — but a `03:00` anchor with a
  `05:00` day start surfaces as "fires late in the window" to a user
  reasoning in calendar days. Recorded; the alternative
  (calendar-date keying) double-fires around the window boundary and
  was rejected — revisit only on user evidence.
- **Per-device dismissal drift**: multi-device fleets double-nudge
  (D-5's accepted worst case); the revisit trigger is recorded there.
- **Quota-gate read-time auto-clear** vs user expectation ("I logged
  it, why did the notification vanish before I dismissed it"): it is
  the derived-surface law (the row's predicate went false); the
  alternative (dismiss-only clearing) keeps a dead prompt alive.
  Recorded for the review.
- **Base drift**: parallel dispatch chunks land while this one is in
  flight; stage-2 rebases and re-verifies the migration counter and
  the fact-10 call sites (all compiler-forced — drift turns into
  build errors, not silent divergence).

## 5. Estimated diff size

**M** — roughly 550–700 lines including tests:

| Area | ≈ lines |
|---|---|
| `taskboard-domain` (reminders.rs pass/arms, habit.rs derivation, command.rs arms, persistence.rs props, appdata.rs skip arms) | ~380 (≈45% tests) |
| `taskboard-state` (core/actor plumbing + tests) | ~70 |
| `taskboard-app` (escalation/notifications sweep) | ~40 |
| `taskboard-ui-slint` (presentation/router arms + row tests) | ~60 |
| `taskboard-storage-sqlite` (codec/round-trip tests) | ~40 |
| `CHANGELOG.md` + doc flips | ~30 |
