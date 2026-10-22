# [Report] Deck 1.17.5 transport characterization — full surface pass

- **Date**: 2026-10-22
- **Kind**: characterization report (backward-looking); input to the
  version-capabilities design addendum
  (`.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`)
  and to the revised chunk-3 plan. Follows the stage-2 STOP report
  (`.artifacts/reports/2026-10-22-daily-driver-appdata-transport-stage2-stop-report.md`),
  which found the two divergences this pass characterizes in full.
- **Method**: the pinned gate instance (`nextcloud:33-apache` + Deck
  1.17.5 from the official `nextcloud-releases` tarball) via (a) the
  entire existing tier-2 docker fixture suite run unmodified, and (b)
  targeted wire probes (curl) replicating the client's exact request
  shapes for everything the suite does not cover.

## 1. The existing fixture suite against 1.17.5 (no code changes)

| Fixture (regular-tier name) | 1.17.5 result |
|---|---|
| `lists_boards` | PASS |
| `board_lifecycle` | PASS |
| `delete_missing_board_is_typed_error` | PASS (typed NotFound/Forbidden) |
| `full_tree_lifecycle` (create card, **full-send card PUT**, label assign, archive, cross-stack reorder, read-back) | PASS |
| `conditional_read_cycle` (validators/304/mutation invalidation) | PASS |
| `sync_two_way_round_trip` | PASS |
| `sync_cross_stack_move` | PASS |
| `sync_archived_cards` | PASS |
| `sync_deleted_board_cascade` | PASS |
| `sync_server_labels_adopt` | PASS |
| `sync_card_field_writes` (duedate + description push/read) | PASS |
| `attachment_content_lifecycle` | FAIL — divergence A (§2.1); with the `data` workaround every step passes (§3) |
| `startdate_round_trip` | FAIL — divergence B (§2.2); behavior characterized in §3 |

**11 of 15 pass unmodified** — the connector's entire board/stack/card/
label/conditional-read surface and the full actor sync cycle already
work against Deck 1.17.5. The version-specific surface is exactly the
two areas the STOP report found.

## 2. The two divergences, characterized

### A — attachment create requires a `data` form parameter

- 1.17.5's API create controller is
  `AttachmentApiController::create(int $cardId, string $type, string
  $data)` — `data` is a required framework parameter; without it
  Nextcloud answers a bare `400`. 1.19.0 does not require it (client's
  current form passes the default tier).
- **Workaround verified on BOTH tiers**: adding an additive
  `data` multipart field (value = the client filename) → 1.17.5: 200;
  1.19.0: 200 (tolerated, stored filename still from the `file` part's
  client filename). `FileService::create` overwrites the entity's
  `data` with the stored filename, so the field's value is inert.
- **Response-shape drift (decode-side)**: the 1.17.5 create/update
  echo carries `data` as an *object* whose `extendedData` has **no
  `fileName` key** (the name sits under `extendedData.info.filename`),
  where 1.19.0 echoes the object with `fileName` (and mutation
  responses echo `data` as a plain string). The client's decode must
  fall back (`fileName` → `info.filename`) for the create/update echo
  assertions to hold on 1.17.5.

### B — `startdate` does not exist on the 1.17.5 card surface

- **Create**: a `startdate` value in the create body is ignored; the
  created card carries no `startdate` (not even `null`).
- **Read**: card payloads have **no `startdate` key at all** (the
  client's `#[serde(default)]` decode → `None` handles this).
- **Full-send PUT**: a valued `startdate` is accepted (**200**) and
  **silently ignored** — the card afterwards still has no key. No 400
  (the earlier raw-probe 400s were the object-shaped `owner`/`labels`
  forms; the client already re-emits `owner` as the bare user-id
  string — that fix predates this wave and the full-tree fixture
  proves it).
- Source-verified: 1.17.5's `CardApiController::update(string $title,
  $type, string $owner, string $description = '', int $order = 0,
  $duedate = null, $archived = null)` has no `startdate` parameter;
  1.19.0's signature adds `$startdate = null`.
- Consequence: the native lane is a *silent write-drop* on 1.17.5 —
  no error, just no effect. The data-safety obligation (the merge
  verdict must not treat the always-null/absent remote as a clearance)
  belongs to the capability gate, not to error handling.

## 3. The six D1 contract behaviors on 1.17.5 with the `data` workaround

| Behavior | 1.17.5 measured |
|---|---|
| Upload part names | `type=deck_file` + `file` + additive `data` → 200; stored filename = `file` part's client filename; echo size matches payload |
| Duplicate create | **409** (maps to the typed `Conflict`) |
| Byte round-trip | byte-identical download |
| In-place update (app-level POST `cards/{card}/attachment/deck_file:{id}?boardId=`) | 200; **`id` and `createdAt` preserved**, `lastModified` advances; content replaced |
| Card-stamp advancement | card `lastModified` advances to exactly the attachment's stamp (D1 §3.3 measured equality holds) |
| Soft delete + restore | delete stamps `deletedAt`; tombstone stays listed; restore via **PUT** `…/attachments/{id}/restore` (the client's verb/route already match); content intact after restore |
| No validators on content | content GET carries **zero** `ETag`/`Last-Modified` headers (typed-pair probe empty) |

Additional measured facts feeding the design:

- **A tombstoned attachment's content is still downloadable on 1.17.5**
  (content GET answers 200 with the bytes while `deletedAt ≠ 0`). The
  sidecar design's `Absent` convention for tombstoned carriers stands
  (we choose not to download), but the *server* would serve it — worth
  knowing for future recovery work (issue-class note, not a change).
- The OCS **capabilities endpoint carries
  `capabilities.deck.version`** as a plain semver string on both tiers
  ("1.17.5" / "1.19.0"), alongside `canCreateBoards` and
  `apiVersions: ["1.0","1.1"]` (both advertise v1.1; we use v1.0).
  This is the detection seam for version-specific behavior.
- **One-off transient**: the very first attachment POSTs against a
  freshly-enabled instance answered `405` twice and then never again
  (identical requests). Not reproduced by the fixture suite; noted for
  the watchlist, not acted on.

## 4. Verdict

With the additive `data` field and a decode fallback, **every behavior
the sidecar transport needs works identically on Deck 1.17.5 and
1.19.0** — full sync-feature parity is achievable with one conditional
surface: the native `startdate` lane (capability-gated, sidecar
fallback per the 2026-10-22 ruling). The full remaining difference set
is §2; nothing else was observed across the whole exercised surface
(boards, stacks, cards, labels, ACL-driven typed errors, conditional
reads, attachment write path, restore verbs, capabilities).
