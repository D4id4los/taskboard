# Chunk B2 — Timer Sound (#1307) — Stage-1 Plan (2026-11-24)

Round 6, chunk B2, branch `fix/timer-sound`. Normative inputs:
`.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md` §B,
`.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md` §1/§9
(the dispatch table records **no UX plan** for B2 — this chunk restores
intended/decided audio behavior; it adds no new verb, view, badge, or
setting beyond the completion chime the accepted dispatch scoped), and
issue #1307.

## 1. Diagnosis (code-confirmed, 2026-11-24 @ dc96c034)

The audio path: `escalation.rs` owns the workspace's only rodio touchpoint
(`RodioChime`, a synthesized two-tone buffer — no asset file, so no
missing-asset failure). The driver (`spawn_escalation_driver`) steps the
pure ladder and applies `EscalationAction::Chime { volume }` through the
`ChimeSink` seam. Four independent silencers, all confirmed:

1. **Normal timers are structurally silent (by design, not a wiring bug).**
   `awaiting_alarms` (escalation.rs:257–274) filters
   `note.insistence == Insistence::Alarm`; the driver parks with no armed
   timer on an empty set (escalation.rs:502–513). A Normal-insistence
   ad-hoc alarm (i.e. a "timer") never enters the awaiting set, so no
   ladder step ever runs and no chime is even considered. There is no
   second audio call site.
2. **Boot gating.** `bootstrap.rs:1460–1466`:
   `audio_allowed = mode == Desktop ∧ resolve_sink(sink, mode) == Desktop`.
   `resolve_sink` (notifications.rs:333–337) sends `Auto` under
   Daemon/Kiosk — and any explicit `Log` — to `SinkKind::Log`, so
   `mount_chime` returns `DisabledChime` **silently** (no UI indication,
   no info-level boot record).
3. **One-shot degrade.** A typed `AudioError::Open` (no ALSA/Pulse device,
   typical headless/kiosk) at `mount_chime` (escalation.rs:176–190)
   degrades to `DisabledChime` with one `tracing::warn!` and is never
   retried for the process lifetime.
4. **Timing.** The chime opener is `CHIME_FIRST = t₀ + 1 min`
   (escalation.rs:36) on a 30 s cadence; an alarm fired and dismissed
   within that minute never sounds. This is the designed escalation
   ladder, not a bug — recorded so the report can discriminate.

`RUST_LOG` discriminates 2/3 from 4 today (the two warn sites); nothing
discriminates 1 except reading the code.

## 2. Decisions (every open question resolved)

- **D1 — which timer kinds chime.** *Alarm*-insistence notifications keep
  the escalating ladder exactly as designed (toast +1 min opener, 30 s
  chime cadence, volume ramp — unchanged). *Normal*-insistence **ad-hoc
  alarms** (`ReminderRef::Adhoc ∧ Insistence::Normal` — what the timer
  overlay creates) gain a **one-shot completion chime at their fire
  instant t₀**, at a named constant volume. Break-anchored and Tracker
  Normal notes stay toast-only: breaks are scheduled posture, tracker
  rows are display nudges — chiming them would be noise, and the bug
  report names timers. (If wanted later: backlog, not this chunk.)
- **D2 — why the fire-instant chime is correct for Normal but not Alarm.**
  Alarm's +1 min opener is escalation ("you have not reacted"); a
  Normal timer's whole purpose is to speak at completion. The existing
  fence "insistence never influences *when* something fires — only what
  escalates after" is preserved: t₀ is the fire instant itself, a
  one-shot, not a cadence.
- **D3 — the boot gate stays.** D-9 (`Desktop ∧ Desktop-sink`) is correct
  kiosk posture: `Auto` under Kiosk/Daemon resolving to Log means
  *structurally silent by design*. Not a bug; documented (D6). The
  *silent* part is the defect class, addressed at log level now (D4)
  and in-UI later (backlog #1322).
- **D4 — visibility fix in-chunk is typed-log-level.** `mount_chime` and
  the bootstrap gate emit one structured record each (warn with
  mode/sink/gate fields; the degrade warn gains `?mode`/sink context).
  No new channel, no `SystemEvent`/`EngineSignal` variant, no AppState
  field — the escalation module's fences (dispatch §11.3) hold, and the
  diff stays S/M. In-UI surfacing of audio posture is out-of-scope
  (backlog #TBD-a).
- **D5 — no retry of a failed open in-chunk.** A retryable open is a
  lifecycle change (re-mount seam, environment-change trigger) —
  out-of-scope (backlog #1323). The one-shot degrade stays; it is now
  logged with enough context to diagnose.
- **D6 — the muted-environment contract (kiosk posture).** Text lands in
  two places: the `escalation.rs` module docs and `docs/architecture.org`
  (notification/escalation section): *Audio requires Desktop app mode AND
  a Desktop-resolved notification sink; `Auto` resolves Desktop only
  under Desktop mode, so daemon/kiosk deployments are structurally
  silent — the log trail is the designed channel. A machine without a
  usable default audio device (headless, no ALSA/Pulse) degrades to
  visual-only for the process lifetime, logged once at boot. Timers
  intended to sound must run under Desktop mode.*
- **D7 — the faked-sink seam for the trigger pin.** The existing
  `ChimeSink` trait is the seam: tests inject a recording fake
  (`RecordingChime`, mirroring the `FakeSink` pattern — a mutex Vec of
  `(volume, count)` records). The typed pin: a driver-level test with a
  Normal ad-hoc alarm awaiting asserts **exactly one `chime` call at
  t₀** (first step at/after t₀), and a second step produces no more —
  typed outcome (`chime_calls == 1`), never audio-output assertions.
  The Alarm-side pins (existing ladder tests) stay unregressed; the
  completion channel is observable only at `ChimeSink`, so the existing
  `normal_insistence_never_escalates` sink-record pin survives
  unchanged (it asserts `NotificationSink` records, which remain empty
  for Normal).
- **D8 — ladder shape.** New pure derivation `awaiting_completion_chimes`
  beside `awaiting_alarms` (never widen the Alarm filter — that would
  drag Normal notes into the re-toast cadence). `LadderMemory` gains a
  `chimed: BTreeSet<(ReminderRef, DateTime<Utc>)>`; `escalation_step`
  stays Alarm-only; the completion channel is applied in the driver
  loop as its own pure helper (`completion_chimes(awaiting_normal,
  memory, now) -> Vec<EscalationAction>`), evicted with the set (D-12
  spirit). Exactly-once is guaranteed by the memory set, not by the
  sink.
- **D9 — volume constant, never config** (D-I9's recorded fence):
  `COMPLETION_CHIME_VOLUME: f32 = 0.5`, named in escalation.rs beside
  the other ladder constants.
- **D10 — timing fence.** The driver already wakes on the fire's
  `StateUpdated` publish, so the completion chime lands within the
  first step at/after t₀ — no new timer topology, no armed timer while
  only Normal notes await beyond their t₀ (they are one-shot; the
  driver parks again once chimed... precisely: while a not-yet-chimed
  Normal note awaits, the next-due computation includes its t₀).

## 3. Implementation outline

1. `crates/taskboard-app/src/escalation.rs`:
   - `awaiting_completion_chimes(state, offset, now) -> Vec<Notification>`
     (delivered, un-dismissed, `Adhoc ∧ Normal`).
   - `LadderMemory.chimed` set + eviction; pure completion-channel step.
   - Driver loop: derive both sets, apply completion actions through the
     same `chime` arc (one audio owner, D-4).
   - `mount_chime` / gate logging per D4; module-docs contract per D6.
2. `crates/taskboard-app/src/bootstrap.rs`: add mode/sink context to the
   gate-time log record (no logic change).
3. `docs/architecture.org`: the muted-environment contract paragraph.
4. Tests (typed, lowest layer, no text/audio-output assertions):
   - Pure: one-shot at t₀; no second chime on later steps; memory evicts
     with dismissal; non-Adhoc Normal notes produce no completion action;
     volume constant clamped.
   - Driver (`start_paused` + fake clock, per the clock discipline):
     the D7 trigger pin — Normal timer awaiting ⇒ exactly one fake-chime
     record at t₀; dismissed ⇒ none; the Alarm ladder unregressed.
   - Existing tests expected green without edits (D7 rationale).

## 4. Quality gates

Pre-commit hooks + `flock`-wrapped `scripts/run-quality-gates.sh` full
suite per dispatch discipline; every `cargo` call routes through
`scripts/memory-gate.sh`. Commit all landed work before any wait; if the
gate lock is contended with no useful work left, report
"gate deferred: lock held by <holder>" and end the stage.

## 5. Risks

- **R1**: driver-loop churn touching the shared chime arc could regress
  the focus-boundary sharing — mitigated: the arc is only consumed, not
  re-created (D-4 pin exists).
- **R2**: a not-yet-chimed Normal note arms the driver where it previously
  parked; a bug could busy-wake — pinned by the "no action before t₀"
  pure test and the parked-while-idle driver test.
- **R3**: `next_due` interaction between the two channels could double-arm
  or starve — mitigated by keeping the completion step's next-due folded
  through the same single `next_due` fold.
- **R4**: CI runners have no audio device; `RodioChime::open` must never
  run in tests — it already never does (the injection seam); keep it so.

## 6. Out-of-scope findings → backlog issues (opened at stage 1)

- **#1322**: in-UI surfacing of the resolved audio posture
  (settings/status surface) — user-visible, journey-gated there.
- **#1323**: retryable audio-open / re-mount on device availability
  change.

## 7. Estimated diff

~200–280 lines including tests (escalation.rs dominant, bootstrap.rs
log context, docs/architecture.org paragraph). No engine, domain, or UI
crate changes.
