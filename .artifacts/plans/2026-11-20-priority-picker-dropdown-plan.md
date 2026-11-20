# [Plan] Priority picker dropdown — round 9 chunk 15 (M2, J5 R-5)

- **Date**: 2026-11-20
- **Kind**: stage-1 implementation plan (TDD-first) — dropdown de-siting
  (self-registering mount) + priority picker adoption; behavior-preserving
  except where a recorded reason and a new test say otherwise.
- **Origin** (normative): round plan
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §1 O-6
  (widget-centralization mandate) + O-9 (menu rulings — "priority picker
  becomes a dropdown; time combobox and labels pickers stay bespoke");
  dispatch plan `2026-11-18-gui-review-round9-dispatch-plan.md` §1 (chunk
  15 row) + §14 (centralization-wave law: each stage-1 plan records the
  absorption map before any code); menus research
  `.artifacts/reports/2026-11-18-round9-menus-controls-research.md` §4
  (dropdown inventory) + §4.3.4 (the de-siting proposal);
  `2026-11-18-menus-remainder-ux-plan.md` (J5) R-5/R-6. Artifact-note:
  the round plan, dispatch plan, and menus research live on the local
  branch `chore/artefact-sweep-2026-11-19` (commit `73a9e09e`, PR not
  yet merged); all citations read from that commit.
- **Preconditions verified 2026-11-20**: chunk 5 (PR #1025), chunk 13
  (PR #1089), chunk 14 (PR #1102) all MERGED; worktree doctor exit 0
  (162G free / 13.7G available); worktree
  `../taskboard-WT-refactor-ui-priority-picker-dropdown` created with
  `-b refactor/ui-priority-picker-dropdown` off `origin/main`
  (`54996e9b`).
- **Status**: READY for stage 2.

## 1. Decisions (all settled; no open questions)

| ID | Decision | Reason |
|---|---|---|
| D-1 | The `site: int` discriminator dies. `DropdownFacts` becomes the self-registering mount: the widget pushes `rows` (its own model), its anchor facts, and assigns the class-global's intent callbacks (`activated`/`context`/`hovered`/`unhovered`/`dismissed`) — the `MenuPanelFacts` precedent (research §4.3.4's named direction: "the widget declares its flyout window-side from its own facts, as `ActionMenu` already does … without a site enum") | kills the per-consumer app-window arm law; new consumers need zero app-window edits (the chunk's entire payoff) |
| D-2 | ALL FOUR existing sites convert (0 settings board picker, 1 board-bar switcher, 2 board-bar strip twin, 3 archived roster); the four per-site `DropdownFlyout` mount arms and the dedicated `dropdown-backdrop` in `app-window.slint:1150-1270` collapse into ONE generic mount | the enum + per-site arms are exactly the coupling the chunk deletes; half-measures keep the enum alive |
| D-3 | The single generic flyout mount rides chunk 3's `AnchoredOverlayHost` (the date-picker/timer-setter precedent at `app-window.slint:853/890`) instead of preserving the four hand clamp/flip expressions inside one arm | deduplicates the last hand clamp/flip family; consistent with W1's law; shrinks chunk 17's (#954) absorption map by one family — census note lands here, #954 gets a comment at PR-open |
| D-4 | The priority picker adopts the widget class as **two embedded `Dropdown` fields** (importance, urgency) hosted in the existing `LabeledValueRow` display rows' content slot (the effort-row hosting precedent, `task-view.slint` effort-row): the echo text becomes the closed field's `field-text`, the pencil `IconButton` retires, the field's own click/Return opens; the six-slot `PrioritySlotRow` quick-list and the in-layout expanding picker block + `priority-catch` underlay in `task-view.slint:781-840` are deleted | the Dropdown's closed field is the echo+pencil grammar already (label + value + affordance in one stop); hosting in the LabeledValueRow keeps chunk 14's row surface (caption, label, walk identity) intact |
| D-5 | The priority-slot check glyph **stays** (chunk 13's stay list): the exact `Path` (`M 2 6 L 5 9 L 10 3`, `Semantic.success`) renders on the selected row in `DropdownFlyout` — a class-wide rendering law (selected ⇒ emphasized text + check glyph), no per-row flag | no speculative generality (no `check` bool nobody turns off); the "stay" ruling is honored at the rendering layer; the other four sites' selected rows gain the glyph — visual delta, re-baselined below |
| D-6 | Close-routing: the `Dropdown` gains one `callback dismissed()` invoked by every internal close (Escape, Tab, walk-away, facts-close, flyout row-activated close does NOT count — it selects first); only the priority mounts wire it (→ `priority-dismiss-requested()`); the four legacy sites leave it unassigned (Slint unassigned callbacks are no-ops) | today's Escape path closes through the action layer (`CancelMove` arm, `adapter.rs:4169`) and must keep recording the origin + restoring focus (journey `us1_s1_picker_close_restores_the_invoking_field`); a widget-internal silent close would strand the adapter's `priority_open` |
| D-7 | Keyboard decline: `keyscope::LadderGlobal` gains a `TaskDetailPriority` arm reading `DropdownFacts.open` (the selector ladders' stroke set: arrows, Escape/Return, C-n/C-p) | today's bespoke picker needs no decline (no ladder); the adopted widget's ladder dies at the single key path without it (the recorded `keyscope.rs:245` seam) |
| D-8 | Single-open global semantics preserved: one `DropdownFacts.open` — two dropdowns cannot coexist statefully (research §4.3.2 "fine today"); the importance/urgency exclusivity is already the adapter's (`priority_open: Option<PriorityField>`) | behavior-preserving; the limitation is homed as a backlog issue (§9) |
| D-9 | Dispatch identity: the seams stay byte-identical — `task_importance_edit_asked`/`task_urgency_edit_asked` route `TaskImportanceEdit`/`TaskUrgencyEdit`; slot activation routes `priority_slot_clicked(index)` (the adapter resolves the level from the row index, `adapter.rs:12710`); dismissal routes `priority_dismiss_requested()` | the behavior-preservation bar; `UpdateTask` masks unchanged |
| D-10 | What stays bespoke, and why (J5 R-5, recorded): the **time combobox** (`TimeField`/`QuickListFlyout` — typed filtering, nearest-row seek, its own ladder: converging is optional per research §4.2) and the **labels picker** (chip toggles — multi-select semantics the widget lacks; J5 R-6 defers the design to a dedicated multi-label-usability round) | the operator ruling verbatim; no conversion pressure this chunk |
| D-11 | Visual deltas and their re-baselines: (a) selected flyout rows gain the check glyph (D-5); (b) the detail view's importance/urgency rows swap pencil→closed-field chevron (D-4); (c) the in-layout expanding picker block disappears (no more layout shift — the overlay law's gain). Affected goldens re-baselined with this recorded reason; any pixel/probe assertion reading the deleted elements is rewritten, never weakened | §14's law: deviations only with a recorded reason + a new test |
| D-12 | Census writes use symbol anchors per #775: `docs/ux-surface-census.org`'s priority picker rows (the picker family, rows 5/14) and the dropdown rows (`board.select`, `board.archive`) gain dated supersession notes with `file::symbol` anchors; `docs/widget_interactions.org`'s dropdown section records the de-sited mount law + the `dismissed()` callback contract | round-plan §6 census law |

## 2. The absorption map (§14 law: before any code)

| Instance | Converts? | Onto | Why |
|---|---|---|---|
| Settings remote picker flyout (site 0, `RemotePicker`) | YES | self-registering `DropdownFacts` + the one `AnchoredOverlayHost` flyout mount | enum deletion |
| Settings publish picker flyout (site 2, `DeckPublishPicker`) | YES | same | enum deletion |
| Board-bar switcher flyout (site 1) | YES | same | enum deletion |
| Archived roster flyout (site 3) | YES | same | enum deletion |
| Task-detail importance picker (bespoke `PrioritySlotRow` block) | YES | `Dropdown` field in the `LabeledValueRow` + the flyout | J5 R-5 |
| Task-detail urgency picker (same block) | YES | same | J5 R-5 |
| Time combobox (`TimeField`/`QuickListFlyout`, 4 sites) | NO — stays | — | typed filter + nearest-row seek ladder; converging optional (research §4.2) |
| Labels picker (chip toggles) | NO — stays | — | multi-select semantics the widget lacks; R-6 defers the design |
| Recur-editor weekday pick (`SegmentedControl`) | NO — stays | — | the recorded round-7 D-7.1 deviation stands |

## 3. The self-registering mount — concrete shape

`DropdownFacts` (in `dropdown.slint`) after the chunk:

```slint
export global DropdownFacts {
    in-out property <bool> open: false;          // unchanged
    // site: DELETED. The opener's identity lives in its callbacks.
    in-out property <[DropdownRow]> rows;         // NEW: pushed at open
    in-out property <length> anchor-*;            // unchanged (4)
    in-out property <length> panel-x/panel-y;     // unchanged (observation seams)
    in-out property <int> highlight/seek-nonce;   // unchanged
    callback activated(int);                      // NEW: was per-site row-activated arms
    callback context(int, int, int);              // NEW (optional arm — no-op unless assigned)
    callback hovered(int, length, length, length, length);  // NEW
    callback unhovered();                         // NEW
    callback dismissed();                         // NEW (D-6)
    // indicator-kind / tone-color functions unchanged
}
```

The `Dropdown` widget assigns its own handlers into the global at
declaration time (Slint globals accept per-component callback
assignment — the stage-2 spike proves this first, T-0 below) and pushes
`rows` in the existing `changed open-mirrored` hook beside the anchor
facts. The window-side mount becomes:

```slint
if (DropdownFacts.open): AnchoredOverlayHost {
    open: true;
    anchor-*: DropdownFacts.anchor-*;             // the click-time mirror idiom
    viewport-width: root.width;  viewport-floor: status.y;
    panel-preferred-width: DropdownFacts.anchor-width;   // anchor-width law preserved
    walk-stops: 1;
    dismissed => DropdownFacts.dismissed();       // → the opener's close seam
    changed panel-x => DropdownFacts.panel-x = self.panel-x;
    changed panel-y => DropdownFacts.panel-y = self.panel-y;
    DropdownFlyout { rows: DropdownFacts.rows; }  // callbacks forward to the facts global
}
```

The backdrop dies (the host's dismissal layer is it). Sibling-order
grouping follows the overlay group's class law (dropdowns above the
date-picker/timer-setter/track-entries overlays, below the menu group —
the existing comment's placement contract is preserved by mounting at
the same tree position).

Spike-gate (T-0): if per-component callback assignment into a global
proves broken on Slint 1.18.1, the recorded fallback is a
`DropdownFacts.kind` consumer enum + ONE forwarding arm in the generic
mount (still no per-site arms in app-window; still enum-of-consumers,
not int sites). Spike outcome recorded in the stage-2 report.

## 4. TDD-first build order (each step: test → code → green)

1. **T-0 spike (probe test first)**: a `dropdown_probe.slint` case
   proving global-callback self-registration (widget assigns, window
   mount forwards, the receiving Rust-side handler fires per instance).
2. **T-1 de-siting probe tests** (extend `probes/dropdown_probe_tests.rs`):
   flyout renders the opener's pushed rows; two instances (settings +
   board bar) each route activation through their own seam (identity
   preservation); `DropdownFacts.site`/`flyout-site` gone (compile-time).
   Existing geometry/flip/clamp + keep-alive tests re-pass unchanged
   (the facts mirrors survive verbatim — D-3 keeps `panel-x/y` seams).
3. **T-2 priority journeys** (extend `tests/journeys.rs`; step tokens
   per the J5 addendum, §7):
   - `us6_s1`: `TaskImportanceEdit` opens the flyout; slot 4 click
     dispatches the exact `UpdateTask` importance mask (D-9 — the
     flow-table tests' oracle, now through the widget).
   - `us6_s2`: Escape closes, the origin fact records, the invoking
     cell re-focuses — `us1_s1_picker_close_restores_the_invoking_field`
     re-passes UNCHANGED (the strongest preservation oracle).
   - `us6_s3`: the ladder — arrows/C-n/C-p move the highlight
     (`DropdownLadder.moves` seam), Return selects, Escape closes only
     (the selector-ladder grammar, now on the detail view; the
     `keyscope` decline arm D-7 proves the keys reach the field).
   - `us6_s4`: urgency mirror-symmetry (pick → `urgency` mask).
4. **T-3 check-glyph probe test**: the flyout's selected row renders
   the check `Path` (structural probe: element existence + stroke
   color, never text).
5. **Implementation order**: `dropdown.slint` (de-site) →
   `dropdown-flyout.slint` (glyph + facts callbacks) → `app-window.slint`
   (one mount) → `keyscope.rs` (decline arm) → `task-view.slint`
   (priority conversion) → `adapter.rs` (wire `dismissed`, open seams
   onto `open-requested`) → goldens re-baseline.
6. **Existing nets that must re-pass unchanged** (behavior-preservation
   manifest): `journeys.rs::us1_s1_picker_close_restores_the_invoking_field`;
   the priority `flow_table_tests.rs` block (`:4613-4769` — slot→mask
   identity, stale-click guard, dismiss-without-dispatch); the
   zero-row gate journey (`us1_ac1_zero_row_open_mounts_nothing` — the
   #753 guard rides `field-enabled && rows.length > 0`, untouched); the
   #794 `open-requested` board-select journey; `dropdown_probe_tests.rs`
   geometry/ladder suites; `us1_ac4_escape_ladder_steps_submenu_then_menu`
   (the escape ladder class).

## 5. J5 addendum (the dispatch plan's "W-x addendum")

A dated addendum section lands in
`.artifacts/plans/2026-11-18-menus-remainder-ux-plan.md` (the
one-line-amendment pattern the research §5.2 sanctioned; the dispatch
plan's UX-coverage column names exactly this addendum):

- Journey specs: `US-6 — the priority pickers are dropdowns`
  (`US-6/S1` open+pick dispatches; `/S2` escape restore; `/S3` ladder;
  `/S4` urgency mirror).
- Wiring inventory rows `W-6` (de-sited facts mount,
  `dropdown.slint::Dropdown`, test `probes/dropdown_probe_tests.rs`)
  and `W-7` (priority adoption, `task-view.slint` +
  `adapter.rs::on_priority_slot_clicked`, test
  `journeys.rs::us6_s1_...`).
- No command-contract delta (D-9: the masks are byte-identical).

Stage 2 runs `scripts/ux-coverage.sh <J5 plan> --gate` and stage 3
re-runs it + the walkthrough protocol over US-6 (P2; the four
questions + six heuristics over the detail rows and the flyout).

## 6. Out of scope (recorded refusals)

- Time combobox convergence, labels picker conversion (D-10).
- Multi-toggle dropdown semantics (J5 R-6; homed as a new backlog
  issue, §9).
- Two-dropdowns-coexist stateful open (D-8; backlog issue, §9).
- `#978` flyout width tokens: out of scope, but D-3's single mount is
  the natural seam for it — a comment on #978 at PR-open.
- Chunk 17's remaining #954 absorption (the twin pickers etc.) — only
  the dropdown family leaves its map.

## 7. Verification

- Pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run
  --workspace` (the subagent gate; the dispatcher owns the full §10
  trio per the dispatch discipline).
- `scripts/ux-coverage.sh <J5> --gate` at stage-2 self-check; stage 3
  re-runs + US-6 walkthrough.
- Goldens: `cargo nextest` golden suites re-baselined only for the
  recorded D-11 deltas; every other golden byte-identical.
- Census + `docs/widget_interactions.org` updates land in the chunk
  (round-plan §6).

## 8. Risks

| Risk | Mitigation |
|---|---|
| Global callback self-registration semantics on Slint 1.18.1 (assignment lifetime, re-assignment ordering with two instances) | T-0 spike first; the D-3-recorded fallback (consumer-kind enum forwarding) keeps the chunk's shape |
| The `changed absolute-position` keep-alive idiom vs the host's binding-based anchor intake (`anchored-overlay.slint` header warns the host does NOT depend on changed-hooks for the initial pass) | the widget's existing keep-alive mirrors stay; the host intake is ordinary property binding — stage 2 verifies the settings `Flickable` scroll journey (the F14/F9 regression net) explicitly |
| Ladder decline widening changes escape behavior for other TaskDetail surfaces | the decline arm is gated on `DropdownFacts.open` (true only while a flyout holds) — no open flyout, no decline; the escape-ladder journey suite re-passes |
| Focus restore contract: the widget's `close()` refocuses ITS field, but the journey demands the title cell re-focuses | D-6: the priority mount's `dismissed` → `priority-dismiss-requested()` runs the adapter's existing restore routing (`PriorityDue` seam facts); stage 2 keeps `us1_s1_...` as the unchanged oracle and wires `dismissed` to bypass the field-refocus when a consumer seam is assigned (a one-branch conditional, documented at the callback) |
| Golden churn beyond D-11 | re-baseline strictly per the recorded deltas; any other diff is a finding |

## 9. Backlog issues (standing authorization)

Opened at planning time (numbers listed in the stage-1 report): the
labels multi-toggle dropdown design (J5 R-6's home) and the single-open
`DropdownFacts` coexistence limitation (research §4.3.2). #762 (the
flyout-site coupling) is CLOSED by this chunk — noted in the plan, the
closure lands at merge per dispatch discipline.

## 10. Diff estimate

~600–750 lines across: `dropdown.slint` (~80), `dropdown-flyout.slint`
(~40), `app-window.slint` (net ≈ −100), `task-view.slint` (net ≈ −60),
`adapter.rs` (~+40), `keyscope.rs` (~+15), probe tests (~+250),
journeys (~+150), J5 addendum + census + widget_interactions.org
(~+80), goldens (re-baselined binaries, uncounted).
