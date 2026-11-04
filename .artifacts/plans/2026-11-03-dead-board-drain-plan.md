# The dead-board drain — plan

- **Chunk**: backlog round 3, batch 2, chunk 2.2 (dispatch plan
  `.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md` §3)
- **Normative**: GitHub issue #282 (audit finding F10-05, origin
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-sync-persistence-report.md`)
  and its rig-evidence comment (chunk 1.1's rider via PR #562, merged
  2026-11-01, stage-3-re-run)
- **Branch**: `fix/dead-board-drain` off `origin/main` @ `7ee1c119`
  (carries chunk 1.2's PR #608 and chunk 2.1's PR #612 — both gates
  verified merged before this plan was written, 2026-11-04)
- **Size class**: S/M — one attributed cancellation pass in
  `cascade_board` plus pins; no classification changes anywhere
- **PR title (stage 3, fixed by the dispatch plan)**: `fix(sync): the
  dead-board drain cancels queued stack and label ops`

## 1. The verified landing state

The liveness hole is a five-link chain, each link verified in tree at
`origin/main` @ `7ee1c119`:

1. **The death confirms locally.** A pull whose snapshot carries a
   dead board (or whose board listing/detail proves unreadability)
   merges into a local tombstone;
   `apply_sync_report` then fires the cascade
   (`crates/taskboard-domain/src/pipeline.rs:568-580`,
   `board_deleted = boards[&board_id].deleted`).
2. **The cascade is task-asymmetric.** `cascade_board`
   (`pipeline.rs:1999-2042`) tombstones the board's stacks, live
   tasks, and labels, and cancels pending outbox ops **only** through
   `op_targets_task` (`pipeline.rs:2027-2031`). Stack ops, label ops,
   board ops, ref-deletes, and flush triggers attributed to the dead
   board survive in the outbox.
3. **The wire answers every write with 403.** Tier-2 rig evidence
   (issue #282 comment; PR #562 §"#282 rider", probe
   `it_nextcloud_docker_probe_dead_board_write_answers`, Deck
   1.19.1 / NC 35, independently re-run by that chunk's stage 3):
   `create_card`, `create_stack`, and `create_label` against a
   confirmed-dead board each answer `DeckError::Forbidden` (403).
   The probe pins typed decodability only — the concrete variants are
   recorded evidence, deliberately not a contract.
4. **The classification strands `Forbidden` as retryable.**
   `classify_push`
   (`crates/taskboard-sync-nextcloud/src/mapping.rs:154-184`):
   `Forbidden` on a delete maps to `RemoteMissing`
   (`mapping.rs:166`, tier-2-verified); every other `Forbidden` maps
   to `Rejected { SyncErrorKind::Forbidden }` (`mapping.rs:167`).
5. **The R9 table only dead-letters `BadRequest`.**
   `apply_push_outcomes`
   (`pipeline.rs:1935-1956`): a `Rejected` outcome dead-letters only
   `BadRequest`, or the narrow post-wipe `LocalData` signature
   (`dead_letters_localdata`, `pipeline.rs:1968`). `Forbidden` logs
   "push rejected, stays queued" — the op retries every cycle, each
   cycle paying one doomed call, forever. F10-05 exactly.

**The asymmetry the issue names is pinned today**: 
`board_cascade_finalizes_present_entities_and_spares_the_dead`
(`pipeline.rs:4261`) asserts `pending_ops == 1` with the message
"stack op survives; task op cancelled" (`pipeline.rs:4365-4367`).
That is a limitation pin ("no pins of today's behavior", AGENTS §5);
the fix deliberately flips it (D-8).

**The read path already has the precedent.** The pull-side dead-board
carve-out (`crates/taskboard-sync-nextcloud/src/actor.rs:1302-1333`)
keys on confirmed-dead state (`dead_board: bool`, derived at
`actor.rs:1110-1174` from the tombstone observation — remote death
observed, never inferred from one code) and answers the *expected*
`NotFound | Forbidden` reads with empty listings instead of wedging
the cycle. The write side has no analogous finalization: that is the
chunk.

## 2. The fix shape (D-1): symmetric cascade cancellation

**Decision**: extend `cascade_board` with one attributed outbox pass —
after the tombstoning loops, cancel every pending op attributed to the
dead board. Symmetric with the task-op cancellation the cascade
already performs; one pass; domain-pure.

The rejected alternative — an executor write-side carve-out (give
`PushCtx` a dead-board bit so `classify_push` maps
`Forbidden → RemoteMissing` for ops of a confirmed-dead board) — loses
on four recorded grounds:

1. **Testable tier.** The cascade fix is fully pinned at the pure
   domain tier (`apply_sync_report` + the existing `merge_testutil`
   fixtures — no HTTP, no fixtures beyond what `pipeline.rs` tests
   already use). The carve-out's liveness journey spans two tiers: the
   sync-tier classification (wiremock/hermetic rig fixtures) *and*
   the domain R9 handling of the fabricated `RemoteMissing`; no
   single tier can pin the whole contract.
2. **No code inference anywhere.** The cascade keys on domain state
   only (the local tombstone). The carve-out makes the *classifier*
   branch on local state to fabricate a "gone" verdict for an op
   whose remoteness was never observed — an inference by
   classification. The dispatch fence "never on the 403 alone" is
   satisfied by the cascade *structurally*: no classification changes
   at all, so live-board retry semantics are untouched by
   construction, and no HTTP-code knowledge moves into or around
   `taskboard-domain`.
3. **Coverage totality and doomed-call economics.** The cascade pass
   is total by attribution (one rule over `LocalOp` via the shared
   helper, D-4) and finalizes all stranded ops at the confirming pull
   with **zero** doomed writes. The carve-out finalizes one op per
   classified attempt — every stranded op still pays exactly one
   doomed call, and the carve-out must be threaded through every
   classification site separately (`mapping.rs` push groups *and*
   `flush.rs:503`'s flush lane) or the lane fragments.
4. **The house precedent's own shape.** The read carve-out's
   principle is "expected failure against confirmed-dead state
   finalizes instead of wedging the machine". The cascade implements
   that principle at the layer that owns the outbox; the carve-out
   would bolt a second, weaker copy of it into the classifier while
   the pull-side one already lives at the state layer.

## 3. The liveness contract

> When a sync report confirms a board dead (the local row is, or
> merges to, tombstoned), every op in that report's outbox attributed
> to that board — except the board-delete intent (D-3) — finalizes in
> that same report as a `PersistenceAction::FailOp`. From that report
> on, no queued op targets the dead board, and no write against it is
> ever paid. A queued `DeleteBoard`, if present, still drains by its
> own classified push outcome under the existing tier-2-verified
> `Forbidden`-on-delete → `RemoteMissing` rule (one call, one cycle).

No work is lost: every op the drain cancels predates the death (the
command layer rejects mutations on deleted entities — the
`update_task_rejects_unknown_and_deleted` guard family,
`command.rs:3701`), so each cancelled op's effect is moot the moment
the board's death is confirmed.

## 4. Decisions

| # | Decision | Status |
|---|---|---|
| D-1 | Fix shape: symmetric cascade cancellation inside `cascade_board`; the executor carve-out is rejected (§2) | **decided** |
| D-2 | Cancellation scope: **uniform attribution rule** — every pending op whose `attributed_board` resolves to the dead board, with exactly one exception (`DeleteBoard`, D-3). Stack ops, label ops, task ops, assign ops, `UpdateAppData`, `UpdateBoard`, ref-deletes (`DeleteRemoteCard`/`DeleteRemoteStack`), and `FlushBoardAppData` all cancel. Ref-deletes and the flush trigger are included because their goals (dedupe, sidecar upload) are moot on a dead board, and per-kind exception lists would fragment the invariant into a second decision table | **decided** |
| D-3 | The `DeleteBoard` exception is **load-bearing**, not stylistic: a local delete queues `DeleteBoard` and tombstones the row *before* the remote dies; cancelling that op would strand the local tombstone against a live remote and resurrect the board at the next pull. The op must drain through its own push outcome. Pinned by P-3 | **decided** |
| D-4 | The pass reuses `attributed_board` (`crates/taskboard-domain/src/state.rs:355` — the same helper `apply_push_report` uses for its affected-boards computation at `pipeline.rs:1616`), so the drain cannot drift from the push report's own notion of attribution. It needs only the stacks/tasks/labels maps `cascade_board` already holds; no signature change beyond nothing — the helper takes exactly those three | **decided** |
| D-5 | Inside `cascade_board`, the new attributed pass **replaces** the per-task `op_targets_task` loop (`pipeline.rs:2027-2031`): for this board the attributed pass is a strict superset (every op targeting a task of the board resolves to the board through the stack chain; the `!op_results.contains_key` guard keeps the overlap idempotent either way). One rule per function. The *other* `op_targets_task` call sites (`pipeline.rs:1123`, `:1224`, `:1253`, `:1357` — stack-level cascades and R9e handlers) are untouched: they key per entity, not per board | **decided** |
| D-6 | Pins live at the **pipeline tier only** (`apply_sync_report` / `apply_push_report` tests, the lowest layer owning the behavior — AGENTS §5 "one behavior, one test, lowest layer"). No engine-tier duplicate: the state engine's `SyncReport` routing is generic and already pinned; the dispatch acceptance's "engine/pipeline tier" is satisfied by the pipeline tier. No sync-crate tier tests for the fix (it changes no sync-crate code) | **decided** |
| D-7 | Tier-2 rig re-verification is **out of scope**. The 403 evidence is recorded twice (issue #282 comment; PR #562 §"#282 rider") and was independently re-run by that PR's stage 3 on the merged branch. The domain pins need no wire: the recorded 403 enters the pins only as its typed consequence, `PushResult::Rejected { SyncErrorKind::Forbidden }` (P-2's fixture input). Re-running the probe would re-derive evidence this chunk does not consume | **decided** |
| D-8 | Flipping `pipeline.rs:4365-4367` ("stack op survives") to `pending_ops == 0` is a **recorded behavior change**, not a test weakening: the old assertion pins the bug F10-05 reports. The flip commit carries the F10-05 reference in its message; the test's framing comment updates to state the drain | **decided** |
| D-9 | CHANGELOG: one `Unreleased` → `Fixed` entry (user-observable reliability fix: ops queued on a deleted board no longer retry forever). Exact wording at stage 2 | **decided** |
| D-10 | Fences: no `classify_push` changes, no read-path changes (`fetch_stacks_listing` and the deadness derivation stand), no `SyncErrorKind` grammar changes, no Deck-version-specific behavior, no changes to live-board retry semantics, rig probe file untouched | **decided** |

## 5. Pin catalogue (TDD order)

All at the domain pipeline tier, using the existing
`base_state`/`bound_task`/`bound_label`/`remote_*`/`apply_sync_report`
fixture kit (`pipeline.rs` tests module). Commit order mirrors chunk
2.1's landed pattern: red pins first, then the fix.

- **P-1 `dead_board_pull_drains_queued_stack_and_label_ops`** (new,
  red). A bound board whose snapshot board carries `deleted_at`; the
  outbox queues `CreateStack`, `RenameStack`, `MoveStack`,
  `CreateLabel`, `UpdateLabel` attributed to that board. Assert: each
  op id appears as `PersistenceAction::FailOp` in the actions, and
  `pending_ops == 0`. This is the liveness contract, typed.
- **P-2 `live_board_forbidden_strand_still_retries`** (new, green —
  the anti-overreach guard, written before the fix so it can never
  silently regress). A **live** bound board; `apply_push_report`
  receives `PushOutcome { result: Rejected { kind: Forbidden } }` —
  the typed shape of the measured transient 403. Assert:
  `pending_ops == 1` (stays queued) and phase stays `Idle`.
  Sibling of the existing
  `rejected_op_stays_queued_but_bad_request_dead_letters`
  (`pipeline.rs:3674`), which this test extends from `Network` to the
  `Forbidden` strand the fix is about. No HTTP anywhere.
- **P-3 `drain_spares_the_board_delete_intent`** (new, red). Dead
  board; the outbox queues `DeleteBoard` (plus one stack op as
  control). Assert: the stack op `FailOp`s, the `DeleteBoard` op id
  appears in **no** action (stays queued for its own push outcome).
  Pins D-3 against the resurrection hazard.
- **P-4 `drain_cancels_board_scope_ops_uniformly`** (new, red). Dead
  board; outbox queues `UpdateBoard` and `FlushBoardAppData`
  attributed to it. Assert: both `FailOp` (`pending_ops == 0`).
  Pins the uniform rule's board-scoped arms (D-2); ref-deletes and
  `UpdateAppData` ride the same `attributed_board` match and get one
  combined assertion here if the fixture cost stays trivial,
  otherwise they are covered by D-2's uniformity argument plus P-1.
- **P-5 (flip) `board_cascade_finalizes_present_entities_and_spares_the_dead`**
  (existing, `pipeline.rs:4261`): the `pending_ops == 1`
  "stack op survives" assertion becomes `== 0` with the comment
  stating the drain (D-8). Turns green with the fix.
- **P-6 regression watch (existing, untouched)**:
  `remote_missing_stack_and_label_ops_finalize_tombstones`
  (`pipeline.rs:5148`) and the `Forbidden`-on-delete classification
  pins (`mapping.rs` tests) must stay green verbatim — the 404/404-like
  drain paths and the delete carve-out are not this chunk's to change.

Also verify during stage 2: grep the full test tree for other
assertions of stack/label ops surviving a board cascade (the survey
for this plan found exactly one — `pipeline.rs:4365-4367` — but the
gate run is the authority).

## 6. Implementation sketch

In `cascade_board` (`pipeline.rs:1999`), after the three tombstoning
loops, replace the per-task cancellation loop (D-5) with:

- collect the pending op ids once: for each `entry` in `outbox` not
  already resolved in `op_results`, if
  `attributed_board(&entry.op, stacks, tasks, labels) == Some(*board_id)`
  **and** the op is not `DeleteBoard` → `op_results.insert(op_id, false)`;
- a `tracing::debug!` line in the house R-rule style (the cascade
  already logs at `pipeline.rs:570`).

The pull path needs zero further changes: `apply_sync_report`
already converts `op_results` into `PersistenceAction::CompleteOp` /
`FailOp` actions (`pipeline.rs:1389-1391`) and the sync aggregates
follow `pending_ops`.

## 7. Risks & tripwires

1. **The flipped pin (P-5) is the chunk's one deliberate behavior
   change.** Any *other* test failing at the gate run is triaged
   against D-2/D-3: either the test pins the old asymmetry elsewhere
   (flip with a recorded reason, same as D-8) or the fix overreached
   (fix the fix, never weaken the test silently).
2. **Overreach trap (stage 3's named trap)**: a fix that dead-letters
   any live-board `Forbidden` strand is a regression dressed as a fix.
   P-2 exists to catch exactly that; the fix never touches
   classification, so the trap's only entry is someone "simplifying"
   into the carve-out — rejected by D-1.
3. **Attribution misses**: `attributed_board` returns `None` for ops
   whose entity rows are gone; such ops stay queued (unchanged
   behavior — other flows own them). Not a drain regression; do not
   widen the helper.
4. **Repeat fires**: the cascade re-fires on every later pull of the
   board while the row stays tombstoned, so an op queued in a crash
   window after the confirming pull drains at the next one. The
   contract (§3) claims only the confirming report's outbox; the
   re-fire is an observed property of the existing call site, not a
   new guarantee, and is not pinned.
5. **Local-delete-in-flight boards** (delete queued, remote still
   live) already fire the cascade for task ops today; the fix extends
   the same finalization to their stack/label ops. Semantically
   correct (D-2's moot-work argument) and P-3 keeps the delete intent
   alive — but stage 2 should confirm no existing local-delete-flow
   test asserts surviving stack ops (see §5's grep note).

## 8. Stage-2 execution order

1. Recreate the worktree on `fix/dead-board-drain` if missing (AGENTS
   §8; the branch is fresh, no PR exists).
2. Commit 1 — `test(domain): the dead-board drain pins, red-first`
   (P-1 … P-4; P-2 green, the rest red).
3. Commit 2 — `fix(domain): the dead-board drain cancels attributed
   ops` (the §6 change; P-1/P-3/P-4/P-5 green).
4. Commit 3 — `docs(changelog): the dead-board drain` (D-9), or fold
   the CHANGELOG into commit 2 if the entry is final at that point.
5. Full AGENTS §10 gate suite via `scripts/run-quality-gates.sh`.
   No push, no PR (stage 3 ships per the dispatch model).

## 9. Estimated diff size

| File | Change |
|---|---|
| `crates/taskboard-domain/src/pipeline.rs` | ~ +30 / −15 (the pass, P-1…P-5, the flipped comment) |
| `CHANGELOG.md` | ~ +4 |
| **Total** | **~ 50 lines, one crate** — plus zero sync-crate diffs (D-10) |
