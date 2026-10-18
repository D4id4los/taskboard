# [Report] Chunk 14 stage-3 review — release tooling & launch (`fix/release-tooling-launch`)

- **Date**: 2026-10-18
- **Verdict**: **PASS** — two findings, both fixed on the branch (amend
  + autosquash, no duplicate-subject fixups); full gates re-run green on
  the final rebased head; PR open and green; never merged — the user
  merges on GitHub.
- **PR**: https://github.com/D4id4los/taskboard/pull/332 (rebased onto
  `origin/main` @ `404fcb4` — chunks 11–13 and three docs commits had
  landed; rebase clean, CHANGELOG merged with exactly the #236 bullet as
  delta).
- **Closes**: #235, #238, #236 (via the PR body); #237 got the plan
  §4.5 supersession comment
  (https://github.com/D4id4los/taskboard/issues/237#issuecomment-6248524208).

## 1. Functional review (dispatch prompt 14.3 focus points)

- **Release cut vs branch protection (walk-through)**: clean-tree check
  (via `git status --porcelain`, so untracked files count) → fetch
  `--prune` → `origin/main` verified → remote-tag check *before* the
  local-tag check (fetch imports remote tags, so the refusal names the
  right place) → release-branch existence checked both locally and
  remotely → `chore/release-v<X.Y.Z>` cut off fetched `origin/main`
  (never off HEAD) → bump / lockfile refresh / changelog promote /
  commit on the branch → push the *branch* (no `main` write anywhere)
  → `gh pr create`. The tag is never created by the script: the printed
  human steps (`git fetch origin --prune; git tag -a vX.Y.Z origin/main;
  git push origin vX.Y.Z`) run *after* the merge, so the tag names the
  rebased post-merge commit and triggers `release.yml` whose validate
  job then sees tag version == workspace version. Every script step
  succeeds for any user with repo push access; the only writes to
  `main` are the human's GitHub merge and the human's tag push. AGENTS
  §9's contract is preserved in shape: clean tree ✓, bump ✓, commit ✓
  (on the release branch), tag ✓ (explicit human step, moved because
  its target SHA only exists post-merge).
- **Whole-Windows-leg bash-ism scan (not just the fixed line)**: only
  the `build` job runs on the `windows-latest` matrix. Its executable
  `run:` steps: the MCP guard (now `shell: bash`), "Compute Archive
  Name" (already `shell: bash`), "Package Release"/"Verify Package"
  (plain `python` invocations — shell-neutral). The apt step carries
  `if: runner.os == 'Linux'` and never executes there. The bash-only
  constructs elsewhere in the file (`[[ =~ ]]`, `${TAG#v}`,
  `<(...)`-process substitution, `!=` tests) all live in the
  `ubuntu-latest`-only `validate`/`publish` jobs. Nothing to fix.
- **Detection-rule truth table (#236)**: the unit test enumerates the
  full 4×3 table — `desktop` display/TTY-blind → `DesktopUi` (4 cells);
  `daemon`+TTY → `Repl` under both display values (ADR 0011
  unchanged); `daemon`+no-TTY+display → `DesktopUi` (the fix);
  `daemon`+no-TTY+no-display → `DaemonLifecycle`; `kiosk` →
  `DaemonLifecycle` in all four tty×display cells. `display_available`
  has its own decision-table test (either var non-empty; empty counts
  as absent). Regression check against #237: explicit `daemon`
  subcommands resolve through the IPC lanes above and never reach
  `bare_invocation_route` (`main.rs`), so the ssh/maintenance path
  stays headless regardless of display; the kiosk mode gate inside the
  daemon lifecycle is byte-untouched. Match-arm order makes the
  fall-through exhaustive: no silent cell.

## 2. Findings & fixes (both fixed on the branch)

1. **Comment drift at the changed call site** (`main.rs`, commit 3):
   the routing block's lead comment still said "a non-TTY daemon …
   keeps the daemon lifecycle" — contradicted one line below by the new
   display split. Rewritten to the three-way rule (TTY → REPL, no-TTY +
   display → GUI, neither → daemon; kiosk refused in every cell);
   amended into the commit.
2. **Plan §2.1 deviation, unrecorded** (`bump_version.sh`, commit 1):
   the plan specifies the release-PR body as "the promoted CHANGELOG
   section + the post-merge tag checklist"; stage 2 shipped a summary
   line + checklist, dropping the notes section the human reviews when
   merging. Fixed via `scripts/changelog.py extract` (the same tool
   `release.yml` uses) with a graceful fallback line when promotion was
   skipped; autosquashed into commit 1 and re-exercised.

Non-findings verified along the way: the `--fill` hint in the printed
manual commands is safe (gh's documented precedence: explicit
`--title`/`--body` override `--fill`); the latent bump-regex and
clean-tree bugs stage 2 found in the original script are genuinely
fixed (`^(version = ")<ver>"$` matches the only `^version` line in the
root manifest; porcelain-based check).

## 3. Re-run validation (stage 3, on the final rebased head)

- **Gates (AGENTS §10)**: fmt ✓; clippy `--workspace --all-targets -D
  warnings` ✓; nextest 1023 passed / 28 skipped ✓; doctests 6 suites ok
  ✓; `cargo deny` (advisories/bans/licenses/sources) ✓; coverage triple
  ✓ (instrumented run + `coverage-quarantine-profraw.sh` + lcov report;
  quarantine did its job on torn profraws, merge succeeded).
- **Script exercise** (fresh temp harness: local bare origin, seed
  workspace, argv-recording `gh` shim): dirty-tree refusal, local-tag
  refusal, `--no-push` happy path from a *non-main* HEAD (branch cut
  off `origin/main`, version bumped, lockfile refreshed, `[0.1.1]`
  promoted, `[Unreleased]` heading kept, nothing pushed, zero `gh`
  calls, manual commands + human tag steps printed), push path (branch
  on the origin; shim recorded `gh pr create --title
  chore(release): v0.1.1` with the promoted-section text *and* the tag
  checklist in the body — the finding-2 fix proven end-to-end).
- **Workflow**: actionlint 1.7.7 (fresh download) exit 0 over the repo;
  PyYAML parse of `release.yml` + `ci.yml`; structural assertion that
  every `run:` executable on the `windows-latest` leg is bash-pinned or
  shell-neutral (first assertion flagged the apt step — a harness false
  positive, the step is `runner.os == 'Linux'`-gated; refined and
  green).
- **Honesty clause**: the pwsh leg remains unproven until the next real
  tag run; the `workflow_dispatch` `dry_run` option was **not**
  exercised (see §5).

## 4. Code-quality mandate

- Naming/comment discipline: comments state constraints (e.g. why the
  remote-tag check precedes the local one; why the wrapper is `pub`;
  why the gate is env-based and pre-mount), not narration. The two
  drift cases found were exactly violations of this rule — fixed.
- SRP / crate boundaries: no boundary touched — `taskboard-app`
  routing stays in `taskboard-app`; the script/workflow changes are
  tooling, not runtime code; no new dependencies.
- Single level of abstraction: `bare_invocation_route` stays a pure
  match over typed inputs; `display_env_available` is the only env
  glue and is deliberately untested per the main-glue convention
  (documented); the script's ship section composes named operations
  (`extract` → compose body → `gh pr create`).
- Test quality per AGENTS §5: typed `BareRoute`/bool asserts, no text
  assertions, no sleeps, no ceilings, one behavior per test at the
  lowest layer; the existing `repl::entry_point` decision table is
  untouched (its tests still pass).

## 5. Deviations & residual risk (for the record)

- The optional `workflow_dispatch` `dry_run` (plan §3.3 item 4) was not
  dispatched: normal PR CI coverage plus the mechanical battery were
  judged sufficient evidence for a workflow whose remaining risk is
  exactly one pinned line, and the plan makes it evidence, not a gate.
  The first real tag run (the §3.4 re-cut) remains the acceptance
  event.
- The v0.1.0 re-cut is documented in the PR body for the user
  (destructive, human-only, valid only while `main` is still 0.1.0).
- Stage-2's exercise harness had two self-inflicted issues (a
  pipefail-vs-expected-failure grep and a targetless seed manifest);
  both were harness bugs, re-verified correct in this stage's harness.
