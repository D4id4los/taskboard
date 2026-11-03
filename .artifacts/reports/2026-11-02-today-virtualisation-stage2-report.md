# Today virtualisation — chunk 4.1 stage-2 report (#461)

- **Date**: 2026-11-02
- **Branch**: `perf/ui-today-virtualisation` (worktree
  `../taskboard-WT-perf-ui-today-virtualisation`, base `origin/main`
  = `9f3508ee`), stage 1 plan
  `.artifacts/plans/2026-10-31-today-virtualisation-plan.md`.
- **Commits** (each standalone-green, pre-commit hooks passed):
  1. `ec9c7251` test(ui-slint): the today anchor holds under an
     insertion above the viewport (D-2 case 2)
  2. `e71c7dad` test(ui-slint): a true removal above the today
     viewport reflows by the removed extent (D-2 case 1, real)
  3. `fa3d6bcc` test(ui-slint): removing a live today row closes the
     list up without scrolling (D-2 case 3)
  4. `a1a1fe45` docs(bench): the today lanes' N semantics after
     virtualisation (D-4)
- **Production diffs: zero.** `git diff origin/main` = the two new
  test functions + helper (+312 lines in `tests/virtualisation.rs`),
  the bench doc block (+30 comment-only lines), and the stage-1 plan.
  No commit touches `today-view.slint`, `listview.slint`,
  `models.rs`, `adapter.rs`, or `view_state.rs`. The six today
  goldens pass byte-identical (D-6).

## 1. The headline finding: the anchor semantics were wrong, and are now pinned-actual

Stage-1 plan D-2 asserted the slint virtual repeater is
**row-anchored**: "a membership change above the viewport leaves
every on-screen row at its exact position". Empirically (slint
1.18.1, this repo's in-house `ListView`), the runtime is
**content-anchored**: the viewport's content offset is preserved, and
the live rows reflow by exactly the changed row's extent —

- **Removal above the viewport** (true case 1): visible rows shift
  **up** by the removed row's exact extent; rows pushed off the top
  drop out of the live window; the section header and page rest.
- **Insertion above the viewport** (case 2): visible rows shift
  **down** by the inserted row's extent, uniformly; the page rests.
- **Removal within the viewport** (case 3, the click-flip): the rows
  below close up by exactly the removed row's extent — the successor
  takes the removed row's old position — rows above rest, the page
  does not scroll. This is the user's-place semantics the dispatch
  prompt demands, and it holds.

This is stage-1 plan R1's exact scenario ("a pin that comes out red is
a finding against the semantics statement"): the first red run of the
insertion pin (rows moved +90 px) triggered the re-derivation. Per the
plan's own trap warning (stage-3 4.1.c), the pins encode the **pinned
actual** behavior, not the D-2 statement; the correction is recorded
here and in the pin doc comments. User-facing consequence to carry to
stage 3 / #461: a done flip *above* the viewport reflows the visible
rows by that row's height (up to ~70–90 px in the fixture) — the
viewport never teleports and never resets scroll, but the focused row
visually shifts by the changed row's extent. If that is judged a bug
against the desired row-anchored UX, the fix belongs to the ListView
mechanism (#373), not the Today markup, and is out of this chunk's
zero-production-diff budget.

### Sub-finding: the landed case-1 pin is vacuous

`a_removal_above_the_today_viewport_leaves_the_rows_resting`
(virtualisation.rs:583, landed with #511, kept untouched per the
plan) flips `bench_label(0)` — fixture order 0, which is `done` by
the every-10th pattern (`has_done`), so it was **never an Overdue
row**. Its flip inserts into the Done-today section (a separate,
lower ListView); the Overdue viewport is untouched and the pin passes
without exercising its case. The catalogue's case 1 is now pinned
properly by `a_true_removal_above_the_today_viewport_reflows_by_the_removed_extent`
(flip order 3, the real Overdue head, pure-removal stamp). Note the
plan's §4 arithmetic inherited the same error ("position 29 =
bench_label(87) — overdue is (order, id)-ordered over orders {0, 3,
6, …}"): overdue rows are the multiples of 3 **minus** the multiples
of 10 (orders 0, 30, 60, 90 are fixture-done), so position 29 is
order 93, not 87. The pins' comments carry the correct derivation;
fixture drift fails them loudly at setup, as R2 intended.

### Sub-finding: the break-notification ladder shares the labels

`find_by_label` (accessible-label search over the whole window) also
matches the break-notification ladder rows (window bottom, ~y 751),
which list Overdue-head tasks. Deep rows (e.g. 87) resolve uniquely
to their section row (count = 1 verified); head rows can double-match.
The pins measure only uniquely-resolved elements; recorded so future
pins know the instrument's aliasing.

## 2. The pins and their invert-checks (plan §4 protocol)

| Pin | Case | Green | De-virtualisation (`ListView`→`ListViev` scratch rename) | Eager swap (section `ListView` → plain clipped `for`) |
|---|---|---|---|---|
| `an_insertion_above_the_today_viewport_reflows_by_the_inserted_extent` | 2 | ✓ | FAILS at the reflow assertion ("got shift 0") | FAILS, same assertion ("got shift 0") |
| `a_true_removal_above_the_today_viewport_reflows_by_the_removed_extent` | 1 | ✓ | PASSES (shape-independent: an eager layout also re-solves from the top, shift == extent) | (same reasoning; not separately run) |
| `a_removal_within_the_today_viewport_closes_the_rows_below_without_scrolling` | 3 | ✓ | FAILS at "the successor row takes exactly the removed row's old position" | FAILS, same assertion |

Both scratch shapes were reverted before any commit (working tree
checked clean). Recording per the plan's D-3/#373 precedent: the
case-1 pin's discriminating power is semantic (exact-extent
measurement), not engagement — engagement stays with the structural
probes (`the_today_sections_virtualise` and the ListView-contract
probes), which is precisely D-4's detection-duty disposition.

All scratch edits used the documented
`SLINT_EMIT_DEBUG_INFO`-enabled build (`build.rs` compiles
`virtualisation_probe.slint`, `walk_probe.slint` — the rename had to
cover those too; the compiler caught the inconsistent first attempt,
which is itself the F1 contract behaving).

## 3. Bench evidence (M2/M3): draw cost decoupled from list extent

Fresh run on this branch (dev box, load average ~4 — references,
never gates; regions deterministic):

| Lane | N=100 | N=1 000 | N=10 000 |
|---|---|---|---|
| `draw-full/today` | 5.07 ms (4.61–5.60) | 1.53 ms | 1.61 ms |
| `draw-partial/today` (region 1342×624 = 79 % of frame, all N) | 7.10 ms | 8.30 ms | 7.04 ms |
| `refresh/{n}` (the #463 guarded no-op path) | 0.41 µs | 0.41 µs | 0.39 µs |

Decoupling confirmed: today draw cost is viewport-scale and
N-independent at every N (the N=100 draw-full outlier is noise on a
busy box — it is *higher*, not lower, and carries no scaling signal;
the budget-relevant reading is N = 10 000 at 1.61 ms against the
pre-#511 eager 19.9 ms, and draw-partial 7–8 ms against the eager
903 ms removal-reflow class). The visible-flip guard reported a
nonzero region at every N. The `refresh` lane reading is newly the
#463 guard's cost (recorded in the bench doc, commit 4): the lane's
routine never changes state, so the derive+diff N-cost sits in the
draw lanes' unmeasured setup — an honest lane-shape note this chunk
inherits, not introduces.

D-4's kept-reference contract is discharged structurally (probes +
recorded baselines); no live unvirtualised lane exists, and the doc
block now says so with the reasons (D-4, unchanged).

## 4. Gate suite (M4) and green sweep (M3)

Run through `scripts/run-quality-gates.sh` (flock wrapper) from the
worktree — per-gate results in the dispatch report; summary:

- `cargo fmt --all -- --check`: clean (one trailing-newline fix
  applied before the first commit).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
  (three `doc_markdown` backtick fixes on the new doc comments).
- `cargo nextest run --workspace`: **2919/2919 passed, 41 skipped**
  (re-run at 2026-11-03 01:00 CET, after the calendar window closed;
  first attempt aborted by the flake below), including the full
  today journey matrix (`the_today_view_walks_completes_and_recommits`
  unmodified), the model-identity and repetition probes, and the
  kiosk-rotation/today-mode goldens byte-identical (D-6).
- `cargo test --doc --workspace`: green.
- `cargo deny check`: green.
- llvm-cov three-command sequence with
  `scripts/coverage-quarantine-profraw.sh` and the trailing
  `rm -rf target/llvm-cov-target`: green (one torn profraw from a
  killed writer quarantined exactly per the 2026-10-13 protocol;
  lcov.info written, `target/llvm-cov-target` removed).

The first gate attempt (2026-11-02 23:16 UTC / 00:16 CET) aborted at
nextest on the pre-existing #560 wall-clock flake
(`h02_log_receipts…`, local-vs-UTC calendar window): verified
identical on unmodified `main` @ `9f3508ee`, chunk diff cannot affect
it (ui-slint tests/docs only), observation recorded as a comment on
[#560](https://github.com/D4id4los/taskboard/issues/560) per the
flaky-test discipline; no opportunistic fix.

## 5. Deviations from the plan

1. **The anchor pins' semantics were corrected to the pinned actual**
   (content-anchored reflow by the changed row's exact extent) instead
   of D-2's "rows resting". Reason: the actual runtime demonstrably
   differs; the plan's own R1 and stage-3-trap clauses prescribe
   pinning the actual and reporting the finding. Consequence: the
   insertion pin's name (`…_reflows_by_the_inserted_extent` vs the
   plan's `…_leaves_the_rows_resting`) and both above-viewport pins'
   assertions differ from §4 T-1/T-2's literal form; case 3 matches
   the plan's semantics exactly.
2. **A third pin was added** (`a_true_removal_above…`) beyond the
   plan's "two test functions". Reason: the landed case-1 pin is
   vacuous (§1 sub-finding); without a real case-1 pin the catalogue
   is incomplete, which M1 makes stage-2's duty.
3. **The plan's "never pixel numbers" instrument clause** is satisfied
   with *measured* extents (pre-walk on-screen measurements), never
   hardcoded pixel constants.
4. **CHANGELOG: no entry**, per stage-1 plan D-7 (the user-visible
   virtualisation entry landed with #511; this chunk adds no
   user-observable change). The dispatch prompt's "one performance
   entry" line is resolved by D-7's recorded decision. No production
   fix surfaced (R1's invert-checks found no production bug — the
   findings are semantics-statement and vacuous-pin corrections), so
   D-7's conditional entry did not trigger either.
5. **T-3 extended** with the #463-guard lane note: the plan's T-3
   text ("what N measures now") would have misstated the refresh
   lane, which since #463 measures the guarded no-op path. Same
   commit purpose, comment-only.

## 6. Handoff to stage 3 (4.1.c)

- Re-read the three anchor pins against the corrected semantics
  statement above (the dispatch prompt's "a jump is a bug" reading:
  case 3 keeps the user's place exactly; cases 1–2 reflow by the
  changed row's extent under a fixed content offset — decide whether
  that residual reflow is a bug to file against #373/#461 or the
  accepted ListView contract).
- Consider commenting the vacuous-landed-pin finding on #461 (the
  closed-issue archive discipline: the pins and this report are the
  record).
- The usual ship steps (push, PR `perf(ui-slint): the Today sections
  virtualise`, green checks) apply unchanged.
