# [Plan] Amendment: cross-device adoption of a moved board (chunk 6, D-BF11)

- **Status**: DRAFT, awaiting user review. No implementation until approved.
- **Amends**: `2026-11-08-deck-to-files-move-plan.md` §2 (step 2/4, undo), §3, §4, §5, §7, §9.
  Also lifts that plan's "no state-engine additions" and "no new taxonomy" limits.
- **Branch**: `feat/deck-to-files-move`, on top of `862d60ed` (the moving-device ceremony).
- **Why**: the stage-2 finding on #689. §4's detection cannot fire, ids are per-device
  on Deck boards, and publishing makes other devices mint a duplicate board.

## 0. Verified anchors (against `862d60ed`)

| Fact | Anchor |
|---|---|
| The ownership map folds only local `Board.remote` rows | `domain/src/ownership.rs` |
| Deck pull mints ids per device | `domain/src/pipeline.rs:547`, `:949` (`ids.new_board_id/new_task_id`) |
| Files ingest keys rows by the published UUIDs; an unmatched subtree mints a board | `pipeline.rs:1773-1860` |
| Discovery mints without reading the identity | `board_cycle.rs` `discover_subtrees` |
| `identity.json` readers ignore extra keys | `sync-files/src/board_identity.rs` (`tolerates_unknown_extra_keys`) |
| Published entities strip Deck refs | `board_payload.rs` `from_task` |
| Snapshots are replaced over time; chunks are GC-shaped | `board_gc.rs` |
| Unknown files in the tree are relayed, never errors | `layout.rs` `classify` (`Other`) |
| A Deck ref-delete op stays queued forever on a files board | `board_cycle.rs` `a_ref_delete_op_on_a_files_board_stays_queued` |
| Id foreign keys that adoption must rewrite | §3 below |

## 1. Changes from the proposal you approved

The proposal was: a `moved_from` marker in `identity.json`, a Deck-ref mapping in the
snapshot, an engine re-key command, and discovery skipping marked subtrees. Two changes:

1. **The mapping moves from the snapshot to a create-once commit record.** Snapshots
   get replaced, so a device offline for long enough could lose the mapping.
2. **The marker alone is not enough; adoption needs a commit signal.** The original
   plan's §9 claim that "non-mover devices have nothing to adopt" during the undo window
   stops being true once a marker exists. Other devices would adopt a move that the
   mover may still undo. The commit record is that signal.

## 2. Wire additions (additive; format version unchanged)

**`boards/<U>/identity.json` gains `moved_from`** (written at step 2, create-once):

```json
"moved_from": { "deck_server": "https://cloud.example", "deck_board": 42 }
```

It means "this subtree replaces that Deck board; do not mint a separate board from it".
`deck_server` is the normalized origin of the Deck connector, because Deck board ids are
per-server. The files tier can be a different server in a multi-connector setup.

**`boards/<U>/move.json`, the commit record** (create-once, never rewritten):

```json
{ "format_version": 1, "deck_server": "...", "deck_board": 42,
  "moved_by": "<device>", "committed_at": "...",
  "stacks": { "<deck stack id>": "<uuid>" },
  "cards":  { "<deck card id>":  "<uuid>" },
  "labels": { "<deck label id>": "<uuid>" } }
```

Typed `MoveRecord` in `taskboard-sync-files`, following the A1 pattern: `deck_board`
and the maps decode through a fallible constructor. A malformed record is a typed
finding and is never adopted.

## 3. Mover changes (amends plan §2–§3)

Step 4 splits, and the stage machine gains one state:

| Stage | World |
|---|---|
| `NotStarted` | Deck-bound, no floor |
| `Published` | Deck-bound, floor present |
| `DeckRetired` | unbound, floor present, no `move.json` (**undo open**) |
| `CommitPublished` (new) | unbound, `move.json` present (**undo closed**, rerun only binds) |
| `Committed` | Files-bound |

- **Step 4a: PUT `move.json` create-once. This is the commit point.** The mapping is
  built from the kept Deck refs, so it must precede 4b, which wipes them.
- **Step 4b: `BindBoard` Files.** A crash between 4a and 4b resumes at 4b.
- **Undo window**: open until 4a succeeds. `--undo` probes for the record and refuses
  `UndoClosed` if it exists.
- **Step 2 always republishes while uncommitted**, overwriting this device's own floor
  (If-Match). This fixes a bug in `862d60ed`: a rerun after an undo, or after edits
  following a crash, currently reuses a stale floor. It replaces the plan's "never a
  re-publication with different content", which only needs to hold after commit.
- After an undo the marked subtree is inert: no commit record, so nobody adopts and
  nobody mints. Hand-deleting it becomes optional in the runbook.

## 4. Adoption on a returning device (replaces plan §4)

**Detection**, in the files lane each tick: for each subtree not bound locally, read
`identity.json` (once per etag).

| Identity | Local state | Action |
|---|---|---|
| no `moved_from` | any | today's discovery |
| `moved_from`, no `move.json` | any | nothing: move in flight or undone. Never mint |
| `moved_from` + `move.json` | live Deck binding matches server + board | **adopt** |
| `moved_from` + `move.json` | no match | today's discovery (a new device joining: the floor is its copy) |
| read failure or malformed record | any | typed finding, retry, never mint |

**Sequence** (the loud-then-fold-first ordering, pinned):

1. Set `FilesSyncStatus.moved_boards[board] = BoardMoveFinding::Adopting { moved_by, committed_at }`
   and `tracing::warn!`, before anything is written.
2. Send `SyncReport::BoardMoved { connector, board, remote, record }` on the existing
   report lane. The actor never writes state itself.
3. The engine applies one atomic batch (`plan_board_adoption`, a pure domain function):
   - The binding goes Deck → `Files(U)` in **one row rewrite**, so the map never shows
     two owners and is never empty.
   - Each row whose Deck ref is in the mapping takes the published UUID. B's board takes U.
   - Rows with no mapping entry (created offline) keep their ids and publish as new.
   - All Deck refs, baselines, Deck validators and Deck sync status are cleared.
   - The outbox is rewritten through the same map. `DeleteRemoteCard` and
     `DeleteRemoteStack` become `DeleteTask` / `DeleteStack` when mapped, otherwise they
     complete with a count, because they would otherwise stay queued forever.
4. The next tick runs the ordinary lane: pull and fold first, then drain, then the boot
   snapshot (publish-after-import). Status becomes `Adopted`.

**The boot snapshot is load-bearing.** If B reconnects and pushes to Deck before it sees
the record, the op completes and leaves no dirty entity. B's full-state boot snapshot
is what carries that edit into the files tier. This gets its own test.

**Re-key surface** (a new `PersistenceAction::RekeyBoard`, one transaction, foreign keys
deferred; no schema migration): `stacks.board`, `tasks.stack`, `labels.board`, task-label
memberships, reminder rules/deliveries/dismissals/snoozes, muted tasks, tracking entries,
calibration, habits, journal and undo rows, the outbox. The in-memory fold destructures
each row type without `..`, so a future id field fails to compile until handled.

**Refusals**, typed and loud, leaving the Deck binding intact and retrying:
`AdoptionCollision` (a target UUID already exists locally, e.g. a duplicate minted by
an older build), `MoveRecordMalformed`.

A Deck-404 tombstone keeps its behavior, as the original plan specified. A device whose
Deck report arrives after adoption has no holder, so the report is dropped (pinned).

## 5. Tests, TDD

Domain (`plan_board_adoption`):
- proptest: no dangling foreign key after a re-key, for arbitrary boards and mappings.
- Unmapped rows keep their ids; outbox ops are rewritten; ref-deletes translate.
- Collision refuses and changes nothing.
- The ownership map has exactly one owner before and after.

Engine (`insta` snapshot): adoption batch end state; a late Deck report is dropped.

Files crate: every row of the §4 detection table, including "never mint" for the
in-flight, undone and read-failure rows; the marker and record codecs; the ordering pin
that the finding precedes the report.

Ceremony: 4a/4b crash resume; undo refused once the record exists; republish after undo.

Two-engine journeys over the fake transport, plus a real-engine pin that `BindBoard`
and the record see the same refs:
- B offline with local edits: B's edits land on A's UUIDs, no duplicates, no tombstone,
  A and B converge.
- B pushed its edits to Deck before adopting: no duplicates, the edit reaches files.
- B created a card offline: it appears on A.
- A undoes: B neither adopts nor mints.

**Dual-tier Docker journey** (plan §5 legs 1–5, real Deck container + files tier), run
via the tier from `.env`. This is also the first live run of the production `AppWorld`.

## 6. Status, docs, bookkeeping

- `FilesSyncStatus.moved_boards` (`Adopting` / `Adopted` / `Refused(kind)`), and CLI `status` lines.
- Runbook: replace the "Current limit" block; add the adoption section, the commit
  point, and the §7 fleet preconditions. Update the CHANGELOG entry.
- The PR body carries "Closes #689" and "Closes #678" once the journeys pass.
- **Dropped from the original plan**: the "pending move" line on every `boards` verb. It
  needs a network probe per verb, and a rerun of `boards move` already diagnoses the state.

## 7. Known limits and risks

- **A device without files sync never sees the move.** It keeps syncing against the
  frozen Deck board. This is a silent ignore, which the hard rules forbid. See decision 1.
- **Older builds ignore the marker** and still mint a duplicate. Runbook precondition:
  upgrade every device before the first move.
- **Findings live in memory.** A one-shot process that adopts shows them only in its
  log. Durable findings would be a follow-up.
- **Re-key breadth is the main risk**: a missed foreign key is silent data damage, hence
  the compile-time destructuring and the proptest.
- **Size**: about 900–1,300 more lines, roughly half tests, likely 4–5 standalone commits.

## 8. Decisions for review

1. **Devices without files sync.** Recommended: the mover also posts a typed marker in
   the Deck board's sidecar document, which any Deck-only device turns into a loud
   `BoardMovedToFiles` finding (no adoption, because it has no files tier). The cost is a
   small Deck-lane change, which the original plan preferred to avoid. Alternative: a
   runbook precondition only, which leaves the silent case.
2. **Unbound local copy of the moved board** (the user ran `boards unbind` earlier).
   Recommended: no adoption, ordinary discovery, so it stays a separate board.
3. **Undo reachability.** The CLI runs retire straight into commit, so `--undo` is only
   reachable after a crash. Recommended: keep it; a deliberate pause would need a new flag.
