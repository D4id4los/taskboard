# Build & Test Runtime Analysis — 2026-11-19

Commissioned analysis: which parts of the workspace make compilation/linking
slow, and which tests run slow. Measured on the dev checkout (rustc 1.99.0,
main @ be5403c7), warm `target/`, 4 logical CPUs, `nextest -j 4`.

## Method

1. `cargo build --workspace --all-targets --timings` (incremental, deps cached)
   → per-unit durations from the timings HTML.
2. Full suite via `scripts/memory-gate.sh cargo nextest run --workspace -j 4`
   with `--message-format libtest-json-plus` for per-test `exec_time`.
3. Static facts: test-target counts, manifests, `.cargo/` config, linker
   availability.
4. Timed rebuild + link of the `taskboard` bin (`touch src/main.rs`).

## A. Compile & link

### The single dominant cost: integration-test binaries

The workspace has **98 `tests/*.rs` integration-test targets**
(34 taskboard-app, 31 taskboard-ui-slint, 12 state, 9 sqlite, 7 sync-nextcloud,
4 sync-files-nextcloud, 1 domain). Every one is a separate binary that
statically links its crate plus the full dependency closure.

Incremental `--all-targets` build (deps cached): wall **4m 25s**, CPU
**2104s** across 123 units. Split:

| Bucket | CPU time | Share |
|---|---|---|
| Integration-test binaries (102 units incl. examples/bench) | **1471s** | **70%** |
| `taskboard-ui-slint` lib (dev) + build script | 145s | 7% |
| All other lib/bin units | ~490s | 23% |

Worst individual units (compile + link, seconds):

- `taskboard-ui-slint` lib **as test target**: 232s
- `taskboard-ui-slint` lib (dev): 128s; its build script: 17.7s
- 10 ui-slint test binaries at 20–71s each (`overlay_migration_probe_tests`
  71s, `window_max_probe_tests` 62s, `journeys` 62s, …)
- ~20 app test binaries at 17–28s each (every `cli_*`, `ipc`, `bootstrap`,
  … links the whole graph: app lib → ui-slint → slint runtime)
- `taskboard-domain` lib(test): 44s (proptest strategies in-test-profile)

Why each ui-slint test binary is so expensive: the slint runtime (winit,
femtovg, fontique) is compiled **once** but *linked* **31 times**, once per
test binary, in dev debuginfo. Linker is the system default **bfd `ld`**
(there is no `.cargo/config.toml`; `lld` *is* installed at `/usr/bin/lld`).
bfd is the slowest of the three mainstream linkers for large static binaries.

Why `lib (test)` is 232s vs 128s plain: `cfg(test)` recompiles the whole
69k-LOC crate *plus* every `#[cfg(test)]` module with dev-deps activated.

The steady-state single-file edit loop is healthy: recompiling the `taskboard`
bin after touching `main.rs` is ~3s; the 2m00s observed once was a one-off
feature-set invalidation after the test-profile build, not the norm.

External dependency build (872 crates in Cargo.lock — slint, reqwest+rustls,
hyper, sqlx, notify-rust/zbus, rodio) is a one-time cold cost and is *not*
part of the recurring pain; the recurring pain is the internal test-binary
matrix. Coverage runs pay it again instrumented (the known 13–25G
`target/llvm-cov-target` note in AGENTS §10).

### Fix levers (ranked)

1. **lld for dev/test profiles** — one `.cargo/config.toml` stanza, zero code
   churn, helps all 98+ links. (`-Clinker-features=-ld,+lld` on rustc 1.99.)
2. **Consolidate probe test binaries.** ui-slint has 19 `*_probe.slint` +
   `*_probe_tests.rs` pairs, one binary each; app mirrors the pattern with
   per-verb `cli_*` files. Merging probe tests into a handful of binaries
   removes ~90% of the duplicated link work — the biggest structural lever
   (~1471s → ~300–400s CPU). Note the tension with the repo's
   one-behavior-one-test convention: consolidation groups *binaries*, not
   test cases; `cargo nextest test-name` filtering still works.
3. **Slint test-mode recompile**: the 232s `lib (test)` unit re-runs the
   include_modules expansion with dev-deps. Moving inline `#[cfg(test)]`
   modules out of the ui crate into integration tests (which already exist
   for everything else) shrinks it toward the 128s baseline.
4. `dev` profile already uses `debug = "line-tables-only"` (2026-10-18
   report R4) — no further debuginfo lever.

## B. Test suite runtime

Suite itself: **4976 tests, 350s wall** at `-j 4` (sum 1368s; median 14ms).
The runtime is *not* broadly slow — it is dominated by **41 tests over 5s**,
nearly all quantized at exact multiples of **10s**:
20.1, 30.1, 40.4, 50.5, 60.6 …

### Root cause of the 10s quantization

`DEFAULT_SHUTDOWN_TIMEOUT_SECS = 10` (crates/taskboard-app/src/config.rs:42).
Each `cli_verbs`-style invocation is "boot the core, work, shut down"
(the file's own doc comment) — and when an actor does not exit promptly at
teardown, the join waits out the full 10s budget. The test helpers
(`config_for`, `common::valid_config`) never override
`shutdown_timeout_secs`, so every verb pays real wall-clock time:

- `cli_verbs` ceremonies: `destruct_refusal…` 60.6s (≈6 verb invocations × 10s)
- `cli_verbs` v15–v18 tests: 40–50s each
- `ipc::ip16_attach_from_persisted_state…` 60.3s
- `bootstrap` failure-path tests: 10–30s (`BOOT_FAILURE_JOIN_BUDGET` = 10s,
  bootstrap.rs:91)
- `dispatch_routing` ×4: 10.1s each (real wiremock 5s delay + 10s teardown)
- `storage-sqlite roundtrip`: 14.1s

Two distinct questions fall out: (a) tests should set
`shutdown_timeout_secs = 1` (or the fixtures should inject a smaller budget);
(b) *why* an actor needs the full 10s at all is itself a finding — a
production `taskboard sync` that always burns its whole shutdown budget is a
real inefficiency, not just a test one.

### The two non-quantized outliers

- `taskboard-domain pipeline::apply_push_report_equals_the_pipeline_push_phase`
  **75s**: a proptest with a five-clause `prop_filter` over `persisted_state_strategy()`
  — rejection sampling discards most draws before a case runs. Fixable with
  a generator that constructs the accepted shape directly (or `PROPTEST_CASES`
  tuning), not by lowering coverage.
- `cli_verbs::v18_stack_add_scopes_the_append_order…` 50.6s / `v17_reset…`
  50.5s: same shutdown-budget pattern (5 × 10s).

### Justified vs fixable

| Cost | Verdict |
|---|---|
| 98 test binaries × duplicated static link | **Fixable** (structure) |
| bfd linker | **Fixable** (config) |
| 10s shutdown budget per verb in tests | **Fixable** (fixture override) — and the underlying actor shutdown latency deserves its own investigation |
| proptest rejection sampling (75s) | **Fixable** (generator shape) |
| wiremock 5s delays asserting timeout behavior | Justified-ish (they pin a real timeout contract); could use shorter delays with proportional assertions |
| slint codegen + 69k-LOC ui crate | Justified (framework reality); only the *test-mode recompile* share is addressable |
| Clean dep build | Justified (one-time, cached) |

## Suggested next steps (in order of ROI)

1. `.cargo/config.toml` with lld for dev/test → measure `--all-targets` delta.
2. Backlog issue: consolidate `*_probe_tests` binaries into grouped targets.
3. Backlog issue: test fixtures override `shutdown_timeout_secs`; separate
   investigation into actor teardown latency (production-relevant).
4. Backlog issue: proptest generator for the push-phase equivalence property.
