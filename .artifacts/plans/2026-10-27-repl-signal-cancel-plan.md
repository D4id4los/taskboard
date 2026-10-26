# [Plan] REPL Signal Cancellation and Prompt Grammar (DR-2/R)

- **Date**: 2026-10-27
- **Kind**: stage-1 TDD plan for chunk DR-2/R of the 2026-10-26
  six-rounds dispatch (issue #469, branch `feat/repl-signal-cancel`;
  closes #174/#192/#200 with the chunk PR).
- **Origin** (normative):
  `.artifacts/plans/2026-10-26-signal-handling-design-plan.md` §3
  D-0/D-1/D-2/D-3/D-4 (the design, accepted by ruling 2026-10-26),
  D-7 (placement/platform), D-9 (the seams), §2 facts F1–F16, §5
  risks, §6 chunk R; the dispatch plan §1/§4.4. This plan applies
  those decisions to the code at `4a3b30c` and resolves the
  implementation-level questions they leave open (§11 — every
  resolution is recorded there; none re-opens a D-decision).
- **Branch / worktree**: `feat/repl-signal-cancel` in
  `../taskboard-WT-feat-repl-signal-cancel`, off `origin/main` @
  `4a3b30c` (created off local `f29dde1`, fast-forwarded before any
  commit — DR-4/S landed first and is disjoint: domain `merge.rs` +
  sync tests only). Doctor passed (53G free, floor 25G).
- **Status**: PLANNED — stage 2 implements; stage 3 reviews and ships
  (closes #174/#192/#200/#469).

---

## 1. Mission & the diff-shape rule

One SIGINT policy for the REPL session and one prompt grammar for the
REPL ceremony, per the design's policy sentence (D-0): SIGINT cancels
the current operation; SIGTERM/SIGHUP restore the terminal then end
gracefully; every ladder ends in a force rung; death by keyboard is
reserved for `exit`/`quit`/Ctrl-D and the force rungs.

As a review-checkable rule (dispatch §4.4 + review prompt R.3):

- **Allowed**: new `crates/taskboard-app/src/signals.rs`; edits in
  `crates/taskboard-app/src/{repl.rs, completion.rs,
  commands/mod.rs, commands/connect.rs (docs + `pub(crate)
  parse_confirm` only), lib.rs (one `mod` line)}`; edits in
  `crates/taskboard-app/tests/{repl.rs, cli_tty.rs}`; dated
  amendments to `docs/adr/0011-*.org` and `docs/adr/0014-*.org`;
  a `CHANGELOG.md` Unreleased entry.
- **Forbidden**: any hunk to `taskboard-domain`/`taskboard-state`/
  `taskboard-storage-sqlite`/`taskboard-sync-nextcloud`/
  `taskboard-ui-slint` (binding constraint, design §1); any hunk to
  `main.rs`/`bootstrap.rs`/`ui.rs` signal handling (the daemon/GUI/
  kiosk tails are chunk DR-2/D — F1's `wait_for_signal` call sites
  stay untouched here); one-shot dispatch paths (§6); any new
  dependency or feature flag (`tokio/signal` and `nix/term` are
  already workspace features, design D-1); any `unsafe`
  (`#![forbid(unsafe_code)]` stands, AGENTS §6); any Cargo.toml.

**Handlers are never on test paths (D-7)**: the only signal
installation of this chunk is the watcher spawn inside
`run_session` (production-only, `crates/taskboard-app/src/repl.rs:912`).
`repl_loop` and everything under it consume injected `watch`
channels — hermetic tests race no real signal; the pty tier is the
only place real signals occur in tests (design §5, last risk).

## 2. Verified baseline (seam facts at `4a3b30c`)

All anchors read at `4a3b30c`; the signal-handling design verified
the same files byte-identical at `cdc41c6`.

- **Editor thread, three states** (F4): `repl.rs:206-260` — editing
  (`editor.readline(PROMPT)`, raw mode, `:207`), awaiting verdict
  (`verdicts_rx.blocking_recv()`, `:237`, cooked mode between
  readlines), answering a prompt (`Disposition::Prompt` arm `:240-257`
  — `PromptKind::Line`/`Confirm` call `connect::StdinPrompts`
  cooked `*_sync` cores; `Masked` calls rpassword). ^C while editing
  is a byte → `ReadlineError::Interrupted` → `LineEvent::Interrupted`
  (`:218-221`) → the loop resumes (F5).
- **Session loop** (`repl.rs:647-767`): `while let Some(event) =
  lines.recv().await`; the `Execute` arm (`:685-755`) matches
  ceremony interception (`connect`/`login` → `connect_line`,
  `logout` → `logout_line`, `boards use` → `boards_use_line`) vs
  `dispatch_line`; every path ends in exactly one
  `session.prompts.resume()` (`:757-760`). Line failures never abort
  the session (`let _ = outcome`, `:754`).
- **TtyStateGuard** (`repl.rs:871-896`, unix): saves
  `tcgetattr(stdin)` at `run_session` entry (`:927`), restores on
  drop (`TCSADRAIN`). Signal death runs no destructors (F10) — the
  watcher must own an explicit restore. Non-unix: a no-op struct
  (`:900-908`).
- **Ceremony prompt machinery** (`commands/connect.rs`):
  `PromptReader` RPITIT trait (`:58-98`); `StdinPrompts` sync cores
  (`:114-173`) — label on stderr + `stdin().read_line`; masked read
  = `rpassword::prompt_password_with_config` on a terminal
  (`:140`); `parse_confirm` pure grammar (`:201-203`); the ceremony
  maps reader errors to `CommandError::Prompt` (`:305`, `:324`,
  `:334`, `:381` — a cancelled prompt is not a declined one).
- **REPL prompt transport** (`ReplPrompts`, `repl.rs:277-343`):
  `ask()` ships the `Disposition::Prompt` and awaits the oneshot;
  peer death maps to `BrokenPipe`; `PromptValue::{Text,Flag}`
  typed answers; prompt answers ride the oneshot and never reach the
  lines channel (pinned by `prompt_answers_bypass_the_lines_channel`,
  `repl.rs:1028`).
- **Typed error surface** (`commands/mod.rs`): `CommandError`
  (`:122-395`), `error_kind()` (`:398-447`, kebab kinds),
  `report_to` prints the `{"error":{"kind","detail"}}` envelope on
  stderr under `--json` (`:1926-1945`). A new variant is one enum
  arm + one kind arm.
- **Entry point**: `main.rs:205-208` — `BareRoute::Repl` calls
  `run_session` then `std::process::exit(code)`. The TtyStateGuard
  in `run_session` drops before that exit (scope end). No signal
  listener exists anywhere on this path today (F1: only the
  daemon/GUI/kiosk tails use `bootstrap::wait_for_signal`).
- **Daemon precedent** (context only, chunk D's): `main.rs:392-437`
  — `drain_and_shutdown` selects the drain against a second
  `wait_for_signal()` and force-exits `std::process::exit(130)`
  (`:426-430`). House precedent for the immediate-exit shape under
  `forbid(unsafe_code)`.
- **Test harnesses**: hermetic loop tests construct `App` via
  `bootstrap(standalone_config(...))` and drive `repl_loop` /
  `run_session_with_lines` with injected `EditorIo`
  (`tests/repl.rs:664-740`); the pty tier wraps the real binary in
  `script(1)` with marker-gated pacing — `feed_session_lines`
  (`cli_tty.rs:46-78`), `wait_for_marker` (`:195`), the
  readline-cycle counter `READLINE_MARKER`/`wait_for_readline`/
  `feed_line` (`:1058-1127`), and the stty hand-back probe pattern
  (`:584-674`).
- **Dependency facts carried from the design** (normative): F8 —
  rpassword on ^C raises SIGINT *and* returns `Err(Interrupted)`;
  under an installed handler the raise is consumed and the `Err` is
  the typed path. F9 — `signal-hook-registry` sets `SA_RESTART`:
  installing a handler never `EINTR`s a cooked read. F16 — rustyline
  writes prompts to stdout; rpassword's to `/dev/tty`.
- **rustyline 19 API** (vendored, `rustyline-19.0.0/src/lib.rs:904-916`):
  `set_helper(Option<H>)` takes ownership; there is **no**
  `take_helper` — `helper()`/`helper_mut()` are borrows. This API
  fact decides §11 Q1.
- `proptest` is **not** a dev-dependency of `taskboard-app`
  (workspace dev-dep elsewhere only) — decides §11 Q8.

## 3. R-1 — `signals.rs`: the policy module

`crates/taskboard-app/src/signals.rs` (SPDX `AGPL-3.0-only`; the
crate-level `#![forbid(unsafe_code)]` covers it). The module doc
carries the D-0 policy sentence verbatim plus the one-paragraph
orphan-completion note (D-2: documented, not hidden).

### 3.1 The ladder state machine (pure, exhaustively unit-tested)

```rust
pub(crate) enum Signal { Interrupt, Terminate, Hangup } // Terminate/Hangup: cfg(unix) ctors only
pub(crate) enum Event  { Signal(Signal), Progress }
pub(crate) enum Action { Initiate, Escalate, Force }

pub(crate) struct Ladder { middle_rungs: u8, state: State, last: Option<Signal> }
enum State { Fresh, Armed { rung: u8 }, Forced }
```

Total transition function `fn on(&mut self, Event) -> Option<Action>`:

| state | event | action | next state |
|---|---|---|---|
| Fresh | `Signal(s)` | `Initiate` | `Armed{1}`, `last = s` |
| `Armed{r}` | `Signal(s)` where `r < 1 + middle_rungs` | `Escalate` | `Armed{r+1}`, `last = s` |
| `Armed{r}` | `Signal(s)` otherwise | `Force` | `Forced` |
| `Armed{..}` | `Progress` | none | `Fresh` (de-arm) |
| `Fresh` | `Progress` | none | `Fresh` (ack without arm) |
| `Forced` | anything | none | `Forced` |

plus `fn last_signal(&self) -> Option<Signal>` (the watcher reads it
for the force exit code) and `fn new(middle_rungs: u8) -> Self`.

- **REPL config** is `Ladder::new(0)` twice — one instance per
  signal *family* (§11 Q4): the SIGINT family `[Initiate(cancel),
  Force(130)]`, the unix termination family `[Initiate(restore +
  graceful), Force(128+signum)]`.
- **Daemon config** is `Ladder::new(1)` — `[Initiate(graceful),
  Escalate(salvage + exit), Force(immediate)]` (design D-6's three
  rungs). Chunk R ships and unit-tests this configuration (it is the
  same machine; D-1: "the counting/ordering logic exists once") but
  consumes only the REPL configs — the daemon wiring is chunk D's,
  which may feed one merged instance (D-6's rung 1 names SIGINT and
  SIGTERM together; the machine is kind-agnostic so both wirings are
  pure configurations of it — recorded so chunk D does not inherit
  the two-instance shape by accident).
- Progress semantics encode D-2's force condition exactly: the
  second signal forces only "while the cancellation is not yet
  acknowledged" — `Progress` is the acknowledgement and de-arms.

**Tests (TDD step 1)** — in-module `#[cfg(test)]`:
- Hand tables pinning each named row above for both configs,
  including the D-6 mixed-kind row: daemon config fed
  `[Interrupt, Terminate]` → `[Initiate, Escalate]` with
  `last_signal() == Terminate` (force code 143).
- An exhaustive sweep over **all** sequences over
  `{Signal(Interrupt), Signal(Terminate), Progress}` of length ≤ 4
  (3⁴ = 81 per config, both configs) asserting *invariants*, not a
  mirror implementation: (a) `Force` appears only as the action to
  the (2+middle_rungs)-th un-acknowledged signal; (b) no action is
  ever produced from `Forced`; (c) `Progress` never produces an
  action; (d) `last_signal` is always the last fed signal kind;
  (e) after a `Progress` the next signal is `Initiate` again.
  No proptest (§11 Q8) — the space is enumerated outright.

### 3.2 The watcher (thin glue, untested per house rule)

One async task, spawned by `run_session` (§3.3). Installs, via
`tokio::signal`: `ctrl_c()` → `Signal::Interrupt`;
`cfg(unix)`: `signal(SignalKind::terminate())` → `Terminate`,
`signal(SignalKind::hangup())` → `Hangup` (registration failure
degrades that stream to `pending()` — the `wait_for_signal`
precedent, `bootstrap.rs:1156-1178`). It owns the two `Ladder`
instances and drives them:

- SIGINT `Initiate` → `cancel_epoch += 1` and send on the
  **cancel watch** (`watch::channel<u64>`, watcher → loop).
- SIGINT `Force` → `restore_tty(&saved)` then
  `std::process::exit(130)` (`std::process::exit`, not `nix`'s
  `_exit`: the crate forbids `unsafe` and the daemon tail already
  force-exits this way — §11 Q2; the design's `_exit(128+signum)`
  spelling is satisfied by the same immediate-exit semantics; the
  tty restore has already run explicitly, so flushing std streams is
  safe). Code = `128 + signum` via `last_signal`:
  Interrupt→130, Terminate→143, Hangup→129.
- Terminate/Hangup `Initiate` → **restore first** (`restore_tty`,
  idempotent, SIGHUP's failure ignored), *then* send once on the
  **terminate watch** (`watch::channel<()>`, watcher → loop) — the
  D-4 ordering: the pane is sane from the first moment of shutdown,
  before any potentially-slow bounded join. No `Progress` exists for
  this family (the loop's graceful end is the process exiting 0;
  a second signal during the drain forces — §7).
- ack watch (`watch::channel<u64>`, loop → watcher) `changed()` →
  feed `Progress` to the SIGINT ladder.

The restore body is extracted from `TtyStateGuard` into one free
function used by both the `Drop` impl and the watcher (one restore
body, two callers):

```rust
#[cfg(unix)]
fn restore_tty(saved: &Option<nix::sys::termios::Termios>) { /* TCSADRAIN tcsetattr, error ignored */ }
```

`run_session` clones the guard's saved state into an
`Arc<Option<Termios>>` shared with the watcher before the guard is
dropped at scope end. Idempotent by construction (restoring the
saved state twice is a no-op).

**Untested by house rule** (design D-9: "the force-exit and restore
side effects stay in the thin watcher glue; everything above them is
[unit-tested]") — its correctness is pinned indirectly by the pty
tier (§8: force exit 130 with a sane pane proves restore-before-exit
ordering) and by the pure machine's tables.

### 3.3 Installation & wiring

`run_session` (`repl.rs:912-936`) becomes the chunk's only install
point (D-1/D-7):

1. `TtyStateGuard::take()` (existing line, unchanged position).
2. Create the three watches; `tokio::spawn` the watcher with the
   signal streams, the `Ladder`s, the saved termios, and the
   watch senders/receivers it needs.
3. Pass the loop-side halves into `run_session_with_parts` via a new
   small `SessionSignals` struct (`cancel_rx`, `ack_tx`, `term_rx`)
   with an inert constructor (`watch::channel(0)`/`watch::channel(())`
   with the senders dropped) — `run_session_with_lines` passes the
   inert one, so the existing factory signature and all current
   hermetic journey tests keep their shape; the two direct
   `run_session_with_parts` call sites in `tests/repl.rs` gain the
   inert argument.

Signal windows before the loop starts (bootstrap + bounded boot
pull): a SIGINT's epoch simply stays pending (drained at the first
dispatch, §4); a SIGTERM is observed at the loop's first select.
Bootstrap is bounded (typed failures, bounded boot-pull wait), so
the window is bounded — accepted (§11 Q6).

## 4. R-2 — the session loop: cancellation select + typed `Cancelled`

### 4.1 The injected seam

`repl_loop` gains three parameters (the loop-side halves):
`cancel: watch::Receiver<u64>`, `ack: watch::Sender<u64>`,
`terminate: watch::Receiver<()>`. No cfg in the loop: on Windows the
terminate watch is simply never sent (the watcher's unix streams are
cfg-gated, D-7).

Dead-channel semantics (§11 Q10): a dropped peer must never fire an
arm — both watches are consumed through one local helper,

```rust
async fn changed_or_never(rx: &mut watch::Receiver<T>) -> bool // Ok(change) => true, Err => pending().await
```

(a dead watcher degrades to today's no-cancellation world; it can
never kill or cancel a session).

### 4.2 The loop's new select arms

- **Main park** (`repl.rs:654`): `lines.recv()` is selected against
  `changed_or_never(&mut terminate)`; the terminate arm ends the
  session exactly like Ctrl-D — `break` → the existing
  `app.shutdown().await` tail → `ReplExit::Graceful` → exit 0
  (D-4). `ReplExit` gains no variant (§11 Q9).
- **Execute arm** (`repl.rs:685`): the whole interception match
  (all arms yield `u8`) is wrapped as one future and selected
  against three outcomes:
  - *dispatch completes* → today's behavior verbatim;
  - *cancel fires* → the line future is dropped by the select; the
    loop reports `CommandError::Cancelled` through `report_to`
    (same envelope as every line failure; exit code ignored — a
    cancelled line is a failed line, ADR 0011), records the epoch as
    acknowledged (`ack.send`), and falls through to the existing
    per-line `resume()` — exactly one `Next` per event;
  - *terminate fires* mid-dispatch → the line is dropped and the
    loop breaks to the graceful shutdown (no line-level output — an
    external termination is not a line event).
- **Stale-epoch drain** at the top of the Execute arm, before the
  select: `let epoch = *cancel.borrow_and_update(); if epoch >
  acked { acked = epoch; let _ = ack.send(acked); }` — a signal
  that landed while nothing was in flight is acknowledged without
  cancelling the line about to run (§11 Q3; the SIGINT ladder's
  force rung stays armed until this drain, which is the design's
  "the ladder counts it — the escape exists").

The cancel arm is deliberately **only** on the Execute select
(design-verbatim: "wraps the Execute arm's await"); the loop parked
at the prompt observes nothing — the drain + armed force cover that
world (§11 Q7).

### 4.3 `CommandError::Cancelled`

One variant in `commands/mod.rs`: `#[error("cancelled")] Cancelled`,
`error_kind() → "cancelled"`, and an `error_detail()` carrying the
orphan-completion note (an already-delivered engine command still
commits — D-0/D-2; UX text, never asserted). The envelope shape is
untouched (`report_to`).

### 4.4 Hermetic tests (TDD step 2, `tests/repl.rs`)

The cancellable stand-in is a line whose future awaits a
never-answered scripted prompt (design D-9's exact suggestion):
a real `EditorIo` whose disposition peer receives the `Prompt`
disposition and parks (never answers) parks the dispatch future on
the oneshot.

1. `a_cancel_fired_mid_dispatch_reports_cancelled_and_continues` —
   feed `connect --server X --user alice --json` (headless ceremony
   asks the masked prompt through the injected disposition channel);
   once the peer observes `Prompt`, fire the cancel watch; assert:
   the loop continues (the next fed line `boards create B --json`
   produces its payload), the stderr stream holds exactly one error
   envelope and its kind is `cancelled` (typed kind — the envelope
   shape is asserted structurally elsewhere; here the kind *is* the
   contract), and the peer counted exactly one `Prompt` + one `Next`
   for the cancelled line (exactly one disposition per event).
2. `a_cancel_fired_while_idle_does_not_kill_the_next_line` — fire
   the cancel while the loop is parked at `recv`; then feed a line;
   assert it executes (payload present, no `cancelled` envelope)
   — the drain rule.
3. `a_terminate_event_ends_the_session_like_ctrl_d` — fire the
   terminate watch while parked; assert the loop returns `Graceful`
   and the lock is released (the r12 pattern,
   `tests/repl.rs:660-698`).
4. `a_terminate_mid_dispatch_drops_the_line_and_shuts_down` — park
   on the scripted prompt, fire terminate; assert the loop returned
   and the peer's channel dropped without a further `Next`.
5. `a_dead_watcher_never_cancels_or_terminates` — drop both watch
   senders; feed a normal line + EOF; assert normal execution
   (`changed_or_never`).
6. The existing `r12_repl_loop_shuts_the_core_down_before_returning`
   call site gains the inert signals arguments.

## 5. R-3 — the prompt grammar: `Line`/`Confirm` onto rustyline

Design D-3, forced by F9 (a caught SIGINT cannot abort a cooked
read — `SA_RESTART` restarts it; in raw mode ^C is a byte rustyline
already answers with `Interrupted`). One prompt grammar for the
whole ceremony.

### 5.1 The editor thread's prompt arm (`repl.rs:240-257`)

- `PromptKind::Line` and `PromptKind::Confirm`:
  suspend the completion helper (§5.2), call
  `editor.readline(&label)` (rustyline writes the label to stdout —
  the F16 consequence, recorded in §9, no code), unsuspend, and map
  the result through the pure helper below.
- `PromptKind::Masked`: unchanged — `read_secret_sync` (rpassword on
  `/dev/tty`, F8's typed path under the handler).

### 5.2 Completion suspension (§11 Q1)

The design's literal `set_helper(None)`/restore is impossible on
rustyline 19: `set_helper(Option<H>)` takes ownership and there is
no `take_helper` (§2, `rustyline-19.0.0/src/lib.rs:904-916`).
Instead `ReplHelper` gains a `suspended: AtomicBool` with
`fn suspend(&self, on: bool)`; `Completer::complete` returns no
candidates while suspended. One helper instance, no reconstruction
site, and the suspension behavior is hermetically unit-testable.
The normative goal is the design's own parenthetical: no
tab-completion inside a server-URL prompt.

**Tests**: `completion.rs` — `a_suspended_helper_completes_nothing`
(suspended → zero candidates; unsuspended → the existing candidates
behavior, already covered by the module's suite).

### 5.3 The mapping helper (pure, unit-tested)

```rust
fn prompt_answer(kind: PromptKind, answer: Result<String, rustyline::error::ReadlineError>)
    -> Result<PromptValue, std::io::Error>
```

- `Ok(line)` + `Line` → `Ok(Text(line.trim()))` — the same trim
  `read_line_sync` applied (`connect.rs:127`).
- `Ok(line)` + `Confirm` → `Ok(Flag(parse_confirm(&line)))` —
  `parse_confirm` becomes `pub(crate)` (one grammar, one home;
  `connect.rs:201`).
- `Err(Interrupted)` → `Err(ErrorKind::Interrupted.into())` → the
  ceremony's existing `CommandError::Prompt` mapping — a cancelled
  prompt is not a declined one (ADR 0014's distinction, unchanged).
- `Err(Eof)` → `Err(ErrorKind::UnexpectedEof.into())` → the same
  typed `prompt` failure (§11 Q5: Ctrl-D at a ceremony prompt fails
  the *line*, never the session; today's cooked path would have
  answered an empty string and failed verification later — the
  typed path is strictly more honest).
- `Err(other)` → `Err(io::Error::other(..))` → typed `prompt`.

**Tests**: the full mapping table as a unit test in `repl.rs`
(`ReadlineError` variants are constructible); plus the
disposition-peer extension the design names: a peer scripted with
`Err(io::Error::from(ErrorKind::Interrupted))` driving
`connect::run` through `ReplPrompts` asserts
`matches!(err, CommandError::Prompt)` — the byte-world mapping
pinned at the ceremony level (the oneshot error path is already
pinned by `a_dropped_peer_maps_to_broken_pipe`; this extends it with
the `Interrupted` kind).

### 5.4 History and completion exclusion (structural, already pinned)

Prompt answers ride the oneshot and never reach the lines channel
(`prompt_answers_bypass_the_lines_channel`, `repl.rs:1028`) and the
prompt arm never calls `history.record` — rustyline does not
auto-append, so exclusion is by construction. The pty tier adds the
file-level pin (§8, case 6): the history file contains no
prompt-supplied URL or secret (absence-of-secret assertion — the
house pattern of the config pins).

`StdinPrompts`' docs (`connect.rs:100-103`, "shared with the REPL
editor thread") are updated: the `*_sync` cores remain the one-shot
terminal impl and the masked read's home; `read_line_sync`/
`read_confirm_sync` lose their editor-thread caller. No dead code —
the one-shot `StdinPrompts` impl keeps all three.

## 6. R-4 — one-shot ceremony exclusion (D-3)

**No handler is installed on any one-shot path in this chunk** —
structural: the watcher spawn exists only in `run_session`; the
diff must show no install outside it (review rule). The rationale
stands as recorded in the design (F14): the `StdinPrompts` sync read
executes inside `poll`, un-cancellable by any `select!`, and F9
blocks the EINTR route — installing a handler would *consume* ^C and
leave the read silently running: strictly worse than dying. The
one-shot `connect`/`login` keep the default disposition (death
pre-persist, ADR 0014's verification note stands for that surface);
the REPL is the interactive ceremony surface, the one-shot the
scripted one. `main.rs`'s one-shot arms are not touched.

Pinned at the pty tier (§8, case 7).

## 7. R-5 — SIGTERM/SIGHUP: restore-then-graceful (D-4)

The mechanics are §3.2's termination family + §4.2's loop arm;
end-to-end:

1. SIGTERM (or SIGHUP) → watcher: `restore_tty` **first** (idempotent;
   SIGHUP's restore fails harmlessly on the dead tty — error
   ignored), then the terminate watch fires.
2. The loop (parked at the prompt *or* mid-dispatch) breaks to the
   existing shutdown tail: `app.shutdown()` (bounded by
   `shutdown_timeout_secs`), `ReplExit::Graceful`, exit 0 — "ends
   exactly like Ctrl-D".
3. A second SIGTERM/SIGHUP before the process exits → the
   termination ladder has no `Progress` → `Force` → restore
   (idempotent) → `std::process::exit(143/129)`.
4. SIGQUIT keeps the kernel default (core-dump diagnostic; runbook
   documents `stty sane`). SIGKILL unhandleable by definition.
   Neither is listened to; nothing to cfg.
5. Windows: no termination streams (cfg(unix)); the ctrl_c listener
   and rustyline/rpassword's per-call console restore are the whole
   Windows story (D-4, documented not engineered).

Accepted residual windows (recorded): a signal landing after the
loop's shutdown completed but before `main`'s exit races the clean
exit 0 (today that signal kills mid-exit anyway — no worse); a
keyboard ^C in the microsecond cooked window between readlines
(verdict wait) is a *signal* in handler world: armed-then-drained,
invisible — today it killed the process (strictly gentler; §12).

## 8. R-6 — the pty tier (`tests/cli_tty.rs`)

Harness additions: a `child_pid` fixture — the inner command becomes
`sh -c 'echo $$ > <pidfile>; exec <binary> --config <cfg>'` (the
pidfile records the exec'd binary's pid) so tests can deliver real
signals to the session process, and `wait_for_prompt`-style marker
gating per the existing rules (no sleeps; blocking reads; the outer
`timeout` guards wedges). All new cases skip when `script` is
unavailable, like the suite's convention.

| # | case | replaces/adds | assertions (structural) |
|---|---|---|---|
| 1 | ^C byte during the masked read | **rewrites** `tty_ctrl_c_during_the_masked_read_never_persists_and_never_wedges` (`:331-447`) | handler world is now deterministic: feed `\x03` at the secret prompt → a typed failure envelope (kind `prompt` **or** `cancelled` — rpassword's own `Err` and the select-cancel race for it; both are correct typed cancellations, §12), session survives, `exit` completes 0, config untouched — the persistence half of the old pin unchanged and still load-bearing (design D-9 sanctions the semantics change) |
| 2 | ^C during a stalled line cancels; the next line executes | new | wiremock boards route delayed 30 s; feed `boards list --json`, wait the readline marker, feed `\x03` (real SIGINT — cooked mode, ISIG on) → the session answers the next marker; feed `tasks add ... --json` → payload present; one error envelope with kind `cancelled`; exit 0 |
| 3 | SIGTERM mid-readline leaves the pane sane | new | pidfile; at the prompt `kill -TERM <pid>` → exit **0**; chained `stty -a` probe (the `:584-674` pattern) shows `echo icanon isig iexten` on — restore-before-graceful ordering proven |
| 4 | second signal forces at 128+signum | new | pidfile; at the prompt (nothing in flight — no `Progress` possible) `kill -INT <pid>` twice → exit **130** and the stty probe is sane (restore-before-exit). The deterministic form of the dispatch's "second ^C forces with 130": the at-keyboard double-press during dispatch races the acknowledgement (§11 Q12) — the ladder's force rung itself is what this pin proves, and the external form is the design's own escape hatch (design §5 risk). **Stage-3 correction (2026-10-27): the SIGINT-at-the-prompt form is unreachable — verified against the built binary, with `TERM` set (the suite's own pacing requirement) an external SIGINT while `readline` is active is consumed by the line editor's own signal handling and redraws the prompt (the F5 grammar, exactly what the ADR 0011 amendment text records); it never reaches the watcher's ladder, so no amount of external SIGINTs at the prompt can arm it. The dispatch acceptance's substance — the watcher's force rung with restore-before-exit at 128+signum — is pinned instead on the termination family, which has no `Progress`: a second SIGTERM during the graceful drain (held open by a sync request parked on a delayed route, `shutdown_timeout_secs = 20`) forces exit 143 with a sane pane. Same `Ladder`, same `Force` side effect; the SIGINT family's counts stay pinned at the pure tier |
| 5 | ^C at the prompt still clears the line (F5) | new | feed `\x03` at the prompt (byte world, untouched) → the prompt re-arms (marker/prompt count) → the next line executes; exit 0 |
| 6 | in-session connect prompts ride the editor (grammar) | new | flag-less `connect` line: markers gate URL → username → secret; ceremony persists (config contains URL+user, not the secret); the session's **history file** (under the test's `XDG_DATA_HOME`) contains neither — prompt answers excluded from history, file-level pin |
| 7 | one-shot ceremony keeps default death | new | one-shot `login` pty (the `run_login_under_pty` harness), `\x03` at the first prompt → process dies **130** (no handler — §6), config untouched |

Cases 1/3/4/2/5 map one-to-one onto the dispatch §4.4 acceptance
list; 6/7 pin the grammar's and the exclusion's user-visible
contracts. The suite's other cases (completion, history recall,
lock, daemon-under-pty) are untouched — the one-shot
`connect --confirm` pty test (`:1135`) keeps exercising the
untouched `StdinPrompts` confirm path.

## 9. R-7 — docs amendments (dated, amendment-not-rewrite)

- **ADR 0011** (`docs/adr/0011-*`, §"Exit and signal semantics",
  `:156-165`): a dated amendment block appended after the v1 text
  (which stays visible below, per D-8): ^C during a dispatching line
  cancels the line (typed `cancelled`, session continues, an
  already-delivered engine command still commits — orphan
  completion); the force rung (second ^C unacknowledged → restore +
  130); SIGTERM/SIGHUP restore-then-graceful (exit 0; second →
  128+signum); SIGQUIT's default kept; the ceremony prompt grammar
  (`Line`/`Confirm` on the editor's rustyline, `Interrupted` = typed
  `prompt` cancellation, labels stderr→stdout with the prompt
  itself, masked stays rpassword on /dev/tty, answers never in
  history/completion); one-shot ceremonies install no handler.
- **ADR 0014** (`docs/adr/0014-*`, after the "Recorded verification"
  note, `:137-163`): a dated addendum — under the session's
  installed handler the masked read's ^C is deterministic on the
  terminal tier too (the re-raise consumed → `Err(Interrupted)` →
  typed `prompt`, session survives); the default-disposition branch
  remains true for the one-shot ceremony surface.
- **CHANGELOG.md**: an Unreleased entry covering the user-observable
  changes (REPL ^C cancel/force, SIGTERM/SIGHUP restore-then-exit-0,
  in-session prompt grammar + prompt-label destination).

ADR 0012 and the runbook are **not** touched (chunk I/D ride those,
D-8).

## 10. The test-first order (stage 2's TDD sequence)

Each step lands red-then-green; every commit standalone green:

1. `signals.rs` ladder machine + its exhaustive tables (pure; no
   wiring).
2. Loop cancellation: `CommandError::Cancelled` + the select wiring +
   tests 4.4.1–4.4.6 (the seam is injected — no signals installed).
3. Prompt grammar: `prompt_answer` table + helper suspension +
   Interrupted-through-ceremony + the editor-thread arm.
4. The watcher + `run_session` install + terminate path (glue; the
   loop-side terminate tests already exist from step 2).
5. The pty tier: rewrite case 1, add cases 2–7.
6. ADR 0011/0014 amendments + CHANGELOG.

## 11. Resolved questions (the decision record)

| # | question | decision | why / evidence |
|---|---|---|---|
| Q1 | How to suspend completion for prompt reads (design says `set_helper(None)`/restore) | A `suspended: AtomicBool` flag on `ReplHelper`; `complete()` yields nothing while suspended | rustyline 19 has no `take_helper` — `set_helper` takes ownership, restore would need a second construction site (vendored `rustyline-19.0.0/src/lib.rs:904-916`). The design's parenthetical states the goal: no Tab-completion inside a prompt. Also hermetically unit-testable |
| Q2 | The design's `_exit(128+signum)` vs `std::process::exit` | `std::process::exit` | `#![forbid(unsafe_code)]` (nix's `_exit` wraps `libc::_exit`, unsafe); house precedent: the daemon tail force-exits via `std::process::exit(130)` (`main.rs:426-430`). The tty restore runs explicitly before the exit, so flushing std streams is safe |
| Q3 | A SIGINT that lands while nothing is in flight (idle/editing) | No immediate action; the stale epoch is drained (and acknowledged) at the next dispatch start — the line then runs; the ladder's force rung stays armed until that drain, so a second signal while still idle forces | Matches the design's own risk text ("consumed silently until the second signal forces — the ladder counts it; the escape exists"). Rejected instant-cancel of the next line: a stray press during editing would kill the first line typed seconds later, and the completion race (signal landing just after a line finished) would misfire onto unrelated work. Rejected de-arm-while-idle: a second idle press would re-arm from Fresh — the force escape would never fire |
| Q4 | One ladder or two in the REPL watcher | Two `Ladder::new(0)` instances — SIGINT family (cancel/force-130) and the unix termination family (restore-graceful/force-128+signum); kinds never cross-count | The two rung-1 actions differ (cancel vs restore+graceful); D-2/D-4 treat them independently. The machine is kind-agnostic, so chunk D may feed one merged instance for D-6's "first SIGINT/SIGTERM" rung — recorded so the daemon chunk decides its own wiring |
| Q5 | Ctrl-D (Eof) at a REPL ceremony prompt | Typed `prompt` failure (`UnexpectedEof`); the session survives; Ctrl-D at the main prompt still exits | Today's cooked path silently answers `""` and fails verify later; the typed path is honest and keeps "a failed line never aborts the session". Leaving the session mid-ceremony was never the grammar |
| Q6 | Signals during bootstrap / before the loop starts | SIGINT: epoch stays pending, drained at the first dispatch; SIGTERM: observed at the loop's first select | The window is bounded (typed boot failures, bounded boot pull); the tty restore is not windowed at all — the watcher restores on receipt regardless of loop state |
| Q7 | Does the loop observe cancellation while parked at the prompt? | No — the select wraps only the Execute await (design-verbatim); idle-pressed signals are the Q3 drain world | The design's D-2 names exactly this shape; the force rung is the always-armed escape |
| Q8 | proptest for the ladder machine? | No — hand tables + an enumerated invariant sweep over all length-≤4 event sequences | `proptest` is not a `taskboard-app` dev-dep (Cargo.toml verified); the space is 81×2 sequences — enumeration is exhaustive without a new dev-dep, and invariants (not a mirror impl) are asserted |
| Q9 | A `ReplExit` variant for signal-graceful? | None — SIGTERM-graceful is `Graceful` (exit 0), same as Ctrl-D | D-4: "ends exactly like Ctrl-D"; the force rungs bypass `ReplExit` entirely (the watcher exits the process) |
| Q10 | Dropped watch peers (dead watcher) | `changed_or_never`: `Err` → `pending()` forever — no arm ever fires | A dead watcher must degrade to today's no-signal world, never cancel or kill a session (also avoids a busy loop on `Err`) |
| Q11 | The masked-read ^C pty assertion's exact kind | Accept `prompt` **or** `cancelled` (typed failure either way), plus survival + nothing-persisted | rpassword's own `Err(Interrupted)` (the design's F8 path) and the select-cancel race for the same keystroke (the raise arms the epoch mid-dispatch); both are correct typed cancellations with identical persistence guarantees |
| Q12 | The "second ^C forces with 130" pty form | External `kill -INT` ×2 while idle (deterministic: no `Progress` is possible while the loop is parked) | The at-keyboard double-press during dispatch races the loop's acknowledgement — inherently flaky; the pty pin's job is proving the ladder's force rung + restore-before-exit, which the external form does deterministically and which is the design's own escape hatch (design §5: "the ladder counts it — the escape exists"). **Stage-3 correction (2026-10-27): the idle-at-prompt form does not exist in the delivered terminal world — with `TERM` set, rustyline's own SIGINT handling consumes an external SIGINT while `readline` is active (F5 grammar, prompt redraw observed) and it never reaches the watcher. The pin ships as the termination-family double-SIGTERM-during-drain form (same ladder, same `Force` side effect; see §8 case 4); the at-keyboard double-press during dispatch remains the design's racy escape hatch and stays unpinned by intent, not by weakening |
| Q13 | `Exit`-arm interaction with a pending epoch | The drain lives at the Execute-arm start only; `exit`/`quit`/EOF break without draining | A pending epoch past loop end is meaningless (the process exits); draining there would be dead code |

## 12. Risks

- **"Cancelled but it landed"** (D-0/D-2): a cancelled line's
  already-delivered engine command still commits. Mitigated by
  naming it in the `Cancelled` detail text and the ADR 0011
  amendment — the alternative (engine cancellation tokens) is actor
  redesign for a cosmetic race (design §5).
- **The orphaned prompt read**: a cancelled ceremony whose editor
  thread is still blocked inside `readline` reconciles only when the
  user answers it — the answer's oneshot send fails silently, the
  queued `Next` resumes editing, one stale prompt read is consumed
  as a no-op. Trace-verified against the handshake's
  one-verdict-per-event rule (`repl.rs:233-259`); the pty cancel
  cases exercise around it. Cosmetic (a stale prompt on screen), not
  a wedge.
- **The rpassword raise arms the ladder**: the masked read's re-raise
  is a real SIGINT the handler counts (F8) — a ^C at a masked prompt
  leaves a pending-or-consumed epoch that the next dispatch drains
  (Q3), and two rapid presses at a masked prompt can force. Both
  outcomes are safe (typed failure / restore+130); recorded so the
  reviewer does not read the arming as a missed select arm.
- **Prompt labels on stdout** (F16/D-3): consistent with where
  `taskboard> ` already writes; recorded in ADR 0011; a `--json`
  session's stdout was already not byte-pure between payloads.
- **Behavior-gentling windows**: external ^C in the between-readlines
  cooked micro-window is now armed-and-drained instead of lethal;
  signals racing the final exit can force (143/129) instead of
  racing a clean 0. Both strictly no-worse than today's default
  disposition.
- **Pty flakiness**: all new cases gate on markers/readline counters
  with blocking reads and an outer `timeout` (suite rules, no
  sleeps); the delayed-server case uses a 30 s delay against a 60 s
  guard. Any observation goes to the `flaky-test` watchlist per
  AGENTS §7 — never fixed opportunistically here.

## 13. Diff estimate & commit plan

| file | shape |
|---|---|
| `src/signals.rs` (new) | ~350–400 (machine + watcher + restore glue + ladder tables) |
| `src/repl.rs` | ~+120/−30 (select arms, drain, prompt arm, `prompt_answer`, restore extraction, unit tests) |
| `src/completion.rs` | ~+20 (suspend flag + test) |
| `src/commands/mod.rs` | ~+12 (`Cancelled` + kind + detail) |
| `src/commands/connect.rs` | docs + `pub(crate) parse_confirm` (~±10) |
| `src/lib.rs` | +1 |
| `tests/repl.rs` | ~+250 (six loop tests + inert-signal call-site updates) |
| `tests/cli_tty.rs` | ~+330/−60 (rewrite + six new cases + pidfile fixture) |
| `docs/adr/0011`, `docs/adr/0014`, `CHANGELOG.md` | ~+45 |

**Estimated total: ≈ 1,100–1,300 additions / ≈ 100 deletions across
11 files — size M**, matching the dispatch table. No migration, no
insta-shape change, no new dependencies (the chunk touches no (w)
lane).

Six commits per §10's order, suggested subjects:
`feat(app): the signal ladder state machine` →
`feat(repl): cancel a dispatching line on SIGINT` →
`feat(repl): ceremony prompts ride the line editor` →
`feat(repl): the session signal watcher and restore` →
`test(tty): the signal and prompt-grammar pty pins` →
`docs(adr): the REPL signal and prompt amendments`.
