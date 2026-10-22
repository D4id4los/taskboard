# [Plan] Daily-Driver Chunk 3 — Sidecar Transport: 1.17.5 Gate + Anchor Ceremony + Document Codec + Evidence

- **Date**: 2026-10-22 (revised same day: the version-capability
  re-run — see Status).
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1/§4 (MD-1…MD-10, as amended 2026-10-22); wave implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-2/4/5/10 (as amended), §3 chunk 3, §4–§6; sidecar design doc
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` §3.1
  (D-S1)/§3.2 (D-S2)/§3.4 items 1–3/§3.5 (D-S5 arm shape)/§6 (ACCEPTED
  + the six session rulings)/§7 (chunk interfaces) + its 2026-10-22
  amendment note; field-family design doc §3.2 D-F2 (as amended
  2026-10-22). **Normative new inputs (the re-run)**: the
  version-capabilities design addendum
  `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`
  (D-V1 seam, D-V2 lane rule, D-V3 wire changes + revised fixtures,
  §4 obligations) and the characterization report
  `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`
  (the measured evidence). All ACCEPTED rulings are treated as binding
  (MD-7); the 2026-10-22 ruling — version-specific parity, no
  sync-functionality degradation — supersedes the original
  expected-degraded stance.
- **Branch / worktree**: `feat/sync-appdata-transport` in
  `../taskboard-WT-feat-sync-appdata-transport`, off `origin/main` @
  `aac5097` (MD-9 re-verified 2026-10-22: chunk 1 — `8015b6b`…`26544d6`
  + the stage-1 signature file
  `.artifacts/plans/2026-10-21-daily-driver-task-field-family-plan.md`
  — and chunk 2 — `7a43cca`/`ac1eff8`/`fe8f846`/`aac5097` — all on
  `origin/main`). `scripts/worktree-doctor.sh` exited 0 (95 G free,
  floor 25 G). **The branch already carries the first run's commits**
  (verified `git log`; they ride this chunk's PR, not redone):
  `84ab03b` the harness parameterization (compose image interpolation
  + `DECK_VERSION` pin incl. the `nextcloud-releases` asset-URL fix),
  `332c89b` the fixture set, `08562fa` the stage-2 STOP report,
  `14b0b20` the characterization report, `000ea0b` the design
  addendum + doc amendments, `0430d26` the dispatch-plan amendment.
- **Goal**: the sidecar's transport half — the **capability seam**
  (D-V1: one capabilities probe, typed `DeckFeatures` flag table,
  conservative fallback), the unconditional attachment-create `data`
  field + echo-decode fallback (D-V3), the **revised gate fixtures
  green on both tiers** before any transport code lands (DDF-10 as
  amended); capability-gated card conversion (D-V2's push half); the
  domain document codec (`appdata.rs`) with verbatim preservation,
  union + GC pure functions, **`scheduled` as the fourth modeled key**
  and the reworded manifest assertion 3; the anchor-ceremony constants
  and actor-local helpers; the typed `AppDataEvidence` on
  `SyncReport::Completed`; the `LocalOp::UpdateAppData` arm + storage
  tag; `LoopState`'s in-memory flags/metadata/document cache;
  download-during-pull.
- **Status**: revision 2 (supersedes the 2026-10-22 morning text where
  the addendum says so — D4/D5 and the risks §7; the commit ladder's
  first two rungs are landed). Awaiting stage-2 implementation (the
  dispatch plan's RE-RUN Prompt 3.2).

**Planning-method note (DDF-10 discipline, re-run)**: the gate already
ran once (the first run's stage 2) and its findings are *measured
facts* in the characterization report; this revision plans the ruled
adaptation, it does not discover behavior. Wire facts are cited from
the characterization, never assumed. A **new** divergence — a fixture
disagreeing with the characterized behavior on either tier, or any
1.17.5/1.19.0 surface observed to differ from the report — is
**STOP-and-report** (MD-5), never a design input and never a silent
fixture edit. The ruled adaptation set is exactly: the capability seam
(D-V1), the `scheduled` lane rule (D-V2), the unconditional wire
changes (D-V3); anything beyond it is a finding.

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
  the landed `DECK_VERSION` knob closes, §1.1);
  `docker/nextcloud-test/compose.yaml` pins
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

### 1.1 The capability-seam sites (re-run additions; re-located on the branch @ `0430d26`)

Symbols are the stable keys; line numbers are today's references. The
first run's commits touched only `client.rs` (the
`download_attachment_validators` fixture seam), the docker test files,
`compose.yaml`, and the setup script — every §1 reference above is
unchanged.

- **Request plumbing** (sync `client.rs`): `Call` :174 (`method`,
  `resource`, `scope`, `body`, `validators`) and `Scope` :185
  (`OcsApi` ⇒ `DECK_API_PATH` = `index.php/apps/deck/api/v1.0`,
  `App` ⇒ `index.php/apps/deck`; `path_prefix` :196); `attempt` :1504
  joins `base_url + prefix + resource`, always sends
  `OCS-APIRequest: true` + `Accept: application/json` + Basic auth
  (:1516–1518). The capabilities route is a **third prefix**
  (`ocs/v2.php/cloud/capabilities` — outside both Deck scopes): a new
  `Scope` variant is the minimal join-compatible change; the header +
  auth plumbing is already correct for it.
- **`DeckClient::new` :338 is a sync constructor** (URL validation +
  `Client::builder()`; the `with_sleeper` :374 test seam pattern). A
  capabilities GET cannot run inside it — D-V1's "parsed once in
  `DeckClient::new`" prose is honored via the recorded deviation in
  D18 (probe once per actor lifetime, cached in `LoopState`).
- **OCS envelope** (sync `ocs.rs`): `OcsEnvelope<T>`/`OcsBody<T>`
  (`meta.statuscode` + `data`) — the capabilities payload decodes
  through the same envelope; unknown fields are ignored (the module's
  stated rule).
- **`multipart_form`** (sync `client.rs` :238): takes
  `(upload, for_update)` and adds the `data` text part only on the
  update arm (:250–254); `CallBody::Multipart(upload, for_update)`
  (the `attempt` match :1522) threads the flag. D-V3.1 makes `data`
  unconditional ⇒ the `for_update` discriminator loses its purpose
  and is removed (D19).
- **`ExtendedData`** (sync `model.rs` :377: `file_name` :379 with
  `#[serde(default)]`, `size` :382 `filesize` alias, `mimetype`,
  `mtime`) decodes via `data_from_object_or_string` :357 (string form
  = filename-only) for `Attachment.data` :347 and via plain derive for
  `Attachment.extended_data` :350. **The D-V3.2 fallback site**: the
  1.17.5 create/update echo carries the name under
  `extendedData.info.filename` with no `fileName` key
  (characterization §2A) — the fallback lands in `file_name` at decode
  (D20), so the landed fixtures' `.file_name` assertions hold
  unchanged on both tiers.
- **Card write conversion** (sync `mapping.rs`): `new_card_from`
  :213 (`startdate: shape.scheduled`, the create conversion) and
  `apply_card_shape` :269 (`card.startdate = shape.scheduled`, the
  full-send PUT conversion) — the two D-V2 gating sites; both take
  the flag after the revision (D21). Wire forms today: `NewCard`
  .startdate skips `None` (create omits the key — create ignores it
  on 1.17.5 anyway); **`Card.startdate` serializes `null`** (no
  `skip_serializing_if`, `ser_opt_datetime` — model.rs :126), so the
  PUT carries explicit `startdate: null` exactly as D-V2 rules once
  the conversion maps `scheduled → None`. Callers:
  `push_exec/execute.rs` :344 (`new_card_from`) and :640
  (`apply_card_shape`), both inside `PushExecutor` (constructed
  `PushExecutor::new(client, overlay)` :32, called from the cycle at
  `actor.rs` :455) — the flag threads through the executor (D21).
- **Actor startup** (sync `actor.rs`): `run` :180 builds `LoopState`
  at :192 (`cache`/`rerun_requested`/`failure_streak`/`offline`/
  `retry_after_floor`) before the select loop — the probe's insertion
  point (D18): one `capabilities()` GET before the first cycle, the
  derived `DeckFeatures` cached as a new `LoopState` field.
- **`push_exec/tests.rs`**: ~25 `PushExecutor::new(&client, overlay…)`
  call sites (exact-request-count and echo assertions) — the
  constructor's new parameter is a mechanical sweep; the probe also
  adds one unmatched-route GET per hermetic actor start, which
  exact-count assertions must absorb (D18's audit note).
- **Landed fixtures** (the first run's `332c89b`, riding this PR —
  the revision edits, never weakens): `tests/common/mod.rs`
  `live_docker_deck1175_config()` (the `TASKBOARD_IT_DOCKER_DECK1175`
  reader, skip-when-unset), the no-validators assertion inside the
  `attachment_content_lifecycle` shared body (both tiers pin it), the
  `startdate_round_trip(client)` shared body (three legs: create
  reads back, PUT reads back, create-without decodes `None`);
  `tests/it_docker.rs` wrappers `it_nextcloud_docker_startdate_round_trip`
  (regular), `it_nextcloud_docker_deck1175_attachment_content_lifecycle`,
  `it_nextcloud_docker_deck1175_startdate_round_trip` (this one
  **renames** per D23). The `it_nextcloud_docker` filter-substring
  convention (D3) is preserved by the new names — zero workflow
  edits stands.

## 2. Decisions (every open question, resolved)

> Numbering is continuous with revision 1. D1–D3 are **landed**
> (`84ab03b`) and kept as the record of the harness mechanics; D4/D5
> are **rewritten** by the re-run (the addendum's revised fixture set
> and divergence protocol); D6–D16 stand unchanged; D17–D24 are the
> re-run's new decisions (the D-V1/D-V2/D-V3 wiring).

- **D1 — 1.17.5 harness variant mechanics. (landed, `84ab03b`)** Reuse the existing
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
     *(Landed amendment, first run: that GitHub URL 404s — the script
     pins the `nextcloud-releases` org's versioned asset
     `deck-v1.17.5.tar.gz` instead; STOP report §4. Kept here as
     provenance, not as an instruction to restore.)*
  The variant instance runs fully through the existing hooks:
  `TASKBOARD_IT_SUFFIX=-deck1175 TASKBOARD_IT_PORT=8371
  EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175
  TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache DECK_VERSION=v1.17.5
  scripts/nextcloud_it_setup.sh up`.
- **D2 — NC image pin of the variant: `nextcloud:33-apache`. (landed,
  `84ab03b`)** The
  gate's contract is Deck 1.17.5 behavior; the production pair is
  NC 33.0.9 + Deck 1.17.5 (known-server matrix), so the variant
  reproduces the whole live pair rather than risking Deck 1.17.5's
  Nextcloud-version compatibility metadata against a 35 image (its
  declared NC range is unverified and the live evidence says 33).
  Residual risk recorded as R1.
- **D3 — fixture naming + CI filter: zero workflow edits. (landed,
  `84ab03b`/`332c89b`; stands)** Tests are
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
- **D4 — the gate's fixture set and exact assertions (rewritten by
  the re-run; addendum D-V3's revised set).** The gate must be **green
  on BOTH tiers** — the regular tier (NC 35.0.1 / Deck 1.19.0) and the
  pinned gate tier (NC 33 / Deck 1.17.5) — and the fixtures pin the
  *characterized* behavior, which on the gate tier now includes two
  measured server limitations. Shared bodies with thin per-tier
  wrappers (the P2 pattern; the naming rule stays D3's):
  1. `attachment_content_lifecycle(client)` — **runs unchanged on both
     tiers** (as landed in `332c89b`, including the no-validators
     pin). Once D19/D20's client changes land, its create/update-echo
     assertions hold on 1.17.5 too — that *is* the parity proof for
     the attachment surface; nothing about the fixture is weakened or
     special-cased per tier.
  2. `startdate_round_trip(client)` — **regular tier only**
     (`it_nextcloud_docker_startdate_round_trip`, unchanged): the
     capability-true-server pin. On 1.19.0 create reads the startdate
     back, full-send PUT reads the new value back, create-without
     decodes `None`.
  3. **`it_nextcloud_docker_deck1175_startdate_round_trip` renames to
     `it_nextcloud_docker_deck1175_startdate_unsupported`**, wrapping
     a new shared body `startdate_unsupported_on_deck1175(client)`
     pinning the *measured* 1.17.5 behavior (characterization §2B —
     the server is the fixed party; this pin is the capability
     table's evidence, not our behavior debt, so it is not a
     `..._currently_pins_...` test):
     create with `startdate: Some(ts)` ⇒ **200**, read back `None`
     (create ignores the field); full-send card PUT with a valued
     `startdate` ⇒ **200**, read back still `None` (the silent
     write-drop — no 400, no effect). Typed outcomes only.
  4. **New shared body `capabilities_probe(client)`** wrapped as
     `it_nextcloud_docker_capabilities` (regular) **and**
     `it_nextcloud_docker_deck1175_capabilities` (gate): the D-V1
     probe succeeds against both real servers, the version decodes
     (`"1.19.0"` / `"1.17.5"`), and the derived flag matches the
     tier's pinned expectation (`card_startdate` true / false) — the
     seam itself pinned end-to-end, per tier.
  Everything asserts typed outcomes/structural facts (decoded values,
  request counts, header absence) — no text assertions.
  **Gate execution order**: commits 3–5 of the revised ladder (§5)
  land the seam + wire changes; the gate suite then runs green on
  both tiers **before the transport commits** (codec/evidence/
  ceremony) land — the amended acceptance ordering.
- **D5 — divergence protocol (rewritten by the re-run).** The
  characterized behavior is now the *expected* behavior. A deck1175
  or regular-tier fixture disagreeing with the characterization
  report (or with the D1-evidenced 1.19.0 behavior) — 409 not
  surfacing, update not in-place, restore losing content, validators
  appearing, `startdate` suddenly honored on 1.17.5, capabilities
  missing the `deck.version` key — is **STOP-and-report**
  (DDF-10 as amended/MD-5): the ruled adaptation set is exactly
  D-V1/D-V2/D-V3, and anything outside it (a third divergence, a
  different echo shape, an unmeasured version row) stops the chunk
  for a user ruling. The flag table (D17) is the ruled repair path
  *after* such a ruling: a new divergence on an unknown version
  re-enters via stop-and-report, then extends the table — never a
  silent adaptation, never a raw version comparison in the connector.
  Decode-shape drift outside the pinned set still goes to the
  known-server matrix as findings, not silent fixture edits.
- **D6 — domain `appdata.rs`: module + constants.** New pure domain
  module `crates/taskboard-domain/src/appdata.rs` (serde/chrono
  only; registered in `lib.rs`). It owns:
  - `pub const APPDATA_FORMAT: u32 = 1;`
  - `pub const ANCHOR_CARD_TITLE: &str = "[taskboard] app-data";`
  - `pub const MODELED_TASK_FIELD_KEYS: &[&str] = &["effort",
    "importance", "urgency", "scheduled"];` (assertion 3's subject,
    D12; the fourth key is D-V2's fallback-lane ruling — the const is
    capability-blind, the emission/fold sites gate).
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
  /// own serde shape (transparent u64 / snake_case unit / rfc3339 or
  /// null). `scheduled` is the fourth modeled key (D-V2): same
  /// envelope as the others, `null` = cleared, **no** tasks-section
  /// tombstones.
  pub enum ModeledValue {
      Effort(Option<Effort>),
      Importance(Option<Importance>),
      Urgency(Option<Urgency>),
      Scheduled(Option<DateTime<Utc>>),
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
      /// The connector's derived capability flags (D18): probed once
      /// per actor lifetime before the select loop, `CONSERVATIVE` on
      /// any probe failure. In-memory like everything else here.
      features: DeckFeatures,
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
- **D12 — manifest assertion 3 (domain `appdata.rs` tests; reworded by
  the re-run per D-V2 — the key set is now four).** Two layers so the
  check is structural, not a const-vs-const tautology:
  1. `MODELED_TASK_FIELD_KEYS` (the codec's single source for its
     modeled-key dispatch) equals `TASK_FIELDS`' rows filtered to
     `FieldFamily::AppData` **plus `scheduled`** (the Native row whose
     sync lane is dynamic per D-V2) — exactly
     `["effort", "importance", "urgency", "scheduled"]`, with
     `scheduled` structurally checked to remain a manifest row and to
     be absent from the AppData rows (the lane decision lives in the
     sync/pipeline layers, never in the codec).
  2. Decode probes: a document carrying each of the four modeled keys
     round-trips into its typed arm (`Scheduled(Some)`/`Scheduled(None)`
     = cleared, same `{v, clock}` envelope); the reserved `task-kind`
     and the reserved `proofs` section land verbatim-unmodeled. The
     codec is **version-blind**: `appdata.rs` never names
     `DeckFeatures` and carries no capability branch (a codec-level
     capability branch is a dispatch-trap finding; review checks this
     structurally).
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
  - Docker tier 2 (regular): the existing lifecycle + the
    `startdate_round_trip` wrapper + the `capabilities_probe`
    wrapper (D4's revised set).
  - Docker tier 2 (deck1175 variant): the lifecycle wrapper
    (unchanged body), `…_startdate_unsupported` (the measured pin),
    and `…_capabilities` — the DDF-10 gate as amended, green on both
    tiers (D3/D4/D23).

### 2.1 The capability-seam decisions (the re-run's D-V1/D-V2/D-V3 wiring)

- **D17 — flag types + table: a new pure domain module
  `crates/taskboard-domain/src/capabilities.rs`** (registered in
  `lib.rs`; serde-free, dependency-free — pure functions only, per the
  domain crate rule). Contents:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct
    DeckFeatures { pub card_startdate: bool }` — a plain struct, one
    field per ruled flag (additive rows extend it); plus
    `DeckFeatures::CONSERVATIVE` (all false) as the single
    fallback value every degradation path converges on.
  - `#[must_use] pub fn flags_for(version: &str) -> DeckFeatures` —
    **total, pure**: parse `"<major>.<minor>[.<patch>]"` (patch and
    any suffix tolerated-but-ignored); the known-row table maps
    major.minor ranges: `(1,17) ⇒ false`, `(1,18) ⇒ false` (the
    conservative arm — 1.18.x is unmeasured and deliberately not
    depended on, addendum §5.1), `≥ (1,19) ⇒ true` (the
    **newest-known-wins** optimistic rule: a version newer than the
    newest known row behaves like the newest known row). A version
    older than the oldest known row, or an unparseable string ⇒
    `CONSERVATIVE` (safe by construction — the conservative arm's
    wire forms work on every measured version).
  - Units + proptest: every known row, the newest-known-wins rule
    (arbitrary `≥1.19` inputs), older-than-known and unparseable ⇒
    conservative. The table is the *only* version switch in the
    codebase (a raw version comparison anywhere in the connector is a
    dispatch-trap finding).
  **Home rationale**: chunk 4's merge-verdict gate and assertion 4's
  lane routing live in domain (`pipeline.rs`/`merge.rs`) and must name
  the type; the pure document codec stays version-blind (D12). Sync
  consumes the domain type exactly as it consumes every other domain
  vocabulary type.
- **D18 — the probe: typed client method + one-shot actor wiring.**
  - Client method (sync `client.rs`): `pub async fn capabilities(&self)
    -> Result<DeckCapabilities, DeckError>` — `GET` via a third
    `Scope::Ocs` variant (`path_prefix` ⇒ `"ocs/v2.php"`, resource
    `cloud/capabilities`), decoding `OcsEnvelope<CapabilitiesPayload>`
    with `CapabilitiesPayload { capabilities: CapabilitiesSection }`,
    `CapabilitiesSection { #[serde(default)] deck:
    Option<DeckSection> }`, `DeckSection { version: String }`
    (unknown fields ignored per the `ocs.rs` rule). New public type
    `DeckCapabilities { pub deck_version: Option<String> }` — `None`
    = the `deck` key absent (Deck not installed / ancient build);
    transport/HTTP failure = typed `Err`.
  - **Recorded deviation (allowed against prose per MD-7)**: D-V1
    says "parsed once in `DeckClient::new`", but `new` is a sync
    constructor (§1.1) and cannot perform a GET. The probe instead
    runs **once per actor lifetime**: `run` (`actor.rs` :180) issues
    `capabilities()` immediately after building `LoopState` (:192),
    before the select loop, and stores the derived flags in a new
    `LoopState.features: DeckFeatures` (probe `Ok` + version ⇒
    `flags_for(version)`; `Ok` + no deck key, or `Err` ⇒
    `CONSERVATIVE` + one typed `tracing::warn!` — the degraded lane
    is *correct*, never an error surfaced to the cycle). One extra
    GET per client construction (per connector actor), not per cycle,
    matching D-V1's cost statement; an actor respawn (reconnect /
    re-attach) re-probes — that is the "until the probe succeeds"
    recovery path (addendum §5.3), accepted without a retry timer
    (no speculative machinery; a persistent probe failure means the
    conservative lane for the process lifetime).
  - **Mechanical consequence, audited in stage 2**: every hermetic
    actor test now sees one extra GET at start (wiremock answers
    unmatched routes with 404 ⇒ the probe degrades conservatively —
    existing tests keep their semantics). Tests asserting **exact
    request counts** (`tests/actor_wiremock.rs`,
    `push_exec/tests.rs`) are swept and updated mechanically (+1
    probe request or an explicit probe stub); a count assertion left
    unadjusted is a review finding. A dedicated stub answering the
    capabilities route is provided by a small helper in the wiremock
    harness for tests that want to pin a flag value.
- **D19 — the `data` field goes unconditional (D-V3.1).**
  `multipart_form` (`client.rs` :238) drops its `for_update`
  parameter: **both** create and update forms carry
  `text("data", upload.file_name)` (value = the client filename;
  measured inert — `FileService::create` overwrites the entity's
  `data` with the stored filename; required by 1.17.5, tolerated by
  1.19.0). `CallBody::Multipart(upload, for_update)` collapses to
  `CallBody::Multipart(AttachmentUpload)`; the two call sites
  (`upload_attachment`, `update_attachment_content`) pass the same
  form. **Unconditional means unconditional**: gating the field
  behind the capability probe is a dispatch-trap finding (one request
  form for all versions). The in-place update route is untouched
  otherwise (already sends `data`; verified working on 1.17.5
  as-is). Tier-1 contract pins of the multipart form gain the
  create-side `data` part assertion.
- **D20 — the echo-decode fallback (D-V3.2), in `model.rs`.**
  `ExtendedData` grows `#[serde(default)] pub info:
  Option<ExtensionInfo>` with `ExtensionInfo {
  #[serde(default, rename = "filename")] pub file_name: String }`
  (serde ignores unknown `info` keys). A decode-side normalization —
  `file_name` empty ⇒ take `info.file_name`, else stay empty —
  applies at **both** decode paths (`data_from_object_or_string`'s
  object arm, and a `deserialize_with` wrapping the plain derive on
  `Attachment.extended_data`), so every decoded `ExtendedData` carries
  the stored filename in `.file_name` regardless of the server's echo
  shape. Consequence: the landed fixtures' `extendedData.fileName ==
  upload_name` assertions hold **unchanged on both tiers** (§1.1);
  consumers (the anchor scan's filename comparison, D8) read the
  already-normalized field. Model units pin: the 1.19.0 echo shape,
  the 1.17.5 echo shape (`info.filename`, no `fileName`), the string
  form, and absence ⇒ `None`.
- **D21 — capability-gated card conversion (D-V2's push half).** The
  two conversion sites take the flag: `mapping::new_card_from(shape,
  features)` sets `startdate` to `shape.scheduled` only when
  `features.card_startdate`, else `None`; `apply_card_shape(fetched,
  shape, resolved_labels, features)` likewise. Wire forms fall out of
  today's serde (§1.1): create omits the key (`NewCard` skips `None`;
  1.17.5 create ignores it anyway), the full-send PUT serializes
  `startdate: null` (`Card` has no skip attr) — exactly D-V2's
  "keeps the wire honest" form, no serde changes needed. Threading:
  `PushExecutor::new(client, overlay)` grows a third parameter
  `features: DeckFeatures` (the executor is per-cycle, constructed in
  the cycle that owns `LoopState.features` at `actor.rs` :455); the
  ~25 `push_exec/tests.rs` call sites pass an explicit fixture
  constant (`DeckFeatures::CONSERVATIVE` or the native-true value) —
  the sweep is mechanical and each changed line is reviewable as
  "flag in, same shape out". Connector-independence holds: the lane
  choice is a connector concern (D-V2); domain `push.rs` shapes keep
  carrying `scheduled` untouched (the D-F2 `from_task` pins stay
  green — only the wire conversion gates).
- **D22 — the codec's fourth modeled key (D-V2's codec half).**
  `ModeledValue::Scheduled(Option<DateTime<Utc>>)` (D7, amended);
  encoding = the field's own domain shape (`null` = cleared, rfc3339
  otherwise) in the same `{v, clock}` envelope; `MODELED_TASK_FIELD_KEYS`
  = the four names (D6, amended); assertion 3 reworded (D12,
  amended). **No tombstones in the tasks section** — `Scheduled(None)`
  is a cleared value, not a deletion marker (D-S2's rule unchanged).
  The codec never reads the flag (version-blind; D12's structural
  check). Union/GC treat the new key exactly like the other three
  (embedded-clock LWW; no special case).
- **D23 — the revised gate set + lane-rule boundaries (the
  re-run's D-V2/D-V3 obligation summary).** The gate fixture set is
  D4 (rewritten); the run order is §5's revised ladder: seam commits
  land first, the gate runs **green on both tiers** before any
  transport commit. Lane-rule boundaries this chunk enforces by
  construction: the deck1175 startdate fixture pins the server's
  unsupported behavior (never our adaptation of it); the native
  round-trip pin lives only on the capability-true tier; **no**
  fold-side gating, no document emission gating, no union lane-GC,
  and no merge-verdict skip ships in this chunk (all chunk 4 —
  dispatch non-goals; chunk 3 ships the flag seam + the codec's
  fourth key + the gated card-write encoding only). D-V2's data-safety
  rationale (the always-absent remote must never clobber local
  `scheduled`) is recorded as the *requirement* chunk 4's
  merge-verdict gate satisfies; this chunk's `LoopState.features` is
  its intended input.
- **D24 — `LoopState.features` consumers this chunk.** Exactly the
  card-write conversion (D21). The evidence/anchor/ceremony paths are
  flag-independent (addendum §1: no flag touches the ceremony's
  conventions); the report carries no flag field (chunk 4 widens
  `SyncReport` if its gate needs the flag traveling — no speculative
  field now); `SyncClient`/probe/flag wiring adds no config surface
  (no `TASKBOARD_*` override — addendum §5.3 records the issue-class
  option if a deployment ever needs one).

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

### 4.0 The capability seam (re-run additions; default suite + wiremock)

- Domain `capabilities.rs` units/proptests per D17 (every known row,
  newest-known-wins, older/unparseable ⇒ conservative).
- Model units per D20 (the two echo shapes + string form + absence).
- Client/wiremock: the probe decodes a captured-shaped capabilities
  payload (both tiers' shapes: deck key present); no `deck` key ⇒
  `DeckCapabilities { deck_version: None }`; transport error ⇒ typed
  `Err`; the actor-start wiring derives `CONSERVATIVE` on both
  failure forms (unit via the injected client stubs, or the wiremock
  harness helper from D18).
- Mapping/executor: `new_card_from`/`apply_card_shape` under both
  flag values (valued `scheduled` survives native-true, becomes
  `None`/`null` conservative; a `None` scheduled is unchanged either
  way); the multipart create form carries the `data` part (D19's
  tier-1 pin).
- Existing exact-request-count assertions absorb the probe request
  (D18's sweep).

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

### 4.3 Docker tier 2 (both variants — the gate, per D4 as rewritten)

- The revised fixture set: `attachment_content_lifecycle` unchanged on
  both tiers; `startdate_round_trip` (regular, capability-true pin);
  `startdate_unsupported_on_deck1175` (gate, the measured pin); the
  `capabilities_probe` pair. Wrappers thin (`#[ignore]`, env-guarded,
  `with_deadline`, run-id resources, board teardown).
- Stage 2 runs the gate green on **both tiers** between the seam
  commits and the transport commits (§5's order) and records the
  tier-2 evidence summary (what ran, against which Deck/NC versions,
  outcomes) in its report; captured payloads never land (round-wide
  evidence clause) — PR-body excerpts only. A new divergence is
  STOP-and-report (D5).

## 5. Commit ladder (each standalone, rebase-merge hygiene; revised)

Already landed on the branch (ride this chunk's PR; never redone):

1. `test(sync): parameterize the docker tier for a pinned deck version`
   — `84ab03b`: compose image interpolation + the setup script's
   `DECK_VERSION` path (D1/D2, incl. the `nextcloud-releases` asset
   URL the first run's fail-loud path fixed).
2. `test(sync): deck 1.17.5 contract fixtures (attachments + startdate)`
   — `332c89b`: the deck1175 cfg reader, the no-validators assertion
   in the shared body, the `startdate_round_trip` shared body, the
   three wrappers. (Its deck1175 startdate wrapper renames in commit
   5 below; never weakened.)

The remaining ladder (stage 2 executes in this order; the gate run
sits between 5 and 6):

3. `feat(domain): the deck capability flag table` — `capabilities.rs`
   (D17): `DeckFeatures`, `flags_for`, the known-row table, units +
   proptest, `lib.rs` registration. Pure, no consumers yet —
   standalone by construction.
4. `feat(sync): capabilities probe and the capability-gated card
   write` — the `Scope::Ocs` variant + `DeckCapabilities` DTOs + the
   `capabilities()` method, `LoopState.features` + the `run` probe
   (D18); `new_card_from`/`apply_card_shape` gating + the
   `PushExecutor` parameter (D21); the wiremock harness probe helper
   + the exact-count sweep (D18's audit). Every count change is
   mechanically explained by the probe.
5. `fix(sync): unconditional attachment-create data field and the
   1.17.5 echo decode` — `multipart_form` simplification (D19), the
   `ExtendedData.info` fallback (D20), model units, the create-side
   contract pin; **the revised fixtures**: the
   `…_startdate_unsupported` rename + new body, the
   `capabilities_probe` pair, the regular-tier wrappers (D4/D23).
6. **The gate run**: the full docker fixture suite green on **both**
   tiers (default + deck1175), executed before any transport commit;
   outcomes recorded in the stage-2 report. A divergence stops the
   chunk (D5).
7. `feat(domain): app-data document codec with verbatim preservation`
   — `appdata.rs` (types per D7 incl. the `Scheduled` arm, union + GC
   per design-doc §3, constants per D6), `lib.rs` registration,
   the reworded assertion 3 (D12), the proptest suite.
8. `feat(domain): app-data evidence and the update_app_data op arm`
   — `messages.rs` evidence enums + the `Completed.app_data` field;
   `outbox.rs` arm + `owning_board`/`op_targets_task`; the
   `plan_pushes` totality arm; storage codec tag; the mechanical
   test-site updates + the `apply_sync_report` `_` binding (D13/D14).
9. `feat(sync): anchor discovery and document evidence ingestion`
   — `anchor.rs` per D8, `pull_snapshot`'s `Pulled` struct +
   detail-attachment capture, the cycle wiring + evidence table,
   `LoopState.documents` (D10/D11), the hermetic wiremock suite.
10. `docs(changelog): the sidecar transport and the capability seam`
   — the Unreleased line (cover the capability seam + sidecar
   transport, per the dispatch Prompt 3.2 bookkeeping).

## 6. Scope fence & no-go checklist (checked by stage 3)

- **No** fold, flush, health derivation, rebuild, restore-then-
  replace, enqueue sites, `pending_task_intents` arms, fence-comment
  amendments, CLI/UI, anchor-task hiding (chunks 4/5).
- **No** scheduled emission/fold *gating code* (chunk 4): no
  merge-verdict skip, no document emission gate, no union lane-GC,
  no assertion 4 lane routing. Chunk 3 ships exactly the flag seam +
  the codec's fourth key + the gated card-write encoding.
- **No raw version comparisons in the connector** — `flags_for`'s
  table is the only version switch (D17); no probe-gating of the
  `data` field (unconditional, D19); no codec-level capability
  branch (version-blind, D12/D22).
- **No** persisted validator kind; **no** storage schema change (the
  codec tag is a new value in an existing TEXT column; no migration);
  `LoopState.features` is in-memory like the document cache (D18).
- `pipeline.rs`'s diff is exactly D14's match-arm acknowledgment.
- The interim (MD-10) stands: nothing enqueues `UpdateAppData`.
- `ci.yml` and `nextcloud-integration.yml` untouched (D3).
- The gate runs green on **both tiers** after ladder commit 5 and
  before commit 7 (the transport commits); any divergence is
  STOP-and-report, never adapted (D5/D23).
- Captured tier-2 payloads stay out of the repo (evidence in the
  stage report / PR body).
- SPDX headers on new files (MIT OR Apache-2.0 — domain + sync are
  library-tier); `#![forbid(unsafe_code)]` holds crate-level.
- Gates per AGENTS §10, incl. `cargo deny` (no new dependencies are
  expected — serde/chrono/uuid only; a surprise dep is a finding).

## 7. Risks (revised: the first run's R2/R3 are resolved by the characterization; the re-run adds R8–R11)

1. **Deck 1.17.5 × Nextcloud 35 compatibility (R1)** — mitigated by
   the NC 33 image pin (D2); the first run proved the invocation
   works end-to-end. A re-run harness failure is still reported, not
   silently worked around.
2. **Release-tarball availability for `v1.17.5`** — resolved in the
   first run: the script pins the `nextcloud-releases` org's
   versioned asset (`deck-v1.17.5.tar.gz`) and fails loud (the STOP
   report §4). Kept as the recorded mechanism.
3. **Local second-instance hygiene** — the suffix/port/export hooks
   make the variant coexist with the singleton; the documented
   invocation (D1) is the contract; `down` is suffix-scoped already.
4. **D9's leniency rule is a planning interpretation** (of D-S2's
   materialization rule) — recorded here and in the stage report for
   user visibility; it does not contradict any ACCEPTED ruling (the
   strict boundary is the envelope; unknown content is never dropped).
5. **`flatten`-based verbatim mechanics** rely on `serde_json::Value`
   preservation — value-verbatim, not byte-verbatim (D7); the
   property tests pin exactly that guarantee so the D5 relay claim
   stays honest.
6. **R8 — the probe reshapes hermetic request counts** (re-run): one
   extra GET per actor start; 404 on unstubbed wiremock servers ⇒
   conservative flags (semantics preserved), but exact-count
   assertions must be swept (D18's audit). Risk is mechanical and
   reviewable line-by-line; a missed count is a finding, not a
   flake.
7. **R9 — 1.18.x unmeasured** (addendum §5.1): mapped conservative;
   a probe against a 1.18.x instance would extend the table
   (fixture-plus-row), out of scope here.
8. **R10 — optimistic newest-known-wins for future versions**
   (addendum §5.2): a future Deck release may diverge again; MD-5's
   stop-and-report protocol is the catch, the flag table the repair.
   The explicitly accepted trade-off of the 2026-10-22 ruling.
9. **R11 — capabilities-route failure on exotic deployments**
   (addendum §5.3): reverse proxies stripping OCS or hardened
   instances degrade to the conservative lane (correct, native
   `scheduled` rides the sidecar) until a successful probe
   (actor respawn re-probes, D18). Accepted; the `TASKBOARD_*`
   override env remains an issue-class option if it ever bites.
   Additionally the one-off transient 405 on freshly-enabled
   instances (characterization §3) is watchlist-noted: a first-cycle
   probe hitting it would conservatively degrade until the next
   actor respawn — harmless (no wrong writes either way), recorded.

## 8. Estimated diff (the remaining ladder on top of the landed commits)

≈ **2,050 added lines** across ~19 files (revision 1 estimated 1,700;
the re-run adds the seam). Harness/fixtures are already landed. The
remaining ladder: domain `capabilities.rs` ~130 (types/table ~50,
units + proptest ~80); probe method + `Scope::Ocs` + DTOs ~90;
`LoopState.features` + `run` wiring ~25; mapping/executor gating ~40 +
the `push_exec/tests.rs` count sweep ~25 mechanical lines; multipart
simplification ±10 net; model `info` fallback ~70; fixture revision
~100 (the `…_unsupported` body ~55, the `capabilities_probe` pair
~45) + contract pins ~15; domain `appdata.rs` ~680 (types/codec ~350
incl. the `Scheduled` arm, tests + proptests ~290, reworded assertion
3 ~40); messages/outbox/push/codec/mechanicals ~180; sync `anchor.rs`
~280 + actor/pull wiring ~150 + wiremock suite ~250; CHANGELOG ~5;
test-site field additions ~40 spread.
