# [Plan] Reminder settlement re-time — prune & snooze compaction keyed to dismissal (#474, DR-3 chunk 1) — stage-1 implementation plan

- **Date**: 2026-10-26
- **Kind**: stage-1 TDD implementation plan for chunk DR-3/1 of the
  design-rounds batch dispatch. Zero production code in this commit;
  this file is the chunk's entire planning artefact. Implementation is
  stage 2's (dispatch prompt 1.2), review + shipping stage 3's
  (prompt 1.3).
- **Origin**: design round
  `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`
  — §3 Part A (D-I1a/b/c, **normative item by item**), §1 (the bug
  facts + the resurrection trace), §5 (the domain/engine test hooks),
  §7 (chunk 1) — dispatched by
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§4.3. The **written authority** the re-time enforces is DR-1's R3
  notification model:
  `.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
  §4-R3 (R3.1 supersessions; R3.2 the two-fact split) and its stage-2
  doc `2026-10-25-reminder-rule-sync-design-plan.md` D-3/D-6/D-10.
- **Normative base**: `main` = `f29dde1` (the worktree's branch point,
  == `origin/main` at stage-1 time). Chunk gates: none (wave 1).
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§4, D-1…D-11). The one design
  correction this planning pass contributes is **D-1**: the design's
  literal "invariance of `active_notifications` under the post-pass on
  arbitrary states" is two-sided-unprovable for the ruled criteria on
  adversarial interior-dismissal chains; the honest re-pin splits it
  into the directional anti-resurrection law (arbitrary states) plus
  two-sided equality on the suite's state shapes, and pins the
  interior-settlement composition as a deterministic contract test.

## 1. Mission & binding constraints

Re-time reminder settlement from **delivery** to **dismissal**: two
pre-DR-1 mechanisms in the domain's post-batch pass still treat
"delivered" as "settled", which makes `active_notifications`' alarm arm
unreachable through the engine and can resurrect a dismissed
notification. The fix is two predicate changes plus the honest re-pin
of every test that pinned the old contract. Per the dispatch §4.3:

1. `prune_settled_alarms` keys on **delivered ∧ dismissed**
   (`reminders.rs:585-598` today keys on delivered alone).
2. `compact_snooze_chains` keys on **both endpoints dismissed**
   (`reminders.rs:549-560` today keys on both-endpoints-delivered).
3. The unobservability properties gain the **second reader**
   (`active_notifications`) — extending only `due_firings` coverage is
   the exact hole this chunk closes.
4. The §1 resurrection trace becomes a deterministic regression test.
5. Engine R9 is re-timed: fire → the alarm row **and** its delivery
   **and** the in-app surface row survive → dismiss → pruned by that
   batch's post-pass.
6. **No schema change** (explicit rule): migrations 0004/0008/0009 are
   untouched; rows simply live longer. The diff shape is
   **domain + tests only** — any `taskboard-storage-sqlite`,
   `taskboard-app`, or `taskboard-ui-slint` hunk is a fence violation.
7. The fence: the OS-delivery/visual plumbing bugs (toasts never
   reaching the daemon; in-app rows appearing only for overdue tasks)
   stay with **GUI review round 3** — this chunk must not touch them
   (origin §3 Part A fence; dispatch §4.3).
8. AGENTS §5's "do not weaken a correct test" is satisfied the honest
   way: the re-timed tests cite **DR-1 R3** as the superseded-contract
   authority (§5).

Everything else about the current lifecycle stands: map membership is
liveness, `remove_alarm`'s cascade is unchanged (`reminders.rs:568-574`),
no GC/retention knob, the post-pass caller-order (compaction → prune,
`persistence.rs:495-501`) is unchanged, unfired alarms live until fired
or cancelled.

## 2. Verified anchors (branch point `f29dde1`; symbols are the stable keys)

| Fact | Anchor |
|---|---|
| Prune criterion (delivered-only) | `reminders.rs:585-598` `prune_settled_alarms` — `alarm_occurrence(..).is_some_and(\|eff\| is_delivered(Adhoc(id), eff))` |
| Compaction criterion (both-delivered) | `reminders.rs:549-560` `compact_snooze_chains` |
| The second reader | `reminders.rs:768-818` `active_notifications` — alarm arm `:803-815` gates `delivered ∧ ¬dismissed` at the **effective** occurrence |
| The first reader | `reminders.rs:697-750` `due_firings` — gates `¬delivered ∧ ¬dismissed` |
| Dismissal lookup / fact | `reminders.rs:447-451` `is_dismissed`; `ReminderDismissal` `:311-320`; `ReminderState.dismissals` `:420` |
| Chain walk | `walk_chain` `:632-650` (cycle-bounded); `alarm_occurrence` `:658-661`; `effective_occurrence` `:616-624` |
| Post-batch pass (order load-bearing) | `persistence.rs:491-501` — compaction `:495`, prune `:501`, inside `apply_actions`; reached from `EngineCore::interpret` (`state/core.rs:58`) and the tick's delivery batch (`state/actor.rs:1085-1113`) |
| Planner acceptance (reachability) | `command.rs:1249-1296` `SnoozeReminder` — Adhoc arm is existence-only (any occurrence, dismissed or not, while the row lives); `command.rs:1312-1362` `DismissReminder` — Adhoc arm requires a live alarm row, **any** occurrence, pre-dismissal accepted, repeat = no-op, no `UpdateAppData` for Adhoc (device-local) |
| Withdraw derivation (no change needed) | `state/actor.rs:1144-1160` `withdrawals_for` — every `RecordReminderDismissal` in an applied batch becomes one `Withdraw`; Adhoc refs included |
| Pinned tests on the old contract | domain: `snooze_compaction_prunes_only_fully_delivered_links` (`persistence.rs:1251`), `the_prune_runs_after_compaction_settled_chain_reverts_then_prunes` `:1584`, `the_composition_narrative_through_batches_only` `:1630`, p5 part (d) `:1403-1406`; engine: `r9_alarm_fires_within_one_tick_and_settles` (`state/tests/reminders.rs:469`), `r11_snoozed_alarm_refires_at_its_target` `:562` |
| Properties to extend | `compaction_is_unobservable_in_due_firings` (`persistence.rs:1410`), `compaction_is_unobservable_on_hand_uncompacted_states` `:1429`, `alarm_prune_is_unobservable_in_due_firings` `:1693` |
| Strategy normalizer (one definition site) | `test_support/reminders.rs:336-338` — generated states normalize through `compact_snooze_chains` itself, so the strategies follow the new criterion automatically; **no test_support diff** |
| Snapshot lane | `state/tests/snapshots.rs:71` runs `ReminderEngineConfig::disabled()` — no tick, no fixture impact; this chunk is not a shape-widener |
| Downstream readers (no code change) | `timers list` renders overdue rows with `remaining: None` (`app/commands/timers.rs:201-221`); `timers cancel` is map-membership (`:149-152`) — fired-undismissed alarms lingering in state render/cancel correctly by the existing contract |

## 3. The two new criteria, exactly

**D-2 — prune (D-I1a).** `prune_settled_alarms` prunes an alarm iff its
effective occurrence is **delivered ∧ dismissed**:

```rust
alarm_occurrence(reminders, id).is_some_and(|eff| {
    let reminder = ReminderRef::Adhoc(id);
    reminders.is_delivered(&reminder, &eff) && reminders.is_dismissed(&reminder, &eff)
})
```

Consequences (the point, per D-I1a): between fire and dismiss the alarm
row **and** its delivery live in state, so `active_notifications`' alarm
arm becomes reachable and `SnoozeReminder`/`CancelAdhocAlarm` on a fired
alarm resolve; a fired-then-dismissed alarm still prunes — with its
deliveries, dismissals, and snooze links — exactly at the dismissal's
post-batch pass (the dismiss command's own `apply` batch runs the
post-pass). A **dismissed-but-unfired** alarm survives (pre-dismissal is
legal data; the criterion requires the delivery too) and fires never.

**D-3 — compaction (D-I1b).** `compact_snooze_chains` prunes a link iff
**both endpoints are dismissed** (the single-predicate swap; keep the
existing collect-then-retain shape). Delivered-ness leaves the criterion
entirely: a merely-delivered occurrence is the live in-app surface under
R3, not a dead one; a dismissed occurrence is dead in both readers (no
fire — `due_firings:723`; no row — `active_notifications:807`). This
closes the §1 resurrection trace: the dismissed-at-B link survives
compaction (A is undismissed), the effective stays at B, and
`is_dismissed(B)` keeps the row off the surface.

**D-4 — caller-order unchanged.** Compaction still runs before the prune
(`persistence.rs:495-501`); only the two doc/comment blocks re-word
(criteria + the re-timed composition rationale, D-7). The mis-order
catcher re-pins with the same two-link shape (§6 T5): a settled prefix
(both links' upstream endpoints delivered ∧ dismissed) compacts first,
the walk reverts to the settled base, the prune removes the alarm;
prune-first would read the undelivered tail and keep the zombie.

## 4. Recorded decisions (every open question resolved)

**D-1 — the property-scope correction (the load-bearing decision).**
Part A D-I1c asks for "invariance of `active_notifications` under the
post-pass on arbitrary states". Two-sided equality on truly arbitrary
states is **false** for the ruled criteria: an interior link whose both
endpoints are dismissed but which sits upstream of a live link (chain
`base→m→t`, all delivered, `base`+`m` dismissed, `t` alive) compacts,
the walk reverts to the dismissed `base`, and the prune then reclaims
the alarm — `t`'s live row disappears. That shape is engine-reachable
(`DismissReminder` accepts any occurrence of a live row;
`SnoozeReminder` is existence-only), and it is the **ruled composition
semantics**: the user settled the walk's reachable prefix; the residual
link is unreachable garbage the composition reclaims — a disappearance,
never a resurrection. Decision, three parts:

  1. The **arbitrary-state** pin is the directional anti-resurrection
     law (T8 below): the post-pass never *adds* an active row or a
     firing — `active(after) ⊆ active(before)` and `due(after) ⊆
     due(before)` as `(reminder, occurrence)` sets. Proof obligation
     discharged in-test: a shortened walk terminates at the source of
     the first pruned link, which is dismissed, so both gates fail
     there; the prune only removes alarms whose own gate already fails;
     cascades touch only the pruned alarm's rows. This law fails under
     the old criteria on the resurrection trace (the second reader is
     the point).
  2. **Two-sided equality** is pinned where it is true: on
     strategy-normalized states (T6/T7 extended with the active reader)
     and on the hand-dirtied shape (T7), for both readers.
  3. The interior-settlement composition is pinned as a **deterministic
     contract test** (T5) so the disappearance direction is recorded
     behavior, not an accident.

**D-5 — `DismissReminder` on `Adhoc` needs no engine change.** The
withdraw derivation (`actor.rs:1144-1160`) already maps every
`RecordReminderDismissal` in an applied batch to a `Withdraw`; the
dismiss batch's post-pass does the pruning inside `apply_actions`. No
`actor.rs`/`core.rs` diff.

**D-6 — downstream consumers take no diff.** `timers list` renders
fired-undismissed alarms as overdue rows (`remaining: None`) and
`timers cancel` deletes by membership — both already correct for
longer-lived rows (§2 last row). The app crate shows **no diff**; the
behavioral consequence is recorded here, not code-patched.

**D-7 — doc-comment re-times ride the criterion commits.**
`reminders.rs`: the `AdhocAlarm` struct doc ("dies by settlement … once
its effective occurrence is delivered", `:263-268`), the
`compact_snooze_chains` doc (`:540-548`), the `prune_settled_alarms`
doc (`:576-584`); `persistence.rs`: the post-pass comments (`:491-501`)
and the `UpsertAdhocAlarm` action doc (`:258-259`). Comments state the
new criteria and the re-timed load-bearing composition — constraint
text, not narration.

**D-8 — no schema change, no migration.** Rows live longer; nothing
moves on disk. The rebase-time "next free migration" ladder (dispatch
§2) is untouched by this chunk.

**D-9 — the #201 annotation.** Stage 2 carries this text in the PR body
notes; stage 3 posts it as the issue comment (issue stays open):

> #474 (DR-3 chunk 1) re-timed settlement to dismissal: an alarm row and
> its dependents now live until its effective occurrence is delivered ∧
> dismissed, and snooze links until both endpoints are dismissed. The
> growth concern this issue tracks therefore re-terms from delivery rows
> to **dismissal rows** (delivery rows for task-anchored rules are
> unchanged and remain the bounded case; alarm-scoped rows already
> reclaim at settle via the post-batch prune). When a real deployment's
> table sizes justify the churn, the age-pruning rule should age out
> dismissal rows.

**D-10 — test names.** Re-timed tests keep their positions and
anchor-citable names where the file allows; renames only where the old
name asserts the dead contract (T1, T4). New tests use the house
behavior-descriptive style. The R9 re-pin keeps the `r9_` prefix (the
file's pre-existing `r9_muted_task_never_fires` numbering collision is
not this chunk's to fix).

**D-11 — new engine test gating.** Every new/changed `#[tokio::test]`
in `state/tests/reminders.rs` carries the suite's
`#[cfg_attr(miri, ignore)]` (timer wheel) and uses the existing
lockstep/poll helpers only — no sleeps, fixed/stepping clocks,
typed/structural assertions (no text assertions, AGENTS §5).

## 5. The honest re-pin: R3 as the superseded-contract authority

AGENTS §5 forbids weakening a correct test; these tests were **correct
pins of a superseded contract**. DR-1's R3 ruling (§4-R3, quoted in the
origin §3 Part A) is the written authority: *"The in-app notification
persists on every device until the user acts"* — dismissal (explicit,
or implicit via Mark Done) is settlement; the delivery receipt is only
the per-device OS-toast guard. Every re-timed test therefore carries a
doc-comment citation of the form:

> Re-timed 2026-10-26 (#474): delivery-keyed settlement was the pre-R3
> contract; DR-1 R3 (`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
> §4-R3) makes dismissal the settle. The persistence half of the old pin
> is intact: the prune still lands in the same batch's post-pass, keyed
> on delivered ∧ dismissed.

The "persistence half intact" rule from the dispatch review prompt is
binding: every re-timed test keeps asserting *that the prune/compaction
lands in the same post-pass batch* — only the key changes. A re-timed
test that drops the same-batch assertion is a weakening and a finding.

## 6. Test plan (TDD-first; milestones are plan structure, stage 2 lands red-green pairs as standalone commits)

Working instants: `t1_000`/`t2_000`/`t3_000` = `ts(1_000)`/`ts(2_000)`/`ts(3_000)`.
All domain tests go through `apply_actions` batches (the post-pass runs
inside every batch — no direct mutation, the
`the_composition_narrative_through_batches_only` discipline).

**M1 — domain criteria + the regression trace (`persistence.rs` tests)**

- **T1 (re-times `:1251`) `snooze_compaction_prunes_only_fully_dismissed_links`**
  — pruned: both endpoints dismissed (one variant delivered ∧ dismissed,
  one undelivered ∧ dismissed — delivered-ness is irrelevant now);
  kept: both endpoints **delivered but undismissed** (the old test's
  "settled" link — the discriminator), kept: source-dismissed-only,
  kept: target-dismissed-only. Red under the old criterion.
- **T2 (new) `prune_settled_alarms_keys_on_delivered_and_dismissed`** —
  delivered-only: alarm **survives** (old contract, red); delivered ∧
  dismissed at the effective occurrence: pruned with all dependents in
  the same batch; dismissed-but-unfired (pre-dismissal): survives and
  still fires never; settled is a fixed point (`apply_actions(&[])`).
- **T3 (new, the §1 regression script)
  `dismissed_notification_does_not_resurrect_through_compaction`** —
  deterministic trace, each step asserting state + both readers:
  1. `UpsertAdhocAlarm(fire_at = t1_000)` — alive.
  2. Deliver `t1_000` → **alarm + delivery survive**; `active_notifications`
     = exactly 1 row `(Adhoc, t1_000, label)`. *(Fully-old code: pruned
     here — red.)*
  3. Snooze `t1_000 → t2_000` → row leaves the surface by derivation
     (effective moves past `now`).
  4. Deliver `t2_000` (now past the target) → alive; surface = 1 row at
     `t2_000`.
  5. Dismiss `(Adhoc, t2_000)` → same batch: compaction **keeps** the
     link (source undismissed), prune reclaims the alarm; alarms,
     deliveries, snoozes, dismissals all empty; surface empty at
     `t2_000` and beyond. *(D-I1a-only code: link compacts, effective
     reverts to `t1_000`, alarm survives with a **resurrected** row at
     `t1_000` — red here. This is what pins that BOTH criteria moved.)*
  6. Empty batch → fixed point, still empty.

**M2 — the composition, the mis-order catcher, and the properties (`persistence.rs` tests)**

- **T4 (re-times `:1630`) `the_composition_narrative_through_batches_only`**
  — same create → snooze → fire arc, re-timed: the delivery batch now
  leaves the alarm **alive** (delivery + link intact); the dismissal
  batch prunes it with its dependents; fixed point after. Keeps the
  stage-2-deviation note updated (the unrepresentable literal sequence
  remark now covers the fired-alarm survival window).
- **T5 (re-times `:1584`) `the_prune_runs_after_compaction_settled_chain_reverts_then_prunes`**
  — two-link chain `t1_000→t2_000→t3_000`; `t1_000`+`t2_000` delivered
  **and dismissed**; `t3_000` undelivered. One empty batch: compaction
  drops the settled `t1_000→t2_000` link, the walk reverts to
  `t1_000`, the prune removes the alarm (assert `alarms.is_empty()`).
  Prune-first would read effective = `t3_000` (undelivered) and keep
  the zombie — the mis-order catcher, re-keyed. Also pins D-1's
  interior-settlement composition (the `t3_000` tail is unreachable
  garbage reclaimed with the row).
- **T6 (extend `:1410` + `:1693`) — both existing "unobservable"
  properties gain the second reader as equality** on strategy states:
  `compaction_is_unobservable_in_due_firings` asserts
  `active_notifications` equal besides `due_firings` equal;
  `alarm_prune_is_unobservable_in_due_firings` likewise (prune-alone is
  two-sided-unobservable in both readers on arbitrary states — the
  pruned alarm's own gate already fails).
- **T7 (re-times `:1429`)
  `compaction_is_unobservable_on_hand_uncompacted_states`** — the dirt
  becomes **dismiss both endpoints of an existing link** (the
  representable-but-unsanctioned shape the new normalizer prunes);
  assert `due_firings` AND `active_notifications` equal on
  dirty vs normalized. (Multi-link collision caveat inherited from the
  existing test's discipline: generated chains need coincident
  timestamps; recorded, accepted.)
- **T8 (new property)
  `the_post_pass_never_resurrects_a_dismissed_notification`** — over
  `app_state_strategy()` inputs: `apply_actions(&[])`, then
  `active(after) ⊆ active(before)` and `due(after) ⊆ due(before)` as
  pair sets (both readers, directional — D-1.1). Red under the old
  criteria: normalized states legitimately contain both-delivered
  links (the new normalizer keeps them), on which the old compaction
  resurrects.
- **T9 (re-times p5 part (d), `:1403-1406`)** — the persisted-invariant
  loop in `p5_settlement_roundtrip_through_apply_actions` re-keys to
  `!(both endpoints dismissed)`; parts (a)–(c) untouched.

**M3 — the engine R-series re-pin (`state/tests/reminders.rs`, paused clock)**

- **T10 (re-times R9, `:469`) `r9_fired_alarm_survives_until_dismissal`**
  — create (`In(30s)`) → advance past the stamp → exactly one `Fire`
  notification with the `Alarm` anchor (kept); **alarm row + delivery
  survive** (`eventually` on membership, not `alarms.is_empty()`);
  `active_notifications` over a `shared_state()` snapshot carries the
  row; `DismissReminder { Adhoc(alarm_id), ts(T0+30) }` → `Applied`,
  one `Withdraw` observed (the `next_withdraw` helper), then
  `eventually(alarms.is_empty())` **and** deliveries empty — pruned by
  the dismissal batch's post-pass. *(Old code: red at the
  survival asserts.)*
- **T11 (re-times R11, `:562`) `r11_snoozed_alarm_refires_at_its_target`**
  — structure kept (pending-snooze → quiet → re-fire at target with the
  rescheduled occurrence); after the re-fire the row **survives** (the
  old `eventually(alarms.is_empty())` + "settled and pruned" tail is
  replaced by survival + dismiss + same-batch prune — the persistence
  half stays).
- **T12 (new) `r12_snooze_on_a_fired_alarm_moves_the_awaiting_key`** —
  create → fire → `SnoozeReminder` on the **fired** occurrence →
  `Applied` (resolves — the dispatch review criterion), quiet below the
  target, re-fire at the target (occurrence assertion), dismiss →
  pruned. Pins "SnoozeReminder/CancelAdhocAlarm on a fired alarm
  resolve" (cancel is map-membership, covered by the existing
  `adhoc_alarm_delete_cascades_dependents_and_nothing_elses`).

Unchanged, verified untouched by the criteria: R1–R8, R10, the mute R9,
the two withdraw tests (task-anchored), all pure-evaluator domain
properties (p1–p4, p7–p13 — no post-pass involved), the strategy
builder and its self-test, snapshots (reminders disabled).

**Gates**: full AGENTS §10 suite green; `git diff main...HEAD` over
`taskboard-storage-sqlite` / `taskboard-app` / `taskboard-ui-slint` and
`migrations/` must be **empty** (stage-2 self-check, stage-3 finding if
not).

## 7. Risks & mitigations

- **The D-1 scope correction could be read as deviating from the
  normative origin.** It is recorded here with the proof sketch and the
  deterministic contract test (T5); the deviation note is exactly the
  "recorded reason" the dispatch §1 requires. Stage 3 reviews T8's
  directionality and T5's composition against D-1.
- **State growth** (fired-undismissed rows live indefinitely): bounded
  by user action by construction — the surface's entire point; the
  #201 annotation (D-9) re-terms the age-pruning backlog entry.
  Kiosk-scale growth is rows per user inaction, not per tick.
- **Over-retained snooze links** (both-dismissed is weaker pruning than
  both-delivered): links with a dismissed endpoint still influence the
  walk, which is exactly why they must survive; fully-dismissed links
  are dead in both readers and still compact. Property-pinned (T8).
- **Engine-test flakiness on the new eventually() windows**: the suite's
  lockstep + 1 ms poll discipline is reused verbatim; assertions are
  terminal-state only.
- **Scope creep**: any urge to fix toast delivery, banner gating, or
  storage layout is the GUI-review-3 fence speaking — stop at the two
  predicates, the comments, and the tests.

## 8. Estimated diff size

| Piece | Size |
|---|---|
| `reminders.rs` — two predicates + doc comments | ~45 lines (~8 logic) |
| `persistence.rs` — post-pass/action doc comments | ~12 lines |
| `persistence.rs` tests — T1–T9 | ~330–420 lines |
| `state/tests/reminders.rs` — T10–T12 | ~120–160 lines |
| **Total branch (excluding this plan commit)** | **~510–640 lines, ≈85% tests** |

Commit plan (stage 2): TDD pairs per milestone — M1 red→green
(`test(domain): settlement keys on dismissal` /
`fix(domain): reminder settlement re-timed to dismissal`), M2
(`test(domain): post-pass unobservability over both readers`), M3
(`test(state): fired alarms survive until dismissal`) — every commit
standalone (rebase-merge hygiene); the criteria change and its
doc-comments land together so no commit carries a stale contract
comment.

## 9. References

- Origin (normative): `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md` §3 Part A, §1, §5, §7.
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md` §1, §4.3 (Covers/Acceptance + prompts 1.1–1.3).
- Authority: `.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md` §4-R3 (R3.1–R3.2, R3.5 resolutions); stage-2 doc `2026-10-25-reminder-rule-sync-design-plan.md` D-3/D-6/D-10.
- Issues: #474 (this chunk — stage 3 closes), #201 (annotated, stays open), #352/#203 (the round's pair), #202 (DR-1, the baseline round).
