# Source-of-Truth Granularity Audit

- **Date**: 2026-11-27
- **Kind**: analysis report (backward-looking audit), operator-ordered
  (ruling 2026-11-27, context #1340/#1341/#1519 and the round-8 dispatch
  report §"two sources of truth").
- **Scope**: every keyed store, persistence action, journal inverse, and
  merge fold in the workspace, examined for one defect class. Recorded
  reality cites `crates/...` line numbers at `origin/main`
  (`9fc0d5ee`).
- **Outcome**: 6 findings — 3 S1, 3 S2, 3 S3 (some findings carry
  multiple severities; the executive table is the authority), 1 S4.
  The survey also produced **anti-examples** (§6) — stores that already
  sit at the right granularity and must not be "fixed" — and one
  unverified hunch (§7). Issues opened: #1520–#1524.

## 1. The defect class

> A data store whose rows aggregate above the granularity its consumers
> need forces bolted-on twin/overlay structures, lossy inverses, and
> non-convergent merges.

Three smells detect an instance:

1. **A twin** — a second structure holding overlapping truth the store
   cannot express (S4), kept coherent by transactional discipline
   inside every writer (the discipline is the confession).
2. **A clock-arbitrated fold over a value that is really a sum/set** —
   LWW on an aggregate row silently drops concurrent increments
   (S1).
3. **An inverse that refuses, approximates, or "documents leniency"** —
   an undo/correction cannot be expressed exactly in the row shape
   (S3), or a consumer derives a fact the store cannot answer (S2).

The founding case (§F-1) exhibits all three. The ruling (#1519,
operator 2026-11-27) already endorses the primitive-shape refactor for
it; this audit supplies the sibling findings that may ride the same
wire-version bump and the ones that must not.

---

## F-1 — Habit completion store: window-aggregated count rows + the
### day-overlay twin (S1 + S2 + S3 + S4) — the founding case

**Status**: refactor operator-endorsed in principle (#1519); this
section is the full design-round brief. Supersedes the E3 design
(`.artifacts/plans/2026-11-25-habit-sync-tails-design.md`, superseded
by `9fc0d5ee`'s commit).

### Mechanism (recorded reality)

- The oracle keys counts per **window**:
  `HabitLogKey { habit, window: WindowStart }` →
  `HabitLogEntry { count, clock, by, skip }`
  (`crates/taskboard-domain/src/habit.rs:634`, `:740`; state at
  `habit.rs:835` `HabitState.log`).
- A Week/Month row aggregates the days away: *which calendar day* a
  completion landed on is inexpressible in the oracle (S2).
- The day attribution was bolted on as a second structure —
  `HabitDayKey`/`HabitDayLogEntry` (`habit.rs:680`, `:727`), the
  `day_log` map (`habit.rs:839`) — written transactionally by
  `LogHabit` (`command.rs:3226–3283`, both `RecordHabitLog` and
  `RecordHabitDayLog` in one batch, the "transactional-twin law") and
  read by the heatmap
  (`crates/taskboard-ui-slint/src/presentation.rs:8796`
  `habit_history_day_cells`, wired at
  `crates/taskboard-ui-slint/src/models.rs:7576`). The twin is
  device-local (no sync lane, `persistence.rs:520–530`), producing
  #1340 (S4/S2).
- The window row is LWW-folded everywhere —
  `apply_actions` `RecordHabitLog` (`persistence.rs:514–519`), the
  sqlite upsert guard
  (`crates/taskboard-storage-sqlite/src/repo.rs:2903–2910`), and the
  wire fold `fold_habit_log` (`merge.rs:1521–1561`, the
  `(clock, by)` tuple duel). Two devices each logging on the same
  window write `count = 2`; the merge keeps one — **a completion
  silently vanishes** (S1). The `mint_log_clock` monotone bump
  (`habit.rs:801–809`, #986) papers over same-device same-instant
  double logs, not cross-device ones; the whole-second sqlite guard
  (#1095, `repo.rs:2894–2902`) widens the loss window further.
- The undo inverse reuses `AdjustHabitLog`: exact for Day-kind
  windows, day-attribution-preserving for Week/Month (#1341), and an
  absent-window adjust **refuses undo entirely**
  (`NotUndoableReason::HabitSurface`, `journal.rs:131`, classified at
  `undo.rs:421`/`:442`, D24: a `count: 0` inverse would forge an
  explicit count-0 row where the pre-state had row-absence) (S3).
- The row shape also strands **former-kind rows** as analytically
  inert luggage after a target-kind change (D-H12 documented
  leniency, `habit.rs:843–855`): keys are minted from the *current*
  kind's period floor, so a kind change orphans every old window key
  permanently.

### Primitive refactor (per-completion event log)

Each completion is one event row: `(habit, event uuid, completion
instant, author)` — the #1519 endorsed shape. Quota checks, streaks,
the day heatmap, and Week/Month windows all **derive** at read time by
counting events in the window. Exact union merge by uuid; exact
inverse (remove event by id); the twin and its transactional law are
deleted; the D-H12 luggage and the D24/D25 refusal fence dissolve.

**Design-round brief** (the decisions a design round must take):

1. **Event id minting**: uuid minted *at log time* (never derived from
   the completion instant — multiple completions on the same second
   stay distinct). Minting must guarantee local total order: adopt the
   process-global `uuid::ContextV7` counter in `idgen.rs` (#1488) as
   part of this work, since event-log folds and GC want `(instant,
   id)` determinism.
2. **Union-merge fold**: wire section keyed by event uuid; fold is
   set-union (adopt absent uuids; identical uuids are the same fact —
   no clock arbitration anywhere in the fold). The `by` author rides
   the event row for display, never for arbitration. The
   `HabitLogRecord` wire row and `fold_habit_log` are replaced; the
   180-day wire retention (D-H14) re-bounds to the new GC window.
3. **GC / retention window**: bounded by max(heatmap horizon, streak
   lookback) + margin — the `lane_gc_old_habit_time_entries` wire-only
   precedent (`push.rs`). Events past the horizon are dropped from the
   *wire* document (local history may keep them longer; the heatmap
   horizon defines what surfaces).
4. **Window derivation at read time, including the grace rule**: the
   `log_window` attribution (`habit.rs:913–967` — natural window,
   24 h grace, surplus-repair guarded on "natural already met quota")
   becomes a read-time derivation: each event attributes to the window
   `log_window` would have chosen *as of the event's completion
   instant* (pure, deterministic, recomputed on kind/config change).
   The frozen-at-log attribution disappears, so a later habit-kind
   change or day-start change re-derives history instead of stranding
   it. The `skip` mark becomes a **per-window derived posture or a
   typed skip event** — the design round must pick (see F-2).
5. **Persisted-row migration**: persisted window rows synthesize
   N timestamped events per count (display-acceptable attribution:
   spread within the window or stamped at the row clock — design-round
   decision), sqlite migration replacing `habit_log`/`habit_day_log`
   (`repo.rs:2903`, `:2936`; migrations 0028/0030 become
   load-then-remap), wire codec versioning for the `habit-log`
   section (old documents fold under a conversion shim or are let to
   age out under retention — the cheaper option, retention-bounded).
6. **Consumer churn list**: `command.rs` LogHabit/AdjustHabitLog/
   SkipHabit/UnskipHabit arms (3226–3436); `habit.rs` streak/quota
   projections (the `count`/`skipped` closures at
   `habit.rs:1000–1090`); `merge.rs` `fold_habit_log`; `persistence.rs`
   both Record actions; `repo.rs` load/flush rows + rewind; ui
   `presentation.rs:8796` + `models.rs:7576` (heatmap derives from
   events); journal `PreImage::HabitLogRow` (`journal.rs:171–175`) →
   per-event pre-image or event-id deletion inverse; app CLI/summary
   projections in `crates/taskboard-app/src/commands/habits.rs`.
7. **UX law**: quota/streak display semantics are user-visible — the
   ux_planning framework applies; the design round must carry a UX
   journey plan before the dispatch plan chunks implementation.

### Blast radius

Domain (habit.rs, command.rs, merge.rs, persistence.rs, journal.rs,
undo.rs), storage (repo.rs + one migration), both sync connectors'
board cycles (wire codec + fold), UI presentation (heatmap, streak,
quota badges), app CLI habit surfaces, golden corpora and snapshots.
This is the largest-radius finding; it is also the one with an
existing p1 issue (#1519) and a superseded design to build on.

---

## F-2 — The skip mark shares the aggregate row and its single LWW
### clock (S1)

**Mechanism**: `SkipHabit`/`UnskipHabit` rewrite the *whole* window
row (`HabitLogEntry { count, skip: true/false, clock: mint_log_clock,
… }`) at `command.rs:3374–3404` and `:3406–3436`; the fold is the same
row-LWW tuple duel as a log (`merge.rs:1537–1544`,
`persistence.rs:514–519`, `repo.rs:2903–2910`). Device A logs
(count 2, clock t1); device B skips (count 2, clock t2 > t1) — the
merge keeps B's row and A's **increment survives** (count carried), but
reverse the clocks and A's log is kept while... both directions carry
count; the real loss is **LogHabit vs AdjustHabitLog vs Skip on the
same key**: an audited correction `count = 0` (clock t3) silently
erases a concurrent increment, and a skip minted after a burst
(`mint_log_clock` bumps past the burst) can out-rank and stand on a
row whose count the skipper's device never saw. Concretely: any two
whole-row writers on the same window key where one's *count* is stale
— the stale count wins on the clock and the newer increment is
dropped (S1, the same fold-laws as F-1's headline case but through the
skip/correction writers).

**Primitive refactor**: dissolves inside F-1 (per-window skip becomes
a derived posture or a typed skip event; corrections become event
deletions/additions). If F-1 is delayed, the standalone mitigation is
splitting the row's `count` and `skip` into independently clocked
fields — but that is a half-measure that keeps the increment-loss
fold; recommended path is F-1 only.

**Blast radius**: same files as F-1, strictly smaller (no heatmap/UI
churn beyond skip badges). **Ride F-1's wire bump.**

---

## F-3 — Absent-window correction inverse is refused (HabitSurface
### fence) (S3)

**Mechanism**: `NotUndoableReason::HabitSurface` (`journal.rs:131`,
`undo.rs:421`, `:442`, D24/D25): an absent-window `AdjustHabitLog`
refuses undo because a `count: 0` inverse would forge an explicit
count-0 row where the pre-state had **row-absence**; an `UpdateHabit`
at the kind-conversion seam refuses because there is no clock slot to
watermark against. Row-absence vs count-0-row is a distinction the
store cannot express *through an inverse* — the lossy-inverse class
exactly. The LogHabit fence itself was removed (R-7,
`journal.rs:136–141`), so this is the surviving residue.

**Primitive refactor**: F-1's event log makes the inverse exact
(remove the minted event by id); the conversion-seam refusal dissolves
because windows are derived, not keyed by kind. **No standalone
migration** — schedule inside F-1 and close the fence variant there.

**Blast radius**: journal.rs/undo.rs only, riding F-1.

---

## F-4 — TimeEntry aggregates the whole span under one mutation clock
### (S1 + S3)

**Mechanism**: `TimeEntry { task, task_title, start, end: Option,
created_at, updated_at, kind, by }` (`tracking.rs:38–85`) is one row
with one LWW clock; `fold_time_entries` overwrites
`task/task_title/start/end/kind` **wholesale** when the remote
`(clock, by)` tuple outranks local `updated_at` (`merge.rs:1182–1258`,
the adoption arm at `:1210–1227`); `apply_actions`/sqlite upsert the
row by id (`persistence.rs:493–494`). Device A adjusts the start while
offline; device B stops the entry (stamps `end`) — one whole-row
write wins, the other's field mutation is silently dropped (S1).
The inverse side: an `AdjustTimeEntry` cannot clear a stamped end and
no command re-opens a closed entry (ADR 0017; journaled as
`TrackingSurface`, `journal.rs:124–130`, `undo.rs:410`/`:450`/`:456`,
D23) — a reopen is inexpressible as an inverse (S3). The
`HabitTimeEntry` row (`habit_time.rs:40`) repeats the shape and
inherits the class.

**Primitive refactor**: per-endpoint stamps — the Task entity's
per-field `Clocks` struct is the in-house precedent
(`entities.rs`): `start_stamp`, `end_stamp` (each `(clock, by)`),
fold per field instead of per row; reopen clears `end` by stamping
`end = None` with a fresh `end_stamp` (tombstone-per-field, the same
discipline the task `deleted` clock uses). The event-log alternative
(open/adjust/stop events) is strictly more general but heavier;
per-field clocks suffice because a span has exactly two mutable
endpoints and the row identity already exists. Migration: synthesize
both stamps from `updated_at`; wire `TimeEntryRecord` gains a stamp
pair (additive, default-absent law); old documents fold
clock-only — one cycle of reduced precision, then convergence.

**Blast radius**: domain tracking.rs/habit_time.rs, merge.rs two
folds, persistence.rs, repo.rs (+ migration), push.rs wire rows,
journal TrackingSurface fence (reopen becomes expressible —
re-classify), app track CLI surfaces. Moderate; can ride the same
wire-version bump as F-1 or its own.

---

## F-5 — Reflection entries freeze the window at mint time; the
### journaling instant is inexpressible (S2)

**Mechanism**: `ReflectionEntry.window_start` is computed at mint time
(`reflection.rs:291–301`, the field contract at `:294–301`) and the
stats membership rule keys on the **stored** value, never derives
(D-B2, `reflection_stats.rs:15–23`) — the limitation is recorded, not
papered over: a Week/Month entry stamps at its *period floor*, so the
reviewed instant is wrong by up to a month and the true journaling
instant exists nowhere in the row. Any future roaming/sync fold or
ordering need re-derives nothing — the fact is gone (S2: the consumer
needs a fact the store cannot express; the store aggregates the
instant to the window).

**Primitive refactor**: store the journaling instant (`at:
DateTime<Utc>`) as the row's primary stamp and **derive**
`window_start` at read through the existing `review_window`
arithmetic (`reflection.rs:615–645` — the single flooring definition
already exists). The family is device-local today (`reflection.rs:5`
"device-local by ruling, roaming-ready by design"), so the migration
is one sqlite column + decode default; no wire section exists yet —
this is the cheap moment to fix the row shape *before* roaming lands.

**Blast radius**: reflection.rs row + record command
(`command.rs:3488–3548`), reflection_stats membership rule, repo.rs
(+ migration), review view surfaces. Small now; expensive later.

---

## F-6 — Time entries cannot distinguish focus-mode spans (S2,
### class-adjacent)

**Mechanism**: recorded as known limitation D-S5
(`tracking_stats.rs:28–32`): the focus runtime stamps nothing into
`TimeEntry`, so a tracked hour under focus mode and a plain hour are
indistinguishable in history; "work blocks" are kind-marked entries,
never focus spans, and real focus-session statistics "would need an
entry-level marker (a future entity change)". The consumer need is
recorded; the store cannot express it; the pressure resolves either
as a bolted-on twin (the class) or a row-shape change (the cure).

**Primitive refactor**: one additive enum field on `TimeEntry`
(`origin: Plain | Focus`, default-absent serde) — or, if F-4 lands
first, an attribute on the endpoint stamps. No wire break: additive
default-decodes. Design note: decide only when a consumer actually
exists (the D-S5 text fences it); the audit's contribution is that
the *row* is the right home, so no twin is started.

**Blast radius**: tracking.rs + wire row + one migration, additive.
**Issue only; no design round until a consumer is scheduled.**

---

## F-7 — Habit pause freezes on the habit row; the paused-window
### attribution is re-derived nowhere (S2, minor, watch)

**Mechanism**: `PauseHabit { until }` is read-modify-write on the
habit row (`command.rs:3437+`, the PauseSeries posture). The pause is
a *span* fact stored as a scalar deadline: a pause edited or undone
across devices folds LWW on the habit row, and paused *windows* that
fell inside a now-edited pause span are neither marked nor unmarked —
the streak projection consults the scalar against `now` only. If
pause history ever needs attribution (which windows were legitimately
paused), the scalar cannot answer it and a window-level twin becomes
tempting. Recorded now as watch-list: the current projections
(`habit.rs` streak walk) only need the scalar, so this is S2-minor
and **no refactor is proposed until a consumer asks**; if F-1 lands,
the event log plus a typed pause event family becomes the natural
home.

**Blast radius**: none today. Issue records the watch.

---

## 2. Journal-inverse sweep (round law 2b)

| Command class | Journal verdict | Class verdict |
|---|---|---|
| `LogHabit` | invertible since R-7 (`journal.rs:136–141`) | fine until F-1 makes it exact |
| `AdjustHabitLog` absent window | refused, D24 (`journal.rs:131`, `undo.rs:421`) | **S3 → F-3**, dissolves in F-1 |
| `UpdateHabit` conversion seam | refused, D25 (`undo.rs:442`) | **S3 → F-3**, dissolves in F-1 |
| `StartTracking`/`StopTracking`, close-by-adjust | refused, `TrackingSurface` (ADR 0017, D23, `journal.rs:124–130`) | **S3 → F-4** (per-endpoint stamps make reopen expressible) |
| `DeleteHabit`/`DeleteLabel`/`DeleteTimeEntry` | `PreImages` restore since #901 | exact; anti-example |
| Reflection record/delete | refused, `ReflectionSurface` (`journal.rs:143–160`) — tombstone-vs-absence lossiness | documented accepted divergence (DR-7 D-J7): the family's own edit/delete verbs are the recovery surface; not scheduled |
| Reminder surface | refused (#354), ephemera | policy, not granularity |
| Board ceremonies | refused (#357) | policy, not granularity |

## 3. Wire-section sweep (round law 2c)

| Section | Fold | Verdict |
|---|---|---|
| `tasks` app fields | per-key `(clock, by)` duel, lane-gated | right granularity (field-level); anti-example |
| `reminder-rules` | keyed LWW by rule uuid | entity rows; fine |
| `dismissals` | additive set-union, first-write-wins | the row *is* the fact; fine |
| `time-entries` | whole-row LWW | **F-4** |
| `habits` | keyed LWW by habit uuid | entity rows; fine |
| `habit-log` | row-LWW over aggregated counts | **F-1/F-2/F-3** |
| `break-rules` | keyed LWW | fine |
| `habit-time-entries` | row LWW | inherits F-4's shape |
| none for habit day-log | device-local twin, no lane | **F-1** (#1340) |

## 4. State-engine mirror sweep (round law 2e)

No finding. `EngineCore` advances memory only by interpreting the
persisted batch (`crates/taskboard-state/src/core.rs:5–9`, the
single-advance rule); the memory-only extras (`sync_counters`,
`settings`, `retired_deck`) are declared never-persisted and
re-hydrate or expire by construction (`core.rs:19–34`). Memory ≡ disk
holds; there is no second source of truth between the `ArcSwap`
state and the repository.

## 5. Executive table

| # | Finding | Sev | Mechanism anchor | Refactor | Radius | Issue |
|---|---|---|---|---|---|---|
| F-1 | habit window-count rows + day twin | S1+S2+S3+S4 | habit.rs:634/740/839; command.rs:3226; merge.rs:1521 | per-completion event log (#1519) | largest | #1519 (existing, p1) + #1520 residue |
| F-2 | skip mark rides the aggregate row clock | S1 | command.rs:3374/3406; merge.rs:1537 | dissolves in F-1 | rides F-1 | #1520 |
| F-3 | absent-window inverse refused (D24/D25) | S3 | journal.rs:131; undo.rs:421/442 | dissolves in F-1 | journal/undo | #1520 |
| F-4 | TimeEntry whole-row LWW over span endpoints | S1+S3 | tracking.rs:38; merge.rs:1182; journal.rs:124 | per-endpoint stamps | moderate | #1521 |
| F-5 | reflection window frozen at mint | S2 | reflection.rs:301; reflection_stats.rs:15 | store instant, derive window | small (device-local today) | #1522 |
| F-6 | no focus-mode marker on time entries | S2 | tracking_stats.rs:28 | additive enum field | additive | #1523 |
| F-7 | pause as scalar deadline, no window attribution | S2-minor (watch) | command.rs:3437 | defer until consumer exists | none today | #1524 |

**Ranking by severity × blast radius**: F-1 ≫ F-4 > F-5 > F-2/F-3
(ride F-1) > F-6/F-7 (watch).

**Design rounds recommended for scheduling**:
1. **Habit-log event refactor** (F-1, absorbing F-2/F-3) — p1, #1519
   already open; the §F-1 brief is the round's input. UX journey plan
   required (ux_planning law) — quota/streak/heatmap semantics change.
2. **Time-entry per-endpoint stamps** (F-4) — schedule after (or
   alongside) the F-1 wire bump; reopen-undo becomes expressible and
   the TrackingSurface fence shrinks.
3. **Reflection instant row** (F-5) — cheap now, do before any
   reflection roaming work; no design round needed beyond a small
   plan (device-local, additive).

## 6. Anti-examples (do not "fix")

- **Task entity per-field clocks** (`entities.rs` `Clocks`) — the
  field-granularity precedent F-4 proposes to reuse.
- **Reminder deliveries/dismissals** — set-union over per-fact rows;
  the row *is* the fact.
- **`sync.pending_ops`** — deliberately derived at load, never stored
  (`persistence.rs:432–435`): the inverse pattern (store below
  granularity, derive up) done right.
- **Engine memory state** — single-advance from the persisted batch
  (§4).
- **Roaming settings rows** — one row per stable name, LWW by name;
  the name *is* the fact identity.

## 7. Unverified hunches

- The files-sync substrate's section-document tree layout
  (`taskboard-sync-files`) may aggregate per-section flushes above
  per-row granularity (a whole-section rewrite per cycle) — a
  *transport* granularity question, not a store-shape one; not
  audited here because no consumer-facing loss was demonstrable from
  the domain layer alone. Candidate for a dedicated transport audit.

## 8. Process notes

All findings carry recorded reality at `origin/main` (`9fc0d5ee`).
Out-of-scope findings filed as `backlog:` issues (#1520–#1524, one
per S1/S2 finding; F-1's residue consolidated into #1520 with #1519
as the p1 parent). No code was changed; this report lands via the
dispatcher's docs PR.
