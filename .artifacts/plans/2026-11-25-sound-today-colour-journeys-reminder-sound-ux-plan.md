# Reminder Sound Semantics UX Journey Plan — reminder-sound

<!--
Written from .artifacts/templates/ux-journey-plan.md. The guidance
comment STAYS in the copy; only <...> placeholders go unfilled.
-->

# [Plan] UX Journey Plan — reminder sound semantics (#1438)

<!-- Test-root: the audible behavior lives in the escalation driver
(taskboard-app), not the UI layer; the journey tests are app-crate
tests driving a fake ChimeSink. -->

Test-root: crates/taskboard-app/tests

- **Date**: 2026-11-25
- **Kind**: UX journey plan — user stories, journeys, flows, wiring
  contract for reminder sound semantics (#1438). No implementation
  decisions beyond the command contract.
- **Origin**: round-8 scope report §C-C2 (2026-11-25-backlog-reduction-round8-scope-report.md); issues #1438 (normative body) and #1307 (the landed fix whose semantics this completes); the audio posture #1322 (recorded context, out of scope here); the muted-environment contract in `crates/taskboard-app/src/escalation.rs` (the #1307/B2 landed base: `ChimeSink` + `RodioChime` + `DisabledChime`, timers' one-shot completion chime, the Alarm ladder).
- **Status**: DRAFT — operator acceptance pending (GATE-PAUSED)
- **Surfaces touched**: `crates/taskboard-app/src/escalation.rs` (the escalation driver's chime arming); no UI surface changes; no `.slint` changes.
- **Platform scope**: desktop (audible) / kiosk+daemon (structurally silent, documented posture)

## 1. Scope & goals

What the user gains: **every firing reminder chimes once** — plain
`Insistence::Normal` reminders (break-anchored notes, tracker notes)
and ad-hoc timers (already landed) each produce exactly one chime at
their fire instant t₀, on top of the existing desktop toast; alarms
keep the continuous ladder (chime opens at t₀+1 min, 30 s cadence,
volume ramp, repeating until dismissed/snoozed). The kind → sound
contract becomes complete and *ruled*:

- **Reminder / timer** (`Insistence::Normal`) → single chime at t₀,
  once. No repetition, no ramp.
- **Alarm** (`Insistence::Alarm`) → continuous ladder until dismissed
  or snoozed (unchanged; pinned by a regression journey test).
- Shared synthesis: timers and reminders use the **same** two-tone
  E5→A5 chime (D-2 below) — no new audio assets.

All playback stays behind the `ChimeSink` seam in `taskboard-app`
(the workspace's only rodio touchpoint); no channel topology, engine,
`SystemEvent`, or `EngineSignal` change (the escalation fences at
`escalation.rs:27-34` hold). The degrade-to-visual posture
(`DisabledChime`) and the muted-environment contract (kiosk/daemon
structurally silent; the log trail is the designed channel) are
preserved and documented.

**Decisions (recorded, binding for C2-impl):**

- **D-1 — the kind contract**: once-on-firing for every
  `Insistence::Normal` firing (reminder, tracker note, ad-hoc timer);
  continuous-until-dismissed for `Insistence::Alarm`. The existing
  break/tracker exception ("toast-only scheduled posture",
  escalation.rs module doc) is **retired by this plan**: they chime
  once like every other Normal firing. Rationale: #1438's request is
  universal ("all reminders and timers"); the noise argument does not
  survive a single 0.6 s chime at t₀.
- **D-2 — one chime sound**: reminders share the timers' completion
  chime (same synthesis, same volume 0.5, no ramp). Distinct tones
  per kind is a non-goal (R-3 below).
- **D-3 — no new config**: the chime's on/off, volume, and sound are
  not configurable (the D-I9 fence: cadence/volume constants stay
  named constants, never config). `reminder mute` (#1153's surface)
  already silences a reminder's channels and now also its chime —
  the existing mute path is the user's per-reminder off switch.
- **D-4 — no UI change in this slice**: #1322's audio-posture
  surfacing (Enabled/Gated/Degraded in Settings/status) stays its own
  backlog item; this plan only keeps the posture facts true
  (silent-when-gated is documented, not surfaced).

**Non-goals (with homes):**

- Audio-posture surfacing in the status/settings surface — home:
  #1322 (open backlog).
- Distinct chime tones per reminder kind — home: R-3's ruling →
  backlog issue (filed by this plan; see §9).
- Retryable audio sink (the #1307 residual "retryable or at least
  reported once in-UI") — the open-failure degrade stays
  process-lifetime, logged once; the in-UI report half is #1322's
  slice — home: #1322.
- Configurable chime sound/volume — home: R-4's ruling (D-I9 fence
  retained).

## 2. Capability × surface matrix

<!-- Fixed header — parser contract. The audible capability has one
surface: the escalation driver (app layer); the toast columns record
the pre-existing display channel the chime accompanies. "—" cells on
"core verbs" are impossible here: there is no user *editing* verb on
the sound itself (R-4 rules the absence of a settings edit surface). -->

| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |
|---|---|---|---|---|---|---|---|
| reminder.audible_fire | — (R-1) | — (R-1) | — (R-1) | — (R-1) | — (R-2) | — (R-1) | — (R-1) |
| reminder.mute | — (R-1) | — (R-1) | — (R-1) | — (R-1) | — (R-1) | — (R-1) | — (R-1) |
| alarm.audible_ladder | — (R-1) | — (R-1) | — (R-1) | — (R-1) | — (R-2) | — (R-1) | — (R-1) |
| timer.audible_fire | — (R-1) | — (R-1) | — (R-1) | — (R-1) | — (R-2) | — (R-1) | — (R-1) |
| reminder.toast_fire | D (pre-existing) | D (pre-existing) | D (pre-existing) | D (pre-existing) | — (R-1) | D (pre-existing) | — (R-1) |

**Rulings** (every deliberate absence):

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | No UI/CLI surface change for audible firing — the chime is a system-initiated, head-of-fire behavior | The fire is automatic; a surface cell would be display-without-control. The user's control verb already exists: per-reminder mute (#1153) | This plan §1 D-4 |
| R-2 | No Settings entry for chime sound/volume — the constants stay named constants (the D-I9 fence) | Config was explicitly fenced at the B2 round; #1322 will add posture *display*, not sound *edit* | This plan §1 D-3; backlog re-home via #1322's slice if the operator wants a master audio toggle |
| R-3 | Timers and reminders share one chime sound | One synthesis, zero new assets, no per-kind tonal vocabulary until users ask | Backlog issue (filed with this plan) |
| R-4 | No master "audio enabled" toggle in this slice | The muted-environment contract (mode + sink resolution) is the existing posture; a toggle is a settings-surface change owned by #1322's slice | #1322 |

## 3. User stories

### US-16 — a plain reminder chimes once when it fires

- **Story**: When one of my `Insistence::Normal` reminders (a break
  note or tracker note) fires I want to hear a single chime at the
  fire instant so that I notice the reminder without watching the
  screen.
- **Platforms**: desktop (audible); kiosk/daemon: toast-only per the
  muted-environment contract.
- **Entry points**: none — system-initiated at the reminder's fire
  instant t₀ (the escalation driver's tick over the published state).
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given a break-anchored `Insistence::Normal` reminder whose
    fire instant arrives, When the escalation driver processes the
    tick at t₀, Then the `ChimeSink` receives exactly one `chime`
    call (volume 0.5) and the toast delivery is unchanged — verified
    by journey test (fake `ChimeSink` call log).
  - AC-2: Given the same firing observed again at any later tick,
    When no new firing occurs, Then no further `chime` call is made
    (once-on-firing; no cadence, no ramp) — journey test.
  - AC-3: Given the sink open previously failed (`DisabledChime`
    degraded), When the firing arrives, Then `chime` returns `Ok`
    silently and the toast still fires (degrade-to-visual) — journey
    test.

### US-17 — a muted reminder stays silent

- **Story**: As a user who muted a specific reminder I want its
  firing to stay silent so that muted reminders do not sound.
- **Platforms**: desktop.
- **Entry points**: none — the mute state rides the reminder's
  persisted rule (the #1153 surface owns the mute verb itself,
  unchanged here).
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given a muted `Insistence::Normal` reminder firing, When
    the escalation driver processes t₀, Then no `chime` call is made
    (the toast posture per mute semantics is unchanged) — journey
    test.
  - AC-2: Given the same reminder unmuted and refiring later, When
    t₀ arrives, Then exactly one `chime` call is made — journey test.

### US-18 — an alarm keeps sounding until dismissed or snoozed

- **Story**: When an `Insistence::Alarm` reminder fires I want the
  chime ladder to continue until I dismiss or snooze it so that I
  cannot miss a hard alarm.
- **Platforms**: desktop (audible); kiosk/daemon: toast ladder only.
- **Entry points**: none — system-initiated; the dismiss/snooze verbs
  are the existing `ActionId::AlarmDismiss` / `ActionId::AlarmSnooze`
  surface (unchanged here).
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given an awaiting `Insistence::Alarm` firing at t₀, When
    the driver ticks at t₀+1 min, +1:30, +2 min, …, Then the chime
    calls follow the landed ladder (CHIME_FIRST, CHIME_EVERY, ramp to
    CHIME_HI) — journey test (regression pin of the existing
    behavior).
  - AC-2: Given the alarm dismissed (the notification consumed),
    When the driver next ticks, Then no further `chime` calls are
    made for that firing — journey test (the "continuous until
    dismissed" half of the contract).

### US-19 — a timer still chimes exactly once at completion

- **Story**: When an ad-hoc timer (Normal-insistent) completes I want
  its one-shot completion chime preserved so that the #1307 fix's
  behavior does not regress when the reminder path joins it.
- **Platforms**: desktop.
- **Entry points**: none — system-initiated at the timer's completion.
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given a Normal ad-hoc timer firing at t₀, When the driver
    processes the tick, Then exactly one `chime` call is made and no
    ladder follows — journey test (regression pin).

### US-20 — kiosk/daemon deployments stay structurally silent

- **Story**: When my taskboard runs in kiosk or daemon mode I want
  firings to remain toast-only so that a headless box never tries to
  open audio.
- **Platforms**: kiosk (the story's subject); desktop is the
  contrast.
- **Entry points**: none — boot-time posture (the muted-environment
  contract).
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given mode = Kiosk, When any firing arrives, Then the
    mounted `ChimeSink` is the `DisabledChime` no-op (no rodio
    construction, no open attempt) and the toast fires — journey
    test.

## 4. Journey specs

### US-16 — plain reminder chimes once on firing

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-16/S1 | Escalation driver tick over the published state (app layer; no UI affordance) | none — the reminder's fire instant t₀ arrives | — display only (driver reads the `ArcSwap` state; no `StateCommand`) | `ChimeSink::chime(0.5)` called once; the engine's toast delivery (E-20) proceeds unchanged | Sink open failed → `DisabledChime` returns `Ok` silently; toast still delivered (degrade-to-visual, logged once at the failure) |
| US-16/S2 | Same driver, subsequent ticks (t₀+Δ) | none | — display only | No further `chime` calls for that firing; the toast/escalation state advances per the landed Normal posture (no re-toast cadence change) | n/a — absence is the specified outcome; pinned by the journey test's call log |

### US-17 — muted reminder stays silent

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-17/S1 | Escalation driver tick; the reminder's rule carries the muted fact (#1153's persisted state) | none at fire time (the mute verb itself lives on the existing reminder surface, unchanged) | — display only | No `chime` call; toast delivery follows the existing mute semantics | Unmute (US-17/S2) restores audibility at the next firing |
| US-17/S2 | Existing reminder surface's unmute verb (pre-existing, unchanged) | unmute the reminder | — display only (no new command; the existing mute/unmute path) | The next firing of this reminder chimes once (US-16/S1 behavior) | Mute again before t₀ → back to US-17/S1 |

### US-18 — alarm ladder continuous until dismissed

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-18/S1 | Escalation driver tick; awaiting `Insistence::Alarm` firing | none | — display only | Chime at t₀+1 min, then every 30 s, volume ramping CHIME_LO→CHIME_HI over 5 min (the landed constants) | Sink degraded → ladder's chime calls no-op via `DisabledChime`; toast ladder continues |
| US-18/S2 | The alarm's toast / notification surface (pre-existing) | dismiss the alarm (`ActionId::AlarmDismiss`, unchanged) | ActionId::AlarmDismiss (pre-existing — consumed by the existing pipeline) | The firing leaves the awaiting set; the next driver tick makes no further `chime` call for it | Snooze instead (`ActionId::AlarmSnooze`) → ladder re-arms at the snooze target (existing semantics, pinned by AC-2's absence after dismiss only) |

### US-19 — timer one-shot chime preserved

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-19/S1 | Escalation driver tick; Normal ad-hoc timer at completion t₀ | none | — display only | Exactly one `chime(0.5)` call; no ladder (the landed completion-chime path, now shared with reminders per D-2) | Sink degraded → silent no-op, toast still delivered |

### US-20 — kiosk stays structurally silent

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-20/S1 | Boot in Kiosk mode (`bootstrap.rs` posture) | none | — display only | `mount_chime` returns `DisabledChime`; all firings toast-only for the process lifetime; the posture is logged (tracing) at boot | n/a — the silent posture *is* the specified outcome; the in-UI posture surfacing is #1322's slice, not this journey's |

## 5. Interaction flows

No multi-surface journeys and no overlays: every story is a single
system-initiated behavior in one layer (the escalation driver), with
the user's only verb (alarm dismiss/snooze, reminder mute) pre-existing
and unchanged. A flowchart would add nodes for the pre-existing
notification pipeline only; the state machine of interest is the
driver's, already documented in `escalation.rs`'s module doc. Per
§2.1.5's purpose (ID-addressable, greppable nodes), the addressable
nodes here are the journey steps themselves (US-16/S1 … US-20/S1) and
the seams they exercise (`ChimeSink::chime`, `DisabledChime`,
`mount_chime`, `RodioChime`).

## 6. Wiring inventory

<!-- Fixed header — parser contract. This slice changes app-layer
glue only; there are no new slint callbacks and no new ActionIds.
Every row therefore carries "—" in Callback and Commands with the
reason stated in Journey steps, per the template's no-command law. -->

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | crates/taskboard-app/src/escalation.rs | n/a (process glue task) | — (no slint callback — app-layer driver change; the UI never owns audio) | `ChimeSink` trait (extended call sites in the driver's `apply()` path; the seam trait itself is unchanged) | — (no new ActionId — system-initiated) | — (no StateCommand/UiRequest — the driver reads published state and owns no commands) | n/a — not a list row | US-16/S1, US-16/S2 | crates/taskboard-app/tests/reminder_sound_journeys.rs::us16_s1_normal_reminder_chimes_once_at_fire |
| W-2 | crates/taskboard-app/src/escalation.rs | n/a | — (no slint callback — mute is read from the persisted reminder rule) | `ChimeSink` call site gated on the muted fact (#1153's rule state) | — | — | n/a | US-17/S1, US-17/S2 | crates/taskboard-app/tests/reminder_sound_journeys.rs::us17_s1_muted_reminder_makes_no_chime_call |
| W-3 | crates/taskboard-app/src/escalation.rs | n/a | — (no slint callback — regression pin of the Alarm ladder) | `ChimeSink` ladder call sites (CHIME_FIRST/EVERY/RAMP constants — unchanged) | ActionId::AlarmDismiss | — | n/a | US-18/S1, US-18/S2 | crates/taskboard-app/tests/reminder_sound_journeys.rs::us18_s1_alarm_ladder_continues_until_dismissed |
| W-4 | crates/taskboard-app/src/escalation.rs | n/a | — (no slint callback — regression pin of the timer path) | `ChimeSink` completion-chime call site (COMPLETION_CHIME_VOLUME — unchanged) | — | — | n/a | US-19/S1 | crates/taskboard-app/tests/reminder_sound_journeys.rs::us19_s1_timer_chime_stays_one_shot |
| W-5 | crates/taskboard-app/src/bootstrap.rs | n/a | — (no slint callback — boot posture unchanged) | `mount_chime` (mode ∧ sink-resolution gate — unchanged) | — | — | n/a | US-20/S1 | crates/taskboard-app/tests/reminder_sound_journeys.rs::us20_s1_kiosk_mounts_the_disabled_chime |

## 7. Command-contract delta

<!-- Fixed header — parser contract. Empty by design: the slice adds
no StateCommand, UiRequest, or ActionId. The audible behavior is
system-initiated app-layer glue behind the ChimeSink seam; the user's
verbs (mute, dismiss, snooze) all exist. An engine work item exists
because US-16/S1 exercises the driver — not because a journey step
dispatches a new command. -->

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|
| — (none) | — (no new command of any Kind) | — (the escalation driver's chime arming is internal to taskboard-app; no channel topology change, the escalation.rs fences hold) | US-16, US-17, US-18, US-19, US-20 |

## 8. Verification plan

- **Journey tests** (step-ID-named, `crates/taskboard-app/tests/reminder_sound_journeys.rs`, fake `ChimeSink` recording the call log; clock via the injected `Arc<dyn Clock>`/deterministic ticks per testing_strategy §6):
  - `us16_s1_normal_reminder_chimes_once_at_fire` (US-16/S1, AC-1)
  - `us16_s2_no_repeat_after_the_firing_tick` (US-16/S2, AC-2)
  - `us16_s3_degraded_sink_still_delivers_the_toast` (US-16/S1 escape, AC-3)
  - `us17_s1_muted_reminder_makes_no_chime_call` (US-17/S1, AC-1)
  - `us17_s2_unmuted_reminder_chimes_again` (US-17/S2, AC-2)
  - `us18_s1_alarm_ladder_continues_until_dismissed` (US-18/S1, AC-1)
  - `us18_s2_dismissed_alarm_stops_the_ladder` (US-18/S2, AC-2)
  - `us19_s1_timer_chime_stays_one_shot` (US-19/S1, AC-1)
  - `us20_s1_kiosk_mounts_the_disabled_chime` (US-20/S1, AC-1)
- **Seam tests**: the fake `ChimeSink` records `(volume, count)` —
  assertions are typed call-log facts, never audio output or log text;
  `DisabledChime` posture asserted by trait-object type at mount.
- **Fold/presentation tests**: none — no presentation model changes.
- **Kiosk gates**: US-20/S1 is the kiosk gate (DisabledChime mount).
- **Manual**: the audible quality of the shared chime (is one
  two-tone E5→A5 recognizable as "something fired") — walkthrough
  protocol at stage 3 on a desktop box with speakers.

## 9. Definition of done

- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0
- [ ] Every P1 story's journey completes end-to-end including escapes
  (US-16–US-20 above)
- [ ] Walkthrough protocol executed at stage 3; findings resolved or
  homed
- [ ] All matrix absences carry rulings (R-1…R-4); census updated
  (the reminder/timer/alarm audible rows note the ruled contract)
- [ ] Non-goals filed as backlog issues: distinct per-kind tones
  (R-3), master audio toggle / posture surfacing (#1322 owns),
  configurable chime (R-4 — D-I9 fence retained, no new issue needed;
  the fence citation is the home)
