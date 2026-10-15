# [Plan] Criterion Benches for the Persistence Hot Path (Chunk P3)

- **Date**: 2026-10-15
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code, no benches yet).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §1 (execution model, §1.4 GUI no-go zones — ci.yml is explicitly off
  limits) and §4 chunk P3; backlog entry "[2026-10-09] Criterion bench
  for the persistence hot path"
  (`.artifacts/taskboard-potential-improvements-backlog.md:705`), itself
  a Phase-2 holdover of
  `.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md`
  §11 (kept out only to hold PR size).
- **Branch / worktree**: `test/storage-hot-path-benches` in
  `../taskboard-WT-test-storage-hot-path-benches`, off `main` @
  `9b79aa1` — verified equal to `origin/main` at planning time (P2's
  attachment-content commits already merged; no P1-style base deviation).
- **Goal**: criterion `[[bench]]` targets in `taskboard-storage-sqlite`
  measuring the two operations the kiosk pays for on every boot and
  every engine batch — `load()` full hydration and one whole-state
  `apply` batch — at N ∈ {100, 1 000, 10 000} tasks, with fixtures that
  are deterministic by construction (no randomness anywhere). The
  existing CI `benchmark` job lights up with **zero ci.yml edits**; a
  perf problem found by a bench goes to the backlog, never fixed here
  (dispatch non-goals).
- **Status**: awaiting stage-2 implementation.

---

## 1. Verified starting point (anchored on main @ 9b79aa1)

1. **The hot path is `repo.rs`, not the actor.** `load_inner`
   (`crates/taskboard-storage-sqlite/src/repo.rs:407`) runs 12 SELECTs
   inside one read transaction and decodes every row into domain types;
   `apply_inner` (`repo.rs:654`) wraps `execute_batch` (`repo.rs:683`)
   in one transaction with the fixed FK partition order. The actor
   (`actor.rs`) is a thin mpsc shell around these two — benches target
   the repository directly through the `TaskRepository` port
   (`load()`/`apply()` are `BoxFuture`s, `repo.rs:394-402`).
2. **The production database shape is `open(path)`** (`connect.rs:73`):
   file-backed, WAL, `synchronous=NORMAL`, FK enforcement, 5 s busy
   timeout, a single pooled connection (`connect.rs:22-45`), migrations
   embedded and applied at boot. `open_memory()` (`connect.rs:90`) is
   the test variant. Benches must use the file-backed recipe — that is
   what a kiosk runs.
3. **The whole-state batch already has a canonical helper** —
   `tests/common/mod.rs::actions_from_state`: boards → stacks → labels
   → tasks → outbox appends → validator upserts → sync status, "the
   batching discipline Phase 3's engine will use". It predates the
   reminder tables (no reminder actions) and lives under `tests/`, so
   benches cannot import it — the bench fixture module builds its own
   equivalent plus the reminder partition (§2 D4).
4. **Deterministic fixture style is established** in `tests/roundtrip.rs`
   (`ts()`, `board(id: u128)`/`stack`/`label`/`task` builders over
   `Uuid::from_u128`). The domain's `test_support` also offers
   `persisted_state_strategy()` — a **proptest strategy (random)**;
   dispatch requires fixed values, so benches do not use strategies or
   the `test-support` feature (the builders construct domain structs
   directly; `taskboard-domain` is a normal dependency).
5. **`PersistedState` derives `PartialEq` + `Default`**
   (`persistence.rs:82`), so fixture determinism is testable by full
   structural equality, and a fresh-db load equals `Default` — cheap
   sanity anchors for the fixture-contract test (§2 D7).
6. **CI's benchmark job needs nothing from this chunk**
   (`.github/workflows/ci.yml:243-279`): stable toolchain, installs the
   GUI system deps, runs `cargo bench --workspace` (ci.yml:265), and on
   `main` pushes a Bencher tracking step re-running the same command
   (ci.yml:267-279) behind `continue-on-error: true` with the TODO
   comment (ci.yml:270-271) that P3 must **not** remove (post-GUI
   CI-housekeeping round owns it, dispatch §1.4/§10.1).
7. **SQLX_OFFLINE holds by construction.** The benchmark job sets
   neither `SQLX_OFFLINE` nor `DATABASE_URL`; the root `.env` is
   git-ignored (`git ls-files` shows only `.env.example`) and carries
   tier-3 live-server values, not a `DATABASE_URL` — so sqlx's macro
   expansion falls back to the committed `.sqlx/` cache (30 query
   files) exactly as the offline mode intends. The check job's
   freshness gate (`scripts/sqlx-prepare.sh --check`, ci.yml:66-67)
   compiles `--workspace -- --all-targets`, i.e. the new bench target
   too — benches call the repository's public API only and add **zero**
   new `query!`/`query_as!` invocations, so the cache diff stays empty
   and the gate stays green.
8. **Criterion 0.8.2 is current** (crates.io index, verified 2026-10-15):
   `rust-version 1.86` ≤ workspace MSRV 1.96; **no default features**
   (`features = {async, cargo_bench_support, html_reports,
   real_blackbox}` — `cargo_bench_support` must be enabled explicitly
   or `cargo bench` integration breaks); mandatory deps are a modest
   permissive set (anes, cast, ciborium, clap, criterion-plot,
   itertools, num-traits, oorandom, page_size, regex, serde,
   serde_json, tinytemplate, walkdir, alloca on unix/windows) —
   plotters/rayon/csv/smol/tokio/futures are all optional now, so a
   trimmed pin keeps the transitive (and `cargo deny`) surface small.
9. **Root manifest pin discipline** (AGENTS §4): shared versions live
   once in `[workspace.dependencies]` (root `Cargo.toml:26-100`, a
   `# Testing` section already exists at `:94-100`); the storage
   crate's `[dev-dependencies]` (crate `Cargo.toml:23-26`) already
   carries `tempfile` — the bench's scratch dbs need nothing new beyond
   criterion.
10. **Coverage gate**: `cargo llvm-cov --workspace` drives cargo's test
    selection; a `harness = false` bench binary is not a test binary
    and is never executed by it, so coverage percentages are unaffected
    (acceptance requires stage 2 to verify this empirically).
    `cargo clippy --workspace --all-targets` and the sqlx freshness
    gate *do* compile the bench target — it must be fmt/clippy-clean
    under the workspace `all`+`pedantic` lints (root `Cargo.toml:22-24`).
11. **Tracing in the measured path**: `apply_inner` logs per batch at
    `info!` and `execute_batch` logs per statement at `trace!`
    (`repo.rs:642-669`). No subscriber is installed in benches (none of
    the crate's deps installs one), so every event hits a disabled
    dispatcher and is a near-zero-cost no-op — benches must NOT
    initialize a subscriber (it would distort measurements).

## 2. Design decisions

### D1 — Layout: one bench target + a criterion-free fixture module

```
crates/taskboard-storage-sqlite/
├── Cargo.toml                  # +criterion dev-dep, +[[bench]]
├── benches/
│   ├── hot_path.rs             # the target: groups, config, main
│   └── support/
│       └── mod.rs              # deterministic fixture builders (no criterion)
└── tests/
    └── bench_fixtures.rs       # fixture-contract test (#[path]-includes support)
```

```toml
[[bench]]
name = "hot_path"
harness = false
```

- `benches/hot_path.rs` declares `mod support;` (resolving to
  `benches/support/mod.rs`; subdirectory files other than
  `benches/<dir>/main.rs` are not auto-discovered, so `support/` cannot
  become a phantom second target).
- `tests/bench_fixtures.rs` pulls the same module in via
  `#[path = "../benches/support/mod.rs"] mod support;` — the standard
  way integration tests and benches share code without touching `src/`
  (production code is frozen for this chunk).
- `support/mod.rs` imports only `chrono`, `uuid`, `taskboard-domain`
  and `std` — all normal dependencies — so it compiles criterion-free
  and commit 1 (fixtures + test) stands alone before criterion exists.
- One target (not per-operation targets): two groups inside one binary
  share the fixture module and keep `cargo bench` output/PR surface
  compact; the N-parameterization is data (a loop over `[100, 1_000,
  10_000]`), not copy-pasted variants (dispatch §P3-3 mandate).
- `#![forbid(unsafe_code)]` and the
  `// SPDX-License-Identifier: MIT OR Apache-2.0` header on all three
  new files (library-tier crate, AGENTS §6; the forbid applies to our
  code only — criterion's internals are its own).

### D2 — Criterion pin: workspace-pinned, features trimmed to what `cargo bench` needs

Root `Cargo.toml`, `# Testing` section:

```toml
criterion = { version = "0.8", default-features = false, features = ["cargo_bench_support"] }
```

and in the storage crate:

```toml
[dev-dependencies]
criterion.workspace = true
```

- **House-style semver range** (`"0.8"`, like `wiremock = "0.6"`);
  the exact version is committed via `Cargo.lock`.
- **`default-features = false` + `cargo_bench_support`**: criterion 0.8
  has *no* default features, and running under `cargo bench` requires
  `cargo_bench_support` explicitly — pinning it in the manifest line
  makes the requirement impossible to forget. `html_reports` (pulls
  plotters/csv) and `rayon` (parallel analysis) are skipped: CI and
  Bencher consume criterion's stdout, and a 6-bench suite needs neither
  — the smaller transitive tree keeps `cargo deny` and the release
  profile compile (workspace `lto = "thin"`, `codegen-units = 1`)
  proportionate.
- **Not the `async` feature**: the benches drive the async port with an
  explicit current-thread `tokio::Runtime` + `block_on` per iteration
  (the same execution shape `tests/roundtrip.rs:142-146` uses, and the
  shape the storage actor's task provides). `block_on` overhead is
  µs-scale against ms-scale measurements; the feature would add
  futures+smol+tokio criterion plumbing for zero measurement fidelity.

### D3 — Deterministic fixtures: fixed arithmetic, no randomness

One builder, parameterized by N, producing a `PersistedState` plus its
`Vec<PersistenceAction>` batch. Every value is a pure function of N and
the index — no rng, no proptest strategies, no clock reads:

| Field | Formula (task index `i`, 0-based) |
|---|---|
| ids | `Uuid::from_u128(KIND_BASE + i as u128)` with one disjoint constant base per entity kind (board/stack/label/task/op/rule) |
| stamps | `EPOCH + i` seconds, `EPOCH = 2024-01-01T00:00:00Z` (`Utc.timestamp_opt`); distinct and strictly increasing in build order per clock family |
| task title/description | `format!("Task {i:05} bench fixture")` / `format!("Deterministic bench fixture task number {i}.")` — realistic payload widths |
| duedate | `Some` iff `i % 3 == 0` |
| done | `Some` iff `i % 10 == 0` |
| stack | `stacks[i % 4]` |
| labels | `i % 2 == 0` → `{labels[i % 6]}`, else `{labels[i % 6], labels[(i + 3) % 6]}` (avg 1.5 labels/task) |
| remote refs | board `RemoteBoardId(77)`, stacks `7 + s`, cards `1_000 + i` |
| remote_seen | `Some(ts)`, fixed per entity |

The exact base constants are implementation detail (arbitrary but
fixed); what matters — determinism, shape, and monotone stamps — is
pinned by the D7 test.

**Fixture shape table** (the N-scaling dimension is tasks; supporting
entities scale proportionally where the engine would scale them):

| Entity | count | notes |
|---|---|---|
| boards | 1 | color `"3366ff"`, remote 77 |
| stacks | 4 | orders 0..=3 |
| labels | 6 | on the one board |
| tasks | N | per the formulas above |
| outbox ops | N/10 | `LocalOp::UpdateTask(task 10j)`, `queued_at` monotone |
| validators | 2 | `Boards` + `Stacks(77)`, fixed etag/`last_modified` strings |
| sync status | 1 | `SyncPhase::Idle`, fixed `last_success` |
| reminder rules | N/20 | one `-15 min` rule per 20th task (`ReminderOffset::before`) |

`reminders.deliveries/snoozes/muted` stay empty (set-shaped sections
with no N-scaling behavior to measure). The fixture's own
`sync.pending_ops` is `0`; `load()` re-derives it to `N/10` — the
fixture-contract roundtrip asserts accordingly (roundtrip.rs precedent,
`pending_ops` is never stored).

### D4 — What is measured: `load` hydration and one whole-state `apply` batch

- **`load` group** (`load/{100,1000,10000}`): one `repo.load()` per
  iteration on a pre-populated file-backed database — the full 12-SELECT
  read transaction plus row decoding plus `PersistedState`
  construction. The result is criterion's black-box (criterion
  black-boxes `iter` return values); the allocation of the hydrated
  maps is part of hydration, i.e. part of the measurement, correctly.
- **`apply` group** (`apply/{100,1000,10000}`): one whole-state batch
  per iteration, where the batch is exactly the fixture's action vector
  — entity upserts for everything in the shape table, the task-label
  set rewrites (delete + inserts) the upserts imply, the outbox
  enqueues, validator upserts, sync status, and the reminder-rule
  upserts. That is `actions_from_state`'s vocabulary plus the reminder
  partition: "the one `apply` batch that persists a whole state"
  (`tests/common/mod.rs:6-8`), ~3.5N statements in one transaction and
  one commit.
- **Population happens in setup, once per bench function, outside the
  measured closure** — the measured iterations therefore always hit the
  `ON CONFLICT … DO UPDATE` branch (the steady-state path a running
  kiosk exercises; the INSERT path is boot-strapping, not hot path).
  Re-applying the identical batch is semantically pinned as idempotent
  by `roundtrip.rs` P4, so per-iteration work is constant.
- **Per-iteration input isolation**: `apply` consumes the action vector,
  so the bench uses `b.iter_batched(|| actions.clone(), |batch|
  rt.block_on(repo.apply(batch)), BatchSize::PerIteration)` — the clone
  (ms-scale against a ≥ 100×-larger apply) stays outside the
  measurement. `load` needs no input cloning (plain `b.iter`).
- **Throughput**: both groups set `Throughput::Elements(N)` — reported
  as tasks hydrated/persisted per second, the comparable axis for
  Bencher trend tracking.

### D5 — Isolation & environment per bench function

- One `tempfile::TempDir` + one sqlite file per bench function (6
  total), created and populated in setup, dropped (auto-cleanup) at
  scope end. Iterations reuse the same file — deliberate: the
  production hot path operates on one long-lived database, and per-
  iteration file recreation would measure migration + population, not
  the hot path.
- File-backed via `open(dir.path().join("bench.db"))` (D-noted at §1.2:
  WAL/NORMAL/FK/single-connection is the production recipe;
  `open_memory` would understate WAL costs). Accepted variance: the OS
  may back the temp dir with tmpfs — a consistent-per-testbed concern,
  not a correctness one (Bencher compares `ubuntu-latest` against
  itself).
- One current-thread tokio `Runtime` per bench function (`::
  enable_all()`), `block_on` per iteration (D2).
- Setup asserts its own sanity once per bench — after population,
  `load()` must return exactly N tasks (and the apply must have
  returned `Ok(())`): a loud panic if the bench would otherwise
  silently measure an empty database (the P3-3 "not a stub" review
  criterion).

### D6 — Sampling configuration & CI wall-time budget

| Group | sample_size | other config |
|---|---|---|
| `load/100`, `apply/100` | default (100) | defaults (3 s warm-up, 5 s measurement) |
| `load/1000`, `apply/1000` | default (100) | defaults |
| `load/10000`, `apply/10000` | **10** (criterion's minimum) | defaults |

Estimated per-iteration cost (WAL/NORMAL, single connection):
`apply/10000` ≈ 35 k statements ≈ 0.3–1.5 s — criterion's default 100
samples would alone cost minutes, hence the minimum-size group;
`sample_size(10)` bounds that bench to roughly warm-up + ~10–30 s.
Total expected `cargo bench --workspace` wall time ≈ 2–3 min including
fixture population — absorbed by the benchmark job's rust-cache and
doubled only on `main` pushes (the Bencher step). Stage 2 records the
actual smoke timings in its report; if `apply/10000` still exceeds ~2
min locally, dropping its `measurement_time` to 10 s is the sanctioned
knob (a recording-configuration choice, never an assertion change).

### D7 — Determinism is pinned by a test, not by hope (the TDD entry point)

`tests/bench_fixtures.rs` (pure, fast, no database except the one
validity roundtrip):

| Test | Pins |
|---|---|
| `fixture_is_deterministic` — build the state twice at each N; assert full `PersistedState` equality | the dispatch acceptance "two runs, identical input shape" (regression-fires if anyone ever swaps in rng/strategy/clock) |
| `fixture_shape_matches_table` — per N: entity counts per the D3 table, label-set sizes in {1, 2}, stamps strictly increasing across tasks | the documented shape; the N-scaling contract Bencher trends rely on |
| `fixture_is_valid_persistence_input` — at N=100: `open_memory()`, `apply(batch)`, `load()` back; per-field equality with `pending_ops == N/10` | the fixture's *contract as bench input* (FK-respecting, decodable) — not a repo re-test (roundtrip.rs owns repo semantics; AGENTS §5 one-behavior-one-test) |

The `#[path]` share (D1) is what makes this a real compiled test of
the exact code the benches run — the standard "don't test test-helpers"
rule is not weakened: these tests exist to satisfy the dispatch's own
acceptance criterion (determinism verification) and pin the fixture's
shape contract; they do not duplicate repository-semantics coverage.

### D8 — CI surface: zero workflow edits, zero cache churn

- No ci.yml change (§1.4 no-go zone): the benchmark job already runs
  `cargo bench --workspace` (benches light up unconditionally), and the
  `continue-on-error` TODO on the Bencher step (ci.yml:270) is
  explicitly the post-GUI CI-housekeeping round's, not P3's (dispatch
  §4 non-goals, §10.1).
- `.sqlx/` untouched: benches add no `query!` macros (§1.7), so
  `scripts/sqlx-prepare.sh --check` — which compiles the bench via
  `--all-targets` — must report the cache fresh with an empty diff.
  Stage 2 runs it explicitly.
- `cargo deny`: criterion's mandatory tree is expected to pass the
  existing allow-list (all MIT/Apache-2.0-class); dispatch §1.4
  pre-authorizes a deny.toml addition *only if* a new transitive
  license demands one — stage 2 verifies and records the outcome.
- MSRV job compiles the workspace with 1.96 ≥ criterion's 1.86 — no
  interaction.

### D9 — Production code is frozen

`crates/taskboard-storage-sqlite/src/**` (and every other crate's src/)
gets zero diff. If a bench reveals a performance problem, the fix is a
backlog entry with the measurement — never an in-chunk change (dispatch
§4 non-goal). The only non-`benches/`/`tests/` files touched: the two
`Cargo.toml` manifests (dependency/target declaration) and `Cargo.lock`.

## 3. TDD test matrix (writing order)

Stage 2 writes, in order:

1. `tests/bench_fixtures.rs` — all three D7 tests, referring to
   `benches/support/mod.rs` via `#[path]` — **red** (module absent).
2. `benches/support/mod.rs` — the deterministic builder (state + batch)
   per D3/D4 — **green**; commit 1 stands alone (criterion-free, D1).
3. `benches/hot_path.rs` + manifest pins (D1/D2) — no unit-test red/green
   applies (measurement code); its correctness gate is the smoke run:
   `cargo bench -p taskboard-storage-sqlite` executes all six benches
   with sane estimates, plus the D5 setup assertions firing on any
   empty-db mistake.
4. Full §10 gates + `cargo bench` smoke + `scripts/sqlx-prepare.sh
   --check` + `cargo deny check` + llvm-cov no-delta verification.

## 4. Implementation steps (each commit standalone, rebase-merge hygiene)

1. `test(storage): deterministic bench fixtures with shape contract` —
   `benches/support/mod.rs` + `tests/bench_fixtures.rs` (TDD: test
   first). Builds and passes alone; no criterion yet.
2. `test(storage): criterion benches for load and apply hot path` —
   root `Cargo.toml` criterion pin (D2), storage `Cargo.toml` dev-dep +
   `[[bench]]` (D1), `benches/hot_path.rs` (D4–D6), `Cargo.lock`.
3. `docs(backlog): mark storage hot-path benches done` — ✅ DONE on the
   `[2026-10-09] Criterion bench for the persistence hot path` entry,
   one-line note (six benches, criterion 0.8, Bencher tracks on main).

## 5. Parallel-safety audit (dispatch §1.4)

- **No-go zones — none touched**: no `taskboard-ui-slint/**`; no app
  `bootstrap.rs`/`config.rs`/`main.rs`; no `AppState`/domain/message
  shape changes (domain diff is zero); **no ci.yml, no pre-commit**; no
  `release.yml`/`bump_version.sh`/workspace version; no ADRs; no
  `docs/testing_strategy.org` (P7/P8's shared file — this chunk stays
  out of it entirely; the bench design lives in this plan).
- **Shared-file expectations met**: root `Cargo.toml`
  `[workspace.dependencies]` gains exactly one line in the `# Testing`
  section (the criterion pin) — adjacent-line rebase with the GUI
  slint pin (chunk 1) per dispatch §1.4, trivial; `deny.toml` only if
  cargo deny demands it (D8); the backlog file only in P3's own final
  commit.
- **Lane B is independent** (dispatch §1.2): nothing else touches the
  storage crate during the GUI phase.

## 6. Risks & mitigations

| Risk | Assessment / mitigation |
|---|---|
| `apply/10000` blows the CI benchmark job's time budget | `sample_size(10)` group config (D6); wall-time estimate ≈ 2–3 min total; sanctioned knob is `measurement_time`, stage 2 records actuals; the job is non-required with rust-cache, and the Bencher TODO removal is deliberately not ours |
| Criterion 0.8 API drift vs the sketched calls (`iter_batched`, `Throughput`, group config) | Long-stable core API; stage 2 verifies against the 0.8.2 rustdoc and adapts mechanically — measurement design (D4–D6) is version-independent |
| `cargo deny` flags a criterion transitive license | Tree is all-permissive (§1.8); if denied, add one commented allow entry — pre-authorized by dispatch §1.4 (deny.toml shared-file row) |
| clippy `all`+`pedantic` (`-D warnings`) fires on bench code (casts, `collapsible_if`, …) | Write clean from the start (`u64::from(usize)`-style conversions, named fixture builders, one abstraction level per fn); `--all-targets` compiles benches so CI catches any slip |
| Benches silently measure an empty/stub database | D5 setup assertions (`Ok(())` on population apply, `tasks.len() == N` on the verification load) panic loudly before any measurement |
| /tmp tmpfs understates disk I/O | Accepted, consistent per testbed (D5); Bencher compares like-for-like on `ubuntu-latest` |
| `cargo test`/`llvm-cov` behavior with a `harness = false` target | Worst case they compile the bench (fmt/clippy-clean anyway); it is never executed as a test — stage 2 verifies coverage output is unchanged (dispatch acceptance) |
| Fixture builder regresses into randomness later | `fixture_is_deterministic` (D7) fails the suite — the acceptance criterion is a durable test, not a one-off check |

## 7. Out of scope → backlog (stage 2 appends only if evidence demands)

- **Any performance fix** the benches surface (hot-path SQL changes,
  read pool, write batching) — dispatch §4: report with the
  measurement as a backlog entry; existing sibling entries ("Read
  pool / concurrent loads", "Write batching / pipelining",
  `backlog.md:720/785`) already cover the likely suspects.
- **Bencher alerting/thresholds** and the `continue-on-error` TODO —
  post-GUI CI-housekeeping round (dispatch §10.3).
- **Bench-issued `cargo bench` filters in CI** (per-commit cheap bench
  vs weekly full run) — a CI-policy decision, not P3's.

## 8. Estimated diff

≈ 450–520 added lines over 5 files (plus `Cargo.lock`):
root `Cargo.toml` (+2), `crates/taskboard-storage-sqlite/Cargo.toml`
(+7), `benches/support/mod.rs` (+~200: builder + shape constants),
`benches/hot_path.rs` (+~190: env setup, two groups, config, sanity
assertions), `tests/bench_fixtures.rs` (+~90), backlog note (+2).
Zero production-code lines.
