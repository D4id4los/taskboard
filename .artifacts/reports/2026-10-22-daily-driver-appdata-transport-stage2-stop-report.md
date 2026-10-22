# [Report] Daily-Driver Chunk 3 Stage 2 — STOP: two Deck 1.17.5 contract divergences

> **Resolved 2026-10-22 (ruled):** the user rejected degradation and
> ruled for version-specific parity — a capability-seamed connector
> and `scheduled` native-when-available/sidecar-fallback. Normative
> follow-ups: the design addendum
> `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`
> and the full-surface characterization
> `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`
> (§5's options below are superseded by that ruling).

- **Date**: 2026-10-22
- **Kind**: stage-2 implementation report — **stopped at the gate** (MD-5/DDF-10/D5).
- **Branch**: `feat/sync-appdata-transport` @ `332c89b` (worktree
  `../taskboard-WT-feat-sync-appdata-transport`). No push, no PR (stage-2
  handoff point; stage 3/PR work resumes only after the user rules on the
  findings below).
- **Landed on the branch**: the two harness/fixture commits only
  (`84ab03b` the parameterized docker variant, `332c89b` the fixtures).
  **No transport code landed** — commits 3–5 of the stage-1 plan's ladder
  (domain `appdata.rs`, evidence/op arm, sync ceremony wiring) are **not
  written**, per the acceptance ordering ("fixtures green before the
  transport code lands") and D5's divergence protocol.

## 1. What ran

The pinned gate instance came up exactly per the stage-1 plan's
invocation (D1/D2):

```sh
TASKBOARD_IT_SUFFIX=-deck1175 TASKBOARD_IT_PORT=8371 \
EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175 \
TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache DECK_VERSION=v1.17.5 \
scripts/nextcloud_it_setup.sh up
```

Instance: `nextcloud:33-apache` + Deck **1.17.5** installed from the
official `nextcloud-releases` GitHub release tarball
(`deck-v1.17.5.tar.gz`; see §4 harness notes), version verified via
`occ app:list` by the script. Fixtures run:

| Fixture | Default tier (NC 35.0.1 / Deck 1.19.0) | Gate tier (NC 33 / Deck 1.17.5) |
|---|---|---|
| `it_nextcloud_docker_attachment_content_lifecycle` (now incl. the no-validators pin) | **PASS** | **FAIL** — divergence A (§2.1) |
| `it_nextcloud_docker_startdate_round_trip` | **PASS** | **FAIL** — divergence B (§2.2) |
| `it_nextcloud_docker_deck1175_attachment_content_lifecycle` | — | **FAIL** — divergence A |
| `it_nextcloud_docker_deck1175_startdate_round_trip` | — | **FAIL** — divergence B |

## 2. The findings (STOP material — the user rules; no adaptation made)

### A — 1.17.5 rejects the attachment-create multipart form our client sends

- The client's create form (`type=deck_file` + `file`, per the D1
  1.19.0-evidenced part set) gets **`400`** from the 1.17.5 API-level
  route `POST …/boards/{b}/stacks/{s}/cards/{c}/attachments`.
- Source-verified against the installed app:
  `AttachmentApiController::create(int $cardId, string $type, string
  $data)` — 1.17.5 declares `data` a **required** request parameter;
  Nextcloud rejects the request with 400 when it is absent. Deck
  1.19.0's controller does not require it (our default-tier fixture
  passes unchanged).
- With a `data` form field added manually (curl), the create succeeds
  (200): the stored filename still comes from the `file` part's client
  filename; `FileService::create` overwrites the entity's `data` with
  it. So the route works on 1.17.5, but only with an extra field.
- Decode-shape drift observed alongside (matrix finding, not adapted):
  the 1.17.5 create response's `extendedData` carries **no `fileName`
  key** (it nests the name under `info.filename`), where the fixture
  asserts `extendedData.fileName == upload_name` per the 1.19.0 shape.

**Impact on the design**: the flush path's upload (D-S1/D-S4) uses this
create route. On 1.17.5 the connector as shipped cannot create the
anchor attachment at all — the live tier is *more* than
expected-degraded for writes; it is write-dead on create. The design's
expected-degraded stance (design §4) anticipated exactly this class of
finding and stands until ruled.

### B — 1.17.5 has no `startdate` support on the card write path at all

- Create with `startdate: Some(ts)`: the field is **ignored** (the
  created card reads back `startdate: null`; the create response does
  not even carry the key).
- Full-send card PUT: 1.17.5's `CardApiController::update(string $title,
  $type, string $owner, string $description = '', int $order = 0,
  $duedate = null, $archived = null)` has **no `startdate` parameter**
  (source-verified). 1.19.0's signature has `$startdate = null` — the
  feature was added to the API inside the 1.17.5→1.19.0 window.
- Consequence for the wave: DDF-2's "scheduled rides Deck's native
  startdate" ruling **cannot work on the live production tier** as
  landed — chunk 1's native-`startdate` writes would be silently
  dropped (create) or rejected (update) by the live server. This is a
  chunk-1-facing finding, not just chunk 3's.

## 3. What was NOT changed

- No fixture was weakened, skipped, or adapted to the 1.17.5 behavior.
- No client code changed to send a `data` field (that would be the
  exact silent adaptation MD-5 forbids — it is one *candidate* fix but
  it is a design decision: it diverges from the 1.19.0-evidenced form
  and from the frontend's own create call, which uses the app-level
  route).
- No transport commits landed; no CHANGELOG entry (nothing to announce
  yet — the chunk is not delivered).

## 4. Harness notes (recorded, not divergences)

- The stage-1 plan's GitHub asset URL
  (`nextcloud/deck/releases/download/$DECK_VERSION/deck.tar.gz`) 404s:
  Deck moved release hosting to the **`nextcloud-releases`** org with a
  versioned asset name (`deck-v1.17.5.tar.gz`); the app store's own
  download links point there. The script pins that URL now (R4's
  fail-loud path fired first; this is harness mechanics, not behavior).
- `occ app:list` parsing needed the leading-space-aware awk field —
  fixed in the same harness commit.

## 5. Decision needed from the user (options, not recommendations)

1. **Divergence A**: keep the client's part set and declare the live
   tier write-dead for the sidecar (expected-degraded extended to
   create); **or** have the connector send an additive `data` form
   field. The additive field is **verified tolerated by 1.19.0** (hand
   probe on the default tier: HTTP 200, stored filename still from the
   `file` part's client filename), so it is a compatible fix — but it
   diverges from the frontend's own create call (which uses the
   app-level route) and is a wire-contract change, so the ruling is
   the user's.
2. **Divergence B**: whether the native-`scheduled` lane (DDF-2 branch
   A) stays (live tier degrades to no `scheduled` sync, silently
   dropped by the server), gets a version gate, or the document lane
   takes `scheduled` over (the previously ruled-out branch).

Per MD-5 nothing here is decided by the implementation; the chunk
resumes at stage-2 commit 3 once the ruling lands.

## 6. Gates on the landed commits

All green (AGENTS §10): `fmt --check`; `clippy --workspace
--all-targets -D warnings`; `nextest --workspace` (1384 passed);
`cargo test --doc --workspace`; `cargo deny check` (advisories/bans/
licenses/sources ok — no new dependencies); the CI-identical
llvm-cov three-command sequence (the quarantine step caught one torn
profraw and merged clean; `target/llvm-cov-target` removed after
`lcov.info`).

Default-tier docker fixtures re-run green after the fixture commit
(`attachment_content_lifecycle` incl. the new no-validators pin, and
`startdate_round_trip`), proving the default tier's behavior is
unchanged and the new assertions hold on 1.19.0.
