# [Report] Chunk 4 stage 2 — sidecar fold + flush + fences + rules + op routing + surfacing

- **Date**: 2026-10-22.
- **Branch**: `feat/sync-appdata-fold` in
  `../taskboard-WT-feat-sync-appdata-fold`, 12 commits on top of
  `origin/main` @ `73538b0` (stage-1 plan commit `1f2b112` first).
- **Plan**: `.artifacts/plans/2026-10-22-daily-driver-appdata-fold-plan.md`
  (stage 1, committed); every decision D1–D18 implemented as planned.

## Commit ladder (all standalone; each passed the full pre-commit gate)

| # | Commit | Content |
|---|---|---|
| 1 | `a046c87` feat(domain): the app-data fold primitives; feat(storage): the rule write clock rides the row | D1/D2/D8/D9 plan-commits 1+2 merged |
| 2 | `9317141` feat(domain): fold the sidecar into the sync report | D3–D6 plan-commit 3 |
| 3 | `adef0c3` feat(domain): enqueue the app-data lane | D7 plan-commit 4 (the MD-10 interim closes here) |
| 4 | `7dc497c` test(state): manifest assertion 4 | D17 plan-commit 5 (#244's closing artifact) |
| 5 | `b97a001` feat(domain): the document push group | D10 plan-commit 6 |
| 6 | `ea826fd` feat(sync): the flush between pull and report | D11/D12 plan-commit 7 |
| 7 | `1be5acd` test(sync): the D-S7 fence matrix | D13 plan-commit 8 |
| 8 | `f0e7ca3` feat(app): the sidecar health surfaces in status | D15 plan-commit 9 |
| 9 | `ab45001` feat(app): the anchor-title guard | D16 plan-commit 10 |
| 10 | `bebe231` test(sync): the tier-2 convergence suite | D18 plan-commit 11 |
| 11 | `c28ec1c` docs(changelog): the sidecar fold, flush, and rules section | plan-commit 12 |
| 12 | `fe8ad12` test(state): align the assertion-4 partition check with the landed planner | see Deviations D-3 |

## Gate results (AGENTS §10, the CI-identical sequence)

- `cargo fmt --all -- --check`: **green**.
- `cargo clippy --workspace --all-targets -- -D warnings`: **green**
  (also enforced by the repo's pre-commit hook on every one of the 12
  commits).
- `cargo nextest run --workspace`: **green** — 1497 run, 1497 passed,
  40 skipped (the `#[ignore]`-gated live tiers).
- `cargo test --doc --workspace`: **green** (1 doctest).
- `cargo deny check`: **green** (advisories/bans/licenses/sources ok).
- Coverage, the three-command CI-identical sequence:
  `cargo llvm-cov nextest --workspace --all-features --no-tests=warn
  --no-report` (1497/1497 passed) →
  `scripts/coverage-quarantine-profraw.sh target/llvm-cov-target`
  (quarantined 2 torn profraws from legitimately killed instrumented
  children, then merged successfully — the 2026-10-13 fix working as
  designed) → `cargo llvm-cov report --lcov` (written), trailing
  `rm -rf target/llvm-cov-target` executed (local hygiene).

## Tier-2 convergence outcomes (MD-5 tier honesty — recorded, not run)

No docker tier was configured in this session, so all four C-series
fixtures were **not executed**; they are `#[ignore]`-gated honest
skips exactly like the existing D-series (skip reason names the setup
script), and they compile inside the gate:

- C1 `it_nextcloud_docker_sync_app_field_convergence` — needs
  `nextcloud_it_setup.sh up` (Deck 1.19.0 tier). Not run.
- C2 `it_nextcloud_docker_sync_rule_convergence` — same tier. Not run.
- C3 `it_nextcloud_docker_sync_echo_idempotence` — same tier. Not run.
- C4 `it_nextcloud_docker_deck1175_scheduled_fallback_convergence` —
  needs the pinned Deck 1.17.5 gate tier. Not run.

Everything the C-series asserts end-to-end on the wire is covered at
tier 1 (wiremock) or in the pure suites: the flush's green paths and
all eight fence rows (F1–F8) ran green; the fold/guard/rules matrix
ran green in domain; the engine wiring pin ran green in state. The
acceptance sentence "app-field edit → UpdateAppData → flush → second
client folds it" is thus tier-1-proven; the tier-2 fixtures are the
recorded convergence evidence for a run with the tier up (the stage-3
reviewer should schedule one `nextcloud_it_setup.sh up` pass).

## Hard-rule compliance (spot audit)

- The two clock domains stay disjoint: `fold_task_app_data` takes no
  card-stamp input (non-readable by construction; pinned by the
  absence test).
- Absence never deletes: keyed absence writes nothing; rules need
  explicit tombstones; `derive_rule_tombstones` only ever tombstones
  base-*present* live rules.
- R3 stays snapshot-only: the fold returns nothing and touches no
  index.
- The guard copies values AND embedded clocks (D3's tier-1 test).
- The flush sits between pull and report and is a read-modify-write
  union (`union` + lane-GC + prune), never blind.
- Rebuild mode scopes ALL live board tasks with app-data.
- Fence comments amended, not deleted (both v1 wording sites).
- `UpdateTask` is never enqueued for rules; rule verbs enqueue only
  `UpdateAppData`.
- The CLI guard is app-layer (`CommandError::AnchorTaskReserved`; the
  engine has no such concept).
- Permanent 4xx dead-letters as counted `FailOp`s (F6/F8, plus the
  pipeline's existing BadRequest pin).
- Typed outcomes everywhere; no sleeps in tests (the actor harness
  advances paused time; the live suites poll under deadlines).
- Tier-2 runs recorded honestly (above).
- SPDX headers on the new `flush.rs` (MIT OR Apache-2.0, sync crate).

## Deviations from the plan (all flagged, none behavioral)

- **D-1 (plan commits 1+2 merged)**: `ReminderRule.updated_at` cannot
  compile standalone without the storage row decode (the
  workspace-wide pre-commit gate caught it), so plan-commits 1 and 2
  landed as one commit with both subjects. Content is exactly the two
  plans' scopes.
- **D-2 (`.sqlx` offline cache refresh)**: changing the
  `reminder_rules` load/upsert queries required regenerating
  `.sqlx/` (committed together with the query change, per the repo's
  rule). The migration-0004 upgrade *simulation test* needed its
  bookkeeping strip extended to version 7 — the re-created
  `reminder_rules` table would otherwise lack the column a real
  pre-0004 upgrade gets from 0007 running in order. Production
  migration files are untouched.
- **D-3 (assertion-4 partition check)**: the commit-5 complement
  check asserted an `UpdateAppData` op plans into no group — true
  when written (commit 6 had not landed), false after commit 6's
  coalescing. Fixed in the follow-up commit to pin the landed shape
  (exactly one group, `PushTarget::Document`). Caught by the final
  gate run, not by review — noted for honesty.
- **D-4 (insta pass scope)**: the deliberate snapshot pass covers the
  three widenings (`ReminderRule.updated_at`,
  `SyncStatus.{sidecar,features}`, `SyncReport::*.features`) — the
  persisted-state snapshots show the same three lines ×24 rows, plus
  nothing else (verified by diff inspection before `cargo insta
  accept`).
- **D-5 (state actor status rows)**: the Failed/Offline verdict rows
  (and `apply_actions`' `UpsertSyncStatus` arm) carry the memory-only
  `sidecar`/`features` fields over instead of writing defaults —
  required for D14's "the last verdict stands" to survive a batch
  replay; the storage codec still never persists them.

## Not done here (per the dispatch)

- No tracker edits (MD-4): #207/#244 close words belong to the PR
  body; #202 annotation is stage 3's.
- No push, no PR: the branch is 12 commits ahead of `origin/main`;
  the user opens the PR (`Closes #207` + `Closes #244` in the body).
- The worktree is kept until the PR lands (AGENTS §8's handoff-at-
  PR-green removal applies to the agent that opens it).
