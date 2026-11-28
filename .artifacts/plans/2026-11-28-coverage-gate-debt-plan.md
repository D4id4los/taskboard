# Coverage-gate debt — stage-1 plan (#1577, #1575, #1583)

- **Date**: 2026-11-28
- **Kind**: Implementation plan (stage 1+2 compressed chunk).
- **Origin**: dispatcher chunk "coverage-gate-debt"; issues #1577, #1575, #1583.
- **Status**: ACCEPTED (executed in this branch).

## 1. Scope

Three small test/tooling-debt items, one commit per issue on branch
`test/coverage-gate-debt` (worktree
`/home/viktoria/src/taskboard-WT-test-coverage-gate-debt`, base 97490154):

1. **#1577** — `scripts/ux-coverage.sh`'s `adapter_match` lacks the #1282
   separator widening: a kebab-authored §6 Callback cell
   (`habits-undo-log-clicked`, the template spelling) never resolves
   against the snake adapter seam (`on_habits_undo_log_clicked`) or the
   snake-keyed `seam_inventory.rs` manifest, so a genuinely wired row
   reads MISSING (observed on #1519 chunk 5's W-8).
2. **#1575** — the keymap-parity journey plan
   `.artifacts/plans/2026-11-16-keymap-parity-ux-plan.md` has 14 MISSING +
   4 AMBIGUOUS rows: its §4/§6 test nominations went stale in round 9's
   test-suite reorganisation. Re-point each stale nomination at the test
   that now covers the row; leave anything uncovered honestly MISSING and
   file it.
3. **#1583** — fold the stalled-line tty test's `NotifyThenDelay` notify
   half onto a plain delayed responder (after #1584/#697 Case 2 no longer
   consumes the notify); the notify-bearing variant stays for its still-
   valid users (the one-shot sibling test at `cli_tty.rs:1949` and
   `tests/services/ipc.rs`).

## 2. #1577 — adapter_match separator widening

- **Red**: two new self-test fixtures mirroring the W-8 shape:
  - `plan-kebabcb.md`: the W-K Callback cell authored **kebab**
    (`habit-edit-asked`) against the existing tree (kebab slint callback,
    snake `on_habit_edit_asked`, snake-keyed inventory) → today MISSING,
    must become FOUND (exit 0).
  - `plan-kebabcb-miss.md`: cell `habit-edit-probe` (kebab) with a kebab
    slint callback present but no adapter seam whose normalised name
    matches → must stay MISSING (exit 1).
  - Fixture-tree edit: add `callback habit-edit-probe();` to both
    `tree/ui/fixture-view.slint` and `tree-modular/ui/fixture-view.slint`
    (after the existing kebab callback, so no citation line number
    shifts) — this makes the negative case exercise the *adapter*
    widening specifically (the slint side resolves; the adapter must not
    fabricate).
- **Green**: widen `adapter_match`'s two grep patterns exactly as
  `tok_match` does (`${cb//_/[-_]}`, boundary-anchored, first-hit) and
  widen the #760 inventory cross-check's quote-delimited grep the same
  way. Letters stay load-bearing; only separator drift is forgiven.
- **Stability pin**: the full self-test matrix (cases 1–28) must keep
  byte-identical verdicts; verified before/after by diffing
  `--self-test` output. New cases 29 (kebab cell → snake seam resolves,
  exit 0) and 30 (the negative) pin the behaviour.

## 3. #1575 — keymap-parity plan re-pointing (verified against the tests)

Investigation result (each replacement read in the tree before
nomination):

| Row | Old nomination (dead) | New nomination (verified) |
|---|---|---|
| US-1/S1 (W-1) | `journeys.rs::us1_s1_emacs_motion_rows_step_the_walks` (retired by round-9 directional focus, supersession note journeys.rs:211) | `crates/taskboard-ui-slint/tests/journeys/focus_law_journeys.rs::us1_s1_taskdetail_directional_rows_walk_the_ladder_subject_pinned` + `journeys.rs::us1_ac2_settings_control_walk_rows_step` |
| US-2/S1 (W-2) | `journeys.rs::us2_s1_menu_first_last_matches_the_key_route` | `bound-test: US-2/S1 -> journeys.rs::the_first_last_jumps_land_on_the_far_ends_and_stay` (journeys.rs:5963) + the key-route half stays covered by the partner test below |
| US-2/S2 (W-2) | bare `us2_s2` (AMBIGUOUS) | `bound-test: US-2/S2 -> journeys.rs::the_punctuation_partner_matches_only_the_held_shift` (journeys.rs:5908 — bare Alt+, dispatches nothing, the shifted press fires the first/last jumps: the normalization + fall-through + key-route oracle in one) |
| US-3/S2 (W-3) | bare `us3_s2` (AMBIGUOUS) | `journeys.rs::us3_s2_description_shortcut_arms_the_editor` (journeys.rs:20888) |
| US-3/S3 (W-3) | bare `us3_s3` (AMBIGUOUS) | `journeys.rs::us3_s3_clock_shortcuts_route_the_exact_commands` (journeys.rs:20937) |
| US-3/S4 (W-3) | bare `us3_s4` (AMBIGUOUS) | `journeys.rs::us3_s4_rule_add_shortcut_arms_the_funnel` (journeys.rs:20987) |

Edits are cell/line-local only (the PR #1580 amendment also touches this
plan — this chunk must merge before or after #1580 with a rebase; no
restructuring). The capability-matrix and census MISSING rows inherit
FOUND once the cited stories' steps resolve. §8's dead journey-test list
is re-pointed to the same live names.

## 4. #1583 — plain delayed responder for the stalled-line test

- New `DelayedBoards` responder (delay + boards payload, no notify half)
  beside `NotifyThenDelay` in `crates/taskboard-app/tests/cli/cli_tty.rs`;
  Case 2's mount switches to it; the `(tx, _rx)` channel and the stale
  "notify half deliberately unused" prose go.
- Unchanged users (semantics preserved): the one-shot sibling
  `tty_second_sigterm_during_the_drain_forces_143…` (cli_tty.rs:1949,
  `rx` is the in-flight gate) and `tests/services/ipc.rs`'s tokio-native
  variant. Reliability: the `cli_output` target run ≥5× through the
  memory gate.

## 5. Gates & handoff

- Per-commit pre-commit hooks (fmt + clippy through the memory gate).
- Fast gate: `scripts/run-quality-gates.sh cargo nextest run --workspace`.
- No push, no PR, no coverage trio from this stage. The stage-3 prompt
  (appended below after implementation) restates the long-command
  discipline verbatim.

## 6. Stage-3 REVIEW prompt (verbatim dispatch)

You are the STAGE 3 (REVIEW) agent for the chunk "coverage-gate debt"
in the taskboard repo (GitHub: D4id4los/taskboard). The implementation
stage landed on branch `test/coverage-gate-debt` (worktree
`/home/viktoria/src/taskboard-WT-test-coverage-gate-debt`; commits
92f40da0 docs(plan), 488208cf fix(ci) #1577, 9503758c docs(plan)
#1575, plus the #1583 commit on top). Review it and ship it.

**Discipline (binding — docs/agent_dispatch.org)**: A subagent is killed after ~600s without observable activity. **Long-command discipline**: (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines** (three PRs were blocked tonight). Out-of-scope findings become `backlog:` GitHub issues (or comments) immediately (AGENTS §7; search first).

Verify, in order:

1. **#1577 red→green**: check out the base commit 92f40da0's script
   (`git show 92f40da0:scripts/ux-coverage.sh > /var/tmp/uxcov-before.sh`,
   run it with `--self-test` AFTER temporarily placing the landed
   fixtures, or simpler: `git stash`-free — run the landed tree's
   self-test with the script file replaced by the pre-fix version and
   confirm self-test 29 FAILS (the kebab cell reads MISSING) while
   1–28, 30 pass; restore the landed script and confirm all 30 PASS.
2. **1–28 byte-stability**: `bash scripts/ux-coverage.sh --self-test`
   on the landed tree vs the base tree; the verdict lines for cases
   1–28 must be byte-identical; 29/30 are the new PASSes.
3. **The negative case**: self-test 30 — the `habit-edit-probe` kebab
   cell resolves the kebab slint callback but must stay MISSING
   (adapter seam differs by letters); read the fixture and the widened
   `adapter_match`/inventory greps (sentinel two-pass, letters
   load-bearing, first-hit, boundary-anchored).
4. **#1575 nominations**: for each re-pointed row in
   `.artifacts/plans/2026-11-16-keymap-parity-ux-plan.md`, open the
   nominated test body and confirm it exercises the row:
   `journeys/focus_law_journeys.rs::us1_s1_taskdetail_directional_rows_walk_the_ladder_subject_pinned`
   (TaskDetail directional rows, subject pinned — the dated supersession
   note sits at journeys.rs above `us1_ac2_settings_control_walk_rows_step`),
   `journeys.rs::us1_ac2_settings_control_walk_rows_step` (Settings,
   all four emacs chords, zero commands),
   `journeys.rs::the_first_last_jumps_land_on_the_far_ends_and_stay`
   (journeys.rs:5963), `journeys.rs::the_punctuation_partner_matches_only_the_held_shift`
   (journeys.rs:5908), and the four `us3_s*` shortcut journeys
   (journeys.rs:20813/20888/20937/20987). Re-run
   `scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --gate`
   and confirm 0 MISSING, 0 AMBIGUOUS. Confirm the edits are cell/line
   local (PR #1580 amends the same plan — the PR body must state this
   PR merges before or after #1580 with a rebase).
5. **#1583**: `DelayedBoards` carries no notify half; Case 2
   (`tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`)
   mounts it; the notify-bearing `NotifyThenDelay` remains only for the
   one-shot sibling (`tty_second_sigterm_during_the_drain_forces_143…`,
   which blocks on `rx`) and `tests/services/ipc.rs` (tokio-native
   `Notify`) — their semantics unchanged. Confirm ≥5 green
   `scripts/memory-gate.sh cargo nextest run -p taskboard-app --test
   cli_output` runs exist for the landed tree (rerun once yourself,
   backgrounded).
6. **Ship**: commit any review fixes on the branch (hooks on, no
   bypasses); then push `test/coverage-gate-debt` and open the PR to
   `main` titled `test(coverage): the ux-coverage gate + plan nominations + tty test debt (#1577 #1575 #1583)`,
   body summarizing the three fixes, the self-test matrix before/after
   (28→30 cases, 1–28 byte-identical), the keymap gate before→after
   (14 MISSING + 4 AMBIGUOUS → 0 + 0), the re-pointing table, the
   cli_output reliability runs, the #1580/#1581 merge-order notes
   (#1581's W-8 flips FOUND when rebased after this merges), and
   `Closes #1577, Closes #1575, Closes #1583`. Report the PR URL and
   stop — the dispatcher owns CI watch and teardown.
