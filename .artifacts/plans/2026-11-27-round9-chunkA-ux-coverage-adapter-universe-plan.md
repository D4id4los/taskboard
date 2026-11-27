# Round 9, Chunk A — ux-coverage adapter universe + small debt — Chunk Plan & Agent Prompts

- **Date**: 2026-11-27
- **Kind**: chunk plan — three-stage agent prompts (planning [this
  document] → implementation → review) for one chunk of backlog-
  reduction round 9: the #1514 ux-coverage adapter-universe repair,
  the #1507 `workspace.package` warning debt, and the #1508 docs
  amendment. No production code in this commit.
- **Origin**: issue bodies of **#1514** (ci, p2), **#1507** (chore,
  p3), **#1508** (documentation, p3) — normative. Round-9 dispatch
  context: backlog-reduction round 9 (chunk R9-A). Recorded reality
  below is at `origin/main` `9fc0d5ee`.
- **Status**: READY — no operator gates, no UX journey plan required
  (a CI tooling fix, a manifest chore, and a plan-artefact docs
  amendment; per `docs/ux_planning.org` none of these adds or changes
  a user-visible capability — no new verb, badge, view, or setting —
  so no UX journey plan is written).

## 1. Scope & ordering

| Issue | Change | Files |
|---|---|---|
| #1514 | `scripts/ux-coverage.sh` builds the adapter-side evidence universe from the adapter module layout instead of the single name `adapter.rs`; new self-test case pins a modular tree (`adapter/settings.rs`) | `scripts/ux-coverage.sh`, `scripts/fixtures/ux-coverage-fixture/tree-modular/**`, one new self-test case |
| #1507 | delete the three unused `[workspace.package]` fields (decision + evidence in §3) | root `Cargo.toml` lines 20–22 |
| #1508 | one-line amendments to the module map §2 | `.artifacts/plans/2026-11-25-adapter-decomposition-plan.md` (rows at lines 152, 169 + the completeness check) |

**Ordering**: single branch `ci/ux-coverage-adapter-universe`, one
PR. #1514 first (its gate evidence is the acceptance headline),
#1507 and #1508 independent of it and of each other. One commit per
coherent change; every commit stands alone (rebase-merge hygiene).

## 2. Recorded reality (origin/main `9fc0d5ee`)

- `scripts/ux-coverage.sh:944`:
  `mapfile -t adapter_files < <(find_evidence 'adapter.rs')` — the
  only universe-build site for the adapter side. `find_evidence`
  (lines ~306–339) prunes the fixture root, `target/`,
  `node_modules`, `.git`, then post-filters into the #1073 tiers
  (tier 0 = not under `/test_support|tests/`, tier 1 = the rest,
  each `LC_ALL=C sort -u`). `adapter_match` (lines ~284–299) greps
  `on_<callback>` / `install_seam!` per file. No file named
  `adapter.rs` exists in the repo tree; the seams live under
  `crates/taskboard-ui-slint/src/adapter/` (12 top-level `.rs`
  modules incl. `mod.rs`, `settings.rs`) and
  `crates/taskboard-ui-slint/src/adapter/seams/` (23 leaf modules).
  `seam_inventory.rs` lives at
  `crates/taskboard-ui-slint/tests/inventory/seam_inventory.rs` and
  is located by a *separate* `find_evidence 'seam_inventory.rs'`
  call (line ~962) — unaffected by this fix.
- **Self-test layout** (`self_test()`, line 418): 26 cases
  (1–26), each running the gate against fixture plans under
  `scripts/fixtures/ux-coverage-fixture/` with `--root` variants of
  the shared `tree/` (which carries a **flat** `tree/adapter.rs`).
  **Current state recorded 2026-11-27: all 26 PASS, exit 0.** The
  fixture `tree/adapter.rs` must keep matching the universe
  predicate (the `-name 'adapter*.rs'` arm) so cases 1–26 are
  untouched.
- **Theme plan red state recorded 2026-11-27**:
  `bash scripts/ux-coverage.sh
  .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate` →
  exit 1, `checks: 34, FOUND: 19, MISSING: 1, AMBIGUOUS: 0,
  BOUND: 14, INVENTORY-MISS: 0`; the one MISSING row is
  `W-1 callback settings_theme_picked → MISSING`. The seam exists:
  `adapter/settings.rs:2727` `install_seam!(window:
  window.on_settings_theme_picked, …)` (fn at :1865, :2731 call).
- **Cargo.toml lines 20–22** (root `[workspace.package]`):
  `readme = "README.org"`, `license = "MIT OR Apache-2.0"`,
  `publish = false`. No member crate inherits any of the three
  (`crates/*/Cargo.toml` each set their own explicit `license`
  per AGENTS §6's dual tier: MIT OR Apache-2.0 for the six library
  crates, AGPL-3.0-only for `taskboard-app` and
  `taskboard-ui-slint`; none sets `readme` or `publish`).
- **#1507 evidence (what cargo actually warns)**: gated
  `cargo check -p taskboard-domain` emits exactly three
  `warning: unused field … in 'workspace.package'` notes
  (`readme`, `license`, `publish`), each pointing at
  `cargo::unused_workspace_package_fields`; summary
  `warning: workspace (manifest) generated 3 warnings`.
  `cargo metadata --no-deps` emits none — the lint fires on member
  builds, so the verification command is a (gated) build/clippy,
  not metadata.

## 3. Decisions

### D1 — #1514 universe predicate (normative for stage 2)

Build the adapter universe from the **module layout**: every `.rs`
file whose path contains an `adapter/` path segment, **plus** any
file whose *name* matches `adapter*.rs` (the legacy flat shape,
which keeps the existing fixture and self-tests 1–26 byte-stable).
Concretely: a dedicated universe builder (e.g.
`find_evidence_adapter()`) that runs the same `find "$EVID_ROOT"`
with the same prunes (fixture root via `FX_ROOT`, `target/`,
`node_modules`, `.git`) under `\( -path '*/adapter/*' -o -name
'adapter*.rs' \) -type f -name '*.rs' -print`, and applies the
**identical** #1073 tier-0/tier-1 post-filter verbatim. Do not
weaken `find_evidence` for the other universes (messages,
requests, actions, slint, seam_inventory keep their name
predicates and callers). The issue's alternative (parameterizing
`find_evidence` with a name-pattern argument) is rejected: the
adapter predicate is a *path* predicate, not a name predicate, and
a parameter would force the other five callers through a signature
change for no behavioural gain — speculative generality (AGENTS §5
spirit; stage-3 code-quality mandate).

### D2 — new self-test case (normative for stage 2)

New fixture tree `scripts/fixtures/ux-coverage-fixture/tree-modular/`:
a copy of `tree/` with `adapter.rs` **moved** to
`adapter/settings.rs` (same file content, so the same wiring rows
resolve). New self-test case **27**: run the existing PASS fixture
plan (`plan.md`) against `--root …/tree-modular` and assert the
same structural outcome as self-test 1 (exit 0, exactly 1 BOUND
row) **and** that the wiring row's adapter citation names
`adapter/settings.rs` (structural: the citation path contains
`adapter/settings.rs` — a path fact, not prose text). This pins
the regression: pre-fix, the modular tree yields zero wiring FOUND
(the #1514 bug); post-fix it resolves. All 26 existing cases run
unchanged — none may be weakened, edited, or renumbered.

### D3 — #1507 decision: delete the three fields

**Delete `readme`, `license`, `publish` from root
`[workspace.package]`.** Evidence: all three are unused (§2), and
making `license` meaningful via inheritance would be *wrong* — the
workspace is dual-tier (AGENTS §6): a single inherited
`license = "MIT OR Apache-2.0"` would mislabel the two AGPL-3.0-only
crates that today correctly carry their own explicit field.
`readme`/`publish` serve published-crate metadata; no member is
published and none inherits them. Expected result: zero
`unused_workspace_package_fields` warnings. (If the gated clippy
check in stage 2 surfaces any *other* workspace-field warning, that
is out-of-scope → backlog issue, not a drive-by fix.)

### D4 — #1508 amendment text (normative for stage 2)

Two one-line amendments to
`.artifacts/plans/2026-11-25-adapter-decomposition-plan.md` §2:
(a) the seams-table gap — add/annotate a row recording that
`install_menu_dismiss_seam` (census 14304–14320, landed commit
`c62ca444`) moved with its contiguous home into the
`seams/context.rs` menu family (called by `install_view_seams`),
closing the undo.rs→context.rs jump the completeness check missed;
(b) the tail-test deviation — amend the `adapter/tests.rs` map
row (line 169) and decision 6 (line 224) to record that
`auto_list_backoff_tests` landed in `adapter/replies.rs` (commit
`2da80494`) as a sibling of its subject type, with the reason (its
tests assert on `AutoListBackoff` private fields/methods; the
mapped home would have forced widening). Evidence base: issue
#1508 body. Docs only — no code may ride this commit.

## 4. Verification commands & expected outcomes (stage 2/3 gate)

1. `bash scripts/ux-coverage.sh --self-test` → exit **0**, all 27
   cases PASS (1–26 byte-identical verdicts, 27 new).
2. `bash scripts/ux-coverage.sh .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate`
   → **before** (recorded): exit 1, `W-1 callback settings_theme_picked → MISSING`;
   **after**: exit **0**, the W-1 callback row reads FOUND citing
   `crates/taskboard-ui-slint/src/adapter/settings.rs` (~:2727),
   `MISSING: 0`, BOUND stays 14.
3. `scripts/memory-gate.sh cargo clippy --workspace --all-targets`
   → zero `unused_workspace_package_fields` warnings (and no new
   warnings of any kind; pre-commit hook runs this class anyway).
4. No regression in the tool's other consumers: the fixture-pin
   self-tests 9/12/13/16 (nested fixture-tree + relative-root
   spellings) must stay PASS — the new predicate must respect
   `FX_ROOT` pruning exactly as the name predicate did.

## 5. Invariants that must not break

- **No self-test weakened**: cases 1–26 run verbatim; their
  expected row counts/exit codes unchanged. A diff touching an
  existing case's expectations is a stage-3 finding.
- **Fixture trees still pruned**: the `FX_ROOT` prune and the
  nested-copy cases (9, 12, 13, 16) keep working under the new
  predicate; a nested `fixtures/…/tree*/adapter/…` is never
  evidence.
- **#1073 tier predicate unchanged**: the tier-0/tier-1
  post-filter logic is copied verbatim into the adapter universe
  builder (or shared); its existing callers (first_match /
  adapter_match / nom_file fallback) and the messages/requests/
  actions universes' behaviour are untouched.
- **Boundary anchoring intact**: `adapter_match` still requires
  `on_<cb>` / `install_seam!` hits; the fix widens the *universe*,
  never the match semantics. A cell that is genuinely absent stays
  MISSING.
- **AGENTS §5 test rules for the new self-test case**: exit-code
  and row-count assertions only (the script's own tests assert
  structure, never output prose); no quality ceilings; one
  behaviour, lowest layer (the self-test matrix is the layer).
- **#1507 diff shape**: exactly the three deleted lines; no
  member-manifest edits, no `[workspace.lints]` churn.
- **#1508 diff shape**: only the named plan artefact; no
  architecture.org/ADR edits.

## 6. Stage prompts

### Prompt A.2 (stage 2 — IMPLEMENTATION)

```text
You are the stage-2 IMPLEMENTATION agent for chunk R9-A of round 9 ("ux-coverage adapter universe + small debt", branch ci/ux-coverage-adapter-universe) in the taskboard repo, worktree /home/viktoria/src/taskboard-WT-ci-ux-coverage-adapter-universe (recreate it on the branch if missing: git -C /home/viktoria/src/taskboard worktree add ../taskboard-WT-ci-ux-coverage-adapter-universe ci/ux-coverage-adapter-universe; if the branch's PR already landed, create ci/ux-coverage-adapter-universe-v2 off fetched origin/main instead per AGENTS §8 — never reuse a landed branch name).

**Long-command discipline** (binding — docs/agent_dispatch.org; restated verbatim): (1) *The coverage trio never runs inside a subagent*: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed). *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* **Every agent-launched cargo invocation** routes through `scripts/memory-gate.sh` (the 2026-11-23 systemd-oomd law) — nothing bare. (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) *Stage prompts restate this discipline verbatim.* (5) You do not run the full coverage suite or the §10 trio; the dispatcher owns them.

1. Read AGENTS.md, issues #1514 #1507 #1508 (`gh issue view` — bodies normative), and the stage-1 plan .artifacts/plans/2026-11-27-round9-chunkA-ux-coverage-adapter-universe-plan.md (§2 recorded reality, §3 decisions D1–D4, §4 verification, §5 invariants — all normative).
2. Implement exactly per the plan, TDD where code is involved (the new self-test case 27 is written first and fails pre-fix — run it against the pre-fix script to observe the #1514 red state, then fix), every commit standalone (rebase-merge hygiene):
   a. #1514 (D1+D2): scripts/ux-coverage.sh — new adapter universe builder per D1 (same prunes incl. FX_ROOT, same #1073 tier post-filter verbatim, predicate = `-path '*/adapter/*'` OR `-name 'adapter*.rs'`, `-type f -name '*.rs'`); line ~944 consumes it. New fixture tree scripts/fixtures/ux-coverage-fixture/tree-modular/ per D2 (copy of tree/ with adapter.rs moved to adapter/settings.rs, content unchanged). New self-test case 27 per D2: plan.md against tree-modular, asserts exit 0 + exactly 1 BOUND row + the wiring citation path contains adapter/settings.rs (structural path assertion, no prose). Write the case first, observe it FAIL pre-fix, then make it pass. Commit: `fix(ci): ux-coverage builds the adapter universe from the adapter/ module layout (#1514)` with a body naming the red-state evidence (W-1 MISSING on the theme plan; case 27 red pre-fix).
   b. #1507 (D3): root Cargo.toml — delete exactly lines 20–22 (readme/license/publish). Verify with `scripts/memory-gate.sh cargo clippy --workspace --all-targets` (background + poll; defer per the rule if contended): zero unused_workspace_package_fields warnings. Commit: `chore(workspace): drop unused workspace.package readme/license/publish fields (#1507)` with a body recording the dual-tier rationale (a single inherited license would mislabel the two AGPL crates).
   c. #1508 (D4): .artifacts/plans/2026-11-25-adapter-decomposition-plan.md §2 — the two one-line amendments exactly per D4. Commit: `docs(plan): record the E1 map deviations — menu-dismiss seam home + auto_list_backoff tests home (#1508)`.
3. Verification per the plan §4: (1) self-test exit 0, all 27 PASS with 1–26 verdicts unchanged; (2) the theme-plan gate exits 0, W-1 callback FOUND citing adapter/settings.rs, MISSING: 0, BOUND stays 14; (3) gated clippy zero unused_workspace_package_fields warnings; (4) fixture-pin self-tests 9/12/13/16 still PASS. The nextest workspace gate (background + poll or flock-defer per the rule) covers the Cargo.toml change; no coverage suite.
4. Diff-shape rules: #1514 touches only scripts/ux-coverage.sh + the new fixture tree + the self_test() function; #1507 touches only root Cargo.toml lines 20–22; #1508 touches only the named plan artefact. No other edits, no drive-bys, no dead code, no speculative generality (the other find_evidence callers keep their signatures).
5. Invariants that must not break (plan §5): self-tests 1–26 byte-stable in expectations; FX_ROOT pruning intact incl. nested-copy cases 9/12/13/16; the #1073 tier filter logic verbatim; adapter_match semantics untouched — the fix widens the universe, never the match rules; a genuinely absent cell stays MISSING.
6. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push / no PR" governs git branches and pull requests only, never issues; list the issue numbers in your report.

Report: the observed pre-fix red evidence for case 27 and the theme-plan W-1 row, the post-fix §4 verification outcomes (all four), per-commit SHAs, per-gate results, any deviation from the plan and why.
```

### Prompt A.3 (stage 3 — REVIEW)

```text
You are the stage-3 REVIEW agent for chunk R9-A of round 9 ("ux-coverage adapter universe + small debt", branch ci/ux-coverage-adapter-universe), worktree /home/viktoria/src/taskboard-WT-ci-ux-coverage-adapter-universe.

**Long-command discipline** (binding — docs/agent_dispatch.org; restated verbatim): (1) *The coverage trio never runs inside a subagent*: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed). *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* **Every agent-launched cargo invocation** routes through `scripts/memory-gate.sh` (the 2026-11-23 systemd-oomd law) — nothing bare. (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) *Stage prompts restate this discipline verbatim.* (5) You do not run the full coverage suite; the dispatcher owns it.

**Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line; (c) a single level of abstraction per function body. Findings are fixed on the branch.

1. Read AGENTS.md, issues #1514 #1507 #1508, the stage-1 plan .artifacts/plans/2026-11-27-round9-chunkA-ux-coverage-adapter-universe-plan.md, and the round-9 dispatch context; then begin by rebasing onto main — git fetch origin --prune, rebase the branch onto origin/main (drop already-merged duplicates; resolve only real conflicts; push --force-with-lease if history was rewritten) — and review the full branch diff versus the post-rebase base in the worktree.
2. Functional focus — this chunk's traps: (a) the new adapter universe predicate must not let the nested fixture trees back into evidence (self-tests 9/12/13/16 are the pins); (b) the #1073 tier post-filter must be applied to the new universe identically (a token present in both a production adapter file and a tests/-twin must cite the production file deterministically); (c) the existing flat-shape fixture tree/adapter.rs must still resolve (self-tests 1–26 expectations byte-stable — any edit to an existing case is a finding); (d) the match semantics in adapter_match are untouched — verify a diff that widened matching instead of the universe; (e) the theme-plan gate reads W-1 FOUND citing adapter/settings.rs with MISSING: 0 and BOUND still 14; (f) root Cargo.toml lost exactly the three lines and nothing else; the plan-artefact diff (#1508) is confined to the named file.
3. The code-quality mandate above applies in full; also re-run the plan §4 verification: bash scripts/ux-coverage.sh --self-test (exit 0, 27 cases PASS), bash scripts/ux-coverage.sh .artifacts/plans/2026-11-27-theme-switch-ux-plan.md --gate (exit 0), scripts/memory-gate.sh cargo clippy --workspace --all-targets (zero unused_workspace_package_fields warnings), and scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). Any failure is a finding.
4. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups; every commit stands alone).
5. Ship — and stop there: push the branch, open the PR against main with gh — title `fix(ci): ux-coverage adapter universe + workspace.package debt (#1514, #1507, #1508)`, body = a short summary of the three changes with the recorded red→green evidence, `Closes #1514`, `Closes #1507`, `Closes #1508`, and a link to the stage-1 plan — then report. The dispatcher owns `gh pr checks --watch`, merge-state verification, teardown, and at-merge issue closures; do not watch checks, do not merge, never touch main.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push / no PR" governs git branches and pull requests only, never issues; list the issue numbers in your report.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, and confirmation the branch is pushed. Your scope ends at PR-open.
```

## 7. Completion bookkeeping

- The PR closes #1514, #1507, #1508 on merge (dispatcher owns
  at-merge closures).
- Deliberately not dispatched here: any further ux-coverage
  coverage-matrix changes; any member-manifest metadata work
  beyond the three deleted fields; ADR/architecture amendments
  (none needed — the adapter layout change was E1's, already
  landed).
