# [Plan] Reminder-Rule & Delivery-Proof Sync — Design Round (agenda)

- **Date**: 2026-10-25
- **Kind**: stage 1 of a design round — the **agenda**, not the design.
  This document only fixes the question catalogue, the evidence base,
  and the candidate options stage 2 must resolve. It records **no
  design decisions**; every §4 question ends in "what stage 2 must
  decide". The accepted decisions of prior rounds (the sidecar design's
  §6 D-S1…D-S9, the version-capabilities rulings D-V1/D-V2) are
  **binding inputs, not re-arguable candidates**.
- **Origin**: GitHub issue #202 (`backlog: Sync of reminder rules and
  delivery proofs`, milestone `wave-daily-driver`, P3) — the charter of
  the reminders design round's D-a deferred property
  (`.artifacts/plans/2026-10-12-reminders-design-plan.md` §4.4 D-a,
  §15); audit findings F16-15/16/17 (+F16-08 shared)
  (`.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`).
- **Normative inputs read this round** (all verified against `main` @
  `abc5aa7`):
  - The parent design: `2026-10-12-reminders-design-plan.md` (§4.4 D-a,
    §15 review checklist).
  - The accepted sidecar design:
    `2026-10-18-appdata-sidecar-design-plan.md` (D-S1…D-S9 — **the
    rules-transport half of this round's charter, accepted and since
    landed**; §3.9 D-S9 is the rules-lane design; §1's binding
    constraints list; §7's doc-2/doc-3 interfaces).
  - The transport evidence:
    `2026-10-22-deck-1175-transport-characterization-report.md` (§3:
    all seven D1 attachment behaviors work identically on Deck
    1.17.5/1.19.0 with the `data` workaround; capabilities endpoint
    carries `deck.version` — the D-V1 probe seam).
  - The capability rulings:
    `2026-10-22-deck-version-capabilities-design-plan.md` (D-V1
    capability seam, D-V2 dynamic `scheduled` lane — accepted
    2026-10-22).
  - ADR 0004 (R1–R9 merge policy + board amendments), ADR 0007
    (push-first cycle, evidence-then-verdict, self-clobber guard),
    ADR 0010 (standalone-first; the connector port section already
    records the multi-board round's `ConnectorModule` decision),
    ADR 0013 (reminders; §12 decision 1's sync-readiness clause).
  - Issue #202's audit rider: F16-17 (proofs substrate-ready),
    F16-15 (substrate fork), F16-16 (state home), F16-08 (vocabulary
    fork).
- **Target**: this document only; stage 2 produces the design doc + any
  ADR amendment. Future implementation targets (if the round produces
  them): `taskboard-domain` (`appdata.rs`, `reminders.rs`, `merge.rs`,
  `pipeline.rs`), `taskboard-sync-nextcloud` (`flush.rs`), possibly
  `taskboard-state`/`taskboard-storage-sqlite` (only what §4's answers
  demand).
- **Branch / worktree**: `docs/reminder-rule-sync-design-round` in
  `../taskboard-WT-docs-reminder-rule-sync-design-round`, off `main` @
  `abc5aa7`.
- **Status**: DRAFT agenda — stage 2 unblocked. Three ruling passes
  received during stage 1 (2026-10-25, user direction): §4(b)-R (the
  roam-set), §4(b)-R2 (the remaining §4 questions), and §4-R3 (the
  notification model — **supersedes parts of R and R2**: fire
  receipts stay local, the synced fact becomes the user dismissal,
  and the notification surface enters the round). R3.5's open details
  are **resolved** (see R3.5 resolutions): every direction input the
  round needed is in. Stage 2's work: the design doc and the audit
  recordings.

---

## 1. The charter, restated against what already landed

Issue #202's framing — "V1 ships local-only because the Deck connector
has no native reminder model… a protocol design round, not a refactor" —
was written **before** the daily-driver wave. The wave's accepted
sidecar design (D-S1…D-S9) already answered and **shipped** the
round's transport question for the rules half:

- The per-board sidecar document lane is landed end to end: document
  codec + union + GC (`appdata.rs`), anchor ceremony and evidence
  (`sync-nextcloud/src/anchor.rs`, `actor.rs:591+`), the typed
  `AppDataEvidence` riding `SyncReport::Completed.app_data`
  (`messages.rs:485`), the fold (`pipeline.rs:668–709`,
  `merge.rs:314–499`), the flush between pull and report
  (`flush.rs:142+`, `actor.rs:629+`), the failure fences, the per-board
  `SidecarHealth` (`state.rs:75–97`), and the outbox op
  `LocalOp::UpdateAppData(TaskId)` (`outbox.rs:59`) enqueued by the
  rule verbs (`command.rs:1301/1327/1342`) and the app-field verbs
  (`command.rs:532`).
- The **rules section itself is landed**: `AppDataSections.reminder_rules`
  (`appdata.rs:67–73`), `RulesEntry`/`RuleEntry` with explicit
  tombstones (`appdata.rs:281–353`), the rules fold with guard,
  adoption of document rule ids, and strictly-newer tombstone removal
  (`merge.rs:437–499`), overlay assembly + tombstone derivation
  (`appdata.rs:621–670`).
- The v1 fence survived in its amended, precise form: *no reminder data
  enters the **native** merge/push path; rules ride the app-data
  document lane* (`pipeline.rs:853–855`, `pipeline.rs:1059`).

Consequently this round's open substance is exactly:

1. **Delivery proofs** — the reserved `proofs` section name
   (`appdata.rs:60–61` reserved names note; the section lands in
   `sections.unknown` verbatim today, proven by the codec test at
   `appdata.rs:830–872`). Nothing of the proofs section is designed or
   landed; this is the round's center of gravity.
2. **The roam-set policy** — snoozes/mutes are per-device per D-S9's
   scope fence and the parent design's D-a; the round re-confirms with
   reasons against the *current* model (which grew `Adhoc` alarms and
   `Default`-ref deliveries since the fence was written).
3. **The connector-vocabulary recording** — F16-15's action ("D5 round
   records branch + vocabulary decision together") and F16-08's fork,
   which this round must discharge *on the record* even if the
   technical answer is "no vocabulary movement".
4. **Compat, offline semantics, and the F16-16 state-home
   verification** for anything the round decides roams.

**What this round is not**: a re-design of the transport, the anchor
ceremony, the document envelope, the merge primitives, or the rules
section — all binding (sidecar design §1: "Binding, not re-arguable").

## 2. Verified baseline — the §15 commitment table

The reminders design round's sync-readiness commitments (§4.4 D-a,
§15; issue #202's "Proposed Approach" list), each verified in code at
`main` @ `abc5aa7`:

| # | Commitment (§15 / #202 wording) | Verdict | Evidence (file:line) |
|---|---|---|---|
| C1 | Portable UUID rule ids | **Holds** | `ids.rs:57–59` — `ReminderRuleId` is a `local_id!` UUIDv7 newtype; D-S9 made ids document-stable; the fold adopts a document rule id verbatim (`merge.rs:469–479`) |
| C2 | Deliveries/proofs: occurrence-keyed **additive** rows, cross-device merge = set-union, conflict-free | **Holds locally; the cross-device half is this round's open work** | `reminders.rs:241–249` — key is `(reminder, occurrence)`; `reminders.rs:344` — `BTreeSet` in `ReminderState`; `persistence.rs:325–331` — replayed delivery is a no-op (first-write-wins set-union); storage PK parity `migrations/0004_reminders.sql` (`PRIMARY KEY (reminder, occurrence)`) — the F16-17 verification; no document section exists yet |
| C3 | Per-field write clocks on rules, added in the `TaskClocks` pattern | **Holds, in the D-S9-consumed form** | The reservation was consumed: `reminders.rs:196–201` — `ReminderRule.updated_at`, embedded LWW write clock (creation stamps it = `created_at`; no edit verbs exist, so per-*field* granularity would model nothing — the single clock is the honest shape); column landed epoch-defaulted in `migrations/0007_task_field_family.sql:16`; the fold compares it (`merge.rs:462, 489`) |
| C4 | A connector capability seam for native server-side reminders | **Seam exists and is general; no reminder flag reserved** | `capabilities.rs:15–40` — `DeckFeatures` (D-V1), one additive flag per ruled behavior, `CONSERVATIVE` fallback; flags ride every report (`messages.rs:485–491`, both variants) and persist on the board's sync row (`state.rs:66`); §4(e) asks whether to reserve a flag now |
| C5 | No single-device assumption anywhere in the shapes | **Holds** | Board-scoped documents keyed by remote card id (`appdata.rs:46–58`), connector-tagged bindings (`messages.rs:151–158`), per-board `SyncStates` (multi-board DF-9); rule ids stable across devices via the document (C1) |
| C6 | Snoozes/mutes are per-device UX facts (v1) | **Holds in code — policy re-confirmation pending** | No snooze/mute op in `LocalOp` (`outbox.rs:23–59`); no document section; `AdhocAlarmId` documented "device-local — never synced" (`ids.rs:61–63`); `CreateAdhocAlarm` doc: "Device-local — no sync op, no sidecar section" (`messages.rs:267–275`); `migrations/0008` repeats it |
| C7 | v1 local-only is a backend fact, not a design commitment | **Superseded — the fact changed** | The Deck *native* gap remains, but the rules half syncs through the document lane since the daily-driver wave; the fence was amended, not deleted (`pipeline.rs:853–855`) |

Model facts that postdate the charter and constrain §4 (all verified):

- **M1 — `ReminderRef` grew a third arm**: `ReminderRef::Default(TaskId)`
  deliveries/snoozes exist (the default reminder fires and settles like
  any other), and `ReminderRef::Adhoc(AdhocAlarmId)` alarms fire,
  snooze, and settle (`reminders.rs:139–147, 225–235, 564–569, 620–633`).
  The parent design's "snoozes and mutes are per-device UX facts"
  sentence predates both.
- **M2 — the document may never carry local `TaskId`s** (binding,
  sidecar §1) — with the single D-S9 exception for rule UUIDs, whose
  document presence *is* their remote identity. A `default:<task-uuid>`
  proof would violate the rule *and* be meaningless cross-device (M3).
- **M3 — the implicit default's offset is per-device config**
  (`[reminders] default_before`; D5, "per-device by nature"): two
  devices compute *different occurrences* for the same task's default
  reminder, so `Default`-ref proofs cannot collide across devices even
  in principle.
- **M4 — deliveries are engine-internal facts, not command receipts**:
  the tick arm's batch records them (`RecordReminderDelivery`), and no
  outbox op is enqueued at that site today — every current
  `UpdateAppData` enqueue is inside `plan_command`'s command handlers
  (`command.rs:532, 1301–1342`). The flush work-gate is group- or
  rebuild-driven only (`flush.rs:88–139`).
- **M5 — proofs pruning exists locally only as a backlog entry**
  (age-based delivery pruning; parent design §15 + D-c cascade). The
  delete cascade prunes a rule's deliveries and snoozes with the rule
  (`persistence.rs:308–316`); orphan proofs for unknown rule ids are
  inert in evaluation (`due_firings` queries only live rules'
  refs, `reminders.rs:596–606`).
- **M6 — the relay guarantee is landed and tested**: unknown sections
  (`proofs`, `settings`) survive decode→encode value-verbatim, so a
  proofs-writing device and a proofs-ignorant device interoperate at
  the transport level today (`appdata.rs:830–872`, the reserved-names
  test; the union's additive-unknown property, `appdata.rs:431–488`).

## 3. Scope & non-goals (this round)

**In scope (design only):** the question catalogue of §4; a decision
record + ADR-side effects stage 2 produces; the F16-15/F16-08
recording; the compat/offline semantics of whatever roams.

**Non-goals:** any production code in this stage; the transport/anchor/
envelope/merge substrate (binding, §1); native Deck reminders (no such
Deck surface exists — the capability question is about *reserving the
seam*, not building the path); the `settings` section (F16-16's
roaming-settings inventory is explicitly **fenced out** of this round —
§4(c) verifies the reminder facts' state home and records the F16-16
obligation as satisfied-or-deferred for settings, nothing more);
insistence levels (F16-19, separate plan); age-based local proof
pruning (backloged; §4(d) only decides whether the *synced* section
changes that entry's terms); snooze/mute UI surfaces.

## 4. The question catalogue

Each question lists the candidate options (no ranking, no decision),
the evidence, and what stage 2 must decide. The decision record stage 2
produces must answer every question explicitly, including the
"no-action" answers.

### (a) Transport — what is left to decide

**The question.** Issue #202's option space was "sidecar-document-carried
vs per-rule attachment vs connector capability seam for future native
reminders". The first option is landed for rules; the second is
structurally answered (the roundtrip §4 granularity ruling settled
one-document-per-board over per-entity attachments); the third is the
only live part.

**Candidate options for the residual:**

- **(a-1) Everything reminder-shaped rides the landed document lane**
  (the `proofs` section joins `tasks` and `reminder-rules`). No new
  transport, no new capability surface. The reserved section name is
  consumed.
- **(a-2) A reminder-specific `DeckFeatures` flag row reserved now**
  (e.g. a native-reminders placeholder, default `false`), so a future
  native path has its seam pre-recorded.
- **(a-3) No flag reserved; the capability note recorded in prose**
  (the audit's "no speculative prep" stance, F16-05's action pattern):
  when a native path is ever designed, that round adds the flag row
  (the table is additive by construction, `capabilities.rs:15–21`) and
  fires the ADR 0010 amendment then.

**Evidence.** The document lane is shipped and characterized on both
live Deck tiers (`2026-10-22` report §4: full parity with the `data`
workaround); the capability seam is shipped and general (C4);
`DeckFeatures` currently carries exactly one flag, and every flag is
read by behavior gates — a dead flag would be the first.

**Stage 2 must decide:** (i) confirm (or reject, with reasons) that the
proofs section rides the landed document lane — the expected answer per
the binding granularity ruling, but it must be *recorded*; (ii) choose
(a-2) vs (a-3) for the native-reminders capability surface; (iii) if
(a-2), specify the flag's name, probe source (there is no Deck
reminders surface to probe — a flag that can never become `true` is
strong evidence for (a-3)).

### (b) What roams — rules (settled), proofs (the round's core), snoozes/mutes/alarms/defaults (policy)

**The question.** Which reminder facts cross devices. Rules roam
(landed, binding). The open set: deliveries (proofs), snoozes, mutes,
alarms, and the default-reminder machinery.

**Candidate options per fact:**

- **Proofs (Rule-ref):**
  - roam (the parent design's stated intent — "the proofs are designed
    to be synced", D-a; issue #202's title);
  - stay device-local (proofs become mere local logs; the at-most-once
    guard remains per-device).
- **Proofs (Default-ref and Adhoc-ref):**
  - stay device-local (M2 forbids the document carrying local TaskIds;
    M3 makes cross-device Default occurrences meaningless; alarms are
    documented device-local, C6). Note: the "remap by remote card id"
    escape hatch exists but is arguably moot under M3 — occurrences
    still differ per device, so rows would never match; stage 2 should
    record why it is rejected (or not).
  - The consequence to weigh: a `Rule`-ref proofs section creates an
    **asymmetry inside `ReminderDelivery`** — some rows roam, some
    never do. Stage 2 must decide whether that asymmetry is type-level
    (two kinds of rows), section-level (only Rule-ref rows serialize
    into the document, `ReminderState` unchanged), or recorded-doc-only
    (one type, a serialization filter).
- **Snoozes:**
  - stay per-device (v1 stance; D-b frames a snooze as "the *viewer's*
    later"; the snooze chain is interaction state, and a synced chain
    would re-nag a user on every device they did *not* snooze on);
  - roam (a "snoozed everywhere" semantic — one device's snooze
    silences the fleet; requires occurrence-identity alignment that
    only Rule-ref reminders have, and a merge policy for competing
    links on the same key that the additive union does not provide —
    snooze links *replace*, not accumulate, locally:
    `persistence.rs:333–341`).
- **Mutes:**
  - stay per-device (a mute is "stop nagging *me*"; the banner is
    unaffected by design, and a kiosk muted on the desktop should still
    show its banner reality — mute never touches the banner anyway);
  - roam (a fleet-wide "never remind me about this task" — a
    preference-shaped fact, which drags F16-16's state-home question
    into this round).
- **Alarms:** documented device-local (`messages.rs:267–275`,
  `ids.rs:61–63`, `migrations/0008`); the round confirms or overturns
  that record (an alarm is task-independent, so it has no card key to
  ride — a roaming alarm would need a new identity scheme in the
  document; the cost is real, the benefit unclear).
- **Default-reminder offset (`ReminderDefaults.default_before`):**
  stays per-device config (D5: "per-device by nature"; roaming it is
  exactly F16-16's "domain change wearing a config costume" in reverse
  — config pretending to be syncable state).

**Evidence.** C6's code record; M2/M3; the sidecar design's D-S9 scope
fence ("deliveries… are the D5 substrate's section (reserved name);
snoozes/muted are per-device interaction state"); the parent design's
D-a ("whether they roam is a policy question for the future round").

**Stage 2 must decide:** the roam-set, per fact, with recorded reasons;
the `ReminderDelivery` asymmetry shape if Rule-ref proofs roam; and
explicit confirmation (or overturn) of the alarm and default-offset
device-local records.

**Ruling received (2026-10-25, user direction, stage 1) — the roam-set
is fixed:**

- **Rule-ref proofs (dismissals) roam** and suppress fleet-wide (the
  §4(d).5 suppression policy is answered with them): a folded proof
  stops the local device's firing of that `(rule, occurrence)`.
- **Snoozes stay per-device**; revisit only on demonstrated need.
- **Mutes stay per-device**; revisit only on demonstrated need.
- Consequences accepted with the ruling: (i) a snooze on one device
  does *not* protect the others — they fire the original occurrence
  (S1's documented noise mode); (ii) `Default`-ref and `Adhoc`-ref
  proofs stay device-local by the M2/M3 facts (the `ReminderDelivery`
  asymmetry is confirmed as section-level or type-level — its exact
  shape remains §4(b)'s one open mechanical sub-item); (iii) alarms and
  the default-reminder offset are confirmed device-local; F16-16's
  settings inventory stays fenced out (the §4(c) answer is (c-1) up to
  recording).

**Second ruling pass (2026-10-25, user direction, stage 1) — the
remaining §4 questions resolved in direction:**

- **§4(a): no capability flag until a native path exists** (option
  a-3). A `DeckFeatures` placeholder would be the table's first flag
  no behavior reads, and there is no Deck reminders surface to probe.
  The ADR 0010 amendment fires the day a native path is designed.
- **§4(b) asymmetry shape: section-level, fail-closed.** The proofs
  section's writer emits **only Rule-ref rows** (a naive all-rows
  serializer would leak `default:<local-task-uuid>` keys — the type
  alone does not prevent it); the fold ignores `default:`/`alarm:`
  keys; one property test pins that no local `TaskId` reaches the
  wire. Confirmed context: alarms cannot sync (no op, no section,
  `ids.rs:61–63`), default reminders have no stored rows — the
  device-local enforcement "happens on its own" everywhere except the
  serializer.
- **§4(d).1 (semantic, now clarified):** "dismissal" was the agenda's
  loose word — there is no dismiss verb or state in the model. The
  `ReminderDelivery` proof is written automatically when a reminder
  *fires* (durable-then-notify, ADR 0013), and it is the at-most-once
  guard; a UI dismiss button closes the toast only. What syncs is the
  fire receipt: once one device has fired an occurrence, the others
  suppress it. **Mechanism default recorded: the tick arm's batch
  enqueues the ordinary `UpdateAppData(task)` op for each task whose
  reminder just settled** — one more `PersistenceAction` in a batch
  the tick already plans, reusing the landed op→group→flush path
  (no second trigger mechanism, no new gate state). Stage 2 confirms
  or improves; the outbox-write-from-a-non-command-path consequence
  is named and accepted.
- **§4(d).2: clock LWW, for uniformity** (user ruling: "every sync
  primitive uses the same functionality"). Sound because a proof has
  exactly one write site: the recording tick stamps the clock (the
  delivery stamp), so the embedded-clock union machinery applies
  unchanged and no special-cased set primitive exists.
- **§4(d).3: evidence-decided, no ruling needed.** Proofs fold at the
  landed §5b position beside the rules fold, and are **guard-immune**:
  the self-clobber guard protects local *edits* a same-cycle sync
  must not overwrite; receipts are append-only facts, never edited.
- **§4(d).4: evidence-decided, no ruling needed.** Accept
  proofs-section growth (one tiny row per fired occurrence; trivial
  at kiosk scale); no synced tombstones, no synced GC; cleanup stays
  with the backloged local age-pruning entry (its terms already
  require pruning only proofs too old to refire). Receipts of deleted
  rules are inert (evaluation consults live rules only) and harmless
  to re-fold; card-absence GC prunes their entries when the card goes.
- **§4(e): not a fork — a recording.** Content fixed by §4(a)'s
  ruling: no vocabulary movement, no flag; stage 2 writes the
  no-movement note into the design doc (the F16-15/F16-08 discharge).
- **§4(f)/§4(g): evidence-decided records.** Verbatim relay covers
  mixed fleets (old devices keep firing — accepted); receipts persist
  at fire time and join the next flush (record-and-forward); offline
  devices fire independently and converge on reconnect (accepted
  at-least-once window); read-only kiosks fold without flushing.
  Stage 2 pins the fold-order property: rules fold before proofs in
  the same pass, so an adopted rule is immediately suppressed by
  already-present receipts.

### §4-R3 — Third ruling pass: the notification model (2026-10-25)

The user clarified the intended *user-facing* semantics, which the
landed model does not implement (it auto-settles every occurrence at
fire time and has no user-facing dismiss at all — the interactive
surface was deferred to the reminders plan's "plan 5"). The direction,
from the user's perspective:

> Three synchronized devices; a reminder fires; **all three devices
> show the notification** (OS toast where available, and an in-app
> notification in the app itself). The in-app notification persists on
> every device **until the user acts**, with buttons **Dismiss**,
> **Mark Done** (implicitly dismisses), **Start Task** (implicitly
> dismisses), **Snooze**, and **Mute**. Dismissal — explicit, or
> implicit via Done/Start — is a **globally synced action**: it clears
> the in-app notification on all devices and withdraws the OS
> notifications where the platform allows. Snooze and Mute remain
> **local** operations, revisited only after experience with the
> system.

**R3.1 — Supersessions (this pass wins over R and R2 where they
conflict):**

- **Fire receipts do not sync and do not suppress other devices.** R's
  "dismissals (= fire receipts) suppress fleet-wide" is replaced: every
  device fires its own toast/in-app notification for an occurrence.
  `ReminderDelivery` remains a **local, per-device** at-most-once toast
  guard, written automatically at fire time (durable-then-notify
  survives unchanged — it now guards only the OS toast, not the user
  settlement).
- **The synced fact becomes the user dismissal** — a new domain fact
  with a new write site (a user command), which dissolves §4(d).1's
  flush-trigger question: R2's tick-arm `UpdateAppData` enqueue default
  is **superseded**; the dismissal command enqueues the ordinary
  `UpdateAppData(task)` op inside `plan_command` exactly like every
  other command. No tick-arm enqueue, no new trigger mechanism, no
  outbox-contract change.
- Snooze/mute per-device (R) is unchanged and now has its final
  reasons; §4(d).2's clock-LWW ruling carries over to dismissal rows;
  §4(d).3's guard-immunity carries over (a dismissal is append-only,
  never edited); §4(d).4's growth acceptance carries over.

**R3.2 — The model delta (design sketch for stage 2; the two-fact
split):**

- **Local fire receipt** — the existing `ReminderDelivery`
  (`reminders.rs:241–249`), unchanged role: written at fire time,
  prevents the OS toast from re-firing on this device, prune-safe by
  the existing cascade. **Stays out of the document entirely.**
- **Synced dismissal** — new fact, shape ruled by R2:
  `{ reminder: Rule-ref only, occurrence, dismissed_at (= the LWW
  clock) }`; one write site (the dismiss command's stamp); set-shaped
  membership; explicit tombstones never needed (dismissals are
  permanent for the occurrence). Proposed home: `ReminderState`
  gains a `dismissals` collection (additive, snapshot-breaking once);
  new persistence action `RecordReminderDismissal`; new storage table
  in the next free migration (0009), additive.
- **Evaluator changes:** `due_firings` suppresses an occurrence that is
  delivered (local) **or dismissed** (synced in) — a dismissal stops
  future firings fleet-wide; new pure derived view
  `active_notifications(tasks, reminders, defaults, now)` — the
  in-app surface reads it like `overdue_tasks`: active ⇔ this device
  fired the occurrence ∧ not dismissed. (A device offline at fire time
  fires on reconnect via the normal tick — the existing kiosk
  shutdown-recovery story — unless a synced dismissal beat it there;
  that race resolves correctly by derivation.)
- **The dismiss surface → engine:** new
  `StateCommand::DismissReminder { reminder, occurrence }` (stamps
  `dismissed_at`, persists, enqueues `UpdateAppData(task)`). Buttons:
  - **Mark Done** sends the existing `SetTaskDone`; the recommended
    shape is that the *command planner* also emits dismissals for the
    task's currently-active occurrences (the done state syncs natively
    and would clear derived notifications everywhere anyway, but the
    explicit rows close the **reopen hazard**: a task reopened after
    Done must not resurrect old occurrences as active notifications).
    Stage 2 confirms planner-emitted dismissals vs derivation coupling.
  - **Start Task** — open detail (R3.5): the model has no in-progress
    state, so Start = dismiss + (a UI affordance or a `scheduled`
    stamp — the user session picks).
  - **Snooze / Mute** dispatch the existing `SnoozeReminder` /
    `SetTaskRemindersMuted` commands unchanged (local).
- **Withdrawal:** the notification sink port grows a best-effort
  withdraw direction (the engine currently only sends *out*): the
  fire/withdraw messages share the notifier channel as a two-variant
  payload; `LogSink` withdraw is a no-op; the desktop sink withdraws
  per platform support (documented matrix — several notification
  servers cannot withdraw; the in-app notification is the source of
  truth, OS withdrawal is best-effort in both directions: closing an
  OS toast natively is not a dismissal).
- **Upgrade compat:** pre-R3 data has delivery rows only and no
  dismissal rows — without a backfill, every already-fired occurrence
  would surface as an active in-app notification after upgrade.
  Migration 0009 backfills one dismissal per existing
  `reminder_deliveries` row (`dismissed_at = delivered_at`), pinning
  "old behavior ≡ everything already dismissed". Mixed fleets: an
  old (pre-R3) device auto-settles occurrences and never shows or
  clears the new in-app surface; it relays the dismissal section
  verbatim (landed relay guarantee) — accepted, recorded.
- **Section naming:** the reserved `proofs` name (`appdata.rs:60–61`)
  now misnames the content; stage 2 picks the section name
  (`dismissals` recommended, `proofs` stays reserved/undocumented-out)
  — additive either way, the verbatim relay is name-blind.
- **Asymmetry (carried from R2, semantics shifted):** only Rule-ref
  dismissals sync. Default-ref occurrences differ per device (M3:
  per-device `default_before`), so their notifications clear locally
  only; alarms stay device-local end to end. Same fail-closed
  emission/fold filters and property pin, now on dismissals.

**R3.3 — ADR/docs impact:** ADR 0013's decisions 2–3 are amended by
this round (the at-most-once guard splits into the local fire receipt
and the synced user dismissal; durable-then-notify narrows to the OS
toast); the reminders design's deferred "plan 5" (desktop sink +
interactive surface) is **absorbed into this round** (R3.4 chunk C).

**R3.4 — Revised implementation breakdown (supersedes §6's estimate):**

| Chunk | Scope | Size |
|---|---|---|
| A — domain/state/storage | `ReminderDismissal` fact + `ReminderState.dismissals`, `DismissReminder` command (+ planner-emitted dismissals for Mark Done, reopen hazard), evaluator changes (`due_firings` dismissed-check, `active_notifications` view), `RecordReminderDismissal` action + `apply_actions`, migration 0009 (table + delivery backfill), proptests | M |
| B — sync lane | dismissals document section (codec, clock-LWW union, Rule-ref-only emission filter + fold ignore + property pin), fold at the §5b position (after rules), `UpdateAppData` enqueue site, flush/fence reuse, snapshot/fixture pass | S/M |
| C — notification surface (absorbs plan 5) | in-app notification view (derived-at-read, action buttons → commands), `DismissReminder` UI path, desktop sink (`notify-rust` or platform equivalent), sink withdraw direction + platform matrix, `cargo deny` for the new tree, kiosk in-app question | M |

Stage 2 (design doc + recordings) sized M — the two-fact split, the
Mark Done/reopen handling, and the withdrawal direction are real
design surface.

**R3.5 — Open details for stage 2's decision session (user inputs
wanted, none blocking the design doc):**

1. **Start Task semantics** beyond dismissing: pure UI navigation, or
   also stamp `scheduled` (the startdate field) to now?
2. **Section name**: `dismissals` (recommended) vs keeping `proofs`.
3. **Kiosk in-app notifications**: does the kiosk show the in-app
   surface too (it has the banner already), or banner-only?
4. **OS toast dismissal**: accepted that closing the native toast is
   *not* a dismissal (the in-app notification is the source of truth)
   — confirm.
5. **Mark Done dismissal shape**: planner-emitted dismissal rows
   (recommended; closes the reopen hazard) vs done-derivation coupling
   — stage 2 confirms.

**R3.5 resolutions (2026-10-25, user direction) — the open details are
closed:**

1. **Start Task: deferred to issue #210 (time tracking subsystem).**
   The v1 button set is Dismiss / Mark Done / Snooze / Mute; #210 is
   charged (by issue comment, recorded 2026-10-25) with adding the
   Start Task button to the reminder UI when it lands, carrying the
   implicit-dismissal semantic.
2. **Section name: `dismissals`.** The reserved `proofs` name stays
   reserved-but-unclaimed (documented in the section-registry notes so
   no future wave invents a colliding meaning).
3. **Kiosk: banner-only.** The in-app notification surface is a
   full-GUI feature — it requires interaction to be useful, and
   interactive cases run the full GUI (kiosk appliance mode keeps the
   scheduler-free banner as its only reminder surface).
4. **Native toast close ≠ dismissal: confirmed**, with the rationale
   recorded — several notification systems are fire-and-forget, and
   cross-OS/display-manager/WM support therefore *requires* the fully
   in-app path as the universal dismissal surface (it is the source of
   truth). **Backlog candidate recorded**: where the platform's
   notification system supports action buttons, native toasts may
   grow Dismiss/Snooze/Mute buttons — considered future enhancement,
   gated on case-by-case functional tests per platform (not a v1
   commitment; issue filing left to the user's promotion).
5. **Mark Done: planner-emitted dismissal rows** (the recommended
   shape stands; the reopen hazard stays closed).

### (c) The state home (F16-16) — verification, not invention

**The question.** F16-16: "anything that roams must become engine state
(a new `AppState` section + actions + stamps; a domain/state change
wearing a config costume)". For this round's facts the home **already
exists**: rules, deliveries, snoozes, mutes, alarms all live in
`AppState.reminders` (`state.rs`; `ReminderState`, `reminders.rs:337–354`)
with persistence actions (`persistence.rs:204–225`), stamps
(`ReminderRule.updated_at`; deliveries carry `delivered_at`), and the
fold path (`pipeline.rs:674–709`). No roaming reminder fact needs a new
`AppState` section.

**Candidate options:**

- **(c-1) Record F16-16 as satisfied for the reminder domain** (the
  audit's inventory obligation discharges against the existing
  `ReminderState` home), and note the obligation's remainder — roaming
  *settings* — belongs to whatever round consumes the `settings`
  section, not this one.
- **(c-2) If §4(b) roams mutes (or anything preference-shaped), grow the
  state home accordingly in this round's design** (new stamps for
  mute/unmute writes if mutes need LWW semantics; new fold deltas; a
  storage column/migration) — the honest cost of that branch.

**Evidence.** The audit's own evidence line ("`UserPreferences` grep
empty; AppConfig device-local") concerns *settings*, not reminder
facts; the reminder facts' engine home predates this round and is
snapshot-pinned.

**Stage 2 must decide:** (c-1) vs (c-2) as a consequence of the §4(b)
roam-set; and, if proofs roam, whether the *synced* proof set needs any
state of its own beyond `ReminderState.deliveries` (e.g. a
"proofs-present-in-document" marker for flush decisions — or whether
flush triggering stays out-of-state, §4(d)).

### (d) Merge / outbox interactions — the proofs lane mechanics

**The questions.** How proofs enter and leave the document; what
R1–R9 touch points see; how a *tick-recorded* proof triggers a flush.

**Sub-questions and candidate options:**

1. **Flush triggering for engine-internal proof writes (M4 is the
   gap).** Every `UpdateAppData` enqueue today is command-driven. The
   tick arm's delivery batch has no op, and `has_flush_work`
   (`flush.rs:119–139`) fires only on a queued document group or a
   rebuild trigger. Options:
   - the tick arm's batch also enqueues `UpdateAppData(task)` for each
     task whose reminder just settled (a second enqueue site outside
     `plan_command` — precedented in shape by the fold's action
     emission, but a genuine outbox-contract event: the engine writes
     its own outbox from a non-command path);
   - the flush work-gate grows a proof-dirty input the engine
     publishes (new state, §4(c)'s open tail);
   - proofs ride rebuild-only timing (accept arbitrary delay — likely
     unacceptable for proofs whose whole value is timely suppression).
2. **The proofs section's union primitive.** The document union is
   embedded-clock LWW per key (`appdata.rs:431–488`) — but proofs have
   no per-entry write clock with write-site semantics (M4: the stamp is
   `delivered_at`, a fact stamp, not an LWW clock; set-union needs no
   ordering — `persistence.rs:325–331` is first-write-wins with no
   comparison). Options: a set-shaped section (rows keyed by the
   `(reminder, occurrence)` text-form pair — the `ReminderSnoozeKey`
   string-encoding precedent, `reminders.rs:266–300`) unioned
   additively; or inventing a clock field for uniformity (must then
   define its write sites; risks re-legislating what set-union already
   gives for free).
3. **R1–R9 touch points for the fold.** Proof entries fold additively
   into `ReminderState.deliveries` at the §5b fold position
   (`pipeline.rs:668–709`), feed no presence facts (the D-S3 fence),
   never tombstone (C2), and orphan proofs are inert (M5). Options to
   record: fold-inside-§5b beside the rules fold (symmetric, landed
   shape) vs a separate pass (no evident benefit). The guard question:
   does the D-S6 `reminder_rules` intent flag extend to proofs, or are
   proofs guard-immune (a proof is not an intent — there is no local
   "edit a delivery" verb to protect; the flag protects *rule set*
   edits only today, `merge.rs:430–433`)?
4. **Tombstone/cascade behavior.** Rule removal prunes the rule's
   deliveries locally (`persistence.rs:308–316`) — but a *synced* proof
   for a later-removed rule would re-fold (additive sections have no
   absence-deletion). Options: accept re-adoption (orphan proofs are
   inert, M5; growth bounded by rule lifecycle); give the proofs
   section a card-absence GC story like the other sections'
   `prune_absent_cards` (`appdata.rs:542–565` — note it prunes by
   *card* absence, which addresses rule-less cards but not aged
   proofs on live cards); tie into the backloged age-pruning entry
   with new cross-device terms (a local prune would be re-fed by the
   next fold unless prunes are also synced — which would break the
   no-tombstones property, C2).
5. **Suppression semantics (the UX-visible fork).** A folded proof
   suppresses the local device's firing for that `(rule, occurrence)` —
   that is what "the proofs are designed to be synced" *means*, but it
   is a behavior decision, not a code fact: the alternatives (proofs as
   history only, no suppression; or per-device configuration of the
   policy) are constructible. Offline devices necessarily fire
   independently and converge on reconnect (accepted at-least-once on
   conflict). This deserves an explicit user ruling in stage 2's
   decision session.

**Evidence.** M4, M5, M6; the flush gate's current shape
(`flush.rs:88–139`); the fold's landed position and fences
(`pipeline.rs:668–709`, `merge.rs:420–437`); the union primitive
(`appdata.rs:431–488`); ADR 0007's evidence-then-verdict rule (a failed
cycle folds nothing — proofs included).

**Stage 2 must decide:** each of the five sub-questions, with the
outbox-contract change (if any) spelled out as a named delta to
`plan_command`/the tick batch/`has_flush_work`.

### (e) The ADR 0010 vocabulary fork (F16-08/F16-15) — discharge on the record

**The question.** The audit's fork: "synthesize Deck shapes vs ADR
0010's anticipated port promotion" (F16-08); "sidecar generalization
reuses the Deck client entirely, journal/org connectors face the
vocabulary fork" (F16-15); both carried the action "the D5 round
records branch + vocabulary decision together". This round is that D5
round for the reminder domain.

**Candidate options:**

- **(e-1) No vocabulary movement — recorded.** The reminder lane rides
  the document lane, which reuses the `DeckClient` surface entirely;
  the shared vocabulary (`SyncCommand`/`SyncReport`/`SyncStateReader`,
  ADR 0007) and the `ConnectorModule` spawn trait (recorded in ADR
  0010's connector-port section by the multi-board round, D-W4)
  already admit a second connector without movement. The fork's
  remaining half (journal/org *native-shape* connectors) is recorded
  as un-fired, with the re-fire condition named (a connector that must
  model a domain concept Deck has no shape for).
- **(e-2) Promote the vocabulary now** (a generic port promotion in
  ADR 0010's terms) — costs an ADR amendment and surface churn with no
  consuming connector in view.

**Evidence.** ADR 0010's connector-port section already records the
trait decision; `DeckFeatures`/the document lane are connector-shaped
but Deck-served; the audit's own P3 sizing and "no speculative prep"
pattern (F16-05's action).

**Stage 2 must decide:** (e-1) vs (e-2), and — if (e-1) — whether the
recording lands as an ADR 0010 amendment line, a note in the round's
design doc, or both (the audit asks that the branch + vocabulary
decision be *recorded together*, so the design doc must carry it even
if the ADR stays untouched).

### (f) Migration / compat with existing local-only v1 data

**The question.** What happens to accrued local facts and mixed fleets
when the proofs section starts riding documents.

**Candidate positions (per sub-item):**

- **Local data:** no schema migration is expected for a document-borne
  section (the deliveries/snoozes/mute/alarms tables exist since 0004;
  the rule clock column since 0007). A first flush uploads local
  proof history (overlay assembly from local truth — bounded by table
  size; the rebuild-mode precedent, sidecar §3.4 item 4).
- **Mixed fleets:** a proofs-ignorant device relays the section
  verbatim (M6, landed and tested) but keeps firing on occurrences a
  proofs-aware device already settled — a semantic (not format)
  divergence, invisible to the format-fence warning machinery. Options:
  document it as accepted behavior; or (cheap, honest) include it in
  the per-board sidecar-health surfacing story only if stage 2 finds a
  typed signal — none exists today, and inventing one may not be worth
  it (record either way).
- **Old proofs for rules adopted later:** a document rule adopted by
  fold (`merge.rs:469–479`) may arrive with proof rows already in the
  section — they must suppress the adopted rule's occurrences
  immediately, before the local device fires once. Stage 2 pins this
  as a property (fold order: rules fold before proofs fold within the
  same pass, or proofs fold against post-rules state).

**Evidence.** Migrations 0004/0007/0008; the verbatim-relay test
(`appdata.rs:830–872`); the rebuild flush path (`flush.rs`); the fold's
rules-then-guard ordering (`pipeline.rs:696–707`).

**Stage 2 must decide:** the compat record per sub-item; whether any
*bootstrap-once* behavior is needed for first-enable (expected: none —
state the reasons); the fold-order property's exact statement.

### (g) Offline / kiosk semantics for proofs recorded while disconnected

**The question.** Delivery proofs are created at fire time by the tick
arm — often offline on a kiosk. Define the disconnected lifecycle.

**Candidate positions:**

- **Record-and-forward:** the proof persists at fire time
  (durable-then-notify, ADR 0013 decision 3 — the notification permit
  is the local row); on the next connected cycle it joins the flush
  union. No special offline mode, no queue beyond the outbox mechanics
  (or the §4(d) trigger answer). Offline suppression is impossible by
  definition (no evidence can arrive); two disconnected devices both
  fire the same occurrence and converge — the accepted at-least-once
  window, to be documented (not fixed: cross-device real-time
  suppression would be a different product).
- **Read-only kiosks:** a fold-only connector (D-S8's read-only rule)
  receives proofs and suppresses without ever flushing — confirm the
  asymmetry is desired for the kiosk (it is the cheap-kiosk story: no
  write permission needed to stop double-nagging *if* another device
  maintains the document).
- **Long-disconnect growth:** proofs accrue locally without bound
  during long offline episodes (bounded by firing frequency; the
  backloged age-pruning entry is the relief valve — §4(d).4's terms
  apply).

**Evidence.** ADR 0013's durable-then-notify decision; the parent
design §5.1 ("a lost toast on a dying disk is acceptable…"); D-S8's
read-only fold-only rule; the flush fences (D-S7: failed upload keeps
ops queued; nothing folds from unreadable evidence).

**Stage 2 must decide:** the offline lifecycle record (expected:
record-and-forward with the convergence window documented); the
read-only-kiosk confirmation; whether long-disconnect growth changes
the priority of the backloged age-pruning entry.

## 5. Risks

- **The outbox-contract risk (§4d.1):** letting the tick arm enqueue
  ops is the round's most invasive candidate — it writes the outbox
  from a non-command path for the first time and touches
  `has_flush_work`. Mitigation: stage 2 sizes the alternatives against
  the same correctness bar (proofs that flush late are worthless) and
  names the chosen trigger's test seams explicitly.
- **The suppression-policy risk (§4d.5):** "synced proofs suppress the
  fleet" changes user-visible nagging behavior; guessed wrong, it
  either double-nags (proofs-as-history) or under-nags (a muted
  desktop silences the kiosk). Mitigation: it is a stage-2 decision
  session item with the UX consequences written out per option.
- **The identity-asymmetry risk (§4b):** `ReminderDelivery` split into
  roaming Rule-ref rows and local Default/Adhoc rows invites accidental
  serialization of local TaskIds into the document (M2 violation).
  Mitigation: the chosen shape must fail closed — a type or filter
  that *cannot* emit a Default/Adhoc row into the section, pinned by a
  property test.
- **The scope-creep risk:** the reserved `settings` section name and
  F16-16's wording invite pulling roaming settings into this round.
  Mitigation: §3 fences it; the design doc must repeat the fence.
- **The growth risk (§4d.4):** synced proofs without a GC story grow
  across devices and re-fold after local prunes. Mitigation: stage 2
  must either size the growth honestly (occurrences per task per year
  at kiosk scale) and accept it, or define the prune semantics — no
  silent "we'll add GC later".
- **The audit-discharge risk:** F16-15/F16-08's actions demand a
  recorded decision this round even if the answer is "no movement"; a
  design doc that silently skips them leaves P2 findings open.
  Mitigation: §4(e) is a mandatory section of stage 2's deliverable.

## 6. Estimated round size

**Superseded by §4-R3.4** (the notification-model ruling): stage 2 is
sized M, implementation is three chunks — A (domain/state/storage,
M), B (sync lane, S/M), C (notification surface, M, absorbing the
reminders plan's deferred plan 5). The original single-S/M-chunk
estimate below is kept for the record of what the round looked like
before the dismissal-model direction landed.

- **Stage 2 (design + decision record + ADR-side effects):** S–M. The
  question count is moderate and most substrate constraints are
  binding; the genuinely open forks are §4(b)'s roam-set, §4(d)'s
  trigger + union + policy triple, and §4(e)'s recording.
- **Implementation, if the decisions are the expected ones** (proofs
  ride the document lane; snoozes/mutes/alarms/defaults stay local; no
  capability flag; outbox trigger answered): **one chunk, S/M** —
  proofs section codec + overlay/fold + the trigger answer + tests
  (the codec, fold, flush, and evidence machinery all exist and are
  pattern-complete for a third section; the relay property is already
  pinned).
- **If §4(b) roams mutes or snoozes,** add a state-home + merge-policy
  chunk (M) — that branch also drags F16-16's inventory into the
  round.
- **Charter delta, stated honestly:** issue #202 sized this round as
  "the protocol design round"; roughly half its content (transport,
  rules section, rule clocks, tombstones, the capability seam) landed
  with the daily-driver wave before the round started. Stage 2 should
  note the de-scope on the issue when the round closes — and, per
  R3.2, the *addition* (dismissal sync + notification surface), which
  issue #202 never scoped.

## 7. Stage-2 checklist (the handoff)

1. Answer §4(a)–(g) explicitly, one decision each (including
   "no-action" answers), with the candidate rejected-list per the
   house decision-record style.
2. Produce the design doc (this file's successor, same branch or
   successor branch) + the user decision session for the policy forks
   (§4(b) roam-set, §4(d).5 suppression semantics, §4(a) flag
   reservation).
3. Record the F16-15/F16-08 discharge (§4(e)) — design doc mandatory,
   ADR 0010 amendment if and only if §4(e) selects movement.
4. Record F16-16's disposition (§4(c)).
5. Update issue #202 (comment or body note) with the landed-half
   de-scope (§6) when the round closes — and the R3 addition
   (dismissal sync + the notification surface), which the issue never
   scoped.
6. Amend the age-based-proof-pruning backlog entry's terms if §4(d).4
   changes them (per R3: it now prunes local fire receipts *and*
   synced dismissal rows; the too-old-to-refire term applies to
   both).
7. Spec the ADR 0013 amendment (R3.3): the at-most-once guard splits
   into the local fire receipt and the synced user dismissal;
   durable-then-notify narrows to the OS toast; the interactive
   dismiss/snooze surface note moves from "plan 5" to this round.
8. Run the R3.5 open-details items through the user decision session
   (Start Task semantics, section name, kiosk in-app, OS-toast-close
   semantics, Mark Done dismissal shape).
9. Pin the migration-0009 backfill as a dedicated upgrade test:
   pre-R3 delivery rows must become dismissal rows one-for-one, so no
   already-fired occurrence resurfaces as an active in-app
   notification after upgrade.
