# CLI TTY Ctrl-C Flaky-Pass Plan & Record — #697 (2026-11-28)

- **Test**: `cli_tty::tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`
  (`crates/taskboard-app/tests/cli/cli_tty.rs`, compiled into the
  `cli_output` test target)
- **Issue**: D4id4los/taskboard#697 (four-plus recurrences, ~82 s signature
  2026-11-20/21/22/23/24/25/26, plus the older ~10 s `created.len()==0`
  signature 2026-11-07 → 2026-11-23)
- **Stage**: 1 (diagnosis) + 2 (compressed fix) in one session, per the
  dispatch compression rule; **stage 3 NOT started** (no push, no PR).
- **Branch**: `test/cli-tty-ctrl-c-hardening` off `origin/main` `22f18232`,
  worktree `/home/viktoria/src/taskboard-WT-test-cli-tty-ctrl-c-hardening`.

## 1. The mechanism (evidence, not the recorded hypothesis)

### 1.1 The ~82 s figure decomposes exactly

`PTY_IDLE_BUDGET = max(70 s, 6 × D_max + D_max)` = **70 s**
(`cli_tty.rs:84-88`), consumed by `conclude_session`'s drain
(`cli_tty.rs:415-424`): after the **last output byte**, the drain waits
70 s of silence, then `child.kill()` (SIGKILL to the `timeout` wrapper)
→ `ExitStatus.code() == None` → the recorded assert
`the session did not survive the stalled line: left: None, right: Some(0)`
(fails at the `assert_eq!(status.code(), Some(0))`).

Observed totals 82.1–82.3 s ⇒ **last output at ~12 s + 70 s silent
drain + kill**. The ~12 s is the session's normal pre-drain runtime
(boot ≈ 2 s under load + the 10 s `DESIGNED_SILENT_WINDOW`). So the 82 s
runs are: the session produced its ~12 s output burst, then went
**silent and alive** for 70 s, then the harness killed it. The load
sensitivity lives in whatever eats the final burst-to-exit window under
full-suite load; the *budget* is not the bug (see §1.4).

### 1.2 The primary finding: the test's ^C never cancels — even idle

Instrumented runs (uncommitted eprintln in the session watcher's SIGINT
feeder + the repl cancel arm, plus `strace -f -tt -e trace=%signal,read,ioctl`
wrapped around the session binary via a temporary edit of
`spawn_session_pty_exec`'s inner; all reverted):

- The transcript gap telemetry (`TASKBOARD_PTY_TEST_GAP_LOG`, idle run)
  shows an **exact 10.0 s silent window** — the wiremock delayed route
  runs to completion. The transcript holds **no `cancelled` envelope**
  and no output at all during the window.
- `strace`: **no SIGINT is ever delivered to the session process**, and
  the decisive line —

  ```
  read(0, "boards list --json\r\3", 1024) = 20
  ```

  the app's editor read the scripted line **and the ^C byte together in
  one raw-mode read**. `readline` was still active (raw mode: ISIG off,
  ECHO off — hence no `^C` echo either), so the byte never reached the
  line discipline as a signal: it queued as input data and was swallowed
  by rustyline.

### 1.3 Root cause: the in-flight gate is bogus

The test gates the ^C on the wiremock notify
(`NotifyThenDelay`, `cli_tty.rs` ~1712): "the stalled request arrived ⇒
the dispatch is in flight ⇒ raw mode is long gone". But the mock matches
**every GET**, and the **sync actor's boot-pull GET satisfies it** —
right after boot, *before the scripted line is even submitted*. The test
then writes `\x03` while `readline` still owns the tty in raw mode; the
byte is swallowed; the SIGINT cancel path the test exists to pin is
**never exercised at all**. The test has been passing vacuously:

- `created.len() == 1` is satisfied because the stalled line *completes
  normally* after its 10 s delay and the scripted `boards create B`
  executes;
- `kinds.iter().all(|kind| kind == "cancelled")` passes on an **empty**
  `kinds` list (`all` on an empty iterator is `true`) — a vacuous
  assertion, a violation of the AGENTS §5 no-quality-ceilings letter.

Manual reproduction with a plain pty (python `pty` + a slow HTTP peer,
same binary, byte written *after* the line was consumed) confirms the
product behavior is correct: the ^C is a real SIGINT, the cancel select
fires within **~1 ms**, the typed `cancelled` envelope prints, the
prompt re-arms. **There is no product bug in the cancel path** — the
armed production race (H1 in the #697 thread, 2026-11-17) is
disproven for this fixture: the session watcher receives the signal and
the loop cancels typed.

### 1.4 Where the load fragility actually lives

With the ^C swallowed, the idle world is: line dispatched → 10 s wiremock
window → burst of output → `boards create` → `exit` → EOF ≈ 10.2 s. Under
full-suite load (thousands of tests, parallel sqlite writers, swap
pressure), the tail of that sequence can stall without output — the
session then sits silent while alive, and the 70 s idle drain
(`conclude_session`) SIGKILLs it → `code() == None` → the 82 s signature.
The stray swallowed `\3` also makes the byte's consumption point
load-dependent (it can land between reads and shift rustyline's
Interrupted/verdict cycles — the mechanism behind the ~10 s
`created.len() == 0` CI signature, #697 2026-11-11/11-17/11-23): the
documented raw-mode input-flush hazard (`cli_tty.rs` ~448) then eats a
scripted line at a load-dependent point. **One defect, both recorded
signatures.**

The 2026-11-17 "determinism constants" round (`50eecbfb`) hardened the
budget derivation but could not cure the flake because the fixture's
gate — not the budget — was broken: budgets bound the wrong window when
the ^C never fires.

## 2. The fix (class (a): fixture hardening; TDD red demonstrated)

File: `crates/taskboard-app/tests/cli/cli_tty.rs` only (test file, no
product change — none warranted, per §1.3).

1. **True gate for the ^C** (replaces the notify recv): wait on the
   session's own dispatch log read off the captured transcript —
   `session line` **and** `cmd: List` substrings. That log line is
   printed inside `dispatch_line`, i.e. after the loop took the line
   from the editor: readline has provably returned (cooked mode, ISIG
   on), and the cancel select is armed; the cancel watch holds the
   epoch, so a signal landing during the select's arming window is
   still observed. This is an observable fact the test waits for, not a
   sleep or a widened budget (AGENTS §5 "deterministic by
   construction").
2. **Kill the vacuous assertion**: the envelope check is now
   `kinds.len() == 1 && kinds[0] == "cancelled"` (mirrors the
   masked-read Case-1 test) — a lost or swallowed ^C now fails the test
   instead of passing vacuously.
3. The wiremock notify half is kept but deliberately unused, with the
   why in the doc comment (`_rx`), so the delayed route (the stall) is
   unchanged and `DESIGNED_SILENT_WINDOW` still derives the budgets.

### TDD evidence

- **Red (before the gate fix, assertion strengthened first)**: the test
  fails *deterministically at idle* —
  `FAIL [10.235s] … tty_ctrl_c_during_a_stalled_line…` — because no
  `cancelled` envelope exists. The red is idle-reproducible, which is
  stronger than the previously unreproducible load red.
- **Green (after the gate fix)**: PASS in **1.2 s** (was 10.2 s — the
  session no longer consumes the 10 s delay; the cancel actually fires).

Reliability counts: see §4.

## 3. What was ruled out

- **Product cancel race (H1)**: disproven by direct observation — with a
  correctly timed ^C, the watcher feeder, ladder, cancel watch, loop
  select, envelope, and prompt re-arm all fire within ~1 ms.
- **Idle-budget mis-derivation**: the 70 s budget is consumed only as
  the drain of a genuinely silent-but-alive session; the constant round
  was sound, the fixture was not.
- **`script(1)` relay loss**: a relay probe (`\x03` written to script's
  piped stdin ≥1 s after spawn) shows the byte relayed, echoed `^C`,
  and raising SIGINT — the relay is fine; only the *timing* (raw-mode
  window) loses the byte.

## 4. Reliability counts and gates

- Idle: 30 consecutive runs of the test (memory-gated `cargo nextest`),
  result recorded in the commit message and the stage-2 report to the
  dispatcher.
- Load: 10 runs under the recorded synthetic stress profile
  (bounded busy-loop spinners, box kept within its memory floor), same
  gate.
- Gates run before handoff: `cargo fmt --all -- --check`,
  `cargo clippy -p taskboard-app --all-targets -- -D warnings`, the
  targeted nextest campaigns above. The full §10 coverage trio is the
  **dispatcher's** (long-command discipline: it never runs inside a
  subagent); the flock-deferral rule applies.

## 5. Siblings and follow-ups

- **#521** (`tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives`):
  same file, but Case 1's gate is the *marker* (`App password: `) — a
  transcript fact, already structural; its flake class is the raw-mode
  verdict cycle under load, not a bogus gate. Not touched here.
- The one-shot sibling
  (`tty_ctrl_c_during_a_stalled_one_shot_cancels_typed_and_exits_130`)
  gates on the stalling peer *receiving the frame* and uses an external
  `kill -INT`: sound, untouched.
- `NotifyThenDelay`'s notify half is now unused by the session test; if
  no future test needs it, a later cleanup may fold the mock to a plain
  delayed responder (out of scope here; not filed as an issue — the
  mock remains used and compiling).

## 6. Stage-3 REVIEW prompt (verbatim, paste-ready)

> You are STAGE 3 (REVIEW) for the dedicated flaky-test hardening pass on
> watchlist issue #697 —
> `cli_tty::tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`
> (`crates/taskboard-app/tests/cli/cli_tty.rs`, `cli_output` target), branch
> `test/cli-tty-ctrl-c-hardening`, worktree
> `/home/viktoria/src/taskboard-WT-test-cli-tty-ctrl-c-hardening`.
> Read `.artifacts/plans/2026-11-28-cli-tty-ctrl-c-hardening-plan.md` and
> issue #697 in full first: the fix is class (a) fixture hardening — the
> wiremock-notify ^C gate was bogus (the sync actor's boot-pull GET
> satisfied it before the scripted line existed, so the ^C landed in
> rustyline's raw-mode input queue and was swallowed; the test passed
> vacuously on an empty envelope list), and the gate is now the session's
> own `session line … cmd: List` transcript fact, with the envelope
> assertion strengthened to exactly one `cancelled`.
>
> **Discipline (binding — docs/agent_dispatch.org):** A subagent is killed
> after ~600s without observable activity. **Long-command discipline**:
> (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3
> agent's gate is the pre-commit hooks (automatic) plus
> `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe;
> background + poll with useful work between polls if needed); the
> dispatcher runs the full AGENTS §10 suite between stage 2's landing and
> stage 3's ship. *Flock-deferral rule: when the gate lock is contended and
> no other useful work remains, never block — commit all landed work,
> report "gate deferred: lock held by <holder>", and end the stage; the
> dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push,
> open the PR, report — the dispatcher owns `gh pr checks --watch`,
> `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any
> command expected over ~5 minutes runs backgrounded* with short
> interleaved polls and useful work between polls — commit all landed work
> BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) *Every
> agent-launched cargo invocation routes through `scripts/memory-gate.sh`*
> — nothing bare. (5) *Stage prompts restate this discipline verbatim.*
> **Never commit with `--no-verify` or any hook bypass.** **The required
> SonarCloud quality gate fails a PR whose NEW code has more than 3 %
> duplicated lines.** Out-of-scope findings become `backlog:` GitHub issues
> (or comments) immediately (AGENTS §7; search first).
>
> Your job: review the landed diff on the branch against AGENTS §5 (no
> sleeps, no weakened assertions — the strengthened envelope assertion must
> survive review; the gate must be the transcript fact, not a timing
> constant), re-run the test's red/green argument if anything looks off,
> verify the plan's reliability counts against the logs, push the branch,
> open the PR (`test(app): …`), and STOP at PR-open — the dispatcher owns
> checks/teardown. Comment the PR description with the mechanism summary
> from the plan (§1) and cite #697.
