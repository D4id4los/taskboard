# ListView row-anchored scroll anchoring (#592) — stage-2 report: measurement, verdict, implementation

- **Date**: 2026-11-05
- **Branch**: `perf/listview-row-anchor` (chunk 8 of the v0.2
  dispatch, §9). Stage-1 plan:
  `.artifacts/plans/2026-11-05-listview-row-anchor-plan.md` (§3's
  criterion is quoted below verbatim in effect; §4's measurement
  design was executed in order).
- **Normative base**: `947b9f76`; the branch adds the M-A mechanism
  test, the implementation, and this report.

## 1. The verdict

**IMPLEMENT.** All four gates pass:

| Gate | Question | Verdict | The number |
|---|---|---|---|
| G-0 | Does a UI-side compensation write control the viewport against the repeater's own content-y rewrite? | **PASS** | Both candidate seams land the commanded displacement exactly — residual **0.00 px** at the first stable frame, **0.00 px** drift over ten further ticks (§3) |
| G-1 | Is the achievable accuracy good enough to be worth it? | **PASS** | Estimator (a) residual: **median 8.9 %, worst 16.5 %** of the uncompensated shift (criterion: ≤ 20 % / ≤ 50 %); estimator (b) exact for ever-live rows (§4) |
| G-2 | Does the discriminator leave the pinned semantics exact? | **PASS** | The within-viewport and cross-section pins pass byte-identical under the live mode; the widget-level discriminator pins cover the boundary guard band (§6) |
| G-3 | No regression in the refresh/draw benches? | **PASS (with one implementation correction)** | Paired before/after medians inside the runs' own spread after the extent-report seam was moved off `changed height` (§5) |

## 2. M-A — the mechanism fact (permanent test)

`models::tests::a_head_membership_change_is_a_rebind_cascade_plus_one_tail_mutation`
pins the §0 correction: a head (above-viewport) membership change
transports through `diff_rows` as a `set_row_data` rebind cascade
plus exactly one tail mutation (head removal of one of N rows = N
mutations; head insertion = N+1), so every structural notification
the push discipline emits is tail-indexed and the slint repeater's
before-window offset-shift branches (`repeater.rs:549-610`) never
fire for Today membership changes. The content-anchored contract's
cause is now code-backed for any future revisit.

Invert-check note (a measurement-hygiene finding worth recording):
the discriminating scratch variant must **preempt** the rebind loop —
a post-loop condition can never distinguish the shapes, because the
cascade has already rebound the mirror to the desired suffix. The
preemptive `remove(0)` variant fails the count assertion (mutations
= 1 ≠ 3); the production shape passes.

## 3. M-B — seam controllability (G-0), scratch transcript

Probe: the real in-house `ListView` (`RowAnchoredProbe`'s ancestor,
300 uniform 64 px rows, 400 px viewport), seek-driven to row 200,
witness `card-205` at y = 320.00. The head-model removal fires the
diff through a live `VecModel` (the push discipline's shape).

```
MB control: y0=320.00 y1=256.00 shift=-64.00
MB control: ch-fired=3 anchor-fired=0
MB variant-i (content-height): y0=320.00 t1=320.00 t2=320.00 t3=320.00 residual1=0.00 residual3=0.00
MB variant-i: ch-fired=3 anchor-fired=0 arm=false
MB variant-i after 10 more ticks: y=320.00 drift=0.00
MB variant-ii (nonce): y0=320.00 t1=320.00 t2=320.00 t3=320.00 residual1=0.00 residual3=0.00
MB variant-ii after 10 more ticks: y=320.00 drift=0.00
```

Reading:

- **Control** (uncompensated): the witness reflows up by exactly the
  removed extent (−64.00) — the pinned content-anchored law,
  reproduced at the probe layer.
- **Variant (i)**, the D20c shape (one-shot write inside
  `changed content-height`, armed before the change): pixel-still at
  the first stable frame, settles, no oscillation over 10 ticks.
  `ch-fired=3` and `arm=false` record the ordering fact: the
  handler's write is consumed by the change's own pass and is not
  superseded by the runtime's `content_y_set` rewrite.
- **Variant (ii)**, the F8 shape (nonce-gated direct write, fired on
  the Rust-side property set after the model change): identical
  exactness and settle.

Both sanctioned seams win; the shipped mode uses the nonce shape
(sketch §5.3 — it carries the Rust-side discriminator's payload).
The write does not fight the runtime's re-anchoring.

## 4. M-C — accuracy campaign (G-1), scratch transcript

Fixture: the today mount (`build_state(600)`, 1000×900, the pins'
keyboard walk). Overdue membership is **open ∧ duedate'd (every 3rd)
∧ not done** — scheduled-ness does not gate membership (geometry
probe: orders 3, 6, 9, 12 render consecutively, a §0 refinement of
the plan's F4 reading). Row extents on this fixture are two-valued:
69 px (plain card) and 90 px (card + context line).

Ground truth: the pins' own law (pinned-actual) — an above-viewport
change shifts the live rows by **exactly** the changed row's rendered
extent, so the uncompensated |shift| is the extent the estimator must
hit. M-B proved a commanded delta lands exactly (0.00 residual), so
the compensated residual is |est − true| by composition.

Estimator (a) sample (consecutive-live-row y-deltas, shallow + deep
windows; the runtime's `cached_item_height` is the mean of observed
instance heights, and `content-height / total-rows` equals it
exactly):

```
MC estimator-(a) sample: n=24 mean=75.12 min=69.00 max=90.00
```

Campaign (4 change classes × 2 walk depths = 8 changes):

```
MC depth=30 case=insert-head:    shift=+90.00 true-extent=90.00 est-a=75.12 residual-a=-16.5% residual-b=0.0%
MC depth=30 case=remove-head:    shift=-69.00 true-extent=69.00 est-a=75.12 residual-a=+8.9%  residual-b=0.0%
MC depth=30 case=remove-second:  shift=-69.00 true-extent=69.00 est-a=75.12 residual-a=+8.9%  residual-b=0.0%
MC depth=30 case=done-flip-pair: shift=-69.00 true-extent=69.00 est-a=75.12 residual-a=+8.9%  residual-b=0.0%
MC depth=15 case=insert-head:    shift=+90.00 true-extent=90.00 est-a=75.12 residual-a=-16.5% residual-b=0.0%
MC depth=15 case=remove-head:    shift=-69.00 true-extent=69.00 est-a=75.12 residual-a=+8.9%  residual-b=0.0%
MC depth=15 case=remove-second:  shift=-69.00 true-extent=69.00 est-a=75.12 residual-a=+8.9%  residual-b=0.0%
MC depth=15 case=done-flip-pair: shift=-69.00 true-extent=69.00 est-a=75.12 residual-a=+8.9%  residual-b=0.0%
MC estimator-(a) summary: median=8.9% worst=16.5%
```

**Estimator (a): median 8.9 %, worst 16.5 %** — inside the
pre-declared 20 %/50 % lines with margin. The fixture's two-valued
extent distribution is more favorable than the plan's ±19 % spread
assumption; the inserted row (90 px, never live) is the worst case.
**Estimator (b)** (the id-keyed height cache of extents observed
while a row was live) is exact (0.0 %) for every campaign removal —
the changed rows are ever-live by construction — which is what makes
the pixel-still pin below assertable. Insertions have no observed
extent by definition (D-2's permanent exclusion of off-window
instantiation) and ride estimator (a).

Post-implementation note: on the live mount the runtime's own average
beat the campaign's sample mean — the compensated insertion left a
−4.44 px residual (4.9 % of the 90 px extent).

## 5. M-D — G-3 bench evidence, and the corrections it forced

The box was **not quiet** for much of the measurement (load 4–24 on
16 cores); every run below is labelled with its sampled load, and the
verdict reads comparable-load pairs only.

**Round 1** (first paired run) showed a consistent `draw-full/today`
regression (+42 %…+70 % at the three N, outside the spread) while
`refresh/{n}` moved inside noise (µs-scale guarded no-op lane). The
**attribution cascade**:

1. Removing only the delegate's extent-report handler restored the
   baseline — the handler was the cost.
2. An **empty** `changed height` handler was cheap — the cost is the
   handler *body's* dispatch, not the subscription: the ungated
   report crosses into Rust every frame (row heights oscillate
   sub-pixel across draw iterations), measured +60 % at
   `draw-full/today/1000` (4.49 ms vs 1.72 ms baseline) even at low
   load.
3. Moving the report to the delegate's `init` was performance-clean
   but **functionally wrong** — `init` fires pre-layout (order-3's
   extent cached as 44 px against a true 69 px; the pixel-still pin
   failed with a 25.0 px shift). The stage-3 lesson: the pin caught
   what the bench-first protocol had "passed".
4. **Final shape**: the report stays on `changed height` but is
   dispatch-gated by the last reported extent with a half-pixel
   band — correct (all 32 virtualisation pins green, pixel-still
   exact) and clean (the gate suppresses the sub-pixel chatter).

**Final paired evidence** (alternating runs, equal load):

| run (load) | draw-full/today 100 / 1000 / 10 000 | draw-partial/today 100 / 1000 / 10 000 |
|---|---|---|
| after (18.7) | 1.54 / 1.61 / 1.58 | 5.94 / 6.42 / 7.28 |
| **before (5.4)** | **1.73 / 1.73 / 1.73** | 6.10 / 5.97 / 6.22 |
| **after (5.3)** | **1.57 / 1.62 / 2.28*** | 6.05 / 6.30 / 6.60 |
| **before (6.6)** | **1.75 / 1.71 / 5.23*** | 14.17* / 8.36* / 7.45* |

\* load spikes inside the measurement window (the 21:37 before-run
coincided with a load burst; its clean cells agree with its own
earlier runs). On the comparable-load pairs the after-deltas are
**negative or ~0** (draw-full/today/1000: after 1.61–1.62 vs before
1.71–1.73) — well inside the runs' own spread. The partial lane's
region carrier stays in its uncompensated class (the guard prints the
same region class; no growth). `refresh/{n}`: before 1.4–3.2 µs,
after 1.4–1.6 µs across all runs — the lane measures the #463 guarded
no-op, untouched by the mode.

**Stage-3 verification** (independent interleaved A/B, the shipped
gated-`changed height` shape vs the rejected `init` shape, three
rounds at `draw-full/today/1000` under load 9–37): on the
comparable-load pairs the shipped shape reads 1.64–1.81 ms against
the init shape's 2.16–2.23 ms — the shipped trigger costs nothing
measurable against the report's before-runs (1.71–1.73 ms), and the
init variant is if anything the slower of the two. The review's
independent numbers confirm the final paired evidence above.

## 6. What landed (the branch diff)

- `feat(ui-slint): the ListView row-anchored compensation mode` —
  the widget seam (`row-anchored` + the anchor-fact inputs + the
  nonce-gated guarded compensation) and the permanent
  `RowAnchoredProbe` pins: mode off = the content-anchored law;
  mode on with the exact extent = pixel-still and settled; a
  supplied extent drives the compensation verbatim; an unobserved
  extent falls back to the runtime average; within-window and
  boundary changes never compensate (the one-average-row guard
  band). Defaults off — no other #373 consumer changes behaviour.
- `feat(ui-slint): the today sections' row-anchored compensation` —
  the Today opt-in and the pipeline: `row_anchor_summary` (pure,
  unit-tested) computed against the pre-diff mirror in `push_today`,
  gated on a real membership shift by the row ids (a pure content
  edit or a tail-only change never compensates); one
  `TodayAnchorFacts` payload per changed section on a nonce edge
  (the FocusScrollHint pattern); the compensation extent from the
  id-keyed height cache (the `today-row-extent` seam, reported by
  the half-pixel-gated `changed height` hook) with the runtime
  average as fallback; the cache pruned to the live
  today set each pass. `TodayRowModel` gains the stable `id` key.
- The three existing today pins **extended, never weakened**: the
  true-removal-above pin now asserts **pixel-still** (exact zero
  shift — strictly stronger than the old reflow-by-extent; verified
  on the live mount, `shift=0.00` with the exact cached extent 69);
  the insertion-above pin asserts the residual stays under one
  ordinary row extent (the mode-off full-extent reflow stays pinned
  at the widget level by the probe); the within-viewport and
  cross-section pins pass byte-identical.
- `models::tests::row_anchor_summary_locates_the_first_structural_change`
  (M-A's extension, sketch §5.1).

Deviations from the sketch (§5), recorded per §4's verdict section:

1. **The extent cache ships** (sketch: "only if it shipped in the
   prototype cheaply") — it was required: the dispatch's
   pixel-still acceptance is unreachable with the average estimator
   alone (8.9-16.5 % residual ≈ 6-15 px). The channel is the
   `today-row-extent` seam — a `changed height` hook whose Rust
   dispatch is half-pixel gated (the G-3 correction: the ungated
   hook's per-frame dispatch cost ~60 % of a full-frame draw, and an
   `init`-only alternative reported pre-layout heights).
2. **The insertion-above pin asserts "under one extent", not exact
   zero** — an inserted row has no observed extent by D-2's
   exclusion; exact-zero would pin the runtime's average, not a
   contract. The removal pin is exact-zero.
3. **Commit shape**: the widget seam and the today integration are
   two standalone-green commits, but the sketch's "facts before
   widget" order inverted — the slint compiler drops a global that
   no markup uses, so the widget (consumable standalone via probe
   inputs) landed first.

## 7. Known limits (the #592 annotation basis)

- A **fresh insertion** above the viewport compensates by the runtime
  average (measured residual −4.44 px on the fixture, 4.9 %): not
  pixel-still, strictly better than the full-extent reflow.
- A **live row's height change smaller than the report gate's
  half-pixel band** is never reported: the cache keeps the last
  reported extent, and the residual is bounded by that sub-half-pixel
  delta — below any visible threshold, and the page never scrolls.
- **No other ListView consumer** opts in (board columns, compact
  leg, stack detail unchanged; kiosk's board mount unchanged).
- Scale changes self-heal (rows re-report on re-instantiation).

Re-entry condition for anything stronger: an upstream repeater that
exposes per-row rendered extents (or an off-window estimator that
does not need instantiation) would make insertions exact; nothing
else in this design would change.

## 8. The #592 annotation (drafted for the stage-3 shipping pass)

> **Resolved as implemented** (the v0.2 dispatch's chunk-8
> measure-first gate ruled **implement**): the today sections now
> carry the optional row-anchored mode — above-viewport removals of
> ever-shown rows are pixel-still (the id-keyed extent cache is
> exact), never-seen insertions compensate by the runtime average
> (measured residual 4.9 % of the extent on the fixture), and the
> within-viewport click-flip geometry is untouched. Measurement
> verdicts: G-0 both sanctioned seams exact and settled (0.00 px
> residual, no oscillation); G-1 estimator residuals median 8.9 % /
> worst 16.5 % against the pre-declared 20 %/50 % lines; G-2 the
> within-viewport and cross-section pins pass byte-identical; > G-3 no systematic bench delta on comparable-load pairs after one
> implementation correction (the extent report's Rust dispatch is
> half-pixel gated — the ungated report crossed into Rust every
> frame and cost ~60 % of a full-frame draw at N = 1000). Full numbers and the deviation list:
> [2026-11-05-listview-row-anchor-stage2-report.md](.artifacts/reports/2026-11-05-listview-row-anchor-stage2-report.md).
> The content-anchored law itself is still pinned, at the widget
> level, with the mode off (the RowAnchoredProbe contract pins).
