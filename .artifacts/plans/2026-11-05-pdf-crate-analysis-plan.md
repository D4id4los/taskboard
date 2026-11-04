# [Plan] PDF crate analysis (#624 stage A) — candidate survey vs the ruled layout set

- **Date**: 2026-11-05
- **Kind**: stage-1 research plan for chunk **2** of the v0.2 dispatch
  ("PDF crate analysis", branch `docs/pdf-crate-analysis`;
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`
  §3, chunk 2). Research only — no implementation, no scratch code
  lands, no push, no PR from this chunk.
- **Origin (normative)**: implementation plan
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  **§5.4** (PDF export & print, two stages — stage A is this chunk) and
  **R-4** (session rulings §2); issue **#624** + its 2026-11-04 ruling
  comment (the analysis mandate: crate choice via a dedicated analysis
  pass that *recommends*; the recommendation is input to the user's
  stack-approval ruling, not the ruling itself); the GUI requirements
  brief row (`2026-10-14-gui-design-requirements-plan.md` line ~278,
  PDF/print preview W2); the user's prior-art template
  `~/knowledge_base/org/daily_task_list_print_template.org` ("improve
  on it", R-4).
- **Branch / worktree**: `docs/pdf-crate-analysis` in
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-docs-pdf-crate-analysis`
  (canonical sibling path; `/home/viktoria/src/taskboard` is a symlink
  view of the main checkout), branched off `main` @ `1dc5bf44`;
  `scripts/worktree-doctor.sh` passed (219G free, floor 25G).
- **Status**: **READY FOR STAGE 2** — the research runs per §3–§6; the
  report skeleton and acceptance criteria are fixed in §6.

## 1. Verified anchors (the data the layouts render; checked 2026-11-05, `main` @ `1dc5bf44`)

| Anchor | Where | What the analysis reads from it |
|---|---|---|
| `day_window`, `window_start`, `DayCalendar` (`today`/`calendar_date`) | `crates/taskboard-domain/src/days.rs` | The day-window convention (`[preferences] day_start_time` + fixed offset); the window-date the header phrase uses |
| `TodaySections` (`overdue`, `scheduled`, `due_today`, `done_today`), `today_sections` | `days.rs` | The four-section today projection — ids only, canonical orderings `(order, id)` / `(scheduled, order, id)` / `(done, id)`; first-match disjointness |
| `Task` fields | `crates/taskboard-domain/src/entities.rs` | `title`, `description`, `scheduled`, `duedate`, `done`, `effort`, `importance`, `urgency`, `recur`, `kind` — the per-task row content (VF-9's scheduling fields) |
| `TaskKind::Standard`/`Tracker` | `entities.rs` | The kind marker the layout may badge |
| `export_on` → `ExportPayload`/`ExportRow`, `render_csv`/`render_json`, `EXPORT_CSV_HEADER` | `crates/taskboard-app/src/commands/track.rs` | The export lane the file-writing PDF leg extends (VF-3): raw payload, fixed header, RFC-3339 stamps, kind `text_form` |
| `duration_phrase` | `track.rs` | The human duration vocabulary `<h>h <m>m`, minute-granular |
| VF-9 / VF-3 definitions | impl-plan §4 (items 3, 9) | Scheduling fields; the export verb family the PDF leg joins |

## 2. Requirement profile — what the layouts must render (feeds the report's §2)

**Data (exact, from §1 anchors):** the PDF pass consumes the same
today projection the UI reads — `today_sections` over
`day_window(now, day_start_time, offset)` — the four sections with
their canonical orderings, titles resolved from the task map. Per
task: title (arbitrary unicode from Deck), local time-of-day phrase
for scheduled entries, due/done marks, `effort`/`importance`/`urgency`
markers, `kind` badge. Header/date phrase = the window-date via
`DayCalendar::today`, ISO + short weekday (the prior art's
`\DTMtoday~\DTMshortweekdayname` shape), mixed German/English content
(de_AT: umlauts, `„…“` quotes). **Optional overlay** (the "improve on
it" leg): the day's tracking entries (`ExportRow`: task title,
start/end stamps, duration) with `duration_phrase` totals per task —
stage B decides inclusion; the analysis only notes the phrase
vocabulary the layout rides.

**Delivery posture (ruled, R-4/#624):** generate PDF → open in the
system viewer/print dialog; destination = explicit path with a config
default; the file-writing `export`-family leg over the daemon
(VF-3's explicit extension; byte-verbatim relay shape). Kiosk builds
exclude the feature — whether they can exclude the *dependency* too is
an axis (A8), not a given.

**Prior-art layout intent (facts, from the template file):** A4
portrait, printed single-sided; near-zero margins (0.1 cm) and ~8 pt
body text; a 2×2 quadrant grid of A6 panels; **top half upright** (TL
= routine checklist with ☐ boxes and priority glyphs; TR = non-routine
tasks + coaching notes; each top panel carries a date box at its
bottom), **bottom half rotated 180°** (BL = system concepts /
feeling-bad protocol / reflection tasks; BR = long/mid/short-term
goals + active & planned projects + goals-for-today); checkbox lists
throughout; icon glyphs for priority/task-class/category/location.
**"Room for improvement" in practice** (why taskboard replaces it):
content was hand-duplicated into the LaTeX source (now: rendered from
the live today projection), brittle absolute `minipage` layout, 30+
LaTeX packages, machine-local font paths (breaks off the original
machine), no fold-correct page semantics beyond the raw
`\rotatebox{180}`. Improvement = fixed curated layouts (R-4), generated
data, no user-editable template files (fenced).

## 3. Candidate list

**Seed (mandated by #624/impl-plan §5.4):**

1. `genpdf` — high-level layout layer (document/element model)
2. `printpdf` — low-level PDF writer with text/primitive support
3. `krilla` — modern high-level, safety-focused PDF generation
4. typst-as-library — the `typst` compiler driven in-process (incl.
   the `typst-as-lib` wrapper crate; surveyed as one lane, wrapper noted)

**Survey extensions (verify existence + maintenance first; add/drop
freely as the survey dictates):**

5. `pdf-writer` — low-level writer (krilla's substrate); evaluated
   standalone as the maximal-control minimal-dep option
6. A maintained `genpdf` fork, if the survey surfaces one
7. The **imposition-companion lane**: if the leading generator lacks
   per-rectangle placement/rotation, a post-processing N-up pass over
   its single-page output (`lopdf`/`pdf-writer`-based) — evaluated as
   a sub-option of the failing candidate, never as a separate
   recommendation

**Excluded classes (one line each in the report, not sections):**
external engines (wkhtmltopdf, WeasyPrint, headless Chromium) —
external non-Rust binaries, packaging burden on the cross-platform
release matrix, process-spawn posture; GUI/scene-graph renderers
off-lane.

## 4. Evaluation axes × evidence sources

Every claim carries a verifiable source (crates.io / repo / changelog
/ docs.rs + *what it states*), checked and dated **2026-11**.

| # | Axis | What is evidenced | Evidence source |
|---|---|---|---|
| A1 | Page sizing & geometry | Exact mm page dims (A4/A5/A6 portrait), margin control | docs.rs API surface (page-construction signatures) |
| A2 | **Imposition capability** | Arbitrary-rectangle content placement at given coordinates on a custom page, **per-rectangle 180° rotation**; verdict "layout math vs built-in" | docs.rs API / cited example per candidate (§5's capability bar) |
| A3 | Text shaping & unicode | Font embedding (TTF/OTF), unicode beyond Latin-1 (umlauts, `„…“`, glyph fallback), wrapping/alignment, font feature flags | docs.rs, repo docs, cited API |
| A4 | Maintenance/support posture | Last release, commit activity, open-issue health, bus factor (contributor count) | crates.io API, GitHub repo, changelog |
| A5 | Update-cadence risk | Release frequency, breaking-change history, pinning burden | changelog/version history |
| A6 | MSRV & dependency weight | `rust-version`, direct+transitive dep counts, feature-flaggability (default-features off) | Cargo.toml/docs.rs, `cargo tree` counts in a scratch dir (nothing lands) |
| A7 | Licensing vs the dual-tier model | Crate license(s) vs the workspace tiers + `cargo deny` config; where the dep lands (likely `taskboard-app`, AGPL-3.0-only) | LICENSE files, root `deny.toml`, AGENTS §6 |
| A8 | Memory footprint & kiosk gate | Documented memory claims; whether a cargo feature can keep the dep **and its tree** out of kiosk builds (optional-dependency pattern) — the report *recommends* the gate shape | crate docs/features list; gap → §8 caveat |
| A9 | no-unsafe posture | The crate's own unsafe usage (workspace crates forbid unsafe; a dep's unsafe is recorded, not auto-disqualifying) | repo grep / docs.rs |

## 5. The imposition requirement, precisely

**Sheet geometry (fact, from the prior art):** one A4 portrait page
(210×297 mm), printed single-sided, carrying **four A6 panels
(105×148.5 mm) in a 2×2 grid**. Print orientations on the sheet: **TL
and TR upright; BL and BR rotated 180°** (the template's
`\rotatebox{180}` bottom minipage). **Fold:** twice — once along the
horizontal axis, once along the vertical axis (print-outward mountain
folds); result: a **4-page A6 booklet**; "panel order mirrors the
fold" (R-4's wording).

**Working panel↔page table** (stage 2 pins exactly one; see notes):

| Booklet page | Sheet panel | Position | Orientation on sheet |
|---|---|---|---|
| **p1** (front cover) | TL | top-left | upright |
| **p2** | TR | top-right | upright |
| **p3** | BL | bottom-left | 180° |
| **p4** (back cover) | BR | bottom-right | 180° |

**Mapping notes (the report must resolve them):** the orientation
pattern (top row upright, bottom row pre-rotated) fixes the invariant
*pages read before the flip = top row, pages read after the flip =
bottom row*; the two legal fold-direction conventions permute **p2↔p3**
(cover-first vs map-fold reading). Stage 2 (a) re-derives the table
from the fold transforms and pins one convention, (b) records the
alternative in one line, (c) flags physical fold verification as a
stage-B duty.

**Capability bar (identical under every permutation — this is what
candidates are scored on, per A2):** place content in an arbitrary
rectangle at given coordinates on a **custom-size page**, with
**per-rectangle 180° rotation**; plus plain multi-page output for the
single-sided A4/A5/A6 layouts. Imposition **by layout math**
(generating panels directly at quadrant coordinates) is the expected
mechanism; a **built-in/post-hoc imposition** (N-up over generated
pages) is evaluated only as the §3 companion lane.

## 6. Deliverable shape (decision-grade)

`.artifacts/reports/2026-11-05-pdf-crate-analysis-report.md` —
the PR diff is the report (+ this plan) and nothing else:

1. **Method** — reproducible: exact commands/queries/URLs, every claim
   dated 2026-11.
2. **Requirement profile** — §2 of this plan, restated with anchors.
3. **Per-candidate evidence sections** — one per candidate, sources
   per claim; imposition feasibility **demonstrated per candidate**
   (docs/API citation or a cited example), never assumed — a candidate
   dismissed for lacking placement/rotation needs the citation just as
   a recommended one does.
4. **Comparison matrix** — candidates × axes A1–A9, each cell cited.
5. **Exactly one recommendation** + its consequences: where the
   dependency lives, the kiosk-gate recommendation (feature shape,
   whether the tree stays out of kiosk builds), and the update-cadence
   burden. **The report feeds the user's stack-approval ruling — it
   must not read as having ruled**; the hand-off ("user rules on
   GitHub after the PR lands") is named explicitly.
6. **Fallback** — what happens if the recommendation is declined
   (next-best candidate, and what changes).
7. **Out-of-scope ideas** (print pipelines, template files — fenced by
   R-4) go to backlog issues per AGENTS §7, not the report body.

## 7. Non-goals (from the dispatch, binding)

No implementation; no print pipelines (fenced); no template-file
formats (fenced by R-4); no transport decision (ruled: PDF-to-viewer);
**no benchmarking** — stage A is evidence-based, not measurement-based:
no scratch code lands, so no measured memory numbers.

## 8. Evidence caveats, stated up front

- **A8 memory footprint**: per-crate measured numbers require scratch
  code (fenced). The report evidences *documented* claims plus
  dependency-tree weight as the proxy and labels the gap as a gap.
- **A4 bus factor**: contributor counts are approximate; stated as
  approximations, not facts.
- Anything else the survey finds unevidencable is named in the
  candidate's section, never papered over.

## 9. References

- Impl plan §5.4 + rulings table R-4:
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
- Dispatch plan §3 chunk 2 (stage prompts, evidence mandate):
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`
- Issue #624 + the 2026-11-04 ruling comment (GitHub)
- GUI requirements brief row:
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`
- Prior art: `/home/viktoria/knowledge_base/org/daily_task_list_print_template.org`
- Data seams: `crates/taskboard-domain/src/days.rs`,
  `crates/taskboard-app/src/commands/track.rs`,
  `crates/taskboard-domain/src/entities.rs`
- AGENTS §4 (stack approval), §6 (licensing headers), §7 (artefacts),
  §10 (gates — docs-only diff: fmt + hooks apply; no code gates)
