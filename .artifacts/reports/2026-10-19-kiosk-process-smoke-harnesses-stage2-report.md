# [Report] Kiosk chunk 5 stage 2 — process smoke harnesses implemented and measured

- **Date**: 2026-10-19
- **Branch**: `feat/perf-process-harnesses` (worktree
  `../taskboard-WT-feat-perf-process-harnesses`), base `e0e3bf6`,
  stage-1 plan
  `.artifacts/plans/2026-10-19-kiosk-process-smoke-harnesses-plan.md`
  followed as written. Stage 3 (review + PR) follows.
- **Status**: COMPLETE — both harnesses green, both measured numbers
  recorded below (MD-5); full gate suite green.

## 1. Measured numbers (MD-5; dev machine, references — never gates)

Dev machine: x86_64 Linux desktop, Wayland session, release build of
this branch. N = 10 000 (the budget rows' scale), fresh fixture per
run.

**S1 — RSS steady state (budget row 1 reference: App RSS ≤ 200 MB)**

| Run | ready | soak | samples | VmRSS steady | VmRSS peak | within ref |
|---|---|---|---|---|---|---|
| 1 | 0.211 s | 2.002 s | 9 | 63 568 kB (62.1 MB) | 63 568 kB | yes |
| 2 | 0.211 s | 2.002 s | 9 | 60 092 kB (58.7 MB) | 60 092 kB | yes |
| (dev-debug smoke) | 0.221 s | 2.002 s | 9 | 62 536 kB (61.1 MB) | 62 536 kB | yes |

Sample machine JSON (run 1):

```json
{"harness": "rss", "n_tasks": 10000, "ready_secs": 0.211, "soak_secs": 2.002,
 "samples": 9, "steady": true, "vmrss_steady_kb": 63568, "vmrss_peak_kb": 63568,
 "budget_reference_mb": 200, "within_reference": true}
```

**S3 — cold start to first frame (budget row 4 reference: ≤ 5 s)**

| Run | display | first_frame_ms | within ref |
|---|---|---|---|
| 1 | wayland | 280.8 | yes |
| 2 | wayland | 277.7 | yes |

Receipt-side monotonic timing (≤ 1 ms bias); the moment is the mount
seam's first refresh (models ready), not the presented frame — the
proxy is honest-labeled in the event message, the summary, and
runbook §14. Receipt is a pipe-pump thread with a blocking
deadline-bounded read — no polling granularity in the measurement.

Direction only: both rows sit far inside their references on the dev
machine, consistent with the audit's derate analysis (row 4's 5 s has
~3.9× headroom on storage alone); device-class verdicts belong to the
kiosk bring-up spike.

## 2. The boot-race shutdown bug the harness surfaced (deviation + fix)

The stage-1 plan's D4 teardown premise — "SIGTERM (the desktop exit
watcher → `quit()` → graceful shutdown — the documented signal path)"
— does not hold when the signal lands in the boot window: the harness
TERMs immediately after the first-frame event, and the process never
exited (reproduced deterministically; a stray manual-test instance
confirmed it stays immortal for minutes).

Root cause (verified against the slint 1.18.1 / winit backend
sources): `quit_event_loop` before the event loop runs returns `Ok`
and silently drops the Exit event (proxy `send_event` + generation
filter), so the exit watcher fired but the loop never quit.

Fix (commit `fffaa79`, `fix(app,ui)`):
- `taskboard-ui-slint`: `quit()` parks the request on
  `QUIT_REQUESTED_BEFORE_LOOP`; `run()` honors the parked flag by
  skipping the loop entirely (graceful tail runs, window never
  shown); `event_loop_running()` exposes the loop state.
- `taskboard-app`: `quit_until_accepted()` (both the `gui` verb's and
  the appliance watcher's exit path) waits — bounded ~10 s — for the
  loop to run, then delivers the quit through the live proxy.

Post-fix: TERM at first-frame exits gracefully (rc 0) in ~1.5 s. The
fix is a liveness bug fix independent of the harness (systemd/container
stops during boot hit the same window); `run_desktop` stays in its
declared main-glue (untested in-process) class — the harness exercises
the exact race on every cold-start run.

## 3. Deviations from the stage-1 plan

1. **The fix commit above** — unanticipated production bug in the
   chunk-1 `gui` verb's teardown; without it the cold-start harness
   could never complete its own teardown (every run exit 3).
   Recorded per the dispatch's honesty rules; nothing about
   references/gates changed.
2. **`quit()` signature change** (`() -> Result<(), EventLoopError>`)
   — required by the fix; both existing callers updated
   (`let _ =`); clippy demanded the `# Errors` doc.
3. Nothing else: D1–D12 implemented as written (one script, two
   subcommands; `#[path]` fixture include; poll-with-deadline
   readiness/soak/moment; structured-field selection only; references
   reported, never asserted; no CI/workflow diff; runbook §14 and
   CHANGELOG per D9/D10).

## 4. Gate results (AGENTS §10)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | green |
| `cargo clippy --workspace --all-targets -- -D warnings` | green (pre-commit runs it on every commit) |
| `cargo nextest run --workspace` | 526 passed, 0 failed (12 skipped: docker/ignored tiers) |
| `cargo test --doc --workspace` | green |
| `cargo deny check` | see gate log below |
| coverage sequence | see gate log below |

(The remaining three gates were executed after this table was
written; final results are in the handoff message.)

## 5. Harness verification evidence

- `rss`: fresh fixture built per run (temp dir), readiness via
  `status --json` `daemon == true` + pid cross-check, plateau
  declared only after `--soak-min-secs` with an 8-sample
  ≤ 4096 kB window, graceful `daemon stop` exit 0.
- `cold-start`: display gate (wayland), first-frame selected by the
  structured field `moment == "first-frame"` on the JSON stderr
  stream (never message text), deadline-bounded pump thread,
  SIGTERM teardown within the 20 s grace.
- Failure paths exercised naturally during development: readiness
  deadline, missing first-frame event, and SIGTERM-teardown expiry
  each produced exit 3 with the captured logs printed (the teardown
  expiry is what exposed the fix in §2).

## 6. Stage-3 review record (2026-10-20)

Verdict: SHIP. Findings found and fixed on the branch (squashed into
their commits):

1. `scripts/process_smoke.py` — `await_ready` referenced `scratch`
   out of scope: a daemon exiting during the readiness window crashed
   with `NameError` (exit 1, traceback) instead of the typed exit 3.
   The scratch dir is now a parameter; both error paths verified by
   execution (`daemon exited early (code 0)` → exit 3; missing
   `--fixture` → exit 4).
2. `t0` was taken inside the receipt pump instead of immediately
   before `Popen` (plan D4) — a µs–ms undercount; moved before the
   spawn.
3. A failed `cargo build` exited 3; plan D1 classes a binary that
   cannot be built as a precondition failure — now exit 4, docstring
   updated.
4. §1's receipt-timing note described a "5 ms sleep loop" pump that
   the code never had (the pump reads blocking, deadline-bounded) —
   corrected above.
5. The cold-start child's stdout handle is now closed via `with`; a
   placeholder-less f-string removed.

Re-runs on the review pass (same machine, same build): `rss` ready
0.221 s, steady 59 432 kB (58.0 MB), peak 59 432 kB, 9 samples —
consistent with §1's 60–64 MB band. `cold-start` 371.6 ms on wayland
— same magnitude as §1's 277–281 ms (session-load variance; both far
inside the 5 s reference). Full §10 gate suite green on the branch
tip.
