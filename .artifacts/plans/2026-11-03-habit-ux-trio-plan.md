# [Plan] Backlog Round 3, Chunk 2.3 — The Habit UX Trio

- **Date**: 2026-11-03 (chunk dispatched 2026-11-04; the dispatch plan's
  date is kept in the artefact name)
- **Kind**: stage-1 TDD-first plan for chunk 2.3 of backlog round 3
  (`feat/habit-ux-trio`) — the three DR-6 D-C10/D-C12 follow-ups of
  issue #506: (i) the one-click series↔habit conversion verb, (ii)
  series `paused_until`, (iii) streak-repair affordances.
- **Origin (normative)**: issue #506; DR-6 plan
  `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §3.7 (bounds/lifecycle), §3.8 (deliberate non-builds), §4.7 (the
  conversion seam), §4.3 (streaks/attribution), §6 rulings D-C8/D-C10/
  D-C12; dispatch plan
  `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md` §1/§3
  chunk 2.3 (acceptance: verb + pin + UI affordance per item, blanket
  rule, conversion round-trips both directions with history preserved
  on the seam, pause stops miss-attribution inside the window and
  resumes cleanly pinned at the attribution layer, repair changes only
  affordance — attribution math diff-empty, CHANGELOG entries).
- **Gate record (2026-11-04)**: chunk 2.2's PR #614 (`fix/dead-board-drain`)
  **merged** into `origin/main` (c5dddaea) — the serial domain lane
  1.2 → 2.1 → 2.2 → 2.3 is at its head, chunk 2.3 may start. GUI review
  round 4 is **fully merged** (chunks 9 #605, 10 #613, 11
  `feat/ui-r4-settings-connections` ancestor-verified) — no
  rebase-friction expectation, no today-view file collision; the
  round-3 batch-4 gate (chunk 11 merged) is green for later chunks.
  Observed, not blocking: round-3 batch-1 chunks 1.4
  (`test/kani-recurrence-harnesses`, `#[cfg(kani)]`-only — exempt from
  the serial lane per §1 lane 1) and 1.5 (`test/app-mutants-oneoff`,
  app test files) have no PRs yet; neither touches the domain/state
  production lane 2.3 continues, and the round has already dispatched
  past them (2.1/2.2 merged). Batch-2 start is recorded as the
  user's own dispatch decision.
- **Worktree**: doctor green (217G free, floor 25G);
  `../taskboard-WT-feat-habit-ux-trio` created with
  `git worktree add -b feat/habit-ux-trio … origin/main` at c5dddaea.
- **Status**: READY for stage 2 — every open question resolved below
  with a recorded decision.

## 0. Findings → mechanisms audit (what the code confirms)

| # | Seam read | Confirms |
|---|---|---|
| F1 | `Habit.continued_from: Option<SeriesId>` (`habit.rs:210`) with **no clock slot** (`HabitClocks`, `habit.rs:148-164`, D-H7a); write path exists: `HabitChanges.continued_from: Option<Option<SeriesId>>` (`messages.rs:596`); `RecurRule.continued_from: Option<HabitId>` (`recur.rs:665`) | The seam is typed on both sides; only the *verbs* are missing |
| F2 | `habit_streak_unioned` (`habit.rs:544-593`): the read-side history union is **habit-side only** — walks the habit's own streak, then counts consecutive *completed* series members backward from the break period via `schedule` + `roll_reference_date` (D-H15: no grace crosses the seam) | Conversion round-trips keep history via this walk; the series side has no reciprocal union (recorded limitation, D-3) |
| F3 | Series arithmetic: `schedule` (`recur.rs:1005`, Until filters, Count engine-evaluated), `occurrence_outcome` (`:1063`, window attribution no-knob), `next_occurrence` (`:1127`, the one badge law), `roll_target` (`:985`); engine arms: spawn `next_after` at `command.rs:1062-1066`, roll `plan_day_rolls` at `:717`; "missed" is derived, never stored | A pause implemented as "members inside the window don't exist" integrates at five named points and every derived analysis stays honest |
| F4 | Attribution/streak family: `log_window` (`habit.rs:418`, grace + surplus-repair), `streak`/`streak_with_break` (`:474/:486`), all pure over public `period_start/period_before/period_after/period_end` | The repair affordances can be **pure presentation reads + `AdjustHabitLog` dispatches**; zero attribution edits |
| F5 | Undo catalogue (`undo.rs:95-250`): v1 PreImages rows; v2 record-time dynamic rules — **D25: `UpdateHabit` with `continued_from` set refuses (`NotUndoable(HabitSurface)`)** because the seam carries no clock; `LogHabit`/`DeleteHabit` NotUndoable; `AdjustHabitLog` row-dependent (D24); `InverseClass::UpdateTask` (`:825-837`) synthesizes a recur-restoring `UpdateTask` from the task pre-image; `command_class()` (`:664`) maps 1:1; `synthesize_undo` returns exactly **one** `StateCommand` | Composite convert verbs cannot synthesize inverses → classify per D25's seam-addressed logic; a `PauseSeries` task mutation *can* reuse `InverseClass::UpdateTask` |
| F6 | Router contract: `DispatchOutcome::Command(StateCommand)` is single (`router.rs:255-271`); habit verbs are one-shot-context actions (`ActionId::HabitLog/HabitMove`, `router.rs:482-491`, adapter one-shot ctx `adapter.rs:214-220`, seams `install_habit_seams` `:3680-3737`) | A UI-composed 3-command flow would need a router contract change — a domain verb fits the house shape instead |
| F7 | UI surfaces: `HabitsSection` (`habits.slint`) — rows + gated edit cluster (kiosk gate `habit_editing_visible`, D-P13/D-T16); `habit_rows` (`presentation.rs:4222`) builds `HabitRow` incl. unioned streak; recurring badge on cards (`models.rs:1959`, `recur_badge`/`recur_badge_label` `presentation.rs:913/939`); **task view has no recur row today**; overlay precedents: capture/focus/date-picker/search-palette | The pause + series→habit affordances open a **new task-view recur row**; habit→habit-row affordances extend the gated edit cluster |
| F8 | CLI: `tasks repeat` family (`repeat.rs`) — `RepeatInvocation::{Off, Set}`, `assemble_rule` builds the whole rule; `habits` family has add/list/log/adjust/rm/update | Pause rides `tasks repeat --pause <date>` / `--resume`; repair's CLI valve (`habits adjust`) already exists — no CLI growth for item (iii) |
| F9 | Storage: `recur` rides the task's appdata document as `ModeledValue::Recur` whole-rule JSON with one clock (`appdata.rs:1344-1383`); no sqlite schema change needed for a new optional rule field | `paused_until` is decode-compatible (`#[serde(default)]`); **no migration** (lane rule 5 satisfied) |
| F10 | Blanket rule (r4 D21): "every new interactive element ships **hover + walk-stop + action-layer dispatch**" (round-4 design plan `:441-447`) | The three requirements every new element below must meet |

## 1. The bundle/split decision

**Bundle: one branch, one PR, three internally-ordered commit groups.**
Recorded reasons:

1. The dispatch plan already bundled all three items into chunk 2.3
   (§1 table; "bundle or split at dispatch — the plan decides"); the
   issue itself says "each small".
2. One shared file family: `recur.rs`/`command.rs`/`messages.rs`/
   `undo.rs` (pause + convert), `presentation.rs`/`habits.slint`/
   `task-view.slint`/`router.rs`/`adapter.rs` (all three). Splitting
   into three PRs would serialize three review cycles over the same
   files for zero parallelism — the domain lane (§1 lane 1) is
   strictly serial anyway, so a split buys no concurrency.
3. The items are reviewable as one coherent slice: the convert verb
   and the pause are both rule-lifecycle verbs on the same entities;
   the repair affordances are the UI counterpoint on the same rows
   the convert buttons live on.

Order inside the branch: domain-first per commit group — (a) pause
arithmetic + verb, (b) convert verbs, (c) repair affordances + all UI.
Every commit standalone (rebase-merge hygiene, AGENTS §8).

## 2. Decisions (all settled here; alternatives recorded)

### D-1. Item (i) verb shape: two new domain verbs, composite planner arms — not UI-composed existing commands

`StateCommand` gains:

```rust
/// One-click series → habit conversion (D-C12's verb): mints a habit
/// on the head's board linked to the series (`continued_from`), and
/// stops the series by clearing the head's rule (§3.7 "stop
/// repeating" — the head remains an ordinary task). The guided UI
/// flow feeds `target`; the planner composes what the manual
/// two-step path would (the `completion_spawn` batch precedent).
ConvertSeriesToHabit { head: TaskId, target: HabitTarget },
/// One-click habit → series conversion: mints an ordinary task head
/// in the habit's board's Backlog stack (the CaptureTask D-V5
/// precedent, created on demand) carrying a rule whose
/// `continued_from` names the habit, then tombstones the habit (the
/// log outlives the tracker — history is never purged).
ConvertHabitToSeries { habit: HabitId, pattern: RecurPattern },
```

**Why domain verbs, why not the UI dispatching
`CreateHabit`+`UpdateHabit`+`UpdateTask`**: (a) the router's
`DispatchOutcome` is single-command (F6) — a fan-out is a contract
change with no other consumer; (b) engine-side composition is the
house pattern (`completion_spawn` `command.rs:618` composes successor
card + ops + label/reminder/mute replication as one planned batch);
(c) three separate journal entries would put a D25-refusing entry at
the undo line head — an incoherent partial-undo UX; one entry with one
recorded classification is honest.

**Planner effects** (mirroring the manual path's emissions exactly):

- `ConvertSeriesToHabit`: load live head, typed rejection when
  `recur.is_none()` (`NotRecurring`-style error — the `Set` repeat
  posture); clear `recur` on the head (stamp `ck_recur`), enqueue the
  head's `UpdateAppData` op; mint the habit (`HabitTarget` from the
  command — the flow's preset chips), title = head title, board =
  head board, `order = append_order_after(board's live orders)`
  (`habit.rs:375`), `continued_from = Some(rule.series)`; persistence
  + sync-op emission mirrors the `CreateHabit` arm verbatim.
- `ConvertHabitToSeries`: load live habit, mint the head task (title =
  habit title, board's Backlog stack per D-V5, order via
  `insertion_order_after` `command.rs:587`), rule = `RecurRule {
  series: mint, anchor: today (day.today), pattern, bound:
  RecurBound::None, continued_from: Some(habit) }`, scheduled at
  today (the `series_reference_date` chain); tombstone the habit;
  persistence/sync emission mirrors `CreateTask` + `DeleteHabit` arms.

**Bounds on conversion**: always `RecurBound::None` and target
default-mapped (D-2); bounds stay editable through the existing
`tasks repeat --until/--count`. No invented surface.

**Undo classification (v1/v2 catalogue rules)**: both converts →
`UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)`. This is the
**D25 extension**, not a new category: the conversion *is* a
seam-addressed operation — the seam field deliberately carries no
clock slot (D-H7a), so there is nothing to watermark the seam restore
against, and the composite's inverse (restore recur + remove the
minted habit / un-tombstone) is unexpressible in the 1-inverse-command
synthesis (`synthesize_undo` returns one `StateCommand`, F5). The
classification table gains two arms beside the existing
`UpdateHabit`-seam arm with this reason in the doc comment.

**Round-trip acceptance** (dispatch §3): S→H (habit
`continued_from=S`, head rule cleared) → S′ (new series minted,
`rule.continued_from=H`, H tombstoned) → H′ (habit
`continued_from=S′`): every link survives in state; the seam chain
S′ → H → S is readable from state alone. Pinned at the domain layer
(§5, P-C1/C2).

### D-2. Conversion mappings (the guided flow's defaults)

- series → habit: `Daily(n) → PerDay(1)`, `Weekly{…} → PerWeek(1)`,
  `Monthly{…} → PerMonth(1)` as the *pre-selected chip*; the flow's
  preset chips (1/day, 3/day, 1/week, 3/week, 1/month + custom count)
  override before confirm. Quota semantics don't transfer from a
  pattern (a weekly series is one completion/week, not a quota) — the
  mapping is a starting point, never silent.
- habit → series: `PerDay(n) → Daily(n)`;
  `PerWeek(n) → Weekly{on: Monday}` / `PerMonth(n) →
  Monthly::OnDay(1)` — **the quota has no series analog** (a series
  member is a completion, not a quota); the flow says so in its
  consequence line. Weekday/on-day stay at the default; editable later
  via `tasks repeat`.

### D-3. The union direction limitation (recorded, not silently dropped)

The read-side history union (`habit_streak_unioned`) is habit-side
only: a series→habit conversion preserves visible history (the habit's
streak continues into the series' completed members); a habit→series
conversion leaves the retired habit's log visible only in the log
itself — no series-side streak view exists to union into (the badge
shows schedule truth, not history). This matches v1 exactly (§4.7:
"the views union them" — the habit view is the one that exists).
Refinement stays F-2 backlog territory; the round-trip pins keep the
seam data intact so a later read-side union needs no migration.

### D-4. Item (ii) semantics: paused members do not exist — no miss, no credit

`RecurRule` gains `#[serde(default)] pub paused_until:
Option<NaiveDate>` — a plain local calendar date (the `anchor`
posture: date-space, no newtype, no instant arithmetic, DST-immune by
construction), **inclusive**: members with `date ≤ paused_until` do
not exist. Consequences, each pinned at the attribution layer
(dispatch acceptance):

- `schedule(rule, from, to)` skips paused members (the cursor walks
  past them; they are neither pushed nor terminal).
- `occurrence_outcome`: a member's attribution window closes at the
  next **live** member's window start — completions made during the
  pause attribute to the last live member before the pause (D-C8's
  late-completion honesty, unchanged; you can still tick off the
  vacation-adjacent occurrence).
- `next_occurrence` (the badge law): stale arm → the first live
  member > max(stale-ref, paused_until); fresh arm → the first live
  member > reference. `paused_until` past `Until` → `date: None`
  (schedule truth).
- Spawn arm (`SetTaskDone`, `command.rs:1062`): `next_date` = first
  live member after the reference date; `bound_admits` unchanged
  (Until is arithmetic truth — a pause reaching past `Until` ends the
  series at the terminal completion, typed clear; Count is
  completion-history-evaluated and untouched).
- Roll arm (`plan_day_rolls`, `command.rs:717`): the target is the
  first live member > stale (there is no "latest member ≤ today"
  inside a pause — mid-pause the head rolls forward to the first
  post-pause member and sits fresh; on resume day it is already due).
  `roll_target(pattern, …)` itself stays untouched (property 4's
  pinned contract) — a rule-level live-member law composes above it.
- `habit_streak_unioned` needs **no edit**: it enumerates `schedule`,
  which now omits paused members — gaps inside a pause don't break the
  joined run (pinned).

Implementation shape: one private helper
`next_live_after(rule, from) -> NaiveDate` (walk `next_after` past
`paused_until`; terminates — the pause is a finite date) used by the
five integration points; proptests extend the catalogue: pausing a
range removes exactly the members in `[from, to] ∩ (−∞,
paused_until]`; streak/union equality between "paused" and "members
deleted from the pattern" on generated inputs.

### D-5. Who sets `paused_until`: the user, via `PauseSeries`; the engine never writes it

New verb:

```rust
/// Pause (until = Some) or resume (until = None) a recurring series.
/// Read-modify-write inside the planner: only `paused_until` moves on
/// the current rule — never a whole-rule replace from the client.
PauseSeries { head: TaskId, until: Option<NaiveDate> },
```

**Why a verb, why not `UpdateTask{recur: Some(modified_rule)}`** (the
§3.7 task→series precedent): that path replaces the whole rule with a
client-constructed copy — under a sync race it clobbers a concurrent
pattern/bound edit (one `ck_recur` clock, last writer wins). The verb
does read-modify-write under the state lock, touches one field, and
gives typed rejections: unknown head, head not live, `recur.is_none()`
(no rule → nothing to pause), and `until` unchanged = accepted no-op
(the `MoveTask` posture). **Undo**: `PreImages` via
`InverseClass::UpdateTask` reuse — the command_class map gains
`PauseSeries → InverseClass::UpdateTask` and the existing recur
pre-image restore does the rest (F5); undoing a pause restores the
pre-pause rule byte-for-byte.

### D-6. Resume behavior: date-derived, zero state transition; the affordance clears the field

Resume is not an event: once `today > paused_until`, every live-member
computation admits members again — nothing fires, nothing is written
(the §3.8 honest default: an expired `paused_until` is semantically
inert — pinned: a rule with a stale `paused_until` behaves exactly
like one with `None`). The **resume affordance** (and
`tasks repeat --resume`) dispatches `PauseSeries { until: None }` to
clear the field for hygiene — otherwise badges would forever carry a
dead pause. Inside the window: no member exists (no miss, no credit —
D-4); the window's completions attribute to the pre-pause member
(D-4's third bullet). The badge shows the paused state for the whole
window (chip phrase from a presentation constant; tests assert
structure, never wording).

### D-7. Item (iii): repair affordances only — the attribution fence

**Fence (binding for stage 2/3)**: `log_window`, `streak`,
`streak_with_break`, `habit_streak_unioned` and their test modules are
**diff-empty** for item (iii); the review agent re-checks this by
diffing `habit.rs`'s attribution functions. The research-aligned
surfaces are:

- **The explicit make-up affordance** (today-view habit row, gated
  edit cluster): when the previous window is under quota **and** still
  within its 24 h overflow (`now < period_end(prev) + 24 h` — the
  D-C8 grace arithmetic, read-side only), the row gains a make-up
  button. Click → `AdjustHabitLog { habit, window: prev, count:
  prev_count + 1 }` — the existing audited valve (§4.2: "the explicit
  valve the automatic path never uses"), deterministic target: what
  you click is what increments, no re-derivation at dispatch. The
  presentation helper `habit_repair_target(habit, log, now, day) ->
  Option<(WindowStart, u32)>` is **new read-side code in ui-slint**
  composing the public `period_end`/`period_before` — no domain
  change.
- **The +1 landing hint**: when `log_window(habit, log, now, day)`
  returns the previous window (the surplus-repair arm), the +1
  button's hover tip names the landing window (a presentation
  constant). `log_window` is *called*, never edited — the attribution
  decision stays the domain's.
- **CLI**: none. `habits adjust --window --count` already is the
  explicit repair valve; a parallel `--repair` sugar would duplicate a
  verb for zero new semantics (recorded non-goal).

**Undo**: no new rows — `AdjustHabitLog`'s existing D24 record-time
rule applies verbatim (pre-row present → undoable; absent →
`NotUndoable(HabitSurface)`), and the make-up click onto an empty
count-0-less window is exactly the case D24 already names.

### D-8. UI surfaces and the blanket rule (three requirements per new element)

- **Task view recur row (new)** — the surface for items (i)+(ii) on a
  series head: pattern echo + paused chip + `Pause…`/`Resume` +
  `Convert to habit`. Pause opens the existing date-picker overlay
  (round-3 D10 component) and dispatches `PauseSeries` with the chosen
  date; Convert opens the confirm overlay (below). Every new element:
  hover tip (label + consequence phrase), tab-walk stop (the row
  joins the task view's focus inventory), action-layer dispatch
  (one-shot ctx + new `ActionId`s: `SeriesPause`, `SeriesResume`,
  `SeriesConvertToHabit` — the `habit.log`/`habit.move` one-shot
  pattern, F6).
- **HabitsSection edit cluster (extended)** — `Convert to repeating
  task` joins the gated cluster (`editing-visible`; kiosk keeps
  rendering rows, actions never mount — D-P13/D-T16 unchanged), and
  the make-up button (D-7) renders under the same gate. New
  `ActionId`s: `HabitConvertToSeries`, `HabitRepair`.
- **The confirm overlay (new composition, capture-overlay/menu-overlay
  precedent)**: one shared scaffold parameterized per direction —
  mapping summary (D-2), consequence lines (presentation constants:
  "the series stops repeating; the habit continues its history" /
  "the habit is retired; its history links to the new series; the
  quota has no series analog"), preset chips, Confirm routes the
  convert command, Cancel/backdrop/Esc dismiss. Overlay discipline
  follows the existing rules (one overlay at a time, drag exclusion,
  activation-stop).
- **The card badge** gains the paused chip (`recur_badge` reads
  `rule.paused_until`; `recur_badge_label` renders the paused phrase
  while `today ≤ paused_until`, and `next: None` already renders the
  pattern-only badge for the pause-past-`Until` end). Display-only —
  no interaction, no blanket obligations beyond existing decoration
  rules.

### D-9. CLI surface: `tasks repeat --pause <date>` / `--resume`

`RepeatModeFlags` gains `--pause <date>` (conflicts with
`--daily/--weekly/--monthly/--off/--resume/--until/--count`) and
`--resume` (conflicts likewise); `RepeatInvocation::Pause { task,
until }` collapses to `StateCommand::PauseSeries`. On a rule-less or
done task: the same typed refusal posture as `Set`. The receipt gains
the paused/resumed phrase (presentation constants, structural tests).
`--off` unchanged. No `habits` CLI growth (D-7).

### D-10. Cross-cutting execution rules (stage 2)

- New user-visible strings are presentation constants; no text
  assertions — structural facts (`matches!`, phrase non-empty +
  deterministic re-push equality, the existing badge-label posture).
- Time-dependent tests use the fake clock seams / paused tokio clock;
  never sleeps; day-boundary journeys derive from `DayCalendar`.
- The `#[serde(default)]` decode pin: a stored/synced rule JSON
  *without* the field decodes to `paused_until: None` (old rows
  decode; round-trip both shapes) — the documented-leniency boundary,
  no schema change, **no migration** (lane rule 5 satisfied).
- SPDX headers per AGENTS §6 (domain/state/app: MIT OR Apache-2.0;
  ui-slint: AGPL-3.0-only); `#![forbid(unsafe_code)]` everywhere
  outside ui-slint.
- A1 discipline: no new newtypes (dates mirror `anchor`'s plainness);
  any count input in the flow rides the existing `NonZeroCount` mint.

## 3. `paused_until` semantics — the recorded answer (dispatch §3's named questions)

- **Who sets it**: only the user, through `PauseSeries` (UI task-view
  row / CLI `--pause`); the mint paths (`repeat`, `completion_spawn`)
  always write `paused_until: None`; the engine never writes it
  (resume is date-derived, D-6).
- **What the attribution layer does inside the window**: members
  inside `[anchor … paused_until]` do not exist — `schedule` omits
  them, so no missed-day derivation, no streak break, no spawn target,
  no roll target can land inside the window (D-4's five integration
  points, each pinned). Completions during the window attribute to the
  last live member before it (window closes at the next live member).
- **Resume**: no miss, no credit for window members (they never
  existed); the first post-pause member resumes the chain on its
  normal pattern date (phase preserved — the anchor never moves, so
  weekly/monthly cadence stays on its weekday/day-of-month); an
  expired `paused_until` is inert; the explicit resume affordance
  clears the field (D-6).

## 4. Crate-by-crate work list (stage 2's build order)

1. **taskboard-domain — pause** (TDD: pins first): `RecurRule
   .paused_until` + serde default pin; `next_live_after` +
   `schedule`/`occurrence_outcome`/`next_occurrence` integration;
   `command.rs` spawn `next_date` + roll target through the live
   law; proptest extensions (D-4); `PauseSeries` message + planner
   arm + no-op/typed-rejection pins; undo: classification-table
   PreImages row + `command_class` mapping + inverse reuse pins.
2. **taskboard-domain — convert**: `ConvertSeriesToHabit`/
   `ConvertHabitToSeries` messages + planner arms (emission mirrors
   `CreateHabit`/`CreateTask`+`DeleteHabit`); round-trip pins
   (P-C1/C2); `NotUndoable(HabitSurface)` classification arms with
   the D25-extension comment.
3. **taskboard-app**: `--pause/--resume` flags, `RepeatInvocation::Pause`,
   collapse + receipt; `tasks repeat` typed-refusal parity tests.
4. **taskboard-ui-slint — models/presentation**: `recur_badge`
   paused fields + label phrase; `habit_repair_target`; row-source
   extensions (`HabitRow` repair fields; task-detail recur-row model);
   presentation constants; structural tests.
5. **taskboard-ui-slint — seams/actions**: task-view recur row
   (`task-view.slint` + date-picker mount for pause), habits.slint
   make-up/convert affordances, confirm overlay composition,
   `ActionId`s + router arms + adapter one-shot ctx + seam installs,
   kiosk gates unchanged; journeys for every new element (blanket
   rule).
6. **CHANGELOG** Unreleased entries (all three items).

## 5. Test/pin catalogue (T-numbers referenced above)

- **P-pause** (domain): serde default decode of field-less JSON;
  round-trip both shapes; `schedule` skips exactly the paused range
  (unit + property: paused ≡ pattern-minus-members on generated
  `(pattern, anchor, pause)` triples); `occurrence_outcome` window
  closes at next live member (mid-pause completion attributes to the
  pre-pause member); `next_occurrence` stale/fresh/dateless arms with
  pause; pause past `Until` → `date: None`; spawn skips to the first
  live member (engine arm test on the state machine); roll lands the
  head on the first post-pause member mid-pause (day-tick journey,
  paused clock); expired `paused_until` inert (equivalence pin);
  `habit_streak_unioned` crosses a pause unbroken.
- **P-PauseSeries** (domain): pause sets only the field (pattern
  byte-identical); resume (`until: None`) clears; typed rejections
  (unknown/not-live/no-rule); repeat command = accepted no-op;
  undo restores the pre-rule (recur pre-image inverse); redo
  re-applies.
- **P-convert** (domain): S→H — head rule cleared + habit minted
  linked + order appended; H→S — head minted in Backlog with linked
  rule + habit tombstoned + log preserved; round-trip S→H→S′→H′ chain
  readable from state (the load-bearing pin, dispatch §3);
  typed rejections (not-recurring head / not-live habit); undo
  refuses `NotUndoable(HabitSurface)` on both; sync-op emission
  mirrors the manual path (op-for-op assertion).
- **P-cli** (app): `--pause/--resume` collapse to `PauseSeries`;
  conflicts rejected by clap; receipt structural pins; typed refusal
  parity with `Set`.
- **P-ui** (ui-slint): badge paused chip (today ≤ / > paused_until);
  `habit_repair_target` grace/under-quota matrix vs `log_window`'s
  decision (read-only agreement pin); row/repair/convert button
  gating (`habit_editing_visible`); router arms dispatch exact
  commands from one-shot ctx (the `habit_verbs_route` test shape);
  confirm-overlay confirm/cancel journeys; blanket-rule journeys:
  every new element hover-tip armed + walk stop + action-layer
  dispatch; kiosk journey: rows render, new actions never mount.

## 6. Risks & fences

- **R1 — the fence**: item (iii) touching attribution functions is a
  stage-3 review gate (diff `habit.rs` attribution fns — must be
  empty). If a pin *seems* to require it, that's a deviation to
  report, not make (dispatch stage-2 rule).
- **R2 — spawn/roll regression surface**: the pause touches the two
  most load-bearing series arms; the existing DR-6 suite (proptests
  1–8, engine snapshot/insta pins) must stay green untouched — any
  edit to an existing pin to admit the pause is a finding. The new
  arithmetic enters only through `next_live_after` with
  `paused_until: None` ≡ today's behavior (equivalence proptest).
- **R3 — task view is r4-heavy**: `task-view.slint` just absorbed
  rounds 3–4 (layout/focus/datetime). The recur row is append-only
  (new section, no rework of landed rows); the walk-stop inventory
  extends at the end. Rebase friction with merged r4 tail: none
  (all merged); with round-3 batch-3/4 ui chunks (3.1/4.x): the
  territory rule (soft) applies — batch gates keep them sequential.
- **R4 — `models.rs`/`adapter.rs` churn**: shared with later round-3
  ui chunks; keep the append-last discipline (the badge decoration
  precedent) and one-shot ctx additions contiguous.
- **R5 — overlay count**: a new confirm overlay must respect the
  one-overlay rule and the drag exclusion; reuse the activation-stop
  widget; no new dismissal semantics.
- **R6 — undo refusal UX**: converts are journal-visible but
  undo-refusing; the CLI/UI undo paths already render typed
  non-undoable reasons — no new surface, but the plan records the
  expectation so review doesn't read it as a gap.

## 7. Commit plan (stage 2; every commit standalone)

1. `feat(domain): the series pause arithmetic` — D-4 field + live-member law + five integration points + pins/proptests.
2. `feat(domain): the series pause verb` — `PauseSeries` + planner + undo rows + pins.
3. `feat(app): the repeat pause flags` — CLI flags/collapse/receipt + tests.
4. `feat(domain): the series-habit convert verbs` — both verbs + planner emissions + round-trip/undo pins.
5. `feat(ui-slint): the recur row and the pause affordances` — badge chip, task-view recur row, pause journeys.
6. `feat(ui-slint): the convert flows` — confirm overlay, habit-row convert, router/adapter seams, journeys.
7. `feat(ui-slint): the streak repair affordances` — `habit_repair_target`, make-up button, +1 hint, journeys.
8. `docs(changelog): the habit ux trio` — Unreleased entries.

(Stage 2 may fold 3 into 2 or 5 into 4 if a group lands small — the
order and standalone-build rule bind, not the count.)

## 8. Quality gates & bookkeeping

- Full AGENTS §10 suite via `scripts/run-quality-gates.sh`; no
  clippy warnings, no fmt drift at handoff.
- `cargo llvm-cov` three-command sequence with the quarantine script;
  no quality ceilings in new pins.
- No migration (F9); no new crates; workspace dependency pins
  untouched.
- Issue #506 stays open until stage 3 closes it with the PR link +
  evidence (dispatch §3 stage-3 prompt). Worktree teardown per
  dispatch §1 step 3 after PR-green.

## 9. Estimated size

Domain ≈ 700–1000 lines incl. tests (recur.rs arithmetic + two
command arms + undo rows + proptest extensions), app ≈ 120–180
(repeat flags/collapse/receipt), ui-slint ≈ 450–700 (recur row,
overlay, affordances, seams, presentation + journeys). **Total
≈ 1.3–1.9k lines**, the chunk's M size honored.
