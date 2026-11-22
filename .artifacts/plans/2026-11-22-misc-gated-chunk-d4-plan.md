# Chunk D4 — the misc gated implementations (#1225, #1234, #1151, #1152, #1184, #1104) — `feat/misc-gated-impl`

- **Date**: 2026-11-22
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-22-misc-gated-journeys-ux-plan.md`
  (US-21..US-26; W-1..W-5; R-1..R-5). Round-5 dispatch plan chunk D4
  (`.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §8). Authorities: the round-4 small-gated plans (the landed base),
  the K4 cursor law (`.artifacts/plans/2026-11-19-row-cursor-walk-plan.md`
  §9; R-9's one-mechanism rule), the v1 undo line (ADR 0019 posture,
  `undo.rs`'s `CaptureTask` inversion), the G6 geometry law and menu
  T-1 curation, J5 R-6 / menus research §4.3.3.
- **Status**: DRAFT — GATE-PAUSED on the journey plan's operator
  acceptance (stage 2 opens "GATE CLEARED: the operator accepted the
  journey — implement exactly its wiring rows").
- **Issues**: #1225, #1234, #1151, #1152, #1184, #1104.

## 1. Scope

1. **#1151 capture accidental-commit recovery (US-21)** — the recovery
   already exists (`CaptureTask` is journaled; undo deletes the
   created task). D4 ships the pin, not the machinery: journey test
   `us21_s3_capture_commit_undo_recovers` through the landed seams
   (W-1), plus the R-1 ruling comment on #1151 (recovery = the v1 undo
   line; no confirm-stage).
2. **#1234 embedded planner cursor (US-22)** — amend the
   `Flow::ReviewPlanner` flow-table vertical arms to route to
   `adapter.rs::walk_plan_cursor` (W-2): the shared K4
   `view_state.rs::plan_cursor` state, armed styling and clamp/no-wrap
   exactly as the standalone planner (US-15/W-16). No new cursor
   mechanism, no new ActionId, no new command; activate stays the
   cursor'd row's own `PlanBeginPlace` seam; exit clears pane-scoped.
   Desktop-only (R-5 — the walk is kiosk-refused already).
3. **#1152 kiosk inventory pins (US-23)** — two structural pins (W-3):
   the degraded badge renders on the kiosk profile
   (`us23_s1_degraded_badge_renders_in_kiosk`); the `AppSettings` menu
   row is present and enabled on the kiosk chain
   (`us23_s2_settings_row_present_on_kiosk_chain` over
   `menu.rs::root_page`). No surface change — the inventory finding is
   that both lanes already have kiosk-visible homes.
4. **#1225 reload-lane placement (US-24)** — docs-only (W-5): a dated
   erratum note on the round-4
   config-files-signals plan's W-5 surface column recording R-2
   (whole-view caption normative); the R-2 ruling comment on #1225.
   No UI move.
5. **#1184 quick-row budget (US-25)** — the R-3 decision lands as: the
   registry-wide guard test
   `us25_s1_root_quick_rows_within_budget` in `menu.rs`'s tests (the
   `menu_group: None` set is exactly the curated five, T-1 order) plus
   the decision comment on #1184. Option (a) (widget scroll/cap) is
   rejected; Export… stays on the root page (it is one of the landed
   five; G6 passes with five — the #1184 "≤4" sketch is superseded by
   the measured budget).
6. **#1104 labels multi-toggle dropdown (US-26)** — DESIGN-ONLY: the
   journey plan §US-26 + §5 state diagram is the design record; D4
   links it from #1104 and queues the implementation as the follow-up.
   **No dropdown widget change, no labels-overlay change, no wiring
   rows, no tests for this item** — a dropdown diff in the D4 branch
   is a scope breach (dispatch-plan trap (c)).

## 2. Work items (journey-id mapped)

- W-1 (#1151): new journey member
  `crates/taskboard-ui-slint/tests/journeys/misc_gated_journeys.rs`;
  `us21_s3_capture_commit_undo_recovers` (+ the redo re-check arm).
- W-2 (#1234): `adapter.rs` `Flow::ReviewPlanner` vertical arms →
  `walk_plan_cursor`; amended `flow_table_tests.rs` rows;
  `us22_s1_embedded_cursor_arms`, `us22_s2_embedded_cursor_walks_clamped`,
  `us22_s3_embedded_activate_uses_own_seam`,
  `us22_s4_walk_exit_clears_cursor`.
- W-3 (#1152): kiosk-profile structural pins
  `us23_s1_degraded_badge_renders_in_kiosk`,
  `us23_s2_settings_row_present_on_kiosk_chain`.
- W-4 (#1184): `menu.rs` tests `us25_s1_root_quick_rows_within_budget`
  (registry sweep, broader than the board-chain T-1 pin).
- W-5 (#1225): the dated erratum in
  `2026-11-21-small-gated-journeys-config-files-signals.md`.
- Issue bookkeeping: R-1 comment on #1151, R-2 comment on #1225, R-3
  decision comment on #1184, design link on #1104 (which stays open —
  it closes only when its implementation chunk lands). Census: the
  `plan.begin_place` row gains the walk-context extension note and the
  R-5 platform qualification.
- Zero new `StateCommand`/`UiRequest`/`ActionId` variants (journey
  plan §7's empty delta is the contract — an engine diff here is a
  finding).

## 3. Traps (from the dispatch plan + the journey plan's rulings)

- **(a) #1151's recovery is the R-4 flip's counterpart** — an
  accidental commit MUST be recoverable. The journey plan's answer is
  the undo line (R-1): if the pin proves undo does NOT reach the
  capture-created task, that is a real bug — fix it on the branch
  (do not weaken the test, do not add a second mechanism).
- **(b) #1234's cursor arms the K4 state — no second mechanism** —
  the routing must land in the flow table's ReviewPlanner arms; a
  parallel cursor bit, a separate walk state, or a re-derived clamp is
  a finding (R-9 / J4 plan R-9's law).
- **(c) #1104 is design-only** — the plan records the design; the
  implementation queues as the follow-up issue; any dropdown or
  labels-overlay code in this branch is out of scope.
- The guard test must sweep the whole registry (not just the board
  chain): the budget law's teeth are registry-wide; a per-chain pin
  would let a future group-less registration slip through on another
  context.
- The kiosk pins must assert typed/structural facts (a property value,
  a registry row's presence), never badge/caption text (AGENTS §5).
- The one-behavior-one-test law: US-21's recovery pin is the journey
  layer's routing test; the undo inversion of `CaptureTask` itself is
  already pinned in `taskboard-domain`'s undo tests — do not
  re-test it through the engine.

## 4. Acceptance

- The journey plan's §8 verification list lands green: the new journey
  member, the amended flow-table rows, the two kiosk pins, the menu
  guard test.
- `scripts/ux-coverage.sh <journey-plan> --gate` exits 0 post-landing
  (pre-landing it exits 1 on the MISSING test rows — expected and
  recorded).
- G6 stays green unchanged; T-1 stays green unchanged.
- The docs erratum + the four issue comments (R-1/R-2/R-3/the #1104
  design link) are in place; #1225/#1152/#1184 closable at stage-3
  acceptance (the dispatcher owns at-merge closures).
- Stage 3 runs both mandates (code-quality + UX completeness incl.
  the walkthrough over US-21/US-22) per the dispatch plan.

## 5. Stage shape

Standard journey-gated chunk (dispatch plan §1's pattern, the round-4
HF1 worked example): stage 1 planning is THIS plan + the accepted
journey plan (landed on `docs/misc-gated-journeys` by the J4 agent);
stage 2 opens on "GATE CLEARED" and implements W-1..W-5 exactly; stage
3 carries both mandates and opens the PR (body references all six
issues; closures are the dispatcher's).

Estimated size: S — one routing amendment, four test additions in
three files, one docs erratum, issue comments.
