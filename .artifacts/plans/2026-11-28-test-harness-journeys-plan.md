# Test harness: plain cargo, key events, fake engine — Chunk Plan

- **Date**: 2026-11-28
- **Kind**: stage-1 (planning) artefact for the chunk "test harness: plain
  cargo, key events, fake engine" (issues #1016, #1216, #1219, #1259,
  #770). No code in the planning commit.
- **Origin issues**: D4id4los/taskboard #1016 (p2 testing), #1216 (p3
  testing/dx), #1219 (testing), #1259 (p3 testing/ui), #770 (p3
  testing/ui). Issue bodies + comments read in full on 2026-11-28 at
  `origin/main` cc25ec7a.
- **Status**: READY. #1016's headline premise and both sibling findings
  are **stale on current main** (measured, §1); the actionable residue
  is the driver-worker panic-isolation seam. #1216 is docs-resolved on
  main; the plan records it and recommends a tracker comment, no code.
  #1219, #770 are live and small. #1259 is live and the only item that
  may touch production code.
- **Worktree**: `/home/viktoria/src/taskboard-WT-test-journeys-plain-cargo`
  (branch `test/journeys-plain-cargo`, off `origin/main` cc25ec7a).

## 1. Recorded reality (measured 2026-11-28, cc25ec7a)

### #1016 — plain `cargo test --test journeys` fails wholesale

**Stale.** Measured on this worktree (plain `cargo test`, no env vars,
no nextest):

- `scripts/memory-gate.sh cargo test -p taskboard-ui-slint --test journeys`
  → **444 passed; 0 failed** (54.39 s).
- `scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journeys`
  → **444 passed; 0 skipped** (parity confirmed).
- `scripts/memory-gate.sh cargo test -p taskboard-app --test app_services`
  → **84 passed; 0 failed** — the #1016 comment-1 sibling findings
  (`ipc::j1_daemon_stop_answers_while_a_verb_is_in_flight`,
  `core boots: Credential(NotFound)`, `tests/services/ipc.rs:68`; the
  two `ui_settings` members) are **also stale**: the whole merged
  `app_services` binary passes plain `cargo test` wholesale.

Why the premise aged out (all landed on main):

- The env seam is gone: `TASKBOARD_JOURNEYS_STAY_ON_TODAY` was replaced
  by the explicit per-mount `Harness::mount_no_escape`
  (`tests/support/harness.rs:124`) and `run_journey_no_escape`
  (`harness.rs:407`) — the B4a fix from #1016 comment-2 is merged.
- `DriverOptions` is a per-process `OnceLock` that asserts with a typed
  message (`tests/support/driver.rs:511-517`), and the merged-binary
  consolidation (#1070) landed: `tests/journey_suites.rs` hosts 30
  journey members behind one `mod support` and one root
  `HEADLESS_RENDERER` const (`journey_suites.rs:19-24`),
  `tests/journeys.rs` standardizes on `HEADLESS_RENDERER: true`
  (`journeys.rs:15-18`). The per-member `mod support` copies that
  multiplied the platform `OnceLock`s are gone.

**Live residue** (from #1016 comment-2, still true on main):

- R1: the driver worker's job loop does not catch panics —
  `driver::run` (`tests/support/driver.rs:506`) spawns one worker
  thread whose loop dispatches `Box<dyn FnOnce()>` jobs bare; a
  panicking job kills the worker thread, so every sibling test in the
  process cascades into `SendError`/`RecvError` instead of failing only
  the offending test. This is the one mechanism by which plain
  `cargo test` can still fail *wholesale* on this binary — exactly
  #1016's headline harm, minus the env/mode mix. The doc comment even
  declares it: "Panics if the job panics" (`driver.rs:~502`).
- R2 (recorded, out of scope here): `tests/journeys.rs` is a 24,707-line
  monolith beside the merged `journey_suites.rs` binary — filed as a
  separate `backlog:` issue at stage 1 (see §8).

### #1216 — raw key events dropped after a modal grab fails

**Resolved on main by the docs leg; issue remains as tracker.** The
"journey key routing" rule is in `docs/testing_strategy.org:211-215`
(raw `press_stroke` closing nothing after a modal mount = the grab did
not survive the headless pass; route the binding's typed action via
`dispatch_action`, assert the typed outcome), and the source comment
sits on the trait method itself
(`tests/support/driver.rs:85-95`, `press_stroke` impl at `:237`).
Issue comment confirms option (a) — a driver-level fallback for
raw-stroke-after-modal — "stays open here as the tracker; if a future
journey genuinely needs raw-stroke-after-modal, the driver-level
fallback reopens with that journey named."

Per AGENTS §5 ("no pins of today's behavior"), a lowest-layer test
pinning "a raw stroke after a modal mount lands nowhere" would pin a
limitation as contract. The correct posture is: no code, no such test;
comment on the issue recording that the docs leg is verified on main.

### #1219 — FakeEngine undo continuity is single-level

**Live, and worse than filed.** `tests/support/fake_engine.rs` keeps
five parallel per-face stacks — `removed_rules` (`:32`),
`added_rules` (`:37`), `restored_entities` (`:41`),
`cleared_batches` (`:44`), `habit_sections` (`:51`) — and the single
`StateCommand::Undo` arm (`:536-573`) pops **all families in one Undo
pass** (the cleared batch, then the restore face, then the first
non-empty rule/section stack, priority-ordered). So one `Undo` can
revert several operations at once and never walks back more than one
level per family. The #1219 body plus its comment (the D1i chunk added
`RestoreLabel`/`RestoreHabit`, `ClearReminderRules`, and the shared
`Undo` arm) describe exactly this; the ledger generalization is the
named fix.

### #1259 — detail-focus widget-inventory row 12 (timer entry) has no journey

**Live.** The inventory row exists
(`.artifacts/plans/2026-11-22-task-edit-view-focus-plan.md` §4, row 12:
"Timer entry buttons (track form) | timer setter (`TimerSetter`) |
stolen (red) | the button's own action | the timer-entry cell | new
(timer)") but no journey lands it. The blocker is recorded in the
issue: a stage-3 attempt failed structurally — once the
`TrackEntries` overlay is open, the control walk never lands on "Set a
timer" (`focused_label` returns `None`; the walk does not traverse the
overlay), and every existing "Set a timer" test reaches it via
`click_by_label` (`tests/journeys/focus_coverage.rs:589`). The proven
idiom to replicate is the detail's mask/walk inventory:
`tests/journeys/detail_focus_journeys.rs` `walk_to` (`:63`) polls
`focused_label` against a named stop address with a deadline panic.

### #770 — habit-create form-local echo steps untested

**Live, polish-grade.** The origin plan's §6 wires US-1/S1 via the
bound-test `lib.rs::habit_create_toggles_and_escape_disarms_without_shadowing`
(src `lib.rs:4228`; W-1 row, "bound 2026-11-13, PC-3" — retro cohort,
so `bound-test:` lines are the sanctioned mapping) and US-1/S5 through
the commit path; US-1/S2 (the title draft echoes in the field, commits
on Return) and US-1/S3 (the `SegmentedControl` target pick shows via
selected-index) have no direct tests — confirmed: the habit journeys
file (`tests/journeys/habit_family_journeys.rs`) covers skip/pause/
quota/history, not the create form's echo steps.

## 2. Decisions & fix shapes

### D-#1016: close the wholesale-failure class — panic-isolated driver job loop

Replace the bare job dispatch in `driver::run`'s worker loop with
`std::panic::catch_unwind(AssertUnwindSafe(job))`; on `Err`, ship the
payload back to the caller over the existing per-job completion
channel and `resume_unwind` **on the test's own thread**. Effects:
the offending test still panics and fails (same observable failure),
the worker thread survives, and sibling tests no longer cascade into
`SendError`/`RecvError`. Update the `# Panics` doc block accordingly
(panics move to the caller thread; the OnceLock option-mismatch assert
stays). Keep `#![forbid(unsafe_code)]` — `catch_unwind` is safe.

Lowest-layer test: a `#[cfg(test)]` self-test inside
`tests/support/driver.rs` (where the existing driver self-tests live):
(1) a job that panics fails the *calling* test thread with that panic
(assert via `catch_unwind` around `driver::run` in the test, typed on
payload presence — no message text); (2) a subsequent `driver::run`
job on the same process still completes (the worker survived). This is
the AGENTS §5 shape: typed outcome, one behavior, lowest layer.

Issue hygiene: comment on #1016 with the measured green evidence
(§1) and the R1 fix; recommend closing once the panic-isolation PR
lands (the issue's remaining harm is gone). R2 → separate backlog
issue, not this chunk.

### D-#1216: no code — verified-resolved comment

Recorded reality (§1) goes into a comment on the issue: docs rule on
main (`docs/testing_strategy.org:211`), trait comment on main
(`driver.rs:85-95`), option (a) stays the tracker per the issue's own
body. Nothing else.

### D-#1219: chronological multi-level undo ledger in FakeEngine

Replace the five parallel stacks with one ledger:

```rust
enum UndoFace {
    RemovedRule(taskboard_domain::ReminderRule),
    AddedRule(taskboard_domain::ReminderRuleId),
    RestoredEntity(RestoredEntity),
    ClearedBatch(Vec<taskboard_domain::ReminderRule>),
    ReorderedSections(Vec<(taskboard_domain::HabitId, i64)>),
}
ledger: Vec<UndoFace>,   // chronological; every continuity-bearing
                         // mutating command pushes exactly one face
```

Each mutating command that today pushes to a per-face stack pushes its
`UndoFace` instead; `StateCommand::Undo` pops **exactly one** face (the
newest) and applies its inverse. Repeated `Undo` commands walk back
multiple levels across families — the multi-level model #1219 asks
for. Keep the module CONTRACT comment ("fixture continuity, not an
oracle — the state engine's fold suite owns undo semantics")
verbatim; the fake is still not an oracle.

Lowest-layer tests: a `#[cfg(test)] mod tests` inside
`tests/support/fake_engine.rs` (the module compiles per binary, so the
tests ride along wherever the fake is used — one behavior, one test,
lowest layer): (a) interleave `RemoveReminderRule` → `AddReminderRule`
→ `Undo` → `Undo` and assert the typed terminal state (the added rule
gone, the removed row back) — the old code popped both families in one
pass, so this exact sequence is the regression pin; (b) a third `Undo`
past the empty ledger is a typed no-op (state unchanged); (c) one
`Undo` after `ClearReminderRules` of n rules restores all n (W-7 face,
now as a single popped batch). Any existing journey that depended on
the accidental multi-family pop must be re-examined, not weakened:
`undo_family_restore_clear_surfaces_journeys.rs:650,707-709` presses
single `AppUndo` steps — they assert one `Undo` command and one face's
effect, which the ledger preserves.

### D-#1259: prove the TrackEntries overlay's keyboard walk, then land row 12

Two-stage shape, in order:

1. **Overlay stop inventory**: give the `TrackEntries` overlay the
   mask/walk idiom the r8 chunk installed on the detail — the overlay's
   controls (including the "Set a timer" opener(s), the timer-entry
   buttons) become named, ordered keyboard stops reachable by the walk
   strokes, with a panel-scope focus grab that survives the headless
   pass. Where this needs view code (the walk's stop inventory lives in
   the view, per the detail precedent:
   `2026-11-22-task-edit-view-focus-plan.md` §5 step 3), that is
   production code in `taskboard-ui-slint` — the presentation client
   role is unchanged (focus ordering is presentation).
2. **The row-12 journey** in `tests/journeys/detail_focus_journeys.rs`
   (the inventory's home file): walk to the "Set a timer" opener inside
   the open entries overlay, press Return, assert the typed outcome —
   the timer setter (`TimerSetter`) flow opens (typed flow-state fact)
   and focus rests on the timer-entry cell (`FocusFacts` kind
   "new (timer)", per the row's Restore column); the never-title
   half is the existing `anchor_kind` oracle (`:28`). Follow the file's
   `walk_to` (`:63`) idiom: poll `focused_label` against the named
   address with a deadline panic — never a sleep.

The journey's §6 Test cell in the origin plan gets its nomination (the
plan already declares row 12 in §4; this fills the missing
implementation, not a new row — the coverage gate needs no structural
change, only the landed `fn` the plan's §4/§6 already name).

### D-#770: two form-local echo journeys + retro bound-test lines

Two tests in `tests/journeys/habit_family_journeys.rs` (the habit
family's home; naming per the house law, step-token prefixed; the
r7 create plan is a retro-cohort plan, so `bound-test:` is its
mapping):

- `us1_s2_create_title_draft_echoes_and_commits_on_return`: arm the
  inline create form (US-1/S1 path), type the title draft, assert the
  field's typed echo (the field's text model carries the draft —
  element value, never label text), press Return, assert the draft
  committed into the armed form's draft state (typed field, no
  dispatch yet — `assert_commands` shows no `CreateHabit`).
- `us1_s3_create_target_pick_shows_via_selected_index`: pick the
  target `SegmentedControl` variant, assert the control's typed
  selected-index/variant fact (not label text), Escape disarms
  (the S3 alternative-flow leg: the draft is discarded — typed
  disarm fact).

Amend `.artifacts/plans/2026-11-13-r7-habit-create-ux-plan.md`'s W-1
row area with the two retro `bound-test:` lines
(`bound-test: US-1/S2 -> habit_family_journeys.rs::us1_s2_...`,
`bound-test: US-1/S3 -> habit_family_journeys.rs::us1_s3_...`); the
plan already declares both steps in §6's US-1 table, so the
`bound-test:` hard-parse rule (a line naming an undeclared step) is
not tripped. No dispatch is involved (form-local edits ride the armed
form), so no FakeEngine/assert_command surface changes.

## 3. Chunk split: TWO chunks (split by risk)

The five issues do **not** belong in one chunk. #1259 is the only item
that may touch production view code (the overlay's stop inventory);
#1016-residue, #1216, #1219, #770 are harness/test/plan-amendment
work with zero production-code risk. Splitting isolates the one chunk
whose failure mode is a view-behaviour regression.

| Chunk | Issues | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 | #1016 (residue R1 + issue comments), #1216 (comment), #1219 (ledger), #770 (2 tests + amendment) | `test/journeys-plain-cargo` (this branch) | `tests/support/driver.rs` panic seam + self-test; `tests/support/fake_engine.rs` ledger + self-tests; `habit_family_journeys.rs` +2 tests; r7 plan amendment; #1016/#1216 comments | #770: US-1/S2, US-1/S3 (retro bound-test); others `n/a — no surface delta` | S/M |
| 2 | #1259 | `test/track-entries-overlay-walk` (fresh, off fetched `origin/main`) | TrackEntries overlay keyboard stop inventory (view + walk idiom) + the row-12 journey in `detail_focus_journeys.rs` | #1259: the §4 row-12 journey (plan declares the row; lands the Test nomination) | M |

Dependency graph: chunks are order-independent and touch disjoint
files (chunk 1 never opens `detail_focus_journeys.rs` or any `src`
view file; chunk 2 never opens `driver.rs`/`fake_engine.rs`) — they
may run in parallel worktrees. Default order: chunk 1 first (its
#1016 comment cites measured reality that chunk 2's plan depends on:
plain-cargo green).

## 4. Invariants

- **No test weakened**: every existing journey and self-test keeps its
  assertion strength; the D-#1219 ledger may change *which* effects a
  journey's `Undo` observes only where the old multi-family pop was
  the accidental behavior — such a journey is re-derived from the
  ledger semantics or flagged in the final report, never weakened.
- **The nextest gate is unchanged or stricter**: no `[[test]]` target
  is removed or renamed; the two new self-test modules and the three
  journey tests add to the gate.
- **The plain-cargo invocation must not need env vars**: no new
  process-global env seams; the `TASKBOARD_JOURNEYS_STAY_ON_TODAY`
  removal stays removal; any new tunables are constructor parameters
  on the fake/driver.
- **`#![forbid(unsafe_code)]` holds**; no new dependency.
- **Fixture continuity, not an oracle**: the FakeEngine CONTRACT
  comment survives the ledger refactor verbatim.
- **No text assertions, no sleeps** (AGENTS §5): typed outcomes only;
  `walk_to`-style deadline polling.

## 5. Tests to add

1. `driver.rs` self-test: panicking job fails the caller, worker
   survives a follow-up job (chunk 1).
2. `fake_engine.rs` self-tests: interleaved remove/add two-level undo;
   third undo is a no-op; clear-batch single-undo restores n (chunk 1).
3. `habit_family_journeys.rs`: `us1_s2_create_title_draft_echoes_and_commits_on_return`,
   `us1_s3_create_target_pick_shows_via_selected_index` (chunk 1).
4. `detail_focus_journeys.rs`: the row-12 timer-entry journey
   (chunk 2), per D-#1259's typed-outcome spec.

## 6. Verification (per chunk; all cargo through the memory gate)

Chunk 1:

```sh
scripts/memory-gate.sh cargo test -p taskboard-ui-slint --test journeys
# expected: 447+ passed (444 + the 3 new journey tests), 0 failed
scripts/memory-gate.sh cargo test -p taskboard-ui-slint --lib
# expected: green — lib.rs's habit-create bound test untouched
scripts/run-quality-gates.sh cargo nextest run --workspace
# expected: green; the nextest gate unchanged-or-stricter
```

Chunk 2:

```sh
scripts/memory-gate.sh cargo test -p taskboard-ui-slint --test journey_suites
# expected: green, +1 (the row-12 journey rides detail_focus_journeys)
scripts/run-quality-gates.sh cargo nextest run --workspace
# expected: green
```

Both chunks hand off at "PR open + `gh pr checks` green"; the
dispatcher owns the full AGENTS §10 suite between landing and ship.

## 7. Diff-shape rules

- Chunk 1 touches exactly: `tests/support/driver.rs`,
  `tests/support/fake_engine.rs`,
  `tests/journeys/habit_family_journeys.rs`,
  `.artifacts/plans/2026-11-13-r7-habit-create-ux-plan.md` (amendment
  only — bound-test lines, no §4/§6 rewrite). No `src/` changes, no
  `Cargo.toml` changes.
- Chunk 2 touches exactly: the `TrackEntries` overlay's view file(s)
  and walk-inventory wiring under `crates/taskboard-ui-slint/src/`,
  `tests/journeys/detail_focus_journeys.rs`, and the
  `2026-11-22-task-edit-view-focus-plan.md` row-12 Test-cell
  nomination if the §4 row lacks one. No harness-seam changes.
- Comments state constraints, not narration; no `#[allow]` without a
  naming comment; one commit per TDD step, each standalone
  (`<type>(<module>): <short-desc>` per AGENTS §8).

## 8. Rulings requested (defaults applied)

1. **#1016 disposition** — (a, default) land the panic-isolation seam,
   comment with the stale-premise evidence, recommend close on
   merge; (b) close now without code. Proceeding with (a).
2. **#1216 disposition** — (a, default) no code, verified-resolved
   comment, tracker stays per the issue's own body; (b) implement the
   driver-level fallback now. Proceeding with (a).
3. **Chunk split** — (a, default) two chunks split by risk (§3);
   (b) one chunk. Proceeding with (a).
4. **#1259 scope boundary** — (a, default) the overlay stop inventory
   may touch view code but stops at presentation (focus ordering);
   any engine/adapter ripple halts the chunk and reports; (b)
   view-code-free attempt only (walk-only, accept if unreachable).
   Proceeding with (a).
5. **journeys.rs monolith (24.7k lines)** — out of scope, filed as a
   separate `backlog:` issue at stage 1 (§1 R2).

---

## Chunk 1 — stage 2 (IMPLEMENTATION) prompt (paste verbatim)

You are the STAGE 2 (IMPLEMENTATION) agent for chunk 1 of the
"test harness: plain cargo, key events, fake engine" work, planned at
`.artifacts/plans/2026-11-28-test-harness-journeys-plan.md` (read it
first; §1's recorded reality and §2's D-#1016/D-#1216/D-#1219/D-#770
decisions are normative, as are AGENTS.md §5 and §7 and
`docs/testing_strategy.org`). Repo: D4id4los/taskboard.

**Discipline (binding — docs/agent_dispatch.org; restated verbatim):**
A subagent is killed after ~600s without observable activity. **Long-command discipline**: (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the real-home sandbox guard is landed: test helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines.**

**Environment**: worktree
`/home/viktoria/src/taskboard-WT-test-journeys-plain-cargo`, branch
`test/journeys-plain-cargo` (already exists, off `origin/main`
cc25ec7a). Work ONLY inside it, by absolute path; never touch the main
checkout or any other `taskboard-WT-*` directory. Create a feature
branch `test/journeys-plain-cargo` is already yours — commit directly
to it.

**Deliverables, in TDD order (one standalone commit each):**

1. `test(ui): the driver job loop survives a panicking job` — first
   write the failing self-test in the `#[cfg(test)]` module of
   `crates/taskboard-ui-slint/tests/support/driver.rs` (where the
   driver self-tests live): job A panics; assert the panic surfaces on
   the *calling* thread (wrap the `driver::run` call in
   `std::panic::catch_unwind` in the test; assert the payload is a
   panic payload — typed fact, no message text); then job B on the same
   process completes successfully (the worker survived). Then fix
   `driver::run` (`tests/support/driver.rs:506`): wrap each job in
   `std::panic::catch_unwind(AssertUnwindSafe(..))` inside the worker
   loop, ship the payload to the caller over the existing completion
   channel, and `resume_unwind` on the caller thread. Update the
   `# Panics` doc block; the OnceLock option-mismatch assert stays.
   `#![forbid(unsafe_code)]` holds.
2. `test(ui): the fake engine's undo ledger is chronological and
   multi-level` — in `crates/taskboard-ui-slint/tests/support/fake_engine.rs`:
   first the failing `#[cfg(test)]` self-tests per plan §5 item 2
   (interleaved remove/add two-level undo; third undo is a typed
   no-op; clear-batch single-undo restores all n); then replace the
   five parallel stacks (`removed_rules` `:32`, `added_rules` `:37`,
   `restored_entities` `:41`, `cleared_batches` `:44`,
   `habit_sections` `:51`) with the chronological `Vec<UndoFace>`
   ledger per plan §2 D-#1219. The CONTRACT comment ("fixture
   continuity, not an oracle") survives verbatim. Re-examine, never
   weaken, any journey whose observed `Undo` effects change — flag any
   such journey in your final report.
3. `test(ui): the habit-create form-local echo steps` — two journeys in
   `crates/taskboard-ui-slint/tests/journeys/habit_family_journeys.rs`:
   `us1_s2_create_title_draft_echoes_and_commits_on_return` and
   `us1_s3_create_target_pick_shows_via_selected_index`, per plan §2
   D-#770 (typed field/selected-index facts; no dispatch on S2; Escape
   disarm on S3's leg). Follow the file's existing harness idioms;
   no sleeps, no text assertions.
4. `docs(plans): bind the habit-create echo journeys to the r7 plan` —
   add the two retro `bound-test:` lines to
   `.artifacts/plans/2026-11-13-r7-habit-create-ux-plan.md`'s W-1 area
   exactly as spelled in plan §2 D-#770. Do not rewrite §4/§6.
5. Issue comments (always permitted; no push needed): on #1016, post
   the measured stale-premise evidence (plan §1: journeys 444/444
   plain-cargo green, app_services 84/84 green, both at cc25ec7a) and
   note the panic-isolation fix on this branch; on #1216, post the
   verified-resolved record (docs rule `docs/testing_strategy.org:211`,
   trait comment `driver.rs:85-95`, tracker stays per the issue body).
   Comment bodies summarize; deep context stays in the plan file.

**Diff-shape rules (binding)**: chunk 1 touches exactly
`tests/support/driver.rs`, `tests/support/fake_engine.rs`,
`tests/journeys/habit_family_journeys.rs`, and the r7 plan amendment —
no `src/` changes, no `Cargo.toml` changes, no `[[test]]` target
removed or renamed. Invariants: no test weakened; the nextest gate
unchanged or stricter; the plain-cargo invocation needs no env vars.

**Gate**: pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(background + poll). Targeted verifications with expected outcomes
(plan §6): `scripts/memory-gate.sh cargo test -p taskboard-ui-slint
--test journeys` → 447+ passed, 0 failed;
`scripts/memory-gate.sh cargo test -p taskboard-ui-slint --lib` →
green. If the gate lock is held when no useful work remains, commit
all landed work, report "gate deferred: lock held by <holder>", and
end the stage.

**Tests that must not have been weakened**: every pre-existing journey
in `--test journeys` (444 at cc25ec7a) and `--test journey_suites`,
the lib.rs habit-create bound test
(`habit_create_toggles_and_escape_disarms_without_shadowing`), and the
existing driver self-tests.

Report: commit SHAs, test counts before/after, any journey re-derived
from the ledger semantics, issue-comment links, gate status.

---

## Chunk 1 — stage 3 (REVIEW) prompt (paste verbatim)

You are the STAGE 3 (REVIEW) agent for chunk 1 of the "test harness:
plain cargo, key events, fake engine" work, planned at
`.artifacts/plans/2026-11-28-test-harness-journeys-plan.md` (§2's
D-#1016/D-#1216/D-#1219/D-#770 and §4's invariants are your review
norms; AGENTS.md §5 is the test law). Repo: D4id4los/taskboard.

**Discipline (binding — docs/agent_dispatch.org; restated verbatim):**
A subagent is killed after ~600s without observable activity. **Long-command discipline**: (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the real-home sandbox guard is landed: test helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines.**

**Environment**: worktree
`/home/viktoria/src/taskboard-WT-test-journeys-plain-cargo`, branch
`test/journeys-plain-cargo`, with stage 2's commits landed. Work ONLY
inside it, by absolute path.

**Review scope**: verify stage 2's landed state against the plan — the
panic-isolated driver job loop (caller-thread resume, worker
survival), the chronological `UndoFace` ledger with the CONTRACT
comment intact, the two habit-create echo journeys, the r7 plan
bound-test amendment, the #1016/#1216 comments. Verify the
diff-shape rules (plan §7 chunk-1 list) hold — no `src/` changes, no
`Cargo.toml` changes, no target removed. Verify the pre-existing 444
journeys + the lib.rs habit-create bound test + the driver self-tests
have not been weakened (diff the assertions, not just the pass count).

**Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client; (c) a single level of abstraction per function body. Findings are fixed on the branch.

**Verification**: `scripts/memory-gate.sh cargo test -p
taskboard-ui-slint --test journeys` → 447+ passed, 0 failed;
`scripts/memory-gate.sh cargo test -p taskboard-ui-slint --lib` →
green; `scripts/run-quality-gates.sh cargo nextest run --workspace`
(background + poll; if contended with no useful work left, report
"gate deferred: lock held by <holder>" — the dispatcher owns it).

**Handoff**: push the branch and open ONE PR to `main`
(`test(ui): the journey harness residue — panic-isolated driver, undo ledger, habit-create echoes (#1016, #1219, #770, #1216)`-style
title; body cites the plan path and the issue numbers), report the PR
URL — and stop. Do not watch checks, do not merge, do not remove the
worktree.

---

## Chunk 2 — stage 2 (IMPLEMENTATION) prompt (paste verbatim)

You are the STAGE 2 (IMPLEMENTATION) agent for chunk 2 of the
"test harness: plain cargo, key events, fake engine" work, planned at
`.artifacts/plans/2026-11-28-test-harness-journeys-plan.md` (§2's
D-#1259 and §4's invariants are normative; the origin inventory is
`.artifacts/plans/2026-11-22-task-edit-view-focus-plan.md` §4 row 12;
AGENTS.md §5 and `docs/testing_strategy.org` are the law — including
the "journey key routing" rule at §Tier 2). Repo: D4id4los/taskboard.

**Discipline (binding — docs/agent_dispatch.org; restated verbatim):**
A subagent is killed after ~600s without observable activity. **Long-command discipline**: (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the real-home sandbox guard is landed: test helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines.**

**Environment**: create the worktree from the MAIN checkout
(`/home/viktoria/src/taskboard` — never modify it otherwise) with
`scripts/worktree-doctor.sh` first (nonzero exit = STOP and report),
then `git -C /home/viktoria/src/taskboard worktree add -b
test/track-entries-overlay-walk
../taskboard-WT-test-track-entries-overlay-walk origin/main` (flat
path suffix; never `<path> main`). Work ONLY inside that worktree, by
absolute path. If chunk 1's PR has landed, fetch `origin/main` fresh
before branching (AGENTS §8 never-reuse law).

**Deliverables, in TDD order (one standalone commit each):**

1. `test(ui): the entries overlay's keyboard walk is unproven — the
   failing inventory pin` — the row-12 journey first, red: in
   `crates/taskboard-ui-slint/tests/journeys/detail_focus_journeys.rs`,
   add the row-12 journey per plan §2 D-#1259: open the detail, walk
   onto "Time entries" (stop 11, existing), open the `TrackEntries`
   overlay, then walk toward the "Set a timer" opener expecting the
   walk to land there (`walk_to`'s deadline-polling idiom, `:63` — no
   sleeps). This is expected to fail exactly as the #1259 stage-3
   attempt recorded (`focused_label` returns `None` inside the
   overlay).
2. `fix(ui): the track-entries overlay carries a proven keyboard stop
   inventory` — give the `TrackEntries` overlay the mask/walk idiom
   the r8 chunk installed on the detail (the
   `2026-11-22-task-edit-view-focus-plan.md` §5 precedent): the
   overlay's controls become named, ordered keyboard stops (including
   the "Set a timer" opener and the timer-entry buttons) with a
   panel-scope focus grab that survives the headless pass. View code
   under `crates/taskboard-ui-slint/src/` only; presentation focus
   ordering — no engine/adapter changes. If the fix wants anything
   beyond presentation (engine, adapter, domain), STOP, commit what
   landed, and report — that is the plan §8 ruling 4 boundary.
3. The row-12 journey green: press Return on the "Set a timer" opener;
   assert the typed outcome — the timer setter (`TimerSetter`) flow
   opens (typed flow-state fact) and focus restores to the
   timer-entry cell (the row's Restore column; `anchor_kind`-style
   typed oracle, `detail_focus_journeys.rs:28`). Amend the origin
   plan's row-12 §6 Test cell with the new journey's nomination if
   the row lacks one (the plan already declares row 12 in §4).

**Diff-shape rules (binding)**: chunk 2 touches exactly the
`TrackEntries` overlay's view file(s) and walk-inventory wiring under
`crates/taskboard-ui-slint/src/`, plus
`tests/journeys/detail_focus_journeys.rs` and (if needed) the origin
plan's row-12 Test cell. No harness-seam changes
(`tests/support/*` untouched), no `Cargo.toml` changes. Invariants:
no test weakened — the existing detail inventory journeys (stops 1-15,
fallback legs, the chord-clamp tests) keep their assertions; the
nextest gate unchanged or stricter; no sleeps, no text assertions —
typed outcomes and deadline polling only.

**Gate**: pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(background + poll). Targeted verification with expected outcome:
`scripts/memory-gate.sh cargo test -p taskboard-ui-slint --test
journey_suites` → green, +1 test (the row-12 journey). If the gate
lock is held when no useful work remains, commit all landed work,
report "gate deferred: lock held by <holder>", and end the stage.

Report: commit SHAs, the overlay stop inventory (named stops in
order), test counts before/after, whether the ruling-4 boundary was
hit, gate status.

---

## Chunk 2 — stage 3 (REVIEW) prompt (paste verbatim)

You are the STAGE 3 (REVIEW) agent for chunk 2 of the "test harness:
plain cargo, key events, fake engine" work, planned at
`.artifacts/plans/2026-11-28-test-harness-journeys-plan.md` (§2's
D-#1259 and §4's invariants are your review norms; the origin
inventory row is `.artifacts/plans/2026-11-22-task-edit-view-focus-plan.md`
§4 row 12). Repo: D4id4los/taskboard.

**Discipline (binding — docs/agent_dispatch.org; restated verbatim):**
A subagent is killed after ~600s without observable activity. **Long-command discipline**: (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the real-home sandbox guard is landed: test helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines.**

**Environment**: worktree
`/home/viktoria/src/taskboard-WT-test-track-entries-overlay-walk`,
branch `test/track-entries-overlay-walk`, with stage 2's commits
landed. Work ONLY inside it, by absolute path.

**Review scope**: verify stage 2's landed state against plan §2
D-#1259 — the failing-first row-12 journey (check commit order: red
pin, then the view fix, then green), the overlay's stop inventory in
the view layer only (no engine/adapter changes — ruling 4), the
typed-outcome assertions (no text assertions, no sleeps), the origin
plan's row-12 Test nomination. Verify the diff-shape rules (plan §7
chunk-2 list) hold and that the existing detail inventory journeys
(stops 1-15, fallback legs, chord clamps) have not been weakened.

**Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client; (c) a single level of abstraction per function body. Findings are fixed on the branch.

**Verification**: `scripts/memory-gate.sh cargo test -p
taskboard-ui-slint --test journey_suites` → green, +1;
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(background + poll; if contended with no useful work left, report
"gate deferred: lock held by <holder>" — the dispatcher owns it).

**Handoff**: push the branch and open ONE PR to `main`
(title in the `test(ui)/fix(ui)` shape citing #1259 and the plan
path), report the PR URL — and stop. Do not watch checks, do not
merge, do not remove the worktree.
