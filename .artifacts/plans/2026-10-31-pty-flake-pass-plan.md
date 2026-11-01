# [Plan] The pty flake pass — one progress-deadline seam for the cli_tty tier (#521 + #524 + #549; dispositions #287/#336/#384) — stage-1 plan

- **Chunk**: backlog round 2, chunk 1.10 (`.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` §3)
- **Branch**: `test/pty-flake-pass` (worktree `../taskboard-WT-test-pty-flake-pass`, created off `origin/main` @ `19f4c336` after `scripts/worktree-doctor.sh` passed: 234G free vs 25G floor)
- **Authored**: 2026-11-01 (filename date follows the dispatch round's naming, per the dispatch table)
- **Scope class**: test-harness-only. No production change (D7).
- **Stage-2 hand-off**: execute §5 (repro) → §6 (fix) → §7 (validation) → §8 (dispositions, drafted) → AGENTS §10 gates. No push, no PR (stage 3 ships).

## 1. Mission & binding constraints

The three sibling flakes — `tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives`
(#521), `tty_ctrl_c_during_the_masked_read_never_persists_and_never_wedges` (#524),
`tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes` (#549) — are one
mechanism with five+ observations in four days (2026-10-27 local gates lane; 2026-10-28
instrumented gates lane; 2026-10-29 ×2 local + GitHub CI runs 40951823545 / 40953446270).
This pass:

1. **Root-causes** the load sensitivity (§4 — verified against the tree, not inferred).
2. **Fixes deterministically per AGENTS §5**: an in-harness progress-deadline seam; never
   a sleep, never a bare larger constant (the constant re-derivation in S-4 is a
   documented consequence of the seam, not the fix — see D4 for the exact boundary,
   which stage 2 and stage 3 must both police).
3. **Pins under a recorded stress profile** (§5/§7): the fix must survive the recorded
   load shape AND isolation.
4. **Disposes the watchlist**: #521/#524/#549 close as fixed; #287/#336/#384 get the
   re-triage the flaky-test protocol owes them (§8) — each of the six issues ends the
   pass with a standalone disposition comment.

Binding rules for this chunk (AGENTS):

- **§5 determinism**: no sleeps as waits (the one pre-existing 500 ms signal-coalescing
  pacing sleep at `cli_tty.rs:1986` stays, untouched — it is pacing between two real
  kills, not a wait for state; D16). Waits are predicate + deadline. No quality
  ceilings. No text assertions (the prompt/marker strings are *synchronization gates*
  and stay never-asserted — the new seam must keep that framing in its docs).
- **§5 "no test weakened"**: every assertion in the touched file keeps its exact
  structure and message text (S-6/D9). A weakened assertion is a deviation to revert.
- **§7 flaky protocol**: this chunk *is* the dedicated pass. One issue per distinct
  test already exists; no opportunistic fixes ride along (D13).
- **§8 workflow**: worktree lifecycle per dispatch; commits land verbatim via rebase
  merge — each commit builds and reads cleanly; no "WIP".
- **§10 gates**: full sequence green before hand-off, via
  `scripts/run-quality-gates.sh`.
- **`#![forbid(unsafe_code)]`** applies to `taskboard-app` test targets: the seam is
  std-only (`std::thread`, `std::sync::mpsc`, `std::sync::Mutex`, `std::time::Instant`).
  No new dev-dependency (D7).

## 2. Verified seam facts (do not re-derive; verified 2026-11-01 against `19f4c336`)

### 2.1 The pty tier and its only bound

`crates/taskboard-app/tests/cli_tty.rs` is the suite's only pty tier (13 tests).
Every session is spawned as `timeout <30|60> script -qec <inner> /dev/null` — **14
spawn sites**: 4 named helpers — `run_login_under_pty` (:82, `30`),
`spawn_session_pty_exec` (:1423, `60`), `spawn_session_pty_probe` (:1775, `60`),
`spawn_oneshot_pty` (:2222, `60`) — and 10 inline `Command::new("timeout")` blocks
(:253 `60`, :368 `60`, :476 `60`, :612 `30`, :724 `30`, :808 `30`, :893 `60`,
:1180 `30`, :1264 `60`, :1309 `60`).

Every in-test wait on the session is an **unbounded blocking read**:

| Site | Helper / block | On EOF today |
|---|---|---|
| :59–68 | `feed_session_lines` prompt gate loop | break → assert w/ transcript (:69) |
| :196–208 | `wait_for_marker` loop | panic w/ transcript (:202) |
| :1074–1083 | `wait_for_readline` loop | break → assert w/ transcript (:1084) |
| :1096–1111 | `wait_for_prompt` loop | panic w/ transcript (:1105) |
| :1479–1491 | `wait_readline` (PtySession) loop | panic w/ transcript (**:1485 — #521's recorded crash site**) |
| :405–416 | #524's `survived` loop (prompt-count ≥ 2 or EOF) | break false/true, post-hoc asserts (**:429 — #524's recorded failure site**) |
| :634–647, :1863–1879, :1963–1979 | inline first-prompt loops | panic w/ transcript |
| :290, :424, :516, :962, :1289, :1336, :1554 | `read_to_end` output drains (`finish_session` :1539) | return on EOF |
| :284, :510, :956, :1566 | stderr `read_to_end` reader threads | return on EOF |
| :1722, :1981 | `in_flight.recv()` (in-flight dispatch gates) | unbounded channel recv |

When no data flows, the **only** thing that terminates any of these is the external
`timeout` killing `script`, which closes the pty: readers see EOF (→ panic/assert with
transcript) or `child.wait()` returns 124 (→ the wedge assertions
`assert_ne!(code, Some(124))`, e.g. :166, :296, :429, :523, :652, :747, :851, :969,
:1209, :1340, :1627, :1732, :1885, :1991, :2040).

### 2.2 The designed wall-clock costs inside the 60 s constant

The wrapper's budget must absorb, *in total*: test-binary scheduling under load,
`script`+`sh`+binary spawn, boot (core init, sqlite, actors, sync), wiremock round
trips, rustyline pacing per line, and exit. Three legs additionally contain *designed*
silent windows: #549's leg parks the dispatch on a 10 s delayed wiremock route
(:1687) with **zero pty output** until the delay expires; graceful drains are bounded
by `shutdown_timeout_secs = 5`. A legitimate loaded run therefore has both a long
total and multi-second silent gaps *by design*.

### 2.3 The suite's established budget idioms (the pty tier is the outlier)

- `common::LIVE_DEADLINE` (300 s named constant) + `common::with_deadline`
  (`common/mod.rs:329`, `:360`) — "a hung server fails the test instead of wedging CI".
- `common::wait_until(pred, what)` (`common/mod.rs:504`) — 30 s deadline, 50 ms poll,
  typed failure message.
- In-file deadline polls: `cli_tty.rs:713`, `:827` (`Instant` deadline + 50 ms poll).
- Channel budgets: `framed_rx.recv_timeout(30 s)` (:2325), `dropped_rx.recv_timeout(10 s)`
  (:2351).
- `it_cli.rs:532` also uses `timeout 30`, but on a **plain-pipe no-TTY one-shot**
  (`spawn_headless`) — a different mechanism, **out of scope** (verified).
- `.config/nextest.toml`: only the docker-tier serialization; **no `slow-timeout`
  overrides** — nextest never kills a test (the default 60 s SLOW *mark* explains the
  "60 s SLOW entries" in the observations; the kill is always the external `timeout`).

## 3. Root cause (the one mechanism)

The pty tier's only failure bound is an external **total-wall-clock constant** whose
value must absorb every slow step of a loaded machine; every in-test wait is an
unbounded block that relies on that external kill for termination. Under the recorded
load shapes (local load average >40 with co-agent builds; the instrumented
`run-quality-gates.sh` lane at 2079–2461 tests; 4-core GitHub CI runners), legitimate
sessions exceeded 60 s — 62 s observed repeatedly — the wrapper killed the pty, EOF
landed in the blocking reads, and the tests failed on assertions whose *names* say
"wedged". Slow-with-progress is structurally conflated with wedged-silent: exit 124 is
the only wedge signal the tests have, so `assert_ne!(code, Some(124))` silently asserts
two properties — "did not wedge" (intended, load-immune) and "finished within 60 s
total" (accidental, load-sensitive — the flake). The app is correct in both SIGINT
worlds by design (ADR 0011/0014, documented in-file); isolation runs and
coverage-lane reruns pass on the identical trees. **The defect is in the harness's
failure-detection mechanism, not in the product and not in the tests' semantics.**

## 4. The repro protocol (stage 2, pre-fix — records the baseline)

Goal: reproduce the recorded signature (exit-124 / wedge-assert / "pty ended before
readline N" under load) at least once on this tree, and record per-leg wall times as
the derivation data for S-4. A **negative repro is a recorded result** — proceed to
the fix regardless (D12; the mechanism stands on five concordant observations).

**Environment record (every leg)**: tree SHA, `nproc`, memory, nextest version, load
samples (`uptime` every 30 s into the stress log), the load-generator settings used.

**Load generator** (the recorded shape is co-agent *build* load, load >40):
`stress-ng --cpu <K>` if available, else K bounded shell spinners, sized so sampled
load holds ≥ 2× `nproc`, **plus** one real `cargo build --workspace` in a scratch
target dir (`/tmp/pty-stress-build`, removed after the legs — disk hygiene; the
doctor floor applies to this pass too). Record the actual tool and load curve.

- **R1 — full-suite lane** (matches all three primary observations): `cargo nextest
  run --workspace` under the load shape. Up to **3 iterations**; stop at first positive.
- **R2 — fast lane** (iteration cost ~minutes): `cargo nextest run -p taskboard-app
  --test cli_tty` under the load shape. Up to **10 iterations**; stop at first positive.
- **R3 — instrumented lane** (matches #524/#549's recorded lane; only if R1+R2 are
  negative): one full `scripts/run-quality-gates.sh` sequence with the load shape
  running *outside* the gate flock (the gate run itself stays serialized).
- **Positive repro** = any of the three legs failing with the recorded signature
  while the same tree passes `--test cli_tty` in isolation immediately after.
- **Capture per run**: per-test outcomes and wall times, failure signatures verbatim,
  SLOW entries, load samples.

## 5. The fix design, decision by decision

### S-1 — The seam: a progress-deadline reader per pty session

One **reader thread** per session owns `out_pipe`: it pushes each chunk into a shared
`Mutex<Vec<u8>>` (`seen`, byte order unchanged — single pipe, single reader) and
announces progress on a `std::sync::mpsc::Sender<PtyProgress>` with
`enum PtyProgress { Bytes, Eof }`. Every wait becomes:

```text
loop {
    if predicate(seen)  -> return Satisfied
    match progress_rx.recv_timeout(PTY_IDLE_BUDGET) {
        Bytes     -> continue,               // progress; keep waiting for the predicate
        Timeout   -> kill child, reap, panic typed:
                     "no pty output for {PTY_IDLE_BUDGET:?} (progress budget): \
                     the session wedged or starved; transcript: {seen}",
        Eof       -> return Eof,             // caller keeps its exact today-behavior
    }
}
```

- **Why idle-progress, not total** (D3): the pinned semantics are "never wedges,
  terminates, correct outcomes" — *not* "terminates within 60 s of wall clock". A
  wedge is silence; a loaded machine is slowness with continued progress. The idle
  budget is load-immune for exactly the recorded failure class.
- **Why channel, not Condvar** (D2): `recv_timeout` is the suite's existing
  process-sync idiom (`:2325`, `:2351`); single consumer; no poison handling.
- **EOF policy per call site** (D8): the seam *returns* `Eof` and each caller keeps
  its exact current on-EOF behavior (the `wait_for_readline` break→assert path, the
  `wait_for_marker`/`wait_for_prompt`/`wait_readline` panic-with-transcript paths,
  #524's `survived = false` path). No message text changes.
- **Drains**: `finish_session`'s `read_to_end` becomes the same receive loop until
  `Eof` (a wedge at drain = silence → idle budget fires → kill → typed failure). The
  stderr reader threads stay `read_to_end`; after any kill, pipes EOF and they return
  promptly.
- **Channel gates**: `in_flight.recv()` (:1722, :1981) become
  `recv_timeout(PTY_IDLE_BUDGET)` — same typed failure class.
- **Stdin writes stay blocking** (payloads ≪ pty input buffer; unchanged today-risk).
- **Kill discipline**: on idle-timeout the seam kills and reaps the child *before*
  panicking, so a failing assertion cannot leak a session (mirrors `common::Child`'s
  Drop guard intent).

### S-2 — Where the seam lives

In `cli_tty.rs`, extending the existing `PtySession` machinery (D10): it is the pty
tier's file and its only consumer today; `common/` promotion happens if a second
consumer ever appears. No `common/mod.rs` changes.

### S-3 — One spawn constructor, derived backstop

The 14 `timeout` spawn sites collapse into one constructor that embeds the derived
backstop (`timeout <PTY_SESSION_BACKSTOP>`) and takes the per-test env additions —
`TERM=xterm` exactly where it is set today (the marker-gated pacing needs it),
`XDG_DATA_HOME` always, per-test extras verbatim. The 30-vs-60 split disappears into
the single derived value (D4/D5). Per-test invariants to preserve, hunk by hunk:
args, env, the gating marker used, and every assertion.

### S-4 — The budget numbers: derived, floors, telemetry

- `PTY_IDLE_BUDGET = max(30 s, 3 × G_max)` — G_max = the largest *legitimate*
  inter-chunk silence measured under the §4 stress profile (telemetry below). The
  30 s floor is justified by construction: #549's leg is silent ~10 s by design
  (the delayed route), ×3 headroom.
- `PTY_SESSION_BACKSTOP = max(120 s, 3 × T_max)` — T_max = the largest legitimate
  per-test total under the same profile. The 120 s floor covers the recorded CI
  evidence (62 s legit runs) with margin.
- Stage 2 **measures and fills** the numbers; the code comment carries the formula,
  the measured values, and the profile they came from. If G_max lands within 2× of
  its floor, that is a **finding to report** (thin headroom), not a silent bump.
- **Telemetry**: the reader thread appends timestamped chunk sizes to
  `TASKBOARD_PTY_TEST_GAP_LOG=<path>` when the variable is set (off by default; used
  by §4/§7 runs to derive G_max/T_max). Diagnostic-only, no behavior change; stage 2
  may drop it with a recorded reason if it obstructs (D10).
- **No env knob that sets the budgets** (D11) — a lane-configurable number is the
  banned finding class laundered through configuration.

### S-5 — Conversion table (every unbounded block routes through the seam)

Shared helpers (`feed_session_lines`, `wait_for_marker`, `wait_for_prompt`,
`wait_for_readline`, `wait_readline`, `feed_line`, `feed_at`) convert once; the
per-test inline blocks convert mechanically: #524's `survived` loop (:405), the three
first-prompt loops (:634, :1863, :1963), the seven output drains (§2.1 table), the
two `in_flight.recv()` gates. `finish_session` (:1539) drains via the seam. After
conversion, **no unbounded blocking read on session output remains in the file** —
the only remaining bound from outside is the backstop, which legitimate runs no
longer exercise.

### S-6 — What does NOT change

- **Every assertion**: structure, order, and message text byte-identical — including
  all `assert_ne!(code, Some(124))` wedge pins (the backstop can still legitimately
  produce 124 on a real runaway; the pin keeps its meaning; D9).
- **Test semantics**: what each test pins (nothing persists; typed outcomes; session
  survives; discipline hand-back) is untouched — the fix changes *how waits are
  bounded*, never *what is asserted*.
- **No production change** (D7): the app is correct in both SIGINT worlds by design;
  all evidence is contention-shaped.
- **No new dev-dependencies, no unsafe, no new sleeps** (D7/D16).
- **The 500 ms pacing sleep** (:1986) and its comment stay (D16).
- **The promptness pin** `assert!(started.elapsed() < 30 s)` at :178 stays — no
  observation against it in 13 tests × many runs; changing pins without evidence is
  scope creep. Recorded as a residual watch item (D14).
- **`it_cli.rs`, every other test file, nextest config**: untouched (verified
  non-pty / no kill mechanism to change).

## 6. Implementation order (stage-2 guidance)

1. §4 repro legs R1→R2(→R3); record outcomes either way.
2. Seam first (S-1/S-2: reader thread, progress enum, wait primitive, budgets as
   named consts with the S-4 derivation comment), then the spawn constructor (S-3),
   then the mechanical conversion (S-5) — the file compiles green at each step.
3. Fill measured G_max/T_max from the repro/stress telemetry into the budget consts.
4. §7 validation legs V1–V5, in order, all recorded.
5. AGENTS §10 full gate suite via `scripts/run-quality-gates.sh`.
6. Draft the six dispositions (§8) — stage 3 posts them.
7. Commit hygiene for rebase-merge: `docs(plan): pty flake pass` (this plan, first),
   then a single `test(app): the pty harness progress-deadline seam` carrying the
   whole mechanical conversion (each commit builds standalone). No push, no PR.

## 7. The stress-validation protocol (acceptance)

Same environment record and load generator as §4.

- **V1**: 3× `cargo nextest run --workspace` under the load shape — all green.
- **V2**: 10× `-p taskboard-app --test cli_tty` under the load shape — all green;
  record per-test wall times and G_max'/T_max' telemetry.
- **V3**: 1× instrumented lane (`scripts/run-quality-gates.sh` full sequence) — green.
- **V4**: 3× `--test cli_tty` isolated (no load) — green (the "in isolation"
  acceptance from the dispatch).
- **V5 — the seam's load-bearing control**: a temporary `#[ignore]` scratch test
  points the seam at a silent inner (`script -qec 'sleep 300'`) and asserts the typed
  idle-timeout failure fires at ≈ the idle budget with the transcript attached; run
  once via `nextest ... -- --ignored`, then **delete** (AGENTS §5: no committed tests
  of test-harness internals; this is a one-time validation action, evidence recorded
  in the stage-2 report — D15).
- **Acceptance**: V1–V4 green + V5 demonstrated; `G_max' ≤ PTY_IDLE_BUDGET / 3`
  (headroom holds, otherwise re-derive with the data and record); numbers recorded.
- **Failure handling**: a V-leg failure with a *wedge signature* (genuine app hang)
  is a production finding — stop, new issue, do not widen scope; a *budget
  false-positive* is re-derived from telemetry with the numbers recorded — never
  silently bumped.

## 8. Dispositions (drafted by stage 2, posted by stage 3)

Every disposition comment **stands alone**: a reader must reconstruct the story from
the comment alone (observation recap → root cause → evidence → decision → what the
next occurrence must capture). Recurrence scans are bounded: the issue's own comment
thread is the primary record, plus one `gh run list --limit 30` pass over the newest
runs with `gh run view <id> --log-failed` grep for the exact test names — no
full-history scraping.

- **#521 / #524 / #549 — close as fixed.** Comment: recap of the issue's own
  observation(s); root cause (§3, one paragraph); the fix (the seam + derived
  budgets, PR link); validation evidence (V1–V5 numbers, isolation + stress);
  closing statement that the `flaky-test`/`watchlist` watch ends with the fix
  (closing is the archive; no deletion).
- **#287 (`actor_wiremock` family) — re-triage, refresh capture policy, keep OPEN.**
  Steps: (1) recurrence scan since 2026-10-16 (comments + bounded CI scan for
  `actor_wiremock` / the two named cases). (2) No recurrence → refresh comment:
  restate the standing capture policy verbatim ("capture the failing test name and
  the `--nocapture` output before re-running"), carry the scope-broadening note
  (plain nextest + coverage lane both, one crate, green runs between), keep open on
  the watchlist. (3) **No code change**: no repro → no speculative fix — the house
  ruling recorded in #384's disposition applies (D13). (4) Recurrence found →
  capture per policy; fixing is outside this branch's scope (sync-crate tests):
  comment the evidence, name a dedicated-chunk candidate. **Not** archive-closed:
  two weeks quiet is not recurrence-free evidence for a family that already
  broadened scope once (2026-10-16).
- **#336 (`delete_404_re_resolves_and_deletes_at_the_fresh_binding`) — re-triage,
  refresh capture policy, keep OPEN.** Steps: (1) recurrence scan since 2026-10-18
  (comments + bounded CI scan). (2) No recurrence → refresh comment: the structural
  facts stand (four `.expect(1)` mocks + `server.verify()`; the 0.021 s failure
  points at mock setup/verify, unconfirmed); the capture policy is restated as "on
  the next occurrence record the full nextest failure output verbatim (nextest
  prints failing-test output by default — the 2026-10-18 loss was run-output
  discipline, not nextest's)". Keep open. (3) No code change (D13).
- **#384 (`l13_alias_happy_path...`) — re-triage, confirm the landed policy, keep
  OPEN.** Steps: (1) verify the capture-policy commit `878169a` (PR #479) is on
  `origin/main`. (2) Recurrence scan since 2026-10-21. (3) Refresh comment: the
  policy stands — the assert now carries the buffer bytes, so the next occurrence's
  verbatim panic message *is* the root-cause evidence, recorded with lane, load
  shape, and commit; note the already-spent green repro protocol (3× full
  workspace, 10× crate, 1× instrumented) makes the next occurrence high-signal.
  Keep open. (4) No code change (D13).

## 9. Decision record (every open question, resolved)

- **D1 — Root cause is harness mechanism, not app behavior.** Five concordant
  observations across three lanes (local, instrumented, CI), isolation passes on
  identical trees, unchanged app semantics. §3.
- **D2 — Fix class: progress-deadline seam.** Rejected: a bare larger constant
  (banned by AGENTS §5 and the dispatch; it re-fails at the new tail); a pure
  in-test *total* budget (any total value re-fails the recorded legitimate-slow
  shape — that is the flake); a nextest `slow-timeout` kill (config-side kill, same
  exit-124 conflation, no transcript improvement).
- **D3 — Budget semantics: idle-progress (silence), not total.** The pinned
  semantics are wedge-freedom and termination, not wall-clock speed; slowness with
  progress is not the failure the tests exist to catch. Residual risk in §10.
- **D4 — The backstop re-derivation is sanctioned and is not the fix.** The seam is
  the fix; `60 → derived` is a documented consequence (legitimate runs legally
  exceed 60 s). The banned finding class is a number change *without* the mechanism
  change — stage 2's "stop if your diff changes a timeout NUMBER upward" rule is
  satisfied by this explicit plan sanction, and stage 3 must verify the seam is
  present and load-bearing (V5), not merely that the numbers moved.
- **D5 — Values come from measurement with floors**: `idle = max(30 s, 3×G_max)`,
  `backstop = max(120 s, 3×T_max)`; floors justified in S-4; stage 2 fills the
  measured numbers; thin headroom is a reported finding.
- **D6 — Scope: all pty sessions in `cli_tty.rs`** route through the seam — the
  shared helpers make a partial conversion incoherent, and leaving any unbounded
  block re-seeds the same watchlist entry on its next observation. The three flaky
  legs are the acceptance-pinned legs. `it_cli.rs` is out of scope (verified
  plain-pipe).
- **D7 — Test-harness-only; std-only; no new deps; no unsafe; no new sleeps.**
- **D8 — EOF semantics preserved per call site** (the seam returns `Eof`; callers
  keep their exact panic/assert paths and message text).
- **D9 — All assertions byte-identical; the 124 wedge pins stay** (the backstop
  remains a real bound; a real runaway must still fail).
- **D10 — Telemetry is env-gated and droppable** with a recorded reason.
- **D11 — No env knob for the budgets** (a lane-tunable number is the banned class
  laundered through configuration; re-derivation happens in code with data).
- **D12 — A negative repro does not block the fix** (dispatch rule; the validation
  protocol plus V5 carry the burden a positive repro would have carried).
- **D13 — Dispositions**: #521/#524/#549 close-as-fixed; #287/#336/#384 refresh and
  keep open; no code changes for the old trio (no repro → no speculative fix, the
  #384 precedent).
- **D14 — The `elapsed < 30 s` promptness pin (:178) stays**, recorded as a residual
  watch item.
- **D15 — V5's sabotage test is temporary and deleted**, evidence in the stage-2
  report (AGENTS §5: no committed tests of test-helpers).
- **D16 — The 500 ms pacing sleep (:1986) stays** — signal-coalescing pacing between
  two real kills, not a wait for state (its own comment says so).
- **D17 — Worktree/branch**: created at planning after the doctor passed (234G ≥ 25G
  floor): `../taskboard-WT-test-pty-flake-pass` on `test/pty-flake-pass` off
  `origin/main` @ `19f4c336`. Stage 3 tears it down per the dispatch after PR-green.

## 10. Risks

- **Idle-budget false positive under extreme descheduling** (a single legitimate
  gap > the budget): we have no gap-distribution data until the telemetry runs —
  that is what S-4's measurement is for. The failure, if it ever happens, is typed
  and names the budget (diagnosable, re-derivable), strictly better than today's
  exit-124 conflation. The 30 s floor covers the only *designed* silent window
  (10 s) with 3× headroom.
- **CI runners (4-core) differ from the local box**: the floors cover the recorded
  CI evidence (62 s); the PR's required checks exercise the tier on CI runners
  before merge, which is the real CI validation.
- **Broad-but-shallow diff**: 14 spawn sites + ~15 wait blocks across 13 tests;
  review risk mitigated by byte-identical assertions, the S-5 conversion table, and
  one commit carrying one mechanical pattern.
- **Negative repro** (the load shape may not reproduce locally): handled by D12;
  V5 proves the detection mechanism independently of nature cooperating.
- **Stress artifacts on disk**: the scratch build target and stress dirs are removed
  per leg; the doctor floor was checked at planning (234G free).
- **Budget drift over time**: the derivation comment in code names the profile and
  date; future re-derivations follow the same formula instead of nudging numbers.

## 11. Estimated diff size

`crates/taskboard-app/tests/cli_tty.rs` only: ≈ +170/−130 lines (seam ~80, spawn
consolidation ~−60 of duplication, conversion hunks mechanical, telemetry ~15).
No other file changes. Stage 3's PR: `test(app): the pty harness budget seam`
(per the dispatch's shipping step).
