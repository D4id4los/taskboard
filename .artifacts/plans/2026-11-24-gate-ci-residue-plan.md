# Chunk D2 — Gate/CI Residue — Implementation Plan

- **Date**: 2026-11-24
- **Round**: backlog-reduction round 6, chunk D2 (dispatch plan
  `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
  §12; scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §D — normative)
- **Branch**: `ci/gate-ci-residue` (off fetched `origin/main` a0a66fc3)
- **Issues**: #1285, #1282, #1266, #1268, #1277, #1274 (priority),
  #1297 (+ #1333 adjacency — verdict in §0.1)
- **Size estimate**: M (~350–450 changed lines: `scripts/ux-coverage.sh`
  + its fixture self-test tree, `scripts/worktree-doctor.sh` +
  `tests/scripts/worktree-doctor-test.sh`, `.githooks/pre-commit` + a
  new mode-decision helper + its test, `mutation-testing.yml`, three
  production files' dedup, two template/doc amendments)
- **Gate**: pre-commit hooks + flock-deferral rule binding; every cargo
  invocation through `scripts/memory-gate.sh`. When the gate lock is
  contended and no other useful work remains: commit, report
  "gate deferred: lock held by <holder>", end the stage.

## 0. Recorded decisions (stage-1 resolutions)

0.1 **#1333 (run-quality-gates liveness false-positive) is adjacent,
not duplicated — cross-referenced, not taken into D2's diff.** Both
#1333 and #1274 live in the gate family and both change the gate's
wait/liveness messaging, but the fixes are independent: #1333 fixes the
*holder-liveness probe* (gate-lock-liveness.sh's pid-only CPU
sampling — the wrapper bash sits in `wait` at zero CPU while children
work), while #1274 fixes *hook admission* (which clippy mode the
pre-commit hook selects). No shared code path changes: #1274's edit
lands in `.githooks/pre-commit` + the new mode helper; #1333's lands in
`scripts/gate-lock-liveness.sh`. Cross-reference comments filed on both
issues so a future agent doesn't merge them. **Verdict: out of D2
scope; already owned (C1 follow-up), backlog comment only.**

0.2 **Survival check (stage-1 verification per instruction 2).** All
seven issues are OPEN on GitHub as of 2026-11-24 and none is fixed on
`main`:

| Issue | Survived? | Evidence |
|---|---|---|
| #1285 | yes | no alias/bind grammar in scripts/ux-coverage.sh (§1); only `bound-test:` (step→fn) exists, no test-name aliasing |
| #1282 | yes | `.artifacts/templates/ux-journey-plan.md` §6 pins no callback-cell spelling; `tok_match` greps verbatim (no snake/kebab normalization) |
| #1266 | yes | `is_absent` (scripts/ux-coverage.sh:379) still matches only empty/`—`/`-` — E1's recorded deferral, filed as this issue |
| #1268 | yes | `mutation-testing.yml` runs `-p taskboard-domain -p taskboard-state` only, default features, no pdf window, taskboard-app not a lane at all |
| #1277 | yes | `scripts/worktree-doctor.sh:78,183-184` still feed plain `sort` output to `comm` |
| #1274 | yes | `.githooks/pre-commit:34` still wraps clippy in an unconditionally-locking `scripts/memory-gate.sh --` |
| #1297 | yes | SonarCloud still reports the three duplication pairs (measured below) |

0.3 **#1274 mechanism: hook-side diff-class detection →
`--preflight` clippy for non-Rust diffs; bounded-wait fallback and
shared-admission REJECTED.** Full argument in §5; the oomd-law
compatibility note: the exclusive lock exists to keep the box's
*heavy-command concurrency* at exactly 1. A docs-only commit's clippy
run is not a heavy command (nothing recompiles — the incremental cache
is warm, so it is a cache-validating walk measured in seconds), and it
only ever reaches clippy through `--preflight`, which still enforces
the MemAvailable floor check before exec. Option (b) (bounded
`flock -w` then preflight fallback for *Rust* commits) is rejected: it
deliberately runs an unserialized real clippy build during peak load —
exactly the aggregate-pressure shape the 2026-11-23 law closes. Option
(c) (shared admission lock) is rejected: it re-admits concurrent heavy
commands by construction, breaking the gate's core invariant.

0.4 **Gate-grammar scope discipline (the E1 precedent, decision 3
there): one grammar change per gate family per chunk.** D2's home gate
is `ux-coverage.sh` and it takes TWO grammar changes (#1285 aliasing,
#1266 dash-cells) plus one normalization (#1282). Rationale for
admitting all three: each lands with its own self-test matrix rows and
none changes another's semantics (aliasing = new line grammar; dash
cells = `is_absent` only; snake/kebab = `tok_match` token comparison
only). The plan keeps them as three independent commits so a review or
revert can slice them apart.

0.5 **Plans are the contract, the gate tolerates spelling, not
prose.** #1282's normalization makes snake-authored callback cells
resolve against kebab slint callbacks (and vice versa) at the token
level only; it does not relax absence/annotation grammar — #1266 owns
that, narrowly (leading-dash ⇒ absent-with-annotation), and #1217/#1236
-style prose cells stay MISSING. The template gets the pinned spelling
rule regardless (docs are cheap; the tolerance is a safety net, not the
contract).

## 1. #1285 — exact-name nomination alias/bind mechanism

**Fix level: gate grammar + self-tests.**

- New plan-body line grammar, parsed like `bound-test:` (same margin
  tolerance, same PARSE-ERROR discipline):

  ```
  test-alias: <nom-name> -> <file-side>.rs::<existing-fn>
  ```

  `<nom-name>` is the plan-invented evidence name that §6's Test column
  nominates (e.g. `us3_s5_clear_reminder_rules_undo_restores_all_rules`);
  `<existing-fn>` is the differently-named behavior-bearing test it
  binds to (e.g. `us3_s4_cleared_set_resurrects_id_ascending`). The
  alias is checked with `fn_boundary_matches` against the named file
  exactly like an exact-name nomination — the plan claims the existing
  test as the step's evidence instead of minting a twin.
- Parse rules (strict, mirroring bound-test): malformed line ⇒
  `PARSE-ERROR: test-alias: malformed line` + exit 2; unknown fn
  (no boundary hit in the named file) ⇒ the nominating step reads
  MISSING — a lying alias is a lie, never a pass (the #768 precedent).
  Alias target must be a full fn name (no bare-token/scoped form) —
  that would collide with `scoped_nom` semantics.
- Priority order unchanged; `test-alias` entries feed `exact_nom` with
  their *existing* fn name, so resolution order stays: exact-name >
  file-scoped token > bound-test > unscoped. The Test column may
  nominate the alias name or the real name — both resolve identically.
  Documented in the script header block and in
  `.artifacts/templates/ux-journey-plan.md` §6 (one sentence, with the
  #1285 issue link).
- **Self-tests** (fixture tree, following the existing miniature-plan
  harness rows): (a) PASS — a step nominated via a `test-alias:` line
  whose target fn exists reads BOUND with the *target's* file:line
  evidence, exit 0; (b) MISSING — a dead alias target exits 1, step
  MISSING, 0 credit from the twin-shaped name; (c) PARSE-ERROR — a
  malformed alias line exits 2; (d) priority — an exact-name fn that
  exists wins over an alias for the same token (aliasing never
  shadows a real test).

## 2. #1282 — snake vs kebab callback spelling

**Fix level: gate normalization + template rule + self-tests.**

- `tok_match` (scripts/ux-coverage.sh:253) gains kebab normalization:
  both the token and each universe line are normalized
  (`_` → `-`) before the fixed-string match; the match itself stays
  literal (no regex, no case folding — case is load-bearing in
  `ActionId::` tokens, so only the `_`/`-` axis normalizes).
- Uniqueness/ambiguity semantics preserved: normalization is applied
  symmetrically on both sides, so a snake cell matching two kebab
  callbacks still reads AMBIGUOUS; a cell that is genuinely absent
  never normalizes into a hit.
- The `adapter_match` `on_<cb>` side is untouched: the kebab adapter
  resolves via the `// slint: <callback>` annotation on the
  `install_seam!` line (the idiom #1282 documents); normalization only
  widens the slint-universe match.
- Template amendment: `.artifacts/templates/ux-journey-plan.md` §6
  pins "Callback cells are spelled exactly as the slint callback
  (kebab-case, no type annotation unless the cell carries a token)"
  with the #1282 link. The rule is normative; the gate normalization is
  the tolerance net.
- **Self-tests**: (a) PASS — a snake callback cell
  (`habit_edit_asked`) against a kebab universe (`habit-edit-asked`)
  resolves, exit 0; (b) AMBIGUOUS — a snake cell matching two kebab
  callbacks stays ambiguous; (c) MISSING unchanged — a snake cell with
  no kebab counterpart (differing letters, not just separators) stays
  missing.

## 3. #1266 — dash-cell grammar with annotations

**Fix level: gate grammar + self-tests.**

- `is_absent` extends to absent-with-annotation: a cell whose first
  non-space character is `—` (em-dash) or `-` (hyphen) is absence; the
  remainder is annotation and is never tokenized. Empty, bare `—`, and
  bare `-` keep the existing behavior (strict superset — no existing
  PASS/MISSING fixture row changes verdict).
- Deliberate leniency, documented at the function: a cell that begins
  with a dash is *authored* as a marked-empty; a real token cannot
  begin with `-` or `—` in any grammar this gate parses
  (`us<n>_s<k>`, fn names, `ActionId::…`, paths), so there is no token
  this rule can swallow. The self-test matrix records that invariant.
- Callers audited (all five `is_absent` sites: reason, callback,
  action, commands, test columns) — none post-processes an absent cell,
  so the change is confined to the predicate.
- **Self-tests**: (a) PASS — a wiring row with `— (binding rows)` in a
  token column reads as absent (no MISSING for the annotation);
  (b) strict superset — every existing bare-dash fixture verdict
  unchanged; (c) MISSING unchanged — a prose cell not beginning with a
  dash (`see §4`) still reads MISSING.

## 4. #1268 — the mutants lane's pdf-feature window

**Fix level: CI workflow change + scope self-check.**

- `mutation-testing.yml` gains a second job,
  `cargo-mutants (app commands, pdf-export)`: same toolchain/cache
  steps, running

  ```
  cargo mutants -p taskboard-app \
    --file 'src/commands/mod.rs' --file 'src/commands/output.rs' \
    --features pdf-export --in-place --timeout-multiplier 2.0
  ```

  (exact `--file` spellings verified against the tree at stage 2),
  `continue-on-error: true` + the same Collect/Upload steps (the
  output-dir contract is per-job — the artifact name gains the job
  suffix so the two jobs never collide).
- Scope is the two files the issue names (the `dispatch_line` pdf
  predicate in `commands/mod.rs` and the pdf-open/`OpenFailure` mapping
  in `commands/output.rs`); no full `taskboard-app` lane is added —
  that is a bigger scheduling decision (backlog note in §10) and the
  issue asks only for the feature window.
- **Self-check (the lane has no in-repo test harness; the guard that
  can fail is the scope proof)**: stage 2 runs, through the memory
  gate, `scripts/memory-gate.sh -- cargo mutants -p taskboard-app
  --file src/commands/mod.rs --features pdf-export --list` (list mode —
  minutes, not hours) and asserts the two #1176 kill-pin lines
  (`mod.rs:2011`-class dispatch predicate, the output.rs mapping) appear
  in the mutant list — proving the window compiles the feature and
  covers the pins, so a correctly-written kill can flip. The assertion
  is recorded in the stage-2 report, not baked into CI (CI's oracle is
  the next confirmation pass reporting #1176's survivor caught).
- The workflow YAML is not shellcheck-covered (not `scripts/*.sh`);
  the embedded `run:` blocks get `shellcheck` disable waivers per
  existing CI-file convention if the hook lints them.

## 5. #1274 — pre-commit hooks behind the memory-gate lock (PRIORITY)

**Fix level: hook mechanism + proof-by-construction self-test.**

Mechanism (decision 0.3):

- The mode decision is factored out of `.githooks/pre-commit` into
  `scripts/commit-clippy-mode.sh` (new, shellcheck-clean): reads the
  staged diff (`git diff --cached --name-only --diff-filter=ACMR HEAD`
  — including renames/copies; new-file-only commits have no HEAD
  baseline and fall back to `--cached --name-only` against the empty
  tree, `4b825dc6…`) and prints `preflight` if **no** staged path
  matches the Rust-relevant set (`*.rs`, `Cargo.toml`, `Cargo.lock`,
  `.cargo/*.toml`), else `gated`. The hook consumes the verdict:

  ```
  case "$(scripts/commit-clippy-mode.sh)" in
    preflight) scripts/memory-gate.sh --preflight -- cargo clippy … ;;
    gated)     scripts/memory-gate.sh -- cargo clippy … ;;
  esac
  ```

  fmt, the slint token lint, and shellcheck run unconditionally for
  every commit (instant, ungated) — only clippy's admission mode
  changes.
- **oomd-law compatibility argument (recorded here per the dispatch
  law)**: the exclusive lock implements *heavy-command concurrency = 1*
  across all worktrees. A docs-only commit's clippy run recompiles
  nothing (warm incremental cache ⇒ seconds of CPU, no rustc fan-out),
  and it admits via `--preflight`, which still refuses to start below
  the MemAvailable floor — so the floor protection is intact and no
  meaningful build pressure escapes serialization. What changes is only
  that a zero-Rust commit no longer *waits behind* a 40-minute mutants
  shard to run a cache-validating no-op. Option (b) is rejected because
  it unserializes a *real* clippy build under load; option (c) is
  rejected because shared admission reintroduces concurrent heavy
  starts by design. AGENTS §10's "the gate must never see an un-gated
  cargo command" is honored: `--preflight` is a memory-gate mode, not a
  bypass.
- **Self-tests** (new `tests/scripts/commit-clippy-mode-test.sh`,
  following the worktree-doctor-test.sh pattern):
  (a) a staged set of only `*.md`/`.org`/`.toml`-config-free paths ⇒
  `preflight`;
  (b) one staged `*.rs` among docs ⇒ `gated`;
  (c) staged `Cargo.toml`/`Cargo.lock` ⇒ `gated`;
  (d) empty stage (nothing to commit path) ⇒ `gated` (conservative
  default — never weaken the gate by accident);
  (e) **the proof-of-law test**: in a scratch fixture repo, hold
  `memory-gate.lock` with a background `flock`, stage a docs-only
  change, run the hook's clippy branch selection + preflight path end
  to end with the mode helper stubbed to always-preflight — completes
  within the deadline while the lock is HELD (the dispatcher repeats
  this live with a real docs-only probe per the runbook §17).
  Time-manipulation rule honored: deadline via poll-with-deadline, no
  sleeps beyond the harness's existing poll loop.

## 6. #1277 — the doctor's locale sort warnings

**Fix level: one-line fix + fixture self-test.**

- `scripts/worktree-doctor.sh`: both `comm -23` inputs (line 183–184)
  and the line-78 `sort` feeding the registered-worktree list get
  `LC_ALL=C sort` — `comm`'s byte-order expectation then matches the
  collation regardless of the ambient locale, killing the
  `comm: file 2 is not in sorted order` spew for case-mixed and
  non-ASCII WT-suffix paths. (`grep -vxF -f` was considered and
  rejected: it changes the output ordering contract the doctor's KEEP/
  REMOVE lines follow and reads worse; the locale pin is the minimal
  correct fix.)
- **Self-test**: extend `tests/scripts/worktree-doctor-test.sh` (the
  file the issue says already filters stderr — its workaround is
  removed as part of this fix) with a fixture worktree sibling whose
  name mixes case and a non-ASCII byte, run the doctor under
  `LC_ALL=en_US.UTF-8` (falling back to the ambient UTF-8 locale if
  unset — the test asserts *structural* absence of comm's sort
  complaints on stderr, and its old stderr filter is deleted so the
  test genuinely guards).

## 7. #1297 — old-code duplication in status/export production files

**Fix level: mechanical dedup refactor, guarded by the existing suite
+ SonarCloud re-check.**

Measured SonarCloud duplication pairs (stage-1, project
`D4id4los_taskboard`):

| File | Blocks | ~Lines |
|---|---|---|
| `crates/taskboard-ui-slint/src/adapter.rs` | 2 | ~14 + ~14 |
| `crates/taskboard-app/src/main.rs` | 2 | ~13 + ~12 (`daemon_stop`-class command prologues: config load + connect + report) |
| `crates/taskboard-app/src/ui.rs` | 2 | ~25 + ~25 (the `settings_apply_registry` adjacent `Apply::live()`/`on_restart` entry stretches) |

- Fold each pair into a shared constructor/helper per the issue's ask:
  the command-prologue pair in `main.rs` gets a small typed helper
  (config-load-or-report + try_connect-or-report, returning the typed
  errors verbatim — no error-shape change); the `ui.rs` registry gets
  a local `live`/`on_restart` entry shorthand removing the repeated
  closure-block shape; the `adapter.rs` pair folds per what stage 2
  finds at the flagged offsets (the offsets are token-based; stage 2
  re-derives the line spans from the SonarCloud API before editing).
- **Guard (typed outcomes, the [L](a) law applied to a refactor)**: no
  new tests assert refactor internals (that would duplicate behavior —
  forbidden); the guards are (i) the existing workspace suite staying
  green through the quality gates, and (ii) stage 3 re-querying the
  SonarCloud duplication API for the three files and recording
  zero-remaining-blocks (or a recorded survivor with reason) in the
  stage-3 report — a guard that can genuinely fail if the fold is
  incomplete.
- Clippy/fmt clean per §10; no API change outside the three files
  except the new `pub(crate)`/private helpers.

## 8. Sequencing & commit plan (each commit stands alone)

1. `fix(scripts): the doctor's comm inputs sort in C locale` — #1277
   (+ test extension). Smallest, independent, unblocks stderr noise
   for every later run.
2. `fix(ci): pre-commit runs docs-only clippy via the gate's preflight`
   — #1274 (+ `scripts/commit-clippy-mode.sh` + its test). **Lands
   second** so the dispatcher's early verification (runbook §17) can
   start while the rest of the chunk proceeds — this is the item the
   dispatcher probes first because it changes every chunk's commit
   latency.
3. `feat(scripts): ux-coverage learns test-alias nominations` — #1285
   (+ fixture self-test rows + template §6 sentence).
4. `fix(scripts): ux-coverage treats leading-dash cells as absence` —
   #1266 (+ matrix rows).
5. `fix(scripts): ux-coverage normalizes snake/kebab callback tokens`
   — #1282 (+ matrix rows + template §6 spelling rule). Commits 3–5
   are independent slices of the same file; each keeps the full
   self-test run green.
6. `ci(mutants): give the app commands lane a pdf-export window` —
   #1268 (+ the `--list` scope proof recorded in the stage-2 report).
7. `refactor(app): fold the status/export old-code duplication blocks`
   — #1297 (three files + SonarCloud re-check evidence).

Heavy local verification order: the `--list` proof (6) and the
quality-gates suite run through
`scripts/run-quality-gates.sh scripts/memory-gate.sh -- …` discipline;
if the gate lock is contended at handoff time with no other work left,
commit, report "gate deferred: lock held by <holder>", end the stage
(the binding [D] law).

## 9. Acceptance criteria

- AC-1 (#1274): a docs-only commit during a HELD memory-gate lock
  completes (fixture test (e) green; dispatcher's live probe recorded).
- AC-2: every gate-grammar change (#1285/#1266/#1282) carries failing-
  first self-test rows that pass after the fix; the full ux-coverage
  self-test suite exits 0.
- AC-3 (#1277): the doctor emits no comm sort warnings under a UTF-8
  locale with case-mixed/non-ASCII worktree names; the old stderr
  filter is gone from the fixture test.
- AC-4 (#1268): the pdf-export window job exists and the recorded
  `--list` proof shows the #1176-class pins in scope.
- AC-5 (#1297): SonarCloud duplication for the three files drops to
  zero blocks (survivors recorded with reason if any).
- AC-6: `cargo fmt --check`, clippy `-D warnings`, nextest, doc-tests,
  `cargo deny` green via the quality-gates wrapper (or the deferral
  law invoked and reported).

## 10. Out-of-scope findings → backlog issues

Filed or cross-referenced during stage 1 (numbers listed in the final
report):

- #1333 — cross-reference comment only (owned by C1 follow-up;
  distinct from #1274, argument in §0.1).
- A full `taskboard-app` mutants lane (beyond the two-file pdf window)
  — new backlog issue if stage 2 confirms appetite; not pre-filed at
  stage 1 since the window job may make it moot.
- No other new out-of-scope work discovered in the seams.

## 11. Risks

- **R1 — `is_absent` superset risk**: if any journey plan ever authors
  a real token beginning with `-`, the dash rule would swallow it. The
  grammar makes this impossible today (recorded at the function); the
  self-test matrix pins it.
- **R2 — snake/kebab normalization could mask a genuine typo** (a
  snake cell whose kebab twin differs by more than separators still
  misses — only separator drift is forgiven). Acceptable: the template
  rule stays normative; the gate is the net.
- **R3 — the pre-commit mode helper must read the *staged* diff, not
  the worktree** (partial staging is legal); the self-tests stage
  fixtures explicitly to pin that.
- **R4 — the mutants window job doubles the workflow's runtime on
  weekly cadence**; scoped to two files it should stay well under the
  domain/state job's runtime. `continue-on-error` keeps report-only
  semantics identical to the existing job.
- **R5 — `main.rs` prologue helper**: the duplicated blocks differ in
  comment prose between sites; the fold must keep each site's
  commentary (moved, not dropped) or the review loses incident law
  context.
- **R6 — gate-lock contention for the local `--list` proof and the
  quality gates**: the deferral law applies; commit and report rather
  than block.
