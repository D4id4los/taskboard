# [Plan] Signal Handling, Once — Design (DR-2)

- **Date**: 2026-10-26
- **Kind**: design round deliverable — DR-2 of the 2026-10-25 backlog
  triage dispatch (§14: "signal handling, once"). One signal design for
  the five issues that each name the same resolution; this document is
  the round's entire output. Zero production code; the PR is the plan
  (this file) plus the implementation issues opened alongside it.
- **Origin**: issues #174 (scoped SIGINT in the connect ceremony),
  #192 (REPL line cancellation), #196 (IPC client
  deadlines/cancellation), #200 (line-discipline restore on signal
  death), #162 (signal-escalation policy) — every body names "one
  signal design, designed together".
- **Normative inputs** (all verified against `origin/main` @
  `cdc41c6`; the studied files are byte-identical to `main` @
  `cf3a117` where the round started):
  - ADR 0011 §"Exit and signal semantics" (the contract this round
    amends), ADR 0012 §4.8 (the budget policy) and its additive-
    amendment convention, ADR 0014 §6 (the disposition handshake
    table) and the "Recorded verification: Ctrl-C during a masked
    read" note.
  - The REPL editor thread (`crates/taskboard-app/src/repl.rs`), the
    IPC pair (`src/ipc/{client,server,protocol}.rs`), the daemon
    lifecycle (`src/main.rs`, `src/bootstrap.rs`), the ceremony prompt
    machinery (`src/commands/connect.rs`, `src/session_ceremonies.rs`),
    the desktop/kiosk watchers (`src/ui.rs`, `src/main.rs`).
  - Dependency facts read from the vendored sources:
    `signal-hook-registry` 1.4.8 (tokio's signal backend) **sets
    `SA_RESTART`** (`src/lib.rs:185-195`); `rpassword` 7.5.4's masked
    read on ^C **raises SIGINT and then returns
    `Err(ErrorKind::Interrupted)`** (`src/lib.rs:129-140`,
    `src/unix.rs:162-167`); rustyline 19 maps the ^C byte to
    `ReadlineError::Interrupted` in raw mode and writes prompts to
    stdout (`src/lib.rs:705-740`).
- **Branch / worktree**: `docs/signal-handling-design` in
  `../taskboard-WT-docs-signal-handling-design`, off `origin/main` @
  `cdc41c6`.
- **Status**: DRAFT for user review. The 2026-10-26 ruling pass did
  not reach the user, so every design fork below is resolved with a
  **recommendation** (first option, reasons, rejected alternatives
  recorded) and the four direction-level forks are flagged
  **⟂ RULING WANTED** — flipping any of them at review is local
  (each is one numbered decision with its consequences already
  written out). The implementation issues opened alongside this plan
  carry the same flag in their bodies.

---

## 1. Mission & binding constraints

Design **one signal policy** for the whole process surface — REPL,
one-shot CLI, IPC client, daemon, GUI, kiosk — so that Ctrl-C and
external termination mean the same thing everywhere, the terminal is
never left broken, and supervision (systemd, kiosk bring-up) can read
our exits. The five origin issues each hold one fragment; the round
closes them with one vocabulary:

- **Cancel**: SIGINT is the user's "stop what you are doing" gesture.
  It cancels the in-flight operation, never the process.
- **Terminate**: external termination signals (SIGTERM/SIGHUP) end the
  process **after restoring the terminal**, through the graceful path,
  with a force rung behind it.
- **Orphan completion**: a cancelled *waiter* never rolls back durable
  work already handed to the engine/sync actor — the work completes;
  only the waiting stops. (This is already today's semantics whenever
  a client dies mid-request; the design names it instead of
  discovering it per feature.)

Binding, not re-arguable:

- AGENTS §2/§3: signals are process glue — everything lands in
  `taskboard-app`; no domain, state, storage, or sync changes; no new
  cross-actor traffic (a watcher task + app-internal channels, never
  domain `SystemEvent`s).
- `#![forbid(unsafe_code)]` stands (AGENTS §6): no hand-rolled
  `sigaction`; only `tokio::signal` (already a workspace feature) and
  the existing `nix` term feature. **No new dependencies.**
- ADR 0014's disposition handshake (strict alternation, prompts
  answered on the editor thread, secrets never in history) keeps its
  shape; this round only changes *which reader* answers `Line`/
  `Confirm` prompts and *what wakes the loop* during dispatch.
- ADR 0012's wire additivity rules: any server behavior change is
  additive, no `PROTOCOL_VERSION` bump, no envelope shape change.
- AGENTS §5 testing rules: typed outcomes only, no sleeps, injected
  seams, pty tier for the real terminal paths.

## 2. Verified baseline — numbered facts

| # | Fact | Evidence |
|---|---|---|
| F1 | The only signal machinery today is `tokio::signal` in `wait_for_signal` (ctrl_c everywhere + SIGTERM on unix, registration failure degrades to ctrl_c-only), consumed by the daemon tail, the GUI exit watcher, and the kiosk watcher | `bootstrap.rs:1156-1178`, `main.rs:392-399`, `ui.rs:431`, `main.rs:550-563` |
| F2 | Second signal during the daemon drain force-exits 130 | `main.rs:424-430` |
| F3 | A degraded drain (budget hit, sync actor aborted) only logs; the process exits **0** | `main.rs:433-437` |
| F4 | The REPL editor thread has exactly three states: editing (`readline`, raw mode), awaiting verdict (`blocking_recv`, cooked mode), answering a prompt (sync read on the editor thread) | `repl.rs:206-260`; ADR 0014 §6 table |
| F5 | ^C while editing is a *byte* (ISIG off in raw mode): rustyline answers `ReadlineError::Interrupted` → `LineEvent::Interrupted` → line cleared, session continues. No signal exists | `repl.rs:218-221`, `repl.rs:656-659` |
| F6 | ^C while a line/ceremony dispatches is a *kernel SIGINT* (editor parked in cooked mode): default disposition → the process dies mid-line. Safe (state durable per `execute`; OS releases the lock) but rough | `repl.rs:519-523`; ADR 0011 §"Exit and signal semantics"; issue #192 |
| F7 | Ceremony prompts: `Line`/`Confirm` are cooked `stdin().read_line` (prompt label on stderr); `Masked` is rpassword (raw, ISIG off, `*` feedback) | `connect.rs:114-173` |
| F8 | rpassword on ^C raises SIGINT **and returns** `Err(Interrupted)`. Under the default disposition the raise kills the process before the `Err` matters; under **any installed handler** the raise is consumed and the `Err` is the typed `CommandError::Prompt` cancellation path — zero extra machinery | rpassword 7.5.4 `lib.rs:129-140`, `unix.rs:162-167`; ADR 0014's recorded verification; pty pin `cli_tty.rs:332` |
| F9 | tokio's signal registration (via `signal-hook-registry`) **sets `SA_RESTART`**: installing a handler never `EINTR`s blocking reads — a caught SIGINT cannot by itself abort a cooked `read_line` | signal-hook-registry 1.4.8 `lib.rs:185-195` |
| F10 | `TtyStateGuard` restores the pre-session termios on drop; signal death runs no destructors, so SIGINT-mid-dispatch and external SIGTERM/SIGHUP leave the pane raw until `stty sane` | `repl.rs:858-908`; issue #200 |
| F11 | The IPC client has **no read deadline** for `exec`/`status`/`shutdown` (stated policy); connect is budgeted 1 s, detach/attach 30 s | `client.rs:1-45` |
| F12 | The IPC dispatcher is serial; a `Shutdown` request is answered *inside* the dispatcher, i.e. **queued behind an in-flight exec** — a wedged exec starves `daemon stop` over IPC; only a signal recovers | `server.rs:176-186`, `server.rs:284-305` |
| F13 | A client that dies mid-request leaves the daemon completing the work (engine commands are durable once `execute` replies; the sync actor finishes its cycle) — unnamed-but-existing orphan completion | `client.rs:140-149`; ADR 0011's durability rule |
| F14 | One-shot CLI installs no handler: Ctrl-C is default death (shell 130). The `StdinPrompts` impl is `ready(sync read)` — the read executes inside `poll` on the calling thread, so a `select!` arm can never cancel it (F9 blocks the EINTR route too) | `connect.rs:175-197` |
| F15 | Verb self-bounding exists: `--timeout-secs` flags on the network verbs (e.g. `boards select` default 60 s), engine receipts, bounded joins | `cli.rs:136/211/250` |
| F16 | rustyline writes prompts to stdout (the `taskboard> ` prompt already rides stdout today, `--json` sessions included); rpassword's masked prompt goes to `/dev/tty` | rustyline 19 `lib.rs:705-740`; rpassword `unix.rs` `DEFAULT_OUTPUT_PATH` |

## 3. The design, decision by decision

### D-0 — The one policy (the invariant)

> **SIGINT means "cancel the current operation". External termination
> (SIGTERM/SIGHUP) means "restore the terminal, then end gracefully".
> Every ladder ends in a force rung. Death by keyboard is reserved
> for the explicit exits (`exit`/`quit`/Ctrl-D) and the force rungs.**

Every decision below is an application of this sentence to one
surface. The point of "one signal design, once" is that the sentence
is written **once** — in a new `taskboard-app/src/signals.rs` module
whose doc comment carries it — and every surface consumes the same
ladder helper, instead of five features growing five local policies.

### D-1 — `signals.rs`: the ladder helper and the installation points

A small module in `taskboard-app` (process glue, AGENTS §2):

- A pure **ladder state machine**: input events `Signal(kind)` and
  `Progress` (the graceful path completed / cancellation
  acknowledged), output actions `Initiate`, `Escalate`, `Force`. Pure
  core, exhaustively unit-tested; no I/O. The rung count is
  surface-configured (REPL cancel/force, daemon graceful/salvage/
  hard — D-2/D-6) but the counting/ordering logic exists once.
- A **watcher task** (thin, untested glue per house rule): installs
  `tokio::signal` listeners, feeds the state machine, performs the
  two side effects that must never be duplicated — tty restore (D-4)
  and `std::process::exit` on `Force`. Restore-before-exit ordering
  lives here, once.
- Installation points: the REPL session entry (`run_session`), the
  one-shot non-ceremony dispatch paths (D-5), and the existing
  daemon/GUI/kiosk tails *migrate onto the helper* (their
  `wait_for_signal` call sites become ladder consumers — behavior
  per D-6, no new installs).

No new dependencies (`tokio/signal` and `nix/term` are already
workspace features); no `unsafe`; Windows gets the ctrl_c listener
only, with the unix-only signals `cfg`-gated (D-7).

**Consequence of installing any SIGINT handler (F9)**: default
disposition death disappears for the whole process from that point
on. That is the design's intent — but it means every surface that
installs must also *select* on the cancellation (or accept that ^C is
consumed silently, which the design never leaves unhandled: the
force rung is always armed behind the first signal).

### D-2 — REPL: Ctrl-C during a running line/ceremony ⟂ RULING WANTED

**Recommended: cancel first, force second.**

The session loop wraps the `Execute` arm's await in a `select!`
against the cancellation watch (the injected seam; production wires
the ladder watcher):

- **First SIGINT during dispatch**: the line future is dropped (the
  select's drop semantics), the loop reports the new typed
  `CommandError::Cancelled` (kind string `"cancelled"`, same envelope
  as every line failure — ADR 0011's "a failed line never aborts the
  session" rule extends to cancelled lines), sends `Next`, editing
  resumes. A ceremony cancelled this way is exactly today's typed
  `Prompt`-error path shape: pre-persist at every await point that
  can be cancelled (prompts and network), and the local persist steps
  between them are quick and non-yielding — a cancellation cannot
  land *inside* verify→persist→store.
- **What "cancel" cancels**: the *wait* and the teardown-able work
  (an in-flight HTTP request future dies with the drop). An engine
  command already delivered **still commits** — orphan completion
  (D-0). The next line observes its effect. This is documented in
  the cancelled envelope's UX text and ADR 0011's amendment, not
  hidden.
- **Force rung**: the *watcher* (not the loop — a future that blocks
  its thread without yielding can never be rescued by a `select!` on
  the same task) counts SIGINTs: a second SIGINT while the line is
  still in flight (cancellation not yet acknowledged) performs
  restore-tty → `exit(130)`. This preserves today's one guarantee
  worth keeping: *the keyboard can always eventually kill the
  session*. No timer is involved (a grace window would need clock
  machinery for no benefit — the user presses again if the first
  press did nothing).
- Quitting the session stays `exit`/`quit`/Ctrl-D; ^C at the prompt
  keeps clearing the line (F5 — byte world, untouched).

Rejected alternatives: **cancel-only** (a wedged line becomes
unkillable from the keyboard — regresses today's eventual-death
guarantee); **double-^C-at-empty-prompt quits** (Python-style — adds
a third ^C semantic for a quit path that already has three spellings;
rejected as vocabulary growth, revisit on demand).

### D-3 — Ceremony prompts become editor-native reads (issues #174's core)

The `Line` and `Confirm` prompt kinds move from cooked
`stdin().read_line` onto the editor thread's own rustyline
`readline(label)` call (the `Masked` kind stays rpassword — F8
already gives it the typed path under the handler):

- **Why (F9 is the reason this is not optional)**: a caught SIGINT
  cannot abort a cooked read (`SA_RESTART` restarts it); in raw mode
  ^C is a byte, and rustyline already answers it with `Interrupted`.
  Routing `Line`/`Confirm` through rustyline gives every prompt kind
  the same ^C grammar as the prompt line itself — the asymmetry
  issue #174 describes (one keystroke earlier vs later = clear vs
  quit) disappears for the whole ceremony.
- Mechanics: the disposition `Prompt` arm calls
  `editor.readline(&label)`; `ReadlineError::Interrupted` maps to
  `Err(ErrorKind::Interrupted)` on the oneshot → the ceremony's
  existing `CommandError::Prompt` mapping (a cancelled prompt is not
  a declined one — the distinction ADR 0014 already records).
  `Confirm` parses its y/N from the returned line with the existing
  `parse_confirm`. History is untouched (only `history.record` on
  submitted session lines writes it — prompts never call it, so
  answers stay out of history and completion by construction). The
  completion helper is suspended for prompt reads (`set_helper(None)`
  / restore — no tab-completion inside a server-URL prompt).
- **Consequence (F16, recorded honestly)**: `Line`/`Confirm` prompt
  labels move stderr → stdout (rustyline writes prompts to stdout).
  The `taskboard> ` prompt already rides stdout today, so an
  interactive `--json` session's stdout was already not byte-pure
  between payloads; the plan records this as the accepted cost of one
  prompt grammar. (The masked prompt already goes to `/dev/tty`,
  unchanged.) Piped/one-shot stdin (non-TTY) never reaches this path
  — that world is D-5's.
- **One-shot ceremonies keep today's death semantics** (F14: the
  `StdinPrompts` sync read executes inside `poll`, un-cancellable by
  any `select!`, and installing a handler there would *consume* ^C
  and leave the read silently running — strictly worse than dying).
  The one-shot `connect`/`login` therefore install no handler;
  Ctrl-C at their prompts dies pre-persist (safe, ADR 0014's
  verification note stands for that surface). The REPL is the
  interactive ceremony surface; the one-shot is the scripted one.
  Rejected alternative: spawning the editor-handshake machinery for
  one-shot ceremonies — real machinery for a surface whose user
  pressed Ctrl-C precisely to leave.

### D-4 — External termination restores the terminal first (issue #200) ⟂ RULING WANTED

**Recommended: SIGTERM and SIGHUP get restore-then-graceful;
SIGQUIT stays at default disposition; SIGKILL stays unrestoreable.**

- The REPL's ladder watcher (D-1) also listens for SIGTERM/SIGHUP
  (unix; `cfg`-gated). On receipt, **the first action is the termios
  restore** (`TtyStateGuard`'s saved state, shared into the watcher —
  the restore happens before any potentially-slow bounded join, so
  the pane is sane from the first moment of shutdown), then the
  watcher delivers a termination event into the session loop, which
  ends exactly like Ctrl-D: `app.shutdown()` (bounded), exit 0.
  With SIGHUP the tty is already gone — the restore fails harmlessly
  (ignored error), the graceful path is still correct and bounded.
- The force rung covers a wedged drain: a second external signal →
  restore (idempotent) → `_exit(128 + signum)` (143/129).
- SIGQUIT keeps the default disposition: the ^\ core-dump convention
  is a diagnostic the design should not swallow, and anyone sending
  SIGQUIT wants the corpse, not the graceful path. The pane cost is
  documented (runbook: `stty sane`).
- SIGKILL cannot be handled by definition — runbook documents
  `stty sane` as the user recovery, exactly as issue #200 proposes.
- Windows: no SIGTERM/SIGHUP analog reaches a normal process;
  console close is process death. The ctrl_c listener (D-1) and
  rustyline/rpassword's per-call console-mode restore are the whole
  Windows story; documented, not engineered further this round.

Rejected alternatives: **+SIGQUIT restore-then-graceful** (maximizes
the restored-pane guarantee but eats the core-dump diagnostic);
**restore-then-die without the graceful attempt** (state is durable
so it is *safe*, but there is nothing to gain — the graceful path is
bounded and already runs at every other exit).

### D-5 — IPC: client cancellation, read deadlines, an out-of-band `daemon stop` (issue #196) ⟂ RULING WANTED

**Recommended: client-side deadlines + Ctrl-C tidy exit + a control
lane for `Shutdown`; no `Cancel` verb.**

1. **One-shot Ctrl-C (non-ceremony verbs)**: the transport arm and
   the in-process dispatch both wrap their await in the cancellation
   select (D-2's mechanism, one consumer wider). On SIGINT: the
   connection closes / the dispatch future drops, the typed
   `cancelled` envelope prints, exit **130** (the shell's ^C
   convention is kept — the difference from today is the typed
   report, the tidy teardown, and for the relayed case a daemon that
   is no longer told a client vanished mid-read). Ceremony verbs
   (`connect`/`login`) are excluded per D-3.
2. **Client read deadline**: `exec`/`status`/`shutdown` gain a read
   budget — derived from the verb's own `--timeout-secs` + a fixed
   margin (10 s) when the verb has one (F15), a default ceiling
   (30 s) when it does not, overridable by a new `[ipc]
   client_read_timeout_secs` (0 disables — the recorded supervision
   stance). Expiry is a new typed `CommandError::IpcTimeout` (kind
   `"ipc-timeout"`, distinct from transport garbage), exit 1. This
   replaces the "no read deadline" sentence in ADR 0012 §4.8 with a
   bounded version of the same stance: verbs still self-bound, the
   deadline only catches the *wedged daemon* case the policy
   name-checks, and it never fires before the verb's own honest
   timeout.
3. **Control lane for `Shutdown`** (fixes F12): the accept path
   already decodes the request in the connection task — a decoded
   `Shutdown` is answered and triggers `request_shutdown()`
   **directly**, without entering the serial dispatcher queue. The
   dispatcher keeps its one-in-flight fairness for everything else.
   Wire-additive by ADR 0012's convention (same variant, same
   envelope, earlier handling — no version bump; an old daemon
   behaves as today, a new daemon answers `daemon stop` even mid-
   wedge).
4. **No `Cancel` verb** — recorded with the reason: orphan
   completion (D-0) makes server-side cancellation valueless for
   correctness (durable commands should complete; a relayed `sync`
   finishing after the client left is harmless — the next command
   observes the same converged state), and preempting the serial
   dispatcher's in-flight future would restructure the engine-
   fairness model ADR 0012 §2.8 pins. If a future verb ever grows
   genuinely expensive-and-pointless-when-unwatched work, that
   verb's round re-opens this with the evidence.

Rejected alternatives: **full server-side cancellation** (per-request
cancellation tokens in the dispatcher + a wire verb — honest Ctrl-C
mirror, but restructures the fairness model for no correctness gain;
see 4); **document-only** (leaves F12's `daemon stop` starvation and
the deadline question open — the round exists precisely to decide
them).

### D-6 — Daemon ladder and the exit-code table (issue #162) ⟂ RULING WANTED

**Recommended: three rungs, honest codes.**

| Rung | Trigger | Behavior |
|---|---|---|
| 1 | first SIGINT/SIGTERM/`daemon stop` | today's graceful drain: stop accepting, bounded joins (`shutdown_timeout_secs`), socket removed, `shutdown_from_arc` |
| 2 | second signal while rung 1 is still running | skip the bounded joins, abort actors, one salvage attempt at an engine flush under a small fixed bound (2 s), exit `128+signum` (130/143) |
| 3 | third signal | immediate `_exit(128+signum)` — the "rung 2 itself wedged" escape |

Exit-code contract (supervision-visible, runbook-documented):

| Code | Meaning |
|---|---|
| 0 | clean: full drain, no aborts — including signal-initiated graceful and `daemon stop` |
| 1 | **degraded drain**: budget hit and/or sync actor aborted (today this exits 0 with a log line, F3 — invisible to supervision) |
| 130 / 143 | forced by SIGINT / SIGTERM at rung 2+ |

The GUI and kiosk tails consume the same ladder helper (their
`drain_and_shutdown` call sites are the shared seam, F1/F15), so the
ladder is written once for all lifecycle surfaces. The runbook gains
the table, the systemd mapping (`Restart=`, `SuccessExitStatus=`
guidance for kiosk units that treat 143 as a clean stop), and the
`stty sane` recovery for SIGKILL (D-4).

Rejected alternatives: **two rungs + honest codes** (drops the
salvage flush — rung 2 becomes today's flat `exit(130)`; the middle
rung is the entire kiosk-graded-response ask of #162, and it is
cheap: the flush is one bounded call); **document today** (leaves
the degraded-drain exit 0 lie and the flat ladder in place — nothing
for supervision to grade).

### D-7 — Where things live; platform shape

- `crates/taskboard-app/src/signals.rs`: the policy sentence (D-0),
  the ladder state machine, the watcher, the restore-before-exit
  glue. Nothing else changes crate topology; no domain/state/
  storage/sync diffs; no UI-slint diffs.
- Unix-only signals (`terminate`, `hangup`, SIGQUIT policy) are
  `cfg(unix)`; the ctrl_c listener and the D-5 client behavior are
  cross-platform. Cross-compile to the Windows target stays green
  (the `cfg` discipline is the whole cost — no `unsafe`, no
  winapi-signal work this round).
- Handlers are installed **only** on production paths (`run_session`,
  the one-shot transport/dispatch arms, the lifecycle tails) — never
  in library code reachable by hermetic tests, so no test ever
  races a harness's own Ctrl-C. The seams (D-9) carry the tests.

### D-8 — Docs amendments (amendment-not-rewrite, dated notes)

- **ADR 0011**: the "Exit and signal semantics" section gains a dated
  amendment — ^C during dispatch now cancels the line (typed
  `cancelled`); the force rung; the external-termination restore;
  the REPL-ceremony prompt grammar (D-3). The ruled v1 text stays
  visible below the note.
- **ADR 0012**: §4.8's budget policy amended — client read deadline
  (derived + override + `ipc-timeout`), the `Shutdown` control lane,
  the no-`Cancel` recording (D-5).
- **ADR 0014**: the "Recorded verification" note gains a dated
  addendum — under the session's installed handler, the masked read's
  ^C becomes the typed `Prompt` cancellation on the terminal tier
  too; the default-disposition branch remains true for the *one-shot*
  ceremony surface (D-3).
- **Runbook** (`docs/kiosk_bringup_runbook.org`): exit-code table,
  systemd mapping, `stty sane` recovery.
- **CHANGELOG**: the implementing chunks add Unreleased entries
  (REPL ^C semantics, daemon exit codes, IPC deadline are
  user-observable).

### D-9 — Testability: the seams

- The **session loop's cancellation input** is an injected
  `watch::Receiver`/oneshot (production: the ladder watcher). Hermetic
  loop tests fire it mid-dispatch (a line whose future awaits a
  never-answered scripted prompt is the cancellable stand-in) and
  assert: typed `cancelled` reported, session continues (the next
  line executes), exactly one `Next` disposition per event.
- The **ladder state machine** is pure: event streams in, action
  sequences asserted (first/second/third signal × progress
  interleavings, per-surface rung configs). No clock, no I/O.
- The **force-exit and restore side effects** stay in the thin
  watcher glue (untested by house rule); everything above them is.
- **Prompt grammar**: disposition-peer scripts answer
  `Err(Interrupted)` (the byte-world mapping) — the existing
  `disposition_tests` pattern extends; the oneshot error path is
  already pinned.
- **Pty tier** (`tests/cli_tty.rs`, `script(1)` harness): the
  masked-read ^C pin **changes semantics** — from "default
  disposition kills / ignored disposition survives" to "handler
  world: typed cancel, session survives, nothing persisted" (this is
  the design taking effect, not a weakened test — the persistence
  half of the pin is unchanged and still load-bearing); new pty
  cases: ^C during a stalled `sync` line cancels and the session
  answers the next line; SIGTERM mid-`readline` leaves the pane sane
  (the harness's `stty` probe before/after); second ^C forces with
  code 130.
- **IPC**: deadline expiry against a scripted stalling server (tokio
  time pause/advance — the budget is a `tokio::time::timeout`, so no
  wall clock ever runs); the control lane against a dispatcher task
  parked on `std::future::pending()` (the `drain_tests` precedent,
  `server.rs:688`) — `daemon stop` still answers.
- No sleeps anywhere; typed outcome asserts only (AGENTS §5).

## 4. Decision record

| # | Decision | Status |
|---|---|---|
| D-0 | One policy sentence, one `signals.rs`, orphan completion named | recommended |
| D-1 | Ladder helper + watcher; tokio-signal only; no new deps; no `unsafe` | recommended |
| D-2 | REPL: cancel-first/force-second; typed `cancelled`; force lives in the watcher | recommended **⟂ ruling** (vs cancel-only; vs +double-^C quit) |
| D-3 | `Line`/`Confirm` prompts ride rustyline; masked stays rpassword; one-shot ceremonies keep death; prompts stderr→stdout recorded | recommended |
| D-4 | SIGTERM/SIGHUP restore-then-graceful; SIGQUIT default; SIGKILL documented | recommended **⟂ ruling** (vs +SIGQUIT; vs restore-then-die) |
| D-5 | IPC: client cancel+130, derived read deadline + `ipc-timeout`, `Shutdown` control lane, no `Cancel` verb | recommended **⟂ ruling** (vs full server cancel; vs document-only) |
| D-6 | Daemon: 3-rung ladder; exit codes 0/1/130/143; GUI/kiosk share the helper | recommended **⟂ ruling** (vs 2 rungs+codes; vs document-only) |
| D-7 | All app-crate, `cfg(unix)` discipline, handlers never on test paths | recommended |
| D-8 | ADR 0011/0012/0014 dated amendments + runbook + CHANGELOG (ride their chunks) | recommended |
| D-9 | Seams per §3 D-9; pty pin semantics change sanctioned here | recommended |

## 5. Risks

- **The "cancelled but it landed" perception** (D-2): a cancelled
  line's already-delivered engine command still commits. Mitigated by
  naming it (orphan completion, D-0) in the envelope text and ADR
  0011's amendment — the alternative (engine-level command
  cancellation tokens) is actor-redesign for a cosmetic race.
- **Prompt destination change** (D-3, F16): `Line`/`Confirm` labels
  stderr→stdout. Consistent with where the REPL prompt already
  writes; recorded; flippable only by hand-rolling a raw reader
  (rejected — duplicates rpassword-class machinery).
- **Exit-code churn for supervision** (D-6): units keyed on
  "0 is the only good exit" see 143 on a forced-but-fine stop. The
  runbook's systemd mapping owns this; the kiosk service file is
  in-repo (`contrib/`) and the implementing chunk updates it.
- **External SIGINT during a raw prompt read is consumed silently**
  (F9 + D-3): `kill -INT` while rpassword/readline holds the terminal
  does nothing visible until the second signal forces (the ladder
  counts it — the escape exists). At-keyboard ^C is unaffected (byte
  world). Documented; acceptable.
- **Handler-process interactions**: installing a handler changes
  `rpassword`'s re-raise from death to no-op (the *intended* change,
  F8) and makes any missed select-arm a silent swallow — mitigated by
  the rule that every installing surface selects on cancellation or
  arms the force rung (D-1's "never leaves unhandled").
- **Test-harness interference**: none by construction (D-7: handlers
  only on production paths); the pty tier is the only place real
  signals occur in tests.

## 6. Implementation chunks (the follow-up round)

Three chunks, one PR each; R first (the module lands), then I and D
in either order. Sizes assume the decisions as recommended; any ⟂
ruling flips at most its own chunk's interior.

| Chunk | Scope | Closes | Size |
|---|---|---|---|
| **R — REPL signals** | `signals.rs` (ladder + watcher + restore), session-loop cancellation select + typed `Cancelled`, prompt reads on rustyline, one-shot-ceremony exclusion, pty pin updates, ADR 0011 + 0014 amendments | #174, #192, #200 | M |
| **I — IPC client** | one-shot cancellation select + exit 130, derived read deadline + `IpcTimeout` + `[ipc]` override, `Shutdown` control lane, no-`Cancel` recording, ADR 0012 amendment | #196 | S/M |
| **D — daemon ladder** | rung 2 salvage + rung 3, exit-code table incl. degraded-drain=1, GUI/kiosk adoption of the helper, runbook + `contrib/` service update, CHANGELOG | #162 | S/M |

The five origin issues stay open until their chunk's PR lands (house
archive rule: closing is the archive). The implementation issues
opened with this plan (one per chunk) carry the ⟂-ruling flags in
their bodies; if a ruling flips a decision, the issue body is edited
at plan review — the plan is the single source.

## 7. References

- Issues: #174, #192, #196, #200, #162 (origins); the dispatch
  round: `.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md`
  §14 DR-2.
- ADRs 0011 §"Exit and signal semantics", 0012 §4.2/§4.8 + additive
  amendment notes, 0014 §6 + recorded verification.
- Code anchors: §2's table (all `crates/taskboard-app/src/…`).
- Dependency evidence: vendored `signal-hook-registry` 1.4.8,
  `rpassword` 7.5.4, `rustyline` 19.0.0 sources (§ Normative inputs).
- Style precedent for round deliverables:
  `.artifacts/plans/2026-10-25-reminder-rule-sync-design-plan.md`
  (DR-1 stage 2).
