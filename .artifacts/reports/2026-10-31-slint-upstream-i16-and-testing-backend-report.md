# Slint upstream draft — the software renderer's i16 geometry space, the Today/board contrast, and the testing backend's empty-element queries (#462, rider #536)

- **Date**: written 2026-11-02 on the `fix/kiosk-board-i16-overflow`
  worktree; artefact of backlog round 2 chunk 3.1.
- **Kind**: *upstream draft — an in-repo artefact only. Posting
  upstream (slint issue(s)) is the user's decision and action; stage 3
  files nothing outside this repository.*
- **Pinned versions** (re-read from this tree's `Cargo.lock`, not
  assumed): `slint` / `i-slint-compiler` / `i-slint-renderer-software`
  **1.18.1**; `euclid` **0.22.14**. Every source citation below was
  re-opened against the local registry copy of the pinned 1.18.1
  sources at write time.
- **Evidence mandate**: every factual claim carries evidence — a
  source cite (file + what it states), verbatim recorded output, or a
  pointer into a recorded in-repo report. Method sections are
  reproducible as written.

## Finding 1 — the software renderer's entire geometry space is i16 physical pixels, and a consumer profile with `overflow-checks = true` turns out-of-space geometry into a process abort

**The source facts** (re-verified 2026-11-02 against
`~/.cargo/registry/src/*/i-slint-renderer-software-1.18.1/lib.rs`,
lines 63–67):

```rust
type PhysicalLength = euclid::Length<i16, PhysicalPx>;
type PhysicalRect = euclid::Rect<i16, PhysicalPx>;
type PhysicalSize = euclid::Size2D<i16, PhysicalPx>;
type PhysicalPoint = euclid::Point2D<i16, PhysicalPx>;
type PhysicalBorderRadius = BorderRadius<i16, PhysicalPx>;
```

Every position, size, and length the software renderer manipulates is
an `i16` count of physical pixels. The hard ceiling is ±32 767 px;
the type admits no larger scene coordinate.

**The amplification** is a *consumer* profile choice, not a slint
choice: this workspace ships `overflow-checks = true` in the release
profile (`Cargo.toml:187-192`, the #303 standing decision "arithmetic
overflow in the shipped binary must be a loud, backtraced panic,
never a silent wrap"). With it, the first `i16` add whose result
exceeds the type's range aborts the process; without it, the same
geometry would silently wrap (its own class of corruption). Slint
thus converts "content deeper than 32 767 px" from a rendering
question into, at best, a silent-wrong-render and, under
overflow-checked consumer profiles, a hard crash.

**The recorded crash** (verbatim from the 2026-10-26 view-pinned
re-measure, `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`
§5; first hand of a `draw-full/board/10000` criterion lane, dev box,
slint 1.18.1):

```
thread 'main' (1357698) panicked at .../euclid-0.22.14/src/length.rs:201:21:
attempt to add with overflow
  4: <euclid::length::Length<i16, i_slint_core::lengths::PhysicalPx> as core::ops::arith::Add>::add
  12: <i_slint_core::items::BasicBorderRectangle as i_slint_core::items::Item_vtable_mod::Item>::render
  32: <i_slint_core::model::repeater::Repeater<...InnerComponent_drop_zone_98>>::visit
```

The aborting frame is a plain `BasicBorderRectangle::render` — an
offscreen rectangle's geometry add overflowed while the renderer
walked an eager repeater's full instance list.

**The empirical bracket** (1366×768 window, software renderer,
four-column round-robin fixture, ~55 px card pitch; method: dev-profile
scratch runs of the bench harness at increasing N, recorded in the
same report §5): **N = 2 000 (500 rows/column) renders; N = 2 600
(650 rows/column) panics.** 650 × ~55 px ≈ 35.7k px of stacked column
content — the first geometry add past 32 767 aborts. The bracket's
tightest reproduction, N = 2 600, is now pinned in this repository's
CI (`a_board_beyond_the_i16_pixel_ceiling_renders`,
`crates/taskboard-ui-slint/tests/virtualisation.rs`).

**The ask for upstream**: where should the software renderer's
geometry space widen or saturate? Concretely: (a) should the
rasterization entry convert `i16` physical geometry from an
`i32`/`f32` stage so that off-viewport content clamps instead of
overflowing, and (b) is the class shared by the other renderers
(femtovg's geometry is `f32`-based and did not abort — the same
fixture renders on the winit/femtovg desktop backend). A documented
"scenes deeper than 32 767 physical px are unsupported on the
software renderer" would also serve, if widening is not acceptable.

## Finding 2 — the mechanism question: the same card widgets rendered ~183k px deep on the Today view without panicking, while the board's column scene aborted at ~35k px

**The contrast** (same run, same renderer, same 1.18.1):
`draw-full/today/10000` — a * Today* list whose eager content depth
was ~183 000 px at N = 10 000 — rendered in 19.945 ms (re-measure
report §2/§4, verbatim cell `19.945 ms`), while the board's ~35.7k-px
column aborted in `BasicBorderRectangle::render`. The board's
overflowing path rasterized through the *unclipped* column stack
(the drop-zone repeater's `visit` in the backtrace above); the Today
list sat inside the page-Flickable/section clip chain.

**The open question for upstream**: is the difference the clip
scissor's culling stage — items fully outside the scissor dropped
before their geometry math runs — or per-item-type arithmetic (the
board's border rectangles carrying offsets the Today rows do not)?
We did not find a consumer-visible contract that says either way.

**Reproducibility note**: in our tree both surfaces have been
virtualised since 2026-10-26 (#373), so the eager-shape contrast is
no longer reachable from our markup; upstream can reproduce it in
principle with two plain eager `for` loops, one inside a clip chain
and one bare, at content depths bracketing 32 767 px. Recorded
against 1.18.1.

## Finding 3 — the testing backend's element queries return empty when a second conditional control mounts in the chrome (#536)

**The reproduction** (recorded during DR-5/4, #489; issue #536): with
the tracking chip's `TouchArea` mounted in the status-bar chrome row —
a second conditional control in that row — the testing backend's
element-traversal query for `SettingsView` returns **empty**. The
settings journeys still pass with the chip mounted; the tooltip
machinery is unaffected in the real (winit) backend. Backend
limitation, not application behavior.

**The in-tree workaround** is documented at the test that needs it
(`crates/taskboard-ui-slint/tests/journeys.rs`, the
`an_info_tip_hover_arms_the_generalized_tooltip` doc comment,
verbatim):

> its chip control trips the testing backend's element traversal
> (a second conditional control in the status row makes the
> SettingsView query return empty — backend limitation, not
> behavior: the settings journeys pass with the chip mounted).
> The tooltip machinery is this test's subject; it runs on a
> chip-free chrome. Issue-worthy against the backend harness.

**The traversal-multiplicity observation** (the #373 ListView round):
the same traversal can yield one live node **once per multi-element
ancestor path** — a query for a single instance returned 11×/14×
duplicates whose counts varied with the ancestor tree shape; the
driver now dedupes instances by node geometry
(`crates/taskboard-ui-slint/tests/support/driver.rs:349-352`,
`dedupe_instances` — "duplicates of one instance read [identical
geometry]"). Both symptoms point at the same place.

**The ask for upstream** (`i-slint-backend-testing`): a stated
`ElementQuery` traversal contract for conditional (`if`) and
repeated ancestors — when is a mounted conditional element visible to
`find_by_element_id`-style queries, and should a node reachable
through several ancestor paths be yielded once or once per path?

## Annex — further 1.18.1 behavior observations from the ListView round

Short-pointer annex; each finding's full evidence chain (source cites
+ isolated probes) is in
`.artifacts/reports/2026-10-26-listview-virtualisation-stage2-report.md`
§2, produced on the same pinned 1.18.1:

- **Flickable reports no content-derived layout info** (the
  compiler's `layout_info` default; preferred 0) — a Flickable
  stretched into a layout claims zero preferred size regardless of
  its `for` content.
- **A model does not flow through a child component's `in property`
  into the child's `for`** — zero delegate instances,
  timing-independent, isolated in a minimal probe shape; the model
  must be set on the `for`'s own component scope.
- **The compiler conservatively detects a `height` ↔
  `content-height` binding loop** and statically rejects the markup
  even through function indirection; consumers decouple via
  change-handler assignment instead.

**Explicitly excluded**: the round's "empty delegate ids behind
components" claim — retracted with cause in the same report §9 (the
observation had targeted the wrong surface); it is not carried
upstream.

## The one decision-grade recommendation

For the slint project: **treat ≥2.5k-row eager content on the
software renderer as a crash class, not a scaling curve — convert the
i16 physical geometry to a wider arithmetic stage (i32 or f32) at the
rasterization entry, or clamp, so that a consumer profile with
`overflow-checks = true` cannot be aborted by scene depth alone.**
Consumer-side virtualisation (which this project shipped as its own
fix, #373) bounds *our* geometry, but the abort remains reachable for
any overflow-checked consumer with one deep eager list, and the
failure mode — a panic inside `euclid::Length<i16>::add` under
`BasicBorderRectangle::render` — gives the consumer no hook to defend.
