# [Plan] Kiosk Wave — K7 DTO↔domain mapping bench (`feat/perf-mapping-bench`)

- **Date**: 2026-10-20
- **Kind**: stage-1 TDD-first implementation plan for dispatch chunk 7 of
  the kiosk implementation round (slug `mapping-bench`, MD-6).
- **Origin** (normative, in MD-7 rank order): implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 KDF-7 row 5, §3 K7, §4 gates, §5 sequencing, §6 bookkeeping);
  audit `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md`
  (§2 row 5 + the composed sync-CPU-share row, §3 sketch S4, §1/§4
  conventions and D2/D4/D7 notes); dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  (§1 chunk 7 + shared execution model, §8 chunk 7 — MD-4/5/6/7/8/9);
  AGENTS.md (§5 testing directives, §6 headers, §10 gates).
- **Status**: **READY** — every open question settled with a recorded
  decision (§10 D1–D13); no deviation from any ACCEPTED ruling exists
  or is needed (MD-7). Precondition: none (MD-1 — no round gate).
  Verified at planning time: `scripts/worktree-doctor.sh` exit 0
  (114 G free, floor 25 G); worktree
  `../taskboard-WT-feat-perf-mapping-bench` created on new branch
  `feat/perf-mapping-bench` off `main` @ `ac77807`.
- **Stage-2 mandate shape**: benches + fixture + contract test +
  manifest dev-section only; run the bench, record numbers (MD-5);
  full AGENTS §10 gate suite green; no push, no PR, no tracker edits.

## 1. Scope

**In** (K7): a criterion bench in `taskboard-sync-nextcloud/benches/`
over the pure mapping surface — `mapping.rs` DTO↔domain arms and
`push.rs` `plan_pushes` — at board scale N with `hot_path` fixture
conventions; the allowance-vs-30 s derivation recorded in the bench
docs once numbers exist; picked up by the existing CI benchmark job
with zero workflow edits. Files touched:

- `crates/taskboard-sync-nextcloud/benches/mapping.rs` (new)
- `crates/taskboard-sync-nextcloud/benches/support/mod.rs` (new)
- `crates/taskboard-sync-nextcloud/tests/mapping_bench_fixtures.rs` (new)
- `crates/taskboard-sync-nextcloud/Cargo.toml` (dev-section only)

**Out** (K7 non-goals, restated as hard rules): no wiremock full-cycle
harness (network-dominated — S4's own wording; the bench imports no
wiremock, no reqwest, no tokio); **no mapping or push code changes**
(any perceived need for one is a STOP-and-report, D13); no CI-workflow
diffs (§9); no CHANGELOG line (dispatch 7.2: bookkeeping = bench docs
only, D11); no actor/production diff anywhere in the workspace.

## 2. Seams re-located (MD-8; symbols @ `ac77807`)

Line numbers are plan-day references; stage 2 re-locates by symbol.

- `crates/taskboard-sync-nextcloud/src/mapping.rs` — public pure fns:
  `map_board` (:46), `map_stack` (:59), `map_card` (:76), `map_label`
  (:101); echo delegates `map_echo`/`map_stack_echo`/`map_label_echo`
  (:116–130); `classify_push` (:152) + `PushCtx` (:135);
  `classify_read` (:190); write builders `new_card_from` (:212),
  `board_changes_from` (:235), `stack_changes_from` (:245),
  `label_changes_from` (:256), `apply_card_shape` (:267); private
  helpers `epoch_secs` (:31), `deleted_at_secs` (:39) — exercised only
  through the public arms.
- `crates/taskboard-domain/src/push.rs` — `plan_pushes` (:317),
  `EntityTables` (:214), `PushGroup` (:229), `MaterializedOp` (:51),
  `NewCardShape` (:143), `CardShape` (:164); private shape builders
  `NewCardShape::from_task` (:182) / `CardShape::from_task` (:196) —
  the bench cannot call these; it constructs shapes by the same field
  list (D3).
- Wire DTOs `crates/taskboard-sync-nextcloud/src/model.rs` — `Board`
  (:18), `Stack` (:80), `Card` (:103), `CardLabel` (:152), `Label`
  (:206); `DeckColor::from_hex` (`color.rs` :34) is the public
  constructor for wire colors. Client write params in `client.rs`
  (`NewCard`, `BoardChanges`, `StackChanges`, `LabelChanges`) are the
  write builders' outputs.
- Fixture + criterion conventions
  `crates/taskboard-storage-sqlite/benches/{hot_path.rs, support/mod.rs}`:
  `N_SIZES = [100, 1_000, 10_000]` (support :27), `STACK_COUNT = 4`
  (:30), `LABEL_COUNT = 6` (:32), `outbox_count(n) = n/10` (:36),
  disjoint per-kind uuid bases (:48–53), fixed `EPOCH_SECS` stamp
  origin with per-family offsets (:58), setup outside measured
  closures, no tracing subscriber, `Throughput::Elements`, and the
  per-crate-helper precedent note (:221–225).
- Fixture-contract test precedent
  `crates/taskboard-storage-sqlite/tests/bench_fixtures.rs`
  (`#[path = "../benches/support/mod.rs"]` include; determinism + shape
  pins; no DB round-trip leg needed here).
- Bench target config precedent
  `crates/taskboard-storage-sqlite/Cargo.toml` (:27 `criterion.workspace
  = true`; :29–31 `[[bench]]` + `harness = false`); criterion pinned
  once at root `Cargo.toml:114`, locked **0.8.2**, which exposes
  `criterion::black_box` (registry `criterion-0.8.2/src/lib.rs:165`).
- CI `.github/workflows/ci.yml` benchmark job (:296–353, re-verified at
  `ac77807` — supersedes the audit's tag-day :279–315 citation):
  `cargo bench --workspace` at :337 (every PR event) and inside
  `bencher run` at :353 (main pushes only, `rust_criterion` adapter).
  The Bencher graduation landed (double run fixed, no
  `continue-on-error`), so the job already tracks every workspace
  criterion group with zero wiring.

## 3. The benched function set (enumerated)

Every public pure fn of the mapping surface is enumerated with its
decision — BENCH (with its group) or SKIP (with the reason). This is
the acceptance list for stage 3.

| # | Function | Decision | Group / scale |
|---|---|---|---|
| 1 | `map_board` | BENCH | `map_board/1` (fixed: 1 board) |
| 2 | `map_stack` | BENCH | `map_stack/4` (fixed: 4 stacks) |
| 3 | `map_card` | BENCH | `map_card/{n}` × N_SIZES (the hot arm) |
| 4 | `map_label` | BENCH | `map_label/6` (fixed: 6 labels) |
| 5 | `map_echo` / `map_stack_echo` / `map_label_echo` | SKIP | one-line delegates to #3/#2/#4 plus an enum wrap; the delegates' numbers upper-bound them (D4) |
| 6 | `classify_push` | SKIP | O(1) table walk, once per *failed* op — an error-path cost, not a per-entity cycle leg; outside S4's enumeration (`map_*` / `*_changes_from` / `apply_card_shape` + `plan_pushes`) (D4) |
| 7 | `classify_read` | SKIP | same (D4) |
| 8 | `new_card_from` | BENCH | `new_card_from/{n}` × N_SIZES |
| 9 | `board_changes_from` | BENCH | `board_changes_from/1` (fixed) |
| 10 | `stack_changes_from` | BENCH | `stack_changes_from/4` (fixed) |
| 11 | `label_changes_from` | BENCH | `label_changes_from/6` (fixed; fixture colors are valid hex → the `Some` arm — the `None` arm is one hex check before it) |
| 12 | `apply_card_shape` | BENCH | `apply_card_shape/{n}` × N_SIZES, `resolved_labels = Some(&[…])` — the label-resolution arm, a superset of the `None` arm's work (D8) |
| 13 | `plan_pushes` | BENCH | `plan_pushes/steady/{n}` + `plan_pushes/burst/{n}` × N_SIZES — the pure fn called directly with `EntityTables` (never through the actor) (D6) |

`EntityTables`/`PushGroup` are the planner's in/out types, not separate
benches. Total: **10 benched functions, 21 criterion benchmarks**
(§5). Read direction (#1–#4) is the pull leg — called once per listed
entity per cycle; write direction (#8–#12) is the push leg's payload
construction; #13 is the coalescing planner.

## 4. Fixture construction (`benches/support/mod.rs`)

A self-contained deterministic builder in *this* crate's
`benches/support/`, mirroring the `hot_path` support conventions
verbatim in shape and spirit (D2). No rng, no proptest strategies, no
clock reads; every value a pure function of N and the entity index.

- **Scale**: `N_SIZES: [usize; 3] = [100, 1_000, 10_000]`;
  `STACK_COUNT = 4`; `LABEL_COUNT = 6`; one board; tasks fan out
  round-robin over stacks; label-set sizes 1–2 (odd index adds a second
  label) — the hot_path ratios exactly.
- **Stamp scheme**: fixed `EPOCH_SECS` origin (2024-01-01T00:00:00Z,
  same value), stamps `EPOCH_SECS + <family offset> + i`, distinct and
  strictly increasing per family.
- **Id scheme**: disjoint per-kind uuid bases via
  `Uuid::from_u128(base + i)` — a *new* base family (documented in the
  module; e.g. `0xA*_...` range) so the two crates' fixtures cannot be
  confused; within this module the per-kind bases are mutually disjoint
  as in `hot_path`.
- **Domain family** (inputs to `plan_pushes` + the `*_from` builders):
  domain `Board` (bound, `remote: Some(RemoteBoardId(77))`), 4 `Stack`s
  (bound), 6 `Label`s (bound), N `Task`s (bound, with `duedate` iff
  `i % 3 == 0`, `done` iff `i % 10 == 0` — both `Option` arms exercised
  downstream), each with the hot_path clock scheme, collected into the
  four `BTreeMap`s that `EntityTables` borrows.
- **Outboxes** (D6):
  - `steady_outbox(n)`: `n/10` `UpdateTask` ops, one per 10th task —
    the `hot_path` `outbox_count`/`fixture_op` shape verbatim (no
    coalescing; distinct targets).
  - `burst_outbox(n)`: one op per task (n ops), kind rotating
    `[UpdateTask, AssignLabel, CreateTask, DeleteTask, MoveTask][i % 5]`
    — the offline-editing-session worst case that exercises create
    materialization (`NewCardShape` construction), delete dominance,
    and per-entity grouping at scale.
- **Wire family** (inputs to the read mappers + `apply_card_shape`):
  `model::Board` (id 77, `DeckColor::from_hex("00c2e0")`), 4
  `model::Stack`s (ids 7–10), 6 `model::Label`s (ids 9–14), N
  `model::Card`s (id `1000 + i`, `stack_id` round-robin, `labels` = the
  same 1–2 bare `CardLabel` ids as the domain label-set rule,
  `duedate`/`done` arms per the same modulo rules, distinct
  `last_modified` per i) — built as **struct literals, not serde JSON**
  (D3: JSON would measure serde and slow setup at N = 10 000; the wire
  shapes themselves are already contract-tested by `model.rs`'s
  fixture-file tests).
- **Shape builders** (for #8/#12): `CardShape`/`NewCardShape` derived
  deterministically from fixture tasks with the same field list as the
  private `from_task`s (documented — the bench constructs shapes
  itself; the private builders are not reachable and would not change
  the builders-under-test anyway, since #8/#12 consume ready shapes).

## 5. Criterion grouping & bench mechanics (`benches/mapping.rs`)

| Group | Ids | One iteration measures | Throughput |
|---|---|---|---|
| `map_board` | `1` | `map_board(&wire_board)`, result `black_box`ed | 1 |
| `map_stack` | `4` | all 4 wire stacks mapped | 4 |
| `map_label` | `6` | all 6 wire labels mapped | 6 |
| `map_card` | `100` / `1000` / `10000` | all n wire cards mapped | n |
| `new_card_from` | `100` / `1000` / `10000` | n shapes → `NewCard`s | n |
| `apply_card_shape` | `100` / `1000` / `10000` | n fetched-card + shape pairs applied (`Some` labels) | n |
| `board_changes_from` | `1` | the domain board → `BoardChanges` | 1 |
| `stack_changes_from` | `4` | all 4 domain stacks | 4 |
| `label_changes_from` | `6` | all 6 domain labels | 6 |
| `plan_pushes` | `steady/100…10000` | one `plan_pushes(&steady_outbox, tables)` | n/10 |
| `plan_pushes` | `burst/100…10000` | one `plan_pushes(&burst_outbox, tables)` | n |

Mechanics (all recorded decisions):

- **D5 — id style**: fixed-scale ids are the element count (`"1"`, `"4"`,
  `"6"`) — the hot_path id-is-the-count style kept honest about scale;
  per-N ids are `"{n}"`; the planner's two cases ride one
  `plan_pushes` group as `"steady/{n}"` / `"burst/{n}"`.
- **D7 — no sampling overrides**: every leg is sub-ms at N = 10 000
  (the mappers are struct copies + String clones; the planner is
  bucketing + one shape build per entity), so criterion defaults
  (sample_size 100, warm-up + 5 s measurement) apply — no
  `configure_group`. The storage bench's 15 s/10-sample budget exists
  for its hundreds-of-ms ops and does not transfer. Expected whole-run
  wall ≈ 3–4 min for 21 benchmarks — inside the CI box (the audit
  measured the storage bench alone at ≈ 6 min, §1 of the audit report).
- **Measured bodies**: setup outside the closure (hot_path rule);
  results returned through `criterion::black_box` (0.8.2 exposes it);
  allocations stay inside the measurement — the mappers allocate by
  design and the Vec-of-results keeps outputs alive, matching
  production use. `b.iter` suffices (no per-iteration fresh input is
  needed: every fn takes shared refs; no `iter_batched`).
- **No runtime**: all benched fns are pure and synchronous — no tokio
  runtime, no tempfile, no DB anywhere in the bench (D1).
- **No tracing subscriber** installed (the hot_path rule) so measured
  paths stay disabled-dispatcher no-ops.

## 6. The allowance-derivation note (verbatim template)

K7 requires the allowance-vs-30 s derivation "recorded in the bench's
docs once numbers exist". The bench module doc carries this section;
**stage 2 fills the bracketed placeholders from the actual run** and
changes nothing else in the text (D9):

```markdown
## Budget derivation (budget row 5: sync-cycle wall ≤ 30 s per
## typical board)

The cycle's wall time is network-dominated (audit plan D7): only the
CPU share is in-repo measurable, and the groups above are that share.
Per cycle at board scale N = 10 000 (the largest fixture tier):

- pull side: `map_board` ×1 + `map_stack` ×4 + `map_label` ×6 +
  `map_card` ×N
- push side: `plan_pushes` on the steady outbox (N/10 ops); the burst
  variant (N ops, five-kind rotation) is the offline-session worst
  case
- write side: the builders, × the cycle's op counts (≤ outbox depth)

Dev-machine numbers (stage-2 run, <date>): pull ≈ <PULL>, push steady
≈ <PUSH_STEADY> (burst ≈ <PUSH_BURST>), write ≈ <WRITE>; composed CPU
share ≈ <TOTAL> of the 30 000 ms budget = <FRACTION>. Under the audit's
D2 derate note (a full order-of-magnitude in-order-core derate) the
share stays ≤ <10×FRACTION> — still a small fraction of the cycle
budget (the S4 pass criterion). Dev-machine references, never device
conformance: the on-device verdict is the kiosk spike's. No budget
value is invented here (audit plan D4) — the 30 s envelope is consumed
from `§budgets`, not re-derived.
```

If the measured fraction is *not* small at the burst worst case, the
note states the measured fraction honestly and the stage report flags
it — the numbers are recorded either way (MD-5 tier honesty); a budget
verdict stays the spike's.

## 7. TDD order & the fixture-contract test

TDD leg first (AGENTS §5; the `bench_fixtures.rs` precedent), then the
builder, then the bench:

1. **`tests/mapping_bench_fixtures.rs`** — includes the support module
   via `#[path = "../benches/support/mod.rs"] mod support;` and pins
   the bench-input contract:
   - *determinism*: two builds at each N are equal (wire family and
     both outboxes) — the durable guard against rng/strategy/clock
     sneaking into the builder;
   - *shape per N*: 1 wire board, 4 wire stacks, 6 wire labels, n wire
     cards; steady outbox = `n/10` ops, all `UpdateTask`, distinct
     target tasks; burst outbox = n ops with the five-kind rotation's
     exact per-kind counts (a partition check over `LocalOp` variants),
     all op ids distinct; card `duedate` present iff `i % 3 == 0`,
     `done` iff `i % 10 == 0` (count assertions, both arms covered);
     label-set sizes ∈ {1, 2}; distinct card ids and stamps;
   - *typed structural assertions only* (AGENTS §5 — no text).
   No DB leg: unlike the storage contract test there is no repository
   here; validity-as-input is the shape + determinism contract, and the
   bench smoke in the default test lane exercises construction.
2. **`benches/support/mod.rs`** — make the contract test green.
3. **`benches/mapping.rs` + `Cargo.toml`** — the target compiles
   (`cargo bench -p taskboard-sync-nextcloud --bench mapping --no-run`)
   and runs, printing the 21 benchmarks of §5.

## 8. Manifest diffs (dev-section only)

`crates/taskboard-sync-nextcloud/Cargo.toml`:

- `[dev-dependencies]` += `criterion.workspace = true` (workspace pin
  at root `Cargo.toml:114`; never a member-local version — AGENTS §4).
- `[[bench]]` / `name = "mapping"` / `harness = false` (the storage
  crate's :29–31 shape).

Nothing else: `uuid` (dev-dep) and `chrono` (dep) are already present;
no new crate, no feature change, no `[dependencies]` change — so
`cargo deny` sees no new tree. SPDX `MIT OR Apache-2.0` +
`#![forbid(unsafe_code)]` on all three new files (§6: a library-crate
member).

## 9. Zero-CI-diff verification

- **Planning-time evidence (recorded)**: the benchmark job runs
  `cargo bench --workspace` on every PR (ci.yml:337) and on main pushes
  inside `bencher run` (ci.yml:353, `rust_criterion` adapter parsing
  criterion stdout). Cargo discovers `[[bench]]` targets automatically;
  Bencher tracks the new groups without wiring. **Zero workflow edits
  are needed** — K7's premise, verified at `ac77807`.
- **Stage-2 verification**: `git diff --stat` shows no `.github/` path;
  `cargo bench -p taskboard-sync-nextcloud --bench mapping` runs green
  and prints the §5 table.
- **Stage-3 verification**: `git diff origin/main...HEAD -- .github/`
  is empty — any workflow diff is a finding (dispatch §8 trap).

## 10. Decision record

- **D1 — bench target**: `benches/mapping.rs` in
  `taskboard-sync-nextcloud`, `[[bench]] name = "mapping"`,
  `harness = false`, criterion via the workspace pin. K7 names the
  location; the storage crate is the in-repo precedent. Criterion over
  a harness script: the legs are pure CPU functions — exactly
  criterion's element, and the CI job + Bencher tracking come free.
- **D2 — self-contained support module**: cross-crate import of the
  storage fixture is rejected (bench targets cannot import another
  crate's `benches/support/`; that module itself documents the
  per-crate-helper precedent). Conventions are mirrored verbatim
  (scale, ratios, stamp + id schemes, setup discipline); constants are
  re-declared locally under a distinct documented uuid-base family.
- **D3 — wire DTOs as struct literals**: serde JSON construction would
  measure deserialization and dominate setup at N = 10 000; the wire
  shapes are contract-tested elsewhere. Shape builders for #8/#12 are
  constructed in the bench by the private `from_task` field list
  (documented in the support module).
- **D4 — skip list**: echo delegates (pure delegation — the delegates'
  numbers upper-bound them) and `classify_push`/`classify_read`
  (O(1) error-path walks, once per failed op, not per-entity cycle
  legs; outside S4's enumeration). Both skips are recorded here so
  "each arm enumerated" is demonstrably true.
- **D5 — grouping/ids**: §5's table; fixed-scale arms bench the whole
  fixture's worth of that arm per iteration (a per-cycle-leg cost,
  directly summable in §6), id-ed by element count.
- **D6 — two `plan_pushes` input shapes**: steady (hot_path's outbox
  verbatim; no coalescing) + burst (one op per task, five-kind
  rotation). Steady alone would never exercise create materialization,
  delete dominance, or subsuming — the burst case is the honest
  worst-case input for the "small fraction of 30 s" claim.
- **D7 — criterion defaults**: all legs sub-ms; no `configure_group`;
  expected run ≈ 3–4 min inside the CI box.
- **D8 — `apply_card_shape` on the `Some` arm**: the label-resolution
  arm is a strict superset of the `None` arm's work; one case per N.
- **D9 — allowance note**: §6's verbatim template lands in the bench
  module doc; stage 2 fills only the bracketed numbers + date. Numbers
  are dev-machine references (MD-5), restated in the stage report and
  PR body; on-device verdicts are the spike's.
- **D10 — zero-CI-diff verification**: §9's three checks (evidence,
  stage-2, stage-3).
- **D11 — bookkeeping**: no CHANGELOG line (dispatch 7.2: "none beyond
  the bench docs"); no tracker edits (MD-4 — #263 closes via the PR
  body's `Closes #263` at stage 3).
- **D12 — test-lane cost**: the new `harness = false` target surfaces
  in `cargo nextest run --workspace --all-targets` as criterion
  smoke entries (each bench function run once in criterion's test
  mode). Cheap here — fixture construction is ms-scale and there is no
  DB population (contrast: the storage bench's ≈ 62 s test-lane cost,
  audit F13-07). F13-07's lane-filtering recommendation is a separate
  item, untouched by this chunk.
- **D13 — no production diff**: the branch's complete diff is §1's
  four files. Any change to `src/` under any crate (including
  "helper" visibility tweaks to mapping.rs) is out of scope — the
  bench consumes only public pure APIs, which all exist. If stage 2
  finds one seemingly necessary, that is a STOP-and-report, not an
  edit.

## 11. Traps → plan answers (dispatch §8 checklist)

| Trap (dispatch §8) | Answered by |
|---|---|
| network/wiremock code in the bench | §1 hard rule + D1 (no wiremock/reqwest/tokio imports) |
| fixture scale mismatched to `hot_path` | §4 (N_SIZES, ratios, stamp/id schemes verbatim) + contract test pins |
| `plan_pushes` benched through the actor | §3 #13 (pure fn + `EntityTables` directly) |
| a CI-workflow diff | §9 (must be empty at stage 3) |
| production-code diffs | §1 + D13 (four-file diff only) |
| missing numbers (MD-5) | D9 (report + PR body carry them) |
| weak black_box hygiene | §5 mechanics (returns through `criterion::black_box`, setup outside closures) |

## 12. Estimated diff size

≈ **430–540 lines**, all new, zero production lines, zero CI lines:
`benches/mapping.rs` ≈ 130–160; `benches/support/mod.rs` ≈ 200–240;
`tests/mapping_bench_fixtures.rs` ≈ 70–100; `Cargo.toml` +3–4.
Single-commit shape (`docs(plan)` on top of this plan commit;
implementation commits per §7's TDD order, each standalone for
rebase-merge hygiene).
