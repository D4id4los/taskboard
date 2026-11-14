# Task View Window Max-Width Collapse — Investigation Report

Date: 2026-11-14
Symptom reported by the user (desktop, Wayland/KWin, base scale 1.1):
opening the Task view shrinks the app window to ≈518 px and clamps its
maximum width there for as long as the view is open; the window cannot
be widened.

## Root cause

Slint propagates layout constraints from an item tree up to the OS
window: the winit backend applies the root component's horizontal
layout `max` as the window's maximum size, and resizes the window down
to satisfy it when the constraints change
(`adjust_window_size_to_satisfy_constraints` in
`i-slint-backend-winit`).

In `TaskView` (`crates/taskboard-ui-slint/ui/views/task-view.slint`),
one row of the root `VerticalLayout` carries a content-bounded max
width: the **add-repeat affordance** — a bare conditional `IconButton`
(a fixed `space-6` icon square) directly under the view root, mounted
for rule-less open subjects (`series_edit_visible`). Once the refresh
settles, the view's root horizontal layout `max` collapses to a small
content-derived value (measured 327.6 px at scale 1.1; invariant to the
row's own width and label). The window's computed max width then
becomes `max(collapsed_max, min-width 480·scale)` = the min-width
floor (528 px at scale 1.1 — the user's "small maximum width"), and
Slint resizes the window down to it. Other views are unaffected because
none of their rows bounds the merge.

Empirical confirmation (X11/XWayland probe instance over a copy of the
user's database): with the task view open, the window's
`WM_NORMAL_HINTS` gained `program specified maximum size: 528 by
16777215` and the window snapped to 528 px; severing the row's
constraint restored an unbounded max.

The collapse value was invariant to the row element (bare square vs.
`IconButton` vs. label text), i.e. it is a Slint-level constraint
artifact of a bounded-max row inside a stretched vertical layout, not
something the row's content controls. The house remedy (the R6-4
idiom, cf. `task-card.slint` / `stat-bars.slint`) applies unchanged:
state an explicit effectively-unbounded `max-width` to sever the merge.

## Fix

One line at the `TaskView` component root
(`ui/views/task-view.slint`): `max-width:
Theme.layout-severing-max-width;` — the view's root merge can never
bound the window's constraints, whatever rows future rounds add.

## Regression host

`tests/window_max_probe.slint` + `tests/window_max_probe_tests.rs`:
the real `TaskView` with a rule-less open subject on the testing
backend; the test asserts the probe root's horizontal layout max stays
effectively unbounded after `ensure_item_tree_instantiated`. Verified
TDD both ways: `h.max = 238` (fail) without the severing, pass with it.
The null-platform lib tests cannot host this regression — the collapse
only materializes through the event-loop materialization pass, which
the null platform never runs.

## Field verification

Debug probe instance (X11 backend, user's DB copy, scale 1.1):
- pre-fix, task view open: `program specified maximum size: 528 by 16777215`,
  window snapped to 528×704;
- post-fix, task view open: max size hint `99999 by 16777215`
  (effectively unbounded), root `h.max` unbounded.

Note: the buggy clamped geometry (581×1027) was persisted into the
user's `[ui.window_geometry]` by the forwarder while the bug was live;
after the fix lands, the window boots at that saved geometry but can be
resized freely again (the new geometry re-persists as usual).

## Out-of-scope findings

- The Slint-level behavior (a bounded-max row collapsing a stretched
  vertical layout's cross-axis max to a content-independent value)
  may be an upstream bug worth reporting; recorded as a `backlog:`
  issue.
- The keymap interception consumes `Return`/`Space` globally, so a
  tab-focused button cannot be activated by keyboard on the desktop
  preset (observed while driving the probe UI by synthetic keys).
