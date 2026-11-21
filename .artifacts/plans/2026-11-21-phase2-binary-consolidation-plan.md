// SPDX-License-Identifier: MIT OR Apache-2.0

# Chunk plan — S2b: the phase-2 test-binary consolidation (#1087)

- **Date**: 2026-11-21
- **Round**: backlog-reduction round 4, chunk S2b; branch
  `refactor/consolidate-state-test-binaries` (this plan's landing
  branch), based on `origin/main` @ `bee2cd46`.
- **Normative inputs**: scope report
  `.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`
  §B-S2 (S2b line); dispatch plan
  `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  §1/§4 (Prompt S2b.x); issue #1087; the phase-1 precedent:
  `.artifacts/plans/2026-11-19-consolidate-test-binaries-plan.md` (the
  map discipline, §3.3) and
  `.artifacts/reports/2026-11-19-consolidate-test-binaries-stage2-report.md`
  (the shared-module mechanics, the §3 function-level-bijection
  erratum); S2a's verdict
  `.artifacts/reports/2026-11-21-nextest-run-tuning-report.md` §5
  (verdict (i): the nextest run side is not binary-bound; phase-2
  proceeds under the standard green-run gate).
- **Acceptance**: the phase-2 crates' `cargo nextest list` identity at
  function level (round law (c) + the R4 §3 erratum discipline), the
  full gate green (`scripts/run-quality-gates.sh cargo nextest run
  --workspace`), and each merged target passing plain `cargo test
  --test <root>` wholesale (the R4 §4 method — the #1016/#1090
  class-tripwire).

## 1. Goal & laws

Finish what R4 §3.3 deliberately left out: merge the remaining
one-binary-per-file integration targets of the small non-slint crates
so the duplicated link work shrinks, under the same map discipline.
Binding laws:

- **(c) Zero test-case renames; zero function renames.** Member files
  keep their names verbatim and move verbatim; only binaries group.
  The nextest-string delta is exactly the inserted member-module
  segment (`engine::<fn>` → `state_engine::engine::<fn>`), inherent to
  grouping per the R4 §3 erratum; acceptance is a programmatic
  function-level bijection (canonical before == after, residual 0).
- **Tooling paths enumerated** (§4) are updated in the same change.
- **Gate**: pre-commit hooks + `scripts/run-quality-gates.sh cargo
  nextest run --workspace` (flock-safe; background + poll; commit all
  landed work before any wait). S2a's verdict (i) means no per-root
  run-side wall gate beyond the standard green run.
- **Diffs confined to**: test-file moves/merges, tooling-path/doc
  updates. No `[[test]]` Cargo.toml edits (verified: none of the five
  crates declares explicit test targets; subdirectory members are not
  auto-discovered).

## 2. Seams inventory (re-derived on `origin/main` @ `bee2cd46`)

- **taskboard-state** `crates/taskboard-state/tests/`: 13 top-level
  binaries (engine, habits, journal_degraded, manifest_routing,
  proptest, reflection, reminders, reminders_deadline, settings,
  snapshots, sqlite_smoke, tracking, undo), shared `tests/common/`,
  insta snapshots under `tests/snapshots/` (55 `.snap` files used only
  by `snapshots.rs`; their names embed the *binary* name —
  `snapshots__s1_persisted_state.snap` — so merging snapshots.rs would
  rename them all), proptest harness in `proptest.rs` with
  `tests/proptest.proptest-regressions` (SourceParallel persistence:
  keyed to the *source file's* directory and stem).
- **taskboard-storage-sqlite** `tests/`: 10 binaries (actor,
  bench_fixtures, board_adoption, contract, corrupt_row_matrix,
  fixture_hygiene, journal_storage, reflection_storage, roundtrip,
  schema), shared `tests/common/`; `bench_fixtures.rs` resolves the
  benches harness via `#[path = "../benches/support/mod.rs"]` (depth-
  sensitive); `roundtrip.rs` is the proptest+slow-tail binary
  (`roundtrip.proptest-regressions`, the 42.7 s
  `persisted_state_roundtrips_through_sqlite` wall member — S2a §2).
- **taskboard-sync-nextcloud** `tests/`: 7 binaries (actor_detach,
  actor_wiremock, contract, e2e_hermetic, it_docker, it_live,
  mapping_bench_fixtures), shared `tests/common/{mod,hermetic,
  sync_pair}.rs` + `tests/fixtures/deck/`; `it_docker`/`it_live` are
  the `#[ignore]` tier-2/3 wrappers over `common::suite` (skip-when-
  unconfigured, `#[ignore = …]` on every test); mapping_bench_fixtures
  uses the same `#[path "../benches/support/mod.rs"]` depth-sensitive
  resolution.
- **taskboard-sync-files-nextcloud** `tests/`: 4 binaries
  (client_wiremock, retained_etag_wiremock, it_docker, it_live), shared
  `tests/common/`.
- **taskboard-domain**: 1 binary (`vocabulary_guards.rs`) — unchanged;
  a single binary cannot consolidate.
- **CI/config surfaces** (all *name- or package-based*, so merging is
  behavior-neutral): `.config/nextest.toml` docker-tier override
  filters on `test(it_nextcloud_docker)` — test *names*, not binaries;
  `.github/workflows/ci.yml` (plain workspace run; the only `--test`
  reference is `mcp_smoke`, untouched by this chunk);
  `nextcloud-integration.yml` (`-p taskboard-sync-nextcloud
  --run-ignored only -E '<matrix filter>'`); `miri.yml` (`-p
  taskboard-state --lib --tests`); `mutation-testing.yml` (package
  list). No script under `scripts/` references a phase-2 test binary
  name. Doc-comment run instructions inside the moved files (`-E
  'test(it_nextcloud_docker)'`) survive verbatim.
- **No `env::set_var` / process-global once-locks in any phase-2 test
  file or their `common/` modules** (grepped 2026-11-21) — the merged
  roots carry no #1016-class process-env hazard by inspection; plain-
  cargo wholesale runs remain the tripwire.

## 3. The binary map (reviewable artifact)

### 3.1 taskboard-state: 13 → 4 targets

| New target (tests/<name>.rs) | Members (moved under tests/suite/) | Why grouped |
|---|---|---|
| `state_engine.rs` | `engine.rs`, `tracking.rs`, `undo.rs`, `journal_degraded.rs`, `manifest_routing.rs`, `settings.rs`, `sqlite_smoke.rs` | engine-core + persistence/config family; all `#[tokio::test]` + shared `common`; sqlite_smoke is the storage smoke leg of the engine story (`docs/testing_strategy.org` §579) |
| `state_features.rs` | `habits.rs`, `reminders.rs`, `reminders_deadline.rs`, `reflection.rs` | feature-family (habits/reminders/reflection engines) over the same harness |
| `proptest.rs` | **unchanged** | property harness; keeping it standalone keeps `proptest.proptest-regressions`'s SourceParallel position byte-stable and the randomized battery a separately-runnable unit |
| `snapshots.rs` | **unchanged** | insta snapshot names embed the binary name (`snapshots__*.snap`, 55 files); merging would rename every `.snap` — needless churn against law (c)'s spirit; same reasoning as R4's journeys.rs/pin_guard.rs keep-separate decisions |

### 3.2 taskboard-storage-sqlite: 10 → 2 targets

| New target | Members (moved under tests/suite/) | Why |
|---|---|---|
| `sqlite_storage.rs` | `actor.rs`, `board_adoption.rs`, `contract.rs`, `corrupt_row_matrix.rs`, `fixture_hygiene.rs`, `journal_storage.rs`, `reflection_storage.rs`, `schema.rs`, `bench_fixtures.rs` | repository-semantics family, one harness; bench_fixtures' contract pins are a member (its `#[path = "../benches/support/mod.rs"]` becomes `../../benches/…` inside the member — a one-line depth fix; only this member uses that harness, so no `duplicate_mod` hoist is needed) |
| `roundtrip.rs` | **unchanged** | the proptest+slow-tail binary (42.7 s member, S2a §2): keeping it separate preserves the `roundtrip.proptest-regressions` position, keeps the suite's longest test a distinct scheduling unit, and matches the R4 keep-separate reasoning for evidence-cited binaries |

### 3.3 taskboard-sync-nextcloud: 7 → 3 targets

| New target | Members (moved under tests/suite/) | Why |
|---|---|---|
| `nextcloud_wiremock.rs` | `actor_wiremock.rs`, `contract.rs`, `e2e_hermetic.rs`, `actor_detach.rs`, `mapping_bench_fixtures.rs` | the wiremock/hermetic API family (all drive the mock Deck server through `common`); mapping_bench_fixtures carries the same benches-support depth fix as §3.2 |
| `it_docker.rs` | **unchanged** | `#[ignore]` tier-2 suite; its doc comment names the `-p taskboard-sync-nextcloud … -E 'test(it_nextcloud_docker)'` invocation; keeping it standalone is zero-diff for `nextcloud-integration.yml` and the docker-tier serialization story |
| `it_live.rs` | **unchanged** | `#[ignore]` tier-3 (production credentials); manual-only lane stays distinct |

### 3.4 taskboard-sync-files-nextcloud: 4 → 2 targets

Recorded decision: **in scope** — issue #1087's body enumerates this
crate's 4 binaries among the remaining targets, and the method is
identical; the dispatch line's "state/sqlite/sync" reads as the
sync-crate family it names.

| New target | Members (moved under tests/suite/) | Why |
|---|---|---|
| `files_wiremock.rs` | `client_wiremock.rs`, `retained_etag_wiremock.rs` | the wiremock WebDAV client family |
| `it_docker.rs` / `it_live.rs` | **unchanged** | same tier-2/3 reasoning as §3.3 |

### 3.5 Post-map count

34 → 11 integration targets in the phase-2 crates
(state 13→4, sqlite 10→2, sync-nextcloud 7→3, sync-files-nextcloud
4→2); **workspace test-binary count 56 → ~33** (S2a's 56-binary
capture; domain 1 and all phase-1-landed roots untouched).

Mechanics (R4 §2 as landed): each new root is a thin file declaring
`#[path = "suite/<member>.rs"] mod <member>;` per member plus exactly
one `#[path = "common/mod.rs"] mod common;`; members consume it via
`use crate::common;` (edition-2024 uniform paths resolve the bare
`use common::…` against the root module — the R4 shared-module fix).
Member files `git mv` into `tests/suite/` (history preserved;
subdirectories are not auto-discovered as targets).

## 4. Tooling-path update list (same change, law c)

Stage 2 re-derives with a word-boundary `grep -rl` per old binary
name/path; the known classes:

1. **bound-test / gate-consumable lines**: the habit-parity UX plan's
   `bound-test: US-6/S1 -> crates/taskboard-state/tests/habits.rs::…`
   (`.artifacts/plans/2026-11-18-habit-parity-ux-plan.md:225`) —
   update the path segment to the new member location
   (`crates/taskboard-state/tests/state_features.rs::habits::…`).
   Any other `bound-test:`/§4 nomination line naming a moved path is
   updated identically (full list re-derived at stage 2).
2. **docs**: `docs/testing_strategy.org` §579 names `sqlite_smoke` as
   a binary — after the merge it is a member module; update the prose
   to name the merged root (member context kept). Any other normative
   `docs/*.org` citing a moved basename is updated the same way.
   ADR 0021's "module reflection.rs" cites `src/`, not `tests/` — no
   edit.
3. **CI/workflows/scripts**: no change (§2 — all filters are name- or
   package-based; `mcp_smoke` untouched; `.config/nextest.toml`
   untouched).
4. **In-file doc comments** inside moved members that show `cargo
   nextest run -p <crate> -E 'test(…)'` invocations: no edit needed
   (package + name filters).
5. **Historical `.artifacts` prose** (dated plans/reports narrating
   past evidence): not updated — the R4 §4 decision stands; only
   gate-consumable structures and normative docs move.

## 5. Identity verification method (the acceptance oracle)

1. **Before** (stage 2, pre-move): `cargo nextest list --workspace |
   sort` captured on the branch base (fresh worktree, not the main
   checkout — the R4 stale-capture lesson).
2. **After**: identical capture on the branch.
3. **Expected delta, exactly**: for each merged member, the nextest
   string gains the member-module segment (`<crate>::engine s1_foo` →
   `<crate>::state_engine engine::s1_foo`). Nothing else may change.
4. **Bijection proof**: programmatically strip the inserted member
   segment from the after-set, canonicalize, compare multisets —
   `before == canonicalized(after)`, residual 0. Also assert the
   untouched binaries (snapshots, proptest, roundtrip, both it_docker/
   it_live pairs, domain, all phase-1 roots) are byte-identical
   subsets. Recorded in the chunk report (the R4 §3 erratum
   discipline).
5. **Nextest filtering smoke**: `-E 'test(us6_s1)'`-class named-filter
   resolutions still resolve the same function names inside the merged
   roots (the member prefix does not affect `test()` substring
   matching — R4 §5 precedent).

## 6. Stage-2 procedure (summary)

1. Baselines: nextest-list capture (§5.1); plain `cargo test --test`
   wholesale status quo is not needed for the untouched binaries.
2. Apply the map: git mv + thin roots + the two `#[path]` depth fixes
   + §4 tooling updates.
3. Compile gate: `cargo clippy --workspace --all-targets -- -D
   warnings` (pre-commit, memory-gate routed) — the `duplicate_mod`
   tripwire from R4 §2 fires here if any shared module is declared
   twice.
4. Plain-cargo wholesale per merged root: `cargo test --test
   state_engine`, `state_features`, `sqlite_storage`,
   `nextcloud_wiremock`, `files_wiremock` — green required; a failure
   reproduces the #1016 triage path (is it pre-existing standalone?
   comment/issue, never fix opportunistically).
5. Identity proof (§5) + the full gate via
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe; background + poll; commit before waiting; defer per
   the [D] rule if the lock is held).
6. Report at `.artifacts/reports/2026-11-21-phase2-binary-consolidation-report.md`
   (stage 2/3 write; not pre-created by this plan).

## 7. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Plain-cargo wholesale interference inside a merged root (per-process globals we did not spot) | No `env::set_var`/once-locks found (§2); §6.4 wholesale run per root is the tripwire; residuals triaged per #1016, never masked |
| `duplicate_mod` if two members declare the same harness module | R4 mechanics: one root-level `mod common;` per merged root; clippy gate fires before commit |
| `#[path]` depth fix missed → compile error, not silent drift | Both sites (bench_fixtures, mapping_bench_fixtures) are enumerated here; compile is an early gate |
| insta/proptest persistence churn | snapshots.rs, proptest.rs, roundtrip.rs kept standalone by design (§3.1/§3.2) — zero `.snap`/`.proptest-regressions` movement |
| Larger merged binaries slow targeted rebuilds | The merged families are stable low-churn suites; the biggest single member (engine.rs, 4.4k lines) was already one binary; net link count drops by 23 |
| docker-tier filter silently matching nothing | Filter is test-name-based and names are untouched; `--no-tests=fail` in the workflow already guards it |
| nextest wall regression (the #1090 fear) | S2a verdict (i): the run side is not binary-bound (one-process-per-test packing); standard green run is the only run-side gate |

## 8. Open questions — resolved decisions

- **Merge snapshots.rs?** No — insta snapshot filenames embed the
  binary name; merging renames 55 `.snap` files for zero real link
  savings relative to the churn (§3.1).
- **Merge the proptest binaries?** No — SourceParallel regression-file
  positions stay byte-stable, and the randomized batteries remain
  separately addressable (§3.1/§3.2).
- **Merge roundtrip.rs (sqlite) into the storage family?** No — the
  suite's 42.7 s tail test and its regression file argue for keeping
  it a distinct unit (§3.2).
- **Merge it_docker/it_live suites?** No — `#[ignore]` tiers stay
  distinct binaries, mirroring R4's it_docker/mcp_smoke decision and
  keeping the tier invocations zero-diff (§3.3/§3.4).
- **Is sync-files-nextcloud in scope?** Yes — #1087's body enumerates
  it; the dispatch line's shorthand does not carve it out (§3.4).
- **domain's single binary?** Untouchable (nothing to merge) — no
  edit (§2).
- **Root/module naming**: descriptive, no `_tests` suffix (house
  style; R4 precedent): `state_engine`, `state_features`,
  `sqlite_storage`, `nextcloud_wiremock`, `files_wiremock`; member
  files keep their names verbatim under `tests/suite/`.
- **Historical .artifacts prose paths?** Not updated (§4.5, R4 §4).

## 9. Estimated diff size

- ~23 member files `git mv` into `tests/suite/` subdirs (verbatim
  bodies; per-member changes are the `use crate::common;` conversion
  and, for two files, one `#[path]` depth character).
- 5 thin root files (state_engine, state_features, sqlite_storage,
  nextcloud_wiremock, files_wiremock), ~15–25 lines each.
- ~2–5 tooling/doc lines (the bound-test path, testing_strategy §579,
  plus whatever the stage-2 grep re-derives).
- Total ≈ 30 files; substantive new content ≈ 5 thin roots; the
  nextest-list delta is pure member-prefix insertion.
