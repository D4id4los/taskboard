# [Plan] Empty-repeater audit — the #725 site sweep (round-6 follow-up)

- **Date**: 2026-11-12
- **Status**: STAGE-1 PLAN (verdict table + TDD-first fix spec; no code).
  Branch `fix/ui-slint-empty-repeater-audit`, worktree
  `../taskboard-WT-fix-ui-slint-empty-repeater-audit`, branched off
  `origin/main` @ `0441ee86`; every mount condition and parent chain below
  was re-verified at that SHA.
- **Origin (normative)**: issue #725 (the site list + the mechanism);
  `.artifacts/plans/2026-11-11-gui-review-round6-plan.md` §3 D-6.4 — the
  probe-verified mechanism and the landed `TaskCard` remedy, which is the
  prescribed fix form for every (c) verdict in this plan.
- **Non-goals**: no change to any (a)/(b) site (their mount conditions and
  window-level placements are load-bearing and stay); no ListView rebase
  bookkeeping work (#592 hazard, D-6.4 records it — the severing remedy
  never fires it); no visual redesign of the two (c) sites; no probe
  beyond the two (c) regressions + the catalogue's confirmation probes.

---

## STAGE-2 ADDENDUM (2026-11-13, probe-falsified corrections)

The TDD probes falsified parts of §0/§1; corrected by measurement
against i-slint-core 1.18.1 (`layout.rs` `box_layout_info` /
`LayoutInfo::merge`). The corrections below supersede the plan body
where they conflict; the same summary is posted on #725.

- **The empty early return is single-cell-only**: `count < 1` pins
  `max = min` only when the repeater is its layout's **only cell**. A
  bare repeater with layout siblings contributes to the box-layout max
  as a *sum of cell maxes*, so its emptiness cannot zero the layout.
- **Cross-axis chains do not propagate a zeroed max**: the viewport
  (Flickable) does not clamp to the content's max, so no on-screen
  freeze exists at either view. The freeze fires only where a layout
  distributes width along the **main axis**, manifesting as the
  min-override: the column clamps into `[min, max]` with `min > max`
  and freezes at its children's minimum floor.
- **Site 4 (`timers.slint:116`) is (b), not (c)**: the repeater rides
  `section-layout` together with the always-present header and creation
  form; their unbounded maxes make the section's max a sum, never zero
  (measured `max-width = unbounded` pre-fix). §2.1 and the
  `timers_rows_severing_probe` are dropped — no fix, no defect to pin.
- **Site 9 (`stat-bars.slint:48`) is (c) confirmed, mechanism
  corrected** (latent, not user-visible today): empty `bars` pins the
  strip's root max at 8px (2·padding) and `ReflectionSection`'s merge
  adopts it; fixed per §2.2/D-C (the explicit root cap). Probe
  `tests/stat_bars_severing_probe*` reproduces the freeze (column
  frozen at its 96px caption floor at 250px and 500px row widths
  pre-fix, tracking post-fix; zero delta populated).

Tally after corrections: 5×(a), 7×(b), 1×(c), 2×(not empty-capable).

---

## 0. The mechanism, restated (from D-6.4, probe-verified; partly
## superseded by the stage-2 addendum above)

An **empty** `for` repeater directly inside a stretch-aligned
`HorizontalLayout`/`VerticalLayout` reports
`LayoutInfo { min: 0, preferred: 0, max: 0 }` — i-slint-core
`box_layout_info`'s `count < 1` early return pins `max = min`. Every
`LayoutInfo::merge` up the layout chain takes `max = min(child maxes)`,
so one empty repeater clamps every ancestor layout's max-width to 0 and
the ancestor freezes at its preferred width, starving every
elided/flexible child inside it. Two compounding compiler behaviours to
respect in any fix:

- **Single-cell lowering**: a plain element wrapping exactly one layout
  inherits that layout's constraints per axis — a naive wrapper is
  defeated by it (the round-6 first wrapper attempt was).
- **Conditional `if` mount**: an `if`-element mounts/unmounts whole, so
  an `if`-mounted ancestor whose condition is content-derived
  (`length > 0`) can never present an empty layout upward — that is the
  (a) verdict's foundation.

The **landed remedy** (`ui/widgets/task-card.slint:234-244`, R6-4): the
empty-capable row rides a plain `Rectangle` that states
`max-width: Theme.layout-severing-max-width` (the theme meta-constant,
`ui/theme.slint:80`, 99999px) explicitly, defeating the single-cell
lowering's inheritance, and pins `height: <inner-layout>.preferred-height`
because the Rectangle no longer inherits the height constraint either.
Zero layout delta otherwise. **This exact shape is the fix form for both
(c) sites below.**

## 1. The verdict table

All 14 line sites from #725, with the concrete parent chain each verdict
rests on. Mount conditions cite the line that decides them.

| # | Site | Verdict | Evidence (parent chain) |
|---|---|---|---|
| 1 | `status-bar.slint:130` (`for row in root.banner`) | **(a)** | The repeater sits in a `VerticalLayout` inside `if (root.banner.length > 0): Rectangle` (status-bar.slint:~155) — the overdue-banner slot mounts only with content. `StatusBar` itself is always mounted but never presents the empty layout. |
| 2 | `notification-surface.slint:52` (`for row in root.notifications`) | **(a)** | The whole surface mounts only with content: `if (root.notification-model.length > 0): NotificationSurface` (`app-window.slint:552`, with the "no silent chrome in an empty state" comment). |
| 3 | `habits.slint:78` (`for row in root.habits`) | **(a)** | `HabitsSection` mounts only with content: `if (root.habits.length > 0): HabitsSection` (`views/today-view.slint:172`). The `editing-visible` conditional inside the row is row-scoped and never presents an empty layout (the row exists ⇒ habits non-empty). |
| 4 | `timers.slint:116` (`for row in root.timers`) | **(c)** | `TimersSection` mounts by **user toggle, not content**: `if (root.timers-visible): TimersSection` (`views/today-view.slint:154`) — a kiosk/desktop preference flag (`today-view.slint:77-79`). With the section visible and `timers` empty, the repeater reports {0,0,0}; `TimersSection` inherits `Rectangle` with a single `section-layout := VerticalLayout` child (single-cell lowering inherits the zeroed constraints); today's page column `content := VerticalLayout` merges `max = min(child maxes)` → 0 → through the `Flickable` the whole today view freezes at preferred width. The exact TaskCard defect at view scope. Fix §2.1. |
| 5 | `track-entries.slint:293` (`for row in root.rows`) | **(b)** | `TrackEntriesPanel` is a window-level overlay (`app-window.slint:740`, mounted `if track-entries-open`) with explicit placement: `width: min(root.width - 2·space-2, max(self.preferred-width, 360px·base-scale))`, absolute `x`/`y` (app-window.slint:741-752). No layout ancestor exists to merge into; the 360px floor keeps the empty-model width sane. The rows are a plain column inside the panel, so an empty pass only shrinks the panel's content-driven height — by design. |
| 6 | `focus-overlay.slint:135` (`for row in root.rows`) | **(b)** | Window-level overlay: `if (root.focus-overlay-open): FocusOverlay` with explicit `width: min(root.width - 2·space-2, 560px·base-scale)` and absolute `x`/`y` (app-window.slint:~1193-1200). No layout ancestor. Zero switcher rows shrink the content-driven height but the stop row and note field remain — acceptable. |
| 7 | `action-menu.slint:145` (`for entry in root.page`) | **(b)** | Window-level overlay with explicit geometry at both mount points (`app-window.slint:1010` keyboard menu, `app-window.slint:1060` pointer menu): explicit `width`, `x`, `y`, `available-height`. No layout ancestor; an empty page is a paging-boundary state the menu already sizes for. |
| 8 | `search-palette.slint:59` (`for row in root.rows`) | **(b)** | Window-level overlay: `if (root.palette-open)` with explicit `width: min(root.width - 2·space-2, 560px·base-scale)` (app-window.slint:~1134) and the palette is content-driven in height (`search-palette.slint:30`, `height: content.preferred-height` — round-3 D24). Zero rows render the query field alone; the overflow caption self-hides (`visible: root.overflow > 0`). |
| 9 | `widgets/stat-bars.slint:48` (`for bar in root.bars`) | **(c)** | **Both** review-view call sites mount `StatBars` unconditionally (`views/review-view.slint:211` work section, `:259` reflection section) — deliberately: D-9 tier 2 pins "an empty window renders dashes and baseline ticks, **never a vanished section**" (`review-view.slint` ReflectionSection comment). But empty `bars` zeroes the strip's `HorizontalLayout`; `StatBars` inherits `Rectangle` with that layout as its single child; `ReflectionSection` **inherits `VerticalLayout` directly** (a layout as component root — no Rectangle in between), so the zero merges straight into the review column and freezes the whole review view at preferred width. The collapse defeats the D-9 skeleton promise. Fix §2.2. |
| 10 | `widgets/dropdown-flyout.slint:72` (`for row in root.rows`) | **(b)** | The rows live inside `list := ListView` (`dropdown-flyout.slint:~46-53`) — a ListView sizes its rows to the viewport width itself; the repeater is not a direct child of a stretch-aligned box layout in any merge chain. The panel is window-level with `width: DropdownFacts.anchor-width` at all three sites (`app-window.slint:827/852/877`). An empty model yields a zero-height panel, reachable only for the board switcher dropdown with zero segments (never-booted install) — cosmetic, recorded as a backlog issue, out of this mechanism's scope. |
| 11 | `compositions/date-picker.slint:187` (`for weekday in ["M",…,"S"]`) | **(not empty-capable)** | The weekday repeater iterates a 7-element literal array — it can never be empty; the constant-array row always reports the texts' merged info. |
| 12 | `compositions/date-picker.slint:~205` (`for row in root.grid`) | **(not empty-capable)** | The calendar grid is 4–6 rows by construction (the month model always emits whole weeks); `root.grid` is never empty while the picker is mounted, and the picker is a window-level overlay with explicit geometry (`app-window.slint:637-648`) besides. |
| 13 | `board-bar.slint:121` (`for segment in root.bar.segments`) | **(a)** | The strip mounts only with content: `if (!root.dropdown-mode && root.bar.segments.length > 0): HorizontalLayout` (`board-bar.slint:~152`, with the "zero-segment model renders no row" comment). The dropdown-mode path replaces the strip with the field. |
| 14 | `board-layout.slint:297` (`for column in root.stacks`) | **(a)** | The whole desktop leg mounts only with content: `if (root.stacks.length > 0 && !root.compact-active): VerticalLayout` (`board-layout.slint:~323`, carrying the D1-b width-comment). |
| 15 | `board-layout.slint:750` (`for column in root.stacks`) | **(a)** | Same for the compact leg: `if (root.stacks.length > 0 && root.compact-active): VerticalLayout` (`board-layout.slint:~776`). |

**Tally**: 5×(a), 6×(b), 2×(c), 2×(not empty-capable — the #725 list
counts date-picker's two repeaters as one site line).

## 2. The (c) fixes

Both apply the landed TaskCard remedy verbatim — the
`Theme.layout-severing-max-width` carrier (normative per D-6.4: the
severing fix is the probe-verified, zero-delta form; "mount only with
content" was tried and reverted in round 6). Both sites are plain
columns (no ListView anchor bookkeeping), so the #592 rebase hazard
that motivated rejecting mount-conditionals at TaskCard cannot fire
here either way — the severing form is chosen for uniformity and
zero-delta guarantees, not out of necessity.

### 2.1 — `timers.slint`: the timers rows ride a severing carrier

Current shape (`compositions/timers.slint:~143`):

```slint
for row[row-index] in root.timers: HorizontalLayout { … }
```

directly inside `section-layout := VerticalLayout`. Fix: carry the
repeater in a plain `Rectangle` with the explicit cap, keeping the
section's spacing rhythm — the carrier is one layout child where the
repeater was, so `spacing: Theme.space-1` semantics are preserved:

```slint
// The rows ride a plain Rectangle (the TaskCard remedy, round-6
// R6-4 / #725): with the section visible and `timers` empty, the
// bare repeater would report max = min = 0 and the page column's
// merge would clamp today's max-width to it — the whole view
// frozen at preferred width. The explicit cap replaces the
// single-cell lowering's inheritance; the height rides the inner
// layout's preferred.
Rectangle {
    max-width: Theme.layout-severing-max-width;
    height: timers-rows.preferred-height;
    timers-rows := VerticalLayout {
        spacing: Theme.space-1;
        for row[row-index] in root.timers: HorizontalLayout { … }
    }
}
```

Notes: the per-row `HorizontalLayout`s move inside a carrier
`VerticalLayout` that owns the old section-level row spacing; the
section header keeps its slot untouched. Verify against the actual
file: if the row spacing is already carried by a `VerticalLayout`
rather than section-level `spacing`, drop the carrier layout and keep
only the Rectangle cap (the plan's intent is the cap, not a
restructure). Read-seam: if the probe needs one, expose
`rows-width` / `rows-preferred-width` on `TimersSection` in the
`task_card_title_probe` pattern (`title-width` precedent).

### 2.2 — `stat-bars.slint`: the bars strip rides a severing carrier

Current shape (`widgets/stat-bars.slint:50-…`): the `for bar in
root.bars` sits directly in the root-level `HorizontalLayout` (padding
+ spacing + repeater). The merge point is one level up: the component
root inherits `Rectangle` whose single child is that layout, so the
empty zero propagates through the root into
`ReflectionSection inherits VerticalLayout`. Fix at the strip's root —
state the cap on the component root itself, replacing the inherited
constraint:

```slint
export component StatBars inherits Rectangle {
    …
    // The explicit cap (#725, the TaskCard remedy): with `bars`
    // empty the strip layout reports max = min = 0; the single-cell
    // lowering would hand that to this root and the reflection
    // section's column merge would clamp the review view's max-width
    // to it. Stating the max severs the merge; the D-9 skeleton
    // promise (values collapse, the section never does) holds.
    max-width: Theme.layout-severing-max-width;
    HorizontalLayout { padding: …; for bar in root.bars: Rectangle { … } }
}
```

One line of .slint, no geometry change when `bars` is non-empty (the
stated cap exceeds any real strip width; consumers give the strip its
width through their column stretch as before).

## 3. Test catalogue (probes)

House probe pattern: `crates/taskboard-ui-slint/tests/<name>_probe.slint`
+ `<name>_probe_tests.rs` (the `task_card_title_probe` shape: mount the
real component in a stretch column with a flexible witness sibling,
drive controlled widths headless, assert **structural relations over
measured widths** — no font-metric pins, no text assertions, AGENTS §5).

| Probe | Pins | Kind |
|---|---|---|
| `timers_rows_severing_probe` | TimersSection visible with `timers = []` beside a stretch witness in a fixed-width column: the witness fills the column (its width tracks the column width at two controlled widths ≥2 deltas apart); then with one timer row the witness width is unchanged (zero-delta guarantee for the non-empty pass). | Regression probe for §2.1 — must fail pre-fix (witness frozen at preferred), pass post-fix. |
| `stat_bars_severing_probe` | ReflectionSection-shaped column (or ReviewView itself if the harness already mounts views — it does not; use the section shape) with `bars = []`: the stretch witness tracks the column width at two controlled widths; with one bar the witness is unchanged. | Regression probe for §2.2 — same fail/pass discipline. |
| empty-timers **pre-fix confirmation** (one-off, during TDD red) | Measure the today-column witness width at two window widths with `timers-visible = true, timers = []` pre-fix; record the frozen values in the probe test comment as the defect signature (the R6-4 108px-at-three-widths precedent). | Red-state evidence, not committed as a pin. |
| `dropdown-flyout` zero-row reachability | **Accepted-as-(b) probe — not required.** The (b) verdict rests on the ListView row-sizing + explicit `anchor-width` chain, read at `0441ee86`; no probe. The zero-row cosmetic case is filed as a backlog issue instead (§5). | — |

`needs probe` sites: none. The verdict table's (a)/(b) entries each cite
a decisive mount condition or placement line read at `0441ee86`; the two
(c) sites' collapse is the D-6.4 mechanism operating through verified
single-child/Single-cell chains, and their probes double as the
confirmation. The plan flags no remaining uncertainty that a probe must
settle before implementation.

## 4. Decisions (every open question settled)

- **D-A — remedy form**: the `Theme.layout-severing-max-width` carrier
  (landed TaskCard form), not content-conditional mounting. Reasons: it
  is the probe-verified landed form (D-6.4 normative); zero layout delta
  for the non-empty pass; no phantom-slot/#592 interaction. Decision
  applies to both (c) sites.
- **D-B — timers spacing home**: the row spacing moves into the carrier
  `VerticalLayout` (or stays wherever the file actually carries it —
  §2.1's note binds the implementer to read the file first and preserve
  the spacing semantics exactly; the probe's zero-delta pin enforces
  this mechanically).
- **D-C — StatBars cap placement**: on the component root (one line),
  not a wrapper inside — the merge point is the root's single-cell
  lowering, so the cap must replace the inherited constraint exactly
  there. A wrapper inside the root would leave the root still inheriting
  the zeroed layout info (single-cell lowering of `Rectangle { Rect {}
  }` chains), defeating the fix.
- **D-D — scope of the sweep**: only the two (c) sites get code; the
  (a)/(b) verdicts are recorded here and in #725 as the audit result;
  no defensive carriers are sprinkled on (b) sites — the window-level
  explicit placements are load-bearing design, and adding caps there
  would be noise.
- **D-E — date-picker**: "not empty-capable" is a verdict, not a
  TODO; the constant-array and month-grid invariants are the evidence.
  No guard is added for impossible emptiness.
- **D-F — dropdown-flyout zero-row panel**: cosmetic, unreachable in
  any flow that matters today (zero segments only pre-boot); filed as
  backlog (§5), not fixed here.

## 5. Execution notes

- TDD order: write `timers_rows_severing_probe` first (red against the
  unfixed tree — capture the frozen-width signature), apply §2.1, green;
  then the same for §2.2. The probes join `cargo nextest run
  --workspace`; no new dev-dependencies (the harness exists).
- Quality gates via `scripts/run-quality-gates.sh` (AGENTS §10); hand
  off at PR-open + `gh pr checks` green; remove the worktree (AGENTS §8
  lifecycle).
- Backlog issues opened at planning time (see the final report):
  dropdown-flyout zero-row panel (D-F). Comment posted on #725 with the
  verdict table summary + this plan's permalink.
- Estimated diff: **S** — ~12 lines across
  `ui/compositions/timers.slint` + `ui/widgets/stat-bars.slint` (plus
  read-seam properties if the probes need them), ~2×60 lines of probe
  .slint + ~2×80 lines of probe tests.
