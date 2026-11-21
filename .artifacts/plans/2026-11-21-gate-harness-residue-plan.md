# Chunk S5 — gate/harness residue + law — implementation plan

- **Round**: backlog-reduction round 4, chunk S5 (scope report
  `.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`
  §B-S5 — normative; dispatch plan
  `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  §1/§4, prompt S5.1).
- **Branch**: `ci/gate-harness-residue` (this worktree,
  `../taskboard-WT-ci-gate-harness-residue`).
- **Covers**: #1073, #1074, #1076, #1108, #1112, #1097, #1133, #1137, #1110.
- **Acceptance** (dispatch plan S5.1 + the B5 bar):
  1. Each item landed per its body and the decisions below.
  2. **Zero FOUND→MISSING flips on the live plans** — before/after
     sweep of `scripts/ux-coverage.sh <plan> --gate` over every
     committed ux-journey plan; the only verdict changes are
     AMBIGUOUS→FOUND (the #1073 determinism fix) and the re-rowed
     2026-11-14 plans going AMBIGUOUS/MISSING→FOUND/BOUND (the #1108
     fix). Recorded as a table in the stage-2 report.
  3. The gate fixes carry self-tests (the N-A/B5 precedent): the
     ux-coverage `--self-test` suite grows; the #1110 liveness helper
     carries its own self-test.
  4. Diffs confined to: `scripts/`, `.github/workflows/ci.yml`,
     `crates/taskboard-app/src/commands/config.rs` (+ its tests),
     `crates/taskboard-storage-sqlite/src/` (+ tests),
     `crates/taskboard-ui-slint/tests/journeys/`,
     `crates/taskboard-domain/src/persistence.rs` (doc-comment only,
     if needed),
     `.artifacts/plans/2026-11-14-*.md` (the two re-rowed plans),
     `docs/agent_dispatch.org` (the law amendment).
- **Round laws**: (a) typed outcomes — the #1133 rename is
  refactor-only, no error-class change; the #1137 swap keeps the
  domain trait's contract; (d) out-of-scope findings become `backlog:`
  issues immediately — none found during planning (§8).
- **Journey gate**: none — no user-visible capability (gate/script
  internals, test-harness migration, a type rename, an internal actor
  transport; all exempt per `docs/ux_planning.org`).

## 2. Per-item decisions

### 2.1 #1073 — by-name evidence universes are order-nondeterministic — D-1073-a

**Problem**: `find_evidence` emits `find` order; `first_match` /
`adapter_match` / `nom_file`'s by-name fallback credit the first hit —
so `crates/taskboard-domain/src/messages.rs` vs
`crates/taskboard-domain/src/test_support/messages.rs` resolves
FOUND on whichever `find` emits first. Same for `adapter.rs`,
`requests.rs`, `actions.rs`, `*.slint`.

**Decision (D-1073-a)**: make the by-name universes deterministic AND
production-preferring, in one place — `find_evidence` itself:

- Output is piped through a stable two-tier sort: tier 0 = paths that
  are **not** under a test-support location (`*/test_support/*`,
  `*/tests/*`), tier 1 = the rest; within a tier, `LC_ALL=C sort -u`
  (byte order, dedup — `find` can emit duplicates only via symlink
  loops, but `-u` costs nothing). Implementation: emit tier-0 lines
  then tier-1 lines, each sorted — a pure post-filter inside
  `find_evidence`; every consumer (`adapter_match`, `first_match`,
  `nom_file` fallback, `head -n 1` uses) inherits the guarantee
  without per-callsite changes.
- No per-crate splitting of universes (the issue's alternative):
  rejected — it forks the universe plumbing for one property the
  tiered sort delivers; the domain-contract preference falls out of
  the tier rule (`src/messages.rs` outranks
  `test_support/messages.rs`; `journeys.rs`'s harness support files
  under `tests/` rank below crate `src/` files).
- The predicate is structural (`/test_support/` or a `tests/` path
  segment), not a hardcoded crate list — a new crate's fakes get the
  same de-prioritization for free. Test-support files stay *in* the
  universe (a token that only exists there still resolves, verdict
  FOUND citing the test-support file — deliberately lenient, the same
  posture as the #965/#967 lineage; only the *nondeterminism* is
  removed).

**Self-test**: extend `--self-test` with a case whose fixture tree
carries `src/messages.rs` and `src/test_support/messages.rs` with the
token present only in the fake; assert the cited evidence path is the
production file across repeated runs (run the scan 3× — the old bug
was order-luck, the new assertion is stability itself).

### 2.2 #1074 — the `all_rs_files` universe is never consumed — D-1074-a

**Decision (D-1074-a)**: delete it. The step-4 unscoped fallback
scans `test_files` only, by design (#920: ambient hits are
AMBIGUOUS, never a credit — wiring `all_rs_files` into that fallback
would widen the credit surface, the opposite of the #920 doctrine).
No consumer, no wanted semantics → dead scan work removed. Diff:
one `mapfile` line gone.

### 2.3 #1076 — the bound-test grep includes the fixture tree — D-1076-a

**Problem**: the bound-test grep (`resolve_step` case 3,
`grep -rnE ... "$root"`) carries the target/node_modules/.git
`--exclude-dir`s but not the fixture-root exclusion — a bound-test
nomination matching `fn us1_s1_fixture_create` reads BOUND out of
`scripts/fixtures/ux-coverage-fixture/...` when the scan root
contains the fixture tree.

**Decision (D-1076-a)**: mirror `find_evidence`'s evidence-scoped
prune in the grep: when `FX_ROOT` is set (the script-relative fixture
tree exists) **and** the scan root is not inside `FX_ROOT`, add
`--exclude-dir=<FX_ROOT>` to the bound-test grep; when the root is
inside the fixture tree (self-tests 1/5/8 posture: `--root` points
at the fixture itself) the exclusion is skipped exactly as today —
the evidence-scoped skip-when-root-is-fixture behavior is preserved,
not blanket. The exclusion is the full fixture-root path
(`--exclude-dir` matches basename per level, so the value passed is
the fixture root's basename under `scripts/fixtures/` —
`ux-coverage-fixture` — which cannot collide with any real source
directory name).

**Self-test**: extend `--self-test` with a case running `--root` on a
temp repo containing a copy of `scripts/fixtures` plus a plan row
whose bound-test nomination matches `us1_s1_fixture_create`; assert
the row resolves MISSING (not BOUND) while self-tests 1/5/8 keep
passing.

### 2.4 #1108 — the 2026-11-14 plans resolve AMBIGUOUS/MISSING post-R4 — D-1108-a

**Problem**: the two retro plans
(`2026-11-14-settings-connections-ux-plan.md`,
`2026-11-14-task-lifecycle-ux-plan.md`) carry bare `us1_s1`-class
journey-test tokens that post-R4 consolidation multiplied across
`journeys.rs`, `journeys/habits_journey.rs`,
`journeys/detail_shortcuts_journeys.rs`, … (AMBIGUOUS), and "cites"
rows (`task.create cites US-1`-class) that have no resolvable
nomination (MISSING). The newer plans' convention is fully qualified
tokens + `bound-test:` nominations; the resolver already implements
that grammar (exact-name > file-scoped > bound-test > unscoped).

**Decision (D-1108-a)**: re-row the two plans per the newer
convention — the bound-test mapping applied to the retro cohort:

1. For every journey-step row whose journey-test token is now
   ambiguous: add a `bound-test: <step-id> <fully-qualified-fn>`
   nomination in the plan's §8 mapping block (the grammar already
   binds `bound-test:` lines; step ids must be declared rows) naming
   the post-R4 test fn's actual location (`journeys.rs::…`,
   `journeys/habits_journey.rs::…`, …). The nominated fn names are
   taken from the tree as it stands at stage 2 — this is a plan-format
   migration, never a test rename.
2. **The "cites" rows are NOT gate rows**: prose capability mentions
   of the form "task.create cites US-1" were never journey steps
   (they do not appear in the plan's §3 step tables) — the MISSING
   verdicts come from the gate scanning token-shaped prose in the
   mapping columns. Decision: convert those columns to non-token
   prose (or drop the token text), so only declared journey rows are
   gate-scanned. No resolver change (teaching the resolver a new
   syntax is rejected — the grammar is the round's asset; the plans
   move, not the gate).
3. Each migrated row keeps its verdict semantics: a step that was
   FOUND pre-consolidation resolves FOUND/BOUND post-migration.

**Acceptance**: `scripts/ux-coverage.sh <plan> --gate` exits 0 with
MISSING 0 / AMBIGUOUS 0 for both plans (stage-2 evidence), and the
whole-live-plan sweep shows no FOUND→MISSING flips anywhere else.

### 2.5 #1112 — migrate `habits_journey.rs` onto the B4a helper — D-1112-a

**Decision (D-1112-a)**: rewrite the file's legacy journeys onto
`support::harness::{run_journey, run_journey_with_settings, Harness}`,
delete the private 3-field `Harness` and the local `mount*` /
`dummy_receiver` copies. Journey test **fn names (tokens) and
assertions verbatim** — the B4b-added journeys at the file's tail
already compose through the helper and must not churn beyond import
tidying. Same treatment applies to any *other* `journeys/` member
still carrying a private preamble (stage 2 checks
`tracker_journeys.rs`, `detail_shortcuts_journeys.rs`,
`board_move_journeys.rs`, `focus_law_journeys.rs` for private
`Harness`/mount copies; migrate exactly those found, nothing else).
Acceptance: `grep -c "struct Harness" crates/taskboard-ui-slint/tests/journeys/`
returns matches only in `tests/support/harness.rs`; the flock-wrapped
nextest run is green; zero test-case renames (the R4 name-set
identity law).

### 2.6 #1097 — ci.yml comment names a nonexistent file — D-1097-a

**Decision (D-1097-a)**: fix the comment at
`.github/workflows/ci.yml:197` to name the real file:
`crates/taskboard-ui-slint/tests/inventory/bench_fixture_contract.rs`
(verified present). Comment-only diff; no workflow semantics change.

### 2.7 #1133 — rename `ConfigBatchError` — D-1133-a

**Decision (D-1133-a)**: **rename-only** — `ConfigBatchError` →
`ConfigBatchEditError` (the unset batch landed; the type now serves
both batch verbs' refusal classes). The `BadValue` variant stays with
its doc updated to state set-only reachability (unset carries no
values — the documented leniency, not a split). The split option is
rejected: one batch lane, one typed-class taxonomy — forking a
set-only sibling enum duplicates the shared variants (`BadArity`,
`DuplicateKey`, `EnvLocked`, `Io`, `UnknownKey`) for zero behavioral
gain. No behavior change; the relay's end-to-end typed-class rule is
unaffected; mechanical rename across
`crates/taskboard-app/src/commands/{mod,config}.rs` + tests.

### 2.8 #1137 — the diagnostics hop's Mutex cell → the plan-named watch channel — D-1137-a

**Problem**: `SqliteTaskRepository.diagnostics` is
`Arc<std::sync::Mutex<Option<LoadDiagnostics>>>`; the accepted plan
D-B2-6 (`.artifacts/plans/2026-11-19-journal-decode-robustness-plan.md`)
named `watch::Sender<LoadDiagnostics>` for this storage→engine hop,
and AGENTS §3 reserves Mutex-free primitives for cross-actor hops.

**Decision (D-1137-a)**: adopt the watch channel exactly as the plan
named it, reconciled with the domain trait's take-once contract:

- `SqliteTaskRepository` holds a
  `tokio::sync::watch::Sender<Option<LoadDiagnostics>>` (channel
  initial value `None`, seeded from the closed-over receiver —
  `watch::channel(None)` then send, or `watch::channel` directly)
  plus its own `watch::Receiver` clone for consumption.
- After a degraded load the repo sends `Some(diag)`; after a healthy
  load it sends `None` (clearing a stale pending value — the Mutex
  cell's take-once semantics preserved one step earlier).
- `LoadDiagnosticsSource::take_load_diagnostics` (domain trait,
  **unchanged**) is implemented via the internal receiver:
  `has_changed()` gate + `borrow_and_update()` — the
  mark-read-on-consume property gives exactly one consumption per
  load, "never a replayed signal" (persistence.rs:174) holds without
  touching the trait. `Option` in the channel (not bare
  `LoadDiagnostics`) keeps the healthy-load "no signal" state
  representable, matching the trait's `Option` return.
- `tokio` is already a workspace dependency of the storage crate
  (features `rt`, `macros`, `time`; `watch` needs no runtime feature)
  — no new dependency edge; `#![forbid(unsafe_code)]` unaffected.
- The `LoadDiagnosticsSource` object still hangs off
  `SqliteTaskRepository` (repo.rs:1039) — only the cell underneath
  changes; fakes and the engine's read site are untouched.

**Self-tests** (in the storage crate's existing test module):
(a) degraded load → `take` returns `Some`, immediate second `take`
returns `None`; (b) healthy load after a degraded one → next `take`
is `None` (cleared, not replayed); (c) two sequential degraded loads
→ `take` yields each diagnostics exactly once, in order.

### 2.9 #1110 — gate-lock hung-holder liveness — D-1110-a

**Problem**: both gate locks (`memory-gate.lock`,
`quality-gates.lock`) release on holder *death* but say nothing about
a holder that hangs while alive (the 2026-11-20 incident: pid
3151920 at ~0.1% CPU for 55+ minutes, every queued pre-commit blind).

**Decision (D-1110-a)** — surface-first, never act (the two-mode law
composes; a liveness check cannot kill or steal from legitimate long
holders):

- New shared helper `scripts/gate-lock-liveness.sh`, sourced by both
  gates: `gate_holder_liveness <lock_file>` reads the holder record
  both gates already write (`pid=… branch=… worktree=… started=…`),
  samples the holder pid's cumulative CPU time
  (`/proc/<pid>/stat` fields 14+15, utime+stime), and — when a
  caller-supplied sample comparison shows a zero delta across the
  poll window — classifies `likely-hung` / `alive`. A vanished pid
  classifies `gone` (the kernel releases the lock; purely
  informational).
- **Memory-gate** (`scripts/memory-gate.sh`): in the phase-1 lock
  wait loop and phase-2 floor wait loop, once a holder has shown no
  CPU-time delta for 3 consecutive polls (~90s at the default 30s
  poll), the waiting message becomes
  `held by a likely-hung pid <pid> (branch <b>, started <t>; no CPU time in ~90s — triage that lane)` ;
  `--status` grows the same classification. Behavior is report-only:
  wait duration, timeout, exit codes unchanged; no signal, no steal.
- **Quality gates** (`scripts/run-quality-gates.sh`): same wiring in
  its wait loop and `--status` (its lock file already carries `pid=`,
  verified at line 136).
- **Law amendment** (`docs/agent_dispatch.org`): under the two-mode
  law, add the constraint — gate-lock liveness reporting is
  detection-only; no gate may kill, signal, or steal a lock from a
  live holder regardless of its CPU profile (a legitimate long
  holder is indistinguishable from a wedged one by CPU time alone;
  triage is the dispatcher's job, the safe release path remains
  flock-on-kill). The #894 flock-deferral rule references the
  classification line as the triage trigger.
- **Self-test**: `scripts/gate-lock-liveness.sh --self-test` spawns
  an idle `sleep`-backed fake holder and a spinning `while :; do :;
  done` holder in a temp lock file, asserts the two classifications
  and the vanished-pid case, and self-cleans. Both gates' `--status`
  paths keep their existing free/held output shape (machine-readable
  contract — "lock is FREE/HELD" lines unchanged; the hung line is
  an addition, not a rewording).

## 3. Order of work (stage 2)

1. #1097 (one-line comment) and #1074 (one line) — trivial, first.
2. #1073 + #1076 together (same file, same self-test suite bump) —
   run `--self-test` before/after.
3. #1110 (helper + both gates + law amendment + self-test).
4. #1137 (storage watch swap + self-tests) and #1133 (rename) —
   independent crates; hook-gated commits.
5. #1112 (journeys migration; zero renames) — flock-wrapped nextest
   for the ui-slint suites.
6. #1108 last (depends on 2's gate fixes being in): re-row the two
   plans, then the full live-plan sweep before/after → the FOUND→
   MISSING flip table in the stage-2 report.
7. Commit per logical item (`fix(scripts): …`, `refactor(app): …`,
   `refactor(storage): …`, `test(ui-slint): …`, `docs(plans): …`,
   `ci: …`); every commit stands alone (rebase-merge hygiene).

## 4. Risks

- **#1073's tier rule could flip an honest FOUND to cite a different
  (correct-tier) file** whose token lives only in test-support →
  verdict stays FOUND but the cited path changes; accepted (evidence
  path change is not a verdict flip; the sweep table records it).
- **#1108 re-rowing** depends on current fn locations; if a journey
  fn moved again before stage 2, the nomination names the tree as it
  stands (plan-format migration, not a rename law).
- **#1137**: `has_changed()` on a receiver clones the pending value;
  `LoadDiagnostics` must be `Clone` (it is — it crosses the domain
  trait by value today). If a second receiver ever appears, the
  take-once contract would need re-stating — noted in the type's doc
  comment (deliberate leniency documented at the type, per A1).
- **#1110**: `/proc/<pid>/stat` field parsing is Linux-specific —
  the dev/target platform is x86_64 Linux; the helper fails closed
  to the unclassified wait message on unreadable proc (never masks
  the existing contract).
- **Gate lock contention**: stage 2 runs everything through the
  flock-deferral rule; a docs-only stage-1 plan commit needs only the
  pre-commit hooks.

## 5. Estimated diff size

~450–600 lines total: scripts ~250 (helper + gate wiring + self-tests
+ ux-coverage fixes), journeys migration ~120 (mostly deletions),
storage watch swap ~80 (incl. tests), app rename ~60 (mechanical),
two plan re-rows ~50, ci.yml 1 line, law amendment ~15.

## 6. Backlog issues filed during planning

None — no out-of-scope findings surfaced (§ [L](d) satisfied vacuously;
report records "none").
