# [Report] Tier-2 Convergence Fixture Repairs — Stage-2 Tier Records

- **Date**: 2026-10-25
- **Kind**: stage-2 execution record — chunk 3 of the backlog-triage
  dispatch (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md`
  §4), per the approved plan
  `.artifacts/plans/2026-10-25-tier2-convergence-fixtures-plan.md` (D1–D5).
- **Branch / HEAD**: `test/tier2-convergence-fixtures` at
  `5dbcecc` (commits: `1bb992a` D1 boot await, `5dbcecc` D2 version
  floor; plan commit `76cf959` beneath).
- **Zero production diffs**: the branch diff touches
  `crates/taskboard-sync-nextcloud/tests/{it_docker.rs,common/mod.rs}`
  and `.artifacts/` only.

## Verdict up front

- **#434 (boot race): FIXED and verified.** All four C-fixtures now
  clear `boot_convergence` — the previous 4/4 failure was the
  `stacks.keys().next().unwrap()` panic at boot; today every C-fixture
  boots, authors, and flushes before failing (when it fails) at a
  *later, distinct* stage.
- **#435 (version pin): FIXED and verified.** The floor assertion
  (`(1, 19, 0)` default / `(1, 17, 5)` gate) passed against Deck
  **1.19.1** on the default tier — exactly the image refresh that broke
  the old literal — and on 1.17.5 on the gate tier, with the
  behavioral `card_startdate` row green on both. The below-floor unit
  runs green in the hermetic binaries (5× per workspace run, per the
  plan's accepted wart).
- **#367 (stamp collision): already fixed by construction** by
  `4613ca0` (plan D3); this chunk's live evidence:
  `it_nextcloud_docker_attachment_content_lifecycle` and its
  `…_deck1175_` twin ran green on both tiers (1.200 s / 2.186 s — real
  runs, not env-guarded early returns). Triage comment posted on the
  issue.
- **Deeper failures past the boot fix (plan §2 step-5 STOP)**: C1
  (`…_sync_app_field_convergence`) and C2 (`…_sync_rule_convergence`)
  fail on the default tier, C4 (`…_deck1175_scheduled_fallback_convergence`)
  fails on the gate tier — all at the *anchor* stage, which the
  wave-exit session never reached (it died at boot). Two distinct
  production-side candidates, characterized below; **not fixed in this
  chunk** (tests-only scope), recorded for a follow-up fix chunk.

## D4 matrix — default tier (Deck 1.19.1, Nextcloud 35.0.1)

- Bring-up: tiers already up from the wave-exit session
  (`taskboard-it-nextcloud`, port 8370, Up 15 h healthy);
  credentials re-minted per run via
  `eval "$(scripts/nextcloud_it_setup.sh up)"`.
- Command: `cargo nextest run -p taskboard-sync-nextcloud --run-ignored
  only -E 'test(it_nextcloud_docker)' --no-fail-fast`
- Summary: `Summary [ 70.615s ] 21 tests run: 19 passed, 2 failed, 277 skipped`

| Test | Outcome | Duration |
|---|---|---|
| it_nextcloud_docker_attachment_content_lifecycle | PASS | 1.200s |
| it_nextcloud_docker_board_lifecycle | PASS | 0.209s |
| it_nextcloud_docker_capabilities | PASS | 0.058s |
| it_nextcloud_docker_conditional_read_cycle | PASS | 0.310s |
| it_nextcloud_docker_deck1175_* (4, env-guarded) | PASS | 0.006s each (early-return skips; the real gate rows ran on the gate tier) |
| it_nextcloud_docker_delete_missing_board_is_not_found | PASS | 0.051s |
| it_nextcloud_docker_full_tree_lifecycle | PASS | 0.921s |
| it_nextcloud_docker_lists_boards | PASS | 0.048s |
| it_nextcloud_docker_startdate_round_trip | PASS | 0.661s |
| **it_nextcloud_docker_sync_app_field_convergence** | **FAIL** | 30.009s (outer deadline) |
| it_nextcloud_docker_sync_archived_cards | PASS | 1.240s |
| it_nextcloud_docker_sync_card_field_writes | PASS | 1.542s |
| it_nextcloud_docker_sync_cross_stack_move | PASS | 1.052s |
| it_nextcloud_docker_sync_deleted_board_cascade | PASS | 0.739s |
| it_nextcloud_docker_sync_echo_idempotence (C3) | PASS | 0.819s |
| **it_nextcloud_docker_sync_rule_convergence** | **FAIL** | 30.010s (outer deadline) |
| it_nextcloud_docker_sync_server_labels_adopt | PASS | 0.530s |
| it_nextcloud_docker_sync_two_way_round_trip (D1) | PASS | 1.189s |

All passing durations are inconsistent with env-guarded early-return
skips (the false-pass trap, D4 §"False-pass trap") except the four
`deck1175_*` rows, which are env-guarded by design on this tier and
ran for real on the gate tier below.

Failure transcript (identical shape for C1 and C2):

```
thread 'it_nextcloud_docker_sync_app_field_convergence' (795125) panicked at crates/taskboard-sync-nextcloud/tests/it_docker.rs:939:10:
test must finish within the outer deadline: Elapsed(())
```

## D4 matrix — gate tier (Deck 1.17.5, Nextcloud 33.0.10)

- Bring-up: `taskboard-it-nextcloud-deck1175`, port 8371, Up 15 h
  healthy; `DECK_VERSION=v1.17.5` + `TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache`,
  suffix `-deck1175`. First run with the default export prefix
  produced four 0.006 s **false passes** (credentials unset →
  env-guarded skips); re-run with
  `EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175`. Recorded honestly per
  D4: the false-pass run is discarded, the run below is the record.
- Command: `cargo nextest run -p taskboard-sync-nextcloud --run-ignored
  only -E 'test(it_nextcloud_docker_deck1175)' --no-fail-fast`
- Summary: `Summary [ 32.817s ] 4 tests run: 3 passed, 1 failed, 294 skipped`

| Test | Outcome | Duration |
|---|---|---|
| it_nextcloud_docker_deck1175_attachment_content_lifecycle | PASS | 2.186s |
| it_nextcloud_docker_deck1175_capabilities | PASS | 0.111s |
| **it_nextcloud_docker_deck1175_scheduled_fallback_convergence (C4)** | **FAIL** | 30.009s (outer deadline) |
| it_nextcloud_docker_deck1175_startdate_unsupported | PASS | 0.510s |

## The deeper failures (plan §2 step-5 STOP) — characterization

Localization method: temporary stage-marker `eprintln!`s (reverted
before commit; final diff clean) + direct wire/DB inspection of the
leftover tier state. Both failures sit **past** the D1 boot fix:
`boot_convergence` completes (A stacks non-empty, echo-bind, B
adoption all green), `set_field` lands, and the fixture then dies
waiting for the anchor.

### F-A — default tier (Deck 1.19.1): anchor created but never confirmed

C1's marker trace: `boot:done → set_field:done → await-anchor (hang)`.
The anchor card **is** created server-side — leftover board 30 carries
card 32 `[taskboard] app-data` with `attachmentCount: 1` — so the
flush works; the *confirmation* never succeeds. Wire evidence (card 32
detail, Deck 1.19.1):

```json
{
  "id": 8, "type": "deck_file", "data": "taskboard-appdata.json",
  "extendedData": {
    "filesize": 278, "mimetype": "application/json",
    "info": { "dirname": ".", "basename": "taskboard-appdata.json",
              "extension": "json", "filename": "taskboard-appdata" }
  }
}
```

On 1.19.1 listings there is **no `extendedData.fileName`**, and
`info.filename` is the basename *without extension*. The decode
(`model.rs::normalize_file_name`) folds `info.filename` into
`file_name`, yielding `"taskboard-appdata"` — non-empty, so
`stored_file_name` (`anchor.rs:116`) prefers it over the string `data`
payload (`"taskboard-appdata.json"`) and the comparison against
`ANCHOR_ATTACHMENT_FILENAME` never matches: the anchor scan can never
confirm on a 1.19.1 server. This explains C1 (explicit anchor wait)
and C2 (B's rule adoption rides the confirmed-anchor document path).
C3 passes because it never awaits scan confirmation.

**Production impact candidate**: not just a fixture problem — the
anchor scan (`scan_snapshot` → `stored_file_name`) runs in the
production pull path, so on Deck 1.19.1+ the appdata anchor is
re-created instead of adopted (the observed leftover state had exactly
one anchor because each failing run uses a fresh board; a long-lived
board would accumulate unconfirmed anchor candidates).

**Proposed fix direction (out of scope here)**: in
`normalize_file_name`, prefer `info.basename` (full stored name) over
`info.filename` when `fileName` is absent — or extend
`ExtensionInfo` with `basename` and rank the fallback
`fileName → info.basename → info.filename`. The 1.17.5 echo shape
(§F-B context) must stay decoding; the tier-1 contract fixtures and
the wiremock suite pin both shapes.

### F-B — gate tier (Deck 1.17.5): anchor never created

C4's marker trace: `boot:done → set-field-done → (anchor wait hang)`.
The leftover gate-tier board (7, `taskboard-it-ffc3840d`, DB-queried
inside the container) contains **only the task card and zero
attachments** — no anchor card at all. So on 1.17.5 the scheduled
fallback flush does not produce the anchor within the deadline. Less
characterized than F-A (actor tracing was not captured); candidates
are the known sporadic 1.17.5 write-path `Server(500)`s exhausting the
30 s budget via actor retry/backoff, or a flush-path decode mismatch
on the 1.17.5 echo shapes. Needs its own fix chunk with actor-level
tracing enabled.

### Why these were never seen before

The C-series' first-ever execution was the wave-exit session
(2026-10-25), where all four died at the *boot* race (#434) before
reaching the anchor stage. The D1 fix unblocked the boot; the anchor
stage had never executed anywhere (tier-1 wiremock mocks pin the
*measured* 1.19.0 echo shape, where `fileName` was present — the
1.19.1 image refresh is new since that shape was recorded).

## No-collateral proof

- Untouched tier-1 wiremock suite (59 tests, includes the f-series
  families):
  `Summary [ 0.127s ] 59 tests run: 59 passed, 0 skipped`.
- Hermetic crate suite (after both commits):
  `Summary [ 2.928s ] 272 tests run: 272 passed, 26 skipped`.
- `cargo fmt --all -- --check` clean; `cargo clippy --workspace
  --all-targets -- -D warnings` clean (also run by the pre-commit
  hook on both commits).

## Deviations from the plan

1. **C1/C2/C4 do not pass** (plan §3 acceptance row 1) — the §2
   step-5 STOP route was taken: bug candidates reported above, no
   fixture weakening, no production changes. #434's acceptance
   ("boot_convergence awaits the first pull") is fully met; the
   deeper failures are new discoveries past the previously-observed
   failure point, not regressions of this chunk.
2. **Gate-tier export prefix** (`EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175`)
   was required for real gate runs — the plan's D4 bring-up line omits
   it; without it the gate run is four false-pass skips. The plan
   text should be read as amended.
3. Localization used temporary (reverted) stage markers and
   out-of-band wire/DB inspection; no committed change came from it.

## Tier teardown

Both tiers were left up at handoff (they pre-existed this chunk's
session; the parallel chunk sessions share them). Neither stage
created new containers; no `down` was issued to avoid disrupting
concurrent sessions.
