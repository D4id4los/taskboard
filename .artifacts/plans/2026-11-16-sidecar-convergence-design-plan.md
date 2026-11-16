# [Design Plan] Sidecar convergence — the stored author slot for the five roaming sidecar families (#708)

- **Date**: 2026-11-16
- **Kind**: design plan (correctness-hygiene round, chunk G stage 2).
- **Origin**: the evidence report
  `.artifacts/reports/2026-11-16-sidecar-convergence-design-report.md`
  (§6 options, §7 recommendation — Axis A); issue #708; round plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §8;
  dispatch plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md`
  §8.
- **Status**: PROPOSED — pending the operator's human gate (round
  plan §8: the operator accepts the design before any dispatch may
  chunk its implementation; #708 stays open through the gate).
- **Zero-production-code boundary (restated)**: this chunk landed no
  code (the PR is exactly the two artefact files). *This plan is the
  implementation blueprint for a future round*; nothing below has
  been executed.

## 1. Scope

Fix the third-reader convergence defect in the five sidecar fold
families (`fold_reminder_rules`, `fold_break_rules`,
`fold_time_entries`, `fold_habits`, `fold_habit_log`) by giving the
five roaming sidecar row states a stored author slot, dueling on it,
stamping adoption, emitting it at flush, and persisting it per
family. Restores lemmas L2-L4 of the evidence report's §3
formalization (L1 already holds). `fold_task_app_data` is untouched
(per-field author slots exist; the T-PROP-1 journey pins it). The
boards-over-files generalization stays chunk-5 territory.

**Non-goals** (→ backlog issues if they surface as work): per-device
attribution UI (#494 territory); fixing old-reader relays in mixed
fleets (residual, §6); the membership-duel / `remote_seen` fences
(ADR 0022 chunk 5).

## 2. Verified anchors

All at `18dbd74d` (see evidence report §2 for the full annotated
inventory; the load-bearing ones restated):

- Duel helpers: `merge.rs:141-150` (`authored_doc_wins`, `>=`),
  `:152-159` (`authored_tombstone_wins`, strict `>`); inlined strict
  duels `merge.rs:1317` (habits), `:1416-1417` (habit log).
- Fold bodies + adoption arms: `merge.rs:871/:905/:928-941/:939`
  (rules), `:1033/:1060/:1071-1083/:1089` (break rules),
  `:1180/:1211/:1223-1239/:1244` (time entries),
  `:1284/:1317/:1345-1358/:1359-1366` (habits),
  `:1401/:1416-1417` (habit log).
- Author plumbing: `pipeline.rs:458, 995, 1021, 1027, 1028, 1037,
  1482-1587`; re-sync `:1930-1944`; fold-delta apply
  `:1491-1588`.
- Emission sites (`by: author`, flushing device's identity):
  `appdata.rs:1861, 1904, 1959, 2004, 2135, 2220, 2271, 2276, 2317,
  2378, 2410`.
- `by` decode leg + demote rule: `appdata.rs:293-299`; byte-compat:
  `appdata.rs:543-549`.
- State structs (gain the slot): `reminders.rs:435` `ReminderRule`,
  `reminders.rs:509` `BreakRule`, `tracking.rs:38` `TimeEntry`,
  `habit.rs:186` `Habit`, `habit.rs:278` `HabitLogEntry`.
- Local creation sites (stamp the device identity):
  `command.rs:709`, `command.rs:2219-2229`, plus the break-rule /
  time-entry / habit creation arms in their respective
  `StateCommand` planners.
- Storage: sidecar tables have no author columns
  (`migrations/0023_clock_author_columns.sql:14-35` touches only
  tasks/stacks/labels/boards); next free migration is **0026**
  (0024/0025 landed — the stage-1 plan's "0024 or amend 0023" is
  dead, evidence report §8).

## 3. Design decision (exactly one; the operator's gate)

**Axis A — the stored author slot**, chosen over Axis B
(adoption-memo side table: strictly dominated — same migration
surface, a second source of truth with a memo↔row invariant) and
Axis C (clock-only duel: converges by construction but rewrites
D-BF2, breaks the T-DUEL-0 authorless byte-compat pin, and does not
fix the L4 relay defect). Full trade-off record: evidence report §6.
Migration: new migration 0026 (amending shipped 0023 is not
possible; 0024 is taken). Fence: ADR 0022 amended, §5 below.

## 4. The design in full

### 4.1 State-shape diff (per family)

Each of the five structs gains exactly one field:

```rust
/// The row's author (F1's `by` leg, the stored duel rank): the
/// document's `by` when adopted, the device identity when created
/// locally, `None` for rows that predate the slot (never privileged —
/// the `None < Some` leg keeps them honest). Documented leniency:
/// `None` here means "author unknown", not "authored by nobody".
pub by: Option<crate::ids::AuthorId>,
```

`ReminderRule` (`reminders.rs:435`), `BreakRule`
(`reminders.rs:509`), `TimeEntry` (`tracking.rs:38`), `Habit`
(`habit.rs:186`), `HabitLogEntry` (`habit.rs:278`). Every in-tree
construction site gains the field: local creation arms stamp the
planner's `author` argument (the same seam the F1 plan threaded,
`pipeline.rs:458`); fold adoption arms stamp the document row's `by`;
tests stamp fixtures explicitly. No derive crates — the plain field
with the doc-commented leniency is the house A1 shape.

### 4.2 Fold duel change

The local duel leg becomes `local.by` instead of the threaded
`local_author` parameter:

- `merge.rs:905, 939, 1060, 1089, 1211, 1244` — call sites pass
  `local.by`; the folds' `local_author` parameters are **removed**
  from all five signatures, and with them the `author` threading at
  `pipeline.rs:995, 1021, 1027, 1028, 1037, 1482-1587, 1930-1944`
  for the sidecar folds (the `SectionFolds` helpers shrink; the
  pipeline `author` parameter itself stays — the flush builders and
  `fold_task_app_data` still need it).
- `merge.rs:1317, 1416-1417` — the inlined strict duels read
  `local.by` / `entry.by`.
- **Tie direction is unified to "ties keep local"** (strict `>` on
  the whole tuple) for the update duel as well: `authored_doc_wins`'
  `>=` tie-to-document direction (`merge.rs:141-150`) becomes strict
  for the sidecar call sites (a new
  `authored_doc_wins_strict`, or the helper gains the direction as
  an argument — implementer's choice, pinned by test). Rationale:
  with stored slots, a full tuple tie means "the same author wrote
  the same clock twice" — the local row already holds that exact
  write, so keeping local is identity-neutral and matches
  `authored_tombstone_wins` and the inline legs (evidence report §4,
  tie-direction asymmetry). The pre-slot behavior the `>=` encoded
  (doc beats an authorless local row at equal clock) is preserved by
  the rank law itself: `Some(_) > None`.
- The fold-delta apply pass (`pipeline.rs:1491-1588`) is
  field-wise unchanged — the rows now carry the slot through
  `PersistenceAction` into storage.

### 4.3 Adoption stamp

Every five `None =>` adoption arms (`merge.rs:928-941, 1071-1083,
1223-1239, 1345-1358, 1416-1425`) construct the row with
`by: *by` (the document row's author). This is L3's fix: the next
cycle's duel leg is the true author, not the reader's identity.

### 4.4 Flush emission

The emission builders (`appdata.rs` §2 sites) change from
`by: author` to `by: row.by.or(author)` — the stored slot when the
row has one, the flushing device's identity as the fallback for
locally-created rows that raced a flush, `None` only in
identity-less headless contexts (unchanged semantics). This is L4's
fix: an adopted row is re-emitted under its *true* author's rank, so
the relayed superset converges. The builder signatures keep taking
`author` (the fallback), so the pipeline plumbing above the builders
is untouched.

### 4.5 Storage + migration

Migration **0026_sidecar_author_columns.sql**, one nullable
`TEXT` column per sidecar table (the 0023 columnar precedent,
`0023_clock_author_columns.sql:14-35`):

- `reminder_rules.au_by`, `break_rules.au_by`,
  `time_entries.au_by`, `habit_trackers.au_by` (verify the habit-log
  table's name at implementation time — `0017_habit_trackers.sql`
  vs the log's own table if separate),
- codec rows read `(row, au_by)` pairs, `NULL → None`; writes emit
  the column from the struct slot (the repo.rs pattern of
  `repo.rs:59-126`),
- roundtrip proptest: slot survives disk both directions,
  old-DB → new codec lossless (the T-STORE-1/2 shape),
- **no data rewrite**: existing authorless rows keep `NULL`.

### 4.6 Fence amendment text (ADR 0022)

The *Fences kept* paragraph (`0022:77-81`) gains a dated sentence:

> *Amended 2026-11-16 (#708 design round)*: the five roaming
> sidecar families (`reminder_rules`, `break_rules`, `time_entries`,
> `habit_trackers`, `habit_log`) persist one nullable author column
> (`au_by`) — device-locality under ADR 0021 continues to govern the
> reflection-entry families that do not roam in documents; the roam
> requirement (the fence's own rationale) now carries the author
> with the row.

## 5. The restored convergence predicate + test catalogue

**Restored predicate** (evidence report §3, all four lemmas): for
any multiset of sidecar facts, any device, any arrival order, any
cycle partition, with restarts — the same survivor row state
(content *and* stored author), and the same `by` on the next flush.
The interleave band's accidental convergence (evidence report §4)
stops being load-bearing: with stored slots the duel is a true
`max` over `(clock, by)` for every reader, inside and outside the
band.

- **T-PROP-2' (proptest, the law)**: extend F1 §4's T-DUEL-2 to all
  five folds: randomized author sets (0-4 distinct authors),
  equal and unequal clocks, distinct contents, randomized arrival
  permutations across 1-3 cycles, and a restart between cycles
  (state through the storage roundtrip). Oracle: identical survivor
  `(content, by)` for every permutation and every reader identity —
  including identities outside and inside the writers' band (the
  band is explicitly generated so the interleave accident can never
  silently regress).
- **T-ADOPT-1 (unit per family)**: adopting a doc row stores its
  `by`; the next cycle's duel uses the stored slot (the amnesia
  cell of evidence report §4 becomes a regression test: writer
  rank b, doc D rank d, reader rank c with b < d < c — reader must
  converge with the writer).
- **T-RELAY-1 (two-engine journey)**: A and B write equal-clock
  distinct contents; C folds both (any order), flushes its document;
  D (fresh) folds C's document and A's/B's — D converges to the
  same survivor (L4 end-to-end).
- **T-TIE-1 (unit)**: full-tuple tie keeps local, all five folds
  (the unified direction, §4.2).
- **T-STORE-x (roundtrip)**: slot survives the SQLite roundtrip;
  `NULL` decodes `None`; an authorless persisted row duels at rank
  `None` and loses every equal-clock duel against `Some` (the safe
  default, evidence report §6 migration dimension).
- **T-MIG-1**: migration 0026 on a copy of a pre-0026 database
  succeeds; all rows decode authorless; no clock text changes.
- **Byte-compat pin (retained)**: emission still omits the `by` key
  when `None` (`appdata.rs:543-549` discipline); authorless documents
  stay byte-identical.
- Journey-level: the existing T-PROP-1 / T-SIDE-2 F1 journeys stay
  green (the slot is additive); the restart-ordering journey from
  the characterization (§4's matrix rows 3-6) becomes the
  two-engine acceptance journey.

## 6. Risks & mitigations

- **Byte-compat of documents**: mitigated by the retained
  omit-when-`None` emission rule (T-STORE/T-BYTE pins); old readers
  ignore the key (F1's mixed-fleet pin).
- **Mixed fleets keep a bounded residual**: a pre-slot reader still
  relays rows `by`-less at its re-stamp (F1 plan §2.6 behavior).
  Full-fleet convergence is only reached once every relaying device
  carries the slot — stated honestly, not claimed away; the
  T-RELAY-1 journey models one old reader and documents the
  outcome.
- **Migration ordering vs release state**: 0026 is purely additive
  nullable columns; it can land in any release after this design is
  accepted, independently of 0023-0025 (no interaction). The only
  sequencing constraint: the code and 0026 land in the same release
  (the structs' new field must not exist before the column — a
  mid-upgrade reader writes rows the older column-less code would
  drop the slot of; tolerated, authorless rows are rank-`None` safe
  defaults, but the same-release rule keeps it clean).
- **Interleave-case surprises**: the band accident is characterized
  (evidence report §4) and neutralized by the true-`max` duel; the
  proptest generates in-band reader identities explicitly so the
  accident can never be load-bearing again.
- **Signature churn**: removing `local_author` from five fold
  signatures touches the pipeline and every fold test — mechanical,
  compiler-guided; no behavior change beyond the duel leg.

## 7. Out-of-scope discoveries → issues

(Standing authorization; numbers reported in the chunk's final
report and listed here at implementation time. None new beyond the
evidence report's findings — the tie-direction asymmetry and the
migration-numbering drift are folded into §4.2/§2 above, and the
interleave accident is covered by T-PROP-2'.)

## 8. References

- Evidence report `.artifacts/reports/2026-11-16-sidecar-convergence-design-report.md`
  (options §6, recommendation §7, formalization §3, matrix §4)
- Issue #708; round plan §8; dispatch plan §8; stage-1 chunk plan
- F1 plan `.artifacts/plans/2026-11-07-f1-author-clocks-plan.md`
  §2.4-2/§2.6/§3/§4 (the law, the wire shape, the test catalogue)
- ADR 0022 `docs/adr/0022-boards-over-files-contract-f1-author-clocks.org`
  (D-BF2, Fences kept :77-81); ADR 0021 (device-local families)
