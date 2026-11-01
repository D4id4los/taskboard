# [Plan] UI R4 chunk 3 — icon assets (vendored Lucide SVGs, the Icon/colorize component, the IconKind migration, distinct notification icons, the D19 amendment)

- **Date**: 2026-11-01
- **Status**: PLANNED — stage-1 plan for chunk 3 of GUI review round 4.
  **Decision gate PASSED**: the round-4 plan §2 decision 1 (vendor
  monochrome Lucide SVGs, ISC) reads **CONFIRMED — confirmed by the
  user on 2026-11-01** (`.artifacts/plans/2026-11-01-gui-review-round4-plan.md`
  header + §2); the dispatch plan §Status repeats it. No PROPOSED
  residue.
- **Origin** (normative): round-4 plan §3.3 (D9 asset pipeline, D10
  Icon component + migration, D11 distinct notification icons, D12 D19
  amendment) + §2 decision 1 + §1 finding 1 (F1) + §7-R2 (the
  colorize+SVG runtime-behaviour risk) + §4 (icons testing rules);
  dispatch plan §1 (ordering, gates — chunk 3 runs in parallel with
  chunk 4, no upstream gate) and §4 (chunk 3 acceptance + prompts).
- **Branch**: `feat/ui-r4-icon-assets` (this worktree:
  `../taskboard-WT-feat-ui-r4-icon-assets`, created off `main`
  4d8be313 after `scripts/worktree-doctor.sh` passed — 234 G free,
  floor 25 G).
- **Scope**: `crates/taskboard-ui-slint` only (slint sources, theme
  token, tests, vendored assets). **Empty diff** over
  domain/state/storage/sync/app — no Rust behaviour change at all
  (IconKind is never referenced from Rust: verified by grep over
  `src/` and `tests/`).
- **Non-goals** (dispatch §4): no new icons beyond the enumerated set
  (6 new kinds below); no non-IconKind icon usage introduced anywhere;
  no tone-map changes (D10: `icon-color()` untouched); no
  IconButton focus/tooltip/geometry changes; no PlusGlyph rework; no
  icon-font clause changes beyond D12's amendment.

## 1. The vendored set — every file, one row each

Vendored verbatim from `lucide-icons/lucide` (ISC), pinned to the
current upstream release tag at download time (recorded in the
license file's provenance header). Destination:
`crates/taskboard-ui-slint/ui/assets/icons/<lucide-name>.svg`, one
monochrome SVG per icon (Lucide ships stroke-based
`stroke="currentColor"` single-colour files — colorize-safe by
construction; stage 2 self-checks each file for absence of baked
colours: no `#rrggbb`, no `fill="` other than `"none"`).

**Self-check on names**: `warn` has no call site today (dead
vocabulary entry) — it is still mapped 1:1 per the dispatch
acceptance ("the 13 existing kinds map 1:1") and stays available to
later chunks (chunk 8's needs-due-date state is a likely consumer).

| # | IconKind row (unchanged API) | Lucide asset | Parity note (drawn glyph → asset) |
|---|---|---|---|
| 1 | `menu` | `menu.svg` | Three horizontal lines — the drawn 3-bar hamburger, now Lucide-weight |
| 2 | `snooze` | `clock.svg` | Ring + hands — the drawn clock, 1:1 |
| 3 | `mute` | `bell.svg` | The drawn dome+base bell silhouette; plain bell (no slash — parity; state rides tone) |
| 4 | `warn` | `triangle-alert.svg` | **Recorded deviation**: the drawn bare "!" has no Lucide equivalent; triangle-! is the canonical warning and stays visually distinct from dismiss's circle-x |
| 5 | `plus` | `plus.svg` | Exact (the drawn two crossed bars) |
| 6 | `chevron-left` | `chevron-left.svg` | Exact polyline |
| 7 | `remove` | `x.svg` | Exact (bare x; dismiss is the circled variant) |
| 8 | `minus` | `minus.svg` | Exact |
| 9 | `chevron-right` | `chevron-right.svg` | Exact polyline |
| 10 | `calendar` | `calendar.svg` | Exact — the drawn box + two tabs + header rule *is* Lucide's calendar |
| 11 | `check` | `check.svg` | Exact polyline (`M 2 6 L 5 9 L 10 3` at 12-grid ≙ Lucide's 24-grid check) |
| 12 | `play` | `play.svg` | **Recorded deviation**: drawn filled triangle → Lucide outline polygon (style unification, below) |
| 13 | `stop` | `square.svg` | **Recorded deviation**: drawn filled square → Lucide outline square (style unification, below) |
| 14 | `dismiss` *(new)* | `circle-x.svg` | D11's X-in-circle — distinguishes "Dismiss notification" from "Mark task done" (F1's bug) |
| 15 | `timer` *(new)* | `timer.svg` | Stopwatch for the countdown-timer surfaces (chunk 9's setter); D11's `alarm-clock` recorded as the alternative — `timer` reads "countdown" and stays legible at 12 px |
| 16 | `link` *(new)* | `link.svg` | Connected indicator (chunk 11's connections dropdown) |
| 17 | `link-off` *(new)* | `unlink.svg` | Not-connected indicator. Lucide has no plain `link-off` file — the chain-link pair is `link`/`unlink`. Fallback if `unlink.svg` is absent at the pinned tag: `link-2-off.svg` (same semantics); stage 2 pins whichever exists and records it |
| 18 | `chevron-down` *(new)* | `chevron-down.svg` | Dropdown affordance (chunk 11's shared dropdown) |
| 19 | `pencil` *(new)* | `pencil.svg` | Edit affordances (chunks 5/6) |

- 19 files, ≈8–12 KB total (Lucide SVGs run 150–600 B each). Expected
  **binary** delta ≈ +10–20 KB (assets + generated mapping code,
  minus the ~200 deleted geometry lines); the stage-2 report records
  the measured before/after release-binary size (dispatch prompt 3.2).
- Style unification is the accepted consequence of the confirmed
  decision: Lucide is uniformly outline-stroke (2/24 stroke width ⇒
  1 logical px at `icon-size` 12 px), where several drawn glyphs were
  filled (play, stop, mute's dome). Parity bar: same affordance reads
  the same — silhouette proximity per the table — **not** pixel
  parity; the regenerated goldens are the reviewed diff.
- Unreferenced files cost nothing: embedding follows `@image-url`
  references (`EmbedResourcesKind::EmbedAllResources` for the Rust
  output — verified in the pinned compiler, `i-slint-compiler-1.18.1/lib.rs:244`),
  so the not-yet-mapped new-kind files add zero weight until their
  enum rows land.

## 2. Component architecture

### 2.1 `ui/widgets/icon.slint` (new) — the renderer + the vocabulary home

```slint
// SPDX-License-Identifier: AGPL-3.0-only
// The icon renderer (r4 D10): the closed IconKind vocabulary mapped
// to vendored monochrome Lucide SVGs, tinted via Image.colorize
// (D19 as amended — round-4 §2 decision 1; the no-icon-font clause
// stands). Assets are compile-time embedded (@image-url; slint-build
// embeds all resources) — offline kiosk-safe, no runtime loading.
// Growth rule (unchanged): one recorded theme decision = one enum
// row + one vendored file + one mapping arm.
import { Theme } from "../theme.slint";

export enum IconKind { menu, snooze, mute, warn, plus,
    chevron-left, remove, minus, chevron-right,
    calendar, check, play, stop,
    // r4 D11 growth: dismiss/timer/link/link-off (F1 + chunk 11),
    // chevron-down (the dropdown), pencil (edit affordances).
    dismiss, timer, link, link-off, chevron-down, pencil,
}

export component Icon inherits Image {
    in property <IconKind> kind: IconKind.menu;
    width: Theme.icon-size;
    height: Theme.icon-size;
    // Lucide ships stroke="currentColor" — resvg resolves that to
    // black, invisible on the dark theme. The default tint is the
    // neutral icon tone so a caller that forgets still renders.
    colorize: Theme.text-secondary;
    source: kind == IconKind.menu        ? @image-url("../assets/icons/menu.svg")
        : kind == IconKind.snooze        ? @image-url("../assets/icons/clock.svg")
        /* … total chain, one arm per §1 row … */
        : @image-url("../assets/icons/triangle-alert.svg");
}
```

Settled decisions in the shape:

- **The enum moves from `icon-button.slint` to `icon.slint`.** The
  mapping function must name the enum, and `icon-button.slint` must
  import `Icon` — keeping the enum in the button would make the
  import cycle `icon-button → icon → icon-button`. `icon-button.slint`
  re-exports it (`import { Icon, IconKind } from "icon.slint";
  export { IconKind }`) so **all 13 call-site imports stay
  untouched** (`import { IconButton, IconKind } from
  "../widgets/icon-button.slint"` keeps compiling). Fallback if the
  pinned compiler rejects the re-export form: mechanically update the
  13 import lines to also name `icon.slint` — API unchanged, call
  sites still keep their `IconKind` API.
- **The mapping is one total conditional expression over
  `@image-url` operands** — `@image-url(...)` is an ordinary
  expression in the pinned grammar (`i-slint-compiler-1.18.1/parser.rs:412–414`,
  `AtImageUrl` listed alongside `ConditionalExpression`), and
  conditionals unify branch types via `common_expression_type`
  (`passes/resolving.rs:2169–2181`) — two `ImageReference` branches
  unify. **Terminal else = the `warn` asset (the loud fallback)**: a
  future kind added without a mapping arm visibly renders the warning
  glyph in dev and goldens instead of silently vanishing.
- **Recorded fallback shape** if the conditional-over-images form
  hits a codegen corner the grammar check above missed: `Icon
  inherits Rectangle` containing 19 `if (kind == …): Image`
  conditional children (the proven construct) — a recorded
  micro-deviation from D10's "inherits Image"; probe golden
  regenerates identically.
- **`Icon` owns no tone property**: callers bind `colorize:` directly
  (the inherited `Image` property) from *their* tone source — the
  button's map, the status bar's `Theme.text-primary`. The component
  default (`Theme.text-secondary`) only covers the forgetful caller.
- **`@image-url` paths resolve relative to the .slint file**: from
  `ui/widgets/` the vendored dir is `../assets/icons/` (D9's
  `ui/assets/icons/` home).
- **No build.rs change for assets**: `slint-build` compiles
  `ui/app-window.slint` and the compiler embeds referenced resources
  (§1). The probe registration in build.rs is the one addition (§3).

### 2.2 `Theme.icon-size` (new token)

```slint
// The icon glyph's edge (r4 D10): the square every vendored SVG
// renders in — Lucide's 24-grid at the icon-button's inner glyph
// footprint (the drawn Path blocks' space-3 edge).
out property <length> icon-size: 12px * base-scale;
```

A named token, not `space-3` reuse: the theme is the only legal home
for dimension arithmetic (theme.slint header; token lint rule 3), the
edge is a glyph-metric rather than a spacing step, and every icon
consumer (button, status bar, later chunks) shares it. 12 px ×
base-scale = the drawn `Path` blocks' exact footprint
(`icon-button.slint:177` etc.), so button geometry, the space-2
padding, and the space-6 square are untouched.

### 2.3 `ui/widgets/icon-button.slint` — the migration

- Header comment: D19 amendment (wording in §5).
- The 13 `if (root.icon == …)` geometry blocks (`:104–309`) die.
  The `icon-box` becomes:

  ```slint
  icon-box := VerticalLayout {
      padding: Theme.space-2;
      pure function icon-color() -> color {
          // …the tone map, byte-identical (D10: untouched)…
      }
      Icon {
          kind: root.icon;
          colorize: icon-color();
      }
  }
  ```

- Everything else stays: background/hover, the square min/max
  (space-6), the focus surface (`:63–84`), the tooltip arm
  (`:317–323`), the TouchArea click path, the a11y plumbing.
- Optical centring: the drawn blocks' stretch wrappers are gone; the
  fixed-size Icon must sit centred in the padded box. Stage 2
  verifies against the first regenerated golden; if the layout
  top-biases the glyph, the fix is an explicit centring wrapper —
  recorded, not silent.

### 2.4 The status-bar stop square — the one in-scope duplicate

`ui/compositions/status-bar.slint:310–328` hand-draws the stop
square inside the tracking chip's TouchArea, its own comment saying
so ("the IconKind.stop glyph's shape") — a live instance of exactly
the duplicate-drawing class F1 exists to kill. Migrated in this
chunk: the inner Rectangle pair becomes
`Icon { kind: IconKind.stop; colorize: Theme.text-primary; }` inside
the unchanged TouchArea/hit-target/accessible plumbing. Recorded
visual delta: filled 10 px square → outlined 12 px glyph, pinned by
the regenerated status-bar goldens.

### 2.5 Duplicate-glyph follow-ups recorded (NOT this chunk)

The grep for other hand-drawn icon geometry outside the button found
three more sites; each is recorded here as follow-up with the reason
it waits, so the class is fully accounted:

- `ui/widgets/plus-glyph.slint` — the board ghosts' plus. Deliberate
  derived 16 px size (r2 D5 contract, comment in file), custom
  hover choreography; adopting Icon would change a separately
  pinned geometry. Revisit when a chunk touches the ghosts.
- `ui/widgets/text-field.slint:200/257/294` — reveal-eye, validity
  check (`M 2 6 L 5 9 L 10 3` — byte-identical to IconKind.check's
  polyline), validity x. Internal widget-state glyphs; adopt Icon
  when chunk 7 reworks validity presentation.
- `ui/widgets/task-card.slint:145` — the done-card check (same
  polyline again). Composite checkbox; chunks 5/6 build new
  checkboxes off this pattern — the hand-off note is: adopt
  `Icon`/`IconKind.check` there.

## 3. The probe (R2 gate — lands FIRST, dispatch acceptance (a))

R2: `Image.colorize` + SVG is runtime behaviour; the migration is
gated on probe evidence, not documentation.

- **Composition** `tests/icon_probe.slint`, compiled in `build.rs`
  via the established probe pattern (the four existing probes —
  `compile_with_output_path` + `cargo:rerun-if-changed`; no debug
  info needed but follow the block shape).
- **Commit 1 content**: the probe + exactly one vendored asset
  (`check.svg`) — the "one asset, one IconButton" of acceptance (a):
  (i) one real `IconButton` (post-swap shape inlined in the probe
  composition, since the swap hasn't landed) tinted through a copy
  of the tone map; (ii) a tone ladder — the same asset in all 8
  `Tone`s on `Theme.background` plus the neutral hover-lift pair
  (`text-secondary` vs `text-primary`, the tone map's light/dark
  extremes) on both `Theme.background` and `Theme.surface-raised`;
  (iii) three sizes (0.5/1.0/3.0 × `icon-size`) — the crispness
  extremes.
- **Evidence that gates the migration** (`tests/icon_probe_tests.rs`,
  software renderer):
  1. renderer-presence assertion (the goldens' contract);
  2. **every cell contains glyph pixels** (non-uniform runs of the
     expected tint colour in the cell region — the `longest_run_in_strip`
     technique from `usability_components.rs`): a blank cell means
     decode or colorize failed — the migration does not proceed;
  3. a committed probe golden (`tests/goldens/icon-probe-1.0-*.png`)
     — byte-stable determinism proof + the review artefact.
- **Renderer #2** (the other renderer CI uses): the `mcp_smoke` CI
  lane boots the full app on the default backend stack
  (femtovg/GL + resvg — `.github/workflows/ci.yml:199–216`). Post-swap,
  a green `mcp_smoke` run is the GL-side decode+tint evidence; the
  stage-2 report records both results. Gate: probe green (software)
  **and** mcp_smoke green (GL) before/with the component-swap commit.
- **Commit 2 extends the probe** with the kind wall: one cell per
  §1 row (19 kinds), regenerating the probe golden — the vocabulary
  wall the review can eyeball in one image.

## 4. LICENSE + attribution (D9)

- `ui/assets/icons/LICENSE-lucide` (D9's exact path): Lucide's ISC
  license text **verbatim**, prepended with a provenance header —
  upstream URL, the pinned release tag, vendored date, the file list,
  and "files are unmodified upstream copies".
- ISC obligation: the copyright + permission notice must accompany
  copies of the work — the in-repo license file satisfies it for the
  source distribution; ISC imposes no binary-redistribution notice
  display, so no app-UI attribution (the AGPL license bundle carries
  the notices). The stage-3 review checks the license covers the
  vendored set (dispatch prompt 3.3 (d)).

## 5. The D19 amendment (D12) — both homes

1. **`ui/widgets/icon-button.slint` header** — replace
   "a geometric icon (D19 — no icon font; shapes composed from
   primitives, coloured Tone→Semantic)" with: "an icon (D19 as
   amended by round-4 §2 decision 1, 2026-11-01: vendored monochrome
   Lucide SVGs tinted via `Image.colorize`; the no-icon-font clause
   stands; the `IconKind` vocabulary gate remains the single closed
   set)". The same amendment lands in `icon.slint`'s header (§2.1).
2. **`docs/architecture.org`** — the `taskboard-ui-slint` module-map
   bullet (the §"Workspace crates" list, `docs/architecture.org:482–495`)
   gains: the =widgets= layer renders icons as vendored monochrome
   Lucide SVGs (ISC, =ui/assets/icons/=) tinted via =Image.colorize=
   behind the closed =IconKind= enum; no icon font (D19 as amended by
   GUI-review round 4 §2 decision 1).
   **Recorded discrepancy**: D12 says "docs/architecture.org (where
   D19 lives)", but D19's original text lives in
   `.artifacts/plans/2026-10-15-ui-views-board-plan.md` §D19 (a dated
   artefact — history, not amended) and architecture.org has **no**
   icon note today — so the architecture.org half of the amendment is
   an *addition*, exactly where the dispatch points. Both named homes
   end up consistent; the historical plan stays untouched.

## 6. Golden strategy

- **All 14 existing goldens regenerate** (board ×6, today ×6, kiosk
  ×2): every rendered icon changes silhouette; the PNG diff is the
  PR's review artefact (the established regen protocol:
  `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
  --test golden_visuals`, run fails afterwards by design). Expected
  changes, all traceable to §1's table: thinner strokes everywhere,
  hamburger 16 px bars → 12 px Lucide menu, play/stop filled →
  outlined, warn's bare-! → triangle-!. **Anything else visible in a
  PNG diff is a finding, not style.**
- **New: the notification-surface golden** (`tests/goldens/notification-3.0-1000.png`,
  one matrix cell — the kiosk precedent): fixture = one delivered
  task notification (can-done) + one delivered alarm (can-cancel) via
  the existing `fixture::deliver_task_notification`/`deliver_alarm`
  helpers; the row then renders dismiss(circle-x) ∥ done(check) ∥
  play ∥ snooze ∥ mute ∥ remove — **acceptance (d)'s distinctness
  evidence** ("Dismiss notification" and "Mark task done" render
  different glyphs). Scale 3.0 for glyph detail. The surface mounts
  in the main window when `notification-model.length > 0`
  (`app-window.slint:397`), so the ordinary mount path captures it.
- **No dedicated alarm-window golden — recorded**: `AlarmWindow` is
  a lazily mounted *second* top-level window (`lib.rs:578–587`) the
  golden driver doesn't snapshot; its three kinds (check/snooze/
  remove) are pinned by the probe kind wall + the notification
  golden carrying the same kinds. A window-capture seam is scope
  growth for zero extra icon coverage.
- **Status bar** rides the board/today/kiosk goldens (chips render
  inside those captures) plus the §2.4 stop-square swap.

## 7. Test catalogue (TDD order)

1. **Structural mapping-totality test** (the dispatch acceptance (c)
   structural test; `pin_guard.rs` is the house precedent for
   source-structure assertions — "a structural manifest assertion,
   not a text assertion on program output"): `include_str!` over
   `ui/widgets/icon.slint` + `icon-button.slint`; (i) the enum's row
   set ≼ the `IconKind.X` references in the `source` chain (totality:
   every kind mapped); (ii) every `../assets/icons/<file>` named in
   the chain exists on disk (no missing asset → no silent warn
   fallback); (iii) every SVG in `assets/icons/` is referenced (no
   dead vendored files). Lands with commit 3 (the component).
2. **Probe tests** (§3): pixel evidence per cell + committed probe
   golden. Commits 1–2.
3. **G3 successor** — `usability_components.rs`'s hamburger contract
   (`the_hamburger_bar_holds_its_token_derived_square_at_every_geometry`)
   pins the drawn 16 px bar runs and **will fail** at the swap: the
   drawn glyph it pins is gone. It is **rewritten, not weakened**
   (still a token-derived geometry pin, same geometry matrix): the
   toolbar strip's longest icon-colour run ≈ `Theme.icon-size` ±1 px
   across the same five geometries. Same commit as the swap.
4. **Notification golden** (§6) + the 14 regenerated goldens.
5. **Journeys**: zero edits expected — every call site keeps its
   `IconKind` API; the journeys suite green unchanged is itself the
   no-API-churn evidence.
6. **Acceptance greps** (stage-3 review, recorded here as the gate):
   `grep -rn "if (root.icon ==" crates/taskboard-ui-slint/ui/` →
   empty; runtime-loaded images:
   `grep -rn "source:" crates/taskboard-ui-slint/ui/ | grep -v @image-url`
   → only non-icon Image sources (card art — none expected in ui/).
7. **No text assertions** anywhere in the new tests (pixel +
   structural only); no quality ceilings (cell checks are "glyph
   present", never "≤ N pixels").

## 8. Tier-4 items (dispatch prompt 3.2)

- Icon crispness/legibility at scale 0.5 (6 px glyphs, 0.5 px
  strokes — the kiosk panel) and 3.0 (36 px, resvg rasterizes at
  device pixels — should be crisp).
- The neutral hover lift (text-secondary → text-primary) still reads
  on the Lucide stroke weight.
- The status-bar stop square at 10 px filled → 12 px outlined.
- Optical centring of the glyph in the button (§2.3).

## 9. Risks

- **R1 (=§7-R2)** colorize+SVG runtime behaviour — the probe-first
  commit order is the gate; fallbacks recorded in §2 decision 1
  (Tabler assets — same mechanism; the `lucide-slint` crate).
- **R2 compiler-shape risk**: conditional-over-`@image-url` and the
  enum re-export are verified against the pinned 1.18.1 sources
  (§2.1) but not compile-run; both carry recorded fallbacks
  (conditional-children Icon; 13-line mechanical import update).
- **R3 golden churn volume**: 15 regenerated/new PNGs + the probe
  golden; every diff must trace to §1's table — an untraceable pixel
  change is a regression finding.
- **R4 asset availability at the pinned upstream tag** (`unlink.svg`,
  `timer.svg`): verified at download; recorded same-semantics
  fallbacks (§1 rows 15/17).
- **R5 the enum move** touches the export surface of a file 13 call
  sites import — the re-export keeps them untouched; failure mode is
  a mechanical, compile-caught import update (§2.1).
- **R6 `warn`'s deviation** (triangle-!) is invisible in goldens
  today (no call site) — recorded in §1 so the first future consumer
  knows the glyph differs from the old drawn bare-!.

## 10. Stage-2 commit series (each standalone, per dispatch prompt 3.2)

1. `test(ui-slint): the icon colorize probe` — probe composition +
   build.rs registration + `check.svg` + probe tests + probe golden.
   Green before anything else moves.
2. `chore(ui-slint): vendor the lucide icon set` — 18 further SVGs +
   `LICENSE-lucide` + the probe kind wall + regenerated probe golden.
3. `feat(ui-slint): icons render vendored svgs via colorize` —
   `icon.slint` (enum + Icon + mapping), theme `icon-size`, the
   icon-button swap, G3 successor, structural totality test, all 14
   goldens regenerated.
4. `feat(ui-slint): distinct dismiss and done notification icons` —
   the six new enum rows + mapping arms, notification-surface
   dismiss→`dismiss`, the notification golden.
5. `docs(ui-slint): d19 amended to vendored svg icons` — status-bar
   stop square → Icon, both D19 amendment homes, CHANGELOG
   Unreleased, the measured binary-size delta into the stage-2
   report.

Full gate suite via `scripts/run-quality-gates.sh` before handoff;
no push, no PR (stage 3 ships).
