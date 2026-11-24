# The CI Structural Severing Guard — Chunk E1 Plan

- **Date**: 2026-11-24
- **Chunk**: Round 6 E1 (`ci/severing-guard`), issues **#1252** (the
  guard — this chunk's whole remainder) and **#1251** (AlarmWindow
  twin — **already closed on main** via PR #1354 / B1; verified
  2026-11-24: `ui/alarm-window.slint` carries the sever at line 50 and
  `alarm_window_max_probe` is wired through build.rs + probe
  composition + tests; the issue is CLOSED upstream). Stage 2 must
  **not** re-implement #1251; the plan records its landed state as the
  ninth inventory row.
- **Normative sources**: scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §E; dispatch plan
  `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
  §1 (E1 row) + §13; issue #1252 body (the ask, verbatim constraints:
  census-style, deny new unsevered roots, enumerate
  `ui/views/*-view.slint`, fail on missing sever or missing
  `window_max` probe twin); precedent commit `1276151a` (the sweep +
  per-view twins).

## 1. Open questions → recorded decisions

### D1 — Guard mechanism: a Rust census test in the `structural` inventory binary (not a script, not a CI lane)

**Decision.** The guard is a new member module
`crates/taskboard-ui-slint/tests/inventory/severing_census.rs`,
declared in `tests/structural.rs` alongside its siblings. It runs in
the standard `cargo nextest run --workspace` lane — no new CI lane,
no standalone script. Rationale:

- The `structural` binary exists precisely for "pure source- and
  data-shape assertions with no platform init" (its own header, the
  #1070 grouping); a census is its canonical member.
- `tests/inventory/view_mount_inventory.rs` already solves the hard
  half: the `Blanked` scanner (comment/string blanking + brace-depth
  tracking) makes structural source scanning robust against
  `//`-in-strings and interpolated labels. The guard reuses that
  pattern (same technique, module-local copy — `Blanked` is private;
  if extraction is cleaner, promote it to a shared
  `inventory/source_scan.rs` module consumed by both; either is
  acceptable, decide at stage 2 by diff size).
- A shell script in CI would be a second harness, second failure
  grammar, and invisible to local pre-commit runs. A nextest member
  fails the same PR CI the hooks already gate.

### D2 — Structural, not textual ([L](a) compliance)

**Decision.** The guard never asserts prose. Its assertions are
**set-shaped**, computed structurally:

- **Enumeration**: `std::fs::read_dir` over
  `<CARGO_MANIFEST_DIR>/ui/views/*.slint` at test runtime — the
  inventory is derived from the directory, never hardcoded, so a
  *new* view file is automatically in census.
- **Root classification**: for each file, find the exported root
  component (the `export component <Name> inherits …` whose block the
  brace-depth scanner closes last at depth 0) — structurally, on the
  blanked source. Files whose exported components are *not* view
  roots are classified via an explicit **exemption table** (same
  honesty rule as `view_mount_inventory`: each exemption names the
  file, the component, and a witness reason — see the inventory in
  §2).
- **Sever check**: within the root component's block, at the root
  element's property level, the scanner requires the property binding
  to `Theme.layout-severing-max-width` (one structural fact: "the
  root element sets max-width to the Theme severing constant"). This
  is markup-structure matching — the thing the class regresses is a
  missing line of markup, exactly what `view_mount_inventory`'s
  header calls out as "which a source assertion pins exactly". No
  error-message text, no log text, no label text is asserted.
- **Probe-twin check**: derive the twin name from the file stem
  (`<stem-with-dashes-as-underscores>_window_max_probe`) and require,
  all structurally (existence checks, not text):
  1. `tests/<twin>.slint` exists;
  2. build.rs's `probes` array contains the twin name;
  3. `tests/probes/<twin>_tests.rs` exists (the probe is asserted,
     not just compiled).
- **Staleness direction (reverse census)**: every name in build.rs's
  array matching `*_window_max_probe` must map back to a census root
  or an exemption — a deleted view with a leftover twin also fails.
- **Failure output**: the test collects all violations into a
  `Vec<SeveringViolation>` (typed enum: `MissingSever { file }`,
  `MissingProbeTwin { file, expected_probe }`,
  `StaleProbeTwin { probe }`, `UnclassifiedExport { file, component }`)
  and asserts the vec is empty (`assert!(violations.is_empty(), …)`)
  — a typed/structural outcome; the Debug print names the offending
  roots for the fixer.

### D3 — The task-view twin's name (`window_max_probe` breaks uniform derivation)

**Decision**: rename `tests/window_max_probe.slint` →
`tests/task_window_max_probe.slint` and its test module
`window_max_probe_tests.rs` → `task_window_max_probe_tests.rs`
(build.rs array entry + `probe_composition.rs` path follow). This
makes D2's derivation uniform (`task-view.slint` →
`task_window_max_probe`) with no special case in the guard. Diff is
two `git mv`s + two reference updates; the alternative (a one-entry
alias table in the census) leaves a permanent wart in the law the
guard exists to enforce. Note: `window_max_probe` currently compiles
TaskView — verify at stage 2 that its .slint body references the
TaskView component and nothing else imports the old file name
(grep before the move).

### D4 — The negative case ([L](c): a guard that can't fail is decoration)

**Decision.** Two negative tests, both in the same inventory binary,
no repo mutation:

1. **Helper-level fixtures (the real negative case)**: the census's
   classification + checking core takes *sources*, not paths —
   `fn census(files: &[(String, String)], build_rs: &str) ->
   Vec<SeveringViolation>` (file-name, source-text pairs). The
   negative tests feed synthetic fixtures: an unsevered root →
   `MissingSever`; a severed root with no twin → `MissingProbeTwin`;
   build_rs carrying a twin for a nonexistent file → `StaleProbeTwin`;
   an exported non-root component without an exemption →
   `UnclassifiedExport`. Each asserts the exact typed variant
   (`matches!`), never text. The real-tree test is then a three-line
   call with the actual directory + build.rs asserting an empty vec.
2. **Honesty rule for the exemption table**: every exemption must
   match a live file + exported component in the real tree (the
   `view_mount_inventory` "no stale exemptions" rule) — so an
   exemption abuse (exempting a real view root to silence the guard)
   fails too.

Two-way evidence for the plan record: with the sever line deleted
from one view in a scratch checkout, `severing_census` fails with
`MissingSever` — stage 2 records the observation in the test module
header (the 1276151a evidence pattern) without committing the
mutation.

### D5 — Which roots are in census (the complete inventory)

**Decision**: the census covers **every `.slint` file under
`ui/views/`**; every exported component in a covered file is either a
severing root (carries sever + twin) or a named exemption. The
top-level `ui/*.slint` windows are *out of boundary* for this chunk
(see §3 / the backlog issue) except as recorded fact.

## 2. The inventory table (verified against origin/main, 2026-11-24)

| File (`ui/views/`) | Exported root | Inherits | Sever present | Probe twin (build.rs) | Census class |
|---|---|---|---|---|---|
| `today-view.slint` | `TodayView` | `FocusScope` | yes | `today_window_max_probe` | root |
| `board-view.slint` | `BoardView` | `FocusScope` | yes | `board_window_max_probe` | root |
| `stack-view.slint` | `StackView` | `FocusScope` | yes | `stack_window_max_probe` | root |
| `task-view.slint` | `TaskView` | `FocusScope` | yes | `window_max_probe` (→ renamed `task_window_max_probe`, D3) | root |
| `review-view.slint` | `ReviewView` | `FocusScope` | yes | `review_window_max_probe` | root |
| `settings-view.slint` | `SettingsView` | `FocusScope` | yes | `settings_window_max_probe` | root |
| `habit-history-view.slint` | `HabitHistoryView` | `Rectangle` | yes | `habit_history_window_max_probe` | root |
| `plan-next-day-view.slint` | `PlanNextDayView` | `FocusScope` | yes | `plan_window_max_probe` | root |
| `review-walk-overlay.slint` | `ReviewWalkOverlay` (+ `VerbButton`) | `Rectangle` | no | — | **exemption** — a hosted dialog body: the sizing law lives in `ModalDialogHost` (the host owns the scrim, the absorbing panel root, the sizing law — the file's own header). Its embedded `PlanNextDayView` mount is itself a severed census root. `VerbButton` is a private widget, out of boundary. Exemption names file + component + this witness (D4's honesty rule). |

Recorded fact (not this chunk's work): `ui/alarm-window.slint`
(`AlarmWindow inherits Window`) carries the sever (#1251, B1/PR
#1354) and `alarm_window_max_probe` — outside the `ui/views/`
boundary, listed so the census's doc header can point at it.
`ui/app-window.slint` (`AppWindow inherits Window`) carries **no**
sever today — see §3's backlog finding.

## 3. Out-of-scope findings → backlog issues (filed per [L](d))

- **`AppWindow` itself carries no layout-severing max-width** — the
  one `Window` root outside the census boundary with no sever. It may
  be structurally safe (the main window's sizing is composition-law
  driven), but it is the same collapse *class*; #1252's scope is
  `ui/views` only. Filed as a `backlog:` issue (number in the stage
  report) proposing either a witnessed exemption-grade analysis or
  the sever+probe treatment on its own chunk.
- *(no other findings — the views tree matched the expected shape
  exactly; #1251 verified closed, no comment needed beyond the
  at-merge closure the dispatcher owns for #1252.)*

## 4. Stage-2 work breakdown (S/M — estimated diff)

| Step | Files | Size |
|---|---|---|
| 1. D3 rename: task twin → `task_window_max_probe` | `tests/window_max_probe.slint` (mv), `tests/probes/window_max_probe_tests.rs` (mv), `build.rs`, `tests/probe_composition.rs` | ~10 lines |
| 2. Census core: blanked scanner (reuse or promote `Blanked`), classification, `SeveringViolation`, `census()` over (name, source) pairs | `tests/inventory/severing_census.rs` (+ optional `tests/inventory/source_scan.rs`) | ~250–350 lines |
| 3. Negative-case tests (D4 fixtures + honesty rule) | same module, `#[test]`s | ~120–180 lines |
| 4. Real-tree census test: read_dir over `ui/views/`, read build.rs, assert empty violations | same module | ~40 lines |
| 5. Declare module in `structural.rs`; two-way evidence note in the module header | `tests/structural.rs` | ~15 lines |

Total: **~450–600 lines, all under `crates/taskboard-ui-slint/tests/`**
— zero production-code changes (the rename touches build.rs, which is
build infra, not behavior).

## 5. Acceptance checklist (stage-3 gate)

- [ ] The census enumerates `ui/views/*.slint` from the filesystem
      (a new file appears in census without touching the test).
- [ ] Every §2 root row passes; the review-walk-overlay exemption
      carries its witnessed reason and passes the honesty rule.
- [ ] The negative cases fail with the exact typed variants (D4).
- [ ] With a sever line scratched out in a scratch checkout, the
      real-tree test fails (evidence recorded, not committed).
- [ ] `cargo fmt`/`clippy -D warnings`/`nextest` green via
      `scripts/run-quality-gates.sh`; every cargo invocation through
      `scripts/memory-gate.sh`.
- [ ] No text assertions anywhere in the new tests (typed verdicts
      only).

## 6. Risks

- **Scanner fidelity**: `Blanked` handles comments/strings; nested
  component blocks inside a root (e.g. `SettingsView`'s 600+ lines)
  mean the sever check must anchor to the *root element's* property
  block, not any descendant. Mitigation: the census scans the root
  component's top-level property region (depth 1 relative to the
  component block) — a descendant's `max-width` does not satisfy the
  census, which is the correct semantics (the #1197 class was exactly
  a sever at the wrong depth).
- **`Theme.layout-severing-max-width` renames**: the census constant
  reference is one string in the guard; a Theme rename breaks the
  guard loudly (fail-closed), which is acceptable — the fixer updates
  one line.
- **build.rs parse**: the probe array is parsed from build.rs source
  (stable literal array since the file's current shape); if build.rs
  restructures, the guard fails closed (`StaleProbeTwin` noise) —
  cheap to re-anchor.
