# The Habit Event Commands Chunk Plan — Chunk 2 of the Habit Event-Log Dispatch

- **Date**: 2026-11-27
- **Kind**: stage-1 (planning) artefact for chunk 2 ("commands, undo,
  journal", branch `feat/domain-habit-event-commands`) of the #1519
  habit event-log dispatch.
- **Origins (normative)**:
  `.artifacts/plans/2026-11-27-habit-event-log-design.md` §0 (Rulings B,
  C, H — all accepted on defaults 2026-11-27), §3 (writes and the exact
  inverse), §7 rows 7–10 and 14; the UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md` (US-1..US-3,
  US-5 engine halves; §7 command-contract delta; §8 verification
  tokens); the dispatch plan
  `.artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md` §1/§3
  (chunk table, shared execution model, Prompts 2.2/2.3); the chunk-1
  plan `.artifacts/plans/2026-11-27-habit-events-domain-plan.md` (D1–D12
  — the landed shapes); `docs/testing_strategy.org` §6 (clock
  discipline); `docs/coding_guidelines.org` §2.1.
- **Worktree**: `/home/viktoria/src/taskboard-WT-feat-domain-habit-event-commands`
  on branch `feat/domain-habit-event-commands` at `origin/main`
  `fafea623` (worktree-doctor passed; created by the dispatcher per
  AGENTS §8). Chunk 1 landed via PR #1532 (merge 2026-11-27); the
  landed code, not the chunk-1 plan's forecast, is cited where they
  differ.
- **Scope**: the four habit write arms re-keyed onto events; receipts
  carry event ids; the journal's habit grammar becomes event-id
  pre-images; the D24/D25 fences die; a new event persistence action;
  undo-facing synthesis commands. **Non-goals** (dispatch plan §3): no
  sqlite persistence of events (chunk 3), no wire fold (chunk 4), no
  production UI/CLI changes (chunk 5), no deletion of the legacy maps
  (chunk 5).

---

## 0. Rulings requested (proceeding on the recommended defaults)

Two decisions in this chunk genuinely belong to the operator; both are
settled here on the recorded default so the plan is dispatchable. The
dispatcher may overrule before stage 2 — only §1 D-C and §1 D-J2 are
affected.

### RQ-1 — the coexistence rule: derived mirror vs strict freeze

The dispatch plan's chunk-2 row says the command arms write ONLY events
("the legacy map freezes; chunk 3's remap consumes it"). Verified
against the landed tree, strict freeze breaks the gate:

- The UI crate reads `state.habits.log` / `day_log` directly
  (`presentation.rs:8209–8247` badge/skip, `:8796+` heatmap) and its
  journey tests dispatch `LogHabit`/`Undo` through the harness
  (`tests/journeys/habit_family_journeys.rs:397`). Chunk 2 must not
  touch the UI crate (dispatch non-goal), and AGENTS §5 forbids
  weakening those tests; AGENTS §8/§10 forbid a red gate. Under strict
  freeze the journey tests read a map no writer feeds — they fail, and
  the fix is either a UI-crate edit (out of scope) or a weakened test
  (forbidden).
- The app CLI surfaces (`crates/taskboard-app/src/commands/habits.rs`)
  read the same maps; the state-suite snapshots assert log-derived
  outcomes after dispatching.

**Default (adopted): a derived mirror.** The command arms mint events
(sole source of truth) and *in the same batch* re-emit the legacy
`RecordHabitLog` / `RecordHabitDayLog` actions with counts/postures
recomputed **from the event derivations** (§1 D-C). The legacy map is
never consulted as authority by anything this chunk owns; it becomes a
mechanical projection of the event log, kept writable solely so the
untouched UI/CLI readers and journey tests stay truthful until chunk 5
deletes the mirror with the maps. This is the chunk-1 precedent
(`log_window` kept as the equivalence oracle) scaled up. "Frozen" is
therefore defined per reader in §1 D-C.

**Rejected alternative**: strict freeze + re-base the UI journey tests.
Re-basing a journey test onto event state hollows it (a journey test
that no longer checks the badge tests nothing user-visible), and it
still requires UI-crate edits.

### RQ-2 — `NotUndoableReason::HabitSurface` disposition

The dispatch acceptance says "the `NotUndoableReason::HabitSurface`
variant and its classifiers are gone". The variant is *persisted* in
journal entries (pre-#1520 profiles record it), and two live classifiers
remain policy-fenced for reasons D24/D25's deaths do not touch:
`PauseHabit` (`undo.rs:488` — `HabitChanges` has no `paused_until` leg)
and the convert composites (`undo.rs:509` — the rule+habit composite
restore stays unexpressible).

**Default (adopted)**: `HabitSurface` becomes a **retired,
serde-stability-only variant** (the `RestoreClass` C8 precedent,
`journal.rs:100–107`): doc-commented "retired — no classifier may
reference it", zero live classifiers, decode-only so old entries keep
refusing with their recorded reason. `PauseHabit` and the two convert
composites move to a new `NotUndoableReason::HabitPolicy` (same policy
fence, honestly named; the D24/D25 *exactness* fences are the ones that
die). The acceptance's evidence — no classifier references
`HabitSurface` — holds; the literal variant deletion is rejected on the
journal serde-stability law the codebase already follows.

**Rejected alternative**: hard-delete the variant — breaks decode of
persisted journal entries recording it; `#[serde(other)]` fallbacks
would silently re-classify recorded refusals.

---

## 1. Recorded decisions

### D-C — The coexistence rule, stated per reader (RQ-1's default)

**Authoritative**: `HabitState.events` — every command arm mint/tombstone
writes there first; every receipt id, journal pre-image, and undo
inverse addresses events.

**The mirror**: after the event writes are computed, the arm emits the
legacy `RecordHabitLog` / `RecordHabitDayLog` actions whose counts,
`skip` bits, and clocks are **recomputed from the event derivations**,
not incremented from the old map (§1 D-Log/D-Adj/D-Skip spell the exact
recomputation). The mirror keeps `mint_log_clock` — but *only* as the
legacy-row LWW mint so the existing `RecordHabitLog` apply semantics
(#1095 whole-second discipline) stay correct for the two-same-window
events case; its #986 *undo* role dies (undo no longer rides
`AdjustHabitLog`). `mint_log_clock` deletion is chunk 5, with the
mirror.

**What "frozen" means for every remaining reader of `HabitState.log` /
`day_log` during this chunk**:

| Reader | This chunk | Chunk |
|---|---|---|
| Command arms (writer) | never read-and-increment; they *write* the mirror from derivations | 5 deletes the write |
| Domain streak/quota/skip consultations (`habit.rs` walk, count closures) | **unchanged** — they read the mirror, which is derived-exact; the mechanical re-key onto `derive_window_counts`/`derive_skip_posture` stays dispatch row 5 (chunk 5), as the dispatch table assigns | 5 re-keys |
| UI presentation (badge `presentation.rs:8209`, skip `:8247`, heatmap `:8796` — reads `log`/`day_log`) | untouched, truthful via the mirror | 5 re-keys |
| App CLI surfaces (`commands/habits.rs` reads) | untouched, truthful via the mirror | 5 re-keys |
| `merge.rs` legacy fold / sync flush (wire) | untouched — chunk 4 territory; the fold keeps feeding the legacy map from old documents | 4/5 |
| sqlite flush/load of `habit_log` / `habit_day_log` rows (state-driven, `repo.rs`) | untouched — they persist the mirror, so a restart keeps the UI truthful | 3 adds events; 5 stops |
| sqlite persistence of the events themselves | **inert partition arm** (§1 D-P): events are not persisted to sqlite until chunk 3's migration 0032. Transitional non-persistence on chunk-2 mains is acceptable — the five chunks merge serially into one release; no shipped build ever lacks it | 3 |
| Chunk-3 remap | sees only pre-event-log rows — §1 D-R's invariant | 3 |

**The keystone test** pins the mirror's honesty: a proptest drives a
random command sequence (log/adjust/skip/unskip/undo) through the
engine and asserts, after every step, that the legacy maps equal the
event derivations (`log` counts ≡ `derive_window_counts`, `day_log` ≡
`derive_day_counts`, per-row `skip` ≡ `derive_skip_posture`). This is
what makes chunk 5's re-key provably mechanical.

### D-R — The chunk-3 remap contract (the synthesized-events invariant)

The remap (chunk 3) synthesizes Ruling-A1 events from the legacy rows
**iff the persisted event store is empty and the migration marker is
absent**. The invariant this chunk must guarantee is therefore
*operational*: **no profile that has ever persisted an event reaches
the remap.** Since chunk 3 persists events from the same release that
ships the remap, every upgraded profile has an empty event store at
upgrade, so the remap sees only genuine pre-event-log rows. Synthesized
(adjust-minted v7) events and their mirror rows can coexist only on
developer intermediates, where the non-empty event store (or the
marker) keeps the remap off. **Stage 2 pins this with a test**: a state
with one synthesized adjust event + its mirror row, encoded as chunk 3
will see it (events present), is asserted to satisfy the remap gate's
negative condition — the gate's predicate function lands in chunk 3;
this chunk pins the *state property* (events non-empty whenever a
synthesized event exists) with `us2` evidence. No v5 ids, no uuid
feature change in this chunk (Ruling I is chunk-3-scoped).

### D-P — Persistence action: ONE new variant, `UpsertHabitEvent(HabitEvent)`

**Decision: one action, not the Record/Tombstone pair.** The design's
§7 row 8 names `RecordHabitEvent` / `TombstoneHabitEvent`; this plan
deviates with reason: a tombstone is just an event row with `deleted`
set, so the pair collapses into the entity-upsert discipline every
other event-carrier already uses (`UpsertHabit` — tombstone rides the
row; the chunk-3 sqlite posture is literally `INSERT OR REPLACE` by
primary key, design §6.2). One variant = one apply arm, one sqlite
statement, one partition arm, and the command layer builds the full
post-row (mint or tombstone) either way.

```rust
/// Insert or replace a habit event by its id (the entity-upsert
/// discipline: identical uuid = identical fact; the tombstone rides
/// the row — Ruling C2). The sqlite twin is INSERT OR REPLACE by
/// primary key (chunk 3); no LWW guards — the fold arbitrates nothing.
UpsertHabitEvent(crate::habit_event::HabitEvent),
```

`RecordHabitLog` / `RecordHabitDayLog` **stay** (the mirror's write
actions; deleted in chunk 5 with the mirror) — second recorded
deviation from §7 row 8, same reason as RQ-1.

**sqlite repo delta this chunk (minimal, forced by the exhaustive
partition match at `repo.rs:2275–2335`)**: one new partition arm that
collects `UpsertHabitEvent` rows into a vector that is *dropped* with a
constraint-stating comment ("event persistence lands with migration
0032, chunk 3 — the events map round-trips through `PersistedState`
serde in every fake/memory path from the day this arm lands"). This is
the only line outside `taskboard-domain` and `taskboard-state` this
chunk touches. Transitional risk recorded in §7.

### D-Log — `LogHabit` (design §3.1)

- Derive the target window via `log_window(habit, &app.habits.log, now,
  day)` — unchanged rule, unchanged call shape (the mirror makes the
  map a legal count source; chunk 5 re-keys the call onto the fold).
- Mint **one** `Completion` event: `id = ids.new_habit_event_id()`,
  `at = now`, `by = author`, `deleted = None`. No clock bump, no twin.
- Actions: `UpsertHabitEvent(minted)` + the mirror
  `RecordHabitLog(window_count + 1 via the legacy increment —
  equivalent-by-mirror-law to the derived post-count; the keystone
  proptest holds the equivalence) + RecordHabitDayLog(day_count + 1)` +
  `board_flush`. Rationale: the legacy increment *is* the derived
  post-count under D-C's keystone law; writing it as the existing
  increment keeps the diff minimal. (The alternative — compute
  `derive_window_counts` after mint and write that — is behaviorally
  identical and rejected as ceremony.)
- Receipt: `CommandOutcome::HabitLogged { habit, window, count, event:
  HabitEventId }` — `count` stays the post-increment value (display);
  `event` is new (serde: verify whether `CommandOutcome` is persisted;
  if any persisted payload embeds it, the new field carries
  `#[serde(default)]` — stage 2 verifies, chunk-1 D9's additive law).

### D-Adj — `AdjustHabitLog` (Ruling H, design §3.2)

- The **current-kind canonical window gate stays**: `window !=
  WindowStart::of_period(habit.target.kind(), window.as_date())` →
  typed `CommandError::InvalidHabitWindow` (Ruling H's recorded
  decision; CLI grammar unchanged — `habits adjust <habit> --window
  --count` untouched).
- **The reconcile diff**: derive the window's live completion events
  with their `(at, id)` order (run the fold, collect the window's
  member ids newest-first); then, to reach `count`:
  - downward: tombstone the **newest** events first (the documented
    audit rule: a correction removes the most recent completions
    first) — emit `UpsertHabitEvent` rows with
    `deleted = Some(HabitEventTombstone { at: now, by: author })`;
  - upward: mint `count - derived` fresh `Completion` events
    (`at = now`, id seam) — **synthesized events**; they are
    indistinguishable from logged events by design (D-R's invariant
    covers the remap);
  - equal: `no_op()` (the identical-count early return stays).
- Mirror: the day-kind twin writes the audited target into
  `day_log`/window row exactly as today (`count` absolute write, fresh
  `mint_log_clock`); a Week/Month adjust mirrors only the window row
  (the R-2 divergence comment stays verbatim).
- Receipt: `HabitLogAdjusted { habit, window, count, created: Vec<
  HabitEventId>, tombstoned: Vec<HabitEventId> }` (Ruling H: "the
  receipt names the events it created or tombstoned"; serde-default the
  two vecs if the outcome is persisted).
- **D24 dies here**: an absent window derives empty → an upward adjust
  mints; no refusal anywhere in the apply path.

### D-Skip — `SkipHabit` / `UnskipHabit` (Ruling B, design §3.3)

- Window minted planner-side exactly as today (`R-4` posture,
  `command.rs:3379`).
- Idempotence checks run against the **derived posture**
  (`derive_skip_posture` for the habit, consulted at the target
  window) — not the mirror bit. Already-skipped skip / not-skipped
  unskip are the accepted `no_op()`s.
- `SkipHabit` mints `Skip { window }`; `UnskipHabit` mints
  `Unskip { window }` — one mark event each, `at = now`, no row
  rewrite, no clock mint.
- Mirror: the legacy row's `skip` bit is written to the derived posture
  (preserving the count), same `RecordHabitLog` shape as today.
- Receipts: `HabitSkipped { habit, window, event: HabitEventId }`;
  `UnskipHabit`'s outcome upgrades from bare `Applied` to
  `HabitUnskipped { habit, window, event: HabitEventId }` (the undo
  needs the mark id; a new outcome variant is internal feedback, not a
  user-facing verb — R-1 governs verbs/surfaces, not receipts).

### D-J1 — Journal grammar: event pre-images and stamps

New/changed persisted shapes (all additive or paired with the RQ-2
retirement; serde-stability verified per shape at implementation):

- `PreImage::HabitEventWindow { habit: HabitId, window: WindowStart,
  events: Vec<(HabitEventId, Option<HabitEventTombstone>)> }` — the
  **exact per-event tombstone-state snapshot** of the addressed
  window's fold membership at pre-command time. This one variant serves
  every habit class: LogHabit, SkipHabit, UnskipHabit (pre-state for
  the guard and the minted-id extraction), and AdjustHabitLog (the
  exact restore source, design §3.4).
  - Membership = the events the fold attributes to `window` under the
    habit's *current* config at journaling time (completions by
    `derive_window_counts` membership, marks by the shared-fold
    attribution). Snapshot, not recomputation: the entry is the truth
    forever after.
- `JournalEntity::HabitEventWindow(HabitId, WindowStart)` and
  `EntityStamps::HabitEventWindow(Vec<(HabitEventId,
  Option<HabitEventTombstone>)>)` — the post-write snapshot, the guard
  oracle (§1 D-U). The legacy `JournalEntity::HabitLog`,
  `EntityStamps::HabitLog`, and `PreImage::HabitLogRow` **stay as
  decode-only retired shapes** (documented; no production writer after
  this chunk): pre-#1520 journal entries decode and keep undoing
  through the unchanged legacy inverse paths (§1 D-U's legacy leg).
- `NotUndoableReason`: per RQ-2.
- `PreImage::HabitLogRow`'s doc-comment cross-reference (`journal.rs:188`)
  re-pointed to the retired note.

### D-J2 — Classification (the D24/D25 deletions)

In `undo.rs`'s facts function:

- `LogHabit`, `SkipHabit`, `UnskipHabit`, `AdjustHabitLog`: all four
  classify `journaled_pre_images` unconditionally — with the pre-image
  capture extended to snapshot the addressed window's event membership
  (D-J1). The `AdjustHabitLog` absent-window check (`undo.rs:439–443`,
  D24) and its comment are **deleted**; the arm's comment states the
  successor law (empty pre-image restores emptiness exactly).
- `UpdateHabit { changes, .. }`: the `continued_from` seam refusal
  (`undo.rs:420–421`, D25) is **deleted** — the seam now journals with
  the ordinary `PreImage::Habit` row pre-image (windows are derived;
  there is no clock slot to watermark against anymore — design §3.4).
- `PauseHabit` and `ConvertSeriesToHabit`/`ConvertHabitToSeries`:
  re-classified to `NotUndoableReason::HabitPolicy` (RQ-2), comments
  carried verbatim.
- `HabitSurface`: zero classifiers; retired doc (RQ-2).
- Reflection/Reminder fences: untouched (acceptance requirement).

### D-U — Inverses: exact, per-event (design §3.4)

**New undo-facing synthesis commands** (the `RestoreTask` precedent —
catalogue verbs that only the undo synthesis plans, never surfaced in
UI/CLI; R-1 governs user-facing surfaces, and the UX plan's matrix is
unchanged):

- `StateCommand::TombstoneHabitEvent { event: HabitEventId }` — loads
  the event (typed `UnknownHabitEvent` rejection if absent/gone), sets
  `deleted = Some(HabitEventTombstone { at: now, by: author })`,
  emits `UpsertHabitEvent(post)` + the mirror refresh for the event's
  derived window/day (recompute the affected legacy rows from the
  post-state derivations) + `board_flush`. Outcome: `Applied`.
- `StateCommand::RestoreHabitEvent { event: HabitEvent }` — verbatim
  row restore (the #900 verbatim-resurrect precedent): upsert the
  given event row exactly (including its tombstone state), + mirror
  refresh + flush. Outcome: `Applied`.

Both classify journal-undoable-passthrough (they are never journaled —
the synthesis plans them outside the journal line, as `RestoreTask`
does today; their classification arms exist for match totality and
degrade to the ordinary pre-image capture).

**Inverse synthesis per class** (all guards: current window snapshot ==
recorded post snapshot, elementwise on `(id, deleted-state)`; mismatch
→ the typed divergence refusal, reusing `UndoField::HabitLogCount`'s
slot renamed — add `UndoField::HabitEvents`; stage 2 verifies whether
`UndoField` is persisted and applies the additive/retired treatment
accordingly):

- `LoggedHabit` / `SkipHabit` / `UnskipHabit`: the minted id = the id
  present in the recorded post snapshot and absent from the recorded
  pre snapshot (exactly one per entry — the per-increment law).
  Inverse = `TombstoneHabitEvent { event: minted_id }`. Exact by
  construction — no `AdjustHabitLog` reuse, no `count: 0` forging, no
  bump re-stamp. The old comment block's count-only-vs-whole-row guard
  analysis (`undo.rs:2520–2540`) dies with the row grammar; the whole
  snapshot compare is safe now because the inverse no longer re-stamps
  the compared row (it tombstones a different fact).
- `AdjustHabitLog`: pre = D-J1's snapshot; the adjust's own effects =
  post-snapshot deltas. Inverse = `TombstoneHabitEvent` for every id
  the adjust created + `RestoreHabitEvent` for every id whose
  tombstone state the adjust changed (restore to the recorded pre
  state). The absent-window case restores emptiness (empty pre
  snapshot, all created ids tombstoned) — D24's successor, exact.
- **Convergence refinement of design §3.4** [recorded deviation]: the
  design's adjust-inverse says "delete synthesized events"; this plan
  **tombstones** them instead — a local `BTreeMap` delete cannot
  survive a peer union re-import (the chunk-4 fold would resurrect the
  row), while a tombstone is the deletion fact family the fold
  understands (Ruling C2). Derived counts are identical; convergence
  is preserved.
- `UpdateHabit` (seam edits included): the ordinary `PreImage::Habit`
  restore path, now also covering `continued_from` edits (D25 dead).
- **Legacy leg**: entries recorded before this chunk (`HabitLogRow`
  pre-images) keep undoing through the *unchanged* legacy inverse
  paths — the mirror keeps the legacy map truthful, so the legacy
  guards (`app.habits.log` compares) stay sound until the last legacy
  entry ages out of the journal retention window. No migration of
  journal entries; no weakening of the legacy undo tests.
- `undo.rs:643–649` board mapping: unchanged (habit-keyed commands
  already map by habit id).
- `undo.rs` habit-log helpers (`habit_log_key`, `habit_log_post`,
  `:1438–1444`): stay for the legacy leg; event-class helpers added
  alongside (snapshot extraction/comparison).

### D-Msg — Receipts (`messages.rs`, design §7 row 14)

`CommandOutcome::HabitLogged`/`HabitLogAdjusted`/`HabitSkipped` gain
the event id(s) per D-Log/D-Adj/D-Skip; `HabitUnskipped` is new. The
doc comments restate the semantics (one minted event; the reconcile
diff names its events). Stage 2 verifies persistence of `CommandOutcome`
and applies `#[serde(default)]` to the new fields if (and only if) a
persisted payload embeds the outcome.

### D-Tests — the snapshot-test catalogue (journey-step tokens)

Naming is normative (UX plan §6 W-rows / §8); the gate is path-sensitive,
so paths matter. The **engine halves** land in this chunk; the
CLI/presentation halves named by W-3/W-5's `habits.rs` path stay
MISSING until chunk 5 — reported honestly, never papered over (the
chunk-1 precedent: D11).

| Token | Test | File | Pins |
|---|---|---|---|
| `us1_s1_log_habit_mints_one_event` | unit | `command.rs` (W-1 path verbatim) | one dispatch → exactly one `UpsertHabitEvent` with a fresh `Completion`; receipt `matches!` on the minted id; mirror row +1 |
| `us2_s1_adjust_reconciles_by_tombstones` | unit | `command.rs` | derived 3 → adjust 1: the two newest tombstone (newest-first order pinned), one `UpsertHabitEvent` per tombstone; receipt names them |
| `us2_ac2_absent_window_adjust_applies` | unit | `command.rs` (US-2 AC-2) | absent window, adjust to 1: mints one synthesized event; the D24 refusal has no successor |
| `us2_ac3_adjust_to_zero_derives_zero` | unit | `command.rs` | adjust to 0 tombstones all window events; derived 0; count-0 ≡ row-absence |
| `us2_s2_adjust_preimage_restores_exactly` | unit | `undo.rs` (W-5 second path verbatim) | undo of an adjust restores the exact pre tombstone-state set, including un-tombstone and tombstone-of-synthesized |
| `us3_s1_week_undo_tombstones_event_id` | unit | `undo.rs` (W-8 path verbatim) | a Week-window log undone: exactly the receipt's id tombstoned; derived count and day attribution drop it (the #1341 phantom class pinned dead at the engine layer) |
| `us3_ac2_undo_rederives_every_projection` | insta snapshot | `taskboard-state/tests/snapshots.rs` | log → undo on a Week habit; full `AppState` snapshot: events, mirror row, day overlay all consistent post-undo |
| `us5_s1_skip_mints_mark_event` / `us5_s2_unskip_mints_mark_event` | unit | `command.rs` (W-6 path verbatim) | mark events minted; derived posture flips; accepted no-ops against the derived posture |
| `us1_ac2_same_second_logs_stay_distinct` | unit | `command.rs` | two same-second `LogHabit` dispatches → two events, derived 2 (engine-layer twin of chunk 1's T3) |
| `union_keeps_both_concurrent_logs` | unit | `command.rs` tests (the chunk-4 fold's seed; #1094 evidence) | two independent engines each run one `LogHabit` on the same habit/window; union the two event maps by id (the fold law one-liner); derived count 2 — both survive |
| `legacy_mirror_equals_event_derivations` | proptest | `command.rs` tests | **the keystone** (D-C): random log/adjust/skip/unskip/undo sequences; after every step `log` ≡ `derive_window_counts`, `day_log` ≡ `derive_day_counts`, `skip` bits ≡ `derive_skip_posture` |
| `remap_gate_negative_condition_holds` | unit | `command.rs` tests | D-R's pinned property: any state containing a synthesized (adjust-minted) event has a non-empty events map — the chunk-3 remap can never see one |
| fence-deletion evidence | compile + unit | `journal.rs`/`undo.rs` | zero classifiers reference `HabitSurface` (grep evidence in the commit body); Reflection/Reminder arms untouched (diff-evident) |

Existing suites re-based, not weakened: `command.rs` habit tests
(`:11899+` — the `RecordHabitLog` arm assertions gain the event-action
assertions), `persistence.rs` apply tests (`:1693+` — the Record-pair
LWW tests stay; new upsert-by-id tests added), `taskboard-state`
`suite/habits.rs`, `suite/undo.rs`, `snapshots.rs`, the app CLI habit
tests (behavior deltas only where D24/D25/receipts deliberately
changed — each such change cites the deliberate delta in the commit
body), UI journey tests (expected untouched — the mirror keeps their
fixture/read paths truthful; if any fails, it is a keystone-law bug:
evaluate both sides per AGENTS §5).

### D-Snap — the insta procedure

The chunk-1 precedent: 51 snapshots gained `"events": {}`. This chunk's
churn is larger but still mechanical: snapshots whose command sequences
include log/adjust/skip/undo gain populated `events` entries (mirror
rows keep their old shape — by D-C's design). Procedure: run the state
snapshot suite via the flock-wrapped gate; inspect every pending
snapshot with `cargo insta review` (or `cargo insta accept` only after
a full diff read — the reviewer's choice, but *never* blind accept);
each snapshot change is cited in its commit body with the behavior
change that caused it (dispatch Prompt 2.2's rule). No snapshot
asserting a value the plan did not predict is accepted without a
recorded reason.

---

## 2. Implementation order (TDD)

Each numbered step is at least one standalone commit (`<type>(<module>):
<desc>` + body; red/green within the commit).

1. **feat(domain): the UpsertHabitEvent action** — D-P; persistence.rs
   variant + apply arm (replace-by-id) + tests (upsert, tombstone-ride,
   replace semantics); sqlite inert partition arm; existing
   persistence tests green.
2. **feat(domain): LogHabit mints one Completion event** — D-Log +
   D-Msg receipts + `us1_s1` + `us1_ac2`; `mint_log_clock` retained for
   the mirror only (comment states the retirement boundary).
3. **feat(domain): AdjustHabitLog as the reconcile diff** — D-Adj +
   `us2_s1`/`us2_ac2`/`us2_ac3`; the gate stays (a gate test exists
   already — keep green).
4. **feat(domain): Skip/Unskip mark events** — D-Skip + `us5_s1`/
   `us5_s2` + derived-posture no-op tests.
5. **feat(domain): the convergence seed** — `union_keeps_both_
   concurrent_logs` + `remap_gate_negative_condition_holds`.
6. **test(domain): the keystone mirror proptest** — D-Tests
   `legacy_mirror_equals_event_derivations` (after 2–4 it must pass
   as-is; if not, a mirror bug — fix production, never the test).
7. **refactor(domain): the journal event grammar** — D-J1 shapes
   (serde round-trips; retired-shape decode tests).
8. **refactor(domain): the D24/D25 fence deletions** — D-J2; the
   behavior-identity-relevant **dedicated commit** (dispatch Prompt
   2.2's rule); grep evidence in the body; `HabitPolicy` re-homing.
9. **feat(domain): exact inverses** — D-U: the two synthesis commands +
   the event-class inverse paths + `us3_s1` + `us2_s2`.
10. **test(state): snapshot re-basing** — `us3_ac2` + the re-based
    state-suite tests; insta procedure per D-Snap.
11. **test(app/state): deliberate-delta re-basing** — app/state tests
    where D24/D25/receipts changed behavior; each cited.

Then the fast gate (§4) and the ux-coverage gate (§5).

---

## 3. Risks

- **R1 — RQ-1/RQ-2 are operator-overridable**: if the dispatcher
  overrules the mirror or the HabitSurface retirement, stage 2 re-opens
  this plan's §0/D-C or RQ-2 sections only (the dispatch plan's own
  overrule law).
- **R2 — transitional sqlite non-persistence of events** (D-P): a
  chunk-2-main restart drops events until chunk 3 lands. Serial merge
  before release makes this unshippable-state only; recorded, not
  mitigated.
- **R3 — the mirror/keystone law is load-bearing**: every untouched
  reader's truthfulness rests on it; the proptest is the guard. A
  failure there is a production bug by default (AGENTS §5).
- **R4 — serde archaeology**: `CommandOutcome` / `UndoField`
  persistence status is unverified at plan time; stage 2 verifies and
  applies the additive/retired treatment; a surprise here is diff-sized,
  not design-sized.
- **R5 — undo guard strictness**: the whole-snapshot compare refuses on
  any interleaved window write (adjust → log → undo-of-adjust refuses —
  correct, the pre-state is gone). Same posture as today's row guards;
  the burst-undo deadlock analysis of `undo.rs:2520–2540` must be
  re-derived for the snapshot grammar — stage 3 reviews it (focus (a)).
- **R6 — snapshot churn volume**: larger than chunk 1's 51; every
  change must trace to a predicted behavior change (D-Snap).

## 4. Estimated diff size

- `command.rs`: ~250 lines arms + ~350 test lines.
- `undo.rs`: ~250 + ~200 tests. `journal.rs`: ~80. `persistence.rs`:
  ~40 + ~40 tests. `messages.rs`: ~30. `habit.rs`: ~10 (comment
  boundaries only). `repo.rs`: ~10. `taskboard-state` tests/snapshots:
  ~250 (incl. snapshot churn). `taskboard-app` tests: ~60.
- **Total: ~1300–1500 lines** across `taskboard-domain` (bulk),
  `taskboard-state`, `taskboard-storage-sqlite` (one arm),
  `taskboard-app` (tests only).

## 5. Gates (long-command discipline — binding for stages 2 and 3)

Identical to the chunk-1 plan §5 (verbatim origin: dispatch plan §1
shared execution model; docs/agent_dispatch.org). Additionally: run
`scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md
--gate --root <the worktree>` — **always pass `--root`; invoked from a
worktree without it the script silently scans the main checkout
(#1531)**.

---

## 6. Stage-2 (IMPLEMENTATION) prompt — verbatim, paste-ready

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the habit event-log dispatch ("commands, undo, journal"). Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-domain-habit-event-commands (branch feat/domain-habit-event-commands); every command by absolute path; never modify the main checkout /home/viktoria/src/taskboard (it resolves to /home/viktoria/nextcloud/viktoria/programming/taskboard); never touch another taskboard-WT-* directory.

**Long-command discipline (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (3) Every agent-launched cargo invocation routes through scripts/memory-gate.sh (the 2026-11-23 systemd-oomd law) — nothing bare. (4) Out-of-scope findings become `backlog:` GitHub issues (or comments on existing ones) the moment they are discovered, without asking (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.**

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings B, C, H)/§3/§7 (rows 7–10, 14), the UX journey plan .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md (US-1..US-3, US-5 engine halves; §6 W-row paths; §7 command-contract delta), the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§3, and the stage-1 plan .artifacts/plans/2026-11-27-habit-event-commands-plan.md (all four are in the worktree) — then implement exactly per the stage-1 plan's §1 decisions (D-C, D-R, D-P, D-Log, D-Adj, D-Skip, D-J1, D-J2, D-U, D-Msg, D-Tests, D-Snap) and §2 order. If the worktree is missing, recreate it: git -C /home/viktoria/src/taskboard worktree add -b feat/domain-habit-event-commands ../taskboard-WT-feat-domain-habit-event-commands origin/main; if that branch's PR already landed, use feat/domain-habit-event-commands-v2 per AGENTS §8 (never `git worktree add <path> main`, never a slash-suffixed worktree path).
2. TDD, every commit standalone (rebase-merge hygiene). Hard rules: SPDX headers on every new .rs file; #![forbid(unsafe_code)] (crate-wide already); no tokio::time::sleep in tests — pure fixtures; no text assertions (typed matches!/structural facts only); no weakening a correct test (AGENTS §5: a failing new test may be a production bug — evaluate both, leave it failing and report if the fix is out of scope); insta snapshots updated deliberately, each change citing the behavior change in the commit body (the stage-1 plan D-Snap procedure — never a blind accept); the D24/D25 fence deletion is behavior-identity-relevant — a dedicated commit with the grep evidence; the two recorded deviations from the design (D-P's one-action swap; D-U's tombstone-not-delete synthesized-event inverse) and the RQ-1 mirror/RQ-2 retirement stand as the plan records them — do not re-litigate them, implement them.
3. Scope fences (dispatch §3 non-goals + the plan): production UI crate untouched; production app CLI untouched (app TEST re-basing where D24/D25/receipts deliberately changed behavior is in scope, each cited); no sqlite persistence of events (the inert repo partition arm per D-P is the only sqlite line); no wire fold work; no uuid v5 feature; RecordHabitLog/RecordHabitDayLog stay (the mirror); mint_log_clock stays (mirror-only role); the legacy journal inverse paths stay for pre-#1520 entries.
4. Run the fast gate per the long-command discipline above: pre-commit hooks automatic; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls, useful work between polls, all work committed BEFORE any wait; every cargo invocation through scripts/memory-gate.sh; the coverage trio never runs in this session; flock deferral = commit + "gate deferred: lock held by <holder>" + end.
5. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate --root /home/viktoria/src/taskboard-WT-feat-domain-habit-event-commands (ALWAYS --root: invoked from a worktree without it the script silently scans the main checkout — issue #1531). Every row this chunk claims must read FOUND where the plan's D-Tests table lands the token at the W-row's verbatim path; the W-3/W-5 habits.rs CLI-half paths (us2_s1's habits.rs entry) stay MISSING until chunk 5 — report the full honest steady state, never paper over a MISSING.
6. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
7. Backlog issues per the discipline above; list the issue numbers in your report.

Report: per-gate results, snapshot-churn justification (per-snapshot cited reasons), the D-R invariant + keystone proptest evidence, any deviation from the plan and why.
```

## 7. Stage-3 (REVIEW) prompt — verbatim, paste-ready

```text
You are the stage-3 REVIEW agent for chunk 2 of the habit event-log dispatch (branch feat/domain-habit-event-commands). Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-domain-habit-event-commands; every command by absolute path; never modify the main checkout /home/viktoria/src/taskboard; never touch another taskboard-WT-* directory.

**Long-command discipline (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, the `mergeStateStatus CLEAN` verification, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh (the 2026-11-23 systemd-oomd law) — nothing bare. (5) Out-of-scope findings become `backlog:` GitHub issues (or comments on existing ones) the moment they are discovered, without asking (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.**

1. **Begin by rebasing onto main**: git fetch origin --prune, then rebase the branch onto origin/main (AGENTS §8's rebase dance: drop already-merged duplicate commits; resolve only real conflicts), push --force-with-lease if the rebase rewrote history. Then read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings B, C, H)/§3/§7, the UX journey plan, the dispatch plan §1/§3 (the code-quality mandate below applies to you), and the stage-1 plan .artifacts/plans/2026-11-27-habit-event-commands-plan.md; then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD) in the worktree.
2. Functional focus, named from the origin: (a) the inverse must be EXACT — undo of LogHabit/Skip/Unskip tombstones exactly the minted id extracted from the entry's pre/post snapshots; the adjust pre-image restores tombstone state exactly (un-tombstone + tombstone-of-synthesized included); no approximation on Week/Month (the #1341 phantom-cell class must be pinned dead by us3_s1 at the engine layer); re-derive the burst-undo guard analysis for the snapshot grammar (the stage-1 plan R5) — consecutive undos of a same-window burst must not deadlock, and any interleaved write must refuse; (b) the reconcile diff must never leave a state where chunk 3's remap could see a synthesized event — verify the remap_gate_negative_condition_holds test enforces the stage-1 plan D-R invariant; (c) the keystone mirror proptest (legacy_mirror_equals_event_derivations) really drives undo through the sequence (not just forward writes) and asserts all three projections — it is the load-bearing law for every untouched reader; (d) concurrent-increment convergence seed: two plans, union keeps both (#1094 dissolution evidence); (e) HabitSurface has zero classifier references (grep evidence) with the RQ-2 retired-variant discipline honored (decode-only; PauseHabit/converts re-homed to HabitPolicy); Reflection/Reminder fences untouched; (f) Ruling H's current-kind window gate kept as the typed InvalidHabitWindow rejection; CLI grammar unchanged; (g) the scope fences held: production UI/app-CLI untouched, no sqlite persistence of events, no wire work, no uuid v5, Record pair + mint_log_clock still present (mirror roles); (h) no RwLock/Mutex/orchestrator shortcuts (AGENTS §3); (i) no weakened tests — every changed test cites a deliberate plan-recorded behavior delta (D24/D25 deaths, receipt fields, mirror law), and the legacy journal inverse paths survive for pre-#1520 entries.
3. **Code-quality mandate**: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crate; the derivation fold never leaves the domain crate; the UI never computes habit semantics); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the fast gate yourself per the long-command discipline (pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace, backgrounded with polls, work committed before any wait; flock deferral rule applies; every cargo invocation through scripts/memory-gate.sh). The coverage trio never runs in this session.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship — **your scope ends at PR-open; do not remove the worktree**: push the branch (--force-with-lease only, never plain --force), open the PR to main with gh, then report the URL — do NOT run gh pr checks --watch; the dispatcher owns CI watch, merge-state verification, teardown of this worktree, and at-merge issue closures. Never merge; never touch main.
7. **UX completeness mandate**: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate --root /home/viktoria/src/taskboard-WT-feat-domain-habit-event-commands (ALWAYS --root — #1531); then execute the walkthrough protocol (docs/ux_planning.org) over every P1 story this chunk implements (US-1, US-2, US-3, US-5 engine halves) — the four walkthrough questions per step with evidence, then the six-heuristic pass over the touched command surfaces (the engine command layer; UI surfaces are chunk 5's walkthrough scope); BLOCKER/MAJOR findings are fixed on the branch, MINOR findings become backlog issues.
8. Backlog issues per the discipline above; list the issue numbers in your report.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, walkthrough verdict. Never merge; never touch main; do not remove the worktree.
```
