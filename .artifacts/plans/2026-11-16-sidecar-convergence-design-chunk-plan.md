# [Chunk Plan] Chunk G — sidecar-convergence design round (stage 1, planning)

- **Date**: 2026-11-16
- **Kind**: research-chunk stage-1 plan (correctness & hygiene round,
  chunk G) — the plan that refines the round plan §8 and dispatch
  plan §8 into a concrete characterization method, formalization
  approach, design axes, and artefact skeletons for the stage-2
  research agent.
- **Origin**: round plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §8;
  dispatch plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md`
  §8 (incl. the verbatim evidence & artefact mandate); issue #708
  (`backlog:`, p1, architecture); F1 design context:
  `.artifacts/plans/2026-11-07-f1-author-clocks-plan.md` §2.4-2/§2.6/§3,
  ADR `docs/adr/0022-boards-over-files-contract-f1-author-clocks.org`
  (the *Fences kept* paragraph, lines 72–89).
- **Status**: PLANNED — stage-2 executes this method verbatim; stage-3
  reviews against it.
- **Hard boundary (stated normatively)**: **zero production code in
  this chunk.** The deliverable PR contains exactly the evidence
  report and the design plan under `.artifacts/` — no `.rs` changes,
  no migrations, no test changes. Any experiment code (probes,
  reproductions) is scratch outside the repo tree; only excerpts and
  outputs land, inside the report.

## 1. Scope & non-goals

**In scope**: characterize the broken local-author leg of the five
sidecar fold families (`fold_reminder_rules`, `fold_break_rules`,
`fold_time_entries`, `fold_habits`, `fold_habit_log`); formalize the
§3 equal-clocks-different-authors law over sidecar folds; develop
candidate designs with trade-offs; select exactly one, with a
migration/compat story for existing on-disk sidecars; end at the
operator's human gate.

**Non-goals** (recorded, then to backlog if they surface as work):
implementing the author slot; touching `fold_task_app_data` (unaffected
per #708 — per-field author slot already exists; the T-PROP-1 journey
pins it); boards-over-files generalization (chunk-5 dependency of the
F1 plan); the hosted-server behavior claims (this bug is purely
file-side; no curl evidence needed, though the report's evidence
section states why).

## 2. Characterization method (stage-2 execution spec)

### 2.1 The write-path inventory (file:line anchors)

All anchors verified against `origin/main` @ `dfdf2562` (worktree
`../taskboard-WT-docs-sidecar-convergence-design`). Stage-2 re-anchors
by symbol before trusting any line number.

**Read path (the fold — where the law breaks).** Pipeline position:
`crates/taskboard-domain/src/pipeline.rs:959-1037` (§5b sidecar fold,
after the task-merge loop; fold order: per-card rules then dismissals,
then board-level time entries, habits, habit log, break rules) and the
re-sync entry at `pipeline.rs:1930-1944`. The `author` plumbing — the
folding device's own identity passed as the local duel leg — enters at
`pipeline.rs:458` and is threaded at `pipeline.rs:995, 1021, 1027,
1028, 1037` and `pipeline.rs:1482-1587` (the `SectionFolds` helpers).
The fold bodies: `crates/taskboard-domain/src/merge.rs:871`
(`fold_reminder_rules`), `:1033` (`fold_break_rules`), `:1180`
(`fold_time_entries`), `:1284` (`fold_habits`), `:1401`
(`fold_habit_log`). The broken leg is the tuple duel
`authored_doc_wins` / `authored_tomb_wins` at `merge.rs:137-158`: the
document's `(clock, by)` is compared against `(local_clock,
local_author)` where `local_author` is *the folding device's own
identity* (the comment at `merge.rs:897-902` states the fence
explicitly). Two sub-cases to characterize separately:

- **Update duel** (`merge.rs:911-929` in `fold_reminder_rules`, and the
  analogous arms in the other four): a locally-present row fights the
  document row with the wrong local author.
- **Adoption arm** (e.g. `merge.rs:930-941`): a row absent locally is
  adopted *without* stamping the document's `by` — the adoption
  forgets the true author, so the *next* cycle's duel leg is wrong too
  (state-level amnesia, not just a one-cycle divergence).

**Write paths (where `by` is stamped on document rows).** Document-row
emission sites stamp `by: author` (the writing device's identity):
`crates/taskboard-domain/src/appdata.rs:1861, 1904, 1959, 2004`
(rules + rule tombstones), `:2135, 2220, 2271-2317, 2378, 2410` (time
entries, break rules, habit rows + tombstones). Local flush-side
creation of sidecar rows (which must stamp the device identity but
today stamp nothing on the *state* row — the state entity has no
author slot): `crates/taskboard-domain/src/command.rs:709`
(`PersistenceAction::UpsertReminderRule`), `:2219` (state-side rule
construction). Storage codec (author decode/demote leg):
`appdata.rs:231-234, 295-299` (the shared `by` decode legs; "silent
de-authoring" prohibition), row author fields at `appdata.rs:469-478,
714, 790-797, 964-1000, 1129`; authorless rows stay byte-identical
pre-F1 (`appdata.rs:543`).

**State side (the amnesia surface).** The sidecar row states
(`ReminderRule`, `BreakRule`, `TimeEntry`, `Habit`, `HabitLogEntry`)
carry no author field — the F1 fence per ADR 0022 lines 72–89. Stage-2
inventories each struct's exact definition site and every construction
site (state engine commands, fold adoption arms, push echo restamps)
as the migration surface for the design.

### 2.2 Orderings and observation model

Characterize with a three-device model, mirroring and extending the
#708 probe:

- **Writers**: device A and device B write distinct contents `x`, `y`
  at the *same* clock for the same sidecar row id. Verify: A and B
  each converge to the same winner (the writers' law holds).
- **Third reader C**, identity `c`:
  - `c < min(a, b)` and `c > max(a, b)` (the #708 probe covered both),
  - `min(a,b) < c < max(a,b)` (not yet covered — the interleave case
    may actually converge by accident; characterize it, don't assume),
  - arrival orders: doc-A-then-doc-B and the reverse, across a
    restart (state persisted between arrivals — exposing the adoption
    amnesia vs the one-shot duel as *distinct* defects).
- **What C observes**: the survivor row's content and, on the next
  cycle after flushing C's own document emission, what `by` C writes
  back — the divergence propagating into C's emitted document is the
  end-to-end harm (a relayed superset of peers then sees C's wrong
  rank).
- **Method**: one scratch two/three-engine test harness per ordering
  (reuse the T-PROP-1 journey harness shape from F1 chunk 5); the
  probe runs are recorded as repro commands in the report, the harness
  itself never lands.
- **Litmus discipline**: the report distinguishes *law violation*
  (divergent terminal states across readers) from *transient
  divergence that self-heals* — only the former is the defect; if some
  ordering self-heals, say so with the mechanism.

### 2.3 Formalization approach

State the convergence law over sidecar folds as a predicate: for any
multiset of document facts (rows + tombstones with `(clock, by,
content)`), any folding device (any identity, any arrival order, any
partition of facts into cycles, with or without restarts) terminates
with the same sidecar row state — and emits the same `by` on its next
flush. Prove the current implementation satisfies it for same-author
facts and exhibits exactly one violated lemma for equal-clock
different-author facts (the forgotten-adoption lemma). The formal
statement goes in the report §3 with the violated lemma named; the
design plan then states the *fixed* predicate and how the chosen
design restores it (candidate oracles: a proptest over the five folds
with randomized author sets and arrival permutations — the T-DUEL-2
extension F1 §4 anticipated).

## 3. Candidate design axes (stage-2 develops, exactly one recommended)

1. **Axis A — stored author slot (the #708 proposed shape)**: add
   `by: Option<AuthorId>` to the five sidecar row states; folds duel
   on the stored slot, adoption stamps the document's `by`, flush
   emits the stored `by` (device identity fallback for locally
   created rows), persist for every roaming family. Trade-offs:
   crosses the ADR 0022 fence (storage families "device-local"),
   widens migration surface (0024 or amend 0023 pre-release); the
   only shape that fixes the adoption-amnesia defect at its root.
2. **Axis B — derive-on-read**: keep state rows authorless; folds
   accept an "adopted-author memo" side table (id → author) maintained
   at adoption time. Smaller state change, but the memo is a second
   source of truth with its own persistence problem — likely worse
   than Axis A; characterize honestly.
3. **Axis C — ordering-insensitive local leg**: change the duel to
   ignore the local author leg entirely (clock-only, ties broken by
   content hash or document id) — converges by construction but
   rewrites the duel semantics F1 just shipped (D-BF2) and breaks the
   authorless-lane byte-compat argument; likely rejected, recorded as
   the alternative that avoids migration entirely.
4. **Migration/compat dimension (orthogonal to A–C)**: for each roaming
   family, decide the migration vehicle (corrected 2026-11-16, stage
   3, per #858: 0024/0025 have landed and 0023 is shipped history —
   the vehicle is a *new* migration, realized as 0026 in the design
   plan, not "0024 vs amend 0023" as first written), the
   read rule for pre-author columns (absent → `None`; the
   `appdata.rs:295-299` demote leg already defines the wire rule),
   and the compat story for existing on-disk sidecars (a persisted
   authorless row duels at rank `None` — is that the safe default?
   state it, don't assume).

The recommendation must cover: which axis, the migration choice per
family, the fence decision (amend ADR 0022's *Fences kept* vs carve an
exception), and the test-oracle story (which laws become proptests,
which become two-engine journeys).

## 4. Artefact skeletons

### 4.1 Evidence report — `.artifacts/reports/2026-11-16-sidecar-convergence-design-report.md`

1. **Method** — how every claim below was produced; probes reproducible
   from this section alone.
2. **Current-behavior inventory** — the §2.1 write-path inventory with
   file:line for every fold, duel leg, emission site, and state
   construction site; the fence statement quoted from ADR 0022.
3. **Characterization results** — the ordering matrix of §2.2 with
   observed survivors per cell; which cells violate the law, which
   self-heal and why; the adoption-amnesia defect separated from the
   duel defect.
4. **Convergence-law formalization** — the §2.3 predicate, the proven
   sub-case, the single violated lemma.
5. **Design options & trade-offs** — axes A–C plus the migration
   dimension, each with evidence-backed consequences.
6. **Recommendation** — exactly one, with consequences for later work
   (fence amendment, migration, test catalogue sketch).
7. **Human-gate handoff** — the report names the gate: operator
   acceptance precedes any implementation dispatch (per round plan §8).

### 4.2 Design plan — `.artifacts/plans/2026-11-16-sidecar-convergence-design-plan.md`

1. Header (date, kind, origin = the report, status PROPOSED pending
   the gate).
2. Scope + the restated zero-production-code boundary *for this
   chunk* (the plan is the implementation blueprint for the *next*
   round).
3. Verified anchors (§2.1 inventory, re-anchored by symbol).
4. Design: the chosen axis in full — state-shape diff per family, fold
   duel change, adoption stamp, flush emission, storage columns +
   migration, fence amendment text.
5. The restored convergence predicate + test catalogue (proptest
   laws, two-engine journeys, storage roundtrip, restart ordering).
6. Risks & mitigations (byte-compat, migration ordering vs release
   state, interleave-case surprises from §2.2).
7. Out-of-scope discoveries → issues (standing authorization).
8. References.

## 5. Risks for stage-2

- **Anchor drift**: `main` moves; re-anchor by symbol, record the new
  commit hash in the report.
- **The interleave case** (§2.2) may show accidental convergence — do
  not fit the formalization to assume total breakage; report per-cell.
- **Scratch-harness leakage**: probes live outside the tree; the PR
  diff is exactly the two artefacts (dispatch plan §8 acceptance).
- **Scope creep into implementation**: any "while we're here" code fix
  is a violation of the hard boundary; it goes to the design plan or
  a backlog issue instead.

## 6. References

- Round plan §8: `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md`
- Dispatch plan §1/§8: `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md`
- Issue #708 (the defect record + proposed fix shape)
- `.artifacts/plans/2026-11-07-f1-author-clocks-plan.md` §2.4-2, §2.6, §3, §4
- `docs/adr/0022-boards-over-files-contract-f1-author-clocks.org` (fences)
- `docs/architecture.org` (sidecar substrate context)
- `crates/taskboard-domain/src/merge.rs`, `pipeline.rs`, `appdata.rs`,
  `command.rs` (anchors in §2.1)
