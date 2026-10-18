# [Plan] Engine Exit Reachability — the WeakSender Emission Seam (#316)

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 3 of the remediation-followups dispatch
  (dispatch §1 row 3 + §4, prompt 3.1). Planning only — no
  implementation in this commit.
- **Origin**: issue **#316** (normative) — the all-sources-closed exit is
  structurally unreachable because `run_loop` holds its own
  `broadcast::Sender<SystemEvent>`; `.artifacts/plans/2026-10-17-state-storage-gaps-plan.md`
  decision **D2** (the unreachability proof + the #240 test prescription
  it blocked); dispatch §4 decision **D3** (remediation candidate (a),
  `broadcast::WeakSender`) — settled; this plan verifies it and designs
  the seam + tests.
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups`, continuing after
  chunk 2's tip (`fbe1790`). Chunks 1–2 confined to
  `taskboard-sync-nextcloud` + ADR 0007 — `crates/taskboard-state/` is
  byte-identical to `main` (`b24446e`; verified: empty diff
  `b24446e..HEAD` for the crate), so every anchor below is against this
  tree.

---

## §summary — what this chunk delivers

One production seam swap in `crates/taskboard-state/src/actor.rs` and
two new lifecycle tests in `crates/taskboard-state/tests/engine.rs`.
`run_loop` stops owning a strong `broadcast::Sender<SystemEvent>` (the
`StorageFailed` emission seam) and holds a `broadcast::WeakSender`
instead, so external-producer death closes the system channel and the
all-sources-closed exit (`actor.rs:351-353`) becomes reachable — for
the first time in any wiring. The audit-prescribed #240 join-completes
test (blocked as structurally unpassable by state-storage-gaps D2)
becomes writable and rides this chunk; the three black-box-unkillable
`delete !` mutants on the line-351 conjunct become killable, and the
two new tests kill them.

The swap is invisible to every caller: `spawn_state_engine`'s public
signature is unchanged (the downgrade happens inside, at `EngineParts`
construction), and all seven external callers keep a strong system
sender for their own lifetime, so `StorageFailed` emission semantics
while anyone runs are byte-for-byte today's.

## §decisions — every open question settled

### D1 — `broadcast::WeakSender` verification: **exists; no deviation from dispatch D3**

Verified against the pinned tokio in the cargo registry
(`tokio-1.53.2/src/sync/broadcast.rs`):

- `pub struct WeakSender<T>` — broadcast.rs:213. Doc: "if all `Sender`
  instances of the channel were dropped and only `WeakSender` instances
  remain, the channel is closed."
- `Sender::downgrade(&self) -> WeakSender<T>` — broadcast.rs:732
  (`#[must_use]`; increments `num_weak_tx`, does **not** count toward
  RAII close).
- `WeakSender::upgrade(&self) -> Option<Sender<T>>` — broadcast.rs:1111.
  Returns `None` iff `num_tx == 0` (all strong senders dropped); on
  success CAS-increments `num_tx`, so the upgraded handle is
  RAII-correct and drops back within the same statement.
- `Clone` (broadcast.rs:1146) and `Drop` (1155, decrements `num_weak_tx`
  only). There is **no `subscribe` on `WeakSender`** — once all strong
  senders are gone no new receiver can ever be created; the channel's
  closed state is terminal for delivery.

Candidate (a) stands as dispatched. Rejected alternatives were already
dispatch-settled: (b) deletes a real lifecycle guarantee, (c) adds a
watcher task for no additional guarantee.

### D2 — downgrade site: **inside `spawn_state_engine`; public signature unchanged**

`spawn_state_engine` keeps taking `system_out:
broadcast::Sender<SystemEvent>` (actor.rs:240) and constructs
`EngineParts { system_out: system_out.downgrade(), .. }`; the strong
parameter drops at the end of the spawn body like any consumed
argument. Caller audit (all `spawn_state_engine` call sites, this tree):

| Caller | Passes | Retains a strong sender? |
|---|---|---|
| `taskboard-app` bootstrap.rs:605-613 | `system.clone()` | yes — `App.system` (bootstrap.rs:134, 660); the sync actor and `attach_connector` hold further clones |
| state `tests/engine.rs` harness (163-191) | `system_tx.clone()` | yes — `Harness.system_tx` (engine.rs:177) |
| state `tests/reminders.rs` (181-197) | `system_tx.clone()` | yes — harness field (reminders.rs:197) |
| state `tests/snapshots.rs` (61-78) | `system_tx.clone()` | yes — spawner field (snapshots.rs:78) |
| state `tests/sqlite_smoke.rs` (50-56) | `system_tx.clone()` | yes (sends `Shutdown` at :84) |
| state `tests/proptest.rs` (451-457) | `system_tx` (moved) | **no** — see D6; analyzed harmless |
| sync `tests/common/sync_pair.rs` (51-58) | `system_tx.clone()` | yes — the pair + actor clones |
| sync `tests/e2e_hermetic.rs` (82-88) | `system_tx.clone()` | yes — `Stack.system_tx` |

**Recorded decision**: zero caller churn — the dispatch non-goal "no
app-runner behavior changes beyond what the seam swap forces
(compile-level)" is satisfied vacuously (nothing is forced). Rejected:
changing the signature to take a `WeakSender`, which would push the
downgrade responsibility into seven callers for no additional
guarantee.

The load-bearing semantic: after the swap, the **external world** owns
the system channel's lifetime. In production the runner always holds a
strong sender (`App.system`) for the app's lifetime, so every emission
while the app runs upgrades successfully — D3's rationale, verified
against the actual wiring rather than assumed.

### D3 — emission shape: **upgrade-per-send inside `note_storage_failure`**

`system_out` reaches production code exactly twice, both times only to
call `note_storage_failure` (`execute_command` via `Seams`,
actor.rs:372/504/551; `reminder_tick` via `TickSeams`,
actor.rs:468/783/797). The seam swap is therefore:

```rust
fn note_storage_failure(
    storage_failing: &mut bool,
    system_out: &broadcast::WeakSender<SystemEvent>,
) {
    if !*storage_failing {
        // Weak by design (#316): external producers own this channel's
        // lifetime. Upgrade failure means the channel is closed — no
        // receiver can ever appear (WeakSender cannot subscribe) — the
        // same silent no-op as today's send-to-zero-receivers Err.
        if let Some(system_out) = system_out.upgrade() {
            let _ = system_out.send(SystemEvent::StorageFailed);
        }
    }
    *storage_failing = true;
}
```

(the comment states the constraint, per house comment discipline —
stage 2 may reword, not delete it). With the type flowing through, the
private aliases change their last element:
`Seams` (actor.rs:492-498) and `TickSeams` (actor.rs:767-772) become
`.., &'a broadcast::WeakSender<SystemEvent>`; `EngineParts.system_out`
(actor.rs:293) becomes `broadcast::WeakSender<SystemEvent>`. The
`run_loop` destructuring (actor.rs:311-319) is untouched — only the
field's type moves.

**Upgrade-on-send failure semantics = today's no-subscriber semantics**:
today, `Sender::send` with zero receivers returns `Err` and the engine
ignores it — nothing is buffered, nothing can be observed. Post-swap,
upgrade failure means `num_tx == 0`: the channel is closed, `recv()`
surfaces `Closed` to every receiver after draining, and (no
`subscribe` on `WeakSender`) no future receiver can be minted. A
skipped send in that world is unobservable-by-construction — strictly
weaker than a promise, it is a tautology. The one wiring where
observables genuinely differ is a subscriber that outlives *every*
strong sender (impossible to construct except by subscribing then
dropping all senders — see §risks 1); no production wiring does this
(subscribers are minted by `App.system`, which the shutdown ordering
keeps alive past the engine join, bootstrap.rs:330-372).

The ephemeral upgraded `Sender` momentarily counts toward `num_tx`;
its drop is RAII-correct and cannot close a channel that external
producers still hold open (upgrade only succeeds when `num_tx ≥ 1`).

### D4 — test catalogue: **two new tests; the still-receives clause is carried by the existing a17**

The dispatch acceptance names a "still receives `StorageFailed` while
running" test. **No new test is written for it** —
`a17_storage_failed_is_edge_triggered_per_epoch` (engine.rs:557-632)
already asserts exactly that behavior: it subscribes a live external
receiver (`h.system_tx.subscribe()`, engine.rs:565 — a strong external
sender's subscriber), triggers a storage failure, and asserts the event
arrives (typed count). Post-swap, that delivery can only happen through
the weak upgrade (the engine's only emission path), so a17 stays green
iff the seam still delivers; the reminders-suite twin
(reminders.rs:499, tick-path emission) pins the same through
`TickSeams`. Duplicating either would violate one-behavior-one-test.
The plan records this so stage 3 checks the acceptance clause against
a17 staying green, not against a new test existing.

The two new tests (detailed in §test-design):

1. `engine_exits_when_all_sources_close` — the #240 join-completes
   prescription, finally writable. **Must fail on pre-change code**
   (the engine parks on `system.recv()` forever — its own strong sender
   keeps `system_open` true).
2. `storage_failed_is_silent_after_the_last_external_producer_dies` —
   pins the D3 upgrade-failure semantics: no event can reach a
   subscriber of a closed channel, and the emission failure must not
   break the loop (engine stays alive, recovery still works). Also red
   pre-change (pre-change the engine's own strong sender delivers to
   the subscriber); this red is the *intended* semantic delta, not a
   bug — stage 2 records both reds as TDD evidence.

### D5 — commit strategy: **tests-first workflow, one standalone-green commit**

Dispatch 3.2 prescribes TDD (tests first, join-completes observed red)
and rebase-merge hygiene (every commit standalone). A commit containing
only the new tests is red on the pre-change seam — not standalone. A
seam-first commit is green but unverified by its own tests. **Recorded
decision**: stage 2 writes both tests first, runs them against the
unchanged production code, records the exact failure output (report
evidence), then lands the seam and commits **once**:

```
fix(state): hold the system emission seam weakly so the all-sources exit is reachable
```

### D6 — proptest wiring: **its engine starts with the system channel closed; no edit**

`drive_sequence` (proptest.rs:451-457) moves its only `system_tx` into
the spawn. Today the loop's own strong sender is that channel's last
handle; post-swap the channel closes at spawn and the engine observes
`Closed` on its first `system.recv()` poll (`system_open == false` from
boot). Analysis: every scenario drives the engine through a live
`handle` (commands open ⇒ no exit — the conjunct is an AND), nothing
sends or asserts system events in that suite, and the runtime drops at
the end of `drive_sequence` regardless of the task's state. `_report_tx`
is a named binding, so reports stay open through each scenario.
**Recorded decision**: no proptest edit; if stage 2 sees a property
failure appear with the swap, that is a suspected bug per AGENTS §5 —
evaluate test-premise and production code before touching either.

### D7 — docs: **module + seam-site constraint comments**

The module doc (actor.rs:11-14, "The engine owns its command inbox…")
gains the ownership correction: the system emission seam is held
weakly; external producers own that channel's lifetime; their death
closes the engine's last input and makes the all-sources-closed exit
reachable. The downgrade site (D2) and `note_storage_failure` (D3)
carry one constraint comment each. No other doc surfaces name the
sender ownership; `spawn_state_engine`'s doc gains one sentence.

## §seam-design — the exact production diff

All in `crates/taskboard-state/src/actor.rs`; nothing else compiles
differently (private types only; public signature unchanged per D2):

1. **actor.rs:293** — `EngineParts.system_out:
   broadcast::WeakSender<SystemEvent>`.
2. **actor.rs:266-282** — `EngineParts { .., system_out:
   system_out.downgrade(), .. }` in `spawn_state_engine`'s `tokio::spawn`
   argument. (The moved strong parameter drops at the end of the
   function body — the caller's retained clones are what keep the
   channel open.)
3. **actor.rs:492-498** — `Seams` last element
   `&'a broadcast::WeakSender<SystemEvent>`.
4. **actor.rs:767-772** — `TickSeams` last element, same.
5. **actor.rs:759-764** — `note_storage_failure` per D3 (signature +
   upgrade-on-send body + constraint comment).
6. **actor.rs:11-14** — module-doc ownership note (D7).

Everything else in `run_loop` — the destructuring (311-319), the
drain-exit check and its ordering (347-349), the conjunct (351-353),
the biased select (354-461), the post-loop log (477) — is **untouched**.
The drain path (ADR 0008) and the `Closed` arm (457) keep their exact
semantics; the swap only makes the `Closed` arm reachable.

## §test-design — the two new tests, step by step

Both land in `crates/taskboard-state/tests/engine.rs`, in the F4
section after `engine_stays_a_daemon_when_commands_and_reports_close`
(engine.rs:1319-1346), with doc comments citing #316/#240. Typed
assertions only; no text matching; no fixed sleeps.

### T1 — `engine_exits_when_all_sources_close`

```rust
/// #316 / #240: with every source closed — commands, reports, and (the
/// #316 fix) the system channel's external producers — the engine task
/// joins cleanly. The mid-test execute pins that the engine serves
/// while any source lives: the exit is all-sources-closed, not
/// any-source-closed.
#[tokio::test(start_paused = true)]
async fn engine_exits_when_all_sources_close() {
    let Harness { handle, report_tx, system_tx, join, .. } = harness().await;

    drop(report_tx);
    drop(system_tx); // the last external strong sender: the channel closes

    // Commands are still open: the engine must still be serving.
    let outcome = handle
        .execute(StateCommand::CreateStack { title: "alive".into(), order: 1 })
        .await
        .expect("the engine serves while its command inbox is open");
    assert!(matches!(outcome, CommandOutcome::CreatedStack(_)));

    drop(handle); // all sources closed

    tokio::time::timeout(Duration::from_secs(60), join)
        .await
        .expect("the engine exits once every source closed")
        .expect("clean exit");
}
```

Why each element is load-bearing:

- **`start_paused = true`** (reminders-suite precedent, e.g.
  reminders.rs:281): current-thread runtime, paused clock. The passing
  path resolves no timer at all (reminders are disabled — the
  `sleep_until` future is never constructed, actor.rs:340-344). In the
  red world the only pending timer is the 60 s deadline and the idle
  paused runtime auto-advances to it, so the pre-change failure is
  fast, not a 60 s wall-clock burn. (Red-path speed only — see §risks 2.)
- **The mid-test execute** is the daemon-contract half of the test: it
  fails under any mutant that breaks out early while a source lives
  (`!commands_open` and — via the phase-4 panic — `!system_open`
  deletions; see §mutant-killability). `CreateStack` is the same
  liveness probe h1 uses; it needs no sync connector and enqueues no
  outbox op.
- **`..` in the destructure** drops `sync_in` (nudge receiver) —
  inert here; no nudge is triggered.
- **Clean-join ⇔ the break fired**: after all three sources close,
  the loop's only non-panic continuation is the line-351 break (the
  drain return needs `draining`, and a `select!` with every
  precondition false panics — tokio `macros/select.rs:765`). So the
  second `.expect` is a proof the conjunct was reached and taken, not
  merely that the task stopped.
- **Red pre-change** (verified by construction, state-storage-gaps D2):
  the loop's own strong sender holds the system channel open;
  `system.recv()` never resolves; the join parks; the deadline expires.

### T2 — `storage_failed_is_silent_after_the_last_external_producer_dies`

```rust
/// #316 (D3 semantics): once every external producer of the system
/// channel has died, a storage failure emits nothing — the channel is
/// closed, no receiver can appear — and the loop is unaffected: the
/// engine answers the failure with `Storage`, then recovers.
#[tokio::test]
async fn storage_failed_is_silent_after_the_last_external_producer_dies() {
    let repo = Arc::new(FlakyRepository::new(1));
    let h = spawn(Spawner {
        repo: repo.clone() as Arc<dyn TaskRepository>,
        ..Spawner::default()
    })
    .await;
    let mut events = h.system_tx.subscribe();
    drop(h.system_tx); // the last external strong sender

    let failed = h
        .handle
        .execute(StateCommand::CreateStack { title: "doomed".into(), order: 1 })
        .await;
    assert!(matches!(
        failed,
        Err(ExecuteError::Storage(RepositoryError::Unavailable))
    ));
    h.handle.flush().await;
    // Typed: nothing was ever delivered, and the channel is observably
    // closed (try_recv surfaces Closed only on a closed, drained
    // buffer — an arriving StorageFailed would surface Ok(..)).
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Closed)
    ));

    // The loop survived the upgrade failure: recovery still serves.
    let recovered = h
        .handle
        .execute(StateCommand::CreateStack { title: "after".into(), order: 1 })
        .await;
    assert!(matches!(recovered, Ok(CommandOutcome::CreatedStack(_))));
}
```

Determinism: the `try_recv` runs after the execute round-trip and the
flush barrier, so the emission (or its absence) has already happened —
no polling, no sleep. Plain `#[tokio::test]` (real time is never
waited on; matching a17's tier). **Red pre-change by intent**: today
the loop's own strong sender plus the live subscriber means the event
*is* delivered (`try_recv` → `Ok(StorageFailed)`); the delta is exactly
D3's accepted semantic change, and stage 2 records the red as evidence
rather than diagnosing a bug. The recovery assertion additionally kills
any upgrade-failure mishandling that panics or exits (the execute would
surface `EngineGone`).

## §mutant-killability — the four `delete !` conjunct mutants

The conjunct (actor.rs:351):
`!draining && !commands_open && !reports_open && !system_open`.

Mechanics used throughout: (i) after all three flags are false, a
missed break reaches `select!` with every arm precondition false
(`commands_open`/`reports_open`/`system_open` flags, `timer_armed ==
false` for the tick arm — reminders disabled in the harness), which
**panics** (`macros/select.rs:765`), surfacing as `JoinError` on the
awaited join; (ii) a mutant that breaks *early* — while a source still
lives — is caught by T1's mid-test execute returning `EngineGone`; (iii)
post-swap T1 genuinely reaches the conjunct with all flags false (the
fix's entire point), so these worlds exist for the tests to drive.

| Mutant (delete `!`) | Divergence world | Killed by | Failure mode |
|---|---|---|---|
| `!draining` → `draining` | all sources closed while *not* draining: conjunct false → no break → select! panic | **T1** | join yields `JoinError` → second `.expect` fails |
| `!commands_open` → `commands_open` | reports+system closed, commands open (T1's mid-phase): conjunct true → early break | **T1** | mid-test execute → `EngineGone` → `.expect` fails (would also panic at the final join) |
| `!reports_open` → `reports_open` | commands+system closed, reports closed at the break: conjunct false → no break → select! panic | **T1** | join yields `JoinError` → second `.expect` fails |
| `!system_open` → `system_open` | (a) commands+system closed, commands open: no early window (needs `!commands_open`); at the break system is closed → conjunct false → panic. (b) commands+reports closed, system open | **T1** (a) + existing `engine_stays_a_daemon_when_commands_and_reports_close` (b) | T1: `JoinError`; daemon test: early break → `NetworkLost` never ingested → poll deadline expires |

The first three rows are the issue's black-box-unkillable survivors
(unkillable only because `system_open == false` never held in any
reachable world — state-storage-gaps D2); the seam swap makes that
world reachable and T1 drives it. The fourth was already killed by the
wave-1 chunk-4 test and stays killed (that test must remain green —
see §snapshot-suite impact). The conjunct's *reachability* itself (a
mutant deleting the whole `if` or the `break`) dies on T1's
clean-exit expect by the same select!-panic mechanics.

## §snapshot-suite impact

None. `snapshots.rs`'s spawner keeps its own `system_tx`
(snapshots.rs:78); scenarios drive commands/reports only; the seam
swap publishes identical `AppState` values through the identical
`publish` path. No `assert_yaml_snapshot!` is added, removed, or
altered; `tests/snapshots/` is untouched. The other existing suites
that must stay byte-for-byte green: a17 (delivery through the seam,
engine.rs:557), the reminders tick-path emission twin (reminders.rs:499),
the daemon-survival test (engine.rs:1319), h1 (engine.rs:1266), the
g-series drain tests (g3/g4, engine.rs:1050-1078), a8 (engine.rs:495),
and the sync crate's `sync_pair`/`e2e_hermetic` harnesses (both retain
strong senders).

## §non-goals

- No drain-path changes: biased select order, ADR 0008 linger-drain
  semantics, and the `draining && !reports_open` return are untouched.
- No `AppState`/snapshot semantics changes; no UI changes.
- No `taskboard-app` diff at all (D2 makes the swap invisible to it);
  no changes to any other crate's tests.
- No `WeakSender` adoption for the engine's own repaint channel:
  `EngineHandle.signals` is the handle's channel; its lifetime policy
  (open while any handle lives) is deliberate and out of scope.
- No CHANGELOG entry (chunk 5 owns `[Unreleased]` for the round).
- No mutant re-runs locally; the weekly sweep confirms the kills —
  this chunk's evidence is the analytic argument in
  §mutant-killability, which stage 3 can verify by reverting the seam.

## §risks

1. **The one observable semantic delta** (D3): a subscriber that
   outlives every strong sender stops receiving `StorageFailed`
   (pre-change it would receive, courtesy of the loop's own strong
   sender). No production wiring constructs this: subscribers are
   minted by `App.system` (bootstrap), which `App::shutdown`'s
   destructure keeps alive past the engine join (bootstrap.rs:330-372),
   and the system-event logger exits on channel close by design. This
   is the cost the issue's candidate (a) names and the dispatch's D3
   accepts; recorded here so stage 3 does not re-litigate it.
2. **Auto-advance reliance is red-path speed only**: T1's passing path
   resolves no timer; if the paused runtime's idle auto-advance ever
   behaved differently than documented, the pre-change red run would
   cost one 60 s timeout instead of milliseconds — the assertion
   itself is unaffected either way.
3. **proptest's engine starts system-closed** (D6): analyzed harmless;
   the contingency protocol (evaluate as suspected bug, do not weaken)
   is recorded so stage 2 does not "fix" the suite opportunistically.
4. **Do not weaken the reds**: T1's `.expect`s and T2's
   `TryRecvError::Closed` assertion are the kill mechanics; tolerating
   `EngineGone`/`Ok(StorageFailed)` "to keep the suite green" during
   the red phase would delete the chunk's entire value (AGENTS §5).
5. **Standalone-ness of the single commit** (D5): if stage 2 finds it
   must split the work across sessions, the split point is
   tests-written-and-observed-red (uncommitted) → seam → commit —
   never a committed red test.

## §stage-2 execution checklist

1. Recreate the worktree on `fix/remediation-wave1-followups` if
   missing; read this plan + dispatch §4 + issue #316 +
   state-storage-gaps D2.
2. Write T1 and T2 first (§test-design), run
   `cargo nextest run -p taskboard-state` — **both must fail** against
   the unchanged seam (T1: deadline panic; T2: `Ok(StorageFailed)`
   mismatching the `Closed` assertion). Record the exact failures in
   the report. Any *other* failure is a suspected bug — stop and
   evaluate per AGENTS §5.
3. Land the seam per §seam-design (actor.rs only). Re-run: T1/T2
   green; the §snapshot-suite-impact list green.
4. Isolation runs: `cargo nextest run -p taskboard-state` and
   `cargo nextest run -p taskboard-sync-nextcloud` (its harnesses
   spawn the engine), then the full AGENTS §10 gate suite.
5. One commit (D5):
   `fix(state): hold the system emission seam weakly so the all-sources exit is reachable`.
   No CHANGELOG. No push, no PR (chunk 5's stage-3 ships).
6. Report: red-run evidence, per-gate results, deviations (none
   expected; D1 confirmed the dispatched candidate).
