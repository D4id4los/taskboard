# [Plan] State & Storage Test Gaps — Catalogue & Hermetic Designs

- **Date**: 2026-10-17
- **Kind**: stage-1 plan for chunk 4 of the remediation-wave-1 dispatch
  (dispatch §1 row 4 + §5, prompt 4.1). Planning only — no
  implementation in this commit.
- **Origin**: issues **#240** (F4-01 loop-exit, F4-02 diff-negative) and
  **#241** (F5-01 decode rejections, F5-02 pool stance) — normative;
  findings report `2026-10-17-v0.1.0-audit-findings-report.md` §findings
  rows F4-01/F4-02/F5-01/F5-02 + §agenda rows B-2/B-3 (both ruled
  schedule-now, §agenda-outcomes); owning reports
  `2026-10-16-v0.1.0-audit-crate-state-report.md` (§5.2, §8) and
  `2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md` (mutant
  triage, findings §F5-01/F5-02).
- **Branch / worktree**: `test/state-storage-gaps` in
  `../taskboard-WT-test-state-storage-gaps`, cut off `main` @ `2884a6a`
  (= `origin/main`; all `file:line` anchors below are against this tree —
  verified: the audit's v0.1.0 anchors for actor.rs:347-353/641/685/722,
  codec.rs:198-200, connect.rs:35-45 are unchanged at HEAD).

---

## §summary — what this chunk delivers

Five tests, no production code. Two engine-suite tests in
`crates/taskboard-state/tests/engine.rs` pin the ingestion-equality
negative halves (F4-02) and the daemon-survival contract adjacent to the
loop-exit gap (F4-01, reshaped — see D2); two storage tests extend the
codec rejection matrix (F5-01) and add a concurrency probe for the
single-connection pool stance (F5-02).

**The load-bearing planning discovery**: F4-01's prescribed test shape
("drop the handle, `report_tx`, and `system_tx`, await the join under a
deadline") is **structurally unachievable** against the current
production code — proven below (D2). The chunk reshapes F4-01's
deliverable to what is honestly testable, kills the one
black-box-killable mutant of its survivor cluster, and files the
production unreachability as a new tracker issue for a follow-up chunk.

## §decisions — every open question settled

### D1 — F5-01 scope: **test-only** (no typed rejection arms added)

The dispatch anticipated "+ rejection arms if F5-01's remedy prescribes
them". It does not. Evidence:

- The production guards already exist for **all three** non-failed
  phases — `codec.rs:198-200`:
  `"idle" if last_error.is_none()`, `"syncing" if last_error.is_none()`,
  `"offline" if last_error.is_none()`. The surviving mutants
  (`:199:22`, `:200:22`) *replaced the guards with `true`* and nothing
  caught them — the **tests** lack coverage; the code is complete.
- Issue #241's remedy: "Extend `sync_phase_rejections` to all three
  combinations" — a test extension, verbatim.
- The existing test (`codec.rs:437-452`) covers exactly one dangling
  combination (`("idle", Some("network"))`); the `idle` guard's mutant
  is caught, the `syncing`/`offline` guards' are not.

**Recorded decision**: diff for F5-01 = test-only — extend
`sync_phase_rejections` with the two missing combinations. No
production edit.

### D2 — F4-01: the prescribed test shape is falsified; deliverable reshaped

The audit's proposed remedy and issue #240 both prescribe: *"one
A-series test dropping handle/reports/system then awaiting the join
under a deadline … the terminal-state assertion is the join
completing"*. **That test cannot pass against current production code.**

Proof (two independent sources, both verified this session):

1. **Source proof** (tokio 1.53.2, the workspace pin): a broadcast
   channel closes only when the last `Sender` drops —
   `Sender::drop` → `num_tx.fetch_sub == 1` → `close_channel()`
   (`tokio-1.53.2/src/sync/broadcast.rs:1097-1102`); `recv()` surfaces
   `RecvError::Closed` only via `tail.closed` (broadcast.rs:1293).
   There is no `Sender::close()` API.
2. **Empirical probe** (standalone current-thread tokio program): a
   loop-shaped task holding its own sender clone while the external
   sender drops **stays parked on `recv()`** — no `Closed`.

The engine's `run_loop` destructures `system_out` from `EngineParts`
(actor.rs:311-319) and holds that `broadcast::Sender<SystemEvent>` for
the loop's entire lifetime (it is the `StorageFailed` emission seam,
actor.rs:372/468). Therefore `num_tx ≥ 1` always; `system.recv()` can
never observe `Closed`; **`system_open` (actor.rs:331) can never become
`false`**; and the all-sources-closed break at actor.rs:351-353
(`!draining && !commands_open && !reports_open && !system_open`) is
**unreachable in every wiring — test or production**. Dropping the
test's `system_tx` leaves the engine's own clone holding the channel
open; the prescribed join would park forever and the test would fail.

Consequences for the mutant set (audit §5.2 item 1: four `delete !`
survivors on line 351's conjuncts): three of the four
(`!draining`, `!commands_open`, `!reports_open` deletions) can only
diverge in worlds where `system_open == false` — black-box unkillable
while the wiring stands. The fourth — deleting `!` off
`!system_open` — makes the conjunct `system_open`, so the engine breaks
as soon as commands+reports close while a system producer still lives.
**That one is killable**, and the reshaped test kills it.

**Recorded decision** (three parts):

1. **Land** `engine_stays_a_daemon_when_commands_and_reports_close`
   (catalogue item 1): drops the handle and `report_tx`, keeps
   `system_tx` alive, then proves liveness through the system channel
   (`NetworkLost` → published `Offline`). This pins the daemon contract
   (the engine must not exit while a system producer lives — the same
   contract h1 pins for connector death) and kills the `!system_open`
   survivor.
2. **File a new GitHub issue** (stage 2, per the wave-wide bookkeeping
   rule — out-of-scope discoveries become issues, never silent
   expansion): the all-sources-closed exit path is structurally
   unreachable because `run_loop` is its own system-channel sender.
   Draft: title `backlog: state engine all-sources-closed exit is
   unreachable (run_loop holds its own system sender)`, labels
   `architecture` + `p2`, body = the D2 proof summary + a permalink to
   this plan + the candidate remediations for a decision round:
   (a) hold the emission seam as a `broadcast::WeakSender` (upgrade on
   send) so external-producer death closes the channel and makes the
   exit reachable — at the cost of `StorageFailed` emission silently
   failing in that world; (b) delete the dead `system_open` conjunct +
   `Closed` arm and keep the drain exit as the only stop path;
   (c) a spawn-side close-watcher. When (whichever) lands, the
   audit-prescribed join test becomes writable and rides that chunk.
3. **#240 closure**: stage 3's PR references #240; F4-02 is fully
   delivered, F4-01 is delivered to the limit the production code
   permits (the killable half pinned, the blocked half transferred to
   the new issue with the falsification recorded). Recommend
   `Closes #240` + a PR comment linking the new issue so the tracker
   stays truthful.

### D3 — F5-02 probe: behavioral probe (not a constructor seam); holder + size-poll + join design

Issue #241 offers two remedies: the concurrency probe (behavioral —
"fails under default pool options") or a constructor seam exposing the
options for a direct assertion (config-pinning). **Chosen: the
behavioral probe** — it pins the *consequence* (single shared dataset,
the `SQLITE_BUSY`-killer stance) rather than today's option values, per
AGENTS §5's no-pins-of-today's-behavior rule.

Why a plain "N concurrent apply+load" is insufficient (analysis
recorded so stage 2 does not simplify it away): sqlx checks out a pool
connection **per operation** and returns it between operations, so
cooperatively-interleaved tasks all serialise onto the one established
connection — exactly the audit's "serial usage lazily opens exactly one
connection" failure, now in concurrent form. The mutant is only exposed
if a task is **forced** onto a second connection while the first is
held. Hence the holder:

- **Holder**: the test acquires and holds one pool connection
  (`repo.pool.acquire()`, visible to connect.rs's in-crate test module —
  `pool_is_single_connection` already reads `repo.pool.size()`),
  guaranteeing every probe's checkout either queues (production
  options, `max_connections(1)`) or opens a fresh `:memory:` database
  (mutant's `Default::default()`: max 10, min 0).
- **Determinism of the kill**: under the mutant, an acquire with no
  idle connection and `size < max` deterministically connects (never
  queues); every fresh `:memory:` connection is an empty database. Each
  probe asserts its load contains **both** the pre-seeded base board
  **and** its own marker — satisfiable only on the one true shared
  connection (a fresh connection has the marker but not the base;
  the held connection has the base but no marker). No interleaving
  lets a mixed-connection world satisfy both.
- **Release discipline**: the holder is released after a
  deadline-guarded poll of `pool.size() > 1`. Mutant world: the poll
  fires within milliseconds (probes connected) — and the kill is
  already sealed, because the probe(s) that caused `size > 1` applied
  their marker to a fresh connection and can no longer see base+marker
  together even if a later load lands on the released connection.
  Production world: `size` can never exceed 1 (`max_connections(1)`),
  the poll deterministically expires (its cost is the poll deadline —
  see risks), the release lets the queued probes proceed serially on
  the single connection, every probe sees base + own marker (+
  whichever others finished first — still a superset), all pass.
- **Terminal assertion**: after joining all probes, one final `load()`
  sees base + **all N** markers — the single-shared-dataset guarantee,
  interleaving-independent.

**Why not `tokio::time::pause()/advance()`**: sqlx connection
establishment and statement execution are real async I/O, not timer
work — tokio's paused clock auto-advance is documented as unsound
against real I/O drivers, and the crate's established hermetic idiom
for awaits is real time under a deadline guard (`tests/actor.rs` A3
`timeout(60s)`, "deadline-guarded await, not a sleep"). The probe is
therefore **join-based with predicate-with-deadline polling** — the
sanctioned alternative the dispatch prompt itself names. The only
`sleep`s are 5 ms poll turns inside deadline-guarded loops, the exact
shape AGENTS §5 sanctions and the state suite's `eventually` helper
implements.

Parameters: **N = 4** probes (plus one held = 5, within the mutant
pool's default max of 10 so acquires connect rather than queue);
poll deadline **2 s** (mutant connects complete in µs–ms locally; 2 s
is CI-load-safe); probe/join guards `timeout(60 s)` per the suite
idiiom.

### D4 — F4-02 observability: a local recording wrapper (domain fake untouched)

"Asserting no new `UpsertValidators` action lands (observable via the
repo fake)" requires an **action log**; `InMemoryRepository`
(`taskboard-domain/src/test_support/memory.rs`) keeps state only — its
API is `snapshot()`. Extending the domain fake would widen this chunk
into a third crate for one assertion. **Decision**: a local
`RecordingRepository` wrapper in `engine.rs` (delegates to an inner
`InMemoryRepository`, records each applied batch in a
`std::sync::Mutex<Vec<Vec<PersistenceAction>>>`) — the established
local-wrapper precedent is `FlakyRepository` (engine.rs:34-77). The
domain crate's diff stays empty.

### D5 — F4-02 settle discipline: Flush does NOT order behind reports

`run_loop`'s select is `biased` with the commands arm first
(actor.rs:354-355): a `Flush` enqueued after a report is processed
*before* it whenever both are pending, so "flush then assert" is not a
settle for report ingestion. The existing validators test settles via
`eventually` on the report's published effect — but the *identical*
second report publishes nothing (empty batch → the actor.rs:658-661
fast path returns before `repo.apply`). **Decision**: settle the
identical report with a trailing observable report — a
`Failed { pushes: [] }` marker after it (mpsc FIFO within the reports
channel guarantees ingestion order), observed via `eventually(phase ==
Failed{..})`. The marker's own batch is a single `UpsertSyncStatus`
(no validators), so it does not pollute the validator assertion, and
its presence is accounted in the batch-count assertion.

## §catalogue — one behavior, one test, lowest layer

| # | Finding | Test (file · layer) | Fixture | Typed assertion | Mutant killed |
|---|---|---|---|---|---|
| 1 | F4-01 (reshaped, D2) | `engine.rs` · engine-suite (A-series; the behavior is loop-lifecycle, not unit-testable) | default `harness()`; capture `h.handle.shared_state()` **before** dropping; `drop(h.handle)`; `drop(h.report_tx)`; keep `system_tx`; send `NetworkLost` | deadline-guarded poll of the captured `ArcSwap`: `app.sync.phase == SyncPhase::Offline` | line-351 `delete !` off `!system_open`: engine breaks at the top-of-loop check once commands+reports close → the event is never processed (or `system_tx.send` itself errors with no receiver) → the poll deadline / send expect fails |
| 2 | F4-02a | `engine.rs` · engine-suite (routes the actor's iff-differs batching; the comparison lives in the actor, not the domain) | `Spawner { repo: RecordingRepository(InMemoryRepository::with_state(seed_board_state)) }`; Completed report with **non-empty** validators (reuse the existing test's etag-bearing fixture), `read_at` fixed; settle via `eventually(board adopted)`; send the **identical** report again; send `Failed { pushes: [] }` marker; settle via `eventually(phase == Failed)` | after the settle, the recording log holds **exactly two** batches: report 1's (adoption + status + 3 validators) and the marker's (one `UpsertSyncStatus`) — i.e. the identical report contributed **zero** batches; equivalently: no batch after the first contains `UpsertValidators` | `EngineCore::validators` `Box::leak` (leaked default → all three keys "differ" → a second validator batch lands → count wrong) **and** `EngineCore::sync_status` `Box::leak` on the status half (leaked default → status "differs" → an extra batch lands) |
| 3 | F4-02b | `engine.rs` · engine-suite (pins the actor.rs:641 carry-forward read) | default harness; Completed report (captures the published `app.sync.last_success` as `stamp`); then `Failed { kind: Network, pushes: [] }`; settle via `eventually(phase == Failed{Network})` | `app.sync.last_success == stamp` (phase is `Failed`, the stamp survived) | `EngineCore::sync_status` `Box::leak` (leaked default carries `last_success: None` → the Failed batch stores `None` → stamp lost → assertion fails) |
| 4 | F5-01 | `codec.rs` in-file unit (the codec is a pure fn — lowest layer) | none (pure string columns) | extend `sync_phase_rejections` with `sync_phase_from_columns("syncing", Some(..)) == Err(CodecError)` and `("offline", Some(..)) == Err(CodecError)` (distinct valid kind tags per arm, e.g. `"auth"`/`"forbidden"` — presence, not the tag, is the guard) | both guard mutants `:199:22`/`:200:22` (guard→`true` decodes the dangling row → `Ok(..)` ≠ `Err`) |
| 5 | F5-02 | `connect.rs` in-file unit (pool stance is construction-layer; needs in-crate `repo.pool` access) | `open_memory()`; seed one base board via `repo.apply`; sanity `pool.size() == 1`; holder task acquires+holds a connection; 4 probe tasks each `apply(marker board_i)` then `load()`; size-poll (2 s) → release holder → join probes (60 s guard); final `load()` | per probe: loaded board-id set ⊇ {base, marker_i}; terminal: final load's board-id set == {base, all 4 markers} | `pool_options` → `Default::default()` (`:36:5`): probes deterministically open fresh `:memory:` DBs while the holder pins the seeded one; no fresh connection can satisfy base+marker together → probe assertion fails |

Layering check (one-behavior-one-test-lowest-layer): items 2/3 assert
actor-owned batching/carry-forward decisions that no domain unit test
can see (the comparisons live in `append_validators_if_changed` /
`append_status_if_changed` / the Failed arm — private actor fns);
item 1 is loop lifecycle; items 4/5 are pure/construction-layer and
stay out of the integration suites. Nothing duplicates a domain unit
test; the engine thread is not used for anything a unit test owns.

## §hermetic-design — the F5-02 probe step by step

```
#[tokio::test] async fn concurrent_operations_share_one_memory_dataset() {
    repo = open_memory();
    repo.apply([UpsertBoard(base)]); assert load sees base; assert pool.size() == 1;

    // Holder pins the seeded connection so a non-pinning pool is forced
    // to open fresh (:memory: ⇒ empty) databases for the probes.
    held = repo.pool.acquire().await;

    probes = (1..=4).map(|i| tokio::spawn(async move {
        repo.apply([UpsertBoard(marker_i)]).await;
        let loaded = repo.load().await;
        assert!(loaded.boards contains base && marker_i);   // typed: id-set containment
    }));

    // Predicate-with-deadline: fires only under non-pinning options.
    timeout(2s, poll every 5ms until pool.size() > 1);       // expiry is the pinned world's normal path
    drop(held);                                              // release → queued probes proceed serially

    for probe in probes { timeout(60s, probe).await.unwrap().unwrap(); }
    let final = repo.load().await;
    assert_eq!(final board-id set, {base} ∪ {marker_1..=4});  // terminal, interleaving-independent
}
```

- No fixed sleeps: every wait is a deadline-guarded poll or a guarded
  join (AGENTS §5; the 5 ms turns are poll turns, the state suite's
  `eventually` idiom).
- No text assertions anywhere in the chunk; every check is a typed
  value, enum, or set containment.
- Failure modes are hangs converted to fast panics by the guards; under
  the mutant the kill is deterministic (D3), not a race window.

## §snapshot-suite impact

None. The two engine tests assert typed outcomes through the shared
state / the recording repo; no `assert_yaml_snapshot!` is added,
removed, or altered, and `tests/snapshots/` is untouched. The insta
contract (snapshots.rs:12-16) is not disturbed.

## §non-goals

- No production-code changes in either crate (D1 settles F5-01 as
  test-only; F4-01's production remediation is *filed*, not made).
- F4-07 (deadline-wrapping hardening of the 8 TIMEOUT hang-kills) is
  not in this chunk (the dispatch row assigns F4-01/F4-02 only); the
  new tests still use the suite's existing 60 s guards.
- No domain-crate edits (D4) — including no `test_support` changes.
- No F5-03 work (`execute_batch` exhaustiveness is chunk 8, which
  dispatches after this chunk's PR is open).
- No mutant re-runs locally; the weekly sweep (2026-10-19) confirms the
  kills — this chunk's evidence is the analytic mutant-check above.

## §risks

1. **F4-02a's exact batch count (2)** assumes a second *byte-identical*
   Completed report yields an empty batch (idempotent domain merge +
   both iff-differs no-ops + the :658 fast path). If stage 2 finds a
   non-empty batch, that is a suspected bug in either the test's
   premise or the production fast path — evaluate both per AGENTS §5
   before touching the assertion; do not weaken. (The domain merge is
   believed idempotent for identical input — `apply_sync_report` has
   no random or clock-relative behavior beyond `read_at`, which the
   identical fixture fixes.)
2. **The 2 s size-poll burn** in the passing (pinned) world: one test,
   once per run, in a crate whose suite is currently fast. Accepted
   knowingly (the alternative — a constructor seam — was rejected in
   D3). If CI timing ever makes it a concern, shrink the deadline; the
   mutant-world kill tolerates any deadline ≥ real connect latency.
3. **The daemon-survival test's mutant-kill path** ends either in the
   poll deadline or in `system_tx.send` returning `Err` (no receivers
   once the loop broke) — both are failures; the `.expect` on the send
   must not be "fixed" to tolerate `Err` (that would weaken the kill).
4. **CI scheduling variance** cannot flip any assertion: every design
   decision above was checked under both serial and maximally
   interleaved schedules (the terminal assertions are
   interleaving-independent; the mutant kills are connect-forced, not
   race-dependent).
5. **`RecordingRepository` is a new local fake** — keep it minimal
   (delegate + log; no behavior of its own) so it cannot mask the
   behaviors under test.

## §stage-2 execution checklist

1. Recreate the worktree if missing; read this plan + the dispatch
   §1/§5 + issues #240/#241.
2. Implement in three standalone commits (each green alone):
   - `test(state): pin ingestion-equality negatives + daemon survival`
     (catalogue 1-3, engine.rs + the local wrapper);
   - `test(storage): extend sync-phase rejection matrix` (catalogue 4);
   - `test(storage): concurrent :memory: dataset-identity probe`
     (catalogue 5).
3. Every new test must pass against **unmodified** production code. A
   failure is a suspected bug (equally likely in the test): stop,
   report verbatim, do not weaken, do not fix production here.
4. File the D2 GitHub issue (draft in D2.2) per AGENTS §7 conventions;
   report the issue number back.
5. Full AGENTS §10 gates green, plus `cargo nextest run -p
   taskboard-state -p taskboard-storage-sqlite` in isolation. No push,
   no PR (stage 3 owns those).
