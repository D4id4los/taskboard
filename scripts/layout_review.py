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

Invariant catalogue (plan §3):
    A1  alignment grid       — singleton left edges off the shared grid
    A2  uniform gaps         — per-section inter-band gap outliers
    T1  font-size ladder     — text-line sizes off the Theme TextRole ladder
    M1  margin conformance   — outer margins off the space-* ladder
    C1  clip sentinel        — content runs touching a canvas edge
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

# T1: a text-line row-run must be at least this many physical px tall to
# be a line candidate (drops 1-px hairline noise).
MIN_LINE_RUN_PX = 3

# T1: minimum text lines in a golden for the height estimator to be
# honest (plan §3 T1: population suppression, recorded as a skip).
T1_MIN_LINES = 5

# T1: fraction of a row-run's height attributed to the em box
# (x-height-to-cap span ≈ 0.70 em; plan §3 T1).
EM_FRACTION = 0.70

# T1: candidate lines with a larger estimated em are blocks/images, not
# text (calibration: non-text bands — chips, icon walls, captures —
# produced em estimates up to 1806 logical px and drowned the signal).
# Above the display rung's tolerance ceiling the run is excluded from the
# line population entirely.
MAX_LINE_EM_LOGICAL = 30.0 * 1.15

# T1: tolerance when mapping a measured em to the TextRole ladder.
# Calibration: rung separation is ≥ 12% (12.5→14); ±15% absorbs the
# antialiasing slop measured in the calibration run (≤ 1 physical px on
# line heights at every corpus scale) without overlapping rungs.
TEXT_TOL_FRACTION = 0.15

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


# ---------------------------------------------------------------------------
# Detectors. Each returns a list of raw finding dicts (invariant, region in
# physical px, metric, threshold strings).


def detect_c1(mask: np.ndarray) -> list[dict]:
    """INV-C1 clip sentinel: content runs touching a canvas edge that are
    neither structural (spanning a large fraction of the edge — side
    rails, window-chrome bands) nor anti-aliasing noise."""
    h, w = mask.shape
    findings = []

    edge_specs = [
        ("right", mask[:, w - 1]),
        ("left", mask[:, 0]),
        ("bottom", mask[h - 1, :]),
        ("top", mask[0, :]),
    ]

    for name, line in edge_specs:
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
                    "region": region,
                    "metric": f"content run touches the {name} edge "
                    f"(extent {run_len} px along the edge)",
                    "threshold": f"structural exemption at ≥ {EDGE_RUN_EXEMPT_FRACTION:.0%} "
                    f"of the edge; min run {C1_MIN_RUN_PX} px; otherwise touching runs "
                    "are clips",
                }
            )
    return findings


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


def detect_t1(mask: np.ndarray, scale: float) -> tuple[list[dict], str | None]:
    """INV-T1 font-size ladder: text-line em estimates vs TextRole sizes.

    Returns (findings, skip_note). The estimator needs population; below
    T1_MIN_LINES lines it suppresses with an explicit, recorded note (an
    honest skip — never silent).
    """
    h, w = mask.shape
    row = mask.sum(axis=1).astype(np.int64)
    floor = max(1, int(BAND_FLOOR_FRACTION * w))
    lines: list[tuple[int, int]] = []  # (y0, y1) per text line
    for b0, b1 in bands(row, floor):
        sub = row[b0 : b1 + 1]
        lfloor = max(1, int(BAND_FLOOR_FRACTION * w * 0.5))
        for y0, y1 in bands(sub, lfloor):
            if y1 - y0 + 1 >= MIN_LINE_RUN_PX:
                lines.append((b0 + y0, b0 + y1))
    if len(lines) < T1_MIN_LINES:
        return [], f"T1: skipped (n={len(lines)} lines; needs ≥ {T1_MIN_LINES})"

    findings = []
    for y0, y1 in lines:
        height = y1 - y0 + 1
        em = height / EM_FRACTION / scale  # logical em estimate
        if em > MAX_LINE_EM_LOGICAL:
            continue  # a block/image band, not a text line (excluded)
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
                    "region": (0, y0, w, height),
                    "metric": f"text line height {height} px → em ≈ {em:.1f} logical px; "
                    f"nearest ladder rung {nearest} px",
                    "threshold": f"em within {TEXT_TOL_FRACTION:.0%} of a TextRole rung "
                    f"{TEXT_LADDER}; em = run height / {EM_FRACTION} ÷ scale {scale}",
                }
            )
    return findings, None


# ---------------------------------------------------------------------------


def logical(region: tuple[int, int, int, int], scale: float) -> dict:
    x, y, w, h = region
    return {
        "x": round(x / scale),
        "y": round(y / scale),
        "w": round(w / scale),
        "h": round(h / scale),
    }


def grade_for(finding: dict) -> str:
    """Deterministic rubric defaults (D5): C1 is the probable-bug class →
    fix-now; style-drift classes → fix-later. accept-as-legacy grades are
    hand-applied to findings.toml after the run (recorded in the report)."""
    return "fix-now" if finding["invariant"] == "C1" else "fix-later"


def analyze_golden(rgb: np.ndarray, scale: float) -> tuple[list[dict], list[str]]:
    mask, _ = content_mask(rgb)
    findings: list[dict] = []
    findings += detect_c1(mask)
    findings += detect_m1(mask, scale)
    findings += detect_a1(mask, scale)
    findings += detect_a2(mask, scale)
    t1, skip = detect_t1(mask, scale)
    findings += t1
    skips = [skip] if skip else []
    for f in findings:
        f["grade"] = grade_for(f)
    return findings, skips


def run(goldens_dir: Path, out: Path | None) -> tuple[list[dict], list[dict]]:
    manifest = read_manifest(goldens_dir)
    all_findings: list[dict] = []
    skips: list[dict] = []
    for entry in manifest:
        name = entry["name"]
        scale = float(entry["axes"]["scale"])
        rgb = load_golden(goldens_dir, name)
        fs, sk = analyze_golden(rgb, scale)
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
# truth, grades hand-applied per the D5 rubric after the run) into the report.


def emit_md(findings_path: Path, md_path: Path, goldens_dir: str) -> None:
    with open(findings_path, "rb") as f:
        data = tomllib.load(f)
    findings = data.get("finding", [])
    skips = data.get("skip", [])
    invariants = ["A1", "A2", "T1", "M1", "C1"]
    grades = ["accept-as-legacy", "fix-now", "fix-later"]
    by: dict[tuple[str, str], int] = {}
    for fnd in findings:
        by[(fnd["invariant"], fnd["grade"])] = by.get((fnd["invariant"], fnd["grade"]), 0) + 1

    out = []
    out.append("# Layout Review Analyzer — Findings (round 7, chunk G3, #1313)")
    out.append("")
    out.append("- **Date**: 2026-11-25")
    out.append("- **Posture**: advisory-first (binding, plan D6). This report is data,"
    " not a gate — the analyzer reports; it never gates. No CI wiring, no"
    " nextest test, no pre-commit hook references this tool.")
    out.append("- **Run command**:")
    out.append("")
    out.append("  ```sh")
    out.append(f"  scripts/layout_review.py {goldens_dir} \\")
    out.append(
        "      --out .artifacts/reports/2026-11-25-layout-review-analyzer-findings.toml"
    )
    out.append("  ```")
    out.append("")
    out.append(
        "  `findings.toml` beside this file is the source of truth; this report is"
        " generated from it by `--emit-md`. Metric/region fields reproduce"
        " byte-identically on re-run (the corpus is byte-deterministic per G1);"
        " `grade` fields are hand-applied per the D5 rubric after the run"
        " (rubric defaults: C1 → fix-now, others → fix-later) and the"
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
    out.append("- C1: extent ≥ 12 px → fix-now (flush panels, the open severing class"
    " #1365/#1378); extent < 12 px → accept-as-legacy (scrollbar thumbs at 8 px"
    " and window-chrome specks at 3 px — deliberate small flush elements).")
    out.append("- A1: delta to the nearest shared grid column ≤ 6 px → fix-later"
    " (near-miss drift); larger → accept-as-legacy (a distinct layout region,"
    " not a near-miss of the grid).")
    out.append("- A2: |gap − median| ≤ 8 px → fix-later (token-adjacent drift, the"
    " #1334 one-row-offset class); larger → accept-as-legacy (deliberate"
    " section breaks and spacers).")
    out.append("- T1: em ≥ 20 logical px → fix-later (genuine off-ladder text between"
    " the title and display rungs); em < 20 → accept-as-legacy (hairline rules"
    " and divider rows, not text ink).")
    out.append("- M1: icon-probe and board-drag-indicator margins → accept-as-legacy"
    " (the manifest's state note: deliberate kind-wall padding / mostly-empty"
    " drag canvas); the rest → fix-later.")
    out.append("")
    out.append("## Findings by golden")
    out.append("")
    goldens = sorted({f["golden"] for f in findings})
    if not goldens:
        out.append("(no findings)")
        out.append("")
    for g in goldens:
        rows = [f for f in findings if f["golden"] == g]
        out.append(f"### `{g}`")
        out.append("")
        out.append("| invariant | grade | region (phys px) | logical | metric |")
        out.append("|---|---|---|---|---|")
        for r in rows:
            reg = r["region"]
            lg = r["logical_region"]
            out.append(
                f"| {r['invariant']} | {r['grade']} "
                f"| x={reg['x']} y={reg['y']} w={reg['w']} h={reg['h']} "
                f"| x={lg['x']} y={lg['y']} w={lg['w']} h={lg['h']} "
                f"| {r['metric']} |"
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

    def findings_of(im: Image.Image, scale: float = 1.0) -> list[dict]:
        rgb = np.asarray(im, dtype=np.int16)
        mask, _ = content_mask(rgb)
        out = []
        out += detect_c1(mask)
        out += detect_m1(mask, scale)
        out += detect_a1(mask, scale)
        out += detect_a2(mask, scale)
        t1, _ = detect_t1(mask, scale)
        out += t1
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

    # --- T1 font-size ladder -------------------------------------------------
    def t1_fixture(em: float, n: int = 6) -> Image.Image:
        im = _canvas(300, 200)
        h_line = max(3, round(em * EM_FRACTION))
        y = 20
        for _ in range(n):
            _rect(im, 20, y, 279, y + h_line - 1)
            y += h_line + 6
        return im

    fs = [f for f in findings_of(t1_fixture(14)) if f["invariant"] == "T1"]
    check("T1 compliant (six 14-px-em lines) → no T1 finding", not fs)
    fs = [f for f in findings_of(t1_fixture(11)) if f["invariant"] == "T1"]
    check("T1 compliant (six 11-px-em lines) → no T1 finding", not fs)
    fs = [f for f in findings_of(t1_fixture(24)) if f["invariant"] == "T1"]
    check("T1 off-ladder (24-px em; between rungs 20 and 30) → six findings", len(fs) == 6)

    # suppression: two lines only → explicit skip note
    rgb = np.asarray(t1_fixture(14, n=2), dtype=np.int16)
    mask, _ = content_mask(rgb)
    _, skip = detect_t1(mask, 1.0)
    fs, _ = detect_t1(mask, 1.0)
    check(
        "T1 suppression pin (n=2) → no findings and a recorded skip note",
        not fs and skip is not None and "n=2" in skip,
    )

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
    ap.add_argument("--self-test", action="store_true", help="run the constructed-case pins")
    args = ap.parse_args(argv)

    if args.self_test:
        return 0 if self_test() else 1
    if args.emit_md:
        emit_md(Path(args.emit_md[0]), Path(args.emit_md[1]), "crates/taskboard-ui-slint/tests/goldens")
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
    for inv in ("A1", "A2", "T1", "M1", "C1"):
        if counts.get(inv):
            print(f"  {inv}: {counts[inv]}")
    for s in skips:
        print(f"  skip {s['golden']}: {s['note']}")
    if args.out:
        print(f"findings written: {args.out}")
    return 0  # advisory-first: findings are data, never a failure (D6)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
