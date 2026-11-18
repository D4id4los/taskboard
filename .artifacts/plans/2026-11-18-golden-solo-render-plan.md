# Chunk P1 — #846 golden solo-fail diagnosis & determinism fix — Compressed Plan

- **Date**: 2026-11-18
- **Kind**: compressed chunk (dispatch plan §2) — the dispatcher is
  planner **and** implementer; no separate stage-1/2/3 dispatch. Small
  diagnosis with a bounded fix; PR open + `gh pr checks` green is the
  handoff.
- **Branch**: `test/golden-solo-render`; worktree
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-test-golden-solo-render`.
- **Issue**: #846 (`flaky-test` + `watchlist`): `the_kiosk_board_golden_holds`
  FAILED filtered to itself (plain and `TASKBOARD_REGEN_GOLDENS=1`) and
  PASSED in the full-binary run, same tree/goldens; the solo regen
  produced a PNG that compared differently. Single observation,
  2026-11-15, branch `fix/ux-r5-surface-friction`.

## Diagnosis method (recorded before the fix)

Facts established from the code on `main` @ c7be6e46:

1. **One process-global testing platform + driver worker per process**
   (`tests/support/driver.rs:452-497`, `static SENDER`/`OPTIONS`).
   `mock_elapsed_time` is the testing backend's **process-global,
   monotonic** clock.
2. `the_kiosk_board_golden_holds` mounts through `kiosk_rotation_settings()`
   (`tests/golden_visuals.rs:340-356`): a `KioskRotation` [Board, Today]
   with a 1 s dwell. The mount arms a **single-shot dwell timer at
   mount** (`src/adapter.rs:9195-9219`); the fire closure ticks the
   rotation to the next mode.
3. The board capture never advances the mock clock past 0
   (`capture_golden`'s `advance_time(0)`). So the captured mode can
   only flip if the **global** mock clock moves ≥ dwell after mount
   but before snapshot — impossible solo, possible only via
   cross-test interference in a one-process threaded run.
4. Runner arithmetic: under `cargo nextest` every test is its own
   process — solo and full are then *identical* configurations, so an
   observed solo/full divergence implies the 2026-11-15 runs were
   libtest (`cargo test`, threaded, one process, 24 tests). Under
   libtest the accumulated elapsed at each test's mount and the job
   ordering are nondeterministic.

Reproduction matrix (each cell evidence-carried, repeated per the
acceptance criteria):

| # | Invocation | Configuration |
|---|---|---|
| A | `cargo test -p taskboard-ui-slint --test golden_visuals the_kiosk_board_golden_holds` | solo, one process, one test |
| B | `cargo test -p taskboard-ui-slint --test golden_visuals` | full, one process, threaded, 24 tests |
| C | `cargo nextest run -p taskboard-ui-slint --test golden_visuals` | full, process-per-test |
| D | `cargo nextest run -p taskboard-ui-slint --test golden_visuals the_kiosk_board_golden_holds` | solo, process-per-test |

Interpretation: if A fails while B/C/D pass, the solo-process
first-test state (font warm-up / platform init ordering) is the
variable. If B is the flaky one (interleaving flips the dwell timer),
the committed golden was regenerated under an interference render —
a different (worse) root cause: the golden would be pinned to
cross-test clock bleed. The fix differs by branch:

- **Solo-state branch**: make the capture independent of
  process-early state (e.g. normalize the mock clock at job start —
  the driver resets `mock_elapsed_time` deterministically, or the
  kiosk settings pin the elapsed base) so solo and full render
  identically. Committed golden stays authoritative.
- **Interference branch**: the determinism fix is at the driver level
  — the mock clock must not advance a mounted window's timers from
  another test's job (a per-job clock expectation or a driver-level
  guard); regen under the fixed regime if the committed PNG encodes
  the interference render.

No product-code change on either branch (test-rig fix only); the
committed golden stays authoritative unless the evidence proves it
encodes suite-only state — then regen + determinism fix land
together, both evidence-carried.

## Timing note (from the dispatch plan)

r9's W-chunks churn widget surfaces. If the fix must touch a golden
PNG that a W-chunk is concurrently regenerating, the regen half is
deferred to post-W1 and the determinism fix lands alone. (Current
state: no W-chunk branches are in flight — verified 2026-11-18.)

## Acceptance (dispatch plan §2, binding)

- Solo + full runs both green, repeated.
- Root cause + why suite-order masked it recorded here and in the
  final report.
- No product-code change.
- #846 closes on the evidence, or carries the armed diagnosis if the
  root cause resists the compressed pass (escalate to full protocol
  then — honest not-fixed-yet beats a speculative fix).

## Status log

- 2026-11-18: plan written; reproduction starting.
