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
