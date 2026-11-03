# The REPL editor re-arm race — chunk 1.1 of backlog round 3 (plan)

**Branch**: `fix/repl-editor-rearm-race` · **Issue**: #572 (p1) · **Rider**: #524
**Stage**: TDD-first implementation plan (stage-1 planning, 2026-11-03) · **One PR** for
the chunk (plan + fix + tests + docs ride the same branch).

## 0. Mandate and normative sources

Chunk contract (`.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md` §2 chunk
1.1): root cause identified **with evidence** (deterministic or reliably-loaded
reproduction; mechanism explained from the code, not guessed); the fix makes the loaded
reproduction pass (the #572 recipe's loop, ≥ the issue's sample size, plus the sabotage
control); the session-survives invariant pins stay unweakened; #524's rider disposition
recorded with evidence; the existing cli_tty suite green.

Non-goals (normative): **no harness changes** — the progress-deadline seam
(`cli_tty.rs:39-190`) is working as designed, it found this bug; no unrelated
session-machinery refactors.

Normative sources:

- #572 — signature, evidence, reproduction recipe, suspect area, triage policy
  ("an occurrence is expected evidence of this issue, not a harness regression";
  "do not weaken the test's pins").
- #524 — the rider: two post-fix observations (2026-11-02 PR #586 coverage run at
  30.097 s, 845 tests concurrent; 2026-11-03 chunk-7 gate run,
  `no pty output for 30s (progress budget)`), both passing in isolation.
- ADR 0011 amendment (2026-10-27, `docs/adr/0011-…org:167-197`): ^C during a
  dispatching line cancels the line (typed `cancelled`); at a prompt an external SIGINT
  is a line-clear; ceremony prompt grammar — `ReadlineError::Interrupted` is a typed
  `prompt` cancellation; the masked read stays rpassword on `/dev/tty`.
- ADR 0014 §6 (`docs/adr/0014-…org:82-101`): the disposition-handshake table —
  "answering prompt | the prompt's sync read (`Masked` = rpassword) | woke by **the
  user's input**". The wedge is exactly a violation of this row: the prompt is woken by
  no input at all. §"Recorded verification" (`:137-154`) records D7; its last line
  already notes the masked-read ^C subtlety as backloged — this chunk is that backlog
  item.
- The repl signal-cancel plan §12 row 1 / §11 Q11
  (`.artifacts/plans/2026-10-27-repl-signal-cancel-plan.md:506,577`): the masked-read ^C
  may surface as `prompt` (rpassword's own `Err(Interrupted)`, the design's F8 path) or
  `cancelled` ("the select-cancel race for the same keystroke — **the raise arms the
  epoch mid-dispatch**"). Both are correct typed cancellations; both survive.

## 1. The mechanism map (facts, from code and vendored sources)

The session's editor handshake (ADR 0014 §6, implemented at
`crates/taskboard-app/src/repl.rs`):

- Editor thread (`spawn_line_reader`, repl.rs:207-312, thread name
  `repl-line-reader`): `readline(PROMPT)` → deliver `LineEvent` → **unconditionally**
  wait for the verdict (`verdicts_rx.blocking_recv()`, repl.rs:268-304). A
  `Prompt { kind, reply }` verdict is answered **on the editor thread**;
  `PromptKind::Masked` runs `connect::StdinPrompts::read_secret_sync`
  (repl.rs:282-297).
- Session loop (`repl_loop`): `Interrupted` → `resume()` (repl.rs:760-763); `Execute`
  wraps the ceremony in one future selected against the cancel/terminate watches
  (repl.rs:873-891); the cancel arm reports `CommandError::Cancelled`, acks, and falls
  through to the per-line `resume()` (repl.rs:883-887, :897) — one verdict per event,
  `Next` queued into the capacity-1 dispositions channel.
- Masked read (`read_secret_sync`, `commands/connect.rs:139-156`): on a terminal,
  `rpassword::prompt_password_with_config("App password: ", mask '*')`.

rpassword 7.5.4 internals (vendored, `~/.cargo/registry/src/…/rpassword-7.5.4/`) — the
load-bearing facts:

1. **The prompt label is written BEFORE the terminal is configured**
   (`lib.rs:275-291`): `prompt_password_with_config` writes and flushes the label to
   `/dev/tty` first; only then does `read_password_with_config` →
   `RawModeInput::new` → `apply_terminal_configuration` run.
2. **`apply_terminal_configuration` clears `ECHO | ICANON | ECHONL | ISIG`** and sets
   `VMIN=1` (`unix.rs:129-151`). From that moment the ^C byte is **data**, not a
   signal.
3. **The ^C-as-data arm re-raises SIGINT itself** (`lib.rs:128-139` →
   `unix.rs:162-167`): `state.abort()`, `libc::raise(SIGINT)`,
   `Err(Interrupted)`. The raise is what Q11 named: it arms the session watcher's
   cancel epoch mid-dispatch (`signals.rs:426-429`), the loop's cancel arm fires
   (`cancelled` envelope), while rpassword's own `Err` races it as the F8 `prompt`
   path. Both converge: the editor answers the (possibly dropped) oneshot, consumes the
   queued `Next`, and re-arms `readline` — the `\x1b[?2004h` marker 2.
4. The window between fact 1 and fact 2 — **from the label write to the `tcsetattr`** —
   is cooked mode (rustyline restored the line discipline when readline 1 returned; ISIG
   on, ECHOCTL on). A ^C byte arriving inside that window is processed by the **kernel
   line discipline**: SIGINT raised, byte consumed, nothing queued for the reader.
5. `input_reader`-fed rpassword never configures the terminal and suppresses all
   feedback (`unix.rs:73-86` computes `input_is_tty` for `FilePath` targets only;
   `feedback.rs` gates every `push/pop/abort/finish` output on
   `needs_terminal_configuration`). Any fix that hands rpassword a custom `Read` loses
   the `*` mask, the finish/abort newlines — rejected on that basis (D1).

## 2. Root-cause hypothesis tree (interleavings over the re-arm path)

Framing from the issue ("the race window between the select-cancel acknowledgement and
the editor thread's return to `readline()`") splits into two legs, and the evidence
already separates them:

**Loop leg (select-cancel acknowledgement): healthy in every wedge transcript.** The
cancelled envelope printed — the cancel arm ran: report → ack → `resume()`. The `Next`
verdict sits in the dispositions channel (capacity 1, send never blocks — the editor is
the only consumer). Capacity/ordering analysis of `ask()`/`resume()` shows no loop-side
block: `ask()`'s Prompt send completes when the editor receives it; the editor is inside
the masked read from then on; the channel is empty when `resume()` fires. **The
handshake has no wedge.**

**Editor leg (return to `readline`): the wedge.** The editor thread is stuck before
`verdicts_rx.blocking_recv()` can matter — inside the masked read. Enumerated
interleavings, by the ^C byte's arrival time:

| # | ^C arrival | Path taken | Outcome | Discriminating evidence |
|---|---|---|---|---|
| I-a | after `tcsetattr` (ISIG off) | data → rpassword abort + `raise` → F8 `Err` ∥ epoch → cancel arm | **healthy**: one typed envelope (`prompt` ∥ `cancelled`, Q11), editor answers → `Next` → marker 2 | mask `*`/abort newline may appear; no `^C` echo |
| **I-b** | **label-write … `tcsetattr` (cooked, ISIG on)** | **kernel eats the byte, raises SIGINT** → epoch → cancel arm → `cancelled` envelope + `Next` queued; rpassword then applies its config and **blocks in `read(2)` on a byte that never comes** | **THE WEDGE**: editor parked in `read(2)` on the rpassword tty fd; `readline` 2 never arms; zero pty bytes forever (the test is inside `wait_readline(2)` — it feeds nothing more) | transcript: **literal `^C` (ECHOCTL echo of the intr char, cooked mode only)**, **no mask `*` bytes**, exactly one `cancelled` envelope, then silence; live introspection: `repl-line-reader` thread in `read(2)` |
| I-c | before the label write (ceremony prelude; production-only — the test gates on the marker) | kernel-ISIG as I-b; the read then blocks | same wedge shape, earlier window | same as I-b minus the ordering proof |
| I-d | during rustyline `readline` (raw) | byte → `Interrupted` → `LineEvent::Interrupted` → resume | healthy (F5 line-clear; Case 5 pin) | prompt re-arms |
| I-e | I-a ordering variants (F8-wins vs cancel-wins) | the same keystroke's two typed outcomes race | healthy either way — **this is the "select-cancel race" Q11 accepted**, not a wedge | envelope kind differs, marker 2 always appears |
| I-f | epoch left armed after F8-wins (ack only at next dispatch's stale-drain) | next SIGINT forces at 130 | by design (D-2's escape hatch), not the wedge | n/a |
| I-g | `signals.ack.send(acked_epoch)` sends the stale epoch value | watch `Progress` de-arms on the version bump; the Ladder ignores the value | cosmetic, no functional effect (D8) | n/a |
| I-h | rustyline re-arm fails after `Next` (tty state corrupted) | editor sends `Eof` → session ends | contradicted: the wedge shows a live, silent process, never EOF/exit | pty EOF would surface as `WaitOutcome::Eof`, not `Idle` |

**H-1 (primary): I-b/I-c — the kernel-ISIG consumption window.** Retrospective
confirmation is already in #572's recorded CI transcript (run 41358528084, comment on
the issue): `App password: ^C …ERROR … kind="cancelled"` — the literal `^C` is the
ECHOCTL echo the cooked line discipline emits when it processes the intr character
(I-a's rpassword mode has ECHO/ECHOCTL off and would show `*`/abort feedback instead),
and the envelope kind is `cancelled` — the loop arm fired, meaning the epoch came from
the kernel-delivered SIGINT, meaning rpassword never read the byte. Every discriminator
in I-b's column is present in the recorded evidence. Under load the sub-millisecond
label-write→`tcsetattr` window stretches to scheduler timescales (the editor thread
descheduled between the two syscalls), which is exactly the ~1/25 loaded rate.

**Stage-2 mandate: evidence first, fix second.** The confirmation protocol below must
reproduce the wedge locally and check the full discriminator set before any fix code is
written. If the discriminator set contradicts H-1 (e.g. mask bytes present ⇒ the read
was alive ⇒ a different stall), STOP, record the transcript on #572, and re-plan — no
speculative fixes.

## 3. Reproduction protocol (TDD step 0)

### 3.1 Pre-fix confirmation loop (the #572 recipe, ≥ its sample size)

Load shape (parameterized, per #572): bounded CPU spinners holding sampled load ≈ 2×
nproc, plus one parallel `cargo build --workspace` in a scratch tree. Loop:

```sh
export TASKBOARD_PTY_TEST_GAP_LOG=/tmp/pty-gap-$ITER.log
cargo nextest run -p taskboard-app --test cli_tty \
  tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives
```

Sample size: **≥ 25 iterations** (#572's recorded rate ≈ 1/25; at p = 0.04,
P(≥1 wedge in 25) ≈ 64% — on a miss, run a second block of 25 before concluding
anything; the wedge is load-scheduled, not absent). Capture per-iteration transcripts
from the panic message (the seam attaches them) and the gap logs.

**Confirmation gate**: a wedged run whose transcript satisfies the full I-b
discriminator set — literal `^C` echo after the label; zero mask-feedback bytes; at
most one JSON error envelope and its kind `cancelled`; no bytes of any kind after it;
`TASKBOARD_PTY_TEST_GAP_LOG` showing the last chunk at the envelope. If the wedge
shows but the discriminator set is ambiguous, run the **600 s control** exactly once
(#572's own control): temporary budget raise via a scratch-tree edit **never pushed**
(harness unchanged on the branch), to exclude descheduling — the recorded wedge already
passed this control once; repeat only on ambiguity. Optional corroboration when a wedge
is caught live: manual `script -qec` session driven by hand under the same load,
`eu-stack`/`gdb -p` on the wedged pid, `repl-line-reader` thread expected in `read(2)`
— this is a production-behavior probe, not a harness change.

### 3.2 Post-fix validation (how we will know the fix works)

Two independent legs, both required:

1. **Mechanism-level pin (load-independent, certain)**: the deterministic interleaving
   seam (§4.2/§6) — the wedge's precondition and its cure are pinned at the unit tier
   with injected fds; a sabotage control proves the pin fails typed when the cure is
   reverted (§6.3). This is the gate the loaded loop cannot be: the window is
   kernel-scheduled and cannot be hit deterministically from a pty test.
2. **Loaded-loop pass rate (corroboration at the recipe's own scale)**: the §3.1 loop
   against the fixed tree, **40 iterations, zero wedges** (residual-at-1/25 rejected at
   ≈80% confidence: 0.96⁴⁰ ≈ 0.20), plus the existing cli_tty test green in isolation
   and in the full quality-gate suite through `scripts/run-quality-gates.sh`. Telemetry
   (`TASKBOARD_PTY_TEST_GAP_LOG`) on during validation; any post-fix wedge goes to the
   #524 stay-open rule (§5) — never tuned away.

## 4. The fix shape (post-confirmation; no speculative code before §3.1's gate)

### 4.1 Decision: an in-repo, wake-interruptible masked-read core (unix)

The root cause: **once the masked read is armed, nothing in the process can unblock
it** — a blocked `read(2)` on the tty observes only input. The cure must make the read
observe the cancellation: a second pollable source. Fix shape:

- **New `MaskedRead` core** (new module in `taskboard-app`, e.g.
  `masked_read.rs`, `cfg(unix)`): given (input tty fd, feedback writer, wake fd), it
  (a) saves the termios and applies rpassword's exact config (`ECHO|ICANON|ECHONL`
  off, `ISIG` off, `VMIN=1`, `TCSANOW`); (b) byte-loops under `poll(2)` over
  **{tty fd, wake fd}**; (c) restores the termios on **every** exit path (ok, error,
  wake — the `RawModeInput::drop` parity). `nix` is already a dependency
  (`signals.rs` termios work); no new crates.
- **Grammar parity with rpassword 7.5.4 is a hard requirement** (D2): the byte
  grammar is the vendored `lib.rs:88-191` table — Enter (`\n`/`\r`), backspace/DEL,
  `^U` clear, `^W` clear-til-space, `^C` → restore, **`raise(SIGINT)` parity**
  (the raise is Q11's mechanism; it keeps the `prompt`∥`cancelled` duality alive),
  `Err(Interrupted)`; `^D` empty → `Err(UnexpectedEof)`, non-empty → ignored; ESC →
  consume CSI/SS3 sequences; printable → push + `*` mask feedback (`\x08 \x08` pops,
  finish/abort newline); UTF-8 assembly per vendored `utf8.rs` (lead-byte width,
  continuation reads poll-gated, invalid → U+FFFD); EOF (read 0) → `Ok("")` parity
  (D4). The module docs carry the parity table; the unit tier pins it (§6).
- **The editor's `Masked` arm** (`repl.rs:282-297`) calls the core instead of
  `read_secret_sync` on unix; `cfg(not(unix))` keeps `read_secret_sync` (rpassword)
  verbatim (D6a — Windows has no poll/termios world in this crate today).
- **Loop-side wake trigger**: while a `Prompt` disposition is in flight
  (`ReplPrompts::ask()`, repl.rs:340-353 — set before the send, cleared when the
  answer await settles), the cancel arm (`repl.rs:883-887`) fires the wake if the flag
  is set. Transport: an eventfd or a `pipe2` owned by the editor and polled by the
  core; the loop writes one byte (never blocks; pipe capacity makes a direct
  `nix::unistd::write` safe). **Stale-wake discipline** (D5): the editor drains the
  wake source at prompt start and after each read, so a wake racing an
  already-finishing prompt can never abandon a later one.
- **What wake buys**: in I-b/I-c the kernel consumed the ^C and armed the epoch; the
  loop's cancel arm fires as it does today; the wake reaches the parked editor through
  the poll set; the core restores the termios, answers the (dropped) oneshot with the
  typed interruption, the editor consumes the queued `Next`, and `readline` 2 arms.
  The wedge's precondition — a read nobody can complete — is removed by construction.

### 4.2 Rejected alternatives (recorded, so stage 2 does not rediscover them)

- **Verdict-only abandon (tell the editor via the disposition channel)**: the editor
  is blocked in `read(2)`; it cannot observe any channel. ✗
- **ISIG pre-arm before the label write**: narrows the window to the ceremony prelude
  (still nonzero, still load-stretchable) and is *invisible to every test* (the pty
  tier gates on the marker) — a "fix" whose only evidence is a test that stopped
  failing for a different reason. ✗ (AGENTS §5 no-quality-ceilings spirit.)
- **Loop-side ignore-the-epoch while a prompt is outstanding**: the wedge is
  editor-side read starvation; the loop's behavior is irrelevant to it. ✗
- **Helper thread + abandoned rpassword read**: the abandoned read parks until Enter
  (a thread per cancelled prompt) and its late `RawModeInput::drop` restore stomps
  rustyline's raw mode mid-`readline` (termios tug-of-war). ✗
- **`input_reader` adapter over a poll/wake `Read`**: rpassword suppresses all
  feedback when `needs_terminal_configuration` is false (`unix.rs:73-86`,
  `feedback.rs:25-27`) — the `*` mask, abort and finish newlines silently vanish. ✗
- **`TIOCSTI` byte injection to complete the dead read**: writing to `/dev/tty` is
  the output side; TIOCSTI is restricted on modern kernels (6.2+ gate). ✗

### 4.3 Scope fences

- The **one-shot** ceremony surface keeps `StdinPrompts`/rpassword and the default
  ^C death (Case 7 pin; no handler on that surface, D-3/F14).
- `StdinPrompts::read_line_sync`/`read_confirm_sync` (piped stdin) untouched.
- **No harness changes**: `cli_tty.rs` pins stay verbatim; the existing pty legs
  (masked `*` feedback exercised by `tty_in_session_connect_prompts_masked_and_persists`,
  Case 1's `prompt ∥ cancelled` assertion) double as the parity net for the new core.

## 5. The #524 rider protocol

**Same-mechanism test** — an observation counts as this issue's mechanism iff all
four hold: (a) same scene: `tty_ctrl_c_during_the_masked_read_*`, the marker-gated
bare `\x03` after `App password: ` (`cli_tty.rs:626-628` for `never_persists`;
`:1552-1554` for Case 1); (b) same failure mode: the seam's idle-budget typed panic
with a zero-byte tail (`no pty output for 30s`, or the pre-seam outer-timeout world's
exit-124 shape for the pre-2026-11-01 observations); (c) transcript discrimination:
the I-b column of §2's table (the `^C` echo where the transcript retains it, no mask
feedback, at most one `cancelled` envelope, prompt/readline 2 never arms); (d) load
dependence: passes in isolation.

**Attribution evidence**: the 2026-11-02 (PR #586 coverage run, 30.097 s) and
2026-11-03 (chunk-7 gate run) observations pre-date the fix, share the scene and the
signature with #572's confirmed mechanism, and the sibling tests differ only in their
post-^C wait. Mechanism-level attribution is completed by the §3.2 validation: the
fix removes the wedge's precondition by construction, and 40 loaded iterations plus
the loaded quality-gate cli_tty run produce zero recurrences.

**Close-vs-stay decision rule**:
- *Close #524* (after §3.2 is green): the fix PR + validation numbers are the closing
  comment's evidence; both post-fix observations are attributed to the fixed
  mechanism; closing is the archive.
- *Stay open*: any post-fix loaded run wedges with the same signature → capture
  transcript + gap log on #524, do not close, cross-link #572 (either a branch of the
  tree the fix missed → re-plan, or a second mechanism → new `flaky-test`+`watchlist`
  issue per AGENTS §7).
- *Stay open, new mechanism*: post-fix failures whose transcripts show a **different**
  discriminator set (e.g. mask feedback bytes present ⇒ the read was alive and
  progressing) → distinct signature, new issue, #524's watch ends only when its own
  signature is explained.

## 6. Test catalogue (TDD order; no timing assertions, no harness changes)

### 6.1 Unit tier — the `MaskedRead` core over injected fds (new; deterministic by
construction)

The core's dependencies are parameters: input fd (production `/dev/tty`, test
`socketpair(2)` end), feedback writer (production tty, test `Vec<u8>`), wake fd
(production eventfd/pipe, test the other socketpair end). No sleeps: poll returns are
the synchronization; a script writes bytes/wakes, then the assertion observes the
outcome.

1. Grammar parity table (one test, table-driven over the §4.1 grammar): Enter
   finishes; backspace pops with `\x08 \x08`; `^U`/`^W` clears; `^C` → typed
   `Interrupted` **and** the raise happened (observe via the injected wake… — no: the
   raise is asserted by extracting the core's "abort" action behind the seam, or by
   the pty tier's existing Case 1 `prompt`-kind leg; decide at implementation: the
   unit tier asserts the typed outcome + restore, the raise's *effect* stays pinned by
   Case 1's `prompt ∥ cancelled` acceptance); `^D` empty/non-empty; ESC-CSI discard;
   UTF-8 assembly (multi-byte char one `*`, invalid byte U+FFFD); EOF → `Ok("")`.
2. Termios lifecycle: applied before the first read (observable via a termios probe
   seam or the pty tier), restored on every exit path (ok, error, wake) — a dropped
   restore is a typed unit failure.
3. **The wedge pin (the mechanism-level pin for §3.2 leg 1)**: read armed with no
   input; wake arrives → the read returns typed-interrupted, termios restored, no
   byte consumed. This is exactly I-b's cure, exercised deterministically.
4. Byte-then-wake and wake-drain discipline: a wake landing while the prompt is
   finishing (answered/abandoned) never abandons the *next* read — the stale-wake
   drain (D5) pinned.
5. Wake-vs-byte simultaneity: both poll sources ready — byte wins the grammar, the
   stale wake is drained, outcome typed; no lost/corrupted state.

### 6.2 Session tier — the wake trigger wiring (hermetic)

6. The outstanding-prompt flag: set during `ask()`'s flight, cleared after the answer
   settles (inject the disposition peer, as `disposition_tests` does today).
7. The cancel arm fires the wake iff the flag is set: an injected-cancel session with
   a scripted editor peer observes the wake exactly once, and not at all for a
   non-prompt line cancellation (the stalled-line scene must stay wake-free —
   `tty_ctrl_c_during_a_stalled_line_…`'s world).

### 6.3 Sabotage control (the acceptance mandate)

Before writing the fix's positive tests off as vacuous: commit (scratch, on the
branch, amended away before the PR opens) a sabotage that drops the wake arm from the
cancel path — the §6.1-3 wedge pin must fail typed. Then remove the sabotage. (The
pty-flake-pass precedent: the control proves the test can fail.)

### 6.4 The pty tier — existing pins, unweakened

- `tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives` (#572's
  test): **verbatim** — session survives, one typed envelope, `prompt ∥ cancelled`,
  nothing persisted, `exit` completes 0.
- `tty_ctrl_c_during_the_masked_read_never_persists_and_never_wedges` (#524's test):
  **verbatim** — the survival/wait-for-prompt-2 pin stays.
- All other cli_tty tests: untouched (non-goal). The masked-feedback pty leg doubles
  as the grammar-parity net for the new core.

### 6.5 Handoff gates

`scripts/run-quality-gates.sh` (AGENTS §10 — the flock wrapper; batch 1 has parallel
worktrees on this box): fmt, clippy `-D warnings`, nextest, doc tests, deny, the
three-command coverage sequence with the profraw quarantine, then the local `rm` of
the coverage target dir.

## 7. Recorded decisions

| # | Question | Decision | Why |
|---|---|---|---|
| D1 | Fix shape | In-repo `cfg(unix)` poll+wake masked-read core with byte-parity to rpassword 7.5.4; loop-side outstanding-prompt flag + wake trigger in the cancel arm | The only shape that removes the wedge *by construction* (§4.2 rejects the rest with reasons; `input_reader` adapters lose the feedback grammar) |
| D2 | `raise(SIGINT)` on ^C-as-data | Kept (parity) | Q11's `prompt ∥ cancelled` duality and Case 1's assertion stay meaningful; the raise is D-2's "cancel the dispatching line" for the masked-read world |
| D3 | Harness changes | None | Chunk non-goal; the seam works; the mechanism pin lives in the unit tier |
| D4 | EOF on the masked read | `Ok("")` parity with rpassword | No behavior drift in this chunk; the empty-secret verify failure is the existing typed world |
| D5 | Wake transport + staleness | eventfd ∥ pipe2 (nix, already a dep — implementation's choice); drain at prompt start and after each read | A stale wake racing a finishing prompt must never abandon the next one (§6.1-4 pins it) |
| D6 | Platform scope | New core `cfg(unix)`; `cfg(not(unix))` keeps `read_secret_sync` | No Windows termios/poll world in this crate today; the session watcher is already cfg-gated |
| D6a | One-shot surface | Untouched (`StdinPrompts` + rpassword + default ^C death) | Case 7 pin; no handler on that surface by design |
| D7 | Sample sizes | Pre-fix ≥25 (two blocks on a miss); post-fix 40 + sabotage control + full gates; 600 s control only on discriminator ambiguity | Honest statistics over p ≈ 1/25; budgets stay derived (never tuned) |
| D8 | `signals.ack.send(acked_epoch)` staleness | Not a defect; out of scope | tokio watch `Progress` de-arms on the version bump; the Ladder never reads the value |
| D9 | Docs | ADR 0014 §"Recorded verification" gains the wedge + fix note; ADR 0011's contract unchanged (the fix enforces it) | The handshake table's "woke by the user's input" row is the contract this bug violated |
| D10 | PR shape | One branch, one PR (plan + fix + tests + docs); plan commit lands first | Dispatch protocol: the PR diff is exactly the chunk's artefacts |
| D11 | Post-fix wedge seen in validation | Never tuned away; §5's stay-open rule runs | AGENTS §7 flaky-test policy; #572's own triage section |

## 8. Risks

- **The wedge may not reproduce locally within the sample** (load-scheduled):
  the protocol iterates two 25-blocks before concluding; the retrospective
  discriminator set in #572's CI transcript is already strong evidence, and the fix's
  acceptance is the mechanism pin + zero-recurrence validation, not the reproduction
  alone. If H-1 is disproven by fresh transcripts, STOP per §2's mandate.
- **Grammar drift** against rpassword (UTF-8 edges, `^W` spaces, ESC sequences):
  mitigated by the parity table in the module docs, the table-driven unit tier, and
  the existing masked-feedback pty leg.
- **poll/wake edges** (simultaneity, staleness): each is a named unit pin (§6.1-4/5).
- **The kernel-ISIG window remains** (bytes still get eaten): it becomes harmless —
  the wake completes the cancellation. The pre-arm hardening was deliberately rejected
  (D1/§4.2) as invisible-to-tests narrowing.
- **Parallel-agent disk/lock pressure**: validation loops are single-test nextest
  runs (no coverage target churn); the full gates go through the flock wrapper.

## 9. Estimated diff size

~500-700 lines, one PR: new `masked_read` module ≈ 200-260 (core + parity docs),
`repl.rs`/editor wiring ≈ 40-60, unit/session tests ≈ 250-350, ADR note ≈ 10-20;
`cli_tty.rs` unchanged.

## 10. Stage-2 contract

Stage 2 executes §3.1's gate **before** writing fix code, records the transcripts as
the stage-2 report's evidence, implements §4 with the §6 catalogue in TDD order,
runs §3.2, disposes of #524 per §5 (comment drafted, `gh` post left to the user's
PR-green handoff per AGENTS §8), and hands off at PR open + `gh pr checks` green.
Stage 3 reviews against §2's mechanism map and §7's decisions.
