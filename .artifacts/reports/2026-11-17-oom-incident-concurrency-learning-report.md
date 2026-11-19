# Report — 2026-11-17 OOM incident & dispatch concurrency learning

- **Kind**: incident report + process proposal (dispatcher, backlog
  reduction round "hygiene r2").
- **Incident**: 2026-11-17 ~10:48, global OOM on the 30G dev box
  (kernel: `oom-kill: CONSTRAINT_NONE ... global_oom`; a `zcode`
  process was killed; drkonqi-coredump-processor killed minutes
  earlier). All eight in-flight dispatcher jobs (seven stage-1 agents
  + the dispatcher's own §10 suite) were interrupted.

## 1. Root cause (evidence-backed)

The flock in `scripts/run-quality-gates.sh` serializes *full §10
suites* between agents. It does **not** cover:

1. **Pre-commit hooks** — every agent commit runs
   `cargo clippy --workspace --all-targets -D warnings` +
   fmt-check + slint lint *outside* the flock. In a fresh worktree
   this is a cold full-workspace build (observed 14–30G of target/
   artifacts per worktree; a clippy build's memory peak is several
   GB).
2. **Agents' ad-hoc builds** (compile checks, test binaries) — also
   unflocked.
3. **The dispatcher's own heavy jobs** — the §10 suite's llvm-cov
   build is the single heaviest job on the box, and the dispatcher
   launched it *concurrently with seven agents*.

At the incident moment: 1 llvm-cov build + 7 concurrent cold clippy
builds (one per fresh worktree) on a 30G machine with browsers and
the ZCode desktop itself resident → global OOM. The 1-minute and
5-minute load averages peaked at ~19.6.

Contributing factor: each worktree has an isolated `target/`, so
seven agents = seven independent cold builds with zero shared
incremental cache. The flock never had a chance — none of the
memory-heavy work went through it.

## 2. What already worked

- **Disk hygiene held**: `worktree-doctor.sh` passed (53G free) and
  the target/ sprawl it watches is a separate (real, but non-crashing)
  problem.
- **Resume-over-redo held**: all seven killed stage-1 agents had
  already landed their plan commits (commit-before-wait discipline);
  the dispatcher verified landed state in each worktree and resumed
  at stage 2. Zero work lost; the OOM cost minutes, not hours —
  exactly the property the long-command discipline is designed for.

## 3. Proposed learnings

### L1 — Generalize the flock into a resource gate (new script)

The flock's true job is "one memory-heavy job at a time". Extend it
to a second, cheap wrapper — `scripts/memory-gate.sh <floor-mib> --
<cmd>`:

- takes an exclusive flock (a *build* lock, distinct from or shared
  with the gates lock — decision for the implementing chunk),
- checks `MemAvailable` against the floor (evidence-based floor:
  the box needs ~8G headroom for a clippy build peak + desktop;
  tune at implementation),
- refuses/defers below the floor instead of starting a build that
  OOMs the box.

Route through it: the pre-commit hook's clippy step, and any agent
build expected to exceed ~1 min. The full §10 suite keeps
`run-quality-gates.sh` (which can then share the same lock family).
This turns "7 unserialized cold builds" into "7 serialized ones" —
slow but structurally OOM-proof, and the flock-deferral rule (#879)
already defines the polite behavior for a deferring agent.

### L2 — A binding agent concurrency cap for committing work

Decision point the operator asked about explicitly. Recommendation:
**cap concurrent work-performing agents at 3 per box**, with:

- **staggered starts** (≥3 min between launches) so cold-build peaks
  don't align;
- **the dispatcher's §10 suite runs only in a free window** — zero
  agents mid-build (the suite is the heaviest single job; treat it
  like an agent slot of its own);
- stage-1 docs-only stages still count toward the cap (their commits
  trigger hook builds — the incident's main memory source).

2–3 staggered agents is operator-validated practice on this box
(2026-11-17); the cap formalizes it. With L1 in place the cap
becomes a throughput guard rather than an OOM defense, but keep both:
the floor protects memory, the cap protects latency and the kill
cutoffs.

### L3 — worktree-doctor gains a memory floor

The doctor already gates worktree *creation* on a disk floor
(25G). Give it the same check for `MemAvailable` (floor ~8G): a
worktree created into a memory-starved box dies at its first
pre-commit hook anyway. One-line change family, same script, same
exit contract.

### L4 (backlog, not law) — attack the cold-build cost itself

Seven isolated `target/` dirs mean every worktree cold-builds the
workspace. A shared `CARGO_TARGET_DIR` per-worktree-hash or `sccache`
would cut both disk (the 36–55G/worktree figure) and the memory/
duration of cold clippy. This changes build semantics (cache
poisoning, concurrent-access behavior) — an exploration issue, not a
round chunk.

## 4. Where each learning lives (proposal)

| Learning | Home |
|---|---|
| L1 memory gate | new `scripts/memory-gate.sh` + pre-commit hook wiring |
| L2 agent cap + stagger + suite-free-window | `docs/agent_dispatch.org` (§2 or §6) + the dispatch-plan template's shared execution model |
| L3 doctor memory floor | `scripts/worktree-doctor.sh` |
| L4 shared build cache | `backlog:` issue (dx/performance), exploration |

L1/L3 are scripts (testable, ~small); L2 is law text (docs-only).
Together they are one small follow-up chunk after this round's wave —
not folded into C1 (whose scope is #879's wording, already landed).

## 5. Interim rule (binding on this round, effective immediately)

Until L1/L2 are codified, this dispatcher runs: **≤3 concurrent
agents, staggered ≥3 min, dispatcher §10 suite only in a free
window.** This is what the resumed wave already operates under.

---

## Addendum — second incident, 2026-11-17 ~17:31 (post-guard): the freeze, not the kill

- **Shape**: three agents (C7 mutants, C9.1 planning, an external-session
  GUI-review agent) → the box *froze* (kwin pageflip timeouts, "system
  too slow") and was hard-reset. **No kernel OOM-kill messages exist**
  for that boot — the journal ends mid-hang at 17:31:58.
- **Root cause 1 — the mutants run ignored its job cap**: the C7
  mutants debug log opens with `starting jobserver n_tasks=16` — the
  stage-2 agent launched cargo-mutants at **16 concurrent jobs**,
  violating the prompt's explicit `-j 2-4` direction. Each job runs a
  full `cargo nextest run --package=taskboard-app` (build + test phase
  with daemons/PTY/wiremock children). 16 concurrent nextest cycles is
  the OOM by itself.
- **Root cause 2 — the memory gate's coverage hole (by design)**:
  `memory-gate.sh` guards the pre-commit clippy step only. The mutants
  loop, agents' ad-hoc builds, and the flock-wrapped nextest suite are
  *not* routed through it; and the gate is a pre-check, not a monitor —
  it cannot see an ungated consumer that starts after a gated build
  passed its floor check.
- **Root cause 3 — freeze-instead-of-kill**: `systemd-oomd` and
  `earlyoom` are both **inactive** on this box; swappiness 60 with 31G
  swap. Under sudden multi-build pressure the box swap-thrashes into a
  hard hang *before* the kernel OOM killer fires — so even a correct
  guard can be defeated after the fact, and the OOM evidence is lost
  with the unflushed journal.
- **Remedies filed**: #917 (route every heavy tool invocation —
  cargo-mutants, miri, ad-hoc cargo builds — through memory-gate.sh +
  law text; pre-flight floor check in run-quality-gates.sh), #918
  (operator decision: enable systemd-oomd or earlyoom so the box kills
  the offender instead of freezing). Interim dispatcher rule: mutants
  runs re-launch with explicit `--jobs 2` AND through the gate; no
  other agent in flight while a mutants run is active.
