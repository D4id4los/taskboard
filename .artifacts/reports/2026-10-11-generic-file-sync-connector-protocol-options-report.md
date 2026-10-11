# [Report] Generic File-Sync Connector — Sync Protocol Options

- **Date**: 2026-10-11
- **Context**: The 2026-10-10 backlog entry *"Stand-alone-first product shape: optional,
  pluggable sync connectors"* names a plain-file backend on a synchronized folder
  (Dropbox, OneDrive, ownCloud/Nextcloud desktop client, …) as the second connector
  after Nextcloud Deck (ADR 0010). The user's brief for that connector: the app gets
  its own private folder inside a sync root (no third-party writers), but **no
  guarantee when a sync goes through** — 10–20 minutes is typical with default
  client settings, and the connector must work across arbitrary sync clients. In
  exchange, both read-sides are ours, so the file format can carry per-field
  information at any level of detail. This report analyses which sync-protocol
  designs survive that trade, and recommends a backbone.
- **Method**: Derived a transport model ("what a file-sync folder actually
  guarantees") and a requirements list from the brief; surveyed seven protocol
  options plus two rejected sidebars and scored them; grounded everything in the
  existing codebase — `TaskClocks` per-field LWW (ADR 0004), the outbox
  (`LocalOp`/`PendingOp`), the ADR 0007 sync cycle, and ADR 0010's connector
  vocabulary. External facts verified 2026-10-11: `yrs` 0.28.0 and `automerge`
  0.12.0 release dates via crates.io; DecSync repo layout and transport claims;
  Syncthing's mtime-preservation from the official FAQ.

---

## 1. Executive summary

The transport's one non-negotiable hazard is this: **when two devices write the
same file path between syncs, the sync client resolves it either by keeping both
as sibling "conflict copies" or by silently picking a winner — and which family
a client belongs to varies by product, version, and configuration.** Everything
else about the medium (slow, unordered, duplicated delivery) is annoying but
benign to a correctly designed protocol. Same-path concurrent writes are the
only thing that can *silently destroy data*.

That yields the central design rule: **every file in the folder must have exactly
one writer device for all time.** Designs that obey it are immune to both
conflict families by construction; designs that don't are only as safe as the
client's conflict behavior.

The second finding is architectural: **most of the protocol already exists in
the domain.** ADR 0004's per-field LWW with `TaskClocks` (8 clocks per task) is
exactly the "per-field information" the brief offers, and it is already a
convergent merge rule — *provided* two gaps are closed that the Deck server
used to paper over: field clocks carry no authoring device (equal or skewed
wall clocks between devices are ambiguous), and LWW needs a total order
(deterministic tie-break) to guarantee convergence.

**Recommendation**: a DecSync-style backbone — per-device *immutable* operation
chunks plus periodic per-device snapshots, folded by the existing per-field LWW
extended with author tags — wrapped in the existing `SyncCommand`/`SyncReport`
vocabulary so the engine stays connector-blind (ADR 0010). The connector is a
translator between the file medium and the sync cycle the engine already
speaks.

| # | Option | Verdict |
|---|--------|---------|
| 5.1 | Monolithic state file (incl. "sync the SQLite db") | **Unfit** — whole-state conflicts, unmergeable |
| 5.2 | Per-entity files, naive LWW / mtime | **Unfit** — silent loss, deletion ambiguity |
| 5.3 | Per-entity files, shared paths, embedded clocks | **Conditional** — correct only if clients are pinned to the conflict-copy family |
| 5.4 | Per-device *current-state* files (path-owned) | **Fit** — simplest transport-safe design; no history, weaker self-healing |
| 5.5 | Per-device immutable op-chunks + snapshots | **Recommended** — transport-immune, auditable, self-healing, maps onto the outbox |
| 5.6 | Embedded CRDT library (`yrs`/`automerge`) | **Viable alternative** for the merge engine; poor fit while ADR 0004/0006 stand |
| 5.7 | Leader/coordinator device | **Unfit** — round trips over a minutes-latency medium kill offline-first |

The one product decision that could change this recommendation is whether the
connector promises "works with *any* sync client" (the brief as written) or
"works with these tested clients" — see Open Questions §10.1.

---

## 2. The problem, precisely

### 2.1 Deployment model and assumptions

- Multiple devices (desktops, kiosks) run the taskboard app against one shared
  folder replicated by an arbitrary third-party sync client.
- The folder is private to the app: no third-party *logical* writer. But note
  the multi-device case means the folder overall has **N writers** — "sole
  writer" holds per path, not per folder, and that distinction is the whole
  game.
- Both read-sides are ours: file formats, metadata, identity, clocks — all
  freely designable. The *only* uncontrolled component is the sync client.
- Single user, multiple devices. There is no authentication surface between
  devices; anyone with write access to the folder is trusted (§7.10).

### 2.2 The file-sync network model

This is the contract any protocol must be programmed against. It is best
understood as a distributed-systems network model, not a filesystem:

- **T1 — Eventual, unbounded delivery.** A file written locally arrives on
  other devices after an arbitrary delay: seconds, minutes (the 10–20 min
  baseline in the brief), or days for a paused/offline client. The protocol
  must be latency-agnostic; the UI must communicate peer freshness instead of
  assuming immediacy.
- **T2 — No cross-file ordering.** Arrival order of *different* files is
  arbitrary (batching, retries, bulk backfill). Multi-file updates are never
  atomic; a reader can observe any interleaving.
- **T3 — Per-file visibility is *mostly* atomic.** Clients materialize
  downloads via temp-write + rename, so readers usually see whole files or old
  files. They must not *rely* on it: torn/partial files are possible
  (interrupted materialization, exotic filesystems, user copies via USB).
  Every file is validated and checksummed before it is applied.
- **T4 — Same-path concurrent writes fork or lose.** When two devices write
  the same path between syncs, the client resolves it either by (a) sibling
  conflict copies — both kept, client-specific naming — or (b) a silent
  winner (LWW-by-mtime or opaque). The family varies per client, version, and
  configuration, and the brief demands support for arbitrary clients. This is
  the only hazard that causes silent data loss.
- **T5 — mtimes are unusable as protocol data.** Clock skew between devices,
  filesystem timestamp granularity, and per-client preservation policies make
  mtime worthless for ordering. (Syncthing does preserve mtimes — verified —
  but the protocol must not depend on any one client doing so.)
- **T6 — Deletion is indistinguishable from not-yet-arrived.** The absence of
  a file carries no information. Logical deletion must be explicit data
  (tombstones).
- **T7 — No transactions, no rename atomicity.** A rename may arrive as
  delete + create; two files never change together.
- **T8 — Capacity and churn cliffs.** File counts and change rates hit
  provider throttles and quotas; ignore/exclusion mechanisms vary by client
  and are user-configured, not app-controlled.
- **T9 — The medium is semi-hostile.** Users hand-edit files, backup and
  antivirus tools touch them, disk images clone device identities, providers
  scan and deduplicate content, and a user can duplicate the whole folder into
  a second sync root.

The brief's phrasing — "only the file sync is random" — is exactly right, and
it dictates the strategy: *quarantine all nondeterminism inside the delivery
layer*, and make the protocol's outcome independent of delivery order,
duplication, and timing. That means commutative, idempotent, deterministic
merging on every reader.

### 2.3 What transfers from the current architecture — and what does not

The existing sync design was built against a Deck server. Most of it
transfers; three assumptions do not.

**Transfers unchanged:**

- **Per-field LWW merge semantics (ADR 0004).** `TaskClocks` (title,
  description, duedate, done, position, labels, archived, deleted), plus
  `StackClocks` and `LabelClocks`, are already a state-based convergent merge
  rule. The file connector's wire format can persist these clocks verbatim.
  The brief's "any detail of per-field information we want" is not a future
  promise — it is the shipped domain model.
- **The outbox as the local write-ahead queue.** `PendingOp`/`LocalOp` ops
  accumulate while offline and drain when a connector is attached — ADR 0010
  already requires this to work, and the file connector is the purest
  expression of it.
- **The sync cycle shape (ADR 0007).** read → push → pull → report maps
  directly: read the engine state, drain the outbox into a published chunk
  (push), scan and fold peers (pull), report a snapshot.
- **Connector-blindness (ADR 0010).** The engine consumes `SyncReport` and
  emits outbox ops without knowing the backend; the file connector slots into
  the same `SyncCommand`/`SyncReport`/`SyncStateReader` vocabulary.

**Does not transfer:**

1. **Single clock authority.** Deck's `last_modified` comes from one server —
   a quasi-coherent ordering source and the merge point. A folder has N device
   wall clocks and *no* merge point; merging happens on every reader. Two
   consequences: (a) a skewed device wins every LWW duel it participates in
   regardless of true edit order; (b) two devices with equal wall-clock
   readings produce *tied* field clocks, and a bare `DateTime<Utc>` cannot
   break the tie deterministically — a convergence bug, not just a fairness
   bug. Both require the domain extension in §7.3 (author-tagged clocks +
   total-order tie-break).
2. **Deck-shaped remote identity.** `RemoteBoardId`/`RemoteCardRef` are Deck
   `u64` ids; `BoardPullValidators` are Deck etags. A file connector has
   neither and must synthesize them or the vocabulary must widen (§7.4).
3. **Network authority.** The Deck actor *is* the network authority and emits
   `SystemEvent::NetworkLost/Restored`. A file connector cannot see the sync
   client's health at all — it only observes file arrivals and heartbeat
   freshness. It must not claim network authority (§7.4).

One more nuance the transport makes more visible: `labels` is a `BTreeSet`
with a **single** clock — whole-set LWW, not per-element OR-set semantics. A
concurrent `AssignLabel` on device A and `UnassignLabel` on device B resolves
as a set duel (one side's set wins wholesale). This is existing ADR 0004
granularity, unchanged by the transport, but true multi-device concurrency is
now the norm rather than the exception, so it is worth documenting as a known
coarse resolution (§7.3).

---

## 3. Requirements

- **R1 Convergence** — all devices reach identical state from the same set of
  delivered files, regardless of order, duplication, or re-delivery.
- **R2 No silent data loss** — outside explicitly documented LWW duels (and
  deletions), no edit may vanish.
- **R3 Offline-first** — local edits apply instantly with zero coordination;
  sync is a background effect. (Non-negotiable per the product shape.)
- **R4 Deletion correctness** — deletes survive late-arriving peers, and
  delete-vs-edit follows the existing clock-duel semantics.
- **R5 Late-join efficiency** — a new device reaches current state without
  replaying all history.
- **R6 Determinism** — the merge is a pure, total function of its inputs
  (required by R1, and by the project's proptest/insta culture).
- **R7 Version tolerance** — devices run different app versions; newer
  formats are parked and surfaced, never misapplied.
- **R8 Garbage tolerance (T9)** — unparsable, torn, or hand-edited files are
  quarantined and reported, never crash or corrupt state.
- **R9 Bounded resources** — kiosk-tractable RAM, scan cost, and bandwidth.
- **R10 Observability** — per-phase sync status, peer freshness, divergence
  alarms.

---

## 4. The design space

Two orthogonal axes determine everything:

1. **Path ownership** — who may write a given file path:
   *shared* (any device), *per-device* (one writer per path), or *immutable*
   (written once, never modified).
2. **Merge semantics** — how concurrent logical updates reconcile:
   *none/LWW-by-mtime*, *three-way*, *per-field LWW with clocks* (what the
   domain has), or *CRDT library*.

T4 (§2.2) rules out the shared+weak-merge corner outright. The interesting
designs are all per-device or immutable on the path axis; the merge-engine
choice then rides on top. Note that the merge engine is largely settled by
ADR 0004 — per-field LWW — so the real question the connector faces is the
file layout, plus the clock-authority repair.

---

## 5. Option analyses

### 5.1 Monolithic state file (incl. "just sync the SQLite database")

**Mechanism.** One file holds the entire state (JSON dump or the SQLite db);
rewritten on every change; conflict copies merged at scan time by the app.

**Benefits.** Trivial to implement; single-file backup; completeness is
obvious.

**Drawbacks.** With 10–20 minute propagation, concurrent edits are the *norm*
for multi-device use, and every concurrent edit — even on unrelated tasks —
conflicts the same file. Under conflict-copy clients the app must deep-merge
two whole documents (which requires per-field metadata, i.e., everything 5.3+
provides anyway); under silent-winner clients, entire sessions of work vanish.
Bandwidth is O(total state) per edit. The SQLite variant is worse: WAL/`-shm`
sidecars must never sync, a torn upload corrupts pages, and two divergent
database files are practically unmergeable. This is the KeePass-over-Dropbox
failure mode (Appendix A).

**Verdict: unfit as a backbone.** Its only legitimate role is as the snapshot
payload inside 5.5.

### 5.2 Per-entity files, naive LWW (mtime or per-file version counter)

**Mechanism.** One file per task/stack/label, named by UUID; newest version
wins; deletion = file gone.

**Drawbacks.** mtime ordering is garbage (T5); a bare per-file version has
equal-version ambiguity between devices; deletion is indistinguishable from
not-yet-arrived (T6) — a device that has not yet received a task's file will
"resurrect" nothing, but a device whose copy is stale will re-create it;
silent-winner clients lose same-task edits (T4).

**Verdict: unfit.** Worth naming only because it is what one gets with zero
protocol design; it is the Obsidian-over-Dropbox status quo (Appendix A).

### 5.3 Per-entity files, shared paths, embedded clocks ("state-based CRDT-lite")

**Mechanism.** `tasks/<uuid>.json` written by *any* device; the file is the
materialized entity with per-field clocks and author tags; tombstone file for
deletes. On scan, the app LWW-merges the canonical file plus any conflict
copies it finds, then writes the merged file back.

**Benefits.** The smallest step beyond 5.2 that is coherent: a task file is
literally a serialization of `Task` + `TaskClocks` + authors. Human-readable,
hand-recoverable, trivially debuggable (the user's per-field detail is on
disk in plain sight). Conflicts are localized per entity; unrelated tasks
never conflict; incremental uploads per changed entity.

**Drawbacks.** Paths are co-owned → T4 exposure. Under conflict-copy clients
it works — *if* the scanner can recognize client-specific conflict names
(parse any file matching the UUID pattern regardless of suffix). Under
silent-winner clients, a same-task concurrent edit is lost, and so is the
merge write-back itself (the merged file can be raced by another device's
concurrent merge). Tombstone files share the co-owned path. The layout is
only as safe as the pinned client list.

**Verdict: fit, conditional** — correct only if the supported-client promise
is narrowed to verified conflict-copy clients (Dropbox, Syncthing, Nextcloud
desktop all behave this way; OneDrive and Google Drive are not guaranteed —
Appendix B). The cheapest correct-enough option under that constraint.

### 5.4 Per-device *current-state* files (path-owned, no history)

**Mechanism.** Same materialized-clock content as 5.3, but each device writes
only its own subtree: `devices/<id>/tasks/<uuid>.json` holds *this device's
last known value* for each field; tombstones live in
`devices/<id>/tombstones/<uuid>.json`. The global state is the per-field LWW
fold across all devices' files.

**Benefits.** First design that is **immune to T4 by construction**: no path
has two writers, so conflict families are irrelevant — this works with *any*
client. No journals, no sequence numbers, no compaction: late join = scan
everything and fold; each file is current. File count is O(entities ×
devices that edited them), acceptable at taskboard scale.

**Drawbacks.** No history or audit (a file overwrite discards the previous
value); weaker self-healing: if a file is torn in transit and quarantined by
a reader (T3), the data is stranded until the owner happens to edit that
entity again — the owner cannot know the remote copy is bad. Overwriting
one's own file also means a moment where the folder holds neither the old
nor the new version for that path (rename discipline narrows but does not
eliminate odd windows for readers on the same machine).

**Verdict: fit** — the simplest transport-immune design, and a legitimate
lean v1 if history is not wanted. Dominated by 5.5 as soon as audit,
self-healing, or divergence forensics matter.

*(A variant sometimes proposed — one append-only log file per device — is
dominated by both: append-in-place requires torn-tail tolerance on readers,
re-uploads the whole growing file per append unless the client block-deltas,
and compaction of a live log rewrites a path, reintroducing T4 unless the
log is rotated into immutable files — at which point it is 5.5.)*

### 5.5 Per-device immutable op-chunk journals + snapshots (DecSync-style)

**Mechanism.** Each device owns `devices/<id>/` and publishes **immutable**
chunk files — never modified, never renamed, deleted only by their owner.
A chunk carries a batch of materialized entity states (fields + clocks +
authors) flushed from the outbox. Periodically the device publishes a
*snapshot* (its converged view at sequence N) and may then delete its own
chunks ≤ N. A heartbeat `info.json` carries freshness, app version, and a
state hash. Every device computes the global state as a deterministic fold
over the union of all snapshots and chunks. Full layout and schemas in §7.

**Benefits.**

- **Transport immunity by construction.** No path is ever written by two
  devices; the medium degenerates to a set-union of immutable files. Order,
  duplication, and delay become irrelevant (T1–T3, T7 fall away); conflict
  copies of our own files can only arise from identity duplication (disk
  clone / folder fork), which `instance_id` detects loudly — and even then,
  folding both copies is *correct*, because the fold is clock-driven, not
  name-driven. A user who duplicates the whole folder into a second sync root
  gets convergence when the roots eventually merge, not corruption.
- **Direct outbox mapping.** Chunks are the drain target of the outbox: the
  connector materializes current field values + clocks at flush time —
  exactly what `plan_pushes`' `MaterializedOp`/`CardShape` already produces
  for Deck pushes. `PushOutcome::Applied` maps to "included in a published
  chunk".
- **Audit and forensics for free.** The chunk stream is a per-device history
  of every materialized write — the "per-field information at any level of
  detail" the brief offers, including *when* and *by which device*.
- **Self-healing.** A torn or quarantined chunk's data is recovered by the
  owner's *next snapshot* (snapshots subsume all prior chunks), so
  transient-medium damage ages out.
- **Divergence detection.** Heartbeat state hashes over the converged fold
  make "two devices disagree after quiescence" an observable, alarmable
  event instead of a silent split.
- **Local-first, trivially.** Publishing a chunk is fire-and-forget; nothing
  ever blocks a local edit.

**Drawbacks.**

- The largest bespoke surface of the viable options: chunk lifecycle,
  sequence watermarks, snapshot cadence, scanning, quarantine, GC rules.
- The wire format is less self-evident than per-entity files (the state of
  task X is a fold, not a file you can open) — mitigated by JSON encoding
  and by snapshots, which are human-readable whole-state documents.
- Determinism bugs in the fold equal divergence. Mitigations: the fold is
  pure domain code (proptest-able to the same standard as the rest of the
  domain), plus the heartbeat state-hash alarm.
- One conceptual subtlety: sequence numbers exist only for GC watermarks,
  never for ordering — reviewers and future maintainers must not "fix" the
  fold into seq order.

**Verdict: recommended backbone.** It is the only option that is
transport-immune *and* keeps history, and it maps onto the existing outbox /
sync-cycle / per-field-LWW machinery with minimal conceptual distance.

*(A further variant — content-addressed chunk names (hash of payload) — adds
de-duplication but requires a manifest indirection and reference-counting GC;
sequence-named immutable chunks are simpler and equally safe. Not
recommended.)*

### 5.6 Embedded CRDT library (`yrs` / `automerge`)

**Verified status (2026-10-11):** `yrs` 0.28.0 (released 2026-09-17, y-crdt
repo, actively maintained); `automerge` 0.12.0 (released 2026-09-16,
first-party Rust crate). Both are healthy. The `crdts` primitives crate
(LWWReg/ORSWot/vclock) has been unmaintained since 2023-08 — do not take it
as a dependency bet.

**Mechanism, two integration shapes.**

- (a) *Whole-board CRDT document as a file.* The board is one CRDT doc
  (yrs/automerge); every edit produces a new file version; conflict copies
  are merged via the library's `merge`.
- (b) *CRDT changes as per-device immutable files.* Automerge's incremental
  change sets (or yrs updates) are appended as immutable per-device chunks —
  this is 5.5's layout with the library owning the fold.

**Benefits.** Correctness off the shelf — registers, sequences, tombstones,
and causal metadata are the library's job; shape (b) inherits 5.5's transport
immunity; if a `description` field ever needs character-level concurrent
merge, sequence CRDTs are already in the box.

**Drawbacks.** The architectural fit is poor *for this codebase*: the app
already has a merge engine (ADR 0004 per-field LWW, applied through
`apply_sync_report`). Embedding a CRDT document demotes `AppState` to a
projection of an opaque document, orphans `TaskClocks`, and collides with
ADR 0006's "State Engine is the single source of truth" — the CRDT doc would
*be* the state. Shape (a) additionally re-introduces co-owned paths (T4
exposure) and whole-document churn per edit. The binary formats are opaque
(the domain's debuggable per-field clocks were a deliberate choice), and
both libraries have a history of wire-format churn across major versions.
Shape (b) is defensible but duplicates what 5.5 + the existing LWW already
deliver, at the cost of a heavy dependency and a second state model.

**Verdict: viable alternative, not recommended now.** Keep it as the
documented escape hatch: if future requirements (character-level text merge,
dense concurrent editing, rich offline collaboration) outgrow per-field LWW,
shape (b) slots into the 5.5 layout without changing the transport story.

### 5.7 Leader / coordinator device

**Mechanism.** Elect one device (e.g., the desktop) as serializer; others
submit intent files; the leader orders everything and publishes canonical
state.

**Benefits.** Total order; conflict resolution becomes trivial; no merge code.

**Drawbacks.** Every interactive write waits a transport round trip — tens of
minutes — which kills offline-first (R3) outright; a kiosk partitioned from
the leader stalls completely; leader failure detection is unsolvable over a
medium with no liveness signal. Note that its one good idea — a total order
— is exactly what the *deterministic fold* provides ("virtual coordinator")
without any availability cost.

**Verdict: unfit.**

### 5.8 Sidebar — CRDT-enabled databases (cr-sqlite and friends)

"Make the SQLite file itself the CRDT" does not fit: those systems assume
*their own* transport (peer connections or a sync server), not an arbitrary
file-sync client; exposing the db file to the medium reintroduces every 5.1
hazard (sidecars, page-level divergence, whole-file loss). The SQLite store
stays exactly where ADR 0005 put it — a local cache behind the engine — and
the synced folder stays a protocol artifact, not a database file.

---

## 6. Comparison matrix

✓ = satisfied, ~ = partial/conditional, ✗ = fails.

| Criterion | 5.1 monolith | 5.2 naive | 5.3 shared+clocks | 5.4 per-device state | 5.5 chunks+snaps | 5.6 CRDT lib | 5.7 leader |
|---|---|---|---|---|---|---|---|
| Safe under silent-winner clients (T4) | ✗ | ✗ | ✗ | ✓ | ✓ | ~ (shape a ✗, b ✓) | ✓ |
| Safe under conflict-copy clients | ~ | ✗ | ✓ | ✓ | ✓ | ~ (a ✓, b ✓) | ✓ |
| Convergence guarantee (deterministic) | ✗ | ✗ | ~ (needs author tags) | ✓ (same) | ✓ | ✓ | ✓ |
| Deletion correctness (T6) | ✗ | ✗ | ~ (tombstone file, co-owned path) | ✓ | ✓ | ✓ | ✓ |
| Late-join cost (R5) | ✓ | ✓ | ✓ | ~ (full scan) | ✓ (snapshots) | ✓ | ~ |
| Offline-return safety (R4) | ✗ | ✗ | ~ | ✓ | ✓ | ✓ | ✗ |
| Bandwidth/churn per edit | ✗ | ✓ | ✓ | ✓ | ✓ | ~ (a ✗, b ✓) | ✗ |
| Offline-first, zero-coordination writes (R3) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ |
| Bespoke protocol surface | small | small | medium | medium | large | small* | medium |
| Human debuggability / hand-recovery | ~ | ✓ | ✓✓ | ✓ | ~ (JSON + snapshots) | ✗ (binary) | ~ |
| History / audit / undo | ✗ | ✗ | ✗ | ✗ | ✓ | ✓ | ✗ |
| Fit with existing domain (clocks, outbox, `SyncReport`) | ✗ | ~ | ✓✓ | ✓✓ | ✓✓ | ✗ (replaces the engine's role) | ~ |

\* small only because the library absorbs it — at the price of the
architectural collisions in 5.6.

---

## 7. Recommendation — reference design

### 7.0 Thesis

Transport immunity comes from **path ownership**; merge semantics already
exist in the domain (ADR 0004). Therefore the connector is three layers:
(a) a file layout with strict path ownership, (b) a deterministic fold into
per-field LWW (the domain's own rule, author-tagged), and (c) a translation
into the `SyncCommand`/`SyncReport` vocabulary the engine already consumes —
the connector is a *translator*, and the engine stays connector-blind
(ADR 0010).

### 7.1 Layer 0 — filesystem discipline

- **Write discipline:** write `<name>.tmp-<uuid7>` → `fsync(file)` →
  `atomic rename` → `fsync(dir)`. Never modify a published file in place;
  never rename a published file. `.tmp-*` files are never parsed (a crash
  between write and rename can leak one; a periodic sweep removes own-device
  temporaries).
- **Naming:** lowercase `[a-z0-9._-]`, UUIDv7 identifiers (the domain's
  `UuidV7Generator` already provides these), directory depth ≤ 4, path
  length ≤ ~200 chars (Windows default limits), no names that collide under
  case-insensitive filesystems.
- **Read discipline:** parse → validate against the schema → apply. Anything
  unparsable or torn is quarantined (copied *into a local, non-synced
  directory* — readers never move or delete other devices' files, since that
  would violate path ownership) and surfaced in sync status (R8).

### 7.2 Layer 1 — folder layout

```
<synced-root>/taskboard/
├── identity.json                     # created once by the first device
├── devices/
│   └── <device-id>/
│       ├── info.json                 # heartbeat, rewritten by owner only
│       ├── ops/
│       │   ├── 000001.json           # immutable chunks
│       │   └── 000002.json
│       └── snapshot-000128.json      # owner's converged view at seq 128
└── (quarantine lives OUTSIDE the sync root, per-device, local-only)
```

**`identity.json`** — `{ folder_id: uuid7, created_at, format_version }`.
Created once (create-if-absent at first attach; adopt if present). Ties the
folder to one logical board. A user who duplicates the folder into a second
sync root still converges when the roots merge (the fold is a set union).

**`devices/<id>/info.json`** — single-writer heartbeat, refreshed on every
flush and snapshot:

```json
{
  "device_id": "018f6a52-…",
  "instance_id": "018f7b01-…",
  "app_version": "0.2.0",
  "format_version": 1,
  "last_seq": 141,
  "last_clock": "2026-10-11T09:15:32Z",
  "state_hash": "sha256:…",
  "written_at": "2026-10-11T09:15:32Z"
}
```

Purposes: peer-freshness display (heartbeat age), divergence detection
(`state_hash` disagreement persisting past quiescence → alarm + guided
resync), version negotiation (R7), and **clone detection** — a chunk whose
`instance_id` disagrees with the owning device's `info.json` means a disk
image was cloned: the discovering device refuses to trust the pair, writes
nothing into that subtree, and surfaces the problem loudly.

**`devices/<id>/ops/NNNNNN.json`** — immutable chunk, one batch of outbox
drain:

```json
{
  "format_version": 1,
  "device_id": "018f6a52-…",
  "instance_id": "018f7b01-…",
  "first_seq": 129,
  "last_seq": 141,
  "written_at": "2026-10-11T09:15:32Z",
  "entries": [
    {
      "kind": "task",
      "id": "018f6a44-…",
      "fields": {
        "title":       { "value": "Ship v0.1",
                         "clock": "2026-10-11T09:15:31Z", "by": "018f6a52-…" },
        "description": { "value": "",
                         "clock": "2026-10-09T18:02:11Z", "by": "018f5c9e-…" },
        "position":    { "value": { "stack": "018f6a40-…", "order": 3 },
                         "clock": "2026-10-11T08:44:00Z", "by": "018f6a52-…" },
        "labels":      { "value": ["018f6a41-…"],
                         "clock": "2026-10-10T11:20:00Z", "by": "018f5c9e-…" },
        "deleted":     { "value": false,
                         "clock": "2026-10-08T10:00:00Z", "by": "018f5c9e-…" }
      }
    }
  ]
}
```

Two deliberate choices:

- **Materialized states, not raw `LocalOp`s.** `LocalOp` is a dirty-set
  reference (`UpdateTask(TaskId)`) meaningful only against base state;
  chunks must be self-describing for late joiners (R5) and version-tolerant
  readers (R7). The connector materializes current field values + clocks at
  drain time — precisely the `MaterializedOp`/`CardShape` pattern the Deck
  push path already uses. Untouched fields ride along with their existing
  clocks and authors.
- **`by` on every field.** The author of a field's current value may differ
  from the chunk's publisher (a device re-publishes fields it did not write
  when it re-materializes an entity). Field-level authorship is what makes
  the fold total (§7.3).

**`devices/<id>/snapshot-NNN.json`** — the owner's converged fold of
*everything it has seen* up to its own sequence N (all devices' data, not
just its own — still single-writer, since it is this device's view).
Snapshots serve late join (floor for replay), self-healing (subsumes the
owner's prior chunks, recovering any torn-in-transit chunk), and GC (below).

**GC rules — deliberately membership-free:**

- The owner may delete its own chunks ≤ N once `snapshot-NNN` exists. No
  acks, no device registry: any reader reconstructs from snapshot + later
  chunks.
- Tombstones (entries with `deleted: true`) are **kept forever in v1**. At
  taskboard scale they are hundreds of bytes each; years of deletes are
  trivial. This is not just deletion bookkeeping — tombstones are the
  *completeness mechanism* (§7.5) and the only defense against resurrection
  by a device that was offline longer than any horizon. A horizon knob is
  documented future debt, not a v1 feature.
- Retiring a decommissioned device is a **user procedure, not automatic**:
  quiesce → each remaining device publishes a snapshot (absorbing the
  leaver's data and tombstones) → delete the leaver's subtree → verify all
  `state_hash`es agree.

### 7.3 Layer 2 — the merge engine: existing per-field LWW, repaired

The fold is the domain's own rule, made total:

- Per field across all snapshots and chunks: the winner is
  `max by (clock, author_device_id)` — wall clock first, author id as the
  deterministic tie-break. Commutative, associative, idempotent ⇒ R1/R6
  hold under any delivery order (T1/T2), duplication, or re-delivery.
- **Required domain extension (the report's one non-negotiable code
  change):** `TaskClocks`/`StackClocks`/`LabelClocks` gain the authoring
  device id per field (a parallel `TaskAuthors` struct or a
  `(DateTime<Utc>, DeviceId)` pair). Today a bare `DateTime<Utc>` cannot
  break ties between devices — and ties *will* happen (equal seconds-resolution
  clocks, restore-from-backup) — so without author tags the fold is
  nondeterministic, i.e., divergent. This extension also fixes attribution:
  "who set this field" becomes answerable, which is exactly the per-field
  detail the brief wants.
- Position (`stack` + `order`, one shared clock) keeps its existing
  semantics: concurrent moves → one wins. Labels keep whole-set LWW
  (existing granularity, §2.3); an OR-set upgrade is a documented option if
  label-churn conflicts prove real in use.
- Delete-vs-edit is the existing `deleted`-clock duel (ADR 0004), unchanged.
- **Clock skew:** convergence requires a *total* order, not an *accurate*
  one — device-tagged LWW converges under arbitrary skew. Fairness (which
  edit *should* win) degrades with skew; NTP-managed desktops are fine, and
  kiosks must run a time-sync daemon. Skew beyond a threshold is detectable
  (heartbeat `written_at` vs observation) and should be surfaced, not
  silently tolerated.
- **Optional fairness upgrade (later, product decision):** carry the
  *read-base* clock per field write, letting the fold *detect* concurrent
  overwrites and surface "edited on two devices" while still resolving
  deterministically. Not required for convergence.

### 7.4 Layer 3 — translation into the engine's vocabulary

The connector speaks ADR 0007's cycle in ADR 0010's vocabulary:

- **read** — `SyncStateReader::read_state()` (unchanged).
- **push** — drain the outbox; publish a chunk containing the materialized
  entities; report `PushOutcome::Applied` per op. The standalone-era outbox
  that ADR 0010 requires to drain safely does so as an ordinary chunk.
- **pull** — scan (notify-triggered + periodic), fold all peers' snapshots
  and chunks, produce the converged view.
- **report** — synthesize `SyncReport::Completed { snapshot, … }` from the
  fold.

Mapping details:

- **The completeness contract translates, via tombstones.** Deck's "absence
  from the snapshot = gone server-side" becomes "tombstone present in the
  fold = gone". Because tombstones are retained forever, the fold contains
  every entity ever known (live or deleted), so absence is never ambiguous —
  a partially-arrived peer's data yields an *older* view of that peer's
  entities, never a spurious deletion. This is the structural reason 5.5
  keeps tombstones forever (§7.2).
- **Remote views:** fold entities map onto `RemoteBoard`/`RemoteStack`/
  `RemoteTask`/`RemoteLabel` with `last_modified` = the entity's max field
  clock (seconds resolution, matching Deck's). `remote_seen` and validators
  synthesize naturally (`etag` = the fold's `state_hash`).
- **Board identity:** one folder = one board (the single-board invariant
  holds). `identity.json`'s `folder_id` synthesizes a stable
  `RemoteBoardId` (e.g., folded to `u64`); the `BindBoard`/`SetBoard`
  ceremony collapses into config-declared attachment at bootstrap.
- **Two integration choices — a real decision point (§10.3):**
  (a) synthesize Deck-shaped ids/validators — zero domain change, ships
  fastest, semantically awkward; (b) widen the vocabulary
  (connector-tagged remote ids, a source tag on `SyncReport`, possibly
  promoting the port to the domain trait ADR 0010 anticipated) — cleaner,
  small domain touch, appropriate once the connector is committed to.
- **Network events:** this actor is **not** a network authority. It must not
  claim `NetworkLost`/`NetworkRestored` from folder readability — a healthy
  folder with a paused sync client looks identical to a syncing one. Peer
  freshness belongs in `SyncStatus`/UI (heartbeat ages). There is a real
  vocabulary gap here ("stale peers" is neither Idle nor Offline) — noted
  as an open question (§10.6).
- **Wiring:** a new crate `taskboard-sync-filesync` mirroring
  `spawn_sync_actor`'s signature; config gains
  `[sync] backend = "filesync"` + a root path (figment chain unchanged);
  `poll_interval` becomes scan cadence; backoff covers scan I/O errors.
  Watching: the `notify` crate (debounced) plus a periodic full-scan
  reconcile — inotify fires when the sync client materializes remote files
  locally, but queue overflow and missed events over months of kiosk uptime
  make the periodic reconcile mandatory, not optional.

### 7.5 Deletion, tombstones, and resurrection — the worked scenarios

- *Device deletes a task; a peer edited it concurrently.* The
  `deleted`-clock duel resolves it (existing semantics); the loser's edit is
  recoverable from the audit chunks.
- *Device returns after being offline for months.* Its own old chunks/snapshots
  are still in the folder (they synced before it left); everyone else's
  tombstones are still there (kept forever); the fold reproduces the delete.
  No resurrection. This property is *why* tombstone GC is deferred.
- *Folder duplicated into two roots, both used, later merged.* Both roots
  contain all pre-fork data plus each side's post-fork chunks; the merged
  folder is the set union; the fold converges on the clock-driven merge.
  Same-device/same-seq collisions from the fork are distinguishable by
  `instance_id` (loud alarm) and harmless to the fold (clock-driven, and the
  instances were one machine pre-fork, so their clocks are consistent).

### 7.6 Conflict copies and garbage

Because every published file is immutable and single-writer, *conflict
copies of our own files should not occur*; when they do (fork/clone), they
are valid chunks that fold correctly. Files that do not parse, fail
checksum, or carry an unknown `format_version` are quarantined locally and
surfaced ("peer runs a newer app" for the version case — never misapplied,
R7). Readers never delete or move other devices' files; the owner's
snapshot-GC eventually removes remote junk that falls below its watermark.

### 7.7 Bootstrap and late join

A new device: scan → fold everything → report → only then begin writing
(the fold-first rule exists so standalone-era outbox ops merge knowingly).
An empty folder creates `identity.json` and starts the board. A non-empty
folder *without* `identity.json` (wrong directory, selective-sync
misconfiguration) is refused loudly — per ADR 0010's loudness rule, no
silent backend flips.

### 7.8 Schema evolution

`format_version` on every file; entry kinds evolve additively; wire formats
*ignore* unknown fields (a deliberate contrast with the config layer's
`deny_unknown_fields` — wire tolerant, config strict). Readers park
newer-major-version files and surface a version-negotiation notice via
`info.json`. Both directions degrade to a visible error, never a misapply.

### 7.9 Security and privacy

Single-user, trusted-folder threat model: any device with write access to
the folder is trusted; the protocol has no authentication surface (a
documented limitation, inherent to the medium). Providers see file counts,
sizes, timing, and (absent encryption) contents; sync providers scan and
deduplicate content. Optional at-rest encryption (e.g., `age` or libsodium
sealed payloads) is a compatible later phase — the fold operates
post-decrypt. No secrets ever live in the folder (credentials stay in the
`CredentialStore`, ADR 0008).

### 7.10 Performance and kiosk budget

Memory: chunk index + fold tables, bounded by snapshot cadence (taskboard
scale: thousands of entities ≈ a few MB). Scan: O(file count) with
name+size fingerprinting between full parses. Bandwidth: coalesced chunks
(draining on a ~2 s debounce or 64-op cap) — one small file per edit
burst. Encoding: JSON for v1 debuggability; CBOR/compression is a measured
need, not a default.

### 7.11 New dependencies (stack approval required)

`notify` (watching), `sha2` (checksums/state hash). `serde`/`serde_json`
already flow through the domain's persistence serialization. Nothing
async-runtime-shaped enters the domain — the fold, codecs, and tie-break
are pure domain code; the connector crate stays `taskboard-state`-free,
like `taskboard-sync-nextcloud`.

### 7.12 Phasing (each phase = a plan doc per AGENTS.md §7)

- **F1 — domain repair:** author-tagged clocks + total-order tie-break,
  with proptest fold laws. Pure domain; valuable even if the connector
  never ships (it hardens ADR 0004).
- **F2 — protocol core:** identity/chunk/snapshot/heartbeat codecs, the
  fold, quarantine rules — a pure library crate + fake-transport property
  tests.
- **F3 — actor and wiring:** `taskboard-sync-filesync` (watch/scan/flush/
  GC), `SyncBackend::Filesync` bootstrap, dockerized Syncthing test tier.
- **F4 — hardening:** divergence alarms + guided resync, device-retirement
  procedure, optional encryption.

---

## 8. Fit with the testing strategy

The design maps onto `docs/testing_strategy.org` almost one-to-one, which is
itself evidence of fit:

- **Property tests (§4, proptest/256):** the fold laws are the crux —
  commutativity (`fold(S) == fold(any permutation of S)`), idempotence
  (`fold(S ∪ dup) == fold(S)`), associativity, tombstone dominance,
  tie-break totality, and snapshot-floor equivalence (late-join fold ==
  incremental fold). Garbage-input cases assert quarantine, not
  application (floors on garbage input are permitted; ceilings are not).
- **Fakes over mocks (§3):** a fake transport actor — an in-memory folder
  that delivers files with injected reordering, duplication, delays,
  silent-winner drops, and conflict copies, deterministically (seeded).
  No sleeps: the scanner runs on an injected tick with `tokio::time`
  paused/advanced (§6), and `written_at` uses the existing `Clock` seam
  (the `RetrySleep` pattern).
- **Snapshot tests (§5, insta):** `AppState` after scripted chunk storms —
  the existing engine-snapshot machinery, unchanged.
- **Black-box actor tests (§2):** drive the connector through its channels,
  assert typed outcomes only.
- **Fuzzing (§11):** chunk/snapshot/heartbeat parsers via cargo-fuzz —
  R8 is a fuzzing requirement in disguise.
- **Tiers (§8):** Tier 0 = recorded chunk fixtures; Tier 1 = the fake
  transport matrix (wiremock's role, file-flavored); Tier 2 = a
  dockerized **Syncthing pair** (syncthing has a REST API to add folders
  and await sync by predicate-polling — the direct analogue of the
  Nextcloud compose tier); Tier 3 = manual real Dropbox/OneDrive folders,
  `#[ignore]`d live tests behind an env var, skip-never-fail.
- **Mutation testing:** extend cargo-mutants scope to the fold and
  tie-break — the two functions where a mutant must die.

---

## 9. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Wall-clock skew between devices | Unfair LWW duels (wrong edit wins) | Convergence unaffected (total order); time-sync on kiosks; skew surfaced via heartbeats |
| Equal field clocks from two devices | Divergence (nondeterministic fold) | Author-tag tie-break — F1 must land before any connector ships |
| Disk-image clone duplicates a device identity | Path-ownership violation (T4 re-enters) | `instance_id` in chunks vs `info.json`; mismatch = refuse + loud alarm |
| User excludes `devices/` subtree via selective sync | Silent partial sync | Undetectable in general; detect own-subtree absence on peers, surface heartbeat staleness; document supported-client caveats |
| Torn/partial file materialization (T3) | Lost chunk data on readers | Checksum + quarantine; owner's next snapshot self-heals |
| Format-version deadlock between peers | Sync stalls | `info.json` negotiation surface; loud "peer runs newer app" status |
| Tombstone accumulation | Folder growth | Hundreds of bytes each; v1 accepts; horizon knob is documented debt |
| Chunk file explosion from edit bursts | Client throttling (T8) | Coalesced flush (debounce / op cap); snapshot GC |
| `.tmp-*` litter after crashes | Noise | Never parsed; periodic own-device sweep |
| Determinism bug in the fold | Silent divergence | Pure-domain fold + proptest laws + heartbeat `state_hash` alarms + quarantine/resync runbook |
| User hand-edits protocol files | Confusion, apparent corruption | R8: quarantine + surface, never apply; snapshots stay human-readable so recovery is manual-friendly |
| Provider content transformation/scanning | Privacy; rare corruption | Checksum detects transformation; encryption is a compatible later phase |

---

## 10. Open questions for the product owner

1. **Client commitment (the pivotal decision).** "Works with any sync client"
   (the brief as written) mandates a path-owned backbone — 5.5 (or 5.4).
   "Works with these tested clients" (conflict-copy family verified:
   Dropbox, Syncthing, Nextcloud desktop) would legitimize the cheaper 5.3
   shared-path design. Recommendation: keep the any-client promise; it is
   the connector's reason to exist next to the Deck connector.
2. **Silent resolution vs. surfaced soft conflicts.** Per-field LWW resolves
   silently. Surfacing "edited on two devices" needs the read-base-clock
   extension (§7.3, optional). Product taste; recommend deferring to F4.
3. **Vocabulary: synthesize Deck ids or widen the port.** (a) ships fastest
   and touches no domain types; (b) is the honest shape and the trigger for
   ADR 0010's anticipated "promote to domain trait". Recommend (a) for the
   prototype, (b) when committing.
4. **Tombstone retention.** Forever (recommended) vs. horizon knob from day
   one.
5. **Encryption at rest.** Not in v1? Scope later: payload-only vs. whole
   files.
6. **"Stale peers" status vocabulary.** `SyncPhase` has no slot for "synced
   as of 40 minutes ago"; `NetworkLost/Restored` is the wrong tool. Needs a
   small vocabulary decision with the UI work.
7. **Snapshot encoding.** JSON (recommended: debuggable, snapshot-testable)
   vs. CBOR+gzip from day one.
8. **Positioning vs. the Deck connector.** Does `filesync` become the
   generic default with Nextcloud REST as the power option, or the reverse?
   Affects testing-investment split (Tier 2 Syncthing vs. the existing
   docker Nextcloud tier).

---

## Appendix A — Prior art

- **DecSync** (`39aldo39/DecSync`, verified active): serverless sync for
  RSS/contacts/calendars/tasks that "uses a synchronized directory to
  synchronize data", explicitly transport-agnostic (Syncthing, Google Drive,
  Dropbox). Its per-device directory layout with immutable operation files
  is the production existence proof for the 5.5 family — years of field use
  across Android and Linux clients.
- **Joplin**: per-note files over Dropbox/OneDrive/WebDAV with per-target
  sync metadata; conflicts surface as duplicated notes. Lesson: per-entity
  files work, but conflict UX must be a designed surface, not an accident.
- **Obsidian over Dropbox**: plain per-note markdown with no protocol —
  duplicated/conflicted notes are the well-known result. Lesson: the naive
  baseline (5.2) is what users get by default.
- **Zotero WebDAV**: immutable zips keyed by item id + `.prop` sidecars.
  Lesson: immutability plus sidecar metadata is a durable file-protocol
  pattern.
- **KeePass over sync clients**: monolithic database, last-writer-wins.
  Lesson: the canonical cautionary tale for 5.1.
- **Syncthing as a transport** (not a protocol): block-level sync, verified
  mtime preservation, conflict files, ignore patterns, file versioning.
  Lesson: the best-behaved instance of the medium — and still no ordering
  guarantees; the protocol must not lean on its good manners.
- **Automerge / Yjs** (`automerge` 0.12.0, `yrs` 0.28.0, both verified
  active 2026-09): correctness off the shelf; the documented escape hatch
  (§5.6) if requirements outgrow per-field LWW.
- **Literature**: Saito & Shapiro, *Optimistic Replication* (ACM CSUR
  2005) — the taxonomy this report's T-model instantiates; Shapiro et al.,
  *Conflict-free Replicated Data Types* (2011) — state/op-based convergence
  conditions the fold satisfies; Kulkarni et al., *Logical Physical Clocks*
  (2014) — the principled clock-repair option if wall-clock skew proves
  intolerable (convergence does not need it; fairness might).

## Appendix B — File-sync client field notes

**Caveat first: no design in this report may depend on any row of this
table.** The working assumption is always "the client belongs to *either*
conflict family"; rows below are orientation for setup-time verification,
not protocol inputs. Behaviors are client-version-dependent.

| Client | Same-path conflict behavior | mtime | Deltas | Notes |
|---|---|---|---|---|
| Dropbox | Sibling "conflicted copy" files | Preserved | Block-level | Well-documented conflict copies |
| OneDrive | Varies by version/mode; silent-winner reports exist | Varies | Block-level (large files) | Treat as unknown family |
| Nextcloud desktop | Conflict files (client-named) | Preserved | Block-level | `.sync-*` internals; conflict naming client-specific |
| Syncthing | Conflict files (`.sync-conflict-*` convention) | Preserved (verified) | Block-level | `.stignore` patterns, `.stversions` versioning; ignore patterns are a misconfiguration vector |
| Google Drive | Opaque for non-native files | Varies | Opaque | Treat as unknown family |
| rclone mount | Thin client; semantics differ per backend | Varies | n/a | Offline-unfriendly; `bisync` is batch-mode with its own conflicts — out of scope |

## References

- `AGENTS.md` §3 (communication primitives), §5 (testing directives), §7 (artefact management)
- `docs/adr/0004-sync-conflict-policy.org`, `docs/adr/0005-sqlite-local-persistence.org`,
  `docs/adr/0006-state-engine-interpret-loop.org`, `docs/adr/0007-sync-actor-push-first-cycle.org`,
  `docs/adr/0008-app-bootstrap-config-secrets.org`, `docs/adr/0010-standalone-first-optional-sync-connector.org`
- `docs/architecture.org`, `docs/testing_strategy.org`
- `crates/taskboard-domain/src/{entities,ids,messages,outbox,pipeline,push,remote,state}.rs`
  (field lists, clock structs, command/outbox/sync-report vocabulary)
- `crates/taskboard-sync-nextcloud/src/actor.rs`, `crates/taskboard-app/src/{bootstrap,config}.rs`
- `.artifacts/taskboard-potential-improvements-backlog.md` — 2026-10-10 stand-alone-first entry
- DecSync: https://github.com/39aldo39/DecSync (verified 2026-10-11)
- Syncthing FAQ (mtime preservation): https://docs.syncthing.net/users/faq.html
- crates.io: `yrs` 0.28.0, `automerge` 0.12.0, `crdts` 7.3.2 (stale; verified 2026-10-11)
- Saito & Shapiro 2005; Shapiro et al. 2011; Kulkarni et al. 2014
