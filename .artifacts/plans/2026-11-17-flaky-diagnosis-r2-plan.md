# [Plan] C3 — Flaky diagnosis r2: #697 post-stall budget reorder + CI gap telemetry; #570 relay instance-locked diagnosis

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan (dispatch chunk C3 of the 2026-11-17
  backlog-reduction round; round plan §3-C3 is normative for this file).
- **Branch**: `test/flaky-diagnosis-r2` (created off `origin/main` @
  `44c6f32b`; worktree
  `../taskboard-WT-test-flaky-diagnosis-r2`).
- **Issues**: #697 (cli_tty stalled-line post-stall flake), #570
  (cli_ipc relay export `instance-locked`).
- **Laws in force**: typed outcomes only; stabilization makes tests
  more deterministic, never more permissive (no sleeps, no
  timeouts-as-fix, no deleted assertions, floors not ceilings); a
  flaky test can be a correct test catching a production race —
  evaluate both sides; `backlog:` issues immediately.

## 1. Verified seam inventory

- **`crates/taskboard-app/tests/cli_tty.rs`** — the stalled-line test
  `tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`
  (line ~1600):
  - **Designed silent window**: `NotifyThenDelay` (line ~1394) —
    `wiremock::ResponseTemplate::set_delay(Duration::from_secs(10))`
    at line ~1620. The window is a bare literal shared by no other
    symbol. The route notifies on request arrival, then answers 200
    after the delay — the only *designed* silence in the suite.
  - **The failing assertion** (was cli_tty.rs:1687 at capture time;
    now the `assert_eq!(created.len(), 1, "the line after the stalled
    one executed: …")` at line ~1687): filters JSON lines for
    `title == "B"`. Upstream of it: `feed_at(2, "boards create B
    --json")` (line ~1662) gates on `wait_readline(2)` →
    `PtySession::wait_for` → `PtyOutput::wait_for`
    (`predicate || progress || PTY_IDLE_BUDGET`, line ~173).
  - **Budget constants** (lines ~54–79):
    `PTY_IDLE_BUDGET = max(30 s, 3 × G_max)` = 30 s — the comment
    itself records the **3×-thin-headroom finding** (G_max = the 10 s
    designed window; 30 s = exactly 3×, zero margin); and
    `PTY_SESSION_BACKSTOP = max(120 s, 3 × T_max)` = 186 s (T_max
    62 s observed on loaded CI).
  - **Gap telemetry**: `GapLog` (line ~103) — env-gated
    `TASKBOARD_PTY_TEST_GAP_LOG=<path>`, one `<micros>\t<len>` line
    per output chunk, `File::create` (truncate) per reader thread.
    **Defect for CI reuse**: the path is a single file and parallel
    nextest processes (and parallel test binaries in one job) would
    clobber it.
- **Signal dispositions** (`crates/taskboard-app/src/signals.rs`
  `install_lifecycle_watcher`; `crates/taskboard-app/src/repl.rs`):
  the app has two SIGINT consumers — the lifecycle watcher (graceful
  shutdown) and the REPL's per-command cancel path (the typed
  `cancelled` envelope this test pins). The test's comment (line
  ~1596) states the design: by the time the dispatch is in flight the
  editor's raw mode is gone, so the ^C byte is a real SIGINT that the
  running-command cancel path should own.
- **`crates/taskboard-app/tests/cli_ipc.rs`** — the relay family:
  - `t16_track_and_export_over_the_relay` (~line 672): relayed verbs
    over the daemon socket, then `daemon stop` →
    `wait_until(daemon:false)` → **kill + reap of the daemon child**
    (lines ~757–760, the in-tree mitigation whose comment cites
    #570) → in-process fallback export.
  - `j20_journal_and_its_export_over_the_relay` (~line 900, failure
    at :969 "the relayed entry is durable"): the **same fallback
    shape without the kill+reap mitigation** — `daemon stop` →
    `wait_until(daemon:false)` → fallback `journal show` / export.
- **Client routing / the relay race**:
  `crates/taskboard-app/src/ipc/client.rs::try_connect` (line 122):
  returns `None` **immediately** on `NotFound | ConnectionRefused |
  PermissionDenied`; only *other* error kinds retry inside
  `CONNECT_BUDGET` (1 s) with `CONNECT_RETRY` (50 ms). `None` routes
  the verb to in-process bootstrap
  (`main.rs` / `commands/mod.rs:380`); bootstrap acquires the
  instance lock (`bootstrap.rs:1251`, `LockError::Held` → typed
  `instance-locked`).
- **Graceful-stop ordering**: `daemon stop` removes the socket
  artifact (client view flips `daemon:false`) while the daemon
  process may still be draining (`shutdown_timeout_secs`, up to 5 s)
  and **holding the instance lock** (the lock is released only by
  process death). `config.rs` documents `status` reporting the
  lock-holder pid with `daemon:false` — the gap is by design
  client-visible.

## 2. The record, condensed

- **#697**: four observations under load (2026-11-05 CI, 11-07 local
  gate, 11-11 CI run 43084174799 with full output, 11-14 CI). All
  fail at the `created.len()` assertion at ~10.1 s. The 11-11 CI
  capture: `boards list --json` dispatched at T+0.05 s, ~10 s of
  silence, ending with the sync actor's capabilities probe; **exit
  code 0** (both earlier asserts passed) and **no `cancelled`
  envelope** in the transcript. Local telemetry (11-16, 10/10 loaded
  passes): max gap ≈ exactly 10.0 s — the designed window, 3× under
  the idle budget.
- **#570**: three observations, two shapes. Late shape (2026-11-01
  t16 @ :753; 2026-11-16 j20 @ :969): fallback in-process export
  after `daemon:false` → `instance-locked`. Early shape (2026-11-06
  t16 @ 0.28 s, i.e. *before* the stop legs): a relayed verb after
  `wait_ready` (`daemon:true` observed) → `instance-locked` — the
  client fell back to bootstrap while the daemon held the lock. A
  30/30 loaded bounded negative (2026-11-04) stands for the test in
  isolation; capture policy recorded on the issue.

## 3. Decisions

### D1 — #697: the reordered budget inequality (structural, named margin)

**Inequality (binding for the implementation):**

```
PTY_IDLE_BUDGET >= 6 × D_max + D_max        // k = 6, slack = one window
PTY_IDLE_BUDGET = max(70 s, 6 × DESIGNED_SILENT_WINDOW + DESIGNED_SILENT_WINDOW)
            → lands at 70 s today
```

with `D_max`/`DESIGNED_SILENT_WINDOW` a **new named constant**
`const DESIGNED_SILENT_WINDOW: Duration = Duration::from_secs(10);`
that *both* the wiremock route's delay (line ~1620) and the budget
derivation consume — `Duration` const arithmetic (`* 6`, `+`) is
legal in const context. The inequality then holds **by construction**:
reordering or retuning the window cannot silently unorder the
budgets, which is exactly stage-3 trap (a).

**k = 6, rationale**: the captured CI interleaving consumed the whole
designed window plus session startup (~10.09 s ≈ window + boot) and
*still* had contention headroom; k = 3 with zero margin is the
retired thin-headroom finding; k = 6 + one-window slack tolerates two
back-to-back designed windows plus CI-load slop inside one budget.
Worst-case wedge-detection latency rises 30 s → 70 s — a floor
movement (later, stricter detection of true wedges), not a permissive
one; the `assert_ne!(code, Some(124))` wedge pins keep their meaning.

**Backstop**: the formula `max(120 s, 3 × T_max)` is unchanged;
stage-2 re-derives `T_max` from fresh loaded telemetry with the 70 s
idle budget in force (expected: unchanged at 186 s — legitimate runs
do not consume the idle budget — but the re-derivation is recorded,
never a nudge). Structural ordering requirement:
`PTY_SESSION_BACKSTOP >= 2 × PTY_IDLE_BUDGET` (a wedge must be
detectable twice over before the backstop fires).

**Also update the `PTY_IDLE_BUDGET` doc comment** (lines ~54–65): the
thin-headroom finding paragraph is replaced by the recorded
inequality and its k/slack rationale; the "re-derivations follow this
formula with fresh telemetry" rule stays.

**Not part of D1**: no test-logic change in the stalled-line test
itself — the feeds, gates, and assertions are untouched.

### D2 — #697: CI gap-telemetry enablement

- **Job**: `Test Execution` (job `test`) in `.github/workflows/ci.yml`
  (the job that runs the workspace nextest suite; the only workflow
  job that runs `cli_tty`). No other workflow file is touched.
- **Test-side seam change first**: extend `GapLog::open` to treat the
  env value as a **directory**: when set, the reader thread writes
  `<dir>/<test-name>-<pid>.log` (tests pass their name through
  `PtyOutput::spawn`; the binary names are distinct nextest
  processes, so pid + test name disambiguates the parallel clobber
  the single-file mode has). A plain-file value keeps today's exact
  behavior (backwards-compatible for local use). Small, test-only,
  diagnostic-only.
- **CI env**: `TASKBOARD_PTY_TEST_GAP_LOG: ${{ runner.temp }}/pty-gap`
  as job env on `test`. The directory is created by the harness
  (`File::create` gets a `create_dir_all` first) — no workflow
  mkdir step needed.
- **Capture destination**: an `actions/upload-artifact@v4` step,
  `if: failure()`, uploading `${{ runner.temp }}/pty-gap/` as
  artifact `pty-gap-logs` with `retention-days: 30` — the same
  mechanism family as C2's #857 proptest-regressions artifact. Gap
  data stays out of the job log.
- **Capture policy alignment**: the next occurrence then satisfies
  #697's standing policy (full failure block + gap telemetry) from CI
  alone.

### D3 — #697: the side question — armed primary hypothesis (production race)

The evidence does **not** fit the round plan's "wait budget expired
inside the 10 s window" hypothesis:

- No wait in the test has a 10 s-scale deadline. The only budgets are
  `PTY_IDLE_BUDGET` (30 s) and the design window itself; an idle
  abort would fire at ≥ 30 s and panic with the *wedge* message, not
  at the `created.len()` assertion.
- The 11-11 capture shows exit code **0** (both preceding asserts
  passed) and **no `cancelled` envelope** — a ^C that took the
  designed cancel path prints one. The session ended ~10 s in, in
  silence, with the sync actor's capabilities probe last.

**Primary armed hypothesis H1 (production race)**: the ^C SIGINT
reached the **lifecycle watcher's graceful-shutdown path** instead of
the running-command cancel path — a disposition window between the
readline teardown and the per-command cancel handler's arm (the
signal arrives while neither owns it, or the watcher claims it
first). Graceful shutdown then drains until the in-flight stalled
HTTP request resolves — which takes exactly the wiremock window
(~10 s) — explaining the 10.09 s total, rc 0, the silence, and the
trailing capabilities probe (the connector/sync teardown). The local
10/10 and 30/30 passes are timing-consistent: the window only opens
under CI interleaving.

**Evidence method for stage-2 (decides the side)**:
1. Backgrounded loaded repro loop (bounded, 30 iterations, 16–32
   spinners, no sleeps) with `TASKBOARD_PTY_TEST_GAP_LOG` set —
   per D2's directory mode.
2. **Transcript-shape triage on every failure**: a transcript
   *without* the `cancelled` envelope ⇒ H1 confirmed (shutdown took
   the signal). A transcript *with* the envelope but no `B` row ⇒ the
   budget/interleaving side ⇒ D1's reorder is the relevant fix.
3. If H1 confirms, the **deliverable is the production fix**: close
   the disposition window — the running-command cancel handler must
   be armed before (or atomically with) the raw-mode teardown so a
   SIGINT between lines is always owned by the REPL cancel path while
   a command is executing; the lifecycle watcher yields. Typed pin:
   the existing flaky test *is* the pin (typed `cancelled` envelope +
   session survival); it becomes deterministic. If the fix turns out
   larger than the chunk (signal-ownership redesign), land D1 + D2 +
   the dated armed-hypothesis comment on #697 naming H1 and the
   reproduction recipe (honest not-fixed-yet per the round plan).

### D4 — #570: diagnosis method and both candidate fixes

**Two shapes, two owners** (both already visible in the record):

- **Early shape** (t16 @ 0.28 s, 2026-11-06): production race in the
  client router. `try_connect` (client.rs:122) returns `None`
  *immediately* on `ConnectionRefused` — on a UDS, refused means the
  socket artifact exists but nobody is accepting (mid-boot bind
  window, or the listener's accept loop stalled under CI load) — and
  misclassifies it as "no daemon" → in-process bootstrap → collides
  with the live lock holder. Exactly the 2026-11-01 comment's
  "transient local refusal mis-classified" hypothesis, now with a
  code anchor.
- **Late shape** (t16 @ :753 pre-mitigation, j20 @ :969 still): false
  test premise — `daemon:false` is the *socket* view; the graceful
  stop removes the artifact while the daemon drains (≤
  `shutdown_timeout_secs`) and still holds the instance lock. t16 got
  the kill+reap mitigation (its comment cites #570); **j20 never
  did**.

**Diagnosis method (stage-2, before landing)**:
1. Backgrounded loaded loop (30 iterations, spinners held, one
   parallel `cargo build --workspace` for the first iterations — the
   2026-11-04 recipe) over **t16 + j20 together**, with the transport
   trace enabled (`RUST_LOG` targeting the ipc client's
   `tracing::debug!` lines — stage-2 records the exact target after
   checking the client's span/log setup) so a
   refusal→bootstrap→locked sequence is visible in the captured
   stderr.
2. Per the standing capture policy: any recurrence records the full
   failure block, both payloads, and the trace before anything else.
3. Land the fixes the evidence confirms — expected: **both** (they
   are independent; A fixes the early shape, B the late).

**Candidate fix A (production — early shape)**: `try_connect` retries
`ConnectionRefused` within the existing `CONNECT_BUDGET` (1 s) /
`CONNECT_RETRY` (50 ms) instead of returning `None` immediately.
Rationale: refused ⇒ artifact exists ⇒ a daemon exists or is
mid-boot, so waiting is honest; `NotFound` (stale artifact ⇒ holder
dead ⇒ lock free) keeps the immediate `None` — bootstrap is the
*correct* outcome there. `PermissionDenied` is not ours to guess;
keep it immediate. Budget constants stay derived, not tuned. Typed
outcome unchanged (`Option<Connection>`). Tests: the existing
budget-expiry timing tests (client.rs ~592) extend to cover the
refused-retry path; a new test pins refused-then-served within budget
connects, and refused-throughout falls back bounded — typed
assertions, no sleeps (the existing staller-listener pattern at
`e10` provides the seam).

**Candidate fix B (test premise — late shape)**: port t16's kill+reap
mitigation to j20 — after `wait_until(daemon:false)`,
`let _ = daemon.0.kill(); daemon.0.wait().expect("reap the stopped
daemon")` before both fallback legs (`journal show` @ :969 and the
final export). A reaped process has released the flock by OS
semantics, so the fallback bootstrap is deterministic. Not a
weakening: zero assertions change; the premise is made true instead
of waited-out. No sleeps.

**Out of scope (backlog, see §6)**: the production-side stop
ordering — the client-observable `daemon:false` precedes lock release
by design today. The test-side mitigation (B) is the in-chunk answer;
changing the graceful-stop contract is its own design decision.

## 4. Implementation order (stage 2)

1. Commit 1 — #697 D1: `DESIGNED_SILENT_WINDOW` const + budget
   re-derivation + doc-comment replacement (`cli_tty.rs` only).
2. Commit 2 — #697 D2: `GapLog` directory mode + test-name threading;
   `ci.yml` env + failure-upload step.
3. Commit 3 — #697 D3 step 1–2: the backgrounded repro loop with gap
   logs; triage; either the production signal-ownership fix (typed
   pin already in tree) or the dated armed-hypothesis comment on
   #697.
4. Commit 4 — #570: fix A (`try_connect` refused-retry) with its
   typed tests.
5. Commit 5 — #570: fix B (j20 kill+reap), evidence comment on #570.
6. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo
   nextest run --workspace` (background + poll; defer per the rule).

## 5. Risks

- **D1 raises worst-case wedge latency 30 s → 70 s**: acceptable
  (floors not ceilings; backstop unchanged); watch the local gate
  wall time — nextest's own per-test timeouts are not affected.
- **H1 may be wrong**: the triage step (D3) is deliberately
  evidence-first; if failures stop reproducing entirely, the chunk
  still lands D1 + D2 (real determinism + future capture) and #697
  carries the dated armed hypothesis — an honest not-fixed-yet, not a
  speculative fix.
- **fix A widens the connect fallback latency by ≤ 1 s for every
  genuinely daemon-less verb on a stale artifact?** No — stale
  artifact is `NotFound` (immediate). Only refused-with-live-artifact
  waits, which is precisely the case where bootstrap would fail.
- **`ci.yml` artifact upload on failure adds a step to the required
  check**: upload failures are non-blocking (`continue-on-error` is
  the default for upload-artifact warnings, but stage-2 verifies the
  step cannot fail the job; pin `continue-on-error: true` if
  needed).
- **Duration const arithmetic**: `Duration * u32` / `Sum` in const
  position is stable, but stage-2 verifies the exact const-expr form
  compiles on the pinned toolchain; a `max()` of two const `Duration`s
  needs a small `const fn` — keep it trivial and documented.

## 6. Backlog issues (standing authorization)

- **New** `backlog:` issue — graceful-stop ordering: `daemon stop`
  removes the socket artifact (client sees `daemon:false`) before the
  daemon process releases the instance lock, so the documented
  "fallback works after daemon:false" contract has a ≤
  `shutdown_timeout_secs` window where it fails typed
  `instance-locked`; candidate directions: release-ordering change,
  or a typed post-stop state. Category `architecture`, `p3`. (Filed
  during stage 1 — number recorded in the stage report.)
- No other out-of-scope work discovered: #521 (the sibling raw-mode
  seam) is already cross-referenced on #697; j20's missing mitigation
  is in-scope (fix B).

## 7. Acceptance (restated from the dispatch plan, with owners)

- #697: the window < budget inequality holds by construction
  (`DESIGNED_SILENT_WINDOW` shared by route and derivation), k = 6
  + slack recorded; `Test Execution` runs with
  `TASKBOARD_PTY_TEST_GAP_LOG` on and uploads gap logs on failure;
  the side question is resolved with evidence (production fix or
  dated armed hypothesis).
- #570: the diagnosis recorded (per-shape owners), the fixes landed
  on the diagnosed side(s), typed-outcome asserted; no sleeps, no
  weakened assertions anywhere in the diff.
- Both issues close on fix evidence at merge (dispatcher) or carry
  the dated armed-hypothesis comments if the round exhausts.
