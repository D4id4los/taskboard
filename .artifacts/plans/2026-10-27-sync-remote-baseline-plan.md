# [Plan] Sync Remote Baseline & Changed-Field Merge (DR-4/G)

- **Date**: 2026-10-27
- **Kind**: stage-1 implementation plan for chunk DR-4/G of the
  design-rounds batch dispatch — the #128 granularity fix. TDD-first;
  this file is written before any code and is the chunk's contract.
- **Origin** (all normative):
  - `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
    §3 D-1 a–f (the design), §2 facts 1–4/9/16, §4 F-128 (tasks-only
    scope, confirmed), §5 (the baseline-drift risk).
  - `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
    §1 (wave 3, the (w) shape-widener of its wave), §2 (migration 0010
    assigned here), §3.1 (DR-4 binding notes: purity, additive storage,
    A1, test discipline, strictly serial), §6.1 (chunk scope +
    acceptance).
  - ADR 0004 (`docs/adr/0004-sync-conflict-policy.org`) — amended, never
    rewritten; ADR 0005 (additive-migration policy).
- **Branch / worktree**: `feat/sync-remote-baseline` in
  `../taskboard-WT-feat-sync-remote-baseline`, branched off
  `origin/main` @ `2a801c1`. Preconditions verified: DR-4/S merged
  (`4a3b30c test(sync): the clock-skew interleaving pins (DR-4/S)` is
  an ancestor of `origin/main`); `scripts/worktree-doctor.sh` exit 0
  (245G free, floor 25G); migrations 0001–0009 shipped ⇒ **0010 is
  next free** — matches the dispatch ladder, no deviation.
- **Lane check**: wave-mate DR-3/3 (`feat/timers-alarm-surface`, worktree
  present) is a ui-slint chunk — no touch of `merge.rs`/`pipeline.rs`/
  the storage codec, and not a shape-widener. Both hard lanes
  (merge/pipeline serial, one (w) per wave) are satisfied.
- **Status**: READY FOR STAGE-2. Every open question this round raised
  is resolved in §5 (decision record). No implementation has happened.

---

## 1. Mission & binding constraints

Persist, per bound task, the **remote baseline** — the field values of
the remote card snapshot as last observed — and gate every field
adoption in `merge_task` on a diff against it, so a remote entity-touch
that changed only field set S can no longer overwrite local edits to
fields outside S (issue #128, closing the granularity trap for tasks).
Everything else in ADR 0004 stays exactly as it is.

Non-negotiable (design §1 + dispatch §3.1):

- **Purity**: the baseline is injected data on the entity (a plain
  serde record, no invariants — explicitly *not* an A1 type, per
  D-1a). No clocks read, no ids generated, no IO anywhere in the
  algebra. `RemoteTaskBaseline::of(&RemoteTask)` is a pure copy.
- **Additive storage** (ADR 0005): migration 0010 adds one NULL column;
  pre-migration rows decode to `remote_baseline: None` and behave
  bit-for-bit as today (the fallback, D-1b), pinned by an upgrade test.
- **Tasks only** (F-128): no baseline on boards/stacks/labels; no
  conflict surfacing (D-1f). The record's shape must not preclude the
  mechanical extension later, but nothing is built for it.
- **Test discipline** (AGENTS §5): typed outcomes, no text assertions,
  `currently_pins_` naming for the D-1d blind spot, proptests for the
  pure algebra, no quality ceilings. The existing test stock —
  including the DR-4/S skew pins and the blanket-LWW proptests — must
  pass **untouched** (their fixtures are baseline-less ⇒ the fallback
  path, which is today's behavior; see D-15).
- **Shape-widener discipline**: `Task` grows a field ⇒ the shared
  insta `persisted_state` suite regenerates once, deliberately, and
  the diff is audited as "the widening, nothing else" (dispatch G.2).

## 2. Verified baseline — numbered facts

All verified against this chunk's base `origin/main` @ `2a801c1`
(line numbers from that tree; the DR-4 design plan's facts 1–4, 9, 16
re-confirmed and made precise):

1. `Task` carries `remote_seen: Option<DateTime<Utc>>` — a timestamp
   only; there is no persisted record of last-observed remote *values*
   (`entities.rs:439`). `TaskClocks` has the twelve per-field clocks
   (`entities.rs:278-303`); the `TASK_FIELDS` manifest
   (`entities.rs:258-271`) lists eight `FieldFamily::Native` user
   fields plus `deleted` — the baseline covers the native set minus
   `deleted`, as bookkeeping *outside* the manifest (like
   `remote_seen`, which has no manifest row either).
2. `merge_task` (`merge.rs:178-285`) applies each native field
   independently iff `remote_wins(ts_r, clocks.f)` (strict, ties keep
   local); position is one composite (`merge.rs:225-235`, R8); labels
   are one whole-set field (`merge.rs:238-242`, R7); `scheduled` is
   additionally gated on `native_scheduled` (D-V2,
   `merge.rs:214-218`); R5 resurrect compares `ts_r > clocks.deleted`
   with no other input (`merge.rs:250-254`). It sets
   `remote_seen := ts_r` unconditionally at the end
   (`merge.rs:282-284`).
3. The granularity trap is pinned at the pure-merge tier by
   `granularity_trap_remote_entity_touch_overwrites_local_title`
   (`merge.rs:1317-1330`) — the test this chunk flips.
4. The pipeline's R4 fast path (`pipeline.rs:613-616`) gates the merge
   on `remote.last_modified > remote_seen.unwrap_or(utc_min)`;
   unchanged remotes never reach `merge_task`. After the merge,
   `keep_task_intent` (`pipeline.rs:185ff`) copies guarded values and
   clocks back — it touches no binding fields and will not touch the
   baseline.
5. The task adoption/clear surface is exactly five functions, all in
   `merge.rs`, with every call site converging on them (grep-verified,
   §3-G3): `adopt_remote_task` (`merge.rs:91`), `merge_task`
   (`merge.rs:178`), `adopt_task_after_push` (`merge.rs:573`),
   `tombstone_task` (`merge.rs:550` — called from pipeline R3 absence
   `pipeline.rs:751`, R6 cascade `:813`, R9e remote-missing `:1296`,
   board cascade `:1445`), `finalize_pushed_task_delete`
   (`merge.rs:643` — pipeline R9c `:1144`). The bind ceremony's
   `neutralized_task` (`reconcile.rs:439`) and `merge_task_pair`
   (`reconcile.rs:353`) are the only other remote-binding mutators.
6. The storage codec maps `tasks` rows through `TaskRow`
   (`repo.rs:77-107`), four load SELECTs (`repo.rs:544-589`), and one
   upsert (`repo.rs:1060-1120`). Migration conventions: value columns
   NULL, clocks epoch-defaulted (`migrations/0007`). The upgrade-test
   precedent is `insert_without_the_family_columns_inherits_epoch_defaults`
   (`tests/schema.rs:105-152`): a raw pre-migration-shape INSERT, then
   load, then assert the decoded defaults. The schema lockstep test
   (`tests/schema.rs:45-102`) asserts ck-columns ≡ manifest clocks
   (exact) and value columns ⊆ table columns — an extra bookkeeping
   column breaks neither, and the new column's presence is
   migration+SELECT-pinned (load fails loudly without it).
7. `serde_json` is workspace-pinned (`Cargo.toml:77`) and already a
   `taskboard-domain` dependency; `taskboard-storage-sqlite` does **not**
   have it yet — the chunk adds `serde_json.workspace = true` to its
   `Cargo.toml` (pinned once in the root, per AGENTS §4).
8. The sync actor needs **zero changes** (design D-6 table, confirmed:
   `push_exec/execute.rs`'s `Task {`-shaped greps are
   `MaterializedOp::CreateTask {` pattern matches, not struct
   literals). `RemoteTask` (`remote.rs:74-99`) already carries every
   field the baseline mirrors; no DTO/mapping work.
9. The echo round-trip hazard (design §5, baseline-drift risk): the R9
   echo (`adopt_task_after_push`) writes the server's *normalized*
   values; if it did not refresh the baseline, the next pull's
   normalization delta would read as a spurious remote change. Hence
   G3's inventory rule: every adoption point sets it.
10. The existing blanket-LWW proptests
    (`merge_is_total_and_strictly_newer_wins` `merge.rs:2352`,
    `merge_clocks_are_total_monotone_and_idempotent_at_extremes`
    `merge.rs:2382`) and the three DR-4/S skew pins
    (`merge.rs:2549-2653`) build their locals via
    `merge_testutil::bound_task` with no baseline — after this chunk
    they run the fallback path and must stay **byte-identical**.
    `merge_never_decreases_field_clocks` (`merge.rs:2329`) holds under
    the gate too (a non-applied field advances nothing).

## 3. The design, decision by decision

### G-1 — the record (`entities.rs`)

```rust
/// The remote card's field values as last observed (D-1a): the diff
/// reference `merge_task` gates adoption against. Remote vocabulary,
/// raw unresolved ids — resolution is per-cycle through `RemoteIndex`;
/// a stored local resolution would go stale when boards re-bind.
/// Plain data, no invariants (not an A1 type). Not covered: the
/// sidecar lane (`effort`/`importance`/`urgency` — their document fold
/// never reads this record) and `deleted` (cards are presence-based,
/// R3/R5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTaskBaseline {
    pub title: String,
    pub description: String,
    pub duedate: Option<DateTime<Utc>>,
    pub scheduled: Option<DateTime<Utc>>,
    pub done: Option<DateTime<Utc>>,
    /// The composite position's remote half (R8: position is one
    /// field; the diff compares both halves as a unit).
    pub stack: RemoteStackId,
    pub order: i64,
    pub labels: BTreeSet<RemoteLabelId>,
    pub archived: bool,
}

impl RemoteTaskBaseline {
    /// The one minting point: an observation copied verbatim from the
    /// remote view (including `scheduled` while `!native_scheduled` —
    /// the record is what the server showed, not what we kept).
    #[must_use]
    pub fn of(remote: &RemoteTask) -> Self { … }
}
```

Decisions folded in (details in §5): lives in `entities.rs` next to
`TaskClocks` (D-1); fields flat, mirroring `RemoteTask` one-for-one
(D-2); `Task` grows `remote_baseline: Option<RemoteTaskBaseline>`
after `remote_seen`; derives match `Task`'s; no `deny_unknown_fields`
(D-12); `BTreeSet` gives a deterministic, byte-stable JSON encoding.

### G-2 — the changed-field gate (pure, in `merge.rs`)

One private helper computes the whole diff once per merge — single
traversal, one `debug!` verdict event, one seam for the mutants:

```rust
/// One bool per baseline-covered field: the remote snapshot differs
/// from the last observation. `scheduled`'s consumer still applies its
/// own `native_scheduled` lane gate (D-V2); the diff is lane-blind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BaselineDiff {
    title: bool, description: bool, duedate: bool, scheduled: bool,
    done: bool, position: bool, labels: bool, archived: bool,
}

fn diff_baseline(baseline: &RemoteTaskBaseline, remote: &RemoteTask) -> BaselineDiff
```

`merge_task` then applies, per field `f` (design D-1b verbatim):

```
apply_remote(f) := remote_wins(ts_r, clocks.f) && changed.f
changed         := baseline.is_none() ? all-true : diff_baseline(baseline, remote)
```

- `position.changed := remote.stack != baseline.stack || remote.order != baseline.order`
  — applied as a unit exactly as today (R8; the local half keeps the
  unmapped-stack fallback: `resolve_local_stack` returning `None`
  still suppresses the whole composite).
- **Unapplied fields keep their local values AND local clocks** — the
  clock is the write time of the current value; an unapplied
  observation advances nothing (dispatch G.3 review focus).
- `remote_seen := ts_r` and
  `remote_baseline := Some(RemoteTaskBaseline::of(remote))`
  **always** — the merge observes the remote wholly even when it
  adopts partially, and also in fallback mode (a fallback cycle
  establishes the baseline; convergence is one cycle, guaranteed).
- **R5 is untouched**: `deleted` has no baseline row; the resurrect
  comparison stays `ts_r > clocks.deleted`. A baseline never shields a
  tombstone (pinned, §4-T9).
- The refresh happens *inside* `merge_task`, before the pipeline's
  `keep_task_intent` runs — the self-clobber guard copies values and
  clocks, never the baseline (D-5): the baseline records what the
  remote last showed, not what we agreed with.

### G-3 — the adoption-point inventory (the chunk's classic bug)

Exhaustive, grep-verified against fact 5. **Set** the baseline:

| # | Function (site) | Called from | Baseline becomes |
|---|---|---|---|
| A1 | `adopt_remote_task` (`merge.rs:91`) | pipeline R1 (`pipeline.rs:659`) | `Some(of(remote))` — fresh observation |
| A2 | `merge_task` (`merge.rs:178`) | pipeline R4 (`pipeline.rs:617`) | `Some(of(remote))` always — even when no field applied, even in fallback |
| A3 | `adopt_task_after_push` (`merge.rs:573`) | pipeline R9 echo (`pipeline.rs:1189`) | `Some(of(echo))` — the server-normalized values are the next pull's diff reference (the design-§5 risk's mitigation) |

**Clear** the baseline (`remote = None` ⇒ baseline = `None`, D-1c):

| # | Function (site) | Called from |
|---|---|---|
| C1 | `tombstone_task` (`merge.rs:550`) | pipeline R3 absence `:751`, R6 cascade `:813`, R9e remote-missing `:1296`, board cascade `:1445` — one edit covers all four |
| C2 | `finalize_pushed_task_delete` (`merge.rs:643`) | pipeline R9c `:1144` |
| C3 | `neutralized_task` (`reconcile.rs:439`) | bind ceremony duplicate (clones the bound row then clears the binding ⇒ must clear the baseline too) |

**Stay `None`** (local-side constructions; no remote observation
exists — the compiler enumerates every `Task { … }` literal):
`command.rs`'s fresh `CreateTask`/`CaptureTask` tasks (D-11);
`merge_testutil::bound_task`; `test_support` strategies/fixtures;
storage/UI/CLI/app/sync test fixtures. Local verbs that *mutate* an
existing task (`UpdateTask`, `MoveTask`, label verbs, done) never
touch the field (D-11). `reconcile.rs`'s `merge_task_pair` produces
`remote_baseline: None` (D-10): the survivor holds no observed
snapshot — `bound.remote_seen` is a stamp, not values — so the first
post-bind pull establishes the baseline under fallback semantics.

The sync actor and the push planner/executor are untouched (fact 8).

### G-4 — the fallback rule (missing baseline)

`baseline.is_none()` ⇒ `changed` = all-true ⇒ exactly today's blanket
per-field LWW for that cycle — bit-for-bit (D-1b). The cycle still
establishes `remote_baseline := Some(of(remote))`, so convergence is
one pull. A pre-migration row (NULL column), a locally-created task
before its first pull, and a post-tombstone resurrect-free path all
decodes to this same state. The upgrade test (§4-S3) pins the decode;
the fallback-then-converge sequence gets its own unit pin (§4-T3).

### G-5 — storage: migration 0010 + codec + upgrade test

`crates/taskboard-storage-sqlite/migrations/0010_task_remote_baseline.sql`:

```sql
-- 0010_task_remote_baseline: the observed remote card snapshot per
-- task (DR-4/G, #128) — merge_task's changed-field diff reference.
-- Conventions: NULL = never observed (pre-migration rows and local
-- creates) ⇒ the fallback reads bit-for-bit as the pre-baseline LWW
-- (ADR 0005 additive policy; the 0007 header's precedent).
ALTER TABLE tasks ADD COLUMN remote_baseline TEXT NULL; -- serde_json(RemoteTaskBaseline)
```

Codec (`repo.rs`): `TaskRow` gains `remote_baseline: Option<String>`;
the four load SELECTs name the column; `decode_task` parses it with
`serde_json::from_str` and maps *any* parse failure to
`RepositoryError::Corrupted` (D-7 — typed corruption, matching the
`unknown_level_text_in_the_db_is_a_typed_corruption` precedent; a
baseline we cannot read must not silently downgrade to fallback, which
would mis-infer "remote changed"); the upsert gains the column bound
to `task.remote_baseline.map(serde_json::to_string).transpose()?`
(serialization of this plain record is infallible in practice; the
`?` keeps it total). `Cargo.toml`: add `serde_json.workspace = true`.

The serde payload is the record's derived form, e.g.
`{"title":"card 1","description":"","duedate":null,"scheduled":null,"done":null,"stack":3,"order":1,"labels":[7],"archived":false}`
— `chrono` RFC3339 strings for timestamps (the crate's existing serde
feature), sorted arrays for the label set.

### G-6 — what does NOT change

- The R4 fast path (unchanged remote never reaches the merge) — the
  gate is strictly additional filtering *inside* `merge_task`.
- `PendingTaskIntent` / `keep_task_intent` (a different race, design
  fact 7; the guard is baseline-blind, D-5).
- The sidecar fold (`fold_task_app_data`): disjoint clock domain; it
  never reads the card stamp today and never reads the baseline (D-6).
- R5 resurrect, R3 tombstoning, R6/R9 paths beyond the two clear
  sites; `reconcile.rs`'s survivorship logic; the push side; the sync
  actor; `TASK_FIELDS` (the baseline is bookkeeping, not a user
  field — no manifest row, like `remote_seen`).
- The existing test stock: skew pins, blanket-LWW proptests,
  board/stack/label merges (fixtures gain one `remote_baseline: None`
  line where the compiler demands it — assertion bodies untouched).

### G-7 — ADR 0004 amendment (dated, amendment-not-rewrite)

Two in-place sentence edits + one appended dated section:

1. Decision §"Per-field LWW" quote block gains the gate:

   > the remote value wins ⟺ =remote.last_modified > field_clock=
   > (strictly) *and the persisted remote baseline records the field
   > as changed* (tasks; ties and older remotes keep local).

2. The "Documented limitation — *granularity data loss*" paragraph is
   amended to: the loss class **closes for tasks** as of 2026-10-27
   (the persisted baseline diff); two residues stay documented — the
   *revert blind spot* (a remote write that exactly restores the
   baseline value is indistinguishable from no change and will not
   land while a local edit holds the field; deterministic, bounded —
   the next genuine remote change to the field lands it — and
   `currently_pins_`-pinned) and the boards/stacks/labels remainder
   (F-128: tasks-only scope, the mechanism generalizes mechanically).

3. Appended section:

   ```org
   * Amendment 2026-10-27 — the remote baseline closes the task
     granularity trap (#128)

   Tasks persist a =remote_baseline= (migration 0010): the remote
   card's native-lane field values as last observed, in remote
   vocabulary with raw unresolved ids. =merge_task= now applies a
   field only when the incoming snapshot *differs from that record*
   in addition to winning the per-field LWW clock comparison; the
   record refreshes at every adoption point (R1 adoption, the merge
   itself, the R9 push echo — whose server-normalized values become
   the next pull's reference) and clears with the binding on the
   tombstone paths. The granularity data loss class closes for tasks;
   the revert blind spot and the boards/stacks/labels remainder are
   the documented residues. A missing baseline (pre-migration rows, a
   local task before its first pull) falls back to the blanket rule
   for one cycle and establishes the record — behavior converged, not
   changed. The self-clobber guard's race is untouched: the baseline
   arbitrates remote-vs-local *value provenance*, not queued-op
   protection. Enforcement: the anti-clobber proptest (fields outside
   a diff's touched set keep local values and clocks) and the flipped
   granularity-trap pin in =crates/taskboard-domain/src/merge.rs=.
   ```

   (Exact wording at implementation; the content above is the
   contract — gate sentence, refresh/clear rule, residues, fallback,
   guard non-subsumption, enforcement tier.)

## 4. Test plan (TDD order)

Tier order follows `docs/testing_strategy.org`: the algebra is
pure-domain tier; storage pins its codec/upgrade edges; the engine
tier re-runs its properties and regenerates snapshots. Within the
implementation commits, each test lands with (or before) the code that
satisfies it; every commit builds standalone.

**Domain unit pins (`merge.rs` tests; fixtures via `merge_testutil`,
which gains a `baseline_of(&RemoteTask)` helper and a
`with_baseline(bound_task, &RemoteTask)` convenience):**

- **T1 — the trap flips.** Rename
  `granularity_trap_remote_entity_touch_overwrites_local_title` →
  `granularity_trap_closed_remote_touch_keeps_local_title`. Setup:
  local title edited newer (as today), baseline established from the
  remote's previous snapshot; remote write bumps only the
  description. Assert: title keeps the local value *and* its local
  clock; description adopts the remote value with clock `ts_r`;
  `remote_seen == ts_r`; baseline == `Some(of(remote))`.
- **T2 — the diff-is-per-field pin.** Baseline present, remote
  changes nothing at all but carries a newer stamp: *nothing*
  applies — every value and every clock keep local — while
  `remote_seen`/baseline still refresh (the "observation without
  adoption" contract).
- **T3 — fallback then converge.** Baseline-less task + remote write
  where the unchanged remote title is newer than the local edit: the
  title clobbers exactly as today (bit-for-bit fallback), and the
  returned task carries `Some(baseline)`; a second merge with the
  same snapshot now applies nothing (converged).
- **T4/T5/T6 — the refresh pins.** A1: `adopt_remote_task` sets
  baseline == `of(remote)`. A2 is T1/T2's tail assertion. A3:
  `adopt_task_after_push` sets baseline == `of(echo)` (server
  normalization included — build an echo whose title is
  server-trimmed and assert the baseline holds the trimmed form).
- **T7/T8 — the clear pins.** `tombstone_task` and
  `finalize_pushed_task_delete` return `remote_baseline: None` (plus
  `neutralized_task`, C3).
- **T9 — R5 unaffected.** Baseline-equipped tombstoned task + strictly
  newer remote: resurrects exactly as
  `newer_remote_edit_resurrects_local_tombstone` (the baseline never
  shields a tombstone).
- **T10 — position composite.** Remote moved the card (stack or order
  differs from baseline) ⇒ whole composite applies; remote changed
  only the title ⇒ position untouched even though `ts_r` beats the
  position clock; the unmapped-stack fallback still suppresses the
  composite.
- **T11 — labels whole-set.** Remote label set == baseline set ⇒ not
  applied; differs ⇒ applied as today (OR-set per-membership events
  are chunk L's build, riding this diff — nothing for it here).
- **T12 — the scheduled lane gate composes.** With
  `native_scheduled = false`, a changed remote `scheduled` still never
  applies (D-V2 unchanged); with `true`, it applies iff changed AND
  newer — the gate is conjunctive with the lane gate.
- **T13 — the blind spot, named as debt.**
  `remote_revert_to_the_baseline_value_stays_hidden_currently_pins_baseline_revert_blind_spot`:
  baseline title T, local title L (newer), remote write restores
  exactly T with the newest stamp ⇒ L stands (the revert does not
  land). Doc comment: deterministic, bounded, accepted residue
  (D-1d); a changed policy must consciously rename.
- **T14 — local edits keep the baseline.** A local verb path is
  engine-tier (below); here, assert directly that the merge does not
  read any baseline field it did not get — no, rather: the
  command-tier pin lives in the state crate's existing command tests
  only if cheap; the *contract* (commands never touch the field) is
  enforced by construction (no command code names it). No dedicated
  test — do not test the compiler (AGENTS §5).

**Domain proptests (`merge.rs` `proptest!`):**

- **P-G1 — anti-clobber (the acceptance property).** Strategy:
  arbitrary local task (values + clocks); arbitrary baseline;
  remote = baseline with a random subset S of the covered fields
  perturbed (each field independently, `bool`-weighted), stamped
  arbitrarily. Assert: fields outside S equal local — values *and*
  clocks; fields in S equal the remote value iff
  `remote_wins(ts_r, clocks.f)` (scheduled additionally iff
  `native_scheduled`); position in/out as a unit; `remote_seen ==
  ts_r`; `baseline == Some(of(remote))`; and no field clock ever
  decreases (subsumes the old monotonicity under the new gate).
- **P-G2 — idempotence at the baseline.** Same setup as P-G1;
  merging the merged task again with the same remote is a fixpoint
  (whole-struct equality). Complements the existing blanket-path
  idempotence proptests, which stay untouched on the fallback path.
- **P-G3 — echo round-trip (the design-§5 drift risk).** Arbitrary
  local + arbitrary echo: `adopted = adopt_task_after_push(local,
  echo)`; then a pull arrives with `remote2 == echo` in values but a
  newer stamp ⇒ `merge_task(adopted, remote2)` applies *nothing*:
  values, clocks, baseline all stable, only `remote_seen` advances.
  This is the "push→echo→pull leaves no spurious diff" acceptance
  clause. A second variant perturbs a random subset of `remote2`
  (reusing P-G1's assertion shape) — the echo-established reference
  diffs like any other.

**Storage tier (`taskboard-storage-sqlite`):**

- **S1 — codec round-trip.** A task with a fully-populated baseline
  (every field set, non-empty labels, non-null timestamps) survives
  `apply` → `load` byte-identically; `None` round-trips as `None`.
  Extends `roundtrip.rs` (the P1 fixture gains `remote_baseline:
  None`; the populated case is a dedicated test).
- **S2 — corrupt JSON is typed corruption.** Raw `UPDATE tasks SET
  remote_baseline = '{not json}'` ⇒ `load` errors
  `RepositoryError::Corrupted` (the typed-corruption precedent's
  shape).
- **S3 — the upgrade pin.** Raw pre-0010-shape INSERT (the
  `schema.rs:105` precedent): load ⇒ `remote_baseline == None`, and
  the loaded task then merges under the fallback exactly as a
  hand-built baseline-less task does (the "merging identically"
  acceptance clause: same clobber as today on the first cycle). The
  existing `insert_without_the_family_columns_inherits_epoch_defaults`
  raw INSERT keeps passing unchanged (NULL-defaulted column, nothing
  NOT NULL added).

**Engine / state tier (`taskboard-state`):**

- The P-series convergence/idempotence properties re-run unchanged
  (whole-state comparison; strategies learn the new field via
  `test_support`, D-14).
- **W1 — the shape-widener pass.** The S-series insta
  `*_persisted_state.snap` suite regenerates once (`insta` review);
  the diff must read exactly "each task row gains `remote_baseline:
  null` — or the record where that scenario's pull adopted a remote
  card (s3/s4-style scenarios) — and nothing else". Any other diff
  line is a finding against this plan.

**Mutants (for stage 3's awareness):** the gate's mutant surface is
`diff_baseline`'s per-field comparisons and the `is_none()` fallback
arm — zero expected survivors via P-G1/P-G2 (each comparison has a
property that flips its outcome); `of()`'s field copies are covered by
the round-trip and echo pins.

## 5. Decision record (every open question, resolved)

- **D-1 — placement.** `RemoteTaskBaseline` lives in `entities.rs`
  (it is part of `Task`'s persisted shape, like `TaskClocks`), not
  `remote.rs` (which holds the *incoming* remote views). `of()` lives
  on the type; `diff_baseline` lives in `merge.rs` (policy, not data).
- **D-2 — field shape.** Flat `stack`/`order` mirroring `RemoteTask`
  one-for-one (no nested `position` struct, no tuple): the diff code
  reads uniformly, serde stays trivial, and R8's unit semantics live
  in the diff (`position = stack_changed || order_changed`), not in
  the data shape.
- **D-3 — coverage.** Exactly the eight native-lane fields (title,
  description, duedate, scheduled, done, position, labels, archived).
  `deleted` excluded (R3/R5 presence semantics — a baseline row for
  it would be dead weight); sidecar fields excluded (their fold has
  its own embedded clocks; mixing domains would weaken both).
- **D-4 — lane-blind diff, conjunctive gates.** `diff_baseline`
  ignores `native_scheduled`; `merge_task`'s `scheduled` branch keeps
  its extra lane gate. Each concern keeps its own predicate; the
  conjunction is the existing `if`.
- **D-5 — guard interaction.** The baseline refreshes inside the
  three adoption functions, before `keep_task_intent`; the guard
  copies values/clocks only. Rationale: the baseline is the record of
  what the remote last showed — a queued local edit doesn't unsee it,
  and the queued op's own push+echo will re-establish the reference.
- **D-6 — sidecar fold.** Never consults the baseline (disjoint clock
  domains, the fold's existing "the card stamp is never read" rule
  extended by one more never-read).
- **D-7 — corrupt stored baseline.** `RepositoryError::Corrupted`,
  not silent fallback: an unreadable reference silently treated as
  "no reference" would flip the next merge into blanket-LWW clobber —
  the exact bug this chunk closes — on arbitrary rows.
- **D-8 — dependency.** `serde_json.workspace = true` joins
  `taskboard-storage-sqlite` (root-pinned; AGENTS §4). No other new
  dependencies.
- **D-9 — serde shape.** Derived field-name serialization, no
  `deny_unknown_fields` (a future field added by a newer version must
  not brick an older reader's load — the downgrade-tolerance
  precedent of the lenient codecs), `BTreeSet` for label order
  stability.
- **D-10 — bind ceremony.** `merge_task_pair` ⇒ `None` (the survivor
  has observed no snapshot; `bound.remote_seen` is a stamp, not
  values — inventing a "baseline" from local values would corrupt the
  diff's meaning); `neutralized_task` ⇒ cleared with the binding.
  First post-bind pull falls back, then converges.
- **D-11 — local verbs.** Fresh-task constructions set `None`;
  mutations never touch the field. The baseline changes only at the
  three adoption points — that's what makes it a *remote* baseline.
- **D-12 — no `deny_unknown_fields`** — see D-9 (recorded separately
  because it is a conscious leniency: the deliberate-leniency note
  lives on the type's doc comment per the house A1 style, even though
  the type itself is not A1).
- **D-13 — commit shape.** Three standalone commits, each a correct
  build: (1) `feat(domain): the remote baseline and the changed-field
  merge` — type, gate, adoption/clear refreshes, storage column +
  codec + upgrade/corrupt tests, all mechanical fixture fixes, the
  unit pins T1–T12, the flip, the .snap regeneration (the workspace
  cannot build with the field present and the codec blind to it, so
  domain+storage ride together); (2) `test(domain): the baseline
  property catalogue` — P-G1/P-G2/P-G3 (compile against (1), keeping
  (1) reviewable); (3) `docs(adr): the remote baseline amendment` —
  the §3-G7 text. The blind-spot pin T13 rides commit (2) with its
  sibling properties (stage 2 may fold (2) into (1) if the split
  feels artificial — the requirement is standalone commits, not this
  exact count).
- **D-14 — strategies.** `test_support` gains
  `remote_task_baseline_strategy()` (all fields, bounded timestamps,
  small label sets); `task_strategy` and the state `task_rows()`
  attach `proptest::option::of(...)` — independent of the task's
  bindings, because nothing in the domain cross-validates the record
  and the consumers (serde, merge gate, codec) are total on all
  values.
- **D-15 — the existing test stock stays untouched.** All current
  merge tests, the blanket-LWW proptests, and the DR-4/S skew pins
  construct baseline-less fixtures ⇒ they pin the fallback path,
  which is definitionally today's behavior. The *only* pre-existing
  test whose assertion changes is the granularity-trap pin (T1's
  flip) — the one test #128 exists to flip. Any other pre-existing
  assertion that needs editing is a bug in the implementation, not in
  the test (AGENTS §5: a correct test is not weakened).
- **D-16 — no conflict surfacing, no UI, no `SystemEvent`.** D-1f is
  ruled out for this round; the merge is silent except its existing
  typed `trace`/`debug` facts (one new field on the existing
  "task field verdicts" event: the per-field diff booleans — typed
  facts, never asserted by tests).

## 6. Risks

- **A missed adoption point** is the classic bug (dispatch §6.1): the
  inventory in §3-G3 is the grep-complete defense, and the echo
  round-trip property P-G3 + the refresh pins T4–T6 make any miss
  loud. Residual: a *future* adoption path forgetting the baseline —
  mitigated by ADR 0004's amendment naming the invariant ("every
  adoption point refreshes").
- **Baseline drift vs server normalization** (design §5): the echo
  path is the watch case; P-G3 pins it. Deck normalization outside
  the echo (a pull-observed reformat of a field we never pushed) is
  *correctly* read as "remote changed" and lands — today's behavior
  too; no regression.
- **The revert blind spot** (D-1d): accepted, bounded, pinned as
  debt. The next genuine change to the field heals it.
- **Storage bloat**: one JSON blob per bound task (~200 bytes
  typical); tasks already store twelve clocks as RFC3339 text — same
  order of magnitude. No action.
- **Snapshot-suite churn** (the (w) lane): one deliberate
  regeneration, diff-audited (W1). The wave enforces no concurrent
  widener.
- **Mutant surface growth** in `diff_baseline`: covered by P-G1's
  per-field assertions; stage 3's mutation run must show zero
  survivors there.

## 7. Estimated diff size

- Production: ~150 lines — `entities.rs` +45 (type, `of()`, docs),
  `merge.rs` +45 net (gate + refreshes + clears, doc-comment
  updates), `reconcile.rs` +3, `repo.rs` +15 (row, SELECTs, upsert),
  migration +5, storage `Cargo.toml` +1. Sync actor, pipeline
  production code, push side, UI: **zero**.
- Tests: ~450–550 — `merge.rs` +260 (pins T1–T13, proptests P-G1–3,
  fixtures), storage +120 (S1–S3, fixture lines), `test_support`
  +35, mechanical `remote_baseline: None` fixture lines across
  `taskboard-state`/`-app`/`-sync-nextcloud`/`-ui-slint` tests and
  benches ~80–120 (compiler-enumerated).
- Docs: ADR 0004 +35; this plan file.
- Snapshots: the shared `*_persisted_state.snap` suite (~20 files)
  regenerates — one added line per task row, nothing else.

Total ≈ 700–850 lines, of which ~150 production. M/L, matching the
dispatch table.

## 8. References

- Design: `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
  §3 D-1, §2 facts 1–4/9/16, §4 F-128, §5, §6 (chunk G).
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (wave 3), §2 (mig 0010), §3.1, §6.1 (prompts G.1–G.3).
- ADR 0004 (`docs/adr/0004-sync-conflict-policy.org`), ADR 0005
  (additive migrations).
- Issue: #128 (closes with this chunk's PR); implementation issue
  #481 (stage 3 closes both).
- Origin of the trap:
  `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md`
  §7.4.
