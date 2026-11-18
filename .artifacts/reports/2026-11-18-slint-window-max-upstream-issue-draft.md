# DRAFT — upstream Slint issue: bounded-max row in a stretched VerticalLayout collapses the root layout's cross-axis max to a content-independent constant

- **Status**: DRAFT, in-repo artefact only (#778). **Posting is the
  operator's call** — an upstream post is an external action.
- **Built from**: `.artifacts/reports/2026-11-14-task-view-window-max-width-collapse-report.md`
  (field evidence + regression host) and
  `crates/taskboard-ui-slint/tests/window_max_probe.slint` (probe).
- **Target**: Slint upstream (issue tracker), category *layout/windowing*.

---

## Title

A row with a bounded `max-width` inside a stretched root `VerticalLayout`
collapses the layout's cross-axis `LayoutInfo.max` to a small,
content-independent constant — which the winit backend then applies as
the OS window's maximum size.

## Environment

- Slint version: the 1.x tree this repo pinned as of 2026-11-14
  (fill exact version from `Cargo.lock` before posting).
- Backend: winit; observed on X11/XWayland and Wayland/KWin.
- Scale: reproduced at base scale 1.1.

## Minimal reproduction

A minimal reproducer should be extracted from the probe
(`crates/taskboard-ui-slint/tests/window_max_probe.slint`) before
posting — the probe currently mounts the real `TaskView`; the upstream
report wants a self-contained `.slint` with a bare `Rectangle` row
carrying explicit `min-width`/`max-width` inside a stretched root
`VerticalLayout`, plus a tiny Rust harness that prints the root's
horizontal `LayoutInfo` after the conditional materialization pass has
run.

Observed shape (from the field evidence):

1. One row of the root `VerticalLayout` carries a content-bounded max
   width (a fixed icon-square `IconButton`, but also a bare `Rectangle`
   with explicit `min/max-width`).
2. Once the conditional materialization pass has run
   (`ensure_item_tree_instantiated`), the root layout's horizontal
   `LayoutInfo.max` collapses to a small constant — measured **327.6 px
   at scale 1.1**, invariant to the row's own width, its label text, and
   the row's element type.
3. The winit backend applies that collapsed max via
   `adjust_window_size_to_satisfy_constraints` as the OS window's
   maximum size and resizes the window down to
   `max(collapsed_max, min-width)` — the app's window became
   un-widenable (X11 field check: `WM_NORMAL_HINTS` gained
   `program specified maximum size: 528 by 16777215`).

## Expected

A single row's bounded max-width should not clamp the whole root
layout's cross-axis max below the content of the other rows — and in
any case the window's max-size hint should stay content-derived, not
collapse to a constant independent of content.

## Workaround

Severing the constraint merge at the view component root with an
explicit effectively-unbounded `max-width`
(`max-width: <large constant>`) restores an unbounded root max and a
resizable window.

## Notes for the poster

- Attach the extracted minimal reproducer + harness output, not the
  in-repo probe (which imports the app's views).
- Re-verify against current upstream Slint before posting; if fixed
  upstream meanwhile, close #778 with that note instead.
