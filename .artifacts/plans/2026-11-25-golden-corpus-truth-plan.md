# The Golden Corpus Truth Chunk Plan (Round 8, L3)

Date: 2026-11-25 · Branch: `test/golden-corpus-truth` · Issues: #1392,
#1390, #1382, #1416, #1414 · Scope report §A-L3 (normative).

Goal: the goldens directory becomes self-describing truth — the capture
methods, the file name stems, and the manifest axes all agree — and the
shadow/contrast token dispositions are recorded per site. Evidence at
hand-off: a re-run of `scripts/layout_review.py` over the corpus plus
`scripts/ux-coverage.sh <plan> --gate` (where applicable) and the green
nextest suite.

All paths below are under `crates/taskboard-ui-slint/` unless noted.

---

## 1. Per-item diagnosis

### 1.1 #1392 — the kiosk goldens render at scale 1.0 while claiming 0.5

Site: `tests/journeys/golden_visuals.rs`

- `kiosk_rotation_settings()` (line 590) builds `UiSettings` from
  `..UiSettings::default()` — no `scale` field. `UiSettings::default()`
  carries scale 1.0; the mount applies it (`src/lib.rs:291` sets
  `Theme.base-scale` from `settings.scale`).
- `KIOSK_SCALE: f64 = 0.5` (line 610) is used **only** as capture
  metadata: the `GoldenMeta.scale` field and the
  `kiosk_golden_path(mode, KIOSK_SCALE, KIOSK_WIDTH)` filename (line
  615). The desktop matrix legs use `scale_settings(scale)` (line 125),
  which does set `UiScale::new(scale)`.
- Affected goldens — **three**, not the two the issue names (new fact,
  commented on the issue):
  1. `kiosk-board-0.5-1000` (`the_kiosk_board_golden_holds`)
  2. `kiosk-today-0.5-1000` (`capture_kiosk`, "today" mode)
  3. `today-habits-kiosk-0.5-1000` (the wave-2 kiosk habits golden,
     capture site at `golden_visuals.rs:2504`, which also calls
     `kiosk_rotation_settings()`)
- All three manifest rows record `scale = 0.5` (`tests/goldens/
  manifest.toml:26,56,284`) — consumed as truth by
  `scripts/layout_review.py` (`read_manifest`, line 141) when it
  multiplies logical-px expectations by scale, so every analyzer
  geometry expectation on these three cells is currently computed from
  a scale the pixels were not rendered at.

**Recorded fix direction (the TRAP resolved): fix the capture to match
the claim — the 0.5 name is truthful.** Rationale:

- The kiosk cells exist as *matrix cells* in the scale×breakpoint
  matrix; the matrix's purpose is scale robustness at the kiosk
  appliance extreme (low-power small displays). The icon probe corpus
  already treats 0.5 as "the kiosk panel's sub-pixel extreme"
  (`probe_edge_golden(0.5, ...)`, R5a) — 0.5 is the kiosk scale of
  record in this corpus.
- The review agent and the analyzer read the manifest as truth; the
  three manifest rows are cross-referenced against the desktop matrix
  cells at the same scale axis. Renaming to 1.0 would make the kiosk
  cells duplicates-in-axis of `board-1.0-*`/`today-1.0-*` cells and
  lose the scale-robustness value.
- Implementation: `kiosk_rotation_settings()` layers the kiosk fields
  on top of `scale_settings(KIOSK_SCALE)` (`..scale_settings(KIOSK_SCALE)`
  as the base, then `kiosk: true`, `kiosk_rotation: ...`, keeping the
  pinned clock — `scale_settings` already pins the same clock, so the
  clock line is unchanged in effect).
- Re-record all three goldens (`TASKBOARD_REGEN_GOLDENS=1`) in their
  own commit with a dated note (manifest `state` fields unchanged in
  meaning; the re-record commit message carries the date + reason, per
  the golden law: comparator tolerance untouched).

### 1.2 #1390 — stale width/height in golden name stems

Ground truth (decoded from the committed PNGs):

| stem | actual PNG | manifest axes | verdict |
|---|---|---|---|
| `alarm-window-1.0-1000` | 351×64 | 351×64 | **stale stem** |
| `alarm-window-1.0-600`  | 600×64 | 600×64 | fine |
| `alarm-window-1.0-1500` | 1500×64 | 1500×64 | fine |
| `icon-probe-1.0-396` | 392×420 | 392×420 | **stale stem** |
| `icon-probe-0.5-396` | 392×420 | 392×420 | **stale stem** (issue named only 1.0; commented) |
| `icon-probe-3.0-396` | 392×420 | 392×420 | **stale stem** |

The manifest is the truth (the bijection test
`the_manifest_bijects_with_the_golden_corpus`,
`golden_visuals.rs:292`, verifies manifest axes against decoded PNG
dims) — the stems are the readability defect. The #1390 law is already
codified at `wave2_golden_path` (`golden_visuals.rs:2214`): "the name
axes are the capture's actual axes; no stale width/height in a stem".

Decision: **rename the stems to the actual rendered width** (keep the
`<surface>-<scale>-<width>` convention rather than inventing a
suffix-less content-sized variant):

- `alarm-window-1.0-1000` → `alarm-window-1.0-351`
- `icon-probe-1.0-396` → `icon-probe-1.0-392` (and `0.5`, `3.0` likewise)

Consumers to update **atomically in one commit** (git mv + edits
together, so the suite never sees a half-renamed corpus):

1. The PNG files: `git mv tests/goldens/alarm-window-1.0-1000.png
   tests/goldens/alarm-window-1.0-351.png`; `git mv` × 3 for the icon
   probes.
2. `tests/goldens/manifest.toml` `name` fields (lines 158, 200, 224,
   230).
3. `tests/probes/icon_probe_tests.rs`: the doc comment (line 8), the
   hardcoded path (line 132), the `GoldenEntry.name` upsert (line 274),
   the `probe_edge_golden` calls (lines 367, 374) — the 0.5/3.0 names
   are string arguments here.
4. The alarm-window capture site: it builds its path from the matrix
   width variable via the gap-golden path helper
   (`golden_visuals.rs:1328`, `{surface}-1.0-{width}.png`). Stage 2
   must special-case the alarm-window stem (the capture *window* is
   1000 logical px wide; the *rendered* PNG is content-sized at 351) —
   give the alarm golden its own literal path or a content-width
   parameter so the stem matches the PNG, not the window parameter.
5. No consumer changes needed: `scripts/layout_review.py` reads only
   `manifest.toml` (verified — no stem/path parsing of goldens names);
   `scripts/ux-coverage.sh` has no golden path references. Re-running
   the analyzer over the renamed corpus is the acceptance evidence.
   Byte content untouched — `the_committed_corpus_reencodes_byte_identical`
   (`golden_visuals.rs:266`) must stay green without a re-encode.

### 1.3 #1382 — the `plan_window_max_probe` rename (census's last alias)

Ground truth in the worktree:

- `tests/inventory/severing_census.rs:124`:
  `const TWIN_ALIASES: &[(&str, &str)] = &[("plan-next-day-view.slint",
  "plan_window_max_probe")];` — the one witnessed alias.
- The probe itself: `tests/plan_window_max_probe.slint`;
  `tests/probes/plan_window_max_probe_tests.rs`;
  `tests/probe_composition.rs:57-58` (`#[path = ...] mod
  plan_window_max_probe_tests;`).
- `build.rs` discovers probes by `read_dir` over `tests/*.slint`
  (lines 36-48, the #1362 law) — **no build.rs array edit** (the issue
  text predates the auto-discovery). Stage 2 must verify the probe's
  `OUT_DIR` include name inside
  `plan_window_max_probe_tests.rs` (it includes the generated probe
  header by file stem) — that include string must rename with the
  file.

Rename to `plan_next_day_window_max_probe` (matching the landed
`task_window_max_probe` D3 pattern), mechanical:

1. `git mv tests/plan_window_max_probe.slint
   tests/plan_next_day_window_max_probe.slint` (+ internal comment refs).
2. `git mv tests/probes/plan_window_max_probe_tests.rs
   tests/probes/plan_next_day_window_max_probe_tests.rs` (+ the
   generated-header include inside it).
3. `probe_composition.rs:57-58` module path + name.
4. `severing_census.rs`: empty the `TWIN_ALIASES` table. If the table
   and its lookup branch (line 127) become dead, delete both — the
   census law then holds with no aliases at all. Keep the census test
   green: it derives the twin name as stem-minus-`-view`, dashes→
   underscores, so `plan-next-day-view.slint` →
   `plan_next_day_window_max_probe` must resolve through the plain
   derivation.

### 1.4 #1416 — the 8px drop-shadow sites vs `Theme.shadow-blur`

The token: `Theme.shadow-blur` = 12px × base-scale. The tokenized
floating-overlay population already uses it: `recur-editor.slint:99`,
`search-palette.slint:48`, `focus-overlay.slint:61`,
`modal-dialog-host.slint:240`.

Per-site disposition:

| site | element | recipe | disposition |
|---|---|---|---|
| `ui/widgets/menu-shell.slint:34` | the action-menu / context flyout — a floating popup over the app, same overlay class as search-palette | `space-2` blur + `surface-hover` shadow colour, focus-ring border | **fix → `Theme.shadow-blur`** (same recipe as the other flyouts; the softer blur is drift, not intent). Evidence gate: the `action-menu-1.0-1000` golden re-records. |
| `ui/compositions/date-picker.slint:104` | the date-picker grid popup — same floating-anchor overlay class | same recipe as menu-shell | **fix → `Theme.shadow-blur`**. Evidence gate: the `due-editor-datepicker-1.0-1000` golden re-records. |
| `ui/app-window.slint:1649` | the board colour strip — a 24px-tall inline swatch bar (`height: space-4 + 2·space-2`), not a floating overlay | `space-2` blur | **waive, recorded**: a 12px blur on a 24px strip washes the element into its background; the tight 8px blur is a deliberate micro-element recipe. Waiver comment at the site + recorded in the plan handoff; not a token change. |

Both fixes change pixels → the two named goldens re-record (dated note
commit), same protocol as §1.1. Waiver text at the site must say why
(deliberate softer blur on a micro element), per the #1121 naming bar.

### 1.5 #1414 — the contrast pair list skips raised/hover

Site: `src/token_tests.rs`, `PAIRS` (line 146). Today the state colours
(`Semantic.success/warning/danger/overdue`) and `Theme.border` are
paired only against `BACKGROUND` and `SURFACE`, while `text-primary`/
`text-secondary`/`focus-ring` already carry the raised/hover legs — the
list's own contract ("every text-bearing foreground × every surface")
is unmet for exactly the five issue foregrounds.

Extend `PAIRS` by ten entries (stage 2, TDD: fails pre-fix):

- `Semantic.success` × `SURFACE_RAISED`, `SURFACE_HOVER` (Body)
- `Semantic.warning` × both (Body)
- `Semantic.overdue` × both (Body)
- `Semantic.danger` × both (Body) — **expected fail** on hover (4.19:1
  < 4.5, sites `status-bar.slint:375`, `review-view.slint:693` to
  verify at stage 2)
- `Theme.border` × both (NonText) — **expected fail** on raised
  (2.75:1) and hover (2.34:1) vs 3.0; markup consumers verified in the
  issue (tooltip.slint:134-136 et al.)

Direction for the two failing legs: this chunk lands the pair
extension (the gate). The remediation (raise `Theme.border` luminance,
or a distinct control-boundary token, or markup changes) is a design
call out of L3 scope → **backlog issue** (see §4). To keep the suite
green without weakening the contract, the two failing entries land as
explicitly-named debt entries in `PAIRS` — a per-pair exemption
variant (e.g. `MinRatio::Waived` with a reason comment naming the
backlog issue), never a lowered threshold and never a text assertion.
The test asserting them honestly (a `..._currently_pins_` companion or
the waiver variant carrying the measured ratio in its failure message
when it worsens) stays within the no-quality-ceiling law: if the
border token later improves, the waiver is deleted and the real
minimum flips on.

---

## 2. Stage-2 work list (order)

1. **#1382 probe rename** (no pixel changes): mechanical rename per
   §1.3; census green.
2. **#1390 stem renames** (no pixel changes): atomic commit per §1.2;
   bijection + byte-identity tests green.
3. **#1392 scale fix + re-record**: `kiosk_rotation_settings()` rides
   `scale_settings(KIOSK_SCALE)`; re-record the three kiosk goldens in
   their own dated commit; per-item pins (TDD: a stage-2 pin asserting
   `settings.scale == KIOSK_SCALE` at the kiosk capture fails pre-fix).
4. **#1416 shadow unification + re-record**: two fixes + one recorded
   waiver; `action-menu-1.0-1000` and `due-editor-datepicker-1.0-1000`
   re-record with dated notes.
5. **#1414 pair extension**: ten new `PAIRS` entries, waiver variant
   for the two failing legs; typed assertions only.
6. Re-run `scripts/layout_review.py tests/goldens/` + the diff of
   findings on the three kiosk cells (the acceptance evidence);
   `scripts/ux-coverage.sh` gate if any plan in scope is journey-gated
   (these are test-corpus internals — no journey gate expected).

Every `cargo` invocation through `scripts/memory-gate.sh`; the full
suite through `scripts/run-quality-gates.sh cargo nextest run
--workspace`; pre-commit hooks on. Commit each item separately
(`test(goldens): ...`, `refactor(probes): ...`, `fix(ui): ...`).

## 3. Stage-3 traps

- **The kiosk re-record changes bytes** — the comparator tolerance is
  untouched; a drift complaint there is the fix working, not a flake.
- **`the_committed_corpus_reencodes_byte_identical` must stay green
  across the renames** — the renames are `git mv` only; any diff in
  PNG bytes there means someone re-encoded, which needs its own
  whole-corpus regen commit.
- **The census test**: after the alias deletion, the derivation
  (stem-minus-`-view`, dashes→underscores) must resolve
  `plan_next_day_window_max_probe` from `plan-next-day-view.slint` —
  verify the census actually enumerates the renamed file, not just
  that it compiles.
- **The wave-2 kiosk habits golden** (`today-habits-kiosk-0.5-1000`)
  is easy to miss — its capture site (§1.1, third bullet) shares
  `kiosk_rotation_settings()` and re-records with the other two.
- **`build.rs` rerun-on-new-probe**: renaming the probe file changes
  the `read_dir` result; the build script's rerun-if-changed footprint
  must cover it (it does per #1362 — verify with one clean build).
- **No text assertions** anywhere in the new pins; typed outcomes
  (`matches!`, axis equality, measured ratios) only.

## 4. Backlog issues opened at stage 1

See the stage-1 report for the final numbers; expected:

- The contrast remediation exposed by the extended pairs (#1414 step
  2): border-on-raised/hover below 3:1 (tooltip boundary, etc.) and
  danger-on-hover below 4.5:1.
- (New facts commented on existing issues, not new issues: the third
  kiosk golden on #1392; the 0.5/3.0 icon-probe stems on #1390.)
