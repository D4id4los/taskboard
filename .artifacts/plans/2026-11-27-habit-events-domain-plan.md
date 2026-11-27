# The Habit Events Domain Chunk Plan — Chunk 1 of the Habit Event-Log Dispatch

- **Date**: 2026-11-27
- **Kind**: stage-1 (planning) artefact for chunk 1 ("domain event model +
  derivations", branch `feat/domain-habit-events`) of the #1519 habit
  event-log dispatch.
- **Origins (normative)**:
  `.artifacts/plans/2026-11-27-habit-event-log-design.md` §0 (Rulings A,
  B, I — all accepted on recommended defaults 2026-11-27; Ruling I is
  chunk-3-scoped), §1 (event model, §1.2 ContextV7/#1488), §2
  (derivations); the dispatch plan §1/§2 (chunk table, shared execution
  model, Prompts 1.2/1.3); the UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md` (W-7, W-11);
  `docs/coding_guidelines.org` §2.1 (A1 type-driven policy) and §2.2
  (clock discipline); `docs/testing_strategy.org` §6 (clock discipline,
  now-as-data).
- **Worktree**: `/home/viktoria/src/taskboard-WT-feat-domain-habit-events`
  on branch `feat/domain-habit-events` at `origin/main` `1c9a7487`
  (worktree-doctor passed; created by the dispatcher per AGENTS §8).
- **Scope**: additive only. New module `habit_event.rs`; idgen gains
  `new_habit_event_id` + the process-global v7 context (#1488);
  `HabitState` gains an additive `events` map; the three derivation
  functions with their property/oracle tests. **Non-goals** (dispatch
  plan §2): no command/persistence changes; `HabitState.log` /
  `day_log` and every existing reader keep passing untouched; no
  sqlite, wire, or UI work; no uuid `v5` feature (chunk 3);
  `mint_log_clock` stays (deleted chunk 5).

---

## 1. Recorded decisions

Every open question is settled here. Deviations from the origin are
marked **[deviation]** with a reason.

### D1 — `HabitEventId`: id-family grammar, minted at the seam

Hand-rolled private-field newtype in `habit_event.rs`, mirroring the
`local_id!` family output (`ids.rs`) and the `AuthorId` doc discipline:

```rust
pub struct HabitEventId(Uuid);
```

- `From<Uuid>` **pub(crate)** — the constructor law: production mints
  only through `IdGenerator::new_habit_event_id`; `From<Uuid>` exists
  for the wire/storage decode and chunk 3's deterministic v5 synthesis,
  which are the trust boundaries. **Documented leniency at the type**:
  `From<Uuid>` accepts any uuid version — chunk 3's synthetic v5 ids
  are legitimate non-v7 members, so a version check here would be
  validate-and-forget against a future fact family.
- `parse(&str) -> Option<Self>` strict (the `AuthorId::parse` shape)
  for text boundary paths; `as_uuid()`, `Display`, serde via uuid
  string (strict on decode — an unparsable id is corruption, not data,
  same as `HabitLogKey::parse_wire_form`).
- `Ord`/`Hash` derive: the derivations' tie-break law is `(at, id)`
  lexicographic, so the id must be orderable (uuid byte order —
  deterministic, version-agnostic).

### D2 — `HabitEventKind` and `HabitEvent` shapes

Exactly design §1.1, serde spelled out:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HabitEventKind {
    /// One completed quota unit. Carries no window: the window is
    /// derived at read time (§2 of the design — the whole point).
    Completion,
    /// A deliberate planned-skip mark for one derived window (Ruling B).
    Skip { window: WindowStart },
    /// Withdrawal of the window's skip mark (Ruling B).
    Unskip { window: WindowStart },
}
```

`WindowStart` already has strict serde (`habit.rs:419` decode-as-is
leniency — kept verbatim; a mark's stored window may be non-canonical
after a kind change and must not be rewritten by decode).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HabitEventTombstone {
    pub at: DateTime<Utc>,
    /// Documented leniency: `None` means "author unknown" (the F1
    /// `None < Some` posture, `habit.rs:600–603` verbatim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HabitEvent {
    pub id: HabitEventId,
    pub habit: HabitId,
    pub kind: HabitEventKind,
    /// The completion / mark instant — the event's own fact. No clock
    /// inflation: `mint_log_clock` (#986) is LWW machinery and dies in
    /// chunk 5; the union fold arbitrates nothing. Documented leniency:
    /// a tombstone `at` later than `at` itself is legitimate —
    /// corrections happen after the fact.
    pub at: DateTime<Utc>,
    /// Display-only author (never for arbitration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
    /// Deletion tombstone (Ruling C2): default-absent; the newest
    /// `(at, by)` tombstone wins in the fold (chunk 4). Absent on every
    /// event this chunk mints or loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<HabitEventTombstone>,
}
```

All fields `Copy`-able, so the event itself is `Copy` — small, pure,
fits the domain discipline. Leniency is documented at each `Option`
per the A1 policy (guidelines §2.1: "leniency without that comment is
indistinguishable from a gap"). No derive crates. No
validate-and-forget: the derivation functions re-check liveness
(tombstones) rather than trusting a pre-filtered input.

### D3 — `HabitState.events` (additive; the old maps stay)

```rust
/// The per-completion event log (design §1.3, #1519): the future sole
/// source of truth. Additive this chunk — `log` / `day_log` stay and
/// every existing reader is untouched (chunk 2 rewires writers, chunk
/// 5 deletes the maps). `BTreeMap` keyed by mint-instant-ordered v7
/// ids: deterministic serialization, and the (at, id) iteration order
/// the derivations' tie-breaks rely on. `serde(default)`: the additive
/// default-absent law, never a decode failure.
#[serde(default)]
pub events: BTreeMap<HabitEventId, HabitEvent>,
```

`Default`/`PartialEq` derives are unaffected.

### D4 — IdGenerator gains `new_habit_event_id`; the trait surface is otherwise untouched

The `new_habit_time_entry_id` twin precedent (`idgen.rs:42–44`).
Mechanical consequence: `CountingIds` (`test_support/mod.rs:100`) must
add the arm too — `HabitEventId::from(Uuid::from_u128(self.next()))`,
the deterministic counter, so every downstream crate's injected
generator keeps compiling. The trait's existing methods do not change
(review focus (b) in Prompt 1.3). `merge_testutil::CountingIds` is a
re-export — no change needed there.

### D5 — #1488 closes with a `OnceLock<Mutex<ContextV7>>` process global
**[deviation from the design's `OnceLock<ContextV7>` spelling — reason
recorded]**

The design (§1.2) says "holds (or references, via a
`OnceLock<ContextV7>` process-global in `idgen.rs`)". Verified against
the resolved uuid 1.29.0 source (`timestamp.rs:805`): `ContextV7`
stores `Cell<ReseedingTimestamp>` / `Cell<Counter>` — it is `!Sync`,
so a bare `static OnceLock<ContextV7>` does not compile. The crate is
`#![forbid(unsafe_code)]`, so an `UnsafeCell` wrapper is out. Decision:

```rust
/// The process-global v7 minting context (#1488). `ContextV7` is
/// `!Sync` (Cell-based counter), so the global carries it behind a
/// `Mutex`; minting is rare (user actions), the lock is uncontended in
/// practice, and the counter's monotonicity guarantee is what #1488
/// needs — it survives only if all mints share one context.
static V7_CONTEXT: OnceLock<Mutex<ContextV7>> = OnceLock::new();
```

`UuidV7Generator` stays a `Copy, Default` unit struct; every mint arm
becomes:

```rust
fn new_habit_event_id(&self) -> HabitEventId {
    HabitEventId::from(v7_now())
}

// Shared by every arm:
fn v7_now() -> Uuid {
    let ctx = V7_CONTEXT.get_or_init(|| Mutex::new(ContextV7::new()));
    let mut guard = ctx.lock().unwrap_or_else(PoisonError::into_inner); // poison-recovery, not a panic path
    Uuid::new_v7(Timestamp::now(&mut guard))
}
```

(`Timestamp::now` takes `&C`; taking the guard by `&mut` matches the
`ClockSequence` receiver the builder uses — verify the exact
deref form at implementation time against the uuid 1.29 signature;
either `&C` or `&mut C` compiles, pick what the resolved source shows.)

**Behavior-identity note for #1488** (must appear verbatim in the
commit body and in the `idgen.rs` doc comment): today `Uuid::now_v7()`
builds `Timestamp::now(NoContext)` — same-millisecond ids order by
random low bits, so rare `(at, id)` tie inversions are possible
(#1488's finding). With the shared context, same-millisecond mints
increment a 42-bit counter and are **locally totally ordered** (the
only guarantee uuid makes — monotonicity per process). Tie *outcomes*
corpus-wide change: every persisted `(clock, id)` duel rank minted
after this change is counter-ordered. This is the behavior-identity
note #1488's own wording requires; the fold and GC that depend on
`(at, id)` determinism (chunks 4) inherit it. Existing wire rows keep
their stored ids — nothing re-mints; `CountingIds` (the test seam) is
untouched.

Miri gate: the new ordering test reads the realtime clock — same
`#[cfg_attr(miri, ignore)]` env gate as the existing
`v7_generator_returns_distinct_correctly_typed_ids`
(`idgen.rs:118`).

### D6 — `derive_window_counts` fold contract

Design §2.1 verbatim as the contract, spelled out for implementation:

- **Input**: `habit: &Habit`, `events: impl Iterator<Item = &HabitEvent>`,
  `day: DayCalendar`. Now-as-data (guidelines §2.2 / testing §6.1
  tool 1): the completion instants are the events' own `at` — the
  function takes no `now` at all.
- **Liveness is the function's job, not the caller's**: skip events
  where `event.habit != habit.id`, `event.deleted.is_some()`, or
  `!matches!(event.kind, HabitEventKind::Completion)`. No
  validate-and-forget: the fold re-checks membership like
  `habit_log_for` does today (`appdata.rs:2707–2716` posture).
- **Order**: collect the live completions into a `Vec`, sort by
  `(at, id)` — lexicographic, the D1 `Ord` derive. The input iterator
  order (e.g. `BTreeMap` iteration) is irrelevant to the result.
- **Fold**: `counts: BTreeMap<WindowStart, u32>` running map; per
  event, `window = attribute(habit, &counts, event.at, day)` where
  `attribute` is the `log_window` rule (`habit.rs:968–994`) with the
  `count` closure reading `counts` instead of `HabitState.log`; then
  `*counts.entry(window).or_default() += 1`.
- **The recursion resolves exactly as the original did** (design §2.1):
  `log_window` at log time read the *stored* counts; the fold
  reproduces that order online. The attribution oracle is
  `log_window` itself, preserved verbatim in-tree this chunk (it dies
  in chunk 5, not here).
- **Return**: `BTreeMap<WindowStart, u32>` (only windows with count
  ≥ 1 — count-0 rows do not exist in the event grammar; the D24 fence
  death is chunk 2's `AdjustHabitLog` reconcile).
- **Cost**: O(n log n) over the habit's live events; kiosk-scale fine
  (design §2.1; the memoization note rides #1489's re-triage, not
  designed here).
- **Mechanical refactor of `log_window`** [decision]: extract the
  attribution core into a private `fn attribute_window(
  kind: PeriodKind, quota: NonZeroU32, counts: impl Fn(WindowStart)
  -> u32, at: DateTime<Utc>, day: DayCalendar) -> WindowStart` and make
  `log_window` a thin wrapper over it reading `HabitState.log`. The
  fold calls the same core — one attribution rule, two count sources.
  This is a pure extraction; `log_window`'s public signature and
  behavior are unchanged (no existing test moves).

### D7 — `derive_skip_posture` — the (at, id) max law, in the shared fold

- **Signature**: `pub fn derive_skip_posture(habit: &Habit, events:
  impl Iterator<Item = &HabitEvent>, day: DayCalendar) ->
  BTreeMap<WindowStart, bool>` — `true` = the window is skipped,
  mirroring today's `HabitLogEntry::skip: bool` read shape so chunk
  5's re-keying of the streak walk's `skipped` closure
  (`habit.rs:1064–1070`) is mechanical. Absent key = unskipped.
- **The max law** (Ruling B): per window, compare the newest
  `Skip { .. }` and the newest `Unskip { .. }` under `(at, id)`
  lexicographic order; the greater kind wins. `None < Some` does not
  arise — a window with only skips is skipped, only unskips is
  unskipped, both is the max.
- **Mark windows re-derive; the stored `window` field is provenance
  only on the read path** (design §2.1's parenthetical, made
  binding): each mark attributes to the window the fold would pick for
  its `at` under the *current* config. Implementation: one **shared
  fold** over completions and marks in `(at, id)` order — completions
  increment `counts`; marks call `attribute_window` with the same
  running counts (they participate in attribution, never in counts)
  and record `(at, id, is_skip)` into a per-window `BTreeMap
  <(DateTime<Utc>, HabitEventId), bool>`; the posture is the max key
  per window. The stored `window: WindowStart` on `Skip`/`Unskip` is
  the mint-time fact (sync provenance, chunk 4's wire shape) — the
  derivation never reads it. **Documented leniency at the type**: a
  mark's stored window may be a former-kind window after a kind
  change; that is legitimate provenance, not corruption (the D-H12
  posture's successor shape).
- **Why marks share the completion fold**: the surplus-repair guard
  ("natural already met quota") is count-dependent, so a mark's
  attribution is only well-defined relative to the completions logged
  before it — the same order the events actually happened in. A
  mark-only fold would answer a different question.

### D8 — `derive_day_counts` — plain calendar-day attribution

- **Signature**: `pub fn derive_day_counts(habit: &Habit, events:
  impl Iterator<Item = &HabitEvent>, day: DayCalendar) ->
  BTreeMap<NaiveDate, u32>`.
- **Law** (design §2.2, heatmap row): every live completion attributes
  to `day.today(event.at)` — the calendar day of the completion
  instant under the device's day calendar. **No grace, no
  surplus-repair**: the day overlay is display data (R-2), and the
  design's day rule is literally `day.today(event.at)`. Order does not
  affect the result (a pure per-event map) — recorded so the
  equivalence test does not invent an order law that does not exist.
- **Oracle equivalence note**: for the 1:1 legacy-correspondence event
  sets of §3, the legacy `day_log` rows were written with
  `count` at `day.today(log-now)`; since correspondence events carry
  `at == log-now`, the derived day map equals the legacy overlay on
  those sets. That is the equivalence assertion; it must *not* be
  claimed for grace-repaired window attributions (the overlay and the
  window log legitimately disagree there — R-2's division of labor).

### D9 — Serde/codec posture this chunk

Domain serde only (strict decode, additive defaults). No wire section,
no `HabitEventRecord` (chunk 4), no sqlite (chunk 3). The events map
round-trips inside `HabitState` wherever `AppState` already does
(insta snapshots that embed `HabitState` will gain an `"events": {}`
key on regeneration — expected, cited churn; see §4 T7).

### D10 — Module and lib wiring

`pub mod habit_event;` after `pub mod habit;` in
`crates/taskboard-domain/src/lib.rs:49–50`. Zero new dependencies (the
domain-crate discipline, AGENTS §2): everything used — `uuid`,
`chrono`, serde, `proptest` (dev) — is already in the crate's graph.
SPDX `MIT OR Apache-2.0` header on the new file;
`#![forbid(unsafe_code)]` already crate-wide.

### D11 — Test placement per the UX journey plan (naming is normative)

The UX plan's W-7 and W-11 rows name the test paths:

- `crates/taskboard-domain/src/habit_event.rs::us1_s3_derived_read_is_total`
  → the totality/conservation property lives in the new module.
- `crates/taskboard-domain/src/habit.rs::us6_s1_kind_change_rederives_history`
  → the mark-window re-derivation property lives in `habit.rs`'s test
  module (it exercises `habit_event` functions from `habit.rs` — same
  crate, no boundary issue). **[deviation from "proptests in
  habit_event.rs" as a blanket reading — reason: the ux-coverage gate
  matches test paths verbatim; a W-11 row that reads MISSING is not
  done (dispatch plan §2 item 2).]**

### D12 — `AdjustHabitLog`'s former-kind gate, tombstone grammar, reconcile diff

Chunk 2 scope (design §3.2, Ruling H). Recorded here only to fence
this chunk: nothing in chunk 1 mints tombstones or synthetic events;
`deleted` is decode-able and derivation-relevant (excluded) but never
written.

---

## 2. Implementation order (TDD)

Each numbered step is at least one standalone commit (rebase-merge
hygiene; `<type>(<module>): <desc>` subjects). TDD: the test lands in
the same commit as the code that makes it pass (the house convention —
red/green within the commit, never a red commit on the branch).

1. **feat(domain): habit event model types** — `habit_event.rs` with
   D1/D2 shapes, `lib.rs` wiring, serde round-trip unit tests
   (decode-leniency cases: absent `by`/`deleted` decode to `None`; a
   tombstoned event round-trips; strict id decode rejects garbage —
   typed `matches!`/`Option` assertions only, no text assertions).
2. **feat(domain): the habit event id seam (#1488 context, part 1)** —
   trait method D4 + `UuidV7Generator` arm + `CountingIds` arm +
   `v7_now()` with D5's context; the ordering test (`context_v7_
   mints_locally_total_order`, §3 T8). Commit body carries the
   #1488 behavior-identity note verbatim (D5).
3. **feat(domain): HabitState.events (additive)** — D3; existing suite
   green untouched; one insta snapshot regeneration if any snapshot
   embeds `HabitState` (cited in the commit body per the snapshot
   discipline).
4. **refactor(domain): extract the log_window attribution core** —
   D6's `attribute_window` extraction; no behavior change (existing
   `log_window` unit tests are the red/green evidence).
5. **feat(domain): derive_window_counts + the equivalence oracle** —
   D6 + §3 T1/T2/T3/T4 (the oracle proptest, totality `us1_s3`,
   same-second distinctness, shuffle invariance).
6. **feat(domain): derive_skip_posture + the max law** — D7 + §3 T5/T6.
7. **test(domain): the us6_s1 kind-change re-derivation property** —
   §3 T7 in `habit.rs` (D11).
8. **feat(domain): derive_day_counts** — D8 + §3 T9/T10.

Then the fast gate (§5) and the ux-coverage gate (§6).

---

## 3. Test catalogue

Proptest domain (all strategies): `NaiveDate` within
2025-01-01..2027-01-01 (bounded, the `period_*` saturation leniency
stays unreachable — mirroring the existing habit proptest posture);
instants within a seeded day ± 40 h (to cross window and grace
boundaries); quotas 1..=5; kinds {Day, Week, Month}; day-start times
{00:00, 03:00, 09:00}; offsets {-05:00, UTC, +05:00}. Events are
generated as `Completion` sequences with distinct monotone ids and
non-decreasing (but sometimes equal) `at`s — equal instants are the
tie-law's subject.

| # | Token / name | File | Kind | Pins |
|---|---|---|---|---|
| T1 | `derive_window_counts_matches_legacy_log_time_attribution` | habit_event.rs | proptest | **The equivalence oracle** (dispatch §2 acceptance (a)). Strategy: generate a completion sequence; *simulate legacy logging* — for each event in `(at, id)` order, `window = log_window(&habit_with_frozen_config, &legacy_map, event.at, day)` and upsert the legacy row count+1 (exactly the pre-event-log write semantics, minus clock/`by` which attribution never reads); then `derive_window_counts` over the same sequence; assert the derived map equals the legacy map's counts for every key (legacy map has no count-0 rows on this strategy — no adjust). This pins rule-identity, not reimplementation: the oracle is the *untouched* `log_window`, called with the legacy `HabitState.log`-shaped map. The stage-3 anti-goal (Prompt 1.3 (a)) — a self-confirming reimplementation — is structurally excluded because the fold calls the same extracted `attribute_window` the oracle wraps; the test would fail if either leg changed the rule. |
| T2 | `us1_s3_derived_read_is_total` (W-7) | habit_event.rs | proptest | **Totality/conservation**: for arbitrary sequences × configs, every live completion lands in exactly one window — `derived.values().sum() == live_completion_count`, and every derived key is a canonical window of the current kind (`WindowStart::of_period(kind, key.as_date()) == key`). |
| T3 | `same_second_completions_stay_distinct` | habit_event.rs | unit | Acceptance (c): two events, same `at`, distinct ids → derived count 2 in the (single) window; the `(at, id)` order is deterministic across two runs with shuffled insertion. |
| T4 | `shuffled_input_order_yields_identical_derivation` | habit_event.rs | proptest | The fold sorts internally: `derive_window_counts` over a random permutation of the same event set returns the identical map (compare against the identity-sorted run). |
| T5 | `derive_skip_posture_newest_at_id_wins` | habit_event.rs | unit + proptest | The max law: Skip(at1) then Unskip(at1, greater id) → unskipped; Unskip then Skip with equal `at` but greater id → skipped; absent marks → absent key. Property: idempotent under re-fold (fold twice, same map), and permutation-invariant (same law as T4). |
| T6 | `tombstoned_events_are_excluded_from_every_derivation` | habit_event.rs | unit | Acceptance-adjacent (Ruling C2 posture): a tombstoned completion contributes to neither window nor day counts; a tombstoned mark contributes to no posture. `deleted: Some(...)` fixtures only — nothing writes tombstones until chunk 2. |
| T7 | `us6_s1_kind_change_rederives_history` (W-11) | habit.rs (tests module) | proptest | **The design-named property** (design §2.1): generate a mixed completion+mark sequence; derive windows/posture under config A (e.g. Day, quota 2); re-derive the *same event set* under config B (kind changed to Week, or day-start moved); assert (i) every completion and every mark attributes to a canonical window of kind B (nothing stranded — the D-H12 luggage has no successor), (ii) the marks' derived windows equal the windows `attribute_window` picks for their `at` in the shared B-fold (marks re-derive *exactly like completions* — the design's sentence, pinned), (iii) the derived posture under B equals a fold replayed with marks and completions interleaved by `(at, id)` (order-identity). |
| T8 | `context_v7_mints_locally_total_order` | idgen.rs | unit (miri-gated) | Acceptance (d), #1488: mint N (e.g. 10_000) ids in a tight loop through `UuidV7Generator` (and once through each id kind); assert strict byte-order increase across consecutive mints (`a.as_bytes() < b.as_bytes()`), and `get_version_num() == 7`. Under the shared context this must hold even within one wall millisecond — the counter's monotonicity. Same `#[cfg_attr(miri, ignore)]` env-gate class as the existing idgen test. If the loop is ever too fast to sample multiple milliseconds the counter leg still carries the assertion — no sleep, no timing assertion (deterministic-by-construction rule). |
| T9 | `derive_day_counts_attributes_calendar_days` | habit_event.rs | proptest | D8: sum == live completion count; every completion lands on `day.today(event.at)`; day-start/offset variations shift attribution exactly per the calendar (no grace logic). |
| T10 | `derive_day_counts_match_legacy_overlay_on_correspondence_sets` | habit_event.rs | proptest | D8's oracle leg: on the T1 correspondence sets (at == log-now), derived day map == legacy overlay counts (`day.today(log-now)` per increment). |
| T11 | serde round-trips (several small units) | habit_event.rs | unit | D2/D9: default-absent `by`/`deleted`; tombstone round-trip; strict id parse failures typed as `None`; `HabitState` with `events` round-trips and a state JSON without the key decodes to an empty map. |
| T12 | `new_habit_event_id_returns_v7_ids` | idgen.rs | unit (miri-gated) | Version nibble 7; distinct across calls; `CountingIds::new_habit_event_id` is deterministic (counter). Note: the CountingIds leg is a seam sanity check, not a test-helper behavior test — kept to one assertion, no expansion (AGENTS §5: don't test test-helpers). |

Clock discipline check (testing §6.2): every test above is pure
now-as-data — instants are fixture data, no wall read except T8/T12's
minting (the production generator's nature, miri-gated). No
`tokio::time` anywhere (no async in the domain crate).

ux-coverage tokens this chunk must flip to FOUND: `us1_s3` (T2),
`us6_s1` (T7). `us1_s1_log_habit_mints_one_event` (W-1) is chunk 2 —
expected MISSING until then, reported honestly, never papered over.

---

## 4. Snapshot / fixture churn forecast

- Insta snapshots embedding `HabitState` (the state-engine suite)
  gain an `"events": {}` key after step 3's regeneration — cited per
  snapshot in the commit body (behavior change: the additive field).
  No other churn: this chunk writes no commands, so no engine
  sequence changes.
- Golden corpora / wire fixtures: untouched (chunk 4).
- `persistence.rs` apply tests: untouched (the persistence action
  pair swaps in chunk 2). Verified: no `HabitState` literal in
  `persistence.rs` asserts field-count-shape — the additive serde
  default keeps decode-identical behavior.

---

## 5. Gates (long-command discipline — binding for stages 2 and 3)

A subagent is killed after ~600s without observable activity, so long
silent commands kill working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe, ~4–6 min; background + poll with useful work between
   polls if needed). The dispatcher runs the full AGENTS §10 suite in
   its own session between stage 2's landing and stage 3's ship; CI's
   Code Coverage check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: the stage-3 agent rebases,
   reviews, fixes, pushes, opens the PR, and reports — it does NOT
   watch CI. The dispatcher owns `gh pr checks --watch`, the
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures.
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
   *Flock deferral (#879)*: when the gate lock is contended and no
   other useful work remains, never block — commit all landed work,
   report "gate deferred: lock held by <holder>", and end the stage;
   the dispatcher runs the gate.
4. *Stage prompts restate this discipline verbatim* (an agent that
   must find it in another file will not carry it).
5. *Resource law*: every cargo invocation routes through
   `scripts/memory-gate.sh` (the 2026-11-23 systemd-oomd law) —
   nothing bare; large scratch under /var/tmp, never /tmp.
6. *Every stage, every agent*: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

---

## 5. Stage-2 (IMPLEMENTATION) prompt — verbatim, paste-ready

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the habit event-log dispatch ("domain event model + derivations"). Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-domain-habit-events (branch feat/domain-habit-events); every command by absolute path; never modify the main checkout /home/viktoria/src/taskboard.

**Long-command discipline (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (3) Every agent-launched cargo invocation routes through scripts/memory-gate.sh (the 2026-11-23 systemd-oomd law) — nothing bare. (4) Out-of-scope findings become `backlog:` GitHub issues (or comments on existing ones) the moment they are discovered, without asking (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.**

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0/§1/§2, the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§2, and the stage-1 plan .artifacts/plans/2026-11-27-habit-events-domain-plan.md (all three are in the worktree) in /home/viktoria/src/taskboard-WT-feat-domain-habit-events (branch feat/domain-habit-events). If the worktree is missing, recreate it on the branch from the main checkout: git -C /home/viktoria/src/taskboard worktree add -b feat/domain-habit-events ../taskboard-WT-feat-domain-habit-events origin/main; if that branch's PR already landed, use feat/domain-habit-events-v2 per AGENTS §8 (never `git worktree add <path> main`, never a slash-suffixed worktree path).
2. Implement exactly per the approved plan (its §1 decisions D1–D12, §2 order, §3 catalogue), TDD, every commit standalone (rebase-merge hygiene). Hard rules: SPDX headers on every new .rs file; #![forbid(unsafe_code)] (crate-wide already); no tokio::time::sleep in tests — pure now-as-data fixtures (the plan's catalogue needs no time manipulation at all); no text assertions (typed matches!/Option outcomes only); proptests for the derivation laws; the legacy log_window stays as the equivalence oracle (it is deleted in chunk 5, not here); scratch never lands; no command/persistence changes; HabitState.log and day_log and every existing reader untouched; no uuid v5 feature (chunk 3); mint_log_clock untouched (chunk 5).
3. Run the fast gate per the long-command discipline above: the pre-commit hooks run automatically; the full suite is scripts/run-quality-gates.sh cargo nextest run --workspace — background it if slow, poll, do useful work between polls; commit all landed work BEFORE any wait. The coverage trio (cargo fmt --check, clippy -D warnings, llvm-cov) never runs in this session — the dispatcher owns it. Every cargo invocation routes through scripts/memory-gate.sh, never bare. If the gate lock is contended and nothing else remains: commit, report "gate deferred: lock held by <holder>", and end.
4. Do NOT touch the backlog origin entries (#1519/#1488 annotations land in chunk 5); do not modify HabitState.log's readers — chunk 2's job. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate from the worktree and report the honest steady state: rows this chunk claims (W-7 us1_s3, W-11 us6_s1) must read FOUND; design-stage MISSING on chunks 2–5's rows (including W-1 us1_s1_log_habit_mints_one_event) is expected and reported, never papered over.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
6. Backlog issues per the discipline above; list the issue numbers in your report.

Report: per-gate results, the #1488 behavior-identity evidence (the idgen doc comment + the T8 test), the ux-coverage gate output summary, any deviation from the plan and why.
```

---

## 6. Stage-3 (REVIEW) prompt — verbatim, paste-ready

```text
You are the stage-3 REVIEW agent for chunk 1 of the habit event-log dispatch (branch feat/domain-habit-events). Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-domain-habit-events; every command by absolute path; never modify the main checkout /home/viktoria/src/taskboard.

**Long-command discipline (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, the `mergeStateStatus CLEAN` verification, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh (the 2026-11-23 systemd-oomd law) — nothing bare. (5) Out-of-scope findings become `backlog:` GitHub issues (or comments on existing ones) the moment they are discovered, without asking (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.**

1. **Begin by rebasing onto main**: git fetch origin --prune, then rebase the branch onto origin/main (AGENTS §8's rebase dance: drop already-merged duplicate commits; resolve only real conflicts), push --force-with-lease if the rebase rewrote history. Then read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0/§1/§2, the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§2 (the code-quality mandate below applies to you), and the stage-1 plan .artifacts/plans/2026-11-27-habit-events-domain-plan.md; then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD) in the worktree.
2. Functional focus, named from the origin: (a) derive_window_counts must reproduce log_window's attribution (natural window, 24 h grace, guarded surplus-repair) AS OF each event's completion instant over (at, id) order — a subtle recursion; verify the equivalence-oracle test really pins 1:1 legacy-row correspondence via the untouched log_window as oracle, not a self-confirming reimplementation (the stage-1 plan's T1 states the structural exclusion — check it holds in the landed code); (b) the #1488 ContextV7 adoption must not change the IdGenerator trait surface beyond the additive new_habit_event_id and must not break merge_testutil::CountingIds injection; the behavior-identity note must be present in the idgen doc comment and the commit body; (c) no existing test weakened; (d) A1 discipline — no derive crates, fallible/id-seam-only minting at the boundary, documented leniency at every Option (D1/D2's comments), no validate-and-forget (the derivations re-check liveness themselves); (e) zero network/async-runtime dependencies in taskboard-domain (AGENTS §2 rule); (f) the additive scope held: no command/persistence changes, log/day_log readers untouched, no sqlite/wire/UI work, no uuid v5, mint_log_clock still present.
3. **Code-quality mandate**: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crate; the derivation fold never leaves the domain crate; the UI never computes habit semantics); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the fast gate yourself per the long-command discipline (pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace, backgrounded with polls, work committed before any wait; flock deferral rule applies; every cargo invocation through scripts/memory-gate.sh). The coverage trio never runs in this session.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship — **your scope ends at PR-open; do not remove the worktree**: push the branch (--force-with-lease only, never plain --force), open the PR to main with gh, then report the URL — do NOT run gh pr checks --watch; the dispatcher owns CI watch and merge-state verification, teardown (git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-domain-habit-events), and at-merge issue closures. Never merge; never touch main.
7. Backlog issues per the discipline above; list the issue numbers in your report.

Report: verdict, findings (functional + mandate), what you fixed, PR URL. Never merge; never touch main.
```

---

## 7. Risks

- **R1 — the Mutex-in-OnceLock deviation (D5)**: the design's literal
  `OnceLock<ContextV7>` does not compile (`ContextV7` is `!Sync`).
  Mitigated: recorded deviation with the verified source cite; the
  guarantee uuid makes (per-process monotonicity) is preserved by the
  shared context; the lock is per-mint and uncontended in practice.
  Review focus (b) checks the trait surface and CountingIds.
- **R2 — the equivalence oracle's faithfulness (T1)**: if the legacy
  simulation drifts from the actual historical write path (e.g. it
  must skip `mint_log_clock`'s bump — attribution never reads clocks,
  so it is sound), the test could pin a fiction. Mitigated: the
  simulation calls the *untouched* `log_window` (the real oracle), and
  stage-3 focus (a) audits the correspondence.
- **R3 — mark attribution under grace/surplus** is the subtlest law
  (D7's shared-fold decision). If stage 3 finds the design intended
  mark-only attribution (no count dependence), the fix is local
  (change `attribute_window`'s caller for marks) — the property test
  T7 would be rewritten to the same law. Flagged here so the reviewer
  reads D7 against design §2.1's parenthetical deliberately.
- **R4 — snapshot churn**: any snapshot embedding `HabitState` changes
  shape (additive key). Low risk, cited churn per the discipline.
- **R5 — proptest runtime**: the catalogue adds ~5 proptests to
  `taskboard-domain`; keep case counts in line with the crate's
  existing proptest posture so the fast gate stays ~4–6 min.

## 8. Estimated diff size

- `habit_event.rs` (new): ~250 lines code + ~350 tests.
- `idgen.rs`: ~40 lines + ~30 test lines.
- `habit.rs`: ~10 lines (`events` field) + ~80 test lines (T7).
- `lib.rs`: 1 line. `test_support/mod.rs`: ~6 lines.
- **Total: ~750–800 lines, all inside `taskboard-domain`.**
