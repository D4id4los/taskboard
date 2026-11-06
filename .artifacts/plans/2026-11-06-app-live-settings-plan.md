# [Plan] GUI Review Round 5, Chunk 5 — Live Settings (F17 + D3)

- **Date**: 2026-11-06
- **Status**: STAGE-1 PLAN (dispatch protocol stage 1) — written for the
  stage-2 implementation agent; no code in this commit.
- **Origin (normative)**:
  `.artifacts/plans/2026-11-05-gui-review-round5-plan.md` §2 row F17,
  §3 D3, §4.5, §6 (test hooks), §8 (risk 3); dispatch plan
  `.artifacts/plans/2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1 (shared execution model + code-quality mandate) and §6 (this
  chunk's covers/acceptance, prompts 5.1–5.3).
- **Branch / worktree**: `feat/app-live-settings` in
  `../taskboard-WT-feat-app-live-settings`, branched off the fetched
  `origin/main` @ `bfc735c3` (local `main` was 7 commits behind and is
  deliberately not used; worktree-doctor passed: 175G free, floor 25G).
- **Line-number drift note**: the dispatch prompt cites line numbers
  verified at `main @ 58ae9c55`; every anchor below was re-verified
  against this worktree's tree (`bfc735c3`) and is the number stage 2
  should trust. The prompt's `resolve_* accessors 1449-1511` now live
  at `config.rs:1605-1721` (`sync_actor_config`,
  `reminder_engine_config`, `resolved_day_calendar`, `day_calendar`,
  `focus_config`, `files_sync_config`); `handle_settings 335-422` is
  `ui_requests.rs:337-522`; `ui.rs resolve_settings_snapshot 423-451`
  is `settings_snapshot_from_config` at `ui.rs:306-479`.

---

## 1. Scope

Every editable General / Reminders / Sync-Timing / Display setting
applies **live**: commit → app-lane prove-and-write (unchanged) →
`StateCommand::UpdateSettings` → the engine updates `AppState`,
publishes, and announces the change; every consuming actor re-reads the
settings per iteration instead of holding a spawn-time capture. The
per-setting apply-semantics registry makes live the default;
`OnRestart` requires a written justification.

### Non-goals (dispatch §6)

- The deadline-driven firing loop and the 30 s → 1 s tick default
  (chunk 6 — this chunk only makes the tick cadence/enabled/defaults
  live-adjustable).
- Config schema changes (no new keys; the existing sections stay).
- Sqlite persistence of settings (the file overlay stays the store;
  the engine's settings are memory-only, re-seeded from figment at
  boot).
- The Web/UI surfacing of `justification` text (the registry carries
  it; v1 enforcement is tests, the slint badge stays as-is).
- The connector *lifecycle* settings (`[sync] backend`, connector
  entries, credentials) — already managed live by the connections
  lane; untouched.

---

## 2. Machinery map, verified against this tree (`bfc735c3`)

| Capture / seam | Location today | Chunk 5 disposition |
|---|---|---|
| Engine reminder knobs | `EngineParts.reminders` (`taskboard-state/src/actor.rs:321-322`), captured at `spawn_state_engine` (`:266`); tick arm reads `reminder_cfg.tick` at `:598`, `reminder_cfg.enabled` gates the tick work at `:562`; defaults flow into `execute_command` (`:426`, param `reminder_defaults`) and `reminder_tick` (`:574`) | Re-read per iteration / per command from `core.settings`; tick re-arms on change |
| Engine day roll | `EngineParts.day_roll` (`:323`); calendar passed into every plan/tick call | Calendar re-minted per use from `core.settings.day_start_time` + the boot-fixed UTC offset (`DayCalendar { day_start_time, offset }`; the offset is device-boot state by the `resolve_day_start` convention) |
| Sync actor cadence | `SyncActorConfig` minted once (`config.rs:1605`), injected at `bootstrap.rs:1374`/`:1458`; `reschedule()` (`taskboard-sync-nextcloud/src/actor.rs:351-364`) reads it per call; loop selects on the system broadcast (`:274-289`) | Per-cycle read via the engine facet (D-5.4); `SettingsChanged` re-arms the deadline (min rule) |
| Files-sync actor cadence | `FilesSyncConfig` at spawn (`files_sync_spawn`, `bootstrap.rs`); loop selects on the broadcast (`taskboard-sync-files-nextcloud/src/actor.rs:130-160`) | **Stays boot-captured** (D-5.6 note); `SettingsChanged` arm added as a documented no-op |
| Escalation driver | `reminder_cfg.defaults` + `config.resolved_day_calendar()` captured at `bootstrap.rs:1284-1295`; loop already re-reads the ArcSwap per iteration and wakes on `EngineSignal` (`escalation.rs:484-556`) | `defaults`/`day` become per-iteration reads from `state.load().settings`; no new wake needed (the settings publish fires `StateUpdated`) |
| Focus runtime | `config.focus_config()`, `config.focus.chain`, `mount_focus_boundaries(config.focus.notify, …)` captured at `bootstrap.rs:1314-1327`; loop re-reads the ArcSwap per iteration (`focus.rs:225-250`) | Timing/chain/enabled re-read per iteration; the boundary posture stays boot-captured (tied to the sink — D-5.6) |
| Notifier sink | `resolve_notifier_sink` one `Arc` per process (`bootstrap.rs:1025-1040`); consumer takes the sink at spawn (`bootstrap.rs:1236`) | **Stays boot-captured** — justified `OnRestart` (D-5.6) |
| UI derivations | `UiSettings.day_start`, `due_soon_horizon`, `reminder_defaults`, `focus` (`taskboard-ui-slint/src/lib.rs:92-130`), copied into adapter fields (`adapter.rs:154-167`), read at refresh/derive sites (`adapter.rs:958-1027`, `:1207`, `:1256`, `:1499`; `models.rs:672-1001`) | The adapter re-reads `state.settings` at each refresh; only the boot-fixed UTC offset stays a struct field (D-5.7) |
| Write path | `run_editor_write` (`taskboard-app/src/ui_requests.rs:523-539`): `prove_and_write` → `fresh_settings_snapshot` (a **file** re-decode, engine-blind) | After a successful write: env-lock guard → mint domain snapshot → `UpdateSettings` → reply resolves from the engine's ArcSwap (D-5.3) |
| Env-lock set | Hand-over only (`ui.rs:300-311`); the adapter gates commits (`adapter.rs:4045-4174`) | App-side guard added at `run_editor_write` (defense in depth; the "absorption" moves into the apply path) |
| Apply registry | `settings_snapshot_from_config`'s hardcoded `applies` loop (`ui.rs:444-464`, all `OnRestart`) | Becomes the registry (D-5.8); badges flip |
| `StateCommand` / `CommandOutcome` | `taskboard-domain/src/messages.rs:20` (38 variants, `Debug, Clone, PartialEq, Serialize, Deserialize`); outcome `taskboard-domain/src/command.rs:228` | New variant + outcome variant (D-5.2) |
| `AppState` | `taskboard-domain/src/state.rs:25` — `Serialize` only (no `Deserialize` derived), snapshot-projected by the insta suites via `PublishedTail` (`tests/snapshots.rs:178-191`) | New `settings` field; existing 78 `.snap` files do **not** churn (the tail is a projection) |

---

## 3. Architecture decisions (the settling points)

### D-5.1 — `SettingsSnapshot` shape: plain projection + three A1 re-mints, living in `taskboard-domain`

New `taskboard-domain/src/settings.rs`. The snapshot is the **typed,
validated** settings projection the engine owns:

```rust
pub struct SettingsSnapshot {
    pub day_start_time: chrono::NaiveTime,          // already typed
    pub reminders_enabled: bool,                    // unconstrained
    pub tick_secs: TickSecs,                        // A1: >= 1
    pub reminder_before: ReminderOffset,            // existing newtype
    pub reminder_sink: ReminderSinkChoice,          // domain mirror of SinkChoice
    pub poll_interval_secs: TickSecs,               // A1: >= 1 (same validation kind)
    pub backoff_initial_secs: TickSecs,             // A1: >= 1
    pub backoff_max_secs: TickSecs,                 // A1: >= 1
    pub due_soon_days: DueSoonDays,                 // A1: <= 3652 (0 legal)
    pub focus_enabled: bool,
    pub focus: Option<FocusTiming>,                 // existing A1 type (try_new)
    pub focus_chain: bool,
    pub focus_notify: bool,
}
```

- The three constrained scalars reuse the **existing validation
  kinds**, per the origin's "reuse existing validation kinds":
  `TickSecs` (the `reminders.tick_secs == 0` /
  `InvalidDuration` rule, also covering the three sync secs fields
  whose `validate()` arms are the same ≥ 1 rule) and `DueSoonDays`
  (the `> 3652` / `InvalidDueSoonDays` rule). Hand-rolled
  private-field newtypes with fallible `new`/`try_from<u64>` — the
  house A1 pattern; no derive crates; deliberate leniency documented
  at the type.
- `backoff_initial ≤ backoff_max` is a **cross-field** rule: it stays
  out of the newtypes and is enforced where it lives today — the app
  lane mints from a `validate()`-proven config, and the overlay's
  prove gate refuses an ordering violation before any mint. Documented
  on the snapshot: the type carries no cross-field invariant.
- `FocusTiming` already mints via `try_new` with its typed
  `InvalidFocusTiming` lattice — reused verbatim; `focus: None` while
  the section is disabled (the `focus_config()` precedent).
- The raw-string fields (`day_start_time`, `default_before`) become
  typed at mint (`NaiveTime` parse, `parse_before` → `ReminderOffset`
  with the existing at-due zero rule) — no raw strings cross the
  command boundary, so the engine never re-validates.
- **Serde re-mint, not derive-and-forget** (`no validate-and-forget`):
  the snapshot derives `Serialize`; `Deserialize` re-mints through the
  newtypes via the `#[serde(try_from = "u64")]` precedent
  (`taskboard-domain/src/habit.rs:24`, `reflection.rs:81`), with the
  documented leniency that an out-of-range integer deserializes to the
  type's default (mirroring `UiSettings`'s lenient-read contract) —
  the trust boundary for *engine input* is the command path, where
  minting is fallible and refusing is the contract; serde leniency
  covers only the serialized-state readers (tests, future embedders).
  Derives: `Debug, Clone, PartialEq, Eq, Serialize` (+ the
  `try_from`-shaped `Deserialize`) — everything `StateCommand` and
  `AppState` require.
- `ReminderSinkChoice` is a small domain enum (`Auto | Log |
  Desktop`); the app's `SinkChoice` and the UI lane's `ReminderSink`
  map to it at their existing boundaries (both mappings already exist
  in one direction each).
- The UI lane's `requests::SettingsSnapshot` (the editor-row echo)
  **stays**: it is the presentation projection (raw display strings,
  facts, applies). `settings_snapshot_from_config` gains a sibling
  `settings_snapshot_from_domain(&domain::SettingsSnapshot) -> …` used
  for post-apply replies; the config-derived constructor remains for
  hand-over (it also renders the read-only facts, which the domain
  type does not carry).

### D-5.2 — `StateCommand::UpdateSettings` and the engine apply

- `StateCommand::UpdateSettings { settings: taskboard_domain::SettingsSnapshot }`
  (the domain grows the variant; no app types in messages).
- `CommandOutcome::SettingsApplied` (the receipt).
- The engine handles it as a special arm in `execute_command`, shaped
  like the existing `RequestSync` early path (`actor.rs:684-702`) — it
  does **not** enter `plan_journaled_command` (no journal row, no
  outbox op, no sqlite write; the file overlay is the store):
  1. `core.set_settings(settings)` — memory-only, like
     `last_updated`/`sync_counters`;
  2. `publish(...)` — `AppState.settings` swaps, `StateUpdated`
     fires, every ArcSwap reader (escalation, focus, UI) wakes;
  3. `system_out.send(SystemEvent::SettingsChanged)` — the doorbell
     for the actors that do not read the engine-signal bus (sync
     actors, files-sync);
  4. reply `Ok(CommandOutcome::SettingsApplied)`.
- `SystemEvent::SettingsChanged` joins the broadcast
  (`taskboard-domain/src/messages.rs:734`): a unit variant. Every
  existing wildcard match that names the ignorable vocabulary
  (engine `actor.rs:542-545`, sync `actor.rs:279-284`, files-sync
  `actor.rs:142-151`) grows the variant; the files-sync arm stays a
  documented no-op (D-5.6).
- **Minting at the trust boundary**: the engine receives an
  already-minted snapshot (the app minted it from a proven config).
  The engine does not re-validate scalars — the newtypes make an
  invalid value unrepresentable, which *is* the validation. There is
  no `SettingsSnapshot::raw` escape hatch.

### D-5.3 — Persist-then-command ordering and the failure arms

`run_editor_write` (`ui_requests.rs:523`) becomes:

1. **Env-lock guard (new, first)**: if `editor_field_key(field)` ∈
   `AppConfig::env_overridden_keys(Some(&workspace.config_write_path))`
   → `Err(SettingsError::Invalid { field, kind: OutOfRange })`? No —
   a new dedicated kind is wrong too; the honest reply is
   `SettingsError::PersistRejected`… **decision**: new
   `SettingsError::EnvLocked { field }` variant (the typed-outcome
   rule favors a nameable outcome over reusing an inexact one). The
   UI adapter's existing env-locked gating stays (it prevents the
   attempt); the app-side guard is the authority (the adapter is a
   client, the app owns the file).
2. `prove_and_write` — unchanged semantics. On
   `OverlayError` → mapped typed reply (existing
   `map_overlay_error`), **no command sent**, engine state untouched,
   file untouched (the prove gate's guarantee).
3. On write success: re-decode the file layer
   (`extract_file_only`, the existing `fresh_settings_snapshot`
   read) → mint the domain snapshot via the new
   `AppConfig::settings_snapshot()` constructor (D-5.5). A decode
   failure here is `PersistFailed` (typed; unreachable in practice —
   the gate just proved this document boots — but typed, never
   `expect`).
4. `engine.execute(StateCommand::UpdateSettings { settings }).await`:
   - `Ok(SettingsApplied)` → the reply snapshot resolves from
     **the engine's ArcSwap** (`workspace.engine.shared_state().load().settings`
     projected through `settings_snapshot_from_domain`) — the reply
     echoes engine truth, not the file (one source of truth; the
     origin's "reply snapshot round-trips through the engine").
   - `Err(ExecuteError::EngineGone)` (draining/stopped engine) →
     `Err(SettingsError::Gone)`; the file *is* written (restart picks
     it up), the running engine keeps the old values — the typed
     refusal tells the UI to say so. No partial-state repair exists
     or is needed: the file remains the boot authority and a written
     file with a stale engine converges at restart.
   - `Err(Rejected(_))` cannot occur (the engine never rejects this
     variant); treat it as `Gone` defensively? **decision**: map any
     non-`EngineGone` error to `SettingsError::Gone` — one arm, no
     speculative lattice.

The debounce lane is untouched (widths/geometry never carried
settings); `handle_settings` stays undebounced.

### D-5.4 — Broadcast `SystemEvent`, not a watch channel (picked)

**Picked: broadcast `SystemEvent::SettingsChanged` on the existing
system bus.** The trade-off, recorded:

- *For broadcast*: every sleeping actor that must wake **already
  selects on the system broadcast** (sync `actor.rs:247-289`,
  files-sync `:130-160`; the engine itself publishes on it), so the
  marginal wiring per consumer is one match arm. No new channel class
  flows through `bootstrap.rs` (whose constructor surface is already
  at its argued maximum). A no-subscriber send is legal (the bus runs
  with zero subscribers in engine-only tests — architecture.org's
  recorded property), so the engine needs no subscriber-count
  awareness. Matches the recorded channel table ("System
  Notifications → broadcast for global status changes").
- *Against watch*: a `watch::Sender<SettingsSnapshot>` would give
  late-joining consumers the latest value without a state read — but
  every consumer here either (a) reads `AppState` per iteration
  anyway (escalation, focus, UI), or (b) can read settings through
  the engine facet per cycle (sync actors, D-5.5). The value lives in
  the state; the event is a doorbell. A `watch` also needs per-actor
  receiver plumbing at spawn (28 `spawn_state_engine` call sites are
  engine-side, but each *sync/files-sync* spawn site grows an
  argument) for a benefit the per-cycle re-read already delivers.
- *Lag semantics (accepted)*: `Lagged` on the broadcast means a
  consumer misses a re-arm until its next natural wake (poll deadline,
  state publish). Since consumers re-read the *value* per iteration,
  a missed doorbell costs at most one stale-cadence cycle, and the
  next publish re-wakes. Documented at the variant; the acceptance
  journey pins "visible within one re-arm", not "instant".

### D-5.5 — Per-consumer re-read points and the boot seeding path

**Boot seeding.** `AppConfig` gains
`fn settings_snapshot(&self) -> taskboard_domain::SettingsSnapshot`
— the single mint over the validated config, absorbing the typing
logic the `resolve_*` accessors scatter (`reminder_engine_config`'s
negation mapping, `resolved_day_calendar`'s parse, `focus_config`'s
mint). The existing accessors stay (they serve non-settings
consumers: files-sync, CLI, day-window projections) but
`reminder_engine_config`/`focus_config` become thin wrappers over the
snapshot's fields where the shapes coincide — or, where they do not
(`ReminderEngineConfig`'s `Duration` shape), remain and are
documented as boot-glue; **no forced unification beyond the snapshot
constructor** (frugality). `spawn_state_engine` gains a
`settings: SettingsSnapshot` parameter; `EngineCore` gains a
`settings` field set by the spawn (from `spawn_state_engine`'s
argument, not `from_persisted` — the 30+ test call sites of
`from_persisted` keep their signature; `EngineCore::from_persisted`
defaults the field to the documented test-policy value, mirroring
`ReminderEngineConfig::disabled()`'s precedent, and the spawn
overwrites it). `EngineCore::app()` carries it into `AppState`.

**Consumer re-reads, exactly:**

1. **Engine tick arm** (`actor.rs:559-598`): `reminder_cfg.enabled`
   → `core.settings().reminders_enabled` (read at the top of the
   arm); `next_tick = now + core.settings().tick_secs` (the re-arm
   reads live values); `reminder_tick`'s `&reminder_cfg` argument
   becomes the live snapshot; `execute_command`'s
   `reminder_defaults` argument becomes a live read at the call site.
   The tick **precondition** becomes
   `timer_armed: day_roll_enabled || /* re-evaluated per loop */ core.settings().reminders_enabled`
   — computed fresh each loop iteration so a live disable stops
   constructing the sleep (the existing paused-time policy survives:
   the day roll is always production-enabled, so the timer never
   disarms in production; `day_roll.enabled == false` test suites
   keep their zero-timer property while reminders stay off in their
   fixtures).
2. **Engine `UpdateSettings` arm**: after the apply, `next_tick` is
   recomputed (`now + new tick`) so a cadence change is visible at
   the very next wake — the acceptance's "tick change visible at the
   next loop iteration".
3. **Engine day roll**: `day_roll.calendar` per use →
   `DayCalendar { day_start_time: core.settings().day_start_time,
   offset: boot_offset }`; the boot offset travels in `EngineParts`
   (one new field, resolved once at spawn — the fixed-offset
   convention is unchanged). The watermark (`last_roll`) keeps its
   same-date guard: a day-start change that moves "today" applies at
   the next tick; a backwards move that re-enters the already-rolled
   date does **not** re-roll (the watermark's documented property —
   pinned by a test, recorded as intended semantics, not debt).
4. **Escalation driver** (`escalation.rs:484`): signature drops
   `defaults: ReminderDefaults, day: DayCalendar` → takes only the
   ArcSwap it already has; `awaiting_alarms(&state.load(), …)` reads
   `state.settings` for defaults and re-mints the calendar per
   iteration (offset via one captured boot value, same convention).
   No new wake: the settings publish fires `StateUpdated`, which the
   driver already selects on.
5. **Focus runtime** (`focus.rs:225`): drops the `timing`/`chain`
   params; per iteration reads timing (`state.settings.focus` —
   `None` ⇒ park in idle, the existing no-timing arm), chain, and
   `focus_enabled` (false ⇒ idle; a runtime that booted spawned stays
   spawned but inert until re-enabled — the park arm already exists
   for the disabled-timing shape). Boundary posture (`notify`,
   sink kind) stays boot-captured (D-5.6).
6. **Connector sync actor**: `SyncStateReader` (domain
   `persistence.rs:68`, implemented by `EngineHandle`) grows
   `fn read_settings(&self) -> SettingsSnapshot` — one facet, one
   implementor in production (the engine), fakes in tests. The
   actor's cycle start fetches it once per cycle: `cycle(...)` and
   `reschedule(...)` take the cadence triple from that read instead
   of `&cfg`'s fields (`cfg` keeps clock/day/app_version). New
   select arm:
   `Ok(SystemEvent::SettingsChanged)` →
   `deadline = deadline.min(now + read_settings().poll_interval)` —
   the **min rule**: a shortened interval applies immediately, a
   lengthened one never postpones an already-due poll. Backoff knobs
   apply at the next `reschedule` naturally (post-failure).
7. **Files-sync actor**: no engine facet exists (not a board
   connector — DR-8's recorded boundary). `SettingsChanged` joins its
   ignore arm with a comment naming the limitation; its cadence
   (`[files_sync] poll_secs`, else the `[sync]` triple **as of
   boot**) stays restart-scoped. Recorded as the one cadence surface
   this chunk does not make live; a `backlog:` issue is the
   follow-up vehicle (thread a settings reader into the files-sync
   gate), not in-chunk scope growth.
8. **Notifier sink**: boot `Arc` stays (D-R13's one-Arc-per-process
   rule); `reminders.sink` is a justified `OnRestart` registry entry
   (D-5.6). The notifier consumer needs no change.
9. **UI derivations**: the adapter's per-refresh derive pass reads
   `state.settings` (D-5.7).
10. **CLI verbs** (`commands/repeat.rs:247-280`, `habits.rs:323`,
    `reminders.rs:125`): one-shot processes that re-read config at
    their own boot — restart-scoped by nature, no change, noted so
    stage 3's "no spawn-time copy" grep does not flag them.

### D-5.6 — The apply-semantics registry: two justified `OnRestart` entries, everything else live

**Location: `taskboard-app/src/ui.rs`, beside
`settings_snapshot_from_config`** (the apply authority is the config
owner — the origin's plan D12 rule; the UI lane's
`requests::SectionApply` stays the wire mirror). The registry is a
pure function:

```rust
fn settings_apply_registry() -> BTreeMap<&'static str, ApplySemantics>
struct ApplySemantics { apply: SectionApply, justification: &'static str }
```

- The UI lane grows `ApplySemantics` (or the map value type)
  in `requests.rs`; `SettingsSnapshot.applies` becomes
  `BTreeMap<String, ApplySemantics>`.
- **Every editable key is `Live`** — the full set:
  `preferences.day_start_time`, `reminders.enabled`,
  `reminders.tick_secs`, `reminders.default_before`, `reminders.sink`*,
  `sync.poll_interval_secs`, `sync.backoff_initial_secs`,
  `sync.backoff_max_secs`, `ui.due_soon_days`, `focus.enabled`*,
  `focus.work_mins`, `focus.break_mins`, `focus.chain`,
  `focus.long_break_mins`, `focus.long_break_every`,
  `focus.notify`* — *except the three starred, which carry
  justifications:
  - `reminders.sink` → `OnRestart`, justification: "the notifier sink
    is one boot Arc per process (D-R13); a live swap must rebuild the
    sink chain and the escalation/focus posture with it — deferred to
    a dedicated chunk".
  - `focus.enabled` → `OnRestart`, justification: "the runtime spawn
    decision is boot-time (`focus_spawn_decision`); a boot-disabled
    runtime cannot be live-enabled without a respawn path — the
    live-disable/inert direction applies to an already-spawned
    runtime, the asymmetric half is restart-scoped".
  - `focus.notify` → `OnRestart`, justification: "the boundary
    posture is resolved into the sink arc at boot
    (`mount_focus_boundaries`); it rides the sink's restart
    semantics".
- **Deviation from D3's "v1 ships with an empty exception list",
  recorded**: three editable knobs cannot honestly apply live without
  rebuilding boot-glue arcs; the justification field is exactly the
  documented-decision mechanism the user ruled for, and an empty-fake
  (`Live` badges on values that do not move) is the "pretend-live"
  failure D5 exists to prevent. The read-only *facts* inventory keeps
  its existing `SectionApply` labels (unchanged; the registry governs
  editable rows only).
- **The enforcement test** (acceptance: "the registry test fails an
  unjustified `OnRestart` entry"): a `taskboard-app` unit test walks
  the registry and asserts every `OnRestart` entry carries
  non-empty justification text, **and** every key in the editable
  `Field` set appears exactly once (a key added to the editor without
  a registry entry fails the test — the registry cannot silently
  drift from the editor's field lattice).
- The settings-view badges (`settings-view.slint:134-219`'
  `*-on-restart` properties, fed from `adapter.rs:4174`) flip to the
  registry's verdicts mechanically — the adapter already renders from
  `applies`; only the app-side map values change. The `RoleText`
  "takes effect at restart" sentence (`settings-view.slint:494-530`)
  stays for the justified rows and disappears for the rest.

### D-5.7 — The UI's derivations read the published state

`UiSettings` keeps `day_start: (NaiveTime, FixedOffset)` **only as
the boot-fixed offset carrier**? No — decision: `UiSettings` drops
`day_start`, `due_soon_horizon`, `reminder_defaults`, `focus`; the
adapter holds one new field `boot_offset: chrono::FixedOffset` (the
device's fixed UTC offset — genuinely boot-fixed state, the
`resolve_day_start` convention) and derives everything else per
refresh from `state.settings`:

- day calendar = `DayCalendar { day_start_time:
  state.settings.day_start_time, offset: boot_offset }` (minted at
  the derive pass — `adapter.rs:958-959`, `:2204-2205`, `models.rs`
  call sites);
- due-soon horizon = `state.settings.due_soon_days` (the typed
  newtype's `TimeDelta` accessor);
- reminder defaults = `state.settings` (the `ReminderDefaults`
  equivalent is carried by the snapshot: `reminder_before` +
  enabled; `models.rs:672-1001` take the snapshot);
- focus = `state.settings.focus` / `focus_chain`.

The hand-over sites (`main.rs:797`, `ui.rs:647`) shrink to passing
the boot offset; the day-start/due-soon values arrive through the
engine-seeded state (the app seeds the engine from the same config,
so the first published state carries them — the acceptance's
"day-start/due-soon derivations on the next refresh"). UI-crate
journeys that hand-build `AppState` gain the `settings` field on
their fixtures (mechanical; `Default` on `SettingsSnapshot` gives the
test-policy values).

### D-5.8 — `AppState::settings` and the snapshot suites

- Field: `pub settings: taskboard_domain::SettingsSnapshot`, on
  `AppState` (Serialize-only as today). Memory-only: **not** in
  `PersistedState` — the file overlay is the store and boot re-seeds
  from figment (the origin's "settings persist via the app-lane file
  overlay, not sqlite").
- The insta suites are **projection-based**
  (`tests/snapshots.rs:178-191`): `PublishedTail` names
  `sync`/`undo`/`last_updated` explicitly, so the 78 `.snap` files do
  not churn. The `memory ≡ disk` equality asserts are per-subtree and
  untouched. A new focused engine test (not a snapshot) pins the
  apply: seed → `UpdateSettings` → ArcSwap read shows the new
  snapshot; a second `UpdateSettings` with a different snapshot
  replaces it wholesale (no field-wise merge — the snapshot is a
  value, not a patch; per-field commits assemble the full snapshot
  app-side from the fresh file decode, so a partial-update command
  variant is never needed and is deliberately absent).

---

## 4. Implementation order (TDD)

Each numbered step is at least one standalone commit
(`<type>(<module>): …`); tests land with or before the code they pin.
Domain first, then the engine, then the app lane, then consumers one
at a time, then the UI.

1. **`feat(domain): the settings snapshot and its minting newtypes`**
   — `settings.rs` with `SettingsSnapshot`, `TickSecs`,
   `DueSoonDays`, `ReminderSinkChoice`; property/`proptest` over the
   newtype mint/round-trip (random `u64` → mint → the documented
   boundary), unit tests for the lenient serde re-mint, the
   `ReminderOffset`/`FocusTiming` reuse (their tests exist; add only
   the snapshot-level wiring tests). `test` module in-file; SPDX MIT
   OR Apache-2.0.
2. **`feat(domain): the UpdateSettings command and the settings-changed event`**
   — `StateCommand::UpdateSettings`, `CommandOutcome::SettingsApplied`,
   `SystemEvent::SettingsChanged`, `SyncStateReader::read_settings`;
   compile-only evidence plus the wildcard-match updates land with
   the consumer steps (the domain commit keeps the workspace
   compiling: the new match arms are added in the same commit where
   the compiler demands them — one commit per crate is acceptable
   here, each standalone).
3. **`feat(state): the engine applies and announces settings`** —
   `EngineCore.settings` + `app()` overlay + `spawn_state_engine`
   param; the `execute_command` special arm (apply → publish →
   system send → receipt); tick/day/defaults re-reads and the
   `next_tick` re-arm; engine tests: the apply journey (ArcSwap
   assertion), the announcement journey (subscribe the system bus,
   paused runtime, assert `SettingsChanged` arrives), the tick
   re-arm journey (paused clock: two ticks at the old cadence,
   apply, the next wake lands within the new cadence), the
   day-calendar journey (watermark non-re-roll), the disabled-tick
   precondition journey.
4. **`feat(app): seed the engine's settings at boot`** —
   `AppConfig::settings_snapshot()`; `spawn_state_engine` call sites
   (production + test helpers); unit tests: the mint over a
   hand-built validated config equals the expected snapshot
   (typed-field asserts, no text assertions).
5. **`feat(app): the persist-then-command write path`** — env-lock
   guard + `SettingsError::EnvLocked`; mint-and-command after a
   proven write; the reply-from-ArcSwap; the failure arms
   (`OverlayError` → no command; `EngineGone` → `Gone`). Tests:
   the app-lane harness (existing `ui_requests` test kit) — a
   successful commit's reply echoes the engine state (mutate the
   engine state independently and verify the reply differs from the
   file echo → proves the engine is the source), the failed-write
   journey (prove-gate refusal → engine state unchanged, observed
   through the ArcSwap), the env-locked journey (env var set →
   refused typed → file byte-identical), the engine-gone journey.
6. **`feat(app): live escalation and focus reads`** — escalation
   signature shrink + per-iteration reads; focus runtime per-iteration
   reads. Tests: `awaiting_alarms` over a state whose settings
   changed (defaults boundary moves), the focus park-on-disable
   journey, the timing-change-at-next-boundary journey (paused
   clock). No `tokio::time::sleep` anywhere — `pause()`/`advance()`
   per AGENTS §5.
7. **`feat(sync-nextcloud): the cadence re-read and the doorbell re-arm`**
   — `read_settings` per cycle, the min-rule arm, `reschedule` on
   live values. Tests (the hermetic pair harness): the poll re-arm
   journey — old interval armed, `SettingsChanged` with a shorter
   interval, advance, the cycle fires within the new interval
   (paused clock); the min-rule journey (a lengthened interval does
   not postpone an imminent deadline); the backoff journey (new
   backoff values after a failure).
8. **`feat(ui-slint): the derivations read the published settings`**
   — adapter field removals, `boot_offset`, the per-refresh mints;
   journeys: day-start change visible at the next refresh (fake
   state, no fixed clock — content asserts are structural: the day
   window boundary moves), due-soon horizon change visible, reminder
   defaults change visible in the notification rows model.
9. **`feat(app): the apply-semantics registry`** —
   `settings_apply_registry` + `ApplySemantics` (lane mirror), the
   wired `applies` map, the two enforcement tests (justification
   present on every `OnRestart`; registry covers exactly the editor
   field lattice). The badge flip rides the existing rendering —
   a `taskboard-ui-slint` journey asserts a live key renders without
   the restart badge and a justified key renders with it
   (structural: the `on-restart` property value, not label text).
10. **`docs: the chunk plan's decision ledger cross-links`** (if any
    doc anchors changed — `docs/testing_strategy.org` §6.1's clock
    rules already cover the paused-clock journeys; no doc change is
    expected).

Gates before handoff: the full AGENTS §10 suite through
`scripts/run-quality-gates.sh` (the shared-lock wrapper — parallel
agents on this box).

---

## 5. The journey catalogue per setting (acceptance matrix)

| Editable key | Live consumer | Journey (layer, clock) |
|---|---|---|
| `reminders.tick_secs` | engine tick arm | engine, paused: apply mid-cadence → next wake ≤ new tick |
| `reminders.enabled` | engine tick arm | engine, paused: disable → advancing past several old ticks fires nothing (day roll still rolls) |
| `reminders.default_before` | engine planner defaults | engine: apply → create task without explicit reminder → its planned reminder carries the new offset |
| `reminders.sink` | — (justified OnRestart) | app: registry test pins the justification; no live journey |
| `sync.poll_interval_secs` | sync actor cycle | sync hermetic pair, paused: doorbell re-arm within one new cycle |
| `sync.backoff_initial_secs` / `_max_secs` | sync actor reschedule | sync hermetic pair, paused: failure → next backoff from the new values |
| `preferences.day_start_time` | engine day roll + UI derive | engine (watermark non-re-roll) + UI (next-refresh window move) |
| `ui.due_soon_days` | UI derive | ui-slint: next refresh changes the soon set's boundary |
| `focus.enabled` | focus runtime inert-park (spawned case) | app focus, paused: disable → no transitions; re-enable resumes; boot-disabled case pinned OnRestart by the registry test |
| `focus.work_mins` / `break_mins` / `long_break_mins` / `long_break_every` | focus runtime timing | app focus, paused: apply → the next boundary lands at the new duration |
| `focus.chain` | focus runtime | app focus: the next break end carries the new chain decision |
| `focus.notify` | — (justified OnRestart) | app: registry test pins the justification |
| every key, env-locked variant | app write path | app: env var set → typed refusal → file and engine byte-identical |

---

## 6. Risks

- **The `spawn_state_engine` signature change fans out** (~28 call
  sites, mostly test helpers): mechanical; each test helper gains one
  explicit `SettingsSnapshot` argument (the disabled-test-policy
  constructor keeps the diff honest — no `..Default::default()`
  sprawl in fixtures).
- **A missed doorbell delays a consumer one cycle** (broadcast
  `Lagged`): accepted and documented (D-5.4); consumers re-read
  values per iteration, so the cost is one stale-cadence cycle, never
  a lost value.
- **The adapter field-removal churn** (~20 read sites + journeys
  hand-building `AppState`): the largest mechanical surface; the
  per-refresh mint must not regress the frozen-clock golden journeys
  (`UiSettings::clock` freezes *content* — the settings-derived
  content is not clock-derived except through the day window, which
  the goldens already pin; any golden diff is a finding to triage,
  not an accepted change).
- **`focus.enabled`'s asymmetric liveness** (disable live, enable at
  restart) could read as a half-applied setting: the registry's
  justification names it; the slint row shows the restart badge for
  that key while the runtime honors live disables — the honest
  wording lives in the justification, not in a fake badge.
- **Day-start backwards move**: the watermark's non-re-roll is
  intended semantics (pinned by a test in step 3), not debt — a
  future re-roll-on-change would be a deliberate behavior change.
- **Files-sync cadence stays boot-captured**: recorded limitation
  with a `backlog:` follow-up; not silently dropped.
- **Snapshot not in `PublishedTail`**: if a future suite wants
  settings in the projection, that is an additive tail change —
  recorded here so stage 3 does not mistake the absence for an
  omission.

## 7. Acceptance (dispatch §6, restated)

- Each editable setting is observable in its consuming actor without
  restart, via the §5 journeys (paused clock; poll re-arm within one
  cycle; tick at the next iteration; UI derivations at the next
  refresh).
- Env-locked settings stay locked (app-side typed refusal; the
  hand-over badge logic untouched).
- The file overlay still prove-and-writes; a failed write sends no
  command (engine state observed unchanged through the ArcSwap).
- The registry test fails an unjustified `OnRestart` entry and a
  registry/editor lattice drift.
- No new `Mutex`/`RwLock` cross-actor state (AGENTS §3): settings
  travel as a snapshot value through the command channel and are read
  via the existing ArcSwap — no new lock class anywhere.
- Gates green (`cargo fmt --check`, clippy `-D warnings`, nextest,
  doc tests, deny, the CI-identical coverage triple) via
  `scripts/run-quality-gates.sh`.

## 8. Estimated diff size

~1,700–2,000 lines across six crates: domain ~+300 (new module +
messages), state ~+150/−60, app ~+450/−120 (registry, minting, write
path, escalation/focus signatures), sync-nextcloud ~+80/−30, ui-slint
~+250/−150 (adapter re-reads, lane mirror, journeys), plus ~+450 test
lines spread through the above. No schema, no lockfile, no doc changes
expected.
