# [Report] Flaky-Test Watchlist Investigation — #287, #288, #289, #290

- **Date**: 2026-10-17
- **Kind**: investigation report (backward-looking analysis; proposes no
  merged code — the forward-looking fix plan is
  `.artifacts/plans/2026-10-17-flaky-test-remediation-plan.md`).
- **Origin**: user directive of 2026-10-17 ("the project's github issue
  tracker has accumulated enough flaky test reports that I think an
  investigation is warranted. Please look at these flakes, see if you can
  find a reason, and propose a plan to fix them.").
- **Scope**: every open issue labelled `flaky-test` at investigation time —
  #287 (`actor_wiremock`), #288 (coverage-lane profraw merge), #289
  (`repl_history` libtest flakes, local-only), #290 (`e2e_hermetic`
  deadline exhaustion, CI). No code changed; all experiments ran in
  worktree `../taskboard-WT-docs-flaky-test-remediation-investigation`
  (branch `docs/flaky-test-remediation-investigation` @ `7b31c0b`).
- **Status**: complete for #290/#287/a10 and #288; #287/coverage-lane has a
  high-confidence mechanism and a cheap hardening; #289 is root-caused to
  the host/kernel boundary with five candidate causes **eliminated by
  experiment** (§4.3) — final attribution needs privileged access or one
  user-shell run (§4.4).

## 1. Executive summary

Four issues reduce to **three distinct causes** (one already resolved):

| Family | Issues touched | Root cause | Fixable in-repo? |
|---|---|---|---|
| F-1 virtual-deadline starvation | #290 (both tests), #287 (a10 occurrence, CI string now recovered), likely #287 (2026-10-11 instrumented occurrence) | The hermetic suites' wait helpers bound their loops on the **paused tokio clock** while the sync actor's progress depends on **real wiremock I/O**; under CI CPU contention the virtual budget (6k–12k iterations ≈ **0.4–0.9 s wall**) expires before the I/O chain completes | **Yes** — hybrid wall-clock deadline (plan chunk 1) |
| F-2 refused-port fixture race | #287 (2026-10-16 coverage-lane occurrence of `create_stack_transport_failure_aborts`) | `refused_client` binds a port and drops the listener; between drop and the client's connect, another concurrently starting `MockServer` can win the same ephemeral port, turning the expected `Transport` (abort) into a 404 `NotFound` (rejected) | **Yes** — assert the refusal precondition in the fixture (plan chunk 2) |
| F-3 host-kernel EACCES | #289 | ~~kernel-level~~ **CORRECTED same day — see §7**: rustyline 18.0.1's `FileHistory::save` mutates the process-global umask around the history-file create; concurrent creates in other threads land with stripped mode bits (directories without `x` → `EACCES` on traversal/child ops). Reproduced with an independent probe. **A code defect (rustyline upstream), fixable in-repo by bypassing `FileHistory::save`.** |

#288 (corrupt profraw aborting the coverage merge) is **resolved** — the
2026-10-13 stage-2 verdict (SIGKILL inside an instrumented child's
profile-write window; quarantine script; five consecutive green runs) is
confirmed by its own evidence trail; no further work.

Also discovered (§5): `/tmp` on this host carries **10,923 leaked
`.tmp*.db-shm` / `.db-wal` files** (2026-10-11 → 2026-10-16) from
sqlite-pool test runs — a separate small hygiene defect, proposed as its
own backlog issue in the plan.

## 2. Method

1. Issue review: all four `flaky-test` issues with full observation
   comments; #288's investigation report and dispatch plan re-read for
   family boundaries.
2. Code reading of every implicated test surface: `e2e_hermetic.rs`,
   `actor_wiremock.rs`, `common/hermetic.rs`, `push_exec.rs` (test
   module), `repl_history.rs`, `client.rs`, `error.rs`, `actor.rs`, and
   `.github/workflows/ci.yml`.
3. Live reproduction on the dev host (the machine every #289 observation
   was recorded on): repeated suite runs under the libtest parallel
   runner, under nextest, under `--test-threads=1`, and in neighboring
   suites, with `strace -f` capture of failing runs, a raw-inotify
   `IN_ATTRIB` watcher over `/tmp` during failing runs, a standalone
   syscall-pattern reproducer, and kernel/LSM checks.
4. CI evidence recovery: nextest's failure output for the 2026-10-16 a10
   occurrence was still present in run `38965966332`'s logs — the
   watchlist had recorded the test name only; the panic string (§4.1)
   settles the mechanism.

## 3. Watchlist roll-call (state after this investigation)

| Issue | Test surface | Occurrences | Verdict |
|---|---|---|---|
| #288 | CI coverage-job profraw merge | 3 CI + 1 local (2026-10-11…13) | **Resolved** 2026-10-13; quarantine script in CI; five green runs recorded. Closed family. |
| #290 | `e2e_hermetic` i1/i2 | 1 CI run (2026-10-16 22:25), 2 tests | **Root-caused** (F-1). Panic string in the issue matches the mechanism exactly. |
| #287 | `actor_wiremock` | 3 recorded: 2026-10-11 instrumented exit 101; 2026-10-16 a10 (plain nextest); 2026-10-16 coverage-lane `create_stack_transport_failure_aborts` | a10 **root-caused** (F-1 — see §4.1); coverage-lane occurrence **mechanism identified** (F-2, high confidence); 2026-10-11 occurrence plausibly F-1 under instrumentation (unconfirmed). |
| #289 | `repl_history::tests` (7 tests) | ~dozens local (2026-10-15 → present), **reproduced at will during this investigation** | **Root-caused (§7 amendment): rustyline's process-global umask window** — a code defect, not environmental; fixable in-repo (plan chunk 3). |

## 4. Findings

### 4.1 F-1 — virtual-deadline starvation in the hermetic wait helpers (#290, #287/a10)

**The mechanism.** All three hermetic wait helpers run the same shape:

```rust
let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
loop {
    if predicate_ready() { return; }
    assert!(tokio::time::Instant::now() < deadline, "…");
    tokio::time::advance(Duration::from_millis(5 or 10)).await;
}
```

Under `#[tokio::test(start_paused = true)]`, `tokio::time::Instant::now()`
reads the **paused virtual clock**, so the deadline buys
`60_000 ms ÷ 5 ms = 12,000` iterations (`next_report`: `÷ 10 ms = 6,000`;
`start_server`: `6,000`). Each `advance` yields the runtime **once**. The
sync actor's progress, however, is a chain of **real** I/O — TCP loopback
to a wiremock server running as tasks on the same current-thread runtime
(`common/hermetic.rs` `start_server` spawns `MockServer::start` locally) —
where every scheduler round is only granted when the test's loop yields.
The virtual budget therefore maps to a *finite, small* number of
scheduling grants, and under CI CPU contention (nextest runs several test
binaries per runner; GitHub runners share host capacity) those grants
complete in a fraction of a second of wall time — regardless of whether
the I/O chain has caught up.

**Arithmetic fits the observed timings.** Budget exhaustion costs
6,000–12,000 near-empty iterations; the observed failures died at
**0.34 s / 0.398 s / 0.54 s** wall (≈ 30–70 µs/iteration) — i.e. the
virtual minute is actually *under one real second* of patience. Local
green runs of the same tests complete in 0.02–0.03 s because the loopback
chain finishes immediately; the flake only appears when contention delays
the chain past ~0.4 s of wall time.

**Recovered CI evidence (new, 2026-10-17).** nextest's captured failure
output for the a10 occurrence (run `38965966332`, 2026-10-16 21:00 UTC):

```
thread 'a10_304_with_a_cold_cache_refetches_unconditional…' panicked at
crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs:68:9:
actor produced no report in simulated time
```

That is the `next_report` virtual-deadline assertion (actor_wiremock.rs:69)
— the same failure family as #290's `predicate satisfied within the
deadline` (e2e_hermetic.rs:134/136). **#287's plain-nextest occurrence and
all of #290 are one root cause.**

**Affected sites (complete list, workspace grep 2026-10-17):**

| Site | Budget | Step | Max iterations |
|---|---|---|---|
| `crates/taskboard-sync-nextcloud/tests/e2e_hermetic.rs:129` (`eventually`) | 60 s virtual | 5 ms | 12,000 |
| `crates/taskboard-sync-nextcloud/tests/e2e_hermetic.rs:828` (`eventually_no_state`) | 60 s virtual | 5 ms | 12,000 |
| `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs:63` (`next_report`) | 60 s virtual | 10 ms | 6,000 |
| `crates/taskboard-sync-nextcloud/tests/common/hermetic.rs:118` (`start_server`) | 60 s virtual | 10 ms | 6,000 |

Other deadline loops in the tree are **not** affected: the app's
subprocess suites (`cli_ipc.rs`, `it_cli.rs`, `it_docker.rs`, `cli_tty.rs`,
`cli_lock.rs`, `common/mod.rs` `LIVE_DEADLINE`) already use
`std::time::Instant` (wall clock), and the remaining matches are
production code (`sync-nextcloud/src/actor.rs:287`, `taskboard-state/src/actor.rs:473`,
`taskboard-app/src/bootstrap.rs:684`).

**Why the 2026-10-11 occurrence is attributed here (plausibly, not
provably).** That occurrence was the whole `actor_wiremock` binary failing
under the *instrumented* lane, where every I/O step is orders of magnitude
slower (coverage probes on exactly these code paths) — the same budget
exhaustion has a much larger attack surface. It failed as an exit-101 test
failure, not a merge failure, so it is distinct from #288's resolved
family. Confidence: moderate; if the family recurs after chunk 1 lands,
the residual capture protocol (§4.4 of the plan) applies.

**Fix direction (detailed in the plan).** Keep the paused-clock stepping
(it is what makes actor poll ticks fire *exactly* while waiting — the
determinism property the module docs promise) and bound the loop on a
**wall-clock deadline** (`std::time::Instant`) instead. The predicate is
not weakened — it must still become true — the wait simply stops lying
about how much patience it has. Minor adjacent defect: the existing panic
messages read `predicate satisfied within the deadline` — the word *not*
is missing (the assertion fires when it was **not** satisfied);
corrected wording while touching those lines.

### 4.2 F-2 — `refused_client` ephemeral-port race (#287, coverage-lane occurrence)

`push_exec.rs:1083` builds a client pointed at a *just-dropped* listener:

```rust
let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
let port = listener.local_addr().unwrap().port();
drop(listener);
DeckClient::new(&format!("http://127.0.0.1:{port}"), "u", "t").unwrap()
```

The scenario needs a *refusing* port: the client's connect must fail with
`ECONNREFUSED`, which maps to `DeckError::Transport` → the group aborts
(`GroupOutcome::Aborted`) — the assertion. Between `drop(listener)` and
the client's connect there is a window in which any other process on the
host calling `bind(0)` (the workspace's test binaries start *many*
`MockServer`s concurrently; the coverage lane multiplies both the number
of binds and the width of the window, because instrumented setup code runs
much slower) can be handed the same ephemeral port. The client then
*connects*, wiremock answers 404 (no mock for `POST …/stacks`), the client
maps 404 → `DeckError::NotFound` → `PushResult::Rejected` → outcome
`GroupOutcome::Outcomes` — and the test fails exactly as recorded
("the test itself" failed in the coverage lane, run `38958620489`).

Confidence: high as a *mechanism* (the race is structural; the failure
signature matches the only divergence the collision can produce), low as
*the* explanation of that single recorded occurrence (one event; the
collision probability per run is small — the kernel's ephemeral allocator
must wrap back to the freed port inside the window — but thousands of
binds across the coverage suite × months of runs makes one hit plausible).

The fix is fixture-level (plan chunk 2): after dropping the listener,
probe-connect the port and require `ConnectionRefused`; on any other
outcome, regenerate. The scenario ("transport failure aborts the group")
is untouched; only the fixture's precondition ("this port actually
refuses") becomes checked rather than assumed.

### 4.3 F-3 — host-kernel `EACCES` on fresh tmpfs paths (#289)

**Reproduced at will during this investigation** (previously "~60% of
runs"): `cargo test -p taskboard-app --lib repl_history` on this host,
plain libtest parallel runner — 10/15 and 8/10 failing runs, 1–3 tests
each, always within the seven `repl_history` tests, at the historically
recorded sites (repl_history.rs:162 empty-entries after load, :180
metadata, :194/:210/:226 direct `std::fs` writes/mkdir). The full
109-test app-lib suite failed identically and **only** in `repl_history`
tests (3/5 runs). All failure outputs are `Os { code: 13, kind:
PermissionDenied }`.

**The syscall-level truth (fresh strace evidence, this investigation).**
Full unfiltered `strace -f` of failing runs shows the denial arriving
from the kernel for a thread whose entire history is:

```
prctl(PR_SET_NAME, "repl_history::t"…)        ← a libtest test thread is born
…stack setup…
mkdir("/tmp/.tmpuEJWsY", 0777)          = 0   ← parent tempdir: SUCCESS
mkdir("/tmp/.tmpuEJWsY/taskboard", 0777) = -1 EACCES   ← next syscall: DENIED
statx(…"/tmp/.tmpuEJWsY/taskboard/history"…) = -1 EACCES
```

Same thread, same uid, two consecutive syscalls, the first succeeds, the
second is denied. Earlier captures show the same signature for
`openat(O_WRONLY|O_CREAT|O_TRUNC, 0666)` of a file inside a fresh
tempdir. No `chmod`/`fchmod` occurs in between (in-process), and the
process tree carries `Seccomp: 0`, `NoNewPrivs: 0`.

**Eliminated hypotheses (each with its disproof):**

| # | Hypothesis | Disproof |
|---|---|---|
| 1 | DAC / permission bits (a 0700-vs-0775 mixup, umask, tempfile quirk) | The denied syscalls run as the directory's owner; owner `rwx` passes DAC regardless of group/other bits. `mkdir` of the *parent* succeeds one syscall earlier. |
| 2 | The ZCode agent sandbox (Landlock/seccomp interposition) | Persists with the sandbox disabled (prior agent's finding, **re-confirmed** in this investigation: 8/10 failing runs with `dangerouslyDisableSandbox`). Independently: Landlock requires `no_new_privs=1` or `CAP_SYS_ADMIN` to apply, and the exec chain shows `NoNewPrivs: 0` / `Seccomp: 0` — **no Landlock or seccomp can be active on these processes**. |
| 3 | An external daemon flipping permissions on the tempdirs (fanotify-perm / reactive watcher / AV) | A raw inotify watcher on `/tmp` (`IN_ATTRIB`, `IN_CREATE`, `IN_MOVED_FROM`) during 8 suite runs (4 failing): 40 events, **all** `IN_CREATE\|IN_ISDIR` from the tests' own tempdir creations, **zero `IN_ATTRIB`** — nobody chmod'ed anything, while EACCES still happened. Also: `mkdir` denial is unreachable for fanotify permission events (they cover open/execute only). |
| 4 | The syscall pattern itself (tmpfs + rapid create/destroy churn) | A standalone dependency-free reproducer (8 threads × 2,000 iterations of exact tempdir → subdir → `File::create` 0666 → fchmod 0600 → statx → teardown) ran **48,000 iterations with zero EACCES**, matching the earlier 400-iteration probe from the watchlist. The pattern is necessary but not sufficient — the trigger involves the test binary's broader context. |
| 5 | Wrong kernel timeline (new kernel + immediate flake onset) | Kernel `7.0.0-22-generic` was installed 2026-10-07, the host booted 2026-10-11; first recorded flake 2026-10-15. Weak-to-moderate correlation; `repl_history` itself only landed ~2026-10-11/12, so "the test began existing" is an equally good onset predictor. Inconclusive. |

**What remains.** With DAC, sandbox/LSM userspace tooling, seccomp,
external chmod, and the bare syscall pattern all excluded, the denial
originates **inside the kernel's own path decision** for these processes —
most plausibly a regression or race in the 7.0.0-22 VFS/tmpfs/security
stack that manifests under libtest's specific shape of concurrency (all
test threads spawned at once, each doing a *short burst* of depth-≥2
creates on fresh tmpfs paths, whole process < 1 s), and which the
standalone reproducer's sustained-uniform shape does not trip. Kernel-side
attribution (bug vs. an LSM path not visible unprivileged) requires
privileged tooling: auditd/apparmor logs (`journalctl -k` is root-only
here and returned nothing readable), `auditctl` syscalls rules, bpftrace,
or a kernel bisect 7.0.0-15 → -22.

**Calibration runs recorded today** (all on this host): nextest lane
6/6 green (`repl_history` filter); serial `--test-threads=1` green ×2;
`taskboard-storage-sqlite --lib` green ×3; full app-lib suite failures
exclusively in `repl_history`. CI (GitHub runners) has never seen a
failure of these tests. The flake is **local-libtest-parallel-only** —
the mandated gate commands (`cargo nextest run --workspace`,
`cargo llvm-cov nextest …`) are and remain unaffected in CI.

**One caveat recorded for honesty:** every recorded occurrence to date —
including today's — was produced by an agent-executed process chain on
this host (the P6 review run, the 2026-10-17 gate-run comment, and this
investigation). No confirmed user-terminal reproduction exists yet.
The mechanism space *should* be parent-independent (kernel denies
syscalls regardless of ancestry), and Landlock/seccomp ancestry capture
is excluded by the NNP/Seccomp flags above — but the single cheapest
decisive experiment is for the **user** to run the same loop from their
own shell:

```sh
for i in $(seq 1 10); do cargo test -p taskboard-app --lib repl_history 2>&1 | grep '^test result'; done
```

Green there ⇒ the trigger involves the agent exec pipeline in some
unimagined way (reopen hypothesis 2 with fresh eyes); failing there ⇒
host/kernel confirmed, and the escalation ladder (plan chunk 3) applies.

### 4.4 Note on #289's "empty in-memory history" symptom

The watchlist records one run where `DefaultHistory` appeared empty after
three infallible `add`s. Today's run set reproduced the *adjacent*
assertion failure (`repl_history.rs:162`: `entries_of(&next_session)` =
`[]` after a session whose earlier `record` → `save` must have been
EACCES-denied — the file was never written, so the reload legitimately
finds nothing). Mechanistically, a same-process EACCES on the save path
explains every observed failure without needing impossible in-memory
mutation; the one "empty after adds" observation is best read as the
same test's second assertion (post-reload), mis-attributed in prose. No
separate defect is warranted.

## 5. Discoveries outside the watchlist scope

1. **`/tmp` leak — sqlite sidecars (10,923 files).** `/tmp` on this host
   holds `.tmpXXXXXXXX.db-shm` / `.db-wal` pairs dating 2026-10-11 → 
   2026-10-16, owner `viktoria`, i.e. sqlite-pool test databases whose
   sidecar files survive `TempDir`/`NamedTempFile` cleanup (the cleaner
   removes the named file/dir it created; sqlite's `-shm`/`-wal` siblings
   are invisible to it — the standalone-file variant of the leak proves
   NamedTempFile use). Harmless to correctness, but 10⁴ stale files on a
   tmpfs and steadily growing. Proposed as its own `backlog:` issue
   (category `testing`/`dx`), fix candidate: pool disposer closing the
   connection before drop, or a test-guard that asserts clean teardown.
2. **Three leftover IPC tempdirs (2026-10-11).** `/tmp/.tmp{8CCkPu,18scaK,AvkfpP}`
   contain `zz.db`, `zz.lock`, `zz.sock` — daemon fixtures from the
   2026-10-11 IPC-test era where a killed daemon held the directory.
   Distinct, benign, low-priority; may fold into the same hygiene issue.
3. **Inverted panic wording.** Both `eventually` helpers and
   `next_report`/`start_server` assert `now < deadline` with the message
   "predicate satisfied within the deadline" — missing the *not*.
   Trivial, folded into plan chunk 1 (it touches the same lines).

## 6. Open questions

1. **#289 attribution**: kernel regression vs. an unseen host policy
   layer — settled by the user-shell experiment (§4.3) and, failing
   that, by root-assisted audit/bisect. Neither blocks the fix plan;
   chunk 3 encodes the protocol.
2. **#287's 2026-10-11 instrumented occurrence**: attributed to F-1 by
   plausibility only. If it recurs after chunk 1 lands, nextest's
   `--failure-output` retention (final-status output already captures
   panics — as demonstrated by today's a10 log recovery) plus the
   quarantine-log cross-check will decide it from one more sample.
3. **F-2 collision probability**: never measured in the wild (needs the
   fixture to log its regenerations post-fix — the plan includes that
   logging, which converts future occurrences from mysteries into
   counters).

## 7. Amendment (2026-10-17, same day): F-3 root cause corrected

After this report was committed, an independent analysis landed on
#289 (comment 6235443727, agent on `refactor/enginecommand-envelope-rename`)
identifying rustyline 18.0.1's `FileHistory::save` as the trigger: the
method wraps the history-file create in a **process-global umask
mutation** (`rustyline-18.0.1/src/history.rs:692-695`):

```rust
let old_umask = umask();               // umask(S_IXUSR | S_IRWXG | S_IRWXO) == 0o177
let f = File::create(path);
restore_umask(old_umask);
```

Any `mkdir`/`File::create` executed by *another thread* of the same
process inside that window gets its mode bits stripped
(`0777 & !0o177 = 0o600`) — a directory **without the execute bit**.
Every symptom in §4.3 follows mechanically: `mkdir` of a child inside
such a directory → `EACCES` (needs `w+x`), `metadata`/`openat` through
it → `EACCES` (traversal needs `x`), and the "empty next session"
assertion (the save itself fails inside a 0600 directory, so the reload
finds no file). The runner matrix follows too: nextest (one test per
process; umask is per-process) and serial libtest are immune; only
in-process parallel libtest (`cargo test --lib`, plain
`cargo llvm-cov --workspace`) is exposed. CI lanes are immune by
construction.

**Independently verified in this session** (not taken on faith):

1. *Source*: the quoted umask dance confirmed at
   `~/.cargo/registry/src/…/rustyline-18.0.1/src/history.rs:693-695`
   (unix arm :819-824). **rustyline 19.0.0 (current latest) still has
   it** — verified against the 19.0.0 source on docs.rs; no upgrade
   escape, and per the comment agent no upstream issue exists yet.
2. *Reproduction*: a standalone probe (one "villain" thread running a
   tight `DefaultHistory::add`+`save` loop; 8 victim threads doing
   tempdir → `mkdir(taskboard, 0777)` → stat → `File::create` inside →
   stat) produced **~1,100 EACCES per 16,000 ops across three runs**,
   and — the decisive detail — captured the victims' actual directory
   modes: `0o600 (384)` vs the session's normal `0o775 (509)`, i.e.
   directories physically created inside the umask window. This also
   explains why this report's original §4.3 reproducer (48,000 clean
   iterations) stayed clean: it replicated rustyline's `flock`+`fchmod`
   but not the umask mutation.

**Where §4.3's elimination table stands**: rows 1 (DAC), 2 (agent
sandbox / Landlock / seccomp) and 3 (external chmod watcher — umask
strips at create time and leaves no `IN_ATTRIB` trace, which is exactly
why the inotify watch was clean) remain valid and were necessary steps;
row 4's conclusion ("the syscall pattern is not sufficient") stands but
its framing missed that the trigger was a *fourth* syscall pattern
element (the umask call) not present in any probe; row 5 (kernel
timeline) is moot. The §4.3 "prime suspect: kernel regression"
conclusion is **retracted** — the denial was userspace all along, with
a mechanism that leaves no kernel-side fingerprint. The §4.3
user-shell experiment is also moot: the bug reproduces under any
parent process, including the probe above.

**Impact beyond tests**: this is not a test-only defect. The REPL runs
in the app process alongside the tokio actors; a history save landing
while any other thread creates a file or directory (sqlite sidecars,
config writes, IPC sockets) strips that path's group/other/execute
bits. Severity in production is bounded (window is one `File::create`
wide; owner access is preserved — `S_IXUSR` is stripped too, but owner
`rw` remains, so the *history file itself* is correct at 0600) but real
for everything else sharing the process.

**Fix path** (plan chunk 3, revised): stop routing persistence through
`FileHistory::save` — `SessionHistory` already owns "the persistence
policy in one place"; write the history file itself via
`OpenOptions::new().write(true).create(true).truncate(true).mode(0o600)`
(same newline format `DefaultHistory::load` reads), keeping rustyline
for in-memory/editor concerns. rustyline's own `save_to` already pairs
the umask trick with an `fchmod(0600)`, so nothing is lost. Plus: file
the upstream rustyline issue (the fix belongs in the crate for
everyone), and the existing `repl_history` libtest suite under default
parallel threads becomes the regression harness — on this host it
failed ~60% of runs before the fix and must go green without any test
change.
