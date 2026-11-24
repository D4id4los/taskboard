# Sync Orphan Lanes — Chunk C2 Plan (Round 6, 2026-11-24)

Chunk: C2 of backlog-reduction round 6. Branch: `fix/sync-orphan-lanes`.
Covers: **#1256** (orphan ops strand when zero boards are owned/bound),
**#1255** (the Files connector has no orphan lane), **#1291** (the
carried deleted-entries section transiently lists a live row).
Dispatch plan §11; scope report §C. No UX plan required (no
user-visible surface — sync internals only).

## 0. Stage-1 verdicts

### #1158 remainder — DROPPED (closed)

#1158 was closed 2026-11-23 (round 5), with zero comments: the issue
body itself records the only remainder ("split the derived pending-ops
counter from the per-bound-board server-contact row") as conditional —
*"only if it ever matters"* — and the aggregate sums rows, so the idle
phantom rows are harmless today. Per AGENTS §7, a completed close is
the archive and reopening needs new information; none exists. The
chunk's acceptance criterion for #1158 is therefore vacuous and is not
carried. The conditional follow-up stays recorded on the closed issue;
it is NOT duplicated into a new backlog issue (that would violate the
one-idea-one-issue rule and effectively reopen it).

### Code seams verified at stage 1

- Deck orphan lane: `crates/taskboard-sync-nextcloud/src/actor.rs`
  ~533–567 — `orphan_ops` = outbox ops with `owning_board(&tables)`
  `None`; extended into the *first* owned board's `board_ops`
  (`if index == 0`). The D-13 comment states the empty-set strand
  explicitly. The owned-empty guard at ~481 returns early (a `NoBoard`
  report only when commanded), so the orphan pass never runs.
- Files op application: `crates/taskboard-sync-files-nextcloud/src/
  board_cycle.rs` ~1010 — `drain_board` filters `owning_board(...) ==
  Some(board)` only; no orphan collection anywhere in the tier. The
  lane runner `run_board_lanes` (~155) collects `owned` and loops it;
  an empty `owned` set means no lane runs at all.
- Planner support (the key fact that makes the law connector-free):
  `plan_pushes` (`crates/taskboard-domain/src/push.rs` ~377) buckets
  ops whose owning entity row is wiped into groups whose
  `materialized` is a **no-op** (synthetic completion, no wire
  contact) — see push.rs ~59, ~158, ~604–623 and the tests at ~1835,
  ~1891. Both connectors consume this same planner.
- Carried deleted-entries section: the two pipeline rebuilds —
  `assemble_state` (`crates/taskboard-domain/src/pipeline.rs` ~1654)
  and the push-only rebuild (~2116) — both call
  `RestoreRoster::derive_tombstones` (which returns
  `deleted_entries: Vec::new()`, `roster.rs` ~171) and then
  `clone_from(&current.roster.deleted_entries)` unfiltered.
  `derive_parts` (`roster.rs` ~133) skips rows whose entry is live in
  the tracking map (`tracking.entries.contains_key(id)` → `continue`)
  — the carry-through clone bypasses exactly that check.

## 1. The orphan law (stated once)

> **Orphan law.** An outbox op whose owning entity row is wiped
> (`owning_board(&EntityTables)` is `None`) is cycle-level
> bookkeeping: the planner synthesizes a no-op completion for it (no
> server contact), so it must *drain every sync cycle*, unconditionally.
> Each connector drains its orphans on the first board pass it runs in
> a cycle; a connector that owns zero boards runs one standalone
> orphan drain per cycle instead. No connector may filter orphans by
> board, and no second statement of the law may appear — a
> connector-specific divergence from this paragraph requires a
> recorded reason in the plan and on the issue.

Both connectors already share the mechanics (`owning_board` predicate
+ `plan_pushes` synthetic no-ops); this chunk only closes the two
scheduling gaps (zero-owned boards, the Files tier's missing ride)
under that one statement.

## 2. Decisions

### D1 — #1256: the zero-board standalone drain (Deck)

When `owned.is_empty()` in the Deck actor's cycle (`actor.rs` ~481),
instead of returning bare, run a **standalone orphan drain**: collect
`orphan_ops` exactly as the in-cycle pass does, `plan_pushes` them,
execute the resulting groups (all no-op/synthetic per §0 — but the
code must not *assume* no-op: it executes whatever the planner
produces, so a future planner change cannot silently strand), complete
the ops, and publish the op results through the connector's existing
report path. Ordered decisions:

- The drain runs even when not `commanded` (orphans are not a user-
  visible failure; no `SyncReport::Failed` is sent for the empty
  owned set when the drain runs — the early-return keeps its
  `NoBoard` report *only* in the not-draining shape? **Decision:** the
  `NoBoard` report is sent exactly as today when commanded; the drain
  is additive and silent on the report channel unless it has results
  to publish. Keeps the report contract unchanged.)
- If `orphan_ops` is empty, the drain is a no-op (no report, no
  publish) — zero-cost guard first.
- The D-13 comment at the in-cycle site is **replaced** by a pointer
  to §1 of this plan (the law lives once); the standalone drain cites
  it too.

### D2 — #1255: the Files connector's lane

Two edits, both consuming the shared `owning_board` predicate (no
second predicate site):

- **The ride** (in-cycle): `drain_board`'s filter stays per-board; the
  orphan collection happens at the `run_board_lanes` level — the
  *first* board lane that runs in a tick carries the orphan ops into
  its drain (append to its op set before the `board_ops.is_empty()`
  early return at ~1013, so a first board with no own ops still
  drains the orphans). This mirrors Deck's `if index == 0` exactly:
  same law, same shape, no divergence to record.
- **The zero-board drain**: when `owned` is empty, `run_board_lanes`
  runs the same standalone orphan drain as D1, via a files-tier
  `drain_orphans_only` path. **Recorded divergence (the one allowed by
  the law):** the files tier cannot reuse `drain_board` for this — it
  takes a bound `board`/`remote` uuid pair and publishes a per-board
  chunk document; an orphan-only drain has neither. Instead it plans
  the orphan ops, executes the groups through the files executor with
  the neutral (empty) overlay, and accounts the op results through
  the same per-tick accounting the lanes use. This is a *mechanism*
  divergence under an identical semantic; it is recorded here and in
  the code comment at the site.

### D3 — #1291: the transient live-row pin

At both carry-through sites, the clone is replaced by a **live-row
filter**: the carried `deleted_entries` keeps a row only if its
`entry_id` is absent from the tracking map in scope at that site —
the folded tracking map when the cycle folded one
(`folded.tracking`), else `current.tracking`. This derives exactly
the check `derive_parts` performs (`roster.rs` ~133) without giving
the pipeline journal access; the "journal is out of the pipeline's
reach" comment stays true and gains "the live-row check is re-derived
from the tracking map in scope". The candidate fix named on the issue
is adopted verbatim. No journal parameter is threaded into the
pipeline (rejected: a new pipeline input for a display-only,
self-healing transient; the tracking-map filter fully drains the row
one cycle earlier, which is the issue's ask).

### D4 — test strategy

TDD; all assertions typed/structural (AGENTS §5: no text asserts, no
behavior pins of transients). Fakes over mocks; no sleeps.

## 3. Test catalogue

**#1256 (Deck actor, `crates/taskboard-sync-nextcloud`):**
1. `orphan_ops_drain_with_zero_owned_boards` — connector state with an
   outbox op whose owning task row is wiped, no owned boards: after
   one cycle, the op's `OpId` is no longer in the persisted outbox
   (typed: outbox membership).
2. `zero_board_cycle_with_empty_orphan_lane_publishes_nothing` — same
   shape, empty orphan set: no report on the channel, outbox
   unchanged (structural).
3. `owned_boards_cycle_still_rides_orphans_on_first_board` — the
   existing ride preserved: two owned boards, one orphan op →
   completed exactly once (not once per board) — typed: the op
   appears in exactly one board's completion set / outbox drained.

**#1255 (files tier, `crates/taskboard-sync-files-nextcloud`):**
4. `files_lane_drains_orphan_ops_on_first_board_pass` — a bound board
   plus an orphan op (owning row wiped): after `run_board_lanes`, the
   op is drained (outbox membership).
5. `files_lanes_drain_orphans_when_zero_boards_bound` — empty owned
   set, orphan op present: drained.
6. `files_first_board_with_no_own_ops_still_drains_orphans` — the
   early-return trap: first board's own op set empty, orphan set
   non-empty → drained (this is why the append precedes the guard).
7. `files_orphan_drain_contacts_no_server_for_noop_groups` — the
   transport fake records calls; the synthetic-completion orphan drain
   performs no addressing of remote entities beyond the lane contract
   (structural: the fake's call log has no entity-addressing calls for
   the no-op groups).

**#1291 (domain pipeline, `crates/taskboard-domain`):**
8. `carried_deleted_entries_drop_rows_whose_entry_is_live` — rebuild
   with `current.roster.deleted_entries` containing a row whose
   `entry_id` is present in the folded tracking map → the rebuilt
   state's roster excludes it (typed: roster membership).
9. `carried_deleted_entries_keep_rows_whose_entry_is_still_gone` — the
   complement: rows for still-dead entries survive the carry (guards
   against an over-broad filter).
10. Both rebuild sites (the cycle's `assemble_state` and the push-only
    rebuild) get the pin — one behavior, one test per site is allowed
    here because the sites are distinct builders, not layers over one
    behavior; the shared helper (if one is extracted) gets no separate
    test of its own (AGENTS §5, don't test test-helpers/inline
    reimplementations).

## 4. Risks & traps

- **Orphan-law duplication trap** (dispatch plan §11): the files ride
  and the Deck ride must read as one law with two call sites; the
  zero-board mechanism divergence (D2) is the only recorded
  divergence.
- **The `drain_board` early-return trap** (test 6): appending orphans
  after the empty check silently reproduces #1255 inside the fix.
- **No-op assumption trap**: neither drain may branch on
  `materialized.is_noop()` — execute what the planner produces, so a
  planner change cannot strand ops behind a now-wrong assumption
  (AGENTS §5: no quality ceilings / no pins of today's behavior).
- **Report-contract risk (D1)**: the standalone drain must not emit a
  success report that advances a board's `last_success` — no board is
  involved; the op accounting rides the existing op-results publish,
  not the per-board sync row.
- **#1291 over-filter risk** (test 9): filtering against
  `current.tracking` when the cycle folded no tracking section is
  correct only because a live entry is live in both views; the test
  pins that complement.

## 5. Estimated diff size

~250–350 lines including tests:
- `taskboard-sync-nextcloud/src/actor.rs`: ~50–70 (standalone drain +
  comment rewrite), tests ~80.
- `taskboard-sync-files-nextcloud/src/board_cycle.rs`: ~40–60 (ride +
  orphan-only drain path), tests ~90.
- `taskboard-domain/src/pipeline.rs`: ~15–25 (the filter at two
  sites, possibly one small helper), tests ~50.
Confined to the sync crates + domain pipeline per the dispatch plan's
acceptance line.

## 6. Gates & handoff

Pre-commit hooks are the gate; every cargo invocation routes through
`scripts/memory-gate.sh`; the full suite is the dispatcher's (subagent
law). Handoff at: branch committed, PR open, `gh pr checks` green.
Close #1256/#1255/#1291 at merge per the dispatch protocol.
