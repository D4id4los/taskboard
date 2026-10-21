# [Plan] Daily-Driver Chunk 3 — Sidecar Transport: 1.17.5 Gate + Anchor Ceremony + Document Codec + Evidence

- **Date**: 2026-10-22
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1/§4 (MD-1…MD-10); wave implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-2/4/5/10, §3 chunk 3, §4–§6; sidecar design doc
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` §1/§2/§3.1
  (D-S1)/§3.2 (D-S2)/§3.4 items 1–3/§3.5 (D-S5 arm shape)/§4 (the ruled
  1.17.5 gate)/§6 (ACCEPTED + the six session rulings)/§7 (chunk
  interfaces); field-family design doc §3.2 D-F2 (the folded startdate
  wire pins). All ACCEPTED rulings are treated as binding (MD-7).
- **Branch / worktree**: `feat/sync-appdata-transport` in
  `../taskboard-WT-feat-sync-appdata-transport`, off `origin/main` @
  `aac5097` (MD-9 verified: chunk 1 via PR #358's commits + the
  stage-1 signature file
  `.artifacts/plans/2026-10-21-daily-driver-task-field-family-plan.md`,
  chunk 2 via `7a43cca` + its two feat commits — all on `origin/main`).
  `scripts/worktree-doctor.sh` exited 0 (118 G free, floor 25 G).
- **Goal**: the sidecar's transport half — the dockerized Deck 1.17.5
  contract-fixture gate runs and passes **before** any transport code
  lands (DDF-10/MD-5); the domain document codec (`appdata.rs`) with
  verbatim preservation, union + GC pure functions, and manifest
  assertion 3; the anchor-ceremony constants and actor-local helpers;
  the typed `AppDataEvidence` on `SyncReport::Completed`; the
  `LocalOp::UpdateAppData` arm + storage tag; `LoopState`'s in-memory
  metadata/document cache; download-during-pull.
- **Status**: awaiting stage-2 implementation.

**Planning-method note (DDF-10 discipline)**: this planning stage ran
no docker probe — planning reads wire facts, it does not discover
them. The 1.17.5 gate therefore **executes in stage 2, first**, per
the acceptance ordering ("fixtures green before the transport code
lands"). No divergence was found or papered over during planning; the
decision points below are harness mechanics and codec shapes, not
adaptations to observed 1.17.5 behavior. Any fixture that fails
against the D1-evidenced (1.19.0) behavior in stage 2 is a
**STOP-and-report** finding (MD-5), never a design input.

---

## 1. Verified starting point (re-located 2026-10-22 @ `aac5097`)

Symbols are the stable keys; line numbers are today's references.

- **Attachment content surface** (sync `client.rs`): `attachments`
  :617, `upload_attachment` :652, `download_attachment` :696,
  `update_attachment_content` :744 (app-level POST route, `Scope::App`
  :185), `delete_attachment` :785, `restore_attachment` :823.
  `AttachmentUpload` :223 + `multipart_form` :238 (parts `type` =
  `ATTACHMENT_KIND` = `deck_file`, `file`; update adds `data`;
  filename = client filename = stored filename; no rename).
- **Wire model** (sync `model.rs`): `Card.attachments: Vec<Attachment>`
  :142 (the card *detail* read inlines full metadata);
  `Card.startdate` :126 (`duedate`'s twin, camelCase-aliased,
  null-tolerant); `Attachment` :326 (`id`/`created_at`/`deleted_at`/
  `last_modified`/`data: Option<ExtendedData>` string-or-object
  decode :357); `ExtendedData` :377 (`file_name`, `size` with the
  `filesize` alias :382, `mimetype`, `mtime`).
- **Card writes** (sync `client.rs`): `create_card` :991 takes
  `NewCard` (with `startdate` :144, `ser_opt_datetime` ISO-8601
  encoding); `update_card` :1021 full-send PUT of `&Card` (startdate
  serializes through the same helper). Chunk 1's tier-1 pins:
  `contract.rs` :757–844 (PUT body carries `startdate`; create posts
  ISO startdate; decode accepts value/null/absence). **No tier-2
  startdate round-trip exists yet** — every docker-suite `NewCard`
  passes `startdate: None` (common/mod.rs :253/:267/:435).
- **Cycle** (sync `actor.rs`): `run` :180 (select loop + nudge
  absorber); `LoopState` :165 (`cache: PullCache`, `rerun_requested`,
  `failure_streak`, `offline`, `retry_after_floor`); `cycle` :325
  (`read_at` :343 stamped pre-read; derive :357; per-board loop :421:
  push :434 → pull :484 → report :526); `pull_snapshot` :632 (three
  conditional reads, active+archived listing merge :762–784,
  **bound-card detail refresh** :809–829 — unconditional
  `client.card` per bound card, `*view = refreshed` overwrite, 404
  carve-out, dead-board skip); `PullCache` retention at cycle start
  :382–387 (memory proportional to the live set — the precedent for
  the document cache's retention).
- **Native merge / report seams** (domain): `SyncReport::Completed`
  (messages.rs :378 — `connector`/`snapshot: Box<_>`/`validators`/
  `pushes`/`read_at`/`duration`; `Failed` :409 carries no snapshot);
  `apply_sync_report` (pipeline.rs :390); `LocalOp` (outbox.rs :23) +
  `owning_board` :74 + `op_targets_task` :102 (both exhaustive);
  `plan_pushes` op-bucketing match (push.rs :338 — exhaustive);
  `TASK_FIELDS` + `FieldFamily` (entities.rs :98–122: 9 Native rows
  incl. `scheduled`, 3 AppData rows `effort`/`importance`/`urgency`);
  `Effort` `#[serde(transparent)]` u64 (entities.rs :49);
  `Importance`/`Urgency` snake_case enums (:71/:85); `ReminderRule`
  (reminders.rs :109 — `offset: ReminderOffset` (≤ 0, seconds
  semantics), `created_at`; the doc :117–119 reserves the additive
  write clock chunk 4 lands); `RemoteCardRef {board, stack, card}`
  (ids.rs :142); `RemoteStack {id, title, order, archived,
  deleted_at}` (remote.rs :42).
- **Op codec** (storage `codec.rs`): `op_kind_to_text` :96
  (snake_case tags; `create_task` :100, `update_task` :101, …),
  `op_from_columns` :182 (unknown tag / missing id ⇒ `CodecError`;
  tests :439–471 pin unknown-tag rejection + the legacy
  `rename_board` rejection — untouched by this chunk).
- **Harness**: `scripts/nextcloud_it_setup.sh` — already
  parameterizes `TASKBOARD_IT_SUFFIX` (:20, compose project +
  container name), `TASKBOARD_IT_PORT` (:23), `EXPORT_PREFIX` (:109,
  default `TASKBOARD_IT_DOCKER`); `install_deck` :75 does
  `occ app:install deck` (**latest** app-store version — the pin gap
  this chunk closes); `docker/nextcloud-test/compose.yaml` pins
  `nextcloud:35-apache` (35.0.1). Tier-2 tests: `tests/common/mod.rs`
  `read_cfg(prefix)` :89 (already prefix-parameterized),
  `live_docker_config` :71, `run_id` :103, `with_deadline` :113,
  `retrying_server_errors` :125, shared body
  `attachment_content_lifecycle(client)` :378 (upload/duplicate-409/
  byte round-trip/in-place update/stamp advancement/soft
  delete/restore); `tests/it_docker.rs` thin `#[ignore]` wrappers
  under `it_nextcloud_docker_*` names (e.g. :626) — the CI job
  `nextcloud-integration.yml` filters `test(it_nextcloud_docker)`
  (substring semantics), so a correctly-named test is picked up with
  zero workflow edits (the P2 precedent). Known-server matrix
  (testing_strategy.org §8): live tier = NC 33.0.9 + **Deck 1.17.5**;
  docker tier = NC 35.0.1 + Deck 1.19.0.
- **SyncReport::Completed construction sites** (the field addition's
  mechanical blast radius, today): sync `actor.rs` :530 (the one
  production site); state `tests/snapshots.rs` :250/:372/:782, state
  `tests/engine.rs` (5), domain `pipeline.rs` (2, test module),
  domain `merge.rs` (test module), sync
  `tests/actor_wiremock.rs`/`tests/e2e_hermetic.rs`,
  `taskboard-app/src/bootstrap.rs` (type mention). State-engine insta
  corpus: `crates/taskboard-state/tests/snapshots/` (20 `.snap`
  files, persisted + published state only).

## 2. Decisions (every open question, resolved)

- **D1 — 1.17.5 harness variant mechanics.** Reuse the existing
  parameterization; add exactly two knobs:
  1. `docker/nextcloud-test/compose.yaml`: interpolate the image —
     `image: ${TASKBOARD_IT_NC_IMAGE:-nextcloud:35-apache}`. Default
     unset ⇒ byte-identical behavior for the existing tier.
  2. `scripts/nextcloud_it_setup.sh`: a `DECK_VERSION` env (default
     empty = today's `occ app:install deck`). When set (e.g.
     `v1.17.5`), install the pinned release instead: download
     `https://github.com/nextcloud/deck/releases/download/$DECK_VERSION/deck.tar.gz`
     inside the container (`curl` is present — the healthcheck uses
     it), extract to `/var/www/html/custom_apps/deck`, then
     `occ app:enable deck` (skipping `app:install`), and verify the
     installed version via `occ app:list` (fail loud on mismatch or a
     failed download — never a silently wrong-tier instance).
  The variant instance runs fully through the existing hooks:
  `TASKBOARD_IT_SUFFIX=-deck1175 TASKBOARD_IT_PORT=8371
  EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175
  TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache DECK_VERSION=v1.17.5
  scripts/nextcloud_it_setup.sh up`.
- **D2 — NC image pin of the variant: `nextcloud:33-apache`.** The
  gate's contract is Deck 1.17.5 behavior; the production pair is
  NC 33.0.9 + Deck 1.17.5 (known-server matrix), so the variant
  reproduces the whole live pair rather than risking Deck 1.17.5's
  Nextcloud-version compatibility metadata against a 35 image (its
  declared NC range is unverified and the live evidence says 33).
  Residual risk recorded as R1.
- **D3 — fixture naming + CI filter: zero workflow edits.** Tests are
  named `it_nextcloud_docker_deck1175_*` (contains the existing
  `it_nextcloud_docker` filter substring ⇒ auto-picked by
  `nextcloud-integration.yml`'s expression) and read credentials via
  a second `read_cfg("TASKBOARD_IT_DOCKER_DECK1175")` reader — unset
  ⇒ **skip, never fail** (the house env-guard rule). Consequence,
  recorded per MD-5 honesty: in today's CI the deck1175 tests run as
  skips; stage 2's report and the PR body state exactly what ran
  where (locally against the pinned instance = the gate run). Wiring
  the pinned instance into CI is **not** in this chunk's diff
  (`ci.yml` is a no-go zone; `nextcloud-integration.yml` stays
  untouched — the gate is CI-able and reproducible in-repo, which is
  what §4 of the design rules; CI wiring is a user decision, noted in
  the stage report).
- **D4 — the gate's fixture set and exact assertions.** The six
  behaviors ride **shared bodies with thin per-tier wrappers** (the
  P2 pattern; MD-5's "no new mechanics beyond the pinned variant"):
  1. `attachment_content_lifecycle(client)` (existing shared body,
     unchanged) wrapped a second time as
     `it_nextcloud_docker_deck1175_attachment_content_lifecycle` —
     pins upload part names/stored filename, duplicate-409 ⇒ typed
     `Conflict`, byte-identical round-trip, in-place update with
     preserved `id`/`created_at`, soft delete (tombstone stays
     listed) + restore (content intact).
  2. **Additive assertion in the shared body** (both tiers then pin
     it): the download response carries **no `ETag`/
     `Last-Modified` headers** — the no-validators-on-content fact
     (D1 spike §3.2) asserted against the live tier, not just mocked.
  3. New shared body `startdate_round_trip(client)` wrapped as
     `it_nextcloud_docker_startdate_round_trip` (regular tier) **and**
     `it_nextcloud_docker_deck1175_startdate_round_trip` (the gate) —
     D-F2's "one assertion per direction": create a card with
     `startdate: Some(ts)` ⇒ read back equal (second-granularity
     tolerance, `>=`/round-to-second, never strict equality on a
     wall clock); full-send card PUT with a new `startdate` ⇒ read
     back the new value; create **without** `startdate` ⇒ read
     `None` (null-tolerance on the live wire, the decode path
     chunk 1 pinned only at tier 1).
  Everything asserts typed outcomes/structural facts (header
  presence, decoded values, request counts) — no text assertions.
- **D5 — divergence protocol.** A deck1175 fixture disagreeing with
  the D1-evidenced behavior (409 not surfacing, update not
  in-place, restore losing content, validators appearing, startdate
  write rejected) is STOP-and-report: transport commits do not land
  until the user rules (DDF-10/MD-5). Decode-shape drift *outside*
  the six behaviors (e.g. a field that decodes differently) goes to
  the known-server matrix as findings, not silent fixture edits.
- **D6 — domain `appdata.rs`: module + constants.** New pure domain
  module `crates/taskboard-domain/src/appdata.rs` (serde/chrono
  only; registered in `lib.rs`). It owns:
  - `pub const APPDATA_FORMAT: u32 = 1;`
  - `pub const ANCHOR_CARD_TITLE: &str = "[taskboard] app-data";`
  - `pub const MODELED_TASK_FIELD_KEYS: &[&str] = &["effort",
    "importance", "urgency"];` (assertion 3's subject, D12).
  **Anchor-title const home (recorded deviation, allowed against
  prose per MD-7):** the implementation plan names "ids.rs or
  entities" as candidate homes; the plan settles on `appdata.rs` —
  the const *is* the sidecar's on-server convention and belongs with
  the document types it serves; `ids.rs` is id newtypes, `entities`
  is entity shapes. The ACCEPTED ruling (exported from the domain
  crate so the UI/app can consume it without a sync dependency;
  sync consumes the domain const) is satisfied exactly. The filename
  and MIME constants stay sync-side (only the connector uses them):
  `anchor.rs` (D8) carries `ANCHOR_ATTACHMENT_FILENAME =
  "taskboard-appdata.json"` and `ANCHOR_ATTACHMENT_MIME =
  "application/json"`.
- **D7 — document types (serde shapes verbatim per D-S2's
  illustrations).** All types `Debug, Clone, PartialEq,
  Serialize, Deserialize`; map keys `String` (wire-faithful; the
  tasks/reminder-rules sections key on **decimal card ids**,
  reminder rules on the **rule UUID string**). Value-verbatim
  preservation (JSON value equality, not byte equality —
  re-serialization normalizes formatting) is the property the tests
  pin. `RemoteBoardId` serializes as the bare number (transparent
  newtype; stage 2 verifies the derive and adds `#[serde(transparent)]`
  on the document field if needed).

  ```rust
  pub struct AppDataDocument {
      pub format: u32,
      pub board: RemoteBoardId,
      pub sections: AppDataSections,
      /// Unknown envelope keys: carried verbatim (the D5 relay rule).
      #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
      pub unknown: BTreeMap<String, serde_json::Value>,
  }

  pub struct AppDataSections {
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub tasks: Option<BTreeMap<String, TaskEntry>>,
      #[serde(rename = "reminder-rules", default,
              skip_serializing_if = "Option::is_none")]
      pub reminder_rules: Option<BTreeMap<String, RulesEntry>>,
      /// Unknown sections: carried verbatim.
      #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
      pub unknown: BTreeMap<String, serde_json::Value>,
  }

  pub struct TaskEntry {
      /// Modeled keys only — exactly `MODELED_TASK_FIELD_KEYS`
      /// (assertion 3). Custom serde splits the wire `fields` object:
      /// modeled keys decode typedly, everything else lands in
      /// `unmodeled`.
      #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
      pub fields: BTreeMap<String, TaskFieldWrite>,
      /// Unmodeled field keys (inside `fields`) and unknown keys beside
      /// `fields` in the entry object: byte-for-value verbatim.
      #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
      pub unmodeled: BTreeMap<String, serde_json::Value>,
  }

  /// A modeled key's `{ "v": <encoding>, "clock": "<rfc3339>" }`.
  /// Strict shape; a modeled key that does not decode is demoted to
  /// `unmodeled` by the deserializer (D-D9's leniency rule).
  pub struct TaskFieldWrite {
      pub value: ModeledValue,
      pub clock: chrono::DateTime<chrono::Utc>,
  }

  /// The key selects the arm — `v`'s encoding is the domain field's
  /// own serde shape (transparent u64 / snake_case unit / null).
  pub enum ModeledValue {
      Effort(Option<Effort>),
      Importance(Option<Importance>),
      Urgency(Option<Urgency>),
  }

  pub struct RulesEntry {
      /// Keyed by the `ReminderRuleId` UUID string (D-S9: the
      /// document *is* the rule's remote identity).
      #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
      pub rules: BTreeMap<String, RuleEntry>,
      #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
      pub unknown: BTreeMap<String, serde_json::Value>,
  }

  /// `{ "offset": -900, "created": "…", "clock": "…" }` |
  /// `{ "deleted": true, "clock": "…" }` — hand-written serde (serde's
  /// internally-tagged enums do not support boolean tags).
  pub enum RuleEntry {
      Rule { offset: crate::reminders::ReminderOffset,
             created: chrono::DateTime<chrono::Utc>,
             clock: chrono::DateTime<chrono::Utc> },
      Tombstone { clock: chrono::DateTime<chrono::Utc> },
      /// A shape this version does not model: preserved verbatim,
      /// never folded (D-S2's materialization rule).
      Verbatim(serde_json::Value),
  }
  ```

  Serde specifics settled here so stage 2 does not re-derive them:
  `TaskEntry` and `RulesEntry`/`RuleEntry` impl serde **manually**
  (the modeled/unknown split of one wire map, and the boolean
  `deleted` tag, are beyond derive); `AppDataDocument`/
  `AppDataSections` derive with `flatten` catches. `ModeledValue`'s
  `v` encoding: `Effort(None)` ⇒ `null`, `Effort(Some(e))` ⇒ the
  transparent number; importance/urgency ⇒ the snake_case unit
  string or `null`. The `clock` is rfc3339 via chrono's serde.
- **D8 — sync-side ceremony module: `crates/taskboard-sync-nextcloud/
  src/anchor.rs` (new, `pub(crate)`).** The actor keeps only
  cycle-level wiring (SRP / one-level-of-abstraction mandate); the
  ceremony is one coherent subsystem. Contents:
  - the two sync-side constants (D6);
  - `pub(crate) struct AnchorCandidate { pub card: RemoteCardRef,
    pub attachment_id: u64, pub last_modified: i64, pub filesize:
    i64, pub tombstoned: bool }` and `pub(crate) struct
    AnchorDiscovery { pub confirmed: Vec<AnchorCandidate>, pub
    unconfirmed: Vec<RemoteCardRef> }`;
  - `pub(crate) fn scan_snapshot(snapshot: &RemoteBoardSnapshot,
    details: &HashMap<RemoteCardRef, Vec<Attachment>>) ->
    AnchorDiscovery` — **pure**. Exact-title match against
    `ANCHOR_CARD_TITLE` over the snapshot's tasks (active + archived
    are already merged in the snapshot): no trim, no casefold —
    deliberately unlike `ReconcileByTitle` (the convention string is
    machine-owned). A candidate is *confirmed* when its card's
    captured detail attachments (or a dedicated detail read) carry
    an attachment whose stored filename
    (`ExtendedData.file_name`) equals
    `ANCHOR_ATTACHMENT_FILENAME` — live **or** tombstoned (both
    count, D-S1); otherwise *unconfirmed*.
  - `pub(crate) fn pick(discovery: &AnchorDiscovery) -> Option<&
    AnchorCandidate>` — **pure**, the deterministic pick rule:
    newest attachment `lastModified`, tie-break lowest remote card
    id. (Multiple matching-filename attachments on one card cannot
    exist — the 409 duplicate rule — so per-card selection is
    unambiguous.)
  - `pub(crate) enum AnchorAction { Use(AnchorCandidate), Adopt(
    RemoteCardRef), Create, None }` + `pub(crate) fn anchor_action(
    discovery: &AnchorDiscovery, has_document_work: bool) ->
    AnchorAction` — **pure**, the ceremony state machine's decision
    face: confirmed carrier ⇒ `Use` (the pick rule; independent of
    document work — downloading is read-side); no carrier + exactly
    one unconfirmed match + work ⇒ `Adopt` (upload onto it —
    closes the interrupted-ceremony loop); no carrier + (zero or
    ≥ 2 unconfirmed matches) + work ⇒ `Create` (never touch
    ambiguous user cards); no work ⇒ `None` for the write arms.
    **Chunk 3's live cycle passes `has_document_work = false`** (no
    flush exists yet), so only `Use`/`None` fire in production this
    chunk; `Adopt`/`Create` are unit-pinned (their execution rides
    chunk 4's flush).
  - `pub(crate) fn creation_target(stacks: &[RemoteStack], tasks:
    &[RemoteTask]) -> Option<(RemoteStackId, i64)>` — **pure**: the
    anchor is created in the first live stack by `order` (minimum),
    at last position (max card order in that stack + 1; 0 for an
    empty stack); `None` when the board has no live stack (⇒ the
    flush strands this cycle — a board without stacks has no card
    space; the actor never auto-creates stacks).
  - `pub(crate) fn build_evidence(bytes: &[u8], board:
    RemoteBoardId) -> AppDataEvidence` — **pure**: JSON slice decode
    fails ⇒ `Unreadable(Decode)`; `format > APPDATA_FORMAT` ⇒
    `Unreadable(FormatTooNew { format })`; `document.board ≠ board`
    ⇒ `Unreadable(ForeignBoard)`; else `Document(Box(document))`.
- **D9 — evidence + strictness decisions.**
  - `AppDataEvidence`/`AppDataUnreadable` live in domain
    `messages.rs` beside `SyncReport` (evidence vocabulary), shapes
    exactly per D-S4:
    `enum AppDataEvidence { Document(Box<AppDataDocument>), Absent,
    Unreadable(AppDataUnreadable) }` — **boxed** per D-S4 (a
    document dwarfs the other fields; messages.rs's own
    snapshot-boxing precedent); `enum AppDataUnreadable {
    FormatTooNew { format: u32 }, Transport, Decode, ForeignBoard }`.
    Derives: `Debug, Clone, PartialEq` only — `SyncReport` itself
    carries no serde (in-process mpsc, never persisted, never on the
    IPC wire); serde lives on the document types in `appdata.rs`.
  - `SyncReport::Completed` grows `app_data: AppDataEvidence`;
    **`Failed` is unchanged** (a failed cycle folds nothing — D-S4).
  - The D-S7 table's failed-download row says "`app_data: None`" —
    pre-enum shorthand (the enum is the later ruling; the dispatch
    trap list: "evidence as a bare `Option` — the typed enum is the
    ruling"). Resolved mapping: a download transport failure on an
    otherwise-completing cycle ⇒ `Unreadable(Transport)` (the reason
    travels — that is the loud-fence ruling's point), never a bare
    `Option` and never silence.
  - **Leniency rule** (planning interpretation of D-S2's
    materialization rule — "unresolvable or unknown entries are not
    errors: dropped from local state, preserved in the document"):
    a *modeled* key whose `{v, clock}` shape does not decode (e.g.
    an effort value a peer wrote as a string, or > `i64::MAX`) is
    demoted to verbatim `unmodeled` content — never a whole-document
    `Decode` failure. Rationale: one malformed key must not blind
    the board's entire app-data fold; the fold (chunk 4) only ever
    sees typed values so demoted content cannot enter local state;
    the strictness boundary is the **envelope** (missing `format`,
    non-document JSON ⇒ `Decode`) — ours, and cheap to be strict
    about.
  - Tombstoned-attachment carrier (only matching attachment has
    `deleted_at ≠ 0`): counts for *confirmation* (D-S1) but yields
    `Absent` evidence and **no download** this chunk — D-S4's
    `Absent` definition names "tombstoned attachment" as the normal
    silent state; restore-then-replace is chunk 4's recovery and is
    referenced, not built.
- **D10 — cycle integration points** (sync `actor.rs`; all inside the
  per-board loop, after the pull, before the report):
  1. `pull_snapshot` returns a small struct
     `Pulled { snapshot, validators, detail_attachments:
     HashMap<RemoteCardRef, Vec<Attachment>> }` — the bound-card
     detail refresh (:809–829) records `detail.attachments` for
     every detailed card (zero marginal GETs; D-S4 item 2). The
     dead-board skip means a dead board contributes nothing — the
     ceremony is skipped entirely for dead boards (evidence
     `Absent`; every read of a dead board answers 404/403).
  2. Discovery: `scan_snapshot` over the pulled snapshot; unconfirmed
     candidates not covered by `detail_attachments` (the unbound /
     adoption-cycle case) get one dedicated `client.card` GET each
     (≤ the number of exact-title matches, normally 0–1).
  3. `pick` + evidence decision table (see §4.2) + `LoopState`
     cache maintenance (D11).
  4. The report carries `app_data` — every completion path sets it
     (at minimum `Absent`).
- **D11 — `LoopState` cache fields + cold-start semantics.**

  ```rust
  struct LoopState {
      // …existing fields unchanged…
      /// Per-board sidecar document cache: the attachment metadata
      /// pair of the last successful ingest plus the parsed document
      /// (in-memory only — D-S4 item 3 / §6 ruling 5; no new
      /// `ValidatorKey` kind: attachment content has no HTTP
      /// validators, so the persisted validator table is the wrong
      /// home).
      documents: BTreeMap<RemoteBoardId, CachedAppData>,
  }

  struct CachedAppData { last_modified: i64, filesize: i64,
                         document: AppDataDocument }
  ```

  - Retention mirrors `PullCache`: entries for boards that left the
    derived set are pruned at cycle start (:382 precedent) — memory
    proportional to the live set.
  - Change detection: metadata equal ⇒ skip the content GET and
    emit `Document(cached.document.clone())` (re-folding is
    idempotent — chunk 4's guarantee); differ, or no entry (cold
    start / prior failure) ⇒ `download_attachment` + `build_evidence`.
  - **Success-only caching**: only a `Document` ingest inserts or
    refreshes an entry. `Decode`/`FormatTooNew`/`ForeignBoard`/
    `Transport` leave the cache untouched ⇒ the next cycle re-GETs —
    the honest retry (negligible: one GET per board per cycle, and
    the fence says fold nothing anyway). Recorded, not optimized.
  - Refresh-from-upload-response (so a flush does not re-GET its own
    write) is chunk 4's; nothing here writes the cache outside
    ingest.
- **D12 — manifest assertion 3 (domain `appdata.rs` tests).** Two
  layers so the check is structural, not a const-vs-const tautology:
  1. `MODELED_TASK_FIELD_KEYS` (the codec's single source for its
     modeled-key dispatch) equals `TASK_FIELDS`' rows filtered to
     `FieldFamily::AppData` — exactly `["effort", "importance",
     "urgency"]`, and `scheduled` is among the **Native** rows.
  2. Decode probes: a document carrying each modeled key round-trips
     into the typed arm; `scheduled`, the reserved `task-kind`, and
     the reserved `proofs` section all land verbatim-unmodeled —
     **no `scheduled` key is ever modeled** (D-F2; a modeled
     `scheduled` arm is a finding).
- **D13 — the op arm's mechanical perimeter (MD-10 honored).**
  `LocalOp::UpdateAppData(TaskId)` (D-S5's shape + doc comment) lands
  in domain `outbox.rs`, which **forces** the exhaustive matches it
  participates in — the chunk grows exactly those arms and nothing
  more:
  - `owning_board`: task-op arm via the task→stack→board chain
    (D-S5's board resolution);
  - `op_targets_task`: id-equality arm (the id-scoping rule);
  - storage `codec.rs`: `op_kind_to_text` ⇒ `"update_app_data"`,
    `op_from_columns` ⇒ `UpdateAppData(parse_id(task_id)?)` —
    additive; no legacy tag renamed (the one-representation rule
    holds); round-trip + missing-id tests mirror the per-op
    pattern;
  - `plan_pushes` (push.rs :338): a **totality arm that buckets the
    op into no group** (fence comment naming chunk 4's document
    group). Nothing enqueues the op yet (MD-10: no enqueue-site
    diff anywhere; the interim stands), so the arm is unreachable
    in production this chunk — it exists for match totality only.
    Stage 2 verifies against the live code whether the planner
    asserts every-op-bucketed and, if so, uses an explicit
    empty-bucket local (still producing no group) — never an
    `unreachable!()`, never a native `UpdateTask` misroute.
  - **Not** in this chunk: the engine's `pending_task_intents`
    `UpdateAppData` arm, any enqueue, the fold, the fence-comment
    amendments (all chunk 4).
- **D14 — `apply_sync_report` mechanics (the "pipeline.rs context
  only" boundary).** Adding the field to `Completed` forces the
  engine's match to acknowledge it: the `app_data` binding is
  consumed as `_` with a one-line comment pointing at chunk 4's fold
  (`D-S3`/`D-S4 item 4`). That is compilation, not fold logic — and
  it is the *only* pipeline.rs diff in this chunk. The test-side
  `Completed` constructions (§1's inventory) gain
  `app_data: AppDataEvidence::Absent` mechanically.
- **D15 — insta-pass catalogue.** Expected **zero `.snap` drift**:
  the evidence is consumed-and-ignored and neither `AppState` nor
  persisted state grows a document-shaped field (design §7:
  "`AppState` gains nothing document-shaped"). The chunk's snapshot
  work is therefore: (a) the mechanical field additions at the
  inventoried construction sites (D14); (b) **if** any `.snap` file
  drifts anyway, that drift is itself a finding to diagnose (evidence
  leaking into state), and only then does the widening own one
  deliberate `insta` regeneration — reviewed as "the exact leak,
  nothing else"; no `.pending` files ship (DF-9's rule). Stage 2
  re-runs the `grep -rn "SyncReport::Completed {"` inventory at its
  start (symbols, not line numbers).
- **D16 — tier/test placement matrix** (one behavior, one test,
  lowest layer):
  - Domain units + proptests (default suite): codec round-trips,
    verbatim preservation, union/GC semantics, `build_evidence`'s
    checks, assertion 3.
  - Sync units (`anchor.rs` + `actor.rs` `#[cfg(test)]`): scan
    exact-match discipline, pick determinism, `anchor_action`
    preconditions, `creation_target`, evidence decision table
    (pure, clock-seamed).
  - Hermetic actor integration (`tests/actor_wiremock.rs`): cycle
    end-to-end over a mocked server — document reaches the engine
    as typed evidence; `Absent` silence; each `Unreadable` variant's
    trigger; download-failure ⇒ cycle still `Completed` with
    `Unreadable(Transport)`; cache ⇒ content served exactly once
    across two cycles; two confirmed carriers ⇒ only the winner is
    downloaded (the pick rule observed via request counts); a
    case/whitespace-mutated title ⇒ no match, no download.
    Creation/adoption stay unit-pinned (the cycle cannot flush yet —
    D8); the `create_card`-with-title IO is already contract-pinned
    generically.
  - Docker tier 2 (regular): the existing lifecycle + the new shared
    `startdate_round_trip` wrapper.
  - Docker tier 2 (deck1175 variant): the same two wrappers under
    `it_nextcloud_docker_deck1175_*` names — the DDF-10 gate
    (D3/D4).

## 3. The evidence decision table (chunk 3's construction rules)

| Cycle state (per board) | `app_data` evidence | Cache action |
|---|---|---|
| Dead board (ceremony skipped) | `Absent` | untouched |
| No anchor-titled card in the snapshot | `Absent` | untouched |
| Title match, no confirmed carrier, no work | `Absent` | untouched |
| Confirmed carrier, attachment live, metadata == cache entry | `Document(cached.clone())` | untouched |
| Confirmed carrier, live, metadata ≠ entry or cold start | download ⇒ `build_evidence` | `Document` ⇒ insert/refresh; anything else ⇒ untouched |
| Download transport error (after retries) | `Unreadable(Transport)` | untouched |
| Body not decodable | `Unreadable(Decode)` | untouched |
| `format > APPDATA_FORMAT` | `Unreadable(FormatTooNew { format })` | untouched |
| Decoded `board` ≠ cycling board | `Unreadable(ForeignBoard)` | untouched |
| Only matching attachment tombstoned | `Absent` (no download) | untouched |

Cycle-completion invariant (D-S7): none of the download/decode rows
fails the cycle — the snapshot and card pushes are unaffected
evidence; the transport-level *pull* failure path is unchanged
(`Failed`, no evidence field). Fence enforcement (fold nothing / no
upload) is chunk 4's consumer logic; chunk 3 delivers the typed
reasons.

## 4. Test design (TDD — tests written first per tier)

### 4.1 Domain `appdata.rs` (default suite)

- Round-trips: envelope + both sections + modeled fields encode and
  decode back equal (unit, per-type).
- **Verbatim preservation property** (proptest): an arbitrary
  document mixing modeled content with unknown sections, unknown
  field keys, unknown keys beside `fields`, unknown rule shapes, and
  unknown envelope keys decodes and re-encodes with every unknown
  part value-identical (`serde_json::Value` equality) and modeled
  content preserved.
- **Union properties** (proptest + examples): per-key newer-clock
  winner; equal clocks ⇒ overlay; `union(d, d) == d` (idempotence);
  unknown sections/keys survive the union from the base.
- **GC**: `prune_absent_cards` removes exactly the entries (both
  known sections, tombstones included) whose card key ∉ present;
  never touches present-card entries or unknown sections (property).
- Leniency: a modeled key with an undecodable `v` demotes to
  `unmodeled` verbatim (unit); a corrupt envelope decodes as
  `Decode`-class failure (unit — the strict boundary).
- Assertion 3 (D12).

### 4.2 Sync `anchor.rs` + actor (units + hermetic wiremock)

- Units per D16 (scan/pick/action/creation_target/decision table via
  the injected clock; the actor's evidence paths unit-tested where
  pure).
- Wiremock integration per D16 — each listed scenario as one test
  with a typed/structural observable (report variant, request
  counts, cache behavior). No sleeps: the actor's existing
  hermetic harness patterns (nudge + report drain).

### 4.3 Docker tier 2 (both variants)

- Shared bodies per D4; wrappers thin (`#[ignore]`, env-guarded,
  `with_deadline`, run-id resources, board teardown).
- Stage 2 records the tier-2 evidence summary (what ran, against
  which Deck/NC versions, outcomes) in its report; captured payloads
  never land (round-wide evidence clause) — PR-body excerpts only.

## 5. Commit ladder (each standalone, rebase-merge hygiene)

1. `test(sync): parameterize the docker tier for a pinned deck version`
   — compose image interpolation + the setup script's `DECK_VERSION`
   path (D1/D2). The default tier's behavior is provably unchanged.
2. `test(sync): deck 1.17.5 contract fixtures (attachments + startdate)`
   — the deck1175 cfg reader, the no-validators assertion in the
   shared body, the `startdate_round_trip` shared body, and the three
   wrappers (regular startdate + two deck1175). **Stage 2 runs the
   gate here, before any transport commit**; a divergence stops the
   chunk (D5).
3. `feat(domain): app-data document codec with verbatim preservation`
   — `appdata.rs` (types per D7, union + GC per D2-of-design §3,
   constants per D6), `lib.rs` registration, assertion 3, the
   proptest suite.
4. `feat(domain): app-data evidence and the update_app_data op arm`
   — `messages.rs` evidence enums + the `Completed.app_data` field;
   `outbox.rs` arm + `owning_board`/`op_targets_task`; the
   `plan_pushes` totality arm; storage codec tag; the mechanical
   test-site updates + the `apply_sync_report` `_` binding (D13/D14).
5. `feat(sync): anchor discovery and document evidence ingestion`
   — `anchor.rs` per D8, `pull_snapshot`'s `Pulled` struct +
   detail-attachment capture, the cycle wiring + evidence table,
   `LoopState.documents` (D10/D11), the hermetic wiremock suite.
6. `docs(changelog): the sidecar transport and the 1.17.5 gate` —
   the Unreleased line.

## 6. Scope fence & no-go checklist (checked by stage 3)

- **No** fold, flush, health derivation, rebuild, restore-then-
  replace, enqueue sites, `pending_task_intents` arms, fence-comment
  amendments, CLI/UI, anchor-task hiding (chunks 4/5).
- **No** persisted validator kind; **no** storage schema change (the
  codec tag is a new value in an existing TEXT column; no migration).
- `pipeline.rs`'s diff is exactly D14's match-arm acknowledgment.
- The interim (MD-10) stands: nothing enqueues `UpdateAppData`.
- `ci.yml` and `nextcloud-integration.yml` untouched (D3).
- The deck1175 gate runs before commits 3–5; any divergence is
  STOP-and-report, never adapted (D5).
- Captured tier-2 payloads stay out of the repo (evidence in the
  stage report / PR body).
- SPDX headers on new files (MIT OR Apache-2.0 — domain + sync are
  library-tier); `#![forbid(unsafe_code)]` holds crate-level.
- Gates per AGENTS §10, incl. `cargo deny` (no new dependencies are
  expected — serde/chrono/uuid only; a surprise dep is a finding).

## 7. Risks

1. **Deck 1.17.5 × Nextcloud 35 compatibility (R1)** — mitigated by
   the NC 33 image pin (D2); if the pinned install still fails, the
   script fails loud and stage 2 reports the harness finding (not a
   behavioral divergence, not silently worked around).
2. **1.17.5 behavioral divergence on the six behaviors** — the gate's
   reason to exist; STOP-and-report path (D5). The design's
   expected-degraded stance for the live tier stands until the user
   rules.
3. **1.17.5 `startdate` write acceptance unknown** — exactly what
   `startdate_round_trip` measures (D-F2 gated the write path on this
   fixture); a rejection is a STOP-and-report finding, not an adaptation.
4. **Release-tarball availability for `v1.17.5`** — the GitHub release
   asset URL is the pin; a missing asset fails loud (D1).
5. **Local second-instance hygiene** — the suffix/port/export hooks
   make the variant coexist with the singleton; the documented
   invocation (D1) is the contract; `down` is suffix-scoped already.
6. **D9's leniency rule is a planning interpretation** (of D-S2's
   materialization rule) — recorded here and in the stage report for
   user visibility; it does not contradict any ACCEPTED ruling (the
   strict boundary is the envelope; unknown content is never dropped).
7. **`flatten`-based verbatim mechanics** rely on `serde_json::Value`
   preservation — value-verbatim, not byte-verbatim (D7); the
   property tests pin exactly that guarantee so the D5 relay claim
   stays honest.

## 8. Estimated diff

≈ **1,700 added lines** across ~16 files: harness ~200 (script ~35,
compose ~2, common ~120, it_docker ~50); domain `appdata.rs` ~650
(types/codec ~330, tests + proptests ~280, assertion 3 ~40);
messages/outbox/push/codec/mechanicals ~180; sync `anchor.rs` ~280 +
actor/pull wiring ~150 + wiremock suite ~250; CHANGELOG ~4; test-site
field additions ~40 spread.
