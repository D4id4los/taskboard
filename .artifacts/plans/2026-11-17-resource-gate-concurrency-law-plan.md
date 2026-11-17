# Plan — the resource gate and the concurrency law (issues #887, #888, #889)

- **Date**: 2026-11-17
- **Origin**: the 2026-11-17 OOM incident report
  `.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`
  (L1 → #887, L2 → #888, L3 → #889; L4 is #890 and **out of scope**).
  Operator approved this follow-up chunk 2026-11-17.
- **Scope**: two scripts (`memory-gate.sh` new, `worktree-doctor.sh`
  extended), one hook wiring change (`.githooks/pre-commit`), two docs
  amendments (`docs/agent_dispatch.org`, the dispatch-plan template).
  **No CI changes.**
- **Branch**: `chore/resource-gate-concurrency-law`, one PR to `main`.

## 0. Evidence base (measured 2026-11-17 on the dev box)

- Box: 31,478 MiB total RAM, 16 cores, ~31.7G swap; `MemAvailable`
  right now 19,951 MiB (desktop + browsers resident).
- Incident (kernel log, 10:37 & 10:46 UTC):
  `oom-kill:constraint=CONSTRAINT_NONE ... global_oom` — global
  OOM with 1 llvm-cov build + 7 concurrent cold per-worktree clippy
  builds; load peaks ~19.6; a `zcode` process and chrome were killed.
  30G of box ÷ 8 concurrent memory-heavy jobs ⇒ average peak
  ≥ ~3.5–4G per build job, and the llvm-cov build (heaviest single
  job) goes above that. `MemAvailable` was driven below the kernel's
  emergency threshold with the desktop still resident.
- `scripts/run-quality-gates.sh` flock proven good at its one job
  (full-suite serialization) and structurally blind to everything else
  (hook builds, ad-hoc builds) — the incident report §1 root cause.
- The flock-deferral rule (#879, `docs/agent_dispatch.org` §2 last
  bullet + template item 3) already defines polite behavior for an
  agent that cannot get a contended lock: commit, report, end stage.

## 1. Decisions

### D1 — `scripts/memory-gate.sh` (new script, #887)

**Usage**: `scripts/memory-gate.sh [--] <cmd> [args…]`
(env knobs mirror the gates wrapper: `MEMORY_GATE_TIMEOUT`, default
3600 s, 0 = forever; `MEMORY_GATE_POLL`, default 30 s;
`MEMORY_GATE_MIN_AVAIL_MIB`, default 8192; `--status` reports lock
state and current `MemAvailable`).

**D1a — a second, distinct lock file** (decision; the incident report
left this open). `memory-gate.lock` lives in the same
`git rev-parse --git-common-dir` directory as `quality-gates.lock`,
same mechanism (flock on an appended fd, holder info written into the
file, kernel-released on OOM kill), but **not shared** with the gates
lock.

Rationale: sharing would serialize a minutes-long pre-commit clippy
behind the 10–60-minute full suite — every commit would wait for
whoever holds the gates lock, an unacceptable latency tax for exactly
zero added safety, because the suite-vs-build contention the incident
actually saw is already excluded by law (D2's suite-free-window rule).
The gate lock serializes *build-vs-build* peaks (the 7-cold-clippy
shape); the gates lock serializes *suite-vs-suite*; and the law keeps
*suite-vs-build* windows apart. Three pairwise protections, one
incident shape each. The residual overlap (suite + one gated build)
is bounded at 2 heavy jobs — the box survives 2 (it died at 8).

**D1b — floor 8192 MiB, self-validating at implementation.** The
default is the incident report's ~8G headroom estimate. Stage 2
**measures before wiring the hook**: in this chunk's fresh worktree,
run a background sampler
(`while :; do grep MemAvailable /proc/meminfo; sleep 2; done`)
during a cold `cargo clippy --workspace --all-targets -- -D warnings`
and record the observed minimum in the implementation report. The
floor must be ≥ observed clippy minimum + ~2G desktop headroom; if
measurement contradicts 8192, adjust the default and record the
evidence. Both the gate and the doctor default from the same constant
(duplicated literal in two scripts is accepted — shell, no shared
config; a comment cross-references them).

**D1c — wait, then refuse (bounded).** Acquire the flock blocking
(with timeout + periodic "waiting, held by …" messages, mirroring
`run-quality-gates.sh`); once the lock is held, poll `MemAvailable`
every `MEMORY_GATE_POLL` and start the command only when it is at or
above the floor; on timeout, exit nonzero with the floor value and
the current reading. Refusing outright would break the pre-commit
hook for a condition that clears in minutes; waiting unbounded would
violate #879's spirit — hence bounded wait, and the long-command
discipline (commit before wait, background + poll) already tells
agents how to wait on a hook safely. A below-floor reading while
*holding* the lock is still checked: the lock serializes the peaks,
the floor checks the box's remaining budget.

**D1d — call sites.**
1. `.githooks/pre-commit`: the clippy step becomes
   `scripts/memory-gate.sh -- cargo clippy --workspace --all-targets
   --quiet -- -D warnings`. `cargo fmt` and the slint token lint stay
   ungated (instant, memory-trivial).
2. Agent usage documented in `docs/agent_dispatch.org` (§2 new
   bullets) and the script header: any agent command expected to be a
   real build (>~1 min, e.g. `cargo build`, `cargo test` subsets,
   clippy re-runs) routes through `scripts/memory-gate.sh`. The full
   §10 suite keeps `run-quality-gates.sh` (unchanged this chunk).
3. CI untouched: one suite per runner, no gate needed.

### D2 — the concurrency law (#888)

`docs/agent_dispatch.org` §2 gains one bullet (house style, named,
with the incident as provenance):

> - *Resource law (the concurrency cap)*: at most **3 concurrent
>   work-performing agents per box**, with **staggered starts ≥3
>   minutes apart** (cold pre-commit build peaks must not align), and
>   the **dispatcher's full §10 suite runs only in a free window** —
>   zero agents mid-build; the llvm-cov build is the heaviest single
>   job on the box and counts as an agent slot of its own.
>   Docs-only stages count toward the cap (their commits trigger hook
>   builds — the incident's main memory source). The memory gate
>   (D1) protects memory; this cap protects latency and the
>   inactivity cutoffs. Evidence: the 2026-11-17 OOM
>   (`.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`);
>   2–3 staggered agents is operator-validated practice on this box.

Mirrored as **item 6** in the template's *Shared execution model*
block, restated-per-prompt style, so every new dispatch plan inherits
it (the #879 precedent: items 1–5 live there now).

The interim rule in the incident report §5 is thereby superseded by
codified law; a one-line note in the new bullet says so.

### D3 — `scripts/worktree-doctor.sh` memory floor (#889)

Mirror the disk floor exactly:

- Env: `WORKTREE_DOCTOR_MIN_AVAIL_MIB`, default **8192** (same
  constant as D1b).
- Read `MemAvailable` from `/proc/meminfo` (Linux-only box family;
  the doctor is already a local-hygiene tool). Unmeasurable counts as
  **below** the floor — same fail-closed contract as the disk check.
- Placement: after the disk floor check, before exit; output line
  `avail: 19G (floor: 8G)`; exit contract identical (nonzero ⇒ the
  caller stops instead of creating a worktree; AGENTS §8 wording
  already covers it as "free disk is below the floor" — the plan
  updates the AGENTS §8 wording? **No** — AGENTS.md edits are out of
  scope; the doctor's own header documents the new floor and the
  dispatch-prompt convention "nonzero = STOP" is unchanged in
  meaning).
- Header comment gains a 4th numbered behavior + pointer to the
  incident report.

## 2. Implementation steps (stage 2)

1. Measure: background sampler + cold clippy in this worktree;
   record the minimum; fix the floor constant if evidence demands.
2. Write `scripts/memory-gate.sh` (header docs, `--status`, flock +
   floor poll, `exec "$@"` keeping fd open — copy the proven
   mechanics from `run-quality-gates.sh`).
3. Wire `.githooks/pre-commit` (clippy step only).
4. Extend `scripts/worktree-doctor.sh` (D3).
5. Amend `docs/agent_dispatch.org` §2 (D2 bullet) and
   `.artifacts/templates/dispatch-plan.md` shared-execution-model
   item 6.
6. Shell-lint both scripts (`bash -n` minimum; the repo has no bats
   harness — behavior is exercised by using the gate for this chunk's
   own commits, which every subsequent commit does via the hook).
7. Gates: pre-commit hooks (now gate-gated — the chunk dogfoods its
   own gate) + `scripts/run-quality-gates.sh cargo nextest run
   --workspace` (background + poll; defer per #879 if contended).
   No test-suite changes needed (shell scripts aren't nextest-covered;
   CI runs the same scripts, not tests of them).

## 3. Deliberately out of scope

- **#890 (L4)** shared build cache / sccache — separate exploration.
- **AGENTS.md** wording updates (§8 "free disk below the floor",
  §10 pointer to the memory gate) — backlog issue filed; docs-law
  chunk touching AGENTS.md deserves its own review, not a rider here.
- CI workflow changes — none (one suite per runner; the gate is a
  local multi-agent tool by construction).
- Unit tests for the shell scripts (no shell test harness in-repo;
  dogfooding + `bash -n` is the accepted bar this chunk sets).

## 4. Risks

- **Floor mis-set**: too high = hooks refuse on a healthy box; too
  low = the gate waves through a killer build. Mitigated by the D1b
  measurement step and the `--status` mode (an agent that hits a
  refusal can report the reading and a human can tune the env).
- **Distinct lock leaves suite + gated build co-running**: bounded at
  2 heavy jobs; excluded in practice by the D2 free-window rule.
  Accepted and documented in D1a.
- **Hook wait vs inactivity cutoff**: a contended gate can hold a
  hook for minutes; the law's background + poll + commit-before-wait
  rules already define safe waiting. The script's periodic messages
  are observable activity for the polling agent.
- **Doctor fails closed on `/proc` absence** (e.g. a future non-Linux
  dev host): same contract as the disk check; acceptable for a
  Linux-first box tool.

## 5. Estimated diff size

~180–220 lines total: `memory-gate.sh` ~90 new; doctor +15; hook ~3;
`agent_dispatch.org` ~25; template ~20; this plan ~250 (artefact).
