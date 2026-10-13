# [Plan] CI `Code Coverage` Job — Corrupt-profraw Flake: Investigation & Fix

- **Date**: 2026-10-13
- **Kind**: stage-1 investigation-and-fix plan (Prompt F1 of the
  dispatch plan) — no code in this commit. Stage 2 (Prompt F2)
  investigates per §3/§4, implements per §5, validates per §6,
  documents per §7.
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-13-coverage-ci-flake-fix-plan.md`
  (**§1 evidence and §2 hard rule are normative**); flaky-watchlist
  entries of 2026-10-11 and 2026-10-13 in
  `.artifacts/taskboard-potential-improvements-backlog.md`.
- **Naming note**: the dispatch plan itself occupies
  `2026-10-13-coverage-ci-flake-fix-plan.md`; this item plan uses the
  `-investigation-plan.md` suffix so both exist side by side on the
  fix branch (Prompt F2 finds it via the
  `*coverage-ci-flake-fix*-plan.md` glob).
- **Branch**: `ci/coverage-flake-fix` off `main` (@ `75da593`),
  worktree `../taskboard-WT-ci-coverage-flake-fix`.
- **Status**: READY for Prompt F2 (implementation).

## 1. Verified context (stage-1 findings — do not re-derive)

The dispatch plan's §1 evidence (four occurrences, shared
merge-corruption signature, one corrupt profraw per run, tests fully
green) stands as quoted. This section adds what stage 1 verified on
top, including one correction to the dispatch hypothesis that changes
where to look first.

### 1.1 The job, its command, and version float

- The coverage job (`.github/workflows/ci.yml` ~line 144) runs:
  `cargo llvm-cov nextest --all-features --workspace --no-tests=warn
  --lcov --output-path lcov.info`.
- Tools come from `taiki-e/install-action` with
  `tool: cargo-llvm-cov,nextest` — **unpinned**, so CI floats to the
  latest release on every run. Neither tool is a workspace dependency
  (nothing in `Cargo.lock` — verified), so the lockfile does not pin
  them. Locally verified versions: cargo-llvm-cov **0.9.1**,
  cargo-nextest **0.9.146** (2026-09-21), rustc 1.99.0 stable.
- `.github/workflows/ci.yml` has **no `workflow_dispatch` trigger**
  (only push-to-main / PR-to-main), unlike `miri.yml`,
  `mutation-testing.yml`, `release.yml`, and
  `nextcloud-integration.yml`, which all have one. The mandated
  validation method (dispatch-plan §2 acceptance: ≥5 consecutive
  green runs from the fix branch, no PR) is therefore **impossible
  until `workflow_dispatch:` is added** — see §6 step 0.

### 1.2 How profraw files are named and merged (cargo-llvm-cov v0.9.1 source, verified)

- `set_env` (v0.9.1 `src/main.rs`) builds:
  `LLVM_PROFILE_FILE = <llvm-cov-target-dir>/<workspace-name>-%p-%Nm.profraw`,
  where in nextest mode the `%m` bound N = `available_parallelism()`
  (e.g. `%4m` on ubuntu-latest; N must be 1–9). `%p` and `%m` are
  placeholders **expanded by the LLVM profile runtime inside each
  process**, not computed by cargo-llvm-cov. The name can be
  overridden wholesale via the `LLVM_PROFILE_FILE_NAME` env var
  (must end in `.profraw`) — this is a workflow-level fix lever (§5
  F1). The source carries the telling comment: *"TODO: remove %p
  (for nextest?) by default?"*.
- Merging (`merge_profraw`, v0.9.1 `src/report.rs`): the input list
  for `llvm-profdata merge -sparse -f` is a **glob of every
  `*.profraw` in the top level of the llvm-cov target dir** — not a
  pattern match. Consequences: (a) any profraw landing there under
  any name is merged (per-child unique filenames will not be silently
  dropped); (b) **one unreadable file aborts the whole merge** —
  exactly our symptom; (c) there is no scrubbing or sorting.
- `llvm-profdata` itself comes from the same rustup stable toolchain
  that instrumented the build (verified in the CI failure log:
  `~/.rustup/toolchains/stable-…/bin/llvm-profdata`), so the classic
  runtime/profdata version-mismatch corruption class is **ruled
  out**.
- Open micro-question (probe P4, §3): nothing in `merge_profraw`
  deletes old profraw files, and `Swatinem/rust-cache` restores the
  whole `target/` — whether a *previous run's* profraws can enter the
  merge glob must be probed, not assumed.

### 1.3 The corrupt filename, decoded — and what it corrects

Exact failing line (run `38399239683`, 2026-10-13):

```
warning: …/target/llvm-cov-target/taskboard-4778-5979293788477358020_2.profraw:
         invalid instrumentation profile data (file header is corrupt)
error: no profile can be merged
error: failed to merge profile data: … llvm-profdata merge -sparse -f
       …/taskboard-profraw-list -o …/taskboard.profdata (exit status: 1)
```

Against the v0.9.1 pattern `taskboard-%p-%4m.profraw`:

- `taskboard` — workspace name.
- `4778` — the `%p` expansion: **some** process's pid (not
  necessarily the corrupt file's writer).
- `5979293788477358020_2` — the `%4m` expansion. It is **not** the
  single digit 1–4 the bound suggests; the runtime's actual
  expansion (a large per-run-looking number plus an `_N` suffix) must
  be decoded against compiler-rt at the in-use rustc (probe P3).
  The dispatch plan called this the "run-id"; treat that reading as
  informal until decoded.

**Stage-1 correction to the dispatch hypothesis.** Because `%p` is
expanded per-process, a spawned child does **not** write its parent
test process's literal file — the naive "two processes, one path,
interleaved writes" reading of the working hypothesis is wrong for
this toolchain/version. The remaining collision classes that fit the
signature (exactly one corrupt file, different every run, tests
green) are:

1. **`%m` continuous-merge sharing**: `%m` exists precisely so that
   multiple processes resolving the *same* expanded path can share
   one file (file-lock-guarded counter merging). Any writer killed
   while holding that lock mid-merge-write can leave the header
   corrupt. This is where instrumented children (SIGKILLed cleanup,
   `timeout` expiry) meet the shared pool.
2. **A stale or cached corrupt file** entering the glob (§1.2
   micro-question).

Both are still "child processes + profile-file lifecycle" — the
dispatch plan's family stands; the first probe target moves.

### 1.4 The spawning surfaces (what creates instrumented children)

All verified by reading the tests on `main` @ `75da593`. Under the
coverage job the whole workspace is built with
`-Cinstrument-coverage`, so every spawned `taskboard` binary is an
instrumented process that writes a profraw on normal exit:

| Surface | What it spawns | Env handling |
|---|---|---|
| `crates/taskboard-app/tests/common/mod.rs:386` `binary_command()` | `CARGO_BIN_EXE_taskboard` (daemon + one-shot clients) | scrubs **only `TASKBOARD_*`** vars — `LLVM_PROFILE_FILE` inherits verbatim |
| `tests/cli_tty.rs` (pty REPL journeys, ~7 tests) | `timeout 30 script -qec "<binary> …" /dev/null` | bare `Command::new("timeout")` — full env inheritance |
| `tests/cli_ipc.rs` | real `taskboard daemon` + one-shot CLI clients over the socket | via `binary_command()`; `Child` drop = `kill()` (**SIGKILL**) + wait |
| `tests/cli_lock.rs` | daemon + second-boot + `kill(1)` (**SIGTERM**) | via `binary_command()` |
| `tests/it_docker.rs` | daemon + clients | docker-tier-gated → among the 25 skipped in CI |

Notes for the investigation:

- The pty tests' skip gate is `script --help` availability;
  ubuntu-latest has util-linux `script`, so **they run in the CI
  coverage job**. Their `timeout 30` wrapper bounds a hung child with
  SIGTERM (default disposition — no atexit, no profile flush; though
  the taskboard daemon's own SIGTERM handler drains and exits
  normally, which does flush).
- Kill paths that can interrupt a writer: SIGKILL in
  `cli_ipc.rs`'s `Child::drop`; SIGTERM via `timeout` expiry and
  `cli_lock.rs`'s `kill(1)`.
- Signal-adjacent context: the instance-lock tests *require* the
  daemon to die on SIGTERM (lock released by the OS on death), and
  ADR 0008 defines the daemon's graceful SIGTERM drain — cleanup
  mechanics may change (§5 F3) but assertions may not (hard rule).

### 1.5 Upstream known-issue landscape

- The exact error ("invalid instrumentation profile data (file
  header is corrupt)" → "no profile can been merged" → merge exit 1)
  is a recognized cargo-llvm-cov flake family in public projects
  (e.g. github.com/suzuke/agend-terminal#1452 — all tests pass, merge
  flake repeats; users.rust-lang.org/t/…/105550; RSigma release notes
  describe scrubbing unreadable profraw before merge as their fix).
  Common mitigations in the wild: unique per-test profraw naming,
  scrubbing unreadable files pre-merge, `--test-threads=1`. None of
  these may be adopted here without the mechanism-first discipline of
  §2/§4/§5.
- `%m` continuous mode is LLVM's facility for same-file
  multi-process profile sharing; its interaction with abnormally
  terminated writers is the classic corrupt-header source. Relevant
  upstream reading for stage 2: compiler-rt `InstrProfiling*` source
  at the in-use rustc (decode `%m`), and the cargo-llvm-cov /
  nextest changelogs between the version CI floated and 0.9.1.

## 2. Scope, hard rule, non-goals (inherited from dispatch plan §2)

**Hard rule (non-negotiable, verbatim intent)**: every change is an
environment/infrastructure fix — **no test is deleted, weakened, or
unignored to make the job green**, and the coverage gate is never
downgraded (no `fail_ci_if_error` games, no swallowing the merge
error, no dropping the instrumented run). A bounded, *logged* retry
of the merge step is acceptable **only** as a component of a fix
whose root cause is understood and otherwise addressed — never as
the whole fix.

**Additional stage-1 guard, adjacent to the hard rule**: the job must
keep measuring the same thing. Child-process coverage (the daemon/CLI
paths exercised through spawned binaries in `cli_ipc`/`cli_tty`/
`cli_lock`) is real coverage today; a fix that silently stops
children writing (e.g. `env_clear`) lowers reported coverage and is
**rejected by design** (§5). If the honest fix were to forgo child
coverage, that is a user decision, not an implementation one.

**In scope**: `.github/workflows/ci.yml`, test-spawning plumbing
(`crates/taskboard-app/tests/` helpers only — never assertions),
cargo-llvm-cov/nextest configuration and version pinning,
toolchain-level changes. **Non-goals** per dispatch §2 (perf jobs,
weekly Miri/mutants unless the same plumbing fix provably applies,
occurrence #1 unless the root cause explains it — see §4 H5).

**Acceptance** (dispatch §2): root cause identified and stated;
fix addresses the mechanism; ≥5 consecutive green `Code Coverage`
runs via `workflow_dispatch` from the branch; watchlist verdict;
docs amended; backlog entry ✅ DONE.

## 3. Root-cause method — reproduction recipe

The flake's base rate is low (3 CI failures in ~2 days of frequent
runs), so the method is staged: observe first, amplify second,
falsify fast with a scratch crate. Investigation artifacts (logs,
straces, profraw captures) go to `/tmp` while working and into
`.artifacts/reports/2026-10-13-coverage-flake-investigation-report.md`
at the end (AGENTS §7).

**P0 — first step: reproduce against the real workload.** Build the
instrumented workspace once, then loop the *exact* CI command,
capturing the profraw dir on every failure:

```sh
cd /home/viktoria/src/taskboard-WT-ci-coverage-flake-fix
export SQLX_OFFLINE=true
mkdir -p /tmp/covrepro
for i in $(seq 1 15); do
  rm -rf target/llvm-cov-target
  if ! cargo llvm-cov nextest --all-features --workspace --no-tests=warn \
         --lcov --output-path /tmp/covrepro/lcov-$i.info \
         > /tmp/covrepro/run-$i.log 2>&1; then
    cp -a target/llvm-cov-target /tmp/covrepro/artifacts-$i
    echo "RUN $i FAILED — artifacts saved"
  fi
done
```

On failure: identify the corrupt file from the log (the `warning:`
line), confirm with a single-file
`llvm-profdata merge -sparse file.profraw -o /tmp/x.profdata`, hexdump
its header, and correlate the embedded pid with a process (record
`ps -ef` snapshots or nextest's per-test process tree during runs).

**P1 — amplified subset loop.** The child-spawning binaries only,
higher repetition:

```sh
for i in $(seq 1 50); do
  rm -rf target/llvm-cov-target
  cargo llvm-cov nextest -p taskboard-app \
    --test cli_tty --test cli_ipc --test cli_lock --test config \
    --lcov --output-path /tmp/covrepro/sub-$i.info \
    > /tmp/covrepro/sub-$i.log 2>&1 || cp -a target/llvm-cov-target /tmp/covrepro/subart-$i
done
```

**P2 — direct observation (the evidence the dispatch plan calls
"LLVM_PROFILE_FILE tracing / strace-level").** One instrumented run
of the IPC suite with syscall tracing on the profile paths:

```sh
rm -rf target/llvm-cov-target
strace -f -e trace=openat,ftruncate,unlink,rename,mmap -s 120 \
  -o /tmp/covrepro/strace.txt \
  cargo llvm-cov nextest -p taskboard-app --test cli_ipc --test cli_lock
ls -la target/llvm-cov-target/*.profraw | head -40   # name shape + count
```

Read out of the strace: which pid opens which expanded path, with
which flags (`O_TRUNC`? `flock`? mmap for continuous mode), whether
killed children die between open and final write, and whether
multiple processes touch the *same* path. The profraw **count after
one green run** also decodes the naming: per-process-unique files
(hundreds, one per test process + child) vs pool-shared files
(a handful) decides H1's shape immediately.

**P3 — decode the `%m` expansion.** With the names from P2 and the
corrupt name from §1.3 (`taskboard-4778-5979293788477358020_2`),
read compiler-rt's path-substitution code at the in-use rustc
(`rustc --print sysroot`/lib/rustlib/src → `InstrProfiling` runtime
sources) and determine whether the bignum is a per-run shared pool id
(all processes share files) or per-process. This is a desk step, not
a build step.

**P4 — stale-file probe (H2).** Before a normally-green subset run:

```sh
head -c 4096 /dev/urandom > target/llvm-cov-target/zz-junk.profraw
```

If the merge fails on `zz-junk.profraw`, prior files enter the glob
→ the rust-cache-restores-stale-profraws vector is live and must be
checked (does the job's `Swatinem/rust-cache` restore
`target/llvm-cov-target`? inspect the cache action's default
exclusions; add a CI debug `ls` step or a workflow-level clean if
so). Also answers whether cargo-llvm-cov cleans the dir pre-run.

**P5 — MRE falsifier (fast, no taskboard code).** A scratch crate in
/tmp (outside the repo): a bin + a test that spawns the bin as a
child and SIGKILLs it at randomized microsecond offsets; run under
`cargo llvm-cov nextest`. If it corrupts, the mechanism is confirmed
at the LLVM-runtime level (H1) without 20-minute taskboard cycles,
and the fix levers in §5 can be evaluated against the MRE first.

**Evidence bar**: a hypothesis is *confirmed* only when a deliberate
trigger reproduces the exact signature (tests green + one corrupt
profraw at merge) **and** P2/P3-grade evidence names the writer
processes; a hypothesis is *eliminated* only with a recorded reason.
Re-run hygiene: never re-run to make a failure disappear — every
reproduction is an artifact.

## 4. Hypothesis ladder (test in this order)

- **H1 (primary — refined dispatch hypothesis): `%m`
  continuous-merge sharing + abnormally terminated instrumented
  children.**
  - H1a: a SIGKILLed child (`cli_ipc.rs` `Child::drop`, `timeout`
    hard-kill paths) dies holding/inside a %m shared-file
    merge-write → truncated header. Predicts: P5 reproduces;
    P2 shows same-path multi-process writers with locking.
  - H1b: `timeout` expiry SIGTERMs a child that lacks the daemon's
    graceful handler → no atexit → *no file at all* (probably
    produces absence, not corruption — eliminate quickly if so).
  - H1c: a child outliving the test phase (daemon leak past a failed
    assertion before `Child` wraps it; a `timeout 30` window
    stretching past nextest's finish) writes — or is killed —
    *during the merge window*. Predicts: corrupt file mtime ≈ merge
    timestamp; check with `stat` in P0 artifacts.
- **H2: stale/cached profraw corruption.** `Swatinem/rust-cache`
  restores `target/llvm-cov-target` from a previous run; a stale
  (or cache-corrupted) file enters the glob. Probe P4 decides in one
  run. If confirmed, the fix is workflow-level hygiene (clean the
  dir or exclude it from the cache), independent of H1 — both may
  be true; fix what the evidence shows.
- **H3: version interaction.** CI floats cargo-llvm-cov/nextest to
  latest per run (§1.1); a regression (or a fix not yet floated)
  could explain onset and the 2026-10-12 timing. Actions: pin the
  locally-verified pair (`cargo-llvm-cov@0.9.1`, `nextest@0.9.146`)
  as a control; read both changelogs between the floated versions
  and the pin for LLVM_PROFILE_FILE / merge changes. Version pinning
  is worth shipping regardless of the verdict (§5 F4).
- **H4: LLVM runtime exit-write window.** Even with per-process
  unique files, a SIGKILL landing inside the *exit-time* profile
  write of a normally-exiting process truncates the file. Tiny
  window, but it survives H1's elimination and would push toward
  F5-component territory. P2 decides: corrupt file whose pid never
  had a kill directed at it.
- **H5: the 2026-10-11 local occurrence** (`actor_wiremock` failed
  *as tests*, exit 101, under a plain non-nextest
  `cargo llvm-cov --workspace`) — different symptom family. Per the
  dispatch non-goals: fold in only if the confirmed mechanism
  explains it; otherwise record the verdict "separate family,
  unexplained" in that entry's cross-reference (§7) and leave it on
  the watchlist.

## 5. Fix ladder (once the mechanism is confirmed)

Take the **lowest rung that addresses the confirmed mechanism**;
record in the report why each higher rung was not needed.

- **F1 — workflow-level profile naming (no test changes).** Set
  `LLVM_PROFILE_FILE_NAME: taskboard-%p.profraw` (env on the
  coverage job) so every process writes its own pid-unique file with
  **no `%m`** — the continuous-merge sharing class disappears by
  construction. Verified safe for measurement: the merge globs all
  `*.profraw` (§1.2), so child files still count. Costs: more files
  (hundreds) → slower merge (measure; `-sparse` handles this fine at
  this workspace size); pid-reuse within a run → overwrite (lost
  lines, not corruption — rare, acceptable, note it). This is the
  expected rung if H1 confirms.
- **F2 — test-plumbing env isolation (only if children must be
  isolated beyond what F1 achieves).** Extend `binary_command()` and
  the `cli_tty.rs` `timeout`/`script` wrapper: when the inherited
  environment carries `LLVM_PROFILE_FILE`, give each spawned child
  its own file (e.g. `LLVM_PROFILE_FILE=<target-dir>/child-%p.profraw`)
  so children keep contributing coverage with unique files. Ships
  with a `docs/testing_strategy.org` note (§7). Rejected variant:
  scrubbing `LLVM_PROFILE_FILE` entirely (`env_remove`) — silently
  drops child-process coverage, violating the §2 measurement guard.
- **F3 — test-plumbing kill policy (only if H1a/H1c confirmed).**
  `Child::drop` in the binary-driving suites: SIGTERM → bounded
  wait → SIGKILL, so the daemon drains and exits normally (ADR 0008
  path), flushing a complete profile before the merge. Never touch
  the kill-semantics *assertions* (the lock test's SIGTERM contract
  stays); cleanup mechanics only. If a daemon can outlive the suite
  (H1c), also ensure the wrap-before-spawn ordering in the affected
  tests.
- **F4 — version pinning (orthogonal, ship regardless).**
  `tool: cargo-llvm-cov@<pinned>,nextest@<pinned>` in
  `taiki-e/install-action` — kills the per-run float (a second,
  independent flake source and a reproducibility hazard: the
  investigation's evidence is only valid for the pinned pair, per
  the MSRV no-float policy precedent, ADR 0003 amendment). If H3
  was the verdict, the pin *is* the fix — then also name the
  upstream tracking issue in the watchlist so the pin gets revisited.
- **F5 — last resort, component-only: bounded logged merge retry.**
  Only inside a fix whose mechanism is otherwise addressed (the
  dispatch hard rule) — e.g. H4's exit-write window proves real and
  unfixable at our layer: pre-merge, validate each profraw with a
  cheap single-file `llvm-profdata merge`, quarantine failures with
  a logged name+pid, retry the merge once — never a bare retry, and
  the log line must name what was dropped (it is a coverage loss
  and must be visible).
- **Rejected by design** (state in the report if anyone reaches for
  them): dropping child coverage via `env_clear`; swallowing the
  merge error or `fail_ci_if_error` games; serializing tests
  (`--test-threads=1`) or otherwise changing the job's meaning;
  deleting/skipping the flaky-prone suites.

## 6. Validation (stage 2)

- **Step 0 (prerequisite, separate standalone commit):** add
  `workflow_dispatch:` to `ci.yml`'s `on:` block — same shape as
  `miri.yml`'s (which WI-C used for exactly this iteration style).
  Additive: push/PR behavior unchanged. Without this, the mandated
  ≥5-runs evidence cannot exist on a branch without opening a PR
  (which stage 2 is forbidden to do).
- **Step 1:** push the branch (explicitly authorized for this item,
  dispatch §4 Prompt F2; never `main`, no PR), then drive ≥5 runs:
  `gh workflow run ci.yml --ref ci/coverage-flake-fix` +
  `gh run watch`. **Any failure in the five is evidence** — capture
  the log and diagnose before touching anything; it may be the flake
  itself reasserting (then the fix is wrong or incomplete), not
  noise to re-run away.
- **Step 2 (measurement guard):** spot-check that coverage did not
  silently shrink: compare `lcov.info` summary lines (per-file hit
  counts for the daemon/IPC paths exercised only via spawned
  binaries — `crates/taskboard-app/src/ipc/*`, `repl.rs`) between a
  `main` run's artifact and the fix branch's. Local one-off, not a
  CI gate.
- **Step 3:** job duration sanity (profraw count ↑ from F1 → merge
  time; the job was ~2–4 min total). No hard threshold; flag if the
  merge step roughly doubles.
- **Step 4:** local quality gates (AGENTS §10) for touched surfaces
  — `cargo fmt --check`, `clippy -D warnings`, `nextest`,
  doc-tests, `cargo deny`, `llvm-cov` locally green with the new
  naming.

## 7. Documentation & bookkeeping (stage 2 final commits)

- **Watchlist** (`.artifacts/taskboard-potential-improvements-backlog.md`):
  2026-10-13 entry → verdict paragraph (confirmed root cause, the
  fix, run-id evidence links); per dispatch §5, the 2026-10-11 entry
  gets a cross-reference only — "linked" or "investigated, separate
  family" — either way stated.
- **`docs/testing_strategy.org`**: only if test plumbing changed
  (F2/F3) — a short note in §13 (*Coverage & Mutation Testing*)
  stating the child-process profile-isolation convention, phrased as
  §10's precedent does: an *environment gate on the instrumented
  build*, never a change to what a test asserts. If only the
  workflow changed (F1/F4), no §13 edit — `ci.yml`'s house style of
  reasoning comments carries the explanation.
- **Investigation report**: `.artifacts/reports/2026-10-13-coverage-flake-investigation-report.md`
  (AGENTS §7) — evidence, probes run, hypotheses eliminated with
  reasons, the decode of the `%m` expansion, artifacts index.
- **Backlog bookkeeping**: mark the watchlist escalation resolved
  when the fix PR merges (dispatch §5); this plan stays as the
  record of intent.

## 8. Risks

- **Reproduction may need patience**: base rate is low and CI-only
  data is thin; lean on P5 (MRE) and P2 (strace) rather than waiting
  for a natural CI hit. If 15 full loops + 50 subset loops produce
  nothing, that is itself evidence — shift weight to H2/H3, which
  need no reproduction to act on (P4 is one run; pinning is cheap).
- **Measurement drift**: any rung that stops children writing (or
  loses their files) silently lowers coverage — the §6 step-2
  spot-check guards; if the honest verdict is "children cannot be
  covered soundly", that goes to the user, not into the diff.
- **`LLVM_PROFILE_FILE_NAME` availability** depends on the floated
  cargo-llvm-cov version; verified present at 0.9.1 — land the F4
  pin (or verify CI's current version) before relying on it.
- **Kill-policy changes (F3) can disturb timing-sensitive tests**
  (lock acquisition windows, IPC `wait_until` deadlines) — review
  each touched test's contract; the hard rule covers assertions,
  but a changed *timing* contract is still a behavioral change to
  defend in the report.
- **Parallel CI-touching work** (WI-C landed on `main` today; the
  connect-flow implementation dispatch is queued) — rebase churn;
  the dispatch plan §3 already serializes this item against other
  CI work, and rebase-merge hygiene (AGENTS §8) applies to every
  commit.
- **If H3 is the verdict**, the fix is a pin that must be revisited
  when upstream fixes the real bug — the watchlist verdict must name
  the upstream issue so the debt is findable.

## 9. Suggested stage-2 commit sequence (each standalone, rebase-merge hygiene)

1. `ci(workflows): add workflow_dispatch trigger to ci.yml` — §6
   step 0, land first so validation can start.
2. `ci(workflows): pin cargo-llvm-cov and nextest versions in the coverage job`
   — F4 (or fold into the fix commit if the pin *is* the fix).
3. The fix commit(s) per the confirmed rung (F1/F2/F3/F5-shaped
   subject naming the mechanism).
4. `docs(report): coverage CI flake investigation evidence` — the
   §7 report.
5. `docs(backlog): coverage-flake watchlist verdict` (+ the
   testing_strategy amendment if plumbing changed).
