# [Report] View-pinned re-measure — Today and Board draw lanes (#460), with the board i16 render-ceiling crash (#462)

- **Date**: 2026-10-26 (clean run 04:04–04:16 CET)
- **Branch**: `fix/bench-view-pinned-lanes` (off `origin/main` =
  `a1bc03d`): `3d0bc9d` (the instrument fix: view pins, rendered-flip
  targets, visible-flip guard) + `c987ef0` (the board i16-ceiling
  skip). This report is the third commit.
- **Machine**: dev box (Ryzen 9 5900HX, 30 GiB, rustc 1.99.0, slint
  1.18.1, software renderer, 1366×768). Clean run's load: 0.66 / 1.35
  / 2.49 — quiet (an earlier attempt at load 10.13 was aborted by the
  §5 crash and is used only as that crash's evidence, never for
  times). Every time below is a **dev-machine reference, never a
  gate** — the on-device verdict belongs to the kiosk wave; the
  100 ms repaint cadence (C2/F13-09) floors composed latency
  regardless. Regions are deterministic and load-independent.
- **Outcome (one line)**: the lanes now measure the scene they name —
  and both named scenes fail budget row 3's partial leg at scale on
  the dev reference machine: the **Today** pin's one-card change
  (an overdue-list removal) costs 8.95 / 88.0 / 903 ms at
  N = 100 / 1 000 / 10 000 — the recorded partial numbers were
  mixed-population underestimates — and the **Board** pin, measured
  here for the first time, shows the same inversion the Today view
  had (partial 14.2 ms vs full-frame 6.0 ms at N = 1 000) **and a
  hard crash above it**: the eager board's columns exceed the
  software renderer's i16 pixel ceiling at ~550-650 cards/column, so
  board N = 10 000 does not render at all (#462).

## 1. What changed in the instrument (3d0bc9d + c987ef0)

The design-round probe
(`.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`
§1) resolved #390's 0×0 anomaly: the draw lanes measured whatever view
`mount` seeds — with default settings the **Today** view, unlabelled —
where most fixture flips render nowhere (honest 0×0) and the overdue
flips are filtered-list **removals**. The fix:

- **View pins**: `draw-full/{today|board}/{n}` and
  `draw-partial/{today|board}/{n}`, via the mount's own
  `UiSettings.kiosk` lever (`adapter.rs:2016`: kiosk → Board,
  otherwise Today). `refresh/{n}` stays unpinned — the pass derives
  and pushes every view's models regardless of which is mounted.
- **Rendered flip targets**: the partial lane rotates over the pinned
  view's rendered head rows (`support::today_flip_targets` — the
  Overdue list's head; `support::board_flip_targets` — the first rows
  of every column), so a "guaranteed one-row change" is guaranteed to
  render.
- **The visible-flip guard**: a preamble flip asserts a nonzero
  region, once per lane — a fixture or mount drift that leaves the
  flip unrendered panics instead of silently measuring nothing. Every
  guard passed on this run (Today: 1342×688; Board: 1334×612).
- **The board i16-ceiling skip** (`c987ef0`): board cells whose
  columns exceed a documented 400-rows/column safe bound are skipped
  with a loud eprintln naming the finding (§5) — never silently
  measured, never allowed to panic the CI bench job.

Region-carrier semantics (unchanged carrier, corrected meaning): the
max is the honest worst case; the last iteration rides along for
warm-up separation; per-iteration readings are meaningful only for
rendered changes — which the guard now underwrites.

## 2. The numbers (criterion medians [range]; quiet box)

| Lane | N=100 | N=1 000 | N=10 000 |
|---|---|---|---|
| `refresh` (unpinned) | 56.6 µs | 774 µs | 20.0 ms |
| `draw-full/today` | 1.367 ms | 2.079 ms | 19.945 ms |
| `draw-full/board` | 3.341 ms | 5.960 ms | **SKIPPED** (#462) |
| `draw-partial/today` | 8.953 ms | 88.003 ms | 903.19 ms |
| `draw-partial/board` | 3.165 ms | 14.177 ms | **SKIPPED** (#462) |

Region lines, verbatim (guard / max / last-iteration):

```
[refresh_draw] draw-partial/today/100: visible-flip guard region 1342×688
[refresh_draw] draw-partial/today/100: region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/today/100: last-iteration region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/today/1000: visible-flip guard region 1342×688
[refresh_draw] draw-partial/today/1000: region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/today/1000: last-iteration region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/today/10000: visible-flip guard region 1342×688
[refresh_draw] draw-partial/today/10000: region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/today/10000: last-iteration region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/board/100: visible-flip guard region 1334×612
[refresh_draw] draw-partial/board/100: region 1342×656 (880352 px = 83% of the full frame)
[refresh_draw] draw-partial/board/100: last-iteration region 306×440 (134640 px = 12% of the full frame)
[refresh_draw] draw-partial/board/1000: visible-flip guard region 1334×612
[refresh_draw] draw-partial/board/1000: region 1342×656 (880352 px = 83% of the full frame)
[refresh_draw] draw-partial/board/1000: last-iteration region 1342×656 (880352 px = 83% of the full frame)
[refresh_draw] draw-full/board/10000: SKIPPED — the eager board's columns exceed the software renderer's i16 pixel ceiling (400 rows/column safe bound; 500 rows/column renders, 650 panics in BasicBorderRectangle::render — recorded finding, not a measurement)
[refresh_draw] draw-partial/board/10000: SKIPPED — the eager board's columns exceed the software renderer's i16 pixel ceiling (400 rows/column safe bound; see draw-full/board's skip note)
```

## 3. Reading

1. **Sanity: the Today pin reproduces the recorded draw-full.**
   1.367 / 2.079 / 19.945 ms vs the un-pinned lane's 1.386 / 2.324 /
   22.55 ms (`2026-10-25-refresh-draw-true-partial-remeasure-report.md`
   §2) — within run-to-run drift. The pin didn't change the scene it
   already measured; it named it.
2. **The recorded partial numbers were underestimates; the corrected
   Today costs are higher.** `draw-partial/today` = 8.95 / 88.0 /
   903 ms vs the instrument-fix report's 1.57 / 56.2 / 566.5 ms.
   The old lane rotated over **all** N tasks: ~2/3 of its iterations
   were invisible flips (the 0×0 anomaly), and criterion's estimates
   mixed the two populations — the "1.57 ms at N=100" median was the
   invisible class, not the removal. The guard lane measures only
   rendered changes, and the design-round probe's raw E1 readings
   (removal renders 7.6-14.7 ms at N=100, 98-141 ms at N=1 000)
   bracket the new numbers exactly. The STILL-DIRTY verdict stands
   with corrected magnitudes: a one-card change dirties a constant
   1342×688 (88 %, max = last at every N — the removal reflows the
   whole list extent, position-independent).
3. **The Board pin is ~2.4-2.9× the Today pin at equal N on the full
   lane** (3.34 vs 1.37 ms; 5.96 vs 2.08 ms) — same fixture, same
   card count classes (4 000 board cards vs 3 334 Today rows at
   N = 1 000), so the board's per-card scene structure (DragArea +
   DropArea wrappers, nested column layouts, two-level index seams)
   carries real cost, not just card count. First numbers ever
   recorded for the board view.
4. **The #390 inversion reproduces on the board.** At N = 1 000 the
   board's partial (14.18 ms, one in-place card change) costs
   **2.4× its full-frame render** (5.96 ms) — the walk over all
   ~N live items dominates the paint, exactly the residual term the
   design-round analysis named for #373, now with board-pinned
   evidence (its actual subject). At N = 100 the two lanes are
   comparable (3.17 vs 3.34 ms).
5. **The board's damage region is bimodal** — a first for this
   markup family: 306×440 (12 %, one column: the flipped card plus
   the siblings its height change shifted) at board/100's last
   iteration, but 1342×656 (83 %, the full columns row) as the max
   at both Ns and as board/1000's last. A card's preferred-height
   change (the done-flip's body↔caption role swap) re-solves the
   column layout and at least sometimes propagates to the row
   layout's geometry, dirtying every column. The #390 open half
   (renderer-side damage marking coarser than the row diff) holds on
   the board too — with a column-scale floor visible in the 12 %
   reading.
6. **Board N = 10 000 does not render at all.** Both board cells
   skipped: the eager columns exceed the software renderer's i16
   pixel ceiling (§5, #462). Above ~2 100-2 600 tasks the board view
   is not slow — it is impossible.
7. **The refresh lane is unchanged** — 56.6 µs / 774 µs / 20.0 ms,
   within noise of the 2026-10-26 re-measure (60.4 µs / 807 µs /
   22.15 ms); its regression vs the 2026-10-20 baseline (27.2 µs /
   229 µs / 2.60 ms) stands as recorded there and remains that
   report's side finding, not this one's.

## 4. Budget row 3, partial leg — per view (references, never gates)

- **Today** (the desktop default view): 8.95 ms at N = 100 is inside
  the ≤ 20 ms envelope on this desktop core; 88.0 ms (N = 1 000) and
  903 ms (N = 10 000) fail it by 4× and 45× before any kiosk-hardware
  scaling.
- **Board** (the kiosk default view): 3.17 ms at N = 100 is inside;
  14.18 ms at N = 1 000 fails on kiosk silicon by inspection (the
  envelope's own 1.5 GHz in-order core is the row's unit), and
  N = 10 000 panics instead of measuring (#462).
- The full-frame leg (≤ 50 ms): today 19.9 ms at N = 10 000 is at the
  desktop-core margin (consistent with the stage-3 note in the chunk-6
  report); board 5.96 ms at N = 1 000 scales off the chart past its
  ceiling N. The on-device D250 session remains the decider for every
  leg (kiosk wave).

## 5. The board i16 render-ceiling crash (#462)

The first run of this re-measure (load 10.13, later aborted) died in
`draw-full/board/10000`:

```
thread 'main' (1357698) panicked at .../euclid-0.22.14/src/length.rs:201:21:
attempt to add with overflow
  4: <euclid::length::Length<i16, i_slint_core::lengths::PhysicalPx> as core::ops::arith::Add>::add
  12: <i_slint_core::items::BasicBorderRectangle as i_slint_core::items::Item_vtable_mod::Item>::render
  32: <i_slint_core::model::repeater::Repeater<...InnerComponent_drop_zone_98>>::visit
```

The software renderer's entire geometry space is `i16` physical
pixels (`i-slint-renderer-software-1.18.1/lib.rs:63-67`:
`type PhysicalLength = euclid::Length<i16, PhysicalPx>`), and the
board's unclipped columns (no `clip` on the column body, by design
comment) cross the ±32 767 px ceiling at ~550-650 fixture cards per
column. Empirical bracket (dev-profile scratch run, deleted; numbers
recorded here per the artefact mandate): **N = 2 000 (500 rows/column)
renders; N = 2 600 (650 rows/column) panics**. With the workspace's
deliberate `overflow-checks = true` (issue #303), this is a hard
crash — and the production chain is complete: the kiosk appliance
runs linuxkms on this renderer, `Board` is the kiosk default view,
so a kiosk board at that scale panics on first render. Filed as
**#462** (bug, ui, p1) with fix levers: #373's virtualisation
(structural — viewport-only instantiation never produces 32k-px
geometry), a markup clip round (a visual-design decision), an
upstream slint report (the Today view renders the same card widget at
~183k px of list depth without panicking — the per-path difference
is upstream's to explain).

## 6. Consequences for the open work

- **#460** (this instrument fix): implemented — view pins, visible-flip
  guarantee, region semantics, board baseline. Its remaining ask (the
  board cells the ceiling blocks) is #462's, not the instrument's.
  Disposition at stage 3.
- **#373** (board-column virtualisation): its motivation is now
  board-pinned end to end — the partial lane inverts the full lane at
  N = 1 000 on the board itself (§3.4), the per-item walk term is the
  same one the design round measured on Today, **and** the crash gives
  virtualisation a second, correctness role: it is the only in-house
  lever that both restores N = 10 000 renderability and removes the
  walk term. The design round's implementation gating (kiosk-wave
  D250 verdict) stands.
- **#390**: the open half (renderer-side damage marking) now has
  per-view numbers — a constant 88 % removal-reflow on Today, a
  12-83 % bimodal in-place profile on Board — and the correction that
  its recorded partial costs were mixed-population underestimates
  (§3.2). Its refresh-lane regression finding is untouched.
- **#461** (Today-list virtualisation): the corrected Today partial
  costs (88 / 903 ms) strengthen it — the desktop default view's
  steady-state per-edit damage is a full-list reflow.

## 7. Deviations / scope notes

- The first full run (load 10.13) was aborted by the §5 crash; its
  log is the crash evidence, its times are not used anywhere.
- The board/10 000 cells are skipped by design (`c987ef0`); the skip
  lines print in every run (§2) and the finding lives in #462.
- The overflow-bracket scratch run (dev profile, N ∈ {2 000, 2 600,
  3 000}) was never committed; its outputs are transcribed in §5 per
  the artefact mandate.
- Bench-only change, no dependency changes: `cargo deny` and the
  coverage sequence are unaffected, per the recorded instrument-fix
  precedent (PR #457). Run gates: fmt, slint token lint, clippy
  (`--workspace --all-targets -D warnings`) in pre-commit; nextest
  workspace 1 897 passed / 0 failed; doc-tests ok.
- No CHANGELOG line: the instrument fix changes bench lanes and their
  output format, not any user- or API-visible surface (the
  instrument-fix follow-up set the same precedent).
- No tracker edits from this round; #460/#461/#462 dispositions
  happen at stage 3 / by the user.

## 8. Acceptance checklist

- [x] The instrument error fixed as asked: lanes measure the scene
      they name (view pins), the flip provably renders (targets +
      preamble guard), and the numbers were re-run end to end on a
      quiet box (§2).
- [x] Both views covered (Today + Board), as asked — including the
      discovery that one of them stops rendering at scale (§5).
- [x] Every claim carries evidence: verbatim lane output (§2), the
      crash backtrace and bracket (§5), the baselines it corrects
      (§3) with the mixing mechanism that explains the deltas.
- [x] Gates green (§7); two standalone commits (`fix(bench)` × 2) +
      this `docs(report)`; no push (handoff: stage 3 ships the PR).
