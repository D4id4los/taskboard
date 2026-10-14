# [Plan] Deck Attachment Writes & Content Download (Chunk P2)

- **Date**: 2026-10-15
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §1 (execution model, §1.4 GUI no-go zones) and §3 chunk P2; backlog
  entry "[2026-10-08] Deck attachment writes and content download"
  (`.artifacts/taskboard-potential-improvements-backlog.md:338`),
  itself a leftover of the Deck client surface plan §9. **Normative
  wire facts**: D1 spike report
  `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  §3.2 (multipart round evidence), §3.3 (stamp advancement), §3.4
  (payload limits).
- **Branch / worktree**: `feat/sync-nextcloud-attachment-content` in
  `../taskboard-WT-feat-sync-nextcloud-attachment-content`, off
  `main` @ `7c9e6ac` (P1 verified merged: retry-after subjects on
  `main`, `git cherry` reports zero patch-unmerged commits, and the
  remote branch was pruned after merge).
- **Goal**: the client surface for card-attachment *content* —
  `multipart/form-data` upload, in-place content update, soft delete,
  restore, and raw-bytes download on `DeckClient` — built strictly on
  the D1-evidenced wire behavior. This is the surface Wave 1's sidecar
  will consume; nothing here decides sidecar semantics (dispatch scope
  fence, honored in §6).
- **Status**: awaiting stage-2 implementation.

---

## 1. Verified starting point (anchored on main @ 7c9e6ac)

1. **`attachments()` is metadata-list-only** (`client.rs:511`): GET
   `boards/{b}/stacks/{s}/cards/{c}/attachments`, decoded through
   `send_json`. Its doc comment says "content download is out of
   scope" — this chunk replaces that sentence.
2. **The request spine is shared and already carries P1.** `send_json`
   (`client.rs:951`) and `fetch_conditional` (`:978`) both build a
   private `Call { method, resource, body: Option<serde_json::Value>,
   validators }`, drive `send_with_retries` (`:1008`, BackoffPolicy +
   `RetrySleep` seam + `RateLimited { retry_after }` from P1), and
   interpret the returned `Raw { status, validators, retry_after,
   body: Vec<u8> }`. `attempt` (`:1046`) attaches the body via
   `.json(...)` and builds the URL as
   `{base}/{DECK_API_PATH}/{resource}` where
   `DECK_API_PATH = "index.php/apps/deck/api/v1.0"`.
3. **The update route is NOT under the API path.** D1 §3.2: the
   API-level `PUT …/attachments/{id}` with multipart returns `400`
   (PHP does not parse multipart PUT bodies); the working update is
   the app-level **POST**
   `/index.php/apps/deck/cards/{c}/attachment/deck_file:{id}?boardId={b}`
   (the frontend's own `updateAttachment` route). The spine's
   hardcoded API prefix cannot express this — §2 D3.
4. **Attachment decode has three evidenced wire facts to absorb**
   (D1 §3.2 + recorded fixtures):
   - mutation responses carry `"data": "<filename>"` as a *string*
     (D1 create probe) while the listing/detail payloads carry `data`
     as an *object* (`tests/fixtures/deck/attachments_list.json`,
     `card_detail.json` — real-server recordings). Today's
     `Attachment.data: Option<ExtendedData>` hard-fails on the string
     form.
   - the tier sends `extendedData.filesize`; our `ExtendedData` maps
     `size` with no alias (D1: "metadata deserialization currently
     yields the default").
   - responses carry `lastModified`; our `Attachment` has no such
     field (unknown fields are ignored, so it decodes — but §3.3's
     tier-2 assertions need it typed).
5. **Error surface is ready for this chunk** (`error.rs`): 400/404/409
   map to `BadRequest`/`NotFound`/`Conflict` — the D1-evidenced
   duplicate-filename `409` and the app route's empty-`data` `400`
   already surface typedly. One new variant is needed for local input
   validation (§2 D1).
6. **Tier harnesses**: tier-1 contract tests live in
   `tests/contract.rs` (wiremock `MockServer`, structural
   request/response assertions, `RecordingSleeper`); tier-2 bodies
   live in `tests/common/mod.rs` `suite` (run-id resources,
   `retrying_server_errors`, 30 s `with_deadline`) wrapped by
   `tests/it_docker.rs` under `#[ignore]`d `it_nextcloud_docker_*`
   names — the CI integration workflow filters
   `test(it_nextcloud_docker)` for `-p taskboard-sync-nextcloud`
   (`nextcloud-integration.yml:52`), so correctly-named tests are
   picked up with **zero workflow edits** (`ci.yml` is a GUI no-go
   zone; `nextcloud-integration.yml` is not edited either).
7. **The reqwest pin** (root `Cargo.toml:42`):
   `{ version = "0.12", default-features = false, features = ["json",
   "rustls-tls"] }` — the `multipart` feature is additive and
   TLS-choice-independent.

## 2. Decisions (every open question, resolved)

- **D1 — multipart body construction: typed payload + private form
  builder.** A public request-payload struct beside `NewCard`:
  `AttachmentUpload { file_name: String, mime_type: String, content:
  Vec<u8> }` (owned fields, house payload style; no serde — multipart
  is not JSON). A private `fn multipart_form(upload, for_update)`
  produces the `reqwest::multipart::Form` with the part names **fixed
  by the D1 §3.2 evidence**: `type` (text part, value hardcoded
  `"deck_file"` — the only evidenced attachment kind; no speculative
  type parametrization), `file` (bytes part with `file_name` as the
  client filename and `mime_type` as the part's content type), plus
  `data` (text part, the stored filename) **only on the update form**
  (validated non-empty by the app route). The **stored filename is the
  part's client filename** (D1: the server's lookup uses it; re-upload
  under a new name creates a second file, a changed-name update 500s).
  Input validation before any HTTP: non-empty `file_name`, non-empty
  parseable `mime_type` (`Part::mime_str`'s error class), else the new
  `DeckError::InvalidUpload` — an early-return typed outcome in the
  `InvalidBaseUrl` precedent family, provable at tier 1 by "zero
  requests received". The builder itself is a pure unit-test target
  (part inventory per mode).
- **D2 — raw-bytes download: a `send_bytes` sibling of `send_json`,
  same spine.** A private `send_bytes(method, resource)` builds a
  `Call` and calls `send_with_retries` with the handler
  `map_status(&raw)?; Ok(raw.body)` — no envelope decode, no
  content-type gate (D1 §3.2: the content route answers
  `text/plain;charset=UTF-8` regardless of the stored mimetype, and
  serves no `ETag`/`Last-Modified`; conditional download does not
  exist, so no validator plumbing is offered — callers detect change
  via attachment metadata, which is Wave 1's business). Public
  `download_attachment` returns `Vec<u8>`, consistent with
  `Raw.body`. GET is idempotent, so the full retry policy — including
  P1's `Retry-After` handling — applies to the new endpoints for free
  through the unchanged spine.
- **D3 — spine widening: `CallBody` enum + route scope.**
  `Call.body` becomes `Option<CallBody>` where `enum CallBody {
  Json(serde_json::Value), Multipart(reqwest::multipart::Form) }`;
  `attempt` attaches `.json(...)` or `.multipart(...)` accordingly
  (two-line change at the two existing construction sites). Because
  the working update route is app-level (§1.3), `Call` also gains a
  private `scope: Scope` where `enum Scope { OcsApi, App }` prefixes
  the resource with `index.php/apps/deck/api/v1.0` or
  `index.php/apps/deck` respectively; existing sites pin `OcsApi`.
  The query rides the resource string
  (`cards/{c}/attachment/deck_file:{id}?boardId={b}` — valid as a
  relative URL reference for `Url::join`). Rejected alternative: a
  parallel request path outside `Call`/`attempt` — it would duplicate
  auth/validator/retry plumbing (SRP violation) and lose P1's
  behavior for the new endpoints.
- **D4 — `data` field polymorphism: decode object-or-string.** Both
  wire shapes are real-server evidence (§1.4). A custom
  `deserialize_with` on `Attachment.data` accepts: object →
  `ExtendedData` (unchanged), **string →
  `ExtendedData { file_name: <string>, ..Default::default() }`** (the
  string *is* the stored filename — the same fact the object's
  `fileName` carries), null/absent → `None` (`#[serde(default)]`
  stays). Serialization keeps emitting the object form; the only
  re-serialization of attachments happens inside the card PUT
  round-trip, where the server ignores the field (server-managed).
  One typed consumer shape, no `serde_json::Value` leakage.
- **D5 — `filesize` alias, in scope.** `ExtendedData.size` gains
  `alias = "filesize"` (alongside `camelCase`'s `size` primary).
  D1 §3.2 deferred this to "the Wave-1 client work", but this chunk
  *is* that client work now, its tier-2 lifecycle asserts the decoded
  size against the uploaded byte count, and shipping endpoints that
  return a payload we knowingly mis-decode would be a half-correct
  surface. One-line additive serde alias, no consumer impact.
- **D6 — `Attachment.last_modified`, in scope as wire-faithful DTO
  completion.** Add `#[serde(default, deserialize_with =
  "null_to_zero")] pub last_modified: i64` (every D1-recorded
  attachment payload carries it). The scope fence's "no Wave-1
  fields" bars app-only *field-family* fields in the domain/outbox —
  this is the sync crate's wire DTO for the surface this chunk owns,
  and D1 §3.3's normative stamp facts are unassertable without it.
- **D7 — tier split** (§4 has the full inventory): tier-0 fixture
  decode tests pin D4/D5/D6; tier-1 wiremock contract tests pin
  routes, multipart structure (via a hand-rolled test-local multipart
  parser — boundary read off the request's `Content-Type`, parts
  split and their `Content-Disposition` name/filename asserted; no
  new dev-dependencies), typed errors, and the raw-bytes download;
  tier-2 docker lifecycle pins the server-side behavior facts (409
  duplicate rule, byte-identical round-trip, in-place update with
  preserved id/`createdAt`, soft delete + restore, §3.3 stamp
  advancement + validator invalidation). Tier-3 (live, manual-only)
  gets no new wrapper — D1 already evidenced the behavior on a real
  server; if a future tier-3 run trips on the app-level update route
  (live tier is Deck 1.17.5 vs docker's 1.19.0), that feeds the
  known-server matrix per the house rule, not this branch.
- **D8 — reqwest pin: feature-add only.** Root `Cargo.toml`:
  `features = ["json", "rustls-tls", "multipart"]` — no version
  change (dispatch §1.4 shared-file table). `cargo deny check` must
  stay green; the expected new transitive is `mime` (MIT/Apache-2.0)
  — a `deny.toml` entry happens only if the gate actually demands it.
- **D9 — no streaming seam.** Downloads buffer to `Vec<u8>` like
  every existing response (`Raw.body`). D1 §3.4: the server ceiling
  is the image's 512 MB PHP config, orders of magnitude above the
  sidecar-sized payloads this surface exists for; a streaming/ range
  seam would be speculative generality. Kiosk memory stays bounded by
  what the caller chooses to download.

## 3. The client surface (all additive, `taskboard-sync-nextcloud` only)

| Operation | Method & route | Request | Returns |
|---|---|---|---|
| Upload | `POST {api}/boards/{b}/stacks/{s}/cards/{c}/attachments` | multipart `type` + `file` | `Attachment` |
| Update content | `POST {app}/cards/{c}/attachment/deck_file:{id}?boardId={b}` | multipart `type` + `data` + `file` | `Attachment` |
| Delete (soft) | `DELETE {api}/boards/{b}/stacks/{s}/cards/{c}/attachments/{id}` | — | `Attachment` (`deleted_at != 0`) |
| Restore | `PUT {api}/boards/{b}/stacks/{s}/cards/{c}/attachments/{id}/restore` | — | `Attachment` (`deleted_at == 0`) |
| Download | `GET {api}/boards/{b}/stacks/{s}/cards/{c}/attachments/{id}` | — | `Vec<u8>` (raw, no JSON detour) |

`{api}` = `index.php/apps/deck/api/v1.0`, `{app}` =
`index.php/apps/deck`. Signatures (house naming; `update` takes no
`stack_id` because the app-level route has none):

```rust
pub async fn upload_attachment(&self, board_id: u64, stack_id: u64,
    card_id: u64, upload: &AttachmentUpload) -> Result<Attachment, DeckError>
pub async fn update_attachment_content(&self, board_id: u64, card_id: u64,
    attachment_id: u64, upload: &AttachmentUpload) -> Result<Attachment, DeckError>
pub async fn delete_attachment(&self, board_id: u64, stack_id: u64,
    card_id: u64, attachment_id: u64) -> Result<Attachment, DeckError>
pub async fn restore_attachment(&self, board_id: u64, stack_id: u64,
    card_id: u64, attachment_id: u64) -> Result<Attachment, DeckError>
pub async fn download_attachment(&self, board_id: u64, stack_id: u64,
    card_id: u64, attachment_id: u64) -> Result<Vec<u8>, DeckError>
```

Doc comments state the D1 traps as contracts: the stored filename
comes from the upload's `file_name` (create: duplicate per
(card, filename) → typed `Conflict`; update: the same name replaces
in place, a different name 500s — there is no rename); every
attachment write advances the card's entity stamp (§3.3). `update_attachment_content`'s
doc names the app-level route and why (PHP multipart-PUT). The
`attachments()` doc's "out of scope" sentence now points at
`download_attachment`. `lib.rs` re-exports `AttachmentUpload`
alongside `NewCard`.

## 4. Test design (TDD — each tier's tests written first)

### 4.1 Tier 0 — fixtures & pure units (default suite)

- New fixture `tests/fixtures/deck/attachment_create_response.json`,
  recorded shape from D1 §3.2 (string `data`, `extendedData.filesize`,
  `lastModified`, zero `deletedAt`).
- `model.rs` tests: string-`data` decode → `file_name` populated
  (D4); `filesize` alias decodes into `size` (D5); `lastModified`
  decodes (D6); the existing listing-fixture test keeps passing
  (object form regression pin).
- `client.rs` unit tests: `multipart_form` part inventory — upload
  mode has exactly `type` + `file`; update mode adds `data`; the file
  part carries the client filename and mime. (Asserted on the
  `reqwest::multipart::Form` via its `Debug`/accessors where
  practical, otherwise exercised through tier 1 below.)

### 4.2 Tier 1 — wiremock contract (`tests/contract.rs`, default suite)

Test-local helper `parse_multipart(content_type, body) -> Vec<Part{
name, filename: Option, body }>` (split on the boundary, parse
`Content-Disposition`; the helper is consumer-local until a second
suite needs it).

1. `upload_attachment_posts_multipart_with_type_and_file_parts` —
   exact nested path; request `Content-Type` is
   `multipart/form-data; boundary=…`; parts: `type` = `deck_file`
   (text), `file` with `filename="…"` and the payload bytes + mime;
   response decodes (fixture above).
2. `upload_attachment_duplicate_maps_conflict` — 409 → `DeckError::
   Conflict` (the D1 duplicate rule surfaces typedly on this route).
3. `upload_attachment_rejects_invalid_payload_without_a_request` —
   empty filename and unparseable mime each → `DeckError::
   InvalidUpload`, `received_requests()` empty.
4. `update_attachment_content_posts_the_app_level_route` — exact
   `index.php/apps/deck/cards/5/attachment/deck_file:9` path with
   `boardId=42` query; parts `type` + `data` (= filename) + `file`;
   response decodes.
5. `delete_attachment_sends_delete_and_decodes_the_tombstone` —
   response with `deletedAt != 0` decodes as such.
6. `restore_attachment_puts_the_restore_route` — `…/attachments/9/
   restore`, response with `deletedAt: 0`.
7. `download_attachment_returns_raw_bytes_verbatim` — mock serves a
   **non-UTF-8** binary body (e.g. `[0x00, 0x80, 0xFF, …]`) with
   `Content-Type: text/plain`: the returned `Vec<u8>` equals it
   byte-for-byte (pins "raw bytes, no JSON/String detour").
8. `download_attachment_retries_through_the_shared_spine` — 503, 503,
   200 sequence + `RecordingSleeper`: succeeds, exactly the policy's
   two delays (proves the new seam rides `send_with_retries`/P1).

### 4.3 Tier 2 — docker lifecycle (`common::suite` body, wrapped in `it_docker.rs`)

One shared body `attachment_content_lifecycle(client)` (run-id
board/stack/card, retry-wrapped writes, board teardown at the end),
wrapped as `it_nextcloud_docker_attachment_content_lifecycle`
(`#[ignore]`, `with_deadline`, picked up by the existing CI filter):

1. Read the card detail → capture `last_modified` (L1) and any
   validators.
2. **Upload** a run-id-named JSON file → assert decoded
   `data.file_name` = the name, `extended_data.size` = byte count
   (D5 against the real tier), `deleted_at == 0`.
3. **Duplicate upload** of the same name → typed `Conflict` (D1
   §3.2's 409).
4. **Download** → byte-equality with the uploaded payload (D1's
   sha256 round-trip, asserted as `Vec<u8>` equality).
5. **Update** through the app route with the *same* filename and new
   bytes → response keeps `id` and `created_at`, `last_modified`
   `>=` the upload's (second-granularity tolerance; strict `>`
   would be a flake, `>=` is not a quality ceiling — the payload
   genuinely changed, pinned by step 6). Re-download → new bytes.
6. **Stamp advancement (D1 §3.3)**: poll the card detail until
   `last_modified >= L1` under the deadline (Deck cache lag), then
   assert the card stamp **equals** the update response's
   attachment stamp (the measured invariant). If the step-1 read
   carried validators, re-`fetch_card` with them and assert fresh
   data (never a 304) — the §3.3 consequence the dispatch calls out
   for conditional-read consumers.
7. **Delete** → response `deleted_at != 0`; `attachments()` listing
   still contains the id (soft delete, D1 §3.2).
8. **Restore** → `deleted_at == 0`; download again → content intact
   (D1: content `lastModified` preserved).
9. Teardown: delete board (assert ok, body failure must not be
   masked).

Stage 2 records the tier-2 evidence summary (what ran, outcomes) in
its report; scratch payloads/captures never land in the repo
(dispatch hard rule) — evidence lives in the PR body at stage 3.

## 5. Commit ladder (each standalone, rebase-merge hygiene)

1. `feat(sync): decode attachment mutation payload shapes` — model.rs
   (D4 string-`data`, D5 alias, D6 `last_modified`) + fixture + tier-0
   tests. A pure decode fix, valuable alone.
2. `feat(sync): multipart attachment upload endpoint` — root
   `Cargo.toml` `multipart` feature (D8); `DeckError::InvalidUpload`;
   `AttachmentUpload` + `multipart_form`; spine widening (D3);
   `upload_attachment`; `lib.rs` export; tier-1 tests 1–3 + builder
   units.
3. `feat(sync): attachment content download returns raw bytes` —
   `send_bytes` (D2), `download_attachment`; tier-1 tests 7–8.
4. `feat(sync): attachment update delete restore endpoints` — the
   three mutations incl. the app-level route; tier-1 tests 4–6;
   `attachments()` doc update.
5. `test(sync): docker-tier attachment content lifecycle` —
   `common::suite` body + `it_docker.rs` wrapper (§4.3).
6. `docs(backlog): mark deck attachment writes done` — the entry ✅
   DONE with the one-line note that Wave 1's sidecar consumes this
   surface (dispatch §10.1; leaves the round-trip strategy entry
   open).

## 6. Scope fence & no-go checklist (checked by stage 3)

- No `taskboard-domain`, `taskboard-state`, actor/outbox/merge, or
  sidecar-format diff; no `AppState` shape change; no UI/app crate
  files. Diff is `taskboard-sync-nextcloud` + root `Cargo.toml` +
  (only if demanded) `deny.toml` + the backlog entry.
- No `ci.yml`/`nextcloud-integration.yml`/`miri.yml` edits — tier-2
  rides the existing name filter.
- reqwest pin changes features only (adjacency with the GUI slint pin
  is the one expected rebase hot spot, dispatch §1.4 — trivial).
- Gates (AGENTS §10): `cargo fmt`, `clippy --workspace --all-targets
  -D warnings`, `nextest`, `test --doc`, `deny check` (multipart
  transitive tree), `llvm-cov`.

## 7. Risks

1. **App-level update route stability** — evidenced on docker
   Deck 1.19.0 + upstream `routes.php`/frontend bundle (D1 §3.2);
   the live tier (1.17.5) is manual-only and unprobed. Mitigation:
   tier-2 pins the CI tier; any tier-3 finding goes to the
   known-server matrix, not a silent workaround.
2. **Multipart parser edge cases in tests** — hand-rolled
   (D7/no-new-deps); validated against reqwest's own wire output in
   the tier-1 tests themselves (boundary-agnostic assertions; the
   boundary is random per request).
3. **Second-granularity stamps at tier 2** — `>=` where equality is
   not normative; the normative §3.3 equality is asserted through
   deadline-bounded polling, never fixed sleeps.
4. **`deny` gate on the new transitive tree** — expected `mime`
   (MIT/Apache-2.0); allowance added only if the gate demands it.
5. **Memory on download** — buffered `Vec<u8>` (D9); bounded by
   caller choice, server ceiling 512 MB (D1 §3.4), sidecar-scale in
   practice.

Estimated diff: ~950 added lines across 9 files (client.rs ≈ +280,
contract.rs ≈ +330, common+it_docker ≈ +200, model.rs ≈ +90, error/
lib/fixture/backlog ≈ +50), plus the one-line root `Cargo.toml`
feature.
