// SPDX-License-Identifier: MIT OR Apache-2.0

# Chunk report — S2b: the phase-2 test-binary consolidation

- **Date**: 2026-11-21
- **Chunk**: backlog-reduction round 4, S2b; branch
  `refactor/consolidate-state-test-binaries`, plan
  `.artifacts/plans/2026-11-21-phase2-binary-consolidation-plan.md`
  (commit ce6926f6 pre-rebase), issue #1087.
- **Stage**: 3 (review + ship). Stage-2 work landed as 5 item commits;
  reviewed post-rebase onto `origin/main` (the S2a docs commits —
  rebase was plan/report text only, zero test-file content).

## 1. Verdict

**Ship.** The map in plan §3 was applied exactly; every trap check
passed; no code-quality findings required branch changes.

## 2. Map outcome (plan §3 ↔ landed diff)

34 → 11 integration targets in the phase-2 crates, verified against
the plan's tables:

| Crate | Before | After | Landed roots |
|---|---|---|---|
| taskboard-state | 13 | 4 | `state_engine.rs` (7 members), `state_features.rs` (4), `proptest.rs`/`snapshots.rs` untouched |
| taskboard-storage-sqlite | 10 | 2 | `sqlite_storage.rs` (9 members), `roundtrip.rs` untouched |
| taskboard-sync-nextcloud | 7 | 3 | `nextcloud_wiremock.rs` (5), `it_docker.rs`/`it_live.rs` untouched |
| taskboard-sync-files-nextcloud | 4 | 2 | `files_wiremock.rs` (2), `it_docker.rs`/`it_live.rs` untouched |

`git diff-tree -r -M` rename status: 23 member moves, 13 at R100
(verbatim), 10 at R098–R099 whose entire delta is the plan-enumerated
conversions — verified per-blob:

- `mod common;` → `use crate::common;` (the R4 shared-module fix);
- `use common::…` → `use crate::common::…`;
- exactly two `#[path = "../benches/support/mod.rs"]` →
  `../../benches/…` depth fixes (`suite/bench_fixtures.rs` in
  storage-sqlite, `suite/mapping_bench_fixtures.rs` in
  sync-nextcloud) — the two sites plan §3.2/§3.3 enumerate.

No member gained, lost, or renamed a test function; the five thin
roots declare exactly the plan's member lists plus one `mod common;`
each (files_wiremock declares none — its members never used the
shared harness, verbatim-confirmed). No `[[test]]` Cargo.toml edits;
no crate dependency changed (AGENTS §2 boundary untouched — the diff
touches only `tests/`, docs, and one CI comment).

## 3. Bijection evidence (plan §5 oracle)

Stage-2 captures `/tmp/s2b/before.txt` (4899 lines, base bee2cd46)
and `/tmp/s2b/after.txt` (4897 lines, branch tip), re-checked by the
review agent programmatically:

- Canonicalization (strip the inserted member-module segment, map the
  root binary back to the member name) yields before 825 =
  canonicalized-after 823 in the four phase-2 crates.
- **Residual, exactly the predicted class**: the
  `suite::version_floor` test dedup —
  `decodes_with_patch_leniency_and_compares` existed once per binary
  in actor_wiremock/contract/e2e_hermetic (3 copies) and exists once
  in `nextcloud_wiremock` (1 copy): 3→1, inherent to grouping per the
  R4 §3 erratum, zero renames. No other before-only or after-only
  entry.
- **Untouched-binary subset check**: `taskboard-state::snapshots`
  (45), `taskboard-state::proptest` (3),
  `taskboard-storage-sqlite::roundtrip` (49),
  `taskboard-sync-nextcloud::it_docker` (1), `…::it_live` (1),
  both sync-files it_docker/it_live (0 listed, `#[ignore]`d),
  `taskboard-domain::vocabulary_guards` (3) — byte-identical
  nextest strings and identical counts before/after.
- Named-filter smoke (plan §5.5): the bound-test line's function
  `us6_s1_update_habit_stores_the_day_inside_the_canonical_window`
  resolves identically inside `state_features.rs::habits::` —
  `test()` substring matching is prefix-insensitive (R4 §5
  precedent).

## 4. Tripwires

- **Plain-cargo wholesale per merged root**: green for
  `state_engine`, `state_features`, `sqlite_storage`,
  `nextcloud_wiremock`, `files_wiremock` (stage 2; no #1016-class
  interference — plan §2's no-`env::set_var` inspection held).
- **Full gate**: dispatcher ran the §10 suite green on the landed
  state; the review agent re-ran
  `scripts/run-quality-gates.sh cargo nextest run --workspace` after
  the rebase (no fixes were needed; the rebase added docs-only
  commits) — green.
- **files_wiremock flake triage**: the `retained_etag_wiremock` class
  is **pre-existing on main**, not introduced by the merge — the
  member file moved at R100 (byte-verbatim), so the merge changed
  nothing about its scheduling environment beyond the binary rename.
  Six observations were filed as watchlist issues, one per distinct
  test, per the flaky-test discipline: **#1165, #1166, #1167, #1168,
  #1169, #1170** (cross-test wiremock journal pollution under
  in-process parallel runs). No opportunistic fix rode this branch;
  triage belongs to the dedicated flaky-test pass.

## 5. Tooling updates (plan §4)

Exactly the re-derived set, no more:

1. `docs/testing_strategy.org`: §579's `sqlite_smoke` binary line →
   `state_engine`'s `sqlite_smoke` member; §122's `tests/reminders.rs`
   path → `tests/state_features.rs`'s `reminders` member (grep
   re-derivation).
2. `.artifacts/plans/2026-11-18-habit-parity-ux-plan.md:225`:
   `bound-test:` path →
   `crates/taskboard-state/tests/state_features.rs::habits::…`.
3. `.github/workflows/ci.yml:197`: the bench-target comment's
   `tests/bench_fixtures.rs` path → `tests/suite/bench_fixtures.rs`.
4. No scripts/, `.config/nextest.toml`, or other workflow edits —
   verified by a word-boundary sweep over `docs/ .github/ .config/
   scripts/` for every old binary basename: zero stale references.

## 6. Deviations

None from the plan. Notes:

- The `ci.yml` comment path fix is beyond the plan's literal §4 list
  but inside its "whatever the stage-2 grep re-derives" clause (it is
  a gate-consumable comment naming a moved path).
- `sync-files-nextcloud`'s wiremock members never consumed
  `tests/common/`, so `files_wiremock.rs` correctly declares no
  `mod common;` — the plan's §3 mechanics text anticipated one root
  declaration per merged root; the root documents why it has none.

## 7. Code-quality review (stage-3 mandate)

- Thin roots: descriptive names, no `_tests` suffix; doc comments
  state constraints (why untouched binaries stay standalone, why the
  harness is declared once) rather than narrating the diff —
  conforming.
- No dead code, no speculative generality; the repeated
  harness-declaration comment is the load-bearing constraint, kept
  to four lines.
- Crate-role boundaries (AGENTS §2): no dependency graph change; all
  member files stay inside their crate's `tests/`.
