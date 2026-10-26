# [Plan] Reminders Surface Pair — Design Round (timers surface, insistence levels, the escalation lane)

- **Date**: 2026-10-26
- **Kind**: design-round doc (backlog-triage dispatch §14 **DR-3**:
  `.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md:998-1002`).
  Design only — no production code in this commit.
- **Origin**: issues **#352** (timers & alarms UI) + **#203** (insistence
  levels), ruled one round by D-R9/D-R12
  (`.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md` §3.9/§3.12);
  the **binding semantics baseline** is DR-1's R3 notification model
  (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  §4-R3, esp. R3.1–R3.2, and its stage-2 doc
  `2026-10-25-reminder-rule-sync-design-plan.md` D-3/D-6/D-10) — the
  in-app notification persists until the user acts; **dismissal** (explicit,
  or implicit via Mark Done) is settlement, synced for Rule refs; the
  delivery receipt is only the per-device OS-toast guard. Audit input
  **F16-19** (v0.1.0 findings,
  `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md:154` — the
  lane question this round must answer). User direction on the two flavours:
  `2026-10-12-reminders-design-plan.md` §6.1.
- **Session rulings (2026-10-26, recorded in §4)**: the settlement re-time
  is **in-round** (chunk 1 — the pre-DR-1 leftovers are bugs, not design
  forks; the OS-delivery/visual plumbing stays queued for GUI review round
  3); insistence sound is **owned audio via `rodio`** (stack approval
  GRANTED this session, matrix in §3.10); the alarm rung is **firm,
  platform-honest** (no focus steal) with a **per-reminder configurable
  ladder**, sensible default `normal`; the timers surface is a
  **today-view section plus a status-bar chip**.
- **Branch / worktree**: `docs/reminders-surface-pair-design` in
  `../taskboard-WT-docs-reminders-surface-pair-design` (off `origin/main`
  @ `6d5507e`).
- **Status**: **ACCEPTED** — the user reviewed and accepted the plan as
  designed (2026-10-26, PR #478), confirming the four session rulings.
  Implementation is the four chunk issues the round opened (§7:
  #474, #475, #476, #477); a follow-up dispatch round chunks them.

---

## 1. Context & verified anchors (do not re-derive; verified on this branch 2026-10-26)

Symbols are the stable keys; line numbers are this branch's.

| Fact | Anchor | Evidence |
|---|---|---|
| The dismissal model is landed and binding | domain `reminders.rs` | `ReminderDismissal` :311–320 (R3 two-fact split; Rule refs roam, Default/Adhoc rows stay local); `ReminderState.dismissals` :418–420; `is_dismissed` :447–451 |
| The in-app notification surface exists (DR-1 R3 chunk C) | domain `reminders.rs`; ui-slint | `active_notifications` :768–818 (fired ∧ ¬dismissed, derived at read, `(occurrence, reminder)` order); `presentation.rs` `notification_rows` :381–410; `models.rs` `push_notifications` :376–410 (row cap + overflow); `ui/compositions/notification-surface.slint` (Dismiss / Mark Done / Snooze / Mute buttons) |
| The interactive command set is complete | domain `command.rs` | `DismissReminder` :1312+ (stamps, persists, `UpdateAppData` enqueue for task refs; Adhoc arm requires a live alarm row); `SnoozeReminder`/`SetTaskRemindersMuted` (per DR-1 D-10, unchanged); `CreateAdhocAlarm`/`CancelAdhocAlarm` (D-R5/D-R7 verbs) |
| **Bug: settlement is keyed on delivery, not dismissal** | domain `reminders.rs`, state tests | `prune_settled_alarms` :585–598 prunes any alarm whose effective occurrence is *delivered*; pinned by engine test R9 (`state/tests/reminders.rs:469` — "the settled alarm row pruned by the same batch's post-pass"). Consequence: a fired alarm vanishes in the batch its delivery lands — `active_notifications`' alarm arm (:803–815) is unreachable through the engine, and snooze-after-fire fails `UnknownAdhocAlarm` |
| **Bug sibling: snooze-link compaction can resurrect a dismissed notification** | domain `reminders.rs` | `compact_snooze_chains` :549–560 prunes links whose both endpoints are *delivered*. Trace: fire at A → snooze A→B → fire at B → dismiss (keyed B) → link compacts (A, B delivered) → effective reverts to A → `active_notifications` sees (A): delivered ∧ ¬dismissed(A) → the dismissed notification resurfaces. The existing compaction property pins unobservability in `due_firings` only — which is exactly the hole |
| The post-batch pass is where both run | domain `persistence.rs` | `compact_snooze_chains` then `prune_settled_alarms` at :495–501 (caller-order load-bearing), reached from `EngineCore::interpret` (`state/core.rs:58`) — the tick's delivery batch (`state/actor.rs:1085–1113`) runs it in the same `interpret` |
| The evaluator is pure, occurrence-keyed, snooze/mute/dismissal-aware | domain `reminders.rs` | `due_firings` :697–750 (fires iff effective ≤ now ∧ ¬delivered ∧ ¬dismissed); `effective_occurrence`/`walk_chain` :616–650; `alarm_occurrence` :658–661 |
| No `urgency`/`insistence` field exists anywhere in the reminder domain | domain `reminders.rs` | `ReminderRule` :230–246 (`id, task, offset, created_at, updated_at`); `AdhocAlarm` :270–279 (`id, label, fire_at, created_at`); `Notification` :471–482 (`reminder, anchor, title, occurrence`) |
| `urgency` is already a **task field** — the flavour field needs another name | domain `appdata.rs`, `entities` | `MODELED_TASK_FIELD_KEYS = [effort, importance, urgency, scheduled]` :42; `entities::Urgency` on `Task`. A reminder field named `urgency` would collide with a different, synced concept |
| The notifier lane (E-20) is fire/withdraw only | domain `messages.rs`; app `notifications.rs` | `NotifierMessage { Fire(Notification), Withdraw { reminder, occurrence } }` :492–505; port `NotificationSink::deliver/withdraw` (`notifications.rs:38–62`); `DesktopSink` :136–219 (tracked handles, `TRACK_CAP`, best-effort withdraw); `spawn_notifier` :297–332; `resolve_sink` (D-R13 matrix) :283–290 |
| **E-14 `SystemEvent` already derives `Clone`, not `Copy`** | domain `messages.rs` | :376–395 — `NetworkLost`…`StorageFailed`; no `Copy` since the field-carrying variants landed. F16-19's "reuse breaks `Copy`" premise has aged; the lane question needs a fresh answer (§3.6) |
| The UI repaint lane is state-derived | domain `messages.rs`; ui-slint | `EngineSignal::StateUpdated` :398–402 (`Copy`, the only repaint signal); the UI holds `broadcast::Receiver<EngineSignal>` + its own slint `Timer`s (`ui-slint/src/lib.rs:170–247`) — the banner precedent: data changes repaint via the signal, the passage of time via the timer (2026-10-12 design §7) |
| The timers CLI exists; no UI surface does | app `commands/timers.rs`, `cli.rs` | `timers add/list/cancel` two-lane (D-R9); `list_on` :194–218 renders remaining/overdue; `TimersCmd` `cli.rs:585`; `remind` add/remove family `commands/reminders.rs` |
| The reminder-rule sync lane (DR-1) is the additive-side precedent | domain `appdata.rs` | `RulesEntry`/`RuleEntry` :289+ keyed by rule UUID; embedded-clock LWW union; verbatim unknown-key relay — an additive field folds as its default on old documents |
| Storage | storage migrations | `0004_reminders.sql` (`reminder_rules`/`deliveries`/`snoozes`/`muted`), `0008_adhoc_alarms.sql`, `0009_reminder_dismissals.sql` — next free migration **0010** |
| UI homes for the new surface | ui-slint | `ui/views/today-view.slint` (sectioned rows, `TodayModel`); `ui/compositions/status-bar.slint`; `ui/compositions/rule-list.slint` (per-rule rows in the task editor, D-R4); action layer `actions.rs` (`ActionId::ReminderSnooze/ReminderMute/ReminderRuleAdd/ReminderRuleRemove` :199–202) + `router.rs` dispatch |
| Kiosk fence | DR-1 R3.5.3 | Kiosk is banner-only for reminder surfaces (interaction runs the full GUI); `resolve_sink` maps Daemon/Kiosk → `LogSink` |

Out-of-scope fences that stay binding: recurrence and repeat/nag policy
(reminders design D1/D4); native toast action buttons (#444, gated on
per-platform functional tests); Start Task (#210 carries the button when
time tracking lands); delivery-proofs-style sync of snoozes/mutes/alarms
(DR-1 D-3: device-local); the OS-delivery/visual plumbing bugs queued for
**GUI review round 3** (the user's 2026-10-26 direction: pre-DR-1
leftovers are bugs; this round designs **on** the recorded semantics, and
chunk 1 fixes the domain half — §3.1).

## 2. Scope & non-goals

**In scope (design only):** the settlement re-time that makes fired
alarms manageable state (Part A); the `Insistence` flavour fields across
domain/storage/sync/CLI (Part B); the F16-19 lane ruling (Part C); the
timers & alarms surface in today-view + status bar (Part D); the
escalation behavior profiles — alarm window, attention requests,
re-toast cadence, owned sound via `rodio` (Part E); decision record §4;
test hooks §5; chunk mapping §7.

**Non-goals:** any production code; fixing the OS-delivery/visual
plumbing bugs (GUI review round 3 owns them; this round's chunk 1 owns
only the domain semantics re-time); recurring alarms or snooze presets;
per-run sound/theme pickers; kiosk reminder surfaces (banner-only fence);
native toast buttons (#444); config knobs beyond the recorded default
(the ladder is per-reminder data, not `[reminders]` config).

---

## 3. Design

### Part A — Settlement re-time: dismissal is the settle, delivery is only the toast guard (D-I1)

The DR-1 R3 model already rules this ("the in-app notification persists
on every device **until the user acts**"); two pre-R3 mechanisms still
key on delivery and must be re-timed. Both are bug fixes, not design
forks — the session ruling places them in this round as **chunk 1**.

- **D-I1a — `prune_settled_alarms` keys on dismissal.** An alarm prunes
  iff its effective occurrence is **delivered ∧ dismissed** (the user
  settled it), replacing the delivered-only criterion
  (`reminders.rs:585–598`). Everything else about D-R7's lifecycle
  stands: map membership is liveness, `remove_alarm`'s cascade
  (:568–574) unchanged, no GC/retention knob, unfired alarms live until
  fired or cancelled. A fired-then-dismissed alarm still prunes — with
  its deliveries, dismissals, and snooze links — exactly at the
  dismissal's post-batch pass. Consequence (the point): between fire
  and dismiss, the alarm row **and** its delivery live in state, so
  `active_notifications`' alarm arm becomes reachable and
  `SnoozeReminder`/`CancelAdhocAlarm` on a fired alarm resolve.
- **D-I1b — snooze-link compaction keys on dismissal.**
  `compact_snooze_chains` prunes a link iff **both endpoints are
  dismissed** (:549–560), replacing both-endpoints-delivered. Rationale:
  a link can still influence two readers — `due_firings` and
  `active_notifications` — and under R3 an occurrence that is merely
  delivered is *not* dead (it is the live in-app surface); an occurrence
  that is dismissed is dead in both readers (no fire, no row). This
  closes the resurrection trace of §1: the dismissed (B) link survives
  compaction, effective stays at B, and `is_dismissed(B)` keeps the row
  off the surface. Delivered-ness leaves the criterion entirely — a
  dismissal row may legally exist for an unfired occurrence (the
  `DismissReminder` planner accepts pre-dismissal), and a
  both-endpoints-dismissed link is unobservable in both readers
  regardless of delivery.
- **D-I1c — the properties re-pin, they are not weakened.** The
  compaction-unobservability and prune-unobservability properties gain
  the **second reader**: invariance of `active_notifications` under the
  post-pass, on arbitrary states (the old pins covered `due_firings`
  only — the exact hole the resurrection slipped through). Engine test
  R9 is re-timed to the recorded contract: fire → row survives (and the
  in-app surface carries it) → dismiss → pruned by that batch's
  post-pass. These are semantics-correcting changes with DR-1 R3 as the
  written authority (AGENTS §5's "do not weaken a correct test" is
  satisfied the honest way: the old tests pinned a superseded contract).
- **No schema change.** Rows simply live longer; migrations 0004/0008/0009
  are untouched. Chunk 1 is domain + state tests + engine tests only.
- **Fence:** the OS-delivery plumbing (toasts never reaching the
  notification daemon; in-app rows appearing only for overdue tasks) is
  **GUI review round 3's** queue per the user's direction. Chunk 1's
  issues note the division so the two rounds cannot double-fix or
  double-close.

### Part B — The insistence flavour fields (D-I2–D-I4)

- **D-I2 — the type: `Insistence { Normal, Alarm }`.** A new domain
  enum, kebab-case serde (`"normal"`/`"alarm"`), `Default = Normal`,
  `Copy + Eq + Ord` (snapshot order: `Alarm` orders before `Normal` so
  an insistent row sorts first — or keep declaration order and sort
  explicitly; the implementing chunk pins one). **Named `Insistence`,
  not `Urgency`**: `urgency` is an existing synced **task field**
  (`entities::Urgency`, `appdata.rs:42`) — a second meaning under the
  same name in one crate is a misdirection bug (§1 anchor). #203's body
  says "urgency field"; this round records the rename as an engineering
  consequence of the later-landed task-field family.
- **D-I3 — the fields.**
  - `ReminderRule.insistence` — set at creation, immutable per row (no
    rule-edit verbs exist; changing a task's insistence re-creates the
    rule, the D-R1 dedup-by-offset semantics unchanged).
  - `AdhocAlarm.insistence` — set at creation; `Cancel` remains the
    off-switch regardless of level.
  - `Notification.insistence` — the fire-time snapshot (the `title`
    precedent: copied by `notifications_for` from the rule/alarm row),
    so the E-20 payload is self-describing for sinks.
  - The **implicit default reminder stays `Normal`** — it has no row to
    carry a field, and v1 adds none. Per-task insistence is therefore
    carried by the task's explicit rules; the task editor's reminder
    section (Part D) exposes one picker that stamps the task's rules,
    with the hint that a rule-less task uses the gentle default.
  - The **evaluator stays insistence-agnostic** (the recorded fence:
    it decides *when*, never *how loud*) — `due_firings`/
    `active_notifications` signatures and logic are untouched; only
    `notifications_for` copies the field.
- **D-I4 — persistence & sync (additive, one motion).**
  - **Migration 0010** (next free, §1): `reminder_rules` gains
    `insistence TEXT NOT NULL DEFAULT 'normal'`; `adhoc_alarms` the
    same. One migration, both columns; codec arms + roundtrip tests.
  - **Sidecar**: `RuleEntry` gains `insistence` with default-on-missing
    decode — an old document folds as `Normal` (the additive-section
    precedent DR-1 landed for every rules field). Emission writes it.
    Mixed fleets: pre-round devices relay the section verbatim; a rule
    (re-)created by a pre-round device carries no field and folds
    `Normal` — accepted and recorded (no edit verbs exist, so a
    same-clock field conflict is unconstructible).
  - **Dismissal/delivery rows**: untouched — insistence never keys
    suppression.
  - **CLI/REPL**: `tasks remind <task> [--before <dur>]
    [--insistence <normal|alarm>]` and `timers add <label>
    (--in|--at <t>) [--insistence <normal|alarm>]`; `remind --list` and
    `timers list` render the level; receipts carry it. Typed outcomes
    only; unknown values are typed CLI errors (the `KeymapPreset`
    precedent for unknown enum values).

### Part C — The lane ruling: F16-19 discharged as "no new lane" (D-I5)

F16-19's fork — grow E-14 `SystemEvent` vs. add a dedicated
notification lane — is answered by the architecture that landed while
the audit waited:

1. **E-14 does not grow.** No surface needs a pushed "notification
   fired" event: fired-ness and awaited-ness are **fully derived**
   (deliveries ∧ ¬dismissals, `active_notifications`), and every
   mutation that changes them already publishes
   `EngineSignal::StateUpdated` — the UI repaints and re-derives (the
   banner's exact contract). The `Copy`-break concern is doubly moot:
   the enum already carries `Clone`-only payloads, and nothing is added
   to it.
2. **E-20 stays fire/withdraw.** The engine→notifier mpsc remains the
   OS-toast lane, exactly as ADR 0013 (amended by DR-1) describes.
3. **The one new consumer — the escalation driver (Part E) — is an
   app-layer task beside the notifier, not a channel.** `bootstrap.rs`
   spawns it with the published-state handle (`Arc<ArcSwap<AppState>>`)
   and the resolved sink. It wakes on its own cadence only while ≥1
   un-dismissed `alarm`-level notification exists (derived from state),
   and acts through existing seams: `NotificationSink::deliver` for
   re-toasts (the payload already carries insistence after D-I3) and
   its own `rodio` output for sound. No engine change, no new channel
   topology, no `SystemEvent` variant — the audit's "dedicated lane
   adds topology" cost is paid down to *one task*.
4. **Surfaces stay thin.** The notification rows, the timers section,
   and the alarm window are state-readers on the ArcSwap with their own
   clocks (data changes via `StateUpdated`; time passage via slint
   timers — the §7 banner precedent of the 2026-10-12 design,
   re-affirmed). This is the recorded direction of #203's own body:
   escalation is "a UI/notifier rendering loop driven by the
   un-dismissed notification's presence in state".

**Kiosk fence (restated):** kiosk keeps banner-only (R3.5.3), `LogSink`
(D-R13), no alarm window, no sound. Insistence is a desktop-mode
feature; a kiosk install that wants alarms runs the full GUI.

### Part D — The timers & alarms surface (#352) (D-I6–D-I8)

- **D-I6 — home: a today-view section + a status-bar chip.** A
  `timers` composition inside `ui/views/today-view.slint` (the
  daily-driver home; no new view family — R1 component discipline), and
  a status-bar chip (`ui/compositions/status-bar.slint`) that appears
  while any timer exists, showing the nearest fire ("Tea · 2m") and
  focusing the section on click. Desktop-only (the kiosk model-side
  gate precedent from `push_notifications`).
- **D-I7 — the rows are pure presentation over domain reads.**
  - **Active (unfired) alarms**: one row per `state.reminders.alarms`
    entry — label, remaining time (effective occurrence − `now`;
    effective via the domain's `alarm_occurrence` walk, called from
    presentation exactly like `overdue_tasks`), an insistence badge,
    and a cancel affordance dispatching `CancelAdhocAlarm`. Countdown
    text re-renders on the existing UI timer; data changes via
    `StateUpdated`. Canonical order: effective occurrence, then id (the
    CLI list's contract).
  - **Awaiting (fired ∧ ¬dismissed) alarms**: the **notification
    surface owns these rows** (one source of truth — R3 chunk C's
    surface, already mounted). The timers section shows only a compact
    awaiting-count pointer to it. What changes for alarm rows there:
    anchor-aware affordances — **Dismiss / Snooze / Cancel**, no
    Mark Done (no task), no Mute (mute is task-shaped; today's row
    template shows all four buttons regardless of anchor — the
    surface repair rides this chunk). Snoozed alarms leave the
    awaiting set by derivation (effective moves; the delivery key
    follows the chain), reappear at the snooze target.
  - **Creation affordance**: an inline form — label + the timing
    grammar (`in 3m` / `at 09:30`, host-local parsing for `at` — the
    D-R9 CLI ruling mirrored; duration parsing reuses the domain's
    offset text-form grammar, absolute parsing is a new pure
    presentation fn) + the insistence picker (default `normal`).
    Dispatches `CreateAdhocAlarm { label, when, insistence }` through
    the action layer.
- **D-I8 — action layer growth is minimal.** New `ActionId`s:
  `TimerCreate`, `TimerCancel`; the awaiting-row buttons reuse the
  notification surface's existing dispatch paths (`DismissReminder`,
  `SnoozeReminder`) plus `TimerCancel`. Router arms map to the existing
  commands; the UI holds no timer state of its own (remaining time is
  derived per frame; no UI-side ticking state machine).

### Part E — Escalation behavior profiles (D-I9–D-I10)

- **D-I9 — the ladder (per-reminder data, two rungs, default
  `normal`).** The session ruling: firm, platform-honest insistence
  with a configurable ladder per reminder and a sensible default.
  - **`normal`** — exactly today's behavior: one passive toast at fire
    (DesktopSink) + the in-app row until dismissed. No sound, no
    window, no re-toast.
  - **`alarm`** — the wake-up profile, a strict superset:
    1. **Fire (t₀):** toast with the platform's critical-urgency hint
       where the server honors one (DesktopSink grows the hint behind
       the port — payload data only, the signature stays), the in-app
       row, and the **alarm window** opens.
    2. **Alarm window:** an always-on-top, small overlay listing every
       awaiting `alarm`-level occurrence (title/label, elapsed
       phrase, Dismiss / Snooze / Cancel per row). It stays open,
       re-raising itself, until the awaiting set is empty. **No focus
       steal** — the platform-honest ruling: an attention request
       (taskbar blink / dock bounce via the windowing system's urgent
       hint) and always-on-top persistence carry the insistence;
       Wayland forbids steal outright and X11/Windows steal is hostile
       to a user mid-input in another app. The window is another
       state-reader (same derivation as the notification rows) — it
       renders because awaiting `alarm` occurrences exist, nothing
       else.
    3. **Re-toast cadence:** while an `alarm` occurrence has been
       awaiting ≥ 1 minute, the escalation driver re-delivers its
       toast every 2 minutes (bounded: one re-toast per awaiting
       occurrence per interval; the DesktopSink's tracked-handle map
       replaces the prior toast — no stacking).
    4. **Sound ramp:** owned audio via `rodio` — a synthesized chime
       pattern (samples generated in code; **no decode dependency**
       — the feature-minimal rodio pin, symphonia excluded) starting
       at t₀+1min, repeating every 30s, volume stepping 30%→100% over
       5 minutes, capped there until dismissal. Audio-device failure
       degrades to visual-only (logged, never fatal — the D-R13
       stance); sound plays only in `Desktop` mode with the desktop
       sink resolved.
  - The cadence/volume constants are named in the driver and recorded
    here; promoting them to `[reminders]` config is a future knob, not
    v1 (the ladder itself is per-reminder data, per the ruling).
- **D-I10 — the dependency (STACK APPROVAL — granted 2026-10-26).**
  `rodio` enters `[workspace.dependencies]` for `taskboard-app`, pinned
  once (AGENTS §4), default features only minus decode-capable ones
  (verify at implementation: rodio's minimal output feature set; the
  tree is `rodio → cpal`, licenses MIT OR Apache-2.0 both —
  `cargo deny check` gates the tree per target). The rejected
  alternatives, on the record: **OS toast sound hints** (the server
  plays once per toast; no volume control, no ramp — cannot express
  "louder the longer ignored"); **visual-only insistence** (guts the
  recorded wake-up vision — sound is the insisting medium);
  **hand-rolled cpal output** (the same tree minus the mixer/queue
  conveniences rodio already owns). Escape hatch: the port-shaped
  confinement — all audio lives behind the escalation driver, so a
  backend swap never touches surfaces or engine.

---

## 4. Decision summary

| ID | Question | Decision | Status |
|---|---|---|---|
| D-I1 | Settlement key (the pre-DR-1 leftovers) | Prune + compaction key on **dismissal** (delivered ∧ dismissed for alarms; both-endpoints-dismissed for links); properties re-pinned over both readers; no schema change; in-round as chunk 1, OS-plumbing bugs stay with GUI review 3 | RULED (2026-10-26 session) + recorded |
| D-I2 | Flavour type & name | `Insistence { Normal, Alarm }`, default `Normal`; named to avoid the `entities::Urgency` task-field collision | recorded (engineering consequence) |
| D-I3 | Where the flavour lives | Fields on `ReminderRule`, `AdhocAlarm`, `Notification` snapshot; implicit default stays `Normal`; evaluator untouched | recorded |
| D-I4 | Persistence & sync | Migration 0010 (two additive columns); sidecar `RuleEntry` additive with default-on-missing fold; CLI `--insistence` flags | recorded |
| D-I5 | **The F16-19 lane** | **No new lane**: E-14 untouched, E-20 untouched; surfaces derive from state (`StateUpdated` + UI clocks); escalation driver = one app-layer task beside the notifier reading `ArcSwap` + acting via the sink port and rodio | recorded (discharges F16-19's P2) |
| D-I6 | Timers surface home | Today-view section + status-bar chip; desktop-only; no new view family | **RULED (2026-10-26 session)** |
| D-I7 | Surface shape | Unfired rows in the timers section (countdown, cancel); awaiting rows owned by the notification surface with anchor-aware buttons (Dismiss/Snooze/Cancel); inline creation form with the `in`/`at` grammar + insistence picker | recorded |
| D-I8 | Action layer | `ActionId::TimerCreate`/`TimerCancel` only; everything else reuses existing commands/dispatch | recorded |
| D-I9 | Alarm-rung behavior | Firm, platform-honest: always-on-top alarm window + attention request + re-toast cadence + sound ramp; **no focus steal**; ladder per-reminder, default `normal` | **RULED (2026-10-26 session)** |
| D-I10 | Sound dependency | **`rodio` (stack approval GRANTED 2026-10-26)** — owned audio, synthesized chime, no decode feature; degradation never fatal | **RULED (2026-10-26 session)** |

## 5. Test-strategy hooks (for the implementation chunks)

- **Domain (chunk 1)**: the two unobservability properties gain the
  `active_notifications` reader on arbitrary states; the resurrection
  trace of §1 becomes a deterministic regression test (fire → snooze →
  fire → dismiss → assert the surface stays empty); prune/compaction
  keying tests re-timed (delivered-only no longer prunes; dismissal
  does).
- **Engine (chunk 1)**: R-series re-pins on the paused clock — fire
  leaves the alarm row + in-app surface row; `SnoozeReminder` on a
  fired alarm resolves and moves the awaiting key; dismissal prunes in
  that batch's post-pass.
- **Domain/storage (chunk 2)**: insistence serde roundtrips + unknown
  value rejection; migration-0010 upgrade test (existing rows fold
  `normal`); sidecar fold default-on-missing + emission includes the
  field; mixed-fleet relay property (verbatim unknown-key carry is
  already pinned — extend the fixture).
- **CLI (chunk 2)**: typed receipts carrying insistence; unknown
  `--insistence` value is a typed error; `--json` envelope rows.
- **UI (chunks 3–4)**: pure row/phrase/countdown functions unit-tested
  with structural assertions (no TTY, no slint run loop in unit tests;
  the `notification_phrase` precedent); anchor-aware affordance
  visibility as a pure per-row decision; the alarm window's
  open/closed predicate is the same pure derivation, tested as such;
  golden visuals extend per the existing harness.
- **Driver (chunk 4)**: the cadence/volume ladder as a pure
  `fn escalation_step(awaiting, now) -> Action` tested over
  constructed states (deterministic; no sleeps — inject the clock);
  the fake sink records re-toasts; audio-init failure degrades to
  visual-only (typed outcome, not a panic); `cargo deny` gates the
  rodio tree.
- **Gates**: insta passes are the mechanical ones (rules section with
  insistence, receipts); no log-text or UI-label assertions anywhere
  (AGENTS §5).

## 6. Risks & mitigations

- **Chunk 1 changes pinned engine behavior** (R9 et al.): authority is
  the written DR-1 R3 contract; the re-timed tests cite it; the
  properties gain the missing reader so the next semantics drift fails
  a proof, not a user.
- **Dismissal-keyed liveness grows state** (fired-undismissed rows live
  indefinitely): bounded by user action by construction — the surface's
  entire point; the DR-1 age-pruning backlog entry already re-terms to
  dismissal rows, and D-I1 prunes at settle. Kiosk-scale growth is rows
  per user inaction, not per tick.
- **The resurrection fix could over-retain links** (both-dismissed
  criterion is weaker pruning than both-delivered): links whose
  endpoints are all dismissed are dead in both readers; any residual
  growth joins the age-prune entry's terms. Property-pinned.
- **rodio/cpal platform surface** (ALSA/PipeWire quirks on the ARM
  targets): confined behind the driver; failure degrades to visual-only
  and is logged; the desktop is the only mode that routes audio. Deny
  gates the license tree; MSRV checked at pin time.
- **Always-on-top on Wayland** (some compositors constrain it): the
  alarm window's insistence does not rest on any single mechanism —
  window + attention + re-toast + sound are independent channels; the
  capability matrix is documented in the chunk, GNOME/KDE tested first.
- **Slint focus/attention API drift**: attention request is the
  windowing layer's urgent hint; if the slint version lacks it on a
  target, that channel no-ops and the others carry the rung (the
  platform-honest stance taken structurally).
- **Scope creep into GUI review 3's queue**: the fence is written into
  chunk 1's issue (domain semantics only) and this doc §2; the
  OS-delivery bugs keep their existing tracker entries.

## 7. Chunk mapping → the issues this round opens

| Chunk | Scope | Size | Issue |
|---|---|---|---|
| 1 — settlement re-time | D-I1 (domain + state/engine test re-pins; no schema) | S/M | `backlog: DR-3 chunk 1 — reminder settlement re-time (prune & compaction keyed to dismissal)` |
| 2 — insistence fields | D-I2–D-I4 (domain enum + fields, migration 0010, sidecar additive, CLI flags) | M | `backlog: DR-3 chunk 2 — insistence flavour fields (domain, storage, sync, CLI)` |
| 3 — timers surface | D-I6–D-I8 (today-view section, status chip, creation form, anchor-aware notification affordances, actions) | M | `backlog: DR-3 chunk 3 — timers & alarms surface (today-view section + status chip)` |
| 4 — escalation | D-I5 (driver half) + D-I9–D-I10 (alarm window, attention, re-toast, rodio sound; DesktopSink urgency hint) | M | `backlog: DR-3 chunk 4 — escalation driver, alarm window, owned sound (rodio)` |

Sequencing: 1 → (2, 3) → 4. Chunk 2 is independent of 1 except for
snapshot fixtures; chunk 3's awaiting-row affordances need 1 landed to
be testable end-to-end; chunk 4 needs 2 (the payload field) and 3's
window mounting can ride 3 or 4 (one motion either way — the
implementing dispatch decides). A follow-up dispatch plan chunks these
once this round is accepted (the §14 DR-3 entry's own rule).

## 8. References

- Mandate: `.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md`
  §14 DR-3 (lines 998–1002).
- Semantics baseline: `.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  §4-R3 (R3.1–R3.5 + resolutions) and
  `.artifacts/plans/2026-10-25-reminder-rule-sync-design-plan.md`
  (D-3 roam-set, D-6 active surface, D-10 dismiss surface).
- Surface lineage: `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md`
  (D-R5–D-R9 the alarm model + CLI; D-R12 the deferral that this round
  completes); `.artifacts/plans/2026-10-12-reminders-design-plan.md`
  §6.1 (the two flavours) and §7 (the repaint precedent).
- Audit: F16-19 (v0.1.0 findings report, line 154; chunk 16 §8.5).
- Issues: #352, #203 (the round's pair); #211 (closed — the CLI-first
  timers surface this round's Part D completes); #444 (native toast
  buttons — fence); #210 (Start Task button — fence); #202 (DR-1,
  whose R3 ruling is the baseline).
- ADRs: 0013 (reminders; amended by DR-1 — this round's Part E adds the
  insistence profiles and the escalation driver to its follow-up
  amendment list); 0015 (slint UI architecture — the thin-client rules
  Part D/E rely on).
- Stack evidence (verified 2026-10-26): rodio/cpal crates.io metadata
  (MIT OR Apache-2.0; feature set) — re-verified at implementation per
  house rule; notify-rust urgency-hint capability (the DesktopSink
  extension of D-I9.1).
