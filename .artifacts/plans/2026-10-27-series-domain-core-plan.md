# [Plan] Series Domain Core — DR-6 chunk 1

- **Date**: 2026-10-27
- **Kind**: stage-1 implementation plan (chunk DR-6/1 of the
  design-rounds dispatch). Normative inputs: the accepted design round
  `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  (§3.1–§3.3, §3.5, §3.9 properties 1–5, §5.4, §7 chunk 1) and the
  dispatch plan `2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§5.1 (issue **#498**). AGENTS §2 + `coding_guidelines.org` §2.1
  (the A1 type-driven ruling) apply to every type below.
- **Branch / worktree**: `feat/series-domain-core` in
  `../taskboard-WT-feat-series-domain-core`, branched off the fetched
  `origin/main` @ `548ecad` (worktree-doctor passed: 75G free, floor
  25G). Plan commit is the branch's first commit; implementation
  commits follow it, each standalone.
- **Status**: READY FOR STAGE 2 (implementation agent).

---

## 1. Scope

**Covers** (dispatch §5.1, origin §7 chunk 1):

- `RecurRule` / `RecurPattern` / `RecurBound` with A1 mints
  (`EveryDays`, `EveryWeeks`, `WeekdaySet`, `MonthlyAnchor`'s
  components) — hand-rolled private-field newtypes, fallible
  constructors rejecting zero/empty, no derive crates, documented
  leniency at the decode boundary.
- The total pure functions `next_after`, `occurs_on`, `schedule`,
  `roll_target`, `occurrence_outcome` with anchor-aligned semantics
  (monthly clamping included).
- The `Task.recur: Option<RecurRule>` field-family integration:
  `TASK_FIELDS` row, `TaskClocks` slot, `TaskChanges` double-Option,
  `plan_command` routing, the appdata codec arm — all in lockstep.
- `SeriesId` / `HabitId` id kinds + the `continued_from` seams.
- Proptest catalogue 1–5 + the serde round-trip property.
- The storage migration + repo column plumbing (deviation from
  dispatch §5.1 — forced by the schema lockstep; see §9).

**Not this chunk**: the `SetTaskDone` spawn arm and `day_tick`
(chunk 2, DR-6/5 wave), appdata emission/adoption wiring and fold
dedup (chunk 3, DR-6/3), CLI/surfaces (chunk 4), habits domain
(chunk 5), ADR 0018 (written by chunk 2 — first code that realizes
the rulings). No RRULE strings anywhere, ever (D-C3).

## 2. Verified anchors (on the branch base `548ecad`, 2026-10-27)

1. `Task` (`crates/taskboard-domain/src/entities.rs:402`) carries
   `duedate`/`scheduled`/`done: Option<DateTime<Utc>>` and per-field
   LWW `TaskClocks`. `effort`/`importance`/`urgency` are the
   appdata-lane field-family precedent (plain `Option<T>` fields, no
   serde defaults, mig `0007` added their columns).
2. `TASK_FIELDS` (`entities.rs:258`) is the mergeable-field manifest:
   `(&str name, &[&str] task fields, FieldFamily)`. Four lockstep
   assertions pin the workspace against it:
   - **A1** `pipeline.rs` (`task_fields_manifest_pins_the_domain_structures`,
     ~line 1475): `TaskClocks` serde keys == manifest clock set
     (structural, via a populated struct literal);
   - **A2** `taskboard-storage-sqlite/tests/schema.rs`
     (`tasks_schema_matches_the_task_fields_manifest`): the migrated
     `tasks` columns match exactly — `ck_*` set == manifest clock set
     (**exact equality**, not subset) and value columns ⊇ manifest
     fields;
   - **A3** `appdata.rs` (`modeled_keys_are_the_appdata_rows_plus_scheduled`,
     ~line 1381): `MODELED_TASK_FIELD_KEYS` == AppData rows ∪
     {`scheduled`}, with a `len() == 4` literal;
   - **A4** `taskboard-state/tests/manifest_routing.rs`
     (`every_manifest_row_routes_through_its_declared_lane`): every
     AppData row routes `UpdateAppData` via `changes_for(field)`; a
     row with no `TaskChanges` slot panics the table.
3. `TaskChanges` (`messages.rs:306`): double-`Option` sparse edits with
   `deserialize_double_option` + `skip_serializing_if`; the
   effort/importance/urgency arms are the pattern to copy.
4. `command.rs` routes appdata-lane edits through the `app_edit`
   predicate (~line 530) and stamps only the touched clocks in the
   `UpdateTask` arm; `CreateTask`-shaped constructors exist at
   ~lines 349/460 (exhaustive `Task` literals — compile fixes).
5. `days.rs::day_window(now, day_start_time, offset)` is the only
   local-date vocabulary: a user-configurable window start plus a
   device-fixed `FixedOffset`; pure, total. Its private
   `offset_local_to_utc` is the local→instant core; there is **no
   public** "window start of a date" helper yet (added in this chunk,
   §4.5).
6. `ids.rs` `local_id!` macro: private-field UUID newtype with
   `from_uuid`/`as_uuid`/`From<Uuid>`; `ReminderRuleId` is the roam-id
   precedent. `SeriesId`/`HabitId` join the family verbatim.
7. `appdata.rs` decode posture: modeled keys decode key-directed
   (`decode_field_write`/`decode_modeled`); a modeled key whose
   `{v, clock}` shape does not decode **demotes to verbatim unmodeled
   content**, never fails the document (lines 146–163, 273–284). The
   strict boundary is the envelope only.
8. Storage: migrations `0001`–`0009` on this base; `0007` is the
   additive-column mechanism to mirror (value columns NULL, clock
   columns `NOT NULL DEFAULT '0001-01-01T00:00:00+00:00'` epoch so
   pre-migration rows read "never locally written"; its comment also
   records the "reserved column lands ahead of codec wiring" precedent).
   Repo row struct + SELECT/INSERT name columns explicitly
   (`repo.rs:91–105`, `:584`, `:1047`); corrupt decoded values →
   `RepositoryError::Corrupted` (the 0007 corrupt-decode semantics).
9. `ReminderOffset` (`reminders.rs:33`) is the house A1 exemplar:
   private-field newtype, fallible `before` mint, typed
   `InvalidReminderOffset`, doc-commented invariant. Every mint below
   follows it.
10. `test_support::task_strategy()` constructs `Task` exhaustively —
    gains `recur: None` + the clock slot; shared proptest strategies
    live in `test_support` (feature-gated `test-support`).

## 3. Type inventory (A1 mints)

New module `crates/taskboard-domain/src/recur.rs`
(`pub mod recur;` + re-exports in `lib.rs`; SPDX `MIT OR Apache-2.0`;
crate-level `#![forbid(unsafe_code)]` already covers it). All types
`Debug, Clone, PartialEq, Eq, Serialize, Deserialize` (Task requires
`Eq`; no `Copy` on the set-carrying ones, no `Hash` — Task has none).

### 3.1 Ids (`ids.rs`, the `local_id!` family)

| Type | Doc | Notes |
|---|---|---|
| `SeriesId` | Local identity of a recurring series (`UUIDv7`, minted client-side; stable across the whole occurrence chain) | `local_id!` verbatim; engine mints it, domain wraps |
| `HabitId` | Local identity of a habit tracker (`UUIDv7`, generated client-side) | `local_id!` verbatim; consumed in chunk 1 only as `RecurRule.continued_from`'s target (entity lands in chunk 5) |

Both re-exported from `lib.rs`; the `ids.rs` roundtrip proptest gains
both accessors.

### 3.2 Mints (all in `recur.rs`)

| Type | Invariant (private field) | Mint → `Result<Self, InvalidRecur>` | Serde |
|---|---|---|---|
| `EveryDays(u16)` | inner ≠ 0 | `EveryDays::new(n: u16)` — `ZeroInterval` on 0 | transparent: ser = inner number, de = route through the mint |
| `EveryWeeks(u16)` | inner ≠ 0 | `EveryWeeks::new(n: u16)` — `ZeroInterval` on 0 | transparent, strict as above |
| `WeekdaySet(BTreeSet<chrono::Weekday>)` | non-empty, canonical (set order = `Weekday`'s Mon..Sun ord) | `WeekdaySet::new(iter: impl IntoIterator<Item = Weekday>)` — `EmptyWeekdaySet` on empty input; duplicates dedup silently (set semantics) | ser = sequence, de = route through the mint |
| `DayOfMonth(u8)` | inner ∈ 1..=31 | `DayOfMonth::new(d: u32)` — `DayOutOfRange { got }` outside 1..=31 | transparent, strict |
| `NthWeek(u8)` | inner ∈ 1..=5 | `NthWeek::new(n: u32)` — `WeekOutOfRange { got }` outside 1..=5 | transparent, strict |
| `OccurrenceCount(u32)` | inner ≠ 0 (a zero-count series is exhausted at birth — meaningless, rejected) | `OccurrenceCount::new(n: u32)` — `ZeroInterval` on 0 (reuses the "must be ≥ 1" variant; doc notes it covers counts) | transparent, strict |

Error type (thiserror, one per module — mirrors
`InvalidReminderOffset`'s variant style):

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidRecur {
    #[error("interval/count must be at least 1")]
    ZeroInterval,
    #[error("weekday set must not be empty")]
    EmptyWeekdaySet,
    #[error("day of month out of range 1..=31")]
    DayOutOfRange { got: u32 },
    #[error("nth weekday out of range 1..=5")]
    WeekOutOfRange { got: u32 },
}
```

Every mint carries a doc comment stating the invariant and the
**leniency ruling**: strict decode by design — a stored/wire value
that fails the mint surfaces as a decode error, and the *leniency*
lives one layer out (the appdata codec demotes the undecodable row to
verbatim, §6; the sqlite codec reports `Corrupted`). This is the
`DeckColor`/`ConnectorId` doc convention: leniency without the comment
is indistinguishable from a gap.

Accessors: `EveryDays::get()`, `EveryWeeks::get()`,
`DayOfMonth::get()`, `NthWeek::get()`, `OccurrenceCount::get()`
(`const fn`, u16/u8/u32), `WeekdaySet::{contains, iter, len,
is_empty→never true but total}` — only what the arithmetic and tests
need; CLI text forms are chunk 4 (deliberately not built here).

### 3.3 Grammar

```rust
pub struct RecurRule {
    pub series: SeriesId,              // stable across the chain
    pub anchor: NaiveDate,             // first scheduled local date; never mutated
    pub pattern: RecurPattern,
    pub bound: RecurBound,
    pub continued_from: Option<HabitId>,   // D-C12 seam
}

pub enum RecurPattern {
    Daily { every: EveryDays },
    Weekly { interval: EveryWeeks, on: WeekdaySet },
    Monthly { on: MonthlyAnchor },
}

pub enum MonthlyAnchor {
    OnDay(DayOfMonth),          // clamped, §4.3
    NthWeekday(NthWeek, chrono::Weekday),
}

pub enum RecurBound {
    None,
    Until(NaiveDate),
    Count(OccurrenceCount),
}
```

serde: default derives (externally tagged enums, ISO `NaiveDate`
strings) — the wire shape is pinned by the round-trip proptest, not by
renames. `MonthlyAnchor`/`RecurBound`/`RecurPattern` get no extra
mints (all invalid states are unrepresentable: components are mints).

## 4. Arithmetic contracts

All functions are pure, total, deterministic; dates in, dates out; no
clock reads (the crate's clock rule). All date arithmetic uses
`days.rs`'s fixed-offset convention; the functions themselves are
offset-free (offsets enter only in §4.5's attribution).

### 4.1 The membership law (anchor alignment — the phase rule)

The **schedule of a rule** is the set

> `members(rule) = { d : d ≥ rule.anchor ∧ aligned(rule.pattern, rule.anchor, d) }`

`aligned` (private) per pattern:

- `Daily { every: n }` — `d − anchor ≡ 0 (mod n·1 day)`, i.e.
  `(d.num_days_from_ce() − anchor.num_days_from_ce()) % n == 0`.
  Occurrences are `anchor + k·n`, `k ≥ 0`: a rolled-forward head never
  drifts the phase (origin §3.2).
- `Weekly { interval: i, on }` — `weekday(d) ∈ on` ∧ the Monday-start
  week index aligns: `(week_index(d) − week_index(anchor)) % i == 0`,
  where `week_index(d) = d.num_days_from_ce().div_euclid(7)`
  (proleptic-Gregorian CE day 1 is a Monday, so `div_euclid(7)` yields
  clean Mon..Sun weeks). Week-index arithmetic — **not** day-mod — is
  what makes `on ≠ {weekday(anchor)}` correct (a day-distance mod
  would misalign the whole set).
- `Monthly { on: OnDay(n) }` — `d.day == min(n, last_day_of_month(d))`:
  the clamp **is** the membership predicate (Feb 31 → Feb 28/29; the
  schedule is defined by the pattern, not by spawn history).
- `Monthly { on: NthWeekday(k, wd) }` — `d` is the k-th `wd` of its
  month. A month lacking a k-th `wd` (no 5th Friday) contributes **no
  member** — clamping is an `OnDay`-only ruling (property 3 pins
  exactly-one-member-per-month for `OnDay` only). Members remain
  infinite (some month within a bounded horizon always has 5 of any
  weekday), so forward scans terminate.

The `d ≥ anchor` gate encodes §3.1's "anchor = first scheduled local
date": the anchor may itself be a member (it is, whenever `aligned`),
and pre-anchor aligned dates are not members. `public occurs_on`
includes the gate.

### 4.2 Signatures (the origin's §3.2 sketches gain the anchor)

```rust
pub fn occurs_on(p: &RecurPattern, anchor: NaiveDate, d: NaiveDate) -> bool;
pub fn next_after(p: &RecurPattern, anchor: NaiveDate, from: NaiveDate) -> NaiveDate;
pub fn roll_target(p: &RecurPattern, anchor: NaiveDate,
                   stale: NaiveDate, today: NaiveDate) -> NaiveDate;
pub fn schedule(rule: &RecurRule, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate>;
```

The anchor is an explicit parameter everywhere (the origin sketch
omitted it; its own proptest triples are `(pattern, anchor, from)` and
`roll_target` already carries it — a pattern-only `next_after` cannot
express phase). Precondition `anchor ≤ from` for meaningful use
(production always satisfies it: the head sits at/after the anchor);
totality holds regardless (`from < anchor` returns the least member ≥
anchor, which is > `from`).

- `next_after` = the least member strictly after `from` (forward scan
  via `aligned`, capped by stepping — members are infinite for every
  pattern, so the loop terminates).
- `occurs_on` = membership (§4.1).
- `schedule` = sorted members in `[from, to]` (empty when `from > to`).
  **Bound handling**: `Until(u)` filters (`d ≤ u` only) — schedule is
  the arithmetic truth of the series; `Count(n)` does **not** filter
  (D-C5: the count is evaluated against completion history at the
  engine, §3.7 of the origin — no counter column, so the pure function
  cannot see it; documented at the function).

### 4.3 `roll_target` — the two-case law

`roll_target(p, anchor, stale, today)` (precondition `stale < today`;
for `stale ≥ today` it degrades total-and-harmless to case 2):

1. **Catch-up case**: some member lies in `(stale, today]` → the
   **latest** such member. (`Daily { every: 1 }` ⇒ always `today`.)
2. **Skip-ahead case**: no member in `(stale, today]` → the first
   member `> stale` (weekly, today=Wednesday → next Monday).

Pure in `(p, anchor, stale, today)` — identical inputs give identical
targets on every device; that is D-C7's "rolls need no dedup". The
bound (`Until`) is checked by the caller (engine, chunk 2); chunk 1's
function is bound-free arithmetic.

### 4.4 `occurrence_outcome` — attribution (origin §3.5, property 5)

```rust
pub enum OccurrenceOutcome {
    Done { at: DateTime<Utc> },   // the earliest attributing completion
    Missed,                        // window closed, nothing attributed
    NotYetReachable,               // window still open (unopened or in progress)
}

pub fn occurrence_outcome(
    rule: &RecurRule,
    day: NaiveDate,
    now: DateTime<Utc>,
    day_start_time: chrono::NaiveTime,
    offset: chrono::FixedOffset,
    completions: &[DateTime<Utc>],   // done stamps of the series' tasks
) -> OccurrenceOutcome;
```

Laws:

- **Window-start conversion**: `ws(d) = window_start(d, day_start_time,
  offset)` — the instant whose offset-local reading is `d` at
  `day_start_time` (a night-owl `05:00` start puts 03:00 local into
  *yesterday's* window).
- **Attribution**: a completion stamp `t` attributes to the **latest
  member `m` with `ws(m) ≤ t`** (partition: exactly one such member
  for `t ≥ ws(first member)`; stamps before the series existed
  attribute to nothing — production never produces them, and the
  proptest generates stamps from `ws(anchor)` on). Late completions
  therefore attribute to the **earlier** member (the origin's rule):
  completing Tuesday's rolled head on Wednesday counts for Tuesday.
  Window = `[ws(day), ws(next member))`; for the **final** member
  (no next member) the window is open-ended.
- **Outcome**: `Done{at}` iff some completion attributes to `day`
  (`at` = earliest such stamp); else `Missed` iff `now ≥ ws(next
  member)` (the attribution window closed); else
  `NotYetReachable`. For the final member the window never closes ⇒
  never `Missed` (recorded decision D8 — no knob, no invented
  deadline; the open final head simply sits on the board like any open
  task).

### 4.5 `days.rs` addition

```rust
pub fn window_start(date: NaiveDate, day_start_time: NaiveTime,
                    offset: FixedOffset) -> DateTime<Utc>;
```

Pure, total; `day_window` refactors over it (its `start_date.and_time(day_start_time)`
conversion is exactly this). Pins: `window_start(d + 1) == window_start(d) + 24h`
(fixed offset); `day_window(t, …).0 == window_start(t's local date, …)`
for `t = window_start(d)`. This stays the **only** local-date
vocabulary — no second timezone concept (origin fact 7).

### 4.6 The UTC-conversion boundary

`scheduled`/`duedate` stay `Option<DateTime<Utc>>`. Chunk 1 converts
local→UTC **nowhere** (no engine calls exist yet); it only guarantees
the vocabulary exists (`window_start`) for chunk 2's spawn/roll to
convert `next_after`/`roll_target` results through. Recurrence
arithmetic never touches `DateTime` — dates in, dates out; the task
field seam is the only UTC boundary (origin §3.2). Nothing in this
chunk reads the clock: `now`/`today`/`stale` are always parameters.

## 5. Field-family integration checklist (the lockstep cascade)

The `recur` row lands atomically — every item below is one commit
(§7, commit 4); the four existing lockstep assertions are the failing-
then-green test harness:

- [ ] `entities.rs`: `TASK_FIELDS` gains `("recur", &["recur"], FieldFamily::AppData)` (after `urgency`); `Task.recur: Option<RecurRule>` (plain field, the effort/importance/urgency precedent — no serde default); `TaskClocks.recur: DateTime<Utc>` (after `urgency`, before `archived` — order matches the manifest).
- [ ] `messages.rs`: `TaskChanges.recur: Option<Option<RecurRule>>` with the `deserialize_double_option` + `skip_serializing_if` attribute block, verbatim the `urgency` arm's shape.
- [ ] `command.rs`: `UpdateTask` arm gains `if let Some(recur) = changes.recur { updated.recur = recur; updated.clocks.recur = now; }`; the `app_edit` predicate gains `|| changes.recur.is_some()` (A4 routes the row to `UpdateAppData`); the exhaustive `Task` literals (~349/460) gain `recur: None` + the clock.
- [ ] `appdata.rs`: `MODELED_TASK_FIELD_KEYS` gains `"recur"`; `ModeledValue::Recur(Option<RecurRule>)` arm + `modeled_key` + `encode_modeled` + `decode_modeled` arms (decode: `null` = cleared, serde shape = set, else demote — the string-effort precedent verbatim); assertion literal `len() == 4` → `5`. Exhaustive `ModeledValue` consumers (`merge.rs`, `pipeline.rs`, wiremock test) get compile-driven arms.
- [ ] `taskboard-state/tests/manifest_routing.rs`: `changes_for("recur")` gains the `TaskChanges { recur: Some(Some(..)), ..Default::default() }` arm.
- [ ] `ids.rs`: `SeriesId`/`HabitId` (§3.1) + roundtrip proptest extension + `lib.rs` re-exports.
- [ ] **Migration 0010** (deviation, §9), mirroring `0007`'s mechanism and comment style:
  `ALTER TABLE tasks ADD COLUMN recur TEXT NULL;` (serde JSON of `Option<RecurRule>`)
  `ALTER TABLE tasks ADD COLUMN ck_recur TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';`
- [ ] `repo.rs`: row struct + SELECT + INSERT + codec `recur_to_text`/`recur_from_text` (JSON ↔ `Option<RecurRule>`; corrupt JSON → `RepositoryError::Corrupted`, the 0007 semantics), mirroring the urgency column end to end. Refresh the `.sqlx/` offline cache (query set changed).
- [ ] `schema.rs`: no assertion edits expected — the test derives from `TASK_FIELDS`; the "insert without family columns inherits epoch defaults" test gains the recur/ck_recur expectation; the corrupt-decode test gains a recur case.
- [ ] Compile-fixture sweep (mechanical, one line each): `TaskClocks { .. }` literals in `state.rs`, `reconcile.rs`, `find.rs`, `outbox.rs`, `persistence.rs`, `merge.rs`/`merge_testutil`, `days.rs` tests, `push.rs`, `reminders.rs` tests, `test_support/*`, `taskboard-state` tests, `taskboard-ui-slint` (`models.rs`, `presentation.rs`, fixtures), `taskboard-app` (`commands/find.rs`, `completion.rs`, `web/render.rs`, `cli_verbs.rs`), `taskboard-sync-nextcloud` tests/benches, `taskboard-storage-sqlite` tests/benches — each gains `recur: <epoch/now>` + `recur: None`.
- [ ] `test_support`: `task_strategy()` gains `recur: None` + clock slot; new `recur_rule_strategy()` (§7) exported under the existing feature gate.
- [ ] Insta snapshot regeneration: the `taskboard-state` snapshot suite (AppState shape widens) — **one deliberate pass**, stage-2 prompt's standing order.

## 6. Decode-leniency behavior for invalid stored rows

Three untrusted decode boundaries, three postures — all typed, none
poisoning state:

1. **Appdata tasks-row (recur's sync lane)**: the existing
   `decode_modeled` mechanism demotes an undecodable `recur` value
   (`{v: {"every": 0, …}}`, a string, a truncated object) to verbatim
   unmodeled content next to its typed siblings — the document never
   fails, the row rides back out on re-serialization (D-S2's
   materialization rule). Chunk 1 pins this with unit tests on the
   codec arm (the emission/adoption wiring itself is chunk 3).
2. **Sqlite `recur` column (local cache)**: corrupt JSON →
   `RepositoryError::Corrupted` (typed, loud) — the 0007
   corrupt-decode semantics; a NULL column decodes `None`.
3. **The mints' serde impls (the trust boundary itself)**: strict —
   `serde_json::from_value::<RecurRule>` on garbage is `Err`, never a
   panic, never a silently-wrong rule (e.g. `every: 0` cannot
   deserialize). Proptest: for arbitrary JSON values, the decode is
   `Ok(valid) ∨ Err`; every `Ok` re-encodes to a value that decodes
   equal (property 8's chunk-1 half; the appdata-row half is chunk 3).

## 7. TDD plan & proptest catalogue

Build order (each step: failing test first, then implementation;
`cargo nextest run -p taskboard-domain` after every step):

1. Mints: accept/reject unit tables (zero/empty/range edges, boundary
   values 1, 31, 5), serde strictness table (`every: 0` → `Err`), the
   `ConnectorId`-style leniency pin (a strict-decode doc statement per
   type).
2. Arithmetic: proptests P1–P4 below + unit tables — clamping
   (`OnDay(31)` across Feb/Mar/Apr, leap Feb 29), the origin's own
   `roll_target` examples (weekly, today=Wednesday → next Monday;
   `Daily{1}` → today), NthWeekday month-skip.
3. Attribution: `window_start` pins, then proptest P5 + unit table
   (late completion attributes to the earlier member; night-owl window
   reattributes 03:00-local to yesterday; final member never Missed).
4. Lockstep cascade (§5) — the four existing assertions are the
   red/green harness.
5. Storage (migration + codec + schema/roundtrip tests: persist a
   `Some(rule)` task → reload equal; corrupt column → `Corrupted`;
   pre-migration-shaped insert inherits epoch clocks).
6. Snapshot regeneration pass, then the full §10 gate suite.

**Proptest catalogue** (in `recur.rs`, 256 cases, house
`proptest_config`; strategies: `recur_pattern_strategy()`,
`naive_date_strategy()` (bounded 2000–2100 to dodge chrono extremes),
`recur_rule_strategy()` in `test_support` for reuse by chunks 2/3/5):

| # | Origin §3.9 | Property |
|---|---|---|
| P1 | 1 | ∀ (p, anchor, from), anchor ≤ from: `next_after(p, anchor, from) > from` ∧ `occurs_on(p, anchor, next_after(..))` (total, strictly-after, member). |
| P2 | 2 | Anchor alignment/chain: seeding at the least member ≥ `from`, repeated `next_after` enumerates exactly `schedule(rule, from, to)` (Until-bounded rule, members within `to`); and `next_after(m)` from any member `m` yields the next member — roll/spawn/completion paths agree on one schedule. |
| P3 | 3 | Clamping: for `Monthly{on: OnDay(n)}` with bound `None`, `schedule` yields exactly one member per month in range, equal to `min(n, last_day(month))`. |
| P4 | 4 | `roll_target` laws: result > stale; result ≤ today ⇒ `occurs_on(result)` ∧ no member in `(result, today]` (it is the latest); result > today ⇒ result == `next_after(stale)` ∧ no member in `(stale, today]`; `Daily{1}` ⇒ result == today; **convergence**: stepping day-by-day (`roll_target(p, a, stale, today−1)` then to `today`) lands on the single-roll target (offline gaps change nothing but `today`); determinism (equal inputs, equal outputs). |
| P5 | 5 | Attribution: for generated (rule, now, window params, completion sets): every generated stamp attributes to exactly one member (partition over `occurrence_outcome` of all members — Done-days' stamps are disjoint and cover the set); late completions attribute to the earlier member (constructed pin); `Done{at}` ⇒ `ws(day) ≤ at < ws(next)`. |
| RT | 8 (half) | Round-trip: `RecurRule → serde_json → RecurRule` lossless over generated rules; arbitrary JSON decodes `Ok(valid) ∨ Err`; `Ok` ⇒ re-encode/decode equal (§6.3). |

No `tokio::time::sleep`, no wall-clock reads, no text assertions
(typed `matches!(err, InvalidRecur::…)` outcomes only), no ceilings.

## 8. Decision record

| ID | Question | Ruling |
|---|---|---|
| D1 | Module home | New `recur.rs` in `taskboard-domain` (house naming: `reminders.rs`, `days.rs`); ids in `ids.rs`. |
| D2 | Arithmetic signatures carry the anchor | `next_after`/`occurs_on` take `(pattern, anchor, …)` — the origin §3.2 sketch omitted it, but its own proptest triples and `roll_target` include it; pattern-only signatures cannot express phase. |
| D3 | Membership includes `d ≥ anchor` | The anchor is the first scheduled local date (§3.1): pre-anchor aligned dates are not members; the anchor itself is a member iff aligned. Uniform law across patterns. |
| D4 | Weekly alignment | Monday-start week index `num_days_from_ce().div_euclid(7)`, aligned iff index distance ≡ 0 (mod interval) — week-index, not day-mod, so `on ≠ {weekday(anchor)}` stays correct. |
| D5 | `NthWeekday(5, wd)` in short months | No member that month (no clamping — clamp is an `OnDay` ruling, property 3 pins exactly-one-per-month for `OnDay` only); members stay infinite so scans terminate. |
| D6 | `Count` and `schedule` | `schedule` filters `Until` but not `Count` — the count is history-evaluated at the engine (origin §3.7); the pure function is history-free. Documented at the function. |
| D7 | `roll_target` is pattern-level | Bound checks (`Until` suppression) are the caller's (chunk 2 engine); chunk 1 keeps the arithmetic total and bound-free. |
| D8 | Final member's attribution window | Open-ended ⇒ the final member is never `Missed` (no invented deadline, no knob — D-C8's anti-configurability); `NotYetReachable` names "window still open", covering unopened and in-progress. |
| D9 | Migration rides chunk 1 | Dispatch §5.1 said "storage migration is NOT this chunk", but storage lockstep assertion A2 (`ck_columns == manifest_clocks`, exact equality) makes "gates green" impossible without the columns. Chunk 1 adds migration **0010** (next free number on this base) + minimal repo plumbing (§5). See §9. |
| D10 | Repo codec in chunk 1 | The columns land with read/write plumbing (urgency-column mirror, ~80 lines) rather than NULL-only: a silently-dropped written field behind a live manifest row is exactly what the lockstep exists to prevent, and chunk 2's spawn needs the persistence to exist. `.sqlx/` cache refreshed in the same commit. |
| D11 | Serde posture | Default serde derives (externally tagged enums, ISO dates), strict decode through the mints; wire shape pinned by the RT proptest; no renames until a wire consumer exists. |
| D12 | No CLI text forms in chunk 1 | `parse_text_form`/`text_form` for the mints are chunk 4's surface concern (Effort precedent) — not built here (§3.8 discipline). |
| D13 | Shared strategies | `recur_rule_strategy()`/`recur_pattern_strategy()` go in `test_support` (feature-gated) — chunks 2/3/5 reuse them for spawn/roll/dedup tests. |

## 9. Deviations from the dispatch plan (for the stage-3 reviewer)

- **Migration + repo plumbing in chunk 1** (D9/D10) versus dispatch
  §5.1's "Storage migration is NOT this chunk" and DR-6/3's "mig 0014
  recur columns". Cause: storage lockstep assertion A2 is exact
  equality — the `TASK_FIELDS` row this chunk is mandated to land
  breaks `schema.rs` until the columns exist; there is no weakening
  path that isn't "weaken a correct test" (house-forbidden).
  Consequences: (a) the migration takes **0010**, the next free number
  on this base — DR-4/G (wave 3, "mig 0010") and successors renumber
  to the then-next free number at their own branch time (the dispatch
  table's numbers are planning aids; "next free at merge time" is the
  binding invariant); (b) DR-6/3's storage scope collapses to the
  appdata emission/adoption wiring + fold dedup (its columns already
  exist).
- **`occurrence_outcome` gains explicit `now`/`day_start_time`/`offset`
  parameters** versus the origin's three-argument sketch — the
  three-way `Outcome` is undefinable without a reading time, and
  attribution is instant-based (windows), not date-based. Recorded at
  D8/§4.4; origin §3.9 property 5 is unaffected.

## 10. Risks

- **Wide mechanical tail**: `TaskClocks` is constructed exhaustively
  in ~35 files across all six crates — the lockstep is doing its job,
  but the diff is wide though shallow (one line each). Mitigation:
  commit 4 is atomic; compiler-driven; no behavior change behind the
  mechanical lines.
- **Snapshot churn**: the (w) shape-widener regenerates the shared
  insta suite — only wave-2 chunk, no concurrent widener (DR-2/I is
  not one). One deliberate `INSTA_FORCE_PASS`-style pass per the
  stage-2 prompt, reviewed by eye in the diff.
- **Migration-number race**: if a wave-1 storage-bearing PR lands
  `0010` before this PR opens, the migration renames (additive file,
  rebase-safe; sqlx migrations are filename-ordered). Stage 3 checks
  at PR time.
- **chrono date-range extremes**: the proptest date strategy bounds
  2000–2100 (the arithmetic uses `num_days_from_ce` differences and
  `Months` stepping — both saturate only at `NaiveDate`'s extremes,
  same posture as `day_window`'s totality note).
- **`ModeledValue` exhaustiveness**: adding the arm breaks exhaustive
  matches in `merge.rs`/`pipeline.rs`/sync tests — compile-caught;
  each new arm adopts the sibling arms' shape (no logic risk).

## 11. Commit plan & estimated diff

| # | Subject | Content |
|---|---|---|
| 0 | `docs(plan): recurring series domain core` | this plan (landed first) |
| 1 | `feat(domain): the recur grammar mints` | `recur.rs` types + `InvalidRecur` + ids + serde strictness + unit tables |
| 2 | `feat(domain): the recurrence arithmetic` | `next_after`/`occurs_on`/`schedule`/`roll_target` + proptests P1–P4 + unit tables |
| 3 | `feat(domain): occurrence attribution` | `occurrence_outcome` + `days.rs::window_start` + proptest P5 |
| 4 | `feat(domain): the recur field family in lockstep` | §5 checklist in full (row/clock/changes/routing/codec arm/migration/repo/fixtures/snapshots) |
| 5 | `test(domain): the recur round-trip pins` | RT proptest + storage roundtrip/corrupt tests (if not already in 1/4) |

Estimated size: `recur.rs` ≈ 700–800 lines incl. tests/proptests;
lockstep cascade ≈ 150 lines mechanical across ~35 files; storage ≈
150 lines; appdata codec arm ≈ 60; total ≈ **1100–1400 insertions**
across ~45 files (core new logic ≈ 400 lines; the rest is lockstep
mechanics and tests).

## 12. References

- Origin (normative): `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md` §3.1–§3.3, §3.5, §3.9, §5.4, §7.
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md` §1, §5.1.
- A1 policy: AGENTS §2; `docs/coding_guidelines.org` §2.1.
- Exemplars: `reminders.rs` (`ReminderOffset`), `entities.rs`
  (`Effort` + `TASK_FIELDS`), `ids.rs` (`ConnectorId` leniency doc),
  `appdata.rs` (demotion), migration `0007` (column mechanism).
