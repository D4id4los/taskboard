# [Plan] Deck Round-Trip Strategy Spike — App-Only Fields (Decision D1)

- **Date**: 2026-10-14
- **Kind**: stage-1 investigation plan (chunk 1, Prompt 1.1 of the wave-0
  dispatch plan) — no code, no experiments in this commit. Stage 2
  (Prompt 1.2) executes §§3–4 and writes the report per §5; stage 3
  (Prompt 1.3) reviews and ships.
- **Origin (normative)**:
  - `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
    §4.1 (decision D1 — the keystone; the question this spike answers is
    normative) and §6 Wave 0 row (why it runs now, before the Wave-1
    field family);
  - `.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
    §1 (shared execution model + evidence & artefact mandate — binding)
    and §2 (chunk 1 covers / deliverable / acceptance);
  - backlog entries "[2026-10-14] Deck round-trip strategy for app-only
    fields" (the item; annotated, not closed, by this chunk) and
    "[2026-10-08] Deck attachment writes and content download" (the known
    client gap the spike probes ad hoc).
- **Branch**: `docs/deck-roundtrip-spike` off `main` (@ `3682da7`),
  worktree `../taskboard-WT-docs-deck-roundtrip-spike`.
- **Status**: READY for stage 2 (execution).

## 1. Verified context (stage-1 findings — do not re-derive)

### 1.1 The question, precisely

Wave-analysis §4.1 recommends dedicated typed fields for anything with
semantics and labels only for free-form taxonomy, bridged by a Deck
round-trip vehicle. This spike answers: **which vehicle lets fields Deck
cannot represent — scheduled-start, effort, importance/urgency,
task-kind/respite flags, recurrence parameters, reminder rules —
round-trip through a Deck sync connection, at what cost, and with what
merge semantics?** The answer gates Wave 1's field family: every field
lands either "syncs day one" or "local-only-first with a recorded
migration story" (report §5 verdict table).

### 1.2 Client-surface facts (`crates/taskboard-sync-nextcloud`)

- Deck API base is `{url}/index.php/apps/deck/api/v1.0`
  (`client.rs` `DECK_API_PATH`); every request carries Basic auth
  (`user` + app password), `OCS-APIRequest: true`, and
  `Accept: application/json` (bare payloads are tolerated alongside the
  OCS envelope — Nextcloud 35 behavior, already decoded by us).
- Attachment coverage is **metadata listing only**:
  `attachments(board, stack, card)` →
  `GET …/cards/{c}/attachments` returning `Attachment { id, card_id,
  kind ("deck_file" | …), created_at, created_by, deleted_at,
  data/extended_data: { file_name, size, mimetype, mtime } }`
  (`client.rs:493`, `model.rs:314–349`). No upload, no content
  download, no rename/delete — that gap *is* the open
  "[2026-10-08] Deck attachment writes and content download" backlog
  entry. The spike probes those endpoints **ad hoc** (curl); it never
  extends the production client.
- Cards already inline `attachments: Vec<Attachment>` and
  `attachment_count` in every read (`model.rs:138–141`) — sidecar
  *metadata* may ride the existing pull for free; *content* fetch is
  the new cost.
- Writes are full-send: Deck fills server-side defaults for omitted PUT
  fields (`BoardChanges` / `update_card` doc comments); `outbox.rs`
  states it as a rule ("every Deck write is full-send anyway"). Any
  vehicle that rides card PUTs inherits this.

### 1.3 Merge vocabulary the recommendation must respect
(`crates/taskboard-domain`)

The dispatch acceptance requires report claims to match this code, not
paraphrase it:

- **Per-field LWW** (`merge.rs`, ADR 0004 R1–R9): a remote field wins
  iff `remote.last_modified > field_clock` *strictly*; ties keep local.
  Deck's stamp is **entity-level** (whole card) — the documented
  granularity trap, pinned intentional (backlog "[2026-10-09]
  Field-level merge refinement" tracks mitigation).
- **Pipeline composition** (`pipeline.rs` `apply_sync_report`): push
  outcomes first (R9), then board → stacks → labels → tasks, then
  presence reconciliation (R3) and cascades, then sync-status +
  persistence batch. `apply_push_report` is the push-only composition
  for `Failed` reports that carried successful pushes
  (evidence-then-verdict, ADR 0007): completed pushes land, but no
  merge or tombstoning runs — an empty/failed snapshot can never
  delete anything.
- **Self-clobber guard**: fields with an outbox op queued after the
  cycle's `read_at` are protected from pull adoption
  (`PendingTaskIntent` et al.) — a sidecar merge for app-only fields
  needs an equivalent protection (§4.3).
- **Outbox ops are id-scoped and field-less** (`outbox.rs` `LocalOp`):
  which fields changed lives in `TaskClocks`. Any future sidecar push
  op follows that shape; its concrete design belongs to Wave 1, not
  this spike (§4 sketches only).
- **Reminder data is deliberately outside sync today**: both pipeline
  entries carry `AppState.reminders` through untouched ("v1: no
  reminder data enters the sync path"). Reminder rules and delivery
  proofs are therefore app-only data in exactly the D1 sense; delivery
  proofs were designed set-union-mergeable (wave analysis §1) — the
  sidecar sketch reuses that property.
- **Task identity across devices is `(board, remote card id)`** (the
  pipeline's card→local matching), never the local `TaskId` UUID —
  sidecar document keys must use the same identity (§4.2).

### 1.4 Tier-2 harness facts (`docs/testing_strategy.org` §8)

- Compose: `docker/nextcloud-test/compose.yaml` → `nextcloud:35-apache`
  on `127.0.0.1:8370`, SQLite, throwaway credentials by design.
- Setup: `eval "$(scripts/nextcloud_it_setup.sh up)"` brings the
  instance up, installs Deck, creates user `taskboard-it`, and mints a
  **full-capability app password** (the script feeds the account
  password on stdin precisely so the token is unrestricted), exporting
  `TASKBOARD_IT_DOCKER_URL/USER/TOKEN`.
- occ access: `docker compose -f docker/nextcloud-test/compose.yaml
  exec -T -u www-data nextcloud php occ …`.
- Pinned versions for evidence: tier 2 = Nextcloud 35 / Deck **1.19.0**;
  the user's live tier (not used here) = Nextcloud 33.0.9 / Deck
  **1.17.5** — availability evidence is pinned to 1.19.0 per the
  dispatch acceptance; the 1.17.5 transfer question is a desk check
  (§3.1.5), not a second docker tier.
- Known tier behaviors to design experiments around (§8 known-server
  matrix): listings lag Deck's cache (confirm writes via detail
  reads); empty stacks send `cards: null`; the boards listing's
  `labels` array is unreliable (board detail read is authoritative);
  write endpoints answer sporadic `500`s under load (experiment
  scaffolding may retry; record occurrences).

## 2. Research questions & vehicle matrix

### 2.1 Vehicles

| ID | Vehicle | One-line shape |
|---|---|---|
| V1 | Deck custom fields | Native per-card typed fields — *if* any API surface exists on Deck 1.19.0 |
| V2 | Attachment sidecar | Structured JSON document (per board or per card) stored as a Deck file attachment, merged set-union-style locally |
| X1 | Description-embedded markers | App-only data hidden in the card's markdown description (HTML comment / fenced block) |
| X2 | Card comments | App-only data written as OCS card comments |
| X3 | Labels-as-fields | Semantics encoded in label sets |

X1–X3 are the **rejected alternatives** the dispatch names (plus X3,
which wave-analysis §4.1 already settled): the report records the
rejection reason — plus whatever evidence the route/payload survey
contributes for free — so later agents do not re-litigate.

### 2.2 The matrix — one evidenced row per question

Rows are the dispatch's acceptance questions; every cell carries tier-2
evidence (command + observed outcome) or a verifiable cited source.
Cells marked §3.x are filled by that experiment.

| # | Question | V1 custom fields | V2 attachment sidecar |
|---|---|---|---|
| Q1 | Availability on Deck 1.19.0 | Capabilities payload; `occ app:list`; in-container Deck route enumeration; candidate-route probes (§3.1) | Attachment endpoints exist (our client already lists metadata); upload / content-download / rename / delete routes confirmed by probe (§3.2) |
| Q2 | API shape (endpoints, verbs, wire fields) | Whatever Q1 finds (routes + payload shapes) or "no surface" | Exact multipart field names, response shape, id semantics, download URL field, mutation primitives (§3.2.1–4) |
| Q3 | Auth scope / permission gate | Per Q1 (route middleware from source if a surface exists) | Which permission gates attachment writes (board edit ACL vs read-only share) — Deck controller source in-container; app-password capability note (§3.2.8) |
| Q4 | Payload limits | Field-size limits from source, if a surface exists | Empirical size ladder (~1 KB → 4 MB) against the observed ceiling; PHP limits read in-container to explain it (§3.2.7) |
| Q5 | Per-field granularity | Per custom-field value, if it exists | Per-key inside the document (set-union); transport is whole-document replacement (§4.2) |
| Q6 | Conflict semantics | Which stamp a custom-field write advances (card `lastModified`? own?) | **Does an attachment write advance card `lastModified`?** (decisive for LWW interplay); attachment `mtime` as version signal; file-level last-writer-wins on concurrent re-upload (§3.2.5–6) |
| Q7 | Kiosk bandwidth per sync cycle | Incremental with card payloads if custom fields ride card JSON | Metadata already rides card reads; content: per-card docs = N downloads vs one board doc = 1 download of size S — arithmetic from captured payload sizes (§3.4) |

### 2.3 Verdict shape

The report delivers **exactly one recommendation** (dispatch
acceptance): V1, or V2 (with the §4.1 granularity choice made), or "no
vehicle — local-only-first for the field family, with the recorded
migration story" — each with consequences for later waves (the D5
app-data substrate generalization included).

## 3. Experiment protocol (docker tier; stage 2 executes)

All probes run from the worktree or `/tmp` with curl (+ jq for
readability); multipart via `curl -F`. Scratch scripts and captured
payloads **never land in the repo** — evidence lives as redacted
excerpts inside the report (evidence mandate). The app password never
appears in any captured artifact; requests are recorded with the
Authorization header elided.

### 3.0 Bring-up (the report's method section must reproduce from here)

```sh
cd <worktree>
eval "$(scripts/nextcloud_it_setup.sh up)"   # compose up + Deck install + user + app password

# Record the tier identity once, as evidence:
curl -s "$TASKBOARD_IT_DOCKER_URL/status.php" | jq .            # server version
curl -s -u "$TASKBOARD_IT_DOCKER_USER:$TASKBOARD_IT_DOCKER_TOKEN" \
     -H 'OCS-APIRequest: true' -H 'Accept: application/json' \
     "$TASKBOARD_IT_DOCKER_URL/ocs/v2.php/cloud/capabilities" | jq .   # incl. deck section
docker compose -f docker/nextcloud-test/compose.yaml exec -T -u www-data \
     nextcloud php occ app:list
```

Every Deck call reuses the `-u …` + `OCS-APIRequest` + `Accept:
application/json` shape against
`$TASKBOARD_IT_DOCKER_URL/index.php/apps/deck/api/v1.0/…`. Fixtures
(one board, one stack, two cards) are created via curl POSTs using the
paths our client already speaks; ids go into shell variables.

### 3.1 V1 — custom-fields availability (establish *positively*; absence is a legitimate, well-evidenced finding)

1. **Capabilities**: inspect the capabilities payload's `deck` section
   (record `deck.version`) and anything custom-fields-shaped anywhere
   in the payload.
2. **Route enumeration (definitive)**: inside the container, enumerate
   the Deck app's registered routes — `custom_apps/deck/appinfo/`
   route file(s) and, if registration is programmatic, the
   `lib/Controller` method annotations — and grep for
   `customfield` / `custom_field` / `field`. The registered-route
   table is the authoritative API surface; record the route count and
   any hits.
3. **occ app survey**: `occ app:list` (enabled + disabled); if the
   Nextcloud appstore advertises a custom-fields-shaped app compatible
   with Nextcloud 35, cite it (web check). A separate-app or UI-only
   surface is a finding, not a vehicle, unless it exposes an API the
   spike can evidence.
4. **Route probes**: GET a small battery of plausible routes on the
   fixture board/card (e.g. `boards/{b}/customfields`,
   `…/cards/{c}/customfields`) — expected 404s, recorded as statuses.
   Also capture one full card detail payload and grep it for
   field-like keys (the server returns every field it has).
5. **Desk check (version transfer)**: Deck release notes / source tags
   between 1.17 and 1.19 for any custom-fields feature, so the verdict
   states whether the live tier (1.17.5) differs. Cited sources; no
   experiment.

### 3.2 V2 — attachment vehicle probes

1. **Upload**: POST a small JSON file to
   `boards/{b}/stacks/{s}/cards/{c}/attachments` as
   multipart/form-data (`type=file` + a file part, per the documented
   shape; establish the exact field names the tier accepts — a 400's
   typed error shape is evidence, not a probe failure).
2. **List + card payload**: GET the attachment list; confirm the shape
   matches our `Attachment` model; confirm the card detail read
   inlines it and whether `attachment_count` moved.
3. **Content download**: find the download URL from the attachment
   payload (confirm the field name on the tier) and fetch the bytes;
   verify a byte-identical round-trip (sha256). **Probe conditional
   download** (`If-None-Match` / `If-Modified-Since` on the content
   GET): a 304 path is the kiosk-bandwidth lever for per-card docs.
4. **Mutation surface**: rename (if the route exists), delete,
   re-upload — establish whether in-place content update exists or the
   cycle is delete + re-create, and what id / `mtime` churn that
   causes. Note whether deletion is soft (a `deletedAt`-stamped entry
   still visible, as our model's `deleted_at` hints).
5. **Conflict-coupling measurement (decisive, mandatory — D-8)**: read
   the card's `lastModified` before/after upload, rename, and delete
   of an attachment. If attachment writes advance the card stamp,
   every sidecar write masquerades as a remote card edit under
   entity-level LWW — the report must state the coupling and the
   sequencing consequence (§4.3).
6. **Version signals**: attachment `mtime` / timestamp behavior across
   re-upload; whether the attachment id is stable across content
   replacement.
7. **Payload ladder (Q4)**: upload sidecar-shaped documents at ~1 KB /
   64 KB / 512 KB / 1 MB / 4 MB; record the first failure and its
   typed error; read the PHP `upload_max_filesize` / `post_max_size`
   in-container to explain the ceiling.
8. **Auth scope (Q3)**: read the Deck attachment controller source
   in-container for its permission middleware (evidence for "board
   edit permission gates writes"); note that app passwords cannot be
   folder-scoped (testing_strategy §8 hard rule) — the scope question
   is Deck ACLs, not token capabilities. Optional stretch only if
   cheap: second user + read-only board share → upload probe expects
   403.

### 3.3 X1/X2 — free evidence for the rejected rows

- **X1 description markers**: the already-captured card payloads show
  `description` round-trips verbatim through card PUT (our client
  proves this). Rejection grounds (§4.4): collision with user-authored
  markdown, full-send PUT semantics, and worst-case conflict coupling.
- **X2 comments**: the §3.1.2 route enumeration shows the comment
  routes; note pagination (the Deck API's only paginated surface,
  per the "[2026-10-08] Deck comments" backlog entry), card
  `comments_unread` counters, and wrong semantics (append-only,
  user-visible).

### 3.4 Bandwidth arithmetic (Q7)

From captured payload sizes, model a 100-card board: (a) per-card
sidecars — metadata rides existing card reads + N content GETs (or N
conditional GETs if §3.2.3 found a 304 path) vs (b) one board-anchored
document — 1 content GET of the union doc (size = per-card entry size
× 100, measured from the probe payloads). State the judgment against
the sync actor's poll cadence; label it an estimate, not a deployment
measurement (D-9).

## 4. Sidecar merge sketch (V2's merge-policy implications)

No code, no schema, no outbox changes in this spike — this section
states how a V2 recommendation *would* ride the existing pipeline, so
the Wave-1 planner can act without re-deriving.

### 4.1 Document granularity (board-anchored vs per-card)

Deck attachments are **card-anchored** — there is no board-level
attachment. The two real shapes:

- **Board document on a designated anchor card**: one JSON document
  holds every card's app-only fields. Pros: one content download per
  sync; one upload per flush; trivial set-union. Cons: the anchor card
  is ordinary Deck state — a remote delete/move of the anchor loses or
  relocates the store (the verdict must specify re-anchor detection
  and recovery, not hand-wave it); every field edit re-uploads the
  whole document; concurrent writers replace the whole file
  (last-writer-wins at file level — safe only because the merge is
  per-key).
- **Per-card documents**: each card carries its own small sidecar.
  Pros: self-anchoring (dies with its card — a natural tombstone);
  granular bandwidth; conflict scope = one card. Cons: N content
  downloads per cycle (mitigable iff §3.2.3's conditional path
  exists); N attachments visible to Deck users.

The report selects one against the §3 evidence. Selection criteria
(fixed now, D-7): anchor fragility, bandwidth at kiosk scale,
conflict-coupling with card LWW, Deck-UX pollution.

### 4.2 Document shape (set-union keys)

Keys use the cross-device identity the pipeline already uses —
`(board, remote card id)` — never the local `TaskId` UUID
(per-device). Entries carry the field value **plus its own write
clock**, the same per-field discipline as `TaskClocks` (delivery
proofs already model set-union-mergeable data). Deletes are explicit
tombstones inside the document (per key/field `deleted_at`), so a
stale or truncated document can never delete a key it never knew —
union-with-tombstones, tombstone wins only by timestamp. Offline
task creates enter the document only after their create push binds a
card id — evidence-then-verdict, exactly like every pushed field
today.

### 4.3 Riding `apply_sync_report` without weakening per-field LWW

- **Pull**: the connector downloads the document(s) while building the
  snapshot (new *evidence*; the metadata to decide cheaply —
  attachment `mtime`/size vs the last-ingested version — already rides
  card reads). A pure merge primitive — a sibling of `merge_task`,
  keyed by the document's embedded per-field clocks, **not** by
  Deck's entity-level card `last_modified` — folds app-only fields
  into the merged tasks. Native-field LWW is untouched: the two field
  families have disjoint clocks, and the granularity trap cannot cross
  families *unless* §3.2.5 shows attachment writes advance card
  `lastModified` — in that case the recommendation must carry a
  sequencing rule (sidecar upload strictly after the cycle's card
  pushes) and quantify the residual risk.
- **Push**: app-only edits queue id-scoped outbox ops (concrete
  `LocalOp` shape is Wave 1's); the connector coalesces them into the
  document and uploads; the echo (our own content + attachment
  metadata) is adopted unconditionally, R9a-style, with clocks
  `max(local, document-embedded)`.
- **Self-clobber**: a just-queued local app-only field edit must
  survive the same cycle's sidecar adoption — the mirror of
  `PendingTaskIntent`'s post-`read_at` protection; the sketch states
  the requirement, Wave 1 designs the op-to-field mapping.
- **Failure**: a failed document upload keeps its ops queued
  (retryable) — `apply_push_report` semantics; a failed pull never
  tombstones sidecar keys (no presence inference from a document we
  failed to fetch). Key deletion happens only via an explicit
  tombstone entry, never via document shape.

### 4.4 Rejected-alternative arguments (recorded so they are not re-litigated)

- **X3 labels**: wave-analysis §4.1 — no typed semantics, no range
  checks, whole-set LWW (R7), no per-field clocks; matrix math
  impossible. Rejected for semantic fields; remains the free-form
  taxonomy vehicle. No new experiment.
- **X1 description markers**: collides with user-authored markdown;
  every write is a full-send card PUT advancing the entity-level
  `lastModified` — the worst possible conflict coupling (the
  granularity trap applies to *every* sidecar write); and the org-mode
  ambitions (wave §3.6) want the description for real content.
- **X2 comments**: append-only, paginated, user-visible; unbounded
  growth per sync; the semantics of a discussion, not a store.

## 5. Report outline (stage-2 deliverable)

`.artifacts/reports/<execution-date>-deck-roundtrip-strategy-report.md`,
house `[Report]` style:

1. **Context & question** (D1; wave-analysis and dispatch cites).
2. **Method** — reproducible from this section alone (§3.0 bring-up,
   probe battery, redaction note).
3. **The evidenced matrix** — §2.2 filled; every cell is a command +
   observed outcome, or a cited source.
4. **Exactly one recommendation** + consequences for later waves (the
   D5 substrate generalization if V2 wins; the migration story if no
   vehicle wins).
5. **Merge-policy implications** stated against the real
   `apply_sync_report` / `LocalOp` code (§4 refined by evidence;
   claims checked line-by-line against `merge.rs`, `pipeline.rs`,
   `outbox.rs`).
6. **Per-field Wave-1 verdict table**, complete for the wave-analysis
   §3.1/§3.2 field names: scheduled-start, effort, importance,
   urgency, task-kind flag, respite flag, recurrence parameters,
   reminder rules, reminder delivery proofs (→ D5 substrate note) —
   each "syncs day one" or "local-only-first + migration story".
7. **Risks & open questions** — the 1.19.0 vs 1.17.5 version skew
   first among them.
8. **Backlog annotation**: a `> Update <date>: report landed at <path>`
   note on the D1 entry — the entry stays open (Wave 1 implements).

## 6. Out of scope (hard fence)

- **No production code of any kind**: `client.rs`, `model.rs`, and the
  domain crates are read-only surfaces for this spike.
- **No client extension**: attachment write/download stays the
  "[2026-10-08]" backlog entry's future work; no `multipart` reqwest
  feature, no new dependencies.
- **No schema / storage / outbox / domain-entity changes** — Wave 1
  owns all of that, informed by this report.
- **No CI/workflow changes; no second docker tier** (no Deck 1.17.5
  matrix leg); **no live-tier (tier 3) experiments** — zero secrets.
- Scratch scripts and captured payloads stay out of the repo; the PR
  diff is exactly this plan + the report + the single backlog
  annotation (evidence mandate).
- Out-of-scope ideas discovered en route → backlog entries per
  template, not report scope.

## 7. Decisions (open questions resolved by stage 1)

| # | Question | Decision |
|---|---|---|
| D-1 | Vehicle set | V1 + V2 investigated; X1/X2 rejected-with-reasons rows; X3 rejected by citation (wave §4.1), no new experiment |
| D-2 | Experiment tier | Docker tier 2 only, versions pinned (nextcloud:35 / Deck 1.19.0); live-tier skew handled by the §3.1.5 desk check |
| D-3 | Tooling | curl + jq only (multipart via `-F`); no scratch cargo binary; scratch lives in the worktree-untracked paths or /tmp and never lands in the repo |
| D-4 | Evidence format | Redacted request/response pairs + verbatim occ / route-enumeration outputs as report excerpts; the app password is never captured in any artifact |
| D-5 | Custom-fields absence bar | Established positively via capabilities + in-container route enumeration + `occ app:list` (+ candidate-route probes); absence is recorded as the finding, never assumed |
| D-6 | Sidecar identity keys | `(board, remote card id)` — the pipeline's existing cross-device identity; local `TaskId` UUIDs never key the document |
| D-7 | Granularity choice | Both §4.1 shapes sketched; the report selects one against §3 evidence; the selection criteria are fixed now (anchor fragility, kiosk bandwidth, LWW coupling, Deck-UX pollution) |
| D-8 | Conflict-coupling probe | §3.2.5 is mandatory: card `lastModified` before/after attachment writes decides the sequencing rule any V2 recommendation must carry |
| D-9 | Bandwidth method | Arithmetic from captured payload sizes on a modeled 100-card board; labeled an estimate, not a deployment measurement |
| D-10 | Deliverable boundary | Exactly one recommendation; per-field Wave-1 verdicts complete for the wave-analysis §3.1/§3.2 field names; the D1 backlog entry is annotated, not closed |

## 8. Risks

- **Version skew** (tier 1.19.0 vs live 1.17.5): availability and
  conflict verdicts may not transfer; mitigated by the §3.1.5 desk
  check; if that check is inconclusive, the report says so and a
  live-tier confirmation becomes a named follow-up (never run inside
  this chunk).
- **Attachment writes may advance card `lastModified`**: would couple
  the sidecar into the entity-level LWW granularity trap — measured by
  the mandatory D-8 probe; a V2 recommendation must then carry the
  sequencing rule and its residual-risk statement, or lose points in
  the verdict.
- **Deck cache lag on the tier**: experiment readbacks are confirmed
  via detail reads (established D-series practice); sporadic write
  `500`s are retried with occurrences recorded.
- **Multipart field-name drift** across Deck versions: documented
  shape first, then observe; a 400 is evidence, not a failure.
- **Anchor-card fragility** (board-document shape): a remote anchor
  delete mid-sync must not lose app-only fields — the recovery policy
  is part of the granularity verdict, not hand-waved.
- **Conditional-download uncertainty**: if attachment content GETs
  ignore validators, per-card docs pay full downloads every cycle —
  feeds Q7 and the D-7 criteria rather than blocking the verdict.
- **Evidence volume vs repo hygiene**: captured payloads stay outside
  the repo; only redacted excerpts ship; stage 3 diffs the PR against
  exactly plan + report + annotation.

## 9. Effort

Stage 2 (execution): **~2–3 days** — bring-up + V1 survey (~0.5 d),
V2 probe battery incl. the D-8 coupling measurement and size ladder
(~0.5–1 d), sketch refinement + merge-policy section checked against
the code (~0.5 d), report + backlog annotation (~0.5 d). Stage 3
(review & ship): ~0.5 d. This matches the dispatch's M (research)
chunk sizing and the wave analysis's S-spike estimate for the
investigation itself.
