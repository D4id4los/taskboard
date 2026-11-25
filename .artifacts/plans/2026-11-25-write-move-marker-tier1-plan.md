# Chunk B5-p1 Plan — write_move_marker Tier-1 wiremock suite (#1366)

Round 7, chunk B5-p1 · branch `test/write-move-marker-tier1` ·
dispatch plan: `2026-11-25-backlog-reduction-round7-dispatch-plan.md` §4 ·
scope report: `2026-11-25-backlog-reduction-round7-scope-report.md` §B5 ·
issue: #1366. Status: planned 2026-11-25.

## 1. Subject

`crates/taskboard-sync-nextcloud/src/move_marker.rs::write_move_marker`
(D-BF11) — 24.29% coverage, worst file in the workspace; the only path
with any execution anywhere is stacks-ok-but-empty → typed no-op. Every
real path (anchor scan/create, sidecar download + parse + merge,
fresh-document build, update-vs-upload, the `NotFound`/`Forbidden` no-op
arms, the `UnreadableDocument` guard) is unexecuted at every tier.

Marker contract lineage: introduced `ca961b3d` (2026-11-09, "other
devices adopt a moved board"); the marker is the mixed-fleet channel of
the #708-era sidecar envelope — `MovedToFilesMarker { files_board,
committed_at }` under the `moved_to_files` key in the anchor card's
sidecar attachment (`taskboard-domain/src/board_move.rs` §22–40).
Call site: `taskboard-app/src/board_move_ceremony.rs::mark_deck` (§496).

## 2. Decisions (all open questions resolved)

- **D1 — suite location**: one new member module
  `crates/taskboard-sync-nextcloud/tests/suite/move_marker.rs`,
  registered in the merged wiremock binary
  `tests/nextcloud_wiremock.rs` with a `#[path = "suite/move_marker.rs"]
  mod move_marker;` line (verbatim pattern of the #1087 grouping).
  The binary is already in the default hermetic suite → CI hermeticity
  is inherited, zero Cargo.toml change (wiremock is already a dev-dep).
- **D2 — harness**: local helpers copied from the house style of
  `suite/contract.rs` (`client_for`, `basic_auth_header`, wiremock
  `Mock::given(... path ...) .expect(n)` with `ResponseTemplate`).
  No shared-harness change: `common` is for the actor suites; the
  contract tier keeps per-module helpers.
- **D3 — wire-level pins, never internal calls** (round law [L](c)):
  every assertion reads the *recorded wire* — wiremock `Mock` request
  expectations (method, path, exact request count) and captured request
  bodies. For uploads/update multipart bodies the test extracts the
  `file` part, parses the JSON, and round-trips it through the
  **public** domain API (`AppDataDocument` → `moved_to_files()`,
  `AppDataSections`). The crate's `build_evidence` /
  `stored_file_name` are `pub(crate)` and stay un-exported — tests do
  not need them; no production signature changes.
- **D4 — assertion style** (AGENTS §5): typed outcomes
  (`matches!(err, MoveMarkerError::UnreadableDocument)` on the
  `Err(...)` payload, not its text), structural facts (request counts,
  parsed document fields). No text/label assertions. Fixed
  `committed_at` from a chrono `FixedOffset`/`Utc.with_ymd_and_hms`
  constant so nothing depends on the wall clock (testing_strategy
  §6.1: the fixture is anchored because the oracle is exact-instant).
- **D5 — no production edits**: the chunk is test-only. If any pin
  fails, the bug is reported per AGENTS §5 (correct test left failing
  + backlog note), never weakened.
- **D6 — request-count pins encode idempotence at the wire**: e.g. the
  re-run pin asserts exactly one `update_attachment_content`-shaped
  request and zero `create_card` requests in run 2.

## 3. Scenario inventory (normative pins)

Each row: scenario → wire-level assertion.

| # | Scenario | Path exercised | Pins |
|---|---|---|---|
| S1 | Fresh board, no anchor card, non-empty first stack | anchor create + fresh document + upload | `stacks` GET (1); `create_card` POST with `title == ANCHOR_CARD_TITLE`, `order == max(existing)+1` (1, in the min-`(order,id)` stack); `upload_attachment` multipart whose JSON file part parses to a document with `format == APPDATA_FORMAT`, `board == deck(<id>)` (via domain accessors), default-empty sections, and `moved_to_files()` equal to the input marker; no `attachments`/`download`/update requests |
| S2 | Anchor card already exists | anchor reuse + update | anchor found (min card id among the titled cards across stacks — S7 refines); `attachments` GET; existing sidecar downloaded; **update** (PUT) used, not upload; merged document re-parses with the marker set |
| S3 | Idempotence on re-run | update path, twice | scripted server returns the S2-merged document for run 2's download; run 2 issues no `create_card`, exactly one update, and the written document equals run 1's (same marker, same sections) |
| S4 | Conflict-with-live-lane merge | merge preserves live data | pre-existing sidecar carries populated `AppDataSections` plus foreign `unknown` keys (the live sync lane's writes); after the write the uploaded document still contains those sections and keys verbatim and adds/replaces only the `moved_to_files` key |
| S5 | Stacks `404` (already-deleted board) | no-op arm | `Ok(())`; total request count exactly 1 (no card/attachment traffic) |
| S6 | Stacks `403` (Forbidden) | no-op arm | `Ok(())`; total request count exactly 1 |
| S7 | Anchor-title dedup | min-card-id selection | two titled cards in different stacks → the sidecar traffic (attachment GET/download/update) targets the lower card id only |
| S8 | Anchor-side attachment is soft-deleted (`deleted_at != 0`) | deleted-attachment filter | no download; fresh-document upload issued (the `deleted_at == 0` + filename filter) |
| S9 | Unreadable sidecar (`UnreadableDocument` guard) | never-rewrite safety | download returns bytes that `build_evidence` classifies as non-Document; `Err(MoveMarkerError::UnreadableDocument)` typed; **zero write requests** recorded — the unreadable sidecar is never destroyed |
| S10 | Stacks ok but all stacks empty of any stack (empty list) | typed no-op (the one currently-executed path, now explicit) | `Ok(())`; total request count exactly 1 |

Note on S4 naming: "conflict-with-live-lane" per the dispatch plan is
the merge-must-not-clobber case — the marker write shares the sidecar
envelope with the live files-sync lane's document folds. S4 is that
pin. S3 is the rerun pin. Together with S1/S2 they cover the issue's
stated scenarios.

## 4. Fixtures & structure

- New JSON fixtures under `tests/fixtures/deck/` only if a shape is not
  already expressible inline: scripted stack/card lists with anchor
  cards (`move_marker_stacks_with_anchor.json`,
  `move_marker_stacks_without_anchor.json`), a sidecar attachment list,
  and a live-lane sidecar document (S4). Reuse the existing
  `fixture()` reader pattern from `contract.rs`.
- Module outline: helpers (client/auth/fixture) → one `#[tokio::test]`
  per scenario S1–S10 → each test builds its own `MockServer` (no
  cross-test state; hermetic by construction).
- Marker under test built from a constant `Uuid` + anchored
  `DateTime<Utc>` (D4).

## 5. Risks

- **R1 — multipart body extraction**: `update_attachment_content` /
  `upload_attachment` send multipart; wiremock captures raw bytes. The
  test must locate the file part robustly (boundary varies). Mitigation:
  a tiny in-test part scanner keyed on the `Content-Disposition`
  filename of `ANCHOR_ATTACHMENT_FILENAME` — test-local, no new deps;
  if the existing contract.rs attachment pins already solved this
  (they do for `upload_attachment_posts_multipart...`, line ~1420),
  copy that extraction verbatim.
- **R2 — a pin exposes a real bug** (most plausible: S4 foreign-`unknown`
  key preservation, or S7 min-id selection). Law: fix production only if
  in-scope (it is not — this chunk is test-only); leave the failing test
  marked `..._currently_pins_...`-style or, if the semantic itself is
  wrong, file backlog + stop. Report either way.
- **R3 — client retry spine**: `DeckClient` retries transient statuses;
  the fixtures return deterministic first-response statuses so counts
  stay exact. Use `expect(n)` (exact) not `expect(n..)` on every Mock.
- **R4 — `StackFilter::Active` request shape**: pin the exact query the
  production client sends (copy from the existing `stacks()` contract
  pins) so S5/S6 mocks don't accidentally match a different route.

## 6. Estimated diff

- `tests/suite/move_marker.rs`: ~450–600 lines (10 tests + helpers).
- `tests/nextcloud_wiremock.rs`: +3 lines (mod registration + comment).
- `tests/fixtures/deck/move_marker_*.json`: 3–5 small fixtures (~120 lines).
- Production code: zero (D5).

## 7. Gate & handoff

Pre-commit hooks (memory-gate-wrapped clippy) automatic; full suite via
`scripts/run-quality-gates.sh cargo nextest run --workspace` per the
long-command discipline; every cargo invocation through
`scripts/memory-gate.sh`. Coverage delta target: `move_marker.rs` from
24.29% to ~100% of executable lines (S1–S10 cover every arm except the
unreachable `expect`s on marker serialization). Commit:
`docs(plan): the write move marker tier1 chunk plan` (this file), then
implementation commits by stage 2.

## 8. Out-of-scope findings → backlog

- **#1385** — `write_move_marker`'s read-modify-write of the sidecar
  attachment has no concurrency guard (no If-Match/ETag): a marker
  write racing a live-lane fold can lose one side's keys. Out of scope
  for a test-only chunk; recorded for a design round.
