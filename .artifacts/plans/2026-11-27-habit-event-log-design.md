# The Habit Event-Log Design (#1519) — Design Round

- **Date**: 2026-11-27
- **Kind**: design round — the full technical design for the per-completion
  timestamped habit event log that replaces the window-count rows and the
  day-overlay twin. Docs-only; no code.
- **Issue**: #1519 (operator ruling 2026-11-27; supersedes the E3 design
  round in full — `.artifacts/plans/2026-11-25-habit-sync-tails-design.md`
  §1 remains the accurate recorded-reality inventory, its decisions are
  moot).
- **Origin findings**: granularity audit §F-1 (the design-round brief,
  items 1–7), §F-2 and §F-3 (both ride this refactor's wire bump), §5
  (executive table), §6 (anti-examples — not touched here), §8.
  `.artifacts/reports/2026-11-27-source-of-truth-granularity-audit.md`,
  main permalink:
  https://github.com/D4id4los/taskboard/blob/main/.artifacts/reports/2026-11-27-source-of-truth-granularity-audit.md
- **Recorded reality**: cited at `origin/main` (`9fc0d5ee`) file:line.
- **Companion artefacts**: UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md`; dispatch plan
  `.artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md` (written
  after this design and the UX plan, per the ux_planning law).
- **Status**: ACCEPTED — the operator accepted every §0 ruling A–I on
  its recommended default (2026-11-27); no ruling was overruled.

---

## 0. Rulings requested

Each ruling lists the options, the recommended default (which the plans
adopt everywhere so they are complete and dispatchable), and what was
assumed. The operator can overrule individual items without invalidating
the rest.

### Ruling A — migration attribution: where synthesized events stamp

**Question**: when the persisted window rows and day-overlay rows
migrate to events, which completion instants do the synthesized events
carry?

- **A1. Stamp every synthesized event at the row's stored `clock`** (N
  events per window row share the instant; distinct synthetic ids keep
  them separate). Uses only recorded facts; invents nothing.
- **A2. Spread N events evenly across the window** (one per day inside
  a Day window, etc.). Prettier heatmap pixels, but forges instants
  that were never recorded.

**Default: A1, with one refinement** — where a `habit_day_log` twin row
exists for the same habit *and* the habit's current target kind is Day,
the twin's day is recorded reality about which calendar day the
completions landed on, so the synthesized event stamps at the twin
row's clock (its day attribution) instead of the window row's. Week /
Month rows and twin-less Day rows stamp at the window row's clock.

**Assumed**: "display-acceptable" (the issue's wording) means
historically approximate, not pixel-pretty — A1 never invents data.
Consequence: migrated Week/Month windows show their completions on one
day (the row-clock day) in the heatmap; after the retention window
(≤ 180 d) ages them out, the artifact disappears.

### Ruling B — the skip mark: typed skip event vs derived posture

**Question**: F-2 — per-window skip becomes a per-window derived
posture or a typed skip event?

- **B1. Derived posture**: skip computed from the completion events.
  **Not available**: a skipped window has *absence* of completions,
  indistinguishable from not-done; skip is user-asserted intent, and a
  derivation cannot conjure it.
- **B2. Typed skip/unskip events in the same event log**: `SkipHabit`
  emits one `SkipMark { window }` event, `UnskipHabit` emits one
  `UnskipMark { window }` event; the per-window skip *posture* is
  derived at read time (a window is skipped iff its newest skip mark
  outranks its newest unskip mark under the `(at, id)` order). Union
  fold over the log is uniform — no separate section, no tombstone
  grammar for the marks.

**Default: B2.** The posture derivation is a tiny deterministic
reduction (max-(at, id) per window over two event kinds), not row-LWW
over a count: no increment can ever be dropped through it (marks carry
no count), so F-2's S1 class is dissolved without giving up
set-union convergence. Concurrent Skip vs Unskip on two devices
converge to "later intent wins" — the correct semantic, stated in the
journey plan (US-5).

**Assumed**: skip remains scoped to the current canonical window at
mark time (R-4 posture, `command.rs:3377–3379` verbatim) — marks are
per-window facts keyed by the *derived* window of their completion
instant, so a later kind/day-start change re-derives their window
membership like every other event.

### Ruling C — corrections and undo: per-event deletion tombstones

**Question**: how do exact inverse (remove an event by id) and audited
corrections sync, given a set-union fold cannot express removal?

- **C1. Deletion fact family**: separate `Deletion { deleted_event_id }`
  rows in the same union; posture = event not in the deletion set.
- **C2. Per-event tombstone stamp**: the event row carries an optional
  `deleted: Option<Tombstone { at, by }>` (default-absent); union
  merges the row by id and the newest tombstone wins; a tombstoned
  event is excluded from every derivation.

**Default: C2.** One row per fact, one fold; the tombstone rides the
same codec slot as the event (the `deleted` key the existing
`HabitLogRecord` decoder already fences against, `appdata.rs:1283` —
the shape was anticipated). `AdjustHabitLog { habit, window, count }`
becomes a diff: tombstone or un-tombstone / mint synthetic-unsigned
events until the window's derived count equals the audited target
(freeing `count: 0` to express row-absence exactly — the D24 fence
dies). Undo of `LogHabit` tombstones exactly the minted event id (the
receipt carries it); undo of an adjust restores the pre-image
tombstone state — exact, per-event. The F-3 `HabitSurface` residue
(#1520's D24/D25 halves, `journal.rs:131`, `undo.rs:421`, `:442`)
dissolves.

**Assumed**: tombstones are retained under the same GC window as the
event they stamp (a tombstone cannot outlive its row) and a GC'd
tombstone cannot resurrect (retention is ≥ the fold horizon on every
peer — same law that governs today's 180 d wire GC, `appdata.rs:2634`).

### Ruling D — old wire documents: conversion shim vs retention age-out

**Question**: what do pre-existing `habit-log` sections in synced
documents do?

- **D1. Age-out**: new writers stop emitting the legacy `habit-log`
  section entirely; a new reader still *folds* a legacy section it
  receives (read-side shim: legacy window rows synthesize
  deterministic-id events — the Ruling-A1 attribution at the row
  clock), and the legacy rows age out of old peers' documents under
  the existing 180 d wire GC (`lane_gc_old_habit_log`,
  `appdata.rs:2729`). Fully converged within ≤ 180 days of fleet
  upgrade; no dual-emission period, no new codec state.
- **D2. Conversion shim both ways**: new writers also emit the legacy
  section derived from events for old peers. Doubles the flush
  builders and reintroduces window-keyed emission — the exact shape
  we are deleting — for ≤ 180 days of compatibility.

**Default: D1** (the audit's stated cheaper option). Old peers lose
habit-log *sync freshness* during the transition (they still hold
their local state; the unknown-section relay keeps a new peer's
`habit-events` section honest-but-inert on an old peer,
`appdata.rs:79`), state stays deterministic on both sides.

**Assumed**: mixed-fleet operation is a transitional nuisance, not a
supported end state (single-user multi-device fleet upgrades fast).

### Ruling E — GC / retention window

**Question**: F-1 brief item 3 — bound the wire GC by
max(heatmap horizon, streak lookback) + margin.

**Analysis**: the heatmap horizon is one visible month (~31 d, with
unlimited month paging backwards — presentation.rs `habit_month_flipped`,
:8845). The streak lookback is unbounded by construction (the walk
terminates at the first miss, `habit.rs` `streak_with_break`), so a
true bound does not exist; today's wire retention is already
180 d (`HABIT_LOG_SYNC_RETENTION_DAYS`, `appdata.rs:2634`) and a
fresh peer already cannot reconstruct streaks older than that.

- **E1. Keep 180 d, re-pointed at the event `at` instant** (constant
  renamed `HABIT_EVENT_SYNC_RETENTION_DAYS`; `habit_log_cutoff` →
  `habit_event_cutoff`). Same observable window as today; events past
  the horizon drop from the *wire* document only (local history keeps
  them; the `lane_gc_old_habit_time_entries` precedent,
  `appdata.rs:2545`).
- **E2. Tighter bound** (heatmap month + margin ≈ 35 d). Smaller
  documents, but changes observable sync behavior and silently
  shortens fresh-peer streak reconstruction from 180 d to ~5 weeks.

**Default: E1.** 180 d ≫ heatmap horizon + margin, so the brief's
formula is satisfied with slack; the streak-lookback bound is already
180 d de facto today and Ruling F makes that limitation explicit
rather than papered over.

**Assumed**: long-streak (> 180 d) fresh-peer reconstruction stays a
recorded limitation (it is one today; an honest unbounded streak would
need retention on all peers forever — a product decision, not a
granularity one; if raised, it belongs in a `backlog:` issue).

### Ruling F — `habit_day_log` sqlite table disposition

**Question**: drop the table, or keep it dormant?

**Default: keep the table dormant** (migration-immutability law,
ADR 0005 — shipped migrations are never edited, `repo.rs:203`): the
new migration creates `habit_event`, the load path stops reading
`habit_log` / `habit_day_log`, the tables remain as inert history of
the schema. Code references are deleted; the tables are not.

**Assumed**: no user ask for an export of the legacy tables exists.

### Ruling G — F-4 (time-entry per-endpoint stamps, #1521): ride now vs follow-up

**Question**: F-4 shares the wire-version bump moment — ride or not?

- **G1. Ride now**: one wire bump exercises both refactors; but the
  combined blast radius (F-1 is the audit's largest-radius finding;
  F-4 is "moderate") risks a mega-PR the long-command discipline
  cannot chunk cleanly, and F-4's migration (synthesize both stamps
  from `updated_at`) is independent of the habit work.
- **G2. Follow-up**: F-1 lands first; F-4 keeps its own design round
  (the audit §5 already schedules it as design round 2).

**Default: G2.** F-4 is additive-flavored (per-field stamps, default-
absent decode — "one cycle of reduced precision, then convergence",
audit §F-4) and does not need this bump to proceed; coupling it would
hold a p1 refactor hostage to a second review surface.

**Assumed**: F-4's eventual wire delta is additive and needs no
`APPDATA_FORMAT` bump (the envelope never bumps for additive change,
`appdata.rs:30–32`); there is no shared "version" being economized.

### Ruling H — `AdjustHabitLog` surface semantics

**Question**: the CLI/UI audited-correction verb takes
`(habit, window, count)`; under the event log, what does it mint?

**Default**: it becomes a *reconcile* — derive the window's current
count from events, then emit the minimal tombstone/un-tombstone/mint
batch (Ruling C) to reach `count`. When the window's kind-membership
changes later, the audited fact re-derives like any completion
(Ruling B's posture law); the receipt names the events it created or
tombstoned. Existing CLI grammar (`habits adjust <habit> --window
--count`, `crates/taskboard-app/src/commands/habits.rs:105`) is
unchanged.

**Assumed**: no user-visible new verb is needed (the UX plan's matrix
keeps `habit.adjust` on the existing command).

### Ruling I — deterministic synthetic event ids (migration + legacy shim)

Synthesized events (sqlite migration, legacy wire fold) need ids that
are (a) deterministic across devices and replays, (b) never colliding
with locally minted v7 ids. **Default**: UUIDv5 over a fixed namespace
UUID and the wire form `"legacy-habit-log|<habit uuid>|<window date>|<row clock rfc3339>|<ordinal>"` — pure, reproducible from the
recorded facts, version-tagged so the recipe can change once.
Documented at the minting site; the A1 pattern's mint-at-trust-boundary
rule applies (the migration and the legacy fold are trust boundaries).

**Assumed**: uuid's v5 feature is enabled (workspace pins
`uuid = { version = "1", features = ["v4", "v7", "serde"] }`,
`Cargo.toml:93` — **the `v5` feature must be added**; flagged in the
dispatch plan's chunk 3).

---

## 1. The event model (domain)

### 1.1 Types (house A1 pattern)

New module `crates/taskboard-domain/src/habit_event.rs`
(sibling of `habit.rs`; same crate, zero new dependencies — the
domain-crate discipline holds):

```rust
/// Identity of one completion or one skip-intent mark. Minted at log
/// time via the IdGenerator seam (never derived from the completion
/// instant — Ruling: same-second completions stay distinct).
pub struct HabitEventId(Uuid);          // private-field newtype; fallible
                                        // minting at the trust boundary
pub enum HabitEventKind {
    /// One completed quota unit. Carries no window: the window is
    /// derived at read time (the whole point — §2).
    Completion,
    /// A deliberate planned-skip mark for one derived window (Ruling B).
    Skip { window: WindowStart },
    /// Withdrawal of the window's skip mark (Ruling B).
    Unskip { window: WindowStart },
}

pub struct HabitEvent {
    pub id: HabitEventId,
    pub habit: HabitId,
    pub kind: HabitEventKind,
    /// The completion / mark instant — the event's own fact. No clock
    /// inflation: the #986 `mint_log_clock` bump is deleted with the
    /// row clock (union fold needs no arbitration, §3).
    pub at: DateTime<Utc>,
    /// Display-only author (the F1 `by` leg survives for display,
    /// never for arbitration).
    pub by: Option<AuthorId>,
    /// Deletion tombstone (Ruling C2): default-absent; the newest
    /// `(at, by)` tombstone wins in the fold.
    pub deleted: Option<HabitEventTombstone>,
}
```

A1 policy notes: hand-rolled newtype, fallible `HabitEventId::parse`
/ minting only via the id seam; deliberate leniency documented at each
`Option` (a tombstone `at` later than `at` itself is legitimate —
corrections happen after the fact); no validate-and-forget (the fold
re-checks membership against the live habit set, as `habit_log_for`
does today at `appdata.rs:2707–2716`).

IdGenerator gains `fn new_habit_event_id(&self) -> HabitEventId;`
(`idgen.rs:31–68` trait; the `new_habit_time_entry_id` twin
precedent).

### 1.2 Event-id minting — closes #1488 (verified mechanism)

`taskboard_domain::idgen`'s `UuidV7Generator` (`idgen.rs:54`) mints
`Uuid::now_v7()`, which builds `Timestamp::now(NoContext)` — no
monotonic counter, so same-millisecond ids order by random low bits
(#1488's finding). The workspace runs uuid **1.29.0** (workspace
`Cargo.toml:93` resolved), and `pub use timestamp::context::ContextV7`
ships since uuid 1.18 (`~/.cargo/registry/.../uuid-1.18.1/src/lib.rs:244`)
— the mechanism exists, unadopted (a workspace-wide grep for
`ContextV7` finds zero uses).

Design: `UuidV7Generator` holds (or references, via a
`OnceLock<ContextV7>` process-global in `idgen.rs`) a
`uuid::ContextV7`; every mint goes through
`Uuid::new_v7(&Timestamp::now(&CONTEXT))` instead of `now_v7()`. All
id kinds benefit, but this refactor's fold and GC depend on `(at, id)`
determinism, so #1488 lands **inside chunk 1** with its own
behavior-identity note (#1488's own wording: tie *outcomes* corpus-wide
change; rare inversions become impossible). The test seam
(`merge_testutil::CountingIds`) is untouched.

### 1.3 State placement

`HabitState` (`habit.rs:835`) gains:

```rust
pub events: BTreeMap<HabitEventId, HabitEvent>,   // serde(default)
```

and **loses** `log` (`habit.rs:839`) and `day_log` (`habit.rs:843`) —
the two-sources-of-truth shape dies; §5 covers the deletion mechanics.

`BTreeMap<HabitEventId, _>` gives deterministic serialization and a
(mint-instant-ordered, v7) iteration order that the derivation in §2
relies on for stable tie-breaks.

---

## 2. Read-time derivation (the one source of truth)

### 2.1 Window attribution — `derive_window_counts`

The `log_window` attribution (`habit.rs:913–967`: natural window, the
24 h grace, the surplus-repair guarded on "natural already met quota")
is preserved **verbatim as the per-event attribution rule**, but
evaluated *as of each event's completion instant*:

```rust
/// Deterministic fold: process the habit's live completion events in
/// (at, id) order; maintain running per-window counts; attribute each
/// event to `log_window(habit, counts_so_far, event.at, day)`.
pub fn derive_window_counts(
    habit: &Habit,
    events: impl Iterator<Item = &HabitEvent>,  // live completions only
    day: DayCalendar,
) -> BTreeMap<WindowStart, u32>
```

Determinism: the (at, id) order and the pure `log_window` arithmetic
make the result a pure function of the event set + current habit
config. The recursion worry (attribution depends on counts, counts
depend on attribution) resolves exactly as the original did —
`log_window` at log time also read the *stored* counts; the fold
reproduces that order online.

Re-derivation on config change: **free by construction**. A habit-kind
change or day-start change re-runs the fold with the new config and
re-derives *all* history — the D-H12 stranded-former-kind-row luggage
(`habit.rs:846–856`) has no successor shape: there are no keys minted
from the current kind anymore. (A Skip/Unskip mark's `window` field is
the one stored derived value — it is re-derived the same way: marks
attribute to the window `log_window` would pick for their `at`, and
the stored `window` field is recomputed on read for marks that
predate a config change. Chunk 1 pins this with a property test:
marks re-derive under kind change exactly like completions.)

Cost: O(n log n) over the habit's live events (≤ 180 d retained on the
wire; local keeps more). Kiosk-scale fine; if a profile ever carries
tens of thousands of events, a memoized per-render cache is the
follow-up (backlog, not designed here).

### 2.2 The projections

Every current consumer of `HabitState.log` re-keys onto the fold:

| Projection | Today | Becomes |
|---|---|---|
| Streak walk (`streak`, `habit.rs:985+`, `streak_with_break`) | per-window `count`/`skip` lookups | `derive_window_counts` + `derive_skip_posture`; the walk body (pending/missed/grace, pause, preschedule, skip-transparency) is **unchanged logic over derived inputs** |
| Quota check (today-met badge, `habit.rs` count closures `:1000–1090`) | row count | derived count of the natural window |
| Day heatmap (`presentation.rs:8796` `habit_history_day_cells`, wired `models.rs:7576`) | `HabitState.day_log` rows | derived per-day counts: every completion event attributes to a *day* (`day.today(event.at)`) under the same (at, id) fold — the twin's job, now derived from synced facts (#1340 dissolves) |
| Week/Month windows | row count | derived count of the window |
| CLI surfaces (`crates/taskboard-app/src/commands/habits.rs:332, 650, 998–1007`) | row reads | derived reads |

`log_window` itself survives as the attribution rule used *inside* the
fold (its `count` closure reads the fold's running map, not
`HabitState.log`).

### 2.3 Skip posture — `derive_skip_posture`

Per Ruling B: per window, `skip_posture = newest SkipMark outranks
newest UnskipMark under (at, id)`. Consultation order in the streak
walk is unchanged (`habit.rs` walk: quota-met → skip-transparent →
preschedule → grace).

---

## 3. Writes (commands) and the exact inverse

### 3.1 `LogHabit` (`command.rs:3226–3283`)

Becomes: derive the target window with `log_window` as of `now`
(unchanged rule), mint **one** `Completion` event (`id` from the id
seam, `at = now`, `by = author`). No twin write, no clock bump. The
receipt (`CommandOutcome::HabitLogged`, messages.rs) gains the minted
`HabitEventId` — the undo needs it (§3.4); the count in the receipt
becomes the *derived* post-count (display only).

### 3.2 `AdjustHabitLog` (Ruling H, `command.rs:3285–3372`)

The reconcile diff: derive the window's current (count, tombstone
state); emit `DeleteEvent(id)`-shaped tombstone stamps and/or new
`Completion` events until derived count == audited target. A `count`
below current preferentially tombstones the *newest* events (the
(posterior) (at, id) order — documented rule: an audit removes the
most recent completions first). The canonical-window gate
(`command.rs:3294–3300`, the D-H12 write-side gate) **dies** — any
window of the current kind is addressable, and former-kind windows
simply have no events; an adjust on a former-kind window is a typed
rejection still (`InvalidHabitWindow`) or, better, derives as empty
and mints into it — decision: **keep the current-kind gate** (a
former-kind window's events still re-derive under the current kind,
so adjusting "a window that no longer exists" is a user error worth
rejecting; recorded, not silent).

### 3.3 `SkipHabit` / `UnskipHabit` (`command.rs:3374–3436`)

Each emits one mark event (Ruling B); no row rewrite, no clock mint;
idempotent no-op checks run against the derived posture.

### 3.4 Undo — the inverse becomes exact (F-3, #1341, #1094)

- `LogHabit` inverse: tombstone the receipt's minted event id. Exact
  by construction; no pre-image needed beyond the id (the journal
  `PreImage::HabitLogRow`, `journal.rs:171–175`, and `undo.rs`
  `:752–754`/`:803` row pre-images are deleted).
- `AdjustHabitLog` inverse: the pre-image is the window's pre-adjust
  (tombstone-state, synthesized-event-set) snapshot — restored
  exactly (un-tombstone / tombstone / delete synthesized events).
  The absent-window refusal (`undo.rs:421`, D24) has no successor
  case: an empty pre-image restores emptiness exactly.
- `UpdateHabit` kind-conversion seam refusal (`undo.rs:442`, D25):
  dissolves — windows are derived, there is no clock slot to
  watermark against anymore.
- `SkipHabit`/`UnskipHabit` inverses: tombstone the receipt's mark id.

`NotUndoableReason::HabitSurface` (`journal.rs:131`) and the whole
`undo.rs:415–510` habit classification surface shrink to the
reflection/reminder entries that remain policy-fenced. The
lossy-inverse class (journal.rs:121–130 per the audit) is emptied of
habit members.

---

## 4. Sync: wire codec, fold, GC

### 4.1 Section shape

`AppDataSections` (`appdata.rs:116`) gains an additive field:

```rust
pub habit_events: Option<BTreeMap<String /* event uuid */, HabitEventRecord>>,
```

Additive default-absent ⇒ **no `APPDATA_FORMAT` bump** (the envelope's
own law, `appdata.rs:30–32`); old readers file the unknown section in
`unknown` verbatim (`appdata.rs:79`) and never fold it — the honest
boundary.

`HabitEventRecord` (new enum, the `HabitLogRecord` grammar):
`Event { kind, at, by, deleted } | Verbatim(Value)`, with the
mint-strict / decode-lenient posture and the `deleted` key handled
exactly as `HabitLogRecord`'s decoder fences it today
(`appdata.rs:1283`). Map key: the event uuid string (the habit id
rides the row — rows for foreign-board habits must be droppable by
key-prefix scan for GC; keying by uuid alone keeps the fold
habit-agnostic, the fold's habit-existence check is per-row, the
D-H6 posture preserved verbatim from `fold_habit_log`'s doc comment,
`merge.rs:1508–1520`).

### 4.2 The fold — `fold_habit_events` (replaces `fold_habit_log`, `merge.rs:1521–1561`)

Union by uuid: a locally-absent uuid adopts its row; an identical uuid
is the same fact — the only competition is the tombstone slot, and the
newest `(deleted.at, deleted.by)` wins (no `(clock, by)` tuple duel
over a count anywhere; two devices each logging write two distinct
uuids and the union keeps both — the S1 headline defect and #1094's
G-Counter question both dissolve by construction). Verbatim rows never
fold. `union_habit_log`'s document-level merge (`appdata.rs:1811`,
called at `:1634`) gains the `habit_events` twin with union-by-key
semantics (identical keys are identical facts; no clock comparison at
all — the `union_habit_log` overlay-tie leniency at `appdata.rs:6221`
dies with it).

### 4.3 Emission, GC, retention

- Builder `habit_events_for(...)` (the `habit_log_for` shape,
  `appdata.rs:2699`): same fail-closed filter — habit exists locally,
  `habit.board == board`, `event.at >= cutoff(today)` (Ruling E1).
- `lane_gc_old_habit_events` (the `lane_gc_old_habit_log` shape,
  `appdata.rs:2729`): strips rows whose `at` (or, unparsable,
  nothing — fail-closed verbatim posture preserved) predates the
  cutoff. Wire-only; local rows never GC'd.
- The legacy `habit-log` section: emission **deleted**; the fold side
  keeps the legacy shim (Ruling D1) until the retention window makes
  it dead code — the shim's removal is chunk 5's cleanup, gated on a
  constant (one release cycle after fleet upgrade; the dispatch plan
  schedules it as a documented removal with its own commit).

### 4.4 Connector wiring

Both connectors touch the same five seams they touch today:
Deck flush builders (`flush.rs:173, 439, 485, 500`), files board-cycle
lanes (`board_cycle.rs:1663–1727` — the `habit_log` lane at `:1673`
becomes `habit_events`; the `habit_time_entries: None` boundary is
**untouched** — #1363 stays its own p3 item, unaffected by this
refactor).

---

## 5. Deletion of `HabitDayLogEntry` / `day_log` (the transactional-twin law)

- `HabitDayKey` / `HabitDayLogEntry` (`habit.rs:680–763`), `HabitState
  .day_log` (`habit.rs:843`), `PersistenceAction::RecordHabitDayLog`
  (`persistence.rs:530`), its apply arm (`persistence.rs:690–702`),
  the twin writes inside `LogHabit`/`AdjustHabitLog`
  (`command.rs:3249–3262`, `:3331–3348`) and `RecordHabitDayLog`'s
  sqlite flush (`repo.rs:2929–2941`) and load (`repo.rs:1438–1450`,
  `:1860`, `:2319`) — **all deleted**. The "transactional-twin law"
  (both writes in one batch, the pinned partial-write bug class) is
  deleted with it: there is no twin.
- `HabitLogKey` / `HabitLogEntry` (`habit.rs:634–763`) and
  `HabitState.log` follow in chunk 5 once every consumer is off them;
  the D-H12 documented-leniency block (`habit.rs:846–856`) is deleted
  (no successor shape exists — §2.1).
- `mint_log_clock` (`habit.rs:801–812`) is deleted with the row clock
  (#986's bump was LWW machinery; union fold needs none). Its
  behavior-identity note: same-instant double logs now stay distinct
  as two events — strictly *more* faithful than the bump, which fused
  them into one count under two stamps.

---

## 6. Storage: sqlite migration

### 6.1 Schema

Migration `0032_habit_events.sql`:

```sql
CREATE TABLE habit_event (
    id TEXT PRIMARY KEY,        -- event uuid
    habit_id TEXT NOT NULL,
    kind TEXT NOT NULL,         -- 'completion' | 'skip' | 'unskip'
    window TEXT,                -- mark target window date; NULL for completion
    at TEXT NOT NULL,           -- rfc3339
    au_by TEXT,                 -- author (0023/0026 column grammar)
    deleted_at TEXT,
    deleted_by TEXT
);
CREATE INDEX idx_habit_event_habit_at ON habit_event (habit_id, at);
```

`habit_log` (migration 0017 + 0023 + 0028 columns) and `habit_day_log`
(0030) stay on disk, unread (Ruling F).

### 6.2 Load-then-remap

The 0028/0030 migrations are immutable SQL — synthesis needs domain
logic — so the remap happens in Rust at the repository boundary,
**once, guarded**:

- On load (`repo.rs:1426–1450` today), if `habit_event` is empty and
  legacy rows exist: read `habit_log` + `habit_day_log`, synthesize
  events per Ruling A1/I (deterministic v5 ids), insert into
  `habit_event`, and set a one-row `habit_event_migration` marker
  (the `retired_deck`-style single-row posture) so the pass never
  re-runs after the user logs new events mid-upgrade. Rewind
  (`crates/taskboard-storage-sqlite/src/rewind.rs`) is event-grammar
  from then on; legacy tables are never re-consulted after the marker.
- The upsert guards: the whole-second LWW guards (`repo.rs:2894–2910`,
  #1095) die — an event insert is `INSERT OR REPLACE` by primary key
  (identical uuid = identical fact); the tombstone update is a plain
  column write. No LWW at rest (the fold arbitrates nothing; the
  state engine holds the write side per AGENTS §2/§3).

### 6.3 What breaks in tests

Golden corpora and insta snapshots carry the new event shapes; the
round's snapshot churn is expected and pinned per chunk (dispatch
plan, chunk acceptance lists).

---

## 7. Consumer churn list (every file:line at `9fc0d5ee`)

| # | File:line | Change |
|---|---|---|
| 1 | `crates/taskboard-domain/src/habit.rs:634–763` | `HabitLogKey`/`HabitDayKey`/`HabitLogEntry`/`HabitDayLogEntry` deleted (chunk 5) |
| 2 | `crates/taskboard-domain/src/habit.rs:801–812` | `mint_log_clock` deleted |
| 3 | `crates/taskboard-domain/src/habit.rs:835–866` | `HabitState`: `log`/`day_log` → `events` |
| 4 | `crates/taskboard-domain/src/habit.rs:877–967` | `log_window` becomes the fold's attribution rule |
| 5 | `crates/taskboard-domain/src/habit.rs:985–1100` | streak/quota projections over derived inputs |
| 6 | `crates/taskboard-domain/src/habit_event.rs` (new) | event types + `derive_window_counts`/`derive_skip_posture`/`derive_day_counts` |
| 7 | `crates/taskboard-domain/src/command.rs:3226–3436` | LogHabit/Adjust/Skip/Unskip arms rewritten (§3) |
| 8 | `crates/taskboard-domain/src/persistence.rs:519–530, 690–704` | actions: `RecordHabitEvent` / `TombstoneHabitEvent` replace the Record pair |
| 9 | `crates/taskboard-domain/src/journal.rs:121–131, 171–175` | `HabitSurface` deleted; `PreImage::HabitLogRow` → event-id pre-images |
| 10 | `crates/taskboard-domain/src/undo.rs:415–510, 643–647, 752–803` | exact inverses; D24/D25 fences die |
| 11 | `crates/taskboard-domain/src/merge.rs:1501–1561` | `fold_habit_events` replaces `fold_habit_log` |
| 12 | `crates/taskboard-domain/src/appdata.rs:116, 1224–1340, 1634, 1811–1830, 2545, 2634–2760` | section field, `HabitEventRecord`, union, builders, GC; legacy shim retained read-side |
| 13 | `crates/taskboard-domain/src/idgen.rs:31–68` | `new_habit_event_id`; process-global `ContextV7` (#1488) |
| 14 | `crates/taskboard-domain/src/messages.rs:601–613` | command payloads/receipts gain event ids |
| 15 | `crates/taskboard-storage-sqlite/src/repo.rs:1426–1450, 1816, 1860, 2257–2319, 2889–2941` | load/flush/rewind on `habit_event`; migration marker (§6) |
| 16 | `crates/taskboard-storage-sqlite/migrations/0032_habit_events.sql` (new) | schema (§6.1) |
| 17 | `crates/taskboard-sync-nextcloud/src/flush.rs:28–29, 173, 439–500` | builders + GC swap |
| 18 | `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs:1663–1727` | `habit_events` lane replaces the `habit_log` lane |
| 19 | `crates/taskboard-ui-slint/src/presentation.rs:8796` | `habit_history_day_cells` derives from events |
| 20 | `crates/taskboard-ui-slint/src/models.rs:7576` | heatmap feed re-keyed |
| 21 | `crates/taskboard-app/src/commands/habits.rs:100–105, 310–332, 576–650, 998–1007, 1146–1190` | CLI log/adjust/skip/surface reads over derived state |

Test surfaces: `merge.rs` fold tests (:7276+), `persistence.rs` apply
tests (:1693+), `repo.rs` round-trips, UI journey tests, app CLI tests,
golden corpora — per-chunk acceptance in the dispatch plan.

---

## 8. Issue dispositions

| Issue | Disposition |
|---|---|
| #1519 | this design; closes on merge of the last chunk |
| #1340 (day-log device-local, no sync lane) | **dissolves** — the heatmap derives from synced events; cross-device day parity falls out |
| #1341 (week/month undo divergence) | **dissolves** — undo is per-event tombstone, attribution re-derives |
| #1363 (files habit-time lane) | **unaffected** — separate lane, separate section; stays p3 |
| #1488 (NoContext v7 minting) | **closed by chunk 1** — process-global `ContextV7` in `idgen.rs` (§1.2, mechanism verified) |
| #1489 (day-log unbounded growth) | **dissolves** — `day_log` is deleted; local event retention note: local events, like local rows today, are never GC'd — if local growth ever matters, a local prune is a new backlog item (deliberately *not* scoped here; the wire bound is Ruling E1) |
| #1520 (F-2/F-3 residue) | **closed by this refactor** — skip marks (F-2, Ruling B) and the HabitSurface fence (F-3, §3.4) dissolve; re-triage on merge per its own note |
| #1094 (G-Counter concurrent increments) | **dissolves** — union-by-uuid keeps concurrent increments; no per-author counter needed |
| #1318–#1319 family (habit enhancements) | schedule *after* this lands (the issue's own note) |

Out-of-scope findings from this round: filed as `backlog:` issues —
see the final report for numbers (local event retention is inside
#1489's re-triage comment rather than a new issue; the derivation-cost
memoization note rides it too).

---

## 9. What this design deliberately does *not* do (anti-examples honored)

- No change to the Task entity per-field clocks (`entities.rs`
  `Clocks`) — audit §6 anti-example.
- No `habit-day-log` wire section (the E3 option 2) — dead on arrival;
  the event log is the day-attribution carrier.
- No pause-span event family (F-7, #1524 watch item) — the scalar
  `paused_until` stays until a consumer asks.
- No touch on `sync.pending_ops` derivation-at-load posture
  (`persistence.rs:432–435`, anti-example).
- No F-4/F-5 work here (Ruling G2; F-5/#1522 has its own small plan).
