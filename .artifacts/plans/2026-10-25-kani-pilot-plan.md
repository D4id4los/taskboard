# [Plan] Kani Pilot on the Pure Domain Core (backlog-triage chunk 6)

- **Date**: 2026-10-25
- **Kind**: chunk plan — stage-1 planning record for chunk 6 of the
  backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §7).
  Implementation = new `#[cfg(kani)]` harness modules in two existing
  crates + one new workflow file + this plan; **zero production-code
  diffs** (dispatch acceptance). The feasibility probe that informs
  everything below was throwaway (`/tmp/kani-probe-*`) and does not
  land.
- **Branch / worktree**: `test/kani-pilot` /
  `../taskboard-WT-test-kani-pilot` (doctor exit 0, 79 G free, floor
  25 G).
- **Base**: `f7b1ac8` = fetched `origin/main` tip. The worktree was
  first cut at `44a7d54`; after the probe phase it was fast-forwarded
  to the new tip (zero commits on the branch at that point; the five
  target files are byte-identical between the two tips — verified via
  `git diff 1c0000b..f7b1ac8` over the target paths).
- **Incidents, recorded** (both shape stage-2 behavior):
  1. **Worktree sweep**: the first worktree instance (zero commits,
     clean tree) was swept mid-probe by a concurrent agent's
     `worktree-doctor.sh` run — the same pattern chunk 3 recorded in
     `.artifacts/plans/2026-10-25-tier2-convergence-fixtures-plan.md`.
     Nothing was lost (nothing had been committed); the worktree was
     recreated on the same branch. **Stage 2/3: commit the first
     substantive change immediately after (re)creating the worktree —
     a commit absent from `origin/main` is what makes the doctor
     refuse the sweep.**
  2. **OOM (the probe's most important finding, see §1)**: an
     uncapped `cargo kani` run over two harnesses whose symbolic
     inputs were `i64` and whose law crossed a symbolic-content
     `String` path exhausted system memory and the kernel OOM-killer
     took down the whole process chain, including the agent session.
     All subsequent runs went through a `systemd-run` memory guard;
     the guard contained four further kills without collateral
     damage. **Stage 2 rule: every local `cargo kani` invocation runs
     under the guard recipe in §6; no exceptions, including "just a
     quick check".**
- **Normative origins**: issue #426 (normative scope, five-harness
  list, CI shape, success criteria, honest-termination path);
  suitability report §3 C1 + §5 Pilot 2
  (`.artifacts/reports/2026-10-24-type-driven-formal-methods-suitability-report.md`);
  dispatch plan §7 (chunk covers + acceptance, verbatim-intent);
  `.github/workflows/miri.yml` (the structural template kani.yml
  mirrors). Line numbers below are plan-day references (base
  `f7b1ac8`), never load-bearing.

## 1. Probe findings (evidence base for every decision below)

Environment: 30 GiB RAM + 31 GiB swap, 16 cores, x86_64 Linux,
warm cargo registry. Probe crates: `/tmp/kani-probe-backoff`
(`backoff.rs` copied verbatim + harness) and `/tmp/kani-probe-domain`
(path dependency on this worktree's `taskboard-domain`), both with a
`#[cfg(kani)] mod verify`. Not committed anywhere; deleted after
stage-2 bootstrap.

- **Toolchain**: `kani-verifier 0.68.0` (current crates.io release),
  bundling **CBMC 6.11.0** and its own pinned
  **nightly-2026-08-21** (rustc 1.100.0-nightly, 8925ea35 2026-08-20).
  One-time setup ≈ 10 min wall (`cargo install kani-verifier --locked`
  ≈ 1 min; `cargo kani setup` downloads the release bundle + pinned
  nightly ≈ 9 min). This matches issue #426's floor: 0.68.0 is the
  first release with edition-2024 support; the workspace is edition
  2024 / rust-version 1.96.
- **Governance facts verified on this machine**:
  - The `kani` proof library (`#[kani::proof]`, `kani::any`,
    `kani::assume`) **auto-provisions from the bundle — no
    `Cargo.toml` dependency**. The crates.io `kani` crate is a 0.0.1
    placeholder; do not add it. Combined with `#[cfg(kani)]` gating
    (the cfg is set only by Kani's rustc driver), the pilot needs
    **zero manifest changes**: stable builds, clippy, nextest,
    coverage, and deny never see harness code.
  - `cargo kani` runs clean inside the worktree **despite
    `rust-toolchain.toml` pinning stable** — Kani selects its own
    toolchain (rustup `+toolchain` beats the toolchain file). Unlike
    `miri.yml`, kani.yml needs no `RUSTUP_TOOLCHAIN` env override.
- **Probe results**:

  | Probe | Result | Verify | Wall | Peak RSS |
  |---|---|---|---|---|
  | H1 shape: `BackoffPolicy::delay` law, symbolic `u32` | **PROVEN** (103 checks, 0 failed, 2 unreachable) | 0.40 s | 0.92 s | not measured (small) |
  | `taskboard-domain` full dep graph compiles under kani-rustc (chrono 0.4.45, serde, serde_json, thiserror 2, uuid, tracing, proptest) | clean | — | ≈ 12 s warm | — |
  | Symbolic `DateTime<Utc>` minting, `i32` seconds → `from_timestamp` → `Ord` | **PROVEN** | 1.37 s | 2.31 s | **154 MB** |
  | `ReminderOffset` canonical text round-trip, symbolic seconds → `text_form` → `parse_text_form` | **> 6 GiB, killed at cap** | — | — | > 6 GiB |

- **The String wall (H2's defining constraint)**. The round-trip
  harness exceeds 6 GiB **invariantly**: with the default solver
  (CaDiCaL) and with `--solver kissat`; with the symbolic domain at
  ±30 days and at ±1 hour. It is structural, not arithmetic:
  symbolic-content `String` through `format!`/std fmt integer
  rendering + `str::parse::<u64>` over symbolic bytes is the known
  CBMC memory bomb class, compounded by the `i64` divisions inside
  `TimeDelta`/`num_seconds` that narrowing the *input* cannot remove.
  This is precisely issue #426's "chrono/serde symbolism proves
  impractical" branch, observed for the String slice of it — the
  containment ladder for H2 is in §3.2 and the honest-termination
  criteria in §8.
- **The width rule**. CBMC bitblasts the *type* of a symbolic value;
  `kani::assume` ranges do not shrink the circuit. The same
  `DateTime` probe that (as `i64` inputs) participated in the
  machine-killing OOM verifies in 1.4 s / 154 MB as `i32`. **Harness
  rule: every symbolic input is the narrowest type that covers the
  law's domain (`i32`/`u32`, `u8` enum tags), widened only after
  minting.** A 32-bit seconds-since-anchor covers ±68 years — more
  than any harness here needs.
- **Memory guard recipe (mandatory for all local runs)**:
  ```sh
  systemd-run --user --scope -p MemoryMax=6G -p MemorySwapMax=0 \
    /usr/bin/time -v cargo kani --harness <name> --output-format terse
  ```
  The kernel then OOM-kills only the scope. `/usr/bin/time -v`
  captures peak RSS for the budget table (§6). CI does not need the
  guard (runners are ephemeral, 16 GiB public-repo runners); the
  budget table is what keeps harnesses inside the runner envelope.
- **Solver**: Kani 0.68's default is CaDiCaL; `--solver kissat` made
  no difference on the String wall and is not needed on integer-only
  harnesses. Slice formula is on by default; never pass
  `--no-slice-formula` except when debugging a counterexample trace.
- **FFI note**: the domain graph carries reachable-unchecked foreign
  functions (getrandom via uuid v4) — Kani warns "unsupported
  constructs" but only fails if a harness *reaches* them; none of the
  five targets does. The sync crate adds reqwest 0.12 (rustls-tls,
  `ring` in the lock, no openssl-sys): `ring`'s C compiles natively
  via build.rs and is unreachable from `backoff.rs`. Compile-wall
  risk only (§3.1, first stage-2 step measures it).

## 2. Repo integration (how harnesses live in the tree)

- One harness module per crate: `crates/taskboard-domain/src/verify.rs`
  and `crates/taskboard-sync-nextcloud/src/verify.rs`, wired in each
  `lib.rs` with `#[cfg(kani)] mod verify;`. Zero production-code
  changes beyond that one gate line (dispatch non-goal: "no
  production-code changes (harnesses are #[cfg(kani)]-gated)" — the
  gate line is the sanctioned exception, it is itself the gate).
- No new `[dependencies]`/`[dev-dependencies]` entries anywhere (§1:
  the library auto-provisions). No `[features]` additions.
- All quality gates are unaffected: `fmt` formats the module (plain
  syntax), `clippy --all-targets` never enables `cfg(kani)` so the
  module is compiled nowhere, `nextest`/`llvm-cov`/`deny` likewise.
  PR CI on this branch therefore exercises nothing Kani — by design
  (issue: "never in the PR pipeline").
- One harness = one law = one `#[kani::proof]` function, named after
  the law it escalates (the existing proptest's name + a proven
  suffix), so a CI failure names the law, not a file.

## 3. Harness catalogue (smallest-to-hardest, order fixed by #426)

Each harness: the law (what is asserted), the symbolic surface
(what `kani::any()` mints, always narrowest-type), the unwind bound
(with its bound-saturation check, §7), and the containment/stub list.
Expected proof outcomes use Kani's own vocabulary: **proven** /
**failure + counterexample** / **unwinding-assertion failure**.

### 3.1 H1 — `BackoffPolicy::delay` (`taskboard-sync-nextcloud/src/backoff.rs:41`, loop at `:56-67`)

- **Law** (escalates `prop_delays_positive_increasing_capped`,
  `backoff.rs:107-125`, 256 samples → exhaustive within bounds):
  over symbolic `retry_index: u32` with `assume(retry_index < 12)`
  against `BackoffPolicy::new()`:
  1. `delay(i) == Some(d)` ⇒ `d > ZERO ∧ d <= 8s`
  2. `delay(i) == Some(d) ∧ delay(i-1) == Some(p)` ⇒ `d >= p`
  3. `delay(i) == None` ⇔ `i + 1 >= max_attempts`
- **Symbolic surface**: one `u32`. `Duration` internals (`u64` secs +
  `u32` nanos) are concrete constants — no wide symbolic arithmetic.
- **Unwind**: `delay_at`'s doubling loop runs ≤ `retry_index - 1` < 11
  iterations → `#[kani::unwind(12)]` (probe-verified at exactly this
  bound: proven, 2 unreachable).
- **Expected**: **proven** in seconds (probe: 0.40 s verify / 0.92 s
  wall). No stubs. This harness alone de-risks the pilot's ≥3 bar
  together with H3.
- **Containment**: none needed. Compile smoke of the sync crate under
  kani-rustc is stage-2 step 1 (reqwest/hyper/rustls wall-time only,
  §1 FFI note); if the crate graph refuses to compile under kani,
  H1 is *deferred, not dropped* — the ≥3 bar falls to H3+H4+H5 and
  the compile failure is documented as a per-dependency containment.

### 3.2 H2 — `ReminderOffset` canonical text round-trip (`taskboard-domain/src/reminders.rs`, `text_form` `:84-103`, `parse_text_form` `:105-140`)

- **Law** (the canonical inverse: `parse_text_form(text_form(o)) == o`
  for every constructible whole-second offset; escalated from the
  round-trip proptest the report cites at `reminders.rs:1502`):
  over symbolic whole seconds `secs: i32` with
  `assume(-2_592_000 <= secs && secs <= 0)` (±30 days, the config
  grammar's practical magnitude):
  mint `ReminderOffset::before(TimeDelta::try_seconds(secs))` ⇒
  `parse_text_form(&text_form(off)) == Some(off)`.
- **Probe verdict on the full law**: exceeds the memory budget on
  every rung tried (§1 String wall). The harness is authored last and
  descends the containment ladder until a rung proves or the ladder
  is exhausted; every rung's outcome is recorded either way:
  1. **Rung A — prove it anyway** at `#[kani::unwind(24)]` (parse
     loop ≤ 2·(len+1), rendered text ≤ 9 chars) under the guard. One
     attempt, capped, expected to fail at the cap — recorded as the
     measurement, then descend.
  2. **Rung B — stubbed String layer** (`#[kani::stub]`): replace the
     harness-local `text_form` binding with an integer-triple model
     `(h, m, s) -> packed u32` mirroring the documented rendering, and
     prove parse over the packed form: grammar totality (never
     panics), `Some` iff well-formed, value == Σ digit·unit, sign
     discipline (result ≤ 0, zero iff `"0s"`), overflow ⇒ `None`.
     The stub severs the real String path; the harness doc comment
     must say so verbatim (issue: "dependency-induced failures
     contained per-harness (dropped/stubbed, documented), never
     project-blocking"). *What is lost*: no evidence about the actual
     `format!`-rendered bytes.
  3. **Rung C — symbolic `&str` into `parse_text_form`** (stretch,
     time-boxed half a day): the bundled kani library's bounded
     string `Arbitrary` (`kani::any::<&str>()` with length ≤ 8)
     proving the same parse-side laws over genuinely symbolic bytes.
     Investigate the 0.68 enablement mechanism; if it re-hits the
     String wall (likely — same machinery), record and stop.
  4. **Rung D — honest partial termination for this law**: the
     canonical round-trip stays with its proptest; the written
     finding (with the §1 table) closes this law, not the pilot.
- **Expected**: proven at Rung B; Rungs A/C likely fail the budget
  (both are the same String machinery). Success of the *pilot* does
  not depend on H2 (§8).

### 3.3 H3 — `aggregate_rows` phase dominance (`taskboard-domain/src/state.rs:289-310`, `phase_rank` `:277-285`)

- **Laws** (escalate the aggregate fold's unit/proptest coverage to
  exhaustive-within-bounds): over a fixed array of **3 symbolic
  `SyncStatus` rows**:
  1. **Phase dominance**: `aggregate.phase` has the maximal
     `phase_rank` of the rows, and among max-rank rows the *first*
     wins (the `Failed`-tie-keeps-lowest-board-id contract through
     sorted-rows feeding).
  2. **`last_success` = row-wise max** (`Option` `Ord`: `None < Some`).
  3. **`pending_ops` = saturating sum** (never overflows — assert
     equality with a reference saturating fold, and panic-freedom).
  4. **Counters**: each `SyncCounters` field monotone non-decreasing
     through `fold_into`, `last_cycle` = max.
  5. **Panic-freedom** of the whole fold (the totality statement).
- **Symbolic surface** (narrowest-type rule): `phase: u8 → SyncPhase`
  via index match (Idle/Offline/Syncing/Failed{u8→SyncErrorKind});
  `last_success` = `i32` seconds since 1970 (probe-proven minting
  pattern, 154 MB class) via `DateTime::from_timestamp(secs, 0)` —
  nanos concrete 0 (the fold compares whole instants; nanos widen the
  circuit for no law-value); `pending_ops: u32`; counters `u64`s from
  `u32` casts; `SidecarHealth`/`DeckFeatures` fields via `Default()`
  (not read by the fold — do not symbolize what the law cannot see).
- **Unwind**: the fold's `for` loop over 3 rows → `#[kani::unwind(4)]`.
- **Expected**: **proven** — integer-only, no String in the fold path;
  same cost class as the symbolic-DateTime probe. No stubs.
- **Containment**: none expected; if `SyncErrorKind`'s breadth (7
  variants × 4 phases) shows up as solver wall-time, narrow Failed's
  `last_error` to 3 representative kinds and document the shrink
  (the dominance law is insensitive to which error kinds exist).

### 3.4 H4 — `days::day_window` totality (`taskboard-domain/src/days.rs:41-56`)

- **Laws** (escalate `window_contains_now` / `window_is_exactly_24_hours`,
  `days.rs:224-287`, to exhaustive-within-bounds; the issue names
  panic-freedom/totality as the target):
  1. **Panic-freedom/totality** (primary): over the full symbolic
     surface below, `day_window` returns without overflow/assert
     failures (chrono `Days` arithmetic stays representable).
  2. `start <= now < end`.
  3. `end - start == 24h`.
- **Symbolic surface** (anchor-relative to keep date math inside
  chrono's range and the circuits 32-bit): `now` = `i32` seconds since
  2020-01-01 (±68 y) + nanos concrete 0; `day_start_time` = `u32` in
  `0..=86_399` via `NaiveTime::from_num_seconds_from_midnight_opt` +
  assume-Some; `offset` = `i32` in `±86_399` via
  `FixedOffset::east_opt` + assume-Some (same ranges the proptests
  use — keeping parity *is* the sampled→exhaustive escalation claim).
- **Unwind**: chrono date math is loop-light on these paths;
  start at `#[kani::unwind(8)]`, raise only on an unwinding-assertion
  failure, never speculatively.
- **Expected**: proven, but the widest solver wall of the integer
  class (symbolic 32-bit date arithmetic: ordinal packing, day-table
  math). Budget: if laws 2+3 exceed the memory budget, ship law 1
  alone (it is the issue's named statement) with the measurement, and
  record 2+3 as proptest-owned tail.
- **Containment**: no stubs (the chrono path *is* the verification
  target); the shrink ladder is law-scope first (1 vs 1+2+3), never
  input-range narrowing below the proptest ranges (that would
  silently void the escalation claim).

### 3.5 H5 — bounded `plan_pushes` grouping (`taskboard-domain/src/push.rs:344-402`)

- **Laws** (the issue's named invariant first): over a symbolic
  outbox of **3 `PendingOp`s** with **concrete id universes** (2 task
  ids, 1 each stack/label/board id; concrete minimal entity rows in
  `EntityTables` — `String` fields concrete always):
  1. **Delete subsumes**: if any op of entity E is `Delete*`, E's
     group materializes the `Delete*` and every other E-op is in its
     `subsumed` (`push.rs:10-14` normative table).
  2. **Partition**: every outbox `op_id` appears exactly once across
     all groups' `primary`+`subsumed` (linear-scan count).
  3. **Kind order**: `kind_rank` non-decreasing across the returned
     groups (adjacent pairs; `push.rs:393-402`).
  4. **Panic-freedom** of the whole planner call.
- **Symbolic surface**: per op, a `u8` tag → `LocalOp` variant via
  index match (17 variants; `AssignLabel/UnassignLabel` included),
  a `u8`/`bool` index pick between the universe's ids, `op_id` from a
  symbolic `u8` mapped to three fixed uuids (uniqueness by
  construction: tag mod 3), `queued_at` concrete. `UpdateAppData`
  attribution walks `entities.tasks`/`stacks` (2-entry BTreeMaps,
  concrete keys — map walks stay concrete-rooted).
- **Unwind**: outbox walk + grouping + sort over ≤3 items with
  B-Tree internals → `#[kani::unwind(16)]` starting bound; raise only
  on unwinding-assertion failures.
- **Expected**: proven, likely the largest integer-class formula
  (branchy enum matching + BTreeMap walks on symbolic keys). Budget
  ladder if needed: N=3→2 ops; universes 2→1; drop `UpdateAppData`
  from the tag space with the restriction documented in the harness
  comment (its lane is separately unit-covered, `push.rs:424-444`).
- **Containment**: entity rows/Strings stay concrete by construction
  (the grouping laws are id/queue-structure laws; entity *content*
  is not load-bearing). No `#[kani::stub]` planned.

### 3.6 Stage-2 execution order and acceptance mapping

1. Sync-crate compile smoke under kani (`cargo kani -p
   taskboard-sync-nextcloud --harness nonexistent` is enough to force
   the build; or author H1 first and run it) — de-risks H1's crate
   graph, measures the compile wall.
2. H1 → H3 (both expected quick wins; commit immediately after the
   first green harness — doctor-proofing, see §0 incidents).
3. H4, then H5 (budget-ladder decisions as they go).
4. H2 last, descending its ladder (§3.2).
5. `kani.yml` (§4) + a manual `workflow_dispatch` smoke run to
   validate the workflow end-to-end.
6. Report + handoff (§8). **≥3 proven** is expected from
   H1+H3+one of {H4, H5}; H2's Rung B (stubbed) can count toward the
   bar *with its stub documented* (the issue's success criteria
   explicitly allow stubbed-and-documented containment).

## 4. CI shape — `.github/workflows/kani.yml`

Mirrors `miri.yml` structurally (issue + dispatch: "scheduled +
manual dispatch, per-family jobs if walls demand; never in the PR
pipeline"). No `push`/`pull_request` triggers, so no PR check is
added and the branch-protection gate set is untouched.

```yaml
name: Kani
# Weekly out-of-band bounded-model-checking sweep over the pure core
# harnesses (#[cfg(kani)]). Kani compiles under its own pinned nightly
# and is never in the PR pipeline; scope + budget: the pilot plan
# (.artifacts/plans/2026-10-25-kani-pilot-plan.md) and issue #426.
on:
  schedule:
    - cron: "0 4 * * 1"   # Mondays 04:00 UTC (one hour behind Miri)
  workflow_dispatch:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always

jobs:
  kani-domain:
    name: cargo kani (taskboard-domain)
    runs-on: ubuntu-latest
    timeout-minutes: 120
    steps:
      - uses: actions/checkout@v7
      - name: Install system deps (keyring sync-secret-service links libdbus-1)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libdbus-1-dev pkg-config
      - name: Install Kani (version-pinned; Kani manages its own nightly)
        run: |
          cargo install kani-verifier --locked --version 0.68.0
          cargo kani setup
      - name: Verify domain harnesses (one per harness in the log, sequential)
        run: cargo kani -p taskboard-domain --output-format terse

  kani-sync-backoff:
    name: cargo kani (taskboard-sync-nextcloud — backoff)
    runs-on: ubuntu-latest
    timeout-minutes: 120
    steps:
      - uses: actions/checkout@v7
      - name: Install system deps (keyring sync-secret-service links libdbus-1)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libdbus-1-dev pkg-config
      - name: Install Kani (version-pinned; Kani manages its own nightly)
        run: |
          cargo install kani-verifier --locked --version 0.68.0
          cargo kani setup
      - name: Verify backoff harnesses
        run: cargo kani -p taskboard-sync-nextcloud --harness verify:: --output-format terse
```

- **Schedule**: Mondays 04:00 UTC — one hour after Miri (03:00) so
  the two heavy out-of-band sweeps never contend for runner credit
  windows; same `workflow_dispatch` escape hatch for the triggered
  pull-forward cases the ruling approved (#426: first sampled
  near-miss on an existing property, or next substantial touch of
  `days.rs` / reminder occurrence arithmetic).
- **Toolchain pin**: `--version 0.68.0` in the install step is the
  pin — Kani's own nightly comes from its bundle, `rust-toolchain.toml`
  needs no override (probe-verified, §1). A Kani bump is a deliberate
  one-line chore commit that re-runs the bound-saturation checks
  (§7) — release cadence wobble (8 months between 0.67→0.68) makes
  floating-risk pointless and pinning free.
- **Jobs**: one per crate family, mirroring `miri.yml`'s per-crate
  pattern; `--jobs` stays at its sequential default so a single
  harness's memory peak is bounded by the 16 GiB runner, not
  multiplied. The sync job filters `--harness verify::` so the job
  name stays honest (backoff is its only harness today; a second sync
  harness reopens the job split).
- **Artifact handling**: none — `miri.yml` precedent holds
  (failures triage via `gh run view --log-failed`; every
  counterexample's durable artifact is the concrete-playback
  regression test committed on the branch, §5, never a CI artifact).
  The report file (§8) is the pilot's record.
- **Cost**: public repo → Actions minutes free (issue #426's own
  analysis); the real cost stays harness authoring, as planned.

## 5. Concrete-playback regression-test plan

When any harness reports **FAILURE** with a counterexample:

1. Re-run that harness under the guard with playback enabled:
   `cargo kani --harness <name> --enable-concrete-playback inplace`
   (`print` for review-only). Kani injects a
   `concrete_playback_run()` test into the harness module carrying
   the counterexample's concrete values.
2. Move the values into a plain `#[test]` regression in the target
   module's `#[cfg(test)]` tests, named `repro_<law>_kani_cex`, with
   a comment linking the harness and the CI run. The playback test
   itself runs under plain `cargo test` on stable (no Kani) — verify
   that on first use and note the mechanism in the harness module's
   doc comment.
3. Triage per AGENTS §5: **a new test failing can be a production
   bug**. Fix production and keep the test green (expected for the
   domain-core arithmetic targets); only if the fix is genuinely
   out of pilot scope may the test stand as a known-failing
   documented bug — never delete or weaken it to green the suite.
4. The harness keeps proving after the fix (the counterexample
   disappears); the regression test guards the fix against
   regression forever after in the normal PR pipeline — that is the
   pilot's durable payoff path.

## 6. Memory budget (the gate a harness must pass to ship)

Local runs are guard-capped at 6 GiB and RSS-measured; the runner
envelope is 16 GiB. A harness ships in kani.yml only with a
**peak RSS ≤ 10 GiB** recorded here and in its module doc comment
(headroom for rustc + OS on the runner). Current table (updated by
stage 2):

| Harness | Peak RSS | Verify wall | Status |
|---|---|---|---|
| H1 backoff (probe) | small (unmeasured) | 0.40 s | proven at probe time |
| H3 aggregate_rows | expected ~0.2 GiB class | — | stage 2 |
| H4 day_window | expected 1–4 GiB class | — | stage 2 |
| H5 plan_pushes | expected 1–6 GiB class | — | stage 2 |
| H2 round-trip (full law) | **> 6 GiB, killed** | — | containment ladder |
| H2 round-trip (Rung B, stubbed) | expected ~0.3 GiB class | — | stage 2 |

Anything that exhausts the 6 GiB cap twice at a rung is recorded and
the ladder descends — no third blind retry, no cap-raising on this
30 GiB machine (the §0 OOM is why the cap exists).

## 7. Bound hygiene (the documented silent-failure mode)

Kani/CBMC silently **over-approximates** when a loop outgrows its
unwind bound — a harness can read "proven" while covering less than
claimed (report §C1, agent-maintainability). Countermeasure, per
harness, before its bound is declared final:

- Re-run once at **2× the declared unwind**; the result must be
  identical (same verdict, same checks count, same unreachable
  count). Record both numbers in the harness doc comment next to the
  `#[kani::unwind]` attribute (report requirement: "per-harness
  unwinding bounds documented next to each #[kani::unwind]").
- Never declare a bound from intuition; the doubling run is the
  evidence. If 2× changes the verdict, the bound was too low — fix,
  re-double, repeat.
- On any future Kani version bump or loop-structure change in a
  target module, the doubling check re-runs (cheap; it is in the
  harness docs as the bump checklist).

## 8. Termination criterion (what evidence ends the pilot, either way)

**Success exit** (closes #426 as completed):
- ≥ 3 harnesses report **proven** within documented bounds — H1 and
  H3 are near-certain from probe data; one of H4/H5 (or H2-Rung-B
  with its stub documented) supplies the third.
- Every shipped harness has: its law stated in the doc comment, its
  unwind bound + doubling-check numbers, its peak RSS in the §6
  table, and an empty-or-documented stub list.
- `kani.yml` exists with the scheduled + manual shape (§4) and has
  passed at least one manual `workflow_dispatch` run green (the cron
  itself can only be validated by waiting a week; the dispatch run
  exercises the same job graph).
- Any counterexample found on the way is converted per §5 and
  triaged as a bug per AGENTS §5.
- Pilot report under `.artifacts/reports/2026-10-25-kani-pilot-…`
  (probe table, harness outcomes, walls, bounds, the decision
  whether proptests stay sole-owner of any law), PR open +
  `gh pr checks` green → user merges.

**Honest-termination exit** (closes #426 with a written finding):
- Per-law termination: a law whose every containment rung exceeds the
  §6 budget (H2's full round-trip is the live candidate — the String
  wall is already measured) is recorded as *"proptests remain the
  right tool for this law"* with the measurement table. The pilot
  continues without it.
- Whole-pilot termination: only if **fewer than 3** harnesses can be
  proven at any legal bounds — not expected (probe evidence points
  the other way), but if it happens: no harness modules, no kani.yml
  (or kani.yml deleted), the report carries the full probe/budget
  evidence, and #426 closes on that finding. Either way the issue
  closes — the pilot was explicitly approved with both exits.

**Not an exit**: a harness "failing" because an unwind bound was too
low (§7 distinguishes this: unwinding-assertion failure ≠
counterexample); or CI-infra failures in the workflow file itself.

## 9. Risks

1. **String symbolism (measured, not hypothetical)**: H2's full law
   is over budget on every rung tried so far. Mitigation: §3.2
   ladder; the ≥3 bar does not depend on H2.
2. **Sync-crate compile under kani-rustc (untested)**: reqwest +
   hyper + rustls(`ring`) is a big graph; FFI is compile-time-only
   but the wall is unknown (domain graph: 12 s; sync graph: expect
   1–3 min). Mitigation: stage-2 step 1 is the compile smoke; if the
   graph breaks, H1 defers and H3+H4+H5 carry the bar (§3.6).
3. **chrono date-math memory in H4 (medium)**: symbolic 32-bit date
   arithmetic is the widest integer-class formula; probe B1 suggests
   it fits, but laws 2+3 may need the law-scope shrink (§3.4).
4. **BTreeMap internals on symbolic keys in H5 (medium)**: branchy
   bounded machinery; budget ladder shrinks N/universe first.
5. **Concurrent worktree sweeps (hit twice across chunks today)**:
   commit immediately after the first green harness; recreate per
   dispatch §1 if swept.
6. **OOM (hit once today, machine-killing)**: the §1 guard recipe is
   mandatory for every local kani run; the 6 GiB cap + no-retry rule
   in §6 keeps it that way.
7. **Kani version wobble**: `--version` pin in kani.yml; bumps are
   deliberate chores that re-run §7's doubling checks.
8. **False confidence from over-approximation**: addressed head-on
   by §7 (doubling checks) — the one silent-failure mode the
   suitability report flagged, budgeted rather than hoped away.
