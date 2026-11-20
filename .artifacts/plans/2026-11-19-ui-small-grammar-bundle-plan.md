# The UI Small Grammar Bundle — Chunk B8 Plan (round 3)

- **Date**: 2026-11-20
- **Chunk**: B8 of `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md` §16
  (scope: `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md` §B8 — normative).
- **Branch**: `refactor/ui-small-grammar-bundle` (off `origin/main` @ `18f23133`).
- **Issues**: #978, #1034, #1060, #1065, #1027 — five small refactors, all with existing
  test cover. **Stage gate**: pre-commit hooks + `scripts/run-quality-gates.sh cargo
  nextest run --workspace` (flock-wrapped; defer per the #879/#894 rule if contended).
- **Round laws**: (a) typed outcomes; (c) behavior-identical — the corpus is the oracle,
  any expectation change carries a recorded diagnosis; (d) out-of-scope findings become
  `backlog:` issues immediately.

## 0. Round-law posture: the #978 exception

Law (c) binds four of the five items. **#978 is not behavior-identical by intent** —
the issue's deliverable *is* the width change (the anchor-width law's readable-row
floor). **Decision (recorded diagnosis)**: #978 lands as the one sanctioned expectation
change; its diagnosis is the issue body itself (W1 landed the R-6 menu tokens and
migrated no consumer; the 160·scale anchor cap is narrower than the readable-row
floor). Its pins are updated in the same commit, and the plan §1.4 names the affected
pins. Items #1034/#1060/#1065/#1027 are strictly behavior-identical; #1027 is
test-only.

## 1. Item shapes, seams, and evidence

### 1.1 #1060 — `switch_plan_pane` wrap arithmetic → a two-branch match

- **Seam**: `crates/taskboard-ui-slint/src/view_state.rs:1611`
  (`ViewState::switch_plan_pane`) + its tests at :6763–:6772.
- **Shape**: replace the `i64::try_from → usize::try_from → filter(< 2) →
  unwrap_or(1 - min(1))` roundtrip with a match on the *stepped* value that states the
  two-pane geometry directly:
  ```rust
  let stepped = i64::from(self.plan_focus_pane) + i64::from(direction);
  self.plan_focus_pane = match stepped {
      0 => 0,
      1 => 1,
      _ => 1 - self.plan_focus_pane.min(1),
  };
  ```
  (keep the doc comment; the guard above the match — `view() != PlanNextDay` no-op —
  stays byte-identical).
- **Behavior identity, exact edge**: the current code lets `direction == 0` land as a
  valid in-range step (pane stays). The stepped-match above reproduces that exactly
  (`0+0 → 0`, `1+0 → 1`); the "bare `1 - pane` toggle" the issue also floats would
  *not* (a 0-direction stroke would toggle) — **decision**: the stepped-match form,
  not the bare toggle, precisely because the corpus is the oracle and the bare toggle
  changes the `direction == 0` degenerate. Verify by a quick prop-check against the
  old expression if desired (not committed — the existing wrap tests at :6763ff plus
  one new `direction == 0` pin **is** committed: a degenerate-direction pin test in
  the same `view_state.rs` test module, typed on `plan_focus_pane`).
- **Evidence**: existing `switch_plan_pane` unit tests green unchanged + the new
  zero-direction pin.

### 1.2 #978 — the R-6 menu width tokens at the dropdown consumer mounts

- **Seams** (`crates/taskboard-ui-slint/ui/app-window.slint`):
  - the generic dropdown mount: `dropdown-host := AnchoredOverlayHost` at ~:1171 —
    `panel-preferred-width: DropdownFacts.anchor-width` (:1181) and the inner
    `DropdownFlyout { width: DropdownFacts.anchor-width }` (:1211);
  - the quick-list mount: `flyout-host := AnchoredOverlayHost` at ~:1319 —
    `panel-preferred-width: TimeFieldFacts.anchor-width` (:1326); `QuickListFlyout`
    itself fills the host panel (no per-site width binding).
- **Token base**: `ui/theme.slint:137-138` (`menu-min-width: 200px * base-scale`,
  `menu-max-width: 360px * base-scale`) — already landed with W1, pinned by
  `src/token_tests.rs:419-420`; `widgets/anchored-overlay.slint:98` already defaults
  `panel-preferred-width` to the token.
- **Shape**: the mount-side width expressions become the issue's formula
  `min(viewport − 2·Theme.space-m, max(preferred, Theme.menu-min-width))`, applied at
  both `panel-preferred-width` sites and the `DropdownFlyout` width (the flyout's
  explicit width must move with the host or the layout child pins the old law).
  Extract one shared Slint helper — a pure function on a small facts global or a
  `pure fn` in `anchored-overlay.slint` — **decision**: put the formula in ONE place,
  a public pure function exported from `widgets/anchored-overlay.slint`
  (`overlay-menu-width(preferred, viewport)`), and call it from both mounts, so the
  probe copies can consume the same law instead of re-deriving it.
- **Pins to update** (the recorded expectation change): `tests/dropdown_probe.slint`
  re-states the mount grammar verbatim (:188/:192/:208) and must adopt the helper; any
  `dropdown_probe_tests.rs` / `overlay_migration_probe_tests.rs` assertions that pin
  the realized panel width get the new floor. **Add** one probe assertion pinning the
  floor leg (anchor narrower than `menu-min-width` ⇒ panel ≥ token) — floor-style, no
  quality ceiling.
- **Decision (no max-width cap)**: the issue formula floors with `menu-min-width` and
  clamps to the viewport only; `menu-max-width` stays unused by this change (it belongs
  to the future MenuShell family, inventory report §4 row 10). Recorded; no cap added.
- **Order**: land early in the chunk (independent of the walk-grammar items); the
  mount comment "The anchor-width law preserved" is rewritten to state the new law.

### 1.3 #1034 — subsume the ControlNext Tab re-injection under the stop ladder

- **Seams**: `crates/taskboard-ui-slint/src/adapter.rs` — the post-table tail at
  ~:5085–:5111 (the `matches!(ControlNext|ControlPrev) || self.directional_control_step(...)`
  gate, with the **duplicated** `directional_control_step` call inside computing
  `backwards` a second time), `directional_control_step` at ~:5483, and
  `dispatch_traversal` at ~:5323.
- **Shape**: one decision function, one call site. Introduce
  `fn control_walk_step(&self, state, effect) -> Option<bool>` that unifies the
  backwards-decision over the ladder:
  ```rust
  match effect {
      UiEffect::ControlNext => Some(false),
      UiEffect::ControlPrev => Some(true),
      UiEffect::FocusDirectional(_) => self.directional_control_step(state, effect),
      _ => None,
  }
  ```
  and the tail becomes `if let Some(backwards) = self.control_walk_step(&state, effect)`
  — the mask clamp vs `dispatch_traversal` split inside the `if` stays. This kills the
  double-call, the `matches!`/`unwrap_or_else` re-derivation, and makes Tab
  literally an arm of the same decision the directional table interprets (the issue's
  "special case of the same stop ladder"), while `directional_control_step` remains
  the per-view table it already is.
- **Decision (the interpret table's mask arms stay)**: the
  `(masked-flows, ControlNext|ControlPrev) => Vec::new()` rows (~:5048) are
  conflict-resolution cells — they suppress the resolver's settle so the tail's clamp
  consumes the stroke — and are pinned cell-by-cell by the pair-table test. Collapsing
  them would change the resolver's totality contract. They stay; the walk-decision
  half is what collapses. Recorded as the issue's scope reading.
- **Evidence**: behavior-identical by construction (the unified fn returns exactly
  what the old disjunction computed for every (effect, view) pair — ControlNext/Prev
  always walked, and `directional_control_step` returns `None` for non-directional
  effects, so no pair flips). The existing focus/walk suite
  (`walk_probe_tests.rs`, `focus_seam_probe_tests.rs`, `flow_table_tests.rs`'s
  pair-table pin) is the oracle.

### 1.4 #1065 — the FocusScope/Return-Space grammar across centralized widgets

- **Seams**: `ui/widgets/segmented-control.slint:78` (`seg-focus`, radio-button role +
  modifier-guarded Return/Space), `ui/compositions/board-bar.slint:269` (`tab`,
  tab-list/tab roles, no key handler), `ui/compositions/board-layout.slint:878`
  (`segment`, tab role + DropArea wrapper, no key handler). W3 already landed
  (`widgets/text-button.slint` on main), so the four bespoke text buttons are gone —
  these three are the remainder the issue names. Gold-standard base:
  `widgets/activation-stop.slint` (C class; its comment records the runtime
  constraint **`accessible-role` rejects property bindings**).
- **Shape (decision)**: a new internal widget file `ui/widgets/focus-stops.slint`
  exporting two thin components, both `inherits FocusScope`, sharing the tab-stop +
  a11y skeleton:
  - `RadioStop` — `accessible-role: radio-button` (static, per the constraint),
    `stop-label`, `stop-checked`, `stop-enabled`, `activated()`; carries the
    modifier-guarded Return/Space body (verbatim from `seg-focus`, including the
    disabled-stroke-absorbs rule, r2 D32);
  - `TabStop` — `accessible-role: tab` (static), `stop-label`; `focus-on-tab-navigation`
    + a11y label only, **no key handler** (the two tab-strip cousins have none today —
    adding one would be a behavior change; the keyboard switch path is the action
    layer's selection-follows-focus, D8/D15).
  Consumers instantiate: segmented's `seg-focus` → `RadioStop` (role/label/checked/
  enabled passed as properties; the `picked` gate moves into the `activated` closure —
  behavior-identical since the component re-raises only through the same guarded body);
  board-bar's `tab` and board-layout's `segment` → `TabStop` (+ their paint/host
  wrappers untouched). `ActivationStop` (button role, EditingFacts latch) and
  `IconButton` (class-C gold standard) stay untouched per the issue.
- **Behavior identity**: keyboard grammar, roles, labels, checked state, and focus
  routing are property-for-property the same; the corpus (segment/board-bar probes:
  `board_bar_probe_tests.rs`, the widget framework probe, the walk suite) is the
  oracle. Slint static-role risk: if a further role variant is ever needed it is a new
  component in the same file (codegen-free, rule E satisfied — three centralized
  consumers now justify the shared file).
- **Order vs #1034**: no shared file (adapter.rs vs .slint), so either order compiles;
  **land #1034 first** anyway so the walk-suite run that validates it also validates
  untouched segment behavior before #1065 churns the segment files (one fewer bisect
  surface if a probe moves).

### 1.5 #1027 — pin the overlay-exclusivity fold row in `om_us2_s1`

- **Seams**: `tests/probes/overlay_migration_probe_tests.rs` (the om catalogue; the
  `om_us2_s1` fixture at :190, tests at :214/:240); the enforcement seam is
  adapter-side — `begin_date_edit`'s settle funnel (`adapter.rs:4654ff`, bind blocks
  at :13323ff); the indirect cover today is `journeys.rs:16033` ("the toggle's close
  arm closes the picker").
- **Shape (decision)**: a new named test in the om catalogue,
  `om_us2_s1_arming_the_editor_closes_the_picker_and_vice_versa`, using the existing
  `om_us2_s1_fixture` + `run_probe`: arm the picker, call the editor-bind seam
  (`begin_date_edit` via the probe's bound callback, Due field), assert the typed
  root seams — the picker's facts open-flag reads false and the editor's reads true;
  then the mirror leg (editor armed → pick seam → editor closed, picker open). Typed
  outcomes only, no text/clock assertions; reuses the catalogue's
  `advance_probe_clock` idiom if the funnel needs a tick.
- **Evidence**: the promised invariant gains its named pin; the debt from
  `2026-11-18-overlay-migration-plan.md` §5 risk 3 is retired. Test-only — zero
  production diff. Land **last** (pure addition; cannot conflict).

## 2. Execution order & commits

One commit per item, in this order (rationale: smallest-first warm-up; the two
walk-grammar-adjacent items sequenced per §1.4; test-only last):

1. `refactor(ui): switch_plan_pane states the two-pane wrap directly (#1060)`
2. `feat(ui): dropdown flyouts take the R-6 menu width floor (#978)` — the sanctioned
   expectation change; probe pins updated in the same commit
3. `refactor(ui): the Tab re-injection joins the directional stop ladder (#1034)`
4. `refactor(ui): the segment focus stops share one grammar file (#1065)`
5. `test(ui): the overlay-exclusivity fold row om_us2_s1 promised (#1027)`

Then `docs(plan)`-free: this plan file is committed first as
`docs(plan): the ui small grammar bundle chunk plan` (stage-1 scope).

## 3. Risks

- **#978** is user-visible (flyouts widen to the 200·scale floor when the anchor is
  narrower) and touches the probe's verbatim mount copy — the only sanctioned
  expectation change; if a snapshot/golden pins the old panel width it is regenerated
  with the diagnosis in the commit body.
- **#1034** sits inside the interpret-table tail; the pair-table pin and walk probes
  must stay green untouched — any movement there means the unified fn changed a pair,
  i.e. a bug, not a pin to update.
- **#1065** risks Slint static-`accessible-role` friction (the recorded constraint);
  the shape above keeps roles static per component, so it cannot bite — but if Slint
  rejects the two-component file for another reason (export plumbing), the fallback is
  inlining `RadioStop`/`TabStop` as private components in each consumer file and
  recording the partial dedup in a comment + a backlog note.
- **#1027** needs the probe to drive `begin_date_edit` through the bound callback —
  if the settle funnel needs an event-loop tick the catalogue's clock idiom covers it;
  no new production seam may be added (if one seems required, that is an out-of-scope
  finding → backlog issue, and the pin lands against the observable facts instead).
- Box load: the gate runs flock-wrapped per the standing law; commit each item before
  any wait.

## 4. Estimated diff

| Item | Files | Δ lines |
|---|---|---|
| #1060 | view_state.rs | ~20 (net −8) |
| #978 | app-window.slint, anchored-overlay.slint, dropdown_probe.slint, probe tests | ~60 |
| #1034 | adapter.rs | ~35 (net −15) |
| #1065 | + focus-stops.slint, segmented-control.slint, board-bar.slint, board-layout.slint | ~180 (net −40) |
| #1027 | overlay_migration_probe_tests.rs | ~70 |
| **total** | ~9 files | **~365 (net ~+230 incl. the new widget file)** |

## 5. Out-of-scope findings → backlog

None discovered in planning that is not already tracked: the mask-arm collapse
remainder (#1034) is recorded as a kept cell family, not deferred work; the unused
`menu-max-width` rides the future MenuShell family (inventory row 10). Backlog issues
opened by this stage: none.
