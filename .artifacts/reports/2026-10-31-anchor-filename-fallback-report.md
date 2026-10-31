# [Report] Stage 2 — anchor filename fallback (chunk 1.1, backlog round 2)

- **Date**: 2026-10-31
- **Branch**: `fix/anchor-filename-fallback` @ `d8901479` (worktree
  `../taskboard-WT-fix-anchor-filename-fallback`; stage 3 ships)
- **Plan**: `.artifacts/plans/2026-10-31-anchor-filename-fallback-plan.md`
- **Issues**: #448 + #447 + #449 implemented; rider #282 verified
  (stays open); #436 untouched (closes on the lane re-run).
- **Commits** (each standalone, in order):
  1. `688b51f2` test(sync-nextcloud): pin the measured Deck 1.19.1 attachment listing shape (RED — reproduced the #448 fold)
  2. `c98783d1` fix(sync-nextcloud): rank the attachment filename fallback fileName → info.basename → info.filename (GREEN)
  3. `fc72c5ac` test(sync-nextcloud): the ranked fallback keeps the 1.17.5 echo and a present fileName winning
  4. `a6374c65` test(sync-nextcloud): the 1.19.1 listing shape confirms the anchor through the actor scan (wiremock S-pin a34b)
  5. `4f03d9a1` test(sync-nextcloud): C1/C2 assert the anchor is adopted exactly once
  6. `32ee3e95` feat(sync-nextcloud): durable flush-ladder tracing + the C4 capture harness
  7. `c4490e01` test(sync-nextcloud): the C1 cleared-field write actually clears the effort (fixture-only)
  8. `d8901479` test(sync-nextcloud): probe Deck's write answers on a deleted board (#282)
- **Zero-unrelated-production-diff statement**: the only production files
  touched are `crates/taskboard-sync-nextcloud/src/model.rs` (the
  `ExtensionInfo.basename` field + the ranked fallback) and
  `src/flush.rs` (durable flush-ladder debug tracing, the §4.1
  allowance). `anchor.rs`: **zero diffs**, as planned. `Cargo.toml` /
  `Cargo.lock` carry one dev-only line (`tracing-subscriber` for the
  test capture harness); everything else is test-only.

## 1. Two-tier run matrix

Harness: `cargo nextest run -p taskboard-sync-nextcloud --run-ignored
only -E 'test(<name>)'`. Bring-up per plan §6.1 (default tier
`taskboard-it-nextcloud` :8370; gate tier
`taskboard-it-nextcloud-deck1175` :8371 with
`EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175`). Both tiers left up at
handoff; never `down`ed.

| Tier | Test | Outcome | Duration |
|---|---|---|---|
| Default (Deck 1.19.1 / NC 35) | `it_nextcloud_docker_sync_app_field_convergence` (C1) | **PASS** | 2.7 s |
| Default | `it_nextcloud_docker_sync_rule_convergence` (C2) | **PASS** | 2.6 s |
| Default | `it_nextcloud_docker_sync_echo_idempotence` (C3) | PASS | 0.8 s |
| Default | `it_nextcloud_docker_attachment_content_lifecycle` | PASS | 1.4 s |
| Default | `it_nextcloud_docker_startdate_round_trip` | PASS | 0.7 s |
| Default | `it_nextcloud_docker_probe_dead_board_write_answers` (#282) | PASS | 0.4 s |
| Gate (Deck 1.17.5 / NC 33) | `it_nextcloud_docker_deck1175_scheduled_fallback_convergence` (C4) | **PASS** | 2.1 s |
| Gate | `it_nextcloud_docker_deck1175_attachment_content_lifecycle` | PASS | 2.4 s |
| Gate | `it_nextcloud_docker_deck1175_startdate_unsupported` | PASS | 0.9 s |
| Gate | `it_nextcloud_docker_deck1175_capabilities` | PASS | 0.1 s |
| Hermetic | `cargo nextest run -p taskboard-sync-nextcloud` | 289 passed, 26 skipped (the docker/live ignores) | ~2 s |

Honesty check: no ≈0.006 s rows — every docker row ran against its
instance (C1/C2 carry the new adopted-once server assertion; the gate
rows carry real server round-trips). C1/C2/C4 each ran **after** the
fix; the pre-fix RED state is pinned by commit 1's failing run
(reproduced `left: "taskboard-appdata"` vs `right:
"taskboard-appdata.json"` at tier 1).

## 2. #448 / #447 — the decode gap and the pins

- The measured 1.19.1 listing shape (tier records §F-A) is pinned
  verbatim at tier 1 (`attachments_list_1_19_1.json`, including the
  unknown `info` keys `dirname`/`extension` and the string `data`
  payload) and at the actor tier (wiremock S-pin `a34b`, full
  scan → confirm → download cycle, deterministic, no docker).
- The production fix is exactly plan §2: `ExtensionInfo.basename`
  (`#[serde(default)]`, `skip_serializing_if = "String::is_empty"` —
  serializations stay byte-identical) and `normalize_file_name` taking
  the first non-empty of `info.basename`, then `info.filename`, only
  when `fileName` is empty. Both decode paths are covered by
  construction; `anchor.rs` untouched.
- The 1.17.5 compatibility contract demonstrably holds: the existing
  echo pin (`echo_decode_normalizes_both_server_shapes`) passes
  verbatim — the recorded 1.17.5 echo has no `basename`, so rank 3
  fires exactly as before — plus three new contract cases (basename
  wins on the 1.19.1 pair; present `fileName` never overwritten; empty
  `basename` falls through).
- Production impact verified on the rig: C1/C2's new
  `assert_anchor_adopted_exactly_once` — exactly one anchor-titled
  card, exactly one reserved-filename attachment — holds on a fresh
  1.19.1 board (adopted, not re-created).

## 3. #449 — C4 triage (verdict: same root cause, fixed in this chunk)

Capture harness: `TASKBOARD_IT_TRACE=1 RUST_LOG=taskboard_sync_nextcloud=debug`
(the commit-6 test-only subscriber; trace files excerpted below,
reproduction command above).

**C4 passes on the gate tier (2.1 s) with the ranked fallback landed.**
The flush-ladder trace shows the write path was never the problem:

```
22:23:44.82 flush work gate verdict=true clause="group queued"
22:23:44.82 flush ladder resolved board=13 action=Create ladder=Create
22:23:44.89 flush ladder step landed board=13 card=21 step="create_card"
22:23:45.06 flush ladder step landed board=13 step="upload" stamped=true
22:23:45.06 flush verdict board=13 verdict=Applied { stamp: AttachmentStamp { .., filesize: 431 }, .. }
22:23:45.28 downloading deck attachment content board=13 attachment_id=8   (the waiter confirms)
```

No `Server(500)` on any ladder step; the scheduled lane armed on the
first post-write cycle (no gate-never-fires). The 2026-10-25 F-B
symptom (anchor never confirmed within the deadline) is explained by
the **same decode gap as #448**: the confirmation scan folds the
filename through the production decode path, and on 1.17.5 (as on
1.19.1) the confirmation-side shape loses the extension — with the
extension-less fold the waiter can never match, so the fixture waits
out its full deadline while the flush side looks idle-ish in DB
snapshots taken mid-window. Fixing `normalize_file_name` repaired C4
with no flush-path change; §4.3 outcome 3 territory (confirmation
decode), root cause already fixed by commit 2 — no additional
production diff needed.

**Not captured (bonus-evidence miss, recorded):** the real 1.17.5
*listing* attachment JSON. C4 (and every gate-tier fixture) tears its
board down on success, so the leftover-board capture hit 403; the
f-series lifecycle rows green on the gate tier stand as the behavioral
evidence that 1.17.5 attachment metadata decodes. The ranking is safe
by construction either way (plan §9 risk 1).

## 4. #282 — dead-board write answers (verified, no fix)

Probe (`it_nextcloud_docker_probe_dead_board_write_answers`, default
tier): create board + stack → typed `delete_board` → one write of each
queued-op kind against the dead board id, using the board's own
(deleted) stack. Measured answers:

| Write kind | Typed answer |
|---|---|
| `create_card` | `DeckError::Forbidden` (403) |
| `create_stack` | `DeckError::Forbidden` (403) |
| `create_label` | `DeckError::Forbidden` (403) |

Executor consequence (read off the existing `classify_push` table, no
code change): `Forbidden` classifies as **retryable strand** — only
`BadRequest` dead-letters — so queued stack/label ops on a
confirmed-dead board keep retrying each cycle. #282's drain-liveness
concern is confirmed on the rig; the write-side carve-out remains a
production change outside this chunk's scope ceiling (p3, unscheduled).
One incidental measured fact: with a *foreign live* stack id, Deck
1.19.1 answers a card create on a dead board with success (it resolves
the stack without board scoping) — the probe deliberately uses the
board's own dead stack. #282 stays open.

## 5. Deviations

1. **One extra commit beyond the plan's sequence** (`c4490e01`): the
   first green C1 run exposed a latent **fixture** bug — `set_field`
   mapped a clear (`value=None`) to `TaskChanges.effort = None`, which
   the engine reads as "no change", so the cleared-field write queued
   nothing and B's `is_none` wait hung the full deadline. The stage had
   never executed before (both prior sessions hung earlier at the
   #448 anchor wait). Test-only fix (`Some(None)`, mirroring the
   `scheduled` arm); no production diff. TDD note: the failing run was
   diagnosed via the commit-6 flush tracing (gate `idle` on every
   cycle after the fold stage — nothing queued), then localized with
   uncommitted stage markers (reverted before commit).
2. **C4 disposition is "fixed via the #448 fix" rather than a separate
   root cause**: the plan's decision tree assigned C4 its own
   candidates; the traces show the flush ladder clean and the
   confirmation path repaired by commit 2. Recorded above with the
   trace excerpt instead of a dedicated fix commit.
3. **1.17.5 listing JSON not captured** (board torn down before the
   leftover capture; 403). Bonus evidence only; see §3.
4. `anchor.rs` untouched — the plan predicted this; confirmed.

## 6. Quality gates

Via `scripts/run-quality-gates.sh` (shared lock), full AGENTS §10
sequence — **all green**:

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (also
  enforced by the pre-commit hook on every commit).
- `cargo nextest run --workspace`: **2681 passed, 41 skipped** (the
  docker/live ignores), 0 failed.
- `cargo test --doc --workspace`: ok.
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok.
- Coverage sequence: `llvm-cov nextest` 2681/2681 passed; quarantine
  reported all 3021 profraw files valid; lcov report written;
  `target/llvm-cov-target` removed afterwards (local hygiene).

## 7. Handoff to stage 3

- Branch `fix/anchor-filename-fallback` @ `d8901479`, worktree left in
  place; both docker tiers up (default :8370, gate :8371, env blocks
  reproducible via `scripts/nextcloud_it_setup.sh up` per §1).
- Stage 3 posts the tier records: PR (branch → `main`), evidence
  comments on #447 / #448 / #449 (close #447, #448, #449), a comment
  on #436 (not closed — closes on the lane re-run), and the #282 probe
  comment (issue stays open). No push, no merge from stage 2.
