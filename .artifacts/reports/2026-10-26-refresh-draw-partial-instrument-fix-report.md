# [Report] Partial-lane instrument fix — the true-partial verdict (follow-up to chunk 7)

- **Date**: 2026-10-26 (runs 02:00–02:10 CEST)
- **Branch**: measured on `fix/bench-partial-instrument` (stacked on
  `docs/bench-true-partial-remeasure` @ `7f09d36`; user-directed
  follow-up to chunk 7). During stage-3 review both commits were
  rebased onto the re-based chunk branch and ship in the chunk-7 PR
  (`docs/bench-true-partial-remeasure`); the stacking branch is folded.
- **Machine**: same dev box (Ryzen 9 5900HX, 30 GiB, rustc 1.99.0,
  criterion 0.8.2, `performance` governor, AC). Load during the fixed
  run: 10.43 / 7.78 / 3.69 — a parallel dispatch chunk was running
  (recorded per the standing D5 honesty rule). The region instrument
  is deterministic and load-independent; the times are references as
  always.
- **Outcome (one line)**: with the instrument fixed (guaranteed one-row
  change + max-region sampling), the true-partial verdict for budget
  row 3 is finally measurable — and it is the STILL-DIRTY branch: one
  card's change still dirties **~88–91 % of the frame**; D9 fixed the
  model layer, not the renderer's damage region.

Every number below is a **dev-machine reference, never a gate**; the
on-device verdict belongs to the kiosk wave; the 100 ms repaint
cadence (C2/F13-09) floors composed latency regardless.

## 1. The bench bug and the fix

Chunk 7's re-measure (the sibling report from 2026-10-25/26) exposed a
latent bench bug that D9 unmasked:

1. The region carrier (`draw_partial_lanes`) kept only the
   **last** iteration's render — representative pre-D9 (every
   iteration dirtied ~97 %), wrong post-D9 (iterations differ).
2. The setup's "toggle" was `done = TOGGLE.get().then(…)` with a
   globally alternating flag — on even iterations it assigned `None`
   to a usually-already-`None` task: a **no-change** state, which the
   D9 row-diff correctly dirties nothing for. The lane was measuring
   the unchanged-refresh property, not the partial repaint.

The fix (`crates/taskboard-ui-slint/benches/refresh_draw.rs`, this
branch):

- flip the task's **current** `done` value (`None → Some(fixed stamp)`
  / `Some → None`), so every iteration guarantees a one-row change —
  the stamp (`PARTIAL_STAMP_SECS = 1_704_067_200 + 9_000_000`) is
  outside the fixture's own stamp space, so `None → Some` is always a
  real change;
- keep the **max-area** region across all iterations (honest worst
  case, immune to last-iteration sampling) and additionally report the
  **last-iteration** region, so a warm-up-only full-frame render (e.g.
  the first render on a freshly switched `ReusedBuffer`) is
  distinguishable from steady-state behaviour.

## 2. Fresh numbers (fixed instrument)

`cargo bench -p taskboard-ui-slint --bench refresh_draw -- draw-partial`
(the filter also re-runs the draw-full warm-up guard, which passes at
every N):

| Lane | N=100 | N=1 000 | N=10 000 |
|---|---|---|---|
| `draw-partial` time (median [range]) | 1.57 ms [1.41–1.73] | 56.2 ms [48.4–64.0] | 566.5 ms [319–811] |
| partial region (max over iterations) | 1366×704 = **91 %** | 1342×688 = **88 %** | 1342×688 = **88 %** |
| partial region (last iteration) | 1342×688 = 88 % | 0×0 (0 %) | 0×0 (0 %) |

Region lines, verbatim:

```
[refresh_draw] draw-partial/100: region 1366×704 (961664 px = 91% of the full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)
[refresh_draw] draw-partial/100: last-iteration region 1342×688 (923296 px = 88% of the full frame)
[refresh_draw] draw-partial/1000: region 1342×688 (923296 px = 88% of the full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)
[refresh_draw] draw-partial/1000: last-iteration region 0×0 (0 px = 0% of the full frame)
[refresh_draw] draw-partial/10000: region 1342×688 (923296 px = 88% of the full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)
[refresh_draw] draw-partial/10000: last-iteration region 0×0 (0 px = 0% of the full frame)
```

(An intermediate run without the last-iteration readout reproduced the
same max regions: 91 % / 88 % / 88 %, medians 1.53 / 24.5 / 330 ms.)

**The key reading is steady-state, not warm-up**: at N=100 the *last*
iteration — thousands of iterations after any ReusedBuffer switch —
still reports 88 %. A one-card change genuinely dirties a
board-scale region on this markup. (Caveat: the stage-3 spot-check's
run read 0×0 on that same last iteration — see the anomaly note in §3;
the steady-state conclusion rests on the max over thousands of
iterations, which reproduced exactly, not on the last-iteration
reading alone.)

## 3. Verdict — perf-budget row 3, partial leg (plan D8 branch b)

**(a) Region sub-verdict (the instrument now measures what row 3
asks).** A guaranteed one-row change produces an **88–91 %-of-frame
dirty region at every N**. The D9 row-diff reaches the models
(`diff_rows` fires exactly one `set_row_data`), but the renderer's
damage region does not follow the diff: the dirty area is board-scale
(1342×688 ≈ the whole board content area minus chrome; 1366×704 at
N=100). This is plan-D8 branch **(b) STILL-DIRTY**: the row-diff did
not reach the dirty region.

**(b) Cost ordering.** The chunk-6 inversion is now total: the partial
lane costs more than full-frame at **every** N (1.57 vs 1.39 ms;
56 vs 2.3 ms; 567 vs 22.6 ms), i.e. an ~88 % redraw *plus*
`ReusedBuffer` damage-tracking bookkeeping. (Yesterday's mixed
population made the small-N cell look 11× cheaper than it is; the
guaranteed-change instrument corrects that.)

**(c) Budget-leg sub-verdict.** Row 3's partial leg (≤ 20 ms at
1366×768 on a kiosk core): **failed as a true partial at every scale
on the dev reference machine** — 56 ms at N=1 000 and 567 ms at
N=10 000 are an order of magnitude and more above the envelope even
before scaling down to kiosk silicon; only N=100 (1.57 ms) is inside.
House caveats verbatim: dev-machine **references, never gates**; the
on-device verdict belongs to the kiosk wave; the 100 ms repaint
cadence floors composed latency regardless.

**Mechanism hypothesis** (plan D8-b's candidate list, narrowed by this
run): Slint's repeater damage marking is coarser than the row diff —
when a repeater row changes, the damage lands on the repeater/layout
level (the 70c812d residual note: "an outer `set_row_data` on a column
row rebuilds that row's inner card repeater" — the flip side is that
inner-row notifications apparently propagate board-scale damage on
this markup family). The levers that would actually shrink the region
live renderer-side: #373's ListView virtualisation (fewer live items →
smaller repeater extent), or upstream Slint per-row damage tracking.

**Open anomaly, recorded not resolved**: at N ≥ 1 000 the
last-iteration region reads 0×0 despite the guaranteed-change setup
(the max over the same run is 88 %). A guaranteed change should never
produce a zero-damage render. Candidates: criterion's
`iter_batched`/PerIteration interaction with the final batch (an
iteration's render landing after the previous iteration's render
already consumed the damage), or refresh-notify coalescing at the
event-loop boundary. **Stage-3 spot-check (2026-10-26 02:30, load
8.6)** re-ran the whole draw-partial group on the rebased branch: the
max regions reproduced exactly (91 % / 88 % / 88 %; times inflated by
the concurrent dispatch load, as expected), but the last-iteration
region read 0×0 at **all three N including 100** — so the
N=100-last-iteration reading above is run-dependent, not a law, and
the steady-state argument rests on the max (which sampled thousands of
iterations, not the final one) rather than on any single last
iteration. The anomaly needs its own small probe before anyone trusts
per-iteration region semantics at any scale.

## 4. What this does to #390 and #373

- **#390**: the true-partial verdict it asks for is now measured
  (branch b): the partial leg of budget row 3 is **not** met at kiosk
  scales, and the remaining dirt is renderer-side damage marking, not
  model push. Recommended disposition: **keep open**, re-scoped — the
  model half is done (D9, confirmed zero-notifications on unchanged
  refresh + exactly one row touched on change); the open half is the
  renderer's damage region (next lever: #373's analysis; optional
  small probe for the last-iteration 0×0 anomaly).
- **#373**: motivation strengthened again — the partial lane now
  shows board-scale dirt *and* board-scale cost per one-card change;
  virtualisation attacks both. Its Ask-2 instrument parenthetical is
  updated to point at this report (the 0 %-region note from
  yesterday's body is obsolete — that was the broken instrument).

## 5. Deviations / scope notes

- The plan-of-record for chunk 7 pre-committed §6.1/§6.2 texts; this
  user-directed follow-up supersedes yesterday's #390 comment
  disposition (keep open for the *unmeasured* reason) with the
  measured branch-b reason. Both comments stay on the issue; neither
  issue is closed here.
- The fix touches only `benches/refresh_draw.rs` (the chunk-7 freeze
  rule bound stage 2 of that chunk, not this user-ordered follow-up).
  No production code changed; `draw-full` and `refresh` lanes are
  untouched.
- Gates: fmt, clippy (`--workspace --all-targets -D warnings`),
  nextest, doc-tests green (bench-only change; no dependency changes —
  `cargo deny`/coverage unaffected; no push, no PR from this session).
