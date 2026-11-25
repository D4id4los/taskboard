# The clip-sentinel law chunk plan (round 8, chunk L1)

- **Issues**: #1418 (`backlog: C1 clip-sentinel structural classification
  (scrollbar thumbs vs real clips)`, p3 testing) + the C1 fix-now class
  (27 findings) + **#1419** (`backlog: the board columns horizontal
  scroll seek (1365) blocked on the clip-focus law`, ui) — **#1419 is
  the round's first OPERATOR GATE; this plan requests the ruling and
  stops the fix half at it**. Branch: `fix/clip-sentinel-law`.
- **Lineage**: #1365 (no horizontal scroll container around the board
  columns row — the overflow regime clips unreachable columns; the
  Flickable wrap was attempted and withdrawn, fead35c2 + revert
  f0afe0e6) and #1378 (closed: the AppWindow sever) are the severing
  class the C1 rubric names. The analyzer itself landed in round 7 G3
  (`scripts/layout_review.py`, the findings report + `findings.toml`
  source of truth).
- **Scope law** (dispatch plan §2): (1) the classifier's output is
  **typed** — `real-clip` / `thumb` / `chrome-speck` per the #1418
  rubric, never prose; (2) the analyzer's C1 rubric is **consumed, not
  re-derived** — the classifier feeds it; (3) the 27 fix-now findings
  in `.artifacts/reports/2026-11-25-layout-review-analyzer-findings.md`
  are the work list, that report is the source of truth; (4) the fix
  half is **gated** on the #1419 ruling.
- **Round laws**: [L](a) typed classifier output; [L](c) consumed
  rubric; [L](d) out-of-scope findings become `backlog:` issues —
  **none were discovered at planning time** (see §7).
- **Long-command discipline** (binding): pre-commit hooks +
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (flock-safe; background + poll; commit before any wait); every cargo
  invocation through `scripts/memory-gate.sh`; flock-deferral when the
  lock is contended and no useful work remains.

## 1. Diagnosis — the C1 detector today and the 27 findings

`scripts/layout_review.py` `detect_c1` scans the four canvas edges of
each golden for content runs (`content_mask` = deviation from the
corner-sampled modal background). Three exemptions frame the finding
population:

- **structural**: a run spanning ≥ `EDGE_RUN_EXEMPT_FRACTION` (30%) of
  its edge — side rails, window-chrome bands;
- **noise**: runs < `C1_MIN_RUN_PX` (3 px) — rounded-corner
  anti-aliasing;
- everything between becomes a C1 finding.

The **D5 rubric** (hand-applied after the run; the detector never sees
it) grades: extent ≥ 12 px → fix-now (flush panels, the #1365/#1378
severing class); extent < 12 px → accept-as-legacy (scrollbar thumbs at
8 px, chrome specks at 3 px). The gap #1418 names: the detector emits
one undifferentiated C1 finding, so the 8 px thumbs arrive as findings
and the rubric absorbs them **by hand on every re-run** — the grade is
human labour the detector could carry.

The 27 fix-now findings (the work list) group into five geometry
signatures over ten goldens — read from the findings report, which is
the source of truth:

| Group | Signature (px along the edge) | Goldens | Likely site class |
|---|---|---|---|
| **B-board** | left 12 (0.5×); left 72 + left/right 48 (3.0×) | `board-0.5-600`, `board-3.0-600` | the board columns row in the overflow regime — **this is #1365's clipped-columns signature and is gate-blocked** (§4) |
| **B-empty** | left+right 28, two y-rows each | `board-empty-1.0-1000` | the empty-board flush panels/columns stubs |
| **E-editor** | left/right 17 + 25, left 31 — the *identical* signature repeated across three goldens | `due-editor-datepicker`, `task-view-focus-cell`, `track-entries` (all 1.0×) | one shared composition surface (editor/track rows) flush at both canvas edges — fix once in the shared composition, three goldens heal |
| **E-recur** | left 31 | `recur-editor-detach-armed`, `recur-editor-modal`, `labels-overlay` | the modal/editor panel family, same sever as E-editor |
| **N-today** | right 133 + left 206 (notification); right 133 + 106 (today, 3.0×) | `notification-3.0-1000`, `today-3.0-1000` | large full-height flush panels — the classic severing-geometry collapse class |

The accept-as-legacy thumbs (8 px right-edge runs at scrollbar-track
positions) already carry recorded rationale in the report; the
classifier's job is to make the *detector* derive that disposition, not
to relitigate it.

## 2. The classifier design (#1418) — typed, constructed both ways

**Detection method: constructed cases both ways, per the G3
precedent** (each round-7 invariant was calibrated with synthetic
images exercising the hit and the miss). Before touching the detector,
build a fixture set of synthetic golden-like images containing: (a) a
scrollbar thumb — a short run flush to the right edge, inside a
darker track column that itself runs most of the edge height;
(b) a real flush panel — a wide content region whose edge run is
panel-coloured and contiguous with interior content; (c) a chrome
speck — a ≤ 3-5 px run at a window corner/chrome band; and the
negative cases (flush-free golden, structural rail). The classifier
must separate (a) from (b) **on the constructed fixtures first**; only
then is it run on the real corpus and reconciled against the hand
grades already in the findings report (they are the oracle — a
disagreement is a classifier bug unless the hand grade is provably
wrong, and any proven-wrong hand grade is recorded in the report, not
silently flipped).

**The typed signal** (round law [L](a)) — `detect_c1` gains a
`classification` field with exactly three values:

- `thumb` — right-edge run whose signature matches the scrollbar
  model: extent within the thumb-size band calibrated from the corpus
  (the 8 px class), positioned at a plausible track offset, **and**
  accompanied by a track-column signature (a low-contrast vertical
  band along the same edge spanning a much larger extent than the run
  — the track the thumb sits in). The scrollbar model, not a bare size
  threshold: a real 8 px flush panel sliver without a track must not
  classify as `thumb`.
- `chrome-speck` — sub-threshold runs at frame periphery (the existing
  `< C1_MIN_RUN_PX` noise class, promoted to a named classification so
  the rubric consumes it explicitly instead of it being pre-filtered
  into silence).
- `real-clip` — everything else; the severing class.

**Rubric consumption (law [L](c))** — the D5 grading step
(`default_grade` + the report emitter's rule strings) stops reading raw
extent and reads the classification: `thumb`/`chrome-speck` →
accept-as-legacy (rationale emitted from the classification +
calibration note, so the grade is reproducible), `real-clip` with
extent ≥ 12 px → fix-now, `real-clip` below 12 px → fix-later (a real
drift smaller than the severing class — today the rubric has no
home for it; the classification makes the gap visible instead of
blurring it into the thumb band). The hand-applied grades in
`findings.toml` are re-reconciled once after the classifier lands and
the report regenerated; the extent-12 px line stays in the metric
strings as the calibration record.

**Enforcement**: `tests` for the classifier itself live beside the
analyzer's existing tests (constructed-case pairs — thumb-in-track
classifies `thumb`, bare sliver classifies `real-clip`, speck
classifies `chrome-speck`), plus a corpus reconciliation check that the
classifier's dispositions agree with the recorded hand grades or the
disagreements are enumerated in the regenerated report.

## 3. Fix shapes for the 27 — grouped by class (pre-ruling sketch)

The severing idiom from #1365/#1378 — a bounded max/inset at the
composition surface + a probe twin pinning it — applies to the panel
groups; the exact bounds come from stage-2's measurement of each site:

- **E-editor + E-recur** (14 findings): the shared editor/modal rows
  and panels gain the inset/sever so no panel edge reaches the canvas
  edge; one fix in the shared composition where the signature is
  shared, per-site where it is not. Each healed golden's finding is
  re-run to zero.
- **N-today** (4 findings): the full-height flush panels get the
  severing max/inset treatment; the notification panel and the Today
  page's flush regions are separate sites even though the signature
  matches.
- **B-empty** (4 findings): the empty-board stubs — likely the same
  columns-row geometry as B-board at rest; stage-2 verifies whether
  one fix serves both.
- **B-board** (5 findings): **gate-blocked** — this is the overflow
  regime's clipped columns, i.e. #1365's subject. Whether these find
  their fix as "the Flickable wrap re-lands (ruling option A)", "the
  clipped subtree is a typed dead-end and the UI must not enter it
  (option B)", or "deferred to #1359's scrollbar work" is decided by
  the ruling. No board fix is attempted before the gate returns.

Determinism law: any fix that changes committed goldens re-records
them; the re-record is deterministic per the round-7 G1 law, and the
regenerated findings report is committed in the same change so the
report and the code never disagree.

## 4. OPERATOR GATE — the #1419 clip-focus law ruling (requested, not assumed)

**The question**: when a focus move targets an element inside a
clipped (scrollable/overflow) container, what is the law? The empirical
bisect (2026-11-25, revert f0afe0e6's trail) established that *any*
clipped subtree at the columns row drops the focused inline-create
field's slint focus on every model refresh — the clip property alone
reproduces it. The ruling decides L1's B-board fix shape and the
#1419 fix prompt's step 1.

### Option A — clipped focus auto-scrolls the nearest scrollable ancestor (the law text)

Focusing an element inside a clipped container is legal and the
container owes the user visibility: the nearest scrollable ancestor
seeks so the focused element enters the viewport (the A5a
`seek-page` hint-consumer precedent, commit 83e5f7af, generalized).
The refresh-synchronous focus drop is a **synthetic unmount echo, not
a dismissal**: `on_text_flow_focus_lost` (adapter.rs ~15722) learns to
distinguish it — a refresh-in-progress flag is set around the push
pass and absorbed in the seam — and the Flickable wrap re-lands as
fead35c2 had it.

- *Enforcement seam*: two test pins together — (1) the clip-focus
  regression (`view_component_tests::editing_dispatches_nothing_and_exits_via_escape`,
  the "mid-edit refresh keeps the flow open" case) must pass *with* the
  Flickable wrap present; (2) the seek probe re-derived from fead35c2's
  `columns_seek_probe_tests.rs` (overflow landing moves the columns
  element's window x negative, clamped; fitting regime stays at rest).
  The walk-ladder interaction: a focus walk that crosses into a
  clipped subtree continues through it — the seek is the walk's
  responsibility, so the ladder never silently strands.
- *What the user sees*: the focused element always visible; clipped
  columns reachable by keyboard focus and pointer alike.
- *Cost/risk*: the absorb seam touches the adapter's text-flow settle
  path (~15.7k-line file); the wrap re-lands the geometry the width
  resolver's oracle reads (D-1365a holds — the resolver reads the
  viewport width app-side, so the oracle input is unaffected; the
  board goldens held through fead35c2).

### Option B — clipped focus is a typed dead-end (the law text)

Focusing into a clipped subtree is **not legal**: the focus walk
treats a clipped container as a stop signal — focus never enters an
element the container cannot show, and the UI must guarantee it
(focus-forwarding skips clipped subtrees; the columns row either
reflows/wraps so nothing is clipped, or clipped content is presented
by another surface, e.g. #1359's scrollbar affordance making the
exclusion visible).

- *Enforcement seam*: a typed dead-end signal in the focus-walk layer
  (a `FocusTarget::Clipped`-class outcome the walk consumes), plus a
  test pin that no focus move can land inside a clipped subtree; the
  clip-focus regression passes because the Flickable wrap is **not**
  re-landed and the row never clips while a text flow is open.
- *What the user sees*: focus never disappears into an unreachable
  region; the price is that clipped columns stay pointer-only (or
  wrapped) until another surface covers them.
- *Cost/risk*: no adapter seam change; but the B-board C1 signatures
  may remain unfixed (a clipped row that is *declared* legal becomes
  accept-as-legacy with the recorded law as rationale), and the
  walk-ladder needs the dead-end type plumbed through the focus order.

### Recommendation: **Option A**

Rationale: the bisect proved the focus drop is a *refresh-synchronous
echo* (clip property alone, all wiring neutralized) — i.e. a synthetic
unmount misread as dismissal, a defect in the absorb site, not a
semantic the UI chose. Option A fixes the actual defect, heals the
B-board C1 class, restores keyboard reachability #1365 records as the
residual gap, and reuses two proven precedents (the `seek-page`
hint-consumer, fead35c2's green pins). Option B legalizes unreachable
focused content — the "no pins of today's behavior" law reads that as
recording a limitation as a contract.

**The gate request to the operator**: rule A or B for the clip-focus
law. Stage-2's fix half starts only after the ruling; the classifier
(§2) is **not** gate-blocked and may proceed in parallel.

## 5. Stage map

- **Stage 1 (this plan)**: committed here; gate requested; no
  implementation.
- **Stage 2**: (a) the classifier + its constructed-case tests +
  findings-report reconciliation (ungated, lands first); (b) the fix
  half per §3 — E-editor/E-recur/N-today/B-empty ungated, B-board per
  the ruling. Gate: pre-commit hooks + flock-wrapped nextest; every
  cargo through `scripts/memory-gate.sh`.
- **Stage 3**: review; scope ends at PR-open (the dispatcher owns the
  full §10 suite, CI watch, teardown, at-merge closures).

## 6. Verification sketch

- Classifier: constructed-case pairs pass; corpus re-run reproduces
  the 1063-finding population with classifications attached; the 56
  C1 findings re-disposition (27 real-clip fix-now until healed, 29
  thumbs/specks accept-as-legacy with emitted rationale).
- Fixes: each healed site's golden re-recorded; the regenerated
  findings report shows the corresponding C1 rows gone; the
  clip-focus regression and the seek probe (per the ruling) green.
- No text assertions anywhere (typed outcomes per AGENTS §5); no
  sleeps (deterministic pins only).

## 7. Backlog issues

None filed at planning time: every out-of-scope thread found while
reading the seams already has an issue (#1359 scrollbar widget,
#1392 kiosk-scale truth — L3's, #1417 estimator upgrade — L2's). The
standing authorization stays live for stages 2–3.
