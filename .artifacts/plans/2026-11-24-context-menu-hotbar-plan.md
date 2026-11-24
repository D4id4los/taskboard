# Chunk A2 — Context-Menu Family: Hot-Bar occlusion + habit menu hints

Round 6, chunk A2 · branch `fix/context-menu-hotbar` · stage-1 plan
(2026-11-24). Covers **#1309** (p1) and **#1311** (p2). Normative
sources: scope report §A2, dispatch plan §3.

## 1. Verified mechanisms (stage-1 reading, both confirmed in source)

### #1309 — the Hot-Bar is painted over by the Flickable (z-order/geometry)

Confirmed exactly as the issue's finding states. In
`crates/taskboard-ui-slint/ui/compositions/action-menu.slint`:

- `quick-bar-row := HorizontalLayout` (lines 143–159) is declared as a
  **conditional element** (`if (root.quick-bar.length > 0):`) with no
  `x`/`y`/`height` — it sits at (0, 0) with its preferred height.
- `flick := Flickable` (lines 161–263) likewise has no geometry and
  defaults to filling the whole `Rectangle` panel — **including the
  bar's band**.
- Both are plain siblings of the `Rectangle` (not in a layout); the
  Flickable is the **later** sibling, so it paints on top of the bar.
  The first menu row (cursor row, `selected: 0` at open) paints an
  opaque `Theme.surface` background at the panel top — exactly the
  bar's band — obliterating it visually. Nothing clips or hits-tests
  differently: the bar is rendered **under** the row list, which is
  the reported bug (presence without hit-testability).
- The trailing overlays (`#1188` border redraw, the scroll indicator)
  are intentional absolute siblings and are unaffected.

Why the tests missed it: `tests/probes/action_menu_bar_probe_tests.rs`
asserts structure/existence (`quick-bar-row` mounts, buttons own
pixels, populated panel taller than barless) but never a y-band
disjointness or a hit under a scrolled row list — no occlusion pin.

### #1311 — habit menu hints read the wrong context chain

Confirmed. In `crates/taskboard-ui-slint/src/adapter.rs`,
`habit_menu_entries` (~line 7841) builds its chain as
`[ContextId::Board, ContextId::Global]` — the same fixed pair the task
context menus use (`adapter.rs` ~14028/~14073). But the habit verb
bindings (`HABIT_TODAY_ROWS`, `keymap/presets.rs` lines 129–195:
`HabitMoveUp/Down`, `HabitUndoLog`, `HabitSkip`, `HabitPause`,
`HabitResume`, `HabitQuotaEdit`, `HabitHistory`, `HabitRepair`,
`HabitLog`, `HabitEdit`, plus `HabitConvertArm`/`HabitCreate`) are all
registered under **`ContextId::Today`**. `Keymap::first_binding_display`
(`keymap/mod.rs:147`) walks only the given chain (`visible_chain`),
so every habit menu row projects an empty hint. Task menus show hints
because their bindings genuinely live in Board/Global.

The submenu path compounds it: `habit_submenu_page` (~7911) and
`reproject_habit_menu` route through `self.menu_chain`, so the fix
must land at the chain the habit **opener** installs, not only in
`habit_menu_entries`'s local chain (otherwise the Convert ▸ children
and the armed-confirm re-projection still hint empty).

## 2. Decisions (all open questions resolved)

**D1 — z-order fix: wrap the bar and the Flickable in a
`VerticalLayout` (the issue's first-listed option).**
A single `VerticalLayout` (`width: 100%; height: 100%`) becomes the
panel's first child: row one is the quick bar (moved inside the
layout, keeping the `if` conditional — a conditional child of a layout
contributes zero height when absent), row two is `flick`. The layout
gives both a real vertical contract and the Flickable can no longer
overlap the bar at any geometry; no hand-maintained y/height
arithmetic to drift. The scroll indicator and the `#1188` border
overlay stay as absolute siblings **after** the layout, so the
topmost-chrome invariant (the overlay draws above the clipped scroll
edge) is preserved unchanged.
*Rejected*: explicit `flick.y = quick-bar-row.height` bindings —
Slint does not allow referencing an element declared inside an `if`
conditional from a sibling scope, so this spelling forces either
de-conditionalizing the bar (a visible-false element still occupying
geometry — its own drift risk) or duplicating the bar's height into a
manual property. The layout removes the whole class.
*Fallback recorded*: if the panel's `height: min(preferred-height,
available-height)` law no longer derives the bar's contribution once
the children sit in a layout (stage-2 verifies with the existing
`us1_s1` taller-than-barless assertion), bind the panel's
`preferred-height` explicitly to `layout.min-height` — the height law
itself does not change.

**D2 — scroll behavior is preserved by construction, and the bar
exits the scroll.** The bar lives **outside** the Flickable (it is a
layout sibling above it), so it never scrolls away; the Flickable's
viewport is strictly the row list. The selection scroll-into-view
arithmetic (`changed watch`, lines 252–260) reads `flick.height`,
which now excludes the bar — the expressions are self-consistent
under the new geometry, no change needed there.

**D3 — hint chain: the habit opener installs
`[ContextId::Today, ContextId::Board, ContextId::Global]`.**
Today first: the habit context menu opens exclusively over habit rows
on the Today view (habit rows exist nowhere else; #1301's all-boards
aggregation keeps them on Today), so Today is the menu's *active*
context and its bindings live exactly there — the hint is truthful,
not merely non-empty. Board and Global follow for any verb bound at
those levels. The chain is installed once at the habit opener (the
same site that sets `self.menu_chain` for task menus,
`adapter.rs` ~14028 pattern) so `habit_menu_entries`'s local chain,
`habit_submenu_page`, and `reproject_habit_menu` all inherit the
correct context through `self.menu_chain` — one derivation, no
per-row patching (the O-6 no-duplication law; the A2.3 trap "a
per-consumer overlay patch re-creates the duplication" is structurally
excluded). The `habit_menu_entries` local `[Board, Global]` chain is
replaced by `self.menu_chain` (or the same Today-prefixed literal) so
no stale-context residue survives.
*Rejected*: re-scoping `HABIT_TODAY_ROWS` to `ContextId::Board` —
would change the Today keyboard walk's context semantics and break
the presets' recorded grammar (M1) to fix a display bug; the walk
must not regress.

**D4 — structural pins (round law (a)):**
1. **Bar presence + disjointness + hit-testability, scrolled.**
   Extend `action_menu_bar_probe_tests.rs`: with a populated bar AND
   a row list tall enough to scroll (force `flick.content-y < 0`),
   assert (i) the bar row's y-band is disjoint from the first menu
   row's y-band (geometry, not existence — pins the occlusion), and
   (ii) a click on a bar button while the list is scrolled still
   routes `invoke-quick-bar` by index (hit-testability — rendered-under
   is exactly what this pin kills). The pre-fix suite must fail pin
   (i) and, on a scrolled list, pin (ii) (TDD red).
2. **Hints per habit row.** In the menu projection tier
   (`menu.rs` habit-context tests): for every `HABIT_CONTEXT` entry
   that `HABIT_TODAY_ROWS` binds, `habit_context_page` over the
   Today-prefixed chain carries that binding's chord in the row's
   `hint`; rows the preset deliberately leaves unbound carry `[]` by
   decision (the parity note in #1311's acceptance — the audit of
   *which* verbs *should* bind stays with #1280, not this chunk).
   Plus one unit pin: `first_binding_display` over the new chain
   resolves a Today-scoped action (red pre-fix: empty).

**D5 — M1 grammar unregression (round law (c)).** The existing
chevron probe (`us3`-family in the same probe file) and the
bar-mount/route tests must stay green unchanged; the barless
consumers' bit-identical invariant (empty model ⇒ no bar, and now
also: the layout contributes zero extra height when the bar is
absent) is re-asserted by the existing taller-than-barless test plus
a barless preferred-height pin if D1's fallback is taken. The fly-out
instance (`is-flyout`, no quick bar) mounts the same layout with an
empty bar row — covered by the existing zero-mount count assertion.

**D6 — out of scope, deferred to backlog:** the habit **row card's**
tooltip `hint-keys: []` (`ui/compositions/habits.slint:346`, #1311's
second acceptance bullet). It is the keymap projection feeding
`TaskCard`, a different seam (`habits.slint` + the card model), and
the dispatch plan confines this chunk's diff to `action-menu.slint` +
the hint derivation + tests. Filed as a backlog issue (see §5).

## 3. Implementation sketch (stage-2, TDD)

1. Write the D4 pins first; run them red (bar disjointness fails,
   habit hints empty).
2. `action-menu.slint`: introduce the `VerticalLayout` host (D1);
   move `quick-bar-row` and `flick := Flickable` inside; leave the
   overlays absolute. No callback or model changes — the seams
   (`invoke-quick-bar`, `QuickBarEntryModel`) are untouched.
3. `adapter.rs`: the habit opener installs
   `[ContextId::Today, ContextId::Board, ContextId::Global]` into
   `self.menu_chain`; `habit_menu_entries` reads `self.menu_chain`
   instead of its local literal. No keymap/preset changes.
4. Green the pins; keep the whole existing menu suite green (M1
   grammar, task menus' hints unaffected — their chain is untouched).
5. New seams carry `// slint: <callback>` (none expected); diff
   confined per dispatch plan §3.

## 4. Risks

- **Preferred-height derivation under the layout host** (D1
  fallback): if the panel stops deriving the bar's height
  contribution, the height law needs the explicit bind — small, but
  it is the one place the fix could grow past `action-menu.slint`
  geometry.
- **Today-in-chain side effects on habit rows that *do* have Board/
  Global bindings**: precedence is Today-first; if any habit verb is
  bound in two contexts the displayed chord could change. Audit at
  implementation time: today no habit verb is bound outside Today in
  the presets, so no visible change beyond empty→chord. Pin this
  expectation in the D4-2 test (no verb shows a Board/Global chord).
- **`visible_chain` masking**: if `ContextId::Today` is ever masked
  in the session, the chain truncates before Board/Global — correct
  semantics (a mask means lower contexts are hidden), but worth one
  assertion in the unit pin.
- **A11y tree re-parenting**: moving the bar into a layout changes
  the probe's element addresses only if ids change — keep the
  `quick-bar-row` / `quick-bar-button` ids verbatim so the existing
  probe and the journeys' registry census stay stable.

## 5. Backlog issues

- Filed at stage-1: see final report for numbers (the habit row
  card's `hint-keys: []` tooltip projection — D6).

## 6. Estimated diff size

S/M: `action-menu.slint` ~30 lines restructure, `adapter.rs` ~6
lines (chain), `menu.rs`/probe tests ~120 lines of pins. No domain,
storage, sync, or app-crate changes.
