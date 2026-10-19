# [Plan] Daily-Driver Wave — Design Round & Scope (framing)

- **Date**: 2026-10-18
- **Kind**: design-round framing plan for the `wave-daily-driver`
  milestone. It records the wave scope and the wave-level decisions
  taken at the user review (2026-10-18, §4), states the Stage-0
  design questions the round must settle (§5), and sketches the
  implementation chunking that the round's output feeds (§6). It is
  not itself the detailed design; the round's design docs supersede
  it question by question, then the wave implementation plan and
  dispatch plan follow the multi-board pattern.
- **Origin**: user request 2026-10-18 (set up wave 1 of the
  future-directions roadmap while the multi-board implementation
  round gets under way), plus:
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (§6 wave 1; §3.1/§3.2/§3.4/§3.7 items; §4 D1 + D4),
  `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  (the accepted D1 answer — board-anchored attachment sidecar; its
  §5 merge implications and §6 per-field verdicts are normative
  inputs), the 13 milestone issues (§9), the reminders design plan
  (2026-10-12), the GUI design requirements plan (2026-10-14), the
  ADHD support research report (2026-10-14), and the accepted
  multi-board design
  (`.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`)
  as the integration backdrop this wave lands on.
- **Status**: scope and wave-level decisions **accepted** (§4,
  applied tracker-side 2026-10-18); Stage-0 design questions (§5)
  **open** — the round dispatched from this framing resolves them;
  the user's decision session then rules the recommendations before
  the wave implementation plan is written. Code anchors verified on
  `main` @ `07088ee` (post remediation-wave-1; multi-board
  implementation dispatch PR #343 open at framing time).

---

## 1. Mission

The "daily driver" wave (future-directions wave 1, v0.1.x line): land
the additive app-only field family **together with its sync vehicle**
(the Deck attachment sidecar — fields sync day one, no local-only
period, no data migration), plus the daily-value features that make
the desktop app replace other tools — N-reminder surfaces, standalone
timers & alarms, reminder toast delivery, today view, inbox capture,
fuzzy search, the day-boundary preference — and run the undo
architecture design round (D4, design only). Everything lands on the
post-multi-board world (N boards × N connectors, per-board status,
connector registry).

## 2. Scope (user-confirmed 2026-10-18)

**In** — the future-directions §6 wave-1 list, plus the user's
explicit addition of the sidecar implementation (the roundtrip
report's own Wave-1 consequence note anticipated this):

- **App-data sidecar** (#207): implement the D1 recommendation —
  board-anchored JSON document as a Deck file attachment on a
  designated anchor card, per-key embedded clocks, set-union merge
  with explicit tombstones, transport-not-truth. The
  **reminder-rules** half of #202 rides the document day one
  (roundtrip §6; comment recorded on the issue).
- **Field family**: scheduled-start split (#208), effort +
  importance/urgency (#209) with list decorations, and the per-field
  lockstep seam (#244 — the audit finding that names this wave).
- **Reminders surfaces**: N-reminder CLI/UI (#204), reminder toast
  sink (#344, new).
- **Standalone timers & alarms** (#211).
- **Day-boundary preference / UserPreferences home** (#345, new).
- **Views & finding**: today view (#346, new), inbox stack + quick
  capture (#347, new), fuzzy search `tasks find` + palette (#348,
  new).
- **Undo architecture design round** (#349, new; D4) — design only.

**Out** (later waves / other homes — restated so nobody re-derives):
task-kind & respite flags (Wave 2, time tracking — the sidecar
*schema* reserves the section space but no field lands); reminder
delivery-proofs sync and settings sync (the proofs half of #202; D5
substrate, Wave 4); recurrence parameters (Wave 3, D3); subtasks
(own design round); reminder insistence levels (#203 — needs this
wave's toast sink, lands after); undo *implementation* (#176 stays
unscheduled); importance-urgency matrix *view*, organiser view,
decision support, PDF export (Wave 2–3 views); spell checker, org
markup fields (own homes); effort-calibration feedback loop (#214).

## 3. Baseline — why the data plane is ready (verified on `main` @ `07088ee`)

- **The sidecar's transport already shipped**: `DeckClient` carries
  the full attachment content surface (`upload_attachment`,
  `update_attachment_content` via the app-level POST route,
  `delete_attachment`, `restore_attachment`, `download_attachment`;
  `client.rs`), landed as the parallel-during-gui chunk P2
  (`.artifacts/plans/2026-10-15-sync-nextcloud-attachment-content-plan.md`)
  with the D1 wire traps contract-pinned (multipart part names, 409
  duplicate rule, soft delete, no HTTP validators on content). What
  remains is everything *above* the client: anchor ceremony,
  document schema, merge, flush integration, outbox op.
- **The field-family lockstep structures** (#244): `Task`
  (`entities.rs:164`) + `TaskClocks` (`entities.rs:49`, eight
  clocks) + `TaskChanges`/`PendingTaskIntent` (`pipeline.rs`) —
  three plain structs that must grow in lockstep per field; nothing
  forces the agreement today. No scheduled/effort/importance/
  urgency field exists anywhere yet.
- **Deck's native `startdate` is unmapped** in the sync model
  (`model.rs` has no start date field) — the "free native slot vs
  sidecar entry" question for scheduled-start (roundtrip §6) is
  genuinely open and is this round's to answer.
- **Reminders subsystem state**: domain `reminders.rs` (N rules per
  task, set-union-mergeable delivery proofs, occurrence/snooze/mute
  pipeline); app `notifications.rs` — `NotificationSink` port with
  `LogSink` v1 only (the toast sink widens this port's
  implementations); CLI `SetReminderRule` (replace-with-one) +
  `ClearReminderRules` (`commands/reminders.rs`); `[reminders]`
  config section (`tick_secs`, `default_before`, `enabled`).
- **UI surface**: full action layer (71 `ActionId` references,
  `actions.rs`), `views/` + `compositions/` + `keymap/` +
  `view_state.rs`/`router.rs` — today view, inbox triage, and the
  search palette extend established seams; GUI stack-create (PR
  #334) is the structure-editing precedent.
- **Outbox/push**: `LocalOp` is id-scoped and field-less
  (`outbox.rs`); every Deck write is full-send — the sidecar push op
  follows the same shape (roundtrip §5).
- **Config**: `[ui]`/`[keymap]`/`[reminders]`/`[sync]`/
  `[nextcloud]`/`[storage]` sections exist; no preferences shape —
  the `UserPreferences` resurrection question (Phase-1 plan deferred
  it) is open.
- **Wave context**: remediation-wave-1 closed (39/39). The
  multi-board implementation round is dispatching now (PR #343) —
  its chunks 1–4 rewrite the very files this wave extends
  (entities, command vocabulary, outbox, sync actor, storage codec);
  see §7. Kiosk wave 1 is READY-unstarted
  (`.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md`) with
  minimal overlap (ui shell/window profile vs this wave's new
  views).

## 4. Wave-level decision record (user review, 2026-10-18)

| ID | Decision |
|---|---|
| D-D1 | Milestone **`wave-daily-driver`** created (named for the wave's mission, not its roadmap index — `wave-1` is already taken by `remediation-wave-1`). Roster: 13 issues (§9). |
| D-D2 | #202 is milestone-assigned for its **rules half** (rides the sidecar day one per roundtrip §6); the **proofs half** stays with the D5 wave. Scope comment recorded on the issue. |
| D-D3 | #176 (undo/redo) left **unscheduled**; the in-wave scope is carried by the new scoped issue #349 (design round only) — mirrors the #212/#213 design-round-issue precedent. |
| D-D4 | The **sidecar implementation is in-wave** (user addition beyond the future-directions wave-1 summary line; matches roundtrip §4's Wave-1 consequence note). |
| D-D5 | The **design round starts now**, in parallel with the in-flight multi-board implementation round (docs-only, no code conflicts; D-W8 precedent). The **implementation dispatch** waits for the multi-board chunks to land (§7). |
| D-D6 | Open for the user's decision session after the round: the design docs' recommendations — notably the native-`startdate` ruling, the anchor-ceremony and document-schema points, the toast-sink dependency choice (stack approval), and the D4 verdict. |

## 5. Stage-0 design questions (the round's work)

Five design docs, each with recorded decisions and exactly one
recommendation per question; evidence anchors re-verified at the
doc's start (multi-board chunks landing meanwhile shift line
numbers; symbols and finding IDs are the stable keys). All design
against the **accepted multi-board world** (per-board
`SyncStates`, `ConnectorId`, derived board sets, registry — the
detailed design plan's DF-1…DF-10 are binding context).

1. **App-data sidecar** (#207, #202-rules; agent A):
   anchor ceremony (creation, title convention, permission,
   detection of loss/archive/move, re-anchor + rebuild flush);
   document schema — format-version field + section registry from
   day one (roundtrip §7's open question: Wave 1 owns the schema so
   D5 generalization is not a breaking change); the merge primitive
   (a `merge_task` sibling folding document entries after the
   native merge; document-embedded per-field clocks, **never** the
   card stamp — the two clock domains stay disjoint; absence ≠
   tombstone; R3 never feeds from the document); actor-cycle
   integration (document evidence alongside `RemoteBoardSnapshot`;
   anchor metadata via one card-detail read; change detection by
   `lastModified`/`filesize`; flush upload **after** the cycle's
   card pushes; read-modify-write union, never blind overwrite);
   the outbox op shape (`UpdateAppData(TaskId)`-class, coalesced at
   flush); the self-clobber mirror (`PendingTaskIntent` protection
   extended to app-field values **and their embedded clocks**);
   failure fences (failed upload keeps ops retryable; failed/
   partial download folds nothing — evidence-then-verdict);
   per-board documents across N connectors (derived board sets,
   per-board flush ordering, what a connector without an anchor
   does); the reminder-rules section (N rules per task as document
   keys; the v1 fence on the native path stays intact).
2. **Field family + lockstep + preferences home** (#208, #209,
   #244, #345; agent B): field shapes — `scheduled:
   Option<DateTime<Utc>>` mirroring `duedate`, effort as a scalar
   duration, importance/urgency as small enums or scores (report
   bias: keep the fields dumb; derived priority is decision
   support, out of wave); the **native-`startdate` evaluation**
   (roundtrip §6: a free native LWW slot vs a sidecar entry —
   exactly one recommendation); the #244 lockstep seam (a
   field-manifest test asserting `Task`/`TaskClocks`/
   `TaskChanges`/`PendingTaskIntent` agree, landing with the
   *first* field); storage columns (additive migration; epoch-
   default precedent from 0005); CLI verbs + UI list decorations;
   the **`UserPreferences` home ruling** (domain entity vs config
   section; the day-start-time semantics incl. DST; consumers
   contract; device-local until D5).
3. **Reminders surfaces** (#204, #211, #344; agent C):
   N-reminder surface (replace `SetReminderRule`'s
   replace-with-one semantics with add/remove/list; CLI/REPL/UI
   growth; the UX of N rules); standalone timers & alarms (the
   `Adhoc` anchor kind on `ReminderRef` + a small `AdhocAlarm`
   entity reusing the occurrence/snooze/sink pipeline; persistence;
   CLI-first surface); the toast sink (dependency candidates for
   Linux/Windows/macOS desktop notification — a **stack-approval
   decision for the user**; the per-platform matrix; kiosk
   degradation; what stays behind the `NotificationSink` port).
4. **Views & finding** (#346, #347, #348; agent D): today view
   (driven by the scheduled field + day-boundary preference;
   R4 keyboard journey, R1 component discipline, kiosk
   degeneration per R9); inbox stack + quick capture (designated
   stack convention + capture verb; triage UX); fuzzy search (pure
   ranking function over `AppState` — proptest target; `tasks
   find` CLI/REPL; the jump-to palette). ADHD research principles
   are an input, not a feature (D6).
5. **Undo architecture — D4** (#349; agent E): command journal +
   inverse synthesis vs state snapshots; interaction with sync
   (the pushed-edit rule: undo after push is a **new edit**, never
   a pretended reversion); the undo-tree scope; what a v1
   implementation would cost. Design only — no implementation in
   this wave.

## 6. Implementation chunk sketch (post-acceptance input)

Sizes on the house scale; the wave implementation plan re-chunks
this per the accepted designs, it does not re-scope.

| # | Chunk | Issues | Size |
|---|---|---|---|
| 1 | Domain field family + lockstep manifest test | #208 #209 #244 | M |
| 2 | UserPreferences + day-boundary preference | #345 | S |
| 3 | Sidecar transport: document codec + anchor ceremony + connector integration | #207 | M–L |
| 4 | Sidecar merge + engine fold + flush + fences + reminder-rules section | #207 #202(rules) | L |
| 5 | Field surfaces: CLI verbs + UI decorations | (parts of #208/#209) | M |
| 6 | N-reminder surfaces | #204 | M |
| 7 | Standalone timers & alarms | #211 | M |
| 8 | Reminder toast sink | #344 | M |
| 9 | Fuzzy search: ranking core + `tasks find` + palette | #348 | M |
| 10 | Today view + inbox capture | #346 #347 | M–L |
| 11 | Undo design round (docs-only; may run first/parallel) | #349 | S–M |
| 12 | Docs + wave exit verification | — | S |

Dependency spine (draft): 1 → {2, 5}; 3 → 4 → 6 (rules section
unblocks N-reminder sync); 8 independent; 9 independent; 2 + 1 → 10;
11 anywhere. Everything gated on the multi-board implementation
chunks having landed (§7).

## 7. Sequencing

1. **Now**: this framing plan + the tracker bookkeeping (§9,
   applied).
2. **Design round**: the five design docs, dispatched in parallel
   off `main` (docs-only — no code conflicts with the in-flight
   multi-board implementation chunks; the D-W8 precedent). Each
   re-verifies its code anchors at start and treats the accepted
   multi-board design as binding context.
3. **User decision session**: rule the recommendations (D-D6's
   list), accept the designs.
4. **Wave implementation plan + implementation dispatch plan**:
   written after acceptance — the follow-up session this framing
   feeds. **Gated on the multi-board implementation round having
   landed**: the field family and sidecar touch `entities.rs`,
   `command.rs`, `outbox.rs`, `merge.rs`, `pipeline.rs`, the sync
   actor, and the storage codec/migrations — precisely the files
   multi-board chunks 1–4 rewrite. Stacking this wave's code on
   pre-multi-board `main` would force a mid-flight redesign.
5. **Kiosk interplay**: kiosk wave 1 is READY-unstarted with
   minimal overlap; whether kiosk or daily-driver implements first
   is the user's scheduling call — both follow multi-board cleanly.
   The today-view design states its kiosk degeneration either way.

## 8. Risks

- **Designing against an in-flight implementation** (multi-board):
  mitigated by treating the accepted design (DF-1…DF-10) as
  normative and re-verifying anchors; symbols and finding IDs are
  the stable keys, line numbers are not.
- **Sidecar edge fragility**: the app-level POST update route is
  contract-pinned but not API-spec'd upstream (roundtrip §7); the
  anchor lifecycle ceremonies must be designed, not hand-waved.
- **Field sprawl before the seam**: the #244 manifest test lands
  with the *first* field, not after the family.
- **Toast dependency**: a new crate needs stack approval (AGENTS
  §4) — surfaced as a decision-session item, not smuggled in.
- **Schema ownership**: the document's format-version + section
  registry must be right on day one (D5 generalization rides it).
- **Scope creep**: task-kind/respite/recurrence parameters are
  Wave-2/3 fields; the schema reserves space, no field lands.

## 9. Tracker bookkeeping (applied 2026-10-18)

- Milestone **`wave-daily-driver`** created (id 5).
- Assigned: **#202, #204, #207, #208, #209, #211, #244**.
- Created + assigned: **#344** (toast sink), **#345**
  (day-boundary/UserPreferences), **#346** (today view), **#347**
  (inbox capture), **#348** (fuzzy search), **#349** (undo design
  round).
- **#176** left open, unscheduled (its design-round scope is
  #349's; implementation is a later wave).
- Scope comment recorded on **#202** (rules in-wave, proofs → D5).
- No issues closed; no other tracker edits.

## 10. References

- Roadmap: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (§6 wave 1; §3.1/§3.2/§3.4/§3.7; §4 D1, D4, D6).
- D1 answer (normative): `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  (§4 recommendation; §5 merge implications against the code; §6
  per-field verdicts; §7 risks).
- Sidecar transport (landed): `.artifacts/plans/2026-10-15-sync-nextcloud-attachment-content-plan.md`
  and `crates/taskboard-sync-nextcloud/src/client.rs`.
- Reminders: `.artifacts/plans/2026-10-12-reminders-design-plan.md`
  ("plan 5" = the toast sink), `2026-10-13-reminders-domain-model-plan.md`;
  `crates/taskboard-domain/src/reminders.rs`,
  `crates/taskboard-app/src/notifications.rs`.
- GUI constraints: `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`
  (R1/R4/R9); ADHD input: `.artifacts/reports/2026-10-14-adhd-support-research-report.md`.
- Multi-board backdrop: `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  (accepted), `-wave-implementation-plan.md` (DF-1…DF-10).
- Lockstep finding: `#244` ←
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`
  (F16-01, F3-05).
- Pattern exemplars: `2026-10-18-multi-board-wave-design-plan.md`
  (this doc's shape), `.artifacts/templates/dispatch-plan.md`.
- Code anchors @ `07088ee`: `entities.rs:49/164` (`TaskClocks`/
  `Task`), `pipeline.rs` (`TaskChanges`, `PendingTaskIntent`),
  `outbox.rs` (`LocalOp`), `notifications.rs:30/46`,
  `commands/reminders.rs:126/162`, `config.rs` (`[reminders]`,
  `[ui]`), `model.rs` (no `startdate` mapping), ui crate
  (`actions.rs`, `views/`, `view_state.rs`, `router.rs`).
