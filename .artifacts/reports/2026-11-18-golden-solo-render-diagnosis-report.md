# P1 — #846 golden solo-fail diagnosis report (compressed chunk)

- **Date**: 2026-11-18
- **Chunk**: P1 of the tier-2 dispatch plan
  (`.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §2), compressed
  (dispatcher = planner + implementer). Plan:
  `.artifacts/plans/2026-11-18-golden-solo-render-plan.md`.
- **Issue**: #846 (`flaky-test`, `watchlist`) — single observation
  2026-11-15 on branch `fix/ux-r5-surface-friction`
  (`6610d6f0..027e4390`): `the_kiosk_board_golden_holds` failed
  filtered to itself, passed in the full-binary run.
- **Tree examined**: `main` @ `c7be6e46`, worktree
  `taskboard-WT-test-golden-solo-render`.

## Verdict

**Not reproducible on `main`; the render is deterministic with respect
to test selection. No fix lands — a speculative change would be
unjustified. The armed diagnosis below is recorded and the capture
policy is updated on the issue.**

## Evidence

All commands run in the worktree on `main` @ `c7be6e46`
(`taskboard-ui-slint` test profile binary,
`golden_visuals-bb495d8075d1b2b1`).

### 1. Reproduction matrix (dispatch plan §2 method)

| # | Invocation | Result |
|---|---|---|
| A | `cargo test -q -p taskboard-ui-slint --test golden_visuals the_kiosk_board_golden_holds` × 3 | 3× PASS (0.66–0.96 s) |
| B | `cargo test -q -p taskboard-ui-slint --test golden_visuals` × 3 | 3× PASS, 24/24 (13.7–15.6 s) |
| C | `cargo nextest run -p taskboard-ui-slint --test golden_visuals` | (not reached — A/B decisive, see below) |
| D | (as C, filtered) | — |

Configs C/D are process-per-test and therefore structurally identical
to A; A's repeated green covers them.

### 2. Render determinism w.r.t. test selection (the decisive experiment)

Captured the kiosk-board PNG through the sanctioned regen mechanism
(`TASKBOARD_REGEN_GOLDENS=1`, which writes the PNG then fails the run
by design; committed goldens restored byte-exact afterwards — worktree
verified clean):

- solo context (filtered run): → `solo-kiosk-board.png`
- suite context (full 24-test binary, same run mechanism):
  → `suite-kiosk-board.png`

`cmp solo suite` → **byte-identical** (20 304 B each). The capture is
byte-stable whether the test runs alone or inside the full binary.

### 3. Committed golden vs fresh captures

The committed `kiosk-board-0.5-1000.png` (20 315 B) differs from both
fresh captures at the PNG level (19 854 bytes) — different encoder
output, not different selection behavior. Pixel-level (PIL RGBA):

- 634 differing channel bytes of 3 600 000, max delta 136, in one
  small region (≈ x 264, y 864).

This is inside the comparison tier's tolerance budget by design
(`support::golden_compare` box-averaged 32×24 luma cells, per-cell
budget 8/255) — the compare passes, which is why the suite is green.
It records ordinary drift since the golden was committed, not a
selection-dependence.

### 4. Structural analysis (why the current tree cannot reproduce it)

- One process-global testing platform + one serialized driver worker
  per process (`tests/support/driver.rs:452-497`): every job is a
  whole mount→capture→drop, jobs never interleave.
- `mock_elapsed_time` is process-global, but within `golden_visuals`
  it only advances inside jobs (`driver.advance_time`), so a mounted
  window's dwell timer cannot be fired by another test.
- Every `advance_probe_clock` caller lives in a *different test
  binary* (probe tests) — separate processes, no interference with
  `golden_visuals`.
- The kiosk board capture never advances the clock past 0; the dwell
  timer (armed at mount, `src/adapter.rs:9195-9219`) cannot fire
  before the snapshot.

## Root-cause assessment of the 2026-11-15 observation

The observation tree (`fix/ux-r5-surface-friction` @
`6610d6f0..027e4390`) no longer exists; the branch's surface churn was
later merged/reworked. The evidence above shows the current pipeline
is selection-deterministic, so the observation most plausibly encoded
**branch-local state**: a mid-churn tree whose committed goldens were
stale relative to the branch's renderer/widget configuration, where
the solo regen captured the branch's (then-drifting) render and the
suite's tolerance-level compare absorbed it. That state is gone; the
single observation cannot be re-examined.

## Consequences

1. **No code change lands** — the determinism the dispatch plan asked
   for is already present and is now evidenced; the committed golden
   stays authoritative; no regen.
2. **#846 capture policy updated** (issue comment): on recurrence,
   record the drift report, both PNGs, the test-filter shape, *and*
   repeat the matrix A/B from §Evidence before any re-run; a recurrence
   on a current tree re-opens the full-protocol diagnosis with this
   report as the baseline.
3. The flaky-watchlist classification stays: single observation, not
   reproduced, armed diagnosis recorded.

## Scratch hygiene

All scratch captures live in `/var/tmp/p1-goldens/` (outside the
repo); the worktree goldens were restored byte-exact (`git status`
clean at every check); nothing scratch landed in the PR.
