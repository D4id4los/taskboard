# [Plan] Sync Actor/Executor Test Gaps — the F6-04/05/06/23/24 set (#242)

- **Date**: 2026-10-18
- **Kind**: stage-1 plan for chunk 2 of the remediation-followups
  dispatch (dispatch §1/§3, prompt 2.1). Planning only — no
  implementation in this commit.
- **Origin**: issue #242, normative, with test shapes from the chunk-6
  findings index of
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md`
  (the F6-04/05/06/23/24 entries). This chunk is **test-only**: no
  sync production changes; the single production-adjacent diff allowed
  is the domain `test-support` fail-reads flag the F6-06 streak test
  needs (dispatch §3).
- **Branch / worktree**: `fix/remediation-wave1-followups` in
  `../taskboard-WT-fix-remediation-wave1-followups` — the shared
  worktree created by chunk 1. Chunk 1 (#314/#319) has landed on the
  branch; this plan anchors to that state.
- **Anchor base**: all `file:line` anchors below are against the
  branch tip `4efb4c7` (post chunk-1). Chunk 1 touched only
  `src/push_exec/execute.rs`, `src/push_exec/mod.rs`,
  `src/push_exec/tests.rs`, and `tests/actor_wiremock.rs`
  (+ ADR/docs) — **`src/actor.rs` is unchanged since the audit
  baseline**; its audit line references drift only ~5 lines. The
  create-path arms this chunk pins (`create_task`'s archive/done
  sub-calls) were not reworked by chunk 1 (that rework was
  `update_task`'s label-delta arms and `delete_card`'s re-resolve).
- **Size**: M (per the dispatch's grade).

---

## §re-anchor — the audit's line references, remapped to `4efb4c7`

| Audit reference | Current home @ `4efb4c7` |
|---|---|
| F6-04 survivor `actor.rs:796-802` (`inline_labels` → `vec![]`) | `actor.rs:803-809` (`fn inline_labels`); consumed by the snapshot assembly `actor.rs:739-744` (`labels` field of `RemoteBoardSnapshot`) |
| F6-05 survivor `actor.rs:143-147` (`PullCache::clear` → `()`) | `actor.rs:148-152` (`fn clear`); call sites: the `SetBoard` select arm `actor.rs:204` and the absorber arm `actor.rs:260` |
| F6-06 survivors `actor.rs:253:46` ×2, `253:56` (absorber guards) | the absorber's drain loop `actor.rs:255-271` — the `SetBoard(id) if st.target != Some(id)` guard `actor.rs:258`, the `cache.clear()` `actor.rs:260`, `rerun_requested = true` `actor.rs:261` |
| F6-06 survivor `actor.rs:317:31` ×2 (streak increment) | the `read_state` failure arm of `cycle`, `actor.rs:316-325` — `st.failure_streak += 1` at `actor.rs:322` |
| F6-06 survivor `actor.rs:357:67` (`bound_board` lookup) | `actor.rs:362` (`persisted.boards.values().find(|b| b.remote == Some(target))`), feeding `local_board` `actor.rs:409` → the synthesized tombstone's title/color `actor.rs:617` |
| F6-23 survivors `actor.rs:670:12` + `670:46` (dedupe guard) | the archived-merge loop `actor.rs:676-687` — stack dedupe `actor.rs:677`, card dedupe `actor.rs:681` (`seen_cards.insert`) |
| F6-23 survivor `actor.rs:716:43` (flag-stamp compare) | the detail-refresh mismatch stamp `actor.rs:723` (`refreshed.archived != local_archived` → `max(clock.now())` at `actor.rs:724`) |
| F6-24 survivors `push_exec.rs:912:5` ×2 (`is_transport` → true/false; callers 494, 510) | `error.rs:69-73` (`DeckError::is_transport`); call sites in `create_task`'s post-create chain: the archive sub-call `execute.rs:445`, the done-stamp sub-call `execute.rs:461`. The old caller lines 494/510 were the pre-module-tree numbering of these two arms |

## §gap-table — finding → existing coverage → new test

| Finding | Existing coverage (why the mutants survived) | New test(s) | Tier / file |
|---|---|---|---|
| **F6-04** pulled labels unasserted below tier 2 | Only the tier-2 docker test `it_nextcloud_docker_sync_server_labels_adopt` (never inside the mutant box); every A-series mount passes `labels: []`; `e2e_hermetic` asserts *pushed* card-label bodies, not pulled board labels | `a23_pulled_board_labels_reach_the_snapshot` | hermetic wiremock, `tests/actor_wiremock.rs` |
| **F6-05** board-switch cache bleed | A10 pins the cold-cache/warm-validator refetch (the cache-*miss* half); no test switches boards — `PullCache::clear` on `SetBoard` is never observed | `a24_board_switch_serves_the_new_board_after_the_cache_clear` (A→B→A) | hermetic wiremock, `tests/actor_wiremock.rs` |
| **F6-06a** absorber `SetBoard`-during-cycle | A3 pins `SyncNow` storm collapse only; a `SetBoard` queued mid-cycle (target switch + clear + one rerun) is untested | `a25_set_board_during_a_cycle_is_absorbed_into_one_switch` | hermetic wiremock, `tests/actor_wiremock.rs` |
| **F6-06b** read-failure streak | Unreachable: `InMemoryRepository::read_state` cannot fail, so `actor.rs:316-325` has no harness path | domain fail-reads flag (§domain-flag) + `a26_unreadable_state_backs_off_then_recovers` | domain `test-support` + hermetic wiremock |
| **F6-06c** `bound_board` tombstone title/color | A11 pins the unlisted+404 tombstone's existence; A19 pins its stamps; neither pins that the title/color come from the *local* bound board | `a27_unlisted_board_tombstone_keeps_the_local_title` | hermetic wiremock, `tests/actor_wiremock.rs` |
| **F6-23** archived-listing dedupe + flag-stamp | A9 merges an archived listing with *no twin* and *no flag disagreement*; A21 (landed with #259, post-dating the mutant run) pins the mismatch stamp — the audit's `716` survivor is already killed by A21 today; the dedupe guard remains unpinned | `a28_archived_twin_dedupes_and_the_flag_is_adopted_with_the_observed_stamp` (pins both properties per the index's one-test shape) | hermetic wiremock, `tests/actor_wiremock.rs` |
| **F6-24** create-path transport/rejection discrimination | A4 pins a create-POST transport abort at the actor tier (`landed` empty); the E5 unit test pins a listing-transport abort (`landed` empty); **no test exercises a *post-create sub-call* failure** — the arms where the create already landed and its evidence must ride `Aborted { landed }` | `post_create_archive_transport_failure_aborts_with_the_create_as_landed` + `post_create_archive_rejection_is_tolerated_without_aborting` | crate unit suite (wiremock-backed), `src/push_exec/tests.rs` |

**Count ruling (recorded decision D8)**: the dispatch acceptance reads
"five new tests, one per finding". The findings index — the normative
shape source — itself specifies finer counts (F6-06: "2 wiremock tests
+ fake read-failure flag"; F6-24: "two `push_exec` suite tests"), and
AGENTS §5's one-behavior-one-test rule splits F6-06's three arms. The
count is therefore read as a **floor of one test per finding** with the
per-finding shapes taken from the index: **7 sync tests + 1 domain
routing test** in total.

## §domain-flag — the F6-06 fail-reads fake flag

The only cross-crate diff, in
`crates/taskboard-domain/src/test_support/memory.rs` — inside the
`#[cfg(any(test, feature = "test-support"))]` module (`lib.rs:47-48`),
so **no production surface is added** (review criterion, prompt 2.3).

```rust
pub struct InMemoryRepository {
    state: Mutex<PersistedState>,
    fail_reads: std::sync::atomic::AtomicBool, // test seam, default false
}

impl InMemoryRepository {
    /// Test seam: while set, every read (`load` and `read_state`)
    /// fails with `RepositoryError::Unavailable`; apply still
    /// succeeds. Exists to reach the sync actor's read-failure arm,
    /// which a healthy fake can never take.
    pub fn set_read_failure(&self, failing: bool) {
        self.fail_reads.store(failing, std::sync::atomic::Ordering::SeqCst);
    }
}
```

- **D1 — shape**: a `bool` + `set_read_failure(bool)` setter, exactly
  the "fail-reads fake flag" the issue and the findings index name. A
  countdown (`set_read_failures(n)`) was considered for tighter
  scenario control but is unnecessary once the test drives the flag
  across virtual-time windows (see a26's determinism note): the actor
  only progresses while the test's wait helpers advance time, so
  flipping the flag between advances is race-free.
- **D2 — failing surface**: both read paths (`load` and `read_state`)
  fail; `apply` keeps succeeding. The fake's story is "one source of
  truth" — a broken store fails every read; the sync scenario only
  needs `read_state`, but failing `load` too keeps the fake coherent
  and costs one `if`.
- **D3 — error variant**: `RepositoryError::Unavailable` (transient /
  "engine mid-restart", matching the arm's comment at `actor.rs:318`).
  `Corrupted` exercises the same arm and adds nothing.
- **Routing-contract extension** (same file, `read_port_tests`):
  `in_memory_read_state_fails_while_the_read_failure_flag_is_set` —
  flag set → `read_state`/`load` return `Err(Unavailable)` (typed
  match, no text); flag cleared → reads succeed again. Mirrors the
  existing `in_memory_read_state_returns_the_post_apply_shape`
  precedent ("routing contract, one per impl"); the flag changes
  `read_state`'s routing, so the contract test extends with it.
- `Debug`/`Default` derive unchanged (`AtomicBool` defaults to false).

## §tests — TDD catalogue

All hermetic-tier tests follow the A-series harness invariants
(`tests/actor_wiremock.rs:1-37`): `#[tokio::test(start_paused = true)]`,
`cfg()` with `poll_interval` 3 600 s (cycles are command/backoff-driven
only, so request counts stay exact), the wall-clock-bounded
`next_report`/`start_server` helpers as the only places time moves,
typed assertions only. Unit-tier F6-24 tests use plain
`#[tokio::test]` + the `InstantSleeper` seam (`push_exec/tests.rs:1755`)
so retry exhaustion costs no real time.

### a23 — F6-04: pulled board labels reach the snapshot

- **Mount**: `mount_conditional_pull(&server, &labels)` with
  `labels = json!([{"id": 1, "title": "todo", "color": "ff0000"}])`
  (the recorded wire shape, `tests/fixtures/deck/boards_list.json:35`;
  `lastModified` omitted — the model defaults it). First cycle is cold
  (no seeded validators), so every read answers 200 fresh.
- **Seed**: `repo_with(&[(2, STACK)], &[])`; `set_board_and_report`.
- **Assert**: `SyncReport::Completed { snapshot, .. }`;
  `snapshot.labels` has length 1; the entry's `id.board ==
  RemoteBoardId(BOARD)`, `id.label == RemoteLabelId(1)`, `title ==
  "todo"`, `color == "ff0000"`.
- **Kill**: `inline_labels → vec![]` empties `snapshot.labels` →
  length assertion fails.

### a24 — F6-05: A→B→A serves A after the cache clear

- **Mount** (hand-rolled in the test — the shared helpers are
  single-board by design; do not churn `common/hermetic.rs` for one
  test, D6): one conditional responder for every GET, 304 iff the
  request carries `If-None-Match: "gen-1"`, else 200 + `ETag "gen-1"`,
  serving by path: `/boards` → a listing with **both** boards (42
  "alpha board", 43 "beta board"); `/boards/42` detail; `/boards/43`
  detail; `/boards/42/stacks` → stack 7 titled **"alpha col"**;
  `/boards/43/stacks` → stack 70 titled **"beta col"**; both
  `stacks/archived` → `[]`. Board titles/colors and stack titles are
  pairwise distinct so a bleed is observable.
- **Seed**: `repo_with(&[(2, STACK)], &[])` (bound to board 42) +
  validators pinned exactly like A10 (`PersistenceAction::
  UpsertValidators` for `Boards`, `Stacks(42)`, `ArchivedStacks(42)`).
  Nothing persists report validators in the actor-only harness, so the
  pinned validators make every later conditional read 304 (D5).
- **Sequence**: `SetBoard(42)` → report 1 (Completed; the reads 304 +
  cold cache → refetch, filling the cache with board 42's listings).
  `SetBoard(43)` → report 2 — assert `snapshot.board.id ==
  RemoteBoardId(43)` and its stacks are beta's. `SetBoard(42)` again →
  report 3.
- **Assert**: report 3 `snapshot.board.id == RemoteBoardId(42)` and
  `snapshot.stacks` is exactly `["alpha col"]` — **no beta stack may
  appear under board 42**; secondary: report 3's cycle made
  unconditional refetches (boards listing + stacks + archived =
  3 refetch requests on top of the 304s + the board-42 detail), the
  A10 warmth rule firing because the clear emptied the cache.
- **Kill**: `PullCache::clear → ()` leaves board 43's cached listings
  in the slots; re-`SetBoard(42)` 304s into serving **beta's** stacks
  as board 42's snapshot → the stacks assertion fails.
- **Determinism**: each `SetBoard` is consumed while the actor idles
  in `select!` (the previous report was received first), and all three
  targets differ from the current one at send time, so the same-id
  no-op arm (`actor.rs:200-202`) never interferes.

### a25 — F6-06a: `SetBoard` during a cycle is absorbed into one switch

- **Mount**: the two-board conditional responder of a24 (or
  single-board mounts per board — reuse a24's responder; the pull
  content is not the observable here, D6).
- **Sequence**: `SetBoard(42)` → report 1. Then send `SyncNow` and
  `SetBoard(43)` **back-to-back** without awaiting anything in
  between: both mpsc sends complete synchronously against capacity, so
  both sit in the inbox before the actor is polled — the exact A3
  storm shape. The actor consumes `SyncNow` in `select!` (cycle 2 for
  board 42), and the `SetBoard(43)` lands in the absorber's drain loop
  (`actor.rs:255-271`) while cycle 2 runs.
- **Assert**: report 2 is board 42 (`snapshot.board.id ==
  RemoteBoardId(42)`); report 3 is board 43; then A3's silence window
  (advance ~5 s virtual inside the poll horizon, `try_recv` must stay
  empty) — the switch produced exactly one rerun, no fourth cycle.
- **Kill**: the absorber guard `st.target != Some(id)` mutated to `==`
  drops the different-board `SetBoard` into the catch-all
  (`actor.rs:263`) — no switch, no rerun, report 3 never arrives →
  `next_report` panics at its wall deadline. Deleting the
  `rerun_requested = true` line dies the same way.

### a26 — F6-06b: unreadable state backs off, then recovers

- **Mount**: `mount_conditional_pull(&server, &json!([]))` — the
  recovered cycle must be able to complete a full pull.
- **Sequence** (the flag is the only production-adjacent seam):
  1. `repo.set_read_failure(true)`.
  2. Send `SyncNow` (target resumes from the persisted binding —
     decision 13; no `SetBoard` needed).
  3. Advance a short virtual window (~300 ms in 10 ms steps, the A3
     silence-loop pattern): the commanded cycle runs, `read_state`
     fails → **no report, no broadcast, no HTTP** — assert
     `server.received_requests()` is still empty after the window
     (the read precedes every wire call; also pins that a storage
     failure is not a network event: `system_rx.try_recv()` err).
  4. `repo.set_read_failure(false)`.
  5. `next_report` → the backoff-scheduled autonomous cycle succeeds:
     `SyncReport::Completed`.
- **Assert**: the Completed report arrives at all (that *is* the pin);
  optionally that its arrival sits at/after the initial backoff from
  the failing cycle (the A14-style `tokio::time::Instant` comparison,
  ≥ `cfg().backoff_initial` since the failure) — include it: it is
  free and tightens the streak semantics.
- **Kill**: with `st.failure_streak += 1` deleted, `reschedule` sees
  streak 0 → `poll_interval` (3 600 s) → the report never arrives →
  wall-deadline panic. With `-=` the u32 underflow either panics the
  actor task or wraps to a saturating-huge backoff — no report either
  way. 
- **Determinism**: under paused time the actor runs whole cycles
  atomically between the test's `advance().await` steps; the flag
  flips happen while the actor is parked on `sleep_until`, so "flag on
  for the failing cycles, off for the recovering one" is exact, not
  racy (D1's rationale).

### a27 — F6-06c: the unlisted-board tombstone keeps the local title

- **Mount**: A11's — `/boards` answers an empty listing; `/boards/42`
  answers 404 (`.expect(1)`).
- **Seed**: `repo_with`-style state but with a **distinctively titled**
  local board (e.g. title `"the local name"`, color `"00ff00"` — the
  stock helper's title `"board"` is indistinguishable from a fixture
  fallback; build the state inline, the A18/A22 precedent).
- **Assert**: `Completed` with `snapshot.board.deleted_at.is_some()`
  (the A11 fact, restated for context) **and** `snapshot.board.title
  == "the local name"` and the color matches — the tombstone's
  identity fields come from the bound board, not an empty fallback.
- **Kill**: the `bound_board` lookup's `==` → `!=` makes `find` return
  `None` (one board, predicate now false) → `local_board` falls back
  to `("", "")` at `actor.rs:617` → title assertion fails.

### a28 — F6-23: archived twin dedupes; the flag is adopted with the observed stamp

- **Mount** (hand-rolled, A9's pattern): `/boards` + `/boards/42`
  healthy; **active** listing: stack 7 with card 55
  (`archived: false` — the lagging live listing); **archived**
  listing: a stack-7 twin (`archived: true`, cards: `[55]`) **and**
  stack 99 (`archived: true`, cards: `[77]`); detail GET
  `/boards/42/stacks/7/cards/55` → card 55 with `archived: true` (the
  authoritative out-of-band archive).
- **Seed**: one bound task at card 55, locally `archived: false`
  (drives the detail refresh + the flag disagreement); nothing bound
  to 77.
- **Assert**:
  - `snapshot.tasks` filtered by `id.card == 55`: **exactly one**
    entry, `archived == true` (the detail settles it), and
    `last_modified == stamped()` (`ts(T + 1000)` — the observation
    instant, strictly after every fixture stamp);
  - filtered by `id.card == 77`: exactly one entry, `archived == true`
    (listing-decided, unbound → no refresh);
  - `snapshot.stacks`: the remote stack ids are exactly `{7, 99}`,
    each once.
- **Kill**: the stack-dedupe mutants (`actor.rs:677`) duplicate 7
  and/or drop 99 → the set assertion fails; the card-dedupe mutants
  (`actor.rs:681`) emit card 55 twice or swallow 77 → the count
  assertions fail; the stamp-compare mutant (`actor.rs:723`) leaves
  `last_modified` at the fixture stamp → the stamp assertion fails
  (already killed by A21 post-#259; re-asserted here because the
  findings index's one-test shape names both properties — recorded as
  accepted redundancy, D7).

### F6-24 unit pair — post-create sub-call discrimination

Both in `src/push_exec/tests.rs`, plain `#[tokio::test]`, client built
with `.with_sleeper(Arc::new(InstantSleeper))` so the 503 retry
exhaustion is real-time-free.

**`post_create_archive_transport_failure_aborts_with_the_create_as_landed`**

- **Mount**: `POST /boards/42/stacks/7/cards` → 200
  `card_json(55, 7, 0)` (`.expect(1)`); `PUT
  /boards/42/stacks/7/cards/55/archive` → **503** (`.expect(3)` — the
  policy's three attempts; 503 → `Unavailable` → transport-class).
- **Exec**: `PushExecutor::new(&client, overlay_with(42, &[(5, 7)],
  &[], &[]))`, one `CreateTask` group with `new_card.archived = true`,
  `done: None`, no subsumed ops.
- **Assert**: `GroupOutcome::Aborted { landed }` with `landed.len()
  == 1`, `landed[0].op` the primary, `Applied { echo:
  Some(RemoteEcho::Task(view)) if view.id.card == RemoteCardId(55) }`
  — **the create's evidence rides the abort** (the re-POST hazard the
  `landed` field exists to prevent); `server.verify().await`.
- **Kill**: `is_transport → false` swallows the archive failure →
  `Outcomes` → the `Aborted` pattern fails.

**`post_create_archive_rejection_is_tolerated_without_aborting`**

- **Mount**: same create POST → 200; the archive PUT → **403**
  (`.expect(1)` — classified failures do not retry).
- **Exec**: identical, `archived = true`, `done: None`.
- **Assert**: `GroupOutcome::Outcomes(outcomes)` with exactly the
  create's `Applied { echo: Some(..) }` — a *classified* rejection on
  the post-create sub-call must not abort the cycle ("repairs on a
  later cycle" tolerance); no `Rejected` outcome appears (the sub-call
  has no op of its own).
- **Kill**: `is_transport → true` turns the 403 into an abort → the
  `Outcomes` pattern fails.
- **Scope note (D4)**: both tests pin the **archive** call site
  (`execute.rs:445`), which is what the findings index names; the
  done-stamp site (`execute.rs:461`) shares the same `is_transport`
  function body, so both audit mutants die, but the done arm's own
  condition-level mutants stay unpinned — recorded in §risks, not
  expanded (index scope).

## §tier-placement — `docs/testing_strategy.org`

- **a23–a28**: the hermetic wiremock actor tier (§8's mock-HTTP
  family; the crate's A-series). Deterministic, secretless, CI-default.
  This is the tier the findings index assigns every actor-level gap —
  the behaviors are pull/push *cycle* semantics (multi-endpoint
  request patterns, cache, absorber, backoff), invisible below the
  actor and needlessly expensive above it.
- **F6-24 pair**: the crate unit suite (`src/push_exec/tests.rs`) —
  executor decision-table semantics over one group, per the findings
  index ("two `push_exec` suite tests"); the actor-tier propagation of
  `landed` into the `Failed` report is already pinned by A4's shape.
- **Domain routing test**: co-located with the fake
  (`test_support/memory.rs::read_port_tests`), the existing
  routing-contract precedent. One behavior per test everywhere; no
  duplication of a unit fact through the actor thread (AGENTS §5).
- No e2e/`it_live`/`it_docker` additions (dispatch non-goal), no bench
  work, no snapshot (`insta`) changes.

## §decisions — summary

| # | Decision |
|---|---|
| D1 | Fail-reads flag = `AtomicBool` + `set_read_failure(bool)` (not a countdown) — paused-time stepping makes a plain bool fully deterministic (a26) |
| D2 | Flag fails both `load` and `read_state`; `apply` keeps succeeding |
| D3 | Failure variant `RepositoryError::Unavailable` |
| D4 | F6-24 pins the archive call site (index scope); done-stamp site shares the mutated fn, its own arms stay unpinned (risk-logged) |
| D5 | a24 seeds validators manually (A10 pattern): nothing persists report validators in the actor-only harness |
| D6 | Two-board mount hand-rolled inside a24 (shared `common/hermetic.rs` stays single-board; no shared-helper churn for one test) |
| D7 | a28 re-asserts the stamp arm already pinned by A21 (index's one-test shape names both properties; redundancy accepted) |
| D8 | "Five tests, one per finding" read as a floor; per-finding counts follow the findings index → 7 sync tests + 1 domain routing test |

## §execution — stage-2 commit series (each standalone, gates green)

1. `test(domain): add a fail-reads seam to the in-memory fake` — the
   flag + routing-contract test (§domain-flag). First because a26
   depends on it; the domain crate's own gates must be green at this
   commit alone.
2. `test(sync): assert pulled board labels reach the snapshot` — a23.
3. `test(sync): pin the board-switch cache clear` — a24.
4. `test(sync): pin the absorber switch, read backoff, tombstone title`
   — a25 + a26 + a27.
5. `test(sync): pin the archived-twin dedupe and flag adoption` — a28.
6. `test(sync): pin post-create failure discrimination` — the F6-24
   pair.

Stage-2 may regroup commits (e.g. one commit per finding) as long as
every commit stands alone and subjects stay `<type>(<scope>):`. TDD
intra-commit ordering is moot for a test-only chunk — each test *is*
the red-green unit; if any new test exposes a production bug, stop at
AGENTS §5 / dispatch §3: do not fix, do not weaken — record the repro
in the report for a new issue.

Full AGENTS §10 gate suite after the last commit (and per commit in
CI-equivalent spirit: `fmt`, `clippy`, `nextest`, doc-tests, `deny`,
the three-command coverage sequence).

## §non-goals

- No sync production changes of any kind (the F6-06b arm's behavior is
  pinned as-built; a bug found there becomes a new issue, not a fix).
- No e2e/it_live/it_docker/bench additions; no `CHANGELOG.md` edits
  (chunk 5 owns the round's entries); no ADR changes.
- No mutants re-run (the audit's survivor list is the input, not a
  gate); no shared-harness rework in `tests/common/`.
- F6-07/08/09 and every other chunk-6 finding: out of scope (owned
  elsewhere or action-none).

## §risks

- **Done-stamp site residual (D4)**: `execute.rs:461`'s own
  condition-level mutants would survive a future mutants run. Cheap to
  add later as a two-test mirror; deliberately not in this chunk.
- **Mutant drift**: the survivor list was computed against the
  pre-chunk-1 tree. The pinned arms are untouched by chunk 1, so
  kill-power should hold, but the authoritative check is a fresh
  mutants run — out of scope, noted for the audit-synthesis follow-up.
- **a26's silence window** depends on the read failure producing zero
  HTTP and zero broadcasts — that is the built behavior; if stage-2
  observes otherwise, it is a production-behaviour discovery → AGENTS
  §5 protocol.
- **a24/a25 verbosity**: two hand-rolled two-board responders (~90
  lines each); if stage-2 finds them diverging, a local (in-file)
  helper is fine — just not in `tests/common/`.
- **Flakiness budget**: every new test inherits the A-series
  wall-clock-bounded waits; the two virtual-time windows (a25's
  silence, a26's failing window) follow the existing A3/A14 stepping
  patterns — no new wait machinery.
- **Estimated diff**: ~700–950 added lines across four files —
  `memory.rs` ~40, `actor_wiremock.rs` ~550–700 (6 tests),
  `push_exec/tests.rs` ~130–160 (2 tests) — zero production lines
  outside `test_support`.
