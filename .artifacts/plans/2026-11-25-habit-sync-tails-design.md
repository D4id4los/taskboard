# The Habit Sync Tails Design (round 8, chunk E3)

- **Date**: 2026-11-25
- **Branch**: `docs/habit-sync-tails-ruling` (stage-1 design; docs-only)
- **Scope report**: `.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md` §E-E3
- **Issues in scope**: #1363 (files sidecar habit-time lane), #1340 (day-log sync lane),
  #1341 (week/month undo divergence — **operator gate**)
- **Status**: **GATE-PAUSED** — the #1341 recommendation below needs the operator
  ruling before any stage-2 implementation may be dispatched. The #1363/#1340
  dispositions are recorded and stand unless the operator overrides.

## 1. Recorded reality (the landed base)

- **The A3 habit-time wire chain (Deck sidecar only)**: `habit_time_entries_for`
  mints the section rows (`crates/taskboard-domain/src/appdata.rs:2506`), the Deck
  flush overlays it (`crates/taskboard-sync-nextcloud/src/flush.rs:431,487`) with
  wire GC beside the habit-log GC (`flush.rs:503`,
  `lane_gc_old_habit_time_entries` at `appdata.rs` — wire-only, never deletes local
  rows), and the merge side folds it by row clock
  (`crates/taskboard-domain/src/merge.rs:1284`, `fold_habit_time_entries`).
- **The files sidecar's lane structure** (`crates/taskboard-sync-files-nextcloud/src/board_cycle.rs:1663–1727`):
  the files board cycle already emits `time_entries`, `habits`, `habit_log`,
  `break_rules` lanes via the shared pure builders, but hard-codes
  `habit_time_entries: None` with the in-place reason comment
  (`board_cycle.rs:1718–1722`). The unknown-section relay keeps a Deck peer's
  `habit-time-entries` rows from folding into a files-bound document
  (`crates/taskboard-domain/src/appdata.rs:79` — unknown sections land in
  `unknown` verbatim), so the boundary is honest, not lossy-silent.
- **The day-log overlay**: `HabitState.day_log`
  (`crates/taskboard-domain/src/habit.rs:839`, keyed `(habit, day)`), written
  transactionally with the window row by `LogHabit`
  (`crates/taskboard-domain/src/command.rs:3237–3283`, the transactional-twin law)
  and by the Day-kind arm of `AdjustHabitLog`
  (`command.rs:3319–3348`). Persisted via sqlite migration 0030
  (`crates/taskboard-storage-sqlite/src/repo.rs:1438,2929–2941`), LWW on `clock`.
  It appears in **no** `AppDataSections` record — device-local by construction,
  the #1144 closure's recorded leniency 1.
- **The B1 clock-bump rule**: `mint_log_clock(now, stored) = max(now, stored + 1s)`
  (`crates/taskboard-domain/src/habit.rs:802,2462`) — every log/adjust/undo write
  stamps strictly newer than any stored same-key row (#986), so undo corrections
  win LWW by construction.
- **The undo machinery**: undo planning synthesizes inverses in
  `crates/taskboard-domain/src/command.rs`; the journal classifies habit verbs
  whose inverse is lossy or unwatermarkable as non-undoable
  (`crates/taskboard-domain/src/journal.rs:121–130` — the absent-window
  `AdjustHabitLog` precedent: a `count: 0` inverse would forge history). The
  `LogHabit` receipt carries `(habit, window, count)` only — **not** the day the
  overlay twin attributed to.

## 2. Decision (a) — #1363: the files sidecar's habit-time lane

**Options**

1. **Implement the files lane now** — add a files-scoped
   `habit_time_entries_for` twin (board-keyed, no card hop — the
   `files_time_entries_for` precedent at `appdata.rs:2255`), emit it in
   `board_cycle.rs` in place of the `None`, and wire fold + GC. The merge-side
   fold (`merge.rs:1284`) and the wire GC are board-agnostic and are reused
   verbatim; only emission is files-specific. Estimated small: one pure builder
   twin, one call site, round-trip tests in the files connector suite.
2. **Record the deferred boundary** — keep #1363 open as the p3 backlog item it
   already is, with the in-place comment and the unknown-section relay as the
   standing contract.

**Trade-offs**: option 1 closes a real substrate divergence — habit timers
simply do not reach files-substrate peers, and nothing surfaces that to the user
(it is honest on the wire, invisible in the product). But the files connector is
the secondary substrate, the lane is p3, the boundary is documented in three
places (code comment, journey plan D-6, issue), and this round's E3 chunk is a
*design decision* chunk, not an implementation chunk. Option 2 costs nothing
now but leaves the divergence open indefinitely.

**Recommendation: implement the lane, as its own small chunk scheduled for
round 9** — not as part of E3. The work is mechanical (the shared codec and the
fold/GC already exist; the #1305 D-6 scoping comment is the only thing keeping
the lane files-absent) and the silent freshness gap on a shipped substrate is
worth closing, but it is code work that does not belong in a docs-only ruling
chunk and needs the files round-trip fixture surface, not a semantics ruling.

## 3. Decision (b) — #1340: the day-log overlay's sync lane

**Options**

1. **Keep device-local (record the boundary)** — the overlay stays display data
   on the logging device; the synced `habit-log` section (already shipped, both
   connectors, `habit_log_for` at `appdata.rs:2699` + `union_habit_log` row-LWW
   at `appdata.rs:1811`) remains the multi-device oracle.
2. **Add a `habit-day-log` wire section** — union fold keyed `(habit, day)`, the
   `habit-log` section's shape: section record, `habit_day_log_for` builder,
   `union_habit_day_log` fold, 30-day GC twin, flush builders in **both**
   connectors, fold plumbing into `HabitState.day_log`, corpus/snapshot fixture
   churn across the board.
3. **Derive day cells from the synced `habit-log`** — not available: a Week/Month
   window row cannot attribute days (exactly the #1341 finding), so this option
   is structurally impossible for the habits where the heatmap diverges most.

**Trade-offs**: option 2 buys cross-device heatmap parity — log on device A, see
the day cell on device B. But the heatmap is a display-only surface (R-2:
overlay-only facts credit zero adherence), the oracle data already syncs, and
option 2 is the largest wire-surface addition on the menu here (two connectors
× builder/fold/GC/codec + fixtures) for display parity on a secondary surface.
It also hard-depends on the #1341 semantics: a synced per-day overlay inherits
whatever attribution ruling #1341 produces, so sequencing it before that ruling
would be backwards. Option 1 keeps the overlay exactly what #1144 scoped it as.

**Recommendation: option 1 — keep the day-log overlay device-local, permanently
as a recorded boundary, not leniency.** The window log is the synced oracle, the
overlay is display data, and if multi-device day attribution is ever wanted it
must be re-opened *after* the #1341 ruling lands, as a deliberate feature round
(ux-journey-plan law applies: new user-visible multi-device behaviour), not as a
sync tail. Amend #1144/#1340 wording from "device-local, no sync lane (recorded
leniency)" to "device-local by design (recorded boundary)".

## 4. Decision (c) — #1341: the week/month undo divergence (THE GATE)

The finding: `LogHabit` writes the window row **and** the day overlay twin in
one batch (`command.rs:3237–3283`). Its undo inverse reuses `AdjustHabitLog`
with the pre-count; that command writes the day-kind overlay twin **only** when
`habit.target.kind() == Day` (`command.rs:3319–3348` — an absolute Week/Month
correction cannot attribute days). Result: undoing a log on a Week/Month habit
restores the window row exactly (the oracle) but leaves the day overlay's +1 in
place — the heatmap keeps counting a completion that no longer exists.

**Options**

1. **Accept as recorded leniency** — amend the comment/docs; no code. Cheapest,
   but the divergence is *user-visible* on the heatmap surface (the overlay's
   entire purpose): on a Week/Month habit, every undo leaves a phantom day cell.
   "Display-only, the window log is the oracle" is true for stats and false for
   the pixels the user looks at. This is a limitation, not a contract —
   per the no-pins law it would need a `..._currently_pins_...` shape forever.
2. **Extend the inverse: carry the day key, undo the twin too** — the `LogHabit`
   journaled effect gains the day key the overlay twin attributed to (the
   receipt today carries only `(habit, window, count)`; `day.today(now)` at
   *undo* time would attribute to the wrong day, so the key must be captured at
   *log* time into the journal payload). The undo batch then emits, beside the
   `AdjustHabitLog`, a `RecordHabitDayLog(day_key, count − 1)` with the standard
   `mint_log_clock` bump — the exact transactional-twin mirror of the log-time
   write. Keeps the overlay truthful; bounded cost (journal effect payload
   field, one inverse arm, the guard that the decrement never goes below zero,
   tests). Consistent with the journal's existing posture: the absent-window
   `AdjustHabitLog` refusal (`journal.rs:121–130`) exists precisely because
   inverses must not lie; an overlay left over-counting is a softer lie of the
   same family.
3. **Refuse undo for Week/Month `LogHabit`** — classify it lossy like the
   absent-window adjust (`journal.rs:121`). Consistent posture, no journal
   payload change, but removes undo availability from the most common habit
   verb on Week/Month habits — a real UX regression to fix a display bug.
4. **Attribute the adjust's delta to today** — make `AdjustHabitLog` on non-Day
   windows write a day twin for `today`. Explicitly rejected: the #1144
   implementation note says a Week/Month correction "cannot attribute days — no
   invention". Inventing today-attribution would corrupt the heatmap on every
   audited correction, not just undos.

**Trade-offs**: option 1 is free but permanent user-visible divergence. Option 2
is the only option that makes the overlay truthful, at the cost of one journaled
payload field and one inverse arm; risk is low (the day twin is already a
first-class action, `RecordHabitDayLog`, with LWW clocks; the increment side is
already transactional, so the mirror has a worked precedent). Option 3 trades a
display bug for a missing feature. Option 4 is ruled out by recorded design.

**Recommendation: option 2 — extend the `LogHabit` undo inverse to undo the
day-overlay twin, with the day key captured into the journaled effect at log
time.** Guarded (floor at zero, `mint_log_clock` bump, same-batch transactional
write), precedent-backed (`RecordHabitDayLog`, the absent-window refusal's
"the inverse must not lie" posture), and it upgrades the overlay from
"divergent display" to data that honours undo everywhere the window log does.
Fallback, if stage-2 finds the journal payload extension heavier than scoped
(e.g. the persisted undo line shape resists the field): degrade to option 3
(refuse undo of Week/Month `LogHabit`) rather than ship the divergence —
never option 1 by default.

**Awaiting the operator ruling.** No implementation is dispatched against this
section until the gate clears.

## 5. Disposition summary

| Question | Recommendation | State |
|---|---|---|
| #1363 files habit-time lane | Implement as a small round-9 chunk (not E3) | recorded, stands unless overridden |
| #1340 day-log sync lane | Device-local **by design** (recorded boundary, not leniency) | recorded, stands unless overridden |
| #1341 undo divergence | Inverse undoes the day twin (journal carries the day key); fallback: refuse undo on Week/Month | **GATE-PAUSED — operator ruling required** |

## 6. Out-of-scope findings → backlog

- The day-log overlay has **no local retention**: `HabitState.day_log` grows
  unbounded (no prune/GC on the sqlite side or in `PersistedState` — the
  window `habit-log` has none either by posture ("history outlives"), but the
  overlay is display data with no such justification). Filed as a `backlog:`
  issue at stage-1; number reported in the stage report.

---

## SUPERSEDED (operator ruling 2026-11-27)

This design round is **superseded in full** by the HabitLog storage
refactor (the per-completion timestamped event log — see the tracking
issue opened from the operator's ruling). The operator found the
window-log + day-overlay twin to be a two-sources-of-truth defect at
the data-model level and ruled that refactoring the store's granularity
is the prerequisite: decisions (a)/(b)/(c) above are moot until the
event-log refactor is planned and implemented, after which the E3
issues are re-triaged for residue.
