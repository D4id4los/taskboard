# [Plan] The Nextcloud Files Transport — `taskboard-sync-files-nextcloud` (TDD implementation plan)

- **Date**: 2026-11-05
- **Kind**: stage-1 implementation plan — chunk 2 of the DR-8
  implementation dispatch (branch `feat/sync-files-nextcloud`, issue
  #630). TDD-first: every commit below names its failing tests before
  the code that satisfies them.
- **Origin** (normative): the accepted design round
  `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  — §3 Part C (D-SF6–D-SF8 + the quarantine posture), Part D
  (D-SF9/D-SF10/D-SF17/D-SF18), §5 test hooks, §7 chunk 2; the
  transport facts
  `.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`
  F1–F8 + §2/§3 (**binding**); the session rulings Q4/Q5/Q8 on issue
  #626 (2026-11-04); the dispatch plan §3 (chunk table, shared
  execution model, code-quality mandate); the cross-milestone context
  of D-SF11–D-SF13 only as *future seams* (chunk 3 owns them — §3.9
  here plans the seam shape, never the ingestion).
- **Status**: READY for the stage-2 implementation agent. Decisions
  D-FT1…D-FT12 below settle every open question this chunk owns;
  deviations from a normative origin section need a recorded reason in
  the stage-2 report.

---

## 1. Context & verified anchors (verified 2026-11-05 against `origin/main` @ `58ae9c55`)

| Fact | Anchor | Consequence for this chunk |
|---|---|---|
| Chunk 1 landed | `feat(sync-files)` commits `27ae77e6…030634f6` on `origin/main`; crate `crates/taskboard-sync-files` (lib, `identity`/`ids`/`layout`/`document`/`fold`/`hash`/`heartbeat`/`test_support`) | The pure substrate exists: `fold(&[UserDocument]) -> UserDocument`, `StateHash::of(&UserDocument)`, `DeviceInfo`, `FolderIdentity`/`DeviceIdentity` payloads, `layout::classify` + path builders, `DeviceId::mint/parse`. This chunk transports exactly those types — nothing is re-defined here |
| The actor shape to mirror | `taskboard-sync-nextcloud/src/actor.rs` | `spawn_sync_actor(id, client, state, commands, reports, system, cfg) -> JoinHandle<()>`: injected channel halves, biased `select!` over commands/deadline/broadcast, `LoopState` in-memory caches, first cycle immediate, nudge absorber, `poll_backoff(streak, initial, max, Retry-After floor)` rescheduling. `SyncActorConfig { poll_interval, backoff_initial, backoff_max, clock: Arc<dyn Clock>, … }` |
| The client shape to mirror | `taskboard-sync-nextcloud/src/client.rs` | `DeckClient`: reqwest + `Url` base + Basic credentials, **Debug redacts credentials**, `BackoffPolicy` (3 attempts, 500 ms→8 s) + `RetrySleep`/`TokioSleep` seam, retry only on the transport class (`Transport | RateLimited | Unavailable`), `Retry-After` parsed in the delay-seconds form, validators emitted **verbatim** (`If-None-Match`/`If-Modified-Suite` precedent of the verbatim rule) |
| No actor is the network authority | `actor.rs` module doc + `SystemEvent` arms | The Deck actor publishes only *its own* `ConnectorLost/Restored`; counting into global `NetworkLost` is `taskboard-app/src/authority.rs`'s job. The files actor (chunk 2) consumes the broadcast **only for `Shutdown`** and publishes nothing on it — the acceptance pin |
| Connector wiring precedent | `taskboard-app/src/connectors.rs` + `bootstrap.rs` | `ConnectorModule::build_and_spawn(commands, reader, reports, system, cfg) -> SpawnedConnector`; registry loop consumes the boot pair; module spawn failure unwinds via typed `BootstrapError`. The files transport is **not** a board connector (no `SyncCommand`/`SyncReport` lane in chunk 2) — it spawns beside the focus/escalation runtimes, not into the connector registry |
| Config section precedents | `taskboard-app/src/config.rs` | `[focus]` = the opt-in section recipe: `#[serde(default, skip_serializing_if = "…::is_default")]` on the field (absent table serializes unchanged), `#[serde(deny_unknown_fields)]` on the section, `Default` = the absent-table placeholder (`enabled = false`), validation-ladder arms with typed per-field `ConfigError` variants, mint into the runtime type via `config.<x>_config()` (expect-on-validated) |
| Credential path | `connectors.rs` `NextcloudModule` + `secrets::CredentialStore` | `credentials.load(server_url, username)` at spawn time; keyring default + `TASKBOARD_APP_PASSWORD` env escape (ADR 0008) — reused unchanged (F2, Q4) |
| Data dir | `config.rs` (`dirs::data_dir()`, default db `<data>/taskboard/taskboard.db`) | `device.json` lives at `<data>/taskboard/device.json` (D-SF4's literal home), resolved by `taskboard-app` and injected as a `PathBuf` — path resolution stays app-side, the file IO is this crate's |
| Docker tier | `docker/nextcloud-test/compose.yaml` + `scripts/nextcloud_it_setup.sh` + `tests/it_docker.rs` pattern | `#[ignore]` tests named `it_nextcloud_docker_*`, `live_docker_config() -> Option` (skip-never-fail), bodies in `common::suite`, `with_deadline`, `run_id()` isolation, `retrying_server_errors`; the compose interpolates `TASKBOARD_IT_NC_IMAGE` (default `nextcloud:35-apache`) — the characterization was captured on 34.0.5 hosted, so the docker pins are the *controlled* re-pin |
| Wiremock tier | `tests/actor_wiremock.rs` | `#[tokio::test(start_paused)]`, wall-clock-bounded wait helpers that `advance(10ms)` while waiting, request counts as exact observables, header matchers for exact request facts |
| Fuzz posture | `docs/testing_strategy.org` §11 | cargo-fuzz deferred *until a custom parser/tokenizer appears* — the PROPFIND extractor **is** that trigger. Decision D-FT5: in-tree proptest fuzz target now (structural + arbitrary-byte garbage); the OSS-Fuzz scaffold stays deferred (recorded escalation note) |
| Workspace pins | root `Cargo.toml` | `reqwest 0.12 (json, multipart, rustls-tls)`, `tokio`, `wiremock 0.6`, `tempfile 3`, `dirs 7`, `proptest` all pinned. This crate adds **no new external dependency**; `[workspace.dependencies]` gains only the two internal path entries; `cargo deny` untouched |

## 2. Scope & non-goals

**In scope**: the new crate
`crates/taskboard-sync-files-nextcloud` — the transport seam +
WebDAV client (D-FT2), the PROPFIND extractor + fixed bodies + fuzz
(D-FT5), the etag store & `LoopState` (D-FT4), the fork status & typed
findings (D-FT6), the device-identity ceremony (D-FT7), the attach
ceremony + cycle state machine (D-FT8), the status watch (D-FT6);
`[files_sync]` config section + validation arms + mint (D-FT9);
bootstrap spawn wiring, mode-agnostic (D-FT10); the docker-tier pins
of F3–F6 and the fake-transport matrix (D-FT12); CHANGELOG entry.

**Non-goals** (fences, per the dispatch §3): engine ingestion — no
`UserDataReport` family, no reports channel, no board ops, no
`SyncReport` change (chunk 3); notify_push / websockets (fenced,
characterization §2); chunked upload (F7 present, unused); status
*surfacing* — UI views, CLI `status` line, freshness/divergence
findings' consumers (chunk 4; chunk 2 only emits the typed findings
onto the watch); settings/proofs sections (second slice); reserved
`ops/`/`snapshot-` emission (B-full); any delete/move of any file —
**the transport has no `DELETE` code path** (F8: trashbin is the user's
recovery; readers never delete); changing the Deck connector, the
sidecar, or `taskboard-sync-files` in any way.

## 3. Design decisions (D-FT1…D-FT12, normative for stage 2)

### D-FT1 — Crate skeleton & manifest

```
crates/taskboard-sync-files-nextcloud/
├── Cargo.toml        # lib; workspace-inherited fields; license = "MIT OR Apache-2.0"
│                     # [dependencies] taskboard-sync-files, taskboard-domain, reqwest, serde,
│                     #   serde_json, chrono, tokio, thiserror, tracing  (all workspace)
│                     # [dev-dependencies] proptest, wiremock, tempfile, uuid,
│                     #   tokio(test-util), tracing-subscriber
│                     # [features] test-support = []   (exports FakeFilesTransport, §5)
│                     # [lints] workspace = true
└── src/
    ├── lib.rs        # crate doc: the transport contract in one screen — conditional-PUT
    │                 # discipline, etags-are-version-tokens, readers-never-delete, the
    │                 # fork alarm; module map; SPDX; #![forbid(unsafe_code)]
    ├── error.rs      # TransportError (typed HTTP outcomes) + FilesSyncError (crate-level)
    ├── etag.rs       # Etag (verbatim newtype) + EtagStore (D-FT4)
    ├── extract.rs    # the PROPFIND request-body constants + the narrow extractor (D-FT5)
    ├── transport.rs  # the FilesTransport trait + outcome types (D-FT2)
    ├── client.rs     # WebDavFilesTransport — the production impl over reqwest (D-FT2)
    ├── device.rs     # device.json read-or-create-once ceremony (D-FT7)
    ├── status.rs     # FilesSyncStatus, FilesSyncFinding, ForkStatus semantics types (D-FT6)
    ├── cycle.rs      # the attach ceremony + the cycle state machine, pure over the seams (D-FT8)
    ├── actor.rs      # spawn_files_sync_actor — the poll loop around the cycle (D-FT8)
    └── test_support.rs # FakeFilesTransport + fixtures (behind `test-support`) (D-FT12)
```

`taskboard-domain` is a dependency only for `Clock`/`SystemClock` and
`ConnectorId` (the spawn identity); no domain sync types cross this
crate. Root `Cargo.toml`: members += `crates/taskboard-sync-files-nextcloud`;
`[workspace.dependencies]` += `taskboard-sync-files` and
`taskboard-sync-files-nextcloud` path entries (internal section).
`taskboard-app` gains `taskboard-sync-files-nextcloud.workspace = true`.
Rough size: ~1100–1500 src LOC + ~1600–2200 test LOC; app deltas
~250–400 LOC.

### D-FT2 — The transport seam & the client's verbs

The actor consumes a **trait**, never the concrete client (the design
§5 fake-transport is trait-implemented):

```rust
pub trait FilesTransport: Send + Sync {
    async fn mkcol(&self, path: &str) -> Result<MkcolOutcome, TransportError>;
    async fn propfind_devices(&self) -> Result<PropfindListing, TransportError>;
    async fn get(&self, path: &str) -> Result<FileContent, TransportError>;
    async fn put_create(&self, path: &str, body: &[u8]) -> Result<PutOutcome, TransportError>;
    async fn put_if_match(&self, path: &str, body: &[u8], etag: &Etag) -> Result<PutOutcome, TransportError>;
}
pub enum MkcolOutcome { Created, AlreadyExists }       // 201 | 405(existing) — the exists-ok posture
pub struct PropfindListing { pub members: Vec<PropfindMember> }  // 207
pub struct PropfindMember { pub href: String, pub etag: Option<Etag> }
pub struct FileContent { pub bytes: Vec<u8>, pub etag: Option<Etag> }
pub enum PutOutcome { Created { etag: Option<Etag> }, Updated { etag: Option<Etag> } }  // 201 | 204
```

- **Verb set is closed**: `MKCOL`, `PROPFIND` (Depth 1, fixed body),
  `GET`, conditional `PUT` (create-once / if-match). **No `DELETE`, no
  `MOVE`, no `PROPPATCH`, no `LOCK`** — the trait has no such method,
  so no code path can remove or relocate a file (readers-never-delete
  enforced by construction, F8).
- `Etag(String)` — the **verbatim** newtype over the raw header value
  *as served* (quoted form included). Constructed only from a response
  header; `Display`/`AsRef` hand it back untouched. The type's doc
  carries the rule: never trim, never strip, never re-quote — the F4
  characterization bug (client double-quoting) is a *docker-tier test*,
  not a comment (§5, pin 3). Both `put_create` (→ `If-None-Match: *`)
  and `put_if_match` (→ `If-Match: <verbatim>`) map 412 to the typed
  `TransportError::PreconditionFailed` and 404 to `NotFound`; every
  other status maps through the Deck-client status table (`Unauthorized`,
  `Forbidden`, `RateLimited { retry_after }`, `Unavailable`, `Server(u16)`,
  `Transport(#[from] reqwest::Error)` — `error.rs` mirrors `DeckError`,
  `thiserror`-derived). **412 is a *result*, not an error arm of the
  retry loop**: preconditions are never retried.
- `WebDavFilesTransport` (production impl): base URL
  `<server>/remote.php/dav/files/<username>/` — the account's DAV file
  home (`<root>`); the substrate root *is* `taskboard/` under it, so a
  transport request path is `<root>/taskboard/devices/…`; the client
  joins base + `ROOT_FOLDER_NAME` + the substrate-relative path.
  Basic auth,
  reqwest `Client`, `BackoffPolicy` + `Arc<dyn RetrySleep>`-style
  sleeper seam **mirroring DeckClient exactly** (retry only the
  transport class: 429/503/transport failures, `Retry-After` floor;
  4xx are terminal). `Debug` redacts credentials. Relative-path joins
  percent-encode nothing: substrate paths are UUIDs and fixed names —
  already URL-safe (asserted by a unit pin, not by a codec).
- href→relative-path mapping (D-FT5): no percent-decoder is written;
  the actor matches raw hrefs against the *expected encoded forms* it
  can compute (`device_dir_name(id)` suffixes are `[0-9a-f-]`, safe by
  construction); unmatched hrefs are ignored (`Other`). The one
  normalization allowed: strip **one** trailing `/` from collection
  hrefs (Nextcloud serves collection members with trailing slashes).

### D-FT4 — The etag store & LoopState (all in-memory, process-local)

```rust
/// Last-known version tokens, keyed per file — plus the per-device
/// directory tokens that drive change detection. In-memory only; a
/// process restart starts empty, which costs one honest full-read
/// cycle (the fold is idempotent; no persistence exists by design,
/// D-SF19 zero migrations).
pub struct EtagStore {
    device_dirs: BTreeMap<DeviceId, Etag>,     // from PROPFIND members — the dirty markers
    files: BTreeMap<String, Etag>,             // per-file last-known (relative path → etag);
                                               //   own paths feed If-Match, peer paths are bookkeeping
}
```

`LoopState` (the actor's mutable state, mirroring the Deck actor's):

```rust
struct LoopState {
    attached: bool,                            // attach ceremony completed
    etags: EtagStore,
    docs: BTreeMap<DeviceId, UserDocument>,    // last decoded peer document per device (excl. self)
    heartbeats: BTreeMap<DeviceId, DeviceInfo>,// last decoded peer heartbeat (chunk 4's freshness input)
    publish_seed: Option<UserDocument>,        // adopted own-file content, consumed once (D-FT8)
    published_hash: Option<StateHash>,         // hash of the last successfully published view
    pending_heartbeat: bool,                   // set at attach + on every appdata publish
    forked_paths: BTreeSet<String>,            // the stop-write set (D-FT6)
    parked: BTreeMap<String, ParkKind>,        // D-SF15: NewerFormat | Quarantined (path → kind)
    failure_streak: u32,
    retry_after_floor: Duration,
}
```

Store-write rules: PROPFIND members write `device_dirs` (+ prune
devices that left the listing); GET responses write `files` (etag when
the server sends one); PUT responses overwrite the own path's `files`
entry. **A guarded rewrite (`put_if_match`) is only ever issued when
`files[own_path]` holds a token** — if it does not (fresh boot), the
cycle GETs the own file first (the adopting read) and never PUTs
unguarded. Pinned by a fake-transport test: no `put_if_match` ever
follows a cold store without an intervening `get` (§5, commit 5).

### D-FT5 — The PROPFIND extractor: fixed bodies, narrow surface, fuzz

Request bodies are **public constants** (one body serves both the
current design's single listing and any future Depth-1 listing — the
surface is `getetag` only, per D-SF17 "parses only what we request"):

```rust
pub const PROPFIND_DEPTH1_BODY: &str =
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
     <d:propfind xmlns:d=\"DAV:\">\n  <d:prop><d:getetag/></d:prop>\n</d:propfind>";
```

(No `getcontentlength`/`getlastmodified` — nothing in the chunk-2
cycle consumes them; adding a prop means adding its parse arm and its
test, in one commit, later.)

The extractor (`extract.rs`) parses **207 multi-status bodies only**:

- Hand-rolled: a tiny well-formedness-scanning state machine over the
  byte stream that (a) locates each `multistatus` → `response` element
  (prefix-agnostic **local-name** matching inside the DAV namespace —
  recorded leniency: the extractor does not verify the namespace URI,
  it matches the local names `multistatus`/`response`/`href`/
  `getetag`/`status` wherever they appear inside a well-formed
  `multistatus` tree), (b) captures each `response`'s first `href`
  text and its `getetag` text (a `response` without `getetag` —
  e.g. a collection that omits it — yields `etag: None`; a `response`
  without `href` is skipped as malformed *member*, not a body
  failure), (c) ignores every other element (`propstat`, `status`,
  anything else — parse surface closed to what we request).
- **Typed outcomes only**: `Ok(PropfindListing)` or
  `Err(PropfindParseError)` (malformed XML, truncated body, missing/
  non-XML root). `multistatus` present with zero `response` members is
  a legal **empty listing** (first attach), not an error. The
  extractor never panics — the fuzz target below is its contract.
- No XML dependency is added (D-SF17); no percent-decoding (D-FT2).

**Fuzz target** (in `extract.rs`'s tests, proptest — the
testing-strategy §11 trigger, in-tree; the cargo-fuzz/OSS-Fuzz scaffold
stays deferred and the plan records the escalation note):

1. `extractor_never_panics_over_arbitrary_bytes` — `any::<Vec<u8>>()` →
   `Ok | Err`, never panic (floors on garbage).
2. `extractor_survives_structured_tearing` — a valid 207 sample
   (fixture captured from the docker tier's shape) mutated by
   generated truncation/splice/byte-flip operators → `Ok | Err`, and
   **whenever it parses, every member it reports carries a non-empty
   href** (structural floor, no ceilings on how much garbage it takes).
3. `document_decode_never_panics_over_arbitrary_bytes` — the GET-body
   consumer: arbitrary bytes → `serde_json::from_slice::<UserDocument>`
   typed outcome (the design §5 fuzz hook over the codecs, riding this
   chunk's transport boundary).

### D-FT6 — The fork status & the typed findings (status watch)

Chunk 2 **emits** typed findings; chunk 4 surfaces them. The channel
is a `tokio::sync::watch<FilesSyncStatus>` (latest-state semantics:
consumers read "what is true now", no backlog semantics to lose) —
the transport crate owns the types:

```rust
pub struct FilesSyncStatus {
    pub last_cycle: Option<CycleOutcome>,          // Ok { published: bool, read_devices: usize } | Failed
    pub forked_paths: BTreeSet<String>,            // the live stop-write set
    pub parked: BTreeMap<String, ParkKind>,        // NewerFormat | Quarantined
    pub last_heartbeat_written: Option<DateTime<Utc>>,
}
pub enum ParkKind { NewerFormat { format: u32 }, Quarantined }   // D-SF15 arms
```

A `finding` is a state transition on this struct plus a `tracing`
record (`tracing::error!` for the fork alarm — "surface loudly" in
chunk 2 means exactly: typed state + loud log; the status surface and
CLI line are chunk 4's mapping).

**Fork semantics (D-SF4's mechanics, normative):**

- **Trigger**: `put_if_match` on one of the device's own single-writer
  paths (`devices/<self>/info.json`, `devices/<self>/appdata.json`)
  answers **412** — and the disambiguation below does not resolve it
  to a vanished file. Create-once 412s (`put_create`) are *never*
  forks: they are the adoption arms (F3 identity race; former-install
  own-file adoption).
- **Disambiguation (one GET, recorded interpretation — flagged for
  review)**: `If-Match` against a *missing* file also fails the
  precondition (412 per RFC 4918 — the docker tier pins this server
  fact, §5 commit 8). On an own-path rewrite 412 the cycle issues one
  `get` of that path: **404 ⇒ `OwnFileVanished`** — the file was
  deleted out-of-band (Files-UI/trashbin class, F8's recoverable
  accident, *not* a fork): typed finding, drop the path's etag, next
  cycle recreates via `put_create` (adoption semantics). **200 ⇒
  fork** (someone holds the path; content is irrelevant — even
  byte-identical content under a fresh etag proves a second writer,
  F5's upload-churn rule). Failing closed here is deliberate: a
  false fork alarm over a user delete would cry wolf; a missed fork
  would silently split a device's history.
- **Stop-write on fork**: the path enters `forked_paths`; every later
  cycle's write phase **skips any `put` to a path in the set** (guard
  checked before the PUT, pinned by the fake-transport request-count
  test); **reads continue forever** — the forked subtree keeps
  participating in PROPFIND/GET/fold exactly like any peer device (our
  own subtree is read as peer data after the fork; the sibling path
  forks on its own 412 if the clone writes it, so the set converges to
  full-subtree stop). No retry loop may re-PUT after a fork-indicating
  412 — a review finding per the dispatch. No rename, no delete, no
  auto-recovery — the runbook is chunk 4's.
- **Corrupt own-file adoption** (the create-412 adopt path reading
  garbage): stop-write that path + `Quarantined` finding — writing an
  empty merged view over an unrecoverable own history is data loss;
  fail closed (pinned).

### D-FT7 — The device identity ceremony (`device.rs`)

- Path resolution stays app-side: bootstrap injects
  `<data>/taskboard/device.json` as a `PathBuf` (`dirs::data_dir()`
  family, the db-default's directory). No `dirs` dependency enters
  this crate.
- `load_or_create(path, clock) -> Result<DeviceIdentity, DeviceIdentityError>`:
  read → strict-envelope decode (extras tolerated — the chunk-1
  payload); missing file → `DeviceIdentity { device_id:
  DeviceId::mint(), created_at: clock.now() }` serialized and written
  **atomically** (write `<file>.tmp-<suffix>` then rename — the
  backbone §7.1 discipline; the `classify` `Temporary` arm exists for
  leaks exactly like this). Decode failure (corrupt file) ⇒ typed
  `DeviceIdentityError::Unusable` — **never regenerate**: a new device
  id over the old file's tree would silently fork the device; the
  files transport stays off and the finding surfaces (runbook chunk
  4). Pinned by tests: corrupt file ⇒ error + file content untouched.
- The mint uses chunk 1's `DeviceId::mint()` — the substrate's single
  nondeterminism site, consumed at exactly one place.

### D-FT8 — The attach ceremony & the cycle state machine

**Attach** (idempotent, retried by the backoff until it completes; a
network failure at any arm is a cycle failure):

```
1. load_or_create device.json (local file; Unusable ⇒ files-sync stays
   off, typed finding, the actor parks permanently).
2. MKCOL taskboard/            → Created | AlreadyExists (201 | 405; any
                                  other status = typed error)
3. MKCOL taskboard/devices     → same posture
4. MKCOL taskboard/devices/<self> → same posture
5. identity.json: put_create(If-None-Match: *)   (F3)
      201 → we minted FolderIdentity { uuid7, TREE_FORMAT_VERSION, now }
      412 → another device won the create race → get + adopt the served
            FolderIdentity (strict-envelope decode; garbage ⇒ Quarantined
            finding + folder treated as unusable: stop, typed error)
6. PROPFIND devices/ (first read — full dirty set, §5 store rule)
7. GET all peers' files (dirty set) → decode → docs/heartbeats/parked
8. own appdata.json: put_create   (first publish — content = the merged
      view, step 9 of the cycle; 412 ⇒ adopt: get, capture etag, content
      becomes publish_seed; corrupt seed ⇒ stop-write appdata path)
9. own info.json: put_create     (412 ⇒ adopt: get + capture etag only)
```

**Steady cycle** (every poll tick; the state machine the dispatch
prompt names, in order):

```
1. PROPFIND devices/ (getetag, Depth 1)  → member list
      → dirty-set = devices whose dir etag ≠ store, plus devices never
        seen; vanished-set = stored devices absent from the listing
      → prune vanished devices' docs/heartbeats/etags (the tree is the
        truth: rows of a vanished device leave the merged view; the
        trashbin is the user's recovery path — pinned by the
        device-disappears fake test)
2. changed-set re-read: for each dirty device ≠ self →
      GET info.json + appdata.json (404-tolerant; decode tolerantly:
      garbage envelope ⇒ Quarantined park, format > DOCUMENT_FORMAT ⇒
      NewerFormat park — fold nothing from it, D-SF15; well-formed ⇒
      docs/heartbeats update, etags store)
      (self ≠ dirty in steady state: our own files are output-only —
      the only self-reads happen at attach/adoption/boot, feeding
      publish_seed, D-FT4)
3. fold-core (chunk 1 fn): merged = fold(local.document()      ← the seam,
      ++ publish_seed (consumed once, then cleared)
      ++ docs.values()  /* peers present in the latest listing */ )
4. own-publish hash-compare: h = StateHash::of(merged)
      skip-PUT iff h == published_hash   (F5: server etags are upload-
      churned — never a skip criterion; the LOCAL hash decides)
5. conditional PUTs (each guarded by forked_paths first):
      appdata: bytes = serde_json::to_vec(merged)
        no etag on file (cold) ⇒ GET own path first (adopt; also seed
          — see D-FT4), then rewrite
        put_if_match(appdata) → 204 ⇒ published_hash = Some(h),
          pending_heartbeat = true, etag store update
        412 ⇒ disambiguate (D-FT6): OwnFileVanished (recreate next
          cycle) or ForkDetected (stop-write appdata path, loud)
        404 ⇒ recreate via put_create
      info: if pending_heartbeat && !forked(info path):
        DeviceInfo { app_version (cfg), format_version: TREE_FORMAT_VERSION,
          last_clock: max clock over merged rows (None ⇒ created_at),
          state_hash: h, written_at: clock.now() }
        same guarded discipline as appdata (412 ⇒ disambiguate)
6. status watch update; failure classification:
      Transport/RateLimited/Unavailable ⇒ failure_streak += 1,
      retry_after_floor updated (Retry-After floor, the poll_backoff
      seam) ; typed 4xx ⇒ finding + streak (the cycle failed; backoff)
      success ⇒ streak = 0
```

Rescheduling mirrors the Deck actor exactly: `poll_interval` after
success, `poll_backoff(streak, initial, max, retry_after_floor)` after
failures, first cycle immediate. **No command/nudge channel exists in
chunk 2** (the poll cadence is D-SF8's ruling; chunk 3 may add a nudge
when journal settles demand one — recorded seam, not speculative
plumbing). The broadcast is subscribed **only** for `Shutdown`; the
actor publishes nothing on it (network-authority pin, §5).

### D-FT9 — The `[files_sync]` config section & validation arms

`taskboard-app/src/config.rs`, the `[focus]` recipe verbatim:

```rust
#[serde(default, skip_serializing_if = "FilesSyncSection::is_default")]
pub files_sync: FilesSyncSection,          // on AppConfig, after `focus`

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesSyncSection {
    /// Spawns the files-sync transport iff true AND the account
    /// identity + credential resolve. Defaults to false — the absent
    /// table is the off posture: nothing spawns, nothing validates.
    pub enabled: bool,                     // #[serde(default)]
    /// Poll-cadence override in seconds (D-SF8: defaults to the
    /// `[sync] poll_interval_secs` the board sync rides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_secs: Option<u64>,
}
```

Validation-ladder arms (after the `[focus]` arms, same
deterministic-validation posture — validate what is written, not what
is live):

1. `files_sync.poll_secs == Some(0)` → `ConfigError::InvalidDuration`
   `{ field: "files_sync.poll_secs" }` (the `[sync]`-knobs precedent:
   only zero is garbage; no ceiling — a slow poll is a legitimate
   choice).
2. `files_sync.enabled` and the **default connector** (first
   `configured_connectors()` entry; empty ⇒ standalone world) is not
   `SyncBackend::Nextcloud` → new
   `ConfigError::FilesSyncWithoutNextcloud { field: "files_sync.enabled" }`
   (Q4: the substrate binds to the one Nextcloud account the Deck
   connector uses; a standalone world has no account to bind).

Mint: `config.files_sync_config() -> FilesSyncConfig` (the
transport-crate type, expect-on-validated):

```rust
pub struct FilesSyncConfig {
    pub poll_interval: Duration,       // poll_secs.unwrap_or([sync] poll_interval_secs)
    pub backoff_initial: Duration,     // the [sync] backoff knobs — same knobs, one cadence family
    pub backoff_max: Duration,
    pub clock: Arc<dyn Clock>,         // injected at the spawn site (bootstrap), SystemClock in production
    pub app_version: String,           // the heartbeat's app_version
}
```

The account identity is **not** part of the mint: bootstrap resolves
the default connector's `server_url`/`username` (the `ConnectorSpec`
it already built) and the credential via `CredentialStore.load` at the
spawn gate (ADR 0008 env escape unchanged, F2) — exactly the
`NextcloudModule::build_and_spawn` order.

### D-FT10 — Bootstrap wiring (mode-agnostic, D-SF18)

- A pure spawn-decision function beside the `focus_spawn_decision`
  precedent: `files_sync_spawn_decision(&config) ->
  Result<Option<FilesSyncSpawnSpec>, BootstrapError>` — `Some(spec)`
  iff `[files_sync] enabled` ∧ the default connector is Nextcloud
  (validate() already guarantees the shape; the decision re-derives,
  never re-validates). **No run-mode arm**: a kiosk is a device of the
  fleet (D-SF18); the mode-agnostic posture is pinned by a test that
  spawns under `AppMode::Kiosk` (§5 commit 7).
- Spawn site: `bootstrap_with_modules`, after the connector registry
  loop, beside the focus/escalation spawns. The spec carries the
  identity (`ConnectorId::from_kebab("files-sync")` — the constant
  proven kebab by a pinned unit test, the `DEFAULT_CONNECTOR_NAME`
  precedent), server URL, username; the gate then loads the
  credential (failure ⇒ typed `BootstrapError::FilesSyncCredential`,
  boot unwinds like a module failure — the user enabled the feature;
  fail loudly at boot, D-SF18's "credentials resolve" conjunct) and
  builds `WebDavFilesTransport` (URL rejection ⇒
  `BootstrapError::FilesSyncClient`).
- `device.json` path: `config.db_path().parent()`'s sibling policy is
  wrong — the fixed home is `dirs::data_dir()/taskboard/device.json`;
  platform without a data dir ⇒ typed boot error
  (`BootstrapError::FilesSyncDataDir`). The parent directory is
  created with `tokio::fs::create_dir_all` before the spawn (the
  db-parent precedent).
- `App` gains `files_sync_join: Option<JoinHandle<()>>` +
  `files_sync_status: Option<watch::Receiver<FilesSyncStatus>>`
  (chunk 4's surface consumes the receiver; `App::shutdown` joins the
  handle with the same bounded-join policy as every other actor).
- **No `UserDataReport`, no reports channel, no engine touch** — the
  seam chunk 3 will fill is the `LocalDocument` port (D-FT11), already
  in the spawn signature so chunk 3 adds an implementation, not
  surgery on the loop.

### D-FT11 — The chunk-3 seams this chunk defines (and stops at)

```rust
/// The device's own contribution to the merged view (D-SF5's "+ the
/// local engine state as one more input"; D-SF12's materialise-from-
/// state). Chunk 2 ships the empty document; chunk 3 derives it from
/// the published ArcSwap<AppState> (the journal section).
pub trait LocalDocument: Send + Sync {
    fn document(&self) -> UserDocument;
}
```

Chunk 2's production impl is the empty document (`EmptyDocument`) —
the actor's cycle, config gate, and wiring are complete and honest
without the engine (peers fold, the device publishes its merged view,
the heartbeat carries its hash). Chunk 3 adds the engine-backed impl
and the report emission. Deliberately **not** defined here: the
report family, any channel into the engine, the attach fold-first
ordering over engine state (chunk 3 owns D-SF13; this chunk's attach
publishes `fold(seed, peers)` which has no local contribution to
resurrect).

### D-FT12 — The test architecture & catalogue

Three tiers, mirroring the Deck connector's exact conventions:

**(a) In-module unit + proptest** (`#[cfg(test)] mod tests` per
module): the etag store's write/consume rules, the extractor + fuzz
targets (D-FT5), the verbatim-`Etag` pins (Display roundtrip; the
constructor rejects nothing — it stores octets; the *client* pins the
wire forms), `device.rs` ceremonies over `tempfile` dirs (create-once,
corrupt-refusal, tmp-rename atomicity, missing-parent error),
`poll.rs`-style backoff reuse is Deck's (no re-test — the same
function is already law-tested there; this crate tests only its
`retry_after_floor` plumbing if it grows any wrapper).

**(b) The fake-transport matrix** (`src/test_support.rs`, behind
`test-support` so chunk 3's convergence journeys reuse it):
`FakeFilesTransport` — a scripted server: per-path queued responses,
request log (the exact observable: method + path + headers incl. the
verbatim `If-Match` value + body), listing state, and *scenario
directives* (reorder members, duplicate members, serve garbage body,
serve torn body, answer 412, vanish a device). The actor tests
(`actor.rs` tests or `tests/actor_fake.rs` — stage-2's call, in-module
preferred for private-state access) run `#[tokio::test(start_paused)]`
with `poll_interval` far beyond the horizon and **advance paused time
to drive cycles** (no nudges exist; the status watch's change is the
completion signal). The matrix (one behavior per test):

1. attach journey: MKCOL ladder + identity create-once + first
   publish (request log asserts the exact verb/header sequence).
2. identity create race: 412 ⇒ adopt (GET follows; no error).
3. quiet fleet: unchanged dir etags ⇒ zero GETs, zero PUTs
   (request-count observable; the F6 cheap-skip posture).
4. changed peer: dir etag changes ⇒ exactly two GETs, fold updates,
   publish iff hash differs (identical-content re-read ⇒ skip-PUT —
   the F5 rule, request-count pinned).
5. reordered/duplicated listing members ⇒ identical fold outcome
   (the fold's order-freedom expressed at the transport level).
6. garbage appdata body ⇒ Quarantined park, file untouched (no
   write to that path), fold proceeds from the other peers.
7. newer-format document ⇒ NewerFormat park, folded nothing.
8. torn/failed GET ⇒ typed cycle failure, backoff, honest retry next
   cycle (no cache poisoning).
9. unexpected 412 on own rewrite + GET 200 ⇒ `ForkDetected`: the path
   joins `forked_paths`, **zero further PUTs to it across advanced
   cycles**, reads continue, status watch carries the fork.
10. 412 + GET 404 ⇒ `OwnFileVanished`: recreate via `put_create`
    next cycle, no fork.
11. publish_seed retirement: adopted own file's rows survive the first
    publish; a later vanish of the seeding device drops them (the
    re-publication semantics pinned end-to-end).
12. vanished device ⇒ its rows leave the merged view; own appdata
    updates (hash differs) exactly once.
13. cold-store no-blind-PUT pin: a fresh boot whose first publish
    needs a rewrite always GETs the own file first (D-FT4).
14. the authority pin: cycles including failures emit **nothing** on
    the injected broadcast (only `Shutdown` is consumed).
15. corrupt device.json ⇒ the actor parks with the typed finding,
    the server sees zero requests; the file is never rewritten.

**(c) Wiremock client tests** (`tests/client_wiremock.rs`, the
`actor_wiremock.rs` harness conventions): exact request-fact pins —
`If-None-Match: *` on create; `If-Match` echoing the served (quoted)
etag **byte-exact** (the wiremock matcher asserts the raw header);
PROPFIND body constant + `Depth: 1`; 405-on-MKCOL ⇒ `AlreadyExists`;
409-on-MKCOL-parent-missing ⇒ typed error; Retry-After delay-seconds
parse + floor plumbing; 429/503 retry budget exhaustion; Basic auth +
redacted-Debug pin.

**(d) Docker-tier pins** (`tests/it_docker.rs` + `tests/common/`,
`#[ignore]` + `it_nextcloud_docker_*` names, skip-never-fail without
the tier env, `with_deadline` + `run_id` isolation, riding the
existing `scripts/nextcloud_it_setup.sh up` invocation):

| Pin | Asserts (the characterization re-pinned against the controlled image) |
|---|---|
| `it_nextcloud_docker_files_create_once` | F3: `put_create` twice ⇒ 201 then typed 412; the winner's file serves the winner's bytes |
| `it_nextcloud_docker_files_lost_update_guard` | F4: GET ⇒ `If-Match: <served etag verbatim>` ⇒ 204/Updated; stale etag ⇒ typed 412; **the verbatim pin**: a double-quoted etag (munged) ⇒ server rejects (412) — the bug class is dead at the wire, not in a comment |
| `it_nextcloud_docker_files_etag_churn` | F5: byte-identical re-PUT ⇒ a *different* etag; etag stable across GETs; (asserts inequality — no ceilings) |
| `it_nextcloud_docker_files_tree_listing` | F6: PROPFIND Depth 1 over `devices/` ⇒ member etags incl. the trailing-slash collection form; a child file write changes the parent dir's etag (the dirty-marker fact the cycle leans on) |
| `it_nextcloud_docker_files_if_match_missing_is_412` | the D-FT6 disambiguation's server fact: `If-Match` against a nonexistent path ⇒ 412 |
| `it_nextcloud_docker_files_mkcol_posture` | MKCOL new ⇒ 201; MKCOL existing ⇒ 405 (the exists-ok mapping's live basis) |

## 4. Decision summary

| ID | Question | Decision |
|---|---|---|
| D-FT1 | Crate shape | `crates/taskboard-sync-files-nextcloud`, 11 files; workspace deps only (adds the two internal path pins; no new external dep; `cargo deny` untouched); `test-support` feature; lib tier, `forbid(unsafe)` |
| D-FT2 | Transport seam & verbs | `FilesTransport` trait (mkcol/propfind_devices/get/put_create/put_if_match); **no DELETE/MOVE/PROPPATCH/LOCK by construction**; `Etag` verbatim newtype; retry mirrors `DeckClient` (transport class only, `Retry-After` floor, 412 never retried); no percent-decoder (expected-form href matching + one trailing-slash strip) |
| D-FT4 | Etag store | `EtagStore { device_dirs, files }` + `LoopState` (docs, heartbeats, publish_seed, published_hash, pending_heartbeat, forked_paths, parked, streak/floor); in-memory only; **no guarded PUT without a stored etag** (cold boot GETs first) |
| D-FT5 | Extractor | One fixed `PROPFIND_DEPTH1_BODY` (getetag only); hand-rolled 207 scanner, prefix-agnostic DAV local names, typed `PropfindParseError`, empty-multistatus = legal empty listing; three fuzz targets (arbitrary bytes, structured tearing, codec bytes) as proptests; cargo-fuzz stays deferred (recorded escalation) |
| D-FT6 | Fork status & findings | rewrite-412 on own path ⇒ disambiguating GET: 404 ⇒ `OwnFileVanished` (recreate next cycle), 200 ⇒ `ForkDetected` — stop-write that path, keep reads forever, loud (`tracing::error!` + status watch), never re-PUT; create-412 ⇒ adoption, never fork; corrupt own-file adopt ⇒ stop-write + Quarantined; channel = `watch<FilesSyncStatus>` |
| D-FT7 | Device identity | `<data>/taskboard/device.json` (path injected from app); read-or-create-once via tmp+rename; corrupt ⇒ typed refusal, **never regenerate** (silent-fork guard); `DeviceId::mint()` the single creation site |
| D-FT8 | Attach + cycle | MKCOL ladder (201/405 exists-ok) → identity create-once/adopt (F3) → first read → first publish; steady cycle = PROPFIND → dirty/vanished sets → tolerant re-read → `fold(local, seed, peers)` → `StateHash` skip-compare (local hash only, F5) → guarded PUTs (appdata then info) → status/backoff; no nudge channel in chunk 2; broadcast = Shutdown-only |
| D-FT9 | Config | `[files_sync] { enabled = false, poll_secs? }`, focus-recipe absent-table posture + `deny_unknown_fields`; arms: `poll_secs = 0` → `InvalidDuration`, enabled ∧ default connector ≠ Nextcloud → `FilesSyncWithoutNextcloud`; mint `files_sync_config()` (poll override, `[sync]` backoff knobs, clock, app_version); credential at spawn gate (ADR 0008 unchanged) |
| D-FT10 | Bootstrap | pure `files_sync_spawn_decision` (focus precedent); spawns beside focus/escalation, **not** in the connector registry; mode-agnostic (kiosk spawns — pinned); credential/URL/data-dir failures = typed boot errors; `App` carries join + status receiver |
| D-FT11 | Chunk-3 seams | `LocalDocument` port defined, `EmptyDocument` impl shipped; no report family, no engine channel, no D-SF13 ordering here |
| D-FT12 | Tests | four tiers: in-module units + proptests; the 15-scenario fake-transport matrix (`test-support`, paused-time, request-count observables); wiremock request-fact pins (verbatim `If-Match` byte-exact); six docker-tier `it_nextcloud_docker_files_*` pins of F3–F6 + the two server facts the design leans on |

## 5. Test plan (TDD-first — the failing tests come before each commit's code)

### Commit 1 — `feat(sync-files): the crate skeleton, the etag store, and the transport seam`
Manifest + workspace entries; `lib.rs`, `error.rs`, `etag.rs`,
`transport.rs`. Failing tests first: `Etag` Display/AsRef verbatim
roundtrip (a quoted value survives untouched); `EtagStore` write rules
(dir etag upsert, file etag upsert, prune-on-vanish); the
`FilesTransport` outcome types' construction table (documented
outcomes only — the trait compiles against a stub impl). Store rule
pin: no read-back API beyond what the cycle needs (private fields,
accessors named for the cycle's questions).

### Commit 2 — `feat(sync-files): the propfind extractor and its fixed bodies`
`extract.rs`. Failing tests first: the body constant's exact bytes
(structural: parses as the documented XML shape — a parse of the
constant itself, no text-beyond-structure assertions); 207 sample ⇒
members (href + etag, trailing-slash strip, unmatched hrefs ignored);
zero-response multistatus ⇒ empty listing; truncated/garbage ⇒ typed
`PropfindParseError`; the three fuzz targets (D-FT5).

### Commit 3 — `feat(sync-files): the webdav client over the reqwest stack`
`client.rs`. Failing wiremock tests first: every verb's exact request
facts (method, path join, Depth/body, `If-None-Match: *`, `If-Match`
byte-exact-quoted); outcome mapping table (201/204/405/207/412/404/
401/403/429+Retry-After/503/5xx); retry budget on the transport class
only; credential-redacted Debug.

### Commit 4 — `feat(sync-files): the device identity ceremony`
`device.rs` + `tempfile` dev-dep. Failing tests first: create-once
(mint + write + re-read identity); tmp+rename leaves no residue;
corrupt file ⇒ typed `Unusable` + bytes untouched; missing parent ⇒
typed error.

### Commit 5 — `feat(sync-files): the attach ceremony and the cycle`
`status.rs`, `cycle.rs`, `actor.rs`, `test_support.rs` (the fake).
Failing tests first: the fake-transport matrix of D-FT12(b), tests
1–15, each naming its law before the code. This is the chunk's core
commit; the fork pins (9, 10) and the authority pin (14) are the
review-critical ones.

### Commit 6 — `feat(app): the files_sync config section`
`config.rs` delta. Failing tests first: absent table serializes
unchanged (the byte-identity pin); `deny_unknown_fields` on the
section; `poll_secs = Some(0)` arm; enabled+standalone arm; the mint
(expect-on-validated): override honored, default = `[sync]`
poll_interval, backoff knobs inherited.

### Commit 7 — `feat(app): the files-sync bootstrap spawn`
`bootstrap.rs` delta. Failing tests first: absent `[files_sync]` ⇒
`files_sync_spawn_decision` = `None` and **nothing spawns** (the
dispatch's bootstrap pin, behavior-level); enabled+kiosk ⇒ `Some`
(mode-agnostic pin); enabled+standalone ⇒ config error upstream;
credential failure ⇒ typed boot error unwind; the spawn-decision
function's purity (no IO in the decision; the gate does the IO).

### Commit 8 — `test(sync-files): the docker-tier pins of F3–F6`
`tests/it_docker.rs` + `tests/common/` (the harness copy: `LiveCfg`,
`with_deadline`, `run_id`, `retrying_server_errors`, the files-client
factory). The six pins of D-FT12(d). The docker tier rides the
existing invocation convention; nothing in CI's default path executes
it (`#[ignore]` + filter, skip-never-fail).

### Commit 9 — `docs(changelog): the nextcloud files transport` + gates green
CHANGELOG Unreleased entry. Full gate suite via
`scripts/run-quality-gates.sh` (fmt, clippy `-D warnings`, nextest,
doc tests, deny, llvm-cov sequence). `cargo deny` must be untouched
(no new external dep). No ADR this chunk (0020 landed with chunk 1;
D-SF19).

### Review pins (stage 3's checklist, from the dispatch)

1. Etag discipline: create-once uses `If-None-Match: *` with
   412→adopt; rewrites use `If-Match` with the **served** etag
   verbatim; **any etag string munging (trimming, re-quoting) is a
   finding** — the docker verbatim pin asserts it at the wire.
2. Fork alarm: an unexpected own-path rewrite 412 stops writes for
   that path and keeps reads; a retry loop that re-PUTs after a
   fork-indicating 412 is a finding. (The D-FT6
   vanished-file disambiguation is a *recorded interpretation* —
   review it against the design letter "an unexpected 412 … means
   another writer exists": the GET-404 arm narrows the alarm to the
   fork condition proper; if the reviewer rules differently, the
   narrowing is one test's deletion, not a redesign.)
3. Skip-PUT is local-hash-based (`StateHash::of`) — any use of server
   etags as content identity is a finding (F5).
4. The extractor tolerates garbage/torn bodies as typed rejections;
   the three fuzz targets exist and are adversarial (floors on
   garbage, no ceilings).
5. No network-authority claims (the broadcast pin), no board ops, no
   `SyncReport` emission/widening, no `UserDataReport` (chunk 3's) —
   grep the diff.
6. The docker-tier pins assert the conditional-PUT semantics
   (201/412, 204/412, verbatim echo, churn), not just HTTP
   reachability.
7. Absent `[files_sync]` spawns nothing (bootstrap pin); kiosk spawns
   (mode-agnostic pin).
8. Workspace dep pinning untouched beyond the two internal path
   entries; `cargo deny` diff empty.

## 6. Risks

- **The 412-disambiguation leans on an uncharacterized server fact**
  (`If-Match` against a missing file ⇒ 412): the docker tier pins it
  (commit 8) before the semantics ship to a live tier; if the image
  answers differently (e.g. 404), the transport's outcome mapping
  absorbs it and the disambiguation arm re-keys on the observed
  outcome — one mapping row, one pin update.
- **MKCOL exists-posture (405) was not in the F-table**: pinned by
  `it_nextcloud_docker_files_mkcol_posture`; the outcome mapping is
  the single place to adjust if the controlled image differs from the
  hosted tier.
- **Extractor leniency (prefix-agnostic local names)** is an
  interpretation of "narrow": the parse surface is still closed to
  `href`+`getetag` inside `multistatus`, and the fuzz targets bound
  the behavior. If review tightens it to namespace-verified matching,
  the change is local to `extract.rs`.
- **In-memory etag store** ⇒ one full-read cycle after every restart:
  bounded, honest, and the D-SF19 zero-migration posture's price;
  fleet-scale GET volume is small (two files per device).
- **Publish latency = poll cadence** (no nudge channel in chunk 2):
  D-SF8's ruling; chunk 3 may add the nudge when the journal's
  settle-to-roam path demands it.
- **Corrupt-own-file stop-write** is user-visible (files-sync halts
  publishing until the runbook fires): fail-closed by design (data
  loss is the alternative); chunk 4 owns the runbook that explains
  it.
- **Hosted-tier drift** (F1): the transport treats every surprise as
  a typed outcome, never a crash; the live re-pins are chunk 4's.

## 7. References

- Design round: `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  (§3 Parts C–D, §5, §7 chunk 2); rulings Q4/Q5/Q8 on #626; dispatch
  plan §3; issue #630.
- Transport facts (binding): `.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`
  F1–F8, §2 (notify_push fenced), §3 (hazard re-derivation).
- Chunk 1 (the substrate consumed): `.artifacts/plans/2026-11-05-sync-files-core-plan.md`
  (D-FC1…D-FC10); the crate itself at `crates/taskboard-sync-files`.
- In-tree mirrors: `crates/taskboard-sync-nextcloud/src/{actor,client,poll,backoff,error}.rs`
  (the actor/client/retry shapes), `crates/taskboard-app/src/{bootstrap,connectors,config}.rs`
  (registry, module seam, `[focus]` section recipe, validation
  ladder), `crates/taskboard-sync-nextcloud/tests/{it_docker,actor_wiremock}.rs`
  + `docker/nextcloud-test/compose.yaml` + `scripts/nextcloud_it_setup.sh`
  (the tiers), `docs/testing_strategy.org` §6/§8/§11 (determinism,
  tiers, fuzz posture).
