# Chunk C2-impl — Reminder Sound Semantics (#1438) — Plan Skeleton

<!-- Skeleton written by the J-C2/C3 journey-stage agent (2026-11-25).
The stage-2 agent COMPLETES this plan at gate clearance (the stage-2
prompt restates the discipline verbatim), filling the verification
sections from the landed implementation. Do not dispatch before the
journey plan is accepted. -->

- **Branch**: `feat/reminder-sound-semantics`
- **Origin**: journey plan
  `.artifacts/plans/2026-11-25-sound-today-colour-journeys-reminder-sound-ux-plan.md`
  (status: GATE-PAUSED — operator acceptance pending); issue #1438.
- **Body reference**: #1438. Closes #1438 on merge (per the
  dispatcher's at-merge closure list).

## 1. Diagnosis (code base, journey-confirmed)

- The escalation driver (`crates/taskboard-app/src/escalation.rs`)
  already owns the audio seam: `ChimeSink` + `RodioChime` +
  `DisabledChime`, timers' one-shot completion chime at t₀
  (`COMPLETION_CHIME_VOLUME = 0.5`), the Alarm ladder
  (`CHIME_FIRST`/`CHIME_EVERY`/`CHIME_RAMP`/`CHIME_LO`/`CHIME_HI`).
- The gap: `Insistence::Normal` *scheduled* reminders
  (break-anchored, tracker notes) are toast-only — the module doc
  records the "scheduled posture" exception that this journey plan's
  D-1 retires.
- No engine, channel, `SystemEvent`, or UI change: the escalation
  fences (escalation.rs:27-34) hold.

## 2. Decisions (ruled by the journey plan — implement as written)

- D-1: once-on-firing for every `Insistence::Normal` firing
  (reminder, tracker note, ad-hoc timer); continuous-until-dismissed
  for `Insistence::Alarm` (unchanged; pinned by US-18's regression
  journey).
- D-2: reminders share the timers' chime synthesis and volume.
- D-3: no new config; the per-reminder mute (#1153) also silences
  the chime.
- D-4: no UI change; #1322 (audio posture surfacing) stays out.
- Kiosk posture documented (the muted-environment contract section of
  the module doc gains the reminder half; US-20 pins the mount).

## 3. Implementation outline

1. Extend the driver's Normal firing path: at a firing's t₀ tick,
   issue one `chime(0.5)` through the mounted `ChimeSink` when the
   reminder is not muted; no cadence entry is created for Normal
   firings.
2. Gate the chime on the mute fact from the reminder's persisted rule
   (the #1153 state the driver already reads for toasts).
3. Update the module doc's posture text (the retired toast-only
   exception) — the kiosk/daemon contract language stays.
4. Journey tests: `crates/taskboard-app/tests/reminder_sound_journeys.rs`
   — the step-ID-named list in the journey plan §8 (fake `ChimeSink`
   call log; injected clock/deterministic ticks, no sleeps).
5. Property/quality: no new property surface; existing escalation
   tests keep passing (the timer and alarm paths must be untouched —
   the US-19/US-18 regression pins prove it).

## 4. Quality gates

- Pre-commit hooks + flock-wrapped `scripts/run-quality-gates.sh`
  full suite (the dispatcher owns the coverage trio; this chunk gates
  on hooks + nextest per the sub-agent dispatch discipline).
- `scripts/ux-coverage.sh <journey plan> --gate` green once the
  journey tests land (stage-2 self-check).

## 5. Risks

- Chime-on-every-reminder noise complaints → the mute verb exists;
  D-1's rationale recorded in the journey plan. Revisit via R-3's
  backlog item only with operator demand.
- Overlap semantics: a chime that overlaps a running Alarm chime
  replaces the held player (the existing RodioChime replace posture)
  — acceptable; note it in the module doc.

## 6. Out-of-scope findings → backlog issues

Filed by the journey stage (see the journey plan §9 and the dispatch
report): distinct per-kind tones; master audio toggle / posture
surfacing (#1322 owns); configurable chime (D-I9 fence retained).
Any new finding → `backlog:` issue immediately per AGENTS.md §7.

## 7. Estimated diff

~120–200 lines: escalation.rs arming + doc, the new app test file
(~150 lines), no domain/UI/engine changes.

## 8. Stage-2 completion checklist (fill at gate clearance)

- [ ] Exact call-site line cites for the chime arming + mute gate
- [ ] Journey test file + names as §8 of the journey plan nominates
- [ ] Module doc posture paragraph rewritten (retired exception noted
  with date + #1438)
- [ ] Full local hook/nextest evidence recorded
