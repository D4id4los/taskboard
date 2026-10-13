# [Plan] Reminders — Chunk 1: Domain Model & Evaluation (`feat/reminders-domain-model`)

- **Date**: 2026-10-13
- **Kind**: stage-1 chunk plan (dispatch plan §2, prompt 1.1) — TDD-first
  implementation spec. **No production code in this commit.** Stage 2
  implements on this branch; stage 3 reviews and ships.
- **Origin / parent**: design round
  `.artifacts/plans/2026-10-12-reminders-design-plan.md` (§4 domain model,
  §9 persistence actions, §13 decisions — **normative**; §4.4 D-a…D-e) and
  dispatch plan
  `.artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md`
  §1/§2 (chunk-1 scope + acceptance).
- **Target**: `taskboard-domain` (new `reminders.rs`, `ids.rs`, `state.rs`,
  `persistence.rs`, `lib.rs`, `test_support/`), plus compile-driven
  one-liners in `taskboard-state` (`core.rs`, test literals) and
  `taskboard-storage-sqlite` (`repo.rs` load hydration — read side only),
  plus one mechanical insta-snapshot regeneration (14 files). **No new
  dependencies.** No engine behavior, no planner, no outbox, no migration,
  no `Notification`, no config, no CLI (chunk boundaries in §2).
- **Branch / worktree**: `feat/reminders-domain-model` in
  `../taskboard-WT-feat-reminders-domain-model`, off `main` @ `bdd9d97`.
- **Goal**: the reminder half of the domain exists as pure, proptest-able
  data + functions, and the five new `PersistenceAction` variants have
  their semantics defined exactly once in `apply_actions` — inherited
  unchanged by the engine's memory advance and every fake — so that chunks
  2–3 only wire an engine arm, tables, and a CLI onto a settled model.
- **Status**: PROPOSED — ready for stage 2. Every open question resolved
  with a recorded decision (§10); deviations from the design round are
  flagged there with reasons.

---

## 1. Context & verified facts (do not re-derive; verified on `main` @ `bdd9d97`)

- **AppState** (`state.rs:24-39`): BTreeMaps of entities + `sync` +
  `last_updated`; derives `Default`; only `Serialize` (not `Deserialize`).
  Doc comment declares shape changes snapshot-breaking. Canonical task
  order = `(order, id)` asc (`state.rs:109-116`, `sorted_tasks`, private).
- **Task eligibility helpers exist**: `is_live` / `is_open`
  (`entities.rs:189-198`); `duedate: Option<DateTime<Utc>>`
  (`entities.rs:166`).
- **`PersistenceAction`** (`persistence.rs:170-191`) is a closed additive
  enum; **`apply_actions`** (`persistence.rs:206-245`) defines per-action
  semantics once and ends with an *unconditional post-batch derivation*
  (`sync.pending_ops` re-derived from outbox depth, `persistence.rs:242-244`)
  — the precedent the snooze-chain compaction post-pass follows.
- **`PersistedState`** (`persistence.rs:79-96`) derives
  `Serialize + Deserialize`; `From<PersistedState> for AppState`
  (`persistence.rs:98-109`) maps field-by-field via a full struct literal.
- **Local id newtypes** come from the `local_id!` macro (`ids.rs:10-40`):
  `from_uuid` / `as_uuid` / `From<Uuid>` + derives incl. `Ord`/`Hash`.
- **The fake inherits semantics by delegation**:
  `InMemoryRepository::apply` calls the shared `apply_actions`
  (`test_support/memory.rs:61-67`) — new action semantics reach the fake,
  the engine's memory advance, and every strategy-driven proptest with
  zero fake-side code.
- **The repo contract harness** (`test_support/contract.rs`) is the fake ↔
  sqlite parity suite; sqlite runs it from its own tests. Its reminder
  clauses are a **chunk-2** extension point (tables must exist first, D8).
- **`EngineCore::app()`** builds `AppState` via full struct literal
  (`taskboard-state/src/core.rs:50-59`) — gains one `reminders` line.
- **sqlite `load_inner`** constructs `PersistedState` via full literal
  (`repo.rs:492-504`) — gains one `reminders` line. **`execute_batch`**
  (`repo.rs:548-839`) matches with `if let` partitions only, so new enum
  variants compile without touching it (and must not be matched before the
  tables exist — D8). `actions_from_state`
  (`storage/tests/common/mod.rs:10`) builds seed actions from state fields
  (entity upserts/outbox/validators/status) — stays reminder-blind here.
- **Snapshots**: exactly 14 `.snap` files, all in
  `taskboard-state/tests/snapshots/` (scenarios s1–s7 × persisted +
  published). Seeded via `seed_board_state` (`tests/common/mod.rs:63`,
  `PersistedState::default()` + one board) — never via strategies, and no
  scenario creates reminder data in this chunk, so every snapshot delta is
  the empty `reminders:` section only.
- **Compile-driven `AppState` literal sites** (all need one added line):
  domain `command.rs:737`, `command.rs:1737`, `command.rs:1939` (tests),
  domain `pipeline.rs:697`, `pipeline.rs:854` (tests), state `core.rs:51`,
  state `tests/proptest.rs:199`, `tests/proptest.rs:572`.
  **`PersistedState` literal sites**: `repo.rs:492` (full), domain
  `test_support/state.rs:436` (inside `persisted_state_strategy`), storage
  `tests/roundtrip.rs:456` (partial, `..Default::default()` — no change
  needed), state `tests/common.rs:63` (default-based — none), sync
  `tests/e2e_hermetic.rs:142` (check at implementation; likely partial).
- **Strategy consumers**: `persisted_state_strategy` /
  `app_state_strategy` are used only by domain `command.rs` tests and
  `taskboard-state` proptests (`tests/proptest.rs:163,175`) — the sqlite
  roundtrip suite hand-builds its states. Growing the strategies with
  reminder data is therefore safe for every existing suite (all consumers
  are pure-domain or fake-repo backed; D15).
- **Migrations on `main`**: 0001–0003 → chunk 2's reminders migration is
  `0004_reminders.sql` exactly as the design round assumed (§9).
- **Domain deps** (`Cargo.toml`): serde, chrono, uuid, thiserror, tracing;
  proptest behind `test-support`. Nothing new needed. Crate is
  `#![forbid(unsafe_code)]`; new file carries
  `// SPDX-License-Identifier: MIT OR Apache-2.0`.
- **cargo-mutants** runs weekly/on-demand against `taskboard-domain` and
  `taskboard-state` (docs/architecture.org §tooling) — the new evaluation
  functions are the primary mutant surface; keep every branch observable
  through a property or unit test (§9.8).

## 2. Scope & non-goals (chunk boundary, binding for stage 2)

**In scope** (dispatch §2): `taskboard-domain/src/reminders.rs` —
`ReminderRule`, `ReminderDelivery`, `ReminderSnooze`, `ReminderRef`,
`ReminderDefaults`, `ReminderState`, `ReminderFiring`, the
`ReminderRuleId` newtype, the `ReminderOffset` guard; pure functions
`occurrence_of`, `effective_occurrence`, `due_firings`, `overdue_tasks`,
`due_soon`; five additive `PersistenceAction` variants + their
one-semantics `apply_actions` definitions (delivery set-union, snooze
replace, delete-cascade prune, mute, chain compaction);
`AppState`/`PersistedState` grow `reminders: ReminderState` (insta update
once, mechanically); reminder proptests P1–P8 + serde roundtrips;
referentially-consistent strategies; the sqlite `load` hydration line
(read-side parity).

**Out of scope** (later chunks — do not drift into):
engine tick arm / notifier channel / `SnoozeReminder` + mute
`StateCommand`s / `DeleteTask`→rules cascade *emission* (all chunk 2 —
planner work, see handoff §14); migration `0004`, sqlite write side,
codec `reminder` text forms, contract-harness reminder clauses (chunk 2);
`Notification` payload + `NotificationSink` (chunk 2/3 per design §6);
`[reminders]` config, CLI verbs, `--json` projections (chunk 3); any
outbox/`LocalOp`/merge change (**no reminder data enters any outbox
path** — acceptance); UI (Phase 7 consumes later).

## 3. Module layout

| File | Change |
|---|---|
| `domain/src/reminders.rs` | **new** — types (§4), pure functions (§5), `ReminderState` helpers, in-module unit + property tests (§9) |
| `domain/src/ids.rs` | + `ReminderRuleId` via `local_id!` (D1) |
| `domain/src/state.rs` | + `pub reminders: ReminderState` on `AppState` (field order: after `labels`, before `sync` — entity sections first, then status; doc note) |
| `domain/src/persistence.rs` | + 5 `PersistenceAction` variants; + 5 `apply_actions` arms + compaction post-pass; `PersistedState` + `reminders`; `From` impl maps it; unit/proptest additions |
| `domain/src/lib.rs` | + `pub mod reminders;` + re-exports (module-map doc line updated) |
| `domain/src/test_support/reminders.rs` | **new** — strategies (§9.1) |
| `domain/src/test_support/state.rs` | wire reminder rows into `app_state_strategy` / `persisted_state_strategy` (builder fn, `build_outbox` precedent) |
| `domain/src/test_support/mod.rs` | + `pub mod reminders;` + flat re-exports |
| `state/src/core.rs` | `app()` maps `reminders` (1 line) |
| `state/tests/proptest.rs` | 2 literal sites (1 line each) |
| `storage/src/repo.rs` | `load_inner` hydrates `reminders: ReminderState::default()` (1 line, D8) |
| `state/tests/snapshots/*.snap` | 14 files, mechanical regen (§7.3) |

Domain-internal test literals (`command.rs` ×3, `pipeline.rs` ×2) ride
with the commit that adds the field.

## 4. Type definitions (exact)

All in `reminders.rs` except `ReminderRuleId`. Every public item carries a
rustdoc comment; serde derives follow the house shapes (P6 roundtrips
them all).

```rust
// ids.rs — the id registry, via the existing local_id! macro (D1)
local_id!(
    /// Local identity of a reminder rule (`UUIDv7`, generated client-side).
    ReminderRuleId
);

// reminders.rs

/// A due-anchored offset that can only be "at or before" the due date
/// (D-e): the inner `chrono::Duration` is ≤ 0 by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize, Default)]
pub struct ReminderOffset(chrono::TimeDelta);
// Default = zero ("at due") — least opinionated; the −5 m figure is
// per-device config (chunk 3), never a domain constant (D11).

impl ReminderOffset {
    /// The only construction path for a non-zero offset: `dur` must not
    /// be positive ("after due" nagging is the repeat-policy question,
    /// design §4.4 D-e — not an offset sign).
    ///
    /// # Errors
    /// [`InvalidReminderOffset`] when `dur` > zero.
    pub fn before(dur: chrono::Duration) -> Result<Self, InvalidReminderOffset>;

    /// Zero offset: fire exactly at the due date.
    #[must_use]
    pub fn at_due() -> Self;                       // Self::default()

    /// The wrapped duration (≤ 0 always).
    #[must_use]
    pub fn as_duration(self) -> chrono::Duration;
}

/// `dur > zero` — positive offsets are not constructible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidReminderOffset {
    /// The proposed offset lies after the due date.
    #[error("reminder offset must not be after the due date")]
    AfterDue,
}

/// Which reminder a firing/delivery/snooze refers to: an explicit rule,
/// or a task's implicit default (no stored row — its identity is the
/// task, design §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         Serialize, Deserialize)]
pub enum ReminderRef {
    /// An explicit rule.
    Rule(ReminderRuleId),
    /// The task's implicit default reminder.
    Default(TaskId),
}

/// A reminder rule anchored on a task's due date. N rules per task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderRule {
    /// Local identity.
    pub id: ReminderRuleId,
    /// The watched task.
    pub task: TaskId,
    /// Fire this far before the due date; zero = at due (≤ 0 by
    /// construction via [`ReminderOffset`], D-e).
    pub offset: ReminderOffset,
    /// Write time. Sync upgrade path (D-a): per-field write clocks are
    /// added additively when a connector demands rule sync; v1 needs
    /// only the creation stamp.
    pub created_at: DateTime<Utc>,
}

/// Proof that an occurrence was notified — the at-most-once guard (D-b).
/// Keyed by `(reminder, occurrence)`, NOT by rule alone: moving the due
/// date or snoozing creates a new occurrence and re-arms. Additive,
/// set-union-mergeable by construction (D-a).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord,
         Serialize, Deserialize)]
pub struct ReminderDelivery {
    /// Which reminder fired.
    pub reminder: ReminderRef,
    /// The fire instant this delivery settled.
    pub occurrence: DateTime<Utc>,
    /// When the engine recorded it (stamped at fire time).
    pub delivered_at: DateTime<Utc>,
}
// Ord field order (reminder, occurrence, delivered_at): the (reminder,
// occurrence) prefix is the §9 SQL primary key — the ≤1-row-per-key
// invariant is pinned by proptest (§9.5).

/// A "show me again later" (D-b): the occurrence is *rescheduled*, not
/// suppressed — the evaluator walks the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderSnooze {
    /// Which reminder was snoozed.
    pub reminder: ReminderRef,
    /// The occurrence being snoozed.
    pub occurrence: DateTime<Utc>,
    /// When it fires again (`now + postpone`, stamped by the engine).
    pub rescheduled_to: DateTime<Utc>,
}

/// Per-device defaults for the implicit reminder (D5). Injected config
/// data, never persisted in `ReminderState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReminderDefaults {
    /// Offset before due for the implicit default reminder (≤ 0).
    pub default_before: ReminderOffset,
}

/// One due firing: exactly the triple that becomes a
/// [`ReminderDelivery`] row plus a notification payload downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
         Serialize, Deserialize)]
pub struct ReminderFiring {
    /// Which reminder fires.
    pub reminder: ReminderRef,
    /// The task it belongs to.
    pub task: TaskId,
    /// The effective occurrence being fired.
    pub occurrence: DateTime<Utc>,
}

/// Persisted reminder facts carried in `AppState`/`PersistedState`
/// (design §4.2). Field order is the snapshot order: rules, deliveries,
/// snoozes, muted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderState {
    /// Explicit rules by id.
    pub rules: BTreeMap<ReminderRuleId, ReminderRule>,
    /// Delivery proofs (≤ 1 row per `(reminder, occurrence)` — §9.5).
    pub deliveries: BTreeSet<ReminderDelivery>,
    /// Snooze chain links: `(reminder, occurrence) → rescheduled`
    /// instant. Chains compose (a snooze target is itself snoozable).
    pub snoozes: BTreeMap<(ReminderRef, DateTime<Utc>), DateTime<Utc>>,
    /// Tasks muted out of firings entirely. The overdue *banner* ignores
    /// this set — mute means "stop nagging", not "hide reality".
    pub muted: BTreeSet<TaskId>,
}

impl ReminderState {
    /// Delivery lookup on the `(reminder, occurrence)` key (the set is
    /// keyed, not identity-compared — see §9.5).
    #[must_use]
    pub fn is_delivered(&self, reminder: &ReminderRef, occurrence: &DateTime<Utc>) -> bool;
}
```

`is_delivered` is a linear `iter().any()` over the key prefix with a doc
note on the ≤1-row invariant; kiosk-scale sets make a range dance
unwarranted (revisit only if chunk-3 CLI profiling says so).

**Serde note**: `ReminderRef` uses default externally tagged forms
(`{"rule": "…"}` / `{"default": "…"}`) on the wire/snapshot path. The
storage text codec forms (`"default:<task>"`, design §9) are chunk-2
`codec.rs` work (D14).

## 5. Pure functions (exact signatures & operational semantics)

```rust
/// The instant an explicit rule fires at, given the task's current
/// duedate. `None` ⇔ no duedate. Pure in `(offset, duedate)`.
#[must_use]
pub fn occurrence_of(rule: &ReminderRule, duedate: Option<DateTime<Utc>>)
    -> Option<DateTime<Utc>>;

/// Effective occurrence of any reminder (explicit or default): the base
/// occurrence, or wherever the snooze chain has moved it.
#[must_use]
pub fn effective_occurrence(
    reminder: &ReminderRef,
    reminders: &ReminderState,
    tasks: &BTreeMap<TaskId, Task>,
    defaults: &ReminderDefaults,           // D3: added vs design §4.3
) -> Option<DateTime<Utc>>;

/// Everything that must fire at `now`: explicit rules plus implicit
/// defaults, occurrence-keyed, snooze- and mute-aware. Sorted by
/// `(occurrence, reminder)` (D-d).
#[must_use]
pub fn due_firings(
    tasks: &BTreeMap<TaskId, Task>,
    reminders: &ReminderState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
) -> Vec<ReminderFiring>;

/// Kiosk banner: live, open tasks with duedate < `now`, canonical order
/// (`(order, id)` — the `AppState::live_tasks` comparator, state.rs:109-116).
/// Ignores rules, deliveries, snoozes, and mute — board truth, not
/// nagging policy (design §7).
#[must_use]
pub fn overdue_tasks<'a>(
    tasks: &'a BTreeMap<TaskId, Task>,
    now: DateTime<Utc>,
) -> Vec<&'a Task>;

/// Desktop "due soon" strip: same eligibility, duedate in
/// `(now, now + horizon]` (design §4.3).
#[must_use]
pub fn due_soon<'a>(
    tasks: &'a BTreeMap<TaskId, Task>,
    now: DateTime<Utc>,
    horizon: chrono::Duration,
) -> Vec<&'a Task>;
```

**Operational semantics (the contract stage 2 implements against):**

- `occurrence_of` = `duedate.map(|d| d + rule.offset.as_duration())`.
  Addition of a non-positive `TimeDelta` cannot overflow for any
  chrono-representable duedate; no clamping.
- `effective_occurrence`:
  1. *Base*: `Rule(id)` → `reminders.rules.get(id)`?`.task` →
     `tasks.get(task)`?`.duedate`? → `occurrence_of`; `Default(task)` →
     `tasks.get(task)`?`.duedate`? + `defaults.default_before`. Missing
     rule/task/duedate ⇒ `None`.
  2. *Snooze walk*: `effective = base`; follow
     `snoozes[(reminder, effective)]` while present, with a `visited` set
     — on a revisit (a cyclic chain: unsanctionable data, D7) the walk
     stops at the last node before the repeat. Total for arbitrary input.
- `due_firings`:
  1. For each task in `BTreeMap` order: skip unless
     `is_live() && is_open()`, has a duedate, and is not in `muted`.
  2. Candidates: the task's rules (in rule-id order) — or, iff the task
     has **no** rule at all, `ReminderRef::Default(task)` (explicit rules
     replace the default, D5 — derived, nothing stored).
  3. Fire iff `effective_occurrence` is `Some(e)` with `e <= now` and
     `!is_delivered(reminder, &e)`.
  4. Sort by `(occurrence, reminder)` — a total order (a reminder appears
     once per call), so output is fully deterministic (D-d).
- `overdue_tasks` / `due_soon`: filter + the canonical
  `sort_by_key(|t| (t.order, t.id))`; the comparator is duplicated inline
  with a doc note pinning it to `AppState::live_tasks`, and P4
  cross-checks the two against each other.
- Time is always a parameter (clock.rs contract); no function reads a
  clock, generates ids, or allocates channels. Everything is `#[must_use]`
  and panic-free on arbitrary input (mutants/Miri surface).

## 6. `PersistenceAction` variants & `apply_actions` semantics

Five additive variants (design §9, exact names):

```rust
/// Insert or replace a reminder rule.
UpsertReminderRule(crate::reminders::ReminderRule),
/// Remove a rule **and prune its deliveries and snooze links** (D-c:
/// bounded growth without a background GC).
DeleteReminderRule(crate::reminders::ReminderRuleId),
/// Record a delivery proof. Set-union on the `(reminder, occurrence)`
/// key: a replayed delivery is a no-op — first write wins (§9 SQL PK
/// parity; the crash-between-apply-and-notify moment stays benign).
RecordReminderDelivery(crate::reminders::ReminderDelivery),
/// Record a snooze link. Replaces any prior link for the same
/// `(reminder, occurrence)`.
RecordReminderSnooze(crate::reminders::ReminderSnooze),
/// Mute (`true`) or unmute (`false`) all firings of a task. The banner
/// is unaffected by construction (it never reads `muted`).
SetTaskRemindersMuted(crate::ids::TaskId, bool),
```

`apply_actions` arms (persistence.rs, after `UpsertSyncStatus`):

```rust
PersistenceAction::UpsertReminderRule(rule) => {
    state.reminders.rules.insert(rule.id, rule.clone());
}
PersistenceAction::DeleteReminderRule(id) => {
    state.reminders.rules.remove(id);
    state.reminders.deliveries
        .retain(|d| d.reminder != ReminderRef::Rule(*id));
    state.reminders.snoozes
        .retain(|(r, _), _| *r != ReminderRef::Rule(*id));
}
PersistenceAction::RecordReminderDelivery(delivery) => {
    if !state.reminders.is_delivered(&delivery.reminder, &delivery.occurrence) {
        state.reminders.deliveries.insert(delivery.clone());
    }
}
PersistenceAction::RecordReminderSnooze(snooze) => {
    state.reminders.snoozes
        .insert((snooze.reminder, snooze.occurrence), snooze.rescheduled_to);
}
PersistenceAction::SetTaskRemindersMuted(task, muted) => {
    if *muted { state.reminders.muted.insert(*task); }
    else { state.reminders.muted.remove(task); }
}
```

**Chain compaction post-pass** (unconditional, after the action loop,
next to the `pending_ops` re-derivation — design §9 "same batch, defined
once in `apply_actions`"):

```rust
// A snooze link whose source and target occurrences are both delivered
// can never influence evaluation again — both endpoints suppress on
// their own. Pruning it bounds chain growth with zero behavioral change
// (pinned by the compaction-unobservability property, §9.6). Deviation
// from design §9's "target has a delivery": target-only pruning provably
// re-arms a snoozed-but-undelivered source — see D6.
state.reminders.snoozes.retain(|&(src_ref, src_occ), &target| {
    state.reminders.is_delivered(&src_ref, &src_occ)
        && state.reminders.is_delivered(&src_ref, &target)
});
```

Consequences spelled out for stage 2: `apply_actions(&mut s, &[])` is the
normalization oracle (runs the post-pass); hand-built states may be
un-compacted, but any state that has been through a batch is compacted —
and the shared strategies generate **compacted** states only (§9.1), so
`empty_batch_is_identity` keeps holding unchanged.

## 7. State integration & compile-driven changes

### 7.1 Domain

- `AppState` gains `pub reminders: ReminderState` (state.rs; doc: "Persisted
  reminder facts (rules, delivery proofs, snooze links, muted tasks);
  delivered-ness and snoozed-ness are plain set-membership checks").
- `From<PersistedState> for AppState` maps `reminders: persisted.reminders`.
- `lib.rs`: `pub mod reminders;` + re-exports
  (`ReminderDefaults, ReminderDelivery, ReminderFiring, ReminderOffset,
  ReminderRef, ReminderRule, ReminderRuleId, ReminderSnooze, ReminderState,
  InvalidReminderOffset, due_firings, due_soon, effective_occurrence,
  occurrence_of, overdue_tasks`) + module-map sentence. `ReminderRuleId`
  re-exported from `ids` alongside its siblings.
- Test literals: `command.rs:737/1737/1939`, `pipeline.rs:697/854` — add
  `reminders: ReminderState::default()` (or copy from the source struct
  where the literal mirrors a persisted state, e.g. `command.rs:1939`).

### 7.2 State & storage crates (compile + read parity)

- `state/src/core.rs:51` `app()`: `reminders: self.state.reminders.clone()`.
- `state/tests/proptest.rs:199` (persisted→AppState mirror): copy
  `persisted.reminders.clone()`; `:572` (disk projection):
  `reminders: disk_after.reminders`.
- `storage/src/repo.rs:492`: `reminders: ReminderState::default()` —
  **read-side parity only** (D8): with no reminder tables yet, an empty
  section is exactly what the future empty tables will hydrate, so boot
  behavior is identical now and after chunk 2. The existing
  `port.rs`/`roundtrip.rs` default-equality assertions then cover the
  fresh-database case for free.

### 7.3 Snapshot regeneration (exactly once, mechanical)

```sh
INSTA_UPDATE=always cargo nextest run -p taskboard-state   # or: cargo insta accept
git diff -- crates/taskboard-state/tests/snapshots/
```

Acceptance: all 14 diffs contain **only** the inserted `reminders`
section (empty maps/sets) in the published-`AppState` and
persisted-`PersistedState` snapshots. Any other delta is a finding.
Review, never blind-accept (stage-2 instruction).

## 8. Storage crate — what deliberately does NOT change (D8)

- `execute_batch`: **untouched**. Its `if let` partitions ignore the new
  variants; matching them against nonexistent tables would fail every
  batch at runtime. Write-side SQL, the `0004_reminders.sql` migration,
  the `reminder`/`"default:<task>"` text codecs, and the contract-harness
  reminder clauses land together in chunk 2 (dispatch §3 covers "sqlite
  batch/load support"), which is also when the first producer (the tick
  arm) exists.
- Deviation note: dispatch §2's parenthetical "(the sqlite repo's
  `apply`/`load` do change)" is satisfied by `load` only — reason: a
  write-side change without a migration is a runtime failure, and chunk
  2's scope explicitly owns it. The *semantics* the repo will execute are
  already fixed here, once, in `apply_actions`.
- `actions_from_state` (tests/common) stays reminder-blind until chunk 2.

## 9. Test strategy (TDD — write failing tests first wherever practical)

### 9.1 Strategies (`test_support/reminders.rs`, new)

- `reminder_offset_strategy()` — `TimeDelta::seconds` in
  `-2_592_000..=0` (≤ −30 d… 0; exercises sign domain without chrono
  extremes).
- `reminder_rule_strategy()` — standalone (id, task, offset, created_at);
  *not* referentially bound (same convention as `task_strategy`).
- `reminder_delivery_strategy()`, `reminder_snooze_strategy()` —
  standalone rows.
- `reminder_rows()` + `build_reminders(tasks: &BTreeMap<TaskId, Task>, rows)
  -> ReminderState` — the referential builder (the `build_outbox`
  precedent, test_support/state.rs): rule rows pick a generated task id;
  delivery/snooze rows pick a `ReminderRef` of an existing rule or
  `Default` of an existing task; occurrence/reschedule instants from
  `timestamp_strategy`; then **normalize to the persisted invariants**:
  first delivery per `(reminder, occurrence)` wins (extras dropped), and
  the compaction retain from §6 is applied so generated states are
  compacted. Muted ⊆ generated task ids.
- Wiring: `app_state_strategy` gains the built `reminders`
  (`AppState { reminders, ..state }` via its prop_map);
  `persisted_state_strategy` gains `reminders: state.reminders` in its
  literal. Verified safe: no sqlite test consumes these strategies (§1).
  Free coverage: the planner/pipeline proptests now prove reminder state
  is inert to every existing command path.

### 9.2 P1 — no double fire / idempotence under settlement

Input: `(state in persisted_state_with_reminders-ish via app_state_strategy,
defaults, now)` — concretely `(tasks, reminders)` from
`app_state_strategy().prop_map(|s| (s.tasks, s.reminders))` +
`reminder_offset`-derived defaults + `now` from `timestamp_strategy`.
Steps: `f1 = due_firings(...)`; assert no `(f.reminder, f.occurrence)`
of `f1` is already in `deliveries`; apply
`RecordReminderDelivery(ReminderDelivery { reminder, occurrence,
delivered_at: now })` for every `f1` into a cloned `ReminderState` (via
`apply_actions` on a `PersistedState` carrying it); `f2 =
due_firings(post, now)`; **assert `f2` is empty** (at a fixed `now`,
eligibility is a pure function of state; the only change is added
suppression — compaction cannot resurrect, both pruned endpoints are
delivered).

### 9.3 P2 — eligibility, exact set (strengthened from ⊆, D10)

Independently re-derive the eligible set with a fresh loop over
`tasks × rules` (mute check, live/open/duedate check, default-only-if-no-
rule, `effective_occurrence` ≤ now, `!is_delivered`) and assert
**equality** of sorted `(occurrence, reminder)` sequences with
`due_firings`' output. A dropped reminder (the worst failure mode —
silence) must fail this, not just a spurious one.

### 9.4 P3 — occurrence monotonicity & shape

For arbitrary rule + `d1 <= d2`:
`occurrence_of(rule, d1)? <= occurrence_of(rule, d2)?`;
`occurrence_of(rule, Some(d)) == Some(d + offset)`; `occurrence_of(rule,
None) == None`. Plus `ReminderOffset::before` accepts every generated
non-positive duration and rejects positive ones (unit test with the
error kind asserted via `matches!`, no text).

### 9.5 P5 — settlement roundtrip through `apply_actions` (fake = sqlite = engine)

For a generated state + `now`: `f1 = due_firings`; batch =
deliveries for `f1` **plus** generated `RecordReminderSnooze` links for
a subset of `f1` (targets strictly future) **plus**
`SetTaskRemindersMuted` toggles for a subset of tasks; `apply_actions`;
assert: (a) none of the settled `(reminder, occurrence)` pairs fires
again at `now`; (b) snoozed reminders do not fire at any sampled `now' <
target`; (c) muted tasks contribute zero firings; (d) post-state
invariants: ≤ 1 delivery row per `(reminder, occurrence)` key, snooze
map keys unique, state compacted (no link with both endpoints delivered)
— (d) is the invariant the strategy normalization of §9.1 mirrors.
Additionally, `PersistedState` serde roundtrips now cover reminders
automatically via the grown strategy (existing proptest, no change).

### 9.6 Compaction unobservability (pins D6)

Property: for arbitrary generated (or hand-mutated — un-compact a link
by inserting both deliveries manually) states,
`due_firings(tasks, reminders, defaults, now)` ==
`due_firings(tasks, compacted, defaults, now)` where `compacted` =
`apply_actions(clone, &[])`'s reminder section. Also unit-pins:
link pruned when source+target delivered; **kept** when only the target
is delivered (the target-only rule's divergence case, D6).

### 9.7 P4 / P7 / P8

- **P4 banner exactness**: `overdue_tasks(tasks, now)` equals the
  independently sorted (by `(order, id)`) set of
  `is_live ∧ is_open ∧ duedate < now` tasks — built with the same
  expected-set style as the `live_tasks` proptest (state.rs tests) —
  and is **invariant under arbitrary reminder state** (generate rich
  rules/deliveries/snoozes/mute, incl. mute of the overdue tasks
  themselves). `due_soon` window unit tests pin `(now, now + horizon]`
  bounds on both edges.
- **P7 snooze suppression**: scenario property — live+open task, duedate
  `d` (generated around `now`), reminder `R` (default and rule variants
  both exercised), snooze link `(R, eff)` → `T = now + postpone`
  (postpone > 0): for sampled `now' ∈ [now, T)`: `R` absent from
  firings; at `now' >= T`: `R` fires iff `(R, T)` undelivered; after
  recording that delivery: never again (sampled).
- **P8 mute exactness**: a muted task produces zero firings under both
  reminder kinds while remaining in `overdue_tasks` when past due;
  `SetTaskRemindersMuted(task, false)` via `apply_actions` restores
  eligibility.

### 9.8 P6 — serde roundtrips & unit pins

Proptest roundtrips (house pattern, `entities.rs:242` style) for
`ReminderRule`, `ReminderDelivery`, `ReminderSnooze`, `ReminderRef`
(both variants), `ReminderState`, `ReminderDefaults`, `ReminderFiring`,
`ReminderOffset` (incl. `at_due()` and strategy extremes). Unit pins:
`InvalidReminderOffset` rejection; `ReminderState::is_delivered` key
semantics; `due_firings` sort order (D-d) on a hand-built multi-firing
fixture.

Mutants discipline (weekly job targets this crate): every branch in §5/§6
is observable through some property above — eligibility flips die on
P2, suppression flips on P1/P7, mute flips on P8, walk/comparator flips
on P4/D-d, compaction flips on §9.6.

Miri discipline: domain tests run under Miri — no `Utc::now()`, no
insta, timestamps only from strategies/fixtures (existing convention).

## 10. Decisions (all open questions resolved; **bold** = deviation from the design round, with reason)

- **D1** `ReminderRuleId` lives in `ids.rs` via `local_id!`, not in
  `reminders.rs` as the design's code block places it. Reason: `ids.rs`
  is the id registry (module-map rule); the macro yields the full house
  derive set (`Ord`, `Hash`, serde) for free. Placement only — the type
  is exactly the design's.
- **D2** D-e is enforced by a `ReminderOffset` newtype (private inner
  `TimeDelta`, `before()` returning `Result`, `at_due()`,
  `as_duration()`), used by both `ReminderRule.offset` and
  `ReminderDefaults.default_before`. Reason: the design offers
  "newtype/constructor validation" — a bare `chrono::Duration` field
  cannot be constrained by construction; the newtype is (serde derive
  reaches the private field from the same module; strategies construct
  only valid values).
- **D3** `effective_occurrence` gains a `defaults: &ReminderDefaults`
  parameter absent from the design §4.3 signature. Reason:
  `ReminderRef::Default(task)`'s base occurrence is defined by the
  per-device offset (D5) and is incomputable without it — the design's
  own `due_firings` already carries `defaults`; this is its per-reminder
  sub-function. Chunk 2's prompt cites the design signature — handoff
  note in §14.
- **D4** `DeleteReminderRule` prunes the rule's deliveries and snooze
  links *inside* `apply_actions` (not by planner-emitted extra actions).
  Reason: D-c requires "pruned with them"; one self-contained action
  keeps the semantics in the single definition site and makes delete
  replay idempotent. The `DeleteTask`→rules cascade *emission* remains
  chunk-2 planner work (§14).
- **D5** Delivery semantics = insert iff no row shares the
  `(reminder, occurrence)` key; an existing row is untouched
  (first-write-wins). Reason: §9's SQL PK is `(reminder, occurrence)`;
  with the normative `BTreeSet<ReminderDelivery>` shape, whole-struct
  insertion would admit two rows per key on a replay with a different
  `delivered_at` — a fake/sqlite parity bug. "Replayed delivery is a
  no-op" (§9) is exactly first-write-wins.
- **D6 (deviation)** Chain compaction prunes a link iff **both** its
  source and target occurrences are delivered, not only the target
  (§9's wording). Reason: with target-only pruning, a snoozed-but-
  undelivered source (a *representable* state, even if no sanctioned
  path produces it — snoozing rides a delivered notification, D-b)
  becomes effective again when its target settles: `due_firings` output
  changes under a "prunable" normalization, violating the design's own
  P7 semantics. Both-endpoints pruning is unconditionally unobservable —
  directly property-pinned (§9.6). In sanctioned flows the two rules
  prune identical rows, so the growth-bounding goal (D-c) is unaffected.
- **D7** The snooze walk is cycle-bounded (visited set; stops at the
  last node before a repeat). Reason: `apply_actions` and the evaluator
  must be total on arbitrary data (P5 generates arbitrary states; a
  cyclic link chain would hang the naive walk). Sanctioned writes are
  strictly increasing (target = now + positive postpone from a ≤ now
  occurrence), so cycles are unsanctionable; chunk 2's planner should
  still validate `postpone > 0` (§14 note).
- **D8 (deviation, dispatch §2 parenthetical)** Chunk 1's sqlite delta
  is `load`-side only; `execute_batch` is untouched. Reason: no reminder
  tables exist (no migration by chunk definition); write arms against
  nonexistent tables fail every batch; dispatch §3 assigns "sqlite
  batch/load support" + migration to chunk 2, where the first producer
  exists. Semantics are nonetheless fixed here, once, in `apply_actions`.
- **D9** `ReminderDefaults::default()` = zero offset (at due). Reason:
  least opinionated domain default; the −5 m figure is per-device config
  (chunk 3, design §5.4) — a domain constant would be a hidden
  single-device assumption (D-a).
- **D10 (strengthening)** P2 asserts exact-set equality, not the
  design's `⊆`. Reason: a dropped reminder (silent failure — the worst
  outcome for a reminder system) passes the subset direction; AGENTS §5
  litmus ("if the code got strictly better, would this test still
  pass?") favors the stronger contract implied by the evaluation
  semantics. Any implementation passing equality passes `⊆`.
- **D11** `ReminderFiring` ships in chunk 1 (`due_firings`'s return
  type, design §4.3); `Notification` does not (design §6; chunk 2's
  engine arm converts firings → payloads). Reason: dispatch §2's chunk-1
  list includes the evaluation surface, not the payload; nothing in
  chunk 1 reads a `Notification`.
- **D12** Reminder `StateCommand`s (`SnoozeReminder`, mute), the
  `DeleteTask`→`DeleteReminderRule` cascade emission, and any
  `plan_command` change are chunk 2. Reason: dispatch §2 defines chunk 1
  as model + semantics + snapshots + proptests; chunk 2 §3 owns command
  planning. Handoff list in §14 guards the D-c item.
- **D13** Serde uses default enum tagging for `ReminderRef`; the storage
  text codec (`"default:<task>"`) is chunk-2 `codec.rs` work. Reason:
  wire/snapshot shape ≠ storage text form (house split since the
  validator keys); codecs without tables are dead code.
- **D14** Both shared strategies (`app_state_strategy`,
  `persisted_state_strategy`) grow a referentially-consistent,
  compaction-normalized reminder section now (no parallel strategy
  family). Reason: verified that no sqlite test consumes them (§1), so
  every existing suite stays green while gaining planner-inertness
  coverage for free; chunk 2 needs no strategy churn.
- **D15** `AppState.reminders` field order: `rules, deliveries, snoozes,
  muted` (declaration order = snapshot order), placed after `labels` and
  before `sync`. Reason: entity sections before status; matches
  `ReminderState`'s own field order.

## 11. Implementation & commit order (each commit standalone: builds, gates green, one subject)

1. `feat(domain): reminder entities and offset guard`
   `ids.rs` (`ReminderRuleId`), `reminders.rs` types only (§4),
   `lib.rs` re-exports of types, `test_support/reminders.rs` standalone
   strategies + wiring into `app_state_strategy` (AppState has no
   `reminders` field yet — the built section feeds P-test inputs via
   `prop_map` extraction, not the AppState literal), P6 serde proptest +
   offset-rejection unit tests.
2. `feat(domain): reminder evaluation functions`
   §5 functions (+ `ReminderState::is_delivered`), `lib.rs` fn
   re-exports, P1–P4, P7, P8, D-d sort unit pin, `due_soon` window pins.
3. `feat(domain): reminder persistence actions and compaction`
   `PersistedState.reminders` + 5 variants + `apply_actions` arms +
   compaction post-pass + §9.5/§9.6 tests + P5, `persisted_state_strategy`
   literal wiring, `storage/repo.rs` load hydration line (D8 — required
   by the literal, verified by existing default-equality tests),
   `From` impl compiles unchanged (drops reminders until commit 4 —
   correct while AppState has no field).
4. `feat(domain,state): AppState carries reminder facts`
   `AppState.reminders` + `From` mapping + `core.rs app()` + all AppState
   literal sites (§7.1/§7.2) + the 14-snapshot mechanical regen,
   reviewed per §7.3.

Rationale for the order: types before functions (functions return the
types), functions before persistence (P5 needs `due_firings`),
persistence before the AppState field (P5 runs against
`PersistedState`; the AppState field is pure projection).

## 12. Gates (AGENTS §10 — stage 2 must run all, stage 3 re-runs)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
cargo deny check          # no dependency changes — expected green
cargo llvm-cov --workspace
```

Plus chunk-specific acceptance (dispatch §2): P1–P8 green as proptests
(256-case house config via `test_support::proptest_config`); snapshot
delta exactly the mechanical addition; mutants-reachable purity (§9.8);
no reminder data in any outbox path (nothing in `outbox.rs`/`push.rs`
was touched — grep-verifiable in review).

## 13. Risks & mitigations

- **Snapshot review discipline**: the one-time insta update is where
  accidental behavior changes would hide — §7.3 makes "only the
  `reminders:` block" an explicit review assertion (stage 2 reviews,
  stage 3 re-diffs).
- **Strategy growth vs existing proptests**: verified no sqlite
  consumer exists (§1); all consumers are fake-backed. Residual risk:
  a proptest finds a *real* interaction (e.g. `empty_batch_is_identity`
  vs compaction) — mitigated by generating compacted states (§9.1); if a
  failure nonetheless appears, treat it as a semantics bug per AGENTS §5
  (test found a bug), not a reason to weaken the strategy.
- **Compaction corner (D6)**: the deviation is property-pinned
  (§9.6); a reviewer diffing against design §9's wording will see the
  recorded reason here.
- **`effective_occurrence` signature drift (D3)**: chunk 2's prompt
  quotes the design's 3-arg form — §14 hands off the correction
  explicitly so the engine agent does not "fix" it backwards.
- **Wire-shape additive field**: `AppState` JSON consumers (daemon IPC
  status paths) see a new `reminders` key — additive; the kebab-case
  envelope contract review belongs to chunk 3 where projections are
  defined. No Rust consumer breaks (all sites compile-driven, §7).
- **Proptest runtime**: P2/P5 re-derive the eligible set per case
  (O(tasks × rules)) — bounded by the strategies' existing size caps
  (≤ ~8 tasks, ≤ ~6 rules); CI time impact expected negligible next to
  the existing merge proptests.

## 14. Handoff notes to chunk 2 (engine scheduler + storage)

- `effective_occurrence` takes `defaults` (D3) — update call sites and
  any copied signature from the design round accordingly.
- Planner work owed from this chunk's scope line (D12):
  `SnoozeReminder { reminder, occurrence, postpone: chrono::Duration }`
  (stamp `now + postpone`, validate `postpone > 0` — keeps chains
  acyclic per D7), mute `StateCommand`, and the `DeleteTask` /
  stack-cascade emission of `DeleteReminderRule` per task rule (D-c;
  the action's own semantics already prune dependents, D4).
- Storage: `0004_reminders.sql` (4 tables, §9 SQL is normative),
  `execute_batch` arms mirroring `apply_actions` exactly (delivery
  upsert = `ON CONFLICT(reminder, occurrence) DO NOTHING`; compaction
  as a same-transaction prune after the action loop), `codec.rs`
  `reminder` text forms (`"default:<task>"`), contract-harness reminder
  clauses, `actions_from_state` reminder arms.
- The notifier channel + `Notification` payload (design §6) convert
  `ReminderFiring`s after `repo.apply` succeeds (durable-then-notify).

## 15. Estimated diff size

~1,000–1,300 added lines across ~12 source files (dominated by
`reminders.rs` ≈ 500 incl. its tests, `persistence.rs` +≈ 170 incl.
tests, `test_support/reminders.rs` ≈ 170) plus 14 regenerated snapshot
files (+≈ 70 lines). Matches the design round's "M" sizing for plan 1.
