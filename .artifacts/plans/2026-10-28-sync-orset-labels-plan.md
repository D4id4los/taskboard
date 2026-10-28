# [Plan] OR-set label memberships — DR-4/L (implementation)

- **Date**: 2026-10-28
- **Kind**: implementation plan for chunk DR-4/L of the design-rounds
  batch dispatch. Implements the accepted design verbatim — origin
  §3 D-2 a–e is normative and the D-2d outcome table is binding
  verbatim; this file adds the record-level decisions the design
  delegated, the seam inventory, and the TDD commit sequence. No
  fork re-litigation: F-129 is closed (build, per §4 of the origin
  plan).
- **Origin**: design
  `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
  (§3 D-2; §2 facts 2, 8; §4 F-129; §5 the reader-sweep risk);
  dispatch `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§2/§7.1; issues #129 (origin), #482 (implementation).
- **Normative inputs** (verified against `origin/main` @ `9af0910`,
  the chunk's base; line numbers are from that tree):
  - `crates/taskboard-domain/src/entities.rs` — `Task.labels`
    (`:483`, the retiring bare set), `TaskClocks.labels` (`:294`,
    the retiring whole-set clock), `TASK_FIELDS` labels row
    (`:265`), G's `RemoteTaskBaseline` with its
    `labels: BTreeSet<RemoteLabelId>` (`:342-384`).
  - `crates/taskboard-domain/src/merge.rs` — `remote_wins` (strict,
    `:71-73`), `BaselineDiff`/`diff_baseline` (`:75-105`), R1
    `adopt_remote_task` (`:122-165`, labels `:153`), `clocks_from`
    (`:167-184`), `merge_task`'s changed-field gate
    (`:233-250`) and the R7 whole-set branch (`:296-302`), R9
    `adopt_task_after_push` (`:649-719`, labels `:691`, whole-set
    clock `:706`), `tombstone_task` (`:625-640`), R9c
    `finalize_pushed_task_delete` (`:725-734`).
  - `crates/taskboard-domain/src/command.rs` — the `AssignLabel`
    (`:951-968`) / `UnassignLabel` (`:969-985`) planners; the
    `DeleteLabel` planner's "task label sets stay untouched" note
    (`:933-934`).
  - `crates/taskboard-domain/src/pipeline.rs` — `PendingTaskIntent`
    (`:94-111`, `labels: bool` at `:102`), `pending_task_intents`
    (`:114-181`), `keep_task_intent` (`:187-244`, labels arm
    `:236-239`), the task-merge loop (`:601-672`, R4 gate
    `:619-622`), the R3 label-absence block (`:788-800`, no op
    cancellation today), the lockstep assertions
    (`:1484-1595`).
  - `crates/taskboard-domain/src/push.rs` — `NewCardShape` /
    `CardShape` (`:160-226`, `labels` cloned from the task at
    `:208`/`:223`), `plan_label_groups` (`:571-603`).
  - `crates/taskboard-domain/src/reconcile.rs` — the label edge
    rewrite (`:171-189`), `merge_task_pair` (`:353-443`, labels
    pick `:393-397`, whole-set clock max `:431`).
  - `crates/taskboard-domain/src/outbox.rs` —
    `LocalOp::AssignLabel/UnassignLabel` (`:47-49`),
    `op_targets_task` (`:110-116`), `owning_board` (`:93-99`) —
    all id-only, **unchanged**.
  - `crates/taskboard-domain/src/persistence.rs` —
    `PersistedState` embeds `Task` directly (`:88`); no
    field-by-field mirror exists, so the serde shape rides the
    entity change (in-memory only — the sqlite codec is the
    on-disk boundary).
  - `crates/taskboard-storage-sqlite/` — `task_labels(task, label)`
    PK with double `ON DELETE CASCADE`
    (`migrations/0001_init.sql:80-85`); `tasks.ck_labels TEXT NOT
    NULL` **without a DEFAULT** (`0001_init.sql:58`); the codec's
    load join (`src/repo.rs:605-627`), `decode_task` label_sets
    (`:314-331`) and `ck_labels` read (`:384-387`), the upsert's
    delete-then-rewrite (`:1165-1181`) with `ck_labels` written at
    `:1145`; pins: `tests/schema.rs:74-82` (task_labels columns are
    exactly `{task, label}`), `tests/roundtrip.rs` T5 (`:541-575`,
    upsert keeps label rows), S3 upgrade precedent (`:963-1005`).
  - `crates/taskboard-sync-nextcloud/` — card-PUT label resolution
    (`src/push_exec/execute.rs:610-630`, through the overlay,
    unresolved ⇒ deferred); the bench mirror of the two
    `from_task` shapes (`benches/support/mod.rs:250,266`). The
    remote DTO/mapping `labels` fields (`mapping.rs`, `model.rs`)
    are remote vocabulary and **out of scope**.
- **Branch / worktree**: `feat/sync-orset-labels` in
  `../taskboard-WT-feat-sync-orset-labels`, off `origin/main` @
  `9af0910` (worktree-doctor passed: 245G free, floor 25G).
- **Precondition verified**: DR-4/G merged — `bb6149e` (feat),
  `8732182` (test), `397d364` (ADR) all in `origin/main`; the
  remote branch was pruned post-merge, which is expected.
- **Status**: DRAFT — handed to the stage-2 implementation agent.

---

## 1. Mission & binding constraints

Label assignments stop being a whole-set LWW field and become
per-membership OR-set state: two devices adding different labels
concurrently both survive; a remove wins only when strictly newer;
removed memberships are remembered so a stale remote re-add cannot
flap. The design's constraints bind unchanged (origin §1): purity
(all stamps injected data, no clocks in the algebra), additive
storage (ADR 0005), typed outcomes / no text assertions / proptest
discipline (AGENTS §5), A1-shaped new state, the self-clobber guards
survive unchanged (fact 7), the card PUT's wire shape is unchanged.

Chunk-specific bindings from dispatch §7.1's acceptance list:
concurrent adds both survive; remove wins only strictly newer;
removed rows are never deleted (re-add flap pinned impossible);
unchanged remote sets emit no events; entity death forces removal and
kills concurrent assignment; upgrade test pins epoch backfill
bit-for-bit; every former `task.labels` reader compiles against the
accessor; gates green. Insta dumps regenerate (the (w) shape-widener
of wave 4 — the sibling DR-2/D chunk touches no domain shape).

## 2. Verified baseline — numbered facts (current tree)

1. `Task.labels: BTreeSet<LabelId>` (`entities.rs:483`) is read by
   the planners (`command.rs:954/972`), merged whole (R7,
   `merge.rs:296-302`, gated by `remote_wins(ts_r, clocks.labels)
   && changed.labels`), rebuilt wholesale by R1 (`:153`) and the R9
   echo (`:691`), cloned into both push shapes (`push.rs:208/223`),
   re-keyed by the bind ceremony (`reconcile.rs:177-178`), and
   picked whole by the pair merge (`reconcile.rs:393-397`).
2. `TaskClocks.labels` (`entities.rs:294`) has exactly five
   consumers: the R7 gate, the echo's `max_ts` stamp
   (`merge.rs:706`), `clocks_from` (`:175`), the pair merge
   (`reconcile.rs:431`), and the storage round-trip
   (`repo.rs:384-387/1145`). The lockstep assertions pin its serde
   key against `TASK_FIELDS` (`pipeline.rs:1484-1517`).
3. `PendingTaskIntent.labels: bool` (`pipeline.rs:102`) guards the
   whole set by cloning the local value + clock over the merge
   result (`:236-239`); label ops feed it (`:174-176`).
4. The R3 label-absence block (`pipeline.rs:788-800`) tombstones the
   label entity and nothing else — no membership effect, no queued
   label-op cancellation (unlike the task R3 block, which cancels).
5. The verbs guard on set containment today: `AssignLabel` no-ops
   when the label is present, `UnassignLabel` when absent
   (`command.rs:954-956/972-974`); both stamp `clocks.labels = now`.
6. The bind ceremony's `merge_task_pair` resolves labels by
   whole-set clock LWW (tie ⇒ bound row) and maxes the clock
   (`reconcile.rs:357/393-397/431`); the edge rewrite swaps the
   absorbed label's id for the survivor's in every candidate task
   (`:171-189`).
7. `task_labels` is a bare `(task, label)` join (`0001:80-85`);
   `tasks.ck_labels` is `TEXT NOT NULL` with **no DEFAULT**
   (`0001:58`), so every INSERT must keep supplying a value. The
   codec rewrites the join per task upsert (delete-then-insert,
   `repo.rs:1165-1181`); T5 pins that an upsert never loses the
   rows (`roundtrip.rs:541-575`).
8. Migrations 0001–0011 exist. **0010 is DR-6/1's recur family
   (`1fa9c14`, landed before G) and 0011 is G's remote baseline
   (`bb6149e`)** — the dispatch §2 ladder assigned 0011 to this
   chunk, but its own landing-order rule moves the number: **the
   next free slot is 0012** (deviation recorded in §3 L-10;
   DR-3/2's stage-1 re-verifies at its own base per dispatch §2).
9. The UI renders no task label chips: every ui-slint occurrence is
   a `Task`/`TaskClocks` struct literal in tests or fixtures
   (inventory in §4). The app's CLI reads the set twice
   (`labels.rs:76`, `output.rs:810`).
10. `RemoteIndex` resolution is per-cycle; the baseline stores raw
    remote ids (G). The pull's label-event derivation therefore
    happens in remote vocabulary against `baseline.labels`, then
    maps through `ctx` — the same discipline as R1/R7 today.
11. The state crate's `manifest_routing.rs` hard-codes the labels
    row's "own verbs" partition (`:352-365`) and excludes labels
    from `TaskChanges` routing (`:161-164`) — labels never rode
    `UpdateTask`, and still don't.

## 3. The design, decision by decision

### L-1 — `MembershipState` and the membership map (A1 shape)

In `entities.rs`, next to `TaskClocks`:

```rust
/// Per-membership OR-set state of one (task, label) assignment
/// (ADR 0004's labels amendment, #129). `Removed` is the membership's
/// tombstone: it persists — forgetting a removal lets a stale remote
/// re-add flap (the classic OR-set requirement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipState {
    /// The assignment is live since `at`.
    Added { at: DateTime<Utc> },
    /// The assignment was removed at `at` (remembered, never deleted).
    Removed { at: DateTime<Utc> },
}

impl MembershipState {
    /// The membership's write clock (either variant's stamp).
    #[must_use]
    pub const fn at(self) -> DateTime<Utc> { /* variant's `at` */ }
}
```

On `Task`, the retiring pair is replaced by the one map the design
names (D-2a):

```rust
/// Label memberships (the OR-set map, #129): every label the task has
/// ever referenced, `Added` = live, `Removed` = remembered tombstone.
/// The live set is [`Task::live_labels`].
pub label_memberships: BTreeMap<LabelId, MembershipState>,
```

with

```rust
/// The live label set (the `Added` memberships' ids) — the read shape
/// every former `task.labels` reader keeps.
#[must_use]
pub fn live_labels(&self) -> std::collections::BTreeSet<LabelId> {
    self.label_memberships.iter()
        .filter(|(_, s)| matches!(s, MembershipState::Added { .. }))
        .map(|(id, _)| *id)
        .collect()
}
```

**A1 discipline (recorded decision D-1).** The type is a hand-rolled
exhaustive enum, not a `(state: bool, clock)` pair and not a
validate-and-forget record: every representable value is well-formed
(any `DateTime<Utc>` is a legal stamp), so there is no invariant for a
fallible minting constructor to guard — the trust boundary is the
storage codec, whose unknown-state decode maps to
`RepositoryError::Corrupted` (L-10). Deliberate leniency, documented
here per AGENTS §2.1: `at` is unrestricted because stamps are
cross-machine wall-clock data by design (ADR 0004); no clocks are
read inside the type. `Copy` is kept (the map is small, merge code
stays value-shaped). The serde encoding is the derived enum form —
no on-disk JSON of `Task` exists (the sqlite codec is column-wise;
`PersistedState` serde is in-memory only), so no
`#[serde(default)]` armor is added: record **D-9**.

`TaskClocks.labels` retires (record **D-2** — the design's "the live
set and the clocks live in one map" leaves the whole-set clock
nothing to mean, and a dead clock is a mutant farm). Consequences,
each handled in its seam's decision below: the `TASK_FIELDS` row
drops (L-11), the lockstep intent-extras set gains `labels` (L-11),
storage keeps writing the vestigial `ck_labels` column (L-10), and
chunk DR-4/D's `has_unsynced_work` must read the membership map's
max `at` instead of `clocks.labels` (record **D-11**, a handoff note
for that chunk's stage-1, not code here).

### L-2 — the outcome table as one exhaustive pure function

In `merge.rs`, the D-2d table binding verbatim, as code:

```rust
/// The per-membership OR-set outcome (ADR 0004's labels amendment):
/// the local membership state `l` against the observed remote event
/// `r`. Add-wins-on-tie: a remove beats an add only when strictly
/// newer; an add at an equal stamp survives in both orders.
///
/// | local            | remote event | outcome                              |
/// |------------------|--------------|--------------------------------------|
/// | —                | `Added{t}`   | added                                |
/// | `Added{a}`       | `Added{t}`   | added; clock `max(a,t)`              |
/// | `Added{a}`       | `Removed{t}` | removed iff `t > a`, else the add stands |
/// | `Removed{r}`     | `Added{t}`   | added iff `t >= r`, else stays removed |
/// | `Removed{r}`     | `Removed{t}` | removed; clock `max(r,t)`            |
#[must_use]
pub fn merge_membership(l: MembershipState, r: MembershipState) -> MembershipState {
    use MembershipState::*;
    match (l, r) {
        (Added { at: a }, Added { at: t }) => Added { at: max_ts(a, t) },
        (Added { at: a }, Removed { at: t }) if t > a => Removed { at: t },
        (Added { at: a }, Removed { at: t }) if t <= a => Added { at: a },
        (Removed { at: r }, Added { at: t }) if t >= r => Added { at: t },
        (Removed { at: r }, Added { at: t }) if t < r => Removed { at: r },
        (Removed { at: r }, Removed { at: t }) => Removed { at: max_ts(r, t) },
    }
}

/// The map-entry form: an event on a membership the map does not know
/// is the table's first row (the "—" row). All callers merge through
/// this so the "—" semantics live in exactly one place.
#[must_use]
pub fn merge_membership_opt(
    l: Option<MembershipState>,
    r: MembershipState,
) -> MembershipState {
    match l { None => r, Some(l) => merge_membership(l, r) }
}
```

(Guards written as exhaustive `if` arms rather than `if let` so the
mutant for each strictness flip is one comparison, killed by the tie
tests; `max_ts` is merge.rs's existing private helper.) The function
is **commutative** — the tie bias is consistent in both orders (row 3
with `(Added{a}, Removed{t})` and row 4 with `(Removed{t},
Added{a})` both reduce to "removed iff t > a") — which the proptest
pins and which L-8's pair merge relies on. Strictness exactly as
written: `>` for remove-vs-add, `>=` for add-vs-removal (the design's
words "removed iff `t > a`" / "added iff `t >= r`", dispatch L.3's
blocking-review focus). The strictness mutants (`>` ↔ `>=` in either
arm) are killed by the two tie unit tests plus the remove-needs-newer
proptest.

### L-3 — local verb stamping (D-2b)

In `command.rs`, the two planners keep their shape and their outbox
ops; only the state effect and the guards change (record **D-6**):

- `AssignLabel`: guards `live_task`/`live_label` as today. The
  membership lookup decides: `Some(Added{..})` ⇒ `no_op()` (the old
  contains-guard); `Some(Removed{..})` **or `None`** ⇒ stamp
  `Added { at: now }` (overwriting the tombstone — "a re-add after a
  remove overwrites with the newer `Added`"), `UpsertTask`, enqueue
  `LocalOp::AssignLabel`. No `TaskClocks` field is stamped (the
  whole-set clock is gone; the membership's `at` **is** the clock).
- `UnassignLabel`: `None` or `Some(Removed{..})` ⇒ `no_op()` (the
  old absent-guard, now tombstone-aware — an explicit re-remove of
  an already-removed membership is a no-op, not a clock bump);
  `Some(Added{..})` ⇒ stamp `Removed { at: now }`, `UpsertTask`,
  enqueue `LocalOp::UnassignLabel`.

Test set (planner tier, typed outcomes): the four guard cells
(Added→assign no-ops; Removed→assign stamps and enqueues;
Removed→unassign no-ops; Added→unassign stamps and enqueues), the
absent-membership cells (both stamp), re-add-after-remove carries
the new `at`, and the tombstoned-label / tombstoned-task rejections
unchanged.

### L-4 — remote events from the baseline diff (D-2c)

In `merge_task`, the R7 branch is replaced. The gate stays
`changed.labels` (G's `BaselineDiff` unchanged — an unchanged remote
set emits **no events**, the Deck echo-invisibility contract, pinned
structurally: `!changed.labels` ⇒ the map is byte-identical after
merge). When the baseline exists and `changed.labels`:

1. Derive per-id events in remote vocabulary:
   `added_ids = remote.labels − baseline.labels`,
   `removed_ids = baseline.labels − remote.labels`.
2. `added_ids`: resolve each through `ctx`; mapped ⇒ apply
   `Added { at: ts_r }` via `merge_membership_opt`; unmapped ⇒ skip
   (record **D-7**: the pipeline adopts label entities before tasks,
   so unmapped means the label is absent remotely — L-7's dominance
   in its quiet form; identical rule to R1's drop today).
3. `removed_ids`: resolve each through `ctx`; mapped ⇒ apply
   `Removed { at: ts_r }` via `merge_membership_opt`; unmapped ⇒
   skip (the binding changed since the baseline — nothing to stamp).
4. Verdict logging: typed facts only (`labels_remote_adds`,
   `labels_remote_removes` counts + the gate bool), per the module's
   logging contract; the `labels_remote_wins` fact retires with the
   branch.

**The missing-baseline fallback (record D-5).** When
`local.remote_baseline` is `None` (pre-0011 rows, a freshly bound
reconcile survivor — `merge_task_pair` sets `None`), today's R7
wholesale-replaces the set. The fallback reproduces that bit-for-bit
at the live-set level while honoring the never-forget rule: every
mapped remote id becomes `Added { at: ts_r }` through
`merge_membership_opt`; every local `Added` membership whose id is
**not** in the mapped remote set becomes `Removed { at:
max(local.at, ts_r) }`; existing `Removed` entries stay. The live
set after the cycle equals today's replacement set exactly — pinned
by the fallback test (§5 C3) — and the cycle establishes the
baseline (G), so convergence is one pull.

R1 adoption (`adopt_remote_task`): the map is built as
`mapped_ids → Added { at: remote.last_modified }` (a fresh task has
no history; `clocks_from` loses its `labels` slot).

### L-5 — the echo translation (R9a/R9b, record D-4)

`adopt_task_after_push` rebuilds the task from the server's
normalized echo (values as-is, clocks `max(local, lm)`). The labels
translation preserves that contract without ever forgetting a
removal:

- For each id in `map_remote_labels(ctx, echo)`: entry :=
  `Added { at: max(local_entry.at (if any), lm) }`.
- For each local entry (either variant) whose id is **not** in the
  mapped echo set: entry := `Removed { at: max(local_entry.at, lm) }`.

Rationale recorded: the echo is *our own* PUT's response; a local
`Added` id missing from it is the server confirming a removal
(normalization or our own unassign riding the same push) — stamping
`Removed` (not dropping) is the no-flap requirement; an
echo-present id restamps `Added` with the advanced clock. A
cache-lagged echo (a stale set, `lm` older than a local edit) still
converges: the baseline refreshes to the echo, so the next genuine
pull diff emits the correcting event with a fresh `ts_r`, which
beats the stale stamp through the table — the one-cycle transient
is today's behavior too, and the round-trip proptest (§5) pins the
convergence. The echo's `labels: max_ts(local.clocks.labels, lm)`
clock line retires with the whole-set clock.

### L-6 — the self-clobber guard (record D-3)

`PendingTaskIntent.labels: bool` **stays a coarse bool** (fact 7:
the guards survive unchanged; the struct keeps `Copy`; the flag's
race window — ops queued after `read_at` — is unchanged). Its
`keep_task_intent` arm becomes the whole-map restore:

```rust
if p.labels {
    merged.label_memberships.clone_from(&local.label_memberships);
}
```

This is exactly today's coarseness (any queued label op reverted the
whole set + clock) translated to the new substrate: no per-membership
surgical guard is introduced — that would drop `Copy`, touch every
`.copied()` call site, and change a contract this chunk must not
change. The lockstep assertion's intent-extras set becomes
`{reminder_rules, labels}` with the comment that both are deliberate
non-clock members (L-11).

### L-7 — entity-death dominance (D-2e)

Two doors, one rule — a dead label's memberships are force-removed
and its queued assignments die with the entity:

1. **R3 remote absence (the design's normative door).** In the
   labels absence block (`pipeline.rs:788-800`), after
   `tombstone_label`: every task with an `Added` membership of that
   label id gets `merge_membership_opt(Added{..}, Removed{at: now})`
   (only `Added` entries change state; `Removed` ones keep their
   older clock — max keeps the older removal, the stamps are both
   `Removed`), each touched task emits its `UpsertTask` in the same
   batch, and every **unprocessed queued `AssignLabel`/`UnassignLabel`
   op for that label id is failed** (`op_results.insert(op_id,
   false)`) — that is the design's "a concurrent local assignment
   dies with the entity (the push would 404)". New local assignments
   remain impossible: the planners' `live_label` guard already
   rejects tombstoned labels.
2. **Local `DeleteLabel` (record D-8, a recorded extension of
   D-2e's rationale).** Today the planner leaves task sets untouched
   ("the tombstone hides the label; R7 reconciles remotely",
   `command.rs:933-934`). Under the OR-set that note is false economy:
   a live `Added` membership on a locally-deleted label survives in
   `live_labels()`, pushes a dead id in the card PUT, and renders a
   ghost in the CLI. The planner therefore force-removes the label's
   memberships across live tasks (same `Removed{now}` stamping,
   `UpsertTask` actions in the same `CommandPlan` batch) and — for an
   unbound label — keeps today's `FailOp` sweep, which now also
   cancels that label's queued assign ops. Bound labels keep their
   queued `DeleteLabel` push (unchanged). The stale comment is
   rewritten to the dominance rule.

Board-content clears (`persistence.rs` clear-board ceremony) delete
whole task rows, so their memberships cascade — no separate rule.
Label-entity *tombstones* never hard-delete rows (the cascade fires
only on real row deletion, compaction's business, unchanged per the
design's storage note).

Tests: the R3 dominance scene (bound label vanishes from the listing
⇒ every task's `live_labels()` loses it; queued assign op fails;
`Removed` stamped at `now`), the local-delete scene, and the
entity-death proptest (§5). The unit-tier home is `pipeline.rs`'s
sync-report tests; the planner scene is a `command.rs` test — one
behavior, one test, lowest layer.

### L-8 — the bind ceremony (reconcile)

- **The edge rewrite** (`reconcile.rs:171-189`): instead of
  `labels.remove(bound)/insert(local)`, the task's membership entry
  for `bound_id` re-keys to `local_id`. Collision rule (record
  **D-10**): if the task already has an entry under `local_id`, the
  two entries merge through `merge_membership` (they are two
  observations of the same conceptual membership; the table is
  commutative and the add-wins tie bias is the ceremony's existing
  "local row survives" spirit at the membership level). A task is
  rewritten (and emits `UpsertTask`) iff its map changed.
- **`merge_task_pair`**: the whole-set pick and the `clocks.labels`
  max retire; the two tasks' maps merge per key through
  `merge_membership` (clocks are per-entry maxes by the same-verb
  rows; contested add-vs-remove resolves by the table — strictly
  better than today's whole-set coin flip, and it keeps the
  ceremony's determinism).

### L-9 — the push side (wire shape unchanged)

`NewCardShape::from_task` / `CardShape::from_task` clone
`task.live_labels()` instead of `task.labels` — the `BTreeSet<LabelId>`
field type, the JSON the executor sends, and the overlay-deferral
behavior are byte-identical (dispatch L.3: "the card PUT's label set
is unchanged on the wire"). `plan_label_groups` is id-only,
unchanged. The bench mirror in `benches/support/mod.rs` follows the
same one-line change. Membership clocks never reach the wire.

### L-10 — storage: migration 0012, codec, upgrade test

**Migration `0012_task_label_memberships.sql`** (number per fact 8;
record **D-12**: deviation from dispatch §2's `0011` assignment —
0010/0011 are taken; the ladder's landing-order rule governs; the
issue body's number is stage-3's bookkeeping, not touched here):

```sql
-- 0012_task_label_memberships: per-membership OR-set state for task
-- labels (DR-4/L, #129) — Task.label_memberships' storage.
-- Conventions: state is 'added' | 'removed' (unknown text decodes to
-- Corrupted); ck_membership is the membership's write clock,
-- epoch-defaulted NOT NULL (0010's mechanism) so pre-migration rows
-- decode as fresh adds — the pre-migration live set bit-for-bit
-- (ADR 0005). Removed rows persist (never garbage-collected while
-- either entity lives); the (task, label) PK and both ON DELETE
-- CASCADEs stay (hard row deletion still sweeps).
ALTER TABLE task_labels ADD COLUMN state         TEXT NOT NULL DEFAULT 'added';
ALTER TABLE task_labels ADD COLUMN ck_membership TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
```

**Codec (`repo.rs`):**
- Load: `SELECT task, label, state, ck_membership FROM task_labels
  ORDER BY task, label` → `BTreeMap<Uuid, BTreeMap<Uuid,
  MembershipState>>` per task; `decode_task` builds
  `label_memberships` from it (state text `'added'`/`'removed'` →
  variant, anything else ⇒ `Corrupted`; `ck_membership` parses as
  RFC3339, failure ⇒ `Corrupted`).
- Upsert: the delete-then-rewrite stays and now writes **every**
  entry including `Removed` rows (`state`, `ck_membership =
  at.to_rfc3339()`). T5 extends: a `Removed` row survives a later
  upsert of the same task (the storage half of no-flap).
- `tasks.ck_labels` vestige (record **D-13**): the column is NOT
  NULL without a DEFAULT, so the INSERT cannot omit it; the codec
  keeps writing it as `max(membership.at)` (epoch when the map is
  empty) and **stops reading it** — dropped from `TaskRow`, the
  load query, and `decode_task`. The column's historical values stay
  in place (additive-only; no drops, ADR 0005); the comment at the
  write site names it vestigial.

**Tests:** `schema.rs:74-82` grows the pinned `task_labels` column
set to `{task, label, state, ck_membership}`; the roundtrip props
gain membership-map strategies; the S-series upgrade pin (the
S3 pattern, `roundtrip.rs:963-1005`): raw pre-0012-shaped INSERT of a
task + two `task_labels` rows, load through the migrated schema ⇒
both decode `Added { at: epoch }`, the live set is identical to the
raw rows, and a follow-up `merge_task` produces the same live labels
as a hand-built fresh-add task (the "bit-for-bit, upgrade-test-pinned"
clause).

### L-11 — the manifest and lockstep surfaces

- `TASK_FIELDS` (`entities.rs:265`): the `("labels", &["labels"],
  Native)` row drops. Labels remain a mergeable field of the *task*,
  but no longer a clock-named field — the manifest's job (clock ⇄
  field lockstep, #244) has no labels clock to pin. The state crate's
  routing coverage is unaffected: labels never routed through
  `TaskChanges` (fact 11); `manifest_routing.rs`'s partition-sanity
  list (`:355-365`) drops its `("labels", vec!["labels"])` row.
- The lockstep assertions (`pipeline.rs:1484-1595`): (a) runs
  against the 12-slot `TaskClocks`; (b)'s deliberate-extras set
  becomes `{reminder_rules, labels}` — `labels` is now the second
  deliberate non-clock guard flag, with a comment saying so (the
  guard protects map entries, not a clock); (c) passes (the task
  key set simply carries `label_memberships`).
- `test_support/entities.rs` (the `task_strategy`/clocks strategy)
  and `merge_testutil.rs` grow membership-map construction; the
  strategy generates `Removed`-heavy maps too (the no-flap and
  entity-death properties need tombstone-rich tasks).

### L-12 — docs: ADR 0004 amendment + code docs

**ADR 0004, dated amendment appended** (amendment-not-rewrite; the
"*Labels* merge as one field (whole-set LWW). Add-wins OR-set
merging is backlogged." bullet gains a pointer to the amendment).
Wording to land:

> **Amendment 2026-10-28 — labels merge as an OR-set (#129).**
> Label assignments are per-membership state: =Task.label_memberships=
> maps each label id to =Added{at}= / =Removed{at}=, replacing the
> whole-set field and its clock. Local verbs stamp the membership at
> =now=. At merge, the incoming card's label set diffs against the
> persisted baseline's: an observed remote add stamps =Added{ts_r}=,
> an observed removal stamps =Removed{ts_r}= — an unchanged remote
> set emits no events, so Deck cache-lag echoes of our own writes
> are invisible by construction. Per membership, local state vs
> remote event resolves by the outcome table with an
> add-wins-on-tie bias: a remove beats an add only when strictly
> newer; adds at equal stamps survive in both orders; same-verb
> merges keep the max clock. Removed memberships are never
> forgotten — a stale remote re-add older than the removal cannot
> resurrect the label (the no-flap requirement). Entity death
> dominates: a label absent from the board listing force-removes its
> memberships and kills concurrent assignments (the assignment
> cannot exist server-side; the push would 404). The whole-set-LWW
> limitation above is retired for labels; its residue moves to the
> skew class: a true concurrent add-vs-remove pair is still decided
> by cross-machine stamp order (the same documented class as
> #133). Enforcement: the outcome-table unit pins and the
> commutativity / concurrent-add / remove-needs-newer / no-flap /
> entity-death proptests in =crates/taskboard-domain/src/merge.rs=,
> =command.rs=, and =pipeline.rs=; the push wire shape is unchanged
> (=live_labels= mapped through =RemoteIndex=).

Code docs: `merge.rs`'s module doc + R7's comment are rewritten to
the OR-set rule (the rule name stays R7 — the labels merge path);
`command.rs:933-934`'s stale note becomes the dominance rule (L-7);
`Task.labels`' doc comment moves to `label_memberships`.

## 4. The reader sweep — the explicit checklist

Every `task.labels` / `clocks.labels` site in the workspace; the
sweep is its own mechanical commit (dispatch L.2). ✓ = the fix.

**Domain — semantic seams (land in their design commits, not the
sweep commit):**
1. ✓ `entities.rs:265` — `TASK_FIELDS` row drops (L-11).
2. ✓ `entities.rs:294` — `TaskClocks.labels` retires (D-2).
3. ✓ `entities.rs:482-483` — `Task.labels` → `label_memberships` +
   `live_labels()` (L-1).
4. ✓ `merge.rs:153/167-184/296-302/691/706` — R1, `clocks_from`,
   R7, echo (L-4, L-5).
5. ✓ `command.rs:951-985` — the two planners (L-3);
   `:933-934` the delete-label note (L-7.2).
6. ✓ `pipeline.rs:102/236-239` — guard arm (L-6); `:1484-1595`
   lockstep (L-11).
7. ✓ `push.rs:171-173/194-195/208/223` — shapes via `live_labels()`
   (L-9).
8. ✓ `reconcile.rs:171-189/353-443` — edge re-key + pair merge (L-8).

**Domain — construction/test literals:**
9. ✓ `test_support/entities.rs:36/131/150` — strategies (L-11).
10. ✓ `merge_testutil.rs:71/85/124/215` — builders.
11. ✓ `state.rs:865/880` — test literals.
12. ✓ `persistence.rs:560-575/866-881` — test literals.
13. ✓ `merge.rs` / `pipeline.rs` / `command.rs` / `reconcile.rs` /
    `push.rs` test modules — every `labels:` / `clocks.labels:`
    literal (mechanical; the R7-specific pins rewrite to the new
    pins per §5).

**Storage:**
14. ✓ `repo.rs:101/314-331/384-387/605-627/1088-1145/1165-1181` —
    codec (L-10).
15. ✓ `tests/schema.rs:74-82`, `tests/roundtrip.rs` (T5 `:541-575`,
    S-series `:963-1005`, prop strategies, `contract.rs` if it
    pins the row shape).

**App (CLI/web):**
16. ✓ `commands/labels.rs:76` — `t.labels.contains(&l.id)` ⇒
    `t.live_labels().contains(&l.id)`.
17. ✓ `commands/output.rs:810` — `task.labels.iter()` ⇒
    `task.live_labels().iter()`.
18. ✓ `web/render.rs:473/507` — Task literals (no render change).

**UI-slint (no live reader exists — literals only):**
19. ✓ `test_platform.rs:277/292`, `flow_table_tests.rs:498/512`,
    `lib.rs:1269`, `view_state.rs:1780/1802`,
    `presentation.rs:735/757/2016`, `models.rs:1471/1700`.

**Sync-nextcloud:**
20. ✓ `benches/support/mod.rs:250/266` — the shape mirrors (L-9).
    (`mapping.rs`/`model.rs` remote-side `labels` are out of scope.)

**State crate:**
21. ✓ `tests/manifest_routing.rs:96/111` literals + `:355-365`
    partition row (L-11); the insta suites
    (`tests/snapshots/snapshots__s*.snap`) regenerate in the one
    deliberate (w) pass.

**Post-sweep verification:** `grep -rn '\.labels\b' crates/ --include='*.rs'`
shows only `AppState.labels`/`RemoteTask.labels`/`baseline.labels`/
DTO sites; `grep -rn 'clocks\.labels'` returns nothing;
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo nextest run --workspace` green (the retired field makes any
surviving reader a compile error — the sweep's own proof).

## 5. Test plan — TDD-first, per commit

House rules in force: typed outcomes only, no text assertions, no
sleeps, properties on the pure algebra, `currently_pins_` naming for
tolerated debt (none is new here — the skew residue is ADR-documented,
not a pinned test), one behavior one test lowest layer.

**C1 `feat(domain): the membership state and the OR-set outcome table`**
- `MembershipState` + `Task.label_memberships` + `live_labels()`;
  `merge_membership`/`merge_membership_opt` (L-1, L-2).
- Unit: the five table rows **including both ties** (`t == a` keeps
  the add; `t == r` lands the add) — these two tests are the
  strictness pins the review gates on; `live_labels` filters
  tombstones; `at()` exposes either stamp.
- Proptest: **commutativity** (`merge_membership(l, r) ==
  merge_membership(r, l)` for arbitrary variants/stamps) and
  **idempotence** (`merge_membership(s, s) == s`; kills max/stamp
  mutants).
- This commit retires the field/clock/manifest rows and updates the
  construction literals it invalidates (the compiler drives the
  checklist) — it is standalone: green gates at its own scope.

**C2 `feat(domain): the label verbs stamp membership state`**
- The planner arms (L-3) with the four guard cells + the two absent
  cells + re-add-after-remove `at` + outbox op assertions (typed
  `matches!` on `LocalOp`).

**C3 `feat(domain): the merge derives label events from the baseline diff`**
- The R7 rewrite (L-4): pins for the acceptance list —
  *concurrent two-device adds both survive* (local `Added{a}` +
  remote `Added{t}` ⇒ still added, clock max), *remove wins only
  strictly newer* (remote `Removed{t}` vs local `Added{a}`, `t > a`
  removes / `t <= a` keeps), *unchanged remote set emits no events*
  (`!changed.labels` ⇒ map byte-identical — the Deck echo pin), the
  fallback wholesale pin (baseline-less task: live set equals
  today's replacement, displaced members tombstoned), unmapped-add
  skip, `remote_seen`/baseline still advance on a labels-only cycle.
- Proptest battery additions over generated op/event sequences on
  one membership: **remove-needs-newer** (removal lands ⟺ `t > a`)
  and **no-flap** (after any `Removed{r}`, every subsequent event
  stream with stamps `< r` leaves the label out of `live_labels()`),
  plus map key-set monotonicity across arbitrary event folds.

**C4 `feat(domain): the push echo preserves removals`**
- The echo translation (L-5): echo-present restamps with max clock;
  local-Added-missing ⇒ `Removed{max(at, lm)}` (the never-forget
  pin); local-Removed-missing keeps its tombstone with advanced
  clock; the push→echo→pull round trip (assign → echo → unchanged
  pull ⇒ no spurious events; unassign → cache-lagged echo → next
  pull converges to removed) — the design §5 drift-mitigation
  property, labels edition.
- R1 adoption pin: adopted tasks carry `Added{ts_r}` maps.

**C5 `feat(domain): label entity death dominates its memberships`**
- R3 absence scene (force-remove + queued-op failure + `UpsertTask`
  batch) and the local `DeleteLabel` sweep (L-7); the entity-death
  proptest: for any task state, after the dominance pass no live
  task's `live_labels()` contains the dead label and no queued op
  for it survives unfailed.

**C6 `feat(domain): the bind ceremony merges memberships`**
- Edge re-key incl. the collision merge + pair-merge via the table
  (L-8); pins: rewritten-only-if-changed, survivor id owns the edge,
  tombstones re-key with their stamps.

**C7 `feat(storage): migration 0012 task label memberships`**
- Migration + codec + schema/T5/S-series/roundtrip work (L-10); the
  upgrade pin asserts the epoch backfill and merge-equivalence.

**C8 `refactor: the label reader sweep`**
- The §4 checklist's remaining sites (app, ui-slint, benches, state
  literals), the `manifest_routing.rs` partition row, and the
  post-sweep greps as commit-message evidence.

**C9 `test(state): the shape-widener regeneration`**
- The one deliberate insta pass over the shared dump+tail suites
  (diff-audited: only Task-shape lines move).

**C10 `docs(adr): the labels OR-set amendment`**
- The L-12 wording + the code-doc rewrites. Stage 3 ships the PR;
  #129/#482 close with it (stage-3's act).

Gates at handoff: the full AGENTS §10 suite through
`scripts/run-quality-gates.sh`, green.

## 6. Decision record

| # | Question | Decision |
|---|---|---|
| D-1 | Is `MembershipState` an A1 minted type? | Plain exhaustive enum, `Copy`, `at()` accessor; no fallible mint — every value is well-formed, the codec is the only trust boundary (unknown state ⇒ `Corrupted`). Leniency documented at the type. |
| D-2 | Does `TaskClocks.labels` retire? | Yes — "the clocks live in one map" leaves it meaningless; a dead clock is a mutant farm. Consequences handled at L-11/L-10/D-11. |
| D-3 | Guard shape after retirement | `PendingTaskIntent.labels` stays a coarse bool restoring the whole map (fact 7: guards survive unchanged; `Copy` kept; today's race-window coarseness preserved verbatim). |
| D-4 | Echo-path label translation | Rebuild from the mapped echo set; every local entry missing from it stamps `Removed{max(at, lm)}` — removals survive the echo; cache-lag echoes converge via the next pull's fresh `ts_r` (pinned). |
| D-5 | Missing-baseline fallback | Wholesale adoption in local vocabulary: mapped remote ids ⇒ `Added{ts_r}`; local `Added` ids absent from it ⇒ `Removed{max(at, ts_r)}`; existing tombstones stay; live set bit-identical to today's replacement (pinned). |
| D-6 | Verb guards | `Assign` no-ops on `Added` only (a `Removed` tombstone re-adds); `Unassign` no-ops on `Removed`/absent; stamps ride the membership, no `TaskClocks` write. |
| D-7 | Unmapped remote ids in the diff | Adds skip (dominance in its quiet form — the label isn't adopted, matching R1's drop); removes skip (binding changed since the baseline). |
| D-8 | Does local `DeleteLabel` force-remove? | Yes — D-2e's rationale (the push would 404; no zombie labels) applies to the local door too; a recorded extension of the design's remote-absence door, with the stale "sets stay untouched" note rewritten. |
| D-9 | Serde armor on the new field? | None — no on-disk `Task` JSON exists (sqlite codec is the boundary); matches every existing `Task` field's plain derives. |
| D-10 | Bind-ceremony edge collision | The re-keyed entry and an existing survivor-id entry merge through `merge_membership` (two observations of one conceptual membership; the table is commutative). |
| D-11 | DR-4/D handoff | `has_unsynced_work`'s "max native field clocks" must read the membership map's max `at` after this chunk — recorded for chunk DR-4/D's stage-1, not code here. |
| D-12 | Migration number | **0012**, not the dispatch ladder's 0011: 0010 is DR-6/1's recur family (`1fa9c14`), 0011 is G's baseline (`bb6149e`); the ladder's landing-order rule governs; deviation recorded here for stage 3. |
| D-13 | The vestigial `tasks.ck_labels` column | NOT NULL without DEFAULT ⇒ the INSERT keeps writing it: `max(membership.at)`, epoch when empty; never read again (dropped from `TaskRow`/query/decode); no column drops (ADR 0005). |

## 7. Risks

- **The reader sweep breadth** (design §5's "widest mechanical churn
  in the family"): mitigated by the accessor's identical read shape,
  the explicit §4 checklist, and its own commit; the retired field
  turns any missed reader into a compile error, not a silent drift.
- **The lockstep/manifest pins** (`pipeline.rs:1484-1595`,
  `manifest_routing.rs`): precise updates enumerated in L-11; both
  assertions get sharper, not weaker (the extras set grows with a
  documented reason).
- **The echo translation is the subtlest new logic**: cache-lag
  convergence is pinned by the C4 round-trip, and the never-forget
  rule has a dedicated unit pin; the residual risk is a mis-stamped
  echo transient (one cycle, self-healing — today's behavior too).
- **Fallback semantics for reconcile survivors** (baseline `None`
  with a merged membership history): the C3 fallback pin covers the
  live-set equivalence; the displaced-member tombstones are new
  state today's path didn't have — benign (they only *prevent*
  resurrection), and the baseline establishes on that cycle.
- **Shape-widener lane**: wave 4's only (w) chunk (DR-2/D touches no
  domain shape); the snap regeneration is conflict-free by the wave
  protocol.
- **Number drift**: D-12's 0012 must survive any upstream merge
  before stage 3's rebase — re-verify "next free" at the rebase
  (dispatch §2's standing rule) and rename the file if 0012 got
  taken.
- **DR-4/D dependency** (D-11): the resurrect predicate's spec
  reads `clocks.labels` implicitly; if DR-4/D dispatched before it
  re-reads this plan, its predicate would silently miss label
  edits. The note is recorded here; the dispatch serializes the
  chunks, and D's stage-1 reads the family plans.

## 8. Estimated diff size

~1.5–2.2k lines including tests and snap churn, consistent with the
dispatched L: domain ~1.0–1.3k (roughly half tests), storage
~250–350, reader sweep ~150–250 mechanical across app/ui-slint/
benches/state literals, insta churn a few hundred regenerated lines,
ADR +~45. Ten commits, each standalone (§5).

## 9. References

- Design: `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
  §3 D-2 (normative), §2 facts 2/8, §4 F-129, §5 reader-sweep risk,
  §6 chunk L.
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (lanes/wave protocol/stage model), §2 (ladder + landing-order
  rule), §7.1 (this chunk's prompts and acceptance).
- Predecessor: the G plan
  `.artifacts/plans/2026-10-27-sync-remote-baseline-plan.md` (the
  baseline record, the diff gate, the upgrade-test pattern).
- ADR 0004 (`docs/adr/0004-sync-conflict-policy.org`) — the labels
  bullet and the amendment trail this chunk appends to.
- AGENTS §2 (A1 policy), §5 (test directives), §8 (branch/PR), §10
  (gates).

---

## 10. Stage-2 sweep verification (2026-10-28)

The §4 checklist's mechanical sites landed compile-driven across the
implementation commits — retiring `Task.labels` at C1 turned every
surviving reader into a compile error, so the sweep's fixes ride the
commits that invalidated them (recorded deviation from "the sweep is its
own commit"; this section is the verification commit). Post-sweep state:

- `grep -rn 'clocks\.labels' crates/ --include='*.rs'` → **zero hits**.
- Every remaining `.labels` site is `AppState.labels`, a receipt/counter
  field, `RemoteTask.labels`/`baseline.labels` (remote vocabulary), or a
  DTO — the §4 allowed set.
- CLI readers converted: `commands/labels.rs:76` and
  `commands/output.rs:810` read `live_labels()`; `web/render.rs` builds
  from it; no render change.
- The lockstep extras set grew to `{reminder_rules, labels}` and the
  state crate's partition pin asserts `labels` has left the clock
  manifest while still never riding `UpdateTask`.
