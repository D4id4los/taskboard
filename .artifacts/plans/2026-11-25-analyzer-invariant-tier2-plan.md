# The Analyzer Invariant Tier-2 Chunk Plan (round 8, chunk L2)

- **Date**: 2026-11-25
- **Branch**: `test/analyzer-invariant-tier2` (worktree
  `../taskboard-WT-test-analyzer-invariant-tier2`, start point `origin/main`
  @ d7beae66)
- **Covers**: #1417 (the connected-component text-line estimator — T1
  upgrade), #1410 (the contrast invariant, tier 2), #1409 (the icon-size
  conformance invariant)
- **Scope report**: `.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md`
  §A-L2 (normative). Dispatch plan §1, L2 row.
- **Precedents**: the G3 analyzer plan
  (`.artifacts/plans/2026-11-25-layout-review-analyzer-plan.md`) — the
  detector/threshold/constructed-case architecture; its findings report
  (`.artifacts/reports/2026-11-25-layout-review-analyzer-findings.md`) — the
  typed record shape and the calibration posture; the #1418 classifier plan
  (`.artifacts/plans/2026-11-25-clip-sentinel-law-plan.md`, chunk L1 —
  **not yet landed at stage-1 time; L1 runs in parallel**. Its typed-
  classification precedent is taken from the dispatch plan §2 L1 text:
  typed finding records, never prose verdicts); `token_tests.rs`
  (`crates/taskboard-ui-slint/src/token_tests.rs`) — the WCAG 2.x
  relative-luminance / contrast-ratio method #1410 adopts verbatim.
- **Laws**: advisory-first posture carries over (the analyzer reports, it
  never gates — no CI wiring, no nextest test, no pre-commit hook);
  constructed cases both ways per invariant (the G3 precedent); typed
  outcomes — every finding is the existing finding-dict record shape;
  thresholds evidence-commented, calibrated once, never widened after
  grading.

## 1. Diagnosis — what exists and what fails today

### 1.1 T1 today (#1417's failure mode)

`detect_t1` (`scripts/layout_review.py:415`) estimates text-line em from
**row-run heights within content bands**: bands() over row counts, then a
half-floor re-band inside each band, one height per merged horizontal run.
Three recorded failure modes (issue #1417, the findings report's 53 T1
rows):

1. **Non-text bands produce garbage em.** Chips, icon walls and capture
   thumbnails are horizontally contiguous, so a whole band collapses into
   one "line" whose height is the band height — calibration measured em
   estimates up to 1806 logical px. The shipped mitigation is the
   `MAX_LINE_EM_LOGICAL = 30 × 1.15` exclusion cap, which throws the
   signal away instead of classifying it.
2. **Hairline rules masquerade as text.** 1–3 px divider rows pass
   `MIN_LINE_RUN_PX = 3` and land as em ≈ 1.4–4.3 findings, all graded
   accept-as-legacy by hand (43 of 53 rows).
3. **Mixed bands merge.** A title (30 px) directly above a caption (11 px)
   in one band measures as a single ~41 px union run — neither rung.
4. **Population suppression**: `T1_MIN_LINES = 5` skips sparse goldens
   with a recorded note; honest but broad, because row-run line detection
   cannot be trusted on small populations.

### 1.2 Contrast: nothing exists in the analyzer (#1410's gap)

Decision D10 of the G3 plan deferred contrast/hue entirely: "design-review
judgments, not layout invariants". The typed method already exists in Rust
(`token_tests.rs`): WCAG 2.x relative luminance over 8-bit sRGB,
contrast ratio `(hi+0.05)/(lo+0.05)`, role classes Body ≥ 4.5,
Large ≥ 3.0, NonText ≥ 3.0. But `token_tests` judges **token pairs as
declared** — it cannot see what actually renders (the #1415 chip bug is
invisible to it: the pair list has no chip-text-on-label-colour entry).
The golden pixels are the ground truth the analyzer can judge.

### 1.3 Icon size: the box-averaging detectors cannot see glyphs (#1409's gap)

`Theme.icon-size: 12px * base-scale` (`ui/theme.slint:92`). G3's A1/A2
detectors average over row/column projections; a 12-px glyph box is below
`MIN_BLOCK_WIDTH_LOGICAL = 12` (excluded as chip noise by design). The
icon-probe goldens (`icon-probe-0.5-396`, `icon-probe-1.0-396`,
`icon-probe-3.0-396` — the kinds wall) are exactly the corpus where icon
sizes are the subject, and none of the five invariants measures them.
#1409 notes the dependency: a **component-level estimator first** — which
#1417 delivers.

### 1.4 The composition (why this chunk is one chunk)

The connected-component pass of #1417 is the shared foundation:

- T1 upgrades its **line grouping** to per-glyph clusters → honest em.
- I1 (#1409) reads **component bounding boxes** directly → icon-size
  conformance on the icon-probe goldens.
- Q1 (#1410) samples **ink vs local background inside each detected text
  line** → pixel contrast with a WCAG floor.

One new primitive (`connected_components`), three invariants on top.

## 2. Design per invariant

### 2.1 INV-T1 upgrade — connected-component text-line estimator (#1417)

**New primitive** `connected_components(mask) -> list[(x0, y0, x1, y1)]`:
8-connectivity flood fill (two-pass union-find or BFS with a visited
array — pure numpy + a small stack; the corpus is ≤ 1500×900, probe
fixtures tiny). Components touching the canvas edges are passed through
unchanged (C1 owns edge semantics).

**Text-ness classification of a component** (replaces the
block/image exclusion cap with classification, not exclusion):

- `GLYPH_MAX_HEIGHT_LOGICAL = 34.0` — a component taller than the
  display rung's tolerance ceiling (30 × 1.15 = 34.5, kept as a named
  constant just below it) is a block/icon/thumbnail, not a glyph cluster.
  Evidence: the 1806 px calibration garbage is now *classified out at the
  component level*, where the row-run method could not see the split.
- `GLYPH_MAX_WIDTH_LOGICAL = 40.0` — a glyph cluster wider than ~3 em of
  the display rung is a rule/divider or a connected icon run, not text
  ink. Evidence-commented at calibration; icons in the kinds wall are
  ≤ 12 logical px wide, dividers are wide-and-thin.
- Height ≥ `MIN_LINE_RUN_PX` (3 px) — retained from T1.

**Line grouping**: components whose vertical centres differ by ≤
`LINE_CENTRE_TOL_FRACTION = 0.35` of the taller component's height AND
whose horizontal gap is ≤ `LINE_X_GAP_LOGICAL = 6.0` logical px
(× scale; word/letter spacing slop — evidence-commented) merge into one
line. A line's height is `max(y1) − min(y0) + 1` over its member
components; its em is `height / EM_FRACTION / scale` as today.

**What is kept from T1**: `TEXT_LADDER`, `EM_FRACTION = 0.70`,
`TEXT_TOL_FRACTION = 0.15`, the rung-matching loop, the finding record
shape (metric now reports the component count and per-line height, and
the threshold string names the new estimator). The `MAX_LINE_EM_LOGICAL`
cap **deletes** — its job is done honestly by the glyph classification.

**What changes about suppression**: `T1_MIN_LINES = 5` stays (a golden
with < 5 honest glyph lines still cannot carry a ladder judgment; the
note stays recorded, never silent).

**Grading rule update** (the report's rubric section): the hairline class
disappears at the source (dividers classify out as too-wide components),
so the old "em < 20 → accept-as-legacy" rule shrinks to genuinely small
text; the rubric line is rewritten in `emit_md` and the findings re-run
re-grades from scratch (advisory data, hand-applied grades are per-run).

### 2.2 INV-Q1 — text contrast floor (#1410)

**Method (the typed method L4 pins against)**: WCAG 2.x relative
luminance and contrast ratio, **transcribed from `token_tests.rs`
`relative_luminance`/`contrast_ratio` byte-for-byte into Python** — the
anchor is WCAG 2.x SC 1.4.3 and the token test suite is the single
definition of the formula in this repo; two implementations of one
formula is the composition contract (L4's stage-1 plan will cite this
method; the Rust side already tests the formula's known-answer pins
21.0/1.0).

**Measurement over a golden**: for each T1 text line (from §2.1):

1. Within the line's bounding box (padded 2 physical px), content pixels
   split by luma into ink (the darker-or-more-background-deviant mode —
   take the modal colour of the pixels farthest from the sampled
   background, the extreme 25% by channel deviation) and local background
   (the modal colour of non-content pixels in the padded box; fallback to
   the global corner-sampled background).
2. `ratio = contrast_ratio(ink, background)`.
3. Floor by role class: line em ≥ 20 logical px (title/display rungs) →
   `Large` ≥ 3.0; otherwise `Body` ≥ 4.5. (The NonText class has no
   pixel analogue in this invariant; icons are I1's subject.)

Anti-aliasing is handled by the modal-ink choice (the *core* glyph
pixels, not the blended edge), evidence-commented at calibration.

**Typed finding record** (never a prose verdict): `invariant: "Q1"`,
region = the line's box, metric =
`"ink RGB (r,g,b) on background RGB (r,g,b) → contrast 2.1:1; role class Body"`,
threshold = `"WCAG 2.x: Body ≥ 4.5:1 (em < 20), Large ≥ 3.0:1 (em ≥ 20); method per token_tests.rs"`.

**Grading default**: `fix-now` — a sub-floor ratio is the accessibility
defect class (the #1415 chip bug at 1.1–2.1:1 is this class; L4 consumes
exactly these findings). 4.5 ≤ ratio is clean; there is no fix-later band
inside a floor (a floor is binary), so Q1's rubric entry is one line.

**Hue**: the tone-ladder conformance half of #1410 stays deferred — the
#1139 comparator pins hue *changes*; absolute hue judgment needs the
Semantic palette's intent in a consumable form. Recorded on #1410 as the
remaining open half; **not** built in this chunk.

### 2.3 INV-I1 — icon-size conformance (#1409)

**Scope guard**: the invariant runs only on goldens whose manifest name
starts with `icon-probe-` (the kinds-wall probes — the corpus where icon
size is the subject). Other goldens are full of glyph-sized chips whose
sizes are not icon tokens; judging them would drown the signal. The
scope guard is stated in the threshold string of every I1 finding and in
the invariant catalogue docstring.

**Rule**: icon components (from §2.1's connected-component pass —
square-ish boxes: `0.6 ≤ w/h ≤ 1.67`, height ≥ 6 physical px so
antialiasing specks don't enter, and within the icon luma band —
non-text, so I1 excludes components already claimed by T1 lines) must
have bounding boxes of `ICON_SIZE_LOGICAL = 12.0` logical px within
`ICON_TOL_LOGICAL = 2.0` logical px, with the same physical floor as the
gap tolerance (`max(2.0 × scale, 2.0 px)` — at scale 0.5 one physical px
is 2 logical px of raster quantization, the GAP_TOL_FLOOR_PX lesson).
Evidence: 12 px is `Theme.icon-size` (`theme.slint:92`, the meta-constant
precedent); ±2 logical px absorbs glyph inset (an icon's ink box is
inset from its slot) measured at calibration.

**Typed finding record**: `invariant: "I1"`, region = the component box,
metric = `"icon bounding box 17 × 15 phys px → 17.0 logical px"`,
threshold = `"icon-size 12 logical px × base-scale within ±2 logical px; scope: icon-probe goldens only"`.

**Grading default**: `fix-later` (a drift from a token with no open
issue; the finding is the intake record — the D5 rubric's middle band).
The kinds wall deliberately shows the icon ladder at every scale, so
compliance there is exact; any finding is a real conformance gap.

### 2.4 Wiring

- `analyze_golden` appends `detect_q1(mask, lines, scale)` (consuming
  T1's lines) and, when the golden name matches the probe scope,
  `detect_i1(components, scale)`; the component pass runs once and feeds
  T1, I1.
- `emit_md`: `invariants` list gains `"Q1"`, `"I1"`; the grading-rules
  section gains the Q1 (fix-now floor) and I1 (fix-later) lines and
  rewrites the T1 line; the report title/date note the tier-2 run.
- The `main()` summary print gains Q1/I1 in its invariant loop.
- Docstring invariant catalogue gains Q1/I1 rows.
- The advisory-first posture is unchanged: no CI, no nextest, no hook.
- **Re-run over the 63-golden corpus** and commit refreshed
  `2026-11-25-layout-review-analyzer-findings.{toml,md}` (or a dated
  tier-2 twin — stage 2 decides; the twin preferred, the G3 report is
  the round-7 record) — the tripwire per the theme-constant header law.

## 3. Constructed cases (both ways, per invariant — the G3 precedent)

All drawn with rectangles on `_canvas` backgrounds, no fonts, no noise
coupling; each pins compliant → clean, violating → exactly the expected
typed finding(s), and a tolerance/scope pin.

**T1 (new estimator)**:

| pin | fixture | expectation |
|---|---|---|
| compliant | six 14-px-em rectangle lines (as today) | no T1 finding |
| off-ladder | six 24-px-em lines | exactly 6 T1 findings |
| mixed band (the estimator's reason to exist) | one 30-px-em line stacked directly against an 11-px-em line in one band | 1 finding only if off-ladder; both lines resolved separately (the row-run method reported one ~41-px union) |
| non-text classified out | a 60×60 px solid block beside a 12-px icon row | no T1 finding, **no reliance on the old MAX cap** (assert the constant is gone or unused) |
| suppression | two lines only | no findings + recorded skip note naming n=2 |
| scale pin | the compliant fixture judged at scale 2.0 | no finding |

**Q1**:

| pin | fixture | expectation |
|---|---|---|
| sub-floor body text | light-grey (150,150,150) bars on the (29,32,40) canvas, em 14 | exactly one Q1 finding per line, ratio < 4.5 recorded in the metric, grade fix-now |
| compliant body text | near-white (230,230,235) bars, em 14 (ratio > 4.5 — assert computed ratio, not just absence) | no Q1 finding |
| large-text floor pin | em-30 bars at a mid grey whose ratio ∈ [3.0, 4.5) | no Q1 finding (Large class — the floor is 3.0) |
| known-answer | `contrast_ratio` helper directly: black/white → 21.0; equal → 1.0 | the formula pins (mirrors token_tests) |

**I1**:

| pin | fixture (name-scoped) | expectation |
|---|---|---|
| compliant | 12×12 px square, icon-probe scope, scale 1.0 | no I1 finding |
| off-token | 17×17 px square, icon-probe scope, scale 1.0 | exactly one I1 finding |
| tolerance pin | 14×14 px square (within ±2) | no finding |
| aspect guard | a 12×40 px bar (not square-ish) in scope | no I1 finding |
| scope guard | the same 17×17 square on a non-probe golden name | no I1 finding |
| scale pin | 24×24 phys px at scale 2.0 (12 logical) | no finding |

The self-test `findings_of` helper gains the golden-name parameter (the
scope guard needs it); existing pins must all still pass — the T1
fixture pins are re-derived where the estimator's line count changes
them (the mixed-band case is new; the old six-line pins keep their
counts because rectangles merge into single-component lines).

## 4. Calibration & corpus re-run (stage-2 work list)

1. Implement `connected_components` + glyph classification (§2.1);
   rewire `detect_t1`; delete `MAX_LINE_EM_LOGICAL`; update self-test
   pins; `--self-test` green.
2. Implement `detect_q1` (§2.2) with the transcribed WCAG helpers +
   known-answer pins; self-test green.
3. Implement `detect_i1` (§2.3) with the scope guard; self-test green.
4. Calibrate once on the 63-golden corpus: run the analyzer, inspect the
   new Q1/I1/T1 populations, fix the evidence comments to the measured
   numbers — **before** any grading; never widen after.
5. Commit the refreshed findings artefacts (the tier-2 twin report +
   toml, dated 2026-11-25) and the re-run command in its header.
6. Comment on #1417/#1410/#1409 with the landed state (the standing
   authorization; closure is the dispatcher's at merge).

Stage-2 commits (each standalone, `<type>(<module>)` subjects):
- `feat(analyzer): connected-component text-line estimator (INV-T1)`
- `feat(analyzer): WCAG contrast floor invariant (INV-Q1)`
- `feat(analyzer): icon-size conformance invariant (INV-I1)`
- `test(analyzer): tier-2 corpus re-run findings`
(plus this plan's `docs(plan)` commit, already landed by stage 1.)

## 5. Stage-3 traps (review focus)

1. **The cap's ghost**: `MAX_LINE_EM_LOGICAL` must be *deleted*, not
   bypassed — if any code path still consults it, the estimator upgrade
   didn't happen.
2. **Formula drift**: the Python WCAG helpers must match
   `token_tests.rs` exactly (the 0.03928/12.92 linear arm, the 2.4
   gamma, the 0.05 lift, channel order R,G,B at indices 0,2,4 — the
   Rust code's `channel(2)` is the G channel because hex slices, a
   transcription trap). The known-answer pins (21.0, 1.0) are the tripwire.
3. **Q1's ink sampling must not regress on anti-aliased text**: if the
   ink modal picks blended edge pixels, real corpus ratios sag below
   floor and Q1 floods. Calibration's measured distributions are the
   evidence; a flood of Q1 findings on known-good corpus text is a
   sampler bug, not 63 goldens' worth of accessibility debt.
4. **I1 scope leakage**: the scope guard keys on the manifest name; a
   golden renamed by L3's corpus-truth chunk (#1392/#1382 renames) must
   not silently drop or widen the scope — the guard's threshold string
   travels with every finding.
5. **Grades are per-run data**: the G3 report's hand-applied grades do
   not carry into the tier-2 run; stage 3 checks the twin report's
   rubric section is self-consistent, not that it matches round 7's.
6. **Advisory posture intact**: no CI wiring, no nextest test, no
   pre-commit hook may reference the analyzer (D6 law) — a green CI
   dependency on `--self-test` would be a posture violation.

## 6. Out-of-scope & backlog

- Hue/tone-ladder conformance (the second half of #1410) — deferred;
  commented on #1410 with this plan's permalink, stays open.
- The #1418 classifier plan file did not exist at stage-1 time (chunk L1
  runs in parallel); its precedent was taken from the dispatch plan §2
  text. No issue needed — L1 owns it.
- No new `backlog:` issues were required by stage-1 diagnosis; the
  per-issue comments carry the round-8 context (numbers listed in the
  stage report).
