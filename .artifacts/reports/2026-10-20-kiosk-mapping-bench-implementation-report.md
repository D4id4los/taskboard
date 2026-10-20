# [Report] Kiosk Wave K7 — DTO↔domain mapping bench, stage-2 implementation

- **Date**: 2026-10-20
- **Chunk**: kiosk implementation dispatch round, chunk 7
  (`feat/perf-mapping-bench`, worktree
  `../taskboard-WT-feat-perf-mapping-bench`, based on `main` @
  `ac77807`).
- **Stage-1 plan**: `.artifacts/plans/2026-10-20-kiosk-mapping-bench-plan.md`
  (committed on the branch as `b008810`).
- **Handoff state**: implemented, gates green, **no push, no PR** (per
  dispatch 7.2 — stage 3 owns push/PR with `Closes #263`).

## Commits (3, each standalone)

1. `001975e` — `test(sync-nextcloud): mapping bench fixture contract`
   (`benches/support/mod.rs` + `tests/mapping_bench_fixtures.rs`).
2. `14b7dd9` — `test(sync-nextcloud): mapping CPU-leg criterion
   benches` (`benches/mapping.rs` + the crate `Cargo.toml` dev section:
   `criterion.workspace = true` + `[[bench]] name = "mapping"
   harness = false`).
3. `b605073` — `docs(sync-nextcloud): record the mapping bench budget
   derivation` (the stage-1 plan §6 template, filled with the run's
   numbers).

Branch diff vs `ac77807`: exactly the four in-scope files, 815 insertions,
zero deletions. `git diff ac77807..HEAD -- .github/` is empty (D10
stage-2 check). No production file touched (D13 held; no STOP condition
fired).

## Measured numbers (dev machine, 2026-10-20; criterion defaults)

| Benchmark | Median | Note |
|---|---|---|
| `map_board/1` | 27.3 ns | |
| `map_stack/4` | 55.2 ns | |
| `map_label/6` | 123.9 ns | |
| `map_card/100` | 9.40 µs | |
| `map_card/1000` | 103.0 µs | |
| `map_card/10000` | 1.251 ms | hot arm |
| `new_card_from/100` | 4.68 µs | |
| `new_card_from/1000` | 51.7 µs | |
| `new_card_from/10000` | 712.9 µs | |
| `apply_card_shape/100` | 13.4 µs | |
| `apply_card_shape/1000` | 137.8 µs | |
| `apply_card_shape/10000` | 2.805 ms | |
| `board_changes_from/1` | 30.4 ns | |
| `stack_changes_from/4` | 47.8 ns | |
| `label_changes_from/6` | 149.7 ns | |
| `plan_pushes/steady/100` | 2.30 µs | |
| `plan_pushes/steady/1000` | 32.4 µs | |
| `plan_pushes/steady/10000` | 402.6 µs | |
| `plan_pushes/burst/100` | 25.8 µs | |
| `plan_pushes/burst/1000` | 397.1 µs | |
| `plan_pushes/burst/10000` | 5.382 ms | offline-session worst case |

All 21 planned benchmarks (stage-1 plan §3/§5) ran and reported.

### Composed budget-share (the S4 derivation, N = 10 000)

- pull ≈ 1.25 ms (`map_board` + `map_stack` + `map_label` + `map_card`×N)
- push steady ≈ 0.40 ms; burst worst case ≈ 5.38 ms
- write ≈ 3.52 ms (`new_card_from` + `apply_card_shape` + the three
  `*_changes_from`)
- composed ≈ 5.2 ms = **0.017%** of the 30 000 ms cycle budget
  (burst ≈ 10.2 ms = **0.034%**); under the audit's D2 10× derate the
  share stays ≤ 0.34% — a small fraction (S4 pass criterion). Dev-machine
  references only; the on-device verdict is the kiosk spike's.

## Gate results (AGENTS §10)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo nextest run --workspace` | 1116 passed, 28 skipped (incl. the 4 new contract tests and criterion smoke entries) |
| `cargo test --doc --workspace` | 6 passed, 0 failed |
| `cargo deny check` | advisories/bans/licenses/sources all ok |
| coverage (CI-identical 3-command sequence + quarantine) | pass (see below) |
| bench run | 21/21 groups report |

Zero-CI-diff verified (`git diff ac77807..HEAD -- .github/` empty).

## Deviations from the stage-1 plan

- **`support/mod.rs` is 390 lines vs the 200–240 estimate** (§12): the
  wire DTOs have no `Default`/`PartialEq`, so struct literals must spell
  every field (the full 19-field `Card`, 12-field `Board`, …). No
  convention deviation — just verbose literals. Total branch size 815
  lines vs the 430–540 estimate, same cause.
- **`MappingFixture` carries the four `BTreeMap`s instead of an
  `EntityTables` field**: the borrow-group struct cannot be stored
  inside its own referent; `benches/mapping.rs` constructs
  `EntityTables` from the public maps per `plan_pushes` leg (the
  pure-fn-direct rule unchanged).
- **The contract test's determinism check compares the wire family via
  `serde_json` value equality**: the wire DTOs derive neither
  `PartialEq` nor `Default`; serialized-value equality is the same
  structural fact. The domain family is compared by value.
- The budget note uses ASCII-safe `~`/`x` for `≈`/`×` in two list lines;
  the template's wording and every number are otherwise verbatim-filled
  per D9.

## Stage-3 reminders

- PR body must carry `Closes #263` and the numbers above (MD-5); zero
  `.github` diff re-check (`git diff origin/main...HEAD -- .github/`).
- Teardown after handoff:
  `git worktree remove --force ../taskboard-WT-feat-perf-mapping-bench`.

## Stage-3 review addendum (2026-10-20)

- **Rebase onto `origin/main` @ `b37acea` (chunk 6 landed) surfaced one
  semantic breakage**: the app-connector-registry work changed
  `Board.remote` to `Option<RemoteBoardBinding>` (connector-tagged);
  the rebase was textually clean but the bench fixture no longer
  compiled. Fixed in `benches/support/mod.rs`
  (`fixture_board` now builds
  `RemoteBoardBinding { connector: ConnectorId::from("nextcloud"),
  board: RemoteBoardId(77) }`) and folded into the fixture-contract
  commit. No production file touched (D13 still holds); `.github`
  diff still empty.
- **Doc-comment nits fixed in `benches/mapping.rs`** (folded into the
  budget-derivation commit): a missing closing paren in the skip
  note, and the budget-derivation heading reflowed to one line (the
  two-line `#` form rendered a bogus second rustdoc heading).
- **All §10 gates re-run green on the rebased branch** (fmt, clippy
  `-D warnings`, nextest 1202 passed / 29 skipped, doc tests, `cargo
  deny` all ok, CI-identical coverage sequence + quarantine, 1457
  profraws valid).
- **Bench reproduced (same day, same machine)**: all 21 groups report;
  N = 10 000 medians — pull ≈ 1.29 ms, steady ≈ 0.47 ms, burst ≈
  7.94 ms, write ≈ 5.0 ms; composed ≈ 6.7 ms (burst ≈ 14.2 ms) =
  0.022% (burst 0.047%) of the 30 000 ms budget, ≤ 0.47% under the
  10× derate. Consistent with the stage-2 table above within machine
  noise; the stage-2 numbers remain the recorded reference.
- **SonarCloud duplication-gate finding, resolved on the branch**: the
  quality gate failed on `new_duplicated_lines_density` 15.6% > 3% —
  CPD matching `benches/support/mod.rs` against the storage crate's
  `benches/support/mod.rs`. That duplication is stage-1 D2's mandate
  (conventions mirrored verbatim; bench targets cannot import another
  crate's support module), i.e. intentional by design. Fixed with a
  scoped `sonar.cpd.exclusions=**/benches/support/**` in
  `sonar-project.properties` (copy-paste detection only; not a
  `.github/` workflow diff, no other analysis behavior changed). This
  is a review-stage deviation from the plan's four-file diff scope
  (§1/D13), recorded here per the AGENTS §8 rule that a failing check
  is diagnosed and fixed on the branch, never bypassed. The fix landed
  as its own standalone commit (`ci(sonarcloud): exclude mirrored
  bench support modules from CPD`).
- Final branch: 5 commits, 815-line diff confined to the four
  in-scope files + `Cargo.lock` (criterion dev-dep entry) plus the
  `sonar-project.properties` exclusion above.
