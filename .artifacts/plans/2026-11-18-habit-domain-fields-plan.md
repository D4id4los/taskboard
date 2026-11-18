# [Plan] Habit Domain Additive Fields (H1, round 9 chunk 4)

- **Date**: 2026-11-18
- **Kind**: implementation plan — TDD-first, single chunk (round plan
  chunk **H1**, branch `feat/domain-habit-fields`), stage-1 planning
  artefact for the three-stage dispatch protocol.
- **Origin**: J1 UX journey plan
  (`.artifacts/plans/2026-11-18-habit-parity-ux-plan.md`, ACCEPTED —
  US-6 + R-2 normative); round plan §1 O-1 **(SUPERSEDED — option (b))**
  and O-7; research
  `.artifacts/reports/2026-11-18-round9-habit-parity-research.md`
  §HAB-2, §Q1; type-driven policy AGENTS §2 / `docs/coding_guidelines.org`
  §2.1.
- **Status**: PLANNED — awaiting stage-2 implementation.
- **Scope**: additive domain wave only. **No UI code, no streak/window
  behavior change** (the walk at `crates/taskboard-domain/src/habit.rs:328-339`
  — `period_start` — and `streak`/`log_window` must remain **byte-identical
  in behavior**; pinned by property tests below). GUI row rework is H2;
  the editing surface and the reminder-engine wiring of the nudge law
  are H4.

## 1. Recorded decisions (settled at planning, per dispatcher instruction)

### D1 — Optional-day representation: typed `Option<AnchorDay>` (A1-minted payload)

Option (b) — the day is a **display/nudge day inside the canonical
window**; `WindowStart`, `period_start`, `period_after/before` are
untouched. The field carries meaning, so it gets a hand-rolled A1 type,
not a bare `u32`:

```rust
/// A display/nudge anchor day inside the canonical window (O-1 option b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorDay {
    /// For a `PerWeek` habit: the ISO weekday the nudge arms on.
    Weekday(chrono::Weekday),
    /// For a `PerMonth` habit: the calendar day (1..=31) the nudge
    /// arms on. A1-minted — 0 and >31 are unrepresentable.
    MonthDay(MonthDay),
}

/// Hand-rolled A1 newtype: private field, fallible mint rejecting 0
/// and >31. Strict decode (the `NonZeroCount` posture — `try_from`).
pub struct MonthDay(u8);           // new(1..=31) -> Result<Self, InvalidMonthDay>
```

- Home: `crates/taskboard-domain/src/habit.rs`, next to `NonZeroCount`.
- Documented leniency (at the type, per §2.1 "deliberate leniency
  documented"): a `Some(anchor_day)` on a `PerDay` habit, or a
  `Weekday(..)` on a `PerMonth` habit, is **inert** — `nudge_day`
  (below) reads the variant matching the current target kind and falls
  through otherwise. No validate-and-forget: the planner accepts the
  field as-is (it is display data, not window data); mismatched
  variants simply never influence anything. The *CLI* refuses
  kind-mismatched `--day` values with a typed error at parse time
  (cheaper feedback at the trust boundary), but the domain does not
  police it.
- Storage on `Habit`: `pub anchor_day: Option<AnchorDay>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` (the
  `by`-slot additive precedent).
- New LWW clock slot: `HabitClocks.anchor_day: DateTime<Utc>`
  (**the field is contested content** — it roams and competes; unlike
  `continued_from` (D-H7a seam data) it changes through ordinary
  editing). `HabitClocks::all` stamps it with `now`. This grows the
  effective-clock max-list in `merge.rs::effective_clock_of` and
  `appdata.rs::competition_clock` — both must add the slot (5→6
  clocks).

### D2 — Description field shape: plain `String` (task precedent)

`Task.description` is a plain `String` with an empty default; the habit
field follows it exactly:

- `pub description: String` on `Habit`, `#[serde(default)]` so
  pre-field synced JSON decodes (lenient additive decode, the
  `by`-slot precedent). No bound, no newtype: no invariant to enforce
  (the A1 ladder starts where meaning demands it; a free-text note
  carries none).
- `HabitChanges.description: Option<String>` — plain single option
  (no clear-vs-untouched ambiguity: `Some("")` clears; the task
  changeset precedent).
- New LWW clock slot `HabitClocks.description` (same reasoning as D1).

### D3 — Sync wire versioning: additive default-absent fields, no version bump

- `HabitClocks` gains `description` + `anchor_day` — it is a plain
  serde struct embedded in `HabitRecord::Live`; older devices that
  hold a *decoded* copy only round-trip state through their own
  version, and the JSON of a new device carries the extra clock keys.
- `HabitRecord::Live` (crates/taskboard-domain/src/appdata.rs:952)
  gains `description: String` and `anchor_day: Option<AnchorDay>`,
  both serialized always (mirroring `title`/`order`, not the `by`
  skip-if-none form, so the row shape stays greppable) but decoded
  with `#[serde(default)]` — an older device's row (fields absent)
  decodes to `""` / `None`.
- **Old readers**: `HabitRecord::deserialize` is the tolerant boundary
  — a `Live` object carrying unknown keys fails the known-shape decode
  and lands in `Verbatim(Value)`, preserved verbatim and never folded.
  That is the existing, tested posture for shape growth; no wire
  version field exists or is added. Consequence recorded here: devices
  on pre-field builds see post-field habit rows as inert verbatim
  content until they upgrade — same class as every prior field-family
  migration (0007, 0010).
- `habit.rs` `Habit` serde is internal-to-storage/state (state
  snapshot, journal) — `#[serde(default)]` on both new fields keeps
  old journals and snapshots loadable.

### D4 — Census supersession note text (for `docs/ux-surface-census.org`, landed by the round-9 census pass, not by this chunk)

> *Habit window/anchor ruling (operator, 2026-11-18, O-1 as superseded
> by the later same-day pass = habit-report Q1 = option (b)):* the
> optional per-week/per-month day is a **display/nudge day inside the
> canonical window** — windows stay ISO-Monday / calendar-1st and the
> D-H12 window-canonicality law is **reaffirmed, not reopened**; the
> streak walk and the `(habit, window)` log key are unchanged (pinned
> by property). The day never influences ordering (the
> `anchor_time_never_influences_the_canonical_order` law extends to
> `description` and `anchor_day`). habit.edit's R-2 "CLI covers"
> cells are already superseded by the habit-parity UX plan (J1,
> 2026-11-18; R-2 overturn) — H2/H4 land those GUI cells; this chunk
> lands only the CLI/domain substrate (CLI cells stay E, now with
> `--description`/`--day`). Authority:
> `.artifacts/plans/2026-11-18-habit-parity-ux-plan.md`,
> `.artifacts/plans/2026-11-18-habit-domain-fields-plan.md`.

## 2. Work breakdown (TDD-first: every phase starts with its failing test)

### Phase 1 — Domain types + mints (`taskboard-domain/src/habit.rs`, `messages.rs`)

1. **Tests first**:
   - `MonthDay` mint unit tests: rejects 0 and 32, accepts 1/31,
     strict serde decode (`serde_json::from_str::<MonthDay>("0")`
     errors — the `NonZeroCount` posture test).
   - `AnchorDay` round-trip + externally-tagged wire form
     (`{"Weekday":"Mon"}` / `{"MonthDay":17}`).
   - `Habit` serde round-trip with the new fields; a *pre-field* JSON
     document (fixture string without `description`/`anchor_day`) still
     decodes (default-absent law).
   - `HabitChanges` double-option serde test for `anchor_day`
     (explicit `null` → `Some(None)` clear; absent → `None`
     untouched) — the `anchor_time` test twin.
   - **Window-invariance property tests** (the load-bearing pins, see
     §3): `anchor_day`/`description` never influence `period_start`,
     `log_window`, `streak`, or ordering.
2. Implement: `MonthDay` + `InvalidMonthDay` (`thiserror`), `AnchorDay`,
   `Habit.description`, `Habit.anchor_day`, `HabitClocks` slots (+`all`),
   `HabitChanges` fields (description single option; anchor_day double
   option with `deserialize_double_option`), `Default` for
   `HabitChanges` stays derive-able.
   Update the ~15 fixture `Habit { .. }` literals in
   `habit.rs` tests/`test_support` (`habit_strategy`,
   `habit_fixture_strategy` gain the new fields — randomized
   `anchor_day` variant/kind pairing for the invariance props).
3. Planner (`command.rs` `UpdateHabit`): extend the mask arms —
   description diff → `clocks.description = now`; anchor_day diff →
   `clocks.anchor_day = now`; unchanged-mask → `no_op()` (existing
   zero-action law test extends to the new fields). `CreateHabit`
   unchanged (both mint to empty/None — additive defaults).

### Phase 2 — Merge/fold + wire (`merge.rs`, `appdata.rs`)

1. **Tests first**: `fold_habits` adopts `description`/`anchor_day`
   wholesale on a strictly-newer row; equal-clock tie keeps local
   (the tuple-duel test extends); `competition_clock`/effective clock
   include the two new slots (a row differing only in a newer
   `anchor_day` clock wins the duel); old-shape `Live` JSON decodes
   with defaults; new-shape JSON emitted contains both keys.
2. Implement: `HabitRecord::Live` fields, hand `Serialize` block,
   tolerant `deserialize_verbatim` known-shape matcher, `fold_habits`
   adoption sites, `effective_clock_of`.

### Phase 3 — Storage (`taskboard-storage-sqlite`)

1. **Tests first**: round-trip a habit with description + anchor_day
   through the repo (connect.rs test style); a pre-migration DB (fixture
   at the 0026 schema) upgrades and old rows read
   `description = ""`, `anchor_day = NULL`.
2. Implement: `migrations/0027_habit_description_anchor_day.sql`
   (`ALTER TABLE habits ADD COLUMN description TEXT NOT NULL DEFAULT '';
   ADD COLUMN anchor_day TEXT; ADD COLUMN ck_description TEXT;
   ADD COLUMN ck_anchor_day TEXT;` — the 0002/0026 columnar precedent,
   no data rewrite), `repo.rs` decode/encode rows + the SELECT at
   repo.rs:1194, upsert statement.
   `anchor_day` TEXT encoding: the serde JSON form of `AnchorDay`
   (one canonical string form; NULL = None).

### Phase 4 — CLI (`taskboard-app/src/commands/habits.rs`)

1. **Tests first**: `habits add --description "..."` /
   `habits update x --description none` (clear) / `--day mon` /
   `--day 17` / `--day none` parse tests; typed refusal when `--day`
   variant mismatches the target kind at *edit* time only when the
   kind is known-and-mismatched (parse-time check against the
   concurrent `--weekly/--monthly` flag; when absent, the planner's
   inertness is the answer — no cross-command lookup in the parser).
   `habits list` gains a description column only when non-empty
   (structural: row carries the field, not text assertions).
2. Implement: clap flags + mapping into `HabitChanges` /
   `CreateHabit` untouched (create keeps no-description; the add verb
   may accept `--description` by routing an immediate sparse
   `UpdateHabit` after create — the `--at` precedent at
   habits.rs:287-294).

### Phase 5 — Nudge-day derivation (pure domain; H4 consumes)

1. **Tests first** (unit + property): `nudge_day(habit, window_start)
   -> NaiveDate` — for `PerDay` always the window date; for `PerWeek`,
   the window date when no anchor day or `Weekday::Mon`, else the
   in-window date matching the weekday; for `PerMonth`, the window
   date when none, else min(`MonthDay`, month length) (clamped — a
   `--day 31` in February arms on the 28th/29th; documented leniency).
2. Implement as a pure fn in `habit.rs`. **No reminders.rs change in
   this chunk** — the engine consumption (arm relative to
   `nudge_day`, else period start; O-1 nudge law) is H4's; this chunk
   only ships the total, typed derivation H4 calls.

### Phase 6 — Gates

`scripts/run-quality-gates.sh` wrapped: fmt, clippy `-D warnings`,
nextest workspace, doc tests. Subagent scope per dispatch discipline:
pre-commit hooks + flock-wrapped nextest only; dispatcher owns the
coverage trio.

## 3. Property tests pinning window invariance (US-6 AC-1)

In `crates/taskboard-domain/src/habit.rs` (tests module, the existing
`habit_fixture_strategy` harness), all with randomized
`description`/`anchor_day` on the fixture habit:

1. **`period_start` invariance**: for every kind/date,
   `period_start(kind, d)` is byte-equal before/after setting
   `anchor_day` + `description` on a habit — trivially true at the
   free-function level; the real pins are the consumers:
2. **`log_window` invariance**: `log_window(&perturbed, log, now, day)
   == log_window(&base, log, now, day)` over the fixture strategy —
   the +1 attribution never reads the new fields.
3. **`streak` invariance** (the habit.rs:328-339 walk contract):
   `streak(&perturbed, log, now, day) == streak(&base, log, now, day)`
   across the in-day→+2.3-day probe grid (the existing probe
   strategy).
4. **`habit_streak_unioned` invariance**: same equality over the
   linked-series seam harness.
5. **Ordering inertness** (extends
   `anchor_time_never_influences_the_canonical_order`): two habit
   vecs differing only in `description`/`anchor_day` sort to the
   identical `(order, id)` sequence.
6. **Nudge law constructive pin** (positive direction, keeps 1–5
   honest): with `Weekday::Wed` on a `PerWeek` habit,
   `nudge_day` of the ISO-Monday window *is* that window's Wednesday —
   the canonical window start itself is still Monday
   (`period_start` unchanged), proving "day inside the window", not
   "day shifts the window".

## 4. Risks / watch items

- **Effective-clock list growth touches LWW competition in two
  places** (`merge.rs::effective_clock_of`,
  `appdata.rs::competition_clock`); missing one desynchronizes duel
  outcomes vs stored rows. Both get explicit test pins (Phase 2).
- **Old-device wire compat**: post-field rows decode as `Verbatim` on
  pre-field builds (D3) — accepted posture, but worth one line in the
  PR description so it is not misread as data loss.
- **Fixture churn**: ~15+ `Habit` literals across
  domain/state/merge/storage tests need the two new fields; mechanical
  but wide — keep it in the same commits as the field addition so
  every commit stands alone (rebase-merge hygiene, AGENTS §8).
- **`HabitClocks` shape change invalidates persisted `ck_*` column
  count** — migration 0027 must land before any binary that writes
  the new clocks reads an old DB; single-PR atomicity covers it.
- **MonthDay clamping** (Feb 31 → Feb 28) is a documented leniency,
  not silent truncation of intent; called out at `nudge_day`'s doc
  comment and pinned by a unit test.
- Out-of-scope observations go to backlog issues (§7 standing
  authorization): see filed issues in the stage report.

## 5. Diff estimate

| Area | Files | +/− lines |
|---|---|---|
| domain habit.rs (types, planner-adjacent, props) | 1 | ~+420 (≈45% tests) |
| domain messages.rs (HabitChanges) | 1 | ~+25 |
| domain command.rs (UpdateHabit arms + tests) | 1 | ~+70 |
| domain merge.rs / appdata.rs (+ tests) | 2 | ~+140 |
| storage migration + repo.rs + connect test | 3 | ~+120 |
| app CLI habits.rs (+ tests) | 1 | ~+130 |
| test_support fixtures | 1–2 | ~+30 |
| **Total** | ~10 | **≈ +900, −20** |

## 6. Definition of done

- [ ] All Phase 1–5 tests present and green (TDD order followed)
- [ ] Window-invariance properties (§3 items 1–6) land in the same PR
- [ ] `period_start` (habit.rs:328-339) untouched — verified by
      `git diff` showing no change inside the function
- [ ] No UI crate change; no reminders.rs change
- [ ] Quality gates green via `scripts/run-quality-gates.sh`
- [ ] Census note (D4) queued for the round-9 census pass (not landed here)
