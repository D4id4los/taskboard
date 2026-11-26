# Layout Review Analyzer — Findings (round 8, chunk L2, tier 2)

- **Date**: 2026-11-25
- **Tier 2 additions** (analyzer-invariant-tier2 plan): Q1 WCAG contrast floor (#1410), I1 icon-size conformance (#1409), and the T1 connected-component glyph-line estimator upgrade (#1417 — the row-run band method and its MAX_LINE_EM_LOGICAL exclusion cap are replaced by component-level glyph classification).
- **Posture**: advisory-first (binding, plan D6). This report is data, not a gate — the analyzer reports; it never gates. No CI wiring, no nextest test, no pre-commit hook references this tool.
- **Run command**:

  ```sh
  scripts/layout_review.py crates/taskboard-ui-slint/tests/goldens \
      --out .artifacts/reports/2026-11-25-layout-review-analyzer-findings-tier2.toml
  ```

  `findings.toml` beside this file is the source of truth; this report is generated from it by `--emit-md`. Metric/region fields reproduce byte-identically on re-run (the corpus is byte-deterministic per G1); `grade` fields are applied deterministically per the recorded grading rules below (grade_for — the toml grades re-derive from the metric fields) and the accept-as-legacy grades name explained debt — the grade never silences the detector.

- **Calibration note** (D7): thresholds calibrated once on this corpus before grading — corner-sampled background flat at RGB ≈ (29,32,40) with ±2/255 anti-aliasing noise; content floor 12/255 sits above the noise and below the dimmest ink; edge floor 8 logical px sits above hairlines and below every real block boundary; text tolerance ±15% absorbs ≤1 px line-height slop at every corpus scale without overlapping ladder rungs (closest separation 12%, 12.5→14). Thresholds were not widened after grading.

## Headline counts — 2153 findings over the corpus

| invariant | accept-as-legacy | fix-now | fix-later | total |
|---|---|---|---|---|
| A1 | 581 | 0 | 61 | 642 |
| A2 | 178 | 0 | 187 | 365 |
| T1 | 185 | 0 | 71 | 256 |
| M1 | 7 | 0 | 3 | 10 |
| C1 | 29 | 27 | 0 | 56 |
| Q1 | 766 | 1 | 0 | 767 |
| I1 | 0 | 0 | 57 | 57 |
| **total** | **1746** | **28** | **379** | **2153** |

Grading rubric (D5): **fix-now** — a defect class the project already tracks as a bug (C1 clip signatures); **fix-later** — real drift from the invariant with no open issue (the finding is the intake record for a future backlog issue); **accept-as-legacy** — the deviation is load-bearing in the committed golden or the manifest's state note explains it.

Grading rules applied to this run (deterministic, recorded per D5 — the rubric absorbs legacy noise, never the detector):

- C1: extent ≥ 12 px → fix-now (flush panels, the open severing class #1365/#1378); extent < 12 px → accept-as-legacy (scrollbar thumbs at 8 px and window-chrome specks at 3 px — deliberate small flush elements).
- A1: delta to the nearest shared grid column ≤ 6 px → fix-later (near-miss drift); larger → accept-as-legacy (a distinct layout region, not a near-miss of the grid); no shared column at all (a singleton left edge) → accept-as-legacy (the grid judgment needs a neighbour to be a near-miss of — a lone edge is recorded, not graded fixable).
- A2: |gap − median| ≤ 8 px → fix-later (token-adjacent drift, the #1334 one-row-offset class); larger → accept-as-legacy (deliberate section breaks and spacers).
- T1: em ≥ 20 logical px → fix-later (genuine off-ladder text between the title and display rungs); em < 20 → accept-as-legacy (genuinely small off-ladder text — the tier-2 connected-component estimator classifies hairline rules and divider rows out at the component level, so the old hairline class no longer reaches the findings).
- Q1: a sub-floor contrast ratio is fix-now — the accessibility defect class (a floor is binary: 4.5 ≤ ratio is clean, below it is fix-now; there is no fix-later band inside a floor) — except the scrim-blend ink class → accept-as-legacy: overlay goldens render backdrop content under the modal scrim (measured ink modes sit on the 50% blend of the text tokens, max channel ≤ 140 vs ≥ 166 for unscrimmed text), which is deliberate de-emphasis, not a token defect.
- I1: fix-later (a drift from the Theme.icon-size token with no open issue; the finding is the intake record. Scope: icon-probe goldens only — the threshold string on every finding carries the guard).
- M1: icon-probe and board-drag-indicator margins → accept-as-legacy (the manifest's state note: deliberate kind-wall padding / mostly-empty drag canvas); the rest → fix-later.

## Findings by golden

### `action-menu-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=332 y=60 w=18 h=784 | x=332 y=60 w=18 h=784 | left edge at 332 px; nearest shared grid column 551 px (delta 219 px) |
| A1 | accept-as-legacy | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 551 px (delta 206 px) |
| A1 | accept-as-legacy | x=378 y=60 w=20 h=784 | x=378 y=60 w=20 h=784 | left edge at 378 px; nearest shared grid column 551 px (delta 173 px) |
| A1 | accept-as-legacy | x=386 y=60 w=12 h=784 | x=386 y=60 w=12 h=784 | left edge at 386 px; nearest shared grid column 551 px (delta 165 px) |
| A1 | accept-as-legacy | x=445 y=8 w=37 h=48 | x=445 y=8 w=37 h=48 | left edge at 445 px; nearest shared grid column 551 px (delta 106 px) |
| A1 | accept-as-legacy | x=470 y=8 w=12 h=48 | x=470 y=8 w=12 h=48 | left edge at 470 px; nearest shared grid column 551 px (delta 81 px) |
| A1 | accept-as-legacy | x=492 y=8 w=57 h=48 | x=492 y=8 w=57 h=48 | left edge at 492 px; nearest shared grid column 551 px (delta 59 px) |
| A1 | accept-as-legacy | x=536 y=60 w=12 h=784 | x=536 y=60 w=12 h=784 | left edge at 536 px; nearest shared grid column 551 px (delta 15 px) |
| A2 | fix-later | x=0 y=844 w=1000 h=9 | x=0 y=844 w=1000 h=9 | gap 9 px vs section median 4 px (delta 5 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `action-menu-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=233 y=60 w=74 h=784 | x=233 y=60 w=74 h=784 | left edge at 233 px; nearest shared grid column 57 px (delta 176 px) |
| A1 | accept-as-legacy | x=280 y=60 w=27 h=784 | x=280 y=60 w=27 h=784 | left edge at 280 px; nearest shared grid column 57 px (delta 223 px) |
| A1 | accept-as-legacy | x=383 y=60 w=56 h=784 | x=383 y=60 w=56 h=784 | left edge at 383 px; nearest shared grid column 57 px (delta 326 px) |
| A1 | accept-as-legacy | x=427 y=60 w=12 h=784 | x=427 y=60 w=12 h=784 | left edge at 427 px; nearest shared grid column 57 px (delta 370 px) |
| A1 | accept-as-legacy | x=499 y=60 w=260 h=784 | x=499 y=60 w=260 h=784 | left edge at 499 px; nearest shared grid column 57 px (delta 442 px) |
| A1 | accept-as-legacy | x=506 y=60 w=253 h=784 | x=506 y=60 w=253 h=784 | left edge at 506 px; nearest shared grid column 57 px (delta 449 px) |
| A1 | accept-as-legacy | x=512 y=864 w=71 h=32 | x=512 y=864 w=71 h=32 | left edge at 512 px; nearest shared grid column 57 px (delta 455 px) |
| A1 | accept-as-legacy | x=516 y=60 w=243 h=784 | x=516 y=60 w=243 h=784 | left edge at 516 px; nearest shared grid column 57 px (delta 459 px) |
| A1 | accept-as-legacy | x=695 y=8 w=37 h=48 | x=695 y=8 w=37 h=48 | left edge at 695 px; nearest shared grid column 1051 px (delta 356 px) |
| A1 | accept-as-legacy | x=720 y=8 w=12 h=48 | x=720 y=8 w=12 h=48 | left edge at 720 px; nearest shared grid column 1051 px (delta 331 px) |
| A1 | accept-as-legacy | x=742 y=8 w=57 h=48 | x=742 y=8 w=57 h=48 | left edge at 742 px; nearest shared grid column 1051 px (delta 309 px) |
| A1 | accept-as-legacy | x=878 y=60 w=20 h=784 | x=878 y=60 w=20 h=784 | left edge at 878 px; nearest shared grid column 1051 px (delta 173 px) |
| A1 | accept-as-legacy | x=886 y=60 w=12 h=784 | x=886 y=60 w=12 h=784 | left edge at 886 px; nearest shared grid column 1051 px (delta 165 px) |
| A1 | accept-as-legacy | x=1036 y=60 w=12 h=784 | x=1036 y=60 w=12 h=784 | left edge at 1036 px; nearest shared grid column 1051 px (delta 15 px) |
| A2 | fix-later | x=0 y=844 w=1500 h=11 | x=0 y=844 w=1500 h=11 | gap 11 px vs section median 4 px (delta 7 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `action-menu-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=155 y=60 w=36 h=800 | x=155 y=60 w=36 h=800 | left edge at 155 px; nearest shared grid column 57 px (delta 98 px) |
| A1 | accept-as-legacy | x=199 y=60 w=30 h=800 | x=199 y=60 w=30 h=800 | left edge at 199 px; nearest shared grid column 57 px (delta 142 px) |
| A1 | accept-as-legacy | x=202 y=60 w=27 h=800 | x=202 y=60 w=27 h=800 | left edge at 202 px; nearest shared grid column 57 px (delta 145 px) |
| A1 | accept-as-legacy | x=212 y=864 w=71 h=32 | x=212 y=864 w=71 h=32 | left edge at 212 px; nearest shared grid column 57 px (delta 155 px) |
| A1 | accept-as-legacy | x=302 y=60 w=20 h=800 | x=302 y=60 w=20 h=800 | left edge at 302 px; nearest shared grid column 460 px (delta 158 px) |
| A1 | accept-as-legacy | x=310 y=60 w=12 h=800 | x=310 y=60 w=12 h=800 | left edge at 310 px; nearest shared grid column 460 px (delta 150 px) |
| A1 | accept-as-legacy | x=436 y=8 w=38 h=48 | x=436 y=8 w=38 h=48 | left edge at 436 px; nearest shared grid column 460 px (delta 24 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `alarm-window-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| M1 | fix-later | x=1469 y=0 w=1 h=64 | x=1469 y=0 w=1 h=64 | right margin 30.0 logical px (30 px at scale 1.0); nearest rung 32 px |
| A1 | accept-as-legacy | x=25 y=32 w=26 h=16 | x=25 y=32 w=26 h=16 | left edge at 25 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=64 y=32 w=28 h=16 | x=64 y=32 w=28 h=16 | left edge at 64 px; no other left edge shares this column |

### `alarm-window-1.0-351`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| M1 | fix-later | x=320 y=0 w=1 h=64 | x=320 y=0 w=1 h=64 | right margin 30.0 logical px (30 px at scale 1.0); nearest rung 32 px |
| A1 | accept-as-legacy | x=25 y=24 w=26 h=32 | x=25 y=24 w=26 h=32 | left edge at 25 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=64 y=24 w=28 h=32 | x=64 y=24 w=28 h=32 | left edge at 64 px; no other left edge shares this column |

### `alarm-window-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| M1 | fix-later | x=569 y=0 w=1 h=64 | x=569 y=0 w=1 h=64 | right margin 30.0 logical px (30 px at scale 1.0); nearest rung 32 px |
| A1 | accept-as-legacy | x=25 y=24 w=26 h=32 | x=25 y=24 w=26 h=32 | left edge at 25 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=64 y=24 w=28 h=32 | x=64 y=24 w=28 h=32 | left edge at 64 px; no other left edge shares this column |

### `board-0.5-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=33 w=1 h=3 | x=0 y=66 w=2 h=6 | content run touches the left edge (extent 3 px along the edge) |
| A1 | accept-as-legacy | x=28 y=882 w=347 h=16 | x=56 y=1764 w=694 h=32 | left edge at 28 px; nearest shared grid column 12 px (delta 16 px) |
| A1 | accept-as-legacy | x=339 y=882 w=36 h=16 | x=678 y=1764 w=72 h=32 | left edge at 339 px; nearest shared grid column 12 px (delta 327 px) |
| A1 | accept-as-legacy | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 12 px (delta 450 px) |
| A1 | accept-as-legacy | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 12 px (delta 484 px) |
| A1 | accept-as-legacy | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 12 px (delta 491 px) |
| A1 | accept-as-legacy | x=534 y=882 w=6 h=16 | x=1068 y=1764 w=12 h=32 | left edge at 534 px; nearest shared grid column 12 px (delta 522 px) |
| A1 | accept-as-legacy | x=953 y=61 w=7 h=18 | x=1906 y=122 w=14 h=36 | left edge at 953 px; nearest shared grid column 12 px (delta 941 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 12 px (delta 968 px) |
| A2 | fix-later | x=0 y=28 w=1000 h=4 | x=0 y=56 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=45 w=1000 h=16 | x=0 y=90 w=2000 h=32 | gap 16 px vs section median 8 px (delta 8 px) |
| A2 | fix-later | x=0 y=79 w=1000 h=3 | x=0 y=158 w=2000 h=6 | gap 3 px vs section median 8 px (delta 5 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `board-0.5-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=33 w=1 h=3 | x=0 y=66 w=2 h=6 | content run touches the left edge (extent 3 px along the edge) |
| A1 | accept-as-legacy | x=28 y=882 w=514 h=16 | x=56 y=1764 w=1028 h=32 | left edge at 28 px; nearest shared grid column 12 px (delta 16 px) |
| A1 | accept-as-legacy | x=506 y=882 w=36 h=16 | x=1012 y=1764 w=72 h=32 | left edge at 506 px; nearest shared grid column 12 px (delta 494 px) |
| A1 | accept-as-legacy | x=712 y=4 w=28 h=24 | x=1424 y=8 w=56 h=48 | left edge at 712 px; nearest shared grid column 12 px (delta 700 px) |
| A1 | accept-as-legacy | x=746 y=4 w=6 h=24 | x=1492 y=8 w=12 h=48 | left edge at 746 px; nearest shared grid column 12 px (delta 734 px) |
| A1 | accept-as-legacy | x=753 y=4 w=16 h=24 | x=1506 y=8 w=32 h=48 | left edge at 753 px; nearest shared grid column 12 px (delta 741 px) |
| A1 | accept-as-legacy | x=784 y=882 w=6 h=16 | x=1568 y=1764 w=12 h=32 | left edge at 784 px; nearest shared grid column 12 px (delta 772 px) |
| A1 | accept-as-legacy | x=1453 y=62 w=7 h=16 | x=2906 y=124 w=14 h=32 | left edge at 1453 px; nearest shared grid column 12 px (delta 1441 px) |
| A1 | accept-as-legacy | x=1480 y=4 w=6 h=24 | x=2960 y=8 w=12 h=48 | left edge at 1480 px; nearest shared grid column 12 px (delta 1468 px) |
| A2 | fix-later | x=0 y=45 w=1500 h=17 | x=0 y=90 w=3000 h=34 | gap 17 px vs section median 13 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=78 w=1500 h=4 | x=0 y=156 w=3000 h=8 | gap 4 px vs section median 13 px (delta 9 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `board-0.5-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=599 y=76 w=1 h=8 | x=1198 y=152 w=2 h=16 | content run touches the right edge (extent 8 px along the edge) |
| C1 | fix-now | x=0 y=56 w=1 h=12 | x=0 y=112 w=2 h=24 | content run touches the left edge (extent 12 px along the edge) |
| C1 | accept-as-legacy | x=0 y=76 w=1 h=8 | x=0 y=152 w=2 h=16 | content run touches the left edge (extent 8 px along the edge) |
| A1 | accept-as-legacy | x=28 y=882 w=214 h=16 | x=56 y=1764 w=428 h=32 | left edge at 28 px; nearest shared grid column 9 px (delta 19 px) |
| A1 | accept-as-legacy | x=206 y=882 w=36 h=16 | x=412 y=1764 w=72 h=32 | left edge at 206 px; nearest shared grid column 9 px (delta 197 px) |
| A1 | accept-as-legacy | x=334 y=882 w=6 h=16 | x=668 y=1764 w=12 h=32 | left edge at 334 px; nearest shared grid column 9 px (delta 325 px) |
| A1 | accept-as-legacy | x=513 y=4 w=28 h=24 | x=1026 y=8 w=56 h=48 | left edge at 513 px; nearest shared grid column 9 px (delta 504 px) |
| A1 | accept-as-legacy | x=547 y=4 w=6 h=24 | x=1094 y=8 w=12 h=48 | left edge at 547 px; nearest shared grid column 9 px (delta 538 px) |
| A1 | accept-as-legacy | x=554 y=4 w=16 h=24 | x=1108 y=8 w=32 h=48 | left edge at 554 px; nearest shared grid column 9 px (delta 545 px) |
| A1 | accept-as-legacy | x=580 y=4 w=6 h=24 | x=1160 y=8 w=12 h=48 | left edge at 580 px; nearest shared grid column 9 px (delta 571 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `board-3.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=999 y=309 w=1 h=3 | x=333 y=103 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) |
| C1 | accept-as-legacy | x=0 y=209 w=1 h=7 | x=0 y=70 w=0 h=2 | content run touches the left edge (extent 7 px along the edge) |
| A1 | accept-as-legacy | x=60 y=492 w=724 h=96 | x=20 y=164 w=241 h=32 | left edge at 60 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=274 y=24 w=166 h=144 | x=91 y=8 w=55 h=48 | left edge at 274 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=368 y=792 w=211 h=96 | x=123 y=264 w=70 h=32 | left edge at 368 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=555 y=192 w=110 h=120 | x=185 y=64 w=37 h=40 | left edge at 555 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=722 y=360 w=38 h=120 | x=241 y=120 w=13 h=40 | left edge at 722 px; no other left edge shares this column |
| A2 | accept-as-legacy | x=0 y=168 w=1000 h=24 | x=0 y=56 w=333 h=8 | gap 24 px vs section median 48 px (delta 24 px) |
| A2 | accept-as-legacy | x=0 y=480 w=1000 h=12 | x=0 y=160 w=333 h=4 | gap 12 px vs section median 48 px (delta 36 px) |
| A2 | accept-as-legacy | x=0 y=684 w=1000 h=108 | x=0 y=228 w=333 h=36 | gap 108 px vs section median 48 px (delta 60 px) |
| T1 | accept-as-legacy | x=884 y=54 w=28 h=3 | x=295 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=64 w=28 h=4 | x=295 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=75 w=28 h=3 | x=295 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=0 y=190 w=26 h=28 | x=0 y=63 w=9 h=9 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=454 y=190 w=28 h=28 | x=151 y=63 w=9 h=9 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `board-3.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=1499 y=309 w=1 h=3 | x=500 y=103 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) |
| C1 | accept-as-legacy | x=0 y=209 w=1 h=7 | x=0 y=70 w=0 h=2 | content run touches the left edge (extent 7 px along the edge) |
| A1 | accept-as-legacy | x=60 y=492 w=1224 h=96 | x=20 y=164 w=408 h=32 | left edge at 60 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=524 y=24 w=166 h=144 | x=175 y=8 w=55 h=48 | left edge at 524 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=535 y=792 w=211 h=96 | x=178 y=264 w=70 h=32 | left edge at 535 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=1222 y=360 w=38 h=120 | x=407 y=120 w=13 h=40 | left edge at 1222 px; no other left edge shares this column |
| A2 | accept-as-legacy | x=0 y=168 w=1500 h=24 | x=0 y=56 w=500 h=8 | gap 24 px vs section median 40 px (delta 16 px) |
| A2 | accept-as-legacy | x=0 y=216 w=1500 h=20 | x=0 y=72 w=500 h=7 | gap 20 px vs section median 40 px (delta 20 px) |
| A2 | fix-later | x=0 y=312 w=1500 h=48 | x=0 y=104 w=500 h=16 | gap 48 px vs section median 40 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=480 w=1500 h=12 | x=0 y=160 w=500 h=4 | gap 12 px vs section median 40 px (delta 28 px) |
| A2 | accept-as-legacy | x=0 y=588 w=1500 h=69 | x=0 y=196 w=500 h=23 | gap 69 px vs section median 40 px (delta 29 px) |
| A2 | accept-as-legacy | x=0 y=663 w=1500 h=129 | x=0 y=221 w=500 h=43 | gap 129 px vs section median 40 px (delta 89 px) |
| T1 | accept-as-legacy | x=1384 y=54 w=28 h=3 | x=461 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1384 y=64 w=28 h=4 | x=461 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1384 y=75 w=28 h=3 | x=461 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=0 y=190 w=26 h=28 | x=0 y=63 w=9 h=9 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=454 y=190 w=28 h=28 | x=151 y=63 w=9 h=9 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `board-3.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=599 y=309 w=1 h=3 | x=200 y=103 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) |
| C1 | fix-now | x=599 y=456 w=1 h=48 | x=200 y=152 w=0 h=16 | content run touches the right edge (extent 48 px along the edge) |
| C1 | accept-as-legacy | x=0 y=309 w=1 h=3 | x=0 y=103 w=0 h=1 | content run touches the left edge (extent 3 px along the edge) |
| C1 | fix-now | x=0 y=336 w=1 h=72 | x=0 y=112 w=0 h=24 | content run touches the left edge (extent 72 px along the edge) |
| C1 | fix-now | x=0 y=456 w=1 h=48 | x=0 y=152 w=0 h=16 | content run touches the left edge (extent 48 px along the edge) |
| A1 | accept-as-legacy | x=145 y=24 w=166 h=144 | x=48 y=8 w=55 h=48 | left edge at 145 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=266 y=792 w=211 h=96 | x=89 y=264 w=70 h=32 | left edge at 266 px; no other left edge shares this column |
| A2 | accept-as-legacy | x=0 y=300 w=600 h=9 | x=0 y=100 w=200 h=3 | gap 9 px vs section median 36 px (delta 27 px) |
| A2 | accept-as-legacy | x=0 y=312 w=600 h=12 | x=0 y=104 w=200 h=4 | gap 12 px vs section median 36 px (delta 24 px) |
| A2 | accept-as-legacy | x=0 y=420 w=600 h=12 | x=0 y=140 w=200 h=4 | gap 12 px vs section median 36 px (delta 24 px) |
| A2 | accept-as-legacy | x=0 y=528 w=600 h=168 | x=0 y=176 w=200 h=56 | gap 168 px vs section median 36 px (delta 132 px) |
| A2 | accept-as-legacy | x=0 y=744 w=600 h=48 | x=0 y=248 w=200 h=16 | gap 48 px vs section median 36 px (delta 12 px) |
| T1 | accept-as-legacy | x=545 y=54 w=28 h=3 | x=182 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=64 w=28 h=4 | x=182 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=75 w=28 h=3 | x=182 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=53 y=108 w=51 h=11 | x=18 y=36 w=17 h=4 | text line height 11 px (3 components) → em ≈ 5.2 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `board-drag-indicator-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=68 w=1 h=4 | x=0 y=68 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) |
| M1 | accept-as-legacy | x=0 y=508 w=1000 h=1 | x=0 y=508 w=1000 h=1 | bottom margin 391.0 logical px (391 px at scale 1.0); nearest rung 96 px |
| A1 | fix-later | x=16 y=459 w=32 h=32 | x=16 y=459 w=32 h=32 | left edge at 16 px; nearest shared grid column 20 px (delta 4 px) |
| A1 | accept-as-legacy | x=136 y=497 w=46 h=12 | x=136 y=497 w=46 h=12 | left edge at 136 px; nearest shared grid column 21 px (delta 115 px) |
| A1 | accept-as-legacy | x=320 y=459 w=71 h=32 | x=320 y=459 w=71 h=32 | left edge at 320 px; nearest shared grid column 528 px (delta 208 px) |
| A1 | accept-as-legacy | x=339 y=497 w=69 h=12 | x=339 y=497 w=69 h=12 | left edge at 339 px; nearest shared grid column 528 px (delta 189 px) |
| A1 | accept-as-legacy | x=391 y=497 w=17 h=12 | x=391 y=497 w=17 h=12 | left edge at 391 px; nearest shared grid column 528 px (delta 137 px) |
| A1 | accept-as-legacy | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 528 px (delta 104 px) |
| A1 | accept-as-legacy | x=436 y=497 w=68 h=12 | x=436 y=497 w=68 h=12 | left edge at 436 px; nearest shared grid column 528 px (delta 92 px) |
| A1 | accept-as-legacy | x=477 y=497 w=27 h=12 | x=477 y=497 w=27 h=12 | left edge at 477 px; nearest shared grid column 528 px (delta 51 px) |
| A1 | accept-as-legacy | x=545 y=497 w=35 h=12 | x=545 y=497 w=35 h=12 | left edge at 545 px; nearest shared grid column 529 px (delta 16 px) |
| A1 | accept-as-legacy | x=660 y=497 w=20 h=12 | x=660 y=497 w=20 h=12 | left edge at 660 px; nearest shared grid column 529 px (delta 131 px) |
| A2 | fix-later | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 14 px (delta 6 px) |
| A2 | fix-later | x=0 y=72 w=1000 h=6 | x=0 y=72 w=1000 h=6 | gap 6 px vs section median 14 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=160 w=1000 h=4 | x=0 y=160 w=1000 h=4 | gap 4 px vs section median 14 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=196 w=1000 h=80 | x=0 y=196 w=1000 h=80 | gap 80 px vs section median 14 px (delta 66 px) |
| A2 | accept-as-legacy | x=0 y=416 w=1000 h=43 | x=0 y=416 w=1000 h=43 | gap 43 px vs section median 23 px (delta 20 px) |
| A2 | accept-as-legacy | x=0 y=491 w=1000 h=6 | x=0 y=491 w=1000 h=6 | gap 6 px vs section median 23 px (delta 17 px) |
| T1 | fix-later | x=16 y=459 w=32 h=32 | x=16 y=459 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| T1 | fix-later | x=920 y=459 w=16 h=32 | x=920 y=459 w=16 h=32 | text line height 32 px (2 components) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| T1 | accept-as-legacy | x=517 y=504 w=2 h=4 | x=517 y=504 w=2 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| Q1 | accept-as-legacy | x=0 y=62 w=10 h=12 | x=0 y=62 w=10 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=150 y=62 w=12 h=12 | x=150 y=62 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `board-empty-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | fix-now | x=999 y=480 w=1 h=28 | x=999 y=480 w=1 h=28 | content run touches the right edge (extent 28 px along the edge) |
| C1 | fix-now | x=999 y=524 w=1 h=28 | x=999 y=524 w=1 h=28 | content run touches the right edge (extent 28 px along the edge) |
| C1 | accept-as-legacy | x=0 y=375 w=1 h=4 | x=0 y=375 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) |
| C1 | fix-now | x=0 y=480 w=1 h=28 | x=0 y=480 w=1 h=28 | content run touches the left edge (extent 28 px along the edge) |
| C1 | fix-now | x=0 y=524 w=1 h=28 | x=0 y=524 w=1 h=28 | content run touches the left edge (extent 28 px along the edge) |
| A1 | accept-as-legacy | x=54 y=8 w=18 h=48 | x=54 y=8 w=18 h=48 | left edge at 54 px; nearest shared grid column 67 px (delta 13 px) |
| A1 | accept-as-legacy | x=347 y=864 w=66 h=32 | x=347 y=864 w=66 h=32 | left edge at 347 px; nearest shared grid column 548 px (delta 201 px) |
| A1 | accept-as-legacy | x=434 y=8 w=56 h=48 | x=434 y=8 w=56 h=48 | left edge at 434 px; nearest shared grid column 548 px (delta 114 px) |
| A1 | accept-as-legacy | x=483 y=426 w=13 h=13 | x=483 y=426 w=13 h=13 | left edge at 483 px; nearest shared grid column 548 px (delta 65 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=0 y=369 w=10 h=12 | x=0 y=369 w=10 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=150 y=369 w=12 h=12 | x=150 y=369 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `capture-overlay-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=220 y=384 w=560 h=92 | x=220 y=384 w=560 h=92 | left edge at 220 px; nearest shared grid column 57 px (delta 163 px) |
| A1 | accept-as-legacy | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 430 px (delta 86 px) |
| A1 | accept-as-legacy | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 430 px (delta 64 px) |
| A1 | accept-as-legacy | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 430 px (delta 59 px) |
| A1 | fix-later | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | x=445 y=24 w=37 h=19 | x=445 y=24 w=37 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | x=529 y=205 w=23 h=16 | x=529 y=205 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 505 px (delta 229 px) |
| A1 | accept-as-legacy | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 505 px (delta 235 px) |
| A1 | accept-as-legacy | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 505 px (delta 404 px) |
| A1 | accept-as-legacy | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 505 px (delta 409 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=43 w=1000 h=10 | x=0 y=43 w=1000 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 16 px (delta 14 px) |
| A2 | fix-later | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=177 w=1000 h=28 | x=0 y=177 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=221 w=1000 h=11 | x=0 y=221 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=241 w=1000 h=1 | x=0 y=241 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=269 w=1000 h=19 | x=0 y=269 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | x=0 y=298 w=1000 h=86 | x=0 y=298 w=1000 h=86 | gap 86 px vs section median 16 px (delta 70 px) |
| T1 | accept-as-legacy | x=545 y=8 w=4 h=3 | x=545 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=53 w=4 h=3 | x=545 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=579 y=9 w=11 h=12 | x=579 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=443 y=24 w=28 h=15 | x=443 y=24 w=28 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=468 y=24 w=16 h=15 | x=468 y=24 w=16 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=923 y=82 w=14 h=16 | x=923 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=84 w=12 h=12 | x=964 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=342 y=121 w=42 h=16 | x=342 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=393 y=121 w=59 h=16 | x=393 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=714 y=121 w=47 h=16 | x=714 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=758 y=121 w=6 h=15 | x=758 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=885 y=121 w=42 h=15 | x=885 y=121 w=42 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=339 y=124 w=6 h=12 | x=339 y=124 w=6 h=12 | ink RGB (66, 69, 80) on background RGB (23, 26, 35) → contrast 1.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=165 w=12 h=12 | x=964 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=203 w=24 h=16 | x=428 y=203 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=449 y=203 w=42 h=20 | x=449 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=493 y=203 w=32 h=16 | x=493 y=203 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=203 w=42 h=20 | x=531 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=525 y=204 w=10 h=15 | x=525 y=204 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=337 y=230 w=58 h=16 | x=337 y=230 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=395 y=230 w=36 h=16 | x=395 y=230 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=230 w=15 h=13 | x=428 y=230 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=442 y=230 w=64 h=16 | x=442 y=230 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=230 w=28 h=13 | x=503 y=230 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=547 y=230 w=30 h=13 | x=547 y=230 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=574 y=230 w=30 h=13 | x=574 y=230 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=604 y=230 w=21 h=13 | x=604 y=230 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=626 y=230 w=13 h=13 | x=626 y=230 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=636 y=230 w=24 h=13 | x=636 y=230 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=359 y=231 w=8 h=12 | x=359 y=231 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=258 w=29 h=13 | x=10 y=258 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=258 w=23 h=13 | x=36 y=258 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=724 y=258 w=26 h=13 | x=724 y=258 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=754 y=258 w=18 h=13 | x=754 y=258 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=285 w=18 h=15 | x=31 y=285 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=285 w=8 h=15 | x=52 y=285 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=286 w=16 h=14 | x=18 y=286 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=934 y=287 w=25 h=13 | x=934 y=287 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=287 w=17 h=13 | x=964 y=287 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `capture-overlay-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 20 px (delta 4 px) |
| A1 | accept-as-legacy | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 23 px (delta 10 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=407 y=226 w=23 h=16 | x=407 y=226 w=23 h=16 | left edge at 407 px; nearest shared grid column 383 px (delta 24 px) |
| A1 | fix-later | x=511 y=123 w=16 h=12 | x=511 y=123 w=16 h=12 | left edge at 511 px; nearest shared grid column 508 px (delta 3 px) |
| A1 | accept-as-legacy | x=529 y=123 w=570 h=12 | x=529 y=123 w=570 h=12 | left edge at 529 px; nearest shared grid column 508 px (delta 21 px) |
| A1 | accept-as-legacy | x=538 y=123 w=561 h=12 | x=538 y=123 w=561 h=12 | left edge at 538 px; nearest shared grid column 508 px (delta 30 px) |
| A1 | accept-as-legacy | x=695 y=24 w=37 h=19 | x=695 y=24 w=37 h=19 | left edge at 695 px; nearest shared grid column 508 px (delta 187 px) |
| A1 | accept-as-legacy | x=720 y=24 w=12 h=19 | x=720 y=24 w=12 h=19 | left edge at 720 px; nearest shared grid column 508 px (delta 212 px) |
| A1 | accept-as-legacy | x=1109 y=123 w=28 h=12 | x=1109 y=123 w=28 h=12 | left edge at 1109 px; nearest shared grid column 508 px (delta 601 px) |
| A1 | accept-as-legacy | x=1115 y=123 w=22 h=12 | x=1115 y=123 w=22 h=12 | left edge at 1115 px; nearest shared grid column 508 px (delta 607 px) |
| A1 | accept-as-legacy | x=1367 y=123 w=121 h=12 | x=1367 y=123 w=121 h=12 | left edge at 1367 px; nearest shared grid column 508 px (delta 859 px) |
| A2 | fix-later | x=0 y=10 w=1500 h=2 | x=0 y=10 w=1500 h=2 | gap 2 px vs section median 9 px (delta 7 px) |
| A2 | fix-later | x=0 y=13 w=1500 h=1 | x=0 y=13 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| A2 | fix-later | x=0 y=16 w=1500 h=1 | x=0 y=16 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| A2 | fix-later | x=0 y=19 w=1500 h=2 | x=0 y=19 w=1500 h=2 | gap 2 px vs section median 9 px (delta 7 px) |
| A2 | fix-later | x=0 y=23 w=1500 h=1 | x=0 y=23 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1500 h=31 | x=0 y=56 w=1500 h=31 | gap 31 px vs section median 9 px (delta 22 px) |
| A2 | fix-later | x=0 y=96 w=1500 h=16 | x=0 y=96 w=1500 h=16 | gap 16 px vs section median 9 px (delta 7 px) |
| A2 | fix-later | x=0 y=145 w=1500 h=16 | x=0 y=145 w=1500 h=16 | gap 16 px vs section median 9 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=170 w=1500 h=19 | x=0 y=170 w=1500 h=19 | gap 19 px vs section median 9 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=199 w=1500 h=27 | x=0 y=199 w=1500 h=27 | gap 27 px vs section median 9 px (delta 18 px) |
| A2 | fix-later | x=0 y=262 w=1500 h=1 | x=0 y=262 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| T1 | accept-as-legacy | x=795 y=8 w=4 h=3 | x=795 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=795 y=53 w=4 h=3 | x=795 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=829 y=9 w=11 h=12 | x=829 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=693 y=24 w=28 h=15 | x=693 y=24 w=28 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=718 y=24 w=16 h=15 | x=718 y=24 w=16 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1423 y=82 w=14 h=16 | x=1423 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=84 w=12 h=12 | x=1464 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=509 y=121 w=42 h=16 | x=509 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=559 y=121 w=59 h=16 | x=559 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1089 y=121 w=47 h=16 | x=1089 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1133 y=121 w=6 h=15 | x=1133 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1344 y=121 w=41 h=15 | x=1344 y=121 w=41 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=506 y=124 w=6 h=12 | x=506 y=124 w=6 h=12 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=159 w=29 h=13 | x=10 y=159 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=159 w=23 h=13 | x=36 y=159 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1099 y=159 w=26 h=13 | x=1099 y=159 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1129 y=159 w=18 h=13 | x=1129 y=159 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=186 w=18 h=15 | x=31 y=186 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=186 w=8 h=15 | x=52 y=186 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=187 w=16 h=14 | x=18 y=187 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1434 y=188 w=25 h=13 | x=1434 y=188 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=188 w=17 h=13 | x=1464 y=188 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=306 y=224 w=24 h=16 | x=306 y=224 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=327 y=224 w=42 h=20 | x=327 y=224 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=371 y=224 w=32 h=16 | x=371 y=224 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=409 y=224 w=42 h=20 | x=409 y=224 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=403 y=225 w=10 h=15 | x=403 y=225 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=230 w=12 h=12 | x=1464 y=230 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=755 y=231 w=18 h=13 | x=755 y=231 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=770 y=231 w=24 h=13 | x=770 y=231 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=799 y=231 w=11 h=13 | x=799 y=231 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=215 y=251 w=58 h=16 | x=215 y=251 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=273 y=251 w=36 h=16 | x=273 y=251 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=306 y=251 w=15 h=13 | x=306 y=251 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=320 y=251 w=64 h=16 | x=320 y=251 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=381 y=251 w=28 h=13 | x=381 y=251 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=425 y=251 w=30 h=13 | x=425 y=251 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=452 y=251 w=30 h=13 | x=452 y=251 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=482 y=251 w=21 h=13 | x=482 y=251 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=504 y=251 w=13 h=13 | x=504 y=251 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=514 y=251 w=24 h=13 | x=514 y=251 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=237 y=252 w=8 h=12 | x=237 y=252 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=555 y=873 w=25 h=13 | x=555 y=873 w=25 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=815 y=873 w=14 h=14 | x=815 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=514 y=874 w=35 h=12 | x=514 y=874 w=35 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `capture-overlay-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=155 y=232 w=48 h=12 | x=155 y=232 w=48 h=12 | left edge at 155 px; nearest shared grid column 229 px (delta 74 px) |
| A1 | accept-as-legacy | x=211 y=123 w=16 h=12 | x=211 y=123 w=16 h=12 | left edge at 211 px; nearest shared grid column 229 px (delta 18 px) |
| A1 | accept-as-legacy | x=329 y=205 w=23 h=16 | x=329 y=205 w=23 h=16 | left edge at 329 px; nearest shared grid column 305 px (delta 24 px) |
| A1 | accept-as-legacy | x=349 y=232 w=12 h=12 | x=349 y=232 w=12 h=12 | left edge at 349 px; nearest shared grid column 305 px (delta 44 px) |
| A1 | accept-as-legacy | x=542 y=123 w=46 h=12 | x=542 y=123 w=46 h=12 | left edge at 542 px; nearest shared grid column 462 px (delta 80 px) |
| A2 | accept-as-legacy | x=0 y=11 w=600 h=7 | x=0 y=11 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=19 w=600 h=2 | x=0 y=19 w=600 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=23 w=600 h=1 | x=0 y=23 w=600 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=43 w=600 h=10 | x=0 y=43 w=600 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=56 w=600 h=28 | x=0 y=56 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=115 w=600 h=8 | x=0 y=115 w=600 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=135 w=600 h=7 | x=0 y=135 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=145 w=600 h=23 | x=0 y=145 w=600 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=177 w=600 h=28 | x=0 y=177 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=221 w=600 h=11 | x=0 y=221 w=600 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | fix-later | x=0 y=269 w=600 h=19 | x=0 y=269 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | x=0 y=298 w=600 h=86 | x=0 y=298 w=600 h=86 | gap 86 px vs section median 16 px (delta 70 px) |
| T1 | accept-as-legacy | x=536 y=8 w=4 h=3 | x=536 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=536 y=53 w=4 h=3 | x=536 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=434 y=24 w=34 h=15 | x=434 y=24 w=34 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=489 y=24 w=20 h=16 | x=489 y=24 w=20 h=16 | ink RGB (117, 119, 125) on background RGB (23, 26, 35) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=464 y=25 w=12 h=14 | x=464 y=25 w=12 h=14 | ink RGB (87, 90, 102) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=523 y=82 w=14 h=16 | x=523 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=84 w=12 h=12 | x=564 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=209 y=121 w=42 h=16 | x=209 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=259 y=121 w=59 h=16 | x=259 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=414 y=121 w=47 h=16 | x=414 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=458 y=121 w=6 h=15 | x=458 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=519 y=121 w=41 h=15 | x=519 y=121 w=41 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=206 y=124 w=6 h=12 | x=206 y=124 w=6 h=12 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=165 w=12 h=12 | x=564 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=228 y=203 w=24 h=16 | x=228 y=203 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=249 y=203 w=42 h=20 | x=249 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=293 y=203 w=32 h=16 | x=293 y=203 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=331 y=203 w=42 h=20 | x=331 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=325 y=204 w=10 h=15 | x=325 y=204 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=137 y=230 w=58 h=16 | x=137 y=230 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=195 y=230 w=36 h=16 | x=195 y=230 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=228 y=230 w=15 h=13 | x=228 y=230 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=242 y=230 w=64 h=16 | x=242 y=230 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=303 y=230 w=28 h=13 | x=303 y=230 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=230 w=30 h=13 | x=347 y=230 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=374 y=230 w=30 h=13 | x=374 y=230 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=404 y=230 w=21 h=13 | x=404 y=230 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=426 y=230 w=13 h=13 | x=426 y=230 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=436 y=230 w=24 h=13 | x=436 y=230 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=159 y=231 w=8 h=12 | x=159 y=231 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=258 w=29 h=13 | x=10 y=258 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=258 w=23 h=13 | x=36 y=258 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=424 y=258 w=26 h=13 | x=424 y=258 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=454 y=258 w=18 h=13 | x=454 y=258 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=285 w=18 h=15 | x=31 y=285 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=285 w=8 h=15 | x=52 y=285 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=286 w=16 h=14 | x=18 y=286 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=534 y=287 w=25 h=13 | x=534 y=287 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=287 w=17 h=13 | x=564 y=287 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=255 y=873 w=25 h=13 | x=255 y=873 w=25 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=365 y=873 w=14 h=14 | x=365 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=214 y=874 w=35 h=12 | x=214 y=874 w=35 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `confirm-strip-danger-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=78 w=1 h=3 | x=0 y=78 w=1 h=3 | content run touches the left edge (extent 3 px along the edge) |
| C1 | accept-as-legacy | x=0 y=108 w=1 h=4 | x=0 y=108 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) |
| A1 | accept-as-legacy | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 322 px (delta 306 px) |
| A1 | accept-as-legacy | x=907 y=160 w=13 h=40 | x=907 y=160 w=13 h=40 | left edge at 907 px; nearest shared grid column 567 px (delta 340 px) |
| A2 | fix-later | x=0 y=56 w=1000 h=19 | x=0 y=56 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | x=0 y=112 w=1000 h=6 | x=0 y=112 w=1000 h=6 | gap 6 px vs section median 16 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=200 w=1000 h=4 | x=0 y=200 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |
| T1 | accept-as-legacy | x=235 y=82 w=2 h=4 | x=235 y=82 w=2 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=280 y=82 w=2 h=4 | x=280 y=82 w=2 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=549 y=82 w=2 h=4 | x=549 y=82 w=2 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | fix-now | x=320 y=73 w=6 h=13 | x=320 y=73 w=6 h=13 | ink RGB (179, 93, 90) on background RGB (20, 22, 28) → contrast 4.0:1; role class Body |
| Q1 | accept-as-legacy | x=0 y=102 w=10 h=12 | x=0 y=102 w=10 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=150 y=102 w=12 h=12 | x=150 y=102 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `confirm-strip-success-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=108 w=1 h=4 | x=0 y=108 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) |
| A1 | accept-as-legacy | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 534 px (delta 518 px) |
| A1 | accept-as-legacy | x=207 y=74 w=71 h=13 | x=207 y=74 w=71 h=13 | left edge at 207 px; nearest shared grid column 534 px (delta 327 px) |
| A1 | accept-as-legacy | x=254 y=74 w=24 h=13 | x=254 y=74 w=24 h=13 | left edge at 254 px; nearest shared grid column 534 px (delta 280 px) |
| A1 | accept-as-legacy | x=292 y=74 w=16 h=13 | x=292 y=74 w=16 h=13 | left edge at 292 px; nearest shared grid column 534 px (delta 242 px) |
| A1 | accept-as-legacy | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 534 px (delta 189 px) |
| A1 | accept-as-legacy | x=391 y=74 w=54 h=13 | x=391 y=74 w=54 h=13 | left edge at 391 px; nearest shared grid column 534 px (delta 143 px) |
| A1 | accept-as-legacy | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 534 px (delta 110 px) |
| A1 | accept-as-legacy | x=514 y=74 w=54 h=13 | x=514 y=74 w=54 h=13 | left edge at 514 px; nearest shared grid column 534 px (delta 20 px) |
| A1 | accept-as-legacy | x=549 y=74 w=19 h=13 | x=549 y=74 w=19 h=13 | left edge at 549 px; nearest shared grid column 535 px (delta 14 px) |
| A1 | accept-as-legacy | x=690 y=74 w=38 h=13 | x=690 y=74 w=38 h=13 | left edge at 690 px; nearest shared grid column 567 px (delta 123 px) |
| A1 | accept-as-legacy | x=907 y=160 w=13 h=40 | x=907 y=160 w=13 h=40 | left edge at 907 px; nearest shared grid column 567 px (delta 340 px) |
| A2 | accept-as-legacy | x=0 y=112 w=1000 h=6 | x=0 y=112 w=1000 h=6 | gap 6 px vs section median 16 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=200 w=1000 h=4 | x=0 y=200 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |
| T1 | accept-as-legacy | x=220 y=82 w=2 h=4 | x=220 y=82 w=2 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=267 y=82 w=2 h=4 | x=267 y=82 w=2 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=0 y=102 w=10 h=12 | x=0 y=102 w=10 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=150 y=102 w=12 h=12 | x=150 y=102 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `due-editor-datepicker-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | fix-now | x=999 y=171 w=1 h=17 | x=999 y=171 w=1 h=17 | content run touches the right edge (extent 17 px along the edge) |
| C1 | fix-now | x=999 y=208 w=1 h=25 | x=999 y=208 w=1 h=25 | content run touches the right edge (extent 25 px along the edge) |
| C1 | accept-as-legacy | x=999 y=399 w=1 h=6 | x=999 y=399 w=1 h=6 | content run touches the right edge (extent 6 px along the edge) |
| C1 | fix-now | x=0 y=171 w=1 h=17 | x=0 y=171 w=1 h=17 | content run touches the left edge (extent 17 px along the edge) |
| C1 | fix-now | x=0 y=397 w=1 h=31 | x=0 y=397 w=1 h=31 | content run touches the left edge (extent 31 px along the edge) |
| A1 | fix-later | x=4 y=144 w=498 h=48 | x=4 y=144 w=498 h=48 | left edge at 4 px; nearest shared grid column 1 px (delta 3 px) |
| A1 | accept-as-legacy | x=127 y=196 w=20 h=272 | x=127 y=196 w=20 h=272 | left edge at 127 px; nearest shared grid column 92 px (delta 35 px) |
| A1 | accept-as-legacy | x=213 y=196 w=15 h=272 | x=213 y=196 w=15 h=272 | left edge at 213 px; nearest shared grid column 226 px (delta 13 px) |
| A1 | accept-as-legacy | x=364 y=71 w=60 h=20 | x=364 y=71 w=60 h=20 | left edge at 364 px; nearest shared grid column 347 px (delta 17 px) |
| A1 | accept-as-legacy | x=411 y=71 w=13 h=20 | x=411 y=71 w=13 h=20 | left edge at 411 px; nearest shared grid column 347 px (delta 64 px) |
| A1 | accept-as-legacy | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 347 px (delta 77 px) |
| A1 | accept-as-legacy | x=514 y=144 w=87 h=48 | x=514 y=144 w=87 h=48 | left edge at 514 px; nearest shared grid column 347 px (delta 167 px) |
| A1 | accept-as-legacy | x=538 y=71 w=19 h=20 | x=538 y=71 w=19 h=20 | left edge at 538 px; nearest shared grid column 347 px (delta 191 px) |
| A2 | fix-later | x=0 y=56 w=1000 h=15 | x=0 y=56 w=1000 h=15 | gap 15 px vs section median 7 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 7 px (delta 13 px) |
| A2 | fix-later | x=0 y=192 w=1000 h=4 | x=0 y=192 w=1000 h=4 | gap 4 px vs section median 7 px (delta 3 px) |
| A2 | fix-later | x=0 y=485 w=1000 h=3 | x=0 y=485 w=1000 h=3 | gap 3 px vs section median 7 px (delta 4 px) |
| A2 | fix-later | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 7 px (delta 3 px) |
| T1 | accept-as-legacy | x=228 y=71 w=10 h=4 | x=228 y=71 w=10 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=4 h=4 | x=298 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=4 h=4 | x=658 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=164 y=75 w=23 h=17 | x=164 y=75 w=23 h=17 | text line height 17 px (2 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=32 y=474 w=6 h=13 | x=32 y=474 w=6 h=13 | ink RGB (107, 111, 125) on background RGB (20, 22, 28) → contrast 3.6:1; role class Body; scrim-blend ink |

### `focus-overlay-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=220 y=8 w=14 h=892 | x=220 y=8 w=14 h=892 | left edge at 220 px; nearest shared grid column 38 px (delta 182 px) |
| A1 | accept-as-legacy | x=268 y=8 w=19 h=892 | x=268 y=8 w=19 h=892 | left edge at 268 px; nearest shared grid column 38 px (delta 230 px) |
| A1 | accept-as-legacy | x=433 y=8 w=44 h=892 | x=433 y=8 w=44 h=892 | left edge at 433 px; nearest shared grid column 38 px (delta 395 px) |
| A1 | accept-as-legacy | x=465 y=8 w=12 h=892 | x=465 y=8 w=12 h=892 | left edge at 465 px; nearest shared grid column 38 px (delta 427 px) |
| A1 | accept-as-legacy | x=551 y=8 w=12 h=892 | x=551 y=8 w=12 h=892 | left edge at 551 px; nearest shared grid column 38 px (delta 513 px) |
| A1 | accept-as-legacy | x=596 y=8 w=38 h=892 | x=596 y=8 w=38 h=892 | left edge at 596 px; nearest shared grid column 38 px (delta 558 px) |
| A1 | accept-as-legacy | x=609 y=8 w=25 h=892 | x=609 y=8 w=25 h=892 | left edge at 609 px; nearest shared grid column 38 px (delta 571 px) |
| A1 | accept-as-legacy | x=766 y=8 w=14 h=892 | x=766 y=8 w=14 h=892 | left edge at 766 px; nearest shared grid column 38 px (delta 728 px) |
| A1 | accept-as-legacy | x=909 y=8 w=31 h=892 | x=909 y=8 w=31 h=892 | left edge at 909 px; nearest shared grid column 38 px (delta 871 px) |
| A1 | accept-as-legacy | x=914 y=8 w=26 h=892 | x=914 y=8 w=26 h=892 | left edge at 914 px; nearest shared grid column 38 px (delta 876 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `icon-probe-0.5-392`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| M1 | accept-as-legacy | x=0 y=26 w=392 h=1 | x=0 y=52 w=784 h=2 | top margin 52.0 logical px (26 px at scale 0.5); nearest rung 48 px |
| M1 | accept-as-legacy | x=0 y=440 w=392 h=1 | x=0 y=880 w=784 h=2 | bottom margin 54.0 logical px (27 px at scale 0.5); nearest rung 48 px |
| M1 | accept-as-legacy | x=17 y=0 w=1 h=468 | x=34 y=0 w=2 h=936 | left margin 34.0 logical px (17 px at scale 0.5); nearest rung 32 px |
| M1 | accept-as-legacy | x=370 y=0 w=1 h=468 | x=740 y=0 w=2 h=936 | right margin 42.0 logical px (21 px at scale 0.5); nearest rung 48 px |
| A1 | accept-as-legacy | x=104 y=60 w=44 h=44 | x=208 y=120 w=88 h=88 | left edge at 104 px; nearest shared grid column 75 px (delta 29 px) |
| A1 | accept-as-legacy | x=123 y=387 w=6 h=6 | x=246 y=774 w=12 h=12 | left edge at 123 px; nearest shared grid column 75 px (delta 48 px) |
| A1 | accept-as-legacy | x=152 y=60 w=44 h=44 | x=304 y=120 w=88 h=88 | left edge at 152 px; nearest shared grid column 171 px (delta 19 px) |
| A2 | fix-later | x=0 y=104 w=392 h=19 | x=0 y=208 w=784 h=38 | gap 19 px vs section median 27 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=143 w=392 h=42 | x=0 y=286 w=784 h=84 | gap 42 px vs section median 27 px (delta 15 px) |
| A2 | accept-as-legacy | x=0 y=249 w=392 h=18 | x=0 y=498 w=784 h=36 | gap 18 px vs section median 42 px (delta 24 px) |
| A2 | accept-as-legacy | x=0 y=273 w=392 h=18 | x=0 y=546 w=784 h=36 | gap 18 px vs section median 42 px (delta 24 px) |
| I1 | fix-later | x=25 y=26 w=10 h=7 | x=50 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=73 y=26 w=10 h=7 | x=146 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=121 y=26 w=10 h=7 | x=242 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=169 y=26 w=10 h=7 | x=338 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=217 y=26 w=10 h=7 | x=434 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=265 y=26 w=10 h=7 | x=530 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=313 y=26 w=10 h=7 | x=626 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=361 y=26 w=10 h=7 | x=722 y=52 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=104 y=60 w=44 h=44 | x=208 y=120 w=88 h=88 | icon bounding box 44 × 44 phys px → 88.0 × 88.0 logical px |
| I1 | fix-later | x=152 y=60 w=44 h=44 | x=304 y=120 w=88 h=88 | icon bounding box 44 × 44 phys px → 88.0 × 88.0 logical px |
| I1 | fix-later | x=25 y=78 w=10 h=7 | x=50 y=156 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=73 y=78 w=10 h=7 | x=146 y=156 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=112 y=123 w=28 h=20 | x=224 y=246 w=56 h=40 | icon bounding box 28 × 20 phys px → 56.0 × 40.0 logical px |
| I1 | fix-later | x=73 y=130 w=10 h=7 | x=146 y=260 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |
| I1 | fix-later | x=17 y=182 w=10 h=7 | x=34 y=364 w=20 h=14 | icon bounding box 10 × 7 phys px → 20.0 × 14.0 logical px |

### `icon-probe-1.0-392`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| M1 | accept-as-legacy | x=0 y=26 w=392 h=1 | x=0 y=26 w=392 h=1 | top margin 26.0 logical px (26 px at scale 1.0); nearest rung 24 px |
| M1 | accept-as-legacy | x=371 y=0 w=1 h=468 | x=371 y=0 w=1 h=468 | right margin 20.0 logical px (20 px at scale 1.0); nearest rung 16 px |
| A1 | accept-as-legacy | x=104 y=60 w=44 h=44 | x=104 y=60 w=44 h=44 | left edge at 104 px; nearest shared grid column 73 px (delta 31 px) |
| A1 | accept-as-legacy | x=152 y=60 w=44 h=44 | x=152 y=60 w=44 h=44 | left edge at 152 px; nearest shared grid column 73 px (delta 79 px) |
| A2 | accept-as-legacy | x=0 y=33 w=392 h=27 | x=0 y=33 w=392 h=27 | gap 27 px vs section median 36 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=104 w=392 h=19 | x=0 y=104 w=392 h=19 | gap 19 px vs section median 36 px (delta 17 px) |
| A2 | fix-later | x=0 y=143 w=392 h=42 | x=0 y=143 w=392 h=42 | gap 42 px vs section median 36 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=188 w=392 h=52 | x=0 y=188 w=392 h=52 | gap 52 px vs section median 36 px (delta 16 px) |
| A2 | accept-as-legacy | x=0 y=252 w=392 h=13 | x=0 y=252 w=392 h=13 | gap 13 px vs section median 36 px (delta 23 px) |
| A2 | accept-as-legacy | x=0 y=275 w=392 h=13 | x=0 y=275 w=392 h=13 | gap 13 px vs section median 36 px (delta 23 px) |
| T1 | accept-as-legacy | x=27 y=132 w=6 h=4 | x=27 y=132 w=6 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=74 y=340 w=8 h=4 | x=74 y=340 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=81 y=433 w=3 h=3 | x=81 y=433 w=3 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| Q1 | accept-as-legacy | x=266 y=240 w=9 h=12 | x=266 y=240 w=9 h=12 | ink RGB (94, 97, 110) on background RGB (20, 22, 28) → contrast 2.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=25 y=288 w=9 h=12 | x=25 y=288 w=9 h=12 | ink RGB (110, 114, 127) on background RGB (20, 22, 28) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=22 y=334 w=16 h=16 | x=22 y=334 w=16 h=16 | ink RGB (112, 116, 130) on background RGB (20, 22, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| I1 | fix-later | x=25 y=26 w=10 h=7 | x=25 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=73 y=26 w=10 h=7 | x=73 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=121 y=26 w=10 h=7 | x=121 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=169 y=26 w=10 h=7 | x=169 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=217 y=26 w=10 h=7 | x=217 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=265 y=26 w=10 h=7 | x=265 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=313 y=26 w=10 h=7 | x=313 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=361 y=26 w=10 h=7 | x=361 y=26 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=104 y=60 w=44 h=44 | x=104 y=60 w=44 h=44 | icon bounding box 44 × 44 phys px → 44.0 × 44.0 logical px |
| I1 | fix-later | x=152 y=60 w=44 h=44 | x=152 y=60 w=44 h=44 | icon bounding box 44 × 44 phys px → 44.0 × 44.0 logical px |
| I1 | fix-later | x=25 y=78 w=10 h=7 | x=25 y=78 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=73 y=78 w=10 h=7 | x=73 y=78 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=112 y=123 w=28 h=20 | x=112 y=123 w=28 h=20 | icon bounding box 28 × 20 phys px → 28.0 × 20.0 logical px |
| I1 | fix-later | x=73 y=130 w=10 h=7 | x=73 y=130 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=17 y=182 w=10 h=7 | x=17 y=182 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=121 y=240 w=10 h=9 | x=121 y=240 w=10 h=9 | icon bounding box 10 × 9 phys px → 10.0 × 9.0 logical px |
| I1 | fix-later | x=169 y=241 w=10 h=7 | x=169 y=241 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |
| I1 | fix-later | x=218 y=242 w=8 h=8 | x=218 y=242 w=8 h=8 | icon bounding box 8 × 8 phys px → 8.0 × 8.0 logical px |
| I1 | fix-later | x=268 y=242 w=5 h=8 | x=268 y=242 w=5 h=8 | icon bounding box 5 × 8 phys px → 5.0 × 8.0 logical px |
| I1 | fix-later | x=314 y=242 w=8 h=8 | x=314 y=242 w=8 h=8 | icon bounding box 8 × 8 phys px → 8.0 × 8.0 logical px |
| I1 | fix-later | x=170 y=265 w=9 h=10 | x=170 y=265 w=9 h=10 | icon bounding box 9 × 10 phys px → 9.0 × 10.0 logical px |
| I1 | fix-later | x=27 y=290 w=5 h=8 | x=27 y=290 w=5 h=8 | icon bounding box 5 × 8 phys px → 5.0 × 8.0 logical px |
| I1 | fix-later | x=121 y=290 w=10 h=7 | x=121 y=290 w=10 h=7 | icon bounding box 10 × 7 phys px → 10.0 × 7.0 logical px |

### `icon-probe-3.0-392`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=104 y=60 w=44 h=44 | x=35 y=20 w=15 h=15 | left edge at 104 px; nearest shared grid column 63 px (delta 41 px) |
| A1 | accept-as-legacy | x=152 y=60 w=44 h=44 | x=51 y=20 w=15 h=15 | left edge at 152 px; nearest shared grid column 63 px (delta 89 px) |
| A1 | accept-as-legacy | x=162 y=229 w=75 h=82 | x=54 y=76 w=25 h=27 | left edge at 162 px; nearest shared grid column 63 px (delta 99 px) |
| A2 | fix-later | x=0 y=33 w=392 h=27 | x=0 y=11 w=131 h=9 | gap 27 px vs section median 19 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=143 w=392 h=42 | x=0 y=48 w=131 h=14 | gap 42 px vs section median 19 px (delta 23 px) |
| A2 | accept-as-legacy | x=0 y=188 w=392 h=41 | x=0 y=63 w=131 h=14 | gap 41 px vs section median 19 px (delta 22 px) |
| T1 | accept-as-legacy | x=25 y=26 w=10 h=7 | x=8 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=73 y=26 w=10 h=7 | x=24 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=121 y=26 w=10 h=7 | x=40 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=169 y=26 w=10 h=7 | x=56 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=217 y=26 w=10 h=7 | x=72 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=265 y=26 w=10 h=7 | x=88 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=313 y=26 w=10 h=7 | x=104 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=361 y=26 w=10 h=7 | x=120 y=9 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=25 y=78 w=10 h=7 | x=8 y=26 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=73 y=78 w=10 h=7 | x=24 y=26 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=73 y=130 w=10 h=7 | x=24 y=43 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=27 y=132 w=6 h=4 | x=9 y=44 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=17 y=182 w=10 h=7 | x=6 y=61 w=3 h=2 | text line height 7 px (1 component) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=161 y=231 w=25 h=17 | x=54 y=77 w=8 h=6 | text line height 17 px (2 components) → em ≈ 8.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=354 y=244 w=24 h=4 | x=118 y=81 w=8 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=122 y=258 w=8 h=5 | x=41 y=86 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=313 y=277 w=10 h=4 | x=104 y=92 w=3 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=13 y=325 w=34 h=19 | x=4 y=108 w=11 h=6 | text line height 19 px (3 components) → em ≈ 9.0 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=67 y=336 w=22 h=12 | x=22 y=112 w=7 h=4 | text line height 12 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=13 y=340 w=34 h=19 | x=4 y=113 w=11 h=6 | text line height 19 px (3 components) → em ≈ 9.0 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=26 y=402 w=8 h=5 | x=9 y=134 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=87 y=423 w=8 h=8 | x=29 y=141 w=3 h=3 | text line height 8 px (1 component) → em ≈ 3.8 logical px; nearest ladder rung 11 px |
| Q1 | accept-as-legacy | x=102 y=58 w=96 h=48 | x=34 y=19 w=32 h=16 | ink RGB (38, 42, 53) on background RGB (20, 22, 28) → contrast 1.3:1; role class Large; scrim-blend ink |
| I1 | fix-later | x=25 y=26 w=10 h=7 | x=8 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=73 y=26 w=10 h=7 | x=24 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=121 y=26 w=10 h=7 | x=40 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=169 y=26 w=10 h=7 | x=56 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=217 y=26 w=10 h=7 | x=72 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=265 y=26 w=10 h=7 | x=88 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=313 y=26 w=10 h=7 | x=104 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=361 y=26 w=10 h=7 | x=120 y=9 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=25 y=78 w=10 h=7 | x=8 y=26 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=73 y=78 w=10 h=7 | x=24 y=26 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=112 y=123 w=28 h=20 | x=37 y=41 w=9 h=7 | icon bounding box 28 × 20 phys px → 9.3 × 6.7 logical px |
| I1 | fix-later | x=73 y=130 w=10 h=7 | x=24 y=43 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=17 y=182 w=10 h=7 | x=6 y=61 w=3 h=2 | icon bounding box 10 × 7 phys px → 3.3 × 2.3 logical px |
| I1 | fix-later | x=210 y=234 w=24 h=24 | x=70 y=78 w=8 h=8 | icon bounding box 24 × 24 phys px → 8.0 × 8.0 logical px |
| I1 | fix-later | x=307 y=235 w=22 h=22 | x=102 y=78 w=7 h=7 | icon bounding box 22 × 22 phys px → 7.3 × 7.3 logical px |
| I1 | fix-later | x=162 y=255 w=27 h=30 | x=54 y=85 w=9 h=10 | icon bounding box 27 × 30 phys px → 9.0 × 10.0 logical px |
| I1 | fix-later | x=361 y=277 w=22 h=23 | x=120 y=92 w=7 h=8 | icon bounding box 22 × 23 phys px → 7.3 × 7.7 logical px |
| I1 | fix-later | x=112 y=283 w=28 h=20 | x=37 y=94 w=9 h=7 | icon bounding box 28 × 20 phys px → 9.3 × 6.7 logical px |
| I1 | fix-later | x=87 y=423 w=8 h=8 | x=29 y=141 w=3 h=3 | icon bounding box 8 × 8 phys px → 2.7 × 2.7 logical px |

### `kiosk-board-0.5-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=33 w=1 h=3 | x=0 y=66 w=2 h=6 | content run touches the left edge (extent 3 px along the edge) |
| A1 | accept-as-legacy | x=20 y=890 w=510 h=8 | x=40 y=1780 w=1020 h=16 | left edge at 20 px; nearest shared grid column 12 px (delta 8 px) |
| A1 | accept-as-legacy | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 12 px (delta 450 px) |
| A1 | accept-as-legacy | x=494 y=890 w=36 h=8 | x=988 y=1780 w=72 h=16 | left edge at 494 px; nearest shared grid column 12 px (delta 482 px) |
| A1 | accept-as-legacy | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 12 px (delta 484 px) |
| A1 | accept-as-legacy | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 12 px (delta 491 px) |
| A1 | accept-as-legacy | x=953 y=60 w=7 h=20 | x=1906 y=120 w=14 h=40 | left edge at 953 px; nearest shared grid column 999 px (delta 46 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 999 px (delta 19 px) |
| A2 | accept-as-legacy | x=0 y=45 w=1000 h=15 | x=0 y=90 w=2000 h=30 | gap 15 px vs section median 6 px (delta 9 px) |
| A2 | fix-later | x=0 y=80 w=1000 h=2 | x=0 y=160 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |

### `kiosk-today-0.5-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=12 y=874 w=19 h=7 | x=24 y=1748 w=38 h=14 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | fix-later | x=20 y=890 w=510 h=8 | x=40 y=1780 w=1020 h=16 | left edge at 20 px; nearest shared grid column 23 px (delta 3 px) |
| A1 | fix-later | x=28 y=57 w=10 h=7 | x=56 y=114 w=20 h=14 | left edge at 28 px; nearest shared grid column 30 px (delta 2 px) |
| A1 | fix-later | x=35 y=856 w=9 h=8 | x=70 y=1712 w=18 h=16 | left edge at 35 px; nearest shared grid column 30 px (delta 5 px) |
| A1 | accept-as-legacy | x=49 y=57 w=33 h=7 | x=98 y=114 w=66 h=14 | left edge at 49 px; nearest shared grid column 30 px (delta 19 px) |
| A1 | accept-as-legacy | x=51 y=856 w=15 h=8 | x=102 y=1712 w=30 h=16 | left edge at 51 px; nearest shared grid column 30 px (delta 21 px) |
| A1 | accept-as-legacy | x=60 y=57 w=22 h=7 | x=120 y=114 w=44 h=14 | left edge at 60 px; nearest shared grid column 30 px (delta 30 px) |
| A1 | accept-as-legacy | x=65 y=57 w=17 h=7 | x=130 y=114 w=34 h=14 | left edge at 65 px; nearest shared grid column 30 px (delta 35 px) |
| A1 | accept-as-legacy | x=68 y=856 w=19 h=8 | x=136 y=1712 w=38 h=16 | left edge at 68 px; nearest shared grid column 30 px (delta 38 px) |
| A1 | accept-as-legacy | x=116 y=57 w=10 h=7 | x=232 y=114 w=20 h=14 | left edge at 116 px; nearest shared grid column 30 px (delta 86 px) |
| A1 | accept-as-legacy | x=127 y=856 w=23 h=8 | x=254 y=1712 w=46 h=16 | left edge at 127 px; nearest shared grid column 210 px (delta 83 px) |
| A1 | accept-as-legacy | x=159 y=57 w=17 h=7 | x=318 y=114 w=34 h=14 | left edge at 159 px; nearest shared grid column 210 px (delta 51 px) |
| A1 | accept-as-legacy | x=176 y=856 w=24 h=8 | x=352 y=1712 w=48 h=16 | left edge at 176 px; nearest shared grid column 210 px (delta 34 px) |
| A1 | accept-as-legacy | x=178 y=57 w=31 h=7 | x=356 y=114 w=62 h=14 | left edge at 178 px; nearest shared grid column 210 px (delta 32 px) |
| A1 | accept-as-legacy | x=182 y=57 w=27 h=7 | x=364 y=114 w=54 h=14 | left edge at 182 px; nearest shared grid column 210 px (delta 28 px) |
| A1 | accept-as-legacy | x=196 y=57 w=13 h=7 | x=392 y=114 w=26 h=14 | left edge at 196 px; nearest shared grid column 210 px (delta 14 px) |
| A1 | accept-as-legacy | x=202 y=57 w=7 h=7 | x=404 y=114 w=14 h=14 | left edge at 202 px; nearest shared grid column 210 px (delta 8 px) |
| A1 | accept-as-legacy | x=226 y=57 w=26 h=7 | x=452 y=114 w=52 h=14 | left edge at 226 px; nearest shared grid column 211 px (delta 15 px) |
| A1 | accept-as-legacy | x=472 y=4 w=12 h=24 | x=944 y=8 w=24 h=48 | left edge at 472 px; nearest shared grid column 211 px (delta 261 px) |
| A1 | accept-as-legacy | x=485 y=4 w=6 h=24 | x=970 y=8 w=12 h=48 | left edge at 485 px; nearest shared grid column 211 px (delta 274 px) |
| A1 | accept-as-legacy | x=494 y=890 w=36 h=8 | x=988 y=1780 w=72 h=16 | left edge at 494 px; nearest shared grid column 211 px (delta 283 px) |
| A1 | accept-as-legacy | x=496 y=4 w=29 h=24 | x=992 y=8 w=58 h=48 | left edge at 496 px; nearest shared grid column 211 px (delta 285 px) |
| A1 | accept-as-legacy | x=630 y=874 w=18 h=7 | x=1260 y=1748 w=36 h=14 | left edge at 630 px; nearest shared grid column 960 px (delta 330 px) |
| A1 | accept-as-legacy | x=638 y=874 w=10 h=7 | x=1276 y=1748 w=20 h=14 | left edge at 638 px; nearest shared grid column 960 px (delta 322 px) |
| A1 | accept-as-legacy | x=642 y=874 w=6 h=7 | x=1284 y=1748 w=12 h=14 | left edge at 642 px; nearest shared grid column 960 px (delta 318 px) |
| A1 | accept-as-legacy | x=653 y=874 w=313 h=7 | x=1306 y=1748 w=626 h=14 | left edge at 653 px; nearest shared grid column 960 px (delta 307 px) |
| A1 | accept-as-legacy | x=693 y=856 w=18 h=8 | x=1386 y=1712 w=36 h=16 | left edge at 693 px; nearest shared grid column 960 px (delta 267 px) |
| A1 | accept-as-legacy | x=701 y=856 w=10 h=8 | x=1402 y=1712 w=20 h=16 | left edge at 701 px; nearest shared grid column 960 px (delta 259 px) |
| A1 | accept-as-legacy | x=705 y=856 w=6 h=8 | x=1410 y=1712 w=12 h=16 | left edge at 705 px; nearest shared grid column 960 px (delta 255 px) |
| A1 | accept-as-legacy | x=716 y=856 w=250 h=8 | x=1432 y=1712 w=500 h=16 | left edge at 716 px; nearest shared grid column 960 px (delta 244 px) |
| A1 | accept-as-legacy | x=975 y=86 w=10 h=6 | x=1950 y=172 w=20 h=12 | left edge at 975 px; nearest shared grid column 960 px (delta 15 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 999 px (delta 19 px) |
| A2 | fix-later | x=0 y=28 w=1000 h=4 | x=0 y=56 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=92 w=1000 h=12 | x=0 y=184 w=2000 h=24 | gap 12 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=109 w=1000 h=4 | x=0 y=218 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=139 w=1000 h=5 | x=0 y=278 w=2000 h=10 | gap 5 px vs section median 8 px (delta 3 px) |
| A2 | fix-later | x=0 y=145 w=1000 h=1 | x=0 y=290 w=2000 h=2 | gap 1 px vs section median 8 px (delta 7 px) |
| A2 | fix-later | x=0 y=850 w=1000 h=6 | x=0 y=1700 w=2000 h=12 | gap 6 px vs section median 9 px (delta 3 px) |

### `labels-overlay-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=999 y=399 w=1 h=6 | x=999 y=399 w=1 h=6 | content run touches the right edge (extent 6 px along the edge) |
| C1 | fix-now | x=0 y=397 w=1 h=31 | x=0 y=397 w=1 h=31 | content run touches the left edge (extent 31 px along the edge) |
| A1 | fix-later | x=9 y=370 w=15 h=121 | x=9 y=370 w=15 h=121 | left edge at 9 px; nearest shared grid column 14 px (delta 5 px) |
| A1 | accept-as-legacy | x=121 y=71 w=29 h=20 | x=121 y=71 w=29 h=20 | left edge at 121 px; nearest shared grid column 108 px (delta 13 px) |
| A1 | accept-as-legacy | x=127 y=370 w=20 h=121 | x=127 y=370 w=20 h=121 | left edge at 127 px; nearest shared grid column 144 px (delta 17 px) |
| A1 | fix-later | x=150 y=370 w=15 h=121 | x=150 y=370 w=15 h=121 | left edge at 150 px; nearest shared grid column 146 px (delta 4 px) |
| A1 | accept-as-legacy | x=213 y=370 w=15 h=121 | x=213 y=370 w=15 h=121 | left edge at 213 px; nearest shared grid column 226 px (delta 13 px) |
| A1 | accept-as-legacy | x=271 y=370 w=409 h=121 | x=271 y=370 w=409 h=121 | left edge at 271 px; nearest shared grid column 244 px (delta 27 px) |
| A1 | accept-as-legacy | x=283 y=370 w=397 h=121 | x=283 y=370 w=397 h=121 | left edge at 283 px; nearest shared grid column 244 px (delta 39 px) |
| A1 | accept-as-legacy | x=313 y=370 w=367 h=121 | x=313 y=370 w=367 h=121 | left edge at 313 px; nearest shared grid column 364 px (delta 51 px) |
| A1 | accept-as-legacy | x=320 y=370 w=360 h=121 | x=320 y=370 w=360 h=121 | left edge at 320 px; nearest shared grid column 364 px (delta 44 px) |
| A1 | accept-as-legacy | x=433 y=24 w=38 h=16 | x=433 y=24 w=38 h=16 | left edge at 433 px; nearest shared grid column 366 px (delta 67 px) |
| A1 | accept-as-legacy | x=459 y=24 w=12 h=16 | x=459 y=24 w=12 h=16 | left edge at 459 px; nearest shared grid column 366 px (delta 93 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 13 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 13 px (delta 12 px) |
| A2 | fix-later | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 13 px (delta 7 px) |
| A2 | fix-later | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 13 px (delta 6 px) |
| A2 | fix-later | x=0 y=137 w=1000 h=19 | x=0 y=137 w=1000 h=19 | gap 19 px vs section median 13 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=167 w=1000 h=4 | x=0 y=167 w=1000 h=4 | gap 4 px vs section median 13 px (delta 9 px) |
| A2 | fix-later | x=0 y=174 w=1000 h=6 | x=0 y=174 w=1000 h=6 | gap 6 px vs section median 13 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=182 w=1000 h=33 | x=0 y=182 w=1000 h=33 | gap 33 px vs section median 13 px (delta 20 px) |
| A2 | accept-as-legacy | x=0 y=227 w=1000 h=33 | x=0 y=227 w=1000 h=33 | gap 33 px vs section median 13 px (delta 20 px) |
| A2 | accept-as-legacy | x=0 y=269 w=1000 h=35 | x=0 y=269 w=1000 h=35 | gap 35 px vs section median 13 px (delta 22 px) |
| A2 | accept-as-legacy | x=0 y=305 w=1000 h=2 | x=0 y=305 w=1000 h=2 | gap 2 px vs section median 13 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=309 w=1000 h=2 | x=0 y=309 w=1000 h=2 | gap 2 px vs section median 13 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=312 w=1000 h=28 | x=0 y=312 w=1000 h=28 | gap 28 px vs section median 13 px (delta 15 px) |
| T1 | accept-as-legacy | x=424 y=8 w=4 h=3 | x=424 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=424 y=53 w=4 h=3 | x=424 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=228 y=71 w=9 h=4 | x=228 y=71 w=9 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=3 h=4 | x=298 y=71 w=3 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=3 h=4 | x=658 y=71 w=3 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=488 y=171 w=8 h=4 | x=488 y=171 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=986 y=171 w=8 h=4 | x=986 y=171 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=567 y=9 w=11 h=12 | x=567 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=431 y=24 w=34 h=16 | x=431 y=24 w=34 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=461 y=24 w=12 h=16 | x=461 y=24 w=12 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=487 y=24 w=19 h=15 | x=487 y=24 w=19 h=15 | ink RGB (85, 88, 100) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=24 w=27 h=18 | x=503 y=24 w=27 h=18 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=42 y=70 w=20 h=20 | x=42 y=70 w=20 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=71 w=13 h=14 | x=976 y=71 w=13 h=14 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=191 y=74 w=48 h=16 | x=191 y=74 w=48 h=16 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=561 y=74 w=15 h=16 | x=561 y=74 w=15 h=16 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=638 y=74 w=21 h=15 | x=638 y=74 w=21 h=15 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=109 w=27 h=13 | x=0 y=109 w=27 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=109 w=10 h=13 | x=24 y=109 w=10 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=240 y=109 w=25 h=13 | x=240 y=109 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=266 y=109 w=28 h=13 | x=266 y=109 w=28 h=13 | ink RGB (83, 86, 98) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=501 y=109 w=30 h=13 | x=501 y=109 w=30 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=109 w=28 h=13 | x=531 y=109 w=28 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=766 y=109 w=32 h=13 | x=766 y=109 w=32 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=279 y=110 w=8 h=12 | x=279 y=110 w=8 h=12 | ink RGB (83, 86, 98) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=545 y=110 w=7 h=12 | x=545 y=110 w=7 h=12 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=364 y=125 w=25 h=13 | x=364 y=125 w=25 h=13 | ink RGB (120, 102, 60) on background RGB (14, 16, 22) → contrast 3.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=127 w=9 h=12 | x=0 y=127 w=9 h=12 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=153 w=30 h=16 | x=0 y=153 w=30 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=513 y=153 w=51 h=16 | x=513 y=153 w=51 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=27 y=154 w=39 h=12 | x=27 y=154 w=39 h=12 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=98 y=213 w=38 h=16 | x=98 y=213 w=38 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=136 y=213 w=47 h=16 | x=136 y=213 w=47 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=214 w=24 h=13 | x=0 y=214 w=24 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=21 y=214 w=29 h=13 | x=21 y=214 w=29 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=46 y=214 w=39 h=13 | x=46 y=214 w=39 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=70 y=215 w=8 h=12 | x=70 y=215 w=8 h=12 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=257 w=12 h=12 | x=976 y=257 w=12 h=12 | ink RGB (69, 72, 82) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=258 w=15 h=13 | x=0 y=258 w=15 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=12 y=258 w=27 h=13 | x=12 y=258 w=27 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=258 w=6 h=13 | x=26 y=258 w=6 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=7 y=299 w=14 h=16 | x=7 y=299 w=14 h=16 | ink RGB (70, 73, 84) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=338 w=27 h=13 | x=0 y=338 w=27 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=338 w=23 h=13 | x=24 y=338 w=23 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=975 y=344 w=14 h=14 | x=975 y=344 w=14 h=14 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=520 y=346 w=17 h=13 | x=520 y=346 w=17 h=13 | ink RGB (113, 115, 121) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=382 w=36 h=13 | x=0 y=382 w=36 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=382 w=5 h=13 | x=24 y=382 w=5 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=33 y=382 w=29 h=13 | x=33 y=382 w=29 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=986 y=394 w=14 h=16 | x=986 y=394 w=14 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=242 y=405 w=30 h=16 | x=242 y=405 w=30 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=23 y=406 w=17 h=13 | x=23 y=406 w=17 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=78 y=406 w=25 h=13 | x=78 y=406 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=141 y=406 w=17 h=13 | x=141 y=406 w=17 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=196 y=406 w=25 h=13 | x=196 y=406 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=268 y=406 w=28 h=15 | x=268 y=406 w=28 h=15 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=437 w=12 h=12 | x=976 y=437 w=12 h=12 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=438 w=53 h=13 | x=0 y=438 w=53 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=50 y=438 w=53 h=13 | x=50 y=438 w=53 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=84 y=438 w=6 h=13 | x=84 y=438 w=6 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=118 y=438 w=31 h=13 | x=118 y=438 w=31 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=103 y=439 w=15 h=12 | x=103 y=439 w=15 h=12 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=474 w=45 h=16 | x=0 y=474 w=45 h=16 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=42 y=475 w=24 h=12 | x=42 y=475 w=24 h=12 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `notification-3.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | fix-now | x=999 y=209 w=1 h=133 | x=333 y=70 w=0 h=44 | content run touches the right edge (extent 133 px along the edge) |
| C1 | accept-as-legacy | x=999 y=429 w=1 h=3 | x=333 y=143 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) |
| C1 | fix-now | x=0 y=209 w=1 h=206 | x=0 y=70 w=0 h=69 | content run touches the left edge (extent 206 px along the edge) |
| A1 | accept-as-legacy | x=225 y=192 w=99 h=240 | x=75 y=64 w=33 h=80 | left edge at 225 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=337 y=24 w=111 h=144 | x=112 y=8 w=37 h=48 | left edge at 337 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=348 y=192 w=59 h=240 | x=116 y=64 w=20 h=80 | left edge at 348 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=475 y=24 w=170 h=144 | x=158 y=8 w=57 h=48 | left edge at 475 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=573 y=792 w=211 h=96 | x=191 y=264 w=70 h=32 | left edge at 573 px; no other left edge shares this column |
| A1 | accept-as-legacy | x=660 y=192 w=192 h=240 | x=220 y=64 w=64 h=80 | left edge at 660 px; no other left edge shares this column |
| A2 | accept-as-legacy | x=0 y=168 w=1000 h=24 | x=0 y=56 w=333 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | x=0 y=432 w=1000 h=37 | x=0 y=144 w=333 h=12 | gap 37 px vs section median 56 px (delta 19 px) |
| A2 | fix-later | x=0 y=513 w=1000 h=64 | x=0 y=171 w=333 h=21 | gap 64 px vs section median 56 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=621 w=1000 h=76 | x=0 y=207 w=333 h=25 | gap 76 px vs section median 56 px (delta 20 px) |
| T1 | accept-as-legacy | x=884 y=54 w=28 h=3 | x=295 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=64 w=28 h=4 | x=295 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=75 w=28 h=3 | x=295 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=837 y=253 w=10 h=4 | x=279 y=84 w=3 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=61 y=262 w=5 h=4 | x=20 y=87 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=164 y=477 w=6 h=5 | x=55 y=159 w=2 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=969 y=498 w=8 h=5 | x=323 y=166 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=75 y=501 w=30 h=7 | x=25 y=167 w=10 h=2 | text line height 7 px (3 components) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=299 y=585 w=6 h=5 | x=100 y=195 w=2 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=894 y=726 w=8 h=5 | x=298 y=242 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `plan-next-day-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 24 px (delta 8 px) |
| A1 | accept-as-legacy | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 24 px (delta 321 px) |
| A1 | accept-as-legacy | x=433 y=8 w=38 h=48 | x=433 y=8 w=38 h=48 | left edge at 433 px; nearest shared grid column 24 px (delta 409 px) |
| A1 | accept-as-legacy | x=459 y=8 w=12 h=48 | x=459 y=8 w=12 h=48 | left edge at 459 px; nearest shared grid column 24 px (delta 435 px) |
| A1 | accept-as-legacy | x=480 y=8 w=57 h=48 | x=480 y=8 w=57 h=48 | left edge at 480 px; nearest shared grid column 24 px (delta 456 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `plan-next-day-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 24 px (delta 8 px) |
| A1 | accept-as-legacy | x=512 y=864 w=71 h=32 | x=512 y=864 w=71 h=32 | left edge at 512 px; nearest shared grid column 24 px (delta 488 px) |
| A1 | accept-as-legacy | x=683 y=8 w=38 h=48 | x=683 y=8 w=38 h=48 | left edge at 683 px; nearest shared grid column 24 px (delta 659 px) |
| A1 | accept-as-legacy | x=709 y=8 w=12 h=48 | x=709 y=8 w=12 h=48 | left edge at 709 px; nearest shared grid column 24 px (delta 685 px) |
| A1 | accept-as-legacy | x=730 y=8 w=57 h=48 | x=730 y=8 w=57 h=48 | left edge at 730 px; nearest shared grid column 24 px (delta 706 px) |
| A2 | fix-later | x=0 y=848 w=1500 h=7 | x=0 y=848 w=1500 h=7 | gap 7 px vs section median 4 px (delta 3 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `plan-next-day-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 24 px (delta 8 px) |
| A1 | fix-later | x=29 y=64 w=12 h=796 | x=29 y=64 w=12 h=796 | left edge at 29 px; nearest shared grid column 26 px (delta 3 px) |
| A1 | accept-as-legacy | x=53 y=64 w=530 h=796 | x=53 y=64 w=530 h=796 | left edge at 53 px; nearest shared grid column 26 px (delta 27 px) |
| A1 | accept-as-legacy | x=60 y=64 w=523 h=796 | x=60 y=64 w=523 h=796 | left edge at 60 px; nearest shared grid column 26 px (delta 34 px) |
| A1 | accept-as-legacy | x=212 y=864 w=71 h=32 | x=212 y=864 w=71 h=32 | left edge at 212 px; nearest shared grid column 26 px (delta 186 px) |
| A1 | accept-as-legacy | x=436 y=8 w=38 h=48 | x=436 y=8 w=38 h=48 | left edge at 436 px; nearest shared grid column 26 px (delta 410 px) |
| A1 | accept-as-legacy | x=462 y=8 w=12 h=48 | x=462 y=8 w=12 h=48 | left edge at 462 px; nearest shared grid column 26 px (delta 436 px) |
| A1 | accept-as-legacy | x=483 y=8 w=57 h=48 | x=483 y=8 w=57 h=48 | left edge at 483 px; nearest shared grid column 26 px (delta 457 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `recur-editor-detach-armed-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=999 y=355 w=1 h=6 | x=999 y=355 w=1 h=6 | content run touches the right edge (extent 6 px along the edge) |
| C1 | fix-now | x=0 y=353 w=1 h=31 | x=0 y=353 w=1 h=31 | content run touches the left edge (extent 31 px along the edge) |
| A1 | fix-later | x=10 y=286 w=14 h=289 | x=10 y=286 w=14 h=289 | left edge at 10 px; nearest shared grid column 14 px (delta 4 px) |
| A1 | accept-as-legacy | x=121 y=71 w=29 h=20 | x=121 y=71 w=29 h=20 | left edge at 121 px; nearest shared grid column 108 px (delta 13 px) |
| A1 | accept-as-legacy | x=127 y=286 w=20 h=289 | x=127 y=286 w=20 h=289 | left edge at 127 px; nearest shared grid column 144 px (delta 17 px) |
| A1 | fix-later | x=150 y=286 w=15 h=289 | x=150 y=286 w=15 h=289 | left edge at 150 px; nearest shared grid column 146 px (delta 4 px) |
| A1 | accept-as-legacy | x=213 y=286 w=158 h=289 | x=213 y=286 w=158 h=289 | left edge at 213 px; nearest shared grid column 242 px (delta 29 px) |
| A1 | accept-as-legacy | x=220 y=286 w=151 h=289 | x=220 y=286 w=151 h=289 | left edge at 220 px; nearest shared grid column 242 px (delta 22 px) |
| A1 | accept-as-legacy | x=433 y=24 w=38 h=16 | x=433 y=24 w=38 h=16 | left edge at 433 px; nearest shared grid column 451 px (delta 18 px) |
| A1 | accept-as-legacy | x=590 y=286 w=14 h=289 | x=590 y=286 w=14 h=289 | left edge at 590 px; nearest shared grid column 598 px (delta 8 px) |
| A1 | accept-as-legacy | x=614 y=286 w=166 h=289 | x=614 y=286 w=166 h=289 | left edge at 614 px; nearest shared grid column 605 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 15 px (delta 14 px) |
| A2 | fix-later | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 15 px (delta 5 px) |
| A2 | fix-later | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 15 px (delta 8 px) |
| A2 | fix-later | x=0 y=137 w=1000 h=19 | x=0 y=137 w=1000 h=19 | gap 19 px vs section median 15 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=167 w=1000 h=4 | x=0 y=167 w=1000 h=4 | gap 4 px vs section median 15 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=174 w=1000 h=6 | x=0 y=174 w=1000 h=6 | gap 6 px vs section median 15 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=182 w=1000 h=33 | x=0 y=182 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| A2 | accept-as-legacy | x=0 y=227 w=1000 h=33 | x=0 y=227 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| T1 | accept-as-legacy | x=424 y=8 w=4 h=3 | x=424 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=424 y=53 w=4 h=3 | x=424 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=228 y=71 w=9 h=4 | x=228 y=71 w=9 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=3 h=4 | x=298 y=71 w=3 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=3 h=4 | x=658 y=71 w=3 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=488 y=171 w=8 h=4 | x=488 y=171 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=986 y=171 w=8 h=4 | x=986 y=171 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=567 y=9 w=11 h=12 | x=567 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=431 y=24 w=34 h=16 | x=431 y=24 w=34 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=461 y=24 w=12 h=16 | x=461 y=24 w=12 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=487 y=24 w=19 h=15 | x=487 y=24 w=19 h=15 | ink RGB (85, 88, 100) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=24 w=27 h=18 | x=503 y=24 w=27 h=18 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=42 y=70 w=20 h=20 | x=42 y=70 w=20 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=71 w=13 h=14 | x=976 y=71 w=13 h=14 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=191 y=74 w=48 h=16 | x=191 y=74 w=48 h=16 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=561 y=74 w=15 h=16 | x=561 y=74 w=15 h=16 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=638 y=74 w=21 h=15 | x=638 y=74 w=21 h=15 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=109 w=27 h=13 | x=0 y=109 w=27 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=109 w=10 h=13 | x=24 y=109 w=10 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=240 y=109 w=25 h=13 | x=240 y=109 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=266 y=109 w=28 h=13 | x=266 y=109 w=28 h=13 | ink RGB (83, 86, 98) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=501 y=109 w=30 h=13 | x=501 y=109 w=30 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=109 w=28 h=13 | x=531 y=109 w=28 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=766 y=109 w=32 h=13 | x=766 y=109 w=32 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=279 y=110 w=8 h=12 | x=279 y=110 w=8 h=12 | ink RGB (83, 86, 98) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=545 y=110 w=7 h=12 | x=545 y=110 w=7 h=12 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=127 w=9 h=12 | x=0 y=127 w=9 h=12 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=153 w=30 h=16 | x=0 y=153 w=30 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=513 y=153 w=51 h=16 | x=513 y=153 w=51 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=27 y=154 w=39 h=12 | x=27 y=154 w=39 h=12 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=98 y=213 w=38 h=16 | x=98 y=213 w=38 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=136 y=213 w=47 h=16 | x=136 y=213 w=47 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=214 w=24 h=13 | x=0 y=214 w=24 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=21 y=214 w=29 h=13 | x=21 y=214 w=29 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=46 y=214 w=39 h=13 | x=46 y=214 w=39 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=70 y=215 w=8 h=12 | x=70 y=215 w=8 h=12 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=257 w=12 h=12 | x=976 y=257 w=12 h=12 | ink RGB (69, 72, 82) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=258 w=15 h=13 | x=0 y=258 w=15 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=12 y=258 w=27 h=13 | x=12 y=258 w=27 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=258 w=6 h=13 | x=26 y=258 w=6 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=294 w=27 h=13 | x=0 y=294 w=27 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=294 w=23 h=13 | x=24 y=294 w=23 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=975 y=300 w=14 h=14 | x=975 y=300 w=14 h=14 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=338 w=36 h=13 | x=0 y=338 w=36 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=338 w=5 h=13 | x=24 y=338 w=5 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=33 y=338 w=29 h=13 | x=33 y=338 w=29 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=986 y=350 w=14 h=16 | x=986 y=350 w=14 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=23 y=362 w=17 h=13 | x=23 y=362 w=17 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=78 y=362 w=25 h=13 | x=78 y=362 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=141 y=362 w=17 h=13 | x=141 y=362 w=17 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=393 w=12 h=12 | x=976 y=393 w=12 h=12 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=394 w=53 h=13 | x=0 y=394 w=53 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=50 y=394 w=53 h=13 | x=50 y=394 w=53 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=84 y=394 w=6 h=13 | x=84 y=394 w=6 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=118 y=394 w=31 h=13 | x=118 y=394 w=31 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=103 y=395 w=15 h=12 | x=103 y=395 w=15 h=12 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=975 y=435 w=14 h=16 | x=975 y=435 w=14 h=16 | ink RGB (70, 73, 84) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=438 w=15 h=13 | x=0 y=438 w=15 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=13 y=438 w=15 h=16 | x=13 y=438 w=15 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=7 y=471 w=14 h=16 | x=7 y=471 w=14 h=16 | ink RGB (70, 73, 84) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=48 y=472 w=13 h=14 | x=48 y=472 w=13 h=14 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=510 w=45 h=16 | x=0 y=510 w=45 h=16 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=42 y=511 w=24 h=12 | x=42 y=511 w=24 h=12 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `recur-editor-modal-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=999 y=355 w=1 h=6 | x=999 y=355 w=1 h=6 | content run touches the right edge (extent 6 px along the edge) |
| C1 | fix-now | x=0 y=353 w=1 h=31 | x=0 y=353 w=1 h=31 | content run touches the left edge (extent 31 px along the edge) |
| A1 | fix-later | x=10 y=286 w=14 h=289 | x=10 y=286 w=14 h=289 | left edge at 10 px; nearest shared grid column 14 px (delta 4 px) |
| A1 | accept-as-legacy | x=121 y=71 w=29 h=20 | x=121 y=71 w=29 h=20 | left edge at 121 px; nearest shared grid column 108 px (delta 13 px) |
| A1 | accept-as-legacy | x=127 y=286 w=20 h=289 | x=127 y=286 w=20 h=289 | left edge at 127 px; nearest shared grid column 144 px (delta 17 px) |
| A1 | fix-later | x=150 y=286 w=15 h=289 | x=150 y=286 w=15 h=289 | left edge at 150 px; nearest shared grid column 146 px (delta 4 px) |
| A1 | accept-as-legacy | x=213 y=286 w=76 h=289 | x=213 y=286 w=76 h=289 | left edge at 213 px; nearest shared grid column 242 px (delta 29 px) |
| A1 | accept-as-legacy | x=220 y=286 w=69 h=289 | x=220 y=286 w=69 h=289 | left edge at 220 px; nearest shared grid column 242 px (delta 22 px) |
| A1 | accept-as-legacy | x=384 y=286 w=14 h=289 | x=384 y=286 w=14 h=289 | left edge at 384 px; nearest shared grid column 366 px (delta 18 px) |
| A1 | accept-as-legacy | x=408 y=286 w=372 h=289 | x=408 y=286 w=372 h=289 | left edge at 408 px; nearest shared grid column 421 px (delta 13 px) |
| A1 | accept-as-legacy | x=433 y=24 w=38 h=16 | x=433 y=24 w=38 h=16 | left edge at 433 px; nearest shared grid column 422 px (delta 11 px) |
| A1 | accept-as-legacy | x=459 y=24 w=12 h=16 | x=459 y=24 w=12 h=16 | left edge at 459 px; nearest shared grid column 422 px (delta 37 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 15 px (delta 14 px) |
| A2 | fix-later | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 15 px (delta 5 px) |
| A2 | fix-later | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 15 px (delta 8 px) |
| A2 | fix-later | x=0 y=137 w=1000 h=19 | x=0 y=137 w=1000 h=19 | gap 19 px vs section median 15 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=167 w=1000 h=4 | x=0 y=167 w=1000 h=4 | gap 4 px vs section median 15 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=174 w=1000 h=6 | x=0 y=174 w=1000 h=6 | gap 6 px vs section median 15 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=182 w=1000 h=33 | x=0 y=182 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| A2 | accept-as-legacy | x=0 y=227 w=1000 h=33 | x=0 y=227 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| T1 | accept-as-legacy | x=424 y=8 w=4 h=3 | x=424 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=424 y=53 w=4 h=3 | x=424 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=228 y=71 w=9 h=4 | x=228 y=71 w=9 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=3 h=4 | x=298 y=71 w=3 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=3 h=4 | x=658 y=71 w=3 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=488 y=171 w=8 h=4 | x=488 y=171 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=986 y=171 w=8 h=4 | x=986 y=171 w=8 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=567 y=9 w=11 h=12 | x=567 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=431 y=24 w=34 h=16 | x=431 y=24 w=34 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=461 y=24 w=12 h=16 | x=461 y=24 w=12 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=487 y=24 w=19 h=15 | x=487 y=24 w=19 h=15 | ink RGB (85, 88, 100) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=24 w=27 h=18 | x=503 y=24 w=27 h=18 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=42 y=70 w=20 h=20 | x=42 y=70 w=20 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=71 w=13 h=14 | x=976 y=71 w=13 h=14 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=191 y=74 w=48 h=16 | x=191 y=74 w=48 h=16 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=561 y=74 w=15 h=16 | x=561 y=74 w=15 h=16 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=638 y=74 w=21 h=15 | x=638 y=74 w=21 h=15 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=109 w=27 h=13 | x=0 y=109 w=27 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=109 w=10 h=13 | x=24 y=109 w=10 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=240 y=109 w=25 h=13 | x=240 y=109 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=266 y=109 w=28 h=13 | x=266 y=109 w=28 h=13 | ink RGB (83, 86, 98) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=501 y=109 w=30 h=13 | x=501 y=109 w=30 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=109 w=28 h=13 | x=531 y=109 w=28 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=766 y=109 w=32 h=13 | x=766 y=109 w=32 h=13 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=279 y=110 w=8 h=12 | x=279 y=110 w=8 h=12 | ink RGB (83, 86, 98) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=545 y=110 w=7 h=12 | x=545 y=110 w=7 h=12 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=127 w=9 h=12 | x=0 y=127 w=9 h=12 | ink RGB (120, 122, 128) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=153 w=30 h=16 | x=0 y=153 w=30 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=513 y=153 w=51 h=16 | x=513 y=153 w=51 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=27 y=154 w=39 h=12 | x=27 y=154 w=39 h=12 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=98 y=213 w=38 h=16 | x=98 y=213 w=38 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=136 y=213 w=47 h=16 | x=136 y=213 w=47 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=214 w=24 h=13 | x=0 y=214 w=24 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=21 y=214 w=29 h=13 | x=21 y=214 w=29 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=46 y=214 w=39 h=13 | x=46 y=214 w=39 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=70 y=215 w=8 h=12 | x=70 y=215 w=8 h=12 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=257 w=12 h=12 | x=976 y=257 w=12 h=12 | ink RGB (69, 72, 82) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=258 w=15 h=13 | x=0 y=258 w=15 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=12 y=258 w=27 h=13 | x=12 y=258 w=27 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=258 w=6 h=13 | x=26 y=258 w=6 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=294 w=27 h=13 | x=0 y=294 w=27 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=294 w=23 h=13 | x=24 y=294 w=23 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=975 y=300 w=14 h=14 | x=975 y=300 w=14 h=14 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=338 w=36 h=13 | x=0 y=338 w=36 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=24 y=338 w=5 h=13 | x=24 y=338 w=5 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=33 y=338 w=29 h=13 | x=33 y=338 w=29 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=986 y=350 w=14 h=16 | x=986 y=350 w=14 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=23 y=362 w=17 h=13 | x=23 y=362 w=17 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=78 y=362 w=25 h=13 | x=78 y=362 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=141 y=362 w=17 h=13 | x=141 y=362 w=17 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=976 y=393 w=12 h=12 | x=976 y=393 w=12 h=12 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=394 w=53 h=13 | x=0 y=394 w=53 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=50 y=394 w=53 h=13 | x=50 y=394 w=53 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=84 y=394 w=6 h=13 | x=84 y=394 w=6 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=118 y=394 w=31 h=13 | x=118 y=394 w=31 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=103 y=395 w=15 h=12 | x=103 y=395 w=15 h=12 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=975 y=435 w=14 h=16 | x=975 y=435 w=14 h=16 | ink RGB (70, 73, 84) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=438 w=15 h=13 | x=0 y=438 w=15 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=13 y=438 w=15 h=16 | x=13 y=438 w=15 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=7 y=471 w=14 h=16 | x=7 y=471 w=14 h=16 | ink RGB (70, 73, 84) on background RGB (14, 16, 22) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=48 y=472 w=13 h=14 | x=48 y=472 w=13 h=14 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=0 y=510 w=45 h=16 | x=0 y=510 w=45 h=16 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=42 y=511 w=24 h=12 | x=42 y=511 w=24 h=12 | ink RGB (83, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `review-empty-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=54 y=8 w=18 h=48 | x=54 y=8 w=18 h=48 | left edge at 54 px; nearest shared grid column 17 px (delta 37 px) |
| A1 | accept-as-legacy | x=413 y=480 w=169 h=12 | x=413 y=480 w=169 h=12 | left edge at 413 px; nearest shared grid column 446 px (delta 33 px) |
| A1 | accept-as-legacy | x=434 y=8 w=56 h=48 | x=434 y=8 w=56 h=48 | left edge at 434 px; nearest shared grid column 446 px (delta 12 px) |
| A1 | accept-as-legacy | x=439 y=480 w=143 h=12 | x=439 y=480 w=143 h=12 | left edge at 439 px; nearest shared grid column 446 px (delta 7 px) |
| A1 | accept-as-legacy | x=466 y=453 w=19 h=16 | x=466 y=453 w=19 h=16 | left edge at 466 px; nearest shared grid column 447 px (delta 19 px) |
| A1 | accept-as-legacy | x=536 y=453 w=24 h=16 | x=536 y=453 w=24 h=16 | left edge at 536 px; nearest shared grid column 545 px (delta 9 px) |

### `review-ledger-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=207 y=821 w=24 h=17 | x=207 y=821 w=24 h=17 | left edge at 207 px; nearest shared grid column 202 px (delta 5 px) |
| A1 | accept-as-legacy | x=261 y=821 w=14 h=17 | x=261 y=821 w=14 h=17 | left edge at 261 px; nearest shared grid column 241 px (delta 20 px) |
| A1 | accept-as-legacy | x=334 y=821 w=29 h=17 | x=334 y=821 w=29 h=17 | left edge at 334 px; nearest shared grid column 345 px (delta 11 px) |
| A1 | accept-as-legacy | x=434 y=8 w=56 h=48 | x=434 y=8 w=56 h=48 | left edge at 434 px; nearest shared grid column 451 px (delta 17 px) |
| A1 | fix-later | x=457 y=821 w=121 h=17 | x=457 y=821 w=121 h=17 | left edge at 457 px; nearest shared grid column 451 px (delta 6 px) |
| A1 | accept-as-legacy | x=741 y=821 w=39 h=17 | x=741 y=821 w=39 h=17 | left edge at 741 px; nearest shared grid column 732 px (delta 9 px) |
| A1 | accept-as-legacy | x=923 y=420 w=29 h=48 | x=923 y=420 w=29 h=48 | left edge at 923 px; nearest shared grid column 956 px (delta 33 px) |
| A2 | fix-later | x=0 y=136 w=1000 h=16 | x=0 y=136 w=1000 h=16 | gap 16 px vs section median 8 px (delta 8 px) |
| A2 | fix-later | x=0 y=208 w=1000 h=15 | x=0 y=208 w=1000 h=15 | gap 15 px vs section median 8 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=336 w=1000 h=19 | x=0 y=336 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=468 w=1000 h=19 | x=0 y=468 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=563 w=1000 h=27 | x=0 y=563 w=1000 h=27 | gap 27 px vs section median 8 px (delta 19 px) |
| A2 | accept-as-legacy | x=0 y=619 w=1000 h=19 | x=0 y=619 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=714 w=1000 h=27 | x=0 y=714 w=1000 h=27 | gap 27 px vs section median 8 px (delta 19 px) |
| A2 | accept-as-legacy | x=0 y=770 w=1000 h=19 | x=0 y=770 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | x=0 y=803 w=1000 h=18 | x=0 y=803 w=1000 h=18 | gap 18 px vs section median 8 px (delta 10 px) |
| A2 | fix-later | x=0 y=838 w=1000 h=1 | x=0 y=838 w=1000 h=1 | gap 1 px vs section median 8 px (delta 7 px) |
| A2 | fix-later | x=0 y=852 w=1000 h=1 | x=0 y=852 w=1000 h=1 | gap 1 px vs section median 8 px (delta 7 px) |
| A2 | fix-later | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 8 px (delta 4 px) |
| T1 | accept-as-legacy | x=706 y=131 w=3 h=5 | x=706 y=131 w=3 h=5 | text line height 5 px (1 component) → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=805 y=131 w=3 h=5 | x=805 y=131 w=3 h=5 | text line height 5 px (1 component) → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=706 y=203 w=3 h=5 | x=706 y=203 w=3 h=5 | text line height 5 px (1 component) → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=805 y=203 w=3 h=5 | x=805 y=203 w=3 h=5 | text line height 5 px (1 component) → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=37 y=223 w=51 h=17 | x=37 y=223 w=51 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | x=37 y=355 w=51 h=17 | x=37 y=355 w=51 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | x=923 y=440 w=29 h=28 | x=923 y=440 w=29 h=28 | text line height 28 px (1 component) → em ≈ 40.0 logical px; nearest ladder rung 30 px |
| T1 | fix-later | x=956 y=741 w=28 h=29 | x=956 y=741 w=28 h=29 | text line height 29 px (1 component) → em ≈ 41.4 logical px; nearest ladder rung 30 px |
| T1 | accept-as-legacy | x=301 y=830 w=3 h=5 | x=301 y=830 w=3 h=5 | text line height 5 px (1 component) → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=28 y=101 w=6 h=13 | x=28 y=101 w=6 h=13 | ink RGB (107, 111, 125) on background RGB (20, 22, 28) → contrast 3.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=28 y=173 w=6 h=13 | x=28 y=173 w=6 h=13 | ink RGB (107, 111, 125) on background RGB (20, 22, 28) → contrast 3.6:1; role class Body; scrim-blend ink |

### `review-walk-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | fix-later | x=41 y=350 w=17 h=152 | x=41 y=350 w=17 h=152 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 64 px (delta 10 px) |
| A1 | accept-as-legacy | x=220 y=350 w=560 h=152 | x=220 y=350 w=560 h=152 | left edge at 220 px; nearest shared grid column 358 px (delta 138 px) |
| A1 | accept-as-legacy | x=227 y=231 w=66 h=12 | x=227 y=231 w=66 h=12 | left edge at 227 px; nearest shared grid column 358 px (delta 131 px) |
| A1 | accept-as-legacy | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 358 px (delta 14 px) |
| A1 | accept-as-legacy | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 359 px (delta 7 px) |
| A1 | accept-as-legacy | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 359 px (delta 12 px) |
| A1 | accept-as-legacy | x=388 y=231 w=600 h=12 | x=388 y=231 w=600 h=12 | left edge at 388 px; nearest shared grid column 359 px (delta 29 px) |
| A1 | fix-later | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | x=445 y=24 w=38 h=19 | x=445 y=24 w=38 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | x=529 y=267 w=23 h=16 | x=529 y=267 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 561 px (delta 173 px) |
| A1 | accept-as-legacy | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 561 px (delta 179 px) |
| A1 | accept-as-legacy | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 986 px (delta 77 px) |
| A1 | accept-as-legacy | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 986 px (delta 72 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 11 px (delta 19 px) |
| A2 | fix-later | x=0 y=96 w=1000 h=16 | x=0 y=96 w=1000 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | fix-later | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 11 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 11 px (delta 12 px) |
| A2 | accept-as-legacy | x=0 y=177 w=1000 h=24 | x=0 y=177 w=1000 h=24 | gap 24 px vs section median 11 px (delta 13 px) |
| A2 | fix-later | x=0 y=208 w=1000 h=4 | x=0 y=208 w=1000 h=4 | gap 4 px vs section median 11 px (delta 7 px) |
| A2 | fix-later | x=0 y=224 w=1000 h=7 | x=0 y=224 w=1000 h=7 | gap 7 px vs section median 11 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=243 w=1000 h=1 | x=0 y=243 w=1000 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | x=0 y=251 w=1000 h=16 | x=0 y=251 w=1000 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=303 w=1000 h=1 | x=0 y=303 w=1000 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | x=0 y=306 w=1000 h=16 | x=0 y=306 w=1000 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | x=0 y=331 w=1000 h=19 | x=0 y=331 w=1000 h=19 | gap 19 px vs section median 11 px (delta 8 px) |
| T1 | accept-as-legacy | x=545 y=8 w=4 h=3 | x=545 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=53 w=4 h=3 | x=545 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=443 y=24 w=34 h=15 | x=443 y=24 w=34 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=498 y=24 w=20 h=16 | x=498 y=24 w=20 h=16 | ink RGB (117, 119, 125) on background RGB (23, 26, 35) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=473 y=25 w=12 h=14 | x=473 y=25 w=12 h=14 | ink RGB (87, 90, 102) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=923 y=82 w=14 h=16 | x=923 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=84 w=12 h=12 | x=964 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=342 y=121 w=42 h=16 | x=342 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=393 y=121 w=59 h=16 | x=393 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=714 y=121 w=47 h=16 | x=714 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=758 y=121 w=6 h=15 | x=758 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=885 y=121 w=42 h=15 | x=885 y=121 w=42 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=339 y=124 w=6 h=12 | x=339 y=124 w=6 h=12 | ink RGB (66, 69, 80) on background RGB (23, 26, 35) → contrast 1.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=165 w=12 h=12 | x=964 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=19 y=210 w=46 h=16 | x=19 y=210 w=46 h=16 | ink RGB (114, 116, 122) on background RGB (19, 21, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=62 y=210 w=22 h=13 | x=62 y=210 w=22 h=13 | ink RGB (114, 116, 122) on background RGB (19, 21, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=229 w=53 h=13 | x=18 y=229 w=53 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=70 y=229 w=20 h=13 | x=70 y=229 w=20 h=13 | ink RGB (81, 84, 96) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=90 y=229 w=33 h=13 | x=90 y=229 w=33 h=13 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=125 y=229 w=13 h=13 | x=125 y=229 w=13 h=13 | ink RGB (72, 75, 86) on background RGB (19, 21, 28) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=140 y=229 w=63 h=16 | x=140 y=229 w=63 h=16 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=218 y=229 w=25 h=13 | x=218 y=229 w=25 h=13 | ink RGB (82, 85, 97) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=323 y=229 w=66 h=16 | x=323 y=229 w=66 h=16 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=240 y=230 w=62 h=12 | x=240 y=230 w=62 h=12 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=299 y=230 w=27 h=12 | x=299 y=230 w=27 h=12 | ink RGB (80, 84, 95) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=386 y=230 w=20 h=15 | x=386 y=230 w=20 h=15 | ink RGB (80, 84, 95) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=265 w=24 h=16 | x=428 y=265 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=449 y=265 w=42 h=20 | x=449 y=265 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=493 y=265 w=32 h=16 | x=493 y=265 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=265 w=42 h=20 | x=531 y=265 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=525 y=266 w=10 h=15 | x=525 y=266 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=337 y=292 w=58 h=16 | x=337 y=292 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=395 y=292 w=36 h=16 | x=395 y=292 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=292 w=15 h=13 | x=428 y=292 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=442 y=292 w=64 h=16 | x=442 y=292 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=292 w=28 h=13 | x=503 y=292 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=547 y=292 w=30 h=13 | x=547 y=292 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=574 y=292 w=30 h=13 | x=574 y=292 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=604 y=292 w=21 h=13 | x=604 y=292 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=626 y=292 w=13 h=13 | x=626 y=292 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=636 y=292 w=24 h=13 | x=636 y=292 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=359 y=293 w=8 h=12 | x=359 y=293 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=320 w=29 h=13 | x=10 y=320 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=320 w=23 h=13 | x=36 y=320 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=724 y=320 w=26 h=13 | x=724 y=320 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=754 y=320 w=18 h=13 | x=754 y=320 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=347 w=18 h=15 | x=31 y=347 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=347 w=8 h=15 | x=52 y=347 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=348 w=16 h=14 | x=18 y=348 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=934 y=349 w=25 h=13 | x=934 y=349 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=349 w=17 h=13 | x=964 y=349 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `review-walk-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 20 px (delta 4 px) |
| A1 | accept-as-legacy | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 23 px (delta 10 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=407 y=226 w=23 h=16 | x=407 y=226 w=23 h=16 | left edge at 407 px; nearest shared grid column 383 px (delta 24 px) |
| A1 | fix-later | x=511 y=123 w=16 h=12 | x=511 y=123 w=16 h=12 | left edge at 511 px; nearest shared grid column 508 px (delta 3 px) |
| A1 | accept-as-legacy | x=529 y=123 w=570 h=12 | x=529 y=123 w=570 h=12 | left edge at 529 px; nearest shared grid column 508 px (delta 21 px) |
| A1 | accept-as-legacy | x=538 y=123 w=561 h=12 | x=538 y=123 w=561 h=12 | left edge at 538 px; nearest shared grid column 508 px (delta 30 px) |
| A1 | accept-as-legacy | x=695 y=24 w=38 h=19 | x=695 y=24 w=38 h=19 | left edge at 695 px; nearest shared grid column 686 px (delta 9 px) |
| A1 | accept-as-legacy | x=721 y=24 w=12 h=19 | x=721 y=24 w=12 h=19 | left edge at 721 px; nearest shared grid column 754 px (delta 33 px) |
| A1 | accept-as-legacy | x=811 y=8 w=629 h=15 | x=811 y=8 w=629 h=15 | left edge at 811 px; nearest shared grid column 757 px (delta 54 px) |
| A1 | accept-as-legacy | x=971 y=296 w=66 h=12 | x=971 y=296 w=66 h=12 | left edge at 971 px; nearest shared grid column 757 px (delta 214 px) |
| A1 | accept-as-legacy | x=1102 y=296 w=386 h=12 | x=1102 y=296 w=386 h=12 | left edge at 1102 px; nearest shared grid column 757 px (delta 345 px) |
| A1 | accept-as-legacy | x=1109 y=123 w=28 h=12 | x=1109 y=123 w=28 h=12 | left edge at 1109 px; nearest shared grid column 757 px (delta 352 px) |
| A1 | accept-as-legacy | x=1115 y=123 w=22 h=12 | x=1115 y=123 w=22 h=12 | left edge at 1115 px; nearest shared grid column 757 px (delta 358 px) |
| A1 | accept-as-legacy | x=1132 y=296 w=356 h=12 | x=1132 y=296 w=356 h=12 | left edge at 1132 px; nearest shared grid column 1486 px (delta 354 px) |
| A1 | accept-as-legacy | x=1367 y=123 w=121 h=12 | x=1367 y=123 w=121 h=12 | left edge at 1367 px; nearest shared grid column 1486 px (delta 119 px) |
| A2 | fix-later | x=0 y=23 w=1500 h=1 | x=0 y=23 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1500 h=31 | x=0 y=56 w=1500 h=31 | gap 31 px vs section median 9 px (delta 22 px) |
| A2 | fix-later | x=0 y=96 w=1500 h=16 | x=0 y=96 w=1500 h=16 | gap 16 px vs section median 9 px (delta 7 px) |
| A2 | fix-later | x=0 y=145 w=1500 h=16 | x=0 y=145 w=1500 h=16 | gap 16 px vs section median 9 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=170 w=1500 h=19 | x=0 y=170 w=1500 h=19 | gap 19 px vs section median 9 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=199 w=1500 h=27 | x=0 y=199 w=1500 h=27 | gap 27 px vs section median 9 px (delta 18 px) |
| A2 | fix-later | x=0 y=262 w=1500 h=1 | x=0 y=262 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| A2 | fix-later | x=0 y=264 w=1500 h=2 | x=0 y=264 w=1500 h=2 | gap 2 px vs section median 9 px (delta 7 px) |
| A2 | fix-later | x=0 y=271 w=1500 h=6 | x=0 y=271 w=1500 h=6 | gap 6 px vs section median 9 px (delta 3 px) |
| A2 | fix-later | x=0 y=286 w=1500 h=1 | x=0 y=286 w=1500 h=1 | gap 1 px vs section median 9 px (delta 8 px) |
| A2 | fix-later | x=0 y=308 w=1500 h=3 | x=0 y=308 w=1500 h=3 | gap 3 px vs section median 9 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=316 w=1500 h=42 | x=0 y=316 w=1500 h=42 | gap 42 px vs section median 9 px (delta 33 px) |
| T1 | accept-as-legacy | x=795 y=8 w=4 h=3 | x=795 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=795 y=53 w=4 h=3 | x=795 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=693 y=24 w=34 h=15 | x=693 y=24 w=34 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=748 y=24 w=20 h=16 | x=748 y=24 w=20 h=16 | ink RGB (117, 119, 125) on background RGB (23, 26, 35) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=723 y=25 w=12 h=14 | x=723 y=25 w=12 h=14 | ink RGB (87, 90, 102) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1423 y=82 w=14 h=16 | x=1423 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=84 w=12 h=12 | x=1464 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=509 y=121 w=42 h=16 | x=509 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=559 y=121 w=59 h=16 | x=559 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1089 y=121 w=47 h=16 | x=1089 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1133 y=121 w=6 h=15 | x=1133 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1344 y=121 w=41 h=15 | x=1344 y=121 w=41 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=506 y=124 w=6 h=12 | x=506 y=124 w=6 h=12 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=159 w=29 h=13 | x=10 y=159 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=159 w=23 h=13 | x=36 y=159 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1099 y=159 w=26 h=13 | x=1099 y=159 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1129 y=159 w=18 h=13 | x=1129 y=159 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=186 w=18 h=15 | x=31 y=186 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=186 w=8 h=15 | x=52 y=186 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=187 w=16 h=14 | x=18 y=187 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1434 y=188 w=25 h=13 | x=1434 y=188 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=188 w=17 h=13 | x=1464 y=188 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=306 y=224 w=24 h=16 | x=306 y=224 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=327 y=224 w=42 h=20 | x=327 y=224 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=371 y=224 w=32 h=16 | x=371 y=224 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=409 y=224 w=42 h=20 | x=409 y=224 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=403 y=225 w=10 h=15 | x=403 y=225 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=230 w=12 h=12 | x=1464 y=230 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=755 y=231 w=18 h=13 | x=755 y=231 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=770 y=231 w=24 h=13 | x=770 y=231 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=799 y=231 w=11 h=13 | x=799 y=231 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=215 y=251 w=58 h=16 | x=215 y=251 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=273 y=251 w=36 h=16 | x=273 y=251 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=306 y=251 w=15 h=13 | x=306 y=251 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=320 y=251 w=64 h=16 | x=320 y=251 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=381 y=251 w=28 h=13 | x=381 y=251 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=425 y=251 w=30 h=13 | x=425 y=251 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=452 y=251 w=30 h=13 | x=452 y=251 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=482 y=251 w=21 h=13 | x=482 y=251 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=504 y=251 w=13 h=13 | x=504 y=251 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=514 y=251 w=24 h=13 | x=514 y=251 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=237 y=252 w=8 h=12 | x=237 y=252 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=763 y=275 w=46 h=16 | x=763 y=275 w=46 h=16 | ink RGB (114, 116, 122) on background RGB (19, 21, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=806 y=275 w=22 h=13 | x=806 y=275 w=22 h=13 | ink RGB (114, 116, 122) on background RGB (19, 21, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=762 y=294 w=53 h=13 | x=762 y=294 w=53 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=814 y=294 w=20 h=13 | x=814 y=294 w=20 h=13 | ink RGB (81, 84, 96) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=834 y=294 w=33 h=13 | x=834 y=294 w=33 h=13 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=869 y=294 w=13 h=13 | x=869 y=294 w=13 h=13 | ink RGB (72, 75, 86) on background RGB (19, 21, 28) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=884 y=294 w=63 h=16 | x=884 y=294 w=63 h=16 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=962 y=294 w=25 h=13 | x=962 y=294 w=25 h=13 | ink RGB (82, 85, 97) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1067 y=294 w=66 h=16 | x=1067 y=294 w=66 h=16 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=984 y=295 w=62 h=12 | x=984 y=295 w=62 h=12 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1043 y=295 w=27 h=12 | x=1043 y=295 w=27 h=12 | ink RGB (80, 84, 95) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1130 y=295 w=20 h=15 | x=1130 y=295 w=20 h=15 | ink RGB (80, 84, 95) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=555 y=873 w=25 h=13 | x=555 y=873 w=25 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=815 y=873 w=14 h=14 | x=815 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=514 y=874 w=35 h=12 | x=514 y=874 w=35 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `review-walk-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | fix-later | x=41 y=350 w=17 h=152 | x=41 y=350 w=17 h=152 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 64 px (delta 10 px) |
| A1 | accept-as-legacy | x=155 y=294 w=48 h=12 | x=155 y=294 w=48 h=12 | left edge at 155 px; nearest shared grid column 225 px (delta 70 px) |
| A1 | accept-as-legacy | x=211 y=123 w=16 h=12 | x=211 y=123 w=16 h=12 | left edge at 211 px; nearest shared grid column 225 px (delta 14 px) |
| A1 | accept-as-legacy | x=329 y=267 w=23 h=16 | x=329 y=267 w=23 h=16 | left edge at 329 px; nearest shared grid column 305 px (delta 24 px) |
| A1 | accept-as-legacy | x=349 y=294 w=12 h=12 | x=349 y=294 w=12 h=12 | left edge at 349 px; nearest shared grid column 358 px (delta 9 px) |
| A1 | accept-as-legacy | x=388 y=201 w=200 h=50 | x=388 y=201 w=200 h=50 | left edge at 388 px; nearest shared grid column 359 px (delta 29 px) |
| A1 | accept-as-legacy | x=542 y=123 w=46 h=12 | x=542 y=123 w=46 h=12 | left edge at 542 px; nearest shared grid column 586 px (delta 44 px) |
| A2 | accept-as-legacy | x=0 y=11 w=600 h=7 | x=0 y=11 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=19 w=600 h=2 | x=0 y=19 w=600 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=23 w=600 h=1 | x=0 y=23 w=600 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=43 w=600 h=10 | x=0 y=43 w=600 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=56 w=600 h=28 | x=0 y=56 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=115 w=600 h=8 | x=0 y=115 w=600 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=135 w=600 h=7 | x=0 y=135 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=145 w=600 h=23 | x=0 y=145 w=600 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | fix-later | x=0 y=177 w=600 h=24 | x=0 y=177 w=600 h=24 | gap 24 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | x=0 y=283 w=600 h=11 | x=0 y=283 w=600 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | fix-later | x=0 y=331 w=600 h=19 | x=0 y=331 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| T1 | accept-as-legacy | x=536 y=8 w=4 h=3 | x=536 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=536 y=53 w=4 h=3 | x=536 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=434 y=24 w=34 h=15 | x=434 y=24 w=34 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=489 y=24 w=20 h=16 | x=489 y=24 w=20 h=16 | ink RGB (117, 119, 125) on background RGB (23, 26, 35) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=464 y=25 w=12 h=14 | x=464 y=25 w=12 h=14 | ink RGB (87, 90, 102) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=523 y=82 w=14 h=16 | x=523 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=84 w=12 h=12 | x=564 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=209 y=121 w=42 h=16 | x=209 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=259 y=121 w=59 h=16 | x=259 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=414 y=121 w=47 h=16 | x=414 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=458 y=121 w=6 h=15 | x=458 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=519 y=121 w=41 h=15 | x=519 y=121 w=41 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=206 y=124 w=6 h=12 | x=206 y=124 w=6 h=12 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=165 w=12 h=12 | x=564 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=19 y=210 w=46 h=16 | x=19 y=210 w=46 h=16 | ink RGB (114, 116, 122) on background RGB (19, 21, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=62 y=210 w=22 h=13 | x=62 y=210 w=22 h=13 | ink RGB (114, 116, 122) on background RGB (19, 21, 28) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=229 w=53 h=13 | x=18 y=229 w=53 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=70 y=229 w=20 h=13 | x=70 y=229 w=20 h=13 | ink RGB (81, 84, 96) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=90 y=229 w=33 h=13 | x=90 y=229 w=33 h=13 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=125 y=229 w=13 h=13 | x=125 y=229 w=13 h=13 | ink RGB (72, 75, 86) on background RGB (19, 21, 28) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=140 y=229 w=63 h=16 | x=140 y=229 w=63 h=16 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=218 y=229 w=25 h=13 | x=218 y=229 w=25 h=13 | ink RGB (82, 85, 97) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=323 y=229 w=66 h=16 | x=323 y=229 w=66 h=16 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=240 y=230 w=62 h=12 | x=240 y=230 w=62 h=12 | ink RGB (86, 89, 101) on background RGB (19, 21, 28) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=299 y=230 w=27 h=12 | x=299 y=230 w=27 h=12 | ink RGB (80, 84, 95) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=386 y=230 w=20 h=15 | x=386 y=230 w=20 h=15 | ink RGB (80, 84, 95) on background RGB (19, 21, 28) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=228 y=265 w=24 h=16 | x=228 y=265 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=249 y=265 w=42 h=20 | x=249 y=265 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=293 y=265 w=32 h=16 | x=293 y=265 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=331 y=265 w=42 h=20 | x=331 y=265 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=325 y=266 w=10 h=15 | x=325 y=266 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=137 y=292 w=58 h=16 | x=137 y=292 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=195 y=292 w=36 h=16 | x=195 y=292 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=228 y=292 w=15 h=13 | x=228 y=292 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=242 y=292 w=64 h=16 | x=242 y=292 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=303 y=292 w=28 h=13 | x=303 y=292 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=292 w=30 h=13 | x=347 y=292 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=374 y=292 w=30 h=13 | x=374 y=292 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=404 y=292 w=21 h=13 | x=404 y=292 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=426 y=292 w=13 h=13 | x=426 y=292 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=436 y=292 w=24 h=13 | x=436 y=292 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=159 y=293 w=8 h=12 | x=159 y=293 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=320 w=29 h=13 | x=10 y=320 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=320 w=23 h=13 | x=36 y=320 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=424 y=320 w=26 h=13 | x=424 y=320 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=454 y=320 w=18 h=13 | x=454 y=320 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=255 y=873 w=25 h=13 | x=255 y=873 w=25 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=365 y=873 w=14 h=14 | x=365 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=214 y=874 w=35 h=12 | x=214 y=874 w=35 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `search-palette-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=220 y=317 w=18 h=227 | x=220 y=317 w=18 h=227 | left edge at 220 px; nearest shared grid column 57 px (delta 163 px) |
| A1 | accept-as-legacy | x=245 y=317 w=396 h=227 | x=245 y=317 w=396 h=227 | left edge at 245 px; nearest shared grid column 430 px (delta 185 px) |
| A1 | accept-as-legacy | x=265 y=317 w=376 h=227 | x=265 y=317 w=376 h=227 | left edge at 265 px; nearest shared grid column 430 px (delta 165 px) |
| A1 | accept-as-legacy | x=278 y=317 w=363 h=227 | x=278 y=317 w=363 h=227 | left edge at 278 px; nearest shared grid column 430 px (delta 152 px) |
| A1 | accept-as-legacy | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 430 px (delta 86 px) |
| A1 | accept-as-legacy | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 430 px (delta 64 px) |
| A1 | accept-as-legacy | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 430 px (delta 59 px) |
| A1 | fix-later | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | x=445 y=24 w=37 h=19 | x=445 y=24 w=37 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | x=529 y=205 w=23 h=16 | x=529 y=205 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | x=545 y=317 w=96 h=227 | x=545 y=317 w=96 h=227 | left edge at 545 px; nearest shared grid column 505 px (delta 40 px) |
| A1 | accept-as-legacy | x=605 y=317 w=36 h=227 | x=605 y=317 w=36 h=227 | left edge at 605 px; nearest shared grid column 505 px (delta 100 px) |
| A1 | accept-as-legacy | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 760 px (delta 26 px) |
| A1 | accept-as-legacy | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 760 px (delta 20 px) |
| A1 | accept-as-legacy | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 762 px (delta 147 px) |
| A1 | accept-as-legacy | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 762 px (delta 152 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=43 w=1000 h=10 | x=0 y=43 w=1000 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 16 px (delta 14 px) |
| A2 | fix-later | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=177 w=1000 h=28 | x=0 y=177 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=221 w=1000 h=11 | x=0 y=221 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=241 w=1000 h=1 | x=0 y=241 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=269 w=1000 h=19 | x=0 y=269 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | x=0 y=298 w=1000 h=19 | x=0 y=298 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| T1 | accept-as-legacy | x=545 y=8 w=4 h=3 | x=545 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=53 w=4 h=3 | x=545 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=562 y=381 w=3 h=3 | x=562 y=381 w=3 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=579 y=9 w=11 h=12 | x=579 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=443 y=24 w=28 h=15 | x=443 y=24 w=28 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=468 y=24 w=16 h=15 | x=468 y=24 w=16 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=923 y=82 w=14 h=16 | x=923 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=84 w=12 h=12 | x=964 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=342 y=121 w=42 h=16 | x=342 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=393 y=121 w=59 h=16 | x=393 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=714 y=121 w=47 h=16 | x=714 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=758 y=121 w=6 h=15 | x=758 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=885 y=121 w=42 h=15 | x=885 y=121 w=42 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=339 y=124 w=6 h=12 | x=339 y=124 w=6 h=12 | ink RGB (66, 69, 80) on background RGB (23, 26, 35) → contrast 1.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=165 w=12 h=12 | x=964 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=203 w=24 h=16 | x=428 y=203 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=449 y=203 w=42 h=20 | x=449 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=493 y=203 w=32 h=16 | x=493 y=203 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=203 w=42 h=20 | x=531 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=525 y=204 w=10 h=15 | x=525 y=204 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=337 y=230 w=58 h=16 | x=337 y=230 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=395 y=230 w=36 h=16 | x=395 y=230 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=230 w=15 h=13 | x=428 y=230 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=442 y=230 w=64 h=16 | x=442 y=230 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=230 w=28 h=13 | x=503 y=230 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=547 y=230 w=30 h=13 | x=547 y=230 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=574 y=230 w=30 h=13 | x=574 y=230 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=604 y=230 w=21 h=13 | x=604 y=230 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=626 y=230 w=13 h=13 | x=626 y=230 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=636 y=230 w=24 h=13 | x=636 y=230 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=359 y=231 w=8 h=12 | x=359 y=231 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=258 w=29 h=13 | x=10 y=258 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=258 w=23 h=13 | x=36 y=258 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=724 y=258 w=26 h=13 | x=724 y=258 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=754 y=258 w=18 h=13 | x=754 y=258 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=285 w=18 h=15 | x=31 y=285 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=285 w=8 h=15 | x=52 y=285 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=286 w=16 h=14 | x=18 y=286 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=934 y=287 w=25 h=13 | x=934 y=287 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=287 w=17 h=13 | x=964 y=287 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `search-palette-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 20 px (delta 4 px) |
| A1 | accept-as-legacy | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 23 px (delta 10 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=407 y=226 w=23 h=16 | x=407 y=226 w=23 h=16 | left edge at 407 px; nearest shared grid column 383 px (delta 24 px) |
| A1 | accept-as-legacy | x=495 y=317 w=396 h=227 | x=495 y=317 w=396 h=227 | left edge at 495 px; nearest shared grid column 506 px (delta 11 px) |
| A1 | fix-later | x=511 y=123 w=16 h=12 | x=511 y=123 w=16 h=12 | left edge at 511 px; nearest shared grid column 508 px (delta 3 px) |
| A1 | accept-as-legacy | x=538 y=123 w=561 h=12 | x=538 y=123 w=561 h=12 | left edge at 538 px; nearest shared grid column 529 px (delta 9 px) |
| A1 | accept-as-legacy | x=695 y=24 w=37 h=19 | x=695 y=24 w=37 h=19 | left edge at 695 px; nearest shared grid column 529 px (delta 166 px) |
| A1 | accept-as-legacy | x=720 y=24 w=12 h=19 | x=720 y=24 w=12 h=19 | left edge at 720 px; nearest shared grid column 529 px (delta 191 px) |
| A1 | accept-as-legacy | x=795 y=317 w=96 h=227 | x=795 y=317 w=96 h=227 | left edge at 795 px; nearest shared grid column 529 px (delta 266 px) |
| A1 | accept-as-legacy | x=855 y=317 w=36 h=227 | x=855 y=317 w=36 h=227 | left edge at 855 px; nearest shared grid column 529 px (delta 326 px) |
| A1 | accept-as-legacy | x=1012 y=317 w=18 h=227 | x=1012 y=317 w=18 h=227 | left edge at 1012 px; nearest shared grid column 529 px (delta 483 px) |
| A1 | accept-as-legacy | x=1109 y=123 w=28 h=12 | x=1109 y=123 w=28 h=12 | left edge at 1109 px; nearest shared grid column 529 px (delta 580 px) |
| A1 | accept-as-legacy | x=1115 y=123 w=22 h=12 | x=1115 y=123 w=22 h=12 | left edge at 1115 px; nearest shared grid column 529 px (delta 586 px) |
| A1 | accept-as-legacy | x=1367 y=123 w=121 h=12 | x=1367 y=123 w=121 h=12 | left edge at 1367 px; nearest shared grid column 529 px (delta 838 px) |
| A2 | accept-as-legacy | x=0 y=10 w=1500 h=2 | x=0 y=10 w=1500 h=2 | gap 2 px vs section median 11 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=13 w=1500 h=1 | x=0 y=13 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=16 w=1500 h=1 | x=0 y=16 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1500 h=2 | x=0 y=19 w=1500 h=2 | gap 2 px vs section median 11 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1500 h=1 | x=0 y=23 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1500 h=31 | x=0 y=56 w=1500 h=31 | gap 31 px vs section median 11 px (delta 20 px) |
| A2 | fix-later | x=0 y=96 w=1500 h=16 | x=0 y=96 w=1500 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | x=0 y=135 w=1500 h=8 | x=0 y=135 w=1500 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | fix-later | x=0 y=145 w=1500 h=16 | x=0 y=145 w=1500 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | x=0 y=170 w=1500 h=19 | x=0 y=170 w=1500 h=19 | gap 19 px vs section median 11 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=199 w=1500 h=27 | x=0 y=199 w=1500 h=27 | gap 27 px vs section median 11 px (delta 16 px) |
| A2 | accept-as-legacy | x=0 y=262 w=1500 h=1 | x=0 y=262 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=264 w=1500 h=53 | x=0 y=264 w=1500 h=53 | gap 53 px vs section median 11 px (delta 42 px) |
| T1 | accept-as-legacy | x=795 y=8 w=4 h=3 | x=795 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=795 y=53 w=4 h=3 | x=795 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=812 y=381 w=3 h=3 | x=812 y=381 w=3 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=829 y=9 w=11 h=12 | x=829 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=693 y=24 w=28 h=15 | x=693 y=24 w=28 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=718 y=24 w=16 h=15 | x=718 y=24 w=16 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1423 y=82 w=14 h=16 | x=1423 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=84 w=12 h=12 | x=1464 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=509 y=121 w=42 h=16 | x=509 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=559 y=121 w=59 h=16 | x=559 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1089 y=121 w=47 h=16 | x=1089 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1133 y=121 w=6 h=15 | x=1133 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1344 y=121 w=41 h=15 | x=1344 y=121 w=41 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=506 y=124 w=6 h=12 | x=506 y=124 w=6 h=12 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=159 w=29 h=13 | x=10 y=159 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=159 w=23 h=13 | x=36 y=159 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1099 y=159 w=26 h=13 | x=1099 y=159 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1129 y=159 w=18 h=13 | x=1129 y=159 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=186 w=18 h=15 | x=31 y=186 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=186 w=8 h=15 | x=52 y=186 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=187 w=16 h=14 | x=18 y=187 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1434 y=188 w=25 h=13 | x=1434 y=188 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=188 w=17 h=13 | x=1464 y=188 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=306 y=224 w=24 h=16 | x=306 y=224 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=327 y=224 w=42 h=20 | x=327 y=224 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=371 y=224 w=32 h=16 | x=371 y=224 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=409 y=224 w=42 h=20 | x=409 y=224 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=403 y=225 w=10 h=15 | x=403 y=225 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1464 y=230 w=12 h=12 | x=1464 y=230 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=755 y=231 w=18 h=13 | x=755 y=231 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=770 y=231 w=24 h=13 | x=770 y=231 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=799 y=231 w=11 h=13 | x=799 y=231 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=215 y=251 w=58 h=16 | x=215 y=251 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=273 y=251 w=36 h=16 | x=273 y=251 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=306 y=251 w=15 h=13 | x=306 y=251 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=320 y=251 w=64 h=16 | x=320 y=251 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=381 y=251 w=28 h=13 | x=381 y=251 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=425 y=251 w=30 h=13 | x=425 y=251 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=452 y=251 w=30 h=13 | x=452 y=251 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=482 y=251 w=21 h=13 | x=482 y=251 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=504 y=251 w=13 h=13 | x=504 y=251 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=514 y=251 w=24 h=13 | x=514 y=251 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=237 y=252 w=8 h=12 | x=237 y=252 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=555 y=873 w=25 h=13 | x=555 y=873 w=25 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=815 y=873 w=14 h=14 | x=815 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=514 y=874 w=35 h=12 | x=514 y=874 w=35 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `search-palette-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | x=45 y=317 w=396 h=227 | x=45 y=317 w=396 h=227 | left edge at 45 px; nearest shared grid column 38 px (delta 7 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 67 px (delta 7 px) |
| A1 | accept-as-legacy | x=78 y=317 w=363 h=227 | x=78 y=317 w=363 h=227 | left edge at 78 px; nearest shared grid column 67 px (delta 11 px) |
| A1 | accept-as-legacy | x=155 y=232 w=48 h=12 | x=155 y=232 w=48 h=12 | left edge at 155 px; nearest shared grid column 229 px (delta 74 px) |
| A1 | accept-as-legacy | x=211 y=123 w=16 h=12 | x=211 y=123 w=16 h=12 | left edge at 211 px; nearest shared grid column 229 px (delta 18 px) |
| A1 | accept-as-legacy | x=329 y=205 w=23 h=16 | x=329 y=205 w=23 h=16 | left edge at 329 px; nearest shared grid column 305 px (delta 24 px) |
| A1 | accept-as-legacy | x=345 y=317 w=96 h=227 | x=345 y=317 w=96 h=227 | left edge at 345 px; nearest shared grid column 305 px (delta 40 px) |
| A1 | accept-as-legacy | x=349 y=232 w=12 h=12 | x=349 y=232 w=12 h=12 | left edge at 349 px; nearest shared grid column 305 px (delta 44 px) |
| A1 | accept-as-legacy | x=405 y=317 w=36 h=227 | x=405 y=317 w=36 h=227 | left edge at 405 px; nearest shared grid column 427 px (delta 22 px) |
| A1 | accept-as-legacy | x=542 y=123 w=46 h=12 | x=542 y=123 w=46 h=12 | left edge at 542 px; nearest shared grid column 462 px (delta 80 px) |
| A1 | accept-as-legacy | x=562 y=317 w=18 h=227 | x=562 y=317 w=18 h=227 | left edge at 562 px; nearest shared grid column 462 px (delta 100 px) |
| A2 | accept-as-legacy | x=0 y=11 w=600 h=7 | x=0 y=11 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=19 w=600 h=2 | x=0 y=19 w=600 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=23 w=600 h=1 | x=0 y=23 w=600 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=43 w=600 h=10 | x=0 y=43 w=600 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=56 w=600 h=28 | x=0 y=56 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=115 w=600 h=8 | x=0 y=115 w=600 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=135 w=600 h=7 | x=0 y=135 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=145 w=600 h=23 | x=0 y=145 w=600 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=177 w=600 h=28 | x=0 y=177 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=221 w=600 h=11 | x=0 y=221 w=600 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | fix-later | x=0 y=269 w=600 h=19 | x=0 y=269 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | x=0 y=298 w=600 h=19 | x=0 y=298 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| T1 | accept-as-legacy | x=536 y=8 w=4 h=3 | x=536 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=536 y=53 w=4 h=3 | x=536 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=362 y=381 w=3 h=3 | x=362 y=381 w=3 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=434 y=24 w=34 h=15 | x=434 y=24 w=34 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=489 y=24 w=20 h=16 | x=489 y=24 w=20 h=16 | ink RGB (117, 119, 125) on background RGB (23, 26, 35) → contrast 3.9:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=464 y=25 w=12 h=14 | x=464 y=25 w=12 h=14 | ink RGB (87, 90, 102) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=523 y=82 w=14 h=16 | x=523 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=84 w=12 h=12 | x=564 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=209 y=121 w=42 h=16 | x=209 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=259 y=121 w=59 h=16 | x=259 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=414 y=121 w=47 h=16 | x=414 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=458 y=121 w=6 h=15 | x=458 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=519 y=121 w=41 h=15 | x=519 y=121 w=41 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=206 y=124 w=6 h=12 | x=206 y=124 w=6 h=12 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=165 w=12 h=12 | x=564 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=228 y=203 w=24 h=16 | x=228 y=203 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=249 y=203 w=42 h=20 | x=249 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=293 y=203 w=32 h=16 | x=293 y=203 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=331 y=203 w=42 h=20 | x=331 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=325 y=204 w=10 h=15 | x=325 y=204 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=137 y=230 w=58 h=16 | x=137 y=230 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=195 y=230 w=36 h=16 | x=195 y=230 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=228 y=230 w=15 h=13 | x=228 y=230 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=242 y=230 w=64 h=16 | x=242 y=230 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=303 y=230 w=28 h=13 | x=303 y=230 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=230 w=30 h=13 | x=347 y=230 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=374 y=230 w=30 h=13 | x=374 y=230 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=404 y=230 w=21 h=13 | x=404 y=230 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=426 y=230 w=13 h=13 | x=426 y=230 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=436 y=230 w=24 h=13 | x=436 y=230 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=159 y=231 w=8 h=12 | x=159 y=231 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=258 w=29 h=13 | x=10 y=258 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=258 w=23 h=13 | x=36 y=258 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=424 y=258 w=26 h=13 | x=424 y=258 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=454 y=258 w=18 h=13 | x=454 y=258 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=285 w=18 h=15 | x=31 y=285 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=285 w=8 h=15 | x=52 y=285 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=286 w=16 h=14 | x=18 y=286 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=534 y=287 w=25 h=13 | x=534 y=287 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=564 y=287 w=17 h=13 | x=564 y=287 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=255 y=873 w=25 h=13 | x=255 y=873 w=25 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=365 y=873 w=14 h=14 | x=365 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=214 y=874 w=35 h=12 | x=214 y=874 w=35 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `search-palette-empty-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | x=220 y=375 w=560 h=111 | x=220 y=375 w=560 h=111 | left edge at 220 px; nearest shared grid column 57 px (delta 163 px) |
| A1 | accept-as-legacy | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 430 px (delta 86 px) |
| A1 | accept-as-legacy | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 430 px (delta 64 px) |
| A1 | accept-as-legacy | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 430 px (delta 59 px) |
| A1 | fix-later | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | x=445 y=24 w=37 h=19 | x=445 y=24 w=37 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | x=529 y=205 w=23 h=16 | x=529 y=205 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 505 px (delta 229 px) |
| A1 | accept-as-legacy | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 505 px (delta 235 px) |
| A1 | accept-as-legacy | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 505 px (delta 404 px) |
| A1 | accept-as-legacy | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 505 px (delta 409 px) |
| A2 | accept-as-legacy | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=43 w=1000 h=10 | x=0 y=43 w=1000 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 16 px (delta 14 px) |
| A2 | fix-later | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=177 w=1000 h=28 | x=0 y=177 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=221 w=1000 h=11 | x=0 y=221 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=241 w=1000 h=1 | x=0 y=241 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | x=0 y=269 w=1000 h=19 | x=0 y=269 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | x=0 y=298 w=1000 h=77 | x=0 y=298 w=1000 h=77 | gap 77 px vs section median 16 px (delta 61 px) |
| T1 | accept-as-legacy | x=545 y=8 w=4 h=3 | x=545 y=8 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=53 w=4 h=3 | x=545 y=53 w=4 h=3 | text line height 3 px (1 component) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=579 y=9 w=11 h=12 | x=579 y=9 w=11 h=12 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=443 y=24 w=28 h=15 | x=443 y=24 w=28 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=468 y=24 w=16 h=15 | x=468 y=24 w=16 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=923 y=82 w=14 h=16 | x=923 y=82 w=14 h=16 | ink RGB (95, 97, 103) on background RGB (14, 16, 22) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=84 w=12 h=12 | x=964 y=84 w=12 h=12 | ink RGB (93, 95, 101) on background RGB (14, 16, 22) → contrast 3.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=85 w=43 h=13 | x=10 y=85 w=43 h=13 | ink RGB (109, 111, 116) on background RGB (14, 16, 22) → contrast 3.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=58 y=85 w=11 h=13 | x=58 y=85 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=121 w=42 h=16 | x=18 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=61 y=121 w=14 h=16 | x=61 y=121 w=14 h=16 | ink RGB (86, 89, 101) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=72 y=121 w=20 h=16 | x=72 y=121 w=20 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=89 y=121 w=6 h=15 | x=89 y=121 w=6 h=15 | ink RGB (82, 86, 97) on background RGB (23, 26, 35) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=342 y=121 w=42 h=16 | x=342 y=121 w=42 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=393 y=121 w=59 h=16 | x=393 y=121 w=59 h=16 | ink RGB (87, 91, 103) on background RGB (23, 26, 35) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=714 y=121 w=47 h=16 | x=714 y=121 w=47 h=16 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=758 y=121 w=6 h=15 | x=758 y=121 w=6 h=15 | ink RGB (88, 90, 97) on background RGB (23, 26, 35) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=885 y=121 w=42 h=15 | x=885 y=121 w=42 h=15 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=339 y=124 w=6 h=12 | x=339 y=124 w=6 h=12 | ink RGB (66, 69, 80) on background RGB (23, 26, 35) → contrast 1.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=165 w=12 h=12 | x=964 y=165 w=12 h=12 | ink RGB (48, 89, 64) on background RGB (14, 16, 22) → contrast 2.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=11 y=166 w=18 h=13 | x=11 y=166 w=18 h=13 | ink RGB (115, 117, 123) on background RGB (14, 16, 22) → contrast 4.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=26 y=166 w=24 h=13 | x=26 y=166 w=24 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=166 w=11 h=13 | x=55 y=166 w=11 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=203 w=24 h=16 | x=428 y=203 w=24 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=449 y=203 w=42 h=20 | x=449 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=493 y=203 w=32 h=16 | x=493 y=203 w=32 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=531 y=203 w=42 h=20 | x=531 y=203 w=42 h=20 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=525 y=204 w=10 h=15 | x=525 y=204 w=10 h=15 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=337 y=230 w=58 h=16 | x=337 y=230 w=58 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=395 y=230 w=36 h=16 | x=395 y=230 w=36 h=16 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=428 y=230 w=15 h=13 | x=428 y=230 w=15 h=13 | ink RGB (83, 86, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=442 y=230 w=64 h=16 | x=442 y=230 w=64 h=16 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=503 y=230 w=28 h=13 | x=503 y=230 w=28 h=13 | ink RGB (81, 84, 96) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=547 y=230 w=30 h=13 | x=547 y=230 w=30 h=13 | ink RGB (80, 83, 95) on background RGB (14, 16, 22) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=574 y=230 w=30 h=13 | x=574 y=230 w=30 h=13 | ink RGB (81, 85, 96) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=604 y=230 w=21 h=13 | x=604 y=230 w=21 h=13 | ink RGB (84, 87, 99) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=626 y=230 w=13 h=13 | x=626 y=230 w=13 h=13 | ink RGB (75, 78, 89) on background RGB (14, 16, 22) → contrast 2.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=636 y=230 w=24 h=13 | x=636 y=230 w=24 h=13 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=359 y=231 w=8 h=12 | x=359 y=231 w=8 h=12 | ink RGB (82, 85, 97) on background RGB (14, 16, 22) → contrast 2.6:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=258 w=29 h=13 | x=10 y=258 w=29 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=36 y=258 w=23 h=13 | x=36 y=258 w=23 h=13 | ink RGB (112, 114, 120) on background RGB (14, 16, 22) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=724 y=258 w=26 h=13 | x=724 y=258 w=26 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=754 y=258 w=18 h=13 | x=754 y=258 w=18 h=13 | ink RGB (119, 121, 127) on background RGB (14, 16, 22) → contrast 4.4:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=31 y=285 w=18 h=15 | x=31 y=285 w=18 h=15 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=52 y=285 w=8 h=15 | x=52 y=285 w=8 h=15 | ink RGB (117, 119, 125) on background RGB (14, 16, 22) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=18 y=286 w=16 h=14 | x=18 y=286 w=16 h=14 | ink RGB (118, 120, 126) on background RGB (14, 16, 22) → contrast 4.3:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=934 y=287 w=25 h=13 | x=934 y=287 w=25 h=13 | ink RGB (86, 89, 101) on background RGB (14, 16, 22) → contrast 2.7:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=964 y=287 w=17 h=13 | x=964 y=287 w=17 h=13 | ink RGB (87, 91, 103) on background RGB (14, 16, 22) → contrast 2.8:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=14 y=862 w=36 h=36 | x=14 y=862 w=36 h=36 | ink RGB (87, 91, 103) on background RGB (19, 21, 28) → contrast 2.7:1; role class Large; scrim-blend ink |
| Q1 | accept-as-legacy | x=55 y=873 w=23 h=13 | x=55 y=873 w=23 h=13 | ink RGB (83, 86, 98) on background RGB (19, 21, 28) → contrast 2.5:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=565 y=873 w=14 h=14 | x=565 y=873 w=14 h=14 | ink RGB (120, 122, 128) on background RGB (19, 21, 28) → contrast 4.2:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=347 y=874 w=35 h=13 | x=347 y=874 w=35 h=13 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=388 y=874 w=25 h=12 | x=388 y=874 w=25 h=12 | ink RGB (120, 122, 128) on background RGB (23, 26, 35) → contrast 4.0:1; role class Body; scrim-blend ink |

### `settings-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=82 y=87 w=18 h=21 | x=82 y=87 w=18 h=21 | left edge at 82 px; nearest shared grid column 71 px (delta 11 px) |
| A1 | accept-as-legacy | x=165 y=435 w=14 h=25 | x=165 y=435 w=14 h=25 | left edge at 165 px; nearest shared grid column 140 px (delta 25 px) |
| A1 | accept-as-legacy | x=181 y=496 w=176 h=33 | x=181 y=496 w=176 h=33 | left edge at 181 px; nearest shared grid column 140 px (delta 41 px) |
| A1 | accept-as-legacy | x=211 y=820 w=23 h=24 | x=211 y=820 w=23 h=24 | left edge at 211 px; nearest shared grid column 222 px (delta 11 px) |
| A1 | fix-later | x=238 y=496 w=119 h=33 | x=238 y=496 w=119 h=33 | left edge at 238 px; nearest shared grid column 233 px (delta 5 px) |
| A1 | accept-as-legacy | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 456 px (delta 111 px) |
| A1 | accept-as-legacy | x=366 y=496 w=19 h=33 | x=366 y=496 w=19 h=33 | left edge at 366 px; nearest shared grid column 456 px (delta 90 px) |
| A1 | accept-as-legacy | x=427 y=496 w=31 h=33 | x=427 y=496 w=31 h=33 | left edge at 427 px; nearest shared grid column 456 px (delta 29 px) |
| A1 | accept-as-legacy | x=433 y=8 w=38 h=48 | x=433 y=8 w=38 h=48 | left edge at 433 px; nearest shared grid column 456 px (delta 23 px) |
| A1 | accept-as-legacy | x=440 y=679 w=45 h=25 | x=440 y=679 w=45 h=25 | left edge at 440 px; nearest shared grid column 456 px (delta 16 px) |
| A1 | accept-as-legacy | x=529 y=496 w=12 h=33 | x=529 y=496 w=12 h=33 | left edge at 529 px; nearest shared grid column 517 px (delta 12 px) |
| A1 | accept-as-legacy | x=549 y=496 w=427 h=33 | x=549 y=496 w=427 h=33 | left edge at 549 px; nearest shared grid column 517 px (delta 32 px) |
| A1 | accept-as-legacy | x=581 y=618 w=37 h=33 | x=581 y=618 w=37 h=33 | left edge at 581 px; nearest shared grid column 517 px (delta 64 px) |
| A1 | accept-as-legacy | x=585 y=557 w=154 h=33 | x=585 y=557 w=154 h=33 | left edge at 585 px; nearest shared grid column 517 px (delta 68 px) |
| A1 | accept-as-legacy | x=606 y=618 w=12 h=33 | x=606 y=618 w=12 h=33 | left edge at 606 px; nearest shared grid column 517 px (delta 89 px) |
| A1 | accept-as-legacy | x=627 y=618 w=118 h=33 | x=627 y=618 w=118 h=33 | left edge at 627 px; nearest shared grid column 517 px (delta 110 px) |
| A1 | accept-as-legacy | x=713 y=172 w=28 h=202 | x=713 y=172 w=28 h=202 | left edge at 713 px; nearest shared grid column 771 px (delta 58 px) |
| A1 | accept-as-legacy | x=725 y=172 w=16 h=202 | x=725 y=172 w=16 h=202 | left edge at 725 px; nearest shared grid column 771 px (delta 46 px) |
| A1 | accept-as-legacy | x=762 y=172 w=221 h=202 | x=762 y=172 w=221 h=202 | left edge at 762 px; nearest shared grid column 771 px (delta 9 px) |
| A1 | accept-as-legacy | x=830 y=557 w=30 h=33 | x=830 y=557 w=30 h=33 | left edge at 830 px; nearest shared grid column 858 px (delta 28 px) |
| A1 | accept-as-legacy | x=868 y=618 w=31 h=33 | x=868 y=618 w=31 h=33 | left edge at 868 px; nearest shared grid column 860 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 23 px (delta 15 px) |
| A2 | fix-later | x=0 y=71 w=1000 h=16 | x=0 y=71 w=1000 h=16 | gap 16 px vs section median 23 px (delta 7 px) |
| A2 | fix-later | x=0 y=108 w=1000 h=16 | x=0 y=108 w=1000 h=16 | gap 16 px vs section median 23 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=132 w=1000 h=3 | x=0 y=132 w=1000 h=3 | gap 3 px vs section median 23 px (delta 20 px) |
| A2 | accept-as-legacy | x=0 y=460 w=1000 h=36 | x=0 y=460 w=1000 h=36 | gap 36 px vs section median 23 px (delta 13 px) |
| A2 | fix-later | x=0 y=529 w=1000 h=28 | x=0 y=529 w=1000 h=28 | gap 28 px vs section median 23 px (delta 5 px) |
| A2 | fix-later | x=0 y=590 w=1000 h=28 | x=0 y=590 w=1000 h=28 | gap 28 px vs section median 23 px (delta 5 px) |
| A2 | fix-later | x=0 y=651 w=1000 h=28 | x=0 y=651 w=1000 h=28 | gap 28 px vs section median 23 px (delta 5 px) |
| A2 | fix-later | x=0 y=704 w=1000 h=15 | x=0 y=704 w=1000 h=15 | gap 15 px vs section median 23 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=729 w=1000 h=41 | x=0 y=729 w=1000 h=41 | gap 41 px vs section median 23 px (delta 18 px) |
| A2 | accept-as-legacy | x=0 y=782 w=1000 h=38 | x=0 y=782 w=1000 h=38 | gap 38 px vs section median 23 px (delta 15 px) |
| A2 | accept-as-legacy | x=0 y=844 w=1000 h=9 | x=0 y=844 w=1000 h=9 | gap 9 px vs section median 23 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 23 px (delta 19 px) |
| T1 | accept-as-legacy | x=96 y=87 w=4 h=4 | x=96 y=87 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=28 y=91 w=4 h=6 | x=28 y=91 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=102 y=91 w=35 h=17 | x=102 y=91 w=35 h=17 | text line height 17 px (3 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=100 y=172 w=4 h=4 | x=100 y=172 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=702 y=215 w=32 h=16 | x=702 y=215 w=32 h=16 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=731 y=215 w=48 h=15 | x=731 y=215 w=48 h=15 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=748 y=216 w=6 h=14 | x=748 y=216 w=6 h=14 | ink RGB (76, 82, 102) on background RGB (29, 32, 40) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=168 y=505 w=22 h=15 | x=168 y=505 w=22 h=15 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=118 y=690 w=12 h=12 | x=118 y=690 w=12 h=12 | ink RGB (70, 89, 132) on background RGB (29, 32, 40) → contrast 2.3:1; role class Body; scrim-blend ink |

### `settings-1.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=82 y=88 w=18 h=20 | x=82 y=88 w=18 h=20 | left edge at 82 px; nearest shared grid column 71 px (delta 11 px) |
| A1 | accept-as-legacy | x=165 y=435 w=14 h=25 | x=165 y=435 w=14 h=25 | left edge at 165 px; nearest shared grid column 140 px (delta 25 px) |
| A1 | accept-as-legacy | x=211 y=820 w=23 h=24 | x=211 y=820 w=23 h=24 | left edge at 211 px; nearest shared grid column 222 px (delta 11 px) |
| A1 | accept-as-legacy | x=321 y=496 w=203 h=33 | x=321 y=496 w=203 h=33 | left edge at 321 px; nearest shared grid column 233 px (delta 88 px) |
| A1 | accept-as-legacy | x=512 y=864 w=71 h=32 | x=512 y=864 w=71 h=32 | left edge at 512 px; nearest shared grid column 708 px (delta 196 px) |
| A1 | accept-as-legacy | x=533 y=496 w=19 h=33 | x=533 y=496 w=19 h=33 | left edge at 533 px; nearest shared grid column 708 px (delta 175 px) |
| A1 | accept-as-legacy | x=593 y=496 w=81 h=33 | x=593 y=496 w=81 h=33 | left edge at 593 px; nearest shared grid column 708 px (delta 115 px) |
| A1 | accept-as-legacy | x=659 y=496 w=15 h=33 | x=659 y=496 w=15 h=33 | left edge at 659 px; nearest shared grid column 708 px (delta 49 px) |
| A1 | accept-as-legacy | x=683 y=8 w=38 h=48 | x=683 y=8 w=38 h=48 | left edge at 683 px; nearest shared grid column 708 px (delta 25 px) |
| A1 | accept-as-legacy | x=690 y=679 w=45 h=25 | x=690 y=679 w=45 h=25 | left edge at 690 px; nearest shared grid column 708 px (delta 18 px) |
| A1 | accept-as-legacy | x=696 y=496 w=12 h=33 | x=696 y=496 w=12 h=33 | left edge at 696 px; nearest shared grid column 708 px (delta 12 px) |
| A1 | accept-as-legacy | x=716 y=496 w=760 h=33 | x=716 y=496 w=760 h=33 | left edge at 716 px; nearest shared grid column 709 px (delta 7 px) |
| A1 | accept-as-legacy | x=730 y=8 w=57 h=48 | x=730 y=8 w=57 h=48 | left edge at 730 px; nearest shared grid column 749 px (delta 19 px) |
| A1 | accept-as-legacy | x=1081 y=618 w=37 h=33 | x=1081 y=618 w=37 h=33 | left edge at 1081 px; nearest shared grid column 1125 px (delta 44 px) |
| A1 | accept-as-legacy | x=1085 y=557 w=154 h=33 | x=1085 y=557 w=154 h=33 | left edge at 1085 px; nearest shared grid column 1125 px (delta 40 px) |
| A1 | accept-as-legacy | x=1088 y=173 w=28 h=201 | x=1088 y=173 w=28 h=201 | left edge at 1088 px; nearest shared grid column 1125 px (delta 37 px) |
| A1 | accept-as-legacy | x=1100 y=173 w=16 h=201 | x=1100 y=173 w=16 h=201 | left edge at 1100 px; nearest shared grid column 1125 px (delta 25 px) |
| A1 | accept-as-legacy | x=1106 y=618 w=12 h=33 | x=1106 y=618 w=12 h=33 | left edge at 1106 px; nearest shared grid column 1125 px (delta 19 px) |
| A1 | accept-as-legacy | x=1137 y=173 w=346 h=201 | x=1137 y=173 w=346 h=201 | left edge at 1137 px; nearest shared grid column 1127 px (delta 10 px) |
| A1 | accept-as-legacy | x=1330 y=557 w=30 h=33 | x=1330 y=557 w=30 h=33 | left edge at 1330 px; nearest shared grid column 1358 px (delta 28 px) |
| A1 | accept-as-legacy | x=1368 y=618 w=31 h=33 | x=1368 y=618 w=31 h=33 | left edge at 1368 px; nearest shared grid column 1360 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1500 h=8 | x=0 y=56 w=1500 h=8 | gap 8 px vs section median 24 px (delta 16 px) |
| A2 | fix-later | x=0 y=69 w=1500 h=19 | x=0 y=69 w=1500 h=19 | gap 19 px vs section median 24 px (delta 5 px) |
| A2 | fix-later | x=0 y=108 w=1500 h=16 | x=0 y=108 w=1500 h=16 | gap 16 px vs section median 24 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=132 w=1500 h=3 | x=0 y=132 w=1500 h=3 | gap 3 px vs section median 24 px (delta 21 px) |
| A2 | accept-as-legacy | x=0 y=460 w=1500 h=36 | x=0 y=460 w=1500 h=36 | gap 36 px vs section median 24 px (delta 12 px) |
| A2 | fix-later | x=0 y=529 w=1500 h=28 | x=0 y=529 w=1500 h=28 | gap 28 px vs section median 24 px (delta 4 px) |
| A2 | fix-later | x=0 y=590 w=1500 h=28 | x=0 y=590 w=1500 h=28 | gap 28 px vs section median 24 px (delta 4 px) |
| A2 | fix-later | x=0 y=651 w=1500 h=28 | x=0 y=651 w=1500 h=28 | gap 28 px vs section median 24 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=704 w=1500 h=15 | x=0 y=704 w=1500 h=15 | gap 15 px vs section median 24 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=729 w=1500 h=41 | x=0 y=729 w=1500 h=41 | gap 41 px vs section median 24 px (delta 17 px) |
| A2 | accept-as-legacy | x=0 y=782 w=1500 h=38 | x=0 y=782 w=1500 h=38 | gap 38 px vs section median 24 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=844 w=1500 h=11 | x=0 y=844 w=1500 h=11 | gap 11 px vs section median 24 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=860 w=1500 h=4 | x=0 y=860 w=1500 h=4 | gap 4 px vs section median 24 px (delta 20 px) |
| T1 | accept-as-legacy | x=96 y=87 w=4 h=4 | x=96 y=87 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=28 y=91 w=4 h=6 | x=28 y=91 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=102 y=91 w=35 h=17 | x=102 y=91 w=35 h=17 | text line height 17 px (3 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=100 y=172 w=4 h=4 | x=100 y=172 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=1077 y=215 w=32 h=16 | x=1077 y=215 w=32 h=16 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1106 y=215 w=48 h=15 | x=1106 y=215 w=48 h=15 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=1123 y=216 w=6 h=14 | x=1123 y=216 w=6 h=14 | ink RGB (76, 82, 102) on background RGB (29, 32, 40) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=210 y=505 w=21 h=15 | x=210 y=505 w=21 h=15 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=118 y=690 w=12 h=12 | x=118 y=690 w=12 h=12 | ink RGB (70, 89, 132) on background RGB (29, 32, 40) → contrast 2.3:1; role class Body; scrim-blend ink |

### `settings-1.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=299 y=64 w=14 h=796 | x=299 y=64 w=14 h=796 | left edge at 299 px; nearest shared grid column 254 px (delta 45 px) |
| A1 | fix-later | x=442 y=64 w=33 h=796 | x=442 y=64 w=33 h=796 | left edge at 442 px; nearest shared grid column 439 px (delta 3 px) |
| A1 | accept-as-legacy | x=450 y=64 w=25 h=796 | x=450 y=64 w=25 h=796 | left edge at 450 px; nearest shared grid column 439 px (delta 11 px) |
| A1 | accept-as-legacy | x=458 y=64 w=17 h=796 | x=458 y=64 w=17 h=796 | left edge at 458 px; nearest shared grid column 439 px (delta 19 px) |
| A1 | accept-as-legacy | x=462 y=8 w=12 h=48 | x=462 y=8 w=12 h=48 | left edge at 462 px; nearest shared grid column 439 px (delta 23 px) |
| A1 | accept-as-legacy | x=483 y=8 w=57 h=48 | x=483 y=8 w=57 h=48 | left edge at 483 px; nearest shared grid column 501 px (delta 18 px) |
| A1 | accept-as-legacy | x=532 y=64 w=14 h=796 | x=532 y=64 w=14 h=796 | left edge at 532 px; nearest shared grid column 503 px (delta 29 px) |
| T1 | accept-as-legacy | x=96 y=87 w=4 h=4 | x=96 y=87 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=28 y=91 w=4 h=6 | x=28 y=91 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=102 y=91 w=35 h=17 | x=102 y=91 w=35 h=17 | text line height 17 px (3 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=100 y=172 w=4 h=4 | x=100 y=172 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=402 y=215 w=32 h=16 | x=402 y=215 w=32 h=16 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=431 y=215 w=48 h=15 | x=431 y=215 w=48 h=15 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=448 y=216 w=6 h=14 | x=448 y=216 w=6 h=14 | ink RGB (76, 82, 102) on background RGB (29, 32, 40) → contrast 2.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=135 y=505 w=21 h=15 | x=135 y=505 w=21 h=15 | ink RGB (100, 108, 133) on background RGB (29, 32, 40) → contrast 3.1:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=118 y=690 w=12 h=12 | x=118 y=690 w=12 h=12 | ink RGB (70, 89, 132) on background RGB (29, 32, 40) → contrast 2.3:1; role class Body; scrim-blend ink |

### `task-view-0.5-500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=40 y=36 w=6 h=10 | x=80 y=72 w=12 h=20 | left edge at 40 px; nearest shared grid column 29 px (delta 11 px) |
| A1 | accept-as-legacy | x=43 y=223 w=21 h=6 | x=86 y=446 w=42 h=12 | left edge at 43 px; nearest shared grid column 29 px (delta 14 px) |
| A1 | accept-as-legacy | x=48 y=223 w=16 h=6 | x=96 y=446 w=32 h=12 | left edge at 48 px; nearest shared grid column 29 px (delta 19 px) |
| A1 | accept-as-legacy | x=53 y=223 w=11 h=6 | x=106 y=446 w=22 h=12 | left edge at 53 px; nearest shared grid column 72 px (delta 19 px) |
| A1 | accept-as-legacy | x=65 y=36 w=10 h=10 | x=130 y=72 w=20 h=20 | left edge at 65 px; nearest shared grid column 72 px (delta 7 px) |
| A1 | fix-later | x=120 y=201 w=870 h=17 | x=240 y=402 w=1740 h=34 | left edge at 120 px; nearest shared grid column 115 px (delta 5 px) |
| A1 | accept-as-legacy | x=173 y=36 w=11 h=10 | x=346 y=72 w=22 h=20 | left edge at 173 px; nearest shared grid column 115 px (delta 58 px) |
| A1 | accept-as-legacy | x=223 y=36 w=7 h=10 | x=446 y=72 w=14 h=20 | left edge at 223 px; nearest shared grid column 246 px (delta 23 px) |
| A1 | accept-as-legacy | x=257 y=36 w=14 h=10 | x=514 y=72 w=28 h=20 | left edge at 257 px; nearest shared grid column 246 px (delta 11 px) |
| A1 | accept-as-legacy | x=306 y=36 w=8 h=10 | x=612 y=72 w=16 h=20 | left edge at 306 px; nearest shared grid column 338 px (delta 32 px) |
| A1 | accept-as-legacy | x=320 y=36 w=11 h=10 | x=640 y=72 w=22 h=20 | left edge at 320 px; nearest shared grid column 338 px (delta 18 px) |
| A1 | accept-as-legacy | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 511 px (delta 49 px) |
| A1 | accept-as-legacy | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 511 px (delta 15 px) |
| A1 | accept-as-legacy | x=501 y=55 w=13 h=6 | x=1002 y=110 w=26 h=12 | left edge at 501 px; nearest shared grid column 511 px (delta 10 px) |
| A1 | accept-as-legacy | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 511 px (delta 8 px) |
| A1 | accept-as-legacy | x=534 y=482 w=6 h=16 | x=1068 y=964 w=12 h=32 | left edge at 534 px; nearest shared grid column 513 px (delta 21 px) |
| A1 | accept-as-legacy | x=759 y=64 w=17 h=6 | x=1518 y=128 w=34 h=12 | left edge at 759 px; nearest shared grid column 988 px (delta 229 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 988 px (delta 8 px) |
| A2 | fix-later | x=0 y=34 w=1000 h=2 | x=0 y=68 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | x=0 y=46 w=1000 h=9 | x=0 y=92 w=2000 h=18 | gap 9 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=61 w=1000 h=3 | x=0 y=122 w=2000 h=6 | gap 3 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=70 w=1000 h=9 | x=0 y=140 w=2000 h=18 | gap 9 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=84 w=1000 h=1 | x=0 y=168 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | x=0 y=121 w=1000 h=11 | x=0 y=242 w=2000 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=137 w=1000 h=18 | x=0 y=274 w=2000 h=36 | gap 18 px vs section median 6 px (delta 12 px) |
| A2 | accept-as-legacy | x=0 y=157 w=1000 h=15 | x=0 y=314 w=2000 h=30 | gap 15 px vs section median 6 px (delta 9 px) |
| A2 | fix-later | x=0 y=181 w=1000 h=13 | x=0 y=362 w=2000 h=26 | gap 13 px vs section median 6 px (delta 7 px) |
| A2 | fix-later | x=0 y=199 w=1000 h=2 | x=0 y=398 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | x=0 y=229 w=1000 h=12 | x=0 y=458 w=2000 h=24 | gap 12 px vs section median 6 px (delta 6 px) |
| A2 | fix-later | x=0 y=247 w=1000 h=1 | x=0 y=494 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | x=0 y=480 w=1000 h=2 | x=0 y=960 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| T1 | fix-later | x=8 y=482 w=16 h=16 | x=16 y=964 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `task-view-0.5-900`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=40 y=36 w=6 h=10 | x=80 y=72 w=12 h=20 | left edge at 40 px; nearest shared grid column 29 px (delta 11 px) |
| A1 | accept-as-legacy | x=43 y=223 w=21 h=6 | x=86 y=446 w=42 h=12 | left edge at 43 px; nearest shared grid column 29 px (delta 14 px) |
| A1 | accept-as-legacy | x=48 y=223 w=16 h=6 | x=96 y=446 w=32 h=12 | left edge at 48 px; nearest shared grid column 29 px (delta 19 px) |
| A1 | accept-as-legacy | x=53 y=223 w=11 h=6 | x=106 y=446 w=22 h=12 | left edge at 53 px; nearest shared grid column 72 px (delta 19 px) |
| A1 | accept-as-legacy | x=65 y=36 w=10 h=10 | x=130 y=72 w=20 h=20 | left edge at 65 px; nearest shared grid column 72 px (delta 7 px) |
| A1 | fix-later | x=120 y=201 w=870 h=17 | x=240 y=402 w=1740 h=34 | left edge at 120 px; nearest shared grid column 115 px (delta 5 px) |
| A1 | accept-as-legacy | x=173 y=36 w=11 h=10 | x=346 y=72 w=22 h=20 | left edge at 173 px; nearest shared grid column 115 px (delta 58 px) |
| A1 | accept-as-legacy | x=223 y=36 w=7 h=10 | x=446 y=72 w=14 h=20 | left edge at 223 px; nearest shared grid column 246 px (delta 23 px) |
| A1 | accept-as-legacy | x=257 y=36 w=14 h=10 | x=514 y=72 w=28 h=20 | left edge at 257 px; nearest shared grid column 246 px (delta 11 px) |
| A1 | accept-as-legacy | x=306 y=36 w=8 h=10 | x=612 y=72 w=16 h=20 | left edge at 306 px; nearest shared grid column 338 px (delta 32 px) |
| A1 | accept-as-legacy | x=320 y=36 w=11 h=10 | x=640 y=72 w=22 h=20 | left edge at 320 px; nearest shared grid column 338 px (delta 18 px) |
| A1 | accept-as-legacy | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 511 px (delta 49 px) |
| A1 | accept-as-legacy | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 511 px (delta 15 px) |
| A1 | accept-as-legacy | x=501 y=55 w=13 h=6 | x=1002 y=110 w=26 h=12 | left edge at 501 px; nearest shared grid column 511 px (delta 10 px) |
| A1 | accept-as-legacy | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 511 px (delta 8 px) |
| A1 | accept-as-legacy | x=534 y=882 w=6 h=16 | x=1068 y=1764 w=12 h=32 | left edge at 534 px; nearest shared grid column 513 px (delta 21 px) |
| A1 | accept-as-legacy | x=759 y=64 w=17 h=6 | x=1518 y=128 w=34 h=12 | left edge at 759 px; nearest shared grid column 988 px (delta 229 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 988 px (delta 8 px) |
| A2 | fix-later | x=0 y=34 w=1000 h=2 | x=0 y=68 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | x=0 y=46 w=1000 h=9 | x=0 y=92 w=2000 h=18 | gap 9 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=61 w=1000 h=3 | x=0 y=122 w=2000 h=6 | gap 3 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=70 w=1000 h=9 | x=0 y=140 w=2000 h=18 | gap 9 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=84 w=1000 h=1 | x=0 y=168 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | x=0 y=121 w=1000 h=11 | x=0 y=242 w=2000 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=137 w=1000 h=18 | x=0 y=274 w=2000 h=36 | gap 18 px vs section median 6 px (delta 12 px) |
| A2 | accept-as-legacy | x=0 y=157 w=1000 h=15 | x=0 y=314 w=2000 h=30 | gap 15 px vs section median 6 px (delta 9 px) |
| A2 | fix-later | x=0 y=181 w=1000 h=13 | x=0 y=362 w=2000 h=26 | gap 13 px vs section median 6 px (delta 7 px) |
| A2 | fix-later | x=0 y=199 w=1000 h=2 | x=0 y=398 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | x=0 y=229 w=1000 h=12 | x=0 y=458 w=2000 h=24 | gap 12 px vs section median 6 px (delta 6 px) |
| A2 | fix-later | x=0 y=247 w=1000 h=1 | x=0 y=494 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | x=0 y=880 w=1000 h=2 | x=0 y=1760 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `task-view-1.5-500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=499 w=3 h=1 | x=0 y=333 w=2 h=1 | content run touches the bottom edge (extent 3 px along the edge) |
| C1 | accept-as-legacy | x=997 y=499 w=3 h=1 | x=665 y=333 w=2 h=1 | content run touches the bottom edge (extent 3 px along the edge) |
| A1 | accept-as-legacy | x=137 y=96 w=49 h=404 | x=91 y=64 w=33 h=269 | left edge at 137 px; nearest shared grid column 67 px (delta 70 px) |
| A1 | accept-as-legacy | x=238 y=96 w=42 h=404 | x=159 y=64 w=28 h=269 | left edge at 238 px; nearest shared grid column 67 px (delta 171 px) |
| A1 | accept-as-legacy | x=246 y=96 w=34 h=404 | x=164 y=64 w=23 h=269 | left edge at 246 px; nearest shared grid column 67 px (delta 179 px) |
| A1 | accept-as-legacy | x=256 y=96 w=24 h=404 | x=171 y=64 w=16 h=269 | left edge at 256 px; nearest shared grid column 67 px (delta 189 px) |
| A1 | accept-as-legacy | x=299 y=96 w=48 h=404 | x=199 y=64 w=32 h=269 | left edge at 299 px; nearest shared grid column 521 px (delta 222 px) |
| A1 | accept-as-legacy | x=387 y=12 w=83 h=72 | x=258 y=8 w=55 h=48 | left edge at 387 px; nearest shared grid column 521 px (delta 134 px) |
| A1 | accept-as-legacy | x=776 y=96 w=19 h=404 | x=517 y=64 w=13 h=269 | left edge at 776 px; nearest shared grid column 522 px (delta 254 px) |
| A1 | accept-as-legacy | x=807 y=96 w=28 h=404 | x=538 y=64 w=19 h=269 | left edge at 807 px; nearest shared grid column 522 px (delta 285 px) |
| T1 | accept-as-legacy | x=342 y=107 w=14 h=6 | x=228 y=71 w=9 h=4 | text line height 6 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=446 y=107 w=6 h=6 | x=297 y=71 w=4 h=4 | text line height 6 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=890 y=126 w=26 h=6 | x=593 y=84 w=17 h=4 | text line height 6 px (3 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=79 y=324 w=3 h=3 | x=53 y=216 w=2 h=2 | text line height 3 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |

### `task-view-1.5-900`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=24 y=846 w=48 h=48 | x=16 y=564 w=32 h=32 | left edge at 24 px; nearest shared grid column 34 px (delta 10 px) |
| A1 | accept-as-legacy | x=181 y=96 w=43 h=744 | x=121 y=64 w=29 h=496 | left edge at 181 px; nearest shared grid column 138 px (delta 43 px) |
| A1 | accept-as-legacy | x=190 y=96 w=34 h=744 | x=127 y=64 w=23 h=496 | left edge at 190 px; nearest shared grid column 138 px (delta 52 px) |
| A1 | accept-as-legacy | x=226 y=96 w=22 h=744 | x=151 y=64 w=15 h=496 | left edge at 226 px; nearest shared grid column 245 px (delta 19 px) |
| A1 | accept-as-legacy | x=387 y=12 w=83 h=72 | x=258 y=8 w=55 h=48 | left edge at 387 px; nearest shared grid column 354 px (delta 33 px) |
| A1 | accept-as-legacy | x=776 y=96 w=19 h=744 | x=517 y=64 w=13 h=496 | left edge at 776 px; nearest shared grid column 539 px (delta 237 px) |
| A1 | accept-as-legacy | x=807 y=96 w=28 h=744 | x=538 y=64 w=19 h=496 | left edge at 807 px; nearest shared grid column 539 px (delta 268 px) |
| T1 | accept-as-legacy | x=342 y=107 w=14 h=6 | x=228 y=71 w=9 h=4 | text line height 6 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=446 y=107 w=6 h=6 | x=297 y=71 w=4 h=4 | text line height 6 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=890 y=126 w=26 h=6 | x=593 y=84 w=17 h=4 | text line height 6 px (3 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=79 y=324 w=3 h=3 | x=53 y=216 w=2 h=2 | text line height 3 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=38 y=575 w=3 h=3 | x=25 y=383 w=2 h=2 | text line height 3 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=988 y=600 w=4 h=4 | x=659 y=400 w=3 h=3 | text line height 4 px (1 component) → em ≈ 3.8 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=63 y=661 w=3 h=3 | x=42 y=441 w=2 h=2 | text line height 3 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=51 y=713 w=3 h=3 | x=34 y=475 w=2 h=2 | text line height 3 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=72 y=713 w=3 h=3 | x=48 y=475 w=2 h=2 | text line height 3 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=24 y=846 w=48 h=48 | x=16 y=564 w=32 h=32 | text line height 48 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `task-view-3.0-500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=499 w=6 h=1 | x=0 y=166 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) |
| C1 | accept-as-legacy | x=994 y=499 w=6 h=1 | x=331 y=166 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) |
| A1 | accept-as-legacy | x=44 y=192 w=38 h=308 | x=15 y=64 w=13 h=103 | left edge at 44 px; nearest shared grid column 73 px (delta 29 px) |
| A1 | accept-as-legacy | x=227 y=192 w=44 h=308 | x=76 y=64 w=15 h=103 | left edge at 227 px; nearest shared grid column 78 px (delta 149 px) |
| A1 | accept-as-legacy | x=274 y=24 w=166 h=144 | x=91 y=8 w=55 h=48 | left edge at 274 px; nearest shared grid column 78 px (delta 196 px) |
| T1 | accept-as-legacy | x=884 y=54 w=28 h=3 | x=295 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=64 w=28 h=4 | x=295 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=75 w=28 h=3 | x=295 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=685 y=216 w=26 h=9 | x=228 y=72 w=9 h=3 | text line height 9 px (2 components) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=755 y=252 w=51 h=11 | x=252 y=84 w=17 h=4 | text line height 11 px (3 components) → em ≈ 5.2 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=336 y=393 w=22 h=4 | x=112 y=131 w=7 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=624 y=393 w=22 h=4 | x=208 y=131 w=7 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |

### `task-view-3.0-900`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | accept-as-legacy | x=0 y=899 w=6 h=1 | x=0 y=300 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) |
| C1 | accept-as-legacy | x=994 y=899 w=6 h=1 | x=331 y=300 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) |
| A1 | accept-as-legacy | x=26 y=192 w=56 h=708 | x=9 y=64 w=19 h=236 | left edge at 26 px; nearest shared grid column 37 px (delta 11 px) |
| A1 | accept-as-legacy | x=363 y=192 w=85 h=708 | x=121 y=64 w=28 h=236 | left edge at 363 px; nearest shared grid column 274 px (delta 89 px) |
| T1 | accept-as-legacy | x=884 y=54 w=28 h=3 | x=295 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=64 w=28 h=4 | x=295 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=75 w=28 h=3 | x=295 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=685 y=216 w=26 h=9 | x=228 y=72 w=9 h=3 | text line height 9 px (2 components) → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=755 y=252 w=51 h=11 | x=252 y=84 w=17 h=4 | text line height 11 px (3 components) → em ≈ 5.2 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=336 y=393 w=22 h=4 | x=112 y=131 w=7 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=624 y=393 w=22 h=4 | x=208 y=131 w=7 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=158 y=646 w=5 h=4 | x=53 y=215 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |

### `task-view-focus-cell-1.0-900`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | fix-now | x=999 y=171 w=1 h=17 | x=999 y=171 w=1 h=17 | content run touches the right edge (extent 17 px along the edge) |
| C1 | fix-now | x=999 y=208 w=1 h=25 | x=999 y=208 w=1 h=25 | content run touches the right edge (extent 25 px along the edge) |
| C1 | accept-as-legacy | x=999 y=399 w=1 h=6 | x=999 y=399 w=1 h=6 | content run touches the right edge (extent 6 px along the edge) |
| C1 | fix-now | x=0 y=171 w=1 h=17 | x=0 y=171 w=1 h=17 | content run touches the left edge (extent 17 px along the edge) |
| C1 | fix-now | x=0 y=397 w=1 h=31 | x=0 y=397 w=1 h=31 | content run touches the left edge (extent 31 px along the edge) |
| A1 | fix-later | x=4 y=156 w=498 h=36 | x=4 y=156 w=498 h=36 | left edge at 4 px; nearest shared grid column 1 px (delta 3 px) |
| A1 | accept-as-legacy | x=150 y=396 w=15 h=33 | x=150 y=396 w=15 h=33 | left edge at 150 px; nearest shared grid column 163 px (delta 13 px) |
| A1 | accept-as-legacy | x=213 y=396 w=15 h=33 | x=213 y=396 w=15 h=33 | left edge at 213 px; nearest shared grid column 226 px (delta 13 px) |
| A1 | accept-as-legacy | x=364 y=71 w=60 h=20 | x=364 y=71 w=60 h=20 | left edge at 364 px; nearest shared grid column 347 px (delta 17 px) |
| A1 | accept-as-legacy | x=411 y=71 w=13 h=20 | x=411 y=71 w=13 h=20 | left edge at 411 px; nearest shared grid column 347 px (delta 64 px) |
| A1 | accept-as-legacy | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 486 px (delta 62 px) |
| A1 | accept-as-legacy | x=538 y=71 w=19 h=20 | x=538 y=71 w=19 h=20 | left edge at 538 px; nearest shared grid column 515 px (delta 23 px) |
| A2 | fix-later | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 15 px (delta 5 px) |
| A2 | accept-as-legacy | x=0 y=120 w=1000 h=3 | x=0 y=120 w=1000 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | fix-later | x=0 y=192 w=1000 h=12 | x=0 y=192 w=1000 h=12 | gap 12 px vs section median 15 px (delta 3 px) |
| A2 | fix-later | x=0 y=237 w=1000 h=23 | x=0 y=237 w=1000 h=23 | gap 23 px vs section median 15 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=269 w=1000 h=35 | x=0 y=269 w=1000 h=35 | gap 35 px vs section median 15 px (delta 20 px) |
| A2 | accept-as-legacy | x=0 y=305 w=1000 h=2 | x=0 y=305 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=309 w=1000 h=2 | x=0 y=309 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=312 w=1000 h=28 | x=0 y=312 w=1000 h=28 | gap 28 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=357 w=1000 h=27 | x=0 y=357 w=1000 h=27 | gap 27 px vs section median 15 px (delta 12 px) |
| A2 | accept-as-legacy | x=0 y=393 w=1000 h=3 | x=0 y=393 w=1000 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | fix-later | x=0 y=429 w=1000 h=11 | x=0 y=429 w=1000 h=11 | gap 11 px vs section median 15 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=449 w=1000 h=27 | x=0 y=449 w=1000 h=27 | gap 27 px vs section median 15 px (delta 12 px) |
| A2 | accept-as-legacy | x=0 y=485 w=1000 h=3 | x=0 y=485 w=1000 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | accept-as-legacy | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 15 px (delta 11 px) |
| T1 | accept-as-legacy | x=228 y=71 w=10 h=4 | x=228 y=71 w=10 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=4 h=4 | x=298 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=4 h=4 | x=658 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=164 y=75 w=23 h=17 | x=164 y=75 w=23 h=17 | text line height 17 px (2 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=32 y=474 w=6 h=13 | x=32 y=474 w=6 h=13 | ink RGB (107, 111, 125) on background RGB (20, 22, 28) → contrast 3.6:1; role class Body; scrim-blend ink |

### `task-view-overdue-banner-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=4 y=155 w=498 h=37 | x=4 y=155 w=498 h=37 | left edge at 4 px; nearest shared grid column 14 px (delta 10 px) |
| A1 | accept-as-legacy | x=81 y=796 w=16 h=16 | x=81 y=796 w=16 h=16 | left edge at 81 px; nearest shared grid column 69 px (delta 12 px) |
| A1 | accept-as-legacy | x=150 y=396 w=15 h=33 | x=150 y=396 w=15 h=33 | left edge at 150 px; nearest shared grid column 163 px (delta 13 px) |
| A1 | accept-as-legacy | x=213 y=396 w=15 h=33 | x=213 y=396 w=15 h=33 | left edge at 213 px; nearest shared grid column 226 px (delta 13 px) |
| A1 | accept-as-legacy | x=217 y=796 w=44 h=16 | x=217 y=796 w=44 h=16 | left edge at 217 px; nearest shared grid column 226 px (delta 9 px) |
| A1 | accept-as-legacy | x=249 y=796 w=12 h=16 | x=249 y=796 w=12 h=16 | left edge at 249 px; nearest shared grid column 236 px (delta 13 px) |
| A1 | accept-as-legacy | x=364 y=64 w=60 h=28 | x=364 y=64 w=60 h=28 | left edge at 364 px; nearest shared grid column 347 px (delta 17 px) |
| A1 | accept-as-legacy | x=380 y=796 w=38 h=16 | x=380 y=796 w=38 h=16 | left edge at 380 px; nearest shared grid column 347 px (delta 33 px) |
| A1 | accept-as-legacy | x=393 y=796 w=25 h=16 | x=393 y=796 w=25 h=16 | left edge at 393 px; nearest shared grid column 428 px (delta 35 px) |
| A1 | accept-as-legacy | x=411 y=64 w=13 h=28 | x=411 y=64 w=13 h=28 | left edge at 411 px; nearest shared grid column 428 px (delta 17 px) |
| A1 | fix-later | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 428 px (delta 4 px) |
| A1 | accept-as-legacy | x=538 y=64 w=19 h=28 | x=538 y=64 w=19 h=28 | left edge at 538 px; nearest shared grid column 515 px (delta 23 px) |
| A2 | accept-as-legacy | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 18 px (delta 10 px) |
| A2 | accept-as-legacy | x=0 y=121 w=1000 h=5 | x=0 y=121 w=1000 h=5 | gap 5 px vs section median 18 px (delta 13 px) |
| A2 | fix-later | x=0 y=140 w=1000 h=15 | x=0 y=140 w=1000 h=15 | gap 15 px vs section median 18 px (delta 3 px) |
| A2 | fix-later | x=0 y=192 w=1000 h=12 | x=0 y=192 w=1000 h=12 | gap 12 px vs section median 18 px (delta 6 px) |
| A2 | fix-later | x=0 y=237 w=1000 h=22 | x=0 y=237 w=1000 h=22 | gap 22 px vs section median 18 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=270 w=1000 h=31 | x=0 y=270 w=1000 h=31 | gap 31 px vs section median 18 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=313 w=1000 h=27 | x=0 y=313 w=1000 h=27 | gap 27 px vs section median 18 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=357 w=1000 h=27 | x=0 y=357 w=1000 h=27 | gap 27 px vs section median 18 px (delta 9 px) |
| A2 | accept-as-legacy | x=0 y=394 w=1000 h=2 | x=0 y=394 w=1000 h=2 | gap 2 px vs section median 18 px (delta 16 px) |
| A2 | fix-later | x=0 y=429 w=1000 h=10 | x=0 y=429 w=1000 h=10 | gap 10 px vs section median 18 px (delta 8 px) |
| A2 | accept-as-legacy | x=0 y=449 w=1000 h=27 | x=0 y=449 w=1000 h=27 | gap 27 px vs section median 18 px (delta 9 px) |
| A2 | fix-later | x=0 y=784 w=1000 h=12 | x=0 y=784 w=1000 h=12 | gap 12 px vs section median 18 px (delta 6 px) |
| T1 | accept-as-legacy | x=228 y=71 w=10 h=4 | x=228 y=71 w=10 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=4 h=4 | x=298 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=4 h=4 | x=658 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=164 y=75 w=23 h=17 | x=164 y=75 w=23 h=17 | text line height 17 px (2 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-0.5-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=12 y=866 w=19 h=7 | x=24 y=1732 w=38 h=14 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | fix-later | x=35 y=848 w=9 h=8 | x=70 y=1696 w=18 h=16 | left edge at 35 px; nearest shared grid column 30 px (delta 5 px) |
| A1 | accept-as-legacy | x=51 y=848 w=15 h=8 | x=102 y=1696 w=30 h=16 | left edge at 51 px; nearest shared grid column 30 px (delta 21 px) |
| A1 | accept-as-legacy | x=68 y=848 w=19 h=8 | x=136 y=1696 w=38 h=16 | left edge at 68 px; nearest shared grid column 30 px (delta 38 px) |
| A1 | accept-as-legacy | x=127 y=848 w=23 h=8 | x=254 y=1696 w=46 h=16 | left edge at 127 px; nearest shared grid column 30 px (delta 97 px) |
| A1 | accept-as-legacy | x=176 y=848 w=24 h=8 | x=352 y=1696 w=48 h=16 | left edge at 176 px; nearest shared grid column 30 px (delta 146 px) |
| A1 | accept-as-legacy | x=210 y=848 w=8 h=8 | x=420 y=1696 w=16 h=16 | left edge at 210 px; nearest shared grid column 30 px (delta 180 px) |
| A1 | accept-as-legacy | x=333 y=56 w=322 h=17 | x=666 y=112 w=644 h=34 | left edge at 333 px; nearest shared grid column 30 px (delta 303 px) |
| A1 | accept-as-legacy | x=339 y=882 w=36 h=16 | x=678 y=1764 w=72 h=32 | left edge at 339 px; nearest shared grid column 30 px (delta 309 px) |
| A1 | accept-as-legacy | x=472 y=4 w=12 h=24 | x=944 y=8 w=24 h=48 | left edge at 472 px; nearest shared grid column 30 px (delta 442 px) |
| A1 | accept-as-legacy | x=485 y=4 w=6 h=24 | x=970 y=8 w=12 h=48 | left edge at 485 px; nearest shared grid column 30 px (delta 455 px) |
| A1 | accept-as-legacy | x=496 y=4 w=29 h=24 | x=992 y=8 w=58 h=48 | left edge at 496 px; nearest shared grid column 960 px (delta 464 px) |
| A1 | accept-as-legacy | x=534 y=882 w=6 h=16 | x=1068 y=1764 w=12 h=32 | left edge at 534 px; nearest shared grid column 960 px (delta 426 px) |
| A1 | accept-as-legacy | x=630 y=866 w=18 h=7 | x=1260 y=1732 w=36 h=14 | left edge at 630 px; nearest shared grid column 960 px (delta 330 px) |
| A1 | accept-as-legacy | x=638 y=866 w=10 h=7 | x=1276 y=1732 w=20 h=14 | left edge at 638 px; nearest shared grid column 960 px (delta 322 px) |
| A1 | accept-as-legacy | x=642 y=866 w=6 h=7 | x=1284 y=1732 w=12 h=14 | left edge at 642 px; nearest shared grid column 960 px (delta 318 px) |
| A1 | accept-as-legacy | x=653 y=866 w=313 h=7 | x=1306 y=1732 w=626 h=14 | left edge at 653 px; nearest shared grid column 960 px (delta 307 px) |
| A1 | accept-as-legacy | x=659 y=56 w=170 h=17 | x=1318 y=112 w=340 h=34 | left edge at 659 px; nearest shared grid column 960 px (delta 301 px) |
| A1 | accept-as-legacy | x=693 y=848 w=18 h=8 | x=1386 y=1696 w=36 h=16 | left edge at 693 px; nearest shared grid column 960 px (delta 267 px) |
| A1 | accept-as-legacy | x=701 y=848 w=10 h=8 | x=1402 y=1696 w=20 h=16 | left edge at 701 px; nearest shared grid column 960 px (delta 259 px) |
| A1 | accept-as-legacy | x=705 y=848 w=6 h=8 | x=1410 y=1696 w=12 h=16 | left edge at 705 px; nearest shared grid column 960 px (delta 255 px) |
| A1 | accept-as-legacy | x=716 y=848 w=250 h=8 | x=1432 y=1696 w=500 h=16 | left edge at 716 px; nearest shared grid column 960 px (delta 244 px) |
| A1 | accept-as-legacy | x=907 y=56 w=87 h=17 | x=1814 y=112 w=174 h=34 | left edge at 907 px; nearest shared grid column 960 px (delta 53 px) |
| A1 | accept-as-legacy | x=975 y=117 w=10 h=6 | x=1950 y=234 w=20 h=12 | left edge at 975 px; nearest shared grid column 960 px (delta 15 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 999 px (delta 19 px) |
| A2 | fix-later | x=0 y=28 w=1000 h=4 | x=0 y=56 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=73 w=1000 h=11 | x=0 y=146 w=2000 h=22 | gap 11 px vs section median 8 px (delta 3 px) |
| A2 | fix-later | x=0 y=89 w=1000 h=14 | x=0 y=178 w=2000 h=28 | gap 14 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | x=0 y=123 w=1000 h=12 | x=0 y=246 w=2000 h=24 | gap 12 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=140 w=1000 h=4 | x=0 y=280 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=170 w=1000 h=4 | x=0 y=340 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=178 w=1000 h=2 | x=0 y=356 w=2000 h=4 | gap 2 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | x=0 y=213 w=1000 h=4 | x=0 y=426 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=842 w=1000 h=6 | x=0 y=1684 w=2000 h=12 | gap 6 px vs section median 9 px (delta 3 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-0.5-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=12 y=867 w=19 h=6 | x=24 y=1734 w=38 h=12 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | accept-as-legacy | x=28 y=882 w=514 h=16 | x=56 y=1764 w=1028 h=32 | left edge at 28 px; nearest shared grid column 10 px (delta 18 px) |
| A1 | accept-as-legacy | x=35 y=849 w=9 h=7 | x=70 y=1698 w=18 h=14 | left edge at 35 px; nearest shared grid column 10 px (delta 25 px) |
| A1 | accept-as-legacy | x=51 y=849 w=15 h=7 | x=102 y=1698 w=30 h=14 | left edge at 51 px; nearest shared grid column 10 px (delta 41 px) |
| A1 | accept-as-legacy | x=68 y=849 w=19 h=7 | x=136 y=1698 w=38 h=14 | left edge at 68 px; nearest shared grid column 10 px (delta 58 px) |
| A1 | accept-as-legacy | x=127 y=849 w=23 h=7 | x=254 y=1698 w=46 h=14 | left edge at 127 px; nearest shared grid column 10 px (delta 117 px) |
| A1 | accept-as-legacy | x=176 y=849 w=24 h=7 | x=352 y=1698 w=48 h=14 | left edge at 176 px; nearest shared grid column 10 px (delta 166 px) |
| A1 | accept-as-legacy | x=210 y=849 w=8 h=7 | x=420 y=1698 w=16 h=14 | left edge at 210 px; nearest shared grid column 10 px (delta 200 px) |
| A1 | accept-as-legacy | x=499 y=56 w=490 h=17 | x=998 y=112 w=980 h=34 | left edge at 499 px; nearest shared grid column 10 px (delta 489 px) |
| A1 | accept-as-legacy | x=506 y=882 w=36 h=16 | x=1012 y=1764 w=72 h=32 | left edge at 506 px; nearest shared grid column 10 px (delta 496 px) |
| A1 | accept-as-legacy | x=722 y=4 w=12 h=24 | x=1444 y=8 w=24 h=48 | left edge at 722 px; nearest shared grid column 10 px (delta 712 px) |
| A1 | accept-as-legacy | x=735 y=4 w=6 h=24 | x=1470 y=8 w=12 h=48 | left edge at 735 px; nearest shared grid column 10 px (delta 725 px) |
| A1 | accept-as-legacy | x=746 y=4 w=29 h=24 | x=1492 y=8 w=58 h=48 | left edge at 746 px; nearest shared grid column 1460 px (delta 714 px) |
| A1 | accept-as-legacy | x=784 y=882 w=6 h=16 | x=1568 y=1764 w=12 h=32 | left edge at 784 px; nearest shared grid column 1460 px (delta 676 px) |
| A1 | accept-as-legacy | x=975 y=867 w=10 h=6 | x=1950 y=1734 w=20 h=12 | left edge at 975 px; nearest shared grid column 1460 px (delta 485 px) |
| A1 | accept-as-legacy | x=993 y=56 w=252 h=17 | x=1986 y=112 w=504 h=34 | left edge at 993 px; nearest shared grid column 1460 px (delta 467 px) |
| A1 | accept-as-legacy | x=1038 y=849 w=10 h=7 | x=2076 y=1698 w=20 h=14 | left edge at 1038 px; nearest shared grid column 1460 px (delta 422 px) |
| A1 | accept-as-legacy | x=1371 y=56 w=8 h=17 | x=2742 y=112 w=16 h=34 | left edge at 1371 px; nearest shared grid column 1460 px (delta 89 px) |
| A1 | accept-as-legacy | x=1480 y=4 w=6 h=24 | x=2960 y=8 w=12 h=48 | left edge at 1480 px; nearest shared grid column 1499 px (delta 19 px) |
| A2 | fix-later | x=0 y=28 w=1500 h=4 | x=0 y=56 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=101 w=1500 h=12 | x=0 y=202 w=3000 h=24 | gap 12 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=148 w=1500 h=4 | x=0 y=296 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=156 w=1500 h=2 | x=0 y=312 w=3000 h=4 | gap 2 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | x=0 y=191 w=1500 h=4 | x=0 y=382 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-0.5-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=12 y=866 w=19 h=7 | x=24 y=1732 w=38 h=14 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | fix-later | x=35 y=848 w=9 h=9 | x=70 y=1696 w=18 h=18 | left edge at 35 px; nearest shared grid column 30 px (delta 5 px) |
| A1 | accept-as-legacy | x=51 y=848 w=15 h=9 | x=102 y=1696 w=30 h=18 | left edge at 51 px; nearest shared grid column 30 px (delta 21 px) |
| A1 | accept-as-legacy | x=68 y=848 w=19 h=9 | x=136 y=1696 w=38 h=18 | left edge at 68 px; nearest shared grid column 30 px (delta 38 px) |
| A1 | accept-as-legacy | x=127 y=848 w=23 h=9 | x=254 y=1696 w=46 h=18 | left edge at 127 px; nearest shared grid column 30 px (delta 97 px) |
| A1 | accept-as-legacy | x=176 y=848 w=24 h=9 | x=352 y=1696 w=48 h=18 | left edge at 176 px; nearest shared grid column 30 px (delta 146 px) |
| A1 | accept-as-legacy | x=199 y=56 w=190 h=17 | x=398 y=112 w=380 h=34 | left edge at 199 px; nearest shared grid column 30 px (delta 169 px) |
| A1 | accept-as-legacy | x=206 y=882 w=36 h=16 | x=412 y=1764 w=72 h=32 | left edge at 206 px; nearest shared grid column 30 px (delta 176 px) |
| A1 | accept-as-legacy | x=210 y=848 w=8 h=9 | x=420 y=1696 w=16 h=18 | left edge at 210 px; nearest shared grid column 30 px (delta 180 px) |
| A1 | accept-as-legacy | x=334 y=882 w=6 h=16 | x=668 y=1764 w=12 h=32 | left edge at 334 px; nearest shared grid column 541 px (delta 207 px) |
| A1 | accept-as-legacy | x=375 y=866 w=10 h=7 | x=750 y=1732 w=20 h=14 | left edge at 375 px; nearest shared grid column 541 px (delta 166 px) |
| A1 | accept-as-legacy | x=393 y=56 w=102 h=17 | x=786 y=112 w=204 h=34 | left edge at 393 px; nearest shared grid column 541 px (delta 148 px) |
| A1 | accept-as-legacy | x=438 y=848 w=10 h=9 | x=876 y=1696 w=20 h=18 | left edge at 438 px; nearest shared grid column 541 px (delta 103 px) |
| A1 | accept-as-legacy | x=517 y=4 w=20 h=24 | x=1034 y=8 w=40 h=48 | left edge at 517 px; nearest shared grid column 541 px (delta 24 px) |
| A1 | fix-later | x=546 y=56 w=8 h=17 | x=1092 y=112 w=16 h=34 | left edge at 546 px; nearest shared grid column 542 px (delta 4 px) |
| A1 | accept-as-legacy | x=575 y=117 w=10 h=7 | x=1150 y=234 w=20 h=14 | left edge at 575 px; nearest shared grid column 560 px (delta 15 px) |
| A1 | accept-as-legacy | x=580 y=4 w=6 h=24 | x=1160 y=8 w=12 h=48 | left edge at 580 px; nearest shared grid column 599 px (delta 19 px) |
| A2 | fix-later | x=0 y=73 w=600 h=11 | x=0 y=146 w=1200 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | x=0 y=89 w=600 h=14 | x=0 y=178 w=1200 h=28 | gap 14 px vs section median 6 px (delta 8 px) |
| A2 | fix-later | x=0 y=124 w=600 h=11 | x=0 y=248 w=1200 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | x=0 y=178 w=600 h=2 | x=0 y=356 w=1200 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | x=0 y=213 w=600 h=3 | x=0 y=426 w=1200 h=6 | gap 3 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | x=0 y=842 w=600 h=6 | x=0 y=1684 w=1200 h=12 | gap 6 px vs section median 9 px (delta 3 px) |
| T1 | fix-later | x=8 y=882 w=16 h=16 | x=16 y=1764 w=32 h=32 | text line height 16 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-3.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | fix-now | x=999 y=209 w=1 h=133 | x=333 y=70 w=0 h=44 | content run touches the right edge (extent 133 px along the edge) |
| C1 | fix-now | x=999 y=429 w=1 h=106 | x=333 y=143 w=0 h=35 | content run touches the right edge (extent 106 px along the edge) |
| A1 | accept-as-legacy | x=85 y=192 w=239 h=360 | x=28 y=64 w=80 h=120 | left edge at 85 px; nearest shared grid column 39 px (delta 46 px) |
| A1 | accept-as-legacy | x=337 y=24 w=111 h=144 | x=112 y=8 w=37 h=48 | left edge at 337 px; nearest shared grid column 39 px (delta 298 px) |
| A1 | accept-as-legacy | x=348 y=192 w=288 h=360 | x=116 y=64 w=96 h=120 | left edge at 348 px; nearest shared grid column 39 px (delta 309 px) |
| A1 | accept-as-legacy | x=368 y=792 w=211 h=96 | x=123 y=264 w=70 h=32 | left edge at 368 px; nearest shared grid column 39 px (delta 329 px) |
| A1 | accept-as-legacy | x=475 y=24 w=170 h=144 | x=158 y=8 w=57 h=48 | left edge at 475 px; nearest shared grid column 39 px (delta 436 px) |
| A1 | accept-as-legacy | x=660 y=192 w=192 h=360 | x=220 y=64 w=64 h=120 | left edge at 660 px; nearest shared grid column 39 px (delta 621 px) |
| A2 | accept-as-legacy | x=0 y=168 w=1000 h=24 | x=0 y=56 w=333 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | x=0 y=552 w=1000 h=37 | x=0 y=184 w=333 h=12 | gap 37 px vs section median 56 px (delta 19 px) |
| T1 | accept-as-legacy | x=884 y=54 w=28 h=3 | x=295 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=64 w=28 h=4 | x=295 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=884 y=75 w=28 h=3 | x=295 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=837 y=253 w=10 h=4 | x=279 y=84 w=3 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=61 y=262 w=5 h=4 | x=20 y=87 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=107 y=505 w=5 h=4 | x=36 y=168 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=376 y=595 w=16 h=6 | x=125 y=198 w=5 h=2 | text line height 6 px (2 components) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=894 y=618 w=8 h=5 | x=298 y=206 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=894 y=726 w=8 h=5 | x=298 y=242 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-3.0-1500`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=497 y=192 w=438 h=360 | x=166 y=64 w=146 h=120 | left edge at 497 px; nearest shared grid column 535 px (delta 38 px) |
| A1 | accept-as-legacy | x=587 y=24 w=111 h=144 | x=196 y=8 w=37 h=48 | left edge at 587 px; nearest shared grid column 541 px (delta 46 px) |
| A1 | accept-as-legacy | x=725 y=24 w=170 h=144 | x=242 y=8 w=57 h=48 | left edge at 725 px; nearest shared grid column 541 px (delta 184 px) |
| A1 | accept-as-legacy | x=959 y=192 w=108 h=360 | x=320 y=64 w=36 h=120 | left edge at 959 px; nearest shared grid column 541 px (delta 418 px) |
| A2 | accept-as-legacy | x=0 y=168 w=1500 h=24 | x=0 y=56 w=500 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | x=0 y=552 w=1500 h=37 | x=0 y=184 w=500 h=12 | gap 37 px vs section median 56 px (delta 19 px) |
| T1 | accept-as-legacy | x=1384 y=54 w=28 h=3 | x=461 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1384 y=64 w=28 h=4 | x=461 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1384 y=75 w=28 h=3 | x=461 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1285 y=253 w=10 h=4 | x=428 y=84 w=3 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=61 y=262 w=5 h=4 | x=20 y=87 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=376 y=595 w=16 h=6 | x=125 y=198 w=5 h=2 | text line height 6 px (2 components) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=517 y=595 w=6 h=6 | x=172 y=198 w=2 h=2 | text line height 6 px (1 component) → em ≈ 2.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1394 y=618 w=8 h=5 | x=465 y=206 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=896 y=621 w=30 h=7 | x=299 y=207 w=10 h=2 | text line height 7 px (3 components) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=1394 y=726 w=8 h=5 | x=465 y=242 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-3.0-600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=85 y=192 w=239 h=360 | x=28 y=64 w=80 h=120 | left edge at 85 px; nearest shared grid column 39 px (delta 46 px) |
| A1 | accept-as-legacy | x=266 y=792 w=211 h=96 | x=89 y=264 w=70 h=32 | left edge at 266 px; nearest shared grid column 173 px (delta 93 px) |
| A1 | accept-as-legacy | x=311 y=24 w=170 h=144 | x=104 y=8 w=57 h=48 | left edge at 311 px; nearest shared grid column 173 px (delta 138 px) |
| A2 | accept-as-legacy | x=0 y=168 w=600 h=24 | x=0 y=56 w=200 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | x=0 y=552 w=600 h=37 | x=0 y=184 w=200 h=12 | gap 37 px vs section median 56 px (delta 19 px) |
| A2 | accept-as-legacy | x=0 y=628 w=600 h=69 | x=0 y=209 w=200 h=23 | gap 69 px vs section median 56 px (delta 13 px) |
| T1 | accept-as-legacy | x=545 y=54 w=28 h=3 | x=182 y=18 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=64 w=28 h=4 | x=182 y=21 w=9 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=545 y=75 w=28 h=3 | x=182 y=25 w=9 h=1 | text line height 3 px (1 component) → em ≈ 1.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=53 y=108 w=51 h=11 | x=18 y=36 w=17 h=4 | text line height 11 px (3 components) → em ≈ 5.2 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=61 y=262 w=5 h=4 | x=20 y=87 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=107 y=505 w=5 h=4 | x=36 y=168 w=2 h=1 | text line height 4 px (1 component) → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=564 y=618 w=8 h=5 | x=188 y=206 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=75 y=621 w=30 h=7 | x=25 y=207 w=10 h=2 | text line height 7 px (3 components) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=564 y=726 w=8 h=5 | x=188 y=242 w=3 h=2 | text line height 5 px (1 component) → em ≈ 2.4 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=75 y=729 w=30 h=7 | x=25 y=243 w=10 h=2 | text line height 7 px (3 components) → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=48 y=792 w=96 h=96 | x=16 y=264 w=32 h=32 | text line height 96 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `today-empty-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | accept-as-legacy | x=64 y=201 w=67 h=50 | x=64 y=201 w=67 h=50 | left edge at 64 px; nearest shared grid column 50 px (delta 14 px) |
| A1 | accept-as-legacy | x=227 y=201 w=66 h=50 | x=227 y=201 w=66 h=50 | left edge at 227 px; nearest shared grid column 50 px (delta 177 px) |
| A1 | accept-as-legacy | x=332 y=112 w=313 h=33 | x=332 y=112 w=313 h=33 | left edge at 332 px; nearest shared grid column 430 px (delta 98 px) |
| A1 | accept-as-legacy | x=347 y=864 w=66 h=32 | x=347 y=864 w=66 h=32 | left edge at 347 px; nearest shared grid column 430 px (delta 83 px) |
| A1 | accept-as-legacy | x=355 y=294 w=74 h=12 | x=355 y=294 w=74 h=12 | left edge at 355 px; nearest shared grid column 430 px (delta 75 px) |
| A1 | accept-as-legacy | x=358 y=201 w=630 h=50 | x=358 y=201 w=630 h=50 | left edge at 358 px; nearest shared grid column 430 px (delta 72 px) |
| A1 | accept-as-legacy | x=388 y=201 w=600 h=50 | x=388 y=201 w=600 h=50 | left edge at 388 px; nearest shared grid column 430 px (delta 42 px) |
| A1 | accept-as-legacy | x=402 y=294 w=27 h=12 | x=402 y=294 w=27 h=12 | left edge at 402 px; nearest shared grid column 430 px (delta 28 px) |
| A1 | accept-as-legacy | x=445 y=8 w=37 h=48 | x=445 y=8 w=37 h=48 | left edge at 445 px; nearest shared grid column 430 px (delta 15 px) |
| A1 | accept-as-legacy | x=492 y=8 w=57 h=48 | x=492 y=8 w=57 h=48 | left edge at 492 px; nearest shared grid column 505 px (delta 13 px) |
| A1 | accept-as-legacy | x=529 y=267 w=23 h=16 | x=529 y=267 w=23 h=16 | left edge at 529 px; nearest shared grid column 549 px (delta 20 px) |
| A1 | accept-as-legacy | x=653 y=112 w=172 h=33 | x=653 y=112 w=172 h=33 | left edge at 653 px; nearest shared grid column 549 px (delta 104 px) |
| A1 | accept-as-legacy | x=909 y=112 w=79 h=33 | x=909 y=112 w=79 h=33 | left edge at 909 px; nearest shared grid column 986 px (delta 77 px) |
| A1 | accept-as-legacy | x=914 y=112 w=74 h=33 | x=914 y=112 w=74 h=33 | left edge at 914 px; nearest shared grid column 986 px (delta 72 px) |
| A2 | fix-later | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | x=0 y=71 w=1000 h=13 | x=0 y=71 w=1000 h=13 | gap 13 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | x=0 y=145 w=1000 h=22 | x=0 y=145 w=1000 h=22 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | fix-later | x=0 y=177 w=1000 h=24 | x=0 y=177 w=1000 h=24 | gap 24 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | x=0 y=283 w=1000 h=11 | x=0 y=283 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=289 y=229 w=6 h=13 | x=289 y=229 w=6 h=13 | ink RGB (109, 113, 128) on background RGB (29, 32, 40) → contrast 3.4:1; role class Body; scrim-blend ink |

### `today-habits-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=41 y=379 w=17 h=12 | x=41 y=379 w=17 h=12 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | fix-later | x=44 y=193 w=52 h=41 | x=44 y=193 w=52 h=41 | left edge at 44 px; nearest shared grid column 38 px (delta 6 px) |
| A1 | accept-as-legacy | x=332 y=112 w=313 h=33 | x=332 y=112 w=313 h=33 | left edge at 332 px; nearest shared grid column 430 px (delta 98 px) |
| A1 | accept-as-legacy | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 430 px (delta 85 px) |
| A1 | accept-as-legacy | x=355 y=324 w=74 h=12 | x=355 y=324 w=74 h=12 | left edge at 355 px; nearest shared grid column 430 px (delta 75 px) |
| A1 | accept-as-legacy | x=402 y=324 w=27 h=12 | x=402 y=324 w=27 h=12 | left edge at 402 px; nearest shared grid column 430 px (delta 28 px) |
| A1 | accept-as-legacy | x=445 y=8 w=37 h=48 | x=445 y=8 w=37 h=48 | left edge at 445 px; nearest shared grid column 430 px (delta 15 px) |
| A1 | accept-as-legacy | x=487 y=193 w=394 h=41 | x=487 y=193 w=394 h=41 | left edge at 487 px; nearest shared grid column 471 px (delta 16 px) |
| A1 | accept-as-legacy | x=492 y=8 w=57 h=48 | x=492 y=8 w=57 h=48 | left edge at 492 px; nearest shared grid column 505 px (delta 13 px) |
| A1 | accept-as-legacy | x=529 y=297 w=23 h=16 | x=529 y=297 w=23 h=16 | left edge at 529 px; nearest shared grid column 549 px (delta 20 px) |
| A1 | accept-as-legacy | x=653 y=112 w=172 h=33 | x=653 y=112 w=172 h=33 | left edge at 653 px; nearest shared grid column 549 px (delta 104 px) |
| A1 | accept-as-legacy | x=893 y=193 w=23 h=41 | x=893 y=193 w=23 h=41 | left edge at 893 px; nearest shared grid column 998 px (delta 105 px) |
| A1 | accept-as-legacy | x=909 y=112 w=79 h=33 | x=909 y=112 w=79 h=33 | left edge at 909 px; nearest shared grid column 998 px (delta 89 px) |
| A1 | accept-as-legacy | x=914 y=112 w=74 h=33 | x=914 y=112 w=74 h=33 | left edge at 914 px; nearest shared grid column 998 px (delta 84 px) |
| A2 | fix-later | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | x=0 y=71 w=1000 h=13 | x=0 y=71 w=1000 h=13 | gap 13 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | x=0 y=145 w=1000 h=22 | x=0 y=145 w=1000 h=22 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=234 w=1000 h=4 | x=0 y=234 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=313 w=1000 h=11 | x=0 y=313 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| T1 | fix-later | x=893 y=193 w=23 h=33 | x=893 y=193 w=23 h=33 | text line height 33 px (1 component) → em ≈ 47.1 logical px; nearest ladder rung 30 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=10 y=191 w=12 h=12 | x=10 y=191 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=978 y=191 w=12 h=12 | x=978 y=191 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=10 y=224 w=12 h=12 | x=10 y=224 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |
| Q1 | accept-as-legacy | x=978 y=224 w=12 h=12 | x=978 y=224 w=12 h=12 | ink RGB (19, 21, 27) on background RGB (20, 22, 28) → contrast 1.0:1; role class Body; scrim-blend ink |

### `today-habits-kiosk-0.5-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=20 y=890 w=510 h=8 | x=40 y=1780 w=1020 h=16 | left edge at 20 px; nearest shared grid column 23 px (delta 3 px) |
| A1 | fix-later | x=28 y=57 w=10 h=7 | x=56 y=114 w=20 h=14 | left edge at 28 px; nearest shared grid column 30 px (delta 2 px) |
| A1 | accept-as-legacy | x=38 y=91 w=10 h=18 | x=76 y=182 w=20 h=36 | left edge at 38 px; nearest shared grid column 30 px (delta 8 px) |
| A1 | accept-as-legacy | x=42 y=91 w=6 h=18 | x=84 y=182 w=12 h=36 | left edge at 42 px; nearest shared grid column 49 px (delta 7 px) |
| A1 | accept-as-legacy | x=60 y=57 w=22 h=7 | x=120 y=114 w=44 h=14 | left edge at 60 px; nearest shared grid column 49 px (delta 11 px) |
| A1 | accept-as-legacy | x=65 y=57 w=17 h=7 | x=130 y=114 w=34 h=14 | left edge at 65 px; nearest shared grid column 49 px (delta 16 px) |
| A1 | accept-as-legacy | x=116 y=57 w=10 h=7 | x=232 y=114 w=20 h=14 | left edge at 116 px; nearest shared grid column 49 px (delta 67 px) |
| A1 | accept-as-legacy | x=159 y=57 w=17 h=7 | x=318 y=114 w=34 h=14 | left edge at 159 px; nearest shared grid column 49 px (delta 110 px) |
| A1 | accept-as-legacy | x=178 y=57 w=31 h=7 | x=356 y=114 w=62 h=14 | left edge at 178 px; nearest shared grid column 49 px (delta 129 px) |
| A1 | accept-as-legacy | x=182 y=57 w=27 h=7 | x=364 y=114 w=54 h=14 | left edge at 182 px; nearest shared grid column 49 px (delta 133 px) |
| A1 | accept-as-legacy | x=196 y=57 w=13 h=7 | x=392 y=114 w=26 h=14 | left edge at 196 px; nearest shared grid column 49 px (delta 147 px) |
| A1 | accept-as-legacy | x=202 y=57 w=7 h=7 | x=404 y=114 w=14 h=14 | left edge at 202 px; nearest shared grid column 49 px (delta 153 px) |
| A1 | accept-as-legacy | x=211 y=57 w=9 h=7 | x=422 y=114 w=18 h=14 | left edge at 211 px; nearest shared grid column 49 px (delta 162 px) |
| A1 | accept-as-legacy | x=226 y=57 w=26 h=7 | x=452 y=114 w=52 h=14 | left edge at 226 px; nearest shared grid column 49 px (delta 177 px) |
| A1 | accept-as-legacy | x=451 y=154 w=19 h=7 | x=902 y=308 w=38 h=14 | left edge at 451 px; nearest shared grid column 468 px (delta 17 px) |
| A1 | fix-later | x=462 y=154 w=8 h=7 | x=924 y=308 w=16 h=14 | left edge at 462 px; nearest shared grid column 468 px (delta 6 px) |
| A1 | fix-later | x=485 y=4 w=6 h=24 | x=970 y=8 w=12 h=48 | left edge at 485 px; nearest shared grid column 491 px (delta 6 px) |
| A1 | fix-later | x=496 y=4 w=29 h=24 | x=992 y=8 w=58 h=48 | left edge at 496 px; nearest shared grid column 494 px (delta 2 px) |
| A1 | accept-as-legacy | x=524 y=154 w=20 h=7 | x=1048 y=308 w=40 h=14 | left edge at 524 px; nearest shared grid column 494 px (delta 30 px) |
| A1 | accept-as-legacy | x=529 y=154 w=15 h=7 | x=1058 y=308 w=30 h=14 | left edge at 529 px; nearest shared grid column 494 px (delta 35 px) |
| A1 | accept-as-legacy | x=942 y=111 w=6 h=22 | x=1884 y=222 w=12 h=44 | left edge at 942 px; nearest shared grid column 999 px (delta 57 px) |
| A1 | accept-as-legacy | x=947 y=91 w=10 h=18 | x=1894 y=182 w=20 h=36 | left edge at 947 px; nearest shared grid column 999 px (delta 52 px) |
| A1 | accept-as-legacy | x=953 y=111 w=40 h=22 | x=1906 y=222 w=80 h=44 | left edge at 953 px; nearest shared grid column 999 px (delta 46 px) |
| A1 | accept-as-legacy | x=958 y=111 w=35 h=22 | x=1916 y=222 w=70 h=44 | left edge at 958 px; nearest shared grid column 999 px (delta 41 px) |
| A1 | accept-as-legacy | x=975 y=183 w=10 h=6 | x=1950 y=366 w=20 h=12 | left edge at 975 px; nearest shared grid column 999 px (delta 24 px) |
| A1 | accept-as-legacy | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 999 px (delta 19 px) |
| A2 | fix-later | x=0 y=28 w=1000 h=4 | x=0 y=56 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | x=0 y=64 w=1000 h=11 | x=0 y=128 w=2000 h=22 | gap 11 px vs section median 8 px (delta 3 px) |
| A2 | fix-later | x=0 y=90 w=1000 h=1 | x=0 y=180 w=2000 h=2 | gap 1 px vs section median 8 px (delta 7 px) |
| A2 | fix-later | x=0 y=109 w=1000 h=2 | x=0 y=218 w=2000 h=4 | gap 2 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | x=0 y=149 w=1000 h=5 | x=0 y=298 w=2000 h=10 | gap 5 px vs section median 8 px (delta 3 px) |

### `today-two-column-1.0-1600`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| A1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 13 px (delta 3 px) |
| A1 | fix-later | x=25 y=833 w=37 h=12 | x=25 y=833 w=37 h=12 | left edge at 25 px; nearest shared grid column 21 px (delta 4 px) |
| A1 | accept-as-legacy | x=41 y=188 w=17 h=12 | x=41 y=188 w=17 h=12 | left edge at 41 px; nearest shared grid column 56 px (delta 15 px) |
| A1 | accept-as-legacy | x=45 y=833 w=17 h=12 | x=45 y=833 w=17 h=12 | left edge at 45 px; nearest shared grid column 56 px (delta 11 px) |
| A1 | fix-later | x=52 y=797 w=19 h=15 | x=52 y=797 w=19 h=15 | left edge at 52 px; nearest shared grid column 56 px (delta 4 px) |
| A1 | accept-as-legacy | x=81 y=797 w=16 h=15 | x=81 y=797 w=16 h=15 | left edge at 81 px; nearest shared grid column 69 px (delta 12 px) |
| A1 | accept-as-legacy | x=217 y=797 w=44 h=15 | x=217 y=797 w=44 h=15 | left edge at 217 px; nearest shared grid column 69 px (delta 148 px) |
| A1 | accept-as-legacy | x=249 y=797 w=12 h=15 | x=249 y=797 w=12 h=15 | left edge at 249 px; nearest shared grid column 69 px (delta 180 px) |
| A1 | accept-as-legacy | x=335 y=797 w=12 h=15 | x=335 y=797 w=12 h=15 | left edge at 335 px; nearest shared grid column 69 px (delta 266 px) |
| A1 | accept-as-legacy | x=380 y=797 w=38 h=15 | x=380 y=797 w=38 h=15 | left edge at 380 px; nearest shared grid column 69 px (delta 311 px) |
| A1 | accept-as-legacy | x=393 y=797 w=25 h=15 | x=393 y=797 w=25 h=15 | left edge at 393 px; nearest shared grid column 69 px (delta 324 px) |
| A1 | accept-as-legacy | x=532 y=112 w=513 h=33 | x=532 y=112 w=513 h=33 | left edge at 532 px; nearest shared grid column 69 px (delta 463 px) |
| A1 | accept-as-legacy | x=545 y=864 w=71 h=32 | x=545 y=864 w=71 h=32 | left edge at 545 px; nearest shared grid column 69 px (delta 476 px) |
| A1 | accept-as-legacy | x=745 y=8 w=37 h=48 | x=745 y=8 w=37 h=48 | left edge at 745 px; nearest shared grid column 69 px (delta 676 px) |
| A1 | accept-as-legacy | x=770 y=8 w=12 h=48 | x=770 y=8 w=12 h=48 | left edge at 770 px; nearest shared grid column 69 px (delta 701 px) |
| A1 | accept-as-legacy | x=792 y=8 w=57 h=48 | x=792 y=8 w=57 h=48 | left edge at 792 px; nearest shared grid column 69 px (delta 723 px) |
| A1 | accept-as-legacy | x=1053 y=112 w=272 h=33 | x=1053 y=112 w=272 h=33 | left edge at 1053 px; nearest shared grid column 1598 px (delta 545 px) |
| A1 | accept-as-legacy | x=1459 y=112 w=129 h=33 | x=1459 y=112 w=129 h=33 | left edge at 1459 px; nearest shared grid column 1598 px (delta 139 px) |
| A1 | accept-as-legacy | x=1464 y=112 w=124 h=33 | x=1464 y=112 w=124 h=33 | left edge at 1464 px; nearest shared grid column 1598 px (delta 134 px) |
| A2 | fix-later | x=0 y=56 w=1600 h=8 | x=0 y=56 w=1600 h=8 | gap 8 px vs section median 15 px (delta 7 px) |
| A2 | fix-later | x=0 y=170 w=1600 h=18 | x=0 y=170 w=1600 h=18 | gap 18 px vs section median 15 px (delta 3 px) |
| A2 | accept-as-legacy | x=0 y=200 w=1600 h=25 | x=0 y=200 w=1600 h=25 | gap 25 px vs section median 15 px (delta 10 px) |
| A2 | fix-later | x=0 y=291 w=1600 h=9 | x=0 y=291 w=1600 h=9 | gap 9 px vs section median 15 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=307 w=1600 h=3 | x=0 y=307 w=1600 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | fix-later | x=0 y=374 w=1600 h=8 | x=0 y=374 w=1600 h=8 | gap 8 px vs section median 15 px (delta 7 px) |
| A2 | fix-later | x=0 y=784 w=1600 h=13 | x=0 y=784 w=1600 h=13 | gap 13 px vs section median 19 px (delta 6 px) |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |

### `track-entries-1.0-1000`

| invariant | grade | region (phys px) | logical | metric |
|---|---|---|---|---|
| C1 | fix-now | x=999 y=171 w=1 h=17 | x=999 y=171 w=1 h=17 | content run touches the right edge (extent 17 px along the edge) |
| C1 | fix-now | x=999 y=208 w=1 h=25 | x=999 y=208 w=1 h=25 | content run touches the right edge (extent 25 px along the edge) |
| C1 | accept-as-legacy | x=999 y=399 w=1 h=6 | x=999 y=399 w=1 h=6 | content run touches the right edge (extent 6 px along the edge) |
| C1 | fix-now | x=0 y=171 w=1 h=17 | x=0 y=171 w=1 h=17 | content run touches the left edge (extent 17 px along the edge) |
| C1 | fix-now | x=0 y=397 w=1 h=31 | x=0 y=397 w=1 h=31 | content run touches the left edge (extent 31 px along the edge) |
| A1 | fix-later | x=4 y=156 w=498 h=36 | x=4 y=156 w=498 h=36 | left edge at 4 px; nearest shared grid column 1 px (delta 3 px) |
| A1 | accept-as-legacy | x=127 y=373 w=20 h=81 | x=127 y=373 w=20 h=81 | left edge at 127 px; nearest shared grid column 108 px (delta 19 px) |
| A1 | accept-as-legacy | x=150 y=373 w=15 h=81 | x=150 y=373 w=15 h=81 | left edge at 150 px; nearest shared grid column 163 px (delta 13 px) |
| A1 | accept-as-legacy | x=213 y=373 w=15 h=81 | x=213 y=373 w=15 h=81 | left edge at 213 px; nearest shared grid column 226 px (delta 13 px) |
| A1 | accept-as-legacy | x=364 y=71 w=60 h=20 | x=364 y=71 w=60 h=20 | left edge at 364 px; nearest shared grid column 347 px (delta 17 px) |
| A1 | accept-as-legacy | x=411 y=71 w=13 h=20 | x=411 y=71 w=13 h=20 | left edge at 411 px; nearest shared grid column 347 px (delta 64 px) |
| A1 | accept-as-legacy | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 347 px (delta 77 px) |
| A1 | accept-as-legacy | x=538 y=71 w=19 h=20 | x=538 y=71 w=19 h=20 | left edge at 538 px; nearest shared grid column 515 px (delta 23 px) |
| A2 | fix-later | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 16 px (delta 4 px) |
| A2 | accept-as-legacy | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | x=0 y=192 w=1000 h=12 | x=0 y=192 w=1000 h=12 | gap 12 px vs section median 16 px (delta 4 px) |
| A2 | fix-later | x=0 y=237 w=1000 h=23 | x=0 y=237 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | x=0 y=269 w=1000 h=35 | x=0 y=269 w=1000 h=35 | gap 35 px vs section median 16 px (delta 19 px) |
| A2 | accept-as-legacy | x=0 y=305 w=1000 h=2 | x=0 y=305 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=309 w=1000 h=2 | x=0 y=309 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | x=0 y=312 w=1000 h=28 | x=0 y=312 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | x=0 y=454 w=1000 h=22 | x=0 y=454 w=1000 h=22 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | x=0 y=485 w=1000 h=3 | x=0 y=485 w=1000 h=3 | gap 3 px vs section median 16 px (delta 13 px) |
| A2 | accept-as-legacy | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |
| T1 | accept-as-legacy | x=228 y=71 w=10 h=4 | x=228 y=71 w=10 h=4 | text line height 4 px (2 components) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=298 y=71 w=4 h=4 | x=298 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=399 y=71 w=73 h=17 | x=399 y=71 w=73 h=17 | text line height 17 px (6 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | accept-as-legacy | x=658 y=71 w=4 h=4 | x=658 y=71 w=4 h=4 | text line height 4 px (1 component) → em ≈ 5.7 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | x=12 y=75 w=4 h=6 | x=12 y=75 w=4 h=6 | text line height 6 px (1 component) → em ≈ 8.6 logical px; nearest ladder rung 11 px |
| T1 | fix-later | x=164 y=75 w=23 h=17 | x=164 y=75 w=23 h=17 | text line height 17 px (2 components) → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | text line height 32 px (1 component) → em ≈ 45.7 logical px; nearest ladder rung 30 px |
| Q1 | accept-as-legacy | x=32 y=474 w=6 h=13 | x=32 y=474 w=6 h=13 | ink RGB (107, 111, 125) on background RGB (20, 22, 28) → contrast 3.6:1; role class Body; scrim-blend ink |

