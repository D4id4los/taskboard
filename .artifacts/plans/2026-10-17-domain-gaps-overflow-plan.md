# [Plan] Domain Conflict-Core Test Gaps + Overflow Hardening — Catalogue & Procedures

- **Date**: 2026-10-17
- **Kind**: stage-1 plan for chunk 3 of the remediation-wave-1 dispatch
  (dispatch §1/§4, prompt 3.1). Planning only — no implementation in
  this commit.
- **Origin**: issues #239 (F3-10/F3-11/F3-12 + F3-08's boundary tests;
  findings report §findings rows F3-08/F3-10/F3-11/F3-12, §agenda row
  B-1) and #303 (standing decision (e), §agenda-outcomes row (e):
  release `overflow-checks = true` + domain arithmetic proptests; the
  full slice/arithmetic audit chunk is accepted-risk-with-trigger and
  **not** this chunk). Both issues normative.
- **Branch / worktree**: `test/domain-gaps-overflow` in
  `../taskboard-WT-test-domain-gaps-overflow`, cut off `main` @
  `7449c1a` (= `origin/main`; chunk 1's EngineEnvelope rename already
  landed — domain is untouched by it, verified:
  `git diff v0.1.0 main -- crates/taskboard-domain/src/{pipeline,merge,command,reminders}.rs`
  is empty, so the four audited files are byte-identical to the tag
  and every audit `file:line` anchor below resolves on this branch).
- **Evidence base**: `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-domain-report.md`
  (§6.2 F3-08, §7.3 mutant triage, §7.4 coverage drill-down, findings
  index §9) — the audit's line anchors are against `v0.1.0` and are
  re-verified against this branch in §D3 below.

---

## §context — the seams, as they stand on this branch

- The **self-clobber guard quartet** (`pipeline.rs:104-327`):
  `pending_task_intents` (:104, per-field bool table keyed by task id),
  `pending_stack_intent` (:209, `(rename, delete)`), `pending_board_intent`
  (:258, rename-only), `pending_label_intent` (:281, `(update, delete)`),
  each with a `keep_*` restore sibling. The `queued_at`/`read_at`
  boundary comparison is written independently 4× (:110, :217, :261,
  :289 — F3-08).
- `apply_sync_report` (`pipeline.rs:345`) with the eight guard
  conjunctions the audit names (§D4 below); `apply_push_outcomes`
  (`:883`, the R9 decision table) containing the **board push-echo
  adoption arm** (`:994-1008`, requires `LocalOp::RenameBoard` +
  `RemoteEcho::Board`) and the **`RemoteMissing` board arm**
  (`:1084-1089`, sole caller of `merge.rs`'s `tombstone_board`,
  `:754-762`) — both line-uncovered at every tier (F3-10).
- `plan_command` (`command.rs:240`): the `UpdateTask.archived` arm
  (`:299-302`, F3-11), the MoveTask no-op conjunction (`:317`), and the
  stack-delete cascade filter conjunction (`:438`) — the audit's two
  production `&&` sites in this function (F3-12).
- The **reminder/scheduling arithmetic** (#303): `occurrence_of`
  (`reminders.rs:315`, `checked_add_signed` → `None` saturation),
  `effective_occurrence` (`:326`, cycle-bounded snooze-chain walk),
  `due_firings` (`:376`), `due_soon` (`:431`, horizon saturation to
  `MAX_UTC`), `ReminderOffset::before` (`:39`, the ≤ 0 sign domain).
- **Merge clocks**: `remote_wins` (`merge.rs:71`, strict `>`),
  `max_ts` (`:75`), the `clocks_from`/`adopt_*_after_push`/`tombstone_*`
  stamping family. Existing proptest coverage (`merge.rs:1225-1281`:
  monotonicity, totality, strict-newer-wins, idempotence) samples only
  `0_i64..10_000` epoch-anchored seconds — no chrono-representable
  extremes, no pre-epoch stamps.
- **Existing tests that bound this chunk** (must not break or weaken):
  the crate's 16 proptest blocks (§7.2 of the audit: ×3 persistence,
  ×2 pipeline, ×2 reminders, ×1 each merge/command/push/remote/state/
  messages/entities/ids + the feature-gated test_support block) and the
  inline example suites in `pipeline.rs` (:1163+, two mods),
  `command.rs` (:760+), `merge.rs` (:806+), `reminders.rs` (:459+).

---

## §decisions

### D1 — Layer: every new example test lives at the domain layer

The findings phrase F3-10's remedy as "engine-level tests", but the
audit's own evidence is that the arms are untested **at every tier**,
and the arms live in domain functions that are directly callable:
`apply_push_report` (`pipeline.rs:788`, pub) for the two board arms
(push-only cycles need no snapshot), `apply_sync_report` (pub) for the
intent quartet and the conjunction differentials, `plan_command` (pub)
for the command arms. Per AGENTS §5 ("one behavior, one test, lowest
layer"), the domain layer *is* the lowest layer for these behaviors;
the state-engine crate would only re-test routing. The existing suites
precedent this exactly (the stack/label twins of both board arms are
pipeline.rs tests: `create_stack_push_echo_binds_remote` :2320,
`remote_missing_stack_and_label_ops_finalize_tombstones` :2448).
No `taskboard-state` test is added by this chunk.

### D2 — Scope of F3-08: the four boundary tests only; the `post_read_ops` iterator is OUT

Issue #239's proposed approach bundles the `post_read_ops` dedupe
iterator with the boundary tests. The dispatch plan §4 (chunk 3,
prompt 3.1/3.2/3.3) is more specific and normative for this chunk: its
covers-line names "**F3-08's boundary tests**", the implementation
prompt lists only "targeted unit tests… proptest suites… the
root-Cargo.toml overflow-checks edit", and the review prompt mandates
"no production logic changed beyond nothing". A production refactor of
four functions' internals on a `test/` branch would fail the chunk's
own stage-3 gate. **Decision**: land the four exact-boundary tests
(D5-D8 below) — they make boundary drift *detectable*, which is the
safety-relevant half of F3-08; the dedupe iterator is deferred and
stage 3 records it explicitly in the PR body when closing #239 (and,
per AGENTS §7 wave-wide bookkeeping, files it as a small follow-up
issue if the user wants it tracked). F3-09's "keep the twins explicit"
note is honored by not touching the functions at all.

Same carve-out for F3-12's "`max_ts` marked equivalent in the sweep
config": that is mutation-tooling config, owned by the #255 sweep-scope
decision (chunk 11). Out of this chunk; noted here so the #239 PR body
can say so.

### D3 — Audit-anchor reconciliation (one wording slip in the audit)

The audit's §7.4 lists "`pending_stack_intent`'s rename arm (`:222`)"
as line-uncovered, but `pipeline.rs:222` on this branch (byte-identical
to the tag) is the **delete** arm (`LocalOp::DeleteStack(s) if s == stack`),
and the rename arm (:221) *is* executed by
`a_post_read_stack_rename_survives_the_same_cycle_pull` (:3313, tag-present).
The line anchor wins over the word "rename" (the audit's own §7.1 cites
the :3163+ guard examples as existing): the uncovered stack arm is the
**delete** arm. Symmetrically for labels, `pending_label_intent`'s
uncovered arm (:294) is the delete arm — consistent with the audit's
wording there. The catalogue (D3/D4 below) targets the delete arms and
the genuinely uncovered task arms (`MoveTask` :138-140,
`AssignLabel|UnassignLabel` :144-146). Stage 2 confirms via the
uncovered-line claim only if a test fails unexpectedly — no new
coverage run is planned (coverage is negative signal, not a target).

### D4 — Which of the eight `apply_sync_report` conjunctions get new tests

The audit names eight sites (`pipeline.rs:451, 528, 544-545, 596, 618,
632, 647-649, 657`). Mapping each to existing killing tests on this
branch, only three need new differentials:

| Site | Conjunction | Status |
|---|---|---|
| `:451` | `merged_stack.deleted && !local.deleted` (newly-tombstoned detection → cascade list) | **new test C3** — the `(false, false)` arm (ordinary changed-but-live stack merge must NOT enter the cascade list). No existing test asserts task-`deleted`/op-cancellation on a live-stack merge; tombstoning preserves field values, so field asserts alone can't kill the `\|\|` mutant — the test must assert `task.deleted == false` and no `FailOp` actions. |
| `:528` | `was_deleted && !merged.deleted` (R5 resurrect detection → delete-op drop) | **new test C4** — the tombstone-stands arm: local tombstone newer than the remote write ⇒ merged stays deleted ⇒ the pre-read `DeleteTask` op stays **queued** (no `FailOp`). The resurrect arms are covered (`resurrect_via_pipeline_drops_pending_delete_op` :2735; `a_post_read_delete_is_not_resurrected_away` :3239 — which also kills the `:544-545` `\|\|` mutant). |
| `:544-545` | `op == DeleteTask(id) && queued_at <= read_at` | covered: :2735 (pre-read dropped) + :3239 (post-read stays queued — kills `\|\|`). No new test. |
| `:596` | `!op_results.contains_key(..) && op_targets_task(..)` (R3 absence cancellation) | **new test C5** — the `contains_key` arm: an op already completed by this cycle's push echo must stay `CompleteOp` when the same task is then absence-tombstoned (first verdict wins; mutant re-labels it `FailOp`). The targeting arm is covered (`resurrect_and_remote_missing_leave_unrelated_ops_queued` :1622's R3 half) and the tombstone+cancel behavior by `remote_absence_tombstones_and_cancels_pending_ops` :1175. |
| `:618` | `!present && !local.deleted` (stack absence) | covered: `stack_absent_from_snapshot_is_finalized` :2605 (present arm) + `already_finalized_entities_are_not_refinalized_on_absence` :1829 (deleted arm). |
| `:632` | same for labels | covered: :2634 + :1829. |
| `:647-649` | `is_live() && remote.is_some_and(board && stack)` (cascade filter) | covered: `stack_cascade_spares_moved_out_and_offline_tasks` :1714. |
| `:657` | cascade op-cancellation | covered: `stack_delete_cascade_cancels_cascaded_tasks_ops` :2705. |

The two `plan_command` conjunctions (F3-12): `:317`'s stack-conjunct
arm is already killed by `move_task_stamps_the_composite_position_clock_once`
(:1176 — same stack, changed order), so **C1** covers the missing
order-conjunct arm (different stack, *unchanged* order ⇒ still a plan);
`:438`'s cascade filter has no two-stack fixture anywhere — **C2**.

### D5 — Proptest design rules (adversarial ranges, house conventions)

- **Additive, never widening**: the existing bounded strategies and
  model oracles stay untouched — their models use plain `+`
  (`reminders.rs:566`, `:640`), which panics on overflow; that is
  exactly why they are bounded (1971–2100) and why the *new* adversarial
  props use `checked_add_signed` oracles instead. New strategies are
  added beside them, not under them.
- **New shared strategies** live in `test_support/mod.rs` beside
  `timestamp_strategy` (both consuming test mods already import
  `test_support`; downstream crates may reuse them later):
  - `adversarial_timestamp_strategy()` — `proptest::oneof` of:
    the existing bounded band; seconds within ±(0..=86_400) of
    chrono's own `DateTime::<Utc>::MIN_UTC`/`MAX_UTC` second bounds
    (derived from the constants, never hard-coded, so the strategy is
    in-range by construction); and the DST-boundary instants (below).
  - `dst_boundary_instants()` — a `Vec<DateTime<Utc>>` of hard-coded
    UTC instants of 2026 DST transitions plus ±1 s neighbors: EU
    spring 2026-03-29T01:00:00Z, EU fall 2026-10-25T01:00:00Z, US
    spring 2026-03-08T07:00:00Z, US fall 2026-11-01T06:00:00Z, AU
    exit 2026-04-05T16:00:00Z, AU entry 2026-10-04T16:00:00Z.
    **No timezone dependency is added** (no chrono-tz, no `Local` —
    CI-environment independent): the crate is UTC-only by design
    (audit §8.4), and the battery's meaning is that these instants are
    *ordinary instants* to the arithmetic — the zone-freeness is the
    pinned contract.
- **Oracles**: library-oracle form (`due.checked_add_signed(off)`) is
  used where the production contract *is* chrono's checked arithmetic
  (`occurrence_of`); that pins "no clamping + None ⇔ chrono overflow"
  without reimplementing production logic. Structural properties
  (sortedness, membership, monotonicity, termination, determinism)
  everywhere else — no model reimplementation for the new props
  except the bounded chain-walk reachability loop in E2 (≤ K hops,
  the house property-oracle precedent).
- **Prop count**: every new property runs the house standard **256
  cases** (`test_support::proptest_config(256)` — Miri-safe,
  `PROPTEST_CASES`-overridable). Recorded here as the plan-required
  count; 8 properties × 256 ≈ 2 048 generated cases total, negligible
  suite cost (pure timestamp arithmetic).
- Any prop failure produces a regression seed file under
  `crates/taskboard-domain/proptest-regressions/` — committed
  alongside the failing-then-passing resolution (house convention;
  3 files already exist).

### D6 — The release `overflow-checks` edit and its smoke

Exact edit, root `Cargo.toml` `[profile.release]` (currently
`Cargo.toml:116-120`; the existing comment block stays):

```toml
[profile.release]
# The kiosk targets low-power hardware: optimize for size + runtime, not build time.
lto = "thin"
codegen-units = 1
strip = "symbols"
# v0.1.0 audit standing decision (e) / issue #303: arithmetic overflow in
# the shipped binary must be a loud, backtraced panic, never a silent
# wrap. Runtime cost is one branch per add on timestamp/counter arithmetic
# — negligible against the map/serde hot paths; kiosk-scale data.
overflow-checks = true
```

Facts recorded for the caveat: dev/test profiles already overflow-check
by default (`debug_assertions` implies it), so the gate suite's
behavior is unchanged — this flag covers only release builds (CI
release lanes via `release.yml`, shipped binaries). Compile-time impact
is nil (no extra codegen passes); the runtime cost is bounded by the
audit's own arithmetic census (domain arithmetic is timestamp math and
counters, not numeric loops).

**Release-smoke procedure** (dispatch §4 acceptance; run by stage 2 in
the worktree, evidence quoted in its report):

```sh
cargo build --release                                  # flag compiles clean workspace-wide
cargo nextest run -p taskboard-domain --release        # targeted release-mode run:
                                                       # the crate whose arithmetic the
                                                       # props pin, executing under the flag
```

The targeted `-p taskboard-domain` release run is the "targeted
release-mode test run" the dispatch names (a full `--workspace
--release` run is not required by the acceptance and would burn CI
minutes without adding evidence — the flag is a codegen setting, not
per-crate logic). Both commands must exit 0.

### D7 — Failing-test protocol (binding on stage 2)

Every catalogue entry below asserts **desired semantics** derived from
the rules' documented contracts (module docs, ADR 0004 vocabulary, the
existing sibling tests). If any new test fails against untouched
production code, that is a suspected production bug (AGENTS §5):
stop, report the failure verbatim, do not weaken the test, do not fix
production in this chunk. Special call-outs where the fixture encodes
a reading that could be wrong: C3 (a changed-but-live stack merge must
not cascade), C4 (tombstone-stands keeps the delete op queued), C5
(first push-echo verdict survives later absence cancellation) — if
production disagrees, the finding report names the rule question for
the user, exactly as the dispatch's wave-wide test rules prescribe.

---

## §catalogue — example tests (one behavior each, typed assertions)

All pipeline tests join the existing guard/push test mods in
`pipeline.rs` (mirroring the fixtures `bound_task`/`local_stack`/
`base_state`/`snapshot`/`single_stack_snapshot` from
`merge_testutil.rs`); command tests join `command.rs`'s test mod
(fixtures `base()`, `task()`, `stack()`, `run()`, `op_for()`).

### F3-10 — the board arms (pipeline.rs)

| # | Test name (working) | Fixture | Typed assertions | Pins |
|---|---|---|---|---|
| A1 | `remote_missing_board_push_tombstones_the_bound_board` | `base_state(bound_task(1))`; outbox = one `RenameBoard` op (queued pre-read); pushes = `PushOutcome { op, PushResult::RemoteMissing }`; via `apply_push_report` | board `deleted == true`; `remote == None` **and** `remote_seen == None` (binding cleared, `tombstone_board` :754-762 contract); action list contains `CompleteOp(op_id)`; the stack and task are untouched (`deleted == false` both, no `FailOp`s — R9c finalizes the entity itself; cascading belongs to the sync path, which has its own tests) | `pipeline.rs:1084-1089` + `merge.rs:754-762` |
| A2 | `board_rename_push_echo_is_adopted` | same state; pushes = `Applied { echo: Some(RemoteEcho::Board(remote_board(77, BASE + 300))) }` with echo title ≠ local; via `apply_push_report` | board title == echo title (unconditional adoption); `clocks.title == max(local.clocks.title, echo.last_modified)`; `remote == Some(board 77)`, `remote_seen == Some(echo lm)`; `CompleteOp(op_id)` present | `pipeline.rs:994-1008` + `adopt_board_after_push` (`merge.rs:702-724`) |

### F3-11 — the archived arm (command.rs)

| # | Test name | Fixture | Typed assertions | Pins |
|---|---|---|---|---|
| B1 | `update_task_archived_edit_flips_the_flag_and_stamps_the_clock` | `base()`; `StateCommand::UpdateTask { id: task_id(), changes: TaskChanges { archived: Some(true), ..Default::default() } }` | plan outcome `Applied`; `UpsertTask`: `archived == true`, `clocks.archived == now()`, untouched clocks (`title`/`duedate`/`position`) still `ts(T0 - 3_600)`; second action enqueues `LocalOp::UpdateTask(task_id)` — mirrors `update_task_stamps_only_touched_field_clocks` (:1000) | `command.rs:299-302` |

### F3-12 — conjunction differentials

| # | Test name | Layer | Fixture | Typed assertions | Kills |
|---|---|---|---|---|---|
| C1 | `move_to_a_different_stack_with_unchanged_order_is_not_a_no_op` | `plan_command` | `base()` + a second **live** stack; task's current position is `(stack_id(), 1)`; `MoveTask { id, stack: other, order: 1 }` | plan ≠ no-op: `UpsertTask` with `stack == other` and `clocks.position == now()`; `EnqueueOp(LocalOp::MoveTask(id))` | `:317` `&&`→`\|\|` (order conjunct; the stack conjunct is killed by :1176) |
| C2 | `stack_delete_cascade_spares_live_tasks_of_other_stacks` | `plan_command` | `base()` + second live stack with a live bound task; `DeleteStack { id: stack_id() }` | no `UpsertTask` and no `EnqueueOp(DeleteTask(..))` naming the other stack's task; that task is absent from the plan's actions entirely; the in-stack task still cascades (one `DeleteTask` enqueue) | `:438` `&&`→`\|\|` |
| C3 | `a_live_stack_merge_does_not_re_cascade_its_tasks` | `apply_sync_report` | `base_state(bound_task(1))` (stack + task live, bound); snapshot stack changed but **live** (`remote_stack(1, BASE + 300)` with a new title; `deleted_at: None`); task card present unchanged-or-changed | merged stack `deleted == false`; **task `deleted == false`**; zero `FailOp` actions in the batch (no op cancellation) | `:451` `&&`→`\|\|` (the `(false,false)` arm — field-value asserts cannot catch a tombstone, so the deleted-flag + FailOp asserts are load-bearing) |
| C4 | `a_standing_tombstone_keeps_its_pending_delete_queued` | `apply_sync_report` | local task `deleted = true`, `clocks.deleted = ts(BASE+300)`, `remote_seen = Some(ts(BASE))`; pre-read `DeleteTask` op (queued_at ≤ read_at); snapshot card present with `last_modified = BASE + 180` (newer than `remote_seen` ⇒ merge runs; older than the tombstone clock ⇒ tombstone stands) | task stays `deleted == true` with binding kept (R9c pre-push semantics); **no `FailOp`/`CompleteOp` for the delete op** (stays queued to re-push); twin of `a_pre_read_delete_still_yields_to_a_newer_remote_write` (:3277) | `:528` `&&`→`\|\|` |
| C5 | `a_push_settled_op_verdict_survives_absence_cancellation` | `apply_sync_report` | bound task + outbox [`UpdateTask` op]; pushes = `Applied { echo: Task(..) }` for that op; snapshot contains the stack but **not** the card (absence) | task tombstoned (`deleted == true`, binding cleared); the pushed op's action is `CompleteOp` (not `FailOp` — first verdict wins); any *second* still-pending op of the task is `FailOp` | `:596` `contains_key` conjunct |

### F3-08 + F3-12 — intent-quartet arms, boundaries, scoping (pipeline.rs, guard test mod)

Boundary convention: `queued_at == read_at` is a **pre-read** op
(`op.queued_at <= read_at` skips protection) — remote truth wins.

| # | Test name | Fixture (all via `apply_sync_report`) | Typed assertions | Pins / kills |
|---|---|---|---|---|
| D1 | `a_post_read_task_move_survives_the_same_cycle_pull` | local task moved: `order = 9`, `clocks.position = ts(BASE+120)`; `MoveTask` op queued at `BASE+120` (read_at `BASE`); snapshot card with old `order`, `last_modified = BASE+300` | `stack`/`order` keep local values; `clocks.position == ts(BASE+120)`; `remote_seen` advances to `BASE+300` | `pending_task_intents` `MoveTask` arm (:138-140) + `keep_task_intent` position restore (:192-196) — both line-uncovered |
| D2 | `a_post_read_label_assignment_survives_the_same_cycle_pull` | local task `labels = {L}` with `clocks.labels = ts(BASE+120)`; `AssignLabel` op queued post-read; snapshot card with empty remote label set, `last_modified = BASE+300` | `labels == {L}`; `clocks.labels == ts(BASE+120)`; `remote_seen` advanced | `AssignLabel\|UnassignLabel` arm (:144-146) + labels restore (:197-200) |
| D3 | `a_post_read_stack_delete_survives_the_same_cycle_pull` | local stack `deleted = true` (local delete stamp `BASE+120`); `DeleteStack` op queued post-read; remote stack live and changed (`deleted_at: None`, `last_modified = BASE+300` — would resurrect) | stack stays `deleted == true`; `clocks.deleted == ts(BASE+120)` | delete arm (:222) + `keep_stack_intent` delete restore (:251-254) |
| D4 | `a_post_read_label_delete_survives_the_same_cycle_pull` | local label `deleted = true` (`BASE+120`); `DeleteLabel` op post-read; remote label live+changed | label stays deleted; clock kept | delete arm (:294-295) + restore (:323-326) |
| D5 | `an_op_queued_exactly_at_read_at_is_not_protected_task` | `UpdateTask` op with `queued_at == read_at == ts(BASE)`; snapshot card newer with `done: None` vs local `done: Some` | merged `done == None` (remote truth; twin of :3209 with equality instead of `BASE-60`) | task-boundary comparison (:110 `<=`→`<`) |
| D6 | …`_stack` | `RenameStack` op at `queued_at == read_at`; remote stack renamed newer | merged title == remote title | stack boundary (:217) |
| D7 | …`_label` | `UpdateLabel` op at boundary; remote label renamed newer | merged title == remote title | label boundary (:289) |
| D8 | …`_board` | `RenameBoard` op at boundary; snapshot board renamed newer | merged board title == remote title | board boundary (:261) — kills **both** named board mutants: `>`→`>=` (boundary op would wrongly protect) and `&&`→`\|\|` (the `matches!` disjunct alone would protect) |
| D9 | `a_post_read_stack_op_protects_only_its_own_stack` | two local stacks bound to remote stacks 1 and 2; `RenameStack(X)` queued post-read; snapshot has **both** remote stacks changed | X keeps local title; Y adopts remote title | `pending_stack_intent` `==`→`!=` (+ guard mutants) |
| D10 | `a_post_read_label_op_protects_only_its_own_label` | two local labels; `UpdateLabel(X)` post-read; both remote labels changed | X keeps local; Y adopts remote | `pending_label_intent` `==`→`!=` |

Task-arm mutant kills come free: D1/D2 assert protection *present*, so
deleting the `MoveTask`/`AssignLabel` match arms (the named survivors)
fails them. Board id-scoping needs no extra test: D8's boundary fixture
kills the `&&`→`||` in the general form (any-post-read-op protection),
and a cross-board rename cannot protect the merged board without also
passing D8's own-board boundary case.

**Catalogue total: 17 example tests** (2 + 1 + 5 + 10).

---

## §proptests — #303 arithmetic properties (all 256 cases)

| # | Property (working name, file) | Strategy (adversarial ranges) | Assertions |
|---|---|---|---|
| E1 | `occurrence_of_is_total_and_saturating` (reminders.rs) | `(offset, due)`: offset ∈ oneof[`at_due`, −1..=−2_592_000 s (realistic band), extreme negatives down to chrono's representable-combination edge — sampled as seconds in `(TimeDelta::MIN-s.Valid … −2_592_001)` clipped to `try_seconds`' valid domain]; due ∈ `adversarial_timestamp_strategy()` | totality (no panic for any pair); library oracle `due.checked_add_signed(offset.as_duration())` equality (no clamping; `None` ⇔ chrono overflow); monotonicity in `due` (d1 ≤ d2 ⇒ occ(d1) ≤ occ(d2), `Option` order — `None` sorts below `Some`, which is the saturation contract) |
| E2 | `effective_occurrence_terminates_on_arbitrary_chains` (reminders.rs) | one rule + fixed due; snooze map over K ≤ 6 candidate instants (drawn from a small set **with replacement**, so self-links and cycles generate naturally) with arbitrary targets inside the set | returns (no hang — the cycle bound is under test); result is `Some`; reachability: a bounded ≤ K-hop manual walk over `snoozes` confirms the result; the walked path never repeats a node (the `visited` contract) |
| E3 | `due_soon_saturates_the_horizon_never_panics` (reminders.rs) | `now` ∈ adversarial; horizon ∈ oneof[1 s..=30 d, `TimeDelta::try_seconds(i64::MAX/1000)` (max representable), its negation, zero]; small task map incl. a live task with `due = MAX_UTC − 1 s` | totality; every returned task satisfies `due > now && due <= now.checked_add_signed(horizon).unwrap_or(MAX_UTC)` (library oracle); with the max horizon the `MAX_UTC − 1 s` task **is** returned (the documented "everything ahead" degradation) |
| E4 | `merge_clocks_are_total_monotone_and_idempotent_at_extremes` (merge.rs, joins the :1225 block) | local/remote clock seconds ∈ `adversarial_timestamp_strategy()` (near-MIN/near-MAX/DST/bounded); card ∈ 1..100 | all three existing properties re-asserted at the extreme range: monotonicity (all 8 clocks ≥ local), strict-newer-wins tie-break (`remote_ts == clock` keeps local), idempotence (`merge_task(merged, remote) == merged`), totality |
| E5 | `due_firings_total_deterministic_ordered_on_adversarial_instants` (reminders.rs) | small task map (≤ 4 tasks) with adversarial duedates; `now` adversarial; a rule per shape incl. offsets that saturate to `None` on the extreme dues; empty deliveries | totality; determinism (two runs equal); output sorted by `(occurrence, reminder)`; every firing: `occurrence <= now`, undelivered (`is_delivered` false), task remindable (`is_live && is_open && duedate.is_some`) and unmuted |
| E6 | `reminder_offset_before_rejects_exactly_positive_durations` (reminders.rs) | seconds ∈ `-(i64::MAX/1000) ..= (i64::MAX/1000)` (the `try_seconds` domain, u64/i64 edges by construction) | `before(dur)` is `Ok` iff `dur <= TimeDelta::zero()`; every `Ok` value has `as_duration() <= 0`; `Err` is exactly `InvalidReminderOffset::AfterDue` (typed `matches!`, no text) |
| E7 | `dst_boundary_instants_are_ordinary_instants` (reminders.rs, example-style battery) | the fixed `dst_boundary_instants()` vector + ±1 s neighbors | known-answer: `occurrence_of(due = i, offset = −1 h) == i − 1 h` for every battery instant (instant-exactness — the arithmetic is zone-free); `due_firings` on a task due at each instant fires at the offset instant, not at any wall-clock reading of it |

**7 properties × 256 cases = 1 792 generated cases** (+ E7's ~18
fixed battery instants). All are pure timestamp/set arithmetic —
Miri-safe, no environment dependence, no sleeps (AGENTS §5 by
construction: the crate has no clock reads).

---

## §release — the flag edit and smoke

Per §D6: one standalone commit editing root `Cargo.toml`
`[profile.release]` exactly as quoted, followed in the same stage-2
session by `cargo build --release` + `cargo nextest run -p
taskboard-domain --release`, both quoted in the stage-2 report.
Nothing else in the workspace's profiles changes; `[profile.dev]` /
`[profile.test]` are untouched (they already overflow-check).

## §commit-plan — stage-2 series (every commit standalone, rebase-merge hygiene)

1. `test(domain): pin board RemoteMissing and push-echo adoption arms`
   (A1, A2 — F3-10).
2. `test(domain): pin archived edit and command guard conjunctions`
   (B1, C1, C2 — F3-11 + `plan_command` half of F3-12).
3. `test(domain): pin apply_sync_report guard conjunction differentials`
   (C3, C4, C5 — the pipeline half of F3-12).
4. `test(domain): cover intent-quartet arms, boundaries and scoping`
   (D1-D10 — F3-08 boundaries + quartet arms).
5. `test(domain): adversarial proptests for domain arithmetic`
   (E1-E7 + the two `test_support` strategies — #303 part 2).
6. `chore(workspace): enable overflow-checks in the release profile`
   (#303 part 1 + the §D6 smoke evidence).

Stage 3 then ships: PR body `Closes #239` + `Closes #303`, plus the
two deferral notes from §D2 (the `post_read_ops` iterator half of
F3-08; the `max_ts` equivalent-mutant marking → #255) so the closures
are honest about what consciously did not land here.

## §non-goals

- **No slice/arithmetic site audit** — #303 part 3 is
  accepted-risk-with-trigger (trigger: overflow-checks firing in the
  field or a kiosk report implicating a site).
- **No production logic changes whatsoever.** The only non-test edit
  is the root `Cargo.toml` profile flag (build configuration). The
  `post_read_ops` iterator and the `max_ts` sweep-config marking are
  out (§D2). Anything noticed mid-work becomes a GitHub issue, never a
  drive-by fix.
- **No new dependencies** — no `chrono-tz`, no tz database access; DST
  battery instants are hard-coded UTC constants.
- **No `taskboard-state` / engine-suite additions** (§D1) and no
  snapshot (`insta`) artifacts — assertions are typed outcomes only.
- **No widening of existing bounded strategies or models** (§D5) — the
  existing 16 proptest blocks and every example test stand untouched.
- **A failing new test is never weakened** (§D7): suspected bug →
  verbatim report + a proposed fix prompt for a future agent, test
  left failing if the fix is out of scope.

## §risks

- **Suspected-bug risk (the real one)**: C3/C4/C5 encode readings of
  the soft-delete verdict, R5 drop-window, and op-bookkeeping
  first-writer semantics. If any fails, that is a live policy finding
  — handled per §D7, not papered over.
- **Fixture realism**: C5's echo-then-absence cycle is likely
  unreachable through the actor's validator-gated flow within one
  cycle; it is still a valid domain-contract test of the pure
  function (the tier rationale is the function's contract, not an
  actor journey). Noted so review doesn't mistake it for an
  integration claim.
- **Chrono-boundary brittleness**: the adversarial strategy derives
  its ranges from `MIN_UTC`/`MAX_UTC`/`TimeDelta` constants rather
  than magic numbers, so a chrono bump shifts the strategy with the
  types instead of panicking in the strategy itself.
- **Prop-suite cost**: ~1.8k additional generated cases of pure
  arithmetic — well under a second of suite time; no CI lane
  rebalancing needed. Release smoke adds one release build
  (~minutes, one-off per stage-2 session).
- **Anchor drift**: all anchors re-verified on this branch
  (byte-identical to `v0.1.0` for the four touched files); stage 2
  re-checks any anchor it edits near, per house habit.
