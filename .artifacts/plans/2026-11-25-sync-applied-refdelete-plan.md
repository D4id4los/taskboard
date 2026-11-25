# Chunk B4 Plan — Sync Applied/ref-delete + Orphan Dedup (#1329, #1331)

- **Date**: 2026-11-25
- **Chunk**: round 7, B4 (`fix/sync-applied-refdelete`), stage-1 plan.
- **Origin**: `.artifacts/reports/2026-11-25-backlog-reduction-round7-scope-report.md` §B4;
  `.artifacts/plans/2026-11-25-backlog-reduction-round7-dispatch-plan.md` §1/§3;
  issues #1329 (p2 bug), #1331 (p3 refactoring). Issue bodies normative.
- **Scope correction (recorded)**: the dispatch prompt says
  `crates/taskboard-sync-nextcloud`; both issue bodies (normative) point
  at `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs`, and the
  code confirms it — both defects live in the **files** connector's
  board cycle. The Deck connector's `drain_standalone_orphans`
  (`taskboard-sync-nextcloud/src/actor.rs`) executes groups through the
  push executor and does **not** re-derive the row-present rule, so it
  is not a #1331 consumer. Stage 2 works in
  `taskboard-sync-files-nextcloud` + `taskboard-domain`.

## 1. Diagnosis

### 1a. #1329 — the unresolved ref-delete reporting `Applied`

The defect sits in `drain_board`'s group walk
(`crates/taskboard-sync-files-nextcloud/src/board_cycle.rs`, the
`PushTarget::RemoteCard` / `PushTarget::RemoteStack` arms):

```rust
None => {
    tracing::warn!(
        board = ?board,
        "a deck-tagged ref-delete with no local row stays queued"
    );
}
```

Both `None` arms warn but **do not `continue`** — unlike the resolved
tombstone arms (`continue` after pushing to `chunk_ops`) and the
`Document` arm (`continue` unconditionally). Control falls through to
the shared tail:

```rust
chunk_ops.push(group.primary);
chunk_ops.extend(group.subsumed.iter().copied());
```

so the op id of the ref-delete that never materialized lands in
`chunk_ops`. `chunk_ops` feeds `publish_drain_report` **only after the
create-once PUT of the whole chunk succeeded**, and that report stamps
every id in the list `Applied { echo: None }`. When the same drain
publishes a chunk containing *other* entities (a dirty task/stack/label
or a resolved tombstone), the PUT succeeds and the never-published
ref-delete is reported `Applied` — a phantom Applied: the op completes
locally while the remote entity survives. The entries-empty guard
(`a_deck_tagged_ref_delete_with_no_local_row_stays_queued`) already
pins the single-op case; the defect is the mixed-chunk fall-through.

### 1b. #1331 — the row-present rule re-derived per target

Two sites in `board_cycle.rs` independently encode "which `PushTarget`
kind maps to which table, and what counts as a present row":

1. `drain_orphans_only`'s per-group `row_present` match — modeled kinds
   → `tables.<kind>.contains_key(&id)`, remote kinds (`RemoteCard`,
   `RemoteStack`, `Document`) → `false`.
2. `drain_board`'s entries-empty synthetic-completion filter — the
   same `contains_key` matrix negated, remote kinds → `false`.

This duplicates knowledge the planner (`plan_pushes`,
`crates/taskboard-domain/src/push.rs`) owns; a new target kind or a
changed wiped-row rule can silently diverge the two filters from what
the planner materializes.

## 2. Decisions (every open question resolved)

### D1 — the typed-honest outcome shape for #1329 (no new `PushResult` variant)

The recorded unresolved posture is the one the orphan-law comment
already names and the entries-empty test already pins: **the op stays
queued, emits no outcome, and retries next cycle** (the warn log is the
loud trace). The fix is the two missing `continue`s, making the
resolved and unresolved ref-delete arms structurally identical to the
`Document` arm. `publish_drain_report`'s `Applied` stays reserved for
ops whose entries actually went on the wire.

*Rejected*: emitting `PushOutcome { result: Rejected { .. } }` or a new
`PushResult::Unresolved` variant for the unresolved case — it would
attribute a sync failure class that never happened, and `Rejected`'s
contract (queue-with-backoff semantics per kind) is not what an
unresolved ref means; the queued/retry posture needs no outcome. The
"typed-honest" acceptance is met structurally: `chunk_ops` is
type-level restricted to ops whose group contributed a wire entry, and
the test matrix pins it.

### D2 — the dedup's home: `taskboard-domain/src/push.rs`, beside `PushTarget`/`EntityTables`

A three-state presence accessor (not a bare `bool` — the two consumers
apply **genuinely different postures to remote targets** and that
difference must stay explicit and exhaustive):

```rust
/// The planner's row-presence knowledge, derived once.
pub enum TargetRowPresence {
    /// A modeled target whose entity row exists in the tables.
    Present,
    /// A modeled target whose entity row is wiped.
    Wiped,
    /// A remote ref / document target — no local row by construction.
    Remote,
}

impl PushTarget {
    #[must_use]
    pub fn row_presence(&self, tables: &EntityTables<'_>) -> TargetRowPresence;
}
```

Consumer postures (unchanged behavior, one match each):

- `drain_orphans_only`: `Wiped | Remote => completion`,
  `Present => warn + stay queued` (a present row means the group needs
  a board lane).
- `drain_board`'s synthetic filter: `Wiped => synthetic completion`,
  `Present | Remote => not synthetic` (an unresolved ref-delete is not
  synthetic — it stays queued, per the landed orphan-law comment).

Exhaustive matching means a future `PushTarget` variant forces both
call sites (and the accessor) to decide — the #1331 drift hazard dies.

Pure domain code; no network/async/UI deps; respects crate boundaries
(`taskboard-sync-files-nextcloud` already depends on
`taskboard-domain`).

### D3 — comment-truth fix rides (same function, zero code change)

`drain_orphans_only`'s doc comment says a "ref-tombstone relay … stays
queued loudly until a board binds", but the code (and behavior, pinned
by the zero-board tests' shape) **completes** remote-target orphan
groups synthetically: a ref-delete orphan whose board row is wiped has
no local row by construction and no board subtree to relay into, so it
accounts and drains rather than stranding forever (the #1150 law).
Stage 2 corrects the comment to state the actual posture. No behavior
change; the `Failed { board: None }` accounting prose already matches
the code (`SyncReport::Failed { board: None, .. }` carrying
`Applied`-stamped pushes — verified against
`files_lanes_drain_orphans_when_zero_boards_bound`).

### D4 — what stays untouched on healthy paths

The sync corpus is unchanged: group planning, chunk composition, the
create-once PUT, the 412 reconcile path, `publish_drain_report`'s
report shape, the entries materialization loops, and the Deck
connector's standalone drain are all untouched. The only wire-adjacent
delta is that an unresolved ref-delete's id no longer rides a mixed
chunk's report — which is exactly the bug.

### D5 — the `#[allow]` posture

`drain_board` already carries `#[allow(clippy::too_many_lines)]`; the
two `continue`s do not change that. No new lints expected; the accessor
adds `#[must_use]`.

## 3. Test plan (the matrix)

All in `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs`
`#[cfg(test)]` (fake transport, existing fixtures) plus one domain unit
test in `push.rs`.

**#1329 matrix — published / unpublished / unresolved:**

1. **Unresolved (the bug pin)** — a mixed drain: one dirty task (row
   present) + one `DeleteRemoteCard` whose local task row is absent +
   one `DeleteRemoteStack` whose local stack row is absent. Assert:
   the drain report flies (the chunk published); the report's push id
   set contains the task op's id but **not** the two ref-delete ids;
   both ref-delete ops are still in the outbox (structural: the
   persisted state's queue, not a log/text assertion). This proves the
   reclassification does not mask the genuine publish.
2. **Published (healthy path unchanged)** — a mixed drain with a
   *resolved* deck-tagged ref-delete (local row present → tombstone)
   plus a dirty task: the report stamps **both** ids `Applied`; the
   wire (fake transport's request log) carries the chunk with the
   tombstone entry. Extends the existing
   `a_files_tagged_ref_delete_materializes_its_tombstone` /
   `n19_ac2_a_ref_delete_no_longer_strands_on_a_files_board` shape to
   the deck-tagged mixed case.
3. **Unpublished (wire failure)** — the matrix of (1) with the fake
   transport rejecting the PUT: no drain report flies; all three ops
   (task included) stay queued. Pins that the fix is not
   "unresolved ops are silently dropped on failure paths".
4. **Unpublished (entries-empty)** — already pinned by
   `a_deck_tagged_ref_delete_with_no_local_row_stays_queued`; kept as
   the matrix's third leg, unchanged.
5. **Retry** — follow-up cycle after (1): the unresolved ops are still
   queued and the next drain attempts them again (structural: still in
   outbox; second-cycle report absent while unresolved). No sleeps —
   deterministic re-invocation.

**#1331 pins:**

6. **Domain unit (table-driven)** — in `push.rs`: every `PushTarget`
   kind × {row present, row wiped} maps to the right
   `TargetRowPresence`; remote kinds are `Remote` regardless of table
   contents. This is the one derivation; the consumers are covered by
   the behavior tests below not duplicating it (one behavior, one
   test, lowest layer).
7. **Consumer behavior unchanged** — the existing zero-board tests
   (`files_lanes_drain_orphans_when_zero_boards_bound`,
   `files_orphan_drain_contacts_no_server_for_noop_groups`) and the
   entries-empty synthetic path pass unmodified: the refactor is
   corpus-neutral. If any needs a mechanical update (e.g. imports),
   that is the only allowed touch.

Test-rule compliance: typed/structural assertions only (no text, no
ceilings); no sleeps; assert terminal state.

## 4. Files touched (estimated diff)

| File | Change | Size |
|---|---|---|
| `crates/taskboard-domain/src/push.rs` | `TargetRowPresence` + `PushTarget::row_presence` + the table-driven test | ~70–90 lines |
| `crates/taskboard-sync-files-nextcloud/src/board_cycle.rs` | the two `continue`s; the two consumer matches re-homed onto `row_presence`; the D3 comment fix; tests (1)–(3), (5) | ~120–150 lines |

Total ~200–240 lines including tests. No new files, no new deps, no
crate-boundary changes.

## 5. Risks

- **Masking a genuine publish** (the dispatch plan's named trap):
  mitigated by matrix test (1) asserting the co-published ids are
  `Applied` and only the unresolved ids drop out.
- **Polarity confusion in the dedup** (the two consumers treat remote
  targets oppositely): the three-state `TargetRowPresence` makes each
  posture an explicit, exhaustive match arm instead of a silent `false`
  — the risk is named in D2 and pinned by test 6.
- **Comment/behavior drift** around the orphan law (D3): resolved by
  correcting the prose to the pinned behavior; if stage 2 finds the
  *behavior* contradicts a normative plan/test rather than a comment,
  that is a stop-and-report, not a silent semantics change.
- **Out-of-scope observation** (recorded as a backlog issue, see §6):
  the owning_board doc (`outbox.rs` ~§240) claims the orphan lane
  "materializes ref-deletes from the remote ref alone", while the
  files zero-board drain completes them synthetically without any
  wire deletion. B4 does not change this posture; the gap between that
  prose and the behavior belongs to a dedicated decision.

## 6. Backlog issues opened by stage 1

- Recorded in the final stage report (issue numbers listed there per
  the dispatch protocol).
