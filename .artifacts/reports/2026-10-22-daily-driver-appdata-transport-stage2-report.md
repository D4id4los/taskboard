# [Report] Daily-Driver Chunk 3 — Sidecar Transport, Stage 2 (re-run) Implementation Report

- **Date**: 2026-10-22
- **Kind**: stage-2 implementation report (the dispatch protocol's
  implementation pass).
- **Origin**: the REVISED stage-1 plan
  `.artifacts/plans/2026-10-22-daily-driver-appdata-transport-plan.md`
  (revision 2, the version-capability re-run); branch
  `feat/sync-appdata-transport`, worktree
  `../taskboard-WT-feat-sync-appdata-transport`, base `aac5097`.
- **Status**: COMPLETE — every ladder rung (3–10) landed, the 1.17.5
  gate green on both tiers, all AGENTS §10 gates green. No push, no PR
  (per the dispatch order); handoff at "branch green", the user merges
  via PR after review.

## 1. Commits landed (this run)

| Commit | Ladder rung | Content |
|---|---|---|
| `e60d87b` | 3 | domain `capabilities.rs`: `DeckFeatures` + `flags_for` (D-V1 table; units + proptests) |
| `e30e5c9` | 4 | `Scope::Ocs` + `capabilities()` probe; `LoopState.features` + the once-per-lifetime probe; `new_card_from`/`apply_card_shape` flag gating + the `PushExecutor` parameter; the `push_exec` fixture sweep + the exact-count audit; wiremock `mount_capabilities` stub + a33 |
| `e195115` | 5 | unconditional multipart `data` field (D19); the `ExtendedData.info.filename` echo fallback at both decode paths (D20); the create-side `data` contract pin; the revised fixtures (`…_deck1175_startdate_unsupported`, the `capabilities_probe` pair) |
| `a701494` | (gate fix) | attachment stamp advancement observable by construction (see §4) |
| `faa0b0e` | 7 | domain `appdata.rs`: the versioned envelope + section registry codec, `scheduled` as the fourth modeled key, verbatim preservation (proptest), union + `prune_absent_cards`, reworded assertion 3 |
| `6549d94` | 8 | `AppDataEvidence`/`AppDataUnreadable` + `Completed.app_data`; `LocalOp::UpdateAppData(TaskId)` + forced perimeter (`owning_board`, `op_targets_task`, codec tag `update_app_data`, the `plan_pushes` totality arm, cascade filters, the `pending_task_intents` fence); the engine's `_` acknowledgment; zero insta drift (D15 held) |
| `34148c9` | 9 | sync `anchor.rs` (exact-title scan, confirmation, newest-stamp/lowest-id pick, action state machine, creation target, `build_evidence`) + the `Pulled` detail-attachment capture + the cycle's evidence decision table + `LoopState.documents` (success-only cache) + the S-series wiremock suite (a34–a40) |
| `8e7e90e` | 10 | CHANGELOG Unreleased line (capability seam + sidecar transport) |

Landed from the first run and riding this PR unchanged: `84ab03b`,
`332c89b`, `08562fa`, `14b0b20`, `000ea0b`, `0430d26`.

## 2. The 1.17.5 gate outcomes (both tiers, explicit)

Final-HEAD runs (`git checkout` state = `8e7e90e`), suites
`it_nextcloud_docker*` via `cargo nextest --run-ignored only`:

- **Default tier — NC 35.0.1 / Deck 1.19.0**: **14/14 PASSED**
  (run twice consecutively after the fixture fix, and a third time at
  final HEAD). Includes `it_nextcloud_docker_startdate_round_trip`
  (capability-true pin), `it_nextcloud_docker_capabilities` (probe
  decodes `"1.19.0"`, flag true), and the unchanged
  `attachment_content_lifecycle` — the D-V3 parity proof.
- **Gate tier — NC 33 / Deck 1.17.5** (`DECK_VERSION=v1.17.5`,
  `TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache`): **3/3 PASSED** (and a
  second time at final HEAD):
  - `it_nextcloud_docker_deck1175_startdate_unsupported` — the
    measured pin held: create with `startdate` ⇒ 200, reads back
    `None`; full-send PUT with a valued `startdate` ⇒ 200, still
    `None`. No divergence from characterization §2B.
  - `it_nextcloud_docker_deck1175_capabilities` — probe decodes
    `"1.17.5"`, derived flag `card_startdate = false`.
  - `it_nextcloud_docker_deck1175_attachment_content_lifecycle` —
    full lifecycle green unmodified once D19/D20 landed: the parity
    proof on the attachment surface (no validators, 409 duplicate,
    byte round-trip, in-place update, soft delete/restore).
- **Divergence count: zero.** Nothing STOP-worthy surfaced; the
  characterized behavior was reproduced exactly on both tiers.

CI note (D3, honest): the deck1175 tests run as *skips* in today's CI
(the `TASKBOARD_IT_DOCKER_DECK1175` env is not wired there); the gate
runs happened locally against the pinned instances documented above.
Wiring the pinned instance into CI remains a user decision (zero
workflow edits in this chunk, per D3).

## 3. Gate suite results (AGENTS §10, at `8e7e90e`)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (0 warnings) |
| `cargo nextest run --workspace` | 1426 passed, 0 failed, 36 skipped (env-guarded tiers) |
| `cargo test --doc --workspace` | 6 passed, 0 failed |
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok (one new dependency, `serde_json`, promoted from dev-dep to dependency of `taskboard-domain` for the codec's `Value` preservation — workspace-pinned, no new sources) |
| coverage three-command sequence | quarantine step dropped one torn profraw (the known killed-writer artifact) and merged; `lcov.info` written; `target/llvm-cov-target` removed afterwards |
| docker tier 2 (both variants) | 14/14 + 3/3 (§2) |

## 4. Deviations and findings (all recorded, none silent)

1. **Fixture observability fix (`a701494`)** — the first in-suite gate
   runs failed deterministically in
   `attachment_content_lifecycle`: with a warm server the whole write
   sequence lands inside the pre-write read's wall-clock second, and
   Deck's second-granular stamps advance by nothing any conditional
   read can observe (the strict `>` failed; re-presented validators
   legitimately 304'd). This is the roundtrip §7 same-second blind
   spot surfacing as a *fixture* time-of-day dependency, **not** a
   server-behavior divergence. Fix: poll the wall clock (under the
   tier deadline) until it leaves the pre-write second before the
   in-place update — observable by construction, no sleep-for-state;
   the strict `>` became `>=` with the measured *equality*
   (card stamp == attachment stamp) carrying the D1 §3.3 contract.
   Watchlist-worthy as a lesson for every fixture comparing
   second-granular stamps.
2. **`build_evidence` maps every download failure to
   `Unreadable(Transport)`** — the design's evidence table names only
   the transport row; HTTP-class download errors (e.g. a 500) travel
   under the same variant. Recorded here: if a future chunk needs the
   4xx/5xx split, that is an additive enum extension, out of chunk 3's
   scope.
3. **`serde_json` promoted to a real dependency of
   `taskboard-domain`** — the verbatim-preservation guarantee is
   `serde_json::Value`-based; the crate rule (pure, no IO/async) is
   unaffected. `cargo deny` green.
4. **Chunk-3 scope fence held**: no fold/flush/health/enqueue code, no
   scheduled emission/fold gating, no merge-verdict skip, no raw
   version comparisons outside `flags_for`, no codec capability
   branch, `ci.yml`/`nextcloud-integration.yml` untouched, no tracker
   edits (MD-4), `pending_task_intents` widened nothing (the fence
   comment names chunk 4). `pipeline.rs` saw only the forced
   match-arm totalities (D13's perimeter grew exactly the matches the
   new arm participates in: the delete/reset cascades and the R9
   outcome table — unreachable-in-production arms, each commented).
5. **`LoopState.features` consumers**: exactly the card-write
   conversion (D24 held); the report carries no flag field.
6. **Worktree left in place** (not at PR-green, so the removal rule
   did not fire); its `target/` has not been size-audited this run.

## 5. Test-count deltas worth review attention

- a2/a3/a10/a15/a16/a17/a26 exact-request-count assertions each
  absorbed exactly one probe GET (the D18 audit's +1, comment-marked).
- a33 pins the capability gating end-to-end (probe 1.19.0 ⇒ create
  carries `startdate`; unstubbed 404 probe ⇒ it never touches the
  wire).
- a34–a40 pin the evidence table: Document travel, Absent silence,
  each `Unreadable` variant, download-failure completes the cycle,
  the once-only download cache, the single-winner pick, and the
  no-normalization title discipline.
