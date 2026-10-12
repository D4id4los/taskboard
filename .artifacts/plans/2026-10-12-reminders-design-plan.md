# [Plan] Reminders & Notifications — Design Round (WI-I)

- **Date**: 2026-10-12 (round 2: user direction folded in)
- **Kind**: design round (stage 1 of WI-I) — **no production code in this
  commit**. Produces the design + draft ADR the user reviews before any
  reminder implementation is planned. Stages 2–3 of WI-I (formalize ADR
  0013 + docs amendments, then review) are unblocked by the approved
  direction below.
- **Origin**: backlog entry "[2026-10-08] Reminders/notifications
  subsystem" (backlog line 356); dispatch plan §4 WI-I; M1-exit pre-GUI
  report §6 ("run the design round **before** the Phase 7 UI plan is
  finalized so the UI is not reshaped after reminders land").
- **Target**: this document only. Future implementation targets (§14):
  `taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-app`; Phase 7 UI consumes the surface defined in §8.
- **Branch / worktree**: `docs/reminders-design` in
  `../taskboard-WT-docs-reminders-design`, off `main` @ `9b46e2e`.
- **Goal**: a decision-ready design for the reminder half of taskboard's
  identity ("task management **and reminder** system", AGENTS §1) that
  (a) keeps all scheduling logic pure and proptest-able in the domain,
  (b) reuses the state engine's apply→interpret→publish loop instead of
  adding a second state owner, (c) defines exactly what the Phase 7 UI
  reads, and (d) records the resolved decisions (§13) and the deferred
  properties (backlog) so stage 2 can formalize the ADR.
- **Status**: DIRECTION APPROVED 2026-10-12 (user review, round 2) —
  §13's decisions are resolved and recorded. Round-2 amendments:
  reminder rule/delivery sync is a **desired, backloged property** with
  a sync-ready model (§4.4 D-a); snooze space is reserved in the
  occurrence model (§4.4 D-b); the implicit default reminder is 5 m
  before due, user-configurable from day one (§13 D5).

---

## 1. Context & verified facts (anchored, do not re-derive)

1. **No reminder design exists anywhere today.** No due-date watching,
   notification scheduling, or banner path appears in
   `docs/architecture.org`, any ADR, or any plan (backlog entry, line
   356). The only reminder-adjacent fact in the codebase:
2. **`Task.duedate` already exists and syncs.**
   `crates/taskboard-domain/src/entities.rs:166` —
   `duedate: Option<DateTime<Utc>>`, per-field LWW clock
   (`TaskClocks.duedate`, entities.rs:55), edited via
   `StateCommand::UpdateTask`'s double-`Option` changeset
   (`messages.rs:152-160`), pushed to Deck as `LocalOp::UpdateTask`
   (`outbox.rs:26`). Due dates arrive from Deck sync by adoption with
   the same merge rules as every other field.
3. **The time seam is established.** `Clock` trait, domain
   `clock.rs:17-20` — `now() -> DateTime<Utc>`, `Send + Sync`, no
   async; "merge functions never read the clock: time is always *data*
   passed into them". The state engine reads the clock **once per
   message** (`actor.rs:406` `let now = clock.now();`), and that stamp
   flows into every transition. Production uses `SystemClock`; tests
   inject fixed/advancing clocks.
4. **The engine is the single state owner and writer.** Sequential
   `tokio::select!` loop (`biased`: commands → sync reports → system
   events; `taskboard-state/src/actor.rs:279-371`). Per mutating
   message: plan (pure, domain `plan_command`) → `repo.apply(batch)` →
   only on success `core.interpret(batch)` (the domain's shared
   `apply_actions`, `persistence.rs:206`) → publish
   `ArcSwap<AppState>` + `EngineSignal::StateUpdated`
   (`actor.rs:378-385`). Memory ≡ disk by construction; storage
   failure leaves memory unchanged (ADR 0006).
5. **Persistence is a closed additive enum.**
   `PersistenceAction` (`persistence.rs:170-191`): entity upserts,
   `EnqueueOp`/`CompleteOp`/`FailOp` (outbox), `UpsertValidators`,
   `UpsertSyncStatus`. One semantics defined once in
   `apply_actions`, executed identically by the engine's memory
   advance, the sqlite repo, and every fake. Additive extension is the
   established pattern (validators and sync status were added exactly
   this way in Phase 2/3).
6. **The outbox is the Deck sync boundary — Deck has no *native*
   reminder model.** `LocalOp` (`outbox.rs:22-51`) covers exactly the
   Deck card/stack/label/board surface. Nextcloud Deck cards expose no
   first-class reminder field; however, Deck cards *do* support custom
   attachments, so a future transport for taskboard-private data is
   conceivable (see D-a — this round treats Deck's gap as a v1 fact,
   not a permanent property of the design).
7. **Actors with IO get their own crate/task; pure evaluation does
   not.** The sync actor (`taskboard-sync-nextcloud/src/actor.rs:181,
   207`) runs its cycle on a `tokio::time::sleep_until(deadline)` arm
   — under `tokio::time::pause()`/`advance()` this is deterministic,
   and the engine/sync tests already rely on that
   (`taskboard-state/tests/snapshots.rs`,
   `taskboard-sync-nextcloud/tests/*`). Ports live in the crate that
   owns the concern: `TaskRepository`/`SyncStateReader` in domain
   (cross-actor), `CredentialStore` in `taskboard-app` (OS boundary,
   ADR 0008).
8. **`SystemEvent` is the global broadcast; `EngineSignal` is the
   repaint trigger.** `messages.rs:186-199`: `NetworkLost`,
   `NetworkRestored`, `Shutdown`, `StorageFailed` (edge-triggered by
   the engine, consumed by everyone). The UI reads state lock-free via
   `ArcSwap<AppState>` and repaints on
   `EngineSignal::StateUpdated` — it never joins actor loops.
9. **`AppState` is a pure function of persisted facts.**
   `state.rs:24-39`: BTreeMaps of the entity sets + `SyncStatus` +
   `last_updated` (mutation stamp). Nothing time-varying is stored in
   it; insta snapshots pin its shape ("Treat any change to this type's
   shape as a snapshot-breaking change", state.rs:20-22).
10. **Kiosk constraints shape the design** (AGENTS §1, architecture.org):
    low-power hardware, offline-resilient, reboots at any moment — so
    "already notified" must survive a restart (persisted, not
    in-memory), and the overdue banner must render from cache with no
    network. The daemon frontend (ADR 0012) is the kiosk's long-lived
    process: whatever fires reminders must live in the bootstrapped
    core, not in a CLI invocation.

## 2. Scope & non-goals

**In scope (design only):** the reminder domain model (rules,
deliveries, snooze space, implicit defaults, pure evaluation), the
scheduler mechanism on the injected clock, the notification output
port, the kiosk overdue-banner path, the `AppState`/`SystemEvent`/
`EngineSignal` surface the Phase 7 UI reads, persistence shape,
sync-readiness and merge interaction, config surface, CLI sketch,
test strategy, draft ADR 0013, resolved decisions (§13), and the
follow-up plan breakdown.

**Non-goals:** any production code; choosing/wiring a desktop
notification library (the port is designed; the toast impl is plan 5,
gated on §13-D3); the *interactive* snooze/dismiss surface (the
model and persistence are designed here, the UI affordances ship with
the desktop notification work); repeating/escalating nag policy
beyond snooze (§13-D4); insistence levels beyond reserving the
architecture for them (§6.1); calendar (fixed-time, non-due-anchored)
reminders (§13-D1); the actual sync protocol for rules/proofs
(desired and backloged — §4.4 D-a — but a future design round).

## 3. Product framing: two surfaces, one rule model

Reminders manifest on exactly two surfaces, with different mechanics:

| Surface | Mode | Mechanism | Needs scheduling? |
|---|---|---|---|
| **Overdue banner** | Kiosk (primary), Desktop | Always-on derived view: "these tasks are past due", rendered every frame from `AppState` + current time | **No** — pure derivation at read time (§7) |
| **Active notification** | Desktop (primary), Kiosk (optional) | Fire-once event at (or before) a due moment: OS toast / log line, delivered at most once per occurrence; dismissable or snoozable | **Yes** — the scheduler (§5) |

This split is load-bearing: the banner — the kiosk's core value — is
deliberately scheduler-free, so it can never break, spam, or need
configuration. The scheduler exists solely for the *event* surface,
where "already fired" state matters and where snooze resurfaces it.

## 4. Domain model — reminder rules (pure, proptest-able)

All in a new `taskboard-domain/src/reminders.rs` (module name
`reminders`; the word "notification" is reserved for the delivery
payload). Everything here is pure data + pure functions; time is
always a parameter (§1.3).

### 4.1 Entities

```rust
/// Identity of a reminder rule (v4/v7 uuid, like every entity id) —
/// portable across devices by construction (sync-ready, D-a).
pub struct ReminderRuleId(Uuid);

/// Which reminder a firing/delivery/snooze refers to: an explicit
/// rule, or a task's implicit default reminder (§13 D5) — the default
/// has no stored row, so its identity is the task itself.
pub enum ReminderRef {
    Rule(ReminderRuleId),
    Default(TaskId),
}

/// A reminder rule anchored on a task's due date. N rules per task by
/// design (the map shape keeps multi-reminder additive everywhere).
pub struct ReminderRule {
    pub id: ReminderRuleId,
    /// The watched task.
    pub task: TaskId,
    /// Fire this far *before* the due date; zero = at due.
    /// Constrained ≤ 0 by construction (D-e).
    pub offset: chrono::Duration,
    /// Write time. Sync upgrade path (D-a): per-field write clocks in
    /// the `TaskClocks` pattern are added additively when a connector
    /// demands rule sync; v1 needs only the creation stamp.
    pub created_at: DateTime<Utc>,
}

/// Proof that an occurrence was already notified — the at-most-once
/// guard (D-b). Keyed by (reminder, occurrence), NOT by rule alone:
/// moving the due date or snoozing creates a new occurrence and
/// re-arms. Additive, set-union-mergeable by construction (D-a).
pub struct ReminderDelivery {
    pub reminder: ReminderRef,
    /// The fire instant this delivery settled.
    pub occurrence: DateTime<Utc>,
    pub delivered_at: DateTime<Utc>,
}

/// A user's "show me again in ~10 minutes" (D-b): the occurrence is
/// *rescheduled*, not suppressed — the evaluator walks the chain.
pub struct ReminderSnooze {
    pub reminder: ReminderRef,
    /// The occurrence being snoozed.
    pub occurrence: DateTime<Utc>,
    /// When it fires again (engine stamps `now + postpone`).
    pub rescheduled_to: DateTime<Utc>,
}
```

**Why occurrence-keyed deliveries.** If suppression keyed on the rule
id alone, a user postponing a task (`duedate` moved to tomorrow) after
the first nag would never be nagged again — the single most common
reminder interaction. Keying on `(reminder, occurrence)` makes re-arm
a *derived* consequence of a duedate edit: the pure evaluator sees a
different occurrence, finds no delivery, fires again. Snooze rides the
same mechanism from the other side: the user explicitly schedules the
*next* occurrence. No imperative "reset reminders" hook anywhere (no
`UpdateTask` special-casing, no merge interaction — the LWW duedate
clock does all the work).

**Why rules are a standalone entity, not a field on `Task`.** Same
reason `labels` grew a join table: (a) N rules per task stays additive
(`BTreeMap<ReminderRuleId, ReminderRule>` in the state shape — the
multi-board precedent), (b) `Task`'s field set is the Deck sync
contract; burying reminder fields in it invites accidentally pushing
them before a sync protocol exists, (c) cascade on task delete is an
explicit, testable rule-set operation.

### 4.2 The state section

```rust
/// Persisted reminder facts carried in AppState.
pub struct ReminderState {
    pub rules: BTreeMap<ReminderRuleId, ReminderRule>,
    pub deliveries: BTreeSet<ReminderDelivery>,
    /// Snooze chain links: (reminder, occurrence) → rescheduled
    /// instant. Chains compose (a snooze target can be snoozed).
    pub snoozes: BTreeMap<(ReminderRef, DateTime<Utc>), DateTime<Utc>>,
    /// Tasks muted out of notifications entirely (§13 D5): no
    /// firings of any kind. The overdue *banner* ignores this set —
    /// mute means "stop nagging", not "hide reality".
    pub muted: BTreeSet<TaskId>,
}
```

`AppState` gains `pub reminders: ReminderState` (snapshot-breaking
for insta; every existing snapshot updates once, mechanically).
AppState remains a function of *persisted facts only* — no `now`-view
ever enters it (§8).

### 4.3 Pure functions (the proptest surface)

```rust
/// The instant an explicit rule fires at, given the task's current
/// duedate. None ⇔ no duedate.
pub fn occurrence_of(rule: &ReminderRule, duedate: Option<DateTime<Utc>>)
    -> Option<DateTime<Utc>>;

/// Effective occurrence of any reminder (explicit or default): the
/// base occurrence, or wherever the snooze chain has moved it.
pub fn effective_occurrence(
    reminder: &ReminderRef,
    reminders: &ReminderState,
    tasks: &BTreeMap<TaskId, Task>,
) -> Option<DateTime<Utc>>;

/// Everything that must fire at `now`: explicit rules plus implicit
/// defaults, occurrence-keyed, snooze- and mute-aware.
pub fn due_firings(
    tasks: &BTreeMap<TaskId, Task>,
    reminders: &ReminderState,
    defaults: &ReminderDefaults,   // injected per-device config (D5)
    now: DateTime<Utc>,
) -> Vec<ReminderFiring>;

/// Kiosk banner: live, open tasks with duedate < now, canonical order.
/// Ignores rules, deliveries, snoozes, and mute — the banner is board
/// truth, not a nagging policy.
pub fn overdue_tasks(tasks: &BTreeMap<TaskId, Task>, now: DateTime<Utc>)
    -> Vec<&Task>;

/// Desktop "due soon" strip: same filter within (now, now + horizon].
pub fn due_soon(tasks: &BTreeMap<TaskId, Task>, now: DateTime<Utc>,
                horizon: Duration) -> Vec<&Task>;

/// Per-device defaults for the implicit reminder (§13 D5).
pub struct ReminderDefaults {
    /// Offset before due for the implicit default reminder.
    pub default_before: chrono::Duration,   // ≤ 0; config default −5m
}
```

`due_firings` is the scheduler's entire brain; the firing type carries
`(ReminderRef, TaskId, occurrence)` — exactly the triple that becomes
a `ReminderDelivery` row plus a `Notification` payload downstream.

**Evaluation semantics (the contract):**

- A reminder participates iff its task `is_live() && is_open()`
  (entities.rs:189-197) and has a duedate. Completed and tombstoned
  tasks never fire and vanish from the banner.
- **Explicit rules replace the default** (D5): the implicit
  `Default(task)` reminder participates iff the task has **no**
  explicit rule. Setting a first explicit rule silences the default
  without touching any stored row — it is derived, so it cannot drift.
- **Mute is total** for firings: a task in `muted` produces no firing
  of any kind (default or explicit). The banner ignores `muted`.
- **Snooze walk:** effective = base occurrence; while `snoozes`
  contains `(reminder, effective)`, effective = that target. A
  reminder fires iff effective ≤ now ∧ no delivery exists for
  `(reminder, effective)`. The snoozed-away occurrences keep their
  deliveries (they *were* delivered) and never re-fire; the target is
  a fresh occurrence that fires once, and is itself snoozable.
- A reopened task re-enters the banner immediately; its notifications
  stay suppressed per occurrence (the banner is the always-on truth,
  the toast is a one-time courtesy that snooze can reschedule).

**Property catalogue** (domain proptests, TDD-first in the
implementation plan; AGENTS §5):

- **P1 no-double-fire**: `due_firings` never returns a `(reminder,
  occurrence)` pair present in `deliveries`; appending the delivered
  firings to `deliveries` makes a second call at the same `now`
  return nothing (idempotence under settlement).
- **P2 eligibility**: for arbitrary tasks/rules, firings ⊆ reminders
  whose task is live ∧ open ∧ duedated ∧ unmuted ∧ effective
  occurrence ≤ now; `Default(task)` firings additionally require the
  task to have no explicit rule.
- **P3 occurrence monotonicity**: `occurrence_of` is a pure function
  of (offset, duedate); moving duedate later never produces an
  earlier occurrence for the same rule.
- **P4 banner exactness**: `overdue_tasks` = exactly the live ∧ open
  ∧ duedate < now set, in canonical order (`order`, then id — same
  comparator as `AppState::live_tasks`, state.rs:109-116), regardless
  of rules/deliveries/snoozes/mute.
- **P5 delivery-settlement roundtrip**: interpreting the reminder
  actions (`RecordReminderDelivery`, `RecordReminderSnooze`, mute
  toggles) via `apply_actions` yields the `ReminderState` that
  suppresses exactly the fired set (pins the shared-`apply_actions`
  invariant, mirroring the existing P2/P3/P4 family of ADR 0006).
- **P6 serde roundtrips** for all new types (house pattern,
  entities.rs:242).
- **P7 snooze suppression**: while `(reminder, occurrence)` is snoozed
  to a future target, `due_firings` at any `now` before the target
  excludes that reminder; at `now ≥` target it fires iff undelivered.
- **P8 mute exactness**: a muted task produces zero firings (default
  and explicit alike) while remaining in `overdue_tasks` when past
  due.

### 4.4 Recorded decisions (direction-approved 2026-10-12)

- **D-a v1 is local-only; the model is sync-ready; syncing is a
  desired, backloged property** (user direction, round 2). Nothing
  reminder-shaped enters the outbox in v1 — but nothing in the design
  *forbids* or *forecloses* sync, because Deck's gap is a fact about
  one connector, not about the model:
  - rule ids are portable UUIDs — device-agnostic, collision-free;
  - deliveries are occurrence-keyed **additive** rows: a cross-device
    merge is set-union, conflict-free by construction — the proofs
    are designed to be synced;
  - `ReminderRef::Default(task)` derives from synced data (the
    duedate) plus per-device config — no stored rows to reconcile;
  - snoozes and mutes are per-device UX facts by intent (a snooze is
    the *viewer's* "later"); whether they roam is a policy question
    for the future round, not a shape constraint;
  - no single-device assumption exists anywhere in the shapes; the
    additive upgrade path for rule sync is per-field write clocks in
    the `TaskClocks` pattern (exactly how the entities grew theirs);
  - future connectors may support reminders natively — the connector
    seam is where a capability like "server-side reminders" belongs;
    and even Deck can plausibly carry taskboard-private data hacked
    through **custom card attachments** — a transport candidate,
    explicitly its own future design round (it touches the outbox,
    the merge policy, and the Deck attachments API).
  Backlog entry added this round: "[2026-10-12] Sync of reminder
  rules and delivery proofs".
- **D-b one suppression mechanism, with snooze space reserved** (user
  direction, round 2). Deliveries remain the only at-most-once guard
  — per *occurrence*. Snooze does not weaken at-most-once; it
  **reschedules the occurrence** (a persisted chain link,
  `ReminderSnooze`), and the same pure evaluator fires the target
  ~10 minutes later. No "delivered = done forever" assumption exists
  anywhere: dismiss/snooze are user intents that flow UI → engine as
  `StateCommand`s (thin-client rule, AGENTS §1) —
  `SnoozeReminder { reminder, occurrence, postpone: Duration }`
  (engine stamps `now + postpone` via the injected clock, the
  `SetTaskDone` precedent). The interactive snooze surface ships with
  the desktop notification plan (§14 plan 5); the model and
  persistence land in plans 1–4.
- **D-c cascade.** Task delete (tombstone) cascades to its rules —
  `plan_command` emits `DeleteReminderRule` for each of the task's
  rules (the `DeleteStack`→tasks cascade precedent). Deliveries and
  snooze links of cascade-deleted rules are pruned with them (bounded
  growth without a background GC; age-based pruning of live tasks'
  old deliveries is a backloged housekeeping entry).
- **D-d ordering.** `due_firings` returns firings sorted by
  (occurrence, reminder ref) — deterministic output is a snapshot/insta
  and proptest precondition (state.rs:6-8 convention).
- **D-e offset ≤ 0** enforced at `ReminderRule` construction
  (newtype/constructor validation, `Color::new` lenient-vs-strict
  precedent inverted: this one *is* domain-meaningful). "After due"
  nagging beyond snooze is the repeat-policy question (§13-D4), not
  an offset sign.

## 5. Scheduler — an engine tick arm, not a second actor

### 5.1 Options considered

- **Option A — engine-internal tick arm (approved, D2).** The engine
  loop grows a fourth `select!` arm: `() = tokio::time::sleep_until(next_tick)`,
  re-arming after each wake. On wake the engine does exactly what it
  does for any other message: read `clock.now()` once, run the pure
  `due_firings` against `EngineCore`, and if non-empty plan a batch of
  `RecordReminderDelivery` actions → `repo.apply` → `interpret` →
  publish → hand the firing payloads to the notifier (§6). Nothing
  fires if the batch fails to persist — the delivery marker **is** the
  notification permit (at-most-once per occurrence beats at-least-once
  for a kiosk; a lost toast on a dying disk is acceptable, a re-nag
  loop is not; snooze covers "I missed it").
- **Option B — `EngineCommand::Tick` from a bootstrap ticker task.**
  Same semantics; the timer lives in `taskboard-app`, the inbox gains
  a `Tick` variant. Costs: one more spawnable moving part, `Tick`
  crosses the public inbox (REPL/IPC/test code can inject it — mostly
  harmless, but it widens `EngineCommand`'s contract), and the
  auto-advance hazard (§5.3) is identical.
- **Option C — standalone scheduler actor** reading `ArcSwap`
  snapshots and writing fired markers back through the engine as
  `StateCommand`s. Rejected: reminder state would have two logical
  writers routed through one inbox; fired markers aren't user intent
  and don't belong on the `StateCommand` enum; and the actor's entire
  job would be "wake up, call a pure function the engine could call
  cheaper" (it already holds `EngineCore`; no `ArcSwap` read-clone
  needed). The sync actor justifies its existence by owning IO
  (reqwest); a reminder actor would own nothing.

### 5.2 Mechanics

- Spawn signature: `spawn_state_engine` grows one injected
  `reminder_cfg: ReminderEngineConfig { enabled: bool, tick: Duration,
  defaults: ReminderDefaults }` (`#[allow(too_many_arguments)]`
  already carries the "seams ARE the constructor" note, actor.rs:179).
  `enabled: false` ⇒ the arm is never armed; existing call
  sites/tests unchanged.
- Per wake: one `clock.now()` read; if `due_firings` is empty, no
  batch, no publish, no signal (a quiet tick is invisible — same
  no-op discipline as the accepted-no-op command path, actor.rs:435).
- Shutdown/drain: timer wakes during drain are ignored (drain
  declines new work, actor.rs:284-292 precedent) — an undelivered
  reminder at shutdown simply fires on the next boot's first tick
  (occurrence still ≤ now, no delivery yet).
- Drain fairness: the `biased` order stays commands → reports →
  system → timer; a busy inbox can starve ticks temporarily, which is
  fine — lateness of minutes is invisible for due-date reminders and
  the banner never depends on ticks.

### 5.3 Testability (AGENTS §5 time rules)

- No `tokio::time::sleep` anywhere in tests. Engine reminder tests:
  `#[tokio::test(start_paused = true)]` + `advance(tick)`; the
  injected clock is a stepping fake synchronized by the test (the
  "fixed or advancing logical times" pattern, clock.rs:6-8). Assert
  terminal state (deliveries present, notification sent) after N
  advances — never wall-clock, never intermediates. Snooze tests
  advance past `postpone` and assert the re-fire plus its settlement.
- **Auto-advance hazard (called out for the implementation plan):**
  under `start_paused`, tokio auto-advances time when all tasks are
  idle, so *existing* paused engine tests that await while idle would
  start absorbing ticks once any test config enables the arm. Guard:
  `enabled` defaults to `false` in every test helper; reminder tests
  opt in explicitly. This must be pinned by a dedicated test
  (existing suite unchanged with reminders disabled).

### 5.4 Config surface

New `[reminders]` figment table (strict-table rule, ADR 0008;
durations decode natively — the `invalid-duration` config error kind
already exists):

- `enabled` (bool, default `true`),
- `tick_secs` (u64, default `30` — generous for minute-scale
  reminders, trivially cheap: one pure pass over a BTreeMap on a
  kiosk),
- `default_before` (Duration, default `"5m"` — §13 D5: the implicit
  default reminder's offset, user-configurable **from day one**; a
  config knob now rather than a retrofit later, because figment
  already carries the hierarchy and the evaluator takes it as a pure
  parameter). Per-device by nature — a kiosk and a desktop may differ.

Absent table = defaults (defaults → `taskboard.toml` →
`TASKBOARD_*`). The kiosk deployment tunes via TOML, not code.

## 6. Notification delivery — a Port, log impl first

**Payload (domain, pure):**

```rust
pub struct Notification {
    /// Which reminder fired — correlates the UI's dismiss/snooze
    /// replies with this payload (both carry the ref + occurrence).
    pub reminder: ReminderRef,
    pub task: TaskId,
    /// Title snapshot at fire time (the task may be edited later).
    pub title: String,
    pub occurrence: DateTime<Utc>,
}
```

In `reminders.rs` — payload/envelope split respected (ADR 0004's
note): the domain owns the pure data; channels stay in actor crates.
The engine's send-side is a `mpsc::Sender<Notification>` injected at
spawn (bootstrap-owned channel halves, ADR 0008); a closed channel is
a `debug!` no-op — the `forward_sync_now` precedent (actor.rs:479-496):
headless/CLI configs legitimately run without a notifier.

**Sink port (app crate, `CredentialStore` precedent §1.7):**

```rust
/// OS-boundary port; impls live in taskboard-app.
pub trait NotificationSink: Send + Sync {
    fn deliver(&self, notification: Notification)
        -> BoxFuture<'_, Result<(), NotifyError>>;
}
```

- **v1 impl: `LogSink`** (D3, approved) — emits a `tracing::info!`
  event with typed fields. Zero new dependencies, already useful
  (daemon journald trail), and the daemon/kiosk doesn't want OS toasts
  anyway (the banner is the surface).
- **follow-up impl: `DesktopSink`** — `notify-rust` (or
  platform-equivalent) behind the same port, its own plan (§14 plan
  5). `cargo deny` check for the new license tree happens there.
  This plan is also where the *interactive* snooze/dismiss surface
  lands (the model is already in place, D-b).
- **tests:** a `Vec<Notification>`-collecting fake (fakes-over-mocks,
  AGENTS §5) at the engine level — assert the *set* of payloads, not
  order/text.

Delivery ordering guarantee: the engine sends the notification only
**after** `repo.apply` of the delivery markers succeeded (§5.1) —
durable-then-notify, the evidence-then-verdict shape of the sync
reports (`SyncReport::Failed` carrying landed pushes) applied to
outbound reminders.

### 6.1 Insistence levels — reserved, not built (user direction)

Two eventual flavors are wanted: **normal** (OS toast + taskbar-icon
blink + an in-app dismissable/snoozable window) and **alarm**
(demand attention: insistent sound rising while ignored,
focus-stealing alarm window, until dismissed or snoozed). The
architecture keeps this additive:

- an `urgency` field on `ReminderRule` and on `Notification` is a
  pure data extension — the evaluator is and stays urgency-agnostic
  (it decides *when*, never *how loud*);
- the port signature does not change (the payload grows a field);
- escalation ("louder the longer it is ignored") is a UI/notifier
  rendering loop driven by the un-dismissed notification's presence
  in state — no engine or scheduler involvement;
- dismiss/snooze ride the same `StateCommand` path regardless of
  urgency.

Backlog entry added this round: "[2026-10-12] Reminder insistence
levels (notification vs alarm)". Multi-reminder UX surfaces (N rules
per task) are backloged alongside it — the model already admits N;
the management surfaces are the future work.

## 7. Kiosk overdue-banner path

The banner is **scheduler-free by design** (§3):

1. UI reads `ArcSwap<AppState>` lock-free every frame.
2. It calls the pure `overdue_tasks(&state.tasks, now)` (§4.3) —
   domain function, presentation-legal (a pure read, no IO; the
   ui-slint crate already depends on domain).
3. `now` comes from the UI's own clock. Time passing never mutates
   `AppState`, so the UI owns a lightweight minute-granularity
   repaint timer (Slint `Timer`) alongside its
   `EngineSignal::StateUpdated` subscription: data changes repaint
   via the signal; the passage of time repaints via the timer. A
   timer is presentation, not business logic — the thin-client rule
   (AGENTS §1) forbids state *mutation* in the UI, not local
   re-derivation.

Consequences the Phase 7 plan can rely on: the banner works from
cache on a networkless boot (offline-resilience, §1.10), renders
before the first tick, cannot spam — it has no memory to spam with —
and ignores mute/snooze by contract (board truth, not nagging
policy, §4.2).

## 8. The AppState / SystemEvent surface Phase 7 reads

**Additive `AppState` changes** (the whole UI-facing delta of this
design):

- `pub reminders: ReminderState` — rules + deliveries + snooze links
  + muted set (§4.2). UI renders rule lists (management views) from
  it; delivered-ness and snoozed-ness are plain set-membership
  checks; the dismiss/snooze buttons dispatch `StateCommand`s and are
  thin-client-legal.

**Unchanged:**

- `EngineSignal::StateUpdated` remains the only repaint signal. A
  reminder firing *is* a state change (delivery markers land), so the
  UI wakes for free; no new signal variant.
- **No new `SystemEvent` variant.** `SystemEvent` is global *status*
  (network, shutdown, storage); a task-level reminder firing is not
  global status, and every consumer of the broadcast would grow a
  no-op arm (compiler-forced, `StorageFailed` precedent,
  actor.rs:365). If Phase 7 later wants a transient toast/sound nudge
  beyond StateUpdated, that variant can be added additively — the
  default is *not* to add it.

**Derived-at-read views** (`overdue_tasks`, `due_soon`) are function
calls, not state fields — the "AppState is persisted facts only"
invariant (§1.9) is preserved, and `last_updated` keeps meaning "last
mutation", not "last tick".

## 9. Persistence & storage shape

- **Migration `0004_reminders.sql`** (numbering assumes WI-A's 0003
  landed; if reminders are implemented first, renumber — the plan
  takes the next free slot). Additive policy (ADR 0005):
  ```sql
  CREATE TABLE reminder_rules (
      rule_id TEXT PRIMARY KEY,
      task_id TEXT NOT NULL REFERENCES tasks(task_id),
      offset_seconds INTEGER NOT NULL,   -- ≤ 0
      created_at TEXT NOT NULL
  );
  CREATE TABLE reminder_deliveries (
      reminder TEXT NOT NULL,           -- rule id | "default:<task>"
      occurrence TEXT NOT NULL,
      delivered_at TEXT NOT NULL,
      PRIMARY KEY (reminder, occurrence)
  );
  CREATE TABLE reminder_snoozes (
      reminder TEXT NOT NULL,
      occurrence TEXT NOT NULL,
      rescheduled_to TEXT NOT NULL,
      PRIMARY KEY (reminder, occurrence)
  );
  CREATE TABLE reminder_muted (
      task_id TEXT PRIMARY KEY
  );
  ```
  No FK on deliveries/snoozes (rules cascade-delete first);
  WAL/NORMAL unchanged; all tables are local cache in v1 — absent
  from the sync surface until the D-a backlog round designs it.
- **Domain actions** (additive `PersistenceAction` variants, §1.5):
  `UpsertReminderRule(ReminderRule)`, `DeleteReminderRule(ReminderRuleId)`,
  `RecordReminderDelivery(ReminderDelivery)`,
  `RecordReminderSnooze(ReminderSnooze)`,
  `SetTaskRemindersMuted(TaskId, bool)`. Semantics defined once in
  `apply_actions` (upsert-by-id; delete removes; delivery insert is
  set-union — a replayed delivery is a no-op, giving the
  crash-between-apply-and-notify moment a benign re-tick; a snooze
  link replaces any prior link for the same occurrence). Snooze links
  whose target has a delivery are prunable during interpret (chain
  compaction — same batch, defined once in `apply_actions`). The
  sqlite repo and every fake inherit the semantics through the shared
  function — the parity that P5 pins.
- `PersistedState` grows the same `ReminderState`; `load()` hydrates
  it with everything else (boot = instant banner from cache).

## 10. Sync & merge interaction

- **V1: no reminder data enters the outbox** — invisible to
  `plan_pushes`, the push executor, and the merge pipeline; no R1–R9
  rule changes. This is a v1 scope line, **not** a design commitment
  (D-a): the shapes are sync-ready and the property is desired.
- **The future sync round** (backlog) designs: per-field rule clocks,
  the connector capability seam ("server-side reminders"), delivery
  proof union across devices, the Deck custom-attachment transport,
  and whether snoozes/mutes roam. The occurrence-keyed additive
  delivery rows are deliberately shaped so their cross-device merge
  is set-union — conflict-free by construction.
- **Due dates flow through the existing field merge unchanged.** A
  remotely adopted `duedate` change (LWW on `clocks.duedate`) alters
  what `occurrence_of` computes on the next tick — re-arm is a
  *reader-side* consequence, no merge hook (§4.1). Conflict cases
  come out right for free: whichever duedate wins defines the
  occurrence; the delivery set suppresses only old occurrences.
- A task arriving from Deck **with** a duedate but no local rule
  still nags via the implicit default (D5) — zero local setup, the
  kiosk story — unless muted or given explicit rules.

## 11. CLI surface (sketch; detailed in the implementation plan)

Reads ride existing verbs: `tasks list` grows an overdue/due-soon
marker column (pure derivation over the same functions). Management:

```
taskboard tasks remind <task-selector> [--before <dur>]   # explicit rule; default offset: config default_before (5m)
taskboard tasks remind --list
taskboard tasks remind --clear <task-selector>
taskboard tasks remind --off <task-selector>              # mute (banner unaffected)
```

Selector semantics reuse the cascade (exact uuid → unique ≥8 prefix →
exact title); `--json` projections follow the kebab-case envelope
contract (ADR 0009). Typed outcomes only (`CommandOutcome::ReminderSet
{ rule }`, `…ReminderCleared { removed: usize }`, `…ReminderMuted`)
— no text assertions (AGENTS §5). Snooze is a notification-interaction
path (plan 5), not a CLI verb in v1.

## 12. Draft ADR 0013 (formalized by stage 2)

```org
#+title: ADR 0013: Reminders — Local Due-Anchored Rules, Engine-Tick Evaluation, Ported Delivery
- *Date:* 2026-10-__           - *Status:* DRAFT (design round WI-I, direction approved)
- *Deciders:* Viktoria Pogrzebacz

* Context
Taskboard's identity includes "reminder system" (AGENTS §1) but no
reminder design existed: tasks carry a synced =duedate= (per-field
LWW), the state engine owns all state transitions (ADR 0006), Deck
exposes no native reminder field, and the kiosk reboots at arbitrary
moments on unreliable networks.

* Decision
1. Reminder *rules* are domain entities anchored on =Task.duedate=
   (=offset ≤ 0=). V1 keeps them local (no Deck counterpart), but the
   model is sync-ready — portable ids, occurrence-keyed *additive*
   delivery proofs (set-union merge, conflict-free by construction),
   per-field write clocks as the additive upgrade path. Syncing
   rules/proofs is a desired, backloged property; Deck may even gain
   a custom-attachment transport later. Snoozes/mutes are per-device
   UX facts until that round says otherwise.
2. Suppression is *occurrence-keyed*: a =ReminderDelivery=
   (reminder, occurrence) row is the single at-most-once guard,
   persisted before any notification is sent. Moving a due date
   re-arms by derivation; *snooze reschedules the occurrence* through
   the same pure evaluator — no imperative reset hooks, and
   "delivered" never means "done forever".
3. Scheduling is an engine *tick arm* (=sleep_until=), not an actor:
   on each tick the engine runs the pure =due_firings= over
   =EngineCore=, persists delivery markers, interprets, publishes,
   and only then hands payloads to the notifier. A failed batch
   fires nothing.
4. Notification delivery is a port (=NotificationSink=,
   CredentialStore precedent): v1 ships a =tracing= log impl; desktop
   toasts (with the interactive dismiss/snooze surface) are
   follow-ups behind the same port.
5. The kiosk overdue banner is scheduler-free: a pure derived view
   (=overdue_tasks=) evaluated by the UI at read time with its own
   clock; passing time never mutates AppState, and the banner ignores
   rules, deliveries, snoozes, and mute — it is board truth.
6. =AppState= grows persisted reminder facts only (=ReminderState=);
   =EngineSignal::StateUpdated= remains the sole repaint signal; no
   new =SystemEvent= variant.
7. Every duedated task carries an *implicit default reminder* at a
   user-configurable offset (=[reminders] default_before=, default
   5 m before due), per-device; explicit rules replace the default;
   mute (=--off=) silences all firings for the task without touching
   the banner.
8. N rules per task and insistence levels (notification vs alarm)
   are additive future extensions the model already admits: no
   surface assumes one-reminder-per-task, and urgency is a future
   rule/payload field the evaluator never reads.
```

## 13. Decisions — resolved by user review 2026-10-12

- **D1 — v1 rule catalog: offset-before-due only** (approved).
  Fixed-time (calendar-style) and per-board defaults stay deferred —
  backloged at stage 2 only if still wanted.
- **D2 — scheduler placement: engine-internal tick arm** (approved,
  Option A of §5.1).
- **D3 — notification v1 delivery: `LogSink` only** (approved);
  desktop toasts remain follow-up plan 5.
- **D4 — overdue policy: single fire per occurrence + persistent
  banner + snooze as the re-surface mechanism** (approved with the
  snooze caveat). Repeating/daily nags stay deferred; snooze is the
  sanctioned "show me again in ~10 minutes", modeled as occurrence
  rescheduling (§4.4 D-b, P7).
- **D5 — implicit default reminder at 5 m before due,
  user-configurable from day one** (approved with amendment). Chosen
  form: a config item in v1 — `[reminders] default_before` (Duration,
  default `"5m"`) — rather than "easily made configurable later",
  because figment already carries the hierarchy and the evaluator
  takes the offset as a pure parameter, so the knob costs nothing at
  design time and avoids a retrofit. Explicit rules replace the
  default; `--off` mutes the task's firings entirely (default and
  explicit alike); the banner is unaffected by mute.
- **Round-2 direction recorded as design commitments:** reminder
  rule/delivery sync is desired-but-deferred with a sync-ready model
  (D-a rewritten accordingly); multi-reminder-per-task (N rules) and
  insistence levels (notification vs alarm) must stay additive —
  backlog entries added this round for both (§6.1).

## 14. Follow-up plan breakdown (the implementation rounds)

| # | Plan (`.artifacts/plans/…`) | Scope | Size |
|---|---|---|---|
| 0 | stage 2 of *this* round | ADR 0013 formalized, architecture.org + testing_strategy amendments, backlog ✅ | S |
| 1 | domain: reminder model + evaluation | `reminders.rs` entities (rules, deliveries, snoozes, mute, defaults), pure fns, `PersistenceAction` variants, `apply_actions`, proptests P1–P8 | M |
| 2 | state engine: tick arm + notifier out | §5 mechanics (incl. `SnoozeReminder` command handling), snapshots, pause/advance tests, auto-advance guard | M |
| 3 | storage: migration + repo | `0004_reminders.sql` (4 tables), repo batch/load, fake parity | S/M |
| 4 | app: wiring + config + CLI | bootstrap notifier channel + `LogSink`, `[reminders]` config (`enabled`, `tick_secs`, `default_before`), `tasks remind` verbs incl. `--off`, `--json` | M |
| 5 | desktop notification surface | `notify-rust` `DesktopSink`, deny check, the interactive dismiss/snooze UI path, platform notes | S/M |

**Estimate: 4 implementation plans now (1–4) + stage-2 formalization;
plan 5 (desktop toasts + interactive snooze) when wanted.** Plans 1–3
are strictly ordered; 4 follows; Phase 7's UI plan consumes §8's
surface independently of all of them (that is the point of running
this round first).

## 15. Risks & mitigations

- **Engine-loop growth** (the tick arm makes an already-dense
  `select!` denser): mitigated by the arm's total size (evaluate →
  batch → existing apply/interpret/publish path) and by keeping
  `due_firings` in the domain where mutants/proptests can reach it.
- **Auto-advance interaction with existing paused tests** (§5.3):
  config-gated arm, default-off in every test helper, pinned by a
  dedicated regression test.
- **Delivery/snooze-table growth on long kiosk uptimes**: bounded by
  the delete-cascade and snooze-chain compaction (D-c, §9);
  age-based pruning deferred to the backlog (entry exists).
- **Snapshot churn**: one mechanical insta-update pass when
  `AppState.reminders` lands; every subsequent snapshot change is
  reviewable signal, not noise.
- **`tokio::time` vs injected-clock divergence**: the tick *cadence*
  is tokio time (pausable) while firing decisions stamp the injected
  clock once per wake (§1.4 convention); tests keep both fakes in
  lockstep — documented in plan 2's test design.
- **Sync-readiness drift** (the D-a commitments eroding): the
  round-2 backlog entry names the concrete properties to preserve
  (portable ids, additive occurrence-keyed proofs, no single-device
  assumptions) so reviews of plans 1–4 can check against them.

## 16. Verification of this round

- Every §1 fact was verified against `main` @ `9b46e2e` by reading
  the cited files; line anchors recorded for the stage-2/3 agents.
- Round 2 (2026-10-12): user direction folded in — D-a rewritten
  (sync-ready model; sync desired + backloged), D-b extended with
  the snooze mechanism, D5 amended (5 m configurable default, mute),
  insistence levels + multi-reminder additivity reserved (§6.1),
  property catalogue extended (P7, P8), persistence shape extended
  (snoozes, muted), ADR draft updated (decisions 1, 2, 7, 8).
- No code, no `Cargo.toml`, no ADR file was touched — this document
  is the whole deliverable (plus the branch bookkeeping commit).
- Backlog entries added by this round: "Sync of reminder rules and
  delivery proofs", "Reminder insistence levels (notification vs
  alarm)", "Multi-reminder UX surfaces (N rules per task)", plus the
  origin-entry status note (updated to direction-approved) and the
  round-1 "Age-based pruning of reminder deliveries" entry.
- Stage 2 checklist: formalize ADR 0013 with the §13 resolutions,
  renumber the migration if WI-A hasn't landed, keep the four new
  backlog entries in sync with what ships, mark the origin entry per
  its status note.
