# [Report] GUI Review Round 4, Chunk 5 — Task-View Layout (Stage 3 Review)

- **Date**: 2026-11-02
- **Branch**: `fix/ui-r4-taskview-layout` — rebased onto `origin/main`
  (post chunk-1/chunk-3 landings) and shipped as PR (URL in the
  handoff message). Plan:
  `.artifacts/plans/2026-11-01-ui-r4-taskview-layout-plan.md`.
- **Verdict**: PASS — all dispatch §6 acceptance items (a)–(e)
  verified against the post-rebase diff; the code-quality mandate
  (round-2 dispatch §1) raised no findings beyond the rebase
  resolutions recorded below.

## 1. Rebase resolution

The branch was cut before round-4 chunks 1 and 3 landed; the rebase
hit two conflicts, both upstream-shape adaptations, no logic change:

- `models.rs`: chunk 1 replaced `HintModels`' `Option<Rc<VecModel>>`
  slots with `MirroredRows` (#464 mirror protocol). This chunk's two
  new hint cells (`task_done`, `task_track`) were rebased into the
  `MirroredRows` form; their `push_rows` call sites merged cleanly.
- `tests/golden_visuals.rs`: chunk 3's notification golden and this
  chunk's task-view golden matrix appended at the same file position —
  both blocks kept.

## 2. R7 repaint-cost evidence (dispatch acceptance (c))

The reasoned delta (the netbook reference machine was not available
for a measurement; the reasoning stands alone per design §7-R7's
fallback):

- **Kiosk: zero delta, pinned.** The gate (`compute_seconds_surfaces_active`,
  `adapter.rs`) carries the explicit `!self.kiosk` term (layer (b) of
  the three-layer exclusion; pinned by
  `the_seconds_gate_stays_false_on_kiosk`). While false, the driver's
  per-tick delta is exactly one wall-second compare already paid for
  the minute boundary (the same `chrono::Utc::now()` read feeds both —
  one clock read per tick, unchanged count). The alarm pass keeps its
  byte-preserved trigger `(signal_dirty || minute_boundary)` — the
  second boundary can never run it at 1 Hz.
- **Desktop, gate armed: one full derive per second.** This is the
  same derive every user interaction already runs (reconcile + model
  diff). The Slint-side diff per tick is bounded by the model diff:
  for the summary row it is 1–2 property mutations
  (`tracked-total` phrase; `MirroredRows`/row diffs no-op everything
  unchanged). No new timer, no sleep path: the cadence stays the
  existing 100 ms `REPAINT_CADENCE_MS`.
- **Gate breadth**: armed only while the task view is mounted on a
  subject holding an open entry — idle board/today/settings desktop
  sessions pay the same zero as kiosk.

## 3. Functional review (dispatch prompt §2)

- **(a) Checkbox desync**: impossible by construction — no local
  checked state anywhere; the box renders `TaskDetailModel.done` only
  (the refresh push is the single writer), and the dispatch is a
  raw-send `SetTaskDone` over the view's *subject* (not the focused
  task — pinned by the mis-focus case in
  `the_done_checkbox_sends_the_toggled_command_over_the_subject`,
  both directions). Done visual = the card checkbox's pattern.
- **(b) Tracking move**: markup-only; the routes
  (`task-track-start/stop-clicked` → `install_tracking_seams`) were
  never attached to the old position. The status-bar chip and
  notification-surface start button share the tracking seams but none
  of the moved markup; the existing journeys and the full suite stay
  green (gates below). A4 pins the header buttons' exact old
  commands.
- **(c) Tick seam pins**: kiosk (A2), no-surface (A1's false arms:
  foreign open entry, no open entry, view left, nothing mounted),
  the `SecondTick` unit (fire-once-per-second, never inactive,
  immediate on re-activation), the `refresh_forced` routing (A7's
  unchanged-guard pins stay green). Cost evidence: §2 above.
  *Recorded divergence*: the plan's A6 "alarm-pass excludes the
  second boundary" classification pin is review-enforced, not
  unit-pinned — the trigger lives inside the driver closure and has
  no unit seam; the diff's restructure makes the exclusion structural
  (`time_boundary` is never added to the alarm condition).
- **(d) Description seams**: the editor's
  `accepted/edited/focus-lost` → `description-committed/edited/`
  `text-flow-focus-lost` routes are untouched (round-3 D15/L1); the
  stretch is layout-only (`vertical-stretch` on the section and both
  branches; the `input.preferred-height` cycle trap stays fenced).
  The existing description-edit journey stays green.
- **(e) Seconds precision**: `TrackedTotal`'s numeric parts are the
  contract (P5: 3661 s → (1,1,1); 59 s and 90 s keep seconds); the
  phrase glue is pinned structurally only (determinism, non-empty,
  zero≠1 s, segment-count growth) — no string pins, per the plan's
  recorded divergence from the `countdown_phrase` sibling.
- **(f) Scope guard**: empty diff over `taskboard-domain`,
  `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`, `taskboard-app`.

## 4. Code-quality mandate

- Naming/comments follow the house style (constraints, not
  narration); the "single tick channel" fence comment (S2) is in
  place on `compute_seconds_surfaces_active` for chunks 8/9.
- SRP: the helper is pure presentation; the gate is adapter-side
  (one computation site in the derive); the driver classifies
  boundaries only. No boundary violations.
- Test quality: typed/structural assertions throughout; no text
  assertions (the tooltip journey asserts a populated description
  slot, never its wording; the hint pins assert row counts). No
  sleeps; the rig tests are refresh-driven, the `SecondTick` unit is
  synthetic-second driven.
- S1 verified: the view's only `vertical-stretch: 1` elements are the
  description section and its two branches; the trailing spacer is
  gone.

## 5. Tier-4 items (manual, on the reference machine)

1. Header reads as one content-height row at 0.5/1.0/1.5/3.0 — no
   phantom lines (the golden matrix pins the geometry; eyeball the
   feel).
2. Tall window: the description region visibly absorbs the leftover;
   short window: it floors at the three-line editor and the page
   scrolls.
3. Tick smoothness: start tracking with the summary mounted, watch
   ~30 s — 1 Hz steps, no flicker, no neighbour-row relayout.
4. Title field in the row: opens focused+selected; Return commits,
   Escape discards, focus loss settles (round-3 rules); no row-height
   jump on open.
5. Checkbox: hover surface + tooltip with hint keys where bound; Tab
   reach + focus ring; Return/Space ≡ click; done visual parity with
   the cards.
6. Tracking button: Start/Stop at the row's right; the status-bar
   chip and the notification surface's Start button still work.
7. Date-picker overlay anchors correct off the due/start cells after
   the metadata row moved up (the `changed` re-mirror handlers should
   absorb the shift).
8. Summary row: `0s` on a never-tracked task; grows while tracking;
   freezes on stop; second precision visible on a sub-minute entry.
