# Chunk plan — B1: the LogHabit equal-clock tie (#986)

Round 3, chunk B1 · branch `fix/domain-loghabit-tie` · stage-1 planning
artifact. Normative inputs: the scope report
`2026-11-19-backlog-reduction-round3-scope-report.md` §B1, the dispatch
plan `2026-11-19-backlog-reduction-round3-dispatch-plan.md` §1/§6, issue
#986, the r9 H1 plan (`2026-11-18-habit-domain-fields-plan.md` §4).

## 1. Problem statement

Two `LogHabit` commands stamped with the same planner `now` (same
second — double-tap, scripted CLI) mint two `RecordHabitLog` actions
carrying the identical row clock. Every apply site is strictly-newer
LWW with an equal-clock keep-stored tie:

- `crates/taskboard-domain/src/persistence.rs` `apply_actions`
  (`stored.clock >= entry.clock => {}` — keep stored);
- `crates/taskboard-storage-sqlite/src/repo.rs` habit-log upsert
  (`WHERE excluded.clock > habit_log.clock`);
- `crates/taskboard-domain/src/merge.rs` `fold_habit_log`
  (tuple duel `(clock, by)`, strict direction keeps local at a full
  tie).

Result: the second increment is silently dropped on replay. Pinned as
debt by `habit.rs::log_same_instant_double_currently_pins_the_lww_tie_loss`;
the Σ property
`log_sequences_are_monotone_and_partition_attribution` excludes the
degenerate sequence via a `dt >= 1s` step clock.

## 2. The tie rule — recorded decision

**Decision: mint-side per-command monotone clock bump (the issue's
"per-command monotone tiebreaker" direction).** `LogHabit` and
`AdjustHabitLog` both mint their row clock as

```
mint_log_clock(now, stored_clock) = max(now, stored_clock + 1s)
```

where `stored_clock` is the currently stored row's clock for the same
`HabitLogKey` (`None` → `now`). One shared pure helper in
`crates/taskboard-domain/src/habit.rs`; both command arms in
`command.rs` call it.

Giving `AdjustHabitLog` the same bump is load-bearing, not symmetric
polish: its arm's own contract is "newest-write under LWW by
construction", and without the bump a same-second or just-after-second
adjust could stamp *older* than a bumped log row (burst of 3 logs at
`T` → row clock `T+2s`; an adjust at `T+1s` would lose). The bump
preserves the adjust's newest-wins intent under the new mint rule.

### Why this candidate (the determinism argument)

The law the rule must satisfy: **deterministic replay across replicas**
— the same journal/fold inputs applied on any replica, in any
interleave of local replay and remote fold, converge to the same state.

- **Mint-side bump**: the bumped clock is a pure function of
  (pre-state, command, `now`). Replay re-executes commands in the same
  order over identical pre-states, so identical clocks are re-minted —
  replay is deterministic by construction. All three apply sites stay
  strictly-newer LWW; nothing at the apply/fold/SQL layer changes.
  The fold duel stays order-independent for rows produced under the
  rule (distinct minted clocks within a device lineage; cross-device
  residue below).
- **Additive tie (merge equal-clock increments in apply) — rejected.**
  `apply_actions` receives only `PersistenceAction::RecordHabitLog`;
  a LogHabit increment and an AdjustHabitLog absolute set mint the
  *same action type with the same row shape* — apply cannot tell them
  apart, and an additive tie corrupts audited corrections (adjust-to-2
  twice at the same second → 4). Distinguishing provenance requires a
  new `HabitLogEntry` field: a journal + wire + sqlite shape change.
  It also breaks commutativity with the fold's wholesale adoption
  (a lower-count-newer-clock legitimate correction would sum instead
  of replace).
- **Sub-second precision stamps — rejected.** The planner `now` is
  second-resolution by contract (fixtures, stamps, the property's step
  clock all carry second resolution); plumbing sub-second `now`
  changes row-clock semantics for *every* entity, and the sqlite
  guard's RFC 3339 lexicographic compare mis-orders fractional seconds
  (`…00.5Z` < `…00Z` as strings). Cross-device same-second concurrent
  increments remain LWW-lossy anyway.
- **Sequence-ordinal field — rejected.** Needs a `HabitLogEntry`
  ordinal: serde/journal shape change, sqlite column (migration 0028),
  and a wire-record change (`HabitLogRecord::Entry` gains a field).
  Pre-field builds ignore the ordinal and resolve the tie differently
  → mixed-version replicas diverge, and the ordinal duel remains
  order-dependent across devices regardless.

### Migration verdict (the #843/#913 precedent, stated explicitly)

**This domain rule change needs NO migration.** No schema, journal, or
wire shape changes: the row clock column already carries an arbitrary
RFC 3339 `DateTime<Utc>`, the SQL guard compares it lexicographically
(exact whole-second values compare correctly), the wire
`HabitLogRecord::Entry` is unchanged, and old journals replay correctly
(they never contained a tie to begin with — the bump only affects
freshly minted rows). Old journals written *after* this change replay
identically on new builds; new journals replay on new builds only (no
downgrade concern beyond the existing posture). The ruling changes
**row-clock value semantics**, which roam on the wire: a device's log
row clock may exceed the wall `now` by whole seconds after a
same-second burst. That leniency is documented at the type and at both
command arms.

### Residual (accepted, filed as backlog)

Two *different devices* independently logging the same habit window
within the same second still resolve by the fold's `(clock, by)` duel
— one increment is lost on merge. Genuinely concurrent increments need
a per-author counter merge (G-Counter style), which is a wire-shape
change and out of this chunk's scope. Filed as a `backlog:` issue.

## 3. The Σ property — degenerate sequence joins (round law (c))

The `dt >= 1s` exclusion is **removed**. In
`habit.rs::log_sequences_are_monotone_and_partition_attribution`, the
step-strategy range becomes `0_i64..=3_600` (dt = 0 legal). The
property's existing assertions then hold unmodified for degenerate
sequences under the mint rule: every LogHabit's action carries
`entry.count == pre + 1` (the bump guarantees the row lands), the
Σ-over-windows invariant is unchanged, and the windows/receipt
assertions never read the row clock. No exclusion, no weakening — the
property strictly strengthens.

## 4. Test changes (TDD order)

1. **Flip the pin to desired semantics** (AGENTS §5 naming law):
   rename `log_same_instant_double_currently_pins_the_lww_tie_loss` →
   `log_same_instant_double_log_increments_twice` and assert the
   stored total is **2** (typed outcome on the summed count, as today).
   The doc comment stops being a DEBT PIN and states the mint rule
   instead. Also update the property's step-clock comment (the
   exclusion rationale) and the `RecordHabitLog` apply-site comment in
   `persistence.rs` (the tie remains keep-stored — the comment's
   "deterministic" claim stands; only its "the planner's now" premise
   moves to the mint).
2. **New unit test for the mint helper**: a fresh key mints `now`; a
   stored clock at `now`, `now+1s`, and `now-5s` mints
   `stored+1s`, `stored+1s`, `now` respectively (property-free
   deterministic table — lowest layer).
3. **Command-arm pins**: two same-instant `LogHabit`s produce strictly
   increasing row clocks (second is `T`, third is `T+1s`); an
   `AdjustHabitLog` at `T+1s` after a 3-burst at `T` lands (its clock
   `T+2s` > stored `T+2s` → `T+3s`) — the adjust-survives pin.
4. **Existing command.rs LogHabit tests** around
   `command.rs:10145–10423` assert receipt counts, not clocks — verify
   and leave untouched unless one pins `clock == now` (none found at
   planning time; if one surfaces, it flips with the rule, named as
   desired semantics).

No SQL/storage-layer test changes: the guard is unchanged and its
equal-clock test in `repo.rs` still passes.

## 5. Implementation steps

1. Branch worktree (done at stage 1); this plan commit.
2. Stage 2: TDD per §4 order — flip pin (red) → mint helper + arm
   wiring (green) → Σ exclusion removal → property run (green, 64
   cases) → doc comments (`HabitLogEntry.clock`: "the writing
   planner's `now`, monotonically advanced past any stored same-key
   log row (#986) — may exceed `now` by whole seconds after a
   same-second burst; documented leniency").
3. Stage 2 gate: pre-commit hooks +
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (background + poll; defer per the contended-lock rule).
4. Stage 3: review per the dispatch plan §6 code-quality mandate;
   UX-coverage gate not applicable (no user-visible verb/badge/view
   change — engine internals only).

## 6. Risks / watch items

- **Row clocks ahead of wall time roam the wire**: another device's
  fold sees a future-dated clock. Harmless under LWW (it *is* the
  newest write), but a `clock`-derived debug/telemetry view could look
  odd; no projection reads habit-log clocks today (verified: windows
  come from the key, streaks never read the clock).
- **Undo-journal replay**: a replayed LogHabit re-mints from the
  then-current state — for the burst scenario replay order is
  preserved and clocks re-derive identically; an interleaved adjust's
  clock also re-derives identically. Pinned by the existing
  journal round-trip tests staying green.
- **Clock drift accumulation**: a pathological same-second burst of N
  logs puts the row at `now + (N−1)s`. Bounded by real input patterns;
  the 180-day retention GC is date-keyed, not clock-keyed — no
  interaction.
- Out-of-scope observations → `backlog:` issues (§7 standing
  authorization); see §7.

## 7. Backlog issues

Filed at planning stage (numbers listed in the stage report):

- Cross-device concurrent same-second LogHabit increments (per-author
  counter merge / G-Counter) — the residual the mint bump cannot fix.
- Habit-log SQL guard's RFC 3339 lexicographic clock compare is only
  correct for whole-second clocks — a hazard note if sub-second
  precision is ever ruled.

## 8. Diff estimate

~120–180 lines across `command.rs` (helper + two arms, ~20),
`habit.rs` (pin flip, property strategy + comments, mint-helper tests,
~80), `persistence.rs` (comment, ~4). No schema, no wire, no UI.
