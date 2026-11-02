# [Plan] Delivery-Proof Pruning — Age-Bounded Reminder Delivery Rows

- **Date**: 2026-11-02 (file name carries the round's dispatch date,
  2026-10-31, per the chunk's mandate).
- **Kind**: stage-1 implementation plan for backlog round 2, chunk 3.3
  (`feat/delivery-proof-pruning`), closing **#201**.
- **Origin**: #201 (normative); the audit's F16-20 note
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`
  §chunk-16, `persistence.rs` dependent-pruning precedent); the
  reminders design plan's delivery-suppression section
  (`.artifacts/plans/2026-10-12-reminders-design-plan.md` §4.1–§4.4,
  the `ReminderDelivery` at-most-once guard and property catalogue);
  the #201 close-comment re-terming (2026-11, #474/DR-3: settlement
  moved to dismissal; dismissal-row aging is a **separate,
  deployment-gated** concern). Dispatch plan:
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` §1
  chunk 3.3 (batch 3; same-crate gate 2.2 — verified merged:
  PR #576, 2026-11-01, commits rebase-landed on `origin/main`).
- **Status**: READY for stage 2. Every open question is resolved in
  §7 (decisions D1–D14). The design **deviates from the issue's
  literal "at record time" mechanism** — the recorded reason is the
  re-fire-loop proof in §3; the chunk's goal (bounded delivery-row
  growth, lockstep fakes/sqlite, pure determinism) is met exactly.

## 1. Problem

At-most-once reminder suppression keys on `(reminder, occurrence)`
delivery rows (`ReminderDelivery`, `reminders.rs:344-351`): the
evaluator fires iff the effective occurrence is past due **and**
neither delivered nor dismissed (`due_firings`,
`reminders.rs:815-818`). Task deletion cascades a rule's delivery rows
away (`DeleteReminderRule`, `persistence.rs:358-372` — the F16-20
precedent), but a **long-lived live task with a moving due date**
accumulates one row per re-arm forever: every duedate move mints a new
occurrence, the occurrence fires when reached, and the row lives
indefinitely. Rows are tiny — this is housekeeping, not a leak — but
the accumulation is unbounded by anything except user behaviour, and
the kiosk deployment class (#201's motivating device) is exactly the
never-cleans-up case.

**Scope guard (D1)**: this chunk prunes **delivery rows only**.
Dismissal rows (whose growth the #201 re-terming comment now tracks)
stay out: their pruning interacts with the *synced* document lane
(Rule-ref dismissals roam; deliveries never do — §6) and the comment
itself gates dismissal aging on "when a real deployment's table sizes
justify the churn". Stage 3 records the deferral on #201 at close.

## 2. The seams (verified on `origin/main` @ `67c0b531`)

| Seam | Location | What it gives this chunk |
|---|---|---|
| Shared interpreter | `apply_actions`, `persistence.rs:309-620` | The one semantics table fakes and sqlite both mirror; the new action's arm lands here |
| Record arm | `RecordReminderDelivery`, `persistence.rs:398-405` | Set-union, first-write-wins via `is_delivered` guard; **stays unchanged** (D2) |
| Cascade precedent | `DeleteReminderRule` / `DeleteAdhocAlarm`, `persistence.rs:358-372`, `:376-378` | F16-20's "one new persistence action" pattern |
| Post-batch derivations | `persistence.rs:595-609` | `compact_snooze_chains` + `prune_settled_alarms` run after every batch and in `load()` — this chunk adds **no** derivation, so `load()` is untouched (D3) |
| Evaluator gates | `due_firings` `reminders.rs:815-818`; `active_notifications` `reminders.rs:885-888` | Both readers consult a delivery row **only at the reminder's current effective occurrence** — the fact the reachability argument (§4) stands on |
| Chain walk | `walk_chain`, `reminders.rs:727-745`; snooze stamp `command.rs:1845-1851` | Sanctioned links point strictly forward (`now + postpone`, `postpone > 0`); cycle-bounded for unsanctioned data |
| Occurrence bases | `occurrence_of` `reminders.rs:700-702`; `base_occurrence` `:758-776`; `alarm_occurrence` `:753-756` | Rule bases = duedate + offset; Default bases need `ReminderDefaults`; alarm bases = immutable `fire_at` (create/cancel are the only verbs, `command.rs:2043/:2057`) |
| Engine tick | `reminder_tick`, `taskboard-state/src/actor.rs:1145-1173`; `delivery_batch` `:1225-1239` | Clock read once → pure evaluation → durable apply → interpret → notify; quiet tick early-returns at `:1156-1158`; 30 s production cadence (`config.rs:16-17`) |
| SQL twin | `execute_batch`, `taskboard-storage-sqlite/src/repo.rs:1155-1241` | **No-wildcard rule** (`:1160-1163`): a new `PersistenceAction` breaks this build until storage persists it — variant and twin land in one commit |
| Delivery upsert | `repo.rs:1873` | `ON CONFLICT (reminder, occurrence) DO NOTHING` — the set-union parity a pruned key re-enters through |
| Lockstep proof | `assert_task_repository_contract_async`, `domain/test_support/contract.rs` (delivery replay pin `:304-326`, cascade pin `:381-394`) | Runs for **every** repository — the in-memory fake and sqlite both; the new lockstep pin lands here |
| Hermetic time | `Clock` doctrine `domain/clock.rs:3-9`; `FixedClock` + `advance_both` + `eventually`, `taskboard-state/tests/reminders.rs:76-248` | The engine tick pins run on the injected clock + paused tokio time; no wall reads |
| Merge side | zero `deliveries` mentions under `taskboard-sync-nextcloud/src/` and `taskboard-state/src/` (grep, 2026-11-02) | Deliveries are per-device facts (R3, `reminders.rs:341-342`); **no merge lane exists to interact with** (§6) |
| Schema | latest migration `0017_habit_trackers.sql` | No migration: pruning deletes rows from the existing `reminder_deliveries` table (D4) |

## 3. Why the issue's literal mechanism is rejected (the loop proof)

The issue proposes "drop deliveries whose occurrence is older than N
days **at record time**". Taken literally — skip the insert in the
`RecordReminderDelivery` arm — the mechanism breaks the guard it
stands on:

1. The engine fires exactly the **effective** occurrences
   (`reminder_tick` → `due_reminder_firings`). A delivery row whose
   occurrence is more than N days older than its `delivered_at` is
   born from a **stale fire**: a kiosk that was offline for months,
   an imported/restored task with an ancient due date.
2. Skip the insert and the state still shows `(r, O)` undelivered and
   undismissed, with `O` still the effective occurrence (the due date
   has not moved). The next tick's `due_firings` returns it again →
   another `NotifierMessage::Fire` → another OS toast, **every tick,
   forever** (until a dismissal or a due-date move). The at-most-once
   contract (design plan §4.4 P1) is violated by construction.
3. Worse, `active_notifications` (`reminders.rs:887`) requires
   `is_delivered` — the dropped row also **blanks the in-app surface**
   for that occurrence while the toasts spam.

A record-time rule can therefore never drop *the just-recorded row*
(the engine always records on-chain occurrences); it could only sweep
*other* rows — and a sweep triggered only by records is strictly
weaker than one driven by every tick. The design below keeps the
record arm untouched and moves the age rule to an engine-side sweep
emitted as data. **D2**: the record arm stays set-union, unconditional
insert; pruning happens only via the new action.

## 4. The design

### 4.1 One new persistence action (the F16-20 prediction)

```rust
/// Remove exactly these delivery rows — the engine's stale-deliveries
/// sweep output (age + unconsulted, `reminders.rs::stale_deliveries`).
/// Absent rows are skipped: a replayed prune is a no-op (the
/// `EnqueueOp` replay parity). Deliveries are per-device facts (R3):
/// this action is engine-minted local bookkeeping and never enters any
/// sync lane. Removing a row the evaluator still consults would break
/// the at-most-once guard — emission-side correctness is pinned by the
/// sweep's unobservability property; interpretation is a dumb set
/// difference on purpose.
PruneReminderDeliveries(Vec<ReminderDelivery>),
```

- **Domain arm** (`apply_actions`, after the snooze arm):
  `state.reminders.deliveries.retain(|d| !doomed.contains(d))` with
  `doomed` a `BTreeSet` built from the payload — order-independent,
  deterministic, identical in fakes and sqlite by construction.
- **SQL twin** (same commit — the no-wildcard rule): a
  `prune_deliveries` partition; per-row prepared
  `DELETE FROM reminder_deliveries WHERE reminder = ? AND occurrence = ?`
  inside the batch transaction (the `DeleteAdhocAlarm` cascade shape at
  `repo.rs:1838-1845`). Per-row statements sidestep SQLite's bound
  variable ceiling for the one-time large first sweep (R1). The batch's
  atomicity clause is preserved (one `tx`).

### 4.2 The sweep (domain-pure, engine-emitted)

```rust
/// Delivery rows the evaluator can no longer consult whose occurrence
/// is older than `horizon` — the engine's prune emission set.
pub fn stale_deliveries(
    tasks: &BTreeMap<TaskId, Task>,
    reminders: &ReminderState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
    horizon: chrono::TimeDelta,
) -> BTreeSet<ReminderDelivery>
```

A row `(r, O, _delivered_at)` is returned iff

- **age**: `O < now − horizon`, and
- **unconsulted**: the reminder is not currently suppressing on this
  row — i.e. the reminder fails its eligibility gates (task anchors:
  `!is_remindable(task)` (deleted/done/undated) or muted; alarms:
  map row absent) **or** `effective_occurrence(r) ≠ Some(O)`.

Both readers consult a delivery row only at
`(current effective occurrence, past due)` behind the same eligibility
gates — so "unconsulted" is exactly "removing this row changes no
reader output today": the compaction-unobservability property, the
same framing the sibling derivations carry
(`persistence.rs:595-604`). Age is measured on the **occurrence**
against a live cutoff, not on the row's birth (`delivered_at −
occurrence`): a birth-age rule never prunes the live-kiosk rows #201
is about, because on-time fires have birth-age ≈ 0 (**D5**, the
rejected alternative recorded).

`effective_occurrence` is now-independent, and `O < now − horizon`
implies `O ≤ now`, so unconsulted-ness does not rotate with the clock:
with state fixed, the returned set only grows as `now` advances (pin
P-H). The helper needs `ReminderDefaults` for Default-ref bases —
**D6**: it runs engine-side (the engine owns `cfg.defaults`), so
`apply_actions` keeps its exact signature and storage interpretation
stays config-free; the alternative (threading defaults into
`apply_actions`) was rejected as a port-contract ripple with no
compensating bound improvement.

### 4.3 The emission site

`reminder_tick` (`actor.rs:1145`), **before** the quiet early return:

```rust
let stale = stale_deliveries(core.tasks(), core.reminders(),
                             &cfg.defaults, now, delivery_prune_age());
let mut actions = stale_prune_batch(&stale);          // [] or [Prune..]
if !firings-based actions otherwise empty && stale.is_empty() { return; }
```

structurally: compute firings and the stale set; if both are empty the
tick stays a no-op; otherwise one batch
`[PruneReminderDeliveries(stale)] ++ delivery_batch(&firings, now)`
goes through the existing durable-then-notify flow unchanged
(`repo.apply` → `core.interpret` → `publish` → `send_notifications`).
A lost batch prunes nothing and re-sweeps next tick (the tick's
failure discipline). The prune rides the reminder tick, not a new
timer (**D7**); with `[reminders] enabled = false` no sweeps run —
and no new rows are minted either, so the accumulation is frozen, not
unbounded (**D8**).

## 5. N, the horizon argument, and the reachability argument

### 5.1 N = 90 days (**D9**)

One domain definition site next to the reminder machinery
(`reminders.rs`; `pub fn delivery_prune_age() -> chrono::TimeDelta` or
a `TimeDelta` const if the pinned chrono allows const construction —
stage 2 picks; a fixed literal cannot fail, so no fallible ceremony).
Not a config knob (**D10**): no deployment has asked to turn it, and
the issue's own bar is "pick N when a real deployment's table sizes
justify the churn" — a const is the smallest honest answer, and
config can be added when evidence exists.

### 5.2 The bound the rule delivers

After the first sweep, a state's delivery set is bounded by:

```
|deliveries| ≤ |live reminders (on-chain rows)|
            + (re-arms younger than 90 days)     // off-chain, not yet age-eligible
            + rows whose reminder died < 90d ago // sweeps age them out; cascades handle most
```

A kiosk moving a due date daily holds ≈ 90 rows per such task plus one
on-chain row — bounded, vs. unbounded before. Growth requires fires;
fires sweep; the bound is an invariant of every tick.

### 5.3 The reachability argument (why pruning cannot loop)

A pruned row can only be *missed* if `(r, O)` fires again later. For
that, `O` must be the effective occurrence at some `T₂ > T₁`:

1. **Chains never walk backward.** Sanctioned snooze links stamp
   `now + postpone` with `postpone > 0` (`command.rs:1845-1851`) at a
   moment when the source occurrence has already fired (`now ≥
   source`), so every link strictly advances; the cycle-bounded walk
   (`reminders.rs:727-745`) rests on forward nodes only. A snoozed-away
   `O` is permanently unreachable except via a recreated base — where
   the walk *passes through* `O` to its target and still never rests
   on it.
2. **Bases are the only way back.** Rule bases return to `O` only by a
   due-date edit recreating `duedate + offset = O`; Default bases
   likewise with the device offset; alarm bases are immutable
   (`fire_at` has no edit verb — create/cancel only).
3. **The sweep never prunes a consulted row.** `O == effective(r)`
   behind passing gates is exactly the consult condition — the live
   occurrence's row survives unconditionally (pin P-A). So the row is
   absent only while `O` is unreachable; the instant a due-date edit
   re-aims at `O`, the reminder fires **once** (the accepted loss,
   §5.4), the record arm re-inserts the row, and suppression is
   restored.
4. **Therefore the re-fire loop of §3 is structurally impossible**: a
   loop needs an absent row at a persistently-consulted occurrence;
   the sweep never produces that shape, and any foreign path that
   did (e.g. a clock-jump mishap, R4) self-heals in one fire because
   the record arm always re-inserts.

### 5.4 N against the re-arm horizon

The prompt's strict form — "largest legal re-arm distance must be < N"
— is **unachievable and unnecessary**, and the plan records why
instead of pretending otherwise:

- **Forward re-arms** (snoozes: `postpone` is only checked positive —
  unbounded; future due dates: unbounded) mint **new occurrence keys**
  with no rows to prune. N never interacts with them; their rows are
  born at their own, on-time fires. No horizon bound on N is needed in
  this direction.
- **Backward re-aims** (recreating an old base) are unbounded by any
  validator — a due date can be set arbitrarily far back. This is the
  only loss direction, and the loss is **one extra toast** per
  re-aim at an occurrence whose row was already pruned (older than
  90 days at sweep time), after which suppression self-heals (§5.3).
  Restoring a ≥ 90-day-stale due date is a deliberate act that makes
  the nag arguably *correct*; suppressing it for the sake of an old
  receipt has no user value. **Recorded acceptance (D11)**: the
  longer-than-N backward re-arm's suppression loss is one
  self-healing toast — acceptable, and bounded by the unobservability
  guard from ever becoming a loop.

### 5.5 Determinism story

No new wall reads anywhere. The domain helper takes `now` as data
(the `clock.rs` doctrine: nothing in the domain calls `Utc::now()`).
The engine stamps nothing new — the sweep consumes the tick's single
clock read (`actor.rs:1154`). Engine-tier pins run on the existing
hermetic harness: `FixedClock` + `advance_both` (injected clock and
paused tokio time move together, `tests/reminders.rs:229-232`) and
`eventually` (1 ms-turn polling under a virtual deadline,
`:238-248`). Domain-tier pins are pure functions of constructed
states. The action log records exactly which rows were pruned, so
replay reproduces the prune bit-for-bit on both interpreters.

## 6. Merge non-interaction

Deliveries are per-device facts and never enter the sync document
(R3; structurally: no code path writes them into it — grep evidence,
§2 table). There is **no merge lane over delivery rows to interact
with**: the set-union language lives in the *write* path
(`ON CONFLICT DO NOTHING`, first-write-wins), which a pruned key
re-enters cleanly with a fresh `delivered_at` when the occurrence
re-fires. The pins: the replay-parity pin already in the contract
suite (`contract.rs:304-326`) keeps holding; the new action's
replay-idempotence pin (P-I) pins that a repeated prune is a no-op;
stage 2 adds no merge pin because there is no lane — the non-interaction
is structural absence, and the existing document-section tests would
fail if a lane ever appeared.

## 7. Recorded decisions

| # | Decision | Reason (compressed) |
|---|---|---|
| D1 | Scope = delivery rows only; dismissal aging deferred to #201's close comment | Re-terming comment gates it on deployment evidence; dismissals roam (sync-lane interaction) — a different design round |
| D2 | Record arm unchanged (always insert); pruning only via the new action | §3 loop proof — a record-time drop of a just-fired on-chain row re-opens the fire gate every tick |
| D3 | No post-batch derivation; `load()` untouched | The sweep is engine-emitted data; interpretation is a dumb set difference — no derivation to mirror in `load()` |
| D4 | No schema migration | Row deletion on the existing `reminder_deliveries` table; next free number stays 0018 |
| D5 | Age = `occurrence < now − N` (live cutoff), not birth age (`delivered_at − occurrence > N`) | Birth age ≈ 0 for on-time fires: it would never prune the #201 live-kiosk rows; the cutoff bounds them |
| D6 | Sweep runs engine-side (owns `ReminderDefaults`); `apply_actions` signature unchanged | Default-ref bases need defaults; threading them into interpretation ripples the port contract for no bound gain |
| D7 | Emission rides `reminder_tick`, before its quiet early return | Same subsystem owns the facts, the clock read, and the cadence; no new timer |
| D8 | Reminders-disabled ⇒ no sweeps | Row creation stops with the tick; frozen accumulation, not unbounded growth |
| D9 | N = 90 days, one domain definition site, not const-panic-prone | §5.1–§5.2: bounded ≈ 90 rows per fast-re-arming task + on-chain rows |
| D10 | No config knob | YAGNI; the issue gates tuning on deployment evidence that does not exist yet |
| D11 | Backward re-aims beyond N lose one self-healing toast — accepted | §5.4: forward re-arms never interact; backward validators are unbounded; deliberate ancient-date restores deserve the nag |
| D12 | Per-row prepared `DELETE`s in the twin, no batch-size cap on the sweep | Parameter-ceiling-safe; the one-time first-sweep cost is µs-scale rows in one tx; a per-tick cap buys complexity, not latency |
| D13 | New action variant + SQL twin land in **one** commit | The no-wildcard rule (`repo.rs:1160-1163`) breaks the storage build on any interim state — commit hygiene demands the vertical slice |
| D14 | The sweep reuses the readers' primitives (`is_remindable`, `muted`, `effective_occurrence`, `alarm_occurrence`); no refactor of the `due_firings`/`active_notifications` bodies | The P-G proptest catches drift; restructuring the near-twin readers is out-of-scope churn |

## 8. TDD-first pin catalogue (stage 2 order)

### Phase 1 — domain, pure (commit 1: `feat(domain)`)

In `reminders.rs` tests (unit pins; `ts()` helpers per house style)
and the existing proptest family (`eval_input` strategies):

- **P-A (loop-guard, the load-bearing pin)**: a stale *on-chain*
  delivery — task due two years ago, fired, never dismissed, due date
  unmoved — is **not** returned by `stale_deliveries`. Mutant check:
  dropping the consult condition from the sweep must fail this pin.
- **P-B**: a stale *off-chain* delivery (occurrence 100 days old,
  due date since moved) **is** returned.
- **P-C**: a *fresh* off-chain delivery (occurrence 10 days old) is
  not returned (inside the horizon).
- **P-D**: a *future* occurrence's delivery (snoozed forward or future
  due date) is never returned regardless of chain position.
- **P-E**: a deleted task's `Default`-ref row is returned (unconsulted:
  `!is_remindable`) — the `DeleteTask` residual (rules cascade via
  D-c, `command.rs:1134-1135`; default facts bound here instead).
- **P-F**: rows whose rule row / alarm row is gone are returned
  (`effective_occurrence` → `None`).
- **P-G (unobservability proptest)**: for arbitrary states, removing
  `stale_deliveries(...)` from `deliveries` leaves `due_firings(now)`
  **and** `active_notifications(now)` unchanged. This is the property
  that makes the whole feature behavior-preserving by construction.
- **P-H (monotonicity proptest)**: fixed state, advancing `now` never
  shrinks the returned set.

### Phase 2 — the action vertical (commit 2: `feat(domain,storage)`)

- **P-I (`apply_actions` pin)**: `PruneReminderDeliveries` removes
  exactly the named rows; a replayed identical action is a no-op; a
  prune naming absent rows is a no-op; the post-batch derivations
  still run after a prune batch (order-orthogonality).
- **P-J (THE LOCKSTEP PIN, `contract.rs`)**: seed both an old
  off-chain and a live on-chain delivery plus the dismissal/replay
  context; apply records + prune; reload and assert exactly the
  off-chain row is gone and the on-chain row retained. The contract
  function runs for the in-memory fake **and** sqlite — the two
  interpreters provably agree on the same scenario (the prompt's
  fake-sqlite lockstep pin).
- **P-K (`tests/roundtrip.rs`)**: the action survives the sqlite twin
  end-to-end (persist → load parity), riding the existing delivery
  seeding in `tests/common/mod.rs`.

### Phase 3 — engine emission (commit 3: `feat(state)` + bookkeeping)

In the R-series harness (`tests/reminders.rs`; `FixedClock`,
`advance_both`, `settle_boot_tick`, the fake-repo state seam):

- **P-L (tick emission pin)**: seeded with a stale off-chain delivery,
  an advanced tick emits the prune — observed through the typed seam
  (the fake repo's post-apply state / `repo.load()`): the row is gone
  and `active_notifications` output is unchanged across the tick.
  No timing assertions; the wake is `advance_both`.
- **P-M (suppression-intact pin)**: a stale *on-chain* delivery
  survives the tick (no prune emitted, no new `Fire` drained, the row
  persists in the fake repo) — the §3 loop-killer at the engine tier.
- **P-N (quiet tick stays quiet)**: no stale rows and no firings ⇒ the
  repo's apply counter does not move (typed counter, the
  `FlakyRepository` precedent).

### Bookkeeping (commit 3)

- `CHANGELOG.md` Unreleased: one entry — delivery proofs older than
  90 days are pruned automatically; restoring a very old due date may
  re-notify once (honest loss edge).
- `docs/architecture.org`: one paragraph extending the bounded-growth
  story (cascades → compaction → the age-bounded delivery sweep,
  engine-emitted, per-device).

### Non-goals

Dismissal-row aging (D1); muted-set pruning (bounded by task count);
snooze-link pruning (already compacted); config knob (D10);
refactoring the `due_firings`/`active_notifications` bodies (D14);
any UI change (deliveries are not rendered facts); migrations (D4).

## 9. Stage-2 execution notes

- Slice = §8's three commits; each builds and is green standalone
  (D13 forces the variant+twin vertical into one commit).
- Expected existing-suite impact: **none** — no existing R-series or
  snapshot test accumulates 90-day-old occurrences (advances are
  seconds/minutes-scale), so goldens and journeys pass untouched;
  a golden diff is a bug to diagnose, not to regenerate.
- Full AGENTS §10 gate suite through
  `scripts/run-quality-gates.sh` from the worktree; CHANGELOG only in
  commit 3. No push, no PR — stage 3 ships.
- Estimated diff: domain ≈ 250 lines (action, arm, helper, pins,
  proptests), storage ≈ 80 (partition, twin, contract/roundtrip pins),
  state ≈ 120 (emission + pins), docs/changelog ≈ 30. S/M confirmed.

## 10. Risks

- **R1 — one-time large first sweep** (long-lived kiosk upgrades with
  thousands of rows): per-row prepared deletes in one tx (D12);
  µs-scale per row; rejected the per-tick cap as complexity without a
  user-visible win.
- **R2 — predicate drift** between the sweep's consult condition and
  the readers' gates: P-G fails CI on any drift the readers can
  observe; the helper reuses the readers' own primitives (D14).
- **R3 — sweep cost per tick**: a delivery-set scan + per-reminder
  effective computations every 30 s; kiosk-scale trivial, and the
  post-sweep steady state keeps the set small.
- **R4 — clock jumps** (device clock moved forward then corrected):
  a forward jump may prune rows that a corrected-back clock makes
  consultable again; the loss is the same self-healing single toast
  (the re-fire re-inserts the row). Never over-prunes on backward
  jumps (the age test only widens with `now`).
- **R5 — gate-suite surprise in unrelated suites**: none expected
  (§9); any failure is investigated as a production-behavior question
  first per AGENTS §5, not papered over.

## 11. Stage-3 handoff

Close **#201** with: the bound statement (§5.2), the loop-proof
summary (§3/§5.3), the lockstep evidence (P-J), and an explicit
deferral note for dismissal-row aging (D1, pointing at the re-terming
comment already on the issue). The issue does not reopen for
dismissals — that concern is deployment-gated and lives in the close
comment as the record.
