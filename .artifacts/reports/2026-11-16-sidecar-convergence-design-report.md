# [Evidence Report] Sidecar convergence design round — the F1 local-author leg (#708)

- **Date**: 2026-11-16
- **Kind**: evidence report, correctness-hygiene round chunk G
  (stage-2 research). Companion design plan:
  `.artifacts/plans/2026-11-16-sidecar-convergence-design-plan.md`.
- **Origin**: round plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §8;
  dispatch plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md` §8
  (chunk G); stage-1 chunk plan
  `.artifacts/plans/2026-11-16-sidecar-convergence-design-chunk-plan.md`;
  issue #708 (`backlog:`, p1, architecture).
- **Code state**: all anchors re-verified by symbol against the branch
  tip `18dbd74d` (= `origin/main` @ `dfdf2562` + the stage-1 chunk-plan
  commit; zero production-code delta). Line numbers below are from
  that tree; every anchor was opened and read, not copied from the
  stage-1 plan.

## 1. Method

1. **Anchor re-verification**: every file:line claim in §2 was
   re-anchored by symbol (`grep` + read) in the worktree
   `../taskboard-WT-docs-sidecar-convergence-design` at `18dbd74d`.
2. **Behavioral characterization**: a scratch probe crate
   (`/tmp/sidecar-probe`, outside the repo tree per the chunk plan's
   hard boundary) drives the *real* domain fold functions
   (`taskboard_domain::merge::fold_reminder_rules`,
   `::fold_habit_log`) and the real document emission builder
   (`taskboard_domain::appdata::rules_entry_for`) with synthetic
   author ranks. Repro:

   ```sh
   # crate at /tmp/sidecar-probe: path dep on
   # crates/taskboard-domain @ 18dbd74d; cargo run prints the matrix
   CARGO_TARGET_DIR=<worktree>/target cargo run --manifest-path /tmp/sidecar-probe/Cargo.toml
   ```

   Full output excerpted verbatim in §4. The probe never lands; the
   PR diff for this chunk is exactly the two artefact files.
3. **What the probe does** (so the matrix is reproducible from this
   section alone): writers at author ranks 1 and 3 emit
   equal-clock (`x`) document rows for the same rule id with distinct
   contents (`DueOffset(-10min)` vs `DueOffset(-20min)`); readers at
   ranks 0 (< min(1,3)), 2 (interleave), 4 (> max(1,3)) and the two
   writers themselves fold the two rows in both arrival orders into a
   fresh `BTreeMap`; the survivor's content is printed. The same
   shape runs against `fold_habit_log` (counts 1 vs 2 at the same
   clock). Two follow-up probes: (a) the converged interleave reader
   re-emits its document via `rules_entry_for(&map, task, reader)`
   and the emitted `by` is printed; (b) the adoption-amnesia cell —
   a reader at rank 5 that adopted rank 3's row then folds a third
   doc by rank 4 at the same clock, compared against the honest rank-3
   writer folding the same doc.
4. **Formalization** (§5) is derived from the duel predicates' exact
   arithmetic (the total order `(clock, author)` with
   `None < Some`), cross-checked against the probe matrix.

## 2. Current-behavior inventory

All anchors at `18dbd74d`, `crates/taskboard-domain/src/` unless
noted.

### 2.1 The duel legs

- `merge.rs:141-150` — `authored_doc_wins(doc_clock, doc_by,
  local_clock, local_author) = (doc_clock, doc_by) >= (local_clock,
  local_author)`: doc wins pull-side ties. The doc-comment states the
  fence explicitly: "`local_author` is the folding device's own
  identity".
- `merge.rs:152-159` — `authored_tombstone_wins(...)`: strict `>` on
  the whole tuple (a tie never deletes).
- `merge.rs:1317` — `fold_habits`'s live-row duel is *inlined*, not
  the helper: `(row_clock, *by) > (effective_clock(local),
  local_author)` — strict `>` (ties keep local), on the effective
  clock (max of the five embedded clocks; `merge.rs:1338-1346`).
- `merge.rs:1416-1417` — `fold_habit_log`'s duel, also inlined
  strict `>`: `None => true` (adopt), `Some(local) => (*clock, *by) >
  (local.clock, local_author)`.

### 2.2 The five fold families (the read path)

| family | body | update duel | tombstone duel | adoption arm (None) |
|---|---|---|---|---|
| `fold_reminder_rules` | `merge.rs:871` | `:905` (`>=`, helper) | `:939` (helper) | `:928-941`, no `by` stamped |
| `fold_break_rules` | `merge.rs:1033` | `:1060` (helper; duels on `created_at`) | `:1089` (helper) | `:1071-1083`, no `by` |
| `fold_time_entries` | `merge.rs:1180` | `:1211` (helper) | `:1244` (helper) | `:1223-1239`, no `by` |
| `fold_habits` | `merge.rs:1284` | `:1317` (inlined strict `>`) | `:1359-1366` (inlined strict) | `:1345-1358`, no `by` |
| `fold_habit_log` | `merge.rs:1401` | `:1416-1417` (inlined strict `>`) | n/a (no tombstones) | `:1416` `None => true`, no `by` |

In every adoption arm the row is inserted with the document's clock
but **without** the document's `by` — the state structs have no slot
to put it in (§2.5). The comment at `merge.rs:898-902` names the
fence: "The sidecar entities carry no author slots (F1's fence), so
the local leg is the folding device's own identity, passed in."

### 2.3 The author plumbing (where the wrong leg enters)

- `pipeline.rs:458` — `apply_sync_report(..., author: Option<AuthorId>)`:
  the *folding device's* identity, threaded to the sidecar fold
  section at `pipeline.rs:959-1037`: `:995` (per-card rules +
  dismissals via the `SectionFolds` helper), `:1021`
  (`fold_time_entries`), `:1027` (`fold_habits`), `:1028`
  (`fold_habit_log`), `:1037` (`fold_break_rules`).
- The re-sync entry `apply_files_board_snapshot`
  (`pipeline.rs:1776-1944`) threads the same `author` through the
  same folds at `pipeline.rs:1930-1944`.
- The `SectionFolds` helpers (`pipeline.rs:1428-1587`) forward
  `author` verbatim (`:1482-1490`, `:1520-1530`, `:1542-1552`,
  `:1560-1569`, `:1577-1587`).
- The fold deltas become `PersistenceAction`s and are applied to the
  memory view in the same pass (`pipeline.rs:1491-1493`,
  `:1531`, `:1553`, `:1588`) — batch and view cannot drift.

### 2.4 The write paths (where `by` is stamped)

Document emission sites stamp `by: author` — **the flushing device's
identity, unconditionally**; no stored author exists to consult
(`crates/taskboard-domain/src/appdata.rs`):

- rules + rule tombstones: `:1861`, `:1904`, `:1959`, `:2004`
- time entries + tombstone: `:2135`, `:2220`
- habit rows, habit-live/tombstone, break rules + tombstone:
  `:2271`, `:2276`, `:2317`, `:2378`, `:2410`

Codec legs: the shared `by` decode is default-absent → `None`, and a
present-but-unparseable `by` **demotes the whole row** — "never a
silent de-authoring: the fold's `None < Some` leg would change rank
against the author-carrying peer" (`appdata.rs:293-299`). Authorless
rows stay byte-identical to pre-F1 documents (`appdata.rs:543-549`:
"authorless rows stay byte-identical to the pre-F1 documents").

Local flush-side creation of sidecar rows stamps the state entity
with clocks only — `command.rs:709` (recur-spawn
`UpsertReminderRule`), `command.rs:2219-2229` (`AddReminderRule`):
`created_at: now, updated_at: now`, no author (structurally
impossible, §2.5).

### 2.5 The state side (the amnesia surface)

The five roaming sidecar row states carry **no author field**:

- `reminders.rs:435` `ReminderRule` (id, task, kind, insistence,
  created_at, updated_at)
- `reminders.rs:509` `BreakRule` (id, board, label, threshold,
  insistence, created_at)
- `tracking.rs:38` `TimeEntry` (id, task, task_title, start, end,
  created_at, updated_at, kind)
- `habit.rs:186` `Habit` (id, board, title, target, order,
  anchor_time, clocks, continued_from, deleted)
- `habit.rs:278` `HabitLogEntry` (count, clock)

Construction sites that would gain a stamp under the fix: the fold
adoption arms (§2.2), the local creation sites (`command.rs:709`,
`:2219-2229`, and the same shape for break rules / time entries /
habits in their respective command arms), and the fold-delta
apply pass (`pipeline.rs:1491-1588`).

### 2.6 Storage (the persistence surface)

- Migration `0023_clock_author_columns.sql:14-35` added 23 `au_*`
  nullable columns **only** to `tasks`/`stacks`/`labels`/`boards`.
  None of the sidecar tables (`reminder_rules` — migrations
  `0004`/`0013`/`0020`; `time_entries` — `0015`; `habit_trackers` —
  `0017`; `break_rules` — `0018`/`0021`) has an author column. A
  fold-adopted row therefore persists authorless across restarts —
  the amnesia survives disk.
- **Migration numbering drift vs the stage-1 plan**: the chunk plan
  §3 said "migration 0024 or amend 0023 pre-release", but `0024`
  (`0024_roaming_settings.sql`) and `0025`
  (`0025_board_remote_tag.sql`) have landed. A new sidecar-author
  migration is **0026**; 0023 is shipped history and cannot be
  amended. This is recorded as a stage-1 plan deviation (§7).
- The next free migration number is verified:
  `ls crates/taskboard-storage-sqlite/migrations/` → 0001..0025.

### 2.7 The fence statement (ADR 0022, quoted)

`docs/adr/0022-boards-over-files-contract-f1-author-clocks.org:77-81`
(the *Fences kept* paragraph):

> *Fences kept*: membership duels (`(t, author)` generalization) and
> `remote_seen` author carriage are chunk 5's territory; the
> reflection/habit *storage* families stay device-local (ADR 0021) —
> their sidecar rows still gain the `by` duel leg because the rows
> roam in documents.

The fence governs *storage persistence* (device-local families keep
no author columns); it is why the fold's local leg had to be the
device identity. The F1 plan's old-reader relay note
(`.artifacts/plans/2026-11-07-f1-author-clocks-plan.md` §2.6)
recorded the pre-F1 relay as tolerable ("content and clock survive,
the fold's content leg still converges") — §4 below shows the
*F1-device* relay is not tolerable: it relays a wrong `by`.

## 3. Convergence-law formalization

**The law over sidecar folds (restated from F1 plan §3, specialized):
for any multiset of sidecar document facts** `{(clock_i, by_i,
content_i)}` **for one row id, any folding device d (any author
rank, any arrival order, any partition of the facts into cycles, with
or without restarts) terminates with the same row state, and emits
the same `by` on its next document flush.**

Lemma decomposition (each must hold for the law to hold):

- **L1 (writer convergence)**: two devices folding each other's facts
  converge to the same survivor. *Holds today*: verified for equal
  clocks by probe (§4, writer rows) and structurally — the duel is a
  total order on `(clock, author)`, and both writers' legs are their
  honest identities, so `max((clock, author))` wins on both.
- **L2 (third-reader convergence)**: any third reader converges to
  the same survivor regardless of arrival order. *Violated today* for
  readers outside the writers' author band (§4, matrix rows
  3-6): arrival order decides the survivor.
- **L3 (adoption fidelity)**: after adopting a row, the reader's
  local leg for future duels is the row's true author. *Violated
  today* (§4, amnesia rows): the leg silently becomes the reader's
  own identity, so L2 breaks again on the *next* cycle even for
  readers the first cycle happened to converge for — and the
  reader's next flush relays the wrong `by` (L4's input).
- **L4 (relay fidelity)**: a converged reader's re-emitted document
  row carries the survivor's true author. *Violated today* (§4,
  relay row): the emission builders stamp the flushing device's
  identity (`appdata.rs:1861` et al.), so the divergence propagates
  to any superset of peers relaying the reader's document — the
  end-to-end harm named in #708.

**Exactly one violated lemma family** (the #708 defect), with two
mechanistically distinct defects both rooted in the missing state
slot: the **duel-leg defect** (L2: the local leg is the reader's
identity instead of the row's author) and the **adoption-amnesia
defect** (L3/L4: the true author is discarded at adoption and
re-forged at flush). They are separable: a fold that stamped
adoption but still dueled on the reader's identity would fix L3/L4's
state but not L2; fixing only the duel comparison would not stop the
wrong `by` relay. The design plan must address both, which is why
the stored-slot axis (§6) is the only complete candidate.

## 4. Characterization results (the ordering matrix)

Probe output, verbatim (authors are `AuthorId` UUIDs ranked by their
final byte; equal clock `x` for every doc; distinct contents):

```text
== fold_reminder_rules, equal clock x, writers rank1(-10) / rank3(-20) ==
  writer rank1 (leg 1): survivor content = Some(-10)
  writer rank3 (leg 3): survivor content = Some(-20)
  reader rank0 <min, order A,B: survivor content = Some(-20)
  reader rank0 <min, order B,A: survivor content = Some(-10)
  reader rank4 >max, order A,B: survivor content = Some(-10)
  reader rank4 >max, order B,A: survivor content = Some(-20)
  reader rank2 interleave, order A,B: survivor content = Some(-20)
  reader rank2 interleave, order B,A: survivor content = Some(-20)
== relay: converged reader C (adopted B's row) re-emits its document ==
  emitted by = Some(AuthorId(00000000-0000-0000-0000-000000000000)) (true author was rank3)
== adoption amnesia, second cycle: writer rank3, doc D rank4, reader rank5 (adopted rank3's row) ==
  rank3 device (honest leg 3) after D(rank4) doc: survivor = Some(-30)
  rank5 reader (amnesic leg 5, true author rank3) after D doc: survivor = Some(-20)
  control: same reader rank5 folding D first-arrival (no local): Some(-30)
== fold_habit_log (strict > duel), equal clock, rank1 count 1 / rank3 count 2 ==
  reader rank0, order A,B: count = Some(2)
  reader rank0, order B,A: count = Some(1)
  reader rank4, order A,B: count = Some(1)
  reader rank4, order B,A: count = Some(2)
  reader rank2, order A,B: count = Some(2)
  reader rank2, order B,A: count = Some(2)
```

(The relay row's `AuthorId(…0000)` is the probe's rank-0 stand-in for
the reader's own identity — the point is that the emitted `by` is
*the reader's* identity, not the true author rank 3.)

Matrix reading:

| cell | survivor | law verdict |
|---|---|---|
| writers themselves | max author's content, both | **L1 holds** |
| reader below both writers | **last arrival wins** (both docs outrank the reader's leg) | **L2 violated**, order-dependent |
| reader above both writers | **first arrival wins** (neither doc outranks the reader's leg) | **L2 violated**, order-dependent |
| reader interleave min(a,b) < c < max(a,b) | **converges to max(a,b)'s content in both orders** | L2 holds *accidentally* (mechanism below) |
| adoption + same-clock third doc ranked between true author and reader | writer → third doc; amnesic reader → keeps first survivor | **L3 violated** |
| reader re-emission after adoption | `by` = reader's identity, true author lost | **L4 violated** |

**The interleave case (the stage-1 untested cell) converges — by
accident, not by correctness.** Mechanism: for `a < c < b`, the
higher writer's doc `(x, b)` outranks the reader's leg `(x, c)`
(win on second arrival) while the lower writer's doc `(x, a)` does
not (lose on second arrival) — both arrival orders funnel to
`max(a, b)`. The same holds for the strict-`>` duel of
`fold_habit_log` (last matrix block). This is order-insensitive only
for *equal clocks*; it is still a fence-riding coincidence: the
reader's identity sits inside the writers' band and acts as an
accidental correct barrier. Any author-set where the reader falls
outside the band diverges (the two violated rows). The design must
not rely on the interleave accident (a test pinning "reader rank 2
converges" would pin the fence, not a law).

**Self-healing check (the litmus discipline)**: none of the violated
cells self-heals. Terminal row states differ across readers with no
further writes; a subsequent cycle re-folds only documents, whose
ranks now include the reader's re-forged `by` (L4), which can
additional lock in the divergence rather than heal it (the relayed
wrong author outranks the true author's rank on peers above the
reader).

**Tie-direction asymmetry (tertiary finding)**: the
`>=`-tie-to-document legs (`merge.rs:141-150`, used by rules/break/
time-entries) and the strict-`>`-keeps-local legs (`merge.rs:1317`,
`:1416-1417`, habits/habit-log) disagree on full-tuple ties. A full
tie requires `doc_by == local_author`, which for a *third* reader is
only reachable through the L4 relay defect (a peer re-emits an
adopted row under its own identity, and that peer is the reader).
Today this is unreachable for honest documents, but Axis A (stored
slots) makes the stored `by` the leg, at which point the tie
direction must be unified deliberately (the design plan §4 fixes
this as "ties keep local", matching `authored_tombstone_wins` and
the strict-`>` inline legs).

**Scope note**: the same matrix shape applies to all five families —
four of the five share the exact helper predicates (§2.2 table) and
the fifth (`fold_habit_log`) differs only in strictness at ties
(unreachable per above). `fold_task_app_data` is unaffected (per
#708: the entity carries per-field author slots; the pipeline
T-PROP-1 journey pins it). No hosted-server claims are made — the
defect is purely file-side (round plan §8's curl-evidence requirement
does not engage; no server behavior is asserted anywhere in this
report).

## 5. Why the writers' law holds but the third reader's doesn't (the one-paragraph proof)

The duel compares `(clock_i, by_i)` against `(clock_local,
leg_local)`. For a *writer*, `leg_local` is the honest author of the
local row, so the duel computes `max` over the true `(clock, author)`
pairs — a total, permutation-invariant order (L1). For a *third
reader*, `leg_local` is the reader's rank `c` (duel defect) or the
reader's rank regardless of the row's history (amnesia defect). Then
the survivor is `argmax` over a set where one element's rank was
substituted with `c`: if `c > max(by_i)` every doc beats the local
row on arrival → last arrival wins; if `c < min(by_i)` no doc beats
it → first arrival wins; only when `min(by_i) < c < max(by_i)` does
the substitution coincidentally preserve the `max` (the interleave
accident). A rank substitution inside a total-order `argmax` is not
order-invariant except in that band — hence L2/L3 fail exactly and
only outside it.

## 6. Design options & trade-offs

(Candidate axes per chunk plan §3; the full decision record with
alternatives-not-chosen lives in the design plan §4.)

**Axis A — stored author slot on the five sidecar row states** (the
#708 proposed shape): `by: Option<AuthorId>` added to `ReminderRule`,
`BreakRule`, `TimeEntry`, `Habit`, `HabitLogEntry` (definition sites
§2.5); folds duel on the stored slot; adoption stamps the document's
`by` (fixes L3); flush emits the stored `by` with the device identity
as the local-creation fallback (fixes L4); persisted as one nullable
`au_*` column per sidecar table (migration **0026**, §2.6 — not 0024
as the stage-1 plan assumed). Fixes L1-L4 completely. Costs: crosses
the ADR 0022 *Fences kept* storage fence (§2.7) for the
reflection/habit families — requires an ADR amendment (or a
carve-out note); widens the storage surface to five tables' worth of
columns + codec rows. Consequences evidenced: the fence text itself
names the storage families as the governed surface
(`0022:77-81`); the migration is additive-nullable (0023's precedent,
`0023_clock_author_columns.sql`), no data rewrite.

**Axis B — derive-on-read adoption memo**: keep the state rows
authorless; maintain a side table `row_id → author` at adoption
time, consult it as the duel leg and at flush. Fixes L2-L4 *if* the
memo persists correctly, but the memo is a second source of truth
with its own lifecycle: it must be persisted (new table — same
migration cost as Axis A), garbage-collected on row deletion, and
kept transactionally consistent with the rows it shadows (the fold
deltas apply through `PersistenceAction`s, `pipeline.rs:1491-1588` —
a second parallel delta stream). It also leaves the fence intact
nominally while actually persisting authorship, which is the fence
with extra steps. Strictly dominated by Axis A: same migration
surface, more machinery, an extra invariant (memo ↔ row) to
maintain and proptest.

**Axis C — ordering-insensitive local leg**: change the duel to
ignore the local author leg entirely (clock-only, ties broken by
content hash or row id). Converges by construction (single-key
`max`), zero migration. Costs: rewrites the duel semantics F1 just
shipped (D-BF2's `(clock, author)` total order, `merge.rs:141-150`
and ADR 0022's contract paragraph); breaks the authorless-lane
byte-compat argument (T-DUEL-0 pins authorless Deck duels to
pre-F1 bit-for-bit behavior — a clock-only tie-break by content
changes outcomes for equal-clock authorless rows); and does *not*
fix L4 (the relay still re-forges `by`), so the end-to-end harm of
#708 survives. Fixes the symptom, not the amnesia.

**Migration/compat dimension (orthogonal)**: for each of the five
families — new migration 0026 with one nullable `au_*` column per
sidecar table (read rule: absent/NULL → `None`, the
`appdata.rs:293-299` wire rule mirrored on disk; existing on-disk
sidecar rows decode authorless and duel at rank `None`).
`None < Some` means an authorless persisted row loses every
equal-clock duel against an author-carrying doc — the safe default
(never privileges old data, matches the F1 `None < Some` leg,
`merge.rs:141-150`). Old F1 documents that already carry `by` keys
decode unchanged; pre-F1 documents decode authorless. Mixed fleets
(old readers relay rows `by`-less at their re-stamp) keep the F1
plan §2.6 behavior — content converges, the wrong-`by` relay is
bounded to F1-capable readers and dies as fleets upgrade; the design
plan states this residual honestly rather than claiming full-fleet
convergence.

## 7. Recommendation

**Axis A** — the stored author slot, migration 0026, ADR 0022
amended (the *Fences kept* storage sentence gains a dated carve-out:
the five roaming sidecar families persist a nullable author column;
ADR 0021's device-locality keeps governing the *reflection entry*
families that do not roam in documents). Axis B is strictly
dominated; Axis C fails L4 and breaks the D-BF2 contract. Full
design (state diffs per family, duel/adoption/flush changes,
migration DDL, fence text, test catalogue) → the companion design
plan. The interleave accident and the tie-direction unification are
called out there as must-pin/ must-decide items.

## 8. Stage-1 plan deviations found during execution

1. **Migration numbering**: the chunk plan §3 offers "0024 or amend
   0023"; both are dead — 0024/0025 have landed (§2.6), and 0023 is
   shipped history. The design plan uses 0026.
2. **The interleave cell** (chunk plan §2.2 "may actually converge by
   accident; characterize it, don't assume"): it *does* converge for
   equal clocks, in both duel styles (§4) — recorded per-cell rather
   than assumed, as the plan required.
3. Everything else in the stage-1 inventory verified anchor-for-anchor.

## 9. Human-gate handoff

Per round plan §8, this design round **ends at the operator's human
gate**: the operator must accept §7's recommendation (or pick a
recorded alternative) before any dispatch may chunk the
implementation. #708 stays open through the gate; the dispatcher
comments this report and the design plan on it at handoff.

## 10. References

- Issue #708 (defect record + proposed fix shape; stage-1 findings
  comment, 2026-11-16)
- Round plan `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §8
- Dispatch plan `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md` §8, §1
- Stage-1 chunk plan `.artifacts/plans/2026-11-16-sidecar-convergence-design-chunk-plan.md`
- F1 plan `.artifacts/plans/2026-11-07-f1-author-clocks-plan.md` §2.4-2, §2.6, §3, §4
- ADR 0022 `docs/adr/0022-boards-over-files-contract-f1-author-clocks.org` (Fences kept, :77-81)
- `crates/taskboard-domain/src/merge.rs`, `pipeline.rs`,
  `appdata.rs`, `command.rs`, `reminders.rs`, `tracking.rs`,
  `habit.rs`; `crates/taskboard-storage-sqlite/migrations/0023..0025`
