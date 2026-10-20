# [Report] Kiosk chunk 6 stage 2 — refresh+draw bench (S2/KDF-7 row 3)

- **Date**: 2026-10-20
- **Branch**: `feat/perf-refresh-draw-bench` (base `2884443` = the
  stage-1 plan commit; plan:
  `.artifacts/plans/2026-10-20-kiosk-refresh-draw-bench-plan.md`)
- **Outcome**: complete — the API-blocker rule never fired (stage 1's
  verdict held); **both legs measured and recorded**. #300 closes via
  stage 3's PR.

## 1. What landed (commit by commit)

| Commit | Content |
|---|---|
| `af725ef` | Seam steps T1+T2: adapter size cell + `set_size` override + thread-local weak adapter registry (`window_adapter()`) |
| `f88c02c` | Seam steps T3+T4: `render_into` / `set_repaint_buffer_type` pass-throughs; T3 pins a full-window, pixel-touched headless render; T4 pins the `ReusedBuffer` cache-hit contract |
| `0057927` | T5: `benches/support/mod.rs` deterministic builder + `tests/bench_fixture_contract.rs` (determinism, shape, ASCII titles, fixed patterns) |
| `03fde84` | T6/D4–D9: criterion target `refresh_draw` — `refresh/{n}`, `draw-full/{n}`, `draw-partial/{n}` at N ∈ {100, 1 000, 10 000} |
| `6da0aa5` | CHANGELOG Unreleased line |

## 2. The recorded numbers (dev machine, criterion median [range])

**Machine**: AMD Ryzen 9 5900HX (16 threads), 30 GiB RAM, Linux
7.0.0-22-generic, release profile, criterion 0.8 (WallTime). Baseline
saved as `chunk6` in the worktree target (regenerable, not committed).

**Budget context (row 3, never asserted)**: full-frame repaint ≤ 50 ms
/ partial ≤ 20 ms @ 1366×768 software renderer on a 1.5 GHz in-order
kiosk core. These are dev-machine references; the on-device verdict
belongs to the phase-B spike.

| Lane | N=100 | N=1 000 | N=10 000 |
|---|---|---|---|
| `refresh` (ArcSwap load → reconcile → models::push) | 27.2 µs | 229 µs | 2.60 ms |
| `draw-full` (NewBuffer, 1366×768 software render) | 2.82 ms | 6.24 ms | 44.0 ms |
| `draw-partial` (ReusedBuffer + per-iter toggle→store→refresh) | 21.9 ms | 162.8 ms | 1.57 s |

**Region evidence**: every `draw-full` warmup rendered the full
1366×768 window (the lane's structural guard). Every `draw-partial`
region was 1351×760 = 1 026 760 px ≈ **97 % of the full frame at every
N**.

### Finding (D6 risk 2 confirmed, recorded not fixed)

The partial lane dirties essentially the whole window and costs
**more** than the full-frame lane at the same N (21.9 ms vs 2.8 ms at
N=100; 1.57 s vs 44 ms at N=10 000). Mechanism: `models::push`
(`adapter.rs` refresh) replaces every `ModelRc` with a fresh
`VecModel` each pass, so the renderer's dirty tracking sees the whole
board changed every refresh, and `ReusedBuffer`'s diff bookkeeping
adds cost on top of the near-full repaint. This feeds C2/F13-09 and
the spike's event-driven-redraw question: until the model push becomes
incremental, dirty-tracked rendering buys nothing over full-frame on
this markup — and the kiosk steady-state budget must be judged against
the `draw-full` numbers, not the partial lane's.

**Refresh-vs-draw composition at N=10 000**: one full pass ≈ 2.6 ms
refresh + 44 ms draw ≈ 47 ms of CPU work per repaint, inside the row-3
50 ms full-frame envelope on this desktop core — with the recorded C2
caveat that the 100 ms cadence, not this work, floors visible latency.

## 3. Mechanism-B cross-check (D11) — PASS

One-off dev check (temporary integration test, run and then removed,
per plan D11's "not a lane"): the testing backend with
`renderer_name: Some("software")` mounted `build_state(1_000)` at
1366×768, `take_snapshot()` returned a full 1366×768 RGBA frame with
non-uniform pixels. Mechanisms A and B agree: the bench's scene is
real, not degenerate. No fallback; #300 closes normally.

## 4. Deviations from the stage-1 plan (none touch an ACCEPTED ruling)

1. **T1 pins liveness only, not weak-die-after-drop** (first observed
   red): after the mounted component drops, slint core retains a
   strong adapter reference somewhere internal, so the
   registry-entry-dies assertion failed against slint internals, not
   the seam. The test keeps the registry-returns-the-mounted-adapter
   pin and documents the retention in a comment. (House rule: a
   failing new test may be a wrong test; the contract is the
   registry, not slint's lifetime graph.)
2. **`#[allow(unused_imports)]` on the seam tests module**: cargo
   builds bench targets with `cfg(test)` set, so the seam's
   `#[cfg(test)] mod tests` compiles into the bench target too, where
   its imports warn unused (the tests never run under the criterion
   harness). Allowed at the module with an explanatory comment; no
   existing test touched (D3 holds).
3. **`share` computation via `checked_div`**: clippy's
   `manual_checked_ops` rejected the plan-literal `if full_area > 0`
   shape; arithmetic only, meaning identical.
4. Bench risk-5 threshold never fired (worst iteration 1.57 s, well
   under the 5 s/iter pathology line; the 10 000 tiers kept the
   planned `sample_size(10)` / `measurement_time(15 s)`).

## 5. Acceptance checklist (stage-1 plan §6, stage-2 items)

- [x] T1–T4 green; no existing test modified (full ui suite 155/155).
- [x] T5 green; builder deterministic (equality-pinned two runs per N).
- [x] Bench prints three separated lanes × three Ns; region areas
      reported; numbers recorded (§2 above + PR body).
- [x] Mechanism-B cross-check recorded (§3) — no fallback needed.
- [x] Row-3 budget context in the bench's module docs; no assertion
      on any number anywhere.
- [x] Full §10 gates green: `cargo fmt --check` clean; clippy
      `--workspace --all-targets -D warnings` clean; nextest
      workspace 1198 passed / 0 failed; doc tests ok;
      `cargo deny check` ok (advisories/bans/licenses/sources); the
      CI-identical coverage sequence ran green (quarantine absorbed
      one killed-writer profraw, per the 2026-10-13 fix).
- [x] CHANGELOG line; no tracker edits (MD-4); no push from stage 2.

## 6. PR-body-ready numbers block (stage 3, MD-5)

> Dev-machine references (AMD Ryzen 9 5900HX, criterion medians,
> 1366×768 software renderer, deterministic board fixture), never
> gates — the on-device verdict belongs to the phase-B spike:
>
> | Lane | N=100 | N=1 000 | N=10 000 |
> |---|---|---|---|
> | refresh (load → reconcile → push) | 27 µs | 229 µs | 2.6 ms |
> | draw-full (NewBuffer) | 2.8 ms | 6.2 ms | 44 ms |
> | draw-partial (ReusedBuffer) | 22 ms | 163 ms | 1.57 s |
>
> Finding: the partial lane's region is ~97 % of the full frame at
> every N — the whole-`VecModel` replacement in `models::push`
> dirties the entire board each refresh, so dirty-tracked rendering
> currently costs *more* than full-frame (diff bookkeeping on top of
> a near-full repaint). Budget context: row 3's ≤ 50 ms full-frame
> envelope is met with margin at N=10 000 on this desktop core; the
> 100 ms repaint cadence still floors composed latency (C2/F13-09).
> Mechanism-B cross-check (testing-backend `take_snapshot` at
> 1366×768, N=1 000): full-window, non-degenerate frame — agrees
> with mechanism A.

## 7. Stage-3 verification (review agent, 2026-10-20)

Gates re-run green on the rebased branch (fmt, clippy
`--workspace --all-targets -D warnings`, nextest 1198 passed,
doc tests, `cargo deny`, the CI-identical coverage sequence —
quarantine absorbed one torn profraw, per the 2026-10-13 fix).
Bench re-run on the same machine reproduced every structural
finding and the numbers within run-to-run variance — with one
honesty note: `draw-full/10000` measured 54.2 ms this run vs the
44.0 ms recorded above (+29 % vs the saved baseline; `draw-full/
1000` similarly 7.7 ms vs 6.2 ms). All other cells landed within
±17 % of the §2 table, and the partial-lane regions were again
1351×760 = 97 % at every N. Consequence for the wording: the
N=10 000 full-frame draw is **at or near** the row-3 50 ms
envelope on this desktop core (44–54 ms across runs), not "met
with margin" — the PR body and this section say that; the §2
table stands as recorded. References, never gates (MD-5).
