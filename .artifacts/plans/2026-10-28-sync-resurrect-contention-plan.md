# [Plan] Resurrect-as-New-Card on Delete Contention (DR-4/D)

- **Date**: 2026-10-28
- **Kind**: stage-1 implementation plan for chunk DR-4/D of the
  design-rounds batch dispatch — the #130 delete-contention fix.
  TDD-first; this file is written before any code and is the chunk's
  contract.
- **Origin** (all normative):
  - `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
    §3 D-3 a–e (the design), §2 facts 5–7/14, §4 F-130 (automatic
    resurrect + `SystemEvent`, no prompt, no config — confirmed),
    §5 (the honest-policy risk), §6 (chunk D row).
  - `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
    §1 (wave 5, not a shape-widener), §3.1 (DR-4 binding notes:
    purity, A1 discipline, test discipline, strictly serial),
    §8.2 (chunk scope + acceptance + stage-3 review focus).
  - ADR 0004 (`docs/adr/0004-sync-conflict-policy.org`) — amended,
    never rewritten.
- **Branch / worktree**: `feat/sync-resurrect-contention` in
  `../taskboard-WT-feat-sync-resurrect-contention`, branched off
  `origin/main` @ `22b5f5d` (the local `main` ref is 28 commits
  behind — the worktree deliberately tracks `origin/main`, which
  carries DR-4/L's rebase-merged commits). Preconditions verified:
  DR-4/L merged (all 11 `feat/sync-orset-labels` commits present on
  `origin/main` with new post-rebase SHAs, subjects matched
  one-to-one); `scripts/worktree-doctor.sh` exit 0 (240G free, floor
  25G). No migration in this chunk (the family table's storage cell
  for #130 is `—`) — the migration ladder is untouched.
- **Lane check**: wave-mates DR-6/2 (`feat/series-engine`) and
  DR-5/4 (`feat/tracking-ui`) touch `command.rs`/`messages.rs` UI
  surfaces and slint respectively. This chunk edits `pipeline.rs`'s
  R3 branch, `merge.rs` (one pure predicate + one event variant in
  `messages.rs`), the engine's ingest/relay, and the status-bar
  surfacing. The merge/pipeline lane is serial by ruling — DR-4/D is
  the only DR-4 chunk in flight (DR-4/R waits for this PR). The
  `messages.rs` touch is one enum variant + one doc line, additive;
  the ui-slint touch is one passive status-bar slot. Both are inside
  the dispatch's expectations for this chunk ("TaskResurrected +
  GUI/CLI toast/status surfacing").
- **Status**: READY FOR STAGE-2. Every open question this round
  raised is resolved in §5 (decision record). No implementation has
  happened.

---

## 1. Mission & binding constraints

A remote card delete observed while this device holds **unsynced
local work** must not destroy that work: the task survives as a live,
unbound task with exactly one `CreateTask` op re-enqueued — the next
push mints a fresh remote card and the echo rebinds. A synced (or
tombstoned, or cascade-collapsing) card's absence keeps today's
delete-wins tombstone **byte-for-byte**. The user is told, once, on a
non-blocking surface.

Non-negotiable (design §1 + dispatch §3.1 + §8.2):

- **Purity**: `has_unsynced_work` is a pure function of injected data
  (the task and the reminder-rules map). No clocks read, no ids
  generated, no IO. The pipeline stays the policy executor.
- **No new persisted state**: no migration, no `Task`/`AppState`
  field, no `PersistedState` widening. The predicate is *derived*
  (D-3a); the event rides the broadcast lane, not the snapshot. This
  chunk is **not a shape-widener** — existing insta dumps and golden
  PNGs must not shift.
- **The 99% path is byte-identical**: a synced card's remote delete
  tombstones exactly as today — same `tombstone_task` stamp, same
  cancel-all, no event. Pinned by a snapshot scene.
- **No interactive prompt anywhere**: kiosk and daemon sessions never
  block on a conflict dialog (F-130). The surfacing is a passive
  status line (GUI), one CLI print, and a `tracing` event.
- **No config hatch** (D-3e ruled against; F-130's rejected
  alternative stays archived).
- **Test discipline** (AGENTS §5): typed outcomes, no text
  assertions (including the toast copy), proptest for the predicate,
  no quality ceilings, no sleeps (the S-scenes use the existing
  poll-until-published harness).

## 2. Verified baseline — numbered facts

All against the worktree @ `22b5f5d` (= `origin/main`); line numbers
from that tree.

1. The R3 task branch (`pipeline.rs:755-777`) is **unconditional
   delete-wins**: for every bound-but-absent card it inserts
   `tombstone_task(local, now)` and cancels every still-pending op
   targeting the task (`op_results.insert(op_id, false)` ⇒
   `PersistenceAction::FailOp`). There is **no `!local.deleted`
   guard** — a tombstoned task that kept its binding still gets its
   binding cleared (the R9c cache-lag safety comment at
   `pipeline.rs:756-758`).
2. `tombstone_task` (`merge.rs:804-818`) stamps
   `deleted = true`, `clocks.deleted = max(local, observed_at)`, and
   clears `remote`, `remote_seen`, `remote_baseline` together.
3. R5 (`merge.rs:309-314` inside `merge_task`, doc at :373-375) is
   **adoption, not contention**: the remote card is *present* and
   strictly newer than `clocks.deleted` ⇒ the tombstone clears. The
   R5 path drops the task's pending `DeleteTask` op
   (`pipeline.rs:647-659`). Untouched by this chunk.
4. R6 stack cascade (`pipeline.rs:850-880`) tombstone-wins today and
   keeps winning: its filter is
   `task.is_live() && task.remote == Some(ref into the dead stack)`.
   The cascade test fixture (`newer_stack_delete_beats_older_local_rename_and_cascades`,
   `pipeline.rs` tests) lists the card as **present** in the
   snapshot under the deleted stack (cache lag) — so R3 skips it
   (`present_refs` contains the ref) and R6 tombstones it.
5. `TaskClocks` has thirteen slots (`entities.rs:280-307`): twelve
   content fields (title, description, duedate, scheduled, done,
   position, labels, effort, importance, urgency, recur, archived)
   plus `deleted`. `Task.remote_seen: Option<DateTime<Utc>>`
   (`entities.rs:530-532`); `Task.remote_baseline`
   (`entities.rs:536-538`).
6. The R9 echo (`adopt_task_after_push`, `merge.rs:828-899`) takes
   the server's values as-is and advances **every** clock — the
   twelve content slots *and* `deleted` — to
   `max(local, echo.last_modified)`, with
   `remote_seen := Some(lm)`. After an echo, synced work compares
   `<= remote_seen` by construction.
7. Post-DR-4/L, label work lives in
   `Task.label_memberships: BTreeMap<LabelId, MembershipState>`
   (`entities.rs:514-515`), `MembershipState::{Added,Removed}{at}`
   (`entities.rs:424-…`). The **label verbs stamp only the
   membership** — `AssignLabel` inserts `Added { at: now }` and does
   **not** touch `clocks.labels` (`command.rs:977-1000`); the mirror
   holds for `UnassignLabel`. So a field-clock-only predicate would
   miss label work; the predicate needs its own membership clause.
8. The echo's membership translation (`adopt_echo_memberships`,
   `merge.rs:289-312`) restamps every membership to
   `max(at, lm)` — echo-present adds through the outcome table,
   echo-missing ids as `Removed{max(at, lm)}`. After an echo,
   **every** membership stamp is `>= remote_seen`, same discipline
   as the field clocks (fact 6).
9. `ReminderRule` (`reminders.rs:230-246`) carries `task: TaskId`
   and `updated_at` (the D-S9 embedded LWW clock). Rules live in
   `AppState.reminders.rules: BTreeMap<ReminderRuleId, ReminderRule>`
   (`reminders.rs:411-413`) — **not** on the task. The sidecar
   document flush completes its `UpdateAppData` ops with
   `Applied { echo: None }` and stamps nothing back — no echo ever
   advances `rule.updated_at` or `remote_seen` (flush.rs outcome
   arms).
10. `SystemEvent` (`messages.rs:436-463`) is a closed six-variant
    enum on the broadcast lane (fact 14 of the design plan).
    Exhaustive matches over it exist in the state engine's relay
    (`taskboard-state/src/actor.rs:523-533`, where `StorageFailed`
    already has the "engine is the producer" no-op arm), the app
    authority, and the app boot/drain tails — the compiler is the
    checklist for the new variant's arm sites.
11. `apply_sync_report` (`pipeline.rs:418-429`) returns
    `(AppState, Vec<PersistenceAction>)` and takes
    `ids: &dyn IdGenerator` + `now` — it can mint an op exactly like
    the command layer's `enqueue` helper (`command.rs:1870-1876`,
    `PersistenceAction::EnqueueOp(PendingOp { op_id:
    ids.new_op_id(), op, queued_at: now })`, action variant at
    `persistence.rs:191`). Destructuring call sites: 68 in
    `pipeline.rs` tests, 1 in `taskboard-state/tests/proptest.rs`,
    1 production (`taskboard-state/src/actor.rs:732`).
12. The engine's sync ingest (`ingest_sync_report`,
    `taskboard-state/src/actor.rs:706-…`, called from the select
    loop at :444) has the `system_out: broadcast::WeakSender<SystemEvent>`
    seam in scope (`EngineParts.system_out`, :316-325); the
    established emit pattern is `note_storage_failure`'s
    upgrade-or-drop (:1055-1070).
13. The GUI drains system events in the repaint driver
    (`taskboard-ui-slint/src/lib.rs:405-455`: `try_recv` loop →
    `record_connector_liveness_batch` → refresh) — today only
    connector liveness is recorded (`adapter.rs:1663-1681`, a
    wildcard match). The status bar (`ui/compositions/status-bar.slint`)
    is the bottom-chrome composition with passive slots (sync badge,
    startup findings, banner, flow hint, timers chip) and mounts in
    both desktop and kiosk windows (`app-window.slint:364`); kiosk
    gates are model-side and per-chip.
14. The CLI's `sync` verb (`taskboard-app/src/commands/sync.rs`)
    subscribes to the **engine-signal** channel for its wait
    (`wait_for_sync_result`, :171-219) and reports a typed
    `SyncOutcome`. It does not touch the system channel today; the
    verb is the natural CLI print point because a resurrect is
    always born inside a cycle the verb just triggered or awaited.
15. The document flush runs **last** in a cycle, keyed on post-echo
    card ids (`push.rs:399-403`); an unbound task's document entry
    is omitted with "no entry, no outcome — they ride the next
    cycle" (`sync-nextcloud/src/flush.rs:213-215`). A kept
    `UpdateAppData` op therefore re-flushes against the resurrect's
    new card id in a later cycle, losing nothing.
16. The S-series engine snapshot suite
    (`taskboard-state/tests/snapshots.rs`) runs the live engine over
    `InMemoryRepository` + `FixedClock` (one minute per message),
    feeds `SyncReport::Completed` through `report_tx` (s3's harness,
    :375-454), and `snap()`s the persisted whole state plus a
    memory-only tail; the scenario holds `system_tx` and can
    `subscribe()` to observe engine-emitted events. Highest scene
    number today: s36.

## 3. The design, decision by decision

### D-1 — the predicate (`merge.rs`, pure)

```rust
/// D-3a: `true` when the task holds user work the remote has never
/// observed — the delete-contention carve-out's input. Pure; derived
/// from entity state; no persisted flag (ADR 0004's #130 amendment).
#[must_use]
pub fn has_unsynced_work(
    task: &Task,
    reminder_rules: &BTreeMap<ReminderRuleId, ReminderRule>,
) -> bool
```

Clauses (disjunction — any one suffices), with
`seen = task.remote_seen.unwrap_or(epoch)` (epoch = the house
`BoardClocks::epoch()` constant pattern; `None` is unreachable for a
bound task — the disjunction-everything fallback is deliberately
resurrect-biased, the data-preserving direction):

1. **Field clocks**: `max` over the **twelve** non-`deleted`
   `TaskClocks` slots `> seen` (fact 5; `deleted` is excluded — it is
   the tombstone's clock, not user work, and the resurrect branch
   only runs on live tasks anyway).
2. **Memberships**: any `label_memberships` entry with `at > seen` —
   `Added` *and* `Removed` both count (a removal is work the server
   has not seen; fact 7 is why this clause exists at all — the verbs
   never touch `clocks.labels`).
3. **Reminder rules**: any rule with `rule.task == task.id &&
   rule.updated_at > seen` (facts 9; the sidecar lane's user data).

Why *synced* work compares `<=`: a drained push's echo maxes all
twelve field clocks (fact 6) and every membership stamp (fact 8) to
`>= echo.last_modified = remote_seen`, so a fully-synced task's
clauses 1–2 are false. Clause 3 has **no echo stamp-back** (fact 9):
a rule edit whose document flush already landed keeps the clause true
until the *card's* next change advances `remote_seen` past
`updated_at`. That residue errs toward resurrect (data-preserving)
and is recorded in the ADR amendment as the accepted limitation —
mirroring the D-1d blind-spot treatment, not a `currently_pins_` pin
(it is a property of the design, deterministic, not tolerated debt
awaiting removal).

Home: `merge.rs`, directly after `tombstone_task` — the two are the
R3 policy pair (tombstone vs resurrect), and `merge.rs` already
hosts the reminder-fold algebra (`fold_reminder_rules`, :691). Not
an A1 type (D-3a: "no A1 minting needed"); no `Task` method (the
rules parameter would lie about entity locality).

### D-2 — the R3 branch rewrite (pure re-planning, `pipeline.rs`)

The task branch of §6 splits into two outcomes. The **branch
condition** is three conjuncts — the two scope guards plus the
predicate:

```text
resurrect ⟺ local.is_live()                      (guard 1: tombstones never resurrect)
           ∧ present_stack_refs.contains(card's stack ref)   (guard 2: cascade territory ⇒ tombstone-wins)
           ∧ has_unsynced_work(local, rules)     (the carve-out itself)
```

Guard 2 is D-3d made **mechanical**, and the seams forced it: a
card absent from a snapshot whose *stack* is also absent is a
collapsing world — resurrecting there would leave a live unbound task
whose stack is about to tombstone (R6's filter skips remote-less
tasks, fact 4), and the re-create would push into a dead stack. The
`present_stack_refs` set (currently computed at `pipeline.rs:778`,
after the task loop) hoists above the task loop — a pure re-derive
from the snapshot, no entity-reconciliation reordering. Both cascade
shapes stay byte-identical: card-present-stack-absent (R6's cache-lag
fixture, fact 4) never enters the branch; card-absent-stack-absent
enters and fails guard 2.

The **resurrect arm** — pure re-planning, in order:

1. `remote = None`, `remote_seen = None`, `remote_baseline = None`
   (cleared *together* — a surviving stale baseline would corrupt the
   next merge's diff, the DR-4/G inventory rule; cleared by direct
   field assignment, **not** `tombstone_task`, which would stamp
   `deleted`). Everything else stays: clocks, memberships, content —
   the work is the point.
2. Cancel every still-pending op targeting the task **except
   `UpdateAppData`** (`op_results.insert(op_id, false)` as today;
   the lane filter is `op_targets_task(op, id) && !matches!(op,
   LocalOp::UpdateAppData(_))`). Rationale per op: `UpdateTask`/
   `MoveTask`/`AssignLabel`/`UnassignLabel` content is subsumed — the
   create full-sends the whole card body including the label set
   (D-3b's "the existing op is the recreate op"; a create+update pair
   would be redundant). A `DeleteTask` on a live task does not exist
   (the command tombstones synchronously) but is cancelled anyway if
   the impossible happened. `UpdateAppData` **keeps riding**: the
   flush runs last against post-echo card ids and an unbound entry
   simply rides to the next cycle (fact 15) — cancelling it would
   orphan the task's sidecar content, the exact loss class this
   chunk exists to prevent.
3. Enqueue exactly one
   `PersistenceAction::EnqueueOp(PendingOp { op_id: ids.new_op_id(),
   op: LocalOp::CreateTask(id), queued_at: now })` — the command
   layer's `enqueue` shape (`command.rs:1870`), minted by the
   pipeline's own `ids`/`now` parameters. No second op of any kind.
4. Emit `SystemEvent::TaskResurrected(id)` (D-3) and
   `tracing::debug!` with `rule = RULE_R3, outcome = "resurrected"`
   next to the existing `"task absence tombstoned"` record.

The **tombstone arm** is today's code, verbatim (fact 1): the
`tombstone_task` insert + the cancel-**all** loop (including
`UpdateAppData` — a dead task's sidecar entries die with it; the
document GC prunes them). Synced-live and already-tombstoned tasks
both land here, unchanged.

Failure semantics inherit the offline-create paths: a rejected
create dead-letters the op and leaves the task live-unbound — the
same resting state as any offline create (s4's dead-letter tier pins
the mechanic); a later re-edit re-enqueues. Nothing new to build.

### D-3 — the event + surfacing contract

**Variant**: `SystemEvent::TaskResurrected(TaskId)` — internal-additive
on the broadcast lane (fact 10). Serde: externally-tagged like its
siblings; the round-trip test extends `connector_system_events_round_trip_through_serde`'s
pattern with its own test.

**Channel shape**: `apply_sync_report` returns a third element —
`(AppState, Vec<PersistenceAction>, Vec<SystemEvent>)`. The events
vector is the report's verdict, symmetric with the actions vector;
the ~70 destructuring call sites are a mechanical one-line touch
(`let (app, actions) = …` → `let (app, actions, events) = …`;
unused sites name it `_events`). Rejected: a
`PersistenceAction::EmitSystemEvent` variant (the storage crate's
`apply_actions` would have to match a non-storage action — an SRP
violation across the crate boundary) and an engine-side derivation
from the batch's action fingerprints (the engine would be
re-implementing policy recognition — legislating, not
orchestrating).

**Engine forwarding**: `ingest_sync_report` forwards each event on
the `system_out` weak sender (fact 12) using the
`note_storage_failure` upgrade-or-drop pattern. The relay's
exhaustive match gains `Ok(SystemEvent::TaskResurrected(_)) => {}`
with the `StorageFailed` comment's shape — the engine is the
producer, consumers are downstream. Every other compiler-forced
match site (authority, boot/drain tails, sync actor) gets a
pass-through arm; the compiler is the checklist.

**GUI** (dumb client, passive, non-blocking): the adapter gains a
transient notice slot — `notice: Option<Notice>` where `Notice {
until: DateTime<Utc> }` — recorded by a sibling of
`record_connector_liveness` in the repaint driver's existing system
drain (fact 13; no new timer: expiry rides the existing cadence
tick's wall-clock read, the same `chrono::Utc::now()` the minute
boundary already uses; expiry horizon: 10 s, a presentation
constant). The status bar gains `notice-text: string` +
`notice-visible: bool` properties, **default invisible** — existing
goldens and dumps do not shift. The phrase is a pure
`presentation.rs` function (the `notification_phrase` precedent)
composed from the task's title at refresh time; per AGENTS §5 the
tests assert structural facts (non-empty, deterministic), never the
copy. No kiosk gate: the line is passive text wherever the status
bar renders — the kiosk constraint is *never block*, and a line
never blocks. No `ActionId`: the notice is not clickable (scope
edge — focus-the-task is a UI nicety for a future round).

**CLI**: the `sync` verb subscribes `app.system` before nudging
(next to the existing engine-signal subscription, fact 14), drains
after the wait resolves, and a pure fold —
`fn resurrected_tasks(events: &[SystemEvent]) -> Vec<TaskId>` —
drives one honest status line per resurrected task (title resolved
from the shared snapshot). Unit-tested typed (`matches!` + ids),
never on the printed text. Daemon/`--no-wait` sessions surface via
`tracing` only — which every path gets anyway.

### D-4 — what does NOT change (scope edges, ruled)

- **R5** (`merge.rs:309-314`): untouched — a present card's newer
  write clearing a local tombstone is adoption, not contention. Its
  doc comment gains one cross-reference sentence pointing at the R3
  carve-out so the pair stays readable together; zero behavior.
- **R6 cascade** (`pipeline.rs:850-880`): byte-identical —
  tombstone-wins even under unsynced edits (D-3d; fabricating a new
  stack is a different policy question, recorded as the documented
  limitation in the ADR amendment). Guard 2 (D-2) is what keeps R6's
  territory out of the resurrect branch; R6 itself grows nothing.
- **The synced path**: byte-identical (§1's third constraint).
- **R9/R9c**: untouched; `finalize_pushed_task_delete` already
  clears the trio the resurrect clears.
- **Stacks/labels/boards R3 branches**: untouched — the carve-out is
  cards-only (F-130's subject; stacks cascade by ruling, labels are
  entity-death-dominated since DR-4/L).
- **No migration, no persisted shape change, no config** (D-3e).
- **Undo v1**: not this chunk's concern — the journal's inverse
  synthesis is undo/1's exhaustive match over `StateCommand`, and no
  command surface changes here.

### D-5 — ADR 0004 amendment (dated, amendment-not-rewrite)

One dated block appended to `docs/adr/0004-sync-conflict-policy.org`
(2026-10-28), amending **§Deletion**'s card bullet — the sentence
"*an observed remote absence always beats concurrent local edits*
(delete-wins tombstone, stamped with the observation time)" gains
the carve-out, and the two following sentences ("Automatic
resurrection would create zombie tasks…", "Resurrect-as-new-card is
backlogged.") are superseded. Wording:

> **Amendment 2026-10-28 (#130, delete/edit contention):** an
> observed remote absence tombstones the card — *unless the task
> holds unsynced user work*, derived as `has_unsynced_work`: any
> field clock, label-membership stamp, or the task's reminder-rule
> `updated_at` strictly newer than `remote_seen` (echo adoption
> maxes the first two, so synced work compares `<=`). Under
> contention the task survives as a live, unbound task with exactly
> one `CreateTask` re-enqueued — the next push mints a fresh remote
> card and the echo rebinds; a second delete of that card is an
> ordinary synced delete (one new card per contention episode).
> Absences inside a collapsing stack are cascade territory and keep
> tombstone-wins, as do already-tombstoned tasks. Contention
> surfacing is the non-blocking `SystemEvent::TaskResurrected`
> (status line / CLI line / tracing) — never a prompt: kiosk and
> daemon sessions must not block. Accepted residue: the
> reminder-rule clause has no echo stamp-back, so a rule edit whose
> sidecar flush already landed can bias toward resurrect until the
> card's next change — the data-preserving direction.

## 4. Test plan (TDD order)

Tier discipline: one behavior, one test, lowest layer. The pipeline
owns the re-planning semantics, the S-series owns the whole-state
disk contract, the surfacing tiers own their folds. No test asserts
text (AGENTS §5) — typed/structural outcomes only.

**T1 — predicate unit catalogue (`merge.rs` tests, pure).** One test
per clause edge, over constructed tasks:

- each clause alone fires (field clock max; an `Added` membership; a
  `Removed` membership — the removal-is-work pin; a task's rule) —
  all with `remote_seen` between the stamps;
- the synced baseline: echo-shaped task (all twelve clocks and every
  membership `>= remote_seen`, no fresh rules) ⇒ false — the 99%-path
  input, stated as data;
- mixed: synced fields but one fresh membership ⇒ true;
- another task's rules don't leak (`rule.task` filter);
- `remote_seen: None` ⇒ true (the defensive epoch fallback);
- the design's one-sentence property as a **proptest**: for
  arbitrary stamps, `has_unsynced_work` is monotone in "work newer
  than `remote_seen`" — concretely: shifting every work stamp below
  `remote_seen` flips it false, any single work stamp above flips it
  true.

**T2 — R3 re-planning scenes (`pipeline.rs` tests).** The branch's
own layer; each scene one behavior:

- `remote_absence_resurrects_unsynced_work_as_new_card`: bound live
  task, field clock > `remote_seen`, absent card ⇒ live, unbound,
  trio cleared, exactly one `EnqueueOp(CreateTask)` in the actions,
  the pre-existing `UpdateTask` op in `op_results` as `false`, and
  the events vector is exactly `[TaskResurrected(id)]`;
- `remote_absence_resurrect_keeps_the_document_lane`: a pending
  `UpdateAppData` op survives the resurrect untouched (not in
  `op_results`) while a sibling `UpdateTask` cancels — the lane
  table's pin;
- `remote_absence_resurrect_preserves_work_and_memberships`: clocks
  and `label_memberships` byte-equal across the branch (no stamping);
- `synced_absence_is_byte_identical_today`: existing
  `remote_absence_tombstones_and_cancels_pending_ops` (:2165) keeps
  passing **unchanged**, plus an explicit empty-events assertion on
  a synced-absence scene (no event on the 99% path);
- `cascade_absence_never_resurrects` ×2: the R6 fixture shape
  (card present, stack absent, unsynced edits ⇒ R6 tombstones, no
  event) and the both-absent shape (⇒ tombstone via guard 2, no
  event);
- `tombstoned_task_absence_stays_tombstone`: `deleted: true` bound
  task, absent card ⇒ today's re-stamp path even with fresh rule
  stamps (guard 1);
- the never-deleted **proptest**: for arbitrary bound live tasks
  with `has_unsynced_work` true whose stack ref is present in an
  arbitrary absent-card snapshot, the merged task is `!deleted` —
  the design's property sentence with the D-3d carve-out made
  explicit (decision 5-8);
- the loop pin: `resurrect_then_rebind_then_second_delete_tombstones`:
  resurrect output → apply `adopt_task_after_push` echo → feed a
  second absence ⇒ tombstoned, empty events (one new card per
  episode; complements the engine-tier s39).

**T3 — event plumbing (`messages.rs`, `taskboard-state`).**

- `task_resurrected_round_trips_through_serde` (the connector
  variant's pattern);
- snapshots.rs gains the four scenes below; additionally one
  `engine.rs`-tier assertion (inside s38's scene, not a separate
  test): after the report settles, `system_tx.subscribe()` yields
  exactly one `TaskResurrected` — the broadcast-forwarding contract.

**T4 — the S-series scenes (`taskboard-state/tests/snapshots.rs`,
additions only).** Harness per s3/s4; each `snap()`s the persisted
whole state — the disk contract of the re-planning (task row shape,
outbox rows, op outcomes):

- `s37_synced_card_absence_disappears` — bound synced task, snapshot
  without the card ⇒ tombstoned, binding cleared, no op churn, **no
  event**. The byte-identical pin at the disk-contract tier.
- `s38_unsynced_edit_resurrects_as_new_card` — bound task with a
  pending `UpdateTask` (edit after last sync), snapshot without the
  card ⇒ live unbound task, `UpdateTask` failed, one `CreateTask`
  queued, event observed on the broadcast.
- `s39_second_absence_after_rebind_tombstones` — s38, then the
  create's echo applied (`PushOutcome::Applied` with echo), then a
  second snapshot without the (new) card ⇒ tombstoned, no event.
- `s40_cascade_absence_stays_tombstoned` — the stack-absence fixture
  with unsynced card edits ⇒ tombstoned, no event.

Existing dumps must not shift: no current scene carries a
bound-but-absent card with unsynced work (verified against s1–s36's
fixtures), the tombstone path is byte-identical, and the status-bar
properties default invisible. Any `.snap` diff outside the four new
scene pairs is a finding.

**T5 — surfacing folds (per-surface tiers).**

- CLI: `resurrected_tasks` fold — typed unit tests (empty in,
  dedup?, order-preserving out; non-`TaskResurrected` variants
  ignored). Dedup: no — duplicates are honest (two episodes, two
  lines); pin the preserving order instead.
- GUI: the adapter's notice record/expiry as pure decision fns
  (`notice_visible(now, notice)`, the expiry constant) unit-tested
  structurally; the phrase fn asserted non-empty + deterministic.
  No golden changes.

**T6 — mutant targets** (the chunk's zero-expected-survivors set):
the predicate's three clauses; the three resurrect conjuncts
(especially guard 2); the lane-table exception (a mutant that
cancels `UpdateAppData` dies on T2's lane pin); the
exactly-one-create enqueue; the event emission site.

## 5. Decision record (every open question, resolved)

1. **Predicate home & signature** — `merge.rs`, free function next to
   `tombstone_task`, taking `(task, reminder_rules)` (§3 D-1). The
   rules map is a parameter because rules live in `ReminderState`,
   not on the task; a `Task` method would hide that. Rejected:
   `pipeline.rs` (composition, not algebra) and an `impl Task`
   (misleading locality).
2. **Clause inventory** — twelve non-`deleted` field clocks + all
   membership stamps + the task's reminder rules (§3 D-1). Facts 7/8
   forced the membership clause: the OR-set verbs never touch
   `clocks.labels`, so the design's "max(native field clocks)"
   wording is implemented as three clauses with the same meaning;
   recorded here so stage 2 does not "simplify" back to
   clocks-only.
3. **`remote_seen: None` fallback** — epoch comparison ⇒ resurrect
   (data-preserving). Unreachable for bound tasks (every adoption
   path stamps it); defensive only.
4. **Resurrect guards** — `is_live()` ∧ stack-ref-present ∧
   predicate (§3 D-2). Guard 1 keeps the ADR's zombie fear dead (a
   locally-deleted task's last intent was deletion); guard 2 is
   D-3d mechanical — discovered from the seams (R6's filter skips
   remote-less tasks, fact 4): without it a resurrected task could
   outlive its stack and re-create into a dead stack.
5. **Op lane table** — cancel native-lane ops
   (`UpdateTask`/`MoveTask`/`AssignLabel`/`UnassignLabel`/`DeleteTask`),
   keep `UpdateAppData` (§3 D-2). The create's full-send subsumes
   the native lane; the document lane's ride-the-next-cycle
   semantics (fact 15) plus the post-echo card key make keeping it
   lossless — cancelling would orphan sidecar content.
6. **Exactly one `CreateTask`** — minted by the pipeline via
   `ids.new_op_id()`/`now` in the `enqueue` shape (§3 D-2). It is
   the recreate op the issue asks for; no create-then-update pair.
7. **Event channel** — third tuple element from `apply_sync_report`
   (§3 D-3). ~70 mechanical call-site touches; rejected
   `PersistenceAction` variant (storage-contract pollution) and
   engine-side fingerprinting (policy in the wrong layer).
8. **Property scoping** — the never-deleted property carries the
   D-3d carve-out explicitly: "never reaches `deleted: true` through
   the R3 task branch" = guard set satisfied (live, stack present).
   The design plan contains both sentences (D-3 tests vs D-3d);
   this resolves the apparent tension in favor of both, each in its
   scene: the property (T2) and the cascade pin (T2 + s40).
9. **The rules-clause residue is a documented limitation, not a
   `currently_pins_` pin** — it is deterministic design behavior in
   the data-preserving direction (§3 D-1, §3 D-5), unlike the skew
   pins (D-4b) which document unintended-but-tolerated behavior.
   No test name uses `currently_pins_` in this chunk.
10. **GUI surfacing** — passive status-bar notice with a 10 s
    wall-clock expiry riding the existing cadence tick; default
    invisible; no kiosk gate; no click action (§3 D-3). Adapter-held
    presentation state matches the `PendingReply`/liveness-map
    precedents; "no UI-held timer state" was a tracking-chunk ruling
    about domain-derived timers, not presentation transients.
11. **CLI surfacing** — the `sync` verb prints per-event lines via a
    pure fold; `--no-wait` and daemon stay tracing-only (§3 D-3).
12. **Expiry horizon** — 10 s, presentation constant. Arbitrary but
    fixed; the pin asserts the mechanism (expiry clears), never the
    number's value in copy.
13. **R5 cross-reference** — one doc-comment sentence in
    `merge.rs` (§3 D-4); no behavior. The prompt's seam note
    ("resurrection there is adoption-not-contention") is already
    the comment's substance; the link completes it.

## 6. Risks

- **A stale baseline / stale `remote_seen` surviving the resurrect**
  corrupts the next merge — the stage-3 review focus calls this out
  by name; T2's trio-cleared pin and the s38 disk contract both pin
  it (three fields, one insert).
- **The 70-call-site signature churn** touches every pipeline test —
  mechanical but wide; a misnamed `_events` is caught by clippy
  `-D warnings`, not by review vigilance. The four new scenes
  destructure `events` for real.
- **Guard 2's hoist** moves a snapshot-derived set above the task
  loop — a pure reordering, but the R6 cascade tests are the
  tripwire: they must pass untouched (fact 4's fixture pins the
  card-present shape R3 must keep skipping).
- **Resurrect pushes content the remote just lost** — the honest
  policy (design §5): someone deleted the card while this device
  held unsynced edits, and the edit wins. The notification is the
  mitigation; the config hatch stays a rejected alternative,
  revivable cheaply if a user asks (F-130's archive note).
- **The rules-clause bias** (fact 9): a flushed rule edit biases
  toward resurrect until the card's next change — one extra new card
  in a narrow race, data-preserving, documented in the ADR block
  (decision 9).
- **Wave-mate overlap on `messages.rs`** (DR-6/2 also touches
  command/message surfaces): this chunk's touch is one additive
  enum variant; any semantic conflict is a rebase-time text fix,
  not a design conflict. The merge/pipeline lane's serial ruling
  already keeps DR-4/R out of the way.

## 7. Estimated diff size

| Area | Shape | Est. |
|---|---|---|
| `taskboard-domain/src/merge.rs` | predicate + unit catalogue + proptest + R5 doc sentence | +170 |
| `taskboard-domain/src/pipeline.rs` | R3 split + guard-2 hoist + events return; **68 mechanical destructuring touches**; T2 scenes + property | +260 / −40 |
| `taskboard-domain/src/messages.rs` | variant + serde test | +20 |
| `taskboard-state/src/actor.rs` | ingest forwarding + relay arm (+ pass-through arms elsewhere as the compiler demands) | +25 |
| `taskboard-state/tests/snapshots.rs` + 8 `.snap` files | s37–s40 (additions) | +320 |
| `taskboard-state/tests/proptest.rs` | one destructuring touch | ~1 |
| `taskboard-app/src/commands/sync.rs` | system subscription + fold + tests | +45 |
| `taskboard-ui-slint` (adapter, presentation, status-bar.slint, tests) | notice record/expiry, phrase fn, two properties | +95 |
| `docs/adr/0004-sync-conflict-policy.org` | the dated §Deletion amendment | +20 |

Total ≈ **900–1000 lines**, of which ~60% is tests and ~25% is the
mechanical destructuring sweep. No migration, no generated-code
regeneration, no golden shifts.

## 8. References

- Design: `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
  §3 D-3, §2 facts 5–7/14, §4 F-130, §5, §6.
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1, §3.1, §8.2.
- ADR 0004 (`docs/adr/0004-sync-conflict-policy.org`) §Deletion; the
  2026-10-28 labels amendment directly above the new block.
- Issue #130 (closes with this chunk's PR, stage 3); implementation
  issue #483.
- Sibling stage-1 plans: `2026-10-27-sync-remote-baseline-plan.md`
  (DR-4/G, the baseline this chunk clears), the OR-set labels plan
  (DR-4/L, the membership stamps the predicate reads).
