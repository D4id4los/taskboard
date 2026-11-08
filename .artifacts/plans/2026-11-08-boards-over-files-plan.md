# [Plan] Boards over Files — Chunk 5 of the B-full Dispatch (D-BF8–D-BF10)

- **Date**: 2026-11-08
- **Kind**: stage-1 TDD-first implementation plan for dispatch chunk 5
  (`feat/boards-over-files`, issue #688). No code in this commit.
- **Origin**: the accepted design plan
  `.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`
  — normative: §3 Part E (D-BF8–D-BF10), §5 test hooks, §6 risks
  (the "chunk 5 is L" split option); the dispatch plan
  `.artifacts/plans/2026-11-07-boards-over-files-implementation-dispatch-plan.md`
  §6; issue #688; the DR-8 round Parts C–E
  (`.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  — conditional-PUT discipline, change detection, the engine seam);
  the WebDAV characterization report
  `.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`
  (F3 create-once, F4 lost-update guard, F5 etag semantics, F6
  PROPFIND change detection — the transport facts the board cycle
  inherits).
- **Base**: `origin/main` @ `0ae47c1f`. Preconditions verified: chunk 3
  (`94072694` tagged remote identities + partition routing) and chunk 4
  (`fda8bc95`…`0ae47c1f` board chunk & snapshot lifecycle) are landed.
- **Status**: READY for stage 2. Every open question below is settled
  with a recorded decision; the one genuine user-visible ruling (the
  PR split) is recorded as a **proposal** — the user rules at review.

---

## 1. What this chunk builds

Boards riding the files substrate end to end:

1. **D-BF8 — the outbox is the routing fabric.** Each connector actor
   drains only the boards it owns (the chunk-3 `BoardOwnershipMap`).
   The files connector materializes its boards' dirty entities into an
   immutable chunk at drain time (the `CardShape` precedent:
   materialize from published state, not from op replay), publishing
   `ops/NNNN.json` create-once.
2. **D-BF9 — the pull side.** The files connector folds peers'
   snapshots + chunks per board (chunk 4's `board_fold`), reports
   `SyncReport::Completed` with a **grown** `FilesBoardSnapshot`, and
   the state engine ingests it through the **existing** per-field LWW
   merge pipeline — the `RemoteBoardSnapshot::Files(_) => fold as
   nothing` arm (`taskboard-state/src/actor.rs:1074`) is the seam this
   chunk replaces. Board-level sections ride the board documents;
   app-only task fields are ordinary chunk fields.
3. **D-BF10 — no sidecar on files-boards.** No anchor card, no
   attachment, no `FlushAppData` push group for a files-owned board;
   `UpdateAppData`/`FlushBoardAppData` route to the files connector as
   ordinary dirty-entity materialization. Deck boards keep the sidecar
   verbatim (regression-pinned).

Non-goals (unchanged from #688): the move ceremony (chunk 6), Deck
behavior change, org anything, per-user section changes (chunks 1–4's
per-user cycle is untouched except where it shares the actor loop).

## 2. Verified seams (against `0ae47c1f`)

| Seam | Where | What chunk 5 does |
|---|---|---|
| Outbox ops | `domain/outbox.rs` — `LocalOp` :23, `UpdateAppData(TaskId)` :81, `FlushBoardAppData(BoardId)` :89, `owning_board(&EntityTables)` :243 | The dirty-entity→board attribution; consumed unmodified |
| Planner | `domain/push.rs` — `PushGroup` :271, `CardShape::from_task` :237, `plan_pushes` :372, document bucket :415–429, `kind_rank` :453 | Gains ownership awareness for the two document-op arms (§5); everything else byte-stable |
| Push outcomes | `domain/remote.rs` — `PushResult::Applied { echo }` :280, `RemoteEcho` :299 | Files ops report `Applied { echo: None }` (§4.4) |
| Ownership map | `domain/ownership.rs` — `BoardOwnershipMap` :22, `owner_of` :53, `bindings()` :60; refusal at `command.rs:1871`; boot guard `bootstrap.rs:1282` | Read-side consumer: `owner_of(board) == Files(connector)` selects the board cycle's work set |
| Files transport | `sync-files-nextcloud/transport.rs` — `FilesTransport` :23 (`mkcol`, `propfind_devices`, `get`, `put_create`, `put_if_match`), `PropfindListing` :81 | Extended with a listing call over a board subtree (§4.2); `put_create` (If-None-Match: \*) publishes chunks — F3's server-enforced immutability |
| Per-user cycle | `sync-files-nextcloud/cycle.rs` — `steady_cycle` :533, `CycleVerdict` :260; `actor.rs` — `spawn_files_sync_actor` :69 | The **board cycle is added beside it** in the same actor (§4): same poll cadence, one more lane |
| Chunk-4 exports | `sync-files` — `board_chunk::BoardChunk` :38, `board_payload::{EntityEntry, from_task/from_stack/from_label/from_board, to_task/…}` :149–495, `board_gc::{SNAPSHOT_EVERY_N_CHUNKS, next_chunk_range, late_join_floor}` :33–106, `board_fold::fold_board` :88, `layout::{classify, board_ops_path, board_snapshot_path}` | Consumed, not modified (except the additive slot extension §6) |
| State ingestion | `taskboard-state/src/actor.rs` — `ingest_sync_report` :985, the `Files(_) => fold as nothing` arm :1074–1086, `apply_sync_report` (pipeline.rs:446) | The Files arm grows into real ingestion through the existing merge pipeline (§4.5) |
| Sidecar machinery | `sync-nextcloud/anchor.rs` (`scan_snapshot` :78), `flush.rs` (`run_flush` :249); enqueue sites `domain/command.rs` :569–590 + ~25 more | Never engaged for files-boards; the Deck actor's `as_deck()`-else-skip (actor.rs:514) already guarantees it — the pin makes it law (§7) |
| App wiring | `app/connectors.rs` (`ConnectorModule` :51), `app/bootstrap.rs` (`files_sync_spawn` :193, `FILES_SYNC_CONNECTOR_NAME` :103) | The spawned files actor gains the board cycle; no new module (§8) |
| Deck regression pins | `taskboard-state/tests/snapshots.rs:2579` `s_deck_full_sync_golden`; second-claim journey `tests/engine.rs:4276` | Untouched, must stay green verbatim |
| Two-engine template | `taskboard-app/tests/files_sync_convergence.rs` (:361, :460) | The journey design's template (§9) |
| Board sections today | `domain/appdata.rs` — `AppDataSections` :81–136: `"time-entries"`, `"habits"`, `"habit-log"`, `"break-rules"` (board-level), `"reminder-rules"`, `"dismissals"` (card-keyed); retention :2032/:2193 | The wire-name + row-codec source for the board-document sections (§6) |

## 3. Open questions → decisions

### D1 — Where does routing-by-assignment happen? (D-BF8)

**Decision: at the connector actors, exactly as the Deck actor does
today** (`sync-nextcloud/actor.rs:528` filters the outbox by
`owning_board` against its bindings). `plan_pushes` itself stays
mostly connector-blind — it is a pure domain planner over
`(PendingOp, EntityTables)` and must not grow a connector concept into
every bucket. Only the two document-op arms change (D3 below). Each
connector iterates `map.bindings()` for its own `ConnectorId`; the
files actor's work set is `{ board | owner_of(board) == Files(me) }`.

### D2 — One actor, two lanes

**Decision: the files board cycle lives in the same spawned actor as
the per-user cycle** (`spawn_files_sync_actor`), as a sibling lane
invoked on the same poll tick: first the per-user cycle (unchanged),
then, per owned files-board, the board cycle (§4). Rationale: one
credential, one transport, one poll cadence, one backoff state; a
second actor would duplicate all four and re-open the attach-race the
per-user cycle already solved. The lanes share nothing but the
transport and the tick — a board-cycle failure is typed and retried
per board without disturbing the per-user verdict.

### D3 — The no-sidecar routing rule (D-BF10)

`UpdateAppData(task)` / `FlushBoardAppData(board)` are enqueued
connector-blind (~25 command.rs sites) — that stays true; the enqueue
sites never learn about connectors.

**Decision: route, don't suppress — at the `Document` arms of
`plan_pushes`.** `plan_pushes` gains an ownership input
(`&BoardOwnershipMap`) and resolves each document op's board:

- **owner = Files** → the op joins that board's **dirty-entity bucket**
  as an entity touch: `UpdateAppData(task)` marks `task` dirty
  (its app-only fields — §6.2 — re-materialize into the next chunk);
  `FlushBoardAppData(board)` marks the board entity (and, for the
  flush's board-level sections, the section payload — §6.1) dirty. No
  `PushGroup::target == Document/FlushAppData` group is ever produced
  for a Files-owned board. Nothing else changes: the op is still the
  dirty signal, the op still drains, the outbox still empties — it
  just materializes as chunk fields instead of an attachment.
- **owner = Deck** → today's behavior verbatim (the `FlushAppData`
  group, deferred between pull and report).
- **no owner** (standalone, files connector not enabled/running) →
  today's behavior for unresolvable ops: stay queued. The UI's
  pending-ops projection (`derive_sync_states`) already shows them as
  pending; adoption (§7.2) drains them.

This keeps `plan_pushes` pure (the map is domain data), keeps enqueue
sites dumb, and makes the no-sidecar property a **plan-shape** fact a
test can pin: for any Files-owned board, no planned group carries
`PushTarget::Document` and `CardShape` materialization never runs for
its tasks. The Deck golden (§9.6) proves the Deck arm is unchanged.

### D4 — Chunk emission is ownership-triggered, not op-replayed

**Decision: at drain, per board, materialize from the published state
(the `CardShape` precedent), never by replaying ops.** The files actor
takes the plan's dirty-entity set for the board, reads each entity's
current `EntityTables` row (fields + clocks + authors — chunk 1's
stamp), and mints `EntityEntry::from_task/from_stack/from_label/
from_board`. Deletes arrive as the entity's `deleted` field set
(tombstones are forever — chunk 4's law); the tombstoned entity
materializes once, then never again. `board_gc::next_chunk_range`
computes `first_seq/last_seq`; `put_create` (If-None-Match: \*) 
publishes `ops/NNNN.json` — the server's 412-on-recreate (F3) is the
immutability enforcement; a 412 here is a typed bug (own-seq collision
— the seq bookkeeping is per-device, so it must not happen).

### D5 — Snapshot duty rides chunk 4 verbatim

`snapshot_due(chunks_since, is_boot)` (chunk 4) governs: on cycle
boot and every 16 chunks the owner folds its own view and publishes
`snapshot-NNN.json` via `put_if_match` (single-writer, F4). GC
(`gc_eligible`) runs after a **published** snapshot only. No new
constants.

## 4. The board cycle (per owned files-board, per tick)

### 4.1 Drain (push)

1. Read the outbox; filter to `owning_board == board`; ask the planner
   for the board's groups (D3's routed document ops included).
2. Collect the dirty entity set (group targets + subsumed op targets).
3. Materialize `EntityEntry`s from published state (D4); sections
   payload per §6.1.
4. Emit one `BoardChunk` (`next_chunk_range`), `put_create`
   `taskboard/boards/<uuid>/ops/NNNN.json`.
5. **Account honestly**: every op in the chunk (primary + subsumed)
   reports `PushOutcome::Applied { echo: None }` **only after the
   chunk PUT returned success**; a failed/blocked PUT leaves every op
   queued (`PushResult::Rejected { kind }` retryable). A pre-emptive
   Applied is data loss under crash — this ordering is a named review
   trap, and the crash-window test (§9.4) pins it.
6. Snapshot/GC duty per D5.

### 4.2 Pull

1. `PROPFIND` Depth:1 over `taskboard/boards/<uuid>/` (transport
   extension: a `propfind_tree` style listing call returning members
   with etags — F6). `layout::classify` partitions members into
   identity / info / snapshots / ops / foreign.
2. **Watermark**: keep a per-board ingest cursor
   `(per-device last ingested seq)` in local storage (migration, §10).
   Fetch any snapshot set per `late_join_floor(snapshots, chunks)`
   (first cycle = floor; later = chunks with `seq > watermark`), then
   only changed members thereafter (etag compare — F5: redundant
   re-GETs are harmless, the fold is idempotent, so the cursor is an
   optimization, not a correctness load-bearer).
3. Decode tolerantly (chunk 4's codecs; unknown rows → `Verbatim`,
   format-too-new → typed park + status finding, never an error).
4. `fold_board(entries) -> BoardState` — chunk 4's fold, seq never
   orders merges.
5. Report (4.3).

Loop/edge semantics inherit the per-user cycle's: fork alarm on an
unexpected 412 on an own path, quarantine posture for garbage, typed
`CycleVerdict` per board lane.

### 4.3 The Files-tagged report

`FilesBoardSnapshot` (today identity-only, `domain/remote.rs:178`)
grows to carry the folded board:

```rust
Files(FilesBoardSnapshot {
    board: BoardId,
    up_to_seq: u64,            // watermark the fold reached
    entities: Vec<MaterializedEntity>,   // the folded board state
    sections: BoardSections,   // §6.1
    read_at, ...               // report timing fields as Deck's
})
```

(Exact field shapes at implementation; the invariant that matters:
**it carries materialized entity states, not ops and not a
pre-merged verdict the engine must swallow whole**.)

### 4.4 Echo semantics

Files remote ids **are** the domain UUIDs (chunk 3), so a files push
can never rename an identity: `PushResult::Applied { echo: None }` —
no `RemoteEcho` variant is added. `DeleteRemoteCard/DeleteRemoteStack`
never occur for files-boards (they are Deck-ref-delete ops; their
`owning_board` routing to Files is a plan-shape impossibility pinned
in §9.5).

### 4.5 Ingestion (the state engine)

The `RemoteBoardSnapshot::Files` arm (`actor.rs:1074`) grows into real
ingestion **through the existing pipeline**: decode each
`MaterializedEntity` back to its domain entity via chunk 4's
`to_task/to_stack/to_label/to_board` (fields + clocks + authors
intact), then run the **same** per-field LWW merge machinery
`apply_sync_report` uses for Deck snapshots (pipeline.rs:446) — the
duel order `(clock, author)` from chunk 1 decides. A second merge
implementation gated on the Files tag is the named review trap (a);
grep-bait: any `if files` fork around a merge call.

Tombstones ride the same path (the `deleted` field duels like any
field). The engine's Files arm updates the binding-derived ownership
facts exactly as the Deck arm does (validators keyed to the binding).

## 5. Board sections (D-BF9's second leg)

**Decision: the board-level sections ride the board document as
`BoardState`-level section payload, mirroring the sidecar's wire
names and row codecs 1:1.**

- **Rides**: `"time-entries"` (`TimeEntryRecord`), `"habits"`
  (`HabitRecord`), `"habit-log"` (`HabitLogRecord`), `"break-rules"`
  (`BreakRuleEntry`) — board-level in the sidecar today
  (appdata.rs:81–136) — plus `"reminder-rules"` and `"dismissals"`
  which are **card-keyed in the sidecar but board documents here key
  by domain task UUID** (there are no remote card ids on files —
  chunk 3's ruling makes the key type collapse natural).
- **Keying**: section rows keep their existing per-row sync identity
  (entry ids/keys as the sidecar uses them); clocks ride in the rows
  exactly as the sidecar's union merges them today.
- **Fold semantics**: section rows fold by the **sidecar's existing
  union machinery** (id-keyed set-union with per-row clock duels),
  re-used from domain — not a second union implementation. The rows
  ride `BoardState`'s verbatim lane (chunk 4's `Verbatim` relay is
  exactly the tolerance hook) wrapped as named section entries; the
  tolerant unknown-section flatten is already chunk 4 behavior.
- **Retention**: the 180-day retention constants
  (`TRACKING_SYNC_RETENTION_DAYS` :2032, `HABIT_LOG_SYNC_RETENTION_DAYS`
  :2193) apply to the board copies identically (the owner prunes at
  snapshot time; a pruning that touches another device's rows is a
  finding).
- **Mixed-kind law**: sections never appear in the per-user root and
  per-user sections (`journal`/`settings`/`proofs`) never appear in a
  board document — pinned both directions (§9.7).

## 6. App-only fields are ordinary chunk fields (D-BF9's third leg)

The sidecar's per-card `"tasks"` section (app-only task fields:
reminder-state bits, tracking flags, per-card dismissals) collapses
into the task entity on files-boards:

**Decision: extend chunk 4's `TASK_FIELD_SLOTS` additively** with the
app-only field slots (exact slot list at implementation, mirroring the
sidecar's per-card record fields), with tolerant decode — a peer on a
pre-extension format decodes the known slots and relays the rest
`Verbatim` (chunk 4's recipe, unchanged semantics). One lane, one
materialization: a task's reminder/tracking state syncs because it is
part of the entity, not because an attachment was flushed. The
`from_task`/`to_task` constructors grow the slots; old chunks
(authored by chunk-4-era code in tests/mixed fleets) decode fine.

## 7. Adoption & standalone boards (D-BF10's tail)

### 7.1 Binding minting for files-boards

Chunk 3's law: config never assigns boards; bindings persist from
remote facts. Files-boards have no remote echo to mint a binding — so:

**Decision: the binding mints locally at adoption, from two doors.**
1. **Opt-in door**: a typed user command
   (`boards enable-files-roaming <board>` surface at stage 2; CLI/UI
   verb naming follows the house connector-verb style) — a device with
   a files connector binds a binding-less board to it; the command
   creates the board's subtree (MKCOL + first snapshot = the current
   converged state, the chunk-6 ceremony's step-2 shape in miniature).
   A board already bound to a Deck connector gets the chunk-3 typed
   refusal (the second-claim journey machinery, reused verbatim).
2. **Discovery door**: a device with a files connector that finds a
   foreign `taskboard/boards/<uuid>/` subtree (its device.json
   identity matches, no local binding) **adopts fold-first** — the
   DR-8 journal precedent (D-SF13): pull everything, fold, ingest,
   *then* mint the local binding and start pushing. Before the fold
   completes, the device pushes nothing (publish-after-import
   ordering, pinned).

Partition invariant throughout: `owner_of` never returns two owners —
the minting doors both run through the existing second-claim guard.

### 7.2 The standalone flow

A board that has never known Deck (created standalone, files connector
enabled) is the opt-in door with an empty prior state. Acceptance's
"standalone-board adoption journey" is door 1 exercised against the
fake transport with a second device adopting via door 2.

## 8. Wiring

**Decision: no new `ConnectorModule`.** The files actor already has
its own bootstrap gate (`files_sync_spawn`, bootstrap.rs:193) outside
the Deck registry, spawned on `[files_sync] enabled` + the default
connector being Nextcloud (chunk-1-era ruling). Chunk 5: the spawned
actor gains the board lane; the bootstrap boot-guard
(`OrphanedConnectorBinding` check) extends naturally — a persisted
files binding with no files connector enabled surfaces typed at boot
(the deck-side orphan case already covered; the files-side orphan is
symmetric and loud). Config: no new keys in this chunk (poll cadence
inherited; the boards the actor serves are read from the ownership
map, never from config — chunk 3's law).

## 9. Test catalogue (TDD order)

New tests are written failing-first in this order; each names the
decision it pins.

1. **Plan-shape routing pins** (`taskboard-domain`, planner unit):
   (a) Files-owned board: `UpdateAppData`/`FlushBoardAppData` join the
   dirty-entity bucket; no `PushTarget::Document` group exists for a
   Files board; (b) Deck-owned board: plan output byte-identical to
   today (the golden's planner-level twin); (c) unowned board:
   document ops stay queued.
2. **Drain honesty** (`sync-files-nextcloud`, fake transport): ops
   report Applied only post-PUT; a failing `put_create` leaves every
   op queued; crash between PUT-success and accounting re-pushes the
   same chunk safely (create-once 412 → reconcile, ops still Applied
   exactly once — the idempotence pin).
3. **Chunk materialization**: from_task/from_stack/from_label/
   from_board round-trips with clocks+authors; deleted entities
   materialize as tombstones; seq ranges from `next_chunk_range`.
4. **Pull fold + watermark**: late-join floor on first cycle;
   incremental chunks after; etag-unchanged members skipped; unknown
   entity rows relay Verbatim; format-too-new parks typed.
5. **The no-sidecar pins** (`taskboard-app`, fake transport):
   (a) a files-board never creates an anchor card — there is no Deck
   actor in the journey at all and the pin is plan-shape (test 1a
   makes it fail-able); (b) no `FlushAppData` group is ever executed
   for a Files board; (c) `UpdateAppData` drains via chunk fields —
   the app-only field arrives on the peer.
6. **Deck regression pins**: `s_deck_full_sync_golden`
   (snapshots.rs:2579) and the second-claim journey (engine.rs:4276)
   pass **untouched**; if any Deck-side assertion needs weakening, the
   chunk is wrong.
7. **The two-engine convergence journey** (`taskboard-app/tests/
   files_sync_convergence.rs` template): two full engines, one shared
   `Arc<FakeFilesTransport>`; concurrent edits to the same task from
   both engines (different authors — chunk 1 legs visible in the
   winner choice) converge to one board state on both, then the fleet
   goes quiet; sections (time entries + a habit log) converge
   likewise; outboxes empty; tombstone delete on one engine stays
   dead on the other (the resurrection scenario).
8. **Section isolation**: board sections never reach the per-user
   root; `journal`/`settings`/`proofs` never reach a board document
   (typed rejection both ways).
9. **Ingestion** (`taskboard-state`): the Files arm merges through the
   existing pipeline — a state-level test feeds a Files snapshot and
   asserts the merged `AppState` (snapshot test, insta, per the house
   StateEngine strategy); a second-merge-implementation would fail
   the clock/author duel assertions.
10. **Adoption journeys**: door-1 opt-in (binding mints, refusal on a
    Deck-bound board reuses the typed refusal); door-2 discovery
    fold-first (push suppressed until the fold completes — pinned by
    asserting no local chunk exists pre-fold).
11. **Docker tier** (`taskboard-app/tests/`, `#[ignore]`-gated,
    existing conventions): `it_files_docker_board_*` — the board cycle
    against the dockerized Nextcloud (real WebDAV: create-once
    chunks, If-Match snapshots, PROPFIND listing), mirroring the
    `it_nextcloud_docker_*` pattern (env `TASKBOARD_IT_DOCKER_*`).
12. **Live Tier-3 legs** ride the `.env` convention
    (`TASKBOARD_IT_NEXTCLOUD_*`), re-pinning F3/F4/F6 for the board
    subtree; ignorable per ADR 0008.

Property tests (proptest): the drain is confluent — the same dirty set
materializes identically regardless of op arrival order (pure
materialization from state); watermark/floor equivalence is chunk 4's
already-protested law (referenced, not re-tested).

## 10. Storage notes

One migration (number re-verified at base; `0024` expected): the
per-board pull watermark table `(board, device_id) -> last_seq` (§4.2)
plus, if the actor's per-board chunk counter isn't derivable from
`ops/` listing + local last-published, the `chunks_since_snapshot`
counter. Nothing else migrates — chunk 3 already widened the remote
identity columns.

## 11. Risks & mitigations

- **The merge-pipeline fork trap** (review trap (a)): mitigated by
  test 9's duel assertions — a forked merger with seq-ordered or
  author-blind duels fails them.
- **Outbox accounting under crash** (trap (e)): test 2's crash window;
  the create-once 412 reconcile path is the designed recovery.
- **Sections double-implemented** (sidecar union vs a new board
  union): §5 mandates reuse; review greps for a second union.
- **Slot-extension wire risk**: additive + tolerant decode per chunk
  4's recipe; mixed-fleet decode test (old chunk w/o new slots).
- **Chunk 5 is L** (design §6 names the split option): see §12.
- **Plan signature churn**: `plan_pushes` gains one parameter; all
  call sites are the two connector actors + tests; the Deck golden
  holds the line.

## 12. The split proposal (user rules, not this agent)

Part E splits naturally into two independently reviewable PRs, both on
stacked branches off `feat/boards-over-files` (or sequential — the
user chooses; dispatch mechanics say one PR per chunk by default, so
**this is a proposal to deviate**, per dispatch §1):

- **PR-A — drain/report** (D-BF8 + D-BF9's report leg): D1–D5, §4,
  the no-sidecar routing rule, tests 1–6, 9; entity payloads carry
  existing chunk-4 slots only. The two-engine journey runs with
  entity-level data.
- **PR-B — sections & adoption** (D-BF9's sections leg + D-BF10's
  tail): §5, §6, §7, tests 5c, 7 (section legs), 8, 10, 11–12;
  the slot extension and the migration ride here.

Rationale: PR-A is the risky core (routing, honesty, merge reuse) and
reviewable alone; PR-B is additive payload surface. Cost of the split:
one extra gate cycle; the interim state (PR-A alone) is coherent —
boards sync entity data, sections follow. **Default if the user
declines to split**: one PR, this plan's whole scope, in TDD order.

## 13. Estimated size

L, as dispatched: ~2,500–4,000 changed lines including tests
(planner ~150, files actor board-lane ~600, state ingestion ~250,
transport extension ~100, slots/sections codecs ~400, storage
migration ~100, wiring ~150; the remainder tests across the six
suites). Unsplit: one PR at the top of that range.

## 14. References

- Design plan §3 Part E, §5, §6; dispatch plan §6; issue #688.
- DR-8 design round Parts C–E (conditional PUTs, change detection,
  engine seam); WebDAV characterization F3–F6.
- Chunk-4 stage-1 plan (`.artifacts/plans/2026-11-06-*board-chunk*`
  lineage — the lifecycle constants and laws consumed, not changed).
- AGENTS §2 crate boundaries, §5 testing directives, §10 gates.
