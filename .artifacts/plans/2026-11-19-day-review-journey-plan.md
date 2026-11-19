# [Plan] Chunk 7 — the Guided Day-Review Journey (J4, `feat/ui-day-review-journey`)

- **Date**: 2026-11-19
- **Kind**: stage-1 implementation plan (TDD-first) for round-9 chunk 7.
- **Normative inputs**: UX journey plan J4
  (`.artifacts/plans/2026-11-18-day-review-ux-plan.md`, ACCEPTED —
  rulings R-1..R-3 and stories US-1..US-4 are the contract), round
  plan §1 O-3, round-9 dispatch plan §8, research
  `.artifacts/reports/2026-11-18-round9-dayreview-details-research.md`
  §1/§3, issue #716 (the embedded-child integration contract).
- **Preconditions verified**: chunks 3 (PR #1005, merged 2026-11-18)
  and 5 (PR #1025, merged 2026-11-19) are on `origin/main`
  (`ModalDialogHost` at `ui/widgets/modal-dialog-host.slint`,
  `AnchoredOverlayHost`, `Theme.scrim`/dialog tokens, the migrated
  overlay mounts, the always-on Effort field, and the walk
  insert-mode posture — `interpret_walk`'s Back rung disengages
  Insert before the ladder, `adapter.rs::interpret_walk`). Chunk 9
  (directional focus, `feat/ui-directional-focus`) is **NOT** on
  main — per the dispatch prompt its ids are designed *for*, never
  bound (Decision D-7).
- **Worktree**: `../taskboard-WT-feat-ui-day-review-journey` off
  `origin/main` (`b42d5526`); `scripts/worktree-doctor.sh` exited 0
  (181G free / 16.5G avail — both above floors).
- **Branch**: `feat/ui-day-review-journey` (created with `-b`).
- **Crate**: everything lands in `crates/taskboard-ui-slint` (AGPL
  header, `#![forbid(unsafe_code)]` already crate-wide). No domain
  or engine changes — every command dispatched is an existing
  `StateCommand` (`MoveTask`, `UpdateTask`, `RecordReflection`).

## 0. The contract in one paragraph

The review walk stops being a shape-B handoff and becomes a 6-step
guided journey inside one `ModalDialogHost` modal: Entry → Inbox →
Carry → Planner (embedded, opt-in per J4's flow graph) → Prompts →
Completion summary. Empty stages auto-skip (Inbox *and* Carry — the
"Move to tomorrow" inert-verb bug dies structurally). Escape/outside
click arms an abort-with-confirm (R-2), never a silent step-back.
The walk always has a visible completion path. The old
`hand_off_to_planner` / `walk_resume` shape-B machinery is deleted.

## 1. Recorded decisions

### D-1 — Stage machine shape (view_state)

`WalkStage` grows to five machine stages; `Flow::ReviewWalk` gains
an optional embedded-planner marker via the chain (D-2), not a
stage field:

```
enum WalkStage { Entry, Inbox, CarryForward, Planner, Prompts, Complete }
```

- **Entry**: informational stage (header + stage-intro sentence +
  the strip). Advanced by the single Next control. Never skipped
  (it is the guidance the operator asked for); opened-to by
  `open_review_walk` unconditionally.
- **Inbox / CarryForward**: row stages, cursor semantics unchanged
  (`review_walk_cursor`, `review_walk_next_row`).
- **Planner**: reached only by the user choosing plan-next-day out
  of Carry (J4's flow graph: `Carry → Planner: choose plan`;
  `Carry → Prompts: skip`). Never auto-advanced into, never
  auto-skipped — while the stage is live the *embedded child
  level* (D-2) is mounted; leaving the child advances the stage to
  Prompts. `back_review_walk` from Planner returns to Carry
  (US-3/S1: "back returns to carry") by ending the child
  `Cancelled`, keeping the walk open.
- **Prompts**: unchanged verbs (`Record` / `Finish`), but both now
  advance to **Complete** instead of closing the walk (P12's
  "Finish without recording" stays guilt-free — Complete presents
  `recorded: no` honestly).
- **Complete**: terminal presentation stage; its single exit
  (`ReviewWalkComplete`) closes the walk and restores focus (the
  D15 hand-off already in `apply_outcomes`).

Machine transitions live in `view_state.rs` beside the existing
walk methods; the planner legible-order rule is one function:

```rust
/// The first non-empty stage at/after `from` (D-3's skip law).
fn walk_first_open_stage(from: WalkStage, rows: WalkRows) -> WalkStage
```

### D-2 — Embedded planner mount contract (issue #716's shape)

The chain's kind rule (`surfaces.rs::Surfaces::push` refusing
`AlreadyInChain` per *exact* `SurfaceKind`) compares
`SurfaceKind::View(view)` per-`ViewId`, so
`Surface::View(ViewId::PlanNextDay)` may be pushed as a **child
level above the live `Flow::ReviewWalk`** — exactly #716's
"planner level above the live walk instead of close-reopen".
Verified against the chain minting law (D-H7/Q6). Consequences:

- The planner child is minted by a new `view_state` entry
  (`walk_open_planner()`), gated on
  `stage == WalkStage::Planner` and refusing when the kind is
  already live (the defensive D7 no-op).
- The standalone route (`UiEffect::OpenPlanNextDay` →
  `open_plan_next_day()`, `view_state.rs::open_plan_next_day`) is
  untouched — it still swaps the root view for direct entry
  (non-goal: no removal).
- Child exit contract: `end_top_flow_with(.., Completed(None))`
  (or `Cancelled` from Escape/back) → the walk parent's
  `on_child_exit` gains a `View(PlanNextDay)`-context arm that
  advances `Planner → Prompts` and refreshes. Placed tasks arrive
  as plain `UpdateTask` writes in the published snapshot, exactly
  as the old resume path's comment promised.
- The UI discriminates the leg by the *stage kind*: while
  `stage-kind == Planner` (a new `ReviewWalkModel` flag rides the
  push), the dialog body renders an **embedded `PlanNextDayView`
  mount** (clipped, height-bounded — D-5) fed by the same
  `plan`/`time-text`/`effort-text` mirrors and routing the same
  callbacks; otherwise it renders the stage body. No second
  surface tree, no ad-hoc mount grammar (O-6 posture).
- **Risk / stage-2 first probe**: `swap_view` ("navigation's
  Replace primitive", `surfaces.rs::swap_view`) drops *pushed view
  levels above the root* — verify the walk's own view-root is not
  swapped while the child is live (nothing in the walk dispatches
  a navigation; the reconcile-abandon rule
  `adapter.rs::refresh`'s `walk_resume` drop *is deleted*, so no
  path remains). If any unexpected swap path materializes, the
  fallback is a dedicated `Flow::ReviewPlanner` child level
  (`SurfaceKind::Flow(tag)` is equally unique) — the plan's tests
  are written against the stage machine and the exit contract, so
  the fallback is a minting-mechanics swap, not a redesign.

### D-3 — Auto-skip law (fixes research §1.4 structurally)

`clamp_review_walk` (view_state.rs::clamp_review_walk) extends
symmetrically: an empty **Carry** stage at arrival/advance/refresh
advances exactly like the empty Inbox today. The skip is decided
in one place — the pass calls the same `walk_first_open_stage`
(normalization on open, on advance, and on refresh-clamp), so a
stage can never *render* empty:

- Inbox empty → Carry (today's rule, kept);
- Carry empty → Prompts (new; the planner is opt-in, never a skip
  destination);
- Both empty → the walk lands directly on Prompts, and finishing
  from there reaches Complete — the "empty everything ⇒ straight
  to completion" acceptance law. Entry→first-open-stage uses the
  same function on open.
- `WalkStep` becomes `{ Advanced, Planner, Inert }` —
  `WalkStep::Handoff` is **deleted** (the shape-B ask dies with
  D-4); advancing out of Carry either lands on Prompts via the
  skip law or the user chose the planner (D-6's `Planner`).
- Defensive UI guard: the carry verb strip renders only when
  `walk_carry_rows > 0` (the model push already carries the rows;
  the strip gains a count-gated arm), so "Move to tomorrow" is
  never rendered clickable over zero rows even in a one-pass race.

### D-4 — Shape-B deletion

`hand_off_to_planner` (`adapter.rs::hand_off_to_planner`),
`walk_resume` (field, the `WalkResume` struct, the abandon rule in
`adapter.rs::refresh`, and the resume arm of the
`(Flow::None, OpenPlanNextDay)` route), `FlowOutcome::PlannerHandoff`,
and `open_review_walk_at(.., WalkStage::Prompts)`'s resume-only
callers are **deleted**. Review (stage 3) checks this as the
"old hand_off mystery path deleted with a recorded reason" — this
paragraph is that reason: the embedded step replaces the
close-reopen return channel; `walk_resume`'s only consumer was the
undiscoverable return path (research §1.5).

### D-5 — Modal mount + geometry (J2 host, US-4)

- The app-window mount (`app-window.slint`'s
  `if (root.walk-model.open): TouchArea` scrim + bare
  `ReviewWalkOverlay` pair) is **replaced** by one
  `ModalDialogHost` mount carrying the J2 anatomy: scrim
  (`Theme.scrim`), absorbing panel root (inside clicks never
  close — research §1.3 dies), content-bounded height
  (`min(viewport-floor - 2·space-4, body preferred)` — research
  §1.2 dies), automatic `MaskWalkFacts` arming with
  `walk-stops` declared per stage body (research §1.1 dies — the
  Tab clamp is structural, no ad-hoc facts), `kind: confirm` so
  the R-4 law routes the outside click to `dismissed()` (D-6's
  abort confirm).
- The `ReviewWalkOverlay` component survives as the host's *body*
  (the stage shell minus its own panel chrome/scrim concerns);
  its root stops being a stretched Rectangle (the rows ListView's
  `vertical-stretch: 1` is replaced by a content-capped clip, the
  today-sections viewport discipline).
- **Severing fix (US-3 AC-2)**: `plan-next-day-view.slint`'s root
  gains `max-width: Theme.layout-severing-max-width;` (token,
  `theme.slint:80`; precedent `task-view.slint:189`) — the empty
  `EmptyState` conditional mount (`plan-next-day-view.slint:327`)
  can no longer collapse the window max. Geometry probe pins the
  view root's realized max-width (the `window_max_probe`-family
  pattern; the collapse only materializes through the event loop).

### D-6 — Abort-with-confirm (R-2) and the Escape ladder

- The Escape ladder's *stage* rungs are re-shaped: after the
  chunk-5 insert-mode rung (Insert disengages first,
  `adapter.rs::interpret_walk`) and the child rung
  (`cancel_top_child`), the next rung arms an **abort confirm**
  instead of stepping back/closing: a nested `ModalDialogHost`
  (kind `confirm`, two buttons — "Keep reviewing" dismiss-role /
  "End review" accept-role) above the walk dialog. Confirm-accept
  → `close_review_walk()` + focus restore; confirm-dismiss → the
  confirm closes, the walk stays exactly as it was.
- **Decision-safety law** (stage-3's acceptance check): decisions
  already made are `StateCommand`s already applied
  (`MoveTask`/`UpdateTask`) — aborting can never undo them; the
  confirm's copy states that facts survive, the tally (D-8) is
  *not* presented on abort. Un-committed drafts (half-typed date
  child, prompt editors) discard with the walk — that is the
  documented child-death rule (the Q3 ruling).
- The outside click / Escape *at Entry* closes without confirm
  (nothing decided, nothing typed) — the one silent exit.
- Back out of Planner ends the planner child `Cancelled` and
  returns to Carry (D-2); that rung precedes the confirm.

### D-7 — Focus/step interaction with chunk 9 (design-for, bind-not)

Chunk 9's `nav.focus-*` ActionIds → `UiEffect::FocusDirectional`
per-view table do not exist on main. This chunk:

- declares each stage body's ordered tab-stop count into the
  host's `walk-stops` (the containment seam chunk 9's directional
  walk will reuse);
- keeps the existing `ReviewWalkCursorUp/Down` row-cursor bindings
  (the walk's one-directional grammar until chunk 9 lands);
- records (in this plan + the census note) that the walk registers
  a directional step table for `ContextId::ReviewWalk` in chunk 9
  — a binding-site list, zero new ActionIds here.

### D-8 — Completion summary payload (US-1/S3)

The `Flow::ReviewWalk` variant gains a session tally:

```rust
struct WalkTally { filed: usize, moved: usize, left: usize, recorded: bool }
```

incremented in the same `view_state`/adapter seams that build the
verbs' commands (`FileCommit`'s consumption `walk_files_cursor_row`
→ `filed += 1`; `CarryTomorrow`/`CarryDateCommit` → `moved += 1`;
`CarryLeave`/`InboxKeep` row-step → `left += 1`; `Record` →
`recorded = true`). The push derives the summary model
(`presentation.rs` beside the `walk_*` helpers — counts ride the
flow, they are *not* derivable from `AppState`). `UiRequest`-level:
J4 §7's `ReviewWalkComplete` delta is realized as the existing
`WalkIntent::Finish`/`Record` advance to Complete plus a new
`WalkIntent::Complete` (the dialog's single exit button) — no new
command channel, the census `review.walk_complete` row binds it.

### D-9 — Progress presentation flags (fold-test surface)

`ReviewWalkModel` gains derived fields pushed from the pass:
`stage-position` ("Step 2 of 5" — machine stages, planner counted
only when visited), `stage-intro` (the entry sentence), skipped
strip notes (today's empty b-lines, extended to Carry), and the
summary struct. All derived in `presentation.rs` — the tree
computes nothing (dumb-client line).

## 2. TDD — test-first order (each red before its green)

Journey tests live in `crates/taskboard-ui-slint/tests/journeys.rs`
(`on_slint_thread`; names embed the J4 tokens verbatim — the
dispatch gate greps them).

| # | Test (new unless noted) | Pins | Decision |
|---|---|---|---|
| T1 | `us1_s1_entry_opens_and_steps_forward` | modal opens at Entry; Next steps Entry→first open stage; strip shows position | D-1, D-9 |
| T2 | `us1_s2_stage_next_prev_roundtrip` | next/prev across Inbox→Carry→Prompts and back; back never skips past a visited stage into a closed walk | D-1 |
| T3 | `us1_s3_complete_closes_and_restores_focus` | Record/Finish → Complete → exit closes; view + focus restored | D-8 |
| T4 | `us2_s1_empty_carry_stage_auto_skips` | rows(2,0) opens at Inbox; empty Carry skipped to Prompts; `Move to tomorrow` never rendered (structural: strip count gate) | D-3 |
| T5 | `us2_s1_empty_everything_reaches_completion` | rows(0,0) → Entry→Prompts→Complete end-to-end — the whole walk is never skipped | D-3 |
| T6 | `us3_s1_planner_embeds_and_back_returns_to_carry` | plan-next-day verb mounts the child level; walk stays open beneath; back → Carry | D-2, D-6 |
| T7 | `us3_s2_planner_complete_advances_to_prompts` | child exit `Completed` → Prompts; placed tasks visible in the resumed strip state | D-2 |
| T8 | `us4_s1_modal_containment` | Tab walk clamped to `walk-stops` (host `mask-extent` seam); inside panel click leaves the walk open; content-bounded `panel-height` (host seams) | D-5 |
| T9 | `us4_s1_abort_confirm_keeps_decisions` | Escape/outside at Carry arms confirm; accept ends with earlier `MoveTask` still applied; dismiss restores the stage verbatim; Entry closes silently | D-6 |
| T10 | seam: `carry_tomorrow_never_dispatches_without_cursor` | `WalkIntent::CarryTomorrow` with an empty carry list dispatches **no** command (typed inert, matches!-free structural count) — the adapter.rs CarryTomorrow guard, kept and pinned | D-3 |
| T11 | geometry: plan-next-day severing probe (window_max_probe family) | view root realized max-width stays `Theme.layout-severing-max-width` with the empty `EmptyState` mount live | D-5 |
| T12 | fold: stage-progress presentation flags | `walk_first_open_stage` property cases (proptest over row-count pairs + stage) — skip law is total, never lands on Inbox/Carry with zero rows | D-3, D-9 |
| T13 | census-adjacent: existing walk unit tests in `view_state.rs` (the `open_review_walk`/`back_review_walk`/`clamp_review_walk` blocks, ~:7415-:7626) re-baselined to the five-stage machine | no silent semantics drift | D-1 |

Test-law compliance: typed outcomes only (`matches!` on
`Flow::ReviewWalk { stage, .. }` / command counts — no text
assertions); no sleeps (journeys drive effects through the
existing pump); the tally tests assert the summary struct, not
phrases; the no-ceiling law — T7 asserts placement *visibility*,
not a bound on placed count.

## 3. Implementation phases (after the tests above are red)

1. **Phase A — the machine** (`view_state.rs`): five-stage enum,
   `walk_first_open_stage`, `clamp_review_walk` extension,
   `back_review_walk` re-shape (Planner→Carry, confirm-arm hook
   via a `FlowOutcome::AbortRequested`-style ask), tally on the
   flow, `WalkStep::{Advanced, Planner, Inert}`. Green: T2, T4,
   T5, T12, T13, T10's view_state half.
2. **Phase B — the adapter** (`adapter.rs`, `router.rs`):
   `WalkIntent::Planner` + `WalkIntent::Complete`; the child
   mint/exit arms (`walk_open_planner`, `on_child_exit`'s
   `View(PlanNextDay)` arm); D-4 deletion sweep
   (`hand_off_to_planner`, `walk_resume`, `PlannerHandoff`);
   `interpret_walk` re-plug (insert rung → child rung → confirm
   rung). Green: T1, T6, T7, T10.
3. **Phase C — the dialog** (`review-walk-overlay.slint`,
   `app-window.slint`): remount on `ModalDialogHost` (kind
   confirm, per-stage `walk-stops`, buttons: stage verbs stay in
   the body, the dialog button row carries stage Next/Prev/Finish
   + the completion exit); the abort-confirm nested host; the
   embedded `PlanNextDayView` mount under `stage-kind == Planner`;
   the carry strip count gate. Green: T8, T9, T3's UI half.
4. **Phase D — geometry + census**: the severing line + T11;
   census slice update (§5 below); `scripts/ux-coverage.sh
   .artifacts/plans/2026-11-18-day-review-ux-plan.md --gate`;
   golden re-baseline if the migrated-mount goldens capture the
   walk (walk-probe goldens likely move — re-baseline with the
   committed-artifact clock law: freeze `UiSettings::clock` for
   any clock-derived text in the summary).

## 4. Wiring inventory (J4 §6, resolved to today's tree)

| J4 ID | Today | Becomes |
|---|---|---|
| W-1 | `review-walk-overlay.slint` mounted bare at `app-window.slint` (scrim TouchArea sibling) | the `ModalDialogHost` body; host owns scrim/absorb/arming; `on_step`/`on_complete` = `WalkIntent::{Advance, Back, Complete}` through the existing walk intent routes |
| W-2 | `clamp_review_walk` (Inbox-only skip) | D-3's symmetric law via `walk_first_open_stage` |
| W-3 | shape-B `OpenPlanNextDay` handoff | D-2's child level + embedded body mount |
| W-4 | the ad-hoc window mount | one J2 host mount (the migrated-overlay grammar from chunk 5) |

Census writes use **symbol anchors** per #775
(`adapter.rs::interpret_walk`, `view_state.rs::clamp_review_walk`,
…) — no bare line numbers in new cells.

## 5. Census delta (`docs/ux-surface-census.org`, Review slice)

- Update in place (symbol anchors, J4 evidence):
  `review.open`, `review.walk_cursor`, `review.walk_back`,
  `review.walk_advance`, `review.inbox_file`, `review.inbox_keep`,
  `review.file_target`, `review.file_commit`,
  `review.carry_tomorrow`, `review.carry_pick`,
  `review.carry_leave`, `reflection.record`, `review.finish` —
  verb bindings unchanged (RULING: J4 §3.4), mounting grammar
  re-anchored to the host mount.
- New rows: `review.walk_enter` (Entry stage), `review.stage_skip`
  (D-3), `review.planner_embed` (D-2; J4's `planner_return` name
  is retired with `walk_resume` — recorded here as the reason),
  `review.walk_complete` (D-8), `review.abort` (D-6).
- Kiosk column: all new cells `— (R-3)` — the walk stays
  kiosk-absent (`adapter.rs`'s kiosk gate on
  `OpenReviewWalk`/`ReviewWalk` unchanged).

## 6. Risks

1. **View-over-Flow chain semantics** (D-2): context precedence
   (`ContextId::PlanNextDay` above `ReviewWalk`) and the Escape
   route split — stage 2 writes the precedence journey (T6 covers
   the back rung) and the fallback (`Flow::ReviewPlanner` child)
   is pre-declared. Contained; tests are contract-level.
2. **`swap_view` dropping pushed view levels**: any navigation
   while the planner child is live truncates the child (and only
   the child — truncate_to semantics). The walk beneath survives;
   acceptable, and the abort-confirm law covers the draft-loss
   disclosure.
3. **Golden churn**: the walk-probe/golden surfaces re-baseline;
   clock discipline applies (§3 Phase D).
4. **`WalkStep::Handoff` deletion breadth**: `PlannerHandoff`
   appears in the settle/outcome matrix and tests; deletion is a
   compile-error-guided sweep, but the settle-matrix exhaustiveness
   law means every removed row must leave the table explicitly.
5. **Prompt-editor seeds across the new stage graph**: the
   mount-transition seed rule (`answer-seeds` one-shot) must
   survive Entry→…→Prompts arrival *and* the Planner→Prompts
   child-exit arrival; T7 asserts the seeds are not clobbered.

## 7. Diff estimate

| Area | Files | ~Lines |
|---|---|---|
| view_state machine + tally + skip law | `src/view_state.rs` | +220 / −80 |
| adapter/router (intents, child arms, D-4 sweep) | `src/adapter.rs`, `src/router.rs` | +140 / −110 |
| dialog remount + embedded planner + confirm | `ui/views/review-walk-overlay.slint`, `ui/app-window.slint` | +260 / −90 |
| severing + probe | `ui/views/plan-next-day-view.slint`, probe slint+rs | +40 |
| journeys + view_state test re-baseline + census | `tests/journeys.rs`, `src/view_state.rs` tests, `docs/ux-surface-census.org` | +420 / −120 |
| **total** | ~9 files | **~1080 ±15%** |

## 8. Definition of done (chunk-local)

- [ ] All J4 journeys green (us1_s1..s3, us2_s1, us3_s1-s2,
      us4_s1) plus T9–T13.
- [ ] `scripts/ux-coverage.sh <J4> --gate` exits 0.
- [ ] Quality gates per AGENTS §10 via `scripts/run-quality-gates.sh`.
- [ ] Census updated with symbol anchors; absences ruled.
- [ ] No `walk_resume`/`PlannerHandoff`/`Handoff` residue
      (`grep` clean), or a recorded retention reason.
- [ ] Standalone PlanNextDay untouched (existing view journeys stay
      green).
