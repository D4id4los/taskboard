# Chunk SB1 — dialog semantics + banner Unmute (#976, #1011) — `feat/ui-dialog-semantics-unmute`

- **Date**: 2026-11-21
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md`
  (US-1, US-2, US-3; W-1..W-4; R-1, R-2). Round-4 dispatch plan
  `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  chunk SB1. Authorities: J2 R-4 (`.artifacts/plans/2026-11-18-widget-framework-ux-plan.md`),
  the #733 twin grammar (`.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §5),
  the W2 migration debt (#976), W1 proof-mount today-behavior.
- **Status**: DRAFT — gated on the journey plan's operator acceptance.
- **Issues**: #976 (p2), #1011 (p3).

## 1. Scope

1. **#976 outside-click flip** — the capture overlay's and the recur
   editor's outside-click route the accept/save seam instead of the
   discard seam (US-1/S1–S2, US-2/S1–S2). The W1 proof-mount's
   today-behavior (outside click = discard) retires here, per the W2
   migration debt. Escape stays the explicit discard/cancel (US-1/S3,
   US-2/S3). Empty-draft / invalid-draft outside-clicks do not commit.
2. **#1011 banner Unmute** — the banner/status-bar mute control
   becomes direction-aware: renders Unmute when
   `state.reminders.muted`, dispatches `SetTaskRemindersMuted
   { muted: false }` (US-3/S1); the unmuted direction is preserved
   (US-3/S2). No separate muted badge on the banner (R-2).

## 2. Work items (journey-id mapped)

- W-1/W-2: capture backdrop release routing (R-1); Escape path
  unchanged. Tests: `us1_s1_outside_click_commits_capture`,
  `us1_s2_empty_draft_does_not_commit`, `us1_s3_escape_still_discards`.
- W-3: recur editor backdrop routing; invalid-draft outside-click
  keeps the editor open. Tests: `us2_s1_outside_click_accepts_recur`,
  `us2_s2_invalid_draft_stays_open`.
- W-4: banner mute-direction control + fold from engine truth. Tests:
  `us3_s1_banner_unmute_dispatches_false`, `us3_s2_banner_mute_preserved`.
- Census slice: capture/recure outside-click semantics recorded in the
  census (R-1); the banner grammar cell updated.
- P1 slice: US-1, US-2, US-3 complete end-to-end with escapes — this
  chunk alone is the P1 slice for SB1.

## 3. Traps (from the dispatch plan + rulings)

- No new command variants: the flip is a **reroute** of the existing
  accept seams (W1's `commit_capture()` path, the recur armed-confirm
  path) — coining a second accept seam is a review finding.
- No text assertions: fold the mute direction from
  `state.reminders.muted`; assert typed dispatches.
- The recur editor's invalid-draft state must not change: outside
  click on an invalid staged draft is a no-op, not a forced accept.
- The #733 notification-surface Unmute must remain untouched — this
  chunk is the *sibling* surface (R-2); a regression there is a
  finding.

## 4. Verification

- Journey tests in `tests/journeys/small_gated_journeys.rs` (the
  plan's shared file); seam tests for both mute directions.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md --gate`
  as the stage-2 self-check.
- Quality gates via `scripts/run-quality-gates.sh` per AGENTS.md §10.

## 5. Definition of done

- [ ] Journey plan's US-1/US-2/US-3 wiring rows implemented and green
- [ ] Gate exits 0 on the journey plan
- [ ] #976 and #1011 close with the PR reference
