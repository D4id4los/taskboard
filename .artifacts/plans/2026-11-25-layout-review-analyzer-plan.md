# The Layout Review Analyzer — Chunk Plan (round 7, chunk G3, #1313)

- **Date**: 2026-11-25
- **Chunk**: G3 of `2026-11-25-backlog-reduction-round7-dispatch-plan.md`
  §11 (the capstone); branch `test/layout-review-analyzer`; issue #1313
  is the body-normative source.
- **Normative inputs**: the scope report
  `.artifacts/reports/2026-11-25-backlog-reduction-round7-scope-report.md`
  §A row G3; the dispatch plan §1/§11; the golden-coverage review report
  (`.artifacts/reports/2026-11-21-golden-coverage-review-report.md`) §4/§6
  (the original recommendation: an offline analysis pass, not a CI gate).
- **Landed base** (origin/main 8c2341ca): G1's deterministic PNG encoder +
  byte-identity pin, the 63-golden corpus re-based on the enriched fixture
  with the wave-2 additions, the #1140 manifest
  (`crates/taskboard-ui-slint/tests/goldens/manifest.toml` + the
  `golden_manifest` bijection test), the #1139 comparator
  (`support/golden_compare.rs`), the Theme token ladder
  (`crates/taskboard-ui-slint/ui/theme.slint`), the widget sizing
  taxonomy (`docs/widget_interactions.org`).
- **Round laws honored**: (a) typed findings (§3/D4); (c) advisory-first —
  the analyzer reports, it never gates (§2/D6); (d) deferred invariant
  ideas became backlog issues at planning time (§2/D10, numbers in the
  final report).

## 1. Seams (read and confirmed)

- **Corpus**: `crates/taskboard-ui-slint/tests/goldens/` holds 63 PNGs
  (64 files incl. `manifest.toml`), named
  `<surface>-<state>-<scale>-<width>.png` at heights of 900 (content
  surfaces) or the surface's own size (icon probe 396, alarm window).
- **Manifest** (`[[golden]]` entries): `name`, `fixture` (constructor
  call, not prose), `state` (structural state note), `axes = { scale,
  width, height }`. The `state` note is the per-surface context the
  analyzer quotes in findings — geometry alone cannot tell a deliberate
  full-bleed backdrop from a clipped pane; the note disambiguates where
  the rubric needs it (D7/D9).
- **Comparator** (`support/golden_compare.rs`): scale-aware box-averaged
  cell grid (`CELL_TARGET_PX = 24`, `MIN_GRID = 8`), luma + integer
  Rec. 601 chroma channels, budgets `MAX_CELL_DIFF`/`MAX_CHROMA_CELL_DIFF`
  = 8 — evidence-commented constants, never tuned per golden. The
  analyzer borrows the *method style* (scale-derived tolerance,
  evidence-commented thresholds) but is a separate offline pass; it does
  not import test-support code (D1).
- **Theme ladder** (`ui/theme.slint`): spacing tokens
  `space-1..8` = {4, 8, 12, 16, 24, 32, 48, 64} px × `base-scale` (plus
  `space-15` = 96), typography
  `TextRole` sizes = display 30, title 20, heading 16, body 14, caption
  12, badge 11, mono 12.5 (all × `base-scale`), plus `icon-size` 12,
  `hairline-width` 1, `radius-small` 4, `radius-medium` 8. These are the
  ladders INV-T1 and INV-M1 check against.
- **Corpus raster facts** (measured at planning time): the theme is
  dark — modal background RGB (29, 32, 40), content lighter (mean luma
  ~27–39). Background-keyed content extraction is therefore reliable:
  the modal color is sampled per golden (corners), content is luma
  deviation from it. No fixed palette assumptions.
- **Precedent for the tool form** (D1): `scripts/check_slint_tokens.py`
  — a standalone `scripts/` tool with a `--self-test` mode, no CI wiring,
  exit code reserved for the tool's own use.

## 2. Recorded decisions

- **D1 — Tool form: `scripts/layout_review.py`** (Python 3, PIL + numpy
  + stdlib `tomllib`; both verified present on the dev box). Rationale:
  (i) the diff stays confined to tooling + report — no Cargo member, no
  new crate, no production code, no test wiring to accidentally gate;
  (ii) `png` decode in Rust would drag a workspace member or a duplicate
  manifest parser; (iii) direct precedent (`check_slint_tokens.py`);
  (iv) PIL is only a dependency of the tool's runtime, not of any build.
  The script header carries `# SPDX-License-Identifier: MIT OR Apache-2.0`
  (it is a library-tier scripts tool by the license tiers; scripts/ has
  no precedent header — recorded here as the decision, header included
  in the diff).
- **D2 — Input contract**: the goldens directory path +
  `manifest.toml` + the Theme ladders. The Theme ladders are *not*
  parsed out of `theme.slint` (regex-parsing Slint is brittle); they are
  recorded as constants in the script, each annotated with its
  `theme.slint` line reference and a drift note: if `theme.slint`
  changes a token, the script's constant is updated in the same commit —
  the example report regeneration is the tripwire. (Rejected:
  a slint-side generated table — production/tooling coupling not worth
  it for an advisory pass.)
- **D3 — Invariant catalogue**: four committed invariants — A1
  alignment grid, A2 uniform gaps, T1 font-size ladder, M1 margin
  conformance, plus C1 clip sentinel (five total; catalogue in §3). The
  two additions (M1, C1) and their rationale are recorded in §3. The
  considered-and-rejected ideas are backlog issues (D10).
- **D4 — Findings record shape (typed, machine-readable)**: the tool
  writes `findings.toml` next to the report (path in §5), one record per
  finding:
  ```toml
  [[finding]]
  golden = "today-0.5-1000"          # manifest name
  surface = "today"                  # name's first segment
  invariant = "A1"                   # catalogue id
  grade = "fix-later"                # accept-as-legacy | fix-now | fix-later
  region = { x = 213, y = 40, w = 12, h = 380 }   # physical px
  logical_region = { x = 426, y = 80, w = 24, h = 760 }  # ÷ scale
  metric = "left edge at 213 px; nearest grid column 208 px (delta 5 px)"
  threshold = "cluster tolerance 2 px * scale 0.5 = 1 px; grid min membership 2"
  ```
  Every finding is graded, located (region), and cites its measured
  metric + threshold — never prose-only. `findings.toml` is the source
  of truth; the markdown report is generated from it (`--emit-md`).
- **D5 — Grading rubric** (applied per finding during the run, recorded
  in the report):
  - `fix-now` — a defect class the project already tracks as a bug
    (e.g. a clip signature on a surface with an open severing issue), or
    an off-ladder font size that is not a rendering artifact.
  - `fix-later` — a real drift from the invariant (off-grid alignment,
    gap outlier, non-token margin) with no open issue; the finding is
    the intake record for a future `backlog:` issue.
  - `accept-as-legacy` — the deviation is load-bearing in the committed
    golden (fixing it would change an approved layout) or the manifest's
    state note explains it (e.g. a deliberate full-bleed backdrop under
    C1). The grade names the debt; it never silences the detector.
- **D6 — Advisory-first posture (binding)**: no CI wiring, no nextest
  test, no pre-commit hook, no gate reference anywhere in the diff. The
  tool is run by hand; its exit code is 0 whenever the run completes
  (findings are data, not failures — a nonzero exit only for tool
  errors: unreadable corpus, manifest mismatch). The report header
  states the posture explicitly.
- **D7 — Threshold authority**: every tolerance is a module constant
  annotated with the evidence for its value (the scale it assumes, the
  rendering slop measured in stage 2's calibration run) and is expressed
  in logical px × the manifest `scale`, mirroring the comparator's
  evidence-commented constant law. Thresholds are calibrated once on the
  corpus (measure the noise floor first, then set), never widened
  per-finding; a finding that looks wrong is a rubric/grade answer, not
  a threshold edit.
- **D8 — Report home & shape**:
  `.artifacts/reports/2026-11-25-layout-review-analyzer-findings.md`
  (generated) + `findings.toml` beside it. Report: posture header, the
  run command, headline counts (per invariant × per grade), then one
  table per golden with findings (golden, invariant, grade, region
  summary, metric). The example report committed on the branch is the
  real run's output over the real 63 goldens (stage-3 re-runs and
  compares).
- **D9 — Scale & region conventions**: all thresholds in logical px,
  multiplied by the manifest `scale` before use; regions reported in
  both physical and logical px (D4). Content-sized surfaces (icon probe
  396, alarm window) are analyzed the same way; their smaller canvas is
  derived from the decoded PNG, not the manifest convention.
- **D10 — Rejected/deferred invariant ideas → backlog issues** (opened
  at planning, numbers in the final report):
  - Icon-size conformance (bounding boxes of glyph-sized components vs
    `icon-size` 12 px × scale) — unreliable at 12-px glyph sizes with
    the box-averaging approach; needs a dedicated icon-probe analysis.
  - Contrast/hue invariants (text-vs-background contrast floors, tone
    ladder conformance) — design review territory, not layout; the
    comparator already pins hue *changes*, this would judge absolute
    quality.
  - Row-pitch uniformity *across* surfaces (cross-golden margins
    comparison) — deferred until A2's per-surface version has run once;
    premature cross-surface coupling at planning time.

## 3. Invariant catalogue

Detection is background-keyed everywhere: the modal color (sampled from
the four corner regions) is the background; "content" is any pixel whose
luma deviates from it by more than the content floor (calibrated
constant, expected well under 20/255). Column and row projections are
the workhorse: `col[x]` = count of content pixels in column x, `row[y]`
likewise.

- **INV-A1 — alignment grid** (left edges / margins consistency).
  Detect vertical edges: x positions where the column gradient
  (`|col[x+1] - col[x]|`) exceeds the edge floor — the left/right
  boundaries of cards, panes, text blocks. Cluster the *left-facing*
  edges with tolerance `t = 2 px × scale`. Every cluster with a single
  member (an edge no other edge shares) whose region is wider than the
  min-block width (12 px × scale, so glyph-level noise is exempt) is a
  finding: the block sits off the grid. Region: the offending block's
  bounding column range × its content rows.
  Self-test: a synthetic fixture with three left-aligned blocks is
  clean; one block shifted by 3 × t produces the finding naming that
  block's region; a shift within t does not (tolerance pin).
- **INV-A2 — uniform gaps** (per-section spacing consistency). Segment
  content bands from the row projection (maximal runs of rows above the
  band floor). Adjacent band gaps within a section (a section = a run of
  bands whose gaps are all under the section-gap ceiling, 96 px × scale
  = `space-15`) must be uniform: gaps with |gap − median| > `t = 2 px ×
  scale` AND median membership ≥ 3 bands are findings; the outlier gap's
  inter-band region is reported. Rationale: Slint layouts space siblings
  with one token; a one-row offset (#1334's class) is exactly a gap
  outlier.
  Self-test: five bands at even 24-px (logical) pitch are clean; one
  24-px pitch replaced by 32-px produces the finding between the right
  bands; two bands only (membership < 3) produce nothing (membership
  pin).
- **INV-T1 — font-size token conformance**. Within each content band,
  find glyph-row heights: connected runs of content rows whose
  sub-projection is text-like (high run count, narrow rows) yield a
  text-line height estimate (the modal connected-component height in
  the band, which for a text line is the x-height-to-cap span ~ 0.70 ×
  em). Map each estimate to the nearest ladder size
  {11, 12, 12.5, 14, 16, 20, 30} px × scale with tolerance ±15% (the
  rendering slop the calibration run measures; pinned as an
  evidence-commented constant). Estimates that map to no ladder rung
  are findings: the size was not drawn from the `TextRole` ladder.
  Region: the offending text line's bounding box. Small samples (< 5
  text lines in a golden) suppress the invariant rather than report —
  the estimator needs population for the modal-height method to be
  honest (recorded suppression, shown in the report as `T1: skipped
  (n=3 lines)`).
  Self-test: synthetic text-line strips at 14 px × scale and 11 px ×
  scale are clean; a 13 px × scale strip produces the finding; a
  golden-stub with 2 lines produces no finding and a skip note.
- **INV-M1 — margin conformance** (recorded addition). The outer
  margin — distance from each canvas edge to the first/last content
  row/column — must equal a `space-*` ladder rung {4, 8, 12, 16, 24,
  32, 48, 64, 96} px × scale within ±1.5 px × scale. Rationale: window
  padding is tokenized at the grammar level; a drifted margin is a
  token-lint escape (#1361's class) that pixel space can see and grep
  cannot. Full-bleed exceptions exist (kiosk backgrounds, the icon
  probe's kind wall) — the manifest state note is the disambiguator per
  D5/`accept-as-legacy`; the calibration run enumerates the exceptions
  once, into the rubric, not into per-golden waivers.
  Self-test: a fixture with a 16 px × scale margin on all edges is
  clean; a 18 px × scale left margin (between rungs) produces the
  finding on that edge; a 24-px margin (a rung) is clean.
- **INV-C1 — clip sentinel** (recorded addition). Content pixels within
  `1 px × scale` of a canvas edge whose run *touches* the edge (i.e.
  the content run is cut off, not merely near it) are a clip signature:
  the severing class (#1378/#1334) ends with content flush to the
  canvas. Region: the touching run's edge strip. Full-bleed backgrounds
  are exempt by construction — they are background-keyed out (the
  background is the modal color; a full-bleed *non-modal* backdrop is
  distinguished by chroma flatness, per the #1139 comparator's channel
  method). Rationale: this is the one invariant that detects a probable
  bug rather than style drift, hence the only default `fix-now` rubric
  line.
  Self-test: a block clipped by the right canvas edge produces the
  finding; the same block fully inside the canvas is clean; a uniform
  non-modal full-bleed field produces nothing (backdrop pin).

Order of analysis per golden: decode → background calibration → C1
(edge touching, cheapest and most severe) → M1 → A1 → A2 → T1. Every
invariant runs independently; one detector's failure cannot suppress
another's findings.

## 4. Self-test method

`scripts/layout_review.py --self-test` builds synthetic raster fixtures
in memory (PIL draw, deterministic: fixed seed, no fonts — glyph strips
are drawn as rectangles with text-like run statistics, since real font
rendering would couple the test to fontconfig) and asserts, per
invariant, the constructed pair from §3: compliant → no finding,
misaligned → exactly one finding with the expected invariant id and
region. Each invariant's negative test pins the tolerance too (the
within-tolerance shift finds nothing). The self-test is the constructed-
case proof stage 3 reviews; it is not wired into any test runner (D6) —
it runs on invocation like `check_slint_tokens.py --self-test`.

## 5. The example report

`.artifacts/reports/2026-11-25-layout-review-analyzer-findings.md` +
`findings.toml` in the same directory, produced by:
`scripts/layout_review.py crates/taskboard-ui-slint/tests/goldens
--emit-md .artifacts/reports/2026-11-25-layout-review-analyzer-findings.md`.
Contents: posture header (advisory; never a gate), the exact command,
the calibration note (noise floor measured, thresholds set), headline
counts (invariant × grade matrix), then per-golden tables for every
golden with ≥1 finding, and the explicit skip notes (T1 suppressions).
The committed file is the real run's output — regenerated, never
hand-edited.

## 6. Stage-2 outline

1. `scripts/layout_review.py`: constants + manifest reader + background
   calibration + the five detectors (§3) + findings.toml writer +
   `--emit-md` + `--self-test`.
2. Calibration run over the corpus (background stability, content
   floor, text-line population); pin the evidence-commented thresholds.
3. Full run; grade every finding per the D5 rubric; enumerate the
   M1/C1 full-bleed exceptions into the rubric section of the report.
4. Generate and commit the example report; each commit standalone
   (`feat(scripts): …` / `docs(reports): …`).
5. Gate: pre-commit hooks +
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (docs/scripts-only diff, but the law is the law). Every cargo
   invocation through `scripts/memory-gate.sh`.

## 7. Risks

- **False positives on the real corpus** (badge clusters, chips, the
  icon probe's deliberate grid) — the primary risk of pixel-level
  invariants. Mitigation: the calibration run measures the noise floor
  before thresholds are set (D7); the min-block-width and population
  suppressions; the grade, not the detector, absorbs explained
  deviations. Accepted: the example report may carry many
  `accept-as-legacy` findings — that is the posture working, not
  failing.
- **T1's glyph-height estimator is the weakest link** — rectangle-strip
  synthetic fixtures cannot prove it against real font rasterization;
  only the corpus run can. Mitigation: population suppression (< 5
  lines) and the ±15% tolerance; if the corpus run shows the estimator
  cannot separate ladder rungs honestly, T1 ships suppressed with the
  evidence recorded in the report (an invariant that cannot measure
  honestly is decoration — the plan prefers an honest skip).
- **Script/runtime coupling**: PIL/numpy versions on the dev box only;
  the tool is dev-machine tooling like the rest of `scripts/`, not a
  release artifact — recorded, not mitigated further.
- **Threshold drift across stages**: stage 3 re-runs and compares; the
  thresholds are constants in the committed script, so reproducibility
  is byte-level by construction (the corpus is byte-deterministic per
  G1).

## 8. Estimated diff size

`scripts/layout_review.py` ~700–900 lines (detectors + self-test + md
emitter), the example report + `findings.toml` generated (report ~200–400
lines depending on finding density), this plan. No Rust, no CI, no
manifest or golden changes. Total hand-written ~800–1000 lines; total
diff ~1100–1400.
