# The Habit Event Consumers Chunk (chunk 5, #1519) — Plan

- **Date**: 2026-11-28
- **Kind**: stage-1 chunk plan — consumers re-keyed onto the event
  derivations, the legacy mirror deleted, the bookkeeping comments.
  No code in this commit.
- **Branch / worktree**: `feat/ui-habit-event-consumers` at
  `/home/viktoria/src/taskboard-WT-feat-ui-habit-event-consumers`.
  **Stacked base** — created on `origin/feat/sync-habit-events-wire`
  (`6bda37c2`, chunk 4's pushed tip, PR #1569) because #1569's merge
  waits on the operator. All `file:line` anchors below are **this
  worktree's** lines (verified 2026-11-28 at `6bda37c2`), not the
  design doc's `9fc0d5ee` anchors.
- **Origins (normative)**: design
  `.artifacts/plans/2026-11-27-habit-event-log-design.md` §5 (deletions),
  §7 rows 1–2, 5, 19–21, §8 (issue dispositions); UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md` (W-2, W-8,
  US-1/S3, US-3/S1, US-4 presentation halves); dispatch plan
  `.artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md` §1/§6;
  chunk-2 plan `.artifacts/plans/2026-11-27-habit-event-commands-plan.md`
  §1 D-C (the mirror) and D-U (the legacy undo legs); issues #1542,
  #1550, #1549, #1555 (the operator's conditions, restated in §6).
- **Status**: PLAN (stage 1) — recorded decisions only; three items the
  operator may overrule sit in §8 Rulings requested with defaults the
  prompts adopt.

---

## 0. The one-sentence shape

Every reader of `HabitState.log` / `day_log` re-keys onto
`derive_window_counts` / `derive_skip_posture` / `derive_day_counts`
(the chunk-1 derivations, `crates/taskboard-domain/src/habit_event.rs`);
then the mirror — the writers, the persistence actions, the storage
arms, the maps themselves — is deleted in an order where every commit
compiles; `#1542` closes with grep evidence; the UX journey plan's
remaining MISSING rows flip FOUND.

## 1. Recorded decisions

### D-1 — the re-key contract: projections take derived views, not row maps

Every window-grain projection changes its signature from
`log: &BTreeMap<HabitLogKey, HabitLogEntry>` to two derived views:

```rust
counts: &BTreeMap<WindowStart, u32>,       // derive_window_counts
posture: &BTreeMap<WindowStart, bool>,     // derive_skip_posture
```

Day-grain projections (`habit_history_day_cells`) take
`&BTreeMap<NaiveDate, u32>` (`derive_day_counts`). The walk bodies are
**unchanged logic over derived inputs** (design §2.2: absent key = 0 /
unskipped is exactly today's `log.get().map_or(0, |e| e.count)` /
`entry.skip` read shape — the chunk-1 derivation docs state this
deliberately). Call sites derive once per pass:

- **domain** (`crates/taskboard-domain/src/habit.rs`): `log_window`
  (`:979`), `streak` (`:1068`) and its helpers (`:1082`, `:1089`,
  `:1096`), `habit_streak_unioned` (`:1343`), `statistics_horizon`
  (`:1485`), `adherence` (`:1541`, the `:1558` entry read),
  `best_streak` (`:1602`, the `:1615` entry read),
  `per_period_totals` (`:1650`). Internal call sites re-keyed in the
  same commit: `command.rs` (the LogHabit arm's `log_window` call and
  every mirror-refresh input), `undo.rs` (the legacy legs — D-3),
  `reminders.rs:1454` (the tracker-nudge quota gate reads the derived
  count of the current canonical window).
- **UI presentation** (`crates/taskboard-ui-slint/src/presentation.rs`):
  the Today row feed (`:8200–8260`: count `:8209`, repair target
  `:8211`/`:8225`, streak `:8222`, skip `:8247`), `habit_repair_target`
  (`:8291`), `habit_statistics` (`:8631`), `habit_history_month`
  (`:8658`), `habit_history_day_cells` (`:8796`). The adapter seams
  re-key with them: `adapter/seams/habit.rs:30–41`,
  `adapter/seams/recur.rs:35–39`, `adapter/sync.rs:948–955`,
  `adapter/seams/board.rs:530–541`, `adapter/effects.rs:860–872`,
  and the wiring `models.rs:7536–7576` (the feed derives once per
  `push_habit_history` pass and hands the derived map to the three
  presentation fns).
- **app CLI** (`crates/taskboard-app/src/commands/habits.rs`): the skip
  receipt's streak read (`:332`), the log receipt's row read-back
  (`:603–616` — see D-4), the adjust receipt's row read-back
  (`:650–656`), the delete receipt's row count (`:823–831` — see D-5),
  the CLI habit-row feed (`:995–1012`), history
  (`:1339–1357` incl. `window_state_view`'s entry read `:1346`) and
  stats (`:1392–1394`). The UI stays a dumb client: it derives via the
  **domain** derivation functions and renders the result — no habit
  arithmetic moves into the UI crate (AGENTS §1/§2).

A tiny domain helper keeps call sites one-line (placed in
`habit_event.rs`, the derivations' home):

```rust
/// The derived (counts, posture) pair for one habit — the two views
/// every window-grain projection consumes.
pub fn derived_window_views<'a>(habit: &Habit,
    events: impl Iterator<Item = &'a HabitEvent>, day: DayCalendar)
    -> (BTreeMap<WindowStart, u32>, BTreeMap<WindowStart, bool>);
```

(Day-grain callers call `derive_day_counts` directly — one call, no
helper ceremony.)

### D-2 — the heatmap contract (W-2, US-4/S1, `us4_s1_day_cells_derive_from_events`)

`habit_history_day_cells(habit, &state.habits.day_log, month)` becomes
`habit_history_day_cells(habit, &derive_day_counts(habit, events, day), month)`.
Behavior deltas, both intended and pinned by test:

- **US-3 presentation half (#1341)**: undoing a Week/Month log removes
  the day cell it derived — the legacy twin could leave a phantom cell
  (the twin was only written on Day-kind and adjust paths); the derived
  map cannot disagree with the events. Presentation test
  `us3_s1_week_undo_day_cell_gone`-shaped (name carries the token per
  the journey plan; the engine half `us3_ac1` already exists at
  `undo/tests.rs:1209`) asserts the cell set re-derives empty after the
  tombstone.
- **US-4 AC-2 render half (migration attribution)**: a synthesized
  (Ruling-A1) event stamps at its row clock, so its completions render
  on that day — the test seeds two legacy-window-shaped events with the
  chunk-3 deterministic ids (`legacy_habit_event_id`) and asserts the
  grid attributes them at the stored instant (`us4_s1` token).
- **Clock discipline** (`docs/testing_strategy.org` §6.1–6.2): the
  presentation fns take `now`/`calendar` as today; the committed
  artifacts that embed clock-derived text (the golden PNGs,
  `golden_visuals.rs`) already render through the frozen mock clock
  (`UiSettings` render clock, `golden_visuals.rs:22`, `:125–130`) —
  that seam is preserved verbatim; no golden regen may change the
  clock seam, only the fixture's data source.

### D-3 — the legacy undo legs survive, re-keyed onto derived state (the deletion-safety answer for pre-#1520 journal entries)

Pre-#1520 journal entries carry `PreImage::HabitLogRow` +
`EntityStamps::HabitLog` and undo through the three legacy legs in
`undo.rs` (`:2788` SkipHabit, `:2829` AdjustHabitLog, `:2914`
LoggedHabit). Their **inverses already work under the event grammar**
(the LoggedHabit inverse is `AdjustHabitLog { count: pre }` = the
reconcile diff; the Skip inverse is the mirror verb `UnskipHabit`) —
only their **guards** read `app.habits.log`, which dies. Decision: the
guards re-key onto the derived projections, at projection grain:

- LoggedHabit: `app.habits.log.get(&key).count == recorded_post.count`
  → `derived counts[key.window] == recorded_post.count`.
- SkipHabit: whole-row compare → derived posture compare:
  `posture[key.window] == recorded_post.skip`.
- AdjustHabitLog legacy leg: whole-row compare →
  `counts[key.window] == recorded_post.count`.

**Recorded honesty note**: the whole-row (clock-included) compare
downgrades to a count/posture compare for these pre-#1520 entries —
an interleaved write that restores the same count without moving any
surviving clock would once have refused and now succeeds. That is
strictly *more* permissive for legacy entries only; the event-grammar
legs (the snapshot compare, `:2843`, `:2936`) keep their exact guards.
Documented at the three sites; pinned by a test that undoes a fixture
entry with `PreImage::HabitLogRow` through the re-keyed guard
(`us3_legacy_row_preimage_still_undoes`, `undo/tests.rs`) — this is
the "state which, with a test" answer: **undo of pre-#1520 entries
works through the retained legacy inverse paths, guards re-keyed**;
nothing is refused that used to succeed.

### D-4 — CLI typed-shape deltas (journey plan §7: payloads change, verbs do not)

- **log receipt** (`habits.rs:603–616`): today it re-reads the stored
  mirror row to report `count`; the outcome already carries the derived
  post-count. The row read-back dies; the receipt reports the outcome's
  count directly (`count == reported` collapse — the two fields were
  row-vs-payload; under one source of truth they are the same fact).
  Typed-shape delta recorded; no text change.
- **adjust receipt** (`habits.rs:650–656`): same collapse — the row
  read-back dies, the receipt reports the outcome's count (the derived
  truth; the #1549 grammar limit's honest surface, see D-7).
- **delete receipt** (`habits.rs:823–831`): today `log_rows` counts the
  habit's mirror rows as a "data deleted" structural fact. Re-keyed to
  count the habit's live events (`state.habits.events` filtered by
  habit id). Field stays named `log_rows`? No — renamed `event_count`
  (the name `log_rows` names a deleted storage shape; a stale name is
  a lie). Typed-shape delta; the JSON output key changes
  (`log_rows` → `event_count`) — recorded in the commit body and the
  journey-plan amendment (D-8); no user-facing sentence changes.
- **history/stats/skip streak** (`:332`, `:1339–1357`, `:1392–1394`):
  derived reads; the `window_state_view` helper takes the derived
  pair. CLI *outcomes* are typed views — the no-text-assertions rule
  holds; tests assert structural facts (counts, enum variants).

### D-5 — #1550 (logging into a skipped window keeps the skip): wording only, no new affordance

The operator ruled the behavior (the skip is asserted intent; a log
does not withdraw it — chunk 2's deliberate delta). Chunk 5 owns the
UX bookkeeping:

- **No compound unskip+log affordance** (the issue's open question):
  R-1 (no new verb/affordance) governs; `UnskipHabit` is already the
  named recovery verb (journey plan R-5). Recorded default; listed in
  §8 Rulings requested in case the operator wants the compound.
- **Journey plan amendment** (see D-8): US-5/S1's System-response cell
  and US-1/S1's escape cell gain the sentence: "a window already
  skipped ignores new logs until unskipped (the skip is asserted
  intent — `UnskipHabit` is the recovery verb; #1550)."
- **Skip badge wording**: the Review skip badge keeps its "skipped"
  label — it is already truthful (the posture derives skipped). The
  journey plan records the wording decision so the walkthrough
  (stage 3) checks the badge does *not* claim or imply that a
  later log cleared it. No `.slint` text change.

### D-6 — #1555 (the `repair_undoable` bit): the mirror-row heuristic is deleted, the bit becomes trivially true

`presentation.rs:8237–8245`: `repair_undoable` keys on the legacy
mirror row's presence. Under the event grammar the invert condition is
"the journal entry is an `AdjustHabitLog` with a
`PreImage::HabitEventWindow` snapshot" — always true for every repair
the bit can be rendered for. Decision: `repair_undoable` becomes
`repair_target.is_some()` (derive the target from events via
`habit_repair_target`, drop the row lookup), the stale comments at
`:8129`, `:8236` and the `:8600`/`:8786` doc-comment blocks are
rewritten to the event grammar, and the pinning test at
`:9301–9324` is rewritten to pin the new semantics under a new name
(no `..._currently_pins_...` debt — the old test pinned the D24 fence,
a deleted behavior). `habit_repair_target` (`:8291`) re-keys onto
derived counts (a repair target = a past window with count < quota —
same arithmetic over the derived map).

### D-7 — #1549 (the over-quota audit limit): the user-facing statement

The operator ruled option 3: the limit is accepted. Chunk 5 owns the
statement. Where it lives:

- **CLI**: the `habits adjust` subcommand's long-help line gains: "an
  audited count above the quota is capped by the attribution rule when
  the predecessor window is unmet — the receipt reports the events
  actually minted" (help text is the CLI's user-facing statement
  surface; no runtime error text, no text assertion — tests stay
  typed on the receipt's `created`/`tombstoned` vectors).
- **Journey plan amendment** (D-8): US-2/S1's System-response cell
  gains the cap sentence.
- The receipt itself (chunk 2's `created`/`tombstoned`) is already the
  typed truth; nothing else changes.

### D-8 — journey plan amendments (the plan file is edited in this chunk)

`.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md`:

1. US-5/S1 + US-1/S1: the #1550 sentence (D-5).
2. US-2/S1: the #1549 cap sentence (D-7).
3. **W-5's Test cell re-pointed**:
   `crates/taskboard-app/src/commands/habits.rs::us2_s1_adjust_reconciles_by_tombstones`
   → `crates/taskboard-domain/src/command.rs::us2_s1_adjust_reconciles_by_tombstones`.
   Reason (recorded): the reconcile test lives in the domain engine
   (chunk 2 landed it there); duplicating it in the CLI crate would
   violate "one behavior, one test, lowest layer" (AGENTS §5) — the
   CLI layer tests routing only, which the new `us1_s2` CLI test
   covers. The gate currently reads US-2/S1 MISSING *because* the
   nomination names the wrong evidence file; this amendment fixes the
   nomination, not a test (the alias mechanism is for renamed tests,
   not relocated ones — a direct path fix is the honest spelling).
4. W-3's Test cell names the new CLI test
   `crates/taskboard-app/src/commands/habits.rs::us1_s2_cli_log_derives_count`
   (already nominated; this chunk lands the test).
5. W-2's Test cell (`us4_s1_day_cells_derive_from_events`) is already
   correct; this chunk lands the test in `presentation.rs`.

### D-9 — Ruling F applied: the sqlite write/read arms GO in this chunk; the remap's reads STAY

`crates/taskboard-storage-sqlite/src/repo.rs`:

- **Deleted** (the mirror's storage):
  - the load-seeding of legacy rows into the in-memory state
    (`:2105–2117` the `habit_log_rows` seeding loop, `:2149` the
    `habit_day_log_rows` seeding loop) — `HabitState` has no fields to
    seed;
  - the partition collection arms
    (`:2632` `habit_log_rows.push`, `:2635` `habit_day_log_rows.push`
    and the `RecordHabitLog`/`RecordHabitDayLog` match arms that feed
    them);
  - the flush write arms (`:3205–3243` the `habit_log` INSERT with the
    #1095 guard, `:3245–3259` the `habit_day_log` INSERT) and the
    vectors themselves (`:2566`, `:2567`).
- **Kept verbatim** (Ruling F + the remap's survival):
  - the load SELECTs `:1697–1706` (`habit_log`) and `:1708–1715`
    (`habit_day_log`) — **the migration 0032 remap
    (`remap_habit_events_if_needed`, `:1192`, called `:2084`) reads
    exactly these vectors; deleting the reads would strand a
    pre-0032 database's habit history**;
  - the marker guard and the whole remap;
  - the tables themselves (inert schema, forever — ADR 0005).
- **Fresh open of a pre-0032 database after the arms are gone**: load
  runs → the remap sees a legacy-populated `habit_log`/`habit_day_log`
  and an empty `habit_event` → synthesizes the Ruling-A1 events with
  the deterministic v5 ids → writes the marker → the state seeds from
  `habit_event` only (`:2231–2241`). No data path reads the legacy
  rows for state; the tables keep them as inert history. A database
  already remapped (marker present) skips the remap and ignores the
  legacy tables entirely. Pinned by the existing
  `habit_event_migration_tests` module (`:4030+`) — one new test
  asserts the remap still fires on a legacy-populated fixture *after*
  the seeding arms are gone (the module already covers the remap; the
  new test guards the ordering).

### D-10 — Ruling D1 reconciled: the read-side shim STAYS; only the write-side builder is deleted

The dispatch plan's chunk-5 slice names `fold_habit_log` /
`habit_log_for` / `lane_gc_old_habit_log` / "the legacy shim" as
deleted here. Chunk 4 landed the shim as
`fold_legacy_habit_log` (`merge.rs:1600`, production callers
`pipeline.rs:1029`, `:2022`) + `union_habit_log` (`appdata.rs:1989`,
called `:1801`) + `lane_gc_old_habit_log` (`appdata.rs:2994`, called
by `flush.rs:510`) + the `HabitLogRecord` codec (`appdata.rs:1236`) +
the `habit_log` section field (`appdata.rs:116`).

**Decision: keep all of the above; delete only `habit_log_for`
(`appdata.rs:2956`) — the write-side builder, whose last production
caller chunk 4 already removed.** Reasoning (recorded against Ruling
D1): D1's convergence guarantee is "an old peer's legacy rows fold
into a new peer's events, then age out under the ≤ 180 d wire GC".
Deleting the read-side shim *now* breaks exactly that: a legacy
section relayed by a not-yet-upgraded peer would file under
`unknown` and never reach the event log — the old peer's habit history
would silently stop propagating for up to 180 days. The shim is tiny
(one fold fn, one union fn, one GC lane, one codec enum), it is fully
read-side (nothing in this chunk's deletion surface writes legacy
rows after D-9/C6), and the age-out is its own clock. Full removal is
**explicitly out of scope** — filed as backlog issue **#1571** with the gate "one release
cycle after the fleet upgrade, verified by a zero-legacy-sections
grep over real profiles"; the dispatch plan's §7 note
("legacy-shim removal is chunk 5's cut-over with the retention window
as the safety margin") is satisfied by recording the deviation here
(origin sections' deviations need a recorded reason — this is it).
`habit_log_for` and its appdata tests (`:6810`, `:6848` region) are
deleted with it; `flush.rs:29` drops the import; the `flush.rs:441`
and `board_cycle.rs:1675` comments ("the field itself survives
until…") are updated to point at the backlog issue.

### D-11 — the mirror dies with its keystone (commit-order contract)

`legacy_mirror_equals_event_derivations` (`command.rs`, one ref) is
**removed in the same commit that deletes the mirror writers** (C6
below) — not before. While the mirror exists it must stay pinned
(readers re-key onto derivations in C1–C4, but the mirror keeps being
written until C6, and the keystone is what makes that window
provably harmless); removing the test earlier would leave an
unpinned dual-write window, removing it later is impossible (the
test's subject — `HabitState.log` — is deleted in C7). #1542's
condition "the keystone proptest removed together with the mirror"
is satisfied exactly.

### D-12 — `HabitLogKey` / `HabitLogEntry` survive as decode-only retired shapes; the day types die completely

`PreImage::HabitLogRow { key: HabitLogKey, before: Option<HabitLogEntry> }`
(`journal.rs:195`), `JournalEntity::HabitLog(HabitLogKey)`
(`journal.rs:286`), `EntityStamps::HabitLog(HabitLogEntry)`
(`journal.rs:319`) are **persisted shapes** — pre-#1520 journal
entries decode through them forever (the undo legs D-3 keep serving
them). So the two types cannot be deleted outright (the design §5
"deleted" is amended here, recorded reason: persisted-shape
decode-stability outranks symbol hygiene). Their fate:

- types **kept**, doc-comments rewritten to "**Retired decode-only
  shape** (the #1519 event grammar; #1542's mirror deleted — no
  production writer or state reader remains; serves pre-#1520
  journal entries' decode and their legacy undo legs)"; moved to the
  bottom of the type section with a section comment so no new reader
  anchors to them by accident;
- `HabitState.log` / `HabitState.day_log` fields (`habit.rs:831`,
  `:839`) **deleted**, with the D-H12 documented-leniency block
  (`:820–825`) — no successor shape exists (design §2.1);
- `HabitDayKey` / `HabitDayLogEntry` (`habit.rs:680–734`) **deleted
  completely** — nothing persisted references them (verified: the
  only non-test refs are the state field, the persistence action, the
  mirror writers, the sqlite arms and `habit_history_day_cells`'s
  parameter — all deleted in this chunk);
- `mint_log_clock` (`habit.rs:802`) **deleted** — its last callers are
  the mirror writers (C6); the legacy undo legs (D-3) no longer
  re-stamp rows (their inverses are event-grammar commands), so the
  #986 bump has no remaining role; `journal.rs:145`'s doc cross-ref
  updated.

## 2. TDD-first commit sequence (every commit compiles and stands alone)

Tests land **in the same commit as the behavior they pin** (the
house TDD posture: red-green per commit is not visible in history;
each commit is a complete verified unit — rebase-merge hygiene).

| # | Commit | Contents (tests named) |
|---|---|---|
| C1 | `feat(domain): habit projections re-key onto the derived window views` | D-1's domain half: the `habit.rs` projection signatures (`log_window`, `streak`, `habit_streak_unioned`, `statistics_horizon`, `adherence`, `best_streak`, `per_period_totals`), `derived_window_views` helper, `command.rs`/`undo.rs`/`reminders.rs` internal call sites. The habit.rs test modules and proptest fixtures re-key onto event fixtures (the mirror is still alive and still keystone-pinned — the re-keyed projections must be derivation-exact, and the keystone holds). `habit_time.rs`'s D-2 proptest fixtures re-key (test-only). No behavior change intended — pinned by the untouched streak/proptest suite passing green. |
| C2 | `feat(ui): the heatmap derives from events` | `habit_history_day_cells` + `models.rs:7576` wiring; tests `us4_s1_day_cells_derive_from_events` (D-2's attribution leg) and the Week-undo phantom-cell presentation leg of US-3; `habit_surface_journeys.rs:1156`, `habit_family_journeys.rs` and `golden_visuals.rs:2823` fixtures re-keyed onto events (day-log inserts die). Golden PNG regen per §4. |
| C3 | `feat(ui): Today badge, skip, streak and repair derive from events (#1555)` | presentation `:8200–8260` + `habit_repair_target` `:8291` + `habit_statistics` `:8631` + `habit_history_month` `:8658`; `repair_undoable` grammar rewrite (D-6) with the rewritten pinning test (`:9301–9324`); stale comments `:8129` `:8236` `:8600` `:8786`; adapter seams (habit/recur/sync/board/effects). |
| C4 | `feat(app): CLI habit surfaces derive from events` | D-4's deltas; new `us1_s2_cli_log_derives_count` (routing-level: the CLI surfaces the outcome's derived count — structural assertions only); history/stats/delete-receipt tests updated to derived fixtures; journey-plan amendment D-8 (items 3–4) rides this commit (it is the commit that makes the nomination true). |
| C5 | `feat(storage): the legacy habit-log sqlite arms retire — the remap's reads stay` | D-9's deletions; `connect.rs:400–415` upgrade fixture re-keyed to events; `roundtrip.rs` / `corrupt_row_matrix.rs` mirror-row cases deleted or re-keyed; **new** post-C5 remap-still-fires test (D-9's last bullet). |
| C6 | `feat(domain): the derived legacy mirror deleted (#1542)` | `habit_mirror_refresh` (`command.rs:866`) and every `RecordHabitLog`/`RecordHabitDayLog` emission (the LogHabit arm `:3447–3500`, the Adjust arm `:3635–3674`, `plan_mark_event` `:832–848`, the Skip/Unskip refreshes `:3781–3813`); `PersistenceAction::RecordHabitLog`/`RecordHabitDayLog` variants + apply arms (`persistence.rs:531`, `:542`, `:710–740`); the keystone proptest removed (D-11); `undo.rs` watermark capture arms (`:1042–1051`, `:1103`) and the `habit_view` mirror legs (`:1285–1302`) deleted; the legacy-leg guards re-keyed (D-3) with `us3_legacy_row_preimage_still_undoes`; `messages.rs:611` doc comment rewritten. **Commit body carries #1542's grep evidence list** (§5). |
| C7 | `refactor(domain): HabitState.log / day_log, the day types and mint_log_clock deleted` | D-12's deletions (`habit.rs:680–734`, `:802–814`, `:820–839`, `lib.rs` re-exports, `test_support/habit_strategies.rs` fixture re-key, journal doc cross-refs). The state insta snapshots lose the `log`/`day_log` keys (§4 churn). Grep §5 must be clean from here. |
| C8 | `feat(sync): the legacy habit-log write builder deleted; the read shim retained (Ruling D1)` | `habit_log_for` (`appdata.rs:2956`) + its unit tests deleted; `flush.rs:29` import dropped; `flush.rs:441` / `board_cycle.rs:1675` comments re-pointed at the backlog issue; the read-shim tests stay (they are the shim's spec). |
| C9 | `docs(plan): journey-plan amendments and the consumers chunk bookkeeping` | D-8 items 1–2 + any remaining wording; the ux-coverage gate output attached to the stage-2 report (not committed); issue comments posted (§6). |

Stage 2 may merge/split commits where the boundaries resist (e.g. C1
is large) — **the order is binding** (readers before writers before
types), every commit compiles and passes the fast gate.

## 3. Snapshot / corpus churn (per-artifact justification)

- **taskboard-state insta snapshots**: unchanged until C6; C6 makes
  the mirror writes stop (snapshot deltas where a command previously
  recorded `RecordHabitLog`/`RecordHabitDayLog` actions); C7 removes
  the `log`/`day_log` keys from every `AppState` snapshot. Justify
  each changed snapshot in the commit body as "mirror retired / maps
  deleted (#1519 chunk 5)" — no other snapshot delta is acceptable.
- **golden PNGs** (`golden_visuals.rs`): regen **only** for goldens
  whose fixture carries habit history whose day cells change source
  (C2). The expectation: pixel-identical for Day-kind fixtures
  (derived day counts ≡ the twin under the keystone law) — if a Day
  golden changes, that is a **finding** (a derivation bug), not
  churn. Week/Month fixtures may legitimately change **only** where
  the old fixture wrote a phantom day cell the derived map correctly
  drops (#1341's fix — cite the exact PNG and the phantom cell it
  loses). The render-clock freeze seam is untouched (D-2).
- **sqlite fixture corpora** (`roundtrip.rs`,
  `corrupt_row_matrix.rs`, `connect.rs` upgrade fixtures): legacy
  mirror-row cases are deleted with their arms (C5) or re-keyed to
  event fixtures; the remap fixtures keep their legacy-table rows
  (they are the remap's input, Ruling F).

## 4. The deletion checklist (each entry: this worktree's file:line)

Order = commit order; every entry compiles when its commit lands.

**C1 (re-key readers — nothing deleted yet)**
- [ ] `habit.rs:979` `log_window` — signature + `:989` key read
- [ ] `habit.rs:1068–1100` `streak` + helpers (`:1082`, `:1089`, `:1096`)
- [ ] `habit.rs:1343` `habit_streak_unioned`; `:1485` `statistics_horizon`;
      `:1541`/`:1558` `adherence`; `:1602`/`:1615` `best_streak`;
      `:1650` `per_period_totals`
- [ ] `command.rs` — the LogHabit arm's `log_window` call and every
      mirror-refresh input (`:835–845`, `:3447–3500`, `:3635–3674`,
      `:3700–3813`) fed from derivations (writers still write the mirror)
- [ ] `undo.rs` — the event-grammar legs' reads already derive; the
      legacy legs re-key in C6 (their guards are the only map reads)
- [ ] `reminders.rs:1454` tracker-nudge quota gate
- [ ] `habit_event.rs:870` region — `us1_s3` test stays (already derived)

**C2 (heatmap)**
- [ ] `presentation.rs:8796` `habit_history_day_cells` — parameter
      `&BTreeMap<HabitDayKey, HabitDayLogEntry>` → `&BTreeMap<NaiveDate, u32>`
- [ ] `models.rs:7567–7576` — `push_habit_history` derives
      (`derive_day_counts`) once; `:7567`/`:7568` still take the log map
      until C3 (they compile — C2 touches only the day-cells line and
      its `day_log` read; the map still exists)
- [ ] test fixtures: `habit_surface_journeys.rs:1156`,
      `habit_family_journeys.rs:168/734/924`,
      `golden_visuals.rs:2823`, `small_gated_journeys.rs:734`,
      `undo_family_restore_clear_surfaces_journeys.rs:470`

**C3 (Today row + repair + adapters)**
- [ ] `presentation.rs:8200–8260` (count `:8209`, repair `:8211`/`:8225`,
      streak `:8222`, skip `:8247`, `repair_undoable` `:8237–8245` per D-6)
- [ ] `presentation.rs:8291` `habit_repair_target`; `:8631`
      `habit_statistics`; `:8658` `habit_history_month`
- [ ] stale comments `:8129`, `:8236`, `:8600`, `:8786`; pinning test
      `:9301–9324` rewritten (D-6); test fixtures `:9328`, `:9473`
- [ ] `adapter/seams/habit.rs:30–41`; `adapter/seams/recur.rs:35–39`;
      `adapter/sync.rs:948–955`; `adapter/seams/board.rs:530–541`;
      `adapter/effects.rs:860–872`

**C4 (CLI)**
- [ ] `commands/habits.rs:332` (skip streak), `:603–616` (log receipt,
      D-4), `:650–656` (adjust receipt), `:823–831` (delete receipt →
      `event_count`, D-4), `:995–1012` (row feed), `:1339–1357`
      (history + `window_state_view`), `:1392–1394` (stats)
- [ ] new `us1_s2_cli_log_derives_count`; `habits adjust` help line
      (D-7) with no text assertion on it

**C5 (storage)**
- [ ] `repo.rs:2105–2117` + `:2149` load-seeding loops
- [ ] `repo.rs:2566–2567` vectors; `:2632`, `:2635` partition pushes +
      their match arms
- [ ] `repo.rs:3205–3243` `habit_log` INSERT; `:3245–3259`
      `habit_day_log` INSERT
- [ ] **kept**: `repo.rs:1697–1715` load SELECTs; `:1192–1290` remap +
      marker; `:2084` remap call
- [ ] `connect.rs:343–415` upgrade fixture re-keyed;
      `roundtrip.rs` / `corrupt_row_matrix.rs` mirror cases
- [ ] new remap-still-fires test (D-9)

**C6 (the mirror + keystone + persistence actions)**
- [ ] `command.rs:850–940` `habit_mirror_refresh`; emissions at
      `:832–848` (`plan_mark_event`), `:3431–3500` (LogHabit),
      `:3550–3674` (Adjust), `:3700–3813` (Skip/Unskip)
- [ ] `command.rs` keystone proptest `legacy_mirror_equals_event_derivations`
- [ ] `persistence.rs:531`, `:542` (variants); `:710–740` (apply arms)
- [ ] `undo.rs:1042–1051`, `:1103` (watermark capture);
      `:1285–1302` (`habit_view` legs); legacy-leg guards re-keyed
      (`:2820–2826`, `:2866–2870`, `:2960–2965` region — the
      `app.habits.log.get(&key)` compares) per D-3
- [ ] `messages.rs:611` doc comment; `pipeline.rs:3641–3770` test
      fixtures + `:3674` action filter arm

**C7 (types + state fields)**
- [ ] `habit.rs:634–672` `HabitLogKey` → retired decode-only block (D-12)
- [ ] `habit.rs:740–772` `HabitLogEntry` → retired decode-only block
- [ ] `habit.rs:680–734` `HabitDayKey`/`HabitDayLogEntry` **deleted**
- [ ] `habit.rs:802–814` `mint_log_clock` + `one_second` (if now unused)
- [ ] `habit.rs:820–839` D-H12 leniency block + `log`/`day_log` fields
- [ ] `habit.rs:848–858` `events` field doc updated (drop the
      coexistence sentence); `:850–851` comment rewritten
- [ ] `lib.rs` re-exports pruned; `habit_event.rs:372–386` doc
      cross-refs to `HabitState.log` reworded to "the deleted mirror"
- [ ] `test_support/habit_strategies.rs:235–310` fixture re-key;
      habit.rs test modules (`:1842–2662` region) re-keyed onto
      event fixtures

**C8 (write-side builder)**
- [ ] `appdata.rs:2956–2992` `habit_log_for` deleted (+ tests
      `:6810` region); `lane_gc_old_habit_log` **kept** (D-10)
- [ ] `flush.rs:29` import; `flush.rs:441`, `board_cycle.rs:1675`
      comments re-pointed

## 5. Grep-verification list (stage 2 end; stage 3 re-runs)

From the worktree root — all must return **zero lines** (test files
included except where noted):

```sh
git grep -n "mint_log_clock"                      -- crates
git grep -n "RecordHabitLog\|RecordHabitDayLog"   -- crates
git grep -n "habit_mirror_refresh"                -- crates
git grep -n "legacy_mirror_equals_event_derivations" -- crates
git grep -n "HabitDayKey\|HabitDayLogEntry"       -- crates
git grep -n "day_log"                             -- crates
git grep -n "habits\.log\b\|habits\.log\."        -- crates   # the state field
git grep -n "habit_log_for"                       -- crates
git grep -n "\.log\.get(&HabitLogKey"             -- crates   # no state-map read survives
git grep -n "HabitState\.log\|state\.habits\.log" -- crates
```

**Documented survivors** (a hit on these symbols is a finding only if
it is NOT one of these):

- `HabitLogKey` / `HabitLogEntry`: `journal.rs:195/:286/:319` (the
  persisted decode-only shapes), `journal/tests.rs`,
  `undo.rs` (the legacy legs + `:1809` key extractor),
  `undo/tests.rs`, `codec.rs:1159–1195` (the journal-payload
  round-trip test), `messages.rs:611` (rewritten in C6 — must be
  clean), `appdata.rs:3005` (`lane_gc_old_habit_log`'s key parse —
  the retained shim), `pipeline.rs` (test fixtures re-keyed in C6 —
  must be clean).
- `fold_legacy_habit_log`, `union_habit_log`, `lane_gc_old_habit_log`,
  `HabitLogRecord`, the `habit_log` **section field**: the retained
  read-side shim (D-10) — `merge.rs:1600`, `pipeline.rs:1029/:2022`,
  `appdata.rs:116/:1236/:1801/:1989/:2994`, `flush.rs:510`, plus
  their tests.

## 6. Bookkeeping (issue comments; closures are dispatcher-owned at merge)

Draft comment texts (stage 2 posts them after C7/C8, citing the PR;
**never closes an issue** — dispatch plan §7):

- **#1542** (closes at merge): "Removed in <PR>. Grep evidence (zero
  hits, `git grep` at <sha>): `mint_log_clock`, `RecordHabitLog`,
  `RecordHabitDayLog`, `habit_mirror_refresh`,
  `legacy_mirror_equals_event_derivations`, `HabitDayKey`,
  `HabitDayLogEntry`, `day_log`, `state.habits.log`,
  `habit_log_for`. Survivors, by design: `HabitLogKey`/`HabitLogEntry`
  (decode-only retired persisted shapes — pre-#1520 journal entries),
  the wire read-side shim (`fold_legacy_habit_log`/`union_habit_log`/
  `lane_gc_old_habit_log`/`HabitLogRecord` — Ruling D1's ≤ 180 d
  age-out, removal tracked in <new-backlog-issue>). The keystone
  proptest was removed with its subject (the mirror)."
- **#1340** (dissolves): "The heatmap now derives per-day counts from
  the synced event log (`derive_day_counts`) — the device-local
  day-overlay twin is deleted with the mirror (#1542); cross-device
  day parity falls out of event sync. Design §8 row 2:
  <permalink to this plan and the design doc>."
- **#1341** (dissolves): "Undo tombstones exactly the minted event id;
  the heatmap re-derives, so the Week/Month phantom day cell is gone
  by construction (presentation leg pinned in chunk 5, engine leg
  `us3_ac1` in chunk 2). Design §3.4."
- **#1489** (dissolves, caveat recorded): "`day_log` is deleted
  (unbounded growth subject gone). Local events, like local rows
  today, are never GC'd — if local event growth ever shows, a new
  backlog item owns a local prune (deliberately out of #1519's
  scope; the wire bound is Ruling E1's 180 d)."
- **#1094** (dissolves): "Union-by-uuid keeps concurrent increments —
  two devices logging in the same second both count (chunk 4's fold
  tests `us5_ac2`/`us5_s3`); no per-author counter needed."
- **#1520** (closes at merge per its own re-triage note): "F-2: skip
  is a typed mark event, posture derives (Ruling B) — the row-LWW
  drop class is gone (chunk 4 fold tests). F-3: the `HabitSurface`
  fence is deleted (chunk 2); chunk 5 deleted the last mirror reads
  that kept the row grammar alive."
- **#1550** (closes at merge): "Behavior ruled and kept: a log into a
  skipped window leaves the posture skipped. Chunk 5 recorded the UX
  bookkeeping in the journey plan (US-5/S1 + US-1/S1 note: unskip is
  the recovery verb); the Review skip badge keeps its 'skipped' label
  (already truthful); no compound unskip+log affordance (R-1) —
  recorded as the default in the chunk-5 plan §8."
- **#1549** (closes at merge): "Option 3 recorded: the limit is
  accepted. The user-facing statement lives in `habits adjust`'s help
  text and the journey plan's US-2/S1 cell; the receipt's
  `created`/`tombstoned` vectors remain the typed truth."
- **#1555** (closes at merge): "The `repair_undoable` bit no longer
  keys on the mirror row — it derives from the event grammar (a
  repair is always invertible: every adjust journals
  `PreImage::HabitEventWindow`); the stale comments and the pinning
  test were rewritten (chunk 5, C3)."
- **#1488**: closed at chunk 1's merge (no chunk-5 action; verified
  still open-closure dispatch plan §7).
- **#1519**: closure note for the dispatcher: "chunk 5 = the last
  chunk; closes with the PR merge after the §7 completion sweep."

## 7. The MISSING rows this chunk must flip (honest pre-chunk steady state)

`scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --root <worktree>`
at `6bda37c2`: 45 checks, 35 FOUND, **10 MISSING**:

1. `US-1/S2 journey test` → the CLI test `us1_s2_cli_log_derives_count`
   (C4 lands it; W-3).
2. `US-2/S1 journey test` → nomination re-pointed to the existing
   domain test (D-8 item 3; C4).
3. `US-4/S1 journey test` → `us4_s1_day_cells_derive_from_events`
   (C2 lands it; W-2).
4.–9. the six matrix-story rows (`habit.log`×2, `habit.adjust`×2,
   `habit.heatmap`, `habit.streak/quota`) — these flip
   automatically once US-1/S2, US-2/S1 and US-4/S1 are FOUND (they
   are story-step-coverage checks over the same tokens).
10. `W-8 callback habits-undo-log-clicked (display only)` — the
    wiring demonstrably exists (`today-view.slint:538`,
    `app-window.slint:777`, `seam_inventory.rs:156`,
    `adapter/seams/habit.rs:108`), yet the gate reads MISSING.
    **Stage-2 instruction**: run the gate verbosely for this row and
    fix what the gate is actually missing (most likely the plan row's
    Commands cell `— (display only)` interacting with the callback
    check, or the Surface-file column pointing at `models.rs` while
    the gate scans `.slint` + adapter from the row). Fix the **plan
    row format** if that is the defect; fix nothing in production
    wiring (it is complete). If the gate genuinely cannot credit an
    existing fully-wired display-only callback, that is a gate bug:
    file it as a backlog issue, cite it in the stage report, and
    record the row as FOUND-by-manual-verification in the report —
    never paper over, never weaken the plan.

Stage 2 runs the gate with `--root` pointed at the worktree and
reports the full output; the gate exits 0 (or the W-8 honest
exception is documented) before stage 2 reports done.

## 8. Rulings requested (defaults adopted by the prompts)

1. **The legacy wire read-side shim** (D-10): *keep*
   `fold_legacy_habit_log`/`union_habit_log`/`lane_gc_old_habit_log`/
   `HabitLogRecord`/the section field; delete only the write-side
   `habit_log_for`. Default reasoning: deleting the read side now
   would make an old peer's relayed legacy section inert for up to
   180 d (D1's convergence promise breaks mid-transition). The
   alternative (delete both sides now) is only safe once the fleet is
   uniformly upgraded — tracked as a backlog issue with an age-out
   gate. *Recommended: keep (as planned).*
2. **#1550's compound affordance** (D-5): *no* unskip+log compound on
   the Today row — R-1 keeps the surface matrix closed; `UnskipHabit`
   is the recovery verb and the journey plan says so. The alternative
   (a compound affordance) is a new user-visible verb family needing
   its own UX round. *Recommended: no (as planned).*
3. **The pre-#1520 undo-guard downgrade** (D-3): *accept* the
   count/posture-grain guard for legacy entries only (whole-row clock
   compare impossible once the mirror dies). The alternative is
   typed-refusing all pre-#1520 habit undos after upgrade — honest but
   destroys still-working history correction for a strictly internal
   reason. *Recommended: accept (as planned).*

## 9. Stage prompts (verbatim; copy-paste)

The long-command discipline (binding, restated verbatim at the top of
each prompt):

> **Long-command discipline** (binding — docs/agent_dispatch.org): a
> subagent is killed after ~600s without observable activity, so long
> silent commands kill working agents. (1) *The coverage trio never
> runs inside a subagent*: a stage-2/stage-3 agent's gate is the
> pre-commit hooks (automatic) plus
> `scripts/run-quality-gates.sh cargo nextest run --workspace`
> (flock-safe; background + poll with useful work between polls if
> needed); the dispatcher runs the full AGENTS §10 suite between
> stage 2's landing and stage 3's ship. *Flock-deferral rule: when the
> gate lock is contended and no other useful work remains, never
> block — commit all landed work, report "gate deferred: lock held by
> <holder>", and end the stage; the dispatcher runs the gate.*
> (2) *Stage 3's scope ends at PR-open*: push, open the PR, report —
> the dispatcher owns `gh pr checks --watch`, `mergeStateStatus
> CLEAN`, teardown, and at-merge issue closures. (3) *Any command
> expected over ~5 minutes runs backgrounded* with short interleaved
> polls and useful work between polls — commit all landed work BEFORE
> any wait. Large scratch under /var/tmp, never /tmp. (4) *Every
> agent-launched cargo invocation routes through
> `scripts/memory-gate.sh`* — nothing bare. (5) *Stage prompts
> restate this discipline verbatim.* **Never commit with `--no-verify`
> or any hook bypass.** **Never let a test or binary touch the user's
> real taskboard data** (isolated config/data dir under /var/tmp;
> never `~/.local/share/taskboard` or `~/.config/taskboard`). **The
> required SonarCloud quality gate fails a PR whose NEW code has more
> than 3 % duplicated lines.**

### Prompt 5.2 (stage 2 — IMPLEMENTATION)

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the habit event-log dispatch ("consumers + deletions + bookkeeping", branch feat/ui-habit-event-consumers).

**Long-command discipline** (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) *Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`* — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** **Never let a test or binary touch the user's real taskboard data** (isolated config/data dir under /var/tmp; never `~/.local/share/taskboard` or `~/.config/taskboard`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines.**

Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-ui-habit-event-consumers (branch feat/ui-habit-event-consumers, stacked base origin/feat/sync-habit-events-wire = 6bda37c2 — do NOT rebase; the dispatcher owns the post-#1569-merge rebase). Never touch /home/viktoria/src/taskboard (the main checkout) or any other taskboard-WT-* directory. If the worktree is missing, recreate it: git fetch origin --prune (from the main checkout) && git worktree add ../taskboard-WT-feat-ui-habit-event-consumers feat/ui-habit-event-consumers (the branch already exists; never -b, never on main).

1. Read AGENTS.md, then in the worktree: the stage-1 plan .artifacts/plans/2026-11-28-habit-event-consumers-plan.md (binding — its §2 commit sequence and §4 deletion checklist are the work program; its recorded decisions D-1..D-12 are settled; deviations need a commit-body reason), the design .artifacts/plans/2026-11-27-habit-event-log-design.md §5/§7/§8, the UX journey plan .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md, the dispatch plan §1/§6, and issues #1542/#1550/#1549/#1555 (the operator's conditions the plan implements).
2. Implement exactly per the approved plan's §2 commit sequence C1..C9, TDD, every commit standalone (rebase-merge hygiene). Hard rules: the UI stays a dumb presentation client (AGENTS §1 — derivation calls are domain functions; presentation only renders); clock discipline per docs/testing_strategy.org §6 (frozen render clock for committed goldens — the UiSettings seam is untouched); deletions are their own commits with the §5 grep evidence in the body (C6 carries #1542's list); no tokio::time::sleep in tests; typed assertions only (matches!/structural — no text assertions, including on the new help text and badge wording); every snapshot/PNG change cites its reason in the commit body (plan §3); the sqlite remap's legacy-table reads (repo.rs:1697–1715) MUST survive C5 (plan D-9); the read-side wire shim survives C8 (plan D-10); HabitLogKey/HabitLogEntry survive as decode-only retired shapes (plan D-12).
3. Run the fast gate per the long-command discipline above (pre-commit hooks automatic; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls; commit BEFORE waits; scripts/memory-gate.sh on every cargo call; the coverage trio never runs here; flock deferral = commit + "gate deferred: lock held by <holder>" + end).
4. After C8: run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --root /home/viktoria/src/taskboard-WT-feat-ui-habit-event-consumers --gate — every row the plan §7 lists must read FOUND; the W-8 row follows the plan §7 item 10 instruction exactly (investigate the gate's actual complaint; fix a plan-row format defect; a genuine gate bug becomes a backlog issue + honest report, never a paper-over). Report the full gate output.
5. Post the issue comments per the plan §6 (verbatim drafts; fill the PR/issue placeholders honestly — cite the branch and commit SHAs; the PR number is stage 3's, so reference the branch). Do NOT close any issue (dispatcher-owned at merge). Open backlog issues per AGENTS §7 standing authorization for any out-of-scope finding (the plan §8 ruling 1's shim-age-out issue #1571 is stage-1's — verify it exists, comment if not); list all issue numbers in your report.
6. No push, no PR. Never push to or merge main. Verify the §5 grep list is clean before reporting; attach its output.

Report: per-commit gate results, the deletion-checklist completion evidence (§4 boxes ticked in your report text), the grep output summary, the ux-coverage gate output, snapshot/PNG churn justifications, deviations.
```

### Prompt 5.3 (stage 3 — REVIEW)

```text
You are the stage-3 REVIEW agent for chunk 5 of the habit event-log dispatch (branch feat/ui-habit-event-consumers).

**Long-command discipline** (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) *Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`* — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** **Never let a test or binary touch the user's real taskboard data** (isolated config/data dir under /var/tmp; never `~/.local/share/taskboard` or `~/.config/taskboard`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines.**

Work ONLY in /home/viktoria/src/taskboard-WT-feat-ui-habit-event-consumers (branch feat/ui-habit-event-consumers, stacked base origin/feat/sync-habit-events-wire = 6bda37c2, PR #1569 possibly unmerged). Never touch /home/viktoria/src/taskboard or another taskboard-WT-* directory.

1. Read AGENTS.md, the design §5/§7/§8, the dispatch plan §1/§6 (the code-quality mandate below applies), the UX journey plan, and the stage-1 plan (.artifacts/plans/2026-11-28-habit-event-consumers-plan.md); then review the full branch diff (git diff 6bda37c2...HEAD — the stacked base; NOT origin/main, which lacks chunks 1–4 until #1569 merges).
2. **Stacked-base rebase rule (replaces the standard stage-3 rebase step)**: git fetch origin --prune; if PR #1569 has merged (origin/main contains origin/feat/sync-habit-events-wire's tip), do the AGENTS §8 rebase dance onto origin/main (drop the merged duplicate commits; push --force-with-lease) and diff against origin/main. If NOT merged: do not rebase, do not force-push; review and ship against the stacked base and note the stacking in the PR body — the dispatcher owns the post-merge rebase.
3. Functional focus, named from the origin: (a) **deletion safety (the chunk's core duty)**: after the deletions the workspace compiles with zero references to the deleted symbols — re-run the stage-1 plan §5 grep list and confirm every hit is a documented survivor; a fresh install with no legacy data behaves identically before/after (no code path reads the deleted maps); a pre-0032 database still migrates — verify the remap's legacy-table SELECTs (repo.rs:1697–1715) survive and the remap-still-fires test exists; the undo of pre-#1520 journal entries still works through the retained legacy inverse paths with guards re-keyed onto derived counts/posture (plan D-3) — the test named in D-3 must exist and pin it; (b) the heatmap derives from events via derive_day_counts with the frozen render-clock seam intact in committed goldens; (c) the #1341 phantom-cell and #1340 cross-device parity presentation evidence exists; (d) CLI outcomes over derived state, typed assertions only, the D-4 receipt deltas as recorded; (e) dormant sqlite tables untouched (no migration edited); the write/read arms gone, the remap intact (plan D-9); (f) no UI-side habit semantics (AGENTS §1/§2 — presentation derives via domain functions only); (g) the read-side wire shim retained, the write-side builder gone (plan D-10); (h) keystone proptest removed only in C6 with the mirror (plan D-11); (i) bookkeeping comments posted with design-doc permalinks, no issue closed.
4. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crates; the derivation fold never leaves the domain crate; the UI never computes habit semantics); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
5. Re-run the fast gate per the long-command discipline (backgrounded flock-wrapped nextest via scripts/run-quality-gates.sh; scripts/memory-gate.sh on every cargo call; commit before waits; flock deferral = commit + "gate deferred: lock held by <holder>" + end). The coverage trio never runs in this session.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. **UX completeness mandate**: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --root /home/viktoria/src/taskboard-WT-feat-ui-habit-event-consumers --gate — EVERY P1 journey row must read FOUND (the W-8 exception only if stage 2 documented a genuine gate bug with a backlog issue); then execute the walkthrough protocol (docs/ux_planning.org) over every P1 story (US-1..US-5) — the four walkthrough questions per step with evidence, then the six-heuristic pass over the Today habit rows, the Review history grid and the CLI habit surfaces; BLOCKER/MAJOR findings fixed on the branch, MINOR findings become backlog issues. Pay special attention to the #1550 wording (the skipped-row note) and the #1549 help-text statement (plan D-5/D-7).
8. Ship: push the branch (plain push if never force-pushed; --force-with-lease only after a recorded rebase), open the PR to main with gh (body: the chunk's scope, the stacked-on-#1569 note if unmerged, the deletion/grep evidence summary, the ux-coverage result), then report the URL — do NOT run gh pr checks --watch; the dispatcher owns CI watch, merge-state verification, the post-merge rebase of stacked descendants, teardown (git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-habit-event-consumers; confirm via git worktree list), and at-merge issue closures.
9. Backlog issues per AGENTS §7 standing authorization (search first); list the numbers.

Report: verdict, findings (functional + code-quality + deletion-safety), what you fixed, the grep re-run result, the walkthrough verdict, PR URL, worktree-removed confirmation. Never merge; never touch main.
```

## 10. Risks

- **C1 is the largest commit** (every domain projection signature +
  the habit.rs test modules' fixture re-key). Mitigation: the keystone
  proptest stays green through C1–C5 — it is the mechanical-rekey
  oracle exactly as the chunk-2 plan promised; if it fails, the re-key
  is wrong, not the test.
- **Golden PNG churn** may exceed the two predicted classes; the per-PNG
  justification rule (§3) keeps honest churn auditable. A Day-kind
  golden changing is a stop-and-investigate signal.
- **W-8's gate MISSING** may be a gate-parser limitation; §7 item 10
  scopes the honest handling.
- **The stacked base**: CI on the PR (opened against main while
  #1569 is unmerged) will show the whole stack's diff and possibly
  phantom conflicts; the dispatcher owns the post-merge rebase
  (stage-3 prompt rule 2). SonarCloud's new-code duplication gate
  measures against the wrong baseline until the rebase — noted for
  the dispatcher, not a stage-3 blocker.
- **The #1549/#1550 wording** lives in help text and plan prose, both
  un-assertable by the no-text-assertions rule; their verification is
  the stage-3 walkthrough, not a test.

## 11. Estimated diff size

~2,500–3,500 lines: deletions dominate (~1,200: the mirror writers,
persistence/storage arms, day types, `habit_log_for`, the keystone);
the re-key touches ~40 production sites; test churn ~1,000–1,500
(habit.rs modules, state suite, UI journey fixtures, sqlite corpora);
docs/bookkeeping ~150.
