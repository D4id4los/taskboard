#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""layout_review.py — offline layout-review analyzer over the golden corpus.

Advisory-first (chunk G3, #1313; plan
.artifacts/plans/2026-11-25-layout-review-analyzer-plan.md, decisions D1-D9):
this tool REPORTS layout-invariant findings over the committed goldens; it
never gates — no CI wiring, no nextest test, no pre-commit hook. The exit
code is 0 whenever the run completes; nonzero only for tool errors
(unreadable corpus, manifest mismatch).

Usage:
    layout_review.py <goldens-dir> [--out findings.toml]
    layout_review.py --emit-md <findings.toml> <report.md>
    layout_review.py --self-test

Invariant catalogue (plan §3, tier-2 additions per the analyzer-invariant-tier2 plan):
    A1  alignment grid       — singleton left edges off the shared grid
    A2  uniform gaps         — per-section inter-band gap outliers
    T1  font-size ladder     — text-line sizes off the Theme TextRole ladder
                                 (tier-2: connected-component glyph-line estimator)
    M1  margin conformance   — outer margins off the space-* ladder
    C1  clip sentinel        — content runs touching a canvas edge
    Q1  text contrast floor  — pixel-measured ink/background WCAG 2.x ratio
                                 vs the role-class floor (tier 2, #1410)
    I1  icon-size conformance— icon bounding boxes vs Theme.icon-size
                                 12 logical px ±2 (tier 2, #1409; scope:
                                 icon-probe-* goldens only)
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

import numpy as np
from PIL import Image

# ---------------------------------------------------------------------------
# Theme ladders (D2): recorded constants, each annotated with its
# ui/theme.slint source. If theme.slint changes a token, update the constant
# in the same commit — regenerating the example report is the tripwire.
SPACE_LADDER = [4, 8, 12, 16, 24, 32, 48, 64, 96]  # space-1..8 + space-15
TEXT_LADDER = [11, 12, 12.5, 14, 16, 20, 30]  # badge, caption, mono, body, heading, title, display

# ---------------------------------------------------------------------------
# Detection thresholds (D7): every constant is evidence-commented and
# expressed in logical px, multiplied by the manifest scale before use.
# Calibrated once on the 63-golden corpus (2026-11-25 run; see the report's
# calibration note), never widened per-finding.

# Content floor: a pixel is "content" when any RGB channel deviates from the
# sampled modal background by more than this. Calibration measured the
# corner-sampled background at RGB (29,32,40) corpus-wide with flat
# margins; 12/255 sits well above the observed ±2 anti-aliasing noise
# floor and well below the dimmest text ink (~60 luma delta).
CONTENT_FLOOR = 12

# A1: a column-projection gradient must exceed this to count as a vertical
# block edge; logical px of content-height appearing per column step.
# 8 logical px filters single hairlines (1 px) and glyph-level columns
# (< 8 px tall) while keeping every card/pane/text-block boundary
# (calibration: minimum real boundary height ~16 logical px).
EDGE_FLOOR_LOGICAL = 8.0

# A1: a block narrower or shorter than this is glyph/chip noise, not a
# layout block (plan §3 A1; height floor added at calibration — badge and
# icon chips at ≤ 11 logical px tall dominated the raw singleton edges).
MIN_BLOCK_WIDTH_LOGICAL = 12.0
MIN_BLOCK_HEIGHT_LOGICAL = 12.0

# A1: blocks wider than this fraction of the canvas are full-bleed panes
# (headers, backdrops) — their left edge is the window edge, not a grid
# member. Calibration: every full-width surface element sits at > 0.9 W.
FULL_BLEED_FRACTION = 0.9

# A2: minimum content pixels per row for the row to join a band;
# a fraction of canvas width. 0.004 (≈2.4 px at 600 wide) keeps even a
# lone badge row in a band while dropping isolated antialiasing specks.
BAND_FLOOR_FRACTION = 0.004

# A2: gaps above this logical size start a new section (space-15 = 96,
# the largest spacing token; inter-section gaps are deliberately large).
SECTION_GAP_CEIL_LOGICAL = 96.0

# A2/A1: cluster/outlier tolerance in logical px (plan §3: 2 px × scale).
CLUSTER_TOL_LOGICAL = 2.0

# A2: the tolerance never drops below this many physical px (calibration:
# at scale 0.5 one physical px of raster quantization is 2 logical px, so
# the bare 2-px × 0.5 = 1-px tolerance fired on pixel-rounding alone).
GAP_TOL_FLOOR_PX = 2.0

# A2: a section needs at least this many gaps before uniformity is judged
# (plan §3 A2: median membership ≥ 3 bands).
MIN_SECTION_GAPS = 3

# T1: a text-line component must be at least this many physical px tall to
# be a line candidate (drops 1-px hairline noise).
MIN_LINE_RUN_PX = 3

# T1: minimum text lines in a golden for the height estimator to be
# honest (plan §3 T1: population suppression, recorded as a skip).
T1_MIN_LINES = 5

# T1: fraction of a row-run's height attributed to the em box
# (x-height-to-cap span ≈ 0.70 em; plan §3 T1).
EM_FRACTION = 0.70

# T1 (tier-2 glyph classification): a connected component taller than this
# logical height is a block/icon/thumbnail, not a glyph cluster. Named
# constant sits just below the display rung's tolerance ceiling
# (30 × 1.15 = 34.5). Calibration: the old row-run method measured em
# estimates up to 1806 logical px on non-text bands (chips, icon walls,
# captures); the component-level classification removes that garbage at
# the source instead of excluding it after the estimate.
GLYPH_MAX_HEIGHT_LOGICAL = 34.0

# T1 (tier-2 glyph classification): a glyph cluster wider than this logical
# width is a rule/divider or a connected icon run, not text ink. Calibration:
# icons in the kinds wall are ≤ 12 logical px wide and dividers are
# wide-and-thin; ~3 em of the display rung caps the honest glyph-cluster width.
GLYPH_MAX_WIDTH_LOGICAL = 40.0

# T1 (tier-2 line grouping): components whose vertical centres differ by at
# most this fraction of the taller component's height AND whose horizontal
# gap is at most LINE_X_GAP_LOGICAL merge into one text line. Calibration:
# 0.35 keeps ascender/descender-split glyphs of one line together while
# separating a 21-px title line from an 8-px caption line (centre delta
# ~14.5 px > 0.35 × 21 = 7.35 px) — the mixed-band case the row-run method
# reported as one ~41-px union.
LINE_CENTRE_TOL_FRACTION = 0.35

# T1 (tier-2 line grouping): word/letter-spacing slop — components with at
# most this many logical px of horizontal gap join the same line.
LINE_X_GAP_LOGICAL = 6.0

# T1: tolerance when mapping a measured em to the TextRole ladder.
# Calibration: rung separation is ≥ 12% (12.5→14); ±15% absorbs the
# antialiasing slop measured in the calibration run (≤ 1 physical px on
# line heights at every corpus scale) without overlapping rungs.
TEXT_TOL_FRACTION = 0.15

# Q1 (WCAG role-class floors, #1410; method per token_tests.rs): body-size
# text ≥ 4.5:1, large text ≥ 3.0:1. The large-text class applies at
# em ≥ LARGE_TEXT_EM_LOGICAL (the title/display rungs); the NonText class
# has no pixel analogue in Q1 (icons are I1's subject).
BODY_MIN_RATIO = 4.5
LARGE_MIN_RATIO = 3.0
LARGE_TEXT_EM_LOGICAL = 20.0

# Q1 sampling: the line bounding box is padded this many physical px before
# ink/background sampling (catches antialiased line edges without leaking
# neighbouring content).
Q1_BOX_PAD_PX = 2

# Q1 sampling: ink is the modal colour of the extreme tail of content
# pixels by channel deviation from the local background. Calibration
# (2026-11-25 corpus run): the first implementation kept the extreme 25%
# and the measured ink modes landed on antialiasing half-blends (e.g.
# (79,83,95) — a ~30% blend of text-primary into the (29,32,40) canvas),
# sagging known-compliant text-secondary (7.1:1 declared) to a measured
# median of 2.6:1 — a flood of 1168 findings, the sampler bug the plan's
# stage-3 trap §5.3 names, not 63 goldens' worth of accessibility debt.
# The extreme 5% tail isolates core glyph pixels (fully-inked stems) on
# every corpus scale ≥ 1.0.
Q1_INK_EXTREME_PERCENTILE = 95.0

# Q1 sampling: lines shorter than this many physical px are skipped —
# below it a rendered glyph carries no core ink pixels (at scale 0.5 a
# 14-px-em body line is ~7 phys px tall and every pixel is a blend), so
# the measured ratio would report rasterization, not the token pair.
# Calibration: all sub-8-px lines measured sub-floor with modal inks at
# 30–60% blends; known-compliant text at scale ≥ 1.0 measures ≥ 8 px.
Q1_MIN_LINE_HEIGHT_PX = 8

# Q1 grading (calibration, 2026-11-25 corpus run): the overlay goldens
# (search-palette, capture-overlay, review-walk, recur-editor-modal,
# labels-overlay) render the backdrop content under the modal scrim
# (scrim #0b0d12 at 50% alpha, theme.slint:135) — the measured ink modes
# there sit exactly on the 50% blend of the text tokens (text-primary
# #e8eaf0 → (121,123,129); text-secondary #a6abbd → (88,91,103)), while
# the dimmest unscrimmed text token (text-secondary) measures ≥ 166 on
# its max channel. The 140 ceiling (mid-gap) classifies scrim-blend ink:
# those lines are deliberate modal de-emphasis, not a token defect — the
# rubric absorbs that legacy noise (accept-as-legacy) and the detector
# still reports every line.
SCRIM_INK_MAX_CHANNEL = 140

# I1 (#1409): icon components must measure this many logical px, within
# ICON_TOL_LOGICAL of it (Theme.icon-size, ui/theme.slint:92).
# Calibration (2026-11-25 corpus run): the fully-scaling kinds-wall icons
# measured exactly 12×12 phys at scale 1.0, 6×6 phys at scale 0.5 and
# 34×34 phys at scale 3.0 (11.3 logical) — inside ±2 logical everywhere;
# the off-token class measured 10×7 phys at identical coordinates in all
# three probe goldens — icon ink that does not scale with base-scale
# (3.3 logical px at scale 3.0), a real conformance gap per #1409.
ICON_SIZE_LOGICAL = 12.0
ICON_TOL_LOGICAL = 2.0
# I1: tolerance never drops below this many physical px (the GAP_TOL_FLOOR_PX
# lesson: at scale 0.5 one physical px of raster quantization is 2 logical px).
ICON_TOL_FLOOR_PX = 2.0
# I1: square-ish guard — aspect ratio w/h must fall inside this band, and a
# component shorter than this many physical px is an antialiasing speck.
ICON_ASPECT_MIN = 0.6
ICON_ASPECT_MAX = 1.67
ICON_MIN_HEIGHT_PX = 6

# M1: outer margin must sit within this logical distance of a space-*
# rung (plan §3 M1: ±1.5 px × scale).
MARGIN_TOL_LOGICAL = 1.5

# M1: margins at or below this logical size are full-bleed signatures,
# not drifted tokens — deliberate edge-to-edge content, exempt here and
# graded by the rubric (C1 covers the pathological cases).
FULL_BLEED_MARGIN_LOGICAL = 1.0

# C1: structural exemption (calibration 2026-11-25) — an edge-touching run
# spanning at least this fraction of its edge is deliberate layout (full-
# height side rails at extent ~788-846 px and window-chrome bands dominate
# the raw run population); only shorter runs are clip candidates.
EDGE_RUN_EXEMPT_FRACTION = 0.3

# C1: runs shorter than this many physical px are anti-aliasing noise of
# rounded window corners (calibration: 18 of the raw runs are 1-2 px).
C1_MIN_RUN_PX = 3

# C1 classification (#1418, the clip-sentinel law, round 8 L1): every
# emitted C1 finding carries a typed `classification` — exactly one of
# `real-clip` / `thumb` / `chrome-speck` — and the D5 rubric reads the
# classification instead of re-deriving prose. Calibration notes:
#
# SEVERING_EXTENT_PX: the fix-now floor (the severing class — flush
# panels, #1365/#1378). Calibrated on the round-7 corpus: every
# hand-graded fix-now C1 sits at ≥ 12 physical px, every
# accept-as-legacy at ≤ 8. The 12-px line stays in the metric strings
# as the calibration record.
SEVERING_EXTENT_PX = 12

# The scrollbar-thumb model is NOT a bare size threshold: a run
# classifies `thumb` only when a track-column signature accompanies it
# — a low-contrast (sub-content-floor, above-background) vertical band
# along the same edge, contiguous through the run's position, spanning
# at least TRACK_BAND_FRACTION of the edge (the track the thumb sits
# in; the corpus thumbs' tracks span 0.29–0.94 of the edge while no
# real-clip run's near-floor band reaches 0.25 with a sub-severing
# extent). A real 8-px flush panel sliver without a track therefore
# classifies `real-clip`, never `thumb`.
TRACK_BAND_FRACTION = 0.25

# The track band's deviation window: below the content floor (the
# track renders dimmer than content) but above the sampling noise
# (±2/255, the calibration run's floor) so a flat background column
# does not read as track.
TRACK_DEV_LOW = 2

# Chrome-speck ceiling: sub-severing runs at or below this extent are
# the window-chrome scale (the corpus specks are 3-4 px, the bottom-
# edge frame runs 3 px). A sub-severing run wider than the speck scale
# without a track is NOT a speck — it is a small real clip and the
# rubric's fix-later class owns it (the 6 px editor-family sever runs
# are the corpus population the old extent-only rubric blurred into
# accept-as-legacy; the classification makes them visible instead).
SPECK_EXTENT_MAX = 4

C1_CLASSIFICATIONS = ("real-clip", "thumb", "chrome-speck")

# ---------------------------------------------------------------------------


def read_manifest(goldens_dir: Path) -> list[dict]:
    with open(goldens_dir / "manifest.toml", "rb") as f:
        data = tomllib.load(f)
    entries = data.get("golden", [])
    if not entries:
        raise SystemExit(f"tool error: no [[golden]] entries in {goldens_dir}/manifest.toml")
    return entries


def load_golden(goldens_dir: Path, name: str) -> np.ndarray:
    path = goldens_dir / f"{name}.png"
    if not path.exists():
        raise SystemExit(f"tool error: golden {name} missing at {path}")
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.int16)


def content_mask(rgb: np.ndarray) -> np.ndarray:
    """Background-keyed content: deviation from the corner-sampled modal
    background above CONTENT_FLOOR on any channel."""
    h, w = rgb.shape[:2]
    corners = np.concatenate(
        [
            rgb[:8, :8].reshape(-1, 3),
            rgb[:8, w - 8 :].reshape(-1, 3),
            rgb[h - 8 :, :8].reshape(-1, 3),
            rgb[h - 8 :, w - 8 :].reshape(-1, 3),
        ]
    )
    bg = np.median(corners, axis=0)
    return (np.abs(rgb - bg).max(axis=2) > CONTENT_FLOOR), bg


def bands(row_counts: np.ndarray, floor: int) -> list[tuple[int, int]]:
    """Maximal runs of rows above `floor` (inclusive indices)."""
    out: list[tuple[int, int]] = []
    start = None
    for y, c in enumerate(row_counts):
        if c > floor and start is None:
            start = y
        elif c <= floor and start is not None:
            out.append((start, y - 1))
            start = None
    if start is not None:
        out.append((start, len(row_counts) - 1))
    return out


def connected_components(mask: np.ndarray) -> list[tuple[int, int, int, int]]:
    """8-connectivity connected components over a boolean mask, as
    (x0, y0, x1, y1) inclusive bounding boxes (tier-2 primitive, #1417).

    Two-pass row-run union-find: per-row runs of True are labelled and
    unioned with the previous row's overlapping runs (±1 px for the
    diagonal of the 8-connectivity). Components touching canvas edges are
    passed through unchanged — C1 owns edge semantics.
    """
    h, _w = mask.shape
    parent: list[int] = []
    boxes: list[list[int]] = []  # per root: [x0, y0, x1, y1]

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a: int, b: int) -> None:
        ra, rb = find(a), find(b)
        if ra == rb:
            return
        if ra > rb:
            ra, rb = rb, ra
        parent[rb] = ra
        boxes[ra] = [
            min(boxes[ra][0], boxes[rb][0]),
            min(boxes[ra][1], boxes[rb][1]),
            max(boxes[ra][2], boxes[rb][2]),
            max(boxes[ra][3], boxes[rb][3]),
        ]

    prev_runs: list[tuple[int, int, int]] = []  # (x0, x1, label)
    for y in range(h):
        xs = np.flatnonzero(mask[y])
        cur: list[tuple[int, int, int]] = []
        if xs.size:
            splits = np.flatnonzero(np.diff(xs) > 1)
            starts = np.concatenate(([0], splits + 1))
            ends = np.concatenate((splits, [xs.size - 1]))
            for s, e in zip(starts, ends):
                x0, x1 = int(xs[s]), int(xs[e])
                lbl = None
                for px0, px1, pl in prev_runs:
                    if px0 <= x1 + 1 and px1 >= x0 - 1:
                        if lbl is None:
                            lbl = pl
                        else:
                            union(lbl, pl)
                if lbl is None:
                    lbl = len(parent)
                    parent.append(lbl)
                    boxes.append([x0, y, x1, y])
                else:
                    r = find(lbl)
                    b = boxes[r]
                    b[0] = min(b[0], x0)
                    b[1] = min(b[1], y)
                    b[2] = max(b[2], x1)
                    b[3] = max(b[3], y)
                cur.append((x0, x1, lbl))
        prev_runs = cur
    return [
        (b[0], b[1], b[2], b[3])
        for i, b in enumerate(boxes)
        if find(i) == i
    ]


# ---------------------------------------------------------------------------
# WCAG 2.x colour math (Q1, #1410): transcribed byte-for-byte from
# crates/taskboard-ui-slint/src/presentation.rs relative_luminance /
# contrast_ratio — presentation.rs is the single definition of the
# formula in this repo (hoisted out of token_tests.rs by chunk L4,
# #1415; the test suite consumes it). The Rust helpers index hex-string
# slices at 0/2/4 (R, G, B — two hex digits per channel); here the
# tuple order is (r, g, b) with the same 0.2126/0.7152/0.0722 weight
# order. The 0.03928 linear arm, the 12.92 divisor, the 2.4 gamma and
# the 0.05 lift match presentation.rs exactly; the known-answer pins
# (21.0, 1.0) mirror its token_tests.rs wcag_math_is_sanity_anchored
# test.


def relative_luminance(rgb: tuple[int, int, int]) -> float:
    def channel(c: int) -> float:
        c = c / 255.0
        if c <= 0.03928:
            return c / 12.92
        return ((c + 0.055) / 1.055) ** 2.4

    return 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2])


def contrast_ratio(a: tuple[int, int, int], b: tuple[int, int, int]) -> float:
    la, lb = relative_luminance(a), relative_luminance(b)
    hi, lo = (la, lb) if la >= lb else (lb, la)
    return (hi + 0.05) / (lo + 0.05)


def _modal_colour(pixels: np.ndarray) -> tuple[int, int, int]:
    vals, counts = np.unique(pixels, axis=0, return_counts=True)
    return tuple(int(v) for v in vals[int(np.argmax(counts))])


# ---------------------------------------------------------------------------
# Detectors. Each returns a list of raw finding dicts (invariant, region in
# physical px, metric, threshold strings).


def detect_c1(rgb: np.ndarray, mask: np.ndarray, bg: np.ndarray) -> list[dict]:
    """INV-C1 clip sentinel: content runs touching a canvas edge that are
    neither structural (spanning a large fraction of the edge — side
    rails, window-chrome bands) nor anti-aliasing noise.

    Every emitted finding carries the typed `classification` (#1418):
    `real-clip` / `thumb` / `chrome-speck`. The scrollbar-thumb model
    is extent-band + track-column signature (see the calibration notes
    on TRACK_BAND_FRACTION), never a bare size threshold."""
    h, w = mask.shape
    findings = []

    edge_specs = [
        ("right", mask[:, w - 1], np.abs(rgb[:, w - 1] - bg).max(axis=1)),
        ("left", mask[:, 0], np.abs(rgb[:, 0] - bg).max(axis=1)),
        ("bottom", mask[h - 1, :], np.abs(rgb[h - 1, :] - bg).max(axis=1)),
        ("top", mask[0, :], np.abs(rgb[0, :] - bg).max(axis=1)),
    ]

    for name, line, dev in edge_specs:
        runs = []
        start = None
        for i, v in enumerate(line):
            if v and start is None:
                start = i
            elif not v and start is not None:
                runs.append((start, i - 1))
                start = None
        if start is not None:
            runs.append((start, len(line) - 1))
        for r0, r1 in runs:
            run_len = r1 - r0 + 1
            if run_len >= round(EDGE_RUN_EXEMPT_FRACTION * len(line)):
                continue  # structural band/rail, not a clip
            if run_len < C1_MIN_RUN_PX:
                continue  # corner anti-aliasing noise
            edge_len = len(line)
            classification, rationale = classify_c1_run(run_len, r0, r1, dev, edge_len)
            if axis_region := (
                (w - 1, r0, 1, run_len)
                if name == "right"
                else (0, r0, 1, run_len) if name == "left" else None
            ):
                region = axis_region
            else:
                region = (
                    (r0, h - 1, run_len, 1) if name == "bottom" else (r0, 0, run_len, 1)
                )
            findings.append(
                {
                    "invariant": "C1",
                    "extent": run_len,
                    "region": region,
                    "extent": run_len,
                    "classification": classification,
                    "rationale": rationale,
                    "metric": f"content run touches the {name} edge "
                    f"(extent {run_len} px along the edge)",
                    "threshold": f"structural exemption at ≥ {EDGE_RUN_EXEMPT_FRACTION:.0%} "
                    f"of the edge; min run {C1_MIN_RUN_PX} px; severing floor "
                    f"{SEVERING_EXTENT_PX} px; thumb band = sub-severing extent with a "
                    f"track-column signature ≥ {TRACK_BAND_FRACTION:.0%} of the edge",
                }
            )
    return findings


def classify_c1_run(
    run_len: int, r0: int, r1: int, dev: np.ndarray, edge_len: int
) -> tuple[str, str]:
    """The #1418 typed classifier for one edge-touching run.

    Returns (classification, rationale). Sub-severing runs split by the
    scrollbar-thumb model: a thumb needs the track-column signature —
    a contiguous near-floor band through the run's position spanning
    TRACK_BAND_FRACTION of the edge; without the track the run is a
    chrome speck (a deliberate small flush element at the frame
    periphery). Everything at or past the severing floor is a
    real-clip regardless of company."""
    if run_len >= SEVERING_EXTENT_PX:
        return (
            "real-clip",
            f"extent {run_len} px at/above the {SEVERING_EXTENT_PX} px severing floor",
        )
    band_a, band_b = _near_floor_band(dev, r0, r1)
    band_len = band_b - band_a + 1
    if band_len / edge_len >= TRACK_BAND_FRACTION:
        return (
            "thumb",
            f"extent {run_len} px inside the thumb band with a track-column "
            f"signature (contiguous near-floor band {band_len} px = "
            f"{band_len / edge_len:.0%} of the edge through the run)",
        )
    if run_len <= SPECK_EXTENT_MAX:
        return (
            "chrome-speck",
            f"sub-severing extent {run_len} px at the chrome scale with no "
            f"track-column signature (near-floor band {band_len} px = "
            f"{band_len / edge_len:.0%} of the edge)",
        )
    return (
        "real-clip",
        f"sub-severing extent {run_len} px with no track-column signature "
        f"(near-floor band {band_len} px = {band_len / edge_len:.0%} of the edge) "
        "and past the chrome-speck scale — a small real clip",
    )


def _near_floor_band(dev: np.ndarray, r0: int, r1: int) -> tuple[int, int]:
    """The contiguous band of near-floor deviation (TRACK_DEV_LOW ≤ dev ≤
    CONTENT_FLOOR) containing/adjacent to the run's extent — the
    scrollbar-track column signature probe."""
    n = len(dev)
    a = r0
    while a > 0 and TRACK_DEV_LOW <= dev[a - 1] <= CONTENT_FLOOR:
        a -= 1
    b = r1
    while b < n - 1 and TRACK_DEV_LOW <= dev[b + 1] <= CONTENT_FLOOR:
        b += 1
    return a, b


def detect_m1(mask: np.ndarray, scale: float) -> list[dict]:
    """INV-M1 margin conformance: outer margins vs the space-* ladder."""
    h, w = mask.shape
    rows = mask.any(axis=1)
    cols = mask.any(axis=0)
    if not rows.any() or not cols.any():
        return []
    top = int(np.argmax(rows))
    bottom = h - 1 - int(np.argmax(rows[::-1]))
    left = int(np.argmax(cols))
    right = w - 1 - int(np.argmax(cols[::-1]))
    margins = {
        "top": top,
        "bottom": h - 1 - bottom,
        "left": left,
        "right": w - 1 - right,
    }
    findings = []
    tol = MARGIN_TOL_LOGICAL * scale
    for side, phys in margins.items():
        logical = phys / scale
        if logical <= FULL_BLEED_MARGIN_LOGICAL:
            continue  # full-bleed signature; rubric territory, not a token drift
        dist = min(abs(logical - rung) for rung in SPACE_LADDER)
        if dist > MARGIN_TOL_LOGICAL:
            if side in ("top", "bottom"):
                region = (0, top if side == "top" else bottom, w, 1)
            else:
                region = (left if side == "left" else right, 0, 1, h)
            findings.append(
                {
                    "invariant": "M1",
                    "region": region,
                    "metric": f"{side} margin {logical:.1f} logical px "
                    f"({phys} px at scale {scale}); nearest rung "
                    f"{min(SPACE_LADDER, key=lambda r: abs(logical - r))} px",
                    "threshold": f"margin within {MARGIN_TOL_LOGICAL} logical px of a "
                    f"space-* rung {SPACE_LADDER}; full-bleed exempt ≤ "
                    f"{FULL_BLEED_MARGIN_LOGICAL} px",
                }
            )
    return findings


def detect_a1(mask: np.ndarray, scale: float) -> list[dict]:
    """INV-A1 alignment grid: per-block left-facing edges clustered across
    the golden's row bands; singleton clusters are off-grid blocks."""
    h, w = mask.shape
    row = mask.sum(axis=1).astype(np.int64)
    edge_floor = max(1, round(EDGE_FLOOR_LOGICAL * scale))
    tol = max(1, round(CLUSTER_TOL_LOGICAL * scale))
    min_w = round(MIN_BLOCK_WIDTH_LOGICAL * scale)

    # per-band edge instances: (x, band rows, block span estimate)
    instances = []  # (x, y0, y1)
    for b0, b1 in bands(row, max(1, int(BAND_FLOOR_FRACTION * w))):
        sub = mask[b0 : b1 + 1, :]
        col = sub.sum(axis=0).astype(np.int64)
        rising = [x + 1 for x in range(w - 1) if col[x + 1] - col[x] >= edge_floor]
        for x in rising:
            instances.append((x, b0, b1))
    if not instances:
        return []

    def cluster(vals: list[int], tol: int) -> list[list[int]]:
        vals = sorted(vals)
        groups = [[vals[0]]]
        for v in vals[1:]:
            if v - groups[-1][-1] <= tol:
                groups[-1].append(v)
            else:
                groups.append([v])
        return groups

    groups = cluster([x for x, _, _ in instances], tol)
    findings = []
    for g in groups:
        if len(g) != 1:
            continue
        left = g[0]
        y0, y1 = next((yy0, yy1) for x, yy0, yy1 in instances if x == left)
        # block span: nearest right-facing boundary within the band rows
        sub = mask[y0 : y1 + 1, :]
        col = sub.sum(axis=0).astype(np.int64)
        falling = [x for x in range(w - 1) if col[x] - col[x + 1] >= edge_floor and x >= left]
        if not falling:
            continue
        right = min(falling)
        width = right - left + 1
        height = y1 - y0 + 1
        if width < min_w or width > FULL_BLEED_FRACTION * w:
            continue
        if height < round(MIN_BLOCK_HEIGHT_LOGICAL * scale):
            continue
        grid = sorted(v for gg in groups if len(gg) > 1 for v in gg)
        near = min(grid, key=lambda gx: abs(gx - left)) if grid else None
        findings.append(
            {
                "invariant": "A1",
                "grid_delta": abs(left - near) if near is not None else None,
                "region": (left, y0, width, y1 - y0 + 1),
                "metric": f"left edge at {left} px; "
                + (
                    f"nearest shared grid column {near} px (delta {abs(left - near)} px)"
                    if near
                    else "no other left edge shares this column"
                ),
                "threshold": f"cluster tolerance {CLUSTER_TOL_LOGICAL} logical px × scale "
                f"{scale} = {tol} px; min block width {MIN_BLOCK_WIDTH_LOGICAL} logical px; "
                "grid min membership 2",
            }
        )
    return findings


def detect_a2(mask: np.ndarray, scale: float) -> list[dict]:
    """INV-A2 uniform gaps: per-section inter-band gap outliers."""
    h, w = mask.shape
    row = mask.sum(axis=1).astype(np.int64)
    floor = max(1, int(BAND_FLOOR_FRACTION * w))
    bs = bands(row, floor)
    if len(bs) < 2:
        return []
    # gaps between consecutive bands: (gap_px, upper band, lower band)
    gaps = []
    for i in range(len(bs) - 1):
        g = bs[i + 1][0] - bs[i][1] - 1
        gaps.append((g, bs[i], bs[i + 1]))
    # sections: interior gaps only — a gap above the ceiling is a section
    # boundary and is never itself judged
    ceil = SECTION_GAP_CEIL_LOGICAL * scale
    sections: list[list] = []
    cur: list = []
    for g in gaps:
        if g[0] > ceil:
            if cur:
                sections.append(cur)
            cur = []
        else:
            cur.append(g)
    if cur:
        sections.append(cur)

    tol = max(CLUSTER_TOL_LOGICAL * scale, GAP_TOL_FLOOR_PX)
    findings = []
    for sec in sections:
        if len(sec) < MIN_SECTION_GAPS:
            continue
        vals = sorted(g[0] for g in sec)
        med = vals[len(vals) // 2]
        for g, upper, lower in sec:
            if abs(g - med) > tol:
                y0, y1 = upper[1] + 1, lower[0] - 1
                findings.append(
                    {
                        "invariant": "A2",
                        "gap_delta": abs(g - med),
                        "region": (0, y0, w, max(1, y1 - y0 + 1)),
                        "metric": f"gap {g} px vs section median {med} px "
                        f"(delta {abs(g - med)} px)",
                        "threshold": f"outlier when |gap − median| > {CLUSTER_TOL_LOGICAL} "
                        f"logical px × scale {scale} = {tol:.0f} px; section ceiling "
                        f"{SECTION_GAP_CEIL_LOGICAL} logical px; min membership "
                        f"{MIN_SECTION_GAPS} gaps",
                    }
                )
    return findings


def classify_glyphs(components: list[tuple[int, int, int, int]], scale: float) -> list:
    """Tier-2 text-ness classification: a component is a glyph candidate
    when its height is at least MIN_LINE_RUN_PX and within
    GLYPH_MAX_HEIGHT_LOGICAL / GLYPH_MAX_WIDTH_LOGICAL (× scale) — the
    component-level replacement for the deleted MAX_LINE_EM_LOGICAL cap
    (blocks, icons, thumbnails and divider rules classify out here, where
    the row-run method could not see the split)."""
    glyphs = []
    for x0, y0, x1, y1 in components:
        cw, ch = x1 - x0 + 1, y1 - y0 + 1
        if ch < MIN_LINE_RUN_PX:
            continue
        if ch > GLYPH_MAX_HEIGHT_LOGICAL * scale:
            continue
        if cw > GLYPH_MAX_WIDTH_LOGICAL * scale:
            continue
        glyphs.append((x0, y0, x1, y1))
    return glyphs


def group_lines(glyphs: list[tuple[int, int, int, int]], scale: float) -> list:
    """Merge glyph clusters into text lines: vertical centres within
    LINE_CENTRE_TOL_FRACTION of the taller member's height AND horizontal
    gap within LINE_X_GAP_LOGICAL (× scale). Returns one box per line:
    (x0, y0, x1, y1, member_count)."""
    xgap = LINE_X_GAP_LOGICAL * scale
    lines: list[list[tuple[int, int, int, int]]] = []
    for g in sorted(glyphs, key=lambda c: (c[1], c[0])):
        merged = False
        for line in lines:
            ly0 = min(c[1] for c in line)
            ly1 = max(c[3] for c in line)
            lx0 = min(c[0] for c in line)
            lx1 = max(c[2] for c in line)
            taller = max(g[3] - g[1] + 1, ly1 - ly0 + 1)
            centres = abs((g[1] + g[3]) / 2 - (ly0 + ly1) / 2)
            gap = max(g[0] - lx1, lx0 - g[2], 0)
            if centres <= LINE_CENTRE_TOL_FRACTION * taller and gap <= xgap:
                line.append(g)
                merged = True
                break
        if not merged:
            lines.append([g])
    return [
        (
            min(c[0] for c in line),
            min(c[1] for c in line),
            max(c[2] for c in line),
            max(c[3] for c in line),
            len(line),
        )
        for line in lines
    ]


def detect_t1(
    mask: np.ndarray, scale: float, components: list[tuple[int, int, int, int]] | None = None
) -> tuple[list[dict], str | None, list]:
    """INV-T1 font-size ladder (tier-2 connected-component estimator).

    Glyph-ness is a classification of connected components (never an
    exclusion cap); components group into lines; each line's em is
    height / EM_FRACTION ÷ scale. Returns (findings, skip_note, lines) —
    the lines feed Q1 regardless of the ladder population (suppression
    covers the ladder judgment's honesty only, never the lines
    themselves). Below T1_MIN_LINES lines the ladder judgment suppresses
    with an explicit, recorded note (an honest skip — never silent).
    """
    if components is None:
        components = connected_components(mask)
    lines = group_lines(classify_glyphs(components, scale), scale)
    if len(lines) < T1_MIN_LINES:
        return [], f"T1: skipped (n={len(lines)} lines; needs ≥ {T1_MIN_LINES})", lines

    findings = []
    for x0, y0, x1, y1, ncomp in lines:
        height = y1 - y0 + 1
        em = height / EM_FRACTION / scale  # logical em estimate
        hit = None
        for rung in TEXT_LADDER:
            if abs(em - rung) <= rung * TEXT_TOL_FRACTION:
                hit = rung
                break
        if hit is None:
            nearest = min(TEXT_LADDER, key=lambda r: abs(em - r))
            findings.append(
                {
                    "invariant": "T1",
                    "em": em,
                    "region": (x0, y0, x1 - x0 + 1, height),
                    "metric": f"text line height {height} px ({ncomp} component"
                    f"{'s' if ncomp != 1 else ''}) → em ≈ {em:.1f} logical px; "
                    f"nearest ladder rung {nearest} px",
                    "threshold": f"em within {TEXT_TOL_FRACTION:.0%} of a TextRole rung "
                    f"{TEXT_LADDER}; em = line height / {EM_FRACTION} ÷ scale {scale} "
                    "(connected-component glyph-line estimator)",
                }
            )
    return findings, None, lines


def detect_q1(
    mask: np.ndarray,
    lines: list,
    scale: float,
    rgb: np.ndarray,
    global_bg: np.ndarray,
) -> list[dict]:
    """INV-Q1 text contrast floor (#1410): per T1 text line, sample ink vs
    local background and judge the WCAG 2.x contrast ratio against the
    role-class floor (Body 4.5:1, Large 3.0:1 at em ≥ 20). Ink is the
    modal colour of the extreme tail of content pixels by channel
    deviation (core glyph pixels, not antialiased edges — see the
    Q1_INK_EXTREME_PERCENTILE calibration note); the local background is
    the modal colour of non-content pixels in the padded line box
    (fallback: the global corner-sampled background)."""
    h, w = mask.shape
    findings = []
    for x0, y0, x1, y1, _ncomp in lines:
        if y1 - y0 + 1 < Q1_MIN_LINE_HEIGHT_PX:
            continue  # no core ink pixels at this raster size (calibrated skip)
        px0, py0 = max(0, x0 - Q1_BOX_PAD_PX), max(0, y0 - Q1_BOX_PAD_PX)
        px1, py1 = min(w - 1, x1 + Q1_BOX_PAD_PX), min(h - 1, y1 + Q1_BOX_PAD_PX)
        box = rgb[py0 : py1 + 1, px0 : px1 + 1].reshape(-1, 3)
        boxmask = mask[py0 : py1 + 1, px0 : px1 + 1].reshape(-1)
        content = box[boxmask]
        if content.size == 0:
            continue
        noncontent = box[~boxmask]
        bg = (
            _modal_colour(noncontent)
            if noncontent.size
            else tuple(int(v) for v in global_bg)
        )
        dev = np.abs(content.astype(np.int64) - np.array(bg)).max(axis=1)
        extreme = content[dev >= np.percentile(dev, Q1_INK_EXTREME_PERCENTILE)]
        ink = _modal_colour(extreme)
        ratio = contrast_ratio(ink, bg)
        em = (y1 - y0 + 1) / EM_FRACTION / scale
        large = em >= LARGE_TEXT_EM_LOGICAL
        floor = LARGE_MIN_RATIO if large else BODY_MIN_RATIO
        if ratio < floor:
            scrim = max(ink) <= SCRIM_INK_MAX_CHANNEL
            findings.append(
                {
                    "invariant": "Q1",
                    "region": (px0, py0, px1 - px0 + 1, py1 - py0 + 1),
                    "metric": f"ink RGB {ink} on background RGB {bg} → contrast "
                    f"{ratio:.1f}:1; role class {'Large' if large else 'Body'}"
                    + ("; scrim-blend ink" if scrim else ""),
                    "threshold": f"WCAG 2.x: Body ≥ {BODY_MIN_RATIO}:1 (em < "
                    f"{LARGE_TEXT_EM_LOGICAL}), Large ≥ {LARGE_MIN_RATIO}:1 (em ≥ "
                    f"{LARGE_TEXT_EM_LOGICAL}); method per token_tests.rs",
                    "scrim": scrim,
                }
            )
    return findings


def detect_i1(
    components: list[tuple[int, int, int, int]],
    scale: float,
    name: str,
    lines: list | None = None,
) -> list[dict]:
    """INV-I1 icon-size conformance (#1409): icon components (square-ish,
    ≥ ICON_MIN_HEIGHT_PX physical px tall, not claimed by a T1 glyph
    line — icons are non-text, so text-line members classify out) must
    measure ICON_SIZE_LOGICAL ± ICON_TOL_LOGICAL logical px.
    Scope guard: icon-probe-* goldens only — elsewhere glyph-sized chips
    are not icon tokens and would drown the signal."""
    if not name.startswith("icon-probe-"):
        return []
    tol_phys = max(ICON_TOL_LOGICAL * scale, ICON_TOL_FLOOR_PX)
    findings = []
    for x0, y0, x1, y1 in components:
        # A line claim only bites for multi-component lines (real grouped
        # text ink): a single-component "line" is the icon component
        # itself — glyph-height ≠ text, and claiming it here would make
        # I1 dead code on every sparse probe golden.
        claimed = False
        for lx0, ly0, lx1, ly1, n in lines or []:
            if n > 1 and x0 >= lx0 and y0 >= ly0 and x1 <= lx1 and y1 <= ly1:
                claimed = True
                break
        if claimed:
            continue
        cw, ch = x1 - x0 + 1, y1 - y0 + 1
        if ch < ICON_MIN_HEIGHT_PX:
            continue
        aspect = cw / ch
        if not (ICON_ASPECT_MIN <= aspect <= ICON_ASPECT_MAX):
            continue
        logical_w, logical_h = cw / scale, ch / scale
        if abs(logical_w - ICON_SIZE_LOGICAL) > tol_phys / scale or abs(
            logical_h - ICON_SIZE_LOGICAL
        ) > tol_phys / scale:
            findings.append(
                {
                    "invariant": "I1",
                    "region": (x0, y0, cw, ch),
                    "metric": f"icon bounding box {cw} × {ch} phys px → "
                    f"{logical_w:.1f} × {logical_h:.1f} logical px",
                    "threshold": f"icon-size {ICON_SIZE_LOGICAL} logical px × base-scale "
                    f"within ±{ICON_TOL_LOGICAL} logical px (tolerance floor "
                    f"{ICON_TOL_FLOOR_PX} phys px; aspect band "
                    f"{ICON_ASPECT_MIN}–{ICON_ASPECT_MAX}); scope: icon-probe goldens only",
                }
            )
    return findings


# ---------------------------------------------------------------------------


def logical(region: tuple[int, int, int, int], scale: float) -> dict:
    x, y, w, h = region
    return {
        "x": round(x / scale),
        "y": round(y / scale),
        "w": round(w / scale),
        "h": round(h / scale),
    }


def grade_for(finding: dict, golden: str = "") -> str:
    """The deterministic D5 rubric (the whole grading step is now
    reproducible — the round-7 hand grades were this rubric applied by
    hand; the #1418 work promotes C1's half from prose to the typed
    classification).

    C1 consumes the classification (law [L](c)): thumb / chrome-speck
    → accept-as-legacy (rationale emitted from the classification);
    real-clip at/above the severing floor → fix-now; real-clip below
    it → fix-later (a real drift smaller than the severing class —
    visible, never blurred into the thumb band). The other invariants
    keep their recorded rule strings:
    A1 — delta to the nearest shared grid column ≤ 6 px → fix-later,
    larger (or no shared column) → accept-as-legacy;
    A2 — |gap − median| ≤ 8 px → fix-later, larger → accept-as-legacy;
    T1 — em ≥ 20 logical px → fix-later, below → accept-as-legacy;
    Q1 — sub-floor → fix-now, except the scrim-blend ink class →
    accept-as-legacy (calibration evidence in the constant's comment);
    M1 — icon-probe and board-drag-indicator margins →
    accept-as-legacy (the manifest's state note), the rest → fix-later.
    """
    inv = finding["invariant"]
    if inv == "C1":
        cls = finding.get("classification", "real-clip")
        if cls in ("thumb", "chrome-speck"):
            return "accept-as-legacy"
        return "fix-now" if finding["extent"] >= SEVERING_EXTENT_PX else "fix-later"
    if inv == "A1":
        delta = finding.get("grid_delta")
        return "fix-later" if delta is not None and delta <= 6 else "accept-as-legacy"
    if inv == "A2":
        return "fix-later" if finding.get("gap_delta", 0) <= 8 else "accept-as-legacy"
    if inv == "T1":
        return "fix-later" if finding.get("em", 0) >= 20 else "accept-as-legacy"
    if inv == "Q1":
        return "accept-as-legacy" if finding.get("scrim") else "fix-now"
    if inv == "M1":
        legacy = ("icon-probe", "board-drag-indicator")
        return "accept-as-legacy" if golden.startswith(legacy) else "fix-later"
    return "fix-later"  # I1


def analyze_golden(
    rgb: np.ndarray, scale: float, name: str = "", golden: str = ""
) -> tuple[list[dict], list[str]]:
    mask, bg = content_mask(rgb)
    components = connected_components(mask)  # one pass feeds T1, Q1, I1
    findings: list[dict] = []
    findings += detect_c1(rgb, mask, bg)
    findings += detect_m1(mask, scale)
    findings += detect_a1(mask, scale)
    findings += detect_a2(mask, scale)
    t1, skip, lines = detect_t1(mask, scale, components)
    findings += t1
    findings += detect_q1(mask, lines, scale, rgb, bg)
    findings += detect_i1(components, scale, name, lines)
    skips = [skip] if skip else []
    for f in findings:
        f["grade"] = grade_for(f, golden or name)
    return findings, skips


def run(goldens_dir: Path, out: Path | None) -> tuple[list[dict], list[dict]]:
    manifest = read_manifest(goldens_dir)
    all_findings: list[dict] = []
    skips: list[dict] = []
    for entry in manifest:
        name = entry["name"]
        scale = float(entry["axes"]["scale"])
        rgb = load_golden(goldens_dir, name)
        fs, sk = analyze_golden(rgb, scale, name)
        for f in fs:
            all_findings.append(
                {
                    "golden": name,
                    "surface": name.split("-")[0],
                    "invariant": f["invariant"],
                    "grade": f["grade"],
                    "region": {
                        "x": f["region"][0],
                        "y": f["region"][1],
                        "w": f["region"][2],
                        "h": f["region"][3],
                    },
                    "logical_region": logical(f["region"], scale),
                    "metric": f["metric"],
                    "threshold": f["threshold"],
                    **(
                        {
                            "classification": f["classification"],
                            "rationale": f["rationale"],
                            "extent": f["extent"],
                        }
                        if f["invariant"] == "C1"
                        else {}
                    ),
                }
            )
        for s in sk:
            skips.append({"golden": name, "note": s})
    if out:
        lines = []
        for f in all_findings:
            lines.append("[[finding]]")
            for k, v in f.items():
                if isinstance(v, dict):
                    inner = ", ".join(f"{ik} = {iv}" for ik, iv in v.items())
                    lines.append(f"{k} = {{ {inner} }}")
                else:
                    lines.append(f"{k} = {tomllib_escape(v)}")
            lines.append("")
        for s in skips:
            lines.append("[[skip]]")
            lines.append(f"golden = {tomllib_escape(s['golden'])}")
            lines.append(f"note = {tomllib_escape(s['note'])}")
            lines.append("")
        out.write_text("\n".join(lines), encoding="utf-8")
    return all_findings, skips


def tomllib_escape(v) -> str:
    if isinstance(v, (int, float)):
        return str(v)
    return '"' + str(v).replace("\\", "\\\\").replace('"', '\\"') + '"'


# ---------------------------------------------------------------------------
# Markdown emission (D8): renders the committed findings.toml (the source of
# truth, grades applied by grade_for per the D5 rubric at run time) into the
# report.


def emit_md(
    findings_path: Path,
    md_path: Path,
    goldens_dir: str,
    reconcile_path: Path | None = None,
) -> None:
    with open(findings_path, "rb") as f:
        data = tomllib.load(f)
    findings = data.get("finding", [])
    skips = data.get("skip", [])
    invariants = ["A1", "A2", "T1", "M1", "C1", "Q1", "I1"]
    grades = ["accept-as-legacy", "fix-now", "fix-later"]
    by: dict[tuple[str, str], int] = {}
    for fnd in findings:
        by[(fnd["invariant"], fnd["grade"])] = by.get((fnd["invariant"], fnd["grade"]), 0) + 1

    # Reconciliation (#1418 law [L](c)): when a previous (hand-graded)
    # findings.toml is supplied, every grade that moved is enumerated
    # here — disagreements are recorded, never silently flipped.
    flips: list[tuple[str, str, str, str, str]] = []  # golden, metric, old, new, cls
    if reconcile_path is not None:
        with open(reconcile_path, "rb") as f:
            old = tomllib.load(f)
        old_grades = {
            (o["golden"], o["invariant"], o["region"]["x"], o["region"]["y"], o["region"]["w"], o["region"]["h"]): o["grade"]
            for o in old.get("finding", [])
        }
        for fnd in findings:
            key = (
                fnd["golden"],
                fnd["invariant"],
                fnd["region"]["x"],
                fnd["region"]["y"],
                fnd["region"]["w"],
                fnd["region"]["h"],
            )
            prev = old_grades.get(key)
            if prev is not None and prev != fnd["grade"]:
                flips.append(
                    (
                        fnd["golden"],
                        fnd["metric"],
                        prev,
                        fnd["grade"],
                        fnd.get("classification", "-"),
                    )
                )

    out = []
    out.append("# Layout Review Analyzer — Findings (round 8, chunk L2, tier 2)")
    out.append("")
    out.append("- **Date**: 2026-11-25")
    out.append("- **Tier 2 additions** (analyzer-invariant-tier2 plan): Q1 WCAG"
    " contrast floor (#1410), I1 icon-size conformance (#1409), and the"
    " T1 connected-component glyph-line estimator upgrade (#1417 — the"
    " row-run band method and its MAX_LINE_EM_LOGICAL exclusion cap are"
    " replaced by component-level glyph classification).")
    out.append("- **Posture**: advisory-first (binding, plan D6). This report is data,"
    " not a gate — the analyzer reports; it never gates. No CI wiring, no"
    " nextest test, no pre-commit hook references this tool.")
    out.append("- **Run command**:")
    out.append("")
    out.append("  ```sh")
    out.append(f"  scripts/layout_review.py {goldens_dir} \\")
    out.append(f"      --out .artifacts/reports/{findings_path.name}")
    out.append("  ```")
    out.append("")
    out.append(
        "  `findings.toml` beside this file is the source of truth; this report is"
        " generated from it by `--emit-md`. Metric/region fields reproduce"
        " byte-identically on re-run (the corpus is byte-deterministic per G1);"
        " `grade` fields are applied deterministically per the recorded grading"
        " rules below (grade_for — the toml grades re-derive from the metric"
        " fields; for C1 the rubric consumes the #1418 typed classification"
        " (real-clip / thumb / chrome-speck) emitted by the detector, so the"
        " grade is reproducible without hand labour) and the"
        " accept-as-legacy grades name explained debt — the grade never"
        " silences the detector."
    )
    out.append("")
    out.append("- **Calibration note** (D7): thresholds calibrated once on this corpus"
    " before grading — corner-sampled background flat at RGB ≈ (29,32,40) with"
    " ±2/255 anti-aliasing noise; content floor 12/255 sits above the noise"
    " and below the dimmest ink; edge floor 8 logical px sits above hairlines"
    " and below every real block boundary; text tolerance ±15% absorbs ≤1 px"
    " line-height slop at every corpus scale without overlapping ladder rungs"
    " (closest separation 12%, 12.5→14). Thresholds were not widened after"
    " grading.")
    out.append("")
    n = len(findings)
    out.append(f"## Headline counts — {n} findings over the corpus")
    out.append("")
    out.append("| invariant | " + " | ".join(grades) + " | total |")
    out.append("|---|" + "---|" * (len(grades) + 1))
    for inv in invariants:
        cells = [by.get((inv, g), 0) for g in grades]
        out.append(f"| {inv} | " + " | ".join(str(c) for c in cells) + f" | {sum(cells)} |")
    totals = [sum(by.get((inv, g), 0) for inv in invariants) for g in grades]
    out.append("| **total** | " + " | ".join(f"**{t}**" for t in totals) + f" | **{sum(totals)}** |")
    out.append("")
    out.append("Grading rubric (D5): **fix-now** — a defect class the project already"
    " tracks as a bug (C1 clip signatures); **fix-later** — real drift from the"
    " invariant with no open issue (the finding is the intake record for a"
    " future backlog issue); **accept-as-legacy** — the deviation is load-bearing"
    " in the committed golden or the manifest's state note explains it.")
    out.append("")
    out.append("Grading rules applied to this run (deterministic, recorded per D5 —"
    " the rubric absorbs legacy noise, never the detector):")
    out.append("")
    out.append("- C1 (the #1418 clip-sentinel law, 2026-11-25): every C1 finding"
    " carries a typed classification — real-clip / thumb / chrome-speck —"
    " and the rubric reads the classification, not raw extent: thumb or"
    " chrome-speck → accept-as-legacy (rationale emitted from the"
    " classification + the calibration note); real-clip ≥ 12 px → fix-now"
    " (the severing class #1365/#1378); real-clip < 12 px → fix-later (a"
    " real drift smaller than the severing class — the class the old"
    " extent-only rubric had no home for). The thumb model is extent-band"
    " + track-column signature (a contiguous near-floor band ≥ 25% of the"
    " edge through the run), never a bare size threshold. The 12-px line"
    " stays in the metric strings as the calibration record.")
    out.append("- A1: delta to the nearest shared grid column ≤ 6 px → fix-later"
    " (near-miss drift); larger → accept-as-legacy (a distinct layout region,"
    " not a near-miss of the grid); no shared column at all (a singleton"
    " left edge) → accept-as-legacy (the grid judgment needs a neighbour to"
    " be a near-miss of — a lone edge is recorded, not graded fixable).")
    out.append("- A2: |gap − median| ≤ 8 px → fix-later (token-adjacent drift, the"
    " #1334 one-row-offset class); larger → accept-as-legacy (deliberate"
    " section breaks and spacers).")
    out.append("- T1: em ≥ 20 logical px → fix-later (genuine off-ladder text between"
    " the title and display rungs); em < 20 → accept-as-legacy (genuinely small"
    " off-ladder text — the tier-2 connected-component estimator classifies"
    " hairline rules and divider rows out at the component level, so the old"
    " hairline class no longer reaches the findings).")
    out.append("- Q1: a sub-floor contrast ratio is fix-now — the accessibility defect"
    " class (a floor is binary: 4.5 ≤ ratio is clean, below it is fix-now;"
    " there is no fix-later band inside a floor) — except the scrim-blend ink"
    " class → accept-as-legacy: overlay goldens render backdrop content under"
    " the modal scrim (measured ink modes sit on the 50% blend of the text"
    " tokens, max channel ≤ 140 vs ≥ 166 for unscrimmed text), which is"
    " deliberate de-emphasis, not a token defect.")
    out.append("- I1: fix-later (a drift from the Theme.icon-size token with no open"
    " issue; the finding is the intake record. Scope: icon-probe goldens only"
    " — the threshold string on every finding carries the guard).")
    out.append("- M1: icon-probe and board-drag-indicator margins → accept-as-legacy"
    " (the manifest's state note: deliberate kind-wall padding / mostly-empty"
    " drag canvas); the rest → fix-later.")
    out.append("")
    out.append("Residual C1 fix-now explanation (round 8 L1, 2026-11-26; amended"
    " round 8 B4, 2026-11-26): the ten remaining C1 fix-now findings are the"
    " recorded, explained residue of the clip-focus law chunk. The four"
    " board-columns signatures (`board-0.5-600`, `board-3.0-600`) are the"
    " #1365 overflow regime — their fix is the #1419 option-A Flickable wrap,"
    " withdrawn twice on the remount-refocus defect (#1490 carries the"
    " diagnosis and the fix prompt); the law records them as accepted legacy"
    " pending #1490 (docs/widget_interactions.org, the clip-focus law's"
    " status section) — the rubric keeps their fix-now grade so the debt"
    " stays visible. The four N-today signatures (`notification-3.0-1000`,"
    " `today-3.0-1000`, the timers-editor row overflow at kiosk scale) are a"
    " separate surface, filed as #1491. The two `task-view-1.5-900`"
    " bottom-edge signatures join with round 8 B4's #1435 vertical editor"
    " inset: the inset shrank the flexible description section's leftover"
    " height, so the section's own Flickable viewport (MultiLineEditor view"
    " branch, widgets/multi-line-editor.slint) now clips its below-the-fold"
    " content at the window's bottom edge — scroll viewport geometry, not a"
    " severed widget; the tight fit is filed as its own backlog issue.")
    out.append("")
    out.append("## Findings by golden")
    out.append("")
    if flips:
        out.append("### Grade reconciliation vs the previous hand-applied run")
        out.append("")
        out.append("| golden | metric | old grade | new grade | classification |")
        out.append("|---|---|---|---|---|")
        for g, metric, oldg, newg, cls in flips:
            out.append(f"| `{g}` | {metric} | {oldg} | {newg} | {cls} |")
        out.append("")
        out.append("Every flip is recorded with its classifier rationale in"
        " `findings.toml`; none is silent. The flips in this run are the"
        " designed gap-fill of the #1418 rubric: sub-severing real clips"
        " (the editor-family sever runs below 12 px) the old extent-only"
        " rule blurred into accept-as-legacy now classify `real-clip` and"
        " grade fix-later.")
        out.append("")
    goldens = sorted({f["golden"] for f in findings})
    if not goldens:
        out.append("(no findings)")
        out.append("")
    for g in goldens:
        rows = [f for f in findings if f["golden"] == g]
        out.append(f"### `{g}`")
        out.append("")
        out.append("| invariant | grade | classification | region (phys px) | logical | metric |")
        out.append("|---|---|---|---|---|---|")
        for r in rows:
            reg = r["region"]
            lg = r["logical_region"]
            cls = r.get("classification", "-")
            rat = r.get("rationale", "")
            metric = r["metric"] + (f" — {rat}" if rat and r["invariant"] == "C1" else "")
            out.append(
                f"| {r['invariant']} | {r['grade']} | {cls} "
                f"| x={reg['x']} y={reg['y']} w={reg['w']} h={reg['h']} "
                f"| x={lg['x']} y={lg['y']} w={lg['w']} h={lg['h']} "
                f"| {metric} |"
            )
        out.append("")
    if skips:
        out.append("## Recorded skips (honest-skip, never silent)")
        out.append("")
        for s in skips:
            out.append(f"- `{s['golden']}` — {s['note']}")
        out.append("")
    md_path.write_text("\n".join(out) + "\n", encoding="utf-8")


# ---------------------------------------------------------------------------
# Self-test (plan §4): constructed compliant/misaligned fixture pairs per
# invariant, drawn deterministically with rectangles (no fonts, no noise
# coupling). Each invariant gets: compliant → no finding; misaligned →
# exactly the expected finding; tolerance pin → within-tolerance deviation
# finds nothing.


def _canvas(w: int, h: int, bg=(29, 32, 40)) -> Image.Image:
    return Image.new("RGB", (w, h), bg)


def _rect(im: Image.Image, x0, y0, x1, y1, color=(200, 200, 200)) -> None:
    im.paste(color, (x0, y0, x1 + 1, y1 + 1))


def self_test() -> bool:
    failures: list[str] = []

    def check(label: str, cond: bool) -> None:
        if cond:
            print(f"self-test PASS: {label}")
        else:
            failures.append(label)
            print(f"self-test FAIL: {label}")

    def findings_of(im: Image.Image, scale: float = 1.0, name: str = "") -> list[dict]:
        rgb = np.asarray(im, dtype=np.int16)
        mask, bg = content_mask(rgb)
        components = connected_components(mask)
        out = []
        out += detect_c1(rgb, mask, bg)
        out += detect_m1(mask, scale)
        out += detect_a1(mask, scale)
        out += detect_a2(mask, scale)
        t1, _, lines = detect_t1(mask, scale, components)
        out += t1
        out += detect_q1(mask, lines, scale, rgb, bg)
        out += detect_i1(components, scale, name, lines)
        for f in out:
            f["grade"] = grade_for(f, name)
        return out

    # --- A1 alignment grid -------------------------------------------------
    def a1_fixture(shift: int) -> Image.Image:
        im = _canvas(200, 300)
        for i, y in enumerate((20, 120, 220)):  # three blocks, height 40
            d = shift if i == 2 else 0  # only the last block may be shifted
            _rect(im, 50 + d, y, 150 + d, y + 40)
        return im

    fs = [f for f in findings_of(a1_fixture(0)) if f["invariant"] == "A1"]
    check("A1 compliant (left-aligned trio) → no A1 finding", not fs)
    fs = [f for f in findings_of(a1_fixture(6)) if f["invariant"] == "A1"]  # 3 × t = 6 px
    check(
        "A1 misaligned (shift 6 px = 3t) → exactly one A1 finding",
        len(fs) == 1 and fs[0]["region"][0] == 56,
    )
    fs = [f for f in findings_of(a1_fixture(2)) if f["invariant"] == "A1"]  # within t
    check("A1 tolerance pin (shift 2 px = t) → no A1 finding", not fs)

    # A1 at scale 2: thresholds double; a 12-physical-px shift (= 6 logical
    # = 3t logical) must fire, and compliant stays clean.
    im2 = a1_fixture(0)
    fs = [f for f in findings_of(im2, 2.0) if f["invariant"] == "A1"]
    check("A1 compliant at scale 2 → no A1 finding", not fs)
    fs = [f for f in findings_of(a1_fixture(12), 2.0) if f["invariant"] == "A1"]
    check("A1 misaligned at scale 2 (12 phys = 6 logical = 3t) → one finding", len(fs) == 1)

    # --- A2 uniform gaps ----------------------------------------------------
    def a2_fixture(pitches: list[int]) -> Image.Image:
        im = _canvas(200, 400)
        y = 20
        for i, p in enumerate(pitches):
            _rect(im, 20, y, 179, y + 7)  # band of height 8
            y += 8 + (pitches[i] if i < len(pitches) else 0)
        return im

    fs = [f for f in findings_of(a2_fixture([16, 16, 16, 16])) if f["invariant"] == "A2"]
    check("A2 compliant (five bands, even 16-px gaps) → no A2 finding", not fs)
    fs = [f for f in findings_of(a2_fixture([16, 24, 16, 16])) if f["invariant"] == "A2"]
    check(
        "A2 outlier (one 24-px gap among 16s) → exactly one A2 finding",
        len(fs) == 1 and fs[0]["region"][1] == 20 + 8 + 16 + 8,  # gap after second band
    )
    # membership pin: two bands → one gap → no judgement
    fs = [f for f in findings_of(a2_fixture([24])) if f["invariant"] == "A2"]
    check("A2 membership pin (two bands) → no A2 finding", not fs)

    # --- T1 font-size ladder (tier-2 connected-component estimator) ---------
    def t1_fixture(em: float, n: int = 6, s: int = 1) -> Image.Image:
        # Each line is drawn as three glyph-cluster segments (width 40·s,
        # gaps 4·s ≤ LINE_X_GAP_LOGICAL·s) that merge into one line; the
        # cluster width stays within GLYPH_MAX_WIDTH_LOGICAL per segment.
        im = _canvas(300 * s, 200 * n)
        h_line = max(3, round(em * EM_FRACTION)) * s
        y = 20 * s
        for _ in range(n):
            for k in range(3):
                x0 = (20 + k * 44) * s
                _rect(im, x0, y, x0 + 40 * s - 1, y + h_line - 1)
            y += h_line + 6 * s
        return im

    fs = [f for f in findings_of(t1_fixture(14)) if f["invariant"] == "T1"]
    check("T1 compliant (six 14-px-em lines) → no T1 finding", not fs)
    fs = [f for f in findings_of(t1_fixture(11)) if f["invariant"] == "T1"]
    check("T1 compliant (six 11-px-em lines) → no T1 finding", not fs)
    fs = [f for f in findings_of(t1_fixture(24)) if f["invariant"] == "T1"]
    check("T1 off-ladder (24-px em; between rungs 20 and 30) → six findings", len(fs) == 6)

    # mixed band (the estimator's reason to exist): a 30-px-em line stacked
    # directly against an 11-px-em line — the row-run method measured one
    # ~41-px union; the component lines resolve separately, each on-rung.
    def t1_mixed_fixture(top_em: float) -> Image.Image:
        im = _canvas(300, 220)
        h_top = max(3, round(top_em * EM_FRACTION))
        h_bot = max(3, round(11 * EM_FRACTION))
        _rect(im, 20, 20, 59, 20 + h_top - 1)
        _rect(im, 20, 21 + h_top, 59, 21 + h_top + h_bot - 1)  # 1 empty row between
        # four on-rung filler lines at a disjoint x range (gap 41 px > the
        # 6-px line-group slop) so the ladder population judges the pair
        for k in range(4):
            y = 80 + k * 20
            _rect(im, 120, y, 159, y + 9)
        return im

    fs = [f for f in findings_of(t1_mixed_fixture(30)) if f["invariant"] == "T1"]
    check(
        "T1 mixed band (30-px em against 11-px em) → both on-rung, no finding",
        not fs,
    )
    fs = [f for f in findings_of(t1_mixed_fixture(24)) if f["invariant"] == "T1"]
    check(
        "T1 mixed band off-ladder (24-px em against 11-px em) → exactly one finding",
        len(fs) == 1 and fs[0]["region"][3] == round(24 * EM_FRACTION),
    )

    # non-text classified out at the component level (no reliance on the
    # deleted MAX_LINE_EM_LOGICAL cap)
    t1_nonext = _canvas(300, 120)
    _rect(t1_nonext, 20, 20, 79, 79)  # 60×60 solid block
    _rect(t1_nonext, 100, 30, 111, 41)  # a 12×12 icon beside it
    fs = [f for f in findings_of(t1_nonext) if f["invariant"] == "T1"]
    check(
        "T1 non-text classified out (60×60 block + 12-px icon) → no T1 finding",
        not fs and "MAX_LINE_EM_LOGICAL" not in globals(),
    )

    # suppression: two lines only → explicit skip note
    rgb = np.asarray(t1_fixture(14, n=2), dtype=np.int16)
    mask, _ = content_mask(rgb)
    _, skip, _lines = detect_t1(mask, 1.0)
    fs, _, _ = detect_t1(mask, 1.0)
    check(
        "T1 suppression pin (n=2) → no findings and a recorded skip note",
        not fs and skip is not None and "n=2" in skip,
    )

    # scale pin: the compliant fixture drawn at 2× physical geometry,
    # judged at scale 2.0 → same logical ems, no finding
    fs = [f for f in findings_of(t1_fixture(14, s=2), 2.0) if f["invariant"] == "T1"]
    check("T1 scale pin (compliant fixture at scale 2.0) → no T1 finding", not fs)

    # --- Q1 WCAG contrast floor ---------------------------------------------
    # known-answer pins (mirror token_tests.rs wcag_math_is_sanity_anchored)
    check(
        "Q1 known-answer (black/white → 21.0)",
        abs(contrast_ratio((0, 0, 0), (255, 255, 255)) - 21.0) < 1e-9,
    )
    check(
        "Q1 known-answer (equal colours → 1.0)",
        abs(contrast_ratio((123, 45, 67), (123, 45, 67)) - 1.0) < 1e-9,
    )

    def q1_fixture(ink: tuple, em: float, n: int = 2) -> Image.Image:
        im = _canvas(300, 140)
        h_line = max(3, round(em * EM_FRACTION))
        y = 20
        for _ in range(n):
            _rect(im, 20, y, 59, y + h_line - 1, color=ink)
            y += h_line + 6
        return im

    # body sub-floor: unscrimmed dim ink (max channel > SCRIM_INK_MAX_CHANNEL)
    # at ~3.4:1 on the (29,32,40) canvas, em 14 (< 20 → Body class, floor
    # 4.5) — the plan's (150,150,150) grey computes ≈ 5.5:1, compliant on
    # this dark canvas; a near-neutral grey below the floor would fall in
    # the scrim-blend band, so the constructed case is danger-toned like the
    # corpus's real fix-now finding (#1450's danger debt).
    r_sub = contrast_ratio((170, 90, 90), (29, 32, 40))
    fs = [f for f in findings_of(q1_fixture((170, 90, 90), 14)) if f["invariant"] == "Q1"]
    check(
        "Q1 sub-floor body text → one finding per line, grade fix-now",
        len(fs) == 2
        and all(f["grade"] == "fix-now" for f in fs)
        and r_sub < 4.5,
    )
    # scrim-blend ink class: a near-neutral sub-floor ink whose max channel
    # sits in the scrim band grades accept-as-legacy (the calibration class)
    fs = [f for f in findings_of(q1_fixture((120, 120, 120), 14)) if f["invariant"] == "Q1"]
    check(
        "Q1 scrim-blend ink (max channel ≤ 140) → findings graded accept-as-legacy",
        len(fs) == 2 and all(f["grade"] == "accept-as-legacy" for f in fs),
    )
    r = contrast_ratio((230, 230, 235), (29, 32, 40))
    fs = [f for f in findings_of(q1_fixture((230, 230, 235), 14)) if f["invariant"] == "Q1"]
    check(
        "Q1 compliant body text (ratio > 4.5 asserted) → no Q1 finding",
        not fs and r > 4.5,
    )
    r_large = contrast_ratio((110, 110, 110), (29, 32, 40))
    fs = [f for f in findings_of(q1_fixture((110, 110, 110), 30, n=1)) if f["invariant"] == "Q1"]
    check(
        "Q1 large-text floor pin (ratio ∈ [3.0, 4.5), em 30 → Large 3.0) → no finding",
        not fs and 3.0 <= r_large < 4.5,
    )

    # --- I1 icon-size conformance ---------------------------------------------
    def i1_fixture(w: int, h: int) -> Image.Image:
        im = _canvas(200, 120)
        _rect(im, 40, 40, 40 + w - 1, 40 + h - 1)
        return im

    fs = [f for f in findings_of(i1_fixture(12, 12), 1.0, "icon-probe-1.0-396") if f["invariant"] == "I1"]
    check("I1 compliant (12×12, icon-probe scope) → no I1 finding", not fs)
    fs = [f for f in findings_of(i1_fixture(14, 14), 1.0, "icon-probe-1.0-396") if f["invariant"] == "I1"]
    check("I1 tolerance pin (14×14, within ±2) → no I1 finding", not fs)
    fs = [f for f in findings_of(i1_fixture(17, 17), 1.0, "icon-probe-1.0-396") if f["invariant"] == "I1"]
    check(
        "I1 off-token (17×17, icon-probe scope) → exactly one I1 finding",
        len(fs) == 1 and fs[0]["region"] == (40, 40, 17, 17),
    )
    fs = [f for f in findings_of(i1_fixture(12, 40), 1.0, "icon-probe-1.0-396") if f["invariant"] == "I1"]
    check("I1 aspect guard (12×40 bar, not square-ish) → no I1 finding", not fs)
    fs = [f for f in findings_of(i1_fixture(17, 17), 1.0, "kinds-1.0-396") if f["invariant"] == "I1"]
    check("I1 scope guard (17×17 off the probe scope) → no I1 finding", not fs)
    fs = [f for f in findings_of(i1_fixture(24, 24), 2.0, "icon-probe-2.0-396") if f["invariant"] == "I1"]
    check("I1 scale pin (24×24 phys at scale 2.0 = 12 logical) → no I1 finding", not fs)

    # --- M1 margin conformance -------------------------------------------------
    m1_ok = _canvas(200, 300)
    _rect(m1_ok, 16, 16, 183, 283)  # 16-px margins all around (a rung)
    fs = [f for f in findings_of(m1_ok) if f["invariant"] == "M1"]
    check("M1 compliant (16-px margins, a rung) → no M1 finding", not fs)

    m1_bad = _canvas(200, 300)
    _rect(m1_bad, 18, 16, 183, 283)  # 18-px left margin; others on-rung
    fs = [f for f in findings_of(m1_bad) if f["invariant"] == "M1"]
    check(
        "M1 off-ladder (18-px left margin) → exactly one M1 finding on the left edge",
        len(fs) == 1 and fs[0]["region"][0] == 18,
    )

    m1_rung24 = _canvas(200, 300)
    _rect(m1_rung24, 24, 24, 175, 275)  # 24-px margins (a rung)
    fs = [f for f in findings_of(m1_rung24) if f["invariant"] == "M1"]
    check("M1 rung pin (24-px margins, a rung) → no M1 finding", not fs)

    # --- C1 clip sentinel --------------------------------------------------------
    c1_bad = _canvas(200, 100)
    _rect(c1_bad, 150, 40, 199, 60)  # flush against the right edge
    fs = [f for f in findings_of(c1_bad) if f["invariant"] == "C1"]
    check("C1 clip (block touching the right edge) → one C1 finding", len(fs) == 1)

    c1_ok = _canvas(200, 100)
    _rect(c1_ok, 150, 40, 180, 60)  # same block, fully inside
    fs = [f for f in findings_of(c1_ok) if f["invariant"] == "C1"]
    check("C1 clean (block fully inside) → no C1 finding", not fs)

    c1_backdrop = _canvas(200, 100, bg=(29, 32, 40))
    # uniform full-bleed non-modal field: corner sampling makes it the
    # background → no content at all (the backdrop pin, plan §3 C1)
    _rect(c1_backdrop, 0, 0, 199, 99, color=(40, 44, 54))
    fs = [f for f in findings_of(c1_backdrop) if f["invariant"] == "C1"]
    check("C1 backdrop pin (uniform full-bleed field) → no C1 finding", not fs)

    # flat backdrop along an edge with other content elsewhere: the touching
    # run must be exempt, real clips must not be
    c1_hdr = _canvas(200, 100)
    _rect(c1_hdr, 0, 0, 199, 9, color=(40, 44, 54))  # flat full-width header band
    fs = [f for f in findings_of(c1_hdr) if f["invariant"] == "C1"]
    check("C1 flat-backdrop exemption (flat edge band) → no C1 finding", not fs)

    # --- C1 classification (#1418): constructed cases both ways -----------
    # (a) thumb-in-track: a scrollbar thumb — a short run flush to the
    # right edge (barely above the content floor) inside a low-contrast
    # track column that runs most of the edge height.
    def thumb_fixture() -> Image.Image:
        im = _canvas(200, 300)
        _rect(im, 196, 10, 199, 289, color=(41, 44, 52))  # the track column (dev 12)
        _rect(im, 196, 130, 199, 137, color=(70, 74, 82))  # the thumb (dev > floor)
        return im

    fs = [f for f in findings_of(thumb_fixture()) if f["invariant"] == "C1"]
    check(
        "C1 thumb-in-track fixture → one finding classified thumb",
        len(fs) == 1 and fs[0]["classification"] == "thumb",
    )

    # (b) bare sliver: the same 8-px flush run WITHOUT the track —
    # must classify real-clip, never thumb (no bare size threshold).
    def sliver_fixture() -> Image.Image:
        im = _canvas(200, 300)
        _rect(im, 196, 130, 199, 137, color=(70, 74, 82))
        return im

    fs = [f for f in findings_of(sliver_fixture()) if f["invariant"] == "C1"]
    check(
        "C1 bare sliver fixture (no track) → classified real-clip",
        len(fs) == 1 and fs[0]["classification"] == "real-clip",
    )

    # (c) chrome speck: a 3-px run at the frame periphery, no track.
    speck = _canvas(200, 300)
    _rect(speck, 197, 40, 199, 42, color=(70, 74, 82))
    fs = [f for f in findings_of(speck) if f["invariant"] == "C1"]
    check(
        "C1 chrome-speck fixture (3-px run, no track) → classified chrome-speck",
        len(fs) == 1 and fs[0]["classification"] == "chrome-speck",
    )

    # (d) real flush panel: a wide, strongly-deviating panel flush at
    # the edge, extent past the severing floor → real-clip fix-now.
    panel = _canvas(200, 300)
    _rect(panel, 150, 100, 199, 160, color=(200, 200, 200))
    fs = [f for f in findings_of(panel) if f["invariant"] == "C1"]
    check(
        "C1 real flush panel fixture → real-clip graded fix-now",
        len(fs) == 1
        and fs[0]["classification"] == "real-clip"
        and fs[0]["grade"] == "fix-now",
    )

    # (e) sub-severing real clip → real-clip graded fix-later (the
    # rubric's new home for the small real drift, law [L](c)).
    small_panel = _canvas(200, 300)
    _rect(small_panel, 190, 100, 199, 107, color=(200, 200, 200))
    fs = [f for f in findings_of(small_panel) if f["invariant"] == "C1"]
    check(
        "C1 small real clip fixture (8 px, strong ink, no track) → real-clip fix-later",
        len(fs) == 1
        and fs[0]["classification"] == "real-clip"
        and fs[0]["grade"] == "fix-later",
    )

    print()
    if failures:
        print(f"self-test: {len(failures)} failure(s)")
        return False
    print("self-test: all invariants pinned (compliant → clean, misaligned → finding)")
    return True


# ---------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("goldens_dir", nargs="?", help="directory containing goldens + manifest.toml")
    ap.add_argument("--out", type=Path, default=None, help="write findings.toml here")
    ap.add_argument(
        "--emit-md",
        nargs=2,
        metavar=("FINDINGS_TOML", "REPORT_MD"),
        default=None,
        help="render a findings.toml into the markdown report",
    )
    ap.add_argument(
        "--reconcile",
        type=Path,
        default=None,
        help="with --emit-md: a previous findings.toml whose hand-applied"
        " grades are reconciled against the new run; every flip is"
        " enumerated in the report",
    )
    ap.add_argument("--self-test", action="store_true", help="run the constructed-case pins")
    args = ap.parse_args(argv)

    if args.self_test:
        return 0 if self_test() else 1
    if args.emit_md:
        emit_md(
            Path(args.emit_md[0]),
            Path(args.emit_md[1]),
            "crates/taskboard-ui-slint/tests/goldens",
            reconcile_path=args.reconcile,
        )
        print(f"report written: {args.emit_md[1]}")
        return 0
    if not args.goldens_dir:
        ap.error("goldens_dir is required (or use --self-test / --emit-md)")
    d = Path(args.goldens_dir)
    if not (d / "manifest.toml").exists():
        raise SystemExit(f"tool error: no manifest.toml under {d}")
    findings, skips = run(d, args.out)
    counts: dict[str, int] = {}
    for f in findings:
        counts[f["invariant"]] = counts.get(f["invariant"], 0) + 1
    print(f"analyzed {len(read_manifest(d))} goldens: {len(findings)} findings")
    for inv in ("A1", "A2", "T1", "M1", "C1", "Q1", "I1"):
        if counts.get(inv):
            print(f"  {inv}: {counts[inv]}")
    for s in skips:
        print(f"  skip {s['golden']}: {s['note']}")
    if args.out:
        print(f"findings written: {args.out}")
    return 0  # advisory-first: findings are data, never a failure (D6)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
