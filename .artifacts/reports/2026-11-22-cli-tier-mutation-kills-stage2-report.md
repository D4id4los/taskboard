# Stage-2 report — C2, the CLI-tier mutation kills (round 5)

- **Date**: 2026-11-23
- **Branch**: `test/cli-tier-mutation-kills` (worktree
  `../taskboard-WT-test-cli-tier-mutation-kills`), plan
  `.artifacts/plans/2026-11-22-cli-tier-mutation-kills-plan.md`
  (committed `c1d87de7`). No push, no PR (handoff per dispatch).

## 1. Landed commits

| Commit | Content |
|---|---|
| `ae18f2c0` | #1230/#1175: the one-shot exit-code table (`tests/cli/cli_exit_codes.rs`, registered in `tests/cli_tasks.rs`) + `logout_command` widened `pub` (D3) |
| `61ff25b6` | #1229: the prose-family structural render pins (per-verb CLI files + config/status unit pins + the `OpenFailure` typed-mapping pin) |
| `62911ae6` | #1229 (C1 split): the `report_resurrections` writer seam (`sync_on` threads `&mut (dyn Write + Send)`, `io.out` at the session-lane call site, stdout at the one-shot) + the exactly-N-lines fold-order pin |
| `0665df67` | #1176: the `#[cfg(feature = "pdf-export")]` dispatch-leg pin in `cli_export_pdf.rs` |
| `66d5687d` | the repeat/track failure legs (found by W1, see §3) |
| `1e1246ab` | the S3 report addendum (survivor → pin mapping, W1/W2 records) |

## 2. Gate results

- Pre-commit hooks: green on every commit (flock-wrapped clippy etc.).
- `scripts/run-quality-gates.sh cargo nextest run --workspace`:
  **green — 5234 passed, 0 failed, 65 skipped** (63 s run, lock FREE
  at start). Full §10 suite remains the dispatcher's.
- `cargo fmt`/`cargo clippy --all-targets` clean on the worktree,
  including a `--features pdf-export` clippy pass.

## 3. Mutant-verification evidence

- **W1** (repeat/breaks/timers/reminders `--file` shard, 113 mutants,
  memory-gated, `--jobs 2`, filtered test targets after the baseline
  timeout taught the fixture lesson): 49 m — **65 caught, 11 missed,
  37 unviable**. Missed = 7 symmetry-preserving `== -> !=` renderer
  flips (→ S7 goldens per D5, on #1229), `repeat_command -> 0` (fixed
  in-round), 3 census rows owned elsewhere (C1 predicates /
  one-behavior-one-test).
- **W2** (repeat.rs re-window after `66d5687d`): 13 m — **10 caught,
  0 missed, 5 unviable**. The `-> 0` kill is verified.
- Families without a window (pdf #1268, age_phrase boundaries #1267,
  mod.rs/habits/track render slices): **expected-caught** per D9, pin
  locations in the S3 addendum; dispatcher schedules confirmation
  windows per R1.

## 4. Deviations from the plan

- Homes: the plan §4 suggested the per-verb files — followed; config
  cluster E and `output.rs` unit-tier per the plan; `age_phrase` pinned
  in a `commands/status.rs` unit module (the fn is private; the
  fixture infra lives there) rather than a CLI file.
- The exit-code table needed the disk-config fixture (the plan D2
  shape) plus a seeded-board step for the read verbs (the verbs
  resolve against the single-live-board default); repeat's success leg
  is the rule-less `Off` no-op receipt (D2's fork, recorded in the
  test doc).
- Logout's success leg uses the `file` credential-store kind (the
  env store honestly refuses delete — plan §9's recorded fork); the
  no-daemon nudge maps to the *clean* `None` arm, so no stderr
  warning is asserted (the plan's "warning on err sink" assumption was
  wrong for `NoDaemon`; the code-stays-0 pin is the D4 contract).
- `track status` on an empty view renders two lines (the
  nothing-running line + the day total) — the pin pins the contract,
  not "today's" formatting.

## 5. Backlog / issue numbers

- #1267 (age_phrase boundaries), #1268 (pdf sweep window): pre-filed
  at planning; no duplicates opened.
- #1229/#1230/#1175/#1176: landed-state evidence comments posted
  (2026-11-23).
- The symmetry-preserving flip family is dispositioned on #1229 to the
  S7 golden wave (#1141/#1140/#1139) — no separate issue per D5's
  clause.

## 6. Handoff

PR open + CI green is the dispatcher's (this stage: no push, no PR).
The worktree stays until the dispatcher confirms (chunks may resume;
S3 addendum §5 is the acceptance mapping).
