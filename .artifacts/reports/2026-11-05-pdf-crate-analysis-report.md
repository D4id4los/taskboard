# [Report] PDF crate analysis (#624 stage A) — candidate survey vs the ruled layout set

- **Date**: 2026-11-05
- **Chunk**: 2 of the v0.2 dispatch ("PDF crate analysis"), branch
  `docs/pdf-crate-analysis`. Research only; no scratch code landed
  (the §1 measurement scratch was deleted after measurement). The PR
  is opened by the stage-3 review pass, per the dispatch.
- **Input**: stage-1 plan
  `.artifacts/plans/2026-11-05-pdf-crate-analysis-plan.md` (§2
  requirement profile, §3 candidate list, §4 axes, §5 imposition bar
  are normative here and are not restated in full); impl plan
  `2026-11-04-v0.2-focus-reflection-implementation-plan.md` §5.4 +
  ruling R-4; issue #624.
- **Standing**: this report **recommends**; the crate choice is the
  user's stack-approval ruling (AGENTS §4), made on GitHub after this
  PR lands. Nothing here reads as having ruled.

## 1. Method (reproducible)

All queries executed **2026-11-05**. Every claim below carries its
source. Reproduce with:

- **crates.io metadata** (version, dates, MSRV, license, repo):
  `curl https://crates.io/api/v1/crates/<name>` for
  `genpdf`, `printpdf`, `krilla`, `pdf-writer`, `typst`,
  `typst-as-lib`, `genpdfi`; yanked flags via
  `https://crates.io/api/v1/crates/printpdf/versions`.
- **API surface**: docs.rs pages as cited per candidate (quoted
  signatures were read from the live rustdoc of the current release).
- **Dependency weight**: scratch Cargo projects (created under
  `/tmp/pdfsurvey/`, deleted after measurement — nothing landed) with
  one dependency each, measured via `cargo tree --quiet | wc -l`
  (total rows ≈ crate + its full transitive closure) on 2026-11-05,
  stable toolchain. Rows, not unique crates, are reported (rows
  slightly over-count duplicates; consistent across candidates so
  comparisons hold).
- **Licensing**: root `deny.toml` `[licenses]` allow-list (read
  2026-11-05) + each crate's SPDX from crates.io.
- **Workspace constraints**: `rust-version = "1.96"` (root
  `Cargo.toml`), toolchain `stable` (`rust-toolchain.toml`).

## 2. Requirement profile (summary; full detail in stage-1 plan §2)

Render the live today projection (`today_sections` over
`day_window`; `Task` title/scheduled/duedate/done/effort/importance/
urgency/kind from `crates/taskboard-domain/src/entities.rs`,
`days.rs`) into fixed curated PDF layouts (R-4): single-sided
A4/A5/A6 plus the foldable A4→4×A6 sheet. Content is arbitrary
unicode from Deck (mixed German/English: umlauts, `„…“`); header =
ISO date + short weekday; optional stage-B overlay of the tracking
export rows (`ExportRow`, `duration_phrase` from
`crates/taskboard-app/src/commands/track.rs`). Delivery: generate →
system viewer/print dialog; file-writing `export`-family verb over
the daemon; kiosk builds exclude the feature (dependency exclusion is
axis A8, decided below).

The **imposition capability bar** (stage-1 plan §5): place content in
an arbitrary rectangle at given coordinates on a **custom-size page**
with **per-rectangle 180° rotation**; plus plain multi-page
A4/A5/A6. Imposition by layout math is the expected mechanism.

## 3. The foldable sheet: pinning the panel↔page table

Stage-1 plan §5 asked stage 2 to pin one of two conventions. Re-
derivation from the fold transforms: the sheet is A4 portrait,
printed **single-sided**; the two folds are horizontal then vertical.
The binding invariant (stated by R-4's "panel order mirrors the
fold") is the stage-1 note: pages read *before* flipping the sheet
over must be the top row, pages read *after* flipping the bottom row
— which forces the bottom row to be pre-rotated 180° so it reads
upright after the flip. Under that invariant the two conventions
differ only in **which top quadrant is the front cover**:

- **Pinned here (cover-first)**: p1 = TL (upright), p2 = TR
  (upright), p3 = BL (180°), p4 = BR (180°). This matches the prior
  art's placement of the routine checklist (the reader's first
  surface) top-left, and is the table carried forward from the plan.
- Alternative (map-fold reading): p2 ↔ p3 swap; recorded, not used.

**Duty flagged for stage B**: the pin is derived from the transform
geometry, not physically verified — a printed fold test is a stage-B
acceptance item before the layout freezes.

## 4. Per-candidate evidence

### 4.1 `krilla` — recommended candidate

**Identity**: krilla 0.8.2 (crates.io, created 2026-06-04; 10
versions total), MIT OR Apache-2.0, MSRV **1.92** ≤ workspace 1.96
✓, repo `github.com/LaurenzV/krilla`. Author is a Typst contributor;
krilla is the generation layer above `pdf-writer` (typst's low-level
writer).

- **A1 page sizing**: `PageSettings::from_wh(w, h)` starts a page at
  arbitrary dimensions (crate-level example, docs.rs/krilla 0.8.2) —
  A4/A5/A6 in mm trivially, custom sheets included. ✓
- **A2 imposition**: `Surface::draw_text(start: Point, font, size,
  text, outlined, TextDirection)` places text at an arbitrary point;
  `Surface::push_transform(&Transform)` applies an arbitrary affine
  transform — a 180° rotation is `Transform` with matrix
  `[-1, 0, 0, -1, tx, ty]` (no dedicated rotate method; the docs
  name `push_transform`/`cur_transform`/`ctm`, docs.rs
  `krilla::surface::Surface`). Shapes via `draw_path`, boxes/checkbox
  glyphs included. So: quadrant placement at coordinates + per-panel
  rotation + custom page = **layout math, fully supported by the
  API**. ✓ (Multi-page plain layouts: `start_page` repeatedly.)
- **A3 text shaping**: `rustybuzz 0.20` (harfbuzz-ng port) for
  shaping and `skrifa` for font parsing are krilla's text stack
  (docs.rs dependency list); `Font::new(data, index)` embeds a TTF.
  Caveat to carry into stage B: `draw_text` "will not perform BIDI
  resolution and only supports a single script" (Surface docs) —
  sufficient for the German/English content; `draw_glyphs` is the
  escape hatch if mixed-script content ever appears. **No automatic
  line wrapping or pagination** — krilla has no layout engine; the
  fixed curated layouts supply the math (which the R-4 fixed-layout
  ruling makes deterministic work, not open-ended).
- **A4/A5 maintenance & cadence**: active — 10 releases between
  2024-09 and 2026-06, latest 2026-06-04 (crates.io). 0.x semver:
  breaking API churn between minors is real (A5 burden: expect to
  track minors deliberately, not blindly). **Bus factor
  (approximation, per stage-1 §8)**: one dominant author (LaurenzV);
  mitigations: krilla sits on `pdf-writer` (typst-org maintained),
  and the fallback ladder below runs through it.
- **A6 MSRV & weight**: MSRV 1.92 (crates.io). **Tree: 106 rows**
  (~105 deps incl. rustybuzz/skrifa/zlib-rs family). Not featureless:
  default features are `raster-images` + `simple-text`; the image
  support can be dropped (`default-features = false, features =
  ["simple-text"]`) since our layouts are vector/text only.
- **A7 licensing**: MIT OR Apache-2.0 — squarely inside the
  `deny.toml` allow-list (MIT + Apache-2.0 are global allows). Lands
  in `taskboard-app` (AGPL-3.0-only tier): no conflict. ✓
- **A8 memory & kiosk gate**: **no documented memory claims**
  (README covers safety and performance-by-rayon only) — gap named
  per stage-1 §8; tree weight is the proxy. Kiosk gate: krilla as an
  **optional dependency of `taskboard-app` behind an opt-in feature
  `pdf-export`** (`krilla = { version = "0.8", optional = true }` +
  `pdf-export = ["dep:krilla", ...]`) keeps crate *and tree* out of
  kiosk/default builds — recommended shape, decided in §6.
- **A9 unsafe**: README "Safety" section: "krilla has zero usages of
  unsafe and forbids unsafe code via a crate-level attribute" (it
  relies on deps like bytemuck/yoke that use unsafe internally —
  recorded, not disqualifying). Matches the workspace's
  `#![forbid(unsafe_code)]` spirit at the crate boundary. ✓

### 4.2 `printpdf`

**Identity**: printpdf 0.12.8 (crates.io, published 2026-09-05),
MIT, MSRV 1.88, repo fschutt/printpdf.

- **A1/A2**: the `units` module (Pt/Mm) and `matrix` module
  ("Transformation and text matrices") plus `ops` give raw control —
  imposition is *possible*, as with any low-level writer (docs.rs
  module index; note docs.rs reports only ~56% of the crate
  documented, so capability evidence here is module-level, not
  signature-level).
- **A3**: text shaping via azul's text3 (`text_shaping` module) —
  capable but drags the azul family in.
- **A4/A5**: active but **churning**: 0.12.0 (2026-07-18) and
  0.10.0 (2026-07-06) are **yanked** (crates.io versions API);
  several releases within days of each other in July 2026; the 0.12
  line pivoted the crate toward read/write/SVG/HTML, i.e. the write
  API surface is the least settled part. High pinning burden.
- **A6 weight**: **tree 199 rows** with default features (default
  feature set includes `html`-side deps: azul-core/css/layout,
  rust-fontconfig, web-sys/js-sys/wasm-bindgen — docs.rs dependency
  table).
- **A7**: MIT — allowed. **A8**: same optional-dep gate applies.
  **A9**: not verified (undocumented) — gap named.

Verdict: superseded by krilla for this use — krilla is the modern
write-only stack with an actual shaping story, and printpdf's churn +
default-feature weight buy nothing the layouts need.

### 4.3 `genpdf`

**Identity**: genpdf 0.2.0, **last release 2021-06-17** (crates.io
`updated_at`; 5 versions total), Apache-2.0 OR MIT, repo
`sr.ht/~ireas/genpdf-rs`. Effectively unmaintained (5 years).

- **A2 — the decisive axis**: the API is a flow/pagination document
  tree ("a Document consists of a LinearLayout that renders the
  added elements", docs.rs genpdf); `Rotation`, `Position`, `Size`
  exist only in the low-level `render` module which "does not
  provide support for measuring the size of rendered text or for
  laying out elements" (docs.rs `genpdf::render`). So the foldable
  sheet would have to be built on the layer the docs themselves
  defer on — no measured text, no layout support, and the flow layer
  cannot place panels at quadrant coordinates. **Capability bar not
  met at the level the crate is meant to be used at.** ✗
- **A3**: wrapping/alignment yes, hyphenation optional; built-in
  fonts limited to Windows-1252 (docs.rs) — the `„…“`/umlaut
  requirement forces font files anyway.
- **A4/A5**: dead upstream; also built on printpdf 0.2-era APIs.
- **A6**: tree unmeasured (moot); **A7** MIT/Apache fine.
Dismissed: unmaintained + imposition-unfit.

### 4.4 `genpdfi` (the maintained genpdf fork, §3 candidate 6)

**Identity**: genpdfi 0.2.7 (2026-01-27; releases Nov 2025 + Jan
2026), Apache-2.0 OR MIT, ~2,007 downloads/month, maintainer count ≈
2 (lib.rs/crates/genpdfi). It **inherits genpdf's flow-layout model**
and sits on **printpdf 0.7 + rusttype 0.8** (lib.rs) — i.e. an
unmaintained lineage two layers down, with the same A2 gap as genpdf
(flow layout, no measured absolute placement model), plus a much
smaller maintenance base than any seed candidate. **Tree: 125 rows**
(measured). Dismissed: does not clear A2 at its intended level and
its substrate is the crate everyone is leaving.

### 4.5 typst-as-library (`typst` in-process / `typst-as-lib` wrapper)

**Identity**: typst 0.15.1 (2026-07-17), Apache-2.0, MSRV 1.92,
repo `github.com/typst/typst`; wrapper `typst-as-lib` 0.17.0
(2026-10-07), MIT, repo Relacibo/typst-as-lib, no declared MSRV.

- **A2**: trivially met — Typst's `place(top, left)` and
  `rotate(180deg)` do arbitrary placement/rotation natively; best in
  class here.
- **A3**: best in class — full shaping, wrapping, hyphenation.
- **A6 weight**: **tree 719 rows** (measured with typst-as-lib) —
  the in-process compiler is the heaviest option by far, on kiosk-
  class hardware budgets the project explicitly cares about (AGENTS
  §1), even behind a feature gate that keeps it out of kiosk builds
  it bloats every desktop build.
- **Paradigm conflict**: typst's strength is its template/markup
  language; R-4 fences user-editable template files, and the
  fixed-curated-layouts ruling means the value of the template layer
  is unused — we would ship a compiler to render strings the layout
  math could place directly. The wrapper's templating framing is the
  same mismatch.
- **A4/A5**: very active (typst org); 0.x cadence of the compiler
  plus the wrapper's own chase-the-compiler releases (three typst
  version bumps visible in its version list, crates.io) double the
  update surface.
Dismissed: capability overkill at 7× krilla's dependency weight,
against a ruling that fences the paradigm it exists for.

### 4.6 `pdf-writer` (standalone; §3 candidate 5)

**Identity**: pdf-writer 0.15.0 (2026-05-27), MIT OR Apache-2.0, no
MSRV declared, repo `github.com/typst/pdf-writer` — typst-org
maintained, 24 versions since 2021. **Tree: 6 rows** — essentially
zero dependencies.

- **A2**: full and exact: `Content::transform([f32; 6])`,
  `Content::set_text_matrix([f32; 6])` (per-rectangle rotation =
  text-space matrix), `rect(x, y, w, h)`, and pages via
  `.media_box(Rect::new(...))` (docs.rs `pdf_writer::Content`,
  crate example). Imposition by layout math at maximal control. ✓
- **A3 — the decisive axis**: this is a raw PDF operation builder;
  **no shaping, no wrapping, no font subsetting ergonomics** —
  embedding a font, shaping German text with umlauts via a shaping
  engine, and tracking glyph→font mappings would all be ours to
  write (krilla exists precisely to do this over pdf-writer). ✗
  standalone.
Verdict: not the recommendation, but **the fallback** (§6) and the
substrate that de-risks krilla's bus factor.

### 4.7 Companion lane (§3 candidate 7) and excluded classes

- **Post-hoc N-up imposition** (`lopdf`/pdf-writer over a failing
  generator's single-page output): never triggered — the two leading
  candidates both support direct quadrant placement, so no
  post-processing pass is needed. Not carried forward.
- **Excluded classes** (one line, per stage-1 §3): external engines
  (wkhtmltopdf, WeasyPrint, headless Chromium) — external non-Rust
  binaries with process-spawn posture and cross-platform packaging
  burden on the release matrix; GUI/scene-graph renderers off-lane.

## 5. Comparison matrix

Dates: all crates.io/docs.rs reads 2026-11-05. Tree rows measured as
in §1. "—" = not verified/declared (gap named per stage-1 §8).

| Axis | krilla 0.8.2 | printpdf 0.12.8 | genpdf 0.2.0 | genpdfi 0.2.7 | typst(-as-lib) 0.15.1/0.17.0 | pdf-writer 0.15.0 |
|---|---|---|---|---|---|---|
| A1 page sizing | ✓ `PageSettings::from_wh` | ✓ Mm/Pt units | ✓ (paper sizes; mm model) | ✓ (inherited) | ✓ | ✓ `media_box(Rect)` |
| **A2 imposition** | ✓ `draw_text(Point)` + `push_transform` | ✓ raw ops (~56% documented) | ✗ flow layout; low-level `render` without measuring | ✗ inherited flow model | ✓✓ `place`+`rotate` | ✓✓ `transform`/`set_text_matrix` |
| A3 shaping/unicode | ✓ rustybuzz+skrifa; no BIDI/wrap (documented) | ✓ azul text3 (heavy) | ✓ wrap; fonts = Win-1252 builtin | ✓ inherited | ✓✓ full | ✗ none (raw ops) |
| A4 maintenance | ✓ active, 1 dominant author (approx.) | ✓ active but yanked releases | ✗ dead 2021 | ~ small fork, ~2 maintainers | ✓✓ typst org | ✓✓ typst org |
| A5 cadence risk | 0.x minors, deliberate tracking | high (0.10/0.12 yanked, pivoting) | n/a | follows dead lineage | 0.x + wrapper chase | slow, stable |
| A6 MSRV / tree rows | 1.92 / 106 | 1.88 / 199 | — / — | — / 125 | 1.92 / 719 | none / **6** |
| A7 license vs deny.toml | MIT OR Apache-2.0 ✓ | MIT ✓ | Apache OR MIT ✓ | Apache OR MIT ✓ | Apache-2.0 / MIT ✓ | MIT OR Apache-2.0 ✓ |
| A8 memory / kiosk gate | no documented claims (gap); optional-dep gate ✓ | no claims; gate ✓ | — | — | no claims; gate ✓ (but desktop weight stands) | trivial either way |
| A9 unsafe | **zero unsafe, `forbid(unsafe_code)`** (README Safety) | — not verified | — not verified | — not verified | — not verified | — not verified (typst org, likely safe) |

## 6. Recommendation (input to the user's stack-approval ruling)

**Recommend `krilla` (0.8.x) as the PDF generation crate, with imposition
done by layout math** — four panels placed at quadrant coordinates on
one A4 `PageSettings::from_wh(210.0, 297.0)` sheet, bottom row under a
180° `push_transform`, plain multi-page for the single-sided layouts.

**Consequences:**

1. **Where it lives**: `taskboard-app` only (the executable that owns
   the `export` verb family and figment config). No UI/state/domain
   crate touches it — the app renders the payload into a `Vec<u8>` PDF
   via the existing export seam.
2. **Kiosk gate (A8 answer)**: yes, the dependency *and its whole tree*
   can stay out of kiosk builds — optional dependency + opt-in feature:
   ```toml
   [dependencies]
   krilla = { version = "0.8", optional = true, default-features = false, features = ["simple-text"] }
   [features]
   pdf-export = ["dep:krilla"]
   ```
   Default/kiosk builds compile zero PDF code; the `export pdf` verb is
   feature-gated with the payload path. `raster-images` stays off (the
   layouts are vector/text only), trimming the tree further.
3. **Update-cadence burden**: 0.x crate — pin `0.8` and track minors
   deliberately at wave boundaries; expect a breaking API bump roughly
   at typst-adjacent release rhythm. MSRV 1.92 stays below the
   workspace's 1.96.
4. **Stage-B carries**: line-wrapping math is ours (no layout engine);
   the fold table pin of §3 needs one physical print-fold verification;
   German quotes/umlauts need an embedded font file in the repo (a
   Linux-liberation/DejaVu-class face), since nothing must depend on
   machine-local font paths (the prior art's failure).
5. **Hand-off**: this recommendation is input to the user's
   stack-approval ruling on GitHub after this PR lands; stage B
   (implementation, #624) starts only after that ruling. For the
   record: an in-session acceptance of krilla was noted on 2026-11-05
   ("until such a time as we find a functionality we absolutely need
   not present therein"), but it is **not the ruling** — the binding
   stack-approval decision is the GitHub one this report requests,
   and stage B stays gated on it. The fallback ladder (§7) stands for
   exactly the declined contingency.

**The two strongest counter-arguments recorded against it** (kept
honest for the ruling):

- **Bus factor ≈ 1**: one dominant author; if the crate stalls, the
  workspace inherits a 0.x dep. Mitigation on record: krilla is a
  layer over typst-org's `pdf-writer`, so the decline path is a
  port down one level, not a rewrite (see fallback).
- **No layout engine**: no automatic wrapping/pagination and no BIDI
  in `draw_text` — every wrap, measure, and alignment is our code,
  where regressions live. Mitigation on record: the R-4 fixed-layout
  ruling makes the layout set small, curated, and snapshot-testable
  (the export payload in → fixed geometry out).

## 7. Fallback if declined

**`pdf-writer` (typst org) used directly.** Same dependency
direction and licensing; the tree collapses to ~6 rows; imposition
control is *greater* (`Content::transform`, `set_text_matrix`,
`media_box`). What changes: font embedding, subsetting, shaping
(rustybuzz), and wrapping move into taskboard-app-owned code — a
step-function increase in stage-B effort and the reason it is second,
not first. Second-order alternative if krilla is declined *because*
of bus factor specifically: wait-and-pin on `genpdfi` is **not**
recommended (inherited flow model + unmaintained printpdf 0.7
substrate).

## 8. Out-of-scope ideas → backlog

None opened. The two candidate ideas the survey surfaced — print
pipelines beyond the viewer hand-off and user-editable template
files — are both **already fenced by explicit rulings** (R-4 /
impl-plan §5.4 stage B), so per AGENTS §7 they are recorded here as
fenced, not as new issues. A print-pipeline issue is only worth
opening if the user reopens that ruling.
