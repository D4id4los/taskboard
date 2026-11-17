# [Plan] The gate two-mode law — chunk plan

- **Date**: 2026-11-17
- **Kind**: chunk plan — implementation spec for chunk C13 of the
  backlog-reduction round (branch `chore/gate-two-mode-law`). Scope:
  2 scripts + 2 docs; no CI changes; no AGENTS.md change (#904 stays
  separate).
- **Origin (normative)**: issue #923 + its comment thread — in
  particular the 2026-11-17 ~21:20 two-mode corollary — and the OOM
  incident report
  `.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`
  (both incidents + the addendum). Law baseline: `docs/agent_dispatch.org`
  §2's memory-gate bullets (commit 50eaa89d) and the dispatch-plan
  template's shared execution model item 6 (same commit).
- **Status**: READY — all origin facts are landed on `origin/main`
  (verified: `scripts/memory-gate.sh` from a33e9f36, hook wiring
  f5bca081, law text 50eaa89d, doctor floor dc892060).

## 1. Problem statement

`scripts/memory-gate.sh` (the #906/#888 guard) has exactly one mode:
acquire the cross-worktree flock, poll `MemAvailable` against the
floor *while holding the lock*, then `exec` the command — the lock
lives as long as the command. Correct for short/heavy builds (the
pre-commit clippy), but the ~21:20 live contention verified the
failure mode for long self-capped runs: a 2 h `timeout 7200
cargo-mutants --jobs 2` held `memory-gate.lock` for its whole
duration and put every other agent's gated pre-commit clippy on the
box into bounded wait. Holding a lock across hours is an
availability collapse, not a safety measure — the run's own job cap
+ wall-clock cap (+ oomd once #924 lands) are the protection it
needs.

Second hole from the same corollary: the flock-wrapped §10 suite in
`scripts/run-quality-gates.sh` has no memory pre-check of its own —
it serializes but does not floor-check, so it can start a 10–60
minute llvm-cov build on a starved box.

## 2. Design decisions (all open questions resolved)

### D1 — Two modes in `memory-gate.sh`; flag name `--preflight`

Default (hold) mode is **unchanged**: bounded lock acquisition →
floor poll while holding → `exec`. New third flag word:
`scripts/memory-gate.sh --preflight [--] <cmd> [args…]`.

Chosen over the issue thread's alternatives: `--no-hold` (names the
mechanism, not the contract) and `--once` with
acquire-check-release-then-exec semantics (the released lock is
re-losable a microsecond later, so the extra acquire/release churn
buys nothing — the single floor reading is the entire claimed
protection; name the mode for what it does).

### D2 — `--preflight` semantics: one reading, fail closed, no waiting

- Check `MemAvailable` against `MEMORY_GATE_MIN_AVAIL_MIB` **once**,
  before exec.
- **Print the reading** on both outcomes (the corollary's explicit
  requirement: "check … once, print the reading"), e.g.
  `memory-gate: preflight MemAvailable 21503 MiB (floor 8192 MiB) — running`.
- Below floor or unreadable `/proc/meminfo` ⇒ nonzero exit, no exec
  (fail closed, same contract as hold mode and the doctor's disk
  floor). **Refuse, do not wait**: a long self-capped run wants a
  go/no-go now, not a queue — the bounded-wait behavior
  (`MEMORY_GATE_TIMEOUT`/`MEMORY_GATE_POLL` loop) deliberately does
  not apply. The caller re-runs when the box frees up (or the
  dispatcher staggers launches — that is what the resource law's
  stagger is for).
- **No lock is acquired at any point** — fd 9 never opened, so
  nothing can be held across the command's duration by construction.
- `--status` is unchanged (works in both modes; it is a query, not a
  route).
- Arg parsing order: `--preflight` is recognized before the existing
  `--` strip (and before `--status`/`-h`), composable as
  `--preflight -- <cmd>`; an unrecognized leading word still falls
  through to the command path, exactly as today.

### D3 — TMPDIR redirection to a disk-backed path (from the #923
comment thread, in scope)

The thread's first comment is a binding additional requirement: the
host `/tmp` is tmpfs (RAM-backed), so any scratch written there by a
gated command consumes host memory directly. `memory-gate.sh` gains,
in **both** modes, before exec:

- if `TMPDIR` is unset, or set to a path whose canonicalized value is
  `/tmp` (or under it), export `TMPDIR=/var/tmp` and print the
  redirection line; an explicitly chosen non-tmpfs `TMPDIR` is
  honored untouched (operator override stays possible);
- best-effort only: if `/var/tmp` is not writable the script prints
  a warning and continues (the gate must not turn a disk-permission
  quirk into a refusal — the command would fail loudly on its own).

The belt-and-braces fstab `size=` cap on the tmpfs mount is an
operator/host change, not a repo change — filed as a backlog issue
(§7).

### D4 — `run-quality-gates.sh` gains the pre-flight floor check

Before acquiring the flock (both the full-suite path and the
arbitrary-command path; `--status` excluded), the script checks
`MemAvailable` once:

- **Same constant source as `memory-gate.sh`**: the env var
  `MEMORY_GATE_MIN_AVAIL_MIB` (default 8192) — one variable controls
  every memory floor except the doctor's own name (the doctor predates
  this chunk and keeps `WORKTREE_DOCTOR_MIN_AVAIL_MIB`; shell has no
  shared config, so the constants stay duplicated **with
  cross-referencing comments** naming the sibling scripts and the
  evidence, exactly the pattern the doctor already carries). No new
  env var is introduced — an agent tuning one floor must not be
  silently diverged from by a second floor.
- Below floor or unreadable ⇒ refuse with a nonzero exit **before**
  the lock is touched, printing the reading and the floor. Above
  floor ⇒ print the reading and proceed. No wait loop — the gate
  sequence's own bounded-lock wait (`GATES_LOCK_TIMEOUT`) stays as
  is, but a starved box is a refusal, not a queued start.
- `--status` additionally prints the current `MemAvailable` reading
  for parity with `memory-gate.sh --status` (cheap, same awk
  helper; keep the helper duplicated, not sourced — neither script
  may grow a shared-source dependency).
- Comment block cross-references `memory-gate.sh` (two-mode law) and
  the incident report.

### D5 — The two-mode routing rule in `docs/agent_dispatch.org` §2

Amend the existing *“Real builds route through the memory gate”*
bullet (do not grow a third bullet — the routing rule is one law)
with a two-mode clause:

- **Short/heavy builds** (clippy, `cargo check`, one-shot test
  builds — anything real but bounded by its own completion) run
  **gated-hold**: the current default; lock held for the command.
- **Long self-capped runs** (cargo-mutants `--jobs N` under
  `timeout`, miri batches, any run with **both** its own job cap and
  a wall-clock cap) run **gated-preflight only**:
  `memory-gate.sh --preflight` — floor-checked, never holding the
  lock.
- **The law sentence**: holding the gate lock across a multi-hour
  run is an availability collapse (every other agent's gated
  pre-commit clippy queues behind it — observed live 2026-11-17
  ~21:20, dispatcher-verified), not a safety measure; the self-caps
  are the protection.
- Evidence pointers: the #923 corollary and the mutants stage-2
  report (`.artifacts/reports/2026-11-17-cargo-mutants-taskboard-app-report.md`
  §4) alongside the existing OOM-report pointer.

### D6 — The dispatch-plan template item 6 amendment

Item 6 (*Resource law*) gains the same two-mode routing sentence in
compressed form (the template carries law summaries; §2 stays the
full law): builds and the hook's clippy step route
gated-hold; self-capped long runs (mutants/miri-style, own job cap +
wall-clock cap) route `--preflight` only; never hold the gate lock
across a multi-hour run.

### D7 — Explicit non-goals

- **No AGENTS.md change** — #904 owns the §8/§10 wording pass.
- **No CI changes** — both scripts are local-only by design.
- **No pre-commit hook change** — clippy is short/heavy; the
  default hold mode already routes it correctly (f5bca081 stands).
- **No shared-source refactor** of the floor/`avail_mib` helpers
  across the three scripts (shell surface stays flat; #907's
  shellcheck gate is the future quality home).
- **No lock unification** — `memory-gate.lock` and
  `quality-gates.lock` stay distinct (existing rationale stands).

## 3. Implementation sketch (stage 2 follows this, file by file)

1. `scripts/memory-gate.sh` — usage text + header: add the
   `--preflight` line and the two-mode routing note (short/heavy ⇒
   hold, long self-capped ⇒ preflight, cross-ref §2); add the D3
   TMPDIR block; add the `--preflight` branch implementing D2
   (~15 lines: strip optional `--`, read floor, `avail_mib`, print,
   exit/exec). Hold-mode path also calls the TMPDIR block. Estimated
   +~40/-~5 lines.
2. `scripts/run-quality-gates.sh` — header comment gains the
   two-mode cross-reference; new pre-flight block before `exec 9>>`
   in both paths (function `preflight_mem_floor()` ~15 lines, called
   once — `--status` gets the reading added instead). Estimated
   +~30/-~3 lines.
3. `docs/agent_dispatch.org` §2 — amend the memory-gate bullet per
   D5. Estimated +~14/-~4 lines.
4. `.artifacts/templates/dispatch-plan.md` item 6 — D6. Estimated
   +~5/-~2 lines.

Total estimated diff: ~90–100 changed lines across 4 files.

## 4. Gates & evidence (stage 2 contract)

- `bash -n scripts/memory-gate.sh scripts/run-quality-gates.sh`
  (docs+scripts stage; the full gate suite runs at stage 3 per the
  dispatch round law).
- Live behavioral evidence (cheap, no real build needed):
  - `scripts/memory-gate.sh --preflight -- sleep 1` prints a reading
    and execs; `MEMORY_GATE_MIN_AVAIL_MIB=999999
    scripts/memory-gate.sh --preflight -- true` refuses nonzero;
    during the refuse-probe, `--status` shows the lock FREE
    (nothing acquired).
  - hold mode unchanged: `scripts/memory-gate.sh -- true` still
    acquires (observable via `--status` HELD during a
    `sleep`-wrapped probe in a second shell — optional).
  - `scripts/run-quality-gates.sh --status` prints the reading;
    `MEMORY_GATE_MIN_AVAIL_MIB=999999 scripts/run-quality-gates.sh
    --status` still reports (status never refuses) while a bare
    `true`-command invocation refuses nonzero below floor.
- Commit landed work before any wait ([D] discipline).

## 5. Risks & mitigations

- **Preflight TOCTOU**: the floor can be eaten between the reading
  and the command's build phase — inherent to a pre-check; the
  residual risk is exactly what the resource law's stagger/cap
  (§2) and #924's oomd cover. Documented in the script header, not
  "solved" in code.
- **TMPDIR redirection surprise**: a command persisting temp paths
  into artifacts would embed `/var/tmp/...`. Assessment: cargo,
  nextest, mutants and the hook toolchain all treat TMPDIR as
  scratch; risk accepted, override honored (pre-set non-/tmp TMPDIR
  untouched, D3).
- **One env var, two scripts**: tuning `MEMORY_GATE_MIN_AVAIL_MIB`
  now also moves the §10-suite floor — intended (one budget, one
  dial) but called out in both headers so the coupling is visible.
- **Flag-word collision**: a command legitimately named `--preflight`
  is unreachable — accepted (same exposure as `--status` today).

## 6. Out-of-scope discoveries → backlog issues

- **tmpfs size cap** (fstab `size=` on `/tmp`, the comment's
  belt-and-braces half) — operator/host change, new issue.
- Shared build cache: already filed as #890 (no new issue).
- oomd/earlyoom: already filed as #924 (no new issue).

## 7. Dispatch notes

Stage 2 prompt restates the [D]/[L] round laws verbatim, names
D1–D7 as binding, and gates on `bash -n` + the D-evidence probes
above (no `cargo` needed — the plan commit's own pre-commit hook
already exercises the hold-mode gate end to end). Stage 3 re-runs
the probes and reads the two-mode law for consistency with §2 and
item 6 before opening the PR.
