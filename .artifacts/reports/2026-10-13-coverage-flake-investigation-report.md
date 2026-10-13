# Report: CI `Code Coverage` Corrupt-Profraw Flake — Investigation Evidence

- **Date**: 2026-10-13
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-13-coverage-ci-flake-fix-plan.md` + item plan
  `.artifacts/plans/2026-10-13-coverage-ci-flake-fix-investigation-plan.md`
  (branch `ci/coverage-flake-fix`).
- **Verdict**: root cause confirmed empirically; fix shipped (split
  coverage job + logged corrupt-profraw quarantine); H2 stale-cache
  eliminated; F1 (drop `%m` from the profile naming) tested and
  **rejected as a fix** — the counterfactual still corrupts.

## 1. Confirmed root cause

The coverage job instruments the whole workspace, so every binary the
tests spawn (daemons, one-shot CLI clients, REPL sessions) is itself an
instrumented process that writes a `.profraw`. The tests legitimately
kill such children mid-flight (`Child::drop` → SIGKILL in `cli_ipc.rs`,
lock contenders in `cli_lock.rs`, `timeout` expiry in `cli_tty.rs`).
A SIGKILL that lands inside an instrumented process's profile-write
window leaves a **partially written file whose header is invalid**;
`llvm-profdata merge -sparse -f` (cargo-llvm-cov's merge step) reads the
whole per-run profraw list and **aborts the entire merge on a single
invalid input**. Result: a fully green test suite fails the job on one
garbage file — exactly the observed signature (one corrupt
`taskboard-<pid>-<pool>_<n>.profraw` per failure, a different file each
time, tests 612/612 and 624/624 green).

### Evidence chain

1. **Naming decode (P2/P3)** — strace of an instrumented
   `cli_ipc`+`cli_lock` run (58 profraws): cargo-llvm-cov v0.9.1 sets
   `LLVM_PROFILE_FILE=<dir>/taskboard-%p-%16m.profraw` (bound =
   `available_parallelism`). The runtime expands `%m` to a per-binary
   pool value plus `_N` with **N = pid mod bound** (verified: every file
   in the capture matches `pid mod 16`; CI's 4-core runner showed
   `_0.._2` → `pid mod 4`). Files are per-process unique (pid in the
   name) — the dispatch plan's naive "two processes, one path" sharing
   is ruled out at this toolchain.
2. **Continuous mode (P2)** — strace shows the profile lifecycle:
   `openat(O_RDWR|O_CREAT)` **at process start** →
   `ftruncate(fd, 0)` → header/name-table write →
   `mmap(PROT_READ|PROT_WRITE, MAP_SHARED|MAP_NORESERVE)` of the counter
   region. `%m` puts the runtime in continuous mode: counters are
   written live via the mapping, which is crash-safe **once mapped** —
   12 daemons SIGKILLed mid-run in the capture all left *valid* files.
   The vulnerable moments are the unmapped windows: the startup write
   and the exit update.
3. **Multi-file merge aborts on one bad input** — reproduced exactly:
   one 24-byte-truncated profraw among 50 valid ones in
   `llvm-profdata merge -sparse -f` →
   `warning: … file header is corrupt` → `error: no profile can be
   merged` → exit 1.
4. **Deliberate trigger on the real binary** — spawn the instrumented
   `target/llvm-cov-target/debug/taskboard` and SIGKILL it at a random
   0–12 ms offset, 800 kills per round, 3 rounds: **6/11/9 invalid
   profraws per round** (~1 in 60). A follow-up round reproduced the
   CI error string byte-for-byte on files of 4 KB–155 KB (the large
   name table mid-write). `taskboard --version`'s whole run is ~17 ms,
   so a kill in the first ~12 ms sits inside the startup profile-write
   window; CI's short-lived lock contenders / rapidly killed children
   hit the same window (and the symmetric exit window).
5. **F1 counterfactual (drop `%m`, exit-write-only profiles)** — same
   kill storm, `LLVM_PROFILE_FILE=…%p.profraw`: **8 invalid files in
   ~640 kills**. Without `%m` the profile is written in one burst at
   exit, and a kill racing that burst tears it the same way. Conclusion:
   no naming scheme fixes the class — the write window cannot be
   eliminated while tests kill instrumented children.

### Hypotheses eliminated (with reason)

- **H2 stale/cached profraw entering the glob**: P4 probe — a junk
  `.profraw` planted in `target/llvm-cov-target` was **deleted by
  cargo-llvm-cov pre-run** and the merge went green (the dir is
  scrubbed). Independently: a cache-borne corrupt file would fail every
  run until cache eviction, but all three CI failures passed on re-run.
- **H3 version interaction**: CI floated cargo-llvm-cov **0.9.1** (=
  local) and nextest **0.9.148** (local 0.9.146) on 2026-10-13 — no
  relevant drift. Pinning still shipped (F4) to keep the evidence base
  stable and kill the float as an independent flake source.
- **H1a cross-process `%m` pool sharing**: ruled out by (1) — files are
  pid-unique; the pool id is per-binary, the slot is pid-derived.
- **F1 as a fix**: eliminated by the counterfactual (5).
- **Straggler writers racing the merge (H1c)**: not needed to explain
  the failure; not separately triggered. The quarantine step makes the
  merge robust to any torn file regardless of which window produced it.

## 2. The fix

`.github/workflows/ci.yml` coverage job split into three steps:

1. `cargo llvm-cov nextest … --no-report` — the instrumented test
   phase, unchanged test set.
2. `scripts/coverage-quarantine-profraw.sh target/llvm-cov-target` —
   one validation merge of the full profraw list (healthy runs stop
   here, paying one extra merge pass); on failure, every file
   llvm-profdata warns about is moved to `llvm-cov-target/quarantine/`
   with its name logged, and the merge of the remainder is retried
   once. A quarantined file holds only the torn tail of a killed
   process's counters — no usable coverage exists in it; the loss is
   visible in the job log. A still-failing merge fails the job.
3. `cargo llvm-cov report --lcov --output-path lcov.info` — the report
   phase over the surviving profiles.

Hard-rule compliance: no test deleted/weakened/unignored;
`fail_ci_if_error` untouched; same test set and same coverage
computation (healthy runs quarantine nothing — the local green run
validated all 755 profraws and produced the report with the
child-process surfaces `src/ipc/*` and `repl.rs` present). The
quarantine is not a bare retry: it is the §5-F5 component of a fix
whose mechanism was confirmed first, and the write window itself is
unfixable at our layer (the F1 counterfactual is the evidence).

Also shipped: `workflow_dispatch:` on `ci.yml` (validation method) and
pinned `cargo-llvm-cov@0.9.1,nextest@0.9.148` (F4).

## 3. Validation

- Local: full split flow green (624 tests, 755 profraws, report
  generated); quarantine script exercised against a directory with 6
  real corrupt files → all 6 quarantined, logged, retry merge green.
- CI: ≥5 consecutive green `Code Coverage` runs via
  `workflow_dispatch` from `ci/coverage-flake-fix` on the final config
  — run URLs in the watchlist verdict (backlog entry of 2026-10-13).

## 4. Artifact index (in /tmp during the investigation, not persisted)

- `/tmp/covrepro/strace.txt` — P2 strace of the instrumented IPC/lock
  suites (naming + continuous-mode evidence).
- `/tmp/killrepro/` — the SIGKILL trigger loop (`killloop.py`) and the
  corrupt-file captures (rounds with 6/11/9 invalid files; the no-%m
  counterfactual with 8).
- `/tmp/covrepro/mre*.log` — the P5 scratch-crate MRE (2000+ kills in
  the startup window and 2000+ in the exit window: no corruption with a
  toy binary — the window scales with the profile name table, hence the
  direct trigger on the real taskboard binary).
- `/tmp/covrepro/p4b.log` — P4 junk-file probe (cargo-llvm-cov scrubs
  the coverage dir pre-run).

The 2026-10-11 watchlist entry (`actor_wiremock` failing *as tests*
under a plain `cargo llvm-cov --workspace`, exit 101) is a different
symptom family — the confirmed merge-corruption mechanism does not
produce test failures — and stays on the watchlist unexplained.
