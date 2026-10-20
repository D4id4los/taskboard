# [Plan] Kiosk Chunk K8 — Sync Metrics on `SyncStatus` + Engine Counters

- **Date**: 2026-10-20
- **Kind**: stage-1 chunk plan (TDD-first) for kiosk implementation round
  chunk 8 (`feat/state-sync-metrics`); stage-2 implements exactly this,
  stage-3 reviews against it.
- **Origin**: kiosk wave implementation plan §2 KDF-8, §3 K8, §4–§6
  (`.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`);
  dispatch plan §9 MD-K8 + MD-4
  (`.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`);
  D-K3/D-K6 rulings (briefing §1.3/§1.6; design §4, §5.3); issues
  **#156** (sync-actor metrics) and **#145** (engine counters — D-K3
  assigned it, so K8 closes both).
- **Window state (MD-K8)**: the "before MB-4" window closed (PR #368
  merged 2026-10-19T16:37Z); the user ruled the chunk dispatches **in
  the MB→DD gap** (MB wave exit `86b8dd8` is on `main`; no DD branch
  has an open PR; DD chunk 3 `feat/sync-appdata-transport` — the next
  contestor of `sync/actor.rs` — has not dispatched). This plan is
  written against the **post-MB-4** shapes; the stage-1 prompt's
  "post-0006/post-board-scoped-vocabulary" seam list was stale and has
  been fully re-verified (§2).
- **Status**: READY for stage 2.

## 1. Scope

Per-board, cumulative, process-lifetime sync counters on the published
sync status (cycles, pushes, pulls, failures, offline episodes,
last-cycle duration), populated at the **existing** `SyncReport`
ingestion publish points; carried by the existing `aggregate_rows` fold
so every surface (CLI `status` aggregate, per-connector health,
per-board rows, future dashboard) extends without re-shaping; plus a
handle-side atomic `EngineMetrics` (#145) with a `status` surface
reading it.

**Non-goals** (implementation plan §3 K8, verbatim bar): no
histograms/quantiles; no metrics crate; **no new channel, no new wake
source, no broadcast**; no UI badge change beyond what falls out of the
fold (`sync_badge_for_board` keeps reading its row); no per-HTTP-request
accounting; no cross-restart persistence of counters.

## 2. Re-verified seam table (2026-10-20, base `ba03646` = `origin/main`)

| Seam | File:line (this base) | Shape K8 builds on |
|---|---|---|
| Domain status | `taskboard-domain/src/state.rs:49` | `SyncStatus { phase, last_success, pending_ops }` — `Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize` |
| Per-board map | `state.rs:101` | `SyncStates(#[serde(transparent)] BTreeMap<BoardId, SyncStatus>)` |
| The fold | `state.rs:191` | **`aggregate_rows(rows) -> SyncStatus`** (D6-4): phase rank, max `last_success`, saturating `pending_ops` sum; `aggregate()` (:129) delegates. **Counters fold lands here once — every subset fold inherits it.** |
| Write seams | `state.rs:137/:164/:154` | `set_stored` (phase + last_success only), `mark_syncing` (phase-only transient), `remove` (DeleteSyncStatus) — all three untouched by K8 |
| Persistence | `persistence.rs:82` `PersistedState` derives serde; sqlite `migrations/0006` `sync_status(board, phase, last_error, last_success)`; `repo.rs:521` loads columns; `codec.rs:195` rebuilds rows via `set_stored` | Storage **never** sees counters: restart-from-zero is codec-shaped, no migration |
| Reports | `messages.rs:322` | `SyncReport::Completed { connector, snapshot: Box<RemoteBoardSnapshot>, validators, pushes, read_at }`, `SyncReport::Failed { connector, kind, pushes, read_at }` — **connector-tagged** (MB-4/DF-1); board-less today |
| Ingestion | `taskboard-state/src/actor.rs:642` `ingest_sync_report`; **`actor.rs:725` fast path `if actions.is_empty() { return; }`** | Completed → `apply_sync_report(.., &connector)`; Failed w/o pushes → fan-out `bound_boards_of(&connector)`; Failed w/ pushes → `apply_push_report`; publish only on `interpret` change |
| Offline episodes | `actor.rs:802` `go_offline(.., scope: Option<&ConnectorId>)`; event arms `:462–:478` | `NetworkLost` (global) and `ConnectorLost(c)` (per-connector) both flow through it; the "not already Offline" target filter is the episode edge; counters increment in its `repo Ok` arm |
| Handle | `actor.rs:111` `EngineHandle { commands, signals, state }`; spawn `:243` | The #145 home: a 4th field `metrics: Arc<EngineMetrics>`; `EngineParts` (:301) threads a clone into the loop; **zero signature churn** for `spawn_state_engine` callers |
| Status chain | `taskboard-app/src/commands/output.rs:136` `SyncStatusView::new` (:178); `status.rs:41` `StatusView` with `sync` (byte-pinned aggregate, D-W6), `connectors: Option<Vec<ConnectorStatusView>>`, `boards: Option<Vec<BoardStatusView>>`; `BoardStatusView.sync: SyncStatusView` (:178 output.rs) | One `SyncStatusView` extension reaches the aggregate, every connector health fold, and every per-board row |
| UI badge | `taskboard-ui-slint/src/presentation.rs:162` `sync_badge_for_board` | Reads the named board's row; **no change** (non-goal) |
| Insta corpus | `taskboard-state/tests/snapshots.rs:111` `snap()` renders published `AppState` **and** `repo.snapshot()` per step; s1–s10; reports constructed inline at s3 (:246), s4 (:360), s6 (:504) | The one widening pass (§4 D10) |

## 3. Architecture of the motion

Counters are **state-borne in the published projection, memory-only in
the engine** (D-K6). The domain's `SyncStatus` gains one nested
`counters: SyncCounters` field; the engine keeps the live values in a
private per-board map inside `EngineCore` and overlays them onto the
published rows in `app()`. Persistence, the storage codec, `set_stored`,
`mark_syncing`, and the `PersistenceAction` batch are untouched — a
restart re-hydrates rows without counters, which *is* the documented
restart-from-zero semantics. The sync actor's only change is emission:
two report fields it alone can know (wall duration; which board failed).

```
sync actor (emission-only: +duration, +board on Failed)
   │ mpsc SyncReport (existing channel)
   ▼
ingest_sync_report ──► repo.apply(actions) ──► core.interpret
   │                      (unchanged batch)        │
   ├─► core.record_report(board, delta)           │
   │    (EngineCore private counters map)         ▼
   └─► publish() ◄── if changed || counted ─── app() overlays
        (existing ArcSwap store + StateUpdated)   counters onto rows
                                                      │
              CLI status ◄── aggregate()/aggregate_rows() ──► UI (fold unchanged)
```

## 4. Settled decisions

**D1 — Counter lifecycle: engine-private map, published overlay.**
`EngineCore` gains `sync_counters: BTreeMap<BoardId, SyncCounters>`
(memory-only; not part of `PersistedState`). `EngineCore::app()` clones
`state.sync` then writes each row's `counters` from the map — via
`get_mut`, **only onto rows that already exist** (a deleted board's
counter facts drop out of the projection with its row; no phantom rows
re-materialize after `DeleteSyncStatus`/board delete). Rejected
alternative: counters as plain `SyncStatus` fields incremented in place
in `PersistedState.sync` — rejected because the `InMemoryRepository`
test fake keeps `PersistedState` in RAM, so a fake-based "restart" would
silently resume counters while production sqlite would not (the exact
memory ≢ disk lie `EngineCore` exists to prevent), and the persisted
corpus would carry live counter values that storage can't actually
restore.

**D2 — Shape and types.** New domain struct in `state.rs`:

```rust
/// Process-lifetime sync activity facts for one board (see D12 text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCounters {
    pub cycles: u64,
    pub pushes: u64,
    pub pulls: u64,
    pub failures: u64,
    pub offline_episodes: u64,
    pub last_cycle: Option<std::time::Duration>,
}
```

`SyncStatus` gains `#[serde(default)] pub counters: SyncCounters`
(the `serde(default)` keeps any legacy JSON decode path trivially
compatible; struct-literal construction sites — the state.rs test
helper `status()`, proptest strategies — are mechanical compile-fix
sites). All-zero/`None` defaults come from `Default`. `u64`, not `u32`:
counters are monotone, `fetch_add`-style growth is unbounded in
principle, and u64 keeps a saturating note unnecessary for the per-board
values (the **fold** still saturates, D3). `std::time::Duration`, not
`chrono::Duration`: the value is a monotonic elapsed measurement
(milled from `Instant`), not a civil time — chrono stays the house
type for timestamps only. `SyncStatus` keeps `Eq` (Duration is `Eq`).

**D3 — Fold carrying shapes** (`aggregate_rows`, `state.rs:191`):
counters fold with a `SyncCounters::fold_into(&mut self, other)`-style
step — `cycles/pushes/pulls/failures/offline_episodes` =
`saturating_add` (matches `pending_ops`; never wraps), `last_cycle` =
`max` over `Option<Duration>` (deterministic and order-independent, the
same rule style as `last_success`; the aggregate's "last cycle" reads
as the longest of the boards' last cycles). The empty fold stays
`SyncStatus::default()` — zeroed counters, no `last_cycle`. Because the
fold lives in `aggregate_rows`, `SyncStates::aggregate()`, the CLI
connector-health folds, and any future subset fold carry counters with
no further edits.

**D4 — Report emission (the ONLY sync-actor + messages change).**

- `SyncReport::Completed` gains `duration: std::time::Duration`.
- `SyncReport::Failed` gains `duration: std::time::Duration` **and**
  `board: Option<RemoteBoardId>` — the failing board in the actor's
  remote id space (DF-1-consistent: scoped by the report's existing
  `connector` field); `None` when no board identity exists (the
  commanded `NoBoard` skip). The actor knows the failing board at both
  `fail_cycle` call sites (the per-board loop's `target`), so the
  engine gets exact per-board attribution without a second report.

Exact emission lines in `crates/taskboard-sync-nextcloud/src/actor.rs`
(base `ba03646`; ~12 production lines, zero deletions, zero logic
changes):

1. `cycle()` :344, directly after `let read_at = cfg.clock.now();`:
   `let started = std::time::Instant::now();` (monotonic — wall-clock
   diffs are not NTP-step-safe).
2. `NoBoard` report :361–370: add `board: None,` +
   `duration: started.elapsed(),`.
3. push-abort `fail_cycle(..)` call :478: pass `Some(target), started`.
4. pull-failure `fail_cycle(..)` call :498: pass `Some(target), started`.
5. `Completed` construction :523: add `duration: started.elapsed(),`.
6. `fail_cycle` signature :572: add params
   `board: Option<RemoteBoardId>, started: std::time::Instant` (+ one
   doc line each).
7. `Failed` construction :593: add `board,` +
   `duration: started.elapsed(),`.

Plus `crates/taskboard-domain/src/messages.rs`: the three fields with
doc comments; and mechanical constructor churn in tests that build
`SyncReport` values (sync-crate tests, `snapshots.rs` s3/s4/s6 —
`duration: Duration::ZERO` / `board: None` unless the test asserts
them). **Trap bar (MD-K8)**: nothing else in this file moves — no new
actor state, no branches, no reshuffles. Duration semantics: wall time
from cycle start to *this board's* report; on multi-board cycles later
boards include earlier boards' time (the engine folds max, D3; the
dashboard reads the aggregate).

**D5 — Engine-side attribution and increment points**
(`ingest_sync_report` `actor.rs:642`, `go_offline` `:802`):

| Report | cycles | pushes | pulls | failures | last_cycle |
|---|---|---|---|---|---|
| `Completed` (resolve local board: the merged board whose tagged binding matches `(connector, snapshot.board.id)` — the same resolution `append_validators_if_changed` uses) | +1 | +`pushes.len()` | +`1 + stacks + tasks + labels` (every remote entity the snapshot carried) | — | =`duration` |
| `Failed { board: None, .. }` (connector-wide verdict) | +1 per `bound_boards_of(&connector)` | — | — | +1 per board | =`duration` per board |
| `Failed { board: Some(remote), .. }` (evidence-then-verdict) | +1 on the resolved board | +`pushes.len()` (the report's `board_ops` are that board's ops) | — | +1 | =`duration` |

- Unresolvable attribution (binding vanished mid-cycle): **drop the
  delta, `tracing::debug!`** — counters are observability, not
  accounting; no ceiling-style compensation.
- Network pull-failure leaves the connector's remaining boards
  unattempted *with no reports* (actor `:506`: "go_offline stamps their
  rows"): those boards get **no** cycle/failure count — no report, no
  count; the episode counter (below) is what records the event. This is
  the documented semantics, not a gap.
- `go_offline` `Ok(())` arm: `offline_episodes + 1` per written target
  board. The existing "not already Offline" filter makes repeat
  `NetworkLost`/`ConnectorLost` events within one episode a no-op —
  one episode per transition, for both the global and per-connector
  event classes, for free.
- **Ordering vs the fast path**: counting sits in exactly two places —
  the `actions.is_empty()` fast path (count, then `publish` if anything
  counted, then return) and the `repo.apply` `Ok(())` arm (count, then
  `publish` if `interpret` changed **or** anything counted). On
  `Err(_)` (batch lost): nothing counted, nothing published — matches
  the existing discard rule. A no-change cycle therefore **still
  counts its cycle and still publishes** — counters ride the existing
  ingestion wake; this is not a new wake source (the trap MD-K8 names).
- Counter motion never advances `last_updated` (no persisted batch; the
  field's contract is "engine mutation" in the durable sense) and never
  enters `interpret`'s actions — recorded strictly after `interpret`,
  before `publish`.

**D6 — `EngineCore` API** (`core.rs`): two small mutators,
`record_report(&mut self, board: BoardId, delta: SyncCounters)`
(saturating-accumulate into the map, default-entry on first sight) and
`record_offline_episode(&mut self, board: BoardId)`; plus the overlay
folded into `app()`. `sync_status()`/`persisted_view()` untouched —
`persisted_view()` keeps mirroring the persisted shape bit-for-bit
(counters are invisible to it; its doc stays true).

**D7 — `EngineMetrics` (#145)** — new file
`crates/taskboard-state/src/metrics.rs` (SPDX MIT OR Apache-2.0;
crate-level `forbid(unsafe_code)` already applies; no serde in this
crate — the serde view lives app-side, D9):

```rust
#[derive(Debug, Default)]
pub struct EngineMetrics {
    commands: AtomicU64,
    rejections: AtomicU64,
    signals: AtomicU64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineMetricsSnapshot { pub commands: u64, pub rejections: u64, pub signals: u64 }
```

- `Arc<EngineMetrics>` minted in `spawn_state_engine` (:270): one clone
  on `EngineHandle` (new 4th private field), one threaded to the loop
  via `EngineParts`. No public constructor churn.
- `EngineHandle::metrics(&self) -> EngineMetricsSnapshot` — three
  `Relaxed` loads into the snapshot (per-counter independence is fine:
  observability, no synchronization role — documented on the struct).
- Increments, all `fetch_add(.., Relaxed)`:
  - `commands` in `execute_command` **after the drain gate** (declined
    commands are refusals, not work — never counted; only
    `EngineEnvelope::Execute` counts — `Flush`/`ReadState`/
    `AttachConnector` are plumbing, not commands).
  - `rejections` at the `plan_command` `Err` arm (:578 in the loop's
    execute path) — the typed `Rejected` receipt is the event.
  - `signals` inside `publish()` (:516) — every `StateUpdated` send
    attempt across all three publish paths (commands, ingestion,
    reminders, offline). "Publishes attempted", including
    zero-receiver sends — the honest engine-activity number.
- Never in `AppState`, never in a snapshot, no insta surface (D-K6;
  the trap the dispatch plan names).

**D8 — Status surfaces** (`taskboard-app`):

- `SyncStatusView` (:136 output.rs) gains `cycles`, `pushes`, `pulls`,
  `failures`, `offline_episodes: u64` and
  `last_cycle_ms: Option<u64>` — duration converted app-side
  (`as_millis` clamped via `u64::try_from().unwrap_or(u64::MAX)`; JSON
  stays flat numbers, never `{secs, nanos}`). `SyncStatusView::new`
  projects once; the aggregate `sync` field, every
  `ConnectorStatusView.health`, and every `BoardStatusView.sync` inherit
  the counters with zero further edits (that is the D6-4 fold payoff).
- `StatusView` (:41 status.rs) gains
  `engine: Option<EngineMetricsView>` —
  `EngineMetricsView { commands, rejections, signals: u64 }` (serde
  projection in `status.rs`), `#[serde(skip_serializing_if =
  "Option::is_none")]` like its siblings. Filled `Some` by
  `StatusView::server()` (daemon-side; `app.engine.metrics()`) **and**
  `StatusView::in_session(app)` (the session holds the core — same
  read, same honesty); `no_daemon()` stays `None`. The wire type is
  `StatusView` itself (Deserialize both ends), so the IPC-relayed
  answer carries the field with no protocol edit; the
  `{"daemon": false}` byte pin (D-W6) is preserved by the skip.
- `render_text` may append the counts to the human line — presentation
  only, **no new text assertions** (AGENTS §5); the structural tests
  (§5 T15–T17) pin the JSON/typed facts.
- REPL `status` line needs no dispatch change: it already renders
  `StatusView::in_session`.

**D9 — Insta pass catalogue (exactly one deliberate regeneration).**
`cargo insta test -p taskboard-state` then `cargo insta accept`:

- All `snapshots__s*_published_app_state.snap` (10): every `sync` row
  gains `counters:` — zeros in s1/s2/s5/s7–s10, **populated** in s3
  (one Completed: cycles 1, pulls = snapshot size, pushes = report's
  outcomes, `last_cycle` = the test's `Duration::ZERO` → `None`?
  — **no**: `Some(ZERO)` is what the report carries; the corpus shows
  `last_cycle: 0s`), s4 (second Completed on top), s6 (Failed verdict
  per connector board + the `NetworkLost` episode).
- All `snapshots__s*_persisted_state.snap` (10): `counters:` appears
  zeroed inside each `sync` row (the field exists on the shared
  `SyncStatus` type) — the widening reviewer must expect **zeros here,
  always**; any non-zero persisted counter is a bug (D1 violated).
- No `.pending` files ship; no other suite's snapshots change
  (`cargo insta test` workspace-wide must show this diff and nothing
  else). s3/s4/s6 test-code `SyncReport` constructors gain the new
  fields with values that make the populated corpus rows
  hand-checkable (e.g. `duration: Duration::from_millis(250)` in s3 →
  `last_cycle: 250ms` in the accepted snap).

**D10 — `restart_from_zero` proof (structural, not codec-dependent).**
`EngineCore::from_persisted` leaves `sync_counters` empty and
`EngineMetrics` is minted fresh in `spawn_state_engine` — a new engine
over the *same* repository starts from zero by construction. The test
(T11) spawns engine #1, ingests a report, drops it, spawns engine #2
over the same `InMemoryRepository`, and asserts the default aggregate —
valid over any repository implementation precisely because the counters
never entered `PersistedState`.

**D11 — Cumulative-since-boot doc text (lands verbatim on the struct
and mirrored in the CHANGELOG line):**

> Counters are cumulative per process run: they start at zero at every
> boot, live only in the state engine's memory, and are never
> persisted — a restart resets them by construction. Cross-restart
> accumulation would need durable per-board rows (a storage migration)
> for numbers no kiosk dashboard has asked for; the live process is
> what the dashboard reads. `last_cycle` is the last cycle's wall
> duration for that board, however the cycle ended.

**D12 — Out of scope, recorded**: per-connector counter *partitioning*
beyond what `aggregate_rows` already gives surfaces (a connector's
health fold carries its boards' counters — enough); remote-side rate
limit / retry-depth gauges (#156's "retry depth" is served by
`failures` + the existing `failure_streak` backoff, which is actor
memory and stays there); any dashboard rendering (K11's job, content
gated on this chunk).

## 5. TDD plan (red → green per phase; AGENTS §5 rules)

Typed outcomes only; no text assertions; no sleeps (`pause()`/
`advance()`; the engine harness already advances a `FixedClock` one
minute per message); one behavior, one test, lowest layer that owns it.

**Domain (`taskboard-domain/src/state.rs` tests)**
- **T1** `sync_counters_default_is_zeroed` — `SyncCounters::default()`
  is all-zero with `last_cycle: None` (structural default pin).
- **T2** `aggregate_rows_folds_counters_saturating_and_takes_max_last_cycle`
  — two rows with distinct counter values; sums per field,
  `last_cycle` = max; a `u64::MAX` cycles row pins saturation (never
  wraps; the code got strictly better test still passes — floors only).
- **T3** proptest `aggregate_counter_sums_are_conserved` — mirrors the
  existing `pending_ops` conservation proptest for all five counters +
  max-law for `last_cycle` (deterministic double-fold already pinned by
  the existing determinism proptest's full-equality assert — it
  widens automatically).
- (Empty-fold zeroing is already pinned by
  `empty_map_aggregates_to_the_default_status`'s full-equality assert —
  no duplicate.)

**Engine core (`taskboard-state/src/core.rs` tests)**
- **T4** `the_overlay_writes_counters_only_onto_existing_rows` — a
  counter entry for a row-less board does not materialize a row; an
  existing row receives its counters (D1/D-rejection-of-phantom-rows
  pin).

**Engine actor (`taskboard-state/tests/` — Scenario harness)**
- **T5** `a_completed_report_counts_cycle_pushes_pulls_and_duration_on_its_board`
  (s3-shape; asserts the row's five fields + `last_cycle`).
- **T6** `a_no_change_cycle_still_counts_and_publishes` — **the trap
  test**: replay an identical Completed report (nothing merges,
  `actions` empty); the cycle counter moves AND an `EngineSignal`
  arrives (deadline poll on the broadcast receiver — no sleeps).
- **T7** `a_connector_wide_failure_counts_every_connector_board` —
  Failed, `board: None`: cycles/failures/`last_cycle` on each board of
  the connector, pushes untouched.
- **T8** `a_board_scoped_failure_counts_only_the_failing_board` —
  Failed, `board: Some(remote)`: counters land on the resolved board
  alone, including its pushes.
- **T9** `network_lost_counts_one_episode_per_board_until_restored` —
  two `NetworkLost` in a row → episodes 1 (edge semantics); a cycle
  offline→restored→lost → 2.
- **T10** `counters_restart_from_zero_on_a_fresh_engine_over_the_same_repo`
  (D10).
- **T11** `nudges_and_mark_syncing_count_no_cycles` — `RequestSync`
  flips the transient, counts nothing (reports-only semantics pin).

**Engine metrics (`crates/taskboard-state/src/metrics.rs` + actor
tests)**
- **T12** `the_handle_counts_commands_rejections_and_signals` — one
  accepted mutating command → `(1, 0, 1)`; one rejected command →
  `(1, 1, 1)`; exact snapshot equality (`EngineMetricsSnapshot` is
  `Copy + PartialEq`). No upper-bound asserts anywhere (no-ceilings
  rule).

**Sync actor emission (`taskboard-sync-nextcloud` tests)**
- **T13** `reports_carry_duration_and_the_failing_board` — the
  completed-cycle test asserts `duration` measured (> 0 under
  `pause()`-advanced time is not guaranteed → assert the **field is
  present and the board is right**; duration's zero-vs-positive is
  timing-dependent, so pin the *carry*, not the clock: `board ==
  Some(target)`, `duration` reaches the report struct; the Failed test
  pins `board: Some(target)` on pull-failure and `board: None` on the
  commanded NoBoard skip). Constructor churn: every existing
  `SyncReport` literal in tests gains the new fields.

**App surfaces (`taskboard-app` tests)**
- **T14** `sync_status_view_projects_counters_and_last_cycle_ms` —
  projection of a folded status; duration→millis conversion pin.
- **T15** `status_view_carries_engine_metrics_when_the_core_is_held` —
  `server()`/`in_session()` views carry `engine: Some(..)`,
  `no_daemon()` carries `None` (reuse the module's existing fixture
  style; if `server()` needs a heavier `App` fixture than the module
  tests hold today, assert the in_session/no_daemon arms structurally
  and cover `server()` through the existing daemon IPC status test
  journey — stage-2 verifies which fixture exists before writing).
- **T16** `the_engine_key_is_skipped_when_absent` — serde pin mirroring
  `the_connector_key_is_skipped_when_absent`.

## 6. Implementation phases (stage 2)

1. **Phase A — domain shapes + emission** (compiles workspace-wide):
   `messages.rs` fields; `state.rs` `SyncCounters` + `SyncStatus`
   field + `aggregate_rows` fold (+T1–T3); sync-actor emission lines
   (D4 list) + test constructor churn (+T13). Green before touching
   the engine.
2. **Phase B — engine counting** (TDD): `core.rs` map/overlay/record
   (+T4); `actor.rs` increment points + publish flags (+T5–T11);
   `metrics.rs` + handle wiring (+T12). Then **the one insta pass**
   (D9), reviewed as exactly the widening.
3. **Phase C — surfaces**: `output.rs`/`status.rs` views (+T14–T16);
   `render_text` presentation line; CHANGELOG Unreleased lines
   (one for the counters, one for `EngineMetrics`).
4. **Bookkeeping**: full AGENTS §10 gate suite green; no tracker edits
   (MD-4 — `Closes #156`/`Closes #145` ride the PR body at ship time);
   no push, no PR from stage 2; teardown per the worktree lifecycle.

## 7. Quality gates (AGENTS §10, verbatim sequence)

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
-- -D warnings`; `cargo nextest run --workspace`;
`cargo test --doc --workspace`; `cargo deny check`;
`cargo llvm-cov nextest --workspace --all-features --no-tests=warn`;
`scripts/coverage-quarantine-profraw.sh target/llvm-cov-target`;
`cargo llvm-cov report --lcov --output-path lcov.info`;
`rm -rf target/llvm-cov-target`. New file `metrics.rs` carries the
library SPDX header; no `.pending` snapshots in the diff.

## 8. Risks & recorded deviations

- **Window collapse mid-flight**: DD chunk 3
  (`feat/sync-appdata-transport`) rewrites the same emission file; the
  chunk is S–M and must land before DD-3 dispatches (dispatcher holds
  the gate; MD-K8's "never proceed onto a contested base" applies at
  stage 2's rebase too — if DD-3 lands first, STOP and re-plan).
- **Publish-frequency change**: no-change cycles now publish (counter
  motion). This rides the existing ingestion wake at cycle cadence
  (30 s poll) — not a new wake source, but stage 3 must not read the
  unconditional publish as scope creep; it is D5's documented rule.
- **Overlay/interpret interaction**: counters move outside
  `interpret`; the publish condition is `changed || counted`. A missed
  OR-arm is the likeliest stage-2 slip — T6 pins it.
- **Corpus discipline**: persisted snapshots must show zeroed counters
  everywhere; populated persisted counters = D1 violated. The stage-3
  reviewer checks the 10 persisted snaps for exactly this.
- **`#[serde(default)]` + struct-literal churn**: `SyncStatus`
  literals exist in domain tests/strategies; the compiler enumerates
  them — mechanical, but do not "fix" by weakening a strategy.
- **Attribution drops**: unresolvable boards drop their delta
  (debug-logged). This is a counter, not accounting — no compensation
  logic, no ceiling asserts on "counted everything".
- **Stale-prompt caveat recorded**: the dispatch prompt's seam list
  assumed "MB chunks 1–2"; this plan re-verified against `ba03646`
  (MB-4 + kiosk phase-A landed). The `board`-field emission (D4) is a
  small superset of the originally imagined "duration-only" emission —
  it is what exact per-board attribution costs post-MB-4, it stays
  emission-only, and it is called out here so stage 3 reviews it as
  planned scope, not drift.

## 9. Estimated diff

| Area | Production | Tests/snapshots |
|---|---|---|
| `domain/messages.rs` | ~14 | — |
| `domain/state.rs` | ~55 | ~70 |
| `state/core.rs` | ~45 | ~30 |
| `state/actor.rs` | ~55 | — |
| `state/metrics.rs` (new) | ~40 | ~45 |
| `state/tests/snapshots.rs` + 20 `.snap` | — | ~10 + regen |
| `sync-nextcloud/actor.rs` | ~12 | ~15 |
| `app/commands/output.rs` + `status.rs` | ~45 | ~55 |
| `CHANGELOG.md` | 2 | — |
| **Total** | **~270** | **~225 + snapshot regen** |

S–M confirmed (dispatch plan §1 row 8). The contested file
(`sync-nextcloud/src/actor.rs`) stays at ~12 production lines,
emission-only.
