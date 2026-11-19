# Button & Checkbox Primitives (round 9, chunk 13 / W3) — Implementation Plan

- **Date**: 2026-11-19
- **Kind**: TDD implementation plan — chunk 13 of round 9, first of the
  strictly-serial O-6 centralization chunks (13→14→15→16→17).
- **Branch**: `refactor/ui-button-primitives` (this worktree:
  `../taskboard-WT-refactor-ui-button-primitives`, off `main` @ 8069b95e).
- **Origin**: round plan §1 O-6 + §4 W3
  (`.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md`);
  inventory report §3 (TextButton/CheckBox/ToggleStrip taxonomy rows),
  §4 (sizing law), §5 (migration order, normative)
  (`.artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md`);
  J2 rulings R-3..R-9 + the §6 O-6 addendum + the W-1 amend
  (`.artifacts/plans/2026-11-18-widget-framework-ux-plan.md`).
- **Closes**: #950 (no text Button primitive), #953 (checkbox grammar
  hand-rolled three ways).
- **Status**: PLANNED — no implementation yet; this commit is docs(plan) only.

## 1. Scope

Two new base widgets in `ui/widgets/` (one file per component, house
convention) plus the absorption of every TextButton-family and CheckBox
consumer on `main`:

- `ui/widgets/text-button.slint` — `TextButton` absorbing CtaButton,
  SettingsButton, VerbButton, CtaButtonArea, and the ModalDialog
  `DialogButton` (the J2 W-1 amend).
- `ui/widgets/check-box.slint` — `CheckBox` absorbing the task-view
  done-box verbatim ActivationStop copy and the TaskCard done checkbox.
- `ui/widgets/toggle-strip.slint` — `ToggleStrip` absorbing the recur
  editor's 7 weekday check-glyph IconButtons (inventory §5 step 3
  explicitly includes it in W3: "CheckBox + ToggleStrip (deletes the
  done-box copy + check Paths + weekday row)").

### Out of scope (chunk boundaries — serial chunks 14–17)

- **Chunk 14 (W4, LabeledValueRow)** owns the task-view metadata rows,
  Effort row, LabeledField. This chunk touches `task-view.slint` ONLY
  inside the `done-box` block (:578–650) and does not reflow anything
  else in that file.
- **Chunk 15 (W5)** owns `GhostOpener` (add-task/add-column ghosts),
  ActionChip, SectionHeader, StatusNotice, ConfirmStrip, and the
  `Theme.tone-color/tone-fill` ladder functions (F-3). This chunk adds
  NO Theme functions; `TextButton` maps tones with local expressions.
- **Chunk 16 (W6)** owns MenuShell and the priority picker; the
  priority-slot check glyph (task-view inline slot rows) stays as-is.
- **Chunk 17 (W7)** owns the `docs/widget_interactions.org` taxonomy
  extension. This chunk adds only a one-line pointer there if the file
  convention requires new widgets be listed (it does not — W7 lands the
  full table).

## 2. Absorption map (convert / stay / why)

All anchors are symbol anchors per #775 (verified on main @ 8069b95e;
task-view refs re-anchored after chunk 10's editor surface landed).

### 2.1 TextButton family (F-1: the button grammar ×8; this chunk deletes 4 bespoke copies + the DialogButton twin)

| Instance (symbol anchor) | Verdict | Notes |
|---|---|---|
| `board-layout.slint::CtaButton` (def :33; uses :301, :305 — the two empty-board CTAs) | **convert → delete** | The `export` was for the deprecation workaround; with the def gone the export goes too. emphasis=cta (space-6 min-height, 4·space-6 min-width, raised→hover bg, focus-only border). |
| `timer-setter.slint::CtaButtonArea` (def :32; use :241 `commit-touch`) | **convert → delete** | emphasis=cta + `enabled` gate + tooltip arm. Generalizes into TextButton properties `enabled` + `tooltip-text` (the TooltipState.hover idiom, blanket rule a). Its `accessible-description` becomes a TextButton `description` property. |
| `settings-view.slint::SettingsButton` (def :372; 13 uses: :982, :1029, :1048, :1053, :1171, :1181, :1201, :1206, :1246, :1293, :1312, :1317, :1331) | **convert → delete** | The `click-through` ceremony variant (r2 D34) becomes a TextButton `click-through` property (verbatim from the inventory §3 row). Content-sized min-height law preserved (see §4 sizing). |
| `review-walk-overlay.slint::VerbButton` (def :111; 12 uses: :274, :278, :306, :310, :314, :327, :331, :335, :339, :358, :369, :373) | **convert → delete** | size=compact (space-5), surface bg + hover/pressed fill, always-on border. **Behavior change B-1** (§5): gains the FocusScope tab stop + Return/Space it lacked (inventory Q-A; the only F-1 copy without keyboard). |
| `modal-dialog-host.slint::DialogButton` (internal, the D-W1-2 button row) | **convert → re-express** | DialogButton stays as the host's thin model adapter (index, autofocus, `host-open` re-arm) but its body renders a TextButton; it needs TextButton's `public function request-focus()` for the mount-focus half. This is the J2 W-1 amend ("the button model is rendered by TextButton"). |
| `icon-button.slint::IconButton` (C, the gold standard) | **stay** | Already centralized; a different class (icon-first, square space-6, tone ladder, tooltip arm). TextButton must match its keyboard/a11y grammar shape, not absorb it. |
| `segmented-control.slint` segments / `board-bar` switcher segments | **stay** | Already centralized widgets (the F-1 count includes their internal copies of the grammar; de-duplicating *across* centralized widgets is not this chunk's mandate and risks churn in hot files). |
| add-task ghost / add-column ghost (`board-layout.slint` ~:752/:847) | **stay** (W5) | GhostOpener is chunk 15's deliverable per the round plan W5 row; the dispatch's "ghost-variant" reading is resolved in favor of the inventory §3 row-4 family list (the four text buttons), which is normative. |

### 2.2 CheckBox family (F-8: checkbox anatomy ×3 + duplicated check Path)

| Instance (symbol anchor) | Verdict | Notes |
|---|---|---|
| `task-view.slint::done-box` (:578, incl. the verbatim ActivationStop re-statement at ~:600–630 whose comment says "keep the copies in sync") | **convert → delete** | The F-8/row-30 sanctioned copy dies. CheckBox carries `accessible-role: checkbox` natively, so the reason for the verbatim copy (ActivationStop's `accessible-role` being constant-only) evaporates. The EditingFacts/TaskViewWalkFacts publish/latch mirroring stays view-side, driven by CheckBox's `out focused` via a `changed` handler (the CtaButtonArea `commit-hovered` derived-property idiom). The check `Path` moves into CheckBox. |
| `task-card.slint::done-box` (:203; TouchArea `done-hit` :221; check Path :224) | **convert → delete** | Card variant: **no tab stop** (deliberate per-card focus posture, :240–242) → CheckBox `focusable: false`. State-less (renders `model.done` only) — CheckBox is state-less by design. `out done-hover` (UX-R8 D-8) re-expresses over CheckBox's `out hovered`. |
| `recur-editor.slint` weekday toggles (:163–210, 7 check-glyph IconButtons) | **convert → ToggleStrip** | `ToggleStrip { options, checked-mask, picked(int) }` per inventory §3. Geometry note: the row is 7 adjacent IconButtons today; ToggleStrip keeps the check-icon + tone-flip rendering inside the strip so the row's visual grammar is preserved (goldens gate this). |
| priority-slot check glyph (`task-view.slint` inline slot rows) | **stay** (W6) | Q-C moves the picker in chunk 16; touching it here would collide with MenuShell. |
| `widgets/activation-stop.slint::ActivationStop` | **stay** | Untouched; it serves its remaining non-checkbox consumers. |

## 3. TextButton contract (the customization points)

```
component TextButton inherits Rectangle {
    in property <string> label;              // paint + a11y name + click-by-label seam
    in property <string> description: "";    // accessible-description (timer commit's fact)
    in property <Emphasis> emphasis: Emphasis.normal;  // enum { normal, cta }
    in property <Size> size: Size.regular;   // enum { compact, regular }  (§4)
    in property <bool> enabled: true;        // dims label, swallows click, gates keys+AT
    in property <bool> click-through: false; // r2 D34: fires regardless of enabled; adapter renders the refusal
    in property <string> tooltip-text: "";   // non-empty arms the TooltipState.hover idiom
    in property <[string]> hint-keys: [];    // tooltip key hints (timer passes ["Return"])
    callback clicked();
    out property <bool> focused;             // DialogButton autofocus / view mirroring seam
    public function request-focus();         // forwards to the internal FocusScope
}
```

Internal grammar (the single copy of what F-1 found ×8): full-size
`FocusScope` overlay with `focus-on-tab-navigation`,
`AccessibleRole.button`, modifier-guarded Return/Space, `focus()`
forward from the TouchArea (two-click-bug dodge, TouchArea declared
**after** the scope so the scope's keys win hit-testing — the CtaButton
order, per F-1's drift note), `accessible-enabled`, one gate shared by
pointer/keys/AT (`enabled || click-through`).

Background/border ladder (union of the absorbed variants, geometry
preserved per variant):

| Variant emphasis×size | bg | border |
|---|---|---|
| normal+regular (SettingsButton) | raised; disabled→surface | 0; hover **or** focus → focus-ring-width/focus-ring (the SettingsButton hover-border) |
| normal+compact (VerbButton) | surface; hover/pressed→surface-hover | always focus-ring-width/focus-ring |
| cta (CtaButton/CtaButtonArea/DialogButton) | raised; hover→surface-hover; disabled→surface | always focus-ring-width; focus → Semantic.focus-ring else Theme.border |

Disabled label color: `Theme.border` (cta family) — SettingsButton uses
`text-secondary`; TextButton preserves per-variant via emphasis? No:
SettingsButton's disabled `text-secondary` becomes the single law (a
color-contrast improvement on a disabled control is invisible to
journeys and sub-pixel to goldens only if settings has a disabled
button frame captured — the goldens will tell; if a golden shifts, that
is an intentional recorded change with the frame named in the commit).

## 4. Sizing (§4 sizing law adoption, R-6)

- compact = space-5 (24px × base-scale), regular = space-6 (32px ×
  base-scale), cta = space-6 min-width 4·space-6 — as recommended by
  inventory §4.
- **Height law**: `min-height` per size + content height (no fixed
  `height`), EXCEPT compact which keeps VerbButton's fixed space-5
  behavior via `height: max(min-height, content)`. This preserves
  SettingsButton's `min-height: label.preferred-height + 2·space-2`
  content-sized law exactly (its buttons may exceed 32px at base-scale
  > 1 — normalizing them to a hard 32 would shift settings-view layout
  and goldens for no functional gain). Where a variant used fixed
  `height` (CtaButton/VerbButton), content never exceeds the floor, so
  `max()` is identity there. **Zero-geometry-delta goal**: every
  absorbed mount renders byte-identical rects; goldens are the oracle.
- min-width: cta → 4·space-6; normal → content + 2·space-3 padding
  (SettingsButton's bare `space-6` min-width is dead in practice — its
  centered label always exceeds it; drop with a note).
- All lengths are `Theme` token expressions, never bare px.

## 5. Intentional behavior changes (each needs a recorded reason + a new test)

| ID | Change | Reason | New test |
|---|---|---|---|
| B-1 | VerbButton (walk overlay's 12 buttons) gains a tab stop + Return/Space | The only F-1 copy with no keyboard; inventory Q-A recommends the gain; round plan W2 already "noticed" the gap. Journey-visible change, operator-approved direction. | New journey probe: from the walk overlay's first stop, Tab reaches the verb row and Return activates the focused verb (extend `walk_probe`/`widget_framework_probe`, named for the walk's tab order). |
| B-2 | SettingsButton disabled-label color `text-secondary` → `Theme.border` law for cta-family vs normal — resolved per §3 table (normal keeps text-secondary; only cta-family disabled uses border) | No change for settings (normal). Recorded to prevent a stage-3 "drift" flag. | none needed (no change) |

Everything else is geometry-identical replacement: same labels, same
click-by-label seams, same dispatched commands. The existing journeys
(`journeys.rs`: "Back", "Quit taskboard", "Undo"/"Redo" click-by-label;
`us4_s1_modal_containment` dialog buttons; timer journeys) and the
goldens prove behavior preservation unchanged — they must re-pass with
no edits and no re-blessing. Any golden shift = a geometry regression
or an unrecorded change: stop and diagnose, never re-bless.

## 6. CheckBox & ToggleStrip contracts

```
component CheckBox {                       // state-less: renders `checked`, never owns it
    in property <bool> checked;
    in property <bool> focusable: true;    // false = the card's no-tab-stop posture
    in property <string> checked-label;    // verb pair, e.g. "Mark task done" / "Mark task not done"
    in property <string> unchecked-label;
    callback toggled();
    out property <bool> focused;           // view-side walk-facts mirroring
    out property <bool> hovered;           // card's done-hover seam
}
```

- space-4 square, `radius-small`, success border when checked, hover
  surface under the fill, the check `Path` (single copy, viewbox 12,
  `M 2 6 L 5 9 L 10 3`), `AccessibleRole.checkbox`, modifier-guarded
  Return/Space, pointer cursor. This is exactly the task-view done-box
  anatomy generalized; the card variant is `focusable: false`.
- ToggleStrip: `options: [string]`, `checked-mask: [bool]`,
  `picked(int)`; renders the existing check-IconButton row verbatim
  (7 slots, info/neutral tone flip) — a thin composition over
  IconButton, not a new grammar.

## 7. TDD order (tests first, red → green per step)

1. **Red — TextButton probe** (`widget_framework_probe_tests.rs`,
   probe fixture extended): g-button_tab_stop_and_return_activates;
   g-disabled_swallows_all_three_channels (pointer/keys/AT);
   g-click_through_fires_while_disabled; g-focus_forwards_to_request_focus.
2. **Green**: `ui/widgets/text-button.slint`.
3. **Red — CheckBox probe**: g-check_renders_model_bit_statelessly
   (flip model, no local state); g-check_keyboard_and_pointer_toggled;
   g-unfocusable_variant_is_no_tab_stop (card posture);
   g_focus_and_hover_are_observable (the out-property seams).
4. **Green**: `ui/widgets/check-box.slint`.
5. **Red — ToggleStrip probe**: g_strip_reports_picked_index.
6. **Green**: `ui/widgets/toggle-strip.slint`.
7. **Absorptions, one commit per variant family** (each keeps every
   existing test green before the next):
   a. DialogButton → TextButton (modal host; `us4_s1` + W-1 probes gate).
   b. CtaButton (board-layout; empty-board journeys gate).
   c. CtaButtonArea (timer-setter; timer journeys + tooltip probe gate).
   d. SettingsButton (13 sites; settings journeys gate).
   e. VerbButton (12 sites; B-1's new probe + walk journeys gate).
   f. done-box (task-view; walk/journeys gate the facts mirroring).
   g. TaskCard checkbox (card hover/title probes gate).
   h. weekday ToggleStrip (habit/recur journeys gate).
8. **Golden + journey suite full pass, unchanged**; census updates in
   `docs/ux-surface-census.org` with symbol anchors (the four deleted
   component symbols re-point to `ui/widgets::TextButton` /
   `::CheckBox` / `::ToggleStrip`); closes #950, #953 at merge.

## 8. Verification

Pre-commit hooks + flock-wrapped `cargo nextest` per the dispatch
discipline (the coverage trio never runs inside this subagent; the
dispatcher owns full-suite runs). Per-chunk gates:
`cargo fmt --all -- --check`, `scripts/run-quality-gates.sh cargo
clippy --workspace --all-targets -- -D warnings`,
`scripts/run-quality-gates.sh cargo nextest run --workspace`. Slint has
no doc-tests; `cargo test --doc` is CI's business. Handoff at PR open +
`gh pr checks` green; worktree removed per AGENTS §8.

## 9. Risks

- **R-1 VerbButton tab-order change** (B-1): the walk overlay's tab
  walk gains 12 stops; MaskWalkFacts arming interactions unknown until
  the probe runs. Mitigation: B-1's probe written before migration;
  if the walk's containment law (US-1/S2) breaks, the walk host's mask
  absorbs first, escalation to the dispatcher recorded.
- **R-2 golden drift from the §4 height law**: the `max(min-height,
  content)` identity argument is analytic, not proven; goldens are the
  oracle and any shift is treated as a bug (no re-blessing).
- **R-3 task-view.slint is chunk 10's hot file**: done-box anchors
  re-verified on main (:578–650); the edit touches only that block;
  chunk 14 owns the rest of the file next and rebases over this.
- **R-4 SettingsButton click-through ceremony** (13 sites, r2 D34):
  the property must survive the absorption verbatim; its refusing
  adapter paths have journey coverage (settings flows) — the gate.
- **R-5 TooltipState idiom inside TextButton**: the timer's tooltip
  arms off a derived `changed` wrapper, not the TouchArea directly;
  the generalized property must keep that shape or the tooltip probe
  flakes. Mitigation: keep the derived-property wrap verbatim.

## 10. Diff estimate

- New: ~340 lines (three widget files + headers).
- Deleted: ~520 lines (4 bespoke button components + DialogButton body
  + done-box copy + card checkbox + weekday row boilerplate).
- Touched call sites: ~34 (13 settings, 12 verb, 2 cta, 1 cta-area,
  1 dialog model, 2 checkbox mounts, 1 weekday strip) — mostly
  one-line import/property renames.
- Tests: ~8 new probe cases, 0 existing tests edited.
- Net: roughly **−200 to −300 lines**, ~8 commits.
