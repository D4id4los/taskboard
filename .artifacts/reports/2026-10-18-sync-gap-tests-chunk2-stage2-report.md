# Stage-2 Report — chunk 2, remediation-followups: sync actor/executor test gaps (#242)

- **Date**: 2026-10-18
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups`
- **Plan**: `.artifacts/plans/2026-10-18-sync-gap-tests-plan.md`
  (implemented verbatim; deviations listed below)
- **Commits** (each standalone, gates green at each commit's pre-commit
  fmt/clippy hooks):
  1. `e8b710e` test(domain): add a fail-reads seam to the in-memory fake
  2. `eb6fbab` test(sync): assert pulled board labels reach the snapshot (F6-04)
  3. `f733546` test(sync): pin the board-switch cache clear (F6-05)
  4. `0fa38ee` test(sync): pin the absorber switch, read backoff, tombstone title
  5. `74f80c0` test(sync): pin the archived-twin dedupe and flag adoption
  6. `70c4e36` test(sync): pin post-create failure discrimination

## Gap-table outcome

| Finding | New test(s) | Tier / file | Result |
|---|---|---|---|
| F6-04 pulled labels unasserted | `a23_pulled_board_labels_reach_the_snapshot` | hermetic wiremock, `tests/actor_wiremock.rs` | landed, green |
| F6-05 board-switch cache bleed | `a24_board_switch_serves_the_new_board_after_the_cache_clear` (A→B→A, 7-request final cycle + no-beta-bleed stacks assertion) | hermetic wiremock | landed, green |
| F6-06a absorber `SetBoard`-during-cycle | `a25_set_board_during_a_cycle_is_absorbed_into_one_switch` | hermetic wiremock | landed, green |
| F6-06b read-failure streak | domain fail-reads seam + routing-contract test + `a26_unreadable_state_backs_off_then_recovers` | `test_support/memory.rs` + hermetic wiremock | landed, green |
| F6-06c `bound_board` tombstone identity | `a27_unlisted_board_tombstone_keeps_the_local_title` | hermetic wiremock | landed, green |
| F6-23 archived dedupe + flag-stamp | `a28_archived_twin_dedupes_and_the_flag_is_adopted_with_the_observed_stamp` (`.expect(1)` + `server.verify()` pin the one-detail-fetch rule for bound vs unbound cards) | hermetic wiremock | landed, green |
| F6-24 create-path transport/rejection discrimination | `post_create_archive_transport_failure_aborts_with_the_create_as_landed` + `post_create_archive_rejection_is_tolerated_without_aborting` | crate unit suite, `src/push_exec/tests.rs` (InstantSleeper, real-time-free) | landed, green |

Count: 7 sync tests + 1 domain routing test, per recorded decision D8
(the dispatch's "five" read as a floor; per-finding shapes from the
findings index).

## Production-bug discovery

**None.** Every new test passed against the as-built production code on
first run. In particular a26's silence-window premise held exactly: the
read failure produced zero HTTP, zero broadcasts, and zero reports, and
the recovered backoff cycle completed. No AGENTS §5 protocol
triggered; no new issues needed.

## Per-gate results (full suite, after last commit)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo nextest run --workspace` | 1078 passed, 0 failed (28 skipped: docker/live tiers) |
| `cargo test --doc --workspace` | all 6 suites ok, 0 failed |
| `cargo deny check` | advisories/bans/licenses/sources ok |
| `cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report` | 1078 passed |
| `scripts/coverage-quarantine-profraw.sh target/llvm-cov-target` | all 3790 profraw files valid |
| `cargo llvm-cov report --lcov --output-path lcov.info` | report written |

## Deviations from the plan

All cosmetic; the test shapes are the plan's:

- **F6-24 `Mock::expect` has no message parameter** in wiremock 0.6 —
  the plan's inline rationale strings were dropped (the assertions and
  comments carry the intent).
- **`PushExecutor::execute` takes `&[PendingOp]`** — the unit pair
  passes `&[op(..)]` instead of an owned `Vec`.
- **a28's final assertion message** reworded to avoid a brace-only
  format-string (`{7, 99}` is not a valid inline format arg); the
  assertion itself is unchanged (sorted stack-id vector equality).
- **a24's request-count observable** is asserted as the *delta* around
  the third cycle (7 requests) rather than an absolute total — same
  fact, robust to unrelated earlier traffic.
- **clippy-driven nudges**: `a24`'s two-board responder closure is
  `Copy` (no `.clone()` in the mount loop); a28 carries
  `#[allow(clippy::too_many_lines)]` (fixture mounting is the scenario,
  the A18/A22 precedent).

## Non-goals honored

No production changes (the only production-adjacent diff is the
`#[cfg(any(test, feature = "test-support"))]` fail-reads flag per the
plan's §domain-flag); no e2e/docker/bench additions; no
`tests/common/` churn (the two-board mount lives in-file); no
CHANGELOG.md edits; no push, no PR, no main contact.
