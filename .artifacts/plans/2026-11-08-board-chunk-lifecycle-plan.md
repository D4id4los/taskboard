# [Plan] Board Chunk & Snapshot Lifecycle — DR-9 chunk 4 (#687)

- **Date**: 2026-11-08
- **Kind**: implementation plan (stage-1 of the three-stage dispatch;
  `.artifacts/plans/2026-11-07-boards-over-files-implementation-dispatch-plan.md` §5).
- **Branch**: `feat/board-chunk-lifecycle` (worktree
  `../taskboard-WT-feat-board-chunk-lifecycle`, cut off `origin/main` @
  `94072694`).
- **Scope**: D-BF6–D-BF7 (design round §3 Part D, ruling Q6) — the core
  crate (`taskboard-sync-files`) grows the board lifecycle: tree
  layout, chunk/snapshot codecs, the board fold, the snapshot cadence,
  the GC predicate, the late-join floor. **No IO in this crate**; the
  transport crates are untouched except the core's exports (chunk 5
  drives them).
- **Preconditions verified 2026-11-08 on `origin/main`**: chunk 1
  (`b064182e`, `790a3513` — author structs + `au_` columns), chunk 3
  (`94072694` — tagged remote identities + partition routing), #632
  CLOSED (B-min hardening, the status vocabulary chunk 6 surfaces
  join). All green.
- **Normative lineage**: design round §3 Part D + §5; issue #687; the
  backbone report §5.5 (the chunk/snapshot mechanism) + §7.2 (the
  layout, file schemas, GC rules) + §7.5 (the worked
  deletion/resurrection scenarios); DR-8's D-SF2 (the reserved kinds
  this chunk activates: `ops/*.json`, `snapshot-*.json`, tolerated
  opaque since the substrate landed).

## 1. What lands (summary)

Pure, IO-free machinery in `taskboard-sync-files`:

1. **Layout**: `boards/` subtree arms in `layout.rs`'s taxonomy —
   board identity, per-(board, device) info, ops chunks, snapshots —
   with path builders that compose with `classify` (the existing
   test discipline).
2. **Codecs**: `board identity.json`, board `info.json` (with
   `last_seq`), the immutable chunk, the snapshot. Strict envelopes,
   lenient content, structural relay — the `document.rs` recipe.
3. **The materialized-entity payload schema** (§3 below): fields +
   clocks + authors per entity, never raw op references.
4. **The board fold**: one generic per-field max over
   `(rank, clock, author, content)` — the `fold.rs` discipline
   applied to entity payloads; **never seq-ordered** (the backbone's
   named design error).
5. **Lifecycle constants + pure decisions**: snapshot cadence, the
   GC predicate (owner-scoped ∧ watermark-bounded; tombstones never),
   the late-join floor algorithm.
6. **Tests**: the property catalogue (§7) and test catalogue (§8),
   TDD — failing tests first.

Non-goals (per #687): the transport cycle/routing (chunk 5), the
Deck→files ceremony (chunk 6), per-user section changes (the root's
`DeviceInfo`, `UserDocument`, `fold`, settings/proofs are untouched).

## 2. The tree layout

```
<synced-root>/taskboard/
├── identity.json                          # per-user root — UNCHANGED
├── devices/<device-id>/                   # per-user current-state — UNCHANGED
│   ├── info.json                          #   DeviceInfo: NO last_seq (the root
│   └── appdata.json                       #   stays current-state; seq is a
│                                          #   board-lane GC watermark only)
└── boards/
    └── <board-uuid>/                      # per-board lifecycle, independent
        ├── identity.json                  # BoardIdentity (create-once)
        └── devices/<device-id>/
            ├── info.json                  # BoardInfo: heartbeat + last_seq
            ├── ops/
            │   ├── 000001.json           # immutable BoardChunk
            │   └── 000002.json
            └── snapshot-000016.json      # BoardSnapshot (owner's fold ≤ 16)
```

Mixed kinds **across** subtrees, never within one: `boards/<uuid>/`
mirrors the root's `devices/` grammar one level down, and `classify`
routes purely by the `boards/` prefix — a board file can never
deguise itself as a root file or vice versa. The board id component
parses as the domain's `BoardId` (uuid); the device component parses
as the existing `DeviceId` (both are strict: an unparsable component
classifies `Other` — relayed opaque, never an error).

### 2.1 New `layout.rs` surface

Constants: `BOARDS_DIR_NAME: &str = "boards"`. Builders (six-digit
normalized emission, all-digit-stem lenient decode — the existing
`classify_reserved_file` convention):

```rust
pub fn board_identity_path(board: BoardId) -> String;
pub fn board_device_info_path(board: BoardId, device: DeviceId) -> String;
pub fn board_ops_path(board: BoardId, device: DeviceId, seq: u64) -> String;
pub fn board_snapshot_path(board: BoardId, device: DeviceId, seq: u64) -> String;
```

New `DeviceFileKind` arms (the enum is the transport's router; the
name stays — it classifies the whole substrate tree):

```rust
BoardIdentity { board: BoardId },
BoardInfo    { board: BoardId, device: DeviceId },
BoardOps     { board: BoardId, device: DeviceId, seq: u64 },
BoardSnapshot{ board: BoardId, device: DeviceId, seq: u64 },
```

`classify` match arms: `[BOARDS_DIR_NAME, board, IDENTITY_FILE_NAME]`;
`[BOARDS_DIR_NAME, board, DEVICES_DIR_NAME, device, INFO_FILE_NAME]`;
`[BOARDS_DIR_NAME, board, DEVICES_DIR_NAME, device, OPS_DIR_NAME, file]`;
`[BOARDS_DIR_NAME, board, DEVICES_DIR_NAME, device, file]` with the
`snapshot-` prefix. Everything else under `boards/` stays `Other`.
The `.tmp-` temporary rule applies unchanged (checked first, as
today). Root arms and their tests are untouched.

## 3. The materialized-entity payload schema (normative)

The backbone §7.2 chunk shape, adapted to the domain's clock
families (D-BF1's slot lists). One uniform wire shape for all four
entity kinds and for both carriers (chunk entries and snapshot
state):

```json
{
  "kind": "task",                       // "board" | "stack" | "task" | "label"
  "id": "018f6a44-…",
  "fields": {
    "title":    { "value": "Ship v0.1", "clock": "2026-11-11T09:15:31Z",
                  "by": "018f6a52-…" },
    "position": { "value": { "stack": "…", "order": 3 },
                  "clock": "…", "by": "…" },
    "deleted":  { "value": false, "clock": "…", "by": "…" }
  }
}
```

### 3.1 Field slots per kind (pinned, slot-for-slot with the clock families)

| kind | field slots (`TaskClocks`/`StackClocks`/`LabelClocks`/`BoardClocks` exact) |
|---|---|
| `board` | `title`, `color`, `archived`, `deleted` |
| `stack` | `title`, `order`, `deleted` — plus the non-clocked residual `archived`, which rides `order`'s clock/author (exactly the reconcile merge's pairing) |
| `task` | `title`, `description`, `duedate`, `scheduled`, `done`, `position`, `effort`, `importance`, `urgency`, `recur`, `kind`, `archived`, `deleted` |
| `label` | `title`, `color`, `deleted` |

Non-clocked companions:

- **`position`** is one field: `value = { "stack": uuid, "order": i64 }`,
  one clock, one author (the domain's composite-position rule).
- **`labels`** (task only, outside the duel fields): the label
  membership OR-set map serialized as
  `{ "<label-uuid>": { "added": true|false, "clock": …, "by": … } }`
  (the `MembershipState` vocabulary); folded per membership key by its
  own `(clock, by)` — per-membership LWW, the OR-set semantics #129
  owns. Absent = no memberships ever referenced.
- Value payloads are plain JSON: `Color`/`Effort`/`Importance`/
  `Urgency`/`TaskKind`/`RecurRule` ride their existing serde forms
  (all decode-lenient or strict-typed like the storage codec's wire
  vocabulary; an unmodelable value demotes the *field* to the unknown
  relay, never the chunk).

### 3.2 The `FieldValue` wire grammar

- `{ "value": <json>, "clock": <rfc3339>, "by": <uuid>? }` — `by`
  present iff `Some(AuthorId)` (v1 emission writes the key only for
  `Some`; the chunk-1 stamp fills it whenever a device identity
  exists).
- Decode: absent `by`/`null` → `None` (tolerant); present-but-
  unparseable `by` or non-RFC-3339 `clock` → the field demotes to
  `UnknownFieldValue(raw)` — **fail-closed** (silently de-authoring
  would change the fold rank), the `UserRecord` author rule applied
  per field.
- Unknown field *names* inside `fields` decode as unknown relays
  (carried, republished by snapshots — the self-healing rule), never
  folded, never an error.
- Unknown entity `kind`s decode as whole-entity relays (carried,
  republished), never errors: a newer peer's entity kind survives an
  older reader.
- A chunk entity that is not an object, or lacks `kind`/`id`,
  demotes to a whole-entity verbatim relay. A missing `id` is the one
  shape that cannot relay meaningfully — it rides the entry-list
  verbatim arm (carried at the chunk level, never folded).

**Never in a payload**: raw op references, `remote` bindings,
`remote_seen` — connector-local bookkeeping the materializer strips
at encode time (files-board remote identities are the domain UUIDs,
D-BF4; there is nothing to rebind).

### 3.3 Rust types (`board_payload.rs`)

```rust
pub enum EntityKind { Board, Stack, Task, Label }        // strict serde ("task" etc.)
pub struct FieldValue { value: Value, clock: DateTime<Utc>, by: Option<AuthorId> }
pub enum Field { Value(FieldValue), Unknown(Value) }     // fail-closed demotion
pub struct MaterializedEntity {
    kind: EntityKind,
    id: Uuid,                                            // plain uuid: unknown-but-valid ids relay
    fields: BTreeMap<String, Field>,
    memberships: Option<BTreeMap<String, MembershipRecord>>, // task only
    unknown: BTreeMap<String, Value>,                    // unknown entity-level keys relay
}
pub struct MembershipRecord { added: bool, clock: DateTime<Utc>, by: Option<AuthorId> }
```

Encode helpers (`from_task(&Task) -> MaterializedEntity` and
siblings) strip remote/remote_seen and read the clock+author families
slot-for-slot; decode helpers (`to_task(&self) -> Option<Task>` —
`None` for unknown kinds/relays) exist so chunk 5's ingest arm and the
fold share one mapping. The slot lists are pinned as data (a test
asserts the emitted slot-name set equals the documented list per kind
— the anti-drift tripwire against `entities.rs`).

## 4. File codecs

All in `taskboard-sync-files`; every codec follows the `document.rs`
posture — **strict envelope** (the required keys, integer
`format_version`, typed failure otherwise), **extras ignored** on the
never-republished identity/info payloads, **structural relay** on the
republished chunk/snapshot payloads. Version-blind decode (the
consumer gates, never the codec).

### 4.1 `board identity.json` — `BoardIdentity` (`board_identity.rs`)

```json
{ "board_id": "<uuid>", "format_version": 1, "created_at": "<rfc3339>" }
```

Created once at board-subtree attach (adopt-if-present); `board_id`
mirrors the directory component (a mismatch is the *reader's* typed
finding, not a decode failure — the codec stays total). `TREE_FORMAT_VERSION`
is the version constant (shared with the root identity). Strict
envelope, extras ignored (never republished).

### 4.2 Board `info.json` — `BoardInfo` (`board_info.rs`, beside `heartbeat.rs`)

```json
{
  "app_version": "0.2.0",
  "format_version": 1,
  "last_seq": 141,
  "last_clock": "2026-11-11T09:15:32Z",
  "state_hash": "fnv1a64:…",
  "written_at": "2026-11-11T09:15:32Z"
}
```

Exactly the root `DeviceInfo` shape **plus `last_seq: u64`** — the GC
watermark and the late-join advertisement. The root `DeviceInfo` is
*not* touched (D-BF6: the per-user root stays current-state; `last_seq`
exists only where chunks exist). Strict envelope, extras ignored
(backwards-compat with the backbone's `instance_id` field rides the
extras rule). Refreshed by the transport on every chunk/snapshot
publish (chunk 5).

### 4.3 The chunk — `BoardChunk` (`board_chunk.rs`), immutable once published

```json
{
  "format_version": 1,
  "device_id": "018f6a52-…",
  "first_seq": 129,
  "last_seq": 141,
  "written_at": "…",
  "entities": [ { "kind": "task", "id": "…", "fields": { … } }, … ]
}
```

- Envelope: strict (`format_version`, `device_id`, `first_seq`,
  `last_seq`, `written_at`, `entities` required; `first_seq ≤
  last_seq` is a typed decode failure — a chunk that cannot name its
  watermark is unusable for GC).
- `entities`: lenient per §3.2 — garbage entity shapes relay, never
  kill the chunk (a torn-bytes chunk that still parses its envelope
  folds its good entities; the rest heals via the next snapshot).
- `device_id` redundancy: the path owns identity (the heartbeat
  precedent); the field is the clone-detection handle the backbone
  names — carried, equality-checked by the *reader* (chunk 5's lane),
  not by the codec.
- Immutability: the type has no mutation surface and the plan pins
  the review trap — any rewrite/rename path over a published `ops/`
  file is a finding.

### 4.4 The snapshot — `BoardSnapshot` (`board_snapshot.rs`)

```json
{
  "format_version": 1,
  "device_id": "018f6a52-…",
  "seq": 141,
  "written_at": "…",
  "state": { "entities": [ … ] }        // the owner's converged fold ≤ 141
}
```

Same entity grammar as chunks (`state.entities` is a `Vec<MaterializedEntity>`
— the fold output serialized, unknown relays included so snapshots
subsume and heal). `seq` is the snapshot's watermark: everything this
device has seen up to and including its own chunk 141. Publishing a
snapshot subsumes the device's prior chunks (self-healing) and arms
GC.

### 4.5 The state hash

`hash.rs` grows `StateHash::of_board(&BoardState)` reusing
`canonicalise` over the serialized fold (the same opaque-equality
token contract; `StateHash::of(&UserDocument)` untouched). The board
heartbeat advertises it; quiescence divergence detection (§5 of the
backbone) is chunk 5/6's surfacing.

## 5. The board fold, snapshot cadence, GC, late join

### 5.1 The fold (`board_fold.rs`)

```rust
pub struct BoardState { entities: BTreeMap<(EntityKind, Uuid), FoldedEntity> }
pub fn fold_board(entities: impl IntoIterator<Item = &MaterializedEntity>) -> BoardState;
```

- Pure, total, input-order-free — per `(kind, id)` the winner is the
  per-field max over **`(rank, clock, author, content)`** — rank:
  clocked `FieldValue` > `Unknown` relay (the verbatim floor, exactly
  `fold.rs`'s rank leg); author: `None < Some(_)` (the pinned
  D-BF2 leg); content: canonical-JSON bytes (the totaliser).
- **Sequence numbers never influence any merge outcome.** They are GC
  watermarks only (the backbone's conceptual subtlety, restated as a
  review trap: a fold that sorts by seq is the design error).
- Memberships fold per membership key by `(clock, by)`; unknown
  relays survive (republished by snapshots).
- The fold is attribution-free and idempotent (re-folding a snapshot
  changes nothing) — the laws chunk-4 proptests assert.
- Entity-vs-entity duels use this payload-level fold, **not**
  `domain::reconcile`'s `*_pair` merges (those are reconcile-specific:
  tie→bound, ref-stealing) and **not** `domain::merge`'s Deck adoption
  merges (remote-shaped inputs). The D-BF9 "reuse, not fork" rule
  binds chunk 5's *engine ingest* (files-tagged payloads flow through
  the state engine's existing per-field LWW pipeline); the substrate
  fold is the wire-level max, written once, generic over the four
  kinds — a per-kind fork of the merge would be a finding.

### 5.2 Constants and triggers (`board_gc.rs`, one home for the lifecycle numbers)

```rust
/// Snapshots publish on boot and after every N chunks (D-BF7).
pub const SNAPSHOT_EVERY_N_CHUNKS: u64 = 16;
pub const BOARD_CHUNK_FORMAT: u32 = 1;
```

- **Boot snapshot**: the first publish of a process's board session is
  a snapshot regardless of cadence (the backbone's self-healing and
  the move ceremony's "first snapshot = converged state" both lean on
  it). Pure decision helper: `snapshot_due(chunks_since_last_snapshot:
  u64, is_boot: bool) -> bool`.
- Chunk seq allocation is per-(board, device), monotonic, gap-free
  `first_seq = last_prev + 1` (pure helper
  `next_chunk_range(last_published: Option<u64>, entries: usize) ->
  (first, last)`); a chunk with zero entities is never published (no
  empty ops files).

### 5.3 GC rules (backbone §7.2, membership-free)

The **predicate** is pure core-crate code (execution is the
transport's, chunk 5):

```rust
/// Own chunks deletable once snapshot-<watermark> is published.
pub fn gc_eligible(own_chunk_seqs: &BTreeSet<u64>, published_watermark: Option<u64>) -> Vec<u64>;
```

- **Owner-scoped ∧ watermark-bounded**: only the device's own `ops/`
  seqs are ever inputs, and only seqs `≤` the *published* snapshot's
  `seq` are eligible. Either leg missing is a finding (pinned unit
  tests: a watermark of `None` deletes nothing; a foreign device's
  seq never appears — the API shape makes it unrepresentable).
- **Tombstones are forever**: deletion is *never* a tombstone GC — a
  tombstone is a field inside an entity payload and rides snapshots
  like any field; GC removes only whole ops files at/below the
  watermark. A horizon knob is documented future debt (backbone §7.2),
  not v1.
- Snapshots are never GC'd by this machinery (retirement of a device
  subtree is the user procedure the runbook owns).
- GC safety law (proptested): deleting `gc_eligible(...)` from any
  fold input set never changes the fold — every eligible chunk's
  content is subsumed by the published snapshot.

### 5.4 The late-join floor algorithm

```rust
pub struct DeviceReplay { pub floor_snapshot: Option<u64>, pub chunk_seqs: Vec<u64> }
/// Per device: the newest snapshot is the floor; chunks ≤ floor are
/// skipped (subsumed), chunks > floor replay in the fold.
pub fn late_join_floor(snapshots: &BTreeSet<u64>, chunks: &BTreeSet<u64>) -> DeviceReplay;
```

Global late join = for every device, fold `snapshot-<floor>` (if any)
⊕ that device's chunks `> floor` ⊕ every device's snapshots/chunks as
above — order-free, because the fold is. The **snapshot-floor
equivalence law** (the acceptance centerpiece, proptested):
`fold(snapshot ⊕ tail) == fold(all chunks)` over generated event
sets — the property *can fail* on a wrong floor (a floor above the
true snapshot seq skips live chunks; the generated sets include
multi-snapshot devices and interleaved deletes precisely so it does).

## 6. Mixed-kind tolerance rules

1. **Routing is the classifier**: `DeviceFileKind`'s board arms route
   board files to the board lane; root arms are unchanged. There is
   no path by which a board payload reaches `fold(&[UserDocument])`
   or vice versa — the types differ, the classifier differs; the
   reviewer trap ("a reader that folds a board subtree as
   current-state is a typed rejection") is satisfied by construction,
   pinned by a classify-vs-lane table test.
2. Unknown shapes *within* the board lane relay (§3.2) — the same
   structural-relay posture the root practises: never parse-and-choke,
   never delete another device's file, never error a document over
   one bad entity.
3. A board subtree without a parseable `boards/<uuid>/` component is
   `Other` (opaque relay) — a hand-created or misnamed directory is
   never parsed, never deleted.
4. Format negotiation: every file carries `format_version`; the
   codecs decode any value (version-blind), consumers gate. A newer
   *payload vocabulary* (unknown kinds/fields) relays; a newer *major
   envelope* is the consumer's typed finding (chunk 5's surfacing).

## 7. Property catalogue (proptest, TDD-first)

| # | Property | Failure it exists to catch |
|---|---|---|
| P1 | **Snapshot-floor equivalence**: for generated event sets (multi-device, interleaved edits/deletes, re-materializations), `fold_board(snapshot ⊕ chunks>floor) == fold_board(all chunks)` | a wrong floor / snapshot that drops a field relay |
| P2 | **GC safety**: `fold(all) == fold(all \ gc_eligible(...))` for generated chunk sets + watermarks | GC removing live data; watermark above/below the published snapshot |
| P3 | **Fold order-freedom**: any permutation of the input entity set folds identically (incl. state hash) | seq-order or attribution leakage into outcomes |
| P4 | **Fold idempotence + associativity** (snapshot refold law) | a non-idempotent fold breaks snapshot subsumption |
| P5 | **Tombstone duels**: delete beats older edit, loses to newer; survives permutation | resurrection-by-fold-order |
| P6 | **Author leg**: `Some(by)` outranks `None` at equal clocks, deterministically; the F1 stamp round-trips through the payload | silently dropped/ignored authors |
| P7 | **Codec roundtrip identity** over generated payloads/chunks/snapshots; emission is v1-clean (`by` present iff `Some`; no raw-op or remote keys) | codec drift |
| P8 | **Fuzz never-panic**: arbitrary strings/values/bytes through every codec and `classify` — total, typed, never a panic | parser panics (the chunk-codec fuzz trigger) |
| P9 | **Resurrection scenarios** (backbone §7.5, generated where possible): late-returning device (months offline, peers' tombstones intact → delete survives), concurrent delete-vs-edit duel, folder fork (duplicate roots merge by set-union) | tombstone GC, non-convergent duels |

## 8. Test catalogue (unit + property, per module)

- `layout.rs` (+tests): board-arm classification table (builders
  compose with `classify`), `boards/` misfits are `Other`, `.tmp-`
  rule over board paths, the never-panic proptest extended, the
  lane-routing table test (rule 6.1).
- `board_identity.rs`: wire shape, strict envelope failures, extras
  ignored, roundtrip.
- `board_info.rs`: the `last_seq` growth vs root `DeviceInfo`
  (unchanged, pinned), strict envelope, extras tolerated.
- `board_payload.rs`: slot tables per kind (the anti-drift data
  pins), encode/decode from/to domain entities, `by` fail-closed
  demotion, unknown field/kind relays, memberships roundtrip +
  per-membership fold, P6–P8.
- `board_chunk.rs` / `board_snapshot.rs`: envelope strictness
  (incl. `first_seq ≤ last_seq`), lenient entity lists (garbage
  relays), roundtrip, immutability surface (no `&mut` paths —
  compile-time).
- `board_fold.rs`: P1, P3–P5, P9; unknown-relay survival through
  snapshots.
- `board_gc.rs`: constants pinned; `snapshot_due` truth table;
  `next_chunk_range` monotonicity (no gaps, no empty chunks);
  `gc_eligible` owner/watermark pins (`None` deletes nothing,
  watermark bounds, foreign input unrepresentable); P2.
- Integration-flavoured (still pure): a generated board session
  (events → chunks → snapshot → GC → late join) ends in the same
  `BoardState` as the full replay — the end-to-end law tying P1+P2.

TDD order: layout → codecs → payload schema → fold laws → GC →
late-join/floor → the fuzz + resurrection set. Failing test first at
each step; `scripts/run-quality-gates.sh` green before handoff.

## 9. Bookkeeping & handoff

- CHANGELOG `Unreleased` entry (the board lifecycle lands behind
  chunk 5's wiring — noted as substrate).
- ADR 0022 already covers the contract (chunk 1); no new ADR. New
  out-of-scope discoveries → `backlog:` issues (standing
  authorization).
- Exports for chunk 5: everything above is `pub` in
  `taskboard-sync-files`; the transport crates otherwise untouched.
- Commit style: `<type>(<module>): <short-desc>`, every commit
  standalone; no push, no PR (stage-2/3 protocol owns that).

## 10. Risks

- **Payload/schema drift vs `entities.rs`**: the slot tables are
  pinned as data (§8) so a clock-family change that outpaces the
  payload fails a test, not a peer device.
- **"Reuse, not fork" misreading**: the substrate fold is new wire-
  level code by design; the rule binds chunk 5's engine ingest. The
  plan states this explicitly (§5.1) so the stage-3 review doesn't
  false-positive, and so a genuine second merge pipeline *would* be
  flagged.
- **`by` fail-closed vs data loss**: a present-but-unparseable author
  demotes the field to an unfolder relay — deliberate (rank
  honesty over silent de-authoring); the value heals via the
  owner's next snapshot. Documented leniency at the type.
- **Seq misuse**: the watermark-only rule is restated in three places
  (backbone, §5.1, review traps) because it is the named design
  error; P3 makes it fail loudly.
- **Estimated size**: core crate ≈ 6 new/extended modules, roughly
  **1,600–2,200 LOC including tests**; domain/transport crates
  untouched; CHANGELOG + this plan are the only non-Rust diffs.
