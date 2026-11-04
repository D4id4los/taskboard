# [Report] Nextcloud Files WebDAV — Live-Tier Characterization (DR-8 input)

- **Date**: 2026-11-05
- **Context**: DR-8 (issue #626, the nextcloud-files-sync substrate
  design round) needs the transport facts its design consumes:
  whether the file-sync report's hazard table
  (`.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  §2.2 — written against an *external sync client* medium) improves as
  predicted for *direct WebDAV* (options report §4.B), and which
  server capabilities exist on the actual tier. The user authorised
  live probing of the Tier-3 test account (2026-11-04/05 session).
- **Method**: curl probes against the live tier via the Tier-3 `.env`
  convention (`TASKBOARD_IT_NEXTCLOUD_URL/USER/TOKEN`,
  `docs/testing_strategy.org` §8; the repo is **public** — no domain,
  account, or token value appears in this report; the probe artefacts
  were deleted server-side after capture). Every finding below lists
  the request shape and the observed outcome so the round (and the
  docker-tier tests) can re-pin it.

---

## 1. Findings

| # | Probe | Observed (2026-11-05) | Design consequence |
|---|---|---|---|
| F1 | Server version (`GET /status.php`) | **Nextcloud 34.0.5**, community edition, no maintenance flag; **hosted instance** (managed provider — no server-side config control) | Design for stock capabilities only; nothing may depend on admin-configurable behaviour |
| F2 | Auth (Basic, app password) | WebDAV and OCS both accept the app-password Basic credential (the same credential family the Deck connector stores) | The substrate reuses `CredentialStore` + the ADR 0008 env escape unchanged; no second credential |
| F3 | `PUT` with `If-None-Match: *` (create-once), twice | **201 then 412** — the server enforces create-once | **Per-device path ownership is server-enforceable** (the report's central T4 hazard becomes a server-side invariant): `identity.json` and any future immutable file kinds are guarded by construction |
| F4 | `PUT` with `If-Match: <served etag>` — match vs stale | **204 on match, 412 on stale**; the etag is served **already quoted** and must be echoed verbatim (an early 412 in this characterization was a client double-quoting bug, not server behaviour) | Single-writer rewrites (`info.json`, `appdata.json`) get lost-update protection; an unexpected 412 on one's own path is the **fork/clone alarm signal** |
| F5 | ETag semantics across identical re-upload | The file etag **changes on every PUT, byte-identical content included** (upload-derived, not content-hash); otherwise stable across GETs | Etag = version token for change detection (per-cycle PROPFIND compare), never a content identity; readers tolerate redundant re-GETs (the fold is idempotent); writers skip no-change PUTs by local comparison |
| F6 | `PROPFIND` Depth 1 with `getetag/getcontentlength/getlastmodified` | Works; member file etags as F5; **collection etag is a shorter tree etag** that changes with member changes | One PROPFIND per cycle over `devices/` suffices as the change detector; collection etags give a cheap whole-tree dirty check |
| F7 | Chunked-upload endpoint (`PROPFIND /remote.php/dav/uploads/<user>/`) | **207 — present** | Available for future snapshot-sized payloads; not needed at v1 journal scale (options report §7 numbers) |
| F8 | Trashbin (`PROPFIND /remote.php/dav/trashbin/<user>/`) | **207 — present**; the deleted probe directory is recoverable there | Accidental deletes are not immediate loss; the round books a recovery runbook note, and quarantine never deletes other devices' files |
| F9 | Cleanup (`DELETE` of the probe collection) | 204 | — |

## 2. notify_push (the responsiveness fact)

The capabilities payload advertises the **notify_push app** with
types `files/activities/notifications`, a `wss://` websocket endpoint,
and a pre-auth URL. Conclusion for DR-8: **near-real-time change push
is available on the live tier**, but it is an *enhancement*, not a v1
dependency — the ruling posture stands (poll at the sync cadence
first; a websocket dependency is a fenced follow-up with its own
stack-approval and battery/mobile analysis).

## 3. The hazard table, re-derived for direct WebDAV

| File-sync report hazard | Via external sync client | Via direct WebDAV on this tier (evidence) |
|---|---|---|
| T1 delivery delay | 10–20 min typical | Poll-cycle bound; push available (§2) |
| T4 same-path concurrent writes | silent-loss or conflict-copy, unknowable | **Server-enforced**: create-once (F3) + lost-update guard (F4) |
| T3 torn files | possible | GET is whole-or-error |
| T5 mtimes as protocol data | unusable | Still unused — etags are the only ordering-free version tokens (F5) |
| T8 quota/throttling | opaque | Server quota applies (hosted tier); v1 payloads are bounded (journal scale); measure, don't assume |
| T9 semi-hostile medium | sync-client quirks | The tree is user-visible/editable in Files UI — tolerant-parse + quarantine + trashbin runbook stay mandatory (F8) |

One new hosted-tier caveat: since the instance is provider-managed,
**server behaviour can change under the app at upgrade time** — the
docker-tier tests must pin these semantics against the project's own
controlled Nextcloud image, and the live Tier-3 tests re-pin them
against the real tier (the existing Deck characterization pattern).

## 4. What DR-8 takes as binding from this report

1. Conditional PUTs are the transport's correctness backbone (F3/F4)
   — the design leans on them, and the docker tier pins them.
2. Etags are version tokens, never content identities (F5) — change
   detection and the skip-PUT optimisation are designed accordingly.
3. notify_push exists but stays fenced (§2).
4. No migration, admin-config, or chunking dependency in v1 (F1/F7).

## 5. References

- Issue #626 (DR-8) + the 2026-11-04 rulings (Q1–Q8) recorded there.
- `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  (the T-model and the backbone this transport replaces the client of).
- `.artifacts/reports/2026-11-04-per-user-appdata-roaming-options-report.md`
  §4.B (the predicted table this report confirms) and §9 (the ruling
  context).
- `docs/testing_strategy.org` §8 (the Tier-3 `.env` convention);
  ADR 0008 (credential store + env escape).
- Deck precedent: `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`
  (the characterization pattern this report follows).
