# [Plan] The Files Core Crate — `taskboard-sync-files` (TDD implementation plan)

- **Date**: 2026-11-05
- **Kind**: stage-1 implementation plan — chunk 1 of the DR-8
  implementation dispatch (branch `feat/sync-files-core`, issue #629).
  TDD-first: every commit below names its failing tests before the
  code that satisfies them.
- **Origin** (normative): the accepted design round
  `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  — §3 Part A (D-SF1–D-SF4), Part B (D-SF5), the conflict-posture
  paragraph (D-SF16's `base_clock` reservation), §5 test hooks, §7
  chunk 1; the session rulings Q1–Q8 on issue #626 (2026-11-04); the
  backbone report
  `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  §5.4/§5.5 (the layout lineage) and §7.1/§7.2 (filesystem + naming
  discipline); the dispatch plan
  `.artifacts/plans/2026-11-05-filesync-substrate-implementation-dispatch-plan.md`
  §2; the cross-milestone contract
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  §3 (the journal passenger's roaming-ready shape — chunk 3 binds it).
- **Status**: READY for the stage-2 implementation agent. Decisions
  D-FC1…D-FC10 below settle every open question this chunk owns;
  deviations from a normative origin section need a recorded reason in
  the stage-2 report.

---

## 1. Context & verified anchors (verified 2026-11-05 against `main` @ `1dc5bf44`)

| Fact | Anchor | Consequence for this chunk |
|---|---|---|
| The merge engine exists as section unions | `crates/taskboard-domain/src/appdata.rs` | The row grammar this crate mirrors: `RuleEntry { Rule, Tombstone, Verbatim }` with `deserialize_verbatim` (an unmodelable shape stays `Verbatim`, never an error), a `clock()` accessor, unknown maps under `#[serde(flatten)]`, unknown sections carried verbatim, **envelope strict** (`format` required) and **rows lenient** |
| The fold's tie discipline must differ from the flush union's | `union_*` in `appdata.rs` | The pairwise flush unions resolve equal clocks to the overlay — deliberately input-ordered. The n-ary device fold must be **input-order-free** (D-SF5 laws); a different tie rule is required, not a bug to copy |
| No canonical-serialisation hash exists in-tree | grep over `crates/` (`state_hash`, `Hasher`, `sha2`, `blake`) — only `taskboard-app/src/ipc/mod.rs` uses a hand-pinned **FNV-1a 64**, with the documented reason that `std::hash::DefaultHasher` has no cross-version stability | The state-hash rule is decided here and recorded (D-FC7): FNV-1a 64 over a canonical serialisation, algorithm-tagged string — no new dependency, no `sha2` |
| Workspace deps are pinned once | root `Cargo.toml` `[workspace.dependencies]` | This crate needs **no new external dependency**: `serde`, `serde_json`, `chrono` (serde), `uuid` (v4/v7/serde), dev `proptest` — all pinned already. `serde_json` is pinned **without** `preserve_order` → its `Map` is `BTreeMap`-backed (see D-FC7's canonicalisation note) |
| Lib-crate manifest house style | `crates/taskboard-domain/Cargo.toml` | `version/edition/rust-version/authors/repository.workspace = true`, explicit `license = "MIT OR Apache-2.0"`, `[lints] workspace = true`; tests live in-module (`#[cfg(test)] mod tests` + `proptest!`); domain shares proptest strategies via an optional `test-support` feature — this crate mirrors that (chunk 3's engine tests will want document strategies) |
| ADR numbering; house style | `docs/adr/0001…0019`, `0019-undo-command-journal.org` | ADR 0020 is this chunk's (D-SF19): org-mode, `Date/Status/Depends on/Carriers` header, Context / Decision (numbered) / Chunk-level settlements / Consequences |
| The v1 passenger's contract is entity-independent | v0.2 impl-plan §3 | The journal (wire section `journal`; the domain vocabulary is `reflection`/`ReflectionEntry` — `journal` is taken by the undo journal) is roaming-ready: `BTreeMap` keyed by family UUID, per-entry clocks, tombstone rows, tolerant serde. **Chunk 1 is journal-independent**: the row *body* stays a preserved JSON object here; the typed reflection decode is chunk 3's |
| The transport facts that shape the layout are measured | characterization F3–F6 | Chunk 2's concern, but the taxonomy below (create-once identity, per-device subtrees, reserved kinds) is the type-level half this chunk owns |

## 2. Scope & non-goals

**In scope**: the new pure crate `crates/taskboard-sync-files` —
identifiers (D-FC2), the tree taxonomy (D-FC3), the identity/heartbeat
payload types (D-FC4), the per-user document codec (D-FC5), the n-ary
fold and its laws (D-FC6), the canonical state hash (D-FC7); ADR 0020
(D-FC10); root-manifest membership + CHANGELOG entry.

**Non-goals** (fences, per the dispatch §2): any IO, fs, network,
async, or UI dependency — the crate compiles as a pure lib and its
dependency tree proves it (§7 review pin); the WebDAV transport,
conditional PUTs, `[files_sync]` config (chunk 2); the engine seam,
`UserDataReport`, the typed journal import (chunk 3); surfacing,
runbooks (chunk 4); **emitting** reserved kinds (`ops/*.json`,
`snapshot-*.json`) — reserved means tolerated, never produced; the
settings/proofs sections' payload schemas (second slice);
task-field/`ops` chunk shapes (B-full territory).

## 3. Design decisions (D-FC1…D-FC10, normative for stage 2)

### D-FC1 — Crate skeleton & manifest

```
crates/taskboard-sync-files/
├── Cargo.toml            # lib; workspace-inherited fields; license = "MIT OR Apache-2.0"
│                         # [dependencies] serde, serde_json, chrono, uuid (all workspace)
│                         # [dev-dependencies] proptest, serde_json
│                         # [features] test-support = ["dep:proptest"]  (domain's precedent)
│                         # [lints] workspace = true
└── src/
    ├── lib.rs            # crate doc: the substrate contract in one screen — per-device
    │                     # path ownership, current-state v1, tolerant relay, the fold's
    │                     # total order; module map; the no-IO constraint stated as a
    │                     # crate invariant. SPDX header. #![forbid(unsafe_code)].
    ├── ids.rs            # DeviceId, AuthorId (D-FC2)
    ├── layout.rs         # tree constants, DeviceFileKind + classify, path builders,
    │                     # reserved-name predicates (D-FC3)
    ├── identity.rs       # FolderIdentity ("identity.json"), DeviceIdentity
    │                     # ("device.json" payload), decode tolerance (D-FC4)
    ├── heartbeat.rs      # DeviceInfo ("info.json" payload) (D-FC4)
    ├── document.rs       # UserDocument/UserSections/UserRecord + tolerant codecs (D-FC5)
    ├── fold.rs           # fold(), the winner order (D-FC6)
    └── hash.rs           # canonicalise + FNV-1a 64 + StateHash (D-FC7)
```

Every file carries the SPDX `MIT OR Apache-2.0` header; the crate (lib
tier) has `#![forbid(unsafe_code)]`. Module set is closed — a module
not listed here is a stage-2 deviation to justify. Rough size: ~500–650
src LOC + ~700–900 test LOC.

### D-FC2 — Identifiers (the A1 pattern)

- `DeviceId(Uuid)` — private field, `DeviceId::parse(&str) -> Option<Self>`
  (strict UUID parse) as the trust-boundary mint, `DeviceId::mint() -> Self`
  (`uuid::Uuid::now_v7()`) the single creation site for a new device —
  the only nondeterminism in the crate, outside the codecs and the
  fold. `Display` = lowercase hyphenated (the **path-safe form** — the
  reason this is a newtype rather than a bare `Uuid`: directory names
  are derived exclusively through it). `Serialize`/`Deserialize`
  hand-written over the display form (serialize = `Display`, deserialize
  = strict `parse`, failure = typed serde error).
- `AuthorId(Uuid)` — the Q2/F1 author slot. Same pattern minus `mint()`
  (authors are minted by B-full, never by v1). **Load-bearing
  leniency rule**: a wire row carrying an `author` value that fails
  this parse demotes the **whole row to `Verbatim`** — never drops the
  author. Silently de-authoring a row would change its fold rank
  against the author-carrying peer (divergence); fail-closed keeps the
  relay honest.
- `folder_id` in `FolderIdentity` is a plain `uuid::Uuid` — it is only
  ever echoed, never ordered or turned into a path; no legality beyond
  "is a UUID" is claimed, so no newtype (A1 restraint).

### D-FC3 — The tree taxonomy (pure path types)

Constants and builders over **forward-slash relative paths below the
substrate root** — strings, never `PathBuf` (the crate has no
filesystem):

```rust
pub const ROOT_FOLDER_NAME: &str = "taskboard";   // Q5: visible root-level folder
pub const DEVICES_DIR_NAME: &str = "devices";
pub const IDENTITY_FILE_NAME: &str = "identity.json";
pub const INFO_FILE_NAME: &str = "info.json";
pub const APPDATA_FILE_NAME: &str = "appdata.json";
pub const OPS_DIR_NAME: &str = "ops";

pub fn device_dir_name(id: DeviceId) -> String;            // "<uuid-lowercase>"
pub fn device_info_path(id: DeviceId) -> String;           // "devices/<id>/info.json"
pub fn device_appdata_path(id: DeviceId) -> String;        // "devices/<id>/appdata.json"
pub fn device_ops_path(id: DeviceId, seq: u64) -> String;  // "devices/<id>/ops/NNNNNN.json" (6-digit zero-pad)
pub fn device_snapshot_path(id: DeviceId, seq: u64) -> String; // "devices/<id>/snapshot-NNN.json"
```

The classifier chunk 2's PROPFIND listing routes through:

```rust
pub enum DeviceFileKind {
    Identity,                    // "identity.json" (root)
    Info(DeviceId),              // "devices/<id>/info.json"
    AppData(DeviceId),           // "devices/<id>/appdata.json"
    Ops { device: DeviceId, seq: u64 },       // RESERVED — v1 never parses
    Snapshot { device: DeviceId, seq: u64 },  // RESERVED — v1 never parses
    Temporary,                   // final component contains ".tmp-" (the backbone §7.1
                                 //   write discipline's leak window) — never parsed
    Other,                       // anything else (unparsable device dir name, unknown
                                 //   file name) — relay-never-choke at the type level
}
pub fn classify(relative_path: &str) -> DeviceFileKind;
```

Classifier rules: strict (exact file names; the device component must
`DeviceId::parse`); ops seq accepts any all-digit component and parses
`u64` (emission normalises to 6 digits, decode does not demand it —
leniency documented at the type); `classify` never panics on any input
string (unit-test pin). Reserved kinds are **structural**: the crate
defines no payload type for `Ops`/`Snapshot`, so there is no code path
that could parse them — the "never parse, never delete, never choke"
posture enforced by construction, not by comment.

### D-FC4 — Identity & heartbeat payloads

Derived serde (`Debug, Clone, PartialEq, Serialize, Deserialize`),
fields in the design's order; `format_version: u32` and timestamps
`DateTime<Utc>`. Decode is **strict-envelope, tolerate-extras**: unknown
keys are ignored by serde's default on these structs (these payloads
are never re-serialized by us, so nothing to relay — contrast the
document below); a missing required key or wrong-typed value is a
typed decode failure (`serde` error), the strict boundary.

```rust
// identity.json — create-once at the root (D-SF2; chunk 2 performs the If-None-Match: * PUT)
pub struct FolderIdentity { pub folder_id: uuid::Uuid, pub format_version: u32, pub created_at: DateTime<Utc> }
pub const TREE_FORMAT_VERSION: u32 = 1;

// device.json — create-once in the app data dir (D-SF4; the file IO is chunk 2's, the payload type is ours)
pub struct DeviceIdentity { pub device_id: DeviceId, pub created_at: DateTime<Utc> }

// info.json — the per-device heartbeat, refreshed on every flush (D-SF14's type half)
pub struct DeviceInfo {
    pub app_version: String,
    pub format_version: u32,
    pub last_clock: DateTime<Utc>,      // the newest entry clock the device has folded
    pub state_hash: StateHash,          // D-FC7; opaque equality token
    pub written_at: DateTime<Utc>,
}
```

Exactly the design D-SF14 field list — no `device_id` field (the
subtree path owns identity; the backbone's extra fields —
`instance_id`, `last_seq` — are the §5.5 machinery B-min replaced with
the D-SF4 fork alarm). `StateHash` deserializes **opaquely** (any
non-empty string; no prefix validation): a peer with a future hash
scheme can never spuriously compare equal, which is the safe failure
direction (worst case a divergence finding, human-eyed — never a false
"converged").

### D-FC5 — The per-user document (`appdata.json`)

Model (all collections `BTreeMap` — this is load-bearing for D-FC7):

```rust
pub const DOCUMENT_FORMAT: u32 = 1;
pub const JOURNAL_SECTION: &str = "journal";      // the v1 passenger (wire name; the
                                                  // domain vocabulary stays reflection/*)
pub const RESERVED_SECTION_NAMES: &[&str] = &["settings", "proofs"];  // never emitted (Q3)
pub fn is_reserved_section_name(name: &str) -> bool;

#[derive(Debug, Clone, PartialEq)]
pub struct UserDocument {
    pub format: u32,
    pub sections: UserSections,
    pub unknown: BTreeMap<String, serde_json::Value>,   // unknown envelope keys, carried (flatten)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UserSections {
    pub journal: Option<BTreeMap<String, UserRecord>>,  // keyed by entry UUID string
    pub unknown: BTreeMap<String, serde_json::Value>,   // unknown/reserved sections, carried (flatten)
}

#[derive(Debug, Clone, PartialEq)]
pub enum UserRecord {
    /// A modeled row: `clock` + optional `author` are the competition
    /// metadata; **every other key of the row object rides `body`
    /// preserved** (the journal row's own fields are chunk 3's typed
    /// decode of exactly this body).
    Entry { body: BTreeMap<String, serde_json::Value>, clock: DateTime<Utc>, author: Option<AuthorId> },
    /// `{ "deleted": true, "clock": …, "author"?: … }` — the `RuleEntry` tombstone grammar.
    Tombstone { clock: DateTime<Utc>, author: Option<AuthorId> },
    /// A shape this version does not model: preserved verbatim, never folded.
    Verbatim(serde_json::Value),
}
```

**Wire shapes** (exact; the codec is hand-written — no derive on
`UserRecord`, mirroring `RuleEntry`):

```json
{
  "format": 1,
  "sections": {
    "journal": {
      "0193b4c5-d6e7-7c89-ab01-2345abcdef01": {
        "kind": "reflection",
        "text": "The fold laws went green.",
        "clock": "2026-11-05T09:15:32Z"
      },
      "0193b4c6-f0e1-7d2c-ab4a-59687a89bcde": {
        "deleted": true,
        "clock": "2026-11-05T10:00:00Z"
      },
      "0193b4c7-1223-7e45-8679-0b1c2d3e4f50": {
        "opaque": "future-peer-row",
        "v2": [1, 2, 3]
      },
      "0193b4c8-a7b8-7c9d-ab0e-f1e2d3c4b5a6": {
        "text": "authored by a B-full peer someday",
        "clock": "2026-11-05T11:00:00Z",
        "author": "0193a1b2-c3d4-7cc9-8a2e-5f1e2d3c4b5a"
      }
    },
    "proofs": { "not": "understood here", "relayed": true }
  }
}
```

Row 1 → `Entry` (body = the object minus `clock`); row 2 → `Tombstone`;
row 3 → `Verbatim` (no decodable clock); row 4 → `Entry` with
`Some(AuthorId)` — a shape v1 never **emits** but must decode (the Q2
wire slot; emission omits the `author` key entirely when `None`).
`proofs` (reserved) lands in `sections.unknown` like any unknown
section.

Decode rules (the `RuleEntry` recipe, in priority order per row):

1. not an object → `Verbatim(raw)`.
2. `"deleted" == true` ∧ `clock` decodes as RFC 3339 → `Tombstone`
   (`author` parsed if present; **author-parse failure → `Verbatim(raw)`**,
   per D-FC2). `deleted: true` with a bad/missing clock → `Verbatim(raw)`.
3. otherwise: `clock` decodes → `Entry { body = object minus
   `clock`/`author`, clock, author? }` (author rule as above; an
   `author: null` decodes as `None` — tolerant).
4. otherwise → `Verbatim(raw)`.

Document/envelope rules: the envelope is the **strict boundary** —
`format` missing or non-integer, or the document not a JSON object, is
a typed decode failure (the `AppDataDocument` precedent). The
`journal` section body that is not an object of rows lands in
`sections.unknown["journal"]` verbatim (never a document failure — the
`DismissalsEntry` non-object precedent). Row **map keys** are plain
strings: this version emits UUIDs, decode tolerates any key (a
hand-edited non-UUID key relays through fold untouched — deliberate
leniency, documented at the codec).

Emission: roundtrip-exact for the model (`decode(encode(doc)) == doc`
law, §5). `author` is emitted only when `Some` (v1 never emits it —
the dispatch's grep-pin target).

**Decision — "ignored" upgraded to "carried"** (a recorded deviation
from the dispatch letter, required by the design's relay doctrine): the
dispatch hard-rule says "unknown fields ignored"; the design doc's own
transport rule (D-SF3, and `appdata.rs`'s doc comment) is *relay* — a
device re-publishes its whole current view, so anything it drops is
lost to the fleet on that device's death. Consequence: unknown envelope
keys, unknown/reserved sections, unknown keys inside `Entry` bodies,
and `Verbatim` rows are all **carried**, never errorized, never
silently dropped. "Ignored" holds only for the never-republished
payloads of D-FC4. `base_clock` (the D-SF16/Q6 reservation) needs no
special handling under this rule: when a future peer emits it, it
rides the `Entry` body preserved — the reservation is structural.
Emissions of reserved kinds/sections stay impossible (grep pin: no
code path writes `settings`/`proofs`/`ops/`/`snapshot-`).

### D-FC6 — The fold (D-SF5's implementation contract)

```rust
/// Folds any set of device documents into the merged view. Pure, total,
/// input-order-free: the laws in §5 are its contract. An empty input
/// yields the empty current-format document (never a panic).
pub fn fold<'a>(documents: impl IntoIterator<Item = &'a UserDocument>) -> UserDocument;
```

Per section map (today only `journal`; unknown sections and unknown
envelope keys merge by the same rule one level down, their whole
`Value` as the comparable content):

- **Clocked rows** (`Entry`/`Tombstone`) compete per entry key by the
  order tuple `(clock, author, content)`:
  - `clock: DateTime<Utc>` — embedded write clock, tombstones compete
    identically (a later delete beats an edit; a later edit beats an
    older tombstone — the `RuleEntry` semantics unchanged);
  - `author: Option<AuthorId>` — the Q2 slot; `None` for every v1
    writer; `Rust`'s `Option` order (`None < Some(_)`) is the pinned
    forward choice: at equal clocks a B-full authored row outranks an
    authorless one;
  - **content tiebreak**: rows with equal `(clock, author)` compare
    their canonical JSON content (`serde_json::Value`'s total `Ord`;
    `Entry` → its body as an object value, `Tombstone` → an identical
    constant). This is the honest reading of D-SF5's "max by (clock,
    author, entry-id)": the entry-id leg is the section map key itself
    — constant within a per-key comparison — so the residual
    equal-(clock, author) tie (two devices stamping the same second,
    plausible at whole-second clocks) needs a deterministic,
    input-order-free totaliser; content order is that totaliser. This
    closes the review prompt's named trap: **no input permutation can
    change any winner.**
- **`Verbatim` rows are the floor**: they never outrank a clocked row
  ("never ordered, never folded" — a shape this version cannot
  understand never displaces modeled content). A key holds a
  `Verbatim` row in the merged output **iff no input document carries
  a clocked row for that key**; multiple unequal `Verbatim` values at
  one key resolve by `Value`'s `Ord` (deterministic, commutative).
  Rationale: `clocked > verbatim` lets B-full's decoded understanding
  propagate over an old peer's opaque relay, and the fold stays a pure
  `max` over one total order — which is exactly what makes
  commutativity/idempotence/associativity hold by construction (the
  laws in §5 then *verify* it rather than establish it).
- Sections/maps as a whole: `journal` is `Some(merged)` iff any input
  carries it (the `union` Option semantics); unknown sections and
  unknown envelope keys merge key-wise by `Value`-max.
- `format` of the output = `max` over inputs (deterministic; the
  version gate itself lives with the consumer — D-FC8).

The fold is **document-attributed-free**: which device published a row
never influences the winner (a re-publication by a third device must
not change anything — idempotence). The local engine state joins as
"one more input" (D-SF5) in chunk 3, by deriving its document first —
`fold` itself takes documents only.

### D-FC7 — The state hash (decided here; the design left the algorithm open)

```rust
pub struct StateHash(String);                      // opaque; Display/AsRef
impl StateHash {
    pub fn of(document: &UserDocument) -> StateHash;
}
```

`StateHash::of(doc) = "fnv1a64:" <> lower-hex(fnv1a64_64(serde_json::to_vec(canonicalise(doc))))`:

- **canonicalise**: recursive rebuild of every JSON object through a
  `BTreeMap<String, Value>` (key-sorted) before serialising — maps in
  the model are already `BTreeMap`, so this pass only pins the raw
  `Value` leaves (`Verbatim` rows, unknown sections) and immunises the
  hash against any future `serde_json/preserve_order` feature leak
  (with that feature on, `Map` becomes insertion-ordered and naive
  `to_string` would make two devices that parsed differently-ordered
  files compute different hashes — false divergence alarms).
- **FNV-1a 64**, hand-pinned in `hash.rs` with the in-tree precedent's
  rationale (`taskboard-app/src/ipc/mod.rs`: `DefaultHasher` is not
  cross-version stable). **Ruling: no `sha2`** — divergence detection
  needs a stable equality fingerprint, not cryptographic strength (the
  folder is plaintext anyway, Q7); a collision can only mask a
  divergence (≈2⁻⁶⁴ per comparison pair — nothing at fleet scale), it
  can never fire a false alarm (equal hashes ⇒ equal canonical bytes).
  The algorithm **tag in the string** (`fnv1a64:`) is the recorded
  upgrade path: a future scheme mints a new prefix; differently-tagged
  hashes can never compare equal (safe direction, D-FC4's note).
- The hash covers the merged per-user document (D-SF5's "the merged
  view") — i.e. exactly what the device's `appdata.json` publishes, so
  `info.json`'s `state_hash` and the published document agree by
  construction. `identity.json`/`info.json` payloads are not hashed.

### D-FC8 — Version posture (the `AppDataDocument` precedent, verbatim)

The codecs are **version-blind**: they decode any `format`/`format_version`
value and never name capability flags; consumers compare and gate
("greater format folds nothing" — chunk 2 parks, D-SF15). The fold
emit-side rule (output `format` = max) is bookkeeping for a caller
error, not a capability mechanism.

### D-FC9 — Test architecture

In-module `#[cfg(test)] mod tests` per module (the domain house
style); `proptest!` blocks in the same modules; a `test-support`
feature exporting the document/row strategies for chunk 3's engine
tests (the `taskboard-domain` precedent). No `insta` (nothing here has
a human-readable state tree worth snapshotting — the laws are the
contract); no tokio (nothing async); no sleeps (nothing async — the
AGENTS determinism rules are satisfied vacuously and the review pins
it).

## 4. Decision summary

| ID | Question | Decision |
|---|---|---|
| D-FC1 | Crate shape | `crates/taskboard-sync-files`, 8 files as listed; workspace deps only (serde, serde_json, chrono, uuid; dev proptest); `test-support` feature; lib tier, `forbid(unsafe)` |
| D-FC2 | Identifiers | `DeviceId`/`AuthorId` A1 newtypes over `Uuid`; strict parse mint; `DeviceId::mint()` = uuid7, the crate's only nondeterminism; unparseable wire `author` demotes the whole row to `Verbatim` (fail-closed); `folder_id` stays plain `Uuid` |
| D-FC3 | Taxonomy | Pure relative-path constants/builders + `DeviceFileKind::classify`; `Ops`/`Snapshot`/`Temporary` kinds have no payload type — reserved kinds are unparseable by construction; `Other` = relay-never-choke |
| D-FC4 | Identity/heartbeat types | `FolderIdentity { folder_id, format_version, created_at }`, `DeviceIdentity { device_id, created_at }`, `DeviceInfo { app_version, format_version, last_clock, state_hash, written_at }` — exactly D-SF14's list, no `device_id`/`instance_id`/`last_seq`; strict-envelope + ignore-extras; `StateHash` opaque on decode |
| D-FC5 | Document codec | Envelope `{format, sections}` + flattened unknowns; `UserRecord {Entry{body, clock, author?}, Tombstone, Verbatim}`; body-preservation relay ("ignored"→"carried", recorded deviation); strict envelope, lenient rows; reserved names carried, never emitted; `base_clock` reservation is structural |
| D-FC6 | Fold | `fold(&[UserDocument]) -> UserDocument`; per-key winner = max over the total order `(rank, clock, author, content)` with `Verbatim` the floor (`rank 0`); content = `Value::Ord` totaliser for the equal-(clock, author) residual; document attribution never influences the winner |
| D-FC7 | State hash | `StateHash::of = "fnv1a64:" + hex(fnv1a64(to_vec(canonicalise(doc))))`; FNV-1a 64 (the ipc precedent), **no sha2**; canonicalise = recursive BTreeMap key-sort (preserve_order immunisation); tagged string = the upgrade path |
| D-FC8 | Version posture | Version-blind codecs; consumers gate; fold's `format = max` is caller-error bookkeeping |
| D-FC9 | Tests | In-module tests + `proptest!`; `test-support` strategies; no insta/tokio |
| D-FC10 | ADR 0020 | Written by this chunk; outline §6 |

## 5. Test plan (TDD-first — the failing tests come before each commit's code)

### Commit 1 — `feat(sync-files): the crate skeleton, identifiers, and tree taxonomy`
Covers D-FC1–D-FC4 (`ids`, `layout`, `identity`, `heartbeat`, `lib`,
manifest, root `Cargo.toml` members entry). Failing tests first:

- `ids`: display form is lowercase hyphenated and filesystem-safe
  (structural: matches `[0-9a-f-]{36}`); `parse` rejects non-UUIDs;
  roundtrip `parse(display(mint())) == mint()`; serde roundtrip;
  **`AuthorId` has no mint** (compile-time: the constructor does not
  exist — pinned by its absence, noted for review).
- `layout`: `classify` table over the full taxonomy —
  `identity.json`, `devices/<id>/info.json`, `devices/<id>/appdata.json`,
  `devices/<id>/ops/000042.json`, `devices/<id>/snapshot-000128.json`,
  `devices/<id>/info.json.tmp-<uuid>` → `Temporary`, unknown names →
  `Other`, unparsable device dir → `Other`; **`classify` never panics**
  over an arbitrary-string proptest; path builders compose with
  `classify` (builder output re-classifies to the intended kind —
  roundtrip pin); reserved predicates (`is_reserved_section_name`).
- `identity`/`heartbeat`: decode of the exact wire examples (§3);
  missing `format_version` / wrong type → typed failure; unknown extra
  keys tolerated; `StateHash` decodes opaquely (a `sha256:`-tagged
  string decodes — the forward tolerance) and `Display` roundtrips.

### Commit 2 — `feat(sync-files): the per-user document codec`
Covers D-FC5. Failing tests first:

- Wire-example decode: the four journal rows of §3's example decode to
  `Entry{body…}` / `Tombstone` / `Verbatim` / `Entry{Some(author)}`;
  `proofs` lands in `sections.unknown`.
- Roundtrip law (proptest): `decode(encode(doc)) == doc` over generated
  documents (bodies with arbitrary JSON, verbatim rows, unknown
  sections, authors `None`/`Some`).
- Garbage catalogue (table, **floors not ceilings** — assert the typed
  outcome of each listed garbage class, never counts): non-object
  document; missing/non-integer `format` → typed failure; row: bare
  string / number / null → `Verbatim`; `deleted: true` without clock →
  `Verbatim`; clock not RFC 3339 → `Verbatim`; `author` not a UUID →
  whole row `Verbatim` (the fail-closed pin); `author: null` → `None`;
  `journal: 42` → `sections.unknown["journal"]`; non-UUID map key →
  relays through decode+encode unchanged. Floor assertion: any document
  carrying ≥1 well-formed row decodes with that row modeled.
- **Emission pins**: a v1-authored document's JSON contains no
  `author` key, no `settings`/`proofs` section, no `base_clock` key
  (structural JSON inspection, not text-matching of error strings);
  unknown content is emitted back (the carry law: encode preserves
  unknown envelope keys / unknown sections / body extras verbatim).

### Commit 3 — `feat(sync-files): the fold and its laws`
Covers D-FC6. Failing proptests first (the laws ARE the tests; the
strategies generate document sets with duplicate keys, equal-clock
collisions, tombstone/edit races, verbatim rows, unknown sections,
mixed authors):

1. `fold_is_commutative` — `fold(permute(S)) == fold(S)` **including
   `StateHash::of` equality** (the hash inherits order-freedom).
2. `fold_is_idempotent` — `fold(S ∪ dup) == fold(S)` for whole-document
   dupes and per-row dupes across documents.
3. `fold_is_associative` — `fold3(A,B,C) == fold(fold(A,B), C) ==
   fold(A, fold(B,C))`.
4. `tombstone_dominates_older_edit_and_loses_to_newer` — property over
   clock pairs at one key (both directions, the `RuleEntry` ruling).
5. `verbatim_is_the_floor` — a clocked row at a key evicts no verbatim
   *into* the slot: the winner is the clocked row; a verbatim survives
   iff no clocked row exists anywhere in the input; unequal verbatims
   alone resolve deterministically (permutation-invariance covers it).
6. `author_none_totality` — the review prompt's named trap, generated
   *adversarially*: sets seeded with equal-clock equal-`None`-author
   rows whose bodies differ; asserts permutation-invariance (one
   winner regardless of input order) — the fold is total over v1's own
   shape.
7. `some_author_outranks_none_at_equal_clock_forward_pins_b_full` —
   the forward pin: with `Some(a)` vs `None` at equal clocks the
   authored row wins, laws still hold over mixed-author sets (Q2's
   seam; the name documents it as the designed forward choice, not
   today's-behavior debt).
8. `unknown_content_merges_order_free` — unknown sections/envelope
   keys merge by `Value`-max under permutation.
9. `empty_input_is_total` — `fold([])` = the empty current-format
   document.

### Commit 4 — `feat(sync-files): the canonical state hash`
Covers D-FC7. Failing tests first:

- `hash_is_stable_across_model_equivalents`: two `UserDocument`s that
  differ only in raw-`Value` key insertion order hash equal (the
  `canonicalise` pin — the preserve_order immunisation, expressed as
  behavior).
- `hash_of_decoded_roundtrip_matches` — `hash(decode(encode(doc))) ==
  hash(doc)` (canonicalisation survives the codec).
- `state_hash_format_is_tagged` — the value is `fnv1a64:<32 lowercase
  hex>` (structural pin on the mint), and unequal documents (one row's
  body changed) hash unequally over a proptest (the *useful* direction;
  no quality ceiling — collision-freeness is never asserted).
- `fold_output_hash_is_input_set_hash_of_merged` — `hash(fold(S))`
  equals `hash` of the same merged document built directly (trivially
  the same call, pinned as the D-SF5(b) seam for chunks 2–4).

### Commit 5 — `docs(adr): the files substrate contract` (ADR 0020, §6)

### Commit 6 — `docs(changelog): the files core crate` + gate green
CHANGELOG Unreleased entry. Full gate suite via
`scripts/run-quality-gates.sh` (fmt, clippy `-D warnings`, nextest,
doc tests, deny, llvm-cov sequence) — the crate adds no dependency, so
`cargo deny` must be untouched.

### Review pins (stage 3's checklist, from the dispatch)

1. Order totality: law 6 exists and is adversarially generated (not a
   vacuous permutation over distinct clocks).
2. Tolerance is structural: no decode error kills a document except
   the envelope's; floors on garbage, no ceilings.
3. Reserved kinds/names never emitted — grep the diff for
   `settings`/`proofs`/`ops`/`snapshot` emissions.
4. Zero IO/async: `cargo tree -p taskboard-sync-files` shows no
   tokio/reqwest/slint/fs-dependent crates; the manifest is the proof.
5. ADR 0020 states the B-full reservations and the `base_clock`
   reservation accurately.
6. Workspace dep pinning untouched (`[workspace.dependencies]` diff is
   members-list only).

## 6. ADR 0020 outline

`docs/adr/0020-files-sync-substrate-path-ownership-current-state-v1.org`,
house style per 0017–0019:

- **Title**: ADR 0020: The Files Sync Substrate — Server-Enforced Path
  Ownership, Current-State v1, and the B-full Reservations
- **Header**: Date 2026-11-05; Status Accepted (lands with chunk 1,
  #629); Depends on ADR 0004 (LWW over per-field write clocks), 0007
  (push-first sync), 0008 (bootstrap/config/secrets), 0010
  (standalone-first optional connector); Carriers #629 (this chunk),
  #630–#632 (transport, seam, hardening), #626 (the round)
- **Context**: per-user roaming need (the journal first); the backbone
  T-model; why path ownership beats every shared-path design (T4
  becomes server-enforced via conditional PUTs — characterization F3/F4);
  the Q1–Q8 rulings in one paragraph each where they bind the contract.
- **Decision** (numbered):
  1. The tree & path ownership (D-SF1/D-SF2): root-level `taskboard/`,
     `identity.json` create-once, per-device subtrees, single-writer
     rewrites under `If-Match`; unexpected own-path 412 = fork alarm
     (chunk 2 mechanics, the contract stated here).
  2. The document & row grammar (D-SF3 + D-FC5): envelope, `UserRecord`,
     body-preservation relay, strict envelope/lenient rows, reserved
     sections/kinds.
  3. The fold's total order (Q2 + D-FC6): `(rank, clock, author,
     content)` with the verbatim floor; the author slot's forward
     semantics (`None < Some`); the entry-id clarification.
  4. The state hash (D-FC7): FNV-1a 64 over canonicalised
     serialisation, tagged string, no cryptographic pretension, the
     upgrade path.
  5. The B-full reservations (Q1's transition-smoothness debt,
     itemised): reserved `ops/*.json` + `snapshot-*.json` kinds
     (opaque-relayed, never emitted in v1); the `author` wire slot
     (decode-only today); the `base_clock` body slot (the Q6
     three-way-merge seam); reserved `settings`/`proofs` sections.
  6. The conflict posture (Q6/D-SF16): silent per-entry LWW,
     documented not secret; auto-merge + manual diff/merge are fenced
     phases of their own.
- **Chunk-level settlements**: pointer to this plan's §3/§4 as the
  normative stage-1 settlements (the ADR names decisions, not code).
- **Consequences**: mixed-fleet relay guarantees; divergence surfacing
  bounds (collision direction analysis); whole-document re-PUT growth
  bounded at journal scale with the reserved chunk kind as the
  measured-need escape (never built speculatively); the
  `preserve_order` hazard and its immunisation; chunks 1–2
  journal-independent, 3 gated on the v0.2 journal entity.

## 7. Risks

- **The content tiebreak is novel** (not in any origin doc verbatim):
  the design's `(clock, author, entry-id)` cannot resolve the
  equal-(clock, author) same-key tie; `Value::Ord` is the smallest
  deterministic totaliser and keeps the fold a pure `max` (laws hold by
  construction). If B-full later rules a different tie rule, it lands
  as a new fold version behind the format gate — v1's rule is
  self-consistent and converged-fleet-stable.
- **`clocked > verbatim` is an interpretation** of "never ordered,
  never folded": recorded with rationale (B-full decode propagation;
  floor compatibility). The alternative (verbatim outranks) would
  freeze old peers' blind spots over newer peers' understanding —
  rejected.
- **serde_json `preserve_order` feature leakage** through some future
  dependency: immunised by `canonicalise` (D-FC7) and pinned by the
  Commit-4 stability test; the residual risk is nil for the hash,
  cosmetic elsewhere.
- **FNV-1a 64 collisions mask divergence** at ≈2⁻⁶⁴/pair: accepted
  (freshness findings still surface a stale peer even if its hash
  collides — the alarm is a second line, not the only one).
- **Body-preservation grows documents** if peers pile unknown fields
  into journal bodies: bounded at journal scale (options report §7);
  the reserved chunk kind remains the measured-need escape.
- **`DeviceId::mint()` nondeterminism**: confined to identity creation
  (chunk 2 consumes it); the fold/codecs never call it, so all laws
  stay deterministic.

## 8. References

- Design round: `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  (§3 Parts A–B, §5, §7); rulings on #626; dispatch plan §2; issue #629.
- Backbone: `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  §5.4/§5.5/§7.1–7.2 (layout lineage, naming/temp-file discipline).
- Passenger contract: `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md` §3.
- In-tree: `crates/taskboard-domain/src/appdata.rs` (the row recipe and
  union precedents, including the tie-rule contrast), `taskboard-app/src/ipc/mod.rs`
  (the FNV-1a rationale), `crates/taskboard-domain/Cargo.toml` (manifest
  + `test-support` precedent), `docs/adr/0019` (ADR house style).
