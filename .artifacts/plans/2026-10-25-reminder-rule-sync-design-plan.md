# [Plan] Reminder-Rule & Delivery-Proof Sync — Design (stage 2)

- **Date**: 2026-10-25
- **Kind**: stage 2 of the reminder-rule sync design round — the
  **design doc + decision record**. Resolves every question of the
  stage-1 agenda (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  §4(a)–(g)) against the three user ruling passes recorded there
  (§4(b)-R, §4(b)-R2, §4-R3 + the R3.5 resolutions), which are treated
  as **binding direction**: this doc's job is to give each ruling its
  exact shape, its rejected alternatives, and its consequences — not to
  re-argue it. Zero production code; this file is the chunk's entire
  diff. The PR is stage 3's task.
- **Origin**: GitHub issue #202 (`backlog: Sync of reminder rules and
  delivery proofs`, `wave-daily-driver`, P3) — the reminders design
  round's D-a deferred property
  (`.artifacts/plans/2026-10-12-reminders-design-plan.md` §4.4 D-a,
  §15); audit findings F16-15/16/17 (+F16-08 shared)
  (`.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`).
- **Normative inputs** (all verified against `main` @ `abc5aa7`, the
  stage-1 base; the worktree carries no code deltas):
  - The **binding substrate**: the accepted sidecar design
    `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`
    (D-S1…D-S9; §1 "Binding, not re-arguable") — landed end to end.
  - The **capability rulings**: `2026-10-22-deck-version-capabilities-design-plan.md`
    (D-V1 capability seam, D-V2 dynamic `scheduled` lane).
  - The **transport evidence**: `2026-10-22-deck-1175-transport-characterization-report.md`
    (§3/§4: all seven D1 attachment behaviors identical on Deck
    1.17.5/1.19.0 with the `data` workaround; `deck.version` on the
    capabilities endpoint).
  - **ADRs**: 0004 (merge policy R1–R9 + board amendments), 0007
    (push-first cycle, evidence-then-verdict, self-clobber guard),
    0010 (standalone-first; connector-port section carries the
    `ConnectorModule` trait decision from the multi-board round),
    0013 (reminders; §12 decision 1's sync-readiness clause) — all in
    `docs/adr/`.
  - The code anchors of the stage-1 agenda §2 (re-verified in this
    worktree; cited as `file:line` below).
- **Branch / worktree**: `docs/reminder-rule-sync-design-round` in
  `../taskboard-WT-docs-reminder-rule-sync-design-round`, off `main` @
  `abc5aa7`.
- **Status**: DRAFT for stage-3 review. Every agenda question §4(a)–(g)
  ends in a numbered decision (§4's table); the three R3.5 open
  details that needed user input were closed by the user (agenda
  R3.5 resolutions) and are recorded here as decisions, not re-opened.

---

## 1. Mission & binding constraints

Design the **dismissal sync lane and the notification model** for
reminder rules: the second (and last) reminder fact family that roams,
riding the landed per-board app-data document lane, plus the in-app
notification surface whose *dismissal* is the synced fact. The round's
charter (issue #202) was written as "delivery proofs sync"; the
stage-1 notification-model ruling (§4-R3) superseded the fire-receipt
reading — **fire receipts stay local, the synced fact is the user
dismissal** — and absorbed the reminders plan's deferred "plan 5"
(desktop notification sink + interactive surface) into this round.

Binding, not re-arguable (sidecar design §1; agenda §1):

- One board-anchored document, versioned envelope, section registry;
  unknown sections/keys preserved verbatim (the relay guarantee,
  D-S2, proven at `appdata.rs:832–872`).
- Document keys are `(board, remote card id)`; **local `TaskId`s never
  serialize into the document** — the single exception is
  `ReminderRuleId`, whose UUID *is* the document identity (D-S9,
  `merge.rs:469–479` adopts it verbatim).
- Two disjoint clock domains; document-embedded clocks decide all
  app-data verdicts; the card stamp is never consulted for app data.
- Absence ≠ tombstone; the document never feeds R3 (the D-S3 fence).
- Flush is read-modify-write union after the cycle's card pushes,
  before the report; failed upload keeps ops queued; a failed or
  unreadable download folds nothing (evidence-then-verdict, ADR 0007).
- The v1 fence, amended: **no reminder data enters the *native*
  merge/push path** (`pipeline.rs:853–855`, `pipeline.rs:1059`);
  reminder data rides the document lane only.
- **§15 sync-readiness commitments are preservative** (agenda §2, all
  verified: C1 portable UUID rule ids, C2 occurrence-keyed additive
  rows, C3 the rule write clock, C4 the capability seam, C5 no
  single-device assumption, C6 the per-device record — amended only
  where this round explicitly rules, C7 superseded by the landed lane).
- **F16-16: anything that roams gets an engine-state home with write
  clocks — no config costume.** Every fact this round decides roams
  must live in `AppState`, carry a write stamp, and persist through
  the storage layer (`docs/adr/0013`; agenda §4(c)).
- **Merge-policy conformance**: the design names its R1–R9 touch
  points explicitly (§3.6; ADR 0004).
- **A1 house pattern** for new types (`docs/coding_guidelines.org`
  §2.1): hand-rolled private-field newtypes with fallible minting
  constructors at trust boundaries, documented leniency, no derive
  crates, no validate-and-forget.

## 2. Verified baseline — numbered facts

Every fact carries its source; symbols are the stable keys, lines are
the stage-1 base tree `abc5aa7` (the branch delta is docs-only, and
the rebase onto `main` @ `30381b0` added UI-only commits touching
none of the cited crates).

- **F1 — the document lane is landed end to end** for `tasks` and
  `reminder-rules`: codec + union + GC (`taskboard-domain/src/appdata.rs`),
  anchor ceremony (`taskboard-sync-nextcloud/src/anchor.rs`,
  `actor.rs:591+`), typed `AppDataEvidence` on
  `SyncReport::Completed.app_data` (`messages.rs:485`), the fold
  (`pipeline.rs:668–709`, `merge.rs:314–499`), flush between pull and
  report (`flush.rs:142+`, `actor.rs:629+`), per-board `SidecarHealth`
  (`state.rs:75–97`), outbox op `LocalOp::UpdateAppData(TaskId)`
  (`outbox.rs:59`) enqueued by the rule verbs (`command.rs:1301/1327/1342`)
  and app-field verbs (`command.rs:532`). *(agenda §1; verified)*
- **F2 — `ReminderDelivery` is occurrence-keyed and additive**:
  `{ reminder: ReminderRef, occurrence: DateTime<Utc>, delivered_at }`
  (`reminders.rs:241–249`), a `BTreeSet` in `ReminderState`
  (`reminders.rs:344`); replayed delivery is a no-op
  (`persistence.rs:325–331`); storage PK parity
  `migrations/0004_reminders.sql` `PRIMARY KEY (reminder, occurrence)`
  — the F16-17 verification. *(agenda §2 C2; verified)*
- **F3 — `ReminderRef` has three arms**: `Rule(ReminderRuleId)`,
  `Default(TaskId)`, `Adhoc(AdhocAlarmId)`
  (`reminders.rs:139–147`); default reminders have **no stored rows** —
  their occurrences are computed from the per-device config
  `[reminders] default_before`; alarms are documented device-local
  (`ids.rs:61–63`, `messages.rs:267–275`, `migrations/0008`). *(agenda
  §2 M1/M3, C6; verified)*
- **F4 — the reserved section names relay verbatim today**:
  `proofs` and `settings` land in `sections.unknown` and survive
  decode→encode value-verbatim (`appdata.rs:60–66`, test
  `appdata.rs:832–872`; the union is additive over unknowns,
  `appdata.rs:491`). A proofs/dismissals-ignorant device is therefore
  a transparent relay at the transport level, today. *(agenda §2 M6;
  verified)*
- **F5 — every `UpdateAppData` enqueue today is inside
  `plan_command`'s command handlers** (`command.rs:532, 1301–1342`);
  the tick arm's delivery batch records deliveries with no outbox op;
  the flush work-gate is group- or rebuild-driven only
  (`flush.rs:119–139`). *(agenda §2 M4; verified)*
- **F6 — the self-clobber guard protects local *edits*** via the
  `reminder_rules` intent flag (`merge.rs:430–433`,
  `pipeline.rs` `PendingTaskIntent`); the guard's reason is the
  same-cycle sync must not overwrite a just-made local edit.
  *(agenda §4(d).3; verified)*
- **F7 — the rules fold sits at the §5b position, after the task
  merge loop, guarded, adopting document rule ids and strictly-newer
  tombstone removal** (`merge.rs:437–499`; fold-order pin
  `pipeline.rs:696–707`). Orphan entries for unknown rules are inert:
  `due_firings` consults live rules only (`reminders.rs:596–606`).
  *(agenda §2, M5; verified)*
- **F8 — `DeckFeatures` carries exactly one flag today** and every
  flag is read by a behavior gate (`capabilities.rs:15–40`,
  `messages.rs:485–491`, `state.rs:66`); the table is additive by
  construction, so a future flag row costs nothing later. There is no
  Deck reminders surface to probe. *(agenda §4(a) evidence; verified)*
- **F9 — the transport is characterized on both live Deck tiers**
  (1.17.5 and 1.19.0): all seven D1 attachment behaviors identical
  with the `data` workaround (`2026-10-22-deck-1175-transport-characterization-report.md`
  §3/§4); `deck.version` on the capabilities endpoint is the D-V1
  probe seam. The live-tier gate from sidecar §4 was discharged by
  that report. *(verified)*
- **F10 — the landed model has no user-facing dismissal at all**: the
  engine auto-settles every occurrence at fire time (durable-then-
  notify, ADR 0013 decision 3); a UI dismiss closes the toast only;
  the interactive surface was deferred to the reminders plan's
  "plan 5", never built. *(agenda §4-R3 preamble; verified)*
- **F11 — snooze links replace, they do not accumulate**
  (`persistence.rs:333–341`); mute is a `BTreeSet<TaskId>` that never
  touches the overdue banner (`reminders.rs:347–352`). Both are
  currently per-device: no `LocalOp` arm, no document section.
  *(agenda §2 C6; verified)*
- **F12 — card-absence GC prunes by *card*, in the flush union**
  (`appdata.rs:542–565`, `prune_absent_cards`); there is no synced
  tombstone or GC for additive set rows, and age-based local proof
  pruning exists only as a backlog entry (parent design §15 + D-c).
  *(agenda §2 M5/M12, §4(d).4; verified)*

## 3. The design, point by point

### 3.1 D-1 — Transport: the dismissal section rides the landed document lane (agenda §4(a)(i))

**Options considered.**
(a-1) everything reminder-shaped rides the document lane, the reserved
name is consumed; (b) a per-rule attachment transport — structurally
dead since the roundtrip §4 granularity ruling fixed
one-document-per-board (sidecar §1, binding); (c) a new transport.

**Choice. (a-1), confirmed.** The synced dismissal fact joins the
section registry as **`dismissals`** (D-9 for the name), a third
modeled section beside `tasks` and `reminder-rules`. No new transport,
no new anchor work, no new merge primitive: the codec, anchor
ceremony, typed evidence, fold position, flush union, failure fences,
and verbatim relay all apply unchanged (F1, F4). The `proofs` reserved
name stays **reserved-but-unclaimed**, documented in the
section-registry notes (`appdata.rs:60–66` doc) so no future wave
invents a colliding meaning — the name misdescribes the content under
the R3 model (the synced fact is a user dismissal, not a mechanical
proof).

**Consequences.** Mixed fleets interop at the transport level on day
one (F4, proven by the landed test). The implementation chunk B is
pattern-complete work: a third section is a codec + union + fold
exercise, not a transport project (agenda §6's original sizing
assumption, now applied to dismissals).

### 3.2 D-2 — No capability flag reserved (agenda §4(a)(ii–iii), option a-3)

**Options considered.**
(a-2) reserve a `DeckFeatures` placeholder flag (e.g.
`native_reminders`, default `false`) now; (a-3) no flag, the
capability note recorded in prose.

**Choice. (a-3).** Reasons: (i) F8 — every existing flag is read by a
behavior gate; a placeholder would be the table's first dead flag,
with nothing to probe (no Deck reminders surface exists); (ii) the
table is additive by construction (`capabilities.rs:15–21`), so
deferring costs nothing later; (iii) the audit's "no speculative
prep" stance (F16-05's action pattern). **The trigger is named**: the
day a round designs a native server-side reminder path, that round
adds the flag row, points its probe at the D-V1 seam
(`deck.version`), and fires the ADR 0010 amendment (one sentence in
that round's doc; recorded here so the obligation is findable).

**Consequences.** C4's seam stays general and unused-for-reminders;
nothing in `DeckFeatures` moves; the capability question leaves this
round with a recorded deferral, not an open question.

### 3.3 D-3 — The roam-set (agenda §4(b), ruling R)

Per fact, fixed by the user ruling and re-confirmed here against the
current model (M1/F3 postdates the old fence wording):

| Fact | Roams? | Reasons |
|---|---|---|
| Rule-ref **dismissals** (new fact, D-5) | **Yes** — the round's payload | Rule UUIDs are document-stable identities (C1/D-S9); occurrences are identical across devices for the same rule; suppression is the product. |
| Rule-ref **fire receipts** (`ReminderDelivery`) | **No** (R3.1) | Per-device at-most-once toast guard; every device fires its own notification; syncing receipts under R3 would wrongly suppress other devices' notifications. |
| `Default`-ref rows (receipts and dismissals) | **No** | F3/M2: the document must not carry local `TaskId`s, and M3 makes cross-device Default occurrences meaningless — the per-device `default_before` offset means two devices compute *different occurrences*, so rows could never match even with a remap. The "remap by remote card id" escape hatch is rejected for exactly this reason: identity alignment is impossible, not merely costly. |
| `Adhoc`-ref rows | **No** | Alarms are task-independent — no card key to ride; documented device-local end to end (`ids.rs:61–63`, `messages.rs:267–275`, `migrations/0008`); the record is confirmed, not overturned. |
| **Snoozes** | **No** | Per-device interaction state (D-S9 fence); a snooze is "the *viewer's* later" — a synced chain would re-nag the user on every device they did *not* snooze on; snooze links replace rather than accumulate (F11), so the additive union does not express their merge; a competing-links LWW policy would be new merge-policy surface for no demonstrated need. Revisit trigger: demonstrated need only. |
| **Mutes** | **No** | A mute is "stop nagging *me*"; the kiosk's banner shows reality regardless (F11). A fleet-wide mute is a preference-shaped fact that would drag F16-16's settings inventory into this round (agenda §3 fence). Revisit trigger: demonstrated need. |
| **Default-reminder offset** | **No** | Stays per-device config (`[reminders] default_before`); roaming it would be F16-16's config-costume violation in reverse — config pretending to be syncable state. |

**Consequences accepted with the ruling** (recorded, user-visible):
(i) a snooze on one device does not protect the others — they fire the
original occurrence (the documented single-device noise mode,
per-device); (ii) `ReminderDelivery` gains a roaming sibling (D-5),
creating a two-fact split instead of one asymmetric type — cleaner
than the section-level-filter-on-`ReminderDelivery` alternative
because the local fact keeps its shape untouched (snapshot-stable) and
the synced fact gets its own write sites and stamps (F16-16-honest);
(iii) `ReminderState` remains the single engine-state home for both
facts — no config costume (F16-16), no new `AppState` section (D-8).

### 3.4 D-4 — The asymmetry is type-level, fail-closed (agenda §4(b) sub-item)

**Options considered.**
(i) type-level — two kinds of rows (`ReminderDelivery` local,
`ReminderDismissal` synced, Rule-ref only); (ii) section-level — one
type, the section's serializer filters to Rule-ref rows;
(iii) recorded-doc-only — one type, a serialization filter with no
type distinction.

**Choice. (i) + the R2 filter, belt and braces.** R2 ruled the
*section-level fail-closed filter*; D-3's two-fact split makes it
type-level *as well*: the synced fact `ReminderDismissal` carries
`reminder: ReminderRuleId` — **not** a `ReminderRef` — so a
`Default`/`Adhoc` row is unrepresentable in the synced type; the
section's writer emits only `ReminderDismissal`s; the fold ignores any
`default:`/`alarm:`-keyed entry it might nevertheless encounter
(defensive, for hand-edited documents). The naive alternative —
serializing `ReminderDelivery` rows filtered at the wire — is
rejected: F3 shows the type alone cannot prevent the leak
(`ReminderRef::Default(TaskId)` serializes the local task UUID), and
M2 makes that leak a binding violation, not a cosmetic one.

**Consequences.** The property pin (§5, P2) has a type-level guarantee
to lean on; the fold's ignore-filter is a second line of defense, not
the only one. `ReminderState` grows a `dismissals` collection; the
local `deliveries` set is untouched (no snapshot churn beyond the one
additive field).

### 3.5 D-5 — The two-fact split: local fire receipt, synced dismissal (agenda §4-R3.1/R3.2)

**Options considered.**
(i) sync fire receipts (the pre-R3 reading — one fact, suppress
fleet-wide at fire time); (ii) two facts — the local receipt stays,
a new synced user-dismissal fact roams; (iii) one fact synced with a
per-device "shown" mask — no such mask exists, and building one to
avoid a second table is the config-costume pattern F16-16 forbids.

**Choice. (ii), per R3.** The user-facing semantics (agenda §4-R3
quote): every device shows the notification for an occurrence; the
user's Dismiss (or implicit dismiss via Mark Done) clears it
everywhere. Therefore:

- **Local fire receipt** — `ReminderDelivery`, unchanged role, written
  automatically at fire time (durable-then-notify survives; it now
  guards only the OS toast on this device). **Stays out of the
  document entirely.**
- **Synced dismissal** — new domain fact:
  `{ reminder: ReminderRuleId, occurrence: DateTime<Utc>,
     dismissed_at: DateTime<Utc> }` — `dismissed_at` *is* the LWW
  write clock (D-7), stamped at the single write site (the dismiss
  command). Set-shaped membership; explicit tombstones never needed (a
  dismissal is permanent for its occurrence — there is no
  un-dismiss). Home: `ReminderState.dismissals` (additive; one
  deliberate snapshot regeneration — agenda §7 DF-9 rule), new
  persistence action `RecordReminderDismissal`, new storage table in
  migration 0009, additive, with PK parity
  `(reminder, occurrence)` mirroring 0004's deliveries table (C2's
  parity discipline, F2).

**Consequences.** The at-most-once guarantee splits into two
guarantees with two owners (local toast: receipt; fleet-wide
notification: dismissal) — ADR 0013's decisions 2–3 are amended
accordingly (R3.3, §3.12). The evaluator gains one OR-term and one
derived view (D-6). Storage grows one table and one backfill (D-11).

### 3.6 D-6 — Evaluator: suppression + the derived notification view (agenda §4-R3.2)

**Choice.** Two pure-domain changes:

- `due_firings` suppresses an occurrence that is delivered (local
  receipt) **or dismissed** (synced in) — the dismissal stops future
  firings fleet-wide, including on a device that was offline at fire
  time (it fires on reconnect via the normal tick, unless the synced
  dismissal beat it there; that race resolves correctly by derivation,
  since both facts are persisted before evaluation).
- New pure derived view `active_notifications(tasks, reminders,
  defaults, now)` — the in-app surface reads it exactly like
  `overdue_tasks` (dumb-presentation rule, AGENTS §1): active ⇔ this
  device fired the occurrence (local receipt exists) ∧ not dismissed.
  No notification state is stored *for the surface* — it is derived at
  read, so nothing can drift.

**R1–R9 touch points** (ADR 0004 conformance, named): the dismissal
lane touches **none of R1–R9** — it rides the document lane, which
itself touches none (sidecar §3.3/D-S3: the fold contributes no
presence facts, never feeds R3; absence ≠ tombstone; the document
never carries native-field verdicts). The R1–R9-relevant statement
this round records is the *negative*: no dismissal row is ever an
input to `merge_task`, push planning, tombstoning, or echo handling.
The only policies the lane *reuses* are the landed app-data fold
disciplines (embedded-clock LWW, guard mirror, evidence gates) —
D-S3/D-S6/D-S7 verbatim.

**Consequences.** Snapshot tests gain the `dismissals` section and the
new view; the tick's firing path is unchanged except the suppression
OR-term.

### 3.7 D-7 — Clock discipline: LWW on `dismissed_at`, uniform (agenda §4(d).2, R2 ruling)

**Options considered.**
(i) a set-shaped section with pure additive union, no ordering (the
"natural" set-union; nothing to order); (ii) embedded-clock LWW per
key, uniform with every other document section.

**Choice. (ii), per the user ruling** ("every sync primitive uses the
same functionality"). Sound because a dismissal has **exactly one
write site**: the dismiss command stamps `dismissed_at`; there is no
edit verb, no un-dismiss, so the clock never has competing writers —
LWW degenerates to set-union in practice while keeping the machinery
identical (the union's embedded-clock path applies unchanged; no
special-cased set primitive exists). The proptest laws (§5, P1) pin
both the LWW contract and the degenerate-set property (union is
idempotent, commutative, additive — conflict-free by construction, C2
preserved for the new fact).

**Consequences.** Reuses `appdata.rs`'s union verbatim; the property
suite is a parameterization of the existing document-union laws; no
new merge code path beyond the section's entry type.

### 3.8 D-8 — The state home & flush trigger (agenda §4(c), §4(d).1-as-superseded)

**Options considered (state home).**
(c-1) record F16-16 satisfied against the existing `ReminderState`
home; (c-2) grow a new `AppState` section.

**Choice. (c-1).** Everything that roams — rules (landed) and
dismissals (this round) — lives in `AppState.reminders`
(`ReminderState`, F2/F7) with write stamps (`ReminderRule.updated_at`;
`dismissed_at`) and persistence actions. No roaming reminder fact
needs a new `AppState` section. F16-16's remainder — roaming
*settings* — belongs to whatever round consumes the `settings`
section and stays **fenced out** here (agenda §3). The disposition is
recorded: **F16-16 discharged for the reminder domain**.

**Options considered (flush trigger).**
(i) R2's tick-arm enqueue (the tick batch emits `UpdateAppData` for
each settled task); (ii) a proof-dirty input to `has_flush_work`
(new engine-published state); (iii) rebuild-only timing (arbitrary
delay); (iv) — under R3 — the dismissal *command* enqueues the
ordinary op inside `plan_command`.

**Choice. (iv), per R3.1 supersession.** The synced fact's only write
site is a user command, so the flush trigger is the landed one:
`StateCommand::DismissReminder` (D-10) enqueues
`LocalOp::UpdateAppData(task)` inside `plan_command` exactly like
every other command (`command.rs:532` pattern). No tick-arm enqueue,
no outbox-contract change, no new gate state, no second trigger
mechanism — F5's "gap" (engine-internal writes with no op) simply
stops existing, because the fact that syncs is no longer
engine-internal. Receipts stay out of the document, so their lack of
an op is correct, not a gap. The planner-emitted Mark Done dismissals
(D-10) are emitted by the command planner *within `plan_command`'s
return* — still a command path, still one enqueue site family.

**Consequences.** `has_flush_work` untouched; the outbox contract
grows zero arms and zero non-command enqueue sites (the agenda's §5
"outbox-contract risk" is dissolved by the R3 model, and this doc
records that explicitly).

### 3.9 D-9 — Section name `dismissals`; fold position; guard-immunity; growth (agenda §4(d).3/.4, R2/R3.5)

- **Name: `dismissals`** (R3.5.2). The reserved `proofs` name stays
  reserved-but-unclaimed, documented in the section-registry notes.
- **Fold position: the landed §5b position, after the rules fold**
  (evidence-decided, agenda §4(d).3): one pass over `dismissals`
  beside `fold_reminder_rules`, updating `AppState.reminders` —
  giving the **fold-order property** (agenda §4(f)): *rules fold
  before dismissals within the same pass, so an adopted rule is
  immediately suppressed by already-present dismissal rows* — a rule
  adopted at cycle N cannot fire an occurrence another device dismissed
  before this device ever saw the rule. Pinned as a property (§5, P4).
- **Guard-immune**: the D-S6 `reminder_rules` intent flag does **not**
  extend to dismissals (F6: the guard protects local *edits* a
  same-cycle sync must not overwrite; a dismissal is an append-only
  fact, never edited — there is no local "edit a dismissal" verb to
  protect). The guard's scope stays exactly what it protects today.
- **Growth: accepted, no synced GC, no synced tombstones**
  (R2, §4(d).4): one tiny row per dismissed occurrence, trivial at
  kiosk scale. Cleanup stays with the backloged local age-pruning
  entry, whose terms this round amends: it prunes local fire receipts
  **and** synced dismissal rows, only when too old to refire (agenda
  §7 item 6 — and note: local pruning of *dismissal* rows is safe
  under the additive union only because a pruned row can re-fold from
  the document; pruning is a local-memory optimization, never a sync
  semantic — which is exactly why no prune is ever synced, preserving
  C2's no-tombstones property). Orphan dismissals for unknown/deleted
  rules are inert (F7: evaluation consults live rules only) and
  harmless to re-fold; card-absence GC prunes their entries when the
  card goes (F12).

### 3.10 D-10 — The dismiss surface: command, Mark Done, Start Task, Snooze/Mute (agenda §4-R3.2 + R3.5)

- **`StateCommand::DismissReminder { reminder: ReminderRuleId,
  occurrence: DateTime<Utc> }`** — stamps `dismissed_at = now`,
  persists via `RecordReminderDismissal`, enqueues `UpdateAppData(task)`
  (D-8). The UI's **Dismiss** button dispatches it.
- **Mark Done** sends the existing `SetTaskDone`; the command planner
  **also emits dismissal rows** for the task's currently-active
  occurrences (R3.5.5, the recommended shape confirmed). The done
  state syncs natively and derivation would clear the notifications
  anyway, but the explicit rows close the **reopen hazard**: a task
  reopened after Done must not resurrect old occurrences as active
  notifications (a derived-only policy would couple notification
  liveness to done-state history, which syncs on a different lane with
  different clocks). Rejected: done-derivation coupling.
- **Start Task**: **deferred to issue #210** (the time-tracking
  subsystem), which is charged (issue comment, 2026-10-25) with adding
  the button and carrying the implicit-dismissal semantic. The v1
  button set is **Dismiss / Mark Done / Snooze / Mute**.
- **Snooze / Mute**: dispatch the existing `SnoozeReminder` /
  `SetTaskRemindersMuted` commands unchanged — local operations
  (D-3), revisited only on demonstrated need.
- **Withdrawal (best-effort, both directions)**: the notification sink
  port grows a withdraw direction (the engine currently only sends
  *out*); fire/withdraw share the notifier channel as a two-variant
  payload. The in-app notification is the **source of truth**;
  OS withdrawal is best-effort (the desktop sink withdraws per
  platform support — documented matrix; several notification servers
  cannot withdraw). Closing an OS toast natively is **not** a
  dismissal (R3.5.4 confirmed: several notification systems are
  fire-and-forget, so cross-OS support *requires* the fully in-app
  path as the universal dismissal surface). Native toast action
  buttons are a recorded backlog candidate (issue #444).
- **Kiosk: banner-only** (R3.5.3) — the in-app notification surface is
  a full-GUI feature (it requires interaction to be useful);
  interactive cases run the full GUI; kiosk appliance mode keeps the
  scheduler-free banner as its only reminder surface.
- **Upgrade compat / migration 0009**: pre-R3 data has delivery rows
  only; without a backfill every already-fired occurrence would
  surface as an active in-app notification after upgrade. Migration
  0009 backfills **one dismissal per existing `reminder_deliveries`
  row** (`dismissed_at = delivered_at`), pinning "old behavior ≡
  everything already dismissed" — and per D-3/D-5's type split, the
  backfill converts only Rule-ref rows into synced dismissals;
  `Default`/`Adhoc` receipts get no synced row (correct: their
  occurrences are per-device, F3). Pinned as a dedicated upgrade test
  (§5, T5). Mixed fleets: an old (pre-R3) device auto-settles
  occurrences, never shows or clears the new in-app surface, and
  relays the `dismissals` section verbatim (F4) — accepted, recorded.

### 3.11 D-11 — Storage & domain deltas, A1-conformant (implementation shape)

New types follow the A1 house pattern (`docs/coding_guidelines.org`
§2.1):

- `ReminderDismissal` — a plain domain struct beside
  `ReminderDelivery` (F2's shape discipline: `Copy`, `Eq`, `Ord`,
  serde; the `(reminder, occurrence)` key pair is the identity,
  `dismissed_at` the clock). The Rule-ref-only constraint is enforced
  by taking `reminder: ReminderRuleId` (D-4) — the type-level mint;
  no derive crates, no validation-after-construction, no leniency
  needed (the type has no illegal values to permit).
- `ReminderState.dismissals: BTreeSet<ReminderDismissal>` — additive,
  one deliberate snapshot regeneration.
- Persistence action `RecordReminderDismissal` — first-write-wins per
  `(reminder, occurrence)` (mirroring `RecordReminderDelivery`,
  `persistence.rs:325–331`); `apply_actions` enforces ≤ 1 row per key,
  pinned by property.
- Migration 0009 — the `reminder_dismissals` table (PK
  `(reminder, occurrence)`, `dismissed_at` column) + the delivery
  backfill (D-10). Additive; no column changes to existing tables.
- `DismissReminder` in `StateCommand` + the planner-emitted dismissals
  on `SetTaskDone` (D-10).
- Sync-side: `DismissalsEntry`/section codec in `appdata.rs`
  (string-encoded `(rule-uuid, rfc3339-occurrence)` keys — the
  `ReminderSnoozeKey` string-encoding precedent), the emission filter
  (Rule-ref only, fail-closed by type), the fold's
  unknown-arm-ignore, the `UpdateAppData` enqueue in the dismiss
  handler.

Chunk mapping (agenda §4-R3.4, unchanged): **A** domain/state/storage
(M), **B** sync lane (S/M), **C** notification surface (M, absorbing
plan 5). §5 maps the tests per chunk.

### 3.12 D-12 — ADR/docs amendments (R3.3 + agenda §7)

- **ADR 0013** (amended by the implementation round, spec'd here): the
  at-most-once guard splits into the local fire receipt (OS toast,
  per device) and the synced user dismissal (notification, fleet);
  durable-then-notify narrows to the OS toast; the interactive
  dismiss/snooze surface note moves from the reminders plan's
  "plan 5" into this round (chunk C).
- **ADR 0013 §12 decision 1** sync-readiness clause: consumed in
  full — C1–C7 all hold or are explicitly superseded (§2 of the
  agenda, restated in this doc's §1).
- **ADR 0010**: **no amendment** — D-13 records no vocabulary
  movement; the amendment fires only with a native-path design (D-2's
  named trigger).
- **Section-registry notes** (`appdata.rs:60–66` doc): `dismissals`
  documented as modeled; `proofs` documented as
  reserved-but-unclaimed.
- **Issue #202** (stage 3, on round close): record the landed-half
  de-scope (transport/rules/clocks/tombstones/seam landed with the
  daily-driver wave) and the R3 addition (dismissal sync + the
  notification surface), which the issue never scoped.

### 3.13 D-13 — The ADR 0010 vocabulary fork: no movement, recorded (agenda §4(e), F16-15/F16-08 discharge)

**Options considered.**
(e-1) no vocabulary movement, recorded; (e-2) promote the generic
connector vocabulary now.

**Choice. (e-1).** The reminder lane rides the document lane, which
reuses the `DeckClient` surface entirely; the shared vocabulary
(`SyncCommand`/`SyncReport`/`SyncStateReader`, ADR 0007) and the
`ConnectorModule` spawn trait (recorded in ADR 0010's connector-port
section) already admit a second connector without movement. The
fork's remaining half — journal/org *native-shape* connectors — is
recorded as **un-fired**, with the re-fire condition named: *a
connector that must model a domain concept Deck has no shape for*
(F16-08's fork wording). This is the F16-15/F16-08 discharge on the
record: branch (document-lane sidecar generalization, Deck client
reused) + vocabulary decision (no movement) recorded together, in this
section, per the audit's action. Recording form: this doc only — no
ADR 0010 amendment (the audit demands the record, not the ADR churn;
e-2 rejected as speculative prep with no consuming connector in view).

## 4. Decision record

| ID | Point | Decision (one line) |
|---|---|---|
| D-1 | Transport (§4a.i) | Dismissals ride the landed document lane as the `dismissals` section; `proofs` stays reserved-but-unclaimed |
| D-2 | Capability flag (§4a.ii–iii) | No flag reserved (a-3); ADR 0010 amendment fires the day a native path is designed |
| D-3 | Roam-set (§4b) | Rule-ref dismissals roam; fire receipts, Default/Adhoc rows, snoozes, mutes, alarms, default offset stay device-local |
| D-4 | Asymmetry shape (§4b) | Type-level + fail-closed filter: synced type carries `ReminderRuleId`; fold ignores `default:`/`alarm:` keys; property pin |
| D-5 | Two-fact split (R3.1/R3.2) | `ReminderDelivery` stays local; new synced `ReminderDismissal {rule, occurrence, dismissed_at}`; ADR 0013 amended |
| D-6 | Evaluator (R3.2) | `due_firings` suppresses delivered ∨ dismissed; pure `active_notifications` view; zero R1–R9 touch points (recorded negative) |
| D-7 | Union primitive (§4d.2) | Embedded-clock LWW on `dismissed_at`, uniform with all sections; degenerates to set-union (single write site) |
| D-8 | State home + flush trigger (§4c, §4d.1) | F16-16 discharged against `ReminderState`; dismissal *command* enqueues `UpdateAppData` in `plan_command`; outbox contract untouched |
| D-9 | Fold mechanics (§4d.3/.4) | §5b fold after rules (adopted rule immediately suppressed); guard-immune; growth accepted; age-prune backlog entry re-terms; no synced GC/tombstones |
| D-10 | Dismiss surface (R3.2 + R3.5) | `DismissReminder` command; Mark Done planner-emits dismissals (reopen hazard closed); Start Task → #210; Snooze/Mute local; kiosk banner-only; OS-toast close ≠ dismissal; migration 0009 backfill |
| D-11 | Implementation shape | A1-conformant types; `ReminderState.dismissals`; `RecordReminderDismissal`; migration 0009; chunks A/B/C per R3.4 |
| D-12 | ADR/docs | ADR 0013 amendment spec'd; ADR 0010 untouched; registry notes updated; issue #202 de-scope comment at close |
| D-13 | Vocabulary fork (§4e) | No movement; branch + vocabulary recorded together here; re-fire condition named |

Every agenda question §4(a)–(g) resolves: (a)→D-1/D-2, (b)→D-3/D-4,
(c)→D-8 (first half), (d)→D-7/D-8 (second half)/D-9, (e)→D-13,
(f)→D-9 (fold-order property) + D-5/D-10 (compat: none needed beyond
0009 — a document-borne section needs no local schema migration; the
first flush uploads local backfilled history via the rebuild-mode
precedent), (g)→D-14 below (recorded here rather than in the table
because it is a lifecycle record, not a fork):

**D-14 — Offline / kiosk lifecycle (agenda §4(g))**: record-and-forward
— a dismissal persists at dismiss time and joins the next connected
cycle's flush; no special offline mode, no queue beyond the outbox.
Offline devices fire independently (no evidence can arrive while
disconnected) and converge on reconnect — the accepted at-least-once
window, documented, not fixed (cross-device real-time suppression
would be a different product). Read-only kiosks fold dismissals and
suppress without ever flushing (D-S8's fold-only rule — the
cheap-kiosk story: another device maintains the document). Failed
cycles fold nothing (ADR 0007 evidence-then-verdict — dismissals
included). Long-disconnect growth is bounded by dismissal frequency
and relieved by the re-termed age-pruning entry (D-9); it does not
raise that entry's priority.

## 5. Testing strategy

Chunk-mapped, per the house directives (AGENTS §5: no text
assertions, deterministic by construction, fakes over mocks,
one-behavior-one-test at the lowest layer):

**Property tests (chunk A/B, `proptest`) — the set-union laws:**

- **P1 — union laws for the dismissals section**: for arbitrary
  document pairs, union is commutative, associative, idempotent, and
  additive over unknown content; per key the max-`dismissed_at` entry
  wins and clocks never decrease (the existing document-union law
  suite parameterized over the third section). Conflict-free by
  construction (C2's cross-device half, now pinned for the shipped
  fact).
- **P2 — the no-local-ids property**: for arbitrary `ReminderState`
  (including `Default`/`Adhoc` receipts, snoozes, alarms), the
  serialized `dismissals` section contains no local `TaskId` and no
  `AdhocAlarmId` text form — the D-4 fail-closed pin (type-level +
  filter, both exercised).
- **P3 — replay idempotence**: re-applying a `RecordReminderDismissal`
  for an existing `(reminder, occurrence)` is a no-op (≤ 1 row per
  key), mirroring `persistence.rs:325–331`'s delivery pin.
- **P4 — fold-order**: for arbitrary documents where a rule and its
  dismissals arrive in the same pass, the adopted rule fires no
  occurrence that a folded dismissal already covers (rules fold
  before dismissals, D-9).

**Snapshot tests (`insta`, chunk A):** the `ReminderState` growth and
the `active_notifications` view — feed command sequences (fire →
dismiss → reopen-after-done) and snapshot the resulting `AppState`
tree; the one deliberate regeneration pass covers the additive
`dismissals` field.

**Tier strategy for the attachment transport (chunk B):** the
transport is already characterized on both live Deck tiers (F9) and
the dismissals section rides it unchanged — **no new live-tier probe
is required**. Chunk B's transport coverage is: (i) the verbatim-relay
test extended to a mixed fleet where a dismissals-ignorant device
forwards the section (the landed `appdata.rs:832–872` test gains the
third section); (ii) the failure-fence matrix re-run for the section
(decode failure, format-too-new, foreign board → fold nothing) as
unit tests against typed evidence, per D-S7; (iii) the
mixed-fleet/upgrade semantics (pre-R3 device relays, never clears)
covered by the migration backfill test, not by transport tests.

**Chunk A/B boundary:** storage backfill test (T5): pre-R3
`reminder_deliveries` rows become `reminder_dismissals` rows
one-for-one (Rule-ref rows), so no already-fired occurrence resurfaces
as active after upgrade (agenda §7 item 9). Chunk C: the sink
withdraw contract is a fake-based port test (`LogSink` withdraw =
no-op; the desktop sink's platform matrix is documentation, tested
only as the typed no-op-on-unsupported path).

## 6. Risks

- **The leak risk (D-4)** — a local `TaskId` reaching the wire is the
  binding M2 violation. Mitigated at the type (`ReminderRuleId`, not
  `ReminderRef`), at the filter, and by P2. Residual: a future
  refactor widening the synced type — P2 is the tripwire.
- **The reopen hazard (D-10)** — Mark Done dismissals are planner-
  emitted; if the planner misses an active occurrence, a stale
  notification survives Done→reopen. Mitigated by deriving "currently
  active" from the same `active_notifications` view the UI reads (one
  definition, two consumers) and snapshot coverage of the
  done→reopen sequence.
- **The mixed-fleet divergence (D-10/D-14)** — a pre-R3 device never
  clears the new in-app surface (it has none) and keeps firing after
  another device dismissed. Accepted and recorded (R3.2); invisible
  to the format-fence machinery (semantic, not format divergence —
  agenda §4(f)); no typed signal invented for it (recorded: inventing
  one is not worth it).
- **The growth risk (D-9)** — dismissals accrue without synced GC.
  Sized honestly: one row per dismissed occurrence, trivial at kiosk
  scale; the relief valve is the local age-prune (safe because prunes
  are local-memory optimizations that the next fold may legally
  re-feed — never a sync semantic). No silent "GC later".
- **The audit-discharge risk** — F16-15/F16-08 demand a recorded
  decision even when the answer is "no movement": D-13 is that
  record; skipping it would leave P2 findings open.
- **The scope-creep risk** — the `settings` section and F16-16's
  settings inventory stay fenced out (agenda §3, repeated in D-8).

## 7. References

- Stage-1 agenda (the question catalogue; rulings R/R2/R3 + R3.5):
  `.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`.
- Sidecar design (BINDING substrate): `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`.
- Capability rulings: `.artifacts/plans/2026-10-22-deck-version-capabilities-design-plan.md`.
- Transport characterization: `.artifacts/reports/2026-10-22-deck-1175-transport-characterization-report.md`.
- Audit findings: `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md` (F16-08/15/16/17).
- Reminders design (parent): `.artifacts/plans/2026-10-12-reminders-design-plan.md` (§4.4 D-a, §15).
- ADRs 0004, 0007, 0010, 0013 (`docs/adr/`).
- Issue #202; issue #210 (Start Task deferral); issue #444 (native
  toast action buttons).
- Code anchors @ `abc5aa7` (§2's facts; symbols are the stable keys).
