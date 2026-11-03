# [Plan] Pomodoro focus mode & concentration overlay (#496, backlog round 2 chunk 5.1) — stage-1 implementation plan

- **Date**: 2026-11-03 (written into the mandated
  `2026-10-31-pomodoro-focus-plan.md` slot of the round-2 dispatch
  table).
- **Kind**: stage-1 TDD implementation plan for chunk **5.1** of the
  backlog round-2 dispatch. Zero production code in this commit; this
  file is the chunk's entire planning artefact. Implementation is
  stage 2's, review + shipping stage 3's.
- **Origin**: the round-2 dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` (batch
  5, chunk 5.1, branch `feat/pomodoro-focus`); the normative fence
  issue **#496** — "a focus-session runtime (app layer — the natural
  home is a state-machine task beside the DR-3 escalation driver,
  driving the existing tracking verbs: session start = `StartTracking`,
  session end = stop) plus the busiest single view of the brainstorm
  (countdown, current task, switcher, quick-note, exit; configurable
  timings). DR-5's open-entry model composes: a session is an open
  entry; the overlay reads it from the published state."
- **Design-round base**: the accepted DR-5 time-tracking plan
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md` —
  its fence note is the binding composition guidance: the open entry
  (`end: null`) **is** the running timer (D-T1), *nothing ticks*
  engine-side, single-global intent (D-T8), the reserved `kind`
  vocabulary landed by chunk 3.5, and the kiosk posture precedent
  (D-T16: the chip renders read-only; actions stay desktop). The
  DR-3 escalation driver's **landed** implementation
  (`crates/taskboard-app/src/escalation.rs`, commit `3d7feb7b`) is the
  runtime-task template this chunk rides beside.
- **Normative base**: branch point = `origin/main` @ `5d8c0751` (the
  worktree's base). **Chunk gates verified** (2026-11-03): chunk 4.1's
  issue **#461** is CLOSED and its PR **#594**
  (`perf/ui-slint: the Today sections virtualise`) is MERGED
  (2026-11-03T10:36:21Z, merge commit `c38f5a28`, verified an ancestor
  of `origin/main`); the DR-3 escalation driver is landed
  (`3d7feb7b feat(app): the escalation driver, the owned chime, the
  critical hint`, in `origin/main`). Note: the dispatch prompt says
  "chunk 4.1's PR (#461)" — #461 is the *issue*; the carrying PR is
  #594. Gate intent (4.1 merged) holds.
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§6, D-F1…D-F21). The two findings
  this planning pass contributes beyond the fence issue are the
  **kind-blind no-op land-mine** (D-F2: the landed `StartTracking`
  idempotent no-op keys on the task, not the kind — a single-dispatch
  focus→break hand-off would be silently swallowed, so the runtime's
  hand-off is the two-verb sequence `StopTracking` →
  `StartTracking { kind: Respite }`) and the **mode-exclusion ruling**
  (D-F13: `[focus] enabled` spawns the runtime on Daemon and Desktop
  but never on Kiosk — the AlarmWindow structural precedent).

## 1. Mission & binding constraints

One feature, two halves, one truth source:

1. **The focus-session runtime** (app layer, new module
   `crates/taskboard-app/src/focus.rs` beside `escalation.rs`): a
   state-machine task that observes the published `AppState` and, when
   a focus-mode session is live, drives the phase transitions through
   **exactly** the landed tracking verbs — `StartTracking` /
   `StopTracking`. It owns no time-tracking truth of its own: the
   open entry *is* the session.
2. **The concentration overlay** (`taskboard-ui-slint`): a full-surface
   in-window overlay — countdown, current task, switcher, quick-note,
   exit affordances — rendered exclusively from published state (the
   dumb-client line), with configurable timings validated at the
   config boundary.

Binding constraints (each verified against the tree at the base):

- **The session drives only the existing tracking verbs.** The runtime
  dispatches `StateCommand::StartTracking { task, kind }` and
  `StateCommand::StopTracking` and nothing else — no `LogTime`, no
  `AdjustTimeEntry`, no direct entry minting, no new write path. A
  parallel time-tracking path is the bug class the DR-5 fence exists
  to prevent; stage 2 greps the diff for any second path.
- **The tracked entry is truth.** The runtime derives its phase from
  `AppState.tracking.newest_open()` (`tracking.rs:182-185`) at every
  wake; it never persists session state, never re-opens what the user
  closed, never fights an externally-driven change (reconciliation
  semantics: D-F16).
- **No second clock source.** The runtime inherits the app's injected
  `Arc<dyn Clock>` (the escalation driver's last parameter,
  `escalation.rs:487`; wired `bootstrap.rs:1109` beside the engine's
  `bootstrap.rs:1063`). Every due computation reads `clock.now()`;
  wakeups arm `tokio::time::sleep((due − now))` — tokio time is the
  *wakeup* mechanism (pausable in tests), never a time *source*. No
  `Utc::now()`, no `Instant::elapsed` in the runtime. Journeys use
  `#[tokio::test(start_paused = true)]` + a fake `Clock` +
  `advance`/`yield_now` (the `escalation.rs:1030-1101` pattern).
- **The overlay renders published state only.** No session truth in
  the UI layer: countdown, phase, and current task derive per frame
  from the `ArcSwap` snapshot + the injected `FocusTiming` datum + the
  render-time `now` the push pass already carries (the status chip's
  elapsed precedent, `models.rs:1497-1514`). ADR 0015's dumb-client
  rule untouched.
- **"Nothing ticks" engine-side** (ADR 0017 / DR-5 §3) is preserved:
  no engine change, no new channel topology, no `SystemEvent`/
  `EngineSignal` variant, no new tick arm — the escalation fence
  (`escalation.rs:8-15`) verbatim. The runtime parks on the existing
  `EngineSignal` broadcast and arms a private `tokio::time::sleep`
  only while a phase is live.
- **Typed config validation at the boundary (A1)** (AGENTS §2): the
  timing values are a hand-rolled private-field newtype with a
  fallible mint; config stores raw fields, `AppConfig::validate`
  rejects bad values as a new typed `ConfigError` variant with a
  stable `kind()` discriminant; no derive crates; no validate-and-
  forget (the mint is called again at the mapper, expect-on-validated
  contract).
- **No schema migration, no new dependencies** (the dispatch's §1.4
  expectation holds): sessions are ordinary `TimeEntry` rows (the
  landed `time_entries` table and sidecar section carry them);
  `FocusTiming` is injected data (the `ReminderDefaults` precedent —
  never persisted, never roamed); `cargo deny` untouched.
- **No quality ceilings, no text assertions, deterministic tests**
  (AGENTS §5): journeys assert terminal published state; the overlay
  goldens are data; countdown assertions are arithmetic-structural.
- **Non-goals (fences)**: auto-chain of the next focus after a break
  and the every-N long-break cadence (D-F4 — fenced as an issue idea);
  phase-boundary toast/chime notifications (D-F14 — fenced; the
  escalation ladder is the future template); any new CLI/IPC verb (the
  landed `track` family already drives and observes sessions); a
  settings-view editor for the timings (config file + env are the
  surface); pausing (the landed verb set has no pause; a stopped entry
  ends the session); roaming focus preferences (D-F17); embedding the
  search palette inside the overlay (D-F10); per-device focus state in
  the sidecar (nothing new roams).

## 2. The runtime state machine — D-F1…D-F3, D-F15, D-F16

### D-F1 — the phase derivation: stateless, over the published open entry

The runtime holds **no bookkeeping memory** — not even the escalation
driver's `LadderMemory`. The entire machine is one pure function over
`(state, timing, now)`:

```rust
// crates/taskboard-app/src/focus.rs
/// One focus-mode action the runtime is due to take.
pub(crate) enum FocusAction {
    /// The work block is over: hand off to the break (two verbs, D-F2).
    OpenBreak { task: TaskId },
    /// The break is over: close it (one verb).
    EndBreak,
}

pub(crate) struct FocusStep {
    pub(crate) actions: Vec<FocusAction>,      // at most one per step
    pub(crate) next_due: Option<DateTime<Utc>>, // None ⇒ park on signals
}

pub(crate) fn focus_step(open: Option<&TimeEntry>,
                         timing: &FocusTiming,
                         now: DateTime<Utc>) -> FocusStep
```

Derivation (`open` = `state.tracking.newest_open()` at the call site —
the D-T8 reader pick; a transient double-open is governed by the
newest entry exactly as every other reader):

| Observed open entry | Phase | Deadline | Due (`deadline ≤ now`) |
|---|---|---|---|
| `None` | Idle | — | no actions, `next_due: None` (park) |
| `Some(e)`, `e.kind == Work` | Focus | `e.start + timing.work()` | `OpenBreak { task: e.task }` |
| `Some(e)`, `e.kind == Respite` | Break | `e.start + timing.respite()` | `EndBreak` |

Not-due ⇒ `next_due = Some(deadline)`, no actions. The deadline
anchors on the **entry's `start`**, never on a runtime-remembered
instant — a foreign or replacement entry re-anchors the session for
free (D-F16), and a restart mid-session re-derives everything from
the rehydrated row (the ADR 0017 crash-safety property, consumed
verbatim).

Why `newest_open()` is the session: the DR-5 fence note says "a
session IS an open entry; the overlay reads it from published state".
With focus mode **opted in** (D-F3), every open entry is a session
block by definition — Work is a focus block, Respite is a break
block. The kind vocabulary landed by chunk 3.5 (#493) is the
discriminator; no new marker, flag, or wire field is added.

### D-F2 — transitions ride exactly the landed verbs; the kind-blind no-op land-mine

**Verified finding (load-bearing)**: the landed `StartTracking`
planner arm's idempotent repeat is **task-keyed, kind-blind**
(`command.rs:2132-2140`: `open.len() == 1 && open[0].task == *task`
→ `NoOp`, regardless of the payload's `kind`). A single-dispatch
hand-off `StartTracking { task, kind: Respite }` fired while the
task's Work entry is open would therefore be **silently swallowed** —
the break would never open. The runtime's focus→break hand-off is
consequently the **two-verb sequence**, each awaited to its receipt:

1. `StopTracking` → `CommandOutcome::TrackingStopped { entries }`
   (closes the Work entry at engine `now`; nothing-open is the
   accepted no-op, so a state surprise degrades safely).
2. `StartTracking { task, kind: Respite }` →
   `CommandOutcome::TrackingStarted { entry, closed }` (opens the
   break entry; `entry.start` is the break's anchor datum).

Break→idle is one verb: `StopTracking`. Starting a focus block is
**never the runtime's act** — the user starts it from any landed
surface (overlay, chip, context menu, notification Start Task, CLI);
the runtime only ever ends phases. Consequences, each recorded:

- The two-dispatch window between (1) and (2) is bounded by the
  driver task's sequential awaits; `track status` in that window shows
  nothing open — cosmetic, honest, sub-second.
- The kind-blind no-op also means a user pressing Start on the
  switcher for the task whose break entry is open gets a `NoOp` (the
  next focus can't be started on the same task until the break ends).
  Acceptable v1; the general fix (make the no-op kind-aware, matching
  chunk 3.5's documented D-3 intent "opens the running break") is a
  landed-behavior change in the respite family's scope — **fenced as
  an issue idea** (§9 R-2), not snuck in here.
- Every `StartTracking` also fires the landed implicit-dismissal
  side effect (D-T5) for the hand-off task's active occurrences — at
  each focus→break hand-off the task's live reminders dismiss. That is
  the landed semantic applied consistently (starting the break entry
  on the task is still "engaging with the task"); recorded, accepted.
- The dispatched verbs are undoable exactly like manual ones (the
  landed tracking undo arms, #555) — nothing new.

### D-F3 — opt-in mode: `[focus] enabled`, default false

The runtime is spawned **only** when the user opted in
(`[focus] enabled = true`) **and** the run mode is not Kiosk (D-F13).
Rationale: without focus mode, an open entry is just tracking — the
last thing a time tracker needs is a 25-minute auto-stop hijacking
ordinary tracked blocks. Opt-in is the hijack containment; the mode's
meaning once enabled is "my open entries are focus blocks". The
disabled posture: no runtime task, no overlay (the toggle action is
disabled in `entry_states`), no config surface beyond the section's
presence, `[focus]` absent from the TOML = disabled (the
`web`-section precedent: absent table decodes disabled and serializes
nothing).

### D-F15 — the driver loop (the escalation template, receipts instead of sinks)

```rust
pub(crate) fn spawn_focus_runtime(
    state: Arc<arc_swap::ArcSwap<AppState>>,
    timing: FocusTiming,
    engine: taskboard_state::EngineHandle,
    mut signals: broadcast::Receiver<EngineSignal>,
    clock: Arc<dyn Clock>,
) -> tokio::task::JoinHandle<()>
```

The loop is `escalation.rs:489-527`'s shape with the engine handle in
the sink role:

- Loop top: `let now = clock.now(); let step = focus_step(&state.load(), &timing, now);`
- **Actions fire through `EngineHandle::execute`** — the receipt-awaiting
  send (`actor.rs:131-141`; the `commands/track.rs:184-233` call
  shape). The reply implies durability; `ExecuteError::EngineGone` is
  logged-and-continues (the broadcast `Closed` arm is the honest
  exit).
- **The post-action wake derives from the receipt, not from a fresh
  state read** — the oneshot reply can resolve before the engine's
  `ArcSwap` store publishes (`actor.rs` reply-after-apply vs the
  publish pass), so a state-derived re-step could fire on stale data.
  After `TrackingStarted { entry, .. }` the loop arms
  `entry.start + timing.respite()`; after `TrackingStopped` it parks.
  This is the escalation driver's memory-backed schedule role: the
  receipt is a *wakeup hint*, never truth — every signal-wakened step
  still re-derives from published state (D-F1), so a surprise between
  dispatch and wake is corrected at the next step, never fought.
- **Park**: `focus_step` ⇒ idle ⇒ `signals.recv().await` with **no
  timer armed** (`Ok | Lagged` ⇒ re-step; `Closed` ⇒ exit — the
  escalation semantics verbatim, including Lagged-counts-as-change).
- **Live**: `tokio::select!` over `sleep((next_due − now).to_std().unwrap_or_default())`
  and `signals.recv()` — the `escalation.rs:516-525` shape; both time
  values come from the injected `Clock` read (D: no second clock).
- Span: `tracing` spans across the awaits; dispatch failures log at
  `warn!` and continue (one lost transition never kills the task —
  the D-R13 consumer discipline).

Defense-in-depth note (recorded, not relied on): even a duplicate
hand-off raced through the stale-state window converges — the
interleaved `StopTracking` is a no-op against engine truth and the
`StartTracking { kind: Respite }` opens exactly one break (nothing
open engine-side). The receipt-derived schedule is still the pinned
shape; the convergence property is the safety net, not the design.

### D-F16 — reconciliation: the entry is truth, the runtime follows

Each external perturbation, and the pinned test for it:

| Perturbation mid-phase | Observed at the next wake | Runtime behavior |
|---|---|---|
| User stops the entry (chip/CLI/task view) | `newest_open() == None` | Idle; park. **Never re-opens.** |
| User starts another task | new Work entry, new `start` | Phase re-anchors at the new `start`; the old deadline is forgotten |
| User starts a Respite entry manually (`track start --kind respite`) | Respite entry | Break phase anchored at its `start`; `EndBreak` due at its deadline |
| Task deleted mid-focus (`DeleteTask` closes the entry, D-T6) | `None` | Idle; park |
| Entry long past due at boot (stale from a dead process) | deadline ≪ now | **One** reconciling transition at engine `now` — no backdating, no cascade of missed cycles (the D-T8 write-maintained invariant; the runaway cap stays declined in DR-5's sense) |
| Transient double-open (D-T8 merge race) | two open rows | `newest_open()` pick governs; the next write closes all |

## 3. Timing configuration — D-F5…D-F7

### D-F5 — the `[focus]` section: plain integer minutes, validated in the flat ladder

Follows the `[preferences]` doc's contract (config.rs:336-343: raw
stored, parsed in `validate`, "a bad value must surface as the typed
… kind, never a silent default") with the `[sync]` plain-numbers
precedent (config.rs:197-198 — no string duration grammar; minutes
are the pomodoro idiom):

```rust
/// Pomodoro focus mode (#496). Absent table = the mode is off (the
/// `web` section's present/absent pattern).
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FocusSection {
    #[serde(default, skip_serializing_if = "is_default")]
    pub enabled: bool,
    #[serde(default = "default_focus_work_mins", skip_serializing_if = "…")]
    pub work_mins: u64,
    #[serde(default = "default_focus_break_mins", skip_serializing_if = "…")]
    pub break_mins: u64,
}
```

Defaults `DEFAULT_FOCUS_WORK_MINS = 25`, `DEFAULT_FOCUS_BREAK_MINS =
5` as named consts at the file's top (the `DEFAULT_*` convention),
hand-written `Default` for the section, `AppConfig` gains
`#[serde(default)] pub focus: FocusSection`. Validation in
`AppConfig::validate`'s flat ladder: when `enabled`, each of
`work_mins`/`break_mins` must be in `1..=240` (strictly positive — a
zero work block would auto-stop instantly; the 240-minute ceiling is
a garbage-input bound, not a quality ceiling). Rejection is a new
typed variant `ConfigError::InvalidFocusTiming { field: &'static str }`
whose `kind()` is the new stable discriminant **`"invalid-focus-timing"`**
(extends the `the_new_kinds_extend_the_stable_discriminants` pin,
config.rs:2643). Raw values stay stored so `config print` echoes the
user's spelling (the section day-start contract). Validation runs on
the values whenever present-and-enabled; disabled sections with
garbage values are still rejected (deterministic validation — validate
what is written, not what is live).

### D-F6 — `FocusTiming`, the A1 newtype, in `taskboard-domain/src/focus.rs`

The runtime (app) and the overlay (ui-slint) both consume the
timings, and ui-slint depends only on domain — so the type lives in
**domain**, beside its siblings `WorkThreshold` (`tracking.rs:246`)
and `ReminderOffset` (`reminders.rs:34`):

```rust
/// Focus-mode block durations (issue #496). Private field; the
/// fallible mint is the config-load trust boundary — a zero-length
/// block can never enter a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTiming {
    work: chrono::TimeDelta,
    respite: chrono::TimeDelta,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidFocusTiming { /* per-field variants, typed */ }

impl FocusTiming {
    pub fn try_new(work: TimeDelta, respite: TimeDelta) -> Result<Self, InvalidFocusTiming>;
    pub fn work(&self) -> TimeDelta;
    pub fn respite(&self) -> TimeDelta;
}
```

Strictly positive, whole-second durations (the `WorkThreshold::try_new`
rules). **No serde impl** — injected config data, never persisted and
never roamed (the `ReminderDefaults` doc precedent, reminders.rs:466).
The 240-minute ceiling lives at the config layer (policy, not
integrity); the type enforces only positivity (integrity). New file
carries the `MIT OR Apache-2.0` SPDX header; the crate's
`forbid(unsafe_code)` is already blanket.

### D-F7 — the hand-over seam: `focus_config()` mapper, `UiSettings.focus`

- `AppConfig::focus_config() -> Option<FocusTiming>` — `None` when
  `!enabled`; otherwise mints via `FocusTiming::try_new(TimeDelta::minutes(...), …)`
  with `.expect("focus timing validated")` — the day-start-time
  expect-on-validated contract (config.rs:1322-1333), never callable
  on an unvalidated config.
- Bootstrap: `let focus = config.focus_config();` then, after the
  escalation driver spawn (`bootstrap.rs:1103-1110`) and before the
  connector registry loop (subscribed before the connectors so no
  `StateUpdated` races the first park — the escalation comment,
  `bootstrap.rs:1092-1095`):

  ```rust
  let focus_join = if let (Some(timing), true) =
      (focus.clone(), config.app.mode != AppMode::Kiosk) {
      Some(spawn_focus_runtime(
          engine.shared_state(), timing, engine.clone(),
          engine.subscribe(), Arc::new(taskboard_domain::SystemClock)))
  } else { None };
  ```

  `App` gains `pub focus_join: Option<tokio::task::JoinHandle<()>>`
  beside `escalation_join` (bootstrap.rs:378-380) for symmetric
  shutdown handling.
- UI hand-over: `UiSettings` (ui-slint `lib.rs:66`) gains
  `pub focus: Option<taskboard_domain::focus::FocusTiming>` (default
  `None`, overwritten app-side right after construction — the
  reminder_defaults precedent, ui.rs:428 / main.rs:797). `None` ⇒ the
  mode is off ⇒ the overlay action is disabled; `Some` ⇒ the overlay
  derives countdowns from it. The adapter holds a copy beside
  `day_start`.

## 4. The concentration overlay — D-F8…D-F12, D-F18…D-F20

### D-F8 — surface ruling: an in-window `Flow::FocusMode` overlay, not a second window

The one-window architecture rule (`app-window.slint:2-9`: "one view
tree: view switching is a property, not windows") plus the in-window
overlay precedent row (menu :496-555, palette :598-649, capture
:611-636 — scrim `TouchArea` + panel as the last siblings of the key
scope) decide it: the concentration overlay is a **full-surface
scrim + panel** toggled by a new flow, `Flow::FocusMode { selected:
usize }` in `view_state.rs`'s `Flow` enum (the `Palette { selected }`
shape — subject-free, `reconcile` leaves it alone). The flows are
XOR: entering the overlay suspends the workspace view beneath (which
is the point of concentration mode) and any other flow (opening
capture or the palette closes the overlay). The `AlarmWindow`
second-window pattern was considered and declined: it exists because
alarms must interrupt *any* work context (no-frame, always-on-top);
the concentration overlay is a mode of the main window where the
tasks live — a second window would fork the keymap context chain,
the state plumbing, and the kiosk story for no user value v1.
Recorded as a fenced alternative (§9 R-5) should review disagree.

### D-F9 — minimal action growth: exactly one new `ActionId`

`ActionId::FocusModeToggle` (dotted id `"focus.toggle"`), registry
row (`REGISTRY`, actions.rs:414 — the tracking rows :944-963 are the
"menu-reachable, dispatch-time gating" template), `id()` arm, router
arm → `UiEffect::ToggleFocusMode` (the exhaustive match forces the
arm), completeness `ALL` array entry, registry snapshot regen
(`tests/keymap_snapshots.rs:42`).

**Session verbs are not re-invented**: the overlay's Start dispatches
the existing `ActionId::TaskTrackStart` with `track_target` set from
the switcher selection — the notification-row Start Task precedent
(`adapter.rs:5299-5310` sets `a.track_target = Some(task)` then
routes; the router arm `router.rs:461-468` maps to
`StateCommand::StartTracking`); the overlay's Stop dispatches the
existing payload-free `TaskTrackStop`. Zero new session actions; the
menu enablement (`entry_states`, adapter.rs:322-336) is untouched for
the tracking pair and gains exactly one row:
`FocusModeToggle ⇒ !kiosk ∧ focus.is_some()`.

**Bindings** (each verified free at the base; stage 2 re-verifies via
the preset snapshots): CUA `Ctrl+J` and emacs `Ctrl+J` at
`ContextId::Global`; vi `Z f` at `ContextId::ViNormal` (the `Z`
leader family's free letter — `Z c` capture, `Z Q` exit, `Z s`
settings). Inside the overlay, `ContextId::FocusMode` binds
`FocusModeToggle` to `Escape` + the same toggle stroke (close),
`MenuNext`/`MenuPrevious` to `Down`/`Up` (arrows, **not** j/k — the
palette's recorded reason: the panel has a text field, printables
must reach it), and `MenuInvoke` to `Return` (start the selected
task). The scrim click closes (the capture scrim precedent).

### D-F10 — switcher and quick-note: existing machinery, no new commands

- **Switcher**: rows derive purely from published state —
  `presentation::focus_switcher_rows(state) -> Vec<SwitcherRow>`:
  live, not-done tasks in board reading order (board → stack order →
  task order, the deterministic walk; not the today-window pool — a
  focus target may not be scheduled today), bounded to 9 rows, each
  carrying `(TaskId, title)`. Selection is the flow's `selected`
  cursor (arrow keys); `MenuInvoke`/the row's click dispatch
  `TaskTrackStart` with `track_target` set. An empty pool renders the
  empty-state hint and disables invoke. The palette (`Ctrl+F`) stays
  the power tool; embedding it inside the overlay is fenced (flows
  are XOR — compositing flows is a view-state architecture change
  with no v1 payoff).
- **Quick-note**: the overlay panel embeds one `TextField` (the
  capture-overlay composition's shape, `capture-overlay.slint:11-45`)
  wired to the **shared view-state buffer** (`set_buffer` — flows are
  XOR, so the buffer has one owner). Commit (`Return`/accepted):
  emit `FlowOutcome::Command(StateCommand::CaptureTask { board: None })`
  — the landed inbox-capture command, exactly the capture flow's
  commit (`view_state.rs:1402-1418`) — then **clear the buffer and
  stay open** (rapid-fire capture semantics; unlike capture, an empty
  commit is a no-op, never a close). Typing auto-engages `ViInsert`
  (the capture precedent, adapter.rs:464-478); `Escape` leaves insert
  first (the vi insert contract), then closes the overlay.
- **Exit affordances**: close-the-overlay (Escape / toggle stroke /
  scrim click — a flow close, session unaffected) and stop-the-session
  (the panel's Stop button → `TaskTrackStop` — session ends, overlay
  stays open showing the idle + switcher state). Quitting the app
  remains the landed `AppExit` family (`Ctrl+Q`, `Z Q`). Closing the
  overlay never stops tracking; stopping tracking never closes the
  overlay. Mid-session app shutdown needs no overlay handling: the
  open entry persists (ADR 0017), the runtime re-derives on the next
  boot (D-F16).

### D-F11 — state flow into the overlay (the dumb-client line)

One new push: `models::push_focus(...)` inside `models::push`
(`models.rs:684-696`; "a new surface adds one `push_*` here"), fed by
pure derivations in `presentation.rs` (the `tracking_chip` /
`open_elapsed_phrase` homes, :575-601):

```rust
pub struct FocusOverlay { ... }
pub fn focus_overlay(state: &AppState, timing: &FocusTiming, now: DateTime<Utc>) -> FocusOverlay;
```

Derives: phase (None/Focus/Break from `newest_open().kind`), the
remaining time `deadline − now` rendered `mm:ss` (Rust-side
formatting — the chip-text precedent :1505-1514), the current task
title (the entry's `task_title` snapshot), and the switcher rows.
AppWindow grows `in-out` properties: `focus-overlay-visible` (the
`Flow::FocusMode` mirror), `focus-phase`, `focus-remaining-text`,
`focus-task-title`, the switcher `VecModel` + selected index, and the
callback seams (`focus-note-accepted`, `focus-overlay-close-requested`,
`focus-switcher-invoked`, `focus-stop-clicked`) installed by a new
`install_focus_seams` beside `install_tracking_seams`
(`adapter.rs:5146-5154`). New slint composition:
`ui/compositions/focus-overlay.slint`, mounted in `app-window.slint`
as the scrim+panel last siblings. Every textual value is derived
Rust-side and pushed — the slint tree computes nothing.

### D-F12 — the countdown tick: the seconds gate, extended — never a second channel

`compute_seconds_surfaces_active` (`adapter.rs:787-805`) is the
single per-second tick channel and its doc **mandates** this exact
extension ("chunks 8/9 extend THIS term; never introduce a second
tick channel"): the term gains `self.view_state.flow() ==
Flow::FocusMode { .. }` (the overlay's remaining-time text changes
with the wall second). The kiosk layers stay: the flow is
unreachable on kiosk (D-F13), the explicit `!self.kiosk` term stays,
and the driver consults the flag as before. When the overlay is
closed, the gate returns false and nothing ticks per-second — the
wall-minute repaint suffices for the status chip as landed. The
`now` the derivation uses arrives through the existing push plumbing
(`day_ctx`'s render-time read, `adapter.rs:775-780` — the UI's one
sanctioned wall read, presentation data only); the runtime's
injected `Clock` is unrelated to it and remains the only
auto-behavior clock.

## 5. Kiosk decision — D-F13

**Decided: structurally excluded, three layers, pinned.** The
DR-3/DR-5 precedents converge (AlarmWindow never mounts on kiosk,
`lib.rs:317-325`; the tracking chip renders read-only, D-T16; the
seconds gate is kiosk-frozen, `flow_table_tests.rs:1300-1343`); a
concentration overlay is an interactive desktop affordance and a
kiosk is a passive appliance — exclusion is the only coherent
posture. The three layers:

1. **Process**: bootstrap never spawns the runtime in
   `AppMode::Kiosk` (D-F7's spawn condition) — no auto-transitions on
   an appliance; a kiosk's open entries are display data.
2. **Model**: `entry_states` returns `false` for `FocusModeToggle`
   on kiosk (the `tracking_chip_action_visible(!kiosk)` shape,
   presentation.rs:607-609) — the action never lights, the menu
   never offers it.
3. **Effect**: `apply_effect`'s `ToggleFocusMode` arm no-ops on
   kiosk (the adapter holds the kiosk bit, adapter.rs:123) — the
   flow is structurally unreachable even if a future surface
   bypasses the gate; the seconds-gate term is therefore
   unreachable, and the explicit `!self.kiosk` term freezes the
   decision anyway.

Pins: an `entry_states` kiosk assertion, an apply-effect/flow-table
pin (the `the_seconds_gate_stays_false_on_kiosk` sibling), and the
**existing kiosk goldens stay byte-identical** (the overlay adds no
kiosk-visible pixel; they are not regenerated). Escalation's
audio-gate comment (`escalation.rs:173-179`, D-9) is the honest-decision
rhetoric to mirror in the spawn-site comment.

## 6. Decision record

| ID | Question | Decision | Status |
|---|---|---|---|
| D-F1 | Where the session lives / the state machine's memory | **Nowhere outside the open entry**: phase derived per wake from `newest_open()` (`Work` ⇒ focus, `Respite` ⇒ break) over one pure `focus_step(open, timing, now) → { actions, next_due }`; zero runtime bookkeeping; deadline anchors on `entry.start` | **decided (the plan's central ruling)** |
| D-F2 | The focus→break transition's verb sequence | **`StopTracking` → `StartTracking { task, kind: Respite }`, two awaited dispatches** — the landed `StartTracking` no-op is task-keyed and kind-blind (`command.rs:2132-2140`), so a single kind-switching dispatch would be swallowed; break→idle and session-stop are one `StopTracking`; the planner no-op refinement is fenced as an issue idea, not changed here | **decided (the kind-blind no-op land-mine)** |
| D-F3 | Mode gating | **Opt-in `[focus] enabled`, default false**; absent table = off (`web`-section pattern); no auto-stop for non-opted tracking — opt-in is the hijack containment | **decided** |
| D-F4 | Auto-chain / long breaks | **None in v1**: break end ⇒ idle; the user starts the next focus. Every-N long-break cadence + auto-start knob fenced as one follow-up issue idea (needs per-session memory — the first `LadderMemory`-shaped state this runtime would own) | **decided** |
| D-F5 | Config shape | **`[focus] { enabled, work_mins = 25, break_mins = 5 }`** — plain integer minutes (the `[sync]` plain-numbers precedent; no string grammar), `deny_unknown_fields`, skip-serializing at defaults; validation `1..=240` in `AppConfig::validate` → `ConfigError::InvalidFocusTiming { field }`, stable kind `"invalid-focus-timing"`; raw echoed by `config print`; garbage rejected even when disabled (validate what is written) | recorded |
| D-F6 | Timing type home | **`FocusTiming` in `taskboard-domain/src/focus.rs`** — private fields, `try_new` (> 0, whole seconds) at the config-load boundary, no serde (injected data, `ReminderDefaults` precedent); the 240 ceiling stays config-layer policy | recorded |
| D-F7 | Hand-over seam | `AppConfig::focus_config() -> Option<FocusTiming>` (expect-on-validated); bootstrap spawns after the escalation driver, before the connector loop, `Arc::new(SystemClock)`; `App.focus_join: Option<JoinHandle<()>>`; `UiSettings.focus: Option<FocusTiming>` (the reminder_defaults precedent) | recorded |
| D-F8 | Overlay surface | **In-window `Flow::FocusMode { selected }` full-surface scrim+panel** (capture/palette overlay pattern); one-window rule; `AlarmWindow` second-window declined (interrupt-any-context vs. a main-window mode), fenced alternative R-5 | **decided** |
| D-F9 | Action growth | **Exactly one new `ActionId::FocusModeToggle`**; session verbs reuse `TaskTrackStart` (overlay sets `track_target` — the notification-row precedent) and `TaskTrackStop`; `ContextId::FocusMode` (total mask, Palette-style), chain `[FocusMode, ViInsert, Global]`; bindings CUA/emacs `Ctrl+J` Global, vi `Z f` ViNormal, in-overlay Escape/toggle/arrows/Return (arrows not j/k — the palette's text-field reason) | recorded |
| D-F10 | Switcher + quick-note | Switcher = pure `focus_switcher_rows` (live, not-done, board reading order, bounded 9), cursor in the flow; invoke = `TaskTrackStart` with `track_target`. Quick-note = the shared view-state buffer committing **`CaptureTask { board: None }`**, stays open, empty commit is a no-op. Palette embedding fenced | recorded |
| D-F11 | Overlay state flow | One `push_focus` in `models::push` + pure `presentation::focus_overlay(state, timing, now)`; all text formatted Rust-side; new `ui/compositions/focus-overlay.slint` + `install_focus_seams`; slint computes nothing | recorded |
| D-F12 | Countdown tick | **`compute_seconds_surfaces_active` gains the `Flow::FocusMode` term** (its doc mandates this extension point); no second tick channel; closed overlay ⇒ no per-second cost; `now` rides the existing push plumbing | recorded |
| D-F13 | Kiosk posture | **Structurally excluded, three layers pinned**: no runtime spawn on `AppMode::Kiosk`; `entry_states` gate; `apply_effect` no-op. Existing kiosk goldens stay byte-identical | **decided** |
| D-F14 | Phase-boundary notifications | **None in v1** — overlay + chip are the surfaces; toast/chime fenced as a follow-up issue idea (the escalation ladder/`ChimeSink` is the future template) | **decided** |
| D-F15 | Runtime task shape | The escalation template verbatim: parks on the `EngineSignal` broadcast with no timer when idle, `select!` sleep/signals while live, `Closed` ⇒ exit, `Lagged` ⇒ change; no engine change, no new channel, no `EngineSignal` variant; actions fire via `EngineHandle::execute` (receipts); post-action wake armed from the receipt's stamped entry (publish-order race), state re-derived at every signal wake | recorded |
| D-F16 | Reconciliation | The entry is truth: external stop ⇒ idle park (never re-open); foreign start ⇒ re-anchor at the new `start`; manual respite ⇒ break phase; stale past-due entry at boot ⇒ **one** reconciling transition at engine `now`, no backdating, no cascade; D-T8 double-open ⇒ `newest_open()` governs | recorded |
| D-F17 | Sync posture | **Nothing new roams**: sessions are ordinary `TimeEntry` rows (kind already on the wire); `FocusTiming` is device-local injected config (never persisted, never synced). Fleet consequence recorded: two focus-enabled devices both drive transitions — LWW + the next-write-closes-all invariant reconcile, the D-T8 tolerance class | recorded |
| D-F18 | Undo | The dispatched verbs ride the landed tracking undo arms (#555) exactly like manual ones — nothing new | recorded |
| D-F19 | CHANGELOG | One Unreleased feature entry covering runtime + config + overlay + kiosk exclusion | recorded |
| D-F20 | CLI/IPC surface | **None**: no new verb, no flag; `track start/stop/status` already drive and observe sessions; the fenced planner no-op refinement is CLI-visible too (one issue covers both) | recorded |
| D-F21 | ADR | **None**: this is an implementation chunk of an accepted design-round fence (#496/DR-5), not a new design round — the wave-ADR convention does not apply; the plan + close comment carry the rationale | recorded |

## 7. Test catalogue (TDD-first: each phase opens with its failing tests)

**Phase F.1 — domain: `FocusTiming`**

- `focus_timing_mints_positive_whole_second_durations` /
  `zero_or_negative_work_or_respite_rejected_typed` /
  `sub_second_duration_rejected_typed` (the `WorkThreshold` mint
  rules; typed `matches!` assertions).
- `focus_timing_accessors_round_trip` (structural).
- No serde impl to test — the type's doc states the injected-data
  contract (compile-time: it does not implement `Serialize`).

**Phase F.2 — app: the `[focus]` section**

- `focus_section_absent_decodes_disabled_and_serializes_nothing` /
  `focus_section_present_defaults_to_25_and_5` (the web-section
  present/absent pins, config.rs:2740-2770 shape).
- `validate_rejects_zero_and_overflow_focus_minutes` — both fields,
  typed `ConfigError::InvalidFocusTiming` with the right `field`
  payload; `240_accepted_and_241_rejected` (the boundary pin).
- `focus_error_kind_is_the_stable_discriminant` +
  `the_new_kinds_extend_the_stable_discriminants` (extend the
  existing pin).
- `serde_round_trip_preserves_focus_defaults` (joins the roundtrip
  pin, config.rs:1813).
- `focus_config_mints_when_enabled_and_none_when_disabled` (mapper,
  expect-path exercised via a validated config).
- Integration (`tests/config.rs`, the c-family): a partial file with
  `[focus] work_mins = 0` → typed kind in the envelope;
  `TASKBOARD_FOCUS__WORK_MINS` env override wins (the c2/c12/c13
  recipe).

**Phase F.3 — app: the runtime (`focus.rs`)**

Pure `focus_step` units (structural, `FixedClock`-style instants):

- `idle_parks_with_no_actions` (`next_due: None`).
- `work_block_arms_its_deadline_and_fires_open_break_at_the_due` —
  one second early: no actions + `Some(start + work)`; at the due
  instant: exactly `OpenBreak { task }`.
- `respite_block_arms_its_deadline_and_fires_end_break_at_the_due`.
- `the_newest_open_entry_governs_the_phase` (two open rows, newer
  wins — the D-T8 reader pick).
- `manual_respite_derives_the_break_phase` / `foreign_work_entry_re_anchors`
  (a Work entry started later than the observed one moves the
  deadline).

Driver journeys (`#[tokio::test(start_paused = true)]`, a fake
`Clock` (the `escalation.rs:874-887` `FakeClock` shape), a **real
engine** over a tempdir repo (`spawn_state_engine` with
`ReminderEngineConfig::disabled()` + `DayRollConfig::disabled()` so
no engine timer interferes; the advance-both lockstep: `clock.set` →
`tokio::time::advance` → `yield_now`; assertions on
`engine.shared_state().load()` terminal shapes — no sleeps, no
transient intermediates):

- `a_focus_session_hands_off_to_the_break_at_the_deadline` — test
  starts tracking via `engine.execute(StartTracking { kind: Work })`,
  advance past `work()` ⇒ published state shows the Work entry
  closed, exactly one open entry with kind `Respite` on the same
  task.
- `the_break_ends_into_idle_at_its_deadline` — advance past
  `respite()` ⇒ zero open entries; the runtime parks (further
  advances change nothing).
- `an_externally_stopped_session_is_never_reopened` — mid-focus
  `execute(StopTracking)` from the test, advance well past the
  deadline ⇒ still zero open entries (the entry-is-truth pin).
- `a_foreign_start_re_anchors_the_session` — mid-focus the test
  starts task B ⇒ no auto-break at task A's original deadline; the
  break opens at B's `start + work()`.
- `a_stale_entry_reconciles_exactly_once_at_boot` — pre-seed a repo
  with an open Work entry started `3 × work()` ago; spawn the runtime
  ⇒ exactly one hand-off fires (the break entry's `start` ≈ engine
  now, the stale entry's `end` ≈ engine now — no backdating, no
  cascade), then the normal cadence continues.
- `the_runtime_exits_when_the_broadcast_closes` (the escalation
  Closed-exit test shape, `escalation.rs:1148-1168`).
- `the_double_dispatch_race_cannot_double_open` — drive the
  receipt/publish race window explicitly (stall the store via the
  engine's ordering) OR assert by construction: across the hand-off
  journey, exactly one `Respite` row ever exists (structural
  terminal-state pin; correctness, not a ceiling).
- Bootstrap-level: `focus_disabled_spawns_no_runtime` /
  `kiosk_mode_never_spawns_the_runtime` (the `bootstrap.rs` test
  module's `FixedTestClock` shape; assert the handle is `None`).

**Phase F.4 — ui-slint: the overlay + gates**

- Presentation units (structural): `focus_overlay_derives_phase_and_remaining`
  (Work/Respite/None; remaining arithmetic at boundary instants;
  `mm:ss` formatting buckets), `focus_switcher_rows_are_live_not_done_in_board_order_bounded`
  (done/deleted filtered; order; the 9-row bound; empty pool),
  `focus_overlay_visibility_follows_the_flow`.
- `the_seconds_gate_covers_the_focus_overlay` —
  `compute_seconds_surfaces_active` true iff `Flow::FocusMode` on
  desktop; extend `the_seconds_gate_stays_false_on_kiosk`
  (`flow_table_tests.rs:1300-1343`) to pin the kiosk-freeze with the
  new term present.
- Router/registry: the `FocusModeToggle` arm ⇒ `UiEffect::ToggleFocusMode`
  (router test, `router.rs:573+`); `entry_states` pins —
  `FocusModeToggle` disabled on kiosk and when focus is `None`,
  enabled otherwise; the tracking pair's rows untouched (existing
  pins hold).
- Effect/flow: dispatching `FocusModeToggle` opens the overlay
  (properties set), again closes it; the scrim callback closes;
  closing does not stop an open session (the published tracking
  state is untouched — the drift-story UI pin).
- Quick-note: `focus-note-accepted` with text emits
  `StateCommand::CaptureTask { board: None }` at the forwarder
  boundary (the journeys harness' typed-command assertion), clears
  the buffer, keeps the overlay open; empty commit emits nothing.
- Stop button routes `ActionId::TaskTrackStop` (typed command at the
  boundary); Start on a switcher row routes `StartTracking { task,
  kind: Work }` with the selected task (the `track_target` seam).
- Keymap: `tests/keymap_snapshots.rs` registry dump + resolved
  preset snapshots regen (`INSTA_UPDATE=always` local, committed as
  data); the `ALL` completeness array extended.
- Golden: one headless-renderer golden of the open overlay
  (`tests/golden_visuals.rs` pattern — mount, dispatch the toggle,
  `driver.snapshot()`, budget compare, `TASKBOARD_REGEN_GOLDENS=1`
  semantics). The **existing kiosk goldens are not regenerated** —
  their byte-identity is the kiosk-exclusion visual pin.

**Phase F.5 — wiring, CHANGELOG, gates**

- `CHANGELOG.md` Unreleased: one Added entry (D-F19).
- Full AGENTS §10 gate suite **through
  `scripts/run-quality-gates.sh`** (the flock wrapper): fmt, clippy
  `-D warnings`, nextest, doc tests, `cargo deny`, the
  three-command coverage sequence with the quarantine step, trailing
  `rm -rf target/llvm-cov-target`.

TDD order inside each phase: pins/journeys first (red), then the
production change (green), then the property/snapshot sweep. The
domain type (F.1) and config (F.2) land before the runtime (F.3)
compiles against them; the overlay (F.4) lands last so its push and
seams have the finished runtime to journey against.

## 8. Phasing & commit plan (one branch, F.1 → F.5 in order)

| Phase | Delivers | Commit shape |
|---|---|---|
| F.1 | `FocusTiming` + `InvalidFocusTiming` in domain `focus.rs` | `feat(domain): the focus timing type` |
| F.2 | `[focus]` section, validate ladder arm, `focus_config()`, config tests | `feat(app): the focus timing configuration` |
| F.3 | `focus_step` + `spawn_focus_runtime` + bootstrap spawn/`focus_join` + journeys | `feat(app): the focus session runtime` |
| F.4 | Overlay composition/push/seams/actions/router/presets/flow + all UI pins + goldens + snapshots + CHANGELOG | `feat(ui-slint): the concentration overlay` (+ fixups, none with duplicate subjects — rebase-merge hygiene) |
| F.5 | Full gate suite green through the flock wrapper | (folded into F.4's final state; no separate "fixups" commit) |

Every commit standalone (builds, meaningful subject, no WIP) — the
rebase-merge hygiene rule. Insta snapshot updates ride the phase that
changed the dump; goldens are regenerated only via
`TASKBOARD_REGEN_GOLDENS=1` and only for the new overlay golden.

## 9. Risks & mitigations

- **R-1 — batch-5 sibling churn** (5.2 `chore/ui-slint-cleanups`
  touches `adapter.rs` seams / `view_state.rs` / pure modules): the
  soft UI-territory rule makes the overlay a new territory, but both
  chunks will touch `adapter.rs`/`presentation.rs`/`view_state.rs`
  hunks. Mitigation: stage 2 lands F.1–F.3 (no ui-slint files)
  independently; F.4 rebases onto whatever of 5.2 has landed at its
  start (§2 rebase dance, `--force-with-lease` only after rewrites).
  Never a semantic merge of the cleanups with this feature.
- **R-2 — the kind-blind no-op** (D-F2): the two-verb hand-off is
  correct against the landed planner, but the same gap makes the CLI's
  `track start <task> --kind respite` a silent no-op while that
  task's Work entry runs — chunk 3.5's own documented D-3 intent.
  Mitigation: fenced as a follow-up issue idea (stage 3 opens it:
  "make the StartTracking idempotent no-op kind-aware"); this chunk
  does not touch the landed planner arm.
- **R-3 — receipt vs publish ordering**: `execute()`'s reply can
  resolve before the engine's `ArcSwap` store is visible. Mitigated
  by construction (D-F15): the post-action wake arms from the receipt;
  every signal wake re-derives from state; the convergence property
  (duplicate hand-offs are benign) is the documented safety net, and
  the journey test pins the single-Respite outcome.
- **R-4 — the seconds gate widened**: while the overlay is open the
  UI repaints per second. Bounded: the gate is off unless the flow is
  open on a desktop with focus enabled; the existing SecondTick
  machinery (`lib.rs:663-680`) already serves the task-view case at
  this cadence; no new repaint channel.
- **R-5 — the overlay-surface ruling** (in-window flow vs second
  window) is the one place a reasonable alternative exists; it is
  forced by the one-window rule and the capture/palette precedents,
  and isolated: if review prefers a borderless always-on-top window,
  the `AlarmLanes` pattern is the migration path, and only F.4's
  surface moves (runtime + config are unaffected).
- **R-6 — snapshot/golden churn**: the registry dump, preset
  snapshots, and one new golden change; the kiosk goldens must stay
  byte-identical (do not regenerate them). Mechanical, data-only.
- **R-7 — daemon-mode focus**: `[focus] enabled` on a headless daemon
  advances sessions with no overlay (the overlay is a desktop-UI
  surface). Coherent (CLI observes via `track status`), recorded at
  D-F13/D-F17; no gate added — the runtime is mode-agnostic apart
  from kiosk.
- **R-8 — bounds bikeshed** (`1..=240`, `25`/`5`): recorded values,
  one constant each, isolated in the config section — review
  adjusts constants, not structure.

## 10. Size, handoff, coordination

- **Estimated diff**: ≈ **1.6–2.2k lines** including tests — domain
  `focus.rs` ~150 (type + mints + tests), app config ~220 (section +
  validate + mapper + unit/integration tests), app `focus.rs` ~600
  (step + driver + journeys + bootstrap spawn ~40), ui-slint ~900
  (actions/registry/router/presets ~120, view_state flow ~60,
  presentation + units ~180, models push ~80, composition slint
  ~180, adapter seams + flow-table pins ~120, journeys + keymap
  snapshots + golden ~160), `UiSettings` ~10, CHANGELOG 1 entry.
  **M** (upper half) — consistent with the dispatch table's sizing.
- **Dependencies**: none outside the workspace (`cargo deny`
  untouched; no rodio touch — audio is D-F14-fenced). Gates run
  under `scripts/run-quality-gates.sh` (5.2 may hold the box).
- **Handoff**: stage 2 implements F.1–F.5 on `feat/pomodoro-focus`
  in `../taskboard-WT-feat-pomodoro-focus`; stage 3 reviews, runs
  the gates, opens the PR titled `feat(app,ui-slint): the pomodoro
  focus mode` closing **#496**, and opens the two fenced issue ideas
  (kind-aware StartTracking no-op; focus follow-ups: auto-chain/
  long-break knob, phase-boundary notifications — one issue each,
  `backlog:` prefix + permalink to this plan). Worktree removal is
  stage 3's final act (doctor rules; `--force` per the lifecycle
  doc). **No push during planning** — this commit stays local.
