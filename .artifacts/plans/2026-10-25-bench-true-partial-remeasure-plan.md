# [Plan] Backlog-Triage Chunk 7 — true-partial re-measure + #373 re-justification

- **Date**: 2026-10-25
- **Kind**: stage-1 execution-chunk plan (dispatch plan
  `.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §8,
  chunk 7) for `docs/bench-true-partial-remeasure`. Execution chunk —
  the deliverable is a report artefact plus tracker bookkeeping; **no
  code changes** (the bench run is read-only over the crate; the §8
  freeze rule's explicit exception).
- **Origin (normative)**: issues **#390** (true-partial verdict for
  budget row 3) and **#373** (ListView analysis; the stale-premise
  half is this chunk's bookkeeping target); the baselines —
  `.artifacts/reports/2026-10-20-kiosk-chunk6-refresh-draw-bench-stage2-report.md`
  (dev-machine table §2 + stage-3 variance note §7) and
  `.artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md`
  (dev references + D250 slot-2 numbers); the dispatch plan §1/§8.
- **Status**: READY — every open question settled below (§3 decisions
  D1–D12). **Base `fc2c52d` = fetched `origin/main` (2026-10-25)**;
  local `main` (`f7b1ac8`) is stale by four commits and untouched
  (chunk-6 stage-1 precedent: branch off the fetched origin, record
  the base). Worktree
  `../taskboard-WT-docs-bench-true-partial-remeasure` created off
  `origin/main`; doctor green before creation (70 G free, floor 25 G).

## 1. What this chunk delivers

1. **The re-measure** — `crates/taskboard-ui-slint/benches/refresh_draw.rs`
   re-run against current `main`, all three lanes × the bench's
   standard N grid {100, 1 000, 10 000}, with the dirty-region
   instrument recorded. This is the first run where the partial lane
   can produce a **true partial**: r2 D9 (`70c812d`, "stable model
   identity via row-diff push") removed the whole-`VecModel`-replacement
   dirt that made every chunk-6 "partial" a ~97 %-of-frame redraw.
2. **The report** —
   `.artifacts/reports/2026-10-25-refresh-draw-true-partial-remeasure-report.md`
   (method; fresh numbers; comparison vs both baselines; the verdict
   for perf-budget row 3's partial leg; what survives of #373's
   motivation). Skeleton in §5.
3. **The tracker bookkeeping** — a comment on #390 (fresh numbers +
   recommended disposition) and a body edit on #373 (stale figures
   replaced/marked superseded with a pointer to the new report; the
   analysis *ask* stays intact). Exact drafts in §6/§7. Neither issue
   is closed by stage 2; stage 3 applies the #390 disposition only on
   the "close as resolved-by-D9 with on-device residual" branch
   (dispatch prompt 7.3).

**Non-goals (§8, binding)**: no renderer/model/bench code changes; no
on-device D250 session (see D3 for the contingency); no #373
implementation; no ui-slint edits; no CHANGELOG (not user-observable).

## 2. Stage-1 verification record (what the plan stands on)

- **r2 D9 premise — confirmed in source at the base commit.**
  `crates/taskboard-ui-slint/src/models.rs` carries the persistent
  handles (`PushModels`, models.rs:151), the positional diff
  (`diff_rows`, models.rs:212 — `set_row_data` only where a row
  differs, then truncate/append) and the get-or-install-once helper
  (`push_rows`, models.rs:347). One `Adapter`-owned `PushModels` per
  mounted window; an unchanged refresh notifies nothing. The R3
  notification surface (fc2c52d, *after* the baselines) follows the
  same discipline (persistent handle, diffed; empty on this fixture —
  no reminder rules in `build_state`), so it is background drift to
  name in the report, not a dirt risk.
- **Why the partial lane should now measure a true partial.** The
  lane's per-iteration setup toggles one task's `done` → the diff
  finds exactly one differing `TaskCardModel` row in one column's
  card model → one `set_row_data` → the renderer's dirty region should
  collapse from ~97 % of 1366×768 to roughly that one card's bounding
  box. The column row itself compares equal (scalars unchanged, same
  inner handle — the unchanged-column rule), so the outer repeater
  does not fire, and 70c812d's recorded residual ("an outer
  `set_row_data` on a column row rebuilds that row's inner card
  repeater") does not engage for a card-row toggle. This is the
  hypothesis the region instrument decides — the plan pre-judges
  nothing (D8's branch rules).
- **Bench build — green at `f7b1ac8`** (`cargo bench --no-run -p
  taskboard-ui-slint --bench refresh_draw`, finished `bench` profile
  in 4m 20s; only pre-existing `.slint` deprecation warnings). The
  base then moved to `fc2c52d` mid-session (R3 chunks A/B/C + the
  notification surface; +661 lines under `crates/taskboard-ui-slint/`),
  so stage 2's T1 re-runs `--no-run` in the worktree as the
  authoritative build evidence at the measured commit (D4).
- **Bench-code drift since the baselines — cosmetic, recorded.** Two
  commits touched `benches/` after the 2026-10-20 run: `8015b6b`
  (+12 lines in `support/mod.rs`: the new task fields, all `None` in
  the fixture) and `c0ece3e` (+3 lines in `refresh_draw.rs`: the
  `UiRequest` lane's mount-signature accommodation). The instrument is
  structurally unchanged: same three groups (`refresh`, `draw-full`,
  `draw-partial`), same N grid, same `configure_group` tiering, same
  region report. The report names both commits in its method section.

## 3. Decisions

- **D1 — Base and worktree.** Branch `docs/bench-true-partial-remeasure`
  off fetched `origin/main` = `fc2c52d` (local `main` stale, untouched;
  reason recorded above). Worktree path flat-suffixed per AGENTS §8.
  Doctor green at creation (70 G free / 25 G floor); **stage 3 teardown
  is mandatory** (`git -C /home/viktoria/src/taskboard worktree remove
  --force ../taskboard-WT-docs-bench-true-partial-remeasure`, verified
  via `git worktree list` + a `df` check — this worktree grows a full
  `target/`).
- **D2 — Scope: dev machine only.** The on-device D250 session stays a
  §8 non-goal (the accepted dispatch ruling outranks the dispatcher's
  informational note that the netbook is up). The residual on-device
  partial verdict is carried by the kiosk wave's next session and named
  as such in the #390 comment. Appendix A holds the ready-made D250
  commands for the *only* case the user explicitly expands scope
  mid-chunk; absent that order, stage 2 does not touch the netbook.
- **D3 — Invocation: the full bench target, criterion's own
  configuration, plus a draw-only confirmation re-run.** The
  dispatch's "both lanes" = the two draw lanes the verdict needs; the
  default run covers `refresh` too at no extra cost, and the
  composition note (refresh + draw per repaint) uses it. Commands in
  §4 (T2/T3). No CLI overrides of warmup/measurement/sample counts —
  identical methodology to the baselines: criterion defaults (3 s
  warm-up, 5 s measurement, sample size 100) for N ∈ {100, 1 000};
  the bench's own `configure_group` tiering for N = 10 000
  (`sample_size(10)`, `measurement_time(15 s)`). The partial lane
  measures one render per iteration (`iter_batched`,
  `BatchSize::PerIteration`; the toggle→store→refresh setup is
  unmeasured) — same as the baselines. The primary run saves criterion
  baseline `true-partial` (regenerable, never committed — the `chunk6`
  precedent); the confirmation re-run omits the flag so it cannot
  overwrite it.
- **D4 — Build evidence at the measured commit.** T1 re-runs
  `cargo bench --no-run -p taskboard-ui-slint --bench refresh_draw` in
  the worktree. The stage-1 green at `f7b1ac8` is recorded context;
  the T1 result is the evidence the report cites. A red T1 is a STOP:
  report, do not patch (freeze rule).
- **D5 — Machine-state protocol (methodology honesty).** Same dev box
  as both dev baselines (AMD Ryzen 9 5900HX, 30 GiB, Linux
  7.0.0-22-generic). Before the primary run, record in the report:
  wall-clock date/time; load average (`uptime`); that no other
  `cargo`/`rustc`/bench process is running (`pgrep -a cargo; pgrep -a
  rustc` — chunks of this dispatch may run concurrently on this box;
  if a quiet window is not available, run anyway and record the
  concurrency honestly — these numbers are references, never gates);
  AC/battery state (or N/A); CPU governor (record, never change);
  `rustc -vV` line; criterion version from `Cargo.lock`; the commit
  hash. Bench stderr is captured (`tee /tmp/refresh_draw_remeasure.log`,
  not committed) — the `[refresh_draw] draw-partial/{n}: region W×H
  (...)` lines are the finding-carrier and are transcribed verbatim
  into the report.
- **D6 — Comparison layout: no cherry-picking.** All six baseline
  cells per lane are reproduced or their absence explained (stage-3
  trap). Tables in §5.3. Primary comparison: the 2026-10-20 §2 table
  (same-box, closest methodology). Variance envelope: the 2026-10-21
  dev references (same box, different day — refresh 30/247/5 150 µs,
  draw-full 3.34/8.61/60.5 ms, draw-partial 25.3/197.9/1 851 ms) and
  the 2026-10-20 stage-3 re-run lesson (`draw-full/10000` 44.0 →
  54.2 ms, +29 %, same machine) — day-to-day drift of this magnitude
  is the yardstick for "changed" vs "noise". The D250 slot-2 column is
  quoted context-only: it is a pre-D9 upper bound measured on a
  different CPU; it is **not** a like-for-like delta target.
- **D7 — Verdict shape (report §4): three sub-verdicts, house caveats
  verbatim.** (a) *Region* (the instrument): per-N W×H and share of
  frame. (b) *Cost ordering*: `draw-partial` vs `draw-full` at equal N
  — the chunk-6 inversion (partial costing more than full: 21.9 vs
  2.8 ms at N=100) either resolves or does not. (c) *Budget row 3
  partial leg* (≤ 20 ms @ 1366×768 software renderer on a 1.5 GHz
  in-order kiosk core): judged against the fresh dev numbers with the
  standing caveats — dev-machine **references, never gates**; the
  on-device verdict belongs to the kiosk wave; the 100 ms repaint
  cadence (C2/F13-09) floors composed latency regardless. The refresh
  lane is reported and compared but never asserted against a budget
  (row 3 speaks about repaint).
- **D8 — Region decision rules (drive the verdict AND the #390
  disposition).**
  - **(a) TRUE-PARTIAL** — every N's region is card-scale (rule of
    thumb: ≤ ~10 % of the frame, one card's bbox ± small chrome),
    explainable row-by-row: the D9 fix direction reached the renderer's
    dirty region. Verdict: row 3's partial leg is measured as a true
    partial for the first time; #390 disposition = close as
    resolved-by-D9, residual = on-device D250 measurement (kiosk wave).
  - **(b) STILL-DIRTY** — any N's region ≥ ~50 %: the row-diff did not
    reach the dirty region on this markup. Verdict records the finding
    with a mechanism hypothesis (candidate: a per-pass fresh
    `ModelRc` — the flow-hint `string_model` and the task-detail
    `due_grid` are the two known non-diffed pushes; neither should be
    visible in the bench's Board view — plus anything the region lines
    themselves indicate). #390 stays open; stage 3 reports to the user
    instead of closing; the #390 comment uses the alternate branch
    text (§6.2).
  - **(c) PARTIAL-WITH-RESIDUALS** — small but > card-scale (roughly
    10–50 %) or N-dependent: enumerate which surfaces dirty (the
    region lines + one targeted look at which models changed) and
    judge the budget leg on the measured numbers with the residuals
    named. Disposition: stage 3 reports; closing is the user's call.
  - In all branches: `draw-full` is expected to be ~unchanged vs the
    baselines (same markup family + the mostly-gated notification
    surface) — a large `draw-full` shift is itself a reported finding,
    not smoothing material.
- **D9 — #373 re-justification is decision-grade, in the report.**
  Report §5 gives options (analyze ListView now / defer until on-device
  kiosk numbers land / drop), trade-offs grounded in the fresh numbers
  (a confirmed true partial removes the dirt argument against the
  status quo; the surviving motivation would be eager instantiation of
  all N cards — `draw-full` board-scale cost — and the 10 k tier's
  relation to the 50 ms full-frame envelope), and **one
  recommendation**. The analysis ask itself is never edited away
  (§7's body draft preserves it).
- **D10 — Tracker mechanics.** Stage 2 materializes §6/§7 drafts to
  temp files **outside the repo** (`/tmp`), fills every `<<FRESH:…>>`
  placeholder from the run, and verifies no `<<` remains before
  `gh issue comment 390 --body-file …` / `gh issue edit 373
  --body-file …`. Tracker writes are in-scope bookkeeping (dispatch
  prompt 7.2); no issue is closed by stage 2.
- **D11 — PR contents and gates.** The branch carries exactly two
  commits: this plan (stage 1) and the report (stage 2,
  `docs(report): …` — one standalone commit, rebase-merge hygiene).
  Stage 2's delta over the plan commit is exactly the report file
  (`git status` clean otherwise; temp files never enter the tree).
  Dispatch prompt 7.2's "PR diff is exactly the report file" is read
  as the stage-2 delta; stage 3 reviews the full branch diff (plan +
  report) per its own prompt. Gates: none apply beyond a clean tree —
  `cargo fmt --all -- --check` as the canary (docs-only change).
- **D12 — Bench numbers language.** Every number in the report and
  both issue texts is labelled a dev-machine reference; no budget row
  is asserted anywhere (MD-5; AGENTS §5 no-ceilings discipline does
  not apply — there are no tests here, only recorded references).

## 4. Stage-2 runbook (exact commands)

Work in `../taskboard-WT-docs-bench-true-partial-remeasure` (recreate
with `git worktree add <path> docs/bench-true-partial-remeasure` if
missing — never re-branch; the PR has not landed).

```sh
cd ../taskboard-WT-docs-bench-true-partial-remeasure

# T1 — build evidence at the measured commit (D4); record the commit hash
git rev-parse HEAD
cargo bench --no-run -p taskboard-ui-slint --bench refresh_draw

# T2 — machine-state record (D5), then the primary run, all 3 groups × 3 Ns
uptime; pgrep -a cargo; pgrep -a rustc
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor   # record, never change
# AC/battery state: cat /sys/class/power_supply/*/online (record; N/A if absent)
rustc -vV | head -3
cargo bench -p taskboard-ui-slint --bench refresh_draw -- --save-baseline true-partial \
    2>&1 | tee /tmp/refresh_draw_remeasure.log

# T3 — confirmation re-run, draw groups only (variance bound; no --save-baseline)
cargo bench -p taskboard-ui-slint --bench refresh_draw -- draw \
    2>&1 | tee /tmp/refresh_draw_remeasure_confirm.log
```

Expected wall time: T2 ≈ 5–10 min, T3 ≈ 3–5 min. Medians come from
`target/criterion/<group>/<n>/new/estimates.json` (the baselines'
stated source); the region lines come from the captured stderr. Both
runs' draw-lane numbers are reported (primary + confirmation) so the
report shows its own run-to-run spread instead of hiding it.

## 5. Report skeleton — `.artifacts/reports/2026-10-25-refresh-draw-true-partial-remeasure-report.md`

House report style (the 2026-10-20 stage-2 report is the model).

```
# [Report] True-partial re-measure — refresh+draw bench after r2 D9 (chunk 7)

- Date / Branch / Base commit / Machine (D5 record) / Outcome (one line)

## 1. Method (reproducible)
   - commands verbatim (§4); criterion configuration table (defaults vs
     the 10k tier); the stage-1 build evidence and the T1 result
   - machine-state record (D5); what the measured tree contains beyond
     the baselines (D9 = 70c812d — the subject; R3 chunks A/B/C +
     notification surface = named background drift; bench-code drift
     8015b6b/c0ece3e — instrument unchanged)

## 2. Fresh numbers (criterion medians [range], both runs)
   Table 1 + the verbatim [refresh_draw] region lines

## 3. Comparison vs the baselines (D6 — no cherry-picking)
   Table 2 (primary, 2026-10-20 §2, with delta %) + Table 3 (2026-10-21
   dev references as the variance envelope) + D250 slot-2 quoted
   context-only (pre-D9 upper bound, different CPU, not re-measured)

## 4. Verdict — perf-budget row 3, partial leg (D7/D8 branch rules)
   region sub-verdict → cost ordering → budget-leg sub-verdict;
   house caveats verbatim (references never gates; on-device verdict =
   kiosk wave; 100 ms cadence floors composed latency)

## 5. What survives of #373's motivation (D9)
   options / trade-offs / one recommendation

## 6. Issue updates applied (permallinks to the #390 comment and #373 revision)

## 7. Deviations from the stage-1 plan (none expected; any recorded here)

## 8. Acceptance checklist (this plan §7, stage-2 items)
```

### 5.3 Comparison table layouts

Table 1/2 shape (one row per lane plus the region row; Table 2 adds a
baseline column and a delta column per N):

| Lane | N=100 | N=1 000 | N=10 000 |
|---|---|---|---|
| `refresh` (load → reconcile → diff-push) | <<FRESH>> | <<FRESH>> | <<FRESH>> |
| `draw-full` (NewBuffer) | <<FRESH>> | <<FRESH>> | <<FRESH>> |
| `draw-partial` (ReusedBuffer) | <<FRESH>> | <<FRESH>> | <<FRESH>> |
| partial region (W×H, share of 1366×768) | <<FRESH>> | <<FRESH>> | <<FRESH>> |

Baseline anchor cells for Table 2 (2026-10-20 §2): refresh 27.2 µs /
229 µs / 2.60 ms; draw-full 2.82 / 6.24 / 44.0 ms; draw-partial
21.9 ms / 162.8 ms / 1.57 s; region 1351×760 ≈ 97 % at every N.

## 6. #390 comment drafts (D8 branches — stage 2 fills placeholders)

### 6.1 Branch (a) — TRUE-PARTIAL (expected; stage 3 closes on this branch)

> **True-partial re-measure (backlog-triage chunk 7) — fresh numbers after r2 D9**
>
> This issue's fix direction landed as r2 D9 (`70c812d`, "stable model
> identity via row-diff push"): `models::push` no longer replaces any
> `ModelRc` — persistent `Rc<VecModel>` handles are updated through a
> positional diff (`diff_rows`/`push_rows`,
> `crates/taskboard-ui-slint/src/models.rs`), so an unchanged refresh
> notifies nothing and a one-task change touches exactly one row.
>
> Re-measured `benches/refresh_draw.rs` at `<<COMMIT>>` (dev machine
> Ryzen 9 5900HX, criterion medians, 1366×768 software renderer,
> deterministic fixture — **references, never gates**):
>
> | Lane | N=100 | N=1 000 | N=10 000 |
> |---|---|---|---|
> | refresh | <<FRESH>> | <<FRESH>> | <<FRESH>> |
> | draw-full | <<FRESH>> | <<FRESH>> | <<FRESH>> |
> | draw-partial | <<FRESH>> | <<FRESH>> | <<FRESH>> |
> | partial region | <<FRESH>> | <<FRESH>> | <<FRESH>> |
>
> Pre-D9 (2026-10-20 chunk-6 report): region ≈ 97 % of the frame at
> every N and the partial lane cost **more** than full-frame (21.9 ms
> vs 2.8 ms at N=100; on the D250, 110.8 ms vs 31.1 ms). Post-D9:
> <<ONE-LINE VERDICT — region share, cost ordering, budget-leg
> reading>>.
>
> **Recommended disposition: close as resolved-by-D9.** Residual: the
> on-device D250 partial verdict (the slot-2 partial column was an
> upper bound under the old whole-model dirt) is carried by the
> kiosk wave's next on-device session. Report:
> <<REPORT-PERMALINK>>. Related: #373 (its stale premise figures are
> updated in the same pass).

### 6.2 Branch (b)/(c) — STILL-DIRTY / PARTIAL-WITH-RESIDUALS

> **True-partial re-measure (backlog-triage chunk 7) — the D9 row-diff
> did <<not reach / only partially reached>> the renderer's dirty region**
>
> D9 (`70c812d`) landed this issue's fix direction (positional
> row-diff onto persistent handles; zero notifications on an unchanged
> refresh). Re-measuring `benches/refresh_draw.rs` at `<<COMMIT>>`
> shows the refresh pass now diffs as designed, but the draw-partial
> region is <<MEASURED SHARES>> of the frame — <<mechanism hypothesis
> + evidence>>. Numbers and analysis:
> <<REPORT-PERMALINK>> (references, never gates).
>
> **Recommended disposition: keep open** — the true-partial verdict
> row 3 asks for is still unmeasured; next lever <<per report §4/§5>>.

## 7. #373 body edit draft (complete replacement body; D9/D10)

`gh issue edit 373 --body-file <tmp>` with exactly the text below
(placeholders filled; the analysis ask in §Ask is verbatim-preserved;
stale figures marked superseded and pointed at the report). The
changed parts vs the current body: the intro paragraph's stale table
and the "97 %" finding sentence are replaced by the supersession
note; Ask item 2's instrument parenthetical is refreshed; everything
else (Ask 1, Origin, the closing relates line) is unchanged.

> ## Summary
>
> The board composition renders columns as a plain `for` repeater
> inside nested layouts
> (`crates/taskboard-ui-slint/ui/compositions/board-layout.slint`) —
> **no `ListView`**. Every card at every board scale is eagerly
> instantiated, laid out, and traversed on every repaint; only the
> final rasterisation is clip-culled.
>
> **Superseded premise (2026-10-25):** this body previously carried
> the chunk-6 pre-D9 lane table and the "partial lane dirties ~97 %
> of the frame" finding. The r2 D9 row-diff push (`70c812d`) removed
> the whole-`VecModel`-replacement dirt; the true-partial re-measure
> (chunk 7) is authoritative — fresh numbers, the row-3 partial-leg
> verdict, and what survives of this issue's motivation:
> <<REPORT-PERMALINK>>. Headline for the virtualisation question:
> <<FRESH: draw-full at N=1 000/10 000, and whether partial now costs
> less than full>> — eager per-repaint traversal of all N cards
> remains the status-quo cost D9 does not remove.
>
> ## Ask
>
> 1. **Analysis first** (design round, not opportunistic): how
>    difficult is replacing the column `for` loops with `ListView`,
>    and what tradeoffs land with it. Known candidates to adjudicate:
>    - the keyboard-driven reading-order walk and focus model in
>      `view_state.rs` (a virtualised column has no items outside the
>      viewport — scrolling must follow focus, focus restoration on
>      model changes must be re-verified);
>    - `IdTables` index→id mapping assumptions (viewport-relative vs
>      board-relative indexing);
>    - scroll-position semantics across refreshes (whole-model
>      replacement resets/rescrolls — needs a story even unvirtualised,
>      but a `ListView` makes it visible);
>    - the bench contract: a virtualised board changes what N means
>      for `benches/refresh_draw.rs` — the draw lanes would measure
>      viewport scale, so an unvirtualised lane (or a size-scaled
>      variant) must be kept to keep board-scale cost visible.
> 2. **Implement if the analysis is positive**, with the partial
>    lane's reported dirty-region area as the success instrument
>    (post-D9 baseline: <<FRESH: region shares per N>>; a virtualised
>    board should push the *draw-full* lane toward viewport scale —
>    today it redraws all N cards regardless).
>
> ## Origin
>
> - True-partial re-measure report (chunk 7, supersedes the stale
>   figures above): <<REPORT-PERMALINK>>
> - Chunk-6 stage-2 report (the D6 finding as originally recorded):
>   `.artifacts/reports/2026-10-20-kiosk-chunk6-refresh-draw-bench-stage2-report.md`
> - Wave implementation plan §1.2 (the no-`ListView` seam finding) and
>   §3 K6/D10 (the "virtualisation changes what N means" note):
>   `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
> - Perf-budgets audit §2 row 3 / §3 S2 + C2 note (budget context,
>   100 ms cadence floor):
>   `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md`
>
> Relates to #300, #390.

## 8. Acceptance checklist

Stage 1 (this commit):
- [x] r2 D9 premise verified in source at the base commit (§2).
- [x] Bench builds green against main (f7b1ac8 evidence + T1 mandate
      at the measured commit, D4).
- [x] Doctor green; worktree created off fetched `origin/main` (D1).
- [x] Plan committed; no implementation, no push.

Stage 2:
- [ ] T1 build green in the worktree at the measured commit (red ⇒
      STOP and report, D4).
- [ ] Machine-state record captured (D5); primary run + confirmation
      re-run per §4; region lines transcribed verbatim.
- [ ] Report written per the §5 skeleton; all `<<…>>` placeholders
      resolved; every number labelled a dev-machine reference.
- [ ] #390 commented with the §6 branch text that matches the D8
      outcome; #373 body edited per §7; no issue closed.
- [ ] `git status` clean except the report file; fmt canary green;
      single standalone `docs(report)` commit; no push.

Stage 3:
- [ ] Rebase onto `origin/main`; full-diff review (plan + report);
      one bench lane re-run as spot-check (or the reason recorded).
- [ ] On the D8-(a) branch only: apply the #390 close as
      resolved-by-D9 with the on-device residual noted; anything else
      reported to the user instead.
- [ ] Worktree removed (`--force`, anchored to the main checkout),
      verified via `git worktree list` + `df` (D1).

## 9. Risks

1. **The region does not collapse to card scale** (D8 b/c): per-pass
   fresh `ModelRc`s exist on two small chrome surfaces (flow-hint
   keys, task-detail due grid) — neither should render in the bench's
   Board view, but Slint's dirty-region behaviour on struct
   re-assignment is exactly what this chunk measures. Mitigation: the
   branch rules pre-commit the disposition for each outcome; no
   post-hoc wording.
2. **Run-to-run variance** (the stage-3 lesson: +29 % on
   `draw-full/10000` same machine): mitigated by the T3 confirmation
   re-run and by leaning on the deterministic region instrument and
   cost *ordering* over absolute cells; deltas inside the 2026-10-21
   envelope are called noise, not change.
3. **Concurrent load on the shared dev box** (parallel dispatch
   chunks): the D5 record makes any concurrency visible in the
   report; numbers stay references.
4. **Worktree disk cost** (full `target/`, 36–55 G): doctor green at
   creation (70 G); stage-3 teardown mandatory and verified.
5. **`origin/main` moves again mid-chunk**: the deliverable is one
   docs file — the stage-3 rebase is conflict-free unless the report
   path itself collides; base is recorded either way.
6. **Base drift vs the baselines** (R3 code in the measured tree): the
   comparison is honest-by-labelling — the report names the drift
   (§5 skeleton §1) so no delta is silently attributed to D9 alone.

## Appendix A — D250 contingency (ONLY on explicit user scope expansion)

Not planned work (D2). If the user explicitly expands scope, the
method is the 2026-10-21 report's own appendix (F8 lesson: no
concurrent commits during the cross-build):

```sh
# dev box, clean settled tree, inside the worktree:
scripts/i686-cross-build.sh --bench refresh_draw
scp target-i686/i686-unknown-linux-gnu/release/deps/refresh_draw-* \
    aspire-net-srv:/tmp/refresh_draw
ssh aspire-net-srv 'chmod +x /tmp/refresh_draw && cd /tmp && ./refresh_draw --bench draw'
# criterion writes ./target/criterion under its CWD; capture stderr for the region lines
```

The on-device partial numbers would then be judged under the D250
slot-2 conditions (i686 cross-built criterion binary, software
renderer, 1366×768 frame) and compared to the 110.8 / 659.0 / 6 141 ms
pre-D9 upper bounds.
