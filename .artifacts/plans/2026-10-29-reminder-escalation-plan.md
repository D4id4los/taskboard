# [Plan] Reminder escalation — the driver task, the alarm window, owned sound (#477, DR-3 chunk 4) — stage-1 implementation plan

- **Date**: 2026-10-29
- **Kind**: stage-1 TDD implementation plan for chunk DR-3/4 of the
  design-rounds batch dispatch. Zero production code in this commit;
  this file is the chunk's entire planning artefact. Implementation is
  stage 2's (dispatch prompt 4.2), review + shipping stage 3's
  (prompt 4.3).
- **Origin**: design round
  `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`
  — §3 Part C item 3 (the D-I5 driver half: **no new lane**) and
  §3 Part E (D-I9–D-I10, **normative item by item**), §5 (the
  chunk-4 driver test hooks), §6 (the rodio/Wayland risk table), §7
  (chunk 4) — dispatched by
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§2/§11.3 (wave 8). DR-3 round binding notes (dispatch §3.2)
  apply: DR-1's R3 settlement semantics are the written authority; the
  kiosk banner-only fence; the OS-delivery/visual plumbing stays with
  GUI review round 3.
- **Normative base**: branch point = `origin/main` @ `fca7961c` (the
  worktree's `-b` base). Chunk gates **verified merged**: DR-3/2
  (insistence fields, PR #542 closing #475, merge commit `a4fcc053`)
  and DR-3/3 (timers & alarms surface, PR #529 closing #476, merge
  commit `a8756227`) are both ancestors of `origin/main`
  (`git merge-base --is-ancestor` checked 2026-10-29). This is **not**
  a shape-widener chunk: no `AppState` field, no migration, no
  `StateCommand` arm — the shared insta suite and the command-surface
  lane are untouched.
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§4, D-1…D-16). The one finding
  this planning pass contributes beyond the origin plan is **D-15**:
  cpal's ALSA host is unconditional on Linux, so four CI workflow apt
  blocks must gain `libasound2-dev` or the app crate stops building in
  CI the moment the rodio pin lands.

## 1. Mission & binding constraints

Wake the `alarm` insistence rung up (origin Part E): one app-layer
**escalation driver task** beside the notifier (re-toast cadence +
owned rodio sound ramp), one **always-on-top alarm window** (a
state-reader in the UI layer), and the **critical-urgency toast hint**
on `alarm` payloads. Per the dispatch §11.3:

1. The driver is **process glue, not topology**: no engine change, no
   new channel, no `SystemEvent`/`EngineSignal` variant, no E-20
   change. It reads the published-state `Arc<ArcSwap<AppState>>` and
   acts through the existing `NotificationSink` port + its own rodio
   output. Stage 3's grep fence: `crates/taskboard-domain/src/messages.rs`
   must have a **zero diff**.
2. The driver **wakes on its own cadence only while ≥ 1 un-dismissed
   `alarm`-level notification exists** (derived from state); an idle
   driver is parked on the `EngineSignal` broadcast with **no armed
   timer** — a busy-loop or an always-armed idle tick is a review
   finding.
3. The cadence/volume ladder is a **pure function**
   (`escalation_step`) tested over constructed states — no sleeps, an
   injected clock; the fake sink records re-toasts and pins
   **replacement-not-stacking**.
4. **No focus steal anywhere**: always-on-top persistence + an
   attention request + re-toasts + sound are four independent
   insistence channels, each individually no-op-able (Wayland
   forbids both window features; the platform-honest ruling).
5. Audio is **owned** (`rodio`, stack approval GRANTED 2026-10-26),
   synthesized in code (**no decode features**), behind the driver
   (port-shaped confinement); **audio failure degrades to
   visual-only — logged, never fatal, never a panic**; sound only
   when the mode is `Desktop` **and** the desktop sink resolved.
6. **Kiosk gets none of it**: banner-only fence (R3.5.3) — the kiosk
   UI never mounts the alarm window, the kiosk path never constructs
   audio. A kiosk install that wants alarms runs the full GUI.
7. The DesktopSink urgency hint is **payload data only** — the port
   signature is untouched (`Notification` has carried `insistence`
   since DR-3/2).
8. The ladder constants are **named in the driver**, never config
   (the ladder itself is per-reminder data — D-I9's recorded fence;
   promoting them to `[reminders]` is a future knob, not v1).
9. **Non-goals (fences)**: recurring alarms/snooze presets;
   per-run sound/theme pickers; native toast buttons (#444); OS
   toast sounds; the OS-delivery plumbing bugs (GUI review round
   3's queue); any new `[reminders]` config key; backlog-issue edits
   (stage 3 closes #477 and #203).

## 2. Verified anchors (branch point `fca7961c`; symbols are the stable keys)

| Fact | Anchor | Evidence |
|---|---|---|
| The awaiting derivation already exists | domain `reminders.rs` | `active_notifications` :863–913 — fired ∧ ¬dismissed ∧ anchor-resolves ∧ ¬muted, `(occurrence, reminder)` order; each `Notification` carries the fire-time `insistence` snapshot :541 |
| `Insistence` is a `Copy + Eq` two-variant enum | domain `reminders.rs` | :240–274 (`Normal` default < `Alarm` in derived `Ord`); `text_form`/`parse_text_form` for logging fields |
| The published-state handle | state `actor.rs` | `EngineHandle::shared_state() -> Arc<ArcSwap<AppState>>` :214 (the ArcSwap is seeded before the loop, :280; **no boot signal** — subscribers cannot have existed yet, :243 doc) |
| The change signal the driver parks on | state `actor.rs` | `EngineHandle::subscribe() -> broadcast::Receiver<EngineSignal>` :200; every accepted mutation publishes `StateUpdated` (:596–602); a no-op tick publishes **nothing** (:1129 doc); `Lagged` = change is the UI's recorded semantics |
| The spawn site (beside the notifier) | app `bootstrap.rs` | `spawn_notifier` call :889 (spawned *before* the engine so the first tick can notify); `spawn_state_engine` returns the `EngineHandle` at :894–906 — the driver spawns immediately after (it needs the handle); the `App` struct :340–379 and its destructure-shutdown :540–609 with `join_bounded` per actor and the `ShutdownOutcome.budget_hit` aggregation :605–630 |
| The sink port the driver re-toasts through | app `notifications.rs` | `NotificationSink::{deliver, withdraw}` :38–62 (boxed-future async, `NotifyError` unit class); `DesktopSink` tracked handles :136–219 — `Mutex<HashMap<(ReminderRef, DateTime), NotificationHandle>>`, `TRACK_CAP` 256, insert-overwrites the key |
| The D-R13 matrix | app `notifications.rs` | `resolve_sink(choice, mode) -> SinkKind {Desktop, Log}` :283–290 — `SinkChoice::Desktop` is honored under **any** mode (the operator asked); auto resolves Desktop only for `AppMode::Desktop` |
| The urgency hook in the toast builder | app `notifications.rs` + notify-rust | `toast_payload` :251–269 (pure, unit-tested); `DesktopSink::deliver` :159–199 builds `notify_rust::Notification`; notify-rust 4.18.2 has `urgency()` on all three backends (Linux `Urgency` enum; Windows/macOS `InterruptionLevel`) — `notification.rs:387/:411/:427` |
| The reminder defaults the driver derives against | app `config.rs` | `reminder_engine_config()` :1226–1239 mints `ReminderDefaults` (:1238) — the same resolution the engine and the UI lifecycle consume |
| The UI's state-reading pattern (the window mirrors it) | ui-slint | `notification_rows` (presentation.rs :403, filters the anchor card, DDF-5); `timer_rows` :675; `awaiting_alarm_count` :701; `notification_phrase` :445 (elapsed phrase, reusable); `models.rs` `push_notifications` :492; `start_repaint_driver` (lib.rs :400) — 100 ms tick (adapter.rs :48 `REPAINT_CADENCE_MS`), wall-minute gate :454, `dirty` = any signal drained |
| A second window is mounted code that already exists | ui-slint `lib.rs` | the virtualisation `ProbeWindow` tests :2228+ instantiate a second slint Window on the testing backend; `mount` :229 applies the kiosk profile from `UiSettings.kiosk` :248–250 — the UI-side mode gate |
| `always-on-top` and `no-frame` are Window properties in slint 1.18.1 | i-slint-compiler 1.18.1 `builtin_elements.rs` | `always-on-top` :2104 ("above all other windows **on window managers supporting it**" — the platform-honest semantics baked into the property); `no-frame` :2133 |
| The attention-request seam (the §6 risk confirmed) | slint 1.18.1 | **No attention API outside `unstable-winit-030`** (grepped core/api/backends). `slint::winit_030::WinitWindowAccessor::with_winit_window(|w| w.request_user_attention(..))` is public behind that slint feature (`slint-1.18.1/lib.rs:663`), re-exporting winit 0.30 types — no direct winit dependency enters any manifest |
| The slint compile entry | ui-slint `build.rs` | one `slint_build::compile` of `ui/app-window.slint`; additional exported windows ride the same unit (the `export { … }` + re-export pattern already used for `SettingsModel` et al.) |
| rodio's minimal shape (verified 2026-10-29) | crates.io metadata + published source | see §3.3 — the entire evidence row is decision **D-1** |

Out-of-scope fences restated: `messages.rs` zero-diff; no
`persistence.rs`/`command.rs` hunks; the engine's fire toast (E-20)
stays the t₀ delivery — the driver only ever **re**-toasts; DR-3/3's
landed surface rows are not reshaped (the window adds its own row fn,
it does not grow `NotificationRow`).

## 3. The design, exactly

### 3.1 The escalation driver task (D-I5's driver half)

New module `crates/taskboard-app/src/escalation.rs` (beside
`notifications.rs`; AGPL-3.0-only header). All escalation state lives
in the task; nothing engine-side changes.

**The awaiting predicate** — derived, never stored:

```rust
/// Every `alarm`-insistent notification currently awaiting the user:
/// the domain's own derivation, filtered one step. Insistence never
/// influences *when* something fires — only what escalates after.
fn awaiting_alarms(
    state: &AppState,
    defaults: &ReminderDefaults,
    now: DateTime<Utc>,
) -> Vec<Notification> {
    taskboard_domain::active_notifications(&state.tasks, &state.reminders, defaults, now)
        .into_iter()
        .filter(|note| note.insistence == Insistence::Alarm)
        .collect()
}
```

**The lifecycle** — park-derive-step:

```rust
pub(crate) fn spawn_escalation_driver(
    state: Arc<arc_swap::ArcSwap<AppState>>,
    defaults: ReminderDefaults,
    sink: Arc<dyn NotificationSink>,
    chime: Arc<dyn ChimeSink>,            // §3.4; the audio confinement seam
    mut signals: broadcast::Receiver<EngineSignal>,
    clock: Arc<dyn taskboard_domain::Clock>,
) -> tokio::task::JoinHandle<()>
```

```
loop {
    let now = clock.now();
    let awaiting = awaiting_alarms(&state.load(), &defaults, now);
    if awaiting.is_empty() {
        memory.clear();                            // D-12: bookkeeping dies with the set
        match signals.recv().await {               // PARKED — no timer armed
            Ok(_) | Err(Lagged) => continue,       // state changed; re-derive
            Err(Closed) => return,                 // engine gone (D-10)
        }
    }
    let StepOutcome { actions, next_due } = escalation_step(&awaiting, &mut memory, now);
    for action in actions { apply(&sink, &chime, action).await; }
    tokio::select! {                               // armed ONLY while awaiting ≠ ∅ (D-2)
        _ = tokio::time::sleep(next_due - now) => {}
        r = signals.recv() => match r { Ok(_) | Err(Lagged) => {}  // re-derive
                                    , Err(Closed) => return },
    }
}
```

`apply` for `ReToast` is **withdraw-then-deliver** (D-3):
`sink.withdraw(&reminder, &occurrence)` then
`sink.deliver(&notification)`, both results logged-and-dropped (the
D-R13 consumer discipline — one lost toast never kills the task).
Both land in the same `DesktopSink` tracked-handle map keyed
`(reminder, occurrence)`, so the withdraw closes the previous toast
where the platform can and the deliver re-inserts one handle —
**replacement, not stacking**, made real at the OS boundary where
close is supported, and real at the bookkeeping level everywhere.
`apply` for `Chime` is `chime.chime(volume)` — a typed
`Err` is logged and dropped (D-8).

Spawn site: immediately after `spawn_state_engine` returns the
`EngineHandle` (bootstrap.rs :906) — the notifier's
spawn-before-engine order is unavailable to the driver (it needs the
handle), and it subscribes *before the connector loop* so no
`StateUpdated` can race its first park. `App` gains an
`escalation_join: JoinHandle<()>` field, destructured in
`App::shutdown`, bounded-joined **after the engine join, beside the
notifier** (by then the engine task and the destructured
`EngineHandle` have both dropped `signals` senders, so the parked
driver has already exited on `Closed`; the bounded join only bounds a
wedged task), and `ShutdownOutcome::budget_hit` aggregates one more
`|` term (D-11).

### 3.2 The pure ladder — `escalation_step` (D-I9's constants)

The entire behavior is one pure function over constructed inputs; the
task loop only transports its outputs to the sink/audio seams.

```rust
/// One insistence step: what is due NOW, and when to wake next.
/// Pure: in → (actions, bookkeeping transition, next deadline);
/// no I/O, no clock reads. `memory` is updated as part of the
/// transition (actions are recorded at their actual instants).
fn escalation_step(
    awaiting: &[Notification],        // the derived set, canonical order
    memory: &mut LadderMemory,
    now: DateTime<Utc>,
) -> StepOutcome;

struct StepOutcome {
    /// ≤ 1 ReToast per awaiting occurrence, ≤ 1 Chime total, per call.
    actions: Vec<EscalationAction>,
    /// The next instant any ladder rule fires; `None` ⇒ the caller
    /// must not arm a timer (park on signals only).
    next_due: Option<DateTime<Utc>>,
}

enum EscalationAction {
    /// withdraw-then-deliver this occurrence's toast again.
    ReToast(Notification),
    /// one chime, at this volume (0.0..=1.0, clamped at the seam).
    Chime { volume: f32 },
}

/// The driver's bookkeeping: per-occurrence last re-toast instant,
/// and the global last-chime instant (sound is one channel — D-4).
#[derive(Default)]
struct LadderMemory {
    last_re_toast: HashMap<(ReminderRef, DateTime<Utc>), DateTime<Utc>>,
    last_chime: Option<DateTime<Utc>>,
}
```

**The ladder table** (t₀ = the occurrence's *effective instant* —
D-2; every constant a named `pub(crate)` const in `escalation.rs` so
the tests and any future config promotion cite one site):

| Channel | Schedule (from t₀) | Named constant | Volume |
|---|---|---|---|
| Fire toast (the engine's, E-20) | t₀ + one tick | — (not the driver's) | — |
| **Re-toast #1** | t₀ + 1 min | `RE_TOAST_FIRST: TimeDelta = 1 min` | — |
| **Re-toasts** | then every 2 min: t₀+3m, t₀+5m, … | `RE_TOAST_EVERY: TimeDelta = 2 min` | — |
| **First chime** | t₀ + 1 min | `CHIME_FIRST: TimeDelta = 1 min` | `0.30 + 0.70·(1m/5m)` ≈ 0.44 |
| **Chimes** | then every 30 s | `CHIME_EVERY: TimeDelta = 30 s` | linear ramp, see below |
| Ramp bound | full volume at t₀ + 5 min, capped after | `CHIME_RAMP: TimeDelta = 5 min` | `CHIME_LO = 0.30`, `CHIME_HI = 1.0` |

Volume at a chime instant c: `min(CHIME_HI, CHIME_LO + (CHIME_HI −
CHIME_LO) · (c − t₀min) / CHIME_RAMP)` where t₀min is the **earliest**
awaiting occurrence (D-4). Derivation rule per call: an action is due
iff its scheduled instant ≤ `now` and no equal-or-later action of the
same channel has been recorded in `memory` — and at most one action
per channel per call fires even if several instants piled up behind a
descheduling (bounded: one re-toast per occurrence per interval —
D-I9.3's own words). `next_due` is the earliest *unrecorded*
scheduled instant across all rules; `None` only when the set is empty
(the caller parks before stepping in that case — `None` is the
type-level assertion that an empty set never arms a timer).

The fire toast itself is the notifier lane's unchanged behavior; its
urgency hint arrives via the payload (§3.5). The driver's first action
against a fresh occurrence is the +1 min re-toast — re-read the
review fence: *"t0 fire, +1 min sound start, 2-min re-toast"* — the
re-toast ladder and the chime ladder both open at +1 min.

### 3.3 The rodio pin (D-I10) — verified at planning, per the house rule

**Pin** (root `Cargo.toml [workspace.dependencies]`, taskboard-app
consumes via `rodio.workspace = true`):

```toml
# Owned alarm audio (taskboard-app, DR-3/4 / D-I10 — stack approval
# GRANTED 2026-10-26). Minimal no-decode feature set, verified against
# rodio 0.23.0's published feature graph 2026-10-29: `playback` pulls
# cpal (output) alone; `recording`, `dither`/`noise` and the five
# symphonia decode formats (flac/mp3/mp4/vorbis/wav) stay out — the
# chime is synthesized in code (`buffer::SamplesBuffer` is an
# unconditional API). Pin discipline: the exact version is committed
# via Cargo.lock (Q12).
rodio = { version = "0.23.0", default-features = false, features = ["playback"] }
```

**Verification evidence (2026-10-29, crates.io API + published
crate source):**

- rodio **0.23.0**, rust-version **1.87** < workspace
  `rust-version = "1.96"` ✓; license **MIT OR Apache-2.0** ✓.
- Feature graph: `default = ["playback", "recording", "flac", "mp3",
  "mp4", "vorbis", "wav", "dither"]`; `playback = ["dep:cpal"]`;
  every decode format rides `symphonia-*`; `recording` adds `rtrb`.
  → `default-features = false, features = ["playback"]` compiles the
  mixer/queue/stream stack with **zero decoder crates**.
- The playback API this chunk uses (all re-verified in the 0.23.0
  source): `stream::DeviceSinkBuilder::open_default_sink() ->
  Result<MixerDeviceSink, DeviceSinkError>` (typed open failure — the
  degradation seam), `MixerDeviceSink::mixer() -> &Mixer`,
  `stream::play(&Mixer, source) -> Result<Player, PlayError>`,
  `Player::set_volume(Float)` (the ramp's volume surface),
  `Player::stop`/`skip_one`; `buffer::SamplesBuffer::new(channels,
  sample_rate, data)` is **not** feature-gated.
- Dependency tree under `playback` on Linux: cpal **0.18.2**
  (rust-version 1.85, Apache-2.0, **default features empty** — no
  JACK/PipeWire/PulseAudio forced) → `alsa` 0.11 (Apache-2.0/MIT),
  `alsa-sys` 0.4 (links the **system** libasound), `dasp_sample`
  0.11 (MIT OR Apache-2.0), `num-rational` 0.4 (MIT OR Apache-2.0)
  and its `num-*` friends, `libc`, `bitflags`, `thiserror` — every
  crate MIT OR Apache-2.0. **`cargo deny` outcome plan (D-15): zero
  new allowlist entries expected;** any license outside the existing
  allow set in the resolved tree is a named finding for stage 2 to
  resolve on the branch, never a silent allowlist edit. Per-target
  trees (Windows WASMAPI/macOS coreaudio bindings) are MIT OR
  Apache-2.0 by published metadata; the CI deny gate runs on the
  Linux dev/CI target, which is the tree CI actually compiles.
- **MSRV check**: `cargo check` under the workspace toolchain pins it
  empirically at stage 2; metadata says the binding floor is 1.87,
  six minor versions under the workspace's 1.96.

**The CI finding (D-15, must land in this chunk):** cpal's ALSA host
is **unconditional** on Linux (cpal 0.18.2 `platform/mod.rs:819` —
`Alsa` has no cfg gate) and `alsa-sys` links libasound at build time.
CI today installs libdbus + the slint stack but **no
`libasound2-dev`**: `ci.yml` (both apt blocks, ~:34 and ~:96),
`benchmark.yml` (:43) and `release.yml` (Linux leg, :89) each gain
`libasound2-dev` (pkg-config is already installed). The dev box has
alsa 1.2.15.3 (verified `pkg-config --exists alsa`). miri/kani/
mutation-testing workflows are untouched (they never build
taskboard-app).

### 3.4 The chime — synthesis in code, behind the driver

**`ChimeSink`** — the port-shaped confinement (D-I10's escape hatch):
all audio lives behind one trait in `escalation.rs`:

```rust
/// The audio seam: one chime at a volume. The only rodio touchpoint
/// in the workspace; a backend swap never touches surfaces/engine.
trait ChimeSink: Send + Sync + std::fmt::Debug {
    /// A failed chime is logged-and-dropped by the caller (typed, never fatal).
    fn chime(&self, volume: f32) -> Result<(), AudioError>;
}
```

- **Real impl** (`RodioChime`): constructed **only** when
  `AppMode::Desktop ∧ SinkKind::Desktop` (D-9), at driver-spawn time.
  Construction calls `DeviceSinkBuilder::open_default_sink()` **once**;
  a typed failure yields `ChimeSink::disabled()` — a no-op impl that
  logs once — the degrade-to-visual-only path (D-8). No retry for the
  process lifetime (the D-R13 never-retried discipline; process
  restart re-attempts).
- **Synthesis** (D-14): the waveform is a pure `fn chime_samples() ->
  Vec<f32>` — mono 44.1 kHz, ~0.6 s two-tone sine pattern (the
  E5→A5 shape; exact tones are a named-constant detail, asserted only
  structurally: length, peak ≤ 1.0, zero-crossing fade), 10 ms
  raised-cosine fade-in/out so the speaker never clicks. Built
  **once** into a `SamplesBuffer` at construction; per-chime is
  `stream::play(mixer, buffer.clone())` + `set_volume(volume)` —
  synthesis stays volume-free, one buffer cloned per chime. The
  latest `Player` handle is held (replacing any still-held one);
  `PlayError` is the typed failure class.

### 3.5 The DesktopSink urgency hint (D-I9.1) — payload data only

`ToastPayload` (the pure mapping, notifications.rs :242–269) gains
`critical: bool` = `notification.insistence == Insistence::Alarm`.
`DesktopSink::deliver` applies it at the builder:

```rust
if payload.critical {
    toast.urgency(<platform-critical>);   // Linux: Urgency::Critical;
}                                         // Win/macOS: InterruptionLevel::Critical
```

(the value expression is cfg'd per target — notify-rust already
provides the three per-OS `urgency()` overloads). The port signature,
`LogSink`, and the E-20 channel are untouched; the `DesktopSink` doc
comment's "no urgency hints" clause (a D-R12 sentence) is re-worded to
cite D-I9.1 — actions and sink-side sound still stay out (#444 fence).
`toast_payload`'s existing unit tests extend with the typed edge
(critical for `Alarm`, not for `Normal`); no text/label assertions.

### 3.6 The alarm window (D-I9.2) — a pure state-reader, mounted in chunk 4

**The .slint side**: `ui/alarm-window.slint` —
`export component AlarmWindow inherits Window { no-frame: true;
always-on-top: true; … }` — small, undecorated, listing one row per
awaiting `alarm` occurrence: the title snapshot, the elapsed phrase
(the existing `notification_phrase` contract), and Dismiss / Snooze /
Cancel (alarm anchors have no task: no Done, no Mute — the D-P11
anchor-aware shape). `AlarmRowModel` + `AlarmWindow` are re-exported
through `ui/app-window.slint` so the single `slint_build` compile
entry generates both. **`always-on-top` is the property whose
documented semantics are "on window managers supporting it"** — the
no-focus-steal ruling baked into the platform; on Wayland both
properties no-op by construction.

**The Rust side** (ui-slint crate; the app layer owns no windows):

- Pure derivations in `presentation.rs` (the dispatch acceptance: the
  open/closed predicate is *the same pure derivation, tested as
  such*):

  ```rust
  struct AlarmRow { alarm: AdhocAlarmId, reminder: ReminderRef,
                    occurrence: DateTime<Utc>, title: String }
  /// Awaiting alarm-insistent occurrences (the domain derivation,
  /// filtered one step — the driver's awaiting_alarms, presentation-shaped).
  fn alarm_window_rows(state, defaults, now) -> Vec<AlarmRow>;
  /// The open/closed predicate: open iff the awaiting set is not empty.
  fn alarm_window_open(rows: &[AlarmRow]) -> bool;
  /// The attention edge: request when the set GAINED an occurrence.
  fn attention_due(rows: &[AlarmRow], previous: &[AlarmRow]) -> bool;
  ```

  (`alarm_window_rows` deliberately does not grow DR-3/3's
  `NotificationRow` or reuse `notification_rows` — the anchor-card
  filter and row shape differ; each surface keeps one derivation over
  the same domain function, nothing second-evaluates.)
- **Mounting**: `mount` creates the `AlarmWindow` lazily **iff
  `!settings.kiosk`** (the existing `UiSettings.kiosk` gate — the
  desktop hand-over sets `kiosk: false`, the kiosk appliance sets
  `true`; daemon mode mounts no UI at all, D-9). Held as a
  `slint::Weak<AlarmWindow>` beside the other timers in `UiHandle`.
- **The pass** rides the existing repaint driver's two existing wake
  branches — `dirty` (a state change) and the wall-minute boundary
  (time passage, lib.rs :454): derive `alarm_window_rows` from the
  `ArcSwap` snapshot; open iff `alarm_window_open` (show, idempotent)
  else hide; request attention when `attention_due` **or** on each
  wall-minute pass while the set stays non-empty (the re-raise: the
  level-triggered nag mirrors the driver's re-toast spirit; winit
  coalesces). The pass replaces the previous row set it held.
- **Attention request** (D-6):
  `slint::winit_030::WinitWindowAccessor::with_winit_window(|w|
  w.request_user_attention(Some(UserAttentionType::Critical)))` —
  **never `focus_window`** (a grep fence for stage 3). The
  `unstable-winit-030` slint feature is added to the workspace pin;
  the types come through slint's own re-export, so **no direct winit
  dependency enters any manifest** and the ui-slint crate stays
  presentation-only. Non-winit backends (the testing platform,
  linuxkms) fail `has_winit_window()` → the channel no-ops
  structurally — the origin §6 risk ("if the slint version lacks it
  on a target, that channel no-ops") taken exactly as written.
- **Buttons** dispatch through the window's own callbacks wired at
  creation to the same `mpsc::Sender<StateCommand>` the adapter
  holds (`DismissReminder`, `SnoozeReminder`, `CancelAdhocAlarm` —
  the D-I8 reuse ruling; no new commands, no keymap interception — a
  mouse-only surface).

### 3.7 Mechanical fall-out (compile-level, not design)

- `App` struct + shutdown: one field, one destructure arm, one
  `join_bounded`, one `|` in the budget aggregation (§3.1).
- `crates/taskboard-app/Cargo.toml`: `rodio.workspace = true`.
- Root `Cargo.toml`: the rodio pin + the `unstable-winit-030`
  feature on the slint pin (comment updated per the pin-discipline
  pattern).
- The four CI apt blocks (§3.3).
- `Cargo.lock`: regenerated (the cpal/alsa tree, ~40 entries).

## 4. Recorded decisions (every open question resolved)

| ID | Question | Decision |
|---|---|---|
| D-1 | rodio pin shape | `rodio = { version = "0.23.0", default-features = false, features = ["playback"] }` — exact version req per the notify-rust precedent; no-decode verified against the published feature graph (§3.3); MSRV 1.87 ≤ 1.96; deny plan: no new allowlist entries expected |
| D-2 | What is t₀ | The **effective occurrence instant** (pure — derivable from state alone, no observed-at memory). Leniency: awaiting-duration is measured from the occurrence, so delivery delay ≤ one engine tick makes the ladder open up to one tick early; accepted, recorded here |
| D-3 | Re-toast mechanics | **withdraw-then-deliver** through the sink port — the tracked-handle map's insert-overwrite becomes a real close+replace at the OS boundary where close is supported, and handle-level replacement everywhere; the fake sink pins the order |
| D-4 | Chime: per-occurrence or global | **Global** — sound is one channel; N awaiting alarms produce one chime stream, scheduled and volume-ramped off the **earliest** awaiting t₀. Re-toasts stay per-occurrence |
| D-5 | Burst coalescing | At most one action per channel per `escalation_step` call even when several scheduled instants piled up (a descheduled driver never bursts) |
| D-6 | The attention channel | `unstable-winit-030` on the slint pin + `WinitWindowAccessor` → `request_user_attention(Critical)`; **never focus**; non-winit/Wayland = structural no-op (origin §6's anticipated risk, accepted); re-asserted per wall-minute while non-empty + on the gained-edge |
| D-7 | The window's mount shape | A **second slint `Window`** (`AlarmWindow`, `always-on-top` + `no-frame`), exported through the app-window compile unit (one `slint_build` entry), created iff `!settings.kiosk`; the ProbeWindow tests prove second-window instantiation works on the testing backend |
| D-8 | Audio failure semantics | `open_default_sink()` once at construction (Desktop∧Desktop-sink only); typed failure → `ChimeSink::disabled()` no-op, logged **once**, visual-only for the process lifetime; a per-chime `PlayError`/`AudioError` is logged-and-dropped — never fatal, never a panic (the D-R13 stance) |
| D-9 | The mode gates | Driver spawns in **every** mode (re-toasts ride whatever sink resolved — log lines under `LogSink` are the daemon's honest toast lane); **audio constructs only** when `AppMode::Desktop ∧ SinkKind::Desktop` (belt-and-braces over the explicit-desktop-under-daemon cell of the D-R13 matrix); **the window only** when a UI is mounted with `kiosk = false` |
| D-10 | Driver exit + the unexpected-engine-death case | Exit on broadcast `Closed` (sender holders: the engine task + `EngineHandle`); `Lagged` counts as change (the UI's recorded semantics). If the engine task dies unexpectedly mid-run, `EngineHandle` still holds a sender → the driver parks until shutdown; accepted — the bounded join bounds it, and the notifier's channel-close luck is an mpsc artifact the broadcast lane doesn't share |
| D-11 | Shutdown integration | `escalation_join` bounded-joined **after the engine join, beside the notifier**; `ShutdownOutcome::budget_hit` grows one term |
| D-12 | Ladder memory lifetime | `LadderMemory` retains only awaiting keys — cleared whenever the derived set is empty, per-key-evicted on each step; a dismissed/snoozed occurrence's cadence dies with its membership |
| D-13 | The audio confinement | One `ChimeSink` trait in `escalation.rs`; the real impl is the workspace's only rodio touchpoint; tests inject a recorder — no audio device in any unit test |
| D-14 | Chime synthesis | Pure `fn chime_samples() -> Vec<f32>`: mono 44.1 kHz ~0.6 s two-tone sine, 10 ms raised-cosine fades, built once; volume applied via `Player::set_volume` at chime time (synthesis volume-free) |
| D-15 | The CI/ALSA finding | `libasound2-dev` added to the four Linux apt blocks (ci.yml ×2, benchmark.yml, release.yml); deny: zero new allowlist entries expected, any surprise is a named finding |
| D-16 | What this chunk does not touch | `messages.rs` (zero-diff fence), `command.rs`/`persistence.rs`, any migration/schema, DR-3/3's landed row shapes, any `[reminders]` config key, the backlog issues (stage 3 closes #477 + #203) |

## 5. Test plan (TDD-first; stage 2 lands red-green pairs as standalone commits)

**Milestone 1 — the pure ladder (no tokio, no I/O).**

- `first_re_toast_opens_one_minute_after_the_occurrence`: step at
  t₀+59 s → no actions, `next_due = Some(t₀+1m)`; step at t₀+1m →
  one `ReToast`, memory records it.
- `re_toasts_hold_the_two_minute_cadence`: at t₀+3m, +5m, +7m — one
  per step, none between; a second step at the same `now` → zero
  actions (the memory transition).
- `first_chime_opens_one_minute_after_the_earliest_occurrence` and
  `chimes_repeat_every_thirty_seconds`: global channel — two awaiting
  occurrences 10 s apart → the chime schedule keys off the earlier
  t₀, and one chime serves both.
- `volume_ramps_thirty_to_hundred_percent_over_five_minutes_capped`:
  chime at t₀+1m ≈ 0.44, at t₀+3m ≈ 0.72, at t₀+4m50s ≈ 0.995, at
  t₀+6m == 1.0 (exact `f32` compares via the formula's own
  evaluation — structural bounds asserted, no float-equality traps).
- `a_descheduled_driver_never_bursts`: no step between t₀+1m and
  t₀+4m, then one step → exactly one re-toast + one chime (D-5),
  `next_due` lands on the next boundary.
- `memory_dies_with_the_awaiting_set`: dismiss one of two → its
  re-toast schedule stops, the other continues; empty set → memory
  empty (D-12).

**Milestone 2 — the fake sink + the driver loop (tokio paused time;
no real sleeps).**

- The `FakeSink` (the `CollectingSink` pattern, notifications.rs
  :416): records `(deliver | withdraw, reminder, occurrence)` in
  order.
- `re_toast_is_withdraw_then_deliver`: apply one `ReToast` → the
  record is `[Withdraw, Deliver]` for the same key (D-3 —
  replacement-not-stacking pinned at the lowest layer).
- `failed_delivery_does_not_kill_the_driver`: sink errs on deliver →
  the loop continues to the next cadence action (the D-R13
  consumer discipline, re-pinned one layer up).
- `the_driver_wakes_only_while_an_alarm_awaits`: fake clock +
  paused tokio time — a state holding no awaiting alarm: advance 10
  min of tokio time → zero sink calls, zero ladder wakes (parked);
  swap in a state with one awaiting `alarm` + send `StateUpdated` →
  the ladder arms; advance to t₀+1m → one re-toast; dismiss (swap
  state) + signal → advancing further produces nothing and the
  memory is empty.
- `normal_insistence_never_escalates`: an awaiting `Normal`
  notification in state → parked, no actions (the rung's whole point,
  pinned at the loop).
- `the_driver_exits_when_the_broadcast_closes`: drop the signal
  sender → the join resolves (D-10).

**Milestone 3 — audio seam + chime synthesis (no audio device).**

- `audio_init_failure_degrades_to_visual_only`: the real-impl
  construction is fed a failing `open` seam (the typed
  `DeviceSinkError` class) → `ChimeSink::disabled()`, a `chime`
  call is `Ok`-ish-no-op from the caller's view (typed outcome, not
  a panic — the dispatch acceptance), and the re-toast cadence is
  unaffected.
- `chime_samples_are_structurally_sound`: length = rate·duration,
  peak ≤ 1.0, the fade windows are sub-peak, the two tones
  dominate the two halves (structural — the tones themselves are
  never asserted as text/frequency literals beyond the named
  constants).

**Milestone 4 — the urgency hint (payload only).**

- `alarm_insistence_marks_the_payload_critical` /
  `normal_insistence_keeps_the_default_urgency`: `toast_payload`
  typed edges (AGENTS §5 — no label text, no bus).

**Milestone 5 — the alarm window (pure fns + mounting).**

- `alarm_window_rows_follow_the_domain_view_filtered_to_alarm`:
  constructed states — an awaiting `alarm` timer, an awaiting
  `Normal` rule, a dismissed alarm, a muted task → exactly the
  alarm-level rows, canonical order (mirrors
  `notification_rows_follow_the_domain_view…`, presentation.rs
  :1253).
- `alarm_window_open_is_the_nonempty_predicate` and
  `attention_is_the_gained_edge`: empty→non-empty = open + attention;
  non-empty→non-empty (same set) = open, no attention; →empty =
  closed (the dispatch's "same pure derivation, tested as such").
- Mounting glue stays out of unit assertions except through the
  existing flow harness: the pass runs, the window's `rows` model
  updates, `show`/`hide` invoked on the testing backend (the
  ProbeWindow precedent) — structural, no screenshots.

**Gates** (AGENTS §10, via `scripts/run-quality-gates.sh`):
`cargo deny check` is the rodio tree's gate (D-1/D-15 outcome plan);
the MSRV job compiles the new dep under 1.96; fmt/clippy/nextest/doc
as always. Insta: untouched (D-16 — not a shape-widener).

## 6. Risks & mitigations

- **ALSA breaks CI before anything works** (the pin lands in the
  same PR as the workflow edits — D-15; verified locally:
  alsa 1.2.15.3 present; worst case the pin is feature-inert until
  the apt blocks land, but they land together by plan).
- **`unstable-winit-030` drift** (named unstable by slint): the
  channel is one call site behind the accessor; a slint bump that
  breaks it degrades that channel to a no-op — the origin §6 ruling
  taken structurally; the other three channels carry the rung.
- **Wayland: always-on-top and attention both no-op** — by ruling:
  no focus steal is a *requirement* there, and re-toast + sound +
  the in-app row still escalate. GNOME/KDE documented first per the
  origin risk table.
- **`Player`/`MixerDeviceSink` API drift in rodio 0.23.x minor
  bumps**: the exact version is lock-pinned (Q12); the confinement
  (one trait, one impl) bounds any future re-pin's blast radius.
- **Toast stacking on servers that ignore CloseNotification**: the
  pin is handle-level replacement + withdraw-then-deliver; where the
  server cannot close, the platform matrix's fire-and-forget stance
  already applies — never asserted as a quality ceiling (AGENTS §5).
- **The driver + notifier double-toast at t₀+1m**? No: the engine's
  fire toast is a one-shot E-20 delivery at the firing tick; the
  driver's first action is a *re*-toast at +1 min — distinct
  instants, distinct lanes, and the tracked-key replacement makes
  the second one replace the first where the platform supports it.
- **A parked driver misses a state change if a `StateUpdated` is
  lost**: broadcast `Lagged` is treated as change (the UI's recorded
  semantics), and every mutation path publishes — the same contract
  the whole UI repaint lane stands on.

## 7. Estimated diff size

| Site | Lines (incl. tests/comments) |
|---|---|
| Root `Cargo.toml` (rodio pin + slint feature) | ~+10 |
| `crates/taskboard-app/Cargo.toml` | +1 |
| app `escalation.rs` (new: ladder, driver, ChimeSink, synthesis) | ~430 (≈ 45% tests) |
| app `notifications.rs` (payload urgency + doc + tests) | ~+35 |
| app `bootstrap.rs` (spawn, App field, shutdown join) | ~+40 |
| ui-slint `presentation.rs` (AlarmRow, 3 fns + tests) | ~+90 |
| ui-slint lib/window module (mount, the pass, attention + tests) | ~+150 |
| `ui/alarm-window.slint` (new) + app-window.slint export | ~+95 |
| `.github/workflows` (4 apt blocks) | +4 |
| `Cargo.lock` (generated) | ~+60 |

**Total ≈ 900–1000 lines, roughly 40% tests — size M**, matching the
dispatch's wave-8 slot.

## 8. References

- Mandate: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (wave 8), §2 (resource ladder), §3.2 (DR-3 binding notes),
  §11.3 (this chunk's prompts).
- Design authority: `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`
  §3 Part C item 3, §3 Part E (D-I9/D-I10), §5 (driver hooks), §6
  (rodio/Wayland risks), §7 (chunk 4 = issue #477).
- Semantics baseline: DR-1 R3
  (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  §4-R3) — settlement/dismissal model this chunk's awaiting set
  reads.
- Sibling chunks already landed: DR-3/2 (the `Insistence` payload
  field this chunk consumes — PR #542) and DR-3/3 (the surfaces the
  window's rows mirror — PR #529).
- Issues: #477 (this chunk), #203 (closed with it per stage 3's
  prompt), #444 (native toast buttons — fence), #475/#476 (the
  gates).
- Stack evidence: crates.io rodio 0.23.0 / cpal 0.18.2 / alsa 0.11.0
  metadata + published sources (verified 2026-10-29, §3.3);
  i-slint-compiler 1.18.1 `builtin_elements.rs` (`always-on-top`
  :2104, `no-frame` :2133); slint 1.18.1 `lib.rs` (`unstable-winit-030`
  :663); notify-rust 4.18.2 `notification.rs` (the three `urgency()`
  overloads).
