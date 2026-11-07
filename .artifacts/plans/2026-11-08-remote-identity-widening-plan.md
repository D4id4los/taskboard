# [Plan] Remote-identity widening & partition routing (DR-9 chunk 3, #686)

- **Date**: 2026-11-08
- **Kind**: stage-1 implementation plan for chunk 3 of the
  boards-over-files dispatch
  (`.artifacts/plans/2026-11-07-boards-over-files-implementation-dispatch-plan.md` §4,
  branch `feat/remote-identity-widening`).
- **Normative origins**: design round §3 Part C (D-BF4–D-BF5) + §5
  (`.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`);
  issue #686; the backbone report §7.4 — integration choice **(b)
  "widen the vocabulary… when committing" is the ruling** (Q5: no
  synthesized Deck-shaped ids); ADR 0010 (the connector vocabulary —
  `ConnectorId` mint-chain, `BindBoard` as the attach primitive — the
  widening promotes, never forks).
- **Precondition verified**: chunk 1 (F1 author clocks) landed on
  `origin/main` (`8c4971f5` and ancestors: `au_` columns, boot-time
  device identity, convergence folds); chunk 2 (settings/proofs,
  `0024_roaming_settings`) landed. Base for this branch: `origin/main`
  @ `8c4971f5`.
- **Non-goals (binding)**: files-board sync itself (chunk 5); the move
  ceremony (chunk 6); **any Deck pipeline behavior change** — the Deck
  path is regression-pinned to byte-equivalence; no binding-table or
  adoption machinery attached to the `Files` tag (files remote ids ARE
  the domain UUIDs); org anything.

## 0. Verified anchors (against base `8c4971f5`)

| Fact | Anchor | Evidence |
|---|---|---|
| The `Remote*` id family is four `u64` newtypes minted by a macro | domain `ids.rs` :124–137 | `RemoteBoardId`/`RemoteStackId`/`RemoteCardId`/`RemoteLabelId`, all `u64` |
| Board bindings **already** carry the connector tag | `ids.rs` :236–241 | `RemoteBoardBinding { connector: ConnectorId, board: RemoteBoardId }` (DF-3 mint-chain) |
| Child refs are connector-blind structural triples | `ids.rs` :246–270 | `RemoteCardRef{board,stack,card}`, `RemoteStackRef`, `RemoteLabelRef` |
| The report payload is Deck-structured today | domain `remote.rs` :23–127, `messages.rs` :861+ | `RemoteBoard{..id: RemoteBoardId}`, `RemoteBoardSnapshot`; `SyncReport::Completed { connector, snapshot: Box<RemoteBoardSnapshot>, validators, … }` already names its `connector` |
| The engine attributes reports by `(connector, remote board)` | state `actor.rs` :1217–1232 | `resolve_board(app, connector, remote)` matches the tagged binding |
| Storage persists Deck ids as INTEGER columns | migrations `0001_init.sql` :17–77 | `remote_board_id`/`remote_stack_id`/`remote_card_id`/`remote_label_id INTEGER NULL` + boards-only `connector TEXT NULL`; `remote_seen TEXT NULL` (a DateTime — tag-blind) |
| Storage decode already defaults a NULL connector tag | storage `repo.rs` :295–312, :410–414 | `decode::remote_board(&[row.remote_board_id], row.connector, default_connector)` |
| Conditional-read validators key on `RemoteBoardId` | domain `persistence.rs` :152+ | `ValidatorKey::{Boards, Stacks(RemoteBoardId), ArchivedStacks(RemoteBoardId)}`; serialized in the journal table |
| The Deck actor is today's **only** producer of these ids | sync-nextcloud `mapping.rs`/`model.rs`/`actor.rs`/`push_exec/*` | every mint is `RemoteXId(<u64 from Deck wire>)` |
| Validators are stored per (board-binding, key) with tag-blind values | storage `connect.rs`/`port.rs`, migration `0006` | values are opaque `SyncValidators{etag,last_modified}` strings |
| The registry is N-actor, config-minted | app `connectors.rs` :47–100 (`ConnectorSpec`, `configured_connectors`), `bootstrap.rs` | `sync.connectors` array (D5-2); the loop is the N-actor seam |
| A single live board + tagged binding is the state invariant today | ADR 0010 (`BindBoard`/`BoardAlreadyBound`), domain `command.rs` | rebinding to a *different* remote is `BoardAlreadyBound`; the *connector* dimension of that guard is what D-BF5 sharpens |
| Migration counter | `migrations/` | next free **0025** (0024 = roaming settings; 0023 = F1 `au_` columns) — re-verify at implementation time |

Fallout census: 836 occurrences of the four id names across ~75 files
(majority in `#[cfg(test)]` fixtures) — wide but mechanical; the plan
orders the work so the domain rewrite is small and everything else is
constructor-site churn.

## 1. The tagged type shape — **enum-of-variants, not generics** (D-BF4)

### Decision

The four `Remote*Id` newtypes become **plain enums with data**, minted
by hand (A1 pattern — no derive crates), one payload type per variant:

```rust
/// A remote board id in its connector's namespace. `Files` payloads are
/// the domain UUIDs — files-boards have no separate remote id space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RemoteBoardId {
    /// A Nextcloud Deck board id (unique per server).
    Deck(u64),
    /// The files substrate: the board's own domain UUID (`BoardId`'s
    /// payload). Identity, not a binding — chunk 5 consumes this.
    Files(crate::ids::Uuid),
}

pub enum RemoteStackId { Deck(u64), Files(crate::ids::Uuid) }
pub enum RemoteCardId { Deck(u64), Files(crate::ids::Uuid) }
pub enum RemoteLabelId { Deck(u64), Files(crate::ids::Uuid) }
```

(`Uuid` = the workspace's existing uuid newtype payload; exact spelling
settled at implementation against `ids.rs`.)

### Why enum, not generic (`RemoteBoardId<T>`)

- **Exhaustiveness is the design.** D-BF4's payoff the dispatch's
  review mandate names — "no wildcards, no `todo!()` anywhere the
  tagged type reaches" — is exactly what a closed enum buys; a generic
  leaves the compiler unable to force the Deck/Files fork at match
  sites, and the codebase would grow `PhantomData` noise instead of
  match arms.
- **Mixed collections are real.** A registry handles N connectors with
  different tags in one process; `RemoteIndex`'s `HashMap<RemoteCardRef,
  TaskId>` must hold both. A generic can't inhabit one collection
  without type erasure — which is the enum again, worse.
- **Serde/`sqlx`/storage text forms**: the enum serializes as a tagged
  representation with one clean text-form grammar (`deck:42`,
  `files:<uuid>`), mirroring the tolerant-additive house style chunk 1
  used for clocks. A generic would need a per-instantiation codec zoo.
- **Precedent**: the codebase's own verdict language ("connector-tagged
  (`Deck(...)` | `Files(...)`)") is the enum; D-BF1 chose parallel
  structs over packed fields for the same keep-the-arms-compiling
  reason.

### What does **not** get a tag

- **`RemoteBoardBinding` keeps `{ connector: ConnectorId, board:
  RemoteBoardId }` verbatim.** The connector dimension already lives
  there (DF-3); the enum's variant and the binding's connector are
  redundant-but-consistent (variant = namespace/shape, `ConnectorId` =
  which server). `BindBoard` validation adds the cross-check: a
  `Deck(...)` payload must ride a Deck connector, a `Files(...)`
  payload a files connector — a typed mismatch is a refusal (§4).
- **Child refs stay structural** (`RemoteCardRef{board, stack, card}`):
  remote child ids are only unique *within a board*, and every child's
  attribution already routes through its board's tagged binding
  (`resolve_board`, `RemoteIndex::from_state`). Tagging the triples
  would triple the churn for zero added invariant — the board tag
  transitively colors the children. `RemoteTask.stack`/
  `RemoteTask.labels` likewise stay bare: they are only meaningful
  inside the snapshot whose board carries the tag.
- **`SyncValidators`/`remote_seen`**: values are opaque tag-blind
  strings/timestamps. The *keys* follow the tag (via `ValidatorKey`'s
  `RemoteBoardId`); nothing else changes.
- **`DeckFeatures`** (`capabilities.rs`) stays Deck-specific — it is a
  capability probe of the Deck API, and files connectors will carry
  their own flag set in chunk 5 if they need one.

### The report payload generalization

`SyncReport::Completed.snapshot` becomes backend-tagged by making the
snapshot itself the enum — **not** by adding a tag field next to a
generic body:

```rust
/// The complete remote view of one board at sync time (completeness
/// contract unchanged; see the enum doc for the Files leg).
pub enum RemoteBoardSnapshot {
    /// Deck's full stacks/cards/labels view (today's struct verbatim —
    /// byte-identical serialization, see §6).
    Deck(DeckBoardSnapshot),
    /// The files substrate: identity-only in chunk 3. Chunk 5 grows
    /// this payload (folded board sections) — never chunk 3.
    Files(FilesBoardSnapshot),
}

pub struct DeckBoardSnapshot { /* today's RemoteBoardSnapshot fields, verbatim */ }
pub struct FilesBoardSnapshot { pub board: RemoteBoardId } // Files(_) by construction
```

Rationale: the enum keeps `SyncReport`'s other fields (`validators`,
`pushes`, `read_at`, …) shared and untouched; `Failed` gains no variant
(it never carried a snapshot). `PushOutcome`/`RemoteEcho` stay
Deck-shaped — push echoes are a Deck-endpoint fact; files pushes
report through chunk 5's lane. `ValidatorKey` needs **no shape change**
(it already keys on `RemoteBoardId`, which is now the enum).

Naming note: today's `RemoteBoardSnapshot` struct is renamed
`DeckBoardSnapshot` so the enum can own the name; every other
`Remote*` view struct (`RemoteBoard`, `RemoteStack`, `RemoteTask`,
`RemoteLabel`) keeps its name — they are Deck-*shaped* but sit inside
the tagged envelope, and renaming them would be churn without
information.

## 2. The ownership map — home: **derived in the engine, fed by the registry, checked at both ends** (D-BF5)

### Where the assignment lives

The assignment already has a home: **each board's persisted `remote`
binding** (`RemoteBoardBinding { connector, board }`) — the state
engine's sole-source-of-truth row. Chunk 3 does **not** invent a
second store. What D-BF5 adds is the **typed map as a derived view**
plus enforcement at the two places claims are made:

- **Domain** (`state.rs` or a new `ownership.rs` — settled by file
  size at implementation): `BoardOwnershipMap`, built as
  `BoardOwnershipMap::from_state(&AppState) -> BTreeMap<BoardId,
  ConnectorId>` (every board with a `remote` binding maps to its
  binding's connector). Pure data, no IO, snapshot-derived — it can
  never drift from state because it *is* state folded.
- **Enforcement point 1 — the engine (state actor)**: every place a
  *claim* lands goes through the map check. Concretely:
  - `BindBoard { remote }`: after today's `BoardAlreadyBound` guard,
    the new check — if another *live* board already binds the same
    `(connector, remote board)` (any tag payload), refuse.
  - Report ingestion (`resolve_board` consumers): unchanged logic, but
    now written against the map, so a report naming a board owned by a
    *different* connector is structurally unresolvable (returns
    `None`-typed "not mine", never "nearest match").
- **Enforcement point 2 — bootstrap (loud, at attach)**: when the
  registry spawns N connectors, bootstrap walks the persisted bindings
  and the configured connector set: a persisted binding naming a
  connector that is *not configured* is a startup-time typed error
  (`AttachError`/`BootstrapError::OrphanedBinding { connector, board }`
  — loud, before any actor runs). Two connectors *cannot* claim the
  same board in config because claims live in bindings, not config —
  that is the invariant made structural: **config never assigns
  boards; bindings do.** (This matches D-BF5's "engine/bootstrap-level
  check, not a convention" and avoids a YAML ownership table that
  would drift from the DB exactly when a laptop restores from backup.)

### The second-claim refusal typing

New typed outcomes, no text assertions anywhere:

```rust
// domain CommandError
BoardRemoteAlreadyOwned {
    /// The live board that already binds this (connector, remote).
    holder: BoardId,
}
// domain BindBoard guard extension (alongside BoardAlreadyBound):
// rebinding the same board to a remote already held by another board.
```

Bootstrap:

```rust
// app AttachError (or BootstrapError — the enum it joins)
OrphanedConnectorBinding { connector: ConnectorId }
```

Surfacing: the engine refusal rides the existing command-outcome
channel to CLI/UI (typed variant, rendered by the existing
error-rendering layer — one label each, no new plumbing); the
bootstrap refusal aborts attach with the typed error before any actor
spawns (ADR 0010's "loud" tradition — the half-configured-file rule).

### Why not config/registry as the home

A `[sync.ownership]` config table (or a registry-side map) would be a
second write-side for an assignment the DB already owns, and it breaks
under the mixed fleet's real lifecycle: `boards publish`/`BindBoard`
write bindings from state, so a config table would need write-back
into config — an IO crate writing config from the engine, violating
the crate boundaries (state may not write app config). Derived-view +
two-end enforcement keeps every write in the state engine.

## 3. Storage migration shape

What migrates is the **boards table only**; child tables need nothing
(files boards have no remote child ids — UUIDs are the identity, so
their children keep `remote_* = NULL` until chunk 5, which is also
*correct* under D-BF10's no-sidecar/no-binding rule).

- **Migration 0025** (`0025_board_remote_tag.sql`): add
  `remote_files_id TEXT NULL` to `boards`.
  - Encode: `Deck(b)` writes `remote_board_id = b`, `remote_files_id
    = NULL`; `Files(u)` writes `remote_files_id = u`,
    `remote_board_id = NULL`. Exactly one non-NULL (checked by
    construction in the codec, not a SQL trigger).
  - Decode: prefer `remote_files_id` when non-NULL → `Files(u)`,
    else `remote_board_id` → `Deck(b)`, else unbound. The `connector`
    TEXT column and `default_connector` fallback stand unchanged.
  - `remote_seen`, `ValidatorKey` journal rows, `SyncValidators`: no
    schema change — values are tag-blind; keys serialize through the
    enum's serde (old `ValidatorKey` JSON carried bare integers — see
    tolerance below).
- **`ValidatorKey` stored-form tolerance**: journal-persisted
  validator keys carry `Stacks(<u64>)` today. The enum's serde makes
  the new form tagged; the journal codec's existing
  unknown-form-tolerance (R7: park and surface, never misapply) is
  extended with an explicit old-form arm: bare-integer decode →
  `Deck(n)`. One migration is *not* needed if (verified at
  implementation) the journal roundtrip already tolerates re-shapes;
  if it strict-decodes, 0025 gains the backfill instead. **Old rows
  must roundtrip losslessly** — a stage-2 acceptance gate.
- Old-reader tolerance (the inverse direction) is out of scope: in
  fleet, mixed versions park unparsable forms (R7), and the enum's
  text forms are additive in the wire grammar the same way F1's author
  suffixes were.

## 4. TDD ordering (failing-test-first sequence)

Each step lands its failing tests, then the minimum to green, then the
fallout. Ordered so the Deck path is pinned **before** it is touched:

1. **Pin the Deck golden first** (§6) — the pre-widening
   characterization: a full Deck sync journey (bind → cycle → merge →
   validators → counters) snapshot-pinned at the `AppState` + storage
   layer. These pins must pass *unchanged* at every later step.
2. **Domain type tests** (pure): enum roundtrips (serde proptest:
   `Deck(u64)`/`Files(Uuid)`); `Ord`/`Hash` law (tag-aware total
   order — `Deck` and `Files` never equal even on payload collision);
   text-form parse/tolerant-parse proptest.
3. **Snapshot payload tests**: `RemoteBoardSnapshot::{Deck, Files}`
   roundtrips; `DeckBoardSnapshot`'s **byte-compat** property — the
   new `Deck(...)` serde form must decode from the *old* struct's
   serialized bytes (externally-tagged enum with one variant field is
   byte-identical to the bare struct — pin it with a fixed-JSON test,
   not just a property).
4. **Ownership map tests** (domain, pure): `from_state` fold; the
   second-claim law — two live boards binding one `(connector,
   remote)` is unrepresentable via commands (the `BindBoard` refusal
   test at the engine layer: `matches!(err,
   CommandError::BoardRemoteAlreadyOwned { .. })`).
5. **Engine journeys**: second-claim refusal journey (bind board A to
   (deck, 42); attempt board B → typed refusal, state unchanged —
   snapshot-still-equal assertion, not a text match); report
   attribution journey (a Completed report from connector X for a
   board bound to connector Y folds nothing — the "not mine" law).
6. **Storage contract tests**: migration 0025 roundtrip (old Deck rows
   → decode `Deck(n)` lossless; new dual-form roundtrip; exactly-one
   -column invariant), `ValidatorKey` old-form decode → `Deck(n)`.
7. **Bootstrap attach test**: persisted binding naming an
   unconfigured connector → typed loud startup refusal; the happy
   mixed-fleet config (one Deck + standalone/files) boots.
8. **The widening sweep**: switch the types (steps land in domain →
   sync-nextcloud → storage → state → app → ui fallout). Every
   match site exhaustive; the golden pins from step 1 must still pass
   with zero pin edits — a pin edit here is a finding, not a fix.
9. **Deck regression re-proof**: the full sync-nextcloud test matrix
   (wiremock, docker-tier where available, mapping proptests) green
   untouched.

## 5. Test catalogue

New tests (all typed-outcome or structural; no text assertions; no
wall-clock):

| # | Layer | Test | Asserts |
|---|---|---|---|
| T1 | domain (proptest) | id enum serde roundtrip, both variants | roundtrip equality, tag-discrimination |
| T2 | domain (proptest) | text-form grammar + tolerant parse | `deck:`/`files:` forms; old bare-form → `Deck` |
| T3 | domain (fixed-JSON) | `DeckBoardSnapshot` byte-compat with the pre-widening serialization | old bytes decode into the enum |
| T4 | domain (proptest) | snapshot enum roundtrip (both legs) | roundtrip |
| T5 | domain (unit) | `BoardOwnershipMap::from_state` | pure fold, unbound boards absent |
| T6 | state (journey + insta) | second-claim refusal | typed `BoardRemoteAlreadyOwned`, prior state snapshot-equal |
| T7 | state (journey) | cross-connector report folds nothing | board owned by connector Y, report from X → no merge (structural: state unchanged) |
| T8 | storage (contract) | migration 0025 old-row decode | lossless `Deck(n)` decode |
| T9 | storage (roundtrip) | dual-form encode/decode + exactly-one rule | both variants roundtrip |
| T10 | storage (contract) | `ValidatorKey` old-form decode | bare-int → `Deck(n)` |
| T11 | app (bootstrap test) | orphaned-binding attach refusal | typed error, no actor spawned |
| T12 | app (bootstrap test) | mixed-fleet boot | Deck connector + files-capable config coexist, ownership map built |
| T13 | state (insta golden) | **Deck full-sync golden** (step 1, pre-widening) | unchanged through step 8 |
| T14 | sync-nextcloud (existing suite) | wiremock/docker/mapping regression | green, untouched |

Exhaustiveness enforcement: clippy `-D warnings` plus a review-stage
`grep` over the diff for wildcard arms in files the tagged types
reach; no `#[non_exhaustive]` on the new enums (the design *wants*
match fallout loud).

## 6. Deck regression-pinning strategy — **goldens first, journeys as the net**

Both, with roles:

- **Golden (the byte-equivalence gate)**: T13 — an insta snapshot of
  the full Deck sync pipeline *result* (engine `AppState` after a
  recorded bind→cycle→merge→validator-append sequence, plus the
  storage batch it produced). Written at step 1 from pre-widening
  `main`, it must pass **unmodified** after the sweep. This is the
  acceptance's "behavior byte-equivalent" made checkable. Fixed-JSON
  wire-compat (T3) covers the serialization face the golden can't
  (the report payload's own bytes).
- **Journeys (the behavior net)**: the existing sync-nextcloud suite
  (wiremock actor tests, docker-tier e2e where the tier is available,
  mapping proptests, push_exec overlay tests) plus the engine sync
  journeys run unchanged; T14 is green-by-untouchedness. Where a
  journey needs an id literal, it constructs `Deck(n)` explicitly —
  the mechanical fallout of step 8 is allowed there, but **no journey
  logic edits** are.
- Deliberately *not*: re-recording the golden after the sweep (that
  would launder a behavior change into a pin), weakening any
  characterization assertion, or asserting on log/label text anywhere.

## 7. Risks & mitigations

- **Fallout breadth (836 refs / ~75 files, mostly fixtures)**:
  mitigated by doing the sweep in one focused step (step 8) with the
  golden pinned beforehand; test-support constructors
  (`test_support::remote*`, `merge_testutil`) get `Deck(...)`
  fixtures once, so fixture churn concentrates in one module.
- **Serde shape drift** (the enum must not silently change the Deck
  report's wire/storage face): T3's fixed-JSON byte-compat pin is the
  tripwire; externally-tagged single-field enum is chosen *because*
  it round-trips the bare struct.
- **`ValidatorKey` persistence tolerance**: if the journal strict
  -decodes keys, the 0025 backfill branch (§3) — decided by a
  stage-2 verification, both branches planned.
- **Second refactoring smuggled in**: the sweep touches everything,
  so "while we're here" temptations are real; the review mandate
  already names this — no pin edits, no Deck logic edits, files-tag
  machinery is chunk 5's.
- **`Ord`/`Hash` semantics change** (`RemoteBoardId` was a plain
  newtype ordering by u64): BTreeMap/BTreeSet users get a tag-aware
  order; grep-verified consumers are internal maps and snapshot
  sorts — covered by T1's law test and the golden (T13).
- **Cargo-compilation cliff**: the domain enum flip breaks every
  crate at once; acceptable (a workspace-wide mechanical fix), but
  stage 2 should keep commits ordered domain-first so each commit
  still builds standalone (commit-hygiene rule).

## 8. Estimated size

Domain rewrite ~150 lines; fallout sweep ~700–900 (mostly mechanical
constructor/match churn, ~60% in test fixtures); storage migration +
codec ~120; ownership map + refusals + bootstrap ~250; tests T1–T13
~600–800. **Total ≈ 1.8–2.3k lines** — M/L per the dispatch's sizing.

## 9. References

- Design round §3 Part C, §5, §7 (chunk table);
  `.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`.
- Dispatch plan §4 (chunk 3 scope + acceptance);
  `.artifacts/plans/2026-11-07-boards-over-files-implementation-dispatch-plan.md`.
- Backbone report §7.4 (choice (b)); options report §4.B (the
  coexistence fence); ADR 0010; ADR 0007 (cycle vocabulary).
- Issue #686 (this chunk), #684/#685 (landed predecessors), #687–#689
  (successors); milestone `nextcloud-files-sync`.
