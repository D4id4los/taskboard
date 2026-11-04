# [Plan] Files Substrate — Design Round (per-user app-data roaming over Nextcloud files; B-min)

- **Date**: 2026-11-05
- **Kind**: design-round doc — **DR-8** (issue #626, milestone
  `nextcloud-files-sync`): the per-user app-data roaming substrate. A
  generic file-sync core (per-device path-owned files, embedded-clock
  fold, tombstones) plus the Nextcloud WebDAV transport as its first
  transport. Design only — no production code.
- **Origin**: the options analysis
  (`.artifacts/reports/2026-11-04-per-user-appdata-roaming-options-report.md`
  — §4.B the shape, §7 the recommendation, §9 the addendum) and the
  user's two-track ruling; the backbone report
  (`.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  — the T-model, §5.4/§5.5 shapes, §7 reference design); the live-tier
  characterization
  (`.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`
  — F1–F8, binding); the cross-milestone contract
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  §3); the session rulings Q1–Q8 recorded on #626 (2026-11-04).
- **Session rulings in force** (restated normative): **Q1** v1 = per-device
  *current-state* files on a 5.5-compatible layout (transition-smoothness
  reservation owed to B-full, the committed successor); **Q2** the fold
  orders by `(clock, author)` with the author optional in v1, F1
  author-tagged clocks are B-full's first chunk; **Q3** journal is the
  only v1 passenger (settings/proofs are the second slice); **Q4** one
  account; **Q5** visible root-level `taskboard/` folder; **Q6** silent
  per-entry LWW v1, fenced eventual goal = automatic merge where
  possible + comfortable manual diff/merge where it fails; **Q7** no
  encryption v1; **Q8** heartbeat + state-hash in, minimal surfacing.
- **Status**: **ACCEPTED** — the user reviewed and approved the plan
  as designed (2026-11-05), confirming the Q1–Q8 rulings.
  Implementation is the four chunk issues the round opened (§7:
  #629, #630, #631, #632); the dispatch round is
  `.artifacts/plans/2026-11-05-filesync-substrate-implementation-dispatch-plan.md`.

---

## 1. Context & verified anchors (verified 2026-11-05 against `main` @ `323a9bf9` + the live tier)

| Fact | Anchor | Evidence |
|---|---|---|
| The merge engine already exists as section unions | domain `appdata.rs` | `AppDataSections` + `union(base, overlay)` (embedded-clock per-key LWW, ties → overlay), `RuleEntry { Rule, Tombstone, Verbatim }` row-enum shape, hand-written tolerant serde, `lane_gc_*` document-side GC — the semantics Part B generalises from board-pull overlays to device documents |
| Flushes materialise from state, not from op payloads | sync `push.rs`, outbox | `MaterializedOp::FlushAppData { board }` is content-free — the flush reads state at cycle time; the substrate's push side takes exactly this shape (D-SF12) |
| The connector slot + registry exist | app `bootstrap.rs`, `connectors.rs` | multi-board wave's registry: N connector actors, per-board routing; `[sync]`/connection config + `CredentialStore` (keyring default, `TASKBOARD_APP_PASSWORD` env escape — ADR 0008); the Tier-3 `.env` convention (`TASKBOARD_IT_NEXTCLOUD_*`) is the live-test credential path |
| The HTTP stack is in-house | sync-nextcloud client | reqwest-based client with retry/`Retry-After` machinery; WebDAV is HTTP verbs + XML bodies on the same stack — **no new transport dependency** (D-SF17) |
| The engine ingests connector reports off-cycle | state `actor.rs` | the sync ingestion path that folds `SyncReport::Completed` (sidecar folds run inside it); `EngineSignal` broadcast wakes consumers; **`SyncReport` is board-shaped** — per-user data needs its own report family (D-SF11) |
| The first passenger's shape is contracted | v0.2 impl-plan §3 | journal = device-local v1, roaming-ready rows (per-entry clocks, tombstones, tolerant serde, `BTreeMap`); the substrate's import is a pure fold of exactly that shape |
| No migrations in B-min | storage `migrations/` | the substrate owns **no local entities**: caches in-memory, device identity in a data-dir file (D-SF4); sqlite numbering (next free 0022) stays v0.2's |
| The transport facts are measured, not assumed | characterization F1–F8 | create-once enforced (F3), lost-update guard on single-writer rewrites (F4, quoted-etag echo), etags = version tokens only (F5), PROPFIND+tree-etags listing (F6), chunking present but unneeded (F7), trashbin present (F8), notify_push present but fenced (§2), hosted stock-capabilities-only posture (F1) |
| Crate + licensing rules | AGENTS §2/§6 | `taskboard-*` prefix; lib tier `MIT OR Apache-2.0`; `#![forbid(unsafe_code)]`; workspace deps pinned once |
| ADR numbering | `docs/adr/` | 0001–0019 used; **ADR 0020** is this round's (chunk 1 writes it — the wave-ADR convention) |

Out-of-scope fences (from #626, binding): boards over files and
coexistence (partition-only when B-full opens); the org connector
(#184); settings/proofs roaming (second slice); folder-sync-client and
LAN transports (successors over the same core); payload encryption
(Q7 fence); task-field merge semantics (DR-4); any UI beyond minimal
sync/freshness status.

## 2. Scope & non-goals

**In scope (design only):** the on-disk protocol — tree layout, device
identity, the per-user section document, heartbeat (Part A); the fold
and its laws (Part B); the WebDAV transport — conditional-PUT
discipline, change detection, quarantine, credentials (Part C); the
wiring — crates, config, bootstrap, kiosk posture (Part D); the engine
ingestion seam + the journal import as first consumer (Part E);
heartbeat/divergence surfacing (Part F); test strategy (§5); chunk
mapping (§7); ADR 0020.

**Non-goals:** anything in the fence list above; chunk/snapshot
emission (reserved, D-SF2); notify_push (websocket dep — fenced
enhancement); read-base clocks / surfaced conflicts / merge UI (the
Q6 eventual goal — its own phase); statistics or UI views; changing
the Deck connector or the sidecar in any way.

## 3. Design

### Part A — The on-disk protocol (D-SF1–D-SF4)

- **D-SF1 — the home.** A visible root-level `taskboard/` folder in
  the account's file home (Q5). First attach creates it (`MKCOL`,
  idempotent). The user can see their data; nothing else in the
  account is touched; uninstall = delete one folder.
- **D-SF2 — the layout (Q1: current-state v1, 5.5-compatible).**

  ```
  taskboard/
  ├── identity.json                # create-once (If-None-Match: *) — F3
  │                                #   { folder_id: uuid7, format_version: 1, created_at }
  └── devices/
      └── <device-id>/
          ├── info.json            # heartbeat, single-writer (D-SF14)
          └── appdata.json         # the device's current-state view of
                                   #   the per-user sections (v1: journal)
  #  Reserved, tolerated, never emitted in v1 (the 5.5 kinds — the
  #  B-full transition lands as new file kinds the fold already
  #  understands well enough to relay): ops/*.json, snapshot-*.json
  ```

  V1 readers encountering a reserved-kind file treat it as **opaque
  relay data** (never parse, never delete, never choke) — the mixed-
  fleet posture every section already practises.
- **D-SF3 — the per-user document.** `appdata.json` is a versioned
  envelope `{ "format": 1, "sections": { "journal": { … } } }`. Rows
  mirror `RuleEntry`'s proven shape: per-section `BTreeMap<entry-uuid,
  UserRecord>` where

  ```rust
  enum UserRecord {
      Entry   { /* the section's own tolerant fields */ clock: DateTime<Utc> },
      Tombstone { clock: DateTime<Utc> },
      Verbatim(serde_json::Value),   // unknown-row tolerance, relayed
  }
  ```

  Hand-written tolerant serde (unknown fields ignored, missing =
  absent); section names reserved and never emitted in B-min:
  `settings`, `proofs`. The document carries the **device's whole
  current view** (its own entries *and* entries it has folded from
  peers — re-publication is how offline peers' data survives a
  device's death; the fold is idempotent so this is safe).
- **D-SF4 — device identity.** A create-once `device.json` in the app
  data dir (`~/.local/share/taskboard/`): `{ device_id: uuid7,
  created_at }`. No sqlite, no migration. Fork/clone detection: every
  rewrite of a device's own files carries `If-Match: <last-known
  etag>` (F4) — an unexpected **412 on one's own single-writer path**
  means another writer with the same device id exists (cloned disk,
  forked folder): the connector **stops writing that path, surfaces
  the alarm loudly, and continues reading** (identity duplication is
  the one condition the medium cannot otherwise signal; F4 makes it a
  first-class typed status).

### Part B — The fold (D-SF5)

Pure function over any set of device documents (+ the local engine
state as one more input):

- Per entry key across all documents: the winner is
  `max by (clock, author, entry-id)` — Q2's seam: the `author` slot
  exists in the order type and in the wire row, is `None` for every
  v1 writer, and becomes the F1 author id the day B-full lands it
  (no wire break; ties today break by entry id, deterministic).
- `Tombstone` competes by clock exactly as `Entry` (a delete after an
  edit wins; a later edit beats an older tombstone) — the `RuleEntry`
  semantics unchanged.
- `Verbatim` rows are relayed, never ordered, never folded.
- Laws (proptest, the union's own tests as the template):
  commutativity (`fold(S) == fold(any permutation)`), idempotence
  (`fold(S ∪ dup) == fold(S)`), associativity, tombstone dominance
  ordering, reserved-kind opacity, and **snapshot-floor equivalence**
  (stated for B-full now, tested against hand-built reserved kinds
  later).
- The fold emits (a) merged sections for import, (b) the per-device
  `state_hash` input (a canonical serialisation hash of the merged
  view).

### Part C — The WebDAV transport (D-SF6–D-SF8, characterization-driven)

- **D-SF6 — conditional-PUT discipline.** `identity.json`: create via
  `PUT` + `If-None-Match: *`; a 412 means another device won the
  create race — adopt the existing file (F3). `info.json` /
  `appdata.json`: single-writer rewrites, each `PUT` carrying
  `If-Match` of the connector's last-known etag, **echoed verbatim as
  served** (F4's quoting note); 412 handling per D-SF4 (own path =
  fork alarm; adoption paths = re-read and re-fold).
- **D-SF7 — change detection.** One `PROPFIND Depth: 1` over
  `devices/` per cycle requesting `getetag` (F6); a device whose
  file etags are unchanged is skipped; changed files are re-`GET`-ed,
  parsed tolerantly, folded. Etags are version tokens only (F5) — a
  redundant re-GET of rewritten-but-identical content is harmless
  (idempotent fold). The local push side mirrors it: re-derive the
  device document from published state, compare a content hash, skip
  the `PUT` when unchanged (F5 makes server etags useless for
  skip-decisions).
- **D-SF8 — cadence.** The connector cycles on the existing sync poll
  interval (`[sync]` cadence; its own `poll_secs` override optional).
  notify_push exists on the tier (characterization §2) and stays a
  fenced enhancement (websocket dependency = its own stack-approval).
- **Quarantine/garbage (the T9 posture):** an unparsable row, section,
  or file is rejected in-memory, **surfaced as a typed sync-status
  finding**, and left untouched on the server (F8's trashbin is the
  user's recovery path; readers never delete or move another device's
  files). Unknown `format > ours`: park the file, fold nothing from
  it, surface "peer runs a newer app".

### Part D — Wiring (D-SF9, D-SF10, D-SF17, D-SF18)

- **D-SF9 — two crates.** `taskboard-sync-files` — the pure core:
  layout types, codecs, the fold, heartbeat types, the state hash.
  No IO, no async, no slint — the transport-agnostic library the
  folder-client and LAN successors reuse. `taskboard-sync-files-nextcloud`
  — the actor + WebDAV client + config + ceremonies (the
  `spawn_sync_actor` shape). Both lib-tier (`MIT OR Apache-2.0`),
  `#![forbid(unsafe_code)]`.
- **D-SF10 — config + credentials.** A `[files_sync]` section:
  `{ enabled = false, poll_secs? }` — the URL and credential come
  from the **same account** the Deck connector uses (Q4): the
  connector binds to the configured Nextcloud connection identity and
  reads its credential via `CredentialStore` (keyring default, the
  ADR 0008 env escape unchanged — F2). Absent table = off
  (standalone-first; nothing spawns).
- **D-SF17 — XML handling.** `PROPFIND` requests are fixed-shape
  constant bodies; responses are parsed by a **narrow hand-rolled
  extractor** for exactly the properties we request (`href`,
  `getetag`) — tolerant, fuzzed (the testing strategy's parser
  trigger), no XML dependency added.
- **D-SF18 — run modes.** The substrate spawns wherever
  `[files_sync] enabled` ∧ credentials resolve — **mode-agnostic**
  (a kiosk is a device of the fleet; syncing read-only journal data
  to it is consistent and has no auto-behavior to gate). No kiosk
  exclusion (contrast the focus runtime — that exclusion exists
  because the runtime *acts*; this only syncs).

### Part E — The engine seam + the first consumer (D-SF11–D-SF13)

- **D-SF11 — ingestion.** `SyncReport` is board-shaped; per-user data
  gets its own report family on the same connector→engine channel:
  `UserDataReport { sections }`. The engine folds it exactly as the
  sidecar folds run (clock-guarded imports, tombstones applied,
  `Verbatim` ignored), into the per-user homes (v0.2's journal state
  first). Imports never enqueue board ops (`UpdateAppData` stays
  board-only); no new `LocalOp` lane exists.
- **D-SF12 — the push side.** No outbox involvement: each cycle, the
  connector re-derives the device document from the published
  `ArcSwap<AppState>` snapshot (the `FlushAppData`
  materialise-from-state precedent), compares, and `PUT`s iff changed
  — journal edits are engine writes like any other; the substrate
  only publishes settled state.
- **D-SF13 — the journal import (first consumer, Q3).** On attach:
  fold-first (read all peers, import) then publish the local journal
  as the device's first `appdata.json`. The import honours the v0.2
  plan §3 contract verbatim (per-entry clocks, tombstones, tolerant
  rows). **Soft gate:** this consumer needs v0.2's journal entity
  chunk landed; core + transport (chunks 1–2) are journal-independent
  and start now.

### Part F — Heartbeat & divergence (D-SF14, Q8)

`info.json` per device: `{ app_version, format_version, last_clock,
state_hash, written_at }`, refreshed on every flush. Consumers: peer
freshness in sync status ("synced as of N min ago"); **divergence
detection** — state hashes that disagree past a quiescence window
(two cycles + no pending writes) surface as a typed finding. v1
surfacing is minimal: the sync-status surface + a `status` CLI line.
The fuller "stale peers" vocabulary stays the recorded open question
it already is.

### Conflict posture (D-SF16, Q6)

v1 resolves concurrent entry edits silently by the D-SF5 order
(documented, not secret). The fenced eventual goal — automatic merge
where possible, comfortable manual diff/merge where it fails — is the
read-base-clock/three-way-merge direction: v1's rows reserve an
optional `base_clock` field slot in the tolerant serde (never
emitted), so the enhancement lands without a wire break. No merge UI,
no surfaced conflicts, in B-min.

## 4. Decision summary

| ID | Question | Decision | Status |
|---|---|---|---|
| D-SF1 | Home | Visible root-level `taskboard/` in the account home; first-attach MKCOL | Q5, ruled |
| D-SF2 | Layout | `identity.json` + `devices/<id>/{info,appdata}.json`; §5.5 kinds (`ops/`, `snapshot-*`) reserved + opaque-relayed | Q1, ruled |
| D-SF3 | Document | Versioned envelope, per-section `BTreeMap<uuid, UserRecord{Entry,Tombstone,Verbatim}>`, tolerant serde; `settings`/`proofs` reserved | recorded |
| D-SF4 | Device identity | Create-once data-dir `device.json`; own-path `If-Match` 412 = fork alarm (stop-write, loud, keep-read) | recorded (F4) |
| D-SF5 | Fold | Pure; order `(clock, author?, entry-id)`; Q2 author slot optional in v1; laws proptested | Q2, ruled |
| D-SF6 | Writes | Create-once via `If-None-Match:*`; single-writer rewrites via `If-Match` (etag echoed verbatim) | recorded (F3/F4) |
| D-SF7 | Change detection | Per-cycle `PROPFIND` etag compare; re-GET changed; skip-PUT by local content hash (etags are version tokens) | recorded (F5/F6) |
| D-SF8 | Cadence | Rides the sync poll interval; notify_push fenced (present on tier, websocket dep out of v1) | recorded (§2) |
| D-SF9 | Crates | `taskboard-sync-files` (pure core) + `taskboard-sync-files-nextcloud` (actor/transport) | recorded |
| D-SF10 | Config/credentials | `[files_sync] {enabled=false, poll_secs?}`; one account, shared with Deck; `CredentialStore` unchanged | Q4, ruled (F2) |
| D-SF11 | Ingestion | New `UserDataReport { sections }` family on the connector→engine channel; sidecar-fold semantics; no board ops | recorded |
| D-SF12 | Push side | Materialise-from-state per cycle, hash-compare, PUT iff changed; no new outbox lane | recorded |
| D-SF13 | First consumer | Journal import (fold-first, then publish); honours v0.2 plan §3; soft-gated on the journal entity | Q3, ruled |
| D-SF14 | Heartbeat | Per-device `info.json` (versions, last_clock, state_hash); freshness + quiescence divergence finding; minimal surfacing | Q8, ruled |
| D-SF15 | Garbage | In-memory typed rejection, file untouched, trashbin = user recovery; unknown format = park + surface | recorded (F8) |
| D-SF16 | Conflicts | Silent per-entry LWW v1; `base_clock` field slot reserved; auto-merge + manual merge UI = fenced phases | Q6, ruled |
| D-SF17 | XML | Fixed-shape PROPFIND bodies; narrow hand-rolled tolerant extractor (fuzzed); no XML dep | recorded |
| D-SF18 | Run modes | Mode-agnostic spawn (enabled ∧ credentials); no kiosk exclusion — sync has no auto-behavior | recorded |
| D-SF19 | Migration/ADR | Zero migrations in B-min; ADR 0020 by chunk 1 | recorded |

## 5. Test-strategy hooks

- **Core proptests**: the D-SF5 laws over generated document sets
  (duplicates, permutations, tombstone/edit races, Verbatim rows,
  reserved-kind files); author-slot ordering with `None` authors
  (v1's own shape) and `Some` (B-full's, forward-pinned).
- **Fuzz**: the PROPFIND extractor and the document codecs (the
  parser trigger; garbage in → typed rejection out, floors not
  ceilings).
- **Fake transport**: an in-process fake WebDAV (trait-implemented)
  delivering reordered, duplicated, torn, garbage, and
  unexpected-412 scenarios deterministically (the backbone report's
  fake-transport matrix, file-flavoured); `tokio::time` paused, no
  sleeps.
- **Docker tier** (the existing Nextcloud compose): pin F3/F4/F5/F6
  semantics against the controlled image — create-once, lost-update
  guard, etag churn, tree etags (the characterization re-pins).
- **Live Tier-3** (`#[ignore]`, `.env` convention): the same pins
  against the real hosted tier (the hosted-instance upgrade caveat,
  characterization §3).
- **Engine**: `UserDataReport` fold journeys (import/tombstone/skip)
  as insta state snapshots; no board ops enqueued (pinned).

## 6. Risks

- **Hosted-tier drift** (F1: provider-managed server may change
  behaviour at upgrade): mitigated by docker-tier pins + live re-pins;
  the transport treats surprises as typed findings, never crashes.
- **Fork/clone identity duplication**: D-SF4's `If-Match` alarm is
  the detection; the runbook (stop-write, keep-read, rename device)
  lands with chunk 4's docs.
- **The ingestion seam is novel** (first non-board report family):
  chunked first and smallest; the sidecar-fold precedent bounds the
  semantics.
- **Whole-document re-PUT growth** (current-state shape): bounded at
  journal scale (options report §7 numbers); the reserved chunk kind
  is the measured-need escape — do not build it speculatively.
- **Two milestones, one implementer**: chunks 1–2 are
  journal-independent (start now); chunk 3 rides v0.2's journal
  entity; chunk 4 last. No chunk touches v0.2 files except the
  ingestion seam's engine addition.
- **Quota/throttling on the hosted tier**: v1 payloads are small;
  skip-PUT (D-SF7) caps steady-state writes at ~one PUT per device
  per changed cycle; measure in chunk 4's hardening.

## 7. Chunk mapping → the issues this round opens on acceptance

| Chunk | Scope | Size | Issue |
|---|---|---|---|
| 1 — the core crate | D-SF2–D-SF5 (layout, codecs, fold + laws, heartbeat types) + ADR 0020; no IO | M | `backlog: DR-8 chunk 1 — files core crate (layout, codecs, fold, heartbeat)` |
| 2 — the transport | D-SF6–D-SF10, D-SF17 (WebDAV client, conditional-PUT discipline, change detection, config, registry wiring) + docker-tier pins | M | `backlog: DR-8 chunk 2 — nextcloud files transport actor & wiring` |
| 3 — the seam + import | D-SF11–D-SF13 (`UserDataReport`, engine fold, journal import) — soft-gated on v0.2's journal entity | M | `backlog: DR-8 chunk 3 — engine ingestion seam & journal import` |
| 4 — hardening & docs | D-SF14/D-SF15 surfacing, divergence alarm, trashbin/fork runbooks, architecture.org, live Tier-3 pins | S | `backlog: DR-8 chunk 4 — substrate hardening, status surfacing, runbooks` |

Sequencing: 1 → 2 → 3 (soft-gate) → 4; 1 and 2 parallel with all of
v0.2's chunks (disjoint files).

## 8. References

- Issue #626 (the round; rulings Q1–Q8 in its comments); milestone
  `nextcloud-files-sync`; the B-full successor ruling (milestone
  description + options report §9 note).
- The backbone: `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`.
- The decision input: `.artifacts/reports/2026-11-04-per-user-appdata-roaming-options-report.md`.
- The transport facts: `.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`.
- The passenger contract: `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md` §3.
- In-tree: `appdata.rs` (the union semantics), `push.rs`/`outbox.rs`
  (materialise-from-state), `bootstrap.rs`/`connectors.rs` (registry),
  ADRs 0004/0006/0007/0008/0010 (the vocabulary this rides); the
  sidecar design round (DR-1) for the section-recipe lineage.
