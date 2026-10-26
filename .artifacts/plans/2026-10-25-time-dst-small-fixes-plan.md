# [Plan] Time/DST small fixes — whole-second offsets & structural sync freshness

- **Date**: 2026-10-26 (file name per dispatch; written 2026-10-26)
- **Kind**: stage-1 chunk plan, backlog-triage dispatch chunk 9 — TDD-first
  implementation spec for issue #283's two code legs. No code in this
  commit.
- **Branch**: `fix/time-dst-small-fixes` (worktree
  `../taskboard-WT-fix-time-dst-small-fixes`, cut off `main` @ `a1bc03d`).
- **Origin**: issue #283 (normative); dispatch plan
  `2026-10-25-backlog-triage-dispatch-plan.md` §1/§10 (chunk table,
  non-goals, acceptance, stage-2/3 traps); theme report
  `2026-10-16-v0.1.0-audit-theme-time-dst-report.md` §trace (round-trip
  verdict), §findings (F11-03, F11-04). F11-05 (UTC-only docs) is
  **chunk 5's** — not touched here.
- **Status**: READY. All `file:line` evidence re-verified on this branch's
  base (`a1bc03d`), not quoted from the audit's tag.

## §1 Scope

Two independent fixes, one branch/PR:

1. **F11-03** — `ReminderOffset` becomes whole-second *by
   construction*, so the persisted `offset_seconds` INTEGER column can
   never diverge from the evaluated offset (the reload-divergence
   double-fire becomes unconstructible rather than merely unreachable).
2. **F11-04** — the `sync` verb detects cycle freshness **structurally**
   (report generation counter), not by comparing two wall stamps; a
   backward host-clock step can no longer misreport a completed,
   successful cycle as `TimedOut`. `last_success` stays exactly what it
   is: a display fact.

**Non-goals** (dispatch §10, binding): no storage schema change or
migration; no reminder-evaluator changes; no DST-semantics changes beyond
the two findings; no UTC-only documentation leg (chunk 5); no
`wait_for_adoption` redesign (already phase-based — boards.rs:997-1047
has no wall-stamp comparison).

## §2 Design decisions

### D1 — The mint rejects; it does not truncate (F11-03)

`ReminderOffset::before` (reminders.rs:40-46) gains a second typed
rejection: a `dur` with `subsec_nanos() != 0` (chrono 0.4.45
`TimeDelta::subsec_nanos`, time_delta.rs:375) is refused.

- **Error shape**: extend `InvalidReminderOffset` (reminders.rs:128-134)
  with a `NotWholeSecond` variant — doc: the offset carries a sub-second
  component; offsets are whole-second so the storage column, the text
  form, and the synced document field can never diverge from the
  evaluated value. `AfterDue` keeps its meaning and **is checked first**:
  a positive sub-second input answers `AfterDue` (existing behavior for
  positive inputs is preserved; the precedence is pinned by test, §6 T3).
- **Why reject, not truncate, per the A1 policy** (AGENTS §2, no
  validate-and-forget): `before` is the trust boundary where an untyped
  `TimeDelta` becomes the invariant-carrying type. Truncation silently
  rewrites the caller's value — a −500 ms offset truncates *toward zero*
  into "fire at due", a real behavior change nobody consented to — and
  re-opens exactly the divergence this fix closes (the caller's mental
  offset vs. the stored/evaluated one). A fallible mint forces every
  boundary to decide; the two whole-second producers in the tree (the
  `parse_before` grammar, the integer-seconds codecs) never see the new
  error (§3), so the rejection costs nothing on sanctioned paths.
- **Inner representation stays `chrono::TimeDelta`** (not an `i64`
  seconds field). A1's pattern is the private field + fallible mint, not
  primitive-level unrepresentability; an `i64` inner would make
  `as_duration()` fallible at the range edge (`TimeDelta::try_seconds`
  returns `Option`) for zero behavioral gain, and would churn every
  `as_duration()` caller. Considered and rejected.

### D2 — Deserialization is custom and strict (the one real gap)

The current `#[derive(Deserialize)]` bypasses the mint entirely — it
constructs the private `TimeDelta` field directly, so
`serde_json::from_str::<ReminderOffset>("[-600, 500000000]")` mints a
sub-second offset and `"[1, 0]"` a positive one. With the mint tightened,
the derived impl would be the single path where the invariant is
unenforceable.

Replace with hand-written impls on `ReminderOffset`:

- **Serialize** delegates to the chrono tuple (`(i64 secs, i32 nanos)`,
  chrono time_delta.rs:686-696) — the wire/JSON shape is unchanged, so
  every existing snapshot/serialized artifact stays byte-identical.
- **Deserialize** decodes the same tuple, rebuilds the `TimeDelta`
  (`TimeDelta::new(secs, nanos)`, mapping its `None` to
  `serde::de::Error::custom`), and **mints through `before`**, mapping
  both error variants to `custom` — sub-second and positive data are
  *corruption*, rejected, never clamped. This is the storage codec's
  standing precedent (repo.rs:455-458: "positive stored offset is
  corruption, not a clamped value") applied to the serde lane.
- **Leniency considered and rejected**: truncating on deserialize would
  silently rewrite corrupt data — the validate-and-forget shape A1
  forbids. Strictness costs nothing today (see §3: no production path
  feeds this impl anything but whole-second, non-positive tuples) and
  protects every future one.

### D3 — Structural freshness = the existing report-generation counter (F11-04)

**Generation counter wins over phase-transition detection.** A phase
transition cannot see a cycle that starts and completes between two
200 ms polls (phase returns to `Idle` — indistinguishable from no cycle
without auxiliary state), and classifying an outcome through the
aggregate fold's dominance (`Syncing > Failed > Offline > Idle`,
state.rs:273-280) is racy mid-cycle. A counter is exact, monotone within
the process, and — decisively — **already exists and is already
published**:

- `SyncCounters.cycles` (state.rs:113) is incremented exactly once per
  ingested `SyncReport`, per board, on every arm of `ingest_sync_report`
  (Completed: actor.rs:754; board-less Failed: :783; board-scoped
  Failed: :838). A report discarded by a storage failure counts nothing
  (actor.rs:866-871) — a counted cycle is a cycle that landed in state.
- The counters are **memory-only** (`SyncCounters` is never persisted —
  the codec and `set_stored` never touch it, state.rs:56-58/:227-237;
  hydrated rows carry the serde default), folded by saturating sum into
  the aggregate (state.rs:132-139/:300), and overlaid onto the published
  `AppState.sync` rows at publish time (core.rs:65-78). The verb's
  `watched_status` therefore already reads it.

**Where the counter lives**: exactly where it lives today —
`SyncStatus.counters.cycles`, no new field, no engine state, no
migration. This satisfies dispatch stage-3 trap 2 ("must not add engine
state whose persistence would need a migration") by construction: the
mechanism is pure re-use.

**What changes in the verb** (`commands/sync.rs`): the freshness
predicate at sync.rs:162 (`status.last_success > startup.last_success`)
is replaced by a pure classifier extracted next to
`scoped_sync_status` (the house precedent for the verb's pure half,
sync.rs:92-112):

```text
fn fresh_outcome(startup: &SyncStatus, status: &SyncStatus) -> Option<SyncOutcome>
  fresh = status.counters.cycles > startup.counters.cycles   // a report landed since startup
  !fresh            -> None                                   // keep waiting
  phase == Failed   -> Some(Failed(kind, view))               // the fresh report's verdict
  phase == Syncing  -> None                                   // some watched board still in flight
  else              -> Some(Succeeded(view))                  // Idle | Offline after a landed report
```

Arm order in `wait_for_sync_result` becomes: `fresh_outcome` → the
existing fresh-failure arm (unchanged: `Failed` phase +
`seen_change || status != *startup` — it stays for the stale-row +
signal case) → deadline. The deadline arm, the 200 ms poll fallback,
the signal subscription, `sync_on`'s nudge/`NoOp` refusal, and
`NoWait` are untouched.

**What `last_success` remains for**: display — `render_text`
("last success …"), `SyncStatusView`, the UI's recency caption
(models.rs:985-998/:1213), and the engine's status-write dedup
(append_status_if_changed, actor.rs:919-936). It loses exactly one
consumer: the verb's freshness predicate. No behavior contract is
placed on its value (dispatch stage-2 rule).

**Doc updates ride the code**: `SyncOutcome::Succeeded`'s doc
(sync.rs:22-24) and the loop's decision-table doc (sync.rs:144-150) are
rewritten to the structural predicate.

### D4 — Recorded semantic deltas (accepted, small)

- **Multi-board scoped watch, mixed outcome**: today, `Succeeded` wins
  whenever the max-fold `last_success` advanced even if a later report
  failed another watched board. Under D3, the aggregate's `Failed`
  dominance resolves the fresh cycle as `Failed` — the honest aggregate
  verdict. Single-board watches (the dominant case) are unaffected: a
  landed report leaves the row `Idle`+advanced-stamp or `Failed`,
  never both.
- **Watched-set changes mid-wait** (D6-6, sync.rs:78-83): the
  saturating sum over a re-derived board set can decrease when a board
  leaves the scope, so freshness can toggle. Today's max-fold
  `last_success` has the same shape. Unchanged honesty semantics; no
  new hazard; the D6-6 doc comment already records the re-derivation
  rule.
- **Backward step, engine stamps**: with the wall comparison gone, a
  backward step can no longer suppress `Succeeded` (the bug), and a
  forward step's behavior is unchanged. `render_text` may print a
  `last_success` older than the previous run's — that is the display
  fact being honest about a stepped clock, per F11-04's intent.

## §3 Construction-path audit (F11-03) — every path `ReminderOffset` can enter

| # | Path | Site | Whole-second today? | Under D1/D2 |
|---|---|---|---|---|
| 1 | `before()` — the mint | reminders.rs:40-46 | accepts sub-second (**the hole**) | rejects: `NotWholeSecond` |
| 2 | `at_due()` / `Default` | reminders.rs:50-52 | zero | unchanged |
| 3 | `parse_text_form` — text grammar | reminders.rs:97-125 | yes (u64 magnitudes × s/m/h/d) | unchanged; already mints via `before` |
| 4 | `[reminders] default_before` | app config.rs:1081 (via `parse_before` grammar) | yes | unchanged; mint never fires |
| 5 | CLI `--before` | app cli.rs → `parse_before` | yes (same grammar) | unchanged |
| 6 | UI preset buttons | ui presentation.rs:279-291, adapter.rs:3036-3046 | yes (const preset magnitudes; `expect` documented unreachable) | unchanged |
| 7 | UI custom input | adapter `on_rule_add_custom_accepted` → `parse_text_form` | yes | unchanged |
| 8 | appdata presets | domain appdata.rs:428/:985/:1631 | yes (`TimeDelta::seconds`) | unchanged |
| 9 | **Sidecar document** — `RuleEntry::Rule.offset` | appdata.rs:400-413 writes `num_seconds()` integer; `decode_offset` :415-422 mints integer seconds through `before`, positive demotes | yes (integer wire shape) | unchanged; lossless now by construction |
| 10 | **Storage reload** | sqlite repo.rs:455-458 (`try_seconds(column)` → `before`, positive/garbage = `Corrupted`) | yes | unchanged |
| 11 | **Storage write** (the audit's truncation site) | sqlite repo.rs:1246-1259, `as_duration().num_seconds()` | truncates toward zero | truncation becomes dead code by construction; comment rewritten to state the invariant (type guarantees whole-second; column stores it exactly); no schema change |
| 12 | **Serde derive** | reminders.rs:27-29 derive | **bypasses the mint** (tuple constructs the private field) | replaced by custom strict impl (D2) |
| 13 | Test-support strategies | test_support/reminders.rs:30-33, messages.rs:162, contract.rs:275 | yes (whole-second `try_seconds`) | unchanged |

Audit conclusion: after D1+D2 every constructible path — API, text
form, config, CLI, UI, storage both directions, sidecar document both
directions, serde — enforces or requires whole-second ≤ 0. The
double-fire is unconstructible, not merely unreachable.

## §4 Implementation outline (TDD order)

1. **Domain, red → green**: add `NotWholeSecond`; write §6 T1–T5
   (they fail/compile-fail first where applicable); tighten `before`;
   swap the serde derives for the custom impls; update the type's doc
   (reminders.rs:22-26 "≤ 0 by construction" gains "whole-second by
   construction"), `text_form`'s stale truncation note
   (reminders.rs:60-68 — the leniency sentence is dead after D1; the
   paragraph states the invariant), and `InvalidReminderOffset`'s
   variant docs.
2. **Storage**: rewrite the repo.rs:1246-1259 comment; add §6 T6–T7.
3. **State**: add §6 T8 (backward-step counters pin).
4. **App**: write §6 T9 (classifier, red — the function doesn't exist);
   extract `fresh_outcome`, rewire the loop, update the two doc
   blocks; add §6 T10 if the harness allows (see T10 note).
5. **CHANGELOG** Unreleased: one bug-fix entry — the false-`TimedOut`
   fix is user-observable (dispatch stage-2 step 3).
6. Full gate suite (AGENTS §10); bookkeeping per dispatch stage-2 step
   4 (comment on #283, do not close).

## §5 Test catalogue

House rules apply (AGENTS §5): typed-outcome assertions
(`matches!`), no text assertions, no wall reads in tests, one
behavior/one test/lowest layer, properties not weakened.

**F11-03 — domain (`taskboard-domain/src/reminders.rs` tests)**

- **T1** `before_rejects_sub_second_durations`: negative sub-second
  (`try_milliseconds(-500)`) → `Err(NotWholeSecond)`; zero and negative
  whole-second inputs still `Ok` (extends, not weakens,
  `offset_guard_accepts_non_positive_and_rejects_positive`).
- **T2** `after_due_dominates_the_precedence`: positive sub-second
  (`+500 ms`) → `Err(AfterDue)` (D1's documented precedence pin).
- **T3** `text_form_and_parse_stay_whole_second`: existing grammar tests
  keep passing unchanged; add one assertion that every
  `parse_text_form`-accepted value satisfies
  `as_duration().subsec_nanos() == 0` (cheap deterministic loop, not a
  new property — P9 already round-trips the magnitudes).
- **T4** `serde_roundtrips_whole_second_and_rejects_corruption`:
  whole-second offsets serialize to `(secs, 0)` tuples and deserialize
  back identical; `[-600, 500000000]` → deserialize error;
  `[1, 0]` → deserialize error (typed `serde` errors, no panics). Pins
  both the unchanged wire shape and the strict mint.
- **T5** properties **updated, not weakened**: P1–P9 run on
  `reminder_offset_strategy()` (already whole-second,
  test_support/reminders.rs:30-33) — no edits expected; if any property
  constructs offsets inline, it migrates to the strategy. The
  P9 text-form round-trip property is the audit's "round-trip
  properties updated" leg and must remain green with zero relaxations.

**F11-03 — storage (`taskboard-storage-sqlite/tests/roundtrip.rs` + repo tests)**

- **T6** `rule_offset_roundtrips_identically_through_sqlite`: property
  (or swept deterministic cases incl. 0 s, −1 s, −90 s, −86 400 s) —
  write a rule, reload, assert `ReminderOffset` equality. Extends the
  existing rules round-trip fixture (currently a single −5 min case,
  roundtrip.rs:255) from sampled to exhaustive-over-domain; the
  `num_seconds()` write is pinned lossless.
- **T7** `corrupt_offset_column_stays_corrupted`: a hand-written row
  with a positive `offset_seconds` reloads as `Corrupted` (pre-existing
  behavior, pinned here so the D2 strictness has a visible codec
  sibling). Skip if an equivalent assertion already exists in the repo
  tests — do not duplicate (AGENTS §5).

**F11-03 — state (`taskboard-state/tests/sqlite_smoke.rs`)**

- **T8** `reloaded_rule_never_refires_a_delivered_occurrence` (the
  double-fire impossibility, end to end at the lowest layer that has
  the whole chain): rule offset −90 s on a due-anchored task; tick
  fires at occurrence O; `RecordReminderDelivery` settles it; persist;
  reload through the real sqlite repo; `due_firings` at `now ≥ O` is
  empty. Under a sub-second-capable type this test's premise
  (reload-recomputed occurrence ≡ O) was the hazard; post-fix it is a
  pin. FixedClock-injected throughout.

**F11-04 — state (`taskboard-state/tests/`)**

- **T9** `counters_advance_across_a_backward_clock_step`: FixedClock
  (tests/common/mod.rs:26-42 — `advance(i64)` takes negatives) at T0:
  deliver a `Completed` report for a bound board → aggregate
  `counters.cycles == 1`, `last_success == T0`; `advance(-3_600)`;
  deliver a second `Completed` report → `counters.cycles == 2` and
  `last_success` moved **backward**. Pins the engine half: the
  generation signal is wall-step-immune. (Engine report-delivery
  harnesses already exist in engine.rs; the assertion is structural —
  counter values and phase, no wall reads.)

**F11-04 — app (`taskboard-app/src/commands/sync.rs` tests)**

- **T10** `fresh_outcome_classifier` table tests (pure, hand-built
  `SyncStatus` — the `scoped_sync_status` precedent):
  - fresh + `Idle` → `Succeeded`;
  - fresh + `Failed{kind}` → `Failed(kind)`;
  - fresh + `Syncing` → `None` (multi-board in-flight patience);
  - stale (equal cycles, any phase) → `None`;
  - **the false-TimedOut regression fixture**: cycles advanced,
    `last_success` moved backward (`Some(T)` under `Some(T+3600)` at
    startup), phase `Idle` → `Succeeded`.
  **Falsifiability (stage-3 trap 3)**: this exact fixture is the
  pre-fix failure input — the old predicate
  `status.last_success > startup.last_success` is false for it, so the
  old loop ran to the deadline and answered `TimedOut`. The classifier
  is new code, so verification is **by argument recorded here** plus a
  temporary revert of the loop wiring to the wall comparison (one-line
  change) observed red; stage 3 records which verification was
  performed.
- **T11** loop-level: the existing `detach_tests` tier keeps the
  `SyncUnavailable` refusal green; **no new loop test** — the loop is
  reduced to calling the classifier between the unchanged
  fresh-failure and deadline arms, and an app-tier injected-clock test
  would require a bootstrap clock seam (bootstrap.rs:585 hardcodes
  `SystemClock`) — out of scope per AGENTS §5 "lowest layer"; the
  behavior lives in T9 (engine) + T10 (classifier).

## §6 Risks

1. **Strict serde rejects data the old derive accepted** (positive or
   sub-second tuples). No production path feeds such tuples (§3); risk
   is test fixtures only — the deterministic-serialization property and
   snapshots will surface any. Mitigation: gate suite.
2. **Multi-board `Failed`-dominance delta** (D4): a semantic change in
   a scope edge case; recorded here as intended. Single-board (the
   CLI's default and the session verb's dominant shape) unchanged.
3. **Counter reset at boot**: counters are process-lifetime; the verb
   always snapshots *after* booting/hydrating its core, so the
   comparison is intra-process and the reset is invisible to it. A
   future cross-process freshness consumer would need a persisted
   generation — explicitly out of scope (dispatch non-goal: no
   migration).
4. **`record_deltas` always reports `counted = true`** for a non-empty
   delta list (actor.rs:907-917) — cosmetic pre-existing quirk
   downstream of `record_report`'s unconditional fold; does not affect
   the predicate (the published state is what the verb reads). Not
   fixed here (no evaluator/engine changes beyond the minimum); noted
   for the reviewer so it isn't mistaken for a chunk bug.
5. **Precedence pin churn**: `AfterDue`-first ordering (T2) is a
   judgment call; if review prefers sub-second-first, the pin flips
   with it — the test is the contract.

## §7 Acceptance mapping (dispatch §10)

| Dispatch acceptance | Where |
|---|---|
| Double-fire construction impossible via public API, pinned by typed rejection | D1 + T1/T2 |
| Backward step no false `TimedOut`, structural, injected clock, no wall reads | T9 (engine) + T10 (classifier fixture) — `FixedClock` only, zero wall reads |
| Round-trip properties updated, not weakened | T5 (P1–P9 intact), T6 (extended fixture) |
| Gate suite green | §4 step 6 |
| No storage schema change | §3 rows 10–11 (comments only) |
| Freshness needs no migration | D3 (reuses memory-only `SyncCounters.cycles`) |

**Estimated diff**: ~400–470 lines, majority tests —
reminders.rs ~150 (mint + serde + docs + T1–T4), storage ~60
(comments + T6/T7), state tests ~80 (T8/T9), commands/sync.rs ~150
(classifier + loop + docs + T10), CHANGELOG ~6. No domain shape
changes beyond the error enum variant.
