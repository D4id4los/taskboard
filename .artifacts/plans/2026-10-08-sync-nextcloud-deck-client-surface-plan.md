# [Plan] Deck Client Full Surface — Boards Slice → Complete API Access Point

- **Date**: 2026-10-08
- **Target**: `taskboard-sync-nextcloud` client surface (wire model + endpoints)
- **Goal**: grow the crate from the three-operation boards slice into a
  complete, typed, retry-safe Deck OCS REST access point covering every
  resource taskboard's future sync actor needs — boards, stacks, cards,
  labels, attachment metadata, and conditional (validator-based) reads.
- **Status**: DRAFT (awaiting user review)
- **Strategy source**: `docs/testing_strategy.org` §8 (all four tiers);
  API reference: `nextcloud/deck` `docs/API.md` (deck/api/v1.0, fetched
  2026-10-08).
- **Successor to**: `.artifacts/plans/2026-10-08-nextcloud-sync-testing-plan.md`
  (whose §8 deferred exactly this surface).

---

## 1. Context & Known Facts (do not re-derive)

The crate currently implements a vertical slice — `boards()`,
`create_board()`, `delete_board()` — with the envelope decoder, typed error
enum, `BackoffPolicy` retries, and the Tier 0–3 test harness all in place and
battle-tested. Facts already established by that effort:

- Board ids are JSON numbers; `u64` is correct across all resources (same
  server, same column family).
- Nextcloud 35 returns the **bare payload** (no OCS envelope) for
  `Accept: application/json`; the decoder tolerates both shapes.
- Deck's DELETE on boards is a **soft delete**: the listing keeps returning
  the board (stamped `deletedAt`) and can lag Deck's board cache; the DELETE
  response is the authoritative record.
- Deleting a missing board yields **403 Forbidden** on Nextcloud 35 (not
  404); implausibly huge ids overflow Deck's int columns and yield 500.
- Nextcloud app passwords never contain `:` (Basic-auth split invariant).
- Retries: 3 attempts total, 500 ms doubling capped at 8 s, only on
  `429`/`503`/transport — unchanged by this plan; every new endpoint simply
  rides `send_json`.
- `reqwest` is pinned with `json` + `rustls-tls`. **`chrono` is not yet a
  dependency of this crate** (it is pinned workspace-wide); PR B adds it.
- The sync **actor** (poll loop, offline queue, conflict resolution,
  channel wiring to the state engine) does not exist yet, and is **out of
  scope** here — this plan delivers the access point the actor will drive.

Deck API facts (from the API reference, to bake into the wire model):

- Timestamps: `createdAt`/`lastModified`/`deletedAt` are epoch-second
  integers; `duedate`/`done` are ISO-8601 strings (`2020-01-20T09:52:43+00:00`)
  or `null`.
- Cards expose `overdue`, `attachmentCount`, `commentsUnread` as
  server-managed read-only fields; labels/assignments/archived state change
  via dedicated sub-endpoints, not card PUT.
- Conditional GETs exist: `If-Modified-Since` on `GET /boards` and
  `GET /boards/{id}/stacks`; ETags on cards/attachments (also embedded in
  JSON); `If-None-Match` yields `304`. There is **no** documented
  write-side `If-Match` optimistic concurrency.
- Only comments paginate (`limit`/`offset`); everything else returns whole
  collections.

## 2. Scope

**In scope** — the crate becomes a complete Deck REST client:

| Resource | Operations |
|---|---|
| Boards | list, get, create, update (title/color/archived), soft delete, undo delete, clone |
| Stacks | list (active/archived filter), get (with cards), create, update, delete |
| Cards | get, create, update (full round-trip), delete, archive/unarchive, reorder/move, label assign/remove |
| Labels | list, create, update, delete |
| Attachments | list metadata only (embedded file info; no upload/download) |
| Cross-cutting | `DeckColor` validated type; complete typed status matrix incl. 400/409/412; conditional reads returning `304` as data, not error |

**Out of scope** (→ backlog entries appended with this plan, §9):
attachment writes + content download (needs `reqwest` `multipart`),
comments, ACL/participant management, card user assignment, board
import/export, session and config endpoints, and the whole sync-actor layer
(poll policy, conflict resolution, DTO→domain mapping).

## 3. Design Decisions

1. **Wire model stays in this crate.** Deck DTOs (`Board`, `Stack`, `Card`,
   …) live in `taskboard-sync-nextcloud` (new `src/model.rs`), not in
   `taskboard-domain`. Rationale: the adapter owns its wire format; the
   domain crate stays Deck-agnostic and gains canonical entities + ports
   with the sync-actor plan, at which point pure DTO→domain mapping
   functions are added here and proptest-ed. Moving types to domain now
   would couple pure domain types to Deck's camelCase wire quirks.
2. **`DeckColor` newtype, crate-internal** (generalizes the backlog's
   `BoardColor` to labels). Strict construction for writes
   (`DeckColor::from_hex` — six hex digits, `#` rejected, case-insensitive,
   stored lowercase); **lenient deserialization** for reads (any string
   passes through so one malformed server color cannot fail a whole
   listing). `Display` round-trips. Proptest: valid inputs round-trip,
   `#`-prefixed / 5- / 7-digit / non-hex inputs are rejected. It moves to
   domain only if a second consumer ever appears (YAGNI).
3. **Timestamps**: epoch-second fields stay wire-faithful `i64`;
   `duedate`/`done` become `Option<chrono::DateTime<Utc>>` via chrono's
   serde support. Timezone interpretation belongs to the future domain
   mapping, not the DTO layer.
4. **PUT semantics — sparse changesets vs full round-trip.** Deck fills
   server-side defaults for missing PUT fields (e.g. omitting `order`
   resets it), so: boards/stacks/labels PUT send explicit sparse changeset
   structs (`BoardChanges`, `StackChanges`, `LabelChanges` — every field
   required by the changeset, none accidentally dropped); **card PUT
   round-trips the full fetched `Card`** (the caller mutates fields on the
   struct it got from a read, then calls `update_card(&card)`), because the
   card PUT consumes too many fields to safely rebuild a partial body.
   Contract tests pin the exact JSON body of every PUT.
5. **Error matrix completion** (PR B, one shot): add non-retryable
   `BadRequest` (400), `Conflict` (409), `PreconditionFailed` (412) to
   `DeckError`. Deck today produces mainly 400 (validation); 409/412 are
   mapped so the Tier 1 matrix named in `docs/testing_strategy.org` §8 is
   fully covered and future server behavior degrades into a typed variant
   instead of a decode failure. `304` is **not** an error — see decision 7.
6. **Client-side validation only where the type system makes it free**:
   color (decision 2). Title lengths (≤100 boards/stacks/labels, ≤255
   cards) are enforced server-side and surface as `BadRequest`; no local
   length rules to drift from the server.
7. **Conditional reads return data, not errors.** New
   `Fetch<T> { data: Option<T>, etag: Option<String>, last_modified:
   Option<String> }`; `data: None` means the server answered `304`. Validators
   are opaque header strings (ETag opaque per RFC; the HTTP-date is only
   re-emitted, never parsed). Plain methods keep their current signatures.
   `304` must be mapped **before** body decoding — today a 304 with an
   empty body would fall through to an `Envelope` error.
8. **Stacks listing filter** modelled as `StackFilter::{Active, Archived}`
   (the `stacks/archived` route), not a bool parameter.
9. **Soft-delete visibility is asserted only where proven.** Following the
   boards precedent (listing lags/keeps soft-deleted rows), lifecycle tests
   assert typed responses and the authoritative DELETE payload; absence
   from listings is asserted only after Tier 2 demonstrates it for that
   resource.
10. **Module layout**: new `src/model.rs` (all DTOs, one reviewable file;
    split into `model/` when it outgrows ~500 lines), `src/color.rs`;
    `client.rs` keeps every endpoint method on `DeckClient`; `ocs.rs`
    envelope untouched; `lib.rs` re-exports the public model.

## 4. Deliverable Shape: four sequential PRs

| PR | Branch | Contents |
|---|---|---|
| A | `refactor/sync-nextcloud-deck-color` | `DeckColor` type; retrofit boards slice |
| B | `feat/sync-nextcloud-deck-read-surface` | full wire model + all read endpoints + fixtures + error matrix |
| C | `feat/sync-nextcloud-deck-write-surface` | all write endpoints + full-tree live lifecycle |
| D | `feat/sync-nextcloud-deck-conditional-reads` | `If-Modified-Since`/ETag conditional GETs + `Fetch<T>` |

Sequential (each branch starts from `main` after the previous PR lands),
like the testing plan's A→B→C: later PRs build on earlier models, and no
stacked-PR rebase dance is needed. Each PR passes all §10 gates on its own
and leaves `main` shippable.

## 5. PR A — `DeckColor` (small, lands first)

Doing this first avoids reopening public signatures across three later
branches — exactly the cost the backlog entry predicted.

- `src/color.rs`: newtype per decision 2, with `from_hex`, `Display`,
  `FromStr` (`Err = ParseColorError`, its own small `thiserror` type — color
  parsing happens outside `DeckClient`, so it does not belong in
  `DeckError`).
- `create_board(title, color: &DeckColor)`; `Board.color: DeckColor`.
- Proptest in `color.rs` (valid/invalid classes, round-trip, lowercase
  normalization); existing Tier 0/1 tests updated to construct colors via
  the type.
- Retires the `[2026-10-08] Dedicated BoardColor type` backlog entry
  (status flip on merge).

**Done when**: gates green; no `&str` color parameter remains in the
crate's public API.

## 6. PR B — Wire Model + Read Surface + Error Matrix

- Add `chrono.workspace = true` to `[dependencies]` (only manifest change
  of the whole plan).
- `src/model.rs`: `Board` (grows `archived`, `labels`, `acl`,
  `permissions`, `owner`, `users`, `shared`, `lastModified`),
  `Stack` (`boardId`, `order`, `cards`), `Card` (`title`, `description`,
  `type`→`kind` (serde rename), `order`, `duedate`, `done`, `archived`,
  `labels`, `assignedUsers`, `owner`, `attachments`, `attachmentCount`,
  `overdue`, `commentsUnread`, epoch timestamps), `Label`, `Acl`,
  `Participant`, `Attachment` + `extendedData` (metadata only),
  `BoardPermissions`, `StackFilter`. Every field the server may omit or
  grow gets `#[serde(default)]`; unknown fields ignored (established
  decoder behavior).
- Read methods: `board(id)`, `stacks(board, filter)`, `stack(board, stack)`
  (nested cards), `card(board, stack, card)`, `labels(board)`,
  `attachments(board, stack, card)`.
- `DeckError` gains `BadRequest`, `Conflict`, `PreconditionFailed`
  (decision 5) with Tier 1 mapping tests.
- Tier 0: hand-written first-pass fixtures per resource
  (`stacks_list.json`, `stack_detail.json`, `card_detail.json`,
  `labels_list.json`, `attachments_list.json`, plus duedate/done/archived
  variants).
- Tier 1: per-endpoint happy path (method, path, headers, decode) +
  error-matrix tests for the new variants.

**Done when**: gates green; `GET`-only surface complete; every new DTO has
a fixture-driven decode test.

## 7. PR C — Write Surface

- Boards: `update_board(id, BoardChanges { title, color, archived })`,
  `restore_board(id)` (`undo_delete`), `clone_board(id, CloneOptions)`
  (all-flags-false `Default`).
- Stacks: `create_stack(board, title, order)`,
  `update_stack(board, stack, StackChanges { title, order })`,
  `delete_stack(board, stack)`.
- Cards: `create_card(board, stack, NewCard { title, order, description,
  duedate, kind })`, `update_card(&Card)` (full round-trip, decision 4),
  `delete_card`, `archive_card`/`unarchive_card`, `reorder_card(board,
  stack, card, order, target_stack)` (also the move primitive),
  `assign_label`/`remove_label(board, stack, card, label)`.
- Labels: `create_label(board, title, color)`,
  `update_label(board, label, LabelChanges { title, color })`,
  `delete_label(board, label)`.
- Tier 1: pin the exact JSON body of every PUT/POST (sparse changesets
  contain exactly their fields; card PUT contains the full struct);
  reorder/assign bodies; error paths per the matrix.
- Tier 2/3 (shared `common::suite`): `full_tree_lifecycle` — create board →
  label → stack → two cards → set duedate + assign label → archive one
  card → reorder/move → update card → read back the tree → delete board
  (teardown), all under `with_deadline` with run-id titles and best-effort
  board teardown. Soft-delete visibility asserted per decision 9.
- Harvest example: extend to build this same run-id tree and write
  per-resource fixtures (review-before-commit rule unchanged).

**Done when**: gates green; docker CI job green incl. the new lifecycle;
the user has run the Tier 3 filter successfully (§10).

## 8. PR D — Conditional Reads

- `Fetch<T>` + `Validators { etag, last_modified }` (decision 7);
  conditional variants for `boards`, `stacks`, and `card`
  (`fetch_boards(&validators)`, `fetch_stacks(...)`, `fetch_card(...)`).
- `attempt()` maps `304` → `Fetch { data: None, … }` **before** decoding;
  validators are read from the response headers and returned so the next
  poll reuses them.
- Tier 1: 304-with-empty-body contract test (guards the fallthrough bug
  class); validator echo; conditional + mutation interplay mocked.
- Tier 2/3: real conditional cycle — fetch (validators) → conditional
  fetch → `304` → mutate → conditional fetch → fresh data with new
  validators.

**Done when**: gates green; a polling caller can run a full
fetch-not_modified-refetch loop using only crate types.

## 9. Out of Scope → Backlog (entries appended with this plan)

- Sync actor: poll scheduling, offline write queue, conflict resolution,
  `taskboard-domain` canonical entities + ports, DTO→domain mapping.
- Attachment writes and content download (`reqwest` `multipart` feature).
- Card comments (OCS endpoints, the API's only pagination).
- Administrative endpoints bundle: ACL/participant writes, card user
  assignment, board import/export, sessions, Deck config.
- Conditional-read *policy* (intervals, backoff between polls) — actor
  concern.

## 10. Verification & Acceptance (overall)

1. Each PR: all §10 gates green (`fmt`, `clippy -D warnings`, `nextest`,
   `doc tests`, `deny`).
2. Default suite stays hermetic: `cargo nextest run --workspace` green with
   Docker stopped and no `.env`.
3. Docker CI job (`Nextcloud Integration (docker)`) green per PR, including
   `full_tree_lifecycle` from PR C on.
4. Tier 3 run against the private server after PR C and PR D (user-gated,
   manual, `.env` credentials; app password only).
5. CHANGELOG `[Unreleased]` entries ride each PR.
6. Backlog updated: `BoardColor` entry retired on PR A; new entries from §9
   appended with this plan.

## 11. YOUR Tasks (manual)

1. Review this plan (decisions in §3 are proposals, esp. scope: *client
   surface only* — the actor is deliberately excluded).
2. Review harvested fixtures before any commit (existing rule).
3. Run the Tier 3 filters after PR C and PR D:
   `cargo nextest run -p taskboard-sync-nextcloud --run-ignored only -E 'test(it_nextcloud_live)'`.
4. Merge the four PRs sequentially per the double-rebase policy (branches
   are cut from updated `main` one after another — no stacked-PR rebase
   dance required).

## 12. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Partial PUT silently resets server defaults (e.g. `order`) | sparse changesets + full card round-trip (decision 4); contract tests pin exact bodies |
| Reorder endpoint's path-vs-body stack semantics are ambiguous in the docs | verify a cross-stack move at Tier 2 **before** freezing `reorder_card`'s signature; the mock then mirrors the verified semantics |
| `duedate`/`done` format drift beyond chrono's ISO parsing | fixtures harvested from the live server; custom deserializer only if Tier 2 demonstrates a non-standard shape |
| `304` falls through to envelope decoding | explicit status mapping before decode + dedicated contract test (PR D) |
| Stack/card soft-delete visibility differs from boards' | lifecycle asserts typed responses + authoritative DELETE payloads only; listing-absence assertions added only where Tier 2 proves them |
| Review load of a large surface | four sequential, independently green PRs; wire model in one file |
| Deck/Nextcloud version drift | unknown-field tolerance everywhere; harvest refresh workflow; NC-major docker matrix already tracked in backlog |
