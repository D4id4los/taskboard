# Date Picker Overlay Clips the Text Field — Chunk Plan (A4, round 5)

- **Date**: 2026-11-22
- **Branch**: `fix/date-picker-clip`
- **Issue**: #1206 (p2, bug, ui) + the systemic stacking-hazard finding #1254
- **Chunk**: A4 of `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md` §4
- **Scope**: task edit view (task-detail) due/start date pickers; diffs confined to the overlay mount (app-window M1/M2 mounts, `DatePickerFacts`, the probe). The base host (`anchored-overlay.slint`) is expected untouched.
- **UX journey**: no new verb/badge/view/setting — this restores the existing open-picker interaction's intended geometry; no journey plan required (the §7 activation gate covers *new* user-visible capability).

## 1. Diagnosis (verified on the worktree, not the issue's framing)

The picker does not clip the field via a z-order accident or an anchor offset
bug. It clips it by **geometrically correct placement onto the wrong anchor**:

1. Both overlays in the task-detail flow mount as sequential full-window
   `AnchoredOverlayHost`s in `app-window.slint`:
   - **M1** (editor overlay, ~:944): `DueEditor` in the host panel.
   - **M2** (picker overlay, ~:987): `DatePicker` in the host panel, mounted
     *after* M1 (later sibling ⇒ hit-tested above).
2. Both hosts read the **same** anchor facts global `DatePickerFacts`
   (`menu-facts.slint:28`), whose contents are the **closed value cell's**
   realized window-space rect (`due-column`/`scheduled-column` in
   `task-view.slint`, pushed by `push-due-anchor`/`push-scheduled-anchor` and
   kept alive by the view's `AnchorMirror`). Nothing re-anchors when the
   picker opens: the calendar button inside the M1 panel toggles
   `task-date-picker-toggled` and the adapter only flips `date-picker-open`;
   the anchor stays the closed cell.
3. Both hosts use the default `OverlayPlacement.auto`. The host's laws
   (`anchored-overlay.slint` §1–3) resolve **identical panel rects**: y =
   `anchor-y + anchor-height + gap` (below the closed cell), x = clamped
   `anchor-x`. The M1 editor panel and the M2 picker panel therefore occupy
   overlapping rects — the picker panel's top edge sits exactly on the editor
   panel's top edge, which is the text field row (`due-editor.slint:78–98`).
4. Result: the editor panel is *under* the picker host in both geometry and
   hit test. The field's band under the 280px picker panel is occluded (the
   right-hand remainder "peeks out from behind the month grid" — the
   screenshot's sliver); clicks in the occluded band land on the picker
   panel's absorbing dead-space TouchArea (`anchored-overlay.slint:221`),
   and clicks on the visible sliver land on the picker host's full-window
   dismissal TouchArea (`:194`) ⇒ dismiss instead of reaching the field.
   That is exactly symptom 2 (the clear "x" does nothing): the staged-clear
   path itself (`text-field.slint:54–71`, `due-editor.slint:38–40`) is
   **not** separately broken — it is unreachable by click.

**Why the existing pins missed it**: `om_us2_s1_*`
(`tests/probes/overlay_migration_probe_tests.rs`) assert the picker panel
never intersects its **anchor** (the closed cell) — a law the host honors
(G-2). No pin relates the picker panel to the **editor overlay's realized
panel**, which is the rect the user needs visible.

## 2. The fix (recorded decision)

**D1 — re-anchor the picker to the editor overlay's realized panel.** When
the picker opens it must anchor to the *editor host panel's realized
window-space rect*, not the closed cell. With the host's G-2 law (the panel
never intersects its anchor) this makes "the field fully visible with the
grid open" hold **by construction**, for the whole editor panel (field,
echo, clear affordance) rather than the field alone.

Concretely:

- Extend `DatePickerFacts` (or add a sibling global — D2) with a second
  anchor rect, `picker-anchor-*`.
- Publish the editor panel's realized rect from the **M1 mount site**: the
  host already exposes `panel-x/panel-y/panel-width/panel-height` out
  seams; an `AnchorMirror` placed at the M1 mount binds its sources to
  those seams and writes `picker-anchor-*`. It is live exactly while the
  editor overlay is mounted — which is exactly when the picker can open —
  so no fighting-mirror problem and no staleness window.
- The M2 mount reads `picker-anchor-*`; the M1 mount keeps reading the
  closed-cell anchor. The view's `push-*`/AnchorMirror keep-alives stay
  untouched (they own the editor's own anchor).
- Placement stays `OverlayPlacement.auto`: below the editor panel's bottom;
  flip above the editor panel when the below rect would cross
  `viewport-floor` (the flip puts the picker over the closed-cell region,
  never over the editor panel — the field stays visible in both arms).

**D2 — one global, four added properties** (not a new global): the picker
anchor is the same family (the date-picker flow), the global's comment
already documents the mirrored-anchor seam, and a second global would
duplicate the staleness lore. The added properties carry their own comment
naming the editor-panel source and the "stale while closed" law.

**D3 — the dismissal ordering stays as-is.** The M2-after-M1 sibling order
is correct for light-dismiss (an outside click dismisses the picker first).
With non-overlapping panel rects the underlay only receives clicks that are
genuinely outside both panels. The generalized cross-host stacking hazard
is filed as **#1254** (out of scope here).

**D4 — symptom 2 needs no separate fix.** The staged-clear path is
reachable again once the geometry is fixed. The plan adds a probe-level
typed pin (below) that the clear affordance's click routes `edited("")`;
if stage 2 finds the staged path itself broken at reachability-independent
level, that is a new `backlog:` issue, not a silent scope extension.

## 3. Structural pins (typed, no text assertions)

Extend `overlay_migration_probe.slint` (it already exposes
`picker-panel-*` and `picker-flipped`; the same seams exist on the editor
host) and its driver `tests/probes/overlay_migration_probe_tests.rs`:

- **P1 — the clip pin (the acceptance)**: with the editor armed and the
  picker open, the picker panel rect and the editor host panel rect are
  **disjoint** (`matches!`-free structural fact: interval intersection on
  both axes is empty). Below arm.
- **P2 — the flip re-check**: same disjointness with the anchor fixture
  pushed near `viewport-floor` so `flipped` resolves true; additionally the
  flipped panel's bottom stays at or above the editor panel's top.
- **P3 — the field band**: the editor's field row rect (exposed as a new
  probe out seam off the M1 mount) is fully outside the picker panel rect —
  the user-facing restatement of P1 (guards against a fix that moves the
  overlap off the panel but back onto the field).
- **P4 — the clear affordance is reachable** (symptom 2, typed outcome):
  with the picker open, driving the field's clear affordance click routes
  one `edited("")` through the probe's counter (the existing
  `date-edited-count` seam). This is the regression pin for "the click was
  swallowed"; it does not assert any text.
- **P5 — the keep-alive edge**: re-publishes the picker anchor when the
  editor panel moves while the picker is open (scroll/resize keep-alive) —
  assert the disjointness survives a synthetic move of the editor panel
  source rect. (If the probe harness cannot move the panel post-mount, pin
  the `changed`-edge publish on the AnchorMirror seam instead and record
  the limitation here at stage 2.)

Existing pins (`om_us2_s1_*`) stay byte-identical — they still pass: the
closed-cell anchor is unchanged for the M1 mount, and P1's oracle
supersedes nothing they assert.

## 4. Open questions — all resolved

| # | Question | Decision |
|---|---|---|
| Q1 | Re-anchor to the field rect or the whole editor panel? | **Whole editor panel** (D1) — G-2 then guarantees the field visible by construction, and the flip arm cannot re-clip the field. |
| Q2 | New facts global vs extend `DatePickerFacts`? | **Extend** (D2). |
| Q3 | Who publishes the picker anchor — the view, the adapter, or the M1 mount? | **The M1 mount** via `AnchorMirror` on the host's panel out seams (D1) — live only while the editor is open, no staleness, no adapter involvement. |
| Q4 | Should the picker's placement change (e.g. forced `below`)? | **No** — keep `auto`; the flip arm is re-pinned by P2. |
| Q5 | Is the M4 embedded picker (`track-entries`) affected? | **No** — it anchors at its own form field (a distinct facts push), not a stacked host pair; out of scope, no change. |
| Q6 | Cross-host stacking contract (underlay vs older panel)? | **Out of scope — #1254** (D3). |
| Q7 | Does symptom 2 need adapter work? | **No** (D4) — reachability only; typed pin P4 guards it. |

## 5. Implementation sketch (stage 2)

1. `menu-facts.slint`: add `picker-anchor-x/y/width/height` to
   `DatePickerFacts` with the D2 comment.
2. `app-window.slint` M1 mount: `AnchorMirror` bound to the host's
   `panel-*` out seams, `published` writes the `picker-anchor-*` set.
3. `app-window.slint` M2 mount: read `picker-anchor-*`.
4. Probe + driver: P1–P5.
5. Diff surface: 2 `.slint` files + 2 test files. **Estimated diff: ~60–90
   lines** (mostly the probe seams and pins; production change ≈ 25 lines).

## 6. Risks

- **R1 — AnchorMirror inside a host panel and window-space coordinates**:
  the host's `panel-x/y` are window-space (the host fills the window), so
  the mirror's sources are already in the right frame; the risk is
  realization order on mount (the time-field trap, `anchored-overlay.slint`
  header lore). Mitigation: the mirror's init + `changed` edges are exactly
  the #954 idiom; the picker cannot open before the editor is realized.
- **R2 — picker taller than the space below the editor panel near the
  floor**: the clamp (law 3) can push the panel up over the anchor when
  `viewport-floor - picker-height < space-2`; then P1/P2 would fail. That
  is a genuine degenerate-viewport case: if stage 2 hits it, the decision
  is record-and-pin-as-degenerate (`..._currently_pins_...` naming per
  AGENTS §5) plus a `backlog:` issue — never a weakened pin.
- **R3 — the echo row grows the editor panel after the picker opens**
  (typing an invalid date while the picker is open): the mirror's
  `changed` keep-alives re-publish; P5 covers the re-anchor. If R2's
  degenerate case triggers here, same disposition as R2.
- **R4 — probe harness limits** (P5's synthetic move): fall back to
  pinning the publish edge; record the limitation in the plan at stage 2.
