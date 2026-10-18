# [Plan] Reminders Surfaces — Design Round (N-rule verbs, adhoc alarms, toast sink)

- **Date**: 2026-10-19
- **Kind**: design-round doc (daily-driver wave framing §5 Q3, agent C —
  chunk 3 of the round's five docs). Design only — no production code in
  this commit. Feeds the wave implementation plan's chunks 6/7/8
  (framing §6) after the user's decision session rules §4's
  recommendations.
- **Origin**: framing plan
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md` §2/§5.3;
  issues **#204** (N-reminder surfaces), **#211** (standalone timers &
  alarms), **#344** (toast sink); the reminders design round
  `.artifacts/plans/2026-10-12-reminders-design-plan.md` (its deferred
  "plan 5" is this doc's Part C; §4.4 D-a…D-e and §13 D1–D5 remain
  binding) and `2026-10-13-reminders-domain-model-plan.md` (the shipped
  model this round widens); future-directions report §3.2 (the wave-1
  reminder items); v0.1.0 audit **F16-20** (the surface-friction finding
  this round resolves) / **F16-21** (pipeline reusable as-designed).
- **Chunk-1 dependency**: wherever the reminder-rules sidecar section
  matters (rule identity, clocks, tombstones, the `UpdateAppData`
  enqueue) this doc **cites**
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` §3.9
  (D-S9, ACCEPTED 2026-10-18) rather than re-deriving it. Chunk 2
  (field family + preferences) is untouched except where named.
- **Branch / worktree**: `docs/daily-driver-design-round` in
  `../taskboard-WT-docs-daily-driver-design-round`, sequential chunk 3
  (chunks 1–2 already on the branch: `2b1a657`, `a8edfdb`+`f236c0b`).
- **Status**: PROPOSED — one recommendation per question (§4). **D-R10
  (the toast dependency) is a stack-approval item for the user's
  decision session** (AGENTS §4; framing D-D6 names it explicitly).

---

## 1. Context & verified anchors (do not re-derive; verified on this branch 2026-10-19)

Code anchors re-verified at doc start per the round's rule (symbols are
the stable keys; line numbers are this branch's):

| Fact | Anchor | Evidence |
|---|---|---|
| `ReminderRef` is a two-arm enum | domain `reminders.rs` | :72–77 `Rule(ReminderRuleId) \| Default(TaskId)` |
| One text-form definition site | domain `reminders.rs` | :79–105 `text_form`/`parse_text_form` (`"rule:<uuid>"`/`"default:<task>"`); the snooze-key serde (:148–185) and the storage codec (storage `codec.rs:250–264`) both delegate |
| N rules per task already modeled | domain `reminders.rs` | :107–120 `ReminderRule { id, task, offset, created_at }`; `ReminderState.rules: BTreeMap<ReminderRuleId, ReminderRule>` (:213); `created_at` doc (:117–119) reserves the additive write clock the sidecar consumes |
| Occurrence-keyed suppression | domain `reminders.rs` | :122–134 `ReminderDelivery` keyed `(reminder, occurrence)`; :136–146 `ReminderSnooze`; chain walk :319–346; both-endpoints compaction :288–308 |
| Evaluator is N-capable, mute-aware, rules-replace-default | domain `reminders.rs` | :366–413 `due_firings` (per-task candidates = its rules, else the implicit default); :447–449 `is_remindable` |
| **The verb surface is replace-with-one** (F16-20) | domain `command.rs` | :889–909 `SetReminderRule` deletes all the task's rules then upserts one fresh rule; :910–921 `ClearReminderRules` (already N-capable, NoOp on empty) |
| Snooze/mute commands exist and validate refs | domain `command.rs` | :838–873 `SnoozeReminder` (`Rule`→rule lookup `UnknownReminderRule` :87/:851, `Default`→task; `postpone > 0` typed check :857); :874–888 mute |
| Board-content clear consults reminder refs | domain `command.rs` | :795–806 `ClearBoardContent`'s `board_reminders` match arms (`Default(t)`, `Rule(_)`) — every exhaustive `ReminderRef` match is compiler-forced to grow with a new arm |
| The delivery port | app `notifications.rs` | :26–41 `NotificationSink::deliver → Pin<Box<dyn Future<Output = Result<(), NotifyError>>>>`; :43–64 `LogSink` v1; :66–88 `spawn_notifier` (failure = log-and-continue; capacity 64 :17) |
| Notification payload is task-shaped | domain `reminders.rs` | :253–264 `Notification { reminder, task: TaskId, title, occurrence }`; :266–286 `notifications_for` (title = task-title snapshot); firing triple :197–205 (`ReminderFiring.task`) |
| Durable-then-notify tick | state `actor.rs` | :797–828 `reminder_tick` (delivery markers are the notification permit); tick arm :350–357/:476–486; `try_send` backpressure :850+ |
| CLI: the replace verb family | app `commands/reminders.rs`, `cli.rs` | `set_on` executes `SetReminderRule` (:124–130); `RemindInvocation` flag-collapse (:307–364); clap group `cli.rs:426–449` (`--list/--clear/--off/--on/--before`, one mode per line); two-lane REPL/one-shot shape (:1–7 module doc) |
| Config | app `config.rs` | :212–229 `RemindersSection { enabled, tick_secs, default_before }` (defaults :54–58: true/30s/5m); `AppMode { Daemon, Desktop, Kiosk }` :84–95; strict tables (`deny_unknown_fields`) |
| UI already has interactive reminder actions | ui-slint | `presentation.rs:173–231` `ReminderTarget`/`BannerRow`/`snooze_target` (first-rule-or-default); `actions.rs:112/344–347` `ActionId::ReminderSnooze`/`ReminderMute`; `router.rs:130–140` dispatch → `SnoozeReminder`/`SetTaskRemindersMuted` |
| Storage/migrations | storage | migrations 0001–0005 present; wave numbering starts at 0007 (sidecar doc §1: multi-board's 0006 is in flight) |
| Deps discipline | root `Cargo.toml` | `[workspace.dependencies]` pinning (AGENTS §4); edition 2024, `rust-version = "1.96"` |

Out-of-scope fences that stay binding (framing §2): reminder
**delivery-proofs sync** (the proofs half of #202 → D5 substrate, wave 4
— D-S9's scope fence keeps `deliveries`/`snoozes`/`muted` device-local);
**insistence levels** (#203 lands *after* this wave's sink; sounds and
escalation ride it); recurrence (wave 3); repeat/nag policy (reminders
design D1/D4).

## 2. Scope & non-goals

**In scope (design only):** the N-rule management vocabulary and its
CLI/REPL/UI surfaces (Part A); the `Adhoc` anchor kind, `AdhocAlarm`
entity, lifecycle, persistence, and CLI-first timer/alarm verbs (Part
B); the desktop-notification sink — dependency recommendation
(stack-approval matrix), port contract, config, kiosk degradation (Part
C); decision record §4; test hooks; chunk mapping.

**Non-goals:** any production code; the sidecar rules-section encoding,
fold/flush mechanics, and rule-clock migration column (chunk 1's doc
owns them — cited, never re-derived); sounds, escalating alarms, and
OS-level insistence (#203); the reminder-center overlay UI (deferred
with #203, D-R12); alarm/toast sync across devices (D5 substrate);
recurrence; `RemindersSection` tick/default changes.

---

## 3. Design

### Part A — The N-reminder surface (#204, F16-20)

#### 3.1 D-R1 — Command vocabulary: add/remove replace replace-with-one

The domain keeps its N-capable model untouched; only the *verb* layer
changes. `StateCommand::SetReminderRule` (replace-with-one,
`command.rs:889–909`) is **retired**, not aliased:

- **`AddReminderRule { task: TaskId, offset: ReminderOffset }`** —
  planner mirrors today's edit-style intent checks (`live_task`), then:
  - **Dedup by offset**: if the task already has a rule with the same
    offset, plan **no actions** (`CommandOutcome::NoOp`-shaped receipt,
    see below) naming the surviving rule — `(task, offset)` fully
    determines firing semantics (`occurrence_of` is pure in
    `(offset, duedate)`, reminders.rs:310–317), and two rules with one
    offset are observably just a duplicate toast pair. Dedup is what
    makes the offset a sound rule selector for removal (D-R3).
  - Otherwise mint a fresh rule (`ids.new_reminder_rule_id()`, the
    `created_at` stamp, `UpsertReminderRule`) exactly as today's set
    path does.
  - Outcome: `CommandOutcome::ReminderRuleAdded { rule: ReminderRuleId, added: bool }` — `added: false` ⇔ the dedup hit; the CLI renders
    "already set" from the flag without diffing state.
- **`RemoveReminderRule { rule: ReminderRuleId }`** — planner rejects
  unknown ids with the existing typed `CommandError::UnknownReminderRule`
  (command.rs:87); emits `PersistenceAction::DeleteReminderRule`, whose
  cascade semantics (prune the rule's deliveries and snooze links) are
  already defined once in `apply_actions` (persistence.rs:282–289).
  Outcome `CommandOutcome::ReminderRuleRemoved { rule }`.
- **`ClearReminderRules { task }` stays** — remove-all is already
  N-capable and NoOp-on-empty (command.rs:910–921); it becomes "remove
  N" instead of "remove the 1".
- `SnoozeReminder`, `SetTaskRemindersMuted` unchanged.

**Why retire rather than reinterpret silently or keep both:** the verb
family is five weeks old in a pre-1.0 single-user product; a
`SetReminderRule` that *means* "add" under a "set" name is a lying
name, and keeping both invites exactly the accidental-clobber the issue
files against. The first-remind flow (task has no rules) is
behaviorally identical under add semantics, so the only behavior change
is the one #204 asks for: reminding a task that already has rules adds
instead of clobbering. Changelog entry flags the semantic change.

#### 3.2 D-R2 — Rule mutations are the sidecar enqueue sites (cite, don't re-derive)

Per sidecar **D-S9** (`2026-10-18-appdata-sidecar-design-plan.md` §3.9,
ACCEPTED): rule mutations stamp the rule's additive write clock, deletes
are document tombstones, and "doc 3's remove verb" emits one tombstone
per removed rule. This doc fixes the enumeration those statements hang
on — **the rule-mutation commands are `AddReminderRule` (stamp on
create), `RemoveReminderRule` (tombstone), `ClearReminderRules` (N
tombstones)** — and nothing else. `UpdateAppData(TaskId)` enqueueing is
D-S5's flush machinery landed by the sidecar implementation chunk; no
outbox/native-path change is designed here (the v1 fence stands, D-S9's
restatement).

#### 3.3 D-R3 — CLI/REPL surface

Keep the flag-coherence group (one mode per line, `cli.rs:426–449`);
grow one mode:

```
taskboard tasks remind <task> [--before <dur>]     # ADD (was replace-with-one); config default_before still applies
taskboard tasks remind <task> --remove <rule>      # NEW: remove one rule
taskboard tasks remind --list                       # unchanged — already renders N rows per task (commands/reminders.rs:216–241)
taskboard tasks remind --clear <task>               # unchanged (now removes N)
taskboard tasks remind --off/--on <task>            # unchanged
```

- **Rule selector** for `--remove`: (a) exact rule UUID, (b) unique ≥8
  char UUID prefix (the task-selector cascade precedent), or (c) the
  offset text form (`30s|5m|1h30m|2d` — the existing `parse_before`
  grammar, `cli.rs:435`) resolved against the task's rules. (c) is
  unambiguous *because* of D-R1's dedup: at most one rule per
  (task, offset). Selector resolution failure is a typed
  `CommandError` (the `UnknownTask` shape's sibling), never text
  matching in the engine.
- **Receipts**: `ReminderSetReceipt` → `ReminderAddReceipt { rule_id,
  task_id, task_title, before, added }`; new `ReminderRemoveReceipt`.
  `--json` follows the kebab-case envelope contract (ADR 0009); typed
  outcomes only (AGENTS §5).
- **REPL**: the two-lane shape is the point of the `*_on` seams
  (commands/reminders.rs:1–7) — `RemindInvocation` grows a
  `Remove { task, rule }` variant; `collapse` and both dispatch lanes
  extend mechanically.

#### 3.4 D-R4 — UI growth (thin-client discipline)

- **Task detail/editor**: a rule-list composition (one row per rule:
  `format_before` offset, created stamp, remove affordance) plus an
  "add reminder" affordance (offset presets + custom `--before`
  grammar). Rows are a `compositions/` component rendering
  `AppState.reminders` filtered by task — **no new state, no UI-side
  logic**: add/remove dispatch `AddReminderRule`/`RemoveReminderRule`
  through the existing action layer (the 71-`ActionId` surface,
  framing §3). R1 component discipline: this extends the task editor,
  not a new view family.
- **Banner unchanged**: `snooze_target` (presentation.rs:221–231)
  keeps first-rule-or-default as the banner's single snooze target —
  the banner is board truth with one well-formed target per task (D12);
  per-firing snooze over N rules belongs to the reminder center
  (deferred, D-R12).
- **N-rules UX stance**: no cap on rules per task (the model is a map);
  the list stays compact because offsets render short; "explicit rules
  replace the implicit default" is already derived (reminders.rs:392–397)
  and needs no extra UI state — the editor shows the rules that exist
  and a hint that no default fires while any rule does.

### Part B — Standalone timers & alarms (#211)

#### 3.5 D-R5 — The `Adhoc` anchor and the `AdhocAlarm` entity

- `ReminderRef` grows a third arm (F16-20's ripple, made concrete in
  D-R6): `Adhoc(AdhocAlarmId)`, declared **after** `Default` (Ord at
  equal instants is arbitrary but total; chronological-variant order
  keeps diffs readable). `AdhocAlarmId` via the `local_id!` registry
  (ids.rs) — portable UUIDv7 like every entity id.
- The entity (domain `reminders.rs`):

```rust
/// A task-independent alarm: tea timers, meditation minimums, wake-up
/// alarms. Fires once at `fire_at` (plus snooze-chain movement); cancel
/// is the off-switch (no mute — mute is task-shaped).
pub struct AdhocAlarm {
    pub id: AdhocAlarmId,
    /// The notification title (no task to snapshot a title from).
    pub label: String,
    /// Absolute fire instant; the planner stamps `In(dur)` as `now + dur`.
    pub fire_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
```

- `ReminderState` grows `pub alarms: BTreeMap<AdhocAlarmId, AdhocAlarm>`
  (after `muted`; snapshot order). Snapshot-breaking for insta — one
  mechanical regeneration pass owned by the implementing chunk (the
  DF-9 rule chunk 1 cites: the widening chunk owns its one deliberate
  pass). Insistence (#203) later adds a field here — reserved, not
  built (the `urgency` reservation pattern, reminders design §6.1).
- **Absolute-vs-duration is a CLI concern, not a domain concern**:
  `CreateAdhocAlarm` carries `when: AdhocWhen { At(DateTime<Utc>), In(chrono::Duration) }`; the planner stamps `In` as `now + dur` (the
  `SnoozeReminder` precedent, command.rs:862–864; time is data, the
  engine's clock read is the one stamp). `In(dur ≤ 0)` is a typed
  rejection (the `postpone > 0` check's sibling).

#### 3.6 D-R6 — Evaluation integration: the F16-20 ripple, site by site

The pipeline is reusable as-designed (F16-21); the named sites one new
enum variant forces (audit F16-20: "ref text form, snooze keys, storage
codec, payloads; ≥4 sites"):

1. **Text form**: `ReminderRef::text_form`/`parse_text_form` gain
   `"alarm:<uuid>"` (reminders.rs:79–105). One definition site — the
   snooze-key serde (:148–185) and the storage codec
   (codec.rs:250–264) inherit it unchanged.
2. **Base occurrence**: `base_occurrence` arm `Adhoc(id) =>
   reminders.alarms.get(&id)?.fire_at` (:348–364). The snooze walk,
   cycle bound, and delivery keying are untouched — snoozing a tea
   timer reschedules its occurrence exactly like a task reminder's.
3. **`due_firings` gains an alarms pass**: an alarm participates iff
   present in `alarms` (map membership is its liveness — no
   done/tombstone states) and fires iff `effective ≤ now ∧ undelivered`.
   Mute does not apply (`muted: BTreeSet<TaskId>` is task-shaped;
   cancel is the alarm's off-switch). Alarms merge into the same
   `(occurrence, reminder)` sort — output stays fully deterministic.
4. **Planner arms**: `SnoozeReminder`'s ref match grows `Adhoc(id)` →
   alarm lookup, typed `CommandError::UnknownAdhocAlarm` (new variant;
   the `UnknownReminderRule` shape), with **no** `live_task` check (not
   a task). `ClearBoardContent`'s `board_reminders` arms
   (command.rs:795–806) grow a deliberate `Adhoc(_) => false` — alarms
   are device-global, not board content.
5. **Payload anchor**: `ReminderFiring.task: TaskId` and
   `Notification.task: TaskId` become `anchor: ReminderAnchor` with
   `Task(TaskId) | Alarm(AdhocAlarmId)`. A task id is meaningless for
   an alarm, and the UI needs the *typed* anchor for navigation; the
   `Alarm` arm also lets consumers match without re-matching the ref.
   `notifications_for` (:266–286) reads the title from the task map
   **or the alarm's label** (signature grows the alarms map / takes
   `&ReminderState`). Port-visible surface: only the payload widening —
   the `NotificationSink` signature does not move (D-R11).
6. **UI**: `ReminderTarget` (presentation.rs:175–181) widens to the
   anchor enum; banner rows are task-only so `snooze_target` is
   mechanically unaffected (its `Default(task)` fallback keeps a
   `TaskId`). No alarm UI lands in-wave (D-R9).
7. **Exhaustiveness**: every other `ReminderRef` match (the engine's
   tick correlation, storage roundtrips, proptest expected-set
   re-derivations) is compiler-forced; each gets a deliberate arm, none
   changes semantics.

#### 3.7 D-R7 — Lifecycle: create/cancel, death by settlement, bounded growth

- **Commands**: `CreateAdhocAlarm { label, when: AdhocWhen }` (outcome
  `AdhocAlarmCreated { alarm: AdhocAlarmId }`) and
  `CancelAdhocAlarm { id }` (typed unknown-id rejection; outcome
  `AdhocAlarmCancelled { alarm }`). Both plan
  `UpsertAdhocAlarm`/`DeleteAdhocAlarm` (D-R8).
- **Death by settlement**: an alarm whose effective occurrence is
  delivered and whose snooze chain is fully settled is dead — it can
  never fire again (the occurrence key is immutable) and carries no
  truth worth keeping. The `apply_actions` post-batch pass (the
  `compact_snooze_chains` site, reminders.rs:288–308) grows, **ordered
  after** snooze compaction: prune `AdhocAlarm`s whose effective
  occurrence is delivered. The composition is exact: once a snooze
  target settles, both chain endpoints are delivered → the link prunes
  → effective reverts to the base → base is delivered → the alarm
  prunes. Pruning an alarm removes its `Adhoc`-ref deliveries and
  snooze links with it (the `DeleteReminderRule` cascade shape,
  persistence.rs:282–289) — nothing can re-fire from a pruned map row.
  Unfired future alarms live until fired or cancelled.
- **No background GC, no retention knob** (D-c precedent: bounded
  growth by construction). A snoozed-but-never-settling alarm lives as
  long as the user keeps snoozing it — correct, not leaky.
- **Property pin** (implementation chunk): `due_firings` is invariant
  under the prune pass on arbitrary states (the compaction-
  unobservability property's sibling, domain-model plan §9.6).

#### 3.8 D-R8 — Persistence & storage

- `PersistenceAction::{UpsertAdhocAlarm(AdhocAlarm), DeleteAdhocAlarm(AdhocAlarmId)}`
  — semantics defined once in `apply_actions` (upsert-by-id; delete
  prunes dependents; the D-R7 prune runs in the shared post-pass), so
  the sqlite repo and every fake inherit them (ADR 0006 parity).
- **Migration**: an `adhoc_alarms` table (`alarm_id TEXT PRIMARY KEY,
  label TEXT NOT NULL, fire_at TEXT NOT NULL, created_at TEXT NOT
  NULL`) rides the wave's additive migration slot (0007+, sidecar doc
  §1's numbering; the rule-clock column D-S9 reserves rides the same
  wave migration — one migration, both concerns, landed by whichever
  chunk lands first per the implementation plan). Deliveries/snoozes
  tables ride unchanged: `alarm:<uuid>` is just another `reminder`
  text value through the existing codec (codec.rs:250–264).
- **Device-local**: alarms join D-S9's device-local set (proofs/
  snoozes/muted) — no outbox op, no sidecar section. The sidecar
  schema preserves unknown sections verbatim (D-S2), so a future
  `adhoc-alarms` section for the D5 substrate is non-breaking; this
  round only notes the name as a D5-round candidate. Nothing lands.

#### 3.9 D-R9 — CLI-first surface (`taskboard timers`)

```
taskboard timers add <label> (--in <dur> | --at <datetime>)   # e.g. timers add Tea --in 3m / timers add Standup --at 09:30
taskboard timers list      # pending (with remaining) + fired-not-yet-pruned (overdue marker); canonical: fire_at then id
taskboard timers cancel <selector>                            # selector: alarm UUID | unique ≥8 prefix | unique label prefix
```

- `--at` parses host-local wall-clock to UTC at the CLI edge (chrono
  local parsing); timezone override is a preferences question that is
  deliberately not designed here (chunk 2's `UserPreferences` home
  owns time-adjacent knobs; a `timers` tz override can ride it later).
- New `commands/timers.rs` in the established two-lane shape
  (`add_on`/`list_on`/`cancel_on` session seams + one-shot wrappers);
  `cli.rs` grows a `TimersCmd`; REPL `dispatch_line` mirrors. Receipts
  and `--json` rows follow the envelope contract; typed outcomes only.
- **Precision stance**: `tick_secs` (default 30s) bounds firing
  lateness — a 3-minute tea timer fires within one tick of target.
  Documented, not re-architected; the knob already exists
  (config.rs:222–224). Sub-second timers are out of scope (not a
  stopwatch).
- **No UI surface in-wave** (#211: "CLI-first; GUI/overlay surfaces
  later"). Kiosk shows nothing new: `overdue_tasks`/banner are
  task-only by contract (reminders design §7) and stay so.

### Part C — The toast sink (#344, the reminders "plan 5")

#### 3.10 D-R10 — Dependency recommendation (STACK APPROVAL REQUIRED)

A new crate family enters `taskboard-app` — AGENTS §4 stack approval;
framing D-D6 lists this as a decision-session item. The matrix (state
of the ecosystem verified 2026-10-19):

| | **notify-rust 4.18.x** (recommended) | Hand-assembled: `zbus` + `mac-notification-sys` + `tauri-winrt-notification` | `ashhd`/XDG portal route |
|---|---|---|---|
| Linux | `zbus ^5` **pure-Rust default** (no libdbus C dep; cross-compiles to the ARM kiosk targets; C libdbus optional via `d`) | `zbus` directly — we hand-roll the `org.freedesktop.Notifications` client | portal API only |
| macOS | `mac-notification-sys ^0.6.15` | same crate, direct | n/a |
| Windows | `tauri-winrt-notification ^0.8` (maintained fork used by Tauri) | same crate, direct | n/a |
| tokio fit | `z-with-tokio` feature = zbus executor on tokio; default features work too (zbus's internal executor thread + awaitable futures) | we own the integration | async portals |
| License | **MIT OR Apache-2.0** (matches the library dual tier; transitive tree permissive: zbus MIT, windows MIT/Apache-2.0) | same tree, three approvals | MIT |
| Maintenance | active through 2026 (4.18.2; 15M+ downloads; MSRV 1.89 ≤ workspace 1.96) | three upstreams to track | portal-spec churn |
| What we write | `DesktopSink`: payload → `Notification::new().summary/title/body.show()` | the same mapping **plus** our own DBus client, capability handling, and three build-target wroutines | restricted API (fixed button protocol, no hints passthrough) |

**Recommendation (the one): `notify-rust` 4.18.x, default features on
Linux (`z`), no image feature, pinned once in
`[workspace.dependencies]`** (AGENTS §4). The façade's entire job is
the three-backend mapping our `DesktopSink` needs; the hand-assembled
stack buys identical transitive deps plus code we would own; the portal
route trades the richest API surface for sandbox-friendliness we do not
need yet (no Flatpak story in-wave) while covering no non-Linux target.
Evaluate `z-with-tokio` vs default at implementation (both work from
tokio; the feature removes zbus's internal executor thread — a
reasonable kiosk-memory nicety, decided by measurement, not doctrine).
`cargo deny check` gates the new license tree per target (AGENTS §10).

#### 3.11 D-R11 — What stays behind the port

The port does not move: `NotificationSink::deliver(&Notification) ->
Result<(), NotifyError>` (notifications.rs:26–41) and the
`spawn_notifier` consumer (log-and-continue, :66–88) are the contract;
`DesktopSink` is a new implementor in `taskboard-app` (the OS-boundary
port precedent — `CredentialStore`; app crate is AGPL, consistent).
Behind it, and *only* behind it: DBus session-bus connection and daemon
discovery, per-platform capability quirks, timeouts, the app identity
(summary/app-name "taskboard"), and — when #203 lands — urgency hints,
sound, and escalation loops. Payload mapping: title =
`Notification.title` (task-title snapshot or alarm label, D-R6.5), body
= human rendering of the occurrence ("due now" / the fire instant),
anchor carried in the payload for the future click-through. `LogSink`
stays exactly as is (daemon trail, test sink, fallback story).

#### 3.12 D-R12 — Passive toasts; interaction stays in-app (evidence-backed)

**V1 toasts carry no action buttons.** GNOME Shell's notification
server — the primary Linux DE — does not implement the `actions` field
of the XDG notification spec (buttons are silently dropped; Chromium
ships a capability probe and fallback for exactly this). Windows toast
buttons need COM activation plumbing; macOS is workable but awkward
through the façade. Toast-embedded snooze would therefore work on some
desktops and silently not on the most common one — unacceptable for the
primary interaction of a reminder app.

The interactive surface is taskboard's own and already exists in seed
form: the banner dispatches `SnoozeReminder`/`SetTaskRemindersMuted`
(actions.rs:344–347, router.rs:130–140). This honors reminders-design
D-b's "the interactive snooze/dismiss surface ships with the desktop
notification plan" by shipping it **in-app**: cross-platform,
deterministic, thin-client-legal. A fuller **reminder center** (the
fired-notification list with per-firing snooze/mute/cancel, the
alarm-window escalation surface) is deferred to #203's insistence
round, where overlay UX belongs; its data — deliveries, snooze chains,
anchors — is all present after Parts A/B.

#### 3.13 D-R13 — Config & kiosk degradation

`[reminders]` grows `sink = "auto" | "log" | "desktop"` (default
`"auto"`), strict-table + typed decode error (the `invalid-duration`
precedent for a typed config failure, config.rs:212–229):

- **`auto` resolves by `AppMode`** (config.rs:84–95): `Desktop` →
  `DesktopSink`; `Daemon`/`Kiosk` → `LogSink`. Rationale: the daemon
  keeps today's journald trail; the kiosk's surface is the
  scheduler-free banner (reminders design §3/§7) and a kiosk session
  cannot be assumed to run a notification daemon at all; the desktop
  gets toasts day one with zero configuration.
- **Degradation is never fatal**: a missing/failed daemon surfaces as
  the port's `NotifyError`, logged by the consumer
  (notifications.rs:76–87) — one lost toast must not kill the actor,
  and there is **no retry and no fallback-to-log-on-error** (a
  fallback after an ambiguous failure risks double delivery; the warn
  line is the trail; the user's remedy is snooze). `LogSink` remains
  selectable explicitly for desktop users who prefer the journal.
- The sink choice is boot-time config, not runtime-switchable (the
  consumer holds one `Arc<dyn NotificationSink>` from bootstrap; no
  live-swap seam is designed).

---

## 4. Decision summary (one recommendation each — for the user's session)

| ID | Question | Recommendation | Status |
|---|---|---|---|
| D-R1 | N-rule verb semantics | Retire `SetReminderRule`; `AddReminderRule` (dedup-by-offset, names surviving rule) + `RemoveReminderRule` (by id) + keep `ClearReminderRules` | PROPOSED |
| D-R3 | CLI shape | Bare `remind <task> --before` becomes add; `--remove <task> <rule>` with id/prefix/offset selectors; list/clear/off/on unchanged | PROPOSED |
| D-R4 | N-rules UX | Task-editor rule-list composition + add affordance; banner target unchanged; no cap | PROPOSED |
| D-R5 | Adhoc anchor | `ReminderRef::Adhoc(AdhocAlarmId)` + `AdhocAlarm { id, label, fire_at, created_at }` in `ReminderState.alarms`; absolute `fire_at`, planner stamps durations | PROPOSED |
| D-R7 | Alarm lifecycle | Create/cancel commands; settlement-prune in the post-batch pass after snooze compaction; no GC/retention knob | PROPOSED |
| D-R9 | Timers surface | `timers add/list/cancel` CLI+REPL two-lane; no UI in-wave; device-local | PROPOSED |
| **D-R10** | **Toast dependency (stack approval)** | **`notify-rust` 4.18.x (single façade; zbus pure-Rust on Linux, mac-notification-sys, tauri-winrt-notification; MIT OR Apache-2.0; `z-with-tokio` evaluated at impl)** over the hand-assembled three-crate stack and the portal route | **USER MUST APPROVE** |
| D-R12 | Toast interactivity | Passive toasts only (GNOME drops `actions`); interactive snooze stays in-app (banner seed exists; reminder center deferred to #203) | PROPOSED |
| D-R13 | Sink config & kiosk | `[reminders] sink = auto/log/desktop`, default `auto` by AppMode (Desktop→toast, Daemon/Kiosk→log); failure logs, never retries/fallback-sends | PROPOSED |

(D-R2, D-R6, D-R8, D-R11 are engineering consequences of the above,
recorded in §3 with their anchors; they carry no independent choice.)

## 5. Test-strategy hooks (for the implementation chunks)

- **Domain proptests**: alarm strategies join the
  `persisted_state_strategy` family (referentially consistent: alarm
  rows ⊆ alarms map for `Adhoc` refs); P1/P2/P7/P8 gain alarm variants
  (P2's independent re-derivation grows the alarms loop); new: the
  prune-unobservability property (D-R7); `text_form` roundtrip +
  garbage rejection for `"alarm:"`; `AdhocWhen::In` stamping and the
  `dur ≤ 0` rejection; dedup-by-offset idempotence (second add at the
  same offset plans no actions and names the same rule).
- **Engine**: tick tests with alarms via paused time
  (`start_paused` + advance, the existing arm's discipline); the
  firing set includes task firings and alarm firings in one
  `(occurrence, reminder)` order; settlement prunes.
- **CLI**: typed receipts (added flag, removed ids); selector
  resolution outcomes; `--json` envelope; REPL lanes. No text
  assertions (AGENTS §5).
- **Sink**: `DesktopSink`'s payload mapping unit-tested at the port
  boundary with a fake backend seam kept minimal (the mapping is
  pure); config `sink` resolution typed; the full suite keeps running
  `sink = log` (LogSink path stays dependency-free in behavior).
- **Gates**: `cargo deny check` newly meaningful (per-target license
  tree, D-R10); insta passes are the two mechanical ones (alarms
  section; receipt/`--json` fixtures).

## 6. Risks & mitigations

- **Semantic change of a live verb** (`remind` replace→add): flagged in
  the changelog; first-use flow identical; the clobber footgun this
  removes is the issue's own complaint.
- **Enum ripple breadth** (F16-20): every exhaustive match is
  compiler-forced; the risk is a wrong *deliberate* arm — mitigated by
  the per-site table (D-R6) the implementing chunk walks, and by
  proptest re-derivations covering the new arm.
- **Prune-pass ordering bug** (compaction before alarm prune):
  property-pinned (D-R7); a mis-order shows as unobservability
  failure, not silent growth.
- **notify-rust upstream drift** (façade lag on a backend): the port
  confines it; the hand-assembled stack remains the escape hatch
  behind the same port (this is *why* D-R11 is strict).
- **zbus executor model**: default (internal executor thread) vs
  `z-with-tokio` — both supported from tokio; decided by measurement
  on the kiosk memory budget, not by preference.
- **Snapshot churn**: two mechanical insta passes, each owned by its
  widening chunk (DF-9 discipline).
- **Timer lateness expectations**: documented one-tick bound (D-R9);
  support burden accepted (knob exists).

## 7. Chunk mapping (framing §6 — advisory sizes confirmed)

| Framing chunk | This doc's parts | Notes |
|---|---|---|
| 6 — N-reminder surfaces (M) | A (D-R1–D-R4) | After sidecar chunk 4 (rules section + clock column — the verbs are the enqueue/tombstone sites, D-R2); migration slot shared |
| 7 — Standalone timers & alarms (M) | B (D-R5–D-R9) | Shares the `ReminderRef` ripple with 6; sequence 6 → 7 to land the anchor widening once; `adhoc_alarms` in the shared wave migration |
| 8 — Reminder toast sink (M) | C (D-R10–D-R13) | Independent of 6/7 except the payload-anchor widening (D-R6.5) — implement against the anchor shape or land after 7 |

Dependency spine unchanged from framing §6 (3 → 4 → 6; 8 independent;
sequencing of 7 pinned here to follow 6).

## 8. Backlog candidates (out-of-scope ideas; the user promotes)

- Reminder center overlay (fired-list with per-firing snooze/mute/
  cancel) — natural #203 companion.
- `timers` timezone override via chunk 2's `UserPreferences` home.
- Sidecar `adhoc-alarms` section for the D5 substrate (name noted,
  D-R8).
- Repeat/insistent alarms (the #203 insistence family on alarms).
- `tasks remind --edit`-style bulk editing (nothing in the issue asks;
  the add/remove pair covers the catalog).

## 9. References

- Framing: `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`
  (§2 scope, §5 Q3, §6 chunks, D-D2/D-D6).
- **Chunk 1 (cited, normative for the rules section)**:
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md` §3.9
  (D-S9), §3.2/D-S2 (schema, reserved sections), §7 chunk interfaces
  ("doc 3: the N-rule verbs are the enqueue sites").
- Chunk 2: `.artifacts/plans/2026-10-18-field-family-design-plan.md`
  (UserPreferences home — the timers tz note only).
- Reminders lineage: `2026-10-12-reminders-design-plan.md` (D1–D5,
  D-a…D-e, §6 port, §6.1 insistence reservation, "plan 5"),
  `2026-10-13-reminders-domain-model-plan.md` (D6/D16 compaction &
  key precedents, P1–P8).
- Roadmap: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §3.2 (wave-1 reminder items), §6.
- Audit: F16-20/F16-21 (v0.1.0 audit, cross-referenced on #204/#211).
- Issues: #204, #211, #344 (+ #202 scope fence, #203 insistence,
  #350 the sidecar repair fence chunk 1 recorded).
- Toast dependency evidence (verified 2026-10-19): notify-rust
  docs.rs/crates.io (4.18.2; MIT OR Apache-2.0; zbus ^5 default /
  `mac-notification-sys ^0.6.15` / `tauri-winrt-notification ^0.8`;
  `z-with-tokio` feature; MSRV 1.89), zbus docs (tokio feature),
  XDG notification spec + GNOME Shell `actions` non-support
  (Chromium's capability-probe fallback as the industry precedent).
