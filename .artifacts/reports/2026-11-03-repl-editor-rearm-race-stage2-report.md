# The REPL editor re-arm race — chunk 1.1 stage-2 report (implementation)

**Branch**: `fix/repl-editor-rearm-race` (worktree
`../taskboard-WT-fix-repl-editor-rearm-race`) · **Plan**:
`.artifacts/plans/2026-11-03-repl-editor-rearm-race-plan.md` · **Issues**:
#572 (p1), rider #524 · **Date**: 2026-11-03

## 1. Root cause (confirmed with fresh evidence — H-1, plan §2 I-b/I-c)

`rpassword`'s masked read blocks in `read(2)` on the tty and observes only
input. A `^C` byte arriving in the window between the prompt label's write
and the `tcsetattr` (the tty still cooked, `ISIG` on) is consumed by the
kernel line discipline: the byte raises SIGINT (arming the session
watcher's cancel epoch — the loop prints the typed `cancelled` envelope and
queues `Next`) but leaves nothing for the reader. rpassword then applies its
config and parks in `read(2)` forever; the editor thread never returns to
`readline`, and the pty goes silent. Under load the sub-millisecond window
stretches to scheduler timescales — the ~1/25 loaded rate.

The loop-side handshake (select-cancel ack → `resume`) is healthy in every
transcript; the wedge is purely editor-side read starvation.

### Reproduction (plan §3.1, executed before any fix code)

The #572 recipe: bounded spinners at ≈2× nproc (32 on the 16-core box) plus
one parallel `cargo build --workspace` in a scratch tree; loop
`cargo nextest run -p taskboard-app --test cli_tty
tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives` with
`TASKBOARD_PTY_TEST_GAP_LOG` on. **Result: 2 wedges in 25 iterations**
(iterations 7 and 16), consistent with #572's ≈1/25. Both transcripts carry
the complete I-b discriminator set:

- literal `^C` echo directly after the label — the ECHOCTL echo only the
  *cooked* line discipline emits, proving the byte never reached the raw
  reader (`App password: ^C2026-11-03T18:57:59.318261Z ERROR
  taskboard_app::commands: command failed kind="cancelled"`);
- zero mask-feedback bytes;
- exactly one JSON error envelope, kind `cancelled`;
- no bytes of any kind after it — 30 s of silence until the progress-deadline
  seam aborts the test (`no pty output for 30s (progress budget)`, panic at
  `cli_tty.rs:243`, the seam working as designed);
- both failures at ~30.3 s of run time, both passes in isolation (~0.5 s) on
  the same tree — load dependence per the #524 test leg (d).

The 600 s budget control was not needed: the discriminator set is unambiguous
and #572 already carries one 600 s control pass.

## 2. The fix (plan §4.1/D1)

New `crates/taskboard-app/src/masked_read.rs` (unix; `pub` module, the
wake types are shared with `repl::EditorIo`): a wake-interruptible masked
read with **byte-grammar parity to rpassword 7.5.4** — the parity table is
the module doc, pinned by a table-driven unit tier over real openpty
pseudoterminals (no sleeps; the tests observe the raw-mode arming via a
termios poll and only then feed, because bytes fed while the tty is still
canonical are processed by the kernel line discipline — the very wedge
mechanism — which ate the first draft's control bytes).

- `read_masked` core: saves termios, applies rpassword's exact config
  (`ECHO|ICANON|ECHONL|ISIG` off, `VMIN=1`, `TCSANOW`), byte-loops under
  `poll(2)` over {tty fd, wake fd}, restores termios on **every** exit path
  via a drop guard (`RawModeInput::drop` parity).
- Wake transport: a nonblocking `UnixStream` pair (std-only — a deviation
  from plan D5's "eventfd ∥ pipe2 (nix)": the socket pair avoids new nix
  features and is the same pollable-byte-fd shape).
- `read_secret_tty`: the production wrapper — stale-wake drain at prompt
  start (D5), label on `/dev/tty`, the core, trim; the piped-stdin fallback
  delegates to `StdinPrompts::read_secret_sync` verbatim.
- Editor wiring: `spawn_line_reader` builds the channel; the `Masked` arm
  calls the core on unix, keeps `read_secret_sync` verbatim on
  `cfg(not(unix))` (D6a: the one-shot surface untouched, D6: Windows keeps
  rpassword).
- Loop wiring: `ReplPrompts` gains a prompt-in-flight flag (set before the
  send, cleared when the answer settles; a ceremony future dropped
  mid-await conservatively stays set — a prompt may still be armed on the
  editor, and a stale wake is drained at the next prompt start) and
  `wake_if_prompt()`; the cancel arm fires it before reporting (repl.rs).
  SIGINT raise parity kept (D2): the core re-raises on `^C`-as-data, so
  Q11's `prompt ∥ cancelled` duality survives — this is why nix's `signal`
  feature moved from dev-only to the main dependency (recorded deviation
  from the dev-deps comment; no safe raise API exists and the crate forbids
  unsafe).
- `signal-hook` joins as a **dev-only** dependency: the `^C` unit pin
  observes the raise through a safe handler (the crate's
  `#![forbid(unsafe_code)]` rules out raw `sigaction`); the raise's
  end-to-end effect stays pinned by cli_tty Case 1's `prompt ∥ cancelled`
  leg, exactly the plan §6.1-1's fallback option.

## 3. Test evidence

- **Unit tier** (`masked_read::tests`, 9 tests, over openpty + injected
  fds): the grammar table (Enter LF/CR, backspace/DEL incl. empty, `^U`,
  `^W`, discarded controls, `^D` empty/non-empty, ESC-CSI/SS3 — note the
  vendored SS3 final-byte consumption is pinned as-is, `é`/`🚲`/invalid-byte
  U+FFFD), `^C` typed-interrupted + raise observed, **the wedge pin**
  (armed read + wake → typed `Interrupted`, nothing consumed, termios
  restored), stale-wake drain never abandons the next read, byte-then-wake
  ordering, wake mid-character, feedback-state parity, cloneable sink.
  One EOF-parity row (read 0 → `Ok(password so far)`) is implemented but
  has no unit pin: a pty cannot produce a clean EOF on the slave (master
  close gives EIO), and a socketpair is not a tty so the raw-mode core
  cannot arm — recorded as a known pin gap in the parity table.
- **Session tier** (`disposition_tests`, 2 tests): the prompt-in-flight
  flag (set-before-send observed by the answering peer via the
  happens-before of the channel; cleared on settle) and the wake trigger
  (no wake with nothing in flight — the stalled-line scene stays
  wake-free; exactly one fire per arm while a prompt is in flight; none
  after the answer settles).
- **Sabotage control** (plan §6.3): the wake arm sabotaged to
  swallow-and-keep-polling → the wedge pin **fails typed**
  (`assertion failed: matches!(result, Err(err) if …Interrupted)`),
  reverted, pin green again. A mid-edit incident left the sabotage's
  control-flow remnant after the revert and silently kept swallowing the
  wake (the pin hung instead of passing) — caught by re-running the pin,
  restored to the exact pre-sabotage block, whole suite re-verified.
- **Pty tier unchanged**: `cli_tty.rs` untouched (harness non-goal); all
  18 tests green including #572's and #524's session-survives pins
  verbatim; `--test repl` 32/32 green (the 8 injected-`EditorIo`
  literals gained the `#[cfg(unix)] wake: None` field, no behavior
  change); lib 432/432 green.

## 4. Validation (plan §3.2)

1. Mechanism pin: the wedge pin + sabotage control (above) —
   load-independent.
2. Loaded loop: the §3.1 recipe against the fixed tree (commit
   `a112cb94`), **40 iterations, zero wedges** — every iteration PASS,
   no `progress budget` panic, gap logs on (`/tmp/pty-gap-post-*.log`,
   `/tmp/rearm-postfix-results.txt` in the session's tmp; the first
   ~20 iterations ran under the full recipe load including the
   12-minute parallel scratch-tree build, the box additionally
   carrying chunk 1.3's coverage suite). Post-fix 0/40 vs pre-fix
   2/25 under the same recipe; residual-at-1/25 rejected at ≈80%
   confidence, per plan D7's statistics.

## 5. Deviations from the plan (each with a reason)

| # | Deviation | Why |
|---|---|---|
| 1 | Wake transport is a std `UnixStream` pair, not nix eventfd/pipe2 (D5's letter) | Same pollable-byte-fd shape, no new nix features on the release profile; the discipline (drain at prompt start + after each read) is exactly D5 |
| 2 | nix `signal` (+`fs`,`poll`) moved to the main dependency | The `^C` raise parity (D2) is production code; previously signal was dev-only. No safe raise exists without it |
| 3 | `signal-hook` added (workspace + app dev-deps) | The `^C` unit pin observes the raise through a safe handler; `#![forbid(unsafe_code)]` rules out `sigaction`. The plan's §6.1-1 option (the raise's effect stays pinned by Case 1's pty leg) is honored in addition |
| 4 | §6.2-7's "injected-cancel session observes the wake" pin lives at the `ReplPrompts` tier, not driving `repl_loop` | `repl_loop` has no hermetic harness (it needs the bootstrapped actor `App`); the routing call-site is one line, covered end-to-end by the unchanged pty pins and the loaded loop |
| 5 | Unit tests feed the pty only *after* observing the raw-mode arming (termios poll), not before | Bytes fed while the tty is canonical are eaten by the kernel line discipline — the wedge mechanism itself; pre-feeding pins the kernel, not the core |
| 6 | EOF-parity row (`Ok(password)` on read 0) has no unit pin | A pty cannot deliver a clean slave EOF (EIO on master close); noted in the module's parity table |
| 7 | §6.1-1's "raise happened" assertion uses signal-hook's flag, not a core "abort action seam" | The plan explicitly allowed this; the seam alternative would add a cfg-test split through the core |

## 6. #524 rider disposition (plan §5)

Both post-fix observations (2026-11-02 PR #586 coverage run, 30.097 s;
2026-11-03 chunk-7 gate run) satisfy same-mechanism legs (a), (b) and (d)
verbatim: same scene (`tty_ctrl_c_during_the_masked_read_*`, marker-gated
bare `\x03` after `App password: `), same failure mode (the seam's
`no pty output for 30s (progress budget)` typed panic with a zero-byte
tail), passes in isolation. Leg (c) — transcript discrimination — matches
the I-b set on every retained artifact, and the pre-fix local reproduction
(§1) produced two fresh transcripts with the identical signature. The
mechanism-level attribution completes with §4's zero-recurrence
validation. **Disposition: close #524** with this chunk's PR + validation
numbers as the closing evidence, once the PR is green (the `gh` post is
left to the user's merge handoff per AGENTS §8; per this chunk's
instructions nothing is pushed or posted by stage 2). Stay-open rule
armed: any post-fix wedge with this signature goes to #524 with transcript
+ gap log, never tuned away.
