# Chunk SB2 — config/files/signals (#1045, #1083, #1093) — `feat/ui-config-files-signals`

- **Date**: 2026-11-21
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md`
  (US-4, US-5, US-6; W-5..W-7; R-3, R-4, R-5). Round-4 dispatch plan
  chunk SB2. Authorities: the config-reload journey plan's broadcast
  row (`.artifacts/plans/2026-11-18-config-reload-batch-ux-plan.md` §7),
  the B7b poll_secs write lane (`.artifacts/plans/2026-11-19-files-sync-poll-write-lane-plan.md`),
  B2's hybrid journal policy (`.artifacts/plans/2026-11-19-journal-decode-robustness-plan.md` §6).
- **Status**: DRAFT — gated on the journey plan's operator acceptance.
- **Issues**: #1045 (p3), #1083, #1093 (p3).

## 1. Scope

1. **#1045 ConfigReload read-only lane** — a Settings lane consumes
   `SystemEvent::ConfigReload(ConfigReloadOutcome)` and shows
   applied / kept-last-known-good, so a kiosk operator sees why an
   external config edit did not take effect (US-4). Read-only forever
   (R-3): the lane installs no command callbacks.
2. **#1083 Files-row poll_secs field** — a spin/secs field on the
   Settings Files row writes through the whole-snapshot
   `UpdateSettings` lane; the landed B7b apply registry
   (`set_files_sync_poll_secs`) is the sole engine writer; the `> 0`
   gate refuses typed (US-5). The Field lattice's explicit-exception
   list shrinks back (no poll_secs exception stays) — the CLI-only
   exception retires.
3. **#1093 JournalDegraded badge** — the status area consumes
   `SystemEvent::JournalDegraded` (typed `CorruptionDetail`s, dropped
   row count, re-pinned head) and renders a persistent display-only
   badge: undo history truncated, tasks intact (US-6). Completes B2's
   hybrid policy.

## 2. Work items (journey-id mapped)

- W-5: the ConfigReload fold + Settings row. Tests:
  `us4_s1_reload_outcome_folds_applied`, `us4_s2_refused_folds_kept`,
  structural: the lane installs no command callbacks.
  **Erratum (2026-11-23, R-2 of
  `2026-11-22-misc-gated-journeys-ux-plan.md` §US-24):** this row's
  surface spelling read as if the reload lane belonged inside the
  Files card — plan-side imprecision, not an implementation gap. The
  whole-view caption at the top of the Settings view is normative: a
  `taskboard.toml` reload applies to the whole configuration, not the
  files-sync subtree. No UI move ships (#1225 closes on this note).
- W-6: the poll_secs field + staged-edit commit/cancel. Tests:
  `us5_s1_poll_secs_rides_update_settings`,
  `us5_s2_poll_secs_typed_refusal_nonpositive`; lattice/registry
  convergence assertion (no exception stays).
- W-7: the degraded badge fold. Tests: `us6_s1_degraded_badge_renders`,
  `us6_s2_healthy_boot_no_badge`; structural: display-only.
- Census slice: Settings gains ConfigReloadLane, FilesPollSecs row
  kinds; the status-area badge cell; R-3/R-4/R-5 recorded.

## 3. Traps (from the dispatch plan + rulings)

- #1045's lane is **read-only** — reload outcomes surface, never
  mutate from the UI; no "reload now" affordance may creep in.
- #1083's field writes through the **landed poll_secs lane** — no
  second writer; if the apply registry rejects a value the CLI also
  rejects, the field's refusal is the typed gate, not a UI-side
  re-validation.
- #1093's signal is the **B2-typed JournalDegraded event** — no new
  event kind; no decode-path change; the badge is not dismissible.
- Kiosk: both signal lanes must render where the kiosk chrome shows
  them (Settings lane + status area); if the kiosk status area is
  absent in practice, that is a walkthrough finding, not a silent
  skip.

## 4. Verification

- Journey tests in `tests/journeys/small_gated_journeys.rs`; fold
  tests assert typed presentation enums; structural tests assert
  absence of command dispatches on the read-only lanes.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md --gate`
  as the stage-2 self-check.
- Quality gates via `scripts/run-quality-gates.sh`.

## 5. Definition of done

- [ ] Journey plan's US-4/US-5/US-6 wiring rows implemented and green
- [ ] The Field lattice exception list no longer carries poll_secs
- [ ] Gate exits 0 on the journey plan
- [ ] #1045, #1083, #1093 close with the PR reference
