# [Plan] Reminders Chunk 2 — Engine Scheduler + Storage

- **Date**: 2026-10-13
- **Kind**: stage-1 implementation plan (chunk 2 of the reminders
  dispatch) — TDD-first, written before any production code.
- **Origin**: design round
  `.artifacts/plans/2026-10-12-reminders-design-plan.md` (§5 scheduler,
  §6 notification, §9 persistence — **normative**; §13 D2/D3/D4/D5) and
  dispatch plan `2026-10-12-reminders-implementation-dispatch-plan.md`
  §1/§3 (shared execution model + code-quality mandate apply).
- **Branch / worktree**: `feat/reminders-engine-storage` in
  `../taskboard-WT-feat-reminders-engine-storage`, off `main` @
  `09cabf4` (**serial** — chunk 1's domain commits `c38cdd9…09cabf4`
  are already on `main`, so no stacking and no rebase dance).
- **Goal**: the engine's reminder tick arm (Option A) with
  durable-then-notify delivery, the `SnoozeReminder`/mute commands, the
  D-c task-delete rule cascade, migration `0004_reminders.sql` with the
  sqlite batch/load extension and codec — every existing suite passing
  **unchanged** with reminders disabled (the auto-advance guard).
- **Status**: ready for stage-2 implementation.

---

## 1. Context & verified facts (anchored; do not re-derive)

1. **Chunk 1 landed on `main`.** `taskboard-domain/src/reminders.rs`
   ships entities (`ReminderRule`/`ReminderDelivery`/`ReminderSnooze`,
   `ReminderRef` + `text_form()`, `ReminderSnoozeKey` with its string
   serde, `ReminderOffset` ≤ 0 by construction, `ReminderDefaults`,
   `ReminderState`, `compact_snooze_chains`), the evaluator
   (`occurrence_of`, `effective_occurrence` — cycle-bounded,
   `due_firings` — sorted by `(occurrence, reminder)`, `overdue_tasks`,
   `due_soon`), and proptests P1–P8. `AppState`/`PersistedState` carry
   `reminders: ReminderState`; the insta snapshots already include it.
2. **The five `PersistenceAction` variants exist with semantics defined
   once in `apply_actions`** (`persistence.rs:198-214`, incl. delivery
   set-union first-write-wins, snooze link replace, rule delete prunes
   dependents, mute toggle, unconditional post-batch snooze compaction).
   `InMemoryRepository` delegates to `apply_actions` — fake parity is
   inherited, not re-implemented (`test_support/memory.rs:61-67`).
3. **The sqlite repo is the parity gap.** `load_inner` hydrates
   `reminders: ReminderState::default()` with an explicit chunk-1
   placeholder comment (`repo.rs:504-509`), and `execute_batch` silently
   drops all five reminder actions (its partition loops match only the
   pre-reminder variants). Migrations shipped: `0001_init.sql`,
   `0002_board_title_clock.sql`, `0003_outbox_board_id.sql` under
   `crates/taskboard-storage-sqlite/migrations/` — **0004 is the next
   free number** (the design's numbering assumption holds; no renumber).
4. **The engine loop** (`taskboard-state/src/actor.rs:271-374`):
   sequential `biased` select! (commands → reports → system), drain mode
   after `SystemEvent::Shutdown` (declines commands, ingests reports
   until the channel closes), edge-triggered `StorageFailed` via the
   `storage_failing` flag, per-message `clock.now()` read, and the
   apply → interpret → publish discipline (`execute_command`,
   `ingest_sync_report`, `go_offline` are the three precedent shapes).
   Loop exit is checked at the **top** of the loop before select!, so a
   live timer can never zombie the loop.
5. **The timer-arm precedent** is the sync actor
   (`taskboard-sync-nextcloud/src/actor.rs:181-266`): `biased` select!
   with `() = tokio::time::sleep_until(deadline) => { …; deadline =
   reschedule(…) }`, first cycle immediate (`Instant::now()`), under
   `tokio::time::pause()`/`advance()` in tests.
6. **`plan_command`** (`domain/command.rs:202`) is the command
   catalogue: pure, total, clock/id-injected, edit-style intents reject
   against tombstones while terminal intents no-op. `DeleteTask`/`DeleteStack`
   exist and do **not** yet cascade reminder rules — D-c is unimplemented
   (chunk 1's scope was the model + actions only). `StateCommand` has no
   reminder variants yet.
7. **Paused-time engine tests exist today** and are the auto-advance
   hazard surface: `taskboard-state/tests/proptest.rs:430`
   (`drive_sequence` runs a live engine on a `start_paused` runtime and
   compares memory ≡ disk between commands),
   `taskboard-sync-nextcloud/tests/e2e_hermetic.rs` +
   `tests/common/sync_pair.rs` (paused engines next to the sync actor's
   own `sleep_until` arm). None of them use `start_paused` in
   `tests/engine.rs` (plain `#[tokio::test]` + the `eventually` poll
   helper with timeout guards).
8. **`spawn_state_engine` call sites** (all must compile after the
   signature change): `tests/{engine.rs:128,637, snapshots.rs:59,
   sqlite_smoke.rs:47, proptest.rs:445}` and the doctest in
   `taskboard-state/src/lib.rs:39` (doc-tested by the gate suite);
   `taskboard-sync-nextcloud/tests/common/sync_pair.rs:45` and
   `tests/e2e_hermetic.rs:68`; `taskboard-app/src/bootstrap.rs:395`.
9. **Storage test patterns**: the migration upgrade tests live in
   `connect.rs` `#[cfg(test)]` (`migration_0002_upgrades_a_pre_clock_database`,
   `migration_0003_backfills_legacy_rename_board_rows` — simulate an
   N−1-era database by running the shipped migrations, stripping the
   newest artifact + bookkeeping row, reopening); the shared repository
   contract is `test_support/contract.rs::
   assert_task_repository_contract_async`, run against both
   `InMemoryRepository` (in-domain) and the sqlite adapter
   (`storage-sqlite/tests/contract.rs`); sqlx offline cache `.sqlx/`
   (18 files) must be refreshed via `scripts/sqlx-prepare.sh` and
   committed **with** the query changes (sqlx-cli 0.9).
10. **Zero new dependencies** in this chunk: everything used (tokio
    select/sleep_until/mpsc, chrono, uuid, sqlx, insta, proptest,
    tempfile) is already in the workspace.

## 2. Scope & non-goals

**In scope** (dispatch §3): the engine's fourth select! arm
(`sleep_until(next_tick)`, re-arm, drain-aware, biased order commands →
reports → system → timer), the tick path (one `clock.now()` read → pure
`due_firings` → `RecordReminderDelivery` batch → apply → interpret →
publish → notify), the `SnoozeReminder` + mute `StateCommand`s with
domain planning, the D-c delete cascade, `ReminderEngineConfig`
injection + the default-off test-helper policy + the mandatory
auto-advance guard test, the notifier `mpsc::Sender<Notification>` +
spawn-signature change, migration `0004_reminders.sql`, sqlite
batch/load + codec, contract-harness extension, pause/advance engine
tests.

**Out of scope** (chunk 3 `feat/reminders-app-wiring`): figment
`[reminders]` table, bootstrap wiring of a live `LogSink`/notifier
consumer, `tasks remind` CLI verbs, rule-management `StateCommand`s
(`SetTaskReminder`-style) and their `CommandOutcome` receipts, the
backlog ✅. **Out of scope** (plan 5): desktop toasts, the interactive
snooze/dismiss UI. No outbox/`LocalOp` changes (D-a: v1 is local-only).

## 3. Recorded decisions (every open question resolved)

Deviations from the design round would need a recorded reason; none of
the below deviates — the few places the design was silent are resolved
here and marked *silence filled*.

- **C1 — branching**: serial off `main` (fact 0/§1); chunk 1 is merged.
- **C2 — timer mechanics** (*design §5.2 silence filled on first tick
  and drain*):
  - The arm is `() = tokio::time::sleep_until(next_tick), if
    timer_armed =>` — **last** in the `biased` order. With the
    precondition false, select! neither constructs nor polls the sleep
    future: a disabled engine performs zero timer work (keeps the
    "Miri-clean, no timers" claim of the existing suites true, fact 7).
  - `next_tick` starts at `tokio::time::Instant::now()` — the **first
    tick is immediate** (sync-actor precedent, fact 5), so an
    undelivered reminder from before a reboot fires on the next boot's
    first tick exactly as design §5.2 states. Re-arm after every wake:
    `next_tick = tokio::time::Instant::now() + cfg.tick` (fixed
    cadence; due-date reminders need no backoff).
  - **Drain disarms the timer**: on entering drain mode
    (`SystemEvent::Shutdown`) set `timer_armed = false`. "Timer wakes
    during drain are ignored" is implemented as *never woken*, which is
    strictly simpler and also removes the timer from the paused-time
    drain window. The undelivered reminder survives to the next boot
    (occurrence ≤ now, no delivery row).
  - Loop exit is unchanged (fact 4): an armed timer never prevents the
    top-of-loop break when all three inbox sources close.
- **C3 — `ReminderEngineConfig`** lives in `taskboard-state` (the
  `SyncActorConfig` precedent — the crate that owns the loop owns its
  knobs):

  ```rust
  /// Reminder scheduler knobs; chunk 3 wires them from figment's
  /// `[reminders]` table. Tests inject tiny/paused values.
  #[derive(Debug, Clone)]
  pub struct ReminderEngineConfig {
      /// `false` ⇒ the tick arm is never armed (the default-off
      /// test-helper policy; auto-advance guard).
      pub enabled: bool,
      /// Fixed tick cadence (production default 30 s, §5.4).
      pub tick: std::time::Duration,
      /// Injected per-device defaults for the implicit reminder.
      pub defaults: taskboard_domain::ReminderDefaults,
  }

  impl ReminderEngineConfig {
      /// The off state used by every pre-existing test helper and the
      /// interim bootstrap wiring. The `defaults` value is irrelevant
      /// while disabled; production defaults (5 m before due) are
      /// figment's to encode in chunk 3 — deliberately not encoded
      /// here so this type carries no hidden production policy.
      #[must_use]
      pub const fn disabled() -> Self { /* enabled: false, tick: 30 s, defaults: ReminderDefaults::default() */ }
  }
  ```

  **No `Default` impl** — explicit construction everywhere, so no call
  site can silently inherit an enabled scheduler.
- **C4 — spawn signature**: two new **trailing** parameters,
  `notifier: mpsc::Sender<Notification>` then `reminder_cfg:
  ReminderEngineConfig` (design §5.2 + §6; `#[allow(too_many_arguments)]`
  already carries "the seams ARE the constructor"). `EngineCommand`
  gains **no** variant (Option A is an internal arm, not an inbox
  message — Option B's cost recorded in §5.1). `EngineSignal` and
  `SystemEvent` unchanged (design §8). `EngineParts` grows
  `notifier` + `reminders: ReminderEngineConfig` so the tick path takes
  a seams tuple like `Seams<'a>` today (`actor.rs:387-394`).
- **C5 — interim bootstrap wiring**: `bootstrap.rs` creates
  `mpsc::channel::<Notification>(64)`, passes the sender, and drops the
  receiver immediately — a closed channel is the sanctioned `debug!`
  no-op (fact: `forward_sync_now` precedent), and chunk 3 replaces the
  dropped receiver with the `LogSink` consumer. A comment states that
  constraint. This keeps the workspace green without pulling chunk-3
  scope forward.
- **C6 — tick-path decomposition** (one level of abstraction per body,
  the dispatch's code-quality mandate):

  - `EngineCore` gains two read accessors (the `outbox()`/`validators()`
    precedent — zero-clone borrows of private state):
    `due_reminder_firings(&self, defaults: &ReminderDefaults, now:
    DateTime<Utc>) -> Vec<ReminderFiring>` (delegates to the domain
    evaluator over `self.state.tasks`/`self.state.reminders` — the
    brain stays in the domain, core stays a state capsule) and
    `tasks(&self) -> &BTreeMap<TaskId, Task>` (notification title
    lookup).
  - `reminder_tick(...) -> impl Future` — the **orchestrator**, shaped
    like `ingest_sync_report`: (1) one `clock.now()` read; (2)
    `core.due_reminder_firings(...)`; (3) empty ⇒ return (a quiet tick
    publishes and signals nothing — the accepted-no-op discipline); (4)
    `delivery_batch(&firings, now)` (implement level: map each firing
    to `RecordReminderDelivery(ReminderDelivery { reminder, occurrence,
    delivered_at: now })`); (5) `repo.apply` — on error, run the shared
    edge-triggered `StorageFailed` path and return (**nothing fires on
    a failed batch** — the delivery marker is the notification permit);
    (6) `core.interpret` + `publish` (the `ingest_sync_report` shape);
    (7) `send_notifications` **after** publish (durable-then-notify,
    design §6 ordering guarantee — review greps this ordering).
  - `send_notifications(notifier, payloads)` — implement level: one
    `notifier.send(payload).await` per notification; a closed channel
    is `debug!` and continues (headless configs are legal). **`send`,
    not `try_send`**: a full channel backpressures the tick — the
    correct failure mode for outbound notifications (drop-on-full is
    a lost toast by choice; backpressure is at-most-once by
    construction). Channel capacity 64 (human-scale bursts on a kiosk
    reboot; the log sink drains fast).
  - The storage-failure edge trigger is **shared** with the command
    path (the `storage_failing` flag + `SystemEvent::StorageFailed`
    broadcast, one epoch semantics): extract the flag/broadcast pair
    into a named private helper used by both `execute_command` and
    `reminder_tick`, so a dying disk during ticks surfaces the same
    banner signal as a failing command write (*design silence filled*:
    §5.1 says "nothing fires"; the engine's existing producer duty for
    `StorageFailed` extends naturally to this second write path).
- **C7 — `Notification` payload** (design §6 verbatim) is added to
  domain `reminders.rs` (chunk 1 did not include it):

  ```rust
  /// Outbound reminder notification: the payload a sink delivers.
  /// Title is a snapshot at fire time (the task may be edited later).
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Notification {
      pub reminder: ReminderRef,
      pub task: TaskId,
      pub title: String,
      pub occurrence: DateTime<Utc>,
  }

  /// Pure payload assembly: firing triples + title snapshots; a firing
  /// whose task is missing is skipped (total; unreachable through the
  /// engine's sequential tick, which evaluates and assembles in one
  /// message).
  #[must_use]
  pub fn notifications_for(
      firings: &[ReminderFiring],
      tasks: &BTreeMap<TaskId, Task>,
  ) -> Vec<Notification>;
  ```

  Re-exported from the domain crate root (`lib.rs` `pub use reminders::…`
  list). The `NotificationSink` port itself stays chunk-3+ (app crate);
  chunk 2 ships only the payload + channel seam.
- **C8 — `SnoozeReminder` command** (design §4.4 D-b verbatim shape):

  ```rust
  /// Snooze a fired reminder occurrence: reschedule it to
  /// `now + postpone` (engine-stamped via the injected clock).
  SnoozeReminder {
      reminder: ReminderRef,
      occurrence: DateTime<Utc>,
      postpone: chrono::TimeDelta,
  },
  ```

  Planning semantics (`plan_command` arm):
  - **Guards**: `Rule(id)` whose rule no longer exists ⇒ new
    `CommandError::UnknownReminderRule(ReminderRuleId)`; the ref's task
    unknown/deleted ⇒ existing `UnknownTask`/`TaskDeleted` (edit-style
    intent — snoozing a dead task's reminder is not
    meaningless-redundant). `postpone` ≤ 0 ⇒ new
    `CommandError::InvalidSnoozePostpone` (the `ReminderOffset::before`
    stance: domain-meaningful sign/positivity validated at the seam).
  - **No effective-occurrence check**: the occurrence is opaque data
    keyed into the snooze map. A stale link (duedate moved between fire
    and snooze) is *inert by map semantics* — the walk starts at the
    new base and never reaches it — and pre-emptive snoozes (link
    before first fire) behave exactly like post-fire snoozes. Checking
    would be racy by nature and buy nothing; compaction bounds the
    settled links (chunk 1, property-pinned).
  - **Stamp**: `rescheduled_to = now.checked_add_signed(postpone)`
    saturated to `DateTime::<Utc>::MAX_UTC` on overflow — the chunk-1
    edge policy (total arithmetic; saturation documented in-test).
  - **Actions/outcome**: exactly
    `[RecordReminderSnooze(ReminderSnooze { reminder, occurrence,
    rescheduled_to })]`, outcome `Applied`. No new `CommandOutcome`
    variant — the dispatch assigns outcome additions to chunk 3, and
    snooze has no CLI surface in v1; plan 5's interactive round can add
    a receipt when something consumes it.
  - `chrono::TimeDelta` rides `StateCommand` serde under the already
    enabled chrono `serde` feature (the `DateTime` fields of
    `TaskChanges` prove the gate).
- **C9 — mute command**:

  ```rust
  /// Mute (`true`) or unmute (`false`) all firings of a task. The
  /// overdue banner is unaffected (board truth, not nagging policy).
  SetTaskRemindersMuted { task: TaskId, muted: bool },
  ```

  Planning: `live_task` guard (edit-style), membership-equal repeat ⇒
  accepted no-op (`NoOp`, zero actions — the `SetTaskDone` idiom),
  otherwise `[SetTaskRemindersMuted(task, muted)]` with outcome
  `Applied` (chunk 3 introduces `CommandOutcome::ReminderMuted` with
  its CLI verb; not pre-added here).
- **C10 — the D-c delete cascade lands in this chunk** (it is command
  planning, so it could not land in chunk 1 and is not CLI, so it will
  not land in chunk 3 — this is the only chunk that touches
  `plan_command`): `DeleteTask` appends
  `DeleteReminderRule(rule.id)` for every rule of the task (rule ids
  are known — no id-generator involvement), and the `DeleteStack`
  per-task cascade does the same for each cascaded task's rules —
  every path that tombstones a task prunes its rules, or stack-delete
  would leave exactly the orphans D-c exists to prevent. **Not pruned**
  (recorded): the task's `Default`-ref deliveries/snooze links and its
  `muted` entry — inert while the task is dead (the evaluator requires
  live+open), exactly D-c's stated scope; age-based pruning stays the
  backloged housekeeping entry.
- **C11 — migration `0004_reminders.sql`** at
  `crates/taskboard-storage-sqlite/migrations/`, the design §9 SQL
  verbatim (fact 3 confirms 0004 is free): four `CREATE TABLE`s —
  `reminder_rules` (`rule_id TEXT PRIMARY KEY`, `task_id TEXT NOT NULL
  REFERENCES tasks(task_id)`, `offset_seconds INTEGER NOT NULL`,
  `created_at TEXT NOT NULL`), `reminder_deliveries` (`reminder`,
  `occurrence`, `delivered_at`, `PRIMARY KEY (reminder, occurrence)`),
  `reminder_snoozes` (`reminder`, `occurrence`, `rescheduled_to`,
  `PRIMARY KEY (reminder, occurrence)`), `reminder_muted`
  (`task_id TEXT PRIMARY KEY`). No FK on deliveries/snoozes/muted
  (rule deletes prune first); **no CHECK constraints** (the domain
  newtype already guarantees `offset ≤ 0`; storage executes semantics,
  it does not re-legislate them); no extra indexes (the PKs are the
  lookup paths at kiosk scale). Additive only — ADR 0005 policy.
  **Offset granularity is whole seconds** (`offset_seconds`):
  `ReminderOffset::as_duration().num_seconds()` truncates sub-second
  offsets toward zero — accepted and documented (due-date reminders
  are minute-scale; config/CLI durations in chunk 3 are
  seconds-granularity).
- **C12 — codec + row mapping** (`storage-sqlite/src/codec.rs`,
  `repo.rs`):
  - `ReminderRef` text codec has **one definition site**:
    `ReminderRef::text_form()` already exists (chunk 1); chunk 2 makes
    the private `parse_ref` public as
    `ReminderRef::parse_text_form(&str) -> Option<ReminderRef>` (used
    by `ReminderSnoozeKey`'s serde impl *and* the storage codec —
    deletion of the duplication, not addition). The codec wraps it:
    `None ⇒ CodecError`.
  - Timestamps (`occurrence`, `delivered_at`, `rescheduled_to`,
    `created_at`) are rfc3339 `TEXT`, read through the existing
    `query_as!` type-override pattern (`"occurrence: DateTime<Utc>"`,
    the `remote_seen` precedent); written with `.to_rfc3339()`.
  - `load_inner`: four new SELECTs inside the existing read
    transaction, assembling `ReminderState` (rules keyed by id;
    deliveries into the set; snoozes into the map via
    `ReminderSnoozeKey`; muted into the set) — replacing the chunk-1
    `default()` placeholder and its comment. Decode failures (bad
    uuid/ref text, `ReminderOffset::before` rejecting a stored positive
    or unrepresentable `offset_seconds` via `try_seconds` → `None`)
    map to `RepositoryError::Corrupted` — the domain constructor *is*
    the integrity check.
  - `execute_batch`: five new partitions appended after the existing
    ones (tasks precede rules — the only FK), preserving batch order
    within each: rule upsert (`ON CONFLICT(rule_id) DO UPDATE`);
    `DeleteReminderRule` = `DELETE` the rule row **plus** `DELETE` its
    deliveries and snooze links `WHERE reminder =
    ReminderRef::Rule(id).text_form()` (mirrors `apply_actions`
    D-c/cascade semantics; parity pinned by the contract harness);
    delivery insert `ON CONFLICT(reminder, occurrence) DO NOTHING`
    (**first-write-wins = the in-memory set-union**, the
    crash-between-apply-and-notify replay stays a no-op in SQL too);
    snooze upsert `ON CONFLICT(reminder, occurrence) DO UPDATE SET
    rescheduled_to = excluded.rescheduled_to` (replace-per-key); muted
    `INSERT … DO NOTHING` / `DELETE`.
  - Note (recorded, not a change): sqlite does **not** execute
    `apply_actions` — its parity is enforced by the shared contract
    harness + roundtrip suite, exactly like every other action today.
    Snooze compaction needs no SQL mirror: compaction is a post-batch
    *derivation* in `apply_actions`; on the sqlite side the pruned rows
    are simply never written again (deleting them eagerly would be a
    second, drifting definition — a `load()`-side normalization is
    equally redundant because the engine's interpret pass compacts
    memory after every batch it persists, and fresh boots hydrate
    settled links that the next tick batch settles identically). If a
    divergence ever matters, the contract harness fails first.
- **C13 — contract harness extension**: `assert_task_repository_contract_async`
  grows a reminder section (rule upsert roundtrip; delivery replay is a
  no-op — reload shows the *first* `delivered_at`; snooze link replace;
  mute toggle; rule delete prunes its deliveries/snoozes but not other
  reminders' rows). Both `InMemoryRepository` and the sqlite adapter
  run it unchanged from their existing entry points — that is the
  fake/sqlite parity proof for this chunk.
- **C14 — default-off test-helper policy + auto-advance guard** (design
  §5.3, mandatory):
  - Every existing engine-spawning helper gains the two seams with the
    off state: `tests/engine.rs` `Harness`/`spawn` (reminder tests opt
    in via a `Spawner` field or a sibling constructor),
    `tests/proptest.rs` `drive_sequence`, `tests/snapshots.rs`,
    `tests/sqlite_smoke.rs`, `tests/common/mod.rs` if it grows a
    spawner, the sync crate's `sync_pair.rs`/`e2e_hermetic.rs`, the
    `lib.rs` doctest, `bootstrap.rs` (C5). All pass
    `ReminderEngineConfig::disabled()`; the harness keeps a notifier
    receiver it exposes to tests.
  - **Guard test** (`tests/reminders.rs`, the mandatory one): spawn an
    engine with reminders **disabled** over a seeded past-due task on a
    `start_paused` runtime; `tokio::time::advance` far beyond many
    tick intervals; assert zero deliveries in the shared state, zero
    notifications on the (held) receiver, and zero `EngineSignal`s.
    This pins the invariant the paused suites rely on (fact 7): with
    the arm disabled, no timer future exists, so auto-advance has
    nothing to fire. If anyone ever flips a helper default, this test
    fails loudly. The stage-3 review additionally diffs every touched
    pre-existing test for byte-identity (dispatch hard rule).
  - New reminder tests carry `#[cfg_attr(miri, ignore)]` (they use the
    timer wheel; the existing suites' "Miri-clean, no timers" header
    stays true — see C2).

## 4. Workstream A — domain (TDD order)

Files: `messages.rs`, `command.rs`, `reminders.rs`, `lib.rs` re-exports,
`test_support/{messages.rs, contract.rs}`. All pure; MIT OR Apache-2.0
headers; no new deps.

1. **`StateCommand` variants** (C8/C9) + serde roundtrip entries in the
   existing `test_support::state_command_strategy` (snooze: existing
   `reminder_ref_strategy` + `timestamp_strategy` + a bounded positive
   `TimeDelta`; mute: `task_id_strategy` + bool). The `plan_command`
   totality/outbox-independence proptests then exercise the new arms
   for free.
2. **`plan_command` arms + `CommandError` variants** (C8/C9/C10), tests
   first (§6 catalogue T1–T7).
3. **`Notification` + `notifications_for` + `ReminderRef::parse_text_form`**
   (C7/C12): unit + serde roundtrip; `notifications_for` property
   (payload triples equal the firings; titles equal the task map;
   missing task ⇒ skipped).
4. **Contract harness reminder section** (C13).

## 5. Workstream B — engine (taskboard-state)

Files: `actor.rs`, `core.rs`, `lib.rs` (export `ReminderEngineConfig`;
doctest), `tests/engine.rs` (harness), new `tests/reminders.rs`,
mechanical two-arg additions in `tests/{proptest,snapshots,sqlite_smoke}.rs`,
`taskboard-sync-nextcloud/tests/{common/sync_pair.rs, e2e_hermetic.rs}`,
`taskboard-app/src/bootstrap.rs` (C5).

1. `ReminderEngineConfig` (C3) + `EngineCore` accessors (C6) with unit
   tests in `core.rs` (accessor delegates; zero-clone borrow).
2. Spawn signature (C4) + `EngineParts` growth + all call sites (fact
   8). Gates must be green **before** the arm exists — this commit is
   compile-only observable.
3. The tick arm + `reminder_tick` + `delivery_batch` +
   `send_notifications` + the shared `StorageFailed` edge-trigger helper
   (C2/C6), in the `run_loop` shape:

  ```rust
  // select! arm, last under biased:
  () = tokio::time::sleep_until(next_tick), if timer_armed => {
      reminder_tick(/* core, shared, signals, seams…, &mut storage_failing */).await;
      next_tick = tokio::time::Instant::now() + reminder_cfg.tick;
  }
  ```

   with `let mut next_tick = tokio::time::Instant::now(); let
   mut timer_armed = reminder_cfg.enabled;` before the loop and
   `timer_armed = false` on entering drain. `clippy::too_many_lines`
   allowance comment extended only if the loop truly grows past the
   existing allowance's spirit — prefer pushing detail into the named
   helpers over widening allowances.
4. `tests/reminders.rs` (§6 catalogue R1–R9), all
   `#[tokio::test(start_paused = true)]` + `#[cfg_attr(miri, ignore)]`,
   driving the engine through `EngineHandle` and the harness's notifier
   receiver. Clock lockstep helper (design §15): `advance_both(clock,
   secs)` = `FixedClock::advance` + `tokio::time::advance` — the
   injected clock decides *what* fires, tokio time decides *when* the
   arm wakes; tests move both together and assert terminal state via
   the existing `eventually` poll pattern (never wall-clock, never
   intermediates).

## 6. Test catalogue (one behavior, one test, lowest layer)

**Domain `command.rs` (T-series, pure unit — semantics live here):**

- T1 snooze plans the engine-stamped link: actions == exactly one
  `RecordReminderSnooze` with `rescheduled_to == now + postpone`;
  outcome `Applied`.
- T2 snooze rejections: unknown rule ⇒ `UnknownReminderRule`; unknown
  task ⇒ `UnknownTask`; deleted task ⇒ `TaskDeleted`; `postpone` 0 and
  negative ⇒ `InvalidSnoozePostpone` (typed `matches!`, no text).
- T3 snooze saturates an unrepresentable target (`postpone` huge ⇒
  `DateTime::<Utc>::MAX_UTC`, no panic).
- T4 mute: set/unset toggle actions; membership-equal repeat ⇒ `NoOp`
  with zero actions; unknown/deleted task rejections.
- T5 `DeleteTask` emits `DeleteReminderRule` per task rule (plus the
  existing actions); delete-repeat stays a zero-action no-op.
- T6 `DeleteStack`'s cascaded tasks' rules are deleted too; other
  tasks' rules untouched.
- T7 (strategy-driven, free): `plan_command` totality +
  outbox-independence proptests absorb the new variants via the
  extended `state_command_strategy`.

**Domain `reminders.rs`:** `Notification` serde roundtrip;
`notifications_for` unit + property (C7); `parse_text_form` roundtrip +
garbage rejection (both ref flavors).

**Storage (S-series):**

- S1 `migration_0004_upgrades_a_pre_0004_database` (in `connect.rs`
  tests, the 0002/0003 pattern): run shipped migrations, `DROP TABLE`
  the four reminder tables + delete the version-4 bookkeeping row, seed
  a board/task row, reopen ⇒ 0004 re-applies cleanly; `load()` returns
  the intact entities and a default-shaped `reminders` section.
- S2 contract harness reminder section green against
  `InMemoryRepository` and sqlite (both `:memory:` and file, via the
  existing `tests/contract.rs` entry points — C13).
- S3 codec units: ref text roundtrip both flavors + garbage;
  positive/`None`-producing `offset_seconds` decode ⇒ `Corrupted`.
- S4 existing roundtrip/actor suites stay green (no edits beyond what
  the harness extension implies).

**Engine (`tests/reminders.rs`, R-series — routing + timing only;
semantics stay in the domain):**

- R1 **fire**: seeded task with past duedate, enabled engine (tiny
  tick), paused runtime — the immediate first tick fires the implicit
  default reminder: exactly one delivery in
  `shared_state().reminders.deliveries`, one `Notification` on the
  harness receiver (payload triple + title snapshot asserted), one
  `StateUpdated`.
- R2 **at-most-once**: after R1's fire, advance across several ticks
  (lockstep helper; auto-advance supplies more) ⇒ still exactly one
  delivery, one notification.
- R3 **snooze re-fire**: `execute(SnoozeReminder { .., postpone: 10 m })`
  ⇒ `Ok(Applied)`, link visible in state; advance to just before the
  target ⇒ no second fire; advance past it ⇒ re-fire with occurrence ==
  the target, second delivery + notification; settled thereafter.
- R4 **drain silence**: enabled engine + due task; send `Shutdown`;
  `wait_draining`; advance far past due ⇒ zero deliveries/
  notifications (the timer was disarmed, not merely ignored).
- R5 **closed notifier channel**: drop the receiver before the tick ⇒
  the delivery still lands and the engine keeps serving commands
  (closed channel = `debug!` no-op, marker-first policy).
- R6 **storage failure fires nothing**: `FlakyRepository`(1) + due task
  ⇒ after the tick: no delivery in state, **no notification sent**, one
  `SystemEvent::StorageFailed` broadcast (edge trigger), engine alive;
  the next tick (injector exhausted) fires normally. This pins
  durable-then-notify end to end.
- R7 **quiet tick invisible**: no due reminders ⇒ advance ⇒ no
  `StateUpdated` (a quiet tick is a no-op, not a repaint).
- R8 **auto-advance guard** (C14, mandatory).
- R9 **mute routing**: `SetTaskRemindersMuted { muted: true }` accepted,
  membership visible in state; advance past due ⇒ no firing. (Banner
  unaffectedness is domain-pinned P8/p4 — not re-asserted here.)

No insta snapshot changes are expected: `AppState`'s shape is unchanged
since chunk 1 and no snapshot scenario dispatches the new commands. Any
snapshot diff in this chunk is a review finding, not churn to accept.

## 7. Implementation & commit order (each standalone, gates green)

1. `feat(domain): snooze and mute commands with task-delete rule cascade`
   — messages, command planning + errors, strategy extension, T1–T7.
2. `feat(domain): reminder notification payload` — `Notification`,
   `notifications_for`, `parse_text_form`, tests, re-export.
3. `feat(storage): reminder tables with repository batch and load` —
   migration 0004, codec, row types + load + batch partitions, contract
   extension, S1–S3, `.sqlx/` refreshed via `scripts/sqlx-prepare.sh`
   and committed **in this commit** (never separately).
4. `feat(state): reminder tick arm with notifier channel` — config,
   core accessors, spawn signature + every call site (incl. interim
   bootstrap + doctest), the arm + tick path, tests/reminders.rs R1–R9.

Commits 1–2 precede 4 (the engine consumes the payload and commands);
commit 3 is independent of 4 but ordered before it so the riskiest
change (the arm) lands last against a settled world. Stage 2 runs the
full gate suite (AGENTS §10) at every commit; stage 3 re-runs, reviews
the mandate (SRP: engine orchestrates, domain legislates, storage
persists, the notifier only delivers; single abstraction level per
body), and ships.

## 8. Risks & mitigations

- **Auto-advance interaction (the headline risk)**: once any paused
  test enables the arm, tokio's auto-advance turns idle awaits into
  timer storms — publishes would interleave with `drive_sequence`'s
  memory ≡ disk comparisons (fact 7) and the sync hermetic suites.
  Mitigations, layered: (a) the arm exists only when `enabled` — select!
  preconditions skip future construction entirely, so disabled engines
  perform zero timer work; (b) **every** pre-existing helper passes
  `disabled()` (C14 list, all 9+ call sites); (c) R8 pins it; (d) drain
  disarms the timer so shutdown windows cannot tick either; (e) stage-3
  diffs touched pre-existing tests byte-for-byte. Residual risk: a
  *future* helper forgetting the policy — R8 does not catch new
  helpers, only regressions in the pinned ones; the review checklist
  (dispatch §3) greps for enabled-by-default spawns.
- **Two-clock divergence in tests** (tokio instant vs injected
  `FixedClock`): the arm wakes on tokio time but evaluates on the
  injected clock; forgetting to move both makes tests flaky-by-design.
  Mitigation: the `advance_both` lockstep helper is the only sanctioned
  way tests move time (§5.4); R1 asserts the payload's occurrence came
  from the injected clock's view.
- **Durable-then-notify ordering**: a regression that notifies before
  `repo.apply` breaks at-most-once (crash ⇒ re-nag). Mitigation: R6
  pins it behaviorally; the review greps the tick path ordering as a
  first-class finding (dispatch prompt 2.3).
- **sqlite semantics drift** (hand-written SQL vs `apply_actions`):
  mitigated by the shared contract harness section (C13) executed
  against both implementations — the established parity mechanism.
- **Migration immutability/numbering**: 0004 verified free (fact 3);
  the migration is additive-only; S1 proves the upgrade path over a
  stripped 0003-era database.
- **`clippy::too_many_lines` on `run_loop`**: the arm adds ~10 lines to
  an already-allowed loop. Mitigation: the arm body is one call +
  re-arm; all detail lives in `reminder_tick` and its implement-level
  helpers (C6). Do not widen the allowance.
- **serde of `chrono::TimeDelta`**: covered by the already-enabled
  chrono `serde` feature; T-series + roundtrip tests pin it.
- **Miri**: new timer-using tests are `miri`-gated; no existing
  Miri-clean claim is touched (C2/C14).

## 9. Acceptance checklist (chunk 2 done when)

- [ ] Ticks fire due reminders at-most-once per occurrence, delivery
      marker persisted **before** the notification send (R1/R2/R6).
- [ ] Snooze reschedules through the same evaluator (R3 + T1–T3).
- [ ] Drain ignores timer wakes by never waking them (R4).
- [ ] The pre-existing engine/sync/app suites pass **unchanged** with
      reminders disabled; R8 exists and passes (auto-advance guard).
- [ ] Migration 0004 additive with a tested upgrade path (S1); next
      free number used; `.sqlx/` cache committed with the queries.
- [ ] Fake + sqlite parity rides the shared contract harness (S2); no
      duplicated action semantics outside `apply_actions` + the
      mirrored SQL (grep-checked in review).
- [ ] No `tokio::time::sleep` in any new test; time moves only via
      `pause`/`advance` + the injected clock (lockstep helper).
- [ ] No new dependencies; SPDX headers on new files
      (`taskboard-state`/`taskboard-storage-sqlite`: MIT OR Apache-2.0);
      `#![forbid(unsafe_code)]` untouched; full AGENTS §10 gate suite
      green (fmt, clippy `-D warnings`, nextest, doc-tests, deny,
      llvm-cov).

## 10. Estimated diff size

~1,500–1,800 added lines across ~16 source files:

| Area | Files | Est. lines |
|---|---|---|
| domain commands + notification | `messages.rs`, `command.rs`, `reminders.rs`, `lib.rs`, `test_support/{messages,contract}.rs` | ~550 (≈300 tests) |
| engine | `actor.rs`, `core.rs`, `lib.rs`, `tests/engine.rs`, new `tests/reminders.rs`, 3 mechanical test files, 2 sync-crate test files, `bootstrap.rs` | ~700 (≈450 tests) |
| storage | `migrations/0004_reminders.sql`, `repo.rs`, `codec.rs`, `connect.rs` tests | ~400 (≈150 tests) |
| sqlx cache | `.sqlx/*` (≈10 new query files) | generated |

Medium chunk, dominated by tests — consistent with the dispatch's "M"
estimate for chunk 2.
