# [Plan] Widget Framework Base — ModalDialog, AnchoredOverlay, hosts (round-9 chunk W1)

- **Date**: 2026-11-18
- **Kind**: implementation chunk plan (TDD-first). Branch
  `feat/ui-widget-framework-base`, round-9 chunk **W1** of
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §4.
- **Normative origins**:
  - J2 UX journey plan (ACCEPTED):
    `.artifacts/plans/2026-11-18-widget-framework-ux-plan.md` — rulings
    R-1..R-9, stories US-1/US-2, wiring rows W-1/W-2, §6 O-6 addendum.
  - Round plan §1 O-6 (widget-centralization mandate); chunk W1 row.
  - Research: `.artifacts/reports/2026-11-18-round9-widget-framework-research.md`
    (§4 base-widget specs, §5 doc outline) and
    `.artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md`
    (§3.2 absorption set, §4 sizing defaults).
- **Scope**: the base layer only — `ModalDialog` + `ModalDialogHost`,
  `AnchoredOverlay` geometry helper + `AnchoredOverlayHost`, scrim
  token, R-6 sizing tokens, dismissal law, automatic walk-mask arming,
  the geometry assertion catalogue (§7 below), and the
  `docs/widget_interactions.org` skeleton. **No consumers migrated**;
  exactly two proof mounts (one modal, one anchored). §9 lists the
  deliberately-out-of-scope items (filed as backlog issues).
- **Non-goals**: Slint-native Dialog/anchor APIs (absent in 1.18.1);
  PopupWindow (R-1, round-5 D6 stands); W2's migration wave; W3's
  TextButton (the button model renders CtaButtonArea-style rows here
  and swaps in W3 — inventory report §7 W-1 amend).

## 1. Recorded design decisions

### D-W1-1 — ModalDialog host mount protocol

Registration is **mount-site-declared, not centrally registered** (no
orchestrator; AGENTS §3). Each dialog keeps today's shape — a
model-driven open flag and one conditional sibling pair in
`app-window.slint` — but the pair collapses into **one** conditional
mount of `ModalDialogHost` (the scrim, the absorbing dismissal layer,
and the panel wrapper are the host's anatomy; the dialog body never
re-declares them):

```slint
// ui/widgets/modal-dialog-host.slint (new)
component ModalDialogHost {
    in property <string> title;              // labels the panel
    in property <int> walk-stops;            // ordered tab-stop count (D-W1-4)
    in property <[DialogButtonModel]> buttons;
    in property <DialogKind> kind;           // edit | confirm  (R-4)
    in property <@children> content;         // the dialog body (default children)
    callback button-invoked(int);            // index into buttons
    callback accepted();                     // edit-kind outside click = accept/save
    callback dismissed();                    // confirm-kind outside click / scrim click
    callback closed();                       // unmount handshake; adapter restores focus
}
```

- **Mount protocol** (replaces the backdrop+panel sibling pair at the
  six modal sites when W2 migrates them; W1 proves it once at
  capture): `if (<site-open-flag>): ModalDialogHost` as a `key-root`
  sibling in the modal-class block (after the anchored block — later
  siblings win the hit test). The site wires `accepted/dismissed/
  button-invoked/closed` to its existing adapter seams; the open flag
  stays the unmount condition so the host can never linger.
- **Scrim (R-3)**: full-window Rectangle painted `Theme.scrim`
  (D-W1-5), carrying the dismissal TouchArea with the r4 D2 idiom
  (`clicked` + right-button-down `pointer-event`). Edit-kind scrim
  click routes `accepted()`; confirm-kind routes `dismissed()` —
  drafts discard **only** through the cancel seam (research §4(d) law
  4).
- **Centering/geometry**: host centers the panel —
  `x: (root.width - w) / 2`, `y: max(Theme.space-4, min((root.height -
  status.height - h) / 2, status.y - h - Theme.space-2))` — the
  capture-overlay expression, promoted into the host so it stops being
  per-mount copy-paste.
- **Sizing (R-6)**: `width = min(root.width - 2·Theme.space-2,
  max(preferred-width, Theme.dialog-min-width))` bounded above by
  `Theme.dialog-max-width`; height by content, capped at
  `status.y - 2·Theme.space-4` — caps are bounds, never targets; no
  `vertical-stretch` anywhere in the host (the review-walk stretch
  class is structurally impossible through the host).
- **Focus restore**: on unmount the host fires `closed()`; the adapter
  restores focus to the opener through the existing `FocusFacts` /
  `focus-*` seam (PR #872 machinery — nothing new). The host itself
  holds no opener reference (the adapter does; §5.1 presentation-only
  law).
- **Absorbing panel root**: a full-surface TouchArea inside the panel
  root under the body (the research §4(a) law): clicks inside the
  panel bounds never close, structurally. Interactive children sit
  above it and keep their own handling.
- **No key handlers** in the host (AGENTS §5.1 / research §3): Escape
  resolves through the root capture + action layer as today.

### D-W1-2 — The button model (US-4, R-4 adjacency)

```slint
struct DialogButtonModel {
    label: string,
    role: DialogButtonRole,   // none | accept | dismiss | destructive
    enabled: bool,
    autofocus: bool,
}
enum DialogButtonRole { none, accept, dismiss, destructive }
```

- Rendering for W1: rows of CtaButtonArea-style buttons arrayed
  accept-cluster right / dismiss left (research §4(a)); W3's TextButton
  absorbs the renderer (inventory report §7 W-1 amend — recorded, not
  done here).
- `autofocus` picks the initial focus target; the host falls back to
  the first `accept` button, else the first stop. The destructive
  role never takes autofocus (research §2 modal laws: Escape never
  bound to the destructive default).
- `button-invoked(index)` is the only intent; the adapter maps index →
  verb. The widget never interprets roles beyond layout/focus.

### D-W1-3 — AnchoredOverlay geometry interface

A **pure geometry helper** — anchor facts in, placed rect out; no
TouchAreas, no dismissal, no focus of its own:

```slint
// ui/widgets/anchored-overlay.slint (new)
enum OverlayPlacement { auto, below, above }
component AnchoredOverlay {
    in property <length> anchor-x;
    in property <length> anchor-y;
    in property <length> anchor-width;
    in property <length> anchor-height;
    in property <length> viewport-width;    // root.width
    in property <length> viewport-floor;    // status.y — the usable floor
    in property <OverlayPlacement> placement: OverlayPlacement.auto;
    out property <length> computed-x;
    out property <length> computed-y;
    out property <bool> flipped;            // observation seam for tests
}
```

- Laws (the date-picker/timer-setter expression, deduplicated — it is
  copy-pasted ~10× today):
  1. `auto` prefers **below**: `anchor-y + anchor-height + gap`; gap =
     `Theme.space-1`.
  2. Flip **above** (`anchor-y - height - gap`) when the below rect
     would cross `viewport-floor`.
  3. Clamp y into `[Theme.space-2, viewport-floor - height -
     Theme.space-2]`; clamp x into `[Theme.space-2, viewport-width -
     width - Theme.space-2]` (x baseline = `anchor-x`).
  4. Width law (caller-side, documented on the component): `min(viewport
     - 2m, max(preferred, floor))` — caps are bounds (R-6).
  5. The overlay **never intersects the anchor rect** when either
     placement resolves (assertion G-2 below pins it).
  6. The helper computes only; the host sibling-order grammar
     (dismissal layer first, panel after) stays at the mount site.
- `AnchoredOverlayHost` (same file) wraps the helper with the facts
  mirroring: `in` anchor facts + open flag, `out` computed position,
  an absorbing dismissal underlay, and **automatic walk-mask arming**
  (D-W1-4). The per-site `changed absolute-position` idiom (#954) is
  absorbed here in W1's proof mount; the full absorption of the 7
  mirror sites is W6 (round plan) — W1 only proves the mechanism.

### D-W1-4 — Structural walk containment (auto-armed MaskWalkFacts)

Law: **mounting through a host arms the walk mask — containment is a
property of being an overlay, not a per-widget ritual.**

- `ModalDialogHost` / `AnchoredOverlayHost` on mount:
  `MaskWalkFacts.extent = walk-stops` (declared by the panel body via
  the host's `walk-stops` property; panels with dynamic stops publish
  the count), `MaskWalkFacts.cursor = 0`, and focus the first stop.
- The host re-publishes `cursor` when its own focus moves and consumes
  `MaskWalkFacts.focus-index/nonce` via the existing `mask-focus`
  AppWindow seam (mount-walk-facts.slint contract unchanged — PR #872
  machinery, zero adapter arithmetic changes).
- On unmount the host clears `MaskWalkFacts.armed`'s site contribution
  through the adapter's existing site table (the adapter already owns
  `armed`; the host only owns extent/cursor — same handshake the
  timer-setter/capture use today, promoted to the host).
- No nested widget can opt out (the extent is the host's push); no new
  overlay can forget the ritual.

### D-W1-5 — Scrim styling token

New token in `theme.slint` (palette layer; semantic consumers only):

```slint
// The modal scrim (J2 R-3): translucent dark drawn under every
// ModalDialogHost panel — visible, non-interactive (the host's
// dismissal TouchArea carries interaction, the paint is passive).
out property <color> scrim: #0b0d1280;
```

Value is a first cut from the surface ladder's darkest base
(`#0b0d12` at ~50% alpha); visual tuning is operator-owned at stage 3
walkthrough (§8). Anchored overlays get **no scrim** (light-dismiss
family, research §2 popover laws) — the token is modal-only.

R-6 sizing tokens (same file, sourced from inventory report §4):

```slint
out property <length> dialog-min-width: 280px * base-scale;
out property <length> dialog-max-width: 560px * base-scale;
out property <length> menu-min-width: 200px * base-scale;
out property <length> menu-max-width: 360px * base-scale;
```

### D-W1-6 — AccessibleRole.dialog: absent; fallback recorded

Verified against the pinned sources: `i-slint-common-1.18.1/
enums.rs` `AccessibleRole` (line 466) has **no `Dialog` variant** in
slint 1.18.1 (roles end at the landmark set: Banner … Search). The
research's Q4 is therefore settled negatively for our pin.

**Fallback (recorded decision)**: the ModalDialogHost panel root takes
`accessible-role: AccessibleRole.region` + `accessible-description:
title` (landmark semantics — the same treatment review-view/board-view
use). Button/field children keep their own roles. A comment on the
host and a backlog issue (#TBD, §9) track adopting `Dialog` on the
next slint bump that ships it; the host is the single edit point.

## 2. Proof mounts (exactly two)

1. **Modal proof — capture overlay** (`compositions/capture-overlay.slint`,
   mount app-window.slint:1460-1487): the existing backdrop+panel pair
   is replaced by one `ModalDialogHost` mount (`kind: edit` — outside
   click = save-per-R-4? **No**: capture keeps its documented
   discard-and-close semantics as `kind: confirm`-style dismiss via its
   existing `capture-dismiss-requested` seam; R-4's edit/confirm mapping
   is the host's two intents and the site maps dismiss→discard, exactly
   today's behavior). Rationale: already walk-masked (extent=1), single
   mount, buttons can be exercised by adding a cancel/accept model
   without changing dispatch (the accept/dismiss intents route to the
   existing `capture-accepted`/`capture-dismiss-requested` seams).
2. **Anchored proof — timer-setter popover**
   (`compositions/timer-setter.slint`, mount app-window.slint:845-886):
   backdrop + hand-written x/y clamp expressions are replaced by
   `AnchoredOverlayHost` fed from `TimerSetterFacts`. Rationale: the
   research calls it "best-behaved of the set", single mount site, so
   the geometry helper is proven against a known-good anchor before W2
   risks the twin pickers.

Both mounts are behavior-preserving today (same seams, same model
wiring); the deltas the journeys assert are the scrim appearing
(modal), the absorbing panel root, and the auto-armed mask.

## 3. TDD order (test first at every step)

1. **G-catalogue tests against a probe mount** (`tests/
   widget_framework_probe.slint` + `tests/widget_framework_probe_tests.rs`,
   the house probe pattern — dropdown_probe/walk_probe precedents):
   written before the helper components exist; red, then implement
   `anchored-overlay.slint`/`modal-dialog-host.slint` + theme tokens to
   green.
2. Theme tokens (scrim + four size tokens) with the pure-token fold
   test (F-3-style: values are `Npx * base-scale` expressions).
3. Host components to green; absorbing-panel + walk-arming probes.
4. Proof mounts swapped; the two mount-level journey tests
   (`tests/journeys.rs` additions `us1_s3`, `us2_s1`) green.
5. `docs/widget_interactions.org` skeleton (§8) + census cross-ref
   stub.

## 4. Journey tests (rig tests, `on_slint_thread` pattern)

| Test | Story step | Asserts (typed/structural — never text) |
|---|---|---|
| `us1_s3_inside_click_never_closes` | US-1/S3 | click the panel dead space → open flag still true; click scrim → `dismissed` intent fired (flag path) |
| `us2_s1_overlay_below_anchor` | US-2/S1 | open anchored proof → G-2 non-intersection + computed-y = anchor bottom + gap |

US-1/S1 (centered + scrim), US-1/S2 (walk containment), US-1/S4
(focus restore), US-2/S2 (flip), US-2/S3 (no-cover) are pinned at the
probe layer (§5 catalogue) — one behavior, one test, lowest layer
(AGENTS §5); `tests/journeys.rs` carries only the routing-level pair
above.

## 5. Geometry assertion catalogue (the chunk's test spine)

All observations ride the facts/probe pattern (probe file exposes
`panel-x/panel-y/panel-w/panel-h/anchor-*` as `out` properties the
test reads; window fixtures use the existing rig support). No text
assertions, no quality ceilings, deterministic by construction.

| ID | Assertion | Law / origin |
|---|---|---|
| G-1 | Modal panel centered: `panel-x == (win-w - panel-w)/2`; y within `[space-4, status.y - panel-h - space-2]` | US-1 AC-1 (no layout shift is G-9); D-W1-1 |
| G-2 | `overlay ∩ anchor == ∅` in **both** placements (below and flipped-above): rects' intersection is empty | US-2 AC-1; research §4(b) law 5 |
| G-3 | Below chosen iff `anchor-bottom + gap + panel-h ≤ viewport-floor` (`flipped == false`); else above | US-2 AC-2; flip law 1-2 |
| G-4 | Clamped: `space-2 ≤ x` and `x + w ≤ win-w - space-2`; same for y vs floor — exercised with an anchor near both edges (the dropdown-160 scenario) | clamp law 3 |
| G-5 | Anchor at window edge + panel wider than remaining space: x clamped, panel never off-window; anchor partially off-window clamps to the margin | clamp law 3 edge case |
| G-6 | Sizing bounds: `dialog-min-width ≤ panel-w ≤ dialog-max-width` when content narrower / wider respectively; preferred wins between the bounds | R-6; US-1 AC-4 |
| G-7 | Height: content height, never stretched — probe with a 1-row body vs a 20-row body; both ≤ `status.y - 2·space-4` | review-walk stretch kill; D-W1-1 |
| G-8 | Scrim: full-window rect, `background == Theme.scrim`, painted **below** the panel sibling (hit-test order probe: a click at a point inside the panel but over no child lands on the absorbing root, not the scrim) | R-3; dismissal law 1 |
| G-9 | **Layout shift (R-7/R-8 pin)**: probe view mounts a sibling stack below the anchor; rects of the siblings asserted equal before open, while open, and after close — the anchored host and the modal host both change nothing | US-1 AC-1; J2 R-7 |
| G-10 | Walk containment: Tab from the last stop wraps to the first (`MaskWalkFacts.focus-index` clamps; cursor never exceeds `walk-stops - 1`), armed on mount, cleared on unmount | US-1 AC-2; D-W1-4 |
| G-11 | Dismissal law: pointer-down right on scrim closes (r4 D2 idiom); click on scrim `clicked` closes; click on panel root does not | US-1 AC-3; D-W1-1 |
| G-12 | Focus restore: after `closed()` the adapter seam receives the restore push (FocusFacts nonce advanced; opener's scope re-focused) | US-1/S4; PR #872 |
| G-13 | Button model: rendered row count == model length; `button-invoked(i)` carries the index; disabled buttons neither focus nor invoke; autofocus lands on the declared button else first accept | US-4 AC-1; D-W1-2 |

Test placement: G-1..G-13 in `widget_framework_probe_tests.rs`;
journey routing pair in `journeys.rs`; token fold test inline
(usability_components.rs pattern). Every test feeds commands and
asserts terminal structure — no sleeps (AGENTS §5 clock discipline;
fixtures are clock-free).

## 6. Files touched (diff estimate)

| File | Change | ~Lines |
|---|---|---|
| `ui/widgets/anchored-overlay.slint` | new (helper + host) | +130 |
| `ui/widgets/modal-dialog-host.slint` | new (host + model structs) | +180 |
| `ui/theme.slint` | scrim + 4 size tokens | +12 |
| `ui/app-window.slint` | imports; capture mount → host; timer-setter mount → host; exports | ±70 |
| `compositions/capture-overlay.slint` | body wrapped for host (buttons added, mask ritual deleted) | ±30 |
| `compositions/timer-setter.slint` | mask ritual deleted (host owns) | ±20 |
| `tests/widget_framework_probe.slint` | new probe mount | +90 |
| `tests/widget_framework_probe_tests.rs` | G-1..G-13 | +450 |
| `tests/journeys.rs` | us1_s3, us2_s1 | +80 |
| `tests/support/` | fixture wiring for the probe | +40 |
| `docs/widget_interactions.org` | new skeleton (§8) | +90 |
| `docs/ux-surface-census.org` | W1 entry stub | +10 |
| **Total** | | **≈ +1200 / ±120 modified** |

New files carry the AGENTS §6 header (`taskboard-ui-slint` is an
application crate: `// SPDX-License-Identifier: AGPL-3.0-only`);
`#![forbid(unsafe_code)]` is crate-level already (no new .rs crate).

## 7. Risks & traps

1. **Sibling-order hit-test regressions at the proof mounts**: the
   host must keep the "dismissal below, panel above" ordering inside
   itself; a wrong order resurrects the fall-through class. Pinned by
   G-8/G-11.
2. **Mask handshake double-arm**: capture already seeds
   `MaskWalkFacts.extent` in its own `init`; moving arming to the host
   must delete the panel's seeds or the host push races the panel init
   (last-writer-wins is nondeterministic across backends). Trap: run
   the probe test that asserts a single deterministic extent.
3. **`changed absolute-position` unreliability in the testing backend**
   (the time-field comment, widgets/time-field.slint:92-126): the
   AnchoredOverlayHost must not depend on `changed` for the initial
   layout pass — feed anchor facts via the same wrap-idiom the
   time-field uses (`open`-flip mirror + `changed` keep-alive), and
   assert G-2/G-3 in a fixture that opens after layout settles.
4. **`status` visibility in the host**: the centering/floor math needs
   `status.y`; it is `root.status`-scoped today. The host takes
   `viewport-floor` as an injected property from the mount site — no
   upward name reaching (rule C/L2 purity).
5. **Capture outside-click semantics (R-4 vs today)**: capture's
   outside click discards typed text. W1 is behavior-preserving; the
   R-4 edit-kind "outside = save" flip for capture/recu editors is a
   W2 migration decision, explicitly not here (flagged so stage 2 does
   not "fix" it opportunistically).
6. **Scrim alpha on the golden visuals**: `tests/golden_visuals.rs`
   renders full-window scenes; a new scrim over the capture mount may
   shift goldens. Expected: the capture-related goldens regenerate
   deliberately in the same commit (a visual-change chunk, J2 governs
   the change); any *unrelated* golden drift is a bug, not a re-record.
7. **Base-scale**: every new dimension is a `Theme` token expression
   (token lint); bare px only inside token definitions.

## 8. `docs/widget_interactions.org` skeleton outline

Landed this chunk as the skeleton (sections with 2-5 sentence stubs
each; W7 fleshes it to the full taxonomy):

1. **Purpose & scope** — every overlay/dialog/flyout is an instance of
   a base widget; ad-hoc overlays forbidden (lint-by-review).
2. **Widget taxonomy** — modal dialog; anchored popover/flyout; menu;
   palette; inline editor flow; toast; tooltip (stub list; per-type
   contracts land with migrations).
3. **Per-type interaction contract** — the four laws (positioning +
   flip/clamp; sizing caps-are-bounds; dismissal inside-safe/outside-
   per-kind; focus enter/trap/restore) with the R-4 kind table.
4. **Base implementations** — ModalDialogHost, AnchoredOverlay(+Host),
   MaskWalkFacts integration, the mount checklist (the six steps a new
   dialog site performs).
5. **Usage rules** — dialog vs popover vs inline; destructive-
   outside-click prohibition; button roles/placement; one-open-overlay
   exclusivity pointer.
6. **Testing** — the G-catalogue patterns (probe + facts observation;
   fall-through probe; walk-containment probe; focus-restore probe).
7. **Known limitations & backlog** — no `AccessibleRole.dialog` in
   1.18.1 (D-W1-6 fallback), no inert property (hover on covered
   chrome), PopupWindow rejection record (R-1), 32px touch-target
   upgrade path.

## 9. Out-of-scope items → backlog issues (opened with this plan)

- AccessibleRole.dialog adoption on slint bump (D-W1-6).
- Capture/recu outside-click R-4 kind flips at migration (W2 concern,
  behavior change needing the journey updates).
- Hover-inertness of covered chrome (no Slint inert property).
- Sizing decoupling for dropdown-flyout anchor-width cap (R-6 menu
  tokens exist after W1; consumer change is W2/W6).

(Numbers reported in the stage-1 report.)

## 10. Verification & handoff

- Stage 2 self-check: `scripts/ux-coverage.sh <J2 plan> --gate` (this
  chunk implements J2's W-1/W-2 rows; the journey plan stays the gate
  input) + pre-commit hooks + flock-wrapped `cargo nextest run -p
  taskboard-ui-slint` (dispatcher owns full suites).
- Handoff state: PR open from `feat/ui-widget-framework-base`, green
  `gh pr checks`; worktree removed per AGENTS §8 lifecycle.
- Stage 3: walkthrough protocol + scrim visual tuning feedback to the
  operator (D-W1-5 first cut).
