# [Chunk Plan] Axis A — the stored sidecar author slot (#708)

- **Date**: 2026-11-16
- **Kind**: stage-1 TDD chunk plan for the ACCEPTED design
  `.artifacts/plans/2026-11-16-sidecar-convergence-design-plan.md`
  (§4 normative design, §5 test catalogue, §6 risks; operator gate
  accepted 2026-11-16).
- **Branch**: `feat/sidecar-author-slot` (worktree
  `../taskboard-WT-feat-sidecar-author-slot`, cut from `origin/main`
  @ `540d2776`).
- **Status**: READY — anchors re-verified by symbol (§2 below); every
  open question resolved with a recorded decision (§8). Stage 2
  implements this plan TDD-first; no design deviations without a
  recorded reason here.

## 1. Scope (from design §1, unchanged)

Five sidecar families (`fold_reminder_rules`, `fold_break_rules`,
`fold_time_entries`, `fold_habits`, `fold_habit_log`) gain a stored
author slot `by: Option<AuthorId>` on their row states; folds duel on
the stored slot; adoption stamps the document's `by`; flush emits
`row.by.or(author)`; migration 0026 persists one nullable `au_by` per
sidecar table. `fold_task_app_data` untouched. Non-goals per design
§1.

## 2. Anchor re-verification (by symbol, against `origin/main` @ `540d2776`)

Design §2 was written at `18dbd74d`; the tree moved (E/#869 and
F/#871 touched the sync crates; round-8 merges touched ui-slint).
Result: **zero semantic drift in any anchor this chunk touches**;
only small line shifts. All anchors below re-verified by reading the
current symbols (not copied):

| anchor | design line | current line | drift |
|---|---|---|---|
| `authored_doc_wins` (`>=`) | merge.rs:141-150 | merge.rs:141-147 | shift only |
| `authored_tombstone_wins` (strict `>`) | :152-159 | :152-158 | shift only |
| inlined habits duel | :1317 | :1317 | none |
| inlined habit-log duel | :1416-1417 | :1416-1417 | none |
| `fold_reminder_rules` body/duel/tomb/adopt | :871/:905/:928-941/:939 | :871/:905/:928-941/:939 | none |
| `fold_break_rules` | :1033/:1060/:1071-1083/:1089 | :1033/:1060/:1071-1083/:1089 | none |
| `fold_time_entries` | :1180/:1211/:1223-1239/:1244 | :1180/:1211/:1223-1239/:1244 | none |
| `fold_habits` | :1284/:1317/:1345-1358/:1359-1366 | :1284/:1317/:1345-1358 (tomb :1352) | none |
| `fold_habit_log` | :1401/:1416 | :1401/:1416-1417 | none |
| `apply_sync_report(..., author)` | pipeline.rs:458 | pipeline.rs:446 | shift only |
| sidecar fold section threading | :995/:1021/:1027/:1028/:1037 | :995/:1021/:1027/:1028/:1037 | none |
| re-sync `apply_files_board_snapshot` | :1930-1944 | :1940-1944 | shift only |
| `SectionFolds` forwarders | :1482-1587 | :1490/:1520-1530/:1542-1552/:1560-1569/:1577-1587 | shift only |
| fold-delta apply pass | :1491-1588 | unchanged shape | none |
| emission sites `by: author` | appdata.rs:1861,1904,1959,2004,2135,2220,2271,2276,2317,2378,2410 | identical (11 sites) | none |
| `by` decode demote rule | appdata.rs:293-299 | :293-299 | none |
| byte-compat omit-when-`None` | appdata.rs:543-549 | :540-549 | shift only |
| state structs | reminders.rs:435/:509, tracking.rs:38, habit.rs:186/:278 | identical | none |
| local creation sites | command.rs:709, :2219-2229 (+ sibling arms :2303, :2347, :2407, :2497, :2661, :2801) | verified | none |
| migrations 0024/0025 landed; next free **0026** | — | `ls migrations/` → 0001..0025 | confirmed |

**E/#869 and F/#871 collision check**: both landed in
`taskboard-sync-nextcloud` (DeckError/retry surface); no symbol they
touched appears in `taskboard-domain`, `taskboard-state`, or the
storage crate anchors above. No collision. The design's anchors are
declared re-verified; stage 2 still anchors by symbol when editing.

## 3. Per-family change list

### 3.0 Shared (all five families)

- **Struct field** (design §4.1 verbatim — plain field, doc-commented
  leniency, no derive crates, A1 policy AGENTS §2.1):
  `pub by: Option<crate::ids::AuthorId>` on `ReminderRule`
  (reminders.rs:435), `BreakRule` (reminders.rs:509), `TimeEntry`
  (tracking.rs:38), `Habit` (habit.rs:186), `HabitLogEntry`
  (habit.rs:278), each with the design's doc comment.
- **Fold signature**: remove the `local_author` parameter from all
  five fold signatures; the local leg becomes `local.by` /
  `entry.by`.
- **Pipeline threading**: drop the `author` forwarding into the five
  sidecar fold call sites (pipeline.rs:995/1021/1027/1028/1037,
  SectionFolds forwarders :1490-1587, re-sync :1940-1944). The
  `apply_sync_report`/`apply_files_board_snapshot` `author` parameter
  itself **stays** (flush builders and `fold_task_app_data` need it).
- **Tie unification (design §4.2, decision D-3 in §8)**:
  `authored_doc_wins` flips `>=` → strict `>` in place — verified:
  its only production callers are the three sidecar folds
  (merge.rs:905/:1060/:1211); tests at :7138/:7151-7152 update with
  it (:7138's `None,None` tie assert flips to `!wins`). The
  doc-comment's "ties keep document" sentence is rewritten to the
  unified law. `authored_tombstone_wins` unchanged.
- **Adoption arms** (design §4.3): construct with `by: *by` at
  merge.rs:928-941 (rules), :1071-1083 (break), :1223-1239 (time
  entries), :1345-1358 (habits), :1416-1425 (habit log `None => true`
  arm stamps `by: *by, count: *count, clock: *clock`).
- **Flush emission** (design §4.4): all 11 `by: author` sites in
  appdata.rs (:1861/:1904/:1959/:2004/:2135/:2220/:2271/:2276/:2317/:2378/:2410)
  become `by: row.by.or(author)` (tombstone legs use the row's stored
  slot where the tombstone record carries one; locally derived
  tombstones keep the `author` fallback). Builder signatures keep
  `author`.
- **Local creation stamps** (command.rs): every sidecar
  `PersistenceAction::Upsert…` construction stamps
  `by: <planner author>` — the planner's author argument where the
  arm has one (AddReminderRule :2219-2229, break-rule/time-entry/
  habit/log arms :2303/:2347/:2407/:2497/:2661/:2801), and the
  recur-spawn site :709 (its planner context's author; if the recur
  planner carries no author seam, thread it — same seam F1 used,
  pipeline.rs:446). None-stamping is a compile error once the field
  lands, so the compiler enumerates any site this list missed.
- **Migration 0026_sidecar_author_columns.sql** (design §4.5 with
  correction D-1): one nullable `TEXT au_by` column each on
  `reminder_rules`, `break_rules`, `time_entries`, `habits`,
  `habit_log`. No data rewrite.
- **Storage codec** (repo.rs): the five raw row structs gain
  `au_by: Option<String>` (0023's precedent, repo.rs:59-126); the
  read queries (:1074/:1084/:1174/:1191 habit_log, habits query in
  the same block) select `au_by`; decode `NULL → None`, parseable
  UUID → `Some`; the INSERT/UPDATE statements (:2104 time_entries,
  :2145 break_rules, :2178 habits, habit-log write ~:2276,
  reminder_rules write in the same block) emit the column from the
  struct slot. `fold_task_app_data` codec untouched.

### 3.1 Family notes

- **Break rules duel on `created_at`** (merge.rs:1060/:1089): the
  local leg is `local.by` and `local.created_at` — unchanged except
  the leg swap.
- **Habits**: the inlined live duel (:1317) reads `local.by`; the
  tombstone leg (:1352) also reads `local.by`. The effective-clock
  mechanism (:1338-1346) unchanged.
- **Habit log**: `HabitLogEntry { count, clock, by }` — the fold's
  `Some(local)` leg (:1416-1417) becomes
  `(*clock, *by) > (local.clock, local.by)`.
- **Reminder rules**: the fold-comment fence sentence
  (merge.rs:898-902, "the sidecar entities carry no author slots")
  is rewritten to the stored-slot law.

## 4. TDD order (which test goes red first)

Order chosen so every red is a compile-error-free, behaviorally red
test (the slot must exist before the duel-leg tests compile; the
catalogue's dependency chain drives it):

1. **Step 1 — the slot exists (mechanical, no red yet)**: add the
   five fields; stamp every construction site (`by: None` at fold
   adoption/local-creation first, tests' fixtures stamped
   explicitly). All existing tests stay green (field is inert). The
   compiler enumerates missed sites.
2. **Step 2 — T-TIE-1 red first** (design §5): full-tuple tie keeps
   local, per family — one unit test per fold (five tests) where the
   doc row equals the local `(clock, by)` exactly; assert the local
   content survives (typed outcome: survivor content identity, never
   message text). Red for the three helper-duel folds until
   `authored_doc_wins` flips strict; green immediately for
   habits/habit-log (already strict — those five tests are the
   canary that the unification does not regress them).
3. **Step 3 — T-ADOPT-1 red** (the amnesia cell): per family, writer
   rank b, doc D rank d, reader rank c with b < d < c — reader
   adopts, then a second cycle folds D; assert the reader converges
   with the writer (survivor `(content, by)` equality). Red until
   the adoption arms stamp `by: *by` **and** the duel legs read
   `local.by`.
4. **Step 4 — the duel-leg flip + signature shrink**: remove
   `local_author` from the five fold signatures; `local.by` becomes
   the leg; `authored_doc_wins` strict. T-TIE-1's three pending reds
   and T-ADOPT-1 go green; the mechanical pipeline/threading edits
   land with it.
5. **Step 5 — T-PROP-2' red→green** (needs the full slot): extend
   F1's T-DUEL-2 proptest to all five folds — randomized author sets
   (0–4 distinct), equal/unequal clocks, distinct contents,
   randomized arrival permutations, 1–3 cycles, a restart between
   cycles via the storage roundtrip, readers explicitly generated
   inside and outside the writers' band. Oracle: identical survivor
   `(content, by)` for every permutation and reader. Tokio-free
   pure-domain test; the restart leg uses the real storage codec.
6. **Step 6 — flush emission (T-RELAY-1 red)**: builders switch to
   `row.by.or(author)`. T-RELAY-1 (two-engine journey): A/B write
   equal-clock distinct contents; C folds both in either order,
   flushes; fresh D folds C's document + A/B's — D's survivor equals
   the writers'. Extends the existing
   `equal_clocks_different_authors_converge_on_every_reader`
   journey's harness.
7. **Step 7 — migration 0026 + codec (T-MIG-1 / T-STORE-1/2)**:
   `0026_sidecar_author_columns.sql`; codec rows read/write `au_by`.
   T-MIG-1: migration on a copy of a pre-0026 fixture DB succeeds,
   all rows decode authorless, no clock text changes. T-STORE-1:
   slot survives the SQLite roundtrip both directions; T-STORE-2:
   `NULL → None`, and an authorless persisted row loses every
   equal-clock duel against `Some` (safe default). Byte-compat pin
   (below) re-pinned.
8. **Step 8 — ADR 0022 amendment** (design §4.6 text verbatim, see
   §8/D-4) + doc-comment rewrites (merge.rs:898-902 fence sentence,
   struct doc comments).

Each step ends with `cargo fmt`, `clippy -D warnings`, and the
affected crate's nextest slice; the full gate is the dispatcher's
(after stage 2's pre-commit hooks + one flock-wrapped
`scripts/run-quality-gates.sh cargo nextest run --workspace`, per
[D] — only that exact gate command).

## 5. Byte-compat pins that must survive untouched

- **Omit-when-`None` emission** (appdata.rs:540-549): a row with
  `by: None` still emits no `by` key; authorless documents stay
  byte-identical. Pin: the existing authorless byte-compat tests
  stay green verbatim (§7 list).
- **Demote-on-garbage `by` decode** (appdata.rs:293-299): unchanged;
  a present-but-unparseable `by` still demotes the whole row.
- **Old-reader relay tolerance**: the `by` key stays default-absent
  on the wire; F1's mixed-fleet pin
  (`old_reader_relay_converges_authorless`,
  `break_rules_section_survives_an_old_peer_relay`) stays green
  verbatim.
- **`authored_remote_wins_reduces_to_remote_wins_for_authorless_rows`**
  (merge.rs): the authorless-lane reduction — with strict flip, a
  `None,None` tie now keeps local instead of adopting the doc;
  verify the reduction test's tie cell: it pins
  "authorless remote wins iff strictly newer clock", which strict `>`
  preserves (clock tie with both-`None` is a full-tuple tie → local).
  If the test asserted the old doc-wins-tie behavior for the
  authorless lane, that cell updates per §7 (expectation-change
  list) with reason D-3.
- **T-DUEL-0** (F1's Deck-lane pin): untouched — Deck merge paths
  (`merge_task` et al.) never call `authored_doc_wins` (verified: its
  only callers are the three sidecar folds).

## 6. Same-release rule check

Design §6 risk 3: the struct field and migration 0026 land in the
**same release** (one PR → one release train entry). No other
sequencing constraint: 0026 is additive-nullable, independent of
0023-0025. Stage 2 keeps both in the single PR; the PR description
notes the same-release pairing. A mid-upgrade mixed state (new code,
old DB) is impossible within a release because the migration runs at
startup before any fold.

## 7. Corpus-unchanged guarantee + the explicit expectation changes

**Everything else stays green verbatim.** The only tests whose
expectations the catalogue explicitly changes:

1. The five `*_fold_converges_on_the_higher_author` tests
   (merge.rs: `reminder_rules_…`, `break_rules_…`,
   `time_entries_…`, `habits_…`, `habit_log_…`): signature change
   (no `local_author` arg) + fixture structs gain `by`; the survivor
   assertions gain stored-author equality (`survivor.by == max
   author`). Content expectations unchanged.
2. `authored_remote_wins_reduces_to_remote_wins_for_authorless_rows`
   (merge.rs): the full-tuple tie cell flips to keep-local per D-3;
   the strictly-newer-clock cells unchanged.
3. The helper's unit tests at merge.rs:7138/:7151-7152: the `>=`
   tie assertions flip with D-3.
4. `equal_clocks_different_authors_converge_on_every_reader`
   (pipeline.rs): extended by T-RELAY-1 (relay leg added); existing
   writer-leg assertions unchanged.
5. `persisted_state_roundtrips_through_sqlite` +
   `author_column_decode_table` (storage): fixtures gain the new
   `au_by` column values; existing 0023-column expectations
   unchanged.
6. `fold_reminder_rules_adopts_stamps_and_lwws` (and the sibling
   adopt tests): fixtures gain `by: None`; adopted-row assertions
   gain `by == doc author` per T-ADOPT-1 where the test already
   asserts the full row.

**Enumerated-verbatim set (must not change)**: all remaining
`merge.rs` fold tests (verbatim/bad-keys/tombstone/resurrect), all
`appdata.rs` emission/union/GC/relay tests listed in the verification
sweep (including `a_due_offset_rule_serializes_the_legacy_shape_verbatim`,
`old_reader_relay_converges_authorless`,
`break_rules_section_survives_an_old_peer_relay`,
`by_serde_roundtrips_absent_decodes_none_and_garbage_fails_closed`),
all pipeline guard/dismissal/push-cycle tests, `fixture_is_valid_persistence_input`,
`authored_clock_grammar_decode_table`, and the F1 journeys
T-PROP-1/T-SIDE-2. Structural fixture edits (adding `by:` to struct
literals) are not expectation changes; any assertion edit beyond the
six enumerated items is a plan deviation and needs a recorded reason.

## 8. Decisions (open questions resolved; deviations recorded)

- **D-1 (resolves design §4.5's table-name flag)**: the habit log
  has its own table — `0017_habit_trackers.sql` creates **`habits`**
  and **`habit_log`** (there is no `habit_trackers` table; verified
  by reading the DDL). Migration 0026 therefore adds `au_by` to
  `reminder_rules`, `break_rules`, `time_entries`, `habits`,
  `habit_log`. Recorded as a design-plan correction (name-level
  only; the column set is unchanged).
- **D-2 (migration number)**: **0026** — confirmed: migrations
  0001..0025 exist; 0024 (`roaming_settings`) and 0025
  (`board_remote_tag`) landed after the design round's evidence
  snapshot. E/F migrated nothing.
- **D-3 (tie direction, design §4.2's implementer's choice)**: flip
  `authored_doc_wins` to strict `>` **in place** (no
  `authored_doc_wins_strict`, no direction argument). Verified: its
  only production callers are the three sidecar folds
  (merge.rs:905/:1060/:1211) — all being changed — so the direction
  parameter would be dead generality. The pre-slot `>=` behavior
  (doc beats an authorless local row at equal clock) is preserved by
  the rank law `Some(_) > None`, as the design argues.
- **D-4 (ADR 0022 amendment text)**: design §4.6's quoted sentence
  goes verbatim into the *Fences kept* paragraph
  (0022:77-81), with the table-name correction D-1 applied:
  `habit_trackers` → `habits`/`habit_log` in the family list.
- **D-5 (recur-spawn author seam)**: command.rs:709 stamps the
  planner-context author; if the recur planner lacks the seam, thread
  it the F1 way (planner carries author like every other mutating
  arm). Compiler-driven; no `by: None` fallback may ship at a
  creation site — `None` is only for adopted/pre-slot rows, never
  fresh local writes.
- **No further deviations from the design plan.**

## 9. Risks (from design §6, plus stage-2 notes)

- Signature churn is the biggest mechanical surface: every fold test
  constructs rows and passes `local_author`; compiler-guided,
  §7 guards against silent expectation drift.
- The proptest's restart leg pulls the storage codec into a domain
  proptest — if the crate-direction dependency forbids it, the
  restart leg runs at the storage-crate integration layer instead
  (same oracle, one test, lowest layer that can express it) — decide
  at implementation, record if it moves.
- Mixed-fleet residual (pre-slot readers relay `by`-less): stated
  honestly in the PR description, not claimed away; T-RELAY-1 models
  one such reader per design §6.

## 10. Estimated diff size

~1,300–1,700 lines across ~10 files: 5 struct fields + ~60
construction sites (mostly test fixtures), 5 fold signatures + duel
legs + adoption arms (merge.rs), pipeline threading removal, 11
emission sites (appdata.rs), command.rs stamps, migration 0026 +
storage codec (repo.rs, 5 row structs / 5 read queries / 5 writes),
ADR 0022 amendment, ~20 new tests (T-TIE-1 ×5, T-ADOPT-1 ×5,
T-PROP-2', T-RELAY-1, T-STORE-1/2, T-MIG-1, plus pin updates).

## 11. Stage discipline restated

Coverage trio never runs here; the gate is pre-commit hooks + one
flock-wrapped `scripts/run-quality-gates.sh cargo nextest run
--workspace` (check `--status` first; if HELD, commit, continue, and
mark the gate "deferred to dispatcher" if it never frees). Backlog
issues opened per [L](d) at discovery; numbers in the stage report.
No push; handoff at commit + report.
