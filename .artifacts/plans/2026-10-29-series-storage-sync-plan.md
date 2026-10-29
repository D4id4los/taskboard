# [Plan] Series Storage & Sync — Fold Dedup (DR-6/3)

- **Date**: 2026-10-29
- **Kind**: stage-1 implementation plan for chunk DR-6/3 of the
  design-rounds batch dispatch — recurring-series storage facts on the
  appdata wire and the fold-time spawn-race dedup. TDD-first; this file
  is written before any code and is the chunk's contract.
- **Origin** (all normative):
  - `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
    §3.6 (roaming + fold dedup — the design), §1 facts 8–11 (appdata
    substrate, DR-4 D-5 precedent, storage conventions, sync lanes),
    §3.9 properties 7–8, §5.4's resource table **as amended by the
    dispatch** (§2: migration 0014, not the plan's 0012), §6 rulings
    D-C1–D-C13 (dispatch §3.5 binds them to every DR-6 chunk), §7
    chunk 3.
  - `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
    §1 (wave 8; merge/pipeline single-flight lane rule 1), §2 (resource
    ladder + renumber rule), §11.1 (chunk scope + acceptance + the
    three stage prompts).
  - Precedents: DR-4/R's crash-window collapse (the in-repo D-5
    realization), DR-5/2's sidecar fold + truncated-view guard
    vocabulary, DR-6/1's A1 recur types and field-family plumbing.
- **Branch / worktree**: `feat/series-storage-sync` in
  `../taskboard-WT-feat-series-storage-sync`, branched off the fetched
  `origin/main` @ `fca7961c` (local `main` was 34 commits stale — the
  sanctioned re-branch rule, AGENTS §8). Preconditions verified:
  DR-6/2 merged (issue #499 closed by PR #534,
  2026-10-28T16:38Z) and DR-5/2 merged (issue #487 closed by PR #548,
  2026-10-29T20:34Z) — both in `origin/main`'s log. The dispatch
  prompt's PR numbers resolve to *issues*; the verifying lookups are
  the issue states plus the lane commits (`d48c6406` migration 0015
  time entries; `534`-series spawn/roll).
  `scripts/worktree-doctor.sh` exit 0 (228G free, floor 25G).
- **Lane check**: the merge/pipeline lane is single-flight with this
  chunk — DR-4/R (#544), DR-5/2 (#548), DR-6/2 (#534) have all merged;
  nothing else is in flight on `merge.rs`/`pipeline.rs`/`appdata.rs`/
  the storage codec. This chunk is **not** a shape-widener: no
  `AppState` field, no new section, no new `LocalOp` variant, no insta
  dump regeneration. Wave-mate DR-5/3 (CLI lane) and DR-3/4 (ui lane)
  are disjoint.
- **Status**: READY FOR STAGE-2. Every open question is resolved in
  §7 (decision record) — including the two reality-vs-dispatch
  deviations the stage-1 prompt asks to be verified (DEC-1's migration
  finding; DEC-2's appdata-facts finding). No implementation has
  happened.

---

## 1. Mission & binding constraints

Two devices can complete the same recurring head offline; each spawns
a successor with the same `(series, scheduled-local-date)` identity.
When the two occurrence cards meet through sync, one open head per
series must win. This chunk (a) finishes the `recur` wire path — the
rule already decodes from the document but is never *emitted* into it
nor *folded* out of it — and (b) adds the fold-time dedup pass to
`apply_sync_report`: DR-4 D-5's collapse shape re-keyed from title
heuristics to the typed `(series, scheduled-local-date)` identity
(D-C5/D-C7).

Non-negotiable (design §3.6 + dispatch §11.1 + AGENTS):

- **`recur` rides the EXISTING per-task appdata rows** (§3.6: "no new
  section for series facts"). A new section is a design deviation
  needing a recorded reason — there isn't one.
- **The dedup is pure domain called from a pipeline pass.** Detection
  and survivorship are pure functions in `merge.rs`; the engine
  executes the merge through the persistence batch; no sync-actor
  logic (dispatch §11.1 prompt 3.2).
- **Structural trigger only**: two *live, open* tasks sharing the key
  — anything looser (title heuristics, done tasks) is the
  false-positive class D-5 avoided (§11.1 review focus).
- **Survivorship is older-created regardless of arrival order** — a
  permutation property (acceptance verbatim).
- **The neutralized twin is a ref-less tombstone AND its pending
  duplicate adoptions are suppressed** — an un-suppressed adoption
  recreates the twin next cycle (§11.1 review focus).
- **Honesty surface**: `tracing` typed facts under rule tag
  `R-recur-dup` only. No `SystemEvent`, no UI churn (§3.6; pinned
  structurally — the workspace has no tracing-capture test utility,
  and text assertions are forbidden anyway, AGENTS §5).
- **Invalid stored/wire recur folds as typed failures, never
  poisoning** (prompt 3.2) — DR-6/1's decode leniency already pins the
  demote-to-verbatim posture; this chunk must not regress it.
- **Test discipline** (AGENTS §5): typed outcomes, no text assertions,
  proptests for properties 7–8, deterministic journeys (no sleeps),
  floors not ceilings, one behavior one test at the lowest layer.
- **Existing stock untouched**: DR-6/1's proptests 1–5, DR-6/2's
  snapshot corpus, DR-4/R's collapse tests, DR-5/2's fold tests must
  pass unchanged (modulo the mechanical `day` parameter, §3-R5).

## 2. Verified baseline — numbered facts

All against `origin/main` @ `fca7961c` (worktree HEAD), paths relative
to `crates/` unless noted.

1. **Migration 0010 already landed the recur columns.**
   `taskboard-storage-sqlite/migrations/0010_recur_field_family.sql`:
   `ALTER TABLE tasks ADD COLUMN recur TEXT NULL` ("serde JSON of
   `Option<RecurRule>`") + `ck_recur TEXT NOT NULL DEFAULT
   '0001-01-01T00:00:00+00:00'` (epoch-default, 0007's mechanism).
   Its header: "The codec arm lands in the same change" — DR-6/1 took
   the storage half of this chunk's dispatch description. Migrations
   on main run 0001–0015 (`0015_time_tracking.sql` is DR-5/2's); the
   next free number is **0016**.
2. **The storage codec is complete and tested.** `codec.rs:98-110`
   `recur_to_text`/`recur_from_text` (None-JSON-aware); round-trip +
   garbage/zero-interval rejection tests at `codec.rs:833-861`;
   `repo.rs` loads `recur` + `ck_recur` (`:377`, `:418-419`) and
   upserts them (`:1169-1238`). No repo-level (SQL-boundary) recur
   round-trip test exists — codec.rs tests cover the text form only.
3. **The appdata decode side is done.** `appdata.rs`: "recur" is in
   `MODELED_TASK_FIELD_KEYS` (`:43-44`); `ModeledValue::Recur(Option<
   RecurRule>)` decodes key-directed (`:282-287`); a null value
   decodes as the typed clear `Recur(None)` (`:1430-1455`); an
   undecodable value demotes to verbatim — one malformed key never
   fails the document (`:1458-1477`). `TaskFieldWrite { value, clock }`
   (`:188-195`); the tasks section carries **no tombstones** — a
   cleared field is an ordinary write (`:190-192`).
4. **GAP 1 — the overlay builder never emits recur.**
   `task_entry_for` (`appdata.rs:976-1016`) writes `effort`,
   `importance`, `urgency` (+ `scheduled` only while
   `!native_scheduled`) — `recur` is absent, so a device holding a
   rule never publishes it. The flush lane consumes exactly this
   builder (`taskboard-sync-nextcloud/src/flush.rs:237-240`).
5. **GAP 2 — the fold never applies recur.** `fold_task_app_data`
   (`merge.rs:575-671`) matches key arms `effort`/`importance`/
   `urgency`/`scheduled` and `_ => continue` — a document recur write
   never folds. The doc-wins apply arm for `ModeledValue::Recur`
   exists (`:661-664`) but is unreachable. The function also carries a
   stale `#[allow(dead_code)]` (`:574`) — its call site is
   `pipeline.rs:973` (§5b).
6. **The intent plumbing is done.** `PendingTaskIntent.recur` exists
   (`pipeline.rs:111`), the outbox derivation sets it (`:139`), and
   `keep_task_intent` guard-copies value + clock (`:240-242`). Only
   the two gaps above remain on the wire path.
7. **`apply_sync_report`'s fixed order** (`pipeline.rs:433-1328`):
   §1 push outcomes (echoes bind; `task_by_board_card` built post-echo)
   → §2 board → §2b crash-window stack duplicates → §3 stacks →
   §4 labels → §4b crash-window card duplicates (create signatures,
   `detect_crash_window_duplicates`, `collapse_for_task`: rebind /
   `dup_suppressed` / `DeleteRemoteCard` enqueues / satisfied-op
   completes `true`) → §5 the task-merge loop (R4 fast path,
   `dup_suppressed` adoption skip, R1 adoption) → §5b the sidecar fold
   (per-card tasks/rules/dismissals via `task_by_board_card`, then the
   board-level time-entries fold) → §6 R3 presence reconciliation
   (+ resurrect carve-out) → §7 the persistence batch
   (`resurrect_enqueues`, `dup_enqueues`, op settlements, entity diff).
8. **The crash-window precedent's vocabulary** (DR-4/R):
   `DuplicateCollapse` (`merge.rs:1021-1034`); survivorship =
   oldest member, exact-stamp tie keeps the bound card, deterministic
   under arrival order (`collapse_for_task`, `merge.rs:1150-1190`);
   `dup_window_contains` (60 s); the pipeline applies verdicts with
   typed `tracing::debug!` under `RULE_R_DUP` (`pipeline.rs:810-858`).
9. **The neutralization shape**: `reconcile::neutralized_task`
   (`reconcile.rs:458-468`) — `deleted = true`, the binding trio
   stripped (`remote`, `remote_seen`, `remote_baseline` → `None`),
   `clocks.deleted = max(local, bound)`. This is the ref-less
   tombstone §3.6 names.
10. **The ref-delete primitive exists**: `LocalOp::DeleteRemoteCard {
    board, card }` (`outbox.rs:60-65`) — completes under `Applied` and
    under `RemoteMissing` with no tombstone side effect. DR-4/R built
    it; the doomed twin's card is deleted server-side through it so
    no later cycle can re-adopt the twin.
11. **`op_results` polarity** (`pipeline.rs:1254-1274`): `true` ⇒
    `CompleteOp`, `false` ⇒ `FailOp`; any `false` dead-letters the op
    and stamps the board phase `Failed { BadRequest }`. DEC-6's rule
    (a collapse-resolved create completes `true`) is why §4b's
    satisfied arm uses `true`.
12. **`merge_task_pair`** (`reconcile.rs:359-454`) is the field-wise
    LWW pair merge: tie keeps the bound row's value, clocks are the
    field-wise max, label memberships OR-set fold key-by-key, the
    composite position is one field; it fixes `id = local.id`,
    `remote = bound.remote`, `remote_baseline = None`.
13. **The recur domain is fully landed** (DR-6/1): `recur.rs` —
    `RecurRule { series, anchor, pattern, bound, continued_from }`
    with the A1 minted components (`EveryDays`, `EveryWeeks`,
    `WeekdaySet`, `DayOfMonth`, `NthWeek`, `OccurrenceCount`,
    `MonthlyAnchor`); `roll_reference_date(task, day)` (`:695-699`) =
    scheduled's local date, else duedate's — the roll's reference and
    this chunk's key share one derivation. `Task.recur` +
    `TaskClocks.recur` + `TaskChanges.recur` + `TASK_FIELDS` row all
    present (`entities.rs:260-273`, `:523`).
14. **`TaskId` is UUIDv7** (`idgen.rs` — `UuidV7Generator`, every id
    kind `now_v7()`): time-ordered, so **older-created is derivable
    from the id order** — no `created_at` column exists on `Task`
    (`entities.rs:491-531`) and none is needed.
15. **The spawn's shape** (DR-6/2, `command.rs:529-590`): the
    successor is `head.clone()` with a fresh v7 id, the binding trio
    stripped, newborn clocks all at the spawn instant, scheduled/
    duedate rebuilt at `next_after`'s date with the head's local
    time-of-day, `recur` copied verbatim, `UpdateAppData` enqueued. A
    dateless head spawns a **dateless** successor (both `.map()`s stay
    `None`) — dateless recur tasks exist.
16. **The roll** (`command.rs:604-670`): stale = live + open +
    recur-bearing + reference (scheduled else duedate; dateless never
    stale) strictly before today; `roll_target` is deterministic per
    `(pattern, anchor, stale, today)` — two devices converge on the
    same date (D-C7's "rolls need no dedup").
17. **`DayCalendar`** (`days.rs:92-120`): `Copy`, fields
    `day_start_time` + fixed `offset`; `calendar_date(utc) -> NaiveDate`
    and `instant(date, tod) -> utc` are the conversions.
    `apply_sync_report` does **not** take one today; the engine has
    `cfg.calendar` in scope (`taskboard-state/src/config.rs:50-56`,
    used by `plan_day_rolls` at `actor.rs:1188`; the ingest call site
    is `actor.rs:768`).
18. **The harnesses**: `test_support::recur::recur_rule_strategy()`
    (proptest rules); the wiremock actor journeys
    (`taskboard-sync-nextcloud/tests/actor_wiremock.rs` —
    `mount_standard_pull`, `repo_with`, `spawn`, `nudge_and_report`,
    document-body assertions at `:2843`); flush fixture tests in
    `flush.rs`; `e2e_hermetic.rs` `DeckSim` if a full journey needs
    the sim. No tracing-capture utility exists anywhere.
19. **Document GC** (D-S2): `prune_absent_cards` drops tasks/rules/
    dismissals rows keyed by absent cards (`appdata.rs:940-967`) —
    after the ref-delete lands, the doomed card's document rows die on
    a later flush. Board-level sections are invisible to it (fact 8 of
    the design); the tasks rows are not board-level.
20. **§4b and §5b/§5c compose**: §4b runs pre-adoption (snapshot cards
    vs create signatures, `dup_suppressed` consulted in §5); §5b's
    fold is what applies a twin's recur fact in its adoption cycle.
    A race discovered *late* (both twins long-bound, no create
    signature anywhere) is invisible to §4b by construction — that is
    the case only §5c can collapse.

## 3. The design, decision by decision

### R-1 — GAP 1: emit `recur` in the overlay row (the push half)

`task_entry_for` gains one full-send write beside the always-app keys:

```rust
let recur = ModeledValue::Recur(task.recur.clone());
fields.insert("recur".to_owned(),
    TaskFieldWrite { value: recur, clock: task.clocks.recur });
```

Always emitted (not `native_scheduled`-gated — recurrence has no
native Deck lane, design fact 11). `None` serializes as the typed
clear (`Value::Null`), which the remote side decodes as `Recur(None)`
— an ordinary write, never a tombstone (fact 3). Unit pins extend
`task_entry_for`'s existing tests: the row carries the rule JSON and
the clock; `None` emits the clear; serde round-trip stays lossless.

### R-2 — GAP 2: the `"recur"` fold arm (the pull half)

`fold_task_app_data`'s key match gains the arm the apply side already
anticipates (fact 5):

```rust
"recur" => (
    intent.recur,
    matches!(write.value, crate::appdata::ModeledValue::Recur(_)),
    write.clock,
    local.clocks.recur,
),
```

plus the guard-copy arm (intent protects a pending local document
write — copy value AND clock, the failed-flush rule at `:615-639`):

```rust
"recur" => {
    merged.recur.clone_from(&local.recur);
    merged.clocks.recur = l;
}
```

The doc-wins arm (`:661-664`) then applies `Option<RecurRule>` under
`max(local, doc)` unchanged. The stale `#[allow(dead_code)]` goes
away. Unit pins (T-4): doc newer adopts value+clock; local newer
stands; the guard copies both; a null doc write with a newer clock
clears; an undecodable value never reaches the fold (fact 3's demotion
is the existing pin — referenced, not duplicated).

### R-3 — the dedup predicate (property 7's subject), pure

New, beside the crash-window family in `merge.rs`:

```rust
pub(crate) const RULE_R_RECUR_DUP: &str = "R-recur-dup";

/// One series-twin group's verdict (§3.6, D-C7): the older-created
/// task survives, the rest are neutralized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesTwinCollapse {
    pub survivor: TaskId,
    pub doomed: Vec<TaskId>,   // ascending; every member but the survivor
}

/// The fold-time dedup predicate: groups live, open, recur-bearing
/// tasks by (rule.series, reference local date); any group of ≥ 2 is
/// a spawn race. Pure, total, injected data only; output order is
/// group-key ascending — deterministic under input order.
#[must_use]
pub fn detect_series_twins(
    tasks: &BTreeMap<TaskId, Task>,
    day: DayCalendar,
) -> Vec<SeriesTwinCollapse>
```

- **Membership**: `task.is_live() && task.done.is_none()` and
  `task.recur.as_ref()` is `Some` — the design's "two live, open
  tasks" verbatim. A done task is a settled occurrence, never a twin
  candidate.
- **Key**: `(rule.series, recur::roll_reference_date(task, day))` —
  the occurrence identity D-C5 names, derived through the exact
  function the roll uses (fact 13; one date vocabulary, no second
  derivation to drift). A `None` reference (dateless twin) is
  **unkeyed** — it joins no group and can never trigger (DEC-4).
- **Survivorship**: `survivor = min_by(TaskId)` — v7 ids are
  time-ordered (fact 14), so the minimum id is the older-created task,
  a total order needing no stamps, no clocks, no tie-break rule.
  Deterministic under arrival order by construction (it is a minimum,
  not a first-seen).

### R-4 — survivorship + neutralization (the pure merge)

```rust
/// The survivor absorbs the doomed twin field-wise under LWW clocks
/// and keeps its own identity and binding trio.
#[must_use]
pub fn series_survivor_merge(survivor: &Task, doomed: &Task) -> Task
```

Implementation: `reconcile::merge_task_pair(survivor, doomed)` —
field-wise LWW with clocks-max, the OR-set label fold, recur under its
clock slot (fact 12) — then restore the survivor's `remote`,
`remote_seen`, `remote_baseline` (merge_task_pair takes the *bound*
row's binding; the survivor's own card is the one that persists —
D-5c's "the first spawn owns the remote history"). The pipeline's
entity-diff writes the merged row; the document lane learns the fresh
clocks through R-5's `UpdateAppData` enqueue.

Neutralization of every doomed row — the `neutralized_task` shape
(fact 9) with `now` as the tombstone's floor:

```rust
let mut absorbed = doomed.clone();
absorbed.deleted = true;
absorbed.remote = None;
absorbed.remote_seen = None;
absorbed.remote_baseline = None;
absorbed.clocks.deleted = absorbed.clocks.deleted.max(now);
```

(Shape-faithful to `reconcile.rs:458-468`; stage 2 reuses
`neutralized_task` directly where the `max(local, bound)` semantics
already match — the doomed row is the `bound` argument — and stamps
`clocks.deleted` up to `now` so the tombstone LWW-wins every older
copy.)

### R-5 — the pipeline call site: new §5c, between §5b and §6

`apply_sync_report` gains a `day: DayCalendar` parameter (after
`features`; the engine passes `cfg.calendar`, fact 17 — the one
call site plus the test helpers update mechanically). The pass:

```rust
// ---- 5c. Series fold dedup (§3.6, D-C7, rule R-recur-dup) --------
// After §5b (the fold is what applies a twin's recur fact in its
// adoption cycle — detection must see it) and before §6 (the
// neutralized rows are ref-less, so R3's bound-ref listing must not
// see stale bindings; the doomed cards' presence is decided by the
// ref-deletes, not by R3).
```

Per verdict, in group order:

1. **Merge**: `merged = series_survivor_merge(&tasks[&survivor],
   &tasks[&doomed])` successively over the doomed vec; insert when it
   differs from the survivor row (§7's entity diff picks it up).
2. **Lane ops for the survivor**: when the merge changed anything,
   enqueue `LocalOp::UpdateTask(survivor)` (native lanes full-send)
   and `LocalOp::UpdateAppData(survivor)` (the document lane re-arms
   for every device — the merged clocks must publish). Over-enqueueing
   is the coalescer's job to absorb, not the pass's.
3. **Neutralize**: each doomed row replaced per R-4.
4. **Ref-delete the doomed cards**: for each stripped binding, one
   `PersistenceAction::EnqueueOp(PendingOp { op:
   LocalOp::DeleteRemoteCard { board: board_id, card }, queued_at:
   now })` — the same primitive §4b uses (fact 10). This is the
   cross-cycle suppression: the card stops existing server-side, so
   no later pull can re-adopt the twin. In-cycle there is nothing to
   suppress (the twins are already local rows; §5's `dup_suppressed`
   gate addresses snapshot adoption, which already happened) — and
   the pass is **idempotent**: in the residual window before the
   ref-delete drains, a re-pull can re-adopt the doomed card; the
   next cycle's §5c collapses it again, unchanged (self-healing;
   DEC-8).
5. **Settle the doomed twins' pending ops**: every outbox op targeting
   a doomed task — `CreateTask`, `AssignLabel`, `UpdateAppData`, the
   `op_targets_task` family (`pipeline.rs:1093` uses it) — is settled
   this cycle with `op_results.insert(op_id, true)` (CompleteOp,
   DEC-6's polarity: a collapse-resolved create must not stamp the
   board `Failed`, fact 11; the op's effect must *never* exist, and
   completing it quietly removes it from the queue with no durable
   lie — the only persisted record is the queue's absence). Unlike
   R5's resurrect drop (`queued_at <= read_at`), **no read-boundary
   guard**: the doom verdict is a state-level decision that outlives
   the read window; a create op queued after `read_at` would otherwise
   mint the twin's card next cycle and restart the race (DEC-7).
6. **Honesty**: one `tracing::debug!` per applied verdict —
   `rule = RULE_R_RECUR_DUP`, typed fields `series`, `date`,
   `survivor`, `doomed` (count), `ref_deletes`, `settled_ops`,
   `"recurring-series twin collapse applied"`. `detect_series_twins`
   traces nothing (unlike `collapse_for_task` — the grouping is pure
   and silent; the pipeline application is the fact worth one line).
   No `SystemEvent`, no UI churn (§3.6) — pinned structurally.

### R-6 — pass composition with §4b (no double collapse)

The two passes are disjoint by trigger class, and compose:

- **§4b** (crash-window): needs a *create signature* — a pending or
  this-cycle-echoed `CreateTask` op of *this device*. It fires
  pre-adoption, suppresses the twin card's R1 adoption, and rebinds.
  After it, only one open task carries the key → §5c is a no-op.
  Content equality (clause 4) compares raw UTC stamps, so it already
  fails harmlessly when the two devices' fixed offsets differ.
- **§5c** (this chunk): needs only two *local rows* sharing the key —
  the late-discovered race (both twins bound for cycles, no create
  signature anywhere on this device) is invisible to §4b by
  construction (fact 20) and is exactly §5c's case.
- A race twin pair can satisfy both predicates only in the narrow
  same-offset window where §4b fires first and prevents the
  adoption — §5c then sees one open task per key. No double collapse,
  no interaction to test beyond one composition pin (T-7e).

### R-7 — the storage pin (the "upgrade test", re-scoped)

**No migration exists to upgrade** (DEC-1): migration 0010 landed the
columns and the codec in DR-6/1 (fact 1). The dispatch's "migration
upgrade test" acceptance is satisfied by the chunk's storage test at
the SQL boundary — `taskboard-storage-sqlite`: create the real schema
in a tempdir (the repo test helpers' pattern), `upsert_task` a task
carrying `Some(rule)` + `clocks.recur`, reopen the repository, load,
assert the rule and the clock byte-equal. This pins what the dispatch
actually wanted from the upgrade test: a persisted recur row survives
a full close/reopen through the real codec, not just the text
round-trip (fact 2's gap). No `.sqlx` churn (zero new or changed
queries).

### R-8 — the wiremock journey (dispatch acceptance verbatim)

*"Adopt an occurrence card, learn the series"* — in
`tests/actor_wiremock.rs`, beside the existing journeys:

1. Seed the repo with the bound board/stack (the `repo_with` pattern)
   and **no recur state anywhere**.
2. Mount the standard pull with two cards; the anchor attachment (the
   appdata document, the real JSON the flush uploads) carries a
   `tasks` row for the second card: `{"recur": {"v": <rule JSON>,
   "clock": T1}}` — the occurrence's series fact, published by the
   other device.
3. Nudge one cycle. Assert, typed: the adopted task's `recur` equals
   the rule (the series was learned through the real wire: document
   JSON → decode → §5b fold → state); the storage repo persists it
   (the load path of R-7); the second card carries no duplicate row;
   `system_events` stays empty.
4. Nudge again: idempotent — no new rows, the rule unchanged (the
   fold's clock comparison rejects the stale re-read).

The flush half (a local rule reaching the wire) is pinned at the unit
layer: `task_entry_for` emits the write (R-1) and the flush overlay
assembly passes it through — one `flush.rs` fixture assertion on the
overlay row (the fixture tests already assert `sections.tasks` rows
this way), not a second wiremock journey.

### R-9 — the fold-dedup journey (§5c end-to-end)

One `apply_sync_report` integration journey (pipeline tests, hand-
built state — the crash-window tests' template): two bound, open,
recur-bearing tasks sharing `(series, date)` with independent cards
and clocks; one cycle → one survivor with merged fields and clocks,
one ref-less tombstone, one `DeleteRemoteCard` op, the doomed row's
pending ops settled, `system_events` empty. This is T-6/T-7's
positive; the wiremock actor needs no second journey (the pass is
domain-internal; the actor is a translator — the boundary stays
pinned by the existing report-shape tests).

## 4. What does NOT change

- **No migration, no schema change** (DEC-1) — 0010 already carries
  the columns; the dispatch's ladder deviation is recorded, not
  enacted. DR-6/6's and undo/1's later migration numbers renumber at
  their own dispatches per §2's rule (0016 is next free; this chunk
  does not take it).
- **No new appdata section, no new `ModeledValue` arm, no decode
  change** — DR-6/1's leniency posture is load-bearing and untouched.
- **No `LocalOp` variant, no push/executor/planner change** — the
  ref-delete primitive exists (fact 10); the flush lane consumes
  `task_entry_for` as-is.
- **No `AppState` field, no insta snapshot regeneration, no config
  knob, no CLI/UI/IPC diff.**
- **The spawn, the roll, and their planner tests** (DR-6/2) — the
  dedup reads their output; nothing feeds back into `command.rs`.
- **`reconcile.rs`'s `plan_reconcile`** (the connect-time bind
  ceremony) — different duplicate class (local-vs-local); only the
  `merge_task_pair`/`neutralized_task` vocabulary is reused.
- **The engine's structure** — one argument through one call site
  (`cfg.calendar`).

## 5. Test plan (TDD order)

Lowest layer first; one behavior, one test; typed outcomes only.
Proptest strategies from `test_support` (`recur_rule_strategy`,
entity strategies); `CountingIds` mints *ascending* ids, so twin
fixtures control "older-created" by minting order and by explicit
v7-shaped ids where order is the subject.

- **T-1 (appdata.rs, R-1)**: `task_entry_for` emits the `recur`
  write — rule JSON + clock; `None` emits the typed clear; serde
  round-trip lossless (property 8's row half, with
  `recur_rule_strategy`).
- **T-2 (merge.rs, R-2)**: the fold arm — doc newer adopts value +
  clock (both `Some` and the `None`-clear case); local newer stands;
  the intent guard copies value + clock; a row *without* the key
  folds nothing (truncation: absence never resurrects a locally
  cleared rule — property 8's guard half).
- **T-3 (merge.rs, R-3 — property 7)**:
  - *No-twin no-op* (proptest): generated task sets where every
    (series, date) group has ≤ 1 open member — `detect_series_twins`
    returns empty, for arbitrary mixes of done/live/plain/dateless
    rows.
  - *Permutation invariance* (proptest): generated twin groups,
    shuffled input order (multiple members per group, groups
    interleaved) — the verdict set is identical every shuffle;
    survivor = min id, doomed = the rest ascending.
  - Unit near-misses: two open tasks, same series, *different* dates
    ⇒ no verdict (the reopen-while-successor-lives shape, T-3b);
    two open tasks, same date, *different* series ⇒ no verdict;
    same series same date but one is *done* ⇒ no verdict; both
    dateless ⇒ no verdict (DEC-4's fence); a tombstoned (not live)
    member is invisible.
- **T-4 (merge.rs, R-4)**: `series_survivor_merge` — the survivor's
  id/binding trio survive; per-field LWW (the doomed row's newer
  title/labels/scheduled win, the survivor's newer effort wins);
  clocks are the field-wise max; label OR-set fold. The
  neutralization shape: deleted + ref-less + clock floor (T-2-style
  direct asserts on the returned row).
- **T-5 (pipeline.rs, §5c integration — the chunk's core pins)**:
  - T-5a: the canonical race — both twins bound, independent clocks:
    survivor merged (fields + clocks + labels), doomed neutralized
    ref-less, `DeleteRemoteCard` enqueued with the doomed card's ref,
    survivor `UpdateTask` + `UpdateAppData` enqueued,
    `system_events` empty.
  - T-5b: the doomed twin's pending `CreateTask` (the unbound arm —
    survivor is the adopted remote twin): the op settles `true`
    (board phase stays `Idle`, fact 11), the row is neutralized
    without a ref-delete (it had no card).
  - T-5c: no twin ⇒ the pass is a no-op — output state, actions and
    ops equal a pre-chunk run's (the structural no-op, integration
    level; the proptest in T-3 is the predicate level).
  - T-5d: the false-positive fences through the full pipeline —
    reopened occurrence + successor (different dates), dateless
    pair, done twin ⇒ zero structural effects.
  - T-5e: composition with §4b — a crash-window twin (create
    signature present, same title/content/window) collapses in §4b;
    §5c fires nothing afterward (exactly one survivor, one
    ref-delete from §4b, none from §5c).
  - T-5f: the late race (both twins long-bound, no signatures) —
    §5c fires where §4b structurally cannot (fact 20's pin).
  - T-5g: idempotence — applying the same report twice collapses
    once; a re-adopted doomed card (the ref-delete still in flight)
    collapses again to the same survivor.
- **T-6 (storage, R-7)**: the tempdir round-trip — upsert with
  `Some(rule)` + clock, reopen, load byte-equal (fact 2's gap; the
  dispatch's upgrade-test acceptance, re-scoped by DEC-1).
- **T-7 (actor_wiremock.rs, R-8)**: the adopt-an-occurrence-learn-
  the-series journey per §3-R8, including the idempotent second
  cycle and the empty events vector.
- **T-8 (flush.rs, R-1's lane)**: the overlay fixture asserts the
  uploaded document's `tasks` row carries the local rule (the flush
  half of the wire path).
- **T-9 (property 8, end-to-end proptest)**:
  `recur_rule_strategy` → task → `task_entry_for` row → serialize →
  decode → fold (doc clock newer) → state rule equals the original
  — lossless through the whole chain, generated rules. (T-1/T-2 pin
  the halves; T-9 pins the composition.)

Mutant targets: the emit arm's clock source, the fold arm's guard
copy, the key's date derivation (scheduled-else-duedate — a mutant
that reads only `scheduled` must fail T-3's duedate-referenced
twin), the min-id survivorship (a mutant keeping *first-seen* fails
the permutation property), the binding-trio restore, the op-settlement
polarity (a `false` mutant flips the phase pin in T-5b), the
ref-delete's board/card fields.

## 6. Implementation order (stage-2 guidance)

1. T-1 + the emit arm (R-1) — smallest, unblocks the wire halves.
2. T-2 + the fold arm (R-2); drop the stale `#[allow(dead_code)]`.
3. T-3/T-4 — the predicate + survivorship (property 7 + the merge).
4. T-5 — the `day` parameter + §5c composition.
5. T-6 — the storage round-trip.
6. T-7/T-8/T-9 — the journeys and the end-to-end property.
7. Full gate suite per AGENTS §10 through
   `scripts/run-quality-gates.sh` (the shared-lock wrapper; parallel
   worktrees are live).

## 7. Decision record (every open question, resolved)

- **DEC-1 — the migration is already landed; the chunk takes none.**
  The dispatch assigned migration 0014 (`tasks.recur TEXT NULL` +
  `ck_recur`, §2's ladder) and the stage-1 prompt says "verify 0014
  next free". Verification found both halves already on `origin/main`:
  DR-6/1's `0010_recur_field_family.sql` added exactly those columns
  with the codec arm in the same change (fact 1), and migrations have
  since advanced to 0015 (DR-5/2's time tracking). Enacting the
  dispatch's letter would re-add existing columns — impossible.
  Recorded deviation per dispatch §1 rule 3 ("the chunk takes the
  next free number at its rebase and records the deviation") — the
  number this chunk takes is **none**. Consequence: the "migration
  upgrade test" acceptance re-scopes to the SQL-boundary round-trip
  (R-7, T-6) — the codec pin the acceptance actually wants, since no
  schema transition exists for recur rows to survive. Ladder
  bookkeeping for later waves (not this chunk's change): undo/1's
  claimed 0015 is taken (`0015_time_tracking`), DR-6/6's claimed 0016
  is next free at this base.
- **DEC-2 — the "appdata facts" work is two one-arm gaps, not a
  shape change.** The dispatch's "the tasks-row recur facts (serde
  shape, leniency at decode)" read as future work, but DR-6/1 landed
  the serde shape, the typed arm, the null-clear, and the demote-to-
  verbatim leniency (fact 3), plus the intent plumbing (fact 6).
  What never landed is the emit (R-1) and the fold arm (R-2) — the
  wire path was decoded-but-dead both directions. The chunk's appdata
  diff is exactly those two arms; the "no new section" rule is
  honored trivially.
- **DEC-3 — the pass is §5c, after the sidecar fold, before R3.**
  The design says "after the task-merge loop" (§3.6); §5b also sits
  after that loop, and the dedup must run *after it*: a twin adopted
  this cycle learns its rule through the fold (fact 20) — detecting
  before the fold misses the canonical adoption-cycle race. Running
  before §6 keeps the neutralized rows (ref-less by then) out of R3's
  bound-ref listing and lets §7's single entity diff persist
  everything. This is a position refinement within the design's
  constraint, same spirit as DR-4/R's DEC-2.
- **DEC-4 — dateless twins never trigger.** The key's date component
  is `roll_reference_date` (scheduled, else duedate); a dateless task
  is unkeyed and joins no group. Rationale: occurrence *identity* is
  `(series, scheduled local date)` by ruling D-C5 — a dateless
  occurrence has no identity component, and treating `None` as a
  groupable key would collapse the design's named false-positive
  (a user reopening a completed occurrence while its dateless
  successor lives — §5.5's analysis, whose safety argument "same
  date impossible" has no date to lean on). Cost: a spawn race
  between two dateless heads never auto-collapses (the spawn chain
  of a dateless series is the only producer — fact 15). Accepted
  imprecision, recorded in Risks; the twin pair stays visible as two
  open cards and the user resolves it like any pre-dedup duplicate.
- **DEC-5 — survivorship is `min(TaskId)`, no tie-break rule.** v7
  ids are mint-time-ordered (fact 14), so id order *is* creation
  order — a total order with no ties (ids are unique), needing no
  stamps (Task carries no `created_at`, fact 14) and no bound-card
  exception (both twins are always local rows here, unlike §4b's
  card-vs-signature world). The D-5c rule "keep the older-created"
  is thus literal, permutation-invariant by construction, and
  property 7's subject.
- **DEC-6 — pending-op settlement polarity is `true`.** A doomed
  twin's pending create never ran and must never run; the honest
  *outbox* fact is "resolved, not rejected": `false` would dead-letter
  the op AND stamp the board `Failed{BadRequest}` (fact 11) — wrong
  for a silent reconcile, exactly DR-4/R's DEC-6 reasoning. No
  durable state records the completion; the tracing fact
  (`settled_ops`) is the audit trail.
- **DEC-7 — settlement ignores the `read_at` boundary.** R5's
  resurrect drop settles only ops the cycle's read actually carried
  (`queued_at <= read_at`) because resurrection re-opens the question
  of server state. The doom verdict is the opposite: a permanent,
  state-level decision (the row is tombstoned). An op queued after
  `read_at` that survived settlement would push the twin's card next
  cycle and restart the race — the suppression must cover the whole
  queue slice. Recorded as a deliberate divergence from the R5
  template, with the reason.
- **DEC-8 — cross-cycle suppression is the ref-delete + idempotence,
  not a persisted tombstone memory.** §3.6's "suppress any pending
  duplicate adoption" is realized as: (a) the doomed card is deleted
  server-side (`DeleteRemoteCard`, the §4b primitive), so no later
  snapshot carries it; (b) within the drain window, a re-adopted card
  re-collapses (the pass is stateless and idempotent — the twin's
  re-adoption shares the key, the same min-id survivor wins, the
  merged row converges). A persisted "suppressed cards" memory was
  rejected: new durable state, a new GC problem, and no property the
  ref-delete + idempotence doesn't already give.
- **DEC-9 — the `day` parameter joins `apply_sync_report`.** The key
  needs the device's day calendar (fact 17); the engine already holds
  it (`cfg.calendar`, the day-tick's vocabulary) and the spawn/roll
  planners take it the same way. `merge_task`-class purity is
  preserved: `DayCalendar` is injected data (Copy, two plain fields),
  not a clock — the functions stay total and proptest-able with a
  fixed calendar.
- **DEC-10 — survivor lane ops enqueue unconditionally on change.**
  The merged survivor may differ in native fields, appdata-lane
  fields, or both; the pass enqueues `UpdateTask` + `UpdateAppData`
  when `merged != survivor` rather than diffing per-lane. Rationale:
  the lanes' own push planners full-send and coalesce; a
  lane-precise diff here duplicates that machinery for one op saved.
  Honest over clever; the coalescer absorbs it.
- **DEC-11 — the honesty surface is one applied-verdict trace per
  group, no formation trace.** §3.6 names the tag `R-recur-dup` and
  "typed facts only". Unlike `collapse_for_task` (which traces at
  formation because the pipeline consumes verdicts across sections),
  §5c forms and applies in one place — one trace with `series`,
  `date`, `survivor`, `doomed`, `ref_deletes`, `settled_ops`. No
  `SystemEvent` (pinned empty in T-5a/T-5d/T-7), no UI surface.
- **DEC-12 — property 8's "truncated views fold as tombstones" reads
  as absence-never-resurrects.** The tasks section carries no
  tombstones (fact 3); a truncated row (the `recur` key missing)
  folds nothing, and a locally cleared rule stays cleared — the
  T-2 pin. The DR-5/2-style explicit tombstone derivation does not
  apply to per-task rows (their GC is `prune_absent_cards`, card-
  keyed, fact 19); the acceptance's phrasing maps to the
  absence-is-not-a-clear pin plus the existing decode-leniency
  tests.
- **DEC-13 — recorded at stage 3: the wire journey lives in
  `e2e_hermetic.rs`, not `actor_wiremock.rs` (R-8/T-7's named
  file), and it carries no separate idempotence nudge.** The journey
  still is a wiremock journey — a real `wiremock` server over real
  HTTP — but it runs the full state engine plus the real storage
  repo, which is what the acceptance actually wants (the persisted
  repo row is asserted); the actor-level harness could not. The
  re-read idempotence R-8 step 4 wanted is a duplicate through a
  higher layer: T-5g pins the pass's idempotence at the pipeline,
  and AGENTS §5 keeps higher layers to routing only.

## 8. Risks

- **Cross-offset midnight wobble (detection may miss, then
  self-heal).** Two devices with different fixed UTC offsets convert
  the same occurrence date to UTC instants an offset-delta apart; a
  device deriving the *other* twin's local date near midnight can
  compute a neighbor date → no key match that cycle. The day-boundary
  roll converges both heads onto the same local date (deterministic
  target, fact 16), after which §5c fires. One-cycle delay in an
  exotic corner (offset mismatch ∧ occurrence at local midnight);
  accepted, no machinery added.
- **Dateless race twins never auto-collapse** (DEC-4's cost). Two
  open cards, same series, no dates — visible to the user, resolvable
  by hand; the alternative (keying `None`) creates the design's named
  false-positive. Revisit only with evidence a dateless series race
  actually occurs.
- **The `day` parameter is a wide mechanical churn** — every
  `apply_sync_report` test call site gains an argument. Contained to
  `pipeline.rs`'s test module and the one engine call site; no
  semantic migration risk.
- **Ref-delete drain window** (shared with §4b, documented there):
  a collapse followed by a board unbind before the delete drains
  leaves the doomed card server-side; benign pollution, the next
  bind cycle drains it. §5c adds no new class (same primitive, same
  window).
- **Survivor-merge semantics under hostile clocks**: a twin carrying
  a far-future clock wins every LWW field (true of every LWW merge in
  the house, including §5b's document fold); no new exposure — the
  twins are device-spawned rows, not foreign input.
- **Journey brittleness**: the wiremock journey embeds a hand-written
  document JSON; a format-version bump (`APPDATA_FORMAT`) would fail
  it loudly, not silently — the fixture mirrors what `flush.rs`
  uploads (fact 18's `:2843` precedent).

## 9. Estimated diff size

| Area | Files | Lines (incl. tests) |
|---|---|---|
| overlay emit arm | `taskboard-domain/src/appdata.rs` | ~60 |
| fold arm + rule tag | `taskboard-domain/src/merge.rs` | ~60 |
| predicate + survivorship | `taskboard-domain/src/merge.rs` | ~140 |
| §5c + `day` param | `taskboard-domain/src/pipeline.rs`, `taskboard-state/src/actor.rs` | ~220 (incl. call-site churn) |
| pipeline integration tests | `pipeline.rs` tests | ~300 |
| property 8 proptest + strategies | `pipeline.rs`/`appdata.rs` tests, `test_support` | ~120 |
| storage round-trip | `taskboard-storage-sqlite` tests | ~90 |
| wiremock journey | `taskboard-sync-nextcloud/tests/actor_wiremock.rs` (+ common) | ~200 |
| **Total** | | **~1100–1300** (production ~200–250) |

Size M, matching the dispatch table. No migration, no snapshot
regeneration, no new dependency, no `LocalOp` change.

## 10. References

- Design: `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §3.6, §3.9 props 7–8, §1 facts 8–11, §5.4, §6 D-C1–D-C13, §7 chunk 3.
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1, §2, §3.5, §11.1.
- Precedents in-tree: DR-4/R's collapse family (`merge.rs:1004-1190`,
  `pipeline.rs:728-860`), the bind ceremony's neutralization
  (`reconcile.rs:283-468`), DR-5/2's fold + GC (`merge.rs:575-671`,
  `appdata.rs:940-1016`), DR-6/1's recur types and codec
  (`recur.rs`, `codec.rs:98-110`), DR-6/2's spawn/roll
  (`command.rs:520-670`).
- Issues: #500 (this chunk — stage 3 closes it), #498/#499 (the
  landed series domain + engine), #487 (the landed merge-lane
  predecessor). Habits are NOT this chunk (DR-6/6, dispatch §2).
