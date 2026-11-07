# [Plan] PDF export & print (#624 stage B) — chunk 5 of the v0.2 completion dispatch

- **Date**: 2026-11-07
- **Branch / worktree**: `feat/pdf-export` / `../taskboard-WT-feat-pdf-export` (off `main` @ `01d4704c`)
- **Stage**: 1 (planning). Stage 2 implements this plan TDD-first; stage 3 reviews, runs the physical fold check, and opens the PR.
- **Normative origins**:
  - `.artifacts/reports/2026-11-05-pdf-crate-analysis-report.md` §2 (requirement profile), §3 (fold panel table, pinned cover-first), §6 (recommended shape), §7 (fallback)
  - Issue #624, ruling comments of 2026-11-04 (R-4 layouts, PDF-to-viewer, user-editable templates fenced, silent printing fenced) and **2026-11-05 (the stack approval: krilla 0.8.x, `pdf-export` feature in `taskboard-app` only, default off, embedded font, pdf-writer as fallback)**
  - `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md` R-4 and §5.4 stage B
  - `.artifacts/plans/2026-11-05-v0.2-completion-dispatch-plan.md` §6 (chunk 5 scope and acceptance)
  - Prior art (layout intent only): `~/knowledge_base/org/daily_task_list_print_template.org`. It uses an A4 sheet with 0.1 cm margins, 8 pt type, a routine checklist first, then space for non-routine entries, German typographic quotes, and machine-local `fontspec` fonts. That last point is the failure this chunk must not repeat.
- **Fallback**: if the krilla ruling is withdrawn, re-plan against report §7 (`pdf-writer` used directly). The layout model below (§3–§5) does not change. Only the backend in §6 is swapped.

## 1. Scope

In scope:

1. Add `krilla` as an optional dependency of `taskboard-app`, behind the `pdf-export` feature (default off).
2. Four fixed, curated layouts: A4, A5 and A6 single-sided, plus the foldable sheet (one A4 sheet → a 4-page A6 booklet).
3. A pure layout engine that takes the today projection and produces a display list. Wrapping and measuring are our own code.
4. An embedded font, committed to the repo.
5. A new `export --what pdf` leg that writes a file. Its destination comes from an argument or a config default. Path errors are typed.
6. Opening the written PDF in the system viewer. A failure to open is typed and non-fatal.
7. Golden tests on the display list, plus a byte-level smoke test of the PDF.
8. Kiosk exclusion, a CHANGELOG entry, and closing #624.

Fenced (out of scope by ruling): user-editable template files, silent or direct printing, and a tracking-rows overlay. The overlay is optional in report §2; it is deferred and recorded in §12.

## 2. Feature gate wiring (app crate only)

Root `Cargo.toml` `[workspace.dependencies]` pins the crate once:

```toml
krilla = { version = "0.8", default-features = false, features = ["simple-text"] }
skrifa = "=<the version krilla 0.8.2 resolves>"   # measurement only; already in krilla's tree
```

`crates/taskboard-app/Cargo.toml`:

```toml
[features]
pdf-export = ["dep:krilla", "dep:skrifa"]
[dependencies]
krilla = { workspace = true, optional = true }
skrifa = { workspace = true, optional = true }
```

- **Why skrifa is a direct dependency**: krilla does not expose glyph advance widths publicly, so the measure helper (§5) reads advances through `skrifa`'s `charmap` and `glyph_metrics`. Pinning it to the exact version krilla already resolves adds zero new crates. Stage 2 verifies this with `cargo tree -i skrifa --features pdf-export` showing one version. If two versions appear, the fallback is to measure through krilla's own API if one exists, otherwise to accept the duplicate and record it.
- **Code gate**: the module is `src/pdf/` (`mod.rs`, `layout.rs`, `measure.rs`, `render.rs`, `deliver.rs`). Only `render.rs` and `measure.rs` import krilla or skrifa. Everything is declared under `#[cfg(feature = "pdf-export")] mod pdf;`. The `layout.rs` display-list types stay pure Rust.
- **CLI gate**: `ExportWhat::Pdf` exists only with the feature on (`#[cfg(feature = "pdf-export")]` on the variant and on its match arms). A default build therefore rejects `--what pdf` as a clap usage error (exit 2). There is no runtime "feature disabled" error path to test, because the variant does not exist.
- **Proof of the gate** (acceptance):
  - `cargo deny check` passes both ways: as-is, and with `--features taskboard-app/pdf-export` (cargo-deny `--features` flag, or `[graph] features` in a CI step).
  - A new CI step mirrors the existing MCP hygiene step at `ci.yml:66`: `! cargo tree -p taskboard-app -e normal | grep -q krilla`. This proves the default tree is unchanged.
  - `cargo check --workspace --all-features` already exists at `ci.yml:120`, so the feature-on build is compiled in CI. Coverage also runs `--all-features`, so the pdf tests are measured.
- **Release artefacts** (open question for the user, see §11): `release.yml` builds the default feature set, so the desktop binaries would ship without PDF. Stage 2 adds `--features pdf-export` to the desktop release build only if the user rules that way. Otherwise the feature stays a build-time opt-in for v0.2.0.

## 3. Layout data model (pure, in `pdf/layout.rs`)

### 3.1 Input: the today projection, no parallel projection

`PrintDay` is built by one function, `print_day(state: &AppState, now, calendar: DayCalendar) -> PrintDay`. It calls `day_window(now, …)` and then `taskboard_domain::today_sections(&state.tasks, now, window)`. It reads the entities by id and does no classification of its own.

```rust
pub struct PrintDay {
    pub date: NaiveDate,              // calendar.today(now)
    pub sections: Vec<PrintSection>,  // display order: Overdue, Scheduled, DueToday, Done; empty ones omitted
}
pub struct PrintSection { pub kind: SectionKind, pub items: Vec<PrintItem> }
pub enum SectionKind { Overdue, Scheduled, DueToday, Done }
pub struct PrintItem {
    pub title: String,
    pub done: bool,            // drawn checkbox: empty or ticked
    pub time: Option<String>,  // "HH:MM"
    pub effort: Option<String>,
}
```

- **Phrases**: reuse `taskboard_ui_slint::presentation`, which the app crate already depends on. `plan_box_time_phrase(instant, calendar)` gives the absolute `HH:MM`: scheduled time for Scheduled, due time for Overdue and DueToday, and done time for Done. `effort_phrase` gives effort.
  - **Deliberately not used**: `due_phrase` and `scheduled_phrase`. They are relative to `now` ("in 2h", "started 3h ago") and go stale once printed. A sheet needs absolute times. Overdue items additionally carry the due date (`%d.%m.`) when it is not today. That small formatter lives in `layout.rs` and is unit-tested.
- **Header phrase**: ISO date plus short weekday (`2026-11-07 Sat`). It reuses `review_day_phrase` if that matches the format; otherwise a local formatter. Stage 2 checks which.
- **Clock discipline (AGENTS §5, clock rule)**: `now` is passed in. The verb reads it once through the app's injected `Arc<dyn Clock>`. Tests pass fixed instants, which anchors the fixture for this relative oracle.

### 3.2 Output: the display list

```rust
pub struct Sheet { pub size: PageSize, pub ops: Vec<DrawOp> }   // one per physical page
pub struct PageSize { pub w_pt: f32, pub h_pt: f32 }            // A1 mint: positive and finite
pub enum DrawOp {
    Text { at: Pt, size_pt: f32, weight: Weight, text: String, panel: PanelXf },
    Rule { from: Pt, to: Pt, width_pt: f32, panel: PanelXf },
    Checkbox { at: Pt, side_pt: f32, ticked: bool, panel: PanelXf },
}
pub enum PanelXf { Identity, Rot180 { tx: f32, ty: f32 } }
```

All coordinates are in points, with the origin at the page's top-left and y pointing down. This matches krilla's convention. The first TDD step (§9, step 1) pins the convention with a one-glyph render whose position is checked from the PDF content stream.

`Layout` is a closed enum, `A4 | A5 | A6 | FoldA6x4`. Each variant has a `LayoutSpec` constant: page size, margins, body size and heading size. The values below are starting points; the goldens freeze them.

| Layout | Page (mm) | Margin (mm) | Body / heading (pt) | Columns |
|---|---|---|---|---|
| A4 | 210 × 297 | 10 | 10 / 13 | 2 |
| A5 | 148 × 210 | 8 | 9 / 11 | 1 |
| A6 | 105 × 148 | 6 | 8 / 10 | 1 |
| FoldA6x4 | 210 × 297 sheet, four 105 × 148.5 panels | 5 per panel | 8 / 10 | 1 per panel |

Section order and vocabulary follow the today projection. Each section has a heading (Overdue, Scheduled, Due today, Done). Each item is a line: checkbox, time column, wrapped title, and effort right-aligned. After the projected sections comes a blank "Notes" area of ruled lines, as in the prior art's non-routine space. It fills the rest of the last page or panel.

**Overflow**: single-sided layouts paginate (new `Sheet`, with the header repeated). The fold sheet has exactly four panels. If content passes the end of panel 4, the last line becomes `+N more`. There is no second sheet, because the booklet is one sheet by construction. This is a property test (§8).

## 4. Fold sheet coordinate math (report §3, pinned cover-first)

The sheet is A4 portrait: W = 595.276 pt, H = 841.890 pt. Each panel is w = W/2 = 297.638 pt by h = H/2 = 420.945 pt. Panel content is always laid out in panel-local coordinates on [0,w] × [0,h], upright. Placement is one affine map per panel, with page coordinates top-left and y down:

| Booklet page | Quadrant | Rotation | Panel-local (x, y) → sheet | krilla `Transform` [a b c d e f] |
|---|---|---|---|---|
| p1 (cover) | TL | 0° | (x, y) | [1 0 0 1 0 0] |
| p2 | TR | 0° | (x + w, y) | [1 0 0 1 w 0] |
| p3 | BR | 180° | (W − x, H − y) | [−1 0 0 −1 W H] |
| p4 | BL | 180° | (w − x, H − y) | [−1 0 0 −1 w H] |

> **Stage-3 amendment (2026-11-07, fold verification)**: report §3's
> pin put p3 bottom-left and p4 bottom-right. Folding the sheet
> (bottom half behind, printed side out) and turning it over shows the
> bottom row mirrored left-to-right: BR reads left of BL. The pinned
> table therefore read p4 before p3 on the inner spread, which is the
> mirrored-panel bug class. The bottom row is swapped above. Reading
> sequence: cover (TL), turn over (TR), open to the spread (BR, BL).

Check for p3: local (0,0) maps to (w, H), the bottom-right corner of the BL quadrant, and local (w, h) maps to (0, h), its top-left corner. So the panel exactly covers [0,w] × [h,H], upside down, as required. p4 is the same shifted by w.

- `PanelXf::Rot180 { tx, ty }` carries (e, f). The renderer wraps each rotated panel's ops in `push_transform` / `pop`. It never rotates individual glyphs.
- **Unit tests** (pure, no krilla):
  - The four transforms map each panel's corners onto exactly their quadrant.
  - The quadrants tile the sheet with no overlap and no gap (proptest over random panel-local points, checking the image stays inside the assigned quadrant).
  - Applying the 180° map twice is the identity.
- **Content order across panels**: p1 holds the header plus the first sections. Overflow flows p1 → p2 → p3 → p4, in reading order of the folded booklet.
- **Physical verification**: stage 3 prints the golden fixture, folds it horizontally then vertically, and records the result in the stage report (report §3 duty; dispatch §6). A wrong panel order is a finding, fixed by swapping p2 and p3 in the table above (the recorded map-fold alternative). It is not a reason to change the model.

## 5. Wrap and measure helpers (`pdf/measure.rs`, `pdf/layout.rs`)

- `trait Measure { fn advance_pt(&self, text: &str, size_pt: f32) -> f32; }` is the seam that keeps layout pure.
  - Production: `FontMetrics`, built once from the embedded font through skrifa. It sums `advance_width(charmap(c))` scaled by `size / units_per_em`. A missing glyph is measured as `.notdef`.
  - Tests: `FixedWidth(f32)`, a handwritten fake where every char is k·size wide. This makes wrap goldens independent of the font file.
- `wrap(text, max_w, size, &impl Measure) -> Vec<String>`: greedy, breaking on whitespace. A single word wider than the line hard-breaks at the char boundary, never at a byte boundary, which matters for umlauts. Lines are trimmed and nothing is dropped.
- **Proptests**:
  - Concatenating the lines, ignoring whitespace, equals the input's non-whitespace characters. Nothing is lost or reordered.
  - Every line except a forced single-character break measures ≤ max_w.
  - Output is deterministic.
- **Known approximation**: measuring by summed advances ignores kerning. krilla shapes with rustybuzz, so the rendered width can differ slightly, almost always narrower in Latin text. A 0.5 pt safety slack per line absorbs this. The width check in the krilla smoke test (§8) is a floor, never a ceiling.
- **Single-script caveat** (report §4.1): krilla's `draw_text` does no BIDI. Content is German and English, so this is accepted and recorded at the type.

## 6. Backend (`pdf/render.rs`) and the embedded font

- `render(sheets: &[Sheet], font: &FontAssets) -> Result<Vec<u8>, PdfError>` creates a krilla `Document`. For each sheet it calls `start_page_with(PageSettings::from_wh(w, h))`, then for each op calls `draw_text`, `draw_path` (rules and checkbox frame plus tick), or `push_transform` around a rotated panel's ops. Then `finish()`.
- `PdfError` is a `thiserror` enum covering `FontLoad` and `Serialize(krilla error)`.
- **Determinism**: document metadata dates are not set. If krilla emits a creation date by default, stage 2 sets a fixed one or disables it so the byte smoke test is stable.
- **Font choice: Noto Sans, Regular and Bold** (Google / Noto project).
  - It has full Latin coverage, including `ä ö ü ß „ “ ‚ ‘ – …` and the ballot glyphs, and its metrics are very legible at 8 pt on kiosk-class printers.
  - **License**: SIL Open Font License 1.1. Embedding and redistribution inside an AGPL-3.0-only binary is permitted; the OFL only forbids selling the font by itself and requires keeping the license text.
  - Files: `crates/taskboard-app/assets/fonts/NotoSans-{Regular,Bold}.ttf` plus `OFL.txt`, loaded with `include_bytes!` inside the cfg-gated module, so default builds carry zero font bytes.
  - `cargo deny` does not scan non-crate assets. Stage 2 records the OFL under third-party notices (the README/licensing section or `docs/adr/0002`'s asset note) and adds the asset to the AGENTS §6 dual-tier rationale if one exists.
  - Size: about 2 × 0.55 MB in the feature-on binary. The krilla subsetter embeds only used glyphs in each PDF.
  - **Rejected alternatives**: DejaVu Sans (Bitstream-Vera-derived custom license, and a bigger file); Liberation Sans (also OFL, but weaker at small sizes).
- **Tick and checkbox**: drawn as paths, not glyphs, so they do not depend on font coverage.

## 7. Verb, destination config, viewer open

### 7.1 CLI

```
taskboard export --what pdf [--layout a4|a5|a6|fold] [--out <path>] [--no-open]
```

- `--layout` defaults to `fold`. `--out` and `--layout` are rejected (usage error) unless `--what pdf`, and the data-family filter flags (`--since`, `--task`, …) are rejected with `--what pdf`. Clap groups or a typed validation in `ExportQuery` enforce this.
- The **raw-payload posture** (D-T15, D-TC10) is unchanged for the CSV and JSON families. The pdf leg is the family's explicit file-writing extension (VF-3): it never writes PDF bytes to stdout.
- Its receipt rides the normal envelope: `PdfExportReceipt { path, pages, opened: OpenOutcome }`, where `OpenOutcome` is `Opened | Skipped | Failed(OpenError)`. Plain text output prints the path; `--json` prints the receipt.
- **Lanes**: both the one-shot lane (`export_command`) and the session/daemon lane (`export_line`) get the leg. The file is written by the process that resolves the path. In the daemon lane that is the daemon, so a relative `--out` is resolved against the client's cwd before sending. Stage 2 checks how the IPC request carries the cwd; if it does not, `--out` must be absolute in the session lane, enforced as a typed error. The viewer is opened by the client process, which owns the user's display session, never by the daemon.

### 7.2 Config: `[export]` section

```toml
[export]
pdf_dir = "~/Documents/taskboard"   # optional; default: dirs::document_dir() / "taskboard", else data dir
pdf_layout = "fold"                 # optional
open_after_export = true            # optional
```

- `ExportSection` follows `deny_unknown_fields` and `skip_serializing_if = is_default`, like `FocusSection` and `FilesSyncSection`.
- The section exists in all builds, so a config is portable between kiosk and desktop. Only the verb is gated. A kiosk build with `[export]` set ignores it without error.
- The file name is `taskboard-<YYYY-MM-DD>-<layout>.pdf`. An existing file is overwritten, since this is a regenerable artefact.
- The A1 pattern applies to `PdfDestination`, minted from `--out` or the config. It is a newtype with private fields and a fallible constructor:

```rust
pub enum PdfPathError {
    NotAbsolute,          // after ~ expansion and cwd resolution
    ParentMissing,        // `--out` names a file in a missing dir (config dir is created instead)
    IsDirectory,          // `--out` names an existing directory
    Write(io::ErrorKind), // create or write failed, e.g. permission
}
```

Each variant is a `CommandError` variant with its exit code. Tests assert only with `matches!`, never on text.

### 7.3 Viewer open (`pdf/deliver.rs`)

- `trait Opener { fn open(&self, path: &Path) -> Result<(), OpenError>; }`
  - Production `SystemOpener` uses `std::process::Command` with no new crate: Linux/BSD `xdg-open <path>`, macOS `open <path>`, Windows `cmd /C start "" <path>`. Selection is by `cfg(target_os)`. The child is spawned and detached, never waited on beyond the spawn.
  - `OpenError` covers `NotFound` (helper missing), `Spawn(io::ErrorKind)`, and `NoDisplay` (on Linux, neither `DISPLAY` nor `WAYLAND_DISPLAY` is set; checked before spawning).
- **Non-fatal**: an open failure leaves exit code 0, `opened: Failed(..)` in the receipt, and a warning line to stderr. The file was still written, which is the deliverable.
- Tests use a `RecordingOpener` fake (handwritten) to check it is called once with the written path, not called under `--no-open` or `open_after_export = false`, and that a failing fake still produces exit 0 and `Failed`. `SystemOpener` itself is not unit-tested (a process-spawn seam). The `NoDisplay` check is a pure function over an env snapshot, and that function is tested.

## 8. Golden and test design (lowest layer first)

| Layer | Test | Tool |
|---|---|---|
| `layout` | One golden per layout (A4, A5, A6, FoldA6x4) from a fixed `PrintDay` fixture with all four sections, a long title that wraps, umlauts and `„quotes“`, and an empty section that must be omitted. The `Sheet` ops are snapshot as a stable text dump: one line per op, coordinates rounded to 0.01 pt. Measured with `FixedWidth`. | `insta::assert_snapshot!` |
| `layout` | Empty day: header plus Notes only, one golden for the fold layout. | insta |
| `layout` | Fold overflow: 200 items gives exactly one sheet, and the last op on p4 is the `+N more` marker with the correct N. | unit |
| `layout` | Proptest: no op of a panel leaves its quadrant (via the §4 map); no text op's measured width passes the panel's right margin. | proptest |
| `measure`/`wrap` | §5 properties; `FontMetrics` gives a positive advance for each glyph of `„äöüß“`. This guards that the embedded font covers them. | proptest, unit |
| `print_day` | Fixed `AppState` + `now`: sections and order come from `today_sections`, and the item phrases match. Routing only: classification is already tested in `days.rs`. | unit |
| `render` | Smoke test: the output starts with `%PDF-`, `lopdf`-free structural checks (page count matches `sheets.len()`, a `/FontFile2` stream exists, MediaBox equals the A4 value). Rendering twice gives identical bytes. | unit |
| `deliver` | Destination minting variants, writing to a `tempdir`, the opener fake (§7.3). | unit |
| CLI integration | One test in `tests/cli_verbs.rs` (feature-gated): `export --what pdf --out <tmp> --no-open` exits 0 and the file exists with the `%PDF-` prefix. One `--json` receipt shape check (structural). Routing only. | assert_cmd style per the existing harness |

- Snapshots live next to the module (`src/pdf/snapshots/`), following the `web/render.rs` precedent.
- No snapshot of PDF bytes: krilla minor upgrades would churn them without any real regression. The geometry golden is the contract; the byte smoke test checks only structure.
- **Umlaut and quote rendering pinned** (dispatch acceptance): the fixture contains `Müßiggang „Prüfung“ – Größe`. The geometry golden pins its layout, the `FontMetrics` coverage test pins the glyphs, and the smoke test checks the page carries a text object.
- All pdf tests are `#[cfg(feature = "pdf-export")]`. Nextest runs default features locally, so stage 2 also runs `cargo nextest run -p taskboard-app --features pdf-export`. CI covers them through the `--all-features` coverage job.

## 9. TDD order (each step red → green, a commit per coherent slice)

1. `feat(app): the pdf-export feature gate and krilla pin`: dependencies, the empty cfg module, the CI tree-hygiene step, `deny` both ways. A spike test renders one glyph at (10, 10) and pins krilla's coordinate origin by reading the content stream (this decides whether `render` flips y).
2. `feat(app): the print fold geometry`: `PanelXf`, the four-panel table, and the §4 tests.
3. `feat(app): the print measure and wrap helpers`: `Measure`, `FixedWidth`, `wrap`, the embedded Noto font and OFL text, `FontMetrics`, and the §5 tests.
4. `feat(app): the print layouts`: `PrintDay`, `print_day`, `LayoutSpec`, the four layouts, overflow, and the goldens.
5. `feat(app): the krilla renderer`: `render` and the smoke tests.
6. `feat(app): the export pdf leg`: CLI flags, `[export]` config, `PdfDestination`, `PdfPathError`, the receipt, both lanes, the opener, and the integration test.
7. `docs(changelog): the PDF export`: CHANGELOG, licensing note, the user docs for the verb and `[export]`, and `Closes #624` in the PR body.

Quality gates per AGENTS §10 via `scripts/run-quality-gates.sh`, plus the feature-on nextest and deny runs.

## 10. Kiosk exclusion

- **Compile time**: default features omit `pdf-export`, so a kiosk or default binary has no PDF code, no krilla tree, no font bytes, and no `--what pdf` variant. Proof: the CI tree-hygiene step in §2.
- **Runtime**, in a feature-on binary running under a kiosk config (the appliance route, ADR 0016): the kiosk appliance does not route CLI verbs. Completions (`completion.rs:1121`) list `export` unchanged, and the `pdf` value appears only in feature-on builds through clap's `ValueEnum`. If the kiosk feature map has a verb allow-list (stage 2 checks `ui.rs` / `bootstrap.rs`), the pdf leg is excluded there explicitly, with a test.

## 11. Risks and open questions

1. **Release packaging (needs a user ruling)**: release binaries build default features, so desktop users get no PDF unless `release.yml` opts in for the desktop matrix. Recommendation: enable it for the x86_64 Linux and Windows desktop artefacts. Stage 2 asks rather than changing release CI silently.
2. **krilla coordinate or API drift**: krilla is 0.x, and docs.rs 0.8.2 is the reference. Step 1's spike pins the origin convention before any geometry is built on it.
3. **Measuring without kerning**: covered by the slack in §5. A visual regression would show in the stage-3 print check rather than in the goldens.
4. **Fold table not yet physically verified**: stage 3 performs the check; the fix is a table swap, not a redesign.
5. **Daemon-lane cwd and display**: `--out` resolution and who opens the viewer (§7.1). If the IPC protocol needs a cwd field, that grows the diff into `ipc/`. The fallback is requiring an absolute `--out` in the session lane.
6. **skrifa version duplication**: §2's fallback applies.
7. **Phrase reuse across the crate boundary**: the app calls `taskboard_ui_slint::presentation` helpers that are pure. If a reviewer objects to the app→ui-slint dependency for non-UI output, the helpers in question (`plan_box_time_phrase`, `effort_phrase`) are trivial and could move to the domain in a follow-up; not done here.
8. **Binary size**: about +1.1 MB of fonts plus krilla's code, only in feature-on builds.

## 12. Out of scope → backlog

- Tracking-rows overlay on the sheet (report §2, optional stage-B item): stage 2 opens a `backlog:` issue linking this plan.
- Per-board or date-range prints, and the user-editable templates: already fenced by R-4, so no issue.

## 13. Estimated diff

| Area | Lines (approx.) |
|---|---|
| `src/pdf/` (layout ≈ 450, measure/wrap ≈ 150, render ≈ 200, deliver ≈ 180, mod ≈ 40) | ~1,000 |
| Tests inside these modules plus goldens | ~650 + 5 snapshot files |
| CLI, config, commands wiring, receipt | ~250 |
| Integration test, CI step, Cargo, CHANGELOG, docs | ~120 |
| **Total** | **~2,000 lines** plus about 1.1 MB of binary font assets and `OFL.txt` |

This is consistent with the dispatch's size M, at its upper end.
