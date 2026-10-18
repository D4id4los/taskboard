# Stage-2 Report — chunk 3, remediation-followups: engine exit reachability (#316)

- **Date**: 2026-10-18
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups`
- **Plan**: `.artifacts/plans/2026-10-18-engine-exit-reachability-plan.md`
  (implemented verbatim; deviations listed below)
- **Commit** (single, standalone, gates green at the pre-commit
  fmt/clippy hooks; referenced by subject — hashes churn on the
  per-chunk rebase discipline this shared branch follows):
  1. fix(state): hold the system emission seam weakly so the
     all-sources exit is reachable

## TDD red-run evidence (pre-change, unchanged seam)

Both new tests were written first and run against the untouched
production code (`cargo nextest run -p taskboard-state -E
'test(engine_exits_when_all_sources_close) |
test(storage_failed_is_silent_after_the_last_external_producer_dies)'`):

- `engine_exits_when_all_sources_close` — **FAIL**, panicked at
  engine.rs:1380: `the engine exits once every source closed: Elapsed(())`.
  Exactly the plan's predicted red: the loop's own strong
  `broadcast::Sender` keeps the system channel open, `system.recv()`
  never resolves, the join parks, the paused-clock 60 s deadline
  expires (in ~10 ms real time).
- `storage_failed_is_silent_after_the_last_external_producer_dies` —
  **FAIL**, panicked at engine.rs:1414:
  `assertion failed: matches!(events.try_recv(), Err(..TryRecvError::Closed))`.
  Exactly the plan's *intended* semantic-delta red: pre-change the
  loop's own strong sender delivers `StorageFailed` to the subscriber
  (`Ok(..)` instead of `Closed`).

No other test failed in the red run — the AGENTS §5 suspected-bug
protocol did not trigger.

## Post-change results

Both new tests green immediately after the seam swap (D3 shape:
`WeakSender` through `EngineParts`/`Seams`/`TickSeams`,
upgrade-per-send in `note_storage_failure`; D2 downgrade site inside
`spawn_state_engine`, public signature unchanged; D7 doc comments at
the module doc, the spawn doc, the downgrade site, and
`note_storage_failure`).

## Gate suite (AGENTS §10, all green)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo nextest run --workspace` | 1080 passed, 28 skipped |
| `cargo test --doc --workspace` | ok (0 failures) |
| `cargo deny check` | advisories/bans/licenses/sources ok |
| `cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report` | 1080 passed |
| `scripts/coverage-quarantine-profraw.sh target/llvm-cov-target` | quarantined 1 torn profraw, merge succeeded |
| `cargo llvm-cov report --lcov --output-path lcov.info` | report written |

Isolation runs (checklist step 4): `cargo nextest run -p
taskboard-state` (68/68) and `-p taskboard-sync-nextcloud` (211 passed,
17 skipped) — both harness families that spawn the engine stay green,
including a17 (seam delivery), the daemon-survival test, g3/g4 drains,
and the proptest suite (D6's system-closed-at-spawn analysis held: no
property failure appeared).

## Deviations from the plan

**None in substance.** Two notes:

1. The worktree was found with T1/T2 already present as *uncommitted*
   working-tree edits — byte-identical to the plan's §test-design
   listings (a prior stage-2 session evidently reached checklist step 2
   without committing). I verified the diff against the plan verbatim,
   then executed the prescribed red run on them rather than rewriting.
   The split-point rule (§risks 5) was honored: no red test was ever
   committed; the single commit contains tests + seam together.
2. This report artefact was committed as a separate `docs(report)`
   commit after the fix commit, matching chunk 2's stage-2 pattern; the
   fix commit itself stands alone green.

## Non-goals honored

No CHANGELOG edit (chunk 5 owns it), no drain-path or `AppState`
changes, no other crate touched, no push, no PR, `main` untouched.
