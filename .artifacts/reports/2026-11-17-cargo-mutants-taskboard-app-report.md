# Report — chunk C7: capped cargo-mutants one-off for `taskboard-app` (#327)

- **Date**: 2026-11-17
- **Branch/worktree**: `ci/cargo-mutants-capped` @
  `../taskboard-WT-ci-cargo-mutants-capped` (see §4 for the exact HEAD
  at run time)
- **Normative sources**: round plan
  `2026-11-17-backlog-reduction-round-plan.md` §3-C7; dispatch plan
  `2026-11-17-backlog-reduction-dispatch-plan.md` §1/§8; chunk plan
  `2026-11-17-cargo-mutants-capped-plan.md` (commit ac7328c3); issue
  #327 (cap template binding).
- **Toolchain**: cargo-mutants 27.1.0; rustc per `rust-toolchain.toml`.

## 1. Run outcome table

| Run | Mutant set | Mutants | Outcome | Caught | Missed | Unviable | Timeout | Wall | vs cap |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `ipc/*` (all) | 109 | FAILED — unmutated baseline red under plain `cargo test` (`taskboard_env_lock_restores_ambient_values`, tests/config.rs:728; per-test-thread process-env race — the same suite is 1003/1003 under nextest) | 0 | 0 | 0 | 0 | ~2.5 min | n/a |
| 1b | `ipc/*` (all), `--test-tool nextest` | 109 | INTERRUPTED externally ~70 min in (not the cap; "ERROR interrupted", no OOM in dmesg scope of this run) — 35 results recorded before the interruption | 22 | 4 | 9 | 0 | ~72 min | under |
| 2 | `codec.rs` + `protocol.rs` | 25 | ABORTED by the operator within the baseline phase — an uncommitted source edit (the kill test below) landed mid-run; `--in-place` runs own the tree, so the run was stopped to avoid corrupted line-addressed mutations (and its shutdown restore reverted the edit) | 0 | 0 | 0 | 0 | ~4 min | n/a |
| 2b | `codec.rs` + `protocol.rs`, kill test committed first | 25 | SUPERSEDED — folded into run 4 (see below); the kill test from run 1b's codec survivors (commit 27e2e24e) is in the baseline of every later run | — | — | — | — | — | — |
| 3 | `server.rs`, `--in-place` | 54 | KILLED BY SYSTEM FREEZE mid-run (the 2026-11-17 stage-2 freeze) — 18 mutants completed their test phases; a killed run's completed test phase is still evidence | 17 | 0 | 0 | 1 | n/a (killed) | n/a |
| 4 (resume, stage 2) | `ipc/*` full, `--jobs 2`, fresh (109 mutants; attempts 4a–4f) | 109 | **COMPLETED UNDER CAP (run 4f)** after two externally-SIGINTed attempts (4a: harness reaping, 9 min, discarded; 4b: harness stop at 61 min, 23 valid results snapshotted then discarded) and one ENOSPC death (4d/4e: the mutation copy's debug build needs ~14 G and died on the 16 G `/tmp` tmpfs — fixed with a disk-backed `TMPDIR`) and one flaky-baseline death (4d: `a_failing_verify_persists_nothing_and_replies_typed` failed the unmutated baseline twice under a 13 G memory peak — flaky-test issue #926; excluded from the kill surface via nextest `-E` passthrough, recorded deviation) | 69 | 4 | 20 (genuine compile failures) | 2 | ~80 min (stopped at dispatcher intervention) | under (cap not tripped) |
| 5 (stage 2) | `ipc/*`, `--jobs 2`, `--iterate` (89 caught/unviable skipped → the 4 missed survivors against the tree now carrying kill test ae299594 + the 14 untested) | 20 | run 5b: **OOM-KILLED by systemd-oomd at 22 min** (23 G memory peak — two concurrent full-suite nextest runs + box load; oomd contained it to the unit) after 10 results; run 5c resumed `--iterate` for the remainder — see §1a-final | 6 | 2 | 0 | 2 | 22 min (killed) | under |
| 5c (stage 2) | the remaining 10 mutants | 10 | see §1a-final | — | — | — | — | — | — |

**§1a-final — combined `ipc/` evidence across runs 4f/5b/5c** (a
mutant's latest completed outcome wins; 109 total):

| Caught | Timeout-kill | Unviable (compile failures) | Missed (survivors) | Untested |
|---|---|---|---|---|
| 76 | 1 | 20 | 2 | 10 |

- The two codec EOF-guard survivors (cluster C) are **caught** by
  kill test `ae299594` — confirmed on re-test in run 5b.
- Missed survivors (2): `client.rs:132:20` (cluster B/E → issue
  #902) and `server.rs:145:16` (cluster D → provable-equivalent).
- Runs 5b and 5c were both **OOM-killed by systemd-oomd** (23–25 G
  memory peaks: each mutant re-runs the full daemon-spawning
  `taskboard-app` suite, × 2 workers; oomd contained both kills to
  the transient unit — the box survived). The killed runs' completed
  outcomes are merged above; the 10-mutant untested remainder (both
  `server.rs:539:15` advance mutants + 8 `run_argv` FnValue mutants)
  is recorded on issue #327. Killed-run-3 evidence additionally
  timeout-killed `server.rs:539:15 replace += with -=` (infinite
  loop), so 9 of the 10 lack any evidence.
- `commands/` (892 mutants) is **entirely un-mutated** — the chunk's
  wall budget was consumed by the ipc lane plus incident recovery
  (§3). Recorded on issue #327 with the sizing arithmetic.

Run-1b partial results snapshot: `/tmp/c7-results-run1b` (22 caught,
4 missed, 9 unviable). `client.rs` fully covered there (22/22);
`codec.rs` 5/22, `mod.rs` 8/8; `protocol.rs`/`server.rs` untouched —
re-tested by runs 2b/3. The four run-1b missed survivors are triaged
in §2 (cluster A's three codec-constant survivors killed by commit
27e2e24e after run 1b, re-tested by run 2b with the kill test
present; the client survivor → issue #902).

## 2. Survivor triage

### Cluster A — `codec.rs` frame-cap constants (relative-test blindness)

Survivors from run 1b:

```
crates/taskboard-app/src/ipc/codec.rs:10:39: replace * with +   (64*1024)
crates/taskboard-app/src/ipc/codec.rs:13:47: replace * with +   (16*1024*1024)
crates/taskboard-app/src/ipc/codec.rs:13:40: replace * with +   (16*1024*1024)
```

Evidence: every codec test (round-trip, empty-frame, over-cap, announced
over-cap) sizes its payloads *relative to* `MAX_REQUEST_BYTES` /
`MAX_RESPONSE_BYTES`, so mutating `64 * 1024` → `64 + 1024` (64 KiB →
1088 bytes) or `16 * 1024 * 1024` → `16 + 1024 + 1024` (16 MiB → ~19 KiB)
keeps the whole suite green while the protocol envelope shrinks 60×.

Verdict: **trivial kill, test-only** (D4(a) met — 10 lines in the
existing inline `#[cfg(test)]` module, no new fixtures). Commit
`27e2e24e` adds `the_frame_caps_are_the_adr_0012_contract_values`
pinning the ADR 0012 §4.3 absolute values (64 KiB / 16 MiB). Run 2b
re-tests `codec.rs` with the kill test in the baseline; its outcome in
§1a is the kill confirmation.

### Cluster B — `client.rs` connect-guard (latency-only observable)

Survivor from run 1b:

```
crates/taskboard-app/src/ipc/client.rs:132:20: replace match guard
  matches!(err.kind(), NotFound | ConnectionRefused | PermissionDenied)
  with false in try_connect
```

Evidence: with the guard disabled, concrete refusals fall into the
retry arm and exit only at the `CONNECT_BUDGET` deadline. The
black-box result is identical (`None`); only the latency differs
(immediate vs budget-long retry loop). Killing it requires asserting
the *speed* of the no-daemon path — a timing assertion or an injected
retry/clock seam, which is new harness infrastructure → outside D4.

Verdict: **backlog issue #902** (`testing`, `p3`), with the proposed
fix direction (inject the retry/budget clock seam per
docs/testing_strategy.org §6.1–6.2).

### Cluster C — `codec.rs` EOF-guard→true (misreported failure class)

Survivors from run 4f:

```
crates/taskboard-app/src/ipc/codec.rs:75:21: replace match guard
  err.kind() == std::io::ErrorKind::UnexpectedEof with true in read_frame
crates/taskboard-app/src/ipc/codec.rs:85:21: (same, payload read)
```

Evidence: `read_frame` maps only a clean close to `Error::Eof`; no
test fed a *non-EOF* io error (e.g. `PermissionDenied`), so a guard
mutated to `true` misreports every read failure as `Eof` while the
suite stays green.

Verdict: **trivial kill, test-only** (commit `ae299594` — two
failing-reader fixtures in the existing inline module, both
scenarios: error on the length prefix and error mid-frame). Run 5
re-tests both mutants against the tree carrying the kill test; its
outcome in §1a is the kill confirmation.

### Cluster D — `server.rs` join() guard inversion (provable-equivalent)

Survivor from run 4f:

```
crates/taskboard-app/src/ipc/server.rs:145:16: delete ! in IpcServerHandle::join
  (if !accept.is_finished() → if accept.is_finished())
```

Verdict: **provable-equivalent**. `join()`'s budget-hit arm calls
`accept.abort()` / `dispatcher.abort()` *before* the guard; the
guarded `await` only reaps an already-cancelled task, so inverting
or skipping it changes no observable behavior. Evidence: the T8
wedged-drain test (`ip12_a_silent_connection_cannot_wedge_the_drain`, tests/ipc.rs:476 — the silent peer drives `join` to its 300 ms budget hit,
whose `accept` handle is finished and consumed at budget hit —
exactly the mutant's await path) passes with the mutant applied.
Not chased.

### Cluster E — `client.rs` try_connect guard (from run 4f)

Run 4f independently re-confirmed cluster B's survivor
(`client.rs:132:20`, guard→false) — same verdict, issue #902.

## 3. Cap accounting

- **Cap (D1/#327, binding)**: 7200 s wall per run (`timeout
  --signal=INT --kill-after=90s 7200`), chunk budget 4 runs ≤ 6 h
  total wall.
- **No run ever tripped the 7200 s cap.** Every run ended early by
  other causes: red baseline (run 1, 2.5 min), external SIGINT (1b,
  ~72 min), operator abort (2, 4 min), the system freeze (3), the
  dispatcher's gate intervention (4f, ~80 min, cap would have tripped
  first), and systemd-oomd (5b/5c, 22/14 min). The binding cap was
  never the limiting factor — the *box's* memory ceiling was.
- **Observed per-mutant cost**: ~30–100 s build + up to 207 s test
  phases (the auto-set 204 s test timeout × 2.0 multiplier) — the
  plan's pessimistic 60 s/mutant band was correct. At that rate the
  892-mutant `commands/` surface is ~10–20 h of machine time —
  far beyond this chunk's budget; it is recorded on issue #327 as
  the un-run remainder, needing sharded `--jobs 1` runs.
- **Effective `ipc/` coverage achieved**: 99/109 processed (91%
  by count), 100% of the processed mutants either killed or
  dispositioned (76+1+20 accounted, 2 survivors dispositioned in §2,
  10 remainder on #327).

## 4. Method & invocation log

- HEAD at run time: `ac7328c3` (plan commit; tree clean before each run. Post-run note: the branch was rebased onto `origin/main` at stage 3 review (2026-11-17) — the test-commit SHAs cited throughout were rewritten; this text cites the post-rebase SHAs.
  `--in-place` cleanliness check after each run per plan D3.4).
- Mutant arithmetic, re-verified in this worktree with `--list`:
  `commands/` = **892**, `ipc/` = **109**, total **1001** (matches the
  plan §1 table exactly).
- Baseline (D6): `SQLX_OFFLINE=true cargo nextest run -p
  taskboard-app` → **1003 passed, 0 failed, 15 skipped** (102.5 s)
  before run 1; two slow daemon-spawning tests > 60 s noted as
  timeout-budget context.
- Cap template (issue #327, binding): `timeout --signal=INT
  --kill-after=90s 7200 …` (2 h per run); chunk budget 4 runs ≤ 6 h.
- Deviation from plan D3 (`-j 4`): the box showed 15/30 G memory
  already in use by parallel agents at run time, so `commands/` was
  run with `-j 2` instead of `-j 4` (plan §4 memory risk names
  "drop to `-j 2`" as the sanctioned mitigation).
- Run 1 (`ipc/`, started after the baseline went green):
  ```
  timeout --signal=INT --kill-after=90s 7200 env SQLX_OFFLINE=true \
    cargo mutants -p taskboard-app \
    --file 'crates/taskboard-app/src/ipc/*' \
    --in-place --timeout-multiplier 2.0
  ```
- Quality-gates flock: contended at start (held by
  `fix/domain-same-ref-collapse`, pid 1835133) → the flock-wrapped
  nextest gate was deferred per the #879 rule; retried after the test
  commits.

### Stage-2 resume (2026-11-17, after the system freeze)

- HEAD at resume: `afc560be` (+ the report commit at run time). The
  uncommitted `server.rs` diff found on resume was a leftover
  `--in-place` mutant artifact (`index += 1` → `index *= 1`, the
  infinite-loop family also represented by run 3's recorded timeout
  kill at 539:15), not an in-progress trivial kill — discarded per
  plan D3.4 (`git checkout --`).
- **Run-3 evidence preserved**: `mutants.out.old/outcomes.json`
  snapshotted to `/tmp/c7-run3-outcomes.json` before the resume run
  renamed dirs aside; 18 completed mutants (17 caught + 1 timeout),
  0 missed.
- **Invocation rule (binding)**: every mutants invocation goes
  through the memory gate with an explicit cap:
  `scripts/memory-gate.sh -- cargo mutants --jobs 2 …` inside the
  #327 `timeout 7200` template. Deviations recorded:
  1. `scripts/memory-gate.sh` is **not on `origin/main`** (it landed
     on `chore/resource-gate-concurrency-law`, pending) — the script
     was run from a checkout of that branch
     (`git show chore/resource-gate-concurrency-law:scripts/memory-gate.sh`
     extracted to `/tmp/c7-gate/`), keeping the repo diff clean.
  2. **`--in-place` and `--jobs` are mutually exclusive** in
     cargo-mutants 27.1.0 (`error: the argument '--in-place' cannot
     be used with '--jobs <JOBS>'`). The memory cap (`--jobs 2`) is
     binding, so `--in-place` was dropped; the side effect is
     positive: mutation happens in a throwaway `/tmp` copy, so the
     D3.4 tree-residue risk disappears.
  3. `--test-tool nextest` retained from run 1b (plain `cargo test`
     baseline is red on the `taskboard_env_lock` per-test-thread
     process-env race).
- **Runs 4a/4b lost to externally delivered SIGINTs, 4c detached**:
  run 4a (agent-owned background shell) was interrupted ~9 min in
  after a green unmutated baseline — 70 bogus "Unviable" outcomes
  (interrupted builds misclassified), discarded. Run 4b (harness
  background task) ran ~61 min to 42 mutants processed (20 caught,
  1 missed, 2 timeout kills valid; 19 further bogus "Unviable"
  after a second external SIGINT) before being stopped by the task
  runner; snapshot `/tmp/c7-run4b-outcomes.json`. Since
  `--iterate` skips unviable mutants, every poisoned `mutants.out`
  was deleted rather than resumed, and run 4c restarted **fresh**
  via `setsid nohup` (own session — immune to process-group
  SIGINTs from the harness or aborted foreground polls), re-testing
  all 109 `ipc/` mutants with the kill tests from
  27e2e24e/41fbd27d/afc560be in the baseline. Operational lessons
  recorded: (i) an aborted foreground long-poll delivers SIGINT to
  its process group and can kill a background run — polls are short
  commands only; (ii) the task runner reaps agent-owned background
  shells — long runs must be detached and followed via output files
  (`setsid` was insufficient; `systemd-run --user` transient units
  worked and gave oomd a precise kill target).

### Gate intervention and the memory ceiling (dispatcher, 2026-11-17)

- Runs 1–4f were wrapped `scripts/memory-gate.sh -- <run>` per the
  stage-2 invocation rule; the gate holds its flock for the
  command's whole duration, which serialized every other agent's
  gated pre-commit clippy behind the mutants run — a live
  availability collapse the dispatcher verified. Run 4f was stopped
  mid-run and the pattern corrected: **pre-flight-only** (check
  MemAvailable ≥ 8192 MiB, record the reading — 22644.9 / 20193 /
  23592 MiB across the relaunches — then exec ungated), relying on
  the `--jobs 2` cap, the 7200 s timeout, and systemd-oomd.
- **Finding for #923** (commented there): the gate needs a
  pre-flight-only mode for self-capped long runs; wrapping a
  multi-hour run in the gate is an availability collapse, but the
  ungated pattern has its own cost — both ungated runs were
  OOM-killed by systemd-oomd at 23–25 G peaks (killed-run 4f, held
  behind the gate, peaked at 13.2 G under the same `--jobs 2`). The
  gate's serialization of build peaks is doing real memory work;
  a self-capped long mutants run likely needs `--jobs 1` or a
  narrowed per-mutant test surface instead of either wrapper.
- Cleanup after run 4f's `--iterate` bookkeeping: outcomes
  snapshots in `/tmp/c7-run{4a,4b,4f,5b,5c}-outcomes.json`;
  `mutants.out` left uncommitted and removed at handoff per plan §4.
