# [Plan] Focus follow-ups — auto-chain & long-break cadence, phase-boundary notifications (#600) — stage-1 implementation plan

- **Date**: 2026-11-05.
- **Kind**: stage-1 TDD implementation plan for chunk **5** of the
  v0.2 focus-&-reflection dispatch. Zero production code in this
  commit; this file is the chunk's entire planning artefact.
  Implementation is stage 2's, review + shipping stage 3's.
- **Origin**: the fence issue **#600** ("focus-mode follow-ups —
  auto-chain/long-break cadence, phase-boundary notifications"), which
  fences exactly two v1 gaps out of the pomodoro chunk: **D-F4**
  (break end lands in idle; a knob for auto-starting the next focus
  block + an every-N long-break cadence "needs per-session memory —
  the first `LadderMemory`-shaped state the focus runtime would own")
  and **D-F14** (a boundary toast/chime "would need the escalation
  ladder's `ChimeSink`/notification templates"). Normative carriers:
  the implementation plan §5.5 + VF-6
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`),
  the dispatch plan §6
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`,
  branch `feat/focus-followups`), and the pomodoro plan
  `.artifacts/plans/2026-10-31-pomodoro-focus-plan.md` (§2's runtime
  constraints and §6's decision record are this chunk's inheritance).
- **Status**: **READY** — every question the seams raised is settled
  with a recorded decision (§7, D-FF1…D-FF14). The finding this
  planning pass contributes beyond the fence issue is the
  **memory-vs-history ruling** (D-FF2: the runtime's first owned
  memory is *one slot* — the long-break decision it itself made,
  which closed-entry history cannot recover when the break closes
  early; the chain *position* itself is never accumulated, it is
  re-derived from entry history by a pure domain walk, which
  structurally kills the double-count-after-restart bug class stage 3
  is told to hunt). The second finding is the **boundary-identity
  ruling** (D-FF7: boundary notifications deliberately do NOT ride
  `ReminderRef`/`Notification` — that vocabulary roams the wire,
  storage, and merge, and a synthetic engine-derived-looking
  notification is the wrong shape; a dedicated app-layer
  `FocusBoundarySink` seam does).

## 1. Mission & binding constraints

Two coherent halves over the landed focus runtime
(`crates/taskboard-app/src/focus.rs`), one truth source unchanged:

1. **Auto-chain + long-break cadence (D-F4)**: when
   `[focus] chain = true`, the runtime's break-end no longer lands in
   idle — it closes the break and starts the next focus block on the
   same task, indefinitely. The break the runtime opens after the
   Nth completed work block of a cycle is the *long* break
   (`long_break_mins`), resetting the cycle. The runtime gains its
   first owned bookkeeping, minimal (D-FF2).
2. **Phase-boundary notifications (D-F14)**: a gentle toast + chime
   at each runtime-driven boundary (break begins / break ends into
   the next focus or into idle), riding the escalation driver's
   `ChimeSink` + toast machinery as templates, P12-gentle by
   construction, opt-out via `[focus] notify = false`.

Binding constraints (inherited verbatim from the pomodoro plan §2,
each still verified against the tree at the base `origin/main` @
`58ae9c55`):

- **The runtime drives only the landed tracking verbs.**
  `StateCommand::StopTracking` and
  `StateCommand::StartTracking { task, kind }` — nothing else. The
  two-verb sequence is still forced by the kind-blind no-op
  (`command.rs` planner arm), now in *both* directions: the
  focus→break hand-off (stop → start `Respite`, landed) and the new
  break→focus auto-resume (stop → start `Work`).
- **The tracked entry is truth.** External stops still never re-open
  and never chain (D-F16's table extends, §6; the chain fires from
  the runtime's own break-end *receipt*, never from observing idle
  state). The deadline still anchors on `entry.start`.
- **No second clock source.** The injected `Clock` reads every due;
  tokio time stays the wakeup mechanism; journeys stay
  `#[tokio::test(start_paused = true)]` + the shared `FakeClock` +
  the advance-both lockstep (`tick`) — the landed
  `focus.rs` test harness, extended not replaced.
- **No engine change, no new channel topology, no
  `SystemEvent`/`EngineSignal` variant.** Boundary notifications are
  fired by the runtime task at its action receipts — not by an engine
  evaluator pass, not by a new reminder rule kind, not by a new tick.
  (VF-5's `due_firings` passes were considered and declined: focus
  mode is app-injected config the engine does not see, and an
  engine-side pass would break the escalation fence verbatim.)
- **No schema migration, no wire-vocabulary change, no new
  dependencies.** Nothing new roams; `TimeEntryKind` stays
  `{Work, Respite}` (a `LongRespite` kind was considered and declined
  — D-FF7 records why); `ReminderRef` grows no variant; `cargo deny`
  untouched; the workspace's only rodio touchpoint stays
  `escalation.rs`'s `ChimeSink` (reused, not duplicated).
- **Constants-never-config holds** (the escalation fence): the
  boundary chime's volume is a named constant in `focus.rs`; the
  chain gap tolerance is *derived* from the user's own timing knobs
  (D-FF4), not a new config surface.
- **Typed config validation at the boundary (A1)**: the new knobs
  join the flat validate ladder as typed
  `ConfigError::InvalidFocusTiming { field }` rejections with the
  existing stable kind **`"invalid-focus-timing"`**; garbage is
  rejected even while disabled; the mint at `focus_config()` stays
  expect-on-validated.
- **No text assertions, no quality ceilings, deterministic tests**
  (AGENTS §5): boundary delivery is pinned as typed events at a fake
  sink seam; countdown assertions stay arithmetic-structural; no
  upper bound is asserted on deliveries that should succeed.
- **Kiosk exclusion stands** (D-F13's three layers): the new
  machinery lives *inside* the layer-1 process gate (the runtime —
  which carries the chain and the boundary sink — still never spawns
  on `AppMode::Kiosk`); no new UI action, effect, or flow exists for
  it; the existing kiosk pins and goldens stay byte-identical.
- **Non-goals**: the settings-view editor (chunk 6 grows rows at the
  knob names pinned here); any new CLI/IPC verb (`track status` is
  unchanged — D-F20 carries); pausing; chain persistence beyond entry
  history; per-task chains; a chain pause at the long break (the
  cycle is uniform — D-FF5 records the alternative); focus-boundary
  rows in the in-app notification surface (the overlay + chip + OS
  toast are the surfaces).

## 2. The chain cadence — the walk (D-FF1), the memory (D-FF2), the length rule (D-FF3)

### D-FF1 — chain position is *derived*, never accumulated: one pure domain walk

The single source of chain truth is a pure function over published
entry history, living in **domain** (`taskboard-domain/src/focus.rs`)
because two consumers on opposite sides of the crate boundary need
the identical answer: the app runtime (break-length decision,
deadline re-derivation) and ui-slint's `presentation::focus_overlay`
(countdown for the currently open block — today it hard-codes
`timing.respite()` for any break, which goes *visibly wrong* the
moment long breaks exist; fixing it is why the walk is domain, not
app-private).

```rust
// crates/taskboard-domain/src/focus.rs (new public surface)
/// How many work blocks the live cycle has completed, counting the
/// given open work entry. Derived from closed-entry history at every
/// call — never stored.
pub fn focus_work_position(
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    open_work: &TimeEntry,
    timing: &FocusTiming,
    decided_long: Option<TimeEntryId>,
) -> u32;

/// Whether the currently open respite is a long break: true iff the
/// cycle position of the work block preceding it hits the cadence.
pub fn focus_break_is_long(
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    open_respite: &TimeEntry,
    timing: &FocusTiming,
    decided_long: Option<TimeEntryId>,
) -> bool;
```

The shared backwards walk (one private core, two thin public
wrappers; every hop deterministic, ties broken by `(start, id)` —
the `newest_open()` tie-break discipline, `tracking.rs:182-185`):

1. Start at the given open entry. At each hop, the predecessor
   candidate is the closed entry (`end: Some`) with the greatest
   `(end, id)` satisfying `end ≤ current.start` and
   `current.start − end ≤ tolerance`. Overlapping or later rows (the
   D-T8 transient double-open ghost class) never qualify.
2. The **tolerance** is `max(timing.respite(), timing.long_break())`
   — "return within one sanctioned pause and the cycle lives; come
   back later and it is a new day's rhythm". Derived from the user's
   own knobs, never config (recorded leniency: a `long_break_mins`
   of 240 makes a 4-hour gap chain-continuing — §10 R-5).
3. The walk **crosses any kind** — including Work→Work hops (a task
   switch without a break is the same pomodoro flow; pomodoros count
   across task switches). Only `Work` entries increment the count.
4. The walk **stops at a long break**: a closed `Respite` entry that
   the cadence declares long — its recorded duration is
   `≥ timing.long_break()`, **or** its id is the caller's
   `decided_long` anchor (D-FF2). Work blocks before that boundary
   belong to the previous cycle and are not counted. Short and
   medium respites (any duration `< long_break()` — including an
   over-long manual respite) are crossed, not stopping.
5. No alternation requirement, no cascade: the walk terminates at
   the first stop condition. A cold history with no predecessor ends
   it.

Why derived rather than a driver-accumulated counter: an accumulated
`chain_done` plus a history recovery rule is *two* derivations that
must agree, and the disagreement window is exactly the
"double-counts after daemon restart" bug class the dispatch names.
With one walk there is nothing to recover and nothing to double:
restart mid-cycle re-derives the same position from the rehydrated
rows (ADR 0017's crash-safety property, consumed a second time). The
accumulated-counter alternative is recorded as declined (§7 D-FF1).

### D-FF2 — the runtime's first owned memory: one slot, the fact history cannot answer

The walk is complete *except* for one question: **was that closed
long break a long break?** History answers by duration inference,
and the inference fails exactly when the runtime's own long break is
externally stopped early (recorded duration `< long_break()` — e.g.
the user clicks Stop two minutes into the 15-minute long break and
resumes). That decision — "the break I opened at that receipt was
the cycle's long break" — is the one fact closed-entry history
cannot recover, and it is the runtime's first owned bookkeeping, the
`LadderMemory`-shaped state the fence issue names:

```rust
// crates/taskboard-app/src/focus.rs
/// The focus runtime's owned bookkeeping (issue #600) — minimal by
/// ruling: one slot holding the long-break *decision* the runtime
/// itself made, which closed-entry history cannot recover when the
/// break closes early. Chain position is never stored; it is
/// re-derived by the domain walk at every step.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct FocusMemory {
    decided_long: Option<TimeEntryId>,
}
```

- **Written** at the driver's break-open receipt: when the hand-off
  opened a long break, `decided_long = Some(entry.id)` from the
  `TrackingStarted { entry }` receipt (the same receipt already arms
  the wake). One slot, overwritten at the next long-break opening —
  an older decision is unreachable once a nearer boundary exists.
  The pure `focus_step` takes `&FocusMemory` (read-only): the step
  feeds the anchor to the walk; the driver owns the write.
- **Restart recovery is zero code**: `FocusMemory::default()` at
  spawn, and the walk's duration inference is the recovery rule.
  Recorded leniency: after a restart, an externally-aborted long
  break whose duration undercuts `long_break()` is inferred short,
  so the cycle may run long by up to `long_break_every − 1` blocks
  until the next real long break — the doc states this at the type
  and the journey pins the recovery path itself (`FF.3`:
  `restart_mid_cycle_recovers_the_position_from_history`).
- **Nothing else survives restart**: chain on/off and notify are
  config; the phase is the rehydrated open entry; the position is
  the walk. The "what survives restart vs what derives from closed
  Work entries" rule the dispatch asks this plan to record is
  exactly: *only the decision slot is owned, it deliberately does
  not survive, and everything else derives from entry history —
  including across restarts.*

### D-FF3 — the length rule: `position % long_break_every == 0`

- At a work block's deadline (the `OpenBreak` due): the break the
  runtime opens is long iff `focus_work_position(...) % timing.long_break_every() == 0`
  (the position counts the closing block). Blocks 1–3 short, block 4
  long, with `long_break_every = 4`.
- At an open break (deadline re-derivation and the overlay):
  `focus_break_is_long(...)` — the walk counts the works strictly
  preceding the break, which is the same number the hand-off used.
  The two computations agree *structurally* (one walk, two starting
  points), not by bookkeeping.
- `long_break_every = 1` makes every break long and every break
  resets the cycle — coherent, allowed. `short == long` durations
  make every break a boundary — coherent degenerate case, no special
  handling. Both recorded at the walk's doc.

## 3. Auto-chain — the end-of-break act (D-FF5, D-FF6)

### D-FF5 — `EndBreak` grows the chain act; the chain rides the receipt, never idle observation

`FocusAction` grows, and `focus_step` grows with it (the step's
signature becomes `focus_step(tracking: &TrackingState, timing,
chain: bool, now, memory: &FocusMemory)` — the caller's
`newest_open()` pick moves inside so the walk's inputs travel
together; the existing pure units adapt mechanically):

```rust
pub(crate) enum FocusAction {
    /// The work block is over: stop, then open the break — long per
    /// the cadence (the walk).
    OpenBreak { task: TaskId, long: bool },
    /// The break is over: stop it; with the chain knob on, start the
    /// next focus block on the same task (two verbs).
    EndBreak { task: TaskId, chain_next: bool },
}
```

- **The auto-resume is part of the break-end action**, fired from
  the `StopTracking` receipt while the runtime still holds the
  break's task: `StopTracking` → `StartTracking { task: break.task,
  kind: Work }`. It is *never* a reaction to "idle observed after a
  break" — an externally stopped break parks the runtime in idle
  forever (never re-open, never chain), which is D-F16 untouched.
  The chain therefore does not survive the user's stop by
  construction, and there is no idle timer to fight the entry-is-
  truth posture.
- **The chain is uniform, including after the long break and after a
  manually opened respite**: with the knob on, every runtime-driven
  break end resumes focus on the same task — the mode's meaning
  stays "my open entries are focus blocks" (D-F3), and there is no
  second end-of-break behavior to test. The alternative (idle after
  the long break, cycle ends) was considered and declined: it adds a
  state the overlay and the user must model, for no fence value;
  recorded at D-FF5 should review disagree.
- **The chained start rides the landed verb semantics**: the
  kind-blind no-op forces the two-verb sequence (D-F2, now in both
  directions); the implicit-dismissal side effect (D-T5) fires at
  every auto-resume exactly as at any manual start; the entry is
  undoable exactly like manual ones (D-F18 carries).
- **The wake after the chained start arms from the receipt**:
  `entry.start + timing.work()` — the D-F15 receipt-not-reread rule,
  unchanged.
- **Failure path**: the `StopTracking` of an `EndBreak { chain_next:
  true }` answering nothing-open (a state surprise) aborts the
  chain act — the runtime logs, does not start a focus block on
  nothing, and re-derives from published state at the next signal.
  A failed chained `StartTracking` logs-and-continues (one lost
  transition never kills the task — the landed consumer discipline).

### D-FF6 — boundary events fire at the receipts, beside the verbs

Each completed transition hands one typed event to the boundary sink
(§4) *after* its receipts confirm it — `BreakBegan { long }` after
the `TrackingStarted { Respite }` receipt, `FocusResumed` after the
chained `TrackingStarted { Work }` receipt, `SessionIdle` after the
chain-off break-end `StopTracking` receipt. Receipt-riding delivery
is what keeps delivery at most one per real transition even across
the signal-spam window the double-open journey exercises (the
re-fired duplicate hand-off attempt cannot produce a second
successful receipt pair; the journey pins delivery count == 1).

## 4. Phase-boundary notifications (D-FF7, D-FF8)

### D-FF7 — a dedicated `FocusBoundarySink` seam; the reminder ladder and vocabulary untouched

The escalation driver contributes its *shape*, not its ladder:
P12-gentle means no insistence, no re-toast cadence, no volume ramp,
no occurrence keys, no withdrawal — one passive toast and one chime
per boundary, fire-and-forget. The reasons the `Notification` /
`NotificationSink` path and an engine evaluator pass were declined,
each recorded:

- `Notification` requires a `ReminderRef`, and that enum roams the
  wire, storage codec, merge, and snooze-map — a `ReminderRef::Focus`
  variant is a roaming-vocabulary change with tolerant-deserialization
  obligations, far outside this chunk's non-goals. No existing
  variant legitimately carries a boundary (`Adhoc` demands an engine
  entity; minting one would be a forbidden second write path).
- The in-app notification surface reads engine-derived state; a
  runtime-synthesized `Notification` would be a foreign body there.
  The overlay and the status chip are already the in-app boundary
  surface — the OS toast exists for the overlay-closed case.

```rust
// crates/taskboard-app/src/focus.rs
/// One runtime-driven focus-mode boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FocusBoundary {
    /// A focus block ended; the break opened (`long` per the cadence).
    BreakBegan { long: bool },
    /// A break ended and the next focus block auto-started (chain on).
    FocusResumed,
    /// A break ended into idle (chain off).
    SessionIdle,
}

/// Where boundary events go. Infallible at the call site by
/// contract: implementations log-and-drop failures, never propagate
/// (the escalation `apply` discipline — one lost toast never kills
/// the runtime).
pub(crate) trait FocusBoundarySink: Send + Sync + std::fmt::Debug {
    fn boundary<'a>(&'a self, event: &'a FocusBoundary)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
```

Three implementations, mirroring `NotificationSink`'s household:

- `DisabledBoundaries` — the `[focus] notify = false` mount (and the
  honest no-op the tests use as a baseline). A no-op that cannot
  fail, the `DisabledChime` pattern.
- `LogBoundaries` — one `tracing::info!` line with typed fields per
  event (the `LogSink` pattern; the daemon-auto surface).
- `DesktopBoundaries { chime: Arc<dyn ChimeSink> }` — the desktop
  surface: a passive `notify-rust` toast (summary/body from typed
  phrase helpers beside the type — text is never asserted in tests;
  no urgency, no handle tracking, nothing to withdraw, the platform
  timeout) **plus** one chime through the *shared* escalation
  `ChimeSink` arc at a named gentle volume (`FOCUS_CHIME_VOLUME`,
  the escalation floor `0.30` — one volume vocabulary, one audio
  stream owner, D-4; `escalation.rs` stays the workspace's only
  rodio touchpoint because the boundary impl only *calls* the seam).

The toast plumbing lives in `notifications.rs` (the module whose doc
owns OS delivery); the trait and event live in `focus.rs` beside
their consumer (the `ChimeSink`-in-`escalation.rs` precedent). A
`mount_focus_boundaries(notify: bool, sink: SinkKind, audio_allowed:
bool, chime: Arc<dyn ChimeSink>) -> Arc<dyn FocusBoundarySink>`
resolves the D-R13 matrix identically to the reminder sink:
`notify = false` ⇒ disabled; `SinkKind::Log` ⇒ log;
`SinkKind::Desktop` ⇒ desktop with the passed chime arc (which
bootstrap already mounted through `mount_chime` — the escalation
chime is constructed before the focus spawn site, `bootstrap.rs:1116-1146`,
so the ordering already holds; no second `RodioChime`, ever).

### D-FF8 — the knob and its posture

`[focus] notify = true` by default: the boundary signal is the point
of D-F14, "opt-in-adjacent" rides the mode's enablement, and the
gentle-by-construction surface (single chime at the escalation floor
volume, passive toast, no escalation) is the P12 posture. One knob
opts the whole bundle out — no separate chime/toast split (§7).
Upgrade note for the CHANGELOG entry: focus users get sounds at
boundaries from this version; `notify = false` silences (§10 R-2).

## 5. Configuration — the knobs (D-FF9, D-FF10)

### D-FF9 — `[focus]` grows four fields, the landed pattern verbatim

```rust
#[serde(deny_unknown_fields)]
pub struct FocusSection {
    pub enabled: bool,            // existing
    pub work_mins: u64,           // existing
    pub break_mins: u64,          // existing
    /// Auto-start the next focus block when a break ends. Default off
    /// (the fence's default: the user starts the next focus).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chain: bool,
    /// Long-break length in minutes. Default 15 (the pomodoro idiom).
    #[serde(default = "default_focus_long_break_mins", skip_serializing_if = "…")]
    pub long_break_mins: u64,
    /// Every N completed work blocks the next break is long. Default 4.
    #[serde(default = "default_focus_long_break_every", skip_serializing_if = "…")]
    pub long_break_every: u64,
    /// Boundary toast/chime at runtime-driven transitions. Default on.
    #[serde(default = "default_focus_notify", skip_serializing_if = "focus_notify_is_default")]
    pub notify: bool,
}
```

Defaults as named `DEFAULT_FOCUS_*` consts (`LONG_BREAK_MINS = 15`,
`LONG_BREAK_EVERY = 4`, notify default `true` with a hand-written
skip predicate). `deny_unknown_fields` and the absent-table-off
posture carry unchanged; `FocusSection::is_default` keeps the
placeholder serializing to nothing.

Validation joins the flat ladder, always-on (validate what is
written, not what is live — garbage rejected even when `enabled =
false` and even when `chain = false`):

- `long_break_mins`: the existing minutes loop gains a third row —
  `1..=FOCUS_MINS_MAX` (`240`), same typed variant, field payload
  `"focus.long_break_mins"`.
- `long_break_every`: `1..=FOCUS_EVERY_MAX` — new const `24`, a
  garbage-input bound (a typo'd `4000000004` must not decode), never
  a quality ceiling (`1` is legal and means every break long).
  Rejects as `InvalidFocusTiming { field: "focus.long_break_every" }`.
- `chain`/`notify` are bools — nothing to validate.
- No new `ConfigError` variant and **no new stable kind**: every
  focus rejection stays `InvalidFocusTiming` / `"invalid-focus-timing"`,
  the `field` payload distinguishing. The
  `the_new_kinds_extend_the_stable_discriminants` pin extends with
  assertions that the new fields surface the existing kind with the
  right payload (and the roundtrip pin gains the new defaults).

### D-FF10 — `FocusTiming` grows the cadence data; the behavior flags stay app-layer

`taskboard-domain/src/focus.rs`'s `FocusTiming` gains
`long_break: TimeDelta` and `long_break_every: u32` (mint-checked:
strictly positive whole seconds; count `≥ 1`), with new typed
`InvalidFocusTiming::{LongBreak, Every}` variants. Both the runtime
and the overlay consume them (the walk's parameters), so they ride
the existing injected datum through `focus_config()` → `UiSettings.focus`
— no second hand-over seam. `chain` and `notify` are *behavior*
policy, not timing integrity and not needed by the UI countdown:
they stay app-layer, passed as plain arguments at the spawn site
(D-FF12's signature). `focus_config()` mints all four from the
validated section; the expect-on-validated contract is untouched.

## 6. Kiosk posture (D-FF12) and reconciliation extension (D-FF13)

### D-FF12 — the three layers keep holding, untouched; the new machinery sits inside layer 1

1. **Process**: `focus_spawn_decision` is unchanged — no runtime on
   kiosk, so no chain acts and no boundary deliveries exist there.
   The boundary mount additionally rides the landed matrices:
   `mount_chime`'s `audio_allowed` is false on kiosk, and
   `resolve_sink` sends kiosk to `SinkKind::Log` — even a future
   kiosk runtime could only ever reach the log surface. New pins:
   the mount decision on kiosk resolves to log + disabled chime; the
   existing spawn-decision and bootstrap `None`-handle pins hold.
2. **Model**: nothing new exists to gate — no new `ActionId`, no
   `entry_states` row, no `UiEffect` (the knobs are config-only;
   their editor rows are chunk 6's). The existing kiosk action pins
   hold unchanged.
3. **Effect/UI**: the seconds gate gains no term (the overlay's
   countdown correction is derived inside the existing
   `focus_overlay` push, under the existing `Flow::FocusMode` term);
   `the_seconds_gate_stays_false_on_kiosk` holds as-is; the kiosk
   goldens stay byte-identical — the overlay's long-break correction
   is desktop-only reachable.

### D-FF13 — the D-F16 reconciliation table extended (not replaced)

| Perturbation mid-phase | Observed at the next wake | Runtime behavior |
|---|---|---|
| User stops mid-break, chain on | idle | Park. **Never chains, never re-opens** — the chain fires from the break-end receipt, never from idle observation |
| User stops the long break early, starts the next focus | closed short-looking Respite + owned anchor | The walk treats it as the cycle boundary via `decided_long`; position resets |
| Same, but across a restart | closed short-looking Respite, memory empty | Duration inference is the recovery rule (recorded leniency: the cycle may run up to `every − 1` blocks long until the next long break) |
| Foreign start during a break, chain on | new Work entry | The pending break-end + chain act is forgotten; the phase re-anchors on the new entry's `start` |
| Restart mid-break, chain on | rehydrated Respite | Deadline re-derived long-or-short by the walk; the break end (and chain, if on) still fires at the deadline — chain-on survives restart because it is config + phase, not memory |
| Manual respite ends at its deadline, chain on | due `EndBreak` | Treated exactly like a runtime-opened break: closes, chains (uniform rule, D-FF5), `FocusResumed` fires |
| Stale break long past due at boot, chain on | deadline ≪ now | **One** reconciling stop + chain-start at engine `now` — no backdating, no cascade (D-F16's row, extended by the chain act) |
| `long_break_every` or `long_break_mins` changed across a restart mid-cycle | re-derived walk with the new cadence | The next hand-off uses the new values; positions never persist, so nothing to migrate |

## 7. Decision record

| ID | Question | Decision | Status |
|---|---|---|---|
| D-FF1 | Where chain position lives | **Derived, never stored**: one pure backwards walk over published entry history in domain (`focus_work_position` / `focus_break_is_long`, shared by runtime + overlay); contiguous hops within `max(respite, long_break)`, any kind crossed (Work→Work included), only Work counted, stopped at a long respite; the accumulated-counter alternative declined (two derivations that must agree = the double-count bug class) | **decided (the chunk's central ruling)** |
| D-FF2 | The runtime's first owned memory; what survives restart | **One slot**: `FocusMemory { decided_long: Option<TimeEntryId> }` — the long-break decision the runtime itself made, unreadable from history when the break closes early; written at the break-open receipt, one-slot overwrite, read by the walk; **deliberately not recovered across restart** (duration inference is the recovery rule, leniency recorded at the type). Nothing else survives: chain/notify are config, phase is state, position is the walk | **decided (the memory-vs-history ruling)** |
| D-FF3 | The long-break rule | Long iff `position % long_break_every == 0` (position counts the closing block); the open break re-derives the same number structurally (one walk); `every = 1` and `short == long` are coherent degenerate cases | decided |
| D-FF4 | The chain gap tolerance | `max(respite, long_break)` — derived from the user's own knobs, never config; "return within one sanctioned pause and the cycle lives". Leniency recorded (R-5) | recorded |
| D-FF5 | Auto-chain shape | `EndBreak { task, chain_next }`: stop → start `Work` on the break's task, fired from the receipts, never from idle observation; uniform after every break (long, short, manual); indefinite while enabled; the long-break-pauses-the-cycle alternative declined and recorded | **decided** |
| D-FF6 | Boundary delivery timing | Events fire at the *receipts* (after the transition confirms), at most one per real transition — the signal-spam window cannot double-deliver; failures logged-and-dropped | decided |
| D-FF7 | The notification path | **Dedicated `FocusBoundarySink`** (app-layer trait in `focus.rs`, impls in `notifications.rs` for the OS plumbing): `DisabledBoundaries` / `LogBoundaries` / `DesktopBoundaries` (passive notify-rust toast + one chime via the *shared* escalation `ChimeSink` arc). No `ReminderRef` variant (roaming vocabulary), no engine pass (escalation fence), no ladder machinery (P12: no insistence, no re-toast, no ramp, no withdrawal), no in-app notification-surface row | **decided** |
| D-FF8 | Notification posture | `[focus] notify = true` default (rides the mode; opt-out is the knob); one knob gates toast + chime together; chime volume a named constant at the escalation floor | recorded |
| D-FF9 | Knob names/shapes | `[focus] { chain = false, long_break_mins = 15, long_break_every = 4, notify = true }` — plain integers/bools, `deny_unknown_fields`, skip-at-default; validation always-on: minutes `1..=240`, every `1..=24` (`FOCUS_EVERY_MAX`, garbage bound); all rejections the existing typed variant + stable kind `"invalid-focus-timing"` with per-field payloads | recorded |
| D-FF10 | `FocusTiming` growth | Gains `long_break` + `long_break_every` (mint-checked, new typed variants `LongBreak`/`Every`); rides `focus_config()`/`UiSettings.focus` unchanged; `chain`/`notify` stay app-layer spawn args (the UI never needs them) | recorded |
| D-FF11 | The overlay's countdown under long breaks | `presentation::focus_overlay` derives the open block's deadline through the same domain walk (inference-only anchor) and carries a structural long-break flag; accepted edge: after an externally-aborted long break the UI's inference can disagree with the runtime's owned decision until the next boundary — recorded at the presentation fn (R-1) | recorded |
| D-FF12 | Kiosk posture | Three layers untouched and still holding: the new machinery lives inside the layer-1 process gate; no new action/effect/flow to gate; mount pins added (kiosk ⇒ log + disabled chime); kiosk goldens byte-identical | **decided** |
| D-FF13 | Reconciliation | D-F16's table extended with the seven new rows of §6 (external stop never chains; anchor-reset; restart leniency; foreign start forgets the pending chain; restart mid-break still chains; manual respite chains; stale break reconciles once) | recorded |
| D-FF14 | CHANGELOG / ADR | One Unreleased entry covering chain + cadence + boundary notifications (naming the default-on sound and the opt-out); no ADR — an implementation chunk of an accepted fence (the D-F21 precedent) | recorded |

## 8. Test catalogue (TDD-first: each phase opens with its failing tests)

**Phase FF.1 — domain: the cadence data + the walk**

- `focus_timing_mints_the_cadence_fields` /
  `zero_or_sub_second_long_break_rejected_typed` /
  `zero_long_break_every_rejected_typed` (new `InvalidFocusTiming::{LongBreak, Every}`,
  typed `matches!`/equality assertions; the existing mint tests extended).
- `accessors_round_trip_the_cadence` (structural).
- Walk units (FixedClock-style instants, seeded entry maps):
  - `a_fresh_work_block_counts_itself` (cold history ⇒ 1).
  - `consecutive_blocks_with_breaks_accumulate` (W R W R W ⇒ 3).
  - `work_to_work_hops_cross_and_count` (task switch without a break
    does not reset).
  - `a_long_break_ends_the_cycle` (closed Respite `≥ long_break`;
    works before it excluded).
  - `the_decided_long_anchor_ends_the_cycle_even_when_the_duration_undercuts`
    — **the memory-vs-history pin** (same history, anchor on ⇒
    reset; anchor off ⇒ crossed).
  - `a_medium_manual_respite_is_crossed_not_stopping` (duration
    between `respite` and `long_break`).
  - `a_gap_beyond_the_tolerance_starts_a_new_cycle` /
    `a_gap_within_the_tolerance_continues`.
  - `overlapping_rows_never_qualify_as_predecessors` (the D-T8 ghost
    class skipped).
  - `an_open_break_is_long_exactly_when_its_preceding_position_hits_the_cadence`
    (the `focus_break_is_long` wrapper; `0 % every` excluded ⇒ a
    cold break is short).

**Phase FF.2 — app: the knobs**

- `focus_section_new_knobs_decode_defaults_and_skip_at_default` /
  `absent_table_still_serializes_nothing` (the placeholder pin
  extended: chain false + notify true + 15/4 all at defaults ⇒
  nothing).
- `validate_rejects_garbage_long_break_minutes_and_every` — zero /
  241 minutes; zero / 25 every; 24 every accepted (the boundary
  pin); typed payloads per field; rejected while disabled.
- `the_focus_stable_kind_covers_the_new_fields` (kind ==
  `"invalid-focus-timing"`, payload distinguishes) + the
  discriminants pin extended.
- `serde_round_trip_preserves_the_focus_knobs` (joins the roundtrip
  pin).
- `focus_config_mints_the_cadence_when_enabled_and_none_when_disabled`.
- Integration (`tests/config.rs` c-family): partial file with
  `[focus] long_break_every = 0` ⇒ typed kind in the envelope;
  `TASKBOARD_FOCUS__CHAIN` / `TASKBOARD_FOCUS__NOTIFY` env overrides
  win (the c2/c12/c13 recipe).

**Phase FF.3 — app: the runtime (chain + boundaries)**

Step units (pure, extended fixtures):

- `the_handoff_names_the_long_break_at_the_cadence` (every = 4:
  positions 1–3 ⇒ `OpenBreak { long: false }`, 4 ⇒ `long: true`).
- `end_break_carries_the_chain_decision` (chain on ⇒
  `EndBreak { chain_next: true }`; off ⇒ false; idle parks).

Driver journeys (`start_paused` + shared `FakeClock` + real engine +
**a fake `FocusBoundarySink` recording `Vec<FocusBoundary>` behind a
Mutex** — the sink-seam typed assertions; the existing harness's
`tick` lockstep and seed fixture reused):

- `the_fourth_break_is_long_and_runs_the_long_length` — chain off,
  four blocks driven: the fourth hand-off opens a break whose
  deadline is `start + long_break`, closed there; the sink sees
  exactly `[BreakBegan { long: false } ×3, BreakBegan { long: true },
  SessionIdle]` in order.
- `chain_on_resumes_focus_after_every_break` — chain on: block →
  short break → auto-resumed Work entry on the same task (start ≈
  engine now) → … → long break → auto-resume; the sink sequence is
  `[BreakBegan{false}, FocusResumed, …, BreakBegan{true},
  FocusResumed]`; each resumed block then hands off at its own
  deadline.
- `an_externally_stopped_break_never_chains` — chain on, test stops
  mid-break, advance far past: idle stays idle, no `FocusResumed`
  ever delivered (the entry-is-truth pin for the chain).
- `a_foreign_start_during_a_break_supersedes_the_pending_chain` —
  mid-break the test starts task B: at the break's deadline nothing
  fires; B re-anchors; no duplicate events.
- `restart_mid_cycle_recovers_the_position_from_history` — seed the
  repo with two closed W–R pairs, spawn the runtime: the first
  hand-off is **short** (position 3, not 5 — the no-double-count
  pin) and the fourth block's break is long.
- `the_owned_decision_survives_an_early_external_long_break_close` —
  runtime opens the long break, test stops it at one minute, test
  starts the next block: the cycle reset holds *within the process*
  (the next long break lands at the correct block, which the
  inference-only walk would misplace).
- `boundary_notifications_fire_once_per_transition` — the
  signal-spam tick pattern across a hand-off (the double-open
  journey's shape): sink deliveries == the real transition count,
  never more (correctness, not a ceiling).
- `a_stale_break_reconciles_once_with_the_chain` — seeded stale
  Respite, chain on: exactly one stop + one chained Work start at
  engine now, no cascade.
- `the_runtime_exits_when_the_broadcast_closes` (holds, adapted to
  the new signature).

Mount/bootstrap units:

- `mount_focus_boundaries_resolves_the_posture_matrix` — notify
  false ⇒ disabled; log sink ⇒ log; desktop ⇒ desktop with the given
  chime arc; kiosk mode ⇒ log + disabled chime (the D-FF12 pin).
- `notify_false_and_chain_false_spawn_the_same_runtime_shape` — the
  knobs never affect the spawn decision (only the act), pinned at
  `focus_spawn_decision` + the bootstrap `None`-handle tests (they
  hold unchanged).

**Phase FF.4 — ui-slint: the countdown correction**

- `focus_overlay_derives_the_long_break_deadline` — seeded history
  putting an open Respite at cadence position 4: remaining =
  `start + long_break − now` (arithmetic-structural, boundary
  instants).
- `focus_overlay_flags_the_long_break` — the structural long flag
  (typed enum/bool on `FocusOverlay`), never the rendered text.
- `focus_overlay_short_break_and_focus_deadlines_hold` (the existing
  units keep passing unchanged — the Work path and cold-break path
  are untouched by the walk).
- Seconds gate + kiosk pins: **no change expected** — the existing
  `the_seconds_gate_covers_the_focus_overlay` /
  `the_seconds_gate_stays_false_on_kiosk` /
  `entry_states` kiosk pins must stay green untouched (their
  unchangedness is the D-FF12 assertion). No new action ⇒ no keymap
  snapshot regen; no slint-tree change ⇒ no golden regen.

**Phase FF.5 — wiring, CHANGELOG, gates**

- `CHANGELOG.md` Unreleased: one Added entry (D-FF14) naming the
  default-on boundary sound and `notify = false`.
- Full AGENTS §10 gate suite through `scripts/run-quality-gates.sh`
  (fmt, clippy `-D warnings`, nextest, doc tests, `cargo deny`, the
  three-command coverage sequence with the quarantine step, trailing
  `rm -rf target/llvm-cov-target`).

TDD order inside each phase: pins/journeys first (red), then the
production change (green), then the sweep. FF.1 lands first (the
walk + mint compile standalone); FF.2 before FF.3 (the knobs mint);
FF.4 last (its correction rides the finished walk). The existing
`focus.rs` pure units and journeys are updated to the extended
signatures *within* the phases that change them — no parallel old/new
test files.

## 9. Phasing & commit plan (one branch, FF.1 → FF.5 in order)

| Phase | Delivers | Commit shape |
|---|---|---|
| FF.1 | `FocusTiming` cadence fields + typed variants + the walk (domain `focus.rs`) | `feat(domain): the focus chain walk` |
| FF.2 | The four `[focus]` knobs, validate ladder rows, `focus_config()` mint, config tests | `feat(app): the focus chain and boundary knobs` |
| FF.3 | `FocusMemory`, the extended step, the chain act, `FocusBoundarySink` + impls + mount, bootstrap wiring, journeys | `feat(app): the focus auto-chain and boundary notifications` |
| FF.4 | `presentation::focus_overlay` deadline correction + long flag + units | `feat(ui-slint): the overlay's long-break countdown` |
| FF.5 | Gates green through the flock wrapper; CHANGELOG rides FF.4's final state | (folded — no fixups-with-duplicate-subjects commits) |

Every commit standalone (builds, meaningful subject, no WIP) — the
rebase-merge hygiene rule.

## 10. Risks & mitigations

- **R-1 — the UI/runtime long-break divergence**: after an
  externally-aborted long break, the overlay's inference-only walk
  can disagree with the runtime's owned anchor (wrong countdown for
  one break window, clamped at zero). Accepted + documented at the
  presentation fn; the alternative (publishing the decision into
  state) violates the schema/wire fences. Self-corrects at the next
  boundary push.
- **R-2 — default-on boundary sound at upgrade**: existing
  `[focus] enabled` users gain chimes/toasts. P12-gentle by
  construction (single chime at the escalation floor volume, passive
  toast, no escalation, nothing on kiosk or daemon-auto beyond the
  log line); one-knob opt-out; the CHANGELOG entry names it.
- **R-3 — walk cost per push**: the overlay re-derives per state
  push and per second while open; the walk terminates at the first
  boundary/gap in real data, but a months-long uninterrupted chain
  walks its whole history per frame. Recorded; acceptable v1 (a
  BTreeMap scan, no allocation per hop); the semantics forbid
  windowing (a truncated walk under-counts). Revisit only if a
  profile says otherwise.
- **R-4 — indefinite auto-chain**: with chain on the session
  alternates forever. The user's stop always wins (never re-open,
  never chain from idle); disabling the knob ends it at the next
  boundary. Recorded as the ruled semantics, not a risk to fence.
- **R-5 — the gap tolerance's ceiling**: `max(respite, long_break)`
  with `long_break_mins = 240` lets a 4-hour gap continue the cycle.
  Leniency recorded at the walk's doc; review adjusts by bounding
  the tolerance, not by re-shaping the walk.
- **R-6 — double-surfacing with break reminder rules**: the respite
  family's break rules (#495) fire on open-respite duration; boundary
  notifications fire on transitions. Different triggers, both honest;
  no dedup, recorded.
- **R-7 — spawn signature width**: `spawn_focus_runtime` grows to
  seven parameters (state, timing, chain, boundaries, engine,
  signals, clock) — at the `clippy::too_many_arguments` boundary.
  If the lint fires, bundle `(timing, chain)` into a small knobs
  struct; never an `allow` without a recorded reason.
- **R-8 — knob-name stability for chunk 6**: the settings editor
  (#625) grows rows at these landed names. The names are pinned here
  (§5/D-FF9); a rename after chunk 6 starts costs its snapshots —
  rename now or never.

## 11. Size, handoff, coordination

- **Estimated diff**: ≈ **1.0–1.2k lines** including tests — domain
  `focus.rs` ~220 (cadence fields + variants + walk + units), app
  config ~140 (four knobs + validate + pins), app `focus.rs` ~500
  (`FocusMemory` + step/driver extensions + seam + journeys), app
  `notifications.rs` ~90 (toast/log impls + phrases), bootstrap ~50,
  ui-slint presentation ~80 (deadline correction + flag + units),
  CHANGELOG one entry. **M** — consistent with the dispatch table's
  sizing.
- **Dependencies**: none outside the workspace; `cargo deny`
  untouched (notify-rust and rodio are already `taskboard-app`
  dependencies via the landed sink/chime; no new crate). Gates run
  under `scripts/run-quality-gates.sh`.
- **Handoff**: stage 2 implements FF.1–FF.5 on `feat/focus-followups`
  in `../taskboard-WT-feat-focus-followups`; stage 3 reviews (the
  dispatch §6 trap list: verbs-only, minimal memory with the
  restart rule pinned, D-F16 untouched-and-extended, gentle +
  kiosk-unreachable notifications, typed knob validation), runs the
  gates, opens the PR titled `feat(app,ui-slint): the focus
  follow-ups` closing **#600**. Worktree removal is stage 3's final
  act (doctor rules; `--force` per the lifecycle doc). **No push
  during planning** — this commit stays local.
