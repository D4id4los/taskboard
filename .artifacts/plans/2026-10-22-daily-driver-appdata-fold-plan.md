# [Plan] Daily-Driver Chunk 4 — Sidecar Fold + Flush + Fences + Rules Section + Op Routing + Surfacing

- **Date**: 2026-10-22.
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1/§5 (MD-1…MD-10); wave implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-6/7/8/9 (+ DDF-2 as amended 2026-10-22), §3 chunk 4, §4–§6
  (incl. the MD-10 interim this chunk closes); sidecar design doc
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` §3.3
  (D-S3), §3.4 item 4 + the D-S7 fence table, §3.5 (D-S5), §3.6
  (D-S6), §3.7–§3.9 (D-S7/D-S8/D-S9), §5 (the binding merge-policy
  clauses), §6 (ACCEPTED record + the six session rulings);
  reminders-surfaces design
  `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md` §3.2
  (D-R2, the enqueue-site enumeration); field-family design
  `.artifacts/plans/2026-10-18-field-family-design-plan.md` §3.3 D-F3
  (assertion 4's mechanics) and §6 (the enqueue contract); the
  version-capabilities addendum
  `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`
  (D-V1/D-V2/D-V3, §4's chunk-4 obligations). All ACCEPTED rulings are
  binding (MD-7); every decision below stays inside them — the two
  interpretation calls are flagged in §7.
- **Branch / worktree**: `feat/sync-appdata-fold` in
  `../taskboard-WT-feat-sync-appdata-fold`, off `origin/main` @
  `73538b0`. **MD-9 verified 2026-10-22**: chunk 3's merge commits
  (`5fe58ca`…`302309f` + docs `5d5f7b0`/`73538b0`) are on
  `origin/main`. `scripts/worktree-doctor.sh` exited 0 (115 G free,
  floor 25 G). Worktree note: the worktree created at stage-1 start
  was removed mid-session by an outside actor (not by this agent); it
  was recreated on the same branch at the same commit after re-running
  the doctor (exit 0) — no work was lost (nothing had been written
  yet). A concurrent session was observed recreating its own
  (KEEP-classified) worktree next door; per the AGENTS §8 lifecycle
  both behaviors are the documented dance.
- **Goal**: the sidecar's fold + engine half — the interim closes
  (MD-10): app-field edits and rule mutations enqueue
  `UpdateAppData`, converge through the document lane, and surface
  their health. Per-board: fold the downloaded document into local
  state behind the guard mirror; flush local truth back as a
  read-modify-write union between the cycle's pull and its report;
  strand loudly per the D-S7 fence table; route every manifest row's
  op per the lane rule (assertion 4); render the format-fence warning
  through CLI `status`; guard the anchor card against app-layer
  rename/delete. Closes #207 + #244; annotates #202 (stage 3's PR).

## 1. Verified baseline (symbols re-located at this plan's start; lines are plan-day references)

All verified in the worktree @ `73538b0`.

| Symbol / seam | Home (today) | State relevant to chunk 4 |
|---|---|---|
| `LocalOp::UpdateAppData(TaskId)` | domain `outbox.rs:59` | landed (chunk 3); `owning_board` + `op_targets_task` arms in; nothing enqueues it yet (MD-10) |
| codec tag `update_app_data` | storage `codec.rs:114/217` | landed; outbox round-trip pinned |
| `AppDataEvidence` / `AppDataUnreadable` | domain `messages.rs:374/388` | landed; `SyncReport::Completed { app_data }` in; `Failed` carries none |
| `SyncReport::Completed` send site | sync `actor.rs:623` | `sidecar_evidence` (actor.rs:727) downloads + classifies; `anchor_action(&discovery, false)` hardcodes *no* document work (actor.rs:758) |
| `LoopState.documents` / `CachedAppData` | sync `actor.rs:166-197` | in-memory metadata+document cache; success-only insert; pruned with the derived set |
| `LoopState.features` | sync `actor.rs:179` | probed once per actor lifetime (`probe_features`, actor.rs:308); consumed by `PushExecutor::new` (capability-gated card conversion, `mapping.rs:224/289`) |
| capability seam | domain `capabilities.rs` | `DeckFeatures { card_startdate }`, `flags_for`, `CONSERVATIVE`/`NATIVE`; pure, serde-less today |
| document codec + `union` + `prune_absent_cards` + `MODELED_TASK_FIELD_KEYS` | domain `appdata.rs` | landed; version-blind; assertion 3 (reworded) pinned in its tests |
| anchor ceremony | sync `anchor.rs` | scan/pick/`anchor_action(discovery, has_document_work)`/`creation_target` (dead_code until this chunk) |
| client surface | sync `client.rs` | `card` :600, `upload_attachment` :681, `download_attachment` :724, `update_attachment_content` :848, `delete_attachment` :888, `restore_attachment` :926, `create_card` :1094 |
| `PendingTaskIntent` + `pending_task_intents` + `keep_task_intent` | domain `pipeline.rs:93/112/171` | all twelve flags present incl. the four app fields + `reminder_rules`; the `UpdateAppData` arm `continue`s (pipeline.rs:140, the fence comment names chunk 4) |
| `apply_sync_report` | domain `pipeline.rs:394` | sections 1 push → 2 board → 3 stacks → 4 labels → 5 tasks → 6 R3 → 6b cascade → 7 status; **no evidence, no features parameter yet**; `reminders` carried through untouched behind the fence comment (pipeline.rs:782) |
| `apply_push_report` / `apply_push_outcomes` | domain `pipeline.rs:889/1004` | echo adoption via `adopt_task_after_push` + `keep_task_intent`; `Rejected { BadRequest }` → `FailOp` dead-letter precedent (pipeline.rs:1243-1263); `UpdateAppData` arms are match-totality placeholders (pipeline.rs:1070/1239) |
| `merge_task` scheduled verdict | domain `merge.rs:205` | unconditional today — the D-V2 gate is this chunk's |
| `adopt_task_after_push` scheduled take | domain `merge.rs:327` | `scheduled: echo.scheduled` — **the echo-path clobber hazard** (§3 D5) |
| `plan_pushes` / `PushGroup` / `MaterializedOp` | domain `push.rs:331/235/34` | buckets per entity (stacks → labels → tasks); no document group yet |
| outcome assembly | sync `push_exec/outcomes.rs` | `primary_outcome` (Applied completes primary+subsumed), `reject_kind` (failure completes primary only), `classify` (transport → `Aborted`) — the flush reuses these shapes |
| `SetReminderRule` / `ClearReminderRules` | domain `command.rs:1107/1128` | no enqueue (the "rules are local-only" comment — this chunk reverses); `SetReminderRule` = cascade-delete + fresh rule; `task_rule_deletes` (command.rs:263) is the cascade |
| `ReminderRule` | domain `reminders.rs:109` | `created_at` only; the reservation comment (:117) names this chunk's demand |
| rule persistence | storage `repo.rs:152/425` | `ReminderRuleRow` has no `updated_at`; 0007's column (`DEFAULT '0001-01-01T00:00:00+00:00'`) exists unread |
| `EditTask` lane split (the MD-10 interim) | domain `command.rs:373-390` | native slots → `UpdateTask`; app-only edits stay local ("Chunk 4 reverses exactly the app-slot half") |
| `AppState` / `SyncStatus` / `SyncStates` | domain `state.rs:25/49/147` | `SyncStatus { phase, last_success, pending_ops, counters }`; `counters` is the memory-only `#[serde(default)]` precedent |
| `attributed_board` | domain `state.rs:299` | `UpdateAppData` arm in (chunk 3) — `pending_ops` accounting rides |
| engine ingestion | state `actor.rs:694-740` | `app_data: _` acknowledged ("compilation, not fold logic") |
| status surfaces | app `commands/status.rs` | `StatusView`/`BoardStatusView`/`SyncStatusView` (output.rs) + `render_text`/`board_line`; additivity via `#[serde(default, skip_serializing_if)]` |
| task verbs | app `commands/tasks.rs` | `edit_on` :410, `delete_on` :454 — both resolve the task first; the CLI and REPL lanes both dispatch through them |
| tier-2 corpus | sync `tests/it_docker.rs` + `tests/common/mod.rs` (+ `common/sync_pair.rs`) | `boot_pair` engine+actor harness, `eventually` polls, `live_config()` honest-skip, `run_id` namespacing, `two_way_round_trip` as the two-device template |

## 2. Shape of the chunk (one board's cycle, after this chunk)

```
read_at stamped → read state
push phase   : plan_pushes → card/stack/label groups … [document group planned, DEFERRED]
pull phase   : pull_snapshot → sidecar_evidence (anchor_action(work=true)):
                 Document | Absent | Unreadable(reason)
flush        : (only when the board planned a document group OR a rebuild trigger fires)
               union(cached/downloaded document, local entries as of read_at)
               → lane-GC (scheduled dropped while CARD_STARTDATE)
               → prune_absent_cards(snapshot card set)
               → upload (create/adopt/restore anchor first when needed)
               → outcomes for the group's resolved ops; LoopState metadata refreshed
report       : Completed { snapshot, validators, pushes+flush outcomes, read_at, app_data, features }
engine       : apply_sync_report:
                 push outcomes (echo adoptions; scheduled kept while flag false)
                 board/stacks/labels/tasks merges (native verdict; scheduled verdict gated)
                 ▸ THE FOLD: tasks section via task_by_board_card (fold_task_app_data,
                   guard copy inside), then rules section into AppState.reminders
                 R3 presence (the fold fed it nothing)
                 sidecar health derived from app_data → the board's SyncStatus row
                 features recorded → the board's SyncStatus row
```

## 3. Decisions (every open question settled here)

### D1 — `fold_task_app_data`: exact signature and semantics

Domain `merge.rs`, a `merge_task` sibling (D-S3's illustration + the
D-V2 gate parameter):

```rust
pub fn fold_task_app_data(
    merged: &mut Task,          // post merge_task + keep_task_intent
    local: &Task,               // pre-merge local truth (guard source)
    entry: &TaskEntry,          // the document's entry for this card
    intent: PendingTaskIntent,  // the guard flags (app fields + reminder_rules)
    native_scheduled: bool,     // D-V2: the connector's CARD_STARTDATE flag
)
```

Per modeled key `k ∈ {effort, importance, urgency, scheduled}`:

- **Lane gate**: the `scheduled` key folds only while
  `!native_scheduled`; while the flag is true the key is skipped
  (native owns the field; the document's stale key must not write).
  The other three keys fold unconditionally. The gate is a *parameter*
  — the function never names `DeckFeatures` (assertion 3's layer
  rule: the decision lives in the sync/pipeline layers).
- **Clock rule** (embedded-clock LWW, D-S3): let `d` = the entry's
  clock for `k`, `l` = `local.clocks.k`.
  `d ≥ l` ⇒ `merged.k = value`, `merged.clocks.k = max(l, d)`;
  `d < l` ⇒ local stands (`merged` already holds it — the fold's
  input fields are untouched by `merge_task`/`keep_task_intent`).
  `null` value = a cleared-field write, competing by the same clock.
- **Guard copy inside the fold** (D-S6): when `intent.<k>` is set, the
  fold result takes `local.k` **and** `local.clocks.k` — overriding
  whatever the document said. Equivalently: the guard skips the key's
  fold verdict. Specified as copy-over (the ruling's wording) — both
  formulations produce the same state; tests pin the observable.
- **The card stamp is never read.** The function takes no
  `last_modified`, no `remote_seen`; the two clock domains stay
  disjoint (a stage-3 trap: any read is a critical finding).
- **Absence is not deletion**: a key absent from `entry.fields` writes
  nothing; a whole entry absent folds nothing (the pipeline simply
  never calls the fold for it).
- **No presence facts**: the function returns nothing and touches no
  index; R3 stays snapshot-only.
- Unknown/unmodeled entry content never folds (the codec already
  quarantines it as `unmodeled`).

### D2 — the rules fold: exact signature and semantics

Domain `merge.rs`, the D-S3 "rules counterpart", one resolved task at
a time (the pipeline resolves card → task via `task_by_board_card`
first; unresolvable cards never reach it):

```rust
pub fn fold_reminder_rules(
    rules: &mut BTreeMap<ReminderRuleId, ReminderRule>, // the live rule map
    task: TaskId,                                       // the resolved task
    entry: &RulesEntry,                                 // the card's document entry
    protected: bool,                                    // the task's guard flag
) -> RuleFoldChanges                                    // typed deltas, see below
```

- `RuleEntry::Rule { offset, created, clock }`, keyed by the rule
  UUID (the document *is* the rule's remote identity, D-S9):
  - local rule with that UUID exists: `clock ≥ rule.updated_at` ⇒
    update local (`offset`, `created`, `updated_at = max(local,
    document)`) and report an `Upsert`; `clock < updated_at` ⇒ local
    stands.
  - no local rule: **adopt** — insert
    `ReminderRule { id: <document uuid>, task, offset, created,
    updated_at: clock }` (the fold adopts the document's id verbatim,
    D-S9) and report an `Upsert`.
- `RuleEntry::Tombstone { clock }`: removes the local rule iff
  `clock > rule.updated_at` (strictly — a rule whose clock is newer
  stands: re-create after delete) and reports a `Delete`; no local
  rule ⇒ nothing to remove, nothing reported.
- `RuleEntry::Verbatim(_)` and the entry's `unknown` map: never folded.
- **Guard** (`protected == true`): the whole task's rules fold is
  skipped and `RuleFoldChanges` is empty. This is exactly D-S6's
  "the mirror copies the task's local rule set and their clocks over
  the fold result": adopt→re-add, update→revert, tombstone→restore all
  collapse to *nothing happened* when the local set is the protected
  truth; the plan implements the collapse (skip) and the tests pin the
  identical end state (one test asserts skip ≡ copy-over for an
  adopt+update+tombstone triple).
- Tombstones are **not** materialized locally: a tombstone for a rule
  this device never had leaves no local trace (the document carries it;
  a later local create wins by clock). No local tombstone map exists —
  `ReminderState` grows nothing.

`RuleFoldChanges` is a small typed struct
(`upserts: Vec<ReminderRule>, deletes: Vec<ReminderRuleId>`) — the
pipeline turns it into `PersistenceAction::UpsertReminderRule` /
`DeleteReminderRule` and interprets the same deltas into
`AppState.reminders`. The pure function mutates the caller's map *and*
returns the deltas so the persistence batch and the memory view cannot
drift (the `apply_push_outcomes` style).

### D3 — the guard arm + in-fold copy mechanics

- `pending_task_intents` (pipeline.rs:112): the `LocalOp::UpdateAppData(t)`
  arm **stops** `continue`-ing (the fence comment there is deleted
  with the arm landing) and sets — coarse by design, D-S6 — *all four*
  app-field flags (`scheduled` included: the guard mirrors both lanes;
  D-V2: "the guard mirror needs no changes") **plus `reminder_rules`**,
  under the inherited boundary convention `queued_at > read_at`
  (`==` is seen-by-the-cycle and unprotected; the existing pin at
  pipeline tests keeps holding).
- `keep_task_intent` (native) stays untouched — it never copies the
  app fields or rules; the app-family copy lives inside
  `fold_task_app_data` (D1) and the rules copy is the fold skip (D2).
  This keeps the binding §5 wording: one guard struct, two application
  sites.
- The `UpdateAppData` op **never** sets `title`/`description`/
  `duedate`/`done`/`position`/`labels`/`deleted` — the native lanes
  are not protected by a sidecar op (and vice versa: `UpdateTask`
  never sets the app flags — already true).
- **Clock copy rationale test** (the D-S6 "why"): a *failed* flush
  followed by a concurrent remote write must not adopt the remote
  value one cycle late — the guard's clock copy makes the next fold's
  `d ≥ l` comparison honest. Tier-1 test: flush fails (no outcomes),
  remote entry lands with a newer clock, the post-`read_at` local edit
  survives both value and clock.

### D4 — signature growth: evidence and features reach the engine

- `apply_sync_report` grows two trailing parameters:
  `app_data: &crate::messages::AppDataEvidence` and
  `features: crate::capabilities::DeckFeatures`.
- `apply_push_report` grows `features: DeckFeatures` (echo adoptions
  of a `Failed` report's landed pushes need the same scheduled gate).
- `SyncReport::Completed` **and** `Failed` grow
  `features: DeckFeatures` — the actor's probed flags ride every
  report (both variants: a failure streak after an actor respawn must
  still refresh the engine's recorded flags; reports are in-process
  mpsc only, so this is not a wire change).
- The engine (`ingest_sync_report`) passes `&app_data`/`features`
  through; the `app_data: _` acknowledgment comment (state actor.rs:702)
  is deleted with the wiring.

### D5 — the native-`scheduled` gates while `CARD_STARTDATE` is false (D-V2's "sharp edge", full extent)

While `features.card_startdate == false`, *nothing native* touches
`scheduled` — the field "belongs exclusively to the sidecar fold":

1. **Pull verdict skip**: `merge_task` grows a
   `native_scheduled: bool` parameter; the `scheduled` verdict block
   (merge.rs:205-209) runs only while true. The always-absent remote
   `startdate` must never clobber local state via "remote wins, value
   null".
2. **Echo-adoption gate** (this plan's extension of the same ruling —
   flagged in §7): `adopt_task_after_push` grows the same parameter;
   while false it takes `scheduled: local.scheduled` (the echo's
   `startdate` is null *because our own capability-gated write sent
   null* — adopting it would clobber local `scheduled` on every
   full-send card push echo, e.g. a title edit, with its clock
   advanced — strictly worse than the pull case D-V2 names). While
   true the echo adoption is unchanged (values-as-is; the server's
   date-granularity normalization must land).
3. **Fold gate**: D1's `native_scheduled` parameter.
4. **Flush gates**: emission skip + union lane-GC (D11).

`pipeline.rs` threads `features.card_startdate` into all four sites;
`merge.rs`'s unit tests grow the flag-arm matrix (the existing
`scheduled` verdict pins become the `native_scheduled = true` arm).

### D6 — how the flag reaches `plan_command`: features recorded on the board row

- `SyncStatus` grows `features: DeckFeatures` with
  `#[serde(default)]` (a `Default for DeckFeatures = CONSERVATIVE`
  impl is added in `capabilities.rs` — the D-V1 fallback is the
  principled default). Memory-only like `counters`: the storage codec
  and `set_stored` never touch it (no migration; a restart re-learns
  from the first report — actors probe at construction, so a restarted
  process re-probes within its first cycle).
- `apply_sync_report` records `features` onto the report's board row
  (and `apply_push_report` onto each affected board's row, using the
  report's `features` — per-board rows carry *their* connector's
  flags; a board's row is only written by reports for that board).
- `plan_command`'s `UpdateTask` handler (command.rs:373) reads the
  task's board row via a small helper
  (`fn board_features(app, board) -> DeckFeatures`, default
  `CONSERVATIVE` when no row exists — unbound boards included) and
  routes:
  - native manifest rows → `UpdateTask` (unchanged);
  - `effort`/`importance`/`urgency` slots → enqueue
    `UpdateAppData(task)` (the interim reversal — exactly the
    app-slot half, MD-10);
  - the `scheduled` slot → `UpdateTask` while the flag is true,
    `UpdateAppData` while false (D-V2's lane rule; the conservative
    default routes it to the document lane on a not-yet-reported
    board — the D-V1 fallback transposed engine-side);
  - **mixed changesets enqueue both ops** (D-F3's contract: "mixed
    slots enqueue both — full-send covers both lanes"); a mixed edit's
    native full-send carries the current `scheduled` value, which
    self-heals the one residual window documented in §7-R2.
- The board resolution for the flag reads the same entity chain the
  plan already uses (`task → stack → board`).

### D7 — the enqueue wiring list (complete)

Every site, with the command it rides (all in domain `command.rs`;
the engine layer needs no change — `EnqueueOp` flows):

| # | Command / site | Enqueues | Notes |
|---|---|---|---|
| 1 | `UpdateTask` — `effort` slot set (command.rs:357) | `UpdateAppData(task)` | alongside the existing lane split; stamping already landed (chunk 1) |
| 2 | `UpdateTask` — `importance` slot set | `UpdateAppData(task)` | ditto |
| 3 | `UpdateTask` — `urgency` slot set | `UpdateAppData(task)` | ditto |
| 4 | `UpdateTask` — `scheduled` slot set | flag-routed (D6) | the manifest's dynamic-lane row |
| 5 | `SetReminderRule` (command.rs:1107) | `UpdateAppData(task)` | the fresh rule is minted with `updated_at = created_at = now` (D9); the cascade-deleted rules become flush-time tombstones (D8); the "rules are local-only" comment is deleted |
| 6 | `ClearReminderRules` (command.rs:1128) | `UpdateAppData(task)` | removed rules → flush-time tombstones (D8); no-op stays a no-op (no enqueue on an empty clear) |
| 7 | `DeleteTask` / `DeleteStack` / `DeleteBoard` cascades | *(nothing new)* | `task_rule_deletes` already cascades rules; pending `UpdateAppData` ops of a delete-pending task are *not* cancelled when bound — they ride the flush (their entry carries the tombstoned task's values; the snapshot-absence GC prunes the entry once the card leaves); the unbound path's `task_ops` cancellation already covers `UpdateAppData` via `op_targets_task` |
| 8 | `CreateTask` | *(nothing)* | the create echo binds first; the first document entry rides the task's first app-field edit or the next flush's rebuild |

The guard mirror (D3) makes all six enqueues self-clobber-safe. No
other command touches the app family (`SetTaskDone`/`MoveTask`/
`AssignLabel` are native-only, unchanged).

### D8 — rule tombstones: derived at flush, stamped with the removal clock

**The problem**: a rule removal must emit `{deleted: true, clock}` into
the document (D-S9: "remove/clear emit one tombstone per removed
rule"), but by flush time the removed rule is gone from local state,
`LocalOp` is id-scoped by design ("carry no field sets" — the ruled op
shape stays), and nothing else persists between command and flush.

**The mechanism** (recorded decision): the flush derives each covered
card's tombstones from the base document, stamped with the removal
instant the outbox already carries:

- New pure function in domain `appdata.rs` (the `union` sibling):
  `derive_rule_tombstones(base: &RulesEntry, local: &BTreeMap<ReminderRuleId, ReminderRule>, clock: DateTime<Utc>) -> BTreeMap<String, RuleEntry>`
  — for every key in `base.rules` that decodes as a live `Rule` and has
  no local rule with that UUID: `Tombstone { clock }`. Existing
  tombstones pass through untouched (no re-stamp), `Verbatim` entries
  are never ordered against.
- **The clock** is `max(queued_at of the UpdateAppData ops being
  flushed for this task)` — the removing command enqueued its op at
  the removal instant, so the derived tombstone's clock *is* "clock:
  now" as D-S9 words it (the latest op's stamp dominates; a strictly
  later same-task edit's op may raise it — LWW-conservative). Tasks in
  a rebuild with no pending op use the cycle's `read_at` (the
  design's own "local entries as of `read_at`" freshness).
- **Soundness**: the base is the document *this cycle read and passed
  the fences* — a truncated foreign view cannot cause a deletion here
  (absence-from-base is not an input; only base-*present* live rules
  missing locally get tombstoned, and the only way a rule is
  base-present but locally absent is a local removal, a task-delete
  cascade, or never-adopted content — and never-adopted content's card
  resolves to no local task, so it is never in flush scope).
- `SetReminderRule`'s replace flow works through the same derivation:
  the old UUIDs are base-present and locally absent ⇒ tombstoned; the
  fresh rule rides as a live entry.

This keeps the ruled observables exactly (one tombstone per removed
rule, removal-instant clock) without widening `LocalOp` and without
new persisted state. Flagged in §7-I1 as an interpretation (the
ruling's *mechanism* was unstated; the *semantics* are preserved).

### D9 — `ReminderRule.updated_at` + stamping + storage wiring

- Domain `reminders.rs`: `ReminderRule` grows
  `updated_at: DateTime<Utc>` (the reserved additive write clock,
  D-S9). The reservation comment (:117-119) is **amended, not
  deleted**: "consumed by the sidecar document lane (chunk 4); every
  rule mutation stamps it". Creation stamps `now` (both `created_at`
  and `updated_at`); there are no rule-edit verbs until chunk 6
  (removals *delete*, they do not stamp a survivor). The two v1 fence
  comments live in `pipeline.rs` (:782, :975) — both are amended to
  the ruled wording: "no reminder data enters the *native*
  sync/push path; rules ride the app-data document lane" — with the
  `reminders` carry-through in `apply_sync_report` replaced by the
  rules fold's result.
- Storage `repo.rs`: `ReminderRuleRow` grows `updated_at`; decode maps
  it (the column is `NOT NULL DEFAULT epoch` — pre-chunk-4 rows decode
  with the epoch, which loses every clock race exactly like
  `BoardClocks::epoch`'s precedent); the rule upsert writes it. The
  codec's serde for rules is column-wise (no JSON blob), so no serde
  default dance is needed. Round-trip test extended; the existing
  0007 migration is untouched (the column predates this chunk).

### D10 — the document group in `plan_pushes`

- `PushTarget` grows `Document(BoardId)`; `MaterializedOp` grows
  `FlushAppData { board: BoardId }` (carrying no content: the flush
  materializes from the actor's state + cache, like every shape
  carries "current local state at plan time" and the executor maps
  it).
- **Coalescing**: all of a board's pending `UpdateAppData` ops bucket
  into **one** group (bucket key = the op's owning board through
  `EntityTables` — the same graph `owning_board` uses);
  `primary` = the first in queue order, `subsumed` = the rest in
  queue order. One PUT per flush (D-S5).
- **Positioning**: the document group is emitted **last** — after all
  stack, label, and task groups (superset of the ruled "after
  `CreateTask` groups": entries key on remote card ids, which exist
  only post-echo; last is the only position that guarantees every
  create of the cycle echoed first). The partition property holds:
  every op id appears exactly once.
- **Executor defer**: the push-phase loop in the sync actor stashes
  `FlushAppData` groups instead of executing them (the flush executes
  post-pull, D-S4 item 4 — the *position* ruling). The executor's
  `execute` match grows the arm as a defensive defer
  (`GroupOutcome::Outcomes(vec![])` — "empty means deferred to the
  next cycle", the existing documented meaning) so the type stays
  total; the actor never relies on it.
- **Unresolved tasks at flush**: ops whose task has no remote binding
  (create failed / still unbound) are **omitted** from the upload and
  from the group's outcome list — no outcome ⇒ the engine never
  completes them ⇒ they stay queued and ride the next cycle (D-S4).
  The primary may itself be unresolved; outcomes are emitted per
  *resolved* member op (the engine's outcome matching is id-keyed and
  order-free).

### D11 — the flush: placement, union, rebuild, metadata refresh

New sync-crate module `flush.rs` (pure assembly helpers + one
orchestration fn the actor calls), invoked in `cycle` **between
`sidecar_evidence` and the report send** (actor.rs:617-619 — after
the cycle's card pushes, after the pull, before the report; the only
position satisfying D-S4's three clauses; the stamp bump lands in the
*next* cycle's pull because the snapshot was assembled pre-upload).

Inputs: the deferred document group, `&persisted` (tasks, rules,
bindings), the cycle's evidence (`Document(doc) | Absent |
Unreadable(reason)`), the snapshot's card-id set, `features`, `read_at`,
`&mut LoopState` (cache), `&DeckClient`.

Gate (has-work): the flush runs iff (a) the board planned a document
group, or (b) a **rebuild trigger** fires and local app-data exists
(D12). `sidecar_evidence`'s `anchor_action(&discovery, false)` call
becomes `anchor_action(&discovery, has_flush_work)` — the ceremony's
Create/Adopt arms wake exactly when a flush has work (lazy, D-S1;
zero sidecar cost otherwise, D-S8).

Content assembly (pure, in domain `appdata.rs` unless noted):

1. **Base** = the cycle's `Document` (freshly downloaded or
   metadata-equal cached — both are "actually read this cycle" per
   D-S7's invariant; the cache only short-circuits the GET).
2. **Overlay** = a document carrying, per *resolved* task in scope
   (group tasks normally; **all** live board tasks with app-data in
   rebuild mode):
   - `TaskEntry`: `effort`/`importance`/`urgency` always,
     `scheduled` only while `!card_startdate` (emission gating,
     D-V2) — each as `{v, clock}` from the task's value +
     `clocks` slot (full-send entries, outbox.rs's own rule);
   - `RulesEntry`: the task's local rules as `Rule { offset,
     created, clock: updated_at }`, plus
     `derive_rule_tombstones(base_entry, local, clock)` (D8).
   Tasks unresolved at upload time are omitted (D10); a task with an
   empty entry shape (no values, no rules) contributes an empty entry
   rather than being dropped — the entry's *presence* is harmless and
   the GC prunes it when the card goes.
3. **Union**: `union(base, overlay)` — per-key embedded-clock LWW,
   ties to the overlay (our just-stamped local writes), unknown
   content preserved verbatim, never a blind overwrite.
4. **Lane-GC** (D-V2, while `card_startdate`): remove every modeled
   `scheduled` key from the unioned document's tasks entries (a pure
   domain fn taking the flag as a `bool` — the decision is the sync
   layer's, the transform stays version-blind data).
5. **Snapshot-absence GC**: `prune_absent_cards(merged,
   snapshot_card_ids)` (both sections, tombstones included).

Upload ladder (per `anchor_action`):

- `Use(candidate)` + live attachment: `update_attachment_content`
  with the assembled JSON.
- `Use(candidate)` + `tombstoned`: **restore-then-replace** —
  `restore_attachment` then `update_attachment_content` (never
  create-against-a-tombstone; renames nowhere).
- `Adopt(ref)`: upload onto the single unconfirmed match
  (`upload_attachment` onto that card) — rebuild content.
- `Create`: `creation_target` (already unit-pinned, dead_code lifts) →
  `create_card("[taskboard] app-data")` → `upload_attachment` (the
  additive `data` field rides unconditionally, D-V3) — rebuild
  content. Actor-side, lazy, adopted by the next pull (R6).
- Rebuild content = the union with **all** local entries as the
  overlay (step 2's rebuild arm) over the same base when one exists —
  a rebuild is about *scope*, never about blind overwrite.

Outcome + cache mechanics:

- **Success**: one `PushOutcome { op, result: Applied { echo: None } }`
  per *resolved* member op (the `primary_outcome` shape, reused) —
  appended to the cycle's `pushes` so the engine completes them via
  the ordinary path. `LoopState.documents[board]` is refreshed from
  the upload response's attachment metadata (`(lastModified,
  filesize)` + the uploaded document) so the next cycle's comparison
  does not re-GET our own write (D-S4 item 4). The anchor card's
  *card* stamp also advanced — irrelevant to the document cache; the
  next pull's detail refresh carries it (already-paid read).
- **Transport failure** (any upload step): the flush emits **no
  outcomes**, logs typed; the cycle still reports `Completed` with its
  valid snapshot + evidence (post-pull, the abort rationale does not
  apply — D-S7). Ops stay queued; retry next cycle. **No outcome ⇒
  no `LoopState.documents` refresh from the failed path** (only the
  read-side cache entry remains valid).
- **Permanent 4xx** (the classify table's `BadRequest` class, or an
  anchor-creation 4xx that will not self-heal): emit
  `Rejected { BadRequest }` for the group's primary (the
  `reject_kind` shape) — the engine dead-letters it as a counted
  `FailOp` (pipeline.rs:1255's precedent), exactly D-S5's completion
  clause. Subsumed ops stay queued and re-form the group next cycle —
  the native bad-request path's own convergence cadence, mirrored
  deliberately (uniform machinery beats a special case; the loop
  converges one primary per cycle). Transport-class failures of the
  same steps strand (retryable) — D-S7's anchor-uncreatable row.
- The flush never aborts the cycle: every failure mode above leaves
  the report `Completed` (or defers to the next cycle's retry).

### D12 — rebuild-mode triggers and scope (D-S1 loss taxonomy + D-S4 item 4)

Rebuild (local side = **all** live board tasks with app-data) fires
when the flush has work and any of:

| Trigger | Detection (this cycle) | Content |
|---|---|---|
| No anchor card exists | discovery: no confirmed carrier, no unconfirmed match; `Absent` evidence | `Create` ladder; base = empty document |
| No attachment on the carrier | confirmed card, tombstoned/absent attachment | restore-then-replace |
| Single unconfirmed title match | `Adopt` | upload onto it |
| Document absent while local app-data exists | `Absent` evidence + `board_has_local_app_data(...)` (any live board task with effort/importance/urgency set, or rules on its tasks, or — while the flag is false — `scheduled` set) | `Create`/restore ladder with the full local overlay |
| Foreign-board document | `Unreadable(ForeignBoard)` | D-S2's moved-card fence: the flush **may** replace the content — base = empty, rebuild content, update-in-place |

Explicitly **not** rebuild triggers (fold *and* flush nothing, ops
wait): `Unreadable(Transport)`, `Unreadable(Decode)`,
`Unreadable(FormatTooNew)` — D-S7's table is the authority (a document
we could not parse must not be re-encoded; the recovery runbook for a
persistently corrupt document is the documented manual delete → the
tombstone path fires restore + rebuild next cycle).

`board_has_local_app_data` is a pure helper (sync crate, over
`EntityTables`-shaped borrows + the rules map + the flag) — the lazy
guarantee's gate: boards whose users touch only native fields (flag
true) never grow an anchor.

### D13 — the D-S7 fence matrix as tests (typed observables)

Tier-1 homes: the sync crate's `actor_wiremock.rs` (HTTP failure
injection, request counts) + the state crate's engine harness (outcome
effects). Each row = one test; "no upload" is observed as **zero
content-PUT/attachment requests recorded by the mock**, "ops queue" as
**no outcome for the group's ops in the ingested report / outbox
unchanged**, "counted" as **`FailOp` actions in the persistence batch**
(never text, never logs):

| # | D-S7 row | Scenario (wiremock) | Typed observable |
|---|---|---|---|
| F1 | download fails | content GET → 500 (after retries) | evidence `Unreadable(Transport)`; 0 upload requests; cycle `Completed`; flush ops still queued |
| F2 | decode fails | body = truncated/corrupt JSON | `Unreadable(Decode)`; 0 uploads; `Completed`; ops queued |
| F3 | format too new | `{"format": 99, ...}` | `Unreadable(FormatTooNew { format: 99 })`; 0 uploads; ops queued; **engine**: board row's `sidecar` = `Unreadable(FormatTooNew)`; **app**: `BoardStatusView` carries the typed warning |
| F4 | foreign board | document `board` ≠ cycling board | `Unreadable(ForeignBoard)`; fold nothing; flush (with work) replaces content (rebuild) |
| F5 | upload fails (transport) | content PUT → connection drop / 5xx | 0 outcomes for the group (ops queued); cycle `Completed` with snapshot + evidence |
| F6 | upload rejected 400 | content PUT → 400 | `Rejected { BadRequest }` outcome → engine batch carries `FailOp(primary)`; `SyncPhase::Failed { BadRequest }` on the board row |
| F7 | anchor creation forbidden | `create_card` → 403 | ops strand (no outcomes, queued); cycle `Completed`; the ceremony re-attempts next flush-with-work |
| F8 | anchor creation bad-request | `create_card` → 400 | counted `FailOp` (D-S5's uncreatable-anchor example) |
| F9 | `SyncReport` send fails after upload | *not HTTP-testable; covered structurally* | the upload already happened ⇒ `LoopState` cache refreshed ⇒ next cycle's metadata-equal path re-folds the same document (idempotent); the engine never saw outcomes ⇒ ops queued (existing engine semantics). Recorded here as the row's coverage note, no new test. |
| F10 | anchor card deleted mid-cycle | carrier vanishes between cycles | next scan: no carrier ⇒ rebuild ladder (covered by the D12 trigger tests) |

Engine-side fence tests (state crate, no HTTP): a `Failed`-report
fold ban (`apply_push_report` folds nothing — existing structure, one
pin that `app_data`-less reports never touch reminders/app fields);
the failed-flush-then-remote-write guard-clock test (D3).

### D14 — per-board sidecar health: derivation and home

- Domain `state.rs`: `SyncStatus` grows
  `sidecar: SidecarHealth`, `#[serde(default)]`, memory-only (the
  `counters` precedent; codec untouched).
  `pub enum SidecarHealth { Healthy, Unreadable(AppDataUnreadable) }`
  (`Default = Healthy`; `AppDataUnreadable` is `Copy + Eq` already).
  `Absent` and `Document` evidence both derive `Healthy` — absent is
  the ruled *silence*; only the four `Unreadable` reasons warn.
- Derivation lives in `apply_sync_report` (it has the evidence and
  builds the board's row): `sidecar = derive_sidecar_health(app_data)`
  — a total match. `apply_push_report` leaves the field carried over
  (a failed cycle produces no new evidence; the last verdict stands
  until the next completed cycle re-derives — a restart re-derives
  from the first cycle's fresh evidence, so memory-only is honest).
- The status row write does not interact with
  `append_status_if_changed` (it compares only `phase`/`last_success`;
  the health rides the published state, not the persisted batch).

### D15 — CLI `status`: the typed warning through the projection

- `SyncStatusView` (app `commands/output.rs`) grows
  `sidecar: Option<SidecarHealthView>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` (the
  `connectors` additivity precedent — no protocol bump);
  `SidecarHealthView` is a flat typed mirror
  (`{ reason: "format_too_new", format: u32 } | { reason: "transport"
  | "decode" | "foreign_board" }` — kebab/snake envelope per ADR 0009;
  `Healthy` serializes as absent).
- `render_text`/`board_line` append one clause when the typed warning
  is present — the loud "sidecar document unreadable (reason …);
  update this app to the version your other devices use" rendering is
  presentation text over the typed fact (D-S2's ruled wording);
  board-level only (the connector health fold ignores sidecar —
  `aggregate_rows` is untouched).
- **Tests assert the typed view**, never the rendered string (AGENTS
  §5): a state where a board's row carries
  `Unreadable(FormatTooNew { format: 99 })` projects to
  `BoardStatusView { sidecar: Some(FormatTooNew { 99 }), .. }` in all
  three projections (`server`, `in_session`, and the IPC-relayed
  answer's serde round-trip).

### D16 — the anchor-title CLI guard (app layer, typed refusal)

- Site: app `commands/tasks.rs` — after `resolve_task`, before the
  engine call, in **both** lanes:
  - `edit_on` (:410): refuse when `changes.title` is `Some(new)` and
    the resolved task's title == `taskboard_domain::ANCHOR_CARD_TITLE`
    (the domain const — the app already depends on the domain crate)
    **and** `new != ANCHOR_CARD_TITLE` (re-asserting the identical
    title is a no-op edit, let the engine's no-op path answer it).
  - `delete_on` (:454): refuse for the resolved anchor-titled task
    (exact-uuid-any resolution included — deleting the anchor by uuid
    is exactly the accident the guard exists for).
- Typed refusal: `CommandError::AnchorTaskReserved` (new variant,
  app-layer error enum; the engine never learns of the rule — the
  ruling: app layer, never engine). The `CommandError` docs name the
  constant so `--help`-adjacent surfaces stay accurate without
  asserting text.
- Non-goals inside the guard: `boards delete`/`ResetBoard`/stack
  cascades that would sweep the anchor away are *not* refused (the
  ceremony never fights user actions; the guard covers only verbs
  *targeting* the anchor task — the ruling's wording). `tasks add`
  with the reserved title is likewise out (the ceremony tolerates
  ambiguous matches by creating its own carrier); recorded as a
  backlog candidate in this plan's §7, promoted by the user if
  wanted.
- The REPL inherits the guard mechanically (it dispatches through the
  `*_on` seams) — no separate implementation, one test per lane.

### D17 — manifest assertion 4: per-row op routing

Home: `taskboard-state` tests (D-F3's placement). Mechanics:

- For every `TASK_FIELDS` row: construct a minimal `AppState` (one
  board/stack/task; the board's row carrying a chosen `features`) and
  a single-slot `TaskChanges` for that row's field (composites:
  `position` has no `EditTask` slot — the assertion derives the
  routable set from the manifest *intersected* with `TaskChanges`'s
  slots, documented in the test; `title`/`description`/`duedate`/
  `done`-via-`SetTaskDone`/`archived`/`deleted`-via-`DeleteTask` are
  the native routables already pinned by chunk 1's engine tests —
  assertion 4 asserts the four app rows + the scheduled row through
  `EditTask`, and cross-checks the native slots' `UpdateTask` routing
  in the same table so the manifest drives both sides).
- `effort`/`importance`/`urgency` ⇒ the plan carries exactly one
  `EnqueueOp(UpdateAppData(task))` and no `UpdateTask`; the native
  rows ⇒ exactly `UpdateTask` and no `UpdateAppData`.
- The `scheduled` row runs three times: `card_startdate: true` ⇒
  `UpdateTask`; `false` ⇒ `UpdateAppData`; the unbound-board default
  (no row ⇒ `CONSERVATIVE`) ⇒ `UpdateAppData`.
- A mixed changeset (one native + one app slot) ⇒ **both** ops (the
  D-F3 contract sentence, pinned).
- The test drives `plan_command` through the engine's execute path
  where the harness allows, else calls `plan_command` directly — the
  manifest is the single enumeration source either way (a fifteenth
  field without routing fails the table; a routed slot without a
  manifest row cannot be expressed).

### D18 — tier-2 convergence suite (the existing corpus's conventions)

Home: `crates/taskboard-sync-nextcloud/tests/it_docker.rs`, new
`d_common`-style scenario fns gated by `live_config()` (honest skip
recorded per MD-5), one fresh board+stack per run (`run_id`),
`with_deadline`/`retrying_server_errors` wrappers, `eventually`
predicates — no new mechanics beyond chunk 3's pinned variant.

**Design: two `boot_pair`s (two full engine+actor devices) against
one shared docker board** — the strongest form of "second client
folds it", exercising enqueue → planner → flush → document →
discovery/download → fold on real wire and real engines:

- **C1 — app-field convergence**: pair A creates a task (echo binds),
  edits `--effort` (the CLI flag is chunk 5's — drive
  `StateCommand::UpdateTask` directly through `pair.engine.execute`,
  as the existing D-tests do) ⇒ `UpdateAppData` enqueued ⇒ A's cycle
  flushes (anchor created — assert via the server: a card titled
  `[taskboard] app-data` with the reserved-filename attachment
  exists) ⇒ `eventually(pair_b, |app| task.effort == Some(900))`
  (B discovered, downloaded, folded). Then A clears the field ⇒
  `eventually(pair_b, … effort == None)` (the `{v: null}` write
  converges — the cleared-field-is-a-write rule, end to end).
- **C2 — rule add → tombstone → converge**: A sets a reminder rule ⇒
  B eventually holds a rule with the same UUID on its task (document
  identity); A clears ⇒ B's rule is gone after B's next cycle
  (tombstone wins by clock). While the flag is false
  (deck1175 variant, below), also assert B's `scheduled` converges
  through the document.
- **C3 — guard mirror end-to-end (best-effort race, tier-1 is the
  authority)**: A edits a field and immediately `sync_now()`s; assert
  A's own value survives its next adoption (the echo case — the
  deterministic value+clock variant is D3's tier-1 test; C3 asserts
  the idempotent echo: after A's flush, A's next cycle re-folds its
  own document and the value/clocks are unchanged).
- **C4 — deck1175 variant** (runs under
  `live_docker_deck1175_config()` where available): the same C1 with
  `scheduled` — proving the fallback lane on the live tier (the
  native verdict skip + document fold + flush emission all while
  `card_startdate == false`). Honest-skip where no docker/1.17.5.

## 4. Implementation inventory (file-by-file, commit ladder)

Standalone commits in order (each builds + passes its own tests;
rebase-merge hygiene). Estimated diff ≈ **1.9–2.5 k lines** (≈60%
tests).

1. **`feat(domain): the app-data fold primitives`**
   `merge.rs` (+~320): `fold_task_app_data` (D1), `fold_reminder_rules`
   (D2), `RuleFoldChanges`; unit + proptest coverage (clock LWW matrix,
   guard copy value+clock, absence ≠ deletion, tombstone-vs-clock,
   adopt-verbatim-uuid, protected-skip ≡ copy-over, card-stamp
   non-readability by construction).
   `appdata.rs` (+~180): entry builders
   (`task_entry_for(task, native_scheduled)`,
   `rules_entry_for(rules)`), `derive_rule_tombstones` (D8),
   `lane_gc_scheduled(document, drop: bool)`; tests + the proptest
   additions (tombstone derivation never invents keys; lane-GC touches
   only modeled scheduled keys).
   `reminders.rs` (+~10): `updated_at` field + amended reservation
   comment. `capabilities.rs` (+~8): `Default = CONSERVATIVE`.
2. **`feat(storage): the rule write clock rides the row`**
   `repo.rs` (+~25): row struct/decode/upsert for `updated_at`;
   round-trip test extension (epoch-default decode included). No
   migration.
3. **`feat(domain): fold the sidecar into the sync report`**
   `messages.rs` (+~10): `features: DeckFeatures` on both `SyncReport`
   variants. `merge.rs` (+~20): `merge_task`/`adopt_task_after_push`
   `native_scheduled` gates (D5). `pipeline.rs` (+~220):
   `apply_sync_report(app_data, features)` — the fold pass after the
   task-merge loop (tasks via `task_by_board_card`, then rules into a
   cloned `reminders` + delta actions), health + features onto the
   board's `SyncStatus` row, both fence comments amended;
   `apply_push_report(features)` + the echo gate; the
   `pending_task_intents` `UpdateAppData` arm (D3) with its fence
   comment replaced by the landed semantics.
   `state.rs` (+~20): `SyncStatus.sidecar` + `SidecarHealth`.
   State crate (+~140): ingestion wiring (the `app_data: _`
   acknowledgment becomes the pass-through), fold integration tests,
   the guard-clock test (D3), health-derivation tests. **The one
   deliberate insta pass**: the `ReminderRule` + `SyncStatus` +
   `SyncReport` widenings (review rule: the diff is exactly these
   shape additions, nothing else; no `.pending` files ship).
4. **`feat(domain): enqueue the app-data lane`**
   `command.rs` (+~70): the lane-split reversal (D6/D7: app slots +
   flag-routed scheduled + the two rule verbs), `board_features`
   helper; the interim's own comment deleted with the code it
   described. Command tests (+~120): per-slot enqueue receipts,
   mixed-sets-both, rule verbs enqueue + stamp, empty-clear no-op.
5. **`test(state): manifest assertion 4`** (+~110): D17's routing
   table. (Own commit: it is the #244 closing artifact and reviewable
   alone.)
6. **`feat(domain): the document push group`**
   `push.rs` (+~80): `PushTarget::Document`, `MaterializedOp::FlushAppData`,
   the board-bucketed coalescing, last-position emission;
   `push_exec/execute.rs` (+~10): the defensive defer arm; planner
   tests (+~90): one group per board, partition property, position
   after creates, unresolved-no-outcome contract (the flush-side half
   of that test rides commit 7).
7. **`feat(sync): the flush between pull and report`**
   New `flush.rs` (+~330): has-work gate, overlay assembly, union +
   lane-GC + prune, the upload ladder (Use/Adopt/Create/restore),
   outcome emission (`primary_outcome`/`reject_kind` reuse), `LoopState`
   metadata refresh from the upload response, typed failure mapping.
   `actor.rs` (+~60): defer-document-groups in the push loop, the
   flush call site, `anchor_action(&discovery, has_flush_work)`,
   `creation_target`'s `dead_code` lifted. `features` onto both report
   variants' construction. Wiremock green-path tests (+~260): C1's
   single-pair skeleton (edit → flush → server-side document content
   pinned structurally), create/adopt/restore ladders, metadata
   no-re-GET pin (request-count assertion).
8. **`test(sync): the D-S7 fence matrix`** (+~280): D13's F1–F8 (+F4's
   replace arm); the engine-side dead-letter half of F6.
9. **`feat(app): the sidecar health surfaces in status`** (+~120):
   `output.rs` typed view + serde additivity; `status.rs` render
   clause + projection wiring; structural view tests (D15).
10. **`feat(app): the anchor-title guard`** (+~70): `CommandError::
    AnchorTaskReserved`, the `edit_on`/`delete_on` refusals (D16);
    typed-refusal tests, both lanes.
11. **`test(sync): the tier-2 convergence suite`** (+~220): D18's
    C1–C4 (C4 honest-skip where no 1.17.5 runner). Recorded outcomes
    travel the stage report (MD-5 tier honesty).
12. *(stage 2 bookkeeping)* **`docs(changelog): the sidecar fold,
    flush, and rules section`** — the Unreleased lines.

## 5. Test matrix summary (layer → subject)

| Layer | Subjects |
|---|---|
| domain unit + proptest | fold LWW matrix; guard value+clock; absence≠deletion; rules fold (adopt/update/tombstone/verbatim/protected); tombstone derivation; lane-GC; entry builders; `updated_at` stamping; echo/pull `scheduled` gates both flag arms |
| storage | rule clock round-trip incl. epoch default |
| state (engine) | fold integration (position after task loop, before R3); rules deltas → batch; guard arm + boundary convention; guard-clock after failed flush; health derivation per evidence variant; features recording; assertion 4 table; `Failed`-report fold ban |
| sync wiremock | flush green paths (Use/Adopt/Create/restore); metadata no-re-GET; fences F1–F8; outcome shapes (resolved-only completion; 400 → primary Rejected) |
| app | status typed warning through all projections; serde additivity; guard refusals (edit-title, delete; non-anchor edits pass; no-op re-assert passes) |
| tier-2 docker | C1 field convergence; C2 rule add/tombstone convergence; C3 echo idempotence; C4 deck1175 `scheduled` fallback lane (honest-skip recorded) |
| insta | ONE deliberate pass: `ReminderRule.updated_at`, `SyncStatus.{sidecar, features}`, `SyncReport::*.features` widenings |

## 6. Non-goals (dispatch plan §5 restated, with this plan's own)

- No N-rule verbs (`AddReminderRule`/`RemoveReminderRule`) — chunk 6;
  this chunk's sites (D7) are exactly what chunk 6 re-points.
- No UI badge — chunk 5 (it reads `SyncStatus.sidecar`).
- No adhoc/alarms, no migration (0008 is chunk 7's), no cross-board
  repair (#350), no `proofs`/`settings` sections (reserved names
  only), no anchor-task hiding on lists (chunk 5), no `tasks add`
  title guard (§7 backlog).
- No `LocalOp` payload widening, no new `ValidatorKey` kind, no
  persisted document state beyond what chunk 3 landed.
- No persisting `SyncStatus.features`/`sidecar` (memory-only; §7-R1).

## 7. Risks, residual windows, and interpretation flags

- **R1 — restart loses the recorded health/flags until the first
  report** (~one cycle after boot): accepted — actors probe at
  construction and report every cycle; persisting them would need a
  migration this chunk doesn't own. The loud warning re-derives from
  fresh evidence, so no user-visible persistence gap exists beyond
  that first cycle.
- **R2 — the provisional-lane window** (D6): on a fresh bind (or
  after a server upgrade, until the next report) the engine's
  recorded flag defaults conservative while the actor may know
  native. A *scheduled-only* edit in that window rides `UpdateAppData`,
  which the actor's lane-GC then drops while its flag is true ⇒ the
  edit propagates late (a later mixed edit's full-send carries it; a
  never-repeated edit converges only via a manual re-edit). One cycle
  wide, self-healing in the common case; the conservative default is
  D-V1's own fallback principle. The dual-enqueue alternative was
  considered and rejected as a deviation from the ruled routing
  (assertion 4's "either/or"); revisit only if the user rules.
- **I1 (interpretation, not deviation) — tombstone derivation at
  flush** (D8): D-S9 says "removing a rule writes `{deleted: true,
  clock: now}`" without fixing the mechanism; the derived form
  preserves the ruled semantics (one tombstone per removed rule,
  removal-instant clock, absence never deletes) without payload-wide
  ops or new persisted state. The ruling's *observables* are exactly
  met.
- **I2 (interpretation, not deviation) — the echo-path `scheduled`
  gate** (D5.2): D-V2 names `merge_task`'s verdict; its invariant
  ("the field belongs exclusively to the sidecar fold while the flag
  is false") cannot hold without also gating `adopt_task_after_push`,
  whose echo carries the null our own gated write produced. Not
  gating it would violate the ruling on every card-push echo; this is
  the ruling applied, not extended.
- **R3 — the 400-dead-letter cadence** (D11): a permanently rejected
  document group dead-letters one primary per cycle until the group
  empties (the native bad-request path's own convergence shape,
  mirrored deliberately). `pending_ops` decays visibly; no data loss
  (local truth). If the user prefers whole-group dead-lettering, that
  is a one-line policy change in the flush's outcome emission — the
  plan flags the knob, the default mirrors the precedent.
- **R4 — tier-2 guard-mirror coverage** (D18-C3): the true
  post-`read_at` race is deterministically covered at tier 1; the
  tier-2 form asserts the (deterministic) echo-idempotence half. The
  acceptance sentence is satisfied by the tier-1 pair + C3; noted so
  stage 3 reads it as designed, not as a gap.

## 8. Acceptance checklist (stage 2/3 verify against this)

- App-field edit → `UpdateAppData` enqueued → flush → second client
  folds it (C1); rule add → tombstone → second client converges (C2).
- The guard mirror holds a post-`read_at` edit through the same
  cycle's adoption — value **and** clock (tier-1 D3 test + C3).
- Fences per the D-S7 table: F1–F8 green with typed observables;
  F3's warning end-to-end through CLI `status`'s typed view (D15).
- The CLI guard refuses anchor-titled rename/delete, typedly, in both
  lanes (D16).
- Assertion 4 green (D17); all four manifest assertions green.
- The interim is closed exactly here: `command.rs` enqueues the app
  slots and nothing else touched the outbox paths out of order
  (MD-10 satisfied, not early).
- Gates green (AGENTS §10, the CI-identical coverage sequence); the
  insta pass is the three widenings and nothing else.
- CHANGELOG Unreleased line (stage 2); PR body carries
  `Closes #207` + `Closes #244`; stage 3 annotates #202 (MD-4).

## 9. References

- Normative: the docs in **Origin**, plus the binding roundtrip
  clauses cited through sidecar §5 (`2026-10-14-deck-roundtrip-strategy-report.md`).
- Chunk 3's revised plan (the transport half this builds on):
  `.artifacts/plans/2026-10-22-daily-driver-appdata-transport-plan.md`.
- Measured 1.17.5 evidence:
  `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`.
- ADR 0007 (evidence-then-verdict), ADR 0009 (receipts/envelopes),
  ADR 0010 (standalone-first).
