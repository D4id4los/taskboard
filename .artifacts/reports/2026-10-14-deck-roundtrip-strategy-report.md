# [Report] Deck Round-Trip Strategy for App-Only Fields — D1 Spike

- **Date**: 2026-10-14
- **Context**: decision D1 of
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §4.1 — the keystone question gating Wave 1's field family
  (scheduled-start, effort, importance/urgency, task-kind/respite,
  recurrence parameters, reminder rules): which vehicle lets fields Deck
  cannot natively represent round-trip through a Deck sync connection?
  Chunk 1 of the wave-0 dispatch plan
  (`.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
  §2); investigation plan
  `.artifacts/plans/2026-10-14-deck-roundtrip-spike-plan.md`.
- **Method**: tier-2 docker Nextcloud (35.0.1 / Deck 1.19.0) via the
  standard harness; ad-hoc curl probes against the live app, source
  inspection in-container, and an upstream desk check. Every factual
  claim below carries its command + observed outcome, or a cited
  source. No production code was touched; all scratch payloads stayed
  in `/tmp` (the app password is redacted everywhere).
- **Recommendation (§4, exactly one)**: **the attachment sidecar — one
  board-anchored JSON document on a designated anchor card**, merged
  set-union-style with per-field clocks; all Wave-1 fields sync day
  one. Deck custom fields do not exist (established positively, §3.1).

---

## 1. The question, precisely

From wave analysis §4.1 and the spike plan §1.1: fields Deck has no
slots for — scheduled-start, effort, importance/urgency,
task-kind/respite flags, recurrence parameters, reminder rules — need a
round-trip vehicle through a Deck sync connection, or they stay
local-only and every Wave-1 feature ships with a migration story. The
candidates examined (plan §2.1): **V1** Deck custom fields (if any API
surface exists), **V2** an attachment-stored JSON sidecar document,
plus the rejected alternatives **X1** description-embedded markers,
**X2** card comments, **X3** labels-as-fields.

## 2. Method (reproducible from this section)

### 2.1 Bring-up

```sh
cd /home/viktoria/src/taskboard-WT-docs-deck-roundtrip-spike
eval "$(scripts/nextcloud_it_setup.sh up)"   # exports TASKBOARD_IT_DOCKER_URL/USER/TOKEN
```

Tier identity recorded (evidence): `status.php` →
`version 35.0.1.1` / `versionstring "35.0.1"`;
`GET /ocs/v2.php/cloud/capabilities` (Basic auth,
`OCS-APIRequest: true`, `Accept: application/json`) →
`capabilities.deck = { "version": "1.19.0", "canCreateBoards": true,
"apiVersions": ["1.0", "1.1"] }`; `occ app:list` → `deck: 1.19.0`
enabled. Matches the pinned versions in `docs/testing_strategy.org` §8.

### 2.2 Fixtures

Created via curl POSTs on the paths the production client already
speaks (all commands below use
`curl -u "$USER:$TOKEN" -H 'OCS-APIRequest: true' -H 'Accept: application/json'`
against `$TASKBOARD_IT_DOCKER_URL/index.php/apps/deck/api/v1.0`, elided
as `<AUTH>`): `POST /boards` → board **2** ("spike-board"),
`POST /boards/2/stacks` `{"title":"spike-stack","order":1}` → stack
**5**, `POST /boards/2/stacks/5/cards` ×2 → cards **6**, **7**.
(Deck's own "Welcome" board is id 1.)

### 2.3 Probe battery

- **V1 availability**: capabilities payload grep; in-container
  inspection of `/var/www/html/custom_apps/deck/` (routes.php, lib
  tree, migrations, controller list); `occ app:list` grep; candidate
  route GETs; card-payload key survey. Control probes to fix the
  no-route response shape.
- **V2 attachment vehicle**: multipart POST/PUT/DELETE/restore round
  trips; sha256 content verification; conditional-GET header probes;
  card-`lastModified` reads bracketing every attachment mutation (the
  mandatory D-8 coupling measurement); attachment-metadata version
  signals; a payload size ladder (1 KB → 64 MB); PHP limit reads.
- **X1/X2**: description round-trip (production client behavior +
  payload keys), comment POST/list/counter probes.
- **Desk checks** (cited, no experiment): Deck CHANGELOG.md
  in-container (1.16→1.19 window), upstream GitHub issues
  nextcloud/deck#1833 and #3472.

Notes on fidelity: the app password never appears in any captured
artifact; timestamps are reported as epoch + UTC; sporadic write-500
retry never became necessary (one deliberate 500 was captured as
evidence, see §3.2/§3.5).

## 3. The evidenced matrix

### 3.1 V1 — Deck custom fields: **no API surface exists on Deck 1.19.0**

Established positively, level by level (plan §3.1 / decision D-5):

| Level | Command / source | Observed outcome |
|---|---|---|
| Capabilities | `GET /ocs/v2.php/cloud/capabilities`, grep for `customfield`/`field` | zero hits; the entire `deck` capabilities section is `{"version":"1.19.0","canCreateBoards":true,"apiVersions":["1.0","1.1"]}` — nothing custom-fields-shaped anywhere in the payload |
| Route table (definitive) | `cat /var/www/html/custom_apps/deck/appinfo/routes.php` in-container (187 lines, full dump inspected) | 0 of ~120 registered routes match `customfield`/`custom_field`/`field`; the route families are boards/stacks/cards/labels/attachments/comments/config/session/search |
| Source tree | `grep -ril customfield …/deck/lib …/deck/appinfo`; `ls …/deck/lib/Controller/` | no hits; 22 controllers, none custom-field-shaped; `grep -ril custom …/deck/lib/Migration/` → no `custom*` tables in any of the 18 migrations |
| occ apps | `occ app:list` (enabled + disabled), grep `custom|field` | no matches in either list |
| Candidate routes | `GET api/v1.0/boards/2/customfields`, `…/cards/6/customfields`, `…/boards/2/fields`, `…/cards/6/fields`, `…/customfields`, OCS variant | all **405**; control: `GET …/total-garbage-path-xyz` → 405 (the catch-all CORS route's no-route shape), `GET …/boards/2` → 200. A 405 on this tier *is* "route does not exist" |
| Card payload survey | `GET api/v1.0/boards/2/stacks/5/cards/6`, key list | 26 keys — `ETag, archived, assignedUsers, attachmentCount, attachments, color, commentsCount, commentsUnread, createdAt, deletedAt, dependentCards, description, done, duedate, id, labels, lastEditor, lastModified, order, overdue, owner, referenceData, stackId, startdate, title, type` — no field-like key. Notably **`startdate` exists natively** (free native slot, see §6) |

**Upstream desk check** (version-transfer, plan §3.1.5): the canonical
feature request
[nextcloud/deck#1833](https://github.com/nextcloud/deck/issues/1833)
("user-defined fields", incl. a sketched `customAttributes` API) is
**open, unassigned, no milestone, no linked PR**, stalled for lack of a
developer since 2020; #3472 was closed as its duplicate (Dec 2021). The
in-container `CHANGELOG.md` (the pinned 1.19.0's own history) contains
no custom-fields entry in the 1.16 → 1.19 window. Since features are
additive and 1.19.0 postdates the live tier's 1.17.5, a surface present
on 1.17.5 would have to appear in 1.19.0's code — it does not. **V1 is
unavailable on both tiers and not on the upstream roadmap.** Absence is
the finding, not a failure (dispatch §2).

### 3.2 V2 — the attachment surface: full CRUD, with two sharp edges

The route table (§3.1, same evidence) registers the complete
attachment API in three shapes: API v1.0, API v1.1 (adds `{type}`
path segments), and OCS — `getAll` (GET `…/cards/{c}/attachments`),
`create` (POST `…/attachments`), `display` (GET
`…/attachments/{id}`), `update` (PUT/POST `…/attachments/{id}`),
`delete` (DELETE), `restore` (PUT `…/attachments/{id}/restore`).
Our client's `attachments()` (`client.rs:493`) covers `getAll` only.

**Upload** — `POST api/v1.0/boards/2/stacks/5/cards/6/attachments`:

```text
-F 'type=deck_file' -F 'file=@/tmp/sidecar_v1.json;type=application/json'
→ 200 {"id":1,"cardId":6,"type":"deck_file","data":"sidecar_v1.json",
       "lastModified":1791989503,"createdAt":1791989503,
       "createdBy":"taskboard-it","deletedAt":0,
       "extendedData":{"filesize":141,"mimetype":"application/json",
                       "info":{…},"attachmentCreator":{…}}}
```

Multipart field names are **`type` + `file`** — established from the
Deck frontend bundle (`grep -o '\.append(…)' deck-main.js` →
`append("type","deck_file")`, `append("file",e)`) after
`-F 'data=@…'` returned `500 "No file uploaded or file size exceeds
maximum of 512 MB"`. The controller signature
(`AttachmentApiController::create(int $cardId, string $type, ?string
$data)`) and `AttachmentService::create` show `data` is a plain string
slot, not the file. Three wire facts with consequences for Wave 1:

- **The stored filename comes from the uploaded part's client
  filename**, not the `data` field: an update POST with
  `data=sidecar_v1.json` but `file=@sidecar_v2.json` failed 500 with
  server log `OCP\Files\NotFoundException:
  /appdata_ocf2ydjos9cf/deck/file-card-6/sidecar_v2.json` — the lookup
  used the client filename. (Storage layout observed:
  `deck/file-card-{cardId}/{filename}`.)
- **Create refuses duplicates per (card, filename)**: re-creating
  `sidecar_v1.json` on card 6 → `409` with the existing attachment
  echoed in the error body. One document name = one attachment per
  card, deterministically.
- **Wire-shape drift vs our model**: the tier sends `extendedData.filesize`
  (our `model.rs` `ExtendedData` maps `size`, no alias) — metadata
  deserialization currently yields the default, not the size. Trivial
  fix, but it belongs in the Wave-1 client work.

**List + card inline** — `GET …/cards/6/attachments` returns the array
matching our `Attachment` model (`id, cardId, type, data, lastModified,
createdAt, createdBy, deletedAt, extendedData`). The card detail read
inlines the same array plus `attachmentCount` (moved 0 → 1 on upload).
**Measured nuance that reshapes the bandwidth model**: the *stacks
listing* (`GET …/boards/2/stacks`, the poll surface) carries
`attachmentCount` but an **empty `attachments` array** — full per-
attachment metadata (`lastModified`, `filesize`) is visible only on the
per-card detail read.

**Content download** — `GET api/v1.0/boards/2/stacks/5/cards/6/attachments/1`:
byte-identical round-trip, sha256
`1ae5dd35…cb04870` uploaded = downloaded (and again after the update,
`060d05ab…d0ef8a8ea`). Response headers carry **no `ETag`, no
`Last-Modified`**, `Cache-Control: no-cache, no-store, must-revalidate`,
`Content-Type: text/plain;charset=UTF-8` (stored mimetype not honored),
`Content-Disposition: attachment; filename="sidecar_v1.json"`.

**Conditional download: not available.** `If-None-Match: "anything-etag"`
→ 200 + full body; `If-Modified-Since: <future date>` → 200 + full
body. There is no 304 path; change detection must be done by the client
from attachment metadata (`lastModified` + `filesize`), which for the
anchor-card design costs one detail read per cycle (§3.4).

**Mutation surface** — in-place content update exists, via a quirk:

- The API-level `PUT …/attachments/1` with multipart returns `400`
  (empty body) — PHP does not parse multipart PUT bodies; the frontend
  never uses this route for file updates.
- The working update is the app-level **POST**
  `/index.php/apps/deck/cards/6/attachment/deck_file:1?boardId=2` with
  `-F type=deck_file -F data=<name> -F file=@<name>` (the routes.php
  comment "also allow to use POST for updates so we can properly access
  files" applies to this app-level route; the frontend's
  `updateAttachment` posts exactly there — bundle excerpt verified).
  `data` is validated non-empty (`400 "data must be provided and must
  be not empty"` without it).
- With a **stable client filename** the update is a true in-place
  replacement: `id` stays 1, `createdAt` stays 1791989503,
  `lastModified` advances to the write instant, `filesize` updates.
  With a changed filename it 500s (see above) — **there is no rename**.
- **Delete is soft**: `DELETE …/attachments/1` → 200 with
  `deletedAt:1791989914`; the entry stays in the list (stamped); the
  card's `attachmentCount` drops to 0 while its inline array keeps the
  tombstoned entry — exactly our `Attachment.deleted_at` modeling.
- **Restore**: `PUT …/attachments/1/restore` → 200, `deletedAt` back
  to 0, content `lastModified` preserved (1791989883).

### 3.3 D-8 (mandatory): attachment writes advance the card's entity stamp

Card `lastModified` read immediately before/after each mutation
(measured; identical stamps to the second):

| Mutation | card `lastModified` before → after | UTC |
|---|---|---|
| upload (create) | 1791989198 → 1791989503 | 14:46:38 → 14:51:43 |
| in-place update | 1791989503 → 1791989883 | 14:51:43 → 14:58:03 |
| soft delete | 1791989883 → 1791989914 | 14:58:03 → 14:58:34 |

Every sidecar write masquerades as a remote card edit: the new card
stamp **equals** the attachment's stamp. Consequences (worked through
against `merge.rs` in §5): the sidecar document must merge on its own
embedded per-field clocks, never on the card stamp; and each flush's
sidecar upload should follow the cycle's card pushes so the bump never
re-baselines data mid-flight. The bump costs no field values (the
attachment write changes no native fields), but it defeats the
unchanged-remote fast path for the anchor card on every device — one
card, not one hundred (this is the per-card-vs-board-anchor criterion
in §4).

### 3.4 Payload limits and bandwidth arithmetic (Q4/Q7, estimate per D-9)

**Limits**: the ladder passed at every rung — 1 KB, 64 KB, 512 KB,
1 MB, 4 MB, 16 MB, 64 MB all `200` (uploads to card 7, ids 2–8).
Ceiling is the image's PHP config, read in-container:
`upload_max_filesize = post_max_size = memory_limit = 512M` — the
server's own error message states the same 512 MB max. Orders of
magnitude above any sidecar need; a non-issue.

**Bandwidth model, 100-card board** (sizes measured from probe
payloads; labeled estimate, not a deployment measurement):

- Sidecar entry (one card's app-only fields, 1–2 fields): 141–198 B
  measured as whole documents; budget ~250 B/entry with overhead.
- **Board-anchored document (recommended)**: one attachment on the
  anchor card. Per cycle: 1 card-detail read of the anchor (~1.5 KB,
  needed because the stacks listing omits attachment metadata — §3.2);
  content GET **only when** `lastModified`/`filesize` differ from our
  last ingest (no HTTP validators exist — §3.2); content PUT only on
  flushes where app-only fields changed. Steady state adds ~1.5 KB per
  poll against a stacks poll that already costs ~50–100 KB for 100
  cards (~1–3%); an active cycle costs one ~15–25 KB GET + PUT (100
  entries × ~200 B).
- **Per-card documents (rejected)**: 100 detail reads per cycle to see
  metadata at all (the listing doesn't carry it), N content GETs
  skipped by the same metadata trick; every card's payload grows by one
  inline attachment entry in detail reads; Deck's web UI shows an
  attachment on every card.

### 3.5 X1/X2/X3 — the rejected rows, with their evidence

- **X3 labels-as-fields** — rejected by citation, no new experiment
  (plan §4.4): no typed semantics, no range checks, whole-set LWW
  (R7), no per-field clocks (wave analysis §4.1). Labels remain the
  free-form taxonomy vehicle.
- **X1 description-embedded markers** — rejected: (a) `description` is
  a native card field (key list, §3.1), user-authored markdown lives
  there and the org-mode ambitions (wave analysis §3.6) want it for
  real content; (b) every Deck write is full-send (`outbox.rs` module
  doc: "every Deck write is full-send anyway"), so each sidecar write
  would rewrite the description — the granularity trap applies to
  *every* app-field write, the worst coupling measured in this spike
  amplified by real field mutation; (c) markers are invisible state in
  a user-editable string — corruption is silent and unrecoverable.
- **X2 card comments** — routes and behavior probed for free:
  `POST /ocs/v2.php/apps/deck/api/v1.0/cards/6/comments`
  `{"message":"spike-comment-one"}` → 200 with
  `{id, objectId, message, actorId, actorType, actorDisplayName,
  creationDateTime, mentions}`; `GET …/comments?limit=20` works (the
  API's only paginated surface); the card carries
  `commentsCount: 1, commentsUnread: 0`. Rejected: append-only
  discussion semantics (edits/deletes are user-visible history, not a
  store), paginated reads, unbounded growth per sync, and everything
  written is displayed to the user in Deck's own UI.

## 4. Recommendation (exactly one)

**Carry app-only fields in a board-anchored attachment sidecar: one
JSON document ("the app-data document") stored as a single Deck file
attachment on a designated anchor card per board; merge locally per
key with document-embedded per-field write clocks and explicit
tombstones; treat the document as transport, local state as truth.**

Granularity choice (D-7 criteria, both shapes sketched in plan §4.1):

| Criterion (fixed in plan D-7) | Board document | Per-card documents |
|---|---|---|
| Anchor fragility | Mitigated by design: the document is *transport* — every device's local state already holds all app-only fields (they enter local state before their card id is bound, R9a evidence-then-verdict). Anchor loss (user delete/move/archive) is detected via the missing/tombstoned attachment and recovered by re-anchoring + one rebuild flush from local state. Deck soft-deletes keep the metadata visible for detection (§3.2). | Self-anchoring (dies with its card — natural tombstone), but a card delete now silently removes that card's app-fields from all *other* devices' documents — desirable for deletes, but it couples app-field presence to card presence in both directions. |
| Kiosk bandwidth | 1 detail read + ≤1 content GET/PUT per active cycle; steady state ~1.5 KB/poll (§3.4). | 100 detail reads/cycle (listing omits metadata, §3.2) + N conditional content GETs. |
| LWW coupling (D-8, measured) | 1 card stamp bump per flush. | N stamp bumps; a multi-card edit session bumps every touched card. |
| Deck-UX pollution | 1 attachment visible on 1 card. | An attachment visible on every card. |

The board document wins on the three measurable criteria; its one
weakness is neutralized by the transport-not-truth rule (and the same
rule is what makes the file-level last-writer-wins on the container
safe: concurrent writers merge per key on read and re-upload the union,
so a lost document update converges on the next flush — §5).

**Consequences for later waves**

- **Wave 1 (field family)**: every field named in §6 syncs day one —
  no local-only-first field, no data migration anywhere. The client
  work is the "[2026-10-08] Deck attachment writes and content
  download" backlog entry (multipart upload + raw-bytes download)
  extended by: the app-level POST update route with stable filename,
  metadata-based change detection, and the `filesize` wire-field fix
  (§3.2).
- **D5 substrate (Wave 4)**: the document generalizes directly —
  reminder delivery proofs, settings, and journal-style app data become
  additional top-level sections (or sibling documents on the same
  anchor card; distinct filenames are enforced by the 409 on duplicate
  names, §3.2). The set-union-with-tombstones merge is exactly the
  delivery-proof model ("designed set-union-mergeable", wave analysis
  §1). The same anchor + read-modify-write machinery serves all of it.
- **Native `startdate`**: Deck cards carry a native `startdate`
  (§3.1 key list) — a free, LWW-merging slot the field-family plan
  should evaluate for scheduled-start *before* defaulting it into the
  sidecar. The sidecar remains the default vehicle for everything else.

## 5. Merge-policy implications (stated against the actual code)

Checked line-by-line against `crates/taskboard-domain/src/{merge.rs,
pipeline.rs, outbox.rs}` and `crates/taskboard-sync-nextcloud/src/
actor.rs`; claims below match the code as of this branch.

- **Identity**: document keys are `(board, remote card id)` — the same
  cross-device identity the pipeline already matches on
  (`task_by_board_card: HashMap<(RemoteBoardId, RemoteCardId), TaskId>`,
  `pipeline.rs:394`; the poll's `LocalCardStamps` key the same pair,
  `actor.rs:388`). Local `TaskId` UUIDs never appear in the document
  (plan D-6; they are per-device).
- **Two disjoint clock domains**: native fields merge per-field LWW on
  Deck's entity stamp (`merge_task`, `merge.rs:170` — each field's
  verdict is `remote_wins(remote.last_modified, local.clocks.X)`);
  app-only fields must merge on the clocks embedded per key in the
  document, **never** on card `last_modified` (measured in §3.3: every
  sidecar write advances it, so it carries no per-field information
  for this family). A new pure merge primitive (a `merge_task` sibling)
  folds document entries into the merged tasks after the native merge;
  `merge.rs`'s per-field-clock discipline (`TaskClocks`,
  `entities.rs:49`) is the shape to mirror.
- **No presence inference from the document**: `merge.rs`'s R3
  delete-wins fires on *remote listing absence*; the document must
  never feed R3 — a key absent from a (possibly stale or
  last-writer-truncated) document is not a deletion. Deletion is an
  explicit tombstone entry, winning only by its own timestamp. This is
  also what makes concurrent whole-document replacement safe: writer A
  re-reading writer B's document that lacks A's keys keeps its local
  keys (absence ≠ tombstone) and re-uploads the union on the next
  flush — the flush rule is read-modify-write of the union, never a
  blind overwrite.
- **Riding `apply_sync_report`** (`pipeline.rs:345`): the connector
  downloads the document while building the snapshot (new evidence
  alongside `RemoteBoardSnapshot`; the anchor's metadata for the
  change-detection decision comes from one card-detail read, §3.4) —
  and the engine folds it in its fixed order (push outcomes first R9,
  then board → stacks → labels → tasks, then presence reconciliation,
  `pipeline.rs:331`). The fold slots with the task merge, *after*
  `merge_task` and *after* `keep_task_intent` has re-applied protected
  fields, so the self-clobber guard's shape is preserved.
- **Self-clobber mirror** (`PendingTaskIntent`, `pipeline.rs:92`,
  protection keyed on `op.queued_at > read_at`): a local app-only edit
  queued after `read_at` must survive the same cycle's document
  adoption. Because the document carries per-key clocks, the natural
  rule is the same one the native family uses — the intent-keep step
  copies the app-field values *and their document-embedded clocks*
  back over the fold result (the requirement; the op-to-field mapping
  is Wave 1's design).
- **Push side / outbox shape** (`outbox.rs`): `LocalOp` is id-scoped
  and field-less ("`TaskClocks` already knows which fields changed,
  and every Deck write is full-send anyway"); a sidecar push op
  follows that shape (e.g. `UpdateAppData(TaskId)`), coalesced by the
  connector into the document at flush time.
- **Flush order** (`actor.rs:291–426`, verified): `read_at` is stamped
  before the read, push runs *before* pull ("the R3 hazard, ADR 0007"),
  then the `SyncReport::Completed` carries snapshot + pushes +
  `read_at`. The sidecar upload appends **after** the card pushes of
  the same flush and before the cycle ends — its §3.3 stamp bump then
  lands in the *next* cycle's pull, where the document merge (clock
  domain disjoint) and the native echo path (R9a) both handle it
  without value loss; nothing a device does between flushes can be
  reordered ahead of it because ops queued after `read_at` are
  protected by the guard above.
- **Failure semantics** (`apply_push_report`, `pipeline.rs:788`): the
  push-only composition exists so that a *failed* cycle can never
  tombstone from an empty snapshot ("evidence-then-verdict", ADR 0007)
  — the sidecar inherits the same fence: a failed document upload
  keeps its ops queued (retryable, `PersistenceAction::FailOp`
  bookkeeping) and a failed/partial document download folds nothing
  (no tombstones from a document we could not read). Reminder data
  stays outside the native path as designed ("v1: no reminder data
  enters the push path", `pipeline.rs:862`) — the document gives it a
  first sync lane without touching that fence.
- **Echo adoption** (R9a, `merge.rs:284`: values taken as-is, clocks
  `max(local, echo.last_modified)`): after our own document upload, the
  next cycle reads back our own union — adopt unconditionally with
  clocks `max(local, document-embedded)`; idempotent by construction.

## 6. Per-field Wave-1 impact

Complete for the field names in wave analysis §3.1/§3.2 (plan D-10).
All fields below sync day one via the sidecar unless marked otherwise;
none needs a data migration (local-only period ≡ zero, because the
fields land together with the vehicle).

| Field (wave §) | Verdict | Notes |
|---|---|---|
| Scheduled-start (§3.1) | Syncs day one | Sidecar entry (`DateTime<Utc>` + clock). The field-family plan should first evaluate Deck's native `startdate` (§3.1) — a free native slot with existing per-field LWW; if it carries scheduled-start, the sidecar entry is unnecessary. Either way: day one, no migration. |
| Effort (§3.1) | Syncs day one | Scalar duration entry; trivial document key. |
| Importance (§3.1) | Syncs day one | Enum/score entry; range validation is local (the document is opaque to Deck). |
| Urgency (§3.1) | Syncs day one | As importance; decision-support math stays pure-domain. |
| Task-kind flag / tracker type (§3.1/§3.3) | Syncs day one | Boolean/kind entry. |
| Respite flag (§3.1) | Syncs day one | Boolean entry; consumers (break reminders, review stats) come in Wave 2. |
| Recurrence parameters (§3.1/§3.7) | Syncs day one | JSON rule object per key; size trivial (§3.4). The recurrence *semantics* remain Wave 3's design round (D3) — the vehicle is ready when they are. |
| Reminder rules (§3.2) | Syncs day one | N rules per task as document keys; gives reminders their first sync lane while `pipeline.rs` keeps them out of the native path (§5). The v1 fence stays intact. |
| Reminder delivery proofs (§3.2, → D5) | Local-only-first **by roadmap choice**, migration story = none needed | Technically rides the same document day one (set-union-with-tombstones is their native model), but wave analysis gates reminder-state sync on the Wave-4 D5 substrate. Migration is purely additive: append a `proofs` section when Wave 4 opens; no data moves. |
| Habits / repeatable tasks (§3.1) | Syncs day one via the recurrence entry | Carried by the recurrence-parameters row above (a habit is a recurring task; wave analysis routes repeatable tasks through D3). The vehicle is ready whenever D3's design round defines the rule shape. |
| Subtasks (§3.1) | Out of this field family — own design round | Wave analysis §3.1 parks subtasks on their own design round ("local-only or custom-field carried", backlog entry appended); this spike does not re-verdict it. If that round chooses to sync them, this document is the natural vehicle (per-card keys, tombstones); parent/child semantics stay that round's problem. |

## 7. Risks & open questions

- **Version skew, tier 1.19.0 vs live 1.17.5**: the availability
  verdict transfers by monotonicity (§3.1); the *behavioral* verdicts
  (upload field names, update quirk, 409, soft-delete) should be
  re-probed against the live tier before the Wave-1 client surface
  ships — the 1.19.0 changelog itself fixes "broken upload of
  attachment via api" (#8156, #8219), evidence this surface moved
  recently. Named follow-up (never run inside this chunk — no
  live-tier experiments, plan §6).
- **The update route is the fragile edge**: API-level PUT multipart is
  broken by PHP semantics; the working path is the app-level POST
  route the frontend itself uses. It is not in Deck's documented API
  spec — contract-test it on both tiers (the established D-series
  practice) and prefer create-409 → update over any filename-changing
  flow (renames 500, §3.2).
- **Anchor card lifecycle**: a user may archive, move, or delete the
  anchor card in Deck's own UI. Detection rides the anchor detail
  read (tombstoned/missing attachment, archived card leaves the stacks
  listing); recovery is unarchive or re-anchor + rebuild flush from
  local truth (§4). Wave 1 must design the anchor ceremony (creation,
  title convention, permission) — stated requirement, not hand-waved.
- **Concurrent document writers**: whole-document last-writer-wins at
  the file level is safe only under the read-modify-write union rule
  and explicit tombstones (§5); the residual loss window (two devices
  writing between each other's reads) converges on the next flush
  because absence is not deletion. Single-writer kiosk fleets rarely
  hit it; shared boards must not rely on last-writer-wins semantics
  for app fields.
- **No HTTP validators on content** (measured): change detection rides
  `lastModified`/`filesize` metadata; a same-size-same-second rewrite
  is theoretically invisible. Negligible for one document per board;
  noted for honesty.
- **Wire drift**: `extendedData.filesize` vs our `size` mapping (§3.2)
  — fix in the Wave-1 client work, add contract fixtures from this
  spike's payloads.
- **Open question — document format ownership**: the document needs a
  format-version field and a section registry from day one so the D5
  generalization (§4) does not become a breaking format change. Wave 1
  owns the schema; this report fixes only the transport and merge
  semantics.

## 8. Bookkeeping

The backlog entry "[2026-10-14] Deck round-trip strategy for app-only
fields" (`.artifacts/taskboard-potential-improvements-backlog.md`) is
annotated with this report's path; the entry stays **open** — Wave 1's
field-family plan implements the recommendation and closes it.
