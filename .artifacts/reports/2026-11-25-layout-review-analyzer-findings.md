# Layout Review Analyzer — Findings (round 7, chunk G3, #1313)

- **Date**: 2026-11-25
- **Posture**: advisory-first (binding, plan D6). This report is data, not a gate — the analyzer reports; it never gates. No CI wiring, no nextest test, no pre-commit hook references this tool.
- **Run command**:

  ```sh
  scripts/layout_review.py crates/taskboard-ui-slint/tests/goldens \
      --out .artifacts/reports/2026-11-25-layout-review-analyzer-findings.toml
  ```

  `findings.toml` beside this file is the source of truth; this report is generated from it by `--emit-md`. Metric/region fields reproduce byte-identically on re-run (the corpus is byte-deterministic per G1); `grade` fields are the deterministic D5 rubric applied at run time — for C1 the rubric consumes the #1418 typed classification (real-clip / thumb / chrome-speck) emitted by the detector, so the grade is reproducible without hand labour — and the accept-as-legacy grades name explained debt — the grade never silences the detector.

- **Calibration note** (D7): thresholds calibrated once on this corpus before grading — corner-sampled background flat at RGB ≈ (29,32,40) with ±2/255 anti-aliasing noise; content floor 12/255 sits above the noise and below the dimmest ink; edge floor 8 logical px sits above hairlines and below every real block boundary; text tolerance ±15% absorbs ≤1 px line-height slop at every corpus scale without overlapping ladder rungs (closest separation 12%, 12.5→14). Thresholds were not widened after grading.

## Headline counts — 1079 findings over the corpus

| invariant | accept-as-legacy | fix-now | fix-later | total |
|---|---|---|---|---|
| A1 | 544 | 0 | 68 | 612 |
| A2 | 192 | 0 | 181 | 373 |
| T1 | 43 | 0 | 10 | 53 |
| M1 | 7 | 0 | 3 | 10 |
| C1 | 21 | 8 | 2 | 31 |
| **total** | **807** | **8** | **264** | **1079** |

Grading rubric (D5): **fix-now** — a defect class the project already tracks as a bug (C1 clip signatures); **fix-later** — real drift from the invariant with no open issue (the finding is the intake record for a future backlog issue); **accept-as-legacy** — the deviation is load-bearing in the committed golden or the manifest's state note explains it.

Grading rules applied to this run (deterministic, recorded per D5 — the rubric absorbs legacy noise, never the detector):

- C1 (the #1418 clip-sentinel law, 2026-11-25): every C1 finding carries a typed classification — real-clip / thumb / chrome-speck — and the rubric reads the classification, not raw extent: thumb or chrome-speck → accept-as-legacy (rationale emitted from the classification + the calibration note); real-clip ≥ 12 px → fix-now (the severing class #1365/#1378); real-clip < 12 px → fix-later (a real drift smaller than the severing class — the class the old extent-only rubric had no home for). The thumb model is extent-band + track-column signature (a contiguous near-floor band ≥ 25% of the edge through the run), never a bare size threshold. The 12-px line stays in the metric strings as the calibration record.
- A1: delta to the nearest shared grid column ≤ 6 px → fix-later (near-miss drift); larger → accept-as-legacy (a distinct layout region, not a near-miss of the grid).
- A2: |gap − median| ≤ 8 px → fix-later (token-adjacent drift, the #1334 one-row-offset class); larger → accept-as-legacy (deliberate section breaks and spacers).
- T1: em ≥ 20 logical px → fix-later (genuine off-ladder text between the title and display rungs); em < 20 → accept-as-legacy (hairline rules and divider rows, not text ink).
- M1: icon-probe and board-drag-indicator margins → accept-as-legacy (the manifest's state note: deliberate kind-wall padding / mostly-empty drag canvas); the rest → fix-later.

## Findings by golden

### Grade reconciliation vs the previous hand-applied run

| golden | metric | old grade | new grade | classification |
|---|---|---|---|---|
| `task-view-0.5-900` | left edge at 501 px; nearest shared grid column 507 px (delta 6 px) | accept-as-legacy | fix-later | - |
| `task-view-0.5-900` | left edge at 503 px; nearest shared grid column 507 px (delta 4 px) | accept-as-legacy | fix-later | - |
| `task-view-0.5-900` | left edge at 980 px; nearest shared grid column 984 px (delta 4 px) | accept-as-legacy | fix-later | - |
| `task-view-0.5-500` | left edge at 501 px; nearest shared grid column 507 px (delta 6 px) | accept-as-legacy | fix-later | - |
| `task-view-0.5-500` | left edge at 503 px; nearest shared grid column 507 px (delta 4 px) | accept-as-legacy | fix-later | - |
| `task-view-0.5-500` | left edge at 980 px; nearest shared grid column 984 px (delta 4 px) | accept-as-legacy | fix-later | - |
| `task-view-3.0-500` | content run touches the bottom edge (extent 6 px along the edge) | accept-as-legacy | fix-later | real-clip |
| `task-view-3.0-500` | content run touches the bottom edge (extent 6 px along the edge) | accept-as-legacy | fix-later | real-clip |
| `capture-overlay-1.0-1000` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-1000` | gap 23 px vs section median 16 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `search-palette-1.0-1000` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-1000` | gap 23 px vs section median 16 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `review-walk-1.0-1000` | gap 30 px vs section median 11 px (delta 19 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-1000` | gap 8 px vs section median 11 px (delta 3 px) | accept-as-legacy | fix-later | - |
| `review-walk-1.0-1000` | gap 7 px vs section median 11 px (delta 4 px) | accept-as-legacy | fix-later | - |
| `review-walk-1.0-1000` | gap 4 px vs section median 11 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `task-view-overdue-banner-1.0-1000` | left edge at 249 px; nearest shared grid column 244 px (delta 5 px) | accept-as-legacy | fix-later | - |
| `search-palette-empty-1.0-1000` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `search-palette-empty-1.0-1000` | gap 23 px vs section median 16 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `search-palette-1.0-600` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-600` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-600` | gap 23 px vs section median 16 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `review-walk-1.0-600` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-600` | gap 2 px vs section median 16 px (delta 14 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-600` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-600` | gap 23 px vs section median 16 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `review-walk-1.0-600` | gap 24 px vs section median 16 px (delta 8 px) | accept-as-legacy | fix-later | - |
| `capture-overlay-1.0-600` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-600` | gap 7 px vs section median 16 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-600` | gap 23 px vs section median 16 px (delta 7 px) | accept-as-legacy | fix-later | - |
| `capture-overlay-1.0-1500` | gap 2 px vs section median 11 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-1500` | gap 1 px vs section median 11 px (delta 10 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-1500` | gap 1 px vs section median 11 px (delta 10 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-1500` | gap 2 px vs section median 11 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `capture-overlay-1.0-1500` | gap 1 px vs section median 11 px (delta 10 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-1500` | gap 2 px vs section median 11 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-1500` | gap 1 px vs section median 11 px (delta 10 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-1500` | gap 1 px vs section median 11 px (delta 10 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-1500` | gap 2 px vs section median 11 px (delta 9 px) | fix-later | accept-as-legacy | - |
| `search-palette-1.0-1500` | gap 1 px vs section median 11 px (delta 10 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-1500` | gap 23 px vs section median 11 px (delta 12 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-1500` | gap 24 px vs section median 11 px (delta 13 px) | fix-later | accept-as-legacy | - |
| `review-walk-1.0-1500` | gap 6 px vs section median 11 px (delta 5 px) | accept-as-legacy | fix-later | - |

Every flip is recorded with its classifier rationale in `findings.toml`; none is silent. The flips in this run are the designed gap-fill of the #1418 rubric: sub-severing real clips (the editor-family sever runs below 12 px) the old extent-only rule blurred into accept-as-legacy now classify `real-clip` and grade fix-later.

### `action-menu-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=332 y=60 w=18 h=784 | x=332 y=60 w=18 h=784 | left edge at 332 px; nearest shared grid column 551 px (delta 219 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 551 px (delta 206 px) |
| A1 | accept-as-legacy | - | x=378 y=60 w=20 h=784 | x=378 y=60 w=20 h=784 | left edge at 378 px; nearest shared grid column 551 px (delta 173 px) |
| A1 | accept-as-legacy | - | x=386 y=60 w=12 h=784 | x=386 y=60 w=12 h=784 | left edge at 386 px; nearest shared grid column 551 px (delta 165 px) |
| A1 | accept-as-legacy | - | x=445 y=8 w=37 h=48 | x=445 y=8 w=37 h=48 | left edge at 445 px; nearest shared grid column 551 px (delta 106 px) |
| A1 | accept-as-legacy | - | x=470 y=8 w=12 h=48 | x=470 y=8 w=12 h=48 | left edge at 470 px; nearest shared grid column 551 px (delta 81 px) |
| A1 | accept-as-legacy | - | x=492 y=8 w=57 h=48 | x=492 y=8 w=57 h=48 | left edge at 492 px; nearest shared grid column 551 px (delta 59 px) |
| A1 | accept-as-legacy | - | x=536 y=60 w=12 h=784 | x=536 y=60 w=12 h=784 | left edge at 536 px; nearest shared grid column 551 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=844 w=1000 h=9 | x=0 y=844 w=1000 h=9 | gap 9 px vs section median 4 px (delta 5 px) |

### `action-menu-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=499 y=60 w=180 h=784 | x=499 y=60 w=180 h=784 | left edge at 499 px; nearest shared grid column 686 px (delta 187 px) |
| A1 | accept-as-legacy | - | x=512 y=864 w=71 h=32 | x=512 y=864 w=71 h=32 | left edge at 512 px; nearest shared grid column 686 px (delta 174 px) |
| A1 | accept-as-legacy | - | x=605 y=60 w=74 h=784 | x=605 y=60 w=74 h=784 | left edge at 605 px; nearest shared grid column 686 px (delta 81 px) |
| A1 | accept-as-legacy | - | x=652 y=60 w=27 h=784 | x=652 y=60 w=27 h=784 | left edge at 652 px; nearest shared grid column 686 px (delta 34 px) |
| A1 | accept-as-legacy | - | x=695 y=8 w=37 h=48 | x=695 y=8 w=37 h=48 | left edge at 695 px; nearest shared grid column 688 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=742 y=8 w=57 h=48 | x=742 y=8 w=57 h=48 | left edge at 742 px; nearest shared grid column 721 px (delta 21 px) |
| A1 | accept-as-legacy | - | x=755 y=60 w=56 h=784 | x=755 y=60 w=56 h=784 | left edge at 755 px; nearest shared grid column 721 px (delta 34 px) |
| A1 | accept-as-legacy | - | x=799 y=60 w=12 h=784 | x=799 y=60 w=12 h=784 | left edge at 799 px; nearest shared grid column 721 px (delta 78 px) |
| A1 | accept-as-legacy | - | x=878 y=60 w=20 h=784 | x=878 y=60 w=20 h=784 | left edge at 878 px; nearest shared grid column 721 px (delta 157 px) |
| A1 | accept-as-legacy | - | x=886 y=60 w=12 h=784 | x=886 y=60 w=12 h=784 | left edge at 886 px; nearest shared grid column 721 px (delta 165 px) |
| A1 | accept-as-legacy | - | x=1036 y=60 w=12 h=784 | x=1036 y=60 w=12 h=784 | left edge at 1036 px; nearest shared grid column 1051 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=844 w=1500 h=11 | x=0 y=844 w=1500 h=11 | gap 11 px vs section median 4 px (delta 7 px) |

### `action-menu-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=155 y=60 w=36 h=800 | x=155 y=60 w=36 h=800 | left edge at 155 px; nearest shared grid column 57 px (delta 98 px) |
| A1 | accept-as-legacy | - | x=199 y=60 w=30 h=800 | x=199 y=60 w=30 h=800 | left edge at 199 px; nearest shared grid column 57 px (delta 142 px) |
| A1 | accept-as-legacy | - | x=202 y=60 w=27 h=800 | x=202 y=60 w=27 h=800 | left edge at 202 px; nearest shared grid column 57 px (delta 145 px) |
| A1 | accept-as-legacy | - | x=212 y=864 w=71 h=32 | x=212 y=864 w=71 h=32 | left edge at 212 px; nearest shared grid column 57 px (delta 155 px) |
| A1 | accept-as-legacy | - | x=302 y=60 w=20 h=800 | x=302 y=60 w=20 h=800 | left edge at 302 px; nearest shared grid column 460 px (delta 158 px) |
| A1 | accept-as-legacy | - | x=310 y=60 w=12 h=800 | x=310 y=60 w=12 h=800 | left edge at 310 px; nearest shared grid column 460 px (delta 150 px) |
| A1 | accept-as-legacy | - | x=436 y=8 w=38 h=48 | x=436 y=8 w=38 h=48 | left edge at 436 px; nearest shared grid column 460 px (delta 24 px) |

### `alarm-window-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| M1 | fix-later | - | x=320 y=0 w=1 h=64 | x=320 y=0 w=1 h=64 | right margin 30.0 logical px (30 px at scale 1.0); nearest rung 32 px |
| A1 | accept-as-legacy | - | x=25 y=24 w=26 h=32 | x=25 y=24 w=26 h=32 | left edge at 25 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=64 y=24 w=28 h=32 | x=64 y=24 w=28 h=32 | left edge at 64 px; no other left edge shares this column |

### `alarm-window-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| M1 | fix-later | - | x=1469 y=0 w=1 h=64 | x=1469 y=0 w=1 h=64 | right margin 30.0 logical px (30 px at scale 1.0); nearest rung 32 px |
| A1 | accept-as-legacy | - | x=25 y=32 w=26 h=16 | x=25 y=32 w=26 h=16 | left edge at 25 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=64 y=32 w=28 h=16 | x=64 y=32 w=28 h=16 | left edge at 64 px; no other left edge shares this column |

### `alarm-window-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| M1 | fix-later | - | x=569 y=0 w=1 h=64 | x=569 y=0 w=1 h=64 | right margin 30.0 logical px (30 px at scale 1.0); nearest rung 32 px |
| A1 | accept-as-legacy | - | x=25 y=24 w=26 h=32 | x=25 y=24 w=26 h=32 | left edge at 25 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=64 y=24 w=28 h=32 | x=64 y=24 w=28 h=32 | left edge at 64 px; no other left edge shares this column |

### `board-0.5-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=33 w=1 h=3 | x=0 y=66 w=2 h=6 | content run touches the left edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 848 px = 94% of the edge through the run) |
| A1 | accept-as-legacy | - | x=28 y=882 w=347 h=16 | x=56 y=1764 w=694 h=32 | left edge at 28 px; nearest shared grid column 12 px (delta 16 px) |
| A1 | accept-as-legacy | - | x=339 y=882 w=36 h=16 | x=678 y=1764 w=72 h=32 | left edge at 339 px; nearest shared grid column 12 px (delta 327 px) |
| A1 | accept-as-legacy | - | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 12 px (delta 450 px) |
| A1 | accept-as-legacy | - | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 12 px (delta 484 px) |
| A1 | accept-as-legacy | - | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 12 px (delta 491 px) |
| A1 | accept-as-legacy | - | x=534 y=882 w=6 h=16 | x=1068 y=1764 w=12 h=32 | left edge at 534 px; nearest shared grid column 12 px (delta 522 px) |
| A1 | accept-as-legacy | - | x=953 y=61 w=7 h=18 | x=1906 y=122 w=14 h=36 | left edge at 953 px; nearest shared grid column 12 px (delta 941 px) |
| A1 | accept-as-legacy | - | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 12 px (delta 968 px) |
| A2 | fix-later | - | x=0 y=28 w=1000 h=4 | x=0 y=56 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=45 w=1000 h=16 | x=0 y=90 w=2000 h=32 | gap 16 px vs section median 8 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=79 w=1000 h=3 | x=0 y=158 w=2000 h=6 | gap 3 px vs section median 8 px (delta 5 px) |

### `board-0.5-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=33 w=1 h=3 | x=0 y=66 w=2 h=6 | content run touches the left edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 848 px = 94% of the edge through the run) |
| A1 | accept-as-legacy | - | x=28 y=882 w=514 h=16 | x=56 y=1764 w=1028 h=32 | left edge at 28 px; nearest shared grid column 12 px (delta 16 px) |
| A1 | accept-as-legacy | - | x=506 y=882 w=36 h=16 | x=1012 y=1764 w=72 h=32 | left edge at 506 px; nearest shared grid column 12 px (delta 494 px) |
| A1 | accept-as-legacy | - | x=712 y=4 w=28 h=24 | x=1424 y=8 w=56 h=48 | left edge at 712 px; nearest shared grid column 12 px (delta 700 px) |
| A1 | accept-as-legacy | - | x=746 y=4 w=6 h=24 | x=1492 y=8 w=12 h=48 | left edge at 746 px; nearest shared grid column 12 px (delta 734 px) |
| A1 | accept-as-legacy | - | x=753 y=4 w=16 h=24 | x=1506 y=8 w=32 h=48 | left edge at 753 px; nearest shared grid column 12 px (delta 741 px) |
| A1 | accept-as-legacy | - | x=784 y=882 w=6 h=16 | x=1568 y=1764 w=12 h=32 | left edge at 784 px; nearest shared grid column 12 px (delta 772 px) |
| A1 | accept-as-legacy | - | x=1453 y=62 w=7 h=16 | x=2906 y=124 w=14 h=32 | left edge at 1453 px; nearest shared grid column 12 px (delta 1441 px) |
| A1 | accept-as-legacy | - | x=1480 y=4 w=6 h=24 | x=2960 y=8 w=12 h=48 | left edge at 1480 px; nearest shared grid column 12 px (delta 1468 px) |
| A2 | fix-later | - | x=0 y=45 w=1500 h=17 | x=0 y=90 w=3000 h=34 | gap 17 px vs section median 13 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=78 w=1500 h=4 | x=0 y=156 w=3000 h=8 | gap 4 px vs section median 13 px (delta 9 px) |

### `board-0.5-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=599 y=76 w=1 h=8 | x=1198 y=152 w=2 h=16 | content run touches the right edge (extent 8 px along the edge) — extent 8 px inside the thumb band with a track-column signature (contiguous near-floor band 848 px = 94% of the edge through the run) |
| C1 | fix-now | real-clip | x=0 y=56 w=1 h=12 | x=0 y=112 w=2 h=24 | content run touches the left edge (extent 12 px along the edge) — extent 12 px at/above the 12 px severing floor |
| C1 | accept-as-legacy | thumb | x=0 y=76 w=1 h=8 | x=0 y=152 w=2 h=16 | content run touches the left edge (extent 8 px along the edge) — extent 8 px inside the thumb band with a track-column signature (contiguous near-floor band 812 px = 90% of the edge through the run) |
| A1 | accept-as-legacy | - | x=28 y=882 w=214 h=16 | x=56 y=1764 w=428 h=32 | left edge at 28 px; nearest shared grid column 9 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=206 y=882 w=36 h=16 | x=412 y=1764 w=72 h=32 | left edge at 206 px; nearest shared grid column 9 px (delta 197 px) |
| A1 | accept-as-legacy | - | x=334 y=882 w=6 h=16 | x=668 y=1764 w=12 h=32 | left edge at 334 px; nearest shared grid column 9 px (delta 325 px) |
| A1 | accept-as-legacy | - | x=513 y=4 w=28 h=24 | x=1026 y=8 w=56 h=48 | left edge at 513 px; nearest shared grid column 9 px (delta 504 px) |
| A1 | accept-as-legacy | - | x=547 y=4 w=6 h=24 | x=1094 y=8 w=12 h=48 | left edge at 547 px; nearest shared grid column 9 px (delta 538 px) |
| A1 | accept-as-legacy | - | x=554 y=4 w=16 h=24 | x=1108 y=8 w=32 h=48 | left edge at 554 px; nearest shared grid column 9 px (delta 545 px) |
| A1 | accept-as-legacy | - | x=580 y=4 w=6 h=24 | x=1160 y=8 w=12 h=48 | left edge at 580 px; nearest shared grid column 9 px (delta 571 px) |

### `board-3.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=999 y=309 w=1 h=3 | x=333 y=103 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 588 px = 65% of the edge through the run) |
| C1 | accept-as-legacy | thumb | x=0 y=209 w=1 h=7 | x=0 y=70 w=0 h=2 | content run touches the left edge (extent 7 px along the edge) — extent 7 px inside the thumb band with a track-column signature (contiguous near-floor band 588 px = 65% of the edge through the run) |
| A1 | accept-as-legacy | - | x=60 y=492 w=724 h=96 | x=20 y=164 w=241 h=32 | left edge at 60 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=274 y=24 w=166 h=144 | x=91 y=8 w=55 h=48 | left edge at 274 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=368 y=792 w=211 h=96 | x=123 y=264 w=70 h=32 | left edge at 368 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=555 y=192 w=110 h=120 | x=185 y=64 w=37 h=40 | left edge at 555 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=722 y=360 w=38 h=120 | x=241 y=120 w=13 h=40 | left edge at 722 px; no other left edge shares this column |
| A2 | accept-as-legacy | - | x=0 y=168 w=1000 h=24 | x=0 y=56 w=333 h=8 | gap 24 px vs section median 48 px (delta 24 px) |
| A2 | accept-as-legacy | - | x=0 y=480 w=1000 h=12 | x=0 y=160 w=333 h=4 | gap 12 px vs section median 48 px (delta 36 px) |
| A2 | accept-as-legacy | - | x=0 y=684 w=1000 h=108 | x=0 y=228 w=333 h=36 | gap 108 px vs section median 48 px (delta 60 px) |

### `board-3.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=1499 y=309 w=1 h=3 | x=500 y=103 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 588 px = 65% of the edge through the run) |
| C1 | accept-as-legacy | thumb | x=0 y=209 w=1 h=7 | x=0 y=70 w=0 h=2 | content run touches the left edge (extent 7 px along the edge) — extent 7 px inside the thumb band with a track-column signature (contiguous near-floor band 588 px = 65% of the edge through the run) |
| A1 | accept-as-legacy | - | x=60 y=492 w=1224 h=96 | x=20 y=164 w=408 h=32 | left edge at 60 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=524 y=24 w=166 h=144 | x=175 y=8 w=55 h=48 | left edge at 524 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=535 y=792 w=211 h=96 | x=178 y=264 w=70 h=32 | left edge at 535 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=1222 y=360 w=38 h=120 | x=407 y=120 w=13 h=40 | left edge at 1222 px; no other left edge shares this column |
| A2 | accept-as-legacy | - | x=0 y=168 w=1500 h=24 | x=0 y=56 w=500 h=8 | gap 24 px vs section median 40 px (delta 16 px) |
| A2 | accept-as-legacy | - | x=0 y=216 w=1500 h=20 | x=0 y=72 w=500 h=7 | gap 20 px vs section median 40 px (delta 20 px) |
| A2 | fix-later | - | x=0 y=312 w=1500 h=48 | x=0 y=104 w=500 h=16 | gap 48 px vs section median 40 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=480 w=1500 h=12 | x=0 y=160 w=500 h=4 | gap 12 px vs section median 40 px (delta 28 px) |
| A2 | accept-as-legacy | - | x=0 y=588 w=1500 h=69 | x=0 y=196 w=500 h=23 | gap 69 px vs section median 40 px (delta 29 px) |
| A2 | accept-as-legacy | - | x=0 y=663 w=1500 h=129 | x=0 y=221 w=500 h=43 | gap 129 px vs section median 40 px (delta 89 px) |
| T1 | accept-as-legacy | - | x=0 y=308 w=1500 h=4 | x=0 y=103 w=500 h=1 | text line height 4 px → em ≈ 1.9 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=657 w=1500 h=6 | x=0 y=219 w=500 h=2 | text line height 6 px → em ≈ 2.9 logical px; nearest ladder rung 11 px |

### `board-3.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=599 y=309 w=1 h=3 | x=200 y=103 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 259 px = 29% of the edge through the run) |
| C1 | fix-now | real-clip | x=599 y=456 w=1 h=48 | x=200 y=152 w=0 h=16 | content run touches the right edge (extent 48 px along the edge) — extent 48 px at/above the 12 px severing floor |
| C1 | accept-as-legacy | chrome-speck | x=0 y=309 w=1 h=3 | x=0 y=103 w=0 h=1 | content run touches the left edge (extent 3 px along the edge) — sub-severing extent 3 px at the chrome scale with no track-column signature (near-floor band 144 px = 16% of the edge) |
| C1 | fix-now | real-clip | x=0 y=336 w=1 h=72 | x=0 y=112 w=0 h=24 | content run touches the left edge (extent 72 px along the edge) — extent 72 px at/above the 12 px severing floor |
| C1 | fix-now | real-clip | x=0 y=456 w=1 h=48 | x=0 y=152 w=0 h=16 | content run touches the left edge (extent 48 px along the edge) — extent 48 px at/above the 12 px severing floor |
| A1 | accept-as-legacy | - | x=145 y=24 w=166 h=144 | x=48 y=8 w=55 h=48 | left edge at 145 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=266 y=792 w=211 h=96 | x=89 y=264 w=70 h=32 | left edge at 266 px; no other left edge shares this column |
| A2 | accept-as-legacy | - | x=0 y=300 w=600 h=9 | x=0 y=100 w=200 h=3 | gap 9 px vs section median 36 px (delta 27 px) |
| A2 | accept-as-legacy | - | x=0 y=312 w=600 h=12 | x=0 y=104 w=200 h=4 | gap 12 px vs section median 36 px (delta 24 px) |
| A2 | accept-as-legacy | - | x=0 y=420 w=600 h=12 | x=0 y=140 w=200 h=4 | gap 12 px vs section median 36 px (delta 24 px) |
| A2 | accept-as-legacy | - | x=0 y=528 w=600 h=168 | x=0 y=176 w=200 h=56 | gap 168 px vs section median 36 px (delta 132 px) |
| A2 | accept-as-legacy | - | x=0 y=744 w=600 h=48 | x=0 y=248 w=200 h=16 | gap 48 px vs section median 36 px (delta 12 px) |
| T1 | accept-as-legacy | - | x=0 y=309 w=600 h=3 | x=0 y=103 w=200 h=1 | text line height 3 px → em ≈ 1.4 logical px; nearest ladder rung 11 px |

### `board-drag-indicator-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=68 w=1 h=4 | x=0 y=68 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) — extent 4 px inside the thumb band with a track-column signature (contiguous near-floor band 391 px = 43% of the edge through the run) |
| M1 | accept-as-legacy | - | x=0 y=508 w=1000 h=1 | x=0 y=508 w=1000 h=1 | bottom margin 391.0 logical px (391 px at scale 1.0); nearest rung 96 px |
| A1 | fix-later | - | x=16 y=459 w=32 h=32 | x=16 y=459 w=32 h=32 | left edge at 16 px; nearest shared grid column 20 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=136 y=497 w=46 h=12 | x=136 y=497 w=46 h=12 | left edge at 136 px; nearest shared grid column 21 px (delta 115 px) |
| A1 | accept-as-legacy | - | x=320 y=459 w=71 h=32 | x=320 y=459 w=71 h=32 | left edge at 320 px; nearest shared grid column 528 px (delta 208 px) |
| A1 | accept-as-legacy | - | x=339 y=497 w=69 h=12 | x=339 y=497 w=69 h=12 | left edge at 339 px; nearest shared grid column 528 px (delta 189 px) |
| A1 | accept-as-legacy | - | x=391 y=497 w=17 h=12 | x=391 y=497 w=17 h=12 | left edge at 391 px; nearest shared grid column 528 px (delta 137 px) |
| A1 | accept-as-legacy | - | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 528 px (delta 104 px) |
| A1 | accept-as-legacy | - | x=436 y=497 w=68 h=12 | x=436 y=497 w=68 h=12 | left edge at 436 px; nearest shared grid column 528 px (delta 92 px) |
| A1 | accept-as-legacy | - | x=477 y=497 w=27 h=12 | x=477 y=497 w=27 h=12 | left edge at 477 px; nearest shared grid column 528 px (delta 51 px) |
| A1 | accept-as-legacy | - | x=545 y=497 w=35 h=12 | x=545 y=497 w=35 h=12 | left edge at 545 px; nearest shared grid column 529 px (delta 16 px) |
| A1 | accept-as-legacy | - | x=660 y=497 w=20 h=12 | x=660 y=497 w=20 h=12 | left edge at 660 px; nearest shared grid column 529 px (delta 131 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 14 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=72 w=1000 h=6 | x=0 y=72 w=1000 h=6 | gap 6 px vs section median 14 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=160 w=1000 h=4 | x=0 y=160 w=1000 h=4 | gap 4 px vs section median 14 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=196 w=1000 h=80 | x=0 y=196 w=1000 h=80 | gap 80 px vs section median 14 px (delta 66 px) |
| A2 | accept-as-legacy | - | x=0 y=416 w=1000 h=43 | x=0 y=416 w=1000 h=43 | gap 43 px vs section median 23 px (delta 20 px) |
| A2 | accept-as-legacy | - | x=0 y=491 w=1000 h=6 | x=0 y=491 w=1000 h=6 | gap 6 px vs section median 23 px (delta 17 px) |

### `board-empty-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=375 w=1 h=4 | x=0 y=375 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) — extent 4 px inside the thumb band with a track-column signature (contiguous near-floor band 796 px = 88% of the edge through the run) |
| A1 | accept-as-legacy | - | x=54 y=8 w=18 h=48 | x=54 y=8 w=18 h=48 | left edge at 54 px; nearest shared grid column 67 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=347 y=864 w=66 h=32 | x=347 y=864 w=66 h=32 | left edge at 347 px; nearest shared grid column 434 px (delta 87 px) |
| A1 | accept-as-legacy | - | x=404 y=522 w=193 h=32 | x=404 y=522 w=193 h=32 | left edge at 404 px; nearest shared grid column 434 px (delta 30 px) |
| A1 | accept-as-legacy | - | x=483 y=426 w=13 h=13 | x=483 y=426 w=13 h=13 | left edge at 483 px; nearest shared grid column 436 px (delta 47 px) |

### `capture-overlay-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=220 y=384 w=560 h=92 | x=220 y=384 w=560 h=92 | left edge at 220 px; nearest shared grid column 57 px (delta 163 px) |
| A1 | accept-as-legacy | - | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 430 px (delta 86 px) |
| A1 | accept-as-legacy | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 430 px (delta 64 px) |
| A1 | accept-as-legacy | - | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 430 px (delta 59 px) |
| A1 | fix-later | - | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=445 y=24 w=37 h=19 | x=445 y=24 w=37 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=529 y=205 w=23 h=16 | x=529 y=205 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 505 px (delta 229 px) |
| A1 | accept-as-legacy | - | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 505 px (delta 235 px) |
| A1 | accept-as-legacy | - | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 505 px (delta 404 px) |
| A1 | accept-as-legacy | - | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 505 px (delta 409 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=43 w=1000 h=10 | x=0 y=43 w=1000 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 16 px (delta 14 px) |
| A2 | fix-later | - | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1000 h=28 | x=0 y=177 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=221 w=1000 h=11 | x=0 y=221 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=241 w=1000 h=1 | x=0 y=241 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=269 w=1000 h=19 | x=0 y=269 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=298 w=1000 h=86 | x=0 y=298 w=1000 h=86 | gap 86 px vs section median 16 px (delta 70 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=1000 h=3 | x=0 y=112 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=1000 h=3 | x=0 y=142 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `capture-overlay-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=470 y=384 w=560 h=92 | x=470 y=384 w=560 h=92 | left edge at 470 px; nearest shared grid column 680 px (delta 210 px) |
| A1 | accept-as-legacy | - | x=511 y=123 w=16 h=12 | x=511 y=123 w=16 h=12 | left edge at 511 px; nearest shared grid column 680 px (delta 169 px) |
| A1 | accept-as-legacy | - | x=529 y=123 w=570 h=12 | x=529 y=123 w=570 h=12 | left edge at 529 px; nearest shared grid column 680 px (delta 151 px) |
| A1 | accept-as-legacy | - | x=538 y=123 w=561 h=12 | x=538 y=123 w=561 h=12 | left edge at 538 px; nearest shared grid column 680 px (delta 142 px) |
| A1 | accept-as-legacy | - | x=695 y=24 w=37 h=19 | x=695 y=24 w=37 h=19 | left edge at 695 px; nearest shared grid column 688 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=779 y=205 w=23 h=12 | x=779 y=205 w=23 h=12 | left edge at 779 px; nearest shared grid column 755 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=1109 y=123 w=28 h=12 | x=1109 y=123 w=28 h=12 | left edge at 1109 px; nearest shared grid column 755 px (delta 354 px) |
| A1 | accept-as-legacy | - | x=1115 y=123 w=22 h=12 | x=1115 y=123 w=22 h=12 | left edge at 1115 px; nearest shared grid column 755 px (delta 360 px) |
| A1 | accept-as-legacy | - | x=1367 y=123 w=121 h=12 | x=1367 y=123 w=121 h=12 | left edge at 1367 px; nearest shared grid column 755 px (delta 612 px) |
| A2 | accept-as-legacy | - | x=0 y=10 w=1500 h=2 | x=0 y=10 w=1500 h=2 | gap 2 px vs section median 11 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=13 w=1500 h=1 | x=0 y=13 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=16 w=1500 h=1 | x=0 y=16 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1500 h=2 | x=0 y=19 w=1500 h=2 | gap 2 px vs section median 11 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1500 h=1 | x=0 y=23 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1500 h=31 | x=0 y=56 w=1500 h=31 | gap 31 px vs section median 11 px (delta 20 px) |
| A2 | fix-later | - | x=0 y=96 w=1500 h=16 | x=0 y=96 w=1500 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=135 w=1500 h=8 | x=0 y=135 w=1500 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=145 w=1500 h=23 | x=0 y=145 w=1500 h=23 | gap 23 px vs section median 11 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1500 h=28 | x=0 y=177 w=1500 h=28 | gap 28 px vs section median 11 px (delta 17 px) |
| A2 | accept-as-legacy | - | x=0 y=217 w=1500 h=1 | x=0 y=217 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=241 w=1500 h=1 | x=0 y=241 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=243 w=1500 h=17 | x=0 y=243 w=1500 h=17 | gap 17 px vs section median 11 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=269 w=1500 h=19 | x=0 y=269 w=1500 h=19 | gap 19 px vs section median 11 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=298 w=1500 h=86 | x=0 y=298 w=1500 h=86 | gap 86 px vs section median 11 px (delta 75 px) |

### `capture-overlay-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=155 y=232 w=48 h=12 | x=155 y=232 w=48 h=12 | left edge at 155 px; nearest shared grid column 229 px (delta 74 px) |
| A1 | accept-as-legacy | - | x=211 y=123 w=16 h=12 | x=211 y=123 w=16 h=12 | left edge at 211 px; nearest shared grid column 229 px (delta 18 px) |
| A1 | accept-as-legacy | - | x=329 y=205 w=23 h=16 | x=329 y=205 w=23 h=16 | left edge at 329 px; nearest shared grid column 305 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=349 y=232 w=12 h=12 | x=349 y=232 w=12 h=12 | left edge at 349 px; nearest shared grid column 305 px (delta 44 px) |
| A1 | accept-as-legacy | - | x=542 y=123 w=46 h=12 | x=542 y=123 w=46 h=12 | left edge at 542 px; nearest shared grid column 462 px (delta 80 px) |
| A2 | accept-as-legacy | - | x=0 y=11 w=600 h=7 | x=0 y=11 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=600 h=2 | x=0 y=19 w=600 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=600 h=1 | x=0 y=23 w=600 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=43 w=600 h=10 | x=0 y=43 w=600 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=600 h=28 | x=0 y=56 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=115 w=600 h=8 | x=0 y=115 w=600 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=135 w=600 h=7 | x=0 y=135 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=145 w=600 h=23 | x=0 y=145 w=600 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=600 h=28 | x=0 y=177 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=221 w=600 h=11 | x=0 y=221 w=600 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=269 w=600 h=19 | x=0 y=269 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=298 w=600 h=86 | x=0 y=298 w=600 h=86 | gap 86 px vs section median 16 px (delta 70 px) |
| T1 | accept-as-legacy | - | x=0 y=8 w=600 h=3 | x=0 y=8 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=53 w=600 h=3 | x=0 y=53 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=600 h=3 | x=0 y=112 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=600 h=3 | x=0 y=142 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `confirm-strip-danger-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | chrome-speck | x=0 y=78 w=1 h=3 | x=0 y=78 w=1 h=3 | content run touches the left edge (extent 3 px along the edge) — sub-severing extent 3 px at the chrome scale with no track-column signature (near-floor band 44 px = 5% of the edge) |
| C1 | accept-as-legacy | thumb | x=0 y=108 w=1 h=4 | x=0 y=108 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) — extent 4 px inside the thumb band with a track-column signature (contiguous near-floor band 779 px = 87% of the edge through the run) |
| A1 | accept-as-legacy | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 322 px (delta 306 px) |
| A1 | accept-as-legacy | - | x=907 y=160 w=13 h=40 | x=907 y=160 w=13 h=40 | left edge at 907 px; nearest shared grid column 567 px (delta 340 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=19 | x=0 y=56 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=112 w=1000 h=6 | x=0 y=112 w=1000 h=6 | gap 6 px vs section median 16 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=200 w=1000 h=4 | x=0 y=200 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |

### `confirm-strip-success-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=108 w=1 h=4 | x=0 y=108 w=1 h=4 | content run touches the left edge (extent 4 px along the edge) — extent 4 px inside the thumb band with a track-column signature (contiguous near-floor band 796 px = 88% of the edge through the run) |
| A1 | accept-as-legacy | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 534 px (delta 518 px) |
| A1 | accept-as-legacy | - | x=207 y=74 w=71 h=13 | x=207 y=74 w=71 h=13 | left edge at 207 px; nearest shared grid column 534 px (delta 327 px) |
| A1 | accept-as-legacy | - | x=254 y=74 w=24 h=13 | x=254 y=74 w=24 h=13 | left edge at 254 px; nearest shared grid column 534 px (delta 280 px) |
| A1 | accept-as-legacy | - | x=292 y=74 w=16 h=13 | x=292 y=74 w=16 h=13 | left edge at 292 px; nearest shared grid column 534 px (delta 242 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 534 px (delta 189 px) |
| A1 | accept-as-legacy | - | x=391 y=74 w=54 h=13 | x=391 y=74 w=54 h=13 | left edge at 391 px; nearest shared grid column 534 px (delta 143 px) |
| A1 | accept-as-legacy | - | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 534 px (delta 110 px) |
| A1 | accept-as-legacy | - | x=514 y=74 w=54 h=13 | x=514 y=74 w=54 h=13 | left edge at 514 px; nearest shared grid column 534 px (delta 20 px) |
| A1 | accept-as-legacy | - | x=549 y=74 w=19 h=13 | x=549 y=74 w=19 h=13 | left edge at 549 px; nearest shared grid column 535 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=690 y=74 w=38 h=13 | x=690 y=74 w=38 h=13 | left edge at 690 px; nearest shared grid column 567 px (delta 123 px) |
| A1 | accept-as-legacy | - | x=907 y=160 w=13 h=40 | x=907 y=160 w=13 h=40 | left edge at 907 px; nearest shared grid column 567 px (delta 340 px) |
| A2 | accept-as-legacy | - | x=0 y=112 w=1000 h=6 | x=0 y=112 w=1000 h=6 | gap 6 px vs section median 16 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=200 w=1000 h=4 | x=0 y=200 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |

### `due-editor-datepicker-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=144 w=490 h=48 | x=12 y=144 w=490 h=48 | left edge at 12 px; nearest shared grid column 9 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=196 w=19 h=272 | x=74 y=196 w=19 h=272 | left edge at 74 px; nearest shared grid column 67 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=135 y=196 w=20 h=272 | x=135 y=196 w=20 h=272 | left edge at 135 px; nearest shared grid column 100 px (delta 35 px) |
| A1 | accept-as-legacy | - | x=221 y=196 w=15 h=272 | x=221 y=196 w=15 h=272 | left edge at 221 px; nearest shared grid column 234 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 246 px (delta 99 px) |
| A1 | accept-as-legacy | - | x=372 y=71 w=60 h=20 | x=372 y=71 w=60 h=20 | left edge at 372 px; nearest shared grid column 492 px (delta 120 px) |
| A1 | accept-as-legacy | - | x=419 y=71 w=13 h=20 | x=419 y=71 w=13 h=20 | left edge at 419 px; nearest shared grid column 492 px (delta 73 px) |
| A1 | accept-as-legacy | - | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 492 px (delta 68 px) |
| A1 | accept-as-legacy | - | x=514 y=144 w=91 h=48 | x=514 y=144 w=91 h=48 | left edge at 514 px; nearest shared grid column 494 px (delta 20 px) |
| A1 | accept-as-legacy | - | x=546 y=71 w=19 h=20 | x=546 y=71 w=19 h=20 | left edge at 546 px; nearest shared grid column 494 px (delta 52 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=15 | x=0 y=56 w=1000 h=15 | gap 15 px vs section median 7 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 7 px (delta 13 px) |
| A2 | fix-later | - | x=0 y=192 w=1000 h=4 | x=0 y=192 w=1000 h=4 | gap 4 px vs section median 7 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=485 w=1000 h=3 | x=0 y=485 w=1000 h=3 | gap 3 px vs section median 7 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 7 px (delta 3 px) |

### `focus-overlay-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=220 y=8 w=14 h=892 | x=220 y=8 w=14 h=892 | left edge at 220 px; nearest shared grid column 38 px (delta 182 px) |
| A1 | accept-as-legacy | - | x=268 y=8 w=19 h=892 | x=268 y=8 w=19 h=892 | left edge at 268 px; nearest shared grid column 38 px (delta 230 px) |
| A1 | accept-as-legacy | - | x=433 y=8 w=44 h=892 | x=433 y=8 w=44 h=892 | left edge at 433 px; nearest shared grid column 38 px (delta 395 px) |
| A1 | accept-as-legacy | - | x=465 y=8 w=12 h=892 | x=465 y=8 w=12 h=892 | left edge at 465 px; nearest shared grid column 38 px (delta 427 px) |
| A1 | accept-as-legacy | - | x=551 y=8 w=12 h=892 | x=551 y=8 w=12 h=892 | left edge at 551 px; nearest shared grid column 38 px (delta 513 px) |
| A1 | accept-as-legacy | - | x=596 y=8 w=38 h=892 | x=596 y=8 w=38 h=892 | left edge at 596 px; nearest shared grid column 38 px (delta 558 px) |
| A1 | accept-as-legacy | - | x=609 y=8 w=25 h=892 | x=609 y=8 w=25 h=892 | left edge at 609 px; nearest shared grid column 38 px (delta 571 px) |
| A1 | accept-as-legacy | - | x=766 y=8 w=14 h=892 | x=766 y=8 w=14 h=892 | left edge at 766 px; nearest shared grid column 38 px (delta 728 px) |
| A1 | accept-as-legacy | - | x=909 y=8 w=31 h=892 | x=909 y=8 w=31 h=892 | left edge at 909 px; nearest shared grid column 38 px (delta 871 px) |
| A1 | accept-as-legacy | - | x=914 y=8 w=26 h=892 | x=914 y=8 w=26 h=892 | left edge at 914 px; nearest shared grid column 38 px (delta 876 px) |

### `icon-probe-0.5-396`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| M1 | accept-as-legacy | - | x=0 y=26 w=392 h=1 | x=0 y=52 w=784 h=2 | top margin 52.0 logical px (26 px at scale 0.5); nearest rung 48 px |
| M1 | accept-as-legacy | - | x=0 y=392 w=392 h=1 | x=0 y=784 w=784 h=2 | bottom margin 54.0 logical px (27 px at scale 0.5); nearest rung 48 px |
| M1 | accept-as-legacy | - | x=17 y=0 w=1 h=420 | x=34 y=0 w=2 h=840 | left margin 34.0 logical px (17 px at scale 0.5); nearest rung 32 px |
| M1 | accept-as-legacy | - | x=370 y=0 w=1 h=420 | x=740 y=0 w=2 h=840 | right margin 42.0 logical px (21 px at scale 0.5); nearest rung 48 px |
| A1 | accept-as-legacy | - | x=104 y=60 w=44 h=44 | x=208 y=120 w=88 h=88 | left edge at 104 px; nearest shared grid column 75 px (delta 29 px) |
| A1 | accept-as-legacy | - | x=123 y=387 w=6 h=6 | x=246 y=774 w=12 h=12 | left edge at 123 px; nearest shared grid column 75 px (delta 48 px) |
| A1 | accept-as-legacy | - | x=152 y=60 w=44 h=44 | x=304 y=120 w=88 h=88 | left edge at 152 px; nearest shared grid column 171 px (delta 19 px) |
| A2 | fix-later | - | x=0 y=104 w=392 h=19 | x=0 y=208 w=784 h=38 | gap 19 px vs section median 27 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=143 w=392 h=42 | x=0 y=286 w=784 h=84 | gap 42 px vs section median 27 px (delta 15 px) |
| A2 | accept-as-legacy | - | x=0 y=249 w=392 h=18 | x=0 y=498 w=784 h=36 | gap 18 px vs section median 42 px (delta 24 px) |
| A2 | accept-as-legacy | - | x=0 y=273 w=392 h=18 | x=0 y=546 w=784 h=36 | gap 18 px vs section median 42 px (delta 24 px) |
| T1 | accept-as-legacy | - | x=0 y=185 w=392 h=3 | x=0 y=370 w=784 h=6 | text line height 3 px → em ≈ 8.6 logical px; nearest ladder rung 11 px |

### `icon-probe-1.0-396`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| M1 | accept-as-legacy | - | x=0 y=26 w=392 h=1 | x=0 y=26 w=392 h=1 | top margin 26.0 logical px (26 px at scale 1.0); nearest rung 24 px |
| M1 | accept-as-legacy | - | x=371 y=0 w=1 h=420 | x=371 y=0 w=1 h=420 | right margin 20.0 logical px (20 px at scale 1.0); nearest rung 16 px |
| A1 | accept-as-legacy | - | x=104 y=60 w=44 h=44 | x=104 y=60 w=44 h=44 | left edge at 104 px; nearest shared grid column 73 px (delta 31 px) |
| A1 | accept-as-legacy | - | x=152 y=60 w=44 h=44 | x=152 y=60 w=44 h=44 | left edge at 152 px; nearest shared grid column 73 px (delta 79 px) |
| A2 | accept-as-legacy | - | x=0 y=33 w=392 h=27 | x=0 y=33 w=392 h=27 | gap 27 px vs section median 36 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=104 w=392 h=19 | x=0 y=104 w=392 h=19 | gap 19 px vs section median 36 px (delta 17 px) |
| A2 | fix-later | - | x=0 y=143 w=392 h=42 | x=0 y=143 w=392 h=42 | gap 42 px vs section median 36 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=188 w=392 h=52 | x=0 y=188 w=392 h=52 | gap 52 px vs section median 36 px (delta 16 px) |
| A2 | accept-as-legacy | - | x=0 y=252 w=392 h=13 | x=0 y=252 w=392 h=13 | gap 13 px vs section median 36 px (delta 23 px) |
| A2 | accept-as-legacy | - | x=0 y=275 w=392 h=13 | x=0 y=275 w=392 h=13 | gap 13 px vs section median 36 px (delta 23 px) |
| T1 | accept-as-legacy | - | x=0 y=185 w=392 h=3 | x=0 y=185 w=392 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `icon-probe-3.0-396`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=104 y=60 w=44 h=44 | x=35 y=20 w=15 h=15 | left edge at 104 px; nearest shared grid column 63 px (delta 41 px) |
| A1 | accept-as-legacy | - | x=152 y=60 w=44 h=44 | x=51 y=20 w=15 h=15 | left edge at 152 px; nearest shared grid column 63 px (delta 89 px) |
| A1 | accept-as-legacy | - | x=162 y=229 w=75 h=82 | x=54 y=76 w=25 h=27 | left edge at 162 px; nearest shared grid column 63 px (delta 99 px) |
| A2 | fix-later | - | x=0 y=104 w=392 h=19 | x=0 y=35 w=131 h=6 | gap 19 px vs section median 27 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=143 w=392 h=42 | x=0 y=48 w=131 h=14 | gap 42 px vs section median 27 px (delta 15 px) |
| A2 | accept-as-legacy | - | x=0 y=188 w=392 h=41 | x=0 y=63 w=131 h=14 | gap 41 px vs section median 27 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=311 w=392 h=14 | x=0 y=104 w=131 h=5 | gap 14 px vs section median 27 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=359 w=392 h=13 | x=0 y=120 w=131 h=4 | gap 13 px vs section median 27 px (delta 14 px) |
| T1 | accept-as-legacy | - | x=0 y=26 w=392 h=7 | x=0 y=9 w=131 h=2 | text line height 7 px → em ≈ 3.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=185 w=392 h=3 | x=0 y=62 w=131 h=1 | text line height 3 px → em ≈ 1.4 logical px; nearest ladder rung 11 px |

### `kiosk-board-0.5-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | chrome-speck | x=0 y=68 w=1 h=3 | x=0 y=136 w=2 h=6 | content run touches the left edge (extent 3 px along the edge) — sub-severing extent 3 px at the chrome scale with no track-column signature (near-floor band 40 px = 4% of the edge) |
| A1 | accept-as-legacy | - | x=43 y=881 w=6 h=15 | x=86 y=1762 w=12 h=30 | left edge at 43 px; nearest shared grid column 29 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=53 y=881 w=6 h=15 | x=106 y=1762 w=12 h=30 | left edge at 53 px; nearest shared grid column 29 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=193 y=78 w=7 h=12 | x=386 y=156 w=14 h=24 | left edge at 193 px; nearest shared grid column 29 px (delta 164 px) |
| A1 | accept-as-legacy | - | x=202 y=78 w=6 h=12 | x=404 y=156 w=12 h=24 | left edge at 202 px; nearest shared grid column 29 px (delta 173 px) |
| A1 | accept-as-legacy | - | x=215 y=78 w=7 h=12 | x=430 y=156 w=14 h=24 | left edge at 215 px; nearest shared grid column 29 px (delta 186 px) |
| A1 | accept-as-legacy | - | x=424 y=8 w=55 h=48 | x=848 y=16 w=110 h=96 | left edge at 424 px; nearest shared grid column 487 px (delta 63 px) |
| A1 | accept-as-legacy | - | x=514 y=8 w=6 h=48 | x=1028 y=16 w=12 h=96 | left edge at 514 px; nearest shared grid column 488 px (delta 26 px) |
| A1 | accept-as-legacy | - | x=522 y=8 w=15 h=48 | x=1044 y=16 w=30 h=96 | left edge at 522 px; nearest shared grid column 549 px (delta 27 px) |
| A1 | accept-as-legacy | - | x=961 y=8 w=10 h=48 | x=1922 y=16 w=20 h=96 | left edge at 961 px; nearest shared grid column 998 px (delta 37 px) |
| A2 | fix-later | - | x=0 y=90 w=1000 h=13 | x=0 y=180 w=2000 h=26 | gap 13 px vs section median 8 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=104 w=1000 h=16 | x=0 y=208 w=2000 h=32 | gap 16 px vs section median 8 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=160 w=1000 h=4 | x=0 y=320 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |

### `kiosk-today-0.5-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=90 y=813 w=7 h=16 | x=180 y=1626 w=14 h=32 | left edge at 90 px; nearest shared grid column 81 px (delta 9 px) |
| A1 | accept-as-legacy | - | x=110 y=813 w=6 h=16 | x=220 y=1626 w=12 h=32 | left edge at 110 px; nearest shared grid column 117 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=145 y=115 w=6 h=12 | x=290 y=230 w=12 h=24 | left edge at 145 px; nearest shared grid column 130 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=152 y=115 w=9 h=12 | x=304 y=230 w=18 h=24 | left edge at 152 px; nearest shared grid column 172 px (delta 20 px) |
| A1 | fix-later | - | x=190 y=115 w=12 h=12 | x=380 y=230 w=24 h=24 | left edge at 190 px; nearest shared grid column 196 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=208 y=813 w=7 h=16 | x=416 y=1626 w=14 h=32 | left edge at 208 px; nearest shared grid column 197 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=327 y=813 w=6 h=16 | x=654 y=1626 w=12 h=32 | left edge at 327 px; nearest shared grid column 339 px (delta 12 px) |
| A1 | fix-later | - | x=343 y=115 w=9 h=12 | x=686 y=230 w=18 h=24 | left edge at 343 px; nearest shared grid column 340 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=352 y=813 w=7 h=16 | x=704 y=1626 w=14 h=32 | left edge at 352 px; nearest shared grid column 340 px (delta 12 px) |
| A1 | fix-later | - | x=360 y=813 w=7 h=16 | x=720 y=1626 w=14 h=32 | left edge at 360 px; nearest shared grid column 364 px (delta 4 px) |
| A1 | fix-later | - | x=375 y=115 w=6 h=12 | x=750 y=230 w=12 h=24 | left edge at 375 px; nearest shared grid column 369 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=406 y=115 w=6 h=12 | x=812 y=230 w=12 h=24 | left edge at 406 px; nearest shared grid column 393 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=408 y=813 w=6 h=16 | x=816 y=1626 w=12 h=32 | left edge at 408 px; nearest shared grid column 393 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=420 y=813 w=7 h=16 | x=840 y=1626 w=14 h=32 | left edge at 420 px; nearest shared grid column 429 px (delta 9 px) |
| A1 | accept-as-legacy | - | x=452 y=115 w=8 h=12 | x=904 y=230 w=16 h=24 | left edge at 452 px; nearest shared grid column 462 px (delta 10 px) |
| A1 | accept-as-legacy | - | x=512 y=115 w=14 h=12 | x=1024 y=230 w=28 h=24 | left edge at 512 px; nearest shared grid column 493 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=961 y=8 w=10 h=48 | x=1922 y=16 w=20 h=96 | left edge at 961 px; nearest shared grid column 969 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=8 | x=0 y=112 w=2000 h=16 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=71 w=1000 h=13 | x=0 y=142 w=2000 h=26 | gap 13 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=96 w=1000 h=19 | x=0 y=192 w=2000 h=38 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=182 w=1000 h=25 | x=0 y=364 w=2000 h=50 | gap 25 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=217 w=1000 h=6 | x=0 y=434 w=2000 h=12 | gap 6 px vs section median 16 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=273 w=1000 h=9 | x=0 y=546 w=2000 h=18 | gap 9 px vs section median 16 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=801 w=1000 h=12 | x=0 y=1602 w=2000 h=24 | gap 12 px vs section median 18 px (delta 6 px) |

### `labels-overlay-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=129 y=71 w=29 h=20 | x=129 y=71 w=29 h=20 | left edge at 129 px; nearest shared grid column 116 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=135 y=370 w=20 h=121 | x=135 y=370 w=20 h=121 | left edge at 135 px; nearest shared grid column 152 px (delta 17 px) |
| A1 | fix-later | - | x=158 y=370 w=15 h=121 | x=158 y=370 w=15 h=121 | left edge at 158 px; nearest shared grid column 154 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=221 y=370 w=15 h=121 | x=221 y=370 w=15 h=121 | left edge at 221 px; nearest shared grid column 234 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=279 y=370 w=401 h=121 | x=279 y=370 w=401 h=121 | left edge at 279 px; nearest shared grid column 318 px (delta 39 px) |
| A1 | accept-as-legacy | - | x=291 y=370 w=389 h=121 | x=291 y=370 w=389 h=121 | left edge at 291 px; nearest shared grid column 318 px (delta 27 px) |
| A1 | accept-as-legacy | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 320 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=433 y=24 w=38 h=16 | x=433 y=24 w=38 h=16 | left edge at 433 px; nearest shared grid column 492 px (delta 59 px) |
| A1 | accept-as-legacy | - | x=459 y=24 w=12 h=16 | x=459 y=24 w=12 h=16 | left edge at 459 px; nearest shared grid column 492 px (delta 33 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 13 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 13 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 13 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 13 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=137 w=1000 h=19 | x=0 y=137 w=1000 h=19 | gap 19 px vs section median 13 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=167 w=1000 h=4 | x=0 y=167 w=1000 h=4 | gap 4 px vs section median 13 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=174 w=1000 h=6 | x=0 y=174 w=1000 h=6 | gap 6 px vs section median 13 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=182 w=1000 h=33 | x=0 y=182 w=1000 h=33 | gap 33 px vs section median 13 px (delta 20 px) |
| A2 | accept-as-legacy | - | x=0 y=227 w=1000 h=33 | x=0 y=227 w=1000 h=33 | gap 33 px vs section median 13 px (delta 20 px) |
| A2 | accept-as-legacy | - | x=0 y=269 w=1000 h=35 | x=0 y=269 w=1000 h=35 | gap 35 px vs section median 13 px (delta 22 px) |
| A2 | accept-as-legacy | - | x=0 y=305 w=1000 h=2 | x=0 y=305 w=1000 h=2 | gap 2 px vs section median 13 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=309 w=1000 h=2 | x=0 y=309 w=1000 h=2 | gap 2 px vs section median 13 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=312 w=1000 h=28 | x=0 y=312 w=1000 h=28 | gap 28 px vs section median 13 px (delta 15 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=171 w=1000 h=3 | x=0 y=171 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | fix-later | - | x=0 y=340 w=1000 h=17 | x=0 y=340 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |

### `notification-3.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | fix-now | real-clip | x=999 y=209 w=1 h=133 | x=333 y=70 w=0 h=44 | content run touches the right edge (extent 133 px along the edge) — extent 133 px at/above the 12 px severing floor |
| C1 | accept-as-legacy | thumb | x=999 y=429 w=1 h=3 | x=333 y=143 w=0 h=1 | content run touches the right edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 351 px = 39% of the edge through the run) |
| C1 | fix-now | real-clip | x=0 y=209 w=1 h=206 | x=0 y=70 w=0 h=69 | content run touches the left edge (extent 206 px along the edge) — extent 206 px at/above the 12 px severing floor |
| A1 | accept-as-legacy | - | x=225 y=192 w=99 h=240 | x=75 y=64 w=33 h=80 | left edge at 225 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=337 y=24 w=111 h=144 | x=112 y=8 w=37 h=48 | left edge at 337 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=348 y=192 w=59 h=240 | x=116 y=64 w=20 h=80 | left edge at 348 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=475 y=24 w=170 h=144 | x=158 y=8 w=57 h=48 | left edge at 475 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=573 y=792 w=211 h=96 | x=191 y=264 w=70 h=32 | left edge at 573 px; no other left edge shares this column |
| A1 | accept-as-legacy | - | x=660 y=192 w=192 h=240 | x=220 y=64 w=64 h=80 | left edge at 660 px; no other left edge shares this column |
| A2 | accept-as-legacy | - | x=0 y=168 w=1000 h=24 | x=0 y=56 w=333 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | - | x=0 y=432 w=1000 h=37 | x=0 y=144 w=333 h=12 | gap 37 px vs section median 56 px (delta 19 px) |
| A2 | fix-later | - | x=0 y=513 w=1000 h=64 | x=0 y=171 w=333 h=21 | gap 64 px vs section median 56 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=621 w=1000 h=76 | x=0 y=207 w=333 h=25 | gap 76 px vs section median 56 px (delta 20 px) |

### `plan-next-day-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 24 px (delta 8 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 24 px (delta 321 px) |
| A1 | accept-as-legacy | - | x=433 y=8 w=38 h=48 | x=433 y=8 w=38 h=48 | left edge at 433 px; nearest shared grid column 24 px (delta 409 px) |
| A1 | accept-as-legacy | - | x=459 y=8 w=12 h=48 | x=459 y=8 w=12 h=48 | left edge at 459 px; nearest shared grid column 24 px (delta 435 px) |
| A1 | accept-as-legacy | - | x=480 y=8 w=57 h=48 | x=480 y=8 w=57 h=48 | left edge at 480 px; nearest shared grid column 24 px (delta 456 px) |

### `plan-next-day-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 24 px (delta 8 px) |
| A1 | accept-as-legacy | - | x=512 y=864 w=71 h=32 | x=512 y=864 w=71 h=32 | left edge at 512 px; nearest shared grid column 24 px (delta 488 px) |
| A1 | accept-as-legacy | - | x=683 y=8 w=38 h=48 | x=683 y=8 w=38 h=48 | left edge at 683 px; nearest shared grid column 24 px (delta 659 px) |
| A1 | accept-as-legacy | - | x=709 y=8 w=12 h=48 | x=709 y=8 w=12 h=48 | left edge at 709 px; nearest shared grid column 24 px (delta 685 px) |
| A1 | accept-as-legacy | - | x=730 y=8 w=57 h=48 | x=730 y=8 w=57 h=48 | left edge at 730 px; nearest shared grid column 24 px (delta 706 px) |
| A2 | fix-later | - | x=0 y=848 w=1500 h=7 | x=0 y=848 w=1500 h=7 | gap 7 px vs section median 4 px (delta 3 px) |
| T1 | accept-as-legacy | - | x=0 y=64 w=1500 h=5 | x=0 y=64 w=1500 h=5 | text line height 5 px → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=855 w=1500 h=5 | x=0 y=855 w=1500 h=5 | text line height 5 px → em ≈ 7.1 logical px; nearest ladder rung 11 px |

### `plan-next-day-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 24 px (delta 8 px) |
| A1 | fix-later | - | x=29 y=64 w=12 h=796 | x=29 y=64 w=12 h=796 | left edge at 29 px; nearest shared grid column 26 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=53 y=64 w=530 h=796 | x=53 y=64 w=530 h=796 | left edge at 53 px; nearest shared grid column 26 px (delta 27 px) |
| A1 | accept-as-legacy | - | x=60 y=64 w=523 h=796 | x=60 y=64 w=523 h=796 | left edge at 60 px; nearest shared grid column 26 px (delta 34 px) |
| A1 | accept-as-legacy | - | x=212 y=864 w=71 h=32 | x=212 y=864 w=71 h=32 | left edge at 212 px; nearest shared grid column 26 px (delta 186 px) |
| A1 | accept-as-legacy | - | x=436 y=8 w=38 h=48 | x=436 y=8 w=38 h=48 | left edge at 436 px; nearest shared grid column 26 px (delta 410 px) |
| A1 | accept-as-legacy | - | x=462 y=8 w=12 h=48 | x=462 y=8 w=12 h=48 | left edge at 462 px; nearest shared grid column 26 px (delta 436 px) |
| A1 | accept-as-legacy | - | x=483 y=8 w=57 h=48 | x=483 y=8 w=57 h=48 | left edge at 483 px; nearest shared grid column 26 px (delta 457 px) |

### `recur-editor-detach-armed-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=129 y=71 w=29 h=20 | x=129 y=71 w=29 h=20 | left edge at 129 px; nearest shared grid column 116 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=135 y=286 w=20 h=289 | x=135 y=286 w=20 h=289 | left edge at 135 px; nearest shared grid column 152 px (delta 17 px) |
| A1 | fix-later | - | x=158 y=286 w=15 h=289 | x=158 y=286 w=15 h=289 | left edge at 158 px; nearest shared grid column 154 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=220 y=286 w=151 h=289 | x=220 y=286 w=151 h=289 | left edge at 220 px; nearest shared grid column 172 px (delta 48 px) |
| A1 | accept-as-legacy | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 451 px (delta 85 px) |
| A1 | accept-as-legacy | - | x=433 y=24 w=38 h=16 | x=433 y=24 w=38 h=16 | left edge at 433 px; nearest shared grid column 451 px (delta 18 px) |
| A1 | accept-as-legacy | - | x=590 y=286 w=14 h=289 | x=590 y=286 w=14 h=289 | left edge at 590 px; nearest shared grid column 601 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=669 y=286 w=111 h=289 | x=669 y=286 w=111 h=289 | left edge at 669 px; nearest shared grid column 614 px (delta 55 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 15 px (delta 14 px) |
| A2 | fix-later | - | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 15 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 15 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=137 w=1000 h=19 | x=0 y=137 w=1000 h=19 | gap 19 px vs section median 15 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=167 w=1000 h=4 | x=0 y=167 w=1000 h=4 | gap 4 px vs section median 15 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=174 w=1000 h=6 | x=0 y=174 w=1000 h=6 | gap 6 px vs section median 15 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=182 w=1000 h=33 | x=0 y=182 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| A2 | accept-as-legacy | - | x=0 y=227 w=1000 h=33 | x=0 y=227 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=171 w=1000 h=3 | x=0 y=171 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `recur-editor-modal-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=129 y=71 w=29 h=20 | x=129 y=71 w=29 h=20 | left edge at 129 px; nearest shared grid column 116 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=135 y=286 w=20 h=289 | x=135 y=286 w=20 h=289 | left edge at 135 px; nearest shared grid column 152 px (delta 17 px) |
| A1 | fix-later | - | x=158 y=286 w=15 h=289 | x=158 y=286 w=15 h=289 | left edge at 158 px; nearest shared grid column 154 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=220 y=286 w=69 h=289 | x=220 y=286 w=69 h=289 | left edge at 220 px; nearest shared grid column 172 px (delta 48 px) |
| A1 | fix-later | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 372 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=384 y=286 w=14 h=289 | x=384 y=286 w=14 h=289 | left edge at 384 px; nearest shared grid column 372 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=408 y=286 w=372 h=289 | x=408 y=286 w=372 h=289 | left edge at 408 px; nearest shared grid column 372 px (delta 36 px) |
| A1 | accept-as-legacy | - | x=422 y=286 w=358 h=289 | x=422 y=286 w=358 h=289 | left edge at 422 px; nearest shared grid column 372 px (delta 50 px) |
| A1 | accept-as-legacy | - | x=433 y=24 w=38 h=16 | x=433 y=24 w=38 h=16 | left edge at 433 px; nearest shared grid column 492 px (delta 59 px) |
| A1 | accept-as-legacy | - | x=459 y=24 w=12 h=16 | x=459 y=24 w=12 h=16 | left edge at 459 px; nearest shared grid column 492 px (delta 33 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 15 px (delta 14 px) |
| A2 | fix-later | - | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 15 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 15 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=137 w=1000 h=19 | x=0 y=137 w=1000 h=19 | gap 19 px vs section median 15 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=167 w=1000 h=4 | x=0 y=167 w=1000 h=4 | gap 4 px vs section median 15 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=174 w=1000 h=6 | x=0 y=174 w=1000 h=6 | gap 6 px vs section median 15 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=182 w=1000 h=33 | x=0 y=182 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| A2 | accept-as-legacy | - | x=0 y=227 w=1000 h=33 | x=0 y=227 w=1000 h=33 | gap 33 px vs section median 15 px (delta 18 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=171 w=1000 h=3 | x=0 y=171 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `review-empty-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=54 y=8 w=18 h=48 | x=54 y=8 w=18 h=48 | left edge at 54 px; nearest shared grid column 17 px (delta 37 px) |
| A1 | accept-as-legacy | - | x=413 y=480 w=169 h=12 | x=413 y=480 w=169 h=12 | left edge at 413 px; nearest shared grid column 446 px (delta 33 px) |
| A1 | accept-as-legacy | - | x=434 y=8 w=56 h=48 | x=434 y=8 w=56 h=48 | left edge at 434 px; nearest shared grid column 446 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=439 y=480 w=143 h=12 | x=439 y=480 w=143 h=12 | left edge at 439 px; nearest shared grid column 446 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=466 y=453 w=19 h=16 | x=466 y=453 w=19 h=16 | left edge at 466 px; nearest shared grid column 447 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=536 y=453 w=24 h=16 | x=536 y=453 w=24 h=16 | left edge at 536 px; nearest shared grid column 545 px (delta 9 px) |

### `review-ledger-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=207 y=821 w=24 h=17 | x=207 y=821 w=24 h=17 | left edge at 207 px; nearest shared grid column 202 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=261 y=821 w=14 h=17 | x=261 y=821 w=14 h=17 | left edge at 261 px; nearest shared grid column 241 px (delta 20 px) |
| A1 | accept-as-legacy | - | x=334 y=821 w=29 h=17 | x=334 y=821 w=29 h=17 | left edge at 334 px; nearest shared grid column 345 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=434 y=8 w=56 h=48 | x=434 y=8 w=56 h=48 | left edge at 434 px; nearest shared grid column 451 px (delta 17 px) |
| A1 | fix-later | - | x=457 y=821 w=121 h=17 | x=457 y=821 w=121 h=17 | left edge at 457 px; nearest shared grid column 451 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=741 y=821 w=39 h=17 | x=741 y=821 w=39 h=17 | left edge at 741 px; nearest shared grid column 732 px (delta 9 px) |
| A1 | accept-as-legacy | - | x=923 y=420 w=29 h=48 | x=923 y=420 w=29 h=48 | left edge at 923 px; nearest shared grid column 956 px (delta 33 px) |
| A2 | fix-later | - | x=0 y=136 w=1000 h=16 | x=0 y=136 w=1000 h=16 | gap 16 px vs section median 8 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=208 w=1000 h=15 | x=0 y=208 w=1000 h=15 | gap 15 px vs section median 8 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=336 w=1000 h=19 | x=0 y=336 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=468 w=1000 h=19 | x=0 y=468 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=563 w=1000 h=27 | x=0 y=563 w=1000 h=27 | gap 27 px vs section median 8 px (delta 19 px) |
| A2 | accept-as-legacy | - | x=0 y=619 w=1000 h=19 | x=0 y=619 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=714 w=1000 h=27 | x=0 y=714 w=1000 h=27 | gap 27 px vs section median 8 px (delta 19 px) |
| A2 | accept-as-legacy | - | x=0 y=770 w=1000 h=19 | x=0 y=770 w=1000 h=19 | gap 19 px vs section median 8 px (delta 11 px) |
| A2 | accept-as-legacy | - | x=0 y=803 w=1000 h=18 | x=0 y=803 w=1000 h=18 | gap 18 px vs section median 8 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=838 w=1000 h=1 | x=0 y=838 w=1000 h=1 | gap 1 px vs section median 8 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=852 w=1000 h=1 | x=0 y=852 w=1000 h=1 | gap 1 px vs section median 8 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 8 px (delta 4 px) |
| T1 | fix-later | - | x=0 y=223 w=1000 h=17 | x=0 y=223 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | - | x=0 y=355 w=1000 h=17 | x=0 y=355 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | - | x=0 y=487 w=1000 h=17 | x=0 y=487 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | - | x=0 y=638 w=1000 h=17 | x=0 y=638 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | - | x=0 y=821 w=1000 h=17 | x=0 y=821 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |

### `review-walk-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | fix-later | - | x=41 y=350 w=17 h=152 | x=41 y=350 w=17 h=152 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 64 px (delta 10 px) |
| A1 | accept-as-legacy | - | x=220 y=350 w=560 h=152 | x=220 y=350 w=560 h=152 | left edge at 220 px; nearest shared grid column 358 px (delta 138 px) |
| A1 | accept-as-legacy | - | x=227 y=231 w=66 h=12 | x=227 y=231 w=66 h=12 | left edge at 227 px; nearest shared grid column 358 px (delta 131 px) |
| A1 | accept-as-legacy | - | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 358 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 359 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 359 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=388 y=231 w=600 h=12 | x=388 y=231 w=600 h=12 | left edge at 388 px; nearest shared grid column 359 px (delta 29 px) |
| A1 | fix-later | - | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=445 y=24 w=38 h=19 | x=445 y=24 w=38 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=529 y=267 w=23 h=16 | x=529 y=267 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 561 px (delta 173 px) |
| A1 | accept-as-legacy | - | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 561 px (delta 179 px) |
| A1 | accept-as-legacy | - | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 986 px (delta 77 px) |
| A1 | accept-as-legacy | - | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 986 px (delta 72 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 11 px (delta 19 px) |
| A2 | fix-later | - | x=0 y=96 w=1000 h=16 | x=0 y=96 w=1000 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 11 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 11 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1000 h=24 | x=0 y=177 w=1000 h=24 | gap 24 px vs section median 11 px (delta 13 px) |
| A2 | fix-later | - | x=0 y=208 w=1000 h=4 | x=0 y=208 w=1000 h=4 | gap 4 px vs section median 11 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=224 w=1000 h=7 | x=0 y=224 w=1000 h=7 | gap 7 px vs section median 11 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=243 w=1000 h=1 | x=0 y=243 w=1000 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=251 w=1000 h=16 | x=0 y=251 w=1000 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=303 w=1000 h=1 | x=0 y=303 w=1000 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=306 w=1000 h=16 | x=0 y=306 w=1000 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=331 w=1000 h=19 | x=0 y=331 w=1000 h=19 | gap 19 px vs section median 11 px (delta 8 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=1000 h=3 | x=0 y=112 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=1000 h=3 | x=0 y=142 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `review-walk-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | fix-later | - | x=41 y=350 w=17 h=152 | x=41 y=350 w=17 h=152 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 64 px (delta 10 px) |
| A1 | accept-as-legacy | - | x=227 y=231 w=66 h=12 | x=227 y=231 w=66 h=12 | left edge at 227 px; nearest shared grid column 64 px (delta 163 px) |
| A1 | accept-as-legacy | - | x=358 y=231 w=1130 h=12 | x=358 y=231 w=1130 h=12 | left edge at 358 px; nearest shared grid column 64 px (delta 294 px) |
| A1 | accept-as-legacy | - | x=388 y=231 w=1100 h=12 | x=388 y=231 w=1100 h=12 | left edge at 388 px; nearest shared grid column 680 px (delta 292 px) |
| A1 | accept-as-legacy | - | x=470 y=350 w=560 h=152 | x=470 y=350 w=560 h=152 | left edge at 470 px; nearest shared grid column 680 px (delta 210 px) |
| A1 | accept-as-legacy | - | x=511 y=123 w=16 h=12 | x=511 y=123 w=16 h=12 | left edge at 511 px; nearest shared grid column 680 px (delta 169 px) |
| A1 | accept-as-legacy | - | x=529 y=123 w=570 h=12 | x=529 y=123 w=570 h=12 | left edge at 529 px; nearest shared grid column 680 px (delta 151 px) |
| A1 | accept-as-legacy | - | x=538 y=123 w=561 h=12 | x=538 y=123 w=561 h=12 | left edge at 538 px; nearest shared grid column 680 px (delta 142 px) |
| A1 | accept-as-legacy | - | x=695 y=24 w=38 h=19 | x=695 y=24 w=38 h=19 | left edge at 695 px; nearest shared grid column 688 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=779 y=267 w=23 h=12 | x=779 y=267 w=23 h=12 | left edge at 779 px; nearest shared grid column 755 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=1109 y=123 w=28 h=12 | x=1109 y=123 w=28 h=12 | left edge at 1109 px; nearest shared grid column 811 px (delta 298 px) |
| A1 | accept-as-legacy | - | x=1115 y=123 w=22 h=12 | x=1115 y=123 w=22 h=12 | left edge at 1115 px; nearest shared grid column 811 px (delta 304 px) |
| A1 | accept-as-legacy | - | x=1367 y=123 w=121 h=12 | x=1367 y=123 w=121 h=12 | left edge at 1367 px; nearest shared grid column 1486 px (delta 119 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1500 h=1 | x=0 y=23 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1500 h=31 | x=0 y=56 w=1500 h=31 | gap 31 px vs section median 11 px (delta 20 px) |
| A2 | fix-later | - | x=0 y=96 w=1500 h=16 | x=0 y=96 w=1500 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=135 w=1500 h=8 | x=0 y=135 w=1500 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=145 w=1500 h=23 | x=0 y=145 w=1500 h=23 | gap 23 px vs section median 11 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1500 h=24 | x=0 y=177 w=1500 h=24 | gap 24 px vs section median 11 px (delta 13 px) |
| A2 | fix-later | - | x=0 y=206 w=1500 h=6 | x=0 y=206 w=1500 h=6 | gap 6 px vs section median 11 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=221 w=1500 h=1 | x=0 y=221 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=223 w=1500 h=8 | x=0 y=223 w=1500 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=243 w=1500 h=3 | x=0 y=243 w=1500 h=3 | gap 3 px vs section median 11 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=251 w=1500 h=16 | x=0 y=251 w=1500 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=279 w=1500 h=1 | x=0 y=279 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=303 w=1500 h=1 | x=0 y=303 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=305 w=1500 h=17 | x=0 y=305 w=1500 h=17 | gap 17 px vs section median 11 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=331 w=1500 h=19 | x=0 y=331 w=1500 h=19 | gap 19 px vs section median 11 px (delta 8 px) |
| T1 | accept-as-legacy | - | x=0 y=201 w=1500 h=5 | x=0 y=201 w=1500 h=5 | text line height 5 px → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=246 w=1500 h=5 | x=0 y=246 w=1500 h=5 | text line height 5 px → em ≈ 7.1 logical px; nearest ladder rung 11 px |

### `review-walk-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | fix-later | - | x=41 y=350 w=17 h=152 | x=41 y=350 w=17 h=152 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 64 px (delta 10 px) |
| A1 | accept-as-legacy | - | x=155 y=294 w=48 h=12 | x=155 y=294 w=48 h=12 | left edge at 155 px; nearest shared grid column 225 px (delta 70 px) |
| A1 | accept-as-legacy | - | x=211 y=123 w=16 h=12 | x=211 y=123 w=16 h=12 | left edge at 211 px; nearest shared grid column 225 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=329 y=267 w=23 h=16 | x=329 y=267 w=23 h=16 | left edge at 329 px; nearest shared grid column 305 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=349 y=294 w=12 h=12 | x=349 y=294 w=12 h=12 | left edge at 349 px; nearest shared grid column 358 px (delta 9 px) |
| A1 | accept-as-legacy | - | x=388 y=201 w=200 h=50 | x=388 y=201 w=200 h=50 | left edge at 388 px; nearest shared grid column 359 px (delta 29 px) |
| A1 | accept-as-legacy | - | x=542 y=123 w=46 h=12 | x=542 y=123 w=46 h=12 | left edge at 542 px; nearest shared grid column 586 px (delta 44 px) |
| A2 | accept-as-legacy | - | x=0 y=11 w=600 h=7 | x=0 y=11 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=600 h=2 | x=0 y=19 w=600 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=600 h=1 | x=0 y=23 w=600 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=43 w=600 h=10 | x=0 y=43 w=600 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=600 h=28 | x=0 y=56 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=115 w=600 h=8 | x=0 y=115 w=600 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=135 w=600 h=7 | x=0 y=135 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=145 w=600 h=23 | x=0 y=145 w=600 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=177 w=600 h=24 | x=0 y=177 w=600 h=24 | gap 24 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=283 w=600 h=11 | x=0 y=283 w=600 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=331 w=600 h=19 | x=0 y=331 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| T1 | accept-as-legacy | - | x=0 y=8 w=600 h=3 | x=0 y=8 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=53 w=600 h=3 | x=0 y=53 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=600 h=3 | x=0 y=112 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=600 h=3 | x=0 y=142 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `search-palette-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=220 y=317 w=18 h=227 | x=220 y=317 w=18 h=227 | left edge at 220 px; nearest shared grid column 57 px (delta 163 px) |
| A1 | accept-as-legacy | - | x=245 y=317 w=396 h=227 | x=245 y=317 w=396 h=227 | left edge at 245 px; nearest shared grid column 430 px (delta 185 px) |
| A1 | accept-as-legacy | - | x=265 y=317 w=376 h=227 | x=265 y=317 w=376 h=227 | left edge at 265 px; nearest shared grid column 430 px (delta 165 px) |
| A1 | accept-as-legacy | - | x=278 y=317 w=363 h=227 | x=278 y=317 w=363 h=227 | left edge at 278 px; nearest shared grid column 430 px (delta 152 px) |
| A1 | accept-as-legacy | - | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 430 px (delta 86 px) |
| A1 | accept-as-legacy | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 430 px (delta 64 px) |
| A1 | accept-as-legacy | - | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 430 px (delta 59 px) |
| A1 | fix-later | - | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=445 y=24 w=37 h=19 | x=445 y=24 w=37 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=529 y=205 w=23 h=16 | x=529 y=205 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=545 y=317 w=96 h=227 | x=545 y=317 w=96 h=227 | left edge at 545 px; nearest shared grid column 505 px (delta 40 px) |
| A1 | accept-as-legacy | - | x=605 y=317 w=36 h=227 | x=605 y=317 w=36 h=227 | left edge at 605 px; nearest shared grid column 505 px (delta 100 px) |
| A1 | accept-as-legacy | - | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 760 px (delta 26 px) |
| A1 | accept-as-legacy | - | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 760 px (delta 20 px) |
| A1 | accept-as-legacy | - | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 762 px (delta 147 px) |
| A1 | accept-as-legacy | - | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 762 px (delta 152 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=43 w=1000 h=10 | x=0 y=43 w=1000 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 16 px (delta 14 px) |
| A2 | fix-later | - | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1000 h=28 | x=0 y=177 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=221 w=1000 h=11 | x=0 y=221 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=241 w=1000 h=1 | x=0 y=241 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=269 w=1000 h=19 | x=0 y=269 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=298 w=1000 h=19 | x=0 y=298 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=1000 h=3 | x=0 y=112 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=1000 h=3 | x=0 y=142 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `search-palette-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=470 y=317 w=18 h=227 | x=470 y=317 w=18 h=227 | left edge at 470 px; nearest shared grid column 528 px (delta 58 px) |
| A1 | accept-as-legacy | - | x=495 y=317 w=396 h=227 | x=495 y=317 w=396 h=227 | left edge at 495 px; nearest shared grid column 528 px (delta 33 px) |
| A1 | accept-as-legacy | - | x=511 y=123 w=16 h=12 | x=511 y=123 w=16 h=12 | left edge at 511 px; nearest shared grid column 528 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=515 y=317 w=376 h=227 | x=515 y=317 w=376 h=227 | left edge at 515 px; nearest shared grid column 528 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=538 y=123 w=561 h=12 | x=538 y=123 w=561 h=12 | left edge at 538 px; nearest shared grid column 529 px (delta 9 px) |
| A1 | accept-as-legacy | - | x=695 y=24 w=37 h=19 | x=695 y=24 w=37 h=19 | left edge at 695 px; nearest shared grid column 688 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=779 y=205 w=23 h=12 | x=779 y=205 w=23 h=12 | left edge at 779 px; nearest shared grid column 755 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=795 y=317 w=96 h=227 | x=795 y=317 w=96 h=227 | left edge at 795 px; nearest shared grid column 755 px (delta 40 px) |
| A1 | accept-as-legacy | - | x=855 y=317 w=36 h=227 | x=855 y=317 w=36 h=227 | left edge at 855 px; nearest shared grid column 755 px (delta 100 px) |
| A1 | accept-as-legacy | - | x=1012 y=317 w=18 h=227 | x=1012 y=317 w=18 h=227 | left edge at 1012 px; nearest shared grid column 755 px (delta 257 px) |
| A1 | accept-as-legacy | - | x=1109 y=123 w=28 h=12 | x=1109 y=123 w=28 h=12 | left edge at 1109 px; nearest shared grid column 755 px (delta 354 px) |
| A1 | accept-as-legacy | - | x=1115 y=123 w=22 h=12 | x=1115 y=123 w=22 h=12 | left edge at 1115 px; nearest shared grid column 755 px (delta 360 px) |
| A1 | accept-as-legacy | - | x=1367 y=123 w=121 h=12 | x=1367 y=123 w=121 h=12 | left edge at 1367 px; nearest shared grid column 755 px (delta 612 px) |
| A2 | accept-as-legacy | - | x=0 y=10 w=1500 h=2 | x=0 y=10 w=1500 h=2 | gap 2 px vs section median 11 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=13 w=1500 h=1 | x=0 y=13 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=16 w=1500 h=1 | x=0 y=16 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1500 h=2 | x=0 y=19 w=1500 h=2 | gap 2 px vs section median 11 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1500 h=1 | x=0 y=23 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1500 h=31 | x=0 y=56 w=1500 h=31 | gap 31 px vs section median 11 px (delta 20 px) |
| A2 | fix-later | - | x=0 y=96 w=1500 h=16 | x=0 y=96 w=1500 h=16 | gap 16 px vs section median 11 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=135 w=1500 h=8 | x=0 y=135 w=1500 h=8 | gap 8 px vs section median 11 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=145 w=1500 h=23 | x=0 y=145 w=1500 h=23 | gap 23 px vs section median 11 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1500 h=28 | x=0 y=177 w=1500 h=28 | gap 28 px vs section median 11 px (delta 17 px) |
| A2 | accept-as-legacy | - | x=0 y=217 w=1500 h=1 | x=0 y=217 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=241 w=1500 h=1 | x=0 y=241 w=1500 h=1 | gap 1 px vs section median 11 px (delta 10 px) |
| A2 | fix-later | - | x=0 y=243 w=1500 h=17 | x=0 y=243 w=1500 h=17 | gap 17 px vs section median 11 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=269 w=1500 h=19 | x=0 y=269 w=1500 h=19 | gap 19 px vs section median 11 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=298 w=1500 h=19 | x=0 y=298 w=1500 h=19 | gap 19 px vs section median 11 px (delta 8 px) |

### `search-palette-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=45 y=317 w=396 h=227 | x=45 y=317 w=396 h=227 | left edge at 45 px; nearest shared grid column 38 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 67 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=78 y=317 w=363 h=227 | x=78 y=317 w=363 h=227 | left edge at 78 px; nearest shared grid column 67 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=155 y=232 w=48 h=12 | x=155 y=232 w=48 h=12 | left edge at 155 px; nearest shared grid column 229 px (delta 74 px) |
| A1 | accept-as-legacy | - | x=211 y=123 w=16 h=12 | x=211 y=123 w=16 h=12 | left edge at 211 px; nearest shared grid column 229 px (delta 18 px) |
| A1 | accept-as-legacy | - | x=329 y=205 w=23 h=16 | x=329 y=205 w=23 h=16 | left edge at 329 px; nearest shared grid column 305 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=345 y=317 w=96 h=227 | x=345 y=317 w=96 h=227 | left edge at 345 px; nearest shared grid column 305 px (delta 40 px) |
| A1 | accept-as-legacy | - | x=349 y=232 w=12 h=12 | x=349 y=232 w=12 h=12 | left edge at 349 px; nearest shared grid column 305 px (delta 44 px) |
| A1 | accept-as-legacy | - | x=405 y=317 w=36 h=227 | x=405 y=317 w=36 h=227 | left edge at 405 px; nearest shared grid column 427 px (delta 22 px) |
| A1 | accept-as-legacy | - | x=542 y=123 w=46 h=12 | x=542 y=123 w=46 h=12 | left edge at 542 px; nearest shared grid column 462 px (delta 80 px) |
| A1 | accept-as-legacy | - | x=562 y=317 w=18 h=227 | x=562 y=317 w=18 h=227 | left edge at 562 px; nearest shared grid column 462 px (delta 100 px) |
| A2 | accept-as-legacy | - | x=0 y=11 w=600 h=7 | x=0 y=11 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=600 h=2 | x=0 y=19 w=600 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=600 h=1 | x=0 y=23 w=600 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=43 w=600 h=10 | x=0 y=43 w=600 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=600 h=28 | x=0 y=56 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=115 w=600 h=8 | x=0 y=115 w=600 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=135 w=600 h=7 | x=0 y=135 w=600 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=145 w=600 h=23 | x=0 y=145 w=600 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=600 h=28 | x=0 y=177 w=600 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=221 w=600 h=11 | x=0 y=221 w=600 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=269 w=600 h=19 | x=0 y=269 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=298 w=600 h=19 | x=0 y=298 w=600 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| T1 | accept-as-legacy | - | x=0 y=8 w=600 h=3 | x=0 y=8 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=53 w=600 h=3 | x=0 y=53 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=600 h=3 | x=0 y=112 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=600 h=3 | x=0 y=142 w=600 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `search-palette-empty-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=33 y=123 w=32 h=12 | x=33 y=123 w=32 h=12 | left edge at 33 px; nearest shared grid column 30 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=74 y=123 w=19 h=12 | x=74 y=123 w=19 h=12 | left edge at 74 px; nearest shared grid column 57 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=220 y=375 w=560 h=111 | x=220 y=375 w=560 h=111 | left edge at 220 px; nearest shared grid column 57 px (delta 163 px) |
| A1 | accept-as-legacy | - | x=344 y=123 w=17 h=12 | x=344 y=123 w=17 h=12 | left edge at 344 px; nearest shared grid column 430 px (delta 86 px) |
| A1 | accept-as-legacy | - | x=366 y=864 w=14 h=32 | x=366 y=864 w=14 h=32 | left edge at 366 px; nearest shared grid column 430 px (delta 64 px) |
| A1 | accept-as-legacy | - | x=371 y=123 w=48 h=12 | x=371 y=123 w=48 h=12 | left edge at 371 px; nearest shared grid column 430 px (delta 59 px) |
| A1 | fix-later | - | x=442 y=123 w=282 h=12 | x=442 y=123 w=282 h=12 | left edge at 442 px; nearest shared grid column 438 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=445 y=24 w=37 h=19 | x=445 y=24 w=37 h=19 | left edge at 445 px; nearest shared grid column 438 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=529 y=205 w=23 h=16 | x=529 y=205 w=23 h=16 | left edge at 529 px; nearest shared grid column 505 px (delta 24 px) |
| A1 | accept-as-legacy | - | x=734 y=123 w=28 h=12 | x=734 y=123 w=28 h=12 | left edge at 734 px; nearest shared grid column 505 px (delta 229 px) |
| A1 | accept-as-legacy | - | x=740 y=123 w=22 h=12 | x=740 y=123 w=22 h=12 | left edge at 740 px; nearest shared grid column 505 px (delta 235 px) |
| A1 | accept-as-legacy | - | x=909 y=123 w=79 h=12 | x=909 y=123 w=79 h=12 | left edge at 909 px; nearest shared grid column 505 px (delta 404 px) |
| A1 | accept-as-legacy | - | x=914 y=123 w=74 h=12 | x=914 y=123 w=74 h=12 | left edge at 914 px; nearest shared grid column 505 px (delta 409 px) |
| A2 | accept-as-legacy | - | x=0 y=19 w=1000 h=2 | x=0 y=19 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=23 w=1000 h=1 | x=0 y=23 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=43 w=1000 h=10 | x=0 y=43 w=1000 h=10 | gap 10 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1000 h=30 | x=0 y=56 w=1000 h=30 | gap 30 px vs section median 16 px (delta 14 px) |
| A2 | fix-later | - | x=0 y=115 w=1000 h=8 | x=0 y=115 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=135 w=1000 h=7 | x=0 y=135 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=145 w=1000 h=23 | x=0 y=145 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=177 w=1000 h=28 | x=0 y=177 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=221 w=1000 h=11 | x=0 y=221 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=241 w=1000 h=1 | x=0 y=241 w=1000 h=1 | gap 1 px vs section median 16 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=269 w=1000 h=19 | x=0 y=269 w=1000 h=19 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | accept-as-legacy | - | x=0 y=298 w=1000 h=77 | x=0 y=298 w=1000 h=77 | gap 77 px vs section median 16 px (delta 61 px) |
| T1 | accept-as-legacy | - | x=0 y=53 w=1000 h=3 | x=0 y=53 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=112 w=1000 h=3 | x=0 y=112 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=142 w=1000 h=3 | x=0 y=142 w=1000 h=3 | text line height 3 px → em ≈ 4.3 logical px; nearest ladder rung 11 px |

### `settings-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=82 y=87 w=18 h=21 | x=82 y=87 w=18 h=21 | left edge at 82 px; nearest shared grid column 71 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=165 y=435 w=14 h=25 | x=165 y=435 w=14 h=25 | left edge at 165 px; nearest shared grid column 140 px (delta 25 px) |
| A1 | accept-as-legacy | - | x=181 y=496 w=176 h=33 | x=181 y=496 w=176 h=33 | left edge at 181 px; nearest shared grid column 140 px (delta 41 px) |
| A1 | accept-as-legacy | - | x=211 y=820 w=23 h=24 | x=211 y=820 w=23 h=24 | left edge at 211 px; nearest shared grid column 222 px (delta 11 px) |
| A1 | fix-later | - | x=238 y=496 w=119 h=33 | x=238 y=496 w=119 h=33 | left edge at 238 px; nearest shared grid column 233 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 456 px (delta 111 px) |
| A1 | accept-as-legacy | - | x=366 y=496 w=19 h=33 | x=366 y=496 w=19 h=33 | left edge at 366 px; nearest shared grid column 456 px (delta 90 px) |
| A1 | accept-as-legacy | - | x=427 y=496 w=31 h=33 | x=427 y=496 w=31 h=33 | left edge at 427 px; nearest shared grid column 456 px (delta 29 px) |
| A1 | accept-as-legacy | - | x=433 y=8 w=38 h=48 | x=433 y=8 w=38 h=48 | left edge at 433 px; nearest shared grid column 456 px (delta 23 px) |
| A1 | accept-as-legacy | - | x=440 y=679 w=45 h=25 | x=440 y=679 w=45 h=25 | left edge at 440 px; nearest shared grid column 456 px (delta 16 px) |
| A1 | accept-as-legacy | - | x=529 y=496 w=12 h=33 | x=529 y=496 w=12 h=33 | left edge at 529 px; nearest shared grid column 517 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=549 y=496 w=427 h=33 | x=549 y=496 w=427 h=33 | left edge at 549 px; nearest shared grid column 517 px (delta 32 px) |
| A1 | accept-as-legacy | - | x=581 y=618 w=37 h=33 | x=581 y=618 w=37 h=33 | left edge at 581 px; nearest shared grid column 517 px (delta 64 px) |
| A1 | accept-as-legacy | - | x=585 y=557 w=154 h=33 | x=585 y=557 w=154 h=33 | left edge at 585 px; nearest shared grid column 517 px (delta 68 px) |
| A1 | accept-as-legacy | - | x=606 y=618 w=12 h=33 | x=606 y=618 w=12 h=33 | left edge at 606 px; nearest shared grid column 517 px (delta 89 px) |
| A1 | accept-as-legacy | - | x=627 y=618 w=118 h=33 | x=627 y=618 w=118 h=33 | left edge at 627 px; nearest shared grid column 517 px (delta 110 px) |
| A1 | accept-as-legacy | - | x=713 y=172 w=28 h=202 | x=713 y=172 w=28 h=202 | left edge at 713 px; nearest shared grid column 771 px (delta 58 px) |
| A1 | accept-as-legacy | - | x=725 y=172 w=16 h=202 | x=725 y=172 w=16 h=202 | left edge at 725 px; nearest shared grid column 771 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=762 y=172 w=221 h=202 | x=762 y=172 w=221 h=202 | left edge at 762 px; nearest shared grid column 771 px (delta 9 px) |
| A1 | accept-as-legacy | - | x=830 y=557 w=30 h=33 | x=830 y=557 w=30 h=33 | left edge at 830 px; nearest shared grid column 858 px (delta 28 px) |
| A1 | accept-as-legacy | - | x=868 y=618 w=31 h=33 | x=868 y=618 w=31 h=33 | left edge at 868 px; nearest shared grid column 860 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 23 px (delta 15 px) |
| A2 | fix-later | - | x=0 y=71 w=1000 h=16 | x=0 y=71 w=1000 h=16 | gap 16 px vs section median 23 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=108 w=1000 h=16 | x=0 y=108 w=1000 h=16 | gap 16 px vs section median 23 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=132 w=1000 h=3 | x=0 y=132 w=1000 h=3 | gap 3 px vs section median 23 px (delta 20 px) |
| A2 | accept-as-legacy | - | x=0 y=460 w=1000 h=36 | x=0 y=460 w=1000 h=36 | gap 36 px vs section median 23 px (delta 13 px) |
| A2 | fix-later | - | x=0 y=529 w=1000 h=28 | x=0 y=529 w=1000 h=28 | gap 28 px vs section median 23 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=590 w=1000 h=28 | x=0 y=590 w=1000 h=28 | gap 28 px vs section median 23 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=651 w=1000 h=28 | x=0 y=651 w=1000 h=28 | gap 28 px vs section median 23 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=704 w=1000 h=15 | x=0 y=704 w=1000 h=15 | gap 15 px vs section median 23 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=729 w=1000 h=41 | x=0 y=729 w=1000 h=41 | gap 41 px vs section median 23 px (delta 18 px) |
| A2 | accept-as-legacy | - | x=0 y=782 w=1000 h=38 | x=0 y=782 w=1000 h=38 | gap 38 px vs section median 23 px (delta 15 px) |
| A2 | accept-as-legacy | - | x=0 y=844 w=1000 h=9 | x=0 y=844 w=1000 h=9 | gap 9 px vs section median 23 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 23 px (delta 19 px) |

### `settings-1.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=82 y=88 w=18 h=20 | x=82 y=88 w=18 h=20 | left edge at 82 px; nearest shared grid column 71 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=165 y=435 w=14 h=25 | x=165 y=435 w=14 h=25 | left edge at 165 px; nearest shared grid column 140 px (delta 25 px) |
| A1 | accept-as-legacy | - | x=211 y=820 w=23 h=24 | x=211 y=820 w=23 h=24 | left edge at 211 px; nearest shared grid column 222 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=321 y=496 w=203 h=33 | x=321 y=496 w=203 h=33 | left edge at 321 px; nearest shared grid column 233 px (delta 88 px) |
| A1 | accept-as-legacy | - | x=512 y=864 w=71 h=32 | x=512 y=864 w=71 h=32 | left edge at 512 px; nearest shared grid column 708 px (delta 196 px) |
| A1 | accept-as-legacy | - | x=533 y=496 w=19 h=33 | x=533 y=496 w=19 h=33 | left edge at 533 px; nearest shared grid column 708 px (delta 175 px) |
| A1 | accept-as-legacy | - | x=593 y=496 w=81 h=33 | x=593 y=496 w=81 h=33 | left edge at 593 px; nearest shared grid column 708 px (delta 115 px) |
| A1 | accept-as-legacy | - | x=659 y=496 w=15 h=33 | x=659 y=496 w=15 h=33 | left edge at 659 px; nearest shared grid column 708 px (delta 49 px) |
| A1 | accept-as-legacy | - | x=683 y=8 w=38 h=48 | x=683 y=8 w=38 h=48 | left edge at 683 px; nearest shared grid column 708 px (delta 25 px) |
| A1 | accept-as-legacy | - | x=690 y=679 w=45 h=25 | x=690 y=679 w=45 h=25 | left edge at 690 px; nearest shared grid column 708 px (delta 18 px) |
| A1 | accept-as-legacy | - | x=696 y=496 w=12 h=33 | x=696 y=496 w=12 h=33 | left edge at 696 px; nearest shared grid column 708 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=716 y=496 w=760 h=33 | x=716 y=496 w=760 h=33 | left edge at 716 px; nearest shared grid column 709 px (delta 7 px) |
| A1 | accept-as-legacy | - | x=730 y=8 w=57 h=48 | x=730 y=8 w=57 h=48 | left edge at 730 px; nearest shared grid column 749 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=1081 y=618 w=37 h=33 | x=1081 y=618 w=37 h=33 | left edge at 1081 px; nearest shared grid column 1125 px (delta 44 px) |
| A1 | accept-as-legacy | - | x=1085 y=557 w=154 h=33 | x=1085 y=557 w=154 h=33 | left edge at 1085 px; nearest shared grid column 1125 px (delta 40 px) |
| A1 | accept-as-legacy | - | x=1088 y=173 w=28 h=201 | x=1088 y=173 w=28 h=201 | left edge at 1088 px; nearest shared grid column 1125 px (delta 37 px) |
| A1 | accept-as-legacy | - | x=1100 y=173 w=16 h=201 | x=1100 y=173 w=16 h=201 | left edge at 1100 px; nearest shared grid column 1125 px (delta 25 px) |
| A1 | accept-as-legacy | - | x=1106 y=618 w=12 h=33 | x=1106 y=618 w=12 h=33 | left edge at 1106 px; nearest shared grid column 1125 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=1137 y=173 w=346 h=201 | x=1137 y=173 w=346 h=201 | left edge at 1137 px; nearest shared grid column 1127 px (delta 10 px) |
| A1 | accept-as-legacy | - | x=1330 y=557 w=30 h=33 | x=1330 y=557 w=30 h=33 | left edge at 1330 px; nearest shared grid column 1358 px (delta 28 px) |
| A1 | accept-as-legacy | - | x=1368 y=618 w=31 h=33 | x=1368 y=618 w=31 h=33 | left edge at 1368 px; nearest shared grid column 1360 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1500 h=8 | x=0 y=56 w=1500 h=8 | gap 8 px vs section median 24 px (delta 16 px) |
| A2 | fix-later | - | x=0 y=69 w=1500 h=19 | x=0 y=69 w=1500 h=19 | gap 19 px vs section median 24 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=108 w=1500 h=16 | x=0 y=108 w=1500 h=16 | gap 16 px vs section median 24 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=132 w=1500 h=3 | x=0 y=132 w=1500 h=3 | gap 3 px vs section median 24 px (delta 21 px) |
| A2 | accept-as-legacy | - | x=0 y=460 w=1500 h=36 | x=0 y=460 w=1500 h=36 | gap 36 px vs section median 24 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=529 w=1500 h=28 | x=0 y=529 w=1500 h=28 | gap 28 px vs section median 24 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=590 w=1500 h=28 | x=0 y=590 w=1500 h=28 | gap 28 px vs section median 24 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=651 w=1500 h=28 | x=0 y=651 w=1500 h=28 | gap 28 px vs section median 24 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=704 w=1500 h=15 | x=0 y=704 w=1500 h=15 | gap 15 px vs section median 24 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=729 w=1500 h=41 | x=0 y=729 w=1500 h=41 | gap 41 px vs section median 24 px (delta 17 px) |
| A2 | accept-as-legacy | - | x=0 y=782 w=1500 h=38 | x=0 y=782 w=1500 h=38 | gap 38 px vs section median 24 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=844 w=1500 h=11 | x=0 y=844 w=1500 h=11 | gap 11 px vs section median 24 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=860 w=1500 h=4 | x=0 y=860 w=1500 h=4 | gap 4 px vs section median 24 px (delta 20 px) |
| T1 | accept-as-legacy | - | x=0 y=64 w=1500 h=5 | x=0 y=64 w=1500 h=5 | text line height 5 px → em ≈ 7.1 logical px; nearest ladder rung 11 px |
| T1 | accept-as-legacy | - | x=0 y=855 w=1500 h=5 | x=0 y=855 w=1500 h=5 | text line height 5 px → em ≈ 7.1 logical px; nearest ladder rung 11 px |

### `settings-1.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=299 y=64 w=14 h=796 | x=299 y=64 w=14 h=796 | left edge at 299 px; nearest shared grid column 254 px (delta 45 px) |
| A1 | fix-later | - | x=442 y=64 w=33 h=796 | x=442 y=64 w=33 h=796 | left edge at 442 px; nearest shared grid column 439 px (delta 3 px) |
| A1 | accept-as-legacy | - | x=450 y=64 w=25 h=796 | x=450 y=64 w=25 h=796 | left edge at 450 px; nearest shared grid column 439 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=458 y=64 w=17 h=796 | x=458 y=64 w=17 h=796 | left edge at 458 px; nearest shared grid column 439 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=462 y=8 w=12 h=48 | x=462 y=8 w=12 h=48 | left edge at 462 px; nearest shared grid column 439 px (delta 23 px) |
| A1 | accept-as-legacy | - | x=483 y=8 w=57 h=48 | x=483 y=8 w=57 h=48 | left edge at 483 px; nearest shared grid column 501 px (delta 18 px) |
| A1 | accept-as-legacy | - | x=532 y=64 w=14 h=796 | x=532 y=64 w=14 h=796 | left edge at 532 px; nearest shared grid column 503 px (delta 29 px) |

### `task-view-0.5-500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=44 y=36 w=6 h=10 | x=88 y=72 w=12 h=20 | left edge at 44 px; nearest shared grid column 33 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=47 y=223 w=21 h=6 | x=94 y=446 w=42 h=12 | left edge at 47 px; nearest shared grid column 33 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=52 y=223 w=16 h=6 | x=104 y=446 w=32 h=12 | left edge at 52 px; nearest shared grid column 33 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=57 y=223 w=11 h=6 | x=114 y=446 w=22 h=12 | left edge at 57 px; nearest shared grid column 76 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=69 y=36 w=10 h=10 | x=138 y=72 w=20 h=20 | left edge at 69 px; nearest shared grid column 76 px (delta 7 px) |
| A1 | fix-later | - | x=124 y=201 w=862 h=17 | x=248 y=402 w=1724 h=34 | left edge at 124 px; nearest shared grid column 119 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=177 y=36 w=11 h=10 | x=354 y=72 w=22 h=20 | left edge at 177 px; nearest shared grid column 119 px (delta 58 px) |
| A1 | accept-as-legacy | - | x=227 y=36 w=7 h=10 | x=454 y=72 w=14 h=20 | left edge at 227 px; nearest shared grid column 119 px (delta 108 px) |
| A1 | accept-as-legacy | - | x=248 y=55 w=21 h=6 | x=496 y=110 w=42 h=12 | left edge at 248 px; nearest shared grid column 339 px (delta 91 px) |
| A1 | accept-as-legacy | - | x=261 y=36 w=14 h=10 | x=522 y=72 w=28 h=20 | left edge at 261 px; nearest shared grid column 339 px (delta 78 px) |
| A1 | accept-as-legacy | - | x=310 y=36 w=8 h=10 | x=620 y=72 w=16 h=20 | left edge at 310 px; nearest shared grid column 339 px (delta 29 px) |
| A1 | accept-as-legacy | - | x=324 y=36 w=11 h=10 | x=648 y=72 w=22 h=20 | left edge at 324 px; nearest shared grid column 339 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 507 px (delta 45 px) |
| A1 | accept-as-legacy | - | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 507 px (delta 11 px) |
| A1 | fix-later | - | x=501 y=55 w=13 h=6 | x=1002 y=110 w=26 h=12 | left edge at 501 px; nearest shared grid column 507 px (delta 6 px) |
| A1 | fix-later | - | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 507 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=534 y=482 w=6 h=16 | x=1068 y=964 w=12 h=32 | left edge at 534 px; nearest shared grid column 513 px (delta 21 px) |
| A1 | accept-as-legacy | - | x=757 y=64 w=17 h=6 | x=1514 y=128 w=34 h=12 | left edge at 757 px; nearest shared grid column 984 px (delta 227 px) |
| A1 | fix-later | - | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 984 px (delta 4 px) |
| A1 | fix-later | - | x=990 y=201 w=6 h=17 | x=1980 y=402 w=12 h=34 | left edge at 990 px; nearest shared grid column 984 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=34 w=1000 h=2 | x=0 y=68 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=46 w=1000 h=9 | x=0 y=92 w=2000 h=18 | gap 9 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=61 w=1000 h=3 | x=0 y=122 w=2000 h=6 | gap 3 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=84 w=1000 h=1 | x=0 y=168 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=121 w=1000 h=11 | x=0 y=242 w=2000 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=137 w=1000 h=18 | x=0 y=274 w=2000 h=36 | gap 18 px vs section median 6 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=157 w=1000 h=15 | x=0 y=314 w=2000 h=30 | gap 15 px vs section median 6 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=181 w=1000 h=13 | x=0 y=362 w=2000 h=26 | gap 13 px vs section median 6 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=199 w=1000 h=2 | x=0 y=398 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=229 w=1000 h=12 | x=0 y=458 w=2000 h=24 | gap 12 px vs section median 6 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=247 w=1000 h=1 | x=0 y=494 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=480 w=1000 h=2 | x=0 y=960 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |

### `task-view-0.5-900`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=44 y=36 w=6 h=10 | x=88 y=72 w=12 h=20 | left edge at 44 px; nearest shared grid column 33 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=47 y=223 w=21 h=6 | x=94 y=446 w=42 h=12 | left edge at 47 px; nearest shared grid column 33 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=52 y=223 w=16 h=6 | x=104 y=446 w=32 h=12 | left edge at 52 px; nearest shared grid column 33 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=57 y=223 w=11 h=6 | x=114 y=446 w=22 h=12 | left edge at 57 px; nearest shared grid column 76 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=69 y=36 w=10 h=10 | x=138 y=72 w=20 h=20 | left edge at 69 px; nearest shared grid column 76 px (delta 7 px) |
| A1 | fix-later | - | x=124 y=201 w=862 h=17 | x=248 y=402 w=1724 h=34 | left edge at 124 px; nearest shared grid column 119 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=177 y=36 w=11 h=10 | x=354 y=72 w=22 h=20 | left edge at 177 px; nearest shared grid column 119 px (delta 58 px) |
| A1 | accept-as-legacy | - | x=227 y=36 w=7 h=10 | x=454 y=72 w=14 h=20 | left edge at 227 px; nearest shared grid column 119 px (delta 108 px) |
| A1 | accept-as-legacy | - | x=248 y=55 w=21 h=6 | x=496 y=110 w=42 h=12 | left edge at 248 px; nearest shared grid column 339 px (delta 91 px) |
| A1 | accept-as-legacy | - | x=261 y=36 w=14 h=10 | x=522 y=72 w=28 h=20 | left edge at 261 px; nearest shared grid column 339 px (delta 78 px) |
| A1 | accept-as-legacy | - | x=310 y=36 w=8 h=10 | x=620 y=72 w=16 h=20 | left edge at 310 px; nearest shared grid column 339 px (delta 29 px) |
| A1 | accept-as-legacy | - | x=324 y=36 w=11 h=10 | x=648 y=72 w=22 h=20 | left edge at 324 px; nearest shared grid column 339 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=462 y=4 w=28 h=24 | x=924 y=8 w=56 h=48 | left edge at 462 px; nearest shared grid column 507 px (delta 45 px) |
| A1 | accept-as-legacy | - | x=496 y=4 w=6 h=24 | x=992 y=8 w=12 h=48 | left edge at 496 px; nearest shared grid column 507 px (delta 11 px) |
| A1 | fix-later | - | x=501 y=55 w=13 h=6 | x=1002 y=110 w=26 h=12 | left edge at 501 px; nearest shared grid column 507 px (delta 6 px) |
| A1 | fix-later | - | x=503 y=4 w=16 h=24 | x=1006 y=8 w=32 h=48 | left edge at 503 px; nearest shared grid column 507 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=534 y=882 w=6 h=16 | x=1068 y=1764 w=12 h=32 | left edge at 534 px; nearest shared grid column 513 px (delta 21 px) |
| A1 | accept-as-legacy | - | x=757 y=64 w=17 h=6 | x=1514 y=128 w=34 h=12 | left edge at 757 px; nearest shared grid column 984 px (delta 227 px) |
| A1 | fix-later | - | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 984 px (delta 4 px) |
| A1 | fix-later | - | x=990 y=201 w=6 h=17 | x=1980 y=402 w=12 h=34 | left edge at 990 px; nearest shared grid column 984 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=34 w=1000 h=2 | x=0 y=68 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=46 w=1000 h=9 | x=0 y=92 w=2000 h=18 | gap 9 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=61 w=1000 h=3 | x=0 y=122 w=2000 h=6 | gap 3 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=84 w=1000 h=1 | x=0 y=168 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=121 w=1000 h=11 | x=0 y=242 w=2000 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=137 w=1000 h=18 | x=0 y=274 w=2000 h=36 | gap 18 px vs section median 6 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=157 w=1000 h=15 | x=0 y=314 w=2000 h=30 | gap 15 px vs section median 6 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=181 w=1000 h=13 | x=0 y=362 w=2000 h=26 | gap 13 px vs section median 6 px (delta 7 px) |
| A2 | fix-later | - | x=0 y=199 w=1000 h=2 | x=0 y=398 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=229 w=1000 h=12 | x=0 y=458 w=2000 h=24 | gap 12 px vs section median 6 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=247 w=1000 h=1 | x=0 y=494 w=2000 h=2 | gap 1 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=880 w=1000 h=2 | x=0 y=1760 w=2000 h=4 | gap 2 px vs section median 6 px (delta 4 px) |

### `task-view-1.5-500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=499 w=3 h=1 | x=0 y=333 w=2 h=1 | content run touches the bottom edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 997 px = 100% of the edge through the run) |
| C1 | accept-as-legacy | thumb | x=997 y=499 w=3 h=1 | x=665 y=333 w=2 h=1 | content run touches the bottom edge (extent 3 px along the edge) — extent 3 px inside the thumb band with a track-column signature (contiguous near-floor band 997 px = 100% of the edge through the run) |
| A1 | accept-as-legacy | - | x=149 y=96 w=49 h=404 | x=99 y=64 w=33 h=269 | left edge at 149 px; nearest shared grid column 79 px (delta 70 px) |
| A1 | accept-as-legacy | - | x=387 y=12 w=83 h=72 | x=258 y=8 w=55 h=48 | left edge at 387 px; nearest shared grid column 521 px (delta 134 px) |
| A1 | accept-as-legacy | - | x=770 y=96 w=19 h=404 | x=513 y=64 w=13 h=269 | left edge at 770 px; nearest shared grid column 522 px (delta 248 px) |
| A1 | accept-as-legacy | - | x=819 y=96 w=28 h=404 | x=546 y=64 w=19 h=269 | left edge at 819 px; nearest shared grid column 522 px (delta 297 px) |

### `task-view-1.5-900`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=112 y=96 w=27 h=744 | x=75 y=64 w=18 h=496 | left edge at 112 px; nearest shared grid column 101 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=193 y=96 w=43 h=744 | x=129 y=64 w=29 h=496 | left edge at 193 px; nearest shared grid column 150 px (delta 43 px) |
| A1 | accept-as-legacy | - | x=202 y=96 w=34 h=744 | x=135 y=64 w=23 h=496 | left edge at 202 px; nearest shared grid column 150 px (delta 52 px) |
| A1 | accept-as-legacy | - | x=387 y=12 w=83 h=72 | x=258 y=8 w=55 h=48 | left edge at 387 px; nearest shared grid column 351 px (delta 36 px) |
| A1 | accept-as-legacy | - | x=770 y=96 w=19 h=744 | x=513 y=64 w=13 h=496 | left edge at 770 px; nearest shared grid column 522 px (delta 248 px) |
| A1 | accept-as-legacy | - | x=819 y=96 w=28 h=744 | x=546 y=64 w=19 h=496 | left edge at 819 px; nearest shared grid column 522 px (delta 297 px) |

### `task-view-3.0-500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | fix-later | real-clip | x=0 y=499 w=6 h=1 | x=0 y=166 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) — sub-severing extent 6 px with no track-column signature (near-floor band 33 px = 3% of the edge) and past the chrome-speck scale — a small real clip |
| C1 | fix-later | real-clip | x=994 y=499 w=6 h=1 | x=331 y=166 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) — sub-severing extent 6 px with no track-column signature (near-floor band 33 px = 3% of the edge) and past the chrome-speck scale — a small real clip |
| A1 | accept-as-legacy | - | x=239 y=192 w=56 h=308 | x=80 y=64 w=19 h=103 | left edge at 239 px; nearest shared grid column 73 px (delta 166 px) |
| A1 | accept-as-legacy | - | x=274 y=24 w=166 h=144 | x=91 y=8 w=55 h=48 | left edge at 274 px; nearest shared grid column 73 px (delta 201 px) |
| A1 | accept-as-legacy | - | x=387 y=192 w=85 h=308 | x=129 y=64 w=28 h=103 | left edge at 387 px; nearest shared grid column 73 px (delta 314 px) |

### `task-view-3.0-900`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | accept-as-legacy | thumb | x=0 y=899 w=6 h=1 | x=0 y=300 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) — extent 6 px inside the thumb band with a track-column signature (contiguous near-floor band 994 px = 99% of the edge through the run) |
| C1 | accept-as-legacy | thumb | x=994 y=899 w=6 h=1 | x=331 y=300 w=2 h=0 | content run touches the bottom edge (extent 6 px along the edge) — extent 6 px inside the thumb band with a track-column signature (contiguous near-floor band 994 px = 99% of the edge through the run) |
| A1 | accept-as-legacy | - | x=50 y=192 w=56 h=708 | x=17 y=64 w=19 h=236 | left edge at 50 px; nearest shared grid column 61 px (delta 11 px) |
| A1 | accept-as-legacy | - | x=239 y=192 w=56 h=708 | x=80 y=64 w=19 h=236 | left edge at 239 px; nearest shared grid column 157 px (delta 82 px) |
| A1 | accept-as-legacy | - | x=274 y=24 w=166 h=144 | x=91 y=8 w=55 h=48 | left edge at 274 px; nearest shared grid column 157 px (delta 117 px) |
| A1 | accept-as-legacy | - | x=297 y=192 w=175 h=708 | x=99 y=64 w=58 h=236 | left edge at 297 px; nearest shared grid column 157 px (delta 140 px) |
| A1 | accept-as-legacy | - | x=387 y=192 w=85 h=708 | x=129 y=64 w=28 h=236 | left edge at 387 px; nearest shared grid column 543 px (delta 156 px) |
| A1 | accept-as-legacy | - | x=823 y=192 w=153 h=708 | x=274 y=64 w=51 h=236 | left edge at 823 px; nearest shared grid column 545 px (delta 278 px) |
| A1 | accept-as-legacy | - | x=910 y=192 w=66 h=708 | x=303 y=64 w=22 h=236 | left edge at 910 px; nearest shared grid column 545 px (delta 365 px) |

### `task-view-focus-cell-1.0-900`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=156 w=490 h=36 | x=12 y=156 w=490 h=36 | left edge at 12 px; nearest shared grid column 9 px (delta 3 px) |
| A1 | fix-later | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 22 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=99 y=204 w=893 h=33 | x=99 y=204 w=893 h=33 | left edge at 99 px; nearest shared grid column 67 px (delta 32 px) |
| A1 | accept-as-legacy | - | x=158 y=396 w=15 h=33 | x=158 y=396 w=15 h=33 | left edge at 158 px; nearest shared grid column 171 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=221 y=396 w=15 h=33 | x=221 y=396 w=15 h=33 | left edge at 221 px; nearest shared grid column 234 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 246 px (delta 99 px) |
| A1 | accept-as-legacy | - | x=372 y=71 w=60 h=20 | x=372 y=71 w=60 h=20 | left edge at 372 px; nearest shared grid column 487 px (delta 115 px) |
| A1 | accept-as-legacy | - | x=419 y=71 w=13 h=20 | x=419 y=71 w=13 h=20 | left edge at 419 px; nearest shared grid column 487 px (delta 68 px) |
| A1 | accept-as-legacy | - | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 487 px (delta 63 px) |
| A1 | accept-as-legacy | - | x=546 y=71 w=19 h=20 | x=546 y=71 w=19 h=20 | left edge at 546 px; nearest shared grid column 515 px (delta 31 px) |
| A2 | fix-later | - | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 15 px (delta 5 px) |
| A2 | accept-as-legacy | - | x=0 y=120 w=1000 h=3 | x=0 y=120 w=1000 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=192 w=1000 h=12 | x=0 y=192 w=1000 h=12 | gap 12 px vs section median 15 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=237 w=1000 h=23 | x=0 y=237 w=1000 h=23 | gap 23 px vs section median 15 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=269 w=1000 h=35 | x=0 y=269 w=1000 h=35 | gap 35 px vs section median 15 px (delta 20 px) |
| A2 | accept-as-legacy | - | x=0 y=305 w=1000 h=2 | x=0 y=305 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=309 w=1000 h=2 | x=0 y=309 w=1000 h=2 | gap 2 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=312 w=1000 h=28 | x=0 y=312 w=1000 h=28 | gap 28 px vs section median 15 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=357 w=1000 h=27 | x=0 y=357 w=1000 h=27 | gap 27 px vs section median 15 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=393 w=1000 h=3 | x=0 y=393 w=1000 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=429 w=1000 h=11 | x=0 y=429 w=1000 h=11 | gap 11 px vs section median 15 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=449 w=1000 h=27 | x=0 y=449 w=1000 h=27 | gap 27 px vs section median 15 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=485 w=1000 h=3 | x=0 y=485 w=1000 h=3 | gap 3 px vs section median 15 px (delta 12 px) |
| A2 | accept-as-legacy | - | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 15 px (delta 11 px) |
| T1 | fix-later | - | x=0 y=123 w=1000 h=17 | x=0 y=123 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |
| T1 | fix-later | - | x=0 y=340 w=1000 h=17 | x=0 y=340 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |

### `task-view-overdue-banner-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=155 w=490 h=37 | x=12 y=155 w=490 h=37 | left edge at 12 px; nearest shared grid column 9 px (delta 3 px) |
| A1 | fix-later | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 22 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=81 y=796 w=16 h=16 | x=81 y=796 w=16 h=16 | left edge at 81 px; nearest shared grid column 69 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=99 y=204 w=893 h=33 | x=99 y=204 w=893 h=33 | left edge at 99 px; nearest shared grid column 125 px (delta 26 px) |
| A1 | accept-as-legacy | - | x=217 y=796 w=44 h=16 | x=217 y=796 w=44 h=16 | left edge at 217 px; nearest shared grid column 234 px (delta 17 px) |
| A1 | accept-as-legacy | - | x=221 y=396 w=15 h=33 | x=221 y=396 w=15 h=33 | left edge at 221 px; nearest shared grid column 234 px (delta 13 px) |
| A1 | fix-later | - | x=249 y=796 w=12 h=16 | x=249 y=796 w=12 h=16 | left edge at 249 px; nearest shared grid column 244 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=335 y=796 w=12 h=16 | x=335 y=796 w=12 h=16 | left edge at 335 px; nearest shared grid column 299 px (delta 36 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 299 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=372 y=64 w=60 h=28 | x=372 y=64 w=60 h=28 | left edge at 372 px; nearest shared grid column 429 px (delta 57 px) |
| A1 | accept-as-legacy | - | x=380 y=796 w=38 h=16 | x=380 y=796 w=38 h=16 | left edge at 380 px; nearest shared grid column 429 px (delta 49 px) |
| A1 | accept-as-legacy | - | x=393 y=796 w=25 h=16 | x=393 y=796 w=25 h=16 | left edge at 393 px; nearest shared grid column 429 px (delta 36 px) |
| A1 | accept-as-legacy | - | x=419 y=64 w=13 h=28 | x=419 y=64 w=13 h=28 | left edge at 419 px; nearest shared grid column 429 px (delta 10 px) |
| A1 | fix-later | - | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 429 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=546 y=64 w=19 h=28 | x=546 y=64 w=19 h=28 | left edge at 546 px; nearest shared grid column 515 px (delta 31 px) |
| A2 | accept-as-legacy | - | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 18 px (delta 10 px) |
| A2 | accept-as-legacy | - | x=0 y=121 w=1000 h=5 | x=0 y=121 w=1000 h=5 | gap 5 px vs section median 18 px (delta 13 px) |
| A2 | fix-later | - | x=0 y=140 w=1000 h=15 | x=0 y=140 w=1000 h=15 | gap 15 px vs section median 18 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=192 w=1000 h=12 | x=0 y=192 w=1000 h=12 | gap 12 px vs section median 18 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=237 w=1000 h=22 | x=0 y=237 w=1000 h=22 | gap 22 px vs section median 18 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=270 w=1000 h=31 | x=0 y=270 w=1000 h=31 | gap 31 px vs section median 18 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=313 w=1000 h=27 | x=0 y=313 w=1000 h=27 | gap 27 px vs section median 18 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=357 w=1000 h=27 | x=0 y=357 w=1000 h=27 | gap 27 px vs section median 18 px (delta 9 px) |
| A2 | accept-as-legacy | - | x=0 y=394 w=1000 h=2 | x=0 y=394 w=1000 h=2 | gap 2 px vs section median 18 px (delta 16 px) |
| A2 | fix-later | - | x=0 y=429 w=1000 h=10 | x=0 y=429 w=1000 h=10 | gap 10 px vs section median 18 px (delta 8 px) |
| A2 | accept-as-legacy | - | x=0 y=449 w=1000 h=27 | x=0 y=449 w=1000 h=27 | gap 27 px vs section median 18 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=784 w=1000 h=12 | x=0 y=784 w=1000 h=12 | gap 12 px vs section median 18 px (delta 6 px) |
| T1 | fix-later | - | x=0 y=340 w=1000 h=17 | x=0 y=340 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |

### `today-0.5-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=866 w=19 h=7 | x=24 y=1732 w=38 h=14 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | - | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | fix-later | - | x=35 y=848 w=9 h=8 | x=70 y=1696 w=18 h=16 | left edge at 35 px; nearest shared grid column 30 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=51 y=848 w=15 h=8 | x=102 y=1696 w=30 h=16 | left edge at 51 px; nearest shared grid column 30 px (delta 21 px) |
| A1 | accept-as-legacy | - | x=68 y=848 w=19 h=8 | x=136 y=1696 w=38 h=16 | left edge at 68 px; nearest shared grid column 30 px (delta 38 px) |
| A1 | accept-as-legacy | - | x=127 y=848 w=23 h=8 | x=254 y=1696 w=46 h=16 | left edge at 127 px; nearest shared grid column 30 px (delta 97 px) |
| A1 | accept-as-legacy | - | x=176 y=848 w=24 h=8 | x=352 y=1696 w=48 h=16 | left edge at 176 px; nearest shared grid column 30 px (delta 146 px) |
| A1 | accept-as-legacy | - | x=210 y=848 w=8 h=8 | x=420 y=1696 w=16 h=16 | left edge at 210 px; nearest shared grid column 30 px (delta 180 px) |
| A1 | accept-as-legacy | - | x=333 y=56 w=322 h=17 | x=666 y=112 w=644 h=34 | left edge at 333 px; nearest shared grid column 30 px (delta 303 px) |
| A1 | accept-as-legacy | - | x=339 y=882 w=36 h=16 | x=678 y=1764 w=72 h=32 | left edge at 339 px; nearest shared grid column 30 px (delta 309 px) |
| A1 | accept-as-legacy | - | x=472 y=4 w=12 h=24 | x=944 y=8 w=24 h=48 | left edge at 472 px; nearest shared grid column 30 px (delta 442 px) |
| A1 | accept-as-legacy | - | x=485 y=4 w=6 h=24 | x=970 y=8 w=12 h=48 | left edge at 485 px; nearest shared grid column 30 px (delta 455 px) |
| A1 | accept-as-legacy | - | x=496 y=4 w=29 h=24 | x=992 y=8 w=58 h=48 | left edge at 496 px; nearest shared grid column 960 px (delta 464 px) |
| A1 | accept-as-legacy | - | x=534 y=882 w=6 h=16 | x=1068 y=1764 w=12 h=32 | left edge at 534 px; nearest shared grid column 960 px (delta 426 px) |
| A1 | accept-as-legacy | - | x=630 y=866 w=18 h=7 | x=1260 y=1732 w=36 h=14 | left edge at 630 px; nearest shared grid column 960 px (delta 330 px) |
| A1 | accept-as-legacy | - | x=638 y=866 w=10 h=7 | x=1276 y=1732 w=20 h=14 | left edge at 638 px; nearest shared grid column 960 px (delta 322 px) |
| A1 | accept-as-legacy | - | x=642 y=866 w=6 h=7 | x=1284 y=1732 w=12 h=14 | left edge at 642 px; nearest shared grid column 960 px (delta 318 px) |
| A1 | accept-as-legacy | - | x=653 y=866 w=313 h=7 | x=1306 y=1732 w=626 h=14 | left edge at 653 px; nearest shared grid column 960 px (delta 307 px) |
| A1 | accept-as-legacy | - | x=659 y=56 w=170 h=17 | x=1318 y=112 w=340 h=34 | left edge at 659 px; nearest shared grid column 960 px (delta 301 px) |
| A1 | accept-as-legacy | - | x=693 y=848 w=18 h=8 | x=1386 y=1696 w=36 h=16 | left edge at 693 px; nearest shared grid column 960 px (delta 267 px) |
| A1 | accept-as-legacy | - | x=701 y=848 w=10 h=8 | x=1402 y=1696 w=20 h=16 | left edge at 701 px; nearest shared grid column 960 px (delta 259 px) |
| A1 | accept-as-legacy | - | x=705 y=848 w=6 h=8 | x=1410 y=1696 w=12 h=16 | left edge at 705 px; nearest shared grid column 960 px (delta 255 px) |
| A1 | accept-as-legacy | - | x=716 y=848 w=250 h=8 | x=1432 y=1696 w=500 h=16 | left edge at 716 px; nearest shared grid column 960 px (delta 244 px) |
| A1 | accept-as-legacy | - | x=907 y=56 w=87 h=17 | x=1814 y=112 w=174 h=34 | left edge at 907 px; nearest shared grid column 960 px (delta 53 px) |
| A1 | accept-as-legacy | - | x=975 y=117 w=10 h=6 | x=1950 y=234 w=20 h=12 | left edge at 975 px; nearest shared grid column 960 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=980 y=4 w=6 h=24 | x=1960 y=8 w=12 h=48 | left edge at 980 px; nearest shared grid column 999 px (delta 19 px) |
| A2 | fix-later | - | x=0 y=28 w=1000 h=4 | x=0 y=56 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=73 w=1000 h=11 | x=0 y=146 w=2000 h=22 | gap 11 px vs section median 8 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=89 w=1000 h=14 | x=0 y=178 w=2000 h=28 | gap 14 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=123 w=1000 h=12 | x=0 y=246 w=2000 h=24 | gap 12 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=140 w=1000 h=4 | x=0 y=280 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=170 w=1000 h=4 | x=0 y=340 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=178 w=1000 h=2 | x=0 y=356 w=2000 h=4 | gap 2 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=213 w=1000 h=4 | x=0 y=426 w=2000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=842 w=1000 h=6 | x=0 y=1684 w=2000 h=12 | gap 6 px vs section median 9 px (delta 3 px) |

### `today-0.5-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=867 w=19 h=6 | x=24 y=1734 w=38 h=12 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | - | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | fix-later | - | x=35 y=849 w=9 h=7 | x=70 y=1698 w=18 h=14 | left edge at 35 px; nearest shared grid column 30 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=51 y=849 w=15 h=7 | x=102 y=1698 w=30 h=14 | left edge at 51 px; nearest shared grid column 30 px (delta 21 px) |
| A1 | accept-as-legacy | - | x=68 y=849 w=19 h=7 | x=136 y=1698 w=38 h=14 | left edge at 68 px; nearest shared grid column 30 px (delta 38 px) |
| A1 | accept-as-legacy | - | x=127 y=849 w=23 h=7 | x=254 y=1698 w=46 h=14 | left edge at 127 px; nearest shared grid column 30 px (delta 97 px) |
| A1 | accept-as-legacy | - | x=176 y=849 w=24 h=7 | x=352 y=1698 w=48 h=14 | left edge at 176 px; nearest shared grid column 30 px (delta 146 px) |
| A1 | accept-as-legacy | - | x=210 y=849 w=8 h=7 | x=420 y=1698 w=16 h=14 | left edge at 210 px; nearest shared grid column 30 px (delta 180 px) |
| A1 | accept-as-legacy | - | x=499 y=56 w=490 h=17 | x=998 y=112 w=980 h=34 | left edge at 499 px; nearest shared grid column 30 px (delta 469 px) |
| A1 | accept-as-legacy | - | x=506 y=882 w=36 h=16 | x=1012 y=1764 w=72 h=32 | left edge at 506 px; nearest shared grid column 30 px (delta 476 px) |
| A1 | accept-as-legacy | - | x=722 y=4 w=12 h=24 | x=1444 y=8 w=24 h=48 | left edge at 722 px; nearest shared grid column 30 px (delta 692 px) |
| A1 | accept-as-legacy | - | x=735 y=4 w=6 h=24 | x=1470 y=8 w=12 h=48 | left edge at 735 px; nearest shared grid column 30 px (delta 705 px) |
| A1 | accept-as-legacy | - | x=746 y=4 w=29 h=24 | x=1492 y=8 w=58 h=48 | left edge at 746 px; nearest shared grid column 1460 px (delta 714 px) |
| A1 | accept-as-legacy | - | x=784 y=882 w=6 h=16 | x=1568 y=1764 w=12 h=32 | left edge at 784 px; nearest shared grid column 1460 px (delta 676 px) |
| A1 | accept-as-legacy | - | x=975 y=867 w=10 h=6 | x=1950 y=1734 w=20 h=12 | left edge at 975 px; nearest shared grid column 1460 px (delta 485 px) |
| A1 | accept-as-legacy | - | x=993 y=56 w=252 h=17 | x=1986 y=112 w=504 h=34 | left edge at 993 px; nearest shared grid column 1460 px (delta 467 px) |
| A1 | accept-as-legacy | - | x=1038 y=849 w=10 h=7 | x=2076 y=1698 w=20 h=14 | left edge at 1038 px; nearest shared grid column 1460 px (delta 422 px) |
| A1 | accept-as-legacy | - | x=1371 y=56 w=8 h=17 | x=2742 y=112 w=16 h=34 | left edge at 1371 px; nearest shared grid column 1460 px (delta 89 px) |
| A1 | accept-as-legacy | - | x=1480 y=4 w=6 h=24 | x=2960 y=8 w=12 h=48 | left edge at 1480 px; nearest shared grid column 1499 px (delta 19 px) |
| A2 | fix-later | - | x=0 y=28 w=1500 h=4 | x=0 y=56 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=73 w=1500 h=11 | x=0 y=146 w=3000 h=22 | gap 11 px vs section median 8 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=89 w=1500 h=14 | x=0 y=178 w=3000 h=28 | gap 14 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=123 w=1500 h=13 | x=0 y=246 w=3000 h=26 | gap 13 px vs section median 8 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=140 w=1500 h=4 | x=0 y=280 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=170 w=1500 h=4 | x=0 y=340 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=178 w=1500 h=2 | x=0 y=356 w=3000 h=4 | gap 2 px vs section median 8 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=213 w=1500 h=4 | x=0 y=426 w=3000 h=8 | gap 4 px vs section median 8 px (delta 4 px) |

### `today-0.5-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=866 w=19 h=7 | x=24 y=1732 w=38 h=14 | left edge at 12 px; nearest shared grid column 10 px (delta 2 px) |
| A1 | fix-later | - | x=14 y=4 w=12 h=24 | x=28 y=8 w=24 h=48 | left edge at 14 px; nearest shared grid column 10 px (delta 4 px) |
| A1 | fix-later | - | x=35 y=848 w=9 h=9 | x=70 y=1696 w=18 h=18 | left edge at 35 px; nearest shared grid column 30 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=51 y=848 w=15 h=9 | x=102 y=1696 w=30 h=18 | left edge at 51 px; nearest shared grid column 30 px (delta 21 px) |
| A1 | accept-as-legacy | - | x=68 y=848 w=19 h=9 | x=136 y=1696 w=38 h=18 | left edge at 68 px; nearest shared grid column 30 px (delta 38 px) |
| A1 | accept-as-legacy | - | x=127 y=848 w=23 h=9 | x=254 y=1696 w=46 h=18 | left edge at 127 px; nearest shared grid column 30 px (delta 97 px) |
| A1 | accept-as-legacy | - | x=176 y=848 w=24 h=9 | x=352 y=1696 w=48 h=18 | left edge at 176 px; nearest shared grid column 30 px (delta 146 px) |
| A1 | accept-as-legacy | - | x=199 y=56 w=190 h=17 | x=398 y=112 w=380 h=34 | left edge at 199 px; nearest shared grid column 30 px (delta 169 px) |
| A1 | accept-as-legacy | - | x=206 y=882 w=36 h=16 | x=412 y=1764 w=72 h=32 | left edge at 206 px; nearest shared grid column 30 px (delta 176 px) |
| A1 | accept-as-legacy | - | x=210 y=848 w=8 h=9 | x=420 y=1696 w=16 h=18 | left edge at 210 px; nearest shared grid column 30 px (delta 180 px) |
| A1 | accept-as-legacy | - | x=334 y=882 w=6 h=16 | x=668 y=1764 w=12 h=32 | left edge at 334 px; nearest shared grid column 541 px (delta 207 px) |
| A1 | accept-as-legacy | - | x=375 y=866 w=10 h=7 | x=750 y=1732 w=20 h=14 | left edge at 375 px; nearest shared grid column 541 px (delta 166 px) |
| A1 | accept-as-legacy | - | x=393 y=56 w=102 h=17 | x=786 y=112 w=204 h=34 | left edge at 393 px; nearest shared grid column 541 px (delta 148 px) |
| A1 | accept-as-legacy | - | x=438 y=848 w=10 h=9 | x=876 y=1696 w=20 h=18 | left edge at 438 px; nearest shared grid column 541 px (delta 103 px) |
| A1 | accept-as-legacy | - | x=517 y=4 w=20 h=24 | x=1034 y=8 w=40 h=48 | left edge at 517 px; nearest shared grid column 541 px (delta 24 px) |
| A1 | fix-later | - | x=546 y=56 w=8 h=17 | x=1092 y=112 w=16 h=34 | left edge at 546 px; nearest shared grid column 542 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=575 y=117 w=10 h=7 | x=1150 y=234 w=20 h=14 | left edge at 575 px; nearest shared grid column 560 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=580 y=4 w=6 h=24 | x=1160 y=8 w=12 h=48 | left edge at 580 px; nearest shared grid column 599 px (delta 19 px) |
| A2 | fix-later | - | x=0 y=73 w=600 h=11 | x=0 y=146 w=1200 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=89 w=600 h=14 | x=0 y=178 w=1200 h=28 | gap 14 px vs section median 6 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=124 w=600 h=11 | x=0 y=248 w=1200 h=22 | gap 11 px vs section median 6 px (delta 5 px) |
| A2 | fix-later | - | x=0 y=178 w=600 h=2 | x=0 y=356 w=1200 h=4 | gap 2 px vs section median 6 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=213 w=600 h=3 | x=0 y=426 w=1200 h=6 | gap 3 px vs section median 6 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=842 w=600 h=6 | x=0 y=1684 w=1200 h=12 | gap 6 px vs section median 9 px (delta 3 px) |

### `today-3.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| C1 | fix-now | real-clip | x=999 y=209 w=1 h=133 | x=333 y=70 w=0 h=44 | content run touches the right edge (extent 133 px along the edge) — extent 133 px at/above the 12 px severing floor |
| C1 | fix-now | real-clip | x=999 y=429 w=1 h=106 | x=333 y=143 w=0 h=35 | content run touches the right edge (extent 106 px along the edge) — extent 106 px at/above the 12 px severing floor |
| A1 | accept-as-legacy | - | x=85 y=192 w=239 h=360 | x=28 y=64 w=80 h=120 | left edge at 85 px; nearest shared grid column 39 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=337 y=24 w=111 h=144 | x=112 y=8 w=37 h=48 | left edge at 337 px; nearest shared grid column 39 px (delta 298 px) |
| A1 | accept-as-legacy | - | x=348 y=192 w=288 h=360 | x=116 y=64 w=96 h=120 | left edge at 348 px; nearest shared grid column 39 px (delta 309 px) |
| A1 | accept-as-legacy | - | x=368 y=792 w=211 h=96 | x=123 y=264 w=70 h=32 | left edge at 368 px; nearest shared grid column 39 px (delta 329 px) |
| A1 | accept-as-legacy | - | x=475 y=24 w=170 h=144 | x=158 y=8 w=57 h=48 | left edge at 475 px; nearest shared grid column 39 px (delta 436 px) |
| A1 | accept-as-legacy | - | x=660 y=192 w=192 h=360 | x=220 y=64 w=64 h=120 | left edge at 660 px; nearest shared grid column 39 px (delta 621 px) |
| A2 | accept-as-legacy | - | x=0 y=168 w=1000 h=24 | x=0 y=56 w=333 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | - | x=0 y=552 w=1000 h=37 | x=0 y=184 w=333 h=12 | gap 37 px vs section median 56 px (delta 19 px) |

### `today-3.0-1500`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=85 y=192 w=388 h=360 | x=28 y=64 w=129 h=120 | left edge at 85 px; nearest shared grid column 39 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=497 y=192 w=438 h=360 | x=166 y=64 w=146 h=120 | left edge at 497 px; nearest shared grid column 535 px (delta 38 px) |
| A1 | accept-as-legacy | - | x=587 y=24 w=111 h=144 | x=196 y=8 w=37 h=48 | left edge at 587 px; nearest shared grid column 541 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=725 y=24 w=170 h=144 | x=242 y=8 w=57 h=48 | left edge at 725 px; nearest shared grid column 541 px (delta 184 px) |
| A1 | accept-as-legacy | - | x=959 y=192 w=266 h=360 | x=320 y=64 w=89 h=120 | left edge at 959 px; nearest shared grid column 541 px (delta 418 px) |
| A2 | accept-as-legacy | - | x=0 y=168 w=1500 h=24 | x=0 y=56 w=500 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | - | x=0 y=552 w=1500 h=37 | x=0 y=184 w=500 h=12 | gap 37 px vs section median 56 px (delta 19 px) |

### `today-3.0-600`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=85 y=192 w=239 h=360 | x=28 y=64 w=80 h=120 | left edge at 85 px; nearest shared grid column 39 px (delta 46 px) |
| A1 | accept-as-legacy | - | x=266 y=792 w=211 h=96 | x=89 y=264 w=70 h=32 | left edge at 266 px; nearest shared grid column 173 px (delta 93 px) |
| A1 | accept-as-legacy | - | x=311 y=24 w=170 h=144 | x=104 y=8 w=57 h=48 | left edge at 311 px; nearest shared grid column 173 px (delta 138 px) |
| A2 | accept-as-legacy | - | x=0 y=168 w=600 h=24 | x=0 y=56 w=200 h=8 | gap 24 px vs section median 56 px (delta 32 px) |
| A2 | accept-as-legacy | - | x=0 y=552 w=600 h=37 | x=0 y=184 w=200 h=12 | gap 37 px vs section median 56 px (delta 19 px) |
| A2 | accept-as-legacy | - | x=0 y=628 w=600 h=69 | x=0 y=209 w=200 h=23 | gap 69 px vs section median 56 px (delta 13 px) |

### `today-empty-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | accept-as-legacy | - | x=64 y=201 w=67 h=50 | x=64 y=201 w=67 h=50 | left edge at 64 px; nearest shared grid column 50 px (delta 14 px) |
| A1 | accept-as-legacy | - | x=227 y=201 w=66 h=50 | x=227 y=201 w=66 h=50 | left edge at 227 px; nearest shared grid column 50 px (delta 177 px) |
| A1 | accept-as-legacy | - | x=332 y=112 w=313 h=33 | x=332 y=112 w=313 h=33 | left edge at 332 px; nearest shared grid column 430 px (delta 98 px) |
| A1 | accept-as-legacy | - | x=347 y=864 w=66 h=32 | x=347 y=864 w=66 h=32 | left edge at 347 px; nearest shared grid column 430 px (delta 83 px) |
| A1 | accept-as-legacy | - | x=355 y=294 w=74 h=12 | x=355 y=294 w=74 h=12 | left edge at 355 px; nearest shared grid column 430 px (delta 75 px) |
| A1 | accept-as-legacy | - | x=358 y=201 w=630 h=50 | x=358 y=201 w=630 h=50 | left edge at 358 px; nearest shared grid column 430 px (delta 72 px) |
| A1 | accept-as-legacy | - | x=388 y=201 w=600 h=50 | x=388 y=201 w=600 h=50 | left edge at 388 px; nearest shared grid column 430 px (delta 42 px) |
| A1 | accept-as-legacy | - | x=402 y=294 w=27 h=12 | x=402 y=294 w=27 h=12 | left edge at 402 px; nearest shared grid column 430 px (delta 28 px) |
| A1 | accept-as-legacy | - | x=445 y=8 w=37 h=48 | x=445 y=8 w=37 h=48 | left edge at 445 px; nearest shared grid column 430 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=492 y=8 w=57 h=48 | x=492 y=8 w=57 h=48 | left edge at 492 px; nearest shared grid column 505 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=529 y=267 w=23 h=16 | x=529 y=267 w=23 h=16 | left edge at 529 px; nearest shared grid column 549 px (delta 20 px) |
| A1 | accept-as-legacy | - | x=653 y=112 w=172 h=33 | x=653 y=112 w=172 h=33 | left edge at 653 px; nearest shared grid column 549 px (delta 104 px) |
| A1 | accept-as-legacy | - | x=909 y=112 w=79 h=33 | x=909 y=112 w=79 h=33 | left edge at 909 px; nearest shared grid column 986 px (delta 77 px) |
| A1 | accept-as-legacy | - | x=914 y=112 w=74 h=33 | x=914 y=112 w=74 h=33 | left edge at 914 px; nearest shared grid column 986 px (delta 72 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=71 w=1000 h=13 | x=0 y=71 w=1000 h=13 | gap 13 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=145 w=1000 h=22 | x=0 y=145 w=1000 h=22 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | fix-later | - | x=0 y=177 w=1000 h=24 | x=0 y=177 w=1000 h=24 | gap 24 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=283 w=1000 h=11 | x=0 y=283 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |

### `today-habits-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=41 y=379 w=17 h=12 | x=41 y=379 w=17 h=12 | left edge at 41 px; nearest shared grid column 38 px (delta 3 px) |
| A1 | fix-later | - | x=44 y=193 w=52 h=41 | x=44 y=193 w=52 h=41 | left edge at 44 px; nearest shared grid column 38 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=332 y=112 w=313 h=33 | x=332 y=112 w=313 h=33 | left edge at 332 px; nearest shared grid column 430 px (delta 98 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 430 px (delta 85 px) |
| A1 | accept-as-legacy | - | x=355 y=324 w=74 h=12 | x=355 y=324 w=74 h=12 | left edge at 355 px; nearest shared grid column 430 px (delta 75 px) |
| A1 | accept-as-legacy | - | x=402 y=324 w=27 h=12 | x=402 y=324 w=27 h=12 | left edge at 402 px; nearest shared grid column 430 px (delta 28 px) |
| A1 | accept-as-legacy | - | x=445 y=8 w=37 h=48 | x=445 y=8 w=37 h=48 | left edge at 445 px; nearest shared grid column 430 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=487 y=193 w=394 h=41 | x=487 y=193 w=394 h=41 | left edge at 487 px; nearest shared grid column 471 px (delta 16 px) |
| A1 | accept-as-legacy | - | x=492 y=8 w=57 h=48 | x=492 y=8 w=57 h=48 | left edge at 492 px; nearest shared grid column 505 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=529 y=297 w=23 h=16 | x=529 y=297 w=23 h=16 | left edge at 529 px; nearest shared grid column 549 px (delta 20 px) |
| A1 | accept-as-legacy | - | x=653 y=112 w=172 h=33 | x=653 y=112 w=172 h=33 | left edge at 653 px; nearest shared grid column 549 px (delta 104 px) |
| A1 | accept-as-legacy | - | x=893 y=193 w=23 h=41 | x=893 y=193 w=23 h=41 | left edge at 893 px; nearest shared grid column 998 px (delta 105 px) |
| A1 | accept-as-legacy | - | x=909 y=112 w=79 h=33 | x=909 y=112 w=79 h=33 | left edge at 909 px; nearest shared grid column 998 px (delta 89 px) |
| A1 | accept-as-legacy | - | x=914 y=112 w=74 h=33 | x=914 y=112 w=74 h=33 | left edge at 914 px; nearest shared grid column 998 px (delta 84 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=8 | x=0 y=56 w=1000 h=8 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=71 w=1000 h=13 | x=0 y=71 w=1000 h=13 | gap 13 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=145 w=1000 h=22 | x=0 y=145 w=1000 h=22 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=234 w=1000 h=4 | x=0 y=234 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=313 w=1000 h=11 | x=0 y=313 w=1000 h=11 | gap 11 px vs section median 16 px (delta 5 px) |

### `today-habits-kiosk-0.5-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=70 y=115 w=6 h=12 | x=140 y=230 w=12 h=24 | left edge at 70 px; nearest shared grid column 65 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=145 y=115 w=6 h=12 | x=290 y=230 w=12 h=24 | left edge at 145 px; nearest shared grid column 102 px (delta 43 px) |
| A1 | accept-as-legacy | - | x=152 y=115 w=9 h=12 | x=304 y=230 w=18 h=24 | left edge at 152 px; nearest shared grid column 102 px (delta 50 px) |
| A1 | accept-as-legacy | - | x=172 y=115 w=6 h=12 | x=344 y=230 w=12 h=24 | left edge at 172 px; nearest shared grid column 102 px (delta 70 px) |
| A1 | accept-as-legacy | - | x=182 y=115 w=7 h=12 | x=364 y=230 w=14 h=24 | left edge at 182 px; nearest shared grid column 102 px (delta 80 px) |
| A1 | accept-as-legacy | - | x=190 y=115 w=12 h=12 | x=380 y=230 w=24 h=24 | left edge at 190 px; nearest shared grid column 102 px (delta 88 px) |
| A1 | accept-as-legacy | - | x=267 y=115 w=6 h=12 | x=534 y=230 w=12 h=24 | left edge at 267 px; nearest shared grid column 339 px (delta 72 px) |
| A1 | fix-later | - | x=343 y=115 w=9 h=12 | x=686 y=230 w=18 h=24 | left edge at 343 px; nearest shared grid column 339 px (delta 4 px) |
| A1 | accept-as-legacy | - | x=368 y=115 w=6 h=12 | x=736 y=230 w=12 h=24 | left edge at 368 px; nearest shared grid column 356 px (delta 12 px) |
| A1 | accept-as-legacy | - | x=375 y=115 w=6 h=12 | x=750 y=230 w=12 h=24 | left edge at 375 px; nearest shared grid column 356 px (delta 19 px) |
| A1 | fix-later | - | x=409 y=306 w=6 h=12 | x=818 y=612 w=12 h=24 | left edge at 409 px; nearest shared grid column 406 px (delta 3 px) |
| A1 | fix-later | - | x=442 y=279 w=8 h=16 | x=884 y=558 w=16 h=32 | left edge at 442 px; nearest shared grid column 445 px (delta 3 px) |
| A1 | fix-later | - | x=515 y=279 w=8 h=16 | x=1030 y=558 w=16 h=32 | left edge at 515 px; nearest shared grid column 512 px (delta 3 px) |
| A1 | fix-later | - | x=554 y=279 w=7 h=16 | x=1108 y=558 w=14 h=32 | left edge at 554 px; nearest shared grid column 549 px (delta 5 px) |
| A1 | accept-as-legacy | - | x=617 y=306 w=6 h=12 | x=1234 y=612 w=12 h=24 | left edge at 617 px; nearest shared grid column 563 px (delta 54 px) |
| A1 | accept-as-legacy | - | x=734 y=334 w=6 h=10 | x=1468 y=668 w=12 h=20 | left edge at 734 px; nearest shared grid column 563 px (delta 171 px) |
| A1 | accept-as-legacy | - | x=764 y=334 w=6 h=10 | x=1528 y=668 w=12 h=20 | left edge at 764 px; nearest shared grid column 563 px (delta 201 px) |
| A1 | accept-as-legacy | - | x=884 y=220 w=12 h=43 | x=1768 y=440 w=24 h=86 | left edge at 884 px; nearest shared grid column 969 px (delta 85 px) |
| A1 | accept-as-legacy | - | x=908 y=175 w=7 h=41 | x=1816 y=350 w=14 h=82 | left edge at 908 px; nearest shared grid column 969 px (delta 61 px) |
| A1 | accept-as-legacy | - | x=916 y=220 w=70 h=43 | x=1832 y=440 w=140 h=86 | left edge at 916 px; nearest shared grid column 969 px (delta 53 px) |
| A1 | accept-as-legacy | - | x=925 y=84 w=9 h=12 | x=1850 y=168 w=18 h=24 | left edge at 925 px; nearest shared grid column 969 px (delta 44 px) |
| A1 | accept-as-legacy | - | x=961 y=8 w=10 h=48 | x=1922 y=16 w=20 h=96 | left edge at 961 px; nearest shared grid column 969 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=56 w=1000 h=8 | x=0 y=112 w=2000 h=16 | gap 8 px vs section median 16 px (delta 8 px) |
| A2 | fix-later | - | x=0 y=71 w=1000 h=13 | x=0 y=142 w=2000 h=26 | gap 13 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=96 w=1000 h=19 | x=0 y=192 w=2000 h=38 | gap 19 px vs section median 16 px (delta 3 px) |
| A2 | fix-later | - | x=0 y=127 w=1000 h=22 | x=0 y=254 w=2000 h=44 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=216 w=1000 h=4 | x=0 y=432 w=2000 h=8 | gap 4 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=295 w=1000 h=11 | x=0 y=590 w=2000 h=22 | gap 11 px vs section median 16 px (delta 5 px) |

### `track-entries-1.0-1000`

| invariant | grade | classification | region (phys px) | logical | metric |
|---|---|---|---|---|---|
| A1 | fix-later | - | x=12 y=156 w=490 h=36 | x=12 y=156 w=490 h=36 | left edge at 12 px; nearest shared grid column 9 px (delta 3 px) |
| A1 | fix-later | - | x=16 y=864 w=32 h=32 | x=16 y=864 w=32 h=32 | left edge at 16 px; nearest shared grid column 22 px (delta 6 px) |
| A1 | accept-as-legacy | - | x=99 y=204 w=893 h=33 | x=99 y=204 w=893 h=33 | left edge at 99 px; nearest shared grid column 114 px (delta 15 px) |
| A1 | accept-as-legacy | - | x=135 y=373 w=20 h=81 | x=135 y=373 w=20 h=81 | left edge at 135 px; nearest shared grid column 116 px (delta 19 px) |
| A1 | accept-as-legacy | - | x=158 y=373 w=15 h=81 | x=158 y=373 w=15 h=81 | left edge at 158 px; nearest shared grid column 171 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=221 y=373 w=15 h=81 | x=221 y=373 w=15 h=81 | left edge at 221 px; nearest shared grid column 234 px (delta 13 px) |
| A1 | accept-as-legacy | - | x=345 y=864 w=71 h=32 | x=345 y=864 w=71 h=32 | left edge at 345 px; nearest shared grid column 246 px (delta 99 px) |
| A1 | accept-as-legacy | - | x=372 y=71 w=60 h=20 | x=372 y=71 w=60 h=20 | left edge at 372 px; nearest shared grid column 492 px (delta 120 px) |
| A1 | accept-as-legacy | - | x=419 y=71 w=13 h=20 | x=419 y=71 w=13 h=20 | left edge at 419 px; nearest shared grid column 492 px (delta 73 px) |
| A1 | accept-as-legacy | - | x=424 y=8 w=56 h=48 | x=424 y=8 w=56 h=48 | left edge at 424 px; nearest shared grid column 492 px (delta 68 px) |
| A1 | accept-as-legacy | - | x=546 y=71 w=19 h=20 | x=546 y=71 w=19 h=20 | left edge at 546 px; nearest shared grid column 515 px (delta 31 px) |
| A2 | fix-later | - | x=0 y=91 w=1000 h=20 | x=0 y=91 w=1000 h=20 | gap 20 px vs section median 16 px (delta 4 px) |
| A2 | accept-as-legacy | - | x=0 y=120 w=1000 h=7 | x=0 y=120 w=1000 h=7 | gap 7 px vs section median 16 px (delta 9 px) |
| A2 | fix-later | - | x=0 y=192 w=1000 h=12 | x=0 y=192 w=1000 h=12 | gap 12 px vs section median 16 px (delta 4 px) |
| A2 | fix-later | - | x=0 y=237 w=1000 h=23 | x=0 y=237 w=1000 h=23 | gap 23 px vs section median 16 px (delta 7 px) |
| A2 | accept-as-legacy | - | x=0 y=269 w=1000 h=35 | x=0 y=269 w=1000 h=35 | gap 35 px vs section median 16 px (delta 19 px) |
| A2 | accept-as-legacy | - | x=0 y=305 w=1000 h=2 | x=0 y=305 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=309 w=1000 h=2 | x=0 y=309 w=1000 h=2 | gap 2 px vs section median 16 px (delta 14 px) |
| A2 | accept-as-legacy | - | x=0 y=312 w=1000 h=28 | x=0 y=312 w=1000 h=28 | gap 28 px vs section median 16 px (delta 12 px) |
| A2 | fix-later | - | x=0 y=454 w=1000 h=22 | x=0 y=454 w=1000 h=22 | gap 22 px vs section median 16 px (delta 6 px) |
| A2 | accept-as-legacy | - | x=0 y=485 w=1000 h=3 | x=0 y=485 w=1000 h=3 | gap 3 px vs section median 16 px (delta 13 px) |
| A2 | accept-as-legacy | - | x=0 y=860 w=1000 h=4 | x=0 y=860 w=1000 h=4 | gap 4 px vs section median 16 px (delta 12 px) |
| T1 | fix-later | - | x=0 y=340 w=1000 h=17 | x=0 y=340 w=1000 h=17 | text line height 17 px → em ≈ 24.3 logical px; nearest ladder rung 20 px |

## Recorded skips (honest-skip, never silent)

- `focus-overlay-1.0-1000` — T1: skipped (n=1 lines; needs ≥ 5)
- `task-view-1.5-900` — T1: skipped (n=3 lines; needs ≥ 5)
- `task-view-1.5-500` — T1: skipped (n=2 lines; needs ≥ 5)
- `task-view-3.0-900` — T1: skipped (n=2 lines; needs ≥ 5)
- `task-view-3.0-500` — T1: skipped (n=2 lines; needs ≥ 5)
- `action-menu-1.0-1000` — T1: skipped (n=4 lines; needs ≥ 5)
- `alarm-window-1.0-1000` — T1: skipped (n=1 lines; needs ≥ 5)
- `alarm-window-1.0-600` — T1: skipped (n=1 lines; needs ≥ 5)
- `plan-next-day-1.0-600` — T1: skipped (n=3 lines; needs ≥ 5)
- `settings-1.0-600` — T1: skipped (n=3 lines; needs ≥ 5)
- `action-menu-1.0-600` — T1: skipped (n=3 lines; needs ≥ 5)
- `alarm-window-1.0-1500` — T1: skipped (n=3 lines; needs ≥ 5)
- `action-menu-1.0-1500` — T1: skipped (n=4 lines; needs ≥ 5)

