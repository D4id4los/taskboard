# Stage-2 implementation report — typed scheduled-phrase direction (issue #458)

- **Date**: 2026-11-01
- **Chunk**: backlog round 2, chunk 1.4
- **Branch**: `refactor/scheduled-phrase-typed-direction`
  (worktree `../taskboard-WT-refactor-scheduled-phrase-typed-direction`)
- **Plan**: `.artifacts/plans/2026-10-31-scheduled-phrase-typed-direction-plan.md`
- **Commit**: `1f140329` — `test(ui): pin the scheduled-phrase direction
  as a typed partition`

## 1. What landed

Per plan §3/§4:

- Private two-variant `enum Direction { Future, Past }` next to
  `DurationBucket`, with the total mint `Direction::of(TimeDelta)` —
  hand-rolled, no derive crates, zero's `Future` leniency documented at
  the type (AGENTS §2 A1).
- `scheduled_phrase` consumes the mint: flat
  `match (Direction::of(delta), bucket)` absorbing the `Now` arm (the
  plan's §7 fallback shape — chosen over the nested match +
  `unreachable!`). The `Now` arm never consults the direction value,
  preserving the Cluster-A `<`/`<=` equivalence.
- Wording bytes unchanged: every format string is the literal it was.
- Tests: `direction_partition_is_typed` — one `matches!` assertion per
  direction per bucket class (Minutes/Hours/Days via ±2 units). Zero
  text assertions added; the pre-existing
  `scheduled_phrase_is_empty_when_absent_and_directional_when_set` was
  retained untouched.

## 2. Deviations from the plan

1. **One commit instead of two.** Plan §6 ordered a red/green commit
   for the pins before the refactor, but the pre-commit gate runs
   `clippy -D warnings`, which rejects the not-yet-consumed
   `Direction`/`of` as `dead_code` — the type-only commit cannot stand
   alone green. Type, pins, and rewiring landed as one commit; the TDD
   ordering (write pins, see the suite pass because the mint is the
   deliverable) was still followed inside the working tree.
2. **No `scheduled_direction` wrapper.** Plan §4's concrete shape
   floated `fn scheduled_direction(delta) -> Direction` as a test seam;
   since `Direction` and `of` are module-private and the tests reach
   them via `super::*` (which the plan itself prefers), the wrapper
   would be a pure pass-through. `Direction::of` is asserted directly.
3. **No `-j` cap on the mutant run.** Plan §5 said `-j 4`; per the #427
   sweep's recorded convention, cargo-mutants 27.1.0 rejects `--jobs`
   with `--in-place`, so the run was serial `--in-place
   --test-tool nextest` (identical to the CI lane's invocation).

## 3. Mutant evidence (plan §5 protocol)

Scoped run:

```sh
SQLX_OFFLINE=true cargo mutants -p taskboard-ui-slint \
  --file crates/taskboard-ui-slint/src/presentation.rs \
  --in-place --test-tool nextest
```

**Summary: 203 mutants tested in 49m — 155 caught, 13 missed, 35
unviable, 0 timeout.**

**Acceptance criterion met**: the direction flip
`presentation.rs:168:18 replace < with > in Direction::of` is
**CAUGHT** (as is the neighbouring `<` → `==` flip); the old
`scheduled_phrase` comparison site no longer exists. The surviving
`<` → `<=` at `Direction::of` — and at `due_phrase` — is the plan §2
recorded, provable-equivalent outcome, not a failure.

The other 11 misses are pre-existing gaps outside this chunk's scope
(`rule_list_hint`, `rule_created_phrase`, `recur_badge_label`,
`timer_rows`, `timer_chip`, `connector_row_status`,
`habit_target_phrase` — placeholder-return and boundary-comparison
mutants owned by the weekly lane / issue #427's follow-up); issue #458
named only the direction flip.

Post-run hygiene: the proptest regression file
`presentation.txt` was dirtied by the in-place run (expected, #427
convention) and restored via `git checkout --`; `git status` is clean
afterwards.

## 4. Quality gates

Run through `scripts/run-quality-gates.sh` from the worktree
(results recorded below in §5 of the handoff message):

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo nextest run --workspace` | pass |
| `cargo test --doc --workspace` | pass |
| `cargo deny check` | pass |
| llvm-cov three-command sequence + quarantine | pass |

No CHANGELOG entry: no user-observable change (wording bytes
identical). No push, no PR — handoff at branch-green per the dispatch
protocol; the user opens/merges the PR.
