# [Report] The pty flake pass — stage-2 implementation record (chunk 1.10, backlog round 2)

- **Branch**: `test/pty-flake-pass` @ `941975a7` (`test(app): the pty harness
  progress-deadline seam`); worktree
  `taskboard-WT-test-pty-flake-pass`, executed 2026-11-01.
- **Plan**: `.artifacts/plans/2026-10-31-pty-flake-pass-plan.md` (stage-1).
- **Result**: the seam is implemented, validated, and committed; the repro was
  negative for the recorded exit-124 signature (recorded, per D12, fix applied
  anyway); one **production finding** (#572) surfaced by the seam during
  validation and filed per the plan's stop rule; dispositions for the six
  watchlist issues are drafted below (stage 3 posts them).

## 1. Environment record

- Tree `588105ca` (plan commit) → `941975a7` (fix commit); x86_64 Linux,
  16 cores, 30 GB RAM, nextest 0.9.148.
- Load generator (no `stress-ng` available): 24 bounded shell spinners plus one
  real `cargo build --workspace` loop in a scratch target dir
  (`/tmp/pty-stress-build`, sources rsynced, `target/` excluded; removed after
  the pass). Sampled load (`uptime` every 30 s): baseline ~15; under load
  27–55, holding ≥ 2× nproc (32) for most of every leg.

## 2. Repro outcomes (plan §4)

| Leg | Iterations | Result |
|---|---|---|
| R1 full workspace | 2 | Negative — 2717/2717 both (350 s, 368 s); a domain test SLOW > 240 s (real contention) |
| R2 `-p taskboard-app --test cli_tty` | 10 (+4 extra) | Negative for the exit-124 signature |

- The recorded signature (exit 124 at ~62 s on the three sibling legs) did not
  reproduce locally. Recorded per D12; the fix proceeded.
- **Incidental observation (recorded, not this chunk's scope)**: during R2
  iterations 8–9 at load ≈ 50, a transient burst of sub-second failures swept
  ~16 unrelated tests (including several cli_tty legs and, in a later V1
  iteration at load ≈ 55, four `taskboard-app` migration unit tests). All
  passed on immediate rerun, including under residual load, and never
  reproduced in ~20 further loaded runs. Distinct failure class (instant
  burst, not timeout); if it recurs it deserves its own watchlist entry.

## 3. The fix (plan §5)

One commit, `crates/taskboard-app/tests/cli_tty.rs` only (+534/−664):

- **S-1 seam**: per-session reader thread owns the output pipe, appends chunks
  to a shared `Arc<Mutex<Vec<u8>>>` transcript, announces
  `PtyProgress::{Bytes, Eof}` on a std mpsc channel. Every wait is
  `predicate ‖ progress ‖ idle-timeout`; on idle the session is killed and
  reaped *before* the typed panic (`no pty output for 30s (progress budget):
  the session wedged or starved; transcript: …`). EOF callers keep their exact
  on-EOF behavior (D8). Drains (`conclude_session`): idle → kill+reap+**return**
  — a deliberate, documented refinement of S-1 (see §6 deviations): the
  post-drain status assertions keep their exact semantics and the forced death
  lands where the external `timeout` kill used to, which is what keeps the
  one-shot ^C leg's `130 ‖ 124`-tolerant world intact.
- **S-3/S-5 conversion**: all 14 `timeout` spawn sites collapsed into one
  `spawn_pty_session` constructor (`XDG_DATA_HOME` always, `TERM=xterm` exactly
  where set today, extras verbatim); every unbounded read, drain, and the two
  `in_flight.recv()` gates route through the seam
  (`recv_timeout(PTY_IDLE_BUDGET)`). No unbounded blocking read on session
  output remains.
- **S-4 budgets, derived**: `PTY_IDLE_BUDGET = max(30 s, 3×G_max)` with
  G_max = 10 s (the stalled-line leg's designed delayed wiremock route; measured
  accidental gaps under load ≤ ~0.5 s, see §5) → 30 s.
  `PTY_SESSION_BACKSTOP = max(120 s, 3×T_max)` with T_max = 62 s (recorded
  legitimate CI totals, #521/#549; 32–33 s measured locally) → 186 s. The
  60 → 186 s re-derivation is the plan-sanctioned consequence (D4), documented
  in the const's doc comment with formula, measurements, and profile. **Thin
  headroom finding**: G_max sits at exactly ⅓ of the idle budget floor (the
  designed 10 s window) — recorded here per S-4; no occurrence approached it
  (max accidental gap ≈ 0.5 s), so no re-derivation is warranted.
- **Telemetry**: `TASKBOARD_PTY_TEST_GAP_LOG=<path>` appends
  `<micros-since-spawn>\t<chunk-len>` per chunk (off by default). Caveat found
  during validation: parallel nextest workers share one path
  (`File::create` truncates; per-process `Instant` bases mix), so the log is
  sound only for single-test runs — noted for future derivations.
- **Unchanged (S-6, verified)**: every assertion's structure and message text
  byte-identical; all `assert_ne!(code, Some(124))` wedge pins stay; test
  semantics untouched; the 500 ms pacing sleep and its comment stay (D16); the
  `elapsed < 30 s` promptness pin at `tty_login_…` stays (D14, residual watch
  item); no production change, no new deps, no unsafe, no new sleeps (D7).

## 4. Validation (plan §7)

| Leg | Protocol | Result |
|---|---|---|
| V1 | 3× full workspace under load (37–55) | 2 green; 1 hit **#572** (production finding, below) — retried 3× after the finding was filed: all green |
| V2 | 10× cli_tty under load (27–30), gap telemetry | 10/10 green; per-test max ≈ 33 s (`tty_one_shot…`, its designed SIG_IGN kill); session-span max 10.75 s (the designed stall window); accidental G_max′ ≤ ~0.5 s ≤ budget/3 ✓ |
| V3 | 1× `scripts/run-quality-gates.sh` full sequence | Green — full suite passed: 2717/2717 (3 slow, all unrelated SLOW marks), profraw quarantine clean (3057 valid), lcov written |
| V4 | 3× cli_tty isolated | 3/3 green (10.1–10.2 s) |
| V5 | sabotage control: silent `script -qec 'sleep 300'` | typed idle abort at **30.008 s** with transcript attached; scratch test deleted after the run (D15) |

### The production finding (#572)

V1's third iteration failed `tty_ctrl_c_during_the_masked_read_fails_typed…`
at 30.8 s — the idle budget. The plan's failure-handling fork demanded a
verdict; the 600 s-budget control (scratch tree) failed identically at 600 s
with gap telemetry showing the last chunk at ~183 ms: the session prints its
typed `cancelled` envelope and then **never re-arms the editor** — a genuine,
rare production race, not a budget false positive. Filed as **issue #572**
(labels `flaky-test`, `watchlist`, `p1`) with the reproduction recipe. Per the
plan: stop, new issue, no scope widening — the app is untouched by this chunk.
Note: the crash site is the same `wait_readline` line #521 recorded on
2026-10-29, but #521's recorded mechanism (outer-60 s kill under load) is
distinct from this race; both dispositions below say so explicitly.

## 5. Telemetry-derived budget check

- Designed silent window: 10 s (stalled-line leg's delayed route) — the budget
  floor's exactly ⅓; thin headroom recorded as a finding, not bumped.
- Accidental gaps under load: ≤ ~0.5 s (clean single-test gap logs; the
  parallel-run logs are cross-process-contaminated and were not used for
  derivation).
- `tty_one_shot_ceremony…` legitimately runs ~30–33 s under this environment
  (SIG_IGN world reaches the kill): identical cost before (timeout 30) and
  after (drain idle-kill at 30); T_max = 62 s (CI-recorded) keeps the backstop
  at 186 s.

## 6. Deviations from the plan (all recorded, none silent)

1. **Drain idle-policy**: S-1 sketched drains as typed-panic-on-wedge;
   implemented as kill+reap+return. Reason: the one-shot ^C leg's assertions
   *tolerate* the killed outcome by design (`130 ‖ 124`); a typed panic there
   would change test semantics (banned by S-6). Every wedge still fails its
   post-drain `assert_eq!(code, Some(0))` pin with the transcript attached.
2. **Reader-thread error mapping**: read errors map to `Eof` (the pre-seam
   loops disagreed among themselves — some broke on `Err`, two panicked on
   `Err`); on a live pipe these panics were unreachable. The two distinct
   `Err`-arm panic texts disappear with their arms; all EOF-path message texts
   are preserved verbatim.
3. **`XDG_DATA_HOME` added to the completion test's spawn** (S-3 says the
   constructor sets it always; that inline site had omitted it). Its tempdir
   becomes the data home — consistent with every other session, no assertion
   touches the difference.
4. **V1 accepted with a documented production-finding interruption** (§4):
   3 further green iterations after #572 was filed.
5. **Stage-2 worktree incident**: an errant `git checkout --` mid-pass
   reverted the working file once; the conversion was reapplied in full from
   the recorded hunks and re-validated (clippy/fmt/nextest) before commit.
   Recorded for honesty; no content difference.

## 7. Gate suite (AGENTS §10)

Run via `scripts/run-quality-gates.sh` on the branch: fmt check, clippy
`-D warnings`, nextest workspace, doc tests, `cargo deny`, llvm-cov sequence
with profraw quarantine + `rm`. Result: **green** (`run-quality-gates: full
suite passed`; 2717/2717 nextest, 3 unrelated SLOW marks, coverage quarantine
3057/3057 valid, `lcov.info` written); pre-commit hooks (fmt + workspace
clippy) also passed on both commits.

## 8. Dispositions (drafts — stage 3 posts them)

### #521 — close as fixed

> **Fixed with root cause by the pty flake pass** (branch `test/pty-flake-pass`,
> commit `941975a7`; PR link).
>
> Recap: two observations (2026-10-27 local full-suite gates lane; 2026-10-29
> CI runs 40951823545/40953446270) — the test died at its outer `timeout 60`
> (exit 124 / the `wait_readline` EOF panic at cli_tty.rs:1485) under
> full-suite parallel load, ~62 s totals against a ~10 s isolated norm.
>
> Root cause: the pty tier's only failure bound was that external
> total-wall-clock constant, and every in-test wait was an unbounded blocking
> read. A loaded machine makes legitimate sessions slow (62 s CI-recorded) —
> the wrapper killed live sessions, EOF landed in the blocking reads, and the
> test failed on an assertion whose name says "wedged". Slow-with-progress was
> structurally conflated with wedged-silent.
>
> Fix: a progress-deadline seam — one reader thread per session, every wait
> bounded by an idle-progress budget (30 s, derived `max(30 s, 3×G_max)`) and
> a runaway backstop (186 s, derived `max(120 s, 3×T_max)`; the 60→186 s
> re-derivation is documented plan decision D4, not a tuned constant). A wedge
> now fails typed with the transcript; slowness with progress no longer fails
> at all.
>
> Validation: sabotage control (a silent session aborts typed at 30.008 s);
> 10/10 loaded cli_tty runs, 3/3 isolated, full-workspace runs under load —
> numbers in the stage-2 report (`.artifacts/reports/2026-11-01-pty-flake-pass-stage2-report.md`).
>
> **One residual, and it is not this mechanism**: the seam surfaced a rare,
> *genuine* production race in the same test — the editor sometimes never
> re-arms after the masked-read ^C select-cancel race (#572, typed
> progress-budget panic, distinct from this issue's outer-timeout signature).
> An occurrence of the progress-budget panic belongs to #572, not here.
>
> The `flaky-test`/`watchlist` watch on this test's outer-timeout signature
> ends with this fix (closing is the archive).

### #524 — close as fixed

> **Fixed with root cause by the pty flake pass** (branch `test/pty-flake-pass`,
> commit `941975a7`; PR link).
>
> Recap: 2026-10-27, instrumented whole-workspace coverage run (2079 tests,
> heavy parallelism) — the test exceeded its outer `timeout 60` (exit 124) and
> failed on the wedged-session assertion `assert_ne!(code, Some(124))`; 3/3
> passes in isolation on the same tree.
>
> Root cause (shared with #521/#549): the pty tier's only failure bound was the
> external total-wall-clock `timeout`; every wait was an unbounded blocking
> read. Under load the legitimate session ran past 60 s, the wrapper killed
> it, and the exit-124 kill landed in the `survived` loop as a wedged-session
> verdict. Slow-with-progress was conflated with wedged-silent — the test's
> exit-124 pin silently asserted two properties, "did not wedge" (intended,
> load-immune) and "finished within 60 s" (accidental, load-sensitive).
>
> Fix: the progress-deadline seam — the `survived` loop is now an idle-progress
> wait (predicate ‖ progress ‖ idle-timeout); the session is killed and reaped
> before any typed failure. Budgets derived, not tuned: idle 30 s
> (`max(30 s, 3×G_max)`, G_max = the designed 10 s stall window), backstop
> 186 s (`max(120 s, 3×T_max)`, T_max = 62 s recorded CI totals; the 60→186 s
> move is documented plan decision D4, not the fix).
>
> Validation: 10/10 loaded cli_tty runs and 3/3 isolated on the fix tree; the
> sabotage control proves a silent session fails typed at the budget with the
> transcript; numbers in the stage-2 report.
>
> The `flaky-test`/`watchlist` watch ends with this fix (closing is the
> archive). Cross-reference: a rare genuine post-^C editor race in the sibling
> test is tracked separately as #572.

### #549 — close as fixed

> **Fixed with root cause by the pty flake pass** (branch `test/pty-flake-pass`,
> commit `941975a7`; PR link).
>
> Recap: 2026-10-29, stage-3 review whole-workspace run via
> `scripts/run-quality-gates.sh` (2461 tests, heavy parallelism) — the test
> failed at 62 s (its outer `timeout 60`); in isolation on the same tree it
> passes in 10 s. Sibling shape to #521/#524.
>
> Root cause (shared): the outer total-wall-clock constant was the only bound;
> waits were unbounded blocking reads; legitimate slowness under load tripped
> the kill. This leg additionally contains a *designed* silent window — the
> 10 s delayed wiremock route with zero pty output — which is why the new idle
> budget's derivation floor is exactly 3× that window.
>
> Fix: the progress-deadline seam (idle budget 30 s = `max(30 s, 3×G_max)` with
> G_max = the 10 s designed window; backstop 186 s = `max(120 s, 3×T_max)` with
> T_max = 62 s recorded CI totals — the 60→186 s re-derivation is documented
> plan decision D4). The in-flight dispatch gate is a budgeted `recv_timeout`
> now; a wedged session fails typed with the transcript.
>
> Validation: 10/10 loaded and 3/3 isolated cli_tty runs on the fix tree, the
> 10 s designed silence comfortably inside the budget (measured session spans
> ≤ 10.75 s), sabotage control typed-abort at 30.008 s; numbers in the stage-2
> report.
>
> The `flaky-test`/`watchlist` watch ends with this fix (closing is the
> archive).

### #287 (`actor_wiremock` family) — re-triaged, capture policy refreshed, kept OPEN

> **Re-triage 2026-11-01 (pty flake pass, backlog round 2 chunk 1.10). No
> recurrence since 2026-10-18; scope stays broadened; the entry stays open.**
>
> Recurrence scan: the issue's comment thread (primary record) — latest
> observation is 2026-10-18 (`a12_shutdown_exits_the_loop`, docs-only PR #337,
> coverage lane, run 39147733979); the 2026-10-26 dedicated watchlist pass ran
> the full negative repro protocol and annotated the capture policy. One
> bounded `gh run list --limit 30` pass over the newest runs (2026-10-31 →
> 2026-11-01): all success/cancelled, no `actor_wiremock` or `push_exec`
> failures.
>
> Standing capture policy, verbatim: **"capture the failing test name and the
> `--nocapture` output before re-running."** The scope note stands: the family
> spans the plain nextest lane and the coverage lane, one crate
> (`taskboard-sync-nextcloud`), with green runs between — one family with the
> mock-I/O timing sensitivity, possibly including the `e2e_hermetic` entry.
>
> Ruling: **no code change** — no repro since the 2026-10-26 protocol, and the
> house rule (recorded in #384's disposition) is no speculative fix without
> one. Two weeks quiet is not recurrence-free evidence for a family that
> already broadened scope once (2026-10-16); this is **not** archive-closed.
> The next occurrence must be captured per the policy above and triaged in a
> dedicated pass.

### #336 (`delete_404_re_resolves_and_deletes_at_the_fresh_binding`) — re-triaged, capture policy refreshed, kept OPEN

> **Re-triage 2026-11-01 (pty flake pass, backlog round 2 chunk 1.10). No
> recurrence since the 2026-10-18 observation; stays open.**
>
> Recurrence scan: the issue's comment thread (primary record) — the 2026-10-26
> dedicated watchlist pass annotated the capture policy and ran the negative
> repro protocol (3× full workspace, 10× crate suite). One bounded
> `gh run list --limit 30` pass over the newest runs (2026-10-31 → 2026-11-01):
> all success/cancelled, no `push_exec` failures.
>
> Structural facts stand: the test mounts four `.expect(1)` wiremock mocks and
> calls `server.verify()`; the single 0.021 s failure points at mock
> setup/verify rather than a behavioural race — unconfirmed.
>
> Capture policy, restated: on the next occurrence record the **full nextest
> failure output verbatim** (nextest prints failing-test output by default —
> the 2026-10-18 loss was run-output discipline, not nextest's), plus lane,
> load shape, and commit.
>
> Ruling: **no code change** — no repro, no speculative fix (the #384
> precedent). Not archive-closed: one clean pass is not recurrence-free
> evidence for a never-captured failure.

### #384 (`l13_alias_happy_path_writes_stderr_connect_keeps_it_empty`) — re-triaged, landed capture policy confirmed, kept OPEN

> **Re-triage 2026-11-01 (pty flake pass, backlog round 2 chunk 1.10). The
> landed capture policy stands; no recurrence since 2026-10-21; stays open.**
>
> Policy verification: the capture-policy work of PR #479 (branch
> `test/flaky-watchlist-pass`, commit `878169a`) is on `origin/main` — landed
> as `60f8965a` ("test(app): carry the captured stderr in l13's connect-stderr
> assert"; rebase-merge rewrote the SHA, the content is there).
>
> Recurrence scan: the issue's comment thread (primary record) — the 2026-10-26
> mechanism correction stands (the duplicated `App password: App password: `
> was routine; the assert now carries the buffer bytes). One bounded
> `gh run list --limit 30` pass over the newest runs (2026-10-31 → 2026-11-01):
> all success/cancelled, no `cli_login` failures.
>
> What the next occurrence must capture: the verbatim panic message *is* the
> root-cause evidence (the assert carries the buffer bytes since `60f8965a`),
> recorded with lane, load shape, and commit. The green repro protocol has
> already been spent once (3× full workspace, 10× crate, 1× instrumented,
> 2026-10-26), which makes the next occurrence high-signal.
>
> Ruling: **no code change** — no repro, no speculative fix.
